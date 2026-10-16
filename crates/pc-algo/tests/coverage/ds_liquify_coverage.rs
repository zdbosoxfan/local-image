use photocraft_algo::liquify::{LiquifyField, LiquifyStroke, LiquifyTool, ProxyImage, apply_liquify, auto_cell};
use photocraft_color::{ColorMode, PixelFormat, SampleType};
use photocraft_geom::Rect;
use photocraft_raster::Surface;

fn rgb_surface(width: i32, height: i32) -> Surface {
    let mut s = Surface::new(PixelFormat::new(ColorMode::Rgb, SampleType::U8, true));
    for y in 0..height {
        for x in 0..width {
            let v = ((x / 4 + y / 4) % 2) as f32;
            s.write_pixel(x, y, &[v, (x as f32) / width as f32, (y as f32) / height as f32, 1.0]);
        }
    }
    s
}

fn bounds(w: i32, h: i32) -> Rect {
    Rect::new(0, 0, w, h)
}

fn stroke(tool: LiquifyTool, size: f64, points: Vec<Vec<f64>>) -> LiquifyStroke {
    let mut s = LiquifyStroke::new(tool, size);
    s.points = points;
    s
}

#[test]
fn tool_all_and_helpers() {
    assert_eq!(LiquifyTool::ALL.len(), 11);
    for tool in LiquifyTool::ALL {
        assert!(!tool.label().is_empty());
    }
    assert!(!LiquifyTool::ForwardWarp.is_global());
    assert!(LiquifyTool::ReconstructAll.is_global());
    assert!(!LiquifyTool::ForwardWarp.is_stationary());
    assert!(LiquifyTool::TwirlCw.is_stationary());
}

#[test]
fn auto_cell_selects_2_or_4_by_area() {
    let small = bounds(1000, 1000); // area 1_000_000 <= 4_200_000
    assert_eq!(auto_cell(small), 2.0);
    let large = bounds(3000, 2000); // area 6_000_000 > 4_200_000
    assert_eq!(auto_cell(large), 4.0);
    assert_eq!(auto_cell(Rect::EMPTY), 2.0);
}

#[test]
fn field_new_clamps_cell_and_sets_dimensions() {
    let b = bounds(10, 8);
    let f = LiquifyField::new(b, 0.5); // clamps to 1.0
    assert_eq!(f.cell, 1.0);
    assert_eq!((f.w, f.h), (12, 10)); // ceil(10/1)+2, ceil(8/1)+2
    assert!(f.is_identity());
    assert_eq!(f.d.len(), f.w * f.h);
    assert_eq!(f.freeze.len(), f.w * f.h);

    let f2 = LiquifyField::new(b, 100.0); // clamps to 64
    assert_eq!(f2.cell, 64.0);
    assert_eq!((f2.w, f2.h), (3, 3)); // ceil(10/64)+2 = 1+2, ceil(8/64)+2 = 1+2
}

#[test]
fn node_pos_uses_pixel_centers() {
    let b = Rect::new(5, 5, 10, 10); // width=5, height=5
    let f = LiquifyField::new(b, 2.0);
    let p00 = f.node_pos(0, 0);
    assert!((p00[0] - 5.5).abs() < 1e-12);
    assert!((p00[1] - 5.5).abs() < 1e-12);
    let p12 = f.node_pos(1, 2);
    assert!((p12[0] - (5.5 + 2.0)).abs() < 1e-12);
    assert!((p12[1] - (5.5 + 4.0)).abs() < 1e-12);
}

#[test]
fn sample_outside_returns_zero_and_freeze_clamps_to_edge() {
    let b = bounds(8, 8);
    let mut f = LiquifyField::new(b, 2.0);
    f.d[f.w + 1] = [3.0, -2.0];
    let p = f.node_pos(1, 1);
    let d = f.sample(p[0], p[1]);
    assert_eq!(d, [3.0, -2.0]);
    let d_out = f.sample(-100.0, -100.0);
    assert_eq!(d_out, [0.0, 0.0]);

    f.freeze[0] = 1.0;
    assert_eq!(f.freeze_at(-100.0, -100.0), 1.0);
    assert_eq!(f.freeze_at(1000.0, 1000.0), 0.0);
}

#[test]
fn apply_stroke_no_points_returns_empty_and_unchanged() {
    let mut f = LiquifyField::new(bounds(20, 20), 2.0);
    let before = f.clone();
    let s = stroke(LiquifyTool::ForwardWarp, 10.0, vec![]);
    let dirty = f.apply_stroke(&s);
    assert_eq!(dirty, Rect::EMPTY);
    assert_eq!(f, before);
}

#[test]
fn forward_warp_stroke_displaces_along_direction() {
    let mut f = LiquifyField::new(bounds(40, 40), 2.0);
    let mut s = stroke(LiquifyTool::ForwardWarp, 10.0, vec![vec![10.0, 20.0], vec![30.0, 20.0]]);
    s.pressure = 100.0;
    let dirty = f.apply_stroke(&s);
    assert!(dirty.width() > 0 && dirty.height() > 0);
    let d = f.sample(30.0, 20.0);
    assert!(d[0] < 0.0, "expected negative x displacement, got {:?}", d);
    assert!(d[1].abs() < 0.5);
}

#[test]
fn freeze_and_thaw_change_mask() {
    let mut f = LiquifyField::new(bounds(30, 30), 2.0);
    let mut fr = stroke(LiquifyTool::Freeze, 10.0, vec![vec![15.0, 15.0]]);
    fr.density = 0.0; // hard brush edge
    f.apply_stroke(&fr);
    let center_freeze = f.freeze_at(15.0, 15.0);
    assert!(center_freeze > 0.9);
    let mut th = fr.clone();
    th.tool = LiquifyTool::Thaw;
    f.apply_stroke(&th);
    assert!(f.freeze_at(15.0, 15.0) < 0.1);
}

#[test]
fn reconstruct_all_restores_identity() {
    let mut f = LiquifyField::new(bounds(30, 30), 2.0);
    let s = stroke(LiquifyTool::ForwardWarp, 10.0, vec![vec![10.0, 10.0], vec![20.0, 10.0]]);
    f.apply_stroke(&s);
    assert!(!f.is_identity());
    let mut rec = LiquifyStroke::new(LiquifyTool::ReconstructAll, 1.0);
    rec.amount = Some(100.0);
    f.apply_stroke(&rec);
    assert!(f.is_identity());
}

#[test]
fn invert_freeze_global_flips_mask() {
    let mut f = LiquifyField::new(bounds(20, 20), 2.0);
    f.apply_stroke(&LiquifyStroke::new(LiquifyTool::FreezeAll, 1.0));
    assert!(f.freeze.iter().all(|&v| v == 1.0));
    f.apply_stroke(&LiquifyStroke::new(LiquifyTool::InvertFreeze, 1.0));
    assert!(f.freeze.iter().all(|&v| v == 0.0));
}

#[test]
fn stroke_validate_checks_range_and_points() {
    let mut s = LiquifyStroke::new(LiquifyTool::ForwardWarp, 10.0);
    s.points = vec![vec![1.0, 2.0], vec![3.0, 4.0, 0.5]];
    assert_eq!(s.validate(), Ok(()));
    s.size = 0.5;
    assert!(s.validate().is_err());
    s.size = 15001.0;
    assert!(s.validate().is_err());
    s.size = 10.0;
    s.points = vec![vec![1.0]];
    assert!(s.validate().is_err());
    s.points = vec![vec![1.0, 2.0, 3.0, 4.0]];
    assert!(s.validate().is_err());
    s.points = vec![vec![f64::NAN, 2.0]];
    assert!(s.validate().is_err());
    s.points = vec![vec![1.0, f64::INFINITY]];
    assert!(s.validate().is_err());
}

#[test]
fn apply_liquify_identity_is_noop() {
    let src = rgb_surface(32, 32);
    let f = LiquifyField::new(bounds(32, 32), 2.0);
    let out = apply_liquify(&src, &f);
    assert_eq!(out, src);
}

#[test]
fn apply_liquify_changes_only_inside_bounds() {
    let src = rgb_surface(40, 40);
    let mut f = LiquifyField::new(bounds(20, 20), 2.0);
    let s = stroke(LiquifyTool::Pucker, 10.0, vec![vec![10.0, 10.0]; 5]);
    f.apply_stroke(&s);
    let out = apply_liquify(&src, &f);

    for y in 0..40 {
        for x in 0..40 {
            if x >= 20 || y >= 20 {
                let orig = src.pixel(x, y);
                let new = out.pixel(x, y);
                assert_eq!(orig, new, "pixel ({x},{y}) changed outside bounds");
            }
        }
    }

    let mut any_change = false;
    for y in 0..20 {
        for x in 0..20 {
            if src.pixel(x, y) != out.pixel(x, y) {
                any_change = true;
                break;
            }
        }
        if any_change {
            break;
        }
    }
    assert!(any_change, "no pixels changed inside bounds");
}

#[test]
fn from_strokes_replays_deterministically() {
    let s1 = stroke(LiquifyTool::TwirlCw, 15.0, vec![vec![10.0, 10.0], vec![12.0, 12.0, 0.5]]);
    let s2 = s1.clone();
    let f1 = LiquifyField::from_strokes(bounds(30, 30), 2.0, std::slice::from_ref(&s1));
    let f2 = LiquifyField::from_strokes(bounds(30, 30), 2.0, std::slice::from_ref(&s2));
    assert_eq!(f1, f2);

    let mut f3 = LiquifyField::new(bounds(30, 30), 2.0);
    for s in std::slice::from_ref(&s1) {
        f3.apply_stroke(s);
    }
    assert_eq!(f1, f3);
}

#[test]
fn max_displacement_and_identity_manual() {
    let mut f = LiquifyField::new(bounds(10, 10), 2.0);
    f.d[0] = [3.0, 4.0];
    f.d[1] = [-1.0, -1.0];
    assert!(!f.is_identity());
    assert_eq!(f.max_displacement(), 5.0);
    f.d.fill([0.0; 2]);
    assert!(f.is_identity());
    assert_eq!(f.max_displacement(), 0.0);
}

#[test]
fn proxy_image_new_computes_scale_and_size() {
    let src = rgb_surface(40, 30);
    let p = ProxyImage::new(&src, bounds(40, 30), 20);
    assert_eq!(p.scale, 2.0);
    assert_eq!(p.bounds, bounds(40, 30));
    assert_eq!((p.w, p.h), (20, 15));
}

#[test]
fn proxy_rect_maps_document_to_proxy_pixels() {
    let src = rgb_surface(40, 30);
    let p = ProxyImage::new(&src, bounds(40, 30), 20); // scale=2
    let r = p.proxy_rect(Rect::new(10, 8, 20, 18));
    assert_eq!(r, [5, 4, 11, 10]);
}

#[test]
fn proxy_render_identity_field_outputs_proxy_color_for_constant_image() {
    let mut src = Surface::new(PixelFormat::new(ColorMode::Rgb, SampleType::U8, true));
    src.fill_rect(bounds(40, 30), &[0.5, 0.25, 0.75, 1.0]);
    let p = ProxyImage::new(&src, bounds(40, 30), 20);
    let f = LiquifyField::new(bounds(40, 30), 2.0);
    let mut out = vec![[0u8; 4]; p.w * p.h];
    p.render(&f, [0, 0, p.w, p.h], &mut out);
    for y in 0..p.h {
        for x in 0..p.w {
            let o = out[y * p.w + x];
            assert!((o[0] as i32 - 128).abs() <= 1, "R channel off at ({x},{y})");
            assert!((o[1] as i32 - 64).abs() <= 1, "G channel off at ({x},{y})");
            assert!((o[2] as i32 - 191).abs() <= 1, "B channel off at ({x},{y})");
            assert_eq!(o[3], 255, "alpha off at ({x},{y})");
        }
    }
}

#[test]
fn proxy_render_with_field_changes_pixels_and_stays_in_range() {
    let src = rgb_surface(40, 30);
    let p = ProxyImage::new(&src, bounds(40, 30), 20);
    let mut f = LiquifyField::new(bounds(40, 30), 2.0);
    let s = stroke(LiquifyTool::Bloat, 10.0, vec![vec![20.0, 15.0]; 5]);
    f.apply_stroke(&s);

    let mut out = vec![[0u8; 4]; p.w * p.h];
    p.render(&f, [0, 0, p.w, p.h], &mut out);

    let f_id = LiquifyField::new(bounds(40, 30), 2.0);
    let mut identity_out = vec![[0u8; 4]; p.w * p.h];
    p.render(&f_id, [0, 0, p.w, p.h], &mut identity_out);

    let mut diff_count = 0;
    for i in 0..out.len() {
        if out[i] != identity_out[i] {
            diff_count += 1;
        }
    }
    assert!(diff_count > 0, "no pixels changed by non-identity field");
}

#[test]
fn apply_liquify_empty_bounds_returns_clone() {
    let src = rgb_surface(10, 10);
    let f = LiquifyField::new(Rect::EMPTY, 2.0);
    let out = apply_liquify(&src, &f);
    assert_eq!(out, src);
}
