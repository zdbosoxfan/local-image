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
#[derive(Clone, Debug, Default, PartialEq)]
pub struct InstalledModel {
    pub key: String,
    /// The architecture's family id (a builder family, a template-only family, or empty).
    pub family: String,
    pub label: String,
    /// Role → ComfyUI file name (main weights, text encoders, VAE).
    pub files: BTreeMap<Role, String>,
    /// The variant detection gave (`9b`, `turbo`, `distilled`).
    pub variant: Option<String>,
    pub quant: Option<crate::identify::QuantClass>,
    /// The precision-independent name key ([`crate::stem::stem_key`]).
    pub stem: String,
}

/// One snapshot of every model.
#[derive(Debug, Default)]
pub struct Catalog {
    pub models: Vec<ModelInfo>,
    pub presets: Vec<Preset>,
    /// On-device resources in the same catalog, borrowing the established runner table.
    /// Runner descriptors remain the compatibility API; downloads use [`on_device_files`].
    pub on_device: &'static [li_seg::ModelSpec],
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
    let mut cat = Catalog { on_device: li_seg::MODELS, ..Catalog::default() };
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
        let unknown = crate::refine::unknown_profile_for(&im.key);
        let f = reg.family(&im.family).unwrap_or(&unknown);
        // A curated model whose files these are is already listed.
        let main = im.files.get(&Role::Unet).or_else(|| im.files.get(&Role::Checkpoint));
        if let Some(main) = main
            && cat.presets.iter().any(|p| !p.installed && p.files.iter().any(|x| crate::comfy::normalized_name(&x.name) == crate::comfy::normalized_name(main)))
        {
            continue;
        }
        let def = ModelDef { key: im.key.clone(), label: im.label.clone(), short: im.label.clone(), ..Default::default() };
        let resolved = f.resolved_for_file(main.map(String::as_str).unwrap_or_default(), im.variant.as_deref());
        let info = info_for(&resolved, &def, Origin::Installed);
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
        // An inventory scan may have replaced the installed models while this was building; caching it then would
        // hide them until the next invalidation, so only a catalogue built from the current list is kept.
        if s.installed == installed {
            s.snapshot = Some(c);
        }
    }
    c
}

/// Forgets the snapshot (profiles changed); the next [`catalog`] rebuilds it.
pub fn invalidate() {
    if let Ok(mut s) = STATE.write() {
        s.snapshot = None;
    }
}

static INVENTORY_GENERATION: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

pub fn inventory_generation() -> u64 {
    INVENTORY_GENERATION.load(std::sync::atomic::Ordering::SeqCst)
}

/// Replaces the installed models (after an inventory scan). Returns true when they changed.
pub fn set_installed(installed: Vec<InstalledModel>) -> bool {
    let Ok(mut s) = STATE.write() else { return false };
    if s.installed == installed {
        return false;
    }
    s.installed = installed;
    INVENTORY_GENERATION.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    s.snapshot = None;
    true
}

/// The starting denoise for refining with `model`: 1.0 for native reference/edit conditioning
/// (FLUX.2, Kontext, Qwen Edit and Qwen 2.1), a partial 0.35 for ordinary img2img families.
pub fn refine_default_denoise(model: ModelId) -> f32 {
    if let Some(d) = crate::refine::model_defaults(model) {
        return d.denoise;
    }
    match model.try_info() {
        Some(i) if matches!(i.resolved.pipeline.reference, crate::family::RefMethod::ReferenceLatent | crate::family::RefMethod::TextEncoder) => 1.0,
        _ => 0.35,
    }
}

/// The on-device resource with this stable key, served by the existing catalog.
pub fn on_device_spec(id: &str) -> Option<&'static li_seg::ModelSpec> {
    catalog().on_device.iter().find(|model| model.id == id)
}

/// The common file descriptors for an on-device resource. Packaged denoisers download one
/// archive, then install the separately pinned members; other resources download the members.
/// `role` is unused here: on-device runners never resolve a ComfyUI loader.
#[derive(Clone, Debug, PartialEq)]
pub struct OnDeviceFiles {
    pub installed: Vec<FileSpec>,
    pub package: Option<FileSpec>,
}

impl OnDeviceFiles {
    /// Compatibility adapter for runner descriptors, including custom test/embedded descriptors.
    pub fn from_spec(spec: &li_seg::ModelSpec) -> Self {
        let installed = spec
            .files()
            .map(|f| FileSpec {
                folder: "segmentation".into(),
                name: f.file.into(),
                bytes: f.bytes,
                sha256: f.sha256.into(),
                url: f.url.into(),
                ..Default::default()
            })
            .collect();
        let package = matches!(spec.task, li_seg::Task::Denoise { .. }).then(|| FileSpec {
            folder: "segmentation".into(),
            name: format!("{}.dtmodel", spec.id),
            bytes: spec.bytes,
            sha256: spec.sha256.into(),
            url: spec.url.into(),
            ..Default::default()
        });
        Self { installed, package }
    }

    pub fn download_bytes(&self) -> u64 {
        self.package.as_ref().map_or_else(|| self.installed.iter().map(|f| f.bytes).sum(), |f| f.bytes)
    }

    /// Visit installed members in runner order, stopping on the first failure. The offset is
    /// cumulative member bytes (archive progress is handled separately by the installer).
    pub fn visit(&self, apply: &dyn Fn(&FileSpec, u64) -> anyhow::Result<()>) -> anyhow::Result<()> {
        let mut done = 0;
        for file in &self.installed {
            apply(file, done)?;
            done += file.bytes;
        }
        Ok(())
    }
}

/// Cached projection of the one runner table into the same FileSpec used by family presets.
/// No pin, filename, license or inference setting is maintained a second time.
pub fn on_device_files(id: &str) -> Option<&'static OnDeviceFiles> {
    static FILES: std::sync::OnceLock<BTreeMap<&'static str, OnDeviceFiles>> = std::sync::OnceLock::new();
    FILES.get_or_init(|| li_seg::MODELS.iter().map(|spec| (spec.id, OnDeviceFiles::from_spec(spec))).collect()).get(id)
}

/// Default descriptor for an on-device function. Pin, files and license have one source.
pub fn on_device_default(group: li_seg::Group) -> &'static li_seg::ModelSpec {
    on_device_spec(group.official().id).expect("the runner's official model is cataloged")
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

/// How an image guides refinement for this model.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RefineMethod {
    Reference,
    Img2Img,
}

pub fn refine_method(model: ModelId) -> RefineMethod {
    if let Some(reference) = crate::refine::model_is_reference(model) {
        return if reference { RefineMethod::Reference } else { RefineMethod::Img2Img };
    }
    match model.try_info() {
        Some(i) if i.resolved.pipeline.reference == RefMethod::ReferenceLatent || i.edit && i.resolved.pipeline.reference == RefMethod::TextEncoder => {
            RefineMethod::Reference
        }
        _ => RefineMethod::Img2Img,
    }
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
    // Models without a builder (unknown files, template-only families) are checked by their own
    // recipe: the refine checker or the model setup's static check.
    let built = model.filter(|m| m.family != "unknown" && m.resolved.buildable());
    if let Some(m) = built {
        nodes.extend(crate::builders::required_nodes(&m.resolved));
    }
    nodes.sort();
    nodes.dedup();
    let missing_nodes: Vec<&str> = nodes.iter().map(String::as_str).filter(|n| !info.has_node(n)).collect();
    if !missing_nodes.is_empty() {
        return Availability { reason: format!("Update ComfyUI: it is missing the nodes {}.", missing_nodes.join(", ")), ..Default::default() };
    }
    let family_choices = built.map(|m| m.resolved.pipeline.required_choices.clone()).unwrap_or_default();
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
    if let Some(m) = built {
        for (role, name) in crate::inventory::resolve_components(&m.resolved, info) {
            files.entry(role).or_insert(name);
        }
        for c in &m.resolved.components {
            if !files.contains_key(&c.role) && preset.installed {
                missing.push(c.names.first().map(String::as_str).unwrap_or("a text encoder or VAE"));
            }
        }
        // A graph is never built with an empty loader file: a separate text encoder or VAE the
        // pipeline loads must have resolved.
        let pl = &m.resolved.pipeline;
        let needs_clip = matches!(pl.clip.node.as_str(), "CLIPLoader" | "DualCLIPLoader" | "TripleCLIPLoader" | "QuadrupleCLIPLoader");
        if missing.is_empty() && needs_clip && !files.contains_key(&Role::Clip) {
            missing.push("a text encoder");
        }
        if missing.is_empty() && pl.vae == "file" && !files.contains_key(&Role::Vae) {
            missing.push("a VAE");
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
    fn refine_method_and_default_follow_the_model_pipeline() {
        for m in [ModelId::Klein4B, ModelId::Klein9B] {
            assert_eq!(refine_method(m), RefineMethod::Reference);
            assert_eq!(refine_default_denoise(m), 1.0);
        }
        // Text-encoder edit models refine by reference too, at full denoise (verified with real refines).
        assert_eq!(refine_method(ModelId::Qwen), RefineMethod::Reference);
        assert_eq!(refine_default_denoise(ModelId::Qwen), 1.0);
        for m in [ModelId::ZImageTurbo, ModelId::Ernie, ModelId::intern("missing-refine-model")] {
            assert_eq!(refine_method(m), RefineMethod::Img2Img);
            assert_eq!(refine_default_denoise(m), 0.35);
        }
    }

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
                let family = &p.model.info().resolved;
                let core_upscaler =
                    family.kind == FamilyKind::Upscaler && family.pipeline.sampler == crate::family::SamplerStyle::UpscaleModel && f.role == Role::Upscaler;
                assert!(crate::download::host_allowed(&f.url));
                assert!(f.url.ends_with(".safetensors") || (core_upscaler && f.url.ends_with(".pth")) || crate::restoration::pinned_file(&f.name).is_some());
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
            ..Default::default()
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

    #[test]
    fn installed_weight_variants_publish_their_official_sampling_defaults() {
        let installed = [
            ("hidream", "hidream_i1_dev_fp8.safetensors", 28.0, 1.0),
            ("hidream", "hidream_i1_fast_fp8.safetensors", 16.0, 1.0),
            ("z-image", "z_image_bf16.safetensors", 25.0, 4.0),
            ("flux1", "flux1-schnell.safetensors", 4.0, 1.0),
            ("flux2-klein", "flux-2-klein-base-9b.safetensors", 20.0, 5.0),
        ];
        for (family, file, steps, cfg) in installed {
            let im = InstalledModel {
                key: format!("unet:{file}"),
                family: family.into(),
                label: file.into(),
                files: [(Role::Unet, file.into())].into(),
                ..Default::default()
            };
            let cat = build(crate::family::registry(), std::slice::from_ref(&im));
            let model = cat.models.iter().find(|m| m.id.key() == im.key).unwrap();
            assert_eq!(model.steps.default, steps);
            assert_eq!(model.guidance.default, cfg);
        }
        // Existing schema-1 profiles without the optional field retain their defaults.
        let sd15 = crate::family::registry().family("sd15").unwrap();
        assert!(sd15.file_overrides.is_empty());
        assert_eq!(sd15.resolved_for_file("custom.safetensors", None), sd15.clone());
    }
}

#[cfg(test)]
#[test]
fn final_review_realvis_lightning_uses_author_sampling_profile() {
    for file in ["realvisxlV50_v50LightningBakedvae.safetensors", "RealVisXL_V5.0_Lightning_fp16.safetensors"] {
        let im = InstalledModel {
            key: format!("ckpt:{file}"),
            family: "sdxl".into(),
            label: file.into(),
            files: [(Role::Checkpoint, file.into())].into(),
            ..Default::default()
        };
        let key = im.key.clone();
        let cat = build(crate::family::registry(), &[im]);
        let f = &cat.models.iter().find(|m| m.id.key() == key).unwrap().resolved;
        assert_eq!((f.sampling.steps.default, f.sampling.cfg.default), (5.0, 1.5));
        assert_eq!((&*f.pipeline.sampler_name, &*f.pipeline.scheduler), ("dpmpp_sde", "karras"));
        let built = crate::builders::build(f, &[(Role::Checkpoint, file.into())].into(), &crate::builders::Params::create("cup", 512, 352, 42, 5, 1.5));
        let sampler = built.graph.as_object().unwrap().values().find(|n| n["class_type"] == "KSampler").unwrap();
        assert_eq!(sampler["inputs"]["sampler_name"], "dpmpp_sde");
        assert_eq!(sampler["inputs"]["scheduler"], "karras");
        assert_eq!(sampler["inputs"]["steps"], 5);
        assert_eq!(sampler["inputs"]["cfg"], 1.5);
    }
    let generic = crate::family::registry().family("sdxl").unwrap();
    for file in ["realvisxlV50_v50Bakedvae.safetensors", "OtherXL_Lightning.safetensors"] {
        assert_eq!(generic.resolved_for_file(file, None), generic.clone());
    }
}

#[cfg(test)]
#[test]
fn lama_on_device_catalog_borrows_existing_registry_and_pins() {
    let cat = catalog();
    assert!(std::ptr::eq(cat.on_device.as_ptr(), li_seg::MODELS.as_ptr()));
    let model = on_device_default(li_seg::Group::Remove);
    assert_eq!(model.id, "big-lama");
    assert_eq!(model.sha256, li_seg::lama::MODEL.sha256);
    assert_eq!(model.file, "big-lama-fp32.onnx");
    assert_eq!(on_device_spec(model.id), Some(model));
    assert!(on_device_spec("missing-local-model").is_none());
    for group in li_seg::Group::ALL {
        assert_eq!(on_device_default(group), group.official());
    }
}

#[test]
fn registry_file_plans_preserve_all_runner_pins_and_golden_bundles() {
    let mut names = std::collections::BTreeSet::new();
    let mut members = 0;
    for spec in li_seg::MODELS {
        assert!(names.insert(spec.id), "duplicate model id");
        let files = on_device_files(spec.id).unwrap();
        assert!(std::ptr::eq(files, on_device_files(spec.id).unwrap()), "projection is cached");
        assert_eq!(files.download_bytes(), spec.download_bytes());
        assert_eq!(files.installed.len(), spec.files().count());
        for (file, old) in files.installed.iter().zip(spec.files()) {
            assert_eq!((&*file.name, &*file.url, file.bytes, &*file.sha256), (old.file, old.url, old.bytes, old.sha256));
            assert_eq!(file.folder, "segmentation");
            assert!(file.compatible.is_empty());
            assert_eq!(crate::download::target_path(std::path::Path::new("root"), file), li_seg::companion_path(std::path::Path::new("root"), old.file));
            assert!(crate::download::host_allowed(&file.url));
            assert_eq!(file.sha256.len(), 64);
            members += 1;
        }
    }
    let clip = on_device_files("clip-b32-laion").unwrap();
    assert_eq!(
        clip.installed.iter().map(|f| (&*f.name, f.bytes)).collect::<Vec<_>>(),
        [
            ("clip-vit-b32-laion2b-visual.onnx", 351613724),
            ("clip-vit-b32-laion2b-textual.onnx", 254193396),
            ("clip-vocab.json", 862328),
            ("clip-merges.txt", 524619),
        ]
    );
    assert_eq!(clip.download_bytes(), 607194067);
    let faces = on_device_files("sface").unwrap();
    assert_eq!(
        faces.installed.iter().map(|f| (&*f.name, f.bytes)).collect::<Vec<_>>(),
        [("face_recognition_sface_2021dec.onnx", 38696353), ("face_detection_yunet_2023mar.onnx", 232589),]
    );
    let raw = on_device_files("rawdenoise-nind").unwrap();
    let package = raw.package.as_ref().unwrap();
    assert_eq!(
        (&*package.name, package.bytes, &*package.sha256),
        ("rawdenoise-nind.dtmodel", 57700134, "d71b5f1e727c85a359e6f74dca9e2016c9d8fc3e2f7ac3e9b347d80ceca969af")
    );
    assert_eq!(
        raw.installed.iter().map(|f| (&*f.name, f.bytes)).collect::<Vec<_>>(),
        [("rawdenoise-nind-linear.onnx", 31053823), ("rawdenoise-nind-bayer.onnx", 31056425),]
    );
    let lama = &on_device_files("big-lama").unwrap().installed[0];
    assert_eq!((&*lama.name, lama.bytes, &*lama.sha256), ("big-lama-fp32.onnx", 208044816, "1faef5301d78db7dda502fe59966957ec4b79dd64e16f03ed96913c7a4eb68d6"));
    assert_eq!(on_device_files("bsrgan-2x"), on_device_files("bsrgan-4x"));
    assert!(on_device_files("not-a-model").is_none());
    eprintln!("descriptor parity: {} models, {members} member references", names.len());
}

#[test]
fn registry_order_and_progress_match_legacy_ui_trace() {
    let files = on_device_files("clip-b32-laion").unwrap();
    let calls = std::cell::RefCell::new(Vec::new());
    files
        .visit(&|file, offset| {
            calls.borrow_mut().push((file.name.clone(), offset));
            Ok(())
        })
        .unwrap();
    assert_eq!(
        calls.into_inner(),
        [
            ("clip-vit-b32-laion2b-visual.onnx".into(), 0),
            ("clip-vit-b32-laion2b-textual.onnx".into(), 351613724),
            ("clip-vocab.json".into(), 605807120),
            ("clip-merges.txt".into(), 606669448),
        ]
    );
    let calls = std::cell::Cell::new(0);
    assert!(
        files
            .visit(&|_, _| {
                calls.set(calls.get() + 1);
                if calls.get() == 2 {
                    anyhow::bail!("fetch failed")
                }
                Ok(())
            })
            .is_err()
    );
    assert_eq!(calls.get(), 2);
}
