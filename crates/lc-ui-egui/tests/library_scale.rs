//! CPU headless scrolling and actual keyboard navigation with neighbour prefetch.
use lightcraft_catalog::{Op, Photo, PhotoId, Source};
use lightcraft_engine::Session;
use lightcraft_ui_egui::{
    LightcraftApp, Services,
    headless::{Headless, HeadlessView},
    render::Slot,
    state::ViewMode,
};
use serde_json::json;
use std::sync::Arc;
use std::time::{Duration, Instant};

fn session(n: usize, pixels: bool) -> Session {
    let mut s = Session::new();
    s.media.file_loader = Some(Arc::new(move |_, _| {
        if pixels {
            Ok((lightcraft_raster::Rgb32f::filled(768, 512, [0.12, 0.25, 0.35]), Default::default()))
        } else {
            Err("synthetic metadata-only grid benchmark".into())
        }
    }));
    for i in 0..n {
        let mut p = Photo::new(
            PhotoId(i as u64 + 1),
            Source::File { path: format!("/library-scale-synthetic/{i}.jpg") },
            &format!("Event_{i:05}.jpg"),
            "JPEG",
            6000,
            4000,
            "2026-01-01",
        );
        p.captured = Some(format!("2026-09-{:02}T{:02}:00:00", 1 + (i / 40) % 28, i % 24));
        s.catalog.apply(Op::AddPhoto { photo: Box::new(p) }).unwrap();
    }
    s
}
fn peak_rss_kib() -> usize {
    std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|s| s.lines().find(|l| l.starts_with("VmHWM:")).and_then(|l| l.split_whitespace().nth(1)).and_then(|v| v.parse().ok()))
        .unwrap_or(0)
}
fn percentiles(row: &str, n: usize, mut times: Vec<f64>) {
    times.sort_by(f64::total_cmp);
    eprintln!(
        "library_bench,row={row},n={n},median_ms={:.3},p95_ms={:.3},process_peak_rss_kib={}",
        times[times.len() / 2],
        times[times.len() * 95 / 100],
        peak_rss_kib()
    );
}
#[test]
#[ignore = "release headless UI benchmark"]
fn library_scale_ui() {
    assert!(!cfg!(debug_assertions), "run with --release");
    lightcraft_engine::gpu::set_enabled(false);
    for n in [2000, 20000] {
        for view in [ViewMode::PhotoGrid, ViewMode::SquareGrid] {
            let app = LightcraftApp::new(session(n, false), Services { png: None, ..Default::default() });
            let mut h = Headless::new(app, [1400.0, 900.0], 1.0);
            h.app.ui.view = view;
            h.app.ui.thumb_size = 160.0;
            assert!(h.settle(Duration::from_secs(60)));
            let canvas = h.app.canvas_rect.unwrap();
            let before = h.app.caches.grid_stats.clone();
            let mut times = Vec::new();
            let mut max_scroll = 0.0f32;
            for frame in 0..120 {
                let raw = HeadlessView::raw_input(
                    h.size,
                    1.0,
                    10.0 + frame as f64 / 60.0,
                    vec![
                        egui::Event::PointerMoved(canvas.center()),
                        egui::Event::MouseWheel {
                            unit: egui::MouseWheelUnit::Point,
                            delta: egui::vec2(0.0, if frame < 60 { -120.0 } else { 120.0 }),
                            phase: egui::TouchPhase::Move,
                            modifiers: Default::default(),
                        },
                    ],
                );
                let t = Instant::now();
                h.view.run(raw, |ui| {
                    h.app.logic(ui.ctx());
                    h.app.ui(ui);
                });
                times.push(t.elapsed().as_secs_f64() * 1000.0);
                max_scroll = max_scroll.max(h.app.grid_scroll.unwrap_or(0.0));
            }
            assert!(max_scroll > 0.0, "wheel events must actually scroll the grid");
            let stats = &h.app.caches.grid_stats;
            eprintln!(
                "library_bench,row=grid_work,n={n},cells_per_frame={},layout_rebuilds={}",
                (stats.cells_visited - before.cells_visited) / 120,
                stats.layout_builds - before.layout_builds
            );
            percentiles(if view == ViewMode::PhotoGrid { "photo_grid_scroll" } else { "square_grid_scroll" }, n, times);
        }
    }
    navigation(session(2000, true), 2000, "");
    if let Some(root) = std::env::var_os("LIBRARY_BENCH_DIR") {
        let n = std::env::var("LIBRARY_BENCH_N").ok().and_then(|s| s.parse().ok()).unwrap_or(2000);
        let paths = lightcraft_engine::import::expand(&[root.to_string_lossy().into_owned()], None);
        let paths: Vec<_> = paths.into_iter().take(n).collect();
        let mut session = Session::new().with_fs();
        session.xmp.auto_write = false;
        let result = lightcraft_engine::import::import(&mut session, &paths, lightcraft_engine::import::ImportMode::Add);
        assert!(result.is_ok(), "real input import failed (source names suppressed)");
        assert!(result.unwrap().failed.is_empty(), "some real inputs failed (source names suppressed)");
        let n = session.catalog.len();
        assert!(n >= 3, "navigation needs at least three unique photos");
        navigation(session, n, "real_");
    }
}
fn navigation(session: Session, n: usize, prefix: &str) {
    let app = LightcraftApp::new(session, Services { png: None, ..Default::default() });
    let mut h = Headless::new(app, [1400.0, 900.0], 1.0);
    let first = h.app.session.visible()[(n / 3).min(30)];
    h.app.session.selection.active = Some(first);
    h.app.session.selection.ids = vec![first];
    h.app.ui.view = ViewMode::Detail;
    assert!(h.settle(Duration::from_secs(60)));
    let mut next = Vec::new();
    let mut prev = Vec::new();
    for (key, values) in [("right", &mut next), ("left", &mut prev)] {
        for _ in 0..20.min(n / 3) {
            // Let the real detail panel finish its neighbour prefetch before pressing the key.
            assert!(h.settle(Duration::from_secs(60)));
            let old = h.app.session.active();
            let t = Instant::now();
            let r = h.request("ui.key", json!({"key":key}), Duration::from_secs(20));
            assert_eq!(r["ok"], true);
            let active = h.app.session.active();
            assert_ne!(active, old);
            assert!(h.step_until(Duration::from_secs(60), |h| h.app.renderer.textures.get(&Slot::Main).is_some_and(|t| Some(t.photo) == active)
                && !h.app.renderer.is_pending(Slot::Main)));
            values.push(t.elapsed().as_secs_f64() * 1000.0);
        }
    }
    percentiles(&format!("loupe_{prefix}next_prefetched"), n, next);
    percentiles(&format!("loupe_{prefix}prev_prefetched"), n, prev);
}
