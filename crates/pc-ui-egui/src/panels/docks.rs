//! The right dock and floating panels: Info, Navigator, Swatches, Channels and History.
use super::*;

// Dock

pub fn right_dock(app: &mut PhotocraftApp, ui: &mut egui::Ui) {
    let t = Tokens::get(ui.ctx());
    let p = app.ui.panels.clone();
    if t.pro {
        dock_panels(app, ui, &t);
    }
    // Narrow icon rail (always visible): shows, expands or collapses panel groups.
    let (rw, rb) = if t.pro { (36.0, 28.0) } else { (44.0, 32.0) };
    egui::Panel::right("rail").resizable(false).exact_size(rw).frame(egui::Frame::NONE.fill(t.chrome).inner_margin(egui::Margin::symmetric(4, 8))).show(
        ui,
        |ui| {
            let r = ui.max_rect();
            ui.painter().line_segment([r.left_top() - vec2(6.0, 8.0), r.left_bottom() + vec2(-6.0, 8.0)], Stroke::new(1.0, t.separator));
            ui.spacing_mut().item_spacing.y = 4.0;
            use crate::dock::Group;
            let entries: [(&str, &str, Group); 6] = [
                ("sliders-horizontal", tl!("Properties"), Group::Properties),
                ("sparkles", "Generate", Group::Generate),
                ("navigation", tl!("Navigator"), Group::Navigator),
                ("palette", tl!("Color & Swatches"), Group::Color),
                ("layers", tl!("Layers"), Group::Layers),
                ("clock", tl!("History"), Group::History),
            ];
            for (icon, name, g) in entries {
                // Studio floats Properties outside the dock.
                let docked = t.pro || g != Group::Properties;
                let on = g.shown(&p) && !(docked && app.ui.dock.is_collapsed(g));
                if icons::rail_button(ui, icon, rb, on, name).clicked() {
                    crate::dock::rail_click(app, g, docked);
                }
            }
        },
    );
    if !t.pro {
        dock_panels(app, ui, &t);
    }
    crate::dock::persist(app, ui.ctx());
}

/// The right dock's width range (points).
pub(super) const DOCK_WIDTH: std::ops::RangeInclusive<f32> = 250.0..=520.0;

pub(super) fn dock_width_id() -> egui::Id {
    egui::Id::new("dock-width-request")
}

/// Set the right dock's width on the next frame (`ui.set {dockWidth}`); clamped to its range.
pub fn request_dock_width(ctx: &egui::Context, w: f32) {
    let w = if w.is_finite() { w.clamp(*DOCK_WIDTH.start(), *DOCK_WIDTH.end()) } else { *DOCK_WIDTH.start() };
    ctx.data_mut(|d| d.insert_temp(dock_width_id(), w));
}

pub(super) fn dock_panels(app: &mut PhotocraftApp, ui: &mut egui::Ui, t: &Tokens) {
    // Open groups that hold tabs (floating in Studio, Properties docks only in Pro).
    let groups = crate::dock::docked(app, t.pro);
    if groups.is_empty() {
        // Nothing docked: panels dragged to the dock's edge still dock there.
        let r = ui.available_rect_before_wrap();
        crate::dock::note_empty(ui.ctx(), egui::Rect::from_min_max(egui::pos2(r.right() - 2.0, r.top()), r.right_bottom()));
        return;
    }
    let margin = if t.pro { 2 } else { 8 };
    let mut panel = egui::Panel::right("dock").resizable(true).default_size(if t.pro { 290.0 } else { 300.0 }).size_range(DOCK_WIDTH);
    if let Some(w) = ui.ctx().data_mut(|d| d.remove_temp::<f32>(dock_width_id())) {
        panel = panel.exact_size(w);
    }
    panel.frame(egui::Frame::NONE.fill(t.dock).inner_margin(egui::Margin::same(margin))).show(ui, |ui| {
        // Groups keep their heights whatever they show (#88): see `dock`.
        crate::dock::show(app, ui, &groups, dock_body);
    });
}

/// Panels torn off the dock, floating over the canvas (see `dock`).
pub fn floating_panels(app: &mut PhotocraftApp, ctx: &egui::Context) {
    crate::dock::show_floating(app, ctx, dock_body);
}

/// One dock tab's content, docked or floating; `dock` bounds it and scrolls it when it's taller.
pub(super) fn dock_body(app: &mut PhotocraftApp, ui: &mut egui::Ui, tab: crate::dock::Tab) {
    use crate::dock::Tab;
    match tab {
        Tab::Gradients => crate::preset_panels::gradients_panel(app, ui),
        Tab::Patterns => crate::preset_panels::patterns_panel(app, ui),
        Tab::Color => swatches_panel::color_panel(app, ui),
        Tab::Swatches => swatches(app, ui),
        Tab::Properties => properties_body(app, ui),
        Tab::Adjustments => adjustments_grid(app, ui),
        Tab::Character => crate::type_tool::character_panel(app, ui, false),
        Tab::Paragraph => crate::type_tool::character_panel(app, ui, true),
        Tab::Navigator => navigator(app, ui),
        Tab::Histogram => crate::tone::histogram_panel(app, ui),
        Tab::Info => info_panel(app, ui),
        Tab::History => history(app, ui),
        Tab::Actions => crate::actions::panel(app, ui),
        Tab::LayerComps => crate::comps_ui::panel(app, ui),
        Tab::Layers => layers(app, ui),
        Tab::Channels => channels(app, ui),
        Tab::Paths => crate::vector_ui::paths_panel(app, ui),
        Tab::Symbols => {
            let ids = app.session.active().map(|s| s.selected_layers()).unwrap_or_default();
            crate::symbols::panel(ui, &mut app.session, &mut app.ui.symbols, &ids, |s| li_i18n::t(s).to_owned());
        }
        Tab::Generate => crate::generate_ui::panel(app, ui),
        Tab::Library => crate::generate_ui::library_panel(app, ui),
        tab => crate::vector_integration::dock_panel(app, ui, tab),
    }
}

/// The Info panel's ruler cells, shown while the Ruler tool is active and a ruler is drawn: the
/// angle and length(s), and the point and size the X/Y and W/H cells show instead of the pointer's
/// and the selection's.
#[derive(Clone, Debug, PartialEq)]
struct RulerRows {
    /// "A:" and then "L:" (a plain ruler) or "L1:" and "L2:" (a protractor), with their values.
    measure: Vec<(&'static str, String)>,
    /// The start point (a plain ruler) or the second arm's end (a protractor), in document pixels.
    point: [f64; 2],
    /// End minus start, signed and y down, for a plain ruler. A protractor's is not measured, so
    /// W/H keep showing the selection's size.
    size: Option<[f64; 2]>,
}

/// [`RulerRows`] for the active document, or `None` with another tool or no ruler. Lengths are in
/// the ruler unit with 2 decimals and the angle has 1 decimal and "°"; the options bar's Use
/// Measurement Scale never changes them.
fn ruler_rows(app: &PhotocraftApp) -> Option<RulerRows> {
    if app.ui.tool != Tool::Ruler {
        return None;
    }
    let doc = &app.session.active()?.doc;
    let r = doc.measurement.ruler?;
    // A unit scale: the Info panel reads pixels converted to the ruler unit, never the scale.
    let info = photocraft_engine::analysis_cmds::ruler_info(&r, &Default::default());
    let g = |k: &str| info[k].as_f64();
    let units = &app.session.prefs().units_and_rulers;
    let dpi = f64::from(doc.resolution_dpi);
    let length = |px: f64| {
        let v = units.rulers.from_px(px, dpi, f64::from(doc.size.width), units.point_size.per_inch());
        format!("{v:.2}")
    };
    let angle = format!("{:.1}°", g("angle")?);
    match r.protractor {
        None => Some(RulerRows { measure: vec![("A:", angle), ("L:", length(g("l1")?))], point: r.start, size: Some([g("w")?, g("h")?]) }),
        Some(q) => Some(RulerRows { measure: vec![("A:", angle), ("L1:", length(g("l1")?)), ("L2:", length(g("l2")?))], point: q, size: None }),
    }
}

/// Photoshop's Info panel: colour under the pointer (RGB and CMYK), position, selection size.
pub(super) fn info_panel(app: &mut PhotocraftApp, ui: &mut egui::Ui) {
    let t = Tokens::get(ui.ctx());
    let Some(st) = app.session.active() else {
        empty(ui, tl!("No document"));
        return;
    };
    let (rev, size) = (st.revision, st.doc.size);
    let cmyk_space = photocraft_compose::cmyk_space(&st.doc);
    // Positions and sizes in the ruler unit (Preferences › Units & Rulers).
    let units = app.session.prefs().units_and_rulers.clone();
    let dpi = st.doc.resolution_dpi as f64;
    let fx = move |px: f64| units.format(px, dpi, size.width as f64);
    let units_y = app.session.prefs().units_and_rulers.clone();
    let fy = move |px: f64| units_y.format(px, dpi, size.height as f64);
    let sel = st.doc.selection.as_ref().map(photocraft_compose::bounds::content_bounds).map(|r| [r.width() as f64, r.height() as f64]);
    let ruler = ruler_rows(app);
    let wh = ruler.as_ref().and_then(|r| r.size).or(sel);
    let pos = app
        .hover_doc
        .map(|p| (p[0].floor() as i32, p[1].floor() as i32))
        .filter(|(x, y)| *x >= 0 && *y >= 0 && *x < size.width as i32 && *y < size.height as i32);
    let rgba = pos.and_then(|(x, y)| {
        if let Some(((cx, cy, cr), v)) = app.info_sample
            && (cx, cy, cr) == (x, y, rev)
        {
            return Some(v);
        }
        let v: Vec<f32> = serde_json::from_value(app.session.execute("document.pixel", json!({"x": x, "y": y})).ok()?).ok()?;
        let v = [v[0], v[1], v[2], v[3]];
        app.info_sample = Some(((x, y, rev), v));
        Some(v)
    });
    let mono = theme::mono(11.5);
    let row = |ui: &mut egui::Ui, k: &str, v: String| {
        ui.horizontal(|ui| {
            ui.add_sized([18.0, 16.0], egui::Label::new(RichText::new(k).color(t.text_dim).size(11.5)));
            ui.label(RichText::new(v).font(mono.clone()).color(t.text));
        });
    };
    ui.columns(2, |cols| {
        let q = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as i32;
        let dash = || "—".to_string();
        row(&mut cols[0], "R:", rgba.map_or_else(dash, |c| q(c[0]).to_string()));
        row(&mut cols[0], "G:", rgba.map_or_else(dash, |c| q(c[1]).to_string()));
        row(&mut cols[0], "B:", rgba.map_or_else(dash, |c| q(c[2]).to_string()));
        let cmyk = rgba.map(|c| readout_cmyk(cmyk_space.as_ref(), [c[0], c[1], c[2]]));
        for (i, k) in ["C:", "M:", "Y:", "K:"].iter().enumerate() {
            row(&mut cols[1], k, cmyk.map_or_else(dash, |c| format!("{}%", c[i])));
        }
    });
    widgets::hairline(ui);
    // The Ruler tool's angle and length(s).
    if let Some(r) = &ruler {
        ui.columns(2, |cols| {
            for (i, (k, v)) in r.measure.iter().enumerate() {
                row(&mut cols[usize::from(i > 0)], k, v.clone());
            }
        });
        widgets::hairline(ui);
    }
    // With a ruler, X/Y are its point instead of the pointer's.
    let xy = ruler.as_ref().map(|r| r.point).or(pos.map(|p| [p.0 as f64, p.1 as f64]));
    ui.columns(2, |cols| {
        let dash = || "—".to_string();
        row(&mut cols[0], "X:", xy.map_or_else(dash, |p| fx(p[0])));
        row(&mut cols[0], "Y:", xy.map_or_else(dash, |p| fy(p[1])));
        row(&mut cols[1], "W:", wh.map_or_else(dash, |s| fx(s[0])));
        row(&mut cols[1], "H:", wh.map_or_else(dash, |s| fy(s[1])));
    });
    widgets::hairline(ui);
    ui.label(
        RichText::new(li_i18n::fmt(tl!("Doc: {w} × {h} px"), &[("w", &size.width.to_string()), ("h", &size.height.to_string())])).color(t.text_dim).size(11.5),
    );
}

/// Use the picker conversion in the document's CMYK space; HDR and NaN samples need a bounded readout.
fn readout_cmyk(space: Option<&std::sync::Arc<photocraft_color::convert::CmykSpace>>, rgb: [f32; 3]) -> [i32; 4] {
    let unit = |v: f32| if v.is_nan() { 0.0 } else { v.clamp(0.0, 1.0) };
    let ink = photocraft_color::convert::with_cmyk_space(space, || photocraft_color::convert::rgb_to_cmyk(rgb.map(unit)));
    ink.map(|v| (unit(v) * 100.0).round() as i32)
}

pub(super) fn navigator(app: &mut PhotocraftApp, ui: &mut egui::Ui) {
    let t = Tokens::get(ui.ctx());
    let Some(idx) = app.session.active_index() else {
        empty(ui, tl!("No document"));
        return;
    };
    let ctx = ui.ctx().clone();
    let Some(tex) = crate::canvas::navigator_texture(app, &ctx, idx) else { return };
    let size = app.session.documents()[idx].doc.size;
    let avail = ui.available_width();
    let box_h = 150.0;
    let (frame, resp) = ui.allocate_exact_size(vec2(avail, box_h), Sense::click_and_drag());
    ui.painter().rect_filled(frame, t.radius_sm, t.canvas);
    let aspect = size.width as f32 / size.height.max(1) as f32;
    let (w, h) = if aspect > avail / box_h { (avail - 16.0, (avail - 16.0) / aspect) } else { ((box_h - 16.0) * aspect, box_h - 16.0) };
    let rect = Rect::from_center_size(frame.center(), vec2(w, h));
    widgets::checker(ui.painter(), rect, 6.0);
    ui.painter().image(tex, rect, Rect::from_min_max(egui::Pos2::ZERO, pos2(1.0, 1.0)), Color32::WHITE);
    let s = w / size.width as f32;
    if (resp.clicked() || resp.dragged())
        && let Some(pp) = resp.interact_pointer_pos()
    {
        let d = (pp - rect.min) / s;
        app.ui.views[idx].center = [d.x.clamp(0.0, size.width as f32), d.y.clamp(0.0, size.height as f32)];
        app.ui.views[idx].fit_pending = false;
    }
    // Visible-area rectangle.
    let v = app.ui.views[idx].clone();
    let canvas = app.last_canvas_rect;
    let vw = canvas.width() / v.zoom * s;
    let vh = canvas.height() / v.zoom * s;
    let c = pos2(rect.min.x + v.center[0] * s, rect.min.y + v.center[1] * s);
    let vr = Rect::from_center_size(c, vec2(vw, vh)).intersect(frame.shrink(1.0));
    ui.painter().rect_stroke(vr, 2.0, Stroke::new(1.5, Color32::from_rgb(255, 84, 84)), StrokeKind::Middle);
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        label(ui, tl!("Zoom"));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            for (lbl, z) in [("200%", 2.0), ("100%", 1.0)] {
                if widgets::pill_tab(ui, lbl, (v.zoom - z).abs() < 1e-3).clicked() {
                    app.ui.views[idx].zoom = z;
                    app.ui.views[idx].fit_pending = false;
                }
            }
            if widgets::pill_tab(ui, tl!("Fit"), false).clicked() {
                app.ui.views[idx].fit_pending = true;
            }
        });
    });
    let mut lz = v.zoom.max(0.01).log2();
    if widgets::slider(ui, &mut lz, -6.64..=5.0, None).changed() {
        app.ui.views[idx].zoom = 2f32.powf(lz);
        app.ui.views[idx].fit_pending = false;
    }
}

pub(super) fn empty(ui: &mut egui::Ui, s: &str) {
    let t = Tokens::get(ui.ctx());
    ui.add_space(6.0);
    ui.label(RichText::new(s).color(t.text_faint));
    ui.add_space(6.0);
}

pub(super) const SWATCHES: [[u8; 3]; 40] = [
    [0, 0, 0],
    [26, 26, 26],
    [51, 51, 51],
    [77, 77, 77],
    [102, 102, 102],
    [128, 128, 128],
    [153, 153, 153],
    [179, 179, 179],
    [204, 204, 204],
    [255, 255, 255],
    [236, 128, 128],
    [244, 176, 132],
    [250, 224, 128],
    [214, 240, 128],
    [150, 232, 150],
    [128, 232, 200],
    [128, 220, 240],
    [128, 176, 244],
    [168, 144, 244],
    [232, 144, 232],
    [230, 40, 40],
    [245, 120, 30],
    [250, 210, 30],
    [160, 220, 40],
    [40, 200, 80],
    [30, 200, 170],
    [30, 170, 230],
    [40, 100, 230],
    [120, 70, 220],
    [210, 50, 180],
    [120, 20, 20],
    [130, 60, 10],
    [130, 110, 10],
    [80, 120, 20],
    [20, 100, 40],
    [10, 100, 90],
    [10, 80, 120],
    [20, 50, 120],
    [60, 30, 110],
    [110, 20, 90],
];

pub(super) fn swatches(app: &mut PhotocraftApp, ui: &mut egui::Ui) {
    swatches_panel::panel(app, ui);
}

pub(super) fn channels(app: &mut PhotocraftApp, ui: &mut egui::Ui) {
    crate::channels_panel::show(app, ui);
}

pub(super) fn history(app: &mut PhotocraftApp, ui: &mut egui::Ui) {
    let t = Tokens::get(ui.ctx());
    let Some(st) = app.session.active() else {
        empty(ui, tl!("No document"));
        return;
    };
    let doc = st.doc.clone();
    let entries = st.history.entries();
    let current = entries.len() - 1;
    let redo: Vec<String> = st.history.redo_labels().map(str::to_string).collect();
    let snapshots: Vec<String> = st.snapshots.iter().map(|s| s.name.clone()).collect();
    let mut snap_action: Option<(&'static str, usize)> = None;
    // Snapshot row (Photoshop shows the document's opening state with a thumbnail).
    {
        let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 38.0), Sense::hover());
        let thumb = Rect::from_min_size(pos2(rect.left() + 28.0, rect.center().y - 14.0), vec2(28.0, 28.0));
        let ctx = ui.ctx().clone();
        if let Some(layer) = doc.layers.first() {
            widgets::checker(ui.painter(), thumb, 4.0);
            let tex = app.layer_thumb(&ctx, &doc, layer);
            ui.painter().image(tex, thumb, Rect::from_min_max(egui::Pos2::ZERO, pos2(1.0, 1.0)), Color32::WHITE);
        }
        ui.painter().rect_stroke(thumb, 0.0, Stroke::new(1.0, t.separator), StrokeKind::Outside);
        ui.painter().text(pos2(thumb.right() + 8.0, rect.center().y), Align2::LEFT_CENTER, &doc.name, egui::FontId::proportional(12.0), t.text);
        ui.painter().line_segment([rect.left_bottom(), rect.right_bottom()], Stroke::new(1.0, t.separator));
    }
    // local-image: snapshots made with the camera button; click restores, right-click deletes.
    for (i, name) in snapshots.iter().enumerate() {
        let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 26.0), Sense::click());
        if resp.hovered() {
            ui.painter().rect_filled(rect, 0.0, t.hover.gamma_multiply(0.5));
        }
        icons::paint(ui, Rect::from_center_size(pos2(rect.left() + 42.0, rect.center().y), vec2(16.0, 16.0)), "scan", 12.0, t.icon);
        ui.painter().text(pos2(rect.left() + 58.0, rect.center().y), Align2::LEFT_CENTER, name, egui::FontId::proportional(12.0), t.text);
        let resp = resp.on_hover_text(tl!("Click to restore this snapshot"));
        if resp.clicked() {
            snap_action = Some(("history.restoreSnapshot", i));
        }
        resp.context_menu(|ui| {
            if ui.button(tl!("Delete Snapshot")).clicked() {
                snap_action = Some(("history.deleteSnapshot", i));
                ui.close();
            }
        });
        if i + 1 == snapshots.len() {
            ui.painter().line_segment([rect.left_bottom(), rect.right_bottom()], Stroke::new(1.0, t.separator));
        }
    }
    let mut target: Option<isize> = None;
    let all = entries.iter().map(|e| (e.clone(), false)).chain(redo.iter().map(|e| (e.clone(), true)));
    // The dock gives History a fixed height: the rows scroll above the footer.
    let footer = if t.pro { 34.0 } else { 0.0 };
    let max_h = (ui.available_height() - footer).max(40.0);
    egui::ScrollArea::vertical().id_salt("history-rows").max_height(max_h).auto_shrink([false, false]).show(ui, |ui| {
        for (i, (e, is_redo)) in all.enumerate() {
            let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 24.0), Sense::click());
            if i == current {
                ui.painter().rect_filled(rect, if t.pro { 0.0 } else { t.radius_sm }, if t.pro { t.row_selected } else { t.accent_soft });
            } else if resp.hovered() {
                ui.painter().rect_filled(rect, 0.0, t.hover.gamma_multiply(0.5));
            }
            let icon = match e.as_str() {
                s if s.contains("Brush") || s.contains("Eraser") => "brush",
                // The Layers panel's controls (layer.setProps).
                "Opacity Change" | "Fill Opacity Change" | "Blending Change" => "layers",
                s if s.contains("Layer") => "layers",
                s if s.contains("Select") || s.contains("Marquee") || s.contains("Deselect") || s.contains("Lasso") => "square-dashed",
                s if s.contains("Fill") => "paint-bucket",
                "Open" => "image",
                _ => "sliders-horizontal",
            };
            let secondary = if resp.hovered() { t.text_secondary_on_fill } else { t.text_faint };
            let color = if is_redo { secondary } else { t.text };
            icons::paint(
                ui,
                Rect::from_center_size(pos2(rect.left() + 42.0, rect.center().y), vec2(16.0, 16.0)),
                icon,
                12.0,
                if is_redo { secondary } else { t.icon },
            );
            ui.painter().text(pos2(rect.left() + 58.0, rect.center().y), Align2::LEFT_CENTER, &e, egui::FontId::proportional(12.0), color);
            if resp.clicked() {
                target = Some(i as isize - current as isize);
            }
        }
    });
    if t.pro {
        widgets::hairline(ui);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if icons::button(ui, "trash", 24.0, false, tl!("Delete current state")).clicked() {
                    snap_action = Some(("history.deleteState", 0));
                }
                if icons::button(ui, "scan", 24.0, false, tl!("Create new snapshot")).clicked() {
                    snap_action = Some(("history.newSnapshot", 0));
                }
                if icons::button(ui, "file-plus", 24.0, false, tl!("Create new document from current state")).clicked() {
                    snap_action = Some(("history.newDocument", 0));
                }
            });
        });
    }
    if let Some((cmd, i)) = snap_action.filter(|_| app.ui.transform.is_none())
        && let Err(e) = app.run(cmd, json!({ "index": i }))
    {
        app.ui.status = e;
        app.ui.status_error = true;
    }
    // An open Free Transform owns Undo (transform_tool::intercept): stepping the document's history under
    // its box would leave it transforming pixels that changed. A drawn crop frame holds the jumps
    // back for the same reason (crop_ui::holds_history).
    if let Some(delta) = target.filter(|_| app.ui.transform.is_none() && !crate::crop_ui::pending(app)) {
        let (cmd, n) = if delta < 0 { ("edit.undo", -delta) } else { ("edit.redo", delta) };
        for _ in 0..n {
            if app.run(cmd, json!({})).is_err() {
                break;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn info_cmyk_agrees_with_the_color_picker_conversion() {
        for rgb in [[1.0, 1.0, 1.0], [0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.5, 0.5, 0.5], [0.2, 0.6, 0.9], [1.0, 0.93, 0.0]] {
            let picker = photocraft_color::convert::rgb_to_cmyk(rgb).map(|v| (v * 100.0).round() as i32);
            assert_eq!(readout_cmyk(None, rgb), picker, "{rgb:?}");
        }
        assert_eq!(readout_cmyk(None, [2.0, 0.0, 0.0]), readout_cmyk(None, [1.0, 0.0, 0.0]));
        assert_eq!(readout_cmyk(None, [f32::NAN; 3]), readout_cmyk(None, [0.0; 3]));
        let in_range = |c: [i32; 4]| c.iter().all(|v| (0..=100).contains(v));
        for rgb in [[-0.2, 0.5, 0.5], [f32::NAN, 0.5, 0.5], [f32::INFINITY, f32::NEG_INFINITY, 0.5]] {
            assert!(in_range(readout_cmyk(None, rgb)), "{rgb:?}");
        }
    }

    /// An 800 × 600 document at 72 ppi with the Ruler tool active and rulers in pixels.
    fn ruler_app() -> PhotocraftApp {
        let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
        app.run("file.new", json!({"width": 800, "height": 600, "resolution": 72})).unwrap();
        app.run("prefs.set", json!({"path": "unitsAndRulers.rulers", "value": "pixels"})).unwrap();
        app.ui.tool = Tool::Ruler;
        app
    }

    fn measure(rows: &RulerRows) -> Vec<(&str, &str)> {
        rows.measure.iter().map(|(k, v)| (*k, v.as_str())).collect()
    }

    #[test]
    fn info_plain_ruler_shows_a_l_and_its_start_and_size() {
        let mut app = ruler_app();
        app.run("image.analysis.rulerTool", json!({"start": [100, 100], "end": [300, 250]})).unwrap();
        let r = ruler_rows(&app).expect("the ruler cells");
        assert_eq!(measure(&r), [("A:", "-36.9°"), ("L:", "250.00")]);
        assert_eq!((r.point, r.size), ([100.0, 100.0], Some([200.0, 150.0])));
    }

    #[test]
    fn info_protractor_shows_a_l1_l2_and_its_second_arm_end() {
        let mut app = ruler_app();
        app.run("image.analysis.rulerTool", json!({"start": [400, 300], "end": [600, 300], "protractor": [500, 200]})).unwrap();
        let r = ruler_rows(&app).expect("the ruler cells");
        assert_eq!(measure(&r), [("A:", "45.0°"), ("L1:", "200.00"), ("L2:", "141.42")]);
        assert_eq!((r.point, r.size), ([500.0, 200.0], None));
    }

    #[test]
    fn info_ruler_cells_ignore_the_measurement_scale() {
        let mut app = ruler_app();
        app.run("image.analysis.rulerTool", json!({"start": [100, 100], "end": [300, 250]})).unwrap();
        app.run("image.analysis.setMeasurementScale", json!({"pixelLength": 100, "logicalLength": 2, "units": "cm"})).unwrap();
        assert!((app.session.active().unwrap().doc.measurement.scale.factor() - 0.02).abs() < 1e-9, "the custom scale is set");
        app.ui.analysis.use_measurement_scale = true;
        let r = ruler_rows(&app).expect("the ruler cells");
        assert_eq!(measure(&r), [("A:", "-36.9°"), ("L:", "250.00")]);
        assert_eq!(r.size, Some([200.0, 150.0]));
    }

    #[test]
    fn info_another_tool_or_no_ruler_has_no_ruler_cells() {
        let mut app = ruler_app();
        assert_eq!(ruler_rows(&app), None, "no ruler");
        app.run("image.analysis.rulerTool", json!({"start": [100, 100], "end": [300, 250]})).unwrap();
        app.ui.tool = Tool::Brush;
        assert_eq!(ruler_rows(&app), None, "another tool");
    }

    #[test]
    fn info_panel_draws_the_ruler_cells() {
        use egui_kittest::Harness;
        use egui_kittest::kittest::Queryable;
        let mut a = ruler_app();
        a.run("image.analysis.rulerTool", json!({"start": [100, 100], "end": [300, 250]})).unwrap();
        let mut h = Harness::builder().with_size(egui::vec2(320.0, 400.0)).build_ui_state(|ui, app: &mut PhotocraftApp| info_panel(app, ui), a);
        PhotocraftApp::setup_context(&h.ctx, Default::default());
        h.run_steps(3);
        for text in ["A:", "-36.9°", "L:", "250.00", "W:", "200", "H:", "150"] {
            assert!(h.query_all_by_label(text).next().is_some(), "{text}");
        }
        // X and Y are the start point, 100 each.
        assert!(h.query_all_by_label("100").count() >= 2, "X 100 and Y 100");
    }
}
