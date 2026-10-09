//! The model catalogue the editor works with: every model from the family profiles
//! ([`crate::family`], with their pinned download presets) plus the models found installed in
//! ComfyUI ([`crate::inventory`]). Each model resolves to a [`ModelInfo`] (what it is good at, its
//! capabilities and ranges) and [`Preset`]s (the exact files a precision needs, and whether a
//! running ComfyUI has them).
//!
//! [`ModelId`] is the model's stable key (0.7's keys for the original models, so settings, the
//! generated library and LoRA registries keep working). The catalogue is rebuilt when profiles or
//! the installed models change; [`catalog`] returns a `&'static` snapshot.

use std::collections::BTreeMap;
use std::sync::RwLock;

use crate::comfy::ObjectInfo;
use crate::family::{Family, FamilyKind, ModelDef, RefMethod};
pub use crate::family::{FileSpec, Range, Role};

/// A model's key, interned (`"qwen"`, `"flux2-klein-4b"`, `"ckpt:juggernautXL_v9.safetensors"`).
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ModelId(&'static str);

impl std::fmt::Debug for ModelId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ModelId({})", self.0)
    }
}

#[allow(non_upper_case_globals)]
impl ModelId {
    pub const Qwen: ModelId = ModelId("qwen");
    pub const ZImageTurbo: ModelId = ModelId("z-image-turbo");
    pub const Klein4B: ModelId = ModelId("flux2-klein-4b");
    pub const Klein9B: ModelId = ModelId("flux2-klein-9b");
    pub const Ernie: ModelId = ModelId("ernie-image");
    pub const SeedVr2: ModelId = ModelId("seedvr2");
    /// FLUX.2 Klein base 4B + fal's object-removal LoRA (AI Remove).
    pub const KleinRemove: ModelId = ModelId("klein-remove");

    /// The models Local Image 0.7 shipped (tools depend on them by key).
    pub const ORIGINAL: [ModelId; 7] =
        [ModelId::Qwen, ModelId::ZImageTurbo, ModelId::Klein4B, ModelId::Klein9B, ModelId::Ernie, ModelId::SeedVr2, ModelId::KleinRemove];

    pub fn key(self) -> &'static str {
        self.0
    }
    /// The model with this key, if the catalogue has it.
    pub fn from_key(k: &str) -> Option<Self> {
        catalog().models.iter().find(|m| m.id.0 == k).map(|m| m.id)
    }
    /// A key for a model that may not be in the catalogue yet (interned).
    pub fn intern(k: &str) -> Self {
        if let Some(m) = Self::from_key(k) {
            return m;
        }
        static INTERNED: RwLock<Vec<&'static str>> = RwLock::new(Vec::new());
        if let Some(s) = INTERNED.read().ok().and_then(|v| v.iter().find(|s| **s == k).copied()) {
            return ModelId(s);
        }
        let s: &'static str = Box::leak(k.to_owned().into_boxed_str());
        if let Ok(mut v) = INTERNED.write() {
            v.push(s);
        }
        ModelId(s)
    }
    /// What the model is. Panics for a key that is not in the catalogue (use [`Self::try_info`]).
    pub fn info(self) -> &'static ModelInfo {
        self.try_info().unwrap_or_else(|| catalog().models.first().expect("the catalogue is never empty"))
    }
    pub fn try_info(self) -> Option<&'static ModelInfo> {
        catalog().models.iter().find(|m| m.id == self)
    }
    /// Every model that creates images (not tool-only ones), catalogue order.
    pub fn generators() -> Vec<ModelId> {
        catalog().models.iter().filter(|m| m.text_to_image && !m.tool).map(|m| m.id).collect()
    }
    pub fn all() -> Vec<ModelId> {
        catalog().models.iter().map(|m| m.id).collect()
    }
}

impl serde::Serialize for ModelId {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(self.0)
    }
}

impl<'de> serde::Deserialize<'de> for ModelId {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        Ok(ModelId::intern(&s))
    }
}

/// Where a catalogue model comes from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Origin {
    /// A curated model from a family profile (downloadable presets).
    Profile,
    /// Found installed in ComfyUI.
    Installed,
}

/// A model with its family's settings resolved.
#[derive(Clone, Debug)]
pub struct ModelInfo {
    pub id: ModelId,
    pub family: String,
    pub family_label: String,
    pub group: String,
    pub label: String,
    pub short: String,
    pub best_for: String,
    pub description: String,
    pub max_references: usize,
    /// The one image input is a starting image for variations (Z-Image), not a semantic reference.
    pub init_image: bool,
    pub text_to_image: bool,
    pub edit: bool,
    pub inpaint: bool,
    pub upscale: bool,
    pub transparent: bool,
    pub negative_prompt: bool,
    pub steps: Range,
    pub guidance: Range,
    pub lora: bool,
    pub vram_gb: u32,
    pub license: String,
    pub license_url: String,
    pub notes: String,
    pub tool: bool,
    pub starter: bool,
    pub origin: Origin,
    /// The family profile with this model's overrides applied (what the builders run).
    pub resolved: Family,
}

impl ModelInfo {
    /// Short capability tags for menus and cards ("Create", "Edit", "3 refs", "Fill", "Alpha").
    pub fn tags(&self) -> Vec<String> {
        let mut t = Vec::new();
        if self.text_to_image {
            t.push("Create".to_owned());
        }
        if self.edit {
            t.push("Edit".to_owned());
        }
        if self.max_references > 0 && !self.init_image {
            t.push(format!("{} ref{}", self.max_references, if self.max_references == 1 { "" } else { "s" }));
        } else if matches!(self.resolved.pipeline.reference, RefMethod::IpAdapter | RefMethod::Redux) {
            t.push("Refs via adapter".to_owned());
        }
        if self.inpaint {
            t.push("Fill".to_owned());
        }
        if self.transparent {
            t.push("Alpha".to_owned());
        }
        if self.upscale {
            t.push("Upscale".to_owned());
        }
        // No "LoRA" tag: in a list of models it read as "this is a LoRA" (LoRA support shows
        // as the Styles (LoRA) section once the model is picked).
        t
    }
}

/// A downloadable (or installed) set of files for a model.
#[derive(Clone, Debug)]
pub struct Preset {
    pub model: ModelId,
    pub variant: String,
    pub label: String,
    pub files: Vec<FileSpec>,
    /// Set when the publisher gates the weights (approval on Hugging Face).
    pub access_url: Option<String>,
    pub required_nodes: Vec<String>,
    /// `(class, input, value)` combo values that must exist.
    pub required_choices: Vec<(String, String, String)>,
    /// Installed models: the files exist in ComfyUI; nothing to download or hash.
    pub installed: bool,
}

impl Preset {
    pub fn total_bytes(&self) -> u64 {
        self.files.iter().map(|f| f.bytes).sum()
    }
    pub fn id(&self) -> String {
        format!("{}:{}", self.model.key(), self.variant)
    }
}

/// A model found installed (built by [`crate::inventory`]).
#[derive(Clone, Debug, PartialEq)]
pub struct InstalledModel {
    pub key: String,
    pub family: String,
    pub label: String,
    /// Role → ComfyUI file name (main weights, text encoders, VAE).
    pub files: BTreeMap<Role, String>,
}

/// One snapshot of every model.
#[derive(Debug, Default)]
pub struct Catalog {
    pub models: Vec<ModelInfo>,
    pub presets: Vec<Preset>,
}

fn info_for(f: &Family, m: &ModelDef, origin: Origin) -> ModelInfo {
    let r = f.resolved_for(m);
    let c = r.capabilities;
    let short = if m.short.is_empty() { m.label.clone() } else { m.short.clone() };
    ModelInfo {
        id: ModelId::intern_raw(&m.key),
        family: f.id.clone(),
        family_label: f.label.clone(),
        group: f.group.clone(),
        label: m.label.clone(),
        short,
        best_for: if m.best_for.is_empty() { f.description.split(':').next().unwrap_or("").to_owned() } else { m.best_for.clone() },
        description: if m.description.is_empty() { f.description.clone() } else { m.description.clone() },
        max_references: c.references as usize,
        init_image: r.pipeline.reference == RefMethod::InitImage,
        text_to_image: c.create && r.kind == FamilyKind::Image,
        edit: c.edit,
        inpaint: c.inpaint,
        upscale: c.upscale,
        transparent: c.transparent,
        negative_prompt: c.negative_prompt,
        steps: r.sampling.steps,
        guidance: r.sampling.cfg,
        lora: !r.lora.loader.is_empty(),
        vram_gb: r.vram_gb,
        license: m.license.clone().unwrap_or_else(|| f.license.clone()),
        license_url: m.license_url.clone().unwrap_or_else(|| f.license_url.clone()),
        notes: m.notes.clone(),
        tool: m.tool,
        starter: m.starter,
        origin,
        resolved: r,
    }
}

impl ModelId {
    /// Interning without the catalogue lookup (used while the catalogue is being built).
    fn intern_raw(k: &str) -> Self {
        for m in ModelId::ORIGINAL {
            if m.0 == k {
                return m;
            }
        }
        static INTERNED: RwLock<Vec<&'static str>> = RwLock::new(Vec::new());
        if let Some(s) = INTERNED.read().ok().and_then(|v| v.iter().find(|s| **s == k).copied()) {
            return ModelId(s);
        }
        let s: &'static str = Box::leak(k.to_owned().into_boxed_str());
        if let Ok(mut v) = INTERNED.write() {
            v.push(s);
        }
        ModelId(s)
    }
}

/// Builds the catalogue from the registry and the installed models.
pub fn build(reg: &crate::family::Registry, installed: &[InstalledModel]) -> Catalog {
    let mut cat = Catalog::default();
    for (f, m) in reg.models() {
        let info = info_for(f, m, Origin::Profile);
        for p in &m.presets {
            cat.presets.push(Preset {
                model: info.id,
                variant: p.variant.clone(),
                label: p.label.clone(),
                files: p.files.clone(),
                access_url: p.access_url.clone(),
                required_nodes: p.required_nodes.clone(),
                required_choices: p.required_choices.clone(),
                installed: false,
            });
        }
        cat.models.push(info);
    }
    for im in installed {
        let Some(f) = reg.family(&im.family) else { continue };
        // A curated model whose files these are is already listed.
        let main = im.files.get(&Role::Unet).or_else(|| im.files.get(&Role::Checkpoint));
        if let Some(main) = main
            && cat.presets.iter().any(|p| !p.installed && p.files.iter().any(|x| crate::comfy::normalized_name(&x.name) == crate::comfy::normalized_name(main)))
        {
            continue;
        }
        let def = ModelDef { key: im.key.clone(), label: im.label.clone(), short: im.label.clone(), ..Default::default() };
        let info = info_for(f, &def, Origin::Installed);
        cat.presets.push(Preset {
            model: info.id,
            variant: "installed".into(),
            label: "Installed".into(),
            files: im
                .files
                .iter()
                .map(|(role, name)| FileSpec { role: *role, folder: role.folder().into(), name: name.clone(), ..Default::default() })
                .collect(),
            access_url: None,
            required_nodes: Vec::new(),
            required_choices: Vec::new(),
            installed: true,
        });
        cat.models.push(info);
    }
    cat
}

struct State {
    snapshot: Option<&'static Catalog>,
    installed: Vec<InstalledModel>,
}

static STATE: RwLock<State> = RwLock::new(State { snapshot: None, installed: Vec::new() });

/// The current catalogue.
pub fn catalog() -> &'static Catalog {
    if let Some(c) = STATE.read().ok().and_then(|s| s.snapshot) {
        return c;
    }
    let installed = STATE.read().map(|s| s.installed.clone()).unwrap_or_default();
    let c: &'static Catalog = Box::leak(Box::new(build(crate::family::registry(), &installed)));
    if let Ok(mut s) = STATE.write() {
        if let Some(existing) = s.snapshot {
            return existing;
        }
        s.snapshot = Some(c);
    }
    c
}

/// Forgets the snapshot (profiles changed); the next [`catalog`] rebuilds it.
pub fn invalidate() {
    if let Ok(mut s) = STATE.write() {
        s.snapshot = None;
    }
}

/// Replaces the installed models (after an inventory scan). Returns true when they changed.
pub fn set_installed(installed: Vec<InstalledModel>) -> bool {
    let Ok(mut s) = STATE.write() else { return false };
    if s.installed == installed {
        return false;
    }
    s.installed = installed;
    s.snapshot = None;
    true
}

pub fn presets() -> &'static [Preset] {
    &catalog().presets
}

pub fn presets_for(model: ModelId) -> impl Iterator<Item = &'static Preset> {
    presets().iter().filter(move |p| p.model == model)
}

pub fn preset(model: ModelId, variant: &str) -> Option<&'static Preset> {
    presets_for(model).find(|p| p.variant == variant)
}

pub fn default_variant(model: ModelId) -> &'static str {
    presets_for(model).next().map(|p| p.variant.as_str()).unwrap_or("bf16")
}

/// Whether a preset can run on the connected ComfyUI, and the loader names to use.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Availability {
    pub available: bool,
    pub reason: String,
    pub files: BTreeMap<Role, String>,
}

pub fn availability(preset: &Preset, info: &ObjectInfo) -> Availability {
    let model = preset.model.try_info();
    let mut nodes: Vec<String> = preset.required_nodes.clone();
    if let Some(m) = model {
        nodes.extend(crate::builders::required_nodes(&m.resolved));
    }
    nodes.sort();
    nodes.dedup();
    let missing_nodes: Vec<&str> = nodes.iter().map(String::as_str).filter(|n| !info.has_node(n)).collect();
    if !missing_nodes.is_empty() {
        return Availability { reason: format!("Update ComfyUI: it is missing the nodes {}.", missing_nodes.join(", ")), ..Default::default() };
    }
    let family_choices = model.map(|m| m.resolved.pipeline.required_choices.clone()).unwrap_or_default();
    for (class, input, value) in preset.required_choices.iter().chain(family_choices.iter()) {
        if !info.choices(class, input).iter().any(|c| c == value) {
            return Availability { reason: format!("Update ComfyUI: {class} does not offer “{value}”."), ..Default::default() };
        }
    }
    let mut files = BTreeMap::new();
    let mut missing = Vec::new();
    for f in &preset.files {
        let found = if preset.installed {
            Some(f.name.clone())
        } else {
            let (class, input) = f.role.loader();
            info.find_file(class, input, &f.name)
        };
        match found {
            Some(found) => {
                files.insert(f.role, found);
            }
            None => missing.push(f.name.as_str()),
        }
    }
    // Installed models: text encoders and VAE the family needs, found by its component rules.
    if let Some(m) = model {
        for (role, name) in crate::inventory::resolve_components(&m.resolved, info) {
            files.entry(role).or_insert(name);
        }
        for c in &m.resolved.components {
            if !files.contains_key(&c.role) && preset.installed {
                missing.push(c.names.first().map(String::as_str).unwrap_or("a text encoder or VAE"));
            }
        }
    }
    if !missing.is_empty() {
        let label = model.map(|m| m.label.as_str()).unwrap_or("this model");
        return Availability {
            reason: format!("Download the {label} {} model files ({}), then refresh.", preset.label, missing.join(", ")),
            files,
            ..Default::default()
        };
    }
    Availability { available: true, reason: String::new(), files }
}

/// Planning recommendation shown in the hardware guide.
pub const VRAM_GUIDE: &[(&str, u32)] = &[
    ("SD 1.5", 4),
    ("SDXL, Pony, Illustrious", 8),
    ("Z-Image Turbo, FLUX.2 Klein 4B, FLUX.1 (FP8) and AI Remove", 16),
    ("Qwen Compact INT8, Qwen Image (FP8), Klein 9B, ERNIE-Image, HiDream (FP8)", 24),
    ("Qwen Full BF16, FLUX.2 dev and SeedVR2 4K enhancement", 32),
];

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn every_original_model_has_a_preset_and_hashes_are_hex() {
        for m in ModelId::ORIGINAL {
            assert!(presets_for(m).next().is_some(), "{m:?}");
            assert_eq!(ModelId::from_key(m.key()), Some(m));
        }
        for p in presets().iter().filter(|p| !p.installed) {
            for f in &p.files {
                assert_eq!(f.sha256.len(), 64);
                assert!(f.sha256.bytes().all(|b| b.is_ascii_hexdigit()));
                assert!(f.url.starts_with("https://huggingface.co/"));
                assert!(f.url.ends_with(".safetensors"));
            }
        }
        assert_eq!(ModelId::Qwen.info().label, "Qwen Image 2.1");
        assert_eq!(ModelId::Klein4B.info().steps.default, 4.0);
        assert!(ModelId::Qwen.info().transparent);
        assert!(!ModelId::generators().contains(&ModelId::KleinRemove));
    }

    #[test]
    fn availability_reports_missing_nodes_then_files() {
        let p = preset(ModelId::Ernie, "bf16").unwrap();
        let empty = ObjectInfo(json!({}));
        assert!(availability(p, &empty).reason.contains("Update ComfyUI"));
        let mut info = json!({});
        for n in crate::builders::required_nodes(&ModelId::Ernie.info().resolved) {
            info[n] = json!({"input": {"required": {}}});
        }
        info["CLIPLoader"] = json!({"input": {"required": {"type": [["flux2"]], "clip_name": [["sub/Ministral-3-3B.safetensors"]]}}});
        info["UNETLoader"] = json!({"input": {"required": {"unet_name": [["ernie-image.safetensors"]]}}});
        let a = availability(p, &ObjectInfo(info.clone()));
        assert!(!a.available && a.reason.contains("flux2-vae"), "{a:?}");
        info["VAELoader"] = json!({"input": {"required": {"vae_name": [["flux2-vae.safetensors"]]}}});
        let a = availability(p, &ObjectInfo(info));
        assert!(a.available, "{a:?}");
        assert_eq!(a.files[&Role::Clip], "sub/Ministral-3-3B.safetensors");
    }

    #[test]
    fn installed_models_join_the_catalogue() {
        let reg = crate::family::registry();
        let im = InstalledModel {
            key: "ckpt:juggernautXL_v9.safetensors".into(),
            family: "sdxl".into(),
            label: "juggernautXL_v9".into(),
            files: [(Role::Checkpoint, "juggernautXL_v9.safetensors".to_owned())].into(),
        };
        let cat = build(reg, &[im]);
        let m = cat.models.iter().find(|m| m.id.key() == "ckpt:juggernautXL_v9.safetensors").unwrap();
        assert_eq!(m.family, "sdxl");
        assert_eq!(m.origin, Origin::Installed);
        assert!(m.tags().contains(&"Fill".to_owned()));
        // a LoRA-capable model is not tagged as if it were a LoRA
        assert!(m.lora && !m.tags().iter().any(|t| t.contains("LoRA")), "{:?}", m.tags());
        let p = cat.presets.iter().find(|p| p.model == m.id).unwrap();
        assert!(p.installed);
    }
}
