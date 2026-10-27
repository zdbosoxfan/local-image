//! Explicit private-runtime API acceptance. Does not exercise native editor/document publication.
//! Run only with no other AI work on the machine, using the generation fixture launcher.
#![expect(clippy::let_underscore_must_use, clippy::map_err_ignore, reason = "baseline 2026-10-25: unreviewed")]

use anyhow::{Context, Result, ensure};
use image::{Rgba, RgbaImage};
use li_ai::{Ai, GenerateMode, GenerateRequest, JobControl, ModelId, Stage, ops::Generated};
use lightcraft_sysmem::jobs::Cancelled;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    fs,
    net::{Ipv4Addr, SocketAddr},
    path::{Path, PathBuf},
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

// Request cancellation after five minutes. The production client still owns/drains the prompt
// until a terminal state or its existing finite 20-minute deadline; no thread is detached.
const WORK_BUDGET: Duration = Duration::from_secs(300);
const OBSERVE_BUDGET: Duration = Duration::from_secs(60);

fn required_path(name: &str) -> Result<PathBuf> {
    let path = PathBuf::from(std::env::var_os(name).with_context(|| format!("set {name} explicitly"))?);
    ensure!(path.is_absolute(), "{name} must be an absolute isolated path");
    path.canonicalize().with_context(|| format!("resolve {name}"))
}

fn write_json(path: &Path, value: &Value) -> Result<()> {
    fs::write(path, serde_json::to_vec_pretty(value)?)?;
    Ok(())
}

fn owned_inputs() -> Result<(String, PathBuf, Value)> {
    let host = std::env::var("LOCAL_IMAGE_COMFY").context("set the owned LOCAL_IMAGE_COMFY explicitly")?;
    let address: SocketAddr = host.parse().context("LOCAL_IMAGE_COMFY must be literal 127.0.0.1:port")?;
    ensure!(address.ip() == Ipv4Addr::LOCALHOST && address.port() >= 1024 && address.port() != 8188, "refuse default/owner/nonloopback host");
    let models = required_path("LOCAL_IMAGE_GENERATION_MODELS")?;
    let generation_manifest = required_path("LOCAL_IMAGE_AI_GENERATION_MANIFEST")?;
    let owned_receipt = required_path("LOCAL_IMAGE_AI_OWNED_RUNTIME_RECEIPT")?;
    let runtime_manifest = required_path("LOCAL_IMAGE_AI_RUNTIME_MANIFEST")?;
    let runtime = runtime_manifest.parent().context("runtime manifest has no private parent")?;
    let manifest_bytes = fs::read(generation_manifest)?;
    let manifest: Value = serde_json::from_slice(&manifest_bytes)?;
    let receipt: Value = serde_json::from_slice(&fs::read(owned_receipt)?)?;
    let identity: Value = serde_json::from_slice(&fs::read(&runtime_manifest)?)?;
    ensure!(receipt["generation"] == true && receipt["port"].as_u64() == Some(u64::from(address.port())), "owned generation runtime/port mismatch");
    ensure!(receipt["unit"].as_str().is_some_and(|unit| !unit.is_empty()), "missing owned unit identity");
    ensure!(receipt["unit"] == identity["unit"] && receipt["description"] == identity["description"], "runtime receipt identity mismatch");
    ensure!(receipt["generation_manifest_sha256"] == format!("{:x}", Sha256::digest(&manifest_bytes)), "generation manifest changed since startup");
    ensure!(manifest["client_root"].as_str() == models.to_str(), "generation client view mismatch");
    let files = manifest["files"].as_array().context("generation receipt files missing")?;
    ensure!(files.len() == 5, "use exactly the verified five-file fixture bundle");
    for file in files {
        let relative = Path::new(file["relative"].as_str().context("fixture relative path missing")?);
        ensure!(relative.components().all(|part| matches!(part, std::path::Component::Normal(_))), "invalid fixture relative path");
        let path = models.join(relative);
        ensure!(fs::symlink_metadata(&path)?.file_type().is_symlink(), "fixture view must use the prepared links");
        ensure!(path.canonicalize()?.to_str() == file["path"].as_str(), "fixture link changed");
        ensure!(file["verified"] == true && Some(fs::metadata(path)?.len()) == file["bytes"].as_u64(), "fixture receipt/size mismatch");
    }
    let data = required_path("LOCAL_IMAGE_DATA_DIR")?;
    ensure!(data.starts_with(runtime) && data != runtime && !data.starts_with(&models), "outputs must be a separate private runtime data directory");
    let output = data.join(format!("real-generation-{}", uuid::Uuid::new_v4().simple()));
    fs::create_dir(&output)?;
    Ok((host, output, json!({"unit":receipt["unit"],"port":address.port(),"generation_manifest_sha256":receipt["generation_manifest_sha256"]})))
}

fn queue_ids(queue: &Value, key: &str) -> Result<Vec<String>> {
    queue[key]
        .as_array()
        .with_context(|| format!("missing {key}"))?
        .iter()
        .map(|item| item.get(1).and_then(Value::as_str).map(str::to_owned).context("unexpected Comfy queue item"))
        .collect()
}

fn idle(ai: &Ai) -> Result<()> {
    let queue = ai.client.queue_state()?;
    ensure!(queue_ids(&queue, "queue_running")?.is_empty() && queue_ids(&queue, "queue_pending")?.is_empty(), "owned acceptance queue is not idle");
    ensure!(li_ai::comfy::our_prompts(ai.client.host()).is_empty(), "this test still owns an unfinished prompt");
    Ok(())
}

fn nontrivial(image: &RgbaImage, expected: (u32, u32)) -> Result<()> {
    ensure!(image.dimensions() == expected, "unexpected output dimensions");
    ensure!(image.pixels().all(|pixel| pixel[3] == 255), "opaque Klein output must remain opaque");
    let first = image.get_pixel(0, 0);
    ensure!(image.pixels().any(|pixel| pixel.0[..3] != first.0[..3]), "constant output is not inference acceptance");
    Ok(())
}

fn complete(ai: &Ai, request: &GenerateRequest, output: &Path, label: &str, expected: (u32, u32)) -> Result<Generated> {
    idle(ai)?;
    let control = JobControl::new();
    let started = Instant::now();
    let generated = thread::scope(|scope| -> Result<Generated> {
        let (done, finished) = mpsc::channel::<()>();
        let watched = control.clone();
        let watcher = scope.spawn(move || {
            if matches!(finished.recv_timeout(WORK_BUDGET), Err(mpsc::RecvTimeoutError::Timeout)) {
                watched.cancel();
            }
        });
        let result = ai.generate_detailed(request, &control);
        let _ = done.send(());
        watcher.join().map_err(|_| anyhow::anyhow!("generation deadline watcher panicked"))?;
        result
    })?;
    ensure!(!control.is_cancelled() && control.progress().stage == Stage::Done, "generation did not complete normally");
    nontrivial(&generated.image, expected)?;
    ensure!(generated.parameters["steps"] == 4 && generated.parameters["guidance"] == 1.0, "production Klein defaults changed");
    ensure!(generated.parameters["sampler"] == "euler" && generated.parameters["scheduler"] == "simple", "production sampler defaults changed");
    generated.image.save(output.join(format!("{label}.png")))?;
    write_json(&output.join(format!("{label}.json")), &json!({"elapsed_seconds":started.elapsed().as_secs_f64(),"parameters":generated.parameters}))?;
    idle(ai)?;
    Ok(generated)
}

fn cancel_one_running(ai: &Ai, request: &GenerateRequest, output: &Path) -> Result<()> {
    idle(ai)?;
    let control = JobControl::new();
    let started = Instant::now();
    let (observation, result) = thread::scope(|scope| {
        let worker = scope.spawn(|| ai.generate_detailed(request, &control));
        // No assertions/early returns before cancelling our control and joining the worker.
        let observation = (|| -> Result<Value> {
            loop {
                ensure!(started.elapsed() < OBSERVE_BUDGET, "did not observe a running owned prompt before the observation deadline");
                let ours = li_ai::comfy::our_prompts(ai.client.host());
                ensure!(ours.len() <= 1, "unexpected second prompt owned by this test process");
                if let Some(id) = ours.first() {
                    let queue = ai.client.queue_state()?;
                    let running = queue_ids(&queue, "queue_running")?;
                    let pending = queue_ids(&queue, "queue_pending")?;
                    ensure!(running.iter().chain(&pending).all(|other| other == id), "another prompt entered the exclusive acceptance interval");
                    if running.contains(id) {
                        return Ok(json!({"prompt_id":id,"observed_running":true,"observed_stage":format!("{:?}",control.progress().stage)}));
                    }
                }
                ensure!(!worker.is_finished(), "request finished before running cancellation could be observed");
                thread::sleep(Duration::from_millis(25));
            }
        })();
        // This is the sole cancellation call: never interrupt_ours, global /interrupt or queue clearing.
        control.cancel();
        (observation, worker.join())
    });
    let observed = observation?;
    let result = result.map_err(|_| anyhow::anyhow!("cancelled generation worker panicked"))?;
    let error = result.err().context("cancelled request returned image data")?;
    ensure!(error.is::<Cancelled>() && control.progress().stage == Stage::Cancelled, "expected typed terminal cancellation, got {error:#}");
    idle(ai)?;
    let id = observed["prompt_id"].as_str().context("missing observed prompt id")?;
    let history = ai.client.history(id)?;
    write_json(
        &output.join("cancel.json"),
        &json!({"observed":observed,"elapsed_seconds":started.elapsed().as_secs_f64(),"returned_pixels":false,
            "terminal_stage":"Cancelled","history_status":history.as_ref().and_then(|entry|entry.get("status")),
            "scope":"one API job; not competing-prompt survival or native publication"}),
    )?;
    Ok(())
}

#[test]
#[ignore = "coordinator only: verified generation fixture view, owned idle CUDA RTX5090 Comfy and exclusive AI interval"]
fn real_klein_create_edit_reference_and_targeted_cancel() -> Result<()> {
    let (host, output, ownership) = owned_inputs()?;
    let ai = Ai::new(host);
    let stats = ai.client.system_stats()?;
    ensure!(
        stats["devices"]
            .as_array()
            .context("missing devices")?
            .iter()
            .any(|device| { device["type"] == "cuda" && device["name"].as_str().is_some_and(|name| name.contains("RTX 5090")) }),
        "actual CUDA RTX5090 required"
    );
    idle(&ai)?;
    write_json(&output.join("preflight.json"), &json!({"ownership":ownership,"devices":stats["devices"],"status":"started"}))?;
    eprintln!("REAL_GENERATION output={}", output.display());
    let info = ai.client.object_info()?;
    let preset = li_ai::catalog::preset(ModelId::Klein4B, "bf16").context("missing Klein4B BF16 preset")?;
    let available = li_ai::catalog::availability(preset, &info);
    ensure!(available.available, "{}", available.reason);
    for node in li_ai::builders::task_nodes(&ModelId::Klein4B.info().resolved, li_ai::builders::Task::Edit, 1) {
        ensure!(info.has_node(&node), "missing production Edit/reference node {node}");
    }
    let mut create = GenerateRequest::new(ModelId::Klein4B, "A red ceramic cube on a blue table, studio photograph, clear shadows");
    ensure!(create.steps == 4 && create.guidance == 1.0 && create.variant == "bf16", "use unmodified production Klein defaults");
    create.width = 256;
    create.height = 256;
    create.seed = 426001;
    complete(&ai, &create, &output, "create", (256, 256))?;
    let source = RgbaImage::from_fn(384, 256, |x, y| {
        if (112..272).contains(&x) && (64..208).contains(&y) { Rgba([210, 35, 30, 255]) } else { Rgba([30, 70 + (y / 4) as u8, 160, 255]) }
    });
    let reference = RgbaImage::from_fn(256, 256, |x, y| Rgba([30, 90 + (x / 3) as u8, 30 + (y / 3) as u8, 255]));
    source.save(output.join("source.png"))?;
    reference.save(output.join("reference.png"))?;
    let mut edit = GenerateRequest::new(ModelId::Klein4B, "Change the red block to green using the reference colors, preserve the scene composition");
    edit.mode = GenerateMode::Edit;
    edit.seed = 426002;
    edit.source = Some(source.clone());
    edit.references.push(reference.clone());
    // Leave request defaults/production work_size + reference scaling intact (native 1024).
    ensure!(edit.width == 1024 && edit.height == 1024 && edit.steps == 4 && edit.guidance == 1.0, "unexpected Edit defaults");
    let edited = complete(&ai, &edit, &output, "edit-reference", source.dimensions())?;
    ensure!(edited.image != source, "Edit returned the unchanged source");
    ensure!(edit.source.as_ref() == Some(&source) && edit.references == [reference], "API mutated input images");
    edit.seed = 426003;
    cancel_one_running(&ai, &edit, &output)?;
    create.seed = 426004;
    complete(&ai, &create, &output, "retry-after-cancel", (256, 256))?;
    write_json(
        &output.join("accepted.json"),
        &json!({"create":true,"edit_reference":true,"single_job_cancel":true,"retry":true,
        "native_ui_target_ownership":false,"native_library_publication":false,"competing_prompt_survival":false}),
    )?;
    Ok(())
}
