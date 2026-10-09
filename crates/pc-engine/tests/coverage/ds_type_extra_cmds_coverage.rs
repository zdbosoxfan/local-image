use photocraft_engine::doc::text::{AntiAlias, Orientation, TextShape};
use photocraft_engine::doc::{LayerContent, TextLayer};
use photocraft_engine::{EngineError, Session, type_extra_cmds};
use serde_json::json;

// ---------- helpers ----------

fn new_doc(s: &mut Session) {
    let r = s.execute("file.new", json!({"width": 800, "height": 600}));
    assert!(r.is_ok(), "file.new failed: {r:?}");
}

fn new_text(s: &mut Session) -> u64 {
    let r = s.execute("type.create", json!({"box": [80.0, 60.0, 640.0, 480.0], "text": "Hello", "size": 24.0})).expect("type.create failed");
    r["layer"].as_u64().expect("type.create did not return layer id")
}

fn active_text(s: &Session) -> Option<&TextLayer> {
    let d = s.active()?;
    let id = d.active_layer?;
    let layer = d.doc.layer(id)?;
    match &layer.content {
        LayerContent::Text(t) => Some(t),
        _ => None,
    }
}

// ---------- public constants & free functions ----------

#[test]
fn lorem_ipsum_constant_is_nonempty_and_starts_with_lorem() {
    assert!(type_extra_cmds::LOREM_IPSUM.len() > 100);
    assert!(type_extra_cmds::LOREM_IPSUM.starts_with("Lorem ipsum dolor sit amet"));
}

#[test]
fn aa_name_maps_all_variants_to_expected_strings() {
    assert_eq!(type_extra_cmds::aa_name(AntiAlias::None), "none");
    assert_eq!(type_extra_cmds::aa_name(AntiAlias::Sharp), "sharp");
    assert_eq!(type_extra_cmds::aa_name(AntiAlias::Crisp), "crisp");
    assert_eq!(type_extra_cmds::aa_name(AntiAlias::Strong), "strong");
    assert_eq!(type_extra_cmds::aa_name(AntiAlias::Smooth), "smooth");
    assert_eq!(type_extra_cmds::aa_name(AntiAlias::Windows), "windows");
    assert_eq!(type_extra_cmds::aa_name(AntiAlias::WindowsLcd), "windowsLcd");
}

#[test]
fn feature_on_reads_liga_dlig_and_general_features_from_real_layer() {
    let mut s = Session::new();
    new_doc(&mut s);
    new_text(&mut s);

    s.execute("type.openType.standardLigatures", json!({"on": true})).unwrap();
    s.execute("type.openType.discretionaryLigatures", json!({"on": true})).unwrap();
    s.execute("type.openType.swash", json!({"on": true})).unwrap();

    let t = active_text(&s).unwrap();
    let style = t.char_runs().first().unwrap().style.clone();
    assert!(type_extra_cmds::feature_on(&style, "liga"));
    assert!(type_extra_cmds::feature_on(&style, "dlig"));
    assert!(type_extra_cmds::feature_on(&style, "swsh"));
    assert!(!type_extra_cmds::feature_on(&style, "unknown"));
}

// ---------- registry / specs ----------

#[test]
fn specs_are_unique_all_type_commands_and_well_formed() {
    let specs = type_extra_cmds::specs();
    let mut ids: Vec<&str> = specs.iter().map(|s| s.id).collect();
    let original_len = ids.len();
    ids.sort_unstable();
    ids.dedup();
    assert_eq!(ids.len(), original_len, "duplicate spec ids found");

    assert!(ids.len() >= 28, "expected at least 28 type extra commands");
    for spec in &specs {
        assert!(spec.id.starts_with("type."), "bad id {}", spec.id);
        assert!(!spec.label.is_empty(), "empty label for {}", spec.id);
        assert!(spec.menu.contains(&"Type"), "menu must contain Type for {}", spec.id);
        assert!(!spec.params.is_empty(), "empty params for {}", spec.id);
    }
}

// ---------- anti-alias / orientation ----------

#[test]
fn anti_alias_commands_roundtrip_all_variants() {
    let mut s = Session::new();
    new_doc(&mut s);
    new_text(&mut s);

    let variants = [
        ("type.antiAlias.none", AntiAlias::None),
        ("type.antiAlias.sharp", AntiAlias::Sharp),
        ("type.antiAlias.crisp", AntiAlias::Crisp),
        ("type.antiAlias.strong", AntiAlias::Strong),
        ("type.antiAlias.smooth", AntiAlias::Smooth),
        ("type.antiAlias.windowsLcd", AntiAlias::WindowsLcd),
        ("type.antiAlias.windows", AntiAlias::Windows),
    ];

    for (cmd, expected) in variants {
        let r = s.execute(cmd, json!({}));
        assert!(r.is_ok(), "{cmd} failed: {r:?}");
        let t = active_text(&s).expect("no active text layer after AA change");
        assert_eq!(t.antialias, expected, "antialias not updated by {cmd}");
    }
}

#[test]
fn orientation_commands_roundtrip_vertical_then_horizontal() {
    let mut s = Session::new();
    new_doc(&mut s);
    new_text(&mut s);

    let r = s.execute("type.orientation.vertical", json!({}));
    assert!(r.is_ok());
    assert_eq!(active_text(&s).unwrap().orientation, Orientation::Vertical);

    let r = s.execute("type.orientation.horizontal", json!({}));
    assert!(r.is_ok());
    assert_eq!(active_text(&s).unwrap().orientation, Orientation::Horizontal);
}

// ---------- OpenType toggles ----------

#[test]
fn opentype_toggles_roundtrip_for_liga_and_custom_feature() {
    let mut s = Session::new();
    new_doc(&mut s);
    new_text(&mut s);

    let r = s.execute("type.openType.standardLigatures", json!({"on": true}));
    assert!(r.is_ok());
    let t = active_text(&s).unwrap();
    let style = t.char_runs().first().unwrap().style.clone();
    assert!(type_extra_cmds::feature_on(&style, "liga"));

    let r = s.execute("type.openType.standardLigatures", json!({"on": false}));
    assert!(r.is_ok());
    let t = active_text(&s).unwrap();
    let style = t.char_runs().first().unwrap().style.clone();
    assert!(!type_extra_cmds::feature_on(&style, "liga"));

    let r = s.execute("type.openType.swash", json!({"on": true, "range": [0, 1]}));
    assert!(r.is_ok());
    let t = active_text(&s).unwrap();
    let found = t.char_runs().iter().any(|r| type_extra_cmds::feature_on(&r.style, "swsh"));
    assert!(found, "swash feature should be enabled on selected range");
}

// ---------- point / paragraph conversion ----------

#[test]
fn convert_to_point_text_and_back_preserves_shape() {
    let mut s = Session::new();
    new_doc(&mut s);
    new_text(&mut s);

    assert!(matches!(active_text(&s).unwrap().shape, TextShape::Box { .. }));

    let r = s.execute("type.convertToPointText", json!({}));
    assert!(r.is_ok());
    assert!(matches!(active_text(&s).unwrap().shape, TextShape::Point));

    let r = s.execute("type.convertToParagraphText", json!({}));
    assert!(r.is_ok());
    assert!(matches!(active_text(&s).unwrap().shape, TextShape::Box { .. }));
}

// ---------- outlines / shape / work path ----------

#[test]
fn convert_to_shape_replaces_text_layer_with_shape() {
    let mut s = Session::new();
    new_doc(&mut s);
    new_text(&mut s);

    let r = s.execute("type.convertToShape", json!({}));
    assert!(r.is_ok());

    let d = s.active().unwrap();
    let id = d.active_layer.unwrap();
    let layer = d.doc.layer(id).unwrap();
    assert!(matches!(layer.content, LayerContent::Shape(_)), "layer should be shape after convertToShape");
}

#[test]
fn create_work_path_sets_document_work_path() {
    let mut s = Session::new();
    new_doc(&mut s);
    new_text(&mut s);

    let r = s.execute("type.createWorkPath", json!({}));
    assert!(r.is_ok());
    let subpaths = r.unwrap()["subpaths"].as_u64().unwrap() as usize;

    let d = s.active().unwrap();
    let work_path = d.doc.work_path.as_ref().expect("work_path should be set");
    assert_eq!(work_path.subpaths.len(), subpaths);
    assert!(subpaths > 0, "text should produce at least one subpath");
}

// ---------- warp ----------

#[test]
fn warp_text_valid_style_updates_text_layer() {
    let mut s = Session::new();
    new_doc(&mut s);
    new_text(&mut s);

    let r = s.execute("type.warpText", json!({"style": "arc", "bend": 25.0, "horizontalDistortion": -10.0}));
    assert!(r.is_ok());

    let t = active_text(&s).unwrap();
    let warp = t.warp.as_ref().expect("warp should be set");
    assert_eq!(warp.value, 25.0);
    assert_eq!(warp.horizontal_distortion, -10.0);
}

#[test]
fn warp_text_invalid_style_returns_bad_params() {
    let mut s = Session::new();
    new_doc(&mut s);
    new_text(&mut s);

    let err = s.execute("type.warpText", json!({"style": "notAStyle"})).unwrap_err();
    assert!(matches!(err, EngineError::BadParams { .. }));
}

// ---------- lorem ipsum / update / fonts / defaults ----------

#[test]
fn paste_lorem_ipsum_inserts_into_active_text_layer() {
    let mut s = Session::new();
    new_doc(&mut s);
    new_text(&mut s); // text is "Hello"

    let r = s.execute("type.pasteLoremIpsum", json!({"at": 0}));
    assert!(r.is_ok());

    let t = active_text(&s).unwrap();
    assert!(t.text.starts_with(type_extra_cmds::LOREM_IPSUM));
    assert!(t.text.ends_with("Hello"));
}

#[test]
fn paste_lorem_ipsum_creates_new_layer_when_no_text_layer_active() {
    let mut s = Session::new();
    new_doc(&mut s); // no text layer

    let r = s.execute("type.pasteLoremIpsum", json!({"new": true, "size": 18.0}));
    assert!(r.is_ok());

    let d = s.active().unwrap();
    let text_layers: Vec<_> = d.doc.walk().into_iter().filter(|(_, _, l)| matches!(l.content, LayerContent::Text(_))).collect();
    assert_eq!(text_layers.len(), 1, "expected exactly one new text layer");

    if let LayerContent::Text(t) = &text_layers[0].2.content {
        assert_eq!(t.text, type_extra_cmds::LOREM_IPSUM);
    } else {
        panic!("new layer is not text");
    }
}

#[test]
fn missing_fonts_is_empty_for_default_font_family() {
    let mut s = Session::new();
    new_doc(&mut s);
    new_text(&mut s);

    let d = s.active().unwrap();
    let missing = type_extra_cmds::missing_fonts(&d.doc);
    assert!(missing.is_empty(), "default font should be installed");
}

#[test]
fn update_all_text_layers_reports_updated_count() {
    let mut s = Session::new();
    new_doc(&mut s);
    new_text(&mut s);

    let r = s.execute("type.updateAllTextLayers", json!({}));
    assert!(r.is_ok());
    let updated = r.unwrap()["updated"].as_u64().unwrap();
    assert_eq!(updated, 1, "exactly one text layer should have been refreshed");
}

#[test]
fn save_and_load_default_type_styles_roundtrip() {
    let mut s = Session::new();
    new_doc(&mut s);
    new_text(&mut s);

    let r = s.execute("type.saveDefaultTypeStyles", json!({}));
    assert!(r.is_ok());
    assert!(s.type_defaults.is_some(), "defaults should be saved");

    let r = s.execute("type.loadDefaultTypeStyles", json!({}));
    assert!(r.is_ok(), "loading defaults should succeed");
}

#[test]
fn replace_all_missing_fonts_reports_zero_when_none_missing() {
    let mut s = Session::new();
    new_doc(&mut s);
    new_text(&mut s);

    let r = s.execute("type.replaceAllMissingFonts", json!({}));
    assert!(r.is_ok());
    let v = r.unwrap();
    assert_eq!(v["missing"], json!([]));
    assert_eq!(v["replaced"], 0);
}

// ---------- error paths ----------

#[test]
fn error_paths_without_document_do_not_panic() {
    let mut s = Session::new();
    let commands = [
        "type.antiAlias.smooth",
        "type.orientation.horizontal",
        "type.openType.standardLigatures",
        "type.createWorkPath",
        "type.convertToShape",
        "type.convertToPointText",
        "type.warpText",
        "type.updateAllTextLayers",
        "type.replaceAllMissingFonts",
        "type.pasteLoremIpsum",
    ];

    for cmd in commands {
        let result = s.execute(cmd, json!({}));
        assert!(result.is_err(), "command `{cmd}` should return an error");
    }
}

#[test]
fn error_path_without_text_layer_returns_error() {
    let mut s = Session::new();
    new_doc(&mut s); // no text layer

    let err = s.execute("type.antiAlias.smooth", json!({})).unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("not a type layer") || msg.contains("type layer"), "expected type-layer error, got: {msg}");
}

#[test]
fn disabled_reason_reports_no_document_for_type_commands() {
    let s = Session::new();
    let reason = s.disabled_reason("type.antiAlias.smooth");
    assert_eq!(reason.as_deref(), Some("no document open"));
}
