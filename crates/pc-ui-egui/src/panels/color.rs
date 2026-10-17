//! The Color panel: colour field, hue strip, foreground/background chips and sRGB helpers.
use super::*;

#[cfg(test)]
pub(super) fn color_picker(app: &mut PhotocraftApp, ui: &mut egui::Ui) {
    color_picker_edit(app, ui, true);
}
pub(super) fn color_picker_edit(app: &mut PhotocraftApp, ui: &mut egui::Ui, notify_type: bool) {
    let fg = app.session.tools.foreground;
    // Keep the last edited HSB while it still gives the foreground: black and greys have no hue or
    // saturation of their own, so recomputing them from RGB would reset what was just typed to 0.
    let key = egui::Id::new("color-picker-hsva");
    let stored: Option<egui::ecolor::Hsva> = ui.data(|d| d.get_temp(key));
    let hsva0 = stored.filter(|h| h.to_srgb() == srgb_bytes(fg)).unwrap_or_else(|| srgb_hsva(fg));
    let mut h = hsva0.h * 360.0;
    let mut s = hsva0.s * 100.0;
    let mut v = hsva0.v * 100.0;
    let hue = widgets::hue_stops();
    let mut changed = widgets::slider_row(ui, tl!("Colour hue"), &mut h, 0.0..=360.0, "°", Some(&hue)).changed();
    let sat_stops =
        [egui::ecolor::Hsva::new(hsva0.h, 0.0, hsva0.v.max(0.2), 1.0), egui::ecolor::Hsva::new(hsva0.h, 1.0, hsva0.v.max(0.2), 1.0)].map(Color32::from);
    changed |= widgets::slider_row(ui, tl!("Colour saturation"), &mut s, 0.0..=100.0, "%", Some(&sat_stops)).changed();
    let val_stops = [Color32::BLACK, Color32::from(egui::ecolor::Hsva::new(hsva0.h, hsva0.s, 1.0, 1.0))];
    changed |= widgets::slider_row(ui, tl!("Colour brightness"), &mut v, 0.0..=100.0, "%", Some(&val_stops)).changed();
    let hsva = egui::ecolor::Hsva::new(h / 360.0, s / 100.0, v / 100.0, 1.0);
    // Only an edit counts: the h/s/v round trip isn't exact, so comparing values would rewrite
    // the foreground (and recolour selected type) every frame.
    if changed {
        app.session.tools.foreground = hsva_srgb(hsva);
        ui.data_mut(|d| d.insert_temp(key, hsva));
        if notify_type {
            crate::type_tool::foreground_changed(app);
        }
    }
    let [r, g, b, _] = hsva.to_srgba_unmultiplied();
    let t = Tokens::get(ui.ctx());
    ui.horizontal(|ui| {
        let (sw, _) = ui.allocate_exact_size(vec2(26.0, 26.0), Sense::hover());
        ui.painter().rect_filled(sw, 6.0, Color32::from_rgb(r, g, b));
        ui.label(RichText::new(format!("#{r:02X}{g:02X}{b:02X}")).font(theme::mono(12.5)).color(t.text));
        ui.label(RichText::new(format!("RGB {r} {g} {b}")).font(theme::mono(11.5)).color(t.text_faint));
    });
}

/// The Color panel's foreground and background chips. A click picks the colour the field edits,
/// framed; a double-click opens the Color Picker on it.
#[cfg(test)]
pub(super) fn field_chips(app: &mut PhotocraftApp, ui: &mut egui::Ui, chips: Rect) {
    let t = Tokens::get(ui.ctx());
    let bgr = Rect::from_min_size(chips.min + vec2(13.0, 13.0), vec2(22.0, 22.0));
    let fgr = Rect::from_min_size(chips.min + vec2(3.0, 3.0), vec2(22.0, 22.0));
    let frame = Stroke::new(1.0, t.text_dim);
    let bg_active = app.ui.color_panel.background;
    let p = ui.painter();
    p.rect_filled(bgr, 2.0, c32(app.session.tools.background));
    p.rect_stroke(bgr, 2.0, Stroke::new(1.0, t.field_border), StrokeKind::Outside);
    if bg_active {
        p.rect_stroke(bgr.expand(2.0), 2.0, frame, StrokeKind::Outside);
    }
    p.rect_filled(fgr, 2.0, c32(app.session.tools.foreground));
    p.rect_stroke(fgr, 2.0, Stroke::new(1.0, Color32::from_gray(210)), StrokeKind::Outside);
    if !bg_active {
        p.rect_stroke(fgr.expand(2.0), 2.0, frame, StrokeKind::Outside);
    }
    // The foreground is on top, so it takes the clicks where the two overlap.
    let bg_resp = ui.interact(bgr, ui.id().with("field-bg"), Sense::click());
    let fg_resp = ui.interact(fgr, ui.id().with("field-fg"), Sense::click());
    let picked = if fg_resp.clicked() { false } else { bg_resp.clicked() || bg_active };
    app.ui.color_panel.background = picked;
    if fg_resp.double_clicked() || bg_resp.double_clicked() {
        crate::color_picker_ui::open(app, if picked { "background" } else { "foreground" });
    }
}

/// Photoshop Color panel: saturation/brightness field + hue strip, drawn as shaded meshes.
#[cfg(test)]
pub(super) fn color_field(app: &mut PhotocraftApp, ui: &mut egui::Ui) {
    let t = Tokens::get(ui.ctx());
    let bg_active = app.ui.color_panel.background;
    let key = egui::Id::new(("color-field-hue", bg_active));
    let mut hsva = srgb_hsva(if bg_active { app.session.tools.background } else { app.session.tools.foreground });
    // Keep hue stable for greys (where RGB->HSV hue is undefined).
    let remembered: f32 = ui.data(|d| d.get_temp(key)).unwrap_or(hsva.h);
    if hsva.s < 0.01 || hsva.v < 0.01 {
        hsva.h = remembered;
    }
    let w = ui.available_width();
    let strip_w = 14.0;
    let h = 120.0;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 8.0;
        let (chips, _) = ui.allocate_exact_size(vec2(38.0, h), Sense::hover());
        field_chips(app, ui, chips);
        // SV field.
        let field_w = w - 38.0 - strip_w - 16.0;
        let (field, fresp) = ui.allocate_exact_size(vec2(field_w, h), Sense::click_and_drag());
        let mut mesh = egui::Mesh::default();
        let n = 16;
        for j in 0..=n {
            for i in 0..=n {
                let (sx, vy) = (i as f32 / n as f32, j as f32 / n as f32);
                let c = Color32::from(egui::ecolor::Hsva::new(hsva.h, sx, 1.0 - vy, 1.0));
                mesh.colored_vertex(pos2(field.left() + sx * field.width(), field.top() + vy * field.height()), c);
            }
        }
        for j in 0..n {
            for i in 0..n {
                let a = (j * (n + 1) + i) as u32;
                let b = a + 1;
                let c = a + (n + 1) as u32;
                let d = c + 1;
                mesh.add_triangle(a, b, d);
                mesh.add_triangle(a, d, c);
            }
        }
        ui.painter().add(mesh);
        ui.painter().rect_stroke(field, 0.0, Stroke::new(1.0, t.separator), StrokeKind::Outside);
        if (fresp.dragged() || fresp.clicked())
            && let Some(p) = fresp.interact_pointer_pos()
        {
            hsva.s = ((p.x - field.left()) / field.width()).clamp(0.0, 1.0);
            hsva.v = 1.0 - ((p.y - field.top()) / field.height()).clamp(0.0, 1.0);
        }
        let knob = pos2(field.left() + hsva.s * field.width(), field.top() + (1.0 - hsva.v) * field.height());
        ui.painter().circle_stroke(knob, 5.0, Stroke::new(1.5, Color32::WHITE));
        ui.painter().circle_stroke(knob, 6.5, Stroke::new(1.0, Color32::from_black_alpha(160)));
        // Hue strip.
        let (strip, sresp) = ui.allocate_exact_size(vec2(strip_w, h), Sense::click_and_drag());
        let mut m2 = egui::Mesh::default();
        let steps = 24;
        for k in 0..=steps {
            let f = k as f32 / steps as f32;
            let c = Color32::from(egui::ecolor::Hsva::new(1.0 - f, 1.0, 1.0, 1.0));
            m2.colored_vertex(pos2(strip.left(), strip.top() + f * strip.height()), c);
            m2.colored_vertex(pos2(strip.right(), strip.top() + f * strip.height()), c);
        }
        for k in 0..steps {
            let a = (k * 2) as u32;
            m2.add_triangle(a, a + 1, a + 3);
            m2.add_triangle(a, a + 3, a + 2);
        }
        ui.painter().add(m2);
        if (sresp.dragged() || sresp.clicked())
            && let Some(p) = sresp.interact_pointer_pos()
        {
            hsva.h = 1.0 - ((p.y - strip.top()) / strip.height()).clamp(0.0, 0.9999);
        }
        let y = strip.top() + (1.0 - hsva.h) * strip.height();
        let tri = vec![pos2(strip.right() + 1.0, y), pos2(strip.right() + 6.0, y - 4.0), pos2(strip.right() + 6.0, y + 4.0)];
        ui.painter().add(egui::Shape::convex_polygon(tri, t.text, Stroke::NONE));
        if fresp.dragged() || fresp.clicked() || sresp.dragged() || sresp.clicked() {
            if bg_active {
                app.session.tools.background = hsva_srgb(hsva);
            } else {
                app.session.tools.foreground = hsva_srgb(hsva);
                crate::type_tool::foreground_changed(app);
            }
            ui.data_mut(|d| d.insert_temp(key, hsva.h));
        }
    });
    let [r, g, b, _] = hsva.to_srgba_unmultiplied();
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        ui.label(RichText::new(format!("#{r:02X}{g:02X}{b:02X}")).font(theme::mono(12.0)).color(t.text));
        ui.label(RichText::new(format!("R {r}  G {g}  B {b}")).font(theme::mono(11.0)).color(t.text_faint));
    });
}

/// Tool colours are sRGB-encoded floats; egui's Hsva works on sRGB bytes via these helpers
/// (its `from_rgba_unmultiplied` expects *linear* RGB, which gave wrong readouts).
pub(super) fn srgb_bytes(c: [f32; 4]) -> [u8; 3] {
    let q = |v: f32| (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
    [q(c[0]), q(c[1]), q(c[2])]
}

pub(super) fn srgb_hsva(c: [f32; 4]) -> egui::ecolor::Hsva {
    egui::ecolor::Hsva::from_srgb(srgb_bytes(c))
}

pub(super) fn hsva_srgb(h: egui::ecolor::Hsva) -> [f32; 4] {
    let [r, g, b] = h.to_srgb();
    [r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, 1.0]
}
