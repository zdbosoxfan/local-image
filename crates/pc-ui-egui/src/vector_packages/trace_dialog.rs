//! Package 28 modal. All pixels/paths are detached; workers use the existing T1 job API.
//! The package 00 menu/router/preferences hooks are in integration/28.md.
use egui::{Color32, Context, Pos2, Rect, Sense, Stroke, TextureHandle, Ui, Vec2};
use photocraft_engine::{
    Session,
    jobs::{JobId, Started},
    vector_packages::trace_output::{self, Capture, Output, PaletteEdit, Params, Preset, Preview},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// Add to the ordinary UI preferences with #[serde(default)]. No document migration.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Preferences {
    pub last_preset: Preset,
}
impl Default for Preferences {
    fn default() -> Self {
        Self { last_preset: Preset::Logo }
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum View {
    #[default]
    Split,
    Result,
    Outlines,
    Overlay,
}
pub struct State {
    pub open: bool,
    pub params: Params,
    pub output: Output,
    pub view: View,
    pub zoom: f32,
    pub pan: Vec2,
    pub split: f32,
    pub preview: Option<Preview>,
    pub error: Option<String>,
    pub preview_rect: Rect,
    pub generation: u64,
    pub capture: Option<Capture>,
    preview_job: Option<JobId>,
    preview_rx: Option<std::sync::mpsc::Receiver<Preview>>,
    apply_job: Option<JobId>,
    source_texture: Option<TextureHandle>,
    result_texture: Option<TextureHandle>,
    selected_color: Option<[u8; 3]>,
    merge_color: Option<[u8; 3]>,
    auto_colors: bool,
    use_swatches: bool,
    dragging_controls: bool,
}
impl Default for State {
    fn default() -> Self {
        Self {
            open: false,
            params: Params::default(),
            output: Output::default(),
            view: View::Split,
            zoom: 1.,
            pan: Vec2::ZERO,
            split: 0.5,
            preview: None,
            error: None,
            preview_rect: Rect::NOTHING,
            generation: 0,
            capture: None,
            preview_job: None,
            preview_rx: None,
            apply_job: None,
            source_texture: None,
            result_texture: None,
            selected_color: None,
            merge_color: None,
            auto_colors: true,
            use_swatches: false,
            dragging_controls: false,
        }
    }
}
impl State {
    pub fn open(&mut self, session: &mut Session, prefs: &Preferences) {
        self.cancel(session);
        self.open = true;
        let preset = if available(prefs.last_preset) { prefs.last_preset } else { Preset::Logo };
        self.set_preset(preset);
        self.zoom = 1.;
        self.pan = Vec2::ZERO;
        self.error = None;
        match trace_output::capture(session, None) {
            Ok(capture) => {
                self.capture = Some(capture);
                self.request_preview(session, 0.5);
            }
            Err(e) => self.error = Some(e.to_string()),
        }
    }
    pub fn set_preset(&mut self, preset: Preset) {
        self.params = Params::for_preset(preset);
        self.output.work_path = false;
        self.output.palette_edits.clear();
        self.selected_color = None;
        self.merge_color = None;
        self.use_swatches = false;
        self.auto_colors = matches!(preset, Preset::Logo | Preset::Photo);
    }
    fn arguments(&self, scale: f64) -> Value {
        json!({"layer":self.capture.as_ref().map(|c| c.layer.0),"capture":self.capture,"params":self.params,"output":self.output,"scale":scale})
    }
    pub fn cancel(&mut self, session: &mut Session) {
        for id in [self.preview_job.take(), self.apply_job.take()].into_iter().flatten() {
            session.cancel_job(id);
        }
        self.open = false;
        self.preview_rx = None;
        self.capture = None;
        self.preview = None;
        self.source_texture = None;
        self.result_texture = None;
        self.dragging_controls = false;
        self.generation = self.generation.wrapping_add(1);
    }
    pub fn disabled_reason(&self, session: &Session) -> Option<String> {
        if self.apply_job.is_some() {
            return Some(tl!("Applying trace…").into());
        }
        let Some(c) = &self.capture else {
            return Some(self.error.clone().unwrap_or_else(|| tl!("Vectorize requires a pixel layer or smart object").into()));
        };
        trace_output::check(session, c)
            .and_then(|()| self.params.validate().map_err(trace_output::error))
            .and_then(|()| self.output.validate(&self.params))
            .err()
            .map(|e| e.to_string())
    }
    pub fn request_preview(&mut self, session: &mut Session, scale: f64) {
        self.preview_rx = None;
        if let Some(id) = self.preview_job.take() {
            session.cancel_job(id);
        }
        self.generation = self.generation.wrapping_add(1);
        if let Some(reason) = self.disabled_reason(session) {
            self.error = Some(reason);
            return;
        }
        self.error = None;
        let Some(capture) = &self.capture else { return };
        match trace_output::start_preview(session, capture, &self.params, &self.output, scale) {
            Ok((started, rx)) => {
                self.preview_rx = Some(rx);
                if let Started::Job(id) = started {
                    self.preview_job = Some(id);
                }
            }
            Err(e) => self.error = Some(e.to_string()),
        }
    }
    /// App polls its Session jobs once per frame; typed results bypass large JSON copies.
    /// The mailbox belongs only to the newest request and is dropped on every cancellation.
    pub fn poll(&mut self, session: &mut Session) {
        if let Some(id) = self.apply_job
            && session.job(id).is_none()
        {
            self.apply_job = None;
            if let Some(info) = session.jobs_with_recent().into_iter().find(|j| j.id == id) {
                if info.state == "done" {
                    self.cancel(session);
                    return;
                }
                self.error = info.error;
            } else {
                self.error = Some(tl!("Trace job result is no longer available").into());
            }
        }
        if let Some(c) = &self.capture
            && let Err(e) = trace_output::check(session, c)
        {
            if let Some(id) = self.preview_job.take() {
                session.cancel_job(id);
            }
            self.preview_rx = None;
            self.preview = None;
            self.source_texture = None;
            self.result_texture = None;
            self.error = Some(e.to_string());
            return;
        }
        if let Some(id) = self.preview_job
            && session.job(id).is_none()
        {
            self.preview_job = None;
            if let Some(info) = session.jobs_with_recent().into_iter().find(|j| j.id == id)
                && info.state != "done"
            {
                self.error = info.error;
                self.preview_rx = None;
            }
        }
        if let Some(rx) = &self.preview_rx
            && let Ok(preview) = rx.try_recv()
        {
            if Some(&preview.capture) == self.capture.as_ref() {
                self.preview = Some(preview);
                self.source_texture = None;
                self.result_texture = None;
            }
            self.preview_rx = None;
        }
    }

    pub fn apply(&mut self, session: &mut Session) {
        self.preview_rx = None;
        if let Some(reason) = self.disabled_reason(session) {
            self.error = Some(reason);
            return;
        }
        if let Some(id) = self.preview_job.take() {
            session.cancel_job(id);
        }
        match session.start("layer.vectorize", self.arguments(1.)) {
            Ok(Started::Job(id)) => self.apply_job = Some(id),
            Ok(Started::Done(_)) => self.cancel(session),
            Err(e) => self.error = Some(e.to_string()),
        }
    }
    pub fn pending_preview(&self) -> Option<JobId> {
        self.preview_job
    }
    pub fn pending_apply(&self) -> Option<JobId> {
        self.apply_job
    }
}
fn available(preset: Preset) -> bool {
    !matches!(preset, Preset::LineArt | Preset::Sketch)
}
fn preset_label(preset: Preset) -> &'static str {
    match preset {
        Preset::Logo => tl!("Logo"),
        Preset::BlackWhite => tl!("Black & White"),
        Preset::FewColors => tl!("Few colours"),
        Preset::Silhouette => tl!("Silhouette"),
        Preset::LineArt => tl!("Line Art"),
        Preset::Sketch => tl!("Sketch"),
        Preset::Photo => tl!("Photo (high fidelity)"),
        Preset::PixelArt => tl!("Pixel Art"),
    }
}
const PRESETS: [Preset; 8] =
    [Preset::Logo, Preset::BlackWhite, Preset::FewColors, Preset::Silhouette, Preset::LineArt, Preset::Sketch, Preset::Photo, Preset::PixelArt];
/// Swatches are supplied by the document/palette owner. An empty library disables the
/// option, instead of quietly substituting the shell's hardcoded colour grid.
pub fn controls(ui: &mut Ui, state: &mut State, session: &mut Session, prefs: &mut Preferences, swatches: &[[u8; 3]]) {
    let mut changed = false;
    egui::ComboBox::from_label(tl!("Preset")).selected_text(preset_label(state.params.preset)).show_ui(ui, |ui| {
        for preset in PRESETS {
            let r = ui
                .add_enabled(available(preset), egui::Button::selectable(state.params.preset == preset, preset_label(preset)))
                .on_disabled_hover_text(tl!("Centreline tracing is not available yet"));
            if r.clicked() {
                state.set_preset(preset);
                prefs.last_preset = preset;
                changed = true;
            }
        }
    });
    ui.horizontal(|ui| {
        ui.label(tl!("Mode"));
        let _ = ui.selectable_label(true, tl!("Fill")).on_hover_text(tl!("Trace filled regions"));
        ui.add_enabled(false, egui::Button::new(tl!("Centreline"))).on_disabled_hover_text(tl!("Centreline tracing is not available yet"));
    });
    ui.horizontal(|ui| {
        ui.label(tl!("Layering"));
        let _ = ui.selectable_label(true, tl!("Stacked"));
        ui.add_enabled(false, egui::Button::new(tl!("Cutout"))).on_disabled_hover_text(tl!("Cutout tracing is not available yet"));
    });
    let binary = matches!(state.params.preset, Preset::BlackWhite | Preset::Silhouette);
    changed |= ui.add_enabled(!binary, egui::Checkbox::new(&mut state.auto_colors, tl!("Auto colours"))).changed();
    let colours = ui
        .add_enabled(!state.auto_colors && !binary, egui::Slider::new(&mut state.params.max_colors, if binary { 1..=64 } else { 2..=64 }).text(tl!("Colours")));
    changed |= colours.changed();
    if state.auto_colors {
        state.params.max_colors = if state.params.preset == Preset::Photo { 64 } else { 8 };
    }
    let use_swatches = ui
        .add_enabled(!swatches.is_empty(), egui::Checkbox::new(&mut state.use_swatches, tl!("Use document swatches")))
        .on_disabled_hover_text(tl!("This document has no swatches"));
    changed |= use_swatches.changed();
    if state.use_swatches {
        state.params.palette = swatches.iter().copied().take(64).collect();
    } else {
        state.params.palette.clear();
    }
    changed |= palette(ui, state);
    for response in [
        ui.add(egui::Slider::new(&mut state.params.detail, 0.01..=16.).logarithmic(true).text(tl!("Detail"))),
        ui.add(egui::Slider::new(&mut state.params.alphamax, 0.0..=4.).text(tl!("Corners"))),
        ui.add(egui::Slider::new(&mut state.params.speckle, 0..=1024).text(tl!("Noise"))),
    ] {
        changed |= response.changed();
    }
    if state.params.preset == Preset::BlackWhite {
        changed |= ui.checkbox(&mut state.params.adaptive, tl!("Adaptive threshold")).changed();
        changed |= ui.add_enabled(!state.params.adaptive, egui::Slider::new(&mut state.params.threshold, 0..=255).text(tl!("Threshold"))).changed();
        if state.params.adaptive {
            changed |= ui.add(egui::Slider::new(&mut state.params.adaptive_sensitivity, 0.0..=100.).text(tl!("Sensitivity"))).changed();
        }
    }
    changed |= ui.checkbox(&mut state.params.use_alpha, tl!("Use alpha")).changed();
    let mut ignore = state.params.ignore_color.is_some();
    if ui.checkbox(&mut ignore, tl!("Ignore colour")).changed() {
        state.params.ignore_color = ignore.then_some([255; 3]);
        changed = true;
    }
    if let Some(color) = &mut state.params.ignore_color {
        changed |= ui.color_edit_button_srgb(color).changed();
    }
    ui.add_enabled_ui(false, |ui| {
        ui.label(tl!("Smoothing"));
        ui.checkbox(&mut false, tl!("Snap lines"));
        ui.checkbox(&mut false, tl!("Snap circles"));
        ui.checkbox(&mut false, tl!("Symmetry"));
        egui::ComboBox::from_id_salt("trace-upscale").selected_text(tl!("Upscale: 1×")).show_ui(ui, |ui| {
            ui.label(tl!("Auto / 2× / 4×"));
        });
    })
    .response
    .on_disabled_hover_text(tl!("Logo cleanup is not available yet"));
    ui.separator();
    ui.checkbox(&mut state.output.expand, tl!("Expand to shape layers"));
    ui.add_enabled(state.params.preset == Preset::BlackWhite, egui::Checkbox::new(&mut state.output.work_path, tl!("Make Work Path")))
        .on_disabled_hover_text(tl!("Make Work Path requires Black & White"));
    if let Some(p) = &state.preview {
        let paths: usize = p.trace.layers.iter().map(|l| l.path.subpaths.len()).sum();
        ui.label(crate::i18n::fmt(
            tl!("{colours} colours · {paths} paths · {nodes} nodes · {time} ms"),
            &[
                ("colours", &p.trace.layers.len().to_string()),
                ("paths", &paths.to_string()),
                ("nodes", &p.trace.nodes.to_string()),
                ("time", &format!("{:.0}", p.elapsed_ms)),
            ],
        ));
    } else if state.preview_job.is_some() {
        ui.label(tl!("Tracing preview…"));
    }
    if let Some(e) = &state.error {
        ui.colored_label(Color32::RED, e);
    }
    let reason = state.disabled_reason(session);
    ui.horizontal(|ui| {
        if ui.add_enabled(reason.is_none(), egui::Button::new(tl!("Trace"))).on_disabled_hover_text(reason.as_deref().unwrap_or("")).clicked() {
            state.apply(session);
        }
        if ui.button(tl!("Cancel")).clicked() {
            state.cancel(session);
        }
    });
    let down = ui.input(|i| i.pointer.primary_down());
    if state.open && state.apply_job.is_none() {
        if changed {
            state.request_preview(session, if down { 0.25 } else { 1. });
        } else if state.dragging_controls && !down {
            state.request_preview(session, 1.);
        }
    }
    state.dragging_controls = changed && down || state.dragging_controls && down;
}
fn hex(c: [u8; 3]) -> String {
    format!("#{:02X}{:02X}{:02X}", c[0], c[1], c[2])
}
fn palette(ui: &mut Ui, state: &mut State) -> bool {
    let colors: Vec<[u8; 3]> =
        state.preview.as_ref().map(|p| p.trace.layers.iter().map(|l| [l.color[0], l.color[1], l.color[2]]).collect()).unwrap_or_default();
    ui.horizontal_wrapped(|ui| {
        for color in &colors {
            let button = egui::Button::selectable(state.selected_color == Some(*color), hex(*color)).fill(Color32::from_rgb(color[0], color[1], color[2]));
            if ui.add(button).clicked() {
                state.selected_color = Some(*color);
                state.merge_color = colors.iter().copied().find(|c| c != color);
            }
        }
    });
    let mut changed = false;
    if let Some(from) = state.selected_color {
        ui.horizontal(|ui| {
            if ui.button(tl!("Remove colour")).clicked() {
                state.output.palette_edits.push(PaletteEdit { from, to: None });
                state.selected_color = None;
                changed = true;
            }
            egui::ComboBox::from_id_salt("trace-merge").selected_text(state.merge_color.map(hex).unwrap_or_default()).show_ui(ui, |ui| {
                for color in colors.iter().filter(|c| **c != from) {
                    ui.selectable_value(&mut state.merge_color, Some(*color), hex(*color));
                }
            });
            if ui.add_enabled(state.merge_color.is_some(), egui::Button::new(tl!("Merge colour"))).clicked() {
                state.output.palette_edits.push(PaletteEdit { from, to: state.merge_color });
                state.selected_color = None;
                changed = true;
            }
        });
    }
    if !state.output.palette_edits.is_empty() && ui.button(tl!("Reset palette")).clicked() {
        state.output.palette_edits.clear();
        changed = true;
    }
    changed
}
/// Preview-only view. The canvas transform, document and history are never changed.
pub fn preview(ui: &mut Ui, state: &mut State) {
    ui.horizontal(|ui| {
        for (view, label) in [(View::Split, tl!("Split")), (View::Result, tl!("Result")), (View::Outlines, tl!("Outlines")), (View::Overlay, tl!("Overlay"))] {
            ui.selectable_value(&mut state.view, view, label);
        }
        if ui.button(tl!("Fit preview")).clicked() {
            state.zoom = 1.;
            state.pan = Vec2::ZERO;
        }
    });
    let size = egui::vec2(ui.available_width().max(100.), ui.available_height().max(180.));
    let (rect, response) = ui.allocate_exact_size(size, Sense::drag());
    state.preview_rect = rect;
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 0., Color32::from_gray(64));
    if response.hovered() {
        let wheel = ui.input(|i| i.smooth_scroll_delta.y);
        if wheel.is_finite() && wheel != 0. {
            let old = state.zoom;
            state.zoom = (state.zoom * (wheel * 0.005).exp()).clamp(0.1, 64.);
            if let Some(pointer) = response.hover_pos() {
                let offset = pointer - rect.center();
                state.pan = offset - (offset - state.pan) * (state.zoom / old);
            }
        }
    }
    let split_x = rect.left() + state.split * rect.width();
    let handle = Rect::from_min_max(egui::pos2(split_x - 5., rect.top()), egui::pos2(split_x + 5., rect.bottom()));
    let divider = (state.view == View::Split).then(|| ui.interact(handle, response.id.with("split"), Sense::drag()));
    if let Some(divider) = &divider
        && divider.dragged()
        && let Some(p) = divider.interact_pointer_pos()
    {
        state.split = ((p.x - rect.left()) / rect.width()).clamp(0., 1.);
    } else if response.dragged() {
        let delta = ui.input(|i| i.pointer.delta());
        if delta.is_finite() {
            state.pan = (state.pan + delta).clamp(Vec2::splat(-1_000_000.), Vec2::splat(1_000_000.));
        }
    }
    let Some(p) = &state.preview else { return };
    let size = [p.trace.width as usize, p.trace.height as usize];
    if state.source_texture.is_none() {
        state.source_texture =
            Some(ui.ctx().load_texture("trace-source", egui::ColorImage::from_rgba_unmultiplied(size, &p.source_rgba), egui::TextureOptions::LINEAR));
        state.result_texture =
            Some(ui.ctx().load_texture("trace-result", egui::ColorImage::from_rgba_unmultiplied(size, &p.result_rgba), egui::TextureOptions::LINEAR));
    }
    let dimensions = egui::vec2(p.bounds[2] as f32, p.bounds[3] as f32);
    let fit = (rect.width() / dimensions.x).min(rect.height() / dimensions.y);
    let image = Rect::from_center_size(rect.center() + state.pan, dimensions * fit * state.zoom);
    let uv = Rect::from_min_max(Pos2::ZERO, egui::pos2(1., 1.));
    // Checkerboard is clipped to the image and viewport.
    let visible = image.intersect(rect);
    let cell = 12.;
    for y in 0..(visible.height() / cell).ceil() as usize {
        for x in 0..(visible.width() / cell).ceil() as usize {
            let tile = Rect::from_min_size(visible.min + egui::vec2(x as f32 * cell, y as f32 * cell), Vec2::splat(cell)).intersect(visible);
            painter.rect_filled(tile, 0., Color32::from_gray(if (x + y) % 2 == 0 { 180 } else { 220 }));
        }
    }
    if let (Some(source), Some(result)) = (&state.source_texture, &state.result_texture) {
        match state.view {
            View::Split => {
                let split_x = rect.left() + state.split * rect.width();
                painter.with_clip_rect(Rect::from_min_max(rect.min, egui::pos2(split_x, rect.bottom()))).image(source.id(), image, uv, Color32::WHITE);
                painter.with_clip_rect(Rect::from_min_max(egui::pos2(split_x, rect.top()), rect.max)).image(result.id(), image, uv, Color32::WHITE);
                painter.line_segment([egui::pos2(split_x, rect.top()), egui::pos2(split_x, rect.bottom())], Stroke::new(2., Color32::WHITE));
            }
            View::Result => {
                painter.image(result.id(), image, uv, Color32::WHITE);
            }
            View::Overlay => {
                painter.image(source.id(), image, uv, Color32::WHITE);
                painter.image(result.id(), image, uv, Color32::from_white_alpha(128));
            }
            View::Outlines => {}
        }
    }
    if matches!(state.view, View::Outlines | View::Overlay) {
        let map = |x: f64, y: f64| {
            image.min + egui::vec2((x / f64::from(p.trace.width)) as f32 * image.width(), (y / f64::from(p.trace.height)) as f32 * image.height())
        };
        for layer in &p.trace.layers {
            for polyline in photocraft_vector::flatten_path(&layer.path, 0.25 / f64::from(state.zoom)) {
                let mut points: Vec<_> = polyline.pts.iter().map(|&(x, y)| map(x, y)).collect();
                if polyline.closed
                    && let Some(first) = points.first().copied()
                {
                    points.push(first);
                }
                painter.add(egui::Shape::line(points, Stroke::new(1., Color32::from_rgb(60, 180, 255))));
            }
            for knot in layer.path.subpaths.iter().flat_map(|s| &s.knots) {
                painter.circle_filled(map(knot.anchor.x, knot.anchor.y), 2., Color32::WHITE);
            }
        }
    }
}
/// Call after the shell's poll_jobs and after other compositing panels. Cancel also on
/// module/persona/focus changes via the integration hook; no desktop window is opened.
pub fn show(ctx: &Context, state: &mut State, session: &mut Session, prefs: &mut Preferences, swatches: &[[u8; 3]]) {
    if !state.open {
        return;
    }
    state.poll(session);
    if !state.open {
        return;
    }
    if !ctx.input(|i| i.focused) {
        state.cancel(session);
        return;
    }
    let modal = egui::Modal::new(egui::Id::new("vectorize-dialog")).show(ctx, |ui| {
        ui.heading(tl!("Vectorize / Image Trace"));
        egui::Resize::default()
            .id_salt("trace-dialog-size")
            .default_size(egui::vec2(900., 600.))
            .min_size(egui::vec2(700., 500.))
            .max_size((ctx.content_rect().size() - Vec2::splat(40.)).max(egui::vec2(700., 500.)))
            .show(ui, |ui| {
                ui.horizontal_top(|ui| {
                    let height = ui.available_height();
                    let width = (ui.available_width() - 320.).max(350.);
                    ui.allocate_ui_with_layout(egui::vec2(width, height), egui::Layout::top_down(egui::Align::Min), |ui| preview(ui, state));
                    ui.allocate_ui_with_layout(egui::vec2(300., height), egui::Layout::top_down(egui::Align::Min), |ui| {
                        egui::ScrollArea::vertical().show(ui, |ui| controls(ui, state, session, prefs, swatches));
                    });
                });
            });
    });
    if modal.should_close() {
        state.cancel(session);
    }
    if state.preview_job.is_some() || state.apply_job.is_some() {
        ctx.request_repaint_after(std::time::Duration::from_millis(16));
    }
}

#[cfg(test)]
#[path = "trace_dialog/gpu_tests.rs"]
mod gpu_tests;
#[cfg(test)]
#[path = "trace_dialog/tests.rs"]
mod tests;
