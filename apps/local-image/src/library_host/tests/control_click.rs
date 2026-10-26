use super::{Host, Module, RoundTripUi};
use egui::{Event, Modifiers, PointerButton};
use photocraft_ui_egui::{PhotocraftApp, canvas::ViewXform, state::Tool};
use serde_json::json;

#[test]
fn control_clicks_open_the_right_click_menus_without_editing() {
    for tool in [Tool::RectMarquee, Tool::Pen, Tool::Move, Tool::Brush] {
        let mut editor = PhotocraftApp::new(photocraft_engine::Session::new(), Default::default());
        editor.run("file.new", json!({"width": 400, "height": 300})).unwrap();
        editor.run("layer.new.layer", json!({"name": "Color"})).unwrap();
        editor.run("edit.fill", json!({"color": "#cc6644"})).unwrap();
        editor.ui.tool = tool;
        let mut host = Host::new(editor);
        host.mode = Module::Compositing;
        let mut h = RoundTripUi::new(host);
        h.view.ctx.set_os(egui::os::OperatingSystem::Mac);
        for _ in 0..6 {
            h.step(Vec::new());
        }
        let at = ViewXform::active(&h.host.editor).unwrap().rect.center() + egui::vec2(0.25, 0.25);
        let before = {
            let st = h.host.editor.session.active().unwrap();
            (st.doc.clone(), st.revision, st.history.past_len())
        };
        h.step(vec![Event::ModifiersChanged(Modifiers::CTRL), Event::PointerMoved(at)]);
        h.step(vec![Event::PointerButton { pos: at, button: PointerButton::Primary, pressed: true, modifiers: Modifiers::CTRL }]);
        h.step(vec![
            Event::ModifiersChanged(Modifiers::NONE),
            Event::PointerButton { pos: at, button: PointerButton::Primary, pressed: false, modifiers: Modifiers::NONE },
        ]);
        for _ in 0..3 {
            h.step(Vec::new());
        }
        let ui = &h.host.editor.ui;
        let open = match tool {
            Tool::RectMarquee | Tool::Pen => ui.canvas_tool_menu.is_some(),
            Tool::Move => ui.layer_menu.is_some(),
            Tool::Brush => ui.brush_picker.is_some(),
            _ => false,
        };
        assert!(open, "{tool:?}: the context menu opens");
        let st = h.host.editor.session.active().unwrap();
        assert_eq!((st.doc.clone(), st.revision, st.history.past_len()), before, "{tool:?}: no edit");
    }
}

#[test]
fn control_click_mapping_survives_module_switches_and_pointer_exit() {
    for (os, button) in [(egui::os::OperatingSystem::Mac, PointerButton::Secondary), (egui::os::OperatingSystem::Nix, PointerButton::Primary)] {
        let editor = PhotocraftApp::new(photocraft_engine::Session::new(), Default::default());
        let mut host = Host::new(editor);
        host.library = Some(lightcraft_ui_egui::LightcraftApp::new(lightcraft_engine::Session::new(), Default::default()));
        let ctx = egui::Context::default();
        ctx.set_os(os);
        let input = |pressed, modifiers| egui::RawInput {
            events: vec![Event::PointerButton { pos: egui::pos2(50.0, 50.0), button: PointerButton::Primary, pressed, modifiers }],
            ..Default::default()
        };
        host.mode = Module::Library;
        let mut press = input(true, Modifiers::CTRL);
        eframe::App::raw_input_hook(&mut host, &ctx, &mut press);
        assert!(press.events.iter().any(|event| matches!(event, Event::PointerButton { button: actual, pressed: true, .. } if *actual == button)), "{os:?}");
        host.mode = Module::Develop;
        let mut outside = egui::RawInput { events: vec![Event::PointerGone, Event::WindowFocused(false)], ..Default::default() };
        eframe::App::raw_input_hook(&mut host, &ctx, &mut outside);
        host.mode = Module::Compositing;
        let mut release = input(false, Modifiers::NONE);
        eframe::App::raw_input_hook(&mut host, &ctx, &mut release);
        assert!(release.events.iter().any(|event| matches!(event, Event::PointerButton { button: actual, pressed: false, .. } if *actual == button)), "{os:?}");
    }
}
