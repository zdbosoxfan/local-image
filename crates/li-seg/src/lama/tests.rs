use super::*;
use image::Rgb;
fn fill() -> Lama {
    Lama {
        predict: Arc::new(|_, mask, check| {
            check()?;
            let v = |p: [u8; 3]| p.map(|c| c as f32 / 255.0);
            Ok(Frame::from_fn(
                SIDE as usize,
                SIDE as usize,
                |x, y| if mask.get_pixel(x as u32, y as u32)[0] > 0 { v([20, 40, 60]) } else { v([240, 240, 240]) },
            ))
        }),
    }
}
#[test]
fn lama_preserves_outside_and_keeps_every_disconnected_hole() {
    let rgb = RgbImage::from_pixel(6000, 80, Rgb([100, 120, 140]));
    let mask = GrayImage::from_fn(6000, 80, |x, y| Luma([if x % 180 == 90 && y == 40 { 255 } else { 0 }]));
    let rs = regions(&mask, &|| Ok(())).unwrap();
    assert!(rs.len() <= 32);
    let output = fill().run(&rgb, &mask, &|| Ok(()), &|_, _| {}).unwrap();
    for (i, (a, b)) in rgb.pixels().zip(output.pixels()).enumerate() {
        if mask.as_raw()[i] == 0 {
            assert_eq!(a, b);
        } else {
            assert_ne!(a, b, "hole {i} was lost");
        }
    }
}
#[test]
fn lama_thin_downsampled_mask_and_aspect_are_preserved() {
    let rgb = RgbImage::from_pixel(4096, 1024, Rgb([0, 0, 0]));
    let mask = GrayImage::from_fn(4096, 1024, |x, _| Luma([if x == 1025 { 1 } else { 0 }]));
    let (_, hard, size, offset) = prepare(&Frame::from_rgb8(&rgb), &mask);
    assert_eq!(size, [512, 128]);
    assert_eq!(offset, [0, 192]);
    assert_eq!(hard.get_pixel(128, 200)[0], 255);
}
#[test]
fn lama_rejects_bad_input_and_cancels_without_partial_output() {
    let rgb = RgbImage::from_pixel(600, 700, Rgb([100, 100, 100]));
    let mask = GrayImage::from_fn(600, 700, |x, y| Luma([if x == 300 && y == 300 { 255 } else { 0 }]));
    assert!(fill().run(&rgb, &GrayImage::new(1, 1), &|| Ok(()), &|_, _| {}).is_err());
    assert!(fill().run(&rgb, &GrayImage::new(600, 700), &|| Ok(()), &|_, _| {}).is_err());
    let calls = std::sync::atomic::AtomicUsize::new(0);
    let error = fill()
        .run(
            &rgb,
            &mask,
            &|| {
                if calls.fetch_add(1, std::sync::atomic::Ordering::Relaxed) >= 3 {
                    bail!("cancelled");
                }
                Ok(())
            },
            &|_, _| {},
        )
        .unwrap_err();
    assert_eq!(error.to_string(), "cancelled");
}
#[test]
fn lama_partial_coverage_blends_once_and_edges_work() {
    let rgb = RgbImage::from_pixel(1, 3, Rgb([100, 120, 140]));
    let mask = GrayImage::from_raw(1, 3, vec![255, 128, 0]).unwrap();
    let out = fill().run(&rgb, &mask, &|| Ok(()), &|_, _| {}).unwrap();
    assert_eq!(out.get_pixel(0, 0).0, [20, 40, 60]);
    assert_eq!(out.get_pixel(0, 1).0, [60, 80, 100]);
    assert_eq!(out.get_pixel(0, 2), rgb.get_pixel(0, 2));
}
#[test]
fn lama_readiness_requires_full_pinned_file_size() {
    let dir = std::env::temp_dir().join(format!("lama-readiness-{}", std::process::id()));
    let path = crate::model_path(&dir, &MODEL);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, b"partial download").unwrap();
    assert!(!installed(&dir));
    assert!(shared(&MODEL, &dir, &|| Ok(())).is_err());
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn lama_patch_keeps_soft_coverage_for_existing_compositor() {
    let rgb = RgbImage::from_pixel(1, 3, Rgb([100, 120, 140]));
    let mask = GrayImage::from_raw(1, 3, vec![255, 128, 0]).unwrap();
    let patch = fill().run_patch(&rgb, &mask, &|| Ok(()), &|_, _| {}).unwrap();
    assert_eq!(patch.get_pixel(0, 1).0, [20, 40, 60], "compositor applies soft coverage once");
    assert_eq!(patch.get_pixel(0, 2), rgb.get_pixel(0, 2));
}

/// The pre-J28 8-bit pipeline, verbatim apart from taking the fake model as a closure: the
/// golden reference for the 8-bit adapters and the "before" of the precision measurements.
mod pre_j28 {
    use super::*;
    use image::imageops;
    pub fn prepare(rgb: &RgbImage, mask: &GrayImage) -> (RgbImage, GrayImage, [u32; 2], [u32; 2]) {
        let (w, h) = rgb.dimensions();
        let k = (SIDE as f64 / w.max(h) as f64).min(1.0);
        let size = [(w as f64 * k).round().max(1.0) as u32, (h as f64 * k).round().max(1.0) as u32];
        let offset = [(SIDE - size[0]) / 2, (SIDE - size[1]) / 2];
        let resized = if size == [w, h] { rgb.clone() } else { imageops::resize(rgb, size[0], size[1], imageops::FilterType::Lanczos3) };
        let small = GrayImage::from_fn(size[0], size[1], |x, y| {
            let xa = x as u64 * w as u64 / size[0] as u64;
            let xb = ((x + 1) as u64 * w as u64).div_ceil(size[0] as u64);
            let ya = y as u64 * h as u64 / size[1] as u64;
            let yb = ((y + 1) as u64 * h as u64).div_ceil(size[1] as u64);
            let hit = (ya as u32..yb as u32).any(|yy| (xa as u32..xb as u32).any(|xx| mask.get_pixel(xx, yy)[0] > 0));
            Luma([if hit { 255 } else { 0 }])
        });
        let input = RgbImage::from_fn(SIDE, SIDE, |x, y| {
            *resized.get_pixel(reflect(x as i64 - offset[0] as i64, size[0]), reflect(y as i64 - offset[1] as i64, size[1]))
        });
        let hard = GrayImage::from_fn(SIDE, SIDE, |x, y| {
            *small.get_pixel(reflect(x as i64 - offset[0] as i64, size[0]), reflect(y as i64 - offset[1] as i64, size[1]))
        });
        (input, hard, size, offset)
    }
    pub fn run(image: &RgbImage, mask: &GrayImage, blend: bool, predict: &dyn Fn(&RgbImage, &GrayImage) -> RgbImage) -> RgbImage {
        let regions = regions(mask, &|| Ok(())).unwrap();
        let mut out = image.clone();
        for region in &regions {
            let [x0, y0, x1, y1] = context(*region, image.width(), image.height());
            let (w, h) = (x1 - x0, y1 - y0);
            let rgb = imageops::crop_imm(&out, x0, y0, w, h).to_image();
            let local = GrayImage::from_fn(w, h, |x, y| {
                let (xx, yy) = (x + x0, y + y0);
                Luma([if xx >= region[0] && xx < region[2] && yy >= region[1] && yy < region[3] { mask.get_pixel(xx, yy)[0] } else { 0 }])
            });
            let (input, hard, size, offset) = prepare(&rgb, &local);
            let predicted = predict(&input, &hard);
            let repaired = imageops::crop_imm(&predicted, offset[0], offset[1], size[0], size[1]).to_image();
            let repaired = if repaired.dimensions() == (w, h) { repaired } else { imageops::resize(&repaired, w, h, imageops::FilterType::Lanczos3) };
            for y in region[1]..region[3] {
                for x in region[0]..region[2] {
                    let coverage = mask.get_pixel(x, y)[0];
                    let alpha = if !blend && coverage > 0 { 255 } else { coverage as u32 };
                    if alpha > 0 {
                        let src = image.get_pixel(x, y);
                        let fill = repaired.get_pixel(x - x0, y - y0);
                        out.put_pixel(x, y, Rgb([0, 1, 2].map(|c| ((src[c] as u32 * (255 - alpha) + fill[c] as u32 * alpha + 127) / 255) as u8)));
                    }
                }
            }
        }
        out
    }
}

/// Fake model: holes become a smooth function of position, known pixels pass through.
fn smooth_model(x: usize, y: usize) -> [f32; 3] {
    [x as f32 / 511.0, y as f32 / 511.0, 0.25 + 0.5 * ((x + y) as f32 / 1022.0)]
}
fn float_model() -> Lama {
    Lama {
        predict: Arc::new(|rgb, mask, _| {
            Ok(Frame::from_fn(SIDE as usize, SIDE as usize, |x, y| if mask.get_pixel(x as u32, y as u32)[0] > 0 { smooth_model(x, y) } else { rgb.get(x, y) }))
        }),
    }
}
fn u8_model(rgb: &RgbImage, mask: &GrayImage) -> RgbImage {
    // The pre-J28 model wrapper: the same float output, rounded to 8 bits.
    RgbImage::from_fn(SIDE, SIDE, |x, y| {
        if mask.get_pixel(x, y)[0] > 0 {
            Rgb(smooth_model(x as usize, y as usize).map(|v| (v * 255.0).round().clamp(0.0, 255.0) as u8))
        } else {
            *rgb.get_pixel(x, y)
        }
    })
}
fn soft_mask(w: u32, h: u32, cx: f32, cy: f32, r: f32) -> GrayImage {
    GrayImage::from_fn(w, h, |x, y| {
        let d = ((x as f32 - cx).powi(2) + (y as f32 - cy).powi(2)).sqrt();
        Luma([((r - d) / 6.0 * 255.0).clamp(0.0, 255.0) as u8])
    })
}

#[test]
fn j28_lama_u8_adapters_match_the_pre_j28_path() {
    let none = &|| Ok(());
    // Context crop at model size: no resampling, so the 8-bit adapters are byte-identical.
    let rgb = RgbImage::from_fn(300, 200, |x, y| Rgb([(x % 256) as u8, (y * 7 % 256) as u8, ((x ^ y) % 256) as u8]));
    let mask = soft_mask(300, 200, 150.0, 90.0, 40.0);
    for blend in [true, false] {
        let old = pre_j28::run(&rgb, &mask, blend, &u8_model);
        let new = if blend { float_model().run(&rgb, &mask, none, &|_, _| {}) } else { float_model().run_patch(&rgb, &mask, none, &|_, _| {}) }.unwrap();
        let diff = old.as_raw().iter().zip(new.as_raw()).map(|(a, b)| a.abs_diff(*b)).max().unwrap();
        // Soft coverage now blends before the single rounding (previously: round, blend, round).
        assert!(diff <= 1, "blend={blend}: max 8-bit difference {diff}");
        if !blend {
            assert_eq!(old, new, "patch pixels are byte-identical without resampling");
        }
    }
    // Large hole: the crop is resampled to the model and back. The kernels are now the shared
    // float Lanczos (li-raster) instead of image-rs on 8-bit samples.
    let rgb = RgbImage::from_fn(1400, 1000, |x, y| Rgb([(x / 6 % 256) as u8, (y / 4 % 256) as u8, 128]));
    let mask = soft_mask(1400, 1000, 700.0, 500.0, 300.0);
    let old = pre_j28::run(&rgb, &mask, false, &u8_model);
    let new = float_model().run_patch(&rgb, &mask, none, &|_, _| {}).unwrap();
    let (mut max, mut sum, mut n) = (0u8, 0u64, 0u64);
    for ((a, b), m) in old.pixels().zip(new.pixels()).zip(mask.pixels()) {
        if m[0] == 0 {
            assert_eq!(a, b, "outside coverage stays byte-identical");
            continue;
        }
        for c in 0..3 {
            let d = a[c].abs_diff(b[c]);
            max = max.max(d);
            sum += d as u64;
            n += 1;
        }
    }
    eprintln!("j28 LaMa resampled patch vs pre-J28: max {max} levels, mean {:.4} levels", sum as f64 / n as f64);
    assert!(max <= 2, "resampled patch within 2 levels of the previous path ({max})");
}

#[test]
fn j28_lama_float_patch_has_no_8bit_step() {
    let none = &|| Ok(());
    // A deep sky-like gradient (about six 8-bit levels across 400 px) under a soft hole.
    let truth = |x: usize, y: usize| -> [f32; 3] { [0.40 + 0.02 * x as f32 / 399.0, 0.55 + 0.01 * y as f32 / 299.0, 0.7] };
    // Identity model: the ideal patch equals the gradient, so any error is precision loss.
    let identity = Lama { predict: Arc::new(|rgb, _, _| Ok(rgb.clone())) };
    let identity_u8 = |rgb: &RgbImage, _: &GrayImage| rgb.clone();
    for (w, h, r) in [(400usize, 300usize, 60.0f32), (1400, 1050, 330.0)] {
        let frame = Frame::from_fn(w, h, |x, y| truth(x * 399 / (w - 1), y * 299 / (h - 1)));
        let mask = soft_mask(w as u32, h as u32, w as f32 / 2.0, h as f32 / 2.0, r);
        let new = identity.run_patch_frame(&frame, &mask, none, &|_, _| {}).unwrap();
        let old = pre_j28::run(&frame.to_rgb8(), &mask, false, &identity_u8);
        let (mut e_new, mut e_old) = (0f32, 0f32);
        let mut levels_new = std::collections::BTreeSet::new();
        let mut levels_old = std::collections::BTreeSet::new();
        for (i, m) in mask.pixels().enumerate() {
            if m[0] == 0 {
                continue;
            }
            let (x, y) = (i % w, i / w);
            let t = frame.get(x, y);
            let (a, b) = (new.get(x, y), old.get_pixel(x as u32, y as u32));
            for c in 0..3 {
                e_new = e_new.max((a[c] - t[c]).abs());
                e_old = e_old.max((b[c] as f32 / 255.0 - t[c]).abs());
            }
            levels_new.insert((a[0] * 65535.0).round() as u32);
            levels_old.insert(b[0]);
        }
        eprintln!(
            "j28 LaMa {w}x{h}: max error in 16-bit levels before {:.1}, after {:.2}; distinct red levels in the patch before {}, after {} (16-bit)",
            e_old * 65535.0,
            e_new * 65535.0,
            levels_old.len(),
            levels_new.len()
        );
        assert!(e_new < e_old / 4.0, "float patch must beat the 8-bit path ({e_new} vs {e_old})");
        assert!(levels_new.len() > levels_old.len() * 10);
    }
}
