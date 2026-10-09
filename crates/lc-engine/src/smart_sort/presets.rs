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
    // Reserved data only in Phase 1; face inference/gating belongs to Phase 3.
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
                (
                    "Speakers",
                    &["a person speaking at a podium on stage", "a presenter giving a talk with a microphone", "a panel discussion on a stage"],
                ),
                ("Audience reactions", &["an audience applauding", "audience members laughing in their seats", "a crowd of people at a conference"]),
                ("Candids & networking", &["people talking and networking at an event", "a candid photo of people having a conversation"]),
                (
                    "Sponsors & exhibitors",
                    &["a sponsor booth with logo banners", "an exhibition stand at a trade show", "a branded backdrop with company logos"],
                ),
                ("Group photos", &["a posed group photo of people smiling at the camera"]),
                (
                    "Venue & details",
                    &[
                        "an empty conference room with rows of chairs",
                        "name badges, signs and event decorations",
                        "food and drinks on a catering table",
                    ],
                ),
            ],
        ),
        preset(
            "Wedding",
            &[
                ("Getting ready", &["a bride or groom getting ready for a wedding", "wedding makeup and dressing"]),
                ("Ceremony", &["a wedding ceremony", "a couple exchanging wedding vows"]),
                ("Couple portraits", &["a posed portrait of a wedding couple"]),
                ("Family & group formals", &["a posed wedding family group photo"]),
                ("Speeches & toasts", &["a wedding speech with a microphone", "people raising glasses for a wedding toast"]),
                ("Reception & dancing", &["people dancing at a wedding reception"]),
                ("Details", &["wedding rings, flowers, cake and table decorations"]),
            ],
        ),
        preset(
            "Sports",
            &[
                ("Action", &["athletes competing in a sports game"]),
                ("Celebrations", &["athletes celebrating a victory"]),
                ("Fans & crowd", &["fans cheering in a sports crowd"]),
                ("Team & portraits", &["a posed sports team photo", "a portrait of an athlete"]),
                ("Venue", &["an empty stadium or sports field"]),
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
        let mut names = std::collections::BTreeSet::new();
        for c in &self.categories {
            if c.name.trim().is_empty() || c.name.contains('|') || c.name.eq_ignore_ascii_case("Unsorted") {
                return Err("invalid category name".into());
            }
            if !names.insert(c.name.to_lowercase()) {
                return Err("duplicate category name".into());
            }
        }
        Ok(())
    }
}
