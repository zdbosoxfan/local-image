#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use super::*;
use photocraft_doc::{Color, ColorMode, Document, Fill, Layer, LayerContent, SampleType, ShapeLayer, Size};
use std::sync::{Mutex, atomic::AtomicUsize};
use std::time::{Duration, Instant};

#[derive(Default)]
struct Stage {
    entered: AtomicBool,
    release: AtomicBool,
    fail_stage: AtomicBool,
    fail_commit: AtomicBool,
    commits: AtomicUsize,
    cleanups: AtomicUsize,
    destination: Mutex<Vec<u8>>,
}
struct Temp {
    stage: Arc<Stage>,
    committed: bool,
}
impl Drop for Temp {
    fn drop(&mut self) {
        if !self.committed {
            self.stage.cleanups.fetch_add(1, Ordering::SeqCst);
        }
    }
}
fn fixture(pdf: bool) -> (PhotocraftApp, Arc<Stage>, Request) {
    let stage = Arc::new(Stage { destination: Mutex::new(b"existing destination".to_vec()), ..Default::default() });
    let worker = stage.clone();
    let services = Services {
        pick_save: Some(Box::new(move |name| Some(name.to_owned()))),
        stage_write: Some(Arc::new(move |_, bytes, interrupt| {
            let temp = Temp { stage: worker.clone(), committed: false };
            worker.entered.store(true, Ordering::SeqCst);
            let until = Instant::now() + Duration::from_secs(10);
            while !worker.release.load(Ordering::SeqCst) {
                interrupt.check().map_err(|e| e.to_string())?;
                if Instant::now() > until {
                    return Err("staging fixture timeout".into());
                }
                std::thread::sleep(Duration::from_millis(2));
            }
            interrupt.check().map_err(|e| e.to_string())?;
            if worker.fail_stage.load(Ordering::SeqCst) {
                return Err("injected stage failure".into());
            }
            let bytes = bytes.to_vec();
            Ok(PreparedWrite {
                commit: Box::new(move || {
                    let mut temp = temp;
                    if temp.stage.fail_commit.load(Ordering::SeqCst) {
                        return Err("injected commit failure".into());
                    }
                    *temp.stage.destination.lock().unwrap() = bytes;
                    temp.stage.commits.fetch_add(1, Ordering::SeqCst);
                    temp.committed = true;
                    Ok(())
                }),
            })
        })),
        write: Some(Box::new(|_, _| panic!("native staged route must not fall back to ambient bulk write"))),
        ..Default::default()
    };
    let mut app = PhotocraftApp::new(Session::new(), services);
    let mut doc = Document::new("Export fixture", Size::new(64, 64), ColorMode::Rgb, SampleType::U8);
    doc.layers.push(Layer::new(
        "shape",
        LayerContent::Shape(ShapeLayer {
            path: photocraft_vector::shapes::rect(5., 6., 20., 18.),
            fill: Some(Fill::Solid(Color::BLACK)),
            ..Default::default()
        }),
    ));
    app.session.add_document(doc, None);
    let state = app.session.active().unwrap();
    let request = if pdf {
        Request::Pdf(pdf_export::Request {
            document: state.doc.id.0,
            revision: state.revision,
            scope: pdf_export::Scope::Document,
            options: Default::default(),
        })
    } else {
        Request::Svg(svg_export::Request {
            document: state.doc.id.0,
            revision: state.revision,
            scope: svg_export::Scope::Document,
            options: Default::default(),
        })
    };
    (app, stage, request)
}
fn ready(app: &mut PhotocraftApp, pending: &Pending) {
    let job = pending.job.unwrap();
    let receipt = app.session.wait_job(job).unwrap();
    assert!(receipt.to_string().len() < 64);
    assert!(receipt.get("bytes").is_none());
}
#[test]
fn vector_export_stage_ready_cancel_stale_closed_and_failure_preserve_destination() {
    for pdf in [false, true] {
        for disposition in ["cancel", "stale", "closed", "commit-failure", "success"] {
            let (mut app, stage, request) = fixture(pdf);
            let source = app.session.active().unwrap().doc.clone();
            stage.release.store(true, Ordering::SeqCst);
            let mut pending = begin(&mut app, request).unwrap().unwrap();
            ready(&mut app, &pending);
            assert_eq!(*stage.destination.lock().unwrap(), b"existing destination");
            match disposition {
                "cancel" => pending.cancel(&mut app.session),
                "stale" => {
                    app.session
                        .edit("independent edit", |doc, _| {
                            doc.name = "changed".into();
                            Ok(())
                        })
                        .unwrap();
                }
                "closed" => {
                    app.session.close(0);
                }
                "commit-failure" => stage.fail_commit.store(true, Ordering::SeqCst),
                _ => {
                    app.session.add_document(Document::new("other tab", Size::new(1, 1), ColorMode::Rgb, SampleType::U8), None);
                }
            }
            let viewing = app.session.active().map(|state| state.doc.id);
            let result = pending.poll(&mut app);
            assert_eq!(app.session.active().map(|state| state.doc.id), viewing, "export must not change the viewed tab");
            if disposition == "success" {
                assert!(matches!(result.unwrap(), Poll::Complete(_)));
                assert_eq!(stage.commits.load(Ordering::SeqCst), 1);
                assert!(stage.destination.lock().unwrap().starts_with(if pdf { b"%PDF-" } else { b"<svg" }));
                assert!(Arc::ptr_eq(&source, &app.session.documents()[0].doc));
                assert_eq!(app.session.documents()[0].history.past_len(), 0);
            } else {
                if disposition == "cancel" {
                    assert!(matches!(result, Ok(Poll::Cancelled)));
                } else {
                    assert!(result.is_err());
                }
                assert_eq!(*stage.destination.lock().unwrap(), b"existing destination");
            }
            drop(pending);
            assert_eq!(stage.cleanups.load(Ordering::SeqCst), usize::from(disposition != "success"));
        }
        let (mut app, stage, request) = fixture(pdf);
        stage.release.store(true, Ordering::SeqCst);
        stage.fail_stage.store(true, Ordering::SeqCst);
        let mut pending = begin(&mut app, request).unwrap().unwrap();
        assert!(app.session.wait_job(pending.job.unwrap()).is_err());
        assert!(pending.poll(&mut app).unwrap_err().contains("injected stage failure"));
        assert_eq!(*stage.destination.lock().unwrap(), b"existing destination");
        assert_eq!(stage.cleanups.load(Ordering::SeqCst), 1);
    }
}

#[test]
fn vector_export_automation_never_uses_ambient_stage_or_write_and_rechecks_authority() {
    for deny in [false, true] {
        let (mut app, stage, request) = fixture(true);
        app.automation_input = true;
        let writes = Arc::new(AtomicUsize::new(0));
        let output = Arc::new(Mutex::new(Vec::new()));
        let count = writes.clone();
        let bytes = output.clone();
        app.services.automation_write = Some(Box::new(move |path, data| {
            assert!(path.ends_with(".pdf"));
            count.fetch_add(1, Ordering::SeqCst);
            *bytes.lock().unwrap() = data.to_vec();
            Ok(())
        }));
        let denied = Arc::new(AtomicBool::new(false));
        let gate = denied.clone();
        app.services.automation_command = Some(Box::new(move |_, _| if gate.load(Ordering::SeqCst) { Err("authority revoked".into()) } else { Ok(()) }));
        let mut pending = begin(&mut app, request).unwrap().unwrap();
        app.automation_input = false;
        ready(&mut app, &pending);
        denied.store(deny, Ordering::SeqCst);
        let result = pending.poll(&mut app);
        assert!(!stage.entered.load(Ordering::SeqCst));
        assert_eq!(stage.commits.load(Ordering::SeqCst), 0);
        assert_eq!(writes.load(Ordering::SeqCst), usize::from(!deny));
        if deny {
            assert!(result.unwrap_err().contains("authority revoked"));
        } else {
            assert!(matches!(result.unwrap(), Poll::Complete(_)));
            assert!(output.lock().unwrap().starts_with(b"%PDF-"));
        }
    }
}

#[test]
fn vector_export_production_dialog_keeps_repainting_and_cancel_stops_staging() {
    use egui_kittest::{Harness, kittest::Queryable};
    for pdf in [false, true] {
        let (mut app, stage, _) = fixture(pdf);
        let source = app.session.active().unwrap().doc.clone();
        if pdf {
            app.ui.pdf_export.open(&app.session).unwrap();
        } else {
            app.ui.svg_export.open(&app.session).unwrap();
        }
        let mut h = Harness::builder().with_size(egui::vec2(760., 480.)).build_ui_state(
            move |ui, app: &mut PhotocraftApp| {
                if !ui.ctx().fonts(|fonts| fonts.families().contains(&egui::FontFamily::Name("medium".into()))) {
                    return;
                }
                let ctx = ui.ctx().clone();
                crate::jobs_ui::tick(app, &ctx);
                ui.ctx().data_mut(|data| *data.get_temp_mut_or_default::<u64>(egui::Id::new("export-test-frames")) += 1);
                egui::CentralPanel::default().show(ui, |ui| {
                    ui.label("Responsive canvas frame");
                });
                if pdf {
                    super::super::pdf_export::frame(app, &ctx);
                } else {
                    super::super::svg_export::frame(app, &ctx);
                }
            },
            app,
        );
        PhotocraftApp::setup_context(&h.ctx, crate::theme::ThemeKind::Studio);
        h.run_steps(3);
        h.get_by_label("Export").click();
        h.run_steps(2);
        let until = Instant::now() + Duration::from_secs(10);
        while !stage.entered.load(Ordering::SeqCst) {
            assert!(Instant::now() < until);
            h.run_steps(1);
            std::thread::sleep(Duration::from_millis(2));
        }
        let frames = h.ctx.data(|data| data.get_temp::<u64>(egui::Id::new("export-test-frames"))).unwrap();
        h.run_steps(8);
        assert!(h.ctx.data(|data| data.get_temp::<u64>(egui::Id::new("export-test-frames"))).unwrap() >= frames + 8);
        assert!(Arc::ptr_eq(&source, &h.state().session.active().unwrap().doc));
        assert_eq!(*stage.destination.lock().unwrap(), b"existing destination");
        h.get_by_label("Cancel").click();
        h.run_steps(2);
        h.state_mut().session.join_cancelled_jobs();
        assert_eq!(stage.commits.load(Ordering::SeqCst), 0);
        assert_eq!(stage.cleanups.load(Ordering::SeqCst), 1);
        assert_eq!(*stage.destination.lock().unwrap(), b"existing destination");
        assert!(Arc::ptr_eq(&source, &h.state().session.active().unwrap().doc));
        assert!(h.state().session.journal.is_empty());
    }
}

fn dialog_harness(app: PhotocraftApp, pdf: bool) -> egui_kittest::Harness<'static, PhotocraftApp> {
    let mut harness = egui_kittest::Harness::builder().with_size(egui::vec2(760., 480.)).build_ui_state(
        move |ui, app: &mut PhotocraftApp| {
            if !ui.ctx().fonts(|fonts| fonts.families().contains(&egui::FontFamily::Name("medium".into()))) {
                return;
            }
            let ctx = ui.ctx().clone();
            crate::jobs_ui::tick(app, &ctx);
            egui::CentralPanel::default().show(ui, |ui| {
                ui.label("Export fixture canvas");
            });
            if pdf {
                super::super::pdf_export::frame(app, &ctx);
            } else {
                super::super::svg_export::frame(app, &ctx);
            }
        },
        app,
    );
    PhotocraftApp::setup_context(&harness.ctx, crate::theme::ThemeKind::Studio);
    harness.run_steps(3);
    harness
}
fn open_dialog(app: &mut PhotocraftApp, pdf: bool) {
    if pdf {
        app.ui.pdf_export.open(&app.session).unwrap();
    } else {
        app.ui.svg_export.open(&app.session).unwrap();
    }
}
fn spin_ui(h: &mut egui_kittest::Harness<'_, PhotocraftApp>, mut done: impl FnMut(&PhotocraftApp) -> bool) {
    let until = Instant::now() + Duration::from_secs(10);
    while !done(h.state()) {
        assert!(Instant::now() < until, "export fixture deadline");
        h.run_steps(1);
        std::thread::sleep(Duration::from_millis(2));
    }
}
#[test]
fn vector_export_warning_success_keeps_full_path_and_one_warning_result() {
    use egui_kittest::kittest::Queryable;
    for pdf in [false, true] {
        let (mut app, stage, _) = fixture(pdf);
        Arc::make_mut(&mut app.session.active_mut().unwrap().doc).layers[0].effects.items.push(photocraft_doc::Effect::default_drop_shadow());
        let chosen = format!("/chosen/full/path/warning-export.{}", if pdf { "pdf" } else { "svg" });
        let picker_path = chosen.clone();
        app.services.pick_save = Some(Box::new(move |_| Some(picker_path.clone())));
        stage.release.store(true, Ordering::SeqCst);
        open_dialog(&mut app, pdf);
        let mut h = dialog_harness(app, pdf);
        h.get_by_label("Export").click();
        h.run_steps(2);
        spin_ui(&mut h, |_| stage.commits.load(Ordering::SeqCst) == 1);
        h.run_steps(3);
        assert!(h.state().ui.status_error, "export warnings remain visible in status");
        assert!(h.state().ui.status.contains("warnings"));
        assert_eq!(h.state().ui.notice_history.len(), 1, "one coherent file result");
        let result = &h.state().ui.notice_history[0];
        assert!(result.warning_count > 0);
        assert_eq!(result.lines[0], chosen);
        assert!(result.lines.len() > 1);
        assert_eq!(h.state().ui.notices.len(), 1);
    }
}
#[test]
fn vector_export_job_events_report_failure_once_and_global_cancel_quietly() {
    use egui_kittest::kittest::Queryable;
    for pdf in [false, true] {
        for cancel in [false, true] {
            let (mut app, stage, _) = fixture(pdf);
            let chosen = format!("/chosen/full/path/failed destination.{}", if pdf { "pdf" } else { "svg" });
            let picked = chosen.clone();
            app.services.pick_save = Some(Box::new(move |_| Some(picked.clone())));
            stage.fail_stage.store(!cancel, Ordering::SeqCst);
            stage.release.store(!cancel, Ordering::SeqCst);
            open_dialog(&mut app, pdf);
            let mut h = dialog_harness(app, pdf);
            h.get_by_label("Export").click();
            h.run_steps(2);
            spin_ui(&mut h, |_| stage.entered.load(Ordering::SeqCst));
            if cancel {
                let id = h.state().session.jobs().iter().find(|job| job.command == if pdf { "file.pdf.export" } else { "file.svg.export" }).unwrap().id;
                crate::jobs_ui::cancel(h.state_mut(), id);
                h.run_steps(4);
                h.state_mut().session.join_cancelled_jobs();
                assert!(!h.state().ui.status_error);
                assert!(h.state().ui.notice_history.is_empty(), "global Jobs cancel is not a failed export");
                assert!(if pdf { h.state().ui.pdf_export.request.is_none() } else { h.state().ui.svg_export.request.is_none() });
            } else {
                spin_ui(&mut h, |app| !app.ui.notice_history.is_empty());
                h.run_steps(4);
                assert_eq!(h.state().ui.notice_history.len(), 1, "generic and owning-dialog reporters must not duplicate failures");
                assert!(h.state().ui.notice_history[0].lines.iter().any(|line| line.contains("injected stage failure")));
                assert_eq!(h.state().ui.notice_history[0].lines[0], chosen, "full chosen destination retained in failure details");
                assert!(h.state().ui.notice_history[0].error);
                assert_eq!(h.state().ui.notice_history[0].warning_count, 0);
                assert!(!h.state().ui.notice_history[0].title.contains("injected stage failure"), "primary failure stays compact");
                assert!(h.state().ui.status_error);
            }
            assert_eq!(*stage.destination.lock().unwrap(), b"existing destination");
            assert_eq!(stage.commits.load(Ordering::SeqCst), 0);
        }
    }
    // A non-owned export command still receives generic job failure reporting.
    let (mut app, _, _) = fixture(false);
    let started = app
        .session
        .start_job(
            "file.svg.export",
            serde_json::json!({}),
            "external export",
            false,
            |_| Err::<(), _>(EngineError::Other("unowned failure".into())),
            |_, _| Ok(Value::Null),
        )
        .unwrap();
    let Started::Job(id) = started else { panic!("worker") };
    let _ = app.session.wait_job(id);
    crate::jobs_ui::tick(&mut app, &egui::Context::default());
    assert_eq!(app.ui.notice_history.len(), 1);
    assert!(app.ui.notice_history[0].title.contains("unowned failure"), "generic notices::error retains its existing primary-message contract");
    assert!(app.ui.notice_history[0].error);
}
