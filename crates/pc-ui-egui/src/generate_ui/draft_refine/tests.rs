use super::*;
use egui::{Event, Key, Modifiers, PointerButton, pos2};
use egui_kittest::{
    Harness,
    kittest::{NodeT, Queryable},
};

fn harness(size: egui::Vec2) -> Harness<'static, PhotocraftApp> {
    let mut s = photocraft_engine::Session::new();
    s.execute("file.new", json!({"width": 320, "height": 240})).unwrap();
    s.execute("edit.fill", json!({"color": "#3366cc"})).unwrap();
    let st = crate::ai_ui::EngineStatus {
        connected: true,
        checked: true,
        presets: catalog::presets().iter().map(|p| (p.id(), Ok(()))).collect(),
        ..Default::default()
    };
    crate::ai_ui::TEST_STATUS.with(|v| *v.borrow_mut() = Some(st));
    let mut h = Harness::builder().with_size(size).with_max_steps(64).build_eframe(move |cc| {
        PhotocraftApp::setup_context(&cc.egui_ctx, Default::default());
        PhotocraftApp::new(
            s,
            crate::Services {
                import: Some(Box::new(|name, bytes| photocraft_io::import(name, bytes).map(|r| (r.document, r.warnings)).map_err(|e| e.to_string()))),
                ..Default::default()
            },
        )
    });
    h.state_mut().background_jobs = false;
    h.run_steps(6);
    h
}

fn open_from_menu(h: &mut Harness<'_, PhotocraftApp>) {
    // The actual menu command, also used by the native AI/Image menu.
    let ctx = h.ctx.clone();
    crate::menus::invoke(h.state_mut(), &ctx, "li.draftRefine", json!({})).unwrap();
    h.run_steps(4);
}

fn drag(h: &mut Harness<'_, PhotocraftApp>, from: egui::Pos2, delta: egui::Vec2) {
    h.hover_at(from);
    h.step();
    h.event(Event::PointerButton { pos: from, button: PointerButton::Primary, pressed: true, modifiers: Modifiers::NONE });
    h.step();
    for i in 1..=5 {
        h.event(Event::PointerMoved(from + delta * (i as f32 / 5.0)));
        h.step();
    }
    h.event(Event::PointerButton { pos: from + delta, button: PointerButton::Primary, pressed: false, modifiers: Modifiers::NONE });
    h.run_steps(2);
}

#[test]
fn draft_refine_large_previews_and_window_open_at_both_resolutions() {
    for size in [vec2(1280.0, 800.0), vec2(1920.0, 1080.0)] {
        let mut h = harness(size);
        // Open using the Generate panel's real button.
        crate::dock::reveal(h.state_mut(), crate::dock::Group::Generate);
        h.state_mut().ui.dock_tabs.generate = 0;
        h.run_steps(3);
        h.get_by_label("Draft / Refinement…").click();
        h.run_steps(4);
        assert!(h.state().draft_refine.open);
        let settings = &h.state().ui.ai.draft_refine.refinement;
        let info = ModelId::Klein9B.info();
        assert_eq!(settings.steps, info.steps.default as u32);
        assert_eq!(settings.guidance, info.guidance.default);
        assert_eq!(settings.denoise, info.resolved.sampling.denoise);
        for label in ["Draft preview", "Refinement preview"] {
            let r = h.get_by_label(label).rect();
            assert!(r.width() >= 500.0 && r.height() >= 300.0, "{size:?}: {label}: {r:?}");
            assert!(r.min.x >= 0.0 && r.max.x <= size.x && r.max.y <= size.y);
        }
        assert_eq!(h.get_all_by_label("Steps").filter(|n| n.accesskit_node().role() == egui::accesskit::Role::SpinButton).count(), 1);
        assert!(h.get_all_by_label("Denoise").any(|n| n.accesskit_node().role() == egui::accesskit::Role::SpinButton));
        h.get_all_by_label("Draft").find(|n| n.accesskit_node().role() == egui::accesskit::Role::Button).unwrap().click();
        h.run_steps(2);
        assert!(!h.state().draft_refine.refine_controls);
        assert!(h.query_by_label("Generate draft").is_some());
        assert_eq!(h.get_all_by_label("Guidance").filter(|n| n.accesskit_node().role() == egui::accesskit::Role::SpinButton).count(), 1);
        h.get_all_by_label("Refinement").find(|n| n.accesskit_node().role() == egui::accesskit::Role::Button).unwrap().click();
        h.run_steps(2);
        assert!(h.state().draft_refine.refine_controls);
        assert!(h.get_by_label("Refine image").rect().max.y <= size.y, "refine action must stay on screen");
        let close = h.get_all_by_label("Close").last().unwrap();
        assert!(close.rect().max.y <= size.y, "{size:?}: close button: {:?}", close.rect());
        close.click();
        h.run_steps(2);
        assert!(!h.state().draft_refine.open);
    }
}

#[test]
fn draft_refine_sampling_is_editable_and_bounded_only_by_comfyui() {
    let mut h = harness(vec2(1280.0, 800.0));
    open_from_menu(&mut h);
    // Qwen has variable steps and guidance; Klein's distilled ranges are fixed.
    h.get_by_value(&ModelId::Klein9B.info().label).click();
    h.run_steps(2);
    h.get_by_label(&ModelId::Qwen.info().label).click();
    h.run_steps(3);
    let settings = &h.state().ui.ai.draft_refine.refinement;
    let info = ModelId::Qwen.info();
    assert_eq!(settings.steps, info.steps.default as u32);
    assert_eq!(settings.guidance, info.guidance.default);
    assert_eq!(settings.denoise, info.resolved.sampling.denoise);
    let steps = h.get_all_by_label("Steps").find(|n| n.accesskit_node().role() == egui::accesskit::Role::SpinButton).unwrap().rect();
    let before = h.state().ui.ai.draft_refine.refinement.steps;
    drag(&mut h, steps.center(), vec2(14.0, 0.0));
    assert_ne!(h.state().ui.ai.draft_refine.refinement.steps, before);
    let guidance = h.get_all_by_label("Guidance").find(|n| n.accesskit_node().role() == egui::accesskit::Role::SpinButton).unwrap().rect();
    let before = h.state().ui.ai.draft_refine.refinement.guidance;
    drag(&mut h, guidance.center(), vec2(15.0, 0.0));
    assert_ne!(h.state().ui.ai.draft_refine.refinement.guidance, before);
    // The limits come from the connected ComfyUI's sampler, not the model profile.
    crate::ai_ui::TEST_STATUS.with(|v| {
        if let Some(st) = v.borrow_mut().as_mut() {
            st.limits = Some(li_ai::comfy::SamplerLimits { steps: (1.0, 10000.0), cfg: (0.0, 100.0), denoise: (0.0, 1.0) });
        }
    });
    let s = &mut h.state_mut().ui.ai.draft_refine.refinement;
    s.steps = 150;
    s.guidance = 25.0;
    h.run_steps(2);
    let s = &h.state().ui.ai.draft_refine.refinement;
    assert_eq!((s.steps, s.guidance), (150, 25.0), "beyond the model's recommendation but within ComfyUI's limits");
    let s = &mut h.state_mut().ui.ai.draft_refine.refinement;
    s.steps = u32::MAX;
    s.guidance = 9999.0;
    s.denoise = -2.0;
    h.run_steps(2);
    let s = &h.state().ui.ai.draft_refine.refinement;
    assert_eq!(s.steps, 10000);
    assert_eq!(s.guidance, 100.0);
    assert_eq!(s.denoise, 0.0);
}

#[test]
fn draft_refine_click_carries_current_composite_selected_layer_and_draft_pixels() {
    let mut h = harness(vec2(1280.0, 800.0));
    open_from_menu(&mut h);
    TEST_REQUEST.with(|m| *m.borrow_mut() = Some(Vec::new()));
    h.get_by_label("Refine image").click();
    h.run_steps(3);
    let request = TEST_REQUEST.with(|m| m.borrow().as_ref().unwrap().last().unwrap().clone());
    assert_eq!(request.mode, GenerateMode::Refine);
    assert_eq!(request.source.unwrap(), photocraft_engine::ai_cmds::flatten_rgba(&h.state().session.active().unwrap().doc));
    // Edit after opening the window: use fresh document pixels at request time.
    h.state_mut().run("edit.fill", json!({"color":"#ff0000"})).unwrap();
    h.get_by_label("Refine image").click();
    h.run_steps(2);
    TEST_REQUEST.with(|m| assert_eq!(m.borrow().as_ref().unwrap().last().unwrap().source.as_ref().unwrap().get_pixel(12, 12).0, [255, 0, 0, 255]));
    h.state_mut().run("layer.new.layer", json!({})).unwrap();
    h.state_mut().run("edit.fill", json!({"color":"#00ff00"})).unwrap();
    let d = h.state_mut().session.active_mut().unwrap();
    let layer = d.active_layer.unwrap();
    Arc::make_mut(&mut d.doc).layer_mut(layer).unwrap().opacity = 0.5;
    d.revision += 1;
    let selected = document_input(h.state(), RefineInput::SelectedLayer).unwrap();
    assert_ne!(selected, photocraft_engine::ai_cmds::flatten_rgba(&h.state().session.active().unwrap().doc));
    h.state_mut().ui.ai.draft_refine.input = Input::SelectedLayer;
    h.get_by_label("Refine image").click();
    h.run_steps(2);
    TEST_REQUEST.with(|m| assert_eq!(m.borrow().as_ref().unwrap().last().unwrap().source, Some(selected)));
    h.state_mut().draft_refine.draft.set(RgbaImage::from_pixel(320, 240, image::Rgba([90, 80, 70, 255])));
    // Choose the Draft result through the input picker.
    h.run_steps(2);
    h.get_by_value("Selected layer").click();
    h.run_steps(2);
    h.get_by_label("Draft result").click();
    h.run_steps(2);
    h.get_by_label("Refine image").click();
    h.run_steps(2);
    TEST_REQUEST.with(|m| assert_eq!(m.borrow().as_ref().unwrap().last().unwrap().source.as_ref().unwrap().get_pixel(12, 12).0, [90, 80, 70, 255]));
    TEST_REQUEST.with(|m| *m.borrow_mut() = None);
}

#[test]
fn draft_refine_zoom_pan_comparison_and_saved_defaults() {
    let mut h = harness(vec2(1280.0, 800.0));
    open_from_menu(&mut h);
    let p = h.get_by_label("Refinement preview").rect().center();
    h.hover_at(p);
    h.event(Event::MouseWheel { unit: egui::MouseWheelUnit::Point, delta: vec2(0.0, 80.0), phase: egui::TouchPhase::Move, modifiers: Modifiers::NONE });
    h.run_steps(4);
    assert!(h.state().draft_refine.source.camera.scale.is_some());
    drag(&mut h, p, vec2(20.0, 10.0));
    assert_ne!(h.state().draft_refine.source.camera.center, vec2(0.5, 0.5));
    h.key_press(Key::F);
    h.run_steps(2);
    assert!(h.state().draft_refine.source.camera.scale.is_none());
    let original = h.state().draft_refine.source.image.as_ref().unwrap().as_ref().clone();
    h.state_mut().draft_refine.before_result.set(original);
    h.state_mut().draft_refine.refinement.set(RgbaImage::from_pixel(320, 240, image::Rgba([255, 90, 40, 255])));
    h.run_steps(2);
    h.get_by_label("Compare with input").click();
    h.run_steps(2);
    assert!(h.state().draft_refine.compare);
    let slider = h.get_all_by_label("Comparison").find(|n| n.accesskit_node().role() == egui::accesskit::Role::Slider).unwrap().rect();
    drag(&mut h, slider.center(), vec2(25.0, 0.0));
    assert!(h.state().draft_refine.split > 0.5);
    let old: crate::ai_ui::AiOptions = serde_json::from_value(json!({"remove_engine":"qwen-int8","generate":{"prompt":"old"}})).unwrap();
    assert!(!old.remove_immediately);
    assert_eq!(old.generate.refine_input, RefineInput::Composite);
    assert_eq!(old.draft_refine.input, Input::Composite);
    h.hover_at(pos2(15.0, 15.0));
}

#[test]
fn draft_refine_dock_request_keeps_open_image_and_drops_hidden_create_references() {
    let h = harness(vec2(1280.0, 800.0));
    let s = GenerateState { mode: Mode::Refine, ..Default::default() };
    let d = h.state().session.active().unwrap();
    let source = document_input(h.state(), s.refine_input).unwrap();
    let stale = (0..6).map(|_| RgbaImage::new(1, 1)).collect();
    let (req, target, open) = local_request(&s, String::new(), Some((d.doc.id, source.clone(), None)), stale).unwrap();
    assert_eq!(req.source, Some(source));
    assert!(req.references.is_empty());
    assert!(req.validate().is_ok());
    assert_eq!(target, Some(d.doc.id));
    assert!(!open);
}

#[test]
fn draft_refine_mock_comfy_uses_source_and_library_results_can_be_used_or_opened() {
    let _lock = crate::ai_ui::AI_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let server = match li_ai::mock::MockComfy::start() {
        Ok(server) => server,
        Err(e)
            if e.kind() == std::io::ErrorKind::PermissionDenied
                || e.get_ref().and_then(|e| e.downcast_ref::<std::io::Error>()).is_some_and(|e| e.kind() == std::io::ErrorKind::PermissionDenied) =>
        {
            eprintln!("skipped: sandbox denies mock sockets");
            return;
        }
        Err(e) => panic!("{e}"),
    };
    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            li_ai::set_service_override(None);
        }
    }
    li_ai::set_service_override(Some(server.host().to_owned()));
    let _reset = Reset;
    let mut h = harness(vec2(1280.0, 800.0));
    open_from_menu(&mut h);
    h.get_by_label("Refine image").click();
    h.run_steps(3);
    let start = Instant::now();
    while !h.state().draft_refine.jobs.is_empty() && start.elapsed() < Duration::from_secs(10) {
        std::thread::sleep(Duration::from_millis(10));
        h.run_steps(2);
    }
    assert!(h.state().draft_refine.refinement_entry.is_some(), "{}", h.state().ui.status);
    let graph = server.prompts().last().unwrap().clone();
    assert!(graph.as_object().unwrap().values().any(|n| n["class_type"] == "LoadImage"));
    assert!(graph.as_object().unwrap().values().any(|n| n["class_type"] == "VAEEncode"));
    let entry = h.state().draft_refine.refinement_entry.as_ref().unwrap().clone();
    assert!(Library::default().image_path(&entry.id).is_file());
    let before = h.state().session.active().unwrap().history.past_len();
    h.get_by_label("Use").click();
    h.run_steps(3);
    assert_eq!(h.state().session.active().unwrap().history.past_len(), before + 1);
    h.state_mut().run("edit.undo", json!({})).unwrap();
    let docs = h.state().session.documents().len();
    h.get_by_label("Open as new document").click();
    h.run_steps(4);
    assert_eq!(h.state().session.documents().len(), docs + 1);
}

#[test]
fn draft_refine_library_use_and_open_actions_are_one_undo_and_a_new_document() {
    let mut h = harness(vec2(1280.0, 800.0));
    open_from_menu(&mut h);
    let image = RgbaImage::from_pixel(320, 240, image::Rgba([70, 100, 130, 255]));
    let library = Library::default();
    let entry =
        library.add(&image, "DraftRefine-UI-test", Some(json!({"model":ModelId::Klein9B.key(),"mode":"refine","steps":4,"guidance":1.0})), None).unwrap();
    set_result(h.state_mut(), true, entry.clone());
    h.run_steps(3);
    let before = h.state().session.active().unwrap().doc.clone();
    let history = h.state().session.active().unwrap().history.past_len();
    h.get_by_label("Use").click();
    h.run_steps(3);
    assert_eq!(h.state().session.active().unwrap().history.past_len(), history + 1);
    assert_eq!(h.state().session.active().unwrap().doc.walk().len(), before.walk().len() + 1);
    h.state_mut().run("edit.undo", json!({})).unwrap();
    assert_eq!(h.state().session.active().unwrap().doc, before);
    let count = h.state().session.documents().len();
    h.get_by_label("Open as new document").click();
    h.run_steps(3);
    assert_eq!(h.state().session.documents().len(), count + 1);
    assert!(library.list().iter().any(|e| e.id == entry.id));
    // Remove only this test's generated entry.
    std::fs::remove_dir_all(library.image_path(&entry.id).parent().unwrap()).unwrap();
}
