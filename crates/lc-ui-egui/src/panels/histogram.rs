//! Develop histogram and cached photo metadata.

use super::detail::Gesture;
use crate::widgets::{numeric_owner, primary_press, primary_pressed};
use egui::{Align2, Pos2, Rect, Response, pos2};
use lightcraft_catalog::PhotoId;
use lightcraft_develop::{ControlSpec, controls};
use serde_json::json;

use crate::{LightcraftApp, render::Slot, theme::Tokens, widgets::register};

pub(super) fn show(app: &mut LightcraftApp, ui: &mut egui::Ui, id: PhotoId) {
    let t = Tokens::get(ui.ctx());
    let resp = li_ui_kit::histogram::library_shell(ui, app.ui.show_clipping, li_i18n::library::t("Histogram"));
    let r = resp.rect;
    let plot = li_ui_kit::histogram::library_plot_rect(r);
    register(ui.ctx(), "histogram", r);
    let active = interact(app, ui, &resp, plot, id);
    if let Some(active) = active {
        if let Some(readout) = active.readout() {
            resp.widget_info(|| {
                egui::WidgetInfo::selected(
                    egui::WidgetType::Button,
                    ui.is_enabled(),
                    app.ui.show_clipping,
                    format!("{}: {readout}", li_i18n::library::t("Histogram")),
                )
            });
        }
        let tint = if active.dragging { t.pressed } else { t.hover };
        ui.painter().rect_filled(active.zone.rect(plot), 0.0, tint.gamma_multiply(if active.dragging { 0.75 } else { 0.45 }));
    }
    // Borrow cached bins: no image-sized work or per-frame histogram allocation.
    let hist = app
        .renderer
        .textures
        .get(&Slot::Main)
        .filter(|t| t.photo == id)
        .and_then(|t| t.histogram.as_ref())
        .or_else(|| app.renderer.textures.get(&Slot::Thumb(id)).and_then(|t| t.histogram.as_ref()));
    let view = hist.map(|h| li_ui_kit::histogram::Histogram::u32([&h.r, &h.g, &h.b], Some(&h.luma)));
    li_ui_kit::histogram::paint_library_contents(ui, r, view, hist.map_or((0.0, 0.0), |h| h.clipping()), app.ui.show_clipping);
    let p = ui.painter();
    if let Some(readout) = active.and_then(|active| active.readout()) {
        p.text(pos2(r.center().x, r.bottom() - 13.0), Align2::CENTER_CENTER, readout, t.font(12.5), t.text_label);
    } else if let Some(ph) = app.session.catalog.photo(id) {
        let m = &ph.meta;
        let items = [
            m.iso.map(|i| format!("ISO {i}")).unwrap_or_default(),
            m.focal_mm.map(|f| format!("{f:.0}mm")).unwrap_or_default(),
            m.aperture.map(|a| format!("f/{a:.1}")).unwrap_or_default(),
            m.shutter.clone(),
        ];
        let y = r.bottom() - 13.0;
        for (i, s) in items.iter().enumerate() {
            let x = r.left() + 18.0 + (r.width() - 36.0) * i as f32 / 3.0;
            let align = match i {
                0 => Align2::LEFT_CENTER,
                3 => Align2::RIGHT_CENTER,
                _ => Align2::CENTER_CENTER,
            };
            p.text(pos2(x, y), align, s, t.font(12.5), t.text_label);
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Zone {
    control: &'static str,
    from: f32,
    to: f32,
}
const ZONES: [Zone; 5] = [
    Zone { control: "light.blacks", from: 0.0, to: 0.15 },
    Zone { control: "light.shadows", from: 0.15, to: 0.35 },
    Zone { control: "light.exposure", from: 0.35, to: 0.65 },
    Zone { control: "light.highlights", from: 0.65, to: 0.85 },
    Zone { control: "light.whites", from: 0.85, to: 1.0 },
];
impl Zone {
    fn spec(self) -> Option<&'static ControlSpec> {
        controls::find(self.control)
    }
    fn rect(self, plot: Rect) -> Rect {
        Rect::from_min_max(pos2(plot.left() + plot.width() * self.from, plot.top()), pos2(plot.left() + plot.width() * self.to, plot.bottom()))
    }
}
fn zone_at(x: f32) -> Option<Zone> {
    if !(0.0..=1.0).contains(&x) {
        return None;
    }
    ZONES.iter().copied().find(|zone| x >= zone.from && (x < zone.to || zone.to == 1.0))
}
fn dragged(spec: &ControlSpec, base: f64, dx: f32, width: f32, fine: bool) -> f64 {
    if !width.is_finite() || width <= 0.0 || !dx.is_finite() || !base.is_finite() {
        return base;
    }
    (base + f64::from(dx) / f64::from(width) * (spec.max - spec.min) * if fine { 0.1 } else { 1.0 }).clamp(spec.min, spec.max)
}
fn quantized(spec: &ControlSpec, value: f64) -> f64 {
    if !value.is_finite() {
        return spec.default;
    }
    ((value / spec.step).round() * spec.step).clamp(spec.min, spec.max)
}
#[derive(Clone, Copy)]
struct Active {
    zone: Zone,
    value: f64,
    dragging: bool,
}
impl Active {
    fn readout(self) -> Option<String> {
        let spec = self.zone.spec()?;
        let value = spec.format(self.value);
        let value = if spec.id == "light.exposure" { format!("{value} EV") } else { value };
        Some(format!("{}: {value}", li_i18n::library::t(spec.label)))
    }
}
#[derive(Clone)]
struct DragOwner {
    owner: egui::Id,
    frame: u64,
    previous: Pos2,
    expected: f64,
    dragging: bool,
}

fn interact(app: &mut LightcraftApp, ui: &egui::Ui, response: &Response, plot: Rect, id: PhotoId) -> Option<Active> {
    let clips = li_ui_kit::histogram::library_clipping_rects(plot);
    let zone_of = |point: Pos2| {
        (plot.is_finite() && plot.width() > 0.0 && plot.contains(point) && !clips.iter().any(|clip| clip.contains(point)))
            .then(|| (point.x - plot.left()) / plot.width())
            .and_then(zone_at)
    };
    let hovered = response.hover_pos().and_then(zone_of);
    let press = primary_press(ui, response);
    let pressed = press.and_then(zone_of);
    let state_id = response.id.with("histogram-owner");
    let owner = numeric_owner(app, "histogram");
    let mut state = ui.data(|data| data.get_temp::<DragOwner>(state_id));
    if let Some(Gesture::HistogramDrag { control, .. }) = app.gesture {
        let owns = app.ui.dragging_control.as_deref() == Some(control)
            && app.session.interaction.as_ref().is_some_and(|interaction| interaction.photo == id);
        if !owns {
            app.gesture = None;
            state = None;
        } else if state.as_ref().is_none_or(|state| {
            state.frame.saturating_add(1) < ui.ctx().cumulative_frame_nr()
                || state.owner != owner
                || controls::find(control).is_none_or(|spec| value_of(app, id, spec) != state.expected)
        }) {
            finish(app, ui);
            state = None;
        }
    }
    if ui.is_enabled()
        && primary_pressed(ui, response)
        && app.ui.dragging_control.is_none()
        && let (Some(zone), Some(press)) = (pressed, press)
        && let Some(spec) = zone.spec()
    {
        match app.run("develop.beginInteraction", json!({"label":spec.label})) {
            Ok(_) => {
                let value = value_of(app, id, spec);
                app.ui.dragging_control = Some(spec.id.into());
                app.gesture = Some(Gesture::HistogramDrag { control: spec.id, value });
                state = Some(DragOwner { owner, frame: ui.ctx().cumulative_frame_nr(), previous: press, expected: value, dragging: false });
            }
            Err(error) => app.toast(ui.ctx(), error),
        }
    }
    let mut active = None;
    if let Some(Gesture::HistogramDrag { control, value }) = app.gesture
        && let (Some(spec), Some(state)) = (controls::find(control), state.as_mut())
    {
        state.frame = ui.ctx().cumulative_frame_nr();
        if ui.input(|input| input.pointer.primary_down() || input.pointer.primary_released()) {
            state.dragging |= ui.input(|input| !input.pointer.could_any_button_be_click());
            if state.dragging
                && let Some(point) = ui.input(|input| input.pointer.interact_pos())
            {
                let next = dragged(spec, value, point.x - state.previous.x, plot.width(), ui.input(|input| input.modifiers.shift));
                state.previous = point;
                app.gesture = Some(Gesture::HistogramDrag { control, value: next });
                let value = quantized(spec, next);
                if value != value_of(app, id, spec)
                    && let Err(error) = app.run("develop.set", json!({"control":control,"value":value}))
                {
                    app.toast(ui.ctx(), error);
                }
                state.expected = value_of(app, id, spec);
            }
        }
        active = ZONES.iter().copied().find(|zone| zone.control == control).map(|zone| Active {
            zone,
            value: value_of(app, id, spec),
            dragging: state.dragging,
        });
        if response.drag_stopped_by(egui::PointerButton::Primary) || ui.input(|input| !input.pointer.primary_down() || !input.focused) {
            finish(app, ui);
        }
    }
    let click_id = response.id.with("histogram-click");
    if response.double_clicked_by(egui::PointerButton::Primary)
        && let Some(zone) = pressed
        && let Some(spec) = zone.spec()
    {
        // Restore the clipping state of the first click when the click pair becomes a reset.
        if let Some((previous_owner, control, clipping)) = ui.data(|data| data.get_temp::<(egui::Id, &'static str, bool)>(click_id))
            && previous_owner == owner
            && control == spec.id
        {
            app.ui.show_clipping = clipping;
        }
        if let Err(error) = app.run("develop.beginInteraction", json!({"label":spec.label})) {
            app.toast(ui.ctx(), error);
        } else {
            app.ui.dragging_control = Some(spec.id.into());
            if let Err(error) = app.run("develop.resetControl", json!({"control":spec.id})) {
                app.toast(ui.ctx(), error);
            }
            finish(app, ui);
        }
        ui.data_mut(|data| data.remove::<(egui::Id, &'static str, bool)>(click_id));
    } else if response.clicked() {
        if let Some(zone) = pressed {
            ui.data_mut(|data| data.insert_temp(click_id, (owner, zone.control, app.ui.show_clipping)));
        }
        app.ui.show_clipping = !app.ui.show_clipping;
    }
    ui.data_mut(|data| {
        if matches!(app.gesture, Some(Gesture::HistogramDrag { .. })) {
            if let Some(state) = state {
                data.insert_temp(state_id, state);
            }
        } else {
            data.remove::<DragOwner>(state_id);
        }
    });
    let active =
        active.or_else(|| hovered.and_then(|zone| zone.spec().map(|spec| Active { zone, value: value_of(app, id, spec), dragging: false })))?;
    ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
    register(ui.ctx(), "histogram:zone", active.zone.rect(plot));
    Some(active)
}
fn finish(app: &mut LightcraftApp, ui: &egui::Ui) {
    app.gesture = None;
    app.ui.dragging_control = None;
    if let Err(error) = app.run("develop.endInteraction", json!({})) {
        app.toast(ui.ctx(), error);
    }
}
fn value_of(app: &LightcraftApp, id: PhotoId, spec: &ControlSpec) -> f64 {
    controls::get(&app.session.develop_of(id).unwrap_or_default(), spec.id).unwrap_or(spec.default)
}

#[cfg(test)]
mod tests_drag;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zones_tile_real_controls_and_reject_nonfinite_positions() {
        for zone in ZONES {
            let spec = zone.spec().unwrap();
            assert_eq!(spec.section, lightcraft_develop::Section::Light);
            assert_eq!(zone_at((zone.from + zone.to) / 2.0), Some(zone));
            assert_eq!(zone_at(zone.from), Some(zone));
        }
        assert_eq!(zone_at(1.0), Some(ZONES[4]));
        for x in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, -0.1, 1.1] {
            assert!(zone_at(x).is_none());
        }
        for pair in ZONES.windows(2) {
            assert_eq!(pair[0].to, pair[1].from);
        }
    }

    #[test]
    fn motion_uses_control_ranges_steps_fine_mode_and_fractional_accumulation() {
        for zone in ZONES {
            let spec = zone.spec().unwrap();
            let range = spec.max - spec.min;
            assert!((dragged(spec, 0.0, 20.0, 200.0, false) - range * 0.1).abs() < 1e-9);
            assert!((dragged(spec, 0.0, 20.0, 200.0, true) - range * 0.01).abs() < 1e-9);
            assert_eq!(dragged(spec, 0.0, 400.0, 200.0, false), spec.max);
            assert_eq!(dragged(spec, 0.0, -400.0, 200.0, false), spec.min);
            let accumulated = (0..10).fold(0.0, |value, _| dragged(spec, value, 0.4, 200.0, false));
            assert!((accumulated - range * 0.02).abs() < 1e-6);
            assert!((quantized(spec, accumulated) - accumulated).abs() <= spec.step / 2.0);
            for (dx, width) in [(f32::NAN, 200.0), (20.0, 0.0), (20.0, f32::INFINITY)] {
                assert_eq!(dragged(spec, 5.0, dx, width, false), 5.0);
            }
            assert_eq!(quantized(spec, f64::NAN), spec.default);
        }
    }

    #[test]
    fn readout_uses_the_slider_format_and_exposure_units() {
        assert_eq!(Active { zone: ZONES[2], value: 0.45, dragging: false }.readout().as_deref(), Some("Exposure: +0.45 EV"));
        assert_eq!(Active { zone: ZONES[1], value: -25.0, dragging: true }.readout().as_deref(), Some("Shadows: -25"));
    }
}
