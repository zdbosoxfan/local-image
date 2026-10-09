//! Model families as data: one JSON profile per family (`crates/li-ai/families/*.json`, built in;
//! newer copies fetched from Local Image's GitHub repository and cached; the user's own profiles in
//! `<data>/families/`). A profile says how ComfyUI loads and samples the family (the pipeline
//! recipe the graph builders follow), its native sizes and sampling ranges, what it can do
//! (create, edit, references, inpaint, transparent output), which LoRAs fit it, which text encoder
//! and VAE files it needs, and the curated models it ships with (each with pinned download
//! presets).
//!
//! A profile can name a `base` family; it inherits everything it does not set (Pony and
//! Illustrious from SDXL, FLUX.1 Kontext and Fill from FLUX.1, Qwen Image Edit from Qwen Image).
//!
//! The registry is loaded once and replaced as a whole when profiles update; [`registry`] hands
//! out a `&'static` snapshot (the few replaced snapshots are leaked, which keeps every reference
//! the UI holds valid).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::RwLock;

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// The profile format this build reads.
pub const SCHEMA: u32 = 1;

/// Built-in profiles.
pub const EMBEDDED: &[(&str, &str)] = &[
    ("sd15", include_str!("../families/sd15.json")),
    ("sdxl", include_str!("../families/sdxl.json")),
    ("pony", include_str!("../families/pony.json")),
    ("illustrious", include_str!("../families/illustrious.json")),
    ("sd35", include_str!("../families/sd35.json")),
    ("flux1", include_str!("../families/flux1.json")),
    ("flux1-kontext", include_str!("../families/flux1-kontext.json")),
    ("flux1-fill", include_str!("../families/flux1-fill.json")),
    ("flux2", include_str!("../families/flux2.json")),
    ("flux2-klein", include_str!("../families/flux2-klein.json")),
    ("qwen-image", include_str!("../families/qwen-image.json")),
    ("qwen-edit", include_str!("../families/qwen-edit.json")),
    ("qwen-image-21", include_str!("../families/qwen-image-21.json")),
    ("z-image", include_str!("../families/z-image.json")),
    ("ernie", include_str!("../families/ernie.json")),
    ("hidream", include_str!("../families/hidream.json")),
    ("seedvr2", include_str!("../families/seedvr2.json")),
];

/// Where newer profiles are published (this repository's `crates/li-ai/families/`).
pub const REMOTE_BASE: &str = "https://raw.githubusercontent.com/zdbosoxfan/local-image/v2/crates/li-ai/families";

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Range {
    pub default: f32,
    pub min: f32,
    pub max: f32,
}

impl Range {
    pub fn fixed(self) -> bool {
        self.min == self.max
    }
    pub fn clamp(self, v: f32) -> f32 {
        v.clamp(self.min, self.max)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FamilyKind {
    #[default]
    Image,
    Upscaler,
    /// Detected but only usable through an official template (no builder).
    Basic,
}

/// How the diffusion model is loaded.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Loader {
    /// `CheckpointLoaderSimple` (model, CLIP and VAE in one file).
    #[default]
    Checkpoint,
    /// `UNETLoader` (`diffusion_models/`), or `UnetLoaderGGUF` for `.gguf` files.
    Unet,
}

/// Text encoder loading: `node` is `checkpoint` (from the checkpoint), `CLIPLoader`,
/// `DualCLIPLoader`, `TripleCLIPLoader` or `QuadrupleCLIPLoader`; `type` is the loader's `type`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ClipSpec {
    pub node: String,
    #[serde(rename = "type")]
    pub kind: Option<String>,
    /// SD1.5-style "CLIP skip" (−1 = none; −2 for Pony and most anime models).
    pub skip: i32,
}

/// The conditioning recipe the builders implement.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Encode {
    /// `CLIPTextEncode` for positive and negative.
    #[default]
    ClipText,
    /// Qwen Image 2.1's `TextEncodeQwenImage21` (positive, negative and the reference latent).
    QwenImage21,
    /// `TextEncodeQwenImageEditPlus` with up to three images (Qwen Image Edit 2509 / 2511).
    QwenEditPlus,
}

/// Which sampler graph.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SamplerStyle {
    /// `KSampler`.
    #[default]
    Ksampler,
    /// `SamplerCustomAdvanced` with `Flux2Scheduler`, `CFGGuider`, `RandomNoise`.
    Flux2Custom,
    /// The SeedVR2 upscaler graph.
    SeedVr2,
}

/// How reference images condition a generation.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RefMethod {
    #[default]
    None,
    /// `ReferenceLatent` per image on both conditionings (FLUX.2, Kontext).
    ReferenceLatent,
    /// The text encoder takes the images (Qwen Image 2.1, Qwen Edit).
    TextEncoder,
    /// A starting image (img2img with denoise), e.g. Z-Image.
    InitImage,
    /// IP-Adapter (ComfyUI_IPAdapter_plus: `IPAdapterUnifiedLoader`, `IPAdapterAdvanced`).
    IpAdapter,
    /// FLUX.1 Redux (`CLIPVisionEncode` + `StyleModelApply`).
    Redux,
}

/// How a masked area is regenerated.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InpaintMethod {
    #[default]
    None,
    /// `VAEEncode` + `SetLatentNoiseMask` (+ `DifferentialDiffusion`), with denoise.
    NoiseMask,
    /// `InpaintModelConditioning` (inpainting models such as FLUX.1 Fill).
    ModelConditioning,
    /// The model edits from an instruction; the result is composited through the mask.
    Instruction,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ModelSampling {
    pub node: String,
    pub shift: f32,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Pipeline {
    pub loader: Loader,
    pub clip: ClipSpec,
    /// `checkpoint` or `file` (a separate `VAELoader`).
    pub vae: String,
    pub encode: Encode,
    /// `FluxGuidance` value on the positive conditioning (FLUX.1/2), if any.
    pub flux_guidance: Option<f32>,
    /// Negative conditioning is zeroed (distilled models at CFG 1).
    pub zero_negative: bool,
    pub model_sampling: Option<ModelSampling>,
    /// The empty latent node (`EmptyLatentImage`, `EmptySD3LatentImage`, `EmptyFlux2LatentImage`).
    pub latent: String,
    pub sampler: SamplerStyle,
    pub sampler_name: String,
    pub scheduler: String,
    pub reference: RefMethod,
    pub inpaint: InpaintMethod,
    /// `DifferentialDiffusion` before sampling soft masks.
    pub differential: bool,
    /// Extra `(class, input, value)` combo values the family needs (e.g. a CLIP type).
    pub required_choices: Vec<(String, String, String)>,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Sizes {
    /// The long side the model was trained at (about native × native pixels).
    pub native: u32,
    /// Width and height snap to this.
    pub multiple: u32,
    pub min: u32,
    pub max: u32,
}

impl Default for Sizes {
    fn default() -> Self {
        Self { native: 1024, multiple: 16, min: 256, max: 2048 }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Sampling {
    pub steps: Range,
    pub cfg: Range,
    /// Default img2img / refine strength.
    pub denoise: f32,
}

/// What the family can do (what the Generate panel offers).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Capabilities {
    pub create: bool,
    /// Instruction edits natively (Qwen Edit, Kontext, FLUX.2); otherwise edits run as inpaint.
    pub edit: bool,
    /// Reference images it takes natively (0 = none, or through IP-Adapter / Redux).
    pub references: u32,
    pub inpaint: bool,
    pub transparent: bool,
    pub negative_prompt: bool,
    pub upscale: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LoraRule {
    /// `LoraLoader` (model + CLIP) or `LoraLoaderModelOnly`; empty = no LoRAs.
    pub loader: String,
    /// Families whose LoRAs fit.
    pub compatible: Vec<String>,
    pub max: u32,
}

/// Prompt conventions (Pony's score tags, Illustrious' quality tags).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PromptRules {
    pub prefix: String,
    pub negative: String,
}

/// A component file the family needs (text encoder, VAE…), found among ComfyUI's files by exact
/// name first, then by any of `patterns` (substrings of the lower-case name).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Component {
    pub role: Role,
    pub names: Vec<String>,
    pub patterns: Vec<String>,
    /// Where to get it: a pinned file (size and hash known), or `hf:{repo}/{path}` resolved live.
    pub download: Option<FileSpec>,
    pub source: Option<String>,
}

/// Which loader input reads a file.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    #[default]
    Unet,
    Checkpoint,
    Clip,
    Clip2,
    Clip3,
    Clip4,
    Vae,
    Lora,
    ClipVision,
    StyleModel,
    Upscaler,
}

impl Role {
    /// The loader node and input that lists files of this role (for availability checks).
    pub fn loader(self) -> (&'static str, &'static str) {
        match self {
            Role::Unet => ("UNETLoader", "unet_name"),
            Role::Checkpoint => ("CheckpointLoaderSimple", "ckpt_name"),
            Role::Clip | Role::Clip2 | Role::Clip3 | Role::Clip4 => ("CLIPLoader", "clip_name"),
            Role::Vae => ("VAELoader", "vae_name"),
            Role::Lora => ("LoraLoaderModelOnly", "lora_name"),
            Role::ClipVision => ("CLIPVisionLoader", "clip_name"),
            Role::StyleModel => ("StyleModelLoader", "style_model_name"),
            Role::Upscaler => ("UpscaleModelLoader", "model_name"),
        }
    }
    /// The ComfyUI model folder files of this role install into.
    pub fn folder(self) -> &'static str {
        match self {
            Role::Unet => "diffusion_models",
            Role::Checkpoint => "checkpoints",
            Role::Clip | Role::Clip2 | Role::Clip3 | Role::Clip4 => "text_encoders",
            Role::Vae => "vae",
            Role::Lora => "loras",
            Role::ClipVision => "clip_vision",
            Role::StyleModel => "style_models",
            Role::Upscaler => "upscale_models",
        }
    }
    pub fn clip_slot(self) -> Option<usize> {
        match self {
            Role::Clip => Some(0),
            Role::Clip2 => Some(1),
            Role::Clip3 => Some(2),
            Role::Clip4 => Some(3),
            _ => None,
        }
    }
}

/// A file with a pinned download: size and SHA-256 are always known.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct FileSpec {
    pub role: Role,
    /// Folder under the model directory.
    pub folder: String,
    pub name: String,
    pub bytes: u64,
    pub sha256: String,
    pub url: String,
    /// Other accepted `(bytes, sha256)` for a file that is already present.
    pub compatible: Vec<(u64, String)>,
}

/// A downloadable set of files for a model (one precision).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PresetDef {
    pub variant: String,
    pub label: String,
    pub files: Vec<FileSpec>,
    pub access_url: Option<String>,
    /// Nodes beyond those the family's graphs use (custom nodes, e.g. SeedVR2's).
    pub required_nodes: Vec<String>,
    pub required_choices: Vec<(String, String, String)>,
}

/// Per-model overrides of the family's defaults.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ModelDef {
    /// Stable id (0.7's keys for the original models: `qwen`, `flux2-klein-4b`…).
    pub key: String,
    pub label: String,
    pub short: String,
    pub best_for: String,
    pub description: String,
    pub notes: String,
    pub vram_gb: Option<u32>,
    pub license: Option<String>,
    pub license_url: Option<String>,
    pub sampling: Option<Sampling>,
    pub capabilities: Option<Capabilities>,
    pub pipeline: Option<Value>,
    /// Shown as a recommended starting model in the browser.
    pub starter: bool,
    /// Hidden from the Generate model list (tool-only models such as AI Remove).
    pub tool: bool,
    pub presets: Vec<PresetDef>,
}

/// A model family profile.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Family {
    pub schema: u32,
    pub version: u32,
    pub id: String,
    pub label: String,
    /// The browser groups families under this (Stable Diffusion, FLUX, Qwen…).
    pub group: String,
    pub description: String,
    pub base: Option<String>,
    pub kind: FamilyKind,
    pub pipeline: Pipeline,
    pub sizes: Sizes,
    pub sampling: Sampling,
    pub capabilities: Capabilities,
    pub lora: LoraRule,
    pub prompt: PromptRules,
    pub components: Vec<Component>,
    pub vram_gb: u32,
    pub license: String,
    pub license_url: String,
    /// Civitai `baseModel` values and Hugging Face tags that mean this family (the browser's
    /// filters).
    pub civitai_base_models: Vec<String>,
    pub hf_tags: Vec<String>,
    pub models: Vec<ModelDef>,
}

impl Family {
    /// The sampling, capabilities and pipeline `model` actually runs with.
    pub fn resolved_for(&self, model: &ModelDef) -> Family {
        let mut f = self.clone();
        if let Some(s) = model.sampling {
            f.sampling = s;
        }
        if let Some(c) = model.capabilities {
            f.capabilities = c;
        }
        if let Some(p) = &model.pipeline {
            let mut base = serde_json::to_value(&f.pipeline).unwrap_or_default();
            merge(&mut base, p);
            if let Ok(p) = serde_json::from_value(base) {
                f.pipeline = p;
            }
        }
        if let Some(v) = model.vram_gb {
            f.vram_gb = v;
        }
        f
    }

    pub fn lora_fits(&self, lora_family: &str) -> bool {
        !self.lora.loader.is_empty() && (lora_family == self.id || self.lora.compatible.iter().any(|c| c == lora_family))
    }
}

/// Deep-merges `over` into `base` (objects merge key by key; anything else replaces).
pub fn merge(base: &mut Value, over: &Value) {
    match (base, over) {
        (Value::Object(b), Value::Object(o)) => {
            for (k, v) in o {
                match b.get_mut(k) {
                    Some(bv) if bv.is_object() && v.is_object() => merge(bv, v),
                    _ => {
                        b.insert(k.clone(), v.clone());
                    }
                }
            }
        }
        (b, o) => *b = o.clone(),
    }
}

/// The loaded profiles.
#[derive(Debug, Default)]
pub struct Registry {
    pub families: Vec<Family>,
    /// Where each profile came from (`built-in`, `update`, a user file).
    pub sources: BTreeMap<String, String>,
}

impl Registry {
    pub fn family(&self, id: &str) -> Option<&Family> {
        self.families.iter().find(|f| f.id == id)
    }
    /// The family and model definition for a model key.
    pub fn model(&self, key: &str) -> Option<(&Family, &ModelDef)> {
        self.families.iter().find_map(|f| f.models.iter().find(|m| m.key == key).map(|m| (f, m)))
    }
    pub fn models(&self) -> impl Iterator<Item = (&Family, &ModelDef)> {
        self.families.iter().flat_map(|f| f.models.iter().map(move |m| (f, m)))
    }
    /// Families grouped for menus: `(group, families)` in profile order.
    pub fn groups(&self) -> Vec<(String, Vec<&Family>)> {
        let mut out: Vec<(String, Vec<&Family>)> = Vec::new();
        for f in &self.families {
            match out.iter_mut().find(|(g, _)| *g == f.group) {
                Some((_, v)) => v.push(f),
                None => out.push((f.group.clone(), vec![f])),
            }
        }
        out
    }
}

/// Parses raw profiles (`id → JSON`), resolving `base` inheritance.
pub fn build(raw: &BTreeMap<String, Value>) -> Result<Vec<Family>> {
    fn resolve(id: &str, raw: &BTreeMap<String, Value>, depth: u32) -> Result<Value> {
        if depth > 4 {
            bail!("family {id}: base chain too deep");
        }
        let v = raw.get(id).with_context(|| format!("unknown family {id}"))?;
        let Some(base) = v.get("base").and_then(Value::as_str) else { return Ok(v.clone()) };
        let mut merged = resolve(base, raw, depth + 1)?;
        // A derived family has its own models and identity; everything else is inherited.
        if let Some(o) = merged.as_object_mut() {
            for k in ["models", "version", "description", "civitai_base_models", "hf_tags"] {
                o.remove(k);
            }
        }
        merge(&mut merged, v);
        Ok(merged)
    }
    let mut out = Vec::new();
    for (id, v) in raw {
        let schema = v.get("schema").and_then(Value::as_u64).unwrap_or(0) as u32;
        if schema > SCHEMA {
            log::warn!("family {id}: profile schema {schema} is newer than this build reads ({SCHEMA}); skipped");
            continue;
        }
        let merged = resolve(id, raw, 0)?;
        let mut f: Family = serde_json::from_value(merged).with_context(|| format!("family {id}"))?;
        f.id = id.clone();
        validate(&f)?;
        out.push(f);
    }
    // Built-in order (the menus' order), then anything else by id.
    let order = |id: &str| EMBEDDED.iter().position(|(k, _)| *k == id).unwrap_or(usize::MAX);
    out.sort_by(|a, b| order(&a.id).cmp(&order(&b.id)).then(a.id.cmp(&b.id)));
    Ok(out)
}

/// Rejects profiles a builder could not run.
pub fn validate(f: &Family) -> Result<()> {
    if f.id.is_empty() || f.label.is_empty() {
        bail!("a family needs an id and a label");
    }
    if f.kind == FamilyKind::Image {
        let p = &f.pipeline;
        if p.sampler == SamplerStyle::Ksampler && (p.sampler_name.is_empty() || p.scheduler.is_empty()) {
            bail!("family {}: KSampler needs a sampler and a scheduler", f.id);
        }
        if p.latent.is_empty() {
            bail!("family {}: no latent node", f.id);
        }
        if f.sampling.steps.max < f.sampling.steps.min || f.sampling.steps.max == 0.0 {
            bail!("family {}: bad step range", f.id);
        }
    }
    for m in &f.models {
        if m.key.is_empty() {
            bail!("family {}: a model has no key", f.id);
        }
        for p in &m.presets {
            for file in &p.files {
                if file.sha256.len() != 64 || !file.sha256.bytes().all(|b| b.is_ascii_hexdigit()) {
                    bail!("{}: {} has no valid SHA-256", m.key, file.name);
                }
                if file.bytes == 0 {
                    bail!("{}: {} has no size", m.key, file.name);
                }
                if !(file.name.ends_with(".safetensors") || file.name.ends_with(".gguf")) {
                    bail!("{}: {} is not a .safetensors or .gguf file", m.key, file.name);
                }
            }
        }
    }
    Ok(())
}

/// Where downloaded profile updates are cached.
pub fn cache_dir() -> PathBuf {
    crate::settings::data_root().join("families")
}

/// The built-in profiles, overridden by newer cached updates, plus the user's own (`*.json` in
/// `<data>/families/user/`).
pub fn load(dir: &Path) -> Registry {
    let mut raw: BTreeMap<String, Value> = BTreeMap::new();
    let mut sources = BTreeMap::new();
    for (id, text) in EMBEDDED {
        match serde_json::from_str::<Value>(text) {
            Ok(v) => {
                raw.insert((*id).to_owned(), v);
                sources.insert((*id).to_owned(), "built-in".to_owned());
            }
            Err(e) => log::error!("built-in family {id}: {e}"),
        }
    }
    let version = |v: &Value| v.get("version").and_then(Value::as_u64).unwrap_or(0);
    let mut read_dir = |d: &Path, label: &str| {
        let Ok(rd) = std::fs::read_dir(d) else { return };
        for e in rd.flatten() {
            let p = e.path();
            if p.extension().and_then(|x| x.to_str()) != Some("json") || p.file_name().is_some_and(|n| n == "index.json") {
                continue;
            }
            let Some(id) = p.file_stem().and_then(|s| s.to_str()).map(str::to_owned) else { continue };
            let Ok(v) = std::fs::read(&p).map_err(anyhow::Error::from).and_then(|b| Ok(serde_json::from_slice::<Value>(&b)?)) else {
                log::warn!("family profile {} is unreadable; skipped", p.display());
                continue;
            };
            let newer = raw.get(&id).is_none_or(|old| version(&v) > version(old) || label == "user");
            if newer {
                raw.insert(id.clone(), v);
                sources.insert(id, label.to_owned());
            }
        }
    };
    read_dir(dir, "update");
    read_dir(&dir.join("user"), "user");
    match build(&raw) {
        Ok(families) => Registry { families, sources },
        Err(e) => {
            // A bad update must never take the built-in families down.
            log::error!("family profiles: {e:#}; using the built-in ones");
            let raw: BTreeMap<String, Value> = EMBEDDED.iter().filter_map(|(k, t)| Some(((*k).to_owned(), serde_json::from_str(t).ok()?))).collect();
            let families = build(&raw).unwrap_or_default();
            Registry { sources: families.iter().map(|f| (f.id.clone(), "built-in".to_owned())).collect(), families }
        }
    }
}

static REGISTRY: RwLock<Option<&'static Registry>> = RwLock::new(None);

/// The current registry (loaded on first use).
pub fn registry() -> &'static Registry {
    if let Some(r) = REGISTRY.read().ok().and_then(|g| *g) {
        return r;
    }
    let r: &'static Registry = Box::leak(Box::new(load(&cache_dir())));
    if let Ok(mut g) = REGISTRY.write() {
        if let Some(existing) = *g {
            return existing;
        }
        *g = Some(r);
    }
    r
}

/// Reloads the profiles (after an update or a user edit).
pub fn reload() -> &'static Registry {
    let r: &'static Registry = Box::leak(Box::new(load(&cache_dir())));
    if let Ok(mut g) = REGISTRY.write() {
        *g = Some(r);
    }
    crate::catalog::invalidate();
    r
}

/// Fetches `index.json` (`{"families": {"id": version}}`) from [`REMOTE_BASE`] and downloads every
/// profile newer than the one in use into `dir`. Returns the ids updated. Profiles are checked
/// before they are kept; a bad one is skipped.
pub fn update_from(fetch: &dyn Fn(&str) -> Result<Vec<u8>>, base_url: &str, dir: &Path) -> Result<Vec<String>> {
    let index: Value = serde_json::from_slice(&fetch(&format!("{base_url}/index.json"))?).context("the family index is not JSON")?;
    let entries = index.get("families").and_then(Value::as_object).context("the family index has no families")?;
    let current = registry();
    std::fs::create_dir_all(dir)?;
    let mut updated = Vec::new();
    for (id, ver) in entries {
        let ver = ver.as_u64().unwrap_or(0) as u32;
        if !id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
            continue;
        }
        if current.family(id).is_some_and(|f| f.version >= ver) {
            continue;
        }
        let bytes = match fetch(&format!("{base_url}/{id}.json")) {
            Ok(b) => b,
            Err(e) => {
                log::warn!("family {id}: {e:#}");
                continue;
            }
        };
        // Check it parses and validates on top of what is loaded now.
        let Ok(v) = serde_json::from_slice::<Value>(&bytes) else { continue };
        let mut raw: BTreeMap<String, Value> = EMBEDDED.iter().filter_map(|(k, t)| Some(((*k).to_owned(), serde_json::from_str(t).ok()?))).collect();
        raw.insert(id.clone(), v);
        if let Err(e) = build(&raw) {
            log::warn!("family {id} update rejected: {e:#}");
            continue;
        }
        crate::settings::write_atomic(&dir.join(format!("{id}.json")), &bytes)?;
        updated.push(id.clone());
    }
    Ok(updated)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn embedded() -> BTreeMap<String, Value> {
        EMBEDDED.iter().map(|(k, t)| ((*k).to_owned(), serde_json::from_str(t).unwrap_or_else(|e| panic!("{k}: {e}")))).collect()
    }

    #[test]
    fn built_in_profiles_load_and_inherit() {
        let fams = build(&embedded()).unwrap();
        assert_eq!(fams.len(), EMBEDDED.len());
        let reg = Registry { families: fams, sources: BTreeMap::new() };
        let pony = reg.family("pony").unwrap();
        let sdxl = reg.family("sdxl").unwrap();
        assert_eq!(pony.pipeline.loader, Loader::Checkpoint);
        assert_eq!(pony.sizes.native, sdxl.sizes.native);
        assert!(pony.prompt.prefix.contains("score_9"));
        assert!(pony.lora_fits("pony") && pony.lora_fits("sdxl"));
        assert!(!sdxl.lora_fits("flux1"));
        let kontext = reg.family("flux1-kontext").unwrap();
        assert!(kontext.capabilities.edit);
        assert_eq!(kontext.pipeline.clip.node, "DualCLIPLoader");
        // The original 0.7 models are all present with their keys.
        for key in ["qwen", "z-image-turbo", "flux2-klein-4b", "flux2-klein-9b", "ernie-image", "seedvr2", "klein-remove"] {
            assert!(reg.model(key).is_some(), "{key}");
        }
        assert_eq!(reg.groups().first().map(|(g, _)| g.as_str()), Some("Stable Diffusion"));
    }

    #[test]
    fn model_overrides_apply() {
        let reg = Registry { families: build(&embedded()).unwrap(), sources: BTreeMap::new() };
        let (f, m) = reg.model("klein-remove").unwrap();
        let r = f.resolved_for(m);
        assert_eq!(r.sampling.steps.default, 28.0);
        let (f, m) = reg.model("flux2-klein-4b").unwrap();
        assert!(f.resolved_for(m).sampling.steps.fixed());
    }

    #[test]
    fn bad_profiles_are_rejected() {
        let mut raw = embedded();
        raw.insert("broken".into(), serde_json::json!({"schema": 1, "id": "broken", "label": "B", "pipeline": {"latent": ""}}));
        assert!(build(&raw).is_err());
        let mut raw = embedded();
        raw.insert("future".into(), serde_json::json!({"schema": 99, "id": "future", "label": "F"}));
        assert!(build(&raw).unwrap().iter().all(|f| f.id != "future"));
    }

    #[test]
    fn updates_are_fetched_checked_and_cached() {
        let dir = std::env::temp_dir().join(format!("li-fam-{}", uuid::Uuid::new_v4().simple()));
        let mut newer: Value = serde_json::from_str(EMBEDDED.iter().find(|(k, _)| *k == "sd15").unwrap().1).unwrap();
        newer["version"] = serde_json::json!(999);
        newer["label"] = serde_json::json!("SD 1.5 (updated)");
        let body = serde_json::to_vec(&newer).unwrap();
        let fetch = |url: &str| -> Result<Vec<u8>> {
            if url.ends_with("/index.json") {
                Ok(br#"{"families": {"sd15": 999, "sdxl": 0, "../evil": 5}}"#.to_vec())
            } else if url.ends_with("/sd15.json") {
                Ok(body.clone())
            } else {
                bail!("404")
            }
        };
        let got = update_from(&fetch, "https://example.invalid/f", &dir).unwrap();
        assert_eq!(got, vec!["sd15"]);
        let reg = load(&dir);
        assert_eq!(reg.family("sd15").unwrap().label, "SD 1.5 (updated)");
        assert_eq!(reg.sources["sd15"], "update");
        let _ = std::fs::remove_dir_all(dir);
    }
}
