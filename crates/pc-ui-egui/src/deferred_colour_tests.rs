//! The actual file-open/dialog path must preserve a deferred profile choice.
use super::*;
use photocraft_cms::Builtin;
use photocraft_color::{ColorMode, PixelFormat, SampleType};
use photocraft_doc::{Document, Layer, Size};
use serde_json::json;

fn source(missing: bool) -> Document {
    let mut doc = Document::new("Profile source", Size::new(2, 1), ColorMode::Rgb, SampleType::U16);
    if !missing {
        doc.icc_profile = Some(Builtin::DisplayP3.profile().to_bytes());
    }
    let mut layer = Layer::raster("Pixels", PixelFormat::new(ColorMode::Rgb, SampleType::U16, true));
    layer.surface_mut().unwrap().write_region(doc.bounds(), &[0.25, 0.5, 0.75, 0.5, 0.75, 0.25, 0.5, 1.]);
    doc.layers.push(layer);
    doc
}
fn pixels(app: &PhotocraftApp) -> Vec<f32> {
    let doc = &app.session.active().unwrap().doc;
    doc.layers[0].surface().unwrap().read_region(doc.bounds())
}
fn open(source: &Document, background: bool, missing: bool) -> PhotocraftApp {
    let bytes = photocraft_format::save_to_bytes(source, &Default::default()).unwrap();
    let mut app = PhotocraftApp::new(
        Session::new(),
        Services {
            import: Some(Box::new(|name, bytes| photocraft_io::import(name, bytes).map(|r| (r.document, r.warnings)).map_err(|e| e.to_string()))),
            ..Default::default()
        },
    );
    app.background_jobs = background;
    app.session.color.settings.policy_rgb = photocraft_engine::color_cmds::Policy::Convert;
    app.session.color.settings.working_rgb = if missing { "adobe-rgb-compat" } else { "srgb" }.into();
    app.session.color.settings.ask_on_mismatch = true;
    app.session.color.settings.ask_on_missing = missing;
    app.open_bytes("external.pcraft", &bytes).unwrap();
    let ctx = egui::Context::default();
    let start = std::time::Instant::now();
    while app.session.has_jobs() && start.elapsed() < std::time::Duration::from_secs(5) {
        crate::jobs_ui::tick(&mut app, &ctx);
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    assert!(!app.session.has_jobs());
    app
}
#[test]
fn deferred_colour_actual_open_preserve_cancel_convert_and_discard() {
    let source = source(false);
    let before = source.layers[0].surface().unwrap().read_region(source.bounds());
    for background in [false, true] {
        for choice in ["cancel", "preserve", "convert", "discard"] {
            let mut app = open(&source, background, false);
            assert_eq!(app.session.active().unwrap().doc.icc_profile, source.icc_profile);
            assert_eq!(pixels(&app), before);
            let dialog = app.ui.dialogs.iter().find(|dialog| dialog.fields.get("__prefsui").and_then(Value::as_str) == Some("mismatch")).unwrap().id;
            let fields = &mut app.ui.dialog_mut(dialog).unwrap().fields;
            assert_eq!(fields["action"], "convert");
            assert_eq!(fields["applied"], "preserve");
            if choice == "cancel" {
                crate::dialogs::cancel(&mut app, dialog);
            } else {
                fields.insert("action".into(), json!(choice));
                crate::dialogs::confirm(&mut app, dialog).unwrap();
            }
            match choice {
                "convert" => {
                    assert!(photocraft_engine::color_cmds::document_profile(&app.session.active().unwrap().doc).same_colors(Builtin::Srgb.profile()));
                    assert_ne!(pixels(&app), before);
                    assert!(app.session.undo());
                }
                "discard" => {
                    assert!(app.session.active().unwrap().doc.icc_profile.is_none());
                    assert_eq!(pixels(&app), before);
                    assert!(app.session.undo());
                }
                _ => {}
            }
            assert_eq!(app.session.active().unwrap().doc.icc_profile, source.icc_profile);
            assert_eq!(pixels(&app), before);
        }
    }
}
#[test]
fn deferred_colour_untagged_choice_is_not_preassigned() {
    let source = source(true);
    for choice in ["preserve", "assignWorking"] {
        let mut app = open(&source, true, true);
        assert!(app.session.active().unwrap().doc.icc_profile.is_none());
        let before = pixels(&app);
        let dialog = app.ui.dialogs[0].id;
        app.ui.dialog_mut(dialog).unwrap().fields.insert("action".into(), json!(choice));
        crate::dialogs::confirm(&mut app, dialog).unwrap();
        assert_eq!(pixels(&app), before);
        assert_eq!(app.session.active().unwrap().doc.icc_profile.is_some(), choice == "assignWorking");
    }
}
