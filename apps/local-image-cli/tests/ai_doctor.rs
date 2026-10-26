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
