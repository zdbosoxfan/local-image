//! File identification on trimmed real headers (`tests/fixtures/headers`): kind, architecture,
//! variant and precision for every fixture, the generation veto (finding 61), GGUF files decided
//! by their tensors (finding 70) and detection rules that arrive as profile data (finding 68).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use li_ai::arch::{self, FileKind, Header};
use li_ai::family::{self, Registry};
use li_ai::identify::{self, Identity};
use serde_json::{Value, json};

fn dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/headers")
}

fn fixture_json(name: &str) -> Value {
    let p = dir().join(format!("{name}.json"));
    serde_json::from_slice(&std::fs::read(&p).unwrap_or_else(|e| panic!("{}: {e}", p.display()))).unwrap()
}

/// A GGUF v3 header with the fixture's metadata (strings, `general.file_type` as u32) and tensors.
pub fn gguf_bytes(v: &Value) -> Vec<u8> {
    let mut b = b"GGUF".to_vec();
    let s = |b: &mut Vec<u8>, s: &str| {
        b.extend((s.len() as u64).to_le_bytes());
        b.extend(s.as_bytes());
    };
    let meta = v.get("__metadata__").and_then(Value::as_object).cloned().unwrap_or_default();
    let tensors: Vec<(&String, &Value)> = v.as_object().unwrap().iter().filter(|(k, _)| !k.starts_with("__")).collect();
    b.extend(3u32.to_le_bytes());
    b.extend((tensors.len() as u64).to_le_bytes());
    b.extend((meta.len() as u64).to_le_bytes());
    for (k, val) in &meta {
        s(&mut b, k);
        let text = val.as_str().unwrap_or("");
        match (k.as_str(), text.parse::<u32>()) {
            ("general.file_type" | "general.quantization_version", Ok(n)) => {
                b.extend(4u32.to_le_bytes());
                b.extend(n.to_le_bytes());
            }
            _ => {
                b.extend(8u32.to_le_bytes());
                s(&mut b, text);
            }
        }
    }
    for (name, t) in tensors {
        s(&mut b, name);
        let shape: Vec<u64> = t["shape"].as_array().unwrap().iter().map(|d| d.as_u64().unwrap()).collect();
        b.extend((shape.len() as u32).to_le_bytes());
        for d in shape.iter().rev() {
            b.extend(d.to_le_bytes());
        }
        let ty: u32 = t["dtype"].as_str().unwrap().trim_start_matches("gguf:").parse().unwrap();
        b.extend(ty.to_le_bytes());
        b.extend(0u64.to_le_bytes());
    }
    b
}

/// The safetensors header bytes (8-byte length + JSON) of a fixture.
pub fn safetensors_bytes(v: &Value) -> Vec<u8> {
    let mut o = serde_json::Map::new();
    for (k, t) in v.as_object().unwrap() {
        match k.as_str() {
            "__metadata__" => {
                o.insert(k.clone(), t.clone());
            }
            k if k.starts_with("__") => {}
            _ => {
                o.insert(k.clone(), json!({"dtype": t["dtype"], "shape": t["shape"], "data_offsets": [0, 0]}));
            }
        }
    }
    let body = serde_json::to_vec(&Value::Object(o)).unwrap();
    let mut out = (body.len() as u64).to_le_bytes().to_vec();
    out.extend(body);
    out
}

/// The header of a fixture, read through the real header reader.
pub fn header(name: &str) -> Header {
    let v = fixture_json(name);
    let bytes = if v.get("__gguf__").is_some() { gguf_bytes(&v) } else { safetensors_bytes(&v) };
    let p = std::env::temp_dir().join(format!("li-hdr-{}-{}", uuid::Uuid::new_v4().simple(), listed_name(name)));
    std::fs::write(&p, bytes).unwrap();
    let h = arch::read_header(&p).unwrap_or_else(|e| panic!("{name}: {e:#}"));
    let _ = std::fs::remove_file(&p);
    h
}

/// The file name a ComfyUI loader would list for a fixture.
pub fn listed_name(fixture: &str) -> String {
    let path = fixture.split_once("__").map_or(fixture, |(_, p)| p);
    let base = path.rsplit('~').next().unwrap_or(path);
    if base.ends_with(".gguf") { base.to_owned() } else { format!("{base}.safetensors") }
}

pub fn embedded_registry() -> Registry {
    let raw: BTreeMap<String, Value> = family::EMBEDDED.iter().map(|(k, t)| ((*k).to_owned(), serde_json::from_str(t).unwrap())).collect();
    Registry { families: family::build(&raw).unwrap(), ..Default::default() }
}

fn identify_fixture(reg: &Registry, name: &str, listed: FileKind) -> Identity {
    identify::identify(&listed_name(name), listed, Some(&header(name)), reg)
}

use FileKind::{Checkpoint as C, DiffusionModel as D, Lora as L, TextEncoder as T, Vae as V};

/// `(fixture, listed by, kind, arch, variant, precision)` with the built-in profiles.
const TABLE: &[(&str, FileKind, FileKind, Option<&str>, Option<&str>, &str)] = &[
    ("Comfy-Org_Fibo-2.0__diffusion_models~fibo2_dev_bf16", D, D, None, None, "bf16"),
    ("Comfy-Org_Fibo-2.0__diffusion_models~fibo2_edit_turbo_int8_convrot", D, D, None, None, "int8_convrot"),
    ("Comfy-Org_Flux-2-Dev__split_files~diffusion_models~flux2_dev_fp8mixed", D, D, Some("flux2"), None, "fp8mixed"),
    ("Comfy-Org_Flux-2-Klein-4B__split_files~diffusion_models~flux-2-klein-4b", D, D, Some("flux2-klein"), Some("4b"), "bf16"),
    ("Comfy-Org_Flux-2-Klein-4B__split_files~diffusion_models~flux-2-klein-base-4b", D, D, Some("flux2-klein"), Some("4b"), "bf16"),
    ("Comfy-Org_HiDream-O1-Image__checkpoints~hidream_o1_image_bf16", C, C, None, None, "bf16"),
    ("Comfy-Org_HiDream-O1-Image__checkpoints~hidream_o1_image_dev_fp8_scaled", C, C, None, None, "fp8_scaled"),
    ("Comfy-Org_Krea-2__diffusion_models~krea2_raw_fp8_scaled", D, D, None, None, "fp8_scaled"),
    ("Comfy-Org_Krea-2__diffusion_models~krea2_turbo_bf16", D, D, None, None, "bf16"),
    ("Comfy-Org_Krea-2__diffusion_models~krea2_turbo_mxfp8", D, D, None, None, "mxfp8"),
    ("Comfy-Org_Krea-2__diffusion_models~krea2_turbo_nvfp4", D, D, None, None, "nvfp4"),
    ("Comfy-Org_MOSS-Image__diffusion_models~moss_image_base_bf16", D, D, None, None, "bf16"),
    ("Comfy-Org_MOSS-Image__diffusion_models~moss_image_sft_int8_convrot", D, D, None, None, "int8_convrot"),
    ("Comfy-Org_Ming-Image__diffusion_models~ming_image_0.1_design_bf16", D, D, None, None, "bf16"),
    ("Comfy-Org_Qwen-Image-2.1__diffusion_models~qwen_image_2.1_turbo_bf16", D, D, Some("qwen-image-21"), None, "bf16"),
    ("Comfy-Org_Qwen-Image-2.1__diffusion_models~qwen_image_2.1_turbo_int8_convrot", D, D, Some("qwen-image-21"), None, "int8_convrot"),
    ("Comfy-Org_Qwen-Image-2.1__model_patches~qwen_image_2.1_fun_controlnet_union_bf16", D, D, None, None, "bf16"),
    ("Comfy-Org_Z-Image__split_files~diffusion_models~z_image_bf16", D, D, Some("z-image"), None, "bf16"),
    ("Comfy-Org_Z-Image__split_files~diffusion_models~z_image_int8_convrot", D, D, Some("z-image"), None, "int8_convrot"),
    ("local__checkpoints~RealVisXL_V5.0_fp16", C, C, Some("sdxl"), None, "fp16"),
    ("local__checkpoints~XL_RealVisXL_V5.0_Lightning", C, C, Some("sdxl"), None, "fp16"),
    ("local__controlnet~diffusion_pytorch_model_promax", D, D, None, None, "fp16"),
    ("local__diffusion_models~flux-2-klein-9b-fp8", D, D, Some("flux2-klein"), Some("9b"), "fp8mixed"),
    ("local__diffusion_models~qwen_image_2.1_bf16", D, D, Some("qwen-image-21"), None, "bf16"),
    ("local__diffusion_models~qwen_image_2.1_int8_convrot", D, D, Some("qwen-image-21"), None, "int8_convrot"),
    ("local__diffusion_models~seedvr2_7b_fp16", D, D, Some("seedvr2"), None, "fp16"),
    ("local__diffusion_models~z_image_turbo_bf16", D, D, Some("z-image"), None, "bf16"),
    ("owner__diffusion_models~flux3-image-klein-bf16", D, D, None, None, "bf16"),
    ("owner__diffusion_models~flux3-image-klein-base-bf16", D, D, None, None, "bf16"),
    ("owner__diffusion_models~moss_image_sft_bf16", D, D, None, None, "bf16"),
    ("realrebelai_FLUX-3-Image-Klein_GGUFs__BASE~flux3_image_klein-Q4_K_M.gguf", D, D, None, None, "gguf:Q4_K_M"),
    ("realrebelai_FLUX-3-Image-Klein_GGUFs__BASE~flux3_image_klein_w4a8_convrot", D, D, None, None, "w4a8"),
    ("realrebelai_FLUX-3-Image-Klein_GGUFs__DISTILLED~flux3_image_klein_sd-Q8_0.gguf", D, D, None, None, "gguf:Q8_0"),
    ("realrebelai_FLUX-3-Image-Klein_GGUFs__DISTILLED~flux3_image_klein_sd_w4a8_convrot", D, D, None, None, "w4a8"),
    ("unsloth_FLUX.2-klein-9B-GGUF__flux-2-klein-9b-Q4_K_M.gguf", D, D, Some("flux2-klein"), Some("9b"), "gguf:Q4_K_M"),
    ("unsloth_flux-3-image-klein-GGUF__flux-3-image-klein-Q4_K_M.gguf", D, D, None, None, "gguf:Q4_K_M"),
    ("unsloth_Qwen-Image-2.1-GGUF__qwen-image-2.1-Q4_K_M.gguf", D, D, Some("qwen-image-21"), None, "gguf:Q4_K"),
    ("unsloth_Z-Image-Turbo-GGUF__z-image-turbo-Q4_K_M.gguf", D, D, Some("z-image"), None, "gguf:Q4_K_M"),
    ("Comfy-Org_Fibo-2.0__text_encoders~qwen3vl_4b_int8_convrot", T, T, None, None, "int8_convrot"),
    ("Comfy-Org_Flux-2-Dev__split_files~text_encoders~mistral_3_small_flux2_fp4_mixed", T, T, None, None, "fp4mixed"),
    ("Comfy-Org_Flux-2-Klein-4B__split_files~text_encoders~qwen_3_4b_fp4_flux2", T, T, None, None, "fp4mixed"),
    ("Comfy-Org_Flux-2-Klein-9B__split_files~text_encoders~qwen_3_8b", T, T, None, None, "bf16"),
    ("Comfy-Org_Krea-2__text_encoders~qwen3vl_4b_bf16", T, T, None, None, "bf16"),
    ("Comfy-Org_MOSS-Image__text_encoders~qwen3.5_9b_bf16", T, T, None, None, "bf16"),
    ("Comfy-Org_Ming-Image__text_encoders~ming_image_0.1_ling_mini_2.0_w4a8", T, T, None, None, "w4a8"),
    ("Comfy-Org_Qwen-Image-2.1__text_encoders~qwen3.5_9b_qwen_image_2.1_pe_t2i.int8_convrot", T, T, None, None, "int8_convrot"),
    ("Comfy-Org_Qwen-Image-2.1__text_encoders~qwen3vl_8b_w4a8", T, T, None, None, "w4a8"),
    ("Comfy-Org_Z-Image__split_files~text_encoders~qwen_3_4b_fp8_mixed", T, T, None, None, "fp8mixed"),
    ("local__text_encoders~ministral-3-3b", T, T, None, None, "bf16"),
    ("local__text_encoders~qwen_3_4b", T, T, None, None, "bf16"),
    ("local__text_encoders~qwen_3_8b_fp8mixed", T, T, None, None, "fp8mixed"),
    ("local__text_encoders~qwen3vl_8b_bf16", T, T, None, None, "bf16"),
    ("local__text_encoders~qwen3vl_8b_int8_convrot", T, T, None, None, "int8_convrot"),
    ("Comfy-Org_Krea-2__vae~qwen_image_vae", V, V, None, None, "bf16"),
    ("Comfy-Org_MOSS-Image__vae~flux2-vae_small", V, V, None, None, "fp32"),
    ("Comfy-Org_Ming-Image__vae~ming_image_vae_bf16", V, V, None, None, "bf16"),
    ("local__vae~ae", V, V, Some("flux1"), None, "fp32"),
    ("local__vae~flux2-vae", V, V, None, None, "fp32"),
    ("local__vae~qwen_image_2.1_vae_bf16", V, V, Some("qwen-image-21"), None, "bf16"),
    ("local__vae~seedvr2_ema_vae_fp16", V, V, Some("seedvr2"), None, "fp16"),
    ("adirik_pixel-art-lora-flux.2-klein-4B__pixelart_lora", L, L, Some("flux2-klein"), None, "bf16"),
    ("akhaliq_Qwen-Image-2.1-Next-Scene-LoRA__checkpoints~qwen21_next_scene_v1", L, L, Some("qwen-image-21"), None, "fp16"),
    ("alibaba-pai_Qwen-Image-2.1-Fun-Acc-LoRAs__models~Qwen-Image-2.1-Fun-Acc-4Step", L, L, Some("qwen-image-21"), None, "bf16"),
    ("chriswritescode_Turbo8-LoRA-Qwen-Image-2.1__turbo8_lora_step2500", L, L, Some("qwen-image-21"), None, "fp32"),
    ("Comfy-Org_Fibo-2.0__loras~fibo_2.0_turbo_lora_avg_rank_270_bf16", L, L, None, None, "bf16"),
    ("Comfy-Org_Flux-2-Dev__split_files~loras~Flux2TurboComfyv2", L, L, Some("flux2"), None, "bf16"),
    ("Comfy-Org_Qwen-Image-2.1__loras~qwen_image_2.1_turbo_lora_avg_rank_178_bf16", L, L, Some("qwen-image-21"), None, "bf16"),
    ("lightx2v_Qwen-Image-2.1-Lightning__pytorch_lora_weights", L, L, Some("qwen-image-21"), None, "fp32"),
    ("linoyts_Flux2-Klein-Delight-LoRA__pytorch_lora_weights", L, L, Some("flux2-klein"), None, "fp32"),
    ("linoyts_Flux2-Klein-Delight-LoRA__pytorch_lora_weights_v2", L, L, Some("flux2-klein"), None, "fp32"),
    ("local__loras~flux-2-klein-object-remove", L, L, Some("flux2-klein"), None, "fp32"),
    ("local__loras~nar_lora_joint_v9_comfyui", L, L, Some("other:yue2"), None, "bf16"),
    ("prithivMLmods_Qwen-Image-2.1-Natural-Exposure-LoRA__Qwen-Image-2.1-Natural-Exposure-LoRA-4000", L, L, Some("qwen-image-21"), None, "bf16"),
    ("thedeoxen_refcontrol-FLUX.2-klein-9B-reference-depth-lora__flux2_klein_9b_refcontrol_depth", L, L, Some("flux2-klein"), None, "bf16"),
    ("WarmBloodAban_Qwen-Image-2.1-LoRAs__Qwen2.1_Anime_consistency", L, L, Some("qwen-image-21"), None, "bf16"),
];

#[test]
fn every_header_fixture_identifies_as_expected() {
    let reg = embedded_registry();
    let mut wrong = Vec::new();
    for (name, listed, kind, arch, variant, quant) in TABLE {
        let id = identify_fixture(&reg, name, *listed);
        let got = (id.kind, id.arch.as_deref(), id.variant.as_deref(), id.quant.class.as_ref().map(|q| q.id()));
        let want = (*kind, *arch, *variant, Some((*quant).to_owned()));
        if got != want {
            wrong.push(format!("{name}: got {got:?}, want {want:?} ({})", id.evidence));
        }
        assert!(id.header_read && id.signature.is_some(), "{name}");
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

#[test]
fn every_fixture_is_in_the_table() {
    let listed: Vec<String> =
        std::fs::read_dir(dir()).unwrap().flatten().filter_map(|e| e.file_name().to_str()?.strip_suffix(".json").map(str::to_owned)).collect();
    let krea_loras = ["Comfy-Org_Krea-2__loras~", "gokaygokay_", "lvladikov_", "darask0_", "Comfy-Org_MOSS-Image__loras~"];
    for name in listed {
        let known = TABLE.iter().any(|(n, ..)| *n == name) || krea_loras.iter().any(|p| name.starts_with(p));
        assert!(known, "{name} has no expectation");
    }
}

#[test]
fn loras_of_bases_nothing_describes_are_marked_foreign() {
    let reg = embedded_registry();
    for (name, base) in [
        ("Comfy-Org_Krea-2__loras~krea2_style_reference", "other:krea2"),
        ("gokaygokay_Krea-2-Realism-LoRA__krea2_realism_lora", "other:krea-2"),
        ("Comfy-Org_MOSS-Image__loras~moss_image_color_depth_control", "other:moss-base"),
    ] {
        assert_eq!(identify_fixture(&reg, name, L).arch.as_deref(), Some(base), "{name}");
    }
    for name in ["Comfy-Org_Krea-2__loras~krea2_darkbrush", "darask0_MOSS-Image_4-step_adapter_lora__lora"] {
        assert_eq!(identify_fixture(&reg, name, L).arch, None, "{name}");
    }
}

#[test]
fn same_network_at_another_precision_has_the_same_signature() {
    let sig = |n: &str| identify::arch_signature(&header(n));
    assert_eq!(sig("Comfy-Org_Krea-2__diffusion_models~krea2_turbo_bf16"), sig("Comfy-Org_Krea-2__diffusion_models~krea2_turbo_nvfp4"));
    assert_eq!(sig("Comfy-Org_Krea-2__diffusion_models~krea2_turbo_bf16"), sig("Comfy-Org_Krea-2__diffusion_models~krea2_turbo_mxfp8"));
    assert_ne!(sig("owner__diffusion_models~flux3-image-klein-bf16"), sig("owner__diffusion_models~flux3-image-klein-base-bf16"));
    let nvfp4 = header("Comfy-Org_Krea-2__diffusion_models~krea2_turbo_nvfp4");
    let wq = nvfp4.tensors.iter().find(|t| t.name == "blocks.0.attn.wq.weight").unwrap();
    assert_eq!(identify::logical_shape(&nvfp4, wq), vec![6144, 6144]);
}

/// Finding 61: names and metadata that name a newer generation never map onto an older family.
#[test]
fn newer_generations_never_fall_back_to_older_families() {
    assert_eq!(arch::guess_from_name("flux3-image-klein-bf16.safetensors", D), None);
    assert_eq!(arch::guess_from_name("flux-3-image-klein-Q4_K_M.gguf", D), None);
    assert_eq!(arch::guess_from_name("hidream_o1_full_bf16.safetensors", D), None);
    assert_eq!(arch::family_from_modelspec("flux-3-image-klein"), None);
    assert_eq!(arch::family_from_modelspec("flux-2-klein-4b"), Some("flux2-klein"));
    for (name, fam) in [
        ("flux1-dev.safetensors", "flux1"),
        ("flux1-kontext-dev.safetensors", "flux1-kontext"),
        ("flux2_dev_fp8mixed.safetensors", "flux2"),
        ("flux-2-klein-4b.safetensors", "flux2-klein"),
        ("hidream_i1_full_fp8.safetensors", "hidream"),
        ("qwen_image_edit_2511_bf16.safetensors", "qwen-edit"),
        ("qwen_image_2512_bf16.safetensors", "qwen-image"),
        ("qwen_image_2.1_turbo_bf16.safetensors", "qwen-image-21"),
    ] {
        assert_eq!(arch::guess_from_name(name, D).as_deref(), Some(fam), "{name}");
    }
    assert_eq!(arch::guess_from_name("flux_3d_style_lora.safetensors", L).as_deref(), Some("flux1"));
    // A local header that matches no layout stays unknown whatever the name says.
    let unknown = arch::parse_safetensors_json(br#"{"new_architecture.weight": {"dtype": "BF16", "shape": [8, 8], "data_offsets": [0, 0]}}"#).unwrap();
    let reg = embedded_registry();
    for name in ["flux3-image-klein-bf16.safetensors", "flux2_dev.safetensors", "my_sdxl_model.safetensors"] {
        let id = identify::identify(name, D, Some(&unknown), &reg);
        assert_eq!(id.arch, None, "{name}");
    }
    assert_eq!(identify::identify("flux3-image-klein-bf16.safetensors", D, Some(&unknown), &reg).vetoed.as_deref(), Some("FLUX generation 3"));
    // Remote files (no header) still use the name, through the veto.
    assert_eq!(identify::identify("juggernautXL_v9.safetensors", C, None, &reg).arch.as_deref(), Some("sdxl"));
    assert_eq!(identify::identify("flux3-image-klein-bf16.safetensors", D, None, &reg).arch, None);
}

/// Finding 70: GGUF tensors decide before `general.architecture` (Klein vs dev).
#[test]
fn gguf_files_are_decided_by_their_tensors() {
    let reg = embedded_registry();
    let mut klein = fixture_json("unsloth_FLUX.2-klein-9B-GGUF__flux-2-klein-9b-Q4_K_M.gguf");
    klein["__metadata__"]["general.architecture"] = json!("flux");
    let p = std::env::temp_dir().join(format!("li-gguf-{}.gguf", uuid::Uuid::new_v4().simple()));
    std::fs::write(&p, gguf_bytes(&klein)).unwrap();
    let h = arch::read_header(&p).unwrap();
    let _ = std::fs::remove_file(&p);
    // A name without "klein": the modulation width still says Klein 9B.
    let id = identify::identify("model-Q8_0.gguf", D, Some(&h), &reg);
    assert_eq!((id.arch.as_deref(), id.variant.as_deref()), (Some("flux2-klein"), Some("9b")));
    assert!(id.evidence.starts_with("GGUF"), "{}", id.evidence);
    let lumina = arch::parse_safetensors_json(
        br#"{"cap_embedder.1.weight": {"dtype": "BF16", "shape": [2304, 2304], "data_offsets": [0, 0]}, "noise_refiner.0.attention.qkv.weight": {"dtype": "BF16", "shape": [6912, 2304], "data_offsets": [0, 0]}}"#,
    )
    .unwrap();
    let lumina = Header { gguf: true, metadata: [("general.architecture".to_owned(), "lumina2".to_owned())].into(), ..lumina };
    assert_eq!(arch::classify(&lumina, "lumina.gguf").family.as_deref(), Some("lumina2"));
}

/// Finding 68: a profile update can teach detection a new family, and a name-only rule can never
/// override what the weights say.
#[test]
fn detection_rules_arrive_as_profile_data() {
    let mut raw: BTreeMap<String, Value> = family::EMBEDDED.iter().map(|(k, t)| ((*k).to_owned(), serde_json::from_str(t).unwrap())).collect();
    raw.insert(
        "flux3-test".into(),
        json!({"schema": 2, "id": "flux3-test", "label": "FLUX.3 test", "kind": "template",
               "detect": {"priority": 10, "tensors_all": ["content_mode_blocks.image.0.attn_out.weight", "single_stream_modulations.txt.lin.weight"],
                          "variants": [{"id": "distilled", "tensors_all": ["ref_time_in.in_layer.weight"]}, {"id": "base", "tensors_none": ["ref_time_in.in_layer.weight"]}]}}),
    );
    let reg = Registry { families: family::build(&raw).unwrap(), ..Default::default() };
    let h = header("owner__diffusion_models~flux3-image-klein-bf16");
    for name in ["flux3-image-klein-bf16.safetensors", "flux-2-klein-4b.safetensors"] {
        let id = identify::identify(name, D, Some(&h), &reg);
        assert_eq!((id.arch.as_deref(), id.variant.as_deref()), (Some("flux3-test"), Some("distilled")), "{name}");
        assert_eq!(id.family, None, "a template-only family has no builder");
    }
    let base = identify::identify("flux3-image-klein-base-bf16.safetensors", D, Some(&header("owner__diffusion_models~flux3-image-klein-base-bf16")), &reg);
    assert_eq!(base.variant.as_deref(), Some("base"));
    // nvfp4-packed FLUX.2 text input: the logical width (12288) still says Klein 9B.
    let packed = arch::parse_safetensors_json(
        br#"{"txt_in.weight": {"dtype": "U8", "shape": [4096, 6144], "data_offsets": [0, 0]}, "txt_in.weight_scale_2": {"dtype": "F32", "shape": [], "data_offsets": [0, 0]},
             "single_stream_modulation.lin.weight": {"dtype": "BF16", "shape": [12288, 4096], "data_offsets": [0, 0]}}"#,
    )
    .unwrap();
    let d = arch::classify(&packed, "model.safetensors");
    assert_eq!((d.family.as_deref(), d.variant.as_deref()), (Some("flux2-klein"), Some("9b")));
    // A name-only block is rejected; a `refines` block may only re-label its built-in family.
    let mut bad = raw.clone();
    bad.insert("by-name".into(), json!({"schema": 2, "id": "by-name", "label": "N", "kind": "template", "detect": {"names": ["flux3*klein"]}}));
    assert!(family::build(&bad).is_err());
    let mut short = raw.clone();
    short.insert(
        "short".into(),
        json!({"schema": 2, "id": "short", "label": "S", "kind": "template", "detect": {"tensors_all": ["x.weight"], "names": ["ab"]}}),
    );
    assert!(family::build(&short).is_err());
}

#[test]
fn local_files_are_identified_from_their_headers_on_disk() {
    let d = std::env::temp_dir().join(format!("li-id-{}", uuid::Uuid::new_v4().simple()));
    std::fs::create_dir_all(d.join("diffusion_models")).unwrap();
    let fixture = "unsloth_Qwen-Image-2.1-GGUF__qwen-image-2.1-Q4_K_M.gguf";
    let p = d.join("diffusion_models").join("renamed.gguf");
    std::fs::write(&p, gguf_bytes(&fixture_json(fixture))).unwrap();
    let id = li_ai::inventory::identify_file(Path::new(&p), D, &embedded_registry()).unwrap();
    assert_eq!(id.arch.as_deref(), Some("qwen-image-21"));
    assert_eq!(id.quant.class.map(|q| q.id()).as_deref(), Some("gguf:Q4_K"));
    let _ = std::fs::remove_dir_all(d);
}
