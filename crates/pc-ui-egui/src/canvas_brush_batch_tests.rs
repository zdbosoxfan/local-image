//! Pen regressions through actual canvas press/move/release UI events.
use super::*;
use crate::stylus::PenSample;
use egui_kittest::Harness;
use serde_json::json;
type H = Harness<'static, PhotocraftApp>;
fn harness() -> H {
    let mut a = PhotocraftApp::new(photocraft_engine::Session::new(), Default::default());
    a.run("file.new", json!({"width":240,"height":100,"background":"transparent"})).unwrap();
    a.run("tools.setBrush", json!({"brush":{"size":30,"hardness":1.0,"pressureSize":true,"pressureOpacity":true,"smoothing":{"amount":0}}})).unwrap();
    a.ui.tool = Tool::Brush;
    a.ui.extras.rulers = false;
    a.ui.extras.snap = false;
    a.sync_views();
    a.ui.views[0].fit_pending = false;
    a.ui.views[0].zoom = 2.0;
    a.ui.views[0].center = [120.0, 50.0];
    let mut h = Harness::builder().with_size(vec2(1000.0, 700.0)).build_ui_state(
        |ui, a: &mut PhotocraftApp| {
            if ui.ctx().fonts(|f| f.families().contains(&egui::FontFamily::Name("medium".into()))) {
                egui::CentralPanel::default().show(ui, |ui| document_area(a, ui));
            }
        },
        a,
    );
    PhotocraftApp::setup_context(&h.ctx, Default::default());
    h.run_steps(4);
    h
}
fn pos(h: &H, x: f32, y: f32) -> Pos2 {
    let a = h.state();
    let v = &a.ui.views[0];
    ViewXform { rect: a.last_canvas_rect, zoom: v.zoom, center: v.center, flip: false, rotation: 0.0 }.to_screen(x, y)
}
fn pen(h: &H, p: f32) {
    h.state().stylus.feed.set(Some(PenSample { pressure: p, ..Default::default() }));
}
fn button(h: &mut H, x: f32, y: f32, pressed: bool) {
    let p = pos(h, x, y);
    h.input_mut().events.push(egui::Event::PointerMoved(p));
    h.input_mut().events.push(egui::Event::PointerButton { pos: p, button: PointerButton::Primary, pressed, modifiers: egui::Modifiers::NONE });
    h.run_steps(1);
}
fn points(h: &H) -> Vec<Vec<f64>> {
    serde_json::from_value(h.state().session.journal.iter().rev().find(|(id, _)| id == "paint.stroke").unwrap().1["points"].clone()).unwrap()
}
fn strokes(h: &H) -> usize {
    h.state().session.journal.iter().filter(|(id, _)| id == "paint.stroke").count()
}
#[test]
fn brush_batch_pen_tap_shorter_than_a_frame_keeps_contact_pressure() {
    let mut h = harness();
    pen(&h, 0.2);
    pen(&h, 0.0);
    h.state().stylus.feed.set(None);
    let p = pos(&h, 60.0, 50.0);
    h.input_mut().events.extend([
        egui::Event::PointerMoved(p),
        egui::Event::PointerButton { pos: p, button: PointerButton::Primary, pressed: true, modifiers: egui::Modifiers::NONE },
        egui::Event::PointerButton { pos: p, button: PointerButton::Primary, pressed: false, modifiers: egui::Modifiers::NONE },
    ]);
    h.run_steps(2);
    assert_eq!(strokes(&h), 1);
    let p = points(&h);
    assert!((p[0][2] - 0.2).abs() < 1e-6, "{p:?}");
    assert!(h.state().session.documents()[0].doc.layers[0].surface().unwrap().rgba(60, 50)[3] < 0.3);
}
#[test]
fn brush_batch_stationary_tap_uses_peak_and_commits_once() {
    let mut h = harness();
    pen(&h, 0.1);
    button(&mut h, 60.0, 50.0, true);
    assert!(h.state().drag.is_some(), "press starts a stroke");
    pen(&h, 0.6);
    h.run_steps(1);
    pen(&h, 0.0);
    button(&mut h, 60.0, 50.0, false);
    assert_eq!(strokes(&h), 1);
    assert!((points(&h)[0][2] - 0.6).abs() < 1e-6, "{:?}", points(&h));
    assert_eq!(h.state().session.documents()[0].history.past_len(), 1);
}
#[test]
fn brush_batch_moves_each_keep_pressure_and_lift_does_not_zero_tail() {
    let mut h = harness();
    pen(&h, 0.2);
    button(&mut h, 20.0, 50.0, true);
    for (x, p) in [(60.0, 0.4), (100.0, 0.6), (140.0, 0.8)] {
        pen(&h, p);
        let q = pos(&h, x, 50.0);
        h.input_mut().events.push(egui::Event::PointerMoved(q));
    }
    pen(&h, 0.0);
    h.run_steps(1);
    button(&mut h, 140.0, 50.0, false);
    let p = points(&h);
    let samples: Vec<_> = p.iter().filter(|p| p[0] >= 59.0).map(|p| (p[2] * 100.0).round() / 100.0).collect();
    assert_eq!(samples.get(..3), Some(&[0.4, 0.6, 0.8][..]), "{p:?}");
    assert_eq!(strokes(&h), 1);
}
#[test]
fn brush_batch_mouse_click_and_pressure_off_are_full_strength() {
    for disabled in [false, true] {
        let mut h = harness();
        if disabled {
            h.state_mut().session.prefs.edit(|p| p.tools.use_tablet_pressure = false);
            pen(&h, 0.15);
        }
        button(&mut h, 60.0, 50.0, true);
        button(&mut h, 60.0, 50.0, false);
        assert_eq!(points(&h)[0][2], 1.0);
        assert_eq!(strokes(&h), 1);
    }
}

/// Real 24 MP document, app UI, production live stroke and GPU canvas. No desktop window.
#[cfg(debug_assertions)]
#[test]
#[ignore = "opt-in real GPU large brush measurement"]
fn brush_batch_real_gpu_large_brush_performance() {
    use photocraft_engine::paint::{BrushSettings, Pattern, TipShape};
    use std::time::Instant;
    // Match the real application's startup before either paint or canvas touches rayon.
    let profile = lightcraft_sysmem::init(Default::default());
    eprintln!("BRUSH COMPUTE {profile}");
    let _guard = crate::gpu_test_support::lock();
    let mut setup = crate::gpu_canvas::wgpu_setup();
    crate::gpu_canvas::use_adapter_limits_with(&mut setup, |a| {
        let i = a.get_info();
        assert_eq!(i.backend, eframe::wgpu::Backend::Vulkan);
        assert_eq!(i.device_type, eframe::wgpu::DeviceType::DiscreteGpu);
        assert!(i.name.contains("RTX 5090"), "real GPU required: {i:?}");
        eprintln!("BRUSH GPU {i:?}");
    });
    let mut h = Harness::builder().with_size(vec2(1200.0, 900.0)).wgpu_setup(setup).build_eframe(|cc| {
        PhotocraftApp::setup_context(&cc.egui_ctx, Default::default());
        let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), Default::default());
        app.set_wgpu(cc.wgpu_render_state.as_ref().unwrap().clone());
        app
    });
    let (width, height, zoom) = if std::env::var("BIGBRUSH_60MP").is_ok() { (10000, 6000, 0.08) } else { (6000, 4000, 0.15) };
    let document = format!("{width}x{height} ({} MP)", width * height / 1_000_000);
    let mut results = Vec::new();
    for (label, size, textured) in [
        ("soft-300", 300.0, false),
        ("soft-1000", 1000.0, false),
        ("soft-2500", 2500.0, false),
        ("textured-dual-300", 300.0, true),
        ("textured-dual-1000", 1000.0, true),
        ("textured-dual-2500", 2500.0, true),
    ] {
        h.state_mut().run("file.new", json!({"width":width,"height":height,"background":"white"})).unwrap();
        let mut b = BrushSettings { size, hardness: 0.0, spacing: 0.1, pressure_size: false, ..Default::default() };
        if textured {
            b.texture.enabled = true;
            b.texture.each_tip = true;
            b.texture.pattern = Pattern::Tile(photocraft_engine::paint::procedural::chalk_tip(128, 42));
            b.dual_brush.enabled = true;
            b.dual_brush.size = size * 0.8;
            b.dual_brush.tip = TipShape::Sampled(photocraft_engine::paint::procedural::bristle_tip(128, 7));
        }
        h.state_mut().run("tools.setBrush", json!({"brush":b})).unwrap();
        h.state_mut().ui.tool = Tool::Brush;
        h.state_mut().ui.extras.rulers = false;
        h.state_mut().ui.extras.snap = false;
        h.run_steps(3);
        let index = h.state().session.active_index().unwrap();
        let v = &mut h.state_mut().ui.views[index];
        v.zoom = zoom;
        v.center = [3000.0, 2000.0];
        v.fit_pending = false;
        h.run_steps(3);
        h.render().unwrap();
        let screen = |h: &H, x: f32| {
            let a = h.state();
            let v = &a.ui.views[index];
            ViewXform { rect: a.last_canvas_rect, zoom: v.zoom, center: v.center, flip: false, rotation: v.rotation }.to_screen(x, 2000.0)
        };
        let start = screen(&h, 2000.0);
        h.input_mut().events.extend([
            egui::Event::PointerMoved(start),
            egui::Event::PointerButton { pos: start, button: PointerButton::Primary, pressed: true, modifiers: egui::Modifiers::NONE },
        ]);
        h.step();
        h.render().unwrap();
        let mut ui_ms = Vec::new();
        let mut gpu_ms = Vec::new();
        let mut dabs = 0;
        let mut dual = 0;
        let mut raster = 0.0;
        let mut stages = [0.0; 5];
        let mut worker_ms = 0.0;
        let mut worker_batches = 0;
        let mut preview_latency_ms: f64 = 0.0;
        for i in 0..40 {
            let frame_started = Instant::now();
            let p = screen(&h, 2050.0 + i as f32 * 40.0);
            h.input_mut().events.push(egui::Event::PointerMoved(p));
            let start = Instant::now();
            let (_, timing) = photocraft_engine::paint::profiling::measure(|| h.step());
            let elapsed = start.elapsed().as_secs_f64() * 1000.0;
            let start = Instant::now();
            let image = h.render().expect("real rendered app frame");
            let gpu = start.elapsed().as_secs_f64() * 1000.0;
            assert!(image.width() > 0);
            // A worker path must be measured at a realistic input rate, not by racing through
            // all pointer samples before it can publish a preview. Waiting is outside UI timing.
            let cadence = std::time::Duration::from_millis(16);
            if let Some(wait) = cadence.checked_sub(frame_started.elapsed()) {
                std::thread::sleep(wait);
            }
            if i >= 5 {
                ui_ms.push(elapsed);
                gpu_ms.push(gpu);
                dabs += timing.primary_dabs;
                dual += timing.dual_dabs;
                raster += timing.raster_ms;
                worker_ms += timing.worker_ms;
                worker_batches += timing.worker_batches;
                preview_latency_ms = preview_latency_ms.max(timing.preview_latency_ms);
                for (sum, ms) in stages.iter_mut().zip([timing.tail_ms, timing.restore_ms, timing.coverage_ms, timing.composite_ms, timing.gpu_upload_ms]) {
                    *sum += ms;
                }
            }
        }
        let p = screen(&h, 2050.0 + 39.0 * 40.0);
        h.input_mut().events.push(egui::Event::PointerButton { pos: p, button: PointerButton::Primary, pressed: false, modifiers: egui::Modifiers::NONE });
        let release = Instant::now();
        h.step();
        h.render().unwrap();
        settle_stroke(&mut h);
        let release_ms = release.elapsed().as_secs_f64() * 1000.0;
        let finished = h.render().expect("GPU-rendered committed stroke");
        let painted = screen(&h, 2700.0) * h.ctx.pixels_per_point();
        let (cx, cy) = (painted.x as u32, painted.y as u32);
        let mut painted_pixels = 0;
        for y in cy.saturating_sub(5)..(cy + 6).min(finished.height()) {
            for x in cx.saturating_sub(5)..(cx + 6).min(finished.width()) {
                if finished.get_pixel(x, y).0[..3].iter().all(|&c| c < 245) {
                    painted_pixels += 1;
                }
            }
        }
        assert!(painted_pixels > 0, "{label}: actual GPU frame must show ink along the stroke");
        assert!(h.state().perf.gpu, "GPU canvas must be active");
        assert!(h.state().perf.gpu_fallback.is_none(), "{:?}", h.state().perf.gpu_fallback);
        assert!(dabs > 0, "real paint engine profiling required");
        let mean = ui_ms.iter().sum::<f64>() / ui_ms.len() as f64;
        ui_ms.sort_by(f64::total_cmp);
        let p95 = ui_ms[(ui_ms.len() * 95).div_ceil(100) - 1];
        let r = json!({"brush":label,"document":document,"measured_frames":ui_ms.len(),"gpu_painted_pixels":painted_pixels,"release_completion_ms":release_ms,"worker_batches":worker_batches,"worker_ms_per_batch":worker_ms/worker_batches.max(1) as f64,"max_preview_latency_ms":preview_latency_ms,"primary_dabs":dabs,"dual_dabs":dual,"raster_ms_per_primary_dab":raster/dabs as f64,"raster_ms_per_frame":raster/ui_ms.len() as f64,"stage_ms_per_frame":stages.map(|s| s/ui_ms.len() as f64),"ui_frame_mean_ms":mean,"ui_frame_p95_ms":p95,"gpu_render_readback_mean_ms":gpu_ms.iter().sum::<f64>()/gpu_ms.len() as f64});
        eprintln!("BRUSH PERFORMANCE {r}");
        results.push(r);
    }
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/brush-work/performance.json");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, serde_json::to_vec_pretty(&results).unwrap()).unwrap();
}

#[test]
fn brush_batch_stroke_path_dialog_clicks_tool_and_simulate_pressure() {
    use egui_kittest::kittest::Queryable;
    let mut a = PhotocraftApp::new(photocraft_engine::Session::new(), Default::default());
    a.run("file.new", json!({"width":200,"height":100,"background":"transparent"})).unwrap();
    a.run("path.set", json!({"path":{"subpaths":[{"closed":false,"knots":[[20,50],[180,50]]}]}})).unwrap();
    a.ui.tool = Tool::Pen;
    a.sync_views();
    let mut h = Harness::builder().with_size(vec2(1000.0, 700.0)).build_ui_state(
        |ui, a: &mut PhotocraftApp| {
            if !ui.ctx().fonts(|f| f.families().contains(&egui::FontFamily::Name("medium".into()))) {
                return;
            }
            let ctx = ui.ctx().clone();
            egui::CentralPanel::default().show(ui, |ui| document_area(a, ui));
            crate::dialogs::show(a, &ctx);
        },
        a,
    );
    PhotocraftApp::setup_context(&h.ctx, Default::default());
    h.run_steps(4);
    let p = h.state().last_canvas_rect.center();
    h.hover_at(p);
    h.run_steps(1);
    for pressed in [true, false] {
        h.input_mut().events.push(egui::Event::PointerButton { pos: p, button: PointerButton::Secondary, pressed, modifiers: egui::Modifiers::NONE });
        h.run_steps(1);
    }
    h.run();
    h.get_by_label("Stroke Path…").click();
    h.run();
    h.get_by_value("Brush").click();
    h.run();
    h.get_by_label("Pencil").click();
    h.run();
    h.get_by_label("Simulate Pressure").click();
    h.run_steps(1);
    h.get_by_label("OK").click();
    h.run();
    let (_, params) = h.state().session.journal.iter().rev().find(|(id, _)| id == "path.stroke").unwrap();
    assert_eq!(params["tool"], "pencil");
    assert_eq!(params["simulatePressure"], true);
}

fn worker_harness() -> H {
    let mut h = harness();
    h.state_mut()
        .run("tools.setBrush", json!({"brush":{"size":600,"hardness":0.0,"pressureSize":false,"pressureOpacity":true,"smoothing":{"amount":0.2}}}))
        .unwrap();
    h
}
fn settle_stroke(h: &mut H) {
    let end = std::time::Instant::now() + std::time::Duration::from_secs(15);
    while h.state().live_stroke.is_some() {
        assert!(std::time::Instant::now() < end, "stroke worker did not finish");
        h.run_steps(1);
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
}
#[test]
fn bigbrush_worker_drag_commits_exact_pixels_once_and_undoes() {
    let mut h = worker_harness();
    let base = h.state().session.active().unwrap().doc.clone();
    let before = strokes(&h);
    let mut replay = photocraft_engine::Session::new();
    replay.execute("file.new", json!({"width":240,"height":100,"background":"transparent"})).unwrap();
    replay.execute("tools.setBrush", json!({"brush":h.state().session.tools.brush})).unwrap();
    button(&mut h, 20.0, 50.0, true);
    assert!(h.state().live_stroke.as_ref().unwrap().worker.is_some());
    for (x, p) in [(60.0, 0.3), (100.0, 0.7), (140.0, 0.5)] {
        pen(&h, p);
        let q = pos(&h, x, 50.0);
        h.input_mut().events.push(egui::Event::PointerMoved(q));
        h.run_steps(1);
    }
    button(&mut h, 140.0, 50.0, false);
    settle_stroke(&mut h);
    assert_eq!(strokes(&h), before + 1);
    assert_eq!(h.state().session.live_commits_reused(), 1);
    let params = h.state().session.journal.iter().rev().find(|(cmd, _)| cmd == "paint.stroke").unwrap().1.clone();
    let mut params = params;
    params["layer"] = json!(replay.active().unwrap().active_layer.unwrap().0);
    replay.execute("paint.stroke", params).unwrap();
    let a = h.state().session.active().unwrap().doc.layers[0].surface().unwrap();
    let b = replay.active().unwrap().doc.layers[0].surface().unwrap();
    let area = a.tile_bounds().union(&b.tile_bounds());
    let (av, bv) = (a.read_region(area), b.read_region(area));
    assert!(av == bv, "first differing sample {:?} over {area:?}", av.iter().zip(&bv).position(|(a, b)| a != b));
    h.state_mut().run("edit.undo", json!({})).unwrap();
    assert_eq!(h.state().session.active().unwrap().doc.layers[0].surface(), base.layers[0].surface());
}
#[test]
fn bigbrush_worker_escape_discards_inflight_and_finished_pending_strokes() {
    for release in [false, true] {
        let mut h = worker_harness();
        let base = h.state().session.active().unwrap().doc.clone();
        let before = strokes(&h);
        button(&mut h, 40.0, 50.0, true);
        let q = pos(&h, 160.0, 50.0);
        h.input_mut().events.push(egui::Event::PointerMoved(q));
        h.run_steps(1);
        if release {
            button(&mut h, 160.0, 50.0, false);
        }
        h.key_press(egui::Key::Escape);
        h.run_steps(2);
        assert!(h.state().live_stroke.is_none());
        assert!(h.state().drag.is_none());
        assert_eq!(strokes(&h), before);
        assert_eq!(h.state().session.active().unwrap().doc.layers[0].surface(), base.layers[0].surface());
        std::thread::sleep(std::time::Duration::from_millis(50));
        h.run_steps(2);
        assert_eq!(strokes(&h), before, "cancelled worker cannot publish a late commit");
    }
}
#[test]
fn bigbrush_worker_fast_following_tap_is_not_lost() {
    let mut h = worker_harness();
    h.state_mut().run("tools.setBrush", json!({"brush":{"size":2500}})).unwrap();
    let before = strokes(&h);
    pen(&h, 0.25);
    button(&mut h, 40.0, 50.0, true);
    button(&mut h, 40.0, 50.0, false);
    h.state().stylus.feed.set(Some(PenSample { pressure: 0.8, tilt_x: 35.0, rotation: 70.0, ..Default::default() }));
    button(&mut h, 180.0, 50.0, true);
    button(&mut h, 180.0, 50.0, false);
    settle_stroke(&mut h);
    assert_eq!(strokes(&h), before + 2);
    assert_eq!(h.state().session.live_commits_reused(), 2);
    let p = points(&h);
    assert_eq!(p[0][2], 0.8_f32 as f64);
    assert_eq!(&p[0][3..6], &[35.0, 0.0, 70.0]);
}
#[test]
fn bigbrush_worker_stale_document_never_commits() {
    let mut h = worker_harness();
    button(&mut h, 40.0, 50.0, true);
    button(&mut h, 160.0, 50.0, false);
    h.state_mut().run("edit.fill", json!({"color":"#00ff00"})).unwrap();
    let after_edit = h.state().session.active().unwrap().doc.clone();
    h.run_steps(2);
    assert!(h.state().live_stroke.is_none());
    std::thread::sleep(std::time::Duration::from_millis(50));
    h.run_steps(2);
    assert_eq!(strokes(&h), 0);
    assert_eq!(h.state().session.active().unwrap().doc.layers[0].surface(), after_edit.layers[0].surface());
}

#[test]
fn bigbrush_worker_changed_settings_are_rerendered_off_ui_and_match_commit() {
    let mut h = worker_harness();
    button(&mut h, 20.0, 50.0, true);
    let q = pos(&h, 140.0, 50.0);
    h.input_mut().events.push(egui::Event::PointerMoved(q));
    h.run_steps(1);
    // The original live renderer now differs from what the existing command will commit.
    h.state_mut().run("tools.setBrush", json!({"brush":{"size":700,"wetEdges":true}})).unwrap();
    button(&mut h, 140.0, 50.0, false);
    settle_stroke(&mut h);
    assert_eq!(h.state().session.live_commits_reused(), 1, "replacement worker supplies the full commit; no UI-thread raster fallback");
    let mut replay = photocraft_engine::Session::new();
    replay.execute("file.new", json!({"width":240,"height":100,"background":"transparent"})).unwrap();
    replay.execute("tools.setBrush", json!({"brush":h.state().session.tools.brush})).unwrap();
    let params = h.state().session.journal.iter().rev().find(|(cmd, _)| cmd == "paint.stroke").unwrap().1.clone();
    let mut params = params;
    params["layer"] = json!(replay.active().unwrap().active_layer.unwrap().0);
    replay.execute("paint.stroke", params).unwrap();
    let a = h.state().session.active().unwrap().doc.layers[0].surface().unwrap();
    let b = replay.active().unwrap().doc.layers[0].surface().unwrap();
    let region = a.tile_bounds().union(&b.tile_bounds());
    assert!(a.read_region(region) == b.read_region(region));
}

#[test]
fn bigbrush_worker_freezes_settings_at_release_and_queued_press() {
    let mut h = worker_harness();
    h.state_mut().run("tools.setColors", json!({"foreground":"#ff0000","background":"#0000ff"})).unwrap();
    button(&mut h, 40.0, 50.0, true);
    button(&mut h, 40.0, 50.0, false);
    h.state_mut().run("tools.setColors", json!({"foreground":"#00ff00","background":"#ffff00"})).unwrap();
    button(&mut h, 180.0, 50.0, true);
    button(&mut h, 180.0, 50.0, false);
    h.state_mut().run("tools.setBrush", json!({"brush":{"size":20}})).unwrap();
    h.state_mut().run("tools.setColors", json!({"foreground":"#0000ff","background":"#ff00ff"})).unwrap();
    settle_stroke(&mut h);
    let commits: Vec<_> = h.state().session.journal.iter().filter(|(cmd, _)| cmd == "paint.stroke").map(|(_, p)| p.clone()).collect();
    assert_eq!(commits.len(), 2);
    assert_eq!(commits[0]["color"], json!([1.0, 0.0, 0.0, 1.0]));
    assert_eq!(commits[1]["color"], json!([0.0, 1.0, 0.0, 1.0]));
    assert_eq!(commits[0]["background"], json!([0.0, 0.0, 1.0, 1.0]));
    assert_eq!(commits[1]["background"], json!([1.0, 1.0, 0.0, 1.0]));
    assert_eq!(commits[1]["brush"]["size"], 600.0);
    assert_eq!(h.state().session.live_commits_reused(), 2);
    let mut replay = photocraft_engine::Session::new();
    replay.execute("file.new", json!({"width":240,"height":100,"background":"transparent"})).unwrap();
    // Deliberately different globals: the command carries everything needed to replay its pixels.
    for mut p in commits {
        p["layer"] = json!(replay.active().unwrap().active_layer.unwrap().0);
        replay.execute("paint.stroke", p).unwrap();
    }
    let a = h.state().session.active().unwrap().doc.layers[0].surface().unwrap();
    let b = replay.active().unwrap().doc.layers[0].surface().unwrap();
    let region = a.tile_bounds().union(&b.tile_bounds());
    assert!(a.read_region(region) == b.read_region(region));
}

#[test]
fn bigbrush_worker_following_airbrush_keeps_idle_samples_and_times() {
    let mut h = worker_harness();
    h.state_mut().run("tools.setBrush", json!({"brush":{"size":2500,"buildUp":true,"flow":0.2}})).unwrap();
    button(&mut h, 40.0, 50.0, true);
    button(&mut h, 40.0, 50.0, false);
    button(&mut h, 160.0, 50.0, true);
    for _ in 0..4 {
        std::thread::sleep(std::time::Duration::from_millis(18));
        h.run_steps(1);
    }

    button(&mut h, 160.0, 50.0, false);
    settle_stroke(&mut h);
    let p = points(&h);
    assert!(p.len() >= 3, "idle airbrush points must survive the preceding commit: {p:?}");
    assert!(p.iter().skip(1).any(|p| p.get(6).is_some_and(|&t| t >= 16.0)), "{p:?}");
    assert_eq!(h.state().session.live_commits_reused(), 2);
}
