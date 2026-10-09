use photocraft_color::PixelFormat;
use photocraft_format::manifest::{ChannelM, ContentM, EffectsM, FORMAT_VERSION, LayerCompM, MaskM, MetadataM, PatternM, SmartSourceM, SurfaceM, TileRef};
use serde_json::{Value, json};

fn surface_value() -> Value {
    let fmt = serde_json::to_value(PixelFormat::GRAY8).unwrap();
    json!({
        "format": fmt,
        "default": "00",
        "tiles": []
    })
}

#[test]
fn format_version_is_one() {
    assert_eq!(FORMAT_VERSION, 1);
}

#[test]
fn tile_ref_round_trip() {
    let v = json!({"tx": 0, "ty": 0, "hash": "abc"});
    let tile: TileRef = serde_json::from_value(v.clone()).unwrap();
    let out = serde_json::to_value(&tile).unwrap();
    assert_eq!(out, v);
}

#[test]
fn tile_ref_extreme_coordinates_round_trip() {
    let v = json!({"tx": i32::MIN, "ty": i32::MAX, "hash": "extreme"});
    let tile: TileRef = serde_json::from_value(v.clone()).unwrap();
    assert_eq!(tile.tx, i32::MIN);
    assert_eq!(tile.ty, i32::MAX);
    let out = serde_json::to_value(&tile).unwrap();
    assert_eq!(out, v);
}

#[test]
fn surface_m_round_trip() {
    let v = surface_value();
    let s: SurfaceM = serde_json::from_value(v.clone()).unwrap();
    let out = serde_json::to_value(&s).unwrap();
    assert_eq!(out, v);
}

#[test]
fn surface_m_with_tile_round_trip() {
    let fmt = serde_json::to_value(PixelFormat::GRAY8).unwrap();
    let v = json!({
        "format": fmt,
        "default": "00",
        "tiles": [
            {"tx": -1, "ty": 2, "hash": "tilehash"}
        ]
    });
    let s: SurfaceM = serde_json::from_value(v.clone()).unwrap();
    assert_eq!(s.tiles.len(), 1);
    let out = serde_json::to_value(&s).unwrap();
    assert_eq!(out, v);
}

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
fn content_m_raster_round_trip() {
    let fmt = serde_json::to_value(PixelFormat::GRAY8).unwrap();
    let v = json!({
        "kind": "raster",
        "surface": {"format": fmt, "default": "00", "tiles": []}
    });
    let c: ContentM = serde_json::from_value(v.clone()).unwrap();
    let out = serde_json::to_value(&c).unwrap();
    assert_eq!(out, v);
}

#[test]
fn content_m_missing_kind_errors() {
    let fmt = serde_json::to_value(PixelFormat::GRAY8).unwrap();
    let v = json!({"surface": {"format": fmt, "default": "00", "tiles": []}});
    let r = serde_json::from_value::<ContentM>(v);
    assert!(r.is_err());
}

#[test]
fn smart_source_m_embedded_and_linked_round_trip() {
    let embedded = json!({
        "kind": "embedded",
        "file_name": "doc.psb",
        "blob": "blobhash"
    });
    let e: SmartSourceM = serde_json::from_value(embedded.clone()).unwrap();
    assert_eq!(serde_json::to_value(&e).unwrap(), embedded);

    let linked = json!({
        "kind": "linked",
        "path": "/tmp/doc.psb"
    });
    let l: SmartSourceM = serde_json::from_value(linked.clone()).unwrap();
    assert_eq!(serde_json::to_value(&l).unwrap(), linked);
}

#[test]
fn smart_source_m_missing_kind_errors() {
    let v = json!({"file_name": "doc.psb", "blob": "blobhash"});
    assert!(serde_json::from_value::<SmartSourceM>(v).is_err());
}

#[test]
fn effects_m_round_trip() {
    let v = json!({
        "enabled": true,
        "items": [],
        "psd_raw": null,
        "reference": [1.5, -2.25]
    });
    let e: EffectsM = serde_json::from_value(v.clone()).unwrap();
    assert_eq!(e.reference, Some((1.5, -2.25)));
    let out = serde_json::to_value(&e).unwrap();
    assert_eq!(out, v);
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
fn layer_comp_m_round_trip() {
    let v = json!({
        "id": 1,
        "name": "Comp 1",
        "comment": "",
        "apply_visibility": true,
        "apply_position": false,
        "apply_appearance": false,
        "states": [
            {"layer": 42, "visible": null, "position": null, "appearance": null}
        ]
    });
    let c: LayerCompM = serde_json::from_value(v.clone()).unwrap();
    assert_eq!(c.states.len(), 1);
    let out = serde_json::to_value(&c).unwrap();
    assert_eq!(out, v);
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
fn pattern_m_round_trip() {
    let fmt = serde_json::to_value(PixelFormat::GRAY8).unwrap();
    let v = json!({
        "id": "pat",
        "name": "Pattern",
        "width": 16,
        "height": 32,
        "surface": {"format": fmt, "default": "00", "tiles": []}
    });
    let p: PatternM = serde_json::from_value(v.clone()).unwrap();
    let out = serde_json::to_value(&p).unwrap();
    assert_eq!(out, v);
}

#[test]
fn metadata_m_empty_object_errors_on_missing_required_vectors() {
    let r = serde_json::from_value::<MetadataM>(json!({}));
    assert!(r.is_err());
}

#[test]
fn metadata_m_round_trip() {
    let v = json!({
        "xmp": "xmpdata",
        "exif": "exifhash",
        "psd_resources": [[1000, "name", "blobhash"]],
        "psd_global_blocks": [["abcd", "1234", "globalblob"]]
    });
    let m: MetadataM = serde_json::from_value(v.clone()).unwrap();
    let out = serde_json::to_value(&m).unwrap();
    assert_eq!(out, v);
}

#[test]
fn channel_m_older_json_gets_defaults() {
    let fmt = serde_json::to_value(PixelFormat::GRAY8).unwrap();
    let v = json!({
        "name": "Alpha 1",
        "surface": {"format": fmt, "default": "00", "tiles": []},
        "spot": null
    });
    let c: ChannelM = serde_json::from_value(v).unwrap();
    assert_eq!(c.opacity, 0.5);
    // Color and indicates are present and serializable; actual values are pinned by
    // photocraft_doc's AlphaChannel / ColorIndicates, not by this manifest model.
    let _ = serde_json::to_value(c.color).unwrap();
    let _ = serde_json::to_value(c.indicates).unwrap();
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
