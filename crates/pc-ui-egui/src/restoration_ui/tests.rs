use super::*;
use egui::{Event, Modifiers, PointerButton};
use egui_kittest::{Harness, kittest::Queryable};
use image::Rgba;
use serde_json::json;

fn harness() -> Harness<'static, PhotocraftApp> {
    let mut session = photocraft_engine::Session::new();
    session.execute("file.new", json!({"width": 64, "height": 48})).unwrap();
    session.execute("edit.fill", json!({"color": "#334455"})).unwrap();
    let mut h = Harness::builder().with_size(vec2(1200.0, 850.0)).with_max_steps(64).build_eframe(move |cc| {
        PhotocraftApp::setup_context(&cc.egui_ctx, Default::default());
        PhotocraftApp::new(session, crate::Services::default())
    });
    h.run_steps(4);
    h
}

fn open_menu(h: &mut Harness<'_, PhotocraftApp>) {
    h.get_by_label("Filter").click();
    h.run_steps(3);
    h.get_by_label("AI ⏵").hover();
    h.run_steps(4);
    h.get_by_label("Restore…").click();
    h.run_steps(4);
    assert!(h.state().restoration.open);
}

fn inject(h: &mut Harness<'_, PhotocraftApp>) {
    TEST_RESULT.with(|r| *r.borrow_mut() = Some(Ok(RgbaImage::from_pixel(64, 48, Rgba([180, 100, 70, 128])))));
    h.get_by_label("Preview restoration").click();
    h.run_steps(3);
    assert!(h.state().restoration.result.is_some(), "preview button must run");
}

#[test]
fn restoration_ui_menu_task_preview_split_apply_and_one_undo() {
    let mut h = harness();
    open_menu(&mut h);
    let before = h.state().session.active().unwrap().doc.clone();
    let history = h.state().session.active().unwrap().history.past_len();
    h.get_by_label("Scan cleanup").click();
    h.run_steps(2);
    assert_eq!(h.state().restoration.task, Task::Scan);
    inject(&mut h);
    assert_eq!(h.state().session.active().unwrap().doc, before, "preview edits no pixels");
    let rect = h.get_by_label("Before / after split").rect();
    let a = egui::pos2(rect.left() + 20.0, rect.center().y);
    let b = egui::pos2(rect.left() + 90.0, rect.center().y);
    h.hover_at(a);
    h.step();
    h.event(Event::PointerButton { pos: a, button: PointerButton::Primary, pressed: true, modifiers: Modifiers::NONE });
    h.step();
    h.event(Event::PointerMoved(b));
    h.step();
    h.event(Event::PointerButton { pos: b, button: PointerButton::Primary, pressed: false, modifiers: Modifiers::NONE });
    h.run_steps(2);
    assert_ne!(h.state().restoration.split, 0.5, "drag actually changes the split");
    h.get_by_label("Apply as new layer").click();
    h.run_steps(3);
    let d = h.state().session.active().unwrap();
    assert_eq!(d.doc.layers.len(), before.layers.len() + 1);
    assert_eq!(d.history.past_len(), history + 1);
    assert_eq!(d.doc.layers[0].content, before.layers[0].content, "source pixels stay untouched");
    assert!(!d.doc.layers[0].visible, "the original composite must not double partially transparent output");
    let applied = photocraft_engine::ai_cmds::flatten_rgba(&d.doc);
    assert_eq!(applied.get_pixel(10, 10).0, [180, 100, 70, 128]);
    assert_eq!(d.doc.layer(d.active_layer.unwrap()).unwrap().name, "Restoration: Scan cleanup");
    assert!(!h.state().restoration.open);
    h.state_mut().run("edit.undo", json!({})).unwrap();
    assert_eq!(h.state().session.active().unwrap().doc, before);
    h.state_mut().run("edit.redo", json!({})).unwrap();
    assert_eq!(h.state().session.active().unwrap().doc.layers.len(), before.layers.len() + 1);
}

#[test]
fn restoration_ui_refuses_unsupported_tasks_cancels_and_opens_browser() {
    let mut h = harness();
    open_menu(&mut h);
    h.get_by_label("Colourise").click();
    h.run_steps(2);
    assert_eq!(h.state().restoration.task, Task::Colourise);
    assert!(h.state().restoration.task.file().is_ok());
    assert!(h.query_by_label(li_ai::restoration::COLOURISE_UNAVAILABLE).is_some());
    h.get_by_label("Old-photo preset").click();
    h.run_steps(2);
    assert!(h.query_by_label(li_ai::restoration::OLD_PHOTO_UNAVAILABLE).is_some());
    h.get_by_label("Denoise").click();
    h.run_steps(2);
    inject(&mut h);
    h.get_by_label("Cancel").click();
    h.run_steps(2);
    assert!(!h.state().restoration.open);
    assert_eq!(h.state().session.active().unwrap().doc.layers.len(), 1);
    open_menu(&mut h);
    h.get_by_label("Install restoration models…").click();
    h.run_steps(3);
    assert!(crate::model_browser::is_open());
    assert_eq!(crate::model_browser::showing(), (false, Some("restoration".into())));
    assert!(h.query_by_label("FBCNN · JPEG cleanup").is_some());
    assert!(h.query_by_label("SCUNet-real · Scan cleanup").is_some());
    assert!(h.query_by_label("Restoration installs only vetted official weights with pinned size and SHA-256.").is_some());
    crate::model_browser::close();
}

#[test]
fn restoration_ui_discards_preview_when_document_or_task_changes() {
    let mut h = harness();
    open_menu(&mut h);
    inject(&mut h);
    h.get_by_label("Denoise").click();
    h.run_steps(2);
    assert!(h.state().restoration.result.is_none());
    inject(&mut h);
    h.state_mut().run("edit.fill", json!({"color": "#999999"})).unwrap();
    assert!(apply(h.state_mut()).unwrap_err().contains("changed"));
    h.run_steps(3);
    assert!(h.state().restoration.result.is_none());
    inject(&mut h);
    h.state_mut().run("file.new", json!({"width": 24, "height": 16})).unwrap();
    h.run_steps(3);
    assert!(h.state().restoration.result.is_none());
    assert_eq!(h.state().restoration.source.as_ref().unwrap().dimensions(), (24, 16));
}

#[test]
fn restoration_ui_byo_selection_requires_persisted_acknowledgement() {
    let mut h = harness();
    open_menu(&mut h);
    let mut info = li_ai::comfy::ObjectInfo(li_ai::mock::object_info());
    info.0["UpscaleModelLoader"]["input"]["required"]["model_name"] = json!([["LED_model.pth", "SUPIR_LED.pth", "HYPIR.pth"]]);
    let models = byo::detect(&info, &[]);
    assert_eq!(models.len(), 1);
    h.state_mut().restoration.byo_models = models;
    h.state_mut().restoration.byo_host = "test-host".into();
    h.get_by_label("Bring your own model").click();
    h.run_steps(3);
    h.get_by_label("LED · LED_model.pth").click();
    h.run_steps(3);
    let model = h.state().restoration.selected_byo.clone().unwrap();
    assert!(!h.state().restoration.acks.contains(&model, "test-host"));
    assert!(h.query_by_label(byo::NOTICE).is_some());
    h.get_by_label("I have read the licence and understand the commercial-use restrictions").click();
    h.run_steps(3);
    assert!(byo::Acknowledgements::load(&ack_path()).contains(&model, "test-host"));
    h.get_by_label("Bring your own model").click();
    h.run_steps(3);
    inject(&mut h);
    h.get_by_label("Apply as new layer").click();
    h.run_steps(3);
    assert_eq!(h.state().session.active().unwrap().doc.layers.last().unwrap().name, "Restoration: LED");
    std::fs::remove_file(ack_path()).unwrap();
}

/// Opt-in real GPU and ComfyUI UI-path timing. No desktop window is created.
#[test]
#[ignore = "requires pinned restoration downloads and idle real RTX 5090 ComfyUI"]
fn restoration_ui_real_5090_preview_apply_speed() {
    let _lock = crate::ai_ui::AI_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let model_dir = std::env::var("LOCAL_IMAGE_RESTORATION_MODELS").expect("set test model directory");
    let host = std::env::var("LOCAL_IMAGE_RESTORATION_COMFY").unwrap_or_else(|_| "127.0.0.1:8188".into());
    li_ai::set_service_override(Some(host.clone()));
    let mut settings = li_ai::AiSettings::load();
    settings.model_directory = model_dir;
    settings.comfy_port = host.rsplit(':').next().unwrap().parse().unwrap();
    settings.comfy_host = "127.0.0.1".into();
    TEST_SETTINGS.with(|s| *s.borrow_mut() = Some(settings));
    let rs = egui_kittest::wgpu::create_render_state(crate::gpu_canvas::wgpu_setup(), eframe::egui_wgpu::RendererOptions::PREDICTABLE);
    let gpu = rs.adapter.get_info();
    assert_ne!(gpu.device_type, eframe::wgpu::DeviceType::Cpu, "real GPU required");
    assert!(gpu.name.contains("5090"), "RTX 5090 required: {gpu:?}");
    eprintln!("Restoration preview GPU: {gpu:?}");
    let mut session = photocraft_engine::Session::new();
    session.execute("file.new", json!({"width": 256, "height": 192})).unwrap();
    session.execute("edit.fill", json!({"color": "#667799"})).unwrap();
    let mut h = Harness::builder()
        .with_size(vec2(1200.0, 850.0))
        .with_max_steps(64)
        .renderer(egui_kittest::wgpu::WgpuTestRenderer::from_render_state(rs))
        .build_eframe(move |cc| {
            PhotocraftApp::setup_context(&cc.egui_ctx, Default::default());
            PhotocraftApp::new(session, crate::Services::default())
        });
    h.run_steps(4);
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/restoration-ui");
    std::fs::create_dir_all(&dir).unwrap();
    for task in [Task::Jpeg, Task::Scan, Task::Denoise] {
        open_menu(&mut h);
        h.get_by_label(task.label()).click();
        h.run_steps(2);
        let started = Instant::now();
        h.get_by_label("Preview restoration").click();
        h.run_steps(2);
        while h.state().restoration.pending.is_some() {
            assert!(started.elapsed() < Duration::from_secs(180));
            std::thread::sleep(Duration::from_millis(30));
            h.run_steps(1);
        }
        assert!(h.state().restoration.result.is_some(), "{}", h.state().restoration.message);
        h.run_steps(3);
        let ready = started.elapsed();
        let frame_start = Instant::now();
        for _ in 0..10 {
            h.run_steps(1);
            h.render().unwrap();
        }
        let frame_ms = frame_start.elapsed().as_secs_f64() * 100.0;
        // Force an actual wgpu render, including the before/after textures.
        h.render().unwrap().save(dir.join(format!("{:?}.png", task))).unwrap();
        let history = h.state().session.active().unwrap().history.past_len();
        let apply_start = Instant::now();
        h.get_by_label("Apply as new layer").click();
        h.run_steps(3);
        assert_eq!(h.state().session.active().unwrap().history.past_len(), history + 1);
        eprintln!(
            "RESTORATION_UI {:?} preview_ms={:.2} frame_ms={:.2} apply_ms={:.2}",
            task,
            ready.as_secs_f64() * 1000.0,
            frame_ms,
            apply_start.elapsed().as_secs_f64() * 1000.0
        );
        h.state_mut().run("edit.undo", json!({})).unwrap();
        h.run_steps(2);
    }
    TEST_SETTINGS.with(|s| *s.borrow_mut() = None);
    li_ai::set_service_override(None);
}
