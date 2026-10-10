#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use super::*;
use egui_kittest::{
    Harness,
    kittest::{NodeT, Queryable},
};
use photocraft_doc::{Color, ColorMode, Document, Fill, Layer, LayerContent, Path, SampleType, ShapeLayer, Size, Subpath};
use serde_json::json;
struct Host {
    session: Session,
    state: State,
    direct: DirectSelection,
    tool: Tool,
    node: bool,
    scope: Scope,
    origin: Pos2,
    show_menu: bool,
    show_options: bool,
}
fn shape(name: &str, x: f64, c: Color) -> Layer {
    Layer::new(
        name,
        LayerContent::Shape(ShapeLayer {
            path: Path::new(vec![Subpath::polygon(&[(x, 20.), (x + 10., 20.), (x + 10., 30.), (x, 30.)])]),
            fill: Some(Fill::Solid(c)),
            ..Default::default()
        }),
    )
}
fn host(layers: Vec<Layer>, tool: Tool) -> Host {
    let mut d = Document::new("UI selection", Size::new(240, 120), ColorMode::Rgb, SampleType::U8);
    d.layers = layers;
    let mut session = Session::new();
    session.add_document(d, None);
    Host {
        session,
        state: State::default(),
        direct: Default::default(),
        tool,
        node: false,
        scope: Default::default(),
        origin: Pos2::ZERO,
        show_menu: false,
        show_options: false,
    }
}
fn harness(host: Host) -> Harness<'static, Host> {
    Harness::builder().with_size(egui::vec2(650., 650.)).build_ui_state(
        |ui, h| {
            if h.show_options {
                wand_options(ui, &mut h.state.settings, str::to_owned);
            }
            if h.show_menu {
                ui.menu_button("Select", |ui| {
                    if menu(ui, &mut h.session, &mut h.state, h.scope.clone(), str::to_owned) {
                        clear_nodes(&mut h.direct, h.session.active().unwrap().doc.id);
                    }
                });
            }
            let rect = ui.allocate_space(egui::vec2(250., 130.)).1;
            h.origin = rect.min;
            canvas(
                ui,
                &mut h.session,
                &mut h.state,
                &mut h.direct,
                rect,
                View { origin: rect.min, document_origin: [0., 0.], zoom: 1., flip: false },
                h.tool,
                h.node,
                h.scope.clone(),
                Readers::default(),
                false,
            );
            // This fixture host reads the same session flag the production Layers panel reads.
            for (_, _, l) in h.session.active().unwrap().doc.walk() {
                let _ = ui.selectable_label(h.session.active().unwrap().is_layer_selected(l.id), &l.name);
            }
            if ui.input(|i| i.key_pressed(egui::Key::Delete)) && !h.direct.anchors.is_empty() {
                let st = h.session.active().unwrap();
                let targets: Vec<_> = node_base(&h.direct)
                    .into_iter()
                    .map(|n| photocraft_engine::vector_packages::node::Target { name: "layer".into(), layer: Some(n.layer.0), anchors: n.anchors })
                    .collect();
                h.session
                    .execute("path.node.edit", json!({"document":st.doc.id.0,"revision":st.revision,"targets":targets,"operation":{"type":"delete"}}))
                    .unwrap();
            }
        },
        host,
    )
}
fn pointer(h: &mut Harness<'_, Host>, p: [f32; 2], press: Option<bool>, mods: Modifiers) {
    let pos = h.state().origin + egui::vec2(p[0], p[1]);
    h.event(Event::ModifiersChanged(mods));
    h.event(Event::PointerMoved(pos));
    if let Some(pressed) = press {
        h.event(Event::PointerButton { pos, pressed, button: PointerButton::Primary, modifiers: mods });
    }
    h.step();
}
fn click(h: &mut Harness<'_, Host>, p: [f32; 2], mods: Modifiers) {
    pointer(h, p, Some(true), mods);
    pointer(h, p, Some(false), mods);
}
fn loop_drag(h: &mut Harness<'_, Host>, points: &[[f32; 2]], mods: Modifiers) {
    pointer(h, points[0], Some(true), mods);
    for p in &points[1..] {
        pointer(h, *p, None, mods);
    }
    pointer(h, points[0], Some(false), mods);
}
#[test]
fn selection_kittest_group_circle_twice_and_node_alt_temporary() {
    let mut circle = shape("Circle", 20., Color::BLACK);
    let LayerContent::Shape(sh) = &mut circle.content else { panic!() };
    sh.path = photocraft_vector::shapes::ellipse(20., 20., 10., 10.);
    let leaf = circle.id;
    let group = Layer::group("Group", vec![circle]);
    let parent = group.id;
    let other = shape("Other", 100., Color::BLACK);
    let other_id = other.id;
    let mut h = harness(host(vec![group, other], Tool::Group));
    let revision = h.state().session.active().unwrap().revision;
    click(&mut h, [25., 25.], Modifiers::NONE);
    assert_eq!(h.state().session.active().unwrap().selected_layers(), [leaf]);
    // Layers row's real selectable widget reflects the circle's selection.
    assert_eq!(h.get_by_label("Circle").accesskit_node().toggled(), Some(egui::accesskit::Toggled::True));
    assert_eq!(h.get_by_label("Group").accesskit_node().toggled(), Some(egui::accesskit::Toggled::False));
    click(&mut h, [25., 25.], Modifiers::NONE);
    assert_eq!(h.state().session.active().unwrap().selected_layers(), [parent]);
    assert_eq!(h.get_by_label("Group").accesskit_node().toggled(), Some(egui::accesskit::Toggled::True));
    assert_eq!(h.get_by_label("Circle").accesskit_node().toggled(), Some(egui::accesskit::Toggled::False));
    h.state_mut().tool = Tool::Node;
    h.state_mut().state.group = None;
    h.state_mut().direct.target = Some(PathRef::Layer(leaf.0));
    h.state_mut().direct.anchors = vec![[0, 0]];
    click(&mut h, [25., 25.], Modifiers { alt: true, ..Modifiers::NONE });
    assert_eq!(h.state().session.active().unwrap().selected_layers(), [leaf]);
    assert!(h.state().direct.target.is_none());
    assert!(h.state().direct.anchors.is_empty());
    h.state_mut().tool = Tool::Group;
    let st = h.state_mut().session.active_mut().unwrap();
    st.active_layer = Some(other_id);
    st.selected_layers = vec![other_id];
    click(&mut h, [25., 25.], Modifiers::SHIFT);
    assert_eq!(h.state().session.active().unwrap().selected_layers(), [leaf, other_id]);
    click(&mut h, [25., 25.], Modifiers::SHIFT);
    assert_eq!(h.state().session.active().unwrap().selected_layers(), [parent, leaf, other_id]);
    assert_eq!(h.state().state.group.unwrap().3, parent);
    assert_eq!(h.state().session.active().unwrap().revision, revision);
    assert!(h.state().session.journal.is_empty());
}
#[test]
fn selection_kittest_object_lasso_three_of_five_preview_release_shift_alt() {
    let layers: Vec<_> = (0..5).map(|i| shape("shape", i as f64 * 20., Color::BLACK)).collect();
    let ids: Vec<_> = layers.iter().map(|l| l.id).collect();
    let mut h = harness(host(layers, Tool::Lasso));
    let before = h.state().session.active().unwrap().doc.clone();
    pointer(&mut h, [-1., 19.], Some(true), Modifiers::NONE); // outside canvas cannot begin
    assert!(!h.state().state.captured());
    pointer(&mut h, [0., 19.], Some(true), Modifiers::NONE);
    pointer(&mut h, [51., 19.], None, Modifiers::NONE);
    pointer(&mut h, [51., 31.], None, Modifiers::NONE);
    pointer(&mut h, [0., 31.], None, Modifiers::NONE);
    assert_eq!(h.state().state.preview.as_ref().unwrap().layers, ids[..3]);
    assert_eq!(h.state().session.active().unwrap().doc, before);
    pointer(&mut h, [0., 19.], Some(false), Modifiers::NONE);
    assert_eq!(h.state().session.active().unwrap().selected_layers(), ids[..3]);
    loop_drag(&mut h, &[[59., 19.], [71., 19.], [71., 31.], [59., 31.]], Modifiers { shift: true, ..Modifiers::NONE });
    assert_eq!(h.state().session.active().unwrap().selected_layers(), ids[..4]);
    loop_drag(&mut h, &[[19., 19.], [31., 19.], [31., 31.], [19., 31.]], Modifiers { alt: true, ..Modifiers::NONE });
    assert_eq!(h.state().session.active().unwrap().selected_layers(), [ids[0], ids[2], ids[3]]);
    assert!(h.state().session.journal.is_empty());
}
#[test]
fn selection_kittest_node_lasso_two_of_four_then_delete_03() {
    let l = shape("Curve", 20., Color::BLACK);
    let id = l.id;
    let mut host = host(vec![l], Tool::Lasso);
    host.node = true;
    let mut h = harness(host);
    let before = h.state().session.active().unwrap().doc.clone();
    loop_drag(&mut h, &[[19., 19.], [31., 19.], [31., 25.], [19., 25.]], Modifiers::NONE);
    assert_eq!(h.state().direct.target, Some(PathRef::Layer(id.0)));
    assert_eq!(h.state().direct.anchors, [[0, 0], [0, 1]]);
    assert_eq!(h.state().session.active().unwrap().doc, before);
    h.key_press(egui::Key::Delete);
    h.step();
    let LayerContent::Shape(sh) = &h.state().session.active().unwrap().doc.layer(id).unwrap().content else { panic!() };
    assert_eq!(sh.path.subpaths[0].knots.len(), 2);
    assert_eq!(sh.path.subpaths[0].knots.iter().map(|k| (k.anchor.x, k.anchor.y)).collect::<Vec<_>>(), [(30., 30.), (20., 30.)]);
    assert_eq!(h.state().session.journal.len(), 1);
    assert!(h.state_mut().session.undo());
    assert_eq!(h.state().session.active().unwrap().doc, before);
}
#[test]
fn selection_kittest_wand_click_shift_add_alt_subtract_and_current_modifiers() {
    let a = shape("red", 20., Color::rgb(1., 0., 0.));
    let b = shape("blue", 50., Color::rgb(0., 0., 1.));
    let c = shape("red again", 80., Color::rgb(1., 0., 0.));
    let ids = [a.id, b.id, c.id];
    let mut locked = shape("locked existing selection", 110., Color::rgb(1., 0., 0.));
    locked.locks.all = true;
    let mut h = harness(host(vec![a, b, c, locked], Tool::Wand));
    h.state_mut().state.settings.wand.fill_tolerance = 0.;
    click(&mut h, [25., 25.], Modifiers::NONE);
    assert_eq!(h.state().session.active().unwrap().selected_layers(), [ids[0], ids[2]]);
    pointer(&mut h, [55., 25.], Some(true), Modifiers::NONE);
    pointer(&mut h, [55., 25.], Some(false), Modifiers { shift: true, ..Modifiers::NONE });
    assert_eq!(h.state().session.active().unwrap().selected_layers(), ids);
    click(&mut h, [25., 25.], Modifiers { alt: true, ..Modifiers::NONE });
    assert_eq!(h.state().session.active().unwrap().selected_layers(), [ids[1]]);
    assert!(h.state().session.journal.is_empty());
    assert_eq!(h.state().session.active().unwrap().history.past_len(), 0);
}
#[test]
fn selection_kittest_escape_focus_switch_and_staleness_cancel() {
    let l = shape("a", 20., Color::BLACK);
    let mut h = harness(host(vec![l], Tool::Lasso));
    let before = h.state().session.active().unwrap().doc.clone();
    for cause in 0..6 {
        pointer(&mut h, [10., 10.], Some(true), Modifiers::NONE);
        pointer(&mut h, [60., 10.], None, Modifiers::NONE);
        pointer(&mut h, [60., 60.], None, Modifiers::NONE);
        match cause {
            0 => {
                h.key_press(egui::Key::Escape);
            }
            1 => h.input_mut().focused = false,
            2 => h.state_mut().tool = Tool::Wand,
            3 => {
                h.state_mut().session.active_mut().unwrap().revision += 1;
            }
            4 => h.state_mut().scope.select_hidden = true,
            _ => {
                h.state_mut().session.active_mut().unwrap().active_layer = None;
                h.state_mut().session.active_mut().unwrap().selected_layers.clear();
            }
        }
        h.step();
        assert!(!h.state().state.captured());
        pointer(&mut h, [10., 10.], Some(false), Modifiers::NONE);
        h.input_mut().focused = true;
        h.state_mut().tool = Tool::Lasso;
    }
    assert_eq!(h.state().session.active().unwrap().doc, before);
    assert!(h.state().session.journal.is_empty());
}
#[test]
fn selection_kittest_menu_same_click_reselect_and_old_settings() {
    let a = shape("red", 20., Color::rgb(1., 0., 0.));
    let b = shape("red", 50., Color::rgb(1., 0., 0.));
    let ids = [a.id, b.id];
    let mut host = host(vec![a, b], Tool::Wand);
    host.show_menu = true;
    host.direct.target = Some(PathRef::Layer(ids[0].0));
    host.direct.anchors = vec![[0, 0]];
    let mut h = harness(host);
    h.get_by_label("Select").click();
    h.run_steps(2);
    h.get_by_label("Same ⏵").click();
    h.run_steps(2);
    h.get_by_label("Fill Color").click();
    h.step();
    assert_eq!(h.state().session.active().unwrap().selected_layers(), ids);
    assert!(h.state().direct.anchors.is_empty());
    h.get_by_label("Select").click();
    h.step();
    h.get_by_label("Deselect Vector Objects").click();
    h.step();
    assert!(h.state().session.active().unwrap().selected_layers().is_empty());
    h.get_by_label("Select").click();
    h.step();
    h.get_by_label("Reselect Vector Objects").click();
    h.step();
    assert_eq!(h.state().session.active().unwrap().selected_layers(), ids);
    assert_eq!(serde_json::from_str::<Settings>("{}").unwrap().wand, Wand::default());
}
#[test]
fn selection_kittest_wand_numeric_tolerance_type_and_clamp() {
    let l = shape("a", 20., Color::BLACK);
    let mut host = host(vec![l], Tool::Wand);
    host.show_options = true;
    let mut h = harness(host);
    for (typed, expected) in [("0.25", 0.25), ("999", 200.)] {
        h.get_by_label("Fill Tolerance ΔE").click();
        h.run_steps(2);
        h.key_press_modifiers(Modifiers::COMMAND, egui::Key::A);
        h.run_steps(2);
        h.get_by_label("Fill Tolerance ΔE").type_text(typed);
        h.key_press(egui::Key::Enter);
        h.run_steps(2);
        assert_eq!(h.state().state.settings.wand.fill_tolerance, expected);
    }
}
