//! A transient removal mask: painting is never a document edit.
use egui::{Color32, Key, Modifiers, Stroke, TextureHandle, vec2};
use photocraft_geom::Rect;
use serde_json::json;

use crate::{PhotocraftApp, Tool, canvas::ViewXform, theme::Tokens, widgets};

pub(crate) struct Pending {
    doc: u64,
    size: [u32; 2],
    mask: Vec<u8>,
    bounds: Rect,
    texture: Option<TextureHandle>,
    dirty: Rect,
    show_bar: bool,
    colourise_after: bool,
    undo: Vec<(Rect, Vec<u8>)>,
}

fn sync(app: &mut PhotocraftApp) {
    if app.ai_remove.as_ref().is_some_and(|p| {
        !matches!(app.ui.tool, Tool::AiRemove | Tool::RotateView)
            || !app.session.active().is_some_and(|d| d.doc.id.0 == p.doc && [d.doc.size.width, d.doc.size.height] == p.size)
    }) {
        app.ai_remove = None;
    }
}

fn new(app: &PhotocraftApp) -> Option<Pending> {
    let d = app.session.active()?;
    let size = [d.doc.size.width, d.doc.size.height];
    Some(Pending {
        undo: Vec::new(),
        doc: d.doc.id.0,
        size,
        mask: vec![0; size[0] as usize * size[1] as usize],
        bounds: Rect::EMPTY,
        texture: None,
        dirty: Rect::from_xywh(0, 0, size[0], size[1]),
        show_bar: true,
        colourise_after: false,
    })
}

fn scan_bounds(p: &Pending, area: Rect) -> Rect {
    let mut bounds = Rect::EMPTY;
    for y in area.y0..area.y1 {
        for x in area.x0..area.x1 {
            if p.mask[y as usize * p.size[0] as usize + x as usize] > 0 {
                bounds = bounds.union(&Rect::from_xywh(x, y, 1, 1));
            }
        }
    }
    bounds
}

fn changed(p: &mut Pending) {
    let full = Rect::from_xywh(0, 0, p.size[0], p.size[1]);
    p.bounds = scan_bounds(p, full);
    p.dirty = full;
    p.show_bar = true;
}

pub(crate) fn paint(app: &mut PhotocraftApp, points: &[[f64; 3]], subtract: bool) {
    sync(app);
    let brush = &app.session.tools.brush;
    let params = json!({"points": points, "size": brush.size, "hardness": brush.hardness * 100.0});
    let Ok((r, coverage)) = photocraft_engine::ai_cmds::stroke_mask(&app.session, &params, "ai.remove") else { return };
    if app.ai_remove.is_none() {
        app.ai_remove = new(app);
    }
    let Some(p) = app.ai_remove.as_mut() else { return };
    let clipped = r.intersect(&Rect::from_xywh(0, 0, p.size[0], p.size[1]));
    let previous = p.bounds;
    let mut before = Vec::new();
    for y in clipped.y0..clipped.y1 {
        before.extend_from_slice(&p.mask[y as usize * p.size[0] as usize + clipped.x0 as usize..y as usize * p.size[0] as usize + clipped.x1 as usize]);
    }
    let mut dirty = Rect::EMPTY;
    let mut removed_edge = false;
    for y in clipped.y0..clipped.y1 {
        for x in clipped.x0..clipped.x1 {
            let v = (coverage[(y - r.y0) as usize * r.width() as usize + (x - r.x0) as usize].clamp(0.0, 1.0) * 255.0).round() as u8;
            let pixel = &mut p.mask[y as usize * p.size[0] as usize + x as usize];
            let next = if subtract { pixel.saturating_sub(v) } else { (*pixel).max(v) };
            if next == *pixel {
                continue;
            }
            if next == 0 && (x == previous.x0 || x + 1 == previous.x1 || y == previous.y0 || y + 1 == previous.y1) {
                removed_edge = true;
            }
            *pixel = next;
            let cell = Rect::from_xywh(x, y, 1, 1);
            dirty = dirty.union(&cell);
            if next > 0 {
                p.bounds = p.bounds.union(&cell);
            }
        }
    }
    if dirty.is_empty() {
        if p.bounds.is_empty() {
            app.ai_remove = None;
        }
        return;
    }
    if removed_edge {
        p.bounds = scan_bounds(p, previous);
    }
    p.dirty = p.dirty.union(&dirty);
    p.show_bar = true;
    if !dirty.is_empty() {
        const UNDO_BYTES: usize = 32 << 20;
        while !p.undo.is_empty() && (p.undo.len() >= 32 || p.undo.iter().map(|(_, bytes)| bytes.len()).sum::<usize>() + before.len() > UNDO_BYTES) {
            p.undo.remove(0);
        }
        if before.len() <= UNDO_BYTES {
            p.undo.push((clipped, before));
        }
    }
    if p.bounds.is_empty() && p.undo.is_empty() {
        app.ai_remove = None;
    } else if app.ui.ai.remove_immediately && !subtract {
        remove(app);
    }
}

pub(crate) fn selection(app: &mut PhotocraftApp) {
    let Some(mask) = app.session.active().and_then(|d| photocraft_engine::ai_cmds::selection_gray(&d.doc)) else { return };
    app.ui.tool = Tool::AiRemove;
    app.ai_remove = new(app);
    if let Some(p) = &mut app.ai_remove {
        p.mask = mask.into_raw();
        changed(p);
    }
}

/// A learned detector's mask enters the same transient, editable confirmation as painting.
/// Even when immediate removal is enabled, importing a mask never submits a fill.
pub(crate) fn detected(app: &mut PhotocraftApp, mask: image::GrayImage, colourise_after: bool) -> Result<(), String> {
    let mut pending = new(app).ok_or("Open an image to repair.")?;
    if mask.dimensions() != (pending.size[0], pending.size[1]) {
        return Err("Scratch mask dimensions differ from the image.".into());
    }
    pending.mask = mask.into_raw();
    pending.colourise_after = colourise_after;
    changed(&mut pending);
    if pending.bounds.is_empty() {
        return Err("No scratches detected.".into());
    }
    app.ui.tool = Tool::AiRemove;
    app.ai_remove = Some(pending);
    Ok(())
}

fn ready(app: &PhotocraftApp) -> bool {
    let (m, v) = crate::ai_ui::remove_engine(&app.ui.ai.remove_engine);
    crate::ai_ui::status().ready(m, v).is_ok()
}

fn remove(app: &mut PhotocraftApp) {
    sync(app);
    if !ready(app) {
        return;
    }
    let Some(p) = app.ai_remove.take() else { return };
    let params = json!({"mask": p.mask, "engine": app.ui.ai.remove_engine});
    #[cfg(not(test))]
    let result = app.run("ai.remove", params);
    #[cfg(test)]
    let result = TEST_REMOVE.with(|mock| match mock.borrow_mut().as_mut() {
        Some(service) => service(app, &params),
        None => app.run("ai.remove", params),
    });
    match result {
        Err(error) => {
            app.ai_remove = Some(p);
            app.ui.status = error;
            app.ui.status_error = true;
        }
        Ok(value) if p.colourise_after => crate::restoration_ui::after_repair(app, &value, p.doc),
        Ok(_) => {}
    }
}

pub(crate) fn keys(app: &mut PhotocraftApp, ctx: &egui::Context) {
    sync(app);
    // A retained mask is dormant while navigating; its keys belong to Remove only.
    if app.ui.tool != Tool::AiRemove {
        return;
    }
    let painting = app.drag.as_ref().is_some_and(|d| d.tool == Tool::AiRemove);
    if (app.ai_remove.is_none() && !painting) || ctx.memory(|m| m.top_modal_layer().is_some()) {
        return;
    }
    if !ctx.text_edit_focused() && ctx.input_mut(|i| i.consume_key(Modifiers::COMMAND, Key::Z)) {
        if let Some(p) = &mut app.ai_remove
            && let Some((rect, before)) = p.undo.pop()
        {
            for (row, y) in (rect.y0..rect.y1).enumerate() {
                let start = y as usize * p.size[0] as usize + rect.x0 as usize;
                p.mask[start..start + rect.width() as usize].copy_from_slice(&before[row * rect.width() as usize..(row + 1) * rect.width() as usize]);
            }
            changed(p);
        }
        app.drag = None;
        app.trail = None;
    } else if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape)) {
        app.ai_remove = None;
        app.drag = None;
        app.trail = None;
    } else if app.drag.is_none() && !ctx.text_edit_focused() && ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Enter)) {
        remove(app);
    }
}

fn engine(app: &mut PhotocraftApp, ui: &mut egui::Ui) {
    let opts: Vec<_> = crate::ai_ui::REMOVE_ENGINES.iter().map(|(k, label, _, _)| ((*k).to_owned(), *label)).collect();
    widgets::dropdown(ui, "remove-engine", &mut app.ui.ai.remove_engine, &opts, 130.0);
}

fn actions(app: &mut PhotocraftApp, ui: &mut egui::Ui) {
    let can = ready(app);
    if ui.add_enabled_ui(can, |ui| widgets::primary_button(ui, tl!("Remove"), 0.0)).inner.clicked() {
        remove(app);
    }
    if widgets::secondary_button(ui, tl!("Keep painting"), 0.0).clicked()
        && let Some(p) = &mut app.ai_remove
    {
        p.show_bar = false;
    }
    if widgets::secondary_button(ui, tl!("Clear"), 0.0).clicked() || widgets::secondary_button(ui, tl!("Cancel"), 0.0).clicked() {
        app.ai_remove = None;
    }
}

pub(crate) fn options(app: &mut PhotocraftApp, ui: &mut egui::Ui) {
    sync(app);
    engine(app, ui);
    if app.ai_remove.is_some() {
        actions(app, ui);
    } else if app.session.active().is_some_and(|d| d.doc.selection.is_some()) && widgets::secondary_button(ui, tl!("Remove Selection"), 0.0).clicked() {
        selection(app);
    }
    widgets::checkbox(ui, &mut app.ui.ai.remove_immediately, tl!("Remove immediately on release"));
    ui.label(tl!("Paint to mark removal; Alt/Option-drag subtracts"));
    let (m, v) = crate::ai_ui::remove_engine(&app.ui.ai.remove_engine);
    crate::ai_ui::readiness(app, ui, m, v);
}

pub(crate) fn overlay(app: &mut PhotocraftApp, painter: &egui::Painter, xf: &ViewXform) {
    sync(app);
    let Some(p) = &mut app.ai_remove else { return };
    if !p.dirty.is_empty() {
        let area = if p.texture.is_some() { p.dirty } else { Rect::from_xywh(0, 0, p.size[0], p.size[1]) };
        let pixels = (area.y0..area.y1)
            .flat_map(|y| {
                p.mask[y as usize * p.size[0] as usize + area.x0 as usize..y as usize * p.size[0] as usize + area.x1 as usize]
                    .iter()
                    .map(|v| Color32::from_rgba_unmultiplied(255, 40, 60, (u16::from(*v) * 110 / 255) as u8))
            })
            .collect();
        let image = egui::ColorImage::new([area.width() as usize, area.height() as usize], pixels);
        if let Some(t) = &mut p.texture {
            t.set_partial([area.x0 as usize, area.y0 as usize], image, egui::TextureOptions::LINEAR);
        } else {
            p.texture = Some(painter.ctx().load_texture("remove-mask", image, egui::TextureOptions::LINEAR));
        }
        p.dirty = Rect::EMPTY;
    }
    if let Some(t) = &p.texture {
        let uv = egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0));
        let doc = egui::Rect::from_min_size(egui::Pos2::ZERO, vec2(p.size[0] as f32, p.size[1] as f32));
        painter.add(xf.camera().textured_quad(t.id(), doc, uv, Color32::WHITE));
    }
}

pub(crate) fn show(app: &mut PhotocraftApp, ctx: &egui::Context) {
    sync(app);
    let Some(p) = &app.ai_remove else { return };
    if !p.show_bar || app.drag.is_some() || ctx.memory(|m| m.top_modal_layer().is_some()) {
        return;
    }
    let Some(xf) = ViewXform::active(app) else { return };
    let r = xf.doc_rect(p.bounds);
    let id = egui::Id::new("remove-confirm");
    let size = ctx.memory(|m| m.area_rect(id)).map_or(vec2(490.0, 42.0), |r| r.size());
    let x = (r.center().x - size.x / 2.0).clamp(xf.rect.left() + 8.0, (xf.rect.right() - size.x - 8.0).max(xf.rect.left() + 8.0));
    let y = if r.bottom() + size.y + 12.0 < xf.rect.bottom() { r.bottom() + 12.0 } else { (r.top() - size.y - 12.0).max(xf.rect.top() + 8.0) };
    let pos = if ctx.input(|i| i.pointer.any_down()) { ctx.memory(|m| m.area_rect(id)).map_or(egui::pos2(x, y), |r| r.min) } else { egui::pos2(x, y) };
    let t = Tokens::get(ctx);
    egui::Area::new(id).order(egui::Order::Foreground).fixed_pos(pos).show(ctx, |ui| {
        egui::Frame::new().fill(t.card).stroke(Stroke::new(1.0, t.card_border)).corner_radius(t.radius).inner_margin(6).show(ui, |ui| {
            ui.horizontal(|ui| {
                actions(app, ui);
                engine(app, ui);
            });
        });
    });
}

#[cfg(test)]
thread_local! {
    static TEST_REMOVE: std::cell::RefCell<Option<Box<dyn FnMut(&mut PhotocraftApp, &serde_json::Value) -> Result<serde_json::Value, String>>>> = const { std::cell::RefCell::new(None) };
}

#[cfg(test)]
pub(crate) fn mask_for_test(app: &PhotocraftApp) -> Option<&[u8]> {
    app.ai_remove.as_ref().map(|p| p.mask.as_slice())
}

#[cfg(test)]
mod tests;
