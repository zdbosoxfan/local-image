use super::*;
use crate::{
    Services,
    headless::Headless,
    state::{RightPanel, ViewMode},
};
use std::time::Duration;

const TIMEOUT: Duration = Duration::from_secs(20);

fn setup() -> Headless {
    let mut app = LightcraftApp::new(lightcraft_engine::Session::with_demo(), Services { png: None, ..Default::default() });
    app.ui.settings.gpu = false;
    app.ui.view = ViewMode::Detail;
    app.ui.right = RightPanel::Edit;
    app.ui.histogram = true;
    let mut headless = Headless::new(app, [1200.0, 1400.0], 1.0);
    headless.step();
    headless.step();
    headless
}
fn plot(headless: &Headless) -> Rect {
    let rect = headless.app.widgets.iter().find(|(name, _)| name == "histogram").expect("histogram visible").1;
    li_ui_kit::histogram::library_plot_rect(rect)
}
fn pointer(headless: &mut Headless, point: Pos2, pressed: Option<bool>, shift: bool) {
    let modifiers = egui::Modifiers { shift, ..Default::default() };
    headless.events.push(egui::Event::ModifiersChanged(modifiers));
    headless.events.push(egui::Event::PointerMoved(point));
    if let Some(pressed) = pressed {
        headless.events.push(egui::Event::PointerButton { pos: point, button: egui::PointerButton::Primary, pressed, modifiers });
    }
    headless.step();
}
fn values(headless: &Headless) -> [f64; 5] {
    let id = headless.app.session.active().unwrap();
    ZONES.map(|zone| value_of(&headless.app, id, zone.spec().unwrap()))
}

#[test]
fn all_zones_update_live_stay_locked_and_undo_in_one_step() {
    for (index, zone) in ZONES.into_iter().enumerate() {
        let mut h = setup();
        let plot = plot(&h);
        let start = zone.rect(plot).center();
        let before = h.app.session.undo.len();
        pointer(&mut h, start, Some(true), false);
        assert_eq!(values(&h), [0.0; 5]);
        let end = start + egui::vec2(plot.width() * 0.2, 150.0);
        pointer(&mut h, end, None, false);
        let value = values(&h)[index];
        assert!(value > 0.0, "{}: {value}", zone.control);
        assert_eq!(h.app.session.undo.len(), before, "live draft");
        assert!(h.app.session.interaction.is_some());
        assert_eq!(h.last_cursor, egui::CursorIcon::ResizeHorizontal);
        assert!(crate::tests_develop_c::texts(&h).contains(&Active { zone, value, dragging: true }.readout().unwrap()));
        pointer(&mut h, end, Some(false), false);
        assert_eq!(h.app.session.undo.len(), before + 1);
        assert!(h.app.session.interaction.is_none());
        for (other, value) in values(&h).into_iter().enumerate() {
            if other != index {
                assert_eq!(value, 0.0);
            }
        }
        h.app.run("edit.undo", json!({})).unwrap();
        assert_eq!(values(&h), [0.0; 5]);
    }
}

#[test]
fn fine_histogram_motion_clamps_and_keeps_fractional_travel() {
    for fine in [false, true] {
        let mut h = setup();
        let plot = plot(&h);
        let start = ZONES[2].rect(plot).center();
        pointer(&mut h, start, Some(true), fine);
        let start = start + egui::vec2(0.0, h.view.ctx.options(|options| options.input_options.max_click_dist) + 1.0);
        pointer(&mut h, start, None, fine);
        for index in 1..=20 {
            pointer(&mut h, start + egui::vec2(plot.width() * index as f32 / 1000.0, 0.0), None, fine);
        }
        let expected = if fine { 0.02 } else { 0.2 };
        assert!((values(&h)[2] - expected).abs() < 0.011, "{:?}", values(&h));
        pointer(&mut h, start + egui::vec2(plot.width() * 20.0, 0.0), None, fine);
        assert_eq!(values(&h)[2], 5.0);
        pointer(&mut h, start - egui::vec2(plot.width() * 20.0, 0.0), None, fine);
        assert_eq!(values(&h)[2], -5.0);
        pointer(&mut h, start, Some(false), fine);
    }
}

#[test]
fn histogram_click_wobble_only_toggles_clipping() {
    for batched in [false, true] {
        let mut h = setup();
        let start = ZONES[2].rect(plot(&h)).center();
        let end = start + egui::vec2(2.0, 0.0);
        let before = h.app.session.undo.len();
        if batched {
            h.events.extend([
                egui::Event::PointerMoved(start),
                egui::Event::PointerButton { pos: start, button: egui::PointerButton::Primary, pressed: true, modifiers: egui::Modifiers::NONE },
                egui::Event::PointerMoved(end),
                egui::Event::PointerButton { pos: end, button: egui::PointerButton::Primary, pressed: false, modifiers: egui::Modifiers::NONE },
            ]);
            h.step();
        } else {
            pointer(&mut h, start, Some(true), false);
            pointer(&mut h, end, None, false);
            assert_eq!(values(&h), [0.0; 5], "click motion must not write a live tone draft");
            pointer(&mut h, end, Some(false), false);
        }
        assert_eq!(values(&h), [0.0; 5]);
        assert_eq!(h.app.session.undo.len(), before);
        assert!(h.app.session.interaction.is_none());
        assert!(h.app.ui.dragging_control.is_none());
        assert!(h.app.ui.show_clipping, "one clipping toggle");
        h.step();
        assert!(h.app.ui.show_clipping);
    }
}

#[test]
fn histogram_excludes_metadata_and_indicators_and_keeps_click_and_space() {
    let mut h = setup();
    let plot = plot(&h);
    let histogram = h.app.widgets.iter().find(|(name, _)| name == "histogram").unwrap().1;
    for point in [
        pos2(histogram.center().x, histogram.bottom() - 10.0),
        li_ui_kit::histogram::library_clipping_rects(plot)[0].center(),
        li_ui_kit::histogram::library_clipping_rects(plot)[1].center(),
    ] {
        pointer(&mut h, point, Some(true), false);
        pointer(&mut h, point + egui::vec2(30.0, 0.0), None, false);
        pointer(&mut h, point + egui::vec2(30.0, 0.0), Some(false), false);
        assert_eq!(values(&h), [0.0; 5]);
    }
    let indicator = li_ui_kit::histogram::library_clipping_rects(plot)[0].center();
    pointer(&mut h, indicator, Some(true), false);
    pointer(&mut h, indicator, Some(false), false);
    assert!(h.app.ui.show_clipping);
    for _ in 0..128 {
        let focused = h.view.ctx.memory(|memory| memory.focused());
        if focused.and_then(|id| h.view.ctx.read_response(id)).is_some_and(|response| response.rect == histogram) {
            break;
        }
        let result = h.request("ui.key", json!({"key":"tab"}), TIMEOUT);
        assert_eq!(result["ok"], true);
    }
    let response = h.view.ctx.read_response(h.view.ctx.memory(|memory| memory.focused()).expect("Tab reaches the histogram")).unwrap();
    assert_eq!(response.rect, histogram);
    let result = h.request("ui.key", json!({"key":"space"}), TIMEOUT);
    assert_eq!(result["ok"], true);
    assert!(!h.app.ui.show_clipping, "focused Space still toggles clipping");
}

#[test]
fn histogram_reset_uses_the_photo_default_without_changing_clipping_or_layers() {
    let mut h = setup();
    let id = h.app.session.active().unwrap();
    let mut photo = h.app.session.catalog.photo(id).unwrap().as_ref().clone();
    let mut defaults = lightcraft_develop::DevelopSettings::default();
    defaults.light.exposure = 0.7;
    photo.import_look = Some(std::sync::Arc::new(defaults));
    h.app.session.catalog.apply(lightcraft_catalog::Op::RemovePhoto { id }).unwrap();
    h.app.session.catalog.apply(lightcraft_catalog::Op::AddPhoto { photo: Box::new(photo) }).unwrap();
    h.app.run("mask.add", json!({"kind":"full"})).unwrap();
    let mask = h.app.session.active_mask.unwrap();
    h.app.run("mask.setTools", json!({"id":mask, "values":{"light.exposure":-0.5}})).unwrap();
    h.app.run("develop.set", json!({"control":"light.exposure", "value":2.0})).unwrap();
    h.step();
    let before = h.app.session.undo.len();
    let result = h.request("ui.clickWidget", json!({"id":"histogram", "count":2}), TIMEOUT);
    assert_eq!(result["ok"], true, "{result}");
    assert_eq!(values(&h)[2], 0.7);
    assert!(!h.app.ui.show_clipping);
    assert_eq!(h.app.session.undo.len(), before + 1);
    let settings = h.app.session.develop_of(id).unwrap();
    assert_eq!(settings.masks[0].tools.light.as_ref().unwrap().exposure, -0.5);
    h.app.run("edit.undo", json!({})).unwrap();
    assert_eq!(values(&h)[2], 2.0);
}

#[test]
fn undo_selection_changes_and_closed_histograms_do_not_resume_writes() {
    for action in ["undo", "selection", "closed"] {
        let mut h = setup();
        let start = ZONES[2].rect(plot(&h)).center();
        pointer(&mut h, start, Some(true), false);
        pointer(&mut h, start + egui::vec2(20.0, 0.0), None, false);
        assert!(values(&h)[2] > 0.0);
        match action {
            "undo" => {
                h.app.run("edit.undo", json!({})).unwrap();
            }
            "selection" => {
                h.app.run("library.select", json!({"ids":[2]})).unwrap();
            }
            _ => {
                h.app.ui.histogram = false;
                h.step();
                h.step();
                h.app.ui.histogram = true;
            }
        }
        let after = values(&h);
        pointer(&mut h, start + egui::vec2(40.0, 0.0), None, false);
        pointer(&mut h, start + egui::vec2(40.0, 0.0), Some(false), false);
        assert_eq!(values(&h), after, "{action}");
        assert!(h.app.session.interaction.is_none());
        assert!(h.app.ui.dragging_control.is_none());
    }
}

#[test]
fn batched_histogram_drag_uses_the_original_zone_and_commits_once() {
    let mut h = setup();
    let plot = plot(&h);
    let start = ZONES[2].rect(plot).center();
    let end = start + egui::vec2(plot.width() * 0.3, 0.0);
    let before = h.app.session.undo.len();
    h.events.extend([
        egui::Event::PointerMoved(start),
        egui::Event::PointerButton { pos: start, button: egui::PointerButton::Primary, pressed: true, modifiers: egui::Modifiers::NONE },
        egui::Event::PointerMoved(end),
        egui::Event::PointerButton { pos: end, button: egui::PointerButton::Primary, pressed: false, modifiers: egui::Modifiers::NONE },
    ]);
    h.step();
    assert!((values(&h)[2] - 3.0).abs() < 0.011);
    assert_eq!(values(&h)[3], 0.0);
    assert_eq!(h.app.session.undo.len(), before + 1);
    assert!(h.app.session.interaction.is_none());
}
