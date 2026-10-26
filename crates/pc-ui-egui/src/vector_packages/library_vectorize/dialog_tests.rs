//! Uses the real trace dialog; no substitute dialog.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use super::*;
use crate::trace_dialog;
use egui_kittest::{Harness, kittest::Queryable};
use photocraft_engine::vector_packages::trace_output::Preset;

pub(super) struct Host {
    pub app: PhotocraftApp,
    pub handoff: State,
    pub dialog: trace_dialog::State,
    pub prefs: trace_dialog::Preferences,
    pub launches: usize,
    pub bytes: Vec<u8>,
}
impl Host {
    pub fn new(app: PhotocraftApp) -> Self {
        Self {
            app,
            handoff: State::default(),
            dialog: trace_dialog::State::default(),
            prefs: trace_dialog::Preferences { last_preset: Preset::PixelArt },
            launches: 0,
            bytes: tests::png(),
        }
    }
    pub fn draw(&mut self, ui: &mut egui::Ui) {
        if ui.button("Load context photo").clicked() {
            self.handoff.open_file(&mut self.app, 29, "context-photo.png", &self.bytes).unwrap();
        }
        if self.handoff.take_launch(&self.app.session).unwrap().is_some() {
            self.launches += 1;
            self.dialog.open(&mut self.app.session, &self.prefs);
        }
        trace_dialog::show(ui.ctx(), &mut self.dialog, &mut self.app.session, &mut self.prefs, &[]);
    }
    pub fn finish_preview(&mut self) {
        if let Some(job) = self.dialog.pending_preview() {
            self.app.session.wait_job(job).unwrap();
        }
        self.dialog.poll(&mut self.app.session);
        assert!(self.dialog.error.is_none(), "{:?}", self.dialog.error);
        assert!(self.dialog.preview.is_some());
    }
}

#[test]
fn library_vectorize_integrated_dialog_targets_decoded_raster_cancel_retains_loaded_document() {
    let mut h = Harness::builder().with_size(egui::vec2(1200., 900.)).with_max_steps(64).build_ui_state(
        |ui, host: &mut Host| {
            host.app.session.poll_jobs();
            host.draw(ui);
        },
        Host::new(tests::app()),
    );
    let old = h.state().app.session.active().unwrap().doc.clone();
    h.get_by_label("Load context photo").click();
    h.run_steps(3);
    h.state_mut().finish_preview();
    h.run_steps(4);
    assert_eq!(h.state().launches, 1);
    let st = h.state().app.session.active().unwrap();
    let capture = h.state().dialog.capture.as_ref().unwrap();
    assert_eq!(capture.document, st.doc.id.0);
    assert_eq!(capture.revision, st.revision);
    assert_eq!(Some(capture.layer), st.active_layer);
    assert!(matches!(st.doc.layer(capture.layer).unwrap().content, LayerContent::Raster(_)));
    assert_ne!(st.doc.id, old.id);
    let loaded = st.doc.clone();
    let revision = st.revision;
    h.get_by_label("Cancel").click();
    h.run_steps(6);
    assert!(!h.state().dialog.open);
    assert_eq!(h.state().launches, 1);
    assert_eq!(h.state().app.session.active().unwrap().doc, loaded);
    assert_eq!(h.state().app.session.active().unwrap().revision, revision);
    assert_eq!(h.state().app.session.active().unwrap().history.past_len(), 0);
    assert_eq!(h.state().app.session.documents()[0].doc, old);
}

#[test]
fn library_vectorize_already_open_photo_opens_its_dialog() {
    let dir = std::env::temp_dir().join(format!("pc-vectorize-reuse-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("context-photo.png");
    std::fs::write(&path, tests::png()).unwrap();
    let path = path.to_str().unwrap();
    for (background, in_flight) in [(false, false), (true, false), (true, true)] {
        let mut app = tests::app();
        app.background_jobs = in_flight;
        app.open_path(path).unwrap();
        let document_count = if in_flight { 1 } else { 2 };
        app.session.set_active(0);
        app.background_jobs = background;
        let mut host = Host::new(app);
        host.handoff.open_path(&mut host.app, 29, path).unwrap();
        if in_flight {
            assert_eq!(host.app.jobs.opens.len(), 1);
            let job = host.app.jobs.opens[0].job;
            assert_eq!(host.handoff.opening_job(), Some(job));
            host.app.session.wait_job(job).unwrap();
            for event in host.app.session.poll_jobs() {
                host.handoff.on_job_event(&host.app, &event).unwrap();
            }
        } else {
            assert!(host.app.jobs.opens.is_empty());
        }
        assert_eq!(host.app.session.documents().len(), document_count + usize::from(in_flight));
        let existing = host.app.session.active().unwrap().clone();
        assert_eq!(existing.path.as_deref(), Some(path));
        let mut h = Harness::builder().with_size(egui::vec2(1200., 900.)).with_max_steps(64).build_ui_state(|ui, host: &mut Host| host.draw(ui), host);
        h.run_steps(3);
        h.state_mut().finish_preview();
        h.run_steps(3);
        assert!(h.state().dialog.open);
        assert_eq!(h.state().launches, 1);
        assert_eq!(h.state().dialog.capture.as_ref().unwrap().document, existing.doc.id.0);
        assert_eq!(h.state().app.session.documents().len(), 2);
        assert_eq!(h.state().app.session.active().unwrap().doc, existing.doc);
        assert!(h.state().handoff.preserves_library_state(&h.state().app.session));
        h.get_by_label("Cancel").click();
        h.run_steps(4);
        assert!(!h.state().dialog.open);
        assert_eq!(h.state().app.session.active().unwrap().doc, existing.doc);
    }
    std::fs::remove_dir_all(dir).unwrap();
}
