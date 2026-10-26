//! Model setup: makes every main model file ComfyUI lists usable, or says exactly why not.
//! A scan reads the server ([`Snapshot`]: version, GPU, `/object_info`), the server's own
//! official templates ([`Templates`]) and the model folder; identifies each file by its weights;
//! keeps curated and buildable models on their builders; matches the rest to official templates
//! and compiles their recipes ([`crate::template`]); checks them statically ([`check`]); stores
//! the result per server ([`state`]) and publishes the usable recipes to the catalogue.
//! Verification ([`verify`]) runs only when asked. Everything here is blocking.

mod background;
pub mod check;
pub mod fixes;
pub mod issue;
pub mod report;
pub mod state;
pub mod summary;
pub mod verify;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result};
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::arch::FileKind;
use crate::catalog::{InstalledModel, ModelId, TemplateModel};
use crate::comfy::{ComfyClient, JobControl, ObjectInfo};
use crate::companions::Runtime;
use crate::family::{Family, Registry, Role};
use crate::identify::Identity;
use crate::template::TemplateRecipe;
use crate::template::compile::{self, Source};
use crate::template::index::{self as tindex, DeclaredFile, IndexSource, TemplateEntry, Version};
use crate::template::matching::{self, Candidate, Query};
use issue::{Fix, Issue};
use report::{BindingReport, CapabilityReport, FileReport, LicenceReport, LoraReport, ModelReport, Report, ServerReport};
use state::{Fingerprint, ModelState, ServerState, SetupState, Status};

/// ComfyUI is running someone else's work; nothing was tested or recorded.
#[derive(Debug)]
pub struct Busy;

impl std::fmt::Display for Busy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(issue::CATALOGUE.iter().find(|(c, _, _)| *c == "Busy").map(|(_, _, t)| *t).unwrap_or("ComfyUI is busy."))
    }
}

impl std::error::Error for Busy {}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Gpu {
    pub name: String,
    pub vram_total: u64,
}

/// What the connected ComfyUI is.
#[derive(Clone, Debug, Default)]
pub struct Snapshot {
    pub host: String,
    pub remote: bool,
    pub comfyui_version: Option<Version>,
    /// As reported (`0.42.5`, `mock`).
    pub version_text: String,
    pub templates_version: String,
    pub required_templates_version: String,
    pub deploy_environment: String,
    pub gpu: Option<Gpu>,
    pub info: ObjectInfo,
}

/// Whether `host` is this computer.
pub fn is_local(host: &str) -> bool {
    let h = host.rsplit_once(':').map(|(h, _)| h).unwrap_or(host);
    matches!(h, "127.0.0.1" | "localhost" | "[::1]" | "::1") || h.starts_with("127.")
}

/// The ComfyUI version `/system_stats` reports, as setup stores it (`0.42.5`, or the text it gave).
pub fn version_of(stats: &Value) -> String {
    let text = stats["system"]["comfyui_version"].as_str().unwrap_or("");
    Version::parse(text).map_or_else(|| text.to_owned(), |v| v.to_string())
}

impl Snapshot {
    pub fn from_parts(host: &str, stats: &Value, info: ObjectInfo) -> Self {
        let sys = &stats["system"];
        let text = |k: &str| sys[k].as_str().unwrap_or("").to_owned();
        let gpu = crate::setup::gpus_from_stats(stats).into_iter().next().map(|g| Gpu { name: g.name, vram_total: g.vram_total });
        Self {
            host: host.to_owned(),
            remote: !is_local(host),
            comfyui_version: Version::parse(&text("comfyui_version")),
            version_text: text("comfyui_version"),
            templates_version: text("installed_templates_version"),
            required_templates_version: text("required_templates_version"),
            deploy_environment: text("deploy_environment"),
            gpu,
            info,
        }
    }

    pub fn take(client: &ComfyClient) -> Result<Self> {
        let stats = client.system_stats()?;
        let info = client.object_info()?;
        Ok(Self::from_parts(client.host(), &stats, info))
    }

    /// The version for messages (`0.42.5`, or what the server reported).
    pub fn version_label(&self) -> String {
        match (self.comfyui_version, self.version_text.is_empty()) {
            (Some(v), _) => v.to_string(),
            (None, false) => self.version_text.clone(),
            (None, true) => "unknown".into(),
        }
    }
}

/// The official image templates of one template package version.
#[derive(Clone, Debug, Default)]
pub struct Templates {
    pub version: String,
    pub source: IndexSource,
    pub entries: Vec<TemplateEntry>,
    pub declared: BTreeMap<String, Vec<DeclaredFile>>,
    bodies: BTreeMap<String, Arc<Vec<u8>>>,
}

impl Templates {
    fn cacheable_version(version: &str) -> bool {
        !version.is_empty() && version.bytes().all(|b| b.is_ascii_alphanumeric() || b".-_".contains(&b))
    }

    /// Builds the set from an `index.json` and a reader for `<name>.json` (unreadable, oversized
    /// or invalid templates are left out, logged).
    pub fn from_reader(version: &str, index: &[u8], read: &dyn Fn(&str) -> Option<Vec<u8>>) -> Result<Self> {
        let v: Value = serde_json::from_slice(index).context("the template index is not valid JSON")?;
        let mut t = Self { version: version.to_owned(), ..Default::default() };
        for e in tindex::parse_template_index(&v) {
            if !tindex::valid_name(&e.name) {
                continue;
            }
            let Some(bytes) = read(&format!("{}.json", e.name)) else { continue };
            let Ok(body) = serde_json::from_slice::<Value>(&bytes) else {
                log::warn!("template {} is not valid JSON", e.name);
                continue;
            };
            if let Err(why) = tindex::check_size(&bytes, &body) {
                log::warn!("template {} skipped: {why}", e.name);
                continue;
            }
            t.declared.insert(e.name.clone(), tindex::declared_files(&e.name, &body));
            t.bodies.insert(e.name.clone(), Arc::new(bytes));
            t.entries.push(e);
        }
        Ok(t)
    }

    /// A template folder (`index.json` + bodies).
    pub fn from_dir(dir: &Path, version: &str) -> Result<Self> {
        let index = std::fs::read(dir.join("index.json")).with_context(|| format!("no index.json in {}", dir.display()))?;
        Self::from_reader(version, &index, &|f| std::fs::read(dir.join(f)).ok())
    }

    /// The connected server's templates (`/templates/…`), cached per host and package version.
    pub fn fetch(client: &ComfyClient, version: &str, cache: Option<&Path>) -> Result<Self> {
        let dir = cache
            .filter(|_| Self::cacheable_version(version))
            .map(|c| c.join(hex::encode(Sha256::digest(client.host().trim_end_matches('/').as_bytes()))).join(version));
        let cached = |f: &str| dir.as_ref().and_then(|d| std::fs::read(d.join(f)).ok());
        let get = |f: &str| -> Option<Vec<u8>> {
            if let Some(b) = cached(f) {
                return Some(b);
            }
            let b = client.template_file(f).ok()?;
            if let Some(d) = &dir
                && std::fs::create_dir_all(d).is_ok()
                && let Err(e) = li_fs::atomic_write(&d.join(f), &b)
            {
                log::warn!("template cache: {e:#}");
            }
            Some(b)
        };
        let index = get("index.json").context("ComfyUI doesn't serve its template index (/templates/index.json)")?;
        Self::from_reader(version, &index, &get)
    }

    pub fn body(&self, name: &str) -> Option<Value> {
        serde_json::from_slice(self.bodies.get(name)?).ok()
    }
}

/// What a scan needs besides the server.
pub struct Options {
    pub host: String,
    /// The local model folder (headers are read from it), when the server is on this computer.
    pub model_dir: Option<PathBuf>,
    pub state_path: PathBuf,
    /// Where template packages are cached (`None`: not cached).
    pub cache_dir: Option<PathBuf>,
    /// Read templates from this folder instead of the server (tests, `--templates-dir`).
    pub templates_dir: Option<PathBuf>,
}

impl Options {
    /// The app's own state and caches for `host`.
    pub fn for_host(host: &str, model_dir: Option<PathBuf>) -> Self {
        Self {
            host: host.to_owned(),
            model_dir,
            state_path: state::default_path(),
            cache_dir: Some(crate::settings::state_dir().join("templates")),
            templates_dir: None,
        }
    }
}

/// An official image template a model can be set to run through.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TemplateChoice {
    pub name: String,
    pub title: String,
}

/// The result of a scan.
#[derive(Clone, Debug, Default)]
pub struct Scan {
    pub snapshot: Option<Snapshot>,
    pub server: ServerState,
    pub issues: Vec<Issue>,
    pub loras: Vec<LoraReport>,
    pub other_files: Vec<FileReport>,
    /// The server's official image templates, by title.
    pub templates: Vec<TemplateChoice>,
}

fn file_stamp(path: &Path) -> Fingerprint {
    let Ok(meta) = std::fs::metadata(path) else { return Fingerprint::default() };
    let mtime_ns = meta.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_nanos() as u64).unwrap_or(0);
    Fingerprint { bytes: meta.len(), mtime_ns, header_sha256: header_sha256(path) }
}

/// SHA-256 of a safetensors header (its 8-byte length and JSON).
fn header_sha256(path: &Path) -> Option<String> {
    use std::io::Read;
    let mut f = std::fs::File::open(path).ok()?;
    let mut len = [0u8; 8];
    f.read_exact(&mut len).ok()?;
    let n = u64::from_le_bytes(len);
    if n == 0 || n > 100 << 20 {
        return None;
    }
    let mut json = vec![0u8; n as usize];
    f.read_exact(&mut json).ok()?;
    let mut h = Sha256::new();
    h.update(len);
    h.update(&json);
    Some(hex::encode(h.finalize()))
}

/// Latent channels of a local VAE (from `decoder.conv_in.weight`).
fn vae_channels(model_dir: Option<&Path>, name: &str) -> Option<u32> {
    let path = crate::inventory::local_path(model_dir, &["vae"], name)?;
    let h = crate::arch::read_header(&path).ok()?;
    h.find_suffix("decoder.conv_in.weight").and_then(|t| t.shape.get(1)).map(|c| *c as u32)
}

/// Fingerprint every executable branch, resolved file binding and derivation setting.
/// Source/package metadata alone does not change what a render executes.
pub fn recipe_sha256(r: &TemplateRecipe) -> String {
    fn executable(g: &mut crate::template::TaskGraph) {
        g.template.clear();
        for binding in &mut g.files {
            binding.why = crate::companions::Why::default();
            binding.url.clear();
            binding.found_in = None;
            binding.folder.clear();
        }
        if let Some(enhanced) = &mut g.enhanced {
            executable(enhanced);
        }
    }
    let mut normalized = r.clone();
    normalized.sources.clear();
    normalized.skipped.clear();
    normalized.polish = None;
    normalized.upsampler = None;
    for graph in [&mut normalized.create, &mut normalized.edit, &mut normalized.style].into_iter().flatten() {
        executable(graph);
    }
    let value = serde_json::to_value(normalized).unwrap_or(Value::Null);
    hex::encode(Sha256::digest(crate::graph::canonical_bytes(&value)))
}

fn model_recipe_sha256(m: &ModelState) -> String {
    if let Some(recipe) = &m.recipe {
        return recipe_sha256(recipe);
    }
    m.universal_recipe
        .as_ref()
        .map(|r| hex::encode(Sha256::digest(crate::graph::canonical_bytes(&serde_json::to_value(r).unwrap_or(Value::Null)))))
        .unwrap_or_default()
}

/// Display a variant as part of the name and precision once, after it.
fn label_for(family: Option<&Family>, entry_model: Option<&str>, id: &Identity, file: &str) -> String {
    let mut base = family.map(|f| f.label.clone()).filter(|l| !l.is_empty()).or_else(|| entry_model.map(str::to_owned)).unwrap_or_else(|| {
        let stem = crate::stem::stem_key(file).display;
        display_words(&if stem.is_empty() { crate::inventory::label_for(file) } else { stem })
    });
    if let Some(v) = id
        .variant
        .as_deref()
        .filter(|v| !matches!(v.to_ascii_lowercase().as_str(), "bf16" | "fp16" | "fp32" | "fp8" | "int8" | "full precision" | "fp8_scaled" | "int8_convrot"))
    {
        let variant = display_words(v);
        if !base.to_lowercase().ends_with(&variant.to_lowercase()) {
            base.push(' ');
            base.push_str(&variant);
        }
    }
    if let Some(q) = &id.quant.class {
        base.push_str(" · ");
        base.push_str(&q.label());
    }
    base
}

fn display_words(text: &str) -> String {
    text.replace(['_', '-'], " ")
        .split_whitespace()
        .map(|w| match w.to_ascii_lowercase().as_str() {
            "sft" => "SFT".into(),
            "lora" => "LoRA".into(),
            "vae" => "VAE".into(),
            _ => {
                let mut chars = w.chars();
                chars.next().map(|c| c.to_uppercase().collect::<String>() + chars.as_str()).unwrap_or_default()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

struct Ctx<'a> {
    snapshot: &'a Snapshot,
    templates: Option<&'a Templates>,
    model_dir: Option<&'a Path>,
    reg: &'a Registry,
    runtime: &'a Runtime,
    stored: Option<&'a ServerState>,
    now: String,
}

fn main_role(key: &str) -> (Role, FileKind, &'static [&'static str]) {
    if key.starts_with("ckpt:") {
        (Role::Checkpoint, FileKind::Checkpoint, &["checkpoints"])
    } else {
        (Role::Unet, FileKind::DiffusionModel, &["diffusion_models", "unet", "unet_gguf"])
    }
}

/// The template model for one installed file (matching, compiling, checking).
fn checked_recipe(ctx: &Ctx, key: &str, m: &ModelState, id: &Identity, recipe: &TemplateRecipe) -> check::Checked {
    let local =
        |folder: &str, file: &str| crate::inventory::local_path(ctx.model_dir, &[folder], file).and_then(|p| std::fs::metadata(p).ok()).map(|m| m.len());
    check::check(&check::ModelCheck {
        key,
        label: &m.label,
        file: &m.file,
        bytes: (m.fingerprint.bytes > 0).then_some(m.fingerprint.bytes),
        identity: id,
        recipe,
        family: id.arch.as_deref().and_then(|a| ctx.reg.family(a)),
        server: ctx.snapshot,
        runtime: ctx.runtime,
        local_bytes: &local,
        try_anyway: m.user.try_anyway,
    })
}

fn universal_model(ctx: &Ctx, mut m: ModelState) -> ModelState {
    m.engine = "universal".into();
    let metadata = crate::inventory::local_path(ctx.model_dir, &["checkpoints"], &m.file)
        .and_then(|p| crate::arch::read_header(&p).ok())
        .map(|h| h.metadata)
        .unwrap_or_default();
    match crate::refine::universal(&m.file, &ctx.snapshot.info, &metadata) {
        Ok(recipe) => {
            m.universal_recipe = Some(recipe);
            m.status = Status::Builtin;
            m.capabilities = ["refine", "upscale"]
                .into_iter()
                .map(|name| check::CapabilityCheck {
                    name: name.into(),
                    ok: true,
                    references: 0,
                    detail: if name == "upscale" { "uses Refine".into() } else { String::new() },
                })
                .collect();
        }
        Err(e) => {
            m.status = Status::Blocked;
            m.issues.push(Issue::new("Unavailable", Some(&m.catalog_id), &[("reason", e.to_string())]));
        }
    }
    m
}

fn restore_last_good(ctx: &Ctx, key: &str, m: &mut ModelState) {
    if m.recipe.is_some() || m.user.ignored || matches!(m.engine.as_str(), "builtin" | "builder") {
        return;
    }
    let Some(stored) = ctx.stored.and_then(|s| s.models.get(key)) else { return };
    let Some(verified) = stored.verified.as_ref().filter(|v| v.capabilities.values().any(|c| c.pass) && !v.capabilities.get("create").is_some_and(|c| !c.pass))
    else {
        return;
    };
    let Some(old) = stored.recipe.as_ref().or(stored.last_good.as_ref()) else { return };
    let Some(id) = m.identity.as_ref() else { return };
    let checked = checked_recipe(ctx, key, m, id, old);
    if checked.blocked() || !checked.capabilities.iter().any(|c| c.ok && matches!(c.name.as_str(), "create" | "edit" | "style")) {
        return;
    }
    let reason = m.issues.first().map(Issue::english).unwrap_or_default();
    m.issues = checked.issues;
    m.issues.push(
        Issue::new("UsingLastGood", Some(key), &[("old", verified.comfyui_version.clone()), ("reason", reason)]).fix(Fix::Verify { model: m.file.clone() }),
    );
    m.bindings = old.bindings().into_iter().cloned().collect();
    m.capabilities = checked.capabilities;
    m.templates = old.sources.iter().map(|source| (source.capability.clone(), source.name.clone())).collect();
    m.last_good = Some(old.clone());
    m.recipe = Some(old.clone());
    m.universal_recipe = None;
    m.engine = "template".into();
    m.status = Status::NeedsReverify;
}

fn template_model(ctx: &Ctx, im: &InstalledModel, name: &str, id: &Identity, mut m: ModelState) -> ModelState {
    let key = im.key.as_str();
    let family = id.arch.as_deref().and_then(|a| ctx.reg.family(a));
    let polish = family.and_then(|f| f.template.as_ref());
    m.label = label_for(family, None, id, name);
    let Some(templates) = ctx.templates else {
        if im.files.contains_key(&Role::Checkpoint) {
            return universal_model(ctx, m);
        }
        m.status = Status::Unmatched;
        m.engine = "unsupported".into();
        m.issues.push(Issue::new("TemplateIndexUnavailable", Some(key), &[]).fix(Fix::CheckOnline));
        return m;
    };
    let foreign_pins: Vec<String> = ctx
        .reg
        .families
        .iter()
        .filter(|f| Some(f.id.as_str()) != id.arch.as_deref())
        .filter_map(|f| f.template.as_ref())
        .flat_map(|p| p.templates.values().flatten().cloned())
        .collect();
    let twins: Vec<String> = ctx
        .stored
        .into_iter()
        .flat_map(|s| s.models.values())
        .filter(|o| o.status == Status::Verified && id.signature.is_some() && o.identity.as_ref().and_then(|i| i.signature.as_ref()) == id.signature.as_ref())
        .flat_map(|o| o.templates.values().cloned())
        .collect();
    let candidates: Vec<Candidate> = templates.entries.iter().map(|e| Candidate { entry: e, declared: &templates.declared[&e.name] }).collect();
    let q = Query {
        file: name,
        identity: id,
        polish,
        foreign_pins: &foreign_pins,
        twins: &twins,
        manual: m.user.template_choice.as_deref(),
        server: ctx.snapshot.comfyui_version,
    };
    let matches = matching::find(&q, &candidates);
    let best = matching::best(&matches);
    let entry_model = best.first().and_then(|b| templates.entries.iter().find(|e| e.name == b.template)).and_then(|e| e.models.first().cloned());
    m.label = label_for(family, entry_model.as_deref(), id, name);
    if best.is_empty() {
        if im.files.contains_key(&Role::Checkpoint) {
            return universal_model(ctx, m);
        }
        m.engine = "unsupported".into();
        m.status = Status::Unmatched;
        let issue = match &id.vetoed {
            Some(generation) => Issue::new("UnsupportedGeneration", Some(key), &[("file", name.to_owned()), ("generation", generation.clone())]),
            None => Issue::new("NoTemplate", Some(key), &[("file", name.to_owned())]),
        };
        m.issues.push(issue.fix(Fix::CheckOnline).fix(Fix::UpdateProfiles));
        return m;
    }
    let bodies: Vec<(Value, &TemplateEntry, &matching::Match)> =
        best.iter().filter_map(|b| Some((templates.body(&b.template)?, templates.entries.iter().find(|e| e.name == b.template)?, *b))).collect();
    let sources: Vec<Source> = bodies.iter().map(|(body, entry, matched)| Source { entry, body, matched, templates_version: &templates.version }).collect();
    let dir = ctx.model_dir;
    let gpu = ctx.snapshot.gpu.as_ref().map(|g| g.name.clone());
    let input = compile::Input {
        key,
        file: name,
        identity: id,
        info: &ctx.snapshot.info,
        runtime: ctx.runtime,
        family,
        gpu: gpu.as_deref(),
        user: &m.user.substitutions,
        vae_channels: &|n| vae_channels(dir, n),
    };
    m.engine = "template".into();
    m.templates = best.iter().map(|b| (b.capability.id().to_owned(), b.template.clone())).collect();
    let recipe = match compile::compile(&input, &sources) {
        Ok(r) => r,
        Err(unusable) => TemplateRecipe { model_key: key.to_owned(), skipped: unusable, ..Default::default() },
    };
    let checked = checked_recipe(ctx, key, &m, id, &recipe);
    m.bindings = recipe.bindings().into_iter().cloned().collect();
    m.issues.extend(checked.issues.iter().cloned());
    let usable = recipe.create.is_some() || recipe.edit.is_some() || recipe.style.is_some();
    m.status = if checked.blocked() || !usable { Status::Blocked } else { Status::Ready };
    if m.status == Status::Ready {
        m.issues.push(Issue::new("NotVerified", Some(key), &[("model", m.label.clone())]).fix(Fix::Verify { model: name.to_owned() }));
    }
    m.capabilities = checked.capabilities;
    m.recipe = usable.then_some(recipe);
    m
}

/// Identifies and prepares every installed main model.
fn prepare(ctx: &Ctx) -> BTreeMap<String, ModelState> {
    let (installed, _) = crate::inventory::scan(&ctx.snapshot.info, ctx.model_dir, ctx.reg);
    let presets = crate::catalog::presets();
    let mut out = BTreeMap::new();
    for im in &installed {
        let (role, kind, folders) = main_role(&im.key);
        let Some(name) = im.files.get(&role) else { continue };
        let local = crate::inventory::local_path(ctx.model_dir, folders, name);
        let id = crate::inventory::identify_listed(name, kind, local.as_deref(), None, ctx.reg);
        let folder = local.as_ref().and_then(|p| p.parent()?.file_name()?.to_str().map(str::to_owned)).unwrap_or_else(|| folders[0].to_owned());
        let mut m = ModelState {
            file: name.clone(),
            folder,
            first_seen: ctx.now.clone(),
            fingerprint: local.as_deref().map(file_stamp).unwrap_or_default(),
            identity: Some(id.clone()),
            catalog_id: im.key.clone(),
            ..Default::default()
        };
        if let Some(stored) = ctx.stored.and_then(|s| s.models.get(&im.key)) {
            m.user = stored.user.clone();
        }
        let curated =
            presets.iter().find(|p| !p.installed && p.files.iter().any(|f| crate::comfy::normalized_name(&f.name) == crate::comfy::normalized_name(name)));
        let family = ctx.reg.family(&im.family);
        let resolved = family.map(|f| f.resolved_for_file(name, im.variant.as_deref()));
        let builder = resolved.as_ref().is_some_and(|f| f.buildable());
        let mut m = if let Some(p) = curated.filter(|p| p.model.try_info().is_some_and(|i| !i.template_engine)) {
            let a = crate::catalog::availability(p, &ctx.snapshot.info);
            m.catalog_id = p.model.key().to_owned();
            m.variant.clone_from(&p.variant);
            m.label = label_for(None, Some(&p.model.info().label), &Identity { variant: None, ..id.clone() }, name);
            m.engine = "builtin".into();
            m.status = if a.available { Status::Builtin } else { Status::Blocked };
            m.capabilities = builder_capabilities(&p.model.info().resolved);
            if !a.available {
                m.issues.push(Issue::new("Unavailable", Some(&im.key), &[("reason", a.reason)]));
            }
            m
        } else if builder {
            let preset = presets.iter().find(|p| p.installed && p.model.key() == im.key);
            let a = preset.map(|p| crate::catalog::availability(p, &ctx.snapshot.info));
            m.variant = preset.map(|p| p.variant.clone()).unwrap_or_default();
            m.label = im.label.clone();
            m.engine = "builder".into();
            m.status = if a.as_ref().is_none_or(|a| a.available) { Status::Builtin } else { Status::Blocked };
            m.capabilities = resolved.as_ref().map(builder_capabilities).unwrap_or_default();
            if let Some(a) = a.filter(|a| !a.available) {
                m.issues.push(Issue::new("Unavailable", Some(&im.key), &[("reason", a.reason)]));
            }
            m
        } else {
            template_model(ctx, im, name, &id, m)
        };
        restore_last_good(ctx, &im.key, &mut m);
        out.insert(im.key.clone(), m);
    }
    out
}

fn builder_capabilities(f: &Family) -> Vec<check::CapabilityCheck> {
    let c = &f.capabilities;
    let cap = |name: &str, ok: bool, references: u32| check::CapabilityCheck { name: name.into(), ok, references, detail: String::new() };
    let mut v = Vec::new();
    if c.create {
        v.push(cap("create", true, 0));
    }
    if c.edit {
        v.push(cap("edit", true, c.references));
    }
    v.push(cap("refine", true, 0));
    if c.inpaint {
        v.push(cap("inpaint", true, 0));
    }
    if c.upscale {
        v.push(cap("upscale", true, 0));
    }
    v
}

/// Scans the server and model folder, stores the result and publishes usable template models.
pub fn scan(opts: &Options) -> Scan {
    scan_with(opts, crate::family::registry())
}

/// [`scan`] with another profile registry (tests: a model no profile describes).
pub fn scan_with(opts: &Options, reg: &Registry) -> Scan {
    let client = ComfyClient::new(&opts.host);
    let snapshot = match Snapshot::take(&client) {
        Ok(s) => s,
        Err(e) => {
            log::info!("model setup: {e:#}");
            let stored = SetupState::load(&opts.state_path).servers.remove(&opts.host).unwrap_or_default();
            return Scan {
                server: stored,
                issues: vec![Issue::new("ServerUnreachable", None, &[("host", opts.host.clone())]).fix(Fix::Recheck)],
                ..Default::default()
            };
        }
    };
    scan_snapshot_with(opts, snapshot, reg)
}

/// Static setup from a snapshot already collected by the background inventory poller.
/// This never submits a render, downloads a model or moves a file.
pub fn scan_snapshot(opts: &Options, snapshot: Snapshot) -> Scan {
    background::scan(opts, snapshot)
}

fn scan_snapshot_with(opts: &Options, snapshot: Snapshot, reg: &Registry) -> Scan {
    let client = ComfyClient::new(&opts.host);
    let templates = match &opts.templates_dir {
        Some(dir) => Templates::from_dir(dir, &snapshot.templates_version),
        None => Templates::fetch(&client, &snapshot.templates_version, opts.cache_dir.as_deref()),
    };
    scan_templates_with(opts, snapshot, reg, templates.as_ref().map_err(|e| anyhow::anyhow!("{e:#}")))
}

fn scan_templates_with(opts: &Options, snapshot: Snapshot, reg: &Registry, templates: Result<&Templates>) -> Scan {
    let model_dir = opts.model_dir.as_deref().filter(|d| d.is_dir());
    crate::inventory::refresh(&snapshot.info, model_dir);
    let mut issues = check::server_issues(&snapshot);
    let templates = match templates {
        Ok(t) => Some(t),
        Err(e) => {
            log::warn!("model setup: {e:#}");
            issues.push(Issue::new("TemplateIndexUnavailable", None, &[]).fix(Fix::CheckOnline));
            None
        }
    };
    let stored_all = SetupState::load(&opts.state_path);
    let stored = stored_all.server(&opts.host);
    let ctx = Ctx {
        snapshot: &snapshot,
        templates,
        model_dir,
        reg,
        runtime: crate::companions::runtime(),
        stored,
        now: crate::trash::local_time(std::time::SystemTime::now()),
    };
    let prepared = prepare(&ctx);
    let gpu = snapshot.gpu.as_ref().map(|g| state::GpuState {
        name: g.name.clone(),
        vram_total: g.vram_total,
        generation: crate::companions::runtime().gpu_generation(&g.name).map(str::to_owned),
    });
    let host = opts.host.clone();
    let version = snapshot.version_label();
    let written = SetupState::update(&opts.state_path, |s| {
        let server = s.servers.entry(host.clone()).or_default();
        let previous = std::mem::take(&mut server.models);
        server.comfyui_version = version.clone();
        server.templates_version = snapshot.templates_version.clone();
        server.gpu = gpu.clone();
        for (key, fresh) in prepared.clone() {
            let merged = state::merge(previous.get(&key), fresh, server, &version);
            server.models.insert(key, merged);
        }
    });
    let server = match written {
        Ok(s) => s.servers.get(&opts.host).cloned().unwrap_or_default(),
        Err(e) => {
            log::warn!("model setup state not written: {e:#}");
            ServerState { models: prepared, ..Default::default() }
        }
    };
    publish(&server, &snapshot.info);
    let loras = crate::inventory::loras()
        .into_iter()
        .map(|l| {
            let foreign = l.family.as_deref().is_some_and(|f| f.starts_with("other:"));
            if let Some(base) = l.family.as_deref().and_then(|f| f.strip_prefix("other:")) {
                issues.push(Issue::new("ForeignLora", None, &[("file", l.name.clone()), ("base", base.to_owned())]));
            }
            LoraReport { file: l.name, arch: l.family, status: if foreign { "INFO".into() } else { "OK".into() } }
        })
        .collect();
    let other_files = other_files(&snapshot.info, &server);
    let mut choices: Vec<TemplateChoice> =
        templates.iter().flat_map(|t| &t.entries).map(|e| TemplateChoice { name: e.name.clone(), title: e.title.clone() }).collect();
    choices.sort_by(|a, b| a.title.cmp(&b.title));
    Scan { snapshot: Some(snapshot), server, issues, loras, other_files, templates: choices }
}

/// Text encoders and VAEs on the server, with the models that use them.
fn other_files(info: &ObjectInfo, server: &ServerState) -> Vec<FileReport> {
    let mut out = Vec::new();
    for (class, input, kind) in [("CLIPLoader", "clip_name", "text encoder"), ("VAELoader", "vae_name", "VAE")] {
        for f in info.choices(class, input).into_iter().filter(|f| f.contains('.')) {
            let used_by =
                server.models.values().filter(|m| m.bindings.iter().any(|b| b.used.as_deref() == Some(f.as_str()))).map(|m| m.label.clone()).collect();
            out.push(FileReport { file: f, kind: kind.into(), used_by });
        }
    }
    out
}

fn template_family(m: &ModelState, recipe: &TemplateRecipe) -> Family {
    let reg = crate::family::registry();
    let arch = m.identity.as_ref().and_then(|i| i.arch.clone());
    let profile = arch.as_deref().and_then(|a| reg.family(a));
    let id = arch.clone().unwrap_or_else(|| format!("template:{}", m.identity.as_ref().map(|i| i.stem.as_str()).unwrap_or("")));
    let family_label = profile.map(|p| p.label.clone()).filter(|l| !l.is_empty()).unwrap_or_else(|| m.label.split(" · ").next().unwrap_or_default().to_owned());
    compile::family_for(recipe, profile, &id, &family_label)
}

/// Publishes the usable template recipes of `server` whose main file `info` still lists.
pub fn publish(server: &ServerState, info: &ObjectInfo) -> bool {
    let mut map = BTreeMap::new();
    for (key, m) in &server.models {
        let Some(recipe) = &m.recipe else { continue };
        if !m.status.usable() || m.engine != "template" {
            continue;
        }
        let (_, _, folders) = main_role(key);
        let listed = ["UNETLoader", "UnetLoaderGGUF", "CheckpointLoaderSimple"]
            .iter()
            .any(|c| info.find_file(c, if *c == "CheckpointLoaderSimple" { "ckpt_name" } else { "unet_name" }, &m.file).is_some());
        if !listed || folders.is_empty() {
            continue;
        }
        let mut family = template_family(m, recipe);
        let enabled = |name: &str| m.capabilities.iter().find(|c| c.name == name).is_none_or(|c| c.ok);
        family.capabilities.create &= enabled("create");
        family.capabilities.edit &= enabled("edit");
        family.capabilities.inpaint &= enabled("inpaint");
        let references = if recipe.edit.is_some() { "references" } else { "style" };
        if !enabled(references) {
            family.capabilities.references = 0;
        }
        if let Some(v) = m.verified.as_ref().filter(|_| m.status == Status::PartiallyVerified) {
            let caps = &mut family.capabilities;
            for tool in turned_off(v) {
                match tool {
                    "edit" => caps.edit = false,
                    "inpaint" => caps.inpaint = false,
                    _ => caps.references = 0,
                }
            }
        }
        map.insert(key.clone(), TemplateModel { recipe: Arc::new(recipe.clone()), family, label: m.label.clone() });
    }
    crate::catalog::set_templates(map)
}

/// The tools of a partly verified template model that its failed tests turn off while they apply
/// (Refine has its own check when it is chosen; a failed Create fails the model).
fn turned_off(v: &state::Verified) -> impl Iterator<Item = &'static str> + '_ {
    ["edit", "inpaint", "references", "style"].into_iter().filter(|c| v.capabilities.get(*c).is_some_and(|r| !r.pass))
}

/// One model's verification outcome.
pub struct Verification {
    pub key: String,
    pub label: String,
    pub result: Result<state::Verified, String>,
}

/// Status after a verification.
pub fn status_after(v: &state::Verified) -> Status {
    let failed = |c: &str| v.capabilities.get(c).is_some_and(|r| !r.pass);
    if failed("create") || v.capabilities.is_empty() {
        Status::Failed
    } else if v.capabilities.values().any(|r| !r.pass) {
        Status::PartiallyVerified
    } else {
        Status::Verified
    }
}

/// Verifies the models `keys` one at a time (after [`scan`]), frees ComfyUI's memory after each,
/// and stores the results. `progress` sees each finished capability. Stops at the first model
/// whose test finds ComfyUI no longer processing prompts.
pub fn verify_models(
    opts: &Options,
    scan: &Scan,
    keys: &[String],
    ctl: &JobControl,
    progress: &mut dyn FnMut(&str, verify::Step),
) -> Result<Vec<Verification>> {
    let snapshot = scan.snapshot.as_ref().context("ComfyUI is not reachable")?;
    let lock_path = opts.state_path.with_file_name("ai-verify.lock");
    let _lock = verify::VerifyLock::acquire(&lock_path).context("Another verification is running (ai-verify.lock).")?;
    let client = ComfyClient::new(&opts.host);
    let mut out = Vec::new();
    for key in keys {
        ctl.check()?;
        let Some(m) = scan.server.models.get(key) else { continue };
        let Some(model) = ModelId::from_key(&m.catalog_id) else {
            out.push(Verification { key: key.clone(), label: m.label.clone(), result: Err("not in the catalogue".into()) });
            continue;
        };
        // A built-in or builder model loads the files its preset resolves to (a checkpoint holds
        // its own text encoder and VAE).
        let variant = if m.variant.is_empty() { crate::catalog::default_variant(model) } else { m.variant.as_str() };
        let preset_files =
            crate::catalog::preset(model, variant).filter(|_| m.engine != "template").map(|p| crate::catalog::availability(p, &snapshot.info).files);
        let file = |role: &str, preset_role: Role| {
            m.bindings.iter().find(|b| b.role == role).and_then(|b| b.used.clone()).or_else(|| {
                let files = preset_files.as_ref()?;
                files.get(&preset_role).or_else(|| files.get(&Role::Checkpoint)).cloned()
            })
        };
        let target = verify::Target {
            model,
            variant: &m.variant,
            label: &m.label,
            quant: m.identity.as_ref().and_then(|i| i.quant.class.as_ref()).map(|q| q.label()).unwrap_or_default(),
            te: file("clip", Role::Clip).unwrap_or_default(),
            vae: file("vae", Role::Vae).unwrap_or_default(),
            off: m.capabilities.iter().filter(|c| !c.ok).map(|c| c.name.clone()).collect(),
            refine_recipe: m
                .universal_recipe
                .clone()
                .or_else(|| m.recipe.as_ref().map(|recipe| crate::refine::Recipe::Compiled { recipe: Box::new(recipe.clone()) })),
            model_info: m.recipe.as_ref().map(|recipe| {
                let family = template_family(m, recipe);
                let def = crate::family::ModelDef { key: m.catalog_id.clone(), label: m.label.clone(), ..Default::default() };
                let mut info = crate::catalog::info_for(&family, &def, crate::catalog::Origin::Installed);
                info.recipe = Some(Arc::new(recipe.clone()));
                info
            }),
        };
        let seed = u64::from(rand::random::<u32>());
        let result = verify::verify(&opts.host, &target, seed, ctl, &mut |step| progress(key, step));
        if let Err(e) = client.free_memory() {
            log::warn!("ComfyUI kept its models loaded after verification: {e:#}");
        }
        let result = match result {
            Ok(mut v) => {
                v.at = crate::trash::local_time(std::time::SystemTime::now());
                v.comfyui_version = snapshot.version_label();
                v.templates_version = snapshot.templates_version.clone();
                v.recipe_version = crate::template::RECIPE_VERSION;
                v.recipe_sha256 = model_recipe_sha256(m);
                v.gpu = snapshot.gpu.as_ref().map(|g| g.name.clone()).unwrap_or_default();
                let issues = verification_issues(key, m, &target, &v);
                let (k, stored) = (key.clone(), v.clone());
                let mut recorded = false;
                SetupState::update(&opts.state_path, |s| {
                    let Some(server) = s.servers.get_mut(&opts.host) else { return };
                    if server.comfyui_version != snapshot.version_label() || server.templates_version != snapshot.templates_version {
                        return;
                    }
                    if let Some(sm) = server.models.get_mut(&k)
                        && &*sm == m
                    {
                        sm.status = status_after(&stored);
                        sm.issues.retain(|i| i.code != "NotVerified" && !i.code.starts_with("Verify"));
                        sm.issues.extend(issues);
                        sm.verified = Some(stored);
                        recorded = true;
                    }
                })?;
                if recorded { Ok(v) } else { Err("Model setup changed during verification. Re-check the model and test it again.".into()) }
            }
            Err(e) if e.is::<Busy>() => Err(Busy.to_string()),
            Err(e) if e.is::<verify::Stalled>() => {
                // Every later test would wait on the same stopped worker.
                out.push(Verification { key: key.clone(), label: m.label.clone(), result: Err(e.to_string()) });
                break;
            }
            Err(e) => {
                if ctl.is_cancelled() {
                    return Err(e);
                }
                Err(format!("{e:#}"))
            }
        };
        out.push(Verification { key: key.clone(), label: m.label.clone(), result });
    }
    Ok(out)
}

/// `Fill`, `Style references`, `Edit`.
fn tool_label(capability: &str) -> String {
    match capability {
        "inpaint" => "Fill".to_owned(),
        "style" => "Style references".to_owned(),
        "prompt_enhancer" => "Prompt enhancer".to_owned(),
        c => format!("{}{}", c[..1].to_ascii_uppercase(), &c[1..]),
    }
}

/// The issues a verification found (failures and warnings, with the files involved, and the
/// tools a template model's failures turn off).
fn verification_issues(key: &str, m: &ModelState, t: &verify::Target, v: &state::Verified) -> Vec<Issue> {
    let mut out = Vec::new();
    for (cap, r) in &v.capabilities {
        let args = [
            ("capability", tool_label(cap)),
            ("model", m.label.clone()),
            ("quant", t.quant.clone()),
            (
                "te",
                if cap == "prompt_enhancer" {
                    m.recipe.as_ref().and_then(|r| r.upsampler.as_ref()).and_then(|u| u.installed.clone()).unwrap_or_else(|| t.te.clone())
                } else {
                    t.te.clone()
                },
            ),
            ("vae", t.vae.clone()),
            ("message", r.message.clone().unwrap_or_default()),
            ("difference", r.difference.map(|d| format!("{d:.1}")).unwrap_or_default()),
            ("outside", r.outside_change.map(|d| format!("{d:.1}")).unwrap_or_default()),
            ("got", r.size.map(|(w, h)| format!("{w}x{h}")).unwrap_or_default()),
            ("want", r.requested.map(|(w, h)| format!("{w}x{h}")).unwrap_or_default()),
        ];
        out.extend(r.failure.iter().chain(&r.warnings).map(|code| Issue::new(code, Some(key), &args)));
    }
    if m.engine == "template" && status_after(v) == Status::PartiallyVerified {
        out.extend(turned_off(v).map(|tool| Issue::new("VerifyTurnedOff", Some(key), &[("capability", tool_label(tool))])));
    }
    out
}

/// The report of a scan (and its verifications).
pub fn report(opts: &Options, scan: &Scan) -> Report {
    let s = scan.snapshot.as_ref();
    let server = ServerReport {
        host: opts.host.clone(),
        reachable: s.is_some(),
        remote: !is_local(&opts.host),
        comfyui_version: s.map(|s| s.version_label()),
        templates_version: s.map(|s| s.templates_version.clone()).filter(|v| !v.is_empty()),
        required_templates_version: s.map(|s| s.required_templates_version.clone()).filter(|v| !v.is_empty()),
        deploy_environment: s.map(|s| s.deploy_environment.clone()).filter(|v| !v.is_empty()),
        gpu: s.and_then(|s| s.gpu.as_ref()).map(|g| report::GpuReport {
            name: g.name.clone(),
            vram_gb: g.vram_total as f64 / 1e9,
            generation: crate::companions::runtime().gpu_generation(&g.name).map(str::to_owned),
        }),
        model_dir: opts.model_dir.as_ref().map(|d| d.display().to_string()),
    };
    let reg = crate::family::registry();
    let mut models = Vec::new();
    let mut issues: Vec<report::IssueReport> = scan.issues.iter().map(Into::into).collect();
    for (key, m) in &scan.server.models {
        let id = m.identity.as_ref();
        let caps = m
            .capabilities
            .iter()
            .map(|c| {
                let verified = m.verified.as_ref().and_then(|v| v.capabilities.get(&c.name));
                let state = match (c.ok, verified) {
                    (false, _) => "blocked",
                    (true, Some(r)) if !r.pass => "failed",
                    (true, Some(r)) if !r.warnings.is_empty() => "warning",
                    (true, Some(_)) => "verified",
                    (true, None) => "ready",
                };
                let detail = if c.references > 0 { c.references.to_string() } else { c.detail.clone() };
                CapabilityReport { name: c.name.clone(), state: state.into(), detail }
            })
            .collect();
        let profile = id.and_then(|i| i.arch.as_deref()).and_then(|a| reg.family(a));
        models.push(ModelReport {
            key: key.clone(),
            file: m.file.clone(),
            folder: m.folder.clone(),
            bytes: (m.fingerprint.bytes > 0).then_some(m.fingerprint.bytes),
            label: m.label.clone(),
            arch: id.and_then(|i| i.arch.clone()),
            variant: id.and_then(|i| i.variant.clone()),
            quant: id.and_then(|i| i.quant.class.as_ref()).map(|q| q.id()),
            engine: m.engine.clone(),
            templates: m.templates.values().cloned().collect(),
            status: m.status.id().into(),
            capabilities: caps,
            bindings: m
                .bindings
                .iter()
                .map(|b| BindingReport {
                    role: b.role.clone(),
                    used: b.used.clone(),
                    declared: b.declared.clone(),
                    why: serde_json::to_value(b.why).ok().and_then(|v| v.as_str().map(str::to_owned)).unwrap_or_default(),
                })
                .collect(),
            verified_at: m.verified.as_ref().map(|v| v.at.clone()),
            seconds: m.verified.as_ref().and_then(|v| v.capabilities.get("create")).map(|c| c.seconds),
            peak_vram_gb: m.verified.as_ref().and_then(|v| v.capabilities.values().filter_map(|c| c.peak_vram_gb).reduce(f64::max)),
            licence: profile.filter(|p| !p.license.is_empty()).map(|p| LicenceReport {
                name: p.license.clone(),
                summary: p.license_summary.clone().unwrap_or_default(),
                commercial: serde_json::to_value(p.commercial).ok().and_then(|v| v.as_str().map(str::to_owned)).unwrap_or_default(),
                url: p.license_url.clone(),
            }),
        });
        issues.extend(m.issues.iter().map(report::IssueReport::from));
    }
    issues.sort_by_key(|i| i.severity);
    let mut r = Report {
        report_version: report::REPORT_VERSION,
        generated_at: crate::trash::local_time(std::time::SystemTime::now()).replacen('T', " ", 1),
        server,
        models,
        loras: scan.loras.clone(),
        other_files: scan.other_files.clone(),
        issues,
        ..Default::default()
    };
    r.summarise();
    r
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_template_models_failed_tool_is_said_to_be_turned_off() {
        let result = |pass: bool, failure: Option<&str>| state::CapabilityResult { pass, failure: failure.map(str::to_owned), ..Default::default() };
        let v = state::Verified {
            capabilities: [("create".to_owned(), result(true, None)), ("inpaint".to_owned(), result(false, Some("VerifyFailed:inpaint_nochange")))].into(),
            ..Default::default()
        };
        let t = verify::Target {
            model: ModelId::Qwen,
            variant: "",
            label: "M",
            quant: String::new(),
            te: String::new(),
            vae: String::new(),
            off: Vec::new(),
            refine_recipe: None,
            model_info: None,
        };
        let sentences = |engine: &str| {
            let m = ModelState { engine: engine.into(), label: "M".into(), ..Default::default() };
            verification_issues("unet:m", &m, &t, &v).iter().map(Issue::english).collect::<Vec<_>>()
        };
        assert_eq!(sentences("template"), ["Fill didn't change the selected area in the test.", "Fill is turned off for this model until it passes its test."]);
        assert_eq!(sentences("builtin"), ["Fill didn't change the selected area in the test."], "a built-in model keeps its tools");
    }
    #[test]
    fn display_names_join_variants_and_name_precision_once() {
        let reg = crate::family::registry();
        for (family, variant, file, expected) in [
            ("flux3-klein", "distilled", "flux3-image-klein-bf16.safetensors", "FLUX.3 Klein Distilled · BF16"),
            ("moss-image", "sft", "moss_image_sft_bf16.safetensors", "MOSS-Image SFT · BF16"),
            ("qwen-image-21", "bf16", "qwen_image_2.1_bf16.safetensors", "Qwen Image 2.1 · BF16"),
            ("", "", "ming_image_0.1_design_bf16.safetensors", "Ming Image 0.1 Design · BF16"),
        ] {
            let id = Identity {
                variant: (!variant.is_empty()).then(|| variant.into()),
                quant: crate::identify::Quant { class: Some(crate::identify::QuantClass::Bf16), ..Default::default() },
                ..Default::default()
            };
            assert_eq!(label_for(reg.family(family), None, &id, file), expected);
        }
    }
    #[test]
    fn the_tested_signature_covers_every_executable_recipe_part() {
        use crate::template::TaskGraph;
        let graph = TaskGraph {
            graph: serde_json::from_value(serde_json::json!({"sample": {"class_type": "KSampler", "inputs": {"cfg": 3.5}}})).unwrap(),
            files: vec![crate::companions::Binding { role: "clip".into(), used: Some("encoder-a.safetensors".into()), ..Default::default() }],
            ..Default::default()
        };
        let mut create = graph.clone();
        create.enhanced = Some(Box::new(graph.clone()));
        let recipe = TemplateRecipe { create: Some(create), edit: Some(graph.clone()), style: Some(graph), ..Default::default() };
        let original = recipe_sha256(&recipe);
        for part in ["edit", "style", "enhanced", "binding", "defaults", "fill"] {
            let mut changed = recipe.clone();
            match part {
                "edit" => changed.edit.as_mut().unwrap().graph["sample"]["inputs"]["cfg"] = serde_json::json!(5.0),
                "style" => changed.style.as_mut().unwrap().graph["sample"]["inputs"]["cfg"] = serde_json::json!(5.0),
                "enhanced" => changed.create.as_mut().unwrap().enhanced.as_mut().unwrap().graph["sample"]["inputs"]["cfg"] = serde_json::json!(5.0),
                "binding" => changed.create.as_mut().unwrap().files[0].used = Some("encoder-b.safetensors".into()),
                "defaults" => changed.defaults.cfg = 8.0,
                "fill" => changed.inpaint = crate::template::InpaintWay::Instruction,
                _ => unreachable!(),
            }
            assert_ne!(recipe_sha256(&changed), original, "a changed {part} must be tested again");
        }
        let mut provenance = recipe;
        provenance.sources.push(crate::template::TemplateSource {
            templates_version: "new package".into(),
            sha256: "different source JSON".into(),
            ..Default::default()
        });
        provenance.create.as_mut().unwrap().files[0].why = crate::companions::Why::User;
        assert_eq!(recipe_sha256(&provenance), original, "source metadata cannot invalidate identical execution");
    }

    #[test]
    fn servers_with_the_same_package_version_keep_separate_template_caches() {
        let root = std::env::temp_dir().join(format!("li-template-cache-{}", uuid::Uuid::new_v4()));
        let one = crate::mock::MockComfy::start().unwrap();
        let two = crate::mock::MockComfy::start().unwrap();
        for (label, server) in [("one", &one), ("two", &two)] {
            let dir = root.join(label);
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(dir.join("index.json"), br#"[{"templates":[{"name":"image_test"}]}]"#).unwrap();
            std::fs::write(dir.join("image_test.json"), serde_json::to_vec(&serde_json::json!({"nodes":[], "origin":label})).unwrap()).unwrap();
            server.set_templates_dir(dir);
            let templates = Templates::fetch(&ComfyClient::new(server.host()), "same-version", Some(&root.join("cache"))).unwrap();
            assert_eq!(templates.body("image_test").unwrap()["origin"], label);
        }
        std::fs::remove_dir_all(root).unwrap();
    }
}
