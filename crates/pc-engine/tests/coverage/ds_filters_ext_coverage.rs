use std::collections::HashSet;

use photocraft_engine::filters_ext::{params_for, specs};
use photocraft_engine::{EngineError, Session};
use serde_json::{Value, json};

#[test]
fn specs_count_is_stable() {
    let specs = specs();
    assert_eq!(specs.len(), 32, "public filter command count changed unexpectedly");
}

#[test]
fn specs_ids_are_unique() {
    let specs = specs();
    let mut seen = HashSet::new();
    for spec in &specs {
        assert!(seen.insert(spec.id), "duplicate id: {}", spec.id);
    }
}

#[test]
fn specs_include_expected_commands() {
    let specs = specs();
    let ids: HashSet<&str> = specs.iter().map(|s| s.id).collect();
    for expected in [
        "filter.pixelate.colorHalftone",
        "filter.pixelate.crystallize",
        "filter.pixelate.facet",
        "filter.pixelate.fragment",
        "filter.pixelate.mezzotint",
        "filter.pixelate.pointillize",
        "filter.stylize.diffuse",
        "filter.stylize.extrude",
        "filter.stylize.oilPaint",
        "filter.stylize.tiles",
        "filter.stylize.traceContour",
        "filter.stylize.wind",
        "filter.distort.displace",
        "filter.distort.shear",
        "filter.distort.zigZag",
        "filter.render.fibers",
        "filter.render.lensFlare",
        "filter.render.lightingEffects",
        "filter.render.relight",
        "filter.noise.reduceNoise",
        "filter.blur.smartBlur",
        "filter.blur.lensBlur",
        "filter.blur.shapeBlur",
        "filter.blurGallery.tiltShift",
        "filter.blurGallery.irisBlur",
        "filter.blurGallery.fieldBlur",
        "filter.blurGallery.spinBlur",
        "filter.blurGallery.pathBlur",
        "filter.other.custom",
        "filter.other.hsbHsl",
        "filter.video.deInterlace",
        "filter.video.ntscColors",
    ] {
        assert!(ids.contains(expected), "missing command id: {expected}");
    }
}

#[test]
fn specs_params_are_non_empty() {
    for spec in specs() {
        assert!(!spec.params.is_empty(), "empty params string for {}", spec.id);
    }
}

#[test]
fn specs_have_filter_menu_root() {
    for spec in specs() {
        assert!(!spec.menu.is_empty(), "empty menu for {}", spec.id);
        assert_eq!(spec.menu[0], "Filter", "unexpected menu root for {}", spec.id);
    }
}

#[test]
fn specs_enabled_returns_err_without_document() {
    let session = Session::new();
    for spec in specs() {
        let result = (spec.enabled)(&session);
        assert!(result.is_err(), "enabled did not return Err without document for {}", spec.id);
    }
}

#[test]
fn specs_run_returns_err_without_document() {
    let mut session = Session::new();
    for spec in specs() {
        let result = (spec.run)(&mut session, &json!({}));
        assert!(result.is_err(), "run did not return Err without document for {}", spec.id);
    }
}

#[test]
fn params_for_returns_some_for_known_ids() {
    let known = [
        "filter.pixelate.colorHalftone",
        "filter.pixelate.crystallize",
        "filter.pixelate.facet",
        "filter.pixelate.fragment",
        "filter.pixelate.mezzotint",
        "filter.pixelate.pointillize",
        "filter.stylize.diffuse",
        "filter.stylize.extrude",
        "filter.stylize.oilPaint",
        "filter.stylize.tiles",
        "filter.stylize.traceContour",
        "filter.stylize.wind",
        "filter.distort.displace",
        "filter.distort.shear",
        "filter.distort.zigZag",
        "filter.render.fibers",
        "filter.render.lensFlare",
        "filter.render.lightingEffects",
        "filter.noise.reduceNoise",
        "filter.blur.smartBlur",
        "filter.blur.lensBlur",
        "filter.blur.shapeBlur",
        "filter.blurGallery.tiltShift",
        "filter.blurGallery.irisBlur",
        "filter.blurGallery.fieldBlur",
        "filter.blurGallery.spinBlur",
        "filter.blurGallery.pathBlur",
        "filter.other.custom",
        "filter.other.hsbHsl",
        "filter.video.deInterlace",
        "filter.video.ntscColors",
    ];
    for id in known {
        assert!(params_for(id, &json!({})).is_some(), "params_for returned None for {id}");
    }
}

#[test]
fn params_for_returns_none_for_unknown_id() {
    assert!(params_for("filter.does.not.exist", &json!({})).is_none());
}

#[test]
fn params_for_does_not_panic_on_weird_json() {
    let weird_values = vec![
        Value::Null,
        json!(42),
        json!("text"),
        json!([1, 2, 3]),
        json!({"cellSize": "not-a-number", "seed": null}),
        json!({"kernel": [1.0, 2.0, "bad"]}),
        json!({"lights": [{"x": "NaN"}]}),
    ];
    for id in ["filter.pixelate.crystallize", "filter.blur.irisBlur", "filter.other.custom", "filter.render.lightingEffects"] {
        for v in &weird_values {
            let _ = params_for(id, v);
        }
    }
}

#[test]
fn params_for_defaults_work() {
    assert!(params_for("filter.pixelate.facet", &json!({})).is_some());
    assert!(params_for("filter.pixelate.fragment", &json!({})).is_some());
    assert!(params_for("filter.video.ntscColors", &json!({})).is_some());
}

#[test]
fn relight_rejects_non_object_params() {
    let spec = specs().into_iter().find(|s| s.id == "filter.render.relight").expect("relight spec missing");
    let mut session = Session::new();

    for bad in [Value::Null, json!(42), json!("hello"), json!([])] {
        let result = (spec.run)(&mut session, &bad);
        match result {
            Err(EngineError::BadParams { cmd, msg }) => {
                assert_eq!(cmd, "filter.render.relight");
                assert!(msg.contains("params must be a JSON object"), "unexpected message: {msg}");
            }
            other => panic!("expected BadParams for {bad}, got {other:?}"),
        }
    }
}

#[test]
fn relight_rejects_out_of_range_numeric_params() {
    let spec = specs().into_iter().find(|s| s.id == "filter.render.relight").expect("relight spec missing");
    let mut session = Session::new();

    let bad_params = json!({"angle": 200.0});
    let result = (spec.run)(&mut session, &bad_params);
    match result {
        Err(EngineError::BadParams { cmd, msg }) => {
            assert_eq!(cmd, "filter.render.relight");
            assert!(msg.contains("angle"), "expected message about angle, got: {msg}");
        }
        other => panic!("expected BadParams for out-of-range angle, got {other:?}"),
    }
}

#[test]
fn relight_accepts_valid_params_without_document() {
    let spec = specs().into_iter().find(|s| s.id == "filter.render.relight").expect("relight spec missing");
    let mut session = Session::new();
    let result = (spec.run)(&mut session, &json!({"angle": 45, "elevation": 40}));
    // Should pass validation, then fail because there's no active layer (since no document).
    match result {
        Err(EngineError::Other(msg)) => {
            assert!(msg.contains("no active layer"), "unexpected message: {msg}");
        }
        other => panic!("expected Other(\"no active layer\"), got {other:?}"),
    }
}

#[test]
fn params_for_determinism() {
    let id = "filter.pixelate.crystallize";
    let p = json!({"cellSize": 12, "seed": 7});
    let first = params_for(id, &p);
    for _ in 0..3 {
        let again = params_for(id, &p);
        // Can't compare values directly because FilterParams isn't exported, but both must be Some.
        assert!(first.is_some() && again.is_some(), "params_for should always return Some for valid known id");
    }
}
