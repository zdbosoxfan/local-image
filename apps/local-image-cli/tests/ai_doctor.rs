//! `local-image-cli ai doctor` against the mock ComfyUI serving ComfyUI 0.42.5's nodes and
//! official templates, with a temporary model folder of header-only weights.

use std::path::PathBuf;
use std::process::Command;

use li_ai::mock::{self, MockComfy};
use li_ai::modelsetup::report::Report;

struct Dir(PathBuf);

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn tmp(name: &str) -> Dir {
    let d = std::env::temp_dir().join(format!("li-doctor-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    Dir(d)
}

/// FLUX.3 Klein (distilled) and MOSS-Image SFT with the companions their templates need.
fn models(name: &str) -> Dir {
    let d = tmp(name);
    for (file, fixture) in [
        ("diffusion_models/flux3-image-klein-bf16.safetensors", "owner__diffusion_models~flux3-image-klein-bf16"),
        ("diffusion_models/moss_image_sft_bf16.safetensors", "owner__diffusion_models~moss_image_sft_bf16"),
        ("text_encoders/qwen3vl_4b_bf16.safetensors", "Comfy-Org_Krea-2__text_encoders~qwen3vl_4b_bf16"),
        ("text_encoders/qwen3.5_9b_bf16.safetensors", "Comfy-Org_MOSS-Image__text_encoders~qwen3.5_9b_bf16"),
        ("vae/flux2-vae.safetensors", "local__vae~flux2-vae"),
    ] {
        mock::write_header_only(&d.0.join(file), &mock::header_fixture(fixture)).unwrap();
    }
    d
}

fn doctor(host: &str, dir: &Dir, data: &Dir, extra: &[&str]) -> (i32, String, String) {
    let o = Command::new(env!("CARGO_BIN_EXE_local-image-cli"))
        .args(["ai", "doctor", "--host", host, "--model-dir"])
        .arg(&dir.0)
        .args(extra)
        .env("LOCAL_IMAGE_DATA_DIR", &data.0)
        .output()
        .unwrap();
    (o.status.code().unwrap_or(-1), String::from_utf8_lossy(&o.stdout).into_owned(), String::from_utf8_lossy(&o.stderr).into_owned())
}

#[test]
fn the_doctor_lists_template_models_as_ready_and_prints_json() {
    let (dir, data) = (models("ready"), tmp("ready-data"));
    let server = MockComfy::start().unwrap();
    server.serve_comfy_0_42_5(&dir.0);
    let (code, out, err) = doctor(server.host(), &dir, &data, &[]);
    assert_eq!(code, 0, "{out}\n{err}");
    assert!(out.contains("ComfyUI  127.0.0.1:") && out.contains("v0.42.5 (local-git)  templates 0.11.96 from the server"), "{out}");
    assert!(out.contains("GPU      NVIDIA GeForce RTX 5090, 33.7 GB (Blackwell)"), "{out}");
    assert!(out.contains("READY    FLUX.3 Klein Distilled · BF16"), "{out}");
    assert!(out.contains("image_flux3_klein_distilled_t2i + image_flux3_klein_distilled_image_edit"), "{out}");
    assert!(out.contains("Run `local-image-cli ai doctor --verify all` to test the 2 untested models"), "{out}");
    let (code, json, _) = doctor(server.host(), &dir, &data, &["--json"]);
    assert_eq!(code, 0);
    let r: Report = serde_json::from_str(&json).unwrap();
    assert_eq!((r.report_version, r.summary.not_tested, r.blockers()), (1, 2, 0));
    // Read-only: nothing was queued.
    assert!(server.prompts().is_empty());
}

#[test]
fn templates_can_come_from_a_folder_when_the_server_serves_none() {
    let (dir, data) = (models("templates-dir"), tmp("templates-dir-data"));
    let server = MockComfy::start().unwrap();
    server.serve_comfy_0_42_5(&dir.0);
    let empty = tmp("templates-dir-empty");
    server.set_templates_dir(&empty.0);
    let (_, out, _) = doctor(server.host(), &dir, &data, &[]);
    assert!(out.contains("doesn't serve its templates") && out.contains("NO MATCH"), "no templates, no template models: {out}");
    let templates = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../crates/li-ai/tests/fixtures/comfy-0.42.5/templates");
    let (code, out, err) = doctor(server.host(), &dir, &data, &["--templates-dir", templates.to_str().unwrap()]);
    assert_eq!(code, 0, "{out}\n{err}");
    assert!(out.contains("READY    FLUX.3 Klein Distilled · BF16"), "{out}");
}

#[test]
fn verify_tests_one_model_at_a_time_and_frees_memory_after_each() {
    let (dir, data) = (models("verify"), tmp("verify-data"));
    let server = MockComfy::start().unwrap();
    server.serve_comfy_0_42_5(&dir.0);
    let (code, out, err) = doctor(server.host(), &dir, &data, &["--verify", "all"]);
    assert!(code == 0 || code == 1, "{out}\n{err}");
    assert!(out.contains("VERIFY FLUX.3 Klein Distilled · BF16: create ..."), "{out}");
    assert!(out.contains(": refine ..."), "{out}");
    assert_eq!(server.frees(), 2, "{out}");
    assert!(!out.contains("READY    FLUX.3"), "tested models are no longer listed as untested: {out}");
}

#[test]
fn usage_errors_and_an_unreachable_server_have_their_own_exit_codes() {
    let (dir, data) = (tmp("usage"), tmp("usage-data"));
    let (code, out, _) = doctor("127.0.0.1:9", &dir, &data, &[]);
    assert_eq!(code, 3);
    assert!(out.contains("ComfyUI  127.0.0.1:9  not reachable") && out.contains("ComfyUI is not reachable at 127.0.0.1:9."), "{out}");
    assert_eq!(doctor("127.0.0.1:9", &dir, &data, &["--frobnicate"]).0, 2);
    let bin = || Command::new(env!("CARGO_BIN_EXE_local-image-cli"));
    assert_eq!(bin().arg("ai").output().unwrap().status.code(), Some(2));
    assert_eq!(bin().args(["ai", "doctor", "--help"]).output().unwrap().status.code(), Some(0));
}

#[cfg(unix)]
#[test]
fn ctrl_c_interrupts_the_owned_render_and_exits_130() {
    use std::process::Stdio;
    use std::time::{Duration, Instant};
    let (dir, data) = (models("cancel"), tmp("cancel-data"));
    let server = MockComfy::start().unwrap();
    server.serve_comfy_0_42_5(&dir.0);
    server.delay_ms.store(30_000, std::sync::atomic::Ordering::SeqCst);
    let mut child = Command::new(env!("CARGO_BIN_EXE_local-image-cli"))
        .args(["ai", "doctor", "--host", server.host(), "--model-dir"])
        .arg(&dir.0)
        .args(["--verify", "all"])
        .env("LOCAL_IMAGE_DATA_DIR", &data.0)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let until = Instant::now() + Duration::from_secs(15);
    while server.prompts().is_empty() && Instant::now() < until {
        if let Some(status) = child.try_wait().unwrap() {
            panic!("doctor exited before rendering: {status}");
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    assert!(!server.prompts().is_empty(), "doctor must have submitted a render");
    let client = li_ai::ComfyClient::new(server.host());
    let queue = client.queue_state().unwrap();
    let owned = queue["queue_running"][0][1].as_str().expect("the doctor's render is running").to_owned();
    assert!(Command::new("kill").args(["-INT", &child.id().to_string()]).status().unwrap().success());
    let until = Instant::now() + Duration::from_secs(15);
    while child.try_wait().unwrap().is_none() && Instant::now() < until {
        std::thread::sleep(Duration::from_millis(25));
    }
    if child.try_wait().unwrap().is_none() {
        child.kill().unwrap();
        panic!("doctor did not stop after Ctrl-C");
    }
    let output = child.wait_with_output().unwrap();
    assert_eq!(output.status.code(), Some(130), "{}", String::from_utf8_lossy(&output.stderr));
    let history = client.history(&owned).unwrap().expect("cancelled prompt history");
    assert_eq!(history["status"]["status_str"], "error");
    assert!(history["status"]["messages"].to_string().contains("execution_interrupted"));
    let queue = client.queue_state().unwrap();
    assert!(queue["queue_running"].as_array().unwrap().is_empty() && queue["queue_pending"].as_array().unwrap().is_empty());
    assert_eq!(server.interrupts(), 0, "cancellation uses the owned job endpoint, never a global interrupt");
    assert_eq!(server.frees(), 1, "cancel still releases model memory");
    assert_eq!(server.prompts().len(), 1, "no later test is submitted");
}

#[cfg(unix)]
#[test]
fn ctrl_c_after_verification_exits_130_during_the_report() {
    use std::io::Read;
    use std::process::Stdio;
    let (dir, data) = (models("report-cancel"), tmp("report-cancel-data"));
    // Keep the JSON report larger than a pipe so the parent can interrupt its writer.
    let source = dir.0.join("diffusion_models/flux3-image-klein-bf16.safetensors");
    for i in 0..128 {
        std::fs::copy(&source, dir.0.join(format!("diffusion_models/flux3-image-klein-copy-{i}.safetensors"))).unwrap();
    }
    let server = MockComfy::start().unwrap();
    server.serve_comfy_0_42_5(&dir.0);
    let mut child = Command::new(env!("CARGO_BIN_EXE_local-image-cli"))
        .args(["ai", "doctor", "--host", server.host(), "--model-dir"])
        .arg(&dir.0)
        .args(["--verify", "flux3-image-klein-bf16.safetensors", "--json"])
        .env("LOCAL_IMAGE_DATA_DIR", &data.0)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut first = [0];
    child.stdout.as_mut().unwrap().read_exact(&mut first).unwrap();
    assert_eq!(first, [b'{'], "JSON output starts only after verification and the final scan");
    assert_eq!(server.frees(), 1, "the owned verification has finished");
    assert!(Command::new("kill").args(["-INT", &child.id().to_string()]).status().unwrap().success());
    let output = child.wait_with_output().unwrap();
    assert_eq!(output.status.code(), Some(130), "{}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(server.interrupts(), 0, "no global interrupt after verification");
}
