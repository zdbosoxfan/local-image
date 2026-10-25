use li_ai::browser::Plan;

use super::*;

#[test]
fn ds_finding_unknown_family_loras_always_open_trending() {
    for nav in [Nav::Installed, Nav::New, Nav::Family("flux".into())] {
        with(|s| s.nav = nav);
        open_loras_any();
        with(|s| {
            assert_eq!(s.nav, Nav::Trending);
            assert_eq!(s.kind, Kind::Lora);
            assert!(s.open);
        });
    }
    with(|s| *s = State::default());
}
fn card_harness(item: Item, run: Option<InstallRun>) -> egui_kittest::Harness<'static, State> {
    let mut state = State::default();
    if let Some(run) = run {
        state.installs.insert(item.id.clone(), Arc::new(Mutex::new(run)));
    }
    let mut initialized = false;
    let mut h = egui_kittest::Harness::builder().with_size(vec2(320.0, 420.0)).build_ui_state(
        move |ui, s: &mut State| {
            if !initialized {
                PhotocraftApp::setup_context(ui.ctx(), crate::theme::ThemeKind::Studio);
                initialized = true;
                ui.ctx().request_repaint();
                return;
            }
            card(ui, s, &Tokens::get(ui.ctx()), &item, 300.0, &HashSet::new());
        },
        state,
    );
    h.run_steps(3);
    h
}

#[test]
fn template_cards_say_when_comfyui_is_too_old_and_offer_set_up_after_install() {
    use egui_kittest::kittest::Queryable;
    let _lock = crate::ai_ui::AI_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    crate::model_setup::record_actions();
    let st = crate::ai_ui::EngineStatus { connected: true, checked: true, comfyui_version: "0.38.0".into(), ..Default::default() };
    crate::ai_ui::TEST_STATUS.with(|s| *s.borrow_mut() = Some(st));
    let item = Item {
        id: "template:image_moss_image_t2i".into(),
        title: "MOSS-Image: Text to Image".into(),
        template: Some("image_moss_image_t2i".into()),
        min_comfyui: Some("0.41.3".into()),
        ..Default::default()
    };
    let h = card_harness(item.clone(), None);
    h.get_by_label("Needs ComfyUI 0.41.3");
    assert!(h.get_all_by_label("Official template").count() > 0);
    drop(h);
    let done = InstallRun { finished: true, setup: Some("unet:moss_image_sft_int8_convrot.safetensors".into()), ..Default::default() };
    let mut h = card_harness(Item { min_comfyui: None, ..item }, Some(done));
    assert!(h.query_by_label("Needs ComfyUI 0.41.3").is_none());
    h.get_by_label("Set up and test").click();
    h.run_steps(2);
    assert!(crate::model_setup::is_open());
    assert_eq!(crate::model_setup::selected().as_deref(), Some("unet:moss_image_sft_int8_convrot.safetensors"));
}

#[test]
fn a_card_with_several_precisions_installs_only_the_one_chosen() {
    use egui_kittest::kittest::Queryable;
    let _lock = crate::ai_ui::AI_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let gpu = li_ai::setup::Gpu { name: "NVIDIA GeForce RTX 4090".into(), vram_total: 24_000_000_000, vram_used: None, utilization: None };
    let st = crate::ai_ui::EngineStatus { connected: true, checked: true, gpu: Some(gpu), ..Default::default() };
    crate::ai_ui::TEST_STATUS.with(|s| *s.borrow_mut() = Some(st));
    let file = |name: &str, folder: &str| browser::CatalogFile { name: name.into(), folder: folder.into(), ..Default::default() };
    let item = Item {
        id: "hf:Comfy-Org/flux3".into(),
        title: "flux3".into(),
        files: vec![
            file("flux3-image-klein-nvfp4.safetensors", "diffusion_models"),
            file("flux3-image-klein-bf16.safetensors", "diffusion_models"),
            file("flux3-image-klein-int8-convrot.safetensors", "diffusion_models"),
            file("qwen3vl_4b_bf16.safetensors", "text_encoders"),
            file("qwen3vl_4b_int8_convrot.safetensors", "text_encoders"),
            file("qwen3.5_9b_flux3_prompt_upsampler_w4a8.safetensors", "text_encoders"),
        ],
        ..Default::default()
    };
    let planned = || with(|s| s.confirm.as_ref().unwrap().plan.lock().unwrap().clone().unwrap().unwrap());
    let names = || planned().item.files.iter().map(|f| f.name.clone()).collect::<Vec<_>>();
    let _plan = test_install_plan(Plan::default());
    with(|s| s.confirm = Some(Confirm::new(item)));
    assert_eq!(names(), ["flux3-image-klein-bf16.safetensors", "qwen3vl_4b_bf16.safetensors"], "the best precision this GPU runs");
    let mut initialized = false;
    let mut h = egui_kittest::Harness::builder().with_size(vec2(700.0, 500.0)).build_ui(move |ui| {
        if !initialized {
            PhotocraftApp::setup_context(ui.ctx(), crate::theme::ThemeKind::Studio);
            initialized = true;
            ui.ctx().request_repaint();
            return;
        }
        confirm_window(ui.ctx(), &Tokens::get(ui.ctx()));
    });
    h.run_steps(3);
    h.get_by_value("flux3-image-klein-bf16.safetensors").click();
    h.run_steps(3);
    let _plan = test_install_plan(Plan::default());
    h.get_by_label("flux3-image-klein-int8-convrot.safetensors").click();
    h.run_steps(3);
    assert_eq!(names(), ["flux3-image-klein-int8-convrot.safetensors", "qwen3vl_4b_int8_convrot.safetensors"], "no prompt enhancer unasked");
    // The prompt enhancer installs once it is ticked.
    let _plan = test_install_plan(Plan::default());
    h.get_by_label("qwen3.5_9b_flux3_prompt_upsampler_w4a8.safetensors").click();
    h.run_steps(3);
    assert_eq!(
        names(),
        ["flux3-image-klein-int8-convrot.safetensors", "qwen3vl_4b_int8_convrot.safetensors", "qwen3.5_9b_flux3_prompt_upsampler_w4a8.safetensors"]
    );
    with(|s| s.confirm = None);
}

#[test]
fn ds_finding_installed_names_are_cached_across_repaints() {
    let mut state = State::default();
    let first = cached_installed(&mut state, || HashSet::from(["sample.safetensors".into()]));
    for _ in 0..20 {
        let next = cached_installed(&mut state, || panic!("filesystem rescan on repaint"));
        assert!(Arc::ptr_eq(&first, &next));
    }
    state.installed.as_mut().unwrap().0 -= std::time::Duration::from_secs(2);
    assert!(cached_installed(&mut state, HashSet::new).is_empty());
}

#[test]
fn ds2_restoration_cards_supersede_pending_catalogue_fetch() {
    let mut state = State { nav: Nav::Family("restoration".into()), fetch: Some(Arc::new(Mutex::new(None))), ..Default::default() };
    let old_fetch = state.fetch.clone().unwrap();
    let mut initialized = false;
    let mut h = egui_kittest::Harness::builder().with_size(vec2(1000.0, 700.0)).build_ui(move |ui| {
        if !initialized {
            PhotocraftApp::setup_context(ui.ctx(), crate::theme::ThemeKind::Studio);
            initialized = true;
            ui.ctx().request_repaint();
            return;
        }
        body(ui, &mut state, &Tokens::get(ui.ctx()));
    });
    h.run_steps(3);
    use egui_kittest::kittest::Queryable;
    assert!(h.get_all_by_label("FBCNN · JPEG cleanup").count() > 0);
    *old_fetch.lock().unwrap() = Some(Fetched { items: vec![], notes: vec!["old family".into()] });
    h.run_steps(2);
    assert!(h.get_all_by_label("FBCNN · JPEG cleanup").count() > 0);
}
