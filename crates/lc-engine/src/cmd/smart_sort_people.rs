//! JSON engine surface for the dialog, CLI and automation. All operations accept local person
//! IDs; face vectors and centroids never appear in responses. Queries do not run inference.

use super::{CommandSpec, always, bad, cmd};
use crate::smart_sort::{
    FolderDef,
    people::{self, FaceKey, NameSuggestion, PeopleData},
};
use crate::{EngineError, Result, Session};
use lightcraft_catalog::{Flag, PhotoId};
use serde_json::{Value, json};
use std::collections::BTreeSet;

fn ready(s: &mut Session) -> Result<String> {
    s.require_faces()?;
    s.smart.people.ready().map_err(EngineError::Other)
}
fn number(p: &Value, key: &str) -> Result<u64> {
    p.get(key).and_then(Value::as_u64).ok_or_else(|| bad("smartSort.people", format!("missing {key}")))
}
fn name(p: &Value) -> Result<String> {
    p.get("name")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|n| !n.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| bad("smartSort.people", "missing name"))
}
fn source(s: &mut Session, p: &Value) -> Result<Vec<PhotoId>> {
    let ids = if let Some(ids) = p.get("ids") {
        serde_json::from_value(ids.clone()).map_err(|_| bad("smartSort.facesAnalyze", "invalid ids"))?
    } else if s.selection.ids.len() > 1 {
        s.selection.ids.clone()
    } else {
        s.visible_cloned()
    };
    let mut seen = BTreeSet::new();
    let rejected = p["includeRejected"].as_bool().unwrap_or(false);
    Ok(ids.into_iter().filter(|id| seen.insert(*id) && s.catalog.photo(*id).is_some_and(|p| rejected || p.flag != Flag::Reject)).collect())
}
fn face_key(s: &Session, p: &Value) -> Result<FaceKey> {
    if let Some(key) = p.get("key").and_then(Value::as_str) {
        return Ok(FaceKey(key.into(), number(p, "face")? as usize));
    }
    let id = PhotoId(number(p, "photo")?);
    let photo = s.catalog.photo(id).ok_or_else(|| bad("smartSort.people", "unknown photo"))?;
    Ok(FaceKey(crate::media::content_key(photo), number(p, "face")? as usize))
}
fn valid_face(s: &Session, p: &Value) -> Result<FaceKey> {
    let key = face_key(s, p)?;
    if people::face_at(&s.smart.people, &key).is_none() {
        return Err(bad("smartSort.people", "unknown face"));
    }
    Ok(key)
}

pub(super) fn status_value(s: &mut Session) -> Result<Value> {
    let enabled = s.smart.prefs.faces_enabled;
    if enabled {
        s.smart.people.ready().map_err(EngineError::Other)?;
    }
    let spec = li_seg::Group::Faces.official();
    let injected = s.smart.people.tagger.as_ref().is_some_and(|t| t.model_id() != people::FACE_MODEL);
    let missing: Vec<_> = if injected {
        Vec::new()
    } else {
        spec.files().filter(|f| s.quick_seg_dir.as_ref().is_none_or(|d| !li_seg::file_installed(d, *f))).map(|f| f.file).collect()
    };
    Ok(
        json!({"id":s.smart.people.model_id(),"enabled":enabled,"installed":injected||missing.is_empty(),"bytes":if injected{0}else{spec.download_bytes()},"missing":missing,
        "analysed":if enabled{s.smart.people.store.len(s.smart.people.model_id())}else{0}}),
    )
}
fn enable(s: &mut Session, p: &Value) -> Result<Value> {
    let enabled = p["enabled"].as_bool().ok_or_else(|| bad("smartSort.facesEnable", "missing enabled"))?;
    let previous = s.smart.prefs.faces_enabled;
    s.smart.prefs.faces_enabled = enabled;
    if let Err(e) = s.save_prefs() {
        s.smart.prefs.faces_enabled = previous;
        return Err(e);
    }
    if !enabled {
        s.smart.people.tagger = None;
    }
    status_value(s)
}
pub(super) fn analyze(s: &mut Session, p: &Value) -> Result<Value> {
    ready(s)?;
    let ids = source(s, p)?;
    let result = people::analyze(s, &ids, &std::sync::atomic::AtomicBool::new(false), &|_, _| {})?;
    serde_json::to_value(result).map_err(|e| EngineError::Other(e.to_string()))
}
fn faces(s: &mut Session, p: &Value) -> Result<Value> {
    let model = ready(s)?;
    let id = PhotoId(number(p, "photo")?);
    let photo = s.catalog.photo(id).ok_or_else(|| bad("smartSort.facesInPhoto", "unknown photo"))?;
    let key = crate::media::content_key(photo);
    let stored = s.smart.people.store.get(&model, &key);
    let rows: Vec<_> = stored.unwrap_or(&[]).iter().enumerate().map(|(i, f)| json!({"photo":id,"face":i,"rect":f.rect,"score":f.score})).collect();
    Ok(json!({"photo":id,"analysed":stored.is_some(),"faces":rows}))
}
fn list(s: &mut Session, p: &Value) -> Result<Value> {
    ready(s)?;
    let requested_min = p
        .get("minPhotos")
        .map(|n| n.as_u64().filter(|n| (1..=10).contains(n)).ok_or_else(|| bad("smartSort.people", "minPhotos must be 1..=10")))
        .transpose()?;
    let folder: Option<BTreeSet<PhotoId>> =
        p.get("folderIds").map(|v| serde_json::from_value(v.clone()).map_err(|_| bad("smartSort.people", "invalid folderIds"))).transpose()?;
    let mut data = s.smart.people.data.clone();
    people::refresh(&s.catalog, &s.smart.people, &mut data, true).map_err(EngineError::Other)?;
    s.commit_people("Smart Sort: suggest people", data)?;
    if let Some(n) = requested_min {
        let mut data = s.smart.people.data.clone();
        data.min_photos = n as u8;
        s.commit_people("Smart Sort: people filter", data)?;
    }
    let rows = people::bubbles(&s.catalog, &s.smart.people, folder.as_ref()).map_err(EngineError::Other)?;
    let show = p["showEveryone"].as_bool().unwrap_or(false);
    let min = s.smart.people.data.min_photos as usize;
    let mut visible = Vec::new();
    let mut ignored = Vec::new();
    let mut hidden = 0;
    for row in rows {
        if row.ignored {
            ignored.push(row);
        } else if show || row.count >= min || row.pinned || !s.smart.people.data.person(row.id).map_err(EngineError::Other)?.seed_faces.is_empty() {
            visible.push(row);
        } else {
            hidden += 1;
        }
    }
    let total = visible.len();
    let limit = p["limit"].as_u64().unwrap_or(60) as usize;
    visible.truncate(limit);
    let suggestions: Vec<_> = visible.iter().filter(|p| p.name.is_empty()).map(|p| json!({"cluster":p.id,"count":p.count,"cover":p.cover})).collect();
    Ok(
        json!({"people":visible,"suggestions":suggestions,"ignored":ignored,"hidden":hidden,"remaining":total.saturating_sub(limit),"minPhotos":min,"nameSuggestions":s.smart.people.data.name_suggestions}),
    )
}
fn find(s: &mut Session, p: &Value) -> Result<Value> {
    ready(s)?;
    let mut data = s.smart.people.data.clone();
    let keys = p
        .get("faces")
        .map(|faces| {
            let faces = faces.as_array().filter(|f| !f.is_empty()).ok_or_else(|| bad("smartSort.findPerson", "faces must be nonempty"))?;
            faces.iter().map(|f| valid_face(s, f)).collect::<Result<Vec<_>>>()
        })
        .transpose()?;
    let id = if let Some(id) = p["person"].as_u64() {
        data.person(id).map_err(EngineError::Other)?;
        id
    } else {
        let keys = keys.as_ref().ok_or_else(|| bad("smartSort.findPerson", "missing faces"))?;
        match data
            .people
            .iter()
            .find(|person| {
                keys.iter().any(|k| person.cluster_faces.contains(k) || person.confirmed_faces.contains(k) || person.seed_faces.contains(k))
            })
            .map(|p| p.id)
        {
            Some(id) => id,
            None => data.add(name(p)?),
        }
    };
    if let Some(keys) = keys {
        for key in keys {
            confirm(&mut data, id, key)?;
        }
        people::update_centroids(&mut data, &s.smart.people);
    }
    if let Some(n) = p.get("name").and_then(Value::as_str).map(str::trim).filter(|n| !n.is_empty()) {
        data.person_mut(id).map_err(EngineError::Other)?.name = n.into();
    }
    s.commit_people("Smart Sort: find person", data)?;
    ranked(s, id, p)
}
fn ranked(s: &Session, id: u64, p: &Value) -> Result<Value> {
    let mut matches = people::matches(&s.catalog, &s.smart.people, &s.smart.people.data, id).map_err(EngineError::Other)?;
    if p["queue"].as_bool().unwrap_or(false) {
        matches.retain(|r| !r.confirmed);
    }
    if p["leastSureFirst"].as_bool().unwrap_or(p["queue"].as_bool().unwrap_or(false)) {
        matches.sort_by(|a, b| a.similarity.total_cmp(&b.similarity).then(a.photo.cmp(&b.photo)).then(a.face.cmp(&b.face)));
    }
    let same = people::same_people(&s.smart.people.data, id).map_err(EngineError::Other)?;
    Ok(
        json!({"person":id,"matches":matches,"probablySame":same.into_iter().map(|(person,similarity)|json!({"person":person,"similarity":similarity})).collect::<Vec<_>>()}),
    )
}
fn confirm(data: &mut PeopleData, id: u64, key: FaceKey) -> Result<()> {
    if data.person(id).map_err(EngineError::Other)?.rejected_faces.contains(&key) {
        return Err(bad("smartSort.confirmFace", "face was rejected for this person"));
    }
    for person in &mut data.people {
        if person.id != id {
            person.confirmed_faces.retain(|k| k != &key);
            person.cluster_faces.retain(|k| k != &key);
            person.seed_faces.retain(|k| k != &key);
        }
    }
    let p = data.person_mut(id).map_err(EngineError::Other)?;
    if !p.confirmed_faces.contains(&key) {
        p.confirmed_faces.push(key);
    }
    Ok(())
}
fn confirm_face(s: &mut Session, p: &Value) -> Result<Value> {
    ready(s)?;
    let id = number(p, "person")?;
    let key = valid_face(s, p)?;
    let mut data = s.smart.people.data.clone();
    confirm(&mut data, id, key)?;
    people::update_centroids(&mut data, &s.smart.people);
    s.commit_people("Smart Sort: confirm face", data)?;
    ranked(s, id, &json!({"queue":true}))
}
fn reject_face(s: &mut Session, p: &Value) -> Result<Value> {
    ready(s)?;
    let id = number(p, "person")?;
    let key = valid_face(s, p)?;
    let mut data = s.smart.people.data.clone();
    let person = data.person_mut(id).map_err(EngineError::Other)?;
    person.confirmed_faces.retain(|k| k != &key);
    person.seed_faces.retain(|k| k != &key);
    person.cluster_faces.retain(|k| k != &key);
    if !person.rejected_faces.contains(&key) {
        person.rejected_faces.push(key);
    }
    people::update_centroids(&mut data, &s.smart.people);
    s.commit_people("Smart Sort: reject face", data)?;
    ranked(s, id, &json!({"queue":true}))
}
fn name_person(s: &mut Session, p: &Value) -> Result<Value> {
    ready(s)?;
    let id = p["person"].as_u64().or_else(|| p["cluster"].as_u64()).ok_or_else(|| bad("smartSort.namePerson", "missing person or cluster"))?;
    let mut data = s.smart.people.data.clone();
    data.person_mut(id).map_err(EngineError::Other)?.name = name(p)?;
    s.commit_people("Smart Sort: name person", data)?;
    Ok(json!({"person":id}))
}
fn pin(s: &mut Session, p: &Value) -> Result<Value> {
    flag(s, p, "pinned")
}
fn ignore(s: &mut Session, p: &Value) -> Result<Value> {
    flag(s, p, "ignored")
}
fn folder(s: &mut Session, p: &Value) -> Result<Value> {
    flag(s, p, "folder")
}
fn flag(s: &mut Session, p: &Value, field: &str) -> Result<Value> {
    ready(s)?;
    let id = number(p, "person")?;
    let value = p["enabled"].as_bool().unwrap_or(true);
    let mut data = s.smart.people.data.clone();
    let person = data.person_mut(id).map_err(EngineError::Other)?;
    match field {
        "pinned" => person.pinned = value,
        "ignored" => person.ignored = value,
        _ => person.folder_enabled = value,
    }
    s.commit_people("Smart Sort: people options", data)?;
    let folders: Vec<_> = s
        .smart
        .people
        .data
        .people
        .iter()
        .filter(|p| p.folder_enabled && !p.ignored)
        .map(|p| FolderDef {
            name: if p.name.is_empty() { format!("Person {}", p.id) } else { p.name.clone() },
            people_enabled: true,
            person_ids: vec![p.id],
            use_rules: false,
            ..Default::default()
        })
        .collect();
    Ok(json!({"person":id,"folders":folders}))
}
fn merge(s: &mut Session, p: &Value) -> Result<Value> {
    ready(s)?;
    let ids: Vec<u64> = serde_json::from_value(p.get("ids").cloned().ok_or_else(|| bad("smartSort.mergePeople", "missing ids"))?)
        .map_err(|_| bad("smartSort.mergePeople", "invalid ids"))?;
    if ids.len() < 2 || ids.iter().collect::<BTreeSet<_>>().len() != ids.len() {
        return Err(bad("smartSort.mergePeople", "choose at least two distinct people"));
    }
    let mut data = s.smart.people.data.clone();
    for id in &ids {
        data.person(*id).map_err(EngineError::Other)?;
    }
    let target = ids[0];
    let others: Vec<_> = data.people.iter().filter(|p| ids[1..].contains(&p.id)).cloned().collect();
    let person = data.person_mut(target).map_err(EngineError::Other)?;
    for other in others {
        if person.name.is_empty() {
            person.name = other.name;
        }
        person.pinned |= other.pinned;
        person.folder_enabled |= other.folder_enabled;
        person.seed_faces.extend(other.seed_faces);
        person.confirmed_faces.extend(other.confirmed_faces);
        person.cluster_faces.extend(other.cluster_faces);
        person.rejected_faces.extend(other.rejected_faces);
    }
    for refs in [&mut person.seed_faces, &mut person.confirmed_faces, &mut person.cluster_faces, &mut person.rejected_faces] {
        refs.sort();
        refs.dedup();
    }
    person.seed_faces.retain(|k| !person.rejected_faces.contains(k));
    person.confirmed_faces.retain(|k| !person.rejected_faces.contains(k));
    person.cluster_faces.retain(|k| !person.rejected_faces.contains(k));
    data.people.retain(|p| !ids[1..].contains(&p.id));
    for replacement in data.merged_ids.values_mut() {
        if ids[1..].contains(replacement) {
            *replacement = target;
        }
    }
    for old in &ids[1..] {
        data.merged_ids.insert(*old, target);
    }
    for pair in &mut data.different {
        for id in pair.iter_mut() {
            if ids.contains(id) {
                *id = target;
            }
        }
        pair.sort_unstable();
    }
    data.different.retain(|pair| pair[0] != pair[1]);
    data.different.sort_unstable();
    data.different.dedup();
    people::update_centroids(&mut data, &s.smart.people);
    s.commit_people("Smart Sort: merge people", data)?;
    Ok(json!({"person":target}))
}
fn split(s: &mut Session, p: &Value) -> Result<Value> {
    ready(s)?;
    let id = number(p, "person")?;
    let rows = p["faces"].as_array().filter(|r| !r.is_empty()).ok_or_else(|| bad("smartSort.splitPerson", "missing faces"))?;
    let keys = rows.iter().map(|r| valid_face(s, r)).collect::<Result<Vec<_>>>()?;
    let current = people::matches(&s.catalog, &s.smart.people, &s.smart.people.data, id).map_err(EngineError::Other)?;
    let existing: BTreeSet<_> =
        current.iter().filter_map(|r| s.catalog.photo(r.photo).map(|p| FaceKey(crate::media::content_key(p), r.face))).collect();
    let sure_remainder: BTreeSet<_> =
        current.iter().filter(|r| r.sure).filter_map(|r| s.catalog.photo(r.photo).map(|p| FaceKey(crate::media::content_key(p), r.face))).collect();
    if keys.iter().any(|k| !existing.contains(k)) {
        return Err(bad("smartSort.splitPerson", "face does not belong to this person"));
    }
    let mut data = s.smart.people.data.clone();
    let new = data.add(p["name"].as_str().unwrap_or("").trim().into());
    for key in keys {
        let person = data.person_mut(id).map_err(EngineError::Other)?;
        person.seed_faces.retain(|k| k != &key);
        person.confirmed_faces.retain(|k| k != &key);
        person.cluster_faces.retain(|k| k != &key);
        if !person.rejected_faces.contains(&key) {
            person.rejected_faces.push(key.clone());
        }
        confirm(&mut data, new, key)?;
    }
    // Preserve accepted remainder when identical prototypes erase the margin. Review-only
    // matches still require an explicit Yes before entering a person folder.
    for key in sure_remainder {
        if !data.person(id).map_err(EngineError::Other)?.rejected_faces.contains(&key) {
            confirm(&mut data, id, key)?;
        }
    }
    data.different.push([id, new]);
    people::update_centroids(&mut data, &s.smart.people);
    s.commit_people("Smart Sort: split person", data)?;
    Ok(json!({"person":id,"newPerson":new}))
}
fn different(s: &mut Session, p: &Value) -> Result<Value> {
    ready(s)?;
    let a = number(p, "person")?;
    let b = number(p, "other")?;
    let mut data = s.smart.people.data.clone();
    data.person(a).map_err(EngineError::Other)?;
    data.person(b).map_err(EngineError::Other)?;
    if a == b {
        return Err(bad("smartSort.peopleDifferent", "choose distinct people"));
    }
    if !data.different.iter().any(|pair| pair.contains(&a) && pair.contains(&b)) {
        data.different.push([a, b]);
    }
    s.commit_people("Smart Sort: different people", data)?;
    ranked(s, a, &json!({"queue":true}))
}
fn names(s: &mut Session, p: &Value) -> Result<Value> {
    ready(s)?;
    let text = p["text"].as_str().ok_or_else(|| bad("smartSort.peoplePasteNames", "missing text"))?;
    let mut data = s.smart.people.data.clone();
    for line in text.lines().map(str::trim).filter(|l| !l.is_empty()) {
        let (name, title) = line.split_once(',').unwrap_or((line, ""));
        let suggestion = NameSuggestion { name: name.trim().into(), title: title.trim().into() };
        if !suggestion.name.is_empty() && !data.name_suggestions.iter().any(|n| n.name.eq_ignore_ascii_case(&suggestion.name)) {
            data.name_suggestions.push(suggestion);
        }
    }
    s.commit_people("Smart Sort: name suggestions", data)?;
    Ok(json!({"names":s.smart.people.data.name_suggestions}))
}
fn headshots(s: &mut Session, p: &Value) -> Result<Value> {
    ready(s)?;
    let dir = p["dir"].as_str().ok_or_else(|| bad("smartSort.peopleSeedHeadshots", "missing dir"))?;
    let tagger = s.smart.people.tagger(s.quick_seg_dir.as_deref()).map_err(EngineError::Other)?;
    let model = s.smart.people.ready().map_err(EngineError::Other)?;
    let mut paths = std::fs::read_dir(dir)
        .map_err(|_| bad("smartSort.peopleSeedHeadshots", "could not read folder"))?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.is_file())
        .collect::<Vec<_>>();
    paths.sort();
    let mut data = s.smart.people.data.clone();
    let mut added = Vec::new();
    let mut skipped = Vec::new();
    for path in paths {
        let label = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let row = (|| -> std::result::Result<(String, Vec<crate::smart_sort::faces_store::StoredFace>), String> {
            let bytes = std::fs::read(&path).map_err(|_| "could not read image")?;
            let thumb = lightcraft_codecs::decode_thumbnail(&bytes, 1024).map_err(|_| "could not decode image")?;
            let oriented = thumb.image.into_oriented(lightcraft_geom::Orientation::from_exif(thumb.orientation));
            let faces = tagger.faces(&oriented)?;
            if faces.len() != 1 {
                return Err("expected exactly one face".into());
            }
            Ok((format!("headshot:{}:{}", lightcraft_preview::hash_bytes(&bytes), lightcraft_preview::hash_bytes(label.as_bytes())), faces))
        })();
        match row {
            Ok((key, faces)) => {
                let fk = FaceKey(key.clone(), 0);
                if data.people.iter().any(|p| p.seed_faces.contains(&fk)) {
                    continue;
                }
                data.seed_sources.insert(key.clone(), path.to_string_lossy().into_owned());
                s.smart.people.store.insert(&model, key, faces).map_err(EngineError::Other)?;
                let id = data.add(path.file_stem().map(|n| n.to_string_lossy().into_owned()).unwrap_or(label));
                data.person_mut(id).map_err(EngineError::Other)?.seed_faces.push(fk);
                added.push(id);
            }
            Err(_) => skipped.push(label),
        }
    }
    s.smart.people.store.save().map_err(|_| EngineError::Other("could not save headshot faces".into()))?;
    people::update_centroids(&mut data, &s.smart.people);
    s.commit_people("Smart Sort: seed headshots", data)?;
    Ok(json!({"people":added,"skipped":skipped}))
}
fn clear(s: &mut Session, p: &Value) -> Result<Value> {
    // Always available, including after opting out.
    s.smart.people.clear().map_err(EngineError::Other)?;
    for entry in s.undo.iter_mut().chain(&mut s.redo) {
        entry.people = None;
    }
    let remove = p["removeNames"].as_bool().unwrap_or(false);
    let mut ops = Vec::new();
    for photo in s.catalog.photos() {
        let mut meta = photo.meta.clone();
        meta.person_ids.clear();
        if remove {
            meta.regions.retain(|r| !r.auto);
        }
        if meta != photo.meta {
            ops.push(lightcraft_catalog::Op::SetMeta { id: photo.id, meta: Box::new(meta) });
        }
    }
    let changed = ops.len();
    if !ops.is_empty() {
        s.commit("Smart Sort: clear face data", lightcraft_catalog::Op::Batch { ops })?;
    }
    Ok(json!({"cleared":true,"changed":changed}))
}

/// Reusing a preset across libraries drops unknown/ignored IDs and returns a notice. A People
/// toggle with no known IDs stays an empty people condition, rather than matching everyone.
/// Inactive rows and conditions do not require face opt-in or produce unknown-ID notices.
pub(super) fn prepare_folders(s: &mut Session, folders: &mut [FolderDef]) -> Result<Vec<String>> {
    fn has_ids(rules: &lightcraft_catalog::RuleSet) -> bool {
        rules.rules.iter().any(|rule| match rule {
            lightcraft_catalog::Rule::Group { group } => has_ids(group),
            lightcraft_catalog::Rule::Field { field, value, .. } => field == "person" && (value.is_number() || value.is_array()),
        })
    }
    if !folders.iter().any(|f| f.enabled && !f.unsorted && (f.people_enabled || (f.use_rules && has_ids(&f.rules)))) {
        return Ok(Vec::new());
    }
    ready(s)?;
    let data = &s.smart.people.data;
    let known: BTreeSet<_> = data.people.iter().filter(|p| !p.ignored).map(|p| p.id).collect();
    let mut missing = BTreeSet::new();
    let mut resolve = |ids: &mut Vec<u64>| {
        ids.retain_mut(|id| {
            let original = *id;
            *id = data.resolve_id(*id);
            if known.contains(id) {
                true
            } else {
                missing.insert(original);
                false
            }
        });
        ids.sort_unstable();
        ids.dedup();
    };
    fn fix(rules: &mut lightcraft_catalog::RuleSet, resolve: &mut dyn FnMut(&mut Vec<u64>)) {
        for rule in &mut rules.rules {
            match rule {
                lightcraft_catalog::Rule::Group { group } => fix(group, resolve),
                lightcraft_catalog::Rule::Field { field, value, .. } if field == "person" && (value.is_number() || value.is_array()) => {
                    let mut ids: Vec<_> = match value {
                        Value::Array(v) => v.iter().filter_map(Value::as_u64).collect(),
                        _ => value.as_u64().into_iter().collect(),
                    };
                    resolve(&mut ids);
                    *value = json!(ids);
                }
                _ => {}
            }
        }
    }
    for folder in folders.iter_mut().filter(|f| f.enabled && !f.unsorted) {
        if folder.people_enabled {
            resolve(&mut folder.person_ids);
        }
        if folder.use_rules {
            fix(&mut folder.rules, &mut resolve);
        }
    }
    Ok(missing.into_iter().map(|id| format!("Skipped unknown or ignored person {id}")).collect())
}

pub fn specs() -> Vec<CommandSpec> {
    vec![
        cmd!(query "smartSort.facesStatus","Face Analysis Status",[],None,"{} → {id, enabled, installed, missing, analysed}",always,|s,_|status_value(s)),
        cmd!("smartSort.facesEnable", "Enable Face Recognition", [], None, "{enabled: bool}; opt-in per library", always, enable),
        cmd!("smartSort.facesAnalyze", "Analyse Faces", [], None, "{ids?, includeRejected?} → {analysed, skipped, videos, failed}", always, analyze),
        cmd!(query "smartSort.facesInPhoto","Faces in Photo",[],None,"{photo} → {analysed, faces: [{photo, face, rect, score}]}",always,faces),
        cmd!(
            "smartSort.people",
            "People Bubbles",
            [],
            None,
            "{minPhotos?: 3, showEveryone?, limit?: 60, folderIds?} → {people, hidden, ignored, remaining}",
            always,
            list
        ),
        cmd!(
            "smartSort.findPerson",
            "Find This Person",
            [],
            None,
            "{person? | name, faces?: [{photo, face}], queue?, leastSureFirst?} → {person, matches, probablySame}",
            always,
            find
        ),
        cmd!("smartSort.namePerson", "Name Person", [], None, "{person? | cluster?, name}; one undo step", always, name_person),
        cmd!("smartSort.confirmFace", "Confirm Person Match", [], None, "{person, photo, face} → re-ranked queue", always, confirm_face),
        cmd!(
            "smartSort.rejectFace",
            "Reject Person Match",
            [],
            None,
            "{person, photo, face}; permanent exclusion unless undone",
            always,
            reject_face
        ),
        cmd!("smartSort.mergePeople", "Merge People", [], None, "{ids: [person, ...]}; undoable", always, merge),
        cmd!("smartSort.splitPerson", "Split Person", [], None, "{person, faces: [{photo, face}], name?}; undoable", always, split),
        cmd!("smartSort.peopleDifferent", "Different People", [], None, "{person, other}; suppress merge suggestion", always, different),
        cmd!("smartSort.peoplePin", "Pin Person", [], None, "{person, enabled?: true}", always, pin),
        cmd!("smartSort.peopleIgnore", "Ignore or Restore Person", [], None, "{person, enabled?: true}", always, ignore),
        cmd!("smartSort.personFolder", "Person Folder", [], None, "{person, enabled}; folders opt-in, off by default", always, folder),
        cmd!("smartSort.peoplePasteNames", "Paste Person Names", [], None, "{text: name[,title] lines} → {names}", always, names),
        cmd!("smartSort.peopleSeedHeadshots", "Seed People from Photos", [], None, "{dir} → {people: [id], skipped: [file]}", always, headshots),
        cmd!(
            "smartSort.clearFaceData",
            "Clear Face Data",
            [],
            None,
            "{removeNames?: false}; removes all face caches, optionally only auto regions",
            always,
            clear
        ),
    ]
}
