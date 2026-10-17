//! Document canvas: display texture cache, view transform, tool input, extra document windows.
//!
//! Rendering is CPU (`photocraft-compose`) for now, uploaded into an egui texture. Brush strokes
//! update only their damage rectangle (`set_partial`). The wgpu compositor (M5) will replace this
//! with direct GPU rendering behind the same `CanvasCache` interface.

use egui::{Color32, PointerButton, Pos2, Rect, Sense, Stroke, TextureOptions, Vec2, pos2, vec2};
use photocraft_doc::{Document, LayerContent};
use photocraft_geom::Rect as DRect;
use serde_json::json;

use crate::PhotocraftApp;
use crate::state::{Tool, View};

#[path = "rotate_view.rs"]
pub mod rotate_view;

mod display;
mod overlays;
mod stroke;
mod tabs;
mod tools;

pub use self::display::*;
pub(crate) use self::overlays::*;
pub(crate) use self::stroke::*;
pub use self::tabs::*;
pub use self::tools::*;

/// Largest texture side we upload; bigger documents display downsampled until the GPU path lands.
pub const MAX_TEXTURE: u32 = 4096;

pub struct CanvasCache {
    pub revision: u64,
    pub texture: Option<egui::TextureHandle>,
    /// Display pixels per document pixel of the cached image (≤ 1).
    pub scale: f32,
    /// Hash of the live-adjust preview used for this render (0 = none).
    pub preview_key: u64,
    /// The current image lives in the GPU canvas (`gpu_canvas`) rather than `texture`.
    pub on_gpu: bool,
    /// Revision and preview key `texture` was rendered at. Tracked apart from the GPU canvas's
    /// `revision`, so the Navigator never shows a stale CPU image after GPU-path edits.
    pub tex_revision: u64,
    pub tex_preview_key: u64,
}

/// An in-progress pointer gesture on the canvas.
#[derive(Clone, Debug)]
pub struct Drag {
    pub tool: Tool,
    pub start: [f64; 2],
    pub points: Vec<[f64; 3]>,
    pub modifiers: egui::Modifiers,
    /// A Brush/Eraser stroke that erases: the Eraser, or a right-button Brush drag with
    /// Preferences › Tools › Right-click with painting tools set to Erase.
    pub erase: bool,
    /// ⇧ constraint of a painting stroke (stroke_constraint.rs).
    pub constrain: Option<crate::stroke_constraint::Axis>,
    /// Modifiers held now (updated on every pointer event of the gesture).
    pub live: egui::Modifiers,
    /// Which of the modifiers held at the press have been released since.
    pub released: egui::Modifiers,
    /// The reposition key (Space) is held: pointer moves move the whole gesture instead of
    /// sizing it (marquees, lasso, shapes; `hold_keys`).
    pub reposition: bool,
    /// A marquee or lasso drag that started inside the selection moves it instead of drawing:
    /// `Some(false)` moves the outline, `Some(true)` moves the floating piece (`select.float`).
    pub sel_move: Option<bool>,
    pub lasso: Option<crate::lasso_ui::Lasso>,
    /// When each point arrived (ms since the press): the airbrush deposits by time.
    pub times: Vec<f64>,
    pub born: std::time::Instant,
}

impl Drag {
    pub fn new(tool: Tool, start: [f64; 2], points: Vec<[f64; 3]>, modifiers: egui::Modifiers, erase: bool) -> Self {
        // The press is time zero. Later `stamp` calls must not move it to the first idle tick.
        let times = vec![0.0; points.len()];
        Self {
            tool,
            start,
            points,
            modifiers,
            erase,
            constrain: None,
            live: modifiers,
            released: egui::Modifiers::NONE,
            reposition: false,
            sel_move: None,
            lasso: None,
            times,
            born: std::time::Instant::now(),
        }
    }

    /// Time-stamps points added since the last call.
    pub(crate) fn stamp(&mut self) {
        let now = self.born.elapsed().as_secs_f64() * 1000.0;
        self.times.truncate(self.points.len());
        while self.times.len() < self.points.len() {
            self.times.push(now);
        }
    }

    /// Reposition: move the start and every point so the last one lands on `to` (same size).
    pub fn shift_to(&mut self, to: [f64; 2]) {
        let last = self.points.last().map_or(self.start, |p| [p[0], p[1]]);
        let (dx, dy) = (to[0] - last[0], to[1] - last[1]);
        if !(dx.is_finite() && dy.is_finite()) {
            return;
        }
        self.start = [self.start[0] + dx, self.start[1] + dy];
        for p in &mut self.points {
            p[0] += dx;
            p[1] += dy;
        }
        if self.points.is_empty() {
            self.points.push([to[0], to[1], 1.0]);
        }
    }

    /// Record the modifiers of a pointer event.
    pub(crate) fn track(&mut self, mods: egui::Modifiers) {
        self.live = mods;
        self.released.shift |= !mods.shift;
        self.released.alt |= !mods.alt;
    }

    /// ⇧ (square / circle) and ⌥ (from the centre) for a marquee drag. A modifier held at the press
    /// picks the selection mode instead (add / subtract, #188) until it is released and pressed
    /// again, as in Photoshop.
    pub fn marquee_mods(&self) -> (bool, bool) {
        let fresh = |now: bool, at_press: bool, released: bool| now && (!at_press || released);
        (fresh(self.live.shift, self.modifiers.shift, self.released.shift), fresh(self.live.alt, self.modifiers.alt, self.released.alt))
    }
}

/// The marquee dragged from `d.start` to `end` as two document-space corners: the options-bar
/// style (fixed ratio / fixed size), ⇧ for a square or circle, ⌥ to draw from the centre.
pub fn marquee_corners(o: &crate::state::ToolOptions, d: &Drag, end: [f64; 2]) -> ([f64; 2], [f64; 2]) {
    let (shift, alt) = d.marquee_mods();
    let e = crate::chrome_ui::marquee_end(&o.marquee_style, o.marquee_width as f64, o.marquee_height as f64, shift, d.start, end);
    if !alt {
        return (d.start, e);
    }
    let (dx, dy) = (e[0] - d.start[0], e[1] - d.start[1]);
    // A fixed size is centred on the press point; otherwise the drag is the half extent.
    let (hx, hy) = if o.marquee_style == "fixedSize" { (dx / 2.0, dy / 2.0) } else { (dx, dy) };
    ([d.start[0] - hx, d.start[1] - hy], [d.start[0] + hx, d.start[1] + hy])
}

/// The pixel rectangle `[x0, y0, x1, y1]` a marquee between two corners selects.
pub fn marquee_px(a: [f64; 2], b: [f64; 2]) -> [f64; 4] {
    [a[0].min(b[0]).floor(), a[1].min(b[1]).floor(), a[0].max(b[0]).ceil(), a[1].max(b[1]).ceil()]
}

/// The size readout shown beside the cursor while dragging a marquee: the width and height values.
pub fn marquee_readout(r: [f64; 4]) -> [String; 2] {
    [format!("{} px", r[2] - r[0]), format!("{} px", r[3] - r[1])]
}

/// Draw the marquee size readout below-right of the cursor (kept on screen), like Photoshop's:
/// two rows, `W:` / `H:` labels on the left and the values right-aligned.
fn draw_marquee_readout(ctx: &egui::Context, cursor: Pos2, values: [String; 2]) {
    draw_readout(ctx, "marquee-readout", cursor, ["W:", "H:"], values);
}

/// A two-row readout beside the pointer (labels left, values right-aligned), above everything.
pub(crate) fn draw_readout(ctx: &egui::Context, id: &str, cursor: Pos2, labels: [&str; 2], values: [String; 2]) {
    let t = crate::theme::Tokens::get(ctx);
    let font = egui::FontId::proportional(11.5);
    let width = |text: &str| ctx.fonts_mut(|f| f.layout_no_wrap(text.to_owned(), font.clone(), t.text).size().x);
    let labels = labels.map(|label| tl!(label));
    let (lw, vw) = (labels.map(width), [width(&values[0]), width(&values[1])]);
    let (label_col, value_col) = (lw[0].max(lw[1]), vw[0].max(vw[1]));
    egui::Area::new(egui::Id::new(id)).order(egui::Order::Tooltip).fixed_pos(cursor + vec2(16.0, 18.0)).interactable(false).constrain(true).show(ctx, |ui| {
        egui::Frame::new().fill(t.card).stroke(Stroke::new(1.0, t.card_border)).corner_radius(t.radius_sm).inner_margin(egui::Margin::symmetric(7, 4)).show(
            ui,
            |ui| {
                ui.spacing_mut().item_spacing = vec2(0.0, 1.0);
                for (i, value) in values.into_iter().enumerate() {
                    // Labels left, values right-aligned on one edge, sharing a baseline.
                    ui.horizontal(|ui| {
                        ui.add(egui::Label::new(egui::RichText::new(labels[i]).font(font.clone()).color(t.text_dim)).extend());
                        ui.add_space(label_col - lw[i] + 12.0 + value_col - vw[i]);
                        ui.add(egui::Label::new(egui::RichText::new(value).font(font.clone()).color(t.text)).extend());
                    });
                }
            },
        );
    });
}

/// The pointer moves this frame delivered while `button` was held, in order. `down_at_start` is
/// whether the button was already down when the frame began. A move before a press or after a
/// release in the same frame is dropped, so a gesture never picks up input from outside its own
/// press..release interval.
fn pointer_moves(events: &[egui::Event], button: egui::PointerButton, down_at_start: bool) -> Vec<Pos2> {
    let mut down = down_at_start;
    let mut out = Vec::new();
    for e in events {
        match e {
            egui::Event::PointerButton { button: b, pressed, .. } if *b == button => down = *pressed,
            egui::Event::PointerMoved(p) if down => out.push(*p),
            _ => {}
        }
    }
    out
}

/// Modifiers attached to the pointer event itself. Some input sources provide these without a
/// separate `ModifiersChanged` event, so use them for click gestures before falling back to the
/// frame-wide modifier state.
fn pointer_button_modifiers(events: &[egui::Event], button: egui::PointerButton) -> Option<egui::Modifiers> {
    events.iter().rev().find_map(|e| match e {
        egui::Event::PointerButton { button: b, modifiers, .. } if *b == button => Some(*modifiers),
        _ => None,
    })
}

/// Abstract tool event, produced by the mouse or by automation (`ui.pointer`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ToolEvent {
    Down { x: f64, y: f64, pressure: f32 },
    Move { x: f64, y: f64, pressure: f32 },
    Up { x: f64, y: f64 },
}

/// Document ↔ screen mapping for a canvas rect and a view.
#[derive(Clone, Copy, Debug)]
pub struct ViewXform {
    pub rect: Rect,
    pub zoom: f32,
    pub center: [f32; 2],
    /// View › Flip Horizontal: the view is mirrored left-right (the document is not).
    pub flip: bool,
    pub rotation: f32,
}

impl ViewXform {
    /// The main canvas's mapping for the active document as last laid out (inside the rulers).
    pub fn active(app: &PhotocraftApp) -> Option<Self> {
        let v = app.ui.views.get(app.session.active_index()?)?;
        Some(Self {
            rect: crate::rulers::content_rect(app, app.last_canvas_rect),
            zoom: v.zoom,
            center: v.center,
            flip: app.ui.view.flip_horizontal,
            rotation: v.rotation,
        })
    }
    /// The shared camera maps both rendered geometry and pointer input.
    pub fn camera(&self) -> crate::view_camera::Camera {
        crate::view_camera::Camera::new(self.rect, self.zoom, self.center, self.flip, self.rotation)
    }
    pub fn to_screen(&self, x: f32, y: f32) -> Pos2 {
        self.camera().to_screen(pos2(x, y))
    }
    pub fn to_doc(&self, p: Pos2) -> [f64; 2] {
        let d = self.camera().to_document(p);
        [f64::from(d.x), f64::from(d.y)]
    }
    pub fn doc_rect(&self, r: DRect) -> Rect {
        self.camera().screen_bounds(Rect::from_min_max(pos2(r.x0 as f32, r.y0 as f32), pos2(r.x1 as f32, r.y1 as f32)))
    }
}

pub fn fit_view(view: &mut View, doc: &Document, area: Vec2) {
    let (w, h) = (doc.size.width as f32, doc.size.height as f32);
    let zoom = rotate_view::fit_zoom(vec2(w, h), area - Vec2::splat(40.0), view.rotation).clamp(0.01, 1.0);
    view.zoom = zoom;
    view.center = [w / 2.0, h / 2.0];
    view.fit_pending = false;
}

/// Zoom steps like Photoshop's (⌘+ / ⌘−).
pub fn zoom_step(z: f32, dir: i32) -> f32 {
    const STEPS: [f32; 22] =
        [0.01, 0.02, 0.03, 0.05, 0.0667, 0.1, 0.125, 0.1667, 0.25, 0.333, 0.5, 0.6667, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 8.0, 12.0, 16.0, 32.0];
    if dir > 0 { STEPS.iter().copied().find(|s| *s > z * 1.001).unwrap_or(32.0) } else { STEPS.iter().rev().copied().find(|s| *s < z * 0.999).unwrap_or(0.01) }
}

/// Tabs + canvas for the active document, or the start screen; then drops layers dragged onto
/// another document (`layer_transfer`).
pub fn document_area(app: &mut PhotocraftApp, ui: &mut egui::Ui) {
    poll_live_stroke(app, ui.ctx());
    let blocked = crate::restoration_ui::is_open(app);
    if blocked {
        ui.disable();
    } else {
        airbrush_tick(app, ui.ctx());
    }
    documents(app, ui);
    // Input is dispatched by `documents`; start its batch before an otherwise idle tap frame ends.
    poll_live_stroke(app, ui.ctx());
    if !blocked {
        crate::layer_transfer::finish(app, ui.ctx());
    }
}

/// Window › Arrange tiles: each document shown with its tile in `rect`, or `None` when the
/// active document fills the area.
pub(crate) fn arranged_cells(app: &PhotocraftApp, rect: Rect) -> Option<Vec<(usize, Rect)>> {
    let idx = app.session.active_index()?;
    let n = app.session.documents().len();
    let cells = crate::view_cmds::cells(&app.ui.view.arrange, rect, n)?;
    let mut shown: Vec<(usize, Rect)> = (0..n).map(|k| (idx + k) % n).zip(cells).collect();
    shown.sort_by_key(|(d, _)| *d);
    Some(shown)
}

/// Draw one canvas view and handle its input. `primary` = main window (tools active).
pub fn canvas_view(app: &mut PhotocraftApp, ui: &mut egui::Ui, idx: usize, rect: Rect, mut view: View, primary: bool) -> View {
    let ctx = ui.ctx().clone();
    // The display this window is on: its monitor profile (#569). A document window on a display
    // the last reading didn't know asks for a new one.
    let output = crate::monitor_status::view_display(app, &ctx);
    if !primary {
        crate::monitor_status::check_window(app, &ctx);
    }
    view.rotation = crate::view_camera::normalized_angle(view.rotation);
    let full = rect;
    let rect = if primary { crate::rulers::content_rect(app, rect) } else { rect };
    let doc = app.session.documents()[idx].doc.clone();
    let size = [doc.size.width, doc.size.height];
    if view.doc_size != size {
        // Like Photoshop: keep the zoom level, re-centre the resized document.
        if view.doc_size != [0, 0] {
            view.center = [size[0] as f32 / 2.0, size[1] as f32 / 2.0];
        }
        view.doc_size = size;
    }
    if view.fit_pending && rect.width() > 50.0 {
        fit_view(&mut view, &doc, rect.size());
    }
    // Preferences › Tools › Overscroll off: clamp before anything is drawn (scrollbars.rs).
    if !app.session.prefs().tools.overscroll && crate::scrollbars::clamp_view_flipped(&mut view, rect.size(), app.ui.view.flip_horizontal) {
        ctx.request_repaint();
    }
    let flip = app.ui.view.flip_horizontal;
    let pixel_grid = app.ui.view.shows(app.ui.view.show.pixel_grid);
    let response = ui.allocate_rect(rect, Sense::click_and_drag());
    let painter = ui.painter_at(rect);
    let input_enabled = ui.is_enabled() && !crate::restoration_ui::is_open(app);
    let rotate_owned = rotate_view::input(&ctx, &response, &mut view, doc.id, app.ui.tool, input_enabled && app.ui.dialogs.is_empty());
    let xf = ViewXform { rect, zoom: view.zoom, center: view.center, flip, rotation: view.rotation };
    // Captured screen-axis gestures must see the camera used by this frame's pointer inverse,
    // including a just-applied Fit/resize/clamp, rather than last frame's cached View.
    if primary {
        app.ui.views[idx] = view.clone();
    }
    let vector_input = !rotate_owned && input_enabled && primary && crate::vector_integration::input(app, ui, idx, &response, &xf, rect);
    let symbol_input = vector_input || (input_enabled && primary && crate::symbols::canvas_input(app, &response, &xf));
    let move_input = rotate_owned || symbol_input || (input_enabled && primary && crate::move_input::canvas_input(app, &response, &xf));
    let doc = if move_input { app.session.documents()[idx].doc.clone() } else { doc };

    match crate::prefs_ui::pasteboard_color(app) {
        Some(c) => {
            ui.painter_at(rect).rect_filled(rect, 0.0, c);
        }
        None => paint_dots(ui, rect),
    }
    let border = app.session.prefs().interface.canvas_border;
    let drop_shadow = border == photocraft_engine::prefs::CanvasBorder::DropShadow;
    // Drop shadow, checkerboard, document image.
    let img_rect = xf.doc_rect(doc.bounds());
    let image_bounds = Rect::from_min_size(Pos2::ZERO, vec2(doc.size.width as f32, doc.size.height as f32));
    // Live adjustment previews on big documents use a downsampled proxy (see proxy.rs).
    let mut on_gpu = false;
    let move_planes = crate::move_fast::planes(app, idx);
    // A flipped view mirrors in the GPU canvas shader (`ViewParams::flip`).
    if let Some(planes) = move_planes {
        on_gpu = true;
        app.perf.gpu = true;
        app.perf.record("gpu-move-offset", (rect.width() * rect.height() * ctx.pixels_per_point().powi(2)) as u64, 0.0, 0.0);
        app.perf.gpu_uploads = 0;
        app.perf.gpu_fallback = None;
        let params = crate::gpu_canvas::ViewParams {
            doc: doc.id.0,
            doc_size: size,
            zoom: view.zoom,
            center: view.center,
            shadow: !crate::theme::Tokens::get(&ctx).bevel && !crate::theme::Tokens::get(&ctx).pro && drop_shadow,
            pixel_grid,
            view_key: egui::Id::new(("pc-canvas-move", ctx.viewport_id(), idx)).value(),
            display: sync_display_lut(app, &doc, doc.id.0, output),
            output: output.unwrap_or(0),
            hdr: hdr_preview(app, &doc),
            flip,
            rotation: view.rotation,
        };
        crate::gpu_canvas::GpuCanvas::paint_move(&painter, rect, params, planes);
    } else if app.gpu.is_some()
        && let Some((k, key)) = ensure_adjust_proxy(app, idx, view.zoom * ctx.pixels_per_point())
            .or_else(|| ensure_filter_preview(app, idx))
            .or_else(|| ensure_proxy_preview(app, idx))
    {
        on_gpu = true;
        let params = crate::gpu_canvas::ViewParams {
            doc: key,
            doc_size: [doc.size.width.div_ceil(k), doc.size.height.div_ceil(k)],
            zoom: view.zoom * k as f32,
            center: [view.center[0] / k as f32, view.center[1] / k as f32],
            shadow: {
                let t = crate::theme::Tokens::get(&ctx);
                !t.bevel && !t.pro && drop_shadow
            },
            pixel_grid: false,
            view_key: egui::Id::new(("pc-canvas-proxy", ctx.viewport_id(), idx)).value(),
            display: sync_display_lut(app, &doc, key, output),
            output: output.unwrap_or(0),
            hdr: hdr_preview(app, &doc),
            flip,
            rotation: view.rotation,
        };
        crate::gpu_canvas::GpuCanvas::paint(&painter, rect, params);
    } else if ensure_gpu(app, &ctx, idx, visible_doc_rect(&xf)) {
        on_gpu = true;
        app.perf.gpu = true;
        // Shadow, checkerboard, document and pixel grid in one custom shader (gpu_canvas.rs).
        let params = crate::gpu_canvas::ViewParams {
            doc: doc.id.0,
            doc_size: [doc.size.width, doc.size.height],
            zoom: view.zoom,
            center: view.center,
            shadow: {
                let t = crate::theme::Tokens::get(&ctx);
                !t.bevel && !t.pro && drop_shadow
            },
            pixel_grid,
            view_key: egui::Id::new(("pc-canvas", ctx.viewport_id(), idx)).value(),
            display: sync_display_lut(app, &doc, doc.id.0, output),
            output: output.unwrap_or(0),
            hdr: hdr_preview(app, &doc),
            flip,
            rotation: view.rotation,
        };
        crate::gpu_canvas::GpuCanvas::paint(&painter, rect, params);
    } else {
        if !crate::theme::Tokens::get(&ctx).bevel && drop_shadow {
            if view.rotation == 0.0 {
                painter.add(egui::Shadow { offset: [0, 8], blur: 28, spread: 0, color: Color32::from_black_alpha(150) }.as_shape(img_rect, 0));
            } else {
                rotate_view::cpu_shadow(&painter, &xf, image_bounds);
            }
        }
        match app.session.prefs().transparency_and_gamut.square() {
            Some(square) => {
                let checker_id = checker(app, &ctx);
                let (anchor, square) = if view.rotation == 0.0 {
                    (img_rect.min, square)
                } else {
                    let ppp = ctx.pixels_per_point();
                    (pos2((rect.left() * ppp).round() / ppp, (rect.top() * ppp).round() / ppp), (square * ppp).round().max(1.0) / ppp)
                };
                painter.add(xf.camera().screen_textured_quad(checker_id, image_bounds, anchor, Vec2::splat(2.0 * square), Color32::WHITE));
            }
            None => {
                if view.rotation == 0.0 {
                    painter.rect_filled(img_rect, 0.0, Color32::WHITE);
                } else {
                    painter.add(egui::Shape::convex_polygon(xf.camera().quad(image_bounds).to_vec(), Color32::WHITE, Stroke::NONE));
                }
            }
        }
        if let Some((tex, _scale)) = ensure_texture(app, &ctx, idx, output) {
            painter.add(xf.camera().textured_quad(tex, image_bounds, Rect::from_min_max(Pos2::ZERO, pos2(1.0, 1.0)), Color32::WHITE));
        }
    }
    if let Err(e) = crate::vector_integration::views::overlay(app, &ctx, idx, &xf, &painter) {
        app.ui.status = e.to_string();
        app.ui.status_error = true;
    }
    // Channels panel: alpha / Quick Mask overlays and single-channel views (channel_view.rs).
    if let Some(tex) = crate::channel_view::ensure(app, &ctx, idx) {
        painter.add(xf.camera().textured_quad(tex, image_bounds, Rect::from_min_max(Pos2::ZERO, pos2(1.0, 1.0)), Color32::WHITE));
    }
    // Artboards: pasteboard between the boards, outlines and names (artboard_ui.rs).
    if doc.has_artboards() {
        let t = crate::theme::Tokens::get(&ctx);
        let pasteboard = crate::prefs_ui::pasteboard_color(app);
        let dot_tex = dots(&ctx, &t);
        crate::artboard_ui::paint_pasteboard(&painter, &xf, &doc, pasteboard.unwrap_or(t.canvas), if pasteboard.is_none() { dot_tex } else { None });
        if primary {
            crate::artboard_ui::draw_frames(app, &painter, &xf);
        }
    }

    // Pixel grid at high zoom (the GPU path draws its own).
    if !on_gpu && pixel_grid && view.zoom >= 12.0 {
        let vis = visible_doc_rect(&xf).intersect(&doc.bounds());
        let grid = Stroke::new(1.0, Color32::from_white_alpha(40));
        for x in vis.x0..=vis.x1 {
            painter.line_segment([xf.to_screen(x as f32, vis.y0 as f32), xf.to_screen(x as f32, vis.y1 as f32)], grid);
        }
        for y in vis.y0..=vis.y1 {
            painter.line_segment([xf.to_screen(vis.x0 as f32, y as f32), xf.to_screen(vis.x1 as f32, y as f32)], grid);
        }
    }

    // View › Show › Layer Edges: the active layer's content bounds.
    if app.ui.view.shows(app.ui.view.show.layer_edges)
        && let Some(st) = app.session.documents().get(idx)
        && let Some(b) = st.active_layer.and_then(|id| st.doc.layer(id)).and_then(|l| l.surface()).map(photocraft_compose::bounds::content_bounds)
        && !b.is_empty()
    {
        let bounds = Rect::from_min_max(pos2(b.x0 as f32, b.y0 as f32), pos2(b.x1 as f32, b.y1 as f32));
        painter.add(egui::Shape::closed_line(xf.camera().quad(bounds).to_vec(), Stroke::new(1.0, Color32::from_rgb(0x2d, 0x8c, 0xeb))));
    }

    // Selection outline: true boundary, animated marching ants (cached per revision).
    if let Some(sel) = doc.selection.as_ref().filter(|_| app.ui.view.shows(app.ui.view.show.selection_edges) && !polygon_replaces_selection(app)) {
        // Trace at display resolution over the visible part only; key by the mask's tile identity
        // (not the document revision) so unrelated edits don't re-trace it.
        let step = (1.0 / view.zoom.max(1e-3)).log2().floor().exp2().clamp(1.0, 64.0) as u32;
        let visible = xf.camera().document_bounds(rect);
        let tl = [visible.left(), visible.top()];
        let br = [visible.right(), visible.bottom()];
        let q = 256 * step as i32; // quantise the region so small pans reuse the cache
        let vis = photocraft_geom::Rect::new(
            (tl[0].floor() as i32).div_euclid(q) * q - q,
            (tl[1].floor() as i32).div_euclid(q) * q - q,
            ((br[0].ceil() as i32).div_euclid(q) + 2) * q,
            ((br[1].ceil() as i32).div_euclid(q) + 2) * q,
        );
        let key = crate::surface_fingerprint(sel)
            ^ (step as u64) << 56
            ^ doc.id.0.rotate_left(17)
            ^ (vis.x0 as u64) << 8
            ^ (vis.y0 as u64) << 24
            ^ (vis.x1 as u64) << 36
            ^ (vis.y1 as u64) << 48;
        let fresh = matches!(&app.outline_cache, Some((d, k, _)) if *d == doc.id && *k == key);
        if !fresh {
            let t0 = crate::gpu_canvas::now_ms();
            let b = app.cached_bounds(u64::MAX - doc.id.0, sel).intersect(&vis);
            let segs = crate::outline::outline_scaled(sel, b, step);
            app.perf.span("outline", crate::gpu_canvas::now_ms() - t0);
            app.outline_cache = Some((doc.id, key, std::sync::Arc::new(segs)));
        }
        if let Some((_, _, segs)) = &app.outline_cache {
            let time = ui.input(|i| i.time);
            // A floating piece (and a selection being dragged) draws its ants where it is shown.
            let mut shown = xf;
            if let Some((dx, dy)) = selection_shown_offset(app) {
                shown.center = [xf.center[0] - dx as f32, xf.center[1] - dy as f32];
            }
            marching_ants_segments(&painter, &shown, segs, time);
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
    }

    // Restoration owns its own preview and all input, including raw-pointer
    // routes that bypass egui responses. The underlying camera stays unchanged.
    if !input_enabled {
        if primary {
            app.ui.views[idx] = view.clone();
        }
        return view;
    }
    // Under an open dialog the canvas widget is inert, but the image still pans and zooms.
    let under_dialog = !app.ui.dialogs.is_empty();
    let free_hover = under_dialog && crate::dialogs::free_pointer_over(&ctx, rect).is_some();
    // Navigation (wheel_nav.rs): scroll pans; pinch, ⌘-scroll and ⌥-scroll zoom around the pointer.
    let wheel = crate::wheel_nav::read(&ctx, app.session.prefs().general.zoom_with_scroll_wheel);
    // The wheel also scrolls over the scrollbars drawn on top of the canvas (last frame's hover).
    let bars_id = egui::Id::new(("pc-canvas-bars-hover", idx));
    let over_bars = ctx.data(|d| d.get_temp::<bool>(bars_id)).unwrap_or(false);
    if response.hovered() || free_hover || over_bars {
        let pointer = ui.input(|i| i.pointer.hover_pos());
        match (wheel, pointer) {
            (Some(crate::wheel_nav::Wheel::Zoom(f)), Some(p)) => {
                let nz = (view.zoom * f).clamp(0.01, 64.0);
                zoom_about(&mut view, &xf, p, nz);
            }
            (Some(crate::wheel_nav::Wheel::Pan(scroll)), _) => {
                rotate_view::pan(&mut view, &xf, scroll);
            }
            _ => {}
        }
    }

    // Pen pressure/tilt for this frame's tool events (mouse = 1.0), unless Preferences › Tools ›
    // Use Tablet Pressure is off; the pen's eraser end selects the Eraser.
    app.stylus.use_pressure = app.session.prefs().tools.use_tablet_pressure;
    // local-image: Preferences › Tools › Pen pressure curve.
    let curve = app.session.prefs().tools.pen_pressure_curve.clone();
    app.stylus.set_pressure_curve(&curve);
    app.stylus.update_for_frame(ui.ctx().cumulative_frame_nr(), &ui.input(|i| i.events.clone()));
    crate::stylus::Stylus::sync_eraser_tool(app);
    // A Magnetic Lasso border left behind by a tool or document switch is dropped.
    crate::magnetic_lasso_ui::frame(app);
    // Held keys (hold_keys.rs): Space repositions a crop frame, marquee, lasso or shape being
    // drawn; otherwise Space is the Hand and ⌘Space / ⌘⌥Space the Zoom tool while held.
    let reposition = crate::hold_keys::reposition_held(app, &ctx);
    crate::crop_ui::set_space(app, reposition);
    crate::crop_ui::ensure_frame(app);
    let mut drawing = crate::crop_ui::active(app);
    if let Some(d) = app.drag.as_mut().filter(|d| crate::hold_keys::repositions(d.tool)) {
        d.reposition = reposition;
        drawing = true;
    }
    let temporary = crate::hold_keys::for_frame(app, &ctx, drawing);
    let space_pan = temporary == Some(crate::hold_keys::Temporary::Hand);
    let middle = ui.input(|i| i.pointer.middle_down());
    let tool = match temporary.filter(|_| !crate::move_input::captured(&ctx)) {
        Some(t) => t.tool(),
        None if middle => Tool::Hand,
        None => app.ui.tool,
    };
    // Zoom direction: the temporary zoom key decides, else ⌥ (Zoom tool).
    let zoom_out = |alt: bool| match temporary {
        Some(crate::hold_keys::Temporary::ZoomOut) => true,
        Some(crate::hold_keys::Temporary::ZoomIn) => false,
        _ => alt,
    };

    if under_dialog {
        // With the Color Picker on top the image is its eyedropper, whatever the tool; Space and
        // the middle button still pan (`color_picker_ui::sample_at`).
        let picking = primary && crate::color_picker_ui::top(app).is_some();
        let hand = app.ui.tool == Tool::Hand && !picking;
        if let Some(d) = crate::dialogs::pan_delta(&ctx, rect, hand) {
            rotate_view::pan(&mut view, &xf, d);
            ctx.set_cursor_icon(egui::CursorIcon::Grabbing);
        } else if free_hover && (space_pan || hand) {
            ctx.set_cursor_icon(egui::CursorIcon::Grab);
        } else if picking && let Some(p) = crate::dialogs::free_pointer_over(&ctx, rect) {
            if app.session.prefs().cursors.other == photocraft_engine::prefs::OtherCursor::Precise {
                ctx.set_cursor_icon(egui::CursorIcon::Crosshair);
            } else {
                // The tip of the icon's pipette is at (2, 22) of its 24-unit box.
                crate::icons::cursor(&ctx, "pipette", p, vec2(2.0, 22.0) / 24.0, 20.0);
                ctx.set_cursor_icon(egui::CursorIcon::None);
            }
            if let Some(p) = crate::dialogs::free_press(&ctx, rect) {
                let d = xf.to_doc(p);
                crate::color_picker_ui::sample_at(app, d[0], d[1]);
            }
        }
    }
    if tool == Tool::Hand && response.dragged() {
        let d = response.drag_delta();
        rotate_view::pan(&mut view, &xf, d);
        ctx.set_cursor_icon(egui::CursorIcon::Grabbing);
    } else if primary {
        let mods = ui.input(|i| i.modifiers);
        // Tools follow the left button; the right one opens the Brush Preset picker or erases
        // (Preferences › Tools, `paint_mouse`).
        crate::paint_mouse::sync_tool_smoothing(app);
        let mut buttons = crate::paint_mouse::canvas_buttons(app, &response, tool);
        if move_input {
            buttons = crate::paint_mouse::Buttons::default();
        }
        // Right-click while transforming: switch the box's mode (Free Transform, Scale, Rotate,
        // Skew, Distort, Perspective).
        let transforming = app.ui.transform.as_ref().is_some_and(|t| t.warp.is_none());
        if response.secondary_clicked()
            && transforming
            && let Some(p) = response.interact_pointer_pos()
        {
            crate::canvas_tool_menu::open_transform(app, [p.x, p.y]);
        }
        if tool == Tool::Lasso {
            crate::lasso_ui::canvas_input(app, &ctx, &xf, &response);
            (buttons.started, buttons.dragged, buttons.stopped, buttons.clicked) = (false, false, false, false);
        }
        let vector_object_menu = crate::canvas_tool_menu::vector_object_tool(app, tool);
        if response.secondary_clicked()
            && !transforming
            && vector_object_menu
            && let Some(p) = response.interact_pointer_pos()
        {
            crate::canvas_tool_menu::open(app, tool, [p.x, p.y]);
        }
        // Right-click with the Move tool, or ⌘/Ctrl+right-click: the layers under the pointer.
        if response.secondary_clicked()
            && !transforming
            && !vector_object_menu
            && crate::layer_pick_ui::is_gesture(tool, mods)
            && let Some(p) = response.interact_pointer_pos()
        {
            let d = xf.to_doc(p);
            app.ui.canvas_tool_menu = None;
            crate::layer_pick_ui::open(app, [p.x, p.y], d[0], d[1]);
        }
        if response.secondary_clicked()
            && !transforming
            && !vector_object_menu
            && !crate::layer_pick_ui::is_gesture(tool, mods)
            && let Some(p) = response.interact_pointer_pos()
        {
            crate::canvas_tool_menu::open(app, tool, [p.x, p.y]);
        }
        // The (temporary) Hand pans above; its gestures never reach the tool underneath.
        if tool == Tool::Hand {
            (buttons.started, buttons.dragged, buttons.stopped) = (false, false, false);
        }
        // Zoom tool drags: scrubby zoom or a zoom rectangle (zoom_tool.rs); clicks step below.
        if tool == Tool::Zoom && crate::zoom_tool::drag(app, &ctx, &mut view, &xf, &buttons, response.interact_pointer_pos()) {
            (buttons.started, buttons.dragged, buttons.stopped) = (false, false, false);
        }
        // A drag is only recognised once the pointer has moved past egui's click distance: the
        // gesture starts where the button went down, not where it is now (#123).
        let gesture_active_before = app.drag.is_some() || queued_stroke_point(app).is_some();
        let events = ui.input(|i| i.events.clone());
        let painting = tool.is_brushlike();
        let pressed = events.iter().find_map(|e| match e {
            egui::Event::PointerButton { pos, button: PointerButton::Primary, pressed: true, modifiers } if rect.contains(*pos) => Some((*pos, *modifiers)),
            _ => None,
        });
        if painting
            && !move_input
            && (response.is_pointer_button_down_on() || buttons.clicked || buttons.started)
            && !gesture_active_before
            && let Some((p, press_mods)) = pressed
        {
            let d = xf.to_doc(p);
            tool_event(app, ToolEvent::Down { x: d[0], y: d[1], pressure: app.stylus.pressure() }, press_mods);
        }
        if buttons.started
            && !(painting && (app.drag.is_some() || queued_stroke_point(app).is_some()))
            && let Some(p) = ui.input(|i| i.pointer.press_origin()).filter(|p| rect.contains(*p)).or(response.interact_pointer_pos())
        {
            if tool == Tool::Move && app.ui.transform.is_none() {
                begin_transform_controls_at(app, &ctx, &xf, p);
            }
            let d = xf.to_doc(p);
            tool_event(app, ToolEvent::Down { x: d[0], y: d[1], pressure: app.stylus.pressure() }, mods);
        }
        if buttons.dragged || buttons.stopped {
            // Feed every pointer move the OS delivered this frame, not just the latest position, so
            // a fast stroke is sampled densely and renders as a smooth curve instead of a coarse
            // polyline. egui-winit pushes one `PointerMoved` per `CursorMoved`, and they accumulate
            // while a frame is slow, so reading them all recovers the moves a per-frame
            // `interact_pointer_pos()` would drop. Only freehand tools take the whole batch; the
            // rest follow the pointer's latest position. The moves are bounded to the gesture's own
            // press..release interval (`pointer_moves`), so the start and stop frames keep their
            // valid samples without leaking a move from outside the gesture.
            let events = ui.input(|i| i.events.clone());
            let button = if response.dragged_by(PointerButton::Secondary)
                || response.drag_started_by(PointerButton::Secondary)
                || response.drag_stopped_by(PointerButton::Secondary)
            {
                PointerButton::Secondary
            } else {
                PointerButton::Primary
            };
            let press_this_frame = events.iter().any(|e| matches!(e, egui::Event::PointerButton { button: b, pressed: true, .. } if *b == button));
            let down_at_start = !press_this_frame && (gesture_active_before || buttons.started);
            let mut positions = if freehand_tool(tool) {
                pointer_moves(&events, button, down_at_start)
            } else if buttons.dragged {
                response.interact_pointer_pos().into_iter().collect()
            } else {
                Vec::new()
            };
            // A frame with no raw move still tracks a held gesture (a still pointer, a modifier
            // change): fall back to the latest position, as the old code always did.
            if positions.is_empty()
                && buttons.dragged
                && let Some(p) = response.interact_pointer_pos()
            {
                positions.push(p);
            }
            app.defer_live_stroke = true;
            let n = positions.len();
            for (k, p) in positions.into_iter().enumerate() {
                app.stylus.select(k, n);
                let d = xf.to_doc(p);
                tool_event(app, ToolEvent::Move { x: d[0], y: d[1], pressure: app.stylus.pressure() }, mods);
            }
            app.stylus.clear_selection();
            app.defer_live_stroke = false;
            // One live-stroke update for the whole frame, not one per recovered sample.
            feed_live_stroke(app);
            crate::magnetic_lasso_ui::flush(app);
        }
        let painting_lift = painting
            && (app.drag.is_some() || queued_stroke_point(app).is_some())
            && events.iter().any(|e| matches!(e, egui::Event::PointerButton { button: PointerButton::Primary, pressed: false, .. }));
        if buttons.stopped || painting_lift {
            // egui may report both stopped and clicked: commit the stroke only once.
            if painting && (app.drag.is_some() || queued_stroke_point(app).is_some()) {
                buttons.clicked = false;
            }
            let p = response
                .interact_pointer_pos()
                .map(|p| xf.to_doc(p))
                .or_else(|| app.drag.as_ref().and_then(|d| d.points.last().map(|q| [q[0], q[1]])))
                .or_else(|| queued_stroke_point(app).map(|p| [p[0], p[1]]));
            if let Some(d) = p {
                tap_at_peak_pressure(app);
                tool_event(app, ToolEvent::Up { x: d[0], y: d[1] }, mods);
            }
        }
        if buttons.clicked
            && let Some(p) = response.interact_pointer_pos()
        {
            let click_button = if app.secondary_erase { PointerButton::Secondary } else { PointerButton::Primary };
            let click_mods = ui.input(|i| pointer_button_modifiers(&i.events, click_button)).unwrap_or(mods);
            let d = xf.to_doc(p);
            match tool {
                Tool::Zoom => {
                    let nz = zoom_step(view.zoom, if zoom_out(click_mods.alt) { -1 } else { 1 });
                    zoom_about(&mut view, &xf, p, nz);
                }
                // A click with the (temporary) Hand does nothing, never the tool underneath.
                Tool::Hand | Tool::RotateView => {}
                // Double-clicking closes the polygonal lasso: the first click placed the last
                // vertex (or already closed it), so the second never starts a new polygon. egui
                // reports it as a triple click when a vertex went down shortly before.
                Tool::PolygonLasso if response.double_clicked() || response.triple_clicked() => commit_polygon(app),
                // The same for the Magnetic Lasso: along the edges, or straight with ⌥.
                Tool::MagneticLasso if response.double_clicked() || response.triple_clicked() => crate::magnetic_lasso_ui::close(app, mods.alt),
                _ => {
                    if tool == Tool::Move && app.ui.transform.is_none() {
                        begin_transform_controls_at(app, &ctx, &xf, p);
                    }
                    tool_event(app, ToolEvent::Down { x: d[0], y: d[1], pressure: app.stylus.pressure() }, click_mods);
                    tap_at_peak_pressure(app);
                    tool_event(app, ToolEvent::Up { x: d[0], y: d[1] }, click_mods);
                }
            }
        }
        // Magnetic Lasso: between clicks the border follows the pointer with the button up.
        if tool == Tool::MagneticLasso && app.ui.magnetic.active() && !(buttons.started || buttons.dragged || buttons.stopped) && response.hovered() {
            let moves: Vec<Pos2> = ui.input(|i| i.events.iter().filter_map(|e| if let egui::Event::PointerMoved(p) = e { Some(*p) } else { None }).collect());
            app.defer_live_stroke = true;
            for p in moves.into_iter().filter(|p| rect.contains(*p)) {
                let d = xf.to_doc(p);
                tool_event(app, ToolEvent::Move { x: d[0], y: d[1], pressure: app.stylus.pressure() }, mods);
            }
            app.defer_live_stroke = false;
            crate::magnetic_lasso_ui::flush(app);
        }
        if app.ui.transform.is_some() && response.double_clicked() {
            crate::transform_tool::commit(app);
        }
        if tool.is_type() && response.double_clicked() {
            crate::type_tool::select_word(app);
        }
        if app.ui.extras.grid && app.ui.view.extras {
            crate::rulers::draw_grid(app, &painter, &xf, &doc);
        }
        if app.ui.view.shows(app.ui.view.show.canvas_guides) {
            crate::rulers::draw_guides(app, &painter, &xf, &doc);
        }
        draw_drag_preview(app, &painter, &xf);
        crate::zoom_tool::draw(&ctx, &painter);
        let resizing = crate::brush_resize::draw(app, &painter, &xf);
        draw_transform_controls(app, &painter, &xf);
        crate::paint_mouse::show_picker(app, &ctx);
        crate::layer_pick_ui::show(app, &ctx);
        crate::canvas_tool_menu::show(app, &ctx);
        crate::snap_ui::draw(app, &painter, &xf);
        if border == photocraft_engine::prefs::CanvasBorder::Line {
            painter.add(egui::Shape::closed_line(xf.camera().quad(image_bounds).to_vec(), Stroke::new(1.0, Color32::from_gray(20))));
        }
        crate::type_tool::draw_overlay(app, &painter, &xf);
        crate::transform_tool::draw_overlay(app, &painter, &xf);
        crate::distort_ui::draw_overlay(app, &painter, &xf);
        crate::retouch_ui::draw_source_marker(app, &painter, &xf);
        crate::vector_integration::after_paint(app, ui, idx, &xf, rect);
        crate::vector_ui::draw_overlay(app, &painter, &xf, &doc);
        crate::analysis_ui::draw_overlay(app, &painter, &xf);
        crate::gradient_ui::draw_overlay(app, &painter, &xf);
        crate::slice_ui::draw_overlay(app, &painter, &xf);
        // Move/Hand hover survives a canvas widget ID changing during relayout.
        let cursor_pos = response.hover_pos().or_else(|| {
            ctx.pointer_hover_pos().filter(|p| matches!(tool, Tool::Move | Tool::Hand) && rect.contains(*p) && ctx.layer_id_at(*p) == Some(response.layer_id))
        });
        // Tool cursors (Photoshop-style).
        let guide_hover = cursor_pos.filter(|_| tool == Tool::Move).and_then(|p| {
            let d = xf.to_doc(p);
            crate::rulers::guide_at(app, d[0], d[1])
        });
        if let Some((vertical, _)) = guide_hover {
            ui.ctx()
                .set_cursor_icon(rotate_view::resize_cursor(if vertical { egui::CursorIcon::ResizeHorizontal } else { egui::CursorIcon::ResizeVertical }, &xf));
        } else if let Some(c) =
            cursor_pos.filter(|_| tool != Tool::RotateView).and_then(|p| crate::transform_tool::cursor(app, xf.to_doc(p), ui.input(|i| i.modifiers.alt)))
        {
            ui.ctx().set_cursor_icon(rotate_view::resize_cursor(c, &xf));
        } else if let Some(c) = cursor_pos.filter(|_| tool == Tool::Crop).and_then(|p| crate::crop_ui::cursor(app, xf.to_doc(p))) {
            ui.ctx().set_cursor_icon(rotate_view::resize_cursor(c, &xf));
        } else if app.drag.as_ref().is_some_and(|d| d.sel_move.is_some())
            || cursor_pos.is_some_and(|p| app.drag.is_none() && selection_drag_kind(app, tool, xf.to_doc(p), ui.input(|i| i.modifiers)).is_some())
        {
            // Over the ants with a marquee or lasso: a press drags the selection.
            ui.ctx().set_cursor_icon(egui::CursorIcon::Move);
        } else if let Some(p) = cursor_pos {
            let alt = ui.input(|i| i.modifiers.alt);
            let icon = match tool {
                // Resizing the brush: the circle stays where the drag began (`brush_resize`).
                t if resizing && crate::brush_resize::applies(t) => egui::CursorIcon::None,
                // ⌥ turns a painting tool into the Eyedropper (`alt_eyedropper`).
                t if app.alt_sampling || (app.drag.is_none() && alt_samples(t, crate::workspace_ui::sticky_mods(app, ui.input(|i| i.modifiers)))) => {
                    egui::CursorIcon::Crosshair
                }
                t if t.is_brushlike() || t == Tool::QuickSelection => {
                    // Preferences › Cursors: brush tip outline (normal = the 50% contour, or
                    // full size), precise crosshair, or the standard pointer.
                    use photocraft_engine::prefs::PaintingCursor;
                    let cur = app.session.prefs().cursors.clone();
                    let painting = app.drag.is_some();
                    let brush = &app.session.tools.brush;
                    let full = (brush.size / 2.0 * view.zoom).max(1.0);
                    let r = if cur.painting == PaintingCursor::NormalTip { (full * (0.5 + 0.5 * brush.hardness.clamp(0.0, 1.0))).max(1.0) } else { full };
                    let crosshair = |len: f32| {
                        for (w, c) in [(2.5, Color32::from_black_alpha(140)), (1.0, Color32::from_white_alpha(220))] {
                            painter.line_segment([p - vec2(len, 0.0), p + vec2(len, 0.0)], Stroke::new(w, c));
                            painter.line_segment([p - vec2(0.0, len), p + vec2(0.0, len)], Stroke::new(w, c));
                        }
                    };
                    match cur.painting {
                        PaintingCursor::Standard => egui::CursorIcon::Default,
                        PaintingCursor::Precise => {
                            crosshair(6.0);
                            egui::CursorIcon::None
                        }
                        _ if painting && cur.show_only_crosshair_while_painting => {
                            crosshair(5.0);
                            egui::CursorIcon::None
                        }
                        // The Pencil: the square of whole pixels its dab fills, on the pixel grid.
                        _ if tool == Tool::Pencil => {
                            let ppp = painter.ctx().pixels_per_point();
                            let doc_point = xf.to_doc(p);
                            let sq = pencil_cursor_rect(&xf, doc_point, brush.size, ppp);
                            let px = 1.0 / ppp;
                            if xf.rotation == 0.0 {
                                painter.rect_stroke(sq, 0.0, Stroke::new(px, Color32::from_black_alpha(160)), egui::StrokeKind::Outside);
                                painter.rect_stroke(sq, 0.0, Stroke::new(px, Color32::from_white_alpha(230)), egui::StrokeKind::Inside);
                            } else {
                                let [x0, y0, x1, y1] = photocraft_engine::paint::grid_square(doc_point[0], doc_point[1], brush.size);
                                let square = Rect::from_min_max(pos2(x0 as f32, y0 as f32), pos2(x1 as f32, y1 as f32));
                                let points = xf.camera().quad(square).to_vec();
                                painter.add(egui::Shape::closed_line(points.clone(), Stroke::new(3.0 * px, Color32::from_black_alpha(160))));
                                painter.add(egui::Shape::closed_line(points, Stroke::new(px, Color32::from_white_alpha(230))));
                            }
                            // Too small to see where it is: the hotspot as well.
                            if cur.show_crosshair_in_brush_tip || sq.width() < 6.0 {
                                crosshair(4.0);
                            }
                            egui::CursorIcon::None
                        }
                        _ => {
                            painter.circle_stroke(p, r + 0.5, Stroke::new(1.0, Color32::from_black_alpha(140)));
                            painter.circle_stroke(p, r, Stroke::new(1.0, Color32::from_white_alpha(220)));
                            if brush_tip_centre(tool, alt || app.ui.shell.sticky_alt, cur.show_crosshair_in_brush_tip, r) {
                                crosshair(3.0);
                            }
                            egui::CursorIcon::None
                        }
                    }
                }
                // Preferences › Cursors › Other Cursors: Precise shows a crosshair for every tool.
                Tool::Move | Tool::Type | Tool::VerticalType | Tool::Eyedropper
                    if app.session.prefs().cursors.other == photocraft_engine::prefs::OtherCursor::Precise =>
                {
                    egui::CursorIcon::Crosshair
                }
                // Photoshop's Move tool uses the normal arrow; only Hand / Space-pan show a hand
                // (several Linux cursor themes draw `CursorIcon::Move` as a large hand).
                Tool::Move => egui::CursorIcon::Default,
                Tool::Hand => {
                    if response.dragged() {
                        egui::CursorIcon::Grabbing
                    } else {
                        egui::CursorIcon::Grab
                    }
                }
                Tool::Zoom => {
                    if zoom_out(alt) {
                        egui::CursorIcon::ZoomOut
                    } else {
                        egui::CursorIcon::ZoomIn
                    }
                }
                Tool::Type | Tool::VerticalType => egui::CursorIcon::Text,
                Tool::MagneticLasso => crate::magnetic_lasso_ui::cursor(app, &painter, p, view.zoom),
                _ => egui::CursorIcon::Crosshair,
            };
            let icon = visible_crosshair(icon, &painter, p, cfg!(target_os = "windows"));
            ui.ctx().set_cursor_icon(icon);
            // Selection tools: + / − / × badge for the effective mode (#170). A gesture keeps the
            // mode it started with (⇧ then constrains the marquee instead of adding).
            let held = crate::workspace_ui::sticky_mods(app, ui.input(|i| i.modifiers));
            let mods = app.drag.as_ref().map_or(held, |d| d.modifiers);
            if let Some(b) = crate::tool_feedback::badge(app, tool, mods) {
                crate::tool_feedback::draw_badge(&painter, p, b, tool == Tool::QuickSelection);
            }
            // ⇧ after a stroke: the straight line a click would paint (#257).
            crate::stroke_constraint::draw_line_preview(app, &painter, &xf, p, tool, held.shift);
        }
        if crate::move_input::captured(&ctx) && app.drag.as_ref().is_some_and(|d| d.tool == Tool::Move) {
            ctx.set_cursor_icon(egui::CursorIcon::Default);
        }
    }
    // Scrollbars (scrollbars.rs): drawn over the canvas edges, they take the pointer there.
    let t0 = crate::gpu_canvas::now_ms();
    let before = view.center;
    let over_bars = crate::scrollbars::show(ui, rect, &mut view, flip, egui::Id::new(("pc-canvas", idx)));
    ctx.data_mut(|d| d.insert_temp(bars_id, over_bars));
    if view.center != before {
        ctx.request_repaint();
    }
    if primary {
        app.perf.span("scrollbars", crate::gpu_canvas::now_ms() - t0);
        app.hover_doc = response.hover_pos().map(|p| xf.to_doc(p));
    }
    if primary && app.ui.extras.rulers {
        crate::rulers::draw_rulers(app, ui, full, &xf);
    }
    if primary {
        app.ui.views[idx] = view.clone();
    }
    view
}

fn zoom_about(view: &mut View, xf: &ViewXform, p: Pos2, new_zoom: f32) {
    let before = xf.to_doc(p);
    crate::zoom_tool::anchor_view(view, xf, p, before, new_zoom);
}

/// Extra OS windows showing documents (multi-window / multi-monitor).
pub fn extra_windows(app: &mut PhotocraftApp, ctx: &egui::Context) {
    let wins = app.ui.windows.clone();
    for w in wins.into_iter().filter(|w| w.open) {
        let Some(st) = app.session.documents().get(w.document) else { continue };
        let title = format!("{} — window {}", st.doc.name, w.id);
        let vid = egui::ViewportId::from_hash_of(("docwin", w.id));
        let builder = egui::ViewportBuilder::default().with_title(title).with_inner_size([800.0, 600.0]);
        let mut view = w.view.clone();
        let mut close = false;
        ctx.show_viewport_immediate(vid, builder, |ui, _class| {
            if ui.ctx().input(|i| i.viewport().close_requested()) {
                close = true;
            }
            if app.ui.tool == Tool::RotateView && app.ui.view.panels_visible() {
                egui::Panel::top(egui::Id::new(("rotate-window-options", w.id))).show(ui, |ui| {
                    ui.horizontal_wrapped(|ui| rotate_view::options_for_view(&mut view, ui));
                });
            }
            egui::CentralPanel::default().frame(egui::Frame::NONE.fill(crate::theme::Tokens::get(ui.ctx()).canvas)).show(ui, |ui| {
                let rect = ui.available_rect_before_wrap();
                view = canvas_view(app, ui, w.document, rect, view.clone(), false);
            });
        });
        if let Some(win) = app.ui.windows.iter_mut().find(|x| x.id == w.id) {
            win.view = view;
            if close {
                win.open = false;
            }
        }
    }
    app.ui.windows.retain(|w| w.open);
}

// PhotoCraft 994a0421: a stationary tap uses the strongest contact sample.
fn tap_at_peak_pressure(app: &mut PhotocraftApp) {
    if app.stylus.sample().is_none() {
        return;
    }
    let peak = f64::from(app.stylus.tap_pressure());
    if let Some(p) = app.drag.as_mut().filter(|d| d.points.len() == 1).and_then(|d| d.points.first_mut()) {
        p[2] = p[2].max(peak);
    }
}

#[cfg(test)]
#[path = "canvas_brush_batch_tests.rs"]
mod brush_batch_tests;

#[cfg(test)]
mod tests;
