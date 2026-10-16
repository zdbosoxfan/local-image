use super::*;
use egui::{Event, Modifiers, PointerButton, Pos2};
use egui_kittest::{Harness, kittest::Queryable};
use photocraft_doc::{Color, ColorMode, Document, Fill, Layer, LayerContent, Path, SampleType, ShapeLayer, Size, Subpath};
use photocraft_engine::vector_packages::snapping::query::{self, Exclusions, Kind, Settings, Source, View};

struct Host {
    doc: Document,
    revision: u64,
    state: State,
    manager: Manager,
    gesture: Gesture,
    screen: egui::Rect,
    actions: Vec<Action>,
    errors: Vec<String>,
    exclusions: Exclusions,
    zoom: f64,
}
fn text(key: &str) -> String {
    let t = include_str!("fixtures/i18n-07.tsv");
    t.lines().find_map(|line| line.split_once('\t').filter(|(k, _)| *k == key).map(|(_, v)| v.to_owned())).unwrap_or_else(|| key.to_owned())
}
fn harness(gesture: Gesture, ppp: f32, zoom: f64) -> Harness<'static, Host> {
    let mut doc = Document::new("UI snapping", Size::new(400, 300), ColorMode::Rgb, SampleType::U8);
    doc.guides.vertical.push(120.0);
    let manager_state = Manager {
        settings: query::Settings { nodes: false, bounds: false, alignment: false, document: false, artboard: false, spacing: false, ..Default::default() },
    };
    let host = Host {
        doc,
        revision: 0,
        state: State::default(),
        manager: manager_state,
        gesture,
        screen: egui::Rect::NOTHING,
        actions: Vec::new(),
        errors: Vec::new(),
        exclusions: Exclusions::default(),
        zoom,
    };
    let mut h = Harness::builder().with_size(egui::vec2(800.0, 600.0)).with_pixels_per_point(ppp).build_ui_state(
        |ui, host: &mut Host| {
            manager(ui, &mut host.manager, text);
            let (rect, response) = ui.allocate_exact_size(egui::vec2(700.0, 450.0), egui::Sense::click_and_drag());
            host.screen = rect;
            let input = Input {
                document: &host.doc,
                revision: host.revision,
                exclusions: host.exclusions.clone(),
                gesture: host.gesture,
                settings: &host.manager.settings,
                view: View { zoom: host.zoom, pixels_per_point: f64::from(ui.ctx().pixels_per_point()) },
                mapping: Mapping { origin: rect.min, document_origin: [0.0, 0.0], zoom: host.zoom, flip: false },
                angle: None,
                geometry: None,
                cancel: false,
            };
            match host.state.input(&response, input) {
                Ok(Some(action)) => host.actions.push(action),
                Ok(None) => {}
                Err(error) => host.errors.push(error.to_string()),
            }
            if let Some(p) = &host.state.preview {
                paint(ui.painter(), &p.snap.guides, |p| rect.min + egui::vec2((p[0] * host.zoom) as f32, (p[1] * host.zoom) as f32), |v| format!("{v:.2} px"));
            }
        },
        host,
    );
    h.run_steps(3);
    h
}
fn screen(h: &Harness<'_, Host>, p: [f64; 2]) -> Pos2 {
    h.state().screen.min + egui::vec2((p[0] * h.state().zoom) as f32, (p[1] * h.state().zoom) as f32)
}
fn pointer(h: &mut Harness<'_, Host>, p: [f64; 2]) {
    h.event(Event::PointerMoved(screen(h, p)));
    h.step();
}
fn button(h: &mut Harness<'_, Host>, p: [f64; 2], pressed: bool, mods: Modifiers) {
    h.event(Event::ModifiersChanged(mods));
    h.event(Event::PointerButton { pos: screen(h, p), button: PointerButton::Primary, pressed, modifiers: mods });
    h.step();
}
#[test]
fn snapping_ui_point_and_rectangle_drag_guide_at_hidpi() {
    for (ppp, zoom) in [(1.0, 1.0), (2.0, 2.0)] {
        let mut h = harness(Gesture::Point, ppp, zoom);
        button(&mut h, [40.0, 80.0], true, Modifiers::NONE);
        pointer(&mut h, [121.0, 90.0]);
        assert_eq!(h.state().state.preview.as_ref().unwrap().pointer, [120.0, 90.0]);
        button(&mut h, [121.0, 90.0], false, Modifiers::NONE);
        let Action::Commit(p) = h.state().actions.last().unwrap() else { panic!("commit") };
        assert_eq!(p.pointer, [120.0, 90.0]);
        assert!(!h.state().state.captured());
        let mut h = harness(Gesture::Rect([40.0, 40.0, 60.0, 60.0]), ppp, zoom);
        button(&mut h, [50.0, 50.0], true, Modifiers::NONE);
        pointer(&mut h, [131.0, 75.0]);
        assert_eq!(h.state().state.preview.as_ref().unwrap().geometry, Gesture::Rect([120.0, 65.0, 140.0, 85.0]));
        button(&mut h, [131.0, 75.0], false, Modifiers::NONE);
        let Action::Commit(p) = h.state().actions.last().unwrap() else { panic!("commit") };
        assert_eq!(p.geometry, Gesture::Rect([120.0, 65.0, 140.0, 85.0]));
    }
}
#[test]
fn snapping_ui_modifiers_toggle_during_capture_and_release() {
    for mods in [Modifiers::CTRL, Modifiers { command: true, mac_cmd: true, ..Default::default() }] {
        let mut h = harness(Gesture::Point, 1.0, 1.0);
        button(&mut h, [40.0, 80.0], true, mods); // press modifiers do not disable the captured gesture
        h.event(Event::ModifiersChanged(Modifiers::NONE));
        pointer(&mut h, [121.0, 90.0]);
        assert_eq!(h.state().state.preview.as_ref().unwrap().pointer, [120.0, 90.0]);
        h.event(Event::ModifiersChanged(mods));
        h.step();
        assert_eq!(h.state().state.preview.as_ref().unwrap().pointer, [121.0, 90.0]);
        assert!(h.state().state.preview.as_ref().unwrap().snap.guides.is_empty());
        h.event(Event::ModifiersChanged(Modifiers::NONE));
        h.step();
        assert_eq!(h.state().state.preview.as_ref().unwrap().pointer, [120.0, 90.0]);
        // Release with bypass uses the RAW endpoint, not the previously snapped preview.
        button(&mut h, [121.0, 90.0], false, mods);
        let Action::Commit(p) = h.state().actions.last().unwrap() else { panic!("commit") };
        assert_eq!(p.pointer, [121.0, 90.0]);
    }
}
#[test]
fn snapping_ui_manager_click_toggles_and_old_settings_load() {
    let mut h = harness(Gesture::Point, 1.0, 1.0);
    h.get_by_label("Snapping Manager").click();
    h.run_steps(3);
    h.get_by_label("Snap to guides").click();
    h.run_steps(3);
    assert!(!h.state().manager.settings.guides);
    h.get_by_label("Enable snapping").click();
    h.run_steps(3);
    assert!(!h.state().manager.settings.enabled);
    let json = serde_json::to_string(&h.state().manager).unwrap();
    let restored: Manager = serde_json::from_str(&json).unwrap();
    assert_eq!(restored.settings, h.state().manager.settings);
    let old: Manager = serde_json::from_str("{}").unwrap();
    assert_eq!(old.settings, Settings::default());
}
#[test]
fn snapping_ui_multi_object_spacing_and_target_exclusions() {
    let mut h = harness(Gesture::Rect([52.0, 32.0, 72.0, 58.0]), 1.0, 1.0);
    h.state_mut().manager.settings.guides = false;
    h.state_mut().manager.settings.spacing = true;
    for r in [[10.0, 30.0, 30.0, 60.0], [90.0, 30.0, 110.0, 60.0], [52.0, 32.0, 60.0, 58.0], [64.0, 32.0, 72.0, 58.0]] {
        let path = Path::new(vec![Subpath::polygon(&[(r[0], r[1]), (r[2], r[1]), (r[2], r[3]), (r[0], r[3])])]);
        let mut sh = ShapeLayer { path, fill: Some(Fill::Solid(Color::BLACK)), ..Default::default() };
        photocraft_engine::vector_cmds::refresh_shape(&h.state().doc, &mut sh);
        let layer = Layer::new("Box", LayerContent::Shape(sh));
        h.state_mut().doc.layers.push(layer);
    }
    let ids = h.state().doc.layers[2..].iter().map(|l| l.id).collect();
    h.state_mut().exclusions.layers = ids;
    button(&mut h, [62.0, 40.0], true, Modifiers::NONE);
    pointer(&mut h, [63.0, 40.0]);
    let p = h.state().state.preview.as_ref().unwrap();
    assert_eq!(p.geometry, Gesture::Rect([50.0, 32.0, 70.0, 58.0]));
    assert_eq!(p.snap.guides.len(), 2);
    assert!(p.snap.guides.iter().all(|g| g.hit.kind == Kind::Spacing && g.value == Some(20.0)));
    assert!(matches!(p.snap.guides[0].hit.source, Source::Layer(_)));
    button(&mut h, [63.0, 40.0], false, Modifiers::NONE);
}
#[test]
fn snapping_ui_cancellation_stale_document_focus_tool_escape() {
    for reason in 0..4 {
        let mut h = harness(Gesture::Point, 1.0, 1.0);
        button(&mut h, [40.0, 80.0], true, Modifiers::NONE);
        match reason {
            0 => {
                h.state_mut().revision += 1;
            }
            1 => {
                h.event(Event::WindowFocused(false));
            }
            2 => {
                h.state_mut().state.cancel();
            } // router's persona/tool cancellation hook
            _ => {
                h.event(Event::Key { key: egui::Key::Escape, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::NONE });
            }
        }
        h.step();
        assert!(!h.state().state.captured());
        assert!(h.state().state.preview.is_none());
        button(&mut h, [121.0, 90.0], false, Modifiers::NONE);
        assert!(!h.state().actions.iter().any(|a| matches!(a, Action::Commit(_))));
    }
}
#[test]
fn snapping_ui_capture_crosses_canvas_and_uses_press_mapping() {
    let mut h = harness(Gesture::Point, 1.0, 1.0);
    button(&mut h, [40.0, 80.0], true, Modifiers::NONE);
    h.state_mut().zoom = 2.0; // current view changed, frozen pointer origin still defines geometry
    h.event(Event::PointerMoved(h.state().screen.min + egui::vec2(740.0, 90.0)));
    h.step();
    assert_eq!(h.state().state.preview.as_ref().unwrap().pointer, [740.0, 90.0]);
    let p = h.state().screen.min + egui::vec2(740.0, 90.0);
    h.event(Event::PointerButton { pos: p, button: PointerButton::Primary, pressed: false, modifiers: Modifiers::NONE });
    h.step();
    let Action::Commit(p) = h.state().actions.last().unwrap() else { panic!("captured release") };
    assert_eq!(p.pointer, [740.0, 90.0]);
}

#[test]
fn snapping_ui_manager_numeric_tolerance_types_and_clamps() {
    let mut h = harness(Gesture::Point, 1.0, 1.0);
    h.get_by_label("Snapping Manager").click();
    h.run_steps(3);
    h.get_by_label("Screen tolerance (pixels)").click();
    h.run_steps(2);
    h.key_press_modifiers(Modifiers::COMMAND, egui::Key::A);
    h.run_steps(2);
    h.get_by_label("Screen tolerance (pixels)").type_text("4");
    h.key_press(egui::Key::Enter);
    h.run_steps(2);
    assert_eq!(h.state().manager.settings.tolerance_px, 4.0);
    h.get_by_label("Screen tolerance (pixels)").click();
    h.run_steps(2);
    h.key_press_modifiers(Modifiers::COMMAND, egui::Key::A);
    h.run_steps(2);
    h.get_by_label("Screen tolerance (pixels)").type_text("999");
    h.key_press(egui::Key::Enter);
    h.run_steps(2);
    assert_eq!(h.state().manager.settings.tolerance_px, 64.0);
}

#[test]
fn snapping_ui_commit_rejects_locked_descendant_and_missing_target() {
    for missing in [false, true] {
        let mut h = harness(Gesture::Rect([40.0, 40.0, 60.0, 60.0]), 1.0, 1.0);
        let mut child = Layer::raster("Child", h.state().doc.pixel_format());
        child.locks.position = !missing;
        let parent = Layer::group("Target group", vec![child]);
        let id = parent.id;
        h.state_mut().doc.layers.push(parent);
        h.state_mut().exclusions.layers = vec![id];
        button(&mut h, [50.0, 50.0], true, Modifiers::NONE);
        pointer(&mut h, [131.0, 75.0]);
        let preview = h.state().state.preview.clone().unwrap();
        assert_eq!(preview.document, h.state().doc.id);
        assert_eq!(preview.revision, 0);
        assert_eq!(preview.targets.layers, vec![id]);
        assert!(preview.preflight(&h.state().doc, 1).is_err(), "publication must recheck revision after cache preparation");
        if missing {
            h.state_mut().doc.layers.clear();
        }
        button(&mut h, [131.0, 75.0], false, Modifiers::NONE);
        assert!(!h.state().state.captured());
        assert!(h.state().state.preview.is_none());
        assert_eq!(h.state().errors.len(), 1);
        assert!(!h.state().actions.iter().any(|a| matches!(a, Action::Commit(_))));
    }
}

#[test]
fn snapping_ui_click_does_not_move_rectangle_near_guide() {
    let rect = [119.0, 40.0, 139.0, 60.0];
    let mut h = harness(Gesture::Rect(rect), 1.0, 1.0);
    button(&mut h, [129.0, 50.0], true, Modifiers::NONE);
    let preview = h.state().state.preview.as_ref().unwrap();
    assert_eq!(preview.geometry, Gesture::Rect(rect));
    assert!(preview.snap.guides.is_empty());
    button(&mut h, [129.0, 50.0], false, Modifiers::NONE);
    assert!(!h.state().state.captured());
    assert!(!h.state().actions.iter().any(|a| matches!(a, Action::Commit(_))));
}
