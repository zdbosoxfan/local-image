//! local-image: **Filter › Camera Raw Filter…** (`filter.develop`) in Compositing, edited in the
//! real Develop module.
//!
//! The editor can't show the Library's panels itself, so it asks the host: [`State::request`]
//! holds the layer's pixels (linear Rec.2020, the develop engine's working space), the current
//! settings and a name. The host opens them in Develop as a temporary photo, with a banner
//! "Camera Raw Filter · ‹layer› — Cancel / OK", and calls [`finish`] with the settings (OK) or
//! `None` (Cancel); OK runs `filter.develop` (or `develop.composite`) once: one history step.
//!
//! * On a pixel layer the menu first asks "Convert to Smart Object to keep it editable?" (Convert
//!   is the default), like Photoshop's Convert for Smart Filters.
//! * Double-clicking a `filter.develop` smart filter edits it ([`open_smart_filter`]).
//! * Going from Compositing to Develop with a layered document and no Develop layer to follow
//!   asks how to develop it ([`on_switch_to_develop`]): the composite live (the visible layers
//!   grouped into a smart object), a merged copy, or just switch. "Don't ask again" is remembered.
//! * Without the Library (`LOCAL_IMAGE_NO_LIBRARY`, or no host), the Camera Raw dialog opens in
//!   its develop mode instead: the controls it has, mapped onto the same develop settings.
//!
//! Control channel: `ui.menu.invoke {"id":"filter.develop","params":{"convert":true|false}}`
//! skips the prompt; `{"smartFilter":{"layer":id,"index":n}}` edits a smart filter;
//! `{"answer":"convert|destructive|cancel"}` answers the open prompt.

use photocraft_doc::{DocId, LayerContent, LayerId};
use photocraft_engine::develop_filter_cmds::{COMPOSITE, DEVELOP, develop_area};
use photocraft_io::develop_filter as dev;
use photocraft_raster::Surface;
use serde_json::{Value, json};

use crate::PhotocraftApp;

/// Where Compositing → Develop's choice is remembered (`prefs.dialogs`).
pub const CHOICE_KEY: &str = "develop.compositeChoice";

/// What the host shows in Develop.
#[derive(Clone, Debug)]
pub struct SessionRequest {
    /// The layer (or "Composite"), for the banner.
    pub name: String,
    pub width: usize,
    pub height: usize,
    /// Linear Rec.2020 pixels, row-major.
    pub rgb: Vec<[f32; 3]>,
    /// The settings to start from (`DevelopSettings` JSON).
    pub settings: Value,
    /// Develop panels/tools a filter can't use (see `photocraft_io::develop_filter::HIDDEN_SECTIONS`).
    pub hidden: Vec<String>,
}

/// What OK applies.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    /// A new filter on a layer (`convert`: pixel layer → smart object first).
    Layer { layer: LayerId, convert: bool },
    /// Edit smart filter `index` of smart object `layer`.
    SmartFilter { layer: LayerId, index: usize },
    /// Develop the visible layers: live smart object or a merged copy.
    Composite { live: bool },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Prompt {
    /// "Convert to Smart Object to keep it editable?"
    Convert { layer: LayerId },
    /// Compositing → Develop on a layered document.
    Composite { dont_ask: bool },
}

#[derive(Default)]
pub struct State {
    /// For the host to take (`Option::take`) and open in Develop.
    pub request: Option<SessionRequest>,
    /// The session waiting for [`finish`].
    pending: Option<(DocId, Action)>,
    prompt: Option<Prompt>,
    /// Switch to Develop once without the composite prompt ("Just switch").
    pub skip_composite_prompt: bool,
}

impl State {
    /// A session is open in Develop (the host shows the banner).
    pub fn in_session(&self) -> bool {
        self.pending.is_some()
    }

    /// A prompt is up (it owns the keyboard).
    pub fn prompting(&self) -> bool {
        self.prompt.is_some()
    }
}

fn status(app: &mut PhotocraftApp, msg: String, error: bool) {
    app.ui.status = msg;
    app.ui.status_error = error;
}

/// The menu / control-channel entry for `filter.develop`; `None` = run the engine command as is
/// (calls carrying settings, e.g. actions and scripts).
pub fn invoke(app: &mut PhotocraftApp, ctx: &egui::Context, params: &Value) -> Option<Result<Value, String>> {
    let fields = params.as_object()?;
    if let Some(a) = fields.get("answer").and_then(Value::as_str) {
        return Some(answer(app, ctx, a));
    }
    if let Some(sf) = fields.get("smartFilter") {
        let layer = sf.get("layer").and_then(Value::as_u64).map(LayerId);
        let index = sf.get("index").and_then(Value::as_u64).map(|i| i as usize);
        return Some(match (layer, index) {
            (Some(l), Some(i)) => open_smart_filter(app, ctx, l, i).map(|()| describe(app)),
            _ => Err("smartFilter needs {layer, index}".into()),
        });
    }
    if fields.keys().any(|k| !matches!(k.as_str(), "convert" | "layer")) {
        return None;
    }
    let convert = fields.get("convert").and_then(Value::as_bool);
    let layer = fields.get("layer").and_then(Value::as_u64).map(LayerId);
    Some(open(app, ctx, layer, convert).map(|()| describe(app)))
}

/// What's open, for the control channel.
pub fn describe(app: &PhotocraftApp) -> Value {
    let s = &app.develop_filter;
    json!({
        "prompt": s.prompt.map(|p| match p { Prompt::Convert { .. } => "convert", Prompt::Composite { .. } => "composite" }),
        "request": s.request.as_ref().map(|r| json!({"name": r.name, "size": [r.width, r.height]})),
        "session": s.pending.map(|(_, a)| format!("{a:?}")),
        "dialog": app.camera_raw.is_some(),
    })
}

/// Filter › Camera Raw Filter… on `layer` (default: the active layer). `convert`: `None` asks
/// on a pixel layer.
pub fn open(app: &mut PhotocraftApp, ctx: &egui::Context, layer: Option<LayerId>, convert: Option<bool>) -> Result<(), String> {
    photocraft_engine::commands::find(DEVELOP).map(|c| (c.enabled)(&app.session)).unwrap_or(Err("unknown command".into()))?;
    let st = app.session.active().ok_or("no document open")?;
    let id = layer.or(st.active_layer).ok_or("no active layer")?;
    let l = st.doc.layer(id).ok_or("no such layer")?;
    // A Develop layer develops its own photo in Develop (every tool, raw data).
    if app.host_modes
        && let Some(photo) = crate::develop_layer::layer_photo(l)
    {
        app.develop_request = Some(photo);
        return Ok(());
    }
    let raster = matches!(l.content, LayerContent::Raster(_));
    match (raster, convert) {
        (true, None) => {
            app.develop_filter.prompt = Some(Prompt::Convert { layer: id });
            Ok(())
        }
        (true, Some(c)) => start(app, ctx, Action::Layer { layer: id, convert: c }, dev::identity_json()),
        (false, _) => start(app, ctx, Action::Layer { layer: id, convert: false }, dev::identity_json()),
    }
}

/// Double-click on a `filter.develop` smart filter: edit its settings.
pub fn open_smart_filter(app: &mut PhotocraftApp, ctx: &egui::Context, layer: LayerId, index: usize) -> Result<(), String> {
    let st = app.session.active().ok_or("no document open")?;
    let Some(LayerContent::Smart(sm)) = st.doc.layer(layer).map(|l| &l.content) else { return Err("the layer is not a smart object".into()) };
    let f = sm.smart_filters.get(index).ok_or("no such smart filter")?;
    if f.command != DEVELOP {
        return Err("the smart filter is not a Camera Raw Filter".into());
    }
    let settings = dev::filter_settings(&f.params).and_then(|s| serde_json::to_value(s).map_err(|e| e.to_string()))?;
    start(app, ctx, Action::SmartFilter { layer, index }, settings)
}

/// The pixels the filter works on, the area, and a name for the banner.
fn source(app: &PhotocraftApp, action: Action) -> Result<(Surface, photocraft_geom::Rect, String), String> {
    let st = app.session.active().ok_or("no document open")?;
    let doc = &st.doc;
    let canvas = doc.bounds();
    let unavailable = || "the smart object's contents are unavailable (missing linked file?)".to_string();
    let (surf, name) = match action {
        Action::Composite { .. } => {
            let mut solo = (**doc).clone();
            solo.layers.retain(|l| l.visible);
            let buf = photocraft_compose::flatten(&solo);
            let fmt = doc.pixel_format();
            let fmt = photocraft_color::PixelFormat::new(fmt.mode, fmt.sample, true);
            let data: Vec<f32> = buf.px.iter().flat_map(|q| photocraft_raster::from_rgba(&fmt, *q)).collect();
            let mut s = Surface::new(fmt);
            s.write_region(canvas, &data);
            (s, tl!("Composite").to_string())
        }
        Action::Layer { layer, .. } => {
            let l = doc.layer(layer).ok_or("no such layer")?;
            let surf = match &l.content {
                LayerContent::Raster(s) => s.clone(),
                LayerContent::Smart(sm) => {
                    // a new filter goes on top of the existing ones
                    photocraft_engine::smart_cmds::render_below_filter(doc, sm, sm.smart_filters.len())
                        .ok()
                        .flatten()
                        .or_else(|| sm.cache.clone())
                        .ok_or_else(unavailable)?
                }
                other => return Err(format!("filters need a pixel layer (active layer is a {} layer)", other.kind_name())),
            };
            (surf, l.name.clone())
        }
        Action::SmartFilter { layer, index } => {
            let l = doc.layer(layer).ok_or("no such layer")?;
            let LayerContent::Smart(sm) = &l.content else { return Err("the layer is not a smart object".into()) };
            let surf = photocraft_engine::smart_cmds::render_below_filter(doc, sm, index).map_err(|e| e.to_string())?.ok_or_else(unavailable)?;
            (surf, l.name.clone())
        }
    };
    let area = develop_area(&surf, canvas);
    Ok((surf, area, name))
}

/// Opens the session: the host's Develop with the layer's pixels, or (no Library) the Camera Raw
/// dialog in develop mode.
pub fn start(app: &mut PhotocraftApp, ctx: &egui::Context, action: Action, settings: Value) -> Result<(), String> {
    let (surf, area, name) = source(app, action)?;
    let st = app.session.active().ok_or("no document open")?;
    let doc_id = st.doc.id;
    let profile = photocraft_engine::color_cmds::composite_profile(&st.doc);
    if area.is_empty() {
        return Err("the layer is empty".into());
    }
    if !app.host_modes {
        app.develop_filter.pending = Some((doc_id, action));
        let base = dev::filter_settings(&settings)?;
        let layer = match action {
            Action::Layer { layer, .. } | Action::SmartFilter { layer, .. } => layer,
            Action::Composite { .. } => st.active_layer.unwrap_or(LayerId(0)),
        };
        return crate::camera_raw_ui::open_develop(app, ctx, layer, surf, base, profile, matches!(action, Action::SmartFilter { .. }));
    }
    let (w, h) = (area.width() as usize, area.height() as usize);
    if w.checked_mul(h).is_none_or(|n| n > 400_000_000) {
        return Err("the layer is too large for the Camera Raw Filter".into());
    }
    let (rgb, _alpha) = dev::surface_to_working(&surf, area, &profile)?;
    app.develop_filter.pending = Some((doc_id, action));
    app.develop_filter.request =
        Some(SessionRequest { name, width: w, height: h, rgb, settings, hidden: dev::HIDDEN_SECTIONS.iter().map(|s| (*s).to_string()).collect() });
    Ok(())
}

/// The session ended: `Some(settings)` (OK) applies them as one history step, `None` (Cancel)
/// leaves the document alone.
pub fn finish(app: &mut PhotocraftApp, settings: Option<Value>) -> Result<Value, String> {
    let (doc, action) = app.develop_filter.pending.take().ok_or("no Camera Raw Filter session")?;
    app.develop_filter.request = None;
    let Some(settings) = settings else {
        status(app, tl!("Camera Raw Filter cancelled").into(), false);
        return Ok(json!({"cancelled": true}));
    };
    let index = app.session.documents().iter().position(|d| d.doc.id == doc).ok_or("the document was closed")?;
    app.session.set_active(index);
    let mut params = dev::filter_settings(&settings).and_then(|s| serde_json::to_value(s).map_err(|e| e.to_string()))?;
    let id = match action {
        Action::Layer { layer, convert } => {
            params["layer"] = json!(layer.0);
            params["convert"] = json!(convert);
            DEVELOP
        }
        Action::SmartFilter { layer, index } => {
            params["layer"] = json!(layer.0);
            params["index"] = json!(index);
            DEVELOP
        }
        Action::Composite { live } => {
            params["mode"] = json!(if live { "live" } else { "stamp" });
            COMPOSITE
        }
    };
    let r = app.run(id, params)?;
    status(app, tl!("Camera Raw Filter applied").into(), false);
    Ok(r)
}

/// The active document is a layered composite with no Develop layer to follow.
pub fn composite_candidate(app: &PhotocraftApp) -> bool {
    let Some(st) = app.session.active() else { return false };
    if !photocraft_engine::develop_layer_cmds::develop_layers(&st.doc).is_empty() {
        return false;
    }
    photocraft_engine::commands::find(COMPOSITE).is_some_and(|c| (c.enabled)(&app.session).is_ok())
}

/// The host is about to switch Compositing → Develop. `true`: go ahead (nothing to ask, the
/// remembered choice is "just switch", or "Just switch" was chosen); `false`: stay, a prompt or a
/// session is on its way.
pub fn on_switch_to_develop(app: &mut PhotocraftApp, ctx: &egui::Context) -> bool {
    if std::mem::take(&mut app.develop_filter.skip_composite_prompt) || !composite_candidate(app) {
        return true;
    }
    match app.session.prefs().dialogs.get(CHOICE_KEY).and_then(Value::as_str) {
        Some("switch") => true,
        Some(c @ ("live" | "stamp")) => {
            if let Err(e) = start(app, ctx, Action::Composite { live: c == "live" }, dev::identity_json()) {
                status(app, e, true);
            }
            false
        }
        _ => {
            app.develop_filter.prompt = Some(Prompt::Composite { dont_ask: false });
            false
        }
    }
}

/// Answers the open prompt (`convert|destructive|cancel`, or `live|stamp|switch|cancel`, with
/// `!` appended for "Don't ask again").
pub fn answer(app: &mut PhotocraftApp, ctx: &egui::Context, a: &str) -> Result<Value, String> {
    let prompt = app.develop_filter.prompt.take().ok_or("no Camera Raw Filter prompt is open")?;
    let (a, remember) = a.strip_suffix('!').map_or((a, false), |x| (x, true));
    match (prompt, a) {
        (_, "cancel") => {}
        (Prompt::Convert { layer }, "convert" | "destructive") => start(app, ctx, Action::Layer { layer, convert: a == "convert" }, dev::identity_json())?,
        (Prompt::Composite { dont_ask }, "live" | "stamp" | "switch") => {
            if dont_ask || remember {
                app.session.edit_prefs(|p| p.dialogs.insert(CHOICE_KEY.into(), json!(a)));
            }
            if a == "switch" {
                app.develop_filter.skip_composite_prompt = true;
                app.switch_module = Some(crate::Module::Develop);
            } else {
                start(app, ctx, Action::Composite { live: a == "live" }, dev::identity_json())?;
            }
        }
        (p, _) => {
            app.develop_filter.prompt = Some(p);
            return Err(format!("unknown answer `{a}`"));
        }
    }
    Ok(describe(app))
}

/// The prompts.
pub fn show(app: &mut PhotocraftApp, ctx: &egui::Context) {
    let Some(prompt) = app.develop_filter.prompt else { return };
    let mut chosen: Option<&'static str> = None;
    let mut dont_ask = matches!(prompt, Prompt::Composite { dont_ask: true });
    let modal = egui::Modal::new(egui::Id::new("develop-filter-prompt")).show(ctx, |ui| {
        ui.set_max_width(440.0);
        match prompt {
            Prompt::Convert { .. } => {
                ui.label(egui::RichText::new(tl!("Camera Raw Filter")).font(crate::theme::semibold(15.0)));
                ui.add_space(4.0);
                crate::widgets::hairline(ui);
                ui.add_space(8.0);
                ui.label(tl!("Convert to Smart Object to keep it editable?"));
                ui.add_space(4.0);
                ui.label(egui::RichText::new(tl!("As a Smart Object the filter stays editable: double-click it in the Layers panel to change it.")).size(11.5));
                ui.add_space(12.0);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.spacing_mut().item_spacing.x = 10.0;
                    if crate::widgets::primary_button(ui, tl!("Convert"), 84.0).clicked() {
                        chosen = Some("convert");
                    }
                    if crate::widgets::secondary_button(ui, tl!("Apply Destructively"), 84.0).clicked() {
                        chosen = Some("destructive");
                    }
                    if crate::widgets::secondary_button(ui, tl!("Cancel"), 84.0).clicked() {
                        chosen = Some("cancel");
                    }
                });
            }
            Prompt::Composite { .. } => {
                ui.label(egui::RichText::new(tl!("Develop a Layered Document")).font(crate::theme::semibold(15.0)));
                ui.add_space(4.0);
                crate::widgets::hairline(ui);
                ui.add_space(8.0);
                ui.label(tl!("This document has no Develop layer. How should Develop work on it?"));
                ui.add_space(10.0);
                for (key, title, hint) in [
                    (
                        "live",
                        tl!("Develop the composite (live)"),
                        tl!("The visible layers become a Smart Object (still editable inside) with a Camera Raw Filter."),
                    ),
                    ("stamp", tl!("Merge visible to a new layer and develop"), tl!("A merged copy on top, as a Smart Object with a Camera Raw Filter.")),
                    ("switch", tl!("Just switch"), tl!("Show the Library's Develop module.")),
                ] {
                    let primary = key == "live";
                    let r = if primary { crate::widgets::primary_button(ui, title, 300.0) } else { crate::widgets::secondary_button(ui, title, 300.0) };
                    if r.clicked() {
                        chosen = Some(key);
                    }
                    ui.label(egui::RichText::new(hint).size(11.0));
                    ui.add_space(6.0);
                }
                crate::widgets::checkbox(ui, &mut dont_ask, tl!("Don't ask again"));
                ui.add_space(8.0);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if crate::widgets::secondary_button(ui, tl!("Cancel"), 84.0).clicked() {
                        chosen = Some("cancel");
                    }
                });
            }
        }
    });
    if let Some(Prompt::Composite { dont_ask: d }) = &mut app.develop_filter.prompt {
        *d = dont_ask;
    }
    if chosen.is_none() && modal.should_close() {
        chosen = Some("cancel");
    }
    if chosen.is_none() && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Enter)) {
        chosen = Some(match prompt {
            Prompt::Convert { .. } => "convert",
            Prompt::Composite { .. } => "live",
        });
    }
    if let Some(a) = chosen
        && let Err(e) = answer(app, ctx, a)
    {
        status(app, e, true);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app() -> PhotocraftApp {
        let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
        app.run("file.new", json!({"width": 40, "height": 24})).unwrap();
        app.run("layer.new.layer", json!({"name": "Sky"})).unwrap();
        app.run("edit.fill", json!({"color": "#6688aa"})).unwrap();
        app.host_modes = true;
        app
    }

    fn pixel(app: &PhotocraftApp) -> [f32; 4] {
        let st = app.session.active().unwrap();
        let l = st.doc.layer(st.active_layer.unwrap()).unwrap();
        match &l.content {
            LayerContent::Smart(sm) => sm.cache.as_ref().unwrap().rgba(10, 10),
            _ => l.surface().unwrap().rgba(10, 10),
        }
    }

    fn steps(app: &PhotocraftApp) -> usize {
        app.session.active().unwrap().history.past_len()
    }

    #[test]
    fn a_pixel_layer_asks_then_the_host_gets_the_layer_and_ok_applies_one_step() {
        let mut app = app();
        invoke(&mut app, &egui::Context::default(), &json!({})).unwrap().unwrap();
        assert!(app.develop_filter.prompting());
        assert!(app.develop_filter.request.is_none());
        answer(&mut app, &egui::Context::default(), "convert").unwrap();
        let req = app.develop_filter.request.take().expect("a request for the host");
        assert_eq!((req.width, req.height, req.name.as_str()), (40, 24, "Sky"));
        assert_eq!(req.rgb.len(), 40 * 24);
        assert!(req.hidden.iter().any(|h| h == "calibration"));
        assert!(app.develop_filter.in_session());
        let before = (pixel(&app), steps(&app));
        // the host's Develop returns the edited settings
        let mut settings = req.settings.clone();
        settings["light"]["exposure"] = json!(1.0);
        finish(&mut app, Some(settings)).unwrap();
        assert!(!app.develop_filter.in_session());
        assert_eq!(steps(&app), before.1 + 1, "convert + filter: one history step");
        assert!(pixel(&app)[2] > before.0[2]);
        let st = app.session.active().unwrap();
        let LayerContent::Smart(sm) = &st.doc.layer(st.active_layer.unwrap()).unwrap().content else { panic!("smart object") };
        assert_eq!(sm.smart_filters[0].command, DEVELOP);
        // double-click → edit: the session starts from the stored settings and replaces them
        let layer = st.active_layer.unwrap();
        open_smart_filter(&mut app, &egui::Context::default(), layer, 0).unwrap();
        let req = app.develop_filter.request.take().unwrap();
        assert_eq!(req.settings["light"]["exposure"], 1.0);
        let mut s2 = req.settings;
        s2["light"]["exposure"] = json!(0.2);
        finish(&mut app, Some(s2)).unwrap();
        let st = app.session.active().unwrap();
        let LayerContent::Smart(sm) = &st.doc.layer(layer).unwrap().content else { panic!() };
        assert_eq!(sm.smart_filters.len(), 1);
        assert_eq!(sm.smart_filters[0].params["light"]["exposure"], 0.2);
    }

    #[test]
    fn cancel_leaves_the_document_alone_and_destructive_edits_pixels() {
        let mut app = app();
        let before = (pixel(&app), steps(&app));
        invoke(&mut app, &egui::Context::default(), &json!({"convert": false})).unwrap().unwrap();
        assert!(app.develop_filter.request.is_some());
        finish(&mut app, None).unwrap();
        assert_eq!((pixel(&app), steps(&app)), before);
        invoke(&mut app, &egui::Context::default(), &json!({"convert": false})).unwrap().unwrap();
        finish(&mut app, Some(json!({"light": {"exposure": -1.0}}))).unwrap();
        assert!(pixel(&app)[2] < before.0[2]);
        assert!(matches!(
            app.session.active().unwrap().doc.layer(app.session.active().unwrap().active_layer.unwrap()).unwrap().content,
            LayerContent::Raster(_)
        ));
        assert!(finish(&mut app, None).is_err(), "no session left");
        // calls with settings go straight to the engine
        assert!(invoke(&mut app, &egui::Context::default(), &json!({"light": {"exposure": 1.0}})).is_none());
    }

    #[test]
    fn switching_to_develop_with_a_composite_asks_and_remembers() {
        let mut app = app();
        assert!(composite_candidate(&app));
        assert!(!on_switch_to_develop(&mut app, &egui::Context::default()));
        assert!(app.develop_filter.prompting());
        answer(&mut app, &egui::Context::default(), "switch!").unwrap();
        assert_eq!(app.switch_module, Some(crate::Module::Develop));
        assert!(on_switch_to_develop(&mut app, &egui::Context::default()), "Just switch goes ahead once");
        // remembered: no prompt any more
        assert!(on_switch_to_develop(&mut app, &egui::Context::default()));
        app.session.edit_prefs(|p| p.dialogs.insert(CHOICE_KEY.into(), json!("live")));
        assert!(!on_switch_to_develop(&mut app, &egui::Context::default()));
        assert!(!app.develop_filter.prompting());
        let req = app.develop_filter.request.take().unwrap();
        assert_eq!((req.width, req.height), (40, 24));
        let n = app.session.active().unwrap().doc.layers.len();
        finish(&mut app, Some(json!({"light": {"exposure": 0.5}}))).unwrap();
        assert_eq!(app.session.active().unwrap().doc.layers.len(), n - 1, "Background + Sky grouped into one smart object");
    }
}
