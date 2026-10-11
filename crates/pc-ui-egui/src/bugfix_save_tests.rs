//! Headless save/data-safety regressions from the final reviews.
#![allow(clippy::unwrap_used)]
use crate::{ExportSettings, PhotocraftApp, Services};
use photocraft_doc::{Document, LayerContent};
use serde_json::json;
use std::sync::{Arc, Mutex};

type Writes = Arc<Mutex<Vec<(String, Vec<u8>)>>>;
fn app() -> (PhotocraftApp, Writes) {
    let writes: Writes = Arc::default();
    let sink = writes.clone();
    let mut app = PhotocraftApp::new(
        photocraft_engine::Session::new(),
        Services {
            export: Some(Box::new(|doc, path, settings| {
                let options = photocraft_io::ExportOptions { tiff_layers: settings.tiff_layers, ..Default::default() };
                let result = photocraft_io::export(doc, path.rsplit('.').next().unwrap(), &options).map_err(|e| e.to_string())?;
                Ok((result.bytes, result.warnings))
            })),
            write: Some(Box::new(move |path, bytes| {
                sink.lock().unwrap().push((path.into(), bytes.to_vec()));
                Ok(())
            })),
            ..Default::default()
        },
    );
    app.run("file.new", json!({"width": 32, "height": 24, "background": "transparent"})).unwrap();
    app.run("layer.new.layer", json!({"name": "Edit"})).unwrap();
    app.session
        .edit("fixture pixels", |doc, _| {
            for layer in &mut doc.layers {
                layer.surface_mut().unwrap().fill_rect(photocraft_geom::Rect::new(0, 0, 2, 2), &[0.2, 0.3, 0.4, 1.]);
            }
            Ok(())
        })
        .unwrap();
    (app, writes)
}
fn enter(app: &mut PhotocraftApp, show: fn(&mut PhotocraftApp, &egui::Context)) {
    press(app, show, egui::Key::Enter);
}
fn press(app: &mut PhotocraftApp, show: fn(&mut PhotocraftApp, &egui::Context), key: egui::Key) {
    let ctx = egui::Context::default();
    PhotocraftApp::setup_context(&ctx, Default::default());
    let mut output = ctx.run_ui(egui::RawInput::default(), |ui| show(app, ui.ctx()));
    output.textures_delta.clear();
    let input = egui::RawInput {
        events: vec![egui::Event::Key { key, physical_key: None, pressed: true, repeat: false, modifiers: Default::default() }],
        ..Default::default()
    };
    let mut output = ctx.run_ui(input, |ui| show(app, ui.ctx()));
    output.textures_delta.clear();
}

#[test]
fn bugfix_save_o008_flat_exports_keep_path_dirty_and_close_guard() {
    for ext in ["png", "jpg", "webp", "tif"] {
        let (mut app, writes) = app();
        app.session.active_mut().unwrap().path = Some("working.psd".into());
        let revision = app.session.active().unwrap().saved_revision;
        app.write_document(format!("copy.{ext}"), &ExportSettings { tiff_layers: false, ..Default::default() }).unwrap();
        assert_eq!(writes.lock().unwrap().len(), 1);
        let st = app.session.active().unwrap();
        assert_eq!(st.path.as_deref(), Some("working.psd"), "{ext}");
        assert_eq!(st.saved_revision, revision, "{ext}");
        assert!(st.is_dirty(), "{ext}");
        assert!(crate::discard_ui::intercept(&mut app, "file.closeAll", &json!({})), "{ext}");
    }
}

#[test]
fn bugfix_save_o008_suggests_psd_beside_original_and_automation_keeps_dirty() {
    let (mut app, _) = app();
    app.session.active_mut().unwrap().path = Some("/photos/photo.jpg".into());
    app.services.pick_save = Some(Box::new(|suggested| {
        assert_eq!(suggested, "/photos/photo.psd");
        None
    }));
    assert_eq!(app.save_as(None).unwrap_err(), "cancelled");
    app.services.automation_write = Some(Box::new(|_, _| Ok(())));
    app.save_automation(Some("copy.png".into())).unwrap();
    assert!(app.session.active().unwrap().is_dirty());
    assert_eq!(app.session.active().unwrap().path.as_deref(), Some("/photos/photo.jpg"));
}

#[test]
fn bugfix_save_o007_tiff_prompt_keeps_target_when_active_tab_changes() {
    let (mut app, writes) = app();
    let doc = app.session.active().unwrap().doc.id;
    app.save_as(Some("original.tif".into())).unwrap();
    assert!(writes.lock().unwrap().is_empty());
    app.run("file.new", json!({"width": 8, "height": 8})).unwrap();
    let other = app.session.active().unwrap().doc.id;
    enter(&mut app, crate::tiff_options_ui::show);
    assert!(app.tiff_options.is_none());
    assert_eq!(app.session.active().unwrap().doc.id, other);
    let output = &writes.lock().unwrap()[0].1;
    let back = photocraft_io::import("original.tif", output).unwrap().document;
    assert_eq!(back.size, photocraft_doc::Size::new(32, 24));
    assert_eq!(back.layers.len(), 2);
    assert!(!app.session.documents().iter().find(|st| st.doc.id == doc).unwrap().is_dirty());
    assert!(app.session.documents().iter().find(|st| st.doc.id == other).unwrap().path.is_none());
}

#[test]
fn bugfix_save_o007_close_and_quit_wait_for_tiff_copy_or_layered_save() {
    for command in ["file.closeAll", "file.exit"] {
        for discard_layers in [false, true] {
            let (mut app, writes) = app();
            app.services.pick_save = Some(Box::new(|_| Some("close.tif".into())));
            assert!(crate::discard_ui::intercept(&mut app, command, &json!({})));
            enter(&mut app, crate::discard_ui::show);
            assert!(!app.allow_close);
            assert_eq!(app.session.documents().len(), 1);
            assert!(app.discard.is_some());
            app.tiff_options.as_mut().unwrap().discard_layers = discard_layers;
            enter(&mut app, crate::tiff_options_ui::show);
            let ctx = egui::Context::default();
            crate::discard_ui::show(&mut app, &ctx);
            assert_eq!(writes.lock().unwrap().len(), 1);
            if discard_layers {
                assert!(app.discard.is_some());
                assert!(!app.allow_close);
                assert_eq!(app.session.documents().len(), 1);
            } else if command == "file.exit" {
                assert!(app.allow_close);
            } else {
                assert!(app.session.documents().is_empty());
            }
        }
    }
}

#[test]
fn bugfix_save_o007_tiff_cancel_and_write_failure_keep_close_and_quit_protected() {
    for command in ["file.closeAll", "file.exit"] {
        for cancel in [true, false] {
            let (mut app, writes) = app();
            app.services.pick_save = Some(Box::new(|_| Some("close.tif".into())));
            assert!(crate::discard_ui::intercept(&mut app, command, &json!({})));
            enter(&mut app, crate::discard_ui::show);
            if cancel {
                press(&mut app, crate::tiff_options_ui::show, egui::Key::Escape);
            } else {
                app.tiff_options.as_mut().unwrap().dont_ask_again = true;
                app.services.write = Some(Box::new(|_, _| Err("disk full".into())));
                enter(&mut app, crate::tiff_options_ui::show);
                assert!(app.ui.status_error);
                assert!(app.ui.status.contains("disk full"));
                assert!(app.session.prefs().file_handling.ask_before_saving_layered_tiff);
            }
            crate::discard_ui::show(&mut app, &egui::Context::default());
            assert!(app.tiff_options.is_none());
            assert!(app.discard.is_some());
            assert!(!app.allow_close);
            assert_eq!(app.session.documents().len(), 1);
            assert!(app.session.active().unwrap().is_dirty());
            assert!(writes.lock().unwrap().is_empty());
        }
    }
}

#[test]
fn bugfix_save_o007_save_and_return_waits_for_tiff_even_when_document_was_clean() {
    for clean in [false, true] {
        for cancel in [false, true] {
            let (mut app, writes) = app();
            app.host_modes = true;
            if clean {
                app.session.active_mut().unwrap().saved_revision = app.session.active().unwrap().revision;
            }
            crate::develop_layer::save_and_return(&mut app, &json!({"path": "return.tif"})).unwrap();
            assert!(app.switch_module.is_none());
            assert!(app.library_saves.is_empty());
            if cancel {
                press(&mut app, crate::tiff_options_ui::show, egui::Key::Escape);
                assert!(app.switch_module.is_none());
                assert!(app.library_saves.is_empty());
                assert!(writes.lock().unwrap().is_empty());
            } else {
                enter(&mut app, crate::tiff_options_ui::show);
                assert!(app.library_saves.last().unwrap().show);
                assert_eq!(writes.lock().unwrap().len(), 1);
            }
            assert!(app.tiff_options.is_none());
        }
    }
}

#[test]
fn bugfix_save_o010_child_save_requires_answer_for_newly_dirty_parent() {
    for command in ["file.closeAll", "file.exit"] {
        let (mut app, _) = app();
        app.run("layer.smartObjects.convertToSmartObject", json!({})).unwrap();
        let parent = app.session.active().unwrap().doc.id;
        app.session.active_mut().unwrap().saved_revision = app.session.active().unwrap().revision;
        app.run("layer.smartObjects.editContents", json!({})).unwrap();
        app.run("layer.new.layer", json!({})).unwrap();
        assert!(crate::discard_ui::intercept(&mut app, command, &json!({})));
        enter(&mut app, crate::discard_ui::show);
        assert!(!app.allow_close);
        assert_eq!(app.session.documents().len(), 2);
        assert!(app.session.documents().iter().find(|st| st.doc.id == parent).unwrap().is_dirty());
        assert!(app.discard.is_some());
    }
}

#[test]
fn bugfix_save_o045_floating_move_is_dirty_and_saved_pixels_reopen() {
    let (mut app, writes) = app();
    let id = app.session.active().unwrap().active_layer.unwrap();
    app.session
        .edit("paint", |doc: &mut Document, _| {
            doc.layer_mut(id).unwrap().surface_mut().unwrap().fill_rect(photocraft_geom::Rect::new(2, 2, 8, 8), &[1., 0., 0., 1.]);
            Ok(())
        })
        .unwrap();
    app.run("select.rect", json!({"x": 2, "y": 2, "width": 6, "height": 6})).unwrap();
    app.session.active_mut().unwrap().saved_revision = app.session.active().unwrap().revision;
    app.run("select.float", json!({"dx": 10, "dy": 0})).unwrap();
    assert!(app.session.active().unwrap().is_dirty());
    app.save_as(Some("float.psd".into())).unwrap();
    assert!(app.session.active().unwrap().floating.is_none());
    let back = photocraft_io::import("float.psd", &writes.lock().unwrap()[0].1).unwrap().document;
    let raster =
        back.layers.iter().find_map(|l| if l.name == "Edit" { if let LayerContent::Raster(s) = &l.content { Some(s) } else { None } } else { None }).unwrap();
    assert_eq!(raster.rgba(3, 3)[3], 0.);
    assert_eq!(raster.rgba(13, 3), [1., 0., 0., 1.]);
    assert!(!app.session.active().unwrap().is_dirty());
}

#[test]
fn bugfix_save_o009_layered_original_always_warns_even_with_dont_ask_again() {
    let (mut app, _) = app();
    app.host_modes = true;
    app.services.overwrite_original = Some(Box::new(|_, _, _| Err("a layered original must require confirmation".into())));
    let original = app.session.active().unwrap().doc.clone();
    let mut layer = photocraft_engine::smart_cmds::layer_to_smart(&original, &original.layers[0]).unwrap();
    if let LayerContent::Smart(smart) = &mut layer.content {
        smart.source = photocraft_doc::SmartSource::Linked { path: "/photos/photo.psd".into() };
        smart.develop = Some(photocraft_doc::DevelopLink { photo: Some(7), settings: json!({}) });
    }
    let st = app.session.active_mut().unwrap();
    Arc::make_mut(&mut st.doc).layers = vec![layer];
    let id = st.doc.id;
    app.session.edit_prefs(|p| {
        p.dialogs.insert("file.saveOverOriginal".into(), json!({"dontAskAgain": true}));
    });
    crate::overwrite_original::request(&mut app, id, false).unwrap();
    assert!(app.overwrite_prompt.is_some());
}

#[test]
fn bugfix_save_o048_export_cancel_failure_and_retry_preserve_dialog_fields() {
    let (mut app, writes) = app();
    let id = crate::export_dialog::open(&mut app).unwrap();
    app.ui.dialog_mut(id).unwrap().fields.insert("quality".into(), json!(61));
    app.services.pick_save = Some(Box::new(|_| None));
    assert_eq!(crate::dialogs::confirm(&mut app, id).unwrap_err(), "cancelled");
    assert_eq!(app.ui.dialog_mut(id).unwrap().fields["quality"], 61);
    app.services.pick_save = Some(Box::new(|_| Some("out.png".into())));
    app.services.write = Some(Box::new(|_, _| Err("disk full".into())));
    enter(&mut app, crate::dialogs::show);
    assert!(app.ui.status_error);
    assert!(app.ui.status.contains("disk full"));
    assert_eq!(app.ui.dialog_mut(id).unwrap().fields["quality"], 61);
    let sink = writes.clone();
    app.services.write = Some(Box::new(move |path, data| {
        sink.lock().unwrap().push((path.into(), data.to_vec()));
        Ok(())
    }));
    crate::dialogs::confirm(&mut app, id).unwrap();
    assert!(app.ui.dialog_mut(id).is_none());
    assert_eq!(writes.lock().unwrap().len(), 1);
}
