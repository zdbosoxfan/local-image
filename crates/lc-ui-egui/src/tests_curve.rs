//! Headless tests of the point-curve editor: dragging points, resetting, presets.

use std::time::Duration;

use serde_json::json;

use crate::headless::Headless;
use crate::{LightcraftApp, Services};

const T: Duration = Duration::from_secs(20);

/// Detail view with the Edit panel's Curve flyout open on `channel`.
fn curve_open(channel: &str) -> Headless {
    let services = Services { png: None, ..Default::default() };
    let app = LightcraftApp::new(lightcraft_engine::Session::with_demo(), services);
    let mut h = Headless::new(app, [1200.0, 1400.0], 1.0);
    let r = h.request("ui.set", json!({"view": "detail"}), T);
    assert_eq!(r["ok"], true, "{r}");
    h.app.ui.right = crate::state::RightPanel::Edit;
    if !h.app.ui.section_open("light") {
        h.app.ui.toggle_section("light");
    }
    if !h.app.ui.flyout_open("curve") {
        h.app.ui.toggle_flyout("curve");
    }
    h.app.ui.curve_channel = channel.into();
    h.step();
    h.step();
    h
}

fn exec(h: &mut Headless, command: &str, params: serde_json::Value) -> serde_json::Value {
    let r = h.request("engine.execute", json!({"command": command, "params": params}), T);
    assert_eq!(r["ok"], true, "{command}: {r}");
    r["result"].clone()
}

fn develop(h: &Headless) -> lightcraft_develop::DevelopSettings {
    let id = h.app.session.active().expect("active photo");
    (*h.app.session.develop_of(id).unwrap_or_default()).clone()
}

fn pts(v: &[lightcraft_geom::Point]) -> Vec<(f64, f64)> {
    v.iter().map(|p| (p.x, p.y)).collect()
}

/// Side of the (square) curve graph in screen points.
fn curve_side(h: &mut Headless) -> f64 {
    let r = h.request("ui.widgets", json!({"filter": "curve"}), T);
    let w = r["result"].as_array().unwrap().iter().find(|w| w["id"] == "curve").cloned().unwrap_or_else(|| panic!("curve widget on screen: {r}"));
    w["rect"][2].as_f64().unwrap()
}

#[test]
fn dragging_a_point_moves_it_in_both_axes_with_one_undo_step() {
    let mut h = curve_open("master");
    exec(&mut h, "develop.curve", json!({"channel": "master", "points": [[0.0, 0.0], [0.5, 0.5], [1.0, 1.0]]}));
    h.step();
    let side = curve_side(&mut h);
    let undo_before = h.app.session.undo.len();
    // grab the middle point and drag it up-left: x 0.5 → 0.4, y 0.5 → 0.7
    let r = h.request("ui.dragWidget", json!({"id": "curve", "fx": 0.5, "fy": 0.5, "dx": -0.1 * side, "dy": -0.2 * side, "steps": 12}), T);
    assert_eq!(r["ok"], true, "{r}");
    h.step();
    let c = pts(&develop(&h).curve.master);
    assert_eq!(c.len(), 3, "dragging moves the point, it doesn't add one: {c:?}");
    assert!((c[1].0 - 0.4).abs() < 0.03, "x follows the pointer: {c:?}");
    assert!((c[1].1 - 0.7).abs() < 0.03, "y follows the pointer: {c:?}");
    assert_eq!(h.app.session.undo.len(), undo_before + 1, "one undo step per drag");
}

fn click_widget(h: &mut Headless, id: &str, count: u64) {
    let r = h.request("ui.clickWidget", json!({"id": id, "count": count}), T);
    assert_eq!(r["ok"], true, "{id}: {r}");
    h.step();
}

const S_CURVE: [[f64; 2]; 4] = [[0.0, 0.0], [0.25, 0.15], [0.75, 0.85], [1.0, 1.0]];

#[test]
fn double_clicking_a_channel_resets_only_that_channel() {
    let mut h = curve_open("master");
    exec(&mut h, "develop.curve", json!({"channel": "master", "points": S_CURVE}));
    exec(&mut h, "develop.curve", json!({"channel": "green", "points": S_CURVE}));
    h.step();
    click_widget(&mut h, "curveChannel:green", 2);
    let c = develop(&h).curve;
    assert!(c.green.is_empty(), "green resets: {:?}", c.green);
    assert_eq!(c.master.len(), 4, "the other channels stay");
    assert_eq!(h.app.ui.curve_channel, "green");
}

#[test]
fn reset_button_resets_every_curve() {
    let mut h = curve_open("red");
    for ch in ["master", "red", "blue"] {
        exec(&mut h, "develop.curve", json!({"channel": ch, "points": S_CURVE}));
    }
    exec(&mut h, "develop.set", json!({"control": "curve.highlights", "value": -40}));
    h.step();
    click_widget(&mut h, "button:curveReset", 1);
    let c = develop(&h).curve;
    assert_eq!(c, lightcraft_develop::ToneCurve::default());
}

#[test]
fn point_curve_preset_dropdown_applies_and_saves() {
    let mut h = curve_open("master");
    click_widget(&mut h, "dropdown:curvePreset", 1);
    click_widget(&mut h, "curvePreset:Medium Contrast", 1);
    let c = develop(&h).curve;
    assert_eq!(c.master.len(), 5, "the preset's S-curve: {:?}", c.master);
    assert!(c.master[1].y < 0.25 && c.master[3].y > 0.75);
    // shape it further, then save it from the dropdown (name prompt)
    exec(&mut h, "develop.curve", json!({"channel": "blue", "points": [[0.0, 0.08], [1.0, 1.0]]}));
    h.step();
    click_widget(&mut h, "dropdown:curvePreset", 1);
    click_widget(&mut h, "curvePresetMenu:save", 1);
    let mut dlg = h.app.ui.dialog.take().expect("a name prompt opens");
    match &mut dlg {
        crate::state::Dialog::TextPrompt { command, value, .. } => {
            assert_eq!(command, "curve.savePreset");
            *value = "Cool Shadows".into();
        }
        _ => panic!("a name prompt opens"),
    }
    let r = crate::panels::dialogs::confirm_dialog(&mut h.app, &dlg);
    assert!(r.is_ok(), "{r:?}");
    assert_eq!(h.app.session.curve_presets.len(), 1);
    // Linear from the dropdown flattens every channel
    click_widget(&mut h, "dropdown:curvePreset", 1);
    click_widget(&mut h, "curvePreset:Linear", 1);
    let c = develop(&h).curve;
    assert!(c.master.is_empty() && c.blue.is_empty());
    click_widget(&mut h, "dropdown:curvePreset", 1);
    click_widget(&mut h, "curvePreset:Cool Shadows", 1);
    assert_eq!(develop(&h).curve.blue.len(), 2);
}

#[test]
fn dragging_a_point_past_its_neighbour_is_clamped() {
    let mut h = curve_open("red");
    exec(&mut h, "develop.curve", json!({"channel": "red", "points": [[0.0, 0.0], [0.3, 0.3], [0.6, 0.6], [1.0, 1.0]]}));
    h.step();
    let side = curve_side(&mut h);
    // drag the point at 0.3 far to the right (past 0.6)
    let r = h.request("ui.dragWidget", json!({"id": "curve", "fx": 0.3, "fy": 0.7, "dx": 0.5 * side, "dy": 0.0, "steps": 12}), T);
    assert_eq!(r["ok"], true, "{r}");
    h.step();
    let c = pts(&develop(&h).curve.red);
    assert_eq!(c.len(), 4, "{c:?}");
    assert!(c[1].0 < c[2].0 && c[1].0 > 0.55, "clamped just left of its neighbour: {c:?}");
    assert_eq!(c[2], (0.6, 0.6), "the neighbour stays: {c:?}");
}

/// Dragging a point whose neighbours are closer than the editor's 0.02 spacing (two clicks side
/// by side, or a curve from a sidecar or an agent) clamped it between bounds that had crossed,
/// which panics: the app went down mid-drag. The point now stays between its neighbours.
#[test]
fn dragging_a_point_between_close_neighbours_does_not_panic() {
    let mut h = curve_open("master");
    let close = [[0.0, 0.0], [0.5, 0.5], [0.508, 0.6], [0.515, 0.3], [1.0, 1.0]];
    exec(&mut h, "develop.curve", json!({"channel": "master", "points": close}));
    h.step();
    let side = curve_side(&mut h);
    let r = h.request("ui.dragWidget", json!({"id": "curve", "fx": 0.508, "fy": 0.4, "dx": 0.05 * side, "dy": -0.1 * side, "steps": 8}), T);
    assert_eq!(r["ok"], true, "{r}");
    h.step();
    let c = pts(&develop(&h).curve.master);
    assert_eq!(c.len(), 5, "{c:?}");
    assert!(c.windows(2).all(|w| w[0].0 <= w[1].0), "the points stay in order: {c:?}");
    assert!(c[2].0 > 0.5 && c[2].0 < 0.515, "the dragged point stays between its neighbours: {c:?}");
}

/// Points an agent sends out of order are stored in the order the curve runs, so the editor's
/// neighbours are the curve's.
#[test]
fn curve_points_from_an_agent_are_stored_in_order() {
    let mut h = curve_open("green");
    exec(&mut h, "develop.curve", json!({"channel": "green", "points": [[0.0, 0.0], [0.7, 0.8], [0.3, 0.2], [1.0, 1.0]]}));
    h.step();
    assert_eq!(pts(&develop(&h).curve.green), vec![(0.0, 0.0), (0.3, 0.2), (0.7, 0.8), (1.0, 1.0)]);
    let side = curve_side(&mut h);
    let r = h.request("ui.dragWidget", json!({"id": "curve", "fx": 0.3, "fy": 0.8, "dx": 0.6 * side, "dy": 0.0, "steps": 8}), T);
    assert_eq!(r["ok"], true, "{r}");
    h.step();
    let c = pts(&develop(&h).curve.green);
    assert!(c[1].0 < c[2].0, "dragged up to its neighbour, not past it: {c:?}");
}

#[test]
fn dragging_a_split_with_reversed_neighbours_keeps_finite_ordered_bounds() {
    let mut h = curve_open("parametric");
    let id = h.app.session.active().unwrap();
    let mut settings = develop(&h);
    settings.curve.split_shadows = 80.0;
    settings.curve.split_mid = 20.0;
    settings.curve.split_highlights = 30.0;
    h.app
        .session
        .catalog
        .apply(lightcraft_catalog::Op::SetDevelop { id, settings: std::sync::Arc::new(settings), label: "Curve".into(), edited: None })
        .unwrap();
    h.step();
    let r = h.request("ui.dragWidget", json!({"id":"curveSplit:1", "dx":10.0, "steps":4}), T);
    assert_eq!(r["ok"], true, "{r}");
    let curve = develop(&h).curve;
    assert!(curve.split_mid.is_finite());
    assert!(curve.split_mid >= curve.split_shadows && curve.split_mid <= curve.split_highlights);
}

fn graph(h: &Headless) -> egui::Rect {
    h.app.widgets.iter().rev().find(|(name, _)| name == "curve").expect("curve is visible").1
}

fn graph_pointer(h: &mut Headless, point: egui::Pos2, pressed: Option<bool>, modifiers: egui::Modifiers) {
    h.events.push(egui::Event::ModifiersChanged(modifiers));
    h.events.push(egui::Event::PointerMoved(point));
    if let Some(pressed) = pressed {
        h.events.push(egui::Event::PointerButton { pos: point, button: egui::PointerButton::Primary, pressed, modifiers });
    }
    h.step();
}

fn painted_level(h: &Headless, x: f64) -> f64 {
    let rect = graph(h);
    let point = h
        .view
        .shapes()
        .iter()
        .find_map(|shape| match &shape.shape {
            egui::Shape::Path(path) if path.points.len() == 257 && (path.stroke.width - 2.75).abs() < 0.01 => path
                .points
                .iter()
                .min_by(|a, b| (a.x - rect.left() - x as f32 * rect.width()).abs().total_cmp(&(b.x - rect.left() - x as f32 * rect.width()).abs())),
            _ => None,
        })
        .expect("painted parametric curve");
    f64::from((rect.bottom() - point.y) / rect.height())
}

#[test]
fn parametric_drags_lock_the_pressed_region_and_commit_one_undo() {
    for (region, x) in [0.125, 0.375, 0.625, 0.875].into_iter().enumerate() {
        for direction in [-1.0, 1.0] {
            let mut h = curve_open("parametric");
            let rect = graph(&h);
            let start = egui::pos2(rect.left() + x * rect.width(), rect.bottom() - x * rect.height());
            let before = h.app.session.undo.len();
            graph_pointer(&mut h, start, None, egui::Modifiers::NONE);
            graph_pointer(&mut h, start, Some(true), egui::Modifiers::NONE);
            assert_eq!(develop(&h).curve, lightcraft_develop::ToneCurve::default(), "the press does not jump");
            let end = start + egui::vec2(0.2 * rect.width(), direction * 0.025 * rect.height());
            graph_pointer(&mut h, end, None, egui::Modifiers::NONE);
            let model = lightcraft_pipeline::curve::ParametricCurve::new(&develop(&h).curve);
            for (index, amount) in model.amounts.into_iter().enumerate() {
                if index == region {
                    assert!(amount * f64::from(direction) < 0.0, "region {region}: {amount}");
                } else {
                    assert_eq!(amount, 0.0);
                }
            }
            assert_eq!(h.app.session.undo.len(), before, "live changes remain one draft");
            let label = lightcraft_develop::controls::find(lightcraft_pipeline::curve::PARAM_AMOUNTS[region]).unwrap().label;
            assert!(super::tests_develop_c::texts(&h).contains(&format!("{label} {:.0}", model.amounts[region])));
            let end = start + egui::vec2(0.2 * rect.width(), direction * 2.0 * rect.height());
            graph_pointer(&mut h, end, None, egui::Modifiers::NONE);
            assert_eq!(lightcraft_pipeline::curve::ParametricCurve::new(&develop(&h).curve).amounts[region], -100.0 * f64::from(direction));
            graph_pointer(&mut h, end, Some(false), egui::Modifiers::NONE);
            assert_eq!(h.app.session.undo.len(), before + 1);
            exec(&mut h, "edit.undo", json!({}));
            h.step();
            assert_eq!(develop(&h).curve, lightcraft_develop::ToneCurve::default());
            assert!((painted_level(&h, f64::from(x)) - f64::from(x)).abs() < 0.005);
        }
    }
}

#[test]
fn horizontal_motion_and_off_curve_presses_do_not_change_the_curve() {
    let mut h = curve_open("parametric");
    exec(&mut h, "develop.set", json!({"control":"curve.darks", "value":60}));
    h.step();
    let before = develop(&h).curve;
    let undo = h.app.session.undo.len();
    let level = lightcraft_pipeline::finish::curve_luts(&before).unwrap().tables[0].eval(0.51);
    let rect = graph(&h);
    let start = egui::pos2(rect.left() + 0.51 * rect.width(), rect.bottom() - level * rect.height());
    graph_pointer(&mut h, start, Some(true), egui::Modifiers::NONE);
    let end = start + egui::vec2(45.0, 0.0);
    graph_pointer(&mut h, end, None, egui::Modifiers::NONE);
    graph_pointer(&mut h, end, Some(false), egui::Modifiers::NONE);
    assert_eq!(develop(&h).curve, before);
    assert_eq!(h.app.session.undo.len(), undo);
    let far = rect.left_top() + egui::vec2(20.0, 20.0);
    graph_pointer(&mut h, far, None, egui::Modifiers::NONE);
    assert_ne!(h.last_cursor, egui::CursorIcon::Grab);
    graph_pointer(&mut h, far, Some(true), egui::Modifiers::NONE);
    graph_pointer(&mut h, far + egui::vec2(0.0, 30.0), None, egui::Modifiers::NONE);
    graph_pointer(&mut h, far, Some(false), egui::Modifiers::NONE);
    assert_eq!(develop(&h).curve, before);
    assert_eq!(h.app.session.undo.len(), undo);
}

#[test]
fn composed_master_drag_tracks_its_slope_and_flat_segments_do_not_jump() {
    for points in [vec![[0.0, 0.0], [1.0, 0.5]], vec![[0.0, 0.4], [1.0, 0.4]]] {
        let mut h = curve_open("parametric");
        exec(&mut h, "develop.curve", json!({"channel":"master", "points":points}));
        h.step();
        let before = develop(&h).curve;
        let rect = graph(&h);
        let level = lightcraft_pipeline::finish::curve_luts(&before).unwrap().tables[0].eval(0.625);
        assert!((painted_level(&h, 0.625) - f64::from(level)).abs() < 0.005);
        let start = egui::pos2(rect.left() + 0.625 * rect.width(), rect.bottom() - level * rect.height());
        graph_pointer(&mut h, start, Some(true), egui::Modifiers::NONE);
        let end = start - egui::vec2(0.0, rect.height() * 0.02);
        graph_pointer(&mut h, end, None, egui::Modifiers::NONE);
        graph_pointer(&mut h, end, Some(false), egui::Modifiers::NONE);
        h.step();
        if points[0][1] == points[1][1] {
            assert_eq!(develop(&h).curve, before);
        } else {
            assert!((painted_level(&h, 0.625) - f64::from(level) - 0.02).abs() < 0.004);
        }
    }
}

#[test]
fn alt_scales_curve_motion_and_undo_cancels_a_held_drag() {
    let drag = |fine: bool| {
        let mut h = curve_open("parametric");
        let rect = graph(&h);
        let start = egui::pos2(rect.left() + 0.625 * rect.width(), rect.bottom() - 0.625 * rect.height());
        let modifiers = egui::Modifiers { alt: fine, ..Default::default() };
        graph_pointer(&mut h, start, Some(true), modifiers);
        graph_pointer(&mut h, start - egui::vec2(0.0, 10.0), None, modifiers);
        let amount = develop(&h).curve.lights;
        h.app.run("edit.undo", json!({})).unwrap();
        graph_pointer(&mut h, start - egui::vec2(0.0, 20.0), None, modifiers);
        graph_pointer(&mut h, start - egui::vec2(0.0, 20.0), Some(false), modifiers);
        assert_eq!(develop(&h).curve.lights, 0.0, "cancelled pointer cannot reapply its amount");
        amount
    };
    let normal = drag(false);
    let fine = drag(true);
    assert!((fine - normal * 0.1).abs() <= 1.0, "{normal}, {fine}");
}

#[test]
fn parametric_layer_drags_never_write_the_background_after_target_changes() {
    let mut h = curve_open("parametric");
    h.app.run("mask.add", json!({"kind":"full"})).unwrap();
    h.app.ui.layer_used_only = false;
    h.app.ui.open_sections = vec!["layer-light".into()];
    h.app.ui.open_flyouts = vec!["layer-curve".into()];
    h.step();
    h.step();
    let rect = graph(&h);
    let start = egui::pos2(rect.left() + 0.625 * rect.width(), rect.bottom() - 0.625 * rect.height());
    graph_pointer(&mut h, start, Some(true), egui::Modifiers::NONE);
    graph_pointer(&mut h, start - egui::vec2(0.0, 10.0), None, egui::Modifiers::NONE);
    let edited = develop(&h);
    assert_eq!(edited.curve.lights, 0.0);
    let amount = edited.masks[0].tools.curve.as_ref().unwrap().lights;
    assert!(amount > 0.0);
    h.app.run("mask.select", json!({"id":null})).unwrap();
    h.app.ui.open_sections = vec!["light".into()];
    h.app.ui.open_flyouts = vec!["curve".into()];
    graph_pointer(&mut h, start - egui::vec2(0.0, 20.0), None, egui::Modifiers::NONE);
    graph_pointer(&mut h, start - egui::vec2(0.0, 20.0), Some(false), egui::Modifiers::NONE);
    assert_eq!(develop(&h).curve.lights, 0.0);
    assert_eq!(develop(&h).masks[0].tools.curve.as_ref().unwrap().lights, amount);
    assert!(h.app.session.interaction.is_none());
}

#[test]
fn batched_primary_curve_drag_uses_the_original_press_and_commits_once() {
    let mut h = curve_open("parametric");
    let rect = graph(&h);
    let start = egui::pos2(rect.left() + 0.625 * rect.width(), rect.bottom() - 0.625 * rect.height());
    let end = start + egui::vec2(-rect.width() * 0.3, -10.0);
    let before = h.app.session.undo.len();
    h.events.extend([
        egui::Event::PointerMoved(start),
        egui::Event::PointerButton { pos: start, button: egui::PointerButton::Primary, pressed: true, modifiers: egui::Modifiers::NONE },
        egui::Event::PointerMoved(end),
        egui::Event::PointerButton { pos: end, button: egui::PointerButton::Primary, pressed: false, modifiers: egui::Modifiers::NONE },
    ]);
    h.step();
    assert!(develop(&h).curve.lights > 0.0);
    assert_eq!(develop(&h).curve.darks, 0.0);
    assert_eq!(h.app.session.undo.len(), before + 1);
    assert!(h.app.session.interaction.is_none());
}

#[test]
fn clicking_the_parametric_line_with_a_wobble_leaves_settings_and_undo_unchanged() {
    for batched in [false, true] {
        let mut h = curve_open("parametric");
        let rect = graph(&h);
        let start = egui::pos2(rect.left() + 0.625 * rect.width(), rect.bottom() - 0.625 * rect.height());
        let end = start - egui::vec2(0.0, 2.0);
        assert!(start.distance(end) < h.view.ctx.options(|options| options.input_options.max_click_dist));
        let before = develop(&h);
        let undo = h.app.session.undo.len();
        if batched {
            h.events.extend([
                egui::Event::PointerMoved(start),
                egui::Event::PointerButton { pos: start, button: egui::PointerButton::Primary, pressed: true, modifiers: egui::Modifiers::NONE },
                egui::Event::PointerMoved(end),
                egui::Event::PointerButton { pos: end, button: egui::PointerButton::Primary, pressed: false, modifiers: egui::Modifiers::NONE },
            ]);
            h.step();
        } else {
            graph_pointer(&mut h, start, Some(true), egui::Modifiers::NONE);
            graph_pointer(&mut h, end, None, egui::Modifiers::NONE);
            assert_eq!(develop(&h), before, "a click wobble must not write live settings");
            graph_pointer(&mut h, end, Some(false), egui::Modifiers::NONE);
        }
        assert_eq!(develop(&h), before);
        assert_eq!(h.app.session.undo.len(), undo);
        assert!(h.app.session.interaction.is_none());
        assert!(h.app.ui.dragging_control.is_none());
    }
}

#[test]
fn dragging_a_region_notch_preserves_its_neighbours_and_undo_label() {
    for index in 0..3 {
        let mut h = curve_open("parametric");
        exec(&mut h, "develop.merge", json!({"settings":{"curve":{"split_shadows":23.0,"split_mid":37.0,"split_highlights":79.0}}}));
        h.step();
        let before = develop(&h).curve;
        let splits = [before.split_shadows, before.split_mid, before.split_highlights];
        let undo = h.app.session.undo.len();
        let r = h.request("ui.dragWidget", json!({"id":format!("curveSplit:{index}"),"dx":10.0,"steps":4}), T);
        assert_eq!(r["ok"], true, "{r}");
        let after = develop(&h).curve;
        let updated = [after.split_shadows, after.split_mid, after.split_highlights];
        for neighbour in 0..3 {
            if neighbour == index {
                assert!(updated[neighbour] > splits[neighbour], "the dragged notch moves");
            } else {
                assert_eq!(updated[neighbour], splits[neighbour], "neighbour {neighbour} of notch {index}");
            }
        }
        assert_eq!(h.app.session.undo.len(), undo + 1);
        assert_eq!(h.app.session.undo.last().unwrap().label, "Curve Regions");
        exec(&mut h, "edit.undo", json!({}));
        assert_eq!(develop(&h).curve, before);
    }
}
