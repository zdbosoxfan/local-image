use photocraft_geom::warp::*;

fn close(a: (f64, f64), b: (f64, f64), eps: f64) -> bool {
    (a.0 - b.0).abs() < eps && (a.1 - b.1).abs() < eps
}

#[test]
fn all_styles_parse_names_and_ids() {
    for style in WarpStyle::all() {
        assert_eq!(WarpStyle::parse(style.id()), Some(style));
        assert_eq!(WarpStyle::parse(style.psd_name()), Some(style));
        assert_eq!(WarpStyle::parse(style.label()), Some(style));

        let spaced_upper = format!("  {}  ", style.label().to_ascii_uppercase());
        assert_eq!(WarpStyle::parse(&spaced_upper), Some(style), "{style:?}");
    }

    assert_eq!(WarpStyle::parse("arc lower"), Some(WarpStyle::ArcLower));
    assert_eq!(WarpStyle::parse("warpArc"), Some(WarpStyle::Arc));
    assert_eq!(WarpStyle::parse("not a style"), None);
}

#[test]
fn style_warp_new_rejects_nonpreset_and_empty() {
    let b = [0.0, 0.0, 100.0, 100.0];

    assert!(StyleWarp::new(WarpStyle::None, 50.0, 0.0, 0.0, false, b).is_none());
    assert!(StyleWarp::new(WarpStyle::Custom, 50.0, 0.0, 0.0, false, b).is_none());

    // Empty/degenerate boxes
    assert!(StyleWarp::new(WarpStyle::Arc, 50.0, 0.0, 0.0, false, [0.0, 0.0, 0.0, 100.0]).is_none());
    assert!(StyleWarp::new(WarpStyle::Arc, 50.0, 0.0, 0.0, false, [0.0, 0.0, -1.0, 100.0]).is_none());

    // Zero warp
    assert!(StyleWarp::new(WarpStyle::Arc, 0.0, 0.0, 0.0, false, b).is_none());

    // Non-finite bend is treated as zero -> None
    assert!(StyleWarp::new(WarpStyle::Arc, f64::NAN, 0.0, 0.0, false, b).is_none());
}

#[test]
fn none_warp_is_identity() {
    let bounds = [5.0, 6.0, 15.0, 16.0];
    let w = Warp::none(bounds);

    assert!(w.is_identity());
    assert!(close(w.map(7.0, 8.0), (7.0, 8.0), 1e-9));
    assert_eq!(w.output_bounds(), bounds);

    let mesh = w.to_mesh(2, 3);
    assert!(mesh.is_valid());
    assert_eq!((mesh.us.len() - 1, mesh.vs.len() - 1), (2, 3));
    let center = mesh.eval(0.5, 0.5);
    assert!(close((center[0], center[1]), (10.0, 11.0), 1e-9));
}

#[test]
fn bezier_mesh_identity_clamps_patch_counts() {
    let bounds = [0.0, 0.0, 10.0, 20.0];

    let m0 = BezierMesh::identity(bounds, 0, 0);
    assert_eq!((m0.nx(), m0.ny()), (4, 4));
    assert!(m0.is_valid());

    let m64 = BezierMesh::identity(bounds, 65, 65);
    assert_eq!((m64.nx(), m64.ny()), (193, 193)); // 3 * 64 + 1
    assert!(m64.is_valid());
}

#[test]
fn remove_split_invalid_indices_return_false() {
    let bounds = [0.0, 0.0, 1.0, 1.0];
    let mut m = BezierMesh::identity(bounds, 1, 1);

    assert!(!m.remove_split_u(0));
    assert!(!m.remove_split_u(m.us.len() - 1));
    assert!(!m.remove_split_v(0));
    assert!(!m.remove_split_v(m.vs.len() - 1));
}

#[test]
fn bezier_mesh_eval_clamps_params_to_01() {
    let bounds = [0.0, 0.0, 10.0, 20.0];
    let m = BezierMesh::identity(bounds, 1, 1);

    let corner = m.eval(0.0, 0.0);
    let outside = m.eval(-0.5, -0.5);
    assert!(close((outside[0], outside[1]), (corner[0], corner[1]), 1e-9));

    let corner1 = m.eval(1.0, 1.0);
    let outside1 = m.eval(1.5, 1.5);
    assert!(close((outside1[0], outside1[1]), (corner1[0], corner1[1]), 1e-9));
}

#[test]
fn bezier_mesh_is_valid_rejects_bad_data() {
    let b = [0.0, 0.0, 1.0, 1.0];
    let good = BezierMesh::identity(b, 1, 1);
    assert!(good.is_valid());

    let mut bad_knots = good.clone();
    bad_knots.us[0] = 0.1;
    assert!(!bad_knots.is_valid());

    let mut bad_knots2 = good.clone();
    let idx = bad_knots2.vs.len() - 1;
    bad_knots2.vs[idx] = 0.9;
    assert!(!bad_knots2.is_valid());

    let mut bad_knots3 = good.clone();
    bad_knots3.us = vec![0.0, 0.5, 0.4, 1.0]; // non-increasing
    assert!(!bad_knots3.is_valid());

    let mut bad_points = good.clone();
    bad_points.points.pop();
    assert!(!bad_points.is_valid());

    let mut bad_nan = good.clone();
    bad_nan.points[0][0] = f64::NAN;
    assert!(!bad_nan.is_valid());
}

#[test]
fn warp_custom_missing_mesh_maps_identity() {
    let bounds = [0.0, 0.0, 10.0, 10.0];
    let w = Warp { style: WarpStyle::Custom, bend: 0.0, h_distort: 0.0, v_distort: 0.0, vertical: false, bounds, mesh: None };

    assert!(w.is_identity());
    assert!(close(w.map(3.0, 4.0), (3.0, 4.0), 1e-9));
}

#[test]
fn warp_with_empty_bounds_maps_identity() {
    // Preset with zero-width bounds
    let p = Warp::preset(WarpStyle::Arc, 50.0, [10.0, 10.0, 10.0, 20.0]);
    assert!(close(p.map(5.0, 5.0), (5.0, 5.0), 1e-9));

    // Custom with degenerate bounds (even with a valid mesh)
    let mesh = BezierMesh::identity([0.0, 0.0, 10.0, 10.0], 1, 1);
    let c = Warp::custom(mesh, [10.0, 10.0, 10.0, 20.0]);
    assert!(close(c.map(5.0, 5.0), (5.0, 5.0), 1e-9));
}

#[test]
fn warp_preset_non_finite_bend_is_identity() {
    let bounds = [0.0, 0.0, 100.0, 100.0];
    for bend in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let w = Warp::preset(WarpStyle::Bulge, bend, bounds);
        assert!(w.is_identity());
        assert!(close(w.map(50.0, 50.0), (50.0, 50.0), 1e-9));
    }
}

#[test]
fn output_bounds_are_finite() {
    let bounds = [10.0, 20.0, 110.0, 70.0];
    for style in WarpStyle::PRESETS {
        let w = Warp::preset(style, 80.0, bounds);
        let ob = w.output_bounds();
        assert!(ob.iter().all(|v| v.is_finite()), "{style:?} produced {ob:?}");

        let sampled = [(10.0, 20.0), (110.0, 20.0), (10.0, 70.0), (110.0, 70.0), (60.0, 45.0)];
        for (x, y) in sampled {
            let (mx, my) = w.map(x, y);
            assert!(mx >= ob[0] && mx <= ob[2] && my >= ob[1] && my <= ob[3], "{style:?}: point ({x},{y}) -> ({mx},{my}) outside {ob:?}");
        }
    }
}

#[test]
fn control_bounds_are_min_max_of_points() {
    let bounds = [0.0, 0.0, 10.0, 10.0];
    let mut m = BezierMesh::identity(bounds, 1, 1);
    m.points[0] = [-1.0, -2.0];
    m.points[15] = [12.0, 13.0];

    let cb = m.control_bounds();
    assert_eq!(cb, [-1.0, -2.0, 12.0, 13.0]);
}
