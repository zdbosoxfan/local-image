//! Filter dialogs generated from engine command parameter specs, with live on-canvas preview.
//!
//! The preview runs the *same engine command* on the downsampled proxy document (pixel-sized
//! parameters scaled by the proxy factor), so what you preview is what you get.

use std::sync::Arc;

use photocraft_doc::Document;
use serde_json::{Map, Value, json};

use crate::PhotocraftApp;
use crate::theme::Tokens;

#[derive(Clone, Debug, PartialEq)]
pub enum Kind {
    Range {
        min: f32,
        max: f32,
        default: f32,
    },
    Choice(Vec<String>),
    Bool(bool),
    Int {
        default: i64,
    },
    /// Free text (e.g. a file path).
    Text,
    /// One of the open documents (stored as its index).
    Document,
    /// A row-major grid of integers (`int[25]` = 5×5).
    Grid(usize),
    /// Structured JSON (pins, curve points…): settable through the command, not shown in the dialog.
    Json,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Param {
    pub key: String,
    pub kind: Kind,
}

/// Parse the registry's parameter notation, e.g.
/// `{"radius":0.1..1000=1,"method":"spin|zoom","monochromatic":bool,"seed":u32=0,"horizontal":px=0}`.
pub fn parse_spec(spec: &str) -> Vec<Param> {
    let inner = spec.trim().trim_start_matches('{').trim_end_matches('}');
    let mut out = Vec::new();
    // Split on commas that start a new `"key":` (not inside strings or brackets).
    let mut parts: Vec<String> = Vec::new();
    let mut cur = String::new();
    let mut in_str = false;
    let mut depth = 0i32;
    for ch in inner.chars() {
        if ch == '"' {
            in_str = !in_str;
        }
        if !in_str {
            match ch {
                '[' | '{' => depth += 1,
                ']' | '}' => depth -= 1,
                _ => {}
            }
        }
        if ch == ',' && !in_str && depth == 0 {
            parts.push(std::mem::take(&mut cur));
        } else {
            cur.push(ch);
        }
    }
    if !cur.trim().is_empty() {
        parts.push(cur);
    }
    for part in parts {
        let Some((k, v)) = part.split_once(':') else { continue };
        let key = k.trim().trim_matches('"').to_string();
        if key.is_empty() || key == "layer" {
            continue;
        }
        let v = v.trim();
        let kind = if let Some(choices) = v.strip_prefix('"') {
            let body = choices.split('"').next().unwrap_or("");
            Kind::Choice(body.split('|').map(str::to_string).collect())
        } else if let Some(rest) = v.strip_prefix("bool") {
            Kind::Bool(rest.trim_start_matches('=').trim() == "true")
        } else if v == "text" {
            Kind::Text
        } else if v == "doc" {
            Kind::Document
        } else if v == "json" || v.starts_with("layer id") || v.starts_with('[') || v.starts_with('{') {
            Kind::Json
        } else if let Some(n) = v.strip_prefix("int[").and_then(|r| r.strip_suffix(']')).and_then(|n| n.parse().ok()) {
            Kind::Grid(n)
        } else if let Some((range, default)) = v.split_once('=').map(|(a, b)| (a, b.trim())).or(Some((v, ""))) {
            if let Some((lo, hi)) = range.split_once("..") {
                let min = lo.trim().parse().unwrap_or(0.0);
                let max = hi.trim().parse().unwrap_or(100.0);
                let default = default.parse().unwrap_or(min);
                Kind::Range { min, max, default }
            } else {
                Kind::Int { default: default.parse().unwrap_or(0) }
            }
        } else {
            continue;
        };
        out.push(Param { key, kind });
    }
    out
}

/// Parameter keys measured in pixels (scaled for proxy previews).
fn is_pixel_param(key: &str) -> bool {
    matches!(
        key,
        "radius"
            | "distance"
            | "cellSize"
            | "horizontal"
            | "vertical"
            | "height"
            | "wavelengthMin"
            | "wavelengthMax"
            | "amplitudeMin"
            | "amplitudeMax"
            | "maxRadius"
            | "size"
            | "blur"
            | "speed"
    )
}

/// Commands outside `filter.*` that get the schema dialog *with* live preview.
pub const PREVIEWED: &[&str] = &[
    "image.adjustments.selectiveColor",
    "image.adjustments.colorLookup",
    "image.adjustments.shadowsHighlights",
    "image.adjustments.replaceColor",
    "image.adjustments.matchColor",
    "image.adjustments.hdrToning",
    "image.rotation.arbitrary",
    "image.mode.indexedColor",
    "image.mode.bitmap",
    "image.mode.duotone",
    "layer.matting.defringe",
    "layer.matting.colorDecontaminate",
    "layer.layerStyle.scaleEffects",
];

pub fn has_dialog(command: &str) -> bool {
    // The Filter Gallery has its own full-window dialog (gallery_ui).
    if command == "filter.filterGallery" {
        return false;
    }
    (command.starts_with("filter.")
        || command.starts_with("select.modify.")
        || PREVIEWED.contains(&command)
        || matches!(
            command,
            "image.trim"
                | "view.newGuide"
                | "select.refineEdge"
                | "edit.assignProfile"
                | "edit.convertToProfile"
                | "view.proofSetup"
                | "layer.layerStyle.globalLight"
                | "image.mode.colorTable"
        ))
        && photocraft_engine::commands::find(command).is_some_and(|c| !parse_spec(c.params).is_empty())
}

/// Submitted durable values only; a pending job owns this snapshot until its
/// terminal event. It never contains layer/document references, paths or UI state.
#[derive(Clone, Debug)]
pub(crate) struct Remembered {
    key: String,
    values: Map<String, Value>,
}

fn preference_key(fields: &Map<String, Value>) -> Option<String> {
    let command = fields.get("__command")?.as_str()?;
    if command.starts_with("filter.") && photocraft_engine::commands::find(command).is_some() {
        Some(format!("filter.dialog/{command}"))
    } else if command == "plugin.run" {
        // A stable registered plug-in identifier namespaces choices; it is not a
        // live layer/document ID and is never retained among the scalar values.
        let plugin = fields.get("id")?.as_str()?;
        (plugin.len() <= 256 && photocraft_plugins::registry::get(plugin).is_some()).then(|| format!("filter.dialog/plugin/{plugin}"))
    } else {
        None // Ruler/canvas-derived defaults and other non-filter dialogs stay contextual.
    }
}

pub(crate) fn durable_value(param: &Param, value: &Value) -> Option<Value> {
    if param.key.starts_with("__")
        || param.key.ends_with("Id")
        || matches!(
            param.key.as_str(),
            "id" | "layer" | "document" | "reference" | "target" | "source" | "path" | "mapPath" | "mapDocument" | "foreground" | "background"
        )
    {
        return None;
    }
    let valid = match &param.kind {
        Kind::Range { min, max, .. } => value.as_f64().is_some_and(|n| n.is_finite() && (n as f32) >= *min && (n as f32) <= *max),
        Kind::Choice(choices) => value.as_str().is_some_and(|s| s.len() <= 256 && choices.iter().any(|choice| choice == s)),
        Kind::Bool(_) => value.is_boolean(),
        Kind::Int { .. } => value.as_i64().is_some_and(|n| (-30_000..=30_000).contains(&n)),
        // Custom Filter's matrix is editable reusable state, unlike arbitrary
        // JSON. Accept only the declared square shape and integer cell bounds.
        Kind::Grid(size) => {
            *size > 0
                && *size <= 256
                && size.isqrt().pow(2) == *size
                && value
                    .as_array()
                    .is_some_and(|cells| cells.len() == *size && cells.iter().all(|cell| cell.as_i64().is_some_and(|n| (-999..=999).contains(&n))))
        }
        Kind::Text | Kind::Document | Kind::Json => false,
    };
    valid.then(|| value.clone())
}

fn schema(fields: &Map<String, Value>) -> Option<&str> {
    fields.get("__spec").and_then(Value::as_str).or_else(|| photocraft_engine::commands::find(fields.get("__command")?.as_str()?).map(|command| command.params))
}

fn snapshot(fields: &Map<String, Value>) -> Option<Remembered> {
    if !fields.contains_key("__filter") {
        return None;
    }
    let key = preference_key(fields)?;
    let values = parse_spec(schema(fields)?)
        .into_iter()
        .filter_map(|param| {
            let value = durable_value(&param, fields.get(&param.key)?)?;
            Some((param.key, value))
        })
        .collect();
    Some(Remembered { key, values })
}

fn restore(app: &PhotocraftApp, fields: &mut Map<String, Value>) {
    let Some(key) = preference_key(fields) else { return };
    let Some(saved) = app.session.prefs().dialogs.get(&key).and_then(Value::as_object) else { return };
    let params = schema(fields).map(parse_spec).unwrap_or_default();
    for param in params {
        if let Some(value) = saved.get(&param.key).and_then(|value| durable_value(&param, value)) {
            fields.insert(param.key, value);
        }
    }
}

pub(crate) const GALLERY_KEY: &str = "filter.dialog/filter.filterGallery";

impl Remembered {
    /// Caller supplies Gallery's validated bounded typed recipe, not its runtime dialog.
    pub(crate) fn gallery(effects: Vec<Value>) -> Self {
        Self { key: GALLERY_KEY.into(), values: Map::from_iter([("effects".into(), Value::Array(effects))]) }
    }

    fn publish(self, app: &mut PhotocraftApp) {
        app.session.edit_prefs(|prefs| prefs.dialogs.insert(self.key, Value::Object(self.values)));
    }
}

/// Run through the existing smart-filter adapter and command/job admission. Only
/// synchronous success or the matching job's applied result publishes preferences.
pub(crate) fn confirm(app: &mut PhotocraftApp, fields: &Map<String, Value>, command: String, params: Value) -> Result<Value, String> {
    submit(app, command, params, snapshot(fields))
}

pub(crate) fn submit(app: &mut PhotocraftApp, command: String, params: Value, remembered: Option<Remembered>) -> Result<Value, String> {
    let result = app.run(&command, params)?;
    if let Some(remembered) = remembered {
        if result.get("pending").and_then(Value::as_bool) == Some(true) {
            if let Some(job) = result.get("job").and_then(Value::as_u64).map(photocraft_engine::jobs::JobId)
                && app.session.job(job).is_some()
            {
                app.jobs.filter_defaults.push((job, command, remembered));
            }
        } else {
            remembered.publish(app);
        }
    }
    Ok(result)
}

pub(crate) fn on_job_event(app: &mut PhotocraftApp, event: &photocraft_engine::jobs::JobEvent) {
    if let Some(index) = app.jobs.filter_defaults.iter().position(|(job, _, _)| *job == event.id) {
        let (_, command, remembered) = app.jobs.filter_defaults.remove(index);
        if command == event.command && matches!(event.outcome, photocraft_engine::jobs::JobOutcome::Done(_)) {
            remembered.publish(app);
        }
    }
}

pub fn open(app: &mut PhotocraftApp, command: &str) -> Option<u64> {
    let spec = photocraft_engine::commands::find(command)?;
    let mut fields = Map::new();
    fields.insert("__command".into(), json!(command));
    fields.insert("__label".into(), json!(spec.label));
    fields.insert("__filter".into(), json!(true));
    if command.starts_with("filter.") || PREVIEWED.contains(&command) {
        fields.insert("__preview".into(), json!(true));
    }
    for p in parse_spec(spec.params) {
        let v = match &p.kind {
            Kind::Range { default, .. } => json!(default),
            Kind::Choice(c) => json!(c.first().cloned().unwrap_or_default()),
            Kind::Bool(default) => json!(default),
            Kind::Int { default } => json!(default),
            Kind::Text => json!(""),
            Kind::Document => json!(-1),
            // Identity kernel: 1 in the centre.
            Kind::Grid(n) => json!((0..*n).map(|i| i64::from(i == *n / 2)).collect::<Vec<_>>()),
            // Colour inputs come from the current swatches, so the proxy preview matches the result.
            Kind::Json if p.key == "foreground" => json!(app.session.tools.foreground),
            Kind::Json if p.key == "background" => json!(app.session.tools.background),
            Kind::Json => continue,
        };
        fields.insert(p.key, v);
    }
    restore(app, &mut fields);
    if command == "image.rotation.arbitrary" {
        straighten_defaults(app, &mut fields);
    }
    if command == "edit.assignProfile" {
        assign_profile_defaults(app, &mut fields);
    }
    if parse_spec(spec.params).iter().any(|p| p.kind == Kind::Document) {
        // The document picker lists every open document (params refer to them by index).
        let names: Vec<String> = app.session.documents().iter().map(|d| d.doc.name.clone()).collect();
        fields.insert("__docs".into(), json!(names));
    }
    let id = if command == "edit.colorSettings" {
        app.ui.open_dialog(crate::state::DialogKind::Command, fields)
    } else {
        crate::dialogs::open_document_dialog(app, crate::state::DialogKind::Command, fields)
    };
    Some(id)
}

/// Arbitrary rotation starts at the angle that straightens the ruler line, when there is one.
fn straighten_defaults(app: &PhotocraftApp, fields: &mut Map<String, Value>) {
    let Some(r) = app.session.active().and_then(|d| d.doc.measurement.ruler) else { return };
    let rot = photocraft_engine::analysis_cmds::straighten_angle(&r);
    // A ruler read from a damaged file could hold non-finite ends: keep the 0° default then.
    if !rot.is_finite() {
        return;
    }
    fields.insert("angle".into(), json!(rot.abs()));
    fields.insert("direction".into(), json!(if rot < 0.0 { "ccw" } else { "cw" }));
}

/// Assign Profile offers only the profiles the document's colour mode can take: the command
/// refuses one of another colour space. `__profiles` lists them for the `profile` menu.
fn assign_profile_defaults(app: &PhotocraftApp, fields: &mut Map<String, Value>) {
    let Some(st) = app.session.active() else { return };
    let Some(spec) = photocraft_engine::commands::find("edit.assignProfile") else { return };
    let space = photocraft_engine::color_cmds::mode_space(st.doc.mode);
    let fits = |name: &str| name == "none" || app.session.color.resolve(name, Some(&st.doc), None).is_ok_and(|p| p.color_space == space);
    let builtin: Vec<String> = parse_spec(spec.params)
        .into_iter()
        .filter(|p| p.key == "profile")
        .flat_map(|p| match p.kind {
            Kind::Choice(options) => options,
            _ => Vec::new(),
        })
        .collect();
    let allowed: Vec<String> = builtin.iter().filter(|name| fits(name)).cloned().collect();
    fields.insert("__profiles".into(), json!(allowed));
}

/// A filter dialog with live preview for `command` whose parameters follow `spec` (registry
/// notation) instead of the command's own; `fixed` params (e.g. a plug-in id) are passed through.
pub fn open_with_spec(app: &mut PhotocraftApp, command: &str, label: &str, spec: &str, fixed: Map<String, Value>) -> u64 {
    let mut fields = fixed;
    fields.insert("__command".into(), json!(command));
    fields.insert("__label".into(), json!(label));
    fields.insert("__filter".into(), json!(true));
    fields.insert("__preview".into(), json!(true));
    fields.insert("__spec".into(), json!(spec));
    for p in parse_spec(spec) {
        let v = match &p.kind {
            Kind::Range { default, .. } => json!(default),
            Kind::Choice(c) => json!(c.first().cloned().unwrap_or_default()),
            Kind::Bool(default) => json!(default),
            Kind::Int { default } => json!(default),
            _ => continue,
        };
        fields.insert(p.key, v);
    }
    restore(app, &mut fields);
    crate::dialogs::open_document_dialog(app, crate::state::DialogKind::Command, fields)
}

pub(crate) fn label(key: &str) -> String {
    tl!(&source_label(key)).to_owned()
}

pub(crate) fn source_label(key: &str) -> String {
    // camelCase → "Camel Case"
    let mut s = String::new();
    for (i, ch) in key.chars().enumerate() {
        if i == 0 {
            s.extend(ch.to_uppercase());
        } else if ch.is_uppercase() {
            s.push(' ');
            s.push(ch);
        } else {
            s.push(ch);
        }
    }
    s
}

fn choice_label(v: &str) -> String {
    label(v)
}

fn uses_logarithmic_slider(min: f32, max: f32) -> bool {
    min > 0.0 && max / min > 500.0
}

/// Dialog body for filter commands.
pub fn body(ui: &mut egui::Ui, f: &mut Map<String, Value>) {
    let t = Tokens::get(ui.ctx());
    let cmd = f.get("__command").and_then(Value::as_str).unwrap_or_default().to_string();
    // Plug-in dialogs carry their own spec (from the plug-in's manifest).
    let spec = match f.get("__spec").and_then(Value::as_str) {
        Some(s) => s.to_string(),
        None => match photocraft_engine::commands::find(&cmd) {
            Some(c) => c.params.to_string(),
            None => return,
        },
    };
    for p in parse_spec(&spec) {
        match p.kind {
            Kind::Range { min, max, default } => {
                let mut v = f.get(&p.key).and_then(Value::as_f64).unwrap_or(default as f64) as f32;
                let unit = if is_pixel_param(&p.key) {
                    "px"
                } else if p.key == "angle" {
                    "°"
                } else if p.key == "amount" && max <= 500.0 {
                    "%"
                } else {
                    ""
                };
                if uses_logarithmic_slider(min, max) {
                    // Keep one numeric field for direct entry and one log-scaled slider for
                    // wide ranges. slider_row would add a second, linear slider (#146).
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new(label(&p.key)).color(t.text_dim));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            crate::widgets::value_field(ui, &mut v, min..=max, unit, 74.0);
                        });
                    });
                    let mut lv = v.clamp(min, max).ln();
                    if crate::widgets::slider(ui, &mut lv, min.ln()..=max.ln(), None).changed() {
                        v = lv.exp();
                    }
                    ui.add_space(4.0);
                } else {
                    crate::widgets::slider_row(ui, &label(&p.key), &mut v, min..=max, unit, None);
                }
                f.insert(p.key, json!((v * 10.0).round() / 10.0));
            }
            Kind::Choice(mut options) => {
                // Assign Profile: only what the document's colour mode can take.
                if cmd == "edit.assignProfile"
                    && p.key == "profile"
                    && let Some(allowed) = f.get("__profiles").and_then(Value::as_array)
                {
                    options.retain(|o| allowed.iter().any(|a| a.as_str() == Some(o.as_str())));
                }
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(label(&p.key)).color(t.text_dim));
                    let mut cur = f.get(&p.key).and_then(Value::as_str).or(options.first().map(String::as_str)).unwrap_or_default().to_string();
                    let labels: Vec<String> = options.iter().map(|o| choice_label(o)).collect();
                    let opts: Vec<(String, &str)> = options.iter().cloned().zip(labels.iter().map(String::as_str)).collect();
                    crate::widgets::dropdown(ui, &format!("flt-{cmd}-{}", p.key), &mut cur, &opts, 170.0);
                    f.insert(p.key.clone(), json!(cur));
                });
            }
            Kind::Bool(default) => {
                let mut b = f.get(&p.key).and_then(Value::as_bool).unwrap_or(default);
                crate::widgets::checkbox(ui, &mut b, &label(&p.key));
                f.insert(p.key, json!(b));
            }
            Kind::Text => {
                let mut v = f.get(&p.key).and_then(Value::as_str).unwrap_or_default().to_string();
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(label(&p.key)).color(t.text_dim));
                    ui.add(egui::TextEdit::singleline(&mut v).desired_width(200.0));
                });
                if v.is_empty() {
                    f.remove(&p.key);
                } else {
                    f.insert(p.key, json!(v));
                }
            }
            Kind::Document => {
                let names: Vec<String> =
                    f.get("__docs").and_then(Value::as_array).map(|a| a.iter().filter_map(|v| v.as_str().map(str::to_string)).collect()).unwrap_or_default();
                let cur = f.get(&p.key).and_then(Value::as_i64).unwrap_or(-1);
                let mut sel = cur.to_string();
                let mut opts: Vec<(String, &str)> = vec![("-1".to_string(), tl!("None"))];
                opts.extend(names.iter().enumerate().map(|(i, n)| (i.to_string(), n.as_str())));
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(label(&p.key)).color(t.text_dim));
                    crate::widgets::dropdown(ui, &format!("flt-{cmd}-{}", p.key), &mut sel, &opts, 170.0);
                });
                match sel.parse::<i64>() {
                    Ok(i) if i >= 0 => f.insert(p.key, json!(i)),
                    _ => f.insert(p.key, json!(-1)),
                };
            }
            Kind::Grid(n) => {
                let side = (n as f32).sqrt().round().max(1.0) as usize;
                let mut vals: Vec<f32> =
                    f.get(&p.key).and_then(Value::as_array).map(|a| a.iter().map(|v| v.as_f64().unwrap_or(0.0) as f32).collect()).unwrap_or_default();
                vals.resize(n, 0.0);
                ui.label(egui::RichText::new(label(&p.key)).color(t.text_dim));
                egui::Grid::new(format!("flt-grid-{cmd}-{}", p.key)).spacing([4.0, 4.0]).show(ui, |ui| {
                    for (i, v) in vals.iter_mut().enumerate() {
                        crate::widgets::value_field(ui, v, -999.0..=999.0, "", 44.0);
                        if (i + 1) % side == 0 {
                            ui.end_row();
                        }
                    }
                });
                f.insert(p.key, json!(vals.iter().map(|v| v.round() as i64).collect::<Vec<_>>()));
            }
            Kind::Json => {}
            Kind::Int { default } => {
                let mut v = f.get(&p.key).and_then(Value::as_f64).unwrap_or(default as f64) as f32;
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(label(&p.key)).color(t.text_dim));
                    crate::widgets::value_field(ui, &mut v, -30000.0..=30000.0, "", 80.0);
                });
                f.insert(p.key, json!(v.round() as i64));
            }
        }
    }
    // Read-only context the dialog opener supplies (e.g. the monitor profile in use).
    if let Some(note) = f.get("__note").and_then(Value::as_str) {
        ui.add_space(4.0);
        ui.add(egui::Label::new(egui::RichText::new(note).color(t.text_dim)).wrap());
    }
    if let Some(mut preview) = f.get("__preview").and_then(Value::as_bool) {
        ui.add_space(4.0);
        crate::widgets::checkbox(ui, &mut preview, tl!("Preview"));
        f.insert("__preview".into(), json!(preview));
    }
}

/// User-facing params (strip the dialog's private `__` keys).
pub fn params_of(f: &Map<String, Value>) -> Value {
    // A document picker left at "None" (-1) means "not given".
    let unset = |k: &str, v: &Value| k == "mapDocument" && v.as_i64() == Some(-1);
    Value::Object(f.iter().filter(|(k, v)| !k.starts_with("__") && !unset(k, v)).map(|(k, v)| (k.clone(), v.clone())).collect())
}

/// Compute a preview document: run `command` with `params` on the proxy (scaled) copy of `doc`.
pub fn preview_document(doc: &Document, active: Option<photocraft_doc::LayerId>, command: &str, params: &Value, k: u32) -> Option<Document> {
    preview_document_with_sources(doc, active, command, params, k, &[])
}

/// Open documents are in session order; document parameters refer to these indices.
pub(crate) fn preview_document_with_sources(
    doc: &Document,
    active: Option<photocraft_doc::LayerId>,
    command: &str,
    params: &Value,
    k: u32,
    documents: &[Arc<Document>],
) -> Option<Document> {
    let proxy = crate::proxy::proxy_document(doc, k);
    let mut s = photocraft_engine::Session::new();
    s.add_document(proxy, None);
    let mut p = params.clone();
    if let Some(spec) = photocraft_engine::commands::find(command)
        && let Some(fields) = p.as_object_mut()
    {
        let mut added = std::collections::BTreeMap::new();
        for param in parse_spec(spec.params).into_iter().filter(|p| p.kind == Kind::Document) {
            let Some(value) = fields.get_mut(&param.key) else { continue };
            let Some(index) = value.as_u64().and_then(|i| usize::try_from(i).ok()) else { continue };
            let Some(source) = documents.get(index) else { continue };
            let mapped =
                if source.id == doc.id { 0 } else { *added.entry(index).or_insert_with(|| s.add_document(crate::proxy::proxy_document(source, k), None)) };
            *value = json!(mapped);
        }
        s.set_active(0);
    }
    if let Some(id) = active {
        s.select_layer(id).ok()?;
    }
    if k > 1
        && let Some(o) = p.as_object_mut()
    {
        for (key, v) in o.iter_mut() {
            if is_pixel_param(key)
                && let Some(x) = v.as_f64()
            {
                *v = json!((x / k as f64).max(if key == "cellSize" { 1.0 } else { 0.1 }));
            }
        }
    }
    s.execute(command, p).ok()?;
    s.active().map(|d| (*d.doc).clone())
}

/// Cached preview state on the app.
pub struct FilterPreview {
    pub doc: photocraft_doc::DocId,
    pub revision: u64,
    pub hash: u64,
    pub k: u32,
    pub result: Option<Arc<Document>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn match_color_preview_uses_the_source_document() {
        use photocraft_doc::{Color, ColorMode, SampleType, Size};
        let solid = |name: &str, c: [f32; 4]| {
            let mut d = Document::with_background(name, Size::new(32, 32), ColorMode::Rgb, SampleType::U8, Color::WHITE);
            d.layers[0].surface_mut().unwrap().fill_rect(photocraft_geom::Rect::new(0, 0, 32, 32), &c);
            d.layers[0].surface_mut().unwrap().fill_rect(photocraft_geom::Rect::new(0, 0, 16, 32), &[c[0] * 0.5, c[1] * 0.5, c[2] * 0.5, 1.0]);
            d
        };
        let mut s = photocraft_engine::Session::new();
        s.add_document(solid("reference", [0.1, 0.3, 0.9, 1.0]), None);
        s.add_document(solid("texture", [0.9, 0.5, 0.2, 1.0]), None);
        let doc = s.active().unwrap().doc.clone();
        let layer = doc.layers[0].id;
        let documents: Vec<_> = s.documents().iter().map(|d| d.doc.clone()).collect();
        for source in [0, 1] {
            let params = json!({"source": source});
            let mut applied = photocraft_engine::Session::new();
            for document in &documents {
                applied.add_document((**document).clone(), None);
            }
            applied.execute("image.adjustments.matchColor", params.clone()).unwrap();
            let want = applied.active().unwrap().doc.layers[0].surface().unwrap().pixel(24, 16);
            let preview = preview_document_with_sources(&doc, Some(layer), "image.adjustments.matchColor", &params, 1, &documents).unwrap();
            let got = preview.layers[0].surface().unwrap().pixel(24, 16);
            assert!((0..3).all(|c| (got[c] - want[c]).abs() < 1.0 / 255.0 + 1e-4), "source {source}: preview {got:?}, applied {want:?}");
        }
        let preview = preview_document_with_sources(&doc, Some(layer), "image.adjustments.matchColor", &json!({"source": 0}), 2, &documents).unwrap();
        let pixel = preview.layers[0].surface().unwrap().pixel(12, 8);
        assert!(pixel[2] > pixel[0], "proxy should take the reference's blue: {pixel:?}");
    }

    #[test]
    fn parses_registry_notation() {
        let p = parse_spec(r#"{"radius":0.1..1000=1,"method":"spin|zoom","monochromatic":bool,"seed":u32=0,"horizontal":px=0}"#);
        assert_eq!(p[0], Param { key: "radius".into(), kind: Kind::Range { min: 0.1, max: 1000.0, default: 1.0 } });
        assert_eq!(p[1].kind, Kind::Choice(vec!["spin".into(), "zoom".into()]));
        assert_eq!(p[2].kind, Kind::Bool(false));
        assert_eq!(p[3].kind, Kind::Int { default: 0 });
        assert_eq!(p[4].kind, Kind::Int { default: 0 });
        assert!(parse_spec("{}").is_empty());
        let p = parse_spec(r#"{"lighting":bool=true,"kernel":int[25],"pins":json,"points":[[0,0],[1,0]],"mapPath":text,"mapDocument":doc,"scale":1..9999=1}"#);
        let kinds: Vec<&Kind> = p.iter().map(|p| &p.kind).collect();
        assert_eq!(
            kinds,
            [&Kind::Bool(true), &Kind::Grid(25), &Kind::Json, &Kind::Json, &Kind::Text, &Kind::Document, &Kind::Range { min: 1.0, max: 9999.0, default: 1.0 }]
        );
    }

    /// A layer-id param has no number field: drawing the Auto-Align dialog used to write
    /// `reference: 0` back, which the engine rejects as not a selected layer (#674).
    #[test]
    fn layer_id_params_stay_out_of_the_dialog() {
        let mut f = Map::new();
        f.insert("__command".into(), json!("edit.autoAlignLayers"));
        egui::Context::default().run_ui(Default::default(), |ui| body(ui, &mut f)).textures_delta.clear();
        assert!(!params_of(&f).as_object().unwrap().contains_key("reference"), "{f:?}");
    }

    #[test]
    fn wide_positive_ranges_use_the_logarithmic_slider_path() {
        assert!(uses_logarithmic_slider(0.1, 1000.0), "Gaussian Blur radius");
        assert!(uses_logarithmic_slider(1.0, 9999.0));
        assert!(!uses_logarithmic_slider(1.0, 500.0));
        assert!(!uses_logarithmic_slider(0.0, 1000.0));
    }

    /// The profiles Assign Profile offers for a new document converted with `mode` (if any),
    /// and which of the command's built-in profiles the command itself accepts for it.
    fn assign_profile_offers(mode: Option<&str>) -> (Vec<String>, Vec<String>) {
        let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), Default::default());
        app.run("file.new", json!({"width": 8, "height": 8})).unwrap();
        if let Some(mode) = mode {
            app.run(mode, json!({})).unwrap();
        }
        let id = open(&mut app, "edit.assignProfile").unwrap();
        let offered: Vec<String> =
            app.ui.dialog_mut(id).unwrap().fields["__profiles"].as_array().unwrap().iter().filter_map(|v| v.as_str().map(str::to_owned)).collect();
        let spec = photocraft_engine::commands::find("edit.assignProfile").unwrap();
        let builtin = parse_spec(spec.params).into_iter().find(|p| p.key == "profile").unwrap();
        let Kind::Choice(options) = builtin.kind else { panic!("profile is a menu") };
        let accepted = options.into_iter().filter(|name| app.session.execute("edit.assignProfile", json!({"profile": name})).is_ok()).collect();
        (offered, accepted)
    }

    /// Assign Profile lists the profiles the command accepts for the document's colour mode, and
    /// none of the others.
    #[test]
    fn assign_profile_offers_only_profiles_of_the_documents_colour_mode() {
        for mode in [None, Some("image.mode.cmyk"), Some("image.mode.grayscale")] {
            let (offered, accepted) = assign_profile_offers(mode);
            for name in &accepted {
                assert!(offered.contains(name), "{mode:?}: {name} can be assigned, so it is offered");
            }
            for name in ["srgb", "coated-cmyk", "sgray"] {
                assert_eq!(offered.iter().any(|o| o == name), accepted.iter().any(|a| a == name), "{mode:?}: {name}");
            }
            assert!(offered.iter().any(|o| o == "working") && offered.iter().any(|o| o == "none"), "{mode:?}: working space and none always fit");
        }
        let (rgb, _) = assign_profile_offers(None);
        assert!(rgb.iter().any(|o| o == "srgb") && !rgb.iter().any(|o| o == "coated-cmyk"), "an RGB document gets RGB profiles");
        let (cmyk, _) = assign_profile_offers(Some("image.mode.cmyk"));
        assert!(cmyk.iter().any(|o| o == "coated-cmyk") && !cmyk.iter().any(|o| o == "srgb"), "a CMYK document gets CMYK profiles");
    }

    #[test]
    fn new_filters_have_dialogs_or_run_directly() {
        for id in [
            "filter.stylize.oilPaint",
            "filter.blur.lensBlur",
            "filter.blurGallery.irisBlur",
            "filter.other.custom",
            "filter.distort.displace",
            "filter.pixelate.mezzotint",
            "filter.render.lightingEffects",
            "filter.render.relight",
        ] {
            assert!(has_dialog(id), "{id}");
        }
        for id in ["filter.pixelate.facet", "filter.pixelate.fragment", "filter.video.ntscColors"] {
            assert!(!has_dialog(id), "{id}");
        }
    }

    #[test]
    fn every_filter_command_spec_parses() {
        for c in photocraft_engine::command_specs().iter().filter(|c| c.id.starts_with("filter.")) {
            let _ = parse_spec(c.params);
        }
        assert!(has_dialog("filter.blur.gaussianBlur"));
        assert!(!has_dialog("filter.stylize.findEdges"));
    }

    #[test]
    fn korean_covers_generated_filter_options_and_gallery_names() {
        let ko = li_i18n::Lang::from_tag("ko-KR").unwrap();
        let mut missing = std::collections::BTreeSet::new();
        let mut check = |s: String| {
            if !matches!(s.as_str(), "X" | "Y" | "A" | "B") && !li_i18n::has(ko, &s) {
                missing.insert(s);
            }
        };
        for c in photocraft_engine::command_specs().iter().filter(|c| c.id.starts_with("filter.") || c.id.starts_with("image.adjustments.")) {
            for p in parse_spec(c.params) {
                if !p.key.chars().all(|c| c.is_ascii_alphanumeric()) || matches!(p.kind, Kind::Json) {
                    continue;
                }
                check(source_label(&p.key));
                if let Kind::Choice(choices) = p.kind {
                    for choice in choices {
                        // Only symbolic options are UI choices; registry docs also contain
                        // colour syntax and array notation, which are not translatable names.
                        if choice.chars().all(|c| c.is_ascii_alphanumeric()) {
                            check(source_label(&choice));
                        }
                    }
                }
            }
        }
        for f in photocraft_algo::GalleryFilter::ALL {
            check(f.name().to_string());
        }
        for cat in photocraft_algo::GALLERY_CATEGORIES {
            check(cat.to_string());
        }
        assert!(missing.is_empty(), "missing Korean dynamic labels: {missing:#?}");
    }

    #[test]
    fn preview_runs_engine_command_on_proxy() {
        let mut doc = Document::with_background(
            "p",
            photocraft_doc::Size::new(64, 64),
            photocraft_doc::ColorMode::Rgb,
            photocraft_doc::SampleType::U8,
            photocraft_doc::Color::WHITE,
        );
        let bg = doc.layers[0].id;
        doc.layers[0].surface_mut().unwrap().fill_rect(photocraft_geom::Rect::new(0, 0, 32, 64), &[0.0, 0.0, 0.0, 1.0]);
        let out = preview_document(&doc, Some(bg), "filter.blur.gaussianBlur", &json!({"radius": 4.0}), 1).unwrap();
        let p = out.layers[0].surface().unwrap().pixel(32, 32);
        assert!(p[0] > 0.2 && p[0] < 0.8, "edge blurred: {p:?}");
        assert_eq!(label("wavelengthMin"), "Wavelength Min");
    }

    #[test]
    fn preview_stays_inside_the_selection_at_every_proxy_factor() {
        use photocraft_doc::SampleType;
        for depth in [SampleType::U8, SampleType::U16, SampleType::F32] {
            let mut doc =
                Document::with_background("p", photocraft_doc::Size::new(64, 64), photocraft_doc::ColorMode::Rgb, depth, photocraft_doc::Color::gray(0.5));
            let bg = doc.layers[0].id;
            let mut sel = photocraft_raster::Surface::new(photocraft_doc::PixelFormat::GRAY8);
            sel.fill_rect(photocraft_geom::Rect::new(0, 0, 32, 64), &[1.0]);
            doc.selection = Some(sel);
            for k in [1, 2, 4] {
                let out = preview_document(&doc, Some(bg), "filter.noise.addNoise", &json!({"amount": 100.0}), k).unwrap();
                let s = out.layers[0].surface().unwrap();
                let half = 32 / k as i32;
                let changed = |x0: i32, x1: i32| (0..64 / k as i32).any(|y| (x0..x1).any(|x| s.pixel(x, y) != doc.layers[0].surface().unwrap().pixel(0, 0)));
                assert!(changed(0, half), "{depth:?} k={k}: noise inside the selection");
                assert!(!changed(half, 64 / k as i32), "{depth:?} k={k}: nothing outside the selection");
            }
        }
    }
}

#[cfg(test)]
mod remembered_tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
    use super::*;
    use photocraft_engine::jobs::Started;
    use std::time::{Duration, Instant};
    const BLUR: &str = "filter.blur.gaussianBlur";
    const NOISE: &str = "filter.noise.addNoise";

    fn app() -> PhotocraftApp {
        let mut s = photocraft_engine::Session::new();
        s.execute("file.new", json!({"width":16,"height":12,"background":"transparent"})).unwrap();
        s.execute("edit.fill", json!({"color":"#3366cc"})).unwrap();
        PhotocraftApp::new(s, crate::Services::default())
    }
    fn saved(app: &PhotocraftApp, command: &str) -> Option<Value> {
        app.session.prefs().dialogs.get(&format!("filter.dialog/{command}")).cloned()
    }
    fn poll(app: &mut PhotocraftApp) {
        let ctx = egui::Context::default();
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            crate::jobs_ui::tick(app, &ctx);
            if !app.session.has_jobs() && app.jobs.filter_defaults.is_empty() {
                break;
            }
            assert!(Instant::now() < deadline, "bounded filter completion");
            std::thread::sleep(Duration::from_millis(1));
        }
    }

    #[test]
    fn remembered_filter_success_restart_preview_cancel_and_error() {
        let mut app = app();
        let id = open(&mut app, BLUR).unwrap();
        app.ui.dialog_mut(id).unwrap().fields.insert("radius".into(), json!(2.5));
        crate::dialogs::confirm(&mut app, id).unwrap();
        assert_eq!(saved(&app, BLUR), Some(json!({"radius":2.5})));
        let id = open(&mut app, NOISE).unwrap();
        app.ui
            .dialog_mut(id)
            .unwrap()
            .fields
            .extend(json!({"amount":7.5,"distribution":"gaussian","monochromatic":true,"seed":17}).as_object().unwrap().clone());
        crate::dialogs::confirm(&mut app, id).unwrap();
        assert_eq!(saved(&app, NOISE), Some(json!({"amount":7.5,"distribution":"gaussian","monochromatic":true,"seed":17})));
        let id = open(&mut app, BLUR).unwrap();
        app.ui.dialog_mut(id).unwrap().fields.insert("radius".into(), json!(5));
        let params = params_of(&app.ui.dialog_mut(id).unwrap().fields);
        let state = app.session.active().unwrap();
        assert!(preview_document(&state.doc, state.active_layer, BLUR, &params, 1).is_some());
        assert_eq!(saved(&app, BLUR), Some(json!({"radius":2.5})), "preview must never publish");
        crate::dialogs::cancel(&mut app, id).unwrap();
        assert_eq!(saved(&app, BLUR), Some(json!({"radius":2.5})));
        let serialized = serde_json::to_string(&app.session.prefs().to_json()).unwrap();
        let mut s = photocraft_engine::Session::new();
        s.load_prefs_json(&serialized).unwrap();
        let mut restarted = PhotocraftApp::new(s, crate::Services::default());
        let id = open(&mut restarted, BLUR).unwrap();
        assert_eq!(restarted.ui.dialog_mut(id).unwrap().fields["radius"], 2.5);
        restarted.ui.dialog_mut(id).unwrap().fields.insert("radius".into(), json!(8));
        assert!(crate::dialogs::confirm(&mut restarted, id).is_err(), "no document");
        assert_eq!(saved(&restarted, BLUR), Some(json!({"radius":2.5})));
        let id = open(&mut restarted, NOISE).unwrap();
        assert_eq!(restarted.ui.dialog_mut(id).unwrap().fields["monochromatic"], true);
        assert_eq!(restarted.ui.dialog_mut(id).unwrap().fields["distribution"], "gaussian");
    }

    #[test]
    fn remembered_filter_async_uses_submitted_snapshot_and_discards_cancel_failure() {
        let mut app = app();
        app.background_jobs = true;
        let id = open(&mut app, BLUR).unwrap();
        app.ui.dialog_mut(id).unwrap().fields.insert("radius".into(), json!(3.5));
        let result = crate::dialogs::confirm(&mut app, id).unwrap();
        assert_eq!(result["pending"], true);
        assert!(saved(&app, BLUR).is_none(), "starting is not applying");
        app.run("file.new", json!({"width":8,"height":8,"background":"transparent"})).unwrap();
        let later = open(&mut app, BLUR).unwrap();
        app.ui.dialog_mut(later).unwrap().fields.insert("radius".into(), json!(9));
        poll(&mut app);
        assert_eq!(saved(&app, BLUR), Some(json!({"radius":3.5})), "not the later dialog");
        crate::dialogs::cancel(&mut app, later).unwrap();
        let id = open(&mut app, BLUR).unwrap();
        app.ui.dialog_mut(id).unwrap().fields.insert("radius".into(), json!(7));
        let job = crate::dialogs::confirm(&mut app, id).unwrap()["job"].as_u64().unwrap();
        crate::jobs_ui::cancel(&mut app, photocraft_engine::jobs::JobId(job));
        poll(&mut app);
        assert_eq!(saved(&app, BLUR), Some(json!({"radius":3.5})));
        // An actual failing worker event exercises the same UI terminal-event path.
        let id = open(&mut app, BLUR).unwrap();
        app.ui.dialog_mut(id).unwrap().fields.insert("radius".into(), json!(8));
        let submitted = snapshot(&app.ui.dialog_mut(id).unwrap().fields).unwrap();
        crate::dialogs::cancel(&mut app, id).unwrap();
        let Started::Job(job) = app
            .session
            .start_job(
                BLUR,
                json!({}),
                "Failing filter fixture",
                true,
                |_ctx| Err::<(), _>(photocraft_engine::EngineError::Other("injected filter failure".into())),
                |_session, ()| Ok(json!({})),
            )
            .unwrap()
        else {
            panic!("native job required")
        };
        app.jobs.filter_defaults.push((job, BLUR.into(), submitted));
        poll(&mut app);
        assert_eq!(saved(&app, BLUR), Some(json!({"radius":3.5})));
        assert!(app.ui.status_error);
        assert!(app.ui.status.contains("injected filter failure"));
    }

    #[test]
    fn remembered_filter_scalar_validation_excludes_targets_paths_and_context() {
        let mut app = app();
        app.session.edit_prefs(|p| {
            p.dialogs.insert(
                format!("filter.dialog/{NOISE}"),
                json!({"amount":-1,"distribution":"invalid","monochromatic":"true","seed":90000,"layer":12,"__preview":false}),
            )
        });
        let id = open(&mut app, NOISE).unwrap();
        let f = &app.ui.dialog_mut(id).unwrap().fields;
        assert_eq!(f["amount"], 12.5);
        assert_eq!(f["distribution"], "uniform");
        assert_eq!(f["monochromatic"], false);
        assert_eq!(f["seed"], 0);
        assert_eq!(f["__preview"], true);
        assert!(!f.contains_key("layer"));
        for param in parse_spec(r#"{"mapPath":text,"mapDocument":doc,"reference":layer id,"pins":json,"kernel":int[25],"id":u32=0,"foreground":json}"#) {
            assert!(durable_value(&param, &json!(12)).is_none(), "{}", param.key);
        }
        let fields = json!({"__command":"image.rotation.arbitrary","__filter":true,"angle":47,"direction":"cw"}).as_object().unwrap().clone();
        assert!(snapshot(&fields).is_none(), "derived ruler angle stays contextual");
    }

    #[test]
    fn remembered_filter_real_cancel_and_ok_buttons_publish_only_success() {
        use egui_kittest::kittest::Queryable;
        let mut h = egui_kittest::Harness::builder().with_size(egui::vec2(1100., 760.)).build_eframe(|cc| {
            PhotocraftApp::setup_context(&cc.egui_ctx, Default::default());
            app()
        });
        let id = open(h.state_mut(), NOISE).unwrap();
        h.state_mut().ui.dialog_mut(id).unwrap().fields.insert("__preview".into(), json!(false));
        h.run_steps(3);
        h.get_by_label("Monochromatic").click();
        h.run_steps(2);
        assert_eq!(h.state_mut().ui.dialog_mut(id).unwrap().fields["monochromatic"], true);
        h.get_by_label("Cancel").click();
        h.run_steps(3);
        assert!(saved(h.state(), NOISE).is_none());
        let id = open(h.state_mut(), NOISE).unwrap();
        h.state_mut().ui.dialog_mut(id).unwrap().fields.insert("__preview".into(), json!(false));
        h.run_steps(3);
        h.get_by_label("Monochromatic").click();
        h.run_steps(2);
        h.get_by_label("OK").click();
        h.run_steps(3);
        assert_eq!(saved(h.state(), NOISE).unwrap()["monochromatic"], true);
    }
    #[test]
    fn remembered_custom_grid_success_restart_cancel_and_failure_preserve_behavior() {
        const CUSTOM: &str = "filter.other.custom";
        let mut app = app();
        let pixel = |app: &PhotocraftApp| {
            let state = app.session.active().unwrap();
            state.doc.layer(state.active_layer.unwrap()).unwrap().surface().unwrap().pixel(4, 4)
        };
        let before = pixel(&app);
        let mut kernel = vec![0; 25];
        kernel[12] = 2;
        let id = open(&mut app, CUSTOM).unwrap();
        app.ui.dialog_mut(id).unwrap().fields.insert("kernel".into(), json!(kernel));
        crate::dialogs::confirm(&mut app, id).unwrap();
        let after = pixel(&app);
        // A doubled centre coefficient is an actual gain, not the identity default.
        for channel in 0..3 {
            assert!((after[channel] - (before[channel] * 2.).min(1.)).abs() <= 1. / 255. + f32::EPSILON);
        }
        assert!(after[0] > before[0]);
        assert_eq!(after[3], before[3]);
        let saved_choices = saved(&app, CUSTOM).unwrap();
        assert_eq!(saved_choices["kernel"], json!(kernel));
        let id = open(&mut app, CUSTOM).unwrap();
        assert_eq!(app.ui.dialog_mut(id).unwrap().fields["kernel"], json!(kernel));
        app.ui.dialog_mut(id).unwrap().fields.insert("kernel".into(), json!(vec![0; 25]));
        crate::dialogs::cancel(&mut app, id).unwrap();
        assert_eq!(saved(&app, CUSTOM), Some(saved_choices.clone()));
        assert_eq!(pixel(&app), after);
        let serialized = serde_json::to_string(&app.session.prefs().to_json()).unwrap();
        let mut session = photocraft_engine::Session::new();
        session.load_prefs_json(&serialized).unwrap();
        let mut restarted = PhotocraftApp::new(session, crate::Services::default());
        let id = open(&mut restarted, CUSTOM).unwrap();
        assert_eq!(restarted.ui.dialog_mut(id).unwrap().fields["kernel"], json!(kernel));
        assert!(crate::dialogs::confirm(&mut restarted, id).is_err(), "no document cannot publish a new matrix");
        assert_eq!(saved(&restarted, CUSTOM), Some(saved_choices));
        restarted.run("file.new", json!({"width":16,"height":12,"background":"transparent"})).unwrap();
        restarted.run("edit.fill", json!({"color":"#3366cc"})).unwrap();
        let id = open(&mut restarted, CUSTOM).unwrap();
        crate::dialogs::confirm(&mut restarted, id).unwrap();
        assert_eq!(pixel(&restarted), after, "reopened settings produce the same filter output");
    }

    #[test]
    fn remembered_custom_grid_requires_exact_bounded_integer_matrix() {
        const CUSTOM: &str = "filter.other.custom";
        let param = Param { key: "kernel".into(), kind: Kind::Grid(25) };
        let mut valid = vec![0; 25];
        valid[0] = -999;
        valid[24] = 999;
        assert_eq!(durable_value(&param, &json!(valid)), Some(json!(valid)));
        let mut too_high = valid.clone();
        too_high[12] = 1000;
        let mut too_low = valid.clone();
        too_low[12] = -1000;
        let mut fractional = json!(valid);
        fractional[12] = json!(0.5);
        let mut typed_float = json!(valid);
        typed_float[12] = json!(1.0);
        let mut missing = json!(valid);
        missing[12] = Value::Null;
        let mut oversized_integer = json!(valid);
        oversized_integer[12] = json!(u64::MAX);
        let bad = [
            json!(vec![0; 24]),
            json!(vec![0; 26]),
            json!(vec![vec![0; 5]; 5]),
            json!(too_high),
            json!(too_low),
            fractional,
            typed_float,
            missing,
            oversized_integer,
        ];
        let mut app = app();
        let identity: Vec<i64> = (0..25).map(|i| i64::from(i == 12)).collect();
        for matrix in bad {
            assert!(durable_value(&param, &matrix).is_none());
            app.session.edit_prefs(|p| p.dialogs.insert(format!("filter.dialog/{CUSTOM}"), json!({"kernel":matrix,"scale":2,"offset":5})));
            let before = saved(&app, CUSTOM);
            let id = open(&mut app, CUSTOM).unwrap();
            let fields = &app.ui.dialog_mut(id).unwrap().fields;
            assert_eq!(fields["kernel"], json!(identity));
            assert_eq!(fields["scale"], 2);
            assert_eq!(fields["offset"], 5);
            crate::dialogs::cancel(&mut app, id).unwrap();
            assert_eq!(saved(&app, CUSTOM), before, "read repair/cancel cannot overwrite stored choices");
        }
        for size in [0, 6, 289, usize::MAX] {
            let malformed = Param { key: "kernel".into(), kind: Kind::Grid(size) };
            assert!(durable_value(&malformed, &json!(valid)).is_none());
        }
    }
}
