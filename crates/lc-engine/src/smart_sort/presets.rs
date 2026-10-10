//! Editable descriptions are data: photographers can adapt a preset to their own event.

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Sensitivity {
    #[serde(alias = "Strict")]
    Strict,
    #[default]
    #[serde(alias = "Balanced")]
    Balanced,
    #[serde(alias = "Loose")]
    Loose,
}

impl Sensitivity {
    pub fn threshold(self) -> f32 {
        match self {
            Self::Strict => 0.60,
            Self::Balanced => 0.45,
            Self::Loose => 0.30,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct SortPreset {
    pub name: String,
    pub keyword_parent: String,
    pub categories: Vec<Category>,
    pub sensitivity: Sensitivity,
    pub multi: bool,
    pub first_match: bool,
    pub folders: Vec<super::FolderDef>,
    pub folder_pattern: String,
    pub event_name: String,
    pub file_pattern: String,
    pub sessions: super::sessions::SessionSettings,
    pub bursts: super::bursts::BurstSettings,
    /// People layout stores library identities, never names. Unknown IDs are skipped on reuse.
    pub people_layout: Vec<super::FolderDef>,
}

impl Default for SortPreset {
    fn default() -> Self {
        Self {
            name: "Custom".into(),
            keyword_parent: "Smart Sort".into(),
            categories: Vec::new(),
            sensitivity: Sensitivity::Balanced,
            multi: false,
            first_match: false,
            folders: Vec::new(),
            folder_pattern: super::tokens::DEFAULT_FOLDER_PATTERN.into(),
            event_name: String::new(),
            file_pattern: String::new(),
            sessions: Default::default(),
            bursts: Default::default(),
            people_layout: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct Category {
    pub name: String,
    pub prompts: Vec<String>,
    pub exemplars: Vec<String>,
    pub match_all: bool,
    /// Optional face-count gates, applied only when face analysis is enabled.
    pub min_faces: Option<u32>,
    pub max_faces: Option<u32>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct SmartSortPrefs {
    pub presets: Vec<SortPreset>,
    pub last: Option<SortPreset>,
    pub faces_enabled: bool,
}

fn preset(name: &str, categories: &[(&str, &[&str])]) -> SortPreset {
    SortPreset {
        name: name.into(),
        categories: categories
            .iter()
            .map(|(name, prompts)| Category { name: (*name).into(), prompts: prompts.iter().map(|s| (*s).into()).collect(), ..Default::default() })
            .collect(),
        ..Default::default()
    }
}

pub fn builtin_presets() -> Vec<SortPreset> {
    vec![
        preset(
            "Conference",
            &[
                ("Speakers", &["speaker on stage", "presenter with microphone", "podium", "panel discussion"]),
                ("Audience reactions", &["audience applauding", "audience laughing", "conference crowd"]),
                ("Candids & networking", &["event networking", "candid conversation"]),
                ("Sponsors & exhibitors", &["sponsor booth", "exhibition stand", "company logo backdrop"]),
                ("Group photos", &["posed group photo"]),
                ("Venue & details", &["conference room", "name badges", "catering table"]),
            ],
        ),
        preset(
            "Wedding",
            &[
                ("Getting ready", &["bride getting ready", "wedding makeup"]),
                ("Ceremony", &["wedding ceremony", "wedding vows"]),
                ("Couple portraits", &["wedding couple portrait"]),
                ("Family & group formals", &["wedding family group"]),
                ("Speeches & toasts", &["wedding speech", "wedding toast"]),
                ("Reception & dancing", &["reception dancing"]),
                ("Details", &["wedding rings", "flowers", "wedding cake", "table decorations"]),
            ],
        ),
        preset(
            "Sports",
            &[
                ("Action", &["sports action"]),
                ("Celebrations", &["victory celebration"]),
                ("Fans & crowd", &["cheering fans"]),
                ("Team & portraits", &["sports team", "athlete portrait"]),
                ("Venue", &["stadium"]),
            ],
        ),
        preset("Custom", &[("Folder 1", &[]), ("Folder 2", &[])]),
    ]
}

impl SortPreset {
    /// Validate names before they become keyword paths or score-map keys. Folder path cleaning
    /// is shared with plans; prompts remain exactly as entered for the model.
    pub fn validate(&self) -> Result<(), String> {
        if self.name.trim().is_empty() {
            return Err("preset name is empty".into());
        }
        if lightcraft_catalog::keywords::clean(&self.keyword_parent).is_empty() {
            return Err("keyword parent is empty".into());
        }
        super::tokens::validate(&self.folder_pattern, false)?;
        super::tokens::validate(&self.file_pattern, true)?;
        let mut names = std::collections::BTreeSet::new();
        for c in &self.categories {
            if c.name.trim().is_empty() || c.name.contains('|') || c.name.trim().eq_ignore_ascii_case("Unsorted") {
                return Err("invalid category name".into());
            }
            if !names.insert(c.name.trim().to_lowercase()) {
                return Err("duplicate category name".into());
            }
        }
        Ok(())
    }
}
