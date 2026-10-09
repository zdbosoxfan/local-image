use photocraft_paint::render::{CoverageMap, mask_combine};
use photocraft_paint::{BrushContext, BrushSettings, MaskMode, Stroke, StrokePoint, StrokeRenderer, dabs, grid_center, grid_square};

#[test]
fn grid_center_odd_even_diameter_snap() {
    assert_eq!(grid_center(3.0, 7.0, 1.0), (3.5, 7.5));
    assert_eq!(grid_center(3.0, 7.0, 2.0), (3.0, 7.0));
    assert_eq!(grid_center(3.2, 7.8, 2.0), (3.0, 8.0));
    assert_eq!(grid_center(3.2, 7.8, 1.0), (3.5, 7.5));
    assert_eq!(grid_center(10.0, 10.0, f32::NAN), (10.5, 10.5));
    assert_eq!(grid_center(10.0, 10.0, f32::INFINITY), (10.5, 10.5));
}

#[test]
fn grid_square_uses_diameter_parity_and_clamps() {
    assert_eq!(grid_square(10.0, 10.0, 1.0), [10.0, 10.0, 11.0, 11.0]);
    assert_eq!(grid_square(10.0, 10.0, 2.0), [9.0, 9.0, 11.0, 11.0]);
    assert_eq!(grid_square(10.0, 10.0, 0.0), [10.0, 10.0, 11.0, 11.0]);
    assert_eq!(grid_square(10.0, 10.0, f32::NAN), [10.0, 10.0, 11.0, 11.0]);

    let big = grid_square(0.0, 0.0, 1_000_000.0_f32);
    assert_eq!(big[3] - big[1], 100_000.0);
    assert!(big.iter().all(|v| v.is_finite()));
}

#[test]
fn mask_combine_zero_coverage_is_zero_for_all_modes() {
    let modes = [
        MaskMode::Multiply,
        MaskMode::Subtract,
        MaskMode::Darken,
        MaskMode::Overlay,
        MaskMode::ColorDodge,
        MaskMode::ColorBurn,
        MaskMode::LinearBurn,
        MaskMode::HardMix,
        MaskMode::LinearHeight,
        MaskMode::Height,
    ];

    for mode in modes {
        assert_eq!(mask_combine(mode, 0.0, 0.5, 0.5), 0.0);
    }
}

#[test]
fn mask_combine_basic_formulas() {
    let (v, t, d) = (0.9, 0.2, 0.6);

    let multiply = mask_combine(MaskMode::Multiply, v, t, d);
    assert!((multiply - 0.468).abs() < 1e-6);

    let subtract = mask_combine(MaskMode::Subtract, v, t, d);
    assert!((subtract - 0.42).abs() < 1e-6);

    let darken = mask_combine(MaskMode::Darken, v, t, d);
    assert!((darken - 0.52).abs() < 1e-6);
}

#[test]
fn mask_combine_results_are_finite_and_clamped() {
    let modes = [
        MaskMode::Multiply,
        MaskMode::Subtract,
        MaskMode::Darken,
        MaskMode::Overlay,
        MaskMode::ColorDodge,
        MaskMode::ColorBurn,
        MaskMode::LinearBurn,
        MaskMode::HardMix,
        MaskMode::LinearHeight,
        MaskMode::Height,
    ];

    for mode in modes {
        for (v, t, d) in [
            (0.9, 0.2, 0.6),
            (1.0, 0.0, 1.0),
            (0.5, 0.5, 0.5),
            (0.1, 0.9, 0.9),
            (0.0, 0.0, 0.0),
            (1.0, 1.0, 1.0),
            (0.7, 0.3, 0.0),
            (0.3, 0.7, 1.0),
            (0.8, 0.8, 0.8),
        ] {
            let r = mask_combine(mode, v, t, d);
            assert!(r.is_finite());
            assert!((0.0..=1.0).contains(&r));
        }
    }
}

#[test]
fn brush_context_default_texture_at_is_one() {
    let brush = BrushSettings { size: 10.0, hardness: 1.0, ..Default::default() };
    let ctx = BrushContext::new(&brush);
    assert_eq!(ctx.texture_at(3, 7), 1.0);
}

#[test]
fn brush_context_dab_rect_contains_dab_center() {
    let brush = BrushSettings { size: 10.0, hardness: 1.0, ..Default::default() };
    let stroke = Stroke { brush: brush.clone(), points: vec![StrokePoint::new(5.0, 5.0, 1.0)] };
    let dab = dabs(&stroke)[0];
    let ctx = BrushContext::new(&brush);
    let rect = ctx.dab_rect(&dab, false);

    assert!(!rect.is_empty());
    assert!(rect.width() >= 2);
    assert!(rect.height() >= 2);

    let cx = dab.center.x as i32;
    let cy = dab.center.y as i32;
    assert!(cx >= rect.x0 && cx < rect.x1);
    assert!(cy >= rect.y0 && cy < rect.y1);
}

#[test]
fn brush_context_rasterize_round_dab_covers_center_finite() {
    let brush = BrushSettings { size: 10.0, hardness: 1.0, ..Default::default() };
    let stroke = Stroke { brush: brush.clone(), points: vec![StrokePoint::new(5.0, 5.0, 1.0)] };
    let dab = dabs(&stroke)[0];
    let ctx = BrushContext::new(&brush);
    let rect = ctx.dab_rect(&dab, false);
    let mut out = Vec::new();

    ctx.rasterize(&dab, false, rect, &mut out);

    assert_eq!(out.len(), (rect.width() * rect.height()) as usize);
    assert!(out.iter().all(|v| v.is_finite()));

    let max_val = out.iter().copied().fold(0.0f32, f32::max);
    assert!(max_val > 0.0);
    assert!(max_val <= dab.alpha + 1e-6);
}

#[test]
fn coverage_map_default_zero_and_empty_bounds() {
    let cm = CoverageMap::new(0);
    assert_eq!(cm.get(10, 10), 0.0);
    assert_eq!(cm.get(-1, -1), 0.0);
    assert!(cm.bounds().is_empty());
}

#[test]
fn coverage_map_accumulate_updates_bounds_and_coverage() {
    let brush = BrushSettings { size: 10.0, hardness: 1.0, ..Default::default() };
    let stroke = Stroke { brush: brush.clone(), points: vec![StrokePoint::new(5.0, 5.0, 1.0)] };
    let dab = dabs(&stroke)[0];
    let ctx = BrushContext::new(&brush);
    let rect = ctx.dab_rect(&dab, false);
    let mut vals = Vec::new();
    ctx.rasterize(&dab, false, rect, &mut vals);

    let mut cm = CoverageMap::new(0);
    cm.accumulate(rect, &vals, 1.0, false, None);

    assert!(!cm.bounds().is_empty());
    let cx = dab.center.x.floor() as i32;
    let cy = dab.center.y.floor() as i32;
    assert!(cm.get(cx, cy) > 0.0);
}

#[test]
fn coverage_map_accumulate_flow_ceiling_builds_up() {
    let brush = BrushSettings { size: 10.0, hardness: 1.0, ..Default::default() };
    let stroke = Stroke { brush: brush.clone(), points: vec![StrokePoint::new(5.0, 5.0, 1.0)] };
    let dab = dabs(&stroke)[0];
    let ctx = BrushContext::new(&brush);
    let rect = ctx.dab_rect(&dab, false);
    let area = (rect.width() * rect.height()) as usize;
    let half = vec![0.5; area];
    let full = vec![1.0; area];

    let mut cm = CoverageMap::new(0);
    cm.accumulate(rect, &half, 1.0, false, None);
    let px = rect.x0;
    let py = rect.y0;
    assert!((cm.get(px, py) - 0.5).abs() < 1e-6);

    cm.accumulate(rect, &full, 1.0, false, None);
    assert!((cm.get(px, py) - 1.0).abs() < 1e-6);
}

#[test]
fn stroke_renderer_empty_points_finish_is_noop() {
    let brush = BrushSettings { size: 10.0, hardness: 1.0, ..Default::default() };
    let mut r = StrokeRenderer::new(&brush, None, 1.0);

    r.finish();

    assert_eq!(r.dab_count(), 0);
    assert!(r.bounds().is_empty());
    let (b, cov) = r.dense_coverage();
    assert!(b.is_empty());
    assert!(cov.is_empty());
}

#[test]
fn stroke_renderer_single_point_paints() {
    let brush = BrushSettings { size: 10.0, hardness: 1.0, ..Default::default() };
    let pts = vec![StrokePoint::new(5.0, 5.0, 1.0)];
    let mut r = StrokeRenderer::new(&brush, None, 1.0);

    r.push(&pts);
    r.finish();

    assert!(r.dab_count() > 0);
    assert!(!r.bounds().is_empty());
    assert!(r.coverage_at(5, 5) > 0.0);
}

#[test]
fn stroke_renderer_deterministic_dense_coverage() {
    let brush = BrushSettings { size: 10.0, hardness: 1.0, spacing: 0.5, ..Default::default() };
    let pts: Vec<StrokePoint> = (0..10).map(|i| StrokePoint::new(i as f64, (i % 2) as f64, 1.0)).collect();

    let mut a = StrokeRenderer::new(&brush, None, 1.0);
    let mut b = StrokeRenderer::new(&brush, None, 1.0);

    a.push(&pts);
    b.push(&pts);
    a.finish();
    b.finish();

    assert_eq!(a.dense_coverage(), b.dense_coverage());
}

#[test]
fn stroke_renderer_chunked_push_matches_single_push() {
    let brush = BrushSettings { size: 8.0, hardness: 1.0, spacing: 0.5, ..Default::default() };
    let pts: Vec<StrokePoint> = (0..20).map(|i| StrokePoint::new(i as f64, (i % 3) as f64, (i as f32 / 20.0).max(0.1))).collect();

    let mut a = StrokeRenderer::new(&brush, None, 1.0);
    for chunk in pts.chunks(3) {
        a.push(chunk);
    }
    a.finish();

    let mut b = StrokeRenderer::new(&brush, None, 1.0);
    b.push(&pts);
    b.finish();

    assert_eq!(a.dense_coverage(), b.dense_coverage());
}

#[test]
fn stroke_renderer_record_dabs_matches_dab_count() {
    let brush = BrushSettings { size: 10.0, hardness: 1.0, spacing: 0.5, ..Default::default() };
    let pts = vec![StrokePoint::new(0.0, 0.0, 1.0), StrokePoint::new(10.0, 0.0, 1.0)];

    let mut r = StrokeRenderer::new(&brush, None, 1.0).record_dabs();
    r.push(&pts);
    r.finish();

    assert_eq!(r.dabs().len(), r.dab_count());
    assert!(!r.dabs().is_empty());
}

#[test]
fn stroke_renderer_finish_is_idempotent() {
    let brush = BrushSettings { size: 10.0, hardness: 1.0, spacing: 0.5, ..Default::default() };
    let pts = vec![StrokePoint::new(0.0, 0.0, 1.0), StrokePoint::new(10.0, 10.0, 0.8)];

    let mut r = StrokeRenderer::new(&brush, None, 1.0);
    r.push(&pts);
    r.finish();

    let count = r.dab_count();
    let dense = r.dense_coverage();

    r.finish();

    assert_eq!(r.dab_count(), count);
    assert_eq!(r.dense_coverage(), dense);
}
