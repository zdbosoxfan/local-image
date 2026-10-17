//! Caller/worker CMYK boundaries, using explicit CMS transforms as the ink oracle.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use super::*;
use photocraft_cms::{Builtin, Intent, Profile, Transform};
use photocraft_color::{
    Color, SampleType,
    convert::{CmykSpace, active_cmyk_space, with_cmyk_space},
};

fn profile() -> Profile {
    photocraft_cms::synth::cmyk_profile(&photocraft_cms::synth::CmykParams {
        description: "Vector profile propagation control".into(),
        tvi: [0.3, 0.28, 0.26, 0.32],
        grid_a2b: 7,
        grid_b2a: 11,
        ..Default::default()
    })
}
fn transforms(profile: &Profile) -> (Transform, Transform) {
    (
        Transform::new(profile, Builtin::Srgb.profile(), Intent::RelativeColorimetric, true).unwrap(),
        Transform::new(Builtin::Srgb.profile(), profile, Intent::RelativeColorimetric, true).unwrap(),
    )
}
fn eval<const N: usize>(transform: &Transform, input: &[f32]) -> [f32; N] {
    let mut out = [0.; N];
    transform.eval(input, &mut out);
    out
}
fn display_rgb(rgb: [f32; 3]) -> [f32; 3] {
    rgb.map(|c| c.clamp(0.0, 1.0))
}
fn close_display(actual: &[f32], expected_ink: [f32; 4], a2b: &Transform, context: &str) -> [f32; 3] {
    // The existing two-code contract concerns displayed RGB,
    // not individual CMYK ink channels. Fast B2A LUT interpolation can change
    // the ink separation while remaining within that visible-color contract.
    // Both sides below use the independent canonical A2B pipeline. Keep the
    // same unaligned samples and the separate strict alpha check.
    let raw_actual = eval::<3>(a2b, &actual[..4]);
    let raw_expected = eval::<3>(a2b, &expected_ink);
    // The established display-code reference clamps at its final RGB boundary
    // (display_color_tests::reference), not in the extended ICC PCS domain.
    let actual_rgb = display_rgb(raw_actual);
    let expected_rgb = display_rgb(raw_expected);
    for c in 0..3 {
        assert!(
            (actual_rgb[c] - expected_rgb[c]).abs() <= 2. / 255.,
            "rendered RGB {actual_rgb:?} expected {expected_rgb:?}; raw RGB {raw_actual:?} expected {raw_expected:?}; inks {actual:?} expected {expected_ink:?}; {context}"
        );
    }
    assert!((actual[4] - 0.6).abs() < 0.00001);
    expected_rgb
}
#[test]
fn tagged_solid_and_gradient_paint_use_profile_across_multiple_tile_bands() {
    let profile = profile();
    let bytes = profile.to_bytes();
    let space = CmykSpace::for_profile(Some(&bytes)).unwrap();
    let (a2b, b2a) = transforms(&profile);
    let (default_a2b, default_b2a) = transforms(Builtin::CoatedCmyk.profile());
    let inks = [[0.7, 0.1, 0.2, 0.1], [0.1, 0.6, 0.3, 0.2]];
    let rgbs = [[0.25, 0.6, 0.3], [0.7, 0.25, 0.5]];
    assert!((0..3).any(|c| (eval::<3>(&a2b, &inks[0])[c] - eval::<3>(&default_a2b, &inks[0])[c]).abs() > 0.01));
    assert!((0..4).any(|c| (eval::<4>(&b2a, &rgbs[0])[c] - eval::<4>(&default_b2a, &rgbs[0])[c]).abs() > 0.01));
    let reference_rgb = display_rgb(eval::<3>(&a2b, &eval::<4>(&b2a, &rgbs[0])));
    let wrong_profile_rgb = display_rgb(eval::<3>(&a2b, &eval::<4>(&default_b2a, &rgbs[0])));
    assert!(a2b.fast_grid().is_some_and(|grid| grid.post.is_some()), "this reference models the existing bounded inverse-sRGB output table");
    assert!((0..3).any(|c| (reference_rgb[c] - wrong_profile_rgb[c]).abs() > 2. / 255.), "default B2A must fail the visible-color contract");
    #[cfg(not(target_arch = "wasm32"))]
    let serial = rayon::ThreadPoolBuilder::new().num_threads(1).build().unwrap();
    #[cfg(not(target_arch = "wasm32"))]
    let parallel = rayon::ThreadPoolBuilder::new().num_threads(2).build().unwrap();
    let rect = Rect::new(0, 0, 16, TILE_SIZE * 3);
    let format = PixelFormat::new(ColorMode::Cmyk, SampleType::F32, true);
    for cmyk in [false, true] {
        for gradient in [false, true] {
            let colors: [Color; 2] = std::array::from_fn(|i| {
                if cmyk {
                    Color { mode: ColorMode::Cmyk, c: inks[i], alpha: 0.6 }
                } else {
                    let mut color = Color::rgb(rgbs[i][0], rgbs[i][1], rgbs[i][2]);
                    color.alpha = 0.6;
                    color
                }
            });
            let fill = if gradient {
                Fill::Gradient {
                    stops: vec![(0., colors[0]), (1., colors[1])],
                    style: GradientStyle::Linear,
                    angle: 0.,
                    scale: 1.,
                    reverse: false,
                    opacity_stops: vec![],
                    midpoints: vec![],
                    offset: (0., 0.),
                    dither: false,
                    align: true,
                }
            } else {
                Fill::Solid(colors[0])
            };
            let shape = ShapeLayer { path: shapes::rect(0., 0., 16., f64::from(TILE_SIZE * 3)), fill: Some(fill), ..Default::default() };
            #[cfg(not(target_arch = "wasm32"))]
            let surface = parallel.install(|| with_cmyk_space(Some(&space), || render_shape(&shape, format, rect)));
            #[cfg(not(target_arch = "wasm32"))]
            let serial_surface = serial.install(|| with_cmyk_space(Some(&space), || render_shape(&shape, format, rect)));
            #[cfg(target_arch = "wasm32")]
            let surface = with_cmyk_space(Some(&space), || render_shape(&shape, format, rect));
            assert!(active_cmyk_space().is_none());
            let raw_rgb: [[f32; 3]; 2] = std::array::from_fn(|i| if cmyk { eval(&a2b, &inks[i]) } else { rgbs[i] });
            // CMYK Color::to_rgb uses the CMS inverse-sRGB post table, whose
            // input is bounded before encoding (Transform::Post::eval). Match
            // that normalized paint domain before interpolating the stops.
            // Retain extended canonical endpoints separately for diagnostics.
            let rgb = if cmyk { raw_rgb.map(display_rgb) } else { raw_rgb };
            for y in [1, TILE_SIZE + 1, TILE_SIZE * 2 + 1] {
                for x in [1, 8, 14] {
                    let t = if gradient { x as f32 / 16. } else { 0. };
                    let color: [f32; 3] = std::array::from_fn(|c| rgb[0][c] * (1. - t) + rgb[1][c] * t);
                    let actual = surface.pixel(x, y);
                    #[cfg(not(target_arch = "wasm32"))]
                    assert_eq!(actual, serial_surface.pixel(x, y), "serial/parallel profile routing at {x},{y}");
                    // rgb_to_cmyk explicitly bounds encoded RGB before B2A;
                    // this is a storage conversion boundary, not an error clamp.
                    let bounded_color = display_rgb(color);
                    let context =
                        format!("cmyk={cmyk} gradient={gradient} at{x},{y}; raw endpoints={raw_rgb:?}; painted RGB={color:?}; B2A input={bounded_color:?}");
                    let expected_rgb = close_display(&actual, eval(&b2a, &bounded_color), &a2b, &context);
                    // Also cover the actual fast A2B/display path, not just
                    // canonical interpretation of the stored ink separation.
                    let displayed = with_cmyk_space(Some(&space), || surface.rgba(x, y));
                    for c in 0..3 {
                        assert!(
                            (displayed[c].clamp(0.0, 1.0) - expected_rgb[c]).abs() <= 2. / 255.,
                            "actual display {displayed:?} expected {expected_rgb:?}; stored inks={actual:?}; {context}"
                        );
                    }
                    assert!((displayed[3] - 0.6).abs() < 0.00001);
                }
            }
        }
    }
}
#[test]
fn worker_scope_explicitly_clears_none_and_restores_context_on_return_and_unwind() {
    let bytes = profile().to_bytes();
    let ambient = CmykSpace::for_profile(Some(&bytes)).unwrap();
    let rect = Rect::new(0, 0, 1, 1);
    with_cmyk_space(Some(&ambient), || {
        let before = active_cmyk_space().unwrap().id;
        render_band_with_space(None, rect, &|_| {
            assert!(active_cmyk_space().is_none());
            Vec::new()
        });
        assert_eq!(active_cmyk_space().unwrap().id, before);
        let unwind = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            render_band_with_space(None, rect, &|_| {
                assert!(active_cmyk_space().is_none());
                panic!("intentional worker unwind")
            });
        }));
        assert!(unwind.is_err());
        assert_eq!(active_cmyk_space().unwrap().id, before);
    });
    assert!(active_cmyk_space().is_none());
}
#[cfg(not(target_arch = "wasm32"))]
#[test]
fn both_rayon_workers_receive_the_captured_profile() {
    let profile = profile();
    let bytes = profile.to_bytes();
    let space = CmykSpace::for_profile(Some(&bytes)).unwrap();
    let parsed = Profile::parse(&bytes).unwrap();
    let (_, b2a) = transforms(&parsed);
    let mut expected = [0.; 4];
    b2a.eval_fast(&[0.25, 0.6, 0.3], &mut expected);
    let format = PixelFormat::new(ColorMode::Cmyk, SampleType::F32, true);
    let rect = Rect::new(0, 0, 2, TILE_SIZE * 4);
    let mut surface = Surface::new(format);
    let pool = rayon::ThreadPoolBuilder::new().num_threads(2).build().unwrap();
    let barrier = std::sync::Barrier::new(2);
    let seen = std::sync::Mutex::new(std::collections::BTreeSet::new());
    pool.install(|| {
        with_cmyk_space(Some(&space), || {
            render_bands(&mut surface, rect, &|band| {
                let thread = rayon::current_thread_index().unwrap();
                let first = seen.lock().unwrap().insert(thread);
                if first {
                    barrier.wait();
                }
                assert_eq!(active_cmyk_space().unwrap().id, space.id);
                let mut data = Vec::new();
                for _ in 0..band.size().area() {
                    data.extend(photocraft_raster::from_rgba(&format, [0.25, 0.6, 0.3, 0.6]));
                }
                vec![(band, data)]
            })
        })
    });
    assert_eq!(seen.lock().unwrap().len(), 2);
    for y in [1, TILE_SIZE + 1, TILE_SIZE * 2 + 1, TILE_SIZE * 3 + 1] {
        let actual = surface.pixel(1, y);
        for c in 0..4 {
            assert!((actual[c] - expected[c]).abs() < 0.00001, "captured profile ink routing");
        }
        assert!((actual[4] - 0.6).abs() < 0.00001);
    }
    pool.broadcast(|_| assert!(active_cmyk_space().is_none()));
    assert!(active_cmyk_space().is_none());
}
