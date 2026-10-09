use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use li_ai::arch::{FileKind, classify, family_from_modelspec, guess_from_name, name_looks_like_lora, parse_safetensors_json, read_header, read_safetensors};

static DIR_COUNTER: AtomicU64 = AtomicU64::new(0);

struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> Self {
        let unique = DIR_COUNTER.fetch_add(1, Ordering::Relaxed);
        let mut p = std::env::temp_dir();
        p.push(format!("li-ai-arch-test-{}-{}-{}", tag, std::process::id(), unique));
        std::fs::create_dir_all(&p).expect("create temp dir");
        TempDir(p)
    }

    fn file(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn build_json(tensors: &[(&str, &[u64])], metadata: Option<&str>) -> Vec<u8> {
    let mut entries = Vec::new();
    for (name, shape) in tensors {
        let shape_str = shape.iter().map(|d| d.to_string()).collect::<Vec<_>>().join(",");
        entries.push(format!("\"{}\":{{\"dtype\":\"F16\",\"shape\":[{}],\"data_offsets\":[0,0]}}", name, shape_str));
    }
    if let Some(meta) = metadata {
        entries.push(format!("\"__metadata__\":{}", meta));
    }
    let json = format!("{{{}}}", entries.join(","));
    json.into_bytes()
}

fn safetensors_bytes(tensors: &[(&str, &[u64])], metadata: Option<&str>) -> Vec<u8> {
    let header = build_json(tensors, metadata);
    let mut out = Vec::with_capacity(8 + header.len());
    out.extend_from_slice(&(header.len() as u64).to_le_bytes());
    out.extend_from_slice(&header);
    out
}

fn push_gguf_string(out: &mut Vec<u8>, s: &str) {
    out.extend_from_slice(&(s.len() as u64).to_le_bytes());
    out.extend_from_slice(s.as_bytes());
}

#[test]
fn parse_safetensors_json_empty_object_returns_empty_header() {
    let h = parse_safetensors_json(b"{}").unwrap();
    assert!(h.tensors.is_empty());
    assert!(h.metadata.is_empty());
    assert!(!h.gguf);
}

#[test]
fn parse_safetensors_json_malformed_json_returns_err() {
    assert!(parse_safetensors_json(b"not json").is_err());
    assert!(parse_safetensors_json(b"[]").is_err());
}

#[test]
fn read_safetensors_zero_header_length_returns_err() {
    let mut data = Vec::new();
    data.extend_from_slice(&0u64.to_le_bytes());
    assert!(read_safetensors(&mut data.as_slice()).is_err());
}

#[test]
fn read_safetensors_header_length_over_max_returns_err() {
    let mut data = Vec::new();
    data.extend_from_slice(&((64u64 << 20) + 1).to_le_bytes());
    assert!(read_safetensors(&mut data.as_slice()).is_err());
}

#[test]
fn read_safetensors_truncated_json_body_returns_err() {
    let mut data = Vec::new();
    data.extend_from_slice(&100u64.to_le_bytes());
    data.extend_from_slice(b"short");
    assert!(read_safetensors(&mut data.as_slice()).is_err());
}

#[test]
fn read_safetensors_valid_header_parses_tensors_and_metadata() {
    let bytes = safetensors_bytes(&[("z.weight", &[2, 3]), ("a.weight", &[1])], Some(r#"{"num":42,"str":"value","bool":true,"null":null}"#));
    let mut cursor = bytes.as_slice();
    let h = read_safetensors(&mut cursor).unwrap();
    assert_eq!(h.tensors.len(), 2);
    assert_eq!(h.tensors[0].name, "a.weight");
    assert_eq!(h.tensors[0].shape, vec![1u64]);
    assert_eq!(h.tensors[0].dtype, "F16");
    assert_eq!(h.tensors[1].name, "z.weight");
    assert_eq!(h.tensors[1].shape, vec![2u64, 3u64]);
    assert_eq!(h.meta("num"), Some("42"));
    assert_eq!(h.meta("str"), Some("value"));
    assert_eq!(h.meta("bool"), Some("true"));
    assert_eq!(h.meta("null"), Some("null"));
}

#[test]
fn header_tensor_query_methods_work() {
    let h = parse_safetensors_json(&build_json(
        &[
            ("model.diffusion_model.input_blocks.0.0.weight", &[320, 4, 3, 3]),
            ("model.diffusion_model.joint_blocks.23.context_block.attn.qkv.weight", &[4608, 1536]),
            ("model.diffusion_model.joint_blocks.7.x_block.mlp.fc1.weight", &[1, 1]),
        ],
        None,
    ))
    .unwrap();

    assert!(h.has("model.diffusion_model.input_blocks.0.0.weight"));
    assert!(!h.has("nope"));

    assert_eq!(h.shape("model.diffusion_model.input_blocks.0.0.weight"), Some(&[320u64, 4u64, 3u64, 3u64][..]));
    assert_eq!(h.shape("nope"), None);

    let t = h.find_suffix("input_blocks.0.0.weight").unwrap();
    assert_eq!(t.name, "model.diffusion_model.input_blocks.0.0.weight");
    assert_eq!(t.shape, vec![320u64, 4u64, 3u64, 3u64]);

    assert!(h.any_contains("joint_blocks."));
    assert!(!h.any_contains("double_blocks."));
    assert!(h.any_prefix("model.diffusion_model."));
    assert!(!h.any_prefix("cond_stage_model."));

    assert_eq!(h.block_count("joint_blocks."), 24);
    assert_eq!(h.block_count("double_blocks."), 0);
}

#[test]
fn header_block_count_no_matches_returns_zero() {
    let h = parse_safetensors_json(&build_json(&[("a.weight", &[1])], None)).unwrap();
    assert_eq!(h.block_count("transformer_blocks."), 0);
}

#[test]
fn family_from_modelspec_normalizes_common_architectures() {
    let cases = [
        ("stable-diffusion-xl-v1-base", Some("sdxl")),
        ("sdxl_base", Some("sdxl")),
        ("stable-diffusion-v1-5", Some("sd15")),
        ("sd-1", Some("sd15")),
        ("stable-diffusion-v3-medium", Some("sd35")),
        ("SD3", Some("sd35")),
        ("flux-1-dev", Some("flux1")),
        ("flux-2", Some("flux2")),
        ("qwen-image-edit", Some("qwen-edit")),
        ("qwen_image", Some("qwen-image")),
        ("hidream-i1", Some("hidream")),
        ("z-image-turbo", Some("z-image")),
        ("ernie-4.5", Some("ernie")),
        ("pony", Some("pony")),
        ("illustrious", Some("illustrious")),
        ("flux-1-dev/lora", Some("flux1")),
        ("some-unknown-arch", None),
    ];
    for (input, expected) in cases {
        assert_eq!(family_from_modelspec(input), expected, "input: {input}");
    }
}

#[test]
fn classify_metadata_modelspec_wins_over_weights() {
    let h = parse_safetensors_json(&build_json(
        &[("lora_unet_x.lora_down.weight", &[16, 64])],
        Some(r#"{"modelspec.architecture":"stable-diffusion-xl-v1-base/lora","modelspec.title":"Pony something"}"#),
    ))
    .unwrap();

    let d = classify(&h, "whatever.safetensors");
    assert_eq!(d.kind, FileKind::Lora);
    assert_eq!(d.family.as_deref(), Some("pony"));
    assert!(d.evidence.contains("modelspec.architecture"));
}

#[test]
fn classify_sd15_inpainting_from_header() {
    let h = parse_safetensors_json(&build_json(
        &[
            ("model.diffusion_model.input_blocks.0.0.weight", &[320, 9, 3, 3]),
            ("model.diffusion_model.input_blocks.1.1.transformer_blocks.0.attn2.to_k.weight", &[320, 768]),
        ],
        None,
    ))
    .unwrap();

    let d = classify(&h, "model.safetensors");
    assert_eq!(d.kind, FileKind::DiffusionModel);
    assert_eq!(d.family.as_deref(), Some("sd15"));
    assert!(d.evidence.contains("inpainting"));
}

#[test]
fn classify_sdxl_refiner_from_label_emb() {
    let h = parse_safetensors_json(&build_json(
        &[("model.diffusion_model.input_blocks.0.0.weight", &[320, 4, 3, 3]), ("model.diffusion_model.label_emb.0.0.weight", &[1280, 2560])],
        None,
    ))
    .unwrap();

    let d = classify(&h, "refiner.safetensors");
    assert_eq!(d.family.as_deref(), Some("sdxl-refiner"));
}

#[test]
fn classify_flux_fill_from_img_in_width() {
    let h = parse_safetensors_json(&build_json(&[("double_blocks.0.img_attn.qkv.weight", &[9216, 3072]), ("img_in.weight", &[3072, 384])], None)).unwrap();

    let d = classify(&h, "fill.safetensors");
    assert_eq!(d.family.as_deref(), Some("flux1-fill"));
}

#[test]
fn classify_qwen_edit_marker_tensor() {
    let h = parse_safetensors_json(&build_json(
        &[("transformer_blocks.0.img_mod.1.weight", &[18432, 3072]), ("txt_norm.weight", &[3584]), ("__index_timestep_zero__", &[1])],
        None,
    ))
    .unwrap();

    let d = classify(&h, "qwen.safetensors");
    assert_eq!(d.family.as_deref(), Some("qwen-edit"));
    assert!(d.evidence.contains("__index_timestep_zero__"));
}

#[test]
fn classify_lora_from_kohya_metadata() {
    let h =
        parse_safetensors_json(&build_json(&[("lora_unet_down_blocks_0.lora_down.weight", &[8, 320])], Some(r#"{"ss_base_model_version":"sdxl_base_v1-0"}"#)))
            .unwrap();

    let d = classify(&h, "detail.safetensors");
    assert_eq!(d.kind, FileKind::Lora);
    assert_eq!(d.family.as_deref(), Some("sdxl"));
    assert!(d.evidence.contains("ss_base_model_version"));
}

#[test]
fn name_looks_like_lora_detects_words_and_camel_case() {
    let truthy = [
        "lora",
        "lora_v2",
        "my_lora",
        "styleLoRA",
        "detailLora",
        "myStyleLoRA_v2",
        "sub/loras/thing.safetensors",
        "lycoris_ink.safetensors",
        "lokr_test.safetensors",
        "lora2.safetensors",
    ];
    for name in truthy {
        assert!(name_looks_like_lora(name), "expected lora: {name}");
    }

    let falsy = [
        "floral_dream_xl.safetensors",
        "FLORA.safetensors",
        "Flora_v1.safetensors",
        "juggernautXL_v9.safetensors",
        "colorful_xl.safetensors",
        "lorem_ipsum_lorem.safetensors",
    ];
    for name in falsy {
        assert!(!name_looks_like_lora(name), "expected not lora: {name}");
    }
}

#[test]
fn guess_from_name_recognizes_families_and_priorities() {
    assert_eq!(guess_from_name("qwen_image_edit_2511_bf16.safetensors", FileKind::DiffusionModel).as_deref(), Some("qwen-edit"));
    assert_eq!(guess_from_name("a_qwen_image_2.1_x.safetensors", FileKind::DiffusionModel).as_deref(), Some("qwen-image-21"));
    assert_eq!(guess_from_name("flux2-klein.safetensors", FileKind::DiffusionModel).as_deref(), Some("flux2-klein"));
    assert_eq!(guess_from_name("some_flux_model.safetensors", FileKind::DiffusionModel).as_deref(), Some("flux1"));
    assert_eq!(guess_from_name("random.safetensors", FileKind::DiffusionModel), None);
}

#[test]
fn guess_from_name_lora_folder_keeps_family_but_model_folder_rejects() {
    let qwen_lora = "qwen_image_lora_v1.safetensors";
    assert_eq!(guess_from_name(qwen_lora, FileKind::Lora).as_deref(), Some("qwen-image"));
    assert_eq!(guess_from_name(qwen_lora, FileKind::Checkpoint), None);
    assert_eq!(guess_from_name(qwen_lora, FileKind::DiffusionModel), None);

    let flux_lora = "flux_realism_lora.safetensors";
    assert_eq!(guess_from_name(flux_lora, FileKind::Lora).as_deref(), Some("flux1"));
    assert_eq!(guess_from_name(flux_lora, FileKind::DiffusionModel), None);
}

#[test]
fn read_header_safetensors_file() {
    let tmp = TempDir::new("safetensors-read");
    let path = tmp.file("model.safetensors");
    let bytes = safetensors_bytes(&[("test.weight", &[2, 2])], None);
    std::fs::write(&path, &bytes).unwrap();

    let h = read_header(&path).unwrap();
    assert_eq!(h.tensors.len(), 1);
    assert_eq!(h.tensors[0].name, "test.weight");
    assert!(!h.gguf);
}

#[test]
fn read_header_gguf_file_parses_metadata_and_tensors() {
    let tmp = TempDir::new("gguf-read");
    let path = tmp.file("model.gguf");

    let mut data = b"GGUF".to_vec();
    data.extend_from_slice(&3u32.to_le_bytes());
    data.extend_from_slice(&1u64.to_le_bytes()); // tensor count
    data.extend_from_slice(&1u64.to_le_bytes()); // kv count

    push_gguf_string(&mut data, "general.architecture");
    data.extend_from_slice(&8u32.to_le_bytes()); // type string
    push_gguf_string(&mut data, "flux");

    push_gguf_string(&mut data, "double_blocks.0.img_attn.qkv.weight");
    data.extend_from_slice(&2u32.to_le_bytes()); // dims
    data.extend_from_slice(&3072u64.to_le_bytes());
    data.extend_from_slice(&9216u64.to_le_bytes());
    data.extend_from_slice(&8u32.to_le_bytes()); // tensor type
    data.extend_from_slice(&0u64.to_le_bytes()); // offset

    std::fs::write(&path, &data).unwrap();

    let h = read_header(&path).unwrap();
    assert!(h.gguf);
    assert_eq!(h.tensors.len(), 1);
    assert_eq!(h.tensors[0].shape, vec![9216u64, 3072u64]);
    assert_eq!(h.meta("general.architecture"), Some("flux"));

    let d = classify(&h, "flux1-dev-Q4_K_S.gguf");
    assert_eq!(d.kind, FileKind::DiffusionModel);
    assert_eq!(d.family.as_deref(), Some("flux1"));
}

#[test]
fn read_header_empty_file_returns_err() {
    let tmp = TempDir::new("empty-read");
    let path = tmp.file("empty.bin");
    std::fs::write(&path, b"").unwrap();
    assert!(read_header(&path).is_err());
}

#[test]
fn read_header_missing_file_returns_err() {
    let tmp = TempDir::new("missing-read");
    let path = tmp.file("nope.safetensors");
    assert!(read_header(&path).is_err());
}

#[test]
fn read_header_gguf_unsupported_version_returns_err() {
    let tmp = TempDir::new("gguf-v1");
    let path = tmp.file("bad.gguf");
    let mut data = b"GGUF".to_vec();
    data.extend_from_slice(&1u32.to_le_bytes());
    std::fs::write(&path, &data).unwrap();
    assert!(read_header(&path).is_err());
}

#[test]
fn read_header_gguf_too_many_tensors_returns_err() {
    let tmp = TempDir::new("gguf-too-many");
    let path = tmp.file("bad.gguf");
    let mut data = b"GGUF".to_vec();
    data.extend_from_slice(&3u32.to_le_bytes());
    data.extend_from_slice(&((1u64 << 20) + 1).to_le_bytes());
    data.extend_from_slice(&0u64.to_le_bytes());
    std::fs::write(&path, &data).unwrap();
    assert!(read_header(&path).is_err());
}

#[test]
fn read_header_gguf_unknown_value_type_returns_err() {
    let tmp = TempDir::new("gguf-unknown-type");
    let path = tmp.file("bad.gguf");
    let mut data = b"GGUF".to_vec();
    data.extend_from_slice(&3u32.to_le_bytes());
    data.extend_from_slice(&0u64.to_le_bytes()); // tensors
    data.extend_from_slice(&1u64.to_le_bytes()); // kvs
    push_gguf_string(&mut data, "test");
    data.extend_from_slice(&99u32.to_le_bytes()); // unknown type
    std::fs::write(&path, &data).unwrap();
    assert!(read_header(&path).is_err());
}
