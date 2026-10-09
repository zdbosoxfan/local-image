use photocraft_automation::Headless;
use photocraft_automation::server::{
    Backend, BatchParams, ControlParams, DocIndex, JobCancelParams, ListParams, MenuParams, NewParams, OpenParams, PhotocraftMcp, PointerParams, PreviewParams,
    RunParams, SaveParams, SelectParams, UiSetParams,
};
use serde_json::json;
use std::sync::{Arc, Mutex};

#[test]
fn open_params_requires_path() {
    assert!(serde_json::from_str::<OpenParams>("{}").is_err());
    let p: OpenParams = serde_json::from_str(r#"{"path":"images/a.png"}"#).unwrap();
    assert_eq!(p.path, "images/a.png");
}

#[test]
fn new_params_defaults() {
    let p: NewParams = serde_json::from_str("{}").unwrap();
    assert_eq!(p.width, None);
    assert_eq!(p.height, None);
    assert_eq!(p.mode, None);
    assert_eq!(p.depth, None);
    assert_eq!(p.background, None);
    assert_eq!(p.name, None);

    let p: NewParams = serde_json::from_str(r#"{"width":800,"height":600,"mode":"gray","depth":16,"background":"black","name":"test"}"#).unwrap();
    assert_eq!(p.width, Some(800));
    assert_eq!(p.height, Some(600));
    assert_eq!(p.mode.as_deref(), Some("gray"));
    assert_eq!(p.depth, Some(16));
    assert_eq!(p.background.as_deref(), Some("black"));
    assert_eq!(p.name.as_deref(), Some("test"));
}

#[test]
fn save_params_tiff_layers_rename_and_defaults() {
    let p: SaveParams = serde_json::from_str("{}").unwrap();
    assert_eq!(p.path, None);
    assert_eq!(p.format, None);
    assert_eq!(p.quality, None);
    assert!(!p.tiff_layers);
    assert_eq!(p.index, None);

    let p: SaveParams = serde_json::from_str(r#"{"path":"out.png","format":"png","quality":85,"tiffLayers":true,"index":1}"#).unwrap();
    assert_eq!(p.path.as_deref(), Some("out.png"));
    assert_eq!(p.format.as_deref(), Some("png"));
    assert_eq!(p.quality, Some(85));
    assert!(p.tiff_layers);
    assert_eq!(p.index, Some(1));

    // quality 0 is valid at deserialization (clamping happens later).
    let p: SaveParams = serde_json::from_str(r#"{"quality":0}"#).unwrap();
    assert_eq!(p.quality, Some(0));
}

#[test]
fn preview_params_defaults() {
    let p: PreviewParams = serde_json::from_str("{}").unwrap();
    assert_eq!(p.index, None);
    assert_eq!(p.max_side, None);

    let p: PreviewParams = serde_json::from_str(r#"{"index":2,"max_side":2048}"#).unwrap();
    assert_eq!(p.index, Some(2));
    assert_eq!(p.max_side, Some(2048));
}

#[test]
fn select_params_requires_index() {
    assert!(serde_json::from_str::<SelectParams>("{}").is_err());
    let p: SelectParams = serde_json::from_str(r#"{"index":3}"#).unwrap();
    assert_eq!(p.index, 3);
}

#[test]
fn list_params_defaults() {
    let p: ListParams = serde_json::from_str("{}").unwrap();
    assert_eq!(p.filter, None);
    assert_eq!(p.enabled_only, None);

    let p: ListParams = serde_json::from_str(r#"{"filter":"blur","enabled_only":true}"#).unwrap();
    assert_eq!(p.filter.as_deref(), Some("blur"));
    assert_eq!(p.enabled_only, Some(true));
}

#[test]
fn run_params_defaults() {
    let p: RunParams = serde_json::from_str(r#"{"id":"layer.new.layer"}"#).unwrap();
    assert_eq!(p.id, "layer.new.layer");
    assert_eq!(p.params, None);
    assert_eq!(p.wait, None);

    let p: RunParams = serde_json::from_str(r#"{"id":"filter.blur.gaussianBlur","params":{"radius":4},"wait":false}"#).unwrap();
    assert_eq!(p.id, "filter.blur.gaussianBlur");
    assert_eq!(p.params, Some(json!({"radius": 4})));
    assert_eq!(p.wait, Some(false));
}

#[test]
fn job_cancel_params_defaults() {
    let p: JobCancelParams = serde_json::from_str("{}").unwrap();
    assert_eq!(p.job, None);

    let p: JobCancelParams = serde_json::from_str(r#"{"job":42}"#).unwrap();
    assert_eq!(p.job, Some(42));
}

#[test]
fn batch_params_requires_steps() {
    assert!(serde_json::from_str::<BatchParams>("{}").is_err());
    let p: BatchParams = serde_json::from_str(r#"{"steps":[{"id":"layer.new.layer","params":{"name":"Ink"}}],"stop_on_error":false}"#).unwrap();
    assert_eq!(p.steps.len(), 1);
    assert_eq!(p.steps[0].id, "layer.new.layer");
    assert_eq!(p.steps[0].params, Some(json!({"name": "Ink"})));
    assert_eq!(p.stop_on_error, Some(false));
}

#[test]
fn pointer_params_deny_unknown_fields() {
    let valid = r#"{"events":[{"kind":"down","x":1.0,"y":2.0}]}"#;
    let p: PointerParams = serde_json::from_str(valid).unwrap();
    assert_eq!(p.events.len(), 1);
    assert_eq!(p.modifiers, None);
    assert_eq!(p.button, None);

    let invalid = r#"{"events":[],"extra":true}"#;
    assert!(serde_json::from_str::<PointerParams>(invalid).is_err());
}

#[test]
fn control_params_requires_method() {
    assert!(serde_json::from_str::<ControlParams>("{}").is_err());
    let p: ControlParams = serde_json::from_str(r#"{"method":"ui.dialog.open","params":{"id":"save"}}"#).unwrap();
    assert_eq!(p.method, "ui.dialog.open");
    assert_eq!(p.params, Some(json!({"id": "save"})));
}

#[test]
fn doc_index_defaults() {
    let p: DocIndex = serde_json::from_str("{}").unwrap();
    assert_eq!(p.index, None);
    let p: DocIndex = serde_json::from_str(r#"{"index":0}"#).unwrap();
    assert_eq!(p.index, Some(0));
}

#[test]
fn ui_set_params_accepts_any_value() {
    let p: UiSetParams = serde_json::from_str(r#"{"fields":{"tool":"move","zoom":1.5}}"#).unwrap();
    assert_eq!(p.fields, json!({"tool":"move","zoom":1.5}));
}

#[test]
fn menu_params_requires_id() {
    assert!(serde_json::from_str::<MenuParams>("{}").is_err());
    let p: MenuParams = serde_json::from_str(r#"{"id":"file.open"}"#).unwrap();
    assert_eq!(p.id, "file.open");
}

#[test]
fn null_deserialization_does_not_panic() {
    // Passing `null` where an object is expected should return Err, never panic.
    assert!(serde_json::from_str::<OpenParams>("null").is_err());
    assert!(serde_json::from_str::<NewParams>("null").is_err());
    assert!(serde_json::from_str::<SaveParams>("null").is_err());
    assert!(serde_json::from_str::<PreviewParams>("null").is_err());
    assert!(serde_json::from_str::<SelectParams>("null").is_err());
    assert!(serde_json::from_str::<ListParams>("null").is_err());
    assert!(serde_json::from_str::<RunParams>("null").is_err());
    assert!(serde_json::from_str::<JobCancelParams>("null").is_err());
    assert!(serde_json::from_str::<BatchParams>("null").is_err());
    assert!(serde_json::from_str::<PointerParams>("null").is_err());
    assert!(serde_json::from_str::<MenuParams>("null").is_err());
    assert!(serde_json::from_str::<UiSetParams>("null").is_err());
    assert!(serde_json::from_str::<ControlParams>("null").is_err());
    assert!(serde_json::from_str::<DocIndex>("null").is_err());
}

#[test]
fn mcp_headless_constructs_without_panic() {
    let _mcp = PhotocraftMcp::headless();
    // No panic, instance exists.
}

#[test]
fn backend_headless_variant_constructs() {
    let headless = Headless::new();
    let backend = Backend::Headless(Arc::new(Mutex::new(headless)));
    // Backend can be constructed and owned.
    drop(backend);
}

#[test]
fn mcp_with_backend_accepts_headless_backend() {
    let headless = Headless::new();
    let backend = Backend::Headless(Arc::new(Mutex::new(headless)));
    let mcp = PhotocraftMcp::with_backend(backend);
    // mcp owns the backend successfully.
    drop(mcp);
}
