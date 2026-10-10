//! One restoration service for Compositing, Generate and Library/Develop export.
use crate::{Ai, JobControl, ModelId, Stage};
use anyhow::{Context, Result, bail};
use image::{RgbaImage, imageops};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{
    path::{Path, PathBuf},
    sync::Mutex,
};

pub mod selection;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Method {
    Faithful,
    Rescue,
    #[default]
    Generative,
}
impl Method {
    pub const ALL: [Self; 3] = [Self::Faithful, Self::Rescue, Self::Generative];
    pub fn label(self) -> &'static str {
        match self {
            Self::Faithful => "Faithful",
            Self::Rescue => "Rescue",
            Self::Generative => "Generative (SeedVR2)",
        }
    }
    pub fn key(self) -> &'static str {
        match self {
            Self::Faithful => "faithful",
            Self::Rescue => "rescue",
            Self::Generative => "generative",
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|m| m.key() == s)
    }
    pub fn model(self, scale: u32) -> Option<&'static li_seg::ModelSpec> {
        let key = match self {
            Self::Faithful => "realplksr",
            Self::Rescue => "bsrgan",
            Self::Generative => return None,
        };
        li_seg::spec(&format!("{key}-{scale}x"))
    }
    pub fn about(self) -> &'static str {
        match self {
            Self::Faithful => "Enlarges clean photos while preserving their detail.",
            Self::Rescue => "Enlarges photos and cleans noise, JPEG artefacts and mild blur.",
            Self::Generative => "Adds detail with SeedVR2. The result may change textures.",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Backend {
    #[default]
    Auto,
    Cpu,
    Gpu,
}

type Loaded = Option<(PathBuf, String, li_seg::enhance::Upscaler)>;
static LOADED: Mutex<Loaded> = Mutex::new(None);

pub fn native_scale(input: (u32, u32), output: (u32, u32)) -> Result<u32> {
    let (sw, sh) = input;
    let (w, h) = output;
    if sw == 0 || sh == 0 || w <= sw || h <= sh || u64::from(w) * u64::from(h) > 400_000_000 {
        bail!("Choose a larger size, up to 400 megapixels.");
    }
    let k = (w as f64 / sw as f64).max(h as f64 / sh as f64);
    if k > 4.001 {
        bail!("Choose an enlargement up to 4×.");
    }
    if (w as f64 / sw as f64 - h as f64 / sh as f64).abs() * sw.min(sh) as f64 > 2.0 {
        bail!("Keep the image's proportions when enhancing.");
    }
    let scale = if k <= 2.001 { 2 } else { 4 };
    if u64::from(sw) * u64::from(sh) * u64::from(scale * scale) > 400_000_000 {
        bail!("The native model output exceeds 400 megapixels.");
    }
    Ok(scale)
}

/// Explicit downloads share the same vetted byte counts, hashes and allow-list as all models.
pub fn download_cpu(method: Method, scale: u32, dir: &Path, ctl: &JobControl, progress: &dyn Fn(u64)) -> Result<()> {
    let spec = method.model(scale).context("No on-device model for this method")?;
    download_cpu_spec(spec, dir, ctl, progress)
}

/// Also used by the model-management UI and its isolated download tests.
pub fn download_cpu_spec(spec: &li_seg::ModelSpec, dir: &Path, ctl: &JobControl, progress: &dyn Fn(u64)) -> Result<()> {
    ctl.check()?;
    if li_seg::file_installed(dir, spec.files().next().context("No model file")?) {
        progress(spec.bytes);
        return Ok(());
    }
    crate::download::download_file(spec.url, &li_seg::model_path(dir, spec), spec.bytes, spec.sha256, ctl, progress)
}

impl Ai {
    pub fn enhance(
        &self,
        input: &RgbaImage,
        width: u32,
        height: u32,
        method: Method,
        backend: Backend,
        models_dir: &Path,
        seed: u64,
        ctl: &JobControl,
    ) -> Result<RgbaImage> {
        let choice = selection::load(models_dir, method);
        self.enhance_with_choice(input, width, height, method, backend, models_dir, seed, ctl, &choice)
    }

    /// Explicit choices let the evaluation harness exercise every candidate through the same
    /// app service without changing the user's per-function settings.
    pub fn enhance_with_choice(
        &self,
        input: &RgbaImage,
        width: u32,
        height: u32,
        method: Method,
        backend: Backend,
        models_dir: &Path,
        seed: u64,
        ctl: &JobControl,
        choice: &selection::Choice,
    ) -> Result<RgbaImage> {
        use selection::Choice;
        ctl.check()?;
        native_scale(input.dimensions(), (width, height))?;
        selection::validate(method, choice)?;
        if method == Method::Generative {
            if backend == Backend::Cpu {
                bail!("Generative enlargement requires the GPU backend.");
            }
            if width > 8192 || height > 8192 {
                bail!("Generative enlargement is limited to 8192 pixels per side.");
            }
            let even_w = width + width % 2;
            let even_h = height + height % 2;
            let Choice::Seed { unet, vae } = choice else { bail!("Choose a SeedVR2 model") };
            let files = [(crate::family::Role::Unet, unet.clone()), (crate::family::Role::Vae, vae.clone())].into_iter().collect();
            let out = self.upscale_with_files(input, even_w, even_h, seed, ctl, Some(&files))?;
            ctl.check()?;
            let out = if out.dimensions() == (width, height) { out } else { imageops::resize(&out, width, height, imageops::FilterType::Lanczos3) };
            ctl.check()?;
            ctl.set_stage(Stage::Done);
            return Ok(out);
        }
        let spec = match choice {
            Choice::Curated { id } => Some(li_seg::spec(id).context("Unknown restoration model")?),
            _ => None,
        };
        let scale = if let Some(spec) = spec {
            if spec.id.ends_with("2x") { 2 } else { 4 }
        } else if let Choice::Custom { scale, .. } = choice {
            *scale
        } else {
            bail!("Invalid restoration choice")
        };
        if u64::from(input.width()) * u64::from(input.height()) * u64::from(scale * scale) > 400_000_000 {
            bail!("The selected model's native output exceeds 400 megapixels.");
        }
        let gpu = if backend == Backend::Cpu || spec.is_none() {
            if backend == Backend::Gpu && spec.is_none() {
                bail!("Custom ONNX restoration models run on the CPU.");
            }
            None
        } else {
            let spec = spec.context("No GPU model preset")?;
            let preset = crate::catalog::preset(ModelId::intern(spec.id), "default").context("No GPU model preset")?;
            match self.client.object_info() {
                Ok(info) => {
                    let available = crate::catalog::availability(preset, &info);
                    if available.available {
                        Some(available.files)
                    } else if backend == Backend::Gpu {
                        bail!("{}", available.reason);
                    } else {
                        None
                    }
                }
                Err(e) if backend == Backend::Gpu => return Err(e),
                Err(_) => None,
            }
        };
        ctl.set_stage(Stage::Preparing);
        // Both engines see sRGB on white; alpha and soft-edge colours are restored once below.
        let rgb = image::DynamicImage::ImageRgba8(crate::imaging::over_white(input)).to_rgb8();
        let enlarged = if let Some(files) = gpu {
            let name = files.get(&crate::family::Role::Upscaler).context("GPU model file missing")?;
            let graph = json!({
                "1": {"class_type": "UpscaleModelLoader", "inputs": {"model_name": name}},
                "2": {"class_type": "LoadImage", "inputs": {"image": ""}},
                "3": {"class_type": "ImageUpscaleWithModel", "inputs": {"upscale_model": ["1", 0], "image": ["2", 0]}},
                "4": {"class_type": "SaveImage", "inputs": {"images": ["3", 0], "filename_prefix": "LocalImage_Enhance"}}
            });
            let png = crate::imaging::encode_png(&rgb)?;
            image::load_from_memory(&self.client.run(graph, &[("2".into(), png)], ctl)?)?.to_rgb8()
        } else {
            ctl.set_stage(Stage::Loading);
            let mut cache = LOADED.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
            if let Some(spec) = spec
                && li_seg::installed_bytes(models_dir, spec).is_none()
            {
                bail!("Download {} in Settings › Local AI first.", spec.label);
            }
            let path = if let Some(spec) = spec { li_seg::model_path(models_dir, spec) } else { selection::custom_path(models_dir, choice)? };
            let metadata = std::fs::metadata(path)?;
            let key = format!("{}:{}:{:?}", serde_json::to_string(choice)?, metadata.len(), metadata.modified().ok());
            if cache.as_ref().is_none_or(|(dir, id, _)| dir != models_dir || *id != key) {
                let model =
                    if let Some(spec) = spec { li_seg::enhance::Upscaler::load(spec, models_dir)? } else { selection::load_custom(models_dir, choice)? };
                *cache = Some((models_dir.to_owned(), key, model));
            }
            ctl.check()?;
            let model = cache.as_ref().context("Model cache empty")?.2.clone();
            drop(cache);
            model.run(&rgb, &|| ctl.check(), &|done, total| ctl.set_step(done as u32, total as u32))?
        };
        ctl.check()?;
        if enlarged.dimensions() != (input.width() * scale, input.height() * scale) {
            bail!("Restoration model returned an unexpected size");
        }
        ctl.set_stage(Stage::Finishing);
        let output = imageops::resize(&enlarged, width, height, imageops::FilterType::Lanczos3);
        let source = imageops::resize(input, width, height, imageops::FilterType::Lanczos3);
        let out = RgbaImage::from_fn(width, height, |x, y| {
            let s = source.get_pixel(x, y);
            let p = output.get_pixel(x, y);
            let a = (s[3] as f32 / 255.0).powi(4);
            image::Rgba([0, 1, 2, 3].map(|c| if c == 3 { s[3] } else { (p[c] as f32 * a + s[c] as f32 * (1.0 - a)).round() as u8 }))
        });
        ctl.check()?;
        ctl.set_stage(Stage::Done);
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn enhance_dimensions_odd_edges_and_old_settings() {
        assert_eq!(native_scale((101, 73), (202, 146)).unwrap(), 2);
        assert_eq!(native_scale((101, 73), (404, 292)).unwrap(), 4);
        assert!(native_scale((1, 1), (5, 5)).is_err());
        assert!(native_scale((101, 73), (210, 170)).is_err());
        assert!(native_scale((11000, 10000), (12100, 11000)).is_err());
        assert_eq!(Method::default(), Method::Generative);
    }

    #[test]
    fn enhance_curated_profiles_accept_pinned_core_weights_only() {
        for method in [Method::Faithful, Method::Rescue] {
            for scale in [2, 4] {
                let id = ModelId::intern(method.model(scale).unwrap().id);
                let preset = crate::catalog::preset(id, "default").unwrap();
                assert_eq!(preset.files[0].role, crate::family::Role::Upscaler);
                assert!(preset.files[0].name.ends_with(".pth"));
                assert_eq!(preset.files[0].sha256.len(), 64);
                let mut family = id.info().resolved.clone();
                crate::family::validate(&family).unwrap();
                family.models[0].presets[0].files[0].role = crate::family::Role::Checkpoint;
                assert!(crate::family::validate(&family).is_err());
            }
        }
    }
    #[test]
    fn enhance_cancelled_before_loading_or_network() {
        let ctl = JobControl::new();
        ctl.cancel();
        let err = Ai::new("127.0.0.1:1").enhance(&RgbaImage::new(2, 2), 4, 4, Method::Faithful, Backend::Auto, Path::new("missing"), 0, &ctl).unwrap_err();
        assert!(err.is::<crate::Cancelled>());
    }

    #[test]
    fn enhance_core_gpu_graph_keeps_alpha_and_exact_dimensions() {
        let server = crate::mock::MockComfy::start().unwrap();
        let ai = Ai::new(server.host());
        let input = RgbaImage::from_fn(31, 19, |x, _| image::Rgba([80, 120, 200, if x < 4 { 0 } else { 255 }]));
        for method in [Method::Faithful, Method::Rescue] {
            for scale in [2, 4] {
                let choice = selection::Choice::Curated { id: method.model(scale).unwrap().id.into() };
                let output =
                    ai.enhance_with_choice(&input, 31 * scale, 19 * scale, method, Backend::Gpu, Path::new("unused"), 0, &JobControl::new(), &choice).unwrap();
                assert_eq!(output.dimensions(), (31 * scale, 19 * scale));
                assert_eq!(output.get_pixel(0, 0)[3], 0);
                assert_eq!(output.get_pixel(20, 10)[3], 255);
            }
        }
        let prompts = server.prompts();
        assert_eq!(prompts.len(), 4);
        for graph in prompts {
            assert!(graph.to_string().contains("ImageUpscaleWithModel"));
            assert!(!graph.to_string().contains("KSampler"));
        }
    }

    #[test]
    fn enhance_download_flow_verifies_retries_and_cancels_in_temp_dir() {
        use sha2::{Digest, Sha256};
        let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
        let authority = server.server_addr().to_string();
        crate::download::allow_test_host(&authority);
        let payload = vec![17u8; 128];
        let url: &'static str = Box::leak(format!("http://{authority}/model").into_boxed_str());
        let sha: &'static str = Box::leak(format!("{:x}", Sha256::digest(&payload)).into_boxed_str());
        let spec = li_seg::ModelSpec { file: "fixture.onnx", bytes: payload.len() as u64, sha256: sha, url, ..*Method::Faithful.model(2).unwrap() };
        let root = std::env::temp_dir().join(format!("li-enhance-download-{}-{}", std::process::id(), authority.replace(':', "-")));
        std::fs::create_dir_all(&root).unwrap();
        let sender = std::thread::spawn(move || {
            for _ in 0..3 {
                server.recv().unwrap().respond(tiny_http::Response::from_data(payload.clone())).unwrap();
            }
        });
        let ctl = JobControl::new();
        download_cpu_spec(&spec, &root, &ctl, &|_| {}).unwrap();
        assert!(li_seg::file_installed(&root, spec.files().next().unwrap()));
        download_cpu_spec(&spec, &root, &ctl, &|_| {}).unwrap(); // no second request
        std::fs::remove_file(li_seg::model_path(&root, &spec)).unwrap();
        let wrong = li_seg::ModelSpec { sha256: "0000", ..spec };
        assert!(download_cpu_spec(&wrong, &root, &ctl, &|_| {}).is_err());
        assert!(!li_seg::model_path(&root, &spec).exists());
        let cancel = JobControl::new();
        let err = download_cpu_spec(&spec, &root, &cancel, &|_| cancel.cancel()).unwrap_err();
        assert!(err.is::<crate::Cancelled>() || err.to_string().contains("Cancelled"));
        assert!(!li_seg::model_path(&root, &spec).exists());
        assert!(std::fs::read_dir(root.join("segmentation")).unwrap().next().is_none());
        sender.join().unwrap();
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    #[ignore = "Opt-in: pinned test-cache models and a real CUDA GPU through ComfyUI"]
    fn enhance_real_gpu_models() {
        let dir = PathBuf::from(std::env::var("LOCAL_IMAGE_TEST_MODELS").unwrap());
        let ai = Ai::new(std::env::var("LOCAL_IMAGE_COMFY").unwrap());
        let stats = ai.client.system_stats().unwrap();
        assert!(
            stats["devices"]
                .as_array()
                .unwrap()
                .iter()
                .any(|d| d["type"].as_str() == Some("cuda") && d["name"].as_str().is_some_and(|name| name.contains("RTX 5090"))),
            "real RTX 5090 required: {stats}"
        );
        let queue = ai.client.queue_state().unwrap();
        assert!(["queue_running", "queue_pending"].iter().all(|k| queue[k].as_array().is_some_and(Vec::is_empty)));
        let input = RgbaImage::from_fn(67, 49, |x, y| image::Rgba([(x * 3) as u8, (y * 4) as u8, 100, 255]));
        for method in [Method::Faithful, Method::Rescue] {
            for scale in [2, 4] {
                let start = std::time::Instant::now();
                let choice = selection::Choice::Curated { id: method.model(scale).unwrap().id.into() };
                let out = ai.enhance_with_choice(&input, 67 * scale, 49 * scale, method, Backend::Gpu, &dir, 0, &JobControl::new(), &choice).unwrap();
                assert_eq!(out.dimensions(), (67 * scale, 49 * scale));
                assert!(out.pixels().any(|p| p[0] > 0));
                eprintln!("{} {scale}x GPU: {:.3}s", method.key(), start.elapsed().as_secs_f64());
            }
        }
    }
}
