use photocraft_engine::Session;
use photocraft_engine::lens_cmds::{LENS, RAW, WIDE, apply_to_surface, camera_raw_surface, lens_params, raw_params, wide_params};
use serde_json::json;

fn make_session(depth: u32) -> Session {
    let mut s = Session::new();
    s.execute("file.new", json!({"width": 40, "height": 30, "depth": depth})).unwrap();
    s.execute("layer.new.layer", json!({"name": "test"})).unwrap();
    s.edit("fill", |doc, active| {
        let surf = doc.layer_mut(active.unwrap()).unwrap().surface_mut().unwrap();
        let fmt = surf.format();
        let n = fmt.channels();
        for y in 0..30 {
            for x in 0..40 {
                let mut pixel = vec![0.0f32; n];
                let t = if x % 6 == 0 || y % 6 == 0 { 0.8 } else { 0.4 };
                for c in 0..n {
                    pixel[c] = if fmt.alpha && c == n - 1 { 1.0 } else { t * (1.0 - 0.2 * c as f32) };
                }
                surf.write_pixel(x, y, &pixel);
            }
        }
        Ok(())
    })
    .unwrap();
    s
}

fn active_surface(s: &Session) -> (photocraft_engine::doc::LayerId, photocraft_raster::Surface, photocraft_geom::Rect) {
    let st = s.active().unwrap();
    let id = st.active_layer.unwrap();
    let layer = st.doc.layer(id).unwrap();
    let surf = layer.surface().unwrap().clone();
    let bounds = surf.content_bounds();
    (id, surf, bounds)
}

#[test]
fn lens_params_clamps_and_rejects_invalid() {
    let s = make_session(8);
    let (_, _, bounds) = active_surface(&s);

    let p = json!({
        "distortion": 150,
        "redCyan": -200,
        "vignetteAmount": 120,
        "vignetteMidpoint": -10,
        "scale": 200,
        "edge": "black"
    });
    let lc = lens_params(LENS, &p, bounds, None).unwrap();
    assert_eq!(lc.distortion, 100.0);
    assert_eq!(lc.red_cyan, -100.0);
    assert_eq!(lc.vignette_amount, 100.0);
    assert_eq!(lc.vignette_midpoint, 0.0);
    assert_eq!(lc.scale, 150.0);
    // `edge` field is present and black, but avoid naming private type:
    let edge_debug = format!("{:?}", lc.edge);
    assert!(edge_debug.contains("Color([0.0, 0.0, 0.0, 1.0])"), "unexpected edge: {}", edge_debug);

    assert!(lens_params(LENS, &json!({"edge": "rainbow"}), bounds, None).is_err());
    assert!(lens_params(LENS, &json!({"profile": "zeiss"}), bounds, None).is_err());
}

#[test]
fn lens_params_straighten_calculates_angle() {
    let s = make_session(8);
    let (_, _, bounds) = active_surface(&s);

    let p = json!({"straighten": [[0, 0], [100, 5]]});
    let lc = lens_params(LENS, &p, bounds, None).unwrap();
    let expected = -((5.0f64 / 100.0).atan().to_degrees());
    assert!((lc.angle - expected).abs() < 0.01, "got {} expected {}", lc.angle, expected);
}

#[test]
fn lens_params_autoscale_changes_scale() {
    let s = make_session(8);
    let (_, _, bounds) = active_surface(&s);

    // Use a distortion that requires scale > 1 (auto_scale returns non-100 only if corrections
    // need it). The previous empty params gave 100 because no correction needed scaling.
    let p = json!({"distortion": -40, "autoScale": true});
    let lc = lens_params(LENS, &p, bounds, None).unwrap();
    assert!(lc.scale > 100.0, "scale should be > 100, got {}", lc.scale);
}

#[test]
fn wide_params_clamps_and_validates() {
    let p = json!({
        "scale": 200,
        "cropFactor": 0.05,
        "constraints": [{"a": [10, 10], "b": [20, 20], "orientation": "free"}]
    });
    let wa = wide_params(WIDE, &p, None).unwrap();
    assert_eq!(wa.scale, 150.0);
    assert_eq!(wa.crop_factor, 0.1);

    let bad = json!({"constraints": [{"a": ["x", 0], "b": [0, 0]}]});
    assert!(wide_params(WIDE, &bad, None).is_err());
}

#[test]
fn raw_params_clamps_noise() {
    let p = json!({
        "noiseLuminance": 500,
        "noiseLuminanceDetail": -50,
        "noiseColor": 120,
        "noiseColorDetail": -10
    });
    let cr = raw_params(RAW, &p).unwrap();
    assert_eq!(cr.noise_luminance, 100.0);
    assert_eq!(cr.noise_luminance_detail, 0.0);
    assert_eq!(cr.noise_color, 100.0);
    assert_eq!(cr.noise_color_detail, 0.0);
}

#[test]
fn camera_raw_surface_identity_and_non_identity() {
    let s = make_session(8);
    let (_, surf, bounds) = active_surface(&s);

    let identity = raw_params(RAW, &json!({})).unwrap();
    let out_id = camera_raw_surface(&surf, bounds, &identity);
    assert_eq!(out_id, surf, "identity should return clone");

    let non_id = raw_params(RAW, &json!({"exposure": 1.0, "vibrance": 30})).unwrap();
    let out_non = camera_raw_surface(&surf, bounds, &non_id);
    assert_ne!(out_non, surf, "non-identity should change surface");

    for (x, y) in [(0, 0), (10, 10), (20, 20), (39, 29)] {
        let p = out_non.rgba(x, y);
        for c in 0..4 {
            assert!(p[c].is_finite(), "pixel ({},{}) ch {} not finite", x, y, c);
            assert!(p[c] >= 0.0 && p[c] <= 1.0, "pixel ({},{}) ch {} out of range: {}", x, y, c, p[c]);
        }
    }
}

#[test]
fn apply_to_surface_known_and_unknown_ids() {
    let s = make_session(8);
    let (_, surf, bounds) = active_surface(&s);

    assert!(apply_to_surface(LENS, &json!({}), &surf, bounds).is_some());
    assert!(apply_to_surface(WIDE, &json!({"constraints": []}), &surf, bounds).is_some());
    assert!(apply_to_surface(RAW, &json!({}), &surf, bounds).is_some());
    assert!(apply_to_surface("filter.unknown", &json!({}), &surf, bounds).is_none());
}

#[test]
fn lens_correction_command_default_noop() {
    let mut s = make_session(8);
    let (id, _, _) = active_surface(&s);
    let before = {
        let st = s.active().unwrap();
        st.doc.layer(id).unwrap().surface().unwrap().rgba(20, 15)
    };

    s.execute(
        LENS,
        json!({
            "correctDistortion": false,
            "correctVignette": false,
            "correctCA": false,
            "distortion": 0,
            "redCyan": 0,
            "blueYellow": 0,
            "vignetteAmount": 0,
            "vignetteMidpoint": 50,
            "vertical": 0,
            "horizontal": 0,
            "angle": 0,
            "scale": 100,
            "edge": "transparency"
        }),
    )
    .unwrap();

    let after = {
        let st = s.active().unwrap();
        st.doc.layer(id).unwrap().surface().unwrap().rgba(20, 15)
    };

    for c in 0..4 {
        assert!((before[c] - after[c]).abs() < 1e-5, "channel {} changed", c);
    }
}

#[test]
fn lens_correction_undo_redo() {
    let mut s = make_session(8);
    let (id, _, _) = active_surface(&s);
    let before = {
        let st = s.active().unwrap();
        st.doc.layer(id).unwrap().surface().unwrap().rgba(1, 1)
    };

    s.execute(LENS, json!({"distortion": -40, "vignetteAmount": -50})).unwrap();
    let changed = {
        let st = s.active().unwrap();
        st.doc.layer(id).unwrap().surface().unwrap().rgba(1, 1)
    };
    assert_ne!(changed, before, "lens correction should change corner pixel");

    assert!(s.undo());
    let undo_px = {
        let st = s.active().unwrap();
        st.doc.layer(id).unwrap().surface().unwrap().rgba(1, 1)
    };
    assert_eq!(undo_px, before, "undo should restore exact pixel");

    assert!(s.redo());
    let redo_px = {
        let st = s.active().unwrap();
        st.doc.layer(id).unwrap().surface().unwrap().rgba(1, 1)
    };
    assert_eq!(redo_px, changed, "redo should reapply");
}

#[test]
fn lens_correction_via_execute_error_paths() {
    let mut s = make_session(8);
    assert!(s.execute(LENS, json!({"edge": "rainbow"})).is_err());
    assert!(s.execute(LENS, json!({"profile": "zeiss"})).is_err());
}

#[test]
fn wide_angle_command_and_error() {
    let mut s = make_session(8);
    let r = s
        .execute(
            WIDE,
            json!({
                "model": "fisheye",
                "focalLength": 10,
                "constraints": [{"a": [10, 15], "b": [30, 15], "orientation": "horizontal"}]
            }),
        )
        .unwrap();
    assert_eq!(r["model"], "fisheye");
    assert!(r["residual"].as_f64().unwrap() < 2.0);

    assert!(s.execute(WIDE, json!({"constraints": [{"a": [0, 0]}]})).is_err());
}

#[test]
fn camera_raw_command_error_paths() {
    let mut s = make_session(8);
    let legacy_curve = json!([[0, 0], [60, 40], [60, 200], [255, 255]]);
    assert!(s.execute(RAW, json!({"pointCurve": legacy_curve})).is_err());

    // Valid params still work (smart filter style)
    let r = s.execute(RAW, json!({"exposure": 1.0})).unwrap();
    assert!(r["layer"].is_number());
}

#[test]
fn is_enabled_requires_document_and_pixel_layer() {
    let empty = Session::new();
    assert!(!empty.is_enabled(LENS));
    assert!(!empty.is_enabled(RAW));
    assert!(!empty.is_enabled(WIDE));

    let s = make_session(8);
    assert!(s.is_enabled(LENS));
    assert!(s.is_enabled(RAW));
    assert!(s.is_enabled(WIDE));
}
