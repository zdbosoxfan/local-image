use photocraft_color::PixelFormat;
use photocraft_format::manifest::{ContentM, EffectsM, LayerCompM, MaskM, MetadataM, SmartSourceM, SurfaceM};
use serde_json::json;

#[test]
fn surface_m_rejects_missing_format() {
    let v = json!({"default": "00", "tiles": []});
    let r = serde_json::from_value::<SurfaceM>(v);
    assert!(r.is_err());
}

#[test]
fn surface_m_rejects_bad_tile() {
    let fmt = serde_json::to_value(PixelFormat::GRAY8).unwrap();
    let v = json!({"format": fmt, "default": "00", "tiles": [{"tx": 0}]});
    let r = serde_json::from_value::<SurfaceM>(v);
    assert!(r.is_err());
}

#[test]
fn content_m_missing_kind_errors() {
    let fmt = serde_json::to_value(PixelFormat::GRAY8).unwrap();
    let v = json!({"surface": {"format": fmt, "default": "00", "tiles": []}});
    let r = serde_json::from_value::<ContentM>(v);
    assert!(r.is_err());
}

#[test]
fn smart_source_m_missing_kind_errors() {
    let v = json!({"file_name": "doc.psb", "blob": "blobhash"});
    assert!(serde_json::from_value::<SmartSourceM>(v).is_err());
}

#[test]
fn effects_m_missing_reference_defaults_none() {
    let v = json!({
        "enabled": false,
        "items": [],
        "psd_raw": null
    });
    let e: EffectsM = serde_json::from_value(v).unwrap();
    assert_eq!(e.reference, None);
}

#[test]
fn layer_comp_m_missing_states_defaults_empty() {
    let v = json!({
        "id": 1,
        "name": "Comp",
        "comment": "",
        "apply_visibility": true,
        "apply_position": true,
        "apply_appearance": false
    });
    let c: LayerCompM = serde_json::from_value(v).unwrap();
    assert!(c.states.is_empty());
}

#[test]
fn metadata_m_empty_object_errors_on_missing_required_vectors() {
    let r = serde_json::from_value::<MetadataM>(json!({}));
    assert!(r.is_err());
}

#[test]
fn mask_m_nan_serializes_as_null() {
    // serde_json maps non-finite floats to null; this test pins that behavior
    // for the manifest model, which does not reject NaN at the serde layer.
    let fmt = serde_json::to_value(PixelFormat::GRAY8).unwrap();
    let v = json!({
        "surface": {"format": fmt, "default": "00", "tiles": []},
        "enabled": true,
        "linked": false,
        "density": 0.5,
        "feather": 0.0
    });
    let mut m: MaskM = serde_json::from_value(v).unwrap();
    m.density = f32::NAN;
    let value = serde_json::to_value(&m).unwrap();
    assert!(value["density"].is_null());
    // A subsequent deserialize will fail because `density` expects a number.
    assert!(serde_json::from_value::<MaskM>(value).is_err());
}

#[test]
fn manifest_malformed_json_errors() {
    let r = serde_json::from_str::<photocraft_format::Manifest>("{");
    assert!(r.is_err());
}

#[test]
fn manifest_missing_required_field_errors() {
    let v = json!({"format_version": 1, "generator": "test"});
    let r = serde_json::from_value::<photocraft_format::Manifest>(v);
    assert!(r.is_err());
}
