use egui_kittest::{Harness, kittest::Queryable};
use photocraft_engine::web_cmds::{self, WebSettings};
use photocraft_ui_egui::{PhotocraftApp, file_ui, theme::ThemeKind};
use serde_json::json;

#[test]
fn failed_preview_shows_the_error_without_a_texture_and_recovers_after_settings_change() {
    let invalid = json!({"format": "unsupported"});
    let error = WebSettings::from_params(&invalid, "preview").unwrap_err().to_string();
    let mut h = Harness::builder().with_size(egui::vec2(1600.0, 1100.0)).build_eframe(|cc| {
        PhotocraftApp::setup_context(&cc.egui_ctx, ThemeKind::Pro);
        let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), Default::default());
        app.run("file.new", json!({"width": 120, "height": 80})).unwrap();
        let id = file_ui::open_web(&mut app);
        let fields = &mut app.ui.dialog_mut(id).unwrap().fields;
        fields.insert("__view".into(), json!("optimized"));
        fields.insert("format".into(), json!("unsupported"));
        fields.remove("percent");
        fields.insert("width".into(), json!(60));
        fields.insert("height".into(), json!(40));
        app
    });
    h.run_steps(4);
    assert!(h.query_by_label(&error).is_some(), "the pane must show the actual settings error");
    assert!(!h.ctx.tex_manager().read().allocated().any(|(_, meta)| meta.name == "web-pane"), "a failed preview must not upload success-shaped pixels");
    h.run_steps(4);
    assert!(h.query_by_label(&error).is_some(), "the failure remains visible across repaints");

    h.state_mut().ui.dialogs[0].fields.insert("format".into(), json!("png24"));
    h.run_steps(4);
    assert!(h.query_by_label(&error).is_none());
    let texture = h.ctx.tex_manager().read().allocated().find(|(_, meta)| meta.name == "web-pane").map(|(id, meta)| (*id, meta.size)).unwrap();
    assert_eq!(texture.1, [60, 40]);
    let document = &h.state().session.active().unwrap().doc;
    let params = json!({"format": "png24", "width": 60, "height": 40});
    let settings = WebSettings::from_params(&params, "preview").unwrap();
    let (resized, _, _) = web_cmds::web_document(document, &params, &settings).unwrap();
    let pixels = photocraft_compose::flatten(&resized);
    let optimized = web_cmds::optimize(&pixels.px, 60, photocraft_geom::Rect::new(0, 0, 60, 40), &settings, None, None, 72.0, true).unwrap();
    let label = format!("PNG  {:.0}K", (optimized.bytes.len() as f64 / 1024.0).max(1.0));
    assert!(h.query_by_label(&label).is_some(), "the successful preview retains its byte estimate");
    h.run_steps(4);
    assert!(h.ctx.tex_manager().read().meta(texture.0).is_some(), "unchanged settings reuse the successful texture");

    h.state_mut().ui.dialogs[0].fields.insert("format".into(), json!("unsupported"));
    h.run_steps(4);
    assert!(h.query_by_label(&error).is_some());
    assert!(!h.ctx.tex_manager().read().allocated().any(|(_, meta)| meta.name == "web-pane"), "a failure must replace the previous success");
    h.state_mut().ui.dialogs[0].fields.insert("format".into(), json!("png24"));
    h.run_steps(4);
    assert!(h.query_by_label(&error).is_none());
    assert!(h.query_by_label(&label).is_some());
}
