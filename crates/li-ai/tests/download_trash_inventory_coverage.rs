use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, UNIX_EPOCH};

use serde_json::json;

use li_ai::arch::FileKind;
use li_ai::catalog::{FileSpec, Preset, Role};
use li_ai::comfy::ObjectInfo;
use li_ai::download::{
    allow_test_host, hash_file, host_allowed, human_bytes, listed_file, missing_files, removal_plan, remove_listed_file, remove_preset, target_path,
    verify_existing,
};
use li_ai::family::Role as FamilyRole;
use li_ai::inventory::{detect_file, family_of, label_for, listed_as_model_but_lora, pattern_matches, resolve_components, scan};
use li_ai::trash::{available, deletion_date, encode_path, home_trash, move_to_trash};

struct TempDir(PathBuf);

impl TempDir {
    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn temp_dir(label: &str) -> TempDir {
    let dir = std::env::temp_dir().join(format!("li-ai-{}-{}", label, uuid::Uuid::new_v4().simple()));
    std::fs::create_dir_all(&dir).unwrap();
    TempDir(dir)
}

fn file_spec_with_content(dir: &Path, folder: &str, name: &str, body: &[u8]) -> FileSpec {
    let path = dir.join(folder).join(name);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, body).unwrap();
    let (bytes, sha256) = hash_file(&path).unwrap();
    FileSpec {
        role: Role::Unet,
        folder: folder.into(),
        name: name.into(),
        bytes,
        sha256,
        url: format!("https://huggingface.co/x/{name}"),
        compatible: Vec::new(),
    }
}

fn preset(model: &str, files: Vec<FileSpec>) -> Preset {
    Preset {
        model: li_ai::catalog::ModelId::intern(model),
        variant: "bf16".into(),
        label: "BF16".into(),
        files,
        access_url: None,
        required_nodes: Vec::new(),
        required_choices: Vec::new(),
        installed: false,
    }
}

fn safetensors_bytes(header: &serde_json::Value) -> Vec<u8> {
    let header = serde_json::to_vec(header).unwrap();
    let mut bytes = (header.len() as u64).to_le_bytes().to_vec();
    bytes.extend(header);
    bytes
}

fn sdxl_header() -> serde_json::Value {
    json!({
        "model.diffusion_model.input_blocks.0.0.weight": {"dtype": "F16", "shape": [320, 4, 3, 3], "data_offsets": [0, 0]},
        "model.diffusion_model.label_emb.0.0.weight": {"dtype": "F16", "shape": [1280, 2816], "data_offsets": [0, 0]},
        "conditioner.embedders.0.transformer.text_model.final_layer_norm.weight": {"dtype": "F16", "shape": [768], "data_offsets": [0, 0]}
    })
}

fn flux_lora_header() -> serde_json::Value {
    json!({
        "double_blocks.18.img_attn.qkv.lora_A.weight": {"dtype": "F16", "shape": [16, 3072], "data_offsets": [0, 0]},
        "double_blocks.18.img_attn.qkv.lora_B.weight": {"dtype": "F16", "shape": [9216, 16], "data_offsets": [0, 0]}
    })
}

fn write_safetensors(dir: &Path, relative: &str, header: &serde_json::Value) -> PathBuf {
    let p = dir.join(relative);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(&p, safetensors_bytes(header)).unwrap();
    p
}

fn info() -> ObjectInfo {
    ObjectInfo(json!({
        "CheckpointLoaderSimple": {"input": {"required": {"ckpt_name": [["juggernautXL_v9.safetensors", "dreamshaper_8.safetensors", "mystery.safetensors", "sdxl_detail_LoRA.safetensors"]]}}},
        "UNETLoader": {"input": {"required": {"unet_name": [["flux1-dev-fp8.safetensors", "qwen_image_edit_2511_bf16.safetensors", "flux-2-klein-4b.safetensors", "flux_realism_lora.safetensors", "loras/qwen_style.safetensors", "flux_ink.safetensors"]]}}},
        "CLIPLoader": {"input": {"required": {"clip_name": [["qwen_2.5_vl_7b_fp8_scaled.safetensors", "qwen_3_4b.safetensors", "clip_l.safetensors", "t5xxl_fp8_e4m3fn_scaled.safetensors"]]}}},
        "DualCLIPLoader": {"input": {"required": {"clip_name1": [["clip_l.safetensors", "t5xxl_fp8_e4m3fn_scaled.safetensors"]]}}},
        "VAELoader": {"input": {"required": {"vae_name": [["ae.safetensors", "qwen_image_vae.safetensors", "flux2-vae.safetensors"]]}}},
        "LoraLoader": {"input": {"required": {"lora_name": [["pony_style.safetensors", "flux_realism.safetensors", "misc.safetensors", "flux_realism_lora.safetensors"]]}}},
        "CLIPVisionLoader": {"input": {"required": {"clip_name": [["sigclip_vision_patch14_384.safetensors"]]}}},
        "StyleModelLoader": {"input": {"required": {"style_model_name": [["redux.safetensors"]]}}},
        "UpscaleModelLoader": {"input": {"required": {"model_name": [["RealESRGAN_x4.pth"]]}}}
    }))
}

#[test]
fn host_allowed_accepts_https_allow_list_and_suffixes() {
    assert!(host_allowed("https://huggingface.co/x/resolve/abc/f.safetensors"));
    assert!(host_allowed("https://github.com/org/repo/releases/download/v1/model.bin"));
    assert!(host_allowed("https://cas-bridge.xethub.hf.co/x"));
    assert!(host_allowed("https://abc.def.r2.cloudflarestorage.com/x"));
    assert!(host_allowed("https://cdn-lfs.huggingface.co/x"));
    assert!(!host_allowed("ftp://huggingface.co/x"));
}

#[test]
fn host_allowed_allows_http_only_for_registered_test_hosts() {
    let authority = "127.0.0.1:12345";
    assert!(!host_allowed(&format!("http://{authority}/file")));
    allow_test_host(authority);
    assert!(host_allowed(&format!("http://{authority}/file")));
    assert!(host_allowed("http://127.0.0.1:12345/file"));
    assert!(!host_allowed("http://127.0.0.1:54321/file"));
    assert!(!host_allowed("https://127.0.0.1:12345/file"));
    allow_test_host("localhost:9999");
    assert!(host_allowed("http://localhost:9999/file"));
}

#[test]
fn host_allowed_rejects_spoofed_subdomains() {
    assert!(!host_allowed("https://huggingface.co.evil.com/x"));
    assert!(!host_allowed("https://raw.githubusercontent.com.evil.com/x"));
    assert!(!host_allowed("https://example.com/x"));
    assert!(!host_allowed("https://github.com@evil.com/x"));
}

#[test]
fn hash_file_reads_size_and_sha256() {
    let dir = temp_dir("hash");
    let p = dir.path().join("data.bin");
    std::fs::write(&p, b"abc").unwrap();
    let (len, sha) = hash_file(&p).unwrap();
    assert_eq!(len, 3);
    assert_eq!(sha, "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
}

#[test]
fn verify_existing_accepts_matching_file() {
    let dir = temp_dir("verify-ok");
    let spec = file_spec_with_content(dir.path(), "", "model.safetensors", b"hello");
    let path = dir.path().join("model.safetensors");
    assert!(verify_existing(&path, &spec).unwrap());
    assert!(!verify_existing(&dir.path().join("missing.bin"), &spec).unwrap());
}

#[test]
fn verify_existing_rejects_wrong_size_or_hash() {
    let dir = temp_dir("verify-bad");
    let p = dir.path().join("f.bin");
    std::fs::write(&p, b"abc").unwrap();
    let (len, sha) = hash_file(&p).unwrap();
    let spec = FileSpec {
        role: Role::Unet,
        folder: "".into(),
        name: "f.bin".into(),
        bytes: len,
        sha256: sha,
        url: "https://huggingface.co/f".into(),
        compatible: Vec::new(),
    };

    let mut wrong_hash = spec.clone();
    wrong_hash.sha256 = "0".repeat(64);
    let err = verify_existing(&p, &wrong_hash).unwrap_err();
    assert!(err.to_string().contains("checksum does not match"));

    let mut wrong_size = spec.clone();
    wrong_size.bytes = 99;
    let err = verify_existing(&p, &wrong_size).unwrap_err();
    assert!(err.to_string().contains("size differs"));
}

#[test]
fn verify_existing_accepts_compatible_hash() {
    let dir = temp_dir("verify-compat");
    let p = dir.path().join("f.bin");
    std::fs::write(&p, b"abc").unwrap();
    let (len, sha) = hash_file(&p).unwrap();
    let spec = FileSpec {
        role: Role::Unet,
        folder: "".into(),
        name: "f.bin".into(),
        bytes: len,
        sha256: "0".repeat(64),
        url: "https://huggingface.co/f".into(),
        compatible: vec![(len, sha)],
    };
    assert!(verify_existing(&p, &spec).unwrap());
}

#[test]
fn target_path_and_missing_files_work_by_name() {
    let dir = temp_dir("missing");
    let spec1 = file_spec_with_content(dir.path(), "diffusion_models", "a.safetensors", b"a");
    let spec2 = file_spec_with_content(dir.path(), "diffusion_models", "b.safetensors", b"b");
    let spec3 = FileSpec {
        role: Role::Unet,
        folder: "diffusion_models".into(),
        name: "c.safetensors".into(),
        bytes: 1,
        sha256: "0".repeat(64),
        url: "https://huggingface.co/x/c.safetensors".into(),
        compatible: vec![],
    };
    let p = preset("missing-test", vec![spec1.clone(), spec2.clone(), spec3.clone()]);

    assert_eq!(target_path(dir.path(), &spec1), dir.path().join("diffusion_models/a.safetensors"));
    let missing = missing_files(dir.path(), &p);
    assert_eq!(missing.len(), 1);
    assert_eq!(missing[0].name, "c.safetensors");
}

#[test]
fn removal_plan_deduplicates_duplicate_specs_and_requires_full_install_for_sharing() {
    let dir = temp_dir("removal-plan");
    let a_spec = file_spec_with_content(dir.path(), "diffusion_models", "a.safetensors", b"a");
    let te_spec = file_spec_with_content(dir.path(), "text_encoders", "te.safetensors", b"te");
    let b_spec = file_spec_with_content(dir.path(), "diffusion_models", "b.safetensors", b"b");
    let a_dup = FileSpec {
        role: Role::Unet,
        folder: "diffusion_models".into(),
        name: "a.safetensors".into(),
        bytes: a_spec.bytes,
        sha256: a_spec.sha256.clone(),
        url: "https://huggingface.co/x/a.safetensors".into(),
        compatible: vec![],
    };
    let missing_spec = FileSpec {
        role: Role::Unet,
        folder: "diffusion_models".into(),
        name: "gone.safetensors".into(),
        bytes: 1,
        sha256: "0".repeat(64),
        url: "https://huggingface.co/x/gone.safetensors".into(),
        compatible: vec![],
    };

    let a = preset("a", vec![a_spec.clone(), te_spec.clone(), a_dup]);
    let b = preset("b", vec![b_spec, te_spec.clone()]);
    let partial = preset("c-partial", vec![a_spec.clone(), missing_spec]);
    let all = vec![a.clone(), b.clone(), partial];

    let plan = removal_plan(dir.path(), &a, &all);
    assert_eq!(plan.shared, vec!["te.safetensors".to_owned()]);
    assert_eq!(plan.remove.len(), 1, "{plan:?}");
    assert_eq!(plan.remove[0].0, dir.path().join("diffusion_models/a.safetensors"));
    assert_eq!(plan.bytes(), b"a".len() as u64);
}

#[test]
fn remove_preset_moves_only_verified_files_and_reports_shared_and_mismatched() {
    let dir = temp_dir("remove");
    let unet_a = file_spec_with_content(dir.path(), "diffusion_models", "a.safetensors", b"model a weights");
    let te = file_spec_with_content(dir.path(), "text_encoders", "te.safetensors", b"shared text encoder");
    let lora = file_spec_with_content(dir.path(), "loras", "helper.safetensors", b"expected lora");
    // Overwrite the lora file with different content of the same length, making it mismatched.
    std::fs::write(dir.path().join("loras/helper.safetensors"), b"EXPECTED LORA").unwrap();

    let unet_b = file_spec_with_content(dir.path(), "diffusion_models", "b.safetensors", b"model b weights");
    let a = preset("rm-a", vec![unet_a.clone(), te.clone(), lora.clone()]);
    let b = preset("rm-b", vec![unet_b, te.clone()]);
    let all = vec![a.clone(), b.clone()];

    let trashed = std::cell::RefCell::new(Vec::new());
    let r = remove_preset(dir.path(), &a, &all, &|p| {
        trashed.borrow_mut().push(p.to_path_buf());
        std::fs::remove_file(p)
    })
    .unwrap();

    assert_eq!(r.removed, vec!["a.safetensors".to_owned()]);
    assert_eq!(r.freed, b"model a weights".len() as u64);
    assert_eq!(r.shared, vec!["te.safetensors".to_owned()]);
    assert_eq!(r.mismatched, vec!["helper.safetensors".to_owned()]);
    assert_eq!(trashed.borrow().len(), 1);
    assert!(!dir.path().join("diffusion_models/a.safetensors").exists());
    assert!(dir.path().join("text_encoders/te.safetensors").exists());
    assert_eq!(std::fs::read(dir.path().join("loras/helper.safetensors")).unwrap(), b"EXPECTED LORA", "mismatched file stays untouched");
}

#[test]
fn listed_file_blocks_path_traversal_and_picks_first_folder() {
    let dir = temp_dir("listed");
    std::fs::create_dir_all(dir.path().join("loras/sub")).unwrap();
    std::fs::create_dir_all(dir.path().join("checkpoints/sub")).unwrap();
    std::fs::write(dir.path().join("loras/sub/style.safetensors"), b"1234").unwrap();
    std::fs::write(dir.path().join("checkpoints/other.safetensors"), b"x").unwrap();
    std::fs::write(dir.path().join("secret.txt"), b"secret").unwrap();

    assert!(listed_file(dir.path(), &["loras"], "../secret.txt").is_none());
    assert!(listed_file(dir.path(), &["loras"], "/etc/passwd").is_none());
    assert!(listed_file(dir.path(), &["checkpoints"], "sub/style.safetensors").is_none());

    let found = listed_file(dir.path(), &["checkpoints", "loras"], "sub/style.safetensors");
    assert_eq!(found, Some(dir.path().join("loras/sub/style.safetensors")));

    let freed = remove_listed_file(dir.path(), &["checkpoints", "loras"], "sub/style.safetensors", &|p| std::fs::remove_file(p)).unwrap();
    assert_eq!(freed, 4);
    assert!(!dir.path().join("loras/sub/style.safetensors").exists());
    assert!(dir.path().join("secret.txt").exists());
}

#[test]
fn human_bytes_formats_scale() {
    assert_eq!(human_bytes(999), "999 bytes");
    assert_eq!(human_bytes(1000), "1 KB");
    assert_eq!(human_bytes(1_500_000), "2 MB");
    assert_eq!(human_bytes(7_256_783_064), "7.3 GB");
}

#[test]
fn deletion_date_formats_known_timestamps() {
    assert_eq!(deletion_date(UNIX_EPOCH), "1970-01-01T00:00:00");
    assert_eq!(deletion_date(UNIX_EPOCH + Duration::from_secs(1_760_000_000)), "2025-10-09T08:53:20");
    assert_eq!(deletion_date(UNIX_EPOCH - Duration::from_secs(1)), "1970-01-01T00:00:00");
}

#[test]
fn encode_path_percent_encodes_spec_chars() {
    assert_eq!(encode_path(Path::new("/a b/ü.onnx")), "/a%20b/%C3%BC.onnx");
    assert_eq!(encode_path(Path::new("/tmp/file!$&'()*+,;=:@.onnx")), "/tmp/file!$&'()*+,;=:@.onnx");
    assert_eq!(encode_path(Path::new("C:\\foo bar")), "C:%5Cfoo%20bar");
}

#[test]
fn home_trash_returns_absolute_trash_path() {
    if let Some(got) = home_trash() {
        assert!(got.is_absolute());
        assert_eq!(got.file_name().and_then(|n| n.to_str()), Some("Trash"));
    }
}

#[test]
fn move_to_trash_rejects_directory_and_missing_file() {
    if !available() {
        return;
    }
    let temp = temp_dir("trash-errors");

    let dir_path = temp.path().join("subdir");
    std::fs::create_dir_all(&dir_path).unwrap();
    let err = move_to_trash(&dir_path).unwrap_err();
    assert_eq!(err.kind(), io::ErrorKind::InvalidInput);

    let missing = temp.path().join("missing.bin");
    let err = move_to_trash(&missing).unwrap_err();
    assert_eq!(err.kind(), io::ErrorKind::NotFound);
}

#[test]
fn pattern_matches_matches_each_part_case_insensitively() {
    assert!(pattern_matches("anima*lllite*inpaint", "models/Anima-LLLite-Inpaint.safetensors"));
    assert!(!pattern_matches("anima*xyz", "models/Anima-LLLite-Inpaint.safetensors"));
    assert!(pattern_matches("*fp8*scaled*", "t5xxl_fp8_e4m3fn_scaled.safetensors"));
    assert!(pattern_matches("sigclip_vision", "sigclip_vision_patch14_384.safetensors"));
    assert!(!pattern_matches("nonexistent", "sigclip_vision_patch14_384.safetensors"));
}

#[test]
fn label_for_humanizes_names() {
    assert_eq!(label_for("sub/juggernaut_XL-v9.safetensors"), "juggernaut XL v9");
    assert_eq!(label_for("pony_style.safetensors"), "pony style");
    assert_eq!(label_for("no_ext"), "no ext");
}

#[test]
fn resolve_components_finds_by_exact_then_shortest_pattern() {
    let info = info();
    let reg = li_ai::family::registry();

    let flux = resolve_components(reg.family("flux1").unwrap(), &info);
    assert!(flux.contains(&(FamilyRole::Clip, "clip_l.safetensors".to_owned())));
    assert!(flux.contains(&(FamilyRole::Clip2, "t5xxl_fp8_e4m3fn_scaled.safetensors".to_owned())));
    assert!(flux.contains(&(FamilyRole::Vae, "ae.safetensors".to_owned())));

    let qwen = resolve_components(reg.family("qwen-edit").unwrap(), &info);
    assert!(qwen.contains(&(FamilyRole::Vae, "qwen_image_vae.safetensors".to_owned())));
}

#[test]
fn detect_file_reads_local_headers_and_caches() {
    let dir = temp_dir("detect");
    let p = write_safetensors(dir.path(), "checkpoints/mystery.safetensors", &sdxl_header());
    let detected = detect_file(&p).unwrap();
    assert_eq!(detected.family.as_deref(), Some("sdxl"));
    assert_eq!(detected.kind, FileKind::Checkpoint);

    let detected2 = detect_file(&p).unwrap();
    assert_eq!(detected2.family, detected.family);
}

#[test]
fn family_of_prefers_header_then_name_and_rejects_lora_as_model() {
    let dir = temp_dir("family");

    let p = write_safetensors(dir.path(), "checkpoints/mystery.safetensors", &sdxl_header());
    let got = family_of("mystery.safetensors", FileKind::Checkpoint, Some(&p), None).unwrap();
    assert_eq!(got.0, "sdxl");

    let by_name = family_of("juggernautXL_v9.safetensors", FileKind::Checkpoint, None, None).unwrap();
    assert_eq!(by_name.0, "sdxl");
    assert_eq!(by_name.1, "file name");

    let lora_p = write_safetensors(dir.path(), "diffusion_models/flux_ink.safetensors", &flux_lora_header());
    assert_eq!(family_of("flux_ink.safetensors", FileKind::DiffusionModel, Some(&lora_p), None), None);
    let as_lora = family_of("flux_ink.safetensors", FileKind::Lora, Some(&lora_p), None).unwrap();
    assert_eq!(as_lora.0, "flux1");
}

#[test]
fn listed_as_model_but_lora_detects_by_name_or_header() {
    assert!(listed_as_model_but_lora("sdxl_detail_LoRA.safetensors", None));
    assert!(!listed_as_model_but_lora("flux1-dev-fp8.safetensors", None));

    let dir = temp_dir("lora-as-model");
    let lora_p = write_safetensors(dir.path(), "diffusion_models/flux_ink.safetensors", &flux_lora_header());
    assert!(listed_as_model_but_lora("flux_ink.safetensors", Some(&lora_p)));

    let sdxl_p = write_safetensors(dir.path(), "checkpoints/mystery.safetensors", &sdxl_header());
    assert!(!listed_as_model_but_lora("mystery.safetensors", Some(&sdxl_p)));
}

#[test]
fn scan_finds_models_and_loras_by_name() {
    let (models, loras) = scan(&info(), None, li_ai::family::registry());
    let fam = |k: &str| models.iter().find(|m| m.key == k).map(|m| m.family.clone());

    assert_eq!(fam("ckpt:juggernautXL_v9.safetensors").as_deref(), Some("sdxl"));
    assert_eq!(fam("unet:flux1-dev-fp8.safetensors").as_deref(), Some("flux1"));
    assert_eq!(fam("unet:qwen_image_edit_2511_bf16.safetensors").as_deref(), Some("qwen-edit"));
    assert!(fam("ckpt:mystery.safetensors").is_none());
    assert!(fam("ckpt:sdxl_detail_LoRA.safetensors").is_none());

    assert_eq!(loras.iter().find(|l| l.name == "pony_style.safetensors").unwrap().family.as_deref(), Some("pony"));
}

#[test]
fn scan_skips_loras_in_model_folders() {
    let dir = temp_dir("scan-lora");
    std::fs::create_dir_all(dir.path().join("diffusion_models")).unwrap();
    let lora_p = write_safetensors(dir.path(), "diffusion_models/flux_ink.safetensors", &flux_lora_header());

    let info = ObjectInfo(json!({
        "CheckpointLoaderSimple": {"input": {"required": {"ckpt_name": [["juggernautXL_v9.safetensors", "sdxl_detail_LoRA.safetensors"]]}}},
        "UNETLoader": {"input": {"required": {"unet_name": [["flux1-dev-fp8.safetensors", "flux_realism_lora.safetensors", "loras/qwen_style.safetensors", "flux_ink.safetensors"]]}}},
        "LoraLoader": {"input": {"required": {"lora_name": [["flux_realism_lora.safetensors"]]}}}
    }));

    let (models, loras) = scan(&info, Some(dir.path()), li_ai::family::registry());
    let keys: Vec<&str> = models.iter().map(|m| m.key.as_str()).collect();
    assert_eq!(keys, vec!["ckpt:juggernautXL_v9.safetensors", "unet:flux1-dev-fp8.safetensors"], "{keys:?}");

    assert_eq!(loras.len(), 1);
    assert_eq!(loras[0].family.as_deref(), Some("flux1"));

    assert!(listed_as_model_but_lora("flux_ink.safetensors", Some(&lora_p)));
}
