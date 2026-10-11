//! Develop's AI services, provided by the app: Compositing's AI Remove engines (FLUX.2 Klein and
//! Qwen through ComfyUI, `li-ai`), handed to the Library's engine as
//! its [`AiHost`] — so the `lc-*` crates depend on neither ComfyUI nor the editor.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use lightcraft_engine::enhance::{AiHost, JobCtl, RemoveEngine, RemoveRequest, RemoveResult};
use photocraft_ui_egui::ai_ui;

/// The AI Remove engines, as Compositing's AI Remove tool offers them.
const ENGINES: [(&str, &str); 3] = [("klein", "FLUX.2 Klein"), ("qwen-int8", "Qwen Compact"), ("qwen-bf16", "Qwen Full")];

pub struct DevelopAi;

impl DevelopAi {
    pub fn shared() -> Arc<dyn AiHost> {
        Arc::new(DevelopAi)
    }
}

fn engine(key: &str) -> li_ai::RemoveEngine {
    match key {
        "qwen-bf16" => li_ai::RemoveEngine::Qwen { variant: "bf16".into() },
        "qwen-int8" => li_ai::RemoveEngine::Qwen { variant: "int8".into() },
        _ => li_ai::RemoveEngine::Klein,
    }
}

struct ProgressDone<'a>(&'a AtomicBool);
impl Drop for ProgressDone<'_> {
    fn drop(&mut self) {
        self.0.store(true, Ordering::SeqCst);
    }
}

impl AiHost for DevelopAi {
    fn upscale(&self, req: &lightcraft_engine::enhance::UpscaleRequest, ctl: &JobCtl) -> Result<Vec<[u8; 4]>, String> {
        if req.width == 0
            || req.height == 0
            || req.width > u32::MAX as usize
            || req.height > u32::MAX as usize
            || req.width.checked_mul(req.height) != Some(req.rgba.len())
        {
            return Err("image buffer size mismatch".into());
        }
        let method = li_ai::enhance::Method::parse(&req.method).ok_or("unknown enhancement method")?;
        let input = image::RgbaImage::from_fn(req.width as u32, req.height as u32, |x, y| image::Rgba(req.rgba[y as usize * req.width + x as usize]));
        let lctl = li_ai::JobControl::new();
        let done = AtomicBool::new(false);
        let out = std::thread::scope(|scope| {
            scope.spawn(|| {
                while !done.load(Ordering::Relaxed) {
                    if ctl.cancelled() {
                        lctl.cancel();
                    }
                    let progress = lctl.progress();
                    ctl.set(progress.fraction().unwrap_or(0.0), progress.stage.label());
                    std::thread::sleep(Duration::from_millis(50));
                }
            });
            let _done = ProgressDone(&done);
            let r = li_ai::service().enhance(
                &input,
                req.target.0 as u32,
                req.target.1 as u32,
                method,
                li_ai::enhance::Backend::Auto,
                &photocraft_engine::seg::models_dir(),
                0,
                &lctl,
            );
            done.store(true, Ordering::Relaxed);
            r
        })
        .map_err(|e| if e.is::<li_ai::Cancelled>() { lightcraft_engine::enhance::CANCELLED.into() } else { format!("{e:#}") })?;
        Ok(out.pixels().map(|p| p.0).collect())
    }

    fn remove_engines(&self) -> Vec<RemoveEngine> {
        let st = ai_ui::status();
        ENGINES
            .iter()
            .map(|(key, label)| {
                let (model, variant) = ai_ui::remove_engine(key);
                RemoveEngine { key: (*key).to_owned(), label: (*label).to_owned(), problem: st.ready(model, variant).err() }
            })
            .collect()
    }

    fn remove(&self, req: &RemoveRequest, ctl: &JobCtl) -> Result<RemoveResult, String> {
        let (w, h) = (req.width as u32, req.height as u32);
        if req.rgb.len() != req.width * req.height || req.mask.len() != req.rgb.len() {
            return Err("image and mask sizes differ".into());
        }
        let rgb = image::RgbImage::from_fn(w, h, |x, y| image::Rgb(req.rgb[(y * w + x) as usize]));
        let mask = image::GrayImage::from_fn(w, h, |x, y| image::Luma([req.mask[(y * w + x) as usize]]));
        // li-ai's progress and cancellation mirrored onto the Develop job
        let lctl = li_ai::JobControl::new();
        let done = Arc::new(AtomicBool::new(false));
        let out = std::thread::scope(|scope| {
            scope.spawn(|| {
                while !done.load(Ordering::SeqCst) {
                    if ctl.cancelled() {
                        lctl.cancel();
                    }
                    let p = lctl.progress();
                    let frac = match p.stage {
                        li_ai::Stage::Sampling => 0.2 + 0.65 * p.fraction().unwrap_or(0.0),
                        li_ai::Stage::Decoding | li_ai::Stage::Saving | li_ai::Stage::Finishing | li_ai::Stage::Done => 0.88,
                        _ => 0.12,
                    };
                    ctl.set(frac, p.stage.label());
                    std::thread::sleep(Duration::from_millis(100));
                }
            });
            let _done = ProgressDone(&done);
            let r = li_ai::service().remove_objects(&rgb, &mask, &engine(&req.engine), req.seed, &lctl);
            done.store(true, Ordering::SeqCst);
            r
        });
        let out = out.map_err(|e| if e.is::<li_ai::Cancelled>() { lightcraft_engine::enhance::CANCELLED.to_owned() } else { format!("{e:#}") })?;
        if out.dimensions() != (w, h) {
            return Err("The AI engine returned an unexpected size.".into());
        }
        // only the cleaned selection changed (what Compositing's AI Remove composites through)
        let alpha = li_ai::imaging::clean_selection_mask(&mask);
        Ok(RemoveResult { rgb: out.pixels().map(|p| p.0).collect(), alpha: alpha.pixels().map(|p| p.0[0]).collect() })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ds2_upscale_rejects_overflow_and_zero_dimensions() {
        for (width, height) in [(usize::MAX, 2), (0, 0)] {
            let req = lightcraft_engine::enhance::UpscaleRequest { width, height, rgba: vec![], target: (2, 2), method: "faithful".into() };
            assert!(DevelopAi.upscale(&req, &JobCtl::new()).unwrap_err().contains("buffer size mismatch"));
        }
    }

    #[test]
    fn ds2_progress_thread_exits_when_service_panics() {
        let done = AtomicBool::new(false);
        let panic = std::panic::catch_unwind(|| {
            std::thread::scope(|scope| {
                scope.spawn(|| {
                    while !done.load(Ordering::SeqCst) {
                        std::thread::yield_now();
                    }
                });
                let _done = ProgressDone(&done);
                panic!("service panic");
            })
        });
        assert!(panic.is_err());
        assert!(done.load(Ordering::SeqCst));
    }
}
