#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use super::*;
use egui_kittest::{Harness, kittest::Queryable};
use image::ImageEncoder;
use serde_json::json;
use std::time::{Duration, Instant};

pub(super) fn png() -> Vec<u8> {
    let pixels: Vec<u8> = (0..12).flat_map(|i| if i % 2 == 0 { [255, 0, 0, 255] } else { [0, 0, 255, 255] }).collect();
    let mut bytes = Vec::new();
    image::codecs::png::PngEncoder::new(&mut bytes).write_image(&pixels, 4, 3, image::ExtendedColorType::Rgba8).unwrap();
    bytes
}
pub(super) fn app() -> PhotocraftApp {
    let mut session = Session::new();
    session.execute("file.new", json!({"width":4,"height":3,"background":"white"})).unwrap();
    let services = crate::Services {
        import: Some(Box::new(|name, bytes| {
            let result = photocraft_io::import(name, bytes).map_err(|e| e.to_string())?;
            Ok((result.document, result.warnings))
        })),
        ..Default::default()
    };
    PhotocraftApp::new(session, services)
}

#[test]
fn library_vectorize_kittest_open_consumes_new_raster_launch_once_cancel_retains_photo() {
    struct Host {
        app: PhotocraftApp,
        handoff: State,
        launches: Vec<Launch>,
    }
    let mut h = Harness::builder().with_size(egui::vec2(600., 400.)).build_ui_state(
        |ui, host: &mut Host| {
            if ui.button("Open context photo").clicked() {
                host.handoff.open_file(&mut host.app, 29, "context-photo.png", &png()).unwrap();
            }
            if let Some(launch) = host.handoff.take_launch(&host.app.session).unwrap() {
                host.launches.push(launch);
            }
            if ui.button("Cancel pending launch").clicked() {
                host.handoff.cancel(&mut host.app);
            }
        },
        Host { app: app(), handoff: State::default(), launches: vec![] },
    );
    let old = h.state().app.session.active().unwrap().doc.clone();
    h.get_by_label("Open context photo").click();
    h.run_steps(5);
    let host = h.state();
    assert_eq!(host.launches.len(), 1);
    let launch = &host.launches[0];
    assert_eq!(launch.photo, 29);
    let st = host.app.session.active().unwrap();
    assert_ne!(launch.document, old.id);
    assert_eq!(launch.document, st.doc.id);
    assert_eq!(st.doc.layer(launch.layer).unwrap().surface().unwrap().pixel(0, 0), vec![1., 0., 0., 1.]);
    assert_eq!(st.doc.layer(launch.layer).unwrap().surface().unwrap().pixel(1, 0), vec![0., 0., 1., 1.]);
    assert_eq!(st.history.past_len(), 0);
    assert_eq!(host.app.session.documents().len(), 2);
    assert!(host.handoff.preserves_library_state(&host.app.session));
    let loaded = st.doc.clone();
    let revision = st.revision;
    h.get_by_label("Cancel pending launch").click();
    h.run_steps(4);
    assert_eq!(h.state().app.session.active().unwrap().doc, loaded);
    assert_eq!(h.state().app.session.active().unwrap().revision, revision);
    assert_eq!(h.state().launches.len(), 1);
    assert!(h.state().handoff.preserves_library_state(&h.state().app.session));
    assert_eq!(h.state().app.session.documents()[0].doc, old);
}

#[test]
fn library_vectorize_decode_failure_retains_existing_document_selection_history() {
    let mut app = app();
    let mut state = State::default();
    let old = app.session.active().unwrap().clone();
    assert!(state.open_file(&mut app, 29, "broken.png", b"not a PNG").is_err());
    assert!(!state.pending());
    assert!(!state.preserves_library_state(&app.session));
    assert!(state.take_launch(&app.session).unwrap().is_none());
    assert_eq!(app.session.documents().len(), 1);
    let current = app.session.active().unwrap();
    assert_eq!(current.doc, old.doc);
    assert_eq!(current.revision, old.revision);
    assert_eq!(current.active_layer, old.active_layer);
    assert_eq!(current.selected_layers(), old.selected_layers());
    assert_eq!(current.history.past_len(), old.history.past_len());
}

#[test]
fn library_vectorize_stale_document_revision_and_layer_selection_discard_launch() {
    for change in 0..3 {
        let mut app = app();
        let mut state = State::default();
        state.open_file(&mut app, 29, "context-photo.png", &png()).unwrap();
        match change {
            0 => {
                app.session.set_active(0);
            }
            1 => {
                app.session.execute("layer.new.layer", json!({})).unwrap();
            }
            _ => {
                app.session.active_mut().unwrap().selected_layers.clear();
                app.session.active_mut().unwrap().active_layer = None;
            }
        }
        let doc = app.session.active().unwrap().doc.clone();
        let revision = app.session.active().unwrap().revision;
        assert!(state.take_launch(&app.session).is_err());
        assert!(state.take_launch(&app.session).unwrap().is_none());
        assert_eq!(app.session.active().unwrap().doc, doc);
        assert_eq!(app.session.active().unwrap().revision, revision);
    }
}

fn finish(state: &mut State, app: &mut PhotocraftApp) -> Vec<JobEvent> {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let events = app.session.poll_jobs();
        if !events.is_empty() {
            for event in &events {
                state.on_job_event(app, event).unwrap();
            }
            return events;
        }
        assert!(Instant::now() < deadline, "file decoder timed out");
        std::thread::sleep(Duration::from_millis(1));
    }
}

#[test]
fn library_vectorize_background_open_uses_exact_job_and_ignores_unrelated_completion() {
    let mut app = app();
    app.background_jobs = true;
    let mut state = State::default();
    state.open_file(&mut app, 29, "context-photo.png", &png()).unwrap();
    let job = state.opening_job().unwrap();
    assert_eq!(app.jobs.opens.len(), 1, "one existing progress tab");
    assert!(state.take_launch(&app.session).unwrap().is_none(), "decode must finish before launch");
    let unrelated = JobEvent {
        id: JobId(job.0 + 1000),
        command: OPEN_JOB.into(),
        label: "Other open".into(),
        document: None,
        outcome: JobOutcome::Done(json!({"document":0})),
    };
    assert!(!state.on_job_event(&app, &unrelated).unwrap());
    assert_eq!(state.opening_job(), Some(job));
    assert!(state.open_file(&mut app, 30, "second.png", &png()).is_err());
    assert_eq!(app.jobs.opens.len(), 1);
    let events = finish(&mut state, &mut app);
    assert_eq!(events.len(), 1);
    let launch = state.take_launch(&app.session).unwrap().unwrap();
    assert_eq!(launch.photo, 29);
    assert!(state.take_launch(&app.session).unwrap().is_none());
    // Duplicate delivery cannot relaunch even after normal completion.
    assert!(!state.on_job_event(&app, &events[0]).unwrap());
    assert!(state.take_launch(&app.session).unwrap().is_none());
}

#[test]
fn library_vectorize_background_failure_and_cancel_cannot_launch_or_replace_old_work() {
    for cancel in [false, true] {
        let mut app = app();
        app.background_jobs = true;
        let mut state = State::default();
        let old = app.session.active().unwrap().clone();
        let bytes = if cancel { png() } else { b"not a PNG".to_vec() };
        state.open_file(&mut app, 29, if cancel { "valid.png" } else { "broken.png" }, &bytes).unwrap();
        if cancel {
            state.cancel(&mut app);
        }
        let events = finish(&mut state, &mut app);
        assert!(if cancel { matches!(events[0].outcome, JobOutcome::Cancelled) } else { matches!(events[0].outcome, JobOutcome::Failed(_)) });
        assert!(!state.pending());
        assert!(state.take_launch(&app.session).unwrap().is_none());
        let st = app.session.active().unwrap();
        assert_eq!(st.doc, old.doc);
        assert_eq!(st.revision, old.revision);
        assert_eq!(st.selected_layers(), old.selected_layers());
        assert_eq!(st.history.past_len(), old.history.past_len());
        assert_eq!(app.session.documents().len(), 1);
    }
}

#[test]
fn library_vectorize_nonraster_open_never_retargets_existing_photo() {
    let mut app = app();
    app.services.import = Some(Box::new(|_, _| {
        let doc = photocraft_doc::Document::new(
            "Empty vector document",
            photocraft_doc::Size::new(4, 3),
            photocraft_color::ColorMode::Rgb,
            photocraft_color::SampleType::U8,
        );
        Ok((doc, vec![]))
    }));
    let old = app.session.active().unwrap().doc.clone();
    let mut state = State::default();
    assert!(state.open_file(&mut app, 29, "empty.pcraft", b"fixture").is_err());
    assert!(state.take_launch(&app.session).unwrap().is_none());
    assert_eq!(app.session.documents()[0].doc, old);
    assert_eq!(app.session.documents().len(), 2, "even unsuitable loaded docs are retained");
    assert!(state.preserves_library_state(&app.session));
}
