use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::Error;
use li_ai::family::{
    Capabilities, Component, EMBEDDED, Family, FamilyKind, FileSpec, Loader, LoraRule, ModelDef, PresetDef, PromptRules, Range, Registry, Role, SCHEMA,
    Sampling, Sizes, build, merge, validate,
};
use serde_json::{Value, json};

fn embedded_raw() -> BTreeMap<String, Value> {
    EMBEDDED.iter().map(|(k, t)| ((*k).to_owned(), serde_json::from_str(t).unwrap_or_else(|e| panic!("{k}: {e}")))).collect()
}

fn valid_image_family(id: &str, label: &str) -> Family {
    serde_json::from_value::<Family>(json!({
        "id": id,
        "label": label,
        "kind": "image",
        "pipeline": {
            "sampler": "ksampler",
            "sampler_name": "euler",
            "scheduler": "normal",
            "latent": "EmptyLatentImage"
        },
        "sampling": {
            "steps": {"default": 20.0, "min": 1.0, "max": 50.0},
            "cfg": {"default": 7.0, "min": 1.0, "max": 20.0}
        }
    }))
    .unwrap()
}

fn unique_dir(tag: &str) -> PathBuf {
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).expect("clock before epoch").as_nanos();
    std::env::temp_dir().join(format!("li-ai-family-test-{tag}-{nanos}"))
}

#[test]
fn range_clamp_between() {
    let r = Range { default: 0.5, min: 0.0, max: 1.0 };
    assert!(!r.fixed());
    assert_eq!(r.clamp(-1.0), 0.0);
    assert_eq!(r.clamp(0.5), 0.5);
    assert_eq!(r.clamp(2.0), 1.0);
}

#[test]
fn range_clamp_nan() {
    let r = Range { default: 0.5, min: 0.0, max: 1.0 };
    assert!(r.clamp(f32::NAN).is_nan());
}

#[test]
fn range_fixed_clamps_to_value() {
    let r = Range { default: 1.0, min: 1.0, max: 1.0 };
    assert!(r.fixed());
    assert_eq!(r.clamp(-5.0), 1.0);
    assert_eq!(r.clamp(5.0), 1.0);
    // NaN input remains NaN: f32::clamp returns NaN when self is NaN.
    assert!(r.clamp(f32::NAN).is_nan());
}

#[test]
fn sizes_defaults_are_expected() {
    let s = Sizes::default();
    assert_eq!((s.native, s.multiple, s.min, s.max), (1024, 16, 256, 2048));
}

#[test]
fn role_loader_and_folder_are_correct() {
    let expectations = [
        (Role::Unet, ("UNETLoader", "diffusion_models")),
        (Role::Checkpoint, ("CheckpointLoaderSimple", "checkpoints")),
        (Role::Clip, ("CLIPLoader", "text_encoders")),
        (Role::Clip2, ("CLIPLoader", "text_encoders")),
        (Role::Clip3, ("CLIPLoader", "text_encoders")),
        (Role::Clip4, ("CLIPLoader", "text_encoders")),
        (Role::Vae, ("VAELoader", "vae")),
        (Role::Lora, ("LoraLoaderModelOnly", "loras")),
        (Role::ClipVision, ("CLIPVisionLoader", "clip_vision")),
        (Role::StyleModel, ("StyleModelLoader", "style_models")),
        (Role::Upscaler, ("UpscaleModelLoader", "upscale_models")),
    ];
    for (role, (loader, folder)) in expectations {
        assert_eq!(role.loader(), (loader, role.loader().1), "loader mismatch for {role:?}");
        assert_eq!(role.folder(), folder, "folder mismatch for {role:?}");
        assert_eq!(role.loader().1, role.loader().1); // input name present
    }
}

#[test]
fn role_clip_slot_identifies_clip_roles() {
    assert_eq!(Role::Clip.clip_slot(), Some(0));
    assert_eq!(Role::Clip2.clip_slot(), Some(1));
    assert_eq!(Role::Clip3.clip_slot(), Some(2));
    assert_eq!(Role::Clip4.clip_slot(), Some(3));
    assert_eq!(Role::Unet.clip_slot(), None);
    assert_eq!(Role::Checkpoint.clip_slot(), None);
    assert_eq!(Role::Vae.clip_slot(), None);
    assert_eq!(Role::Lora.clip_slot(), None);
}

#[test]
fn merge_nested_objects_deeply() {
    let mut base = json!({"a": {"b": 1, "c": 2}, "d": 3});
    let over = json!({"a": {"b": 10, "e": 20}, "f": 30});
    merge(&mut base, &over);
    assert_eq!(base, json!({"a": {"b": 10, "c": 2, "e": 20}, "d": 3, "f": 30}));
}

#[test]
fn merge_scalar_overwrites_non_objects() {
    let mut base = json!({"a": 1, "b": [1, 2]});
    let over = json!({"a": "x", "b": [3]});
    merge(&mut base, &over);
    assert_eq!(base, json!({"a": "x", "b": [3]}));
}

#[test]
fn built_in_profiles_load_and_inherit() {
    let fams = build(&embedded_raw()).expect("embedded profiles build");
    assert_eq!(fams.len(), EMBEDDED.len());
    let reg = Registry { families: fams, sources: BTreeMap::new() };
    let pony = reg.family("pony").expect("pony exists");
    let sdxl = reg.family("sdxl").expect("sdxl exists");
    assert_eq!(pony.pipeline.loader, Loader::Checkpoint);
    assert_eq!(pony.sizes.native, sdxl.sizes.native);
    assert!(pony.prompt.prefix.contains("score_9"));
    assert!(pony.lora_fits("pony") && pony.lora_fits("sdxl"));
    assert!(!sdxl.lora_fits("flux1"));

    let kontext = reg.family("flux1-kontext").expect("kontext exists");
    assert!(kontext.capabilities.edit);
    assert_eq!(kontext.pipeline.clip.node, "DualCLIPLoader");

    for key in ["qwen", "z-image-turbo", "flux2-klein-4b", "flux2-klein-9b", "ernie-image", "seedvr2", "klein-remove"] {
        assert!(reg.model(key).is_some(), "missing model {key}");
    }
    assert_eq!(reg.groups().first().map(|(g, _)| g.as_str()), Some("Stable Diffusion"));
}

#[test]
fn model_overrides_apply_to_family() {
    let reg = Registry { families: build(&embedded_raw()).unwrap(), sources: BTreeMap::new() };
    let (f, m) = reg.model("klein-remove").expect("model exists");
    let r = f.resolved_for(m);
    assert_eq!(r.sampling.steps.default, 28.0);

    let (f, m) = reg.model("flux2-klein-4b").unwrap();
    assert!(f.resolved_for(m).sampling.steps.fixed());
}

#[test]
fn build_base_chain_too_deep_errors() {
    let mut raw = BTreeMap::new();
    raw.insert("a".to_owned(), json!({"schema": 1, "id": "a", "base": "b"}));
    raw.insert("b".to_owned(), json!({"schema": 1, "id": "b", "base": "a"}));
    assert!(build(&raw).is_err());
}

#[test]
fn build_unknown_base_errors() {
    let mut raw = BTreeMap::new();
    raw.insert("a".to_owned(), json!({"schema": 1, "id": "a", "base": "missing"}));
    assert!(build(&raw).is_err());
}

#[test]
fn build_skips_future_schema() {
    let mut raw = embedded_raw();
    raw.insert("future".to_owned(), json!({"schema": SCHEMA + 1, "id": "future", "label": "Future"}));
    let fams = build(&raw).expect("build should succeed, skipping future");
    assert!(fams.iter().all(|f| f.id != "future"));
    assert_eq!(fams.len(), EMBEDDED.len());
}

#[test]
fn validate_accepts_embedded_profiles() {
    let fams = build(&embedded_raw()).expect("embedded profiles build");
    for f in &fams {
        validate(f).expect("embedded profile validates");
    }
}

#[test]
fn validate_rejects_empty_id_label() {
    let mut f = valid_image_family("", "Label");
    assert!(validate(&f).is_err());

    f = valid_image_family("id", "");
    assert!(validate(&f).is_err());
}

#[test]
fn validate_rejects_bad_sampler_settings() {
    let mut f = valid_image_family("bad-sampler", "Bad Sampler");
    f.pipeline.sampler_name = String::new();
    assert!(validate(&f).is_err());

    let mut f = valid_image_family("bad-fixed-range", "Bad Range");
    // Force a fixed range with max == min == 0 (invalid because max == 0)
    f.sampling.steps = Range { default: 0.0, min: 0.0, max: 0.0 };
    assert!(validate(&f).is_err());
}

#[test]
fn validate_rejects_model_missing_key() {
    let mut f = valid_image_family("model-issues", "Model Issues");
    f.models.push(ModelDef { key: String::new(), ..Default::default() });
    assert!(validate(&f).is_err());
}

#[test]
fn validate_rejects_preset_bad_sha() {
    let mut f = valid_image_family("bad-sha", "Bad SHA");
    f.models.push(ModelDef {
        key: "bad-sha-model".to_owned(),
        presets: vec![PresetDef {
            files: vec![FileSpec { name: "model.safetensors".to_owned(), bytes: 1024, sha256: "not-hex".to_owned(), ..Default::default() }],
            ..Default::default()
        }],
        ..Default::default()
    });
    assert!(validate(&f).is_err());
}

#[test]
fn validate_rejects_preset_zero_bytes() {
    let mut f = valid_image_family("zero-bytes", "Zero Bytes");
    f.models.push(ModelDef {
        key: "zero-bytes-model".to_owned(),
        presets: vec![PresetDef {
            files: vec![FileSpec { name: "model.safetensors".to_owned(), bytes: 0, sha256: "a".repeat(64), ..Default::default() }],
            ..Default::default()
        }],
        ..Default::default()
    });
    assert!(validate(&f).is_err());
}

#[test]
fn validate_rejects_preset_bad_extension() {
    let mut f = valid_image_family("bad-ext", "Bad Extension");
    f.models.push(ModelDef {
        key: "bad-ext-model".to_owned(),
        presets: vec![PresetDef {
            files: vec![FileSpec { name: "model.bin".to_owned(), bytes: 1024, sha256: "a".repeat(64), ..Default::default() }],
            ..Default::default()
        }],
        ..Default::default()
    });
    assert!(validate(&f).is_err());
}

#[test]
fn family_resolved_for_model_applies_overrides() {
    let mut fam = valid_image_family("base", "Base");
    fam.sampling.steps = Range { default: 20.0, min: 1.0, max: 50.0 };
    fam.capabilities = Capabilities { create: true, ..Default::default() };

    let model = ModelDef {
        key: "m".to_owned(),
        sampling: Some(Sampling { steps: Range { default: 30.0, min: 5.0, max: 100.0 }, cfg: Range { default: 8.0, min: 1.0, max: 20.0 }, denoise: 0.7 }),
        capabilities: Some(Capabilities { edit: true, ..Default::default() }),
        vram_gb: Some(16),
        ..Default::default()
    };

    let resolved = fam.resolved_for(&model);
    assert_eq!(resolved.sampling.steps.default, 30.0);
    assert_eq!(resolved.sampling.steps.min, 5.0);
    assert_eq!(resolved.sampling.steps.max, 100.0);
    assert_eq!(resolved.sampling.cfg.default, 8.0);
    assert_eq!(resolved.sampling.denoise, 0.7);
    assert!(resolved.capabilities.edit);
    assert_eq!(resolved.vram_gb, 16);
}

#[test]
fn family_lora_fits_checks_loader_and_compatibility() {
    let mut fam = valid_image_family("fam", "Fam");
    fam.lora = LoraRule { loader: String::new(), compatible: vec![], max: 0 };
    assert!(!fam.lora_fits("fam"));
    assert!(!fam.lora_fits("other"));

    fam.lora.loader = "LoraLoader".to_owned();
    assert!(fam.lora_fits("fam"));
    assert!(!fam.lora_fits("other"));

    fam.lora.compatible = vec!["other".to_owned()];
    assert!(fam.lora_fits("other"));
    assert!(!fam.lora_fits("third"));
}

#[test]
fn registry_family_and_model_lookup() {
    let reg = Registry { families: build(&embedded_raw()).unwrap(), sources: BTreeMap::new() };
    assert!(reg.family("sd15").is_some());
    assert!(reg.family("nonexistent").is_none());
    let (f, m) = reg.model("qwen").expect("qwen exists");
    assert_eq!(m.key, "qwen");
    assert!(f.models.iter().any(|model| model.key == "qwen"));
}

#[test]
fn registry_groups_preserve_order_and_group_names() {
    let reg = Registry { families: build(&embedded_raw()).unwrap(), sources: BTreeMap::new() };
    let groups = reg.groups();
    assert!(!groups.is_empty());
    assert_eq!(groups[0].0, "Stable Diffusion");
    // SD families are first
    let sd_ids: Vec<&str> = groups[0].1.iter().map(|f| f.id.as_str()).collect();
    assert!(sd_ids.contains(&"sd15"));
    assert!(sd_ids.contains(&"sdxl"));
    // All families appear exactly once
    let total: usize = groups.iter().map(|(_, v)| v.len()).sum();
    assert_eq!(total, reg.families.len());
    // Every family belongs to its group
    for f in &reg.families {
        assert!(groups.iter().any(|(g, v)| *g == f.group && v.iter().any(|x| x.id == f.id)));
    }
}

#[test]
fn load_user_profiles_override_embedded() {
    let dir = unique_dir("load-user");
    let user_dir = dir.join("user");
    std::fs::create_dir_all(&user_dir).unwrap();

    let mut updated: Value = serde_json::from_str(EMBEDDED.iter().find(|(k, _)| *k == "sd15").unwrap().1).unwrap();
    updated["version"] = serde_json::json!(999);
    updated["label"] = serde_json::json!("SD 1.5 (user)");
    std::fs::write(user_dir.join("sd15.json"), serde_json::to_vec(&updated).unwrap()).unwrap();

    let reg = li_ai::family::load(&dir);
    assert_eq!(reg.family("sd15").unwrap().label, "SD 1.5 (user)");
    assert_eq!(reg.sources.get("sd15").map(String::as_str), Some("user"));

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn load_bad_user_profile_falls_back_to_builtin() {
    let dir = unique_dir("load-bad");
    let user_dir = dir.join("user");
    std::fs::create_dir_all(&user_dir).unwrap();
    std::fs::write(user_dir.join("bad.json"), b"{ not valid json").unwrap();

    let reg = li_ai::family::load(&dir);
    assert!(reg.family("sd15").is_some(), "built-in sd15 should still load");

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn update_from_downloads_newer_profile() {
    let dir = unique_dir("update-good");
    std::fs::create_dir_all(&dir).unwrap();

    let mut newer: Value = serde_json::from_str(EMBEDDED.iter().find(|(k, _)| *k == "sd15").unwrap().1).unwrap();
    newer["version"] = serde_json::json!(999);
    newer["label"] = serde_json::json!("SD 1.5 (updated)");
    let body = serde_json::to_vec(&newer).unwrap();

    let fetch = |url: &str| -> Result<Vec<u8>, Error> {
        if url.ends_with("/index.json") {
            Ok(br#"{"families": {"sd15": 999, "sdxl": 0, "../evil": 5}}"#.to_vec())
        } else if url.ends_with("/sd15.json") {
            Ok(body.clone())
        } else {
            Err(anyhow::anyhow!("404"))
        }
    };

    let updated = li_ai::family::update_from(&fetch, "https://example.invalid/f", &dir).unwrap();
    assert_eq!(updated, vec!["sd15".to_owned()]);
    let reg = li_ai::family::load(&dir);
    assert_eq!(reg.family("sd15").unwrap().label, "SD 1.5 (updated)");
    assert_eq!(reg.sources["sd15"], "update");

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn update_from_rejects_bad_profile() {
    let dir = unique_dir("update-bad");
    std::fs::create_dir_all(&dir).unwrap();

    let fetch = |url: &str| -> Result<Vec<u8>, Error> {
        if url.ends_with("/index.json") {
            Ok(br#"{"families": {"broken": 123}}"#.to_vec())
        } else if url.ends_with("/broken.json") {
            Ok(b"{ not valid json".to_vec())
        } else {
            Err(anyhow::anyhow!("404"))
        }
    };

    let updated = li_ai::family::update_from(&fetch, "https://example.invalid/f", &dir).unwrap();
    assert!(updated.is_empty(), "invalid update should be skipped");
    assert!(!dir.join("broken.json").exists(), "no file should be written for invalid profile");

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn component_and_file_spec_round_trip_deserialization() {
    let json = json!({
        "role": "clip",
        "names": ["t5xxl_fp16.safetensors"],
        "patterns": ["t5xxl"],
        "download": {
            "role": "clip",
            "folder": "text_encoders",
            "name": "t5xxl_fp16.safetensors",
            "bytes": 1000,
            "sha256": "a".repeat(64),
            "url": "https://example.invalid/file",
            "compatible": [[2000, "b".repeat(64)]]
        },
        "source": "https://example.invalid"
    });
    let c: Component = serde_json::from_value(json).unwrap();
    assert_eq!(c.role, Role::Clip);
    assert_eq!(c.names, vec!["t5xxl_fp16.safetensors".to_owned()]);
    assert_eq!(c.patterns, vec!["t5xxl".to_owned()]);
    let spec = c.download.unwrap();
    assert_eq!(spec.bytes, 1000);
    assert_eq!(spec.sha256, "a".repeat(64));
    assert_eq!(spec.compatible.len(), 1);
    assert_eq!(spec.compatible[0].0, 2000);
    assert_eq!(spec.compatible[0].1, "b".repeat(64));
}

#[test]
fn family_kind_and_pipeline_defaults_are_consistent() {
    let f = Family::default();
    assert_eq!(f.schema, 0);
    assert_eq!(f.version, 0);
    assert_eq!(f.kind, FamilyKind::Image);
    assert_eq!(f.pipeline.loader, Loader::Checkpoint);
    assert_eq!(f.pipeline.encode, li_ai::family::Encode::ClipText);
    assert_eq!(f.pipeline.sampler, li_ai::family::SamplerStyle::Ksampler);
    assert_eq!(f.pipeline.reference, li_ai::family::RefMethod::None);
    assert_eq!(f.pipeline.inpaint, li_ai::family::InpaintMethod::None);
    assert_eq!(f.sizes, Sizes::default());
    assert_eq!(f.sampling, Sampling::default());
    assert_eq!(f.capabilities, Capabilities::default());
    assert_eq!(f.lora, LoraRule::default());
    assert_eq!(f.prompt, PromptRules::default());
}
