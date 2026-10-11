//! Package 04 isolated interaction host. Full-app acceptance is supplied separately for 00.
macro_rules! tl {
    ($s:expr) => {
        photocraft_ui_egui::i18n::t($s)
    };
}
#[path = "../../src/vector_packages/pen.rs"]
#[deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod pen_ui;

use egui::{Event, Modifiers, PointerButton, Pos2};
use egui_kittest::{Harness, kittest::Queryable};
use photocraft_doc::{Color, Fill, LayerContent, Path};
use photocraft_engine::{Session, vector_packages::pen::Mode};
use photocraft_geom::Point;
use serde_json::json;

struct Host {
    s: Session,
    state: pen_ui::State,
    rect: egui::Rect,
    enabled: bool,
    texture: Option<egui::TextureHandle>,
    error: Option<String>,
}
fn host() -> Host {
    let mut s = Session::new();
    s.execute("file.new", json!({"width":128,"height":128,"background":"transparent"})).unwrap();
    Host { s, state: Default::default(), rect: egui::Rect::NOTHING, enabled: true, texture: None, error: None }
}
fn draw(ui: &mut egui::Ui, h: &mut Host) {
    pen_ui::context(ui, &mut h.state);
    let (rect, _) = ui.allocate_exact_size(egui::vec2(300., 200.), egui::Sense::click_and_drag());
    h.rect = rect;
    let to_doc = |p: Pos2| Point::new(f64::from(p.x - rect.min.x), f64::from(p.y - rect.min.y));
    if let Err(e) = pen_ui::input(&mut h.state, &mut h.s, ui.ctx(), rect, to_doc, h.enabled, true, false, 4., Some(Fill::Solid(Color::BLACK)), None) {
        h.error = Some(e.to_string());
    }
    let doc = h.state.preview.as_ref().unwrap_or(&h.s.active().unwrap().doc);
    let frame = photocraft_compose::flatten(doc);
    let rgba: Vec<_> = frame
        .px
        .iter()
        .map(|p| egui::Color32::from_rgba_unmultiplied((p[0] * 255.) as u8, (p[1] * 255.) as u8, (p[2] * 255.) as u8, (p[3] * 255.) as u8))
        .collect();
    let image = egui::ColorImage::new([128, 128], rgba);
    if let Some(tex) = &mut h.texture {
        tex.set(image, egui::TextureOptions::NEAREST)
    } else {
        h.texture = Some(ui.ctx().load_texture("pen", image, egui::TextureOptions::NEAREST));
    }
    if let Some(tex) = &h.texture {
        ui.painter().image(
            tex.id(),
            egui::Rect::from_min_size(rect.min, egui::vec2(128., 128.)),
            egui::Rect::from_min_max(egui::pos2(0., 0.), egui::pos2(1., 1.)),
            egui::Color32::WHITE,
        );
    }
    pen_ui::paint_overlay(&h.state, ui.painter(), |p| rect.min + egui::vec2(p.x as f32, p.y as f32));
}
fn harness() -> Harness<'static, Host> {
    Harness::builder().with_size(egui::vec2(700., 320.)).build_ui_state(draw, host())
}
fn at(h: &Harness<'_, Host>, x: f32, y: f32) -> Pos2 {
    h.state().rect.min + egui::vec2(x, y)
}
fn button(h: &mut Harness<'_, Host>, x: f32, y: f32, pressed: bool, m: Modifiers) {
    h.event(Event::ModifiersChanged(m));
    h.event(Event::PointerButton { pos: at(h, x, y), button: PointerButton::Primary, pressed, modifiers: m });
    h.step();
}
fn click(h: &mut Harness<'_, Host>, x: f32, y: f32) {
    button(h, x, y, true, Modifiers::NONE);
    button(h, x, y, false, Modifiers::NONE);
}
fn motion(h: &mut Harness<'_, Host>, x: f32, y: f32, m: Modifiers) {
    // event_modifiers releases modifiers immediately after the event. A pen drag
    // needs the actual held state across frames, including stationary changes.
    h.event(Event::ModifiersChanged(m));
    h.event(Event::PointerMoved(at(h, x, y)));
    h.step();
}
fn path<'a>(h: &'a Harness<'_, Host>) -> &'a Path {
    let st = h.state().s.active().unwrap();
    let LayerContent::Shape(sh) = &st.doc.layer(st.active_layer.unwrap()).unwrap().content else { panic!("shape") };
    &sh.path
}

#[test]
fn pen_ui_click_click_drag_close_and_undo() {
    let mut h = harness();
    let before = h.state().s.active().unwrap().doc.clone();
    let history = h.state().s.active().unwrap().history.entries().len();
    let journal = h.state().s.journal.len();
    click(&mut h, 20., 20.);
    click(&mut h, 100., 20.);
    button(&mut h, 80., 90., true, Modifiers::NONE);
    motion(&mut h, 50., 90., Modifiers::NONE);
    button(&mut h, 50., 90., false, Modifiers::NONE);
    assert_eq!(h.state().s.active().unwrap().doc, before);
    assert_eq!(h.state().s.journal.len(), journal);
    click(&mut h, 20., 20.);
    let sp = &path(&h).subpaths[0];
    assert!(sp.closed);
    assert_eq!(sp.knots.len(), 3);
    assert_eq!(sp.knots.iter().filter(|k| k.smooth).count(), 1);
    assert_eq!(h.state().s.active().unwrap().history.entries().len(), history + 1);
    assert_eq!(h.state().s.journal.len(), journal + 1);
    assert!(h.state().state.gesture.is_none());
    assert!(h.state_mut().s.undo());
    assert_eq!(h.state().s.active().unwrap().doc, before);
}

#[test]
fn pen_ui_alt_shift_rubber_band_enter_and_capture_outside_canvas() {
    let mut h = harness();
    click(&mut h, 20., 20.);
    button(&mut h, 60., 60., true, Modifiers::NONE);
    motion(&mut h, 70., 60., Modifiers::NONE);
    let alt = Modifiers { alt: true, ..Modifiers::NONE };
    motion(&mut h, 80., 80., alt);
    let shift = Modifiers { alt: true, shift: true, ..Modifiers::NONE };
    motion(&mut h, 90., 68., shift);
    let k = h.state().state.gesture.as_ref().unwrap().path().subpaths[0].knots[1];
    assert_eq!(k.in_ctrl, Point::new(50., 60.));
    assert_eq!(k.out_ctrl.y, 60.);
    assert!(!k.smooth);
    // Real release outside the canvas must release capture without adding a node.
    button(&mut h, 350., 68., false, shift);
    assert!(!h.state().state.gesture.as_ref().unwrap().captured());
    motion(&mut h, 110., 100., Modifiers::NONE);
    let g = h.state().state.gesture.as_ref().unwrap();
    assert!(g.rubber_band(Default::default()).is_some());
    assert_eq!(g.path().subpaths[0].knots.len(), 2);
    h.key_press(egui::Key::Enter);
    h.step();
    assert_eq!(path(&h).subpaths[0].knots.len(), 2);
    assert!(!path(&h).subpaths[0].closed);
}

#[test]
fn pen_ui_escape_focus_persona_document_revision_and_mode_switch_cancel() {
    for cancel in 0..6 {
        let mut h = harness();
        let before = h.state().s.active().unwrap().doc.clone();
        let journal = h.state().s.journal.len();
        click(&mut h, 20., 20.);
        click(&mut h, 80., 20.);
        match cancel {
            0 => h.key_press(egui::Key::Escape),
            1 => h.event(Event::WindowFocused(false)),
            2 => h.state_mut().enabled = false,
            3 => {
                h.state_mut().s.active_mut().unwrap().revision += 1;
            }
            4 => h.get_by_label("Polygon").click(),
            _ => {
                h.state_mut().s.execute("file.new", json!({"width":128,"height":128})).unwrap();
            }
        }
        h.step();
        if cancel == 0 {
            assert!(h.state().state.gesture.is_none());
            assert!(h.state().state.preview.is_none());
            assert_eq!(path(&h).subpaths[0].knots.len(), 2);
            assert!(!path(&h).subpaths[0].closed);
            assert_eq!(h.state().s.journal.len(), journal + 1);
            assert!(h.state_mut().s.undo());
            assert_eq!(h.state().s.active().unwrap().doc, before);
        } else if cancel == 1 {
            let gesture = h.state().state.gesture.as_ref().unwrap();
            assert_eq!(gesture.path().subpaths[0].knots.len(), 2);
            assert!(!gesture.captured());
            assert!(h.state().state.preview.is_some());
            assert_eq!(h.state().s.active().unwrap().doc, before);
            assert_eq!(h.state().s.journal.len(), journal);
        } else {
            assert!(h.state().state.gesture.is_none());
            assert!(h.state().state.preview.is_none());
            if cancel != 5 {
                assert_eq!(h.state().s.active().unwrap().doc, before);
            }
        }
    }
}

#[test]
fn pen_ui_context_modes_and_selected_curve_addition_continuation() {
    let mut h = harness();
    h.get_by_label("Line").click();
    h.step();
    assert_eq!(h.state().state.options.mode, Mode::Line);
    click(&mut h, 20., 20.);
    click(&mut h, 80., 20.);
    assert!(h.state().state.gesture.is_none());
    assert_eq!(path(&h).subpaths[0].knots.len(), 2);
    h.get_by_label("Pen").click();
    h.step();
    let existing = path(&h).clone();
    click(&mut h, 80., 20.);
    click(&mut h, 100., 60.);
    h.key_press(egui::Key::Enter);
    h.step();
    assert_eq!(&path(&h).subpaths[0].knots[..2], &existing.subpaths[0].knots);
    h.get_by_label("Add to selected curve").click();
    h.step();
    click(&mut h, 20., 80.);
    click(&mut h, 70., 80.);
    h.key_press(egui::Key::Enter);
    h.step();
    assert_eq!(path(&h).subpaths.len(), 2);
    h.get_by_label("Smart").click();
    h.step();
    assert_eq!(h.state().state.options.mode, Mode::Smart);
    click(&mut h, 40., 100.);
    click(&mut h, 70., 110.);
    h.key_press(egui::Key::Enter);
    h.step();
    assert!(path(&h).subpaths[2].knots.iter().all(|k| k.smooth));
}

#[test]
fn pen_ui_old_options_load_with_defaults() {
    let options: pen_ui::Options = serde_json::from_str("{}").unwrap();
    assert_eq!(options, Default::default());
    let options = pen_ui::Options { mode: Mode::Smart, rubber_band: false, add_to_selected: true };
    assert_eq!(serde_json::from_value::<pen_ui::Options>(serde_json::to_value(&options).unwrap()).unwrap(), options);
}

#[test]
fn pen_ui_add_to_selected_requires_one_curve_and_leaves_raster_untouched() {
    let mut h = harness();
    h.state_mut().s.execute("layer.new.layer", json!({})).unwrap();
    h.step();
    let before = h.state().s.active().unwrap().doc.clone();
    let journal = h.state().s.journal.len();
    h.get_by_label("Add to selected curve").click();
    h.step();
    click(&mut h, 20., 20.);
    assert!(h.state().error.as_ref().is_some_and(|e| e.contains("select one curve")));
    assert!(h.state().state.gesture.is_none());
    assert_eq!(h.state().s.active().unwrap().doc, before);
    assert_eq!(h.state().s.journal.len(), journal);
}

#[test]
fn pen_ui_line_drag_commits_one_straight_segment_on_release() {
    let mut h = harness();
    h.get_by_label("Line").click();
    h.step();
    let history = h.state().s.active().unwrap().history.entries().len();
    button(&mut h, 20., 20., true, Modifiers::NONE);
    let shift = Modifiers { shift: true, ..Modifiers::NONE };
    motion(&mut h, 70., 31., shift);
    assert!(h.state().state.gesture.is_some());
    button(&mut h, 70., 31., false, shift);
    assert!(h.state().state.gesture.is_none());
    let sp = &path(&h).subpaths[0];
    assert_eq!(sp.knots.len(), 2);
    assert_eq!(sp.knots[1].anchor.y, 20.);
    assert!(sp.knots.iter().all(|k| !k.smooth));
    assert_eq!(h.state().s.active().unwrap().history.entries().len(), history + 1);
}

mod gpu;
