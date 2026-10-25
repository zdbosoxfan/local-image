//! The setup state (`<state>/ai-models.json`), per ComfyUI server: what each main model file
//! is, how it runs (its compiled recipe), what was found, and what its verification measured.
//! The app and the CLI both write it; every write re-reads the file and merges per model key, so
//! a newer verification is never lost.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use anyhow::Result;
use serde::{Deserialize, Serialize};

use super::issue::{Fix, Issue};
use crate::companions::Binding;
use crate::identify::Identity;
use crate::template::TemplateRecipe;

pub const STATE_VERSION: u32 = 1;

/// Where a model stands (wizard states).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    #[default]
    New,
    Identified,
    Unmatched,
    Blocked,
    Ready,
    Verifying,
    Verified,
    PartiallyVerified,
    Failed,
    NeedsReverify,
    Broken,
    Builtin,
    Ignored,
    Removed,
}

impl Status {
    pub fn id(self) -> &'static str {
        match self {
            Self::New => "new",
            Self::Identified => "identified",
            Self::Unmatched => "unmatched",
            Self::Blocked => "blocked",
            Self::Ready => "ready",
            Self::Verifying => "verifying",
            Self::Verified => "verified",
            Self::PartiallyVerified => "partially_verified",
            Self::Failed => "failed",
            Self::NeedsReverify => "needs_reverify",
            Self::Broken => "broken",
            Self::Builtin => "builtin",
            Self::Ignored => "ignored",
            Self::Removed => "removed",
        }
    }

    /// Whether the model is offered for generation.
    pub fn usable(self) -> bool {
        matches!(self, Self::Ready | Self::Verifying | Self::Verified | Self::PartiallyVerified | Self::NeedsReverify | Self::Builtin)
    }
}

/// How the file looked when it was identified (a change triggers re-identification).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Fingerprint {
    pub bytes: u64,
    pub mtime_ns: u64,
    /// SHA-256 of the safetensors header (length + JSON) or the GGUF metadata.
    pub header_sha256: Option<String>,
}

/// Measurements of one rendered image.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Metrics {
    pub luma_mean: f64,
    pub luma_std: f64,
    pub noise_ratio: f64,
    pub top_bin_share: f64,
    pub red_centre: f64,
    pub blue_centre: f64,
    pub ring_luma: f64,
    pub ring_diff: f64,
}

/// One capability's test.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CapabilityResult {
    pub pass: bool,
    pub seconds: f64,
    /// The first render of the model (includes loading it).
    pub cold: bool,
    /// GPU memory in use during the test (all processes), in GB.
    pub peak_vram_gb: Option<f64>,
    pub size: Option<(u32, u32)>,
    /// The size asked for (Create).
    pub requested: Option<(u32, u32)>,
    pub metrics: Option<Metrics>,
    /// Mean absolute difference to the source (refine), or inside the mask (fill).
    pub difference: Option<f64>,
    pub outside_change: Option<f64>,
    /// Warning codes (`VerifyWarning:prompt`).
    pub warnings: Vec<String>,
    /// The failure code, when it failed.
    pub failure: Option<String>,
    pub message: Option<String>,
    /// Through another capability (`refine` for upscale).
    pub via: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Verified {
    pub at: String,
    pub seed: u64,
    pub comfyui_version: String,
    pub templates_version: String,
    pub recipe_version: u32,
    /// SHA-256 of the canonical create (or edit) graph that was tested.
    pub recipe_sha256: String,
    pub gpu: String,
    pub capabilities: BTreeMap<String, CapabilityResult>,
}

/// What the person chose for a model.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct UserChoice {
    pub template_choice: Option<String>,
    /// `declared → installed`.
    pub substitutions: BTreeMap<String, String>,
    pub ignored: bool,
    /// Files whose GPU warning the person overrode.
    pub try_anyway: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ModelState {
    pub status: Status,
    pub file: String,
    pub folder: String,
    pub first_seen: String,
    pub fingerprint: Fingerprint,
    pub identity: Option<Identity>,
    /// `template`, `builder`, `universal` or `unsupported`.
    pub engine: String,
    /// Capability → template name.
    pub templates: BTreeMap<String, String>,
    pub bindings: Vec<Binding>,
    pub recipe: Option<TemplateRecipe>,
    /// The previous recipe, kept when a recompile after an update fails.
    pub last_good: Option<TemplateRecipe>,
    pub issues: Vec<Issue>,
    pub user: UserChoice,
    pub verified: Option<Verified>,
    pub label: String,
    /// The catalogue model it runs as (`unet:<file>`, or a curated model's key).
    pub catalog_id: String,
    /// The preset of `catalog_id` this file is (`int8`, `bf16`); empty: the model's default.
    pub variant: String,
    /// What each capability's static check found.
    pub capabilities: Vec<super::check::CapabilityCheck>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct GpuState {
    pub name: String,
    pub vram_total: u64,
    pub generation: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ServerState {
    pub comfyui_version: String,
    pub templates_version: String,
    pub object_info_sig: String,
    pub gpu: Option<GpuState>,
    pub models: BTreeMap<String, ModelState>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SetupState {
    pub version: u32,
    pub servers: BTreeMap<String, ServerState>,
}

/// `<state>/ai-models.json`.
pub fn default_path() -> PathBuf {
    crate::settings::state_dir().join("ai-models.json")
}

static WRITE: Mutex<()> = Mutex::new(());

impl SetupState {
    /// Reads the state (missing, oversized or unreadable: empty, logged).
    pub fn load(path: &Path) -> Self {
        let bytes = match std::fs::metadata(path) {
            Ok(m) if m.len() > 16 << 20 => {
                log::warn!("{} is larger than 16 MiB; ignoring it", path.display());
                return Self::default();
            }
            Ok(_) => std::fs::read(path).unwrap_or_default(),
            Err(_) => return Self::default(),
        };
        match serde_json::from_slice::<Self>(&bytes) {
            Ok(s) => s,
            Err(e) => {
                log::warn!("{} could not be read ({e}); starting empty", path.display());
                Self::default()
            }
        }
    }

    pub fn server(&self, host: &str) -> Option<&ServerState> {
        self.servers.get(host)
    }

    /// Applies `change` to the stored state under a lock and writes it back; returns the result.
    pub fn update(path: &Path, change: impl FnOnce(&mut SetupState)) -> Result<SetupState> {
        let _guard = WRITE.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut s = Self::load(path);
        change(&mut s);
        s.version = STATE_VERSION;
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        li_fs::atomic_write(path, &serde_json::to_vec_pretty(&s)?)?;
        Ok(s)
    }
}

/// Merges a freshly prepared model into the stored one: the person's choices and the
/// verification survive; a stored verification, and the failures and warnings it found, count
/// only while the tested recipe and file are unchanged (else the model needs a re-check).
pub fn merge(stored: Option<&ModelState>, mut fresh: ModelState, server: &ServerState, comfyui_version: &str) -> ModelState {
    let Some(old) = stored else { return fresh };
    fresh.user = old.user.clone();
    fresh.first_seen = old.first_seen.clone();
    if old.user.ignored {
        fresh.status = Status::Ignored;
    }
    let Some(v) = old.verified.clone() else { return fresh };
    let file_changed = old.fingerprint.bytes != fresh.fingerprint.bytes
        || old.fingerprint.header_sha256.is_some()
            && fresh.fingerprint.header_sha256.is_some()
            && old.fingerprint.header_sha256 != fresh.fingerprint.header_sha256;
    let tested = fresh.recipe.as_ref().map(super::recipe_sha256).unwrap_or_default();
    let same_recipe = tested == v.recipe_sha256;
    let blocked = matches!(fresh.status, Status::Blocked | Status::Unmatched | Status::Broken);
    fresh.verified = Some(v.clone());
    if blocked || fresh.status == Status::Ignored {
        return fresh;
    }
    let failed = v.capabilities.values().any(|c| !c.pass && c.failure.is_some());
    let create_failed = v.capabilities.get("create").is_some_and(|c| !c.pass);
    let key = fresh_key(&fresh);
    // It was tested: "not tested yet" no longer applies, whatever the test's standing now.
    fresh.issues.retain(|i| i.code != "NotVerified");
    let reverify = if file_changed {
        Some(Issue::new("FileChanged", Some(&key), &[("file", fresh.file.clone()), ("date", v.at.clone())]))
    } else if !same_recipe && v.comfyui_version != comfyui_version {
        Some(Issue::new(
            "ReverifyNeeded",
            Some(&key),
            &[("old", v.comfyui_version.clone()), ("new", comfyui_version.to_owned()), ("model", fresh.label.clone())],
        ))
    } else if !same_recipe {
        Some(Issue::new("RecipeChanged", Some(&key), &[]))
    } else {
        None
    };
    fresh.status = match reverify {
        Some(issue) => {
            fresh.issues.push(issue.fix(Fix::Verify { model: fresh.file.clone() }));
            Status::NeedsReverify
        }
        None => {
            // The stored test still applies, and so do the failures and warnings it found.
            fresh.issues.extend(old.issues.iter().filter(|i| i.code.starts_with("Verify")).cloned());
            if create_failed {
                Status::Failed
            } else if failed {
                Status::PartiallyVerified
            } else {
                if v.comfyui_version != comfyui_version {
                    fresh.issues.push(Issue::new("StillValid", Some(&key), &[("new", comfyui_version.to_owned())]));
                }
                Status::Verified
            }
        }
    };
    if let Some(gpu) = &server.gpu
        && !v.gpu.is_empty()
        && v.gpu != gpu.name
    {
        fresh.issues.push(Issue::new("OtherGpu", Some(&key), &[("old_gpu", v.gpu.clone()), ("new_gpu", gpu.name.clone())]));
    }
    fresh
}

fn fresh_key(m: &ModelState) -> String {
    m.recipe.as_ref().map(|r| r.model_key.clone()).unwrap_or_else(|| m.file.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn verified_model(sha: &str, version: &str) -> ModelState {
        let mut caps = BTreeMap::new();
        caps.insert("create".to_owned(), CapabilityResult { pass: true, ..Default::default() });
        ModelState {
            status: Status::Verified,
            file: "m.safetensors".into(),
            fingerprint: Fingerprint { bytes: 10, ..Default::default() },
            verified: Some(Verified { recipe_sha256: sha.into(), comfyui_version: version.into(), capabilities: caps, ..Default::default() }),
            ..Default::default()
        }
    }

    #[test]
    fn verification_survives_only_an_unchanged_recipe_and_file() {
        let server = ServerState::default();
        let fresh = || ModelState {
            status: Status::Ready,
            file: "m.safetensors".into(),
            fingerprint: Fingerprint { bytes: 10, ..Default::default() },
            recipe: Some(TemplateRecipe::default()),
            ..Default::default()
        };
        let empty = super::super::recipe_sha256(&TemplateRecipe::default());
        // Same recipe on a newer server: still verified, with a note.
        let m = merge(Some(&verified_model(&empty, "0.42.5")), fresh(), &server, "0.43.0");
        assert_eq!(m.status, Status::Verified);
        assert!(m.issues.iter().any(|i| i.code == "StillValid"));
        // A different recipe after the update: needs a re-check.
        let m = merge(Some(&verified_model("other", "0.42.5")), fresh(), &server, "0.43.0");
        assert_eq!(m.status, Status::NeedsReverify);
        assert!(m.issues.iter().any(|i| i.code == "ReverifyNeeded"));
        // The file was replaced.
        let changed = ModelState { fingerprint: Fingerprint { bytes: 11, ..Default::default() }, ..fresh() };
        assert_eq!(merge(Some(&verified_model(&empty, "0.42.5")), changed, &server, "0.42.5").status, Status::NeedsReverify);
        // A blocker found now wins over the old verification.
        let blocked = ModelState { status: Status::Blocked, ..fresh() };
        assert_eq!(merge(Some(&verified_model(&empty, "0.42.5")), blocked, &server, "0.42.5").status, Status::Blocked);
    }

    #[test]
    fn a_rescan_keeps_what_the_test_found_and_drops_not_tested() {
        let empty = super::super::recipe_sha256(&TemplateRecipe::default());
        let mut old = verified_model(&empty, "0.42.5");
        old.verified
            .as_mut()
            .unwrap()
            .capabilities
            .insert("inpaint".into(), CapabilityResult { failure: Some("VerifyFailed:black".into()), ..Default::default() });
        old.issues = vec![Issue::new("VerifyFailed:black", None, &[("capability", "Fill".into())]), Issue::new("LicenceUnknown", None, &[])];
        let fresh = || ModelState {
            status: Status::Ready,
            file: "m.safetensors".into(),
            fingerprint: Fingerprint { bytes: 10, ..Default::default() },
            recipe: Some(TemplateRecipe::default()),
            issues: vec![Issue::new("NotVerified", None, &[("model", "M".into())]), Issue::new("LicenceUnknown", None, &[])],
            ..Default::default()
        };
        let m = merge(Some(&old), fresh(), &ServerState::default(), "0.42.5");
        let codes: Vec<&str> = m.issues.iter().map(|i| i.code.as_str()).collect();
        assert_eq!(m.status, Status::PartiallyVerified);
        assert_eq!(codes, ["LicenceUnknown", "VerifyFailed:black"]);
        assert!(m.issues[1].english().starts_with("The Fill test image came out black"));
        // A replaced file needs a new test: the old findings go, and the re-check offers Verify.
        let changed = ModelState { fingerprint: Fingerprint { bytes: 11, ..Default::default() }, ..fresh() };
        let m = merge(Some(&old), changed, &ServerState::default(), "0.42.5");
        let codes: Vec<&str> = m.issues.iter().map(|i| i.code.as_str()).collect();
        assert_eq!(codes, ["LicenceUnknown", "FileChanged"]);
        assert!(matches!(&m.issues[1].fixes[..], [Fix::Verify { model }] if model == "m.safetensors"));
    }

    #[test]
    fn state_round_trips_and_unreadable_files_start_empty() {
        let dir = std::env::temp_dir().join(format!("li-setup-state-{}", uuid::Uuid::new_v4()));
        let p = dir.join("ai-models.json");
        assert_eq!(SetupState::load(&p), SetupState::default());
        SetupState::update(&p, |s| {
            s.servers.entry("127.0.0.1:8189".into()).or_default().models.insert("unet:m".into(), verified_model("x", "0.42.5"));
        })
        .unwrap();
        let back = SetupState::load(&p);
        assert_eq!(back.version, STATE_VERSION);
        assert_eq!(back.server("127.0.0.1:8189").unwrap().models["unet:m"].status, Status::Verified);
        std::fs::write(&p, b"{not json").unwrap();
        assert_eq!(SetupState::load(&p), SetupState::default());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
