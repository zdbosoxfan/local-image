use photocraft_automation::server::{
    BatchParams, ControlParams, DocIndex, JobCancelParams, ListParams, MenuParams, NewParams, OpenParams, PointerParams, PreviewParams, RunParams, SaveParams,
    SelectParams, UiSetParams,
};
use serde_json::json;

#[test]
fn open_params_requires_path() {
    assert!(serde_json::from_str::<OpenParams>("{}").is_err());
    let p: OpenParams = serde_json::from_str(r#"{"path":"images/a.png"}"#).unwrap();
    assert_eq!(p.path, "images/a.png");
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

    // quality 0 deserializes; doc_save refuses it.
    let p: SaveParams = serde_json::from_str(r#"{"quality":0}"#).unwrap();
    assert_eq!(p.quality, Some(0));
}

#[test]
fn select_params_requires_index() {
    assert!(serde_json::from_str::<SelectParams>("{}").is_err());
    let p: SelectParams = serde_json::from_str(r#"{"index":3}"#).unwrap();
    assert_eq!(p.index, 3);
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
