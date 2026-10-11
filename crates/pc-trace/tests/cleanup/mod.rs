//! Cleanup-specific fixtures and independent CPU/GPU/oracle gates. No desktop window.
use crate::cleanup::{self, Options, Upscale};
use crate::{ColorPath, Error, Image, Params, Preset, Trace};
use photocraft_doc::{Path, Subpath};
use photocraft_testkit::vector as metrics;
type TestResult = Result<(), Box<dyn std::error::Error>>;
use crate::test_common as common;
mod gpu;
mod quality;

fn input(w: u32, h: u32, rgba: &[u8]) -> Image<'_> {
    Image { width: w, height: h, rgba }
}
fn params() -> Params {
    Params { speckle: 0, ..Params::for_preset(Preset::Logo) }
}
fn fixture() -> Vec<u8> {
    let mut pixels = vec![255; 64 * 48 * 4];
    for y in 8..40 {
        for x in 8..56 {
            pixels[(y * 64 + x) * 4..][..4].copy_from_slice(&[20, 80, 190, 255]);
        }
    }
    for y in 18..30 {
        for x in 20..36 {
            pixels[(y * 64 + x) * 4..][..4].fill(255);
        }
    }
    pixels
}
fn with_path(path: Path) -> Trace {
    let nodes = metrics::node_count(&path);
    Trace { width: 128, height: 128, strokes: Vec::new(), layers: vec![ColorPath { color: [0, 0, 0, 255], path }], nodes }
}
fn coverage(out: &Trace, x: usize, y: usize) -> f32 {
    let rect = photocraft_geom::Rect::new(0, 0, out.width as i32, out.height as i32);
    let mut value = 0.;
    for layer in &out.layers {
        let a = photocraft_vector::path_coverage(&layer.path, rect)[y * out.width as usize + x];
        value = value * (1. - a) + if layer.color[..3] == [255; 3] { 0. } else { a };
    }
    value
}
#[test]
fn cleanup_disabled_is_byte_identical_and_old_settings_load() -> TestResult {
    let rgba = fixture();
    let p = params();
    let legacy = crate::trace(input(64, 48, &rgba), &p)?;
    let opts: Options = serde_json::from_str("{}")?;
    let out = cleanup::trace(input(64, 48, &rgba), &p, &opts)?;
    assert_eq!(serde_json::to_vec(&legacy)?, serde_json::to_vec(&out.trace)?);
    assert_eq!(out.stats.scale, 1);
    assert_eq!(out.stats.nodes_after, legacy.nodes);
    assert!(serde_json::from_str::<Options>(r#"{"unknown":true}"#).is_err());
    Ok(())
}
#[test]
fn cleanup_upscale_2x_4x_returns_source_coordinates_and_counters() -> TestResult {
    let rgba = fixture();
    let p = params();
    let original = crate::trace(input(64, 48, &rgba), &p)?;
    for scale in [Upscale::X2, Upscale::X4, Upscale::Auto] {
        let out = cleanup::trace(input(64, 48, &rgba), &p, &Options { upscale: scale, ..Options::default() })?;
        assert_eq!((out.trace.width, out.trace.height), (64, 48));
        assert_eq!(out.stats.scale, if scale == Upscale::X2 { 2 } else { 4 });
        assert!(coverage(&out.trace, 12, 12) > 0.99);
        assert!(coverage(&out.trace, 25, 22) < 0.01);
        assert!(coverage(&out.trace, 60, 43) < 0.01);
        let baseline = common::render(&original);
        let rendered = common::render(&out.trace);
        let f0 = metrics::fidelity(&baseline, &baseline)?.fidelity;
        let f1 = metrics::fidelity(&baseline, &rendered)?.fidelity;
        eprintln!("upscale {scale:?}: fidelity loss {:.6} nodes {} -> {}", f0 - f1, original.nodes, out.trace.nodes);
        assert!(f0 - f1 <= 0.002);
    }
    Ok(())
}
#[test]
fn cleanup_palette_excludes_aa_fringe_and_hidden_rgb() -> TestResult {
    let mut rgba = vec![255; 48 * 32 * 4];
    for y in 4..28 {
        for x in 4..44 {
            let c = if x == 4 || x == 43 || y == 4 || y == 27 { [120, 155, 222, 255] } else { [20, 80, 190, 255] };
            rgba[(y * 48 + x) * 4..][..4].copy_from_slice(&c);
        }
    }
    rgba[..4].copy_from_slice(&[255, 0, 170, 0]);
    let out = cleanup::trace(input(48, 32, &rgba), &params(), &Options { aa_palette: true, ..Options::default() })?;
    assert_eq!(out.stats.palette.len(), 2);
    assert!(out.stats.palette.contains(&[20, 80, 190]));
    assert!(out.stats.palette.contains(&[255; 3]));
    assert!(!out.stats.palette.contains(&[120, 155, 222]));
    assert!(!out.stats.palette.contains(&[255, 0, 170]));
    assert!(out.stats.palette_pixels >= 120);
    assert!(out.trace.layers.iter().all(|l| l.color[..3] == [20, 80, 190] || l.color[..3] == [255; 3]));
    Ok(())
}
#[test]
fn cleanup_bilateral_reduces_noise_preserves_edge_and_alpha() -> TestResult {
    let mut rgba = vec![255; 32 * 24 * 4];
    for y in 0..24 {
        for x in 0..16 {
            let v = if (x + y) % 2 == 0 { 24 } else { 36 };
            rgba[(y * 32 + x) * 4..][..4].copy_from_slice(&[v, v, v, 255]);
        }
    }
    rgba[..4].copy_from_slice(&[255, 0, 0, 0]);
    let mut stats = cleanup::Stats::default();
    let filtered =
        super::preprocess::prepare(input(32, 24, &rgba), &params(), &Options { prefilter: true, ..Options::default() }, 1, &mut stats, &|| false)?.pixels;
    assert_eq!(filtered[3], 0);
    assert_eq!(&filtered[16 * 4..][..4], &[255; 4]);
    assert!(filtered[(12 * 32 + 8) * 4].abs_diff(30) < rgba[(12 * 32 + 8) * 4].abs_diff(30));
    assert!(filtered[(12 * 32 + 15) * 4] < 40);
    assert!(stats.filtered_pixels > 200);
    Ok(())
}
#[test]
fn cleanup_hole_policy_fills_defects_preserves_contrasting_counters() -> TestResult {
    let mut rgba = vec![255; 24 * 24 * 4];
    for y in 3..21 {
        for x in 3..21 {
            rgba[(y * 24 + x) * 4..][..4].copy_from_slice(&[40, 40, 40, 255]);
        }
    }
    rgba[(8 * 24 + 8) * 4..][..3].fill(46);
    rgba[(12 * 24 + 12) * 4..][..3].fill(255);
    rgba[(16 * 24 + 16) * 4 + 3] = 0;
    let mut stats = cleanup::Stats::default();
    let out =
        super::preprocess::prepare(input(24, 24, &rgba), &params(), &Options { small_holes: true, ..Options::default() }, 1, &mut stats, &|| false)?.pixels;
    assert_eq!(&out[(8 * 24 + 8) * 4..][..3], &[40; 3]);
    assert_eq!(&out[(12 * 24 + 12) * 4..][..3], &[255; 3]);
    assert_eq!(out[(16 * 24 + 16) * 4 + 3], 0);
    assert_eq!(stats.filled_holes, 1);
    Ok(())
}
#[test]
fn cleanup_axes_collinear_right_angles_use_independent_oracles() -> TestResult {
    let path = Path::new(vec![Subpath::polygon(&[(8., 8.), (28., 8.01), (48., 8.04), (48.03, 48.), (8., 48.)])]);
    let original = with_path(path);
    for option in
        [Options { axes: true, ..Options::default() }, Options { collinear: true, ..Options::default() }, Options { right_angles: true, ..Options::default() }]
    {
        let out = cleanup::clean_geometry(&original, &option, || false)?;
        assert!(metrics::hausdorff(&original.layers[0].path, &out.trace.layers[0].path, 256)? <= option.tolerance);
        assert!(metrics::self_intersections(&out.trace.layers[0].path).is_empty());
        if option.collinear {
            assert_eq!(out.stats.collinear_nodes, 1);
            assert_eq!(out.trace.nodes, 4);
        }
        if option.axes {
            assert!(out.stats.axis_segments >= 1);
        }
        if option.right_angles {
            assert!(out.stats.right_angle_corners >= 1);
            let p: Vec<_> = out.trace.layers[0].path.subpaths[0].knots.iter().map(|k| k.anchor).collect();
            let last = p.len() - 1;
            let u = (p[last].x - p[last - 1].x, p[last].y - p[last - 1].y);
            let v = (p[0].x - p[last].x, p[0].y - p[last].y);
            assert!((u.0 * v.0 + u.1 * v.1).abs() < 1e-6);
        }
    }
    Ok(())
}
fn sampled_ellipse(rx: f64, ry: f64, rotation: f64, reverse: bool) -> Subpath {
    let (sin, cos) = rotation.sin_cos();
    let mut points: Vec<_> = (0..128)
        .map(|i| {
            let a = std::f64::consts::TAU * i as f64 / 128.;
            (64. + rx * a.cos() * cos - ry * a.sin() * sin, 64. + rx * a.cos() * sin + ry * a.sin() * cos)
        })
        .collect();
    if reverse {
        points.reverse();
    }
    Subpath::polygon(&points)
}
#[test]
fn cleanup_circle_ellipse_preserves_area_winding_small_circle_and_counter() -> TestResult {
    for (rx, ry, angle, circle) in [(28., 28., 0., true), (32., 16., 0.37, false), (2., 2., 0., true)] {
        let mut hole = sampled_ellipse(rx / 2., ry / 2., angle, true);
        hole.op = photocraft_doc::PathOp::Join;
        let path = Path::new(vec![sampled_ellipse(rx, ry, angle, false), hole]);
        let original = with_path(path);
        let out = cleanup::clean_geometry(&original, &Options { circles: true, ellipses: true, ..Options::default() }, || false)?;
        let p = &out.trace.layers[0].path;
        assert_eq!(p.subpaths.len(), 2);
        assert!(out.trace.nodes <= 12);
        assert_eq!(if circle { out.stats.circles } else { out.stats.ellipses }, 2);
        assert!((metrics::area(p, p.fill_rule)? - std::f64::consts::PI * rx * ry * 0.75).abs() <= 0.003 * std::f64::consts::PI * rx * ry);
        assert!(metrics::hausdorff(&original.layers[0].path, p, 256)? <= 0.2);
        assert!(metrics::self_intersections(p).is_empty());
        if rx > 2. {
            assert!(coverage(&out.trace, 64, 64) < 0.01);
        }
    }
    Ok(())
}
#[test]
fn cleanup_arc_reduces_nodes_without_changing_endpoints() -> TestResult {
    let pts: Vec<_> = (0..65)
        .map(|i| {
            let a = std::f64::consts::PI * i as f64 / 64.;
            (64. + 32. * a.cos(), 64. + 32. * a.sin())
        })
        .collect();
    for closed in [false, true] {
        let sub = if closed { Subpath::polygon(&pts) } else { Subpath::polyline(&pts) };
        let original = with_path(Path::new(vec![sub]));
        let out = cleanup::clean_geometry(&original, &Options { arcs: true, ..Options::default() }, || false)?;
        assert_eq!(out.stats.arcs, 1);
        assert!(out.trace.nodes <= 5);
        assert_eq!(out.trace.layers[0].path.subpaths[0].closed, closed);
        assert!(metrics::hausdorff(&original.layers[0].path, &out.trace.layers[0].path, 256)? <= 0.2);
    }
    Ok(())
}
#[test]
fn cleanup_symmetry_only_above_strict_mask_iou() -> TestResult {
    let mut pixels = vec![255; 64 * 48 * 4];
    for y in 8..40 {
        for x in 8..56 {
            pixels[(y * 64 + x) * 4..][..4].copy_from_slice(&[0, 0, 0, 255]);
        }
    }
    let symmetric = cleanup::trace(input(64, 48, &pixels), &params(), &Options { symmetry: true, ..Options::default() })?;
    assert!(symmetric.stats.symmetry_iou.iter().all(|&v| v > 0.98));
    // An asymmetric triangular bite leaves the same bbox; bbox-only detection would be wrong.
    for y in 8..24 {
        for x in 8..8 + (24 - y) {
            pixels[(y * 64 + x) * 4..][..4].fill(255);
        }
    }
    let asymmetric = cleanup::trace(input(64, 48, &pixels), &params(), &Options { symmetry: true, ..Options::default() })?;
    assert!(asymmetric.stats.symmetry_iou.iter().any(|&v| v < 0.98));
    assert_eq!(asymmetric.stats.symmetry_axes, 0);
    // Exactly .98 is rejected (198 black pixels, intersection 196 => 196/200).
    let mut pixels = vec![255; 20 * 10 * 4];
    for p in pixels.as_chunks_mut::<4>().0 {
        p[..3].fill(0);
    }
    pixels[(2 * 20 + 1) * 4..][..3].fill(255);
    pixels[(3 * 20 + 2) * 4..][..3].fill(255);
    let trace = Trace { width: 20, height: 10, strokes: Vec::new(), layers: vec![ColorPath { color: [0, 0, 0, 255], path: Path::default() }], nodes: 0 };
    let mut stats = cleanup::Stats::default();
    let axes = super::symmetry::axes(&trace, 0, &pixels, 1, &mut stats, &|| false)?;
    assert!((stats.symmetry_iou[0] - 0.98).abs() < 1e-12);
    assert!(axes.iter().all(|a| !a.vertical));
    Ok(())
}
#[test]
fn cleanup_budget_cancel_finite_and_thread_determinism() -> TestResult {
    let rgba = fixture();
    assert!(matches!(cleanup::trace_with_cancel(input(64, 48, &rgba), &params(), &Options::logo(), || true), Err(Error::Cancelled)));
    let calls = std::cell::Cell::new(0);
    assert!(matches!(
        cleanup::trace_with_cancel(input(64, 48, &rgba), &params(), &Options::logo(), || {
            calls.set(calls.get() + 1);
            calls.get() > 9
        }),
        Err(Error::Cancelled)
    ));
    let big = vec![255; 1025 * 1025 * 4];
    assert!(matches!(cleanup::trace(input(1025, 1025, &big), &params(), &Options { upscale: Upscale::X4, ..Options::default() }), Err(Error::PixelBudget(_))));
    for bad in [f64::NAN, f64::INFINITY, -1., 3.] {
        assert!(Options { tolerance: bad, ..Options::default() }.validate().is_err());
    }
    let expected = serde_json::to_vec(&cleanup::trace(input(64, 48, &rgba), &params(), &Options::logo())?)?;
    for threads in [1, 2, 4] {
        std::thread::scope(|scope| -> TestResult {
            let handles: Vec<_> = (0..threads).map(|_| scope.spawn(|| cleanup::trace(input(64, 48, &rgba), &params(), &Options::logo()))).collect();
            for handle in handles {
                let result = handle.join().map_err(|_| "trace worker panicked")??;
                assert_eq!(serde_json::to_vec(&result)?, expected);
            }
            Ok(())
        })?;
    }
    Ok(())
}

#[test]
fn cleanup_symmetrizes_control_handles_and_disjoint_rings() -> TestResult {
    let mut path = Path::new(vec![Subpath::polygon(&[(8.02, 8.), (55.99, 8.01), (56.01, 40.), (8., 39.99)])]);
    let axis = super::symmetry::Axis { vertical: true, position: 32. };
    let out = super::symmetry::symmetrize(&path, axis, 0.2).ok_or("symmetry was not applied")?;
    let knots = &out.subpaths[0].knots;
    assert!((knots[0].anchor.x + knots[1].anchor.x - 64.).abs() < 1e-12);
    assert_eq!(knots[0].anchor.y, knots[1].anchor.y);
    assert!(metrics::hausdorff(&path, &out, 16)? < 0.2);
    path = Path::new(vec![
        Subpath::polygon(&[(8.02, 8.), (20., 8.), (20., 20.), (8., 20.)]),
        Subpath::polygon(&[(44., 8.), (56., 8.), (56., 20.), (44., 20.)]).with_op(photocraft_doc::PathOp::Join),
    ]);
    assert!(super::symmetry::symmetrize(&path, axis, 0.2).is_some());
    let mut bad = with_path(path);
    bad.layers[0].path.subpaths[0].knots[0].out_ctrl.x = f64::INFINITY;
    assert!(cleanup::clean_geometry(&bad, &Options { circles: true, ..Options::default() }, || false).is_err());
    Ok(())
}

proptest::proptest! {
    #![proptest_config(proptest::test_runner::Config { cases:256, ..proptest::test_runner::Config::default() })]
    #[test]
    fn cleanup_random_masks_are_finite_closed_and_deterministic(
        width in 1u32..9, height in 1u32..9,
        bits in proptest::collection::vec(proptest::bool::ANY,64),
    ) {
        let rgba:Vec<u8> = (0..width as usize*height as usize).flat_map(|i| if bits[i%bits.len()] { [0,0,0,255] } else { [255;4] }).collect();
        let opts=Options { aa_palette:true,collinear:true,..Options::default() };
        let p=Params { fitter:crate::Fitter::Pixel,speckle:0,..params() };
        let a=cleanup::trace(input(width,height,&rgba),&p,&opts).map_err(|e|proptest::test_runner::TestCaseError::fail(e.to_string()))?;
        let b=cleanup::trace(input(width,height,&rgba),&p,&opts).map_err(|e|proptest::test_runner::TestCaseError::fail(e.to_string()))?;
        proptest::prop_assert_eq!(serde_json::to_vec(&a).map_err(|e|proptest::test_runner::TestCaseError::fail(e.to_string()))?,serde_json::to_vec(&b).map_err(|e|proptest::test_runner::TestCaseError::fail(e.to_string()))?);
        for l in a.trace.layers {
            proptest::prop_assert!(metrics::is_finite(&l.path));
            proptest::prop_assert!(l.path.subpaths.iter().all(|s|s.closed));
            proptest::prop_assert!(metrics::self_intersections(&l.path).is_empty());
        }
    }
}

#[test]
fn cleanup_logo_keeps_counters_small_circles_and_ignored_background() -> TestResult {
    let mut rgba = vec![255; 64 * 64 * 4];
    for y in 0..64 {
        for x in 0..64 {
            let r = (x as f64 + 0.5 - 24.).hypot(y as f64 + 0.5 - 34.);
            let small = (x as f64 + 0.5 - 52.).hypot(y as f64 + 0.5 - 12.);
            if (7.0..=18.).contains(&r) || small <= 2.5 {
                rgba[(y * 64 + x) * 4..][..3].fill(0);
            }
        }
    }
    let p = Params { ignore_color: Some([255; 3]), ..Params::default() };
    let out = cleanup::trace(input(64, 64, &rgba), &p, &Options::logo())?;
    assert!(out.trace.layers.iter().all(|l| l.color[..3] == [0; 3]));
    assert!(coverage(&out.trace, 24, 34) < 0.01);
    assert!(coverage(&out.trace, 36, 34) > 0.99);
    assert!(coverage(&out.trace, 51, 11) > 0.99);
    assert!(coverage(&out.trace, 1, 1) < 0.01);
    assert_eq!(out.stats.filled_holes, 0);
    Ok(())
}

#[test]
fn cleanup_bw_palette_toggle_preserves_threshold_and_pixel_upscale() -> TestResult {
    let rgba = fixture();
    let p = Params::for_preset(Preset::BlackWhite);
    let a = crate::trace(input(64, 48, &rgba), &p)?;
    let b = cleanup::trace(input(64, 48, &rgba), &p, &Options { aa_palette: true, ..Options::default() })?;
    assert!(!b.trace.layers.is_empty());
    assert_eq!(serde_json::to_vec(&a)?, serde_json::to_vec(&b.trace)?);
    let p = Params::for_preset(Preset::PixelArt);
    let a = crate::trace(input(64, 48, &rgba), &p)?;
    let b = cleanup::trace(input(64, 48, &rgba), &p, &Options { upscale: Upscale::X4, ..Options::default() })?;
    assert_eq!(common::render(&a), common::render(&b.trace));
    assert!(b.trace.layers.iter().flat_map(|l| &l.path.subpaths).flat_map(|s| &s.knots).all(|k| k.anchor.x.fract() == 0. && k.anchor.y.fract() == 0.));
    assert!(cleanup::trace(input(64, 48, &rgba), &Params { speckle: usize::MAX, ..params() }, &Options::default()).is_err());
    Ok(())
}
