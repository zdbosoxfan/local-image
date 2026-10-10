//! Independent exact-area and randomized partition oracles for package 27.
use super::*;
use crate::{Fitter, Preset};
use photocraft_doc::Path;
use photocraft_geom::{Affine, Rect};
use photocraft_testkit::vector as metrics;
use proptest::prelude::*;

type TestResult = Result<(), Box<dyn std::error::Error>>;
fn grid(w: u32, labels: &[u32], n: usize) -> LabelMap {
    LabelMap { width: w, height: labels.len() as u32 / w, labels: labels.to_vec(), colors: (0..n).map(|i| [i as u8 * 40, 20, 160, 255]).collect() }
}
fn render_map(map: &LabelMap, fitter: Fitter) -> Result<Trace, Error> {
    let graph = graph::BoundaryGraph::extract(map, EDGE_BUDGET, &|| false)?;
    let faces = face::assemble(&graph, map, &|| false)?;
    let mut params = Params::for_preset(Preset::PixelArt);
    params.fitter = fitter;
    params.detail = 0.7;
    let fitted = fit::fit_all(&graph, &params, &|| false)?;
    let fitted = if topology::valid(&graph, &fitted, EDGE_BUDGET, &|| false)? { fitted } else { fit::pixel_all(&graph, &|| false)? };
    compose::trace(map, &faces, &fitted, &|| false)
}
fn rgba(map: &LabelMap) -> Vec<u8> {
    map.labels.iter().flat_map(|&id| if id == OUTSIDE { [255, 255, 255, 0] } else { map.colors[id as usize] }).collect()
}
fn valid(t: &Trace) {
    assert_eq!(t.nodes, t.layers.iter().map(|l| metrics::node_count(&l.path)).sum::<usize>());
    for l in &t.layers {
        assert!(metrics::is_finite(&l.path));
        assert!(l.path.subpaths.iter().all(|s| s.closed && s.knots.len() >= 2));
        assert!(metrics::self_intersections(&l.path).is_empty());
    }
}
fn exact_oracle(map: &LabelMap, t: &Trace) {
    valid(t);
    for scale in [1, 2, 4, 8] {
        let rect = Rect::new(0, 0, map.width as i32 * scale, map.height as i32 * scale);
        let coverage: Vec<_> = t.layers.iter().map(|l| photocraft_vector::path_coverage(&l.path.transform(&Affine::scale(f64::from(scale))), rect)).collect();
        for y in 0..rect.height() as usize {
            for x in 0..rect.width() as usize {
                let label = map.label(x as i32 / scale, y as i32 / scale);
                let mut sum = 0.;
                for (l, cov) in t.layers.iter().zip(&coverage) {
                    let expected = if label != OUTSIDE && l.color == map.colors[label as usize] { 1. } else { 0. };
                    let v = cov[y * rect.width() as usize + x];
                    assert!((v - expected).abs() <= 1. / 255., "scale {scale}, ({x},{y}) expected {expected}, got {v}");
                    sum += v;
                }
                assert!((sum - if label == OUTSIDE { 0. } else { 1. }).abs() <= 1. / 255.);
            }
        }
    }
}
#[test]
fn cutout_one_pixel_and_empty_alpha() -> TestResult {
    let p = Params::for_preset(Preset::PixelArt);
    let t = trace(Image { width: 1, height: 1, rgba: &[1, 2, 3, 255] }, &p)?;
    assert_eq!(t.layers.len(), 1);
    assert_eq!(t.nodes, 4);
    assert!(trace(Image { width: 1, height: 1, rgba: &[1, 2, 3, 0] }, &p)?.layers.is_empty());
    Ok(())
}
#[test]
fn cutout_junction_counter_border_and_diagonal_partitions() -> TestResult {
    for map in [
        grid(2, &[0, 1, 1, 0], 2),
        grid(3, &[0, 0, 1, 0, 2, 1, 2, 2, 1], 3),
        grid(5, &[0, 0, 0, 0, 0, 0, 1, 1, 1, 0, 0, 1, OUTSIDE, 1, 0, 0, 1, 1, 1, 0, 0, 0, 0, 0, 0], 2),
        grid(3, &[OUTSIDE, 0, OUTSIDE, 0, OUTSIDE, 0, OUTSIDE, 0, OUTSIDE], 1),
    ] {
        exact_oracle(&map, &render_map(&map, Fitter::Pixel)?);
    }
    let diagonal = render_map(&grid(2, &[0, 1, 1, 0], 2), Fitter::Pixel)?;
    assert!(diagonal.layers.iter().all(|l| l.path.subpaths.len() == 2));
    Ok(())
}
proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]
    #[test]
    fn cutout_random_partition_matches_independent_coverage(labels in prop::collection::vec(0u32..5, 64)) {
        let labels: Vec<_> = labels.into_iter().map(|id| if id == 4 { OUTSIDE } else { id }).collect();
        let map = grid(8, &labels, 4);
        let t = render_map(&map, Fitter::Pixel)?;
        exact_oracle(&map, &t);
    }
}
fn bits(p: photocraft_geom::Point) -> [u64; 2] {
    [p.x.to_bits(), p.y.to_bits()]
}
fn edges(p: &Path) -> Vec<[[u64; 2]; 4]> {
    p.subpaths.iter().flat_map(|s| s.segments()).map(|c| c.map(bits)).collect()
}
#[test]
fn cutout_adjacent_cubic_controls_reverse_byte_identically() -> TestResult {
    // Curved island shares its entire fitted ring with the surrounding colour.
    let labels: Vec<_> = (0..24).flat_map(|y| (0..24).map(move |x| u32::from((x - 12i32).pow(2) + (y - 12i32).pow(2) < 64))).collect();
    let map = grid(24, &labels, 2);
    for fitter in [Fitter::Potrace, Fitter::Spline] {
        let t = render_map(&map, fitter)?;
        valid(&t);
        let inside = t.layers.iter().find(|l| l.color == map.colors[1]).ok_or("missing island")?;
        let outside = t.layers.iter().find(|l| l.color == map.colors[0]).ok_or("missing surrounding region")?;
        let surrounding = edges(&outside.path);
        let curves = edges(&inside.path);
        assert!(curves.iter().any(|c| c[0] != c[1] && c[2] != c[3]), "must test real fitted cubic controls");
        for c in curves {
            assert!(surrounding.contains(&[c[3], c[2], c[1], c[0]]), "shared control bits differ");
        }
    }
    Ok(())
}
#[test]
fn cutout_alpha_ignore_palette_and_paint_order() -> TestResult {
    let map = grid(5, &[0, 0, 0, 0, 0, 0, 1, 1, 1, 0, 0, 1, OUTSIDE, 1, 0, 0, 1, 1, 1, 0, 0, 0, 0, 0, 0], 2);
    let data = rgba(&map);
    let image = Image { width: 5, height: 5, rgba: &data };
    let mut p = Params::for_preset(Preset::PixelArt);
    p.palette = map.colors.iter().map(|c| [c[0], c[1], c[2]]).collect();
    let t = trace(image, &p)?;
    exact_oracle(&map, &t);
    let stacked = crate::trace(image, &p)?;
    assert_eq!(t.layers.iter().map(|l| l.color).collect::<Vec<_>>(), stacked.layers.iter().map(|l| l.color).collect::<Vec<_>>());
    p.ignore_color = Some([map.colors[0][0], 20, 160]);
    let ignored = trace(image, &p)?;
    assert_eq!(ignored.layers.len(), 1);
    assert_eq!(photocraft_vector::path_coverage(&ignored.layers[0].path, Rect::new(0, 0, 5, 5))[12], 0.);
    p.ignore_color = None;
    p.use_alpha = false;
    assert!(trace(image, &p)?.layers.iter().any(|l| photocraft_vector::path_coverage(&l.path, Rect::new(0, 0, 5, 5))[12] > 0.99));
    Ok(())
}
#[test]
fn cutout_budget_and_cancellation_at_each_stage() -> TestResult {
    let map = grid(2, &[0, 1, 1, 0], 2);
    assert!(matches!(graph::BoundaryGraph::extract(&map, 11, &|| false), Err(Error::BoundaryBudget)));
    let graph = graph::BoundaryGraph::extract(&map, 12, &|| false)?;
    assert!(matches!(graph::BoundaryGraph::extract(&map, 12, &|| true), Err(Error::Cancelled)));
    assert!(matches!(face::assemble(&graph, &map, &|| true), Err(Error::Cancelled)));
    let p = Params::for_preset(Preset::PixelArt);
    assert!(matches!(fit::fit_all(&graph, &p, &|| true), Err(Error::Cancelled)));
    let f = fit::fit_all(&graph, &p, &|| false)?;
    assert!(matches!(topology::valid(&graph, &f, 12, &|| true), Err(Error::Cancelled)));
    let faces = face::assemble(&graph, &map, &|| false)?;
    assert!(matches!(compose::trace(&map, &faces, &f, &|| true), Err(Error::Cancelled)));
    let data = rgba(&map);
    assert!(matches!(trace_cancellable(Image { width: 2, height: 2, rgba: &data }, &p, &AtomicBool::new(true)), Err(Error::Cancelled)));
    Ok(())
}
#[test]
fn cutout_invalid_inputs_and_numeric_params_are_rejected() {
    let mut p = Params::default();
    for value in [f64::NAN, f64::INFINITY, -1., 17.] {
        p.detail = value;
        assert!(trace(Image { width: 1, height: 1, rgba: &[0; 4] }, &p).is_err());
    }
    assert!(matches!(trace(Image { width: 16000, height: 16000, rgba: &[] }, &Params::default()), Err(Error::PixelBudget(_))));
    assert!(matches!(trace(Image { width: 0, height: 1, rgba: &[] }, &Params::default()), Err(Error::EmptyImage)));
    assert!(matches!(trace(Image { width: 1, height: 1, rgba: &[] }, &Params::default()), Err(Error::InvalidBuffer)));
}
#[test]
fn cutout_is_deterministic_across_one_two_four_threads() -> TestResult {
    let map = grid(8, &(0..64).map(|i| (i % 5) as u32).collect::<Vec<_>>(), 5);
    let data = rgba(&map);
    let work = || -> Result<Vec<u8>, Box<dyn std::error::Error + Send + Sync>> {
        Ok(serde_json::to_vec(&trace(Image { width: 8, height: 8, rgba: &data }, &Params::for_preset(Preset::PixelArt))?)?)
    };
    let baseline = work().map_err(|e| e.to_string())?;
    for count in [1, 2, 4] {
        std::thread::scope(|scope| -> Result<(), String> {
            let handles: Vec<_> = (0..count).map(|_| scope.spawn(work)).collect();
            for h in handles {
                assert_eq!(h.join().map_err(|_| "trace thread panicked")?.map_err(|e| e.to_string())?, baseline);
            }
            Ok(())
        })?;
    }
    Ok(())
}
#[test]
fn cutout_jpeg60_is_finite_closed_and_topology_valid() -> TestResult {
    let pixels = image::RgbImage::from_fn(32, 32, |x, y| {
        image::Rgb(if (x as i32 - 16).pow(2) + (y as i32 - 16).pow(2) < 100 { [25, 60, 190] } else { [240, 240, 240] })
    });
    let mut bytes = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut bytes, 60).encode_image(&pixels)?;
    let decoded = image::load_from_memory(&bytes)?.to_rgba8();
    for preset in [Preset::Logo, Preset::Photo] {
        valid(&trace(Image { width: 32, height: 32, rgba: decoded.as_raw() }, &Params::for_preset(preset))?);
    }
    Ok(())
}

mod benchmark;
#[path = "../it/common/mod.rs"]
mod common;

#[test]
fn cutout_open_shared_chain_partition_and_controls() -> TestResult {
    let labels: Vec<_> = (0..32).flat_map(|y| (0..32).map(move |x| u32::from(x >= 16 + ((f64::from(y) / 5.).sin() * 5.).round() as i32))).collect();
    let map = grid(32, &labels, 2);
    let t = render_map(&map, Fitter::Spline)?;
    valid(&t);
    let a = edges(&t.layers[0].path);
    let b = edges(&t.layers[1].path);
    let shared: Vec<_> = a.iter().filter(|c| b.contains(&[c[3], c[2], c[1], c[0]])).collect();
    assert!(!shared.is_empty());
    assert!(shared.iter().any(|c| c[0] != c[1] && c[2] != c[3]), "shared open boundary must actually be curved");
    for scale in [1., 2., 4., 8.] {
        let rect = Rect::new(0, 0, (32. * scale) as i32, (32. * scale) as i32);
        let cov: Vec<_> = t.layers.iter().map(|l| photocraft_vector::path_coverage(&l.path.transform(&Affine::scale(scale)), rect)).collect();
        let worst = cov[0].iter().zip(&cov[1]).map(|(a, b)| (a + b - 1.).abs()).fold(0., f32::max);
        assert!(worst <= 1. / 255., "coverage sum differs by {worst} at {scale}x");
    }
    Ok(())
}

#[test]
fn cutout_topology_guard_rejects_crossing_shared_chains_and_ring_inversion() -> TestResult {
    use kurbo::{Line, PathSeg, Point as KPoint};
    let map = grid(4, &[0, 0, 1, 1, 0, 0, 1, 1, 0, 0, 1, 1, 0, 0, 1, 1], 2);
    let g = graph::BoundaryGraph::extract(&map, EDGE_BUDGET, &|| false)?;
    let mut f = fit::pixel_all(&g, &|| false)?;
    let id = g.segments.iter().position(|s| s.left != OUTSIDE && s.right != OUTSIDE).ok_or("shared chain missing")?;
    let s = &g.segments[id];
    let a = s.points[0];
    let b = s.points[s.points.len() - 1];
    let p = KPoint::new(f64::from(a.x), f64::from(a.y));
    let q = KPoint::new(f64::from(b.x), f64::from(b.y));
    f[id] = vec![PathSeg::Line(Line::new(p, KPoint::new(8., 2.))), PathSeg::Line(Line::new(KPoint::new(8., 2.), q))];
    assert!(!topology::valid(&g, &f, EDGE_BUDGET, &|| false)?);
    let ring = grid(3, &[0; 9], 1);
    let g = graph::BoundaryGraph::extract(&ring, EDGE_BUDGET, &|| false)?;
    let mut f = fit::pixel_all(&g, &|| false)?;
    f[0] = f[0].iter().rev().map(|s| s.reverse()).collect();
    assert!(!topology::valid(&g, &f, EDGE_BUDGET, &|| false)?);
    Ok(())
}

#[test]
fn cutout_fitted_random_opaque_partitions_cover_canvas() -> TestResult {
    let mut state = 27u64;
    for _ in 0..32 {
        let labels: Vec<_> = (0..64)
            .map(|_| {
                state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
                (state >> 32) as u32 % 4
            })
            .collect();
        let map = grid(8, &labels, 4);
        for fitter in [Fitter::Potrace, Fitter::Spline] {
            let t = render_map(&map, fitter)?;
            valid(&t);
            let mut sums = vec![0.; 64];
            for l in &t.layers {
                for (sum, coverage) in sums.iter_mut().zip(photocraft_vector::path_coverage(&l.path, Rect::new(0, 0, 8, 8))) {
                    *sum += coverage;
                }
            }
            assert!(sums.iter().all(|s| (s - 1.).abs() <= 1. / 255.), "fitted opaque partition has gap/overlap: {sums:?}");
        }
    }
    Ok(())
}
#[test]
fn cutout_layering_defaults_and_old_params_load() -> TestResult {
    assert_eq!(Layering::default(), Layering::Stacked);
    assert_eq!(serde_json::to_string(&Layering::Cutout)?, "\"cutout\"");
    assert_eq!(serde_json::from_str::<Layering>("\"stacked\"")?, Layering::Stacked);
    let params: Params = serde_json::from_str("{}")?;
    assert_eq!(params.fitter, Fitter::Potrace);
    params.validate()?;
    Ok(())
}

#[test]
fn cutout_fitted_one_pixel_wide_canvas_border_stays_exact() -> TestResult {
    for map in [grid(2, &[0, 1], 2), grid(1, &[0, 1], 2), grid(3, &[0, 1, 2], 3)] {
        for fitter in [Fitter::Potrace, Fitter::Spline] {
            exact_oracle(&map, &render_map(&map, fitter)?);
        }
    }
    Ok(())
}

#[test]
fn cutout_midflight_cancellation_drops_all_output() {
    let data: Vec<_> = (0..64 * 64).flat_map(|i| if i % 3 == 0 { [200, 20, 40, 255] } else { [20, 40, 200, 255] }).collect();
    let checks = std::cell::Cell::new(0usize);
    let result = trace_with_cancel(Image { width: 64, height: 64, rgba: &data }, &Params::for_preset(Preset::PixelArt), || {
        let n = checks.get() + 1;
        checks.set(n);
        n >= 30
    });
    assert!(matches!(result, Err(Error::Cancelled)));
    assert!(checks.get() >= 30);
}

#[test]
fn cutout_gradient_merging_uses_running_means_and_keeps_outside_and_order() -> TestResult {
    let mut map = grid(5, &[0, 1, 2, OUTSIDE, 3], 4);
    map.colors = vec![[100, 100, 100, 255], [103, 103, 103, 255], [106, 106, 106, 255], [100, 100, 100, 255]];
    merge::similar(&mut map, 9, &|| false)?;
    assert_eq!(map.colors, vec![[101, 101, 101, 255], [106, 106, 106, 255], [100, 100, 100, 255]]);
    assert_eq!(map.labels, vec![0, 0, 1, OUTSIDE, 2]);
    // A disconnected same-colour patch cannot be merged across alpha.
    assert!(matches!(merge::similar(&mut map, 9, &|| true), Err(Error::Cancelled)));
    let mut exact = grid(2, &[0, 1], 2);
    exact.colors = vec![[100, 100, 100, 255], [101, 101, 101, 255]];
    merge::similar(&mut exact, 0, &|| false)?;
    assert_eq!(exact.labels, vec![0, 1]);
    Ok(())
}
