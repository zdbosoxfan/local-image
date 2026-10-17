//! Verify logs describe the backend actually used, including host-side CPU refusals.
#[path = "../../lc-gpu/tests/support/mod.rs"]
mod gpu_support;

use std::sync::{Arc, Mutex};

use li_raster::RgbFrame;
use lightcraft_develop::DevelopSettings;
use lightcraft_pipeline::{Proof, RenderRequest, SourceInfo, StageCache};

struct Capture(Mutex<Vec<String>>);
static LOG: Capture = Capture(Mutex::new(Vec::new()));

impl log::Log for Capture {
    fn enabled(&self, _: &log::Metadata<'_>) -> bool {
        true
    }
    fn log(&self, record: &log::Record<'_>) {
        if record.level() == log::Level::Info {
            self.0.lock().unwrap().push(record.args().to_string());
        }
    }
    fn flush(&self) {}
}

#[test]
fn develop_backend_logs_actual_adapter_and_cpu_reasons() {
    let _gpu = gpu_support::lock();
    log::set_logger(&LOG).unwrap();
    log::set_max_level(log::LevelFilter::Info);
    let src = Arc::new(RgbFrame::from_fn(64, 48, |x, y| [0.1 + x as f32 / 128.0, 0.2 + y as f32 / 128.0, 0.3]));
    let (info, settings, stages) = (SourceInfo::default(), DevelopSettings::default(), StageCache::default());
    let req = RenderRequest::fit(64, 48);
    assert!(lightcraft_gpu::available(), "real GPU required: {:?}", lightcraft_gpu::unavailable_reason());
    let adapter = lightcraft_gpu::adapter_name().unwrap();
    eprintln!("Develop backend test: {adapter}");
    lightcraft_engine::media::develop(&src, &info, &settings, &req, Some(&stages), true);
    assert!(lightcraft_gpu::stage_bytes(&stages) > 0, "the engine used GPU stages");
    assert!(LOG.0.lock().unwrap().iter().any(|m| m == &format!("Develop render backend: GPU: {adapter}")));
    let proof = RenderRequest { proof: Some(Proof::default()), ..req };
    lightcraft_engine::media::develop(&src, &info, &settings, &proof, None, true);
    assert!(LOG.0.lock().unwrap().iter().any(|m| m == "Develop render backend: CPU: soft proofing requires CPU processing"));
    lightcraft_gpu::set_enabled(false);
    lightcraft_engine::media::develop(&src, &info, &settings, &req, None, true);
    lightcraft_gpu::set_enabled(true);
    assert!(LOG.0.lock().unwrap().iter().any(|m| m.starts_with("Develop render backend: CPU: disabled by")));
}
