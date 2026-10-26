use super::*;

#[test]
fn cloud_cap_blocks_only_runs_that_would_pass_it() {
    assert_eq!(cloud_cap_message(0, 500, 4), None, "0 is unlimited");
    assert_eq!(cloud_cap_message(10, 6, 4), None);
    assert!(cloud_cap_message(10, u32::MAX, 4).is_some());
    let blocked = cloud_cap_message(10, 8, 4).unwrap();
    assert!(blocked.contains("8 of 10") && blocked.contains("needs 4") && blocked.contains("Local AI › Cloud"), "{blocked}");
}

#[test]
fn cloud_run_waits_for_confirmation_and_cancel_starts_nothing() {
    use egui_kittest::kittest::Queryable;
    let _lock = crate::ai_ui::AI_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let c = li_ai::cloud::model("openai:gpt-image-1").unwrap();
    let base = li_ai::cloud::CloudRequest {
        model: c,
        mode: li_ai::cloud::CloudMode::Create,
        prompt: "a red boat".into(),
        negative: String::new(),
        width: 1024,
        height: 1024,
        seed: 0,
        count: 1,
        source: None,
        mask: None,
        refs: vec![],
        strength: 0.6,
    };
    rt(|r| r.cloud_pending = Some(CloudRun { c, mode: Mode::Create, seed: None, base, target: None, images: 3, dont_ask: false }));
    let mut first = true;
    let mut h = egui_kittest::Harness::builder().with_size(vec2(700.0, 500.0)).build_ui_state(
        move |ui, app: &mut PhotocraftApp| {
            if first {
                PhotocraftApp::setup_context(ui.ctx(), crate::theme::ThemeKind::Pro);
                first = false;
                ui.ctx().request_repaint();
                return;
            }
            cloud_confirm_window(app, ui.ctx());
        },
        PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default()),
    );
    h.run_steps(4);
    h.get_by_label("Generate with OpenAI?");
    h.get_by_label("Images: 3");
    h.get_by_label("Don't ask again this session");
    h.get_by_label("Cancel").click();
    h.run_steps(3);
    assert!(rt(|r| r.cloud_pending.is_none()));
    assert!(!rt(|r| r.cloud_skip_confirm));
    assert!(h.state().session.jobs().is_empty());
}

#[test]
fn gallery_click_selects_while_thumbnail_decodes_off_thread() {
    use egui_kittest::kittest::Queryable;
    let entry = library().add(&RgbaImage::from_pixel(401, 203, image::Rgba([80, 90, 100, 255])), "gpu-resource-gallery-fixture", None, None).unwrap();
    let id = entry.id.clone();
    let accessible_name = entry.name.clone();
    rt(|r| r.selected = None);
    let items = vec![Some(entry)];
    let mut first = true;
    let mut h = egui_kittest::Harness::builder().with_size(vec2(400.0, 240.0)).build_ui_state(
        move |ui, app: &mut PhotocraftApp| {
            if first {
                PhotocraftApp::setup_context(ui.ctx(), crate::theme::ThemeKind::Pro);
                first = false;
                ui.ctx().request_repaint();
                return;
            }
            tile_grid(app, ui, &items, &[], "async-gallery-test");
        },
        PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default()),
    );
    h.run_steps(4);
    h.get_by_label(&accessible_name).click();
    h.run_steps(2);
    assert_eq!(rt(|r| r.selected.clone()), Some(id.clone()));
    let start = Instant::now();
    while !rt(|r| r.gallery_thumbs.loaded(&id)) {
        assert!(start.elapsed() < Duration::from_secs(10));
        std::thread::sleep(Duration::from_millis(10));
        h.run_steps(1);
    }
    library().delete(std::slice::from_ref(&id)).unwrap();
    rt(|r| r.gallery_thumbs.remove(&id));
}

#[test]
fn enhance_generate_upscale_clicks_three_methods_and_scale() {
    use egui_kittest::kittest::Queryable;
    let _lock = crate::ai_ui::AI_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    rt(|r| r.upscale_any = false);
    let mut session = photocraft_engine::Session::new();
    session.execute("file.new", json!({"width": 32, "height": 24})).unwrap();
    let mut app = PhotocraftApp::new(session, crate::Services::default());
    app.ui.ai.generate.mode = Mode::Upscale;
    app.ui.ai.generate.model = ModelId::SeedVr2.key().into();
    let mut initialized = false;
    let mut h = egui_kittest::Harness::builder().with_size(vec2(620.0, 900.0)).build_ui_state(
        move |ui, app: &mut PhotocraftApp| {
            if !initialized {
                PhotocraftApp::setup_context(ui.ctx(), crate::theme::ThemeKind::Studio);
                initialized = true;
                ui.ctx().request_repaint();
                return;
            }
            panel(app, ui);
        },
        app,
    );
    h.run_steps(3);
    h.get_by_label("Faithful").click();
    h.run_steps(2);
    assert_eq!(h.state().ui.ai.generate.enhance_method, li_ai::enhance::Method::Faithful);
    h.get_by_label("4×").click();
    h.run_steps(2);
    assert_eq!(h.state().ui.ai.generate.upscale, 4.0);
    h.get_by_label("Rescue").click();
    h.run_steps(2);
    assert_eq!(h.state().ui.ai.generate.enhance_method, li_ai::enhance::Method::Rescue);
    h.get_by_label("Generative (SeedVR2)").click();
    h.run_steps(2);
    assert_eq!(h.state().ui.ai.generate.enhance_method, li_ai::enhance::Method::Generative);
    assert_eq!(serde_json::from_value::<GenerateState>(json!({})).unwrap().enhance_method, li_ai::enhance::Method::Generative);
}

#[test]
fn selected_layer_refine_is_disabled_without_a_selected_layer() {
    use egui_kittest::kittest::Queryable;
    crate::ai_ui::TEST_STATUS.with(|s| {
        *s.borrow_mut() = Some(crate::ai_ui::EngineStatus {
            connected: true,
            checked: true,
            presets: catalog::presets().iter().map(|p| (p.id(), Ok(()))).collect(),
            ..Default::default()
        })
    });
    let mut session = photocraft_engine::Session::new();
    session.execute("file.new", json!({"width":64,"height":64})).unwrap();
    session.active_mut().unwrap().active_layer = None;
    let mut app = PhotocraftApp::new(session, Default::default());
    app.ui.ai.generate.mode = Mode::Refine;
    app.ui.ai.generate.refine_input = RefineInput::SelectedLayer;
    let mut initialized = false;
    let mut h = egui_kittest::Harness::builder().with_size(vec2(420.0, 1600.0)).build_ui_state(
        move |ui, app: &mut PhotocraftApp| {
            if !initialized {
                PhotocraftApp::setup_context(ui.ctx(), crate::theme::ThemeKind::Studio);
                initialized = true;
                return;
            }
            panel(app, ui);
        },
        app,
    );
    h.run_steps(4);
    h.get_all_by_label("Refine").last().unwrap().click();
    h.run_steps(3);
    assert!(h.state().session.jobs().is_empty());
    let ctx = h.ctx.clone();
    start(h.state_mut(), &ctx);
    assert_eq!(h.state().ui.status, "Select a layer first.");
    assert!(h.state().session.jobs().is_empty());
}

#[test]
fn refine_model_groups_exclude_upscalers_tools_and_video() {
    let mut models = catalog::catalog().models.clone();
    // Detected video/template-only models are represented by FamilyKind::Basic.
    let mut video = ModelId::Klein4B.info().clone();
    video.id = ModelId::intern("video:refine-picker-test");
    video.resolved.kind = li_ai::family::FamilyKind::Basic;
    models.push(video);
    let mut upscale = ModelId::Klein4B.info().clone();
    upscale.id = ModelId::intern("upscale:refine-picker-test");
    upscale.upscale = true;
    models.push(upscale);
    let groups = model_groups(&models, Mode::Refine);
    assert_eq!(groups.len(), 2);
    assert_eq!(groups[0].0, "Edit models — refine by reference");
    assert_eq!(groups[1].0, "Image models — refine by denoise");
    let ids = |group: usize| groups[group].1.iter().map(|m| m.id).collect::<Vec<_>>();
    assert!(ids(0).contains(&ModelId::Klein9B));
    assert!(ids(0).contains(&ModelId::Qwen));
    assert!(ids(1).contains(&ModelId::ZImageTurbo));
    for excluded in [ModelId::SeedVr2, ModelId::KleinRemove, ModelId::intern("video:refine-picker-test"), ModelId::intern("upscale:refine-picker-test")] {
        assert!(!ids(0).contains(&excluded) && !ids(1).contains(&excluded), "{excluded:?}");
    }
}

#[test]
fn refine_panel_mode_and_second_pass_use_model_defaults_and_grouped_picker() {
    use egui::accesskit::Role;
    use egui_kittest::kittest::Queryable;
    let _lock = crate::ai_ui::AI_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut session = photocraft_engine::Session::new();
    session.execute("file.new", json!({"width": 64, "height": 64})).unwrap();
    let app = PhotocraftApp::new(session, crate::Services::default());
    assert_eq!(app.ui.ai.generate.refine_strength, 1.0);
    crate::ai_ui::TEST_STATUS.with(|s| {
        *s.borrow_mut() = Some(crate::ai_ui::EngineStatus {
            connected: true,
            checked: true,
            presets: catalog::presets().iter().map(|p| (p.id(), Ok(()))).collect(),
            ..Default::default()
        });
    });
    let mut initialized = false;
    let mut h = egui_kittest::Harness::builder().with_size(vec2(420.0, 1800.0)).with_max_steps(64).build_ui_state(
        move |ui, app: &mut PhotocraftApp| {
            if !initialized {
                PhotocraftApp::setup_context(ui.ctx(), crate::theme::ThemeKind::Studio);
                initialized = true;
                ui.ctx().request_repaint();
                return;
            }
            panel(app, ui);
        },
        app,
    );
    h.run_steps(3);
    h.get_by_role_and_label(Role::Button, "Refine").click();
    h.run_steps(3);
    assert_eq!(h.state().ui.ai.generate.denoise, 1.0);
    h.get_by_label("Refines by reference: the image guides the result");
    h.get_by_value(&ModelId::Klein4B.info().label).click();
    h.run_steps(2);
    h.get_by_label("Edit models — refine by reference");
    h.get_by_label("Image models — refine by denoise");
    for excluded in [ModelId::SeedVr2, ModelId::KleinRemove] {
        assert!(h.query_by_role_and_label(Role::Button, &excluded.info().label).is_none());
    }
    h.get_by_role_and_label(Role::Button, &ModelId::ZImageTurbo.info().label).click();
    h.run_steps(3);
    assert_eq!(h.state().ui.ai.generate.denoise, 0.35);
    h.get_by_label("Refines by denoise: lower keeps more of the image");
    h.state_mut().ui.ai.generate.denoise = 0.72;
    h.get_by_value(&ModelId::ZImageTurbo.info().label).click();
    h.run_steps(2);
    h.get_by_role_and_label(Role::Button, &ModelId::Klein9B.info().label).click();
    h.run_steps(3);
    assert_eq!(h.state().ui.ai.generate.denoise, 1.0);
    h.get_by_role_and_label(Role::Button, "Create").click();
    h.run_steps(3);
    h.get_by_label("Refine with").click();
    h.run_steps(2);
    h.get_by_value("Choose a model").click();
    h.run_steps(2);
    h.get_by_role_and_label(Role::Button, &ModelId::Klein9B.info().label).click();
    h.run_steps(3);
    assert_eq!(h.state().ui.ai.generate.refine_strength, 1.0);
    h.state_mut().ui.ai.generate.refine_strength = 0.72;
    h.get_all_by_value(&ModelId::Klein9B.info().label).last().unwrap().click();
    h.run_steps(2);
    h.get_by_role_and_label(Role::Button, &ModelId::ZImageTurbo.info().label).click();
    h.run_steps(3);
    assert_eq!(h.state().ui.ai.generate.refine_strength, 0.35);
    h.get_by_label("Refines by denoise: lower keeps more of the image");
    // Missing preset strengths and non-finite request values also use the selected model.
    let s = &mut h.state_mut().ui.ai.generate;
    let mut preset = li_ai::presets::GenPreset { refine_model: Some(ModelId::Klein9B.key().into()), ..Default::default() };
    apply_preset(s, &preset);
    assert_eq!(s.refine_strength, 1.0);
    s.refine_strength = f32::NAN;
    let (req, _, _) = local_request(s, "detail".into(), None, Vec::new()).unwrap();
    assert_eq!(req.refine.unwrap().strength, 1.0);
    preset.refine_strength = Some(0.6);
    apply_preset(s, &preset);
    assert_eq!(s.refine_strength, 0.6, "an explicit saved strength stays editable");
}

#[test]
fn steps_and_guidance_follow_the_backend_limits_only() {
    let limits = li_ai::comfy::SamplerLimits { steps: (1.0, 10000.0), cfg: (0.0, 100.0), denoise: (0.0, 1.0) };
    // well outside every model's recommendation, but inside what ComfyUI accepts: kept as typed
    let mut s = GenerateState { steps: 150, guidance: 25.0, ..Default::default() };
    fit_to_backend(&mut s, Some(limits));
    assert_eq!((s.steps, s.guidance), (150, 25.0));
    s.steps = 20000;
    s.guidance = 500.0;
    fit_to_backend(&mut s, Some(limits));
    assert_eq!((s.steps, s.guidance), (10000, 100.0));
    s.steps = 0;
    fit_to_backend(&mut s, None);
    assert_eq!(s.steps, 1);
    let real = li_ai::comfy::ObjectInfo(serde_json::json!({"KSampler": {"input": {"required": {
            "steps": ["INT", {"default": 20, "min": 1, "max": 10000}], "cfg": ["FLOAT", {"default": 8.0, "min": 0.0, "max": 100.0}],
            "denoise": ["FLOAT", {"default": 1.0, "min": 0.0, "max": 1.0}]}}}}));
    assert_eq!(real.sampler_limits(), Some(limits));
}

#[test]
fn refine_with_lora_picker_and_precision_carry_the_second_pass_settings() {
    use egui_kittest::kittest::Queryable;
    let _lock = crate::ai_ui::AI_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    li_ai::inventory::refresh(
        &li_ai::comfy::ObjectInfo(
            json!({"LoraLoader": {"input": {"required": {"lora_name": [["qwen_image_2.1_style.safetensors", "flux_realism_lora.safetensors"]]}}}}),
        ),
        None,
    );
    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            li_ai::inventory::refresh(&li_ai::comfy::ObjectInfo(json!({})), None);
        }
    }
    let _reset = Reset;
    let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
    app.ui.ai.generate.prompt = "a castle".into();
    let st = crate::ai_ui::EngineStatus {
        connected: true,
        checked: true,
        presets: catalog::presets().iter().map(|p| (p.id(), Ok(()))).collect(),
        ..Default::default()
    };
    crate::ai_ui::TEST_STATUS.with(|s| *s.borrow_mut() = Some(st));
    // Render the real panel with enough room for the expanded second-pass controls.
    let mut initialized = false;
    let mut h = egui_kittest::Harness::builder().with_size(vec2(420.0, 1600.0)).with_max_steps(64).build_ui_state(
        move |ui, app: &mut PhotocraftApp| {
            if !initialized {
                PhotocraftApp::setup_context(ui.ctx(), crate::theme::ThemeKind::Studio);
                initialized = true;
                ui.ctx().request_repaint();
                return;
            }
            panel(app, ui);
        },
        app,
    );
    h.run_steps(3);
    // The panel exposes the same size component as the Refinement workspace.
    h.get_by_role_and_label(egui::accesskit::Role::Button, "4:3").click();
    h.run_steps(2);
    assert_eq!(h.state().ui.ai.generate.aspect, "4:3");
    assert!(h.state().ui.ai.generate.aspect_locked);
    let grid = model_of(&h.state().ui.ai.generate).info().resolved.sizes.multiple.max(8);
    assert_eq!((h.state().ui.ai.generate.width % grid, h.state().ui.ai.generate.height % grid), (0, 0));
    h.get_by_label("Link width and height").click();
    h.run_steps(2);
    assert!(!h.state().ui.ai.generate.aspect_locked);
    h.get_by_label("Refine with").click();
    h.run_steps(2);
    h.get_by_value("Choose a model").click();
    h.run_steps(2);
    h.get_by_role_and_label(egui::accesskit::Role::Button, &ModelId::Qwen.info().label).click();
    h.run_steps(3);
    let current_variant = h.state().ui.ai.generate.refine_variant.clone();
    let current_label = catalog::presets_for(ModelId::Qwen).find(|p| p.variant == current_variant).unwrap().label.clone();
    let alternate = catalog::presets_for(ModelId::Qwen).find(|p| p.variant != current_variant).unwrap();
    h.get_all_by_value(&current_label).last().unwrap().click();
    h.run_steps(2);
    h.get_by_role_and_label(egui::accesskit::Role::Button, &alternate.label).click();
    h.run_steps(2);
    assert_eq!(h.state().ui.ai.generate.refine_variant, alternate.variant);
    h.get_all_by_label("+ Add").last().unwrap().click();
    h.run_steps(2);
    assert!(h.query_by_label("flux realism lora").is_none());
    h.get_by_label("qwen image 2.1 style").click();
    h.run_steps(3);
    let s = &h.state().ui.ai.generate;
    assert!(s.loras.is_empty());
    assert_eq!(s.refine_loras, vec![LoraPick { name: "qwen_image_2.1_style.safetensors".into(), strength: 0.8 }]);
    let (req, _, _) = local_request(s, s.prompt.clone(), None, Vec::new()).unwrap();
    let refine = req.refine.unwrap();
    assert_eq!(refine.variant, alternate.variant);
    assert_eq!(refine.loras, vec![li_ai::workflows::LoraUse { name: "qwen_image_2.1_style.safetensors".into(), strength: 0.8 }]);
    let steps = h.get_by_role_and_label(egui::accesskit::Role::SpinButton, "Refine steps").rect();
    h.hover_at(steps.center());
    h.event(egui::Event::PointerButton { pos: steps.center(), button: egui::PointerButton::Primary, pressed: true, modifiers: Default::default() });
    h.step();
    h.event(egui::Event::PointerMoved(steps.center() + vec2(20.0, 0.0)));
    h.step();
    h.event(egui::Event::PointerButton {
        pos: steps.center() + vec2(20.0, 0.0),
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers: Default::default(),
    });
    h.run_steps(2);
    let s = &mut h.state_mut().ui.ai.generate;
    assert!(s.refine_steps.is_some());
    let (req, _, _) = local_request(s, s.prompt.clone(), None, Vec::new()).unwrap();
    assert_eq!(req.refine.unwrap().steps, s.refine_steps);
    s.refine_strength = f32::NAN;
    let (req, _, _) = local_request(s, s.prompt.clone(), None, Vec::new()).unwrap();
    assert_eq!(req.refine.unwrap().strength, catalog::refine_default_denoise(ModelId::Qwen), "a second-pass fallback follows its model");
    let old: GenerateState = serde_json::from_value(json!({"refine": true})).unwrap();
    assert!(old.refine_loras.is_empty());
}

#[test]
fn natural_order() {
    let mut v = vec!["img10.jpg", "img2.jpg", "IMG1.jpg", "a.png"];
    v.sort_by(|a, b| natural_cmp(a, b));
    assert_eq!(v, vec!["a.png", "IMG1.jpg", "img2.jpg", "img10.jpg"]);
}

#[test]
fn aspect_sizes_are_about_a_megapixel_on_the_grid() {
    let s = GenerateState::default();
    for (k, r) in ASPECTS {
        let (w, h) = size_for(k, &s, 1024);
        assert!(w % 16 == 0 && h % 16 == 0);
        assert!(((w as f32 / h as f32) - r).abs() < 0.05, "{k}: {w}x{h}");
        assert!((800_000..1_300_000).contains(&(w * h)), "{k}: {w}x{h}");
    }
}

/// Upscale lists the official upscalers (SeedVR2) and routes them to AI Enhance; "Choose
/// another model…" switches to every image model (diffusion refine-upscale).
#[test]
fn upscale_offers_the_official_upscalers_first() {
    let official: Vec<&str> = catalog::catalog().models.iter().filter(|m| fits_mode_with(m, Mode::Upscale, false)).map(|m| m.id.key()).collect();
    assert_eq!(official, vec![ModelId::SeedVr2.key(), "realplksr-2x", "realplksr-4x", "bsrgan-2x", "bsrgan-4x"]);
    let any: Vec<ModelId> = catalog::catalog().models.iter().filter(|m| fits_mode_with(m, Mode::Upscale, true)).map(|m| m.id).collect();
    assert!(any.contains(&ModelId::Klein4B) && any.contains(&ModelId::Qwen) && !any.contains(&ModelId::SeedVr2), "{any:?}");
    // other modes never offer the upscaler
    for mode in [Mode::Create, Mode::Edit, Mode::Fill, Mode::Refine] {
        assert!(!fits_mode_with(ModelId::SeedVr2.info(), mode, false), "{mode:?}");
    }
    assert!(runs_as_enhance(ModelId::SeedVr2.info(), Mode::Upscale));
    assert!(!runs_as_enhance(ModelId::Klein4B.info(), Mode::Upscale));
    assert!(!runs_as_enhance(ModelId::SeedVr2.info(), Mode::Refine));
    // the picker's filter follows the session's choice
    assert!(!upscale_any());
    assert!(!fits_mode(ModelId::Klein4B.info(), Mode::Upscale));
    rt(|r| r.upscale_any = true);
    assert!(fits_mode(ModelId::Klein4B.info(), Mode::Upscale));
    rt(|r| r.upscale_any = false);
}

/// The model list holds models only: LoRA-capable models carry no "LoRA" chip.
#[test]
fn model_tags_do_not_call_models_loras() {
    for m in &catalog::catalog().models {
        assert!(!m.tags().iter().any(|t| t.contains("LoRA")), "{}: {:?}", m.label, m.tags());
    }
}

/// The Styles (LoRA) row always offers the LoRA browser, opened on the model's family.
#[test]
fn styles_row_opens_the_lora_browser() {
    use egui_kittest::kittest::Queryable;
    let fam = ModelId::Klein4B.info().resolved.clone();
    let mut initialized = false;
    let mut h = egui_kittest::Harness::builder().with_size(egui::vec2(360.0, 200.0)).build_ui_state(
        move |ui, s: &mut GenerateState| {
            if !initialized {
                PhotocraftApp::setup_context(ui.ctx(), crate::theme::ThemeKind::Studio);
                initialized = true;
                ui.ctx().request_repaint();
                return;
            }
            let t = Tokens::get(ui.ctx());
            loras_ui(ui, &mut s.loras, &fam, &t);
        },
        GenerateState::default(),
    );
    h.run_steps(2);
    crate::model_browser::close();
    h.get_by_label("Browse LoRAs…").click();
    h.run_steps(2);
    assert!(crate::model_browser::is_open());
    assert_eq!(crate::model_browser::showing(), (true, Some(ModelId::Klein4B.info().resolved.id.clone())));
    crate::model_browser::close();
    // and from the model picker's footer, for a model of any family
    browse_loras("no-such-model");
    assert!(crate::model_browser::showing().0);
    crate::model_browser::close();
}

#[test]
fn refine_preset_persists_custom_sampler_and_loads_old_defaults() {
    let mut state = GenerateState { refine: true, refine_steps: Some(17), refine_guidance: Some(2.5), ..Default::default() };
    save_preset(&mut state, "DS refine regression");
    let saved = li_ai::presets::all(&preset_path()).into_iter().find(|p| p.name == "DS refine regression").unwrap();
    assert_eq!(saved.refine_steps, Some(17));
    assert_eq!(saved.refine_guidance, Some(2.5));
    let mut loaded = GenerateState::default();
    apply_preset(&mut loaded, &saved);
    assert_eq!((loaded.refine_steps, loaded.refine_guidance), (Some(17), Some(2.5)));
    let old: li_ai::presets::GenPreset = serde_json::from_value(json!({"name":"old","refine_model":"flux2-klein-4b"})).unwrap();
    apply_preset(&mut loaded, &old);
    assert_eq!((loaded.refine_steps, loaded.refine_guidance), (None, None));
}

#[test]
fn library_snapshots_share_entries_until_removal() {
    let entry = li_ai::library::Entry {
        id: "fixture".into(),
        name: "large metadata".repeat(100),
        created_at: 0.,
        width: 1,
        height: 1,
        generation: Some(json!({"prompt":"prompt".repeat(100)})),
        reference_attributions: vec![],
        sha256: "hash".into(),
        upscale: None,
    };
    rt(|r| {
        r.library = Arc::new(vec![entry; 10_000]);
        r.library_loaded = Some(Instant::now());
    });
    let a = rt(|r| r.library.clone());
    for _ in 0..30 {
        let b = rt(|r| r.library.clone());
        assert!(Arc::ptr_eq(&a, &b));
        assert_eq!(a.as_ptr(), b.as_ptr());
    }
    rt(|r| {
        Arc::make_mut(&mut r.library).remove(0);
    });
    assert_eq!(a.len(), 10_000);
    assert_eq!(rt(|r| r.library.len()), 9_999);
    rt(|r| r.library = Arc::new(vec![]));
}

#[test]
fn refine_lora_preset_roundtrip_and_invalid_step_limits() {
    use egui_kittest::kittest::Queryable;
    crate::ai_ui::TEST_STATUS.with(|s| {
        *s.borrow_mut() = Some(crate::ai_ui::EngineStatus {
            connected: true,
            checked: true,
            presets: catalog::presets().iter().map(|p| (p.id(), Ok(()))).collect(),
            ..Default::default()
        })
    });
    let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), Default::default());
    app.ui.ai.generate.refine = true;
    app.ui.ai.generate.refine_loras = vec![LoraPick { name: "refine-style.safetensors".into(), strength: 0.65 }];
    let mut initialized = false;
    let mut h = egui_kittest::Harness::builder().with_size(vec2(420., 1800.)).build_ui_state(
        move |ui, app: &mut PhotocraftApp| {
            if !initialized {
                PhotocraftApp::setup_context(ui.ctx(), Default::default());
                initialized = true;
                return;
            }
            panel(app, ui);
        },
        app,
    );
    h.run_steps(4);
    let at = h.ctx.data(|d| d.get_temp::<egui::Rect>(egui::Id::new("gen-save-preset-test"))).unwrap().center();
    h.hover_at(at);
    h.step();
    h.event(egui::Event::PointerButton { pos: at, button: egui::PointerButton::Primary, pressed: true, modifiers: egui::Modifiers::NONE });
    h.step();
    h.event(egui::Event::PointerButton { pos: at, button: egui::PointerButton::Primary, pressed: false, modifiers: egui::Modifiers::NONE });
    h.step();
    h.run_steps(3);
    h.get_by_label("Save Preset").click();
    h.run_steps(3);
    let saved = li_ai::presets::all(&preset_path()).into_iter().find(|p| p.name == h.state().ui.ai.generate.preset).unwrap();
    assert_eq!(saved.refine_loras.len(), 1);
    assert_eq!(saved.refine_loras[0].name, "refine-style.safetensors");
    let mut loaded = GenerateState::default();
    apply_preset(&mut loaded, &saved);
    assert_eq!(loaded.refine_loras[0].strength, 0.65);
    let old = serde_json::from_value(json!({"name":"old"})).unwrap();
    apply_preset(&mut loaded, &old);
    assert!(loaded.refine_loras.is_empty());
    fit_to_backend(&mut loaded, Some(li_ai::comfy::SamplerLimits { steps: (20., 4.), cfg: (0., 10.), denoise: (0., 1.) }));
    assert_eq!(loaded.steps, 20);
    crate::ai_ui::TEST_STATUS.with(|s| *s.borrow_mut() = None);
}

#[test]
fn zimage_variety_settings_presets_and_requests_keep_old_defaults() {
    let old: GenerateState = serde_json::from_value(json!({"model":"z-image-turbo"})).unwrap();
    assert_eq!(old.variety, 0.0);
    let old_draft: DraftRefineSettings = serde_json::from_value(json!({"draft":{"model":"z-image-turbo"}})).unwrap();
    assert_eq!(old_draft.draft.variety, 0.0);
    let mut s = GenerateState { variety: 75.0, ..old };
    let restored: GenerateState = serde_json::from_value(serde_json::to_value(&s).unwrap()).unwrap();
    assert_eq!(restored.variety, 75.0);
    assert_eq!(local_request(&s, "a fox".into(), None, Vec::new()).unwrap().0.variety, 75.0);
    s.variety = f32::NAN;
    assert_eq!(local_request(&s, "a fox".into(), None, Vec::new()).unwrap().0.variety, 0.0);
    s.variety = 75.0;
    set_model(&mut s, ModelId::Ernie);
    assert_eq!(s.variety, 0.0);
    s.variety = 75.0;
    assert_eq!(local_request(&s, "a fox".into(), None, Vec::new()).unwrap().0.variety, 0.0);
    let old_preset: li_ai::presets::GenPreset = serde_json::from_value(json!({"model":"z-image-turbo"})).unwrap();
    apply_preset(&mut s, &old_preset);
    assert_eq!(s.variety, 0.0);
    let preset = li_ai::presets::GenPreset { variety: Some(75.0), ..old_preset };
    let restored: li_ai::presets::GenPreset = serde_json::from_value(serde_json::to_value(&preset).unwrap()).unwrap();
    apply_preset(&mut s, &restored);
    assert_eq!(s.variety, 75.0);
    s.mode = Mode::Refine;
    let source = RgbaImage::new(64, 64);
    assert_eq!(local_request(&s, "a fox".into(), Some((photocraft_doc::DocId(1), source, None)), Vec::new()).unwrap().0.variety, 0.0);
}

#[test]
fn restored_upscale_keeps_non_official_model_choice() {
    for model in [ModelId::Klein4B.key(), "custom:saved-workflow", "cloud:saved-model"] {
        rt(|r| r.upscale_any = false);
        let state = GenerateState { mode: Mode::Upscale, model: model.into(), ..Default::default() };
        restore_upscale_choice(&state);
        assert!(upscale_any());
    }
    rt(|r| r.upscale_any = false);
    restore_upscale_choice(&GenerateState { mode: Mode::Upscale, model: ModelId::SeedVr2.key().into(), ..Default::default() });
    assert!(!upscale_any());
}

#[test]
fn unsupported_recipe_picker_explains_and_can_clear_styles() {
    use egui_kittest::kittest::Queryable;
    let mut initialized = false;
    let mut h = egui_kittest::Harness::builder().with_size(vec2(500.0, 200.0)).build_ui_state(
        move |ui, picks: &mut Vec<LoraPick>| {
            if !initialized {
                PhotocraftApp::setup_context(ui.ctx(), crate::theme::ThemeKind::Studio);
                initialized = true;
                ui.ctx().request_repaint();
                return;
            }
            refine_recipe_loras_ui(ui, picks, None, &Tokens::get(ui.ctx()));
        },
        vec![LoraPick { name: "flux-2-klein-style.safetensors".into(), strength: 0.7 }],
    );
    h.run_steps(3);
    h.get_by_label("This refinement recipe does not support styles (LoRAs).");
    assert_eq!(h.state().len(), 1, "discovery must not silently discard a chosen style");
    h.get_by_label("Clear styles").click();
    h.run_steps(3);
    assert!(h.state().is_empty());
}

#[test]
fn history_persists_effective_sampling_and_replays_it_explicitly() {
    let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
    app.session.execute("file.new", json!({"width":32,"height":24})).unwrap();
    let layer = app.session.active().unwrap().active_layer.unwrap();
    let image = RgbaImage::from_pixel(32, 24, image::Rgba([35, 86, 119, 255]));
    let expected = image.clone();
    let meta =
        json!({"model":ModelId::Klein4B.key(), "variant":"bf16", "mode":"refine", "prompt":"refine", "seed":42, "steps":4, "guidance":1.0, "denoise":1.0});
    assert!(launch_resolved(&mut app, "Refinement", "Refinement", "review-effective".into(), meta, None, false, move |_| {
        Ok((image, json!({"steps":8,"guidance":2.0,"denoise":0.35,"sampler":"euler","scheduler":"simple","recipe":"template:verified","width":32,"height":24})))
    }));
    let until = Instant::now() + Duration::from_secs(5);
    let entry = loop {
        if let Some(event) = app.session.poll_jobs().into_iter().next() {
            on_generated(&mut app, &event);
            let JobOutcome::Done(value) = event.outcome else { panic!("{event:?}") };
            break serde_json::from_value::<Entry>(value).unwrap();
        }
        assert!(Instant::now() < until, "generation did not finish");
        std::thread::sleep(Duration::from_millis(5));
    };
    assert_eq!(library().load(&entry).unwrap(), expected);
    let persisted = library().list().into_iter().find(|e| e.id == entry.id).unwrap();
    let generation = persisted.generation.as_ref().unwrap();
    assert_eq!(generation["denoise"], 0.35);
    assert_eq!(generation["steps"], 8);
    assert_eq!(generation["recipe"], "template:verified");
    regenerate_layer(&mut app, &egui::Context::default(), layer.0, generation, true).unwrap();
    let state = &app.ui.ai.generate;
    assert_eq!((state.steps, state.guidance, state.denoise), (8, 2.0, 0.35));
    assert_eq!(state.refine_defaults, [false; 3]);
    assert_eq!((&*state.sampler, &*state.scheduler), ("euler", "simple"));
    let req = local_request(state, state.prompt.clone(), Some((app.session.active().unwrap().doc.id, expected, None)), vec![]).unwrap().0;
    assert_eq!(req.refine_defaults, [false; 3]);
    assert_eq!(req.denoise, 0.35);
}

#[test]
fn history_replay_preserves_the_resolved_second_pass() {
    let defaults = GenerateState {
        refine: true,
        refine_model: ModelId::Klein9B.key().into(),
        refine_variant: catalog::default_variant(ModelId::Klein9B).into(),
        ..Default::default()
    };
    let automatic = local_request(&defaults, "a house".into(), None, vec![]).unwrap().0.refine.unwrap();
    assert_eq!((automatic.steps, automatic.guidance), (None, None), "recipe defaults must be resolved on the worker");
    assert!(automatic.default_strength);
    let legacy: GenerateState = serde_json::from_value(json!({"steps":20,"guidance":8.0,"denoise":0.35,"refine_strength":0.35})).unwrap();
    assert_eq!(legacy.refine_defaults, [false; 3], "older saved numbers remain explicit");
    assert!(!legacy.refine_strength_default);
    let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
    app.session.execute("file.new", json!({"width":32,"height":24})).unwrap();
    let layer = app.session.active().unwrap().active_layer.unwrap();
    let meta = json!({"model":ModelId::Klein4B.key(),"mode":"create","prompt":"a house","steps":4,"guidance":1.0,"denoise":0.6,
        "refine":{"model":ModelId::Klein9B.key(),"variant":"fp8","strength":0.25,"steps":12,"guidance":5.0,"scale":1.5,"loras":[{"name":"style.safetensors","strength":0.7}]}});
    regenerate_layer(&mut app, &egui::Context::default(), layer.0, &meta, true).unwrap();
    let state = &app.ui.ai.generate;
    let req = local_request(state, state.prompt.clone(), None, vec![]).unwrap().0;
    let step = req.refine.unwrap();
    assert_eq!(step.model, ModelId::Klein9B);
    assert_eq!(step.variant, "fp8");
    assert_eq!((step.steps, step.guidance, step.strength, step.scale), (Some(12), Some(5.0), 0.25, 1.5));
    assert!(!step.default_strength);
    assert_eq!(step.loras[0].name, "style.safetensors");
    assert_eq!(step.loras[0].strength, 0.7);
}

#[test]
fn second_pass_typing_nominal_values_marks_them_explicit() {
    use egui_kittest::kittest::Queryable;
    let _lock = crate::ai_ui::AI_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    crate::ai_ui::TEST_STATUS.with(|s| *s.borrow_mut() = Some(crate::ai_ui::EngineStatus { checked: true, ..Default::default() }));
    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            crate::ai_ui::TEST_STATUS.with(|s| *s.borrow_mut() = None);
        }
    }
    let _reset = Reset;
    let model = ModelId::Klein9B;
    let steps = model.info().steps.default as u32;
    let guidance = model.info().guidance.default;
    let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
    app.ui.ai.generate.refine = true;
    app.ui.ai.generate.refine_model = model.key().into();
    app.ui.ai.generate.refine_variant = catalog::default_variant(model).into();
    let mut initialized = false;
    let mut h = egui_kittest::Harness::builder().with_size(vec2(620.0, 1600.0)).build_ui_state(
        move |ui, app: &mut PhotocraftApp| {
            if !initialized {
                PhotocraftApp::setup_context(ui.ctx(), crate::theme::ThemeKind::Studio);
                initialized = true;
                ui.ctx().request_repaint();
                return;
            }
            panel(app, ui);
        },
        app,
    );
    h.run_steps(3);
    assert_eq!((h.state().ui.ai.generate.refine_steps, h.state().ui.ai.generate.refine_guidance), (None, None));
    h.get_by_role_and_label(egui::accesskit::Role::SpinButton, "Refine steps").click();
    h.run_steps(2);
    h.event(egui::Event::Text(steps.to_string()));
    h.run_steps(2);
    h.get_by_role_and_label(egui::accesskit::Role::SpinButton, "Refine guidance").click();
    h.run_steps(2);
    h.event(egui::Event::Text(guidance.to_string()));
    h.run_steps(2);
    let state = &h.state().ui.ai.generate;
    assert_eq!((state.refine_steps, state.refine_guidance), (Some(steps), Some(guidance)));
    let step = local_request(state, "a house".into(), None, vec![]).unwrap().0.refine.unwrap();
    assert_eq!((step.steps, step.guidance), (Some(steps), Some(guidance)));
}

#[test]
fn cancelled_prepared_generation_never_enters_library() {
    let _lock = crate::ai_ui::AI_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
    let name = format!("cancelled-prepared-{}", std::process::id());
    assert!(launch_resolved(&mut app, "Generation", "Generation", name.clone(), json!({}), None, false, |_| {
        Ok((RgbaImage::from_pixel(16, 16, image::Rgba([30, 70, 90, 255])), Value::Null))
    }));
    let id = app.session.jobs().into_iter().find(|j| j.command == GENERATE_JOB).unwrap().id;
    let until = Instant::now() + Duration::from_secs(5);
    loop {
        let job = app.session.job(id).unwrap();
        if job.progress >= 0.99 {
            break;
        }
        assert!(Instant::now() < until, "generation was not prepared");
        std::thread::sleep(Duration::from_millis(2));
    }
    assert!(!library().list().iter().any(|e| e.name.starts_with(&name)), "worker published before the apply step");
    crate::ai_ui::stop_generation(&mut app, &[id]);
    app.session.join_cancelled_jobs();
    app.session.poll_jobs();
    assert!(!library().list().iter().any(|e| e.name.starts_with(&name)), "cancelled result was published");
}

#[test]
fn generated_edit_returns_to_the_document_the_user_is_viewing() {
    let _lock = crate::ai_ui::AI_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
    app.run("file.new", json!({"width": 8, "height": 8})).unwrap();
    let target = app.session.active().unwrap().doc.id.0;
    let layers = app.session.active().unwrap().doc.layers.len();
    app.run("file.new", json!({"width": 8, "height": 8})).unwrap();
    let viewing = app.session.active().unwrap().doc.clone();
    let revision = app.session.active().unwrap().revision;
    app.ui.tool = crate::state::Tool::Lasso;
    crate::lasso_ui::pointer(&mut app, crate::canvas::ToolEvent::Down { x: 1.0, y: 1.0, pressure: 1.0 }, egui::Modifiers::NONE);
    assert!(crate::lasso_ui::active(&app));
    let entry = library().add(&RgbaImage::from_pixel(8, 8, image::Rgba([30, 70, 90, 255])), "active-tab-regression", None, None).unwrap();
    place_into(&mut app, target, &entry);
    assert_eq!(app.session.active().unwrap().doc.id, viewing.id);
    assert!(std::sync::Arc::ptr_eq(&app.session.active().unwrap().doc, &viewing));
    assert_eq!(app.session.active().unwrap().revision, revision);
    assert_eq!(app.session.documents()[0].doc.layers.len(), layers + 1);
    assert!(crate::lasso_ui::active(&app), "background completion cancelled the viewed document's gesture");
    assert!(!app.ui.status_error, "{}", app.ui.status);
    library().delete(&[entry.id]).unwrap();
}

#[cfg(test)]
mod bg_batch_tests {
    use super::*;
    use photocraft_engine::batch_io::batch_destination;

    fn directory() -> PathBuf {
        let p = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/bgremove-tests").join(li_ai::ops::new_seed().to_string());
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn folder_uses_shared_readable_registry() -> Result<(), Box<dyn std::error::Error>> {
        let dir = std::env::temp_dir().join(format!("generate-extension-registry-{}", std::process::id()));
        std::fs::create_dir_all(&dir)?;
        for name in ["a.CRW", "b.QOI", "c.PSB", "d.pcraft", "unsupported.avif", "notes.txt"] {
            std::fs::write(dir.join(name), [])?;
        }
        let files = folder_images(&dir);
        for (name, included) in [("a.CRW", true), ("b.QOI", true), ("c.PSB", true), ("d.pcraft", true), ("unsupported.avif", false), ("notes.txt", false)] {
            assert_eq!(files.contains(&dir.join(name)), included, "{name}");
        }
        std::fs::remove_dir_all(dir)?;
        Ok(())
    }

    #[test]
    // Adds Windows-invalid characters, device names and trailing dots/spaces.
    fn bg_batch_template_preview_and_conflicts() {
        let dir = directory();
        let name = batch_name("prefix-{name}-{counter}-suffix", std::path::Path::new("cat.photo.jpg"), 12, "psd").unwrap();
        assert_eq!(name, "prefix-cat.photo-12-suffix.psd");
        for invalid in [
            "",
            "../{name}",
            "{unknown}",
            "foo/bar",
            "foo\\bar",
            "a*b",
            "a?b",
            "a\"b",
            "a<b",
            "a>b",
            "a|b",
            "tail.",
            "tail ",
            "CON",
            "NUL.txt",
            "com1",
            "LPT9",
            "COM¹",
        ] {
            assert!(batch_name(invalid, std::path::Path::new("cat.jpg"), 1, "png").is_err());
        }
        let dest = dir.join(&name);
        std::fs::write(&dest, b"existing").unwrap();
        assert_eq!(batch_destination(&dir, &name, "overwrite"), Some(dest.clone()));
        assert_eq!(batch_destination(&dir, &name, "skip"), None);
        let renamed = batch_destination(&dir, &name, "rename").unwrap();
        assert!(renamed.ends_with("prefix-cat.photo-12-suffix-2.psd"));
        std::fs::write(renamed, b"existing").unwrap();
        assert!(batch_destination(&dir, &name, "rename").unwrap().ends_with("prefix-cat.photo-12-suffix-3.psd"));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn bg_batch_cancel_mid_batch_publishes_only_completed_photo() {
        fn process(_: &std::path::Path, dest: &std::path::Path, _: &str, _: &crate::ai_ui::AiOptions, ctl: &lightcraft_sysmem::JobCtl) -> anyhow::Result<()> {
            std::fs::write(dest, b"completed")?;
            ctl.cancel();
            Ok(())
        }
        let dir = directory();
        let files: Vec<_> = (0..3).map(|n| dir.join(format!("photo{n}.png"))).collect();
        for file in &files {
            std::fs::write(file, b"original").unwrap();
        }
        let b = BatchState {
            files,
            output: Some(dir.join("outputs")),
            template: "{name}-{counter}".into(),
            format: "png".into(),
            conflict: "rename".into(),
            ..Default::default()
        };
        let run = start_batch_with(&b, &Default::default(), &process);
        let deadline = Instant::now() + Duration::from_secs(5);
        while !run.lock().unwrap().finished {
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(5));
        }
        let r = run.lock().unwrap();
        assert_eq!(r.done, 1);
        assert!(r.ctl.is_cancelled());
        assert_eq!(r.status[1].1, "Cancelled");
        assert_eq!(r.status[2].1, "Cancelled");
        assert_eq!(std::fs::read_dir(dir.join("outputs")).unwrap().count(), 1);
        for file in &b.files {
            assert_eq!(std::fs::read(file).unwrap(), b"original");
        }
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn bg_batch_overwrite_never_changes_another_input_photo() {
        fn process(_: &std::path::Path, dest: &std::path::Path, _: &str, _: &crate::ai_ui::AiOptions, _: &lightcraft_sysmem::JobCtl) -> anyhow::Result<()> {
            std::fs::write(dest, b"cutout")?;
            Ok(())
        }
        let dir = directory();
        let files = vec![dir.join("source.png"), dir.join("1.png")];
        for input in &files {
            std::fs::write(input, b"original").unwrap();
        }
        let b = BatchState {
            files,
            output: Some(dir.clone()),
            template: "{counter}".into(),
            format: "png".into(),
            conflict: "overwrite".into(),
            ..Default::default()
        };
        let run = start_batch_with(&b, &Default::default(), &process);
        let deadline = Instant::now() + Duration::from_secs(5);
        while !run.lock().unwrap().finished {
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(run.lock().unwrap().status[0].1.contains("overwrite an input"));
        for input in &b.files {
            assert_eq!(std::fs::read(input).unwrap(), b"original");
        }
        assert_eq!(std::fs::read(dir.join("2.png")).unwrap(), b"cutout");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn bg_batch_formats_preserve_alpha_and_psd_layer_mask() {
        use photocraft_doc::{ColorMode, Document, LayerMask, SampleType, Size};
        let mut doc = Document::new("Photo", Size::new(2, 1), ColorMode::Rgb, SampleType::U8);
        let img = RgbaImage::from_fn(2, 1, |_, _| image::Rgba([200, 30, 20, 255]));
        let layer = photocraft_engine::ai_cmds::layer_from_rgba(&doc, "Photo", &img, 0, 0).unwrap();
        let id = doc.insert_above(None, layer);
        let mut mask = photocraft_raster::Surface::new(photocraft_color::PixelFormat::GRAY8);
        mask.write_region(doc.bounds(), &[0.0, 0.5]);
        doc.layer_mut(id).unwrap().mask = Some(LayerMask { surface: mask, ..LayerMask::reveal_all() });
        for format in ["png", "tiff", "psd"] {
            let encoded = photocraft_io::export(&doc, format, &Default::default()).unwrap();
            let loaded = photocraft_io::import(&format!("photo.{format}"), &encoded.bytes).unwrap().document;
            let image = photocraft_engine::ai_cmds::flatten_rgba(&loaded).unwrap();
            assert_eq!(image.get_pixel(0, 0)[3], 0, "{format}");
            assert!((127..=129).contains(&image.get_pixel(1, 0)[3]), "{format}: {:?}", image.get_pixel(1, 0));
            if format == "psd" {
                assert!(loaded.layers[0].mask.is_some());
            }
        }
    }

    #[test]
    fn bg_batch_actual_cancel_and_format_controls() {
        use egui_kittest::kittest::Queryable;
        batch(|b| *b = None);
        open_batch();
        let control = lightcraft_sysmem::JobCtl::new();
        let ctl = control.clone();
        batch(|b| {
            b.as_mut().unwrap().run =
                Some(Arc::new(std::sync::Mutex::new(BatchRun { ctl, status: vec![("photo.jpg".into(), "Working…".into())], ..Default::default() })))
        });
        let mut h = egui_kittest::Harness::builder().with_size(vec2(1100.0, 1100.0)).build_ui_state(
            |ui, app| {
                if ui.ctx().fonts(|f| f.families().contains(&egui::FontFamily::Name("medium".into()))) {
                    batch_window(app, ui.ctx());
                }
            },
            crate::PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default()),
        );
        crate::PhotocraftApp::setup_context(&h.ctx, Default::default());
        h.run_steps(4);
        h.get_by_label("Cancel").click();
        h.run_steps(4);
        assert!(control.is_cancelled());
        h.get_by_value("PNG").click();
        h.run_steps(4);
        h.get_by_label("TIFF").click();
        h.run_steps(4);
        assert_eq!(batch(|b| b.as_ref().unwrap().format.clone()), "tiff");
        batch(|b| *b = None);
    }
}

const FLUX3_KEY: &str = "unet:flux3-image-klein-bf16.safetensors";
const KREA_KEY: &str = "unet:krea2_turbo_bf16.safetensors";
const FLUX3_FILES: &[(&str, &str)] = &[
    ("diffusion_models/flux3-image-klein-bf16.safetensors", "owner__diffusion_models~flux3-image-klein-bf16"),
    ("text_encoders/qwen3vl_4b_bf16.safetensors", "Comfy-Org_Krea-2__text_encoders~qwen3vl_4b_bf16"),
    ("vae/flux2-vae.safetensors", "local__vae~flux2-vae"),
];
const KREA_FILES: &[(&str, &str)] = &[
    ("diffusion_models/krea2_turbo_bf16.safetensors", "Comfy-Org_Krea-2__diffusion_models~krea2_turbo_bf16"),
    ("text_encoders/qwen3vl_4b_bf16.safetensors", "Comfy-Org_Krea-2__text_encoders~qwen3vl_4b_bf16"),
    ("vae/qwen_image_vae.safetensors", "Comfy-Org_Krea-2__vae~qwen_image_vae"),
    ("loras/krea2_style_reference.safetensors", "Comfy-Org_Krea-2__loras~krea2_style_reference"),
];
const UPSAMPLER: (&str, &str) = ("text_encoders/qwen3.5_9b_flux3_prompt_upsampler_w4a8.safetensors", "Comfy-Org_MOSS-Image__text_encoders~qwen3.5_9b_bf16");

/// Models set up through their official templates against the mock ComfyUI 0.42.5 (header-only
/// weights), in the catalogue until dropped.
struct TemplateModels {
    _mock: li_ai::mock::MockComfy,
    dir: PathBuf,
    server: li_ai::modelsetup::state::ServerState,
}

impl TemplateModels {
    fn new(files: &[(&str, &str)]) -> Self {
        static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let n = N.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!("li-ui-setup-{}-{n}", std::process::id()));
        for (file, fixture) in files {
            li_ai::mock::write_header_only(&dir.join(file), &li_ai::mock::header_fixture(fixture)).unwrap();
        }
        let mock = li_ai::mock::MockComfy::start().unwrap();
        mock.serve_comfy_0_42_5(&dir);
        let opts = li_ai::modelsetup::Options {
            host: mock.host().to_owned(),
            model_dir: Some(dir.clone()),
            state_path: dir.join("state/ai-models.json"),
            cache_dir: None,
            templates_dir: None,
        };
        let scan = li_ai::modelsetup::scan(&opts);
        Self { _mock: mock, dir, server: scan.server }
    }

    fn status(&self) -> crate::ai_ui::EngineStatus {
        crate::ai_ui::EngineStatus {
            connected: true,
            checked: true,
            presets: catalog::presets().iter().map(|p| (p.id(), Ok(()))).collect(),
            comfyui_version: "0.42.5".into(),
            setup: li_ai::modelsetup::summary::summarise(Some(&self.server), &catalog::catalog().models, Some("0.42.5")),
            ..Default::default()
        }
    }
}

impl Drop for TemplateModels {
    fn drop(&mut self) {
        catalog::set_templates(Default::default());
        li_ai::inventory::refresh(&li_ai::comfy::ObjectInfo(json!({})), None);
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn panel_harness(app: PhotocraftApp, size: egui::Vec2) -> egui_kittest::Harness<'static, PhotocraftApp> {
    let mut initialized = false;
    let mut h = egui_kittest::Harness::builder().with_size(size).with_max_steps(64).build_ui_state(
        move |ui, app: &mut PhotocraftApp| {
            if !initialized {
                PhotocraftApp::setup_context(ui.ctx(), crate::theme::ThemeKind::Studio);
                initialized = true;
                ui.ctx().request_repaint();
                return;
            }
            panel(app, ui);
        },
        app,
    );
    h.run_steps(4);
    h
}

fn app_on(key: &str) -> PhotocraftApp {
    let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
    app.ui.ai.generate.mode = Mode::Create;
    set_model(&mut app.ui.ai.generate, ModelId::from_key(key).unwrap());
    app
}

#[test]
fn template_models_refine_and_upscale_only_through_refine() {
    let _lock = crate::ai_ui::AI_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _models = TemplateModels::new(FLUX3_FILES);
    let flux = ModelId::from_key(FLUX3_KEY).expect("set up from its template").info();
    assert!(fits_mode(flux, Mode::Refine), "an image model refines");
    assert!(!fits_mode_with(flux, Mode::Upscale, false), "Upscale offers the official upscalers first");
    assert!(fits_mode_with(flux, Mode::Upscale, true), "Choose another model… refine-upscales with it");
    assert!(!runs_as_enhance(flux, Mode::Upscale), "it never goes to ai.enhance");
}

#[test]
fn template_models_carry_their_setup_chip_and_failed_ones_wait_for_setup() {
    use egui_kittest::kittest::{NodeT, Queryable};
    let _lock = crate::ai_ui::AI_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    crate::model_setup::record_actions();
    let models = TemplateModels::new(&[FLUX3_FILES, KREA_FILES].concat());
    let flux = ModelId::from_key(FLUX3_KEY).expect("set up from its template").info();
    assert!(flux.recipe.is_some() && fits_mode(flux, Mode::Create) && fits_mode(flux, Mode::Fill));
    let mut st = models.status();
    assert_eq!(st.setup.status(FLUX3_KEY), Some(li_ai::modelsetup::state::Status::Ready));
    st.setup.models.get_mut(KREA_KEY).unwrap().status = li_ai::modelsetup::state::Status::Failed;
    crate::ai_ui::TEST_STATUS.with(|s| *s.borrow_mut() = Some(st));
    let mut h = panel_harness(app_on(FLUX3_KEY), vec2(520.0, 1100.0));
    h.get_by_value(&flux.label).click();
    h.run_steps(3);
    h.get_by_label("Not tested");
    h.get_by_label("OTHER INSTALLED MODELS");
    let krea = ModelId::from_key(KREA_KEY).unwrap().info();
    assert!(h.get_by_label(&krea.label).accesskit_node().is_disabled(), "a model whose test failed can't be picked");
    h.get_all_by_label("Set up…").last().unwrap().click();
    h.run_steps(3);
    assert!(crate::model_setup::is_open());
    assert_eq!(crate::model_setup::selected().as_deref(), Some(KREA_KEY));
}

#[test]
fn the_new_models_banner_opens_set_up_models() {
    use egui_kittest::kittest::Queryable;
    let _lock = crate::ai_ui::AI_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    crate::model_setup::record_actions();
    let mut st = crate::ai_ui::EngineStatus {
        connected: true,
        checked: true,
        presets: catalog::presets().iter().map(|p| (p.id(), Ok(()))).collect(),
        ..Default::default()
    };
    st.setup.new = vec![FLUX3_KEY.into(), KREA_KEY.into()];
    crate::ai_ui::TEST_STATUS.with(|s| *s.borrow_mut() = Some(st));
    let app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
    let mut h = panel_harness(app, vec2(420.0, 900.0));
    h.get_by_label("2 new models found");
    h.get_by_label("Set up…").click();
    h.run_steps(3);
    assert!(crate::model_setup::is_open());
    assert_eq!(crate::model_setup::selected().as_deref(), Some(FLUX3_KEY));
}

#[test]
fn enhance_prompt_is_off_until_the_enhancer_is_installed_and_remembered_per_model() {
    use egui_kittest::kittest::{NodeT, Queryable};
    let _lock = crate::ai_ui::AI_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    crate::model_setup::record_actions();
    {
        let models = TemplateModels::new(FLUX3_FILES);
        crate::ai_ui::TEST_STATUS.with(|s| *s.borrow_mut() = Some(models.status()));
        let mut h = panel_harness(app_on(FLUX3_KEY), vec2(420.0, 1100.0));
        assert!(h.get_by_label("Enhance prompt").accesskit_node().is_disabled());
        h.get_by_label("Get the prompt enhancer…").click();
        h.run_steps(3);
        assert_eq!(crate::model_setup::selected().as_deref(), Some(FLUX3_KEY));
    }
    let models = TemplateModels::new(&[FLUX3_FILES, &[UPSAMPLER]].concat());
    crate::ai_ui::TEST_STATUS.with(|s| *s.borrow_mut() = Some(models.status()));
    let mut h = panel_harness(app_on(FLUX3_KEY), vec2(420.0, 1100.0));
    assert!(h.query_by_label("Get the prompt enhancer…").is_none());
    h.get_by_label("Enhance prompt").click();
    h.run_steps(3);
    let s = h.state().ui.ai.generate.clone();
    assert_eq!(s.enhance_prompt.get(FLUX3_KEY), Some(&true));
    let (req, _, _) = local_request(&s, "a house".into(), None, Vec::new()).unwrap();
    assert!(req.enhance);
    let saved: GenerateState = serde_json::from_value(serde_json::to_value(&s).unwrap()).unwrap();
    assert_eq!(saved.enhance_prompt.get(FLUX3_KEY), Some(&true));
    assert!(serde_json::from_value::<GenerateState>(json!({})).unwrap().enhance_prompt.is_empty(), "off by default");
}

#[test]
fn a_style_reference_model_labels_its_references_as_styles() {
    use egui_kittest::kittest::Queryable;
    let _lock = crate::ai_ui::AI_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    crate::model_setup::record_actions();
    let models = TemplateModels::new(KREA_FILES);
    crate::ai_ui::TEST_STATUS.with(|s| *s.borrow_mut() = Some(models.status()));
    let krea = ModelId::from_key(KREA_KEY).unwrap().info();
    assert!(krea.max_references > 0, "the style template takes references");
    let h = panel_harness(app_on(KREA_KEY), vec2(420.0, 1100.0));
    assert!(h.query_by_label_contains("Style references · 0/").is_some());
}

#[test]
fn an_unknown_checkpoint_is_offered_for_refine_without_a_builder_check() {
    use egui_kittest::kittest::{NodeT, Queryable};
    let _lock = crate::ai_ui::AI_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let info = li_ai::comfy::ObjectInfo(json!({"CheckpointLoaderSimple": {"input": {"required": {"ckpt_name": [["mystery_model.safetensors"]]}}}}));
    li_ai::inventory::refresh(&info, None);
    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            li_ai::inventory::refresh(&li_ai::comfy::ObjectInfo(json!({})), None);
        }
    }
    let _reset = Reset;
    let id = ModelId::from_key("ckpt:mystery_model.safetensors").expect("listed as installed");
    assert_eq!(id.info().family, "unknown");
    let presets: std::collections::BTreeMap<String, Result<(), String>> = catalog::presets()
        .iter()
        .map(|p| {
            let a = catalog::availability(p, &info);
            (p.id(), if a.available { Ok(()) } else { Err(a.reason) })
        })
        .collect();
    let st = crate::ai_ui::EngineStatus { connected: true, checked: true, presets, ..Default::default() };
    assert_eq!(refine_readiness(&st, id, catalog::default_variant(id)), Ok(()), "no builder node check blocks it");
    crate::ai_ui::TEST_STATUS.with(|s| *s.borrow_mut() = Some(st));
    let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
    app.ui.ai.generate.mode = Mode::Refine;
    app.ui.ai.generate.model = id.key().into();
    let mut h = panel_harness(app, vec2(420.0, 1000.0));
    h.get_by_value(&format!("{} · {}", id.info().label, id.info().family_label)).click();
    h.run_steps(3);
    assert!(!h.get_by_role_and_label(egui::accesskit::Role::Button, &id.info().label).accesskit_node().is_disabled());
    assert!(h.query_by_label_contains("missing the nodes").is_none());
}

#[test]
fn the_picker_does_not_repeat_a_family_already_in_the_model_name() {
    assert_eq!(family_suffix("FLUX.3 Klein Distilled · BF16", "FLUX.3 Klein"), None);
    assert_eq!(family_suffix("Krea 2 Turbo · BF16", "Krea 2"), None);
    assert_eq!(family_suffix("Model", "Other family"), Some("Other family"));
}
