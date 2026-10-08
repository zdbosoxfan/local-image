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
    fn tip(self) -> &'static str {
        match self {
            Mode::Create => "A new image from a description and optional reference images",
            Mode::Edit => "Change the open image from an instruction (edit models), or restyle it",
            Mode::Fill => "Regenerate the selection (Photoshop's Generative Fill)",
            Mode::Refine => "Resample the open image at a chosen strength (img2img)",
            Mode::Upscale => "Enlarge the open image and add detail, tile by tile",
        }
    }
    fn default_strength(self) -> f32 {
        match self {
            Mode::Fill => 1.0,
            Mode::Upscale => 0.3,
            Mode::Edit => 0.5,
            _ => 0.35,
        }
    }
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
    pub prompt: String,
    pub negative: String,
    /// A model key (`flux2-klein-4b`, `ckpt:…`) or `custom:<workflow name>`.
    pub model: String,
    pub variant: String,
    /// `1:1`, `4:3`, `3:2`, `16:9`, `9:16`, `2:3`, `3:4`, `custom`.
    pub aspect: String,
    pub width: u32,
    pub height: u32,
    pub transparent: bool,
    pub count: u32,
    pub steps: u32,
    pub guidance: f32,
    /// Fixed seed (🔒), else a new one each time.
    pub seed: Option<u64>,
    /// Strength for Edit (non-edit models), Fill, Refine and Upscale.
    pub denoise: f32,
    pub advanced: bool,
    pub library_filter: String,
    /// The selected preset's name.
    pub preset: String,
    pub loras: Vec<LoraPick>,
    /// Draft → Refine.
    pub refine: bool,
    pub refine_model: String,
    pub refine_variant: String,
    pub refine_strength: f32,
    pub refine_scale: f32,
    pub upscale: f32,
    /// Sampler / scheduler instead of the family's (empty = the family's).
    pub sampler: String,
    pub scheduler: String,
}

impl Default for GenerateState {
    fn default() -> Self {
        let m = ModelId::Klein4B;
        Self {
            mode: Mode::Create,
            prompt: String::new(),
            negative: String::new(),
            model: m.key().into(),
            variant: catalog::default_variant(m).into(),
            aspect: "1:1".into(),
            width: 1024,
            height: 1024,
            transparent: false,
            count: 1,
            steps: m.info().steps.default as u32,
            guidance: m.info().guidance.default,
            seed: None,
            denoise: 0.35,
            advanced: false,
            library_filter: String::new(),
            preset: "None".into(),
            loras: Vec::new(),
            refine: false,
            refine_model: String::new(),
            refine_variant: String::new(),
            refine_strength: 0.35,
            refine_scale: 1.0,
            upscale: 2.0,
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
    library: Vec<Entry>,
    library_loaded: Option<Instant>,
    selected: Option<String>,
    focus_prompt: bool,
    /// Imported custom workflows (loaded on first use and after an import).
    customs: Option<Vec<li_ai::custom::CustomWorkflow>>,
    presets: Option<Vec<li_ai::presets::GenPreset>>,
    preset_name: String,
}

struct Reference {
    name: String,
    image: Arc<RgbaImage>,
    texture: Option<TextureHandle>,
}

enum Tile {
    /// `edit`: the document the result lands on as a layer; `open`: open it as a new document.
    Pending { job: u64, label: String, edit: Option<u64>, open: bool },
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

fn customs() -> Vec<li_ai::custom::CustomWorkflow> {
    rt(|r| r.customs.get_or_insert_with(|| li_ai::custom::list(&li_ai::custom::dir())).clone())
}

fn presets() -> Vec<li_ai::presets::GenPreset> {
    rt(|r| r.presets.get_or_insert_with(|| li_ai::presets::all(&li_ai::presets::path())).clone())
}

fn custom_of(s: &GenerateState) -> Option<li_ai::custom::CustomWorkflow> {
    let name = s.model.strip_prefix("custom:")?;
    customs().into_iter().find(|w| w.name == name)
}

fn model_of(s: &GenerateState) -> ModelId {
    ModelId::from_key(&s.model).filter(|m| m.try_info().is_some()).unwrap_or(ModelId::Klein4B)
}

fn set_model(s: &mut GenerateState, m: ModelId) {
    let info = m.info();
    s.model = m.key().into();
    s.variant = catalog::default_variant(m).into();
    s.steps = info.steps.default as u32;
    s.guidance = info.guidance.default;
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
    }
}

/// Whether a model can run `mode`.
fn fits_mode(info: &catalog::ModelInfo, mode: Mode) -> bool {
    if info.tool || info.resolved.kind != li_ai::family::FamilyKind::Image {
        return false;
    }
    match mode {
        Mode::Create => info.text_to_image,
        Mode::Fill => info.inpaint,
        Mode::Edit | Mode::Refine | Mode::Upscale => true,
    }
}

fn size_for(aspect: &str, s: &GenerateState, native: u32) -> (u32, u32) {
    let Some((_, r)) = ASPECTS.iter().find(|(k, _)| *k == aspect) else { return (s.width, s.height) };
    // About the model's native pixel count, snapped to a 16 px grid.
    let area = native as f32 * native as f32;
    let h = (area / r).sqrt();
    li_ai::imaging::snap_size((h * r) as u32, h as u32, 16)
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

/// Readiness of a model on the running engine: `Ok` ready, `Err(reason)` otherwise.
fn readiness(st: &crate::ai_ui::EngineStatus, m: ModelId, variant: &str) -> Result<(), String> {
    st.ready(m, variant)
}

/// The model picker: models grouped by family group, each with a readiness dot and its capability
/// tags; custom workflows last; Browse Models… and Import Workflow… at the bottom. Returns the
/// chosen key.
fn model_picker(ui: &mut egui::Ui, id: &str, current: &str, mode: Mode, st: &crate::ai_ui::EngineStatus, allow_custom: bool) -> Option<String> {
    let t = Tokens::get(ui.ctx());
    let custom = current.strip_prefix("custom:");
    let current_label = match custom {
        Some(name) => format!("{name} · Custom workflow"),
        None => ModelId::from_key(current).and_then(|m| m.try_info()).map(|i| format!("{} · {}", i.label, i.family_label)).unwrap_or_else(|| "Choose a model".into()),
    };
    let mut picked = None;
    let width = ui.available_width();
    egui::ComboBox::from_id_salt(id).selected_text(current_label).width(width).height(460.0).icon(widgets::chevron_icon).show_ui(ui, |ui| {
        ui.set_min_width(width.max(300.0));
        let cat = catalog::catalog();
        let mut groups: Vec<(&str, Vec<&catalog::ModelInfo>)> = Vec::new();
        for m in cat.models.iter().filter(|m| fits_mode(m, mode)) {
            match groups.iter_mut().find(|(g, _)| *g == m.group) {
                Some((_, v)) => v.push(m),
                None => groups.push((m.group.as_str(), vec![m])),
            }
        }
        for (g, models) in groups {
            ui.add_space(4.0);
            ui.label(RichText::new(g.to_uppercase()).size(10.5).color(t.text_faint).strong());
            for m in models {
                let variant = catalog::default_variant(m.id);
                let ok = catalog::presets_for(m.id).any(|p| readiness(st, m.id, &p.variant).is_ok());
                let reason = readiness(st, m.id, variant).err().unwrap_or_default();
                let row = ui.horizontal(|ui| {
                    let (dot, _) = ui.allocate_exact_size(vec2(10.0, 16.0), Sense::hover());
                    let colour = if ok {
                        Color32::from_rgb(70, 190, 110)
                    } else if m.origin == catalog::Origin::Installed {
                        t.warning
                    } else {
                        t.text_faint
                    };
                    ui.painter().circle_filled(dot.center(), 3.5, colour);
                    let sel = current == m.id.key();
                    let resp = ui.add(egui::Button::selectable(sel, RichText::new(&m.label).color(if ok || !st.connected { t.text } else { t.text_dim })));
                    ui.label(RichText::new(&m.family_label).size(10.5).color(t.text_faint));
                    for tg in m.tags().iter().take(4) {
                        tag(ui, tg, &t);
                    }
                    resp
                });
                let tip = if ok {
                    format!("{}\n{}", m.best_for, m.tags().join(" · "))
                } else if m.origin == catalog::Origin::Profile {
                    format!("{}\nNot installed: {}", m.best_for, reason)
                } else {
                    format!("{}\n{}", m.best_for, reason)
                };
                if row.inner.on_hover_text(tip).clicked() {
                    picked = Some(m.id.key().to_owned());
                }
            }
        }
        if allow_custom {
            let cws = customs();
            if !cws.is_empty() {
                ui.add_space(4.0);
                ui.label(RichText::new("CUSTOM WORKFLOWS").size(10.5).color(t.text_faint).strong());
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
        ui.horizontal(|ui| {
            if ui.button("Browse Models…").clicked() {
                picked = Some("browse:".into());
            }
            if allow_custom && ui.button("Import Workflow…").clicked() {
                picked = Some("import:".into());
            }
        });
    });
    picked
}

/// The Generate tab.
pub fn panel(app: &mut PhotocraftApp, ui: &mut egui::Ui) {
    let t = Tokens::get(ui.ctx());
    let ctx = ui.ctx().clone();
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.spacing_mut().item_spacing.y = 6.0;
        let has_doc = app.session.active().is_some();
        let has_sel = app.session.active().is_some_and(|d| d.doc.selection.is_some());
        let st = crate::ai_ui::status();
        let s = &mut app.ui.ai.generate;
        // ---- Mode.
        let labels: Vec<&str> = Mode::ALL.iter().map(|m| m.label()).collect();
        let tips: Vec<&str> = Mode::ALL.iter().map(|m| m.tip()).collect();
        let disabled: Vec<Option<&str>> = Mode::ALL
            .iter()
            .map(|m| match m {
                Mode::Create => None,
                Mode::Fill if !has_sel => Some("Make a selection first"),
                _ if !has_doc => Some("Open an image first"),
                _ => None,
            })
            .collect();
        let cur = Mode::ALL.iter().position(|m| *m == s.mode).unwrap_or(0);
        if let Some(i) = chips_with(ui, &labels, cur, &tips, &disabled) {
            s.mode = Mode::ALL[i];
            s.denoise = s.mode.default_strength();
            // A model that can't run the new mode gives way to one that can.
            let m = model_of(s);
            if !s.model.starts_with("custom:") && !fits_mode(m.info(), s.mode) {
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
        // ---- Preset.
        let all_presets = presets();
        ui.horizontal(|ui| {
            label(ui, "Preset");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let save = crate::icons::button(ui, "bookmark-plus", 22.0, false, "Save these settings as a preset");
                if save.clicked() {
                    rt(|r| r.preset_name = format!("My preset {}", all_presets.iter().filter(|p| !p.built_in).count() + 1));
                }
                egui::Popup::from_toggle_button_response(&save).close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside).show(|ui| {
                    ui.set_min_width(220.0);
                    ui.label("Preset name");
                    let mut name = rt(|r| r.preset_name.clone());
                    ui.text_edit_singleline(&mut name);
                    rt(|r| r.preset_name = name.clone());
                    if widgets::primary_button(ui, "Save Preset", 0.0).clicked() && !name.trim().is_empty() {
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
        // ---- Model.
        if let Some(k) = model_picker(ui, "gen-model", &s.model.clone(), s.mode, &st, true) {
            match k.as_str() {
                "browse:" => crate::model_browser::open(),
                "import:" => import_workflow(app),
                k if k.starts_with("custom:") => app.ui.ai.generate.model = k.to_owned(),
                k => {
                    if let Some(m) = ModelId::from_key(k) {
                        set_model(&mut app.ui.ai.generate, m);
                    }
                }
            }
        }
        let s = &mut app.ui.ai.generate;
        let custom = custom_of(s);
        let m = model_of(s);
        let info = m.info();
        if custom.is_none() {
            let presets_m: Vec<_> = catalog::presets_for(m).collect();
            if presets_m.len() > 1 {
                ui.horizontal(|ui| {
                    label(ui, "Precision");
                    let opts: Vec<(String, &str)> = presets_m.iter().map(|p| (p.variant.clone(), p.label.as_str())).collect();
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        widgets::dropdown(ui, "gen-variant", &mut s.variant, &opts, ui.available_width().min(200.0));
                    });
                });
            }
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                ui.label(RichText::new(&info.best_for).color(t.text_faint).size(11.0));
            });
        }
        // ---- Prompt.
        let wants_prompt = custom.as_ref().is_none_or(|w| w.has(&li_ai::custom::FieldKind::Prompt));
        let mut enter = false;
        if wants_prompt {
            let hint = match s.mode {
                Mode::Create => "Describe an image to create…",
                Mode::Edit if info.edit => "Describe the change, e.g. “make it golden hour”…",
                Mode::Edit => "Describe the image you want it to become…",
                Mode::Fill => "What should appear in the selection…",
                Mode::Refine => "Describe the image (optional)…",
                Mode::Upscale => "Describe details to add (optional)…",
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
        let wants_negative = match &custom {
            Some(w) => w.has(&li_ai::custom::FieldKind::Negative),
            None => info.negative_prompt,
        };
        if wants_negative {
            ui.add(egui::TextEdit::singleline(&mut s.negative).hint_text("Avoid… (negative prompt)").desired_width(f32::INFINITY));
        }
        // ---- Size (Create).
        let custom_size = custom.as_ref().is_some_and(|w| w.has(&li_ai::custom::FieldKind::Width));
        if (s.mode == Mode::Create && custom.is_none()) || custom_size {
            let items: Vec<&str> = ASPECTS.iter().map(|(k, _)| *k).chain(["Custom"]).collect();
            let sel = ASPECTS.iter().position(|(k, _)| *k == s.aspect).unwrap_or(ASPECTS.len());
            if let Some(i) = chips(ui, &items, sel) {
                if i < ASPECTS.len() {
                    s.aspect = ASPECTS[i].0.into();
                    let (w, h) = size_for(&s.aspect, s, info.resolved.sizes.native);
                    s.width = w;
                    s.height = h;
                } else {
                    s.aspect = "custom".into();
                }
            }
            ui.horizontal(|ui| {
                let (mut w, mut h) = (s.width as f32, s.height as f32);
                let max = info.resolved.sizes.max.max(1024) as f32 * 2.0;
                let cw = widgets::value_field(ui, &mut w, 256.0..=max, "px", 84.0);
                ui.label(RichText::new("×").color(t.text_faint));
                let ch = widgets::value_field(ui, &mut h, 256.0..=max, "px", 84.0);
                if cw.changed() || ch.changed() {
                    s.aspect = "custom".into();
                }
                if cw.lost_focus() || ch.lost_focus() || !(cw.has_focus() || ch.has_focus()) {
                    let (sw, sh) = li_ai::imaging::snap_size(w as u32, h as u32, info.resolved.sizes.multiple.max(8));
                    s.width = sw;
                    s.height = sh;
                } else {
                    s.width = w as u32;
                    s.height = h as u32;
                }
            });
        }
        if s.mode == Mode::Create {
            ui.horizontal(|ui| {
                if info.transparent && custom.is_none() {
                    widgets::checkbox(ui, &mut s.transparent, "Transparent");
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let mut n = s.count as f32;
                    if widgets::value_field(ui, &mut n, 1.0..=4.0, "", 40.0).changed() {
                        s.count = (n.round() as u32).clamp(1, 4);
                    }
                    label(ui, "Images");
                });
            });
        }
        // ---- Strength.
        let strength_shown = match &custom {
            Some(w) => w.has(&li_ai::custom::FieldKind::Denoise),
            None => matches!(s.mode, Mode::Refine | Mode::Upscale) || (s.mode == Mode::Edit && !info.edit) || (s.mode == Mode::Fill && info.resolved.pipeline.inpaint != li_ai::family::InpaintMethod::Instruction),
        };
        if strength_shown {
            let mut pct = s.denoise * 100.0;
            if widgets::slider_row(ui, "Strength", &mut pct, 5.0..=100.0, "%", None).on_hover_text("How much may change: 100 % regenerates, lower keeps more of the image").changed() {
                s.denoise = (pct / 100.0).clamp(0.05, 1.0);
            }
        }
        if s.mode == Mode::Upscale {
            let scales = [("1.5×", 1.5f32), ("2×", 2.0), ("3×", 3.0), ("4×", 4.0)];
            let items: Vec<&str> = scales.iter().map(|(l, _)| *l).collect();
            let sel = scales.iter().position(|(_, v)| (*v - s.upscale).abs() < 0.01).unwrap_or(1);
            ui.horizontal(|ui| {
                label(ui, "Enlarge");
                if let Some(i) = chips(ui, &items, sel) {
                    s.upscale = scales[i].1;
                }
            });
        }
        // ---- References.
        let ref_slots = match &custom {
            Some(w) => w.images(),
            None => {
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
            references_ui(app, ui, ref_slots, info.init_image && custom.is_none());
        }
        let s = &mut app.ui.ai.generate;
        // ---- LoRAs.
        if custom.is_none() && info.lora {
            loras_ui(ui, s, &info.resolved, &t);
        }
        // ---- Draft → Refine.
        if s.mode == Mode::Create && custom.is_none() {
            ui.horizontal(|ui| {
                widgets::checkbox(ui, &mut s.refine, "Refine with")
                    .on_hover_text("Draft → Refine: generate with this model, then resample the draft with another (e.g. a fast draft refined by a detailed model)");
            });
            if s.refine {
                if let Some(k) = model_picker(ui, "gen-refine-model", &s.refine_model.clone(), Mode::Refine, &st, false) {
                    if k == "browse:" {
                        crate::model_browser::open();
                    } else if let Some(m) = ModelId::from_key(&k) {
                        s.refine_model = k;
                        s.refine_variant = catalog::default_variant(m).into();
                    }
                }
                let mut pct = s.refine_strength * 100.0;
                if widgets::slider_row(ui, "Refine strength", &mut pct, 5.0..=100.0, "%", None).changed() {
                    s.refine_strength = pct / 100.0;
                }
                let scales = [("1×", 1.0f32), ("1.5×", 1.5), ("2×", 2.0)];
                let items: Vec<&str> = scales.iter().map(|(l, _)| *l).collect();
                let sel = scales.iter().position(|(_, v)| (*v - s.refine_scale).abs() < 0.01).unwrap_or(0);
                ui.horizontal(|ui| {
                    label(ui, "Enlarge first");
                    if let Some(i) = chips(ui, &items, sel) {
                        s.refine_scale = scales[i].1;
                    }
                });
            }
        }
        // ---- Advanced.
        let resp = egui::CollapsingHeader::new(RichText::new("Advanced").color(t.text_dim)).default_open(s.advanced).show(ui, |ui| {
            let (steps_range, cfg_range) = (info.steps, info.guidance);
            let show_steps = custom.as_ref().map_or(!steps_range.fixed(), |w| w.has(&li_ai::custom::FieldKind::Steps));
            if show_steps {
                let mut v = s.steps as f32;
                let r = if custom.is_some() { 1.0..=150.0 } else { steps_range.min..=steps_range.max };
                if widgets::slider_row(ui, "Steps", &mut v, r, "", None).changed() {
                    s.steps = v.round() as u32;
                }
            }
            let show_cfg = custom.as_ref().map_or(!cfg_range.fixed(), |w| w.has(&li_ai::custom::FieldKind::Cfg));
            if show_cfg {
                let r = if custom.is_some() { 1.0..=30.0 } else { cfg_range.min..=cfg_range.max };
                widgets::slider_row(ui, "Guidance", &mut s.guidance, r, "", None);
            }
            if info.init_image && custom.is_none() && s.mode == Mode::Create {
                let mut pct = s.denoise * 100.0;
                if widgets::slider_row(ui, "Variation strength", &mut pct, 5.0..=100.0, "%", None).changed() {
                    s.denoise = pct / 100.0;
                }
            }
            if custom.is_none() && info.resolved.pipeline.sampler == li_ai::family::SamplerStyle::Ksampler {
                ui.horizontal(|ui| {
                    label(ui, "Sampler");
                    let samplers = ["", "euler", "euler_ancestral", "dpmpp_2m", "dpmpp_2m_sde", "dpmpp_sde", "uni_pc", "res_multistep", "lcm"];
                    let opts: Vec<(String, &str)> = samplers.iter().map(|x| (x.to_string(), if x.is_empty() { "Model default" } else { *x })).collect();
                    widgets::dropdown(ui, "gen-sampler", &mut s.sampler, &opts, 130.0);
                    let schedulers = ["", "simple", "normal", "karras", "sgm_uniform", "beta", "exponential"];
                    let opts: Vec<(String, &str)> = schedulers.iter().map(|x| (x.to_string(), if x.is_empty() { "Default" } else { *x })).collect();
                    widgets::dropdown(ui, "gen-scheduler", &mut s.scheduler, &opts, 100.0);
                });
            }
            ui.horizontal(|ui| {
                label(ui, "Seed");
                let mut locked = s.seed.is_some();
                if widgets::checkbox(ui, &mut locked, "Keep").changed() {
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
                    ui.label(RichText::new("new each time").color(t.text_faint));
                }
            });
        });
        s.advanced = resp.fully_open();
        // ---- Readiness and the button.
        let variant = s.variant.clone();
        let ready = match &custom {
            Some(_) if st.connected => Ok(()),
            Some(_) => Err("The AI engine (ComfyUI) is not running.".to_owned()),
            None => st.ready(m, &variant),
        };
        let refine_ready = if s.refine && s.mode == Mode::Create && custom.is_none() {
            match ModelId::from_key(&s.refine_model) {
                Some(rm) => st.ready(rm, &s.refine_variant).map_err(|e| format!("Refine model: {e}")),
                None => Err("Choose a model to refine with.".into()),
            }
        } else {
            Ok(())
        };
        let needs_prompt = wants_prompt && matches!(s.mode, Mode::Create | Mode::Edit | Mode::Fill);
        let blocker = match s.mode {
            _ if needs_prompt && s.prompt.trim().is_empty() => Some("Describe what you want first."),
            Mode::Edit | Mode::Refine | Mode::Upscale if !has_doc => Some("Open an image first."),
            Mode::Fill if !has_sel => Some("Make a selection to fill."),
            _ => None,
        };
        ui.add_space(4.0);
        for why in [ready.as_ref().err(), refine_ready.as_ref().err()].into_iter().flatten() {
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new(why).color(t.warning).size(11.5));
                if !st.connected {
                    if widgets::secondary_button(ui, "Set up AI…", 0.0).clicked() {
                        crate::ai_ui::open_local_ai(app);
                    }
                } else if widgets::secondary_button(ui, "Get models…", 0.0).clicked() {
                    crate::model_browser::open();
                }
            });
        }
        let s = &app.ui.ai.generate;
        let label_text = match s.mode {
            Mode::Create if s.count > 1 => format!("Generate {}", s.count),
            Mode::Create => "Generate".into(),
            Mode::Edit => "Apply Edit".into(),
            Mode::Fill => "Fill Selection".into(),
            Mode::Refine => "Refine".into(),
            Mode::Upscale => format!("Upscale {}×", widgets::fmt_num(s.upscale as f64)),
        };
        let can = blocker.is_none() && ready.is_ok() && refine_ready.is_ok();
        let clicked = ui
            .add_enabled_ui(can, |ui| {
                let w = ui.available_width();
                widgets::primary_button(ui, &label_text, w)
            })
            .inner
            .on_disabled_hover_text(blocker.unwrap_or("The model isn't ready."))
            .clicked();
        if (clicked || enter) && can {
            start(app, &ctx);
        }
        ui.add_space(8.0);
        results_ui(app, ui);
    });
}

fn loras_ui(ui: &mut egui::Ui, s: &mut GenerateState, fam: &li_ai::family::Family, t: &Tokens) {
    let available = li_ai::inventory::loras_for(fam);
    let max = fam.lora.max.max(1) as usize;
    ui.horizontal(|ui| {
        label(ui, &format!("Styles (LoRA) · {}/{max}", s.loras.len()));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let free: Vec<&(li_ai::inventory::InstalledLora, bool)> = available.iter().filter(|(l, _)| !s.loras.iter().any(|p| p.name == l.name)).collect();
            let can = s.loras.len() < max && !free.is_empty();
            let tip = if available.is_empty() { "No LoRAs for this model family are installed (Browse Models… › LoRAs)" } else { "Add a LoRA" };
            if ui.add_enabled(can, egui::Button::new("+ Add")).on_hover_text(tip).on_disabled_hover_text(tip).clicked()
                && let Some((l, _)) = free.first()
            {
                s.loras.push(LoraPick { name: l.name.clone(), strength: 0.8 });
            }
        });
    });
    let mut remove = None;
    for (i, pick) in s.loras.iter_mut().enumerate() {
        ui.horizontal(|ui| {
            let opts: Vec<(String, String)> = available
                .iter()
                .map(|(l, sure)| (l.name.clone(), format!("{}{}", li_ai::inventory::label_for(&l.name), if *sure { "" } else { " (family unknown)" })))
                .collect();
            let refs: Vec<(String, &str)> = opts.iter().map(|(k, l)| (k.clone(), l.as_str())).collect();
            widgets::dropdown(ui, &format!("gen-lora-{i}"), &mut pick.name, &refs, (ui.available_width() - 110.0).max(80.0));
            widgets::value_field(ui, &mut pick.strength, -2.0..=2.0, "", 52.0).on_hover_text("Strength");
            if crate::icons::button(ui, "x", 20.0, false, "Remove").clicked() {
                remove = Some(i);
            }
        });
    }
    if let Some(i) = remove {
        s.loras.remove(i);
    }
    if s.loras.is_empty() && available.is_empty() {
        ui.label(RichText::new("No compatible LoRAs installed.").color(t.text_faint).size(11.0));
    }
}

fn apply_preset(s: &mut GenerateState, p: &li_ai::presets::GenPreset) {
    s.preset = p.name.clone();
    let model = p.model.as_deref().and_then(ModelId::from_key).or_else(|| {
        let fam = p.family.as_deref()?;
        let st = crate::ai_ui::status();
        let all = ModelId::all();
        all.iter().copied().find(|m| m.info().family == fam && catalog::presets_for(*m).any(|x| st.ready(*m, &x.variant).is_ok())).or_else(|| all.into_iter().find(|m| m.info().family == fam))
    });
    if let Some(m) = model {
        set_model(s, m);
    }
    if !p.negative.is_empty() {
        s.negative = p.negative.clone();
    }
    if let Some(v) = p.steps {
        s.steps = v;
    }
    if let Some(v) = p.cfg {
        s.guidance = v;
    }
    if let Some((a, b)) = &p.sampler {
        s.sampler = a.clone();
        s.scheduler = b.clone();
    }
    if !p.loras.is_empty() {
        s.loras = p.loras.iter().map(|l| LoraPick { name: l.name.clone(), strength: l.strength }).collect();
    }
    if let Some(rm) = &p.refine_model {
        s.refine = true;
        s.refine_model = rm.clone();
        s.refine_variant = ModelId::from_key(rm).map(|m| catalog::default_variant(m).to_owned()).unwrap_or_default();
        s.refine_strength = p.refine_strength.unwrap_or(0.35);
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
        sampler: (!s.sampler.is_empty()).then(|| (s.sampler.clone(), if s.scheduler.is_empty() { "simple".into() } else { s.scheduler.clone() })),
        loras: s.loras.iter().map(|l| li_ai::presets::PresetLora { name: l.name.clone(), strength: l.strength }).collect(),
        refine_model: s.refine.then(|| s.refine_model.clone()),
        refine_strength: s.refine.then_some(s.refine_strength),
        built_in: false,
    };
    let mut all = presets();
    all.retain(|p| p.name != preset.name || p.built_in);
    all.push(preset);
    let _ = li_ai::presets::save_user(&li_ai::presets::path(), &all);
    rt(|r| r.presets = None);
    s.preset = name.to_owned();
}

/// Generate › Import Workflow…: a ComfyUI workflow (API or editor format) with `li:` titles.
pub fn import_workflow(app: &mut PhotocraftApp) {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let Some(path) = rfd::FileDialog::new().set_title("Import ComfyUI Workflow").add_filter("ComfyUI workflow", &["json"]).pick_file() else { return };
        match import_workflow_path(&path) {
            Ok(name) => {
                app.ui.ai.generate.model = format!("custom:{name}");
                app.ui.status = format!("Imported the workflow “{name}”.");
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
    let json: Value = serde_json::from_slice(&bytes).map_err(|_| "That file is not a ComfyUI workflow (JSON).".to_owned())?;
    let name = path.file_stem().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "Workflow".into());
    let info = li_ai::service().client.object_info().ok();
    let w = li_ai::custom::import(&name, &json, info.as_ref()).map_err(|e| format!("{e:#}"))?;
    if w.fields.is_empty() {
        return Err("The workflow has no nodes titled li:prompt, li:image, li:mask, li:seed or li:output, so Local Image can't fill it in. Rename the nodes in ComfyUI and export again.".into());
    }
    li_ai::custom::save(&w, &li_ai::custom::dir()).map_err(|e| format!("{e:#}"))?;
    rt(|r| r.customs = None);
    Ok(w.name)
}

fn references_ui(app: &mut PhotocraftApp, ui: &mut egui::Ui, max: usize, init_image: bool) {
    let t = Tokens::get(ui.ctx());
    let n = rt(|r| r.references.len());
    ui.horizontal(|ui| {
        label(ui, &format!("{} · {n}/{max}", if init_image { "Starting image" } else { "References" }));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let full = n >= max;
            ui.add_enabled_ui(!full, |ui| {
                #[cfg(not(target_arch = "wasm32"))]
                if crate::icons::button(ui, "folder-open", 22.0, false, "Add an image file").clicked()
                    && let Some(path) = rfd::FileDialog::new().add_filter("Images", &["png", "jpg", "jpeg", "webp", "tif", "tiff"]).pick_file()
                    && let Ok(img) = image::open(&path)
                {
                    let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                    add_reference(ui.ctx(), name, img.to_rgba8());
                }
                if app.session.active().is_some() && crate::icons::button(ui, "image", 22.0, false, "Add the current image").clicked() {
                    let d = app.session.active().map(|d| (d.doc.name.clone(), photocraft_engine::ai_cmds::flatten_rgba(&d.doc)));
                    if let Some((name, img)) = d {
                        add_reference(ui.ctx(), name, img);
                    }
                }
            });
        });
    });
    let mut remove = None;
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
                if resp.on_hover_text(format!("{} — click to remove", rf.name)).clicked() {
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

/// Starts the generation jobs for the current settings.
fn start(app: &mut PhotocraftApp, ctx: &egui::Context) {
    let s = app.ui.ai.generate.clone();
    let preset = presets().into_iter().find(|p| p.name == s.preset);
    let prompt = preset.as_ref().map_or_else(|| s.prompt.trim().to_owned(), |p| p.apply(&s.prompt));
    let doc = app.session.active().map(|d| (d.doc.id, photocraft_engine::ai_cmds::flatten_rgba(&d.doc), photocraft_engine::ai_cmds::selection_gray(&d.doc)));
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
        let mask = doc.as_ref().and_then(|(_, _, m)| m.clone());
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
        launch(app, &label, name, meta, None, false, move |ctl| li_ai::service().run_custom(&w, &inputs, &images, mask.as_ref(), ctl));
        ctx.request_repaint();
        return;
    }
    let m = model_of(&s);
    let info = m.info();
    let mut req = GenerateRequest::new(m, prompt);
    req.variant = s.variant.clone();
    req.width = s.width;
    req.height = s.height;
    req.transparent = s.transparent && info.transparent;
    req.steps = s.steps;
    req.guidance = s.guidance;
    req.negative = s.negative.clone();
    req.denoise = s.denoise;
    req.scale = s.upscale;
    req.loras = s.loras.iter().map(|l| li_ai::workflows::LoraUse { name: l.name.clone(), strength: l.strength }).collect();
    if !s.sampler.is_empty() {
        req.sampler = Some((s.sampler.clone(), if s.scheduler.is_empty() { info.resolved.pipeline.scheduler.clone() } else { s.scheduler.clone() }));
    }
    req.references = refs;
    let (mut target_doc, mut open) = (None, false);
    match s.mode {
        Mode::Create => {
            if s.refine
                && let Some(rm) = ModelId::from_key(&s.refine_model)
            {
                req.refine = Some(li_ai::RefineStep {
                    model: rm,
                    variant: s.refine_variant.clone(),
                    strength: s.refine_strength,
                    steps: None,
                    guidance: None,
                    scale: s.refine_scale,
                });
            }
        }
        mode => {
            let Some((id, img, sel)) = doc else { return };
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
        let name = format!("{}-{}", info.short.replace(' ', ""), r.seed);
        let meta = json!({
            "model": m.key(), "family": info.family, "variant": r.variant, "prompt": r.prompt, "negative_prompt": r.negative, "width": r.width, "height": r.height,
            "seed": r.seed, "transparent": r.transparent, "steps": r.steps, "guidance": r.guidance, "denoise": r.denoise,
            "loras": s.loras.iter().map(|l| json!({"name": l.name, "strength": l.strength})).collect::<Vec<_>>(),
            "refine": r.refine.as_ref().map(|x| json!({"model": x.model.key(), "strength": x.strength, "scale": x.scale})),
            "reference_count": r.references.len(), "mode": format!("{:?}", s.mode).to_lowercase(),
        });
        let mask = r.mask.clone();
        let ok = launch(app, &label, name, meta, target_doc.map(|d| d.0), open, move |ctl| {
            let out = li_ai::service().generate(&r, ctl)?;
            // Fill lands as a layer holding only the regenerated area.
            Ok(match (fill, mask) {
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
            })
        });
        if !ok {
            break;
        }
    }
    ctx.request_repaint();
}

/// Starts one job that produces an image, saves it to the library and shows its tile.
fn launch(
    app: &mut PhotocraftApp,
    label: &str,
    name: String,
    meta: Value,
    edit: Option<u64>,
    open: bool,
    work: impl FnOnce(&li_ai::JobControl) -> anyhow::Result<RgbaImage> + Send + 'static,
) -> bool {
    let params = json!({ "edit": edit, "name": name.clone(), "open": open });
    let started = app.session.start_job(
        GENERATE_JOB,
        params,
        label,
        false,
        move |ctx| {
            let img = photocraft_engine::ai_cmds::bridged(ctx, work)?;
            ctx.progress(0.98, "Saving to the library");
            let entry = Library::default()
                .add(&img, &name, Some(meta), None)
                .map_err(|e| photocraft_engine::EngineError::Other(format!("Could not save to the library: {e:#}")))?;
            Ok(entry)
        },
        |_, entry| Ok(serde_json::to_value(entry).unwrap_or(Value::Null)),
    );
    match started {
        Ok(photocraft_engine::jobs::Started::Job(id)) => {
            rt(|rt| rt.results.push_front(Tile::Pending { job: id.0, label: label.to_owned(), edit, open }));
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
    let tile = match &e.outcome {
        JobOutcome::Done(v) => match serde_json::from_value::<Entry>(v.clone()) {
            Ok(entry) => {
                app.ui.status = format!("Generated {} · seed {}", entry.name, entry.seed().unwrap_or(0));
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
                    let path = Library::default().image_path(&entry.id);
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
            app.ui.status = format!("Generation failed: {err}");
            app.ui.status_error = true;
            Tile::Failed(err.clone())
        }
        JobOutcome::Cancelled => Tile::Failed("Cancelled".into()),
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
    app.session.set_active(index);
    app.sync_views();
    let path = Library::default().image_path(&entry.id);
    let name = format!("Edit: {}", entry.prompt().chars().take(40).collect::<String>());
    if let Err(e) = app.run("ai.placeLayer", json!({ "path": path.display().to_string(), "name": name, "fit": "none" })) {
        app.ui.status = e;
        app.ui.status_error = true;
    }
}

fn results_ui(app: &mut PhotocraftApp, ui: &mut egui::Ui) {
    let t = Tokens::get(ui.ctx());
    let n = rt(|r| r.results.len());
    if n == 0 {
        ui.label(RichText::new("Results appear here and are saved to the Library.").color(t.text_faint).size(11.5));
        return;
    }
    widgets::section_label(ui, "RESULTS");
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
    let lib = Library::default();
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
                        "{}\n{} · {}×{}{}\nDouble-click to open",
                        e.prompt(),
                        e.model(),
                        e.width,
                        e.height,
                        e.seed().map(|s| format!(" · seed {s}")).unwrap_or_default()
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
                            ("Open", "open"),
                            ("Place as Layer", "place"),
                            ("Use as Reference", "reference"),
                            ("Recreate", "recreate"),
                            ("Copy Prompt", "copy"),
                        ] {
                            let enabled = act != "place" || app.session.active().is_some();
                            if ui.add_enabled(enabled, egui::Button::new(label)).clicked() {
                                action = Some((e.clone(), act));
                                ui.close();
                            }
                        }
                        ui.separator();
                        if ui.button("Delete from Library").clicked() {
                            action = Some((e.clone(), "delete"));
                            ui.close();
                        }
                    });
                }
                None => {
                    let text = pending.get(i).cloned().unwrap_or_default();
                    let busy = !text.starts_with("Cancelled") && text.contains('\n');
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
            if widgets::secondary_button(ui, "Open", 0.0).clicked() {
                action = Some((e.clone(), "open"));
            }
            if app.session.active().is_some() && widgets::secondary_button(ui, "Place as Layer", 0.0).clicked() {
                action = Some((e.clone(), "place"));
            }
            if widgets::secondary_button(ui, "Use as Reference", 0.0).clicked() {
                action = Some((e.clone(), "reference"));
            }
        });
    }
    if let Some((e, act)) = action {
        entry_action(app, ui.ctx(), &e, act);
    }
}

fn entry_action(app: &mut PhotocraftApp, ctx: &egui::Context, e: &Entry, act: &str) {
    let lib = Library::default();
    match act {
        "open" => match std::fs::read(lib.image_path(&e.id)) {
            Ok(bytes) => {
                if let Err(err) = app.open_bytes(&e.name, &bytes) {
                    app.ui.status = err;
                    app.ui.status_error = true;
                }
            }
            Err(err) => {
                app.ui.status = format!("Could not open {}: {err}", e.name);
                app.ui.status_error = true;
            }
        },
        "place" => {
            let r = app.run(
                "ai.placeLayer",
                json!({ "path": lib.image_path(&e.id).display().to_string(), "name": e.name.trim_end_matches(".png"), "fit": "contain" }),
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
                r.library.retain(|x| x.id != e.id);
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
        let list = Library::default().list();
        rt(|r| {
            r.library = list;
            r.library_loaded = Some(Instant::now());
        });
    }
    ui.horizontal(|ui| {
        ui.add(egui::TextEdit::singleline(&mut app.ui.ai.generate.library_filter).hint_text("Search prompts").desired_width(ui.available_width() - 30.0));
        if crate::icons::button(ui, "rotate-cw", 22.0, false, "Refresh").clicked() {
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
        ui.label(RichText::new(if filter.is_empty() { "Generated images are kept here." } else { "Nothing matches." }).color(t.text_faint));
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
        let Some(dir) = rfd::FileDialog::new().set_title("Open Folder").pick_folder() else { return Ok(Value::Null) };
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
    app.ui.status = format!("{} images in {}", files.len(), dir.display());
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
    egui::Window::new("Remove Backgrounds")
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
                    RichText::new("Cuts out the subject of each image with Qwen Image 2.1 and saves PNGs. Originals are never changed.").color(t.text_dim),
                );
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    #[cfg(not(target_arch = "wasm32"))]
                    {
                        if widgets::secondary_button(ui, "Add Images…", 0.0).clicked()
                            && let Some(paths) = rfd::FileDialog::new().add_filter("Images", &["png", "jpg", "jpeg", "webp", "tif", "tiff"]).pick_files()
                        {
                            b.files.extend(paths);
                        }
                        if widgets::secondary_button(ui, "Add Folder…", 0.0).clicked()
                            && let Some(dir) = rfd::FileDialog::new().pick_folder()
                        {
                            b.files.extend(folder_images(&dir).into_iter().filter(|p| {
                                p.extension()
                                    .and_then(|e| e.to_str())
                                    .is_some_and(|e| ["png", "jpg", "jpeg", "webp", "tif", "tiff"].contains(&e.to_ascii_lowercase().as_str()))
                            }));
                        }
                    }
                    ui.label(RichText::new(format!("{} images", b.files.len())).color(t.text_dim));
                    if !b.files.is_empty() && widgets::secondary_button(ui, "Clear", 0.0).clicked() {
                        b.files.clear();
                    }
                });
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Background:").color(t.text_dim));
                    let opts = [("transparent".to_string(), "Transparent PNG"), ("white".to_string(), "White"), ("color".to_string(), "Color")];
                    widgets::dropdown(ui, "batch-bg", &mut b.background, &opts, 150.0);
                    if b.background == "color" {
                        ui.color_edit_button_srgb(&mut b.color);
                    }
                    ui.label(RichText::new("Model:").color(t.text_dim));
                    let opts = [("int8".to_string(), "Qwen Compact"), ("bf16".to_string(), "Qwen Full")];
                    widgets::dropdown(ui, "batch-model", &mut b.variant, &opts, 130.0);
                });
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Save to:").color(t.text_dim));
                    let out = b.output.as_ref().map(|p| p.display().to_string()).unwrap_or_else(|| "a “cutouts” folder beside each image".into());
                    ui.label(out);
                    #[cfg(not(target_arch = "wasm32"))]
                    if widgets::secondary_button(ui, "Choose…", 0.0).clicked() {
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
                        None => {
                            b.files.iter().map(|p| (p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default(), "Waiting".into())).collect()
                        }
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
                            ui.add(egui::ProgressBar::new(d as f32 / n.max(1) as f32).desired_width(240.0).text(format!("{d} of {n}")));
                            if widgets::secondary_button(ui, "Stop", 0.0).clicked()
                                && let Ok(r) = r.lock()
                            {
                                r.ctl.cancel();
                            }
                        }
                        ui.ctx().request_repaint_after(Duration::from_millis(250));
                    } else {
                        let ok = !b.files.is_empty() && st.ready(ModelId::Qwen, &b.variant).is_ok();
                        if ui.add_enabled_ui(ok, |ui| widgets::primary_button(ui, "Remove Backgrounds", 0.0)).inner.clicked() {
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
        status: b.files.iter().map(|p| (p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default(), "Waiting".into())).collect(),
        ..Default::default()
    }));
    let (files, output, bg, color, variant) = (b.files.clone(), b.output.clone(), b.background.clone(), b.color, b.variant.clone());
    let r2 = run.clone();
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
            set("Working…".into());
            let res = (|| -> anyhow::Result<PathBuf> {
                let img = image::open(path)?.to_rgba8();
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
                Ok(dest) => set(format!("Saved {}", dest.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default())),
                Err(e) if e.is::<li_ai::Cancelled>() => set("Stopped".into()),
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
    fn natural_order() {
        let mut v = vec!["img10.jpg", "img2.jpg", "IMG1.jpg", "a.png"];
        v.sort_by(|a, b| natural_cmp(a, b));
        assert_eq!(v, vec!["a.png", "IMG1.jpg", "img2.jpg", "img10.jpg"]);
    }

    #[test]
    fn aspect_sizes_are_about_a_megapixel_on_the_grid() {
        let s = GenerateState::default();
        for (k, r) in ASPECTS {
            let (w, h) = size_for(k, &s);
            assert!(w % 16 == 0 && h % 16 == 0);
            assert!(((w as f32 / h as f32) - r).abs() < 0.05, "{k}: {w}x{h}");
            assert!((800_000..1_300_000).contains(&(w * h)), "{k}: {w}x{h}");
        }
    }
}
