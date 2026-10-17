use photocraft_algo::lens::{EdgeMode, LensCorrection, LensMap, LensProfile, Sample, auto_scale, correct, generic_profile, remap, straighten_angle};
use photocraft_color::{ColorMode, PixelFormat, SampleType};
use photocraft_geom::Rect;
use photocraft_raster::Surface;

fn solid_surface(fmt: PixelFormat, w: i32, h: i32, rgba: [f32; 4]) -> Surface {
    let mut s = Surface::new(fmt);
    let px = photocraft_raster::from_rgba(&fmt, rgba);
    s.fill_rect(Rect::new(0, 0, w, h), &px);
    s
}

#[test]
fn generic_profile_clamps_at_table_ends() {
    let low = generic_profile(5.0);
    let low_table = generic_profile(12.0);
    assert!((low.k1 - low_table.k1).abs() < 1e-12);
    assert!((low.vignette[0] - low_table.vignette[0]).abs() < 1e-12);

    let high = generic_profile(300.0);
    let high_table = generic_profile(200.0);
    assert!((high.k1 - high_table.k1).abs() < 1e-12);
}

#[test]
fn generic_profile_interpolates_monotonically() {
    let p24 = generic_profile(24.0);
    let p30 = generic_profile(30.0);
    let p35 = generic_profile(35.0);
    assert!(p30.k1 > p24.k1, "24→30 should become less negative");
    assert!(p30.k1 < p35.k1, "30→35 should become further less negative");
}

#[test]
fn generic_profile_50mm_is_neutral_distortion() {
    let m = generic_profile(50.0);
    assert!((m.k1).abs() < 1e-9);
    assert!((m.k2).abs() < 1e-9);
    assert!(m.vignette[0] < 0.0, "typical vignetting should be present");
    assert!((m.ca_red).abs() < 1e-9 && (m.ca_blue).abs() < 1e-9);
}

#[test]
fn sample_center_is_unaffected_by_most_parameters() {
    let frame = Rect::new(0, 0, 200, 100);
    let lc = LensCorrection {
        profile: Some(LensProfile { k1: 0.1, k2: -0.05, p1: 0.02, p2: -0.01, ..Default::default() }),
        correct_distortion: true,
        distortion: 40.0,
        angle: 15.0,
        vertical: -20.0,
        horizontal: 10.0,
        scale: 80.0,
        ..Default::default()
    };
    let m = LensMap::new(&lc, frame);
    let s = m.sample(100.0, 50.0).unwrap();
    assert!((s.p[0] - 100.0).abs() < 1e-9, "center x moved: {}", s.p[0]);
    assert!((s.p[1] - 50.0).abs() < 1e-9, "center y moved: {}", s.p[1]);
}

#[test]
fn negative_distortion_samples_corners_outside_frame() {
    let frame = Rect::new(0, 0, 200, 100);
    let m = LensMap::new(&LensCorrection { distortion: -60.0, ..Default::default() }, frame);
    let s = m.sample(0.0, 0.0).unwrap();
    assert!(s.p[0] < 0.0, "corner x should be negative: {}", s.p[0]);
    assert!(s.p[1] < 0.0, "corner y should be negative: {}", s.p[1]);
}

#[test]
fn chromatic_aberration_shifts_red_blue_planes() {
    let frame = Rect::new(0, 0, 200, 100);
    let m = LensMap::new(&LensCorrection { red_cyan: -100.0, blue_yellow: 100.0, ..Default::default() }, frame);
    let s = m.sample(10.0, 10.0).unwrap();
    assert!(s.pr[0] < s.p[0], "red should sample further left: pr={:?} p={:?}", s.pr, s.p);
    assert!(s.pb[0] > s.p[0], "blue should sample further right: pb={:?} p={:?}", s.pb, s.p);
}

#[test]
fn straighten_angle_handles_horizontal_vertical_and_tilt() {
    assert!((straighten_angle([0.0, 0.0], [100.0, 0.0])).abs() < 1e-9);
    assert!((straighten_angle([0.0, 0.0], [0.0, 100.0])).abs() < 1e-9);

    let angle_deg: f64 = 5.0;
    let tilted = [100.0, 100.0 * angle_deg.to_radians().tan()];
    assert!((straighten_angle([0.0, 0.0], tilted) + angle_deg).abs() < 1e-9, "straighten should return -tilt");

    let near_vertical = straighten_angle([0.0, 0.0], [3.0, 100.0]);
    assert!((near_vertical - 1.718).abs() < 0.01, "near vertical got {near_vertical}");
}

#[test]
fn auto_scale_default_is_100() {
    let frame = Rect::new(0, 0, 200, 100);
    let s = auto_scale(&LensCorrection::default(), frame);
    assert!((s - 100.0).abs() < 1e-9, "auto scale for identity: {s}");
}

#[test]
fn edge_extension_repeats_edge_pixel_outside() {
    let fmt = PixelFormat::new(ColorMode::Rgb, SampleType::U8, false);
    let mut src = solid_surface(fmt, 64, 32, [0.1, 0.2, 0.3, 1.0]);
    // Make left edge white, others dark, so extension is obvious.
    let white = photocraft_raster::from_rgba(&fmt, [1.0, 1.0, 1.0, 1.0]);
    src.fill_rect(Rect::new(0, 0, 1, 32), &white);

    let lc = LensCorrection { distortion: -80.0, edge: EdgeMode::Extension, ..Default::default() };
    let frame = Rect::new(0, 0, 64, 32);
    let out = correct(&src, frame, &lc);
    assert!(!out.format().alpha, "Extension should not add alpha channel");
    let p = out.pixel(0, 0);
    assert!((p[0] - 1.0).abs() < 0.01 && (p[1] - 1.0).abs() < 0.01 && (p[2] - 1.0).abs() < 0.01, "corner should repeat white edge: {p:?}");
}

#[test]
fn nan_parameters_do_not_panic() {
    let lc = LensCorrection { distortion: f64::NAN, angle: f64::NAN, scale: f64::NAN, vertical: f64::NAN, horizontal: f64::NAN, ..Default::default() };
    let frame = Rect::new(0, 0, 64, 64);
    let src = solid_surface(PixelFormat::new(ColorMode::Rgb, SampleType::F32, true), 64, 64, [0.5, 0.5, 0.5, 1.0]);
    let out = correct(&src, frame, &lc);
    assert!(out.format().alpha);

    let map = LensMap::new(&lc, frame);
    let _ = map.sample(10.0, 10.0);
}

#[test]
fn infinite_coordinates_sample_do_not_panic() {
    let lc = LensCorrection::default();
    let m = LensMap::new(&lc, Rect::new(0, 0, 100, 100));
    let _ = m.sample(f64::INFINITY, 0.0);
    let _ = m.sample(0.0, f64::INFINITY);
    let _ = m.sample(f64::NEG_INFINITY, f64::NEG_INFINITY);
}

#[test]
fn remap_empty_area_returns_surface_without_panic() {
    let fmt = PixelFormat::new(ColorMode::Rgb, SampleType::U8, true);
    let src = solid_surface(fmt, 10, 10, [1.0, 0.0, 0.0, 1.0]);
    let empty = Rect::new(5, 5, 5, 5);
    let out = remap(&src, Rect::new(0, 0, 10, 10), empty, EdgeMode::Transparency, &|x, y| Some(Sample { p: [x, y], pr: [x, y], pb: [x, y], gain: 1.0 }));
    assert_eq!(out.format(), fmt);
}

#[test]
fn remap_with_map_returning_none_fills_edge_color() {
    let fmt = PixelFormat::new(ColorMode::Rgb, SampleType::U8, false);
    let src = solid_surface(fmt, 16, 16, [0.2, 0.8, 0.4, 1.0]);
    let out = remap(
        &src,
        Rect::new(0, 0, 16, 16),
        Rect::new(0, 0, 16, 16),
        EdgeMode::Color([1.0, 0.0, 0.0, 1.0]),
        &|_, _| None, // every output pixel is outside
    );
    let p = out.pixel(8, 8);
    assert!(p[0] > 0.99 && p[1] < 0.01 && p[2] < 0.01, "all pixels should receive edge colour: {p:?}");
}

#[test]
fn correct_output_is_finite_for_normal_inputs() {
    let fmt = PixelFormat::new(ColorMode::Rgb, SampleType::F32, true);
    let mut src = Surface::new(fmt);
    let w = 20;
    let h = 20;
    let mut row = vec![0.0f32; w as usize * 4];
    for y in 0..h {
        for x in 0..w {
            let rgba = [((x + y) % 5) as f32 / 5.0, (x as f32 * y as f32) / 400.0, 0.5, 1.0];
            let px = photocraft_raster::from_rgba(&fmt, rgba);
            row[x as usize * 4..(x as usize + 1) * 4].copy_from_slice(&px);
        }
        src.write_region(Rect::new(0, y, w, y + 1), &row);
    }

    let lc = LensCorrection { distortion: 30.0, red_cyan: -20.0, vignette_amount: 40.0, angle: 3.0, vertical: -10.0, ..Default::default() };
    let out = correct(&src, Rect::new(0, 0, w, h), &lc);
    let data = out.read_region(Rect::new(0, 0, w, h));
    for v in &data {
        assert!(v.is_finite(), "output contains non-finite value: {v}");
    }
}
