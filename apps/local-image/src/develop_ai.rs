//! Develop's AI services, provided by the app: Compositing's AI Remove engines (FLUX.2 Klein and
//! Qwen through ComfyUI, `li-ai`) and the local model downloads (the same hash-checked downloads
//! and model list as Compositing's Local AI › Selection models), handed to the Library's engine as
//! its [`AiHost`] — so the `lc-*` crates depend on neither ComfyUI nor the editor.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use lightcraft_engine::enhance::{AiHost, Download, JobCtl, RemoveEngine, RemoveRequest, RemoveResult};
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

impl AiHost for DevelopAi {
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

    fn start_model_download(&self, id: &str) -> Result<(), String> {
        let spec = li_seg::spec(id).ok_or_else(|| format!("unknown model `{id}`"))?;
        ai_ui::start_seg_download(spec);
        Ok(())
    }

    fn model_download(&self, id: &str) -> Option<Download> {
        ai_ui::downloads().get(&format!("seg:{id}")).map(|d| Download { running: !d.finished, done: d.done, total: d.total, error: d.error.clone() })
    }

    fn cancel_model_download(&self, id: &str) {
        if let Some(d) = ai_ui::downloads().get(&format!("seg:{id}")) {
            d.ctl.cancel();
        }
    }
}
