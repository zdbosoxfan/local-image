//! Folder membership uses the catalog's existing smart-album rules after keywords are applied.
//! No export work is done in Phase 1. Names are portable, and deterministic duplicates get a suffix.

use super::SortPreset;
use lightcraft_catalog::{Catalog, PhotoId, Rule, RuleSet};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct FolderDef {
    pub name: String,
    pub rules: RuleSet,
    pub enabled: bool,
    pub people_enabled: bool,
    pub person_ids: Vec<u64>,
    pub everyone: bool,
    /// false: tag/rules AND people; true: tag/rules OR people.
    pub people_or: bool,
    /// When false, this is a pure people folder (empty RuleSet otherwise matches everything).
    pub use_rules: bool,
}
impl Default for FolderDef {
    fn default() -> Self {
        Self {
            name: "Untitled".into(),
            rules: RuleSet::default(),
            enabled: true,
            people_enabled: false,
            person_ids: Vec::new(),
            everyone: false,
            people_or: false,
            use_rules: true,
        }
    }
}

#[derive(Debug, Serialize)]
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
                    value: format!("{}|{}", lightcraft_catalog::keywords::clean(&preset.keyword_parent), c.name).into(),
                }],
                ..Default::default()
            },
            enabled: true,
            ..Default::default()
        })
        .collect()
}

pub fn sanitize(name: &str) -> Result<String, String> {
    if name.starts_with('/') || name.starts_with('\\') || name.split(['/', '\\']).any(|s| s == "..") {
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

pub fn plan(cat: &Catalog, ids: &[PhotoId], folders: &[FolderDef]) -> Result<Vec<Folder>, String> {
    let mut used = std::collections::BTreeSet::new();
    let mut out = Vec::new();
    for folder in folders.iter().filter(|f| f.enabled) {
        let base = sanitize(&folder.name)?;
        let mut name = base.clone();
        let mut n = 2;
        while !used.insert(name.to_lowercase()) {
            name = format!("{base} {n}");
            n += 1;
        }
        let mut seen = std::collections::BTreeSet::new();
        let ids = ids.iter().filter(|id| seen.insert(**id)).filter(|id| cat.photo(**id).is_some_and(|p| folder.matches(p, cat))).copied().collect();
        out.push(Folder { name, ids });
    }
    Ok(out)
}

impl FolderDef {
    pub fn matches(&self, photo: &lightcraft_catalog::Photo, cat: &Catalog) -> bool {
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
