//! What the app shows about model setup between scans (the Generate banner, the picker's chips,
//! the status pill): the stored setup state against the installed models in the catalogue. Reads
//! no weights and makes no network request, so the status poller can afford it.

use std::collections::BTreeMap;

use super::issue::{Issue, Severity};
use super::state::{ServerState, Status};
use crate::catalog::{ModelId, ModelInfo, Origin};

/// One scanned model, as the picker shows it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ModelSummary {
    /// Canonical scanned file key (catalogue and preset keys are lookup aliases).
    pub file_key: String,
    pub status: Status,
    /// The first blocker (else warning) found, for a disabled row's tooltip.
    pub issue: Option<Issue>,
}

/// Model setup for the connected server.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Summary {
    /// Unscanned and ready, untested model files (the wizard's New group).
    pub new: Vec<String>,
    /// Scanned models by catalogue key.
    pub models: BTreeMap<String, ModelSummary>,
    /// `(stored, now)` when ComfyUI's version changed since the last scan.
    pub updated: Option<(String, String)>,
}

impl Summary {
    /// The model's setup status (`New` for a file no scan has seen).
    pub fn status(&self, key: &str) -> Option<Status> {
        self.models.get(key).map(|m| m.status).or_else(|| self.new.iter().any(|k| k == key).then_some(Status::New))
    }

    /// The status of one preset of a curated model (each precision is its own file), else the
    /// model's.
    pub fn status_for(&self, key: &str, variant: &str) -> Option<Status> {
        self.models.get(&preset_key(key, variant)).map(|m| m.status).or_else(|| self.status(key))
    }

    fn count(&self, f: impl Fn(Status) -> bool) -> usize {
        self.models.values().filter(|m| f(m.status)).map(|m| &m.file_key).collect::<std::collections::BTreeSet<_>>().len()
    }

    pub fn needs_reverify(&self) -> usize {
        self.count(|s| s == Status::NeedsReverify)
    }

    /// Models that need the person: blocked, unmatched, failed, to re-check, partly verified.
    pub fn attention(&self) -> usize {
        self.count(|s| matches!(s, Status::Blocked | Status::Unmatched | Status::Failed | Status::NeedsReverify | Status::PartiallyVerified))
    }

    /// Files awaiting setup, testing or a fix (the New and Needs attention groups).
    pub fn banner(&self) -> usize {
        self.new.len() + self.attention()
    }
}

fn preset_key(catalog_id: &str, variant: &str) -> String {
    format!("{catalog_id}#{variant}")
}

/// Whether an installed model runs only once model setup has prepared it (no builder runs it).
pub fn needs_setup(m: &ModelInfo) -> bool {
    m.origin == Origin::Installed && !m.tool && !m.resolved.buildable()
}

/// The summary of `server` (the stored state for the connected host) against `models`, the
/// catalogue's models; `version` is the version ComfyUI reports now.
pub fn summarise(server: Option<&ServerState>, models: &[ModelInfo], version: Option<&str>) -> Summary {
    let mut out = Summary::default();
    if let Some(s) = server {
        for (key, m) in &s.models {
            let issue = m.issues.iter().filter(|i| i.severity != Severity::Info).min_by_key(|i| i.severity).cloned();
            let summary = ModelSummary { file_key: key.clone(), status: m.status, issue };
            if matches!(m.status, Status::New | Status::Ready) {
                out.new.push(key.clone());
            }
            if !m.catalog_id.is_empty() && m.catalog_id != *key {
                if !m.variant.is_empty() {
                    out.models.insert(preset_key(&m.catalog_id, &m.variant), summary.clone());
                }
                // The model's own key stands for its default preset (else the first file seen).
                let default = ModelId::from_key(&m.catalog_id).map(crate::catalog::default_variant);
                if default == Some(m.variant.as_str()) || !out.models.contains_key(&m.catalog_id) {
                    out.models.insert(m.catalog_id.clone(), summary.clone());
                }
            }
            out.models.insert(key.clone(), summary);
        }
        out.updated = s
            .models
            .values()
            .flat_map(|m| &m.issues)
            .find(|i| i.code == "ReverifyNeeded")
            .and_then(|i| Some((i.args.get("old")?.clone(), i.args.get("new")?.clone())));
        let known = |v: &str| !v.is_empty() && v != "unknown";
        if let Some(v) = version.filter(|v| known(v) && known(&s.comfyui_version) && *v != s.comfyui_version) {
            out.updated = Some((s.comfyui_version.clone(), v.to_owned()));
        }
    }
    out.new.extend(models.iter().filter(|m| needs_setup(m) && !out.models.contains_key(m.id.key())).map(|m| m.id.key().to_owned()));
    out.new.sort();
    out.new.dedup();
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::InstalledModel;
    use crate::family::Role;
    use crate::modelsetup::state::ModelState;

    fn installed(name: &str, family: &str) -> InstalledModel {
        InstalledModel {
            key: format!("unet:{name}"),
            family: family.into(),
            label: name.into(),
            files: [(Role::Unet, name.into())].into(),
            ..Default::default()
        }
    }

    fn stored(status: Status, issues: Vec<Issue>) -> ModelState {
        ModelState { status, issues, ..Default::default() }
    }

    #[test]
    fn unscanned_installed_models_without_a_builder_are_new() {
        let reg = crate::family::registry();
        let cat = crate::catalog::build(reg, &[installed("mystery.safetensors", ""), installed("flux1-dev-fp8.safetensors", "flux1")]);
        let s = summarise(None, &cat.models, Some("0.42.5"));
        assert_eq!(s.new, ["unet:mystery.safetensors"], "a builder family runs without setup");
        assert_eq!(s.status("unet:mystery.safetensors"), Some(Status::New));
        assert_eq!(s.banner(), 1);
        let mut server = ServerState { comfyui_version: "0.42.5".into(), ..Default::default() };
        server.models.insert("unet:mystery.safetensors".into(), stored(Status::Ignored, Vec::new()));
        let s = summarise(Some(&server), &cat.models, Some("0.42.5"));
        assert!(s.new.is_empty() && s.banner() == 0, "{s:?}");
    }

    #[test]
    fn stored_statuses_count_and_carry_their_worst_issue() {
        let mut server = ServerState { comfyui_version: "0.42.5".into(), ..Default::default() };
        let info = Issue::new("LicenceUnknown", None, &[]);
        let warn = Issue::new("QuantSlow", None, &[("file", "a".into()), ("quant", "MXFP8".into()), ("gpu", "RTX 4090".into())]);
        let block = Issue::new("NodeMissing", None, &[("class", "Flux3Thing".into())]);
        server.models.insert("unet:a".into(), stored(Status::Blocked, vec![info.clone(), warn.clone(), block.clone()]));
        server.models.insert("unet:b".into(), stored(Status::NeedsReverify, vec![info, warn.clone()]));
        server.models.insert("unet:c".into(), stored(Status::Failed, Vec::new()));
        server.models.insert("unet:d".into(), ModelState { status: Status::Builtin, catalog_id: "qwen-image-21".into(), ..Default::default() });
        let s = summarise(Some(&server), &[], Some("0.43.0"));
        assert_eq!(s.models["unet:a"].issue.as_ref().map(|i| i.code.as_str()), Some("NodeMissing"));
        assert_eq!(s.models["unet:b"].issue.as_ref().map(|i| i.code.as_str()), Some("QuantSlow"));
        assert_eq!(s.status("qwen-image-21"), Some(Status::Builtin), "curated models are found by their catalogue key");
        assert_eq!((s.attention(), s.needs_reverify(), s.banner()), (3, 1, 3));
        assert_eq!(s.updated, Some(("0.42.5".into(), "0.43.0".into())));
    }

    #[test]
    fn each_precision_of_a_curated_model_keeps_its_own_status() {
        let mut server = ServerState { comfyui_version: "0.42.5".into(), ..Default::default() };
        let preset = |status, variant: &str| ModelState { status, catalog_id: "qwen".into(), variant: variant.into(), ..Default::default() };
        server.models.insert("unet:qwen_image_2.1_bf16.safetensors".into(), preset(Status::Verified, "bf16"));
        server.models.insert("unet:qwen_image_2.1_int8_convrot.safetensors".into(), preset(Status::Builtin, "int8"));
        let s = summarise(Some(&server), &[], Some("0.42.5"));
        assert_eq!(s.status_for("qwen", "bf16"), Some(Status::Verified));
        assert_eq!(s.status_for("qwen", "int8"), Some(Status::Builtin));
        assert_eq!(s.status("qwen"), Some(Status::Builtin), "the model's key is its default preset, {}", crate::catalog::default_variant(ModelId::Qwen));
    }
    #[test]
    fn file_counts_ignore_catalogue_and_precision_aliases() {
        let mut server = ServerState::default();
        for (key, status, variant) in [("unet:a", Status::NeedsReverify, "bf16"), ("unet:b", Status::Ready, "int8")] {
            server.models.insert(key.into(), ModelState { status, catalog_id: "qwen".into(), variant: variant.into(), ..Default::default() });
        }
        let s = summarise(Some(&server), &[], None);
        assert_eq!((s.attention(), s.needs_reverify(), s.banner()), (1, 1, 2));
        assert_eq!(s.new, ["unet:b"]);
        assert_eq!(s.status("unet:b"), Some(Status::Ready));
        assert_eq!(s.status_for("qwen", "bf16"), Some(Status::NeedsReverify));
    }
    #[test]
    fn a_scanned_update_keeps_the_actionable_version_context() {
        let issue = Issue::new("ReverifyNeeded", Some("unet:a"), &[("old", "0.42.5".into()), ("new", "0.43.0".into()), ("model", "A".into())]);
        let server = ServerState {
            comfyui_version: "0.43.0".into(),
            models: [("unet:a".into(), stored(Status::NeedsReverify, vec![issue]))].into(),
            ..Default::default()
        };
        let summary = summarise(Some(&server), &[], Some("0.43.0"));
        assert_eq!(summary.updated, Some(("0.42.5".into(), "0.43.0".into())));
        assert_eq!(summary.banner(), 1);
    }
}
