//! local-image: Edit › Transform › Cage on the canvas (GIMP's and Krita's cage transform).
//!
//! Two phases: **drawing** the cage (click to add points; click the first point or press ↩ to
//! close it; ⌫ removes the last point), then **deforming** (drag the cage's points; the content
//! inside follows through Green or mean value coordinates). ↩ / Commit runs
//! `edit.transform.cage` with the cage and its targets (one history step, works on pixel layers
//! and smart objects); Esc cancels.
//!
//! The preview while deforming is the layer without its caged part plus the caged part on a
//! textured mesh mapped through the same `CageMap` the engine uses.

use std::sync::Arc;

use egui::{Color32, Stroke, TextureHandle, pos2};
use photocraft_doc::{Document, LayerId};
use photocraft_geom::Rect;
use photocraft_geom::cage::{CageCoords, CageMap, point_in_polygon};
use photocraft_raster::Surface;
use serde_json::{Value, json};

use crate::PhotocraftApp;
use crate::canvas::{ToolEvent, ViewXform};

/// Grid cells per side of the preview mesh.
const PREVIEW_CELLS: usize = 48;
/// Longest side of the preview textures.
const TEX_SIDE: usize = 2048;

/// Textures of the caged part and the rest (built when the cage closes).
struct Previews {
    inside: TextureHandle,
    outside: TextureHandle,
}

pub struct CageSession {
    pub key: u64,
    pub layer: LayerId,
    surface: Surface,
    bounds: Rect,
    /// The cage as drawn (document coordinates).
    pub cage: Vec<[f64; 2]>,
    /// Where each cage point goes (equal to `cage` until a point is dragged).
    pub target: Vec<[f64; 2]>,
    /// Drawing (false) or deforming (true).
    pub closed: bool,
    pub coords: CageCoords,
    pub selected: Option<usize>,
    drag: Option<(usize, [f64; 2])>,
    map: Option<CageMap>,
    previews: Option<Previews>,
    opacity: f32,
    pub preview_doc: Arc<Document>,
    /// The last pointer position over the canvas (the rubber band while drawing).
    pub hover: Option<[f64; 2]>,
}

impl CageSession {
    pub fn describe(&self) -> Value {
        json!({
            "layer": self.layer.0,
            "cage": self.cage,
            "target": self.target,
            "closed": self.closed,
            "coordinates": self.coords.id(),
            "selected": self.selected,
        })
    }

    fn rebuild_map(&mut self) {
        self.map = CageMap::new(&self.cage, &self.target, self.coords).ok();
    }

    fn point_at(pts: &[[f64; 2]], p: [f64; 2], tol: f64) -> Option<usize> {
        pts.iter()
            .enumerate()
            .map(|(i, q)| (i, (q[0] - p[0]).hypot(q[1] - p[1])))
            .filter(|(_, d)| *d <= tol)
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(i, _)| i)
    }

    /// Adds a cage point (drawing phase). A point that would make the cage cross itself is
    /// refused.
    fn add_point(&mut self, p: [f64; 2]) -> Result<(), String> {
        if !p[0].is_finite() || !p[1].is_finite() {
            return Err("bad point".into());
        }
        // The new edge may not cross the edges drawn so far.
        if let Some(&last) = self.cage.last() {
            let n = self.cage.len();
            if (0..n.saturating_sub(2)).any(|i| crosses(self.cage[i], self.cage[i + 1], last, p)) {
                return Err(tl!("The cage can't cross itself").into());
            }
        }
        let mut next = self.cage.clone();
        next.push(p);
        self.cage = next;
        self.target = self.cage.clone();
        Ok(())
    }

    /// Closes the cage and starts deforming.
    fn close(&mut self, ctx: Option<&egui::Context>) -> Result<(), String> {
        if self.closed {
            return Ok(());
        }
        if self.cage.len() < 3 {
            return Err(tl!("A cage needs at least three points").into());
        }
        CageMap::new(&self.cage, &self.cage, self.coords).map_err(|e| e.to_string())?;
        self.closed = true;
        self.target = self.cage.clone();
        self.rebuild_map();
        if let Some(ctx) = ctx {
            self.previews = Some(self.build_previews(ctx));
        }
        Ok(())
    }

    /// The layer image split by the cage (alpha × coverage, alpha × (1 − coverage)).
    fn build_previews(&self, ctx: &egui::Context) -> Previews {
        let img = crate::distort_ui::surface_image(&self.surface, self.bounds, TEX_SIDE);
        let [tw, th] = img.size;
        let (sx, sy) = (tw as f64 / f64::from(self.bounds.width().max(1)), th as f64 / f64::from(self.bounds.height().max(1)));
        let poly: Vec<[f64; 2]> = self.cage.iter().map(|p| [(p[0] - f64::from(self.bounds.x0)) * sx, (p[1] - f64::from(self.bounds.y0)) * sy]).collect();
        let cov = photocraft_algo::cage::polygon_coverage(&poly, Rect::new(0, 0, tw as i32, th as i32));
        let split = |inside: bool| {
            let px = img
                .pixels
                .iter()
                .zip(&cov)
                .map(|(c, k)| {
                    let k = if inside { *k } else { 1.0 - *k };
                    let [r, g, b, a] = c.to_srgba_unmultiplied();
                    Color32::from_rgba_unmultiplied(r, g, b, (f32::from(a) * k).round() as u8)
                })
                .collect();
            egui::ColorImage::new([tw, th], px)
        };
        Previews {
            inside: ctx.load_texture(format!("cage-in-{}", self.key), split(true), egui::TextureOptions::LINEAR),
            outside: ctx.load_texture(format!("cage-out-{}", self.key), split(false), egui::TextureOptions::LINEAR),
        }
    }

    /// The command params for the current cage.
    pub fn params(&self) -> Value {
        json!({"layer": self.layer.0, "cage": self.cage, "target": self.target, "coordinates": self.coords.id()})
    }
}

/// Do segments `a–b` and `c–d` cross properly?
fn crosses(a: [f64; 2], b: [f64; 2], c: [f64; 2], d: [f64; 2]) -> bool {
    let cr = |o: [f64; 2], p: [f64; 2], q: [f64; 2]| (p[0] - o[0]) * (q[1] - o[1]) - (p[1] - o[1]) * (q[0] - o[0]);
    let (d1, d2, d3, d4) = (cr(c, d, a), cr(c, d, b), cr(a, b, c), cr(a, b, d));
    d1 * d2 < 0.0 && d3 * d4 < 0.0
}

/// Starts Cage on the active layer.
pub fn begin(app: &mut PhotocraftApp) -> Result<(), String> {
    if app.ui.transform.is_some() {
        return Err("finish Free Transform first".into());
    }
    let (layer, surface, _) = crate::distort_ui::active_pixels(app)?;
    let st = app.session.active().ok_or("no document")?;
    let l = st.doc.layer(layer).ok_or("no layer")?;
    let bounds = surface.content_bounds();
    if bounds.is_empty() {
        return Err(tl!("Could not use Cage: the layer is empty").into());
    }
    let opacity = l.opacity * l.fill_opacity;
    let preview_doc = crate::distort_ui::without_layer(&st.doc, layer);
    let key = app.ui.alloc_id();
    app.distort.cage = Some(CageSession {
        key,
        layer,
        surface,
        bounds,
        cage: Vec::new(),
        target: Vec::new(),
        closed: false,
        coords: CageCoords::Green,
        selected: None,
        drag: None,
        map: None,
        previews: None,
        opacity,
        preview_doc,
        hover: None,
    });
    app.ui.status = tl!("Click to draw a cage around the content; click the first point or press Enter to close it").into();
    app.ui.status_error = false;
    Ok(())
}

pub fn commit(app: &mut PhotocraftApp) {
    let Some(s) = app.distort.cage.take() else { return };
    if !s.closed || s.cage == s.target {
        return;
    }
    if let Err(e) = app.run("edit.transform.cage", s.params()) {
        app.ui.status = e;
        app.ui.status_error = true;
    }
}

/// Control channel: `edit.transform.cage {"ui": {...}}` while the mode is active:
/// `{"point": [x, y]}` adds a cage point, `{"close": true}`, `{"move": {"index": i, "to": [x, y]}}`,
/// `{"coordinates": "green|meanValue"}`, `{"reset": true}`, `{"commit": true}`, `{"cancel": true}`.
pub fn control(app: &mut PhotocraftApp, ctx: &egui::Context, ui: &Value) -> Result<Value, String> {
    if ui.get("commit").and_then(Value::as_bool) == Some(true) {
        commit(app);
        return Ok(json!({"committed": true}));
    }
    if ui.get("cancel").and_then(Value::as_bool) == Some(true) {
        app.distort.cage = None;
        return Ok(json!({"cancelled": true}));
    }
    let s = app.distort.cage.as_mut().ok_or(tl!("Cage is not active"))?;
    let xy = |v: &Value| Some([v.get(0)?.as_f64()?, v.get(1)?.as_f64()?]);
    if let Some(p) = ui.get("point").and_then(xy) {
        if s.closed {
            return Err(tl!("The cage is closed: drag its points").into());
        }
        s.add_point(p)?;
    }
    if ui.get("close").and_then(Value::as_bool) == Some(true) {
        s.close(Some(ctx))?;
    }
    if let Some(m) = ui.get("move") {
        let i = m.get("index").and_then(Value::as_u64).ok_or("move: {index, to}")? as usize;
        let to = m.get("to").and_then(xy).ok_or("move: {index, to}")?;
        if !s.closed || i >= s.target.len() {
            return Err("no such cage point".into());
        }
        s.target[i] = to;
        s.rebuild_map();
    }
    if let Some(c) = ui.get("coordinates").and_then(Value::as_str) {
        s.coords = CageCoords::parse(c).ok_or("coordinates: green|meanValue")?;
        s.rebuild_map();
    }
    if ui.get("reset").and_then(Value::as_bool) == Some(true) {
        s.target = s.cage.clone();
        s.rebuild_map();
    }
    Ok(s.describe())
}

pub fn pointer(app: &mut PhotocraftApp, ev: ToolEvent) {
    let tol = crate::distort_ui::tolerance(app);
    let mut status: Option<String> = None;
    let Some(s) = app.distort.cage.as_mut() else { return };
    match ev {
        ToolEvent::Down { x, y, .. } => {
            let p = [x, y];
            if !s.closed {
                if s.cage.len() >= 3 && CageSession::point_at(&s.cage[..1], p, tol).is_some() {
                    // The preview textures are built by the options bar on the next frame.
                    if let Err(e) = s.close(None) {
                        status = Some(e);
                    }
                } else if let Err(e) = s.add_point(p) {
                    status = Some(e);
                }
            } else if let Some(i) = CageSession::point_at(&s.target, p, tol) {
                s.selected = Some(i);
                s.drag = Some((i, [s.target[i][0] - x, s.target[i][1] - y]));
            } else {
                s.selected = None;
            }
        }
        ToolEvent::Move { x, y, .. } => {
            s.hover = Some([x, y]);
            if let Some((i, off)) = s.drag {
                s.target[i] = [x + off[0], y + off[1]];
                s.rebuild_map();
            }
        }
        ToolEvent::Up { x, y } => {
            if let Some((i, off)) = s.drag.take() {
                s.target[i] = [x + off[0], y + off[1]];
                s.rebuild_map();
            }
        }
    }
    if let Some(e) = status {
        app.ui.status = e;
        app.ui.status_error = true;
    }
}

/// Keys while Cage is active: ↩ closes the cage (drawing) or commits (deforming), Esc cancels,
/// ⌫ removes the last cage point while drawing.
pub fn keys(app: &mut PhotocraftApp, ctx: &egui::Context) {
    use egui::{Key, Modifiers};
    let pressed = |k: Key| ctx.input_mut(|i| i.consume_key(Modifiers::NONE, k));
    if pressed(Key::Escape) {
        app.distort.cage = None;
        return;
    }
    let Some(s) = app.distort.cage.as_mut() else { return };
    if pressed(Key::Enter) {
        if s.closed {
            commit(app);
        } else if let Err(e) = s.close(Some(ctx)) {
            app.ui.status = e;
            app.ui.status_error = true;
        }
    } else if !s.closed && pressed(Key::Backspace) {
        s.cage.pop();
        s.target = s.cage.clone();
    }
}

pub fn draw(s: &CageSession, painter: &egui::Painter, xf: &ViewXform) {
    let scr = |p: [f64; 2]| xf.to_screen(p[0] as f32, p[1] as f32);
    let accent = crate::theme::Tokens::get(painter.ctx()).accent;
    if let (true, Some(pv), Some(map)) = (s.closed, &s.previews, &s.map) {
        let b = s.bounds;
        let (bw, bh) = (f64::from(b.width()), f64::from(b.height()));
        let uv = |x: f64, y: f64| pos2(((x - f64::from(b.x0)) / bw) as f32, ((y - f64::from(b.y0)) / bh) as f32);
        let tint = Color32::from_white_alpha((s.opacity.clamp(0.0, 1.0) * 255.0) as u8);
        // The rest of the layer, unmoved.
        let mut rest = egui::Mesh::with_texture(pv.outside.id());
        let r = egui::Rect::from_min_max(scr([f64::from(b.x0), f64::from(b.y0)]), scr([f64::from(b.x1), f64::from(b.y1)]));
        rest.add_rect_with_uv(r, egui::Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), tint);
        painter.add(rest);
        // The caged part on a grid over the cage's box (within the layer), mapped forward.
        let cb = map.source_bounds();
        let (x0, y0) = (cb[0].max(f64::from(b.x0)), cb[1].max(f64::from(b.y0)));
        let (x1, y1) = (cb[2].min(f64::from(b.x1)), cb[3].min(f64::from(b.y1)));
        if x1 > x0 && y1 > y0 {
            let n = PREVIEW_CELLS;
            let mut mesh = egui::Mesh::with_texture(pv.inside.id());
            for j in 0..=n {
                for i in 0..=n {
                    let (x, y) = (x0 + (x1 - x0) * i as f64 / n as f64, y0 + (y1 - y0) * j as f64 / n as f64);
                    let (dx, dy) = map.map(x, y);
                    mesh.vertices.push(egui::epaint::Vertex { pos: scr([dx, dy]), uv: uv(x, y), color: tint });
                }
            }
            for j in 0..n {
                for i in 0..n {
                    let a = (j * (n + 1) + i) as u32;
                    mesh.add_triangle(a, a + 1, a + n as u32 + 2);
                    mesh.add_triangle(a, a + n as u32 + 2, a + n as u32 + 1);
                }
            }
            painter.add(mesh);
        }
    }
    // The cage: the drawn outline (dashed while deforming) and the moving one.
    let line = |pts: &[[f64; 2]], closed: bool, stroke: Stroke| {
        let p: Vec<egui::Pos2> = pts.iter().map(|q| scr(*q)).collect();
        if p.len() >= 2 {
            painter.add(if closed { egui::Shape::closed_line(p, stroke) } else { egui::Shape::line(p, stroke) });
        }
    };
    if s.closed {
        line(&s.cage, true, Stroke::new(1.0, Color32::from_white_alpha(110)));
        line(&s.target, true, Stroke::new(1.5, accent));
    } else {
        line(&s.cage, false, Stroke::new(1.5, accent));
        let hover = painter.ctx().input(|i| i.pointer.hover_pos()).filter(|p| xf.rect.contains(*p)).map(|p| xf.to_doc(p)).or(s.hover);
        if let (Some(last), Some(h)) = (s.cage.last(), hover) {
            painter.line_segment([scr(*last), scr(h)], Stroke::new(1.0, accent.gamma_multiply(0.6)));
        }
    }
    let pts = if s.closed { &s.target } else { &s.cage };
    for (i, p) in pts.iter().enumerate() {
        let c = scr(*p);
        let r = egui::Rect::from_center_size(c, egui::vec2(8.0, 8.0));
        let first_open = !s.closed && i == 0 && s.cage.len() >= 3;
        painter.rect_filled(r, 0.0, if s.selected == Some(i) || first_open { accent } else { Color32::WHITE });
        painter.rect_stroke(r, 0.0, Stroke::new(1.0, Color32::BLACK), egui::StrokeKind::Inside);
    }
}

pub fn options_bar(app: &mut PhotocraftApp, ui: &mut egui::Ui) {
    let ctx = ui.ctx().clone();
    let Some(s) = app.distort.cage.as_mut() else { return };
    let t = crate::theme::Tokens::get(ui.ctx());
    if s.closed && s.previews.is_none() {
        s.previews = Some(s.build_previews(&ctx));
    }
    if s.closed {
        let before = s.coords;
        crate::widgets::dropdown(
            ui,
            "cage-coords",
            &mut s.coords,
            &[(CageCoords::Green, tl!("Green Coordinates")), (CageCoords::MeanValue, tl!("Mean Value Coordinates"))],
            190.0,
        );
        if s.coords != before {
            s.rebuild_map();
        }
        if ui.button(tl!("Reset Cage")).clicked() {
            s.target = s.cage.clone();
            s.rebuild_map();
        }
        crate::widgets::vline(ui, 22.0);
        ui.label(egui::RichText::new(tl!("Drag the cage's points to deform")).color(t.text_dim).size(12.0));
    } else {
        ui.label(egui::RichText::new(tl!("Click to draw a cage around the content; click the first point or press Enter to close it")).color(t.text_dim).size(12.0));
        if s.cage.len() >= 3 && ui.button(tl!("Close Cage")).clicked() {
            if let Err(e) = s.close(Some(&ctx)) {
                app.ui.status = e;
                app.ui.status_error = true;
            }
            return;
        }
    }
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        if crate::widgets::primary_button(ui, "✓", 32.0)
            .on_hover_text(crate::i18n::fmt(tl!("Commit Cage ({key})"), &[("key", &crate::shortcuts::pretty("Enter"))]))
            .clicked()
        {
            commit(app);
        } else if crate::widgets::secondary_button(ui, "⊘", 32.0).on_hover_text(tl!("Cancel (Esc)")).clicked() {
            app.distort.cage = None;
        }
    });
}

/// Is `p` inside the cage being drawn or deformed (for the cursor)?
pub fn inside(s: &CageSession, p: [f64; 2]) -> bool {
    s.cage.len() >= 3 && point_in_polygon(if s.closed { &s.target } else { &s.cage }, p)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app() -> PhotocraftApp {
        let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), Default::default());
        app.session.execute("file.new", json!({"width": 100, "height": 80, "depth": 8})).unwrap();
        app.session.execute("layer.new.layer", json!({})).unwrap();
        app.session
            .edit("paint", |doc, active| {
                doc.layer_mut(active.unwrap()).unwrap().surface_mut().unwrap().fill_rect(Rect::new(40, 30, 50, 40), &[1.0, 0.0, 0.0, 1.0]);
                Ok(())
            })
            .unwrap();
        app.sync_views();
        app
    }

    fn alpha(app: &PhotocraftApp, x: i32, y: i32) -> f32 {
        let st = app.session.active().unwrap();
        st.doc.layer(st.active_layer.unwrap()).unwrap().surface().unwrap().pixel(x, y)[3]
    }

    #[test]
    fn draw_close_drag_and_commit() {
        let ctx = egui::Context::default();
        let mut app = app();
        // Edit › Transform › Cage starts the interactive mode.
        assert!(crate::menus::is_enabled(&app, "edit.transform.cage"));
        crate::menus::invoke(&mut app, &ctx, "edit.transform.cage", json!({})).unwrap();
        assert!(app.distort.cage.is_some());
        let ev = |app: &mut PhotocraftApp, e| crate::distort_ui::pointer(app, e, egui::Modifiers::NONE);
        for p in [[30.0, 20.0], [60.0, 20.0], [60.0, 50.0], [30.0, 50.0]] {
            ev(&mut app, ToolEvent::Down { x: p[0], y: p[1], pressure: 1.0 });
            ev(&mut app, ToolEvent::Up { x: p[0], y: p[1] });
        }
        assert!(!app.distort.cage.as_ref().unwrap().closed);
        // Clicking the first point closes the cage.
        ev(&mut app, ToolEvent::Down { x: 30.5, y: 20.5, pressure: 1.0 });
        ev(&mut app, ToolEvent::Up { x: 30.5, y: 20.5 });
        let s = app.distort.cage.as_ref().unwrap();
        assert!(s.closed && s.cage.len() == 4);
        // Drag every point 20 px right: the content moves with the cage.
        for i in 0..4 {
            let p = app.distort.cage.as_ref().unwrap().target[i];
            ev(&mut app, ToolEvent::Down { x: p[0], y: p[1], pressure: 1.0 });
            ev(&mut app, ToolEvent::Move { x: p[0] + 10.0, y: p[1], pressure: 1.0 });
            ev(&mut app, ToolEvent::Up { x: p[0] + 20.0, y: p[1] });
        }
        assert_eq!(app.distort.cage.as_ref().unwrap().target[2], [80.0, 50.0]);
        let before = app.session.active().unwrap().history.past_len();
        crate::distort_ui::menu(&mut app, &ctx, "edit.transform.cage", &json!({"ui": {"commit": true}})).unwrap().unwrap();
        assert!(app.distort.cage.is_none());
        assert_eq!(app.session.active().unwrap().history.past_len(), before + 1);
        assert!(alpha(&app, 65, 35) > 0.9 && alpha(&app, 45, 35) < 0.1);
    }

    #[test]
    fn control_channel_cancel_and_bad_cages() {
        let ctx = egui::Context::default();
        let mut app = app();
        crate::distort_ui::menu(&mut app, &ctx, "edit.transform.cage", &json!({})).unwrap().unwrap();
        let ui = |app: &mut PhotocraftApp, v: Value| crate::distort_ui::menu(app, &ctx, "edit.transform.cage", &json!({ "ui": v })).unwrap();
        assert!(ui(&mut app, json!({"close": true})).is_err(), "too few points");
        for p in [[30, 20], [60, 20], [60, 50]] {
            ui(&mut app, json!({"point": p})).unwrap();
        }
        // A point whose edge would cross the cage is refused.
        assert!(ui(&mut app, json!({"point": [45, 10]})).is_err());
        assert_eq!(app.distort.cage.as_ref().unwrap().cage.len(), 3);
        let r = ui(&mut app, json!({"close": true, "coordinates": "meanValue", "move": {"index": 0, "to": [25, 15]}})).unwrap();
        assert_eq!(r["closed"], json!(true));
        assert_eq!(r["coordinates"], json!("meanValue"));
        ui(&mut app, json!({"reset": true})).unwrap();
        let s = app.distort.cage.as_ref().unwrap();
        assert_eq!(s.cage, s.target);
        let before = app.session.active().unwrap().history.past_len();
        ui(&mut app, json!({"cancel": true})).unwrap();
        assert!(app.distort.cage.is_none());
        assert_eq!(app.session.active().unwrap().history.past_len(), before, "cancel changes nothing");
    }
}
