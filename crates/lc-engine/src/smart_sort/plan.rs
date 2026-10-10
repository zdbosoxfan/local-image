//! Folder membership uses the catalog's existing smart-album rules after keywords are applied.
//! Prepared exports share the normal rendering pipeline. Names are portable, and deterministic duplicates get a suffix.

use super::SortPreset;
use lightcraft_catalog::{Catalog, PhotoId, Rule, RuleSet};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct FolderDef {
    pub name: String,
    pub rules: RuleSet,
    pub enabled: bool,
    pub unsorted: bool,
    pub custom: bool,
    /// Category names used by the simple picker. Advanced rules remain authoritative.
    pub tags: Vec<String>,
    pub person_ids: Vec<u64>,
    pub combine_all: bool,
    pub people_enabled: bool,
    pub everyone: bool,
    /// false: tag/rules AND people; true: tag/rules OR people.
    pub people_or: bool,
    /// When false, this is a pure people folder.
    pub use_rules: bool,
    /// Session start timestamp, or `no-time`. Names remain editable.
    pub session: Option<String>,
    pub session_only: bool,
}
impl Default for FolderDef {
    fn default() -> Self {
        Self {
            name: "Untitled".into(),
            rules: RuleSet::default(),
            enabled: true,
            unsorted: false,
            custom: false,
            tags: Vec::new(),
            person_ids: Vec::new(),
            combine_all: false,
            people_enabled: false,
            everyone: false,
            people_or: false,
            use_rules: true,
            session: None,
            session_only: false,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Folder {
    pub name: String,
    pub ids: Vec<PhotoId>,
}

pub fn keyword_folders(preset: &SortPreset) -> Vec<FolderDef> {
    preset
        .categories
        .iter()
        .map(|c| FolderDef {
            name: c.name.clone(),
            rules: RuleSet {
                rules: vec![Rule::Field {
                    field: "keywords".into(),
                    op: "is".into(),
                    value: format!("{}|{}", lightcraft_catalog::keywords::clean(&preset.keyword_parent), c.name.trim()).into(),
                }],
                ..Default::default()
            },
            tags: vec![c.name.clone()],
            ..Default::default()
        })
        .collect()
}

pub fn sanitize(name: &str) -> Result<String, String> {
    if name.as_bytes().get(1) == Some(&b':') || name.starts_with('/') || name.starts_with('\\') || name.split(['/', '\\']).any(|s| s == "..") {
        return Err("folder must be relative and cannot contain '..'".into());
    }
    Ok(name
        .split('/')
        .map(|part| {
            let cleaned: String = part.chars().map(|c| if c.is_control() || "\\:*?\"<>|".contains(c) { '_' } else { c }).collect();
            let trimmed = cleaned.trim_matches(|c: char| c == '.' || c.is_whitespace());
            if trimmed.is_empty() { "Untitled".into() } else { trimmed.into() }
        })
        .collect::<Vec<String>>()
        .join("/"))
}

/// The simple picker compiles to the same exact-keyword rules used by smart albums.
pub fn tag_rules(parent: &str, tags: &[String], all: bool) -> RuleSet {
    RuleSet {
        mode: if all && !tags.is_empty() { lightcraft_catalog::Match::All } else { lightcraft_catalog::Match::Any },
        rules: tags
            .iter()
            .map(|tag| Rule::Field {
                field: "keywords".into(),
                op: "is".into(),
                value: format!("{}|{}", lightcraft_catalog::keywords::clean(parent), tag.trim()).into(),
            })
            .collect(),
    }
}

pub fn default_folders(preset: &SortPreset) -> Vec<FolderDef> {
    let mut folders = keyword_folders(preset);
    folders.push(FolderDef { name: "Unsorted".into(), enabled: false, unsorted: true, ..Default::default() });
    folders
}

pub fn plan(cat: &Catalog, ids: &[PhotoId], folders: &[FolderDef]) -> Result<Vec<Folder>, String> {
    plan_with_options(cat, ids, folders, false, None)
}

pub fn plan_with_options(
    cat: &Catalog,
    ids: &[PhotoId],
    folders: &[FolderDef],
    first_match: bool,
    unsorted: Option<&str>,
) -> Result<Vec<Folder>, String> {
    plan_with_sessions(cat, ids, folders, first_match, unsorted, None, false)
}

fn plan_with_sessions(
    cat: &Catalog,
    ids: &[PhotoId],
    folders: &[FolderDef],
    first_match: bool,
    unsorted: Option<&str>,
    sessions: Option<&super::sessions::Sessions>,
    numbered: bool,
) -> Result<Vec<Folder>, String> {
    let mut used = std::collections::BTreeSet::new();
    let mut matched = std::collections::BTreeSet::new();
    let mut out = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    let unique_ids: Vec<_> = ids
        .iter()
        .copied()
        .filter(|id| seen.insert(*id))
        .filter(|id| cat.photo(*id).is_some_and(|p| p.kind != lightcraft_catalog::MediaKind::Video))
        .collect();
    let mut unique_name = |name: &str| -> Result<String, String> {
        let base = sanitize(name)?;
        let mut name = base.clone();
        let mut n = 2;
        while !used.insert(name.to_lowercase()) {
            name = if numbered { format!("{base} ({n})") } else { format!("{base} {n}") };
            n += 1;
        }
        Ok(name)
    };
    for folder in folders.iter().filter(|f| f.enabled && !f.unsorted) {
        let name = unique_name(&folder.name)?;
        let selected = unique_ids
            .iter()
            .filter(|id| {
                (!first_match || !matched.contains(*id))
                    && cat.photo(**id).is_some_and(|p| folder.matches(p, cat))
                    && folder.session.as_ref().is_none_or(|key| sessions.is_none_or(|s| s.key_of(**id) == key))
            })
            .copied()
            .collect::<Vec<_>>();
        matched.extend(selected.iter().copied());
        out.push(Folder { name, ids: selected });
    }
    let unsorted_def = folders.iter().find(|f| f.enabled && f.unsorted);
    let unsorted_name = unsorted_def.map(|f| f.name.as_str()).or(unsorted);
    if let Some(name) = unsorted_name {
        out.push(Folder {
            name: unique_name(name)?,
            ids: unique_ids
                .iter()
                .filter(|id| {
                    !matched.contains(*id)
                        && unsorted_def.and_then(|f| f.session.as_ref()).is_none_or(|key| sessions.is_none_or(|s| s.key_of(**id) == key))
                })
                .copied()
                .collect(),
        });
    }
    Ok(out)
}

/// Resolve existing ancestors too, so a symlink or a not-yet-created child cannot bypass guards.
fn resolved(path: &std::path::Path) -> Result<std::path::PathBuf, String> {
    use std::path::{Component, PathBuf};
    let mut absolute = if path.is_absolute() { PathBuf::new() } else { std::env::current_dir().map_err(|e| e.to_string())? };
    for part in path.components() {
        match part {
            Component::ParentDir => {
                absolute.pop();
            }
            Component::CurDir => {}
            _ => absolute.push(part),
        }
        if absolute.exists() {
            absolute = absolute.canonicalize().map_err(|e| e.to_string())?;
        }
    }
    Ok(absolute)
}

pub fn check_destination(session: &crate::Session, dir: &str) -> Result<(), String> {
    if dir.trim().is_empty() {
        return Err("choose an export destination".into());
    }
    let target = resolved(std::path::Path::new(dir))?;
    if let Some(lib) = &session.library
        && lib.on_disk
        && target.starts_with(resolved(&lib.dir)?)
    {
        return Err("Smart Sort destination must be outside the library".into());
    }
    for p in session.catalog.photos() {
        if let lightcraft_catalog::Source::File { path } = &p.source
            && let Some(parent) = std::path::Path::new(path).parent()
            && target == resolved(parent)?
        {
            return Err("Smart Sort destination cannot be an originals folder".into());
        }
    }
    session.check_write_target(dir)
}

pub fn prepare(
    session: &mut crate::Session,
    folders: &[Folder],
    options: &crate::export::ExportOptions,
    dir: &str,
) -> Result<Vec<crate::export::PreparedExport>, String> {
    check_destination(session, dir)?;
    let guard = std::sync::Arc::new(session.original_guard());
    let mut out = Vec::new();
    for folder in folders {
        let name = sanitize(&folder.name)?;
        // Reject symlinked child folders pointing back into the library/originals as well.
        check_destination(session, &std::path::Path::new(dir).join(&name).to_string_lossy())?;
        for (i, id) in folder.ids.iter().enumerate() {
            let mut item = crate::export::prepare_guarded(session, *id, options, i + 1, guard.clone())?;
            item.subfolder = Some(name.clone());
            out.push(item);
        }
    }
    Ok(out)
}

impl FolderDef {
    pub fn matches(&self, photo: &lightcraft_catalog::Photo, cat: &Catalog) -> bool {
        if self.session_only {
            return true;
        }
        let tags = self.rules.matches(photo, cat);
        if !self.people_enabled {
            return self.use_rules && tags;
        }
        let people = !self.person_ids.is_empty()
            && if self.everyone {
                self.person_ids.iter().all(|id| photo.meta.person_ids.contains(id))
            } else {
                self.person_ids.iter().any(|id| photo.meta.person_ids.contains(id))
            };
        if !self.use_rules || self.rules.rules.is_empty() {
            people
        } else if self.people_or {
            tags || people
        } else {
            tags && people
        }
    }
}

/// Session folders are independent rows; explicit rows retain their order and people rules.
pub fn folder_defs(preset: &SortPreset, sessions: &super::sessions::Sessions) -> Vec<FolderDef> {
    let mut defs = preset.folders.clone();
    if preset.sessions.enabled && preset.sessions.export_folders {
        defs.extend(sessions.sessions.iter().map(|s| FolderDef {
            name: s.name.clone(),
            session: Some(s.start.clone()),
            session_only: true,
            ..Default::default()
        }));
        if !sessions.no_time.is_empty() {
            defs.push(FolderDef { name: "No time".into(), session: Some(super::sessions::NO_TIME.into()), session_only: true, ..Default::default() });
        }
    }
    defs
}

pub fn plan_preset(cat: &Catalog, ids: &[PhotoId], preset: &SortPreset) -> Result<Vec<Folder>, String> {
    let sessions = super::sessions::split_sessions(cat, ids, &preset.sessions);
    plan_with_sessions(cat, ids, &folder_defs(preset, &sessions), preset.first_match, None, preset.sessions.enabled.then_some(&sessions), true)
}

/// Expand per-photo tokens, reserving each resolved path once per logical folder. Distinct
/// folders resolving to the same path get a numbered suffix; photos within one folder share it.
pub fn resolve_tokens(cat: &Catalog, folders: &[Folder], preset: &SortPreset, sessions: &super::sessions::Sessions) -> Result<Vec<Folder>, String> {
    Ok(resolve_groups(cat, folders, preset, sessions)?.into_iter().map(|(folder, _)| folder).collect())
}
fn resolve_groups(
    cat: &Catalog,
    folders: &[Folder],
    preset: &SortPreset,
    sessions: &super::sessions::Sessions,
) -> Result<Vec<(Folder, String)>, String> {
    use super::tokens::{self, TokenValues};
    let pattern = if preset.folder_pattern.is_empty() { "{folder}" } else { &preset.folder_pattern };
    let mut groups = Vec::<(String, std::path::PathBuf, Vec<PhotoId>)>::new();
    for (index, folder) in folders.iter().enumerate() {
        let mut paths = std::collections::BTreeMap::<std::path::PathBuf, Vec<PhotoId>>::new();
        for id in &folder.ids {
            let photo = cat.photo(*id).ok_or("unknown export photo")?;
            let values = TokenValues {
                event: preset.event_name.clone(),
                folder: folder.name.clone(),
                session: preset.sessions.enabled.then(|| sessions.name_of(*id).to_string()),
                captured: photo.captured.clone(),
                camera: Some(photo.meta.camera.clone()),
                person: photo
                    .meta
                    .regions
                    .iter()
                    .filter(|r| matches!(r.kind, lightcraft_meta::RegionKind::Face))
                    .filter_map(|r| r.name.clone())
                    .next(),
                ..Default::default()
            };
            let path = tokens::expand_folder(pattern, &values)?;
            let path = if path.as_os_str().is_empty() { "Untitled".into() } else { path };
            paths.entry(path).or_default().push(*id);
        }
        for (path, ids) in paths {
            groups.push((index.to_string(), path, ids));
        }
    }
    let paths = tokens::dedupe_paths(groups.iter().map(|(key, path, _)| (key.clone(), path.clone())).collect());
    Ok(paths
        .into_iter()
        .zip(groups)
        .map(|((key, path), (_, _, ids))| {
            (Folder { name: path.to_string_lossy().into_owned(), ids }, folders[key.parse::<usize>().unwrap()].name.clone())
        })
        .collect())
}

pub fn prepare_preset(
    session: &mut crate::Session,
    folders: &[Folder],
    options: &crate::export::ExportOptions,
    dir: &str,
    preset: &SortPreset,
    sessions: &super::sessions::Sessions,
) -> Result<Vec<crate::export::PreparedExport>, String> {
    let groups = resolve_groups(&session.catalog, folders, preset, sessions)?;
    let mut out = Vec::new();
    for (folder, original_name) in groups {
        let mut items = prepare(session, std::slice::from_ref(&folder), options, dir)?;
        if !preset.file_pattern.is_empty() {
            for (index, item) in items.iter_mut().enumerate() {
                let photo = session.catalog.photo(item.photo).ok_or("unknown export photo")?;
                let values = super::tokens::TokenValues {
                    event: preset.event_name.clone(),
                    folder: original_name.clone(),
                    session: preset.sessions.enabled.then(|| sessions.name_of(item.photo).into()),
                    captured: photo.captured.clone(),
                    camera: Some(photo.meta.camera.clone()),
                    person: photo
                        .meta
                        .regions
                        .iter()
                        .filter(|r| matches!(r.kind, lightcraft_meta::RegionKind::Face))
                        .filter_map(|r| r.name.clone())
                        .next(),
                    original: Some(std::path::Path::new(&photo.file_name).file_stem().unwrap_or_default().to_string_lossy().into_owned()),
                    seq: Some(options.start_number.saturating_add(index as u32)),
                };
                let stem = super::tokens::expand_file(&preset.file_pattern, &values)?;
                let ext = std::path::Path::new(&item.file_name).extension().unwrap_or_default().to_string_lossy();
                item.file_name = format!("{}.{}", if stem.is_empty() { "Untitled" } else { &stem }, ext);
            }
        }
        out.extend(items);
    }
    Ok(out)
}
