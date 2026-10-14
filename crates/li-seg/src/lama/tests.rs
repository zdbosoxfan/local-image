use super::*;
fn fill() -> Lama {
    Lama {
        predict: Arc::new(|_, mask, check| {
            check()?;
            Ok(RgbImage::from_fn(SIDE, SIDE, |x, y| if mask.get_pixel(x, y)[0] > 0 { Rgb([20, 40, 60]) } else { Rgb([240, 240, 240]) }))
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
    let (_, hard, size, offset) = prepare(&rgb, &mask);
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
