//! `local-image-cli ai doctor`: what every model file on the connected ComfyUI is, whether it
//! runs, and why not, with fixes; `--verify` tests models with small renders. Read-only unless
//! verifying (no downloads or moves); it never starts ComfyUI.
//!
//! Exit codes: 0 nothing blocks and no test failed, 1 blockers or failed tests, 2 usage, 3
//! ComfyUI unreachable.

use std::io::Write;
use std::path::PathBuf;

use li_ai::comfy::JobControl;
use li_ai::modelsetup::{self, Options, report};

use super::Args;

/// The models `--verify` names: `all` (every model not verified yet), a model key or a file name.
fn verify_keys(scan: &modelsetup::Scan, which: &str) -> Result<Vec<String>, String> {
    let models = &scan.server.models;
    if which == "all" {
        return Ok(models
            .iter()
            .filter(|(_, m)| m.engine == "template" && matches!(m.status, modelsetup::state::Status::Ready | modelsetup::state::Status::NeedsReverify))
            .map(|(k, _)| k.clone())
            .collect());
    }
    let wanted = |k: &str, file: &str| k == which || file == which || file.trim_end_matches(".safetensors").trim_end_matches(".gguf") == which;
    let keys: Vec<String> = models.iter().filter(|(k, m)| wanted(k, &m.file)).map(|(k, _)| k.clone()).collect();
    if keys.is_empty() {
        return Err(format!("no model `{which}` on this ComfyUI (use a file name, a key from --json, or all)"));
    }
    Ok(keys)
}

fn host_of(a: &Args) -> String {
    if let Some(h) = a.get("--host") {
        return h.to_owned();
    }
    match std::env::var("LOCAL_IMAGE_COMFY") {
        Ok(h) if !h.trim().is_empty() => h.trim().to_owned(),
        _ => li_ai::AiSettings::load().host(),
    }
}

/// Runs the doctor; returns the exit code.
pub(crate) fn doctor(a: &Args, out: &mut dyn Write, err: &mut dyn Write) -> i32 {
    if !a.positional.is_empty() {
        let _ = writeln!(err, "error: ai doctor takes no file arguments\n\n{}", super::USAGE);
        return 2;
    }
    let host = host_of(a);
    let model_dir = match a.get("--model-dir") {
        Some(d) => Some(PathBuf::from(d)),
        None if modelsetup::is_local(&host) => Some(li_ai::AiSettings::load().model_dir()),
        None => None,
    };
    let opts = Options::for_host(&host, model_dir);
    let scan = modelsetup::scan(&opts);
    let mut code = 0;
    if let Some(which) = a.get("--verify").filter(|_| scan.snapshot.is_some()) {
        let keys = match verify_keys(&scan, which) {
            Ok(k) => k,
            Err(e) => {
                let _ = writeln!(err, "error: {e}");
                return 2;
            }
        };
        let ctl = JobControl::new();
        let labels: std::collections::BTreeMap<String, String> = scan.server.models.iter().map(|(k, m)| (k.clone(), m.label.clone())).collect();
        let quiet = a.has("--json");
        let mut progress = |key: &str, step: modelsetup::verify::Step| {
            if quiet {
                return;
            }
            let r = step.result;
            let verdict = match (&r.failure, r.warnings.is_empty()) {
                (Some(code), _) => format!("FAILED ({code})"),
                (None, false) => format!("passed with a warning ({})", r.warnings.join(", ")),
                (None, true) => "passed".into(),
            };
            let extra = [
                r.cold.then(|| "first run, includes loading".to_owned()),
                r.difference.map(|d| format!("difference {d:.1}")),
                r.peak_vram_gb.map(|g| format!("GPU memory in use up to {g:.1} GB")),
            ]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>();
            let extra = if extra.is_empty() { String::new() } else { format!(" ({})", extra.join(", ")) };
            let _ =
                writeln!(out, "VERIFY {}: {} ... {verdict} in {:.1} s{extra}", labels.get(key).map(String::as_str).unwrap_or(key), step.capability, r.seconds);
        };
        match modelsetup::verify_models(&opts, &scan, &keys, &ctl, &mut progress) {
            Ok(done) => {
                for v in &done {
                    if let Err(e) = &v.result {
                        let _ = writeln!(err, "{}: not tested: {e}", v.label);
                        code = 1;
                    }
                }
            }
            Err(e) => {
                let _ = writeln!(err, "error: {e:#}");
                return 1;
            }
        }
    }
    // The report reflects what was stored (including the tests just run).
    let scan = if a.has("--verify") && scan.snapshot.is_some() { modelsetup::scan(&opts) } else { scan };
    let r = modelsetup::report(&opts, &scan);
    let written = if a.has("--json") {
        serde_json::to_string_pretty(&r).map_err(|e| e.to_string()).and_then(|s| writeln!(out, "{s}").map_err(|e| e.to_string()))
    } else {
        write!(out, "{}", report::render_text(&r)).map_err(|e| e.to_string())
    };
    if let Err(e) = written {
        let _ = writeln!(err, "error: {e}");
        return 1;
    }
    if !r.server.reachable {
        3
    } else if r.blockers() > 0 || r.failures() > 0 {
        1
    } else {
        code
    }
}
