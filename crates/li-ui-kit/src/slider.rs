//! Shared slider geometry, continuous gradient painting and pointer/keyboard interaction.
//! Domain formatting, reset policy and numeric-edit ownership remain in UI adapters.

use egui::{Color32, Rect, Response, Sense, Stroke, Ui, pos2, vec2};

use crate::theme::Tokens;
use crate::widget_style as m;

/// Slider range and quantization, independent of Develop control specifications.
#[derive(Clone, Copy, Debug)]
pub struct Scale {
    pub min: f64,
    pub max: f64,
    pub step: Option<f64>,
}

impl Scale {
    fn span(self) -> f64 {
        (self.max - self.min).max(1e-9)
    }

    pub fn fraction(self, value: f64) -> f32 {
        ((value - self.min) / self.span()).clamp(0.0, 1.0) as f32
    }

    pub fn position(self, track: Rect, value: f64) -> f32 {
        track.left() + self.fraction(value) * track.width()
    }

    fn value_at(self, track: Rect, x: f32) -> f64 {
        self.min + ((x - track.left()) / track.width().max(f32::EPSILON)).clamp(0.0, 1.0) as f64 * self.span()
    }

    fn quantize(self, value: f64) -> f64 {
        let value = if let Some(step) = self.step {
            let step = step.max(1e-9);
            (value / step).round() * step
        } else {
            value
        };
        value.clamp(self.min, self.max)
    }

    /// A keyboard nudge is a whole number of steps, roughly 1/200 of the range.
    /// Adapters may override the unit (e.g. white-balance temperature uses 50 K).
    pub fn nudged(self, value: f64, steps: f64, unit: Option<f64>) -> f64 {
        let step = self.step.unwrap_or(self.span() / 200.0).max(1e-9);
        let unit = unit.unwrap_or_else(|| (self.span() / 200.0 / step).round().max(1.0) * step);
        self.quantize(value + unit * steps)
    }
}

#[derive(Clone, Copy)]
pub struct Interaction {
    pub scale: Scale,
    pub fine_factor: Option<f64>,
    pub epsilon: f64,
    pub hover_nudge: Option<Rect>,
    pub nudge_unit: Option<f64>,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct InteractionOut {
    pub value: Option<f64>,
    pub drag_started: bool,
    pub drag_stopped: bool,
}

/// Shared drag engine. Fine drags retain an unrounded accumulator across frames; release
/// records the final pointer position before retiring it. Call only when editing is admissible.
pub fn interact(ui: &Ui, response: &Response, track: Rect, value: f64, config: Interaction) -> InteractionOut {
    let mut out = InteractionOut::default();
    if !ui.is_enabled() || !ui.memory(|m| m.is_above_modal_layer(ui.layer_id())) {
        return out;
    }
    let drag_id = response.id.with("continuous-value");
    if response.drag_started() {
        out.drag_started = true;
        let x = ui.input(|i| i.pointer.press_origin()).map_or(track.center().x, |p| p.x);
        ui.data_mut(|d| d.insert_temp(drag_id, (value, x)));
    }
    let released = response.drag_stopped() && ui.input(|i| !i.pointer.primary_down());
    if (response.dragged() || response.drag_started() || released)
        && let Some(pointer) = response.interact_pointer_pos()
    {
        let fine = config.fine_factor.filter(|_| ui.input(|i| i.modifiers.shift));
        let (continuous, last_x) = ui.data(|d| d.get_temp::<(f64, f32)>(drag_id)).unwrap_or((value, pointer.x));
        let next = if let Some(factor) = fine {
            continuous + f64::from(pointer.x - last_x) * config.scale.span() / f64::from(track.width().max(f32::EPSILON)) * factor
        } else {
            config.scale.value_at(track, pointer.x)
        };
        let next = next.clamp(config.scale.min, config.scale.max);
        ui.data_mut(|d| d.insert_temp(drag_id, (next, pointer.x)));
        let next = config.scale.quantize(next);
        if (next - value).abs() > config.epsilon {
            out.value = Some(next);
        }
    } else if response.clicked()
        && let Some(pointer) = response.interact_pointer_pos()
    {
        out.value = Some(config.scale.quantize(config.scale.value_at(track, pointer.x)));
        out.drag_started = true;
        out.drag_stopped = true;
    }
    if response.drag_stopped() {
        out.drag_stopped = true;
        ui.data_mut(|d| d.remove_temp::<(f64, f32)>(drag_id));
    }
    if out.value.is_none() && !ui.ctx().egui_wants_keyboard_input() && config.hover_nudge.is_some_and(|row| ui.rect_contains_pointer(row)) {
        let (up, down, shift) = ui.input_mut(|i| {
            let shift = i.modifiers.shift;
            let modifiers = if shift { egui::Modifiers::SHIFT } else { egui::Modifiers::NONE };
            (i.consume_key(modifiers, egui::Key::ArrowUp), i.consume_key(modifiers, egui::Key::ArrowDown), shift)
        });
        if up || down {
            let next = config.scale.nudged(value, if up { 1.0 } else { -1.0 } * if shift { 5.0 } else { 1.0 }, config.nudge_unit);
            if (next - value).abs() > config.epsilon {
                out.value = Some(next);
                out.drag_started = true;
                out.drag_stopped = true;
            }
        }
    }
    out
}

/// One mesh for every stop interval, preserving exact colors at each stop. Compared with
/// sampled rectangle bands this eliminates quantization in position and overlapping edges.
pub fn gradient_mesh(track: Rect, colors: &[Color32]) -> egui::Mesh {
    let mut mesh = egui::Mesh::default();
    let intervals = colors.len().saturating_sub(1);
    if intervals == 0 {
        return mesh;
    }
    for (index, colors) in colors.windows(2).enumerate() {
        let left = track.left() + track.width() * index as f32 / intervals as f32;
        let right = track.left() + track.width() * (index + 1) as f32 / intervals as f32;
        let rect = Rect::from_min_max(pos2(left, track.top()), pos2(right, track.bottom()));
        let base = mesh.vertices.len() as u32;
        mesh.colored_vertex(rect.left_top(), colors[0]);
        mesh.colored_vertex(rect.right_top(), colors[1]);
        mesh.colored_vertex(rect.right_bottom(), colors[1]);
        mesh.colored_vertex(rect.left_bottom(), colors[0]);
        mesh.add_triangle(base, base + 1, base + 2);
        mesh.add_triangle(base, base + 2, base + 3);
    }
    mesh
}

fn paint_gradient(ui: &Ui, track: Rect, colors: Option<&[Color32]>) -> bool {
    if let Some(colors) = colors.filter(|colors| colors.len() >= 2) {
        ui.painter().add(gradient_mesh(track, colors));
        true
    } else {
        false
    }
}

/// Inherited hollow-thumb profile, with all palette roles supplied by theme tokens.
#[derive(Clone, Copy)]
pub struct HollowStyle {
    pub track: Color32,
    pub background: Color32,
    pub thumb: Color32,
    pub thumb_hover: Color32,
    pub disabled: Color32,
}

pub fn paint_hollow(ui: &Ui, track: Rect, colors: Option<&[Color32]>, thumb_x: f32, enabled: bool, hovered: bool, style: HollowStyle) {
    let y = track.center().y;
    let gradient = Rect::from_center_size(track.center(), vec2(track.width(), m::SLIDER_HOLLOW_GRADIENT_HEIGHT));
    if !paint_gradient(ui, gradient, colors) {
        ui.painter().rect_filled(
            Rect::from_center_size(track.center(), vec2(track.width(), m::SLIDER_HOLLOW_TRACK_HEIGHT)),
            m::SLIDER_HOLLOW_TRACK_RADIUS,
            style.track,
        );
    }
    let ring = m::SLIDER_HOLLOW_RADIUS;
    let thumb = pos2(thumb_x, y);
    let painter = ui.painter();
    painter.rect_filled(Rect::from_center_size(thumb, vec2(ring * 2.0 + m::SLIDER_HOLLOW_GAP_EXTRA, m::SLIDER_HOLLOW_GAP_HEIGHT)), 0.0, style.background);
    let color = if !enabled {
        style.disabled
    } else if hovered {
        style.thumb_hover
    } else {
        style.thumb
    };
    painter.circle_filled(thumb, ring, style.background);
    painter.circle_stroke(thumb, ring, Stroke::new(m::SLIDER_THUMB_STROKE, color));
}

/// Editor thin slider profile. Callers supply label metadata through their labelled row.
pub fn editor(ui: &mut Ui, value: &mut f32, scale: Scale, colors: Option<&[Color32]>) -> Response {
    let tokens = Tokens::get(ui.ctx());
    let palette = tokens.slider_colors();
    let width = ui.available_width().max(m::SLIDER_EDITOR_MIN_WIDTH);
    let (rect, mut response) = ui.allocate_exact_size(vec2(width, m::SLIDER_EDITOR_HEIGHT), Sense::click_and_drag());
    let track = Rect::from_center_size(
        rect.center(),
        vec2(rect.width() - m::SLIDER_EDITOR_TRACK_INSET, if colors.is_some() { m::SLIDER_EDITOR_GRADIENT_HEIGHT } else { m::SLIDER_EDITOR_TRACK_HEIGHT }),
    );
    let result = interact(
        ui,
        &response,
        track,
        f64::from(*value),
        Interaction { scale, fine_factor: None, epsilon: f64::from(f32::EPSILON), hover_nudge: None, nudge_unit: None },
    );
    if let Some(next) = result.value
        && (next - f64::from(*value)).abs() > f64::from(f32::EPSILON)
    {
        *value = next as f32;
        response.mark_changed();
    }
    let thumb = pos2(scale.position(track, f64::from(*value)), rect.center().y);
    let painter = ui.painter();
    if !paint_gradient(ui, track, colors) {
        painter.rect_filled(track, m::SLIDER_EDITOR_TRACK_RADIUS, tokens.field_border);
        painter.rect_filled(
            Rect::from_min_max(track.min, pos2(thumb.x, track.max.y)),
            m::SLIDER_EDITOR_TRACK_RADIUS,
            if tokens.bevel { tokens.accent } else { tokens.text_dim },
        );
    }
    if tokens.bevel {
        let knob = Rect::from_center_size(thumb, m::SLIDER_CLASSIC_KNOB_SIZE);
        crate::widgets::surface(ui, knob, tokens.card, true);
    } else {
        let radius = if tokens.pro { m::SLIDER_PRO_KNOB_RADIUS } else { m::SLIDER_STUDIO_KNOB_RADIUS };
        painter.circle_filled(thumb + m::SLIDER_KNOB_SHADOW_OFFSET, radius + m::SLIDER_KNOB_SHADOW_GROWTH, palette.knob_shadow);
        painter.circle_filled(thumb, radius, palette.knob);
        if tokens.pro {
            painter.circle_stroke(thumb, radius, Stroke::new(m::STROKE, palette.knob_border));
        }
        if response.hovered() || response.dragged() {
            painter.circle_stroke(thumb, m::SLIDER_KNOB_HOVER_RADIUS, Stroke::new(m::SLIDER_KNOB_HOVER_STROKE, tokens.accent_soft));
        }
    }
    response
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::Pos2;

    fn frame(ctx: &egui::Context, events: Vec<egui::Event>, value: &mut f64, fine: bool, enabled: bool) -> (Rect, InteractionOut) {
        let mut events = events;
        events.insert(0, egui::Event::ModifiersChanged(if fine { egui::Modifiers::SHIFT } else { egui::Modifiers::NONE }));
        let mut track = Rect::NOTHING;
        let mut result = InteractionOut::default();
        let mut output =
            ctx.run_ui(egui::RawInput { screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(250.0, 100.0))), events, ..Default::default() }, |ui| {
                ui.add_enabled_ui(enabled, |ui| {
                    let (rect, response) = ui.allocate_exact_size(vec2(200.0, 18.0), Sense::click_and_drag());
                    track = rect;
                    result = interact(
                        ui,
                        &response,
                        track,
                        *value,
                        Interaction {
                            scale: Scale { min: 0.0, max: 100.0, step: Some(1.0) },
                            fine_factor: Some(0.1),
                            epsilon: 1e-12,
                            hover_nudge: Some(track),
                            nudge_unit: None,
                        },
                    );
                    if let Some(next) = result.value {
                        *value = next;
                    }
                });
            });
        output.textures_delta.clear();
        (track, result)
    }

    fn button(pos: egui::Pos2, pressed: bool, fine: bool) -> egui::Event {
        egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: if fine { egui::Modifiers::SHIFT } else { egui::Modifiers::NONE },
        }
    }

    #[test]
    fn slider_pointer_drag_clamps_and_commits_release() {
        let ctx = egui::Context::default();
        let mut value = 50.0;
        let (track, _) = frame(&ctx, vec![], &mut value, false, true);
        let origin = track.center();
        frame(&ctx, vec![egui::Event::PointerMoved(origin), button(origin, true, false)], &mut value, false, true);
        let (_, moved) = frame(&ctx, vec![egui::Event::PointerMoved(track.right_center() + vec2(40.0, 0.0))], &mut value, false, true);
        assert!(moved.drag_started);
        assert_eq!(value, 100.0);
        let end = track.left_center() - vec2(20.0, 0.0);
        let (_, released) = frame(&ctx, vec![egui::Event::PointerMoved(end), button(end, false, false)], &mut value, false, true);
        assert!(released.drag_stopped);
        assert_eq!(value, 0.0, "release must retain the last pointer motion");
    }

    #[test]
    fn slider_shift_drag_retains_substep_motion_and_disabled_is_inert() {
        let ctx = egui::Context::default();
        let mut value = 50.0;
        let (track, _) = frame(&ctx, vec![], &mut value, true, true);
        let origin = track.center();
        frame(&ctx, vec![egui::Event::PointerMoved(origin), button(origin, true, true)], &mut value, true, true);
        for pixels in 8..=20 {
            frame(&ctx, vec![egui::Event::PointerMoved(origin + vec2(pixels as f32, 0.0))], &mut value, true, true);
        }
        assert_eq!(value, 51.0, "fine motion must accumulate instead of sticking after rounding each delta");
        frame(&ctx, vec![button(origin + vec2(20.0, 0.0), false, true)], &mut value, true, true);
        frame(&ctx, vec![egui::Event::PointerMoved(origin), button(origin, true, false)], &mut value, false, false);
        let (_, disabled) = frame(&ctx, vec![egui::Event::PointerMoved(track.right_center())], &mut value, false, false);
        assert!(disabled.value.is_none());
        assert_eq!(value, 51.0);
    }

    #[test]
    fn slider_scale_matches_develop_quantization_and_nudges() {
        for (min, max, step, unit) in [(-5.0, 5.0, 0.01, None), (-100.0, 100.0, 1.0, None), (2000.0, 50000.0, 1.0, Some(50.0))] {
            let scale = Scale { min, max, step: Some(step) };
            for value in [min, (min + max) / 2.0, max] {
                for steps in [-5.0, -1.0, 1.0, 5.0] {
                    let inherited_unit = unit.unwrap_or_else(|| ((max - min) / 200.0 / step).round().max(1.0) * step);
                    let inherited = (((value + inherited_unit * steps) / step).round() * step).clamp(min, max);
                    assert_eq!(scale.nudged(value, steps, unit), inherited);
                }
            }
        }
        let fixed = Scale { min: 7.0, max: 7.0, step: None };
        assert_eq!(fixed.fraction(7.0), 0.0);
        assert_eq!(fixed.value_at(Rect::ZERO, 0.0), 7.0);
    }

    #[test]
    fn slider_gradient_improves_sample_error_and_reduces_primitives() {
        let track = Rect::from_min_size(Pos2::ZERO, vec2(200.0, 3.0));
        let mesh = gradient_mesh(track, &[Color32::BLACK, Color32::WHITE]);
        assert_eq!(mesh.vertices.len(), 4);
        assert_eq!(mesh.indices.len(), 6);
        let mut inherited_max = 0.0_f32;
        let mut shared_max = 0.0_f32;
        for index in 0..=96 {
            let fraction = index as f32 / 96.0;
            let ideal = 255.0 * fraction;
            let sampled = (((fraction * 48.0).floor().min(47.0) / 48.0) * 255.0).round();
            let shared = f32::from(mesh.vertices[0].color.r()) * (1.0 - fraction) + f32::from(mesh.vertices[1].color.r()) * fraction;
            inherited_max = inherited_max.max((sampled - ideal).abs());
            shared_max = shared_max.max((shared - ideal).abs());
        }
        assert_eq!(inherited_max, 5.0);
        assert_eq!(shared_max, 0.0);
        println!(
            "gradient: 97 edge/midpoint samples, maximum grayscale channel error {inherited_max} -> {shared_max}; Library gradient primitives 48 rectangles -> 1 mesh (2 triangles for two stops)"
        );
        for count in 2..=8 {
            let colors: Vec<_> = (0..count).map(|index| Color32::from_rgb(index as u8 * 30, 0, 0)).collect();
            let mesh = gradient_mesh(track, &colors);
            assert_eq!(mesh.indices.len(), (count - 1) * 6);
            for (index, pair) in colors.windows(2).enumerate() {
                assert_eq!(mesh.vertices[index * 4].color, pair[0]);
                assert_eq!(mesh.vertices[index * 4 + 1].color, pair[1]);
            }
        }
    }
}
