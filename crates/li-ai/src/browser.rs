//! The model and LoRA browser's data: live catalogues from Hugging Face, Civitai, ComfyUI's
//! official workflow templates and ComfyUI-Manager's community list, turned into one kind of
//! card ([`Item`]) with its family, capabilities, download size, rough GPU memory and licence;
//! plus install planning with Local Image's guardrails:
//!
//! - only `.safetensors` and `.gguf` files;
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

// ------------------------------------------------------------------------------ families

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

// ------------------------------------------------------------------------------ parsers

/// `GET {hf}/api/models?…` (a JSON array).
pub fn parse_hf_list(reg: &Registry, hf_base: &str, v: &Value, kind: Kind) -> Vec<Item> {
    let Some(arr) = v.as_array() else { return Vec::new() };
    arr.iter()
        .filter_map(|m| {
            let id = m.get("id").or_else(|| m.get("modelId"))?.as_str()?.to_owned();
            let tags: Vec<String> = m.get("tags").and_then(Value::as_array).map(|a| a.iter().filter_map(|t| t.as_str().map(str::to_owned)).collect()).unwrap_or_default();
            let is_lora = tags.iter().any(|t| t == "lora" || t.starts_with("base_model:adapter:"));
            if (kind == Kind::Lora) != is_lora {
                return None;
            }
            let base = tags.iter().find_map(|t| t.strip_prefix("base_model:").map(|b| b.rsplit(':').next().unwrap_or(b).to_owned()));
            let family = tags
                .iter()
                .find_map(|t| family_for_label(reg, t))
                .or_else(|| base.as_deref().and_then(|b| crate::arch::guess_from_name(b, crate::arch::FileKind::DiffusionModel)))
                .or_else(|| crate::arch::guess_from_name(&id, crate::arch::FileKind::DiffusionModel))
                .filter(|f| reg.family(f).is_some());
            let card = m.get("cardData");
            let license = card
                .and_then(|c| c.get("license_name").or_else(|| c.get("license")))
                .and_then(Value::as_str)
                .map(str::to_owned)
                .or_else(|| tags.iter().find_map(|t| t.strip_prefix("license:").map(str::to_owned)))
                .unwrap_or_default();
            let files = m
                .get("siblings")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(|s| s.get("rfilename").and_then(Value::as_str))
                .filter(|f| is_weights(f))
                .map(|f| {
                    let name = f.rsplit('/').next().unwrap_or(f).to_owned();
                    let folder = if is_lora { "loras" } else { model_folder(reg, family.as_deref(), f) };
                    CatalogFile { name, folder: folder.into(), url: format!("{}/{id}/resolve/main/{f}", hf_base.trim_end_matches('/')), ..Default::default() }
                })
                .collect();
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
            let sha = lfs.get("oid").or_else(|| lfs.get("sha256"))?.as_str()?.to_owned();
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
                license: match commercial {
                    Some(true) => format!("{base} base licence · commercial use allowed"),
                    Some(false) => format!("{base} base licence · no commercial use"),
                    None => format!("{base} base licence"),
                },
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

/// One entry of the templates' `index.json`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TemplateEntry {
    pub name: String,
    pub title: String,
    pub description: String,
    pub tags: Vec<String>,
    pub models: Vec<String>,
    pub date: String,
    pub size: u64,
    pub custom_nodes: Vec<String>,
}

/// The image templates of `index.json` that run locally (open-source, no API nodes).
pub fn parse_template_index(v: &Value) -> Vec<TemplateEntry> {
    v.as_array()
        .into_iter()
        .flatten()
        .filter(|c| c.get("type").and_then(Value::as_str).is_none_or(|t| t == "image"))
        .flat_map(|c| c.get("templates").and_then(Value::as_array).cloned().unwrap_or_default())
        .filter(|t| t.get("openSource").and_then(Value::as_bool) != Some(false))
        .filter(|t| t.get("mediaType").and_then(Value::as_str).is_none_or(|m| m == "image"))
        .filter_map(|t| {
            let strs = |k: &str| t.get(k).and_then(Value::as_array).map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_owned)).collect()).unwrap_or_default();
            Some(TemplateEntry {
                name: t.get("name")?.as_str()?.to_owned(),
                title: t.get("title").and_then(Value::as_str).unwrap_or("").to_owned(),
                description: t.get("description").and_then(Value::as_str).unwrap_or("").to_owned(),
                tags: strs("tags"),
                models: strs("models"),
                date: t.get("date").and_then(Value::as_str).unwrap_or("").to_owned(),
                size: t.get("size").and_then(Value::as_u64).unwrap_or(0),
                custom_nodes: strs("requiresCustomNodes"),
            })
        })
        .filter(|t| !t.tags.iter().any(|x| x == "API"))
        .collect()
}

/// The model files a template declares: `properties.models` of every node, including the nodes
/// of subgraph definitions (deduplicated by folder and name).
pub fn template_files(template: &Value) -> Vec<CatalogFile> {
    fn walk(nodes: &Value, out: &mut Vec<CatalogFile>) {
        for n in nodes.as_array().into_iter().flatten() {
            for m in n.get("properties").and_then(|p| p.get("models")).and_then(Value::as_array).into_iter().flatten() {
                let (Some(name), Some(url), Some(dir)) = (m.get("name").and_then(Value::as_str), m.get("url").and_then(Value::as_str), m.get("directory").and_then(Value::as_str))
                else {
                    continue;
                };
                if out.iter().any(|f| f.name == name && f.folder == dir) {
                    continue;
                }
                let sha = m.get("hash").and_then(Value::as_str).filter(|_| m.get("hash_type").and_then(Value::as_str).is_none_or(|t| t.eq_ignore_ascii_case("sha256")));
                out.push(CatalogFile { name: name.to_owned(), folder: dir.to_owned(), url: url.to_owned(), sha256: sha.map(str::to_ascii_lowercase), ..Default::default() });
            }
        }
    }
    let mut out = Vec::new();
    walk(&template["nodes"], &mut out);
    for sg in template.get("definitions").and_then(|d| d.get("subgraphs")).and_then(Value::as_array).into_iter().flatten() {
        walk(&sg["nodes"], &mut out);
    }
    out
}

/// A card for a template: its family from the main model's file name (or none: basic controls).
pub fn template_item(reg: &Registry, e: &TemplateEntry, files: Vec<CatalogFile>, templates_base: &str) -> Item {
    let main = files.iter().find(|f| f.folder == "diffusion_models" || f.folder == "checkpoints" || f.folder == "unet");
    let family = main.and_then(|f| crate::arch::guess_from_name(&f.name, crate::arch::FileKind::DiffusionModel)).filter(|f| reg.family(f).is_some());
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
        ..Default::default()
    }
}

// ------------------------------------------------------------------------------ queries

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
                    item.family = e
                        .models
                        .iter()
                        .chain(std::iter::once(&e.name))
                        .find_map(|m| family_for_label(reg, m).or_else(|| crate::arch::guess_from_name(m, crate::arch::FileKind::DiffusionModel)))
                        .filter(|f| reg.family(f).is_some());
                    out.push(item);
                }
            }
            Source::Installed => {}
        }
    }
    let needle = q.search.trim().to_ascii_lowercase();
    out.retain(|i| needle.is_empty() || i.title.to_ascii_lowercase().contains(&needle) || i.description.to_ascii_lowercase().contains(&needle));
    match &q.view {
        View::Family(f) => out.retain(|i| i.family.as_deref() == Some(f.as_str()) || reg.family(f).is_some_and(|fam| i.family.as_deref().is_some_and(|x| fam.lora_fits(x)))),
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

// ------------------------------------------------------------------------------ network & cache

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

impl HttpNet {
    pub fn new(cfg: Config) -> Self {
        let agent = ureq::Agent::config_builder()
            .timeout_connect(Some(std::time::Duration::from_secs(15)))
            .timeout_global(Some(std::time::Duration::from_secs(60)))
            .max_redirects(4)
            .user_agent(concat!("LocalImage/", env!("CARGO_PKG_VERSION")))
            .build()
            .into();
        Self { cfg, agent }
    }

    fn allowed(&self, url: &str) -> bool {
        let c = &self.cfg;
        let bases = [c.hf.as_str(), c.civitai.as_str(), c.manager_list.as_str(), c.templates.as_str()];
        bases.iter().any(|b| !b.is_empty() && url.starts_with(b.trim_end_matches('/')))
            || url.starts_with("https://image.civitai.com/")
            || crate::download::host_allowed(url)
    }

    fn token(&self, url: &str) -> Option<&str> {
        if url.starts_with(&self.cfg.hf) {
            self.cfg.hf_token.as_deref()
        } else if url.starts_with(&self.cfg.civitai) {
            self.cfg.civitai_token.as_deref()
        } else {
            None
        }
    }
}

impl Net for HttpNet {
    fn get(&self, url: &str) -> Result<Vec<u8>> {
        if !self.allowed(url) {
            bail!("Local Image doesn't read catalogues from {url}");
        }
        let mut req = self.agent.get(url);
        if let Some(t) = self.token(url).filter(|t| !t.is_empty()) {
            req = req.header("Authorization", &format!("Bearer {t}"));
        }
        let mut r = req.call().with_context(|| format!("Could not reach {}", url.split('?').next().unwrap_or(url)))?;
        Ok(r.body_mut().with_config().limit(48 << 20).read_to_vec()?)
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
        crate::settings::write_atomic(&self.path(url), body)
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

// ------------------------------------------------------------------------------ install plans

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

/// Resolves exact sizes and hashes and adds the family's missing text encoders and VAE. Fails
/// (with the reason) for anything Local Image won't install unverified.
pub fn plan(item: &Item, files: &[CatalogFile], fam: Option<&Family>, info: &ObjectInfo, cfg: &Config, net: &dyn Net) -> Result<Plan> {
    let mut want: Vec<CatalogFile> = files.to_vec();
    // The family's components that ComfyUI doesn't have.
    if let Some(f) = fam
        && item.kind != Some(Kind::Lora)
    {
        let have = crate::inventory::resolve_components(f, info);
        for c in &f.components {
            if have.iter().any(|(r, _)| *r == c.role) || want.iter().any(|w| c.names.iter().any(|n| n.eq_ignore_ascii_case(&w.name))) {
                continue;
            }
            if let Some(d) = &c.download {
                want.push(CatalogFile { name: d.name.clone(), folder: d.folder.clone(), url: d.url.clone(), bytes: Some(d.bytes), approx_bytes: None, sha256: Some(d.sha256.clone()) });
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
                let url = format!("{}/api/models/{repo}/tree/main?recursive=true", cfg.hf.trim_end_matches('/'));
                let body = net.get(&url).with_context(|| format!("Could not read the file list of {repo}"))?;
                trees.insert(repo.clone(), parse_hf_tree(&serde_json::from_slice(&body)?));
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
                bail!("{}: the download is {} but the listing says {}; not installing.", f.name, crate::download::human_bytes(e), crate::download::human_bytes(a));
            }
            bytes = exact;
        }
        let (Some(bytes), Some(sha)) = (bytes, sha.filter(|s| s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit()))) else {
            bail!("{} has no published size and SHA-256, so it can't be verified; Local Image doesn't install unverified files.", f.name);
        };
        out.files.push(PlannedFile { name: f.name, folder: f.folder, url: f.url, bytes, sha256: sha.to_ascii_lowercase() });
    }
    Ok(out)
}

/// Downloads a plan into `model_dir/<folder>/<name>` (verified, atomic). Tokens go only to their
/// own hosts.
pub fn install(plan: &Plan, model_dir: &Path, cfg: &Config, ctl: &crate::comfy::JobControl, on_progress: &dyn Fn(u64, u64, &str)) -> Result<()> {
    let total = plan.total();
    let mut done = 0u64;
    for f in &plan.files {
        let dest = model_dir.join(&f.folder).join(&f.name);
        if dest.exists() {
            let spec = crate::family::FileSpec { name: f.name.clone(), bytes: f.bytes, sha256: f.sha256.clone(), ..Default::default() };
            crate::download::verify_existing(&dest, &spec)?;
            done += f.bytes;
            continue;
        }
        let token = if f.url.starts_with(&cfg.hf) {
            cfg.hf_token.as_deref()
        } else if f.url.starts_with(&cfg.civitai) {
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
mod tests {
    use super::*;
    use crate::family::registry;
    use serde_json::json;

    fn hf_list() -> Value {
        json!([
            {"id": "Comfy-Org/z_image_turbo", "author": "Comfy-Org", "likes": 900, "downloads": 45000000, "trendingScore": 80, "createdAt": "2025-11-27T00:00:00Z",
             "tags": ["comfyui", "diffusion-single-file", "base_model:Tongyi-MAI/Z-Image-Turbo", "license:apache-2.0", "text-to-image"],
             "cardData": {"license": "apache-2.0"}, "gated": false,
             "siblings": [{"rfilename": "split_files/diffusion_models/z_image_turbo_bf16.safetensors"}, {"rfilename": "README.md"}]},
            {"id": "someone/qwen-watercolor", "likes": 12, "tags": ["lora", "base_model:adapter:Qwen/Qwen-Image"], "siblings": [{"rfilename": "watercolor.safetensors"}]},
            {"id": "black-forest-labs/FLUX.1-dev", "likes": 12000, "gated": "auto", "tags": ["text-to-image", "license:other"], "cardData": {"license": "other", "license_name": "flux-1-dev-non-commercial-license"},
             "siblings": [{"rfilename": "flux1-dev.safetensors"}, {"rfilename": "ae.safetensors"}]}
        ])
    }

    #[test]
    fn hugging_face_cards() {
        let reg = registry();
        let models = parse_hf_list(reg, "https://huggingface.co", &hf_list(), Kind::Model);
        assert_eq!(models.len(), 2);
        let z = &models[0];
        assert_eq!(z.family.as_deref(), Some("z-image"));
        assert_eq!(z.files[0].folder, "diffusion_models");
        assert_eq!(z.files[0].url, "https://huggingface.co/Comfy-Org/z_image_turbo/resolve/main/split_files/diffusion_models/z_image_turbo_bf16.safetensors");
        assert_eq!(z.files[0].hf_path("https://huggingface.co"), Some(("Comfy-Org/z_image_turbo".into(), "split_files/diffusion_models/z_image_turbo_bf16.safetensors".into())));
        assert_eq!(z.license, "apache-2.0");
        let flux = &models[1];
        assert!(flux.gated);
        assert_eq!(flux.family.as_deref(), Some("flux1"));
        assert_eq!(flux.license, "flux-1-dev-non-commercial-license");
        let loras = parse_hf_list(reg, "https://huggingface.co", &hf_list(), Kind::Lora);
        assert_eq!(loras.len(), 1);
        assert_eq!(loras[0].files[0].folder, "loras");
        assert_eq!(loras[0].family.as_deref(), Some("qwen-image"));
    }

    fn civitai() -> Value {
        json!({"items": [
            {"id": 101, "name": "Juggernaut XL", "type": "Checkpoint", "nsfw": false, "allowCommercialUse": ["Image", "RentCivit"],
             "creator": {"username": "KandooAI"}, "stats": {"downloadCount": 900000, "thumbsUpCount": 30000}, "tags": ["photorealistic"],
             "modelVersions": [{"id": 5, "baseModel": "SDXL 1.0", "publishedAt": "2025-01-01",
                "files": [{"name": "juggernautXL_v9.safetensors", "sizeKB": 6775430.0, "type": "Model", "hashes": {"SHA256": "AA11AA11AA11AA11AA11AA11AA11AA11AA11AA11AA11AA11AA11AA11AA11AA11"}, "downloadUrl": "https://civitai.com/api/download/models/5"}],
                "images": [{"url": "https://image.civitai.com/x/nsfw.jpeg", "nsfwLevel": 8}, {"url": "https://image.civitai.com/x/ok.jpeg", "nsfwLevel": 1}]}]},
            {"id": 102, "name": "Pony style", "type": "LORA", "nsfw": false, "modelVersions": [{"id": 6, "baseModel": "Pony", "files": [{"name": "style.safetensors", "sizeKB": 100.0, "hashes": {"SHA256": "BB"}, "downloadUrl": "https://civitai.com/api/download/models/6"}]}]},
            {"id": 103, "name": "Hidden", "type": "Checkpoint", "nsfw": true, "modelVersions": [{"id": 7, "baseModel": "SD 1.5", "files": [{"name": "x.safetensors", "downloadUrl": "https://civitai.com/api/download/models/7"}]}]},
            {"id": 104, "name": "Pickle", "type": "Checkpoint", "modelVersions": [{"id": 8, "baseModel": "SD 1.5", "files": [{"name": "x.ckpt", "downloadUrl": "https://civitai.com/api/download/models/8"}]}]}
        ]})
    }

    #[test]
    fn civitai_cards_skip_nsfw_and_pickles() {
        let items = parse_civitai(registry(), &civitai());
        assert_eq!(items.len(), 2);
        let j = &items[0];
        assert_eq!(j.family.as_deref(), Some("sdxl"));
        assert_eq!(j.preview.as_deref(), Some("https://image.civitai.com/x/ok.jpeg"));
        assert_eq!(j.files[0].folder, "checkpoints");
        assert_eq!(j.files[0].sha256.as_deref().map(str::len), Some(64));
        assert_eq!(j.commercial, Some(true));
        assert_eq!(items[1].family.as_deref(), Some("pony"));
        assert_eq!(items[1].kind, Some(Kind::Lora));
    }

    #[test]
    fn templates_list_their_files_including_subgraphs() {
        let t = json!({
            "nodes": [{"id": 1, "type": "SaveImage", "properties": {}}],
            "definitions": {"subgraphs": [{"nodes": [
                {"type": "UNETLoader", "properties": {"models": [{"name": "qwen_image_fp8_e4m3fn.safetensors", "url": "https://huggingface.co/Comfy-Org/Qwen-Image_ComfyUI/resolve/main/split_files/diffusion_models/qwen_image_fp8_e4m3fn.safetensors", "directory": "diffusion_models"}]}},
                {"type": "VAELoader", "properties": {"models": [{"name": "qwen_image_vae.safetensors", "url": "https://huggingface.co/Comfy-Org/Qwen-Image_ComfyUI/resolve/main/split_files/vae/qwen_image_vae.safetensors", "directory": "vae", "hash": "CC", "hash_type": "SHA256"}]}}
            ]}]}
        });
        let files = template_files(&t);
        assert_eq!(files.len(), 2);
        assert_eq!(files[1].sha256.as_deref(), Some("cc"));
        let e = TemplateEntry { name: "image_qwen_image".into(), title: "Qwen-Image: Text to Image".into(), size: 30_000_000_000, ..Default::default() };
        let item = template_item(registry(), &e, files, "https://example.org/templates");
        assert_eq!(item.family.as_deref(), Some("qwen-image"));
        assert!(!item.basic_controls());
        assert_eq!(item.preview.as_deref(), Some("https://example.org/templates/image_qwen_image-1.webp"));
        let unknown = TemplateEntry { name: "image_omnigen2_t2i".into(), ..Default::default() };
        let files = vec![CatalogFile { name: "omnigen2_fp16.safetensors".into(), folder: "diffusion_models".into(), ..Default::default() }];
        assert!(template_item(registry(), &unknown, files, "x").basic_controls());
    }

    #[test]
    fn real_template_index_parses() {
        // A trimmed copy of the official index.json shape.
        let idx = json!([
            {"moduleName": "default", "category": "Foundation", "title": "Image", "type": "image", "templates": [
                {"name": "image_qwen_image", "title": "Qwen-Image: Text to Image", "mediaType": "image", "tags": ["Image", "Text to Image"], "models": ["Qwen-Image"], "date": "2025-08-05", "size": 31782757990u64, "openSource": true},
                {"name": "api_flux2", "title": "API", "mediaType": "image", "tags": ["API"], "openSource": false}
            ]},
            {"moduleName": "default", "category": "Foundation", "title": "Video", "type": "video", "templates": [{"name": "video_wan", "mediaType": "image"}]}
        ]);
        let t = parse_template_index(&idx);
        assert_eq!(t.len(), 1);
        assert_eq!(t[0].size, 31782757990);
    }

    #[test]
    fn manager_list_maps_types_bases_and_sizes() {
        let v = json!({"models": [
            {"name": "Qwen-Image Diffusion Model (bf16)", "type": "diffusion_model", "base": "Qwen-Image", "save_path": "diffusion_models/qwen-image", "description": "d", "reference": "https://huggingface.co/Comfy-Org/Qwen-Image_ComfyUI", "filename": "qwen_image_bf16.safetensors", "url": "https://huggingface.co/Comfy-Org/Qwen-Image_ComfyUI/resolve/main/split_files/diffusion_models/qwen_image_bf16.safetensors", "size": "9.78GB"},
            {"name": "RealESRGAN x2", "type": "upscale", "base": "upscale", "save_path": "default", "filename": "RealESRGAN_x2.pth", "url": "https://x/RealESRGAN_x2.pth", "size": "67.1MB"}
        ]});
        let items = parse_manager(registry(), &v);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].family.as_deref(), Some("qwen-image"));
        assert_eq!(items[0].files[0].folder, "diffusion_models/qwen-image");
        assert_eq!(items[0].files[0].approx_bytes, Some(9_780_000_000));
    }

    struct FakeNet(BTreeMap<String, Vec<u8>>, Option<u64>);
    impl Net for FakeNet {
        fn get(&self, url: &str) -> Result<Vec<u8>> {
            self.0.get(url).cloned().with_context(|| format!("404 {url}"))
        }
        fn head_size(&self, _url: &str) -> Result<Option<u64>> {
            Ok(self.1)
        }
    }

    #[test]
    fn plans_verify_add_components_and_refuse_unverified() {
        let reg = registry();
        let cfg = Config::default();
        let info = ObjectInfo(json!({}));
        let sha = "a".repeat(64);
        let tree = json!([
            {"type": "file", "path": "split_files/diffusion_models/z_image_turbo_bf16.safetensors", "size": 12309866400u64, "lfs": {"oid": sha, "size": 12309866400u64}},
        ]);
        let net = FakeNet([("https://huggingface.co/api/models/Comfy-Org/z_image_turbo/tree/main?recursive=true".to_owned(), serde_json::to_vec(&tree).unwrap())].into(), None);
        let items = parse_hf_list(reg, &cfg.hf, &hf_list(), Kind::Model);
        let z = &items[0];
        let p = plan(z, &z.files, reg.family("z-image"), &info, &cfg, &net);
        // Z-Image's text encoder and VAE have no pinned download or source in its profile: only
        // the model is planned.
        let p = p.unwrap();
        assert_eq!(p.files[0].bytes, 12309866400);
        assert_eq!(p.files[0].sha256, sha);
        // Civitai: hash published, size from HEAD (must agree with the listing).
        let civ = parse_civitai(reg, &civitai());
        let j = &civ[0];
        let ok = FakeNet(BTreeMap::new(), Some((6775430.0f64 * 1024.0) as u64));
        let p = plan(j, &j.files, reg.family("sdxl"), &info, &cfg, &ok).unwrap();
        assert_eq!(p.files.len(), 1);
        let wrong = FakeNet(BTreeMap::new(), Some(10));
        assert!(plan(j, &j.files, reg.family("sdxl"), &info, &cfg, &wrong).is_err());
        // No hash: refused.
        let bad = CatalogFile { name: "x.safetensors".into(), folder: "checkpoints".into(), url: "https://civitai.com/api/download/models/9".into(), ..Default::default() };
        let e = plan(j, &[bad], None, &info, &cfg, &ok).unwrap_err().to_string();
        assert!(e.contains("no published size and SHA-256"), "{e}");
        // Already installed: skipped.
        let have = ObjectInfo(json!({"CheckpointLoaderSimple": {"input": {"required": {"ckpt_name": [["juggernautXL_v9.safetensors"]]}}}}));
        let p = plan(j, &j.files, reg.family("sdxl"), &have, &cfg, &ok).unwrap();
        assert!(p.files.is_empty() && p.present == vec!["juggernautXL_v9.safetensors"]);
        // FLUX.1 adds its text encoders and VAE from their Hugging Face sources.
        let flux_item = Item { id: "x".into(), kind: Some(Kind::Model), family: Some("flux1".into()), ..Default::default() };
        let e = plan(&flux_item, &[], reg.family("flux1"), &info, &cfg, &FakeNet(BTreeMap::new(), None)).unwrap_err().to_string();
        assert!(e.contains("comfyanonymous/flux_text_encoders"), "{e}");
    }

    #[test]
    fn queries_build_urls_and_filter() {
        let reg = registry();
        let cfg = Config::default();
        let q = Query { view: View::Family("pony".into()), kind: Kind::Lora, search: "ink style".into(), sources: vec![] };
        let u = urls(&cfg, reg, &q);
        assert!(u.iter().any(|(s, u)| *s == Source::Civitai && u.contains("baseModels=Pony") && u.contains("types=LORA") && u.contains("query=ink%20style")));
        assert!(u.iter().any(|(s, u)| *s == Source::HuggingFace && u.contains("filter=lora")));
        assert!(!u.iter().any(|(s, _)| *s == Source::Template));
        let q = Query { view: View::FitsGpu(12.0), kind: Kind::Model, search: String::new(), sources: vec![Source::Civitai] };
        let bodies = vec![(Source::Civitai, civitai())];
        let all = items(reg, &cfg, &q, &bodies);
        assert!(all.iter().all(|i| i.vram_gb().unwrap() <= 12.0));
        let cache = Cache::new(std::env::temp_dir().join(format!("li-cat-{}", uuid::Uuid::new_v4().simple())));
        let net = FakeNet([("https://x/a".to_owned(), b"[1]".to_vec())].into(), None);
        assert_eq!(cache.fetch(&net, "https://x/a", true).unwrap(), (b"[1]".to_vec(), false));
        let offline = FakeNet(BTreeMap::new(), None);
        assert_eq!(cache.fetch(&offline, "https://x/a", true).unwrap(), (b"[1]".to_vec(), true));
        assert!(cache.fetch(&offline, "https://x/b", false).is_err());
        let _ = std::fs::remove_dir_all(&cache.dir);
    }

    /// The whole flow against the mock server: browse every source, install a Hugging Face
    /// model, a Civitai checkpoint (through its CDN redirect) and an official template, refuse the
    /// unverified and gated ones, and see the installs in ComfyUI's refreshed listing.
    #[test]
    fn browse_and_install_against_the_mock() {
        let mock = crate::mock::MockComfy::start().unwrap();
        let host = mock.host().to_owned();
        let models = std::env::temp_dir().join(format!("li-browser-{}", uuid::Uuid::new_v4().simple()));
        mock.set_model_dir(&models);
        let cfg = Config {
            hf: format!("http://{host}/hf"),
            civitai: format!("http://{host}/civitai"),
            manager_list: format!("http://{host}/manager/model-list.json"),
            templates: format!("http://{host}/templates"),
            hf_token: None,
            civitai_token: None,
        };
        let net = HttpNet::new(cfg.clone());
        let reg = registry();
        let info = || ObjectInfo(serde_json::from_slice(&ureq::get(&format!("http://{host}/object_info")).call().unwrap().body_mut().read_to_vec().unwrap()).unwrap());
        let cache = Cache::new(models.join(".cache"));
        let fetch = |q: &Query| {
            let bodies: Vec<(Source, Value)> =
                urls(&cfg, reg, q).into_iter().map(|(s, u)| (s, serde_json::from_slice(&cache.fetch(&net, &u, true).unwrap().0).unwrap())).collect();
            items(reg, &cfg, q, &bodies)
        };
        let trending = fetch(&Query { view: View::Trending, kind: Kind::Model, search: String::new(), sources: vec![] });
        let titles: Vec<&str> = trending.iter().map(|i| i.title.as_str()).collect();
        assert!(titles.contains(&"z-image-turbo-mini") && titles.contains(&"Mockernaut XL") && titles.contains(&"Qwen-Image: Text to Image"), "{titles:?}");
        assert!(!titles.contains(&"Hidden"));
        let pony_loras = fetch(&Query { view: View::Family("pony".into()), kind: Kind::Lora, search: String::new(), sources: vec![] });
        assert_eq!(pony_loras.iter().map(|i| i.title.as_str()).collect::<Vec<_>>(), vec!["Ink Wash (Pony)"]);

        let ctl = crate::comfy::JobControl::new();
        let install_item = |item: &Item, fam: Option<&Family>| -> Result<Plan> {
            let p = plan(item, &item.files, fam, &info(), &cfg, &net)?;
            install(&p, &models, &cfg, &ctl, &|_, _, _| {})?;
            Ok(p)
        };
        // Hugging Face: size and hash from the repo tree.
        let z = trending.iter().find(|i| i.title == "z-image-turbo-mini").unwrap();
        let p = install_item(z, reg.family("z-image")).unwrap();
        assert!(p.files.iter().any(|f| f.name == "z_image_turbo_mini.safetensors"));
        assert!(models.join("diffusion_models/z_image_turbo_mini.safetensors").exists());
        assert!(info().find_file("UNETLoader", "unet_name", "z_image_turbo_mini.safetensors").is_some());
        // Civitai: published hash, size from the download's headers, CDN redirect.
        let civ = trending.iter().find(|i| i.title == "Mockernaut XL").unwrap();
        install_item(civ, None).unwrap();
        assert!(models.join("checkpoints/mockernautXL_v1.safetensors").exists());
        assert!(plan(civ, &civ.files, None, &info(), &cfg, &net).unwrap().files.is_empty(), "installed files are skipped");
        // No published hash: refused before anything downloads.
        let fresh = fetch(&Query { view: View::New, kind: Kind::Model, search: "unverified".into(), sources: vec![Source::Civitai] });
        let e = install_item(&fresh[0], None).unwrap_err().to_string();
        assert!(e.contains("no published size and SHA-256"), "{e}");
        assert!(!models.join("checkpoints/unverified_mix.safetensors").exists());
        // Gated: the licence must be accepted on the publisher's page.
        let gated = trending.iter().find(|i| i.gated).unwrap();
        let e = install_item(gated, None).unwrap_err().to_string();
        assert!(e.contains("accept its licence"), "{e}");
        // An official template for an unknown family: its files, then basic controls.
        let (ui, files) = fetch_template(&cfg, &net, "image_omnigen2_t2i").unwrap();
        assert!(!files.is_empty());
        let item = Item { id: "template:image_omnigen2_t2i".into(), template: Some("image_omnigen2_t2i".into()), ..Default::default() };
        let p = plan(&item, &files, None, &info(), &cfg, &net).unwrap();
        install(&p, &models, &cfg, &ctl, &|_, _, _| {}).unwrap();
        for f in &p.files {
            assert!(models.join(&f.folder).join(&f.name).exists(), "{}", f.name);
        }
        let w = crate::custom::from_template("OmniGen2", &ui, &info()).unwrap();
        assert!(w.basic && w.has(&crate::custom::FieldKind::Prompt));
        // Offline: the cached catalogue still answers.
        let q = Query { view: View::Trending, kind: Kind::Model, search: String::new(), sources: vec![Source::HuggingFace] };
        let (_, u0) = urls(&cfg, reg, &q).remove(0);
        drop(mock);
        assert!(cache.fetch(&net, &u0, true).unwrap().1, "served from the cache");
        let _ = std::fs::remove_dir_all(&models);
    }
}
