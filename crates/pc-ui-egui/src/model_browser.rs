//! local-image: the Model Browser window. Models and LoRAs from Hugging Face, Civitai, ComfyUI's
//! official templates and the ComfyUI-Manager community list, laid out like SwarmUI's and
//! InvokeAI's model managers with Civitai/Hugging Face-style cards: a navigation column (New,
//! Trending, Installed, Works on my GPU, then each family), search and source filters, and a grid
//! of cards showing capabilities, download size, rough GPU memory and licence.
//!
//! Nothing goes online until the window is open; every catalogue is cached so the window still
//! works offline. Installing shows the licence and the exact files first, then downloads them
//! verified (size and SHA-256) into ComfyUI's folders and refreshes the engine. A template card is
//! checked against the connected ComfyUI before anything downloads; a template for a family Local
//! Image has no builder for is set up to run through that template (Set Up Models tests it).

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use egui::{Color32, RichText, Sense, Stroke, StrokeKind, TextureHandle, vec2};
use li_ai::browser::{self, Cache, Config, HttpNet, Item, Kind, Net, Query, Source, View};
use li_ai::settings::AiSettings;
use serde_json::Value;

use crate::PhotocraftApp;
use crate::theme::Tokens;
use crate::widgets;

mod install;

#[cfg(test)]
pub(crate) use install::test_install_plan;
use install::{Confirm, InstallRun, confirm_window};

#[derive(Clone, Debug, PartialEq)]
enum Nav {
    New,
    Trending,
    Installed,
    FitsGpu,
    Family(String),
}

type Slot<T> = Arc<Mutex<Option<T>>>;

struct Fetched {
    items: Vec<Item>,
    notes: Vec<String>,
}

enum Preview {
    Loading(Slot<Option<egui::ColorImage>>),
    Ready(TextureHandle),
    Failed,
}

struct State {
    open: bool,
    kind: Kind,
    nav: Nav,
    search: String,
    sources: [bool; 4],
    loaded: Option<(Nav, Kind, String, [bool; 4])>,
    fetch: Option<Slot<Fetched>>,
    items: Vec<Item>,
    notes: Vec<String>,
    previews: HashMap<String, Preview>,
    installs: HashMap<String, Arc<Mutex<InstallRun>>>,
    confirm: Option<Confirm>,
    profiles: Option<Slot<Result<li_ai::family::UpdateReport, String>>>,
    profiles_msg: String,
    installed: Option<(std::time::Instant, Arc<HashSet<String>>)>,
    installed_at: Option<std::time::Instant>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            open: false,
            kind: Kind::Model,
            nav: Nav::Trending,
            search: String::new(),
            sources: [true; 4],
            loaded: None,
            fetch: None,
            items: Vec::new(),
            notes: Vec::new(),
            previews: HashMap::new(),
            installs: HashMap::new(),
            confirm: None,
            profiles: None,
            profiles_msg: String::new(),
            installed: None,
            installed_at: None,
        }
    }
}

thread_local! { static STATE: RefCell<State> = RefCell::new(State::default()); }

fn with<R>(f: impl FnOnce(&mut State) -> R) -> R {
    STATE.with(|s| f(&mut s.borrow_mut()))
}

const SOURCES: [(Source, &str); 4] =
    [(Source::HuggingFace, "Hugging Face"), (Source::Civitai, "Civitai"), (Source::Template, "Templates"), (Source::Community, "Community")];

#[cfg(feature = "snapshot-fixtures")]
pub(crate) fn snapshot_scene() {
    with(|s| {
        *s = State { open: true, ..Default::default() };
        s.loaded = Some((s.nav.clone(), s.kind, s.search.clone(), s.sources));
        s.installed = Some((std::time::Instant::now(), Arc::new(HashSet::new())));
        s.items = vec![
            Item {
                id: "synthetic-flux3".into(),
                title: "FLUX.3 Klein Distilled".into(),
                family: Some("flux3-klein".into()),
                license: "FLUX Kommunity".into(),
                source: Some(Source::HuggingFace),
                ..Default::default()
            },
            Item {
                id: "synthetic-moss".into(),
                title: "MOSS-Image SFT".into(),
                template: Some("image_moss_image_t2i".into()),
                license: "Apache-2.0".into(),
                min_comfyui: Some("0.44.0".into()),
                ..Default::default()
            },
            Item {
                id: "synthetic-krea".into(),
                title: "Krea 2 Turbo".into(),
                family: Some("krea2".into()),
                license: "Apache-2.0".into(),
                source: Some(Source::HuggingFace),
                ..Default::default()
            },
        ];
    });
}

/// Opens the browser window.
pub fn open() {
    with(|s| s.open = true);
}

pub(crate) fn open_restoration() {
    with(|s| {
        s.open = true;
        s.kind = Kind::Model;
        s.nav = Nav::Family("restoration".into());
        s.search.clear();
        s.fetch = None;
        s.loaded = None;
    });
}

/// Opens it on the LoRAs that fit `family`.
pub fn open_loras(family: &str) {
    with(|s| {
        s.open = true;
        s.kind = Kind::Lora;
        s.nav = Nav::Family(family.to_owned());
    });
}

/// Opens it on LoRAs (trending), for a model whose family is unknown.
pub fn open_loras_any() {
    with(|s| {
        s.open = true;
        s.kind = Kind::Lora;
        s.nav = Nav::Trending;
    });
}

/// Which list it shows: (LoRAs, family) — for tests.
pub fn showing() -> (bool, Option<String>) {
    with(|s| {
        (
            s.kind == Kind::Lora,
            match &s.nav {
                Nav::Family(f) => Some(f.clone()),
                _ => None,
            },
        )
    })
}

/// Closes the window.
pub fn close() {
    with(|s| s.open = false);
}

pub fn is_open() -> bool {
    with(|s| s.open)
}

/// The sources' configuration: the AI settings' tokens, and the connected ComfyUI's own
/// (version-matched) copy of the official templates.
pub(crate) fn config() -> Config {
    let mut cfg = Config::from_env(&AiSettings::load());
    let st = crate::ai_ui::status();
    if st.connected && std::env::var("LOCAL_IMAGE_TEMPLATES_BASE").is_err() {
        cfg.templates = format!("http://{}/templates", st.host);
    }
    cfg
}

fn gpu_gb() -> Option<f32> {
    crate::ai_ui::status().gpu.map(|g| g.vram_total as f32 / 1e9).filter(|v| *v > 0.5)
}

fn query(s: &State) -> Query {
    let view = match &s.nav {
        Nav::New => View::New,
        Nav::Trending => View::Trending,
        Nav::Installed => View::Installed,
        Nav::FitsGpu => View::FitsGpu(gpu_gb().unwrap_or(8.0)),
        Nav::Family(f) => View::Family(f.clone()),
    };
    let sources = SOURCES.iter().zip(s.sources).filter(|(_, on)| *on).map(|((src, _), _)| *src).collect::<Vec<_>>();
    Query { view, kind: s.kind, search: s.search.clone(), sources: if sources.len() == SOURCES.len() { vec![] } else { sources } }
}

fn age(secs: u64) -> String {
    match secs {
        0..120 => tl!("just now").into(),
        120..7200 => li_i18n::fmt(tl!("{n} min ago"), &[("n", &(secs / 60).to_string())]),
        7200..172800 => li_i18n::fmt(tl!("{n} h ago"), &[("n", &(secs / 3600).to_string())]),
        _ => li_i18n::fmt(tl!("{n} days ago"), &[("n", &(secs / 86400).to_string())]),
    }
}

/// Fetches the query's catalogues on a worker thread (cache fallback per source).
fn start_fetch(q: Query) -> Slot<Fetched> {
    let slot: Slot<Fetched> = Arc::new(Mutex::new(None));
    let out = slot.clone();
    // Notes are shown in the window: the worker draws them in the UI's language.
    let lang = li_i18n::current();
    let _ = std::thread::Builder::new().name("model-browser".into()).spawn(move || {
        li_i18n::set_current(lang);
        let cfg = config();
        let reg = li_ai::family::registry();
        let net = HttpNet::new(cfg.clone());
        let cache = Cache::new(Cache::default_dir());
        let mut bodies = Vec::new();
        let mut notes = Vec::new();
        for (src, url) in browser::urls(&cfg, reg, &q) {
            match cache.fetch(&net, &url, true) {
                Ok((body, cached)) => {
                    if cached {
                        let when = cache.get(&url).map(|(_, a)| age(a)).unwrap_or_default();
                        notes.push(li_i18n::fmt(tl!("{source}: offline, showing results from {when}"), &[("source", src.label()), ("when", &when)]));
                    }
                    match serde_json::from_slice::<Value>(&body) {
                        Ok(v) => bodies.push((src, v)),
                        Err(_) => notes.push(li_i18n::fmt(tl!("{source}: the catalogue couldn't be read"), &[("source", src.label())])),
                    }
                }
                Err(e) => notes.push(format!("{}: {}", src.label(), first_line(&format!("{e:#}")))),
            }
        }
        let items = browser::items(reg, &cfg, &q, &bodies);
        if let Ok(mut o) = out.lock() {
            *o = Some(Fetched { items, notes });
        }
        request_repaint();
    });
    slot
}

fn first_line(s: &str) -> String {
    let l = s.lines().next().unwrap_or(s);
    if l.chars().count() > 140 { format!("{}…", l.chars().take(140).collect::<String>()) } else { l.to_owned() }
}

fn request_repaint() {
    if let Some(ctx) = CTX.lock().ok().and_then(|c| c.clone()) {
        ctx.request_repaint();
    }
}

static CTX: Mutex<Option<egui::Context>> = Mutex::new(None);

/// Installed models and LoRAs as cards (no network).
fn installed_items(kind: Kind) -> Vec<Item> {
    let st = crate::ai_ui::status();
    match kind {
        Kind::Model => li_ai::catalog::catalog()
            .models
            .iter()
            .filter(|m| (!m.tool || m.family == "restoration") && li_ai::catalog::presets_for(m.id).any(|p| st.ready(m.id, &p.variant).is_ok()))
            .map(|m| Item {
                id: format!("installed:{}", m.id.key()),
                source: Some(Source::Installed),
                kind: Some(Kind::Model),
                title: m.label.clone(),
                description: m.best_for.clone(),
                family: Some(m.family.clone()),
                license: m.license.clone(),
                license_url: Some(m.license_url.clone()).filter(|u| !u.is_empty()),
                ..Default::default()
            })
            .collect(),
        Kind::Lora => li_ai::inventory::loras()
            .into_iter()
            .map(|l| Item {
                id: format!("installed-lora:{}", l.name),
                source: Some(Source::Installed),
                kind: Some(Kind::Lora),
                title: l.name.rsplit('/').next().unwrap_or(&l.name).trim_end_matches(".safetensors").to_owned(),
                description: l.evidence.clone(),
                family: l.family.clone(),
                ..Default::default()
            })
            .collect(),
    }
}

/// The model folder (the settings re-read every two seconds, not for every card every frame).
fn model_dir() -> std::path::PathBuf {
    thread_local! { static DIR: RefCell<Option<(std::time::Instant, std::path::PathBuf)>> = const { RefCell::new(None) }; }
    DIR.with(|d| {
        let mut d = d.borrow_mut();
        match d.as_ref() {
            Some((t, p)) if t.elapsed() < Duration::from_secs(2) => p.clone(),
            _ => {
                let p = AiSettings::load().model_dir();
                *d = Some((std::time::Instant::now(), p.clone()));
                p
            }
        }
    })
}

/// What removing an Installed card takes out, when its files are in the model folder: a curated
/// model's downloaded presets, or the one file of a model or LoRA ComfyUI found.
fn removal_for(item: &Item) -> Option<crate::ai_ui::RemoveTarget> {
    use crate::ai_ui::RemoveTarget;
    if AiSettings::load().is_remote() || item.source != Some(Source::Installed) {
        return None;
    }
    let dir = model_dir();
    if let Some(name) = item.id.strip_prefix("installed-lora:") {
        return li_ai::download::listed_file(&dir, &["loras"], name).map(|_| RemoveTarget::Listed { folders: vec!["loras"], name: name.to_owned() });
    }
    let key = item.id.strip_prefix("installed:")?;
    let m = li_ai::catalog::ModelId::from_key(key)?;
    let info = m.try_info()?;
    if info.origin == li_ai::catalog::Origin::Profile {
        let ids: Vec<String> =
            li_ai::catalog::presets_for(m).filter(|p| li_ai::download::missing_files(&dir, p).len() < p.files.len()).map(|p| p.id()).collect();
        return (!ids.is_empty()).then_some(RemoveTarget::Presets(ids));
    }
    let (folders, name): (Vec<&'static str>, &str) = match key.split_once(':')? {
        ("ckpt", n) => (vec!["checkpoints"], n),
        ("unet", n) => (vec!["diffusion_models", "unet", "unet_gguf"], n),
        _ => return None,
    };
    li_ai::download::listed_file(&dir, &folders, name).map(|_| RemoveTarget::Listed { folders, name: name.to_owned() })
}

/// File names ComfyUI already has (main models and LoRAs), by base name.
fn cached_installed(s: &mut State, scan: impl FnOnce() -> HashSet<String>) -> Arc<HashSet<String>> {
    if let Some((at, names)) = &s.installed
        && at.elapsed() < std::time::Duration::from_secs(1)
    {
        return names.clone();
    }
    let names = Arc::new(scan());
    s.installed = Some((std::time::Instant::now(), names.clone()));
    names
}

fn installed_names() -> HashSet<String> {
    let base = |n: &str| n.rsplit('/').next().unwrap_or(n).to_ascii_lowercase();
    let mut out: HashSet<String> = li_ai::catalog::catalog().models.iter().filter_map(|m| m.id.key().split_once(':').map(|(_, n)| base(n))).collect();
    out.extend(li_ai::inventory::loras().iter().map(|l| base(&l.name)));
    if AiSettings::load().is_remote() {
        return out;
    }
    let dir = model_dir();
    out.extend(
        li_ai::restoration::policy()
            .models
            .iter()
            .flat_map(|m| &m.presets)
            .flat_map(|p| &p.files)
            .filter(|f| li_ai::download::target_path(&dir, f).is_file())
            .map(|f| base(&f.name)),
    );
    out
}

fn is_installed(item: &Item, names: &HashSet<String>) -> bool {
    let weights: Vec<_> = item
        .files
        .iter()
        .filter(|f| f.name.ends_with(".safetensors") || f.name.ends_with(".gguf") || li_ai::restoration::pinned_file(&f.name).is_some())
        .collect();
    item.source == Some(Source::Installed) || (!weights.is_empty() && weights.iter().all(|f| names.contains(&f.name.to_ascii_lowercase())))
}

fn load_preview(url: String) -> Slot<Option<egui::ColorImage>> {
    let slot: Slot<Option<egui::ColorImage>> = Arc::new(Mutex::new(None));
    let out = slot.clone();
    let _ = std::thread::Builder::new().name("model-preview".into()).spawn(move || {
        let net = HttpNet::new(config());
        let cache = Cache::new(Cache::default_dir().join("previews"));
        let img = cache.fetch(&net, &url, true).ok().and_then(|(b, _)| image::load_from_memory(&b).ok()).map(|i| {
            let t = i.thumbnail(480, 480).to_rgba8();
            egui::ColorImage::from_rgba_unmultiplied([t.width() as usize, t.height() as usize], t.as_raw())
        });
        if let Ok(mut o) = out.lock() {
            *o = Some(img);
        }
        request_repaint();
    });
    slot
}

fn start_profile_update() -> Slot<Result<li_ai::family::UpdateReport, String>> {
    let slot = Arc::new(Mutex::new(None));
    let out = slot.clone();
    let _ = std::thread::Builder::new().name("family-update".into()).spawn(move || {
        let net = HttpNet::new(config());
        let base = std::env::var("LOCAL_IMAGE_FAMILIES_BASE").unwrap_or_else(|_| li_ai::family::REMOTE_BASE.to_owned());
        let r = li_ai::family::update_from(&|u| net.get(u), &base, &li_ai::family::cache_dir()).map_err(|e| first_line(&format!("{e:#}")));
        if matches!(&r, Ok(v) if !v.updated.is_empty() || !v.kept_user.is_empty()) {
            li_ai::family::reload();
            crate::ai_ui::refresh();
        }
        if let Ok(mut o) = out.lock() {
            *o = Some(r);
        }
        request_repaint();
    });
    slot
}

/// One line per outcome of a profile update.
fn profile_report(r: &li_ai::family::UpdateReport) -> String {
    let mut parts = Vec::new();
    let names = |v: &[String]| v.join(", ");
    match r.updated.len() {
        0 => {}
        1 => parts.push(li_i18n::fmt(tl!("Updated 1 model profile: {names}"), &[("names", &names(&r.updated))])),
        n => parts.push(li_i18n::fmt(tl!("Updated {n} model profiles: {names}"), &[("n", &n.to_string()), ("names", &names(&r.updated))])),
    }
    if !r.kept_user.is_empty() {
        parts.push(li_i18n::fmt(tl!("Your own profiles stay in use: {names}"), &[("names", &names(&r.kept_user))]));
    }
    if !r.needs_newer_app.is_empty() {
        parts.push(li_i18n::fmt(tl!("Needs a newer Local Image: {names}"), &[("names", &names(&r.needs_newer_app))]));
    }
    if !r.rejected.is_empty() {
        let ids: Vec<String> = r.rejected.iter().map(|(id, _)| id.clone()).collect();
        parts.push(li_i18n::fmt(tl!("Not used (the previous copy stays): {names}"), &[("names", &names(&ids))]));
    }
    parts.join(" · ")
}

/// The template's minimum ComfyUI, when the connected server is older.
fn needs_newer_comfy(item: &Item) -> Option<String> {
    use li_ai::template::index::Version;
    let need = Version::parse(item.min_comfyui.as_deref()?)?;
    let have = Version::parse(&crate::ai_ui::status().comfyui_version)?;
    (have < need).then(|| need.to_string())
}

fn human(n: u64) -> String {
    li_ai::download::human_bytes(n)
}

/// A source filter: a quiet outlined chip, filled when on.
fn filter_chip(ui: &mut egui::Ui, text: &str, on: bool, disabled: bool, t: &Tokens) -> egui::Response {
    let ink = if disabled {
        t.text_faint
    } else if on {
        t.text
    } else {
        t.text_dim
    };
    let galley = ui.painter().layout_no_wrap(text.to_owned(), egui::FontId::proportional(12.0), ink);
    let (r, resp) = ui.allocate_exact_size(vec2(galley.size().x + 22.0, 22.0), Sense::click());
    let fill = if on && !disabled {
        t.accent_soft
    } else if resp.hovered() && !disabled {
        t.hover
    } else {
        t.field
    };
    ui.painter().rect_filled(r, 11.0, fill);
    ui.painter().rect_stroke(r, 11.0, Stroke::new(1.0, if on && !disabled { t.accent_border } else { t.field_border }), StrokeKind::Inside);
    if on && !disabled {
        ui.painter().circle_filled(egui::pos2(r.left() + 9.0, r.center().y), 2.5, t.accent);
    }
    ui.painter().galley(egui::pos2(r.left() + 15.0, r.center().y - galley.size().y / 2.0), galley, ink);
    resp.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Checkbox, !disabled, on, text));
    resp
}

fn capability_tags(item: &Item) -> Vec<String> {
    let reg = li_ai::family::registry();
    let mut tags = Vec::new();
    if item.kind == Some(Kind::Lora) {
        tags.push("LoRA".to_owned());
    }
    if let Some(f) = item.family.as_deref().and_then(|f| reg.family(f)) {
        tags.push(f.label.clone());
        if item.kind != Some(Kind::Lora) {
            let c = f.capabilities;
            if c.create {
                tags.push(tl!("Create").into());
            }
            if c.edit {
                tags.push(tl!("Edit").into());
            }
            if c.inpaint {
                tags.push(tl!("Fill").into());
            }
            if c.references > 0 {
                tags.push(li_i18n::trn(li_i18n::current(), c.references as u64, "{n} ref", "{n} refs"));
            }
        }
    } else if item.template.is_none() {
        tags.push(tl!("Unknown family").into());
    }
    tags
}

/// The window (call every frame).
pub fn window(app: &mut PhotocraftApp, ctx: &egui::Context) {
    if !is_open() {
        return;
    }
    if let Ok(mut c) = CTX.lock()
        && c.is_none()
    {
        *c = Some(ctx.clone());
    }
    let t = Tokens::get(ctx);
    let mut open = true;
    let screen = ctx.content_rect().size();
    let size = vec2((screen.x - 160.0).clamp(640.0, 1120.0), (screen.y - 200.0).clamp(380.0, 680.0));
    if crate::restoration_ui::model_browser_is_modal(app) || app.ui.dialogs.iter().any(|dialog| crate::prefs_ui::is_preferences(&dialog.fields)) {
        let modal = egui::Modal::new(egui::Id::new("restoration-model-browser")).backdrop_color(Color32::TRANSPARENT).show(ctx, |ui| {
            ui.set_width(size.x);
            ui.horizontal(|ui| {
                ui.heading(tl!("Model Browser"));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button(tl!("Close")).clicked() {
                        open = false;
                    }
                });
            });
            ui.allocate_ui_with_layout(size, egui::Layout::top_down(egui::Align::Min), |ui| browser_content(app, ui, &t));
        });
        #[cfg(test)]
        ctx.data_mut(|data| data.insert_temp(egui::Id::new("restoration-model-browser-rect"), modal.response.rect));
        if modal.is_top_modal && !modal.any_popup_open && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {
            open = false;
        }
    } else {
        egui::Window::new(tl!("Model Browser"))
            .collapsible(false)
            .resizable(false)
            .fixed_size(size)
            .frame(egui::Frame::window(&ctx.global_style()).fill(t.card).stroke(Stroke::new(1.0, t.card_border)).inner_margin(egui::Margin::same(0)))
            .open(&mut open)
            .anchor(egui::Align2::CENTER_CENTER, vec2(0.0, 0.0))
            .show(ctx, |ui| browser_content(app, ui, &t));
    }
    confirm_window(ctx, &t);
    if !open {
        with(|s| {
            s.open = false;
            s.confirm = None;
        });
    }
    let _ = app;
}

fn browser_content(app: &mut PhotocraftApp, ui: &mut egui::Ui, t: &Tokens) {
    if with(|s| matches!(&s.nav, Nav::Family(f) if f == "restoration")) && ui.button(tl!("Bring your own model…")).clicked() {
        crate::restoration_ui::open(app);
        close();
    }
    with(|s| body(ui, s, t));
}

fn body(ui: &mut egui::Ui, s: &mut State, t: &Tokens) {
    // Start or collect the catalogue fetch.
    let key = (s.nav.clone(), s.kind, s.search.trim().to_owned(), s.sources);
    if s.loaded.as_ref() != Some(&key) && matches!(&s.nav, Nav::Family(f) if f == "restoration") {
        s.fetch = None;
    }
    if s.loaded.as_ref() != Some(&key) && s.fetch.is_none() {
        s.loaded = Some(key);
        if s.nav == Nav::Installed {
            s.installed_at = Some(std::time::Instant::now());
            s.items = installed_items(s.kind);
            s.items.retain(|i| s.search.trim().is_empty() || i.title.to_lowercase().contains(&s.search.trim().to_lowercase()));
            s.notes.clear();
        } else if matches!(&s.nav, Nav::Family(f) if f == "restoration") {
            s.items = browser::items(li_ai::family::registry(), &Config::default(), &query(s), &[]);
            s.notes.clear();
        } else {
            s.fetch = Some(start_fetch(query(s)));
        }
    }
    // Installed: always current (no network), so removed models drop off once ComfyUI re-scans.
    if s.nav == Nav::Installed && s.fetch.is_none() && s.installed_at.is_none_or(|at| at.elapsed() >= std::time::Duration::from_secs(1)) {
        s.installed_at = Some(std::time::Instant::now());
        s.items = installed_items(s.kind);
        s.items.retain(|i| s.search.trim().is_empty() || i.title.to_lowercase().contains(&s.search.trim().to_lowercase()));
    }
    if let Some(f) = s.fetch.as_ref().and_then(|f| f.lock().ok().and_then(|mut o| o.take())) {
        s.items = f.items;
        s.notes = f.notes;
        s.fetch = None;
    }
    let loading = s.fetch.is_some();
    let full = ui.available_size();

    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        // Navigation column.
        let side = 200.0;
        let h = full.y;
        ui.allocate_ui_with_layout(vec2(side, h), egui::Layout::top_down(egui::Align::LEFT), |ui| {
            ui.painter().rect_filled(ui.max_rect(), 0.0, t.dock);
            egui::Frame::new().inner_margin(egui::Margin::symmetric(10, 12)).show(ui, |ui| {
                ui.set_width(side - 20.0);
                ui.horizontal(|ui| {
                    for (k, label) in [(Kind::Model, tl!("Models")), (Kind::Lora, "LoRAs")] {
                        if widgets::pill_tab(ui, label, s.kind == k).clicked() {
                            s.kind = k;
                        }
                    }
                });
                ui.add_space(10.0);
                widgets::section_label(ui, tl!("BROWSE"));
                let gpu = gpu_gb();
                let gpu_text = gpu.map(|g| format!("{g:.0} GB")).unwrap_or_default();
                for (nav, icon, label, trailing) in [
                    (Nav::New, "star", li_i18n::tr_ctx(li_i18n::current(), "ai", "New"), ""),
                    (Nav::Trending, "trending-up", tl!("Trending"), ""),
                    (Nav::Installed, "hard-drive", tl!("Installed"), ""),
                    (Nav::FitsGpu, "cpu", tl!("Works on my GPU"), gpu_text.as_str()),
                ] {
                    if widgets::nav_row(ui, icon, None, label, trailing, s.nav == nav).clicked() {
                        s.nav = nav;
                    }
                }
                ui.add_space(10.0);
                widgets::section_label(ui, tl!("FAMILIES"));
                egui::ScrollArea::vertical().id_salt("browser-families").auto_shrink([false, false]).show(ui, |ui| {
                    let reg = li_ai::family::registry();
                    let mut group = "";
                    for f in
                        reg.families.iter().filter(|f| f.id == "restoration" || f.models.iter().any(|m| !m.tool) || f.base.is_some() || f.models.is_empty())
                    {
                        if f.id == "seedvr2" {
                            continue;
                        }
                        if f.group != group {
                            group = &f.group;
                            ui.add_space(4.0);
                            ui.label(RichText::new(&f.group).size(10.5).color(t.text_faint));
                        }
                        let on = s.nav == Nav::Family(f.id.clone());
                        if widgets::nav_row(ui, "", None, &f.label, "", on).clicked() {
                            s.nav = Nav::Family(f.id.clone());
                        }
                    }
                });
            });
        });
        // Main column.
        ui.allocate_ui_with_layout(vec2(full.x - side, h), egui::Layout::top_down(egui::Align::LEFT), |ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            egui::Frame::new().inner_margin(egui::Margin { left: 14, right: 14, top: 12, bottom: 0 }).show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    let hint = if s.kind == Kind::Lora { tl!("Search LoRAs") } else { tl!("Search models") };
                    let resp = ui.add(egui::TextEdit::singleline(&mut s.search).hint_text(hint).desired_width(ui.available_width().min(240.0)));
                    if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        s.loaded = None;
                    }
                    ui.add_space(6.0);
                    for (i, (_, label)) in SOURCES.iter().enumerate() {
                        let disabled = (s.kind == Kind::Lora && i == 2) || s.nav == Nav::Installed;
                        let on = s.sources[i] && !disabled;
                        let resp = filter_chip(ui, tl!(label), on, disabled, t);
                        if resp.clicked() && !disabled {
                            s.sources[i] = !s.sources[i];
                            if !s.sources.iter().any(|x| *x) {
                                s.sources = [true; 4];
                            }
                        }
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if crate::icons::button(ui, "refresh-cw", 26.0, false, tl!("Refresh")).clicked() {
                            s.loaded = None;
                        }
                        if loading {
                            ui.spinner();
                        }
                    });
                });
                ui.add_space(4.0);
                let title = match &s.nav {
                    Nav::New => tl!("Newest").to_owned(),
                    Nav::Trending => tl!("Trending this week").to_owned(),
                    Nav::Installed => tl!("Installed").to_owned(),
                    Nav::FitsGpu => tl!("Fits your GPU memory").to_owned(),
                    Nav::Family(f) => li_ai::family::registry().family(f).map(|f| f.label.clone()).unwrap_or_default(),
                };
                ui.horizontal(|ui| {
                    ui.label(RichText::new(title).size(15.0).strong().color(t.text));
                    let count = if s.kind == Kind::Lora { tl!("{n} LoRAs") } else { tl!("{n} models") };
                    ui.label(RichText::new(li_i18n::fmt(count, &[("n", &s.items.len().to_string())])).color(t.text_faint));
                });
                if let Nav::Family(f) = &s.nav
                    && let Some(fam) = li_ai::family::registry().family(f)
                {
                    ui.label(RichText::new(&fam.description).color(t.text_dim));
                }
                if s.nav == Nav::FitsGpu && gpu_gb().is_none() {
                    ui.label(RichText::new(tl!("Start the AI engine to see your GPU's memory; showing what fits 8 GB.")).color(t.warning));
                }
                for n in &s.notes {
                    ui.label(RichText::new(n).size(11.5).color(t.warning));
                }
                ui.add_space(6.0);
            });
            let footer = 34.0;
            let h = (ui.available_height() - footer).max(120.0);
            egui::ScrollArea::vertical().id_salt("browser-grid").max_height(h).auto_shrink([false, false]).show(ui, |ui| {
                egui::Frame::new().inner_margin(egui::Margin { left: 14, right: 14, top: 0, bottom: 12 }).show(ui, |ui| grid(ui, s, t, loading));
            });
            footer_ui(ui, s, t);
        });
    });
}

fn grid(ui: &mut egui::Ui, s: &mut State, t: &Tokens, loading: bool) {
    if s.items.is_empty() {
        ui.add_space(40.0);
        ui.vertical_centered(|ui| {
            let msg = if loading {
                tl!("Loading catalogues…")
            } else if s.nav == Nav::Installed {
                tl!("Nothing installed yet. Pick a model from Trending or a family.")
            } else {
                tl!("No matches. Try another family, source or search.")
            };
            ui.label(RichText::new(msg).color(t.text_dim));
        });
        return;
    }
    let names = cached_installed(s, installed_names);
    let gap = 12.0;
    let w = ui.available_width();
    let cols = ((w + gap) / (228.0 + gap)).floor().max(1.0) as usize;
    let card_w = ((w - gap * (cols as f32 - 1.0)) / cols as f32).floor();
    let mut action: Option<Item> = None;
    let items = s.items.clone();
    for row in items.chunks(cols) {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = gap;
            for item in row {
                if card(ui, s, t, item, card_w, &names) {
                    action = Some(item.clone());
                }
            }
        });
        ui.add_space(gap);
    }
    if let Some(item) = action {
        s.confirm = Some(Confirm::new(item));
    }
}

/// One card; returns true when Install is clicked.
fn card(ui: &mut egui::Ui, s: &mut State, t: &Tokens, item: &Item, w: f32, names: &HashSet<String>) -> bool {
    let mut clicked = false;
    let preview_h = (w * 0.62).round();
    let (outer, _) = ui.allocate_exact_size(vec2(w, preview_h + 150.0), Sense::hover());
    if !ui.is_rect_visible(outer) {
        return false;
    }
    ui.painter().rect_filled(outer, t.radius, t.field);
    ui.painter().rect_stroke(outer, t.radius, Stroke::new(1.0, t.field_border), StrokeKind::Inside);
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(outer).layout(egui::Layout::top_down(egui::Align::LEFT)));
    let ui = &mut child;
    // Preview.
    let pr = egui::Rect::from_min_size(outer.min, vec2(w, preview_h));
    let tex = item.preview.as_ref().and_then(|url| match s.previews.get(url) {
        Some(Preview::Ready(t)) => Some(t.clone()),
        Some(Preview::Loading(slot)) => {
            let done = slot.lock().ok().and_then(|mut o| o.take());
            match done {
                Some(Some(img)) => {
                    let tex = ui.ctx().load_texture(format!("browser-{url}"), img, egui::TextureOptions::LINEAR);
                    s.previews.insert(url.clone(), Preview::Ready(tex.clone()));
                    Some(tex)
                }
                Some(None) => {
                    s.previews.insert(url.clone(), Preview::Failed);
                    None
                }
                None => None,
            }
        }
        Some(Preview::Failed) => None,
        None => {
            let in_flight = s.previews.values().filter(|p| matches!(p, Preview::Loading(_))).count();
            if in_flight < 6 {
                s.previews.insert(url.clone(), Preview::Loading(load_preview(url.clone())));
            }
            None
        }
    });
    let rounding = egui::CornerRadius { nw: t.radius as u8, ne: t.radius as u8, sw: 0, se: 0 };
    match tex {
        Some(tex) => {
            // Cover: crop the texture to the preview's aspect.
            let [tw, th] = tex.size();
            let (tw, th) = (tw as f32, th as f32);
            let target = pr.width() / pr.height();
            let uv = if tw / th > target {
                let u = target * th / tw;
                egui::Rect::from_min_max(egui::pos2((1.0 - u) / 2.0, 0.0), egui::pos2((1.0 + u) / 2.0, 1.0))
            } else {
                let v = tw / target / th;
                egui::Rect::from_min_max(egui::pos2(0.0, (1.0 - v) / 2.0), egui::pos2(1.0, (1.0 + v) / 2.0))
            };
            egui::Image::from_texture(&tex).uv(uv).corner_radius(rounding).paint_at(ui, pr);
        }
        None => {
            ui.painter().rect_filled(pr, rounding, t.canvas);
            let icon = if item.kind == Some(Kind::Lora) {
                "bookmark-plus"
            } else if item.template.is_some() {
                "workflow"
            } else {
                "package"
            };
            crate::icons::paint(ui, egui::Rect::from_center_size(pr.center() - vec2(0.0, 8.0), vec2(34.0, 34.0)), icon, 30.0, t.text_faint);
            let fam = item
                .family
                .as_deref()
                .and_then(|f| li_ai::family::registry().family(f))
                .map(|f| f.label.clone())
                .unwrap_or_else(|| if item.template.is_some() { item.title.clone() } else { tl!("Unknown family").into() });
            let mut job = egui::text::LayoutJob::simple_singleline(fam, crate::theme::medium(11.0), t.text_faint);
            job.wrap = egui::text::TextWrapping::truncate_at_width((pr.width() - 16.0).max(0.0));
            let galley = ui.painter().layout_job(job);
            ui.painter().galley(pr.center() + vec2(-galley.size().x / 2.0, 22.0 - galley.size().y / 2.0), galley, t.text_faint);
        }
    }
    // Source and access badges over the preview.
    let mut badge_ui = ui.new_child(egui::UiBuilder::new().max_rect(pr.shrink(8.0)).layout(egui::Layout::left_to_right(egui::Align::Min)));
    if let Some(src) = item.source {
        widgets::overlay_chip(&mut badge_ui, li_i18n::t(src.label()), widgets::Tone::Neutral);
    } else if item.template.is_some() {
        widgets::overlay_chip(&mut badge_ui, tl!("ComfyUI template"), widgets::Tone::Neutral);
    }
    if item.gated {
        widgets::overlay_chip(&mut badge_ui, tl!("Licence gate"), widgets::Tone::Warning)
            .on_hover_text(tl!("Accept the licence on the publisher's page first"));
    }
    if let Some(need) = needs_newer_comfy(item) {
        widgets::overlay_chip(&mut badge_ui, &li_i18n::fmt(tl!("Needs ComfyUI {need}"), &[("need", &need)]), widgets::Tone::Warning).on_hover_text(
            li_i18n::fmt(
                tl!("This template was made for ComfyUI {need} or newer; this server runs {have}."),
                &[("need", &need), ("have", &crate::ai_ui::status().comfyui_version)],
            ),
        );
    }
    // Text.
    let body = egui::Rect::from_min_max(egui::pos2(outer.left() + 10.0, pr.bottom() + 8.0), outer.max - vec2(10.0, 8.0));
    let mut b = ui.new_child(egui::UiBuilder::new().max_rect(body).layout(egui::Layout::top_down(egui::Align::LEFT)));
    let ui = &mut b;
    ui.spacing_mut().item_spacing.y = 3.0;
    ui.add(egui::Label::new(RichText::new(&item.title).strong().size(13.0).color(t.text)).truncate()).on_hover_text(&item.title);
    let by = match (item.author.is_empty(), item.downloads) {
        (false, 0) => item.author.clone(),
        (false, d) => li_i18n::fmt(tl!("{author} · {n} downloads"), &[("author", &item.author), ("n", &compact(d))]),
        (true, 0) => item.description.clone(),
        (true, d) => li_i18n::fmt(tl!("{n} downloads"), &[("n", &compact(d))]),
    };
    let r = ui.add(egui::Label::new(RichText::new(&by).size(11.0).color(t.text_faint)).truncate());
    if !item.description.is_empty() {
        r.on_hover_text(&item.description);
    }
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        for tg in capability_tags(item).iter().take(5) {
            widgets::tag_chip(ui, tg);
        }
    });
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        let meta = |ui: &mut egui::Ui, icon: &str, text: String, tip: &str| {
            let (r, _) = ui.allocate_exact_size(vec2(14.0, 14.0), Sense::hover());
            crate::icons::paint(ui, r, icon, 12.0, t.text_faint);
            ui.label(RichText::new(text).size(11.0).color(t.text_dim)).on_hover_text(tip);
        };
        if let Some(b) = item.total_bytes() {
            meta(ui, "download", human(b), tl!("Download size"));
        }
        // A template's size is all its files together: its family's guide is closer.
        let fam_vram = item.family.as_deref().and_then(|f| li_ai::family::registry().family(f)).map(|f| f.vram_gb as f32).filter(|v| *v > 0.0);
        let vram = if item.template.is_some() { fam_vram } else { item.vram_gb().or(fam_vram) };
        if let Some(v) = vram.filter(|_| item.kind != Some(Kind::Lora)) {
            let fits = gpu_gb().is_none_or(|g| v <= g);
            meta(
                ui,
                "cpu",
                format!("~{v:.0} GB"),
                if fits { tl!("Rough GPU memory to run it") } else { tl!("Rough GPU memory to run it: more than your GPU has") },
            );
        }
    });
    let licence = match (item.license.is_empty(), item.commercial) {
        (true, _) => tl!("Licence: see its page").to_owned(),
        (false, Some(false)) => li_i18n::fmt(tl!("{licence} · non-commercial"), &[("licence", &item.license)]),
        (false, _) => item.license.clone(),
    };
    ui.add(egui::Label::new(RichText::new(licence).size(11.0).color(t.text_faint)).truncate());
    // Actions at the bottom.
    let actions = egui::Rect::from_min_max(egui::pos2(body.left(), body.bottom() - 24.0), body.max);
    let mut a = ui.new_child(egui::UiBuilder::new().max_rect(actions).layout(egui::Layout::left_to_right(egui::Align::Center)));
    let ui = &mut a;
    let run = s.installs.get(&item.id).cloned();
    let installed = is_installed(item, names);
    let setup = run.as_ref().and_then(|r| r.lock().ok().and_then(|r| r.setup.clone()));
    match run.as_ref().and_then(|r| r.lock().ok().map(|r| (r.finished, r.error.clone(), r.done, r.total, r.note.clone()))) {
        Some((false, _, done, total, _)) => {
            ui.add(
                egui::ProgressBar::new(done as f32 / total.max(1) as f32)
                    .desired_width(body.width() - 34.0)
                    .text(li_i18n::fmt(tl!("{done} of {total}"), &[("done", &human(done)), ("total", &human(total))])),
            );
            if crate::icons::button(ui, "x", 22.0, false, tl!("Stop")).clicked()
                && let Some(r) = &run
                && let Ok(r) = r.lock()
            {
                r.ctl.cancel();
            }
            ui.ctx().request_repaint_after(Duration::from_millis(300));
        }
        Some((true, Some(err), ..)) => {
            ui.label(RichText::new(tl!("Install failed")).size(11.0).color(t.danger)).on_hover_text(err);
            if widgets::secondary_button(ui, tl!("Retry"), 0.0).clicked() {
                s.installs.remove(&item.id);
                clicked = true;
            }
        }
        Some((true, None, _, _, note)) => {
            let r = ui.label(RichText::new(tl!("Installed")).size(11.5).color(t.accent));
            if let Some(n) = note {
                r.on_hover_text(n);
            }
            if let Some(key) = setup
                && widgets::secondary_button(ui, tl!("Set up and test"), 0.0).clicked()
            {
                crate::model_setup::open(Some(&key));
            }
        }
        None if installed => {
            ui.label(RichText::new(tl!("Installed")).size(11.5).color(t.accent));
            if let Some(target) = removal_for(item) {
                let tip = if li_ai::trash::available() { tl!("Remove (moves the files to the Trash)") } else { tl!("Remove (deletes the files)") };
                let r = crate::icons::button(ui, "trash", 22.0, false, tip);
                r.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, tl!("Remove")));
                if r.clicked() {
                    crate::ai_ui::ask_remove(item.title.clone(), target);
                }
            }
        }
        None if AiSettings::load().is_remote() && item.source != Some(Source::Installed) => {
            ui.label(RichText::new(tl!("Install on server")).size(11.5).color(t.text_dim));
        }
        None if item.source != Some(Source::Installed)
            && widgets::primary_button(ui, if item.template.is_some() { tl!("Get Template") } else { tl!("Install") }, 0.0).clicked() =>
        {
            clicked = true;
        }
        None => {}
    }
    if !item.page_url.is_empty() {
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if crate::icons::button(ui, "external-link", 22.0, false, tl!("Open its page")).clicked() {
                ui.ctx().open_url(egui::OpenUrl::new_tab(&item.page_url));
            }
        });
    }
    clicked
}

fn compact(n: u64) -> String {
    match n {
        0..1000 => n.to_string(),
        1000..1_000_000 => format!("{:.1}k", n as f64 / 1e3).replace(".0k", "k"),
        _ => format!("{:.1}M", n as f64 / 1e6).replace(".0M", "M"),
    }
}

fn footer_ui(ui: &mut egui::Ui, s: &mut State, t: &Tokens) {
    if let Some(r) = s.profiles.as_ref().and_then(|p| p.lock().ok().and_then(|mut o| o.take())) {
        s.profiles = None;
        s.profiles_msg = match r {
            Ok(v) if v.is_empty() => tl!("Model profiles are up to date").into(),
            Ok(v) => profile_report(&v),
            Err(e) => li_i18n::fmt(tl!("Couldn't update profiles: {error}"), &[("error", &e)]),
        };
    }
    let r = ui.available_rect_before_wrap();
    ui.painter().hline(r.x_range(), r.top(), Stroke::new(1.0, t.separator));
    egui::Frame::new().inner_margin(egui::Margin::symmetric(14, 6)).show(ui, |ui| {
        ui.horizontal(|ui| {
            let note = if matches!(&s.nav, Nav::Family(f) if f == "restoration") {
                tl!("Restoration installs only vetted official weights with pinned size and SHA-256.")
            } else {
                tl!("Only .safetensors and .gguf files with a published size and SHA-256 are installed.")
            };
            ui.label(RichText::new(note).size(11.0).color(t.text_faint));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let busy = s.profiles.is_some();
                if ui
                    .add_enabled(!busy, egui::Button::new(tl!("Update Model Profiles")))
                    .on_hover_text(tl!("Fetch the latest family profiles (new models, settings) from Local Image's repository"))
                    .clicked()
                {
                    s.profiles = Some(start_profile_update());
                    s.profiles_msg = tl!("Checking for profile updates…").into();
                }
                if !s.profiles_msg.is_empty() {
                    ui.label(RichText::new(&s.profiles_msg).size(11.0).color(t.text_dim));
                }
            });
        });
    });
}

#[cfg(test)]
mod tests;
