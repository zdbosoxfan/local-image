//! Where Local Image keeps its data, and the AI settings in `config.json`.
//!
//! The data root and `config.json` keys are the same as Local Image 0.7, so an existing ComfyUI
//! setup, model folder and generated library are picked up by V2 unchanged. Unknown keys written by
//! other versions are preserved.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde_json::{Map, Value};

/// The per-user data root: `LOCAL_IMAGE_DATA_DIR`, else `%LOCALAPPDATA%\Local Image` (Windows),
/// `~/Library/Application Support/Local Image` (macOS), `$XDG_DATA_HOME/local-image` (Linux).
pub fn data_root() -> PathBuf {
    for var in ["LOCAL_IMAGE_DATA_DIR", "LOCAL_REMOVE_DATA_DIR"] {
        if let Some(v) = std::env::var_os(var).filter(|v| !v.is_empty()) {
            return PathBuf::from(v);
        }
    }
    let base = dirs::data_local_dir().unwrap_or_else(|| PathBuf::from("."));
    if cfg!(target_os = "linux") { base.join("local-image") } else { base.join("Local Image") }
}

pub fn config_path() -> PathBuf {
    data_root().join("config.json")
}
pub fn state_dir() -> PathBuf {
    data_root().join("state")
}
pub fn default_model_dir() -> PathBuf {
    std::env::var_os("LOCAL_IMAGE_MODELS_DIR").filter(|v| !v.is_empty()).map(PathBuf::from).unwrap_or_else(|| data_root().join("models"))
}

/// The AI-related settings. Field names match 0.7's `config.json`.
#[derive(Clone, Debug, PartialEq)]
pub struct AiSettings {
    pub comfy_host: String,
    pub comfy_port: u16,
    /// A ComfyUI installation folder (contains `main.py`), used to start it.
    pub comfy_directory: String,
    pub comfy_python: String,
    pub model_directory: String,
    /// `discover`, `portable` or `later`.
    pub setup_mode: String,
    pub hardware_guide_dismissed: bool,
    pub lora_show_adult_content: bool,
    /// Start ComfyUI with Local Image when an installation is configured.
    pub auto_start: bool,
    rest: Map<String, Value>,
}

impl Default for AiSettings {
    fn default() -> Self {
        Self {
            comfy_host: "127.0.0.1".into(),
            comfy_port: 8188,
            comfy_directory: String::new(),
            comfy_python: String::new(),
            model_directory: String::new(),
            setup_mode: "discover".into(),
            hardware_guide_dismissed: false,
            lora_show_adult_content: false,
            auto_start: true,
            rest: Map::new(),
        }
    }
}

impl AiSettings {
    /// A string setting kept alongside the typed ones (tokens, browser options).
    pub fn extra_string(&self, key: &str) -> Option<String> {
        self.rest.get(key).and_then(Value::as_str).map(str::trim).filter(|s| !s.is_empty()).map(str::to_owned)
    }
    pub fn set_extra_string(&mut self, key: &str, value: &str) {
        if value.trim().is_empty() {
            self.rest.remove(key);
        } else {
            self.rest.insert(key.to_owned(), Value::String(value.trim().to_owned()));
        }
    }
    pub fn host(&self) -> String {
        format!("{}:{}", self.comfy_host, self.comfy_port)
    }

    pub fn model_dir(&self) -> PathBuf {
        if self.model_directory.trim().is_empty() { default_model_dir() } else { PathBuf::from(&self.model_directory) }
    }

    pub fn load() -> Self {
        Self::load_from(&config_path())
    }

    pub fn load_from(path: &Path) -> Self {
        let text = std::fs::read_to_string(path).unwrap_or_default();
        let text = text.trim_start_matches('\u{feff}');
        let Ok(Value::Object(mut m)) = serde_json::from_str::<Value>(text) else { return Self::default() };
        let d = Self::default();
        let mut take_str = |k: &str, def: &str| m.remove(k).and_then(|v| v.as_str().map(str::to_owned)).unwrap_or_else(|| def.to_owned());
        let comfy_host = take_str("comfy_host", &d.comfy_host);
        let comfy_directory = take_str("comfy_directory", "");
        let comfy_python = take_str("comfy_python", "");
        let model_directory = take_str("model_directory", "");
        let setup_mode = take_str("setup_mode", &d.setup_mode);
        let comfy_port = m.remove("comfy_port").and_then(|v| v.as_u64()).filter(|p| (1..=65535).contains(p) && *p != 51247).unwrap_or(8188) as u16;
        let mut take_bool = |k: &str, def: bool| m.remove(k).and_then(|v| v.as_bool()).unwrap_or(def);
        let hardware_guide_dismissed = take_bool("hardware_guide_dismissed", false);
        let lora_show_adult_content = take_bool("lora_show_adult_content", false);
        let auto_start = take_bool("auto_start_comfy", true);
        Self {
            comfy_host,
            comfy_port,
            comfy_directory,
            comfy_python,
            model_directory,
            setup_mode,
            hardware_guide_dismissed,
            lora_show_adult_content,
            auto_start,
            rest: m,
        }
    }

    pub fn save(&self) -> Result<()> {
        self.save_to(&config_path())
    }

    /// Atomic write (temp file + rename), keeping keys this version doesn't know.
    pub fn save_to(&self, path: &Path) -> Result<()> {
        let mut m = self.rest.clone();
        m.insert("comfy_host".into(), self.comfy_host.clone().into());
        m.insert("comfy_port".into(), self.comfy_port.into());
        m.insert("comfy_directory".into(), self.comfy_directory.clone().into());
        m.insert("comfy_python".into(), self.comfy_python.clone().into());
        m.insert("model_directory".into(), self.model_directory.clone().into());
        m.insert("setup_mode".into(), self.setup_mode.clone().into());
        m.insert("hardware_guide_dismissed".into(), self.hardware_guide_dismissed.into());
        m.insert("lora_show_adult_content".into(), self.lora_show_adult_content.into());
        m.insert("auto_start_comfy".into(), self.auto_start.into());
        write_atomic(path, serde_json::to_string_pretty(&Value::Object(m))?.as_bytes())
    }
}

/// Writes `bytes` to `path` through a uniquely named temporary file and a rename.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    let dir = path.parent().context("path has no folder")?;
    std::fs::create_dir_all(dir)?;
    let tmp = dir.join(format!(".{}.{}.tmp", path.file_name().and_then(|n| n.to_str()).unwrap_or("file"), uuid::Uuid::new_v4().simple()));
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, path).inspect_err(|_| {
        let _ = std::fs::remove_file(&tmp);
    })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_keeps_unknown_keys_and_rejects_the_app_port() {
        let dir = std::env::temp_dir().join(format!("li-settings-{}", uuid::Uuid::new_v4()));
        let p = dir.join("config.json");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(&p, "\u{feff}{\"comfy_port\": 51247, \"model_directory\": \"/m\", \"future_key\": [1,2]}").unwrap();
        let mut s = AiSettings::load_from(&p);
        assert_eq!(s.comfy_port, 8188);
        assert_eq!(s.model_dir(), PathBuf::from("/m"));
        s.comfy_port = 8190;
        s.save_to(&p).unwrap();
        let v: Value = serde_json::from_str(&std::fs::read_to_string(&p).unwrap()).unwrap();
        assert_eq!(v["future_key"], serde_json::json!([1, 2]));
        assert_eq!(AiSettings::load_from(&p).comfy_port, 8190);
        std::fs::remove_dir_all(dir).ok();
    }
}
