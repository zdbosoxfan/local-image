//! Saved generation presets (Krita AI Diffusion's "styles"): a prompt template, negative prompt,
//! sampling settings, LoRAs and a preferred model or family. Built-in presets ship with the app;
//! the user's own are kept in `<data>/presets.json`.
//!
//! A template either contains `{prompt}` (replaced with the prompt) or is appended after it.

use std::path::{Path, PathBuf};

use anyhow::Result;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PresetLora {
    pub name: String,
    pub strength: f32,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct GenPreset {
    pub name: String,
    /// A model key to use (`flux2-klein-4b`, `ckpt:…`), if the preset is tied to one.
    pub model: Option<String>,
    /// Else the first available model of this family.
    pub family: Option<String>,
    pub template: String,
    pub negative: String,
    pub steps: Option<u32>,
    pub cfg: Option<f32>,
    pub sampler: Option<(String, String)>,
    pub loras: Vec<PresetLora>,
    /// Draft → Refine: refine with this model at this strength.
    pub refine_model: Option<String>,
    pub refine_strength: Option<f32>,
    #[serde(skip)]
    pub built_in: bool,
}

impl GenPreset {
    /// The prompt with the template applied.
    pub fn apply(&self, prompt: &str) -> String {
        let t = self.template.trim();
        let p = prompt.trim();
        if t.is_empty() {
            p.to_owned()
        } else if t.contains("{prompt}") {
            t.replace("{prompt}", p)
        } else if p.is_empty() {
            t.to_owned()
        } else {
            format!("{p}, {t}")
        }
    }
}

/// Built-in presets (after Krita AI Diffusion's built-in styles).
pub fn built_in() -> Vec<GenPreset> {
    let p = |name: &str, family: Option<&str>, template: &str, negative: &str, steps: Option<u32>, cfg: Option<f32>| GenPreset {
        name: name.into(),
        family: family.map(str::to_owned),
        template: template.into(),
        negative: negative.into(),
        steps,
        cfg,
        built_in: true,
        ..Default::default()
    };
    vec![
        p("None", None, "", "", None, None),
        p("Cinematic Photo", None, "cinematic film still of {prompt}, natural light, shallow depth of field, sharp focus", "", None, None),
        p("Digital Artwork", None, "concept art of {prompt}, digital artwork, illustrative, matte painting, highly detailed", "photo, photorealistic", None, None),
        p("Product Shot", None, "studio product photograph of {prompt}, soft box lighting, clean background, high detail", "", None, None),
        p(
            "Anime (Illustrious)",
            Some("illustrious"),
            "{prompt}, masterpiece, best quality, newest, absurdres, highres",
            "worst quality, worst aesthetic, bad quality, average quality, oldest, old, displeasing",
            Some(24),
            Some(5.0),
        ),
        p("Photo (SDXL)", Some("sdxl"), "cinematic film still {prompt}", "anime, cartoon, graphic, text, painting, crayon, graphite, abstract", Some(25), Some(6.0)),
        p("Poster (ERNIE)", Some("ernie"), "{prompt}", "", None, None),
    ]
}

pub fn path() -> PathBuf {
    crate::settings::data_root().join("presets.json")
}

/// The user's presets.
pub fn load_user(path: &Path) -> Vec<GenPreset> {
    std::fs::read(path).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
}

pub fn save_user(path: &Path, presets: &[GenPreset]) -> Result<()> {
    let user: Vec<&GenPreset> = presets.iter().filter(|p| !p.built_in).collect();
    crate::settings::write_atomic(path, &serde_json::to_vec_pretty(&user)?)
}

/// Built-in presets followed by the user's.
pub fn all(path: &Path) -> Vec<GenPreset> {
    let mut v = built_in();
    v.extend(load_user(path));
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn templates_apply() {
        let b = built_in();
        let cine = b.iter().find(|p| p.name == "Cinematic Photo").unwrap();
        assert!(cine.apply("a fox").starts_with("cinematic film still of a fox,"));
        let suffix = GenPreset { template: "watercolor".into(), ..Default::default() };
        assert_eq!(suffix.apply("a fox"), "a fox, watercolor");
        assert_eq!(b[0].apply(" a fox "), "a fox");
    }

    #[test]
    fn user_presets_round_trip() {
        let path = std::env::temp_dir().join(format!("li-presets-{}.json", uuid::Uuid::new_v4().simple()));
        let mine = GenPreset { name: "Mine".into(), model: Some("flux2-klein-4b".into()), template: "{prompt}, ink".into(), ..Default::default() };
        let mut all_now = all(&path);
        all_now.push(mine.clone());
        save_user(&path, &all_now).unwrap();
        let back = all(&path);
        assert_eq!(back.last(), Some(&mine));
        assert_eq!(back.iter().filter(|p| p.built_in).count(), built_in().len());
        let _ = std::fs::remove_file(path);
    }
}
