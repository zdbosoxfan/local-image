#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use super::*;
use egui_kittest::{
    Harness,
    kittest::{NodeT, Queryable},
};
use photocraft_doc::{Color, ColorMode, Fill, Layer, SampleType, ShapeLayer, Size};
use photocraft_engine::vector_packages::mesh::MeshData;
use photocraft_vector::mesh_paint::{Appearance, Mesh};
fn read(_: &Layer) -> Result<Option<MeshData>> {
    Ok(Some(Mesh::regular([10., 10., 110., 110.], 2, 2, Appearance::Flat, 0., |_| [0.2, 0.4, 0.8, 1.]).unwrap().into()))
}
fn hooks() -> Hooks {
    Hooks { read, ..engine::NATIVE }
}
struct Host {
    session: Session,
    state: State,
    ids: Vec<LayerId>,
    active: bool,
    origin: Pos2,
    actions: Vec<Action>,
    opacity: Rect,
}
pub(super) fn fixture() -> (Session, LayerId) {
    let mut doc = Document::new("Mesh UI", Size::new(120, 120), ColorMode::Rgb, SampleType::F32);
    let mut shape =
        ShapeLayer { path: photocraft_vector::shapes::rect(10., 10., 100., 100.), fill: Some(Fill::Solid(Color::rgb(0.2, 0.4, 0.8))), ..Default::default() };
    shape.cache = Some(photocraft_vector::render_shape(&shape, doc.pixel_format(), doc.bounds()));
    let layer = Layer::new("Mesh", LayerContent::Shape(shape));
    let id = layer.id;
    doc.layers.push(layer);
    let mut s = Session::new();
    s.add_document(doc, None);
    (s, id)
}
fn harness() -> Harness<'static, Host> {
    let (session, id) = fixture();
    Harness::builder().with_size(egui::vec2(600., 800.)).build_ui_state(
        |ui, h| {
            if ui.input(|i| i.key_pressed(Key::U)) {
                h.active = true;
            }
            h.state.lifecycle(&h.session, &h.ids, h.active, ui.input(|i| i.focused), ui.input(|i| i.key_pressed(Key::Escape)));
            ui.label("Mesh tool replay");
            let (rect, _) = ui.allocate_exact_size(egui::vec2(120., 120.), egui::Sense::click_and_drag());
            h.origin = rect.min;
            if h.active {
                let (events, mods) = ui.input(|i| (i.events.clone(), i.modifiers));
                let actions = h
                    .state
                    .events(&h.session, &h.ids, &events, rect, |p| [f64::from(p.x - rect.min.x), f64::from(p.y - rect.min.y)], mods, 3., hooks())
                    .unwrap();
                h.actions.extend(actions);
            }
            h.opacity = Rect::from_min_size(ui.next_widget_position(), egui::vec2(250., 50.));
            h.actions.extend(h.state.point_controls_with(ui, &h.session, &[[1., 0., 0., 1.]], hooks()).unwrap());
        },
        Host {
            session,
            state: State { color: [1., 0., 0., 1.], ..Default::default() },
            ids: vec![id],
            active: false,
            origin: Pos2::ZERO,
            actions: Vec::new(),
            opacity: Rect::NOTHING,
        },
    )
}
fn pointer(h: &mut Harness<'_, Host>, at: [f32; 2], pressed: Option<bool>, modifiers: Modifiers) {
    let pos = h.state().origin + egui::vec2(at[0], at[1]);
    h.event(Event::ModifiersChanged(modifiers));
    h.event(Event::PointerMoved(pos));
    if let Some(pressed) = pressed {
        h.event(Event::PointerButton { pos, button: PointerButton::Primary, pressed, modifiers });
    }
    h.step();
}
fn activate(h: &mut Harness<'_, Host>) {
    h.key_press(Key::U);
    h.run_steps(2);
    assert!(h.state().active);
}
#[test]
fn mesh43_kittest_u_click_adds_row_and_column_shift_retains_colour() {
    for shift in [false, true] {
        let mut h = harness();
        activate(&mut h);
        let mods = Modifiers { shift, ..Default::default() };
        pointer(&mut h, [35., 35.], Some(true), mods);
        let request = h.state().state.pending().unwrap();
        assert_eq!((request.edits[0].mesh.rows, request.edits[0].mesh.cols), (3, 3));
        let i = h.state().state.selected.unwrap().1;
        let color = request.edits[0].mesh.points[i].rgba;
        if shift {
            assert!((color[0] - 0.2).abs() < 1e-6);
        } else {
            assert_eq!(color, [1., 0., 0., 1.]);
        }
        assert!(h.state().session.journal.is_empty());
        pointer(&mut h, [35., 35.], Some(false), mods);
        assert_eq!(h.state().actions.iter().filter(|a| matches!(a, Action::Commit(_))).count(), 1);
        assert!(!h.state().state.captured());
    }
}
#[test]
fn mesh43_kittest_point_handle_drag_alt_delete_and_outside_release() {
    let mut h = harness();
    activate(&mut h);
    pointer(&mut h, [60., 60.], Some(true), Modifiers::NONE);
    pointer(&mut h, [72., 69.], None, Modifiers::NONE);
    let p = &h.state().state.pending().unwrap().edits[0].mesh.points[4];
    assert_eq!(p.position, [72., 69.]);
    assert!(h.state().session.journal.is_empty());
    pointer(&mut h, [160., 140.], Some(false), Modifiers::NONE);
    assert_eq!(h.state().actions.iter().filter(|a| matches!(a, Action::Commit(_))).count(), 1);
    let mut h = harness();
    activate(&mut h);
    pointer(&mut h, [60. + 50. / 3., 60.], Some(true), Modifiers::NONE);
    pointer(&mut h, [90., 65.], None, Modifiers::NONE);
    assert!(distance(h.state().state.pending().unwrap().edits[0].mesh.points[4].handles[0], [30., 5.]) < 1e-4);
    pointer(&mut h, [90., 65.], Some(false), Modifiers::NONE);
    let mut h = harness();
    activate(&mut h);
    pointer(&mut h, [60., 60.], Some(true), Modifiers { alt: true, ..Default::default() });
    assert_eq!((h.state().state.pending().unwrap().edits[0].mesh.rows, h.state().state.pending().unwrap().edits[0].mesh.cols), (1, 1));
    pointer(&mut h, [60., 60.], Some(false), Modifiers { alt: true, ..Default::default() });
    assert_eq!(h.state().actions.iter().filter(|a| matches!(a, Action::Commit(_))).count(), 1);
}
#[test]
fn mesh43_kittest_escape_focus_loss_and_revision_change_cancel() {
    for cause in 0..3 {
        let mut h = harness();
        activate(&mut h);
        pointer(&mut h, [60., 60.], Some(true), Modifiers::NONE);
        pointer(&mut h, [72., 69.], None, Modifiers::NONE);
        match cause {
            0 => h.key_press(Key::Escape),
            1 => h.event(Event::WindowFocused(false)),
            _ => {
                h.state_mut()
                    .session
                    .edit("External edit", |d, _| {
                        d.name = "changed".into();
                        Ok(())
                    })
                    .unwrap();
            }
        }
        h.run_steps(2);
        assert!(!h.state().state.captured());
        assert!(h.state().actions.iter().all(|a| !matches!(a, Action::Commit(_))));
    }
}
#[test]
fn mesh43_kittest_swatch_click_and_opacity_drag_emit_point_colour_edits() {
    let mut h = harness();
    activate(&mut h);
    pointer(&mut h, [60., 60.], Some(true), Modifiers::NONE);
    pointer(&mut h, [60., 60.], Some(false), Modifiers::NONE);
    h.state_mut().actions.clear();
    h.get_by_label("Apply mesh swatch").click();
    h.run_steps(2);
    let action = h
        .state()
        .actions
        .iter()
        .find_map(|a| match a {
            Action::Commit(r) => Some(r),
            _ => None,
        })
        .unwrap();
    assert_eq!(action.edits[0].mesh.points[4].rgba, [1., 0., 0., 1.]);
    assert_eq!(action.edits[0].mesh.points[4].opacity, 1.);
    // Egui accessibility identifies the actual slider; drive its pointer input, not the model.
    let slider = h.get_by_role(egui::accesskit::Role::Slider);
    let rect = slider.rect();
    let start = egui::pos2(rect.min.x + rect.width() * 0.8, rect.center().y);
    let end = egui::pos2(rect.min.x + rect.width() * 0.3, rect.center().y);
    h.event(Event::PointerMoved(start));
    h.event(Event::PointerButton { pos: start, button: PointerButton::Primary, pressed: true, modifiers: Modifiers::NONE });
    h.step();
    h.event(Event::PointerMoved(end));
    h.step();
    assert!(h.state().actions.iter().any(|a| matches!(a, Action::Preview(_))));
    assert!(h.state().session.journal.is_empty());
    h.event(Event::PointerButton { pos: end, button: PointerButton::Primary, pressed: false, modifiers: Modifiers::NONE });
    h.step();
    let r = h
        .state()
        .actions
        .iter()
        .rev()
        .find_map(|a| match a {
            Action::Commit(r) => Some(r),
            _ => None,
        })
        .unwrap();
    assert!(r.edits[0].mesh.points[4].opacity < 0.6);
}
#[test]
fn mesh43_kittest_colour_picker_channel_drag_previews_and_releases_once() {
    let mut h = harness();
    activate(&mut h);
    pointer(&mut h, [60., 60.], Some(true), Modifiers::NONE);
    pointer(&mut h, [60., 60.], Some(false), Modifiers::NONE);
    h.get_by_role(egui::accesskit::Role::ColorWell).click();
    h.run_steps(2);
    let channel = h
        .query_all_by_role(egui::accesskit::Role::SpinButton)
        .find(|n| n.accesskit_node().numeric_value() == Some(51.))
        .expect("actual red colour-picker channel")
        .rect();
    h.state_mut().actions.clear();
    let start = channel.center();
    let end = start + egui::vec2(40., 0.);
    h.event(Event::PointerMoved(start));
    h.event(Event::PointerButton { pos: start, button: PointerButton::Primary, pressed: true, modifiers: Modifiers::NONE });
    h.step();
    h.event(Event::PointerMoved(end));
    h.step();
    assert!(h.state().actions.iter().any(|a| matches!(a, Action::Preview(_))));
    assert!(h.state().session.journal.is_empty());
    h.event(Event::PointerButton { pos: end, button: PointerButton::Primary, pressed: false, modifiers: Modifiers::NONE });
    h.step();
    let commits = h
        .state()
        .actions
        .iter()
        .filter_map(|a| match a {
            Action::Commit(r) => Some(r),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(commits.len(), 1);
    assert!((commits[0].edits[0].mesh.points[4].rgba[0] - 0.2).abs() > 0.01);
    assert_eq!(h.state().state.color, commits[0].edits[0].mesh.points[4].rgba);
}
#[path = "gpu_tests.rs"]
mod gpu_tests;
