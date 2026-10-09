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
    /// Reserved stable ids for Phase 3's person picker.
    pub person_ids: Vec<u64>,
    pub combine_all: bool,
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
            name = format!("{base} {n}");
            n += 1;
        }
        Ok(name)
    };
    for folder in folders.iter().filter(|f| f.enabled && !f.unsorted) {
        let name = unique_name(&folder.name)?;
        let selected = unique_ids
            .iter()
            .filter(|id| (!first_match || !matched.contains(*id)) && cat.photo(**id).is_some_and(|p| folder.rules.matches(p, cat)))
            .copied()
            .collect::<Vec<_>>();
        matched.extend(selected.iter().copied());
        out.push(Folder { name, ids: selected });
    }
    let unsorted = folders.iter().find(|f| f.enabled && f.unsorted).map(|f| f.name.as_str()).or(unsorted);
    if let Some(name) = unsorted {
        out.push(Folder { name: unique_name(name)?, ids: unique_ids.iter().filter(|id| !matched.contains(*id)).copied().collect() });
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
