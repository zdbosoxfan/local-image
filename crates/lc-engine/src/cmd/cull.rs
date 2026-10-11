//! Assisted culling: score photos for focus and clipping, group similar shots taken close
//! together (bursts) and mark the sharpest of each group; optionally reject blurry ones and pick
//! each group's best. Scores are kept on the photos (rules / smart albums: `sharpness`,
//! `bestOfGroup`).

use lightcraft_catalog::{Analysis, Flag, Op, PhotoId};
use serde_json::{Value, json};

use super::{CommandSpec, always, cmd};
use crate::{Result, Session};

/// Seconds between shots of the same burst at most.
const BURST_GAP: i64 = 10;
/// Signature similarity for "the same scene".
const SAME: f32 = 0.93;

fn secs(iso: &str) -> Option<i64> {
    let d = lightcraft_meta::DateTime::parse_iso(iso)?;
    Some(((d.year as i64 * 372 + d.month as i64 * 31 + d.day as i64) * 24 + d.hour as i64) * 3600 + d.minute as i64 * 60 + d.second as i64)
}

/// Progress and cancellation shared with the culling worker.
#[derive(Default)]
pub struct Control {
    pub cancel: std::sync::atomic::AtomicBool,
    pub completed: std::sync::atomic::AtomicUsize,
    pub total: usize,
}

type Row = (PhotoId, Option<i64>, f32, f32, [f32; 64]);
struct Input {
    id: PhotoId,
    captured: Option<i64>,
    key: String,
    flag: Flag,
    source: crate::media::SourceRef,
}
pub struct Work {
    generation: u64,
    inputs: Vec<Input>,
}
pub struct Measured {
    generation: u64,
    rows: Vec<Row>,
    failed: Vec<Value>,
    identities: Vec<(PhotoId, String, Flag)>,
}

pub fn prepare(s: &mut Session, p: &Value) -> Work {
    let ids = if p.get("ids").is_some() || s.selection.ids.len() > 1 { s.targets(p) } else { s.visible_cloned() };
    let inputs = ids
        .into_iter()
        .filter_map(|id| {
            let photo = s.catalog.photo(id)?.clone();
            Some(Input {
                id,
                captured: photo.captured.as_deref().and_then(secs),
                key: crate::media::content_key(&photo),
                flag: photo.flag,
                source: s.media.source_ref(&photo, crate::media::SourceLevel::Thumb),
            })
        })
        .collect();
    Work { generation: s.library_generation, inputs }
}
impl Work {
    pub fn len(&self) -> usize {
        self.inputs.len()
    }
    pub fn is_empty(&self) -> bool {
        self.inputs.is_empty()
    }
    pub fn run(self, control: &Control) -> Measured {
        use std::sync::atomic::Ordering;
        let mut result = Measured { generation: self.generation, rows: Vec::new(), failed: Vec::new(), identities: Vec::new() };
        for input in self.inputs {
            if control.cancel.load(Ordering::Relaxed) {
                break;
            }
            let source = crate::guard::catch("Assisted Culling", || input.source.load_source()).and_then(|r| r);
            match source {
                Ok(src) => {
                    use lightcraft_pipeline::cull;
                    result.rows.push((input.id, input.captured, cull::sharpness(&src.image), cull::clipped(&src.image), cull::signature(&src.image)));
                    result.identities.push((input.id, input.key, input.flag));
                }
                Err(e) => result.failed.push(json!([input.id.0, e])),
            }
            control.completed.fetch_add(1, Ordering::Relaxed);
        }
        result
    }
}
fn analyze(s: &mut Session, p: &Value) -> Result<Value> {
    let work = prepare(s, p);
    let measured = work.run(&Control::default());
    apply(s, measured, p)
}
/// Apply only to the same library, sources and flags that were measured. A switched library
/// or an intervening user edit must never receive an obsolete worker's results.
pub fn apply(s: &mut Session, measured: Measured, p: &Value) -> Result<Value> {
    if s.library_generation != measured.generation {
        return Ok(json!({"discarded":true}));
    }
    let mut rows = measured.rows;
    let identities: std::collections::HashMap<_, _> = measured.identities.into_iter().map(|(id, key, flag)| (id, (key, flag))).collect();
    rows.retain(|row| {
        identities.get(&row.0).is_some_and(|(key, flag)| {
            s.catalog.photo(row.0).is_some_and(|photo| crate::media::content_key(photo) == *key && photo.flag == *flag && photo.in_library())
        })
    });
    let failed = measured.failed;
    let reject_below = p.get("rejectBelow").and_then(Value::as_f64).map(|v| v as f32);
    let pick_best = p.get("pickBest").and_then(Value::as_bool).unwrap_or(false);
    // bursts: consecutive (by capture time) look-alike shots
    rows.sort_by_key(|r| (r.1.unwrap_or(i64::MAX), r.0));
    let mut group_of: Vec<Option<u32>> = vec![None; rows.len()];
    let mut next = 1u32;
    for i in 1..rows.len() {
        let (a, b) = (&rows[i - 1], &rows[i]);
        let close = matches!((a.1, b.1), (Some(x), Some(y)) if (y - x).abs() <= BURST_GAP);
        if close && lightcraft_pipeline::cull::similarity(&a.4, &b.4) >= SAME {
            let g = *group_of[i - 1].get_or_insert_with(|| {
                next += 1;
                next - 1
            });
            group_of[i] = Some(g);
        }
    }
    let mut best: std::collections::HashMap<u32, (usize, f32)> = Default::default();
    for (i, g) in group_of.iter().enumerate() {
        if let Some(g) = g {
            let e = best.entry(*g).or_insert((i, -1.0));
            if rows[i].2 > e.1 {
                *e = (i, rows[i].2);
            }
        }
    }
    let mut ops = Vec::new();
    let (mut rejected, mut picked) = (0, 0);
    let mut out = Vec::new();
    for (i, (id, _, sharp, clip, _)) in rows.iter().enumerate() {
        let g = group_of[i];
        let is_best = g.is_some_and(|g| best.get(&g).is_some_and(|b| b.0 == i));
        ops.push(Op::SetAnalysis { id: *id, analysis: Some(Analysis { sharpness: *sharp, clipped: *clip, group: g, best: is_best }) });
        let flag = s.catalog.photo(*id).map(|p| p.flag).unwrap_or_default();
        if reject_below.is_some_and(|t| *sharp < t) && flag != Flag::Reject {
            ops.push(Op::SetFlag { id: *id, flag: Flag::Reject });
            rejected += 1;
        } else if pick_best && is_best && flag == Flag::None {
            ops.push(Op::SetFlag { id: *id, flag: Flag::Pick });
            picked += 1;
        }
        out.push(json!({"id": id.0, "sharpness": (sharp * 10.0).round() / 10.0, "clipped": clip, "group": g, "best": is_best}));
    }
    if !ops.is_empty() {
        s.commit("Assisted Culling", Op::Batch { ops })?;
    }
    Ok(json!({"photos": out, "groups": best.len(), "rejected": rejected, "picked": picked, "failed": failed}))
}

/// Find Similar: photos that look like `id` (signatures from thumbnail-level sources, cached by
/// content), most similar first; the view is filtered to them.
fn find_similar(s: &mut Session, p: &Value) -> Result<Value> {
    const C: &str = "library.findSimilar";
    let id = p.get("id").and_then(Value::as_u64).map(PhotoId).or(s.active()).ok_or_else(|| super::bad(C, "no photo"))?;
    let min = p.get("similarity").and_then(Value::as_f64).unwrap_or(0.8) as f32;
    let all: Vec<PhotoId> = s.catalog.photos().filter(|q| q.in_library()).map(|q| q.id).collect();
    let sig = |s: &mut Session, id: PhotoId| -> Option<[f32; 64]> {
        let key = s.catalog.photo(id).map(|p| crate::media::content_key(p))?;
        if let Some(v) = s.signatures.get(&key) {
            return Some(*v);
        }
        let src = s.source_now(id, crate::media::SourceLevel::Thumb).ok()?;
        let v = lightcraft_pipeline::cull::signature(&src);
        s.signatures.insert(key, v);
        Some(v)
    };
    let me = sig(s, id).ok_or_else(|| super::bad(C, "can't read the photo"))?;
    let mut hits: Vec<(PhotoId, f32)> = Vec::new();
    for q in all {
        if let Some(v) = sig(s, q) {
            let sim = lightcraft_pipeline::cull::similarity(&me, &v);
            if sim >= min {
                hits.push((q, sim));
            }
        }
    }
    hits.sort_by(|a, b| b.1.total_cmp(&a.1));
    if p.get("filter").and_then(Value::as_bool).unwrap_or(true) {
        s.filter.only = hits.iter().map(|h| h.0).collect();
    }
    Ok(json!({"photos": hits.iter().map(|(i, v)| json!({"id": i.0, "similarity": (v * 1000.0).round() / 1000.0})).collect::<Vec<_>>()}))
}

pub fn specs() -> Vec<CommandSpec> {
    vec![
        cmd!(
            "library.findSimilar",
            "Find Similar Photos",
            ["Photo"],
            None,
            "{id?, similarity?: 0..1 (0.8), filter?: bool (true)} — photos that look like the active one (composition and tones), most similar first; filters the view to them (Clear Filters to go back) → {photos: [{id, similarity}]}",
            always,
            find_similar
        ),
        cmd!(
            "photo.analyze",
            "Assisted Culling",
            [],
            None,
            "{ids?, rejectBelow?: sharpness 0..100, pickBest?: bool} — score the selected photos (else all in view) for focus (0..100) and clipping, group look-alike shots taken within 10 s and mark the sharpest of each; optionally reject blurry photos and pick each group's best; one undo step → {photos: [{id, sharpness, clipped, group, best}], groups, rejected, picked}",
            always,
            analyze
        ),
    ]
}
