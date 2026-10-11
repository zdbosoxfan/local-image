struct RequestReset;
impl Drop for RequestReset {
    fn drop(&mut self) {
        TEST_REQUEST.with(|m| *m.borrow_mut() = None);
    }
}

use super::*;
use egui::{Event, Key, Modifiers, PointerButton, pos2};
use egui_kittest::{
    Harness,
    kittest::{NodeT, Queryable},
};

#[derive(Default)]
struct CpuRenderer(crate::tool_icon_tests::cpu::TextureStore);

impl egui_kittest::TestRenderer for CpuRenderer {
    fn handle_delta(&mut self, delta: &mut egui::TexturesDelta) {
        self.0.apply(std::mem::take(delta));
    }
    fn render(&mut self, ctx: &egui::Context, output: &egui::FullOutput) -> Result<RgbaImage, String> {
        let scale = output.pixels_per_point;
        let size = ctx.content_rect().size() * scale;
        let primitives = ctx.tessellate(output.shapes.clone(), scale);
        let image = crate::tool_icon_tests::cpu::paint(&primitives, &self.0, [size.x as usize, size.y as usize], scale, Tokens::get(ctx).canvas);
        RgbaImage::from_raw(image.size[0] as u32, image.size[1] as u32, image.pixels.iter().flat_map(|p| p.to_array()).collect())
            .ok_or_else(|| "invalid CPU image size".into())
    }
}

fn harness(size: egui::Vec2) -> Harness<'static, PhotocraftApp> {
    harness_render(size, false)
}

fn harness_render(size: egui::Vec2, gpu: bool) -> Harness<'static, PhotocraftApp> {
    let mut s = photocraft_engine::Session::new();
    s.execute("file.new", json!({"width": 320, "height": 240})).unwrap();
    s.execute("edit.fill", json!({"color": "#3366cc"})).unwrap();
    let st = crate::ai_ui::EngineStatus {
        connected: true,
        checked: true,
        host: li_ai::service().client.host().into(),
        presets: catalog::presets().iter().map(|p| (p.id(), Ok(()))).collect(),
        ..Default::default()
    };
    crate::ai_ui::TEST_STATUS.with(|v| *v.borrow_mut() = Some(st));
    let mut builder = Harness::builder().with_size(size).with_max_steps(64).renderer(CpuRenderer::default());
    if gpu {
        let rs = egui_kittest::wgpu::create_render_state(crate::gpu_canvas::wgpu_setup(), eframe::egui_wgpu::RendererOptions::PREDICTABLE);
        let adapter = rs.adapter.get_info();
        assert_ne!(adapter.device_type, eframe::wgpu::DeviceType::Cpu, "hardware GPU required: {adapter:?}");
        eprintln!("Draft / Refinement screenshot adapter: {adapter:?}");
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/draft-style");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("gpu-adapter.txt"), format!("{adapter:?}\n")).unwrap();
        builder = builder.renderer(egui_kittest::wgpu::WgpuTestRenderer::from_render_state(rs));
    }
    let mut h = builder.build_eframe(move |cc| {
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

#[test]
fn zimage_turbo_variety_tip_follows_picker_clicks_in_generate_and_draft_gpu() {
    use egui::accesskit::Role;
    let _lock = crate::ai_ui::AI_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let tip = model_variety_tip(ModelId::ZImageTurbo).unwrap();
    for model in ModelId::all().into_iter().filter(|m| *m != ModelId::ZImageTurbo) {
        assert!(model_variety_tip(model).is_none(), "{model:?}");
    }
    assert!(model_variety_tip(ModelId::intern("installed:z_image_bf16.safetensors")).is_none());
    assert!(crate::icons::exists("info"));
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/zimage-variety/ui");
    std::fs::create_dir_all(&dir).unwrap();
    let mut h = harness_render(vec2(1280.0, 800.0), true);
    crate::dock::reveal(h.state_mut(), crate::dock::Group::Generate);
    h.state_mut().ui.dock_tabs.generate = 0;
    h.run_steps(3);
    assert!(h.query_by_label(tip).is_none());
    let current = model_of(&h.state().ui.ai.generate).info();
    h.get_by_value(&format!("{} · {}", current.label, current.family_label)).click();
    h.run_steps(2);
    h.get_by_role_and_label(Role::Button, &ModelId::ZImageTurbo.info().label).click();
    h.run_steps(3);
    h.get_by_label(tip).scroll_to_me();
    h.run_steps(20);
    let note = h.get_by_label(tip).rect();
    assert!(egui::Rect::from_min_size(egui::Pos2::ZERO, vec2(1280.0, 800.0)).contains_rect(note));
    h.render().unwrap().save(dir.join("generate-tip.png")).unwrap();
    let info = ModelId::ZImageTurbo.info();
    h.get_by_value(&format!("{} · {}", info.label, info.family_label)).scroll_to_me();
    h.run_steps(20);
    h.get_by_value(&format!("{} · {}", info.label, info.family_label)).click();
    h.run_steps(2);
    let row = h.get_by_role_and_label(Role::Button, &info.label).rect();
    h.hover_at(row.center());
    h.run_steps(90);
    h.get_by_label(&format!("{}\n{}\n{tip}", info.best_for, info.tags().join(" · ")));
    h.render().unwrap().save(dir.join("picker-tooltip.png")).unwrap();
    h.get_by_role_and_label(Role::Button, &ModelId::Ernie.info().label).click();
    h.run_steps(3);
    assert!(h.query_by_label(tip).is_none());
    open_from_menu(&mut h);
    h.get_by_role_and_label(Role::Button, "Draft").click();
    h.run_steps(3);
    let current = model_of(&h.state().ui.ai.draft_refine.draft).info();
    h.get_by_value(&format!("{} · {}", current.label, current.family_label)).click();
    h.run_steps(2);
    h.get_by_role_and_label(Role::Button, &info.label).click();
    h.run_steps(3);
    let note = h.get_by_label(tip).rect();
    assert!(egui::Rect::from_min_size(egui::Pos2::ZERO, vec2(1280.0, 800.0)).contains_rect(note));
    h.get_by_role_and_label(Role::Slider, "Variety").scroll_to_me();
    h.run_steps(20);
    let slider = h.get_by_role_and_label(Role::Slider, "Variety").rect();
    let from = slider.left_center() + vec2(8.0, 0.0);
    let delta = vec2(slider.width() * 0.6, 0.0);
    h.hover_at(from);
    h.step();
    h.event(Event::PointerButton { pos: from, button: PointerButton::Primary, pressed: true, modifiers: Modifiers::NONE });
    h.step();
    let mut frame_ms = Vec::new();
    for i in 1..=5 {
        let start = std::time::Instant::now();
        h.event(Event::PointerMoved(from + delta * (i as f32 / 5.0)));
        h.step();
        h.render().unwrap();
        frame_ms.push(start.elapsed().as_secs_f64() * 1000.0);
    }
    h.event(Event::PointerButton { pos: from + delta, button: PointerButton::Primary, pressed: false, modifiers: Modifiers::NONE });
    h.run_steps(2);
    std::fs::write(
        dir.join("interaction-times.json"),
        serde_json::to_vec_pretty(&json!({"scenario":"Draft Variety drag: app update and GPU render/readback", "size":[1280,800], "frame_ms":frame_ms}))
            .unwrap(),
    )
    .unwrap();
    let variety = h.state().ui.ai.draft_refine.draft.variety;
    assert!(variety > 0.0 && variety <= 100.0, "slider={slider:?}, value={variety}");
    let req = stage_request(h.state(), false).unwrap();
    assert_eq!(req.variety, variety);
    let saved = serde_json::to_value(&h.state().ui.ai.draft_refine).unwrap();
    let restored: Settings = serde_json::from_value(saved).unwrap();
    assert_eq!(restored.draft.variety, variety);
    h.render().unwrap().save(dir.join("draft-tip.png")).unwrap();
    h.get_by_role_and_label(Role::Button, "Off").click();
    h.run_steps(2);
    assert_eq!(h.state().ui.ai.draft_refine.draft.variety, 0.0);
    h.get_by_value(&format!("{} · {}", info.label, info.family_label)).scroll_to_me();
    h.run_steps(20);
    h.get_by_value(&format!("{} · {}", info.label, info.family_label)).click();
    h.run_steps(2);
    h.get_by_role_and_label(Role::Button, &ModelId::Klein4B.info().label).click();
    h.run_steps(3);
    assert!(h.query_by_label(tip).is_none());
    assert!(h.query_by_role_and_label(Role::Slider, "Variety").is_none());
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
    let _gpu_gate = crate::gpu_test_support::lock();
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
        assert_eq!(settings.denoise, catalog::refine_default_denoise(ModelId::Klein9B));
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
    let _gpu_gate = crate::gpu_test_support::lock();
    let mut h = harness(vec2(1280.0, 800.0));
    open_from_menu(&mut h);
    // Qwen has variable steps and guidance; Klein's distilled ranges are fixed.
    h.get_by_value(&format!("{} · {}", ModelId::Klein9B.info().label, ModelId::Klein9B.info().family_label)).click();
    h.run_steps(2);
    h.get_by_role_and_label(egui::accesskit::Role::Button, &ModelId::Qwen.info().label).click();
    h.run_steps(3);
    let settings = &h.state().ui.ai.draft_refine.refinement;
    let info = ModelId::Qwen.info();
    assert_eq!(settings.steps, info.steps.default as u32);
    assert_eq!(settings.guidance, info.guidance.default);
    assert_eq!(settings.denoise, catalog::refine_default_denoise(ModelId::Qwen));
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
    let _gpu_gate = crate::gpu_test_support::lock();
    let mut h = harness(vec2(1280.0, 800.0));
    open_from_menu(&mut h);
    let _request_reset = RequestReset;
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
    let _gpu_gate = crate::gpu_test_support::lock();
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
    let _gpu_gate = crate::gpu_test_support::lock();
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
    let _gpu_gate = crate::gpu_test_support::lock();
    let _lock = crate::ai_ui::AI_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    // The catalogue, checker and status poller are process-wide. Other UI tests
    // replace their inventories, so an asynchronous refinement needs its own
    // process, like the unload-completion regression. Keep real model files and
    // persisted verdicts out of this mock-server test too.
    const CHILD: &str = "PC_DRAFT_REFINE_MOCK_TEST_CHILD";
    if std::env::var_os(CHILD).is_none() {
        let profile = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/ds/draft-refine-mock").join(std::process::id().to_string());
        std::fs::create_dir_all(profile.join("models")).unwrap();
        struct ProfileReset(std::path::PathBuf);
        impl Drop for ProfileReset {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
        let _profile_reset = ProfileReset(profile.clone());
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "generate_ui::draft_refine::tests::draft_refine_mock_comfy_uses_source_and_library_results_can_be_used_or_opened", "--nocapture"])
            .env(CHILD, "1")
            .env("LOCAL_IMAGE_DATA_DIR", &profile)
            .env("LOCAL_IMAGE_MODELS_DIR", profile.join("models"))
            .output()
            .unwrap();
        eprint!("{}", String::from_utf8_lossy(&output.stderr));
        assert!(output.status.success(), "Mock refinement child failed: {}", String::from_utf8_lossy(&output.stdout));
        return;
    }
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
    // A hang watchdog, not a UI speed budget: allow saturated builders time to
    // schedule the real HTTP worker and encode the fixture before asserting its
    // result and the Use/Open actions below.
    while !h.state().draft_refine.jobs.is_empty() && start.elapsed() < Duration::from_secs(120) {
        std::thread::sleep(Duration::from_millis(10));
        h.run_steps(2);
    }
    assert!(h.state().draft_refine.refinement_entry.is_some(), "{}", h.state().ui.status);
    let graph = server.prompts().last().unwrap().clone();
    assert!(graph.as_object().unwrap().values().any(|n| n["class_type"] == "LoadImage"));
    assert!(graph.as_object().unwrap().values().any(|n| n["class_type"] == "VAEEncode"));
    let entry = h.state().draft_refine.refinement_entry.as_ref().unwrap().clone();
    assert!(crate::generate_ui::library().image_path(&entry.id).is_file());
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
    let _gpu_gate = crate::gpu_test_support::lock();
    let mut h = harness(vec2(1280.0, 800.0));
    open_from_menu(&mut h);
    let image = RgbaImage::from_pixel(320, 240, image::Rgba([70, 100, 130, 255]));
    let library = crate::generate_ui::library();
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

#[test]
fn draft_refine_gpu_design_system_all_themes_and_resolutions() {
    let _gpu_gate = crate::gpu_test_support::lock();
    use egui::accesskit::{Role, Toggled};
    let _lock = crate::ai_ui::AI_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/draft-style");
    std::fs::create_dir_all(&dir).unwrap();
    for theme in crate::theme::ThemeKind::ALL {
        for size in [vec2(1280.0, 800.0), vec2(1920.0, 1080.0)] {
            let mut h = harness_render(size, true);
            let ctx = h.ctx.clone();
            h.state_mut().set_theme(&ctx, theme);
            open_from_menu(&mut h);
            h.state_mut().ui.ai.draft_refine.draft.prompt = "a colour study".into();
            let source = h.state().draft_refine.source.image.as_ref().unwrap().as_ref().clone();
            h.state_mut().draft_refine.before_result.set(source);
            h.state_mut().draft_refine.draft.set(RgbaImage::from_fn(320, 240, |x, y| image::Rgba([40 + (x / 2) as u8, 50 + (y / 2) as u8, 110, 255])));
            h.state_mut().draft_refine.refinement.set(RgbaImage::from_fn(320, 240, |x, y| image::Rgba([60 + (x / 2) as u8, 70 + (y / 2) as u8, 140, 255])));
            h.state_mut().draft_refine.compare = true;
            h.run_steps(2);
            for refine in [true, false] {
                let tab = if refine { "Refinement" } else { "Draft" };
                h.get_by_role_and_label(Role::Button, tab).click();
                h.run_steps(3);
                let node = h.get_by_role_and_label(Role::Button, tab);
                assert_eq!(node.accesskit_node().toggled(), Some(Toggled::True), "pill tab selection: {theme:?}");
                assert_eq!(node.rect().height(), 24.0, "shared pill-tab height");
                for label in ["Draft preview", "Refinement preview"] {
                    let rect = h.get_by_label(label).rect();
                    assert!(rect.width() >= 500.0 && rect.height() >= 300.0, "{theme:?}: {size:?}: {label}: {rect:?}");
                    assert!(egui::Rect::from_min_size(egui::Pos2::ZERO, size).contains_rect(rect));
                }
                let label = if refine { "Refine image" } else { "Generate draft" };
                let button = h.get_by_role_and_label(Role::Button, label).rect();
                assert_eq!(button.height(), if Tokens::for_kind(theme).pro { 28.0 } else { 30.0 });
                for name in ["Steps", "Guidance", "Width", "Height"] {
                    let node = h.get_by_role_and_label(Role::SpinButton, name);
                    assert!(egui::Rect::from_min_size(egui::Pos2::ZERO, size).contains_rect(node.rect()), "{theme:?}: {name}: {:?}", node.rect());
                    assert!(node.rect().width() >= 40.0, "app value field");
                }
                if refine {
                    let denoise = h.get_by_role_and_label(Role::SpinButton, "Denoise");
                    assert_eq!(denoise.accesskit_node().numeric_value(), Some(1.0), "Klein's reference default must be visible");
                    h.get_by_label("Refines by reference: the image guides the result");
                }
                for name in ["Close", label] {
                    let rect = h.get_all_by_label(name).last().unwrap().rect();
                    assert!(egui::Rect::from_min_size(egui::Pos2::ZERO, size).contains_rect(rect), "{theme:?}: {name}: {rect:?}");
                }
                h.hover_at(pos2(5.0, 5.0));
                h.run_steps(2);
                let pixels = h.render().unwrap();
                // The custom primary action paints the theme's primary colour, including Pro density.
                let point = button.left_center() + vec2(20.0, 0.0);
                assert_eq!(pixels.get_pixel(point.x as u32, point.y as u32).0, Tokens::for_kind(theme).primary_bg.to_array());
                pixels
                    .save(dir.join(format!("{}-{}x{}-{}.png", theme.id(), size.x as u32, size.y as u32, if refine { "refinement" } else { "draft" })))
                    .unwrap();
                if refine && theme == crate::theme::ThemeKind::Pro && size.x == 1280.0 {
                    h.get_by_value(&format!("{} · {}", ModelId::Klein9B.info().label, ModelId::Klein9B.info().family_label)).click();
                    h.run_steps(3);
                    h.get_by_label("Edit models — refine by reference");
                    h.get_by_label("Image models — refine by denoise");
                    h.render().unwrap().save(dir.join("pro-1280x800-refine-model-picker.png")).unwrap();
                    h.event(Event::Key { key: Key::Escape, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::NONE });
                    h.run_steps(2);
                }
            }
        }
    }
}

#[test]
fn draft_refine_lora_picker_filters_family_and_carries_strength_in_request() {
    let _gpu_gate = crate::gpu_test_support::lock();
    let _lock = crate::ai_ui::AI_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let info = li_ai::comfy::ObjectInfo(
        json!({"LoraLoader": {"input": {"required": {"lora_name": [["qwen_image_2.1_style.safetensors", "flux_realism_lora.safetensors"]]}}}}),
    );
    li_ai::inventory::refresh(&info, None);
    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            li_ai::inventory::refresh(&li_ai::comfy::ObjectInfo(json!({})), None);
            TEST_REQUEST.with(|m| *m.borrow_mut() = None);
        }
    }
    let _reset = Reset;
    let mut h = harness(vec2(1280.0, 800.0));
    open_from_menu(&mut h);
    h.get_by_value(&format!("{} · {}", ModelId::Klein9B.info().label, ModelId::Klein9B.info().family_label)).click();
    h.run_steps(2);
    h.get_by_role_and_label(egui::accesskit::Role::Button, &ModelId::Qwen.info().label).click();
    h.run_steps(3);
    h.get_by_label("+ Add").click();
    h.run_steps(2);
    assert!(h.query_by_label("flux realism lora").is_none(), "other families aren't offered");
    h.get_by_label("qwen image 2.1 style").click();
    h.run_steps(3);
    let pick = &h.state().ui.ai.draft_refine.refinement.loras[0];
    assert_eq!(pick.name, "qwen_image_2.1_style.safetensors");
    // Scroll the extra LoRA row into view, then scrub its actual strength field.
    h.get_by_role_and_label(egui::accesskit::Role::SpinButton, "Strength").scroll_to_me();
    h.run_steps(3);
    let strength = h.get_by_role_and_label(egui::accesskit::Role::SpinButton, "Strength").rect();
    drag(&mut h, strength.center(), vec2(10.0, 0.0));
    let strength = h.state().ui.ai.draft_refine.refinement.loras[0].strength;
    assert_ne!(strength, 0.8);
    let _request_reset = RequestReset;
    TEST_REQUEST.with(|m| *m.borrow_mut() = Some(Vec::new()));
    h.get_by_label("Refine image").click();
    h.run_steps(2);
    let request = TEST_REQUEST.with(|m| m.borrow().as_ref().unwrap().last().unwrap().clone());
    assert_eq!(request.loras, vec![li_ai::workflows::LoraUse { name: "qwen_image_2.1_style.safetensors".into(), strength }]);
}

#[test]
fn draft_refine_properties_sections_collapse_and_sampling_has_no_offline_ceiling() {
    let _gpu_gate = crate::gpu_test_support::lock();
    let mut h = harness(vec2(1280.0, 800.0));
    open_from_menu(&mut h);
    h.get_all_by_label("Advanced").last().unwrap().click();
    h.run_steps(2);
    assert!(h.query_by_role_and_label(egui::accesskit::Role::SpinButton, "Steps").is_none());
    h.get_all_by_label("Advanced").last().unwrap().click();
    h.run_steps(2);
    let s = &mut h.state_mut().ui.ai.draft_refine.refinement;
    s.steps = 16_777_217;
    s.guidance = -5.0;
    s.denoise = 1.25;
    h.run_steps(2);
    let s = &h.state().ui.ai.draft_refine.refinement;
    assert_eq!((s.steps, s.guidance, s.denoise), (16_777_217, -5.0, 1.25));
    assert_eq!(h.get_by_role_and_label(egui::accesskit::Role::SpinButton, "Steps").accesskit_node().numeric_value(), Some(16_777_217.0));
    let request = stage_request(h.state(), true).unwrap();
    assert_eq!((request.steps, request.guidance, request.denoise), (16_777_217, -5.0, 1.25));
}

#[test]
fn draft_refine_fresh_and_changed_refinement_model_use_per_model_denoise() {
    let _gpu_gate = crate::gpu_test_support::lock();
    use egui::accesskit::Role;
    let mut h = harness(vec2(1280.0, 800.0));
    open_from_menu(&mut h);
    assert_eq!(h.state().ui.ai.draft_refine.refinement.denoise, 1.0);
    h.get_by_label("Refines by reference: the image guides the result");
    let denoise = h.get_by_role_and_label(egui::accesskit::Role::SpinButton, "Denoise").rect();
    drag(&mut h, denoise.center(), vec2(-20.0, 0.0));
    assert_ne!(h.state().ui.ai.draft_refine.refinement.denoise, 1.0, "denoise stays editable");
    h.get_by_value(&format!("{} · {}", ModelId::Klein9B.info().label, ModelId::Klein9B.info().family_label)).click();
    h.run_steps(2);
    let reference = h.get_by_label("Edit models — refine by reference").rect();
    let img2img = h.get_by_label("Image models — refine by denoise").rect();
    let klein = h.get_by_role_and_label(Role::Button, &ModelId::Klein9B.info().label).rect();
    let zimage = h.get_by_role_and_label(Role::Button, &ModelId::ZImageTurbo.info().label).rect();
    assert!(reference.max.y < klein.min.y && klein.max.y < img2img.min.y && img2img.max.y < zimage.min.y);
    for excluded in [ModelId::SeedVr2, ModelId::KleinRemove] {
        assert!(h.query_by_role_and_label(Role::Button, &excluded.info().label).is_none());
    }
    h.get_by_role_and_label(egui::accesskit::Role::Button, &ModelId::Qwen.info().label).click();
    h.run_steps(3);
    // Qwen edit refines by reference at full denoise (verified with real refines).
    assert_eq!(h.state().ui.ai.draft_refine.refinement.denoise, 1.0);
    assert_eq!(stage_request(h.state(), true).unwrap().denoise, 1.0);
    h.get_by_value(&format!("{} · {}", ModelId::Qwen.info().label, ModelId::Qwen.info().family_label)).click();
    h.run_steps(2);
    h.get_by_role_and_label(Role::Button, &ModelId::ZImageTurbo.info().label).click();
    h.run_steps(3);
    assert_eq!(h.state().ui.ai.draft_refine.refinement.denoise, 0.35);
    h.get_by_label("Refines by denoise: lower keeps more of the image");
    h.get_by_value(&format!("{} · {}", ModelId::ZImageTurbo.info().label, ModelId::ZImageTurbo.info().family_label)).click();
    h.run_steps(2);
    h.get_by_role_and_label(Role::Button, &ModelId::Klein9B.info().label).click();
    h.run_steps(3);
    assert_eq!(h.state().ui.ai.draft_refine.refinement.denoise, 1.0);
    assert_eq!(stage_request(h.state(), true).unwrap().denoise, 1.0);
}

#[test]
fn draft_refine_shared_aspect_presets_lock_and_model_grid() {
    let _gpu_gate = crate::gpu_test_support::lock();
    let mut h = harness(vec2(1280.0, 800.0));
    open_from_menu(&mut h);
    h.get_by_role_and_label(egui::accesskit::Role::Button, "16:9").click();
    h.run_steps(2);
    let s = &h.state().ui.ai.draft_refine.refinement;
    let grid = model_of(s).info().resolved.sizes.multiple.max(8);
    assert_eq!(s.aspect, "16:9");
    assert!(s.aspect_locked);
    let before = (s.width, s.height);
    let width = h.get_by_role_and_label(egui::accesskit::Role::SpinButton, "Width").rect();
    drag(&mut h, width.center(), vec2(60.0, 0.0));
    let s = &h.state().ui.ai.draft_refine.refinement;
    assert_ne!((s.width, s.height), before);
    assert_eq!((s.width % grid, s.height % grid), (0, 0));
    assert!((s.width as f32 / s.height as f32 - 16.0 / 9.0).abs() < 0.03);
    h.get_by_label("Link width and height").click();
    h.run_steps(2);
    let height_before = h.state().ui.ai.draft_refine.refinement.height;
    drag(&mut h, width.center(), vec2(60.0, 0.0));
    let s = &h.state().ui.ai.draft_refine.refinement;
    assert!(!s.aspect_locked);
    assert_eq!(s.height, height_before);
    assert_eq!(s.aspect, "custom");
    assert_eq!((s.width % grid, s.height % grid), (0, 0));
    let old: GenerateState = serde_json::from_value(json!({"width": 320, "height": 240})).unwrap();
    assert!(old.aspect_locked, "old settings load with linked dimensions");
}

#[test]
fn autodetect_refine_picker_disables_failure_and_click_retest_verifies_on_worker_gpu() {
    let _gpu_gate = crate::gpu_test_support::lock();
    use egui::accesskit::Role;
    use li_ai::refine::{Status, Verdict};
    let _lock = crate::ai_ui::AI_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/autodetect-ui");
    std::fs::create_dir_all(dir.join("checkpoints")).unwrap();
    std::fs::write(dir.join("checkpoints/autodetect-ui-new.safetensors"), b"fake installed weights").unwrap();
    let server = li_ai::mock::MockComfy::start().unwrap();
    server.set_model_dir(&dir);
    server.delay_ms.store(1, std::sync::atomic::Ordering::SeqCst);
    li_ai::set_service_override(Some(server.host().into()));
    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            li_ai::set_service_override(None);
            li_ai::catalog::set_installed(Vec::new());
        }
    }
    let _reset = Reset;
    let client = li_ai::ComfyClient::new(server.host());
    li_ai::inventory::refresh(&client.object_info().unwrap(), Some(&dir));
    let model = ModelId::from_key("ckpt:autodetect-ui-new.safetensors").unwrap();
    server.fail_next();
    let checker = li_ai::refine::checker();
    let result = checker.check_in(&client, model, "installed", &dir, true, &li_ai::JobControl::new()).unwrap().unwrap();
    assert!(matches!(result, Verdict::Failed { .. }));
    let mut h = harness_render(vec2(1280.0, 800.0), true);
    open_from_menu(&mut h);
    let current = format!("{} · {}", ModelId::Klein9B.info().label, ModelId::Klein9B.info().family_label);
    h.get_by_value(&current).click();
    h.run_steps(3);
    h.get_by_role_and_label(Role::Button, &model.info().label).scroll_to_me();
    h.run_steps(3);
    assert!(h.get_by_role_and_label(Role::Button, &model.info().label).accesskit_node().is_disabled());
    h.render().unwrap().save(dir.join("unsupported-picker.png")).unwrap();
    // This is the actual Re-test button and actual worker, including HTTP queue execution.
    server.delay_ms.store(600, std::sync::atomic::Ordering::SeqCst);
    h.get_by_label("Re-test").click();
    h.run_steps(2);
    assert_eq!(checker.status(&client, model), Status::Testing);
    // The UI keeps accepting input while the slow mock job runs.
    if h.query_by_label("testing…").is_none() {
        h.get_by_value(&current).click();
        h.run_steps(3);
    }
    h.get_by_label("testing…");
    let frame_started = Instant::now();
    h.run_steps(1);
    h.render().unwrap().save(dir.join("testing-picker.png")).unwrap();
    let frame_ms = frame_started.elapsed().as_secs_f64() * 1000.0;
    std::fs::write(dir.join("testing-frame-ms.txt"), format!("{frame_ms:.3}\n")).unwrap();
    eprintln!("Refine picker update and Vulkan render while checking: {frame_ms:.3} ms (includes screenshot encoding)");
    let start = Instant::now();
    while checker.status(&client, model) == Status::Testing && start.elapsed() < Duration::from_secs(10) {
        std::thread::sleep(Duration::from_millis(10));
        h.run_steps(1);
    }
    assert_eq!(checker.status(&client, model), Status::Verified);
    h.run_steps(3);
    let label = model.info().label.clone();
    let row = h.get_by_role_and_label(Role::Button, &label);
    assert!(!row.accesskit_node().is_disabled());
    row.click();
    h.run_steps(3);
    assert_eq!(h.state().ui.ai.draft_refine.refinement.model, model.key());
    h.render().unwrap().save(dir.join("verified-selection.png")).unwrap();
    assert_eq!(server.prompts().len(), 2, "one failed check and one successful re-test");
    server.delay_ms.store(2000, std::sync::atomic::Ordering::SeqCst);
    let current = format!("{} · {}", model.info().label, model.info().family_label);
    h.get_by_value(&current).click();
    h.run_steps(2);
    h.get_by_role_and_label(Role::Button, &label).scroll_to_me();
    h.run_steps(2);
    h.get_by_label("Re-test").click();
    h.run_steps(2);
    if h.query_by_label("Cancel test").is_none() {
        h.get_by_value(&current).click();
        h.run_steps(2);
    }
    h.get_by_label("Cancel test").click();
    h.run_steps(2);
    let start = Instant::now();
    while checker.status(&client, model) == Status::Testing && start.elapsed() < Duration::from_secs(10) {
        std::thread::sleep(Duration::from_millis(10));
        h.run_steps(1);
    }
    assert_eq!(checker.status(&client, model), Status::Unchecked, "cancellation is transient");
    assert_eq!(server.interrupts(), 0, "the checker never sends a global interrupt");
}

#[test]
fn ds_finding_mock_request_guard_clears_during_unwind() {
    let result = std::panic::catch_unwind(|| {
        let _reset = RequestReset;
        TEST_REQUEST.with(|m| *m.borrow_mut() = Some(vec![]));
        panic!("test unwind");
    });
    assert!(result.is_err());
    assert!(TEST_REQUEST.with(|m| m.borrow().is_none()));
}
