use lightcraft_develop::{DevelopSettings, Spot, SpotMode};
use lightcraft_geom::Point;
use lightcraft_pipeline::geometry::Frame;
use lightcraft_pipeline::spots::{self, Stages};
use lightcraft_raster::Rgb32f;
use std::time::Duration;

/// Deterministic textured test image: gradients, a hash noise grain.
fn synthetic(w: usize, h: usize) -> Rgb32f {
    Rgb32f::from_fn(w, h, |x, y| {
        let n = (x as u32).wrapping_mul(0x9E37_79B1) ^ (y as u32).wrapping_mul(0x85EB_CA6B);
        let n = (n ^ (n >> 15)).wrapping_mul(0x2C1B_3C6D);
        let grain = ((n >> 8) & 0xFFFF) as f32 / 65535.0 - 0.5;
        let base = 0.25 + 0.3 * (x as f32 / w as f32) + 0.15 * ((y as f32 * 0.01).sin());
        [base + grain * 0.04, base * 0.9 + grain * 0.03, base * 0.8 + grain * 0.05]
    })
}

/// Flat image with a dark square in the middle.
fn image_with_dot(w: usize, h: usize, dot_size: usize) -> Rgb32f {
    let mut img = Rgb32f::from_fn(w, h, |x, _| [0.2 + x as f32 * 0.001; 3]);
    let cx = w / 2;
    let cy = h / 2;
    let half = dot_size / 2;
    for y in cy.saturating_sub(half)..cy.saturating_add(half).min(h) {
        for x in cx.saturating_sub(half)..cx.saturating_add(half).min(w) {
            img.set(x, y, [0.0; 3]);
        }
    }
    img
}

/// Half dark, half bright image.
fn half_half(w: usize, h: usize) -> Rgb32f {
    Rgb32f::from_fn(w, h, |x, _| if x < w / 2 { [0.1; 3] } else { [0.9; 3] })
}

fn make_spot(mode: SpotMode, points: Vec<Point>, size: f64, feather: f64, source_offset: Option<Point>) -> Spot {
    Spot { mode, points, size, feather, source_offset, ..Default::default() }
}

#[test]
fn empty_spots_leave_image_unchanged() {
    let mut img = synthetic(64, 48);
    let original = img.clone();
    let frame = Frame::new(64, 48, &DevelopSettings::default(), true);
    let ppl = frame.px_per_long(64);
    spots::apply(&mut img, &[], &frame, ppl);
    assert_eq!(img.data, original.data);
}

#[test]
fn clone_spot_copies_source_region() {
    let mut img = half_half(100, 100);
    let frame = Frame::new(100, 100, &DevelopSettings::default(), true);
    let ppl = frame.px_per_long(100);
    let spot = make_spot(SpotMode::Clone, vec![Point::new(0.25, 0.5)], 0.05, 0.0, Some(Point::new(0.5, 0.0)));
    spots::apply(&mut img, &[spot], &frame, ppl);
    // Left half should become bright (copied from right half)
    let left = img.get(25, 50)[0];
    let right = img.get(5, 50)[0];
    assert!(left > 0.8, "expected left to be bright, got {left}");
    assert!(right < 0.2, "expected right to remain dark, got {right}");
}

#[test]
fn heal_spot_removes_dot() {
    let mut img = image_with_dot(120, 80, 8);
    let frame = Frame::new(120, 80, &DevelopSettings::default(), true);
    let ppl = frame.px_per_long(120);
    let spot = make_spot(SpotMode::Heal, vec![Point::new(0.5, 0.5)], 8.0 / 120.0, 30.0, None);
    spots::apply(&mut img, &[spot], &frame, ppl);
    let c = img.get(60, 40);
    assert!((c[0] - 0.26).abs() < 0.05, "expected healed dot to match background, got {c:?}");
}

#[test]
fn apply_multiple_spots_sequentially() {
    let mut img = synthetic(200, 150);
    let frame = Frame::new(200, 150, &DevelopSettings::default(), true);
    let ppl = frame.px_per_long(200);
    let spot1 = make_spot(SpotMode::Clone, vec![Point::new(0.2, 0.2)], 0.05, 0.0, Some(Point::new(0.1, 0.0)));
    let spot2 = make_spot(SpotMode::Heal, vec![Point::new(0.8, 0.8)], 0.05, 20.0, None);
    let original = img.clone();
    spots::apply(&mut img, &[spot1, spot2], &frame, ppl);
    assert_ne!(img.data, original.data, "image should change after applying two spots");
    // Both areas should differ from original
    assert_ne!(img.get(40, 30), original.get(40, 30));
    assert_ne!(img.get(160, 120), original.get(160, 120));
}

#[test]
fn boundary_spot_near_edge_does_not_panic() {
    let mut img = synthetic(10, 10);
    let frame = Frame::new(10, 10, &DevelopSettings::default(), true);
    let ppl = frame.px_per_long(10);
    // Spot at top-left corner, large radius
    let spot = make_spot(SpotMode::Heal, vec![Point::new(0.0, 0.0)], 0.5, 20.0, None);
    spots::apply(&mut img, &[spot], &frame, ppl);
    // All pixels must be non-negative and finite
    for px in &img.data {
        for &v in px.iter() {
            assert!(v.is_finite() && v >= 0.0, "found invalid pixel {v}");
        }
    }
}

#[test]
fn tiny_image_1x1_does_not_panic() {
    let mut img = Rgb32f::new(1, 1);
    img.set(0, 0, [0.5; 3]);
    let frame = Frame::new(1, 1, &DevelopSettings::default(), true);
    let ppl = frame.px_per_long(1);
    let spot = make_spot(SpotMode::Heal, vec![Point::new(0.5, 0.5)], 1.0, 0.0, None);
    spots::apply(&mut img, &[spot], &frame, ppl);
    assert!(img.get(0, 0)[0].is_finite());
}

#[test]
fn odd_sizes_work() {
    for (w, h) in [(5, 3), (7, 1), (3, 5)] {
        let mut img = synthetic(w, h);
        let frame = Frame::new(w, h, &DevelopSettings::default(), true);
        let ppl = frame.px_per_long(w);
        let spot = make_spot(SpotMode::Heal, vec![Point::new(0.5, 0.5)], 0.1, 10.0, None);
        spots::apply(&mut img, &[spot], &frame, ppl);
        for px in &img.data {
            for &v in px.iter() {
                assert!(v.is_finite() && v >= 0.0);
            }
        }
    }
}

#[test]
fn ranked_sources_empty_near_boundary() {
    let img = synthetic(100, 100);
    let target = (1.0f32, 1.0f32);
    let r = 20.0f32;
    let ranked = spots::ranked_sources(&img, target, r);
    assert!(ranked.is_empty(), "expected empty, got {ranked:?}");
}

#[test]
fn ranked_sources_returns_sorted_candidates() {
    let img = synthetic(200, 200);
    let target = (100.0f32, 100.0f32);
    let r = 20.0f32;
    let ranked = spots::ranked_sources(&img, target, r);
    assert!(!ranked.is_empty());
    assert!(ranked.len() <= 48);
    // Ensure deterministic order and that first is different from second
    let first = ranked[0];
    assert_ne!(first, ranked[1]);
    // auto_source should return the first candidate
    let auto = spots::auto_source(&img, target, r);
    assert_eq!(auto, first);
}

#[test]
fn auto_source_fallback_when_no_candidates() {
    let img = synthetic(100, 100);
    let target = (1.0f32, 1.0f32);
    let r = 20.0f32;
    let auto = spots::auto_source(&img, target, r);
    assert_eq!(auto, (r * 2.5, 0.0));
}

#[test]
fn pick_source_returns_none_for_empty_spot_points() {
    let src = synthetic(100, 100);
    let info = lightcraft_pipeline::SourceInfo::default();
    let settings = DevelopSettings::default();
    let spot = make_spot(SpotMode::Heal, vec![], 0.1, 0.0, None);
    let res = spots::pick_source(&src, &info, &settings, &spot, None);
    assert!(res.is_none());
}

#[test]
fn pick_source_returns_none_when_no_candidates_fit() {
    // Small image (1x1) so rings go out of bounds
    let src = Rgb32f::from_fn(1, 1, |_, _| [0.5; 3]);
    let info = lightcraft_pipeline::SourceInfo::default();
    let settings = DevelopSettings::default();
    let spot = make_spot(SpotMode::Heal, vec![Point::new(0.5, 0.5)], 0.5, 0.0, None);
    let res = spots::pick_source(&src, &info, &settings, &spot, None);
    assert!(res.is_none());
}

#[test]
fn pick_source_respects_avoid() {
    let src = synthetic(300, 300);
    let info = lightcraft_pipeline::SourceInfo::default();
    let settings = DevelopSettings::default();
    let spot = make_spot(SpotMode::Heal, vec![Point::new(0.5, 0.5)], 0.1, 0.0, None);
    let first = spots::pick_source(&src, &info, &settings, &spot, None).expect("pick_source should return a point");
    // If we avoid the first, we should get a different point (unless only one candidate)
    let second = spots::pick_source(&src, &info, &settings, &spot, Some(first));
    if let Some(second) = second {
        assert_ne!(first, second, "avoid should change source if possible");
    }
}

#[test]
fn apply_timed_matches_apply_and_populates_stages() {
    let mut img_a = synthetic(200, 150);
    let mut img_b = img_a.clone();
    let frame = Frame::new(200, 150, &DevelopSettings::default(), true);
    let ppl = frame.px_per_long(200);
    let spot_a = make_spot(SpotMode::Heal, vec![Point::new(0.5, 0.5)], 0.05, 20.0, None);
    let spot_b = spot_a.clone();

    spots::apply(&mut img_a, &[spot_a], &frame, ppl);

    let mut stages = Stages::default();
    spots::apply_timed(&mut img_b, &[spot_b], &frame, ppl, Some(&mut stages));

    assert_eq!(img_a.data, img_b.data, "apply and apply_timed differ");
    // Auto source search should take some measurable time (usually >0)
    assert!(stages.search > Duration::ZERO, "search stage should be timed");
    assert!(stages.feather > Duration::ZERO, "feather stage should be timed");
    assert!(stages.solve > Duration::ZERO, "solve stage should be timed");
    assert!(stages.render > Duration::ZERO, "render stage should be timed");
}

#[test]
fn stages_default_zero() {
    let s = Stages::default();
    assert_eq!(s.search, Duration::ZERO);
    assert_eq!(s.feather, Duration::ZERO);
    assert_eq!(s.solve, Duration::ZERO);
    assert_eq!(s.render, Duration::ZERO);
}

#[test]
fn output_finite_and_non_negative_after_heal() {
    let mut img = image_with_dot(120, 80, 8);
    let frame = Frame::new(120, 80, &DevelopSettings::default(), true);
    let ppl = frame.px_per_long(120);
    let spot = make_spot(SpotMode::Heal, vec![Point::new(0.5, 0.5)], 8.0 / 120.0, 30.0, None);
    spots::apply(&mut img, &[spot], &frame, ppl);
    for px in &img.data {
        for &v in px.iter() {
            assert!(v.is_finite() && v >= 0.0, "pixel {v} not finite/non-negative");
        }
    }
}

#[test]
fn determinism_repeated_apply() {
    let base = synthetic(120, 90);
    let frame = Frame::new(120, 90, &DevelopSettings::default(), true);
    let ppl = frame.px_per_long(120);
    let spots_vec = vec![
        make_spot(SpotMode::Heal, vec![Point::new(0.3, 0.4)], 0.05, 20.0, None),
        make_spot(SpotMode::Clone, vec![Point::new(0.7, 0.7)], 0.06, 0.0, Some(Point::new(-0.1, 0.0))),
    ];
    let mut img1 = base.clone();
    let mut img2 = base.clone();
    spots::apply(&mut img1, &spots_vec, &frame, ppl);
    spots::apply(&mut img2, &spots_vec, &frame, ppl);
    assert_eq!(img1.data, img2.data);
}

#[test]
fn clone_with_opacity_zero_leaves_image_unchanged() {
    let mut img = synthetic(80, 60);
    let original = img.clone();
    let frame = Frame::new(80, 60, &DevelopSettings::default(), true);
    let ppl = frame.px_per_long(80);
    let spot = make_spot(SpotMode::Clone, vec![Point::new(0.5, 0.5)], 0.1, 0.0, Some(Point::new(0.2, 0.0)));
    let mut spot_zero = spot.clone();
    spot_zero.opacity = 0.0;
    spots::apply(&mut img, &[spot_zero], &frame, ppl);
    assert_eq!(img.data, original.data);
}

#[test]
fn clone_with_full_opacity_and_no_feather_copies_exact() {
    let mut img = half_half(100, 100);
    let frame = Frame::new(100, 100, &DevelopSettings::default(), true);
    let ppl = frame.px_per_long(100);
    let spot = make_spot(SpotMode::Clone, vec![Point::new(0.25, 0.5)], 0.05, 0.0, Some(Point::new(0.5, 0.0)));
    spots::apply(&mut img, &[spot], &frame, ppl);
    // The target area should exactly equal the source (no feather)
    let src_val = img.get(75, 50)[0];
    let dst_val = img.get(25, 50)[0];
    assert!((dst_val - src_val).abs() < 1e-6);
}

#[test]
fn source_ranking_is_deterministic() {
    let img = synthetic(128, 128);
    let target = (64.0f32, 64.0f32);
    let r = 15.0f32;
    let a = spots::ranked_sources(&img, target, r);
    let b = spots::ranked_sources(&img, target, r);
    assert_eq!(a, b);
}
