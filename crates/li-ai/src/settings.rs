//! AI settings live in the application `preferences.json` under `ai`.
//! Generated resources keep their data root. Unknown settings are preserved.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde_json::{Map, Value};

/// The per-user data root: `LOCAL_IMAGE_DATA_DIR`, else `%LOCALAPPDATA%\Local Image` (Windows),
/// `~/Library/Application Support/Local Image` (macOS), `$XDG_DATA_HOME/local-image` (Linux).
pub fn data_root() -> PathBuf {
    if let Some(v) = std::env::var_os("LOCAL_IMAGE_DATA_DIR").filter(|v| !v.is_empty()) {
        return PathBuf::from(v);
    }
    let base = dirs::data_local_dir().unwrap_or_else(|| PathBuf::from("."));
    if cfg!(target_os = "linux") { base.join("local-image") } else { base.join("Local Image") }
}

/// The single application preferences file.
pub fn config_path() -> PathBuf {
    lightcraft_sysmem::app_dirs::config_dir().unwrap_or_else(|| PathBuf::from(".")).join("preferences.json")
}
pub fn state_dir() -> PathBuf {
    data_root().join("state")
}
pub fn default_model_dir() -> PathBuf {
    lightcraft_sysmem::app_dirs::default_models_root()
}

/// The AI-related settings, stored as the `ai` section of `preferences.json`.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct AiSettings {
    pub comfy_host: String,
    #[serde(deserialize_with = "checked_port")]
    pub comfy_port: u16,
    /// A ComfyUI installation folder (contains `main.py`), used to start it.
    pub comfy_directory: String,
    pub comfy_python: String,
    pub model_directory: String,
    /// `discover`, `portable`, `later` or `remote`.
    pub setup_mode: String,
    pub hardware_guide_dismissed: bool,
    pub lora_show_adult_content: bool,
    /// Start ComfyUI with Local Image when an installation is configured.
    #[serde(rename = "auto_start_comfy")]
    pub auto_start: bool,
    #[serde(flatten)]
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
        let host = self.comfy_host.trim().trim_start_matches('[').trim_end_matches(']');
        if host.contains(':') { format!("[{host}]:{}", self.comfy_port) } else { format!("{host}:{}", self.comfy_port) }
    }

    /// Remote mode can also use loopback via an SSH tunnel. A non-loopback host is always
    /// remote so a non-local host can never start or download into a local installation.
    pub fn is_remote(&self) -> bool {
        let host = self.comfy_host.trim().trim_start_matches('[').trim_end_matches(']');
        self.setup_mode == "remote" || !(host.eq_ignore_ascii_case("localhost") || host.parse::<std::net::IpAddr>().is_ok_and(|ip| ip.is_loopback()))
    }

    pub fn model_dir(&self) -> PathBuf {
        if self.model_directory.trim().is_empty() { default_model_dir() } else { PathBuf::from(&self.model_directory) }
    }

    /// Model folder for frame-time callers. Explicit settings loads and successful saves refresh it.
    pub fn cached_model_dir() -> PathBuf {
        let path = config_path();
        let cached = MODEL_DIR.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(cache) = cached.as_ref().filter(|cache| cache.config == path) {
            return cache.directory.clone();
        }
        drop(cached);
        Self::load().model_dir()
    }

    pub fn load() -> Self {
        let path = config_path();
        let settings = match store_backend() {
            Some(store) => (store.load)().ok().flatten().map(Self::from_value).unwrap_or_default(),
            None => {
                let value = std::fs::read_to_string(&path).ok().and_then(|text| serde_json::from_str::<Value>(&text).ok());
                value.and_then(|root| root.get("ai").cloned()).map(Self::from_value).unwrap_or_default()
            }
        };
        publish(&settings);
        refresh_model_dir_cache(&path, &settings);
        settings
    }

    pub fn load_from(path: &Path) -> Self {
        let text = std::fs::read_to_string(path).unwrap_or_default();
        let text = text.trim_start_matches('\u{feff}');
        let Ok(value) = serde_json::from_str::<Value>(text) else { return Self::default() };
        Self::from_value(value.get("ai").cloned().unwrap_or(value))
    }
    /// Lenient typed adapter; unknown keys are retained for forwards compatibility.
    pub fn from_value(value: Value) -> Self {
        let Value::Object(mut m) = value else { return Self::default() };
        let d = Self::default();
        let mut take_str = |k: &str, def: &str| m.remove(k).and_then(|v| v.as_str().map(str::to_owned)).unwrap_or_else(|| def.to_owned());
        let comfy_host = take_str("comfy_host", &d.comfy_host);
        let comfy_directory = take_str("comfy_directory", "");
        let comfy_python = take_str("comfy_python", "");
        let model_directory = take_str("model_directory", "");
        let setup_mode = take_str("setup_mode", &d.setup_mode);
        let comfy_port = m.remove("comfy_port").and_then(|v| v.as_u64()).filter(|p| (1..=65535).contains(p)).unwrap_or(8188) as u16;
        let mut take_bool = |k: &str, def: bool| m.remove(k).and_then(|v| v.as_bool()).unwrap_or(def);
        let hardware_guide_dismissed = take_bool("hardware_guide_dismissed", false);
        let lora_show_adult_content = take_bool("lora_show_adult_content", false);
        let auto_start = m.remove("auto_start_comfy").and_then(|value| value.as_bool()).unwrap_or(d.auto_start);
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
        let value = serde_json::to_value(self)?;
        if let Some(store) = store_backend() {
            (store.save)(value).map_err(anyhow::Error::msg)?;
        } else {
            let path = config_path();
            let mut root = match std::fs::read(&path) {
                Ok(bytes) => serde_json::from_slice::<Value>(&bytes).context("The preferences file cannot be read; it was not replaced")?,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => serde_json::json!({}),
                Err(error) => return Err(error.into()),
            };
            root.as_object_mut().context("Preferences must be an object; the original was not replaced")?.insert("ai".into(), value);
            li_fs::atomic_write(&path, serde_json::to_string_pretty(&root)?.as_bytes())?;
        }
        publish(self);
        refresh_model_dir_cache(&config_path(), self);
        SETTINGS_REV.fetch_add(1, std::sync::atomic::Ordering::Release);
        Ok(())
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
        li_fs::atomic_write(path, serde_json::to_string_pretty(&Value::Object(m))?.as_bytes())?;
        refresh_model_dir_cache(path, self);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn j11_settings_unknown_keys_durable_fault_and_shared_adapter_parity() {
        let dir = std::env::temp_dir().join(format!("j11-settings-{}", uuid::Uuid::new_v4()));
        let path = dir.join("nested/config.json");
        let original = AiSettings::from_value(serde_json::json!({"comfy_port":8190,"future":{"nested":[1,2]}}));
        let before = li_fs::syncs_on_this_thread();
        original.save_to(&path).unwrap();
        assert!(li_fs::syncs_on_this_thread() > before);
        let golden = std::fs::read(&path).unwrap();
        let updated = AiSettings::from_value(serde_json::json!({"comfy_port":8191}));
        {
            let _fault = li_fs::fail_writes_after(4);
            assert!(updated.save_to(&path).is_err());
        }
        assert_eq!(std::fs::read(&path).unwrap(), golden);
        assert_eq!(AiSettings::load_from(&path), original);
        assert_eq!(std::fs::read_dir(path.parent().unwrap()).unwrap().count(), 1);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn remote_hosts_and_tunnels_do_not_manage_local_models() {
        let mut settings = AiSettings::default();
        assert!(!settings.is_remote());
        for host in ["192.168.1.20", "gpu.example.ts.net", "100.64.0.2", "fe80::1"] {
            settings.comfy_host = host.into();
            assert!(settings.is_remote());
        }
        settings.comfy_host = " ::1 ".into();
        assert!(!settings.is_remote());
        assert_eq!(settings.host(), "[::1]:8188");
        settings.setup_mode = "remote".into();
        assert!(settings.is_remote(), "SSH tunnels must be explicitly remote");
    }

    #[test]
    fn round_trip_keeps_unknown_keys_and_rejects_port_zero() {
        let dir = std::env::temp_dir().join(format!("li-settings-{}", uuid::Uuid::new_v4()));
        let p = dir.join("config.json");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(&p, "\u{feff}{\"comfy_port\": 0, \"model_directory\": \"/m\", \"future_key\": [1,2]}").unwrap();
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

struct ModelDirCache {
    config: PathBuf,
    fallback: PathBuf,
    directory: PathBuf,
}
static MODEL_DIR: std::sync::Mutex<Option<ModelDirCache>> = std::sync::Mutex::new(None);

#[cfg(test)]
fn cached_model_dir_for(path: &Path, fallback: PathBuf) -> PathBuf {
    let mut cached = MODEL_DIR.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some(c) = cached.as_ref().filter(|c| c.config == path && c.fallback == fallback) {
        return c.directory.clone();
    }
    let settings = AiSettings::load_from(path);
    let directory = if settings.model_directory.trim().is_empty() { fallback.clone() } else { PathBuf::from(&settings.model_directory) };
    *cached = Some(ModelDirCache { config: path.into(), fallback, directory: directory.clone() });
    directory
}

fn refresh_model_dir_cache(path: &Path, settings: &AiSettings) {
    let mut cached = MODEL_DIR.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some(c) = cached.as_mut().filter(|c| c.config == path) {
        c.directory = if settings.model_directory.trim().is_empty() { c.fallback.clone() } else { PathBuf::from(&settings.model_directory) };
    }
}

#[test]
fn dsfix_model_dir_cache_avoids_frame_reads_and_refreshes_after_save() {
    let dir = std::env::temp_dir().join(format!("li-dsfix-cache-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let config = dir.join("config.json");
    std::fs::write(&config, br#"{"model_directory":"first"}"#).unwrap();
    let fallback = dir.join("default");
    assert_eq!(cached_model_dir_for(&config, fallback.clone()), PathBuf::from("first"));
    // Removing the file makes an accidental per-frame load observable as a fallback.
    std::fs::remove_file(&config).unwrap();
    for _ in 0..120 {
        assert_eq!(cached_model_dir_for(&config, fallback.clone()), PathBuf::from("first"));
    }
    let settings = AiSettings { model_directory: "second".into(), ..Default::default() };
    settings.save_to(&config).unwrap();
    assert_eq!(cached_model_dir_for(&config, fallback.clone()), PathBuf::from("second"));
    AiSettings::default().save_to(&config).unwrap();
    assert_eq!(cached_model_dir_for(&config, fallback.clone()), fallback);
    std::fs::remove_dir_all(dir).unwrap();
}

/// The host binds AI to its existing checked preferences writer, never a second settings file.
#[derive(Clone)]
pub struct StoreBackend {
    pub load: std::sync::Arc<dyn Fn() -> std::result::Result<Option<Value>, String> + Send + Sync>,
    pub save: std::sync::Arc<dyn Fn(Value) -> std::result::Result<(), String> + Send + Sync>,
}
static STORE: std::sync::OnceLock<StoreBackend> = std::sync::OnceLock::new();
pub fn install_store_backend(store: StoreBackend) {
    let _ = STORE.set(store);
}
thread_local! { static SESSION_STORE: std::cell::RefCell<Option<StoreBackend>> = const { std::cell::RefCell::new(None) }; }
fn store_backend() -> Option<StoreBackend> {
    SESSION_STORE.with(|store| store.borrow().clone()).or_else(|| STORE.get().cloned())
}
/// A scoped store binding for embedded sessions and offscreen UI fixtures; restores the previous
/// binding even if the caller unwinds. Native windows normally use the installed process store.
pub fn with_store_backend<R>(store: StoreBackend, run: impl FnOnce() -> R) -> R {
    struct Restore(Option<StoreBackend>);
    impl Drop for Restore {
        fn drop(&mut self) {
            SESSION_STORE.with(|store| *store.borrow_mut() = self.0.take());
        }
    }
    let _restore = Restore(SESSION_STORE.with(|slot| slot.replace(Some(store))));
    run()
}
static SETTINGS_REV: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
pub fn revision() -> u64 {
    SETTINGS_REV.load(std::sync::atomic::Ordering::Acquire)
}
pub fn publish(settings: &AiSettings) {
    lightcraft_sysmem::app_dirs::set_models_root(settings.model_dir());
    *MODEL_DIR.lock().unwrap_or_else(std::sync::PoisonError::into_inner) =
        Some(ModelDirCache { config: config_path(), fallback: default_model_dir(), directory: settings.model_dir() });
}

fn checked_port<'de, D: serde::Deserializer<'de>>(deserializer: D) -> std::result::Result<u16, D::Error> {
    let port = <u16 as serde::Deserialize>::deserialize(deserializer)?;
    if port == 0 {
        return Err(serde::de::Error::custom("not a usable ComfyUI port"));
    }
    Ok(port)
}

#[test]
fn ai_auto_start_comfy_normalizes_without_duplicate_fields_and_preserves_unknowns() {
    for (value, expected) in [(serde_json::json!({"auto_start_comfy":false,"future":[1,2]}), false), (serde_json::json!({"future":[1,2]}), true)] {
        let settings = AiSettings::from_value(value.clone());
        let normalized = serde_json::to_value(&settings).unwrap();
        assert_eq!(normalized["auto_start_comfy"], expected);
        assert_eq!(normalized.get("future"), value.get("future"));
        assert_eq!(serde_json::from_value::<AiSettings>(normalized).unwrap(), settings);
    }
}
