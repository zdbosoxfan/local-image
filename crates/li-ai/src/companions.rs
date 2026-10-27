//! Companion files (text encoders, VAEs, LoRAs, model patches) for template models, and the
//! runtime data that rules them (`families/runtime.json`, on the profile-update channel):
//! interchangeable files, byte-identical public mirrors, which precisions need which GPU, custom
//! node packs, and how ComfyUI's error messages translate.
//!
//! A template names one file per loader; the user may have another precision of it, an
//! equivalent file, or none. [`resolve`] finds the installed file a loader should use: the exact
//! file, the same model at another precision (equal [`crate::stem`] keys), a file `runtime.json`
//! declares equivalent, or the user's own choice. There is never a substring match. With a local
//! header, a VAE whose latent channels differ from the declared one's is never substituted.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::RwLock;

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

use crate::comfy::{ObjectInfo, normalized_name};
use crate::stem::stem_key;

pub const EMBEDDED_RUNTIME: &str = include_str!("../families/runtime.json");

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct StemGroup {
    pub folder: String,
    pub stems: Vec<String>,
    pub verified: String,
    pub note: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Mirror {
    pub name: String,
    pub bytes: u64,
    pub sha256: String,
    pub url: String,
    pub repo: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct QuantRule {
    pub class: String,
    pub needs: Option<String>,
    pub needs_nodes: Vec<String>,
    pub pack: Option<String>,
    pub label: String,
    pub block: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct GpuRule {
    pub generation: String,
    pub name_contains: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Pack {
    pub id: String,
    pub name: String,
    pub manager: String,
    pub repo: String,
    pub classes: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ErrorPattern {
    pub contains: Vec<String>,
    pub classes: Vec<String>,
    pub file_input: Option<bool>,
    pub code: String,
}

/// `families/runtime.json`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Runtime {
    pub schema: u32,
    pub version: u32,
    pub upsampler: String,
    pub equivalent: Vec<StemGroup>,
    pub never_substitute: Vec<StemGroup>,
    pub vae_latent_channels: BTreeMap<String, u32>,
    pub mirrors: Vec<Mirror>,
    pub quant: Vec<QuantRule>,
    pub gpus: Vec<GpuRule>,
    pub packs: Vec<Pack>,
    pub zero_classes: Vec<String>,
    pub error_patterns: Vec<ErrorPattern>,
}

/// The error codes `error_patterns` may name.
pub const ERROR_CODES: &[&str] =
    &["ComfyCantReadModel", "OutOfMemory", "ComponentMismatch", "QuantUnsupported", "FileDamaged", "ComfyCantSeeFile", "ValueNotOffered", "TemplateUnusable"];

impl Runtime {
    /// Latent channels of a VAE by stem (`flux2-vae` → 32).
    pub fn vae_channels(&self, file: &str) -> Option<u32> {
        let key = stem_key(file).key;
        self.vae_latent_channels.iter().find(|(s, _)| stem_key(s).key == key).map(|(_, c)| *c)
    }

    /// The GPU generation of a device name (`NVIDIA GeForce RTX 5090` → `blackwell`).
    pub fn gpu_generation(&self, name: &str) -> Option<&str> {
        self.gpus.iter().find(|g| g.name_contains.iter().any(|n| name.contains(n.as_str()))).map(|g| g.generation.as_str())
    }

    pub fn quant_rule(&self, class: &crate::identify::QuantClass) -> Option<&QuantRule> {
        let id = class.group();
        self.quant.iter().find(|q| q.class == id)
    }

    /// Whether `gpu` can run a precision at all (`block` rules only).
    pub fn quant_runs_on(&self, class: &crate::identify::QuantClass, gpu: Option<&str>) -> bool {
        match self.quant_rule(class) {
            Some(QuantRule { needs: Some(need), block: true, .. }) => gpu.and_then(|g| self.gpu_generation(g)) == Some(need.as_str()),
            _ => true,
        }
    }

    fn group_has(groups: &[StemGroup], folder: &str, a: &str, b: &str) -> bool {
        let (a, b) = (stem_key(a).key, stem_key(b).key);
        groups.iter().any(|g| g.folder == folder && g.stems.iter().any(|s| stem_key(s).key == a) && g.stems.iter().any(|s| stem_key(s).key == b))
    }

    /// Whether two files are interchangeable by `equivalent` (and not forbidden).
    pub fn equivalent(&self, folder: &str, a: &str, b: &str) -> bool {
        Self::group_has(&self.equivalent, folder, a, b) && !Self::group_has(&self.never_substitute, folder, a, b)
    }

    pub fn mirror(&self, name: &str) -> Option<&Mirror> {
        self.mirrors.iter().find(|m| normalized_name(&m.name) == normalized_name(name))
    }

    pub fn pack_for(&self, class: &str) -> Option<&Pack> {
        self.packs.iter().find(|p| p.classes.iter().any(|c| c == class))
    }
}

/// Rejects runtime data that would substitute wrong files, download unverified ones or name
/// unknown error codes.
pub fn validate_runtime(r: &Runtime) -> Result<()> {
    for g in &r.equivalent {
        for (i, a) in g.stems.iter().enumerate() {
            for b in &g.stems[i + 1..] {
                if Runtime::group_has(&r.never_substitute, &g.folder, a, b) {
                    bail!("equivalent group joins {a} and {b}, which must never substitute");
                }
                if g.folder == "vae"
                    && let (Some(x), Some(y)) = (r.vae_channels(a), r.vae_channels(b))
                    && x != y
                {
                    bail!("equivalent VAEs {a} ({x} channels) and {b} ({y} channels) differ");
                }
            }
        }
    }
    for m in &r.mirrors {
        if m.sha256.len() != 64 || !m.sha256.bytes().all(|b| b.is_ascii_hexdigit()) || m.bytes == 0 || !crate::download::host_allowed(&m.url) {
            bail!("mirror {} needs a SHA-256, a size and an allowed host", m.name);
        }
    }
    if let Some(p) = r.error_patterns.iter().find(|p| !ERROR_CODES.contains(&p.code.as_str())) {
        bail!("unknown error code {}", p.code);
    }
    Ok(())
}

static RUNTIME: RwLock<Option<&'static Runtime>> = RwLock::new(None);

fn embedded() -> Runtime {
    serde_json::from_str(EMBEDDED_RUNTIME).expect("the built-in runtime.json parses")
}

/// The runtime data in use: a newer valid cached update (`<data>/families/runtime.json`), else
/// the built-in copy.
pub fn load(dir: &Path) -> Runtime {
    let builtin = embedded();
    let cached = std::fs::read(dir.join("runtime.json")).ok().and_then(|b| serde_json::from_slice::<Runtime>(&b).ok());
    match cached {
        Some(r) if r.schema <= 1 && r.version > builtin.version && validate_runtime(&r).is_ok() => r,
        Some(r) if r.version > builtin.version => {
            log::warn!("runtime.json update (version {}) rejected; using the built-in copy", r.version);
            builtin
        }
        _ => builtin,
    }
}

pub fn runtime() -> &'static Runtime {
    if let Some(r) = RUNTIME.read().ok().and_then(|g| *g) {
        return r;
    }
    let r: &'static Runtime = Box::leak(Box::new(load(&crate::family::cache_dir())));
    if let Ok(mut g) = RUNTIME.write() {
        if let Some(existing) = *g {
            return existing;
        }
        *g = Some(r);
    }
    r
}

/// Reloads after a profile update.
pub fn reload() {
    let r: &'static Runtime = Box::leak(Box::new(load(&crate::family::cache_dir())));
    if let Ok(mut g) = RUNTIME.write() {
        *g = Some(r);
    }
}

/// Parses and checks a downloaded runtime.json.
pub fn parse_update(bytes: &[u8]) -> Result<Runtime> {
    let r: Runtime = serde_json::from_slice(bytes).context("runtime.json is not valid")?;
    if r.schema > 1 {
        bail!("runtime.json format {} needs a newer Local Image", r.schema);
    }
    validate_runtime(&r)?;
    Ok(r)
}

/// How a companion was found.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Why {
    Exact,
    SameStem,
    Equivalent,
    User,
    WrongFolder,
    #[default]
    Missing,
}

/// The installed file a template loader input uses.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Binding {
    /// `clip`, `vae`, `lora`, `upsampler`, `model_patch` or another loader's role.
    pub role: String,
    pub node: String,
    pub input: String,
    pub loader: String,
    pub declared: String,
    pub used: Option<String>,
    pub folder: String,
    pub why: Why,
    /// Not needed for the model to work (the optional prompt enhancer).
    pub optional: bool,
    /// The template's download URL for the declared file.
    pub url: String,
    /// When `why` is `wrong_folder`: the loader that lists it instead.
    pub found_in: Option<String>,
}

/// What resolution needs besides the loader's choices.
pub struct Ctx<'a> {
    pub info: &'a ObjectInfo,
    pub runtime: &'a Runtime,
    /// The device name (for quantisation limits).
    pub gpu: Option<&'a str>,
    /// The main model's precision.
    pub main_quant: Option<crate::identify::QuantClass>,
    /// `declared → installed` choices the user made.
    pub user: &'a BTreeMap<String, String>,
    /// Latent channels of an installed VAE (from its local header), when readable.
    pub vae_channels: &'a dyn Fn(&str) -> Option<u32>,
}

/// The folder a loader input lists.
pub fn folder_of(loader: &str, input: &str) -> &'static str {
    match (loader, input) {
        ("VAELoader", _) => "vae",
        ("LoraLoader" | "LoraLoaderModelOnly", _) => "loras",
        ("CheckpointLoaderSimple", _) => "checkpoints",
        ("UNETLoader" | "UnetLoaderGGUF", _) => "diffusion_models",
        ("ModelPatchLoader", _) => "model_patches",
        ("CLIPVisionLoader", _) => "clip_vision",
        (_, i) if i.starts_with("clip_name") => "text_encoders",
        _ => "",
    }
}

fn candidates(info: &ObjectInfo, loader: &str, input: &str) -> Vec<String> {
    let mut v = info.choices(loader, input);
    if input.starts_with("clip_name") && info.has_node("CLIPLoaderGGUF") {
        v.extend(info.choices("CLIPLoaderGGUF", "clip_name"));
    }
    v.sort();
    v.dedup();
    v
}

/// Other loaders that list files, for the wrong-folder check.
const LOADERS: &[(&str, &str)] = &[
    ("UNETLoader", "unet_name"),
    ("CheckpointLoaderSimple", "ckpt_name"),
    ("CLIPLoader", "clip_name"),
    ("VAELoader", "vae_name"),
    ("LoraLoaderModelOnly", "lora_name"),
];

/// Resolves the file `loader.input` should load for a template that declares `declared`.
pub fn resolve(ctx: &Ctx, loader: &str, input: &str, declared: &str) -> Binding {
    let folder = folder_of(loader, input).to_owned();
    let mut b = Binding { loader: loader.into(), input: input.into(), declared: declared.into(), folder: folder.clone(), ..Default::default() };
    let choices = candidates(ctx.info, loader, input);
    if let Some(found) = choices.iter().find(|c| normalized_name(c) == normalized_name(declared)) {
        b.used = Some(found.clone());
        b.why = Why::Exact;
        return b;
    }
    let declared_key = stem_key(declared);
    let declared_quant = crate::stem::name_quant(&declared_key.quant_tokens);
    let vae_ok = |c: &str| folder != "vae" || ctx.runtime.vae_channels(declared).is_none_or(|want| (ctx.vae_channels)(c).is_none_or(|have| have == want));
    let runs = |c: &str| {
        let q = crate::stem::name_quant(&stem_key(c).quant_tokens);
        let gguf_ok = !c.to_ascii_lowercase().ends_with(".gguf") || ctx.info.has_node("CLIPLoaderGGUF") || folder != "text_encoders";
        gguf_ok && q.as_ref().is_none_or(|q| ctx.runtime.quant_runs_on(q, ctx.gpu))
    };
    let mut same: Vec<&String> = choices.iter().filter(|c| stem_key(c).key == declared_key.key && runs(c) && vae_ok(c)).collect();
    // Prefer the declared precision, then the main model's, then the shortest name.
    same.sort_by_key(|c| {
        let q = crate::stem::name_quant(&stem_key(c).quant_tokens);
        (q != declared_quant, q != ctx.main_quant, c.len())
    });
    if let Some(found) = same.first() {
        b.used = Some((*found).clone());
        b.why = Why::SameStem;
        return b;
    }
    if let Some(found) = choices.iter().filter(|c| ctx.runtime.equivalent(&folder, declared, c) && runs(c) && vae_ok(c)).min_by_key(|c| c.len()) {
        b.used = Some(found.clone());
        b.why = Why::Equivalent;
        return b;
    }
    if let Some(found) = ctx.user.get(declared).filter(|u| choices.contains(u)) {
        b.used = Some(found.clone());
        b.why = Why::User;
        return b;
    }
    for (other, other_input) in LOADERS {
        if *other == loader {
            continue;
        }
        if let Some(found) =
            ctx.info.choices(other, other_input).into_iter().find(|c| normalized_name(c) == normalized_name(declared) || stem_key(c).key == declared_key.key)
        {
            b.used = Some(found);
            b.why = Why::WrongFolder;
            b.found_in = Some(folder_of(other, other_input).to_owned());
            return b;
        }
    }
    b
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn info(clip: &[&str], vae: &[&str], unet: &[&str]) -> ObjectInfo {
        ObjectInfo(json!({
            "CLIPLoader": {"input": {"required": {"clip_name": [clip], "type": [["flux3"]]}}},
            "VAELoader": {"input": {"required": {"vae_name": [vae]}}},
            "UNETLoader": {"input": {"required": {"unet_name": [unet]}}}
        }))
    }

    fn ctx<'a>(info: &'a ObjectInfo, rt: &'a Runtime, user: &'a BTreeMap<String, String>, gpu: Option<&'a str>) -> Ctx<'a> {
        Ctx { info, runtime: rt, gpu, main_quant: Some(crate::identify::QuantClass::Bf16), user, vae_channels: &|_| None }
    }

    #[test]
    fn companions_resolve_exact_then_stem_then_equivalent_then_user() {
        let rt = embedded();
        let none = BTreeMap::new();
        let i = info(
            &["qwen3vl_4b_bf16.safetensors", "qwen3.5_9b_bf16.safetensors", "qwen3.5_9b_flux3_prompt_upsampler_w4a8.safetensors"],
            &["flux2-vae.safetensors"],
            &[],
        );
        let c = ctx(&i, &rt, &none, Some("NVIDIA GeForce RTX 5090"));
        let b = resolve(&c, "CLIPLoader", "clip_name", "qwen3vl_4b_int8_convrot.safetensors");
        assert_eq!((b.used.as_deref(), b.why), (Some("qwen3vl_4b_bf16.safetensors"), Why::SameStem));
        let b = resolve(&c, "CLIPLoader", "clip_name", "qwen3.5_9b_w4a8_gptq.safetensors");
        assert_eq!(b.used.as_deref(), Some("qwen3.5_9b_bf16.safetensors"), "never the upsampler, which only shares a prefix");
        let b = resolve(&c, "VAELoader", "vae_name", "flux2-vae.safetensors");
        assert_eq!(b.why, Why::Exact);
        let b = resolve(&c, "VAELoader", "vae_name", "flux2-vae_small.safetensors");
        assert_eq!((b.used.as_deref(), b.why), (Some("flux2-vae.safetensors"), Why::Equivalent));
        let b = resolve(&c, "VAELoader", "vae_name", "qwen_image_vae.safetensors");
        assert_eq!((b.used, b.why), (None, Why::Missing));
        let user: BTreeMap<String, String> = [("qwen_image_vae.safetensors".to_owned(), "flux2-vae.safetensors".to_owned())].into();
        let b = resolve(&ctx(&i, &rt, &user, None), "VAELoader", "vae_name", "qwen_image_vae.safetensors");
        assert_eq!(b.why, Why::User);
    }

    #[test]
    fn guards_keep_wrong_files_out() {
        let rt = embedded();
        let none = BTreeMap::new();
        // A 16-channel VAE never stands in for a 32-channel one, even by stem.
        let i = info(&[], &["flux2-vae_bf16.safetensors"], &[]);
        let c = Ctx { vae_channels: &|_| Some(16), ..ctx(&i, &rt, &none, None) };
        assert_eq!(resolve(&c, "VAELoader", "vae_name", "flux2-vae.safetensors").why, Why::Missing);
        // NVFP4 text encoders don't run off Blackwell.
        let i = info(&["qwen3vl_4b_nvfp4.safetensors"], &[], &[]);
        assert_eq!(resolve(&ctx(&i, &rt, &none, Some("NVIDIA GeForce RTX 4090")), "CLIPLoader", "clip_name", "qwen3vl_4b_bf16.safetensors").why, Why::Missing);
        assert_eq!(resolve(&ctx(&i, &rt, &none, Some("NVIDIA GeForce RTX 5090")), "CLIPLoader", "clip_name", "qwen3vl_4b_bf16.safetensors").why, Why::SameStem);
        // A text encoder saved under diffusion_models is reported, not used.
        let i = info(&[], &[], &["qwen3vl_4b_bf16.safetensors"]);
        let b = resolve(&ctx(&i, &rt, &none, None), "CLIPLoader", "clip_name", "qwen3vl_4b_bf16.safetensors");
        assert_eq!((b.why, b.found_in.as_deref()), (Why::WrongFolder, Some("diffusion_models")));
    }

    #[test]
    fn runtime_data_is_validated() {
        let rt = embedded();
        validate_runtime(&rt).unwrap();
        assert_eq!(rt.gpu_generation("cuda:0 NVIDIA GeForce RTX 5090 : cudaMallocAsync"), Some("blackwell"));
        let ada = "cuda:0 NVIDIA RTX 5000 Ada Generation : cudaMallocAsync";
        assert_eq!(rt.gpu_generation(ada), Some("ada"));
        assert_eq!(rt.gpu_generation("NVIDIA GeForce RTX 5090 Laptop GPU"), Some("blackwell"));
        assert!(!rt.quant_runs_on(&crate::identify::QuantClass::Nvfp4, Some(ada)));
        assert_ne!(rt.gpu_generation("Quadro RTX 5000"), Some("blackwell"));
        assert_eq!(rt.vae_channels("flux2-vae_small.safetensors"), Some(32));
        let mut bad = rt.clone();
        bad.equivalent.push(StemGroup { folder: "text_encoders".into(), stems: vec!["qwen3vl_4b".into(), "qwen_3_4b".into()], ..Default::default() });
        assert!(validate_runtime(&bad).is_err());
        let mut bad = rt.clone();
        bad.equivalent.push(StemGroup { folder: "vae".into(), stems: vec!["ae".into(), "flux2-vae".into()], ..Default::default() });
        assert!(validate_runtime(&bad).is_err());
        let mut bad = rt.clone();
        bad.mirrors[0].sha256.clear();
        assert!(validate_runtime(&bad).is_err());
        let mut bad = rt;
        bad.error_patterns[0].code = "Mystery".into();
        assert!(validate_runtime(&bad).is_err());
    }
}
