//! Small reusable tag lists live beside sort presets, using the library's atomic store.
use super::builtin_presets;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct TagSet {
    pub name: String,
    pub tags: Vec<String>,
}

pub fn builtins() -> Vec<TagSet> {
    builtin_presets()
        .into_iter()
        .flat_map(|p| p.categories.into_iter().map(move |c| TagSet { name: format!("{} / {}", p.name, c.name), tags: c.prompts }))
        .filter(|s| !s.tags.is_empty())
        .collect()
}
