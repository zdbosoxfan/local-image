//! The Phase 1 command surface shared by CLI, automation and future UI. Long analysis is
//! synchronous here; prepared input jobs and cancellation are also available to workers.

use super::{CommandSpec, always, bad, cmd};
use crate::smart_sort::{DEFAULT_MODEL, FolderDef, SortPreset, builtin_presets, classify::Classifier};
use crate::{EngineError, Result, Session};
use lightcraft_catalog::{Flag, MediaKind, Op, PhotoId};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

fn source(s: &mut Session, p: &Value) -> Result<Vec<PhotoId>> {
    let ids = if let Some(ids) = p.get("ids") {
        serde_json::from_value::<Vec<PhotoId>>(ids.clone()).map_err(|e| bad("smartSort", e.to_string()))?
    } else if s.selection.ids.len() > 1 {
        s.selection.ids.clone()
    } else {
        s.visible_cloned()
    };
    let include = p.get("includeRejected").and_then(Value::as_bool).unwrap_or(false);
    let mut seen = BTreeSet::new();
    Ok(ids.into_iter().filter(|id| seen.insert(*id)).filter(|id| s.catalog.photo(*id).is_some_and(|p| include || p.flag != Flag::Reject)).collect())
}

fn preset(s: &Session, p: &Value, id: &str) -> Result<SortPreset> {
    let preset = match p.get("preset") {
        Some(Value::String(name)) => s
            .smart
            .prefs
            .presets
            .iter()
            .chain(builtin_presets().iter())
            .find(|p| p.name.eq_ignore_ascii_case(name))
            .cloned()
            .ok_or_else(|| bad(id, "unknown sort preset"))?,
        Some(v) => serde_json::from_value(v.clone()).map_err(|e| bad(id, e.to_string()))?,
        None => s.smart.prefs.last.clone().unwrap_or_else(|| builtin_presets().remove(0)),
    };
    preset.validate().map_err(|e| bad(id, e))?;
    Ok(preset)
}

fn status(s: &mut Session, _: &Value) -> Result<Value> {
    let (model, dim) = s.smart.tagger.as_ref().map_or((DEFAULT_MODEL.to_string(), 512), |t| (t.model_id().to_string(), t.dim()));
    s.smart.store.ensure(&model, dim).map_err(EngineError::Other)?;
    let spec = li_seg::spec(DEFAULT_MODEL).ok_or_else(|| EngineError::Other("missing CLIP specification".into()))?;
    let injected = model != DEFAULT_MODEL && s.smart.tagger.is_some();
    let missing: Vec<_> = if injected {
        Vec::new()
    } else {
        spec.files().filter(|f| s.quick_seg_dir.as_ref().is_none_or(|d| !li_seg::file_installed(d, *f))).map(|f| f.file).collect()
    };
    let installed = injected || missing.is_empty();
    Ok(json!({"tagger":{"id":model,"installed":installed,"bytes":if injected {0} else {spec.download_bytes()},"missing":missing},
        "faces":{"installed":false,"enabled":false},"analysed":{"tags":s.smart.store.len(&model),"faces":0},"total":s.catalog.photos().filter(|p| p.in_library()).count()}))
}

fn list(s: &mut Session, _: &Value) -> Result<Value> {
    Ok(Value::Array(
        builtin_presets()
            .into_iter()
            .map(|p| json!({"name":p.name,"builtin":true,"preset":p}))
            .chain(s.smart.prefs.presets.iter().map(|p| json!({"name":p.name,"builtin":false,"preset":p})))
            .collect(),
    ))
}

fn save(s: &mut Session, p: &Value) -> Result<Value> {
    const ID: &str = "smartSort.savePreset";
    let mut preset = preset(s, p, ID)?;
    if let Some(name) = p.get("name").and_then(Value::as_str) {
        preset.name = name.trim().into();
    }
    preset.validate().map_err(|e| bad(ID, e))?;
    if builtin_presets().iter().any(|b| b.name.eq_ignore_ascii_case(&preset.name)) {
        return Err(bad(ID, "choose a name other than a built-in preset"));
    }
    match s.smart.prefs.presets.iter_mut().find(|p| p.name.eq_ignore_ascii_case(&preset.name)) {
        Some(p) => *p = preset,
        None => s.smart.prefs.presets.push(preset),
    }
    s.save_prefs()?;
    list(s, &Value::Null)
}

fn delete(s: &mut Session, p: &Value) -> Result<Value> {
    const ID: &str = "smartSort.deletePreset";
    let name = p.get("name").and_then(Value::as_str).ok_or_else(|| bad(ID, "missing name"))?;
    let n = s.smart.prefs.presets.len();
    s.smart.prefs.presets.retain(|p| !p.name.eq_ignore_ascii_case(name.trim()));
    if n == s.smart.prefs.presets.len() {
        return Err(bad(ID, "no user preset with this name"));
    }
    s.save_prefs()?;
    list(s, &Value::Null)
}

fn analyze(s: &mut Session, p: &Value) -> Result<Value> {
    if p.get("faces").and_then(Value::as_bool).unwrap_or(false) {
        return Err(bad("smartSort.analyze", "face analysis is not available in Phase 1"));
    }
    let ids = source(s, p)?;
    let cancel = std::sync::atomic::AtomicBool::new(false);
    let result = crate::smart_sort::analyze(s, &ids, &cancel, &|_, _| {})?;
    serde_json::to_value(result).map_err(|e| EngineError::Other(e.to_string()))
}

#[derive(Deserialize)]
struct Assignment {
    id: PhotoId,
    categories: Vec<String>,
}
#[derive(Serialize)]
struct Classified {
    id: PhotoId,
    #[serde(flatten)]
    row: crate::smart_sort::classify::Classification,
}

fn classify(s: &mut Session, p: &Value) -> Result<Value> {
    const ID: &str = "smartSort.classify";
    let mut preset = preset(s, p, ID)?;
    let ids = source(s, p)?;
    let overrides: Vec<Assignment> =
        serde_json::from_value(p.get("overrides").cloned().unwrap_or_else(|| json!([]))).map_err(|e| bad(ID, e.to_string()))?;
    let mut manual = BTreeMap::new();
    for assignment in overrides {
        if assignment.categories.iter().any(|n| !preset.categories.iter().any(|c| c.name == *n)) {
            return Err(bad(ID, "unknown override category"));
        }
        let photo = s.catalog.photo(assignment.id).ok_or_else(|| bad(ID, "unknown override photo"))?;
        let key = crate::media::content_key(photo);
        // A move both sets this photo's assignment and refines category prototypes for others.
        for c in &mut preset.categories {
            c.exemplars.retain(|e| e != &key);
            if assignment.categories.contains(&c.name) {
                c.exemplars.push(key.clone());
            }
        }
        if manual.insert(assignment.id, assignment.categories).is_some() {
            return Err(bad(ID, "duplicate override photo"));
        }
    }
    let tagger = s.smart.tagger(s.quick_seg_dir.as_deref()).map_err(EngineError::Other)?;
    s.smart.store.ensure(tagger.model_id(), tagger.dim()).map_err(EngineError::Other)?;
    let classifier = Classifier::new(tagger.as_ref(), &s.smart.store, &preset).map_err(EngineError::Other)?;
    let mut counts: BTreeMap<String, usize> = preset.categories.iter().map(|c| (c.name.clone(), 0)).collect();
    counts.insert("Unsorted".into(), 0);
    let mut photos = Vec::new();
    for id in ids {
        let photo = s.catalog.photo(id).ok_or_else(|| bad(ID, "unknown photo"))?;
        if photo.kind == MediaKind::Video {
            continue;
        }
        let key = crate::media::content_key(photo);
        let row =
            classifier.classify(&key, s.smart.store.get(tagger.model_id(), &key), manual.get(&id).map(Vec::as_slice)).map_err(|e| bad(ID, e))?;
        if row.assigned.is_empty() {
            *counts.entry("Unsorted".into()).or_default() += 1;
        }
        for c in &row.assigned {
            *counts.entry(c.clone()).or_default() += 1;
        }
        photos.push(Classified { id, row });
    }
    s.smart.prefs.last = Some(preset);
    s.save_prefs()?;
    Ok(json!({"photos":photos,"counts":counts}))
}

fn keywords(s: &mut Session, p: &Value) -> Result<Value> {
    const ID: &str = "smartSort.applyKeywords";
    let preset = preset(s, p, ID)?;
    let parent = lightcraft_catalog::keywords::clean(&preset.keyword_parent);
    let rows: Vec<Assignment> =
        serde_json::from_value(p.get("assignments").cloned().ok_or_else(|| bad(ID, "missing assignments"))?).map_err(|e| bad(ID, e.to_string()))?;
    let replace = p.get("replace").and_then(Value::as_bool).unwrap_or(true);
    let mut seen = BTreeSet::new();
    let mut ops = Vec::new();
    for row in rows {
        if !seen.insert(row.id) {
            return Err(bad(ID, "duplicate assignment photo"));
        }
        if row.categories.iter().any(|n| !preset.categories.iter().any(|c| c.name == *n)) {
            return Err(bad(ID, "unknown assignment category"));
        }
        let photo = s.catalog.photo(row.id).ok_or_else(|| bad(ID, "unknown assignment photo"))?;
        let mut meta = photo.meta.clone();
        if replace {
            // Preserve the parent itself: replace only the category descendants.
            meta.keywords.retain(|k| k.eq_ignore_ascii_case(&parent) || !lightcraft_catalog::keywords::is_under(k, &parent));
        }
        for category in row.categories {
            let keyword = format!("{parent}|{}", category.trim());
            if !meta.keywords.iter().any(|k| k.eq_ignore_ascii_case(&keyword)) {
                meta.keywords.push(keyword);
            }
        }
        if meta != photo.meta {
            ops.push(Op::SetMeta { id: row.id, meta: Box::new(meta) });
        }
    }
    let changed = ops.len();
    if changed > 0 {
        s.commit("Smart Sort", Op::Batch { ops })?;
    }
    Ok(json!({"changed":changed}))
}

fn plan(s: &mut Session, p: &Value) -> Result<Value> {
    let folders: Vec<FolderDef> = serde_json::from_value(p.get("folders").cloned().ok_or_else(|| bad("smartSort.plan", "missing folders"))?)
        .map_err(|e| bad("smartSort.plan", e.to_string()))?;
    let ids = source(s, p)?;
    let folders = crate::smart_sort::plan::plan(&s.catalog, &ids, &folders).map_err(|e| bad("smartSort.plan", e))?;
    Ok(json!({"folders":folders}))
}

pub fn specs() -> Vec<CommandSpec> {
    vec![
        cmd!(query "smartSort.status","Smart Sort Status",[],None,"{} → {tagger, faces, analysed, total}",always,status),
        cmd!(query "smartSort.presets","Smart Sort Presets",[],None,"{} → [{name, builtin, preset}]",always,list),
        cmd!("smartSort.savePreset", "Save Smart Sort Preset", [], None, "{name?, preset: SortPreset}", always, save),
        cmd!("smartSort.deletePreset", "Delete Smart Sort Preset", [], None, "{name}", always, delete),
        cmd!(
            "smartSort.analyze",
            "Analyse Smart Sort",
            [],
            None,
            "{ids?, faces?: false, includeRejected?: false} → {analysed, skipped, videos, failed}",
            always,
            analyze
        ),
        cmd!(
            "smartSort.classify",
            "Classify Smart Sort",
            [],
            None,
            "{preset: SortPreset | name, ids?, overrides?: [{id, categories}], includeRejected?: false} → {photos, counts}",
            always,
            classify
        ),
        cmd!(
            "smartSort.applyKeywords",
            "Apply Smart Sort Keywords",
            [],
            None,
            "{preset, assignments: [{id, categories}], replace?: true} → {changed}; one undo step",
            always,
            keywords
        ),
        cmd!(query "smartSort.plan","Plan Smart Sort Folders",[],None,"{folders: [{name, rules, enabled}], ids?, includeRejected?: false} → {folders: [{name, ids}]} (keywords only, overlap included)",always,plan),
    ]
}
