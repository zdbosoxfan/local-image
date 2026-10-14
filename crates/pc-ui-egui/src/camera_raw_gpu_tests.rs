//! Small opt-in measurement of real UI drags; file data stays local.
use super::*;
use egui_kittest::{Harness, kittest::Queryable};
use std::sync::Mutex;

// Observe the actual shared renderer, so a CPU fallback cannot count as a GPU timing.
struct BackendLog(Mutex<String>);
static BACKEND: BackendLog = BackendLog(Mutex::new(String::new()));
impl log::Log for BackendLog {
    fn enabled(&self, _: &log::Metadata<'_>) -> bool {
        true
    }
    fn log(&self, record: &log::Record<'_>) {
        let message = record.args().to_string();
        if let Some(backend) = message.strip_prefix("Develop render backend: ") {
            *self.0.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = backend.to_owned();
        }
    }
    fn flush(&self) {}
}

#[test]
#[ignore = "requires CR_GPU_TEST_IMAGE (a local 24–100 MP test photo); run isolated"]
fn cr_gpu_real_photo_preview_drag_latency() {
    log::set_logger(&BACKEND).unwrap();
    log::set_max_level(log::LevelFilter::Info);
    let path = std::env::var("CR_GPU_TEST_IMAGE").expect("set a local test-image path");
    let bytes = std::fs::read(&path).unwrap();
    let source_path = path.clone();
    let cpu = std::env::var("LIGHTCRAFT_GPU").is_ok_and(|v| v == "0");
    let references = std::env::current_dir().unwrap().join("target/cr-gpu-reference");
    std::fs::create_dir_all(&references).unwrap();
    let mut h = Harness::builder().with_size(vec2(1200.0, 800.0)).build_eframe(move |cc| {
        PhotocraftApp::setup_context(&cc.egui_ctx, Default::default());
        // The offscreen context defaults to 2048, without a native backend supplying
        // its texture limit. Allow the existing main-canvas proxy (not the CR proxy).
        cc.egui_ctx.input_mut(|input| input.max_texture_side = 4096);
        let services = crate::Services {
            import: Some(Box::new(|name, bytes| photocraft_io::import(name, bytes).map(|r| (r.document, r.warnings)).map_err(|e| e.to_string()))),
            ..Default::default()
        };
        let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), services);
        app.open_file(&path, &bytes).unwrap();
        open(&mut app, &cc.egui_ctx).unwrap();
        app
    });
    h.run_steps(2);
    let d = h.state().camera_raw.as_ref().unwrap();
    assert!((24_000_000..=100_000_000).contains(&(d.full_w * d.full_h)));
    println!("CR UI source={}x{} proxy={}x{}", d.full_w, d.full_h, d.pw, d.ph);
    let cases = [
        ("tone", CameraRaw { contrast: 25.0, shadows: 30.0, vibrance: 25.0, ..Default::default() }),
        ("dehaze", CameraRaw { dehaze: 40.0, ..Default::default() }),
        ("texture_clarity", CameraRaw { texture: 35.0, clarity: 30.0, ..Default::default() }),
        ("NR", CameraRaw { noise_luminance: 45.0, noise_color: 35.0, ..Default::default() }),
        ("grain", CameraRaw { grain_amount: 45.0, seed: 42, ..Default::default() }),
    ];
    for (name, params) in cases {
        let ctx = h.ctx.clone();
        let d = h.state_mut().camera_raw.as_mut().unwrap();
        d.params = params;
        d.render(&ctx); // Warm settings-specific allocations; not included in drag samples.
        assert!(d.detail.error.is_none());
        h.run_steps(2);
        let rect = h.get_by_label("Exposure slider").rect();
        let start = rect.center();
        h.hover_at(start);
        h.run_steps(1);
        h.drag_at(start);
        h.run_steps(1);
        let mut samples = Vec::new();
        for offset in [12.0, 24.0, 36.0] {
            let revision = h.state().camera_raw.as_ref().unwrap().preview_revision;
            h.hover_at(start + vec2(offset, 0.0));
            let t = std::time::Instant::now();
            // Controls mark dirty at the end of the input frame; the next frame renders.
            h.run_steps(2);
            let frame_ms = t.elapsed().as_secs_f64() * 1000.0;
            let d = h.state().camera_raw.as_ref().unwrap();
            assert!(d.preview_revision > revision, "drag must render a new preview");
            assert!(d.detail.error.is_none());
            assert_ne!(d.params.exposure, 0.0);
            samples.push(d.render_ms);
            println!("CR UI {name} render_ms={:.3} frame_ms={frame_ms:.3}", d.render_ms);
            let reference_path = references.join(format!("{name}-{offset}.json"));
            if cpu {
                let reference = json!({"source": source_path, "params": d.params, "pixels": d.processed});
                std::fs::write(reference_path, serde_json::to_vec(&reference).unwrap()).unwrap();
            } else {
                let reference: Value = serde_json::from_slice(&std::fs::read(reference_path).expect("run the CPU measurement first")).unwrap();
                assert_eq!(reference["source"], source_path);
                assert_eq!(reference["params"], serde_json::to_value(&d.params).unwrap());
                let reference: Vec<[f32; 4]> = serde_json::from_value(reference["pixels"].clone()).unwrap();
                assert_eq!(reference.len(), d.processed.len());
                let diffs: Vec<_> = reference.iter().zip(&d.processed).flat_map(|(a, b)| (0..3).map(move |c| (a[c] - b[c]).abs())).collect();
                let max = diffs.iter().copied().fold(0.0f32, f32::max);
                let mean = diffs.iter().sum::<f32>() / diffs.len() as f32;
                let rmse = (diffs.iter().map(|v| v * v).sum::<f32>() / diffs.len() as f32).sqrt();
                println!("CR UI parity {name} mean={mean:.8} max={max:.8} rmse={rmse:.8}");
                assert!(mean * 255.0 < 0.5 && max * 255.0 <= 3.0);
                assert!(reference.iter().zip(&d.processed).all(|(a, b)| a[3].to_bits() == b[3].to_bits()));
            }
        }
        h.drop_at(start + vec2(36.0, 0.0));
        h.run_steps(2);
        samples.sort_by(f64::total_cmp);
        let backend = BACKEND.0.lock().unwrap().clone();
        assert!(backend.starts_with(if cpu { "CPU:" } else { "GPU:" }), "unexpected backend: {backend}");
        assert!(!backend.to_lowercase().contains("llvmpipe"));
        println!("CR UI {name} median_ms={:.3} backend={backend}", samples[1]);
    }
    // Imported ProPhoto can also have a colour-policy Cancel in the underlying app.
    // Camera Raw's fullscreen footer is the lowest button; verify that dialog closes.
    h.query_all_by_label("Cancel").max_by(|a, b| a.rect().bottom().total_cmp(&b.rect().bottom())).unwrap().click();
    h.run_steps(2);
    assert!(h.state().camera_raw.is_none());
}
