#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use super::*;
use egui::{Event, Key, Modifiers, PointerButton};
use egui_kittest::{
    Harness,
    kittest::{NodeT, Queryable},
};
use photocraft_color::{ColorMode, SampleType};
use photocraft_doc::{Document, Layer, Size};

pub(super) fn session(width: u32, height: u32) -> Session {
    let mut doc = Document::new("Trace UI fixture", Size::new(width, height), ColorMode::Rgb, SampleType::U8);
    let mut layer = Layer::raster("Logo", doc.pixel_format());
    let surface = layer.surface_mut().unwrap();
    surface.fill_rect(photocraft_geom::Rect::from_xywh(2, 2, width / 4, height / 4), &[1., 0., 0., 1.]);
    surface.fill_rect(photocraft_geom::Rect::from_xywh(width as i32 / 2, 2, width / 4, height / 4), &[0., 0., 1., 1.]);
    doc.layers.push(layer);
    let mut s = Session::new();
    s.add_document(doc, None);
    s
}
struct Host {
    state: State,
    prefs: Preferences,
    session: Session,
    swatches: Vec<[u8; 3]>,
}
fn harness() -> Harness<'static, Host> {
    Harness::builder().with_size(egui::vec2(1200., 900.)).with_max_steps(64).build_ui_state(
        |ui, h: &mut Host| {
            h.session.poll_jobs();
            if ui.button("Open Vectorize").clicked() {
                h.state.open(&mut h.session, &h.prefs);
            }
            show(ui.ctx(), &mut h.state, &mut h.session, &mut h.prefs, &h.swatches);
        },
        Host {
            state: State::default(),
            prefs: Preferences { last_preset: Preset::PixelArt },
            session: session(32, 24),
            swatches: vec![[0, 255, 0], [255, 0, 255]],
        },
    )
}
fn open(h: &mut Harness<'_, Host>) {
    h.get_by_label("Open Vectorize").click();
    h.run_steps(3);
    finish_preview(h);
}
fn finish_preview(h: &mut Harness<'_, Host>) {
    if let Some(id) = h.state().state.pending_preview() {
        h.state_mut().session.wait_job(id).unwrap();
    }
    h.run_steps(3);
    assert!(h.state().state.error.is_none(), "{:?}", h.state().state.error);
}
fn pointer(h: &mut Harness<'_, Host>, pos: Pos2, down: Option<bool>) {
    h.event(Event::PointerMoved(pos));
    if let Some(pressed) = down {
        h.event(Event::PointerButton { pos, button: PointerButton::Primary, pressed, modifiers: Modifiers::NONE });
    }
    h.step();
}
#[test]
fn trace28_ui_open_raster_preset_controls_persist_and_cancel_no_changes() {
    let mut h = harness();
    let before = h.state().session.active().unwrap().doc.clone();
    open(&mut h);
    assert!(!h.get_by_role_and_label(egui::accesskit::Role::Button, "Trace").accesskit_node().is_disabled());
    h.get_by_role_and_label(egui::accesskit::Role::ComboBox, "Preset").click();
    h.run_steps(3);
    h.get_by_label("Black & White").click();
    h.run_steps(3);
    finish_preview(&mut h);
    assert_eq!(h.state().state.params.max_colors, 1);
    assert_eq!(h.state().state.params.speckle, 2);
    assert_eq!(h.state().prefs.last_preset, Preset::BlackWhite);
    let stored = serde_json::to_value(&h.state().prefs).unwrap();
    let loaded: Preferences = serde_json::from_value(stored).unwrap();
    assert_eq!(loaded.last_preset, Preset::BlackWhite);
    let old: Preferences = serde_json::from_value(json!({})).unwrap();
    assert_eq!(old.last_preset, Preset::Logo);
    h.get_by_label("Cancel").click();
    h.run_steps(3);
    assert!(!h.state().state.open);
    assert_eq!(h.state().session.active().unwrap().doc, before);
    assert!(h.state().session.journal.is_empty());
    h.get_by_label("Open Vectorize").click();
    h.run_steps(3);
    assert_eq!(h.state().state.params.preset, Preset::BlackWhite);
}
#[test]
fn trace28_ui_nonpixel_trace_disabled_tooltip_and_unavailable_modes() {
    let mut h = harness();
    h.state_mut().session.execute("shape.create", json!({"kind":"rect","rect":[0,0,10,10]})).unwrap();
    h.get_by_label("Open Vectorize").click();
    h.run_steps(3);
    let button = h.get_by_role_and_label(egui::accesskit::Role::Button, "Trace");
    assert!(button.accesskit_node().is_disabled());
    assert_eq!(h.query_all_by_label("Vectorize requires a pixel layer or smart object").count(), 1);
    button.hover();
    h.run_steps(40);
    assert!(
        h.query_all_by_label("Vectorize requires a pixel layer or smart object").count() >= 2,
        "disabled hover displays the tooltip in addition to the status label"
    );
    h.get_by_role_and_label(egui::accesskit::Role::ComboBox, "Preset").click();
    h.run_steps(3);
    assert!(h.get_by_label("Line Art").accesskit_node().is_disabled());
    assert!(h.get_by_label("Sketch").accesskit_node().is_disabled());
}
#[test]
fn trace28_ui_slider_drag_supersedes_reduced_preview_release_latest_wins() {
    let mut h = harness();
    open(&mut h);
    let rect = h.get_by_role_and_label(egui::accesskit::Role::Slider, "Detail").rect();
    pointer(&mut h, rect.center(), Some(true));
    let old = h.state().state.pending_preview().unwrap();
    pointer(&mut h, egui::pos2(rect.right() - 3., rect.center().y), None);
    let old_state = h.state().session.jobs_with_recent().into_iter().find(|j| j.id == old).unwrap().state;
    assert!(matches!(old_state, "cancelled" | "done"), "old request is retired");
    let reduced = h.state().state.pending_preview().unwrap();
    pointer(&mut h, egui::pos2(rect.right() - 3., rect.center().y), Some(false));
    assert_ne!(h.state().state.pending_preview(), Some(reduced));
    finish_preview(&mut h);
    assert_eq!(h.state().state.preview.as_ref().unwrap().scale, 1.);
    assert!(h.state().session.journal.is_empty());
}
#[test]
fn trace28_ui_numeric_typing_and_palette_swatches_merge_remove() {
    let mut h = harness();
    open(&mut h);
    // The slider's number is an editable DragValue; real keyboard text changes it.
    let number = h.get_by_role_and_label(egui::accesskit::Role::SpinButton, "Noise");
    number.click();
    h.run_steps(2);
    h.key_press_modifiers(Modifiers::COMMAND, Key::A);
    h.run_steps(2);
    h.event(Event::Text("7".into()));
    h.step();
    h.key_press(Key::Enter);
    h.run_steps(3);
    assert_eq!(h.state().state.params.speckle, 7);
    finish_preview(&mut h);
    h.get_by_label("#FF0000").click();
    h.run_steps(3);
    h.get_by_label("Merge colour").click();
    h.run_steps(3);
    finish_preview(&mut h);
    assert_eq!(h.state().state.preview.as_ref().unwrap().trace.layers.len(), 1);
    h.get_by_label("Reset palette").click();
    h.run_steps(3);
    finish_preview(&mut h);
    h.get_by_label("#FF0000").click();
    h.run_steps(3);
    h.get_by_label("Remove colour").click();
    h.run_steps(3);
    finish_preview(&mut h);
    assert_eq!(h.state().state.preview.as_ref().unwrap().trace.layers.len(), 1);
    h.get_by_label("Reset palette").click();
    h.run_steps(3);
    finish_preview(&mut h);
    h.get_by_label("Use document swatches").click();
    h.run_steps(3);
    finish_preview(&mut h);
    assert_eq!(h.state().state.params.palette, vec![[0, 255, 0], [255, 0, 255]]);
    assert!(
        h.state().state.preview.as_ref().unwrap().trace.layers.iter().all(|l| [[0, 255, 0], [255, 0, 255]].contains(&[l.color[0], l.color[1], l.color[2]]))
    );
}
#[test]
fn trace28_ui_preview_view_click_zoom_pan_split_independent_cancel() {
    let mut h = harness();
    let before = h.state().session.active().unwrap().doc.clone();
    open(&mut h);
    h.get_by_label("Outlines").click();
    h.run_steps(3);
    assert_eq!(h.state().state.view, View::Outlines, "button {:?}, viewport {:?}", h.get_by_label("Outlines").rect(), h.state().state.preview_rect);
    let rect = h.state().state.preview_rect;
    let from = rect.center();
    pointer(&mut h, from, Some(true));
    pointer(&mut h, from + egui::vec2(30., 15.), None);
    pointer(&mut h, from + egui::vec2(30., 15.), Some(false));
    assert!(h.state().state.pan.length() > 20.);
    h.event(Event::MouseWheel { unit: egui::MouseWheelUnit::Point, phase: egui::TouchPhase::Move, delta: egui::vec2(0., 100.), modifiers: Modifiers::NONE });
    h.run_steps(8);
    assert!(h.state().state.zoom > 1.);
    h.get_by_label("Overlay").click();
    h.run_steps(3);
    assert_eq!(h.state().state.view, View::Overlay);
    h.get_by_label("Split").click();
    h.run_steps(3);
    let rect = h.state().state.preview_rect;
    pointer(&mut h, rect.center(), Some(true));
    pointer(&mut h, egui::pos2(rect.left() + rect.width() * 0.75, rect.center().y), None);
    pointer(&mut h, egui::pos2(rect.left() + rect.width() * 0.75, rect.center().y), Some(false));
    assert!((h.state().state.split - 0.75).abs() < 0.02);
    assert_eq!(h.state().session.active().unwrap().doc, before);
    h.key_press(Key::Escape);
    h.run_steps(3);
    assert!(!h.state().state.open);
    assert!(h.state().session.journal.is_empty());
}
#[test]
fn trace28_ui_apply_two_colours_expand_and_one_undo() {
    let mut h = harness();
    let before = h.state().session.active().unwrap().doc.clone();
    open(&mut h);
    h.get_by_label("Expand to shape layers").click();
    h.run_steps(3);
    h.get_by_role_and_label(egui::accesskit::Role::Button, "Trace").click();
    h.run_steps(3);
    if let Some(id) = h.state().state.pending_apply() {
        h.state_mut().session.wait_job(id).unwrap();
    }
    h.run_steps(3);
    assert!(
        !h.state().state.open,
        "apply {:?}, preview {:?}, error {:?}, button {:?}",
        h.state().state.pending_apply(),
        h.state().state.pending_preview(),
        h.state().state.error,
        h.get_by_role_and_label(egui::accesskit::Role::Button, "Trace").rect()
    );
    let doc = &h.state().session.active().unwrap().doc;
    assert_eq!(doc.layers.len(), 2);
    assert_eq!(doc.layers[1].children().unwrap().len(), 2);
    assert_eq!(h.state().session.journal.len(), 1);
    assert!(h.state_mut().session.undo());
    assert_eq!(h.state().session.active().unwrap().doc, before);
}
#[test]
fn trace28_ui_black_white_work_path_click_and_stale_edit_blocks_apply() {
    let mut h = harness();
    open(&mut h);
    h.get_by_role_and_label(egui::accesskit::Role::ComboBox, "Preset").click();
    h.run_steps(3);
    h.get_by_label("Black & White").click();
    h.run_steps(3);
    finish_preview(&mut h);
    h.get_by_label("Make Work Path").click();
    h.run_steps(3);
    h.get_by_role_and_label(egui::accesskit::Role::Button, "Trace").click();
    h.run_steps(3);
    if let Some(id) = h.state().state.pending_apply() {
        h.state_mut().session.wait_job(id).unwrap();
    }
    h.run_steps(3);
    assert!(h.state().session.active().unwrap().doc.work_path.is_some());
    assert_eq!(h.state().session.active().unwrap().doc.paths.len(), 1);
    assert!(h.state_mut().session.undo());
    h.get_by_label("Open Vectorize").click();
    h.run_steps(3);
    h.state_mut().session.execute("layer.setProps", json!({"name":"Changed while tracing"})).unwrap();
    h.run_steps(3);
    assert!(h.state().state.preview.is_none());
    assert!(h.get_by_role_and_label(egui::accesskit::Role::Button, "Trace").accesskit_node().is_disabled());
    assert!(h.state().state.error.as_ref().unwrap().contains("changed"));
}
