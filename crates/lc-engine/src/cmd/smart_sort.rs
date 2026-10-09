//! Commands shared by CLI, automation and the Smart Sort dialog. Long analysis is
//! synchronous here; prepared input jobs and cancellation are also available to workers.

use super::{CommandSpec, always, bad, cmd};
use crate::smart_sort::{DEFAULT_MODEL, FolderDef, SortPreset, builtin_presets, classify::Classifier};
use crate::{EngineError, Result, Session};
use lightcraft_catalog::{Flag, MediaKind, Op, PhotoId};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

pub(crate) fn source(s: &mut Session, p: &Value) -> Result<Vec<PhotoId>> {
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
    let faces = super::smart_sort_people::status_value(s)?;
    let face_count = faces["analysed"].clone();
    Ok(json!({"tagger":{"id":model,"installed":installed,"bytes":if injected {0} else {spec.download_bytes()},"missing":missing},
        "faces":faces,"analysed":{"tags":s.smart.store.len(&model),"faces":face_count},"total":s.catalog.photos().filter(|p| p.in_library()).count()}))
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
    preset.name = preset.name.trim().to_string();
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
        s.require_faces()?;
    }
    let ids = source(s, p)?;
    let cancel = std::sync::atomic::AtomicBool::new(false);
    let result = crate::smart_sort::analyze(s, &ids, &cancel, &|_, _| {})?;
    let mut result = serde_json::to_value(result).map_err(|e| EngineError::Other(e.to_string()))?;
    if p.get("faces").and_then(Value::as_bool).unwrap_or(false) {
        result["faces"] = super::smart_sort_people::analyze(s, p)?;
    }
    Ok(result)
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
    let face_model = if s.smart.prefs.faces_enabled { Some(s.smart.people.ready().map_err(EngineError::Other)?) } else { None };
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
        let row = classifier
            .classify_with_faces(
                &key,
                s.smart.store.get(tagger.model_id(), &key),
                manual.get(&id).map(Vec::as_slice),
                face_model.as_ref().and_then(|model| s.smart.people.store.get(model, &key)).map(<[_]>::len),
            )
            .map_err(|e| bad(ID, e))?;
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
    let mut folders: Vec<FolderDef> = serde_json::from_value(p.get("folders").cloned().ok_or_else(|| bad("smartSort.plan", "missing folders"))?)
        .map_err(|e| bad("smartSort.plan", e.to_string()))?;
    let ids = source(s, p)?;
    let notices = super::smart_sort_people::prepare_folders(s, &mut folders)?;
    let folders = crate::smart_sort::plan::plan_with_options(
        &s.catalog,
        &ids,
        &folders,
        p.get("firstMatch").and_then(Value::as_bool).unwrap_or(false),
        p.get("unsorted").and_then(Value::as_str),
    )
    .map_err(|e| bad("smartSort.plan", e))?;
    Ok(json!({"folders":folders,"notices":notices}))
}

fn tag_sets(s: &mut Session, _: &Value) -> Result<Value> {
    Ok(Value::Array(
        crate::smart_sort::tagsets::builtins()
            .into_iter()
            .map(|set| json!({"name":set.name,"tags":set.tags,"builtin":true}))
            .chain(s.smart.tag_sets.iter().map(|set| json!({"name":set.name,"tags":set.tags,"builtin":false})))
            .collect(),
    ))
}
fn set_name(p: &Value, key: &str) -> Result<String> {
    p.get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|n| !n.is_empty())
        .map(str::to_string)
        .ok_or_else(|| bad("smartSort.tagSets", "missing tag set name"))
}
fn check_set_name(name: &str) -> Result<()> {
    if crate::smart_sort::tagsets::builtins().iter().any(|t| t.name.eq_ignore_ascii_case(name)) {
        return Err(bad("smartSort.tagSets", "choose a name other than a built-in tag set"));
    }
    Ok(())
}
fn save_tag_set(s: &mut Session, p: &Value) -> Result<Value> {
    let name = set_name(p, "name")?;
    check_set_name(&name)?;
    let tags: Vec<String> =
        serde_json::from_value(p.get("tags").cloned().unwrap_or(Value::Null)).map_err(|e| bad("smartSort.saveTagSet", e.to_string()))?;
    let mut cleaned = Vec::new();
    for tag in tags {
        let tag = tag.trim().to_string();
        if !tag.is_empty() && !cleaned.contains(&tag) {
            cleaned.push(tag);
        }
    }
    let set = crate::smart_sort::tagsets::TagSet { name, tags: cleaned };
    let old = s.smart.tag_sets.clone();
    if let Some(existing) = s.smart.tag_sets.iter_mut().find(|t| t.name.eq_ignore_ascii_case(&set.name)) {
        *existing = set;
    } else {
        s.smart.tag_sets.push(set);
    }
    if let Err(e) = s.save_smart_tag_sets() {
        s.smart.tag_sets = old;
        return Err(e);
    }
    tag_sets(s, &Value::Null)
}
fn delete_tag_set(s: &mut Session, p: &Value) -> Result<Value> {
    let name = set_name(p, "name")?;
    let old = s.smart.tag_sets.clone();
    s.smart.tag_sets.retain(|t| !t.name.eq_ignore_ascii_case(&name));
    if old.len() == s.smart.tag_sets.len() {
        return Err(bad("smartSort.deleteTagSet", "unknown user tag set"));
    }
    if let Err(e) = s.save_smart_tag_sets() {
        s.smart.tag_sets = old;
        return Err(e);
    }
    tag_sets(s, &Value::Null)
}
fn rename_tag_set(s: &mut Session, p: &Value) -> Result<Value> {
    let name = set_name(p, "name")?;
    let to = set_name(p, "to")?;
    check_set_name(&to)?;
    if s.smart.tag_sets.iter().any(|t| !t.name.eq_ignore_ascii_case(&name) && t.name.eq_ignore_ascii_case(&to)) {
        return Err(bad("smartSort.renameTagSet", "duplicate tag set name"));
    }
    let old = s.smart.tag_sets.clone();
    let set = s
        .smart
        .tag_sets
        .iter_mut()
        .find(|t| t.name.eq_ignore_ascii_case(&name))
        .ok_or_else(|| bad("smartSort.renameTagSet", "unknown user tag set"))?;
    set.name = to;
    if let Err(e) = s.save_smart_tag_sets() {
        s.smart.tag_sets = old;
        return Err(e);
    }
    tag_sets(s, &Value::Null)
}
fn export(s: &mut Session, p: &Value) -> Result<Value> {
    let result = plan(s, p)?;
    let folders: Vec<crate::smart_sort::plan::Folder> =
        serde_json::from_value(result["folders"].clone()).map_err(|e| bad("smartSort.export", e.to_string()))?;
    let dir = p.get("dir").and_then(Value::as_str).ok_or_else(|| bad("smartSort.export", "missing destination"))?;
    let params = s.export_params(p).map_err(EngineError::Other)?;
    let mut opts = crate::export::ExportOptions::from_json(&params);
    opts.same_folder = false;
    opts.subfolder.clear();
    let items = crate::smart_sort::plan::prepare(s, &folders, &opts, dir).map_err(EngineError::Other)?;
    let files = crate::export::run_batch(
        items,
        &opts,
        &crate::export::Destination { dir: dir.into(), exact: None },
        &mut crate::export::write_file,
        &|p| std::path::Path::new(p).exists(),
        false,
        &mut |_, _| true,
    )
    .map_err(EngineError::Other)?;
    Ok(Value::Array(files))
}

pub fn specs() -> Vec<CommandSpec> {
    vec![
        cmd!(query "smartSort.tagSets", "Smart Sort Tag Sets", [], None, "{} → [{name, tags, builtin}]", always, tag_sets),
        cmd!("smartSort.saveTagSet", "Save Tag Set", [], None, "{name, tags}", always, save_tag_set),
        cmd!("smartSort.renameTagSet", "Rename Tag Set", [], None, "{name, to}", always, rename_tag_set),
        cmd!("smartSort.deleteTagSet", "Delete Tag Set", [], None, "{name}", always, delete_tag_set),
        cmd!(
            "smartSort.export",
            "Export Smart Sort",
            [],
            None,
            "{dir, folders, ids?, firstMatch?, unsorted?, preset? | export params}",
            always,
            export
        ),
        cmd!(query "smartSort.status","Smart Sort Status",[],None,"{} → {tagger, faces, analysed, total}",always,status),
        cmd!(query "smartSort.presets","Smart Sort Presets",[],None,"{} → [{name, builtin, preset}]",always,list),
        cmd!("smartSort.savePreset", "Save Smart Sort Preset", [], None, "{name?, preset: SortPreset}", always, save),
        cmd!("smartSort.deletePreset", "Delete Smart Sort Preset", [], None, "{name}", always, delete),
        cmd!(
            "smartSort.analyze",
            "Analyse Smart Sort",
            [],
            None,
            "{ids?, faces?: bool, includeRejected?: false} → {analysed, skipped, videos, failed, faces?}",
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
        cmd!(query "smartSort.plan","Plan Smart Sort Folders",[],None,"{folders: [FolderDef], ids?, firstMatch?: false, unsorted?: string|null, includeRejected?: false} → {folders: [{name, ids}], notices: [string]}",always,plan),
    ]
}
