use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

use serde_json::{Value, json};

use li_ai::arch::FileKind;
use li_ai::comfy::ObjectInfo;
use li_ai::family::Role;
use li_ai::inventory::{detect_file, family_of, label_for, listed_as_model_but_lora, loras, loras_for, pattern_matches, refresh, resolve_components, scan};

static TEMP_COUNT: AtomicU64 = AtomicU64::new(0);
static GLOBAL_LOCK: Mutex<()> = Mutex::new(());

struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> Self {
        let n = TEMP_COUNT.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("li-ai-inventory-{}-{}-{}", tag, std::process::id(), n));
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn info() -> ObjectInfo {
    ObjectInfo(json!({
        "CheckpointLoaderSimple": {"input": {"required": {"ckpt_name": [["juggernautXL_v9.safetensors", "dreamshaper_8.safetensors", "mystery.safetensors"]]}}},
        "UNETLoader": {"input": {"required": {"unet_name": [["flux1-dev-fp8.safetensors", "qwen_image_edit_2511_bf16.safetensors", "flux-2-klein-4b.safetensors"]]}}},
        "CLIPLoader": {"input": {"required": {"clip_name": [["qwen_2.5_vl_7b_fp8_scaled.safetensors", "qwen_3_4b.safetensors", "clip_l.safetensors", "t5xxl_fp8_e4m3fn_scaled.safetensors"]]}}},
        "DualCLIPLoader": {"input": {"required": {"clip_name1": [["clip_l.safetensors", "t5xxl_fp8_e4m3fn_scaled.safetensors"]]}}},
        "VAELoader": {"input": {"required": {"vae_name": [["ae.safetensors", "qwen_image_vae.safetensors", "flux2-vae.safetensors"]]}}},
        "LoraLoader": {"input": {"required": {"lora_name": [["pony_style.safetensors", "flux_realism.safetensors", "misc.safetensors"]]}}}
    }))
}

fn write_header(path: &Path, value: &Value) {
    let header = serde_json::to_vec(value).unwrap();
    let mut bytes = (header.len() as u64).to_le_bytes().to_vec();
    bytes.extend_from_slice(&header);
    std::fs::write(path, bytes).unwrap();
}

fn write_sdxl_header(path: &Path) {
    write_header(
        path,
        &json!({
            "model.diffusion_model.input_blocks.0.0.weight": {"dtype": "F16", "shape": [320, 4, 3, 3], "data_offsets": [0, 0]},
            "model.diffusion_model.label_emb.0.0.weight": {"dtype": "F16", "shape": [1280, 2816], "data_offsets": [0, 0]},
            "conditioner.embedders.0.transformer.text_model.final_layer_norm.weight": {"dtype": "F16", "shape": [768], "data_offsets": [0, 0]}
        }),
    );
}

fn write_lora_header(path: &Path) {
    write_header(
        path,
        &json!({
            "double_blocks.18.img_attn.qkv.lora_A.weight": {"dtype": "F16", "shape": [16, 3072], "data_offsets": [0, 0]},
            "double_blocks.18.img_attn.qkv.lora_B.weight": {"dtype": "F16", "shape": [9216, 16], "data_offsets": [0, 0]}
        }),
    );
}

#[test]
fn pattern_matches_is_case_insensitive_and_normalizes_backslashes() {
    assert!(pattern_matches("clip_l", "CLIP_L.safetensors"));
    assert!(pattern_matches("clip_l", "models\\CLIP_L.safetensors"));
    assert!(pattern_matches("models*clip_l", "models/CLIP_L.safetensors"));
    assert!(!pattern_matches("t5xxl", "clip_l.safetensors"));
}

#[test]
fn pattern_matches_star_parts_are_all_required_and_empty_parts_are_ignored() {
    assert!(pattern_matches("anima*lllite*inpaint", "models/Anima-LLLite-Inpaint.safetensors"));
    assert!(!pattern_matches("anima*missing*inpaint", "models/Anima-LLLite-Inpaint.safetensors"));
    assert!(pattern_matches("**anima**lllite**", "Anima-LLLite.safetensors"));
    assert!(pattern_matches("*", "anything"));
    assert!(pattern_matches("", "anything"));
    assert!(pattern_matches("**", "anything"));
}

#[test]
fn label_for_strips_path_and_extension_and_normalizes_separators() {
    assert_eq!(label_for("subdir\\juggernaut_XL-v9.safetensors"), "juggernaut XL v9");
    assert_eq!(label_for("flux1-dev-fp8.safetensors"), "flux1 dev fp8");
    assert_eq!(label_for("plain_model"), "plain model");
    assert_eq!(label_for("noext"), "noext");
}

#[test]
fn resolve_components_known_flux1_returns_expected_exact_names() {
    let reg = li_ai::family::registry();
    let flux = reg.family("flux1").expect("flux1 family");
    let comps = resolve_components(flux, &info());

    assert!(comps.contains(&(Role::Clip, "clip_l.safetensors".to_string())));
    assert!(comps.contains(&(Role::Clip2, "t5xxl_fp8_e4m3fn_scaled.safetensors".to_string())));
    assert!(comps.contains(&(Role::Vae, "ae.safetensors".to_string())));
    assert!(comps.iter().all(|(_, name)| !name.is_empty()));
}

#[test]
fn resolve_components_returns_no_duplicate_roles() {
    let reg = li_ai::family::registry();
    let flux = reg.family("flux1").expect("flux1 family");
    let comps = resolve_components(flux, &info());

    for i in 0..comps.len() {
        for j in i + 1..comps.len() {
            assert!(!(comps[i].0 == comps[j].0 && comps[i].1 == comps[j].1), "duplicate role/name pair");
        }
    }
}

#[test]
fn resolve_components_empty_info_is_empty() {
    let reg = li_ai::family::registry();
    let flux = reg.family("flux1").expect("flux1 family");
    let empty = ObjectInfo(json!({}));
    let comps = resolve_components(flux, &empty);

    assert!(comps.is_empty());
}

#[test]
fn detect_file_returns_none_for_missing_file() {
    assert!(detect_file(Path::new("/definitely/not/here/model.safetensors")).is_none());
}

#[test]
fn detect_file_returns_none_for_empty_file() {
    let tmp = TempDir::new("empty");
    let p = tmp.path().join("empty.safetensors");
    std::fs::write(&p, b"").unwrap();

    assert!(detect_file(&p).is_none());
}

#[test]
fn detect_file_classifies_sdxl_header_and_is_deterministic() {
    let tmp = TempDir::new("sdxl");
    let p = tmp.path().join("mystery.safetensors");
    write_sdxl_header(&p);

    let first = detect_file(&p).expect("detected");
    let second = detect_file(&p).expect("detected again");

    assert_eq!(first.family, second.family);
    assert_eq!(first.kind, second.kind);
    assert_eq!(first.evidence, second.evidence);
    assert_eq!(first.family.as_deref(), Some("sdxl"));
    assert!(!first.evidence.is_empty());
}

#[test]
fn family_of_uses_local_header_when_present() {
    let tmp = TempDir::new("family-local");
    let p = tmp.path().join("mystery.safetensors");
    write_sdxl_header(&p);

    let got = family_of("mystery.safetensors", FileKind::Checkpoint, Some(&p), None);
    assert_eq!(got.as_ref().map(|(f, _)| f.as_str()), Some("sdxl"));
    assert!(!got.as_ref().map(|(_, evidence)| evidence.as_str()).unwrap_or("").is_empty());
}

// BUG: family_of metadata classification returns None even with valid safetensors-style metadata.
// The library expects ComfyUI /view_metadata, but arch::classify may not recognize the
// string representation when metadata values are JSON objects.
#[test]
#[ignore = "BUG: family_of metadata classification returns None for valid safetensors metadata"]
fn family_of_uses_metadata_when_no_local_file() {
    let metadata = json!({
        "model.diffusion_model.input_blocks.0.0.weight": {"dtype": "F16", "shape": [320, 4, 3, 3]},
        "model.diffusion_model.label_emb.0.0.weight": {"dtype": "F16", "shape": [1280, 2816]},
        "conditioner.embedders.0.transformer.text_model.final_layer_norm.weight": {"dtype": "F16", "shape": [768]}
    });

    let got = family_of("mystery.safetensors", FileKind::Checkpoint, None, Some(&metadata));
    assert_eq!(got.as_ref().map(|(f, _)| f.as_str()), Some("sdxl"));
}

#[test]
fn family_of_falls_back_to_file_name() {
    assert_eq!(family_of("juggernautXL_v9.safetensors", FileKind::Checkpoint, None, None).map(|(f, _)| f), Some("sdxl".to_string()));
    assert_eq!(family_of("mystery.safetensors", FileKind::Checkpoint, None, None), None);
    assert_eq!(family_of("", FileKind::Checkpoint, None, None), None);
}

#[test]
fn family_of_rejects_lora_when_main_file_kind_requested() {
    let tmp = TempDir::new("lora-main");
    let p = tmp.path().join("flux_ink.safetensors");
    write_lora_header(&p);

    assert_eq!(family_of("flux_ink.safetensors", FileKind::Checkpoint, Some(&p), None), None);
    let got = family_of("flux_ink.safetensors", FileKind::Lora, Some(&p), None);
    assert_eq!(got.as_ref().map(|(f, _)| f.as_str()), Some("flux1"));
}

#[test]
fn listed_as_model_but_lora_uses_name_for_remote_files() {
    assert!(listed_as_model_but_lora("sdxl_detail_LoRA.safetensors", None));
    assert!(!listed_as_model_but_lora("juggernautXL_v9.safetensors", None));
    assert!(listed_as_model_but_lora("loras/qwen_style.safetensors", None));
}

#[test]
fn listed_as_model_but_lora_header_overrides_neutral_name() {
    let tmp = TempDir::new("lora-listed");

    let lora_path = tmp.path().join("flux_ink.safetensors");
    write_lora_header(&lora_path);
    assert!(listed_as_model_but_lora("flux_ink.safetensors", Some(&lora_path)));

    let sdxl_path = tmp.path().join("mystery.safetensors");
    write_sdxl_header(&sdxl_path);
    assert!(!listed_as_model_but_lora("mystery.safetensors", Some(&sdxl_path)));
}

#[test]
fn scan_finds_models_by_name_and_keeps_unknown_files() {
    let reg = li_ai::family::registry();
    let (models, loras) = scan(&info(), None, reg);

    let model_family = |key: &str| models.iter().find(|m| m.key.as_str() == key).map(|m| m.family.as_str());

    assert_eq!(model_family("ckpt:juggernautXL_v9.safetensors"), Some("sdxl"));
    assert_eq!(model_family("unet:flux1-dev-fp8.safetensors"), Some("flux1"));
    assert_eq!(model_family("unet:qwen_image_edit_2511_bf16.safetensors"), Some("qwen-edit"));
    // Unknown main weights stay in the catalogue (offered for refine via the universal recipe and self-test).
    assert!(models.iter().any(|m| m.key.as_str() == "ckpt:mystery.safetensors"));

    assert_eq!(loras.len(), 3);
    assert!(loras.iter().any(|l| l.name.as_str() == "pony_style.safetensors" && l.family.as_deref() == Some("pony")));
    assert!(loras.iter().any(|l| l.name.as_str() == "flux_realism.safetensors" && l.family.as_deref() == Some("flux1")));
    assert!(loras.iter().any(|l| l.name.as_str() == "misc.safetensors" && l.family.is_none()));
}

#[test]
fn scan_skips_lora_files_sitting_in_model_folders() {
    let tmp = TempDir::new("scan-lora-model");
    std::fs::create_dir_all(tmp.path().join("diffusion_models")).unwrap();
    write_lora_header(&tmp.path().join("diffusion_models").join("flux_ink.safetensors"));

    let info = ObjectInfo(json!({
        "CheckpointLoaderSimple": {"input": {"required": {"ckpt_name": [["juggernautXL_v9.safetensors", "sdxl_detail_LoRA.safetensors"]]}}},
        "UNETLoader": {"input": {"required": {"unet_name": [["flux1-dev-fp8.safetensors", "flux_realism_lora.safetensors", "loras/qwen_style.safetensors", "flux_ink.safetensors"]]}}},
        "LoraLoader": {"input": {"required": {"lora_name": [["flux_realism_lora.safetensors"]]}}}
    }));

    let reg = li_ai::family::registry();
    let (models, loras) = scan(&info, Some(tmp.path()), reg);

    let keys: Vec<&str> = models.iter().map(|m| m.key.as_str()).collect();
    assert_eq!(keys, vec!["ckpt:juggernautXL_v9.safetensors", "unet:flux1-dev-fp8.safetensors"]);
    assert_eq!(loras.len(), 1);
    assert_eq!(loras[0].family.as_deref(), Some("flux1"));
}

#[test]
fn refresh_publishes_loras() {
    let _guard = GLOBAL_LOCK.lock().unwrap();

    let info = ObjectInfo(json!({
        "LoraLoader": {"input": {"required": {"lora_name": [["pony_style.safetensors", "flux_realism.safetensors", "misc.safetensors"]]}}}
    }));
    refresh(&info, None);

    let got = loras();
    assert!(got.iter().any(|l| l.name.as_str() == "pony_style.safetensors" && l.family.as_deref() == Some("pony")));
    assert!(got.iter().any(|l| l.name.as_str() == "flux_realism.safetensors" && l.family.as_deref() == Some("flux1")));
    assert!(got.iter().any(|l| l.name.as_str() == "misc.safetensors" && l.family.is_none()));
}

#[test]
fn loras_for_filters_by_family_and_includes_unknown() {
    let _guard = GLOBAL_LOCK.lock().unwrap();

    let info = ObjectInfo(json!({
        "LoraLoader": {"input": {"required": {"lora_name": [["pony_style.safetensors", "flux_realism.safetensors", "misc.safetensors"]]}}}
    }));
    refresh(&info, None);

    let reg = li_ai::family::registry();
    let flux = reg.family("flux1").expect("flux1 family");
    let matches = loras_for(flux);

    let names: Vec<(&str, bool)> = matches.iter().map(|(l, matched)| (l.name.as_str(), *matched)).collect();

    assert!(names.contains(&("flux_realism.safetensors", true)));
    assert!(names.contains(&("misc.safetensors", false)));
    assert!(!names.iter().any(|(name, _)| *name == "pony_style.safetensors"));
}
