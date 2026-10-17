use photocraft_engine::doc::text::AntiAlias;
use photocraft_engine::doc::{LayerContent, TextLayer};
use photocraft_engine::{Session, type_extra_cmds};
use serde_json::json;

// Helpers

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

// Public constants & free functions

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

// Lorem ipsum / update / fonts / defaults

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
fn missing_fonts_is_empty_for_default_font_family() {
    let mut s = Session::new();
    new_doc(&mut s);
    new_text(&mut s);

    let d = s.active().unwrap();
    let missing = type_extra_cmds::missing_fonts(&d.doc);
    assert!(missing.is_empty(), "default font should be installed");
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

// Error paths

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
