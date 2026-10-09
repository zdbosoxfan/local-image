//! local-image: the Pen pressure curve editor in Preferences › Tools: a preset menu (Soft,
//! Linear, Firm, Custom) and a small curve graph (click to add a point, drag to move, drag off or
//! ⌫ to remove), editing the `tools.penPressureCurve` draft value. The curve itself is
//! `photocraft_paint::pressure`; the stylus applies it to every pen sample.

use egui::{Color32, Pos2, Rect, Sense, Stroke, StrokeKind, pos2, vec2};
use photocraft_engine::paint::pressure::{self, PressureCurve};
use serde_json::{Map, Value, json};

use crate::state::PointCurveState;
use crate::theme::Tokens;

/// The curve points stored under `key` (or the linear curve).
fn read(obj: &Map<String, Value>, key: &str) -> Vec<[f32; 2]> {
    obj.get(key).and_then(|v| serde_json::from_value::<Vec<[f32; 2]>>(v.clone()).ok()).map(|p| pressure::sanitize(&p)).unwrap_or_else(|| pressure::LINEAR.to_vec())
}

/// Preset id for the menu: one of [`pressure::PRESETS`], or `"custom"`.
pub fn preset_id(points: &[[f32; 2]]) -> &'static str {
    pressure::preset_of(points).unwrap_or("custom")
}

/// The editor for the draft value `obj[key]`. Returns whether the curve changed.
pub fn editor(ui: &mut egui::Ui, obj: &mut Map<String, Value>, key: &str) -> bool {
    let t = Tokens::get(ui.ctx());
    let mut pts = read(obj, key);
    let before = pts.clone();
    ui.vertical(|ui| {
        let mut cur = preset_id(&pts).to_string();
        let opts = [
            ("soft".to_string(), tl!("Soft")),
            ("linear".to_string(), tl!("Linear")),
            ("firm".to_string(), tl!("Firm")),
            ("custom".to_string(), tl!("Custom")),
        ];
        let prev = cur.clone();
        crate::widgets::dropdown(ui, "pen-pressure-preset", &mut cur, &opts, 140.0);
        if cur != prev
            && let Some((_, p)) = pressure::PRESETS.iter().find(|(id, _)| *id == cur)
        {
            pts = p.to_vec();
        }
        ui.add_space(4.0);
        let side = 150.0;
        let (graph, resp) = ui.allocate_exact_size(vec2(side, side), Sense::click_and_drag());
        let state_id = egui::Id::new("pen-pressure-curve-gesture");
        let mut st: PointCurveState = ui.data(|d| d.get_temp(state_id)).unwrap_or_default();
        // The shared point-curve gestures work in 0..255.
        let mut p255: Vec<[f32; 2]> = pts.iter().map(|q| [q[0] * 255.0, q[1] * 255.0]).collect();
        if crate::point_curve::interact(ui, &resp, graph, &mut p255, &mut st).changed {
            pts = pressure::sanitize(&p255.iter().map(|q| [q[0] / 255.0, q[1] / 255.0]).collect::<Vec<_>>());
        }
        ui.data_mut(|d| d.insert_temp(state_id, st));
        draw(ui, graph, &pts, st.selected, &t, resp.has_focus());
        ui.label(egui::RichText::new(tl!("Input pressure → brush pressure")).color(t.text_faint).size(11.0));
    });
    let changed = pts != before;
    if changed {
        obj.insert(key.to_string(), json!(pts));
    }
    changed
}

fn draw(ui: &egui::Ui, graph: Rect, pts: &[[f32; 2]], selected: Option<usize>, t: &Tokens, focus: bool) {
    let p = ui.painter_at(graph.expand(4.0));
    let to_scr = |q: [f32; 2]| pos2(graph.left() + q[0] * graph.width(), graph.bottom() - q[1] * graph.height());
    p.rect_filled(graph, 0.0, t.field);
    for i in 1..4 {
        let f = i as f32 / 4.0;
        let g = Stroke::new(1.0, t.separator);
        p.line_segment([pos2(graph.left() + f * graph.width(), graph.top()), pos2(graph.left() + f * graph.width(), graph.bottom())], g);
        p.line_segment([pos2(graph.left(), graph.top() + f * graph.height()), pos2(graph.right(), graph.top() + f * graph.height())], g);
    }
    p.line_segment([graph.left_bottom(), graph.right_top()], Stroke::new(1.0, t.separator.gamma_multiply(1.6)));
    p.rect_stroke(graph, 0.0, Stroke::new(if focus { 1.5 } else { 1.0 }, if focus { t.accent } else { t.field_border }), StrokeKind::Outside);
    let curve = PressureCurve::new(pts);
    let line: Vec<Pos2> = (0..=64).map(|i| i as f32 / 64.0).map(|x| to_scr([x, curve.map(x)])).collect();
    p.add(egui::Shape::line(line, Stroke::new(1.5, t.text)));
    for (i, q) in pts.iter().enumerate() {
        let r = Rect::from_center_size(to_scr(*q), vec2(7.0, 7.0));
        if Some(i) == selected {
            p.rect_filled(r, 0.0, t.text);
            p.rect_stroke(r, 0.0, Stroke::new(1.0, Color32::BLACK), StrokeKind::Outside);
        } else {
            p.rect_filled(r, 0.0, t.field);
            p.rect_stroke(r, 0.0, Stroke::new(1.0, t.text), StrokeKind::Inside);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presets_are_recognised_and_bad_values_read_as_linear() {
        let mut obj = Map::new();
        assert_eq!(read(&obj, "penPressureCurve"), pressure::LINEAR.to_vec());
        obj.insert("penPressureCurve".into(), json!("nonsense"));
        assert_eq!(read(&obj, "penPressureCurve"), pressure::LINEAR.to_vec());
        obj.insert("penPressureCurve".into(), json!(pressure::FIRM));
        assert_eq!(preset_id(&read(&obj, "penPressureCurve")), "firm");
        assert_eq!(preset_id(&[[0.0, 0.0], [0.5, 0.9], [1.0, 1.0]]), "custom");
    }

    #[test]
    fn the_editor_draws_without_changing_the_value() {
        let ctx = egui::Context::default();
        let mut obj = Map::new();
        obj.insert("penPressureCurve".into(), json!(pressure::SOFT));
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                assert!(!editor(ui, &mut obj, "penPressureCurve"));
            });
        });
        assert_eq!(read(&obj, "penPressureCurve"), pressure::SOFT.to_vec());
    }
}
