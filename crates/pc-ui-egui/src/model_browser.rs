//! local-image: the Model Browser window. Models and LoRAs from Hugging Face, Civitai, ComfyUI's
//! official templates and the ComfyUI-Manager community list, laid out like SwarmUI's and
//! InvokeAI's model managers with Civitai/Hugging Face-style cards: a navigation column (New,
//! Trending, Installed, Works on my GPU, then each family), search and source filters, and a grid
//! of cards showing capabilities, download size, rough GPU memory and licence.
//!
//! Nothing goes online until the window is open; every catalogue is cached so the window still
//! works offline. Installing shows the licence and the exact files first, then downloads them
//! verified (size and SHA-256) into ComfyUI's folders and refreshes the engine. A template for a
//! family Local Image doesn't know yet becomes a custom workflow with basic controls.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use egui::{Color32, RichText, Sense, Stroke, StrokeKind, TextureHandle, vec2};
use li_ai::browser::{self, Cache, Config, HttpNet, Item, Kind, Net, Plan, Query, Source, View};
use li_ai::comfy::JobControl;
use li_ai::settings::AiSettings;
use serde_json::Value;

use crate::PhotocraftApp;
use crate::theme::Tokens;
use crate::widgets;

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

#[derive(Default)]
struct InstallRun {
    done: u64,
    total: u64,
    file: String,
    finished: bool,
    error: Option<String>,
    ctl: JobControl,
    note: Option<String>,
}

struct Confirm {
    item: Item,
    plan: Slot<Result<(Plan, Option<Value>), String>>,
    accept: bool,
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
    profiles: Option<Slot<Result<Vec<String>, String>>>,
    profiles_msg: String,
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
        }
    }
}

thread_local! { static STATE: RefCell<State> = RefCell::new(State::default()); }

fn with<R>(f: impl FnOnce(&mut State) -> R) -> R {
    STATE.with(|s| f(&mut s.borrow_mut()))
}

const SOURCES: [(Source, &str); 4] =
    [(Source::HuggingFace, "Hugging Face"), (Source::Civitai, "Civitai"), (Source::Template, "Templates"), (Source::Community, "Community")];

/// Opens the browser window.
pub fn open() {
    with(|s| s.open = true);
}

/// Opens it on the LoRAs that fit `family`.
pub fn open_loras(family: &str) {
    with(|s| {
        s.open = true;
        s.kind = Kind::Lora;
        s.nav = Nav::Family(family.to_owned());
    });
}

pub fn is_open() -> bool {
    with(|s| s.open)
}

/// The sources' configuration: the AI settings' tokens, and the connected ComfyUI's own
/// (version-matched) copy of the official templates.
fn config() -> Config {
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
        120..7200 => crate::i18n::fmt(tl!("{n} min ago"), &[("n", &(secs / 60).to_string())]),
        7200..172800 => crate::i18n::fmt(tl!("{n} h ago"), &[("n", &(secs / 3600).to_string())]),
        _ => crate::i18n::fmt(tl!("{n} days ago"), &[("n", &(secs / 86400).to_string())]),
    }
}

/// Fetches the query's catalogues on a worker thread (cache fallback per source).
fn start_fetch(q: Query) -> Slot<Fetched> {
    let slot: Slot<Fetched> = Arc::new(Mutex::new(None));
    let out = slot.clone();
    // Notes are shown in the window: the worker draws them in the UI's language.
    let lang = crate::i18n::current();
    let _ = std::thread::Builder::new().name("model-browser".into()).spawn(move || {
        crate::i18n::set_current(lang);
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
                        notes.push(crate::i18n::fmt(tl!("{source}: offline, showing results from {when}"), &[("source", src.label()), ("when", &when)]));
                    }
                    match serde_json::from_slice::<Value>(&body) {
                        Ok(v) => bodies.push((src, v)),
                        Err(_) => notes.push(crate::i18n::fmt(tl!("{source}: the catalogue couldn't be read"), &[("source", src.label())])),
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
            .filter(|m| !m.tool && li_ai::catalog::presets_for(m.id).any(|p| st.ready(m.id, &p.variant).is_ok()))
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

/// File names ComfyUI already has (main models and LoRAs), by base name.
fn installed_names() -> HashSet<String> {
    let base = |n: &str| n.rsplit('/').next().unwrap_or(n).to_ascii_lowercase();
    let mut out: HashSet<String> = li_ai::catalog::catalog().models.iter().filter_map(|m| m.id.key().split_once(':').map(|(_, n)| base(n))).collect();
    out.extend(li_ai::inventory::loras().iter().map(|l| base(&l.name)));
    out
}

fn is_installed(item: &Item, names: &HashSet<String>) -> bool {
    let weights: Vec<_> = item.files.iter().filter(|f| f.name.ends_with(".safetensors") || f.name.ends_with(".gguf")).collect();
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

/// Resolves what installing `item` downloads (and, for a template, its workflow).
fn start_plan(item: Item) -> Slot<Result<(Plan, Option<Value>), String>> {
    let slot = Arc::new(Mutex::new(None));
    let out = slot.clone();
    let lang = crate::i18n::current();
    let _ = std::thread::Builder::new().name("model-plan".into()).spawn(move || {
        crate::i18n::set_current(lang);
        let r = (|| -> anyhow::Result<(Plan, Option<Value>)> {
            let cfg = config();
            let net = HttpNet::new(cfg.clone());
            let info = li_ai::service()
                .client
                .object_info()
                .map_err(|e| {
                    anyhow::anyhow!(
                        "{}",
                        crate::i18n::fmt(
                            tl!("Start the AI engine first: Local Image checks what ComfyUI already has. ({error})"),
                            &[("error", &format!("{e:#}"))]
                        )
                    )
                })?;
            let reg = li_ai::family::registry();
            if let Some(name) = &item.template {
                let (ui, files) = browser::fetch_template(&cfg, &net, name)?;
                let mut plan = browser::plan(&item, &files, None, &info, &cfg, &net)?;
                if plan.license.is_empty() {
                    plan.license =
                        item.family.as_deref().and_then(|f| reg.family(f)).map(|f| f.license.clone()).unwrap_or_else(|| tl!("See each model's page").into());
                }
                return Ok((plan, Some(ui)));
            }
            let fam = item.family.as_deref().and_then(|f| reg.family(f));
            Ok((browser::plan(&item, &item.files, fam, &info, &cfg, &net)?, None))
        })()
        .map_err(|e| format!("{e:#}"));
        if let Ok(mut o) = out.lock() {
            *o = Some(r);
        }
        request_repaint();
    });
    slot
}

fn start_install(item: Item, plan: Plan, template: Option<Value>) -> Arc<Mutex<InstallRun>> {
    let run = Arc::new(Mutex::new(InstallRun { total: plan.total(), ..Default::default() }));
    let r = run.clone();
    let lang = crate::i18n::current();
    let _ = std::thread::Builder::new().name("model-install".into()).spawn(move || {
        crate::i18n::set_current(lang);
        let cfg = config();
        let dir = AiSettings::load().model_dir();
        let ctl = r.lock().map(|x| x.ctl.clone()).unwrap_or_default();
        let progress = |done: u64, total: u64, file: &str| {
            if let Ok(mut x) = r.lock() {
                x.done = done;
                x.total = total;
                x.file = file.to_owned();
            }
            request_repaint();
        };
        let result = browser::install(&plan, &dir, &cfg, &ctl, &progress).and_then(|()| {
            // A template for a family Local Image doesn't know runs as a custom workflow.
            if let (Some(ui), None) = (&template, &item.family) {
                let info = li_ai::service().client.object_info()?;
                let mut w = li_ai::custom::from_template(&item.title, ui, &info)?;
                w.family = item.family.clone();
                li_ai::custom::save(&w, &li_ai::custom::dir())?;
                return Ok(Some(crate::i18n::fmt(tl!("Added “{name}” to the model list (basic controls)"), &[("name", &item.title)])));
            }
            Ok(None)
        });
        if let Ok(mut x) = r.lock() {
            x.finished = true;
            match result {
                Ok(note) => x.note = note,
                Err(e) => x.error = Some(first_line(&format!("{e:#}"))),
            }
        }
        crate::generate_ui::reload_customs();
        crate::ai_ui::refresh();
        request_repaint();
    });
    run
}

fn start_profile_update() -> Slot<Result<Vec<String>, String>> {
    let slot = Arc::new(Mutex::new(None));
    let out = slot.clone();
    let _ = std::thread::Builder::new().name("family-update".into()).spawn(move || {
        let net = HttpNet::new(config());
        let base = std::env::var("LOCAL_IMAGE_FAMILIES_BASE").unwrap_or_else(|_| li_ai::family::REMOTE_BASE.to_owned());
        let r = li_ai::family::update_from(&|u| net.get(u), &base, &li_ai::family::cache_dir()).map_err(|e| first_line(&format!("{e:#}")));
        if matches!(&r, Ok(v) if !v.is_empty()) {
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

fn human(n: u64) -> String {
    li_ai::download::human_bytes(n)
}

/// A small pill.
fn pill(ui: &mut egui::Ui, text: &str, fill: Color32, ink: Color32) -> egui::Response {
    let galley = ui.painter().layout_no_wrap(text.to_owned(), egui::FontId::proportional(10.5), ink);
    let (r, resp) = ui.allocate_exact_size(vec2(galley.size().x + 10.0, 16.0), Sense::hover());
    ui.painter().rect_filled(r, 8.0, fill);
    ui.painter().galley(r.center() - galley.size() / 2.0, galley, ink);
    resp
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

fn nav_row(ui: &mut egui::Ui, icon: &str, label: &str, on: bool, t: &Tokens) -> bool {
    nav_row_with(ui, icon, label, "", on, t)
}

fn nav_row_with(ui: &mut egui::Ui, icon: &str, label: &str, trailing: &str, on: bool, t: &Tokens) -> bool {
    let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 26.0), Sense::click());
    if on {
        ui.painter().rect_filled(r, t.radius_sm, t.row_selected);
    } else if resp.hovered() {
        ui.painter().rect_filled(r, t.radius_sm, t.hover);
    }
    let ink = if on { t.text } else { t.text_dim };
    let mut x = r.left() + 8.0;
    if !icon.is_empty() {
        crate::icons::paint(ui, egui::Rect::from_min_size(egui::pos2(x, r.top() + 5.0), vec2(16.0, 16.0)), icon, 14.0, ink);
        x += 22.0;
    }
    ui.painter().text(egui::pos2(x, r.center().y), egui::Align2::LEFT_CENTER, label, egui::FontId::proportional(12.5), ink);
    if !trailing.is_empty() {
        ui.painter().text(egui::pos2(r.right() - 8.0, r.center().y), egui::Align2::RIGHT_CENTER, trailing, egui::FontId::proportional(11.0), t.text_faint);
    }
    resp.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::SelectableLabel, true, on, label));
    resp.clicked()
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
                tags.push(crate::i18n::fmt(tl!("{n} refs"), &[("n", &c.references.to_string())]));
            }
        }
    } else if item.basic_controls() {
        tags.push(tl!("Basic controls").into());
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
    egui::Window::new(tl!("Model Browser"))
        .collapsible(false)
        .resizable(false)
        .fixed_size(size)
        .frame(egui::Frame::window(&ctx.global_style()).fill(t.card).stroke(Stroke::new(1.0, t.card_border)).inner_margin(egui::Margin::same(0)))
        .open(&mut open)
        .anchor(egui::Align2::CENTER_CENTER, vec2(0.0, 0.0))
        .show(ctx, |ui| with(|s| body(ui, s, &t)));
    confirm_window(ctx, &t);
    if !open {
        with(|s| {
            s.open = false;
            s.confirm = None;
        });
    }
    let _ = app;
}

fn body(ui: &mut egui::Ui, s: &mut State, t: &Tokens) {
    // Start or collect the catalogue fetch.
    let key = (s.nav.clone(), s.kind, s.search.trim().to_owned(), s.sources);
    if s.loaded.as_ref() != Some(&key) && s.fetch.is_none() {
        s.loaded = Some(key);
        if s.nav == Nav::Installed {
            s.items = installed_items(s.kind);
            s.items.retain(|i| s.search.trim().is_empty() || i.title.to_lowercase().contains(&s.search.trim().to_lowercase()));
            s.notes.clear();
        } else {
            s.fetch = Some(start_fetch(query(s)));
        }
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
                    (Nav::New, "star", crate::i18n::tr_ctx(crate::i18n::current(), "ai", "New"), ""),
                    (Nav::Trending, "trending-up", tl!("Trending"), ""),
                    (Nav::Installed, "hard-drive", tl!("Installed"), ""),
                    (Nav::FitsGpu, "cpu", tl!("Works on my GPU"), gpu_text.as_str()),
                ] {
                    if nav_row_with(ui, icon, label, trailing, s.nav == nav, t) {
                        s.nav = nav;
                    }
                }
                ui.add_space(10.0);
                widgets::section_label(ui, tl!("FAMILIES"));
                egui::ScrollArea::vertical().id_salt("browser-families").auto_shrink([false, false]).show(ui, |ui| {
                    let reg = li_ai::family::registry();
                    let mut group = "";
                    for f in reg.families.iter().filter(|f| f.models.iter().any(|m| !m.tool) || f.base.is_some() || f.models.is_empty()) {
                        if f.id == "seedvr2" {
                            continue;
                        }
                        if f.group != group {
                            group = &f.group;
                            ui.add_space(4.0);
                            ui.label(RichText::new(&f.group).size(10.5).color(t.text_faint));
                        }
                        let on = s.nav == Nav::Family(f.id.clone());
                        if nav_row(ui, "", &f.label, on, t) {
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
                ui.horizontal(|ui| {
                    let hint = if s.kind == Kind::Lora { tl!("Search LoRAs") } else { tl!("Search models") };
                    let resp = ui.add(egui::TextEdit::singleline(&mut s.search).hint_text(hint).desired_width(240.0));
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
                    ui.label(RichText::new(crate::i18n::fmt(count, &[("n", &s.items.len().to_string())])).color(t.text_faint));
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
    let names = installed_names();
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
        s.confirm = Some(Confirm { plan: start_plan(item.clone()), item, accept: false });
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
            let fam =
                item.family.as_deref().and_then(|f| li_ai::family::registry().family(f)).map(|f| f.label.clone()).unwrap_or_else(|| tl!("Unknown family").into());
            ui.painter().text(pr.center() + vec2(0.0, 22.0), egui::Align2::CENTER_CENTER, fam, egui::FontId::proportional(11.0), t.text_faint);
        }
    }
    // Source and access badges over the preview.
    let mut badge_ui = ui.new_child(egui::UiBuilder::new().max_rect(pr.shrink(8.0)).layout(egui::Layout::left_to_right(egui::Align::Min)));
    let dark = Color32::from_black_alpha(170);
    if let Some(src) = item.source {
        pill(&mut badge_ui, src.label(), dark, Color32::WHITE);
    }
    if item.gated {
        pill(&mut badge_ui, tl!("Licence gate"), dark, Color32::from_rgb(255, 200, 90)).on_hover_text(tl!("Accept the licence on the publisher's page first"));
    }
    if item.basic_controls() {
        pill(&mut badge_ui, tl!("Basic controls"), dark, Color32::WHITE)
            .on_hover_text(tl!("Local Image doesn't know this family yet: it runs through the official template with prompt, seed and size only"));
    }
    // Text.
    let body = egui::Rect::from_min_max(egui::pos2(outer.left() + 10.0, pr.bottom() + 8.0), outer.max - vec2(10.0, 8.0));
    let mut b = ui.new_child(egui::UiBuilder::new().max_rect(body).layout(egui::Layout::top_down(egui::Align::LEFT)));
    let ui = &mut b;
    ui.spacing_mut().item_spacing.y = 3.0;
    ui.add(egui::Label::new(RichText::new(&item.title).strong().size(13.0).color(t.text)).truncate()).on_hover_text(&item.title);
    let by = match (item.author.is_empty(), item.downloads) {
        (false, 0) => item.author.clone(),
        (false, d) => crate::i18n::fmt(tl!("{author} · {n} downloads"), &[("author", &item.author), ("n", &compact(d))]),
        (true, 0) => item.description.clone(),
        (true, d) => crate::i18n::fmt(tl!("{n} downloads"), &[("n", &compact(d))]),
    };
    let r = ui.add(egui::Label::new(RichText::new(&by).size(11.0).color(t.text_faint)).truncate());
    if !item.description.is_empty() {
        r.on_hover_text(&item.description);
    }
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        for tg in capability_tags(item).iter().take(5) {
            pill(ui, tg, t.card, t.text_dim);
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
            meta(ui, "cpu", format!("~{v:.0} GB"), if fits { tl!("Rough GPU memory to run it") } else { tl!("Rough GPU memory to run it: more than your GPU has") });
        }
    });
    let licence = match (item.license.is_empty(), item.commercial) {
        (true, _) => tl!("Licence: see its page").to_owned(),
        (false, Some(false)) => crate::i18n::fmt(tl!("{licence} · non-commercial"), &[("licence", &item.license)]),
        (false, _) => item.license.clone(),
    };
    ui.add(egui::Label::new(RichText::new(licence).size(11.0).color(t.text_faint)).truncate());
    // Actions at the bottom.
    let actions = egui::Rect::from_min_max(egui::pos2(body.left(), body.bottom() - 24.0), body.max);
    let mut a = ui.new_child(egui::UiBuilder::new().max_rect(actions).layout(egui::Layout::left_to_right(egui::Align::Center)));
    let ui = &mut a;
    let run = s.installs.get(&item.id).cloned();
    let installed = is_installed(item, names);
    match run.as_ref().and_then(|r| r.lock().ok().map(|r| (r.finished, r.error.clone(), r.done, r.total, r.note.clone()))) {
        Some((false, _, done, total, _)) => {
            ui.add(
                egui::ProgressBar::new(done as f32 / total.max(1) as f32)
                    .desired_width(body.width() - 34.0)
                    .text(crate::i18n::fmt(tl!("{done} of {total}"), &[("done", &human(done)), ("total", &human(total))])),
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
            let r = ui.label(RichText::new(tl!("Installed")).size(11.5).color(Color32::from_rgb(70, 190, 110)));
            if let Some(n) = note {
                r.on_hover_text(n);
            }
        }
        None if installed => {
            ui.label(RichText::new(tl!("Installed")).size(11.5).color(Color32::from_rgb(70, 190, 110)));
        }
        None if item.source != Some(Source::Installed) => {
            if widgets::primary_button(ui, if item.template.is_some() { tl!("Get Template") } else { tl!("Install") }, 0.0).clicked() {
                clicked = true;
            }
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
            Ok(v) if v.len() == 1 => crate::i18n::fmt(tl!("Updated 1 model profile: {names}"), &[("names", &v.join(", "))]),
            Ok(v) => crate::i18n::fmt(tl!("Updated {n} model profiles: {names}"), &[("n", &v.len().to_string()), ("names", &v.join(", "))]),
            Err(e) => crate::i18n::fmt(tl!("Couldn't update profiles: {error}"), &[("error", &e)]),
        };
    }
    let r = ui.available_rect_before_wrap();
    ui.painter().hline(r.x_range(), r.top(), Stroke::new(1.0, t.separator));
    egui::Frame::new().inner_margin(egui::Margin::symmetric(14, 6)).show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.label(RichText::new(tl!("Only .safetensors and .gguf files with a published size and SHA-256 are installed.")).size(11.0).color(t.text_faint));
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

/// The install confirmation: licence first, then the exact files.
fn confirm_window(ctx: &egui::Context, t: &Tokens) {
    let Some((item, plan, mut accept)) = with(|s| s.confirm.as_ref().map(|c| (c.item.clone(), c.plan.lock().ok().and_then(|p| p.clone()), c.accept))) else {
        return;
    };
    let mut close = false;
    let mut go: Option<(Plan, Option<Value>)> = None;
    use widgets::{ButtonRole, DialogButton};
    let buttons = |ui: &mut egui::Ui, list: &[DialogButton]| -> Option<ButtonRole> {
        ui.add_space(10.0);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| widgets::dialog_buttons(ui, list)).inner
    };
    // Like every dialog: no dimming behind it.
    egui::Modal::new(egui::Id::new("browser-confirm")).backdrop_color(Color32::TRANSPARENT).show(ctx, |ui| {
        ui.set_width(500.0);
        ui.label(RichText::new(crate::i18n::fmt(tl!("Install {name}"), &[("name", &item.title)])).size(15.0).strong().color(t.text));
        ui.add_space(6.0);
        match &plan {
            None => {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(RichText::new(tl!("Checking the files’ sizes and checksums…")).color(t.text_dim));
                });
                ui.ctx().request_repaint_after(Duration::from_millis(200));
                if buttons(ui, &[DialogButton::new(ButtonRole::Cancel, tl!("Cancel"), 80.0)]).is_some() {
                    close = true;
                }
            }
            Some(Err(e)) => {
                ui.label(RichText::new(e).color(t.danger));
                let page = item.license_url.clone().or(Some(item.page_url.clone())).filter(|u| !u.is_empty());
                let mut list = vec![DialogButton::new(ButtonRole::Cancel, tl!("Close"), 80.0)];
                if page.is_some() {
                    list.insert(0, DialogButton::new(ButtonRole::Alternate, tl!("Open Its Page"), 0.0));
                }
                match buttons(ui, &list) {
                    Some(ButtonRole::Alternate) => {
                        if let Some(url) = page {
                            ui.ctx().open_url(egui::OpenUrl::new_tab(url));
                        }
                    }
                    Some(_) => close = true,
                    None => {}
                }
            }
            Some(Ok((p, template))) => {
                // Licence.
                egui::Frame::new().fill(t.field).corner_radius(t.radius_sm).inner_margin(egui::Margin::same(10)).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(RichText::new(tl!("Licence")).size(11.0).color(t.text_faint));
                    ui.label(RichText::new(if p.license.is_empty() { tl!("Not stated: check the model's page before use") } else { &p.license }).color(t.text));
                    if item.commercial == Some(false) {
                        ui.label(RichText::new(tl!("The author doesn't allow commercial use.")).color(t.warning));
                    }
                    if p.gated {
                        ui.label(RichText::new(tl!("The publisher asks you to accept its licence on its page first; add a Hugging Face token in Local AI settings.")).color(t.warning));
                    }
                    if let Some(url) = &p.license_url
                        && ui.link(tl!("Read the licence")).clicked()
                    {
                        ui.ctx().open_url(egui::OpenUrl::new_tab(url));
                    }
                });
                ui.add_space(8.0);
                if p.files.is_empty() {
                    ui.label(RichText::new(tl!("Everything it needs is already installed.")).color(t.text_dim));
                } else {
                    let size = human(p.total());
                    let what = if p.files.len() == 1 {
                        crate::i18n::fmt(tl!("1 file to download ({size}), verified by SHA-256:"), &[("size", &size)])
                    } else {
                        crate::i18n::fmt(tl!("{n} files to download ({size}), verified by SHA-256:"), &[("n", &p.files.len().to_string()), ("size", &size)])
                    };
                    ui.label(RichText::new(what).color(t.text_dim));
                    egui::ScrollArea::vertical().max_height(160.0).show(ui, |ui| {
                        for f in &p.files {
                            ui.horizontal(|ui| {
                                ui.label(RichText::new(&f.name).color(t.text));
                                ui.label(RichText::new(format!("→ {}", f.folder)).size(11.0).color(t.text_faint));
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| ui.label(RichText::new(human(f.bytes)).color(t.text_dim)));
                            });
                        }
                    });
                }
                if !p.present.is_empty() {
                    ui.label(RichText::new(crate::i18n::fmt(tl!("Already installed: {names}"), &[("names", &p.present.join(", "))])).size(11.0).color(t.text_faint));
                }
                let dir = AiSettings::load().model_dir();
                if let Some(free) = li_ai::download::free_space(&dir)
                    && free < p.total() + (256 << 20)
                {
                    ui.label(RichText::new(crate::i18n::fmt(tl!("Not enough space in {folder} ({free} free)."), &[("folder", &dir.display().to_string()), ("free", &human(free))])).color(t.danger));
                }
                if template.is_some() && item.family.is_none() {
                    ui.label(RichText::new(tl!("Local Image doesn't know this model family yet: it will appear in the model list as a custom workflow with basic controls (prompt, seed, size).")).size(11.5).color(t.text_dim));
                }
                ui.add_space(8.0);
                if !p.files.is_empty() {
                    widgets::checkbox(ui, &mut accept, tl!("I have read and accept the licence"));
                }
                let label = if p.files.is_empty() { tl!("Done") } else { tl!("Install") };
                match buttons(ui, &[DialogButton::new(ButtonRole::Default, label, 80.0).enabled(accept || p.files.is_empty()), DialogButton::new(ButtonRole::Cancel, tl!("Cancel"), 80.0)]) {
                    Some(ButtonRole::Default) => go = Some((p.clone(), template.clone())),
                    Some(_) => close = true,
                    None => {}
                }
            }
        }
    });
    with(|s| {
        if let Some(c) = s.confirm.as_mut() {
            c.accept = accept;
        }
        if let Some((p, template)) = go {
            if !p.files.is_empty() || template.is_some() {
                s.installs.insert(item.id.clone(), start_install(item.clone(), p, template));
            }
            s.confirm = None;
        } else if close {
            s.confirm = None;
        }
    });
}
