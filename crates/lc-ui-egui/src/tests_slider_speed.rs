//! Develop slider drag latency, measured through the headless app (the same UI code, render
//! queue, engine jobs and GPU/CPU renderer the desktop app runs). The automatic CPU and GPU
//! tests use a procedural photo; the ignored continuous-drag bench optionally takes a real photo.
//!
//! ```sh
//! LC_SLIDER_BENCH_FILE=/path/photo.arw cargo +1.98.1 test --offline --release -p lightcraft-ui-egui \
//!   tests_slider_speed -- --ignored --nocapture --test-threads=1
//! ```
//! `LIGHTCRAFT_GPU=0` measures the CPU pipeline; `LC_SLIDER_BENCH_PPP=2` a HiDPI display;
//! `LC_SLIDER_BENCH_CONTROL=effects.clarity` another slider. Omit the file to use the procedural
//! photo. `LC_REQUIRE_GPU=1` makes a missing hardware adapter a failure.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use serde_json::json;

use crate::LightcraftApp;
use crate::headless::Headless;
use crate::render::Slot;

fn pct(v: &mut [f64], p: f64) -> f64 {
    if v.is_empty() {
        return f64::NAN;
    }
    v.sort_by(|a, b| a.total_cmp(b));
    v[((v.len() - 1) as f64 * p).round() as usize]
}

fn mean(v: &[f64]) -> f64 {
    v.iter().sum::<f64>() / v.len().max(1) as f64
}

#[test]
#[ignore = "continuous 120-step timing benchmark; optional LC_SLIDER_BENCH_FILE"]
fn develop_slider_drag_latency() {
    let _gpu_gate = crate::gpu_test_support::lock();
    let ppp: f32 = std::env::var("LC_SLIDER_BENCH_PPP").ok().and_then(|v| v.parse().ok()).unwrap_or(1.0);
    let control = std::env::var("LC_SLIDER_BENCH_CONTROL").unwrap_or_else(|_| "light.exposure".into());
    let gpu = lightcraft_engine::gpu::available();
    eprintln!("renderer: {}", if gpu { lightcraft_engine::gpu::adapter_name().unwrap_or_default() } else { "CPU".into() });
    assert!(gpu || std::env::var_os("LC_REQUIRE_GPU").is_none(), "hardware GPU required");
    let session = match std::env::var("LC_SLIDER_BENCH_FILE") {
        Ok(path) => {
            let mut s = lightcraft_engine::Session::new().with_fs();
            s.execute("library.import", &json!({"paths": [path]})).expect("import");
            s
        }
        Err(_) => procedural_session(),
    };
    let app = LightcraftApp::new(session, crate::Services { png: None, ..Default::default() });
    let size: Vec<f32> =
        std::env::var("LC_SLIDER_BENCH_SIZE").unwrap_or_else(|_| "3840x2160".into()).split('x').filter_map(|v| v.parse().ok()).collect();
    let mut h = Headless::new(app, [size[0] / ppp, size[1] / ppp], ppp);
    if let Ok(partial) = std::env::var("LC_SLIDER_BENCH_SETTINGS") {
        let partial: serde_json::Value = serde_json::from_str(&partial).expect("settings JSON");
        h.app.session.execute("develop.merge", &json!({"settings": partial})).expect("merge");
    }
    let t = Duration::from_secs(30);
    h.request("ui.set", json!({"view": "detail", "right": "edit"}), t);
    // Keep the chosen primary section visible, including the Effects example in the docs.
    let section = match control.split('.').next().unwrap_or("light") {
        "wb" => "color",
        "vignette" | "grain" => "effects",
        section => section,
    };
    h.app.ui.open_sections = vec![section.into()];
    let t0 = Instant::now();
    assert!(h.step_until(Duration::from_secs(120), |h| h.app.renderer.textures.contains_key(&Slot::Main) && !h.app.renderer.is_pending(Slot::Main)));
    h.settle(Duration::from_secs(60));
    eprintln!("first loupe render: {:.0} ms", t0.elapsed().as_secs_f64() * 1e3);
    let main = h.app.renderer.textures.get(&Slot::Main).expect("loupe");
    eprintln!("loupe texture {}×{} ({:.1} ms)", main.size[0], main.size[1], main.ms);
    let rect = h.app.widgets.iter().find(|(id, _)| *id == format!("slider:{control}")).map(|(_, r)| *r).expect("slider on screen");

    // drag across the slider for ~2 s at 60 Hz, one pointer move per frame (like a real drag)
    let frame = Duration::from_micros(16_667);
    let start = egui::pos2(rect.left() + rect.width() * 0.5, rect.center().y);
    h.app.synthetic.push(egui::Event::PointerMoved(start));
    h.app.synthetic.push(egui::Event::PointerButton {
        pos: start,
        button: egui::PointerButton::Primary,
        pressed: true,
        modifiers: Default::default(),
    });
    let steps = 120;
    for i in 0..steps {
        // one way across (every value is new: each render key is requested once)
        let f = 0.5 + 0.4 * (i as f32 / steps as f32);
        h.app.synthetic.push(egui::Event::PointerMoved(egui::pos2(rect.left() + rect.width() * f, rect.center().y)));
    }
    let end = egui::pos2(rect.left() + rect.width() * 0.9, rect.center().y);
    h.app.synthetic.push(egui::Event::PointerMoved(end));
    h.app.synthetic.push(egui::Event::PointerButton {
        pos: end,
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers: Default::default(),
    });

    let mut requested: HashMap<u64, Instant> = HashMap::new();
    let mut frame_ms = Vec::new();
    let mut latency = Vec::new();
    let mut job_ms = Vec::new();
    let mut shown_key = h.app.renderer.textures.get(&Slot::Main).map(|t| t.key);
    let mut updates = 0;
    let drag_start = Instant::now();
    let mut released = None;
    let mut values = Vec::new();
    let mut last_value = None;
    while !h.app.synthetic.is_empty() || released.is_none() {
        let f0 = Instant::now();
        h.step();
        frame_ms.push(f0.elapsed().as_secs_f64() * 1e3);
        if let Some(k) = h.app.renderer.wanted(Slot::Main) {
            requested.entry(k).or_insert(f0);
        }
        let tex = h.app.renderer.textures.get(&Slot::Main).expect("loupe");
        if Some(tex.key) != shown_key {
            if h.app.session.interaction.is_some() {
                assert!(tex.size[0].max(tex.size[1]) <= crate::render::DRAFT_EDGE, "continuous drag displays a bounded preview");
            }
            shown_key = Some(tex.key);
            updates += 1;
            job_ms.push(tex.ms);
            if let Some(r) = requested.get(&tex.key) {
                latency.push(r.elapsed().as_secs_f64() * 1e3);
            }
        }
        let a = h.app.session.active().expect("active");
        let value = lightcraft_develop::controls::get(&h.app.session.develop_of(a).expect("settings"), &control).expect("control");
        values.push(value);
        if last_value != Some(value) {
            eprintln!(
                "drag step control={control} value={value:.2} UI={:.2}ms texture_job={:.2}ms texture_size={:?}",
                frame_ms.last().unwrap(),
                tex.ms,
                tex.size
            );
            last_value = Some(value);
        }
        if h.app.synthetic.is_empty() && released.is_none() {
            released = Some(Instant::now());
        }
        if let Some(rest) = frame.checked_sub(f0.elapsed()) {
            std::thread::sleep(rest);
        }
    }
    let drag_s = drag_start.elapsed().as_secs_f64();
    let released = released.expect("released");
    // the settled (full-quality) render after the release
    let final_key = loop {
        h.step();
        if let Some(k) = h.app.renderer.wanted(Slot::Main)
            && !h.app.renderer.is_pending(Slot::Main)
        {
            break k;
        }
        if released.elapsed() > Duration::from_secs(60) {
            panic!("final render did not finish");
        }
        std::thread::sleep(Duration::from_millis(1));
    };
    let settle_ms = released.elapsed().as_secs_f64() * 1e3;
    let tex = h.app.renderer.textures.get(&Slot::Main).expect("loupe");
    assert_eq!(tex.key, final_key);
    values.dedup();
    eprintln!("control {control}: {} distinct values over {} frames ({drag_s:.2} s)", values.len(), frame_ms.len());
    eprintln!("UI frame: mean {:.1} ms, p95 {:.1} ms, max {:.1} ms", mean(&frame_ms), pct(&mut frame_ms.clone(), 0.95), pct(&mut frame_ms, 1.0));
    eprintln!("loupe updates during drag: {updates} ({:.1}/s)", updates as f64 / drag_s);
    eprintln!("render job: mean {:.1} ms, p95 {:.1} ms", mean(&job_ms), pct(&mut job_ms, 0.95));
    eprintln!("change → texture ready: mean {:.1} ms, p95 {:.1} ms", mean(&latency), pct(&mut latency, 0.95));
    eprintln!("release → final texture ready: {settle_ms:.0} ms ({}×{}, {:.1} ms job)", tex.size[0], tex.size[1], tex.ms);
    if let Some(f) = lightcraft_engine::gpu::last_fallback() {
        eprintln!("gpu fallback: {f}");
    }
    assert!(updates > 0, "the loupe never updated during the drag");
}

fn procedural_session() -> lightcraft_engine::Session {
    use lightcraft_catalog::{Op, Photo, PhotoId, Source};
    use lightcraft_raster::Rgb32f;
    use std::sync::Arc;
    let mut s = lightcraft_engine::Session::new();
    let id = PhotoId(1);
    let p = Photo::new(id, Source::File { path: "slider-speed-procedural.jpg".into() }, "procedural.jpg", "JPEG", 6000, 4000, "2026-10-09");
    s.catalog.apply(Op::AddPhoto { photo: Box::new(p) }).unwrap();
    s.selection = lightcraft_engine::Selection::single(id);
    s.media.file_loader = Some(Arc::new(|_, edge| {
        let w = edge.min(6000);
        let h = w * 2 / 3;
        let image = Rgb32f::from_fn(w, h, |x, y| {
            let (x, y) = (x as f32 / w as f32, y as f32 / h as f32);
            let v = 0.02 + 1.8 * x * x + 0.03 * (x * 300.0).sin() * (y * 160.0).cos();
            [v.max(0.002), (v * (0.65 + y * 0.3)).max(0.002), (v * 0.5 + y * 0.15).max(0.002)]
        });
        Ok((image, Default::default()))
    }));
    s
}

fn headless(gpu: bool, size: [f32; 2], ppp: f32) -> Headless {
    let mut app = LightcraftApp::new(procedural_session(), crate::Services { png: None, ..Default::default() });
    app.ui.settings.gpu = gpu;
    app.ui.view = crate::state::ViewMode::Detail;
    app.ui.right = crate::state::RightPanel::Edit;
    app.ui.left_panel = false;
    for section in ["light", "effects", "color"] {
        if !app.ui.section_open(section) {
            app.ui.toggle_section(section);
        }
    }
    let mut h = Headless::new(app, size, ppp);
    for _ in 0..3 {
        h.step();
    }
    h
}

fn slider_rect(h: &Headless, control: &str) -> egui::Rect {
    h.app.widgets.iter().find(|(id, _)| id == &format!("slider:{control}")).map(|(_, r)| *r).expect("slider on screen")
}

fn pointer(h: &mut Headless, p: egui::Pos2) {
    h.app.synthetic.push(egui::Event::PointerMoved(p));
    h.step();
}

fn button(h: &mut Headless, p: egui::Pos2, pressed: bool, modifiers: egui::Modifiers) {
    h.app.synthetic.push(egui::Event::PointerButton { pos: p, button: egui::PointerButton::Primary, pressed, modifiers });
    h.step();
}

fn value(h: &Headless, control: &str) -> f64 {
    lightcraft_develop::controls::get(&h.app.session.develop_of(h.app.session.active().unwrap()).unwrap(), control).unwrap()
}

fn wait_main(h: &mut Headless) {
    assert!(
        h.step_until(Duration::from_secs(120), |h| {
            h.app.renderer.textures.get(&Slot::Main).is_some_and(|t| Some(t.key) == h.app.renderer.wanted(Slot::Main))
                && !h.app.renderer.is_pending(Slot::Main)
        }),
        "latest main render did not finish: {:?}",
        h.app.renderer.failure(Slot::Main)
    );
}

/// Record actual UI-generated jobs without executing expensive full-size requests. Rendering
/// and queue execution are exercised separately by the timing tests and JobPool regressions.
#[derive(Clone)]
struct Capture(std::rc::Rc<std::cell::RefCell<Vec<(Slot, lightcraft_engine::media::RenderJob)>>>);
impl crate::render::RenderOffload for Capture {
    fn try_start(&mut self, slot: Slot, job: lightcraft_engine::media::RenderJob) -> Option<lightcraft_engine::media::RenderJob> {
        self.0.borrow_mut().push((slot, job));
        None
    }
    fn finished(&mut self) -> Vec<(Slot, lightcraft_engine::media::RenderResult, f64)> {
        vec![]
    }
}

#[test]
fn slider_drag_caps_hidpi_zoom_jobs_and_release_restores_full_quality() {
    let _gpu_gate = crate::gpu_test_support::lock();
    let mut app = LightcraftApp::new(procedural_session(), crate::Services { png: None, ..Default::default() });
    app.ui.settings.gpu = false;
    app.ui.settings.preview_edge = 8192;
    app.ui.zoom = crate::state::Zoom::Percent(200);
    app.ui.view = crate::state::ViewMode::Detail;
    app.ui.right = crate::state::RightPanel::Edit;
    app.ui.left_panel = false;
    let work = Capture(Default::default());
    app.renderer.set_offload(Box::new(work.clone()));
    let mut h = Headless::new(app, [1800.0, 1200.0], 2.0);
    for _ in 0..3 {
        h.step();
    }
    let rect = slider_rect(&h, "light.exposure");
    let p = |f| egui::pos2(rect.left() + rect.width() * f, rect.center().y);
    let undo = h.app.session.undo.len();
    button(&mut h, p(0.5), true, Default::default());
    pointer(&mut h, p(0.6));
    assert!(h.app.session.interaction.is_some());
    h.app.ui.second_window = true; // embedded viewport in the headless harness
    let thumbs = h.app.renderer.thumb_jobs_built;
    work.0.borrow_mut().clear();
    for f in [0.65, 0.7, 0.75] {
        pointer(&mut h, p(f));
    }
    assert_eq!(h.app.renderer.thumb_jobs_built, thumbs, "filmstrip updates wait until release");
    let jobs = work.0.borrow();
    assert!(!jobs.iter().any(|(s, _)| *s == Slot::Second), "the second window shares the main draft during a drag");
    let drafts: Vec<_> = jobs.iter().filter(|(s, _)| *s == Slot::Main).map(|(_, j)| j).collect();
    assert_eq!(drafts.len(), 3, "every pointer step reaches the renderer");
    for job in drafts {
        assert_eq!(job.request.quality, lightcraft_engine::pipeline::Quality::Draft);
        assert!(job.request.max_w.max(job.request.max_h) <= crate::render::DRAFT_EDGE);
        assert_eq!(job.level, lightcraft_engine::media::SourceLevel::Preview);
    }
    drop(jobs);
    // A different release position is also applied; it is part of the same history step.
    button(&mut h, p(0.8), false, Default::default());
    assert!((value(&h, "light.exposure") - 3.0).abs() < 0.01);
    assert!(h.app.session.interaction.is_none());
    assert_eq!(h.app.session.undo.len(), undo + 1);
    let jobs = work.0.borrow();
    let final_job = jobs.iter().rev().find(|(s, _)| *s == Slot::Main).unwrap();
    assert_eq!(final_job.1.request.quality, lightcraft_engine::pipeline::Quality::Full);
    assert_eq!(final_job.1.request.max_w, 8192);
    let second = jobs.iter().rev().find(|(s, _)| *s == Slot::Second).expect("second window settles after release");
    assert_eq!(second.1.request.quality, lightcraft_engine::pipeline::Quality::Full);
    drop(jobs);
    h.app.session.execute("edit.undo", &json!({})).unwrap();
    assert_eq!(value(&h, "light.exposure"), 0.0);
    lightcraft_engine::gpu::set_enabled(true);
}

#[test]
fn slider_fine_drag_accumulates_substep_motion_and_missing_widget_releases() {
    let _gpu_gate = crate::gpu_test_support::lock();
    let mut h = headless(false, [1200.0, 1000.0], 1.0);
    let work = Capture(Default::default());
    h.app.renderer.set_offload(Box::new(work));
    let rect = slider_rect(&h, "light.contrast");
    let from = rect.center();
    let undo = h.app.session.undo.len();
    button(&mut h, from, true, egui::Modifiers::SHIFT);
    for i in 1..=40 {
        pointer(&mut h, from + egui::vec2(i as f32 * 0.5, 0.0));
    }
    let expected = (20.0 * 200.0 / rect.width() as f64 * 0.1).round();
    assert_eq!(value(&h, "light.contrast"), expected);
    assert!(expected >= 1.0, "motion crosses a step after accumulation");
    h.app.ui.right = crate::state::RightPanel::None;
    button(&mut h, from + egui::vec2(20.0, 0.0), false, egui::Modifiers::SHIFT);
    assert!(h.app.session.interaction.is_none(), "a hidden slider must leave draft mode on release");
    assert!(h.app.ui.dragging_control.is_none());
    assert_eq!(h.app.session.undo.len(), undo + 1);
    h.app.session.execute("edit.undo", &json!({})).unwrap();
    assert_eq!(value(&h, "light.contrast"), 0.0);
    drop(h);
    lightcraft_engine::gpu::set_enabled(true);
}

#[test]
fn slider_gesture_cannot_continue_into_another_photo_after_selection() {
    let _gpu_gate = crate::gpu_test_support::lock();
    use lightcraft_catalog::{Op, Photo, PhotoId, Source};
    let mut h = headless(false, [1200.0, 1000.0], 1.0);
    h.app.renderer.set_offload(Box::new(Capture(Default::default())));
    let p = Photo::new(PhotoId(2), Source::File { path: "second-procedural.jpg".into() }, "second.jpg", "JPEG", 6000, 4000, "2026-10-09");
    h.app.session.catalog.apply(Op::AddPhoto { photo: Box::new(p) }).unwrap();
    let rect = slider_rect(&h, "light.exposure");
    let p = |f| egui::pos2(rect.left() + rect.width() * f, rect.center().y);
    button(&mut h, p(0.5), true, Default::default());
    pointer(&mut h, p(0.65));
    let edited = value(&h, "light.exposure");
    assert!(edited > 1.0);
    let r = h.request("engine.execute", json!({"command": "library.select", "params": {"ids": [2]}}), Duration::from_secs(10));
    assert_eq!(r["ok"], true, "{r}");
    pointer(&mut h, p(0.9));
    button(&mut h, p(0.9), false, Default::default());
    assert_eq!(value(&h, "light.exposure"), 0.0, "the old pointer gesture cannot edit the newly selected photo");
    assert_eq!(h.app.session.develop_of(PhotoId(1)).unwrap().light.exposure, edited);
    assert_eq!(h.app.session.undo.len(), 1, "only the original photo's drag was committed");
    assert!(h.app.session.interaction.is_none());
    drop(h);
    lightcraft_engine::gpu::set_enabled(true);
}

fn slider_step_timings(gpu: bool) {
    let _gpu_gate = crate::gpu_test_support::lock();
    let mut h = headless(gpu, [1800.0, 1600.0], 1.0);
    wait_main(&mut h);
    let backend = if gpu { lightcraft_engine::gpu::adapter_name().unwrap() } else { "CPU".into() };
    let controls = ["light.exposure", "light.highlights", "effects.clarity", "color.saturation"];
    for control in controls {
        h.app.session.execute("develop.reset", &json!({})).unwrap();
        h.step();
        wait_main(&mut h);
        let rect = slider_rect(&h, control);
        let p = |f| egui::pos2(rect.left() + rect.width() * f, rect.center().y);
        let undo = h.app.session.undo.len();
        let old_pixels = h.app.renderer.textures[&Slot::Main].pixels.clone().unwrap();
        button(&mut h, p(0.5), true, Default::default());
        let mut latencies = Vec::new();
        for i in 0..6 {
            let f = 0.54 + i as f32 * 0.035;
            let t0 = Instant::now();
            pointer(&mut h, p(f));
            let ui_ms = t0.elapsed().as_secs_f64() * 1e3;
            assert!(h.app.session.interaction.is_some());
            let spec = lightcraft_develop::controls::find(control).unwrap();
            let expected = (((spec.min + f as f64 * (spec.max - spec.min)) / spec.step).round() * spec.step).clamp(spec.min, spec.max);
            assert!((value(&h, control) - expected).abs() <= spec.step, "the setting tracks the pointer");
            wait_main(&mut h);
            let ms = t0.elapsed().as_secs_f64() * 1e3;
            latencies.push(ms);
            let tex = &h.app.renderer.textures[&Slot::Main];
            assert!(tex.size[0].max(tex.size[1]) <= crate::render::DRAFT_EDGE);
            if gpu {
                assert!(lightcraft_engine::gpu::enabled(), "GPU rendering stopped during a slider step");
                assert!(lightcraft_engine::gpu::last_fallback().is_none(), "GPU slider fell back: {:?}", lightcraft_engine::gpu::last_fallback());
            }
            eprintln!(
                "slider step backend={backend} control={control} step={i} value={:.2} UI={ui_ms:.2}ms job={:.2}ms change_to_texture={ms:.2}ms size={:?}",
                value(&h, control),
                tex.ms,
                tex.size
            );
        }
        let t0 = Instant::now();
        button(&mut h, p(0.72), false, Default::default());
        wait_main(&mut h);
        assert!(h.app.session.interaction.is_none());
        assert_eq!(h.app.session.undo.len(), undo + 1, "one history entry for all live steps and release");
        let tex = &h.app.renderer.textures[&Slot::Main];
        let final_job = h.app.session.loupe_job(tex.photo, tex.size[0], tex.size[1], true).unwrap();
        assert_eq!(tex.key, final_job.key, "settled texture uses the full-quality job key");
        assert_ne!(tex.pixels.as_ref().unwrap().pixels, old_pixels.pixels, "edits reach the displayed pixels");
        assert!(tex.size[0].max(tex.size[1]) > crate::render::DRAFT_EDGE, "release restores display resolution");
        eprintln!(
            "slider summary backend={backend} control={control} step_mean={:.2}ms step_p95={:.2}ms release_to_full={:.2}ms full_size={:?}",
            mean(&latencies),
            pct(&mut latencies, 0.95),
            t0.elapsed().as_secs_f64() * 1e3,
            tex.size
        );
    }
    let bytes = h.app.renderer.memory()["stageCaches"]["gpuBytes"].as_u64().unwrap();
    if gpu {
        assert!(bytes > 0, "UI/engine render path retained GPU stages; it did not silently fall back");
    } else {
        assert_eq!(bytes, 0, "CPU timing uses the CPU pipeline");
    }
    drop(h); // join all render workers before the GPU is torn down
    assert!(lightcraft_engine::gpu::quiesce(Duration::from_secs(10)));
    lightcraft_engine::gpu::set_enabled(true);
}

#[test]
fn develop_slider_cpu_step_timings() {
    // The preference is process-wide. Other headless apps running in parallel can
    // enable it, so pin this CPU measurement in its own process.
    if std::env::var_os("LC_SLIDER_CPU_TEST_CHILD").is_none() {
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "tests_slider_speed::develop_slider_cpu_step_timings", "--nocapture"])
            .env("LC_SLIDER_CPU_TEST_CHILD", "1")
            .env("LIGHTCRAFT_GPU", "0")
            .output()
            .unwrap();
        eprint!("{}", String::from_utf8_lossy(&output.stderr));
        assert!(output.status.success(), "CPU timing child failed: {}", String::from_utf8_lossy(&output.stdout));
        return;
    }
    slider_step_timings(false);
}

#[test]
fn develop_slider_gpu_step_timings() {
    // Other headless apps change a process-wide GPU preference when applying settings.
    // Give this measurement private preference and fallback state, like the CPU one.
    if std::env::var_os("LC_SLIDER_GPU_TEST_CHILD").is_none() {
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "tests_slider_speed::develop_slider_gpu_step_timings", "--nocapture"])
            .env("LC_SLIDER_GPU_TEST_CHILD", "1")
            .env("LIGHTCRAFT_GPU", "1")
            .output()
            .unwrap();
        eprint!("{}", String::from_utf8_lossy(&output.stderr));
        assert!(output.status.success(), "GPU timing child failed: {}", String::from_utf8_lossy(&output.stdout));
        return;
    }
    lightcraft_engine::gpu::set_enabled(true);
    if !lightcraft_engine::gpu::available() {
        assert!(std::env::var_os("LC_REQUIRE_GPU").is_none(), "hardware GPU required: {:?}", lightcraft_engine::gpu::unavailable_reason());
        eprintln!("skipped: no GPU adapter");
        return;
    }
    slider_step_timings(true);
}
