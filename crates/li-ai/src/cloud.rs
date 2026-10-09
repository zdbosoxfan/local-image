//! Cloud generation with the user's own API key: OpenAI (GPT Image), Google (Gemini image),
//! Black Forest Labs (FLUX pro) and Stability AI. Optional and off until a key is entered in
//! Local AI settings; every request goes straight from this computer to the provider (no relay),
//! and the Generate panel says which provider receives the prompt and images.
//!
//! Each model states what it can do (create, edit an image, fill a selection, reference images).
//! Fill works on every provider: with a native mask where the API has one, otherwise by painting
//! the selection green and asking the model to fill it (the same trick as local instruction
//! models); either way the result is composited back through the feathered selection
//! ([`crate::inpaint`]), so pixels outside it never change.
//!
//! Base URLs can be redirected (`LOCAL_IMAGE_CLOUD_BASE`) to the mock server for tests.

use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use base64::Engine as _;
use image::{GrayImage, Luma, RgbaImage};
use serde_json::{Value, json};

use crate::comfy::JobControl;
use crate::settings::AiSettings;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Provider {
    OpenAi,
    Google,
    Bfl,
    Stability,
}

impl Provider {
    pub const ALL: [Provider; 4] = [Provider::OpenAi, Provider::Google, Provider::Bfl, Provider::Stability];
    pub fn label(self) -> &'static str {
        match self {
            Provider::OpenAi => "OpenAI",
            Provider::Google => "Google",
            Provider::Bfl => "Black Forest Labs",
            Provider::Stability => "Stability AI",
        }
    }
    /// The settings key holding its API key.
    pub fn key_name(self) -> &'static str {
        match self {
            Provider::OpenAi => "openai_api_key",
            Provider::Google => "google_api_key",
            Provider::Bfl => "bfl_api_key",
            Provider::Stability => "stability_api_key",
        }
    }
    /// Where to get a key.
    pub fn keys_url(self) -> &'static str {
        match self {
            Provider::OpenAi => "https://platform.openai.com/api-keys",
            Provider::Google => "https://aistudio.google.com/apikey",
            Provider::Bfl => "https://dashboard.bfl.ai/",
            Provider::Stability => "https://platform.stability.ai/account/keys",
        }
    }
    fn default_base(self) -> &'static str {
        match self {
            Provider::OpenAi => "https://api.openai.com",
            Provider::Google => "https://generativelanguage.googleapis.com",
            Provider::Bfl => "https://api.bfl.ai",
            Provider::Stability => "https://api.stability.ai",
        }
    }
    fn slug(self) -> &'static str {
        match self {
            Provider::OpenAi => "openai",
            Provider::Google => "google",
            Provider::Bfl => "bfl",
            Provider::Stability => "stability",
        }
    }
    pub fn base(self) -> String {
        let over = BASE_OVERRIDE.lock().ok().and_then(|b| b.clone()).or_else(|| std::env::var("LOCAL_IMAGE_CLOUD_BASE").ok().filter(|b| !b.trim().is_empty()));
        match over {
            Some(b) => format!("{}/{}", b.trim_end_matches('/'), self.slug()),
            None => self.default_base().to_owned(),
        }
    }
}

static BASE_OVERRIDE: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);

/// Sends every provider's requests to `base/{openai,google,bfl,stability}` (the mock server).
#[doc(hidden)]
pub fn set_test_base(base: Option<String>) {
    if let Ok(mut b) = BASE_OVERRIDE.lock() {
        *b = base;
    }
}

/// A cloud model and what it can do.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CloudModel {
    /// `provider:model`, the Generate panel's model key after `cloud:`.
    pub key: &'static str,
    pub provider: Provider,
    /// The provider's model id.
    pub model: &'static str,
    pub label: &'static str,
    pub create: bool,
    pub edit: bool,
    pub fill: bool,
    /// Reference images it takes (beyond the image being edited).
    pub max_refs: u32,
    pub negative: bool,
    pub best_for: &'static str,
}

pub const MODELS: &[CloudModel] = &[
    CloudModel {
        key: "openai:gpt-image-1",
        provider: Provider::OpenAi,
        model: "gpt-image-1",
        label: "GPT Image 1",
        create: true,
        edit: true,
        fill: true,
        max_refs: 8,
        negative: false,
        best_for: "Instruction edits, text in images, combining references",
    },
    CloudModel {
        key: "google:gemini-2.5-flash-image",
        provider: Provider::Google,
        model: "gemini-2.5-flash-image",
        label: "Gemini 2.5 Flash Image",
        create: true,
        edit: true,
        fill: true,
        max_refs: 3,
        negative: false,
        best_for: "Fast conversational edits and consistent characters",
    },
    CloudModel {
        key: "bfl:flux-kontext-pro",
        provider: Provider::Bfl,
        model: "flux-kontext-pro",
        label: "FLUX.1 Kontext [pro]",
        create: true,
        edit: true,
        fill: true,
        max_refs: 0,
        negative: false,
        best_for: "Precise local edits that keep the rest of the image",
    },
    CloudModel {
        key: "bfl:flux-pro-1.1",
        provider: Provider::Bfl,
        model: "flux-pro-1.1",
        label: "FLUX1.1 [pro]",
        create: true,
        edit: false,
        fill: false,
        max_refs: 0,
        negative: false,
        best_for: "High-quality images from a description",
    },
    CloudModel {
        key: "bfl:flux-pro-1.0-fill",
        provider: Provider::Bfl,
        model: "flux-pro-1.0-fill",
        label: "FLUX.1 Fill [pro]",
        create: false,
        edit: false,
        fill: true,
        max_refs: 0,
        negative: false,
        best_for: "Filling a selection with a native mask",
    },
    CloudModel {
        key: "stability:sd3.5-large",
        provider: Provider::Stability,
        model: "sd3.5-large",
        label: "Stable Diffusion 3.5 Large",
        create: true,
        edit: true,
        fill: false,
        max_refs: 0,
        negative: true,
        best_for: "Varied styles; restyling an image at a strength",
    },
    CloudModel {
        key: "stability:ultra",
        provider: Provider::Stability,
        model: "ultra",
        label: "Stable Image Ultra",
        create: true,
        edit: false,
        fill: false,
        max_refs: 0,
        negative: true,
        best_for: "Photorealistic images from a description",
    },
    CloudModel {
        key: "stability:inpaint",
        provider: Provider::Stability,
        model: "inpaint",
        label: "Stable Image Inpaint",
        create: false,
        edit: false,
        fill: true,
        max_refs: 0,
        negative: true,
        best_for: "Filling a selection with a native mask",
    },
];

pub fn model(key: &str) -> Option<&'static CloudModel> {
    MODELS.iter().find(|m| m.key == key)
}

/// The user's key for `p`, if entered.
pub fn api_key(settings: &AiSettings, p: Provider) -> Option<String> {
    settings.extra_string(p.key_name())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CloudMode {
    Create,
    Edit,
    Fill,
}

#[derive(Clone, Debug)]
pub struct CloudRequest {
    pub model: &'static CloudModel,
    pub mode: CloudMode,
    pub prompt: String,
    pub negative: String,
    pub width: u32,
    pub height: u32,
    pub seed: u64,
    pub count: u32,
    /// The image being edited or filled.
    pub source: Option<RgbaImage>,
    /// Fill: white = regenerate.
    pub mask: Option<GrayImage>,
    pub refs: Vec<RgbaImage>,
    /// Edit strength for models that restyle at a strength (0–1).
    pub strength: f32,
}

impl CloudRequest {
    pub fn validate(&self) -> Result<()> {
        let m = self.model;
        match self.mode {
            CloudMode::Create if !m.create => bail!("{} doesn't create images from a description.", m.label),
            CloudMode::Edit if !m.edit => bail!("{} doesn't edit images.", m.label),
            CloudMode::Fill if !m.fill => bail!("{} doesn't fill selections.", m.label),
            CloudMode::Edit | CloudMode::Fill if self.source.is_none() => bail!("Open an image first."),
            CloudMode::Fill if self.mask.is_none() => bail!("Make a selection to fill."),
            _ => {}
        }
        if self.refs.len() as u32 > m.max_refs {
            bail!("{} takes at most {} reference image{}.", m.label, m.max_refs, if m.max_refs == 1 { "" } else { "s" });
        }
        if self.prompt.trim().is_empty() && self.mode != CloudMode::Fill {
            bail!("Describe what you want first.");
        }
        Ok(())
    }
}

// ------------------------------------------------------------------------------ transport

const HOSTS: &[&str] = &["api.openai.com", "generativelanguage.googleapis.com", "api.bfl.ai", "api.stability.ai"];

/// Requests go only to the providers (and BFL's result delivery hosts), over HTTPS; plain HTTP
/// only to an allowed test host (the mock).
pub fn host_allowed(url: &str) -> bool {
    if url.starts_with("http://") {
        return crate::download::host_allowed(url);
    }
    let Some(rest) = url.strip_prefix("https://") else { return false };
    let host = rest.split(['/', '?', '#', ':']).next().unwrap_or("").to_ascii_lowercase();
    HOSTS.contains(&host.as_str()) || host.ends_with(".bfl.ai")
}

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_connect(Some(Duration::from_secs(20)))
        .timeout_global(Some(Duration::from_secs(300)))
        .max_redirects(0)
        .http_status_as_error(false)
        .user_agent(concat!("LocalImage/", env!("CARGO_PKG_VERSION")))
        .build()
        .into()
}

fn check(url: &str) -> Result<()> {
    if !host_allowed(url) {
        bail!("Refusing to contact an unexpected host: {url}");
    }
    Ok(())
}

/// The provider's error message, if the body has one.
fn error_text(provider: Provider, status: u16, body: &[u8]) -> String {
    let v: Value = serde_json::from_slice(body).unwrap_or(Value::Null);
    let msg = v["error"]["message"]
        .as_str()
        .or_else(|| v["errors"][0].as_str())
        .or_else(|| v["detail"].as_str())
        .or_else(|| v["message"].as_str())
        .map(str::to_owned)
        .unwrap_or_else(|| String::from_utf8_lossy(&body[..body.len().min(300)]).into_owned());
    match status {
        401 | 403 => format!("{} rejected the API key ({status}): {msg}", provider.label()),
        402 | 429 => format!("{} says the account is out of credit or rate-limited ({status}): {msg}", provider.label()),
        _ => format!("{} returned an error ({status}): {msg}", provider.label()),
    }
}

fn read(provider: Provider, r: ureq::http::Response<ureq::Body>) -> Result<Vec<u8>> {
    let status = r.status().as_u16();
    let mut r = r;
    let body = r.body_mut().with_config().limit(64 << 20).read_to_vec()?;
    if !(200..300).contains(&status) {
        bail!("{}", error_text(provider, status, &body));
    }
    Ok(body)
}

fn post_json(p: Provider, url: &str, headers: &[(&str, String)], body: &Value) -> Result<Vec<u8>> {
    check(url)?;
    let mut req = agent().post(url).header("Content-Type", "application/json");
    for (k, v) in headers {
        req = req.header(*k, v);
    }
    let r = req.send(serde_json::to_vec(body)?.as_slice()).with_context(|| format!("Could not reach {}", p.label()))?;
    read(p, r)
}

/// A multipart body: text fields and PNG files.
struct Multipart {
    boundary: String,
    body: Vec<u8>,
}

impl Multipart {
    fn new() -> Self {
        Self { boundary: format!("----localimage{}", uuid::Uuid::new_v4().simple()), body: Vec::new() }
    }
    fn text(&mut self, name: &str, value: &str) {
        self.body.extend_from_slice(format!("--{}\r\nContent-Disposition: form-data; name=\"{name}\"\r\n\r\n{value}\r\n", self.boundary).as_bytes());
    }
    fn png(&mut self, name: &str, file: &str, data: &[u8]) {
        self.body.extend_from_slice(
            format!("--{}\r\nContent-Disposition: form-data; name=\"{name}\"; filename=\"{file}\"\r\nContent-Type: image/png\r\n\r\n", self.boundary)
                .as_bytes(),
        );
        self.body.extend_from_slice(data);
        self.body.extend_from_slice(b"\r\n");
    }
    fn finish(mut self) -> (String, Vec<u8>) {
        self.body.extend_from_slice(format!("--{}--\r\n", self.boundary).as_bytes());
        (format!("multipart/form-data; boundary={}", self.boundary), self.body)
    }
}

fn post_multipart(p: Provider, url: &str, headers: &[(&str, String)], form: Multipart) -> Result<Vec<u8>> {
    check(url)?;
    let (ctype, body) = form.finish();
    let mut req = agent().post(url).header("Content-Type", &ctype);
    for (k, v) in headers {
        req = req.header(*k, v);
    }
    let r = req.send(body.as_slice()).with_context(|| format!("Could not reach {}", p.label()))?;
    read(p, r)
}

fn get(p: Provider, url: &str, headers: &[(&str, String)]) -> Result<Vec<u8>> {
    check(url)?;
    let mut req = agent().get(url);
    for (k, v) in headers {
        req = req.header(*k, v);
    }
    let r = req.call().with_context(|| format!("Could not reach {}", p.label()))?;
    read(p, r)
}

fn b64(bytes: &[u8]) -> String {
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

fn unb64(s: &str) -> Result<Vec<u8>> {
    Ok(base64::engine::general_purpose::STANDARD.decode(s.trim())?)
}

fn png(img: &RgbaImage) -> Result<Vec<u8>> {
    crate::imaging::encode_png(img)
}

fn png_gray(img: &GrayImage) -> Result<Vec<u8>> {
    crate::imaging::encode_png(img)
}

// ------------------------------------------------------------------------------ sizes

/// The nearest of `ratios` (`"W:H"`) to `w × h`.
fn nearest_ratio(w: u32, h: u32, ratios: &[&'static str]) -> &'static str {
    let want = (w.max(1) as f32 / h.max(1) as f32).ln();
    ratios
        .iter()
        .copied()
        .min_by(|a, b| {
            let r = |s: &str| {
                let (x, y) = s.split_once(':').unwrap_or(("1", "1"));
                (x.parse::<f32>().unwrap_or(1.0) / y.parse::<f32>().unwrap_or(1.0)).ln()
            };
            (r(a) - want).abs().total_cmp(&(r(b) - want).abs())
        })
        .unwrap_or("1:1")
}

/// OpenAI's sizes.
fn openai_size(w: u32, h: u32) -> &'static str {
    match nearest_ratio(w, h, &["1:1", "3:2", "2:3"]) {
        "3:2" => "1536x1024",
        "2:3" => "1024x1536",
        _ => "1024x1024",
    }
}

const GOOGLE_RATIOS: &[&str] = &["1:1", "2:3", "3:2", "3:4", "4:3", "4:5", "5:4", "9:16", "16:9", "21:9"];
const BFL_RATIOS: &[&str] = &["1:1", "2:3", "3:2", "3:4", "4:3", "9:16", "16:9", "9:21", "21:9"];
const STABILITY_RATIOS: &[&str] = &["1:1", "2:3", "3:2", "4:5", "5:4", "9:16", "16:9", "9:21", "21:9"];

/// What the image being edited or filled is sent as: the source (the green-filled source for
/// mask-less fills), shrunk to at most `max` per side.
fn prepared_source(req: &CloudRequest, max: u32, green: bool) -> Option<RgbaImage> {
    let src = req.source.as_ref()?;
    let img = match (&req.mask, green) {
        (Some(m), true) => {
            let rgb = image::DynamicImage::ImageRgba8(crate::imaging::over_white(src)).to_rgb8();
            image::DynamicImage::ImageRgb8(crate::inpaint::green_fill(&rgb, m)).to_rgba8()
        }
        _ => src.clone(),
    };
    Some(shrink(&img, max))
}

fn shrink(img: &RgbaImage, max: u32) -> RgbaImage {
    let (w, h) = img.dimensions();
    if w.max(h) <= max {
        return img.clone();
    }
    let k = max as f32 / w.max(h) as f32;
    image::imageops::resize(img, ((w as f32 * k).round() as u32).max(1), ((h as f32 * k).round() as u32).max(1), image::imageops::FilterType::Lanczos3)
}

fn shrink_gray(img: &GrayImage, w: u32, h: u32) -> GrayImage {
    if img.dimensions() == (w, h) {
        return img.clone();
    }
    image::imageops::resize(img, w, h, image::imageops::FilterType::Triangle)
}

fn prompt_for(req: &CloudRequest, green: bool) -> String {
    let p = req.prompt.trim();
    if req.mode == CloudMode::Fill && green {
        if p.is_empty() { crate::inpaint::GREEN_INSTRUCTION.to_owned() } else { format!("{} Fill it with: {p}", crate::inpaint::GREEN_INSTRUCTION) }
    } else if req.mode == CloudMode::Fill && p.is_empty() {
        "Fill the area so it blends with the rest of the image".to_owned()
    } else {
        p.to_owned()
    }
}

// ------------------------------------------------------------------------------ providers

fn openai(req: &CloudRequest, key: &str, ctl: &JobControl) -> Result<Vec<RgbaImage>> {
    let base = Provider::OpenAi.base();
    let auth = [("Authorization", format!("Bearer {key}"))];
    let n = req.count.clamp(1, 4);
    let body = if req.mode == CloudMode::Create && req.refs.is_empty() {
        let v = json!({ "model": req.model.model, "prompt": prompt_for(req, false), "n": n, "size": openai_size(req.width, req.height) });
        post_json(Provider::OpenAi, &format!("{base}/v1/images/generations"), &auth, &v)?
    } else {
        let mut f = Multipart::new();
        f.text("model", req.model.model);
        f.text("prompt", &prompt_for(req, false));
        f.text("n", &n.to_string());
        let src = prepared_source(req, 2048, false);
        let (w, h) = src.as_ref().map(|s| s.dimensions()).unwrap_or((req.width, req.height));
        f.text("size", openai_size(w, h));
        if let Some(s) = &src {
            f.png("image[]", "image.png", &png(s)?);
        }
        for (i, r) in req.refs.iter().enumerate() {
            f.png("image[]", &format!("ref{i}.png"), &png(&shrink(r, 2048))?);
        }
        if let (CloudMode::Fill, Some(m), Some(s)) = (req.mode, &req.mask, &src) {
            // OpenAI's mask: transparent where to edit, the same size as the image.
            let m = shrink_gray(m, s.width(), s.height());
            let mask = RgbaImage::from_fn(s.width(), s.height(), |x, y| image::Rgba([0, 0, 0, 255 - m.get_pixel(x, y)[0]]));
            f.png("mask", "mask.png", &png(&mask)?);
        }
        ctl.check()?;
        post_multipart(Provider::OpenAi, &format!("{base}/v1/images/edits"), &auth, f)?
    };
    let v: Value = serde_json::from_slice(&body)?;
    let images: Vec<RgbaImage> = v["data"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|d| d["b64_json"].as_str())
        .map(|s| crate::imaging::decode_rgba(&unb64(s)?))
        .collect::<Result<_>>()?;
    if images.is_empty() {
        bail!("OpenAI returned no image.");
    }
    Ok(images)
}

fn google(req: &CloudRequest, key: &str, ctl: &JobControl) -> Result<Vec<RgbaImage>> {
    let base = Provider::Google.base();
    let url = format!("{base}/v1beta/models/{}:generateContent", req.model.model);
    let auth = [("x-goog-api-key", key.to_owned())];
    let mut parts = Vec::new();
    if let Some(s) = prepared_source(req, 2048, req.mode == CloudMode::Fill) {
        parts.push(json!({ "inline_data": { "mime_type": "image/png", "data": b64(&png(&s)?) } }));
    }
    for r in &req.refs {
        parts.push(json!({ "inline_data": { "mime_type": "image/png", "data": b64(&png(&shrink(r, 2048))?) } }));
    }
    parts.push(json!({ "text": prompt_for(req, true) }));
    let (w, h) = req.source.as_ref().map(|s| s.dimensions()).unwrap_or((req.width, req.height));
    let body = json!({
        "contents": [{ "parts": parts }],
        "generationConfig": { "responseModalities": ["IMAGE"], "imageConfig": { "aspectRatio": nearest_ratio(w, h, GOOGLE_RATIOS) } }
    });
    let mut out = Vec::new();
    for _ in 0..req.count.clamp(1, 4) {
        ctl.check()?;
        let v: Value = serde_json::from_slice(&post_json(Provider::Google, &url, &auth, &body)?)?;
        let found = v["candidates"]
            .as_array()
            .into_iter()
            .flatten()
            .flat_map(|c| c["content"]["parts"].as_array().cloned().unwrap_or_default())
            .find_map(|p| p.get("inlineData").or_else(|| p.get("inline_data")).and_then(|d| d["data"].as_str()).map(str::to_owned));
        match found {
            Some(d) => out.push(crate::imaging::decode_rgba(&unb64(&d)?)?),
            None => {
                let why =
                    v["candidates"][0]["finishReason"].as_str().or_else(|| v["promptFeedback"]["blockReason"].as_str()).unwrap_or("no image in the reply");
                bail!("Google returned no image ({why}).");
            }
        }
    }
    Ok(out)
}

fn bfl(req: &CloudRequest, key: &str, ctl: &JobControl) -> Result<Vec<RgbaImage>> {
    let base = Provider::Bfl.base();
    let auth = [("x-key", key.to_owned())];
    let model = req.model.model;
    let mut out = Vec::new();
    for i in 0..req.count.clamp(1, 4) {
        ctl.check()?;
        let seed = req.seed.wrapping_add(i as u64) % 4_294_967_295;
        let mut body = json!({ "prompt": prompt_for(req, model != "flux-pro-1.0-fill"), "seed": seed, "output_format": "png" });
        let (w, h) = req.source.as_ref().map(|s| s.dimensions()).unwrap_or((req.width, req.height));
        match model {
            "flux-pro-1.0-fill" => {
                let s = prepared_source(req, 2048, false).context("Open an image first.")?;
                let m = shrink_gray(req.mask.as_ref().context("Make a selection to fill.")?, s.width(), s.height());
                body["image"] = json!(b64(&png(&s)?));
                body["mask"] = json!(b64(&png_gray(&m)?));
            }
            "flux-pro-1.1" => {
                let k = (1024.0 * 1024.0 / (w.max(1) as f32 * h.max(1) as f32)).sqrt();
                let snap = |v: f32| (((v * k) / 32.0).round() as u32 * 32).clamp(256, 1440);
                body["width"] = json!(snap(w as f32));
                body["height"] = json!(snap(h as f32));
            }
            _ => {
                body["aspect_ratio"] = json!(nearest_ratio(w, h, BFL_RATIOS));
                if let Some(s) = prepared_source(req, 2048, req.mode == CloudMode::Fill) {
                    body["input_image"] = json!(b64(&png(&s)?));
                }
            }
        }
        let v: Value = serde_json::from_slice(&post_json(Provider::Bfl, &format!("{base}/v1/{model}"), &auth, &body)?)?;
        let poll = v["polling_url"]
            .as_str()
            .map(str::to_owned)
            .or_else(|| v["id"].as_str().map(|id| format!("{base}/v1/get_result?id={id}")))
            .context("Black Forest Labs didn't return a job")?;
        let start = Instant::now();
        let sample = loop {
            ctl.check()?;
            if start.elapsed() > Duration::from_secs(300) {
                bail!("Black Forest Labs took too long.");
            }
            std::thread::sleep(Duration::from_millis(if cfg!(test) { 20 } else { 800 }));
            let r: Value = serde_json::from_slice(&get(Provider::Bfl, &poll, &auth)?)?;
            match r["status"].as_str().unwrap_or("") {
                "Ready" => break r["result"]["sample"].as_str().context("no result URL")?.to_owned(),
                "Pending" | "Queued" | "Processing" | "Task not found" => {}
                other => bail!("Black Forest Labs: {other}"),
            }
        };
        out.push(crate::imaging::decode_rgba(&get(Provider::Bfl, &sample, &[])?)?);
    }
    Ok(out)
}

fn stability(req: &CloudRequest, key: &str, ctl: &JobControl) -> Result<Vec<RgbaImage>> {
    let base = Provider::Stability.base();
    let auth = [("Authorization", format!("Bearer {key}")), ("Accept", "image/*".to_owned())];
    let mut out = Vec::new();
    for i in 0..req.count.clamp(1, 4) {
        ctl.check()?;
        let mut f = Multipart::new();
        f.text("prompt", &prompt_for(req, false));
        if !req.negative.trim().is_empty() {
            f.text("negative_prompt", req.negative.trim());
        }
        f.text("seed", &(req.seed.wrapping_add(i as u64) % 4_294_967_294).to_string());
        f.text("output_format", "png");
        let url = match (req.model.model, req.mode) {
            ("inpaint", _) => {
                let s = prepared_source(req, 2048, false).context("Open an image first.")?;
                let m = shrink_gray(req.mask.as_ref().context("Make a selection to fill.")?, s.width(), s.height());
                f.png("image", "image.png", &png(&s)?);
                f.png("mask", "mask.png", &png_gray(&m)?);
                format!("{base}/v2beta/stable-image/edit/inpaint")
            }
            ("sd3.5-large", mode) => {
                f.text("model", "sd3.5-large");
                if mode == CloudMode::Edit {
                    let s = prepared_source(req, 2048, false).context("Open an image first.")?;
                    f.text("mode", "image-to-image");
                    f.text("strength", &format!("{:.2}", req.strength.clamp(0.05, 1.0)));
                    f.png("image", "image.png", &png(&s)?);
                } else {
                    f.text("aspect_ratio", nearest_ratio(req.width, req.height, STABILITY_RATIOS));
                }
                format!("{base}/v2beta/stable-image/generate/sd3")
            }
            (m, _) => {
                f.text("aspect_ratio", nearest_ratio(req.width, req.height, STABILITY_RATIOS));
                format!("{base}/v2beta/stable-image/generate/{m}")
            }
        };
        out.push(crate::imaging::decode_rgba(&post_multipart(Provider::Stability, &url, &auth, f)?)?);
    }
    Ok(out)
}

/// Runs a cloud request. Fill results are composited onto the source through the feathered
/// selection; edits come back at the provider's size and are scaled to the source's.
pub fn generate(req: &CloudRequest, settings: &AiSettings, ctl: &JobControl) -> Result<Vec<RgbaImage>> {
    req.validate()?;
    let p = req.model.provider;
    let key = api_key(settings, p).with_context(|| format!("Add your {} API key in Edit › Preferences › Local AI › Cloud.", p.label()))?;
    let raw = match p {
        Provider::OpenAi => openai(req, &key, ctl)?,
        Provider::Google => google(req, &key, ctl)?,
        Provider::Bfl => bfl(req, &key, ctl)?,
        Provider::Stability => stability(req, &key, ctl)?,
    };
    let Some(src) = req.source.as_ref().filter(|_| req.mode != CloudMode::Create) else { return Ok(raw) };
    let (w, h) = src.dimensions();
    let fit = |img: RgbaImage| if img.dimensions() == (w, h) { img } else { image::imageops::resize(&img, w, h, image::imageops::FilterType::Lanczos3) };
    if req.mode == CloudMode::Fill
        && let Some(mask) = &req.mask
    {
        let plan = crate::inpaint::plan(mask).context("The selection is empty.")?;
        let comp = crate::inpaint::compositing_mask(mask, &plan);
        let canvas = image::DynamicImage::ImageRgba8(src.clone()).to_rgb8();
        // The whole image came back: composite its context crop.
        let whole = crate::inpaint::Plan { context: crate::imaging::Rect { x0: 0, y0: 0, x1: w, y1: h }, ..plan };
        return Ok(raw
            .into_iter()
            .map(|r| {
                let rgb = image::DynamicImage::ImageRgba8(fit(r)).to_rgb8();
                let mut out = image::DynamicImage::ImageRgb8(crate::inpaint::composite(&canvas, &rgb, &whole, &comp)).to_rgba8();
                // Keep the source's transparency outside the selection.
                for (x, y, p) in out.enumerate_pixels_mut() {
                    let a = comp.get_pixel(x, y)[0] as u32;
                    let sa = src.get_pixel(x, y)[3] as u32;
                    p[3] = ((sa * (255 - a) + 255 * a) / 255) as u8;
                }
                out
            })
            .collect());
    }
    Ok(raw.into_iter().map(fit).collect())
}

/// The mask in pixels of `w × h` (white = selected) from a selection that may be another size.
pub fn mask_for(sel: &GrayImage, w: u32, h: u32) -> GrayImage {
    if sel.dimensions() == (w, h) { sel.clone() } else { GrayImage::from_fn(w, h, |x, y| Luma([sel.get_pixel(x * sel.width() / w, y * sel.height() / h)[0]])) }
}

/// The size or aspect ratio the provider is asked for, for display.
pub fn describe_size(m: &CloudModel, w: u32, h: u32) -> String {
    match m.provider {
        Provider::OpenAi => openai_size(w, h).replace('x', " × "),
        Provider::Google => nearest_ratio(w, h, GOOGLE_RATIOS).to_owned(),
        Provider::Bfl => nearest_ratio(w, h, BFL_RATIOS).to_owned(),
        Provider::Stability => nearest_ratio(w, h, STABILITY_RATIOS).to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_and_hosts() {
        assert_eq!(openai_size(1920, 1080), "1536x1024");
        assert_eq!(openai_size(800, 1200), "1024x1536");
        assert_eq!(nearest_ratio(1920, 1080, GOOGLE_RATIOS), "16:9");
        assert_eq!(nearest_ratio(1000, 1000, BFL_RATIOS), "1:1");
        assert!(host_allowed("https://api.openai.com/v1/images/generations"));
        assert!(host_allowed("https://delivery-eu1.bfl.ai/results/x.png"));
        assert!(!host_allowed("https://evil.example/v1"));
        assert!(!host_allowed("http://api.openai.com/v1"));
        assert!(MODELS.iter().all(|m| m.create || m.edit || m.fill));
    }

    #[test]
    fn requests_are_validated() {
        let m = model("bfl:flux-pro-1.0-fill").unwrap();
        let r = CloudRequest {
            model: m,
            mode: CloudMode::Create,
            prompt: "x".into(),
            negative: String::new(),
            width: 1024,
            height: 1024,
            seed: 1,
            count: 1,
            source: None,
            mask: None,
            refs: vec![],
            strength: 0.6,
        };
        assert!(r.validate().unwrap_err().to_string().contains("doesn't create"));
        let r = CloudRequest { mode: CloudMode::Fill, ..r };
        assert!(r.validate().unwrap_err().to_string().contains("Open an image"));
    }

    /// Every model in every mode it supports, end to end against the mock providers.
    #[test]
    fn every_model_runs_against_the_mock() {
        let mock = crate::mock::MockComfy::start().unwrap();
        set_test_base(Some(format!("http://{}/cloud", mock.host())));
        let mut settings = AiSettings::default();
        for p in Provider::ALL {
            settings.set_extra_string(p.key_name(), "test-key");
        }
        let ctl = JobControl::new();
        let src = RgbaImage::from_pixel(640, 480, image::Rgba([10, 20, 30, 255]));
        let mask = GrayImage::from_fn(640, 480, |x, y| Luma([if (200..400).contains(&x) && (150..300).contains(&y) { 255 } else { 0 }]));
        for m in MODELS {
            for mode in [CloudMode::Create, CloudMode::Edit, CloudMode::Fill] {
                let ok = match mode {
                    CloudMode::Create => m.create,
                    CloudMode::Edit => m.edit,
                    CloudMode::Fill => m.fill,
                };
                if !ok {
                    continue;
                }
                let req = CloudRequest {
                    model: m,
                    mode,
                    prompt: "a red boat".into(),
                    negative: String::new(),
                    width: 1216,
                    height: 832,
                    seed: 7,
                    count: 2,
                    source: (mode != CloudMode::Create).then(|| src.clone()),
                    mask: (mode == CloudMode::Fill).then(|| mask.clone()),
                    refs: vec![],
                    strength: 0.6,
                };
                let out = generate(&req, &settings, &ctl).unwrap_or_else(|e| panic!("{} {mode:?}: {e:#}", m.key));
                assert_eq!(out.len(), 2, "{} {mode:?}", m.key);
                if mode != CloudMode::Create {
                    assert_eq!(out[0].dimensions(), (640, 480), "{} {mode:?}", m.key);
                }
                if mode == CloudMode::Fill {
                    // Outside the selection nothing changed; inside it did (the mock inverts).
                    assert_eq!(out[0].get_pixel(10, 10).0, [10, 20, 30, 255], "{}", m.key);
                    assert_ne!(out[0].get_pixel(300, 220).0, [10, 20, 30, 255], "{}", m.key);
                }
            }
        }
        // A rejected key says so; a missing key says where to add it.
        settings.set_extra_string("openai_api_key", "bad-key");
        let req = CloudRequest {
            model: model("openai:gpt-image-1").unwrap(),
            mode: CloudMode::Create,
            prompt: "x".into(),
            negative: String::new(),
            width: 1024,
            height: 1024,
            seed: 1,
            count: 1,
            source: None,
            mask: None,
            refs: vec![],
            strength: 0.6,
        };
        let e = generate(&req, &settings, &ctl).unwrap_err().to_string();
        assert!(e.contains("rejected the API key") && e.contains("Incorrect API key"), "{e}");
        settings.set_extra_string("openai_api_key", "");
        let e = generate(&req, &settings, &ctl).unwrap_err().to_string();
        assert!(e.contains("Local AI › Cloud"), "{e}");
        set_test_base(None);
    }
}
