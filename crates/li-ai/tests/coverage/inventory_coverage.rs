use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

use serde_json::json;

use li_ai::arch::FileKind;
use li_ai::comfy::ObjectInfo;
use li_ai::inventory::{family_of, identify_file, label_for, listed_as_model_but_lora, loras, loras_for, pattern_matches, refresh, resolve_components};

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
fn identify_file_returns_none_for_missing_file() {
    assert!(identify_file(Path::new("/definitely/not/here/model.safetensors"), FileKind::Checkpoint, li_ai::family::registry()).is_none());
}

#[test]
fn identify_file_returns_none_for_empty_file() {
    let tmp = TempDir::new("empty");
    let p = tmp.path().join("empty.safetensors");
    std::fs::write(&p, b"").unwrap();

    assert!(identify_file(&p, FileKind::Checkpoint, li_ai::family::registry()).is_none());
}

#[test]
fn family_of_falls_back_to_file_name() {
    assert_eq!(family_of("juggernautXL_v9.safetensors", FileKind::Checkpoint, None, None).map(|(f, _)| f), Some("sdxl".to_string()));
    assert_eq!(family_of("mystery.safetensors", FileKind::Checkpoint, None, None), None);
    assert_eq!(family_of("", FileKind::Checkpoint, None, None), None);
}

#[test]
fn listed_as_model_but_lora_uses_name_for_remote_files() {
    assert!(listed_as_model_but_lora("sdxl_detail_LoRA.safetensors", None));
    assert!(!listed_as_model_but_lora("juggernautXL_v9.safetensors", None));
    assert!(listed_as_model_but_lora("loras/qwen_style.safetensors", None));
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
