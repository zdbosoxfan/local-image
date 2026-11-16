//! UI tests for the adjustment editors: the whole app (eframe harness, CPU canvas) with an
//! adjustment layer on a new document, edited through the Properties panel with real pointer and
//! key input (issue #12: "add an adjustment layer to a new document, edit it, crash"), and the
//! Curves point interactions (issue #43).

use egui::{Pos2, Rect, vec2};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use photocraft_doc::{Adjustment, LayerContent, LayerId};
use serde_json::json;

use crate::PhotocraftApp;

const ALL_KINDS: [&str; 16] = [
    "brightnessContrast",
    "levels",
    "curves",
    "exposure",
    "vibrance",
    "hueSaturation",
    "colorBalance",
    "blackWhite",
    "photoFilter",
    "channelMixer",
    "invert",
    "posterize",
    "threshold",
    "gradientMap",
    "selectiveColor",
    "colorLookup",
];

fn app_harness(kind: &str, mode: &str, depth: u32) -> Harness<'static, PhotocraftApp> {
    let (kind, mode) = (kind.to_string(), mode.to_string());
    let mut h = Harness::builder().with_size(vec2(1440.0, 900.0)).with_max_steps(64).build_eframe(move |cc| {
        PhotocraftApp::setup_context(&cc.egui_ctx, Default::default());
        let mut s = photocraft_engine::Session::new();
        s.execute("file.new", json!({"width": 96, "height": 64, "mode": mode, "depth": depth})).unwrap();
        // Two tones so histograms (and Levels' Auto) have something to work with.
        s.execute("select.rect", json!({"x": 0, "y": 0, "width": 48, "height": 64})).unwrap();
        s.execute("edit.fill", json!({"color": "#6a3020"})).unwrap();
        s.execute("select.deselect", json!({})).unwrap();
        s.execute(&format!("layer.newAdjustmentLayer.{kind}"), json!({})).unwrap();
        let mut app = PhotocraftApp::new(s, crate::Services::default());
        // The default Properties group gives way to Layers (#147) and scrolls taller editors;
        // these tests drive every control without scrolling, so they size it like a user would.
        app.ui.dock.heights.insert(crate::dock::Group::Properties, 560.0);
        app
    });
    h.run_steps(8);
    h
}

fn layer(h: &Harness<'_, PhotocraftApp>) -> (LayerId, Adjustment) {
    let st = h.state().session.active().unwrap();
    let id = st.active_layer.unwrap();
    match &st.doc.layer(id).unwrap().content {
        LayerContent::Adjustment(a) => (id, a.clone()),
        other => panic!("{other:?}"),
    }
}

fn drag(h: &mut Harness<'_, PhotocraftApp>, from: Pos2, to: Pos2) {
    h.hover_at(from);
    h.run_steps(1);
    h.drag_at(from);
    h.run_steps(1);
    for i in 1..=6 {
        h.hover_at(from + (to - from) * (i as f32 / 6.0));
        h.run_steps(1);
    }
    h.drop_at(to);
    h.run_steps(3);
}

fn click(h: &mut Harness<'_, PhotocraftApp>, at: Pos2) {
    h.hover_at(at);
    h.run_steps(1);
    h.drag_at(at);
    h.run_steps(1);
    h.drop_at(at);
    h.run_steps(3);
}

fn label_rect(h: &Harness<'_, PhotocraftApp>, text: &str) -> Option<Rect> {
    // The last match: the Properties header repeats the kind's name above its controls.
    h.query_all_by_label(text).last().map(|n| n.rect())
}

fn curves_graph(h: &Harness<'_, PhotocraftApp>, id: LayerId) -> Rect {
    h.ctx.data(|d| d.get_temp::<Rect>(egui::Id::new(("adjust-layer", id.0)).with("curves-graph"))).expect("curves graph drawn")
}

/// Issue #12: every adjustment kind, on a new document at 8/16/32 bits and in RGB, Grayscale,
/// CMYK and Lab, opens its Properties editor and takes an edit through real input without
/// panicking; slider kinds commit exactly one history step per gesture.
#[test]
fn every_adjustment_layer_edits_through_properties() {
    for (mode, depth) in [("rgb", 8), ("rgb", 16), ("rgb", 32), ("gray", 8), ("cmyk", 8), ("lab", 16)] {
        for kind in ALL_KINDS {
            let mut h = app_harness(kind, mode, depth);
            let (id, before) = layer(&h);
            let steps = h.state().session.active().unwrap().history.past_len();
            // A probe label per editor; the control sits just below it.
            let probe = match kind {
                "brightnessContrast" => Some("Brightness"),
                "exposure" => Some("Exposure"),
                "vibrance" => Some("Vibrance"),
                "hueSaturation" => Some("Hue"),
                "colorBalance" => Some("Cyan  ·  Red"),
                "blackWhite" => Some("Reds"),
                "photoFilter" => Some("Density"),
                "channelMixer" => Some("Constant"),
                "posterize" => Some("Levels"),
                "threshold" => Some("Threshold Level"),
                "selectiveColor" => Some("Cyan"),
                _ => None,
            };
            match kind {
                "curves" => {
                    let g = curves_graph(&h, id);
                    drag(&mut h, g.center(), g.center() + vec2(0.0, -40.0));
                }
                "levels" => {
                    // The output black handle sits below the output gradient bar.
                    let r = label_rect(&h, "Output Levels:").unwrap_or_else(|| panic!("{kind} {mode}: Output Levels"));
                    let y = r.bottom() + 17.0;
                    drag(&mut h, Pos2::new(r.left() + 6.0, y), Pos2::new(r.left() + 60.0, y));
                }
                "gradientMap" => {
                    let r = label_rect(&h, "Reverse").unwrap_or_else(|| panic!("{kind} {mode}: Reverse"));
                    click(&mut h, r.center());
                }
                _ => {
                    if let Some(p) = probe {
                        let r = label_rect(&h, p).unwrap_or_else(|| panic!("{kind} {mode}: no `{p}` label"));
                        let y = r.bottom() + 14.0;
                        drag(&mut h, Pos2::new(r.left() + 40.0, y), Pos2::new(r.left() + 120.0, y));
                    }
                }
            }
            h.run_steps(4);
            let (_, after) = layer(&h);
            assert!(h.state().live_adjust.is_none(), "{kind} {mode}: preview ended with the gesture");
            if !matches!(kind, "invert" | "colorLookup") {
                assert_ne!(after, before, "{kind} {mode}@{depth}: the edit reached the layer");
                assert_eq!(h.state().session.active().unwrap().history.past_len(), steps + 1, "{kind} {mode}: one history step");
            }
        }
    }
}

fn curve_of(a: &Adjustment) -> Vec<[f32; 2]> {
    match a {
        Adjustment::Curves { master, .. } => master.iter().map(|p| [(p.input * 255.0).round(), (p.output * 255.0).round()]).collect(),
        other => panic!("{other:?}"),
    }
}

/// Issue #43: pressing on a point selects it (no new point), Delete/Backspace and ⌘/Ctrl-click
/// remove points, dragging off the graph removes one, endpoints stay.
#[test]
fn curves_points_are_added_selected_and_deleted() {
    let mut h = app_harness("curves", "rgb", 8);
    let (id, _) = layer(&h);
    let g = curves_graph(&h, id);
    let at = |v: [f32; 2]| Pos2::new(g.left() + v[0] / 255.0 * g.width(), g.bottom() - v[1] / 255.0 * g.height());
    // Add a point.
    click(&mut h, at([128.0, 128.0]));
    assert_eq!(curve_of(&layer(&h).1).len(), 3);
    // Press 4 px off it and drag: moves that point, adds none.
    drag(&mut h, at([128.0, 128.0]) + vec2(4.0, 3.0), at([128.0, 180.0]));
    let c = curve_of(&layer(&h).1);
    assert_eq!(c.len(), 3, "{c:?}");
    assert!(c[1][1] > 165.0, "{c:?}");
    // Delete key removes the selected point (the graph has focus).
    h.key_press(egui::Key::Delete);
    h.run_steps(3);
    assert_eq!(curve_of(&layer(&h).1).len(), 2);
    // Backspace too.
    click(&mut h, at([64.0, 100.0]));
    assert_eq!(curve_of(&layer(&h).1).len(), 3);
    h.key_press(egui::Key::Backspace);
    h.run_steps(3);
    assert_eq!(curve_of(&layer(&h).1).len(), 2);
    // ⌘/Ctrl-click removes.
    click(&mut h, at([190.0, 120.0]));
    assert_eq!(curve_of(&layer(&h).1).len(), 3);
    let p = at([190.0, 120.0]);
    h.event_modifiers(egui::Event::PointerMoved(p), egui::Modifiers::COMMAND);
    h.event_modifiers(
        egui::Event::PointerButton { pos: p, button: egui::PointerButton::Primary, pressed: true, modifiers: egui::Modifiers::COMMAND },
        egui::Modifiers::COMMAND,
    );
    h.run_steps(1);
    h.event_modifiers(
        egui::Event::PointerButton { pos: p, button: egui::PointerButton::Primary, pressed: false, modifiers: egui::Modifiers::COMMAND },
        egui::Modifiers::COMMAND,
    );
    h.run_steps(3);
    assert_eq!(curve_of(&layer(&h).1).len(), 2);
    // Dragging a point off the graph removes it; endpoints survive both.
    click(&mut h, at([100.0, 60.0]));
    drag(&mut h, at([100.0, 60.0]), g.right_center() + vec2(80.0, 0.0));
    assert_eq!(curve_of(&layer(&h).1).len(), 2);
    drag(&mut h, at([0.0, 0.0]), g.left_top() + vec2(-80.0, -80.0));
    click(&mut h, at([0.0, 0.0]));
    h.key_press(egui::Key::Delete);
    h.run_steps(3);
    assert_eq!(curve_of(&layer(&h).1).len(), 2, "endpoints can't be deleted");
}

/// Capture and history are shared with Camera Raw, but Properties commits on release.
#[test]
fn curves_fast_release_commits_one_edit_and_undo_restores_the_curve() {
    let mut h = app_harness("curves", "rgb", 8);
    let (id, _) = layer(&h);
    let graph = curves_graph(&h, id);
    click(&mut h, graph.center());
    let (_, before) = layer(&h);
    let steps = h.state().session.active().unwrap().history.past_len();
    let at = graph.center() + vec2(4.0, 3.0);
    h.hover_at(at);
    h.run_steps(1);
    h.drag_at(at);
    h.run_steps(1);
    // Only the release frame supplies the destination, as a very fast physical gesture can.
    h.drop_at(at + vec2(40.0, -32.0));
    h.run_steps(3);
    let after = curve_of(&layer(&h).1);
    assert_eq!(after.len(), 3);
    assert!(after[1][0] > 150.0 && after[1][1] > 150.0, "{after:?}");
    assert_eq!(h.state().session.active().unwrap().history.past_len(), steps + 1);
    assert!(h.state().live_adjust.is_none());
    h.state_mut().run("edit.undo", json!({})).unwrap();
    assert_eq!(layer(&h).1, before);
}

/// In a Hue/Saturation layer's Properties, dragging the zones between the range handles moves them
/// as Photoshop does: the fully-on span all four, a fade its own two; one history step per drag.
#[test]
fn hue_saturation_range_zones_drag_their_handles() {
    let mut h = app_harness("hueSaturation", "rgb", 8);
    let (id, _) = layer(&h);
    let mem = egui::Id::new(("adjust-layer", id.0));
    h.ctx.data_mut(|d| d.insert_temp(mem.with("hs-range"), 3usize)); // Greens: 75/105…135\165
    h.run_steps(3);
    let bars: Rect = h.ctx.data(|d| d.get_temp(mem.with("hs-range-bars"))).expect("the range bars");
    let x = |deg: f32| bars.left() + deg / 360.0 * bars.width();
    let y = bars.center().y;
    let greens = |h: &Harness<'_, PhotocraftApp>| match layer(h).1 {
        Adjustment::HueSaturation { ranges, .. } => ranges[2].bounds,
        other => panic!("{other:?}"),
    };
    let steps = h.state().session.active().unwrap().history.past_len();
    drag(&mut h, Pos2::new(x(120.0), y), Pos2::new(x(156.0), y));
    let b = greens(&h);
    assert!((b[0] - 111.0).abs() <= 1.0 && (b[3] - 201.0).abs() <= 1.0 && (b[1] - b[0] - 30.0).abs() < 0.5, "all four moved ~36°: {b:?}");
    assert_eq!(h.state().session.active().unwrap().history.past_len(), steps + 1, "one step");
    let before = greens(&h);
    drag(&mut h, Pos2::new(x((before[0] + before[1]) / 2.0), y), Pos2::new(x((before[0] + before[1]) / 2.0 - 18.0), y));
    let b = greens(&h);
    assert!((b[0] - (before[0] - 18.0)).abs() <= 1.0 && (b[1] - (before[1] - 18.0)).abs() <= 1.0, "left two moved: {before:?} → {b:?}");
    assert_eq!([b[2], b[3]], [before[2], before[3]], "right two stay");
}

/// Black & White's Auto button sets the layer's weights from the image under it, as one history
/// step.
#[test]
fn black_white_auto_button_sets_the_weights() {
    let mut h = app_harness("blackWhite", "rgb", 8);
    // A red and a blue the default mix turns into the same grey, under the adjustment layer.
    h.state_mut()
        .session
        .edit("setup", |doc, _| {
            let b = doc.bounds();
            let bg = doc.layers.first_mut().and_then(|l| l.surface_mut()).unwrap();
            let fmt = bg.format();
            let mut data = Vec::new();
            for _ in b.y0..b.y1 {
                for x in b.x0..b.x1 {
                    let c = if x < b.x1 / 2 { [0.776, 0.2, 0.2, 1.0] } else { [0.29, 0.29, 1.0, 1.0] };
                    data.extend(photocraft_raster::from_rgba(&fmt, c));
                }
            }
            bg.write_region(b, &data);
            Ok(())
        })
        .unwrap();
    h.run_steps(2);
    let (_, before) = layer(&h);
    let steps = h.state().session.active().unwrap().history.past_len();
    let auto = h.query_all_by_label("Auto").last().map(|n| n.rect().center()).expect("an Auto button");
    click(&mut h, auto);
    let (_, after) = layer(&h);
    let Adjustment::BlackWhite { weights, .. } = &after else { panic!("{after:?}") };
    let id = layer(&h).0;
    let expected = h.state_mut().run("image.adjustments.blackWhite.auto", json!({"layer": id.0})).unwrap();
    let want: Vec<f32> = photocraft_engine::adjust_params::HUE_RANGES.iter().map(|k| expected[*k].as_f64().unwrap() as f32).collect();
    assert_eq!(weights.to_vec(), want);
    assert_ne!(after, before, "the red and blue are told apart");
    assert_eq!(h.state().session.active().unwrap().history.past_len(), steps + 1);
}
