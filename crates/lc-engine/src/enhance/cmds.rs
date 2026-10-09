//! Commands of AI Remove. `spot.add {mode: "ai", …}` (in `cmd/masks.rs`) comes
//! here too.

use lightcraft_develop::SpotMode;
use lightcraft_geom::Point;
use serde_json::{Value, json};

use super::session::patch_state;
use super::{AiStroke, CANCELLED};
use crate::cmd::{CommandSpec, always, bad, bool_or, cmd, f64_or, has_active, str_param};
use crate::{Result, Session};

/// Spot radius limits (fraction of the long edge), as `spot.add`'s.
const SIZE: (f64, f64) = (0.001, 0.25);

fn points(p: &Value, key: &str) -> Vec<Point> {
    p.get(key)
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(|q| Some(Point::new(q.get(0)?.as_f64()?, q.get(1)?.as_f64()?))).collect())
        .unwrap_or_default()
}

fn other(c: &str, e: String) -> crate::EngineError {
    bad(c, e)
}

/// After a job ran inline (`wait`): apply it now and report what it did.
fn finish_inline(s: &mut Session, c: &str, job: &super::Job) -> Result<Value> {
    let p = s.enhance_poll();
    if let Some(e) = p.errors.into_iter().next() {
        return Err(other(c, e));
    }
    if job.ctl.cancelled() {
        return Err(other(c, CANCELLED.into()));
    }
    Ok(json!({"job": job.id, "done": true, "activeSpot": s.active_spot}))
}

/// `spot.add {mode: "ai", …}`: start an AI removal of a brushed stroke, a lasso or a develop
/// layer's area on the active photo.
pub(crate) fn add_ai(s: &mut Session, p: &Value) -> Result<Value> {
    const C: &str = "spot.add";
    let id = s.active().ok_or_else(|| bad(C, "no active photo"))?;
    let d = lightcraft_develop::Spot::default();
    let stroke = AiStroke {
        points: points(p, "points"),
        polygon: points(p, "polygon"),
        size: f64_or(p, "size", d.size).clamp(SIZE.0, SIZE.1),
        feather: f64_or(p, "feather", d.feather).clamp(0.0, 100.0),
        opacity: f64_or(p, "opacity", d.opacity).clamp(0.0, 100.0),
        mask: p.get("mask").and_then(Value::as_u64).map(|m| m as u32),
    };
    let wait = bool_or(p, "wait", false);
    let seed = p.get("seed").and_then(Value::as_u64);
    let job = s.start_ai_remove(id, stroke, str_param(p, "engine"), seed, None, wait).map_err(|e| other(C, e))?;
    if wait { finish_inline(s, C, &job) } else { Ok(json!({"job": job.id})) }
}

/// `spot.update` / `spot.refreshSource` on an AI spot: only its opacity can change (its pixels
/// were generated for where it is).
pub(crate) fn check_ai_update(s: &Session, i: usize, p: &Value, c: &str) -> Result<()> {
    if p.get("mode").and_then(Value::as_str) == Some("ai") {
        return Err(bad(c, "paint a new AI removal instead (spot.add with mode ai)"));
    }
    let is_ai = s.active().and_then(|id| s.develop_of(id)).and_then(|d| d.spots.get(i).map(|sp| sp.is_ai())).unwrap_or(false);
    if !is_ai {
        return Ok(());
    }
    let allowed = ["index", "opacity"];
    if c != "spot.update" || p.as_object().is_some_and(|o| o.keys().any(|k| !allowed.contains(&k.as_str()))) {
        return Err(bad(c, "an AI removal can't be moved or reshaped: delete it and paint again, or Regenerate it"));
    }
    Ok(())
}

fn regenerate(s: &mut Session, p: &Value) -> Result<Value> {
    const C: &str = "spot.regenerate";
    let id = s.active().ok_or_else(|| bad(C, "no active photo"))?;
    let i = p.get("index").and_then(Value::as_u64).map(|v| v as usize).or(s.active_spot).ok_or_else(|| bad(C, "no spot selected (give `index`)"))?;
    let d = s.develop_of(id).unwrap_or_default();
    let sp = d.spots.get(i).filter(|sp| sp.mode == SpotMode::Ai).ok_or_else(|| bad(C, "not an AI removal"))?;
    let stroke =
        AiStroke { points: sp.points.clone(), polygon: sp.polygon.clone(), size: sp.size, feather: sp.feather, opacity: sp.opacity, mask: sp.mask };
    let engine = str_param(p, "engine").map(str::to_owned).or_else(|| sp.patch.as_ref().map(|x| x.engine.clone()));
    let wait = bool_or(p, "wait", false);
    let seed = p.get("seed").and_then(Value::as_u64);
    let job = s.start_ai_remove(id, stroke, engine.as_deref(), seed, Some(i), wait).map_err(|e| other(C, e))?;
    if wait { finish_inline(s, C, &job) } else { Ok(json!({"job": job.id})) }
}

fn jobs(s: &mut Session, _: &Value) -> Result<Value> {
    Ok(json!({"jobs": s.enhance.jobs().iter().map(|j| j.json()).collect::<Vec<_>>()}))
}

fn cancel(s: &mut Session, p: &Value) -> Result<Value> {
    Ok(json!({"cancelled": s.enhance.cancel(p.get("job").and_then(Value::as_u64))}))
}

fn status(s: &mut Session, p: &Value) -> Result<Value> {
    const C: &str = "enhance.status";
    let id = match p.get("id").and_then(Value::as_u64) {
        Some(i) => lightcraft_catalog::PhotoId(i),
        None => s.active().ok_or_else(|| bad(C, "no active photo"))?,
    };
    let d = s.develop_of(id).ok_or_else(|| bad(C, "no such photo"))?;
    let spots: Vec<Value> = d
        .spots
        .iter()
        .enumerate()
        .filter(|(_, sp)| sp.is_ai())
        .map(|(i, sp)| json!({"index": i, "state": patch_state(s, id, sp), "engine": sp.patch.as_ref().map(|x| x.engine.clone())}))
        .collect();
    let engines = s.enhance.host.as_ref().map(|h| h.remove_engines()).unwrap_or_default();
    Ok(json!({"spots": spots, "engines": engines, "jobs": s.enhance.running_for(id).map(|j| j.json()).collect::<Vec<_>>()}))
}

pub(crate) fn specs() -> Vec<CommandSpec> {
    vec![
        cmd!(
            "spot.regenerate",
            "Regenerate AI Removal",
            [],
            None,
            "{index? (the selected AI spot), engine?, seed?, wait?} — a new variation of an AI removal (one undo step when it is ready) → {job}",
            has_active,
            regenerate
        ),
        cmd!(query "enhance.jobs", "AI Jobs", [], None, "{} → {jobs: [{id, photo, job, label, progress, message, running}]}", always, jobs),
        cmd!("enhance.cancel", "Cancel AI Job", [], None, "{job? (every job when omitted)} → {cancelled}", always, cancel),
        cmd!(
            query "enhance.status",
            "AI Status",
            [],
            None,
            "{id? (the active photo)} → {spots: [{index, state: ok|stale|missing|foreign, engine}], engines, jobs}",
            has_active,
            status
        ),
    ]
}
