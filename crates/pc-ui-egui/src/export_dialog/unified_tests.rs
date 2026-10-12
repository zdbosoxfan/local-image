#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use super::*;
use egui_kittest::{
    Harness,
    kittest::{By, Queryable},
};
use photocraft_doc::{Color, ColorMode, Fill, Layer, LayerContent, SampleType, ShapeLayer, Size};
use std::sync::{
    Mutex,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};
use std::time::{Duration, Instant};

#[derive(Default)]
struct Output {
    pick: AtomicBool,
    fail: AtomicBool,
    hold: AtomicBool,
    entered: AtomicBool,
    commits: AtomicUsize,
    names: Mutex<Vec<String>>,
    bytes: Mutex<Vec<Vec<u8>>>,
    settings: Mutex<Vec<ExportSettings>>,
}
fn fixture(output: &Arc<Output>) -> PhotocraftApp {
    let picker = output.clone();
    let writer = output.clone();
    let encoder = output.clone();
    let stage = output.clone();
    let services = crate::Services {
        pick_save: Some(Box::new(move |name| {
            picker.names.lock().unwrap().push(name.into());
            picker.pick.load(Ordering::SeqCst).then(|| format!("/exports/{name}"))
        })),
        export: Some(Box::new(move |_, _, settings| {
            encoder.settings.lock().unwrap().push(settings.clone());
            Ok((b"raster encoded".to_vec(), vec![]))
        })),
        write: Some(Box::new(move |_, bytes| {
            if writer.fail.load(Ordering::SeqCst) {
                return Err("destination not writable".into());
            }
            writer.commits.fetch_add(1, Ordering::SeqCst);
            writer.bytes.lock().unwrap().push(bytes.to_vec());
            Ok(())
        })),
        stage_write: Some(Arc::new(move |_, bytes, interrupt| {
            stage.entered.store(true, Ordering::SeqCst);
            let until = Instant::now() + Duration::from_secs(10);
            while stage.hold.load(Ordering::SeqCst) {
                interrupt.check().map_err(|e| e.to_string())?;
                if Instant::now() >= until {
                    return Err("fixture deadline".into());
                }
                std::thread::sleep(Duration::from_millis(2));
            }
            if stage.fail.load(Ordering::SeqCst) {
                return Err("destination not writable".into());
            }
            let stage = stage.clone();
            let bytes = bytes.to_vec();
            Ok(crate::PreparedWrite {
                commit: Box::new(move || {
                    stage.commits.fetch_add(1, Ordering::SeqCst);
                    stage.bytes.lock().unwrap().push(bytes);
                    Ok(())
                }),
            })
        })),
        ..Default::default()
    };
    let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), services);
    let mut doc = Document::new("Different title", Size::new(32, 32), ColorMode::Rgb, SampleType::U8);
    let shape = Layer::new(
        "Rectangle",
        LayerContent::Shape(ShapeLayer {
            path: photocraft_vector::shapes::rect(4., 5., 20., 18.),
            fill: Some(Fill::Solid(Color::BLACK)),
            ..Default::default()
        }),
    );
    let layer = shape.id;
    doc.layers.push(shape);
    app.session.add_document(doc, Some("/source/Photo version 2.photocraft".into()));
    app.session.select_layer(layer).unwrap();
    app
}
fn harness(app: PhotocraftApp, size: egui::Vec2) -> Harness<'static, PhotocraftApp> {
    let mut h = Harness::builder().with_size(size).build_ui_state(
        |ui, app| {
            let ctx = ui.ctx().clone();
            if !ctx.fonts(|fonts| fonts.families().contains(&egui::FontFamily::Name("semibold".into()))) {
                return;
            }
            if ui.button("Canvas action").clicked() {
                app.ui.status = "canvas touched".into();
            }
            crate::jobs_ui::tick(app, &ctx);
            crate::vector_packages::svg_export::frame(app, &ctx);
            crate::vector_packages::pdf_export::frame(app, &ctx);
            crate::dialogs::show(app, &ctx);
        },
        app,
    );
    PhotocraftApp::setup_context(&h.ctx, crate::theme::ThemeKind::Studio);
    h.run_steps(3);
    h
}
fn switch(h: &mut Harness<'_, PhotocraftApp>, from: &str, to: &str) {
    h.get(By::new().role(egui::accesskit::Role::ComboBox).value(from)).click();
    h.run();
    h.get_by_label(to).click();
    h.run();
}
fn fields(app: &PhotocraftApp) -> &Map<String, Value> {
    &app.ui.dialogs.iter().find(|d| d.fields.contains_key("__export")).unwrap().fields
}
fn spin(h: &mut Harness<'_, PhotocraftApp>, done: impl Fn(&PhotocraftApp) -> bool) {
    let until = Instant::now() + Duration::from_secs(10);
    while !done(h.state()) {
        assert!(Instant::now() < until, "export deadline");
        h.run_steps(1);
        std::thread::sleep(Duration::from_millis(2));
    }
    h.run_steps(2);
}
#[test]
fn unified_export_actual_format_switch_escape_discards_draft_and_preserves_prefs() {
    for size in [egui::vec2(1440., 900.), egui::vec2(760., 480.)] {
        let output = Arc::new(Output::default());
        let mut app = fixture(&output);
        let before = app.session.prefs().dialogs.clone();
        open(&mut app).unwrap();
        let mut h = harness(app, size);
        for (from, to) in [("PNG", "JPG"), ("JPG", "WebP"), ("WebP", "TIFF"), ("TIFF", "TGA"), ("TGA", "SVG")] {
            for label in ["Export", "Cancel"] {
                assert!(egui::Rect::from_min_size(egui::Pos2::ZERO, size).contains_rect(h.get_by_label(label).rect()), "{from} {label} must remain visible");
            }
            switch(&mut h, from, to);
        }
        h.get_by_label("Minify").click();
        h.run();
        h.get_by_label("Text as curves").click();
        h.run();
        switch(&mut h, "SVG", "PDF");
        h.get_by_label("Compress PDF").click();
        h.run();
        let compress = h.state().ui.pdf_export.options.compress;
        // Real modal action rectangles must fit the actual viewport.
        for label in ["Export", "Cancel"] {
            let r = h.get_by_label(label).rect();
            assert!(egui::Rect::from_min_size(egui::Pos2::ZERO, size).contains_rect(r), "{label}: {r:?}");
        }
        assert_eq!(h.state().ui.dialogs.len(), 1);
        assert!(h.state().ui.svg_export.embedded && h.state().ui.pdf_export.embedded);
        h.key_press(egui::Key::Escape);
        h.run();
        assert!(h.state().ui.dialogs.is_empty());
        assert!(h.state().ui.svg_export.request.is_none() && h.state().ui.pdf_export.request.is_none());
        assert_eq!(h.state().session.prefs().dialogs, before, "Escape does not publish draft defaults");
        let saved = serde_json::to_vec(h.state().session.prefs()).unwrap();
        let mut reopened = fixture(&output);
        reopened.session.edit_prefs(|prefs| *prefs = serde_json::from_slice(&saved).unwrap());
        open(&mut reopened).unwrap();
        assert_eq!(s_fmt(fields(&reopened)), "png");
        assert!(!reopened.ui.svg_export.options.minify && !reopened.ui.svg_export.options.text_as_curves);
        assert_eq!(reopened.ui.pdf_export.options.compress, !compress);
        let mut h = harness(reopened, size);
        h.get_by_label("Cancel").click();
        h.run();
        assert!(h.state().ui.dialogs.is_empty());
        assert_eq!(output.commits.load(Ordering::SeqCst), 0);
    }
}
#[test]
fn unified_export_picker_cancel_keeps_raster_and_vector_choices_and_basename() {
    let output = Arc::new(Output::default());
    let mut app = fixture(&output);
    let original_preferences = app.session.prefs().dialogs.clone();
    open(&mut app).unwrap();
    let mut h = harness(app, egui::vec2(1000., 700.));
    switch(&mut h, "PNG", "WebP");
    h.get_by_label("Lossless").click();
    h.run();
    h.get_by_label("Export").click();
    h.run();
    assert!(lossless(fields(h.state())));
    assert_eq!(output.names.lock().unwrap().last().unwrap(), "Photo version 2.webp");
    assert_eq!(h.state().ui.dialogs.len(), 1);
    switch(&mut h, "WebP", "SVG");
    h.get_by_label("Selection").click();
    h.run();
    h.get_by_label("Export").click();
    h.run();
    assert_eq!(output.names.lock().unwrap().last().unwrap(), "Photo version 2.svg");
    assert!(matches!(h.state().ui.svg_export.request.as_ref().unwrap().scope, photocraft_engine::vector_packages::svg_export::Scope::Selection { .. }));
    assert!(!pending(h.state()));
    let count = output.names.lock().unwrap().len();
    h.run_steps(5);
    assert_eq!(output.names.lock().unwrap().len(), count, "cancel must not auto-resubmit");
    h.get_by_label("Cancel").click();
    h.run();
    open(h.state_mut()).unwrap();
    h.run_steps(3);
    assert_eq!(s_fmt(fields(h.state())), "png");
    assert_eq!(h.state().ui.svg_export.scope_name(), Some("document"));
    assert_eq!(h.state().session.prefs().dialogs, original_preferences);
}
#[test]
fn unified_export_legacy_aliases_share_one_modal_and_one_visible_menu_entry() {
    let output = Arc::new(Output::default());
    let mut app = fixture(&output);
    let ctx = egui::Context::default();
    for (id, fmt) in [("ui.svg.export", "svg"), ("ui.pdf.export", "pdf"), ("file.export.pdf", "pdf")] {
        crate::menus::invoke(&mut app, &ctx, id, json!({})).unwrap();
        assert_eq!(app.ui.dialogs.len(), 1);
        assert_eq!(s_fmt(fields(&app)), fmt);
    }
    let entries = crate::menus::menu_items(&app);
    assert_eq!(entries.iter().filter(|e| e.id == "file.export.exportAs").count(), 1);
    assert!(!entries.iter().any(|e| matches!(e.id.as_str(), "ui.svg.export" | "ui.pdf.export" | "file.export.pdf")));
}
#[test]
fn unified_export_async_cancel_failure_and_success_keep_one_owner() {
    for fmt in ["svg", "pdf"] {
        let output = Arc::new(Output::default());
        output.pick.store(true, Ordering::SeqCst);
        output.hold.store(true, Ordering::SeqCst);
        let mut app = fixture(&output);
        let original_preferences = app.session.prefs().dialogs.clone();
        open_format(&mut app, fmt).unwrap();
        let mut h = harness(app, egui::vec2(1000., 700.));
        h.get_by_label("Export").click();
        h.run_steps(3);
        spin(&mut h, |_| output.entered.load(Ordering::SeqCst));
        assert!(pending(h.state()));
        let id = h.state().ui.dialogs[0].id;
        let picked = output.names.lock().unwrap().len();
        assert!(crate::dialogs::confirm(h.state_mut(), id).unwrap_err().contains("already running"));
        assert!(open_format(h.state_mut(), if fmt == "svg" { "pdf" } else { "svg" }).unwrap_err().contains("already running"));
        assert_eq!(output.names.lock().unwrap().len(), picked, "no replacement or duplicate job");
        assert_eq!(h.state().session.prefs().dialogs, original_preferences);
        assert_eq!(h.state().ui.dialogs.len(), 1, "settings retained but not drawn");
        assert!(
            h.ctx.data(|data| data.get_temp::<Vec<egui::Rect>>(egui::Id::new("pc-dialog-rects"))).unwrap_or_default().is_empty(),
            "settings must not own invisible hit rectangles"
        );
        h.key_press(egui::Key::Escape);
        h.run_steps(3);
        spin(&mut h, |app| !pending(app));
        assert!(!pending(h.state()));
        assert_eq!(s_fmt(fields(h.state())), fmt);
        assert_eq!(output.commits.load(Ordering::SeqCst), 0);
        assert_eq!(h.state().session.prefs().dialogs, original_preferences, "cancel does not update defaults");
        let submitted = output.names.lock().unwrap().len();
        h.run_steps(5);
        assert_eq!(output.names.lock().unwrap().len(), submitted);
        output.hold.store(false, Ordering::SeqCst);
        output.fail.store(true, Ordering::SeqCst);
        h.get_by_label("Export").click();
        h.run_steps(3);
        spin(&mut h, |app| !pending(app));
        assert_eq!(h.state().ui.notice_history.len(), 1);
        assert!(h.state().ui.notice_history[0].lines.iter().any(|s| s.contains("destination not writable")));
        assert_eq!(s_fmt(fields(h.state())), fmt);
        assert_eq!(h.state().session.prefs().dialogs, original_preferences, "failure does not update defaults");
        output.fail.store(false, Ordering::SeqCst);
        h.get_by_label("Export").click();
        h.run_steps(3);
        spin(&mut h, |app| app.ui.dialogs.is_empty());
        assert_eq!(output.commits.load(Ordering::SeqCst), 1);
        assert!(output.bytes.lock().unwrap()[0].starts_with(if fmt == "svg" { b"<svg" } else { b"%PDF-" }));
        assert_eq!(h.state().session.prefs().dialogs[PREFS]["format"], fmt, "success publishes the submitted format");
        let saved = serde_json::to_vec(h.state().session.prefs()).unwrap();
        let mut next = fixture(&output);
        next.session.edit_prefs(|p| *p = serde_json::from_slice(&saved).unwrap());
        open(&mut next).unwrap();
        assert_eq!(s_fmt(fields(&next)), fmt);
    }
}
#[test]
fn unified_export_stale_capture_never_picks_or_writes() {
    let output = Arc::new(Output::default());
    output.pick.store(true, Ordering::SeqCst);
    let mut app = fixture(&output);
    let id = open_format(&mut app, "svg").unwrap();
    app.session
        .edit("Late edit", |doc, _| {
            doc.name = "Updated".into();
            Ok(())
        })
        .unwrap();
    assert!(crate::dialogs::confirm(&mut app, id).unwrap_err().contains("changed"));
    assert!(output.names.lock().unwrap().is_empty());
    assert_eq!(output.commits.load(Ordering::SeqCst), 0);
    assert_eq!(app.ui.dialogs.len(), 1);
}

#[test]
fn unified_export_raster_failure_keeps_settings_until_success() {
    let output = Arc::new(Output::default());
    output.pick.store(true, Ordering::SeqCst);
    output.fail.store(true, Ordering::SeqCst);
    let mut app = fixture(&output);
    open(&mut app).unwrap();
    let mut h = harness(app, egui::vec2(1000., 700.));
    switch(&mut h, "PNG", "WebP");
    h.get_by_label("Lossless").click();
    h.run();
    h.get_by_label("Export").click();
    h.run();
    assert_eq!(h.state().ui.dialogs.len(), 1);
    assert!(lossless(fields(h.state())));
    assert!(h.state().ui.status.contains("/exports/Photo version 2.webp"));
    assert!(h.state().ui.status.contains("destination not writable"));
    assert_eq!(output.commits.load(Ordering::SeqCst), 0);
    output.fail.store(false, Ordering::SeqCst);
    h.get_by_label("Export").click();
    h.run();
    assert!(h.state().ui.dialogs.is_empty());
    assert_eq!(output.commits.load(Ordering::SeqCst), 1);
    assert!(output.settings.lock().unwrap().last().unwrap().webp_lossless);
    open(h.state_mut()).unwrap();
    h.run_steps(3);
    assert!(lossless(fields(h.state())));
}

#[test]
fn unified_export_success_remembers_submitted_options_without_document_artboard_ids() {
    let output = Arc::new(Output::default());
    output.pick.store(true, Ordering::SeqCst);
    output.hold.store(true, Ordering::SeqCst);
    let mut app = fixture(&output);
    let board = app.run("layer.new.artboard", json!({"rect":[0,0,32,32],"name":"First document page"})).unwrap()["layer"].as_u64().unwrap();
    open_format(&mut app, "pdf").unwrap();
    app.ui.pdf_export.options.artboards = vec![photocraft_doc::LayerId(board)];
    app.ui.pdf_export.options.compress = false;
    let mut h = harness(app, egui::vec2(1000., 700.));
    h.get_by_label("Export").click();
    h.run_steps(3);
    spin(&mut h, |_| output.entered.load(Ordering::SeqCst));
    h.state_mut().ui.pdf_export.options.compress = true; // Late state change must not replace the submitted defaults.
    output.hold.store(false, Ordering::SeqCst);
    spin(&mut h, |app| app.ui.dialogs.is_empty());
    assert_eq!(output.commits.load(Ordering::SeqCst), 1);
    let saved = &h.state().session.prefs().dialogs["vector.pdf.exportOptions"];
    let options: photocraft_pdf::export::ExportOptions = serde_json::from_value(saved.clone()).unwrap();
    assert!(!options.compress);
    assert!(options.artboards.is_empty(), "global defaults never contain document-local IDs");
    let prefs = h.state().session.prefs().clone();
    let mut next = fixture(&output);
    next.session.edit_prefs(|p| *p = prefs);
    open(&mut next).unwrap();
    assert_eq!(s_fmt(fields(&next)), "pdf");
    assert!(next.ui.pdf_export.options.artboards.is_empty());
    let mut h = harness(next, egui::vec2(1000., 700.));
    h.get_by_label("Export").click();
    h.run_steps(3);
    spin(&mut h, |app| app.ui.dialogs.is_empty());
    assert_eq!(output.commits.load(Ordering::SeqCst), 2, "a second document without artboards exports normally");
}
