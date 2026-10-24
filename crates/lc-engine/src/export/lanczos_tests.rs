use super::*;

fn pattern(sw: usize, sh: usize) -> Vec<f32> {
    (0..sw * sh)
        .flat_map(|i| {
            let (x, y) = (i % sw, i / sw);
            [((x * 7 + y * 13) % 17) as f32 / 16.0, x as f32 / sw as f32, if (x / 5 + y / 3) % 2 == 0 { 0.9 } else { 0.1 }]
        })
        .collect()
}

fn rendered(sw: usize, sh: usize, samples: DeepSamples) -> lightcraft_pipeline::Rendered {
    lightcraft_pipeline::Rendered {
        image: Rgba8::new(sw, sh),
        histogram: li_raster::Histogram::of_srgb8(&Rgba8::new(1, 1)),
        deep: Some(DeepImage { width: sw, height: sh, space: OutputSpace::Srgb, samples }),
    }
}

/// Parity gate: Library export with a kernel gives the editor's Image Size result (same
/// `li-raster` taps, same edge rule, same Preserve Details sharpening) for every choice.
#[test]
fn library_kernel_resize_matches_editor_image_size() {
    use photocraft_doc::{ColorMode, PixelFormat, Rect, SampleType, Surface};
    let (sw, sh) = (61usize, 37usize);
    let src = pattern(sw, sh);
    let canvas = Rect::new(0, 0, sw as i32, sh as i32);
    let mut surface = Surface::new(PixelFormat::new(ColorMode::Rgb, SampleType::F32, false));
    surface.write_region(canvas, &src);
    for kernel in ResampleChoice::ALL {
        for (w, h) in [(23usize, 14usize), (150, 91)] {
            let mut r = rendered(sw, sh, DeepSamples::F32(src.clone()));
            resample_rendered(&mut r, kernel, (w, h), true);
            let deep = r.deep.expect("deep kept");
            assert_eq!((deep.width, deep.height, r.image.width, r.image.height), (w, h, w, h));
            let DeepSamples::F32(library) = &deep.samples else { panic!("f32 stays f32") };
            let (sx, sy) = (w as f64 / sw as f64, h as f64 / sh as f64);
            let editor = photocraft_algo::resample::resize_surface_in_canvas(&surface, sx, sy, kernel.resolve(w > sw), canvas)
                .read_region(Rect::new(0, 0, w as i32, h as i32));
            let max = library.iter().zip(&editor).map(|(a, b)| (a - b).abs()).fold(0.0f32, f32::max);
            assert!(max <= 1e-6, "{kernel:?} {w}x{h}: max |library - editor| = {max}");
        }
    }
}

/// 8-bit exports resize the 16-bit render (no 8-bit pre-quantisation) and drop `deep`; 16-bit
/// exports keep it.
#[test]
fn kernel_resize_depths() {
    let (sw, sh) = (40usize, 30usize);
    let u16s: Vec<u16> = (0..sw * sh * 3).map(|i| [13107u16, 32768, 52428][i % 3]).collect();
    let mut r = rendered(sw, sh, DeepSamples::U16(u16s.clone()));
    resample_rendered(&mut r, ResampleChoice::Lanczos, (17, 13), false);
    assert!(r.deep.is_none());
    assert_eq!((r.image.width, r.image.height), (17, 13));
    assert!(r.image.data.iter().all(|p| p[3] == 255 && p[0].abs_diff(51) <= 1 && p[1].abs_diff(128) <= 1 && p[2].abs_diff(204) <= 1));
    let mut r = rendered(sw, sh, DeepSamples::U16(u16s));
    resample_rendered(&mut r, ResampleChoice::BicubicAutomatic, (80, 60), true);
    let deep = r.deep.expect("16-bit kept");
    let DeepSamples::U16(v) = deep.samples else { panic!("u16 stays u16") };
    assert_eq!(v.len(), 80 * 60 * 3);
    assert!(v.as_chunks::<3>().0.iter().all(|c| c[0].abs_diff(13107) <= 1 && c[1].abs_diff(32768) <= 1 && c[2].abs_diff(52428) <= 1));
}

/// A 16-bit render in `space` of `linear(x, y)` (the same in every channel).
fn encoded16(sw: usize, sh: usize, space: OutputSpace, linear: impl Fn(usize, usize) -> f32) -> lightcraft_pipeline::Rendered {
    let trc = space.trc();
    let v = (0..sw * sh).flat_map(|i| [(trc.encode(linear(i % sw, i / sw)) * 65535.0).round() as u16; 3]).collect();
    lightcraft_pipeline::Rendered {
        deep: Some(DeepImage { width: sw, height: sh, space, samples: DeepSamples::U16(v) }),
        ..rendered(sw, sh, DeepSamples::F32(vec![]))
    }
}

/// The linear values of a resized 16-bit render's first channel.
fn linear16(r: &lightcraft_pipeline::Rendered) -> Vec<f32> {
    let deep = r.deep.as_ref().expect("16-bit kept");
    let DeepSamples::U16(v) = &deep.samples else { panic!("u16 stays u16") };
    v.iter().step_by(3).map(|s| deep.space.trc().decode(f32::from(*s) / 65535.0)).collect()
}

/// Reductions average light, not encoded values: fine bright-on-dark detail (lit window grids,
/// mullions) keeps its brightness, in every space's own curve.
#[test]
fn kernel_reductions_keep_the_light_of_fine_detail() {
    for space in [OutputSpace::Srgb, OutputSpace::AdobeRgb, OutputSpace::ProPhoto] {
        for kernel in [ResampleChoice::Lanczos, ResampleChoice::BicubicSharper, ResampleChoice::Bilinear] {
            let mut r = encoded16(64, 32, space, |x, _| if x % 2 == 0 { 0.2 } else { 0.8 });
            resample_rendered(&mut r, kernel, (32, 16), true);
            let v = linear16(&r);
            let mean = v.iter().sum::<f32>() / v.len() as f32;
            assert!((mean - 0.5).abs() < 0.02, "{space:?} {kernel:?}: mean light {mean} (0.5 in the source)");
        }
    }
}

/// A dark interior beside a bright window: the reduction rings no darker than the interior.
#[test]
fn kernel_reductions_add_no_dark_halos_beside_bright_windows() {
    for kernel in [ResampleChoice::Lanczos, ResampleChoice::BicubicSharper, ResampleChoice::BicubicAutomatic] {
        for (w, h) in [(48, 24), (24, 12)] {
            let mut r = encoded16(96, 48, OutputSpace::Srgb, |x, _| if x < 52 { 0.01 } else { 1.0 });
            resample_rendered(&mut r, kernel, (w, h), true);
            let darkest = linear16(&r).into_iter().fold(f32::INFINITY, f32::min);
            assert!(darkest > 0.0099, "{kernel:?} to {w}x{h}: halo down to {darkest}");
        }
    }
}

#[test]
fn resample_param_round_trips_and_defaults_off() {
    let d = ExportOptions::default();
    assert_eq!(d.resample, None);
    assert!(d.to_json().get("resample").is_none(), "default presets are unchanged");
    let o = ExportOptions::from_json(&serde_json::json!({"longEdge": 1024, "resample": "bicubicSharper"}));
    assert_eq!(o.resample, Some(ResampleChoice::BicubicSharper));
    assert_eq!(o.to_json()["resample"], "bicubicSharper");
    assert_eq!(ExportOptions::from_json(&o.to_json()).resample, Some(ResampleChoice::BicubicSharper));
    assert_eq!(ExportOptions::from_json(&serde_json::json!({"resample": "automatic"})).resample, Some(ResampleChoice::BicubicAutomatic));
    assert_eq!(ExportOptions::from_json(&serde_json::json!({"resample": "nonsense"})).resample, None);
}

/// End to end on a demo photo: a kernel export has the requested size and stays close to the
/// renderer's own scaling (same develop, different resampler).
#[test]
fn demo_export_with_kernel() {
    let mut session = crate::Session::with_demo();
    let id = session.catalog.photos().map(|p| p.id).next().expect("demo photo");
    let base = ExportOptions { format: ExportFormat::Png, resize: Some(Resize::long_edge(320)), ..Default::default() };
    export_photo(&mut session, id, &base, 1).unwrap(); // warm the decode cache
    let t = std::time::Instant::now();
    let a = export_photo(&mut session, id, &base, 1).unwrap();
    let renderer_ms = t.elapsed().as_secs_f64() * 1e3;
    let t = std::time::Instant::now();
    let b = export_photo(&mut session, id, &ExportOptions { resample: Some(ResampleChoice::Lanczos), ..base.clone() }, 1).unwrap();
    let kernel_ms = t.elapsed().as_secs_f64() * 1e3;
    assert_eq!((a.width, a.height), (b.width, b.height));
    let da = lightcraft_codecs::decode(&a.bytes, Default::default()).unwrap().to_srgb8();
    let db = lightcraft_codecs::decode(&b.bytes, Default::default()).unwrap().to_srgb8();
    let n = (da.data.len() * 3) as f64;
    let mae: f64 = da.data.iter().zip(&db.data).map(|(p, q)| (0..3).map(|c| f64::from(p[c].abs_diff(q[c]))).sum::<f64>()).sum::<f64>() / n;
    println!("demo {}x{} (warm): renderer {renderer_ms:.1} ms, lanczos {kernel_ms:.1} ms, mean |diff| {mae:.3} levels", a.width, a.height);
    assert!(mae < 6.0, "same photo, different resampler: {mae}");
}
