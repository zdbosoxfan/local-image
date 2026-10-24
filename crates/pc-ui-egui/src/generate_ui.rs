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
#[cfg(not(target_arch = "wasm32"))]
use crate::theme::Tokens;
use crate::widgets;

pub(crate) mod draft_refine;
mod folder_batch;
mod model_picker;
mod panel_controls;
mod results;
mod run;
mod thumbs;

pub use self::folder_batch::*;
use self::model_picker::*;
pub use self::results::*;
pub use self::run::*;
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
    /// Jobs started by this panel, including the official Enhance route.
    panel_jobs: Vec<photocraft_engine::jobs::JobId>,
    #[cfg(test)]
    capture_panel_start: bool,
    #[cfg(test)]
    panel_start_clicks: usize,
    gallery_thumbs: thumbs::Thumbs,
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
    /// Cloud images started this session, counted against `ai.cloud_session_cap_images`.
    cloud_images: u32,
    /// "Don't ask again this session" was ticked on the cloud confirmation.
    cloud_skip_confirm: bool,
    /// A cloud run waiting for the user's go-ahead.
    cloud_pending: Option<CloudRun>,
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
        Err(li_i18n::fmt(tl!("Add your {provider} API key in Local AI › Cloud."), &[("provider", m.provider.label())]))
    }
}

/// The Generate tab.
pub fn panel(app: &mut PhotocraftApp, ui: &mut egui::Ui) {
    let layout = panel_controls::layout(ui);
    if layout.bounds.height() < 124.0 {
        panel_controls::show_compact(app, ui, &layout);
        return;
    }
    let mut settings_ui =
        ui.new_child(egui::UiBuilder::new().id_salt("generate-settings-area").max_rect(layout.settings).layout(egui::Layout::top_down(egui::Align::Min)));
    settings_ui.set_clip_rect(ui.clip_rect().intersect(layout.settings));
    let action = egui::ScrollArea::vertical()
        .id_salt("generate-settings-scroll")
        .max_height(layout.settings.height())
        .min_scrolled_height(0.0)
        .auto_shrink([false, false])
        .show(&mut settings_ui, |ui| settings_ui_body(app, ui))
        .inner;
    panel_controls::show(app, ui, &layout, action);
}

/// Settings have one rendering path, either in their viewport or the squeezed-dock popup.
fn settings_ui_body(app: &mut PhotocraftApp, ui: &mut egui::Ui) -> panel_controls::Action {
    let t = Tokens::get(ui.ctx());
    let mut submit = false;
    (|ui: &mut egui::Ui| {
        let draft = widgets::secondary_button(ui, tl!("Draft / Refinement…"), ui.available_width());
        panel_controls::note(ui, "settings-draft", &draft);
        if draft.clicked() {
            draft_refine::open(app);
        }
        ui.set_width(ui.available_width());
        ui.spacing_mut().item_spacing.y = 6.0;
        let has_doc = app.session.active().is_some();
        let has_sel = app.session.active().is_some_and(|d| d.doc.selection.is_some());
        let st = crate::ai_ui::status();
        let s = &mut app.ui.ai.generate;
        // Mode.
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
            if widgets::secondary_button(ui, tl!("Choose another model…"), ui.available_width()).clicked() {
                rt(|r| r.upscale_any = true);
            }
            return;
        }
        // Preset.
        let all_presets = presets();
        ui.horizontal(|ui| {
            label(ui, tl!("Preset"));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let save = crate::icons::button(ui, "bookmark-plus", 22.0, false, tl!("Save these settings as a preset"));
                #[cfg(test)]
                ui.ctx().data_mut(|d| d.insert_temp(egui::Id::new("gen-save-preset-test"), save.rect));
                if save.clicked() {
                    rt(|r| r.preset_name = li_i18n::fmt(tl!("My preset {n}"), &[("n", &(all_presets.iter().filter(|p| !p.built_in).count() + 1).to_string())]));
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
        // Model.
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
                    RichText::new(li_i18n::fmt(
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
        // Prompt.
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
        // Size (Create): shared aspect presets and linked model-grid dimensions.
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
                RichText::new(li_i18n::fmt(tl!("Sent as {size}"), &[("size", &li_ai::cloud::describe_size(c, s.width, s.height))]))
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
        // Strength.
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
        // References.
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
        // LoRAs.
        if custom.is_none() && cloud.is_none() && (info.lora || s.mode == Mode::Refine) {
            if s.mode == Mode::Refine {
                refine_loras_ui(ui, &mut s.loras, info.id, &s.variant, &t);
            } else {
                loras_ui(ui, &mut s.loras, &info.resolved, &t);
            }
        }
        // Draft → Refine.
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
        // Advanced.
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
                    label(ui, li_i18n::tr_ctx(li_i18n::current(), "ai", "Sampler"));
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
        submit = enter;
    })(ui);
    let mut action = panel_controls::action(app);
    action.enter = submit;
    let st = crate::ai_ui::status();
    for why in &action.warnings {
        readiness_warning(app, ui, why, &st, action.cloud);
    }
    action
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
        label(ui, &li_i18n::fmt(tl!("Styles (LoRA) · {n}/{max}"), &[("n", &picks.len().to_string()), ("max", &max.to_string())]));
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
                    (l.name.clone(), if *sure { name } else { li_i18n::fmt(tl!("{name} (family unknown)"), &[("name", &name)]) })
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

/// Opens the Generate panel on the model `key` (Set Up Models › Use in Generate).
pub(crate) fn use_model(app: &mut PhotocraftApp, ctx: &egui::Context, key: &str) {
    let _ = crate::menus::invoke(app, ctx, "li.panel.generate", json!({}));
    if let Some(m) = ModelId::from_key(key).filter(|m| m.try_info().is_some()) {
        let s = &mut app.ui.ai.generate;
        if !fits_mode(m.info(), s.mode) {
            s.mode = Mode::Create;
        }
        set_model(s, m);
    }
}

/// Generate › Import Workflow…: a ComfyUI workflow (API or editor format) with `li:` titles.
pub fn import_workflow(app: &mut PhotocraftApp) {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let request = crate::FileDialogSpec::new(crate::Pick::File).title(tl!("Import ComfyUI Workflow")).filter(tl!("ComfyUI workflow"), &["json"]);
        let r = app.ask_file(crate::FileDialogRequest::Described(request), |app, answer| {
            let Some(path) = answer.into_path() else { return Ok(Value::Null) };
            let name = import_workflow_path(std::path::Path::new(&path))?;
            app.ui.ai.generate.model = format!("custom:{name}");
            app.ui.status = li_i18n::fmt(tl!("Imported the workflow “{name}”."), &[("name", &name)]);
            app.ui.status_error = false;
            Ok(Value::Null)
        });
        if let Err(e) = r
            && e != crate::file_dialog::CANCELLED
        {
            app.ui.status = e;
            app.ui.status_error = true;
        }
    }
    #[cfg(target_arch = "wasm32")]
    let _ = app;
}

/// Imports a workflow file; returns its name.
pub fn import_workflow_path(path: &std::path::Path) -> Result<String, String> {
    let bytes = photocraft_engine::batch_io::read_file(path)?;
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
        label(ui, &li_i18n::fmt(heading, &[("n", &n.to_string()), ("max", &max.to_string())]));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let full = n >= max;
            ui.add_enabled_ui(!full, |ui| {
                #[cfg(not(target_arch = "wasm32"))]
                if crate::icons::button(ui, "folder-open", 22.0, false, tl!("Add an image file")).clicked() {
                    let request = crate::FileDialogSpec::new(crate::Pick::File).filter(tl!("Images"), &photocraft_codecs::readable_extensions());
                    let ctx = ui.ctx().clone();
                    crate::file_dialog::ask_later(ui.ctx(), crate::FileDialogRequest::Described(request), move |_, answer| {
                        let Some(path) = answer.into_path().map(PathBuf::from) else { return Ok(Value::Null) };
                        let img = photocraft_engine::ai_cmds::file_rgba_srgb(&path).map_err(|e| e.to_string())?;
                        let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                        add_reference(&ctx, name, img);
                        Ok(Value::Null)
                    });
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
                if resp.on_hover_text(li_i18n::fmt(tl!("{name} — click to remove"), &[("name", &rf.name)])).clicked() {
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

#[cfg(test)]
mod tests;
