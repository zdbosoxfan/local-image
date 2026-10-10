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
    h.run_steps(1);
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
    h.get_by_label("Clean up scan").click();
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
    assert_eq!(d.doc.layer(d.active_layer.unwrap()).unwrap().name, "Restoration: Clean up scan");
    assert!(!h.state().restoration.open);
    h.state_mut().run("edit.undo", json!({})).unwrap();
    assert_eq!(h.state().session.active().unwrap().doc, before);
    h.state_mut().run("edit.redo", json!({})).unwrap();
    assert_eq!(h.state().session.active().unwrap().doc.layers.len(), before.layers.len() + 1);
}

#[test]
fn restoration_ui_four_curated_tasks_cancel_and_browser() {
    let mut h = harness();
    open_menu(&mut h);
    for task in TASKS {
        h.get_by_label(caption(task)).click();
        h.run_steps(2);
        assert_eq!(h.state().restoration.task, task);
    }
    assert!(h.query_by_label("Denoise").is_none());
    assert!(h.query_by_label("Old-photo preset").is_none());
    h.get_by_label("Colourise").click();
    h.run_steps(2);
    inject(&mut h);
    h.get_by_label("Cancel").click();
    h.run_steps(2);
    assert!(!h.state().restoration.open);
    assert_eq!(h.state().session.active().unwrap().doc.layers.len(), 1);
    open_menu(&mut h);
    h.get_by_label("Fix JPEG artefacts").click();
    h.run_steps(2);
    h.get_by_label("Install restoration models…").click();
    h.run_steps(3);
    assert!(crate::model_browser::is_open());
    assert_eq!(crate::model_browser::showing(), (false, Some("restoration".into())));
    assert!(h.get_all_by_label("FBCNN · JPEG cleanup").count() > 0);
    assert!(h.get_all_by_label("SCUNet-real · Scan cleanup").count() > 0);
    crate::model_browser::close();
}

#[test]
fn restoration_ui_detected_mask_review_requires_remove_confirmation() {
    let mut h = harness();
    h.state_mut().ui.ai.remove_immediately = true;
    open_menu(&mut h);
    h.get_by_label("Repair old photo").click();
    h.run_steps(2);
    h.get_by_label("Colourise after repair").click();
    h.run_steps(2);
    let before = h.state().session.active().unwrap().doc.clone();
    TEST_MASK.with(|r| *r.borrow_mut() = Some(GrayImage::from_fn(64, 48, |x, y| image::Luma([if x == 25 && y > 10 && y < 40 { 255 } else { 0 }]))));
    h.run_steps(1);
    h.get_by_label("Preview restoration").click();
    h.run_steps(3);
    assert!(h.state().restoration.mask.is_some());
    h.get_by_label("Review scratches in AI Remove").click();
    h.run_steps(3);
    assert!(!h.state().restoration.open);
    assert!(h.state().ai_remove.is_some());
    assert_eq!(h.state().session.active().unwrap().doc, before);
    assert!(crate::ai_ui::ai_jobs(h.state()).is_empty());
    assert!(h.get_all_by_label("Remove").count() > 0);
    h.get_all_by_label("Cancel").next().unwrap().click();
    h.run_steps(3);
    assert!(h.state().ai_remove.is_none());
    assert_eq!(h.state().session.active().unwrap().doc, before);
}

#[test]
fn restoration_ui_discards_preview_when_document_or_task_changes() {
    let mut h = harness();
    open_menu(&mut h);
    inject(&mut h);
    h.get_by_label("Colourise").click();
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
    struct AckCleanup;
    impl Drop for AckCleanup {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(ack_path());
        }
    }
    let _cleanup = AckCleanup;
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
    for task in [Task::Jpeg, Task::Scan] {
        open_menu(&mut h);
        h.get_by_label(caption(task)).click();
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

/// End-to-end native worker + actual RTX 5090 egui rendering + actual existing AI Remove.
/// Uses NASA's public-domain astronaut, approved ONNX pins and already installed fill models.
#[test]
#[ignore = "requires prepared exports, NASA fixture and idle real RTX 5090 ComfyUI"]
fn restoration_ui_local_real_5090_colourise_and_repair() {
    let _lock = crate::ai_ui::AI_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let exports = std::path::PathBuf::from(std::env::var("LOCAL_IMAGE_RESTORE_EXPORTS").expect("export folder"));
    let dir = exports.join("ui-installed");
    let host = std::env::var("LOCAL_IMAGE_RESTORATION_COMFY").unwrap_or("127.0.0.1:8188".into());
    li_ai::set_service_override(Some(host.clone()));
    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            li_ai::set_service_override(None);
            TEST_SETTINGS.with(|s| *s.borrow_mut() = None);
            crate::ai_ui::TEST_STATUS.with(|s| *s.borrow_mut() = None);
        }
    }
    let _reset = Reset;
    let stats = li_ai::service().client.system_stats().unwrap();
    assert!(stats["devices"][0]["name"].as_str().unwrap().contains("5090"), "{stats}");
    assert!(li_ai::service().client.queue_state().unwrap()["queue_running"].as_array().unwrap().is_empty());
    let mut config = li_ai::AiSettings::load();
    config.model_directory = dir.to_string_lossy().into();
    TEST_SETTINGS.with(|s| *s.borrow_mut() = Some(config));
    let rs = egui_kittest::wgpu::create_render_state(crate::gpu_canvas::wgpu_setup(), eframe::egui_wgpu::RendererOptions::PREDICTABLE);
    let gpu = rs.adapter.get_info();
    assert_ne!(gpu.device_type, eframe::wgpu::DeviceType::Cpu);
    assert!(gpu.name.contains("5090"), "{gpu:?}");
    eprintln!("RESTORE_UI_GPU {gpu:?}");
    let input = image::open(exports.join("astronaut.png")).unwrap().to_rgba8();
    let mut input = image::imageops::resize(&input, 257, 193, image::imageops::FilterType::CatmullRom);
    for (x, y, p) in input.enumerate_pixels_mut() {
        let gray = ((u32::from(p[0]) + u32::from(p[1]) + u32::from(p[2])) / 3) as u8;
        *p = image::Rgba([gray, gray, gray, if x == 0 && y == 0 { 128 } else { 255 }]);
    }
    for y in 15..180 {
        for x in 80..84 {
            *input.get_pixel_mut(x, y) = image::Rgba([245, 245, 245, 255]);
        }
    }
    let mut session = photocraft_engine::Session::new();
    session.execute("file.new", json!({"width":257,"height":193})).unwrap();
    session
        .edit("NASA public-domain fixture", |doc, active| {
            let layer = photocraft_engine::ai_cmds::layer_from_rgba(doc, "NASA astronaut", &input, 0, 0);
            doc.layers.clear();
            let id = doc.insert_above(None, layer);
            *active = Some(id);
            Ok(())
        })
        .unwrap();
    let mut h =
        Harness::builder().with_size(vec2(1200., 850.)).with_max_steps(64).renderer(egui_kittest::wgpu::WgpuTestRenderer::from_render_state(rs)).build_eframe(
            move |cc| {
                PhotocraftApp::setup_context(&cc.egui_ctx, Default::default());
                PhotocraftApp::new(session, crate::Services::default())
            },
        );
    h.run_steps(4);
    let before = h.state().session.active().unwrap().doc.clone();
    for task in [Task::Colourise, Task::OldPhoto] {
        open_menu(&mut h);
        h.get_by_label(caption(task)).click();
        h.run_steps(2);
        let pin = local_pin(task).unwrap();
        crate::ai_ui::TEST_PICK.with(|p| *p.borrow_mut() = Some(exports.join(pin.file)));
        h.get_by_label("Import Restore model…").click();
        h.run_steps(2);
        let import_started = Instant::now();
        while h.state().restoration.pending.is_some() {
            assert!(import_started.elapsed() < Duration::from_secs(30));
            std::thread::sleep(Duration::from_millis(20));
            h.run_steps(1);
        }
        assert!(pin.installed(&dir), "{}", h.state().restoration.message);
        if task == Task::OldPhoto {
            h.get_by_label("Colourise after repair").click();
            h.run_steps(2);
        }
        let start = Instant::now();
        h.get_by_label("Preview restoration").click();
        h.run_steps(2);
        let mut frames = Vec::new();
        while h.state().restoration.pending.is_some() {
            assert!(start.elapsed() < Duration::from_secs(90));
            let frame = Instant::now();
            h.run_steps(1);
            h.render().unwrap();
            frames.push(frame.elapsed().as_secs_f64() * 1000.);
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(h.state().restoration.result.is_some(), "{}", h.state().restoration.message);
        frames.sort_by(f64::total_cmp);
        eprintln!("RESTORE_UI {:?} preview_ms={:.2} running_frame_median_ms={:.2}", task, start.elapsed().as_secs_f64() * 1000., frames[frames.len() / 2]);
        h.run_steps(3);
        h.render().unwrap().save(exports.join(format!("ui-{task:?}.png"))).unwrap();
        if task == Task::Colourise {
            let history = h.state().session.active().unwrap().history.past_len();
            let start = Instant::now();
            h.get_by_label("Apply as new layer").click();
            h.run_steps(3);
            eprintln!("RESTORE_UI colourise_apply_ms={:.2}", start.elapsed().as_secs_f64() * 1000.);
            assert_eq!(h.state().session.active().unwrap().history.past_len(), history + 1);
            assert_eq!(photocraft_engine::ai_cmds::flatten_rgba(&h.state().session.active().unwrap().doc).get_pixel(0, 0)[3], 128);
            h.state_mut().run("edit.undo", json!({})).unwrap();
            h.run_steps(2);
            assert_eq!(h.state().session.active().unwrap().doc, before);
        } else {
            let history = h.state().session.active().unwrap().history.past_len();
            h.state_mut().ui.ai.remove_immediately = true;
            h.get_by_label("Review scratches in AI Remove").click();
            h.run_steps(3);
            assert!(h.state().ai_remove.is_some());
            assert_eq!(h.state().session.active().unwrap().doc, before);
            assert!(li_ai::service().client.queue_state().unwrap()["queue_running"].as_array().unwrap().is_empty(), "mask review must not submit a fill");
            // Use actual Comfy model/node readiness, not fake inference.
            let info = li_ai::service().client.object_info().unwrap();
            let status = li_ai::catalog::availability(li_ai::catalog::preset(li_ai::ModelId::KleinRemove, "bf16").unwrap(), &info);
            assert!(status.available, "{}", status.reason);
            crate::ai_ui::TEST_STATUS.with(|s| {
                *s.borrow_mut() = Some(crate::ai_ui::EngineStatus {
                    checked: true,
                    connected: true,
                    presets: [(format!("{}:bf16", li_ai::ModelId::KleinRemove.key()), Ok(()))].into(),
                    ..Default::default()
                })
            });
            h.state_mut().background_jobs = true;
            h.run_steps(2);
            // The official model misses part of this synthetic line. Review is an actual
            // editable mask, so exercise a real canvas drag to include that missed part.
            h.get_by_label("Remove immediately on release").click();
            h.run_steps(2);
            h.get_all_by_label("Keep painting").next().unwrap().click();
            h.run_steps(2);
            h.state_mut().session.tools.brush.size = 10.0;
            h.state_mut().session.tools.brush.hardness = 1.0;
            let xf = crate::canvas::ViewXform::active(h.state()).unwrap();
            let a = xf.to_screen(82.0, 15.0);
            let b = xf.to_screen(82.0, 180.0);
            h.hover_at(a);
            h.step();
            h.event(Event::PointerButton { pos: a, button: PointerButton::Primary, pressed: true, modifiers: Modifiers::NONE });
            h.step();
            h.event(Event::PointerMoved(b));
            h.step();
            h.event(Event::PointerButton { pos: b, button: PointerButton::Primary, pressed: false, modifiers: Modifiers::NONE });
            h.run_steps(3);
            assert!(h.state().ai_remove.is_some());
            assert!(crate::ai_ui::ai_jobs(h.state()).is_empty(), "review drag must wait for confirmation");
            h.render().unwrap().save(exports.join("ui-reviewed-mask.png")).unwrap();
            let start = Instant::now();
            h.get_all_by_label("Remove").next().unwrap().click();
            h.run_steps(3);
            while !crate::ai_ui::ai_jobs(h.state()).is_empty() {
                assert!(start.elapsed() < Duration::from_secs(240));
                h.run_steps(1);
                h.render().unwrap();
                std::thread::sleep(Duration::from_millis(50));
            }
            assert_eq!(h.state().session.active().unwrap().history.past_len(), history + 1, "{}", h.state().ui.status);
            assert!(h.state().restoration.open, "successful repair opens optional Colourise");
            assert_eq!(h.state().restoration.task, Task::Colourise);
            eprintln!("RESTORE_UI confirmed_fill_ms={:.2}", start.elapsed().as_secs_f64() * 1000.);
            let repaired = photocraft_engine::ai_cmds::flatten_rgba(&h.state().session.active().unwrap().doc);
            repaired.save(exports.join("repaired.png")).unwrap();
            let changed = (15..180).flat_map(|y| (80..84).map(move |x| (x, y))).filter(|&(x, y)| repaired.get_pixel(x, y).0[..3] != [245, 245, 245]).count();
            assert!(changed > 330, "reviewed mask must repair most of the injected scratch: {changed}/660");
            eprintln!("RESTORE_UI reviewed_scratch_changed={changed}/660");
            let optional = Instant::now();
            while h.state().restoration.pending.is_some() {
                assert!(optional.elapsed() < Duration::from_secs(90));
                h.run_steps(1);
                h.render().unwrap();
                std::thread::sleep(Duration::from_millis(20));
            }
            assert!(h.state().restoration.result.is_some(), "{}", h.state().restoration.message);
            h.state().restoration.result.as_ref().unwrap().save(exports.join("repaired-colourised-preview.png")).unwrap();
            eprintln!("RESTORE_UI optional_colourise_ms={:.2}", optional.elapsed().as_secs_f64() * 1000.0);
            h.state_mut().run("edit.undo", json!({})).unwrap();
            assert_eq!(h.state().session.active().unwrap().doc, before);
        }
    }
}

#[test]
fn restoration_repair_followup_requires_success_and_same_active_document() {
    use photocraft_engine::jobs::{JobEvent, JobId, JobOutcome};
    let mut h = harness();
    let doc = h.state().session.active().unwrap().doc.id.0;
    for outcome in [JobOutcome::Failed("fill failed".into()), JobOutcome::Cancelled] {
        h.state_mut().restoration.repair = Some((JobId(42), doc));
        repair_finished(h.state_mut(), &JobEvent { id: JobId(42), command: "ai.remove".into(), label: "AI Remove".into(), document: None, outcome });
        assert!(!h.state().restoration.open);
        assert!(h.state().restoration.repair.is_none());
    }
    h.state_mut().restoration.repair = Some((JobId(42), doc));
    h.state_mut().run("file.new", json!({"width":10,"height":10})).unwrap();
    repair_finished(
        h.state_mut(),
        &JobEvent { id: JobId(42), command: "ai.remove".into(), label: "AI Remove".into(), document: None, outcome: JobOutcome::Done(json!({})) },
    );
    assert!(!h.state().restoration.open, "finished repair cannot open another document's preview");
}

#[test]
fn restoration_ui_import_rejects_unpinned_file_without_document_edits() {
    let mut h = harness();
    let before = h.state().session.active().unwrap().doc.clone();
    let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/restore-import-rejection");
    std::fs::create_dir_all(&dir).unwrap();
    let bad = dir.join("wrong.onnx");
    std::fs::write(&bad, b"not a model").unwrap();
    open_menu(&mut h);
    h.get_by_label("Colourise").click();
    h.run_steps(2);
    crate::ai_ui::TEST_PICK.with(|p| *p.borrow_mut() = Some(bad));
    h.get_by_label("Import Restore model…").click();
    h.run_steps(2);
    let start = Instant::now();
    while h.state().restoration.pending.is_some() {
        assert!(start.elapsed() < Duration::from_secs(10));
        std::thread::sleep(Duration::from_millis(5));
        h.run_steps(1);
    }
    assert!(h.state().restoration.message.contains("size mismatch"));
    assert_eq!(h.state().session.active().unwrap().doc, before);
    std::fs::remove_dir_all(dir).unwrap();
}
