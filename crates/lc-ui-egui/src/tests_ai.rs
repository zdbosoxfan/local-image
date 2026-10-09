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
    // brush a removal: it runs in the background and lands as one AI spot, selected
    let r = h.request("ui.pointer", json!({"events": [{"kind": "down", "x": 0.5, "y": 0.5}, {"kind": "drag", "x": 0.52, "y": 0.5}, {"kind": "up", "x": 0.52, "y": 0.5}]}), T);
    assert_eq!(r["ok"], true, "{r}");
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
    assert!(h.step_until(JOB, |h| spots(h).len() == 2), "the lasso removal arrives");
    let lasso = spots(&h)[1].clone();
    assert!(lasso.is_ai() && lasso.polygon.len() >= 3 && lasso.points.is_empty(), "{lasso:?}");
    // undo takes the lasso removal away, as one step
    exec(&mut h, "edit.undo", json!({}));
    assert_eq!(spots(&h).len(), 1);
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
