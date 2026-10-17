//! Panel pointer/type acceptance using an explicit synthetic persistence provider. XMP here
//! is ONLY the test bridge: native Graphic Styles field round-trip is covered elsewhere.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use super::*;
use egui::vec2;
use egui_kittest::{Harness, kittest::Queryable};
use photocraft_doc::{Color, ColorMode, Fill, Layer, LayerContent, LayerId, Path, SampleType, ShapeLayer, Size, Subpath};
struct Host {
    session: Session,
    panel: State,
    ids: Vec<LayerId>,
    error: Option<String>,
}
fn hooks() -> Hooks {
    Hooks {
        read: |d| d.metadata.xmp.as_deref().map_or_else(|| Ok(Store::default()), |s| serde_json::from_str(s).map_err(|e| EngineError::Other(e.to_string()))),
        write: |d, s| {
            d.metadata.xmp = Some(serde_json::to_string(s).map_err(|e| EngineError::Other(e.to_string()))?);
            Ok(())
        },
    }
}
fn host() -> Host {
    let mut doc = Document::new("Styles panel", Size::new(120, 72), ColorMode::Rgb, SampleType::U8);
    let mut ids = Vec::new();
    for x in [10., 60.] {
        let mut sh = ShapeLayer {
            path: Path::new(vec![Subpath::polygon(&[(x, 10.), (x + 30., 10.), (x + 30., 50.), (x, 50.)])]),
            fill: Some(Fill::Solid(if x == 10. { Color::BLACK } else { Color::WHITE })),
            ..Default::default()
        };
        sh.cache = Some(photocraft_vector::render_shape(&sh, doc.pixel_format(), doc.bounds()));
        let l = Layer::new("Object", LayerContent::Shape(sh));
        ids.push(l.id);
        doc.layers.push(l);
    }
    let mut session = Session::new();
    session.add_document(doc, None);
    session.select_layer(ids[0]).unwrap();
    Host { session, panel: State::default(), ids, error: None }
}
fn harness() -> Harness<'static, Host> {
    Harness::builder().with_size(vec2(420., 650.)).build_ui_state(
        |ui, h| {
            h.error = show_with_hooks(ui, &mut h.panel, &mut h.session, Some(hooks())).err().map(|e| e.to_string());
        },
        host(),
    )
}
fn click(h: &mut Harness<'_, Host>, name: &str) {
    h.get_by_label(name).click();
    h.run_steps(2);
    assert!(h.state().error.is_none(), "{:?}", h.state().error);
}
fn library(h: &Host) -> Store {
    (hooks().read)(&h.session.active().unwrap().doc).unwrap()
}
fn replace_name(h: &mut Harness<'_, Host>, text: &str) {
    h.get_by_role_and_label(egui::accesskit::Role::TextInput, "Style name").focus();
    h.run_steps(1);
    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::A);
    h.get_by_role_and_label(egui::accesskit::Role::TextInput, "Style name").type_text(text);
    h.run_steps(2);
}

#[test]
fn styles38_ui_panel_create_type_apply_redefine_and_undo() {
    let mut h = harness();
    replace_name(&mut h, "Ink");
    click(&mut h, "New from selection");
    assert_eq!(library(h.state()).graphic_styles[0].name, "Ink");
    click(&mut h, "Ink");
    let id = h.state().ids[1];
    h.state_mut().session.select_layer(id).unwrap();
    h.run_steps(1);
    let before = h.state().session.active().unwrap().doc.clone();
    let button = h.get_by_label("Apply style");
    let pos = button.rect().center();
    h.event(egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed: true, modifiers: egui::Modifiers::NONE });
    h.run_steps(1);
    assert!(h.state().panel.preview.is_some());
    assert_eq!(h.state().session.active().unwrap().doc, before);
    h.event(egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed: false, modifiers: egui::Modifiers::NONE });
    h.run_steps(1);
    assert!(h.state().error.is_none(), "{:?}", h.state().error);
    assert_eq!(library(h.state()).links.len(), 2);
    h.state_mut()
        .session
        .edit("local opacity", |d, _| {
            d.layer_mut(id).unwrap().opacity = 0.4;
            Ok(())
        })
        .unwrap();
    h.run_steps(1);
    click(&mut h, "Redefine style");
    for id in &h.state().ids {
        assert_eq!(h.state().session.active().unwrap().doc.layer(*id).unwrap().opacity, 0.4);
    }
    assert!(h.state_mut().session.undo());
    assert_eq!(h.state().session.active().unwrap().doc.layer(h.state().ids[0]).unwrap().opacity, 1.);
}
#[test]
fn styles38_ui_panel_rename_duplicate_delete_breaklink_starters() {
    let mut h = harness();
    click(&mut h, "New from selection");
    click(&mut h, "Graphic Style 1");
    replace_name(&mut h, "Renamed");
    click(&mut h, "Rename style");
    assert_eq!(library(h.state()).graphic_styles[0].name, "Renamed");
    click(&mut h, "Duplicate style");
    assert_eq!(library(h.state()).graphic_styles.len(), 2);
    click(&mut h, "Break Link");
    assert!(library(h.state()).links.is_empty());
    click(&mut h, "Delete style");
    assert_eq!(library(h.state()).graphic_styles.len(), 1);
    click(&mut h, "Add starter library");
    assert_eq!(library(h.state()).graphic_styles.len(), 4);
}
#[test]
fn styles38_ui_panel_stale_press_release_publishes_nothing() {
    let mut h = harness();
    click(&mut h, "New from selection");
    click(&mut h, "Graphic Style 1");
    let id = h.state().ids[1];
    h.state_mut().session.select_layer(id).unwrap();
    h.run_steps(1);
    let pos = h.get_by_label("Apply style").rect().center();
    h.event(egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed: true, modifiers: egui::Modifiers::NONE });
    h.run_steps(1);
    assert!(h.state().panel.preview.is_some());
    h.state_mut()
        .session
        .edit("Unrelated", |d, _| {
            d.name = "Changed".into();
            Ok(())
        })
        .unwrap();
    let before = h.state().session.active().unwrap().doc.clone();
    h.event(egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed: false, modifiers: egui::Modifiers::NONE });
    h.run_steps(1);
    assert_eq!(h.state().session.active().unwrap().doc, before);
    assert!(h.state().panel.preview.is_none());
}
