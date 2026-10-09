use std::sync::Arc;

use lightcraft_engine::{
    EngineError, Session,
    catalog::PhotoId,
    merge::{MergeFinish, MergeJob, MergeKind, MergeOutput, parse},
};
use serde_json::{Value, json};

// Helper: parse a merge kind from empty params.
fn kind_from(id: &str) -> MergeKind {
    let (kind, _) = parse(id, &json!({})).unwrap();
    kind
}

#[test]
fn parse_hdr_defaults() {
    let (kind, finish) = parse("merge.hdr", &json!({})).unwrap();
    let v = serde_json::to_value(&kind).unwrap();
    assert_eq!(v, json!({"kind":"hdr","hdr":{"align":true,"deghost":"none"}}));
    assert_eq!(finish, MergeFinish { auto_settings: true, stack: false, show_overlay: false });
}

#[test]
fn parse_hdr_custom_deghost_and_align() {
    let (kind, _) = parse("merge.hdr", &json!({"deghost":"high","align":false})).unwrap();
    let v = serde_json::to_value(&kind).unwrap();
    assert_eq!(v, json!({"kind":"hdr","hdr":{"align":false,"deghost":"high"}}));
}

#[test]
fn parse_panorama_defaults() {
    let (kind, finish) = parse("merge.panorama", &json!({})).unwrap();
    let v = serde_json::to_value(&kind).unwrap();
    assert_eq!(v, json!({"kind":"panorama","pano":{"projection":"auto","boundaryWarp":0.0,"autoCrop":false,"fillEdges":false,"maxMegapixels":40.0}}));
    assert_eq!(finish, MergeFinish { auto_settings: true, stack: false, show_overlay: false });
}

#[test]
fn parse_hdr_panorama_custom() {
    let (kind, finish) = parse(
        "merge.hdrPanorama",
        &json!({
            "bracket": 3,
            "deghost": "medium",
            "projection": "perspective",
            "boundaryWarp": 25.5,
            "autoCrop": true,
            "fillEdges": true,
            "maxMegapixels": 80.0
        }),
    )
    .unwrap();
    let v = serde_json::to_value(&kind).unwrap();
    assert_eq!(
        v,
        json!({
            "kind": "hdrPanorama",
            "hdr": {"align": true, "deghost": "medium"},
            "pano": {"projection": "perspective", "boundaryWarp": 25.5, "autoCrop": true, "fillEdges": true, "maxMegapixels": 80.0},
            "bracket": 3
        })
    );
    assert_eq!(finish, MergeFinish { auto_settings: true, stack: false, show_overlay: false });
}

#[test]
fn parse_unknown_id_treated_as_hdr_panorama() {
    let (kind, _) = parse("merge.bogus", &json!({})).unwrap();
    assert!(matches!(kind, MergeKind::HdrPanorama { .. }));
}

#[test]
fn parse_invalid_deghost_returns_badparams() {
    let err = parse("merge.hdr", &json!({"deghost":"bogus"})).unwrap_err();
    match err {
        EngineError::BadParams { cmd, msg } => {
            assert_eq!(cmd, "merge.hdr");
            assert!(msg.contains("deghost"));
        }
        other => panic!("expected BadParams, got {other:?}"),
    }
}

#[test]
fn parse_invalid_projection_returns_badparams() {
    let err = parse("merge.panorama", &json!({"projection":"bogus"})).unwrap_err();
    match err {
        EngineError::BadParams { cmd, msg } => {
            assert_eq!(cmd, "merge.panorama");
            assert!(msg.contains("projection"));
        }
        other => panic!("expected BadParams, got {other:?}"),
    }
}

#[test]
fn parse_boundary_warp_clamps_and_defaults_non_numeric() {
    // JSON cannot represent NaN or Infinity, so we only test clamping and non-numeric default.
    let get_bw = |p: Value| -> f64 {
        let (kind, _) = parse("merge.panorama", &p).unwrap();
        let v = serde_json::to_value(&kind).unwrap();
        v["pano"]["boundaryWarp"].as_f64().unwrap()
    };
    assert_eq!(get_bw(json!({"boundaryWarp": 150})), 100.0);
    assert_eq!(get_bw(json!({"boundaryWarp": -50})), 0.0);
    assert_eq!(get_bw(json!({"boundaryWarp": "not a number"})), 0.0);
    assert_eq!(get_bw(json!({})), 0.0);
}

#[test]
fn merge_kind_roundtrip_serialization() {
    for id in ["merge.hdr", "merge.panorama", "merge.hdrPanorama"] {
        let (kind, _) = parse(id, &json!({})).unwrap();
        let json_str = serde_json::to_string(&kind).unwrap();
        let kind2: MergeKind = serde_json::from_str(&json_str).unwrap();
        assert_eq!(kind, kind2);
    }
}

#[test]
fn merge_finish_roundtrip_serialization() {
    let finish = MergeFinish { auto_settings: true, stack: true, show_overlay: true };
    let json_str = serde_json::to_string(&finish).unwrap();
    let finish2: MergeFinish = serde_json::from_str(&json_str).unwrap();
    assert_eq!(finish, finish2);
}

#[test]
fn run_insufficient_sources_returns_error() {
    let cases: [(&str, u64); 3] = [("merge.hdr", 1), ("merge.panorama", 1), ("merge.hdrPanorama", 3)];
    for (id, count) in cases {
        let (kind, _) = parse(id, &json!({})).unwrap();
        let job = MergeJob {
            kind,
            finish: MergeFinish::default(),
            sources: (0..count).map(|i| (PhotoId(i), format!("dummy{i}"))).collect(),
            preview: None,
            read: Arc::new(|_| Ok(vec![])),
        };
        let result = job.run(&|_, _| true);
        assert!(result.is_err(), "id {id}: expected error");
        if let Err(e) = result {
            assert!(e.contains("select at least"), "id {id}: {e}");
        }
    }
}

#[test]
fn run_cancels_at_start() {
    let (kind, _) = parse("merge.hdrPanorama", &json!({})).unwrap();
    let job = MergeJob {
        kind,
        finish: MergeFinish::default(),
        sources: (0..4).map(|i| (PhotoId(i), format!("dummy{i}"))).collect(),
        preview: None,
        read: Arc::new(|_| Ok(vec![])),
    };
    let result = job.run(&|_, _| false);
    assert_eq!(result.err().unwrap(), "cancelled");
}

#[test]
fn run_read_error_propagates() {
    let (kind, _) = parse("merge.hdr", &json!({})).unwrap();
    let job = MergeJob {
        kind,
        finish: MergeFinish::default(),
        sources: vec![(PhotoId(0), "dummy_path".to_string()), (PhotoId(1), "dummy_path".to_string())],
        preview: None,
        // Return empty bytes to force load_frame to fail; the error will include the path.
        read: Arc::new(|_| Ok(vec![])),
    };
    let result = job.run(&|_, _| true);
    assert!(result.is_err());
    let err = result.err().unwrap();
    assert!(err.contains("dummy_path"), "error was: {err}");
}

#[test]
fn plan_merge_unknown_photo_returns_catalog_error() {
    let sess = Session::new();
    let (kind, finish) = parse("merge.hdr", &json!({})).unwrap();
    let result = sess.plan_merge(kind, finish, &[PhotoId(1)], false);
    assert!(matches!(result, Err(EngineError::Catalog(_))));
}

#[test]
fn plan_merge_empty_ids_returns_error() {
    let sess = Session::new();
    let (kind, finish) = parse("merge.hdr", &json!({})).unwrap();
    let result = sess.plan_merge(kind, finish, &[], false);
    assert!(result.is_err());
    match result {
        Err(e) => assert!(e.to_string().contains("select at least")),
        Ok(_) => panic!("expected error"),
    }
}

#[test]
fn finish_merge_no_sources_errors() {
    let mut sess = Session::new();
    let job =
        MergeJob { kind: kind_from("merge.hdr"), finish: MergeFinish::default(), sources: vec![], preview: None, read: Arc::new(|_| Ok(vec![])) };
    let out = MergeOutput { dng: vec![], preview: None, crop: None, info: json!({}) };
    let result = sess.finish_merge(&job, out);
    assert!(result.is_err());
    match result {
        Err(e) => assert!(e.to_string().contains("nothing merged")),
        Ok(_) => panic!("expected error"),
    }
}

#[test]
fn finish_merge_write_to_missing_dir_errors() {
    let mut sess = Session::new();
    let job = MergeJob {
        kind: kind_from("merge.hdr"),
        finish: MergeFinish::default(),
        sources: vec![(PhotoId(0), "/nonexistent_dir_for_test/photo.dng".to_string())],
        preview: None,
        read: Arc::new(|_| Ok(vec![])),
    };
    let out = MergeOutput { dng: vec![1, 2, 3], preview: None, crop: None, info: json!({}) };
    let result = sess.finish_merge(&job, out);
    assert!(result.is_err());
}

#[test]
fn merge_now_invalid_photo_returns_error() {
    let mut sess = Session::new();
    let (kind, finish) = parse("merge.hdr", &json!({})).unwrap();
    let result = sess.merge_now(kind, finish, &[PhotoId(1)]);
    assert!(matches!(result, Err(EngineError::Catalog(_))));
}

#[test]
fn parse_null_params_works() {
    let (kind, finish) = parse("merge.hdr", &Value::Null).unwrap();
    assert!(matches!(kind, MergeKind::Hdr { .. }));
    assert_eq!(finish, MergeFinish { auto_settings: true, stack: false, show_overlay: false });
}
