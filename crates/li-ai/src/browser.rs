//! The model and LoRA browser's data: live catalogues from Hugging Face, Civitai, ComfyUI's
//! official workflow templates and ComfyUI-Manager's community list, turned into one kind of
//! card ([`Item`]) with its family, capabilities, download size, rough GPU memory and licence;
//! plus install planning with Local Image's guardrails:
//!
//! - `.safetensors` and `.gguf`, plus the immutable restoration policy's pinned PTH files;
//! - every file needs a known size and SHA-256 (Hugging Face's LFS hash from the repo tree;
//!   Civitai's published SHA-256, size confirmed with a HEAD request) or it is refused;
//! - downloads come only from the allow-listed hosts ([`crate::download::host_allowed`]);
//! - the licence is shown before anything downloads (the UI's job; [`Plan::license`]);
//! - catalogues are cached ([`Cache`]) and fetched only when the browser asks (never in the
//!   background), so the browser works offline from its last fetch.
//!
//! The formats follow the public APIs (Hugging Face `/api/models` and `/tree`, Civitai
//! `/api/v1/models`, the workflow templates' `index.json` and per-node `properties.models`,
//! ComfyUI-Manager's `model-list.json`). Base URLs come from [`Config`], so tests and demos point
//! them at the mock server. Ideas from SwarmUI's and InvokeAI's model managers (no code).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::comfy::ObjectInfo;
use crate::family::{Family, Loader, Registry, Role};

pub mod hf;

#[cfg(test)]
#[path = "browser_security_tests.rs"]
mod security_tests;

/// Where the catalogues are.
#[derive(Clone, Debug, PartialEq)]
pub struct Config {
    pub hf: String,
    pub civitai: String,
    /// ComfyUI-Manager's `model-list.json`.
    pub manager_list: String,
    /// The official templates (`…/templates`, holding `index.json`, `{name}.json`,
    /// `{name}-1.webp`). The connected ComfyUI serves a version-matched copy at `/templates`.
    pub templates: String,
    pub hf_token: Option<String>,
    pub civitai_token: Option<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            hf: "https://huggingface.co".into(),
            civitai: "https://civitai.com".into(),
            manager_list: "https://raw.githubusercontent.com/ltdrdata/ComfyUI-Manager/main/model-list.json".into(),
            templates: "https://raw.githubusercontent.com/Comfy-Org/workflow_templates/main/templates".into(),
            hf_token: None,
            civitai_token: None,
        }
    }
}

impl Config {
    /// Defaults, overridden by `LOCAL_IMAGE_HF_BASE`, `LOCAL_IMAGE_CIVITAI_BASE`,
    /// `LOCAL_IMAGE_MANAGER_LIST`, `LOCAL_IMAGE_TEMPLATES_BASE` (tests, mirrors) and the tokens
    /// from the AI settings.
    pub fn from_env(settings: &crate::settings::AiSettings) -> Self {
        let mut c = Self::default();
        let var = |k: &str| std::env::var(k).ok().filter(|v| !v.trim().is_empty());
        if let Some(v) = var("LOCAL_IMAGE_HF_BASE") {
            c.hf = v;
        }
        if let Some(v) = var("LOCAL_IMAGE_CIVITAI_BASE") {
            c.civitai = v;
        }
        if let Some(v) = var("LOCAL_IMAGE_MANAGER_LIST") {
            c.manager_list = v;
        }
        if let Some(v) = var("LOCAL_IMAGE_TEMPLATES_BASE") {
            c.templates = v;
        }
        c.hf_token = settings.extra_string("hf_token").or_else(|| var("HF_TOKEN"));
        c.civitai_token = settings.extra_string("civitai_token").or_else(|| var("CIVITAI_TOKEN"));
        c
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Source {
    HuggingFace,
    Civitai,
    Template,
    Community,
    Installed,
}

impl Source {
    pub fn label(self) -> &'static str {
        match self {
            Source::HuggingFace => "Hugging Face",
            Source::Civitai => "Civitai",
            Source::Template => "ComfyUI template",
            Source::Community => "Community list",
            Source::Installed => "Installed",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Kind {
    Model,
    Lora,
}

/// A file a card would install.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct CatalogFile {
    pub name: String,
    /// ComfyUI folder (`checkpoints`, `diffusion_models`, `loras`, `text_encoders`, `vae`…).
    pub folder: String,
    pub url: String,
    pub bytes: Option<u64>,
    /// Size shown before the exact size is known (Civitai's `sizeKB`, the Manager's text).
    pub approx_bytes: Option<u64>,
    pub sha256: Option<String>,
}

impl CatalogFile {
    /// `(repo, path)` of a Hugging Face `resolve` URL.
    pub fn hf_path(&self, hf_base: &str) -> Option<(String, String)> {
        let rest = self.url.strip_prefix(hf_base.trim_end_matches('/'))?.trim_start_matches('/');
        let (repo, tail) = rest.split_once("/resolve/")?;
        let (_rev, path) = tail.split_once('/')?;
        let path = path.split('?').next()?;
        Some((repo.to_owned(), path.to_owned()))
    }
    pub fn size(&self) -> Option<u64> {
        self.bytes.or(self.approx_bytes)
    }
}

/// One card in the browser.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Item {
    pub id: String,
    pub source: Option<Source>,
    pub kind: Option<Kind>,
    pub title: String,
    pub author: String,
    pub description: String,
    /// The family id, when known (else the template's basic controls).
    pub family: Option<String>,
    pub license: String,
    pub license_url: Option<String>,
    pub commercial: Option<bool>,
    pub preview: Option<String>,
    pub likes: u64,
    pub downloads: u64,
    /// ISO date.
    pub created: String,
    pub trending: f64,
    pub tags: Vec<String>,
    pub files: Vec<CatalogFile>,
    pub page_url: String,
    pub gated: bool,
    pub nsfw: bool,
    /// The template this card runs through (official templates; basic controls when the family
    /// is unknown).
    pub template: Option<String>,
    /// The template's `minComfyUIVersion`.
    #[serde(default)]
    pub min_comfyui: Option<String>,
}

impl Item {
    pub fn total_bytes(&self) -> Option<u64> {
        let sizes: Vec<u64> = self.files.iter().filter_map(CatalogFile::size).collect();
        (!sizes.is_empty()).then(|| sizes.iter().sum())
    }
    /// Rough GPU memory to run it: the weights plus a working margin (text encoders unload
    /// between stages, so the largest file counts most).
    pub fn vram_gb(&self) -> Option<f32> {
        let largest = self.files.iter().filter_map(CatalogFile::size).max()?;
        let gb = largest as f32 / 1e9;
        Some((gb * 1.2 + 1.5).ceil())
    }
    /// Basic controls: runs through its template, with prompt, seed, size and steps only.
    pub fn basic_controls(&self) -> bool {
        self.template.is_some() && self.family.is_none()
    }
}

// families

/// The family a Civitai `baseModel`, a Hugging Face tag or a Manager `base` names.
pub fn family_for_label(reg: &Registry, label: &str) -> Option<String> {
    let l = label.trim();
    if l.is_empty() {
        return None;
    }
    let low = l.to_ascii_lowercase();
    for f in &reg.families {
        if f.civitai_base_models.iter().any(|b| b.eq_ignore_ascii_case(l)) || f.hf_tags.iter().any(|t| t.eq_ignore_ascii_case(l)) {
            return Some(f.id.clone());
        }
    }
    // A generation no profile lists ("HiDream O1", "Flux.3") is not its older namesake.
    if matches!(crate::detect::generation(l), crate::detect::Gen::Unknown(_)) {
        return None;
    }
    // ComfyUI-Manager's `base` spellings and common Civitai variants.
    let map = [
        ("sd1.5", "sd15"),
        ("sd1.x", "sd15"),
        ("sd 1.5", "sd15"),
        ("sdxl", "sdxl"),
        ("pony", "pony"),
        ("illustrious", "illustrious"),
        ("noobai", "illustrious"),
        ("sd3.5", "sd35"),
        ("sd3", "sd35"),
        ("flux.1 kontext", "flux1-kontext"),
        ("flux.1", "flux1"),
        ("flux.2 klein", "flux2-klein"),
        ("flux.2", "flux2"),
        ("qwen-image-edit", "qwen-edit"),
        ("qwen-image", "qwen-image"),
        ("qwen 2", "qwen-image-21"),
        ("qwen", "qwen-image"),
        ("zimage", "z-image"),
        ("z-image", "z-image"),
        ("hidream", "hidream"),
        ("ernie", "ernie"),
    ];
    map.iter().find(|(k, _)| low.starts_with(k)).map(|(_, f)| (*f).to_owned()).filter(|f| reg.family(f).is_some())
}

/// The ComfyUI folder a main model of `family` installs into.
pub fn model_folder(reg: &Registry, family: Option<&str>, file: &str) -> &'static str {
    if file.to_ascii_lowercase().ends_with(".gguf") {
        return "diffusion_models";
    }
    match family.and_then(|f| reg.family(f)).map(|f| f.pipeline.loader) {
        Some(Loader::Unet) => "diffusion_models",
        _ => "checkpoints",
    }
}

fn is_weights(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    n.ends_with(".safetensors") || n.ends_with(".gguf")
}

// parsers

/// `GET {hf}/api/models?…` (a JSON array).
/// Hugging Face pipeline tags of models the editor can't use (their LoRAs are hidden from the browser).
const NON_IMAGE_PIPELINES: &[&str] = &[
    "text-generation",
    "text2text-generation",
    "image-text-to-text",
    "text-classification",
    "token-classification",
    "feature-extraction",
    "sentence-similarity",
    "automatic-speech-recognition",
    "text-to-speech",
    "text-to-audio",
    "audio-to-audio",
    "question-answering",
    "summarization",
    "translation",
];

/// The installed name of a Hugging Face LoRA file: `<repo>-<file>` (subfolders joined with `-`), unless the file is
/// already named after its repo, so two repos' `adapter_model.safetensors` never collide in the LoRA folder.
pub fn lora_file_name(repo_id: &str, path: &str) -> String {
    let repo = repo_id.rsplit('/').next().unwrap_or(repo_id);
    let flat = path.replace('/', "-");
    let clean = |s: &str| s.chars().map(|c| if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.') { c } else { '_' }).collect::<String>();
    if flat.to_ascii_lowercase().starts_with(&repo.to_ascii_lowercase()) { clean(&flat) } else { clean(&format!("{repo}-{flat}")) }
}

pub fn parse_hf_list(reg: &Registry, hf_base: &str, v: &Value, kind: Kind) -> Vec<Item> {
    let Some(arr) = v.as_array() else { return Vec::new() };
    arr.iter()
        .filter_map(|m| {
            let id = m.get("id").or_else(|| m.get("modelId"))?.as_str()?.to_owned();
            let tags: Vec<String> =
                m.get("tags").and_then(Value::as_array).map(|a| a.iter().filter_map(|t| t.as_str().map(str::to_owned)).collect()).unwrap_or_default();
            let weights: Vec<&str> = m
                .get("siblings")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(|s| s.get("rfilename").and_then(Value::as_str))
                .filter(|f| is_weights(f) && !hf::not_comfy(f))
                .collect();
            // A LoRA repo by its tags, its name ("flux-lora-collection"), or because every weights
            // file in it is named as a LoRA: none of those may install into a model folder.
            let looks_lora = crate::arch::name_looks_like_lora;
            let is_lora = tags.iter().any(|t| t == "lora" || t.starts_with("base_model:adapter:"))
                || looks_lora(id.rsplit('/').next().unwrap_or(&id))
                || (!weights.is_empty() && weights.iter().all(|f| looks_lora(f)));
            if (kind == Kind::Lora) != is_lora {
                return None;
            }
            // The editor runs image models only: LoRAs for language, speech or audio models never load in ComfyUI here.
            let pipeline = m.get("pipeline_tag").and_then(Value::as_str).unwrap_or("");
            if NON_IMAGE_PIPELINES.contains(&pipeline) || tags.iter().any(|t| NON_IMAGE_PIPELINES.contains(&t.as_str())) {
                return None;
            }
            let file_kind = if is_lora { crate::arch::FileKind::Lora } else { crate::arch::FileKind::DiffusionModel };
            let base = tags.iter().find_map(|t| t.strip_prefix("base_model:").map(|b| b.rsplit(':').next().unwrap_or(b).to_owned()));
            let family = tags
                .iter()
                .find_map(|t| family_for_label(reg, t))
                .or_else(|| base.as_deref().and_then(|b| crate::arch::guess_from_name(b, crate::arch::FileKind::DiffusionModel)))
                .or_else(|| crate::arch::guess_from_name(&id, file_kind))
                .filter(|f| reg.family(f).is_some());
            let card = m.get("cardData");
            let license = card
                .and_then(|c| c.get("license_name").or_else(|| c.get("license")))
                .and_then(Value::as_str)
                .map(str::to_owned)
                .or_else(|| tags.iter().find_map(|t| t.strip_prefix("license:").map(str::to_owned)))
                .unwrap_or_default();
            let mut files: Vec<CatalogFile> = Vec::new();
            for &f in &weights {
                let file = f.rsplit('/').next().unwrap_or(f);
                // A LoRA shipped beside a model (a speed-up LoRA, say) goes to the LoRAs.
                let folder = if is_lora || looks_lora(f) { "loras" } else { hf::file_folder(reg, family.as_deref(), f) };
                // Every LoRA shares one folder and many repos use generic names (adapter_model.safetensors,
                // pytorch_lora_weights.safetensors), so a LoRA file is named after its repo (and subfolder).
                let name = if folder == "loras" { lora_file_name(&id, f) } else { file.to_owned() };
                if files.iter().any(|x| x.folder == folder && x.name == name) {
                    continue;
                }
                files.push(CatalogFile {
                    name,
                    folder: folder.into(),
                    url: format!("{}/{id}/resolve/main/{f}", hf_base.trim_end_matches('/')),
                    ..Default::default()
                });
            }
            Some(Item {
                id: format!("hf:{id}"),
                source: Some(Source::HuggingFace),
                kind: Some(kind),
                title: id.rsplit('/').next().unwrap_or(&id).to_owned(),
                author: m.get("author").and_then(Value::as_str).map(str::to_owned).unwrap_or_else(|| id.split('/').next().unwrap_or("").to_owned()),
                family,
                license_url: (!license.is_empty()).then(|| format!("{}/{id}", hf_base.trim_end_matches('/'))),
                license,
                likes: m.get("likes").and_then(Value::as_u64).unwrap_or(0),
                downloads: m.get("downloads").and_then(Value::as_u64).unwrap_or(0),
                created: m.get("createdAt").and_then(Value::as_str).unwrap_or("").to_owned(),
                trending: m.get("trendingScore").and_then(Value::as_f64).unwrap_or(0.0),
                gated: m.get("gated").is_some_and(|g| g.as_bool() != Some(false) && !g.is_null()),
                page_url: format!("{}/{id}", hf_base.trim_end_matches('/')),
                tags,
                files,
                ..Default::default()
            })
        })
        .collect()
}

/// `GET {hf}/api/models/{repo}/tree/main?recursive=true`: `path → (size, sha256)` of LFS files.
pub fn parse_hf_tree(v: &Value) -> BTreeMap<String, (u64, String)> {
    v.as_array()
        .into_iter()
        .flatten()
        .filter_map(|e| {
            let path = e.get("path")?.as_str()?.to_owned();
            let lfs = e.get("lfs")?;
            // A gated repo read without its token masks the hashes ("****…"): no hash, so the plan explains the gate.
            let sha = lfs.get("oid").or_else(|| lfs.get("sha256"))?.as_str()?.to_owned();
            if sha.starts_with('*') {
                return None;
            }
            let size = lfs.get("size").or_else(|| e.get("size"))?.as_u64()?;
            Some((path, (size, sha)))
        })
        .collect()
}

fn commercial(item: &Value) -> Option<bool> {
    let v = item.get("allowCommercialUse")?;
    let list: Vec<&str> = match v {
        Value::Array(a) => a.iter().filter_map(Value::as_str).collect(),
        Value::String(s) => vec![s.as_str()],
        _ => return None,
    };
    Some(list.iter().any(|x| *x != "None"))
}

/// `GET {civitai}/api/v1/models?…`.
pub fn parse_civitai(reg: &Registry, v: &Value) -> Vec<Item> {
    v.get("items")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|m| {
            let kind = match m.get("type")?.as_str()? {
                "LORA" | "LoCon" | "DoRA" => Kind::Lora,
                "Checkpoint" => Kind::Model,
                _ => return None,
            };
            let version = m.get("modelVersions")?.as_array()?.first()?;
            let base = version.get("baseModel").and_then(Value::as_str).unwrap_or("");
            let family = family_for_label(reg, base);
            let files = version
                .get("files")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter(|f| matches!(f.get("type").and_then(Value::as_str), Some("Model") | Some("Pruned Model") | None))
                .filter_map(|f| {
                    let name = f.get("name")?.as_str()?.to_owned();
                    if !is_weights(&name) {
                        return None;
                    }
                    let folder = if kind == Kind::Lora { "loras" } else { model_folder(reg, family.as_deref(), &name) };
                    Some(CatalogFile {
                        name,
                        folder: folder.into(),
                        url: f.get("downloadUrl")?.as_str()?.to_owned(),
                        bytes: None,
                        approx_bytes: f.get("sizeKB").and_then(Value::as_f64).map(|kb| (kb * 1024.0) as u64),
                        sha256: f.get("hashes").and_then(|h| h.get("SHA256")).and_then(Value::as_str).map(str::to_ascii_lowercase),
                    })
                })
                .take(1)
                .collect();
            let preview = version
                .get("images")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .find(|i| i.get("nsfwLevel").and_then(Value::as_u64).unwrap_or(1) <= 1 && i.get("type").and_then(Value::as_str).unwrap_or("image") == "image")
                .and_then(|i| i.get("url")?.as_str().map(str::to_owned));
            let id = m.get("id")?.as_u64()?;
            let commercial = commercial(m);
            Some(Item {
                id: format!("civitai:{id}"),
                source: Some(Source::Civitai),
                kind: Some(kind),
                title: m.get("name")?.as_str()?.to_owned(),
                author: m.get("creator").and_then(|c| c.get("username")).and_then(Value::as_str).unwrap_or("").to_owned(),
                description: String::new(),
                family,
                // The card adds "non-commercial" from `commercial`.
                license: format!("{base} base licence"),
                license_url: Some(format!("https://civitai.com/models/{id}")),
                commercial,
                preview,
                likes: m.get("stats").and_then(|s| s.get("thumbsUpCount")).and_then(Value::as_u64).unwrap_or(0),
                downloads: m.get("stats").and_then(|s| s.get("downloadCount")).and_then(Value::as_u64).unwrap_or(0),
                created: version.get("publishedAt").and_then(Value::as_str).unwrap_or("").to_owned(),
                trending: 0.0,
                tags: m.get("tags").and_then(Value::as_array).map(|a| a.iter().filter_map(|t| t.as_str().map(str::to_owned)).collect()).unwrap_or_default(),
                files,
                page_url: format!("https://civitai.com/models/{id}"),
                gated: false,
                nsfw: m.get("nsfw").and_then(Value::as_bool).unwrap_or(false),
                template: None,
                min_comfyui: None,
            })
        })
        .filter(|i| !i.nsfw && !i.files.is_empty())
        .collect()
}

/// Text sizes like `"11.9GB"` (ComfyUI-Manager).
fn parse_size(s: &str) -> Option<u64> {
    let s = s.trim().to_ascii_uppercase();
    let (num, mult) = if let Some(n) = s.strip_suffix("GB") {
        (n, 1e9)
    } else if let Some(n) = s.strip_suffix("MB") {
        (n, 1e6)
    } else if let Some(n) = s.strip_suffix("KB") {
        (n, 1e3)
    } else {
        (s.as_str(), 1.0)
    };
    num.trim().parse::<f64>().ok().map(|v| (v * mult) as u64)
}

/// ComfyUI-Manager's `model-list.json` (`{"models": [...]}`): main models and LoRAs.
pub fn parse_manager(reg: &Registry, v: &Value) -> Vec<Item> {
    v.get("models")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|m| {
            let ty = m.get("type")?.as_str()?.to_ascii_lowercase();
            let kind = match ty.as_str() {
                "lora" => Kind::Lora,
                "checkpoint" | "checkpoints" | "diffusion_model" | "unet" => Kind::Model,
                _ => return None,
            };
            let url = m.get("url")?.as_str()?.to_owned();
            let mut name = m.get("filename")?.as_str()?.to_owned();
            if name == "<huggingface>" {
                name = url.rsplit('/').next()?.split('?').next()?.to_owned();
            }
            if !is_weights(&name) {
                return None;
            }
            let family = family_for_label(reg, m.get("base").and_then(Value::as_str).unwrap_or(""));
            let save = m.get("save_path").and_then(Value::as_str).unwrap_or("default");
            let folder = if save != "default" && !save.contains("..") && !save.starts_with('/') {
                save.to_owned()
            } else if kind == Kind::Lora {
                "loras".into()
            } else if ty == "checkpoint" || ty == "checkpoints" {
                "checkpoints".into()
            } else {
                "diffusion_models".into()
            };
            Some(Item {
                id: format!("manager:{name}"),
                source: Some(Source::Community),
                kind: Some(kind),
                title: m.get("name")?.as_str()?.to_owned(),
                author: String::new(),
                description: m.get("description").and_then(Value::as_str).unwrap_or("").to_owned(),
                family,
                license: String::new(),
                license_url: m.get("reference").and_then(Value::as_str).map(str::to_owned),
                page_url: m.get("reference").and_then(Value::as_str).unwrap_or("").to_owned(),
                files: vec![CatalogFile { name, folder, url, approx_bytes: m.get("size").and_then(Value::as_str).and_then(parse_size), ..Default::default() }],
                ..Default::default()
            })
        })
        .collect()
}

pub use crate::template::index::{TemplateEntry, parse_template_index, template_files};

/// A card for a template: the builder family its main model file belongs to (by the profiles'
/// name rules, then the built-in names through the generation veto), or none: the template runs
/// as it is.
pub fn template_item(reg: &Registry, e: &TemplateEntry, files: Vec<CatalogFile>, templates_base: &str) -> Item {
    let main = files.iter().find(|f| f.folder == "diffusion_models" || f.folder == "checkpoints" || f.folder == "unet");
    let family = main
        .and_then(|f| {
            crate::detect::by_name(reg, &f.name).map(|m| m.id).or_else(|| crate::arch::guess_from_name(&f.name, crate::arch::FileKind::DiffusionModel))
        })
        .filter(|f| reg.family(f).is_some_and(crate::family::Family::buildable));
    let mut files = files;
    if e.size > 0 && files.iter().all(|f| f.size().is_none()) && !files.is_empty() {
        // The index's total, spread for display until the exact sizes are resolved.
        let n = files.len() as u64;
        for f in &mut files {
            f.approx_bytes = Some(e.size / n);
        }
    }
    Item {
        id: format!("template:{}", e.name),
        source: Some(Source::Template),
        kind: Some(Kind::Model),
        title: if e.title.is_empty() { e.name.clone() } else { e.title.clone() },
        author: "ComfyUI".into(),
        description: e.description.clone(),
        family,
        license: String::new(),
        preview: Some(format!("{}/{}-1.webp", templates_base.trim_end_matches('/'), e.name)),
        created: e.date.clone(),
        tags: e.tags.clone(),
        files,
        page_url: format!("https://github.com/Comfy-Org/workflow_templates/blob/main/templates/{}.json", e.name),
        template: Some(e.name.clone()),
        min_comfyui: e.min_comfyui.map(|v| v.to_string()),
        ..Default::default()
    }
}

// queries

/// What the browser shows.
#[derive(Clone, Debug, PartialEq)]
pub enum View {
    New,
    Trending,
    Family(String),
    Installed,
    FitsGpu(f32),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Query {
    pub view: View,
    pub kind: Kind,
    pub search: String,
    /// Only these sources (empty = all).
    pub sources: Vec<Source>,
}

/// The catalogue URLs a query fetches.
pub fn urls(cfg: &Config, reg: &Registry, q: &Query) -> Vec<(Source, String)> {
    if matches!(&q.view, View::Family(f) if f == "restoration") {
        return Vec::new();
    }
    let enc = |s: &str| urlencode(s);
    let mut out = Vec::new();
    let family = match &q.view {
        View::Family(f) => reg.family(f),
        _ => None,
    };
    let want = |s: Source| q.sources.is_empty() || q.sources.contains(&s);
    if want(Source::HuggingFace) && !matches!(q.view, View::Installed) {
        let sort = match q.view {
            View::New => "createdAt",
            View::Trending => "trendingScore",
            _ => "likes",
        };
        let mut u = format!("{}/api/models?sort={sort}&limit=40&full=true", cfg.hf.trim_end_matches('/'));
        match q.kind {
            Kind::Lora => u.push_str("&filter=lora"),
            Kind::Model => u.push_str("&filter=text-to-image"),
        }
        if let Some(t) = family.and_then(|f| f.hf_tags.first()) {
            u.push_str(&format!("&filter={}", enc(t)));
        }
        if !q.search.trim().is_empty() {
            u.push_str(&format!("&search={}", enc(q.search.trim())));
        }
        out.push((Source::HuggingFace, u));
    }
    if want(Source::Civitai) && !matches!(q.view, View::Installed) {
        let sort = match q.view {
            View::New => "Newest",
            View::Trending => "Most Downloaded&period=Week",
            _ => "Highest Rated",
        };
        let ty = if q.kind == Kind::Lora { "LORA" } else { "Checkpoint" };
        let mut u = format!("{}/api/v1/models?limit=40&nsfw=false&types={ty}&sort={}", cfg.civitai.trim_end_matches('/'), sort.replace(' ', "%20"));
        for b in family.map(|f| f.civitai_base_models.as_slice()).unwrap_or(&[]) {
            u.push_str(&format!("&baseModels={}", enc(b)));
        }
        if !q.search.trim().is_empty() {
            u.push_str(&format!("&query={}", enc(q.search.trim())));
        }
        out.push((Source::Civitai, u));
    }
    if want(Source::Template) && q.kind == Kind::Model && !matches!(q.view, View::Installed) {
        out.push((Source::Template, format!("{}/index.json", cfg.templates.trim_end_matches('/'))));
    }
    if want(Source::Community) && !matches!(q.view, View::Installed | View::New | View::Trending) {
        out.push((Source::Community, cfg.manager_list.clone()));
    }
    out
}

pub fn urlencode(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => (b as char).to_string(),
            b' ' => "%20".into(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

/// Turns fetched catalogue bodies into cards, filtered and sorted for the query.
pub fn items(reg: &Registry, cfg: &Config, q: &Query, bodies: &[(Source, Value)]) -> Vec<Item> {
    if matches!(&q.view, View::Family(f) if f == "restoration") {
        if q.kind != Kind::Model {
            return Vec::new();
        }
        let needle = q.search.trim().to_ascii_lowercase();
        return restoration_items()
            .into_iter()
            .filter(|i| i.title.to_ascii_lowercase().contains(&needle) || i.description.to_ascii_lowercase().contains(&needle))
            .collect();
    }
    let mut out = Vec::new();
    for (src, v) in bodies {
        match src {
            Source::HuggingFace => out.extend(parse_hf_list(reg, &cfg.hf, v, q.kind)),
            Source::Civitai => out.extend(parse_civitai(reg, v)),
            Source::Community => out.extend(parse_manager(reg, v).into_iter().filter(|i| i.kind == Some(q.kind))),
            Source::Template => {
                // Template files come with the template; the card lists the main models by name.
                for e in parse_template_index(v) {
                    let files = e.models.iter().map(|m| CatalogFile { name: m.clone(), ..Default::default() }).collect();
                    let mut item = template_item(reg, &e, files, &cfg.templates);
                    item.files.clear();
                    if e.size > 0 {
                        item.files.push(CatalogFile { name: "(template files)".into(), approx_bytes: Some(e.size), ..Default::default() });
                    }
                    // The index's `models` label names the series ("HiDream" for HiDream-O1 too); a
                    // template named after a generation no profile describes has no builder family.
                    let new_generation = matches!(crate::detect::generation(&e.name), crate::detect::Gen::Unknown(_));
                    item.family = e
                        .models
                        .iter()
                        .chain(std::iter::once(&e.name))
                        .filter(|_| !new_generation)
                        .find_map(|m| family_for_label(reg, m).or_else(|| crate::arch::guess_from_name(m, crate::arch::FileKind::DiffusionModel)))
                        .filter(|f| reg.family(f).is_some());
                    out.push(item);
                }
            }
            Source::Installed => {}
        }
    }
    let needle = q.search.trim().to_ascii_lowercase();
    out.retain(|i| !external_only_item(i));
    out.retain(|i| needle.is_empty() || i.title.to_ascii_lowercase().contains(&needle) || i.description.to_ascii_lowercase().contains(&needle));
    match &q.view {
        View::Family(f) => {
            out.retain(|i| i.family.as_deref() == Some(f.as_str()) || reg.family(f).is_some_and(|fam| i.family.as_deref().is_some_and(|x| fam.lora_fits(x))))
        }
        View::FitsGpu(gb) => out.retain(|i| i.vram_gb().is_some_and(|v| v <= *gb)),
        _ => {}
    }
    match q.view {
        View::New => out.sort_by(|a, b| b.created.cmp(&a.created)),
        View::Trending => out.sort_by(|a, b| b.trending.partial_cmp(&a.trending).unwrap_or(std::cmp::Ordering::Equal).then(b.downloads.cmp(&a.downloads))),
        _ => out.sort_by(|a, b| b.likes.cmp(&a.likes).then(b.downloads.cmp(&a.downloads))),
    }
    out
}

// network & cache

/// HTTP access for the browser (tests and the mock substitute their own).
pub trait Net {
    /// GET a JSON or image body (with the source's token when it has one).
    fn get(&self, url: &str) -> Result<Vec<u8>>;
    /// The exact size of a download (`Content-Length` after redirects), if the server says.
    fn head_size(&self, url: &str) -> Result<Option<u64>>;
}

/// The browser's HTTP client. Catalogue reads go only to the configured sources (and Civitai's
/// image host for previews); tokens go only to their own source.
pub struct HttpNet {
    cfg: Config,
    agent: ureq::Agent,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Origin {
    scheme: String,
    host: String,
    port: u16,
}

fn origin(url: &str) -> Option<Origin> {
    let uri: ureq::http::Uri = url.parse().ok()?;
    let authority = uri.authority()?;
    if authority.as_str().contains('@') {
        return None;
    }
    let scheme = uri.scheme_str()?.to_ascii_lowercase();
    let default_port = match scheme.as_str() {
        "https" => 443,
        "http" => 80,
        _ => return None,
    };
    let host = authority.host();
    if host.is_empty() {
        return None;
    }
    let suffix = authority.as_str().strip_prefix(host)?;
    let port = if suffix.is_empty() { default_port } else { suffix.strip_prefix(':')?.parse::<u16>().ok()? };
    Some(Origin { scheme, host: authority.host().to_ascii_lowercase(), port })
}

fn same_origin(url: &str, base: &str) -> bool {
    origin(url).zip(origin(base)).is_some_and(|(url, base)| url == base)
}

fn routed_token<'a>(token: Option<&'a str>, initial: &Origin, destination: &str) -> Option<&'a str> {
    token.filter(|token| !token.is_empty() && origin(destination).as_ref() == Some(initial))
}

fn redirect_url(current: &str, location: &str) -> Result<String> {
    let uri: ureq::http::Uri = current.parse().context("Invalid catalogue URL")?;
    let scheme = uri.scheme_str().context("Catalogue URL has no scheme")?;
    let authority = uri.authority().context("Catalogue URL has no authority")?;
    let next = if location.starts_with("//") {
        format!("{scheme}:{location}")
    } else if location.contains("://") {
        location.to_owned()
    } else if location.starts_with('/') {
        format!("{scheme}://{authority}{location}")
    } else if location.starts_with('?') {
        format!("{scheme}://{authority}{}{location}", uri.path())
    } else {
        let directory = uri.path().rsplit_once('/').map_or("", |(directory, _)| directory);
        format!("{scheme}://{authority}{directory}/{location}")
    };
    let next_origin = origin(&next).context("Invalid catalogue redirect")?;
    if scheme == "https" && next_origin.scheme != "https" {
        bail!("Catalogue redirect would downgrade HTTPS");
    }
    Ok(next)
}

impl HttpNet {
    pub fn new(cfg: Config) -> Self {
        let agent = ureq::Agent::config_builder()
            .tls_config(crate::tls::config())
            .timeout_connect(Some(std::time::Duration::from_secs(15)))
            .timeout_global(Some(std::time::Duration::from_secs(60)))
            .max_redirects(0)
            .http_status_as_error(false)
            .user_agent(concat!("LocalImage/", env!("CARGO_PKG_VERSION")))
            .build()
            .into();
        Self { cfg, agent }
    }

    fn allowed(&self, url: &str) -> bool {
        let Some(candidate) = origin(url) else { return false };
        let c = &self.cfg;
        let bases = [c.hf.as_str(), c.civitai.as_str(), c.manager_list.as_str(), c.templates.as_str()];
        bases.iter().any(|base| same_origin(url, base))
            || same_origin(url, "https://image.civitai.com")
            || same_origin(url, crate::family::REMOTE_BASE)
            || std::env::var("LOCAL_IMAGE_FAMILIES_BASE").is_ok_and(|base| same_origin(url, &base))
            || (candidate.scheme == "https" && candidate.port == 443 && crate::download::host_allowed(url))
    }

    fn token(&self, url: &str) -> Option<&str> {
        if same_origin(url, &self.cfg.hf) {
            self.cfg.hf_token.as_deref()
        } else if same_origin(url, &self.cfg.civitai) {
            self.cfg.civitai_token.as_deref()
        } else {
            None
        }
    }
}

impl Net for HttpNet {
    fn get(&self, url: &str) -> Result<Vec<u8>> {
        let first = origin(url).context("Invalid catalogue URL")?;
        let token = self.token(url).filter(|token| !token.is_empty());
        let mut current = url.to_owned();
        for _ in 0..5 {
            if !self.allowed(&current) {
                bail!("Local Image doesn't read catalogues from this origin");
            }
            let mut req = self.agent.get(&current);
            if let Some(token) = routed_token(token, &first, &current) {
                req = req.header("Authorization", &format!("Bearer {token}"));
            }
            let mut response = req.call().context("Could not reach catalogue source")?;
            if response.status().is_redirection() {
                let location = response.headers().get("location").and_then(|value| value.to_str().ok()).context("Catalogue redirect has no location")?;
                current = redirect_url(&current, location)?;
                continue;
            }
            if !response.status().is_success() {
                bail!("Catalogue source returned HTTP {}", response.status());
            }
            return Ok(response.body_mut().with_config().limit(48 << 20).read_to_vec()?);
        }
        bail!("Too many catalogue redirects")
    }
    fn head_size(&self, url: &str) -> Result<Option<u64>> {
        crate::download::remote_size(url, self.token(url))
    }
}

/// An official template: its editor-format workflow and the model files it declares.
pub fn fetch_template(cfg: &Config, net: &dyn Net, name: &str) -> Result<(Value, Vec<CatalogFile>)> {
    if name.contains('/') || name.contains("..") {
        bail!("bad template name");
    }
    let body = net.get(&format!("{}/{name}.json", cfg.templates.trim_end_matches('/')))?;
    let ui: Value = serde_json::from_slice(&body).context("the template isn't valid JSON")?;
    let files = template_files(&ui).into_iter().filter(|f| is_weights(&f.name)).collect();
    Ok((ui, files))
}

/// Catalogue bodies cached by URL: the browser shows the last copy offline.
pub struct Cache {
    pub dir: PathBuf,
}

impl Cache {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }
    pub fn default_dir() -> PathBuf {
        crate::settings::data_root().join("catalog-cache")
    }
    fn path(&self, url: &str) -> PathBuf {
        use sha2::Digest;
        let h = hex::encode(sha2::Sha256::digest(url.as_bytes()));
        self.dir.join(format!("{}.json", &h[..24]))
    }
    /// The cached body and its age in seconds.
    pub fn get(&self, url: &str) -> Option<(Vec<u8>, u64)> {
        let p = self.path(url);
        let age = std::fs::metadata(&p).ok()?.modified().ok()?.elapsed().map(|d| d.as_secs()).unwrap_or(0);
        Some((std::fs::read(p).ok()?, age))
    }
    pub fn put(&self, url: &str, body: &[u8]) -> Result<()> {
        std::fs::create_dir_all(&self.dir)?;
        li_fs::atomic_write(&self.path(url), body)?;
        Ok(())
    }
    /// Fetches `url` online (caching it), or falls back to the cache; `online = false` reads the
    /// cache only. Returns the body and whether it came from the cache.
    pub fn fetch(&self, net: &dyn Net, url: &str, online: bool) -> Result<(Vec<u8>, bool)> {
        if online {
            match net.get(url) {
                Ok(b) => {
                    let _ = self.put(url, &b);
                    return Ok((b, false));
                }
                Err(e) => {
                    if let Some((b, _)) = self.get(url) {
                        log::warn!("{url}: {e:#}; showing the cached copy");
                        return Ok((b, true));
                    }
                    return Err(e);
                }
            }
        }
        self.get(url).map(|(b, _)| (b, true)).context("not cached yet: connect to the internet and refresh")
    }
}

// install plans

/// A verified file to download.
#[derive(Clone, Debug, PartialEq)]
pub struct PlannedFile {
    pub name: String,
    pub folder: String,
    pub url: String,
    pub bytes: u64,
    pub sha256: String,
}

/// What installing a card does.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Plan {
    pub files: Vec<PlannedFile>,
    /// Files already installed (skipped).
    pub present: Vec<String>,
    pub license: String,
    pub license_url: Option<String>,
    pub gated: bool,
}

/// Offline curated cards: the embedded restoration policy cannot be replaced by an online list.
fn external_only_item(item: &Item) -> bool {
    crate::restoration::byo::external_only(&format!("{}/{}/{}", item.id, item.title, item.page_url))
        || item.files.iter().any(|f| crate::restoration::byo::external_only(&format!("{}/{}", f.url, f.name)))
}

/// Offline curated cards: the embedded restoration policy cannot be replaced by an online list.
pub fn restoration_items() -> Vec<Item> {
    crate::restoration::policy()
        .models
        .iter()
        .map(|m| Item {
            id: format!("restoration:{}", m.key),
            kind: Some(Kind::Model),
            title: m.label.clone(),
            description: format!("{} {}", m.description, m.notes),
            family: Some("restoration".into()),
            license: m.license.clone().unwrap_or_default(),
            license_url: m.license_url.clone(),
            commercial: Some(true),
            page_url: m.license_url.clone().unwrap_or_default(),
            files: m.presets[0]
                .files
                .iter()
                .map(|f| CatalogFile {
                    name: f.name.clone(),
                    folder: f.folder.clone(),
                    url: f.url.clone(),
                    bytes: Some(f.bytes),
                    sha256: Some(f.sha256.clone()),
                    ..Default::default()
                })
                .collect(),
            ..Default::default()
        })
        .collect()
}

impl Plan {
    pub fn total(&self) -> u64 {
        self.files.iter().map(|f| f.bytes).sum()
    }
}

/// Whether ComfyUI already lists `name` in `folder`.
fn installed(info: &ObjectInfo, folder: &str, name: &str) -> bool {
    let (class, input) = match folder {
        "checkpoints" => ("CheckpointLoaderSimple", "ckpt_name"),
        "diffusion_models" | "unet" => ("UNETLoader", "unet_name"),
        "loras" => ("LoraLoader", "lora_name"),
        "text_encoders" | "clip" => ("CLIPLoader", "clip_name"),
        "vae" => ("VAELoader", "vae_name"),
        "clip_vision" => ("CLIPVisionLoader", "clip_name"),
        "style_models" => ("StyleModelLoader", "style_model_name"),
        "upscale_models" => ("UpscaleModelLoader", "model_name"),
        _ => return false,
    };
    info.find_file(class, input, name).is_some() || (class == "UNETLoader" && info.find_file("UnetLoaderGGUF", "unet_name", name).is_some())
}

/// Catalogue paths are portable relative names, never paths chosen by a remote publisher.
fn validate_install_path(folder: &str, name: &str) -> Result<()> {
    fn component(s: &str) -> bool {
        let stem = s.split('.').next().unwrap_or("").to_ascii_uppercase();
        !s.is_empty()
            && s != "."
            && s != ".."
            && !s.ends_with(['.', ' '])
            && !s.chars().any(|c| c.is_control() || matches!(c, '/' | '\\' | ':' | '<' | '>' | '"' | '|' | '?' | '*'))
            && !matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
            && !(stem.len() == 4 && (stem.starts_with("COM") || stem.starts_with("LPT")) && matches!(stem.as_bytes()[3], b'1'..=b'9'))
    }
    if !component(name) || (!folder.is_empty() && !folder.split('/').all(component)) {
        bail!("Invalid model destination: use a file name and relative model folder without traversal or platform path prefixes");
    }
    Ok(())
}

/// Resolve only within the selected root, including existing directory symlinks. The root
/// itself may be a user-selected symlink (for example to a model drive).
fn confined_destination(root: &Path, file: &PlannedFile) -> Result<PathBuf> {
    validate_install_path(&file.folder, &file.name)?;
    let mut parent = root.to_path_buf();
    for part in file.folder.split('/').filter(|p| !p.is_empty()) {
        parent.push(part);
        match std::fs::symlink_metadata(&parent) {
            Ok(_) => {
                let resolved = parent.canonicalize().context("The model folder link is unavailable")?;
                if !resolved.starts_with(root) || !resolved.is_dir() {
                    bail!("The model folder leaves the selected models directory or is not a directory");
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
    }
    let dest = parent.join(&file.name);
    match std::fs::symlink_metadata(&dest) {
        Ok(meta) if meta.file_type().is_symlink() => bail!("The model destination is a symbolic link"),
        Ok(_) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(e.into()),
    }
    Ok(dest)
}

/// Resolves exact sizes and hashes and adds the family's missing text encoders and VAE. Fails
/// (with the reason) for anything Local Image won't install unverified.
pub fn plan(item: &Item, files: &[CatalogFile], fam: Option<&Family>, info: &ObjectInfo, cfg: &Config, net: &dyn Net) -> Result<Plan> {
    for file in files {
        validate_install_path(&file.folder, &file.name)?;
    }
    if external_only_item(item) || files.iter().any(|f| crate::restoration::byo::external_only(&format!("{}/{}", f.url, f.name))) {
        bail!("This model is user-installed only; Local Image never downloads it");
    }
    if item.family.as_deref() == Some("restoration") || files.iter().any(|f| crate::restoration::pinned_file(&f.name).is_some()) {
        let canonical = restoration_items().into_iter().find(|i| i.id == item.id).context("Unknown restoration browser card")?;
        if files != canonical.files {
            bail!("Restoration browser files differ from the built-in allowlist");
        }
        let mut plan = Plan { license: canonical.license, license_url: canonical.license_url, ..Default::default() };
        for f in canonical.files {
            if installed(info, &f.folder, &f.name) {
                plan.present.push(f.name);
            } else {
                plan.files.push(PlannedFile { name: f.name, folder: f.folder, url: f.url, bytes: f.bytes.unwrap(), sha256: f.sha256.unwrap() });
            }
        }
        return Ok(plan);
    }

    let mut want: Vec<CatalogFile> = files.to_vec();
    // The family's components that ComfyUI doesn't have.
    if let Some(f) = fam
        && item.kind != Some(Kind::Lora)
    {
        let have = crate::inventory::resolve_components(f, info);
        for c in &f.components {
            // A planned file stands in for a component only in the component's own folder.
            if have.iter().any(|(r, _)| *r == c.role)
                || want.iter().any(|w| w.folder == c.role.folder() && c.names.iter().any(|n| n.eq_ignore_ascii_case(&w.name)))
            {
                continue;
            }
            if let Some(d) = &c.download {
                want.push(CatalogFile {
                    name: d.name.clone(),
                    folder: d.folder.clone(),
                    url: d.url.clone(),
                    bytes: Some(d.bytes),
                    approx_bytes: None,
                    sha256: Some(d.sha256.clone()),
                });
            } else if let Some(src) = c.source.as_deref().and_then(|s| s.strip_prefix("hf:")) {
                let mut parts = src.splitn(3, '/');
                let (Some(owner), Some(repo), Some(path)) = (parts.next(), parts.next(), parts.next()) else { continue };
                let name = path.rsplit('/').next().unwrap_or(path).to_owned();
                want.push(CatalogFile {
                    name,
                    folder: c.role.folder().into(),
                    url: format!("{}/{owner}/{repo}/resolve/main/{path}", cfg.hf.trim_end_matches('/')),
                    ..Default::default()
                });
            }
        }
    }
    let mut out = Plan { license: item.license.clone(), license_url: item.license_url.clone(), gated: item.gated, ..Default::default() };
    let mut trees: BTreeMap<String, BTreeMap<String, (u64, String)>> = BTreeMap::new();
    for file in &want {
        validate_install_path(&file.folder, &file.name)?;
    }
    for f in want {
        if !is_weights(&f.name) {
            bail!("{} is not a .safetensors or .gguf file; Local Image only installs those.", f.name);
        }
        if installed(info, &f.folder, &f.name) {
            out.present.push(f.name.clone());
            continue;
        }
        if !crate::download::host_allowed(&f.url) {
            bail!("{} would come from an unexpected host ({}).", f.name, f.url);
        }
        let (mut bytes, mut sha) = (f.bytes, f.sha256.clone());
        if let Some((repo, path)) = f.hf_path(&cfg.hf) {
            if !trees.contains_key(&repo) {
                trees.insert(repo.clone(), hf_tree(cfg, net, &repo)?);
            }
            if let Some((size, hash)) = trees.get(&repo).and_then(|t| t.get(&path)) {
                bytes = Some(*size);
                sha = Some(hash.clone());
            }
        } else if bytes.is_none() && sha.is_some() {
            // Civitai: the published hash, and the exact size from the download's headers,
            // which must agree with the listed size.
            let exact = net.head_size(&f.url)?;
            if let (Some(e), Some(a)) = (exact, f.approx_bytes)
                && (e as f64 - a as f64).abs() > a as f64 * 0.02 + 4096.0
            {
                bail!(
                    "{}: the download is {} but the listing says {}; not installing.",
                    f.name,
                    crate::download::human_bytes(e),
                    crate::download::human_bytes(a)
                );
            }
            bytes = exact;
        }
        let (Some(bytes), Some(sha)) = (bytes, sha.filter(|s| s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit()))) else {
            if item.gated && cfg.hf_token.as_deref().is_none_or(str::is_empty) && f.hf_path(&cfg.hf).is_some() {
                bail!(
                    "{} is gated: accept its terms on its Hugging Face page and add a Hugging Face token in Local AI › Accounts, then install again.",
                    f.name
                );
            }
            bail!("{} has no published size and SHA-256, so it can't be verified; Local Image doesn't install unverified files.", f.name);
        };
        out.files.push(PlannedFile { name: f.name, folder: f.folder, url: f.url, bytes, sha256: sha.to_ascii_lowercase() });
    }
    Ok(out)
}

/// A Hugging Face repository's files: path → (size, SHA-256).
fn hf_tree(cfg: &Config, net: &dyn Net, repo: &str) -> Result<BTreeMap<String, (u64, String)>> {
    let url = format!("{}/api/models/{repo}/tree/main?recursive=true", cfg.hf.trim_end_matches('/'));
    let body = net.get(&url).with_context(|| format!("Could not read the file list of {repo}"))?;
    Ok(parse_hf_tree(&serde_json::from_slice(&body)?))
}

/// Downloads a plan into `model_dir/<folder>/<name>` (verified, atomic). Tokens go only to their
/// own hosts.
pub fn install(plan: &Plan, model_dir: &Path, cfg: &Config, ctl: &crate::comfy::JobControl, on_progress: &dyn Fn(u64, u64, &str)) -> Result<()> {
    for f in &plan.files {
        validate_install_path(&f.folder, &f.name)?;
        if crate::restoration::byo::external_only(&format!("{}/{}", f.url, f.name)) {
            bail!("This model is user-installed only; Local Image never downloads it");
        }
        if f.name.to_ascii_lowercase().ends_with(".pth") || crate::restoration::pinned_file(&f.name).is_some() {
            crate::restoration::validate_download(&f.name, &f.folder, &f.url, f.bytes, &f.sha256)?;
        }
    }
    if plan.files.is_empty() {
        return Ok(());
    }
    ctl.check()?;
    std::fs::create_dir_all(model_dir)?;
    let root = model_dir.canonicalize().context("The models directory is unavailable")?;
    // Check the whole plan before fetching even its first file.
    for file in &plan.files {
        confined_destination(&root, file)?;
    }
    let total = plan.total();
    let mut done = 0u64;
    for f in &plan.files {
        ctl.check()?;
        let dest = confined_destination(&root, f)?;
        if dest.exists() {
            let spec = crate::family::FileSpec { name: f.name.clone(), bytes: f.bytes, sha256: f.sha256.clone(), ..Default::default() };
            crate::download::verify_existing(&dest, &spec)?;
            done += f.bytes;
            continue;
        }
        let token = if same_origin(&f.url, &cfg.hf) {
            cfg.hf_token.as_deref()
        } else if same_origin(&f.url, &cfg.civitai) {
            cfg.civitai_token.as_deref()
        } else {
            None
        };
        let base = done;
        crate::download::download_file_with(&f.url, &dest, f.bytes, &f.sha256, token, ctl, &|n| on_progress(base + n, total, &f.name)).map_err(|e| {
            let s = e.to_string();
            if s.contains("401") || s.contains("403") {
                anyhow::anyhow!("{}: the publisher requires you to sign in or accept its licence (add a token in Local AI › Accounts). {s}", f.name)
            } else {
                e
            }
        })?;
        done += f.bytes;
    }
    Ok(())
}

/// Where a role's files go (for component downloads).
pub fn folder_for(role: Role) -> &'static str {
    role.folder()
}

#[cfg(test)]
mod tests;
