//! The Preferences dialog body: section navigation, field widgets, GPU status and processing rows.
use super::*;

pub(super) fn humanize(key: &str) -> String {
    if key == "checkForUpdatesAtStartup" {
        return "Check for updates at startup".into();
    }
    if key == "tabletCompatibilityMode" {
        return "Pen tablet: use compatibility mode (XWayland)".into();
    }
    let mut s = String::new();
    for (i, ch) in key.chars().enumerate() {
        if i == 0 {
            s.extend(ch.to_uppercase());
        } else if ch.is_uppercase() {
            s.push(' ');
            s.extend(ch.to_lowercase());
        } else {
            s.push(ch);
        }
    }
    s.replace("Psd", "PSD").replace("Gpu", "GPU").replace("Ui ", "UI ").replace("Mb", "(MB)").replace("Exif", "EXIF").replace("Hud", "HUD")
}

pub(super) fn choice_label(v: &str) -> String {
    match v {
        "colour" => "Colour".into(),
        "monochrome" => "Monochrome (default)".into(),
        "cm" => "Centimeters".into(),
        "editedFolder" => "A copy in the Edited folder (originals untouched)".into(),
        "original" => "The original file".into(),
        "saveAs" => "Ask each time (Save As)".into(),
        "mm" => "Millimeters".into(),
        "rich" => "Rich (name, shortcut and how to use)".into(),
        "simple" => "Simple (name and shortcut)".into(),
        "75" | "100" | "125" | "150" | "175" | "200" | "250" | "300" => format!("{v}%"),
        "8" => "8 Bits/Channel".into(),
        "16" => "16 Bits/Channel".into(),
        "postScript" => "PostScript (72 points/inch)".into(),
        "traditional" => "Traditional (72.27 points/inch)".into(),
        "vulkan" => "Vulkan".into(),
        "dx12" => "DirectX 12".into(),
        "metal" => "Metal".into(),
        "gl" => "OpenGL".into(),
        "cpu" => "CPU (no GPU acceleration)".into(),
        v => humanize(v),
    }
}

/// An editable `#rrggbb` field beside a colour swatch (#668): typing a valid value sets the
/// colour; while it's being typed a partial value is kept, and it shows the colour otherwise.
pub(super) fn hex_field(ui: &mut egui::Ui, key: &str, rgb: &mut [u8; 3]) {
    let id = egui::Id::new(("pref-hex", key));
    let shown = format!("#{:02x}{:02x}{:02x}", rgb[0], rgb[1], rgb[2]);
    let mut text = ui.data(|d| d.get_temp::<String>(id)).unwrap_or_else(|| shown.clone());
    let r = ui.add(egui::TextEdit::singleline(&mut text).font(crate::theme::mono(11.5)).desired_width(72.0).char_limit(7));
    if r.changed()
        && let Some(c) = prefs::parse_hex(&text)
    {
        *rgb = c;
    }
    if r.has_focus() {
        ui.data_mut(|d| d.insert_temp(id, text));
    } else {
        ui.data_mut(|d| d.remove::<String>(id));
    }
}

pub(super) fn color_of(s: &str) -> Color32 {
    prefs::parse_hex(s).map_or(Color32::GRAY, |c| Color32::from_rgb(c[0], c[1], c[2]))
}

pub(super) fn prefs_nav_row(ui: &mut egui::Ui, title: &str, selected: bool, empty: bool) -> bool {
    crate::widgets::preferences_tab(ui, title, selected, empty).clicked()
}

/// Preferences: section list on the left, the section's settings on the right.
pub(super) fn prefs_body(app: &mut PhotocraftApp, ui: &mut egui::Ui, f: &mut Map<String, Value>) {
    let t = Tokens::get(ui.ctx());
    if let Some(error) = f.get("__error").and_then(Value::as_str) {
        ui.add(egui::Label::new(RichText::new(error).color(t.warning)).wrap());
        ui.add_space(6.0);
    }
    let mut section = f.get("section").and_then(Value::as_str).unwrap_or("general").to_string();
    let mut values = f.get("values").cloned().unwrap_or(Value::Null);
    // The dialog follows the language being edited, so a change shows before OK.
    let lang = li_i18n::Lang::from_pref(values.pointer("/interface/language").and_then(Value::as_str).unwrap_or("auto"));
    let available = (ui.ctx().content_rect().width() - 48.0).min(750.0);
    let nav_width = (available * 0.23).clamp(130.0, 170.0);
    let body_width = (available - nav_width - 24.0).max(280.0);
    let body_height = (ui.ctx().content_rect().height() - 210.0).clamp(160.0, 390.0);
    ui.horizontal_top(|ui| {
        // Section list.
        ui.vertical(|ui| {
            ui.set_width(nav_width);
            egui::ScrollArea::vertical().max_height(body_height).id_salt("prefs-navigation").show(ui, |ui| {
                for (id, title) in SECTIONS {
                    if id == "integrations" || (!app.host_preferences_pages.is_empty() && !["general", "interface", "performance"].contains(&id)) {
                        continue;
                    }
                    if ui.push_id(id, |ui| prefs_nav_row(ui, tl!(title), section == id, !has_visible_fields(&values, id))).inner {
                        section = id.to_string();
                    }
                }
                if !app.host_preferences_pages.is_empty() {
                    crate::widgets::hairline(ui);
                    crate::widgets::section_label(ui, "Library & Develop");
                    for (id, title) in app.host_preferences_pages {
                        let response = ui.push_id(id, |ui| crate::widgets::preferences_tab(ui, tl!(title), section == *id, false)).inner;
                        ui.ctx().data_mut(|data| data.insert_temp(egui::Id::new(("host-prefs-tab", *id)), response.rect));
                        if response.clicked() {
                            section = id.to_string();
                        }
                    }
                }
                if !app.host_preferences_pages.is_empty() {
                    crate::widgets::hairline(ui);
                    crate::widgets::section_label(ui, "Compositing");
                    for (id, title) in SECTIONS {
                        if ["general", "interface", "performance", "integrations"].contains(&id) {
                            continue;
                        }
                        if ui.push_id(id, |ui| prefs_nav_row(ui, tl!(title), section == id, !has_visible_fields(&values, id))).inner {
                            section = id.to_string();
                        }
                    }
                }
                ui.separator();
                crate::widgets::section_label(ui, "AI");
                for (id, title) in crate::ai_ui::settings_ui::PAGES {
                    if ui.push_id(id, |ui| prefs_nav_row(ui, tl!(title), section == id, false)).inner {
                        section = id.to_string();
                    }
                }
            });
            ui.add_space(6.0);
            if crate::widgets::secondary_button(ui, "Keyboard Shortcuts…", 0.0).clicked() {
                f.insert("openShortcuts".into(), json!(true));
            }
            crate::attributions::prefs_nav(ui, &mut section);
        });
        crate::widgets::vline(ui, body_height + 30.0);
        ui.vertical(|ui| {
            ui.set_width(body_width);
            if app.host_preferences_pages.iter().any(|(id, _)| *id == section) {
                if let Some(host) = app.host_preferences.as_mut() {
                    let draft = f.entry("hostValues").or_insert_with(|| host.values());
                    egui::ScrollArea::vertical().max_height(body_height).id_salt("host-prefs-scroll").show(ui, |ui| host.body(ui, &section, draft));
                }
                return;
            }
            if crate::ai_ui::settings_ui::page(app, ui, &section) {
                return;
            }
            if crate::attributions::prefs_page(ui, &section, f) {
                return;
            }
            let title = SECTIONS.iter().find(|(id, _)| *id == section).map_or("General", |(_, t)| *t);
            ui.label(RichText::new(tl!(&title)).font(crate::theme::semibold(14.0)).color(t.text));
            ui.add_space(6.0);
            egui::ScrollArea::vertical().max_height(body_height).id_salt("prefs-scroll").show(ui, |ui| {
                let order: Vec<String> =
                    f.get("__order").and_then(|o| o.get(&section)).and_then(|v| serde_json::from_value(v.clone()).ok()).unwrap_or_default();
                if section == "tools" {
                    let mut preset = serde_json::from_value(values["toolbar"]["vectorKeyboard"]["preset"].clone()).unwrap_or_default();
                    if crate::vector_integration::keyboard_preset(ui, &mut preset) {
                        values["toolbar"]["vectorKeyboard"]["preset"] = json!(preset);
                    }
                }
                if !has_visible_fields(&values, &section) {
                    ui.add_space(4.0);
                    ui.label(RichText::new(tl!("These settings aren't available in Local Image yet.")).color(t.text_faint));
                } else if let Some(obj) = values.get_mut(&section).and_then(Value::as_object_mut) {
                    section_fields(ui, &section, obj, &order, lang);
                    if section == "performance" {
                        gpu_status_rows(ui, f.get("__gpuInfo"), f.get("__gpuFailure").and_then(Value::as_str), obj);
                        processing_rows(ui, obj);
                    }
                    ui.add_space(8.0);
                }
                if has_visible_fields(&values, &section)
                    && crate::widgets::secondary_button(ui, tl!("Reset Section"), 110.0).clicked()
                    && let Some(def) = prefs::Preferences::default().get(&section)
                {
                    values[&section] = def;
                }
            });
        });
    });
    f.insert("section".into(), json!(section));
    f.insert("values".into(), values);
}

/// Preferences › Performance: what the app renders with now, and a reset of the GPU backend
/// (a crashed start may have moved it to a safer choice).
pub(super) fn gpu_status_rows(ui: &mut egui::Ui, info: Option<&Value>, failure: Option<&str>, obj: &mut Map<String, Value>) {
    let t = Tokens::get(ui.ctx());
    ui.add_space(10.0);
    ui.label(RichText::new(tl!("Graphics")).font(crate::theme::semibold(12.5)).color(t.text));
    // CPU mode because of a GPU failure: say so first, with the way back.
    if let Some(reason) = failure {
        ui.label(RichText::new(tl!("GPU acceleration is off after a graphics failure.")).color(t.warning));
        ui.label(RichText::new(reason).color(t.text_dim));
        let retry_chosen = rendering_mode_value(obj) != "cpu" && obj.get("gpuBackend").and_then(Value::as_str).unwrap_or("auto") == "auto";
        if !retry_chosen && crate::widgets::secondary_button(ui, tl!("Retry GPU"), 120.0).clicked() {
            obj.insert("renderingMode".into(), json!("gpu"));
            obj.insert("gpuBackend".into(), json!("auto"));
        }
        ui.add_space(4.0);
    }
    for line in info.and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str) {
        ui.label(RichText::new(line).color(t.text_dim));
    }
    ui.add_space(4.0);
    ui.label(RichText::new(tl!("Applies at next launch.")).color(t.text_faint));
    ui.collapsing(tl!("Advanced"), |ui| {
        ui.horizontal(|ui| {
            ui.label(tl!("GPU Backend"));
            let mut current = obj.get("gpuBackend").and_then(Value::as_str).unwrap_or("auto").to_string();
            let options = prefs::choices("performance.gpuBackend").unwrap_or(&[]);
            let labels: Vec<String> = options.iter().map(|o| choice_label(o)).collect();
            let pairs: Vec<(String, &str)> = options.iter().map(|o| o.to_string()).zip(labels.iter().map(String::as_str)).collect();
            crate::widgets::dropdown(ui, "graphics-backend", &mut current, &pairs, 220.0);
            obj.insert("gpuBackend".into(), json!(current));
        });
    });
}

/// local-image: new Processing labels, shown in English until the next translation pass.
pub(super) const PROCESSING: &str = "Processing";
pub(super) const PROCESSING_THREADS: &str = "Processing threads";
pub(super) const BACKGROUND_PRIORITY: &str = "Background work priority";
pub(super) const PROCESSING_MEMORY: &str = "Memory for processing";
pub(super) const DETECTED: &str = "Detected: {logical} logical / {physical} physical cores{hybrid}, {ram} GB RAM";

/// local-image: Preferences › Performance › Processing: optional overrides over the values the app
/// derives from the hardware at start-up (`lightcraft_sysmem::profile`; threading policy rules 1, 6
/// and 7). Every row defaults to Automatic and shows the automatic value. The compute pool is built
/// at launch, so changes apply then.
pub(super) fn processing_rows(ui: &mut egui::Ui, obj: &mut Map<String, Value>) {
    let t = Tokens::get(ui.ctx());
    let p = lightcraft_sysmem::profile();
    let hw = &p.hardware;
    let mut c: prefs::SystemResources = obj.get("system").and_then(|v| serde_json::from_value(v.clone()).ok()).unwrap_or_default();
    ui.add_space(10.0);
    ui.label(RichText::new(tl!(PROCESSING)).font(crate::theme::semibold(12.5)).color(t.text));
    let auto = |v: String| format!("{} ({v})", tl!("Automatic"));
    let threads: Vec<(u32, String)> =
        std::iter::once((0, auto(p.threads_auto.to_string()))).chain((1..=hw.logical.clamp(1, 1024) as u32).map(|n| (n, n.to_string()))).collect();
    let memory: Vec<(u32, String)> = std::iter::once((0, auto(format!("{} %", lightcraft_sysmem::profile::AUTO_MEMORY_PERCENT))))
        .chain((1..=9).map(|n| (n * 10, format!("{} %", n * 10))))
        .collect();
    let priority = [(prefs::BackgroundPriority::Low, tl!("Low").to_string()), (prefs::BackgroundPriority::Normal, tl!("Normal").to_string())];
    egui::Grid::new("prefs-processing").num_columns(2).spacing([14.0, 7.0]).show(ui, |ui| {
        ui.label(RichText::new(tl!(PROCESSING_THREADS)).color(t.text_dim));
        let opts: Vec<(u32, &str)> = threads.iter().map(|(n, l)| (*n, l.as_str())).collect();
        crate::widgets::dropdown(ui, "pref-processing-threads", &mut c.threads, &opts, 220.0);
        ui.end_row();
        ui.label(RichText::new(tl!(BACKGROUND_PRIORITY)).color(t.text_dim));
        let opts: Vec<(prefs::BackgroundPriority, &str)> = priority.iter().map(|(v, l)| (*v, l.as_str())).collect();
        crate::widgets::dropdown(ui, "pref-background-priority", &mut c.background_priority, &opts, 220.0);
        ui.end_row();
        ui.label(RichText::new(tl!(PROCESSING_MEMORY)).color(t.text_dim));
        let opts: Vec<(u32, &str)> = memory.iter().map(|(n, l)| (*n, l.as_str())).collect();
        crate::widgets::dropdown(ui, "pref-processing-memory", &mut c.memory_percent, &opts, 220.0);
        ui.end_row();
    });
    crate::widgets::checkbox(ui, &mut c.full_speed_on_battery, tl!("Keep full speed on battery"))
        .on_hover_text(tl!("Otherwise use fewer compute threads and pause indexing, Smart Sort and background preview builds on battery."));
    let hybrid = match (hw.perf_cores, hw.eff_cores) {
        (Some(pc), Some(ec)) => format!(" ({pc}P + {ec}E)"),
        _ => String::new(),
    };
    let detected = tl!(DETECTED)
        .replace("{logical}", &hw.logical.to_string())
        .replace("{physical}", &hw.physical.to_string())
        .replace("{hybrid}", &hybrid)
        .replace("{ram}", &format!("{:.0}", hw.ram_total as f64 / (1u64 << 30) as f64));
    ui.label(RichText::new(detected).color(t.text_dim));
    ui.label(RichText::new(tl!("Applies at next launch.")).color(t.text_faint));
    if let Ok(v) = serde_json::to_value(&c) {
        obj.insert("system".into(), v);
    }
}

/// One user-facing choice: Automatic, GPU or CPU / Compatibility.
pub(super) fn rendering_mode_row(ui: &mut egui::Ui, obj: &mut Map<String, Value>) {
    let mut current = rendering_mode_value(obj);
    ui.horizontal(|ui| {
        ui.label(tl!("Rendering Mode"));
        let pairs =
            vec![("auto".to_string(), tl!("Automatic (recommended)")), ("gpu".to_string(), tl!("GPU")), ("cpu".to_string(), tl!("CPU / Compatibility"))];
        let previous = current.clone();
        crate::widgets::dropdown(ui, "rendering-mode", &mut current, &pairs, 240.0);
        if current != previous {
            obj.insert("renderingMode".into(), json!(current));
        }
    });
    ui.label(tl!("Automatic uses GPU acceleration when available and falls back to CPU rendering on errors."));
    ui.add_space(8.0);
}

pub(super) fn rendering_mode_value(obj: &Map<String, Value>) -> String {
    obj.get("renderingMode").and_then(Value::as_str).unwrap_or("auto").to_string()
}

/// Does `section` have any setting the dialog shows (see [`prefs::HIDDEN_UNTIL_IMPLEMENTED`])?
pub(super) fn has_visible_fields(values: &Value, section: &str) -> bool {
    values.get(section).and_then(Value::as_object).is_some_and(|o| o.keys().any(|k| !prefs::is_hidden(&format!("{section}.{k}"))))
}

/// Generic editor for a section's fields: checkboxes, dropdowns for choices, colour swatches,
/// number fields with the preference's range, text fields.
pub(super) fn section_fields(ui: &mut egui::Ui, section: &str, obj: &mut Map<String, Value>, order: &[String], lang: li_i18n::Lang) {
    let t = Tokens::get(ui.ctx());
    if section == "interface" {
        let mut current = obj.get("toolIcons").and_then(Value::as_str).unwrap_or("colour").to_string();
        ui.horizontal(|ui| {
            ui.label(tl!("Tool icons"));
            let choices = [("monochrome".to_string(), tl!("Monochrome (default)")), ("colour".to_string(), tl!("Colour"))];
            crate::widgets::dropdown(ui, "pref-interface.toolIcons", &mut current, &choices, 220.0);
        });
        obj.insert("toolIcons".into(), json!(current));
        ui.add_space(8.0);
    }
    if section == "performance" {
        rendering_mode_row(ui, obj);
    }
    let mut keys: Vec<String> = order.iter().filter(|k| obj.contains_key(*k)).cloned().collect();
    keys.extend(obj.keys().filter(|k| !order.contains(k)).cloned());
    egui::Grid::new(("prefs-grid", section)).num_columns(2).spacing([14.0, 7.0]).show(ui, |ui| {
        for k in keys {
            let path = format!("{section}.{k}");
            // Settings nothing reads yet stay out of the dialog (issue #204); their stored values
            // pass through untouched.
            if (section == "interface" && k == "toolIcons")
                || prefs::is_hidden(&path)
                || (section == "performance" && matches!(k.as_str(), "gpuBackend" | "renderingMode"))
            {
                continue;
            }
            let v = obj.get(&k).cloned().unwrap_or(Value::Null);
            let human = humanize(&k);
            let label = tl!(&human).to_string();
            match &v {
                Value::Bool(b) => {
                    ui.label("");
                    let mut b = *b;
                    crate::widgets::checkbox(ui, &mut b, &label);
                    obj.insert(k, json!(b));
                }
                Value::String(s) if path == "tools.tabletCompatibilityMode" => {
                    ui.label(tl!("Pen tablet: use compatibility mode (XWayland)"))
                        .on_hover_text("Auto detects an attached pen and uses XWayland for pressure, tilt and eraser. On forces compatibility; Off keeps native Wayland. XWayland forces 8-bit display output (dithered); native Wayland can use 10-bit. XWayland can look softer at fractional display scaling. Restart to apply. LOCAL_IMAGE_NATIVE_WAYLAND=1 keeps native Wayland.");
                    let mut cur = s.clone();
                    crate::widgets::dropdown(ui, "pref-tools.tabletCompatibilityMode", &mut cur, &[("auto".into(), "Auto"), ("on".into(), "On"), ("off".into(), "Off")], 220.0);
                    obj.insert(k, json!(cur));
                }
                Value::String(s) if path == "interface.language" => {
                    ui.label(RichText::new(tl!(&label)).color(t.text_dim));
                    let mut pairs: Vec<(String, &str)> = vec![("auto".into(), tl!("Auto"))];
                    pairs.extend(li_i18n::Lang::ALL.into_iter().map(|l| (l.code().to_string(), l.name())));
                    let mut cur = s.clone();
                    crate::widgets::dropdown(ui, &format!("pref-{path}"), &mut cur, &pairs, 220.0);
                    obj.insert(k, json!(cur));
                }
                Value::String(s) if prefs::choices(&path).is_some() => {
                    ui.label(RichText::new(tl!(&label)).color(t.text_dim));
                    let opts = prefs::choices(&path).unwrap_or(&[]);
                    let labels: Vec<String> = opts.iter().map(|o| choice_label(o)).collect();
                    let pairs: Vec<(String, &str)> = opts.iter().map(|o| o.to_string()).zip(labels.iter().map(String::as_str)).collect();
                    let mut cur = s.clone();
                    crate::widgets::dropdown(ui, &format!("pref-{path}"), &mut cur, &pairs, 220.0);
                    obj.insert(k, json!(cur));
                }
                Value::String(s) if prefs::is_color(&path) => {
                    ui.label(RichText::new(tl!(&label)).color(t.text_dim));
                    let c = prefs::parse_hex(s).unwrap_or([128, 128, 128]);
                    let mut rgb = c;
                    ui.horizontal(|ui| {
                        ui.color_edit_button_srgb(&mut rgb);
                        hex_field(ui, &path, &mut rgb);
                    });
                    obj.insert(k, json!(format!("#{:02x}{:02x}{:02x}", rgb[0], rgb[1], rgb[2])));
                    let _ = color_of(s);
                }
                Value::String(s) => {
                    ui.label(RichText::new(tl!(&label)).color(t.text_dim));
                    let mut s = s.clone();
                    ui.add(egui::TextEdit::singleline(&mut s).desired_width(260.0));
                    obj.insert(k, json!(s));
                }
                Value::Number(n) => {
                    ui.label(RichText::new(tl!(&label)).color(t.text_dim));
                    let (lo, hi) = prefs::range(&path).unwrap_or((-1e9, 1e9));
                    if n.is_u64() || n.is_i64() {
                        let mut x = n.as_i64().unwrap_or(0);
                        ui.add(egui::DragValue::new(&mut x).range(lo as i64..=hi as i64).custom_parser(crate::widgets::parse_num));
                        obj.insert(k, json!(x));
                    } else {
                        let mut x = n.as_f64().unwrap_or(0.0);
                        ui.add(egui::DragValue::new(&mut x).range(lo..=hi).speed(0.1).max_decimals(3).custom_parser(crate::widgets::parse_num));
                        obj.insert(k, json!(x));
                    }
                }
                Value::Array(_) if path == "tools.penPressureCurve" => {
                    // local-image: a preset menu and a small curve graph.
                    ui.label(RichText::new(tl!(&label)).color(t.text_dim));
                    crate::pressure_curve_ui::editor(ui, obj, &k);
                }
                Value::Array(items) if k == "disks" => {
                    ui.label(RichText::new(tl!("Scratch disks")).color(t.text_dim));
                    let mut items = items.clone();
                    ui.vertical(|ui| {
                        for d in &mut items {
                            ui.horizontal(|ui| {
                                let mut on = d.get("enabled").and_then(Value::as_bool).unwrap_or(false);
                                let mut path = d.get("path").and_then(Value::as_str).unwrap_or("").to_string();
                                crate::widgets::checkbox(ui, &mut on, "");
                                ui.add(egui::TextEdit::singleline(&mut path).desired_width(240.0));
                                *d = json!({"enabled": on, "path": path});
                            });
                        }
                        if ui.small_button(tl!("Add disk")).clicked() {
                            items.push(json!({"enabled": true, "path": ""}));
                        }
                    });
                    obj.insert(k, Value::Array(items));
                }
                Value::Array(items) => {
                    ui.label(RichText::new(tl!(&label)).color(t.text_dim));
                    ui.label(RichText::new(li_i18n::trn(lang, items.len() as u64, "{n} item", "{n} items")).color(t.text_faint));
                    if !items.is_empty() && ui.small_button(tl!("Clear")).clicked() {
                        obj.insert(k, json!([]));
                    }
                }
                _ => continue,
            }
            ui.end_row();
        }
    });
}
