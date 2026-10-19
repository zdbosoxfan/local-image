//! Library navigation: actual events, persistence and no-op controls.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use super::*;
use crate::{
    headless::HeadlessView,
    state::{BeforeAfter, UiState, ViewMode},
};
use lightcraft_catalog::{Op, Photo, PhotoId, Source};
use serde_json::json;
use std::sync::{Arc, Mutex};
struct Harness {
    app: LightcraftApp,
    view: HeadlessView,
    frame: u64,
    size: egui::Vec2,
    kind: u8,
}
impl Harness {
    fn new(n: u64, kind: u8) -> Self {
        let mut session = lightcraft_engine::Session::new();
        for i in 1..=n {
            session
                .catalog
                .apply(Op::AddPhoto {
                    photo: Box::new(Photo::new(
                        PhotoId(i),
                        Source::Demo { scene: 1 },
                        &format!("photo{i}.jpg"),
                        "JPEG",
                        60,
                        40,
                        "2026-10-01T12:00:00",
                    )),
                })
                .unwrap();
        }
        if n > 0 {
            session.execute("library.select", &json!({"ids":[1]})).unwrap();
        }
        let mut app = LightcraftApp::new(session, Services::default());
        app.ui.settings.gpu = false;
        app.ui.view = ViewMode::PhotoGrid;
        app.ui.right = state::RightPanel::None;
        app.ui.left_panel = false;
        let mut h = Self { app, view: HeadlessView::new(), frame: 0, size: egui::vec2(900., 650.), kind };
        for _ in 0..4 {
            h.step(vec![]);
        }
        h
    }
    fn step(&mut self, events: Vec<egui::Event>) {
        self.frame += 1;
        let raw = HeadlessView::raw_input(self.size, 1., self.frame as f64 / 60., events);
        let app = &mut self.app;
        let kind = self.kind;
        self.view.run(raw, |ui| {
            app.logic(ui.ctx());
            if kind == 0 {
                app.ui(ui);
            } else {
                egui::CentralPanel::default().show(ui, |ui| {
                    if matches!(kind, 1 | 3 | 4 | 6) {
                        let d = app.session.develop_of(app.session.active().unwrap()).unwrap();
                        if kind == 3 {
                            panels::edit::control(app, ui, &d, "light.contrast", true);
                        }
                        if kind == 4 {
                            let mid = app.session.active_mask.unwrap();
                            let view = d.masks.iter().find(|m| m.id == mid).unwrap().tools.view((6500., 0.));
                            panels::edit::control_t(app, ui, &view, "light.exposure", true, panels::edit::Target::Layer(mid));
                        } else {
                            panels::edit::control(app, ui, &d, "light.exposure", kind != 6);
                        }
                    } else {
                        panels::enhance::denoise_download(app, ui, "denoise-nind");
                    }
                });
                app.widgets = widgets::take_registry(ui.ctx());
            }
        });
    }
    fn rect(&self, id: &str) -> egui::Rect {
        self.app.widgets.iter().find(|(key, _)| key == id).unwrap_or_else(|| panic!("missing {id}")).1
    }
    fn has(&self, id: &str) -> bool {
        self.app.widgets.iter().any(|(key, _)| key == id)
    }
    fn click(&mut self, id: &str) {
        let rect = self.rect(id);
        assert!(egui::Rect::from_min_size(egui::Pos2::ZERO, self.size).contains_rect(rect), "{id}: {rect:?}");
        let pos = rect.center();
        self.step(vec![
            egui::Event::PointerMoved(pos),
            egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed: true, modifiers: egui::Modifiers::NONE },
        ]);
        self.step(vec![egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed: false, modifiers: egui::Modifiers::NONE }]);
        self.step(vec![]);
    }
    fn key(&mut self, key: egui::Key) {
        self.step(vec![key_event(key, true)]);
        self.step(vec![key_event(key, false)]);
    }
}
fn key_event(key: egui::Key, pressed: bool) -> egui::Event {
    egui::Event::Key { key, physical_key: None, pressed, repeat: false, modifiers: egui::Modifiers::NONE }
}
#[test]
fn library_navigation_original_never_latches_grid_or_persists() {
    let mut h = Harness::new(3, 0);
    h.key(egui::Key::Backslash);
    assert_eq!(h.app.ui.before_after, BeforeAfter::Off);
    h.app.run("view.showOriginal", json!({})).unwrap();
    assert_eq!(h.app.ui.before_after, BeforeAfter::Off);
    h.app.run("view.detail", json!({})).unwrap();
    h.step(vec![]);
    h.key(egui::Key::Backslash);
    assert_eq!(h.app.ui.before_after, BeforeAfter::Off, "held keyboard comparison never toggles the menu state");
    h.app.run("view.showOriginal", json!({})).unwrap();
    assert_eq!(h.app.ui.before_after, BeforeAfter::Original);
    let restored: UiState = serde_json::from_value(serde_json::to_value(&h.app.ui).unwrap()).unwrap();
    assert_eq!(restored.before_after, BeforeAfter::Off);
    h.app.ui.before_after = BeforeAfter::Split;
    let restored: UiState = serde_json::from_value(serde_json::to_value(&h.app.ui).unwrap()).unwrap();
    assert_eq!(restored.before_after, BeforeAfter::Split, "explicit comparison preference survives");
    h.app.ui.before_after = BeforeAfter::Original;
    h.app.run("view.photoGrid", json!({})).unwrap();
    h.step(vec![]);
    assert_eq!(h.app.ui.before_after, BeforeAfter::Off);
}
#[test]
fn library_navigation_four_directions_follow_visible_rows_and_preserve_edges() {
    let mut h = Harness::new(24, 0);
    h.app.ui.thumb_size = 160.;
    for _ in 0..3 {
        h.step(vec![]);
    }
    for view in ["view.photoGrid", "view.squareGrid"] {
        h.app.run(view, json!({})).unwrap();
        h.app.run("library.select", json!({"ids":[1]})).unwrap();
        for _ in 0..3 {
            h.step(vec![]);
        }
        let ids = h.app.session.visible_cloned();
        let first = ids[0];
        h.app.run("library.select", json!({"ids":[first.0]})).unwrap();
        h.step(vec![]);
        let first_rect = h.rect(&format!("thumb:{}", first.0));
        let below = ids
            .iter()
            .filter_map(|id| h.app.widgets.iter().find(|(k, _)| k == &format!("thumb:{}", id.0)).map(|(_, r)| (*id, *r)))
            .filter(|(_, r)| r.top() > first_rect.bottom())
            .min_by(|(_, a), (_, b)| {
                a.top()
                    .total_cmp(&b.top())
                    .then_with(|| (a.center().x - first_rect.center().x).abs().total_cmp(&(b.center().x - first_rect.center().x).abs()))
            })
            .unwrap()
            .0;
        h.key(egui::Key::ArrowUp);
        assert_eq!(h.app.session.active(), Some(first));
        h.key(egui::Key::ArrowLeft);
        assert_eq!(h.app.session.active(), Some(first));
        h.key(egui::Key::ArrowDown);
        assert_eq!(h.app.session.active(), Some(below));
        h.key(egui::Key::ArrowUp);
        assert_eq!(h.app.session.active(), Some(first));
        h.key(egui::Key::ArrowRight);
        assert_eq!(h.app.session.active(), Some(ids[1]));
        h.key(egui::Key::ArrowLeft);
        assert_eq!(h.app.session.active(), Some(first));
    }
    h.app.run("dialog.export", json!({})).unwrap();
    h.step(vec![]);
    let selected = h.app.session.active();
    h.key(egui::Key::ArrowDown);
    assert_eq!(h.app.session.active(), selected, "dialog owns input");
}
#[test]
fn library_navigation_typed_values_commit_once_cancel_and_reject_invalid() {
    let mut h = Harness::new(1, 1);
    let id = h.app.session.active().unwrap();
    let exposure = |h: &Harness| h.app.session.develop_of(id).unwrap().light.exposure;
    for (text, key, want) in [
        ("1.25", egui::Key::Enter, 1.25),
        ("2.5", egui::Key::Escape, 1.25),
        ("NaN", egui::Key::Enter, 1.25),
        ("999999", egui::Key::Enter, 1.25),
        ("-", egui::Key::Enter, 1.25),
    ] {
        let prior = exposure(&h);
        h.click("value:light.exposure");
        h.step(vec![egui::Event::Text(text.into())]);
        assert_eq!(exposure(&h), prior, "typing is not an edit");
        h.key(key);
        assert_eq!(exposure(&h), want);
    }
    h.app.run("edit.undo", json!({})).unwrap();
    assert_eq!(exposure(&h), 0., "one undo reverses the one valid numeric commit");
}
#[test]
fn library_navigation_footer_compact_bounds_and_smart_sort_destination() {
    let mut h = Harness::new(3, 0);
    h.size = egui::vec2(760., 480.);
    h.app.run("view.develop", json!({})).unwrap();
    for _ in 0..4 {
        h.step(vec![]);
    }
    let left = h.rect("footer:left");
    let right = h.rect("footer:right");
    assert!(left.right() <= right.left(), "footer control groups overlap: {left:?}/{right:?}");
    for id in ["button:compactViews", "icon:sort", "icon:beforeAfter", "icon:filmstrip", "dropdown:zoom"] {
        let r = h.rect(id);
        assert!(egui::Rect::from_min_size(egui::Pos2::ZERO, h.size).contains_rect(r));
    }
    h.click("button:compactViews");
    h.click("button:compactView:photoGrid");
    assert_eq!(h.app.ui.view, ViewMode::PhotoGrid);
    h.click("icon:sort");
    h.click("button:sortSmartExport");
    assert!(matches!(h.app.ui.dialog, Some(state::Dialog::SmartSort { .. })));
    // Use the real chooser on the Export step; a cancelled chooser must retain memory.
    h.size = egui::vec2(1280., 800.);
    h.app.services.file_dialog = Some(crate::file_dialog::choosing("/tmp/Chosen Photos"));
    if let Some(state::Dialog::SmartSort { state }) = &mut h.app.ui.dialog {
        state.step = 2;
    }
    for _ in 0..4 {
        h.step(vec![]);
    }
    h.click("smartSort:chooseDest");
    assert_eq!(h.app.ui.smart_sort_destination, "/tmp/Chosen Photos");
    h.app.services.file_dialog = Some(crate::file_dialog::answering(|_| None));
    h.click("smartSort:chooseDest");
    assert_eq!(h.app.ui.smart_sort_destination, "/tmp/Chosen Photos");
    let saved = serde_json::to_value(&h.app.ui).unwrap();
    h.app.ui = serde_json::from_value(saved).unwrap();
    h.app.ui.dialog = None;
    h.app.run("dialog.smartSort", json!({})).unwrap();
    assert!(matches!(&h.app.ui.dialog,Some(state::Dialog::SmartSort{state}) if state.dest=="/tmp/Chosen Photos"));
}
#[test]
fn library_navigation_denoise_download_reflects_running_ready_and_failure() {
    let mut h = Harness::new(1, 2);
    let status = Arc::new(Mutex::new(ModelDownloadStatus::default()));
    let installed = Arc::new(Mutex::new(false));
    let calls = Arc::new(Mutex::new(Vec::new()));
    let read = status.clone();
    h.app.services.model_download_status = Some(Box::new(move |_| read.lock().unwrap().clone()));
    let read = installed.clone();
    h.app.services.model_installed = Some(Box::new(move |_| *read.lock().unwrap()));
    let sink = calls.clone();
    h.app.services.download_models = Some(Box::new(move |id, cancel| {
        sink.lock().unwrap().push((id.to_string(), cancel));
        Ok(())
    }));
    h.step(vec![]);
    h.click("button:denoiseDownload");
    *status.lock().unwrap() = ModelDownloadStatus { running: true, done: 25, total: 100, error: None };
    h.step(vec![]);
    assert!(h.has("progress:denoiseDownload"));
    assert!(!h.has("button:denoiseDownload"));
    h.click("button:denoiseDownloadCancel");
    *status.lock().unwrap() = ModelDownloadStatus::default();
    h.step(vec![]);
    assert!(h.has("button:denoiseDownload"), "cancelled download can be resumed");
    *status.lock().unwrap() = ModelDownloadStatus { error: Some("fixture failure".into()), ..Default::default() };
    h.step(vec![]);
    h.click("button:denoiseDownload");
    *status.lock().unwrap() = ModelDownloadStatus::default();
    *installed.lock().unwrap() = true;
    h.step(vec![]);
    assert!(h.has("status:denoiseReady"));
    assert!(!h.has("button:denoiseDownload"));
    assert_eq!(*calls.lock().unwrap(), vec![("denoise-nind".into(), false), ("denoise-nind".into(), true), ("denoise-nind".into(), false)]);
    let mut restarted = Harness::new(1, 2);
    restarted.app.services.model_installed = Some(Box::new(|_| true));
    restarted.step(vec![]);
    assert!(restarted.has("status:denoiseReady"));
    assert!(!restarted.has("button:denoiseDownload"), "ready survives absent download history after restart");
}

#[test]
fn library_navigation_numeric_drafts_cancel_across_photos_masks_and_libraries() {
    let mut h = Harness::new(2, 1);
    h.click("value:light.exposure");
    h.step(vec![egui::Event::Text("2.25".into())]);
    h.app.run("library.select", json!({"ids":[2]})).unwrap();
    h.step(vec![]);
    h.key(egui::Key::Enter);
    for id in [PhotoId(1), PhotoId(2)] {
        assert_eq!(h.app.session.develop_of(id).unwrap().light.exposure, 0.);
    }
    h.app.run("library.select", json!({"ids":[1]})).unwrap();
    h.step(vec![]);
    h.key(egui::Key::Enter);
    assert_eq!(h.app.session.develop_of(PhotoId(1)).unwrap().light.exposure, 0.);
    for _ in 0..2 {
        h.app.run("mask.add", json!({"kind":"radial"})).unwrap();
    }
    let masks: Vec<_> = h.app.session.develop_of(PhotoId(1)).unwrap().masks.iter().map(|m| m.id).collect();
    h.kind = 4;
    h.app.run("mask.select", json!({"id":masks[0]})).unwrap();
    h.step(vec![]);
    h.click("value:light.exposure");
    h.step(vec![egui::Event::Text("1.75".into())]);
    h.app.run("mask.select", json!({"id":masks[1]})).unwrap();
    h.step(vec![]);
    h.key(egui::Key::Enter);
    for mask in &h.app.session.develop_of(PhotoId(1)).unwrap().masks {
        assert_eq!(mask.tools.view((6500., 0.)).light.exposure, 0.);
    }
    h.app.run("mask.select", json!({"id":masks[0]})).unwrap();
    h.step(vec![]);
    h.key(egui::Key::Enter);
    assert_eq!(h.app.session.develop_of(PhotoId(1)).unwrap().masks[0].tools.view((6500., 0.)).light.exposure, 0.);
    h.kind = 1;
    h.app.session.active_mask = None;
    h.step(vec![]);
    h.click("value:light.exposure");
    h.step(vec![egui::Event::Text("3.5".into())]);
    let generation = h.app.session.library_generation;
    h.app
        .session
        .open_library_in(
            lightcraft_engine::library::LibraryStores {
                dir: "numeric-replacement-library".into(),
                catalog: Box::new(lightcraft_catalog::MemStore::new()),
                files: Box::new(lightcraft_catalog::MemStore::new()),
                on_disk: false,
            },
            false,
        )
        .unwrap();
    assert_ne!(h.app.session.library_generation, generation);
    h.app
        .session
        .catalog
        .apply(Op::AddPhoto {
            photo: Box::new(Photo::new(PhotoId(1), Source::Demo { scene: 1 }, "replacement.jpg", "JPEG", 60, 40, "2026-10-01T12:00:00")),
        })
        .unwrap();
    h.app.run("library.select", json!({"ids":[1]})).unwrap();
    h.step(vec![]);
    h.key(egui::Key::Enter);
    assert_eq!(h.app.session.develop_of(PhotoId(1)).unwrap().light.exposure, 0., "reused PhotoId in a new library cannot receive the old draft");
}
#[test]
fn library_navigation_numeric_focus_blur_arrow_and_disappearance() {
    let mut h = Harness::new(1, 3);
    let id = h.app.session.active().unwrap();
    h.click("value:light.exposure");
    h.step(vec![egui::Event::Text("1.25".into())]);
    let history = h.app.session.catalog.photo(id).unwrap().history.len();
    let other_row = h.rect("slider:light.contrast").center();
    h.step(vec![egui::Event::PointerMoved(other_row)]);
    h.key(egui::Key::ArrowUp);
    h.key(egui::Key::ArrowDown);
    assert_eq!(h.app.session.develop_of(id).unwrap().light.exposure, 0., "draft arrows never mutate the edited slider");
    assert_eq!(h.app.session.develop_of(id).unwrap().light.contrast, 0., "hovering another earlier-drawn row cannot steal numeric-field arrows");
    assert_eq!(h.app.session.catalog.photo(id).unwrap().history.len(), history, "cross-row arrows make no undo entry");
    // Contrast is drawn earlier: its focus transfer must still commit Exposure once.
    h.click("value:light.contrast");
    assert_eq!(h.app.session.develop_of(id).unwrap().light.exposure, 1.25);
    h.step(vec![egui::Event::Text("20".into())]);
    h.key(egui::Key::Enter);
    assert_eq!(h.app.session.develop_of(id).unwrap().light.contrast, 20.);
    h.app.run("edit.undo", json!({})).unwrap();
    assert_eq!(h.app.session.develop_of(id).unwrap().light.contrast, 0.);
    assert_eq!(h.app.session.develop_of(id).unwrap().light.exposure, 1.25);
    h.app.run("edit.undo", json!({})).unwrap();
    assert_eq!(h.app.session.develop_of(id).unwrap().light.exposure, 0.);
    h.click("value:light.exposure");
    h.step(vec![egui::Event::Text("4.5".into())]);
    h.kind = 2;
    for _ in 0..3 {
        h.step(vec![]);
    }
    h.kind = 3;
    h.step(vec![]);
    h.click("value:light.contrast");
    h.step(vec![egui::Event::Text("10".into())]);
    h.key(egui::Key::Enter);
    assert_eq!(h.app.session.develop_of(id).unwrap().light.exposure, 0., "disappeared draft must not resurrect or steal another field's Enter");
    assert_eq!(h.app.session.develop_of(id).unwrap().light.contrast, 10.);
    h.click("value:light.exposure");
    h.step(vec![egui::Event::Text("2.5".into())]);
    h.app.run("develop.set", json!({"control":"light.exposure","value":0.5})).unwrap();
    h.step(vec![]);
    h.key(egui::Key::Enter);
    assert_eq!(h.app.session.develop_of(id).unwrap().light.exposure, 0.5, "external base-value changes cancel a draft");
    h.click("value:light.exposure");
    h.step(vec![egui::Event::Text("3.25".into())]);
    h.kind = 6;
    h.step(vec![]);
    h.key(egui::Key::Enter);
    assert_eq!(h.app.session.develop_of(id).unwrap().light.exposure, 0.5, "disabled field cancels its draft");
    h.kind = 3;
    h.step(vec![]);
    h.click("value:light.exposure");
    h.step(vec![egui::Event::Text("4.25".into())]);
    h.app.run("dialog.export", json!({})).unwrap();
    h.step(vec![]);
    h.key(egui::Key::Enter);
    assert_eq!(
        h.app.session.develop_of(id).unwrap().light.exposure,
        0.5,
        "opening a dialog cancels rather than blur-committing underlying numeric text"
    );
}
struct HoldRenders;
impl crate::render::RenderOffload for HoldRenders {
    fn try_start(&mut self, _: crate::render::Slot, job: lightcraft_engine::media::RenderJob) -> Option<lightcraft_engine::media::RenderJob> {
        Some(job)
    }
    fn finished(&mut self) -> Vec<(crate::render::Slot, lightcraft_engine::media::RenderResult, f64)> {
        Vec::new()
    }
}
#[test]
fn library_navigation_held_original_draws_before_then_returns_to_edit() {
    use crate::render::{Renderer, Slot, Tex};
    let mut h = Harness::new(1, 0);
    h.app.renderer = Renderer::default();
    h.app.renderer.set_offload(Box::new(HoldRenders));
    h.app.run("view.detail", json!({})).unwrap();
    h.app.run("develop.set", json!({"control":"light.exposure","value":1.})).unwrap();
    for (slot, color) in [(Slot::Main, egui::Color32::RED), (Slot::Before, egui::Color32::BLUE)] {
        let pixels = Arc::new(egui::ColorImage::filled([60, 40], color));
        let tex = h.view.ctx.load_texture(format!("oracle-{slot:?}"), pixels.clone(), egui::TextureOptions::NEAREST);
        h.app
            .renderer
            .textures
            .insert(slot, Tex { key: 0, photo: PhotoId(1), tex, size: [60, 40], histogram: None, ms: 0., quick: None, pixels: Some(pixels) });
    }
    let pixel = |h: &Harness| {
        let c = h.app.canvas_rect.unwrap().center();
        h.view.paint(&h.app.renderer.cpu_textures())[(c.x as usize, c.y as usize)]
    };
    h.step(vec![]);
    assert_eq!(pixel(&h), egui::Color32::RED);
    h.step(vec![key_event(egui::Key::Backslash, true)]);
    assert_eq!(pixel(&h), egui::Color32::BLUE);
    assert!(h.app.renderer.is_pending(Slot::Before));
    h.step(vec![key_event(egui::Key::Backslash, false)]);
    assert_eq!(pixel(&h), egui::Color32::RED);
    h.click("field:search");
    h.step(vec![key_event(egui::Key::Backslash, true)]);
    assert_eq!(pixel(&h), egui::Color32::RED, "focused text input cannot switch displayed pixels to Before");
    h.step(vec![key_event(egui::Key::Backslash, false)]);
    h.app.run("dialog.export", json!({})).unwrap();
    h.step(vec![]);
    h.step(vec![key_event(egui::Key::Backslash, true)]);
    assert_eq!(h.app.ui.before_after, BeforeAfter::Off, "modal key cannot latch hidden Original state");
}

#[test]
fn library_navigation_two_mask_components_keep_independent_numeric_fields() {
    let mut h = Harness::new(1, 0);
    h.size = egui::vec2(1200., 1600.);
    h.app.run("view.develop", json!({})).unwrap();
    h.app.run("panel.masking", json!({})).unwrap();
    h.app.run("mask.add", json!({"kind":"luminanceRange","lo":0.2,"hi":0.8})).unwrap();
    h.app.run("mask.addComponent", json!({"kind":"luminanceRange","op":"add","lo":0.3,"hi":0.7})).unwrap();
    for _ in 0..4 {
        h.step(vec![]);
    }
    let id = h.app.session.active().unwrap();
    let before = h.app.session.develop_of(id).unwrap().masks[0].components.clone();
    let fields: Vec<_> = h.app.widgets.iter().filter(|(key, _)| key == "value:smoothness").map(|(_, r)| *r).collect();
    assert_eq!(fields.len(), 2);
    let pos = fields[1].center();
    assert!(egui::Rect::from_min_size(egui::Pos2::ZERO, h.size).contains_rect(fields[1]));
    h.step(vec![
        egui::Event::PointerMoved(pos),
        egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed: true, modifiers: egui::Modifiers::NONE },
    ]);
    h.step(vec![egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed: false, modifiers: egui::Modifiers::NONE }]);
    h.step(vec![]);
    h.step(vec![egui::Event::Text("50".into())]);
    assert_eq!(h.app.session.develop_of(id).unwrap().masks[0].components, before);
    h.key(egui::Key::Enter);
    let after = h.app.session.develop_of(id).unwrap().masks[0].components.clone();
    assert_eq!(after[0], before[0]);
    assert_ne!(after[1], before[1], "second same-kind component retains and commits its own numeric draft");
    h.app.run("edit.undo", json!({})).unwrap();
    assert_eq!(h.app.session.develop_of(id).unwrap().masks[0].components, before);
}

#[test]
fn library_navigation_numeric_auto_sync_authority_changes_cancel_drafts() {
    let mut h = Harness::new(3, 1);
    let values = |h: &Harness| [1, 2, 3].map(|id| h.app.session.develop_of(PhotoId(id)).unwrap().light.exposure);
    h.app.run("library.select", json!({"ids":[1,2],"active":1})).unwrap();
    h.app.run("develop.autoSync", json!({"on":true})).unwrap();
    h.step(vec![]);
    h.click("value:light.exposure");
    h.step(vec![egui::Event::Text("1.25".into())]);
    assert_eq!(values(&h), [0.; 3]);
    h.app.run("library.select", json!({"ids":[1,3],"active":1})).unwrap();
    assert_eq!(h.app.session.active(), Some(PhotoId(1)));
    h.step(vec![]);
    h.key(egui::Key::Enter);
    assert_eq!(values(&h), [0.; 3], "same active photo cannot retarget pending Auto Sync to C");
    for (before, after) in [(false, true), (true, false)] {
        h.app.run("library.select", json!({"ids":[1,2],"active":1})).unwrap();
        h.app.run("develop.autoSync", json!({"on":before})).unwrap();
        h.step(vec![]);
        h.click("value:light.exposure");
        h.step(vec![egui::Event::Text("2.5".into())]);
        h.app.run("develop.autoSync", json!({"on":after})).unwrap();
        h.step(vec![]);
        h.key(egui::Key::Enter);
        assert_eq!(values(&h), [0.; 3], "Auto Sync policy change must cancel pending numeric authority");
    }
    // Unchanged authority still performs the real multi-photo edit as one undo step.
    h.app.run("develop.autoSync", json!({"on":true})).unwrap();
    h.step(vec![]);
    h.click("value:light.exposure");
    h.step(vec![egui::Event::Text("0.75".into())]);
    h.key(egui::Key::Enter);
    assert_eq!(values(&h), [0.75, 0.75, 0.]);
    h.app.run("edit.undo", json!({})).unwrap();
    assert_eq!(values(&h), [0.; 3]);
}
