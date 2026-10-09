//! Headless tests of AI in Develop: the Remove tool's AI mode (brush, lasso, Regenerate, undo)
//! with a mock AI engine, and AI Denoise in the Detail section with a stand-in denoiser.

use std::sync::Arc;
use std::time::Duration;

use lightcraft_engine::enhance::{AiHost, Download, JobCtl, RemoveEngine, RemoveRequest, RemoveResult};
use serde_json::json;

use crate::headless::Headless;
use crate::{LightcraftApp, Services};

const T: Duration = Duration::from_secs(20);
const JOB: Duration = Duration::from_secs(300);

/// Paints the masked area magenta.
struct Mock;

impl AiHost for Mock {
    fn remove_engines(&self) -> Vec<RemoveEngine> {
        vec![RemoveEngine { key: "mock".into(), label: "Mock Engine".into(), problem: None }]
    }
    fn remove(&self, req: &RemoveRequest, _: &JobCtl) -> Result<RemoveResult, String> {
        let rgb = req.rgb.iter().zip(&req.mask).map(|(c, m)| if *m > 127 { [250, 0, 250] } else { *c }).collect();
        Ok(RemoveResult { rgb, alpha: req.mask.iter().map(|m| if *m > 127 { 255 } else { 0 }).collect() })
    }
    fn start_model_download(&self, _: &str) -> Result<(), String> {
        Err("not in tests".into())
    }
    fn model_download(&self, _: &str) -> Option<Download> {
        None
    }
    fn cancel_model_download(&self, _: &str) {}
}

fn app() -> Headless {
    let mut session = lightcraft_engine::Session::with_demo();
    session.enhance.host = Some(Arc::new(Mock));
    let app = LightcraftApp::new(session, Services { png: None, ..Default::default() });
    let mut h = Headless::new(app, [1200.0, 800.0], 1.0);
    let r = h.request("ui.set", json!({"view": "detail"}), T);
    assert_eq!(r["ok"], true, "{r}");
    h
}

fn exec(h: &mut Headless, command: &str, params: serde_json::Value) -> serde_json::Value {
    let r = h.request("engine.execute", json!({"command": command, "params": params}), T);
    assert_eq!(r["ok"], true, "{command}: {r}");
    r["result"].clone()
}

fn click(h: &mut Headless, id: &str) {
    let r = h.request("ui.clickWidget", json!({"id": id}), T);
    assert_eq!(r["ok"], true, "{id}: {r}");
}

fn spots(h: &Headless) -> Vec<lightcraft_develop::Spot> {
    let id = h.app.session.active().expect("active photo");
    h.app.session.develop_of(id).unwrap_or_default().spots.clone()
}

#[test]
fn ai_remove_by_brush_and_lasso() {
    let mut h = app();
    exec(&mut h, "panel.remove", json!({}));
    click(&mut h, "button:removeMode-ai");
    assert_eq!(h.app.ui.tool, "ai");
    h.step();
    assert_eq!(h.app.ui.remove_engine, "mock", "the ready engine is chosen");
    let w = h.request("ui.widgets", json!({"filter": "button:remove"}), T);
    for id in ["button:removeAiEngine", "button:removeBrush", "button:removeLasso"] {
        assert!(w["result"].as_array().unwrap().iter().any(|x| x["id"] == id), "{id} in {w}");
    }
    // brush a removal: it waits for Remove, then runs in the background and lands as one AI
    // spot, selected
    let r = h.request(
        "ui.pointer",
        json!({"events": [{"kind": "down", "x": 0.5, "y": 0.5}, {"kind": "drag", "x": 0.52, "y": 0.5}, {"kind": "up", "x": 0.52, "y": 0.5}]}),
        T,
    );
    assert_eq!(r["ok"], true, "{r}");
    h.settle(T);
    assert!(h.app.ui.remove_draft.is_some() && h.app.session.enhance.jobs().is_empty() && spots(&h).is_empty(), "nothing runs yet");
    click(&mut h, "button:removeDraftApply");
    assert!(h.app.ui.remove_draft.is_none());
    assert!(h.step_until(JOB, |h| spots(h).len() == 1), "the removal arrives");
    let sp = spots(&h)[0].clone();
    assert!(sp.is_ai() && sp.patch.is_some());
    assert_eq!(sp.patch.as_ref().unwrap().engine, "mock");
    assert_eq!(h.app.session.active_spot, Some(0));
    // the selected AI removal offers Regenerate: a new seed, still one spot
    h.step();
    click(&mut h, "button:spotRegenerate");
    let seed = sp.patch.as_ref().unwrap().seed;
    assert!(h.step_until(JOB, |h| spots(h)[0].patch.as_ref().is_some_and(|p| p.seed != seed)), "regenerated");
    assert_eq!(spots(&h).len(), 1);
    // dragging it doesn't move it (its pixels were made for that place)
    let at = sp.points[0];
    let _ = h.request(
        "ui.pointer",
        json!({"events": [{"kind": "down", "x": at.x, "y": at.y}, {"kind": "drag", "x": at.x + 0.01, "y": at.y}, {"kind": "drag", "x": 0.2, "y": 0.2}, {"kind": "up", "x": 0.2, "y": 0.2}]}),
        T,
    );
    assert_eq!(spots(&h)[0].points, sp.points);
    // a lasso
    click(&mut h, "button:removeLasso");
    assert!(h.app.ui.remove_lasso);
    let r = h.request(
        "ui.pointer",
        json!({"events": [
            {"kind": "down", "x": 0.2, "y": 0.6}, {"kind": "drag", "x": 0.3, "y": 0.6}, {"kind": "drag", "x": 0.3, "y": 0.75},
            {"kind": "drag", "x": 0.2, "y": 0.75}, {"kind": "up", "x": 0.2, "y": 0.75}
        ]}),
        T,
    );
    assert_eq!(r["ok"], true, "{r}");
    h.settle(T);
    assert_eq!(spots(&h).len(), 1, "the lasso waits for Remove");
    // Enter removes it
    h.request("ui.key", json!({"key": "Enter"}), T);
    assert!(h.step_until(JOB, |h| spots(h).len() == 2), "the lasso removal arrives");
    assert_eq!(h.app.ui.right, crate::state::RightPanel::Remove, "Enter removed the draft, it didn't close the tool");
    let lasso = spots(&h)[1].clone();
    assert!(lasso.is_ai() && lasso.polygon.len() >= 3 && lasso.points.is_empty(), "{lasso:?}");
    // undo takes the lasso removal away, as one step
    exec(&mut h, "edit.undo", json!({}));
    assert_eq!(spots(&h).len(), 1);
}

fn widget(h: &mut Headless, id: &str) -> bool {
    let w = h.request("ui.widgets", json!({"filter": id}), T);
    w["result"].as_array().is_some_and(|a| a.iter().any(|x| x["id"] == id))
}

fn brush(h: &mut Headless, from: (f64, f64), to: (f64, f64), alt: bool) {
    let r = h.request(
        "ui.pointer",
        json!({"alt": alt, "events": [
            {"kind": "down", "x": from.0, "y": from.1}, {"kind": "drag", "x": (from.0 + to.0) / 2.0, "y": (from.1 + to.1) / 2.0},
            {"kind": "drag", "x": to.0, "y": to.1}, {"kind": "up", "x": to.0, "y": to.1}
        ]}),
        T,
    );
    assert_eq!(r["ok"], true, "{r}");
    h.settle(T);
}

/// Lightroom's generative Remove: strokes gather until Remove (button or Enter); Cancel (button or
/// Esc), another tool or another photo drops them; nothing runs before.
#[test]
fn ai_remove_waits_for_confirmation() {
    let mut h = app();
    exec(&mut h, "panel.remove", json!({}));
    click(&mut h, "button:removeMode-ai");
    let no_job = |h: &Headless| h.app.session.enhance.jobs().is_empty() && spots(h).is_empty();
    // two strokes make one draft, shown with Remove / Cancel on the photo and in the panel
    brush(&mut h, (0.3, 0.3), (0.35, 0.3), false);
    let first = h.app.ui.remove_draft.as_ref().map(|d| d.points.len()).expect("a draft");
    brush(&mut h, (0.6, 0.6), (0.65, 0.6), false);
    let both = h.app.ui.remove_draft.as_ref().map(|d| d.points.len()).unwrap();
    assert!(both > first, "{first} → {both}");
    assert!(no_job(&h), "releasing starts nothing");
    for id in ["button:removeDraftApply", "button:removeDraftCancel", "button:removePanelApply", "button:removePanelCancel"] {
        assert!(widget(&mut h, id), "{id}");
    }
    // ⌥ takes the second stroke away again
    brush(&mut h, (0.6, 0.6), (0.65, 0.6), true);
    assert_eq!(h.app.ui.remove_draft.as_ref().map(|d| d.points.len()), Some(first));
    // Esc drops it (and doesn't leave the photo)
    h.request("ui.key", json!({"key": "Escape"}), T);
    h.settle(T);
    assert!(h.app.ui.remove_draft.is_none() && no_job(&h));
    assert_eq!(h.app.ui.view, crate::state::ViewMode::Detail);
    // the panel's Cancel drops it
    brush(&mut h, (0.3, 0.3), (0.35, 0.3), false);
    click(&mut h, "button:removePanelCancel");
    assert!(h.app.ui.remove_draft.is_none() && no_job(&h));
    // another tool or another photo drops it
    brush(&mut h, (0.3, 0.3), (0.35, 0.3), false);
    click(&mut h, "button:removeMode-heal");
    h.step();
    assert!(h.app.ui.remove_draft.is_none() && no_job(&h));
    click(&mut h, "button:removeMode-ai");
    brush(&mut h, (0.3, 0.3), (0.35, 0.3), false);
    exec(&mut h, "library.next", json!({}));
    h.step();
    assert!(h.app.ui.remove_draft.is_none() && no_job(&h));
    // a stroke and a lasso, removed together by the panel's Remove: one AI removal
    brush(&mut h, (0.3, 0.3), (0.35, 0.3), false);
    click(&mut h, "button:removeLasso");
    let r = h.request(
        "ui.pointer",
        json!({"events": [
            {"kind": "down", "x": 0.6, "y": 0.6}, {"kind": "drag", "x": 0.7, "y": 0.6}, {"kind": "drag", "x": 0.7, "y": 0.7},
            {"kind": "drag", "x": 0.6, "y": 0.7}, {"kind": "up", "x": 0.6, "y": 0.7}
        ]}),
        T,
    );
    assert_eq!(r["ok"], true, "{r}");
    h.settle(T);
    assert!(no_job(&h));
    click(&mut h, "button:removePanelApply");
    assert!(h.step_until(JOB, |h| spots(h).len() == 1), "the removal arrives");
    let sp = spots(&h)[0].clone();
    assert!(sp.is_ai() && !sp.points.is_empty() && sp.polygon.len() >= 3, "{sp:?}");
}

/// After an AI removal the Heal brush works over it: the brush keeps its own size, the old spot
/// is deselected and only its pin grabs, and its outline (the "ghost" of the stroke) isn't drawn
/// unless it is selected or hovered.
#[test]
fn heal_over_an_ai_removal_makes_a_new_heal_spot() {
    let mut h = app();
    // a small photo (a file the loader makes up), so the heal runs in a moment
    let id = {
        use lightcraft_catalog::{Op, Photo, Source};
        let s = &mut h.app.session;
        s.media.file_loader = Some(Arc::new(|_, _| {
            let img = lightcraft_raster::Rgb32f::from_fn(360, 240, |x, y| [0.2 + 0.1 * (((x + y) / 4) % 2) as f32, 0.25, 0.3]);
            Ok((img, lightcraft_pipeline::SourceInfo::default()))
        }));
        let id = s.catalog.alloc_photo_id();
        let now = (s.clock)();
        let mut p = Photo::new(id, Source::File { path: "/synthetic/small.tif".into() }, "small.tif", "TIFF", 360, 240, &now);
        p.content_hash = Some("synthetic-small".into());
        s.catalog.apply(Op::AddPhoto { photo: Box::new(p) }).unwrap();
        id
    };
    exec(&mut h, "library.select", json!({"ids": [id.0]}));
    h.settle(T);
    exec(&mut h, "panel.remove", json!({}));
    click(&mut h, "button:removeMode-ai");
    h.app.ui.remove_size = 0.08;
    brush(&mut h, (0.5, 0.5), (0.52, 0.5), false);
    h.request("ui.key", json!({"key": "Enter"}), T);
    assert!(h.step_until(JOB, |h| spots(h).len() == 1), "the removal arrives");
    h.settle(T);
    assert_eq!(h.app.session.active_spot, Some(0));
    let ai_points = spots(&h)[0].points.clone();
    assert!(widget(&mut h, "spotOutline:0"), "the selected removal shows its outline");
    // a smaller brush stays smaller while the AI removal is selected
    h.app.ui.remove_size = 0.01;
    h.settle(T);
    assert_eq!(h.app.ui.remove_size, 0.01, "an AI spot's size isn't copied into the brush");
    // Heal: the AI removal is deselected; with the pointer elsewhere only its pin shows
    click(&mut h, "button:removeMode-heal");
    assert_eq!(h.app.session.active_spot, None);
    h.request("ui.pointer", json!({"events": [{"kind": "move", "x": 0.9, "y": 0.9}]}), T);
    h.settle(T);
    assert!(!widget(&mut h, "spotOutline:0"), "no ghost of the AI stroke");
    assert!(widget(&mut h, "spotPin:0"));
    // a heal stroke starting inside the removed area (away from its pin) heals, at the heal size
    brush(&mut h, (0.56, 0.5), (0.58, 0.5), false);
    let jobs: Vec<String> = h.app.session.enhance.jobs().iter().map(|j| j.json().to_string()).collect();
    let ok = h.step_until(T, |h| spots(h).len() == 2);
    assert!(ok, "the heal arrives: jobs {jobs:?}, active spot {:?}, toast {:?}", h.app.session.active_spot, h.app.ui.toast);
    let sp = spots(&h);
    assert_eq!(sp[0].points, ai_points, "the AI removal didn't move");
    let heal = &sp[1];
    assert!(heal.is_ai() && heal.patch.as_ref().is_some_and(|p| p.engine == "local"), "a content-aware heal: {heal:?}");
    assert!((heal.size - 0.01).abs() < 1e-6, "at the heal brush size: {}", heal.size);
    assert_eq!(h.app.session.undo.last().map(|u| u.label.clone()).as_deref(), Some("Heal"));
}

#[test]
fn ai_denoise_in_the_detail_section() {
    let mut h = app();
    h.app.session.enhance.denoiser = Some(Arc::new(|px, _, _, _| Ok(px.iter().map(|p| [(p[0] + p[1] + p[2]) / 3.0; 3]).collect())));
    exec(&mut h, "panel.edit", json!({}));
    h.step();
    let id = h.app.session.active().unwrap();
    let ready = h.request("ui.widgets", json!({"filter": "button:denoiseRun"}), T);
    if ready["result"].as_array().is_some_and(|a| !a.is_empty()) {
        click(&mut h, "button:denoiseRun");
    } else {
        // (the Detail section is scrolled out of view or folded: start it the way the button does)
        exec(&mut h, "enhance.denoise", json!({}));
    }
    assert!(h.step_until(JOB, |h| h.app.session.develop_of(id).is_some_and(|d| d.enhance.ai.is_some())), "Denoise finishes");
    let d = h.app.session.develop_of(id).unwrap();
    assert_eq!(d.enhance.denoise, 60.0);
    assert_eq!(h.app.session.undo.last().map(|u| u.label.clone()).as_deref(), Some("AI Denoise"));
    // the Amount slider drives it now
    exec(&mut h, "develop.set", json!({"control": "enhance.denoise", "value": 30}));
    assert_eq!(h.app.session.develop_of(id).unwrap().enhance.denoise, 30.0);
}
