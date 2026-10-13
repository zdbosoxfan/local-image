//! Develop layer selection. This bar only edits settings; rendering remains on preview workers.
//!
//! Laid out like Capture One's Layers tool: a compact list whose first row is Background, each
//! layer row with an eye, a mask thumbnail, its name, opacity and a ⋮ menu; one "+" in the list
//! header; the selected layer's opacity, mask and overlay controls under the list. Without
//! layers the whole bar is one line.
use crate::{
    LightcraftApp,
    icons::Icon,
    theme::Tokens,
    widgets::{icon_button, register, text_button},
};
use egui::{Align2, Color32, Rect, Sense, pos2, vec2};
use lightcraft_catalog::PhotoId;
use lightcraft_develop::{ControlSpec, Mask, MaskOp, MaskShape, Section, Track};
use serde_json::json;

pub const OPACITY: ControlSpec = ControlSpec {
    id: "opacity",
    label: "Opacity",
    section: Section::Light,
    min: 0.0,
    max: 100.0,
    default: 100.0,
    step: 1.0,
    decimals: 0,
    track: Track::Plain,
};

pub fn select_background(app: &mut LightcraftApp) {
    app.session.active_mask = None;
    app.ui.tool.clear();
    let _ = app.session.end_interaction();
    app.ui.pending_mask_shape = None;
    app.ui.pending_mask_origin = None;
    app.ui.renaming_mask = None;
}

/// A whole-image tool inside a layer's section (B&W, Vignette, Grain): one dimmed line, like
/// Capture One greys out tools a layer can't use, with the link back to Background.
pub fn global_only(app: &mut LightcraftApp, ui: &mut egui::Ui, key: &str, label: &str) {
    let t = Tokens::get(ui.ctx());
    egui::Frame::NONE.inner_margin(egui::Margin { left: 24, right: 22, top: 4, bottom: 4 }).show(ui, |ui| {
        ui.horizontal(|ui| {
            let name =
                ui.add(egui::Label::new(egui::RichText::new(crate::i18n::tr(label)).font(t.semibold(12.5)).color(t.text_disabled)).selectable(false));
            let tip = format!("{label} applies to the whole image. Select Background to edit it.");
            name.on_hover_text(&tip);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let r = background_link(ui).on_hover_text(&tip);
                register(ui.ctx(), format!("layerBackground:{key}"), r.rect);
                if r.clicked() {
                    select_background(app);
                }
                if ui.available_width() > 110.0 {
                    ui.label(egui::RichText::new(crate::i18n::tr("whole image only")).size(11.0).color(t.text_disabled));
                }
            });
        });
    });
}

/// The whole-image sections a layer can't hold (Profile & RAW, Negative, …), folded into ONE
/// line under the layer's tools: "▸ 5 whole-image tools · Select Background". Expanding lists
/// them; each name, like the link, selects Background.
pub fn global_group(app: &mut LightcraftApp, ui: &mut egui::Ui, tools: &[(&str, &str)]) {
    let t = Tokens::get(ui.ctx());
    let open_id = egui::Id::new("layerGlobalGroupOpen");
    let open = ui.data(|d| d.get_temp::<bool>(open_id)).unwrap_or(false);
    let names = tools.iter().map(|(_, l)| crate::i18n::tr(l)).collect::<Vec<_>>().join(", ");
    let tip = format!("{names}: these apply to the whole image. Select Background to edit them.");
    egui::Frame::NONE.inner_margin(egui::Margin { left: 16, right: 22, top: 8, bottom: 8 }).show(ui, |ui| {
        let title = format!("{} whole-image tools", tools.len());
        let font = t.semibold(12.5);
        let title_w = ui.painter().layout_no_wrap(title.clone(), font.clone(), t.text).size().x + 22.0;
        let link_w = ui.painter().layout_no_wrap(crate::i18n::tr("Select Background").to_string(), t.font(12.0), t.text).size().x + 8.0;
        // one line where it fits; a narrow dock puts the link under the title instead of overlapping
        let one_line = ui.available_width() >= title_w + link_w;
        let link = |app: &mut LightcraftApp, ui: &mut egui::Ui| {
            let r = background_link(ui).on_hover_text(&tip);
            register(ui.ctx(), "layerBackground:global", r.rect);
            if r.clicked() {
                select_background(app);
            }
        };
        ui.horizontal(|ui| {
            let (r, resp) = ui.allocate_exact_size(vec2(title_w.min(ui.available_width()), 22.0), Sense::click());
            resp.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::CollapsingHeader, true, open, &title));
            register(ui.ctx(), "layerGlobalGroup", r);
            let c = if resp.hovered() { t.text } else { t.text_dim };
            crate::icons::paint(
                ui.painter(),
                Rect::from_center_size(pos2(r.left() + 7.0, r.center().y), vec2(11.0, 11.0)),
                if open { Icon::ChevronDown } else { Icon::ChevronRight },
                c,
            );
            ui.painter().with_clip_rect(r).text(pos2(r.left() + 18.0, r.center().y), Align2::LEFT_CENTER, &title, font.clone(), c);
            if resp.on_hover_text(&tip).clicked() {
                ui.data_mut(|d| d.insert_temp(open_id, !open));
            }
            if one_line {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| link(app, ui));
            }
        });
        if !one_line {
            ui.horizontal(|ui| {
                ui.add_space(18.0);
                link(app, ui);
            });
        }
        if open {
            for (key, label) in tools {
                ui.horizontal(|ui| {
                    ui.add_space(18.0);
                    let r = ui
                        .link(egui::RichText::new(crate::i18n::tr(label)).size(12.0).color(t.text_dim))
                        .on_hover_text(format!("{label} applies to the whole image. Click to select Background."));
                    register(ui.ctx(), format!("layerBackground:{key}"), r.rect);
                    if r.clicked() {
                        select_background(app);
                    }
                });
            }
        }
    });
}

/// "Select Background" in the hyperlink colour (a sized RichText would bake the text colour).
fn background_link(ui: &mut egui::Ui) -> egui::Response {
    let c = ui.visuals().hyperlink_color;
    ui.link(egui::RichText::new(crate::i18n::tr("Select Background")).size(12.0).color(c))
}

pub fn new_menu(app: &mut LightcraftApp, ui: &mut egui::Ui) {
    for (kind, label) in [
        ("full", "New Filled Layer"),
        ("empty", "New Empty Layer"),
        ("brush", "Brush"),
        ("linear", "Linear Gradient"),
        ("radial", "Radial Gradient"),
        ("colorRange", "Color Range"),
        ("luminanceRange", "Luminance Range"),
        ("object", "AI Object"),
        ("prompt", "Describe…"),
        ("subject", "AI Subject"),
        ("sky", "AI Sky"),
        ("background", "AI Background"),
        ("depth", "Depth Range"),
    ] {
        if text_button(ui, &format!("maskNew:{kind}"), label, false).clicked() {
            app.ui.layer_bar_collapsed = false;
            app.ui.mask_overlay = true;
            if matches!(kind, "linear" | "radial") {
                let _ = app.run("develop.beginInteraction", json!({"label":label}));
            }
            app.ui.tool.clear();
            if kind == "object" {
                super::masking::start_object(app, ui.ctx(), "new");
            } else if kind == "prompt" {
                super::masking::start_describe(app, "new");
            } else if app.run("mask.add", json!({"kind":kind})).is_ok() {
                if matches!(kind, "brush" | "linear" | "radial" | "colorRange") {
                    app.ui.tool = kind.into();
                }
                if matches!(kind, "linear" | "radial") {
                    app.ui.pending_mask_shape = app.session.active_mask.map(|id| (id, 0));
                }
                app.ui.layer_components_open = kind == "empty";
                app.ui.layer_bar_collapsed = false;
                app.ui.mask_overlay = true;
            }
            ui.close();
        }
    }
}

fn menu(app: &mut LightcraftApp, ui: &mut egui::Ui, m: &lightcraft_develop::Mask) {
    for (label, cmd) in [("Invert", "mask.invert"), ("Duplicate", "mask.duplicate"), ("Delete", "mask.delete"), ("Reset", "mask.resetTools")] {
        if text_button(ui, &format!("layerMenu:{label}"), label, false).clicked() {
            let _ = app.run(cmd, json!({"id":m.id}));
            ui.close();
        }
    }
    if text_button(ui, "layerMenu:Rename", "Rename…", false).clicked() {
        app.ui.renaming_mask = Some((m.id, m.name.clone()));
        ui.close();
    }
}

/// A borderless icon button registered under `reg` (keeps the automation ids the old text
/// buttons had, e.g. `button:layerNew`).
fn small_button(ui: &mut egui::Ui, reg: &str, icon: Icon, size: egui::Vec2, tooltip: &str) -> egui::Response {
    let t = Tokens::get(ui.ctx());
    let tooltip = crate::i18n::tr(tooltip);
    let (r, resp) = ui.allocate_exact_size(size, Sense::click());
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, tooltip));
    register(ui.ctx(), reg.to_string(), r);
    if resp.hovered() || resp.has_focus() {
        ui.painter().rect_filled(r, 4.0, t.hover.gamma_multiply(0.8));
    }
    let g = (size.x.min(size.y) * 0.6).min(16.0);
    crate::icons::paint(ui.painter(), Rect::from_center_size(r.center(), vec2(g, g)), icon, if resp.hovered() { t.text } else { t.text_label });
    resp.on_hover_text(tooltip)
}

/// What a mask is, for the thumbnail tooltip.
fn kind(shape: &MaskShape) -> &'static str {
    match shape {
        MaskShape::Full => "Filled",
        MaskShape::Brush { .. } => "Brush",
        MaskShape::Linear { .. } => "Linear Gradient",
        MaskShape::Radial { .. } => "Radial Gradient",
        MaskShape::ColorRange { .. } => "Color Range",
        MaskShape::LuminanceRange { .. } => "Luminance Range",
        MaskShape::DepthRange { .. } => "Depth Range",
        MaskShape::Subject { .. } => "AI Subject",
        MaskShape::Sky { .. } => "AI Sky",
        MaskShape::Background { .. } => "AI Background",
        MaskShape::Object { .. } => "AI Object",
        MaskShape::Prompt { .. } => "Describe",
        MaskShape::People { .. } => "People",
        MaskShape::Landscape { .. } => "Landscape",
    }
}

fn smooth(e0: f64, e1: f64, x: f64) -> f64 {
    let t = ((x - e0) / (e1 - e0).max(1e-9)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Coverage 0..1 of a geometric mask at normalized `p` (the pipeline's formulas for Full,
/// Linear, Radial and Brush, in long-edge units). `None` for image-dependent masks.
fn coverage(m: &Mask, p: lightcraft_geom::Point, aspect: f64) -> Option<f64> {
    let (sx, sy) = if aspect >= 1.0 { (1.0, 1.0 / aspect) } else { (aspect, 1.0) };
    let long = |q: lightcraft_geom::Point| (q.x * sx, q.y * sy);
    let (px, py) = long(p);
    let mut alpha = 0.0;
    let mut first = true;
    for c in &m.components {
        let mut a = match &c.shape {
            MaskShape::Full => 1.0,
            MaskShape::Linear { start, end } => {
                let (a, b) = (long(*start), long(*end));
                let d = (b.0 - a.0, b.1 - a.1);
                let len2 = (d.0 * d.0 + d.1 * d.1).max(1e-12);
                1.0 - smooth(0.0, 1.0, ((px - a.0) * d.0 + (py - a.1) * d.1) / len2)
            }
            MaskShape::Radial { center, rx, ry, angle, feather, invert } => {
                let c = long(*center);
                let (s, co) = (-angle.to_radians()).sin_cos();
                let d = (px - c.0, py - c.1);
                let (x, y) = (d.0 * co - d.1 * s, d.0 * s + d.1 * co);
                let r = ((x / rx.max(1e-6)).powi(2) + (y / ry.max(1e-6)).powi(2)).sqrt();
                let a = 1.0 - smooth(1.0 - (feather / 100.0).clamp(0.01, 1.0), 1.0, r);
                if *invert { 1.0 - a } else { a }
            }
            MaskShape::Brush { strokes } => {
                let mut a: f64 = 0.0;
                for s in strokes {
                    // at most ~64 dabs per stroke: a thumbnail needs the shape, not every dab
                    let step = (s.points.len() / 64).max(1);
                    let size = s.size.max(1e-4);
                    let d = s.points.iter().step_by(step).map(|q| {
                        let q = long(*q);
                        ((px - q.0).powi(2) + (py - q.1).powi(2)).sqrt() / size
                    });
                    let v = d.fold(f64::MAX, f64::min);
                    let v = (1.0 - smooth(1.0 - (s.feather / 100.0).clamp(0.01, 1.0), 1.0, v)) * (s.flow / 100.0).clamp(0.0, 1.0).max(0.25);
                    a = if s.erase { a * (1.0 - v) } else { a.max(v) };
                }
                a
            }
            _ => return None,
        };
        if c.invert {
            a = 1.0 - a;
        }
        alpha = if first && c.op != MaskOp::Intersect {
            a
        } else {
            match c.op {
                MaskOp::Add => f64::max(alpha, a),
                MaskOp::Subtract => alpha * (1.0 - a),
                MaskOp::Intersect => alpha * a,
            }
        };
        first = false;
    }
    Some(if m.invert { 1.0 - alpha } else { alpha })
}

/// The mask thumbnail: white where the layer applies, black where it doesn't (Capture One and
/// Lightroom's convention), evaluated from the mask's own geometry on a small vertex grid, so it
/// costs microseconds and never waits for a render. Image-dependent masks (AI, ranges) show their
/// kind's icon instead; a layer without a mask is an empty frame.
fn mask_thumb(ui: &egui::Ui, r: Rect, m: Option<&Mask>, aspect: f64, visible: bool) {
    let t = Tokens::get(ui.ctx());
    let p = ui.painter();
    let frame_stroke = egui::Stroke::new(1.0, t.field_border);
    let Some(m) = m else {
        // Background: the photo itself
        p.rect_filled(r, 2.0, t.inset);
        crate::icons::paint(p, r.shrink2(vec2((r.width() - r.height()) / 2.0 + 2.0, 2.0)), Icon::Single, t.text_label);
        p.rect_stroke(r, 2.0, frame_stroke, egui::StrokeKind::Inside);
        return;
    };
    let fade = if visible { 1.0 } else { 0.45 };
    if m.components.is_empty() {
        p.rect_filled(r, 2.0, Color32::BLACK.gamma_multiply(fade));
        p.rect_stroke(r, 2.0, frame_stroke, egui::StrokeKind::Inside);
        return;
    }
    let probe = coverage(m, lightcraft_geom::Point::new(0.5, 0.5), aspect);
    if probe.is_none() {
        p.rect_filled(r, 2.0, Color32::from_gray(40).gamma_multiply(fade));
        let shape = &m.components[0].shape;
        crate::icons::paint_tool(ui, r.shrink(3.0), crate::icons::mask_name(shape), Icon::Mask, Color32::from_gray(225), visible);
        p.rect_stroke(r, 2.0, frame_stroke, egui::StrokeKind::Inside);
        return;
    }
    let (nx, ny) = (12usize, 8usize);
    let mut mesh = egui::Mesh::default();
    for j in 0..=ny {
        for i in 0..=nx {
            let (u, v) = (i as f64 / nx as f64, j as f64 / ny as f64);
            let a = coverage(m, lightcraft_geom::Point::new(u, v), aspect).unwrap_or(0.0).clamp(0.0, 1.0);
            let g = (a * 255.0).round() as u8;
            let pos = pos2(r.left() + r.width() * u as f32, r.top() + r.height() * v as f32);
            mesh.colored_vertex(pos, Color32::from_gray(g).gamma_multiply(fade));
        }
    }
    let w = (nx + 1) as u32;
    for j in 0..ny as u32 {
        for i in 0..nx as u32 {
            let a = j * w + i;
            mesh.add_triangle(a, a + 1, a + w);
            mesh.add_triangle(a + 1, a + w + 1, a + w);
        }
    }
    p.add(mesh);
    p.rect_stroke(r, 2.0, frame_stroke, egui::StrokeKind::Inside);
}

const ROW_H: f32 = 26.0;
const THUMB: egui::Vec2 = vec2(30.0, 20.0);

/// The Background row: styled like a layer row (no eye, opacity or menu: it can't hide).
fn background_row(app: &mut LightcraftApp, ui: &mut egui::Ui, aspect: f64, trailing_plus: bool) {
    let t = Tokens::get(ui.ctx());
    let selected = app.session.active_mask.is_none();
    // the "+" (24 pt) and the 3 pt gap before it stay inside the panel
    let plus_w = if trailing_plus { 27.0 } else { 0.0 };
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 3.0;
        let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width() - plus_w, ROW_H), Sense::click());
        resp.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Button, true, selected, crate::i18n::tr("Background")));
        register(ui.ctx(), "button:layerBackground", r);
        let fill = if selected {
            t.tool_active
        } else if resp.hovered() {
            t.hover.gamma_multiply(0.7)
        } else {
            Color32::TRANSPARENT
        };
        ui.painter().rect_filled(r, 4.0, fill);
        let thumb = Rect::from_min_size(pos2(r.left() + 25.0, r.center().y - THUMB.y / 2.0), THUMB);
        mask_thumb(ui, thumb, None, aspect, true);
        ui.painter().text(pos2(thumb.right() + 8.0, r.center().y), Align2::LEFT_CENTER, crate::i18n::tr("Background"), t.font(13.0), t.text);
        let resp = resp.on_hover_text(crate::i18n::tr("The whole photo: every tool, before the layers"));
        if resp.clicked() {
            app.ui.layer_bar_focus = true;
            select_background(app);
        }
        if trailing_plus {
            let plus = small_button(ui, "button:layerNew", Icon::Plus, vec2(24.0, 24.0), "New layer");
            egui::Popup::menu(&plus).show(|ui| new_menu(app, ui));
        }
    });
}

pub fn show(app: &mut LightcraftApp, ui: &mut egui::Ui, id: PhotoId) {
    app.session.resolve_active_mask();
    if app.ui.pending_mask_shape.is_some_and(|(mid, _)| app.session.active_mask != Some(mid)) {
        app.ui.pending_mask_shape = None;
        app.ui.pending_mask_origin = None;
    }
    let d = app.session.develop_of(id).unwrap_or_default();
    let t = Tokens::get(ui.ctx());
    let aspect = app.session.catalog.photo(id).filter(|p| p.width > 0 && p.height > 0).map_or(1.5, |p| p.width as f64 / p.height as f64);
    let frame = egui::Frame::NONE.inner_margin(egui::Margin::symmetric(12, 8)).show(ui, |ui| {
        if d.masks.is_empty() {
            // one line: Background … +
            background_row(app, ui, aspect, true);
        } else {
            // header: ▾ Layers (n) … +
            ui.horizontal(|ui| {
                let collapsed = app.ui.layer_bar_collapsed;
                ui.spacing_mut().item_spacing.x = 3.0;
                let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width() - 27.0, 24.0), Sense::click());
                resp.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::CollapsingHeader, true, !collapsed, crate::i18n::tr("Layers")));
                register(ui.ctx(), "button:layerCollapse", r);
                let c = if resp.hovered() { t.text } else { t.text_label };
                crate::icons::paint(
                    ui.painter(),
                    Rect::from_center_size(pos2(r.left() + 8.0, r.center().y), vec2(11.0, 11.0)),
                    if collapsed { Icon::ChevronRight } else { Icon::ChevronDown },
                    c,
                );
                let title = if collapsed {
                    // one line: which layer is being edited
                    let active = d.masks.iter().find(|m| Some(m.id) == app.session.active_mask).map(|m| m.name.as_str()).unwrap_or("Background");
                    format!("{} · {active}", crate::i18n::tr("Layers"))
                } else {
                    format!("{} ({})", crate::i18n::tr("Layers"), d.masks.len())
                };
                let galley = ui.painter().layout(title, t.semibold(12.5), c, r.width() - 22.0);
                ui.painter().with_clip_rect(r).galley(pos2(r.left() + 20.0, r.center().y - galley.size().y / 2.0), galley, c);
                if resp.on_hover_text(if collapsed { "Show the layer list" } else { "Hide the layer list" }).clicked() {
                    app.ui.layer_bar_collapsed = !collapsed;
                }
                let plus = small_button(ui, "button:layerNew", Icon::Plus, vec2(24.0, 24.0), "New layer");
                egui::Popup::menu(&plus).show(|ui| new_menu(app, ui));
            });
        }
        if !app.ui.layer_bar_collapsed && !d.masks.is_empty() {
            background_row(app, ui, aspect, false);
            for (index, m) in d.masks.iter().enumerate() {
                let selected = app.session.active_mask == Some(m.id);
                ui.push_id((id, m.id), |ui| {
                    let bg = if selected { t.tool_active } else { Color32::TRANSPARENT };
                    egui::Frame::NONE.fill(bg).corner_radius(4).inner_margin(egui::Margin::symmetric(0, 1)).show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 3.0;
                            if icon_button(
                                ui,
                                &format!("maskVisible:{}", m.id),
                                if m.visible { Icon::Eye } else { Icon::EyeOff },
                                vec2(22.0, 24.0),
                                false,
                                true,
                                "Show/hide layer",
                            )
                            .clicked()
                            {
                                let _ = app.run("mask.visible", json!({"id":m.id}));
                            }
                            let (tr, tresp) = ui.allocate_exact_size(THUMB + vec2(2.0, 4.0), Sense::click());
                            register(ui.ctx(), format!("layerThumb:{}", m.id), tr);
                            mask_thumb(ui, Rect::from_center_size(tr.center(), THUMB), Some(m), aspect, m.visible);
                            let what = m.components.iter().map(|c| kind(&c.shape)).collect::<Vec<_>>().join(" + ");
                            let tip = if what.is_empty() {
                                "No mask yet: add one with Mask ▾".to_string()
                            } else {
                                format!("Mask: {what}{}", if m.invert { " (inverted)" } else { "" })
                            };
                            if tresp.on_hover_text(tip).clicked() {
                                app.ui.layer_bar_focus = true;
                                let _ = app.session.end_interaction();
                                let _ = app.run("mask.select", json!({"id":m.id}));
                                app.ui.tool.clear();
                                app.ui.pending_mask_shape = None;
                            }
                            let room = (ui.available_width() - 72.0).max(30.0);
                            if let Some((_, name)) = app.ui.renaming_mask.as_mut().filter(|(id, _)| *id == m.id) {
                                let r = ui.add_sized(vec2(room, 24.0), egui::TextEdit::singleline(name).id_salt("rename"));
                                register(ui.ctx(), format!("maskRename:{}", m.id), r.rect);
                                if !app.host_modal && !r.has_focus() && !r.lost_focus() {
                                    r.request_focus();
                                }
                                let cancel = ui.input(|i| i.key_pressed(egui::Key::Escape));
                                if cancel || r.lost_focus() {
                                    let name = name.trim().to_string();
                                    app.ui.renaming_mask = None;
                                    if !cancel && !name.is_empty() && name != m.name {
                                        let _ = app.run("mask.rename", json!({"id":m.id,"name":name}));
                                    }
                                }
                            } else {
                                let label = if m.components.is_empty() { format!("{} (no mask)", m.name) } else { m.name.clone() };
                                let (nr, r) = ui.allocate_exact_size(vec2(room, 24.0), Sense::click_and_drag());
                                r.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Button, true, selected, &label));
                                let color = if m.visible { t.text } else { t.text_disabled };
                                let galley = egui::WidgetText::from(egui::RichText::new(&label).font(t.font(13.0)).color(color)).into_galley(
                                    ui,
                                    Some(egui::TextWrapMode::Truncate),
                                    nr.width() - 4.0,
                                    egui::TextStyle::Body,
                                );
                                ui.painter().galley(pos2(nr.left() + 2.0, nr.center().y - galley.size().y / 2.0), galley, color);
                                if r.has_focus() {
                                    ui.painter().rect_stroke(nr, 3.0, egui::Stroke::new(1.0, t.accent), egui::StrokeKind::Inside);
                                }
                                let r = r.on_hover_text(&m.name);
                                register(ui.ctx(), format!("mask:{}", m.id), nr);
                                if r.clicked() {
                                    app.ui.layer_bar_focus = true;
                                    let _ = app.session.end_interaction();
                                    let _ = app.run("mask.select", json!({"id":m.id}));
                                    app.ui.tool.clear();
                                    app.ui.pending_mask_shape = None;
                                    r.request_focus();
                                }
                                if r.double_clicked() || (r.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter))) {
                                    app.ui.renaming_mask = Some((m.id, m.name.clone()));
                                }
                                if r.drag_started() {
                                    app.ui.dragging_layer = Some(m.id);
                                }
                                if let Some(mid) = app.ui.dragging_layer
                                    && mid != m.id
                                    && r.contains_pointer()
                                    && ui.input(|i| i.pointer.any_released())
                                {
                                    app.ui.dragging_layer = None;
                                    if let Some(old) = d.masks.iter().position(|m| m.id == mid) {
                                        let _ = app.run("mask.move", json!({"id":mid,"delta":index as i64-old as i64}));
                                    }
                                }
                                if r.has_focus() {
                                    let delta = ui.input(|i| {
                                        if i.key_pressed(egui::Key::ArrowDown) {
                                            1
                                        } else if i.key_pressed(egui::Key::ArrowUp) {
                                            -1
                                        } else {
                                            0
                                        }
                                    });
                                    if delta != 0 {
                                        let next = (index as i32 + delta).clamp(-1, d.masks.len() as i32 - 1);
                                        if next < 0 {
                                            select_background(app);
                                        } else {
                                            let _ = app.run("mask.select", json!({"id":d.masks[next as usize].id}));
                                        }
                                    }
                                }
                                r.context_menu(|ui| menu(app, ui, m));
                            }
                            let mut opacity = m.opacity;
                            let r = ui.add_sized(
                                vec2(44.0, 22.0),
                                egui::DragValue::new(&mut opacity).range(0.0..=100.0).speed(1.0).suffix("%").max_decimals(0),
                            );
                            register(ui.ctx(), format!("layerOpacity:{}", m.id), r.rect);
                            if r.drag_started() {
                                let _ = app.run("develop.beginInteraction", json!({"label":"Opacity","mask":m.id}));
                            }
                            if r.changed() {
                                let _ = app.run("mask.setOpacity", json!({"id":m.id,"value":opacity}));
                            }
                            if r.drag_stopped() {
                                let _ = app.run("develop.endInteraction", json!({}));
                            }
                            let more = small_button(ui, &format!("button:layerMenu:{}", m.id), Icon::Dots, vec2(18.0, 24.0), "Layer options");
                            egui::Popup::menu(&more).show(|ui| menu(app, ui, m));
                        });
                    });
                });
            }
        }
        if let Some(mid) = app.ui.deleting_layer {
            ui.label("Delete this layer?");
            ui.horizontal(|ui| {
                if text_button(ui, "layerDeleteConfirm", "Delete", false).clicked() {
                    let _ = app.run("mask.delete", json!({"id":mid}));
                    app.ui.deleting_layer = None;
                }
                if text_button(ui, "layerDeleteCancel", "Cancel", false).clicked() {
                    app.ui.deleting_layer = None;
                }
            });
        }
        super::masking::describe_field(app, ui, true);
        // the selected layer's controls, under the list: opacity, then Mask ▾ · Overlay · filter
        if let Some(m) = d.masks.iter().find(|m| Some(m.id) == app.session.active_mask) {
            ui.add_space(2.0);
            let out =
                crate::widgets::numeric_slider(ui, &OPACITY, m.opacity, true, None, crate::widgets::numeric_owner(app, ("layer-opacity", m.id)));
            super::edit::apply_slider_out(app, &OPACITY, out, |app, v| app.run("mask.setOpacity", json!({"id":m.id,"value":v})));
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                let plus = text_button(ui, &format!("maskPlus:{}", m.id), "Mask ▾", false)
                    .on_hover_text("Add to, subtract from or intersect this layer's mask");
                if app.ui.layer_components_open {
                    egui::Popup::open_id(ui.ctx(), egui::Popup::default_response_id(&plus));
                    app.ui.layer_components_open = false;
                }
                egui::Popup::menu(&plus).show(|ui| {
                    for op in ["add", "subtract", "intersect"] {
                        let response = ui.menu_button(
                            match op {
                                "add" => "Add",
                                "subtract" => "Subtract",
                                _ => "Intersect",
                            },
                            |ui| super::masking::component_menu(app, ui, op),
                        );
                        register(ui.ctx(), format!("layerOp:{op}"), response.response.rect);
                    }
                    if text_button(ui, "layerIntersectLuma", "Intersect with Luminance Range", false).clicked() {
                        let _ = app.run("mask.addComponent", json!({"kind":"luminanceRange","op":"intersect"}));
                        ui.close();
                    }
                    if text_button(ui, "layerEditMask", "Edit Mask…", false).clicked() {
                        app.ui.right = crate::state::RightPanel::Masking;
                        ui.close();
                    }
                });
                let overlay = text_button(ui, "maskOverlay", "Overlay", app.ui.mask_overlay)
                    .on_hover_text("Show the mask on the photo (O), with its overlay options");
                if overlay.clicked() {
                    app.ui.mask_overlay = !app.ui.mask_overlay;
                }
                egui::Popup::menu(&overlay).show(|ui| super::masking::overlay_options(app, ui));
                let r = ui.checkbox(&mut app.ui.layer_used_only, crate::i18n::tr("Only used tools"));
                register(ui.ctx(), "layerUsedOnly", r.rect);
                r.on_hover_text("Show only tools this layer uses");
            });
            super::masking::describe_field(app, ui, false);
        }
        if ui.input(|i| i.pointer.any_released()) {
            app.ui.dragging_layer = None;
        }
    });
    register(ui.ctx(), "layerBar", frame.response.rect);
    if app.ui.tool == "brush" {
        super::masking::brush_settings(app, ui);
    }
}

pub fn canvas_pill(app: &LightcraftApp, ui: &mut egui::Ui, canvas: Rect) {
    if let Some(name) = app
        .session
        .active()
        .and_then(|id| app.session.develop_of(id))
        .and_then(|d| d.masks.iter().find(|m| Some(m.id) == app.session.active_mask).map(|m| m.name.clone()))
    {
        let t = Tokens::get(ui.ctx());
        let galley = ui.painter().layout_no_wrap(name, t.font(12.0), t.text);
        let r = Rect::from_min_size(canvas.min + vec2(12.0, 12.0), vec2((galley.size().x + 20.0).min(canvas.width() - 24.0), 26.0));
        ui.painter().rect_filled(r, 13.0, t.chrome);
        ui.painter().with_clip_rect(r.shrink(5.0)).text(
            pos2(r.left() + 10.0, r.center().y),
            Align2::LEFT_CENTER,
            galley.job.text.clone(),
            t.font(12.0),
            t.text,
        );
        register(ui.ctx(), "activeLayerPill", r);
    }
}

/// Layer-list navigation owns these keys while the list has focus.
pub fn keyboard(app: &mut LightcraftApp, ctx: &egui::Context) {
    if !app.ui.layer_bar_focus || app.ui.right != crate::state::RightPanel::Edit || app.ui.renaming_mask.is_some() || app.ui.dialog.is_some() {
        return;
    }
    if ctx.input(|i| i.pointer.any_pressed())
        && ctx.pointer_interact_pos().is_some_and(|p| !app.widgets.iter().any(|(id, r)| id == "layerBar" && r.contains(p)))
    {
        app.ui.layer_bar_focus = false;
        return;
    }
    let Some(d) = app.session.active().and_then(|id| app.session.develop_of(id)) else {
        return;
    };
    let current = d.masks.iter().position(|m| Some(m.id) == app.session.active_mask).map(|i| i as i32).unwrap_or(-1);
    let delta = ctx.input_mut(|i| {
        if i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown) {
            1
        } else if i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp) {
            -1
        } else {
            0
        }
    });
    if delta != 0 {
        let next = (current + delta).clamp(-1, d.masks.len() as i32 - 1);
        let _ = app.session.end_interaction();
        app.ui.tool.clear();
        app.ui.pending_mask_shape = None;
        app.ui.pending_mask_origin = None;
        app.session.active_mask = if next < 0 { None } else { Some(d.masks[next as usize].id) };
    }
    if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Enter))
        && let Some(m) = d.masks.iter().find(|m| Some(m.id) == app.session.active_mask)
    {
        app.ui.renaming_mask = Some((m.id, m.name.clone()));
    }
    if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Delete)) {
        app.ui.deleting_layer = app.session.active_mask;
    }
}

#[cfg(test)]
mod thumb_tests {
    use super::*;
    use lightcraft_develop::{BrushStroke, MaskComponent};
    use lightcraft_geom::Point;

    fn mask(shapes: Vec<(MaskOp, MaskShape)>) -> Mask {
        Mask {
            components: shapes.into_iter().map(|(op, shape)| MaskComponent { name: None, op, invert: false, shape }).collect(),
            ..Default::default()
        }
    }

    #[test]
    fn thumbnail_coverage_follows_the_mask_geometry() {
        let p = Point::new;
        let full = mask(vec![(MaskOp::Add, MaskShape::Full)]);
        assert_eq!(coverage(&full, p(0.1, 0.9), 1.5), Some(1.0));
        let radial = mask(vec![(MaskOp::Add, MaskShape::Radial { center: p(0.5, 0.5), rx: 0.2, ry: 0.2, angle: 0.0, feather: 10.0, invert: false })]);
        assert_eq!(coverage(&radial, p(0.5, 0.5), 1.5), Some(1.0));
        assert_eq!(coverage(&radial, p(0.0, 0.0), 1.5), Some(0.0));
        let linear = mask(vec![(MaskOp::Add, MaskShape::Linear { start: p(0.0, 0.5), end: p(1.0, 0.5) })]);
        assert!(coverage(&linear, p(0.0, 0.5), 1.5).unwrap() > 0.99 && coverage(&linear, p(1.0, 0.5), 1.5).unwrap() < 0.01);
        let stroke = BrushStroke { points: vec![p(0.2, 0.5), p(0.8, 0.5)], size: 0.05, ..Default::default() };
        let brush = mask(vec![(MaskOp::Add, MaskShape::Brush { strokes: vec![stroke] })]);
        assert!(coverage(&brush, p(0.8, 0.5), 1.5).unwrap() > 0.9 && coverage(&brush, p(0.5, 0.1), 1.5).unwrap() < 0.01);
        // a subtracted radial punches a hole in a filled layer; invert flips the result
        let mut hole = mask(vec![
            (MaskOp::Add, MaskShape::Full),
            (MaskOp::Subtract, MaskShape::Radial { center: p(0.5, 0.5), rx: 0.2, ry: 0.2, angle: 0.0, feather: 10.0, invert: false }),
        ]);
        assert_eq!(coverage(&hole, p(0.5, 0.5), 1.5), Some(0.0));
        assert_eq!(coverage(&hole, p(0.05, 0.05), 1.5), Some(1.0));
        hole.invert = true;
        assert_eq!(coverage(&hole, p(0.5, 0.5), 1.5), Some(1.0));
        // image-dependent masks don't pretend: the row shows their kind icon instead
        assert_eq!(coverage(&mask(vec![(MaskOp::Add, MaskShape::Sky { seg: None })]), p(0.5, 0.5), 1.5), None);
    }
}
