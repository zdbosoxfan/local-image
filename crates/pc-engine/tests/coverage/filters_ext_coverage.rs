use photocraft_engine::filters_ext::{params_for, specs};
use photocraft_engine::{EngineError, Session};
use serde_json::{Value, json};

#[test]
fn specs_run_returns_err_without_document() {
    let mut session = Session::new();
    for spec in specs() {
        let result = (spec.run)(&mut session, &json!({}));
        assert!(result.is_err(), "run did not return Err without document for {}", spec.id);
    }
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
