//! Analytic cases adapted from VectorCraft crates/brush/src/tests.rs at
//! d522c1d7be4035bd4f4a84cd6ebfca44f5155092, with original independent oracles.
//! Copyright (c) 2026 ArtCraft Team and the VectorCraft contributors. MIT OR Apache-2.0.
//! See licenses/vectorcraft-{LICENSE-MIT,LICENSE-APACHE,NOTICE}.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#![expect(clippy::allow_attributes_without_reason, reason = "baseline 2026-10-25: unreviewed")]

use photocraft_doc::vector_design::appearance::{StrokeItem, WidthPoint, WidthProfile};
use photocraft_doc::{Path, ShapeStroke, Subpath};
use photocraft_geom::Point;
use photocraft_vecfx::brush::*;
fn line(points: &[(f64, f64)]) -> Path {
    Path::new(vec![Subpath::polyline(points)])
}
fn rect(w: f64, h: f64) -> Path {
    Path::new(vec![Subpath::polygon(&[(0., -h / 2.), (w, -h / 2.), (w, h / 2.), (0., h / 2.)])])
}
fn stroke(brush: Brush, seed: u64, width: f32) -> StrokeItem {
    StrokeItem { base: ShapeStroke { width, ..Default::default() }, brush: Some(Resource { brush, seed }.reference().unwrap()), ..Default::default() }
}
fn bounds(p: &Path) -> (f64, f64, f64, f64) {
    photocraft_pathops::bounds(p).unwrap().unwrap()
}
#[test]
fn brush17_calligraphic_width_has_independent_ellipse_and_flat_nib_oracles() {
    for (angle, roundness, expected) in [(0., 0.2, 2.), (90., 0.2, 10.), (45., 0.2, 52_f64.sqrt()), (30., 0., 5.)] {
        let s = stroke(Brush::Calligraphic(Calligraphic { angle, roundness, size: 1. }), 0, 10.);
        let p = project(&line(&[(0., 0.), (100., 0.)]), &s).unwrap();
        let b = bounds(&p[0]);
        assert!((b.3 - b.1 - expected).abs() < 0.02, "{b:?}: {expected}");
        assert!(photocraft_pathops::contains(&p[0], Point::new(50., expected * 0.49)).unwrap());
        assert!(!photocraft_pathops::contains(&p[0], Point::new(50., expected * 0.51)).unwrap());
    }
}
#[test]
fn brush17_pressure_uses_arc_length_and_zero_width_lens_area() {
    let samples = [(Point::new(0., 0.), 0.), (Point::new(20., 0.), 1.), (Point::new(100., 0.), 0.)];
    let profile = pressure_profile(&samples, 20.).unwrap();
    assert_eq!(profile.points[1].t, 0.2);
    let mut s = stroke(Brush::Calligraphic(Calligraphic { angle: 0., roundness: 1., size: 1. }), 0, 20.);
    s.profile = Some(profile);
    let p = project(&line(&[(0., 0.), (100., 0.)]), &s).unwrap();
    assert!((photocraft_pathops::area(&p[0]).unwrap() - 1000.).abs() < 0.01);
    assert!(photocraft_pathops::contains(&p[0], Point::new(20., 9.9)).unwrap());
    assert!(!photocraft_pathops::contains(&p[0], Point::new(20., 10.1)).unwrap());
}
#[test]
fn brush17_pattern_straight_spacing_and_fit_modes_are_exact() {
    for (fit, length, count, spacing) in
        [(PatternFit::Stretch, 104., 10, 10.4), (PatternFit::AddSpace, 109., 10, 11.), (PatternFit::Approximate, 100., 10, 10.)]
    {
        let p = PatternBrush { side: rect(10., 4.), spacing: 0., fit, ..Default::default() };
        let out = project(&line(&[(0., 50.), (length, 50.)]), &stroke(Brush::Pattern(p), 0, 1.)).unwrap();
        assert_eq!(out.len(), count);
        for (i, p) in out.iter().enumerate() {
            let b = bounds(p);
            assert!((b.0 - i as f64 * spacing).abs() < 1e-6);
            assert!((b.3 - b.1 - 4.).abs() < 1e-6);
        }
        assert!((bounds(&out[count - 1]).2 - length).abs() < 1e-6);
    }
}
#[test]
fn brush17_pattern_corners_and_ends_reserve_tile_lengths() {
    let p = PatternBrush { side: rect(10., 4.), inner_corner: Some(rect(6., 6.)), outer_corner: Some(rect(6., 6.)), spacing: 0., ..Default::default() };
    let square = Path::new(vec![Subpath::polygon(&[(0., 0.), (100., 0.), (100., 100.), (0., 100.)])]);
    let out = project(&square, &stroke(Brush::Pattern(p), 0, 1.)).unwrap();
    assert_eq!(out.len(), 40);
    let p = PatternBrush { side: rect(10., 4.), start: Some(rect(5., 2.)), end: Some(rect(5., 2.)), spacing: 0., ..Default::default() };
    let out = project(&line(&[(0., 0.), (100., 0.)]), &stroke(Brush::Pattern(p), 0, 1.)).unwrap();
    assert_eq!(out.len(), 11);
    assert_eq!(bounds(&out[0]).0, 0.);
    assert_eq!(bounds(&out[1]).2, 100.);
}
#[test]
fn brush17_art_stretch_proportions_guides_and_corner_mapping() {
    for (scale, width, height) in [(ArtScale::Stretch, 2., 8.), (ArtScale::Proportional, 1., 80.), (ArtScale::BetweenGuides { start: 0.2, end: 0.8 }, 1., 4.)] {
        let a = ArtBrush { art: rect(10., 4.), scale, width, ..Default::default() };
        let p = project(&line(&[(20., 50.), (220., 50.)]), &stroke(Brush::Art(a), 0, 1.)).unwrap();
        let b = bounds(&p[0]);
        assert!((b.0 - 20.).abs() < 1e-6 && (b.2 - 220.).abs() < 1e-6);
        assert!((b.3 - b.1 - height).abs() < 1e-6);
    }
    let a = ArtBrush { art: rect(10., 2.), ..Default::default() };
    let p = project(&line(&[(0., 0.), (37.25, 0.), (37.25, 40.)]), &stroke(Brush::Art(a), 0, 1.)).unwrap();
    assert!(photocraft_pathops::contains(&p[0], Point::new(20., 0.)).unwrap());
    assert!(photocraft_pathops::contains(&p[0], Point::new(37.25, 20.)).unwrap());
    assert!(!photocraft_pathops::contains(&p[0], Point::new(20., 20.)).unwrap());
}
#[test]
fn brush17_art_tight_folds_are_finite_bounded_and_normalized() {
    let a = ArtBrush { art: rect(10., 4.), ..Default::default() };
    for pts in [&[(0., 0.), (20., 0.), (0.1, 0.01)][..], &[(0., 0.), (0., 0.), (10., 0.), (10., 1.), (0., 1.)][..]] {
        let paths = project(&line(pts), &stroke(Brush::Art(a.clone()), 0, 2.)).unwrap();
        for p in paths {
            assert!(photocraft_pathops::area(&p).unwrap() > 0.);
            assert!(p.subpaths.iter().all(|s| s.closed));
            assert!(p.subpaths.iter().flat_map(|s| &s.knots).all(|k| k.anchor.x.is_finite() && k.anchor.y.is_finite()));
        }
    }
}
#[test]
fn brush17_scatter_is_byte_identical_across_threads_and_seed_changes_it() {
    let s = stroke(Brush::Scatter(Scatter { art: rect(4., 4.), ..Default::default() }), 7, 1.);
    let path = line(&[(0., 0.), (300., 0.)]);
    let expected = serde_json::to_vec(&project(&path, &s).unwrap()).unwrap();
    let handles: Vec<_> = (0..4)
        .map(|_| {
            let p = path.clone();
            let s = s.clone();
            std::thread::spawn(move || serde_json::to_vec(&project(&p, &s).unwrap()).unwrap())
        })
        .collect();
    for h in handles {
        assert_eq!(h.join().unwrap(), expected);
    }
    let changed = stroke(Brush::Scatter(Scatter { art: rect(4., 4.), ..Default::default() }), 8, 1.);
    assert_ne!(expected, serde_json::to_vec(&project(&path, &changed).unwrap()).unwrap());
}
#[test]
fn brush17_degenerate_hostile_and_overbudget_inputs_return_errors() {
    let s = stroke(Brush::Calligraphic(Default::default()), 0, 10.);
    assert_eq!(project(&line(&[(1., 1.), (1., 1.)]), &s), Err(Error::Degenerate));
    assert_eq!(project(&line(&[(f64::NAN, 0.), (10., 0.)]), &s), Err(Error::Invalid));
    let huge = stroke(Brush::Pattern(PatternBrush { side: rect(1., 1.), scale: 0.01, spacing: 0., ..Default::default() }), 0, 1.);
    assert_eq!(project(&line(&[(0., 0.), (100_000., 0.)]), &huge), Err(Error::Budget));
    assert_eq!(pressure_profile(&[(Point::new(1., 1.), 1.), (Point::new(1., 1.), 1.)], 10.), Err(Error::Degenerate));
    let mut bad = s;
    bad.profile = Some(WidthProfile { points: vec![WidthPoint { t: 0., left: 1., right: 1. }, WidthPoint { t: 0., left: 1., right: 1. }] });
    assert_eq!(project(&line(&[(0., 0.), (10., 0.)]), &bad), Err(Error::Invalid));
}
#[test]
fn brush17_versioned_native_references_keep_resources_and_reject_unknowns() {
    for (_, brush) in presets() {
        let r = Resource { brush, seed: 42 };
        let reference = r.reference().unwrap();
        let stored = serde_json::to_string(&reference).unwrap();
        let reference = serde_json::from_str(&stored).unwrap();
        assert_eq!(Resource::resolve(&reference).unwrap(), r);
    }
    assert_eq!(Resource::resolve(&photocraft_doc::vector_design::appearance::BrushRef { id: "unknown".into() }), Err(Error::Reference));
    let old: Calligraphic = serde_json::from_str("{}").unwrap();
    assert_eq!(old, Calligraphic::default());
}
