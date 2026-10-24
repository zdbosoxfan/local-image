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

/// The profile format this build reads. Schema 2 adds `kind: template`, `engine`, `detect`,
/// `template` and per-file `components`; a profile with a newer schema is left to a newer build.
pub const SCHEMA: u32 = 2;

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
    ("realplksr", include_str!("../families/realplksr.json")),
    ("bsrgan", include_str!("../families/bsrgan.json")),
    ("restoration", include_str!("../families/restoration.json")),
];

/// Where newer profiles are published (this repository's `crates/li-ai/families/`).
pub const REMOTE_BASE: &str = "https://raw.githubusercontent.com/zdbosoxfan/local-image/main/crates/li-ai/families";

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
    /// An image family that runs only through official ComfyUI templates; its profile carries
    /// detection rules and template polish, not a pipeline.
    Template,
}

/// Which graph source runs a model.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Engine {
    /// The family's builder when it has one, else the official template.
    #[default]
    Auto,
    Builder,
    Template,
}

/// Whether outputs may be used commercially.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Commercial {
    Yes,
    Limited,
    No,
    #[default]
    Unknown,
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
    /// The original single-image Qwen Image Edit encoder.
    QwenEdit,
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
    /// ComfyUI core ImageUpscaleWithModel (no diffusion sampler).
    UpscaleModel,
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
/// name first, then by precision-independent stem (`stems`, [`crate::stem::stem_key`]), then by
/// any of `patterns` ([`crate::inventory::pattern_matches`]).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Component {
    pub role: Role,
    pub names: Vec<String>,
    pub stems: Vec<String>,
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

/// Known weight variants within a family (e.g. HiDream Dev vs Full, Klein base vs 9B), matched by
/// installed file name (`patterns`, the component rule's `*`-separated parts) or by the variant
/// detection gave (`variants`). Every matching rule applies, in file order, each over the last.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct FileOverride {
    pub patterns: Vec<String>,
    pub variants: Vec<String>,
    pub sampling: Option<Sampling>,
    pub pipeline: Option<Value>,
    pub capabilities: Option<Capabilities>,
    /// Replace the family's components of the same role (a 9B file's larger text encoder).
    pub components: Option<Vec<Component>>,
    pub engine: Option<Engine>,
}

/// A tensor shape condition on logical shapes ([`crate::identify::logical_shape`]): `axis` is
/// `"out"` (dimension 0), `"in"` (dimension 1) or an index.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ShapeRule {
    pub suffix: String,
    pub axis: Value,
    pub one_of: Vec<u64>,
}

/// A header metadata condition; `pointer` reads into a JSON-valued key.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MetaRule {
    pub key: String,
    pub equals: Option<String>,
    pub prefix: Option<String>,
    pub contains: Option<String>,
    pub pointer: Option<String>,
}

/// A variant within a detected architecture; the first matching rule wins.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct VariantRule {
    pub id: String,
    pub tensors_all: Vec<String>,
    pub tensors_any: Vec<String>,
    pub tensors_none: Vec<String>,
    pub shapes: Vec<ShapeRule>,
    pub names: Vec<String>,
}

/// How a LoRA for this family is recognised.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LoraDetect {
    /// Substrings of the LoRA's module names.
    pub tensors_any: Vec<String>,
    /// Prefixes of `ss_base_model_version` or `base_model` (separators ignored).
    pub kohya_prefixes: Vec<String>,
    pub modelspec_prefixes: Vec<String>,
}

/// Detection rules ([`crate::detect`]).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Detect {
    /// This block may only re-label a built-in result equal to this family id.
    pub refines: Option<String>,
    pub priority: i32,
    /// Tensor name suffixes that must all exist.
    pub tensors_all: Vec<String>,
    /// Name parts of which at least one must occur.
    pub tensors_any: Vec<String>,
    /// Tensor name suffixes that must not exist.
    pub tensors_none: Vec<String>,
    pub shapes: Vec<ShapeRule>,
    pub metadata: Vec<MetaRule>,
    /// GGUF `general.architecture` values; narrows, never decides alone.
    pub gguf_arch: Vec<String>,
    pub modelspec_prefixes: Vec<String>,
    pub kohya_prefixes: Vec<String>,
    /// Name patterns for files whose header can't be read (`*` parts, `!token` negatives).
    pub names: Vec<String>,
    pub variants: Vec<VariantRule>,
    pub lora: Option<LoraDetect>,
}

/// Per-variant corrections of what the templates say.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct VariantPolish {
    pub sampling: Option<Sampling>,
    pub sizes: Option<Sizes>,
    /// Template values by role path (`"edit.cfg": 3.5`).
    pub fix: BTreeMap<String, Value>,
}

/// Polish for models that run through official templates: labels and licence come from the
/// family; the graph, files and sampling come from the template.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TemplatePolish {
    /// Variant → official stem keys (for renamed files and community quantisations).
    pub stems: BTreeMap<String, Vec<String>>,
    /// Capability → template names, preferred first.
    pub templates: BTreeMap<String, Vec<String>>,
    pub references_max: Option<u32>,
    /// `show`, `hide` or `default_text`.
    pub negative: Option<String>,
    /// `noise_mask` or `instruction`.
    pub inpaint: Option<String>,
    pub variants: BTreeMap<String, VariantPolish>,
    /// Role → `Class.input` or node title, when detection picks the wrong slot.
    pub roles: BTreeMap<String, String>,
    /// Capability → a pre-converted API graph, when the converter cannot read a template.
    pub graphs: BTreeMap<String, Value>,
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
    pub file_overrides: Vec<FileOverride>,
    pub engine: Engine,
    pub detect: Option<Detect>,
    pub template: Option<TemplatePolish>,
    pub license_summary: Option<String>,
    pub commercial: Commercial,
}

impl Family {
    /// Defaults for an installed main-weights file (with the variant detection gave), before any
    /// curated model overrides. Every matching override applies in file order.
    pub fn resolved_for_file(&self, name: &str, variant: Option<&str>) -> Family {
        let name = name.replace('\\', "/");
        let name = name.rsplit('/').next().unwrap_or(&name);
        let mut f = self.clone();
        for r in &self.file_overrides {
            let by_name = r.patterns.iter().any(|p| crate::inventory::pattern_matches(p, name));
            let by_variant = variant.is_some_and(|v| r.variants.iter().any(|x| x == v));
            if !(by_name || by_variant) {
                continue;
            }
            f = f.resolved_for(&ModelDef { sampling: r.sampling, pipeline: r.pipeline.clone(), capabilities: r.capabilities, ..Default::default() });
            for c in r.components.iter().flatten() {
                f.components.retain(|x| x.role != c.role);
                f.components.push(c.clone());
            }
            if let Some(e) = r.engine {
                f.engine = e;
            }
        }
        f
    }

    /// Whether the graph builders can run this family (an image pipeline, not template-only).
    pub fn buildable(&self) -> bool {
        self.kind == FamilyKind::Image && !self.pipeline.latent.is_empty() && self.engine != Engine::Template
    }

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
    /// Profiles that could not be used, with why (the built-in copy is used where there is one).
    pub errors: BTreeMap<String, String>,
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

fn schema_of(v: &Value) -> u32 {
    v.get("schema").and_then(Value::as_u64).unwrap_or(0) as u32
}

fn version_of(v: &Value) -> u64 {
    v.get("version").and_then(Value::as_u64).unwrap_or(0)
}

fn resolve_raw(id: &str, raw: &BTreeMap<String, Value>, depth: u32) -> Result<Value> {
    if depth > 4 {
        bail!("family {id}: base chain too deep");
    }
    let v = raw.get(id).with_context(|| format!("unknown family {id}"))?;
    let Some(base) = v.get("base").and_then(Value::as_str) else { return Ok(v.clone()) };
    let mut merged = resolve_raw(base, raw, depth + 1)?;
    // A derived family has its own models and identity; everything else is inherited.
    if let Some(o) = merged.as_object_mut() {
        for k in ["models", "version", "description", "civitai_base_models", "hf_tags", "detect", "template", "schema"] {
            o.remove(k);
        }
    }
    merge(&mut merged, v);
    Ok(merged)
}

fn build_one(id: &str, raw: &BTreeMap<String, Value>) -> Result<Family> {
    let merged = resolve_raw(id, raw, 0)?;
    let mut f: Family = serde_json::from_value(merged).with_context(|| format!("family {id}"))?;
    f.id = id.to_owned();
    validate(&f)?;
    Ok(f)
}

fn sort_families(out: &mut [Family]) {
    // Built-in order (the menus' order), then anything else by id.
    let order = |id: &str| EMBEDDED.iter().position(|(k, _)| *k == id).unwrap_or(usize::MAX);
    out.sort_by(|a, b| order(&a.id).cmp(&order(&b.id)).then(a.id.cmp(&b.id)));
}

/// Parses raw profiles (`id → JSON`), resolving `base` inheritance. One bad profile drops only
/// itself and the families based on it; the errors are returned by id. Profiles with a newer
/// schema than [`SCHEMA`] are left out with a "needs a newer Local Image" error.
pub fn build_tolerant(raw: &BTreeMap<String, Value>) -> (Vec<Family>, BTreeMap<String, String>) {
    let mut out = Vec::new();
    let mut errors = BTreeMap::new();
    for (id, v) in raw {
        let schema = schema_of(v);
        if schema > SCHEMA {
            errors.insert(id.clone(), format!("profile format {schema} needs a newer Local Image"));
            continue;
        }
        match build_one(id, raw) {
            Ok(f) => out.push(f),
            Err(e) => {
                errors.insert(id.clone(), format!("{e:#}"));
            }
        }
    }
    sort_families(&mut out);
    (out, errors)
}

/// [`build_tolerant`] for validation: any rejected profile fails the whole set (newer-schema
/// profiles are skipped, as a reader of this schema must).
pub fn build(raw: &BTreeMap<String, Value>) -> Result<Vec<Family>> {
    let (families, errors) = build_tolerant(raw);
    if let Some((id, e)) = errors.iter().find(|(id, _)| raw.get(*id).is_none_or(|v| schema_of(v) <= SCHEMA)) {
        bail!("family {id}: {e}");
    }
    Ok(families)
}

const FIX_CAPABILITIES: &[&str] = &["create", "edit", "style", "refine", "inpaint"];
const FIX_ROLES: &[&str] = &["cfg", "steps", "sampler_name", "scheduler", "denoise", "width", "height", "negative"];

/// Rejects profiles a builder could not run, and detection or template data that could
/// mislabel files or break graphs.
pub fn validate(f: &Family) -> Result<()> {
    if f.id == "restoration" && f != crate::restoration::policy() {
        bail!("Restoration profiles must match the immutable built-in policy");
    }
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
    if let Some(d) = &f.detect {
        if !crate::detect::has_header_conditions(d) && d.refines.is_none() {
            bail!("family {}: a detect block needs a tensor, shape or metadata condition, or `refines`", f.id);
        }
        if let Some(r) = &d.refines
            && !EMBEDDED.iter().any(|(k, _)| k == r)
        {
            bail!("family {}: `refines` must name a built-in family ({r} is not one)", f.id);
        }
        let patterns = d.names.iter().chain(d.variants.iter().flat_map(|v| &v.names));
        if let Some(p) = patterns.into_iter().find(|p| p.trim_start_matches('!').len() < 4) {
            bail!("family {}: name pattern \"{p}\" is shorter than 4 characters", f.id);
        }
    }
    if let Some(t) = &f.template {
        for key in t.variants.values().flat_map(|v| v.fix.keys()) {
            let ok = key.split_once('.').is_some_and(|(c, r)| FIX_CAPABILITIES.contains(&c) && FIX_ROLES.contains(&r));
            if !ok {
                bail!("family {}: template fix key \"{key}\" is not <capability>.<role>", f.id);
            }
        }
        for (cap, g) in &t.graphs {
            let nodes = g.as_object().with_context(|| format!("family {}: template graph {cap} is not an object", f.id))?;
            if nodes.is_empty()
                || nodes.values().any(|n| n.get("class_type").and_then(Value::as_str).is_none() || !n.get("inputs").is_some_and(Value::is_object))
            {
                bail!("family {}: template graph {cap} is not an API-format graph", f.id);
            }
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
                let core_upscaler = f.kind == FamilyKind::Upscaler && f.pipeline.sampler == SamplerStyle::UpscaleModel && file.role == Role::Upscaler;
                if !(file.name.ends_with(".safetensors")
                    || file.name.ends_with(".gguf")
                    || (core_upscaler && file.name.ends_with(".pth"))
                    || f.id == "restoration")
                {
                    bail!("{}: {} is not a supported weights file", m.key, file.name);
                }
            }
        }
    }
    Ok(())
}

/// Tensor-name parts the built-in rules key on; a downloaded detect block that claims them
/// without `refines` would take files away from built-in families.
const BUILTIN_MARKERS: &[&str] = &[
    "input_blocks",
    "label_emb",
    "joint_blocks",
    "double_stream_blocks",
    "double_stream_modulation",
    "single_stream_modulation.",
    "txt_in.text_norm",
    "double_blocks",
    "img_mod",
    "cap_embedder",
    "noise_refiner",
    "linear_fc2",
    "distilled_guidance_layer",
];

fn claims_builtin(d: &Detect) -> Option<&str> {
    d.tensors_all.iter().chain(&d.tensors_any).find(|t| BUILTIN_MARKERS.iter().any(|m| t.contains(m))).map(String::as_str)
}

/// Where downloaded profile updates are cached.
pub fn cache_dir() -> PathBuf {
    crate::settings::data_root().join("families")
}

fn embedded_raw() -> BTreeMap<String, Value> {
    EMBEDDED.iter().filter_map(|(k, t)| Some(((*k).to_owned(), serde_json::from_str(t).ok()?))).collect()
}

fn read_profiles(d: &Path) -> Vec<(String, Value)> {
    let Ok(rd) = std::fs::read_dir(d) else { return Vec::new() };
    let mut out = Vec::new();
    for e in rd.flatten() {
        let p = e.path();
        if p.extension().and_then(|x| x.to_str()) != Some("json") || p.file_name().is_some_and(|n| n == "index.json" || n == "runtime.json") {
            continue;
        }
        let Some(id) = p.file_stem().and_then(|s| s.to_str()).map(str::to_owned) else { continue };
        // Restoration pins and licensing policy are vetted in the binary, never widened by an
        // editable profile or an online update.
        if id == "restoration" {
            continue;
        }
        match std::fs::read(&p).map_err(anyhow::Error::from).and_then(|b| Ok(serde_json::from_slice::<Value>(&b)?)) {
            Ok(v) => out.push((id, v)),
            Err(_) => log::warn!("family profile {} is unreadable; skipped", p.display()),
        }
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

/// The raw profiles in use: built-in, newer cached updates (`dir`), then the user's own
/// (`dir/user`). Profiles this build can't read (newer schema) never replace a readable one.
fn load_raw(dir: &Path) -> (BTreeMap<String, Value>, BTreeMap<String, String>, BTreeMap<String, String>) {
    let mut raw = embedded_raw();
    let mut sources: BTreeMap<String, String> = raw.keys().map(|k| (k.clone(), "built-in".to_owned())).collect();
    let mut errors = BTreeMap::new();
    for (sub, label) in [(dir.to_path_buf(), "update"), (dir.join("user"), "user")] {
        for (id, v) in read_profiles(&sub) {
            if schema_of(&v) > SCHEMA {
                errors.insert(id, format!("profile format {} needs a newer Local Image", schema_of(&v)));
                continue;
            }
            let newer = raw.get(&id).is_none_or(|old| version_of(&v) > version_of(old) || label == "user");
            if newer {
                raw.insert(id.clone(), v);
                sources.insert(id, label.to_owned());
            }
        }
    }
    (raw, sources, errors)
}

/// The built-in profiles, overridden by newer cached updates, plus the user's own (`*.json` in
/// `<data>/families/user/`). A profile that fails to build is replaced by its built-in copy (or
/// dropped with the families based on it); the others stay in use.
pub fn load(dir: &Path) -> Registry {
    let (mut raw, mut sources, mut errors) = load_raw(dir);
    let (_, first) = build_tolerant(&raw);
    let embedded = embedded_raw();
    for (id, e) in &first {
        log::warn!("family profile {id} ({}): {e}", sources.get(id).map_or("?", String::as_str));
        errors.insert(id.clone(), e.clone());
        match embedded.get(id) {
            Some(builtin) if raw.get(id) != Some(builtin) => {
                raw.insert(id.clone(), builtin.clone());
                sources.insert(id.clone(), "built-in".to_owned());
            }
            _ => {}
        }
    }
    let (families, _) = build_tolerant(&raw);
    sources.retain(|id, _| families.iter().any(|f| &f.id == id));
    Registry { families, sources, errors }
}

static REGISTRY: RwLock<Option<&'static Registry>> = RwLock::new(None);
static GENERATION: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Changes whenever the profiles are reloaded (detection caches are keyed by it).
pub fn generation() -> u64 {
    GENERATION.load(std::sync::atomic::Ordering::SeqCst)
}

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
    GENERATION.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    crate::catalog::invalidate();
    r
}

/// What a profile update did.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct UpdateReport {
    /// Profiles downloaded and in use after a reload.
    pub updated: Vec<String>,
    /// Profiles that need a newer Local Image (not written).
    pub needs_newer_app: Vec<String>,
    /// Profiles rejected, with why (not written; the previous copy stays in use).
    pub rejected: Vec<(String, String)>,
    /// Profiles downloaded while the user's own copy of the same id stays in use.
    pub kept_user: Vec<String>,
}

impl UpdateReport {
    pub fn is_empty(&self) -> bool {
        self.updated.is_empty() && self.needs_newer_app.is_empty() && self.rejected.is_empty() && self.kept_user.is_empty()
    }
}

fn index_number(v: &Value) -> u64 {
    v.as_u64().or_else(|| v.get("version").and_then(Value::as_u64)).unwrap_or(0)
}

/// Fetches `index.json` (`{"families": {"id": version}, "min_schema": {"id": schema}}`) from
/// [`REMOTE_BASE`] and downloads every profile newer than the built-in or cached copy into `dir`.
/// The whole batch is validated together (a new family may be based on another new one);
/// profiles for a newer schema are reported, never written.
pub fn update_from(fetch: &dyn Fn(&str) -> Result<Vec<u8>>, base_url: &str, dir: &Path) -> Result<UpdateReport> {
    let index: Value = serde_json::from_slice(&fetch(&format!("{base_url}/index.json"))?).context("the family index is not JSON")?;
    let entries = index.get("families").and_then(Value::as_object).context("the family index has no families")?;
    let min_schema = index.get("min_schema").and_then(Value::as_object);
    std::fs::create_dir_all(dir)?;
    let mut report = UpdateReport::default();
    let mut known = embedded_raw();
    for (id, v) in read_profiles(dir) {
        if known.get(&id).is_none_or(|old| version_of(&v) > version_of(old)) {
            known.insert(id, v);
        }
    }
    let user: Vec<String> = read_profiles(&dir.join("user")).into_iter().map(|(id, _)| id).collect();
    let mut batch: Vec<(String, Vec<u8>, Value)> = Vec::new();
    for (id, ver) in entries {
        if id.is_empty() || !id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') || id == "restoration" {
            continue;
        }
        if known.get(id).is_some_and(|old| version_of(old) >= index_number(ver)) {
            continue;
        }
        if min_schema.and_then(|m| m.get(id)).and_then(Value::as_u64).is_some_and(|s| s > u64::from(SCHEMA)) {
            report.needs_newer_app.push(id.clone());
            continue;
        }
        let bytes = match fetch(&format!("{base_url}/{id}.json")) {
            Ok(b) => b,
            Err(e) => {
                report.rejected.push((id.clone(), format!("{e:#}")));
                continue;
            }
        };
        match serde_json::from_slice::<Value>(&bytes) {
            Ok(v) if schema_of(&v) > SCHEMA => report.needs_newer_app.push(id.clone()),
            Ok(v) => batch.push((id.clone(), bytes, v)),
            Err(e) => report.rejected.push((id.clone(), format!("not JSON: {e}"))),
        }
    }
    let mut raw = known;
    for (id, _, v) in &batch {
        raw.insert(id.clone(), v.clone());
    }
    let (_, errors) = build_tolerant(&raw);
    for (id, bytes, v) in batch {
        if let Some(e) = errors.get(&id) {
            report.rejected.push((id, e.clone()));
            continue;
        }
        if let Some(d) = v.get("detect").and_then(|d| serde_json::from_value::<Detect>(d.clone()).ok())
            && d.refines.is_none()
            && let Some(t) = claims_builtin(&d)
        {
            report.rejected.push((id, format!("its detect rule claims built-in tensors ({t}) without `refines`")));
            continue;
        }
        li_fs::atomic_write(&dir.join(format!("{id}.json")), &bytes)?;
        if user.contains(&id) {
            report.kept_user.push(id);
        } else {
            report.updated.push(id);
        }
    }
    Ok(report)
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
        let reg = Registry { families: fams, ..Default::default() };
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
        let reg = Registry { families: build(&embedded()).unwrap(), ..Default::default() };
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
        assert_eq!(got.updated, vec!["sd15"]);
        let reg = load(&dir);
        assert_eq!(reg.family("sd15").unwrap().label, "SD 1.5 (updated)");
        assert_eq!(reg.sources["sd15"], "update");
        let _ = std::fs::remove_dir_all(dir);
    }
}
