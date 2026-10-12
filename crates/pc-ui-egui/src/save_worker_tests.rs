#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use super::*;
use crate::{PreparedWrite, Services};
use photocraft_doc::{ColorMode, Layer, LayerContent, SampleType, Size, SmartObject, SmartSource};
use std::{
    sync::Mutex,
    time::{Duration, Instant},
};

#[derive(Default)]
struct Probe {
    started: AtomicBool,
    ready: AtomicBool,
    release: AtomicBool,
    fail_commit: AtomicBool,
    commits: Mutex<Vec<Arc<Document>>>,
    original: Mutex<Option<Arc<Vec<u8>>>>,
    settings: Mutex<Vec<ExportSettings>>,
}
fn fixture() -> (PhotocraftApp, Arc<Probe>) {
    let probe = Arc::new(Probe::default());
    let work = probe.clone();
    let services = Services {
        prepare_save: Some(Arc::new(move |snapshot, path, settings, interrupt| {
            work.started.store(true, Ordering::Release);
            let started = Instant::now();
            // Simulate an indivisible codec phase: cancellation must reserve the slot until
            // the actual thread returns, without blocking any UI frames.
            while !work.release.load(Ordering::Acquire) {
                if started.elapsed() > Duration::from_secs(5) {
                    return Err("test encoder release timeout".into());
                }
                std::thread::sleep(Duration::from_millis(1));
            }
            interrupt.check().map_err(|e| e.to_string())?;
            work.settings.lock().unwrap().push(settings.clone());
            let original = work.original.lock().unwrap().clone();
            let snapshot = if let Some(bytes) = &original {
                photocraft_engine::develop_layer_cmds::freeze_source_snapshot(&snapshot, path, bytes.clone()).map_err(|e| e.to_string())?
            } else {
                snapshot
            };
            let saved = snapshot.clone();
            let commit = work.clone();
            work.ready.store(true, Ordering::Release);
            Ok(PreparedSave {
                snapshot,
                original,
                warnings: vec!["retained encoder warning".into()],
                write: PreparedWrite {
                    commit: Box::new(move || {
                        if commit.fail_commit.load(Ordering::Acquire) {
                            return Err("injected publication failure".into());
                        }
                        commit.commits.lock().unwrap().push(saved);
                        Ok(())
                    }),
                },
            })
        })),
        write: Some(Box::new(|_, _| panic!("background save reached synchronous writer"))),
        ..Default::default()
    };
    let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), services);
    app.run("file.new", json!({"width":32,"height":24,"background":"transparent"})).unwrap();
    app.run("layer.new.layer", json!({"name":"content"})).unwrap();
    app.background_jobs = true;
    (app, probe)
}
fn wait(flag: &AtomicBool) {
    let start = Instant::now();
    while !flag.load(Ordering::Acquire) {
        assert!(start.elapsed() < Duration::from_secs(5), "worker did not reach fixture barrier");
        std::thread::sleep(Duration::from_millis(1));
    }
}
fn settle(app: &mut PhotocraftApp, probe: &Probe) {
    probe.release.store(true, Ordering::Release);
    let ctx = egui::Context::default();
    let start = Instant::now();
    while app.pending_save.is_some() {
        crate::jobs_ui::tick_background(app, &ctx);
        assert!(start.elapsed() < Duration::from_secs(5), "save completion did not settle");
        std::thread::sleep(Duration::from_millis(1));
    }
}
fn frame(app: &mut PhotocraftApp, show: fn(&mut PhotocraftApp, &egui::Context), key: Option<egui::Key>) {
    let ctx = egui::Context::default();
    PhotocraftApp::setup_context(&ctx, Default::default());
    let mut first = ctx.run_ui(Default::default(), |ui| show(app, ui.ctx()));
    first.textures_delta.clear();
    let mut input = egui::RawInput::default();
    if let Some(key) = key {
        input.events.push(egui::Event::Key { key, physical_key: None, pressed: true, repeat: false, modifiers: Default::default() });
    }
    let mut out = ctx.run_ui(input, |ui| show(app, ui.ctx()));
    out.textures_delta.clear();
}

#[test]
fn background_save_keeps_ui_live_and_saves_snapshot_without_cleaning_newer_edits_or_other_tab() {
    let (mut app, probe) = fixture();
    let captured = app.session.active().unwrap().doc.clone();
    let revision = app.session.active().unwrap().revision;
    app.save_as(Some("A.pcraft".into())).unwrap();
    wait(&probe.started);
    for _ in 0..4 {
        frame(&mut app, crate::jobs_ui::tick, None);
    }
    assert!(probe.commits.lock().unwrap().is_empty());
    app.session
        .edit("newer edit", |doc, _| {
            doc.layers[0].opacity = 0.4;
            Ok(())
        })
        .unwrap();
    app.run("file.new", json!({"width":8,"height":8})).unwrap();
    let b = app.session.active().unwrap().doc.id;
    let b_saved = app.session.active().unwrap().saved_revision;
    settle(&mut app, &probe);
    let commits = probe.commits.lock().unwrap();
    assert_eq!(commits.len(), 1);
    assert!(*commits[0] == *captured, "worker encoded a later document");
    let a = app.session.documents().iter().find(|s| s.doc.id == captured.id).unwrap();
    assert_eq!(a.path.as_deref(), Some("A.pcraft"));
    assert_eq!(a.saved_revision, revision);
    assert!(a.is_dirty());
    assert_eq!(app.session.active().unwrap().doc.id, b);
    assert_eq!(app.session.active().unwrap().saved_revision, b_saved);
    assert!(app.session.active().unwrap().path.is_none());
}
#[test]
fn background_save_group_disclosure_is_clean_but_same_revision_content_replacement_is_not() {
    for content in [false, true] {
        let (mut app, probe) = fixture();
        app.session
            .edit("group", |doc, _| {
                doc.layers.push(Layer::group("group", vec![]));
                Ok(())
            })
            .unwrap();
        let group = app.session.active().unwrap().doc.layers.last().unwrap().id;
        let revision = app.session.active().unwrap().revision;
        app.save_as(Some("group.pcraft".into())).unwrap();
        wait(&probe.started);
        if content {
            Arc::make_mut(&mut app.session.active_mut().unwrap().doc).resolution_dpi = 144.0;
        } else {
            app.run("layer.setExpanded", json!({"layer":group.0,"expanded":false})).unwrap();
        }
        assert_eq!(app.session.active().unwrap().revision, revision);
        settle(&mut app, &probe);
        assert_eq!(app.session.active().unwrap().is_dirty(), content);
        if !content {
            assert!(matches!(&app.session.active().unwrap().doc.layer(group).unwrap().content, LayerContent::Group(g) if !g.expanded));
        }
    }
}
#[test]
fn background_save_cancel_reserves_worker_slot_and_never_publishes() {
    let (mut app, probe) = fixture();
    app.save_as(Some("cancel.pcraft".into())).unwrap();
    wait(&probe.started);
    let job = app.pending_save.as_ref().unwrap().job;
    crate::jobs_ui::cancel(&mut app, job);
    crate::jobs_ui::tick_background(&mut app, &egui::Context::default());
    assert!(app.pending_save.is_some(), "indivisible encoder is still running");
    assert!(app.save_as(Some("new.pcraft".into())).is_err());
    settle(&mut app, &probe);
    assert!(probe.commits.lock().unwrap().is_empty());
    assert!(app.session.active().unwrap().path.is_none());
    assert!(app.session.active().unwrap().is_dirty());
}
#[test]
fn background_save_failure_and_closed_document_do_not_publish_or_change_saved_state() {
    for close in [false, true] {
        let (mut app, probe) = fixture();
        let doc = app.session.active().unwrap().doc.id;
        let saved = app.session.active().unwrap().saved_revision;
        app.save_as(Some("failed.pcraft".into())).unwrap();
        wait(&probe.started);
        if close {
            app.session.execute("file.close", json!({"discardDocuments":[doc.0]})).unwrap();
        } else {
            probe.fail_commit.store(true, Ordering::Release);
        }
        settle(&mut app, &probe);
        assert!(probe.commits.lock().unwrap().is_empty());
        if !close {
            assert_eq!(app.session.active().unwrap().saved_revision, saved);
            assert!(app.ui.status_error);
        }
    }
}
#[test]
fn background_save_closing_waits_for_commit_and_rechecks_new_edits() {
    for edit in [false, true] {
        let (mut app, probe) = fixture();
        app.save_as(Some("close.pcraft".into())).unwrap();
        wait(&probe.started);
        assert!(crate::discard_ui::intercept(&mut app, "file.close", &json!({})));
        frame(&mut app, crate::discard_ui::show, None);
        assert_eq!(app.session.documents().len(), 1);
        if edit {
            app.session
                .edit("newer", |doc, _| {
                    doc.layers[0].name = "newer".into();
                    Ok(())
                })
                .unwrap();
        }
        settle(&mut app, &probe);
        frame(&mut app, crate::discard_ui::show, None);
        assert_eq!(app.session.documents().len(), usize::from(edit));
        if edit {
            assert!(app.session.active().unwrap().is_dirty());
            assert!(app.discard.is_some());
        }
    }
}
#[test]
fn background_save_tiff_and_library_return_continue_only_after_success() {
    let (mut app, probe) = fixture();
    app.host_modes = true;
    crate::develop_layer::save_and_return(&mut app, &json!({"path":"return.tif"})).unwrap();
    assert!(app.tiff_options.is_some());
    assert!(!probe.started.load(Ordering::Acquire));
    app.tiff_options.as_mut().unwrap().dont_ask_again = true;
    frame(&mut app, crate::tiff_options_ui::show, Some(egui::Key::Enter));
    wait(&probe.started);
    assert!(app.library_saves.is_empty());
    assert!(app.session.prefs().file_handling.ask_before_saving_layered_tiff);
    settle(&mut app, &probe);
    assert_eq!(app.library_saves.len(), 1);
    assert!(app.library_saves[0].show);
    assert!(!app.session.prefs().file_handling.ask_before_saving_layered_tiff);
    assert!(probe.settings.lock().unwrap()[0].tiff_layers);
}
#[test]
fn background_save_scripts_target_saved_document_and_keep_explicit_callback_focus() {
    let (mut app, probe) = fixture();
    app.host_modes = true;
    let a = app.session.active().unwrap().doc.id;
    app.run("file.new", json!({"width":8,"height":8})).unwrap();
    let b = app.session.active().unwrap().doc.id;
    app.session.set_active(0);
    app.run(
        "file.scripts.scriptEventsManager",
        json!({"add":{"event":"saveDocument","steps":[["layer.new.layer",{"name":"saved callback"}],["document.activate",{"document":1}]]}}),
    )
    .unwrap();
    app.save_as(Some("script.pcraft".into())).unwrap();
    wait(&probe.started);
    app.session.set_active(1);
    settle(&mut app, &probe);
    assert_eq!(app.session.active().unwrap().doc.id, b);
    let a = app.session.documents().iter().find(|s| s.doc.id == a).unwrap();
    assert!(a.doc.layers.iter().any(|l| l.name == "saved callback"));
    assert!(a.is_dirty());
    assert!(!app.session.active().unwrap().doc.layers.iter().any(|l| l.name == "saved callback"));
    assert_eq!(app.library_saves[0].path, "script.pcraft");
}
#[test]
fn background_save_freezes_encoded_links_and_rebases_current_history_without_losing_new_edits() {
    let (mut app, probe) = fixture();
    let bytes = Arc::new(vec![4, 3, 2, 1]);
    *probe.original.lock().unwrap() = Some(bytes.clone());
    let smart = || Layer::new("linked", LayerContent::Smart(SmartObject::new(SmartSource::Linked { path: "source.pcraft".into() }, Default::default(), None)));
    app.session
        .edit("link A", |doc, _| {
            doc.layers.push(smart());
            Ok(())
        })
        .unwrap();
    let a = app.session.active().unwrap().doc.id;
    let mut other = Document::new("other", Size::new(8, 8), ColorMode::Rgb, SampleType::U8);
    other.layers.push(smart());
    app.session.add_document(other, None);
    let b = app.session.active().unwrap().doc.id;
    app.session.set_active(0);
    app.save_as(Some("source.pcraft".into())).unwrap();
    wait(&probe.started);
    app.session.set_active(1);
    app.session
        .edit("new B name", |doc, _| {
            doc.name = "new B name".into();
            Ok(())
        })
        .unwrap();
    settle(&mut app, &probe);
    let embedded = |doc: &Document| {
        doc.layers
            .iter()
            .filter_map(|l| if let LayerContent::Smart(s) = &l.content { Some(&s.source) } else { None })
            .all(|source| matches!(source, SmartSource::Embedded { bytes: b, .. } if **b == *bytes))
    };
    assert!(embedded(&probe.commits.lock().unwrap()[0]));
    assert!(embedded(&app.session.documents().iter().find(|s| s.doc.id == a).unwrap().doc));
    assert_eq!(app.session.active().unwrap().doc.id, b);
    assert_eq!(app.session.active().unwrap().doc.name, "new B name");
    assert!(embedded(&app.session.active().unwrap().doc));
    app.session.undo();
    assert!(embedded(&app.session.active().unwrap().doc));
}

#[test]
fn background_save_floating_edits_block_navigation_without_losing_saved_base_state() {
    let (mut app, probe) = fixture();
    app.host_modes = true;
    let layer = app.session.active().unwrap().active_layer.unwrap();
    app.session
        .edit("pixels", |doc, _| {
            doc.layer_mut(layer).unwrap().surface_mut().unwrap().fill_rect(photocraft_geom::Rect::new(0, 0, 8, 8), &[1.0, 0.0, 0.0, 1.0]);
            Ok(())
        })
        .unwrap();
    app.run("select.rect", json!({"x":0,"y":0,"width":4,"height":4})).unwrap();
    let revision = app.session.active().unwrap().revision;
    crate::develop_layer::save_and_return(&mut app, &json!({"path":"float.pcraft"})).unwrap();
    wait(&probe.started);
    app.run("select.float", json!({"dx":8,"dy":0})).unwrap();
    assert_eq!(app.session.active().unwrap().revision, revision);
    settle(&mut app, &probe);
    assert!(app.session.active().unwrap().is_dirty());
    assert_eq!(app.session.active().unwrap().saved_revision, revision, "captured base pixels were saved");
    assert!(!app.library_saves[0].show, "a newer floating edit must keep the editor open");
    assert_eq!(photocraft_engine::float_cmds::floating(app.session.active().unwrap()).unwrap().offset, (8, 0));
}

#[test]
fn background_save_ready_frame_escape_wins_before_publication() {
    let (mut app, probe) = fixture();
    app.save_as(Some("ready.pcraft".into())).unwrap();
    wait(&probe.started);
    probe.release.store(true, Ordering::Release);
    wait(&probe.ready);
    let ctx = egui::Context::default();
    PhotocraftApp::setup_context(&ctx, Default::default());
    let raw = egui::RawInput {
        events: vec![egui::Event::Key { key: egui::Key::Escape, physical_key: None, pressed: true, repeat: false, modifiers: Default::default() }],
        ..Default::default()
    };
    let mut out = ctx.run_ui(raw, |ui| crate::jobs_ui::tick(&mut app, ui.ctx()));
    out.textures_delta.clear();
    settle(&mut app, &probe);
    assert!(probe.commits.lock().unwrap().is_empty());
}

#[test]
fn background_save_rechecks_publication_authority() {
    fn deny(_: &str, _: &serde_json::Value) -> photocraft_engine::Result<()> {
        Err(EngineError::Other("save authority revoked".into()))
    }
    let (mut app, probe) = fixture();
    app.save_as(Some("authority.pcraft".into())).unwrap();
    wait(&probe.started);
    app.session.authorize = Some(deny);
    settle(&mut app, &probe);
    assert!(probe.commits.lock().unwrap().is_empty());
    assert!(app.session.active().unwrap().path.is_none());
    assert!(app.ui.status_error);
}

#[test]
fn background_save_image_assets_use_committed_snapshot_not_newer_edits() {
    let (mut app, probe) = fixture();
    let dir = std::env::temp_dir().join(format!(
        "li-save-assets-{}-{}",
        std::process::id(),
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
    ));
    std::fs::create_dir(&dir).unwrap();
    let path = dir.join("saved.pcraft").to_string_lossy().into_owned();
    let id = app.session.active().unwrap().doc.id;
    let layer = app.session.active().unwrap().active_layer.unwrap();
    app.session
        .edit("asset", |doc, _| {
            let layer = doc.layer_mut(layer).unwrap();
            layer.name = "asset.png".into();
            layer.surface_mut().unwrap().fill_rect(photocraft_geom::Rect::new(0, 0, 4, 4), &[1.0, 0.0, 0.0, 1.0]);
            Ok(())
        })
        .unwrap();
    app.session.file_menu.image_assets.push(id);
    app.save_as(Some(path.clone())).unwrap();
    wait(&probe.started);
    app.session
        .edit("new green pixels", |doc, _| {
            doc.layer_mut(layer).unwrap().surface_mut().unwrap().fill_rect(photocraft_geom::Rect::new(0, 0, 4, 4), &[0.0, 1.0, 0.0, 1.0]);
            Ok(())
        })
        .unwrap();
    settle(&mut app, &probe);
    let asset = PathBuf::from(photocraft_engine::web_cmds::assets_dir(&path)).join("asset.png");
    let imported = photocraft_io::import("asset.png", &std::fs::read(&asset).unwrap()).unwrap().document;
    let pixels = photocraft_compose::flatten(&imported).to_rgba8().pixels;
    assert_eq!(&pixels[..4], &[255, 0, 0, 255]);
    assert!(app.session.active().unwrap().is_dirty());
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn background_save_and_next_waits_for_publication_and_keeps_new_edits() {
    for edit in [false, true] {
        let (mut app, probe) = fixture();
        let dir = std::env::temp_dir().join(format!("li-background-next-{}-{edit}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let a = dir.join("a.pcraft");
        let b = dir.join("b.pcraft");
        app.session.active_mut().unwrap().path = Some(a.to_string_lossy().into_owned());
        let a_id = app.session.active().unwrap().doc.id;
        app.session.add_document(Document::new("b", Size::new(8, 8), ColorMode::Rgb, SampleType::U8), Some(b.to_string_lossy().into_owned()));
        let b_id = app.session.active().unwrap().doc.id;
        app.session.set_active(0);
        app.session.edit_prefs(|p| p.interface.save_and_next_saves_to = "original".into());
        crate::filmstrip_ui::set_folder(&dir, vec![a.clone(), b]);
        assert!(!crate::filmstrip_ui::save_and_next(&mut app));
        wait(&probe.started);
        assert_eq!(app.session.active().unwrap().doc.id, a_id);
        if edit {
            app.run("layer.new.layer", json!({"name":"new edit"})).unwrap();
        }
        settle(&mut app, &probe);
        assert_eq!(probe.commits.lock().unwrap().len(), 1);
        assert_eq!(app.session.active().unwrap().doc.id, if edit { a_id } else { b_id });
        if edit {
            assert!(app.session.active().unwrap().is_dirty());
        }
        std::fs::remove_dir_all(dir).unwrap();
    }
}
