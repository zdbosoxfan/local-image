use egui::{Align2, CursorIcon, Pos2, Response, Ui, vec2};
use lightcraft_develop::{ToneCurve, controls};
use lightcraft_pipeline::curve::{PARAM_AMOUNTS, ParametricCurve};

use super::{SliderOut, primary_press, primary_pressed};
use crate::theme::Tokens;

#[derive(Clone)]
struct Drag {
    owner: egui::Id,
    frame: u64,
    region: usize,
    initial: f64,
    expected: f64,
    per_unit: f64,
    previous: Pos2,
    travel: f32,
    dragging: bool,
}

/// Direct curve manipulation returns the same transaction output as a numeric slider.
pub(crate) fn parametric_drag(
    ui: &Ui,
    response: &Response,
    curve: &ToneCurve,
    owner: egui::Id,
    active: Option<&str>,
) -> Option<(&'static str, SliderOut)> {
    let rect = response.rect;
    if !rect.is_finite() || rect.width() <= 1.0 || rect.height() <= 1.0 {
        return None;
    }
    let model = ParametricCurve::new(curve);
    let tables = lightcraft_pipeline::finish::curve_luts(curve);
    let near = |point: Pos2| {
        let x = f64::from(((point.x - rect.left()) / rect.width()).clamp(0.0, 1.0));
        let level = tables.as_ref().map_or(x as f32, |tables| tables.tables[0].eval(x as f32));
        ((rect.bottom() - level * rect.height()) - point.y).abs() <= 14.0
    };
    let drag_id = response.id.with("parametric-drag");
    let mut drag = ui.data(|data| data.get_temp::<Drag>(drag_id));
    let mut output = None;
    if drag.as_ref().is_some_and(|drag| {
        drag.frame.saturating_add(1) < ui.ctx().cumulative_frame_nr()
            || drag.owner != owner
            || active != Some(PARAM_AMOUNTS[drag.region])
            || model.amounts[drag.region] != drag.expected
    }) {
        if let Some(previous) = drag.take() {
            output = Some((PARAM_AMOUNTS[previous.region], SliderOut { drag_stopped: true, ..Default::default() }));
        }
    } else if primary_pressed(ui, response)
        && let Some(press) = primary_press(ui, response)
        && rect.contains(press)
        && near(press)
        && active.is_none()
    {
        let x = f64::from(((press.x - rect.left()) / rect.width()).clamp(0.0, 1.0));
        let region = model.region_at(x);
        let master = li_color::spline::MonotoneCurve::new(&curve.master.iter().map(|p| (p.x, p.y)).collect::<Vec<_>>()).to_lut(1024);
        let level = model.at(x);
        let lo = (level - 1.0 / 1023.0).max(0.0);
        let hi = (level + 1.0 / 1023.0).min(1.0);
        let slope = f64::from(master.eval(hi as f32) - master.eval(lo as f32)) / (hi - lo);
        let initial = model.amounts[region];
        drag = Some(Drag {
            owner,
            frame: ui.ctx().cumulative_frame_nr(),
            region,
            initial,
            expected: initial,
            per_unit: model.level_per_unit(region, x) * slope,
            previous: press,
            travel: 0.0,
            dragging: false,
        });
        output = Some((PARAM_AMOUNTS[region], SliderOut { drag_started: true, ..Default::default() }));
    }
    if let Some(drag) = drag.as_mut() {
        drag.frame = ui.ctx().cumulative_frame_nr();
        let control = PARAM_AMOUNTS[drag.region];
        let mut out = output.map_or_else(SliderOut::default, |(_, out)| out);
        drag.dragging |= ui.input(|input| !input.pointer.could_any_button_be_click());
        if drag.dragging
            && ui.input(|input| input.pointer.primary_down() || input.pointer.primary_released())
            && let Some(point) = ui.input(|input| input.pointer.interact_pos())
        {
            let motion = drag.previous.y - point.y;
            drag.travel += motion * if ui.input(|input| input.modifiers.alt) { 0.1 } else { 1.0 };
            drag.previous = point;
            let amount = if drag.per_unit.is_finite() && drag.per_unit.abs() > f64::EPSILON {
                (drag.initial + f64::from(drag.travel / rect.height()) / drag.per_unit).clamp(-100.0, 100.0).round()
            } else {
                drag.initial
            };
            if amount != model.amounts[drag.region] {
                out.value = Some(amount);
            }
            drag.expected = amount;
        }
        out.drag_stopped =
            response.drag_stopped_by(egui::PointerButton::Primary) || ui.input(|input| !input.pointer.primary_down() || !input.focused);
        output = Some((control, out));
    }
    let shown = drag.as_ref().map(|drag| (drag.region, drag.expected)).or_else(|| {
        response.hover_pos().filter(|point| near(*point)).map(|point| {
            let region = model.region_at(f64::from((point.x - rect.left()) / rect.width()));
            (region, model.amounts[region])
        })
    });
    if let Some((region, value)) = shown {
        ui.ctx().set_cursor_icon(if drag.is_some() { CursorIcon::Grabbing } else { CursorIcon::Grab });
        if let Some(spec) = controls::find(PARAM_AMOUNTS[region]) {
            let tokens = Tokens::get(ui.ctx());
            ui.painter().text(
                rect.left_top() + vec2(6.0, 4.0),
                Align2::LEFT_TOP,
                format!("{} {value:.0}", li_i18n::library::t(spec.label)),
                tokens.font(11.0),
                tokens.text_dim,
            );
        }
    }
    ui.data_mut(|data| {
        if output.is_some_and(|(_, out)| out.drag_stopped) {
            data.remove::<Drag>(drag_id);
        } else if let Some(drag) = drag {
            data.insert_temp(drag_id, drag);
        }
    });
    output
}
