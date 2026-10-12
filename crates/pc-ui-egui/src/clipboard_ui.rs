//! Colour decisions are made before paste changes history, against a captured document.
use crate::{PhotocraftApp, state::DialogKind};
use photocraft_engine::Session;
use serde_json::{Map, Value, json};

fn target(session: &Session) -> Option<Value> {
    let doc = session.active()?;
    Some(json!({"doc":doc.doc.id.0,"revision":doc.revision,"active":doc.active_layer.map(|id|id.0),
        "layers":doc.selected_layers().iter().map(|id|id.0).collect::<Vec<_>>()}))
}
pub(crate) fn clipboard(session: &Session) -> Option<Value> {
    let clip = session.clipboard.as_ref()?;
    let b = clip.bounds;
    Some(json!({"revision":clip.surface.revision(),"profile":clip.profile.content_hash(),"bounds":[b.x0,b.y0,b.x1,b.y1]}))
}

/// Called while the intended destination is active, including within a file job's apply phase.
pub(crate) fn request(session: &Session, command: &str, params: &Value) -> Option<Value> {
    if !session.color.settings.ask_on_paste
        || params.get("colorPolicy").is_some()
        || !matches!(command, "edit.paste" | "edit.pasteSpecial.pasteInPlace" | "edit.pasteSpecial.pasteSeamless")
    {
        return None;
    }
    let doc = session.active()?;
    let clip = session.clipboard.as_ref()?;
    let destination = photocraft_engine::color_cmds::document_profile(&doc.doc);
    if clip.surface.format().mode == doc.doc.mode && clip.profile.same_colors(&destination) {
        return None;
    }
    Some(json!({"__prefsui":"clipboard","__label":"Paste Colour Profile", "command":command,"params":params,
        "target":target(session)?,"clipboard":clipboard(session)?,"action":"convert",
        "source":clip.profile.description,"destination":destination.description,"document":doc.doc.name,
        "sameMode":clip.surface.format().mode==doc.doc.mode}))
}

pub(crate) fn open(app: &mut PhotocraftApp, fields: Value) -> Value {
    let id = app.ui.open_dialog(DialogKind::Command, fields.as_object().cloned().unwrap_or_default());
    json!({"dialog":id,"pending":true,"pasted":false})
}

pub(crate) fn body(ui: &mut egui::Ui, fields: &mut Map<String, Value>) {
    ui.label(format!("Paste into {}", fields.get("document").and_then(Value::as_str).unwrap_or("document")));
    ui.label(format!(
        "Copied colours: {}\nDestination: {}",
        fields.get("source").and_then(Value::as_str).unwrap_or("?"),
        fields.get("destination").and_then(Value::as_str).unwrap_or("?")
    ));
    ui.add_space(8.0);
    let mut choice = fields.get("action").and_then(Value::as_str).unwrap_or("convert").to_string();
    ui.radio_value(&mut choice, "convert".into(), "Convert to preserve appearance");
    ui.add_enabled_ui(fields.get("sameMode").and_then(Value::as_bool) == Some(true), |ui| {
        ui.radio_value(&mut choice, "numbers".into(), "Keep colour numbers (appearance may change)");
    });
    ui.weak("Cancel leaves the document unchanged.");
    fields.insert("action".into(), json!(choice));
}

pub(crate) fn confirm(app: &mut PhotocraftApp, fields: &Map<String, Value>) -> Result<Value, String> {
    let expected = fields.get("target").ok_or("The paste destination is missing; copy and paste again")?;
    let doc_id = expected["doc"].as_u64().ok_or("The paste destination is missing")?;
    let destination = app.session.documents().iter().position(|doc| doc.doc.id.0 == doc_id).ok_or("The paste destination was closed; paste again")?;
    if clipboard(&app.session).as_ref() != fields.get("clipboard") {
        return Err("The clipboard changed; paste again".into());
    }
    let viewed = app.session.active().map(|doc| doc.doc.id);
    app.session.set_active(destination);
    let result = (|| {
        if target(&app.session).as_ref() != Some(expected) {
            return Err("The destination document or selection changed; paste again".into());
        }
        let command = fields.get("command").and_then(Value::as_str).ok_or("The paste command is missing")?;
        if !matches!(command, "edit.paste" | "edit.pasteSpecial.pasteInPlace" | "edit.pasteSpecial.pasteSeamless") {
            return Err("Invalid paste command".into());
        }
        let mut params = fields.get("params").cloned().unwrap_or_else(|| json!({}));
        let params_map = params.as_object_mut().ok_or("Invalid paste parameters; paste again")?;
        params_map.insert("colorPolicy".into(), fields.get("action").cloned().unwrap_or_else(|| json!("convert")));
        // A control request can confirm a dialog too: retain the same command
        // authorization and script-event suppression as PhotocraftApp::run.
        if app.automation_input
            && let Some(authorize) = app.services.automation_command.as_ref()
        {
            authorize(command, &params)?;
        }
        let gate = if app.automation_input && app.session.authorize.is_none() { app.services.automation_authorize } else { None };
        if gate.is_some() {
            app.session.authorize = gate;
        }
        let suppress = app.automation_input && app.session.prefs().script_events.enabled;
        if suppress {
            app.session.edit_prefs(|prefs| prefs.script_events.enabled = false);
        }
        let result = app.session.execute(command, params).map_err(|error| error.to_string());
        if suppress {
            app.session.edit_prefs(|prefs| prefs.script_events.enabled = true);
        }
        if gate.is_some() {
            app.session.authorize = None;
        }
        result
    })();
    if let Some(index) = viewed.and_then(|id| app.session.documents().iter().position(|doc| doc.doc.id == id)) {
        app.session.set_active(index);
    }
    app.sync_views();
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Services;
    use photocraft_cms::Builtin;
    use photocraft_color::{ColorMode, PixelFormat, SampleType};
    use photocraft_doc::{Document, Layer};
    use photocraft_geom::Size;
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    fn doc(profile: Builtin) -> Document {
        let mut doc = Document::new("Colour destination", Size::new(2, 2), ColorMode::Rgb, SampleType::F32);
        doc.icc_profile = Some(profile.profile().to_bytes());
        let mut layer = Layer::raster("Pixels", PixelFormat::new(ColorMode::Rgb, SampleType::F32, true));
        let surface = layer.surface_mut().unwrap();
        surface.write_region(doc.bounds(), &[0.25, 0.5, 0.1, 0.375].repeat(4));
        doc.layers.push(layer);
        doc
    }
    fn new_app() -> PhotocraftApp {
        let mut session = Session::new();
        session.add_document(doc(Builtin::LinearSrgb), None);
        session.execute("edit.copy", json!({})).unwrap();
        session.add_document(doc(Builtin::Srgb), None);
        let mut app = PhotocraftApp::new(session, Services::default());
        app.sync_views();
        app
    }
    fn pixel(app: &PhotocraftApp, index: usize) -> Vec<f32> {
        app.session.documents()[index].doc.layers.last().unwrap().surface().unwrap().pixel(0, 0)
    }
    #[test]
    fn clipboard_colour_prompt_cancel_numbers_convert_and_undo() {
        for choice in ["cancel", "numbers", "convert"] {
            let mut app = new_app();
            let revision = app.session.active().unwrap().revision;
            let value = app.run("edit.paste", json!({})).unwrap();
            let id = value["dialog"].as_u64().unwrap();
            assert_eq!(app.session.active().unwrap().revision, revision);
            assert_eq!(app.session.active().unwrap().doc.layers.len(), 1);
            if choice == "cancel" {
                crate::dialogs::cancel(&mut app, id);
                assert_eq!(app.session.active().unwrap().revision, revision);
            } else {
                app.ui.dialog_mut(id).unwrap().fields.insert("action".into(), json!(choice));
                crate::dialogs::confirm(&mut app, id).unwrap();
                assert_eq!(app.session.active().unwrap().doc.layers.len(), 2);
                let expected = if choice == "numbers" { 0.25 } else { 0.537099 };
                assert!((pixel(&app, 1)[0] - expected).abs() < 0.0001);
                assert_eq!(pixel(&app, 1)[3], 0.375);
                assert!(app.session.undo());
                assert_eq!(app.session.active().unwrap().doc.layers.len(), 1);
            }
        }
    }
    #[test]
    fn clipboard_colour_prompt_keeps_original_destination_and_rejects_stale_state() {
        let mut app = new_app();
        let value = app.run("edit.paste", json!({})).unwrap();
        app.session.set_active(0);
        app.sync_views();
        crate::dialogs::confirm(&mut app, value["dialog"].as_u64().unwrap()).unwrap();
        assert_eq!(app.session.active_index(), Some(0));
        assert_eq!(app.session.documents()[0].doc.layers.len(), 1);
        assert_eq!(app.session.documents()[1].doc.layers.len(), 2);
        for stale in ["clipboard", "document", "layer"] {
            let mut app = new_app();
            let value = app.run("edit.paste", json!({})).unwrap();
            match stale {
                "clipboard" => {
                    app.session.clipboard.as_mut().unwrap().surface.write_region(photocraft_geom::Rect::from_xywh(0, 0, 1, 1), &[0., 0., 0., 1.]);
                }
                "document" => {
                    app.session.execute("layer.new.layer", json!({"name":"Changed"})).unwrap();
                }
                _ => {
                    app.session.active_mut().unwrap().active_layer = None;
                }
            }
            let count = app.session.active().unwrap().doc.layers.len();
            assert!(crate::dialogs::confirm(&mut app, value["dialog"].as_u64().unwrap()).is_err(), "{stale}");
            assert_eq!(app.session.active().unwrap().doc.layers.len(), count);
        }
    }
    #[test]
    fn clipboard_colour_os_image_is_read_once_before_prompt() {
        let reads = Arc::new(AtomicUsize::new(0));
        let count = reads.clone();
        let mut app = new_app();
        app.session.set_active(0); // linear destination, OS image is sRGB
        app.services.clipboard_get_image = Some(Box::new(move || {
            count.fetch_add(1, Ordering::SeqCst);
            Some((1, 1, vec![128, 64, 32, 96]))
        }));
        let value = app.run("edit.paste", json!({})).unwrap();
        assert_eq!(reads.load(Ordering::SeqCst), 1);
        crate::dialogs::confirm(&mut app, value["dialog"].as_u64().unwrap()).unwrap();
        assert_eq!(reads.load(Ordering::SeqCst), 1);
        assert!((pixel(&app, 0)[0] - 0.21586).abs() < 0.0001);
    }
    #[test]
    fn clipboard_colour_explicit_or_automation_policy_does_not_open_prompt() {
        for automation in [false, true] {
            let mut app = new_app();
            app.automation_input = automation;
            let params = if automation { json!({}) } else { json!({"colorPolicy":"numbers"}) };
            app.run("edit.paste", params).unwrap();
            assert!(app.ui.dialogs.is_empty());
            assert_eq!(app.session.active().unwrap().doc.layers.len(), 2);
        }
        let mut app = new_app();
        app.session.color.settings.ask_on_paste = false;
        app.run("edit.paste", json!({})).unwrap();
        assert!(app.ui.dialogs.is_empty());
        assert!((pixel(&app, 1)[0] - 0.537099).abs() < 0.0001);
    }
    #[test]
    fn clipboard_colour_copied_file_job_keeps_profile_depth_and_destination() {
        let path = std::env::temp_dir().join(format!("li-clipboard-colour-{}.pcraft", std::process::id()));
        struct Cleanup(std::path::PathBuf);
        impl Drop for Cleanup {
            fn drop(&mut self) {
                let _ = std::fs::remove_file(&self.0);
            }
        }
        let _cleanup = Cleanup(path.clone());
        let bytes = photocraft_format::save_to_bytes(&doc(Builtin::LinearSrgb), &Default::default()).unwrap();
        std::fs::write(&path, bytes).unwrap();
        let mut app = new_app();
        app.services.clipboard_get_files = Some(Box::new(move || vec![path.to_string_lossy().into_owned()]));
        let value = crate::menus::invoke(&mut app, &egui::Context::default(), "edit.paste", json!({})).unwrap();
        assert!(value.get("job").is_some());
        app.session.set_active(0);
        let start = std::time::Instant::now();
        let ctx = egui::Context::default();
        while app.ui.dialogs.is_empty() && start.elapsed() < std::time::Duration::from_secs(5) {
            crate::jobs_ui::tick(&mut app, &ctx);
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        assert_eq!(app.ui.dialogs.len(), 1, "{}", app.ui.status);
        let clip = app.session.clipboard.as_ref().unwrap();
        assert_eq!(clip.surface.format().sample, SampleType::F32);
        assert!(clip.profile.same_colors(Builtin::LinearSrgb.profile()));
        assert_eq!(app.session.documents()[1].doc.layers.len(), 1, "prompt must not have pasted yet");
        let id = app.ui.dialogs[0].id;
        crate::dialogs::confirm(&mut app, id).unwrap();
        assert_eq!(app.session.active_index(), Some(0));
        assert_eq!(app.session.documents()[0].doc.layers.len(), 1);
        assert_eq!(app.session.documents()[1].doc.layers.len(), 2);
        assert!((pixel(&app, 1)[0] - 0.537099).abs() < 0.0001);
        // A later copy wins over an older completed decode awaiting UI application.
        let value = app.run("edit.paste", json!({})).unwrap();
        let newer = [0.9, 0.8, 0.7, 0.6];
        app.session.clipboard.as_mut().unwrap().surface.write_region(photocraft_geom::Rect::from_xywh(0, 0, 1, 1), &newer);
        let job = photocraft_engine::jobs::JobId(value["job"].as_u64().unwrap());
        assert!(app.session.wait_job(job).is_err());
        assert_eq!(app.session.clipboard.as_ref().unwrap().surface.pixel(0, 0), newer);
        assert_eq!(app.session.documents()[0].doc.layers.len(), 1);
        assert_eq!(app.session.documents()[1].doc.layers.len(), 2);
    }
    #[test]
    fn clipboard_colour_control_confirmation_keeps_authorization() {
        let mut app = new_app();
        let prompt = app.run("edit.paste", json!({})).unwrap();
        app.session.set_active(0);
        app.automation_input = true;
        app.services.automation_command = Some(Box::new(|command, _| if command == "edit.paste" { Err("denied paste".into()) } else { Ok(()) }));
        let error = crate::dialogs::confirm(&mut app, prompt["dialog"].as_u64().unwrap()).unwrap_err();
        assert_eq!(error, "denied paste");
        assert_eq!(app.session.active_index(), Some(0));
        assert_eq!(app.session.documents()[1].doc.layers.len(), 1);
    }
    #[test]
    fn clipboard_colour_file_paste_without_document_does_not_adopt_later_tab() {
        let path = std::env::temp_dir().join(format!("li-clipboard-no-document-{}.ppm", std::process::id()));
        std::fs::write(&path, b"P6\n1 1\n255\n\x80\x40\x20").unwrap();
        let source = path.clone();
        let mut app = PhotocraftApp::new(
            Session::new(),
            Services { clipboard_get_files: Some(Box::new(move || vec![source.to_string_lossy().into_owned()])), ..Default::default() },
        );
        let pending = app.run("edit.paste", json!({})).unwrap();
        let unrelated = doc(Builtin::Srgb);
        let original = unrelated.clone();
        app.session.add_document(unrelated, None);
        let result = app.session.wait_job(photocraft_engine::jobs::JobId(pending["job"].as_u64().unwrap())).unwrap();
        assert_eq!(app.session.documents().len(), 2);
        assert!(app.session.documents()[0].doc.as_ref() == &original, "the later-opened document changed");
        assert_eq!(result["document"], 1);
        assert_eq!(app.session.documents()[1].doc.size, Size::new(1, 1));
        std::fs::remove_file(path).unwrap();
    }
    #[test]
    fn clipboard_colour_dialog_visual_and_keyboard_flow() {
        use egui::accesskit::Role;
        use egui_kittest::{Harness, kittest::Queryable};
        #[derive(Default)]
        struct CpuRenderer(crate::tool_icon_tests::cpu::TextureStore);
        impl egui_kittest::TestRenderer for CpuRenderer {
            fn handle_delta(&mut self, delta: &mut egui::TexturesDelta) {
                self.0.apply(std::mem::take(delta));
            }
            fn render(&mut self, ctx: &egui::Context, output: &egui::FullOutput) -> Result<image::RgbaImage, String> {
                let scale = output.pixels_per_point;
                let size = ctx.content_rect().size() * scale;
                let primitives = ctx.tessellate(output.shapes.clone(), scale);
                let image =
                    crate::tool_icon_tests::cpu::paint(&primitives, &self.0, [size.x as usize, size.y as usize], scale, crate::theme::Tokens::get(ctx).canvas);
                image::RgbaImage::from_raw(image.size[0] as u32, image.size[1] as u32, image.pixels.iter().flat_map(|p| p.to_array()).collect())
                    .ok_or_else(|| "invalid CPU image size".into())
            }
        }
        let app = new_app();
        let mut harness = Harness::builder().with_size(egui::vec2(1000., 700.)).with_max_steps(32).renderer(CpuRenderer::default()).build_eframe(move |cc| {
            PhotocraftApp::setup_context(&cc.egui_ctx, Default::default());
            app
        });
        harness.run_steps(4);
        harness.state_mut().run("edit.paste", json!({})).unwrap();
        harness.run_steps(4);
        harness.get_by_label("Convert to preserve appearance");
        harness.get_by_label("Keep colour numbers (appearance may change)");
        harness.get_by_role_and_label(Role::Button, "Cancel");
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/clipboard-colour-ux");
        std::fs::create_dir_all(&dir).unwrap();
        harness.render().unwrap().save(dir.join("paste-colour-dialog.png")).unwrap();
        harness.key_press(egui::Key::Escape);
        harness.run_steps(3);
        assert!(harness.state().ui.dialogs.is_empty());
        assert_eq!(harness.state().session.documents()[1].doc.layers.len(), 1);
        harness.state_mut().run("edit.paste", json!({})).unwrap();
        harness.run_steps(4);
        harness.get_by_label("Keep colour numbers (appearance may change)").click();
        harness.run_steps(2);
        harness.get_by_role_and_label(Role::Button, "OK").click();
        harness.run_steps(3);
        assert!(harness.state().ui.dialogs.is_empty());
        assert_eq!(pixel(harness.state(), 1), vec![0.25, 0.5, 0.1, 0.375]);
    }
}
