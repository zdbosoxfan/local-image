//! Live acceptance of model setup against a dedicated, memory-capped test ComfyUI and the owner's
//! installed models (see `docs/MODEL-SETUP.md#live-test`). Every test is ignored by default and
//! refuses to run unless the server is a literal `127.0.0.1:<port>` (port 1024 or more, never
//! 8188), the model folder and a separate data folder are given as absolute paths, and the
//! server's queue is empty. It never downloads, and never starts or stops ComfyUI.
//!
//! ```sh
//! LOCAL_IMAGE_COMFY=127.0.0.1:8189 LOCAL_IMAGE_LIVE_MODELS=<models> LOCAL_IMAGE_DATA_DIR=<scratch> \
//! LOCAL_IMAGE_LIVE_REPORT=<scratch>/live-report.json \
//!   cargo test -p li-ai --test live_models -- --ignored --test-threads=1
//! ```

use std::net::{Ipv4Addr, SocketAddr};
use std::path::PathBuf;

use anyhow::{Context, Result, ensure};
use li_ai::catalog::{ModelId, TemplateModel};
use li_ai::comfy::{ComfyClient, JobControl};
use li_ai::modelsetup::{self, Options, state::Status};
use serde_json::{Value, json};

struct Live {
    host: String,
    models: PathBuf,
    data: PathBuf,
}

fn absolute(name: &str) -> Result<PathBuf> {
    let p = PathBuf::from(std::env::var_os(name).with_context(|| format!("set {name}"))?);
    ensure!(p.is_absolute(), "{name} must be an absolute path");
    Ok(p)
}

fn live() -> Result<Live> {
    let host = std::env::var("LOCAL_IMAGE_COMFY").context("set LOCAL_IMAGE_COMFY to the test server (127.0.0.1:<port>)")?;
    let address: SocketAddr = host.parse().context("LOCAL_IMAGE_COMFY must be a literal 127.0.0.1:port")?;
    ensure!(address.ip() == Ipv4Addr::LOCALHOST && address.port() >= 1024 && address.port() != 8188, "refusing a default, remote or everyday ComfyUI");
    let models = absolute("LOCAL_IMAGE_LIVE_MODELS")?;
    ensure!(models.is_dir(), "LOCAL_IMAGE_LIVE_MODELS is not a folder");
    let data = absolute("LOCAL_IMAGE_DATA_DIR")?;
    ensure!(!data.starts_with(&models), "the data folder must not be inside the model folder");
    let q = ComfyClient::new(&host).queue_state()?;
    ensure!(["queue_running", "queue_pending"].iter().all(|k| q[*k].as_array().is_some_and(Vec::is_empty)), "the test ComfyUI is busy; refusing to queue work");
    Ok(Live { host, models, data })
}

fn options(l: &Live) -> Options {
    Options {
        host: l.host.clone(),
        model_dir: Some(l.models.clone()),
        state_path: l.data.join("state/ai-models.json"),
        cache_dir: Some(l.data.join("state/templates")),
        templates_dir: None,
    }
}

const TEMPLATE_MODELS: &[(&str, &str, &str, &str, &str)] = &[
    // key, arch, variant, create template, text encoder
    ("unet:flux3-image-klein-bf16.safetensors", "flux3-klein", "distilled", "image_flux3_klein_distilled_t2i", "qwen3vl_4b_bf16.safetensors"),
    ("unet:flux3-image-klein-base-bf16.safetensors", "flux3-klein", "base", "image_flux3_klein_t2i", "qwen3vl_4b_bf16.safetensors"),
    ("unet:moss_image_sft_bf16.safetensors", "moss-image", "sft", "image_moss_image_t2i", "qwen3.5_9b_bf16.safetensors"),
    ("unet:krea2_turbo_bf16.safetensors", "krea2", "turbo", "image_krea2_turbo_t2i_int8", "qwen3vl_4b_bf16.safetensors"),
];

#[test]
#[ignore = "needs the memory-capped test ComfyUI; see docs/MODEL-SETUP.md#live-test"]
fn live_setup_identifies_matches_and_checks_the_installed_models() -> Result<()> {
    let l = live()?;
    let scan = modelsetup::scan(&options(&l));
    ensure!(scan.snapshot.is_some(), "the test ComfyUI is not reachable");
    let mut problems = Vec::new();
    for (key, arch, variant, create, te) in TEMPLATE_MODELS {
        let Some(m) = scan.server.models.get(*key) else {
            println!("skipped: {key} is not installed");
            continue;
        };
        let id = m.identity.as_ref().context("no identity")?;
        let clip = m.bindings.iter().find(|b| b.role == "clip").and_then(|b| b.used.as_deref());
        println!(
            "{key}: {} {:?} engine={} templates={:?} clip={clip:?} issues={:?}",
            m.label,
            m.status,
            m.engine,
            m.templates,
            m.issues.iter().map(|i| &i.code).collect::<Vec<_>>()
        );
        if id.arch.as_deref() != Some(*arch) || id.variant.as_deref() != Some(*variant) {
            problems.push(format!("{key}: identity {:?}/{:?}", id.arch, id.variant));
        }
        if m.templates.get("create").map(String::as_str) != Some(*create) {
            problems.push(format!("{key}: create template {:?}", m.templates.get("create")));
        }
        if clip != Some(*te) {
            problems.push(format!("{key}: text encoder {clip:?}"));
        }
        if !m.status.usable() {
            problems.push(format!("{key}: {:?} {:?}", m.status, m.issues.iter().map(|i| i.english()).collect::<Vec<_>>()));
        }
        if m.issues.iter().any(|i| i.code == "VramLow") {
            problems.push(format!("{key}: VramLow"));
        }
    }
    for curated in ["unet:qwen_image_2.1_bf16.safetensors", "unet:qwen_image_2.1_int8_convrot.safetensors", "unet:flux-2-klein-9b-fp8.safetensors"] {
        if let Some(m) = scan.server.models.get(curated) {
            println!("{curated}: {} {:?} engine={}", m.label, m.status, m.engine);
            if m.engine != "builtin" || m.status != Status::Builtin {
                problems.push(format!("{curated}: {} {:?} {:?}", m.engine, m.status, m.issues.iter().map(|i| i.english()).collect::<Vec<_>>()));
            }
        }
    }
    println!("{}", modelsetup::report::render_text(&modelsetup::report(&options(&l), &scan)));
    ensure!(problems.is_empty(), "{}", problems.join("\n"));
    Ok(())
}

fn record(report: &mut Vec<Value>, key: &str, label: &str, v: &modelsetup::state::Verified) {
    for (cap, r) in &v.capabilities {
        report.push(json!({
            "model": key, "label": label, "capability": cap, "seed": v.seed, "pass": r.pass, "seconds": r.seconds, "cold": r.cold,
            "peak_vram_gb": r.peak_vram_gb, "size": r.size, "metrics": r.metrics, "difference": r.difference,
            "warnings": r.warnings, "failure": r.failure, "message": r.message,
        }));
    }
}

fn write_report(name: &str, report: &[Value]) -> Result<()> {
    if let Some(path) = std::env::var_os("LOCAL_IMAGE_LIVE_REPORT") {
        let path = PathBuf::from(path).with_extension(format!("{name}.json"));
        std::fs::write(&path, serde_json::to_vec_pretty(report)?)?;
        println!("report: {}", path.display());
    }
    Ok(())
}

#[test]
#[ignore = "needs the memory-capped test ComfyUI; see docs/MODEL-SETUP.md#live-test"]
fn live_template_models_verify_one_at_a_time() -> Result<()> {
    let l = live()?;
    let opts = options(&l);
    let scan = modelsetup::scan(&opts);
    let keys: Vec<String> = TEMPLATE_MODELS.iter().map(|t| t.0.to_owned()).filter(|k| scan.server.models.contains_key(k)).collect();
    let mut report = Vec::new();
    let done = modelsetup::verify_models(&opts, &scan, &keys, &JobControl::new(), &mut |key, step| {
        let r = step.result;
        println!(
            "VERIFY {key}: {} pass={} {:.1} s cold={} vram={:?} failure={:?} warnings={:?} difference={:?}",
            step.capability, r.pass, r.seconds, r.cold, r.peak_vram_gb, r.failure, r.warnings, r.difference
        );
    })?;
    let mut failed = Vec::new();
    for v in &done {
        match &v.result {
            Ok(ver) => {
                record(&mut report, &v.key, &v.label, ver);
                if ver.capabilities.get("create").is_none_or(|c| !c.pass) {
                    failed.push(format!("{}: create failed", v.key));
                }
                for (cap, r) in ver.capabilities.iter().filter(|(_, r)| !r.pass) {
                    println!("{}: {cap} failed: {:?} {:?}", v.key, r.failure, r.message);
                }
            }
            Err(e) => failed.push(format!("{}: {e}", v.key)),
        }
    }
    write_report("verify", &report)?;
    ensure!(failed.is_empty(), "{}", failed.join("\n"));
    Ok(())
}

/// Verifies `key` with one companion forced to `file` (bypassing every guard), for calibration.
fn verify_with(l: &Live, scan: &modelsetup::Scan, key: &str, role: &str, file: &str) -> Result<modelsetup::state::Verified> {
    let m = scan.server.models.get(key).context("not installed")?;
    let mut recipe = m.recipe.clone().context("no recipe")?;
    for g in [&mut recipe.create, &mut recipe.edit, &mut recipe.style].into_iter().flatten() {
        for b in g.files.iter_mut().filter(|b| b.role == role) {
            b.used = Some(file.to_owned());
        }
    }
    let family = li_ai::template::compile::family_for(
        &recipe,
        li_ai::family::registry().family(m.identity.as_ref().and_then(|i| i.arch.as_deref()).unwrap_or("")),
        "negative-control",
        &m.label,
    );
    let label = format!("{} (control)", m.label);
    li_ai::catalog::set_templates([(key.to_owned(), TemplateModel { recipe: std::sync::Arc::new(recipe), family, label: label.clone() })].into());
    let target = modelsetup::verify::Target {
        model: ModelId::from_key(key).context("not in the catalogue")?,
        label: &label,
        quant: String::new(),
        te: String::new(),
        vae: String::new(),
    };
    let v = modelsetup::verify::verify(&l.host, &target, 518_877_231, &JobControl::new(), &mut |s| {
        println!(
            "CONTROL {key} {role}={file}: {} pass={} failure={:?} warnings={:?} metrics={:?}",
            s.capability, s.result.pass, s.result.failure, s.result.warnings, s.result.metrics
        );
    });
    let _ = ComfyClient::new(&l.host).free_memory();
    v
}

#[test]
#[ignore = "needs the memory-capped test ComfyUI; see docs/MODEL-SETUP.md#live-test"]
fn live_negative_controls_fail_a_check() -> Result<()> {
    let l = live()?;
    let scan = modelsetup::scan(&options(&l));
    let mut report = Vec::new();
    let mut unseparated = Vec::new();
    for (key, role, file) in
        [("unet:moss_image_sft_bf16.safetensors", "vae", "ae.safetensors"), ("unet:flux3-image-klein-bf16.safetensors", "clip", "qwen_3_4b.safetensors")]
    {
        if !scan.server.models.contains_key(key) {
            println!("skipped: {key} is not installed");
            continue;
        }
        match verify_with(&l, &scan, key, role, file) {
            Ok(v) => {
                record(&mut report, key, &format!("{role}={file}"), &v);
                let caught = v.capabilities.values().any(|c| !c.pass || !c.warnings.is_empty());
                if !caught {
                    unseparated.push(format!("{key} with {role} {file}"));
                }
            }
            Err(e) => println!("{key} with {role} {file}: the run failed ({e:#}), which is caught"),
        }
    }
    write_report("controls", &report)?;
    // Restore the real recipes.
    modelsetup::scan(&options(&l));
    ensure!(unseparated.is_empty(), "no check separated: {}", unseparated.join(", "));
    Ok(())
}
