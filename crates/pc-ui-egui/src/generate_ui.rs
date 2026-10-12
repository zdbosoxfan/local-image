//! local-image: the Generate | Library dock group, Open Folder, and File › Automate › Remove
//! Backgrounds (AI).
//!
//! **Generate** creates images from a prompt (Create), edits the open image from an instruction
//! (Edit) or regenerates the selection (Fill), with any local model. Results appear as tiles in
//! the panel while they are generated and are saved to the library; a tile opens as a document,
//! places as a layer, or becomes a reference for the next generation.
//!
//! **Library** is every generated image (the same store as Local Image 0.7).

use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use egui::{Color32, RichText, Sense, Stroke, StrokeKind, TextureHandle, vec2};
use image::RgbaImage;
use li_ai::catalog::{self, ModelId};
use li_ai::library::{Entry, Library};
use li_ai::{GenerateMode, GenerateRequest};
use photocraft_engine::jobs::{JobEvent, JobOutcome};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::PhotocraftApp;
use crate::theme::Tokens;
use crate::widgets;

pub(crate) mod draft_refine;
pub use draft_refine::Settings as DraftRefineSettings;

#[cfg(not(test))]
fn library() -> Library {
    Library::default()
}

#[cfg(test)]
fn library() -> Library {
    struct TestLibrary(Library);
    impl Drop for TestLibrary {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(self.0.root());
        }
    }
    thread_local! {
        static LIBRARY: TestLibrary = {
            static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
            let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/ds/test-libraries")
                .join(format!("{}-{}", std::process::id(), NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)));
            TestLibrary(Library::open(root))
        };
    }
    LIBRARY.with(|l| l.0.clone())
}

pub const GENERATE_JOB: &str = "ai.generate";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Mode {
    #[default]
    Create,
    Edit,
    Fill,
    Refine,
    Upscale,
}

impl Mode {
    const ALL: [Mode; 5] = [Mode::Create, Mode::Edit, Mode::Fill, Mode::Refine, Mode::Upscale];
    fn label(self) -> &'static str {
        match self {
            Mode::Create => "Create",
            Mode::Edit => "Edit",
            Mode::Fill => "Fill",
            Mode::Refine => "Refine",
            Mode::Upscale => "Upscale",
        }
    }
    /// The label in the UI language.
    fn shown(self) -> &'static str {
        match self {
            Mode::Create => tl!("Create"),
            Mode::Edit => tl!("Edit"),
            Mode::Fill => tl!("Fill"),
            Mode::Refine => tl!("Refine"),
            Mode::Upscale => tl!("Upscale"),
        }
    }
    fn tip(self) -> &'static str {
        match self {
            Mode::Create => tl!("A new image from a description and optional reference images"),
            Mode::Edit => tl!("Change the open image from an instruction (edit models), or restyle it"),
            Mode::Fill => tl!("Regenerate the selection (Photoshop's Generative Fill)"),
            Mode::Refine => tl!("Resample the open image at a chosen strength (img2img)"),
            Mode::Upscale => tl!("Enlarge the open image and add detail, tile by tile"),
        }
    }
    fn default_strength(self, model: ModelId) -> f32 {
        match self {
            Mode::Refine => catalog::refine_default_denoise(model),
            Mode::Fill => 1.0,
            Mode::Upscale => 0.3,
            Mode::Edit => 0.5,
            _ => 0.35,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum RefineInput {
    #[default]
    Composite,
    SelectedLayer,
}

/// A LoRA picked for the next generations.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LoraPick {
    pub name: String,
    pub strength: f32,
}

/// Saved Generate settings.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct GenerateState {
    pub mode: Mode,
    pub refine_input: RefineInput,
    pub prompt: String,
    pub negative: String,
    /// A model key (`flux2-klein-4b`, `ckpt:…`) or `custom:<workflow name>`.
    pub model: String,
    pub variant: String,
    /// `1:1`, `4:3`, `3:2`, `16:9`, `9:16`, `2:3`, `3:4`, `custom`.
    pub aspect: String,
    /// Keep width and height linked while editing the model-grid dimensions.
    pub aspect_locked: bool,
    pub width: u32,
    pub height: u32,
    pub transparent: bool,
    pub count: u32,
    pub steps: u32,
    pub guidance: f32,
    /// Z-Image Turbo Create conditioning variety; old settings default to Off.
    pub variety: f32,
    /// Fixed seed (🔒), else a new one each time.
    pub seed: Option<u64>,
    /// Strength for Edit (non-edit models), Fill, Refine and Upscale.
    pub denoise: f32,
    /// Explicit provenance for [steps, guidance, denoise] in refinement. Older saved values are explicit.
    #[serde(default)]
    pub refine_defaults: [bool; 3],
    pub advanced: bool,
    pub library_filter: String,
    /// The selected preset's name.
    pub preset: String,
    pub loras: Vec<LoraPick>,
    /// Draft → Refine.
    pub refine: bool,
    pub refine_model: String,
    pub refine_variant: String,
    pub refine_loras: Vec<LoraPick>,
    pub refine_strength: f32,
    #[serde(default)]
    pub refine_strength_default: bool,
    /// The refine pass's steps and guidance (None: the refine model's defaults).
    pub refine_steps: Option<u32>,
    pub refine_guidance: Option<f32>,
    pub refine_scale: f32,
    pub upscale: f32,
    pub enhance_method: li_ai::enhance::Method,
    pub enhance_backend: li_ai::enhance::Backend,
    /// Sampler / scheduler instead of the family's (empty = the family's).
    pub sampler: String,
    pub scheduler: String,
}

impl Default for GenerateState {
    fn default() -> Self {
        let m = ModelId::Klein4B;
        Self {
            mode: Mode::Create,
            refine_input: RefineInput::Composite,
            prompt: String::new(),
            negative: String::new(),
            model: m.key().into(),
            variant: catalog::default_variant(m).into(),
            aspect: "1:1".into(),
            aspect_locked: true,
            width: 1024,
            height: 1024,
            transparent: false,
            count: 1,
            steps: m.info().steps.default as u32,
            guidance: m.info().guidance.default,
            variety: 0.0,
            seed: None,
            denoise: 0.35,
            refine_defaults: [true; 3],
            advanced: false,
            library_filter: String::new(),
            preset: "None".into(),
            loras: Vec::new(),
            refine: false,
            refine_model: String::new(),
            refine_variant: String::new(),
            refine_loras: Vec::new(),
            refine_strength: catalog::refine_default_denoise(m),
            refine_strength_default: true,
            refine_steps: None,
            refine_guidance: None,
            refine_scale: 1.0,
            upscale: 2.0,
            enhance_method: Default::default(),
            enhance_backend: Default::default(),
            sampler: String::new(),
            scheduler: String::new(),
        }
    }
}

const ASPECTS: [(&str, f32); 7] =
    [("1:1", 1.0), ("4:3", 4.0 / 3.0), ("3:2", 1.5), ("16:9", 16.0 / 9.0), ("3:4", 0.75), ("2:3", 2.0 / 3.0), ("9:16", 9.0 / 16.0)];

/// Runtime state (UI thread only): references, results, thumbnails, the library listing.
#[derive(Default)]
struct Runtime {
    references: Vec<Reference>,
    /// Newest first: finished entries and pending jobs.
    results: VecDeque<Tile>,
    textures: HashMap<String, TextureHandle>,
    library: Arc<Vec<Entry>>,
    library_loaded: Option<Instant>,
    selected: Option<String>,
    focus_prompt: bool,
    /// Imported custom workflows (loaded on first use and after an import).
    customs: Option<Vec<li_ai::custom::CustomWorkflow>>,
    presets: Option<Vec<li_ai::presets::GenPreset>>,
    preset_name: String,
    /// Upscale lists every image model, not just the official upscalers.
    upscale_any: bool,
    learned_refine: HashMap<&'static str, (ModelId, String, li_ai::refine::Defaults)>,
}

struct Reference {
    name: String,
    image: Arc<RgbaImage>,
    texture: Option<TextureHandle>,
}

enum Tile {
    /// `edit`: the document the result lands on as a layer; `open`: open it as a new document.
    Pending {
        job: u64,
        label: String,
        edit: Option<u64>,
        open: bool,
    },
    Done(Entry),
    Failed(String),
}

fn rt<R>(f: impl FnOnce(&mut Runtime) -> R) -> R {
    thread_local! { static R: std::cell::RefCell<Runtime> = std::cell::RefCell::new(Runtime::default()); }
    R.with(|r| f(&mut r.borrow_mut()))
}

pub fn focus_prompt(ctx: &egui::Context) {
    rt(|r| r.focus_prompt = true);
    ctx.request_repaint();
}

/// Re-reads the custom workflows (after an import or a template install).
pub fn reload_customs() {
    CUSTOMS_STALE.store(true, std::sync::atomic::Ordering::SeqCst);
}

static CUSTOMS_STALE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

fn customs() -> Vec<li_ai::custom::CustomWorkflow> {
    if CUSTOMS_STALE.swap(false, std::sync::atomic::Ordering::SeqCst) {
        rt(|r| r.customs = None);
    }
    rt(|r| r.customs.get_or_insert_with(|| li_ai::custom::list(&li_ai::custom::dir())).clone())
}

fn preset_path() -> PathBuf {
    #[cfg(test)]
    {
        library().root().join("presets.json")
    }
    #[cfg(not(test))]
    {
        li_ai::presets::path()
    }
}

fn presets() -> Vec<li_ai::presets::GenPreset> {
    rt(|r| r.presets.get_or_insert_with(|| li_ai::presets::all(&preset_path())).clone())
}

fn custom_of(s: &GenerateState) -> Option<li_ai::custom::CustomWorkflow> {
    let name = s.model.strip_prefix("custom:")?;
    customs().into_iter().find(|w| w.name == name)
}

/// The cloud model chosen (`cloud:provider:model`).
fn cloud_of(s: &GenerateState) -> Option<&'static li_ai::cloud::CloudModel> {
    li_ai::cloud::model(s.model.strip_prefix("cloud:")?)
}

fn cloud_fits(m: &li_ai::cloud::CloudModel, mode: Mode) -> bool {
    match mode {
        Mode::Create => m.create,
        Mode::Edit => m.edit,
        Mode::Fill => m.fill,
        Mode::Refine | Mode::Upscale => false,
    }
}

/// Providers with a key entered (re-read from the settings every two seconds, not every frame).
fn cloud_keys() -> Vec<li_ai::cloud::Provider> {
    thread_local! { static CACHE: std::cell::RefCell<Option<(Instant, Vec<li_ai::cloud::Provider>)>> = const { std::cell::RefCell::new(None) }; }
    CACHE.with(|c| {
        let mut c = c.borrow_mut();
        if let Some((t, v)) = c.as_ref()
            && t.elapsed() < Duration::from_secs(2)
        {
            return v.clone();
        }
        let settings = li_ai::settings::AiSettings::load();
        let v: Vec<_> = li_ai::cloud::Provider::ALL.into_iter().filter(|p| li_ai::cloud::api_key(&settings, *p).is_some()).collect();
        *c = Some((Instant::now(), v.clone()));
        v
    })
}

/// Whether the cloud model's key is entered.
fn cloud_ready(m: &li_ai::cloud::CloudModel) -> Result<(), String> {
    if cloud_keys().contains(&m.provider) {
        Ok(())
    } else {
        Err(crate::i18n::fmt(tl!("Add your {provider} API key in Local AI › Cloud."), &[("provider", m.provider.label())]))
    }
}

fn model_of(s: &GenerateState) -> ModelId {
    ModelId::from_key(&s.model).filter(|m| m.try_info().is_some()).unwrap_or(ModelId::Klein4B)
}

/// Keeps sampling inside what the connected ComfyUI accepts (its sampler's reported limits); with no
/// engine connected only "at least one step" applies.
fn fit_to_backend(s: &mut GenerateState, limits: Option<li_ai::comfy::SamplerLimits>) {
    let info = model_of(s).info();
    s.steps = s.steps.max(1);
    if !s.guidance.is_finite() {
        s.guidance = info.guidance.default;
    }
    if !s.denoise.is_finite() {
        s.denoise = if s.mode == Mode::Create { info.resolved.sampling.denoise } else { s.mode.default_strength(info.id) };
    }
    if let Some(l) = limits {
        s.steps = s.steps.clamp(l.steps.0.max(1.0) as u32, l.steps.1.max(l.steps.0).max(1.0) as u32);
        s.guidance = s.guidance.clamp(l.cfg.0, l.cfg.1.max(l.cfg.0));
        s.denoise = s.denoise.clamp(l.denoise.0, l.denoise.1.max(l.denoise.0));
    }
}

/// "Recommended: 4–8 (default 6)" or "Recommended: 8" for a model's suggested range.
fn range_hint(min: f32, max: f32, default: f32, decimals: usize) -> String {
    let f = |v: f32| format!("{v:.decimals$}");
    if min == max {
        crate::i18n::fmt(tl!("Recommended: {value}"), &[("value", &f(default))])
    } else {
        crate::i18n::fmt(tl!("Recommended: {min}–{max} (default {default})"), &[("min", &f(min)), ("max", &f(max)), ("default", &f(default))])
    }
}

// The shared number widget reports numeric changes, while provenance also needs text
// entry of the same number (including a value equal to the model's recommendation).
fn number_edited(response: &egui::Response) -> bool {
    response.changed()
        || (response.has_focus() && response.ctx.input(|i| i.events.iter().any(|event| matches!(event, egui::Event::Text(_) | egui::Event::Paste(_)))))
}

/// Number fields use the Properties value field; model recommendations never become slider ranges.
fn number_row(ui: &mut egui::Ui, label: &str, v: &mut u32, lo: u32, hi: u32, hint: Option<String>) -> bool {
    *v = (*v).clamp(lo, hi.max(lo));
    number_edited(&value_row(ui, label, v, lo..=hi.max(lo), hint, 0))
}

fn value_row<T: egui::emath::Numeric>(
    ui: &mut egui::Ui,
    label: &str,
    v: &mut T,
    range: std::ops::RangeInclusive<T>,
    hint: Option<String>,
    decimals: usize,
) -> egui::Response {
    let t = Tokens::get(ui.ctx());
    let response = ui
        .horizontal(|ui| {
            let label = ui.label(RichText::new(label).color(t.text_dim));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                widgets::value_field_with_precision(ui, v, range, "", 92.0, decimals).labelled_by(label.id)
            })
            .inner
        })
        .inner;
    if let Some(h) = hint {
        ui.label(RichText::new(h).font(crate::theme::medium(11.0)).color(t.text_faint));
    }
    response
}

fn guidance_row(ui: &mut egui::Ui, v: &mut f32, lo: f32, hi: f32, hint: Option<String>) -> bool {
    guidance_row_labeled(ui, tl!("Guidance"), v, lo, hi, hint)
}

fn denoise_row(ui: &mut egui::Ui, label: &str, v: &mut f32, lo: f32, hi: f32) -> bool {
    number_edited(
        &value_row(ui, label, v, lo..=hi.max(lo), None, 2).on_hover_text(tl!("ComfyUI denoise: 1 regenerates the image, lower values keep more of it")),
    )
}

fn guidance_row_labeled(ui: &mut egui::Ui, label: &str, v: &mut f32, lo: f32, hi: f32, hint: Option<String>) -> bool {
    number_edited(&value_row(ui, label, v, lo..=hi.max(lo), hint, 1))
}

fn sampling_ui(ui: &mut egui::Ui, s: &mut GenerateState, limits: Option<li_ai::comfy::SamplerLimits>, denoise: bool) {
    fit_to_backend(s, limits);
    let info = model_of(s).info();
    let (slo, shi) = limits.map_or((1.0, u32::MAX as f32), |l| l.steps);
    let steps_changed = number_row(
        ui,
        tl!("Steps"),
        &mut s.steps,
        slo.max(1.0) as u32,
        shi.max(1.0) as u32,
        Some(range_hint(info.steps.min, info.steps.max, info.steps.default, 0)),
    );
    let (clo, chi) = limits.map_or((f32::MIN, f32::MAX), |l| l.cfg);
    let guidance_changed = guidance_row(ui, &mut s.guidance, clo, chi, Some(range_hint(info.guidance.min, info.guidance.max, info.guidance.default, 1)));
    if steps_changed {
        s.refine_defaults[0] = false;
    }
    if guidance_changed {
        s.refine_defaults[1] = false;
    }
    if info.id == ModelId::ZImageTurbo && s.mode == Mode::Create {
        if !s.variety.is_finite() {
            s.variety = 0.0;
        }
        s.variety = s.variety.clamp(0.0, 100.0);
        ui.horizontal(|ui| {
            label(ui, tl!("Variety"));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                widgets::value_field(ui, &mut s.variety, 0.0..=100.0, "%", 74.0);
                if widgets::secondary_button(ui, tl!("Off"), 0.0).clicked() {
                    s.variety = 0.0;
                }
            });
        });
        let r = widgets::slider(ui, &mut s.variety, 0.0..=100.0, None);
        r.widget_info(|| egui::WidgetInfo::slider(ui.is_enabled(), s.variety as f64, tl!("Variety")));
        r.on_hover_text(tl!("Higher Variety can change compositions and loosen prompt details. Set to 0 to turn it off."));
    }
    if denoise {
        let (lo, hi) = limits.map_or((f32::MIN, f32::MAX), |l| l.denoise);
        if denoise_row(ui, tl!("Denoise"), &mut s.denoise, lo, hi) {
            s.refine_defaults[2] = false;
        }
    }
}

/// The Generate size component used by both the panel and Draft / Refinement.
/// Presets and edited dimensions use the selected model's pixel grid; merely opening an image
/// never changes its dimensions.
fn size_controls(ui: &mut egui::Ui, s: &mut GenerateState) {
    let t = Tokens::get(ui.ctx());
    let sizes = model_of(s).info().resolved.sizes;
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = vec2(crate::theme::ROW_GAP, crate::theme::ROW_GAP);
        for (aspect, _) in ASPECTS {
            if widgets::pill_tab(ui, aspect, s.aspect == aspect).clicked() {
                s.aspect = aspect.into();
                s.aspect_locked = true;
                (s.width, s.height) = size_for(aspect, s, sizes.native);
            }
        }
    });
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = crate::theme::ROW_GAP;
        let (mut w, mut h) = (s.width, s.height);
        let lo = if s.mode == Mode::Refine { 1 } else { sizes.min };
        let hi = sizes.max.max(w).max(h);
        let fw = ((ui.available_width() - 70.0) / 2.0).clamp(56.0, 112.0);
        ui.label(RichText::new("W").font(crate::theme::medium(11.0)).color(t.text_dim));
        let width = widgets::value_field_with_precision(ui, &mut w, lo..=hi, "px", fw, 0).on_hover_text(tl!("Width"));
        width.widget_info(|| egui::WidgetInfo { label: Some(tl!("Width").into()), ..egui::WidgetInfo::drag_value(ui.is_enabled(), w as f64) });
        let lock = crate::icons::button(ui, if s.aspect_locked { "link" } else { "unlink" }, 22.0, s.aspect_locked, tl!("Link width and height"));
        lock.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Button, ui.is_enabled(), s.aspect_locked, tl!("Link width and height")));
        if lock.clicked() {
            s.aspect_locked = !s.aspect_locked;
        }
        ui.label(RichText::new("H").font(crate::theme::medium(11.0)).color(t.text_dim));
        let height = widgets::value_field_with_precision(ui, &mut h, lo..=hi, "px", fw, 0).on_hover_text(tl!("Height"));
        height.widget_info(|| egui::WidgetInfo { label: Some(tl!("Height").into()), ..egui::WidgetInfo::drag_value(ui.is_enabled(), h as f64) });
        if width.changed() || height.changed() {
            if s.aspect_locked {
                let ratio = ASPECTS.iter().find(|(k, _)| *k == s.aspect).map_or(s.width as f32 / s.height.max(1) as f32, |(_, r)| *r);
                if width.changed() {
                    h = (w as f32 / ratio).round().max(1.0) as u32;
                } else {
                    w = (h as f32 * ratio).round().max(1.0) as u32;
                }
            } else {
                s.aspect = "custom".into();
            }
            (s.width, s.height) = (w, h);
        }
        if ((width.changed() || height.changed()) && !(width.has_focus() || height.has_focus())) || width.lost_focus() || height.lost_focus() {
            (s.width, s.height) = li_ai::imaging::snap_size(s.width, s.height, sizes.multiple.max(8));
        }
    });
    ui.label(
        RichText::new(crate::i18n::fmt(tl!("Grid: {multiple} px"), &[("multiple", &sizes.multiple.max(8).to_string())]))
            .font(crate::theme::medium(11.0))
            .color(t.text_faint),
    );
}

fn variant_picker(ui: &mut egui::Ui, model: ModelId, variant: &mut String) {
    let presets: Vec<_> = catalog::presets_for(model).collect();
    if presets.len() > 1 {
        ui.horizontal(|ui| {
            label(ui, tl!("Precision"));
            let opts: Vec<_> = presets.iter().map(|p| (p.variant.clone(), p.label.as_str())).collect();
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                widgets::dropdown(ui, "gen-variant", variant, &opts, ui.available_width().min(240.0));
            });
        });
    }
}

/// Apply a newly verified recipe's recommendations once, preserving values the user edited
/// while the checker ran. This matters when fallback changes full-reference denoise to img2img.
fn inherit_refine_defaults(id: &'static str, s: &mut GenerateState) {
    let model = model_of(s);
    let Some(defaults) = li_ai::refine::model_defaults(model) else { return };
    let changed = rt(|r| {
        let value = (model, s.variant.clone(), defaults.clone());
        if r.learned_refine.get(id) == Some(&value) {
            return false;
        }
        r.learned_refine.insert(id, value);
        true
    });
    if !changed {
        return;
    }
    if s.refine_defaults[0] {
        s.steps = defaults.steps;
    }
    if s.refine_defaults[1] {
        s.guidance = defaults.cfg;
    }
    if s.refine_defaults[2] {
        s.denoise = defaults.denoise;
    }
}

fn set_model(s: &mut GenerateState, m: ModelId) {
    let info = m.info();
    s.model = m.key().into();
    s.variant = catalog::default_variant(m).into();
    s.steps = info.steps.default as u32;
    s.refine_defaults = [true; 3];
    s.variety = 0.0;
    s.guidance = info.guidance.default;
    if s.mode == Mode::Refine
        && let Some(d) = li_ai::refine::model_defaults(m)
    {
        s.steps = d.steps;
        s.guidance = d.cfg;
    }
    s.denoise = if s.mode == Mode::Create { info.resolved.sampling.denoise } else { s.mode.default_strength(m) };
    s.sampler.clear();
    s.scheduler.clear();
    if !info.transparent {
        s.transparent = false;
    }
    // LoRAs that don't fit the new family are dropped.
    let fam = info.resolved.clone();
    let fits: Vec<String> = li_ai::inventory::loras_for(&fam).into_iter().map(|(l, _)| l.name).collect();
    s.loras.retain(|l| fits.contains(&l.name));
    if s.mode == Mode::Create && !info.text_to_image {
        s.mode = Mode::Refine;
        s.denoise = s.mode.default_strength(m);
    }
}

/// Whether a model can run `mode` (Upscale: the official upscalers, or any image model once
/// "Choose another model…" was picked this session).
fn fits_mode(info: &catalog::ModelInfo, mode: Mode) -> bool {
    fits_mode_with(info, mode, upscale_any())
}

/// [`fits_mode`]; `any_upscaler`: Upscale lists every image model (diffusion refine-upscale)
/// instead of the official upscalers (SeedVR2).
fn fits_mode_with(info: &catalog::ModelInfo, mode: Mode, any_upscaler: bool) -> bool {
    if mode == Mode::Upscale && !any_upscaler {
        return is_upscaler(info);
    }
    if info.tool || info.resolved.kind != li_ai::family::FamilyKind::Image {
        return false;
    }
    match mode {
        Mode::Create => info.text_to_image,
        Mode::Fill => info.inpaint,
        Mode::Refine => !info.upscale && (info.text_to_image || info.edit || info.family == "unknown"),
        Mode::Edit | Mode::Upscale => true,
    }
}

/// A supported upscaler: runs through the shared Enhance service.
fn is_upscaler(info: &catalog::ModelInfo) -> bool {
    info.upscale
}

/// Upscale with a supported restoration model goes to `ai.enhance`, not a diffusion
/// refine-upscale.
fn runs_as_enhance(info: &catalog::ModelInfo, mode: Mode) -> bool {
    mode == Mode::Upscale && is_upscaler(info)
}

/// "Choose another model…" in Upscale mode (this session only).
fn upscale_any() -> bool {
    rt(|r| r.upscale_any)
}

fn restore_upscale_choice(s: &GenerateState) {
    if s.mode == Mode::Upscale
        && !upscale_any()
        && (s.model.starts_with("custom:")
            || s.model.starts_with("cloud:")
            || ModelId::from_key(&s.model).and_then(|m| m.try_info()).is_some_and(|info| !is_upscaler(info)))
    {
        rt(|r| r.upscale_any = true);
    }
}

fn size_for(aspect: &str, s: &GenerateState, native: u32) -> (u32, u32) {
    let Some((_, r)) = ASPECTS.iter().find(|(k, _)| *k == aspect) else { return (s.width, s.height) };
    // About the model's native pixel count, snapped to its pixel grid.
    let area = native as f32 * native as f32;
    let h = (area / r).sqrt();
    li_ai::imaging::snap_size((h * r) as u32, h as u32, model_of(s).info().resolved.sizes.multiple.max(8))
}

fn thumb_texture(ctx: &egui::Context, key: &str, path: &std::path::Path) -> Option<TextureHandle> {
    if let Some(t) = rt(|r| r.textures.get(key).cloned()) {
        return Some(t);
    }
    let img = image::open(path).ok()?.to_rgba8();
    let ci = egui::ColorImage::from_rgba_unmultiplied([img.width() as usize, img.height() as usize], img.as_raw());
    let tex = ctx.load_texture(format!("gen-{key}"), ci, egui::TextureOptions::LINEAR);
    rt(|r| r.textures.insert(key.to_owned(), tex.clone()));
    Some(tex)
}

fn image_texture(ctx: &egui::Context, key: &str, img: &RgbaImage) -> TextureHandle {
    let t = image::imageops::thumbnail(img, 160.min(img.width()).max(1), (160 * img.height() / img.width().max(1)).clamp(1, 160));
    let ci = egui::ColorImage::from_rgba_unmultiplied([t.width() as usize, t.height() as usize], t.as_raw());
    ctx.load_texture(format!("ref-{key}"), ci, egui::TextureOptions::LINEAR)
}

fn label(ui: &mut egui::Ui, s: &str) {
    let t = Tokens::get(ui.ctx());
    ui.label(RichText::new(s).color(t.text_dim));
}

/// A row of chip buttons; returns the clicked index.
fn chips(ui: &mut egui::Ui, items: &[&str], selected: usize) -> Option<usize> {
    chips_with(ui, items, selected, &[], &[])
}

/// Chips with tooltips; `disabled[i]` greys a chip out (with its reason as tooltip).
fn chips_with(ui: &mut egui::Ui, items: &[&str], selected: usize, tips: &[&str], disabled: &[Option<&str>]) -> Option<usize> {
    let t = Tokens::get(ui.ctx());
    let mut hit = None;
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = vec2(4.0, 4.0);
        for (i, s) in items.iter().enumerate() {
            let on = i == selected;
            let off = disabled.get(i).copied().flatten();
            let ink = if on {
                t.accent_text
            } else if off.is_some() {
                t.text_faint
            } else {
                t.text_dim
            };
            let galley = ui.painter().layout_no_wrap((*s).to_owned(), crate::theme::medium(12.0), ink);
            let (r, resp) = ui.allocate_exact_size(vec2(galley.size().x + 16.0, 22.0), Sense::click());
            let fill = if on {
                t.accent
            } else if resp.hovered() && off.is_none() {
                t.hover
            } else {
                t.field
            };
            ui.painter().rect_filled(r, 11.0, fill);
            ui.painter().galley(r.center() - galley.size() / 2.0, galley, t.text);
            resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, off.is_none(), *s));
            let resp = match (off, tips.get(i)) {
                (Some(why), _) => resp.on_hover_text(why),
                (None, Some(tip)) => resp.on_hover_text(*tip),
                _ => resp,
            };
            if resp.clicked() && off.is_none() {
                hit = Some(i);
            }
        }
    });
    hit
}

/// A small capability tag.
fn tag(ui: &mut egui::Ui, text: &str, t: &Tokens) {
    let galley = ui.painter().layout_no_wrap(text.to_owned(), egui::FontId::proportional(10.5), t.text_dim);
    let (r, _) = ui.allocate_exact_size(vec2(galley.size().x + 10.0, 16.0), Sense::hover());
    ui.painter().rect_filled(r, 8.0, t.field);
    ui.painter().galley(r.center() - galley.size() / 2.0, galley, t.text_dim);
}

fn model_variety_tip(model: ModelId) -> Option<&'static str> {
    (model == ModelId::ZImageTurbo)
        .then(|| tl!("Low variety by design: different seeds give similar layouts. Change the prompt (or raise Variety) for different compositions."))
}

/// Shared by Generate and Draft, scoped to the Turbo model rather than the Z-Image family.
fn model_variety_note(ui: &mut egui::Ui, model: ModelId) {
    if let Some(tip) = model_variety_tip(model) {
        let t = Tokens::get(ui.ctx());
        ui.horizontal_wrapped(|ui| {
            let (r, _) = ui.allocate_exact_size(vec2(14.0, 14.0), Sense::hover());
            crate::icons::paint(ui, r, "info", 12.0, t.text_faint);
            ui.add(egui::Label::new(RichText::new(tip).color(t.text_faint).size(11.0)).wrap());
        });
    }
}

/// Readiness of a model on the running engine: `Ok` ready, `Err(reason)` otherwise.
fn readiness(st: &crate::ai_ui::EngineStatus, m: ModelId, variant: &str) -> Result<(), String> {
    st.ready(m, variant)
}

fn refine_readiness(st: &crate::ai_ui::EngineStatus, m: ModelId, variant: &str) -> Result<(), String> {
    readiness(st, m, variant)?;
    match li_ai::refine::checker().status_for_variant(&st.host, m, variant) {
        li_ai::refine::Status::Failed(reason) => Err(format!("Refine not supported ({reason})")),
        li_ai::refine::Status::Testing => Err("testing…".into()),
        _ => Ok(()),
    }
}

/// Shared readiness hint and the action that resolves it in either Generate surface.
fn readiness_warning(app: &mut PhotocraftApp, ui: &mut egui::Ui, why: &str, st: &crate::ai_ui::EngineStatus, cloud: bool) {
    let t = Tokens::get(ui.ctx());
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new(why).font(crate::theme::medium(11.5)).color(t.warning));
        if cloud {
            if widgets::secondary_button(ui, tl!("Add Key…"), 0.0).clicked() {
                crate::ai_ui::open_local_ai(app);
            }
        } else if !st.connected {
            if widgets::secondary_button(ui, tl!("Set up AI…"), 0.0).clicked() {
                crate::ai_ui::open_local_ai(app);
            }
        } else if widgets::secondary_button(ui, tl!("Get models…"), 0.0).clicked() {
            crate::model_browser::open();
        }
    });
}

/// The model picker: models grouped by family group, each with a readiness dot and its capability
/// tags; custom workflows last; Browse Models… and Import Workflow… at the bottom. Returns the
/// chosen key.
fn model_groups(models: &[catalog::ModelInfo], mode: Mode) -> Vec<(&str, Vec<&catalog::ModelInfo>)> {
    if mode == Mode::Refine {
        let mut groups = vec![(tl!("Edit models — refine by reference"), Vec::new()), (tl!("Image models — refine by denoise"), Vec::new())];
        for m in models.iter().filter(|m| fits_mode(m, mode)) {
            let group = match catalog::refine_method(m.id) {
                catalog::RefineMethod::Reference => 0,
                catalog::RefineMethod::Img2Img => 1,
            };
            groups[group].1.push(m);
        }
        groups.retain(|(_, models)| !models.is_empty());
        groups
    } else {
        let mut groups: Vec<(&str, Vec<&catalog::ModelInfo>)> = Vec::new();
        for m in models.iter().filter(|m| fits_mode(m, mode)) {
            match groups.iter_mut().find(|(g, _)| *g == m.group) {
                Some((_, v)) => v.push(m),
                None => groups.push((m.group.as_str(), vec![m])),
            }
        }
        groups
    }
}

fn model_picker(ui: &mut egui::Ui, id: &str, current: (&str, &str), mode: Mode, st: &crate::ai_ui::EngineStatus, allow_custom: bool) -> Option<String> {
    let (current, selected_variant) = current;
    let t = Tokens::get(ui.ctx());
    let custom = current.strip_prefix("custom:");
    let current_label = match custom {
        Some(name) => crate::i18n::fmt(tl!("{name} · Custom workflow"), &[("name", name)]),
        None => ModelId::from_key(current)
            .and_then(|m| m.try_info())
            .map(|i| format!("{} · {}", i.label, i.family_label))
            .unwrap_or_else(|| tl!("Choose a model").into()),
    };
    let mut picked = None;
    let width = ui.available_width();
    widgets::dropdown_menu(ui, id, &current_label, width, |ui| {
        ui.set_min_width(width.max(300.0));
        let cat = catalog::catalog();
        for (g, models) in model_groups(&cat.models, mode) {
            ui.add_space(4.0);
            let heading = if mode == Mode::Refine { g.to_owned() } else { g.to_uppercase() };
            ui.label(RichText::new(heading).size(10.5).color(t.text_faint).strong());
            for m in models {
                let variant = if current == m.id.key() { selected_variant } else { catalog::default_variant(m.id) };
                let ok = catalog::presets_for(m.id).any(|p| readiness(st, m.id, &p.variant).is_ok());
                let reason = readiness(st, m.id, variant).err().unwrap_or_default();
                let refine_status =
                    if mode == Mode::Refine { li_ai::refine::checker().status_for_variant(&st.host, m.id, variant) } else { li_ai::refine::Status::Unchecked };
                let failed = matches!(refine_status, li_ai::refine::Status::Failed(_));
                let ok = ok && !failed && refine_status != li_ai::refine::Status::Testing;
                let row = ui.horizontal(|ui| {
                    let (dot, _) = ui.allocate_exact_size(vec2(10.0, 16.0), Sense::hover());
                    let colour = if ok {
                        t.accent
                    } else if m.origin == catalog::Origin::Installed {
                        t.warning
                    } else {
                        t.text_faint
                    };
                    ui.painter().circle_filled(dot.center(), 3.5, colour);
                    let sel = current == m.id.key();
                    let resp = ui.add_enabled(
                        !failed,
                        egui::Button::selectable(sel, RichText::new(&m.label).color(if ok || !st.connected { t.text } else { t.text_dim })),
                    );
                    if mode == Mode::Refine && refine_status != li_ai::refine::Status::Unchecked {
                        if refine_status == li_ai::refine::Status::Testing {
                            if ui.small_button(tl!("Cancel test")).clicked() {
                                li_ai::refine::checker().cancel_variant(&li_ai::ComfyClient::new(&st.host), m.id, variant);
                            }
                        } else if ui.small_button(tl!("Re-test")).clicked() {
                            li_ai::refine::checker().request(li_ai::ComfyClient::new(&st.host), m.id, variant.into(), true);
                        }
                    }
                    ui.label(RichText::new(&m.family_label).size(10.5).color(t.text_faint));
                    for tg in m.tags().iter().take(4) {
                        tag(ui, tg, &t);
                    }
                    resp
                });
                match &refine_status {
                    li_ai::refine::Status::Testing => {
                        ui.label(tl!("testing…"));
                    }
                    li_ai::refine::Status::Failed(reason) => {
                        ui.add(egui::Label::new(RichText::new(format!("Refine not supported ({reason})")).size(10.5).color(t.warning)).truncate())
                            .on_hover_text(reason);
                    }
                    _ => {}
                }
                let mut tip = if ok {
                    format!("{}\n{}", m.best_for, m.tags().join(" · "))
                } else if m.origin == catalog::Origin::Profile {
                    format!("{}\n{}", m.best_for, crate::i18n::fmt(tl!("Not installed: {reason}"), &[("reason", &reason)]))
                } else {
                    format!("{}\n{}", m.best_for, reason)
                };
                if let Some(variety) = model_variety_tip(m.id) {
                    tip.push('\n');
                    tip.push_str(variety);
                }
                if row.inner.on_hover_text(tip).clicked() {
                    picked = Some(m.id.key().to_owned());
                }
            }
        }
        if allow_custom && mode != Mode::Refine {
            let clouds: Vec<_> = li_ai::cloud::MODELS.iter().filter(|m| cloud_fits(m, mode)).collect();
            if !clouds.is_empty() {
                ui.add_space(4.0);
                ui.label(RichText::new(tl!("CLOUD · YOUR API KEY")).size(10.5).color(t.text_faint).strong());
                let keys = cloud_keys();
                for m in clouds {
                    let key = format!("cloud:{}", m.key);
                    let has_key = keys.contains(&m.provider);
                    let row = ui.horizontal(|ui| {
                        let (dot, _) = ui.allocate_exact_size(vec2(10.0, 16.0), Sense::hover());
                        ui.painter().circle_filled(dot.center(), 3.5, if has_key { t.accent } else { t.text_faint });
                        let resp = ui.add(egui::Button::selectable(current == key, RichText::new(m.label).color(if has_key { t.text } else { t.text_dim })));
                        ui.label(RichText::new(m.provider.label()).size(10.5).color(t.text_faint));
                        for (on, tg) in [(m.create, tl!("Create")), (m.edit, tl!("Edit")), (m.fill, tl!("Fill"))] {
                            if on {
                                tag(ui, tg, &t);
                            }
                        }
                        resp
                    });
                    let tip = if has_key {
                        format!(
                            "{}\n{}",
                            m.best_for,
                            crate::i18n::fmt(tl!("Runs on {provider}'s servers: your prompt and images are sent there."), &[("provider", m.provider.label())])
                        )
                    } else {
                        format!(
                            "{}\n{}",
                            m.best_for,
                            crate::i18n::fmt(tl!("Add your {provider} API key in Local AI › Cloud to use it."), &[("provider", m.provider.label())])
                        )
                    };
                    if row.inner.on_hover_text(tip).clicked() {
                        picked = Some(key);
                    }
                }
            }
            let cws = customs();
            if !cws.is_empty() {
                ui.add_space(4.0);
                ui.label(RichText::new(tl!("CUSTOM WORKFLOWS")).size(10.5).color(t.text_faint).strong());
                for w in cws {
                    let key = format!("custom:{}", w.name);
                    let row = ui.horizontal(|ui| {
                        ui.add_space(14.0);
                        let resp = ui.add(egui::Button::selectable(current == key, &w.name));
                        for tg in w.tags().iter().take(4) {
                            tag(ui, tg, &t);
                        }
                        resp
                    });
                    if row.inner.clicked() {
                        picked = Some(key);
                    }
                }
            }
        }
        ui.separator();
        ui.horizontal_wrapped(|ui| {
            if widgets::secondary_button(ui, tl!("Browse Models…"), 0.0).clicked() {
                picked = Some("browse:".into());
            }
            if widgets::secondary_button(ui, tl!("Browse LoRAs…"), 0.0).on_hover_text(tl!("Find LoRAs (styles) for the chosen model's family")).clicked() {
                picked = Some("browse-loras:".into());
            }
            if allow_custom && mode != Mode::Refine && widgets::secondary_button(ui, tl!("Import Workflow…"), 0.0).clicked() {
                picked = Some("import:".into());
            }
        });
    });
    if mode == Mode::Refine
        && st.connected
        && let Some(m) = ModelId::from_key(current)
    {
        let variant = selected_variant;
        if st.ready(m, variant).is_ok() {
            #[cfg(not(test))]
            li_ai::refine::checker().request(li_ai::ComfyClient::new(&st.host), m, variant.into(), false);
        }
        if li_ai::refine::checker().status_for_variant(&st.host, m, variant) == li_ai::refine::Status::Testing {
            ui.label(tl!("testing…"));
            ui.ctx().request_repaint_after(Duration::from_millis(200));
        }
    }
    if mode == Mode::Refine
        && let Some(m) = picked.as_deref().and_then(ModelId::from_key).or_else(|| ModelId::from_key(current))
    {
        let hint = match catalog::refine_method(m) {
            catalog::RefineMethod::Reference => tl!("Refines by reference: the image guides the result"),
            catalog::RefineMethod::Img2Img => tl!("Refines by denoise: lower keeps more of the image"),
        };
        ui.label(RichText::new(hint).size(11.0).color(t.text_faint));
    }
    picked
}

/// The Model Browser on LoRAs for `model`'s family (or on LoRAs in general).
fn browse_loras(model: &str) {
    match ModelId::from_key(model).and_then(|m| m.try_info()) {
        Some(info) => crate::model_browser::open_loras(&info.family),
        None => crate::model_browser::open_loras_any(),
    }
}

/// The Generate tab.
pub fn panel(app: &mut PhotocraftApp, ui: &mut egui::Ui) {
    let t = Tokens::get(ui.ctx());
    let ctx = ui.ctx().clone();
    if widgets::secondary_button(ui, tl!("Draft / Refinement…"), ui.available_width()).clicked() {
        draft_refine::open(app);
    }
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.spacing_mut().item_spacing.y = 6.0;
        let has_doc = app.session.active().is_some();
        let has_active_layer = app.session.active().is_some_and(|d| d.active_layer.is_some());
        let has_sel = app.session.active().is_some_and(|d| d.doc.selection.is_some());
        let st = crate::ai_ui::status();
        let s = &mut app.ui.ai.generate;
        // ---- Mode.
        let labels: Vec<&str> = Mode::ALL.iter().map(|m| m.shown()).collect();
        let tips: Vec<&str> = Mode::ALL.iter().map(|m| m.tip()).collect();
        let disabled: Vec<Option<&str>> = Mode::ALL
            .iter()
            .map(|m| match m {
                Mode::Create => None,
                Mode::Fill if !has_sel => Some(tl!("Make a selection first")),
                _ if !has_doc => Some(tl!("Open an image first")),
                _ => None,
            })
            .collect();
        let cur = Mode::ALL.iter().position(|m| *m == s.mode).unwrap_or(0);
        if let Some(i) = chips_with(ui, &labels, cur, &tips, &disabled) {
            s.mode = Mode::ALL[i];
            s.denoise = s.mode.default_strength(model_of(s));
            s.refine_defaults[2] = true;
            // A model that can't run the new mode gives way to one that can.
            let m = model_of(s);
            let cloud_ok = cloud_of(s).map(|c| cloud_fits(c, s.mode));
            if cloud_ok == Some(true) {
                // The cloud model handles this mode.
            } else if !s.model.starts_with("custom:") && (cloud_ok == Some(false) || !fits_mode(m.info(), s.mode)) {
                let next = ModelId::generators()
                    .into_iter()
                    .chain(ModelId::all())
                    .find(|x| fits_mode(x.info(), s.mode) && catalog::presets_for(*x).any(|p| st.ready(*x, &p.variant).is_ok()))
                    .or_else(|| ModelId::all().into_iter().find(|x| fits_mode(x.info(), s.mode)));
                if let Some(n) = next {
                    set_model(s, n);
                }
            }
        }
        restore_upscale_choice(s);
        if s.mode == Mode::Upscale && !upscale_any() {
            crate::ai_ui::enhance_controls(ui, &mut s.enhance_method, &mut s.enhance_backend, &mut s.upscale);
            let ready = crate::ai_ui::enhance_ready(s.enhance_method, s.enhance_backend, s.upscale as u32);
            if let Err(why) = &ready {
                ui.label(RichText::new(why).color(t.warning));
            }
            let clicked = ui.add_enabled_ui(has_doc && ready.is_ok(), |ui| widgets::primary_button(ui, tl!("Upscale"), ui.available_width())).inner.clicked();
            if widgets::secondary_button(ui, tl!("Choose another model…"), ui.available_width()).clicked() {
                rt(|r| r.upscale_any = true);
            }
            if clicked {
                start(app, &ctx);
            }
            results_ui(app, ui);
            return;
        }
        // ---- Preset.
        let all_presets = presets();
        ui.horizontal(|ui| {
            label(ui, tl!("Preset"));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let save = crate::icons::button(ui, "bookmark-plus", 22.0, false, tl!("Save these settings as a preset"));
                #[cfg(test)]
                ui.ctx().data_mut(|d| d.insert_temp(egui::Id::new("gen-save-preset-test"), save.rect));
                if save.clicked() {
                    rt(|r| {
                        r.preset_name = crate::i18n::fmt(tl!("My preset {n}"), &[("n", &(all_presets.iter().filter(|p| !p.built_in).count() + 1).to_string())])
                    });
                }
                egui::Popup::from_toggle_button_response(&save).close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside).show(|ui| {
                    ui.set_min_width(220.0);
                    ui.label(tl!("Preset name"));
                    let mut name = rt(|r| r.preset_name.clone());
                    ui.text_edit_singleline(&mut name);
                    rt(|r| r.preset_name = name.clone());
                    if widgets::primary_button(ui, tl!("Save Preset"), 0.0).clicked() && !name.trim().is_empty() {
                        save_preset(s, name.trim());
                        ui.close();
                    }
                });
                let opts: Vec<(String, &str)> = all_presets.iter().map(|p| (p.name.clone(), p.name.as_str())).collect();
                let mut name = s.preset.clone();
                if widgets::dropdown(ui, "gen-preset", &mut name, &opts, ui.available_width().min(220.0))
                    && let Some(p) = all_presets.iter().find(|p| p.name == name)
                {
                    apply_preset(s, p);
                }
            });
        });
        if s.mode == Mode::Refine {
            inherit_refine_defaults("generate-main", s);
        }
        if s.refine
            && s.mode == Mode::Create
            && let Some(model) = ModelId::from_key(&s.refine_model)
        {
            let mut refinement = GenerateState {
                model: s.refine_model.clone(),
                variant: s.refine_variant.clone(),
                steps: s.refine_steps.unwrap_or(model.info().steps.default as u32),
                guidance: s.refine_guidance.unwrap_or(model.info().guidance.default),
                denoise: s.refine_strength,
                mode: Mode::Refine,
                refine_defaults: [s.refine_steps.is_none(), s.refine_guidance.is_none(), s.refine_strength_default],
                ..Default::default()
            };
            inherit_refine_defaults("generate-second-pass", &mut refinement);
            s.refine_strength = refinement.denoise;
            if s.refine_steps.is_some() {
                s.refine_steps = Some(refinement.steps);
            }
            if s.refine_guidance.is_some() {
                s.refine_guidance = Some(refinement.guidance);
            }
        }
        // ---- Model.
        // Upscale offers the official upscalers; a diffusion model chosen earlier (saved
        // settings) keeps the full list.
        let picked = model_picker(ui, "gen-model", (&s.model, &s.variant), s.mode, &st, true);
        if s.mode == Mode::Upscale {
            let any = upscale_any();
            ui.horizontal(|ui| {
                let (text, tip) = if any {
                    (tl!("Official upscalers only"), tl!("List only the official upscalers (SeedVR2)"))
                } else {
                    (tl!("Choose another model…"), tl!("Upscale with any image model instead: enlarge, then add detail with img2img"))
                };
                if ui.add(egui::Button::new(RichText::new(text).size(11.0)).small()).on_hover_text(tip).clicked() {
                    rt(|r| r.upscale_any = !any);
                    // Back to the official list: the model gives way to an upscaler.
                    if any && let Some(u) = ModelId::all().into_iter().find(|m| is_upscaler(m.info())) {
                        set_model(s, u);
                    }
                }
            });
        }
        if let Some(k) = picked {
            match k.as_str() {
                "browse:" => crate::model_browser::open(),
                "browse-loras:" => browse_loras(&app.ui.ai.generate.model),
                "import:" => import_workflow(app),
                k if k.starts_with("custom:") || k.starts_with("cloud:") => app.ui.ai.generate.model = k.to_owned(),
                k => {
                    if let Some(m) = ModelId::from_key(k) {
                        set_model(&mut app.ui.ai.generate, m);
                    }
                }
            }
        }
        let s = &mut app.ui.ai.generate;
        let custom = custom_of(s);
        let cloud = cloud_of(s);
        let m = model_of(s);
        let info = m.info();
        // An official upscaler (SeedVR2) restores and enlarges by itself: no prompt or strength.
        let upscaler = s.mode == Mode::Upscale && custom.is_none() && cloud.is_none() && is_upscaler(info);
        if let Some(c) = cloud {
            ui.horizontal_wrapped(|ui| {
                let (r, _) = ui.allocate_exact_size(vec2(14.0, 14.0), Sense::hover());
                crate::icons::paint(ui, r, "cloud", 12.0, t.text_faint);
                ui.label(
                    RichText::new(crate::i18n::fmt(
                        tl!("Runs on {provider}: your prompt and images are sent there, billed to your key."),
                        &[("provider", c.provider.label())],
                    ))
                    .color(t.text_faint)
                    .size(11.0),
                );
            });
        } else if custom.is_none() {
            variant_picker(ui, m, &mut s.variant);
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                ui.label(RichText::new(&info.best_for).color(t.text_faint).size(11.0));
            });
            model_variety_note(ui, m);
        }
        // ---- Prompt.
        let wants_prompt = !upscaler && custom.as_ref().is_none_or(|w| w.has(&li_ai::custom::FieldKind::Prompt));
        let mut enter = false;
        if wants_prompt {
            let hint = match s.mode {
                Mode::Create => tl!("Describe an image to create…"),
                Mode::Edit if info.edit => tl!("Describe the change, e.g. “make it golden hour”…"),
                Mode::Edit => tl!("Describe the image you want it to become…"),
                Mode::Fill => tl!("What should appear in the selection…"),
                Mode::Refine => tl!("Describe the image (optional)…"),
                Mode::Upscale => tl!("Describe details to add (optional)…"),
            };
            let resp = ui.add(egui::TextEdit::multiline(&mut s.prompt).hint_text(hint).desired_rows(3).desired_width(f32::INFINITY));
            if rt(|r| std::mem::take(&mut r.focus_prompt)) {
                resp.request_focus();
            }
            enter = resp.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter) && !i.modifiers.shift);
            if enter && s.prompt.ends_with('\n') {
                s.prompt.pop();
            }
        }
        let wants_negative = match (&custom, cloud) {
            (Some(w), _) => w.has(&li_ai::custom::FieldKind::Negative),
            (None, Some(c)) => c.negative,
            (None, None) => info.negative_prompt,
        };
        if wants_negative {
            ui.add(egui::TextEdit::singleline(&mut s.negative).hint_text(tl!("Avoid… (negative prompt)")).desired_width(f32::INFINITY));
        }
        // ---- Size (Create): shared aspect presets and linked model-grid dimensions.
        let custom_size = custom.as_ref().is_some_and(|w| w.has(&li_ai::custom::FieldKind::Width));
        if (s.mode == Mode::Create && custom.is_none()) || custom_size {
            size_controls(ui, s);
        }
        if s.mode == Mode::Create && info.transparent && custom.is_none() && cloud.is_none() {
            widgets::checkbox(ui, &mut s.transparent, tl!("Transparent background"));
        }
        if let Some(c) = cloud
            && s.mode == Mode::Create
        {
            ui.label(
                RichText::new(crate::i18n::fmt(tl!("Sent as {size}"), &[("size", &li_ai::cloud::describe_size(c, s.width, s.height))]))
                    .color(t.text_faint)
                    .size(11.0),
            );
        }
        if s.mode == Mode::Refine {
            ui.horizontal(|ui| {
                label(ui, tl!("Input"));
                let choices = [(RefineInput::Composite, tl!("Open image (composite)")), (RefineInput::SelectedLayer, tl!("Selected layer"))];
                widgets::dropdown(ui, "refine-input", &mut s.refine_input, &choices, ui.available_width());
            });
            ui.label(tl!("Refinement uses the current pixels of the open image."));
        }
        // ---- Strength.
        let strength_shown = match (&custom, cloud) {
            (Some(w), _) => w.has(&li_ai::custom::FieldKind::Denoise),
            (None, Some(c)) => c.key == "stability:sd3.5-large" && s.mode == Mode::Edit,
            (None, None) => {
                (matches!(s.mode, Mode::Refine | Mode::Upscale) && !upscaler)
                    || (s.mode == Mode::Edit && !info.edit)
                    || (s.mode == Mode::Fill && info.resolved.pipeline.inpaint != li_ai::family::InpaintMethod::Instruction)
            }
        };
        if strength_shown {
            // ComfyUI's denoise, in ComfyUI's own range: 1 regenerates, lower keeps more of the image.
            let (lo, hi) = crate::ai_ui::status().limits.map_or((f32::MIN, f32::MAX), |l| l.denoise);
            denoise_row(ui, tl!("Denoise"), &mut s.denoise, lo, hi);
        }
        if s.mode == Mode::Upscale {
            let scales = [("1.5×", 1.5f32), ("2×", 2.0), ("3×", 3.0), ("4×", 4.0)];
            let items: Vec<&str> = scales.iter().map(|(l, _)| *l).collect();
            let sel = scales.iter().position(|(_, v)| (*v - s.upscale).abs() < 0.01).unwrap_or(1);
            ui.horizontal(|ui| {
                label(ui, tl!("Enlarge"));
                if let Some(i) = chips(ui, &items, sel) {
                    s.upscale = scales[i].1;
                }
            });
        }
        // ---- References.
        let ref_slots = match (&custom, cloud) {
            (Some(w), _) => w.images(),
            (None, Some(c)) => c.max_refs as usize,
            (None, None) => {
                let adapter = matches!(info.resolved.pipeline.reference, li_ai::family::RefMethod::IpAdapter | li_ai::family::RefMethod::Redux);
                match s.mode {
                    Mode::Create if adapter => 4,
                    Mode::Create => info.max_references,
                    Mode::Edit if info.edit => info.max_references.saturating_sub(1),
                    Mode::Fill if info.edit => info.max_references.saturating_sub(1),
                    _ => 0,
                }
            }
        };
        if ref_slots > 0 {
            references_ui(app, ui, ref_slots, info.init_image && custom.is_none() && cloud.is_none());
        }
        let s = &mut app.ui.ai.generate;
        // ---- LoRAs.
        if custom.is_none() && cloud.is_none() && (info.lora || s.mode == Mode::Refine) {
            if s.mode == Mode::Refine {
                refine_loras_ui(ui, &mut s.loras, info.id, &s.variant, &t);
            } else {
                loras_ui(ui, &mut s.loras, &info.resolved, &t);
            }
        }
        // ---- Draft → Refine.
        if s.mode == Mode::Create && custom.is_none() && cloud.is_none() {
            ui.horizontal(|ui| {
                widgets::checkbox(ui, &mut s.refine, tl!("Refine with")).on_hover_text(tl!(
                    "Draft → Refine: generate with this model, then resample the draft with another (e.g. a fast draft refined by a detailed model)"
                ));
            });
            if s.refine {
                if let Some(k) = model_picker(ui, "gen-refine-model", (&s.refine_model, &s.refine_variant), Mode::Refine, &st, false) {
                    if k == "browse:" {
                        crate::model_browser::open();
                    } else if k == "browse-loras:" {
                        browse_loras(&s.refine_model);
                    } else if let Some(m) = ModelId::from_key(&k) {
                        s.refine_model = k;
                        s.refine_variant = catalog::default_variant(m).into();
                        s.refine_strength = catalog::refine_default_denoise(m);
                        s.refine_strength_default = true;
                        s.refine_steps = None;
                        s.refine_guidance = None;
                        let fits: Vec<_> = li_ai::inventory::loras_for(&m.info().resolved).into_iter().map(|(l, _)| l.name).collect();
                        s.refine_loras.retain(|l| fits.contains(&l.name));
                    }
                }
                if let Some(rm) = ModelId::from_key(&s.refine_model).filter(|m| m.try_info().is_some()) {
                    ui.push_id("refine-pass", |ui| {
                        variant_picker(ui, rm, &mut s.refine_variant);
                        refine_loras_ui(ui, &mut s.refine_loras, rm, &s.refine_variant, &t);
                    });
                }
                // The refine pass's own sampler settings, bounded only by what ComfyUI reports.
                let limits = crate::ai_ui::status().limits;
                let (dlo, dhi) = limits.map_or((f32::MIN, f32::MAX), |l| l.denoise);
                if denoise_row(ui, tl!("Refine denoise"), &mut s.refine_strength, dlo, dhi) {
                    s.refine_strength_default = false;
                }
                if let Some(ri) = ModelId::from_key(&s.refine_model).and_then(|m| m.try_info()) {
                    let (slo, shi) = limits.map_or((1.0, f32::MAX), |l| l.steps);
                    let mut steps = s.refine_steps.unwrap_or(ri.steps.default as u32);
                    let hint = range_hint(ri.steps.min, ri.steps.max, ri.steps.default, 0);
                    if number_row(ui, tl!("Refine steps"), &mut steps, slo.max(1.0) as u32, shi.max(1.0) as u32, Some(hint)) {
                        s.refine_steps = Some(steps);
                    }
                    let (clo, chi) = limits.map_or((f32::MIN, f32::MAX), |l| l.cfg);
                    let mut cfg = s.refine_guidance.unwrap_or(ri.guidance.default);
                    if guidance_row_labeled(
                        ui,
                        tl!("Refine guidance"),
                        &mut cfg,
                        clo,
                        chi,
                        Some(range_hint(ri.guidance.min, ri.guidance.max, ri.guidance.default, 1)),
                    ) {
                        s.refine_guidance = Some(cfg);
                    }
                }
                let scales = [("1×", 1.0f32), ("1.5×", 1.5), ("2×", 2.0)];
                let items: Vec<&str> = scales.iter().map(|(l, _)| *l).collect();
                let sel = scales.iter().position(|(_, v)| (*v - s.refine_scale).abs() < 0.01).unwrap_or(0);
                ui.horizontal(|ui| {
                    label(ui, tl!("Enlarge first"));
                    if let Some(i) = chips(ui, &items, sel) {
                        s.refine_scale = scales[i].1;
                    }
                });
            }
        }
        // ---- Advanced.
        let resp = egui::CollapsingHeader::new(RichText::new(tl!("Advanced")).color(t.text_dim)).default_open(s.advanced).show(ui, |ui| {
            let (steps_range, cfg_range) = (info.steps, info.guidance);
            // The only limits are the ones the connected ComfyUI reports for its sampler; the model's numbers are
            // shown as recommendations, never enforced (every model, fixed-step ones included, can change steps).
            let limits = crate::ai_ui::status().limits;
            fit_to_backend(s, limits);
            if custom.is_none() && cloud.is_none() {
                sampling_ui(ui, s, limits, false);
            } else {
                if cloud.is_none() && custom.as_ref().is_none_or(|w| w.has(&li_ai::custom::FieldKind::Steps)) {
                    let (lo, hi) = limits.map_or((1.0, f32::MAX), |l| l.steps);
                    let recommended = (custom.is_none()).then(|| range_hint(steps_range.min, steps_range.max, steps_range.default, 0));
                    number_row(ui, tl!("Steps"), &mut s.steps, lo as u32, hi as u32, recommended);
                }
                if cloud.is_none() && custom.as_ref().is_none_or(|w| w.has(&li_ai::custom::FieldKind::Cfg)) {
                    let (lo, hi) = limits.map_or((f32::MIN, f32::MAX), |l| l.cfg);
                    let recommended = (custom.is_none()).then(|| range_hint(cfg_range.min, cfg_range.max, cfg_range.default, 1));
                    guidance_row(ui, &mut s.guidance, lo, hi, recommended);
                }
            }
            if info.init_image && custom.is_none() && cloud.is_none() && s.mode == Mode::Create {
                let mut pct = s.denoise * 100.0;
                if widgets::slider_row(ui, tl!("Variation strength"), &mut pct, 5.0..=100.0, "%", None).changed() {
                    s.denoise = pct / 100.0;
                }
            }
            if custom.is_none() && cloud.is_none() && info.resolved.pipeline.sampler == li_ai::family::SamplerStyle::Ksampler {
                ui.horizontal(|ui| {
                    label(ui, crate::i18n::tr_ctx(crate::i18n::current(), "ai", "Sampler"));
                    let samplers = ["", "euler", "euler_ancestral", "dpmpp_2m", "dpmpp_2m_sde", "dpmpp_sde", "uni_pc", "res_multistep", "lcm"];
                    let opts: Vec<(String, &str)> = samplers.iter().map(|x| (x.to_string(), if x.is_empty() { tl!("Model default") } else { *x })).collect();
                    widgets::dropdown(ui, "gen-sampler", &mut s.sampler, &opts, 130.0);
                    let schedulers = ["", "simple", "normal", "karras", "sgm_uniform", "beta", "exponential"];
                    let opts: Vec<(String, &str)> = schedulers.iter().map(|x| (x.to_string(), if x.is_empty() { tl!("Default") } else { *x })).collect();
                    widgets::dropdown(ui, "gen-scheduler", &mut s.scheduler, &opts, 100.0);
                });
            }
            ui.horizontal(|ui| {
                label(ui, tl!("Seed"));
                let mut locked = s.seed.is_some();
                if widgets::checkbox(ui, &mut locked, tl!("Keep")).changed() {
                    s.seed = if locked { Some(li_ai::ops::new_seed()) } else { None };
                }
                if let Some(seed) = s.seed.as_mut() {
                    let mut text = seed.to_string();
                    if ui.add(egui::TextEdit::singleline(&mut text).desired_width(120.0).font(crate::theme::mono(12.0))).changed()
                        && let Ok(v) = text.trim().parse::<u64>()
                    {
                        *seed = v;
                    }
                } else {
                    ui.label(RichText::new(tl!("new each time")).color(t.text_faint));
                }
            });
        });
        s.advanced = resp.fully_open();
        // ---- Readiness and the button.
        let variant = s.variant.clone();
        let ready = match (&custom, cloud) {
            (_, Some(c)) => cloud_ready(c),
            (Some(_), _) if st.connected => Ok(()),
            (Some(_), _) => Err(tl!("The AI engine (ComfyUI) is not running.").to_owned()),
            (None, None) if s.mode == Mode::Refine => refine_readiness(&st, m, &variant),
            (None, None) => readiness(&st, m, &variant),
        };
        let refine_ready = if s.refine && s.mode == Mode::Create && custom.is_none() && cloud.is_none() {
            match ModelId::from_key(&s.refine_model) {
                Some(rm) => refine_readiness(&st, rm, &s.refine_variant).map_err(|e| crate::i18n::fmt(tl!("Refine model: {why}"), &[("why", &e)])),
                None => Err(tl!("Choose a model to refine with.").into()),
            }
        } else {
            Ok(())
        };
        let needs_prompt = wants_prompt && matches!(s.mode, Mode::Create | Mode::Edit | Mode::Fill);
        let blocker = match s.mode {
            _ if needs_prompt && s.prompt.trim().is_empty() => Some(tl!("Describe what you want first.")),
            Mode::Edit | Mode::Refine | Mode::Upscale if !has_doc => Some(tl!("Open an image first.")),
            Mode::Refine if s.refine_input == RefineInput::SelectedLayer && !has_active_layer => Some(tl!("Select a layer first.")),
            Mode::Fill if !has_sel => Some(tl!("Make a selection to fill.")),
            _ => None,
        };
        ui.add_space(4.0);
        for why in [ready.as_ref().err(), refine_ready.as_ref().err()].into_iter().flatten() {
            readiness_warning(app, ui, why, &st, cloud.is_some());
        }
        let s = &app.ui.ai.generate;
        let label_text = match s.mode {
            Mode::Create if s.count > 1 => crate::i18n::fmt(tl!("Generate {n}"), &[("n", &s.count.to_string())]),
            Mode::Create => tl!("Generate").into(),
            Mode::Edit => tl!("Apply Edit").into(),
            Mode::Fill => tl!("Fill Selection").into(),
            Mode::Refine => tl!("Refine").into(),
            Mode::Upscale => crate::i18n::fmt(tl!("Upscale {scale}×"), &[("scale", &widgets::fmt_num(s.upscale as f64))]),
        };
        let can = blocker.is_none() && ready.is_ok() && refine_ready.is_ok();
        let create = s.mode == Mode::Create;
        let mut count = s.count as f32;
        let clicked = ui
            .horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                let w = ui.available_width() - if create { 46.0 } else { 0.0 };
                let r = ui
                    .add_enabled_ui(can, |ui| widgets::primary_button(ui, &label_text, w))
                    .inner
                    .on_disabled_hover_text(blocker.unwrap_or(tl!("The model isn't ready.")));
                if create {
                    widgets::value_field(ui, &mut count, 1.0..=4.0, "", 40.0).on_hover_text(tl!("Images per run (1–4)"));
                }
                r
            })
            .inner
            .clicked();
        app.ui.ai.generate.count = (count.round() as u32).clamp(1, 4);
        if (clicked || enter) && can {
            start(app, &ctx);
        }
        ui.add_space(8.0);
        results_ui(app, ui);
    });
}

fn refine_loras_ui(ui: &mut egui::Ui, picks: &mut Vec<LoraPick>, model: ModelId, variant: &str, t: &Tokens) {
    let profile = li_ai::refine::lora_profile_for(model, variant);
    refine_recipe_loras_ui(ui, picks, profile.as_ref(), t);
}

fn refine_recipe_loras_ui(ui: &mut egui::Ui, picks: &mut Vec<LoraPick>, profile: Option<&li_ai::family::Family>, t: &Tokens) {
    match profile {
        Some(profile) => loras_ui(ui, picks, profile, t),
        None => {
            ui.label(tl!("This refinement recipe does not support styles (LoRAs)."));
            if !picks.is_empty() && widgets::secondary_button(ui, tl!("Clear styles"), 0.0).clicked() {
                picks.clear();
            }
        }
    }
}

fn loras_ui(ui: &mut egui::Ui, picks: &mut Vec<LoraPick>, fam: &li_ai::family::Family, t: &Tokens) {
    let available = li_ai::inventory::loras_for(fam);
    let max = fam.lora.max.max(1) as usize;
    ui.horizontal(|ui| {
        label(ui, &crate::i18n::fmt(tl!("Styles (LoRA) · {n}/{max}"), &[("n", &picks.len().to_string()), ("max", &max.to_string())]));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let free: Vec<&(li_ai::inventory::InstalledLora, bool)> = available.iter().filter(|(l, _)| !picks.iter().any(|p| p.name == l.name)).collect();
            let can = picks.len() < max && !free.is_empty();
            let tip = if available.is_empty() { tl!("No LoRAs for this model family are installed (Find LoRAs…)") } else { tl!("Add a LoRA") };
            // "+ Add" lists the LoRAs to choose from (never picks one by itself): the ones that fit this model's
            // family first, then those whose family couldn't be identified, under a warning.
            let mut picked = None;
            ui.add_enabled_ui(can, |ui| {
                let r = widgets::secondary_button(ui, tl!("+ Add"), 0.0).on_hover_text(tip).on_disabled_hover_text(tip);
                egui::Popup::menu(&r).show(|ui| {
                    ui.set_min_width(220.0);
                    let (sure, unsure): (Vec<&&(li_ai::inventory::InstalledLora, bool)>, Vec<_>) = free.iter().partition(|(_, sure)| *sure);
                    for (l, _) in sure.iter().copied().copied() {
                        if widgets::secondary_button(ui, &li_ai::inventory::label_for(&l.name), 0.0).clicked() {
                            picked = Some(l.name.clone());
                            ui.close();
                        }
                    }
                    if !unsure.is_empty() {
                        if !sure.is_empty() {
                            ui.separator();
                        }
                        ui.label(RichText::new(tl!("Family unknown: may not work with this model")).size(11.0).color(t.warning));
                        for (l, _) in unsure.iter().copied().copied() {
                            if widgets::secondary_button(ui, &li_ai::inventory::label_for(&l.name), 0.0).clicked() {
                                picked = Some(l.name.clone());
                                ui.close();
                            }
                        }
                    }
                });
            });
            if let Some(name) = picked {
                picks.push(LoraPick { name, strength: 0.8 });
            }
            let browse = crate::icons::button(ui, "search", 22.0, false, tl!("Browse LoRAs for this model family"));
            browse.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, tl!("Browse LoRAs…")));
            if browse.clicked() {
                crate::model_browser::open_loras(&fam.id);
            }
        });
    });
    let mut remove = None;
    for (i, pick) in picks.iter_mut().enumerate() {
        ui.horizontal(|ui| {
            let opts: Vec<(String, String)> = available
                .iter()
                .map(|(l, sure)| {
                    let name = li_ai::inventory::label_for(&l.name);
                    (l.name.clone(), if *sure { name } else { crate::i18n::fmt(tl!("{name} (family unknown)"), &[("name", &name)]) })
                })
                .collect();
            let refs: Vec<(String, &str)> = opts.iter().map(|(k, l)| (k.clone(), l.as_str())).collect();
            widgets::dropdown(ui, &format!("gen-lora-{i}"), &mut pick.name, &refs, (ui.available_width() - 110.0).max(80.0));
            let strength = widgets::value_field(ui, &mut pick.strength, -2.0..=2.0, "", 52.0).on_hover_text(tl!("Strength"));
            strength.widget_info(|| egui::WidgetInfo {
                label: Some(tl!("Strength").into()),
                ..egui::WidgetInfo::drag_value(ui.is_enabled(), pick.strength as f64)
            });
            if crate::icons::button(ui, "x", 20.0, false, tl!("Remove")).clicked() {
                remove = Some(i);
            }
        });
    }
    if let Some(i) = remove {
        picks.remove(i);
    }
    if picks.is_empty() && available.is_empty() {
        ui.horizontal(|ui| {
            ui.label(RichText::new(tl!("No compatible LoRAs installed.")).color(t.text_faint).size(11.0));
            if ui.link(RichText::new(tl!("Find LoRAs…")).size(11.0)).clicked() {
                crate::model_browser::open_loras(&fam.id);
            }
        });
    }
}

fn apply_preset(s: &mut GenerateState, p: &li_ai::presets::GenPreset) {
    s.refine_strength_default = p.refine_strength.is_none();
    s.preset = p.name.clone();
    let model = p.model.as_deref().and_then(ModelId::from_key).or_else(|| {
        let fam = p.family.as_deref()?;
        let st = crate::ai_ui::status();
        let all = ModelId::all();
        all.iter()
            .copied()
            .find(|m| m.info().family == fam && catalog::presets_for(*m).any(|x| st.ready(*m, &x.variant).is_ok()))
            .or_else(|| all.into_iter().find(|m| m.info().family == fam))
    });
    if let Some(m) = model {
        set_model(s, m);
    }
    if !p.negative.is_empty() {
        s.negative = p.negative.clone();
    }
    if let Some(v) = p.steps {
        s.steps = v;
        s.refine_defaults[0] = false;
    }
    if let Some(v) = p.cfg {
        s.guidance = v;
        s.refine_defaults[1] = false;
    }
    s.variety = p.variety.filter(|v| v.is_finite()).unwrap_or(0.0).clamp(0.0, 100.0);
    if let Some((a, b)) = &p.sampler {
        s.sampler = a.clone();
        s.scheduler = b.clone();
    }
    if !p.loras.is_empty() {
        s.loras = p.loras.iter().map(|l| LoraPick { name: l.name.clone(), strength: l.strength }).collect();
    }
    s.refine_loras = p.refine_loras.iter().map(|l| LoraPick { name: l.name.clone(), strength: l.strength }).collect();
    if let Some(rm) = &p.refine_model {
        s.refine = true;
        s.refine_model = rm.clone();
        s.refine_variant = ModelId::from_key(rm).map(|m| catalog::default_variant(m).to_owned()).unwrap_or_default();
        s.refine_strength = p.refine_strength.unwrap_or_else(|| catalog::refine_default_denoise(ModelId::intern(rm)));
        s.refine_steps = p.refine_steps;
        s.refine_guidance = p.refine_guidance;
    }
}

fn save_preset(s: &mut GenerateState, name: &str) {
    let current = presets().into_iter().find(|p| p.name == s.preset);
    let preset = li_ai::presets::GenPreset {
        name: name.to_owned(),
        model: (!s.model.starts_with("custom:")).then(|| s.model.clone()),
        family: None,
        template: current.map(|p| p.template).unwrap_or_default(),
        negative: s.negative.clone(),
        steps: Some(s.steps),
        cfg: Some(s.guidance),
        variety: Some(s.variety),
        sampler: (!s.sampler.is_empty()).then(|| (s.sampler.clone(), if s.scheduler.is_empty() { "simple".into() } else { s.scheduler.clone() })),
        loras: s.loras.iter().map(|l| li_ai::presets::PresetLora { name: l.name.clone(), strength: l.strength }).collect(),
        refine_model: s.refine.then(|| s.refine_model.clone()),
        refine_strength: s.refine.then_some(s.refine_strength),
        refine_steps: s.refine.then_some(s.refine_steps).flatten(),
        refine_guidance: s.refine.then_some(s.refine_guidance).flatten(),
        refine_loras: if s.refine {
            s.refine_loras.iter().map(|l| li_ai::presets::PresetLora { name: l.name.clone(), strength: l.strength }).collect()
        } else {
            Vec::new()
        },
        built_in: false,
    };
    let mut all = presets();
    all.retain(|p| p.name != preset.name || p.built_in);
    all.push(preset);
    let _ = li_ai::presets::save_user(&preset_path(), &all);
    rt(|r| r.presets = None);
    s.preset = name.to_owned();
}

/// Generate › Import Workflow…: a ComfyUI workflow (API or editor format) with `li:` titles.
pub fn import_workflow(app: &mut PhotocraftApp) {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let Some(path) = rfd::FileDialog::new().set_title(tl!("Import ComfyUI Workflow")).add_filter(tl!("ComfyUI workflow"), &["json"]).pick_file() else {
            return;
        };
        match import_workflow_path(&path) {
            Ok(name) => {
                app.ui.ai.generate.model = format!("custom:{name}");
                app.ui.status = crate::i18n::fmt(tl!("Imported the workflow “{name}”."), &[("name", &name)]);
                app.ui.status_error = false;
            }
            Err(e) => {
                app.ui.status = e;
                app.ui.status_error = true;
            }
        }
    }
    #[cfg(target_arch = "wasm32")]
    let _ = app;
}

/// Imports a workflow file; returns its name.
pub fn import_workflow_path(path: &std::path::Path) -> Result<String, String> {
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    let json: Value = serde_json::from_slice(&bytes).map_err(|_| tl!("That file is not a ComfyUI workflow (JSON).").to_owned())?;
    let name = path.file_stem().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "Workflow".into());
    let info = li_ai::service().client.object_info().ok();
    let w = li_ai::custom::import(&name, &json, info.as_ref()).map_err(|e| format!("{e:#}"))?;
    if w.fields.is_empty() {
        return Err(tl!("The workflow has no nodes titled li:prompt, li:image, li:mask, li:seed or li:output, so Local Image can't fill it in. Rename the nodes in ComfyUI and export again.").into());
    }
    li_ai::custom::save(&w, &li_ai::custom::dir()).map_err(|e| format!("{e:#}"))?;
    rt(|r| r.customs = None);
    Ok(w.name)
}

fn references_ui(app: &mut PhotocraftApp, ui: &mut egui::Ui, max: usize, init_image: bool) {
    let t = Tokens::get(ui.ctx());
    let n = rt(|r| r.references.len());
    ui.horizontal(|ui| {
        let heading = if init_image { tl!("Starting image · {n}/{max}") } else { tl!("References · {n}/{max}") };
        label(ui, &crate::i18n::fmt(heading, &[("n", &n.to_string()), ("max", &max.to_string())]));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let full = n >= max;
            ui.add_enabled_ui(!full, |ui| {
                #[cfg(not(target_arch = "wasm32"))]
                if crate::icons::button(ui, "folder-open", 22.0, false, tl!("Add an image file")).clicked()
                    && let Some(path) = rfd::FileDialog::new().add_filter(tl!("Images"), &["png", "jpg", "jpeg", "webp", "tif", "tiff"]).pick_file()
                {
                    match photocraft_engine::ai_cmds::file_rgba_srgb(&path) {
                        Ok(img) => {
                            let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                            add_reference(ui.ctx(), name, img);
                        }
                        Err(e) => {
                            app.ui.status = e.to_string();
                            app.ui.status_error = true;
                        }
                    }
                }
                if app.session.active().is_some() && crate::icons::button(ui, "image", 22.0, false, tl!("Add the current image")).clicked() {
                    let d = app.session.active().map(|d| (d.doc.name.clone(), photocraft_engine::ai_cmds::flatten_rgba(&d.doc)));
                    if let Some((name, img)) = d {
                        match img {
                            Ok(img) => add_reference(ui.ctx(), name, img),
                            Err(e) => {
                                app.ui.status = e.to_string();
                                app.ui.status_error = true;
                            }
                        }
                    }
                }
            });
        });
    });
    let mut remove = None;
    if n == 0 {
        return;
    }
    ui.horizontal_wrapped(|ui| {
        rt(|r| {
            for (i, rf) in r.references.iter_mut().enumerate() {
                let tex = rf.texture.get_or_insert_with(|| image_texture(ui.ctx(), &format!("{i}-{}", rf.name), &rf.image)).clone();
                let (rect, resp) = ui.allocate_exact_size(vec2(52.0, 52.0), Sense::click());
                widgets::checker(ui.painter(), rect, 6.0);
                let sz = tex.size_vec2();
                let k = (52.0 / sz.x).min(52.0 / sz.y);
                ui.painter().image(
                    tex.id(),
                    egui::Rect::from_center_size(rect.center(), sz * k),
                    egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                    Color32::WHITE,
                );
                ui.painter().rect_stroke(rect, 3.0, Stroke::new(1.0, t.field_border), StrokeKind::Inside);
                if resp.hovered() {
                    crate::icons::paint(ui, egui::Rect::from_min_size(rect.right_top() - vec2(16.0, 0.0), vec2(16.0, 16.0)), "x", 12.0, Color32::WHITE);
                }
                if resp.on_hover_text(crate::i18n::fmt(tl!("{name} — click to remove"), &[("name", &rf.name)])).clicked() {
                    remove = Some(i);
                }
            }
        });
    });
    if let Some(i) = remove {
        rt(|r| r.references.remove(i));
    }
}

fn add_reference(_ctx: &egui::Context, name: String, img: RgbaImage) {
    rt(|r| r.references.push(Reference { name, image: Arc::new(img), texture: None }));
}

/// Restore a placed Generate/Library result's settings. Regenerate starts a fresh job;
/// Variations brings its settings forward so the user can choose a batch or adjust the prompt.
pub(crate) fn regenerate_layer(app: &mut PhotocraftApp, ctx: &egui::Context, layer: u64, g: &Value, variations: bool) -> Result<(), String> {
    let model = g["model"].as_str().ok_or_else(|| tl!("This layer has no generation model to repeat").to_owned())?;
    let mut s = GenerateState::default();
    if let Some(m) = ModelId::from_key(model) {
        set_model(&mut s, m);
    } else if model.starts_with("custom:") || model.starts_with("cloud:") {
        s.model = model.to_owned();
    } else {
        return Err(tl!("This layer's generation model is unavailable").into());
    }
    s.prompt = g["prompt"].as_str().unwrap_or("").to_owned();
    s.negative = g["negative_prompt"].as_str().unwrap_or("").to_owned();
    if let Some(v) = g["variant"].as_str() {
        s.variant = v.to_owned();
    }
    let uint = |key: &str| g[key].as_u64().and_then(|n| u32::try_from(n).ok());
    s.width = uint("width").unwrap_or(s.width);
    s.height = uint("height").unwrap_or(s.height);
    s.aspect = "custom".into();
    s.steps = uint("steps").unwrap_or(s.steps);
    s.guidance = g["guidance"].as_f64().map_or(s.guidance, |v| v as f32);
    s.variety = g["variety"].as_f64().filter(|v| v.is_finite()).map_or(0.0, |v| v.clamp(0.0, 100.0) as f32);
    s.denoise = g["denoise"].as_f64().map_or(s.denoise, |v| v as f32);
    s.refine_defaults = [false; 3];
    s.sampler = g["sampler"].as_str().unwrap_or("").into();
    s.scheduler = g["scheduler"].as_str().unwrap_or("").into();
    s.transparent = g["transparent"].as_bool().unwrap_or(false);
    s.loras = serde_json::from_value(g["loras"].clone()).unwrap_or_default();
    if let Some(refine) = g["refine"].as_object() {
        s.refine = true;
        s.refine_model = refine.get("model").and_then(Value::as_str).unwrap_or("").into();
        s.refine_variant = refine.get("variant").and_then(Value::as_str).unwrap_or("").into();
        s.refine_strength = refine.get("strength").and_then(Value::as_f64).unwrap_or(0.35) as f32;
        s.refine_strength_default = false;
        s.refine_steps = refine.get("steps").and_then(Value::as_u64).and_then(|v| u32::try_from(v).ok());
        s.refine_guidance = refine.get("guidance").and_then(Value::as_f64).map(|v| v as f32);
        s.refine_scale = refine.get("scale").and_then(Value::as_f64).unwrap_or(1.0) as f32;
        s.refine_loras = serde_json::from_value(g["refine"]["loras"].clone()).unwrap_or_default();
    }
    s.mode = match g["mode"].as_str() {
        Some("fill") => Mode::Fill,
        Some("edit") => Mode::Edit,
        Some("refine") => Mode::Refine,
        Some("upscale") => Mode::Upscale,
        _ => Mode::Create,
    };
    if s.mode == Mode::Fill {
        app.run("select.loadSelection", json!({"channel": "transparency", "layer": layer}))?;
    }
    app.ui.ai.generate = s;
    crate::dock::reveal(app, crate::dock::Group::Generate);
    app.ui.dock_tabs.generate = 0;
    focus_prompt(ctx);
    if !variations {
        app.ui.status_error = false;
        start(app, ctx);
        if app.ui.status_error {
            return Err(app.ui.status.clone());
        }
    }
    Ok(())
}

/// Snapshot the chosen open-image input; references never replace it in Refine mode.
fn document_input(app: &PhotocraftApp, input: RefineInput) -> Result<RgbaImage, String> {
    let d = app.session.active().ok_or_else(|| tl!("Open an image first.").to_owned())?;
    let image = match input {
        RefineInput::Composite => photocraft_engine::ai_cmds::flatten_rgba(&d.doc),
        RefineInput::SelectedLayer => {
            let l = d.active_layer.and_then(|id| d.doc.layer(id)).ok_or_else(|| tl!("Select a layer first.").to_owned())?;
            photocraft_engine::ai_cmds::selected_layer_rgba(&d.doc, l, d.doc.bounds())
        }
    };
    image.map_err(|e| e.to_string())
}

type OpenImageInput = (photocraft_doc::DocId, RgbaImage, Option<image::GrayImage>);

/// Only operations that consume the open image should render or colour-convert it.
fn request_document(app: &PhotocraftApp, s: &GenerateState) -> Result<Option<OpenImageInput>, String> {
    if s.mode == Mode::Create {
        return Ok(None);
    }
    let Some(d) = app.session.active() else { return Ok(None) };
    let input = if s.mode == Mode::Refine { s.refine_input } else { RefineInput::Composite };
    Ok(Some((d.doc.id, document_input(app, input)?, photocraft_engine::ai_cmds::selection_gray(&d.doc))))
}

fn local_request(
    s: &GenerateState,
    prompt: String,
    doc: Option<OpenImageInput>,
    refs: Vec<RgbaImage>,
) -> Option<(GenerateRequest, Option<photocraft_doc::DocId>, bool)> {
    let m = model_of(s);
    let info = m.info();
    let mut req = GenerateRequest::new(m, prompt);
    req.variant = s.variant.clone();
    req.width = s.width;
    req.height = s.height;
    req.transparent = s.transparent && info.transparent;
    // Steps/guidance fitted to what the connected ComfyUI accepts (never built-in ranges).
    let mut fitted = s.clone();
    fit_to_backend(&mut fitted, crate::ai_ui::status().limits);
    req.steps = fitted.steps;
    req.guidance = fitted.guidance;
    req.variety = if m == ModelId::ZImageTurbo && s.mode == Mode::Create && s.variety.is_finite() { s.variety.clamp(0.0, 100.0) } else { 0.0 };
    req.negative = s.negative.clone();
    req.denoise = fitted.denoise;
    if s.mode == Mode::Refine {
        req.refine_defaults = s.refine_defaults;
    }
    req.scale = s.upscale;
    req.loras = s.loras.iter().map(|l| li_ai::workflows::LoraUse { name: l.name.clone(), strength: l.strength }).collect();
    if !s.sampler.is_empty() {
        req.sampler = Some((s.sampler.clone(), if s.scheduler.is_empty() { info.resolved.pipeline.scheduler.clone() } else { s.scheduler.clone() }));
    }
    // Refine/Upscale cannot accept the Create panel's reference slots. They were kept in
    // runtime while hidden in these modes, causing validation to reject open-image jobs.
    req.references = if matches!(s.mode, Mode::Refine | Mode::Upscale) { Vec::new() } else { refs };
    let (mut target_doc, mut open) = (None, false);
    match s.mode {
        Mode::Create => {
            if s.refine
                && let Some(rm) = ModelId::from_key(&s.refine_model)
            {
                let mut refine = GenerateState {
                    mode: Mode::Refine,
                    model: s.refine_model.clone(),
                    steps: s.refine_steps.unwrap_or(rm.info().steps.default as u32),
                    guidance: s.refine_guidance.unwrap_or(rm.info().guidance.default),
                    denoise: s.refine_strength,
                    ..Default::default()
                };
                fit_to_backend(&mut refine, crate::ai_ui::status().limits);
                req.refine = Some(li_ai::RefineStep {
                    model: rm,
                    variant: s.refine_variant.clone(),
                    strength: refine.denoise,
                    default_strength: s.refine_strength_default,
                    steps: s.refine_steps.map(|_| refine.steps),
                    guidance: s.refine_guidance.map(|_| refine.guidance),
                    loras: s.refine_loras.iter().map(|l| li_ai::workflows::LoraUse { name: l.name.clone(), strength: l.strength }).collect(),
                    scale: s.refine_scale,
                });
            }
        }
        mode => {
            let (id, img, sel) = doc?;
            req.mode = match mode {
                Mode::Edit => GenerateMode::Edit,
                Mode::Fill => GenerateMode::Inpaint,
                Mode::Refine => GenerateMode::Refine,
                _ => GenerateMode::UpscaleRefine,
            };
            (req.width, req.height) = (img.width(), img.height());
            req.source = Some(img);
            if mode == Mode::Fill {
                req.mask = sel;
            }
            if mode == Mode::Upscale {
                open = true;
            } else {
                target_doc = Some(id);
            }
        }
    }
    Some((req, target_doc, open))
}

/// Starts the generation jobs for the current settings.
fn start(app: &mut PhotocraftApp, ctx: &egui::Context) {
    let s = app.ui.ai.generate.clone();
    restore_upscale_choice(&s);
    if s.mode == Mode::Upscale && !upscale_any() {
        if app.session.active().is_none() {
            return;
        }
        let seed = s.seed.unwrap_or_else(li_ai::ops::new_seed);
        if let Err(e) = app.run("ai.enhance", json!({"scale": s.upscale, "seed": seed, "method": s.enhance_method, "backend": s.enhance_backend})) {
            app.ui.status = e;
            app.ui.status_error = true;
        }
        ctx.request_repaint();
        return;
    }
    if s.mode == Mode::Refine && s.refine_input == RefineInput::SelectedLayer && app.session.active().is_none_or(|d| d.active_layer.is_none()) {
        app.ui.status = tl!("Select a layer first.").into();
        return;
    }
    let preset = presets().into_iter().find(|p| p.name == s.preset);
    let prompt = preset.as_ref().map_or_else(|| s.prompt.trim().to_owned(), |p| p.apply(&s.prompt));
    let doc = match request_document(app, &s) {
        Ok(doc) => doc,
        Err(e) => {
            app.ui.status = e;
            app.ui.status_error = true;
            return;
        }
    };
    let refs: Vec<RgbaImage> = rt(|r| r.references.iter().map(|x| (*x.image).clone()).collect());
    // Custom workflows.
    if let Some(w) = custom_of(&s) {
        let mut images = Vec::new();
        if let Some((_, img, _)) = &doc
            && s.mode != Mode::Create
        {
            images.push(img.clone());
        }
        images.extend(refs);
        let mask =
            doc.as_ref().and_then(|(_, _, m)| m.clone()).or_else(|| app.session.active().and_then(|d| photocraft_engine::ai_cmds::selection_gray(&d.doc)));
        let seed = s.seed.unwrap_or_else(li_ai::ops::new_seed);
        let inputs = li_ai::custom::Inputs {
            prompt: prompt.clone(),
            negative: s.negative.clone(),
            seed,
            steps: Some(s.steps),
            cfg: Some(s.guidance),
            denoise: Some(s.denoise),
            width: Some(s.width),
            height: Some(s.height),
        };
        let name = format!("{}-{seed}", w.name.replace(' ', ""));
        let meta = json!({ "model": format!("custom:{}", w.name), "prompt": prompt, "negative_prompt": s.negative, "seed": seed, "mode": "custom" });
        let label = format!("Running {}", w.name);
        let shown = crate::i18n::fmt(tl!("Running {name}"), &[("name", &w.name)]);
        launch(app, &label, &shown, name, meta, None, false, move |ctl| li_ai::service().run_custom(&w, &inputs, &images, mask.as_ref(), ctl));
        ctx.request_repaint();
        return;
    }
    if let Some(c) = cloud_of(&s) {
        start_cloud(app, &s, c, prompt, doc, refs);
        ctx.request_repaint();
        return;
    }
    let m = model_of(&s);
    let info = m.info();
    // Official restoration models share ai.enhance.
    if runs_as_enhance(info, s.mode) {
        if doc.is_none() {
            return;
        }
        let seed = s.seed.unwrap_or_else(li_ai::ops::new_seed);
        if let Err(e) = app.run("ai.enhance", json!({ "scale": s.upscale, "seed": seed, "model": m.key(), "method": if info.id == ModelId::SeedVr2 { li_ai::enhance::Method::Generative } else if info.family == "bsrgan" { li_ai::enhance::Method::Rescue } else { li_ai::enhance::Method::Faithful }, "backend": s.enhance_backend })) {
            app.ui.status = e;
            app.ui.status_error = true;
        }
        ctx.request_repaint();
        return;
    }
    let Some((req, target_doc, open)) = local_request(&s, prompt, doc, refs) else { return };
    if let Err(e) = req.validate() {
        app.ui.status = e.to_string();
        app.ui.status_error = true;
        return;
    }
    let fill = s.mode == Mode::Fill;
    let count = if s.mode == Mode::Create { s.count.clamp(1, 4) } else { 1 };
    for i in 0..count {
        let mut r = req.clone();
        r.seed = s.seed.map(|v| v.wrapping_add(i as u64)).unwrap_or_else(li_ai::ops::new_seed);
        let label = format!("{} with {}", s.mode.label(), info.short);
        let shown = crate::i18n::fmt(tl!("{mode} with {model}"), &[("mode", s.mode.shown()), ("model", &info.short)]);
        let name = format!("{}-{}", info.short.replace(' ', ""), r.seed);
        let meta = json!({
            "model": m.key(), "family": info.family, "variant": r.variant, "prompt": r.prompt, "negative_prompt": r.negative, "width": r.width, "height": r.height,
            "seed": r.seed, "transparent": r.transparent, "steps": r.steps, "guidance": r.guidance, "denoise": r.denoise, "variety": r.variety,
            "loras": s.loras.iter().map(|l| json!({"name": l.name, "strength": l.strength})).collect::<Vec<_>>(),
            "refine": r.refine,
            "reference_count": r.references.len(), "mode": format!("{:?}", s.mode).to_lowercase(),
        });
        let mask = r.mask.clone();
        let ok = launch_resolved(app, &label, &shown, name, meta, target_doc.map(|d| d.0), open, move |ctl| {
            let generated = li_ai::service().generate_detailed(&r, ctl)?;
            let out = generated.image;
            // Fill lands as a layer holding only the regenerated area.
            let image = match (fill, mask) {
                (true, Some(m)) => {
                    let grown = li_ai::imaging::dilate(&m, 6);
                    let soft = image::imageops::blur(&grown, 3.0);
                    RgbaImage::from_fn(out.width(), out.height(), |x, y| {
                        let mut p = *out.get_pixel(x, y);
                        p[3] = soft.get_pixel(x, y)[0];
                        p
                    })
                }
                _ => out,
            };
            Ok((image, generated.parameters))
        });
        if !ok {
            break;
        }
    }
    ctx.request_repaint();
}

/// Home screen › Generate: the prompt typed there, in Create mode, with the Generate panel open
/// so the results show.
pub fn generate_from_home(app: &mut PhotocraftApp, ctx: &egui::Context) {
    let _ = crate::menus::invoke(app, ctx, "li.panel.generate", json!({}));
    app.ui.ai.generate.mode = Mode::Create;
    start(app, ctx);
}

/// A cloud generation: one job per image, each with its own seed. Edits and fills land on the
/// open document as a layer (a fill holds only the regenerated area).
fn start_cloud(
    app: &mut PhotocraftApp,
    s: &GenerateState,
    c: &'static li_ai::cloud::CloudModel,
    prompt: String,
    doc: Option<OpenImageInput>,
    refs: Vec<RgbaImage>,
) {
    use li_ai::cloud::{CloudMode, CloudRequest};
    let mode = match s.mode {
        Mode::Create => CloudMode::Create,
        Mode::Edit => CloudMode::Edit,
        Mode::Fill => CloudMode::Fill,
        Mode::Refine | Mode::Upscale => {
            app.ui.status = crate::i18n::fmt(tl!("{model} creates, edits and fills; choose a local model to refine or upscale."), &[("model", c.label)]);
            app.ui.status_error = true;
            return;
        }
    };
    let (target, source, mask) = match (&doc, mode) {
        (Some((id, img, sel)), CloudMode::Edit | CloudMode::Fill) => (Some(id.0), Some(img.clone()), if mode == CloudMode::Fill { sel.clone() } else { None }),
        _ => (None, None, None),
    };
    let base = CloudRequest {
        model: c,
        mode,
        prompt: prompt.clone(),
        negative: s.negative.clone(),
        width: s.width,
        height: s.height,
        seed: 0,
        count: 1,
        source,
        mask,
        refs,
        strength: s.denoise,
    };
    if let Err(e) = base.validate() {
        app.ui.status = e.to_string();
        app.ui.status_error = true;
        return;
    }
    let count = if mode == CloudMode::Create { s.count.clamp(1, 4) } else { 1 };
    for i in 0..count {
        let mut r = base.clone();
        r.seed = s.seed.map(|v| v.wrapping_add(i as u64)).unwrap_or_else(li_ai::ops::new_seed);
        let label = format!("{} with {}", s.mode.label(), c.label);
        let shown = crate::i18n::fmt(tl!("{mode} with {model}"), &[("mode", s.mode.shown()), ("model", c.label)]);
        let name = format!("{}-{}", c.model.replace(['.', ' '], ""), r.seed);
        let meta = json!({
            "model": format!("cloud:{}", c.key), "provider": c.provider.label(), "prompt": r.prompt, "negative_prompt": r.negative,
            "width": r.width, "height": r.height, "seed": r.seed, "mode": format!("{:?}", s.mode).to_lowercase(), "reference_count": r.refs.len(),
        });
        let fill_mask = r.mask.clone();
        let ok = launch(app, &label, &shown, name, meta, target, false, move |ctl| {
            let settings = li_ai::settings::AiSettings::load();
            let out = li_ai::cloud::generate(&r, &settings, ctl)?.into_iter().next().ok_or_else(|| anyhow::anyhow!("no image came back"))?;
            Ok(match fill_mask {
                Some(m) => {
                    let soft = image::imageops::blur(&li_ai::imaging::dilate(&m, 6), 3.0);
                    RgbaImage::from_fn(out.width(), out.height(), |x, y| {
                        let mut p = *out.get_pixel(x, y);
                        p[3] = soft.get_pixel(x, y)[0];
                        p
                    })
                }
                None => out,
            })
        });
        if !ok {
            break;
        }
    }
}

/// Starts one job that produces an image, saves it to the library and shows its tile (`shown`:
/// the tile's label in the UI language; the job keeps the English `label`).
#[allow(clippy::too_many_arguments)]
fn launch(
    app: &mut PhotocraftApp,
    label: &str,
    shown: &str,
    name: String,
    meta: Value,
    edit: Option<u64>,
    open: bool,
    work: impl FnOnce(&li_ai::JobControl) -> anyhow::Result<RgbaImage> + Send + 'static,
) -> bool {
    launch_resolved(app, label, shown, name, meta, edit, open, move |ctl| Ok((work(ctl)?, Value::Null)))
}

#[allow(clippy::too_many_arguments)]
fn launch_resolved(
    app: &mut PhotocraftApp,
    label: &str,
    shown: &str,
    name: String,
    mut meta: Value,
    edit: Option<u64>,
    open: bool,
    work: impl FnOnce(&li_ai::JobControl) -> anyhow::Result<(RgbaImage, Value)> + Send + 'static,
) -> bool {
    let params = json!({ "edit": edit, "name": name.clone(), "open": open });
    let library = library();
    let started = app.session.start_job(
        GENERATE_JOB,
        params,
        label,
        false,
        move |ctx| {
            let (img, resolved) = photocraft_engine::ai_cmds::bridged(ctx, work)?;
            if let (Some(meta), Some(resolved)) = (meta.as_object_mut(), resolved.as_object()) {
                meta.extend(resolved.clone());
            }
            ctx.check()?;
            ctx.progress(0.98, "Preparing library image");
            let prepared = library
                .prepare(&img, &name, Some(meta), None)
                .map_err(|e| photocraft_engine::EngineError::Other(format!("Could not prepare the library image: {e:#}")))?;
            ctx.check()?;
            ctx.progress(0.99, "Ready to save to the library");
            Ok(prepared)
        },
        |_, prepared| {
            // Session checks cancellation before applying on the UI thread. A queued
            // result cancelled before this point is dropped with its staging files.
            let entry = prepared.publish().map_err(|e| photocraft_engine::EngineError::Other(format!("Could not save to the library: {e:#}")))?;
            Ok(serde_json::to_value(entry).unwrap_or(Value::Null))
        },
    );
    match started {
        Ok(photocraft_engine::jobs::Started::Job(id)) => {
            rt(|rt| rt.results.push_front(Tile::Pending { job: id.0, label: shown.to_owned(), edit, open }));
            true
        }
        Ok(photocraft_engine::jobs::Started::Done(v)) => {
            if let Ok(e) = serde_json::from_value::<Entry>(v) {
                rt(|rt| rt.results.push_front(Tile::Done(e)));
            }
            true
        }
        Err(e) => {
            app.ui.status = e.to_string();
            app.ui.status_error = true;
            false
        }
    }
}

/// A generation job ended.
pub fn on_generated(app: &mut PhotocraftApp, e: &JobEvent) {
    draft_refine::on_generated(app, e);
    let tile = match &e.outcome {
        JobOutcome::Done(v) => match serde_json::from_value::<Entry>(v.clone()) {
            Ok(entry) => {
                app.ui.status =
                    crate::i18n::fmt(tl!("Generated {name} · seed {seed}"), &[("name", &entry.name), ("seed", &entry.seed().unwrap_or(0).to_string())]);
                app.ui.status_error = false;
                // An edit of an open document lands on it as a new layer.
                let (edit_doc, open) = rt(|r| {
                    r.results
                        .iter()
                        .find_map(|t| match t {
                            Tile::Pending { job, edit, open, .. } if *job == e.id.0 => Some((*edit, *open)),
                            _ => None,
                        })
                        .unwrap_or((None, false))
                });
                if let Some(doc) = edit_doc {
                    place_into(app, doc, &entry);
                } else if open {
                    let path = library().image_path(&entry.id);
                    if let Ok(bytes) = std::fs::read(&path)
                        && let Err(err) = app.open_bytes(&format!("{}.png", entry.name), &bytes)
                    {
                        app.ui.status = err;
                        app.ui.status_error = true;
                    }
                }
                Tile::Done(entry)
            }
            Err(err) => Tile::Failed(err.to_string()),
        },
        JobOutcome::Failed(err) => {
            app.ui.status = crate::i18n::fmt(tl!("Generation failed: {error}"), &[("error", err)]);
            app.ui.status_error = true;
            Tile::Failed(err.clone())
        }
        JobOutcome::Cancelled => Tile::Failed(tl!("Cancelled").into()),
    };
    rt(|r| {
        if let Some(i) = r.results.iter().position(|t| matches!(t, Tile::Pending { job, .. } if *job == e.id.0)) {
            r.results[i] = tile;
        } else {
            r.results.push_front(tile);
        }
        r.results.truncate(48);
        r.library_loaded = None;
    });
}

fn place_into(app: &mut PhotocraftApp, doc_id: u64, entry: &Entry) {
    let Some(index) = app.session.documents().iter().position(|d| d.doc.id.0 == doc_id) else { return };
    let previous = app.session.active().map(|d| d.doc.id);
    app.session.set_active(index);
    let path = library().image_path(&entry.id);
    let name = format!("Edit: {}", entry.prompt().chars().take(40).collect::<String>());
    // Apply through the engine without syncing UI gestures against the temporary target.
    // Sync once after restoring the viewed document, so an in-progress lasso survives.
    if let Err(e) =
        app.session.execute("ai.placeLayer", json!({ "path": path.display().to_string(), "name": name, "fit": "none", "generation": entry.generation }))
    {
        app.ui.status = e.to_string();
        app.ui.status_error = true;
    }
    if let Some(previous) = previous.filter(|id| id.0 != doc_id)
        && let Some(index) = app.session.documents().iter().position(|d| d.doc.id == previous)
    {
        app.session.set_active(index);
    }
    app.sync_views();
}

fn results_ui(app: &mut PhotocraftApp, ui: &mut egui::Ui) {
    let t = Tokens::get(ui.ctx());
    let n = rt(|r| r.results.len());
    if n == 0 {
        ui.label(RichText::new(tl!("Results appear here and are saved to the Library.")).color(t.text_faint).size(11.5));
        return;
    }
    widgets::section_label(ui, tl!("RESULTS"));
    let items: Vec<Option<Entry>> = rt(|r| r.results.iter().map(|t| if let Tile::Done(e) = t { Some(e.clone()) } else { None }).collect());
    let pendings: Vec<String> = rt(|r| {
        r.results
            .iter()
            .map(|t| match t {
                Tile::Pending { label, job, .. } => {
                    let p = app.session.job(photocraft_engine::jobs::JobId(*job)).map(|j| format!("{:.0}%", j.progress * 100.0)).unwrap_or_default();
                    format!("{label}\n{p}")
                }
                Tile::Failed(e) => e.clone(),
                Tile::Done(_) => String::new(),
            })
            .collect()
    });
    tile_grid(app, ui, &items, &pendings, "results");
    if pendings.iter().zip(&items).any(|(p, i)| i.is_none() && !p.is_empty()) {
        ui.ctx().request_repaint_after(Duration::from_millis(250));
    }
}

/// A grid of square tiles; `None` items draw `pending[i]` as text.
fn tile_grid(app: &mut PhotocraftApp, ui: &mut egui::Ui, items: &[Option<Entry>], pending: &[String], salt: &str) {
    let t = Tokens::get(ui.ctx());
    let lib = library();
    let cols = ((ui.available_width() + 6.0) / 96.0).floor().max(2.0) as usize;
    let size = ((ui.available_width() - 6.0 * (cols as f32 - 1.0)) / cols as f32).floor();
    let mut action: Option<(Entry, &'static str)> = None;
    egui::Grid::new(format!("tiles-{salt}")).spacing(vec2(6.0, 6.0)).show(ui, |ui| {
        for (i, item) in items.iter().enumerate() {
            let (rect, resp) = ui.allocate_exact_size(vec2(size, size), Sense::click());
            ui.painter().rect_filled(rect, t.radius, t.field);
            match item {
                Some(e) => {
                    if let Some(tex) = thumb_texture(ui.ctx(), &e.id, &lib.thumbnail_path(&e.id)) {
                        let sz = tex.size_vec2();
                        let k = (size / sz.x).min(size / sz.y);
                        let r = egui::Rect::from_center_size(rect.center(), sz * k);
                        widgets::checker(ui.painter(), r, 6.0);
                        ui.painter().image(tex.id(), r, egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), Color32::WHITE);
                    }
                    let sel = rt(|r| r.selected.as_deref() == Some(e.id.as_str()));
                    if sel || resp.hovered() {
                        ui.painter().rect_stroke(
                            rect,
                            t.radius,
                            Stroke::new(if sel { 2.0 } else { 1.0 }, if sel { t.accent } else { t.text_dim }),
                            StrokeKind::Inside,
                        );
                    }
                    let tip = format!(
                        "{}\n{} · {}×{}{}\n{}",
                        e.prompt(),
                        e.model(),
                        e.width,
                        e.height,
                        e.seed().map(|s| format!(" · {}", crate::i18n::fmt(tl!("seed {seed}"), &[("seed", &s.to_string())]))).unwrap_or_default(),
                        tl!("Double-click to open")
                    );
                    let resp = resp.on_hover_text(tip);
                    if resp.clicked() {
                        rt(|r| r.selected = Some(e.id.clone()));
                    }
                    if resp.double_clicked() {
                        action = Some((e.clone(), "open"));
                    }
                    resp.context_menu(|ui| {
                        for (label, act) in [
                            (tl!("Open"), "open"),
                            (tl!("Place as Layer"), "place"),
                            (tl!("Use as Reference"), "reference"),
                            (tl!("Recreate"), "recreate"),
                            (tl!("Copy Prompt"), "copy"),
                        ] {
                            let enabled = act != "place" || app.session.active().is_some();
                            if ui.add_enabled(enabled, egui::Button::new(label)).clicked() {
                                action = Some((e.clone(), act));
                                ui.close();
                            }
                        }
                        ui.separator();
                        if ui.button(tl!("Delete from Library")).clicked() {
                            action = Some((e.clone(), "delete"));
                            ui.close();
                        }
                    });
                }
                None => {
                    let text = pending.get(i).cloned().unwrap_or_default();
                    let busy = !text.starts_with(tl!("Cancelled")) && text.contains('\n');
                    if busy {
                        let phase = (ui.input(|i| i.time) * 2.0).sin() as f32 * 0.5 + 0.5;
                        ui.painter().rect_filled(rect, t.radius, t.hover.gamma_multiply(0.6 + 0.4 * phase));
                    }
                    let galley = ui.painter().layout(text, egui::FontId::proportional(11.0), if busy { t.text_dim } else { t.warning }, size - 10.0);
                    ui.painter().galley(rect.center() - galley.size() / 2.0, galley, t.text);
                }
            }
            if (i + 1) % cols == 0 {
                ui.end_row();
            }
        }
    });
    if let Some(e) = rt(|r| r.selected.clone()).and_then(|id| items.iter().flatten().find(|e| e.id == id).cloned()) {
        ui.add_space(4.0);
        ui.horizontal_wrapped(|ui| {
            if widgets::secondary_button(ui, tl!("Open"), 0.0).clicked() {
                action = Some((e.clone(), "open"));
            }
            if app.session.active().is_some() && widgets::secondary_button(ui, tl!("Place as Layer"), 0.0).clicked() {
                action = Some((e.clone(), "place"));
            }
            if widgets::secondary_button(ui, tl!("Use as Reference"), 0.0).clicked() {
                action = Some((e.clone(), "reference"));
            }
        });
    }
    if let Some((e, act)) = action {
        entry_action(app, ui.ctx(), &e, act);
    }
}

fn entry_action(app: &mut PhotocraftApp, ctx: &egui::Context, e: &Entry, act: &str) {
    let lib = library();
    match act {
        "open" => match std::fs::read(lib.image_path(&e.id)) {
            Ok(bytes) => {
                if let Err(err) = app.open_bytes(&e.name, &bytes) {
                    app.ui.status = err;
                    app.ui.status_error = true;
                }
            }
            Err(err) => {
                app.ui.status = crate::i18n::fmt(tl!("Could not open {name}: {error}"), &[("name", &e.name), ("error", &err.to_string())]);
                app.ui.status_error = true;
            }
        },
        "place" => {
            let r = app.run(
                "ai.placeLayer",
                json!({ "path": lib.image_path(&e.id).display().to_string(), "name": e.name.trim_end_matches(".png"), "fit": "contain", "generation": e.generation }),
            );
            if let Err(err) = r {
                app.ui.status = err;
                app.ui.status_error = true;
            }
        }
        "reference" => match lib.load(e) {
            Ok(img) => {
                app.ui.ai.generate.mode = Mode::Create;
                add_reference(ctx, e.name.clone(), img);
            }
            Err(err) => {
                app.ui.status = format!("{err:#}");
                app.ui.status_error = true;
            }
        },
        "recreate" => {
            if let Some(g) = &e.generation {
                let s = &mut app.ui.ai.generate;
                match g.get("model").and_then(Value::as_str) {
                    Some(k) if k.starts_with("custom:") => s.model = k.to_owned(),
                    Some(k) => {
                        if let Some(m) = ModelId::from_key(k) {
                            set_model(s, m);
                        }
                    }
                    None => {}
                }
                if let Some(n) = g.get("negative_prompt").and_then(Value::as_str) {
                    s.negative = n.to_owned();
                }
                s.prompt = e.prompt().to_owned();
                if let Some(v) = g.get("variant").and_then(Value::as_str) {
                    s.variant = v.to_owned();
                }
                s.width = e.width;
                s.height = e.height;
                s.aspect = "custom".into();
                s.mode = Mode::Create;
                s.seed = None;
                app.ui.dock_tabs.generate = 0;
            }
        }
        "copy" => ctx.copy_text(e.prompt().to_owned()),
        "delete" => {
            let _ = lib.delete(std::slice::from_ref(&e.id));
            rt(|r| {
                r.results.retain(|t| !matches!(t, Tile::Done(x) if x.id == e.id));
                Arc::make_mut(&mut r.library).retain(|x| x.id != e.id);
                r.textures.remove(&e.id);
            });
        }
        _ => {}
    }
}

/// The Library tab: every generated image, newest first.
pub fn library_panel(app: &mut PhotocraftApp, ui: &mut egui::Ui) {
    let t = Tokens::get(ui.ctx());
    if rt(|r| r.library_loaded.is_none_or(|t| t.elapsed() > Duration::from_secs(10))) {
        let list = library().list();
        rt(|r| {
            r.library = Arc::new(list);
            r.library_loaded = Some(Instant::now());
        });
    }
    ui.horizontal(|ui| {
        ui.add(egui::TextEdit::singleline(&mut app.ui.ai.generate.library_filter).hint_text(tl!("Search prompts")).desired_width(ui.available_width() - 30.0));
        if crate::icons::button(ui, "rotate-cw", 22.0, false, tl!("Refresh")).clicked() {
            rt(|r| r.library_loaded = None);
        }
    });
    let filter = app.ui.ai.generate.library_filter.to_lowercase();
    let items: Vec<Option<Entry>> = rt(|r| {
        r.library
            .iter()
            .filter(|e| filter.is_empty() || e.prompt().to_lowercase().contains(&filter) || e.name.to_lowercase().contains(&filter))
            .take(200)
            .cloned()
            .map(Some)
            .collect()
    });
    if items.is_empty() {
        ui.add_space(12.0);
        ui.label(RichText::new(if filter.is_empty() { tl!("Generated images are kept here.") } else { tl!("Nothing matches.") }).color(t.text_faint));
        return;
    }
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        tile_grid(app, ui, &items, &[], "library");
    });
}

// ------------------------------------------------------------------------------ Open Folder

/// File › Open Folder…: opens a folder's images (natural order). The filmstrip shows them.
pub fn open_folder(app: &mut PhotocraftApp) -> Result<Value, String> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let Some(dir) = rfd::FileDialog::new().set_title(tl!("Open Folder")).pick_folder() else { return Ok(Value::Null) };
        open_folder_path(app, &dir)
    }
    #[cfg(target_arch = "wasm32")]
    {
        let _ = app;
        Err("Open Folder needs the desktop app".into())
    }
}

pub const IMAGE_EXTS: &[&str] = &[
    "psd", "psb", "pcraft", "png", "jpg", "jpeg", "tif", "tiff", "webp", "gif", "bmp", "tga", "exr", "hdr", "heic", "heif", "dng", "cr2", "cr3", "nef", "nrw",
    "arw", "pef", "orf", "rw2", "raf",
];

pub fn folder_images(dir: &std::path::Path) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .map(|rd| {
            rd.flatten()
                .map(|e| e.path())
                .filter(|p| p.is_file() && p.extension().and_then(|e| e.to_str()).is_some_and(|e| IMAGE_EXTS.contains(&e.to_ascii_lowercase().as_str())))
                .collect()
        })
        .unwrap_or_default();
    files.sort_by(|a, b| natural_cmp(&a.to_string_lossy(), &b.to_string_lossy()));
    files
}

/// Natural sort: "img2" before "img10".
pub fn natural_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    let (mut ai, mut bi) = (a.chars().peekable(), b.chars().peekable());
    loop {
        match (ai.peek().copied(), bi.peek().copied()) {
            (None, None) => return std::cmp::Ordering::Equal,
            (None, _) => return std::cmp::Ordering::Less,
            (_, None) => return std::cmp::Ordering::Greater,
            (Some(x), Some(y)) if x.is_ascii_digit() && y.is_ascii_digit() => {
                let mut na = String::new();
                while let Some(c) = ai.peek().copied().filter(char::is_ascii_digit) {
                    na.push(c);
                    ai.next();
                }
                let mut nb = String::new();
                while let Some(c) = bi.peek().copied().filter(char::is_ascii_digit) {
                    nb.push(c);
                    bi.next();
                }
                let o = na
                    .trim_start_matches('0')
                    .len()
                    .cmp(&nb.trim_start_matches('0').len())
                    .then_with(|| na.trim_start_matches('0').cmp(nb.trim_start_matches('0')));
                if o != std::cmp::Ordering::Equal {
                    return o;
                }
            }
            (Some(x), Some(y)) => {
                let o = x.to_lowercase().cmp(y.to_lowercase());
                if o != std::cmp::Ordering::Equal {
                    return o;
                }
                ai.next();
                bi.next();
            }
        }
    }
}

pub fn open_folder_path(app: &mut PhotocraftApp, dir: &std::path::Path) -> Result<Value, String> {
    let files = folder_images(dir);
    if files.is_empty() {
        return Err(format!("{} has no images Local Image can open.", dir.display()));
    }
    // Open the first image now; the rest wait in the filmstrip (opened on click).
    crate::filmstrip_ui::set_folder(dir, files.clone());
    let first = files[0].clone();
    let bytes = std::fs::read(&first).map_err(|e| e.to_string())?;
    let _ = app.open_file(&first.display().to_string(), &bytes)?;
    app.ui.status = crate::i18n::fmt(tl!("{n} images in {folder}"), &[("n", &files.len().to_string()), ("folder", &dir.display().to_string())]);
    app.ui.status_error = false;
    Ok(json!({ "count": files.len() }))
}

// ------------------------------------------------------------------------------ batch

/// File › Automate › Remove Backgrounds (AI).
#[derive(Default)]
pub struct BatchState {
    files: Vec<PathBuf>,
    output: Option<PathBuf>,
    /// `transparent`, `white`, `color`.
    background: String,
    color: [u8; 3],
    variant: String,
    items: Vec<(String, String)>,
    run: Option<std::sync::Arc<std::sync::Mutex<BatchRun>>>,
}

#[derive(Default)]
struct BatchRun {
    done: usize,
    status: Vec<(String, String)>,
    finished: bool,
    ctl: li_ai::JobControl,
}

fn batch<R>(f: impl FnOnce(&mut Option<BatchState>) -> R) -> R {
    thread_local! { static B: std::cell::RefCell<Option<BatchState>> = const { std::cell::RefCell::new(None) }; }
    B.with(|b| f(&mut b.borrow_mut()))
}

pub fn open_batch() {
    batch(|b| {
        if b.is_none() {
            *b = Some(BatchState { background: "transparent".into(), color: [255, 255, 255], variant: "int8".into(), ..Default::default() });
        }
    });
}

pub fn batch_window(app: &mut PhotocraftApp, ctx: &egui::Context) {
    if batch(|b| b.is_none()) {
        return;
    }
    let t = Tokens::get(ctx);
    let mut open = true;
    let st = crate::ai_ui::status();
    egui::Window::new(tl!("Remove Backgrounds"))
        .collapsible(false)
        .resizable(true)
        .default_size(vec2(560.0, 480.0))
        .frame(egui::Frame::window(&ctx.global_style()).fill(t.card).stroke(Stroke::new(1.0, t.card_border)).inner_margin(egui::Margin::same(16)))
        .open(&mut open)
        .anchor(egui::Align2::CENTER_CENTER, vec2(0.0, 0.0))
        .show(ctx, |ui| {
            batch(|b| {
                let Some(b) = b.as_mut() else { return };
                ui.label(
                    RichText::new(tl!("Cuts out the subject of each image with Qwen Image 2.1 and saves PNGs. Originals are never changed.")).color(t.text_dim),
                );
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    #[cfg(not(target_arch = "wasm32"))]
                    {
                        if widgets::secondary_button(ui, tl!("Add Images…"), 0.0).clicked()
                            && let Some(paths) = rfd::FileDialog::new().add_filter(tl!("Images"), &["png", "jpg", "jpeg", "webp", "tif", "tiff"]).pick_files()
                        {
                            b.files.extend(paths);
                        }
                        if widgets::secondary_button(ui, tl!("Add Folder…"), 0.0).clicked()
                            && let Some(dir) = rfd::FileDialog::new().pick_folder()
                        {
                            b.files.extend(folder_images(&dir).into_iter().filter(|p| {
                                p.extension()
                                    .and_then(|e| e.to_str())
                                    .is_some_and(|e| ["png", "jpg", "jpeg", "webp", "tif", "tiff"].contains(&e.to_ascii_lowercase().as_str()))
                            }));
                        }
                    }
                    ui.label(RichText::new(crate::i18n::fmt(tl!("{n} images"), &[("n", &b.files.len().to_string())])).color(t.text_dim));
                    if !b.files.is_empty() && widgets::secondary_button(ui, tl!("Clear"), 0.0).clicked() {
                        b.files.clear();
                    }
                });
                ui.horizontal(|ui| {
                    ui.label(RichText::new(tl!("Background:")).color(t.text_dim));
                    let opts = [("transparent".to_string(), tl!("Transparent PNG")), ("white".to_string(), tl!("White")), ("color".to_string(), tl!("Color"))];
                    widgets::dropdown(ui, "batch-bg", &mut b.background, &opts, 150.0);
                    if b.background == "color" {
                        ui.color_edit_button_srgb(&mut b.color);
                    }
                    ui.label(RichText::new(tl!("Model:")).color(t.text_dim));
                    let opts = [("int8".to_string(), tl!("Qwen Compact")), ("bf16".to_string(), tl!("Qwen Full"))];
                    widgets::dropdown(ui, "batch-model", &mut b.variant, &opts, 130.0);
                });
                ui.horizontal(|ui| {
                    ui.label(RichText::new(tl!("Save to:")).color(t.text_dim));
                    let out = b.output.as_ref().map(|p| p.display().to_string()).unwrap_or_else(|| tl!("a “cutouts” folder beside each image").into());
                    ui.label(out);
                    #[cfg(not(target_arch = "wasm32"))]
                    if widgets::secondary_button(ui, tl!("Choose…"), 0.0).clicked() {
                        b.output = rfd::FileDialog::new().pick_folder();
                    }
                });
                if let Err(why) = st.ready(ModelId::Qwen, &b.variant) {
                    ui.label(RichText::new(why).color(t.warning));
                }
                ui.add_space(6.0);
                let run = b.run.clone();
                let running = run.as_ref().is_some_and(|r| r.lock().map(|r| !r.finished).unwrap_or(false));
                egui::ScrollArea::vertical().max_height(220.0).show(ui, |ui| {
                    let rows: Vec<(String, String)> = match &run {
                        Some(r) => r.lock().map(|r| r.status.clone()).unwrap_or_default(),
                        None => b
                            .files
                            .iter()
                            .map(|p| (p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default(), tl!("Waiting").into()))
                            .collect(),
                    };
                    for (name, s) in rows {
                        ui.horizontal(|ui| {
                            ui.label(name);
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| ui.label(RichText::new(s).color(t.text_dim)));
                        });
                    }
                });
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if running {
                        if let Some(r) = &run {
                            let (d, n) = r.lock().map(|r| (r.done, r.status.len())).unwrap_or((0, 1));
                            ui.add(
                                egui::ProgressBar::new(d as f32 / n.max(1) as f32)
                                    .desired_width(240.0)
                                    .text(crate::i18n::fmt(tl!("{done} of {total}"), &[("done", &d.to_string()), ("total", &n.to_string())])),
                            );
                            if widgets::secondary_button(ui, tl!("Stop"), 0.0).clicked()
                                && let Ok(r) = r.lock()
                            {
                                r.ctl.cancel();
                            }
                        }
                        ui.ctx().request_repaint_after(Duration::from_millis(250));
                    } else {
                        let ok = !b.files.is_empty() && st.ready(ModelId::Qwen, &b.variant).is_ok();
                        if ui.add_enabled_ui(ok, |ui| widgets::primary_button(ui, tl!("Remove Backgrounds"), 0.0)).inner.clicked() {
                            b.run = Some(start_batch(b));
                        }
                    }
                });
                let _ = &b.items;
            });
        });
    if !open {
        batch(|b| {
            if let Some(r) = b.as_ref().and_then(|b| b.run.clone())
                && let Ok(r) = r.lock()
            {
                r.ctl.cancel();
            }
            *b = None;
        });
    }
    let _ = app;
}

fn start_batch(b: &BatchState) -> std::sync::Arc<std::sync::Mutex<BatchRun>> {
    let run = std::sync::Arc::new(std::sync::Mutex::new(BatchRun {
        status: b.files.iter().map(|p| (p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default(), tl!("Waiting").into())).collect(),
        ..Default::default()
    }));
    let (files, output, bg, color, variant) = (b.files.clone(), b.output.clone(), b.background.clone(), b.color, b.variant.clone());
    let r2 = run.clone();
    let (working, saved, stopped) = (tl!("Working…"), tl!("Saved {name}"), tl!("Stopped"));
    let _ = std::thread::Builder::new().name("batch-cutout".into()).spawn(move || {
        let ai = li_ai::service();
        let ctl = r2.lock().map(|r| r.ctl.clone()).unwrap_or_default();
        for (i, path) in files.iter().enumerate() {
            if ctl.is_cancelled() {
                break;
            }
            let set = |s: String| {
                if let Ok(mut r) = r2.lock() {
                    r.status[i].1 = s;
                }
            };
            set(working.into());
            let res = (|| -> anyhow::Result<PathBuf> {
                let img = photocraft_engine::ai_cmds::file_rgba_srgb(path)?;
                let alpha = ai.cutout(&img, &variant, "", li_ai::ops::new_seed(), &ctl)?;
                let out = image::RgbaImage::from_fn(img.width(), img.height(), |x, y| {
                    let p = img.get_pixel(x, y);
                    let a = alpha.get_pixel(x, y)[0] as f32 / 255.0 * p[3] as f32 / 255.0;
                    match bg.as_str() {
                        "transparent" => image::Rgba([p[0], p[1], p[2], (a * 255.0).round() as u8]),
                        _ => {
                            let c = if bg == "white" { [255, 255, 255] } else { color };
                            let m = |k: usize| (p[k] as f32 * a + c[k] as f32 * (1.0 - a)).round() as u8;
                            image::Rgba([m(0), m(1), m(2), 255])
                        }
                    }
                });
                let dir = output.clone().unwrap_or_else(|| path.parent().map(|p| p.join("cutouts")).unwrap_or_default());
                std::fs::create_dir_all(&dir)?;
                let stem = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "image".into());
                let mut dest = dir.join(format!("{stem}-cutout.png"));
                let mut n = 2;
                while dest.exists() {
                    dest = dir.join(format!("{stem}-cutout-{n}.png"));
                    n += 1;
                }
                out.save(&dest)?;
                Ok(dest)
            })();
            match res {
                Ok(dest) => set(crate::i18n::fmt(saved, &[("name", &dest.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default())])),
                Err(e) if e.is::<li_ai::Cancelled>() => set(stopped.into()),
                Err(e) => set(format!("{e:#}")),
            }
            if let Ok(mut r) = r2.lock() {
                r.done = i + 1;
            }
        }
        if let Ok(mut r) = r2.lock() {
            r.finished = true;
        }
    });
    run
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enhance_generate_upscale_clicks_three_methods_and_scale() {
        use egui_kittest::kittest::Queryable;
        let _lock = crate::ai_ui::AI_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        rt(|r| r.upscale_any = false);
        let mut session = photocraft_engine::Session::new();
        session.execute("file.new", json!({"width": 32, "height": 24})).unwrap();
        let mut app = PhotocraftApp::new(session, crate::Services::default());
        app.ui.ai.generate.mode = Mode::Upscale;
        app.ui.ai.generate.model = ModelId::SeedVr2.key().into();
        let mut initialized = false;
        let mut h = egui_kittest::Harness::builder().with_size(vec2(620.0, 900.0)).build_ui_state(
            move |ui, app: &mut PhotocraftApp| {
                if !initialized {
                    PhotocraftApp::setup_context(ui.ctx(), crate::theme::ThemeKind::Studio);
                    initialized = true;
                    ui.ctx().request_repaint();
                    return;
                }
                panel(app, ui);
            },
            app,
        );
        h.run_steps(3);
        h.get_by_label("Faithful").click();
        h.run_steps(2);
        assert_eq!(h.state().ui.ai.generate.enhance_method, li_ai::enhance::Method::Faithful);
        h.get_by_label("4×").click();
        h.run_steps(2);
        assert_eq!(h.state().ui.ai.generate.upscale, 4.0);
        h.get_by_label("Rescue").click();
        h.run_steps(2);
        assert_eq!(h.state().ui.ai.generate.enhance_method, li_ai::enhance::Method::Rescue);
        h.get_by_label("Generative (SeedVR2)").click();
        h.run_steps(2);
        assert_eq!(h.state().ui.ai.generate.enhance_method, li_ai::enhance::Method::Generative);
        assert_eq!(serde_json::from_value::<GenerateState>(json!({})).unwrap().enhance_method, li_ai::enhance::Method::Generative);
    }

    #[test]
    fn ds_finding_selected_layer_refine_is_disabled_without_a_selected_layer() {
        use egui_kittest::kittest::Queryable;
        crate::ai_ui::TEST_STATUS.with(|s| {
            *s.borrow_mut() = Some(crate::ai_ui::EngineStatus {
                connected: true,
                checked: true,
                presets: catalog::presets().iter().map(|p| (p.id(), Ok(()))).collect(),
                ..Default::default()
            })
        });
        let mut session = photocraft_engine::Session::new();
        session.execute("file.new", json!({"width":64,"height":64})).unwrap();
        session.active_mut().unwrap().active_layer = None;
        let mut app = PhotocraftApp::new(session, Default::default());
        app.ui.ai.generate.mode = Mode::Refine;
        app.ui.ai.generate.refine_input = RefineInput::SelectedLayer;
        let mut initialized = false;
        let mut h = egui_kittest::Harness::builder().with_size(vec2(420.0, 1600.0)).build_ui_state(
            move |ui, app: &mut PhotocraftApp| {
                if !initialized {
                    PhotocraftApp::setup_context(ui.ctx(), crate::theme::ThemeKind::Studio);
                    initialized = true;
                    return;
                }
                panel(app, ui);
            },
            app,
        );
        h.run_steps(4);
        h.get_all_by_label("Refine").last().unwrap().click();
        h.run_steps(3);
        assert!(h.state().session.jobs().is_empty());
        let ctx = h.ctx.clone();
        start(h.state_mut(), &ctx);
        assert_eq!(h.state().ui.status, "Select a layer first.");
        assert!(h.state().session.jobs().is_empty());
    }

    #[test]
    fn refine_model_groups_exclude_upscalers_tools_and_video() {
        let mut models = catalog::catalog().models.clone();
        // Detected video/template-only models are represented by FamilyKind::Basic.
        let mut video = ModelId::Klein4B.info().clone();
        video.id = ModelId::intern("video:refine-picker-test");
        video.resolved.kind = li_ai::family::FamilyKind::Basic;
        models.push(video);
        let mut upscale = ModelId::Klein4B.info().clone();
        upscale.id = ModelId::intern("upscale:refine-picker-test");
        upscale.upscale = true;
        models.push(upscale);
        let groups = model_groups(&models, Mode::Refine);
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].0, "Edit models — refine by reference");
        assert_eq!(groups[1].0, "Image models — refine by denoise");
        let ids = |group: usize| groups[group].1.iter().map(|m| m.id).collect::<Vec<_>>();
        assert!(ids(0).contains(&ModelId::Klein9B));
        assert!(ids(0).contains(&ModelId::Qwen));
        assert!(ids(1).contains(&ModelId::ZImageTurbo));
        for excluded in [ModelId::SeedVr2, ModelId::KleinRemove, ModelId::intern("video:refine-picker-test"), ModelId::intern("upscale:refine-picker-test")] {
            assert!(!ids(0).contains(&excluded) && !ids(1).contains(&excluded), "{excluded:?}");
        }
    }

    #[test]
    fn refine_panel_mode_and_second_pass_use_model_defaults_and_grouped_picker() {
        use egui::accesskit::Role;
        use egui_kittest::kittest::Queryable;
        let _lock = crate::ai_ui::AI_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let mut session = photocraft_engine::Session::new();
        session.execute("file.new", json!({"width": 64, "height": 64})).unwrap();
        let app = PhotocraftApp::new(session, crate::Services::default());
        assert_eq!(app.ui.ai.generate.refine_strength, 1.0);
        crate::ai_ui::TEST_STATUS.with(|s| {
            *s.borrow_mut() = Some(crate::ai_ui::EngineStatus {
                connected: true,
                checked: true,
                presets: catalog::presets().iter().map(|p| (p.id(), Ok(()))).collect(),
                ..Default::default()
            });
        });
        let mut initialized = false;
        let mut h = egui_kittest::Harness::builder().with_size(vec2(420.0, 1800.0)).with_max_steps(64).build_ui_state(
            move |ui, app: &mut PhotocraftApp| {
                if !initialized {
                    PhotocraftApp::setup_context(ui.ctx(), crate::theme::ThemeKind::Studio);
                    initialized = true;
                    ui.ctx().request_repaint();
                    return;
                }
                panel(app, ui);
            },
            app,
        );
        h.run_steps(3);
        h.get_by_role_and_label(Role::Button, "Refine").click();
        h.run_steps(3);
        assert_eq!(h.state().ui.ai.generate.denoise, 1.0);
        h.get_by_label("Refines by reference: the image guides the result");
        h.get_by_value(&format!("{} · {}", ModelId::Klein4B.info().label, ModelId::Klein4B.info().family_label)).click();
        h.run_steps(2);
        h.get_by_label("Edit models — refine by reference");
        h.get_by_label("Image models — refine by denoise");
        for excluded in [ModelId::SeedVr2, ModelId::KleinRemove] {
            assert!(h.query_by_role_and_label(Role::Button, &excluded.info().label).is_none());
        }
        h.get_by_role_and_label(Role::Button, &ModelId::ZImageTurbo.info().label).click();
        h.run_steps(3);
        assert_eq!(h.state().ui.ai.generate.denoise, 0.35);
        h.get_by_label("Refines by denoise: lower keeps more of the image");
        h.state_mut().ui.ai.generate.denoise = 0.72;
        h.get_by_value(&format!("{} · {}", ModelId::ZImageTurbo.info().label, ModelId::ZImageTurbo.info().family_label)).click();
        h.run_steps(2);
        h.get_by_role_and_label(Role::Button, &ModelId::Klein9B.info().label).click();
        h.run_steps(3);
        assert_eq!(h.state().ui.ai.generate.denoise, 1.0);
        h.get_by_role_and_label(Role::Button, "Create").click();
        h.run_steps(3);
        h.get_by_label("Refine with").click();
        h.run_steps(2);
        h.get_by_value("Choose a model").click();
        h.run_steps(2);
        h.get_by_role_and_label(Role::Button, &ModelId::Klein9B.info().label).click();
        h.run_steps(3);
        assert_eq!(h.state().ui.ai.generate.refine_strength, 1.0);
        h.state_mut().ui.ai.generate.refine_strength = 0.72;
        h.get_all_by_value(&format!("{} · {}", ModelId::Klein9B.info().label, ModelId::Klein9B.info().family_label)).last().unwrap().click();
        h.run_steps(2);
        h.get_by_role_and_label(Role::Button, &ModelId::ZImageTurbo.info().label).click();
        h.run_steps(3);
        assert_eq!(h.state().ui.ai.generate.refine_strength, 0.35);
        h.get_by_label("Refines by denoise: lower keeps more of the image");
        // Missing preset strengths and non-finite request values also use the selected model.
        let s = &mut h.state_mut().ui.ai.generate;
        let mut preset = li_ai::presets::GenPreset { refine_model: Some(ModelId::Klein9B.key().into()), ..Default::default() };
        apply_preset(s, &preset);
        assert_eq!(s.refine_strength, 1.0);
        s.refine_strength = f32::NAN;
        let (req, _, _) = local_request(s, "detail".into(), None, Vec::new()).unwrap();
        assert_eq!(req.refine.unwrap().strength, 1.0);
        preset.refine_strength = Some(0.6);
        apply_preset(s, &preset);
        assert_eq!(s.refine_strength, 0.6, "an explicit saved strength stays editable");
    }

    #[test]
    fn steps_and_guidance_follow_the_backend_limits_only() {
        let limits = li_ai::comfy::SamplerLimits { steps: (1.0, 10000.0), cfg: (0.0, 100.0), denoise: (0.0, 1.0) };
        // well outside every model's recommendation, but inside what ComfyUI accepts: kept as typed
        let mut s = GenerateState { steps: 150, guidance: 25.0, ..Default::default() };
        fit_to_backend(&mut s, Some(limits));
        assert_eq!((s.steps, s.guidance), (150, 25.0));
        s.steps = 20000;
        s.guidance = 500.0;
        fit_to_backend(&mut s, Some(limits));
        assert_eq!((s.steps, s.guidance), (10000, 100.0));
        s.steps = 0;
        fit_to_backend(&mut s, None);
        assert_eq!(s.steps, 1);
        let real = li_ai::comfy::ObjectInfo(serde_json::json!({"KSampler": {"input": {"required": {
            "steps": ["INT", {"default": 20, "min": 1, "max": 10000}], "cfg": ["FLOAT", {"default": 8.0, "min": 0.0, "max": 100.0}],
            "denoise": ["FLOAT", {"default": 1.0, "min": 0.0, "max": 1.0}]}}}}));
        assert_eq!(real.sampler_limits(), Some(limits));
    }

    #[test]
    fn refine_with_lora_picker_and_precision_carry_the_second_pass_settings() {
        use egui_kittest::kittest::Queryable;
        let _lock = crate::ai_ui::AI_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        li_ai::inventory::refresh(
            &li_ai::comfy::ObjectInfo(
                json!({"LoraLoader": {"input": {"required": {"lora_name": [["qwen_image_2.1_style.safetensors", "flux_realism_lora.safetensors"]]}}}}),
            ),
            None,
        );
        struct Reset;
        impl Drop for Reset {
            fn drop(&mut self) {
                li_ai::inventory::refresh(&li_ai::comfy::ObjectInfo(json!({})), None);
            }
        }
        let _reset = Reset;
        let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
        app.ui.ai.generate.prompt = "a castle".into();
        let st = crate::ai_ui::EngineStatus {
            connected: true,
            checked: true,
            presets: catalog::presets().iter().map(|p| (p.id(), Ok(()))).collect(),
            ..Default::default()
        };
        crate::ai_ui::TEST_STATUS.with(|s| *s.borrow_mut() = Some(st));
        // Render the real panel with enough room for the expanded second-pass controls.
        let mut initialized = false;
        let mut h = egui_kittest::Harness::builder().with_size(vec2(420.0, 1600.0)).with_max_steps(64).build_ui_state(
            move |ui, app: &mut PhotocraftApp| {
                if !initialized {
                    PhotocraftApp::setup_context(ui.ctx(), crate::theme::ThemeKind::Studio);
                    initialized = true;
                    ui.ctx().request_repaint();
                    return;
                }
                panel(app, ui);
            },
            app,
        );
        h.run_steps(3);
        // The panel exposes the same size component as the Refinement workspace.
        h.get_by_role_and_label(egui::accesskit::Role::Button, "4:3").click();
        h.run_steps(2);
        assert_eq!(h.state().ui.ai.generate.aspect, "4:3");
        assert!(h.state().ui.ai.generate.aspect_locked);
        let grid = model_of(&h.state().ui.ai.generate).info().resolved.sizes.multiple.max(8);
        assert_eq!((h.state().ui.ai.generate.width % grid, h.state().ui.ai.generate.height % grid), (0, 0));
        h.get_by_label("Link width and height").click();
        h.run_steps(2);
        assert!(!h.state().ui.ai.generate.aspect_locked);
        h.get_by_label("Refine with").click();
        h.run_steps(2);
        h.get_by_value("Choose a model").click();
        h.run_steps(2);
        h.get_by_role_and_label(egui::accesskit::Role::Button, &ModelId::Qwen.info().label).click();
        h.run_steps(3);
        let current_variant = h.state().ui.ai.generate.refine_variant.clone();
        let current_label = catalog::presets_for(ModelId::Qwen).find(|p| p.variant == current_variant).unwrap().label.clone();
        let alternate = catalog::presets_for(ModelId::Qwen).find(|p| p.variant != current_variant).unwrap();
        h.get_all_by_value(&current_label).last().unwrap().click();
        h.run_steps(2);
        h.get_by_role_and_label(egui::accesskit::Role::Button, &alternate.label).click();
        h.run_steps(2);
        assert_eq!(h.state().ui.ai.generate.refine_variant, alternate.variant);
        h.get_all_by_label("+ Add").last().unwrap().click();
        h.run_steps(2);
        assert!(h.query_by_label("flux realism lora").is_none());
        h.get_by_label("qwen image 2.1 style").click();
        h.run_steps(3);
        let s = &h.state().ui.ai.generate;
        assert!(s.loras.is_empty());
        assert_eq!(s.refine_loras, vec![LoraPick { name: "qwen_image_2.1_style.safetensors".into(), strength: 0.8 }]);
        let (req, _, _) = local_request(s, s.prompt.clone(), None, Vec::new()).unwrap();
        let refine = req.refine.unwrap();
        assert_eq!(refine.variant, alternate.variant);
        assert_eq!(refine.loras, vec![li_ai::workflows::LoraUse { name: "qwen_image_2.1_style.safetensors".into(), strength: 0.8 }]);
        let steps = h.get_by_role_and_label(egui::accesskit::Role::SpinButton, "Refine steps").rect();
        h.hover_at(steps.center());
        h.event(egui::Event::PointerButton { pos: steps.center(), button: egui::PointerButton::Primary, pressed: true, modifiers: Default::default() });
        h.step();
        h.event(egui::Event::PointerMoved(steps.center() + vec2(20.0, 0.0)));
        h.step();
        h.event(egui::Event::PointerButton {
            pos: steps.center() + vec2(20.0, 0.0),
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: Default::default(),
        });
        h.run_steps(2);
        let s = &mut h.state_mut().ui.ai.generate;
        assert!(s.refine_steps.is_some());
        let (req, _, _) = local_request(s, s.prompt.clone(), None, Vec::new()).unwrap();
        assert_eq!(req.refine.unwrap().steps, s.refine_steps);
        s.refine_strength = f32::NAN;
        let (req, _, _) = local_request(s, s.prompt.clone(), None, Vec::new()).unwrap();
        assert_eq!(req.refine.unwrap().strength, catalog::refine_default_denoise(ModelId::Qwen), "a second-pass fallback follows its model");
        let old: GenerateState = serde_json::from_value(json!({"refine": true})).unwrap();
        assert!(old.refine_loras.is_empty());
    }

    #[test]
    fn natural_order() {
        let mut v = vec!["img10.jpg", "img2.jpg", "IMG1.jpg", "a.png"];
        v.sort_by(|a, b| natural_cmp(a, b));
        assert_eq!(v, vec!["a.png", "IMG1.jpg", "img2.jpg", "img10.jpg"]);
    }

    #[test]
    fn aspect_sizes_are_about_a_megapixel_on_the_grid() {
        let s = GenerateState::default();
        for (k, r) in ASPECTS {
            let (w, h) = size_for(k, &s, 1024);
            assert!(w % 16 == 0 && h % 16 == 0);
            assert!(((w as f32 / h as f32) - r).abs() < 0.05, "{k}: {w}x{h}");
            assert!((800_000..1_300_000).contains(&(w * h)), "{k}: {w}x{h}");
        }
    }

    /// Upscale lists the official upscalers (SeedVR2) and routes them to AI Enhance; "Choose
    /// another model…" switches to every image model (diffusion refine-upscale).
    #[test]
    fn upscale_offers_the_official_upscalers_first() {
        let official: Vec<&str> = catalog::catalog().models.iter().filter(|m| fits_mode_with(m, Mode::Upscale, false)).map(|m| m.id.key()).collect();
        assert_eq!(official, vec![ModelId::SeedVr2.key(), "realplksr-2x", "realplksr-4x", "bsrgan-2x", "bsrgan-4x"]);
        let any: Vec<ModelId> = catalog::catalog().models.iter().filter(|m| fits_mode_with(m, Mode::Upscale, true)).map(|m| m.id).collect();
        assert!(any.contains(&ModelId::Klein4B) && any.contains(&ModelId::Qwen) && !any.contains(&ModelId::SeedVr2), "{any:?}");
        // other modes never offer the upscaler
        for mode in [Mode::Create, Mode::Edit, Mode::Fill, Mode::Refine] {
            assert!(!fits_mode_with(ModelId::SeedVr2.info(), mode, false), "{mode:?}");
        }
        assert!(runs_as_enhance(ModelId::SeedVr2.info(), Mode::Upscale));
        assert!(!runs_as_enhance(ModelId::Klein4B.info(), Mode::Upscale));
        assert!(!runs_as_enhance(ModelId::SeedVr2.info(), Mode::Refine));
        // the picker's filter follows the session's choice
        assert!(!upscale_any());
        assert!(!fits_mode(ModelId::Klein4B.info(), Mode::Upscale));
        rt(|r| r.upscale_any = true);
        assert!(fits_mode(ModelId::Klein4B.info(), Mode::Upscale));
        rt(|r| r.upscale_any = false);
    }

    /// The model list holds models only: LoRA-capable models carry no "LoRA" chip.
    #[test]
    fn model_tags_do_not_call_models_loras() {
        for m in &catalog::catalog().models {
            assert!(!m.tags().iter().any(|t| t.contains("LoRA")), "{}: {:?}", m.label, m.tags());
        }
    }

    /// The Styles (LoRA) row always offers the LoRA browser, opened on the model's family.
    #[test]
    fn styles_row_opens_the_lora_browser() {
        use egui_kittest::kittest::Queryable;
        let fam = ModelId::Klein4B.info().resolved.clone();
        let mut initialized = false;
        let mut h = egui_kittest::Harness::builder().with_size(egui::vec2(360.0, 200.0)).build_ui_state(
            move |ui, s: &mut GenerateState| {
                if !initialized {
                    PhotocraftApp::setup_context(ui.ctx(), crate::theme::ThemeKind::Studio);
                    initialized = true;
                    ui.ctx().request_repaint();
                    return;
                }
                let t = Tokens::get(ui.ctx());
                loras_ui(ui, &mut s.loras, &fam, &t);
            },
            GenerateState::default(),
        );
        h.run_steps(2);
        crate::model_browser::close();
        h.get_by_label("Browse LoRAs…").click();
        h.run_steps(2);
        assert!(crate::model_browser::is_open());
        assert_eq!(crate::model_browser::showing(), (true, Some(ModelId::Klein4B.info().resolved.id.clone())));
        crate::model_browser::close();
        // and from the model picker's footer, for a model of any family
        browse_loras("no-such-model");
        assert!(crate::model_browser::showing().0);
        crate::model_browser::close();
    }
}

#[cfg(test)]
#[test]
fn ds_finding_refine_preset_persists_custom_sampler_and_loads_old_defaults() {
    let mut state = GenerateState { refine: true, refine_steps: Some(17), refine_guidance: Some(2.5), ..Default::default() };
    save_preset(&mut state, "DS refine regression");
    let saved = li_ai::presets::all(&preset_path()).into_iter().find(|p| p.name == "DS refine regression").unwrap();
    assert_eq!(saved.refine_steps, Some(17));
    assert_eq!(saved.refine_guidance, Some(2.5));
    let mut loaded = GenerateState::default();
    apply_preset(&mut loaded, &saved);
    assert_eq!((loaded.refine_steps, loaded.refine_guidance), (Some(17), Some(2.5)));
    let old: li_ai::presets::GenPreset = serde_json::from_value(json!({"name":"old","refine_model":"flux2-klein-4b"})).unwrap();
    apply_preset(&mut loaded, &old);
    assert_eq!((loaded.refine_steps, loaded.refine_guidance), (None, None));
}

#[cfg(test)]
#[test]
fn ds_finding_library_snapshots_share_entries_until_removal() {
    let entry = li_ai::library::Entry {
        id: "fixture".into(),
        name: "large metadata".repeat(100),
        created_at: 0.,
        width: 1,
        height: 1,
        generation: Some(json!({"prompt":"prompt".repeat(100)})),
        reference_attributions: vec![],
        sha256: "hash".into(),
        upscale: None,
    };
    rt(|r| {
        r.library = Arc::new(vec![entry; 10_000]);
        r.library_loaded = Some(Instant::now());
    });
    let a = rt(|r| r.library.clone());
    for _ in 0..30 {
        let b = rt(|r| r.library.clone());
        assert!(Arc::ptr_eq(&a, &b));
        assert_eq!(a.as_ptr(), b.as_ptr());
    }
    rt(|r| {
        Arc::make_mut(&mut r.library).remove(0);
    });
    assert_eq!(a.len(), 10_000);
    assert_eq!(rt(|r| r.library.len()), 9_999);
    rt(|r| r.library = Arc::new(vec![]));
}

#[cfg(test)]
#[test]
fn ds2_refine_lora_preset_roundtrip_and_invalid_step_limits() {
    use egui_kittest::kittest::Queryable;
    crate::ai_ui::TEST_STATUS.with(|s| {
        *s.borrow_mut() = Some(crate::ai_ui::EngineStatus {
            connected: true,
            checked: true,
            presets: catalog::presets().iter().map(|p| (p.id(), Ok(()))).collect(),
            ..Default::default()
        })
    });
    let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), Default::default());
    app.ui.ai.generate.refine = true;
    app.ui.ai.generate.refine_loras = vec![LoraPick { name: "refine-style.safetensors".into(), strength: 0.65 }];
    let mut initialized = false;
    let mut h = egui_kittest::Harness::builder().with_size(vec2(420., 1800.)).build_ui_state(
        move |ui, app: &mut PhotocraftApp| {
            if !initialized {
                PhotocraftApp::setup_context(ui.ctx(), Default::default());
                initialized = true;
                return;
            }
            panel(app, ui);
        },
        app,
    );
    h.run_steps(4);
    let at = h.ctx.data(|d| d.get_temp::<egui::Rect>(egui::Id::new("gen-save-preset-test"))).unwrap().center();
    h.hover_at(at);
    h.step();
    h.event(egui::Event::PointerButton { pos: at, button: egui::PointerButton::Primary, pressed: true, modifiers: egui::Modifiers::NONE });
    h.step();
    h.event(egui::Event::PointerButton { pos: at, button: egui::PointerButton::Primary, pressed: false, modifiers: egui::Modifiers::NONE });
    h.step();
    h.run_steps(3);
    h.get_by_label("Save Preset").click();
    h.run_steps(3);
    let saved = li_ai::presets::all(&preset_path()).into_iter().find(|p| p.name == h.state().ui.ai.generate.preset).unwrap();
    assert_eq!(saved.refine_loras.len(), 1);
    assert_eq!(saved.refine_loras[0].name, "refine-style.safetensors");
    let mut loaded = GenerateState::default();
    apply_preset(&mut loaded, &saved);
    assert_eq!(loaded.refine_loras[0].strength, 0.65);
    let old = serde_json::from_value(json!({"name":"old"})).unwrap();
    apply_preset(&mut loaded, &old);
    assert!(loaded.refine_loras.is_empty());
    fit_to_backend(&mut loaded, Some(li_ai::comfy::SamplerLimits { steps: (20., 4.), cfg: (0., 10.), denoise: (0., 1.) }));
    assert_eq!(loaded.steps, 20);
    crate::ai_ui::TEST_STATUS.with(|s| *s.borrow_mut() = None);
}

#[cfg(test)]
#[test]
fn zimage_variety_settings_presets_and_requests_keep_old_defaults() {
    let old: GenerateState = serde_json::from_value(json!({"model":"z-image-turbo"})).unwrap();
    assert_eq!(old.variety, 0.0);
    let old_draft: DraftRefineSettings = serde_json::from_value(json!({"draft":{"model":"z-image-turbo"}})).unwrap();
    assert_eq!(old_draft.draft.variety, 0.0);
    let mut s = GenerateState { variety: 75.0, ..old };
    let restored: GenerateState = serde_json::from_value(serde_json::to_value(&s).unwrap()).unwrap();
    assert_eq!(restored.variety, 75.0);
    assert_eq!(local_request(&s, "a fox".into(), None, Vec::new()).unwrap().0.variety, 75.0);
    s.variety = f32::NAN;
    assert_eq!(local_request(&s, "a fox".into(), None, Vec::new()).unwrap().0.variety, 0.0);
    s.variety = 75.0;
    set_model(&mut s, ModelId::Ernie);
    assert_eq!(s.variety, 0.0);
    s.variety = 75.0;
    assert_eq!(local_request(&s, "a fox".into(), None, Vec::new()).unwrap().0.variety, 0.0);
    let old_preset: li_ai::presets::GenPreset = serde_json::from_value(json!({"model":"z-image-turbo"})).unwrap();
    apply_preset(&mut s, &old_preset);
    assert_eq!(s.variety, 0.0);
    let preset = li_ai::presets::GenPreset { variety: Some(75.0), ..old_preset };
    let restored: li_ai::presets::GenPreset = serde_json::from_value(serde_json::to_value(&preset).unwrap()).unwrap();
    apply_preset(&mut s, &restored);
    assert_eq!(s.variety, 75.0);
    s.mode = Mode::Refine;
    let source = RgbaImage::new(64, 64);
    assert_eq!(local_request(&s, "a fox".into(), Some((photocraft_doc::DocId(1), source, None)), Vec::new()).unwrap().0.variety, 0.0);
}

#[cfg(test)]
#[test]
fn ds2_restored_upscale_keeps_non_official_model_choice() {
    for model in [ModelId::Klein4B.key(), "custom:saved-workflow", "cloud:saved-model"] {
        rt(|r| r.upscale_any = false);
        let state = GenerateState { mode: Mode::Upscale, model: model.into(), ..Default::default() };
        restore_upscale_choice(&state);
        assert!(upscale_any());
    }
    rt(|r| r.upscale_any = false);
    restore_upscale_choice(&GenerateState { mode: Mode::Upscale, model: ModelId::SeedVr2.key().into(), ..Default::default() });
    assert!(!upscale_any());
}

#[cfg(test)]
#[test]
fn final_review_unsupported_recipe_picker_explains_and_can_clear_styles() {
    use egui_kittest::kittest::Queryable;
    let mut initialized = false;
    let mut h = egui_kittest::Harness::builder().with_size(vec2(500.0, 200.0)).build_ui_state(
        move |ui, picks: &mut Vec<LoraPick>| {
            if !initialized {
                PhotocraftApp::setup_context(ui.ctx(), crate::theme::ThemeKind::Studio);
                initialized = true;
                ui.ctx().request_repaint();
                return;
            }
            refine_recipe_loras_ui(ui, picks, None, &Tokens::get(ui.ctx()));
        },
        vec![LoraPick { name: "flux-2-klein-style.safetensors".into(), strength: 0.7 }],
    );
    h.run_steps(3);
    h.get_by_label("This refinement recipe does not support styles (LoRAs).");
    assert_eq!(h.state().len(), 1, "discovery must not silently discard a chosen style");
    h.get_by_label("Clear styles").click();
    h.run_steps(3);
    assert!(h.state().is_empty());
}

#[cfg(test)]
#[test]
fn final_review_history_persists_effective_sampling_and_replays_it_explicitly() {
    let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
    app.session.execute("file.new", json!({"width":32,"height":24})).unwrap();
    let layer = app.session.active().unwrap().active_layer.unwrap();
    let image = RgbaImage::from_pixel(32, 24, image::Rgba([35, 86, 119, 255]));
    let expected = image.clone();
    let meta =
        json!({"model":ModelId::Klein4B.key(), "variant":"bf16", "mode":"refine", "prompt":"refine", "seed":42, "steps":4, "guidance":1.0, "denoise":1.0});
    assert!(launch_resolved(&mut app, "Refinement", "Refinement", "review-effective".into(), meta, None, false, move |_| {
        Ok((image, json!({"steps":8,"guidance":2.0,"denoise":0.35,"sampler":"euler","scheduler":"simple","recipe":"template:verified","width":32,"height":24})))
    }));
    let until = Instant::now() + Duration::from_secs(5);
    let entry = loop {
        if let Some(event) = app.session.poll_jobs().into_iter().next() {
            on_generated(&mut app, &event);
            let JobOutcome::Done(value) = event.outcome else { panic!("{event:?}") };
            break serde_json::from_value::<Entry>(value).unwrap();
        }
        assert!(Instant::now() < until, "generation did not finish");
        std::thread::sleep(Duration::from_millis(5));
    };
    assert_eq!(library().load(&entry).unwrap(), expected);
    let persisted = library().list().into_iter().find(|e| e.id == entry.id).unwrap();
    let generation = persisted.generation.as_ref().unwrap();
    assert_eq!(generation["denoise"], 0.35);
    assert_eq!(generation["steps"], 8);
    assert_eq!(generation["recipe"], "template:verified");
    regenerate_layer(&mut app, &egui::Context::default(), layer.0, generation, true).unwrap();
    let state = &app.ui.ai.generate;
    assert_eq!((state.steps, state.guidance, state.denoise), (8, 2.0, 0.35));
    assert_eq!(state.refine_defaults, [false; 3]);
    assert_eq!((&*state.sampler, &*state.scheduler), ("euler", "simple"));
    let req = local_request(state, state.prompt.clone(), Some((app.session.active().unwrap().doc.id, expected, None)), vec![]).unwrap().0;
    assert_eq!(req.refine_defaults, [false; 3]);
    assert_eq!(req.denoise, 0.35);
}

#[cfg(test)]
#[test]
fn final_review_history_replay_preserves_the_resolved_second_pass() {
    let defaults = GenerateState {
        refine: true,
        refine_model: ModelId::Klein9B.key().into(),
        refine_variant: catalog::default_variant(ModelId::Klein9B).into(),
        ..Default::default()
    };
    let automatic = local_request(&defaults, "a house".into(), None, vec![]).unwrap().0.refine.unwrap();
    assert_eq!((automatic.steps, automatic.guidance), (None, None), "recipe defaults must be resolved on the worker");
    assert!(automatic.default_strength);
    let legacy: GenerateState = serde_json::from_value(json!({"steps":20,"guidance":8.0,"denoise":0.35,"refine_strength":0.35})).unwrap();
    assert_eq!(legacy.refine_defaults, [false; 3], "older saved numbers remain explicit");
    assert!(!legacy.refine_strength_default);
    let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
    app.session.execute("file.new", json!({"width":32,"height":24})).unwrap();
    let layer = app.session.active().unwrap().active_layer.unwrap();
    let meta = json!({"model":ModelId::Klein4B.key(),"mode":"create","prompt":"a house","steps":4,"guidance":1.0,"denoise":0.6,
        "refine":{"model":ModelId::Klein9B.key(),"variant":"fp8","strength":0.25,"steps":12,"guidance":5.0,"scale":1.5,"loras":[{"name":"style.safetensors","strength":0.7}]}});
    regenerate_layer(&mut app, &egui::Context::default(), layer.0, &meta, true).unwrap();
    let state = &app.ui.ai.generate;
    let req = local_request(state, state.prompt.clone(), None, vec![]).unwrap().0;
    let step = req.refine.unwrap();
    assert_eq!(step.model, ModelId::Klein9B);
    assert_eq!(step.variant, "fp8");
    assert_eq!((step.steps, step.guidance, step.strength, step.scale), (Some(12), Some(5.0), 0.25, 1.5));
    assert!(!step.default_strength);
    assert_eq!(step.loras[0].name, "style.safetensors");
    assert_eq!(step.loras[0].strength, 0.7);
}

#[cfg(test)]
#[test]
fn final_review_second_pass_typing_nominal_values_marks_them_explicit() {
    use egui_kittest::kittest::Queryable;
    let _lock = crate::ai_ui::AI_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    crate::ai_ui::TEST_STATUS.with(|s| *s.borrow_mut() = Some(crate::ai_ui::EngineStatus { checked: true, ..Default::default() }));
    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            crate::ai_ui::TEST_STATUS.with(|s| *s.borrow_mut() = None);
        }
    }
    let _reset = Reset;
    let model = ModelId::Klein9B;
    let steps = model.info().steps.default as u32;
    let guidance = model.info().guidance.default;
    let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
    app.ui.ai.generate.refine = true;
    app.ui.ai.generate.refine_model = model.key().into();
    app.ui.ai.generate.refine_variant = catalog::default_variant(model).into();
    let mut initialized = false;
    let mut h = egui_kittest::Harness::builder().with_size(vec2(620.0, 1600.0)).build_ui_state(
        move |ui, app: &mut PhotocraftApp| {
            if !initialized {
                PhotocraftApp::setup_context(ui.ctx(), crate::theme::ThemeKind::Studio);
                initialized = true;
                ui.ctx().request_repaint();
                return;
            }
            panel(app, ui);
        },
        app,
    );
    h.run_steps(3);
    assert_eq!((h.state().ui.ai.generate.refine_steps, h.state().ui.ai.generate.refine_guidance), (None, None));
    h.get_by_role_and_label(egui::accesskit::Role::SpinButton, "Refine steps").click();
    h.run_steps(2);
    h.event(egui::Event::Text(steps.to_string()));
    h.run_steps(2);
    h.get_by_role_and_label(egui::accesskit::Role::SpinButton, "Refine guidance").click();
    h.run_steps(2);
    h.event(egui::Event::Text(guidance.to_string()));
    h.run_steps(2);
    let state = &h.state().ui.ai.generate;
    assert_eq!((state.refine_steps, state.refine_guidance), (Some(steps), Some(guidance)));
    let step = local_request(state, "a house".into(), None, vec![]).unwrap().0.refine.unwrap();
    assert_eq!((step.steps, step.guidance), (Some(steps), Some(guidance)));
}

#[cfg(test)]
#[test]
fn cancelled_prepared_generation_never_enters_library() {
    let _lock = crate::ai_ui::AI_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
    let name = format!("cancelled-prepared-{}", std::process::id());
    assert!(launch_resolved(&mut app, "Generation", "Generation", name.clone(), json!({}), None, false, |_| {
        Ok((RgbaImage::from_pixel(16, 16, image::Rgba([30, 70, 90, 255])), Value::Null))
    }));
    let id = app.session.jobs().into_iter().find(|j| j.command == GENERATE_JOB).unwrap().id;
    let until = Instant::now() + Duration::from_secs(5);
    loop {
        let job = app.session.job(id).unwrap();
        if job.progress >= 0.99 {
            break;
        }
        assert!(Instant::now() < until, "generation was not prepared");
        std::thread::sleep(Duration::from_millis(2));
    }
    assert!(!library().list().iter().any(|e| e.name.starts_with(&name)), "worker published before the apply step");
    crate::ai_ui::stop_generation(&mut app, &[id]);
    app.session.join_cancelled_jobs();
    app.session.poll_jobs();
    assert!(!library().list().iter().any(|e| e.name.starts_with(&name)), "cancelled result was published");
}

#[cfg(test)]
#[test]
fn generated_edit_returns_to_the_document_the_user_is_viewing() {
    let _lock = crate::ai_ui::AI_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
    app.run("file.new", json!({"width": 8, "height": 8})).unwrap();
    let target = app.session.active().unwrap().doc.id.0;
    let layers = app.session.active().unwrap().doc.layers.len();
    app.run("file.new", json!({"width": 8, "height": 8})).unwrap();
    let viewing = app.session.active().unwrap().doc.clone();
    let revision = app.session.active().unwrap().revision;
    app.ui.tool = crate::state::Tool::Lasso;
    crate::lasso_ui::pointer(&mut app, crate::canvas::ToolEvent::Down { x: 1.0, y: 1.0, pressure: 1.0 }, egui::Modifiers::NONE);
    assert!(crate::lasso_ui::active(&app));
    let entry = library().add(&RgbaImage::from_pixel(8, 8, image::Rgba([30, 70, 90, 255])), "active-tab-regression", None, None).unwrap();
    place_into(&mut app, target, &entry);
    assert_eq!(app.session.active().unwrap().doc.id, viewing.id);
    assert!(std::sync::Arc::ptr_eq(&app.session.active().unwrap().doc, &viewing));
    assert_eq!(app.session.active().unwrap().revision, revision);
    assert_eq!(app.session.documents()[0].doc.layers.len(), layers + 1);
    assert!(crate::lasso_ui::active(&app), "background completion cancelled the viewed document's gesture");
    assert!(!app.ui.status_error, "{}", app.ui.status);
    library().delete(&[entry.id]).unwrap();
}
