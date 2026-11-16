use egui::{Event, Key, Modifiers, accesskit::Role};
use egui_kittest::{
    Harness,
    kittest::{NodeT, Queryable},
};
use serde_json::{Value, json};

use crate::{PhotocraftApp, Services, state::DialogKind};

const COMMAND: &str = "file.scripts.imageProcessor";

fn fonts_ready(ui: &egui::Ui) -> bool {
    ui.ctx().fonts(|fonts| fonts.definitions().families.contains_key(&egui::FontFamily::Name("medium".into())))
}

fn draw_dialogs(ui: &mut egui::Ui, app: &mut PhotocraftApp) {
    if fonts_ready(ui) {
        crate::dialogs::show(app, ui.ctx());
    }
}

fn open() -> (Harness<'static, PhotocraftApp>, u64) {
    let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), Services::default());
    app.run("file.new", json!({"width": 40, "height": 20})).unwrap();
    let mut h = Harness::builder().with_size(egui::vec2(1000.0, 760.0)).build_ui_state(draw_dialogs, app);
    PhotocraftApp::setup_context(&h.ctx, Default::default());
    let ctx = h.ctx.clone();
    let id = crate::menus::invoke(h.state_mut(), &ctx, COMMAND, json!({})).unwrap()["dialog"].as_u64().unwrap();
    h.run_steps(4);
    (h, id)
}

fn field(h: &Harness<'_, PhotocraftApp>, id: u64, key: &str) -> Value {
    h.state().ui.dialogs.iter().find(|dialog| dialog.id == id).unwrap().fields[key].clone()
}

fn edit_number(h: &mut Harness<'static, PhotocraftApp>, label: &str, input: &str) {
    h.get_by_role_and_label(Role::SpinButton, label).click();
    h.run_steps(2);
    h.key_press_modifiers(Modifiers::COMMAND, Key::A);
    if input.is_empty() {
        h.key_press(Key::Backspace);
    } else {
        h.event(Event::Text(input.into()));
    }
    h.run_steps(1);
    h.key_press(Key::Tab);
    h.run_steps(4);
}

fn alert(h: &Harness<'_, PhotocraftApp>, message: &str) {
    let dialog = h.state().ui.dialogs.last().unwrap();
    assert_eq!(dialog.kind, DialogKind::Error);
    assert_eq!(dialog.fields["message"], message);
}

fn dismiss(h: &mut Harness<'_, PhotocraftApp>) {
    h.get_by_role_and_label(Role::Button, "OK").click();
    h.run_steps(4);
}

/// Upstream 11c77d26: the File Type section's defaults, and Run with no type checked.
#[test]
fn defaults_and_no_format_alert_preserve_the_dialog() {
    let (mut h, id) = open();
    assert_eq!(field(&h, id, "jpeg"), json!({"enabled": true, "quality": 5.0, "convertToSrgb": false, "resize": false, "width": 1.0, "height": 1.0}));
    assert_eq!(field(&h, id, "psd"), json!({"enabled": false, "resize": false, "width": 1.0, "height": 1.0}));
    assert_eq!(field(&h, id, "tiff"), json!({"enabled": false, "lzw": false, "resize": false, "width": 1.0, "height": 1.0}));
    assert!(!h.get_by_label("Convert Profile to sRGB").accesskit_node().is_disabled());
    assert!(h.get_by_label("LZW Compression").accesskit_node().is_disabled());
    assert!(h.query_all_by_role_and_label(Role::SpinButton, "W").all(|node| node.accesskit_node().is_disabled()));
    h.get_by_role_and_label(Role::CheckBox, "Save as JPEG").click();
    h.run_steps(4);
    let output = field(&h, id, "output");
    h.get_by_role_and_label(Role::Button, "Run").click();
    h.run_steps(4);
    alert(&h, super::NO_FORMAT);
    assert_eq!(field(&h, id, "output"), output);
    dismiss(&mut h);
    assert_eq!(h.state().ui.dialogs.len(), 1);
    assert_eq!(field(&h, id, "jpeg")["enabled"], false);
}

/// Upstream 11c77d26: Run writes every checked type, each at its own size.
#[test]
fn run_dispatches_all_enabled_formats_with_their_own_dimensions() {
    let dir = std::env::temp_dir().join(format!("li-image-processor-ui-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let (input, output) = (dir.join("in"), dir.join("out"));
    std::fs::create_dir_all(&input).unwrap();
    let mut source = photocraft_engine::Session::new();
    source.execute("file.new", json!({"width": 40, "height": 20})).unwrap();
    source.execute("file.saveACopy", json!({"path": input.join("a.png")})).unwrap();

    let (mut h, id) = open();
    let fields = &mut h.state_mut().ui.dialog_mut(id).unwrap().fields;
    fields.insert("input".into(), json!(input));
    fields.insert("output".into(), json!(output));
    fields.insert("jpeg".into(), json!({"enabled": true, "resize": true, "width": 20, "height": 20}));
    fields.insert("psd".into(), json!({"enabled": true, "resize": true, "width": 10, "height": 10}));
    fields.insert("tiff".into(), json!({"enabled": true}));
    h.run_steps(4);
    h.get_by_role_and_label(Role::Button, "Run").click();
    h.run_steps(4);
    assert!(h.state().ui.dialogs.is_empty(), "{:?}", h.state().ui.status);
    let mut sizes: Vec<_> = std::fs::read_dir(&output)
        .unwrap()
        .map(|entry| {
            let path = entry.unwrap().path();
            let ext = path.extension().unwrap().to_str().unwrap().to_owned();
            let bytes = std::fs::read(&path).unwrap();
            let doc = photocraft_io::import(path.to_str().unwrap(), &bytes).unwrap().document;
            (ext, doc.size.width, doc.size.height)
        })
        .collect();
    sizes.sort();
    assert_eq!(sizes, [("jpg".into(), 20, 10), ("psd".into(), 10, 5), ("tif".into(), 40, 20)]);
    let _ = std::fs::remove_dir_all(&dir);
}

/// Upstream 11c77d26: a typed number out of range is clamped, one that is not a number keeps the
/// last value, and each shows its alert; one format's field leaves the others alone.
#[test]
fn numeric_fields_clamp_and_restore_the_last_value() {
    let (mut h, id) = open();
    edit_number(&mut h, "Quality", "7.6");
    assert_eq!(field(&h, id, "jpeg")["quality"], 7.6);
    for (input, value, message) in [
        ("abc", 7.6, super::invalid_entry("quality")),
        ("", 7.6, super::invalid_entry("quality")),
        ("NaN", 7.6, super::invalid_entry("quality")),
        ("20", 12.0, super::closest_value("quality")),
        ("0", 1.0, super::closest_value("quality")),
    ] {
        edit_number(&mut h, "Quality", input);
        alert(&h, message);
        assert_eq!(field(&h, id, "jpeg")["quality"], value);
        dismiss(&mut h);
    }
    // Only JPEG's Resize to Fit is enabled; it is the first.
    h.query_all_by_label("Resize to Fit").next().unwrap().click();
    h.run_steps(4);
    let width = h.query_all_by_role_and_label(Role::SpinButton, "W").next().unwrap();
    width.click();
    h.run_steps(2);
    h.key_press_modifiers(Modifiers::COMMAND, Key::A);
    h.event(Event::Text("40000".into()));
    h.key_press(Key::Enter);
    h.run_steps(4);
    alert(&h, super::closest_value("width"));
    assert_eq!(field(&h, id, "jpeg")["width"], 30_000.0);
    assert_eq!(field(&h, id, "psd")["width"].as_f64(), Some(1.0));
    assert_eq!(field(&h, id, "tiff")["width"].as_f64(), Some(1.0));
}
