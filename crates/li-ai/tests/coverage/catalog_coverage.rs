use li_ai::catalog;
use li_ai::catalog::catalog as current_catalog;
use li_ai::catalog::{InstalledModel, ModelId, Preset, Role, availability, default_variant, preset, presets, presets_for};
use li_ai::comfy::ObjectInfo;
use serde_json::json;

#[test]
fn model_id_constants_key_and_from_key() {
    assert_eq!(ModelId::Qwen.key(), "qwen");
    assert_eq!(ModelId::from_key("qwen"), Some(ModelId::Qwen));
    assert_eq!(ModelId::from_key("flux2-klein-4b"), Some(ModelId::Klein4B));
    assert_eq!(ModelId::from_key("nonexistent"), None);
    assert_eq!(ModelId::ORIGINAL.len(), 7);
    let all = ModelId::all();
    assert!(all.contains(&ModelId::Qwen));
    assert!(all.contains(&ModelId::KleinRemove));
}

#[test]
fn model_id_intern_edge_cases() {
    let a = ModelId::intern("qwen");
    assert_eq!(a, ModelId::Qwen);
    let unknown = ModelId::intern("totally-new-model-key");
    assert_eq!(unknown.key(), "totally-new-model-key");
    assert_eq!(ModelId::intern("totally-new-model-key"), unknown);
    let empty = ModelId::intern("");
    assert_eq!(empty.key(), "");
    assert!(ModelId::from_key("").is_none());
    let long = "x".repeat(10_000);
    let m = ModelId::intern(&long);
    assert_eq!(m.key().len(), 10_000);
}

#[test]
fn model_id_serde_roundtrip_and_invalid_type() {
    let m = ModelId::Qwen;
    let s = serde_json::to_string(&m).unwrap();
    assert_eq!(s, "\"qwen\"");
    let back: ModelId = serde_json::from_str(&s).unwrap();
    assert_eq!(back, m);
    let u: ModelId = serde_json::from_str("\"some-new-key\"").unwrap();
    assert_eq!(u.key(), "some-new-key");
    let err = serde_json::from_str::<ModelId>("123").unwrap_err();
    assert!(err.to_string().contains("string"));
}

#[test]
fn model_id_info_and_try_info_for_unknown() {
    let cat = current_catalog();
    let first = cat.models.first().expect("catalogue non-empty");
    let unknown = ModelId::intern("definitely-not-in-catalog");
    assert!(unknown.try_info().is_none());
    let info = unknown.info();
    assert_eq!(info.id, first.id);
    let qwen = ModelId::Qwen.info();
    assert_eq!(qwen.id, ModelId::Qwen);
}

#[test]
fn generators_exclude_tool_models_and_have_correct_flags() {
    let gens = ModelId::generators();
    for g in &gens {
        let info = g.try_info().expect("generator has info");
        assert!(info.text_to_image && !info.tool, "{g:?} should be image generator");
    }
    assert!(!gens.contains(&ModelId::KleinRemove));
    assert!(gens.contains(&ModelId::Qwen));
    let all = ModelId::all();
    assert!(all.contains(&ModelId::KleinRemove));
}

/// Serialises the tests that replace the installed models (which rebuilds the shared catalogue) with the one that
/// expects the catalogue to stay the same between two reads.
static CATALOG_STATE: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[test]
fn catalog_snapshot_is_deterministic_and_self_consistent() {
    let _state = CATALOG_STATE.lock().unwrap_or_else(|e| e.into_inner());
    let c1 = current_catalog();
    let c2 = current_catalog();
    assert!(std::ptr::eq(c1, c2));
    for p in &c1.presets {
        assert!(c1.models.iter().any(|m| m.id == p.model), "preset {} has no model", p.id());
    }
    for m in ModelId::ORIGINAL {
        assert!(c1.presets.iter().any(|p| p.model == m));
    }
}

#[test]
fn presets_for_and_preset_lookup_work() {
    for m in ModelId::ORIGINAL {
        let mut presets = presets_for(m);
        let first = presets.next().expect("original model has preset");
        assert_eq!(first.model, m);
        assert!(presets.all(|p| p.model == m));
        let variant = first.variant.clone();
        let p = preset(m, &variant).expect("preset for variant");
        assert_eq!(p.model, m);
        assert_eq!(p.variant, variant);
    }
    let unknown = ModelId::intern("no-presets-model");
    assert_eq!(presets_for(unknown).count(), 0);
    assert!(preset(unknown, "bf16").is_none());
}

#[test]
fn default_variant_and_preset_id_format() {
    for m in ModelId::ORIGINAL {
        let first_variant = presets_for(m).next().map(|p| p.variant.as_str()).unwrap_or("bf16");
        assert_eq!(default_variant(m), first_variant);
    }
    let unknown = ModelId::intern("unknown-variant-model");
    assert_eq!(default_variant(unknown), "bf16");
    for p in presets().iter().take(10) {
        assert_eq!(p.id(), format!("{}:{}", p.model.key(), p.variant));
    }
}

#[test]
fn preset_total_bytes_sums_files() {
    for p in presets().iter().filter(|p| !p.files.is_empty()).take(5) {
        let sum: u64 = p.files.iter().map(|f| f.bytes).sum();
        assert_eq!(p.total_bytes(), sum);
    }
    let mut empty_preset = presets()[0].clone();
    empty_preset.files.clear();
    assert_eq!(empty_preset.total_bytes(), 0);
}

#[test]
fn availability_reports_missing_required_choice() {
    // Synthetic preset with an unknown model: avoids needing a real catalogue model that has
    // required_choices, and the model info is None so no family choices are added.
    let unknown_model = ModelId::intern("test-choice-model");
    let p = Preset {
        model: unknown_model,
        variant: "test".to_string(),
        label: "Test".to_string(),
        files: vec![],
        access_url: None,
        required_nodes: vec![],
        required_choices: vec![("MyClass".to_string(), "my_input".to_string(), "expected_value".to_string())],
        installed: false,
    };
    let empty = ObjectInfo(json!({}));
    let a = availability(&p, &empty);
    assert!(!a.available);
    assert!(a.reason.contains("does not offer"), "{a:?}");
    assert!(a.reason.contains("MyClass"), "{a:?}");
}

#[test]
fn availability_installed_preset_reports_missing_nodes() {
    let _state = CATALOG_STATE.lock().unwrap_or_else(|e| e.into_inner());
    // `availability` looks the preset's model up in the *live* catalogue (`ModelId::try_info`), which is
    // where an installed model's family pipeline (and so its required nodes) lives. A preset from a
    // throwaway `catalog::build` is not in it, so it has no model info and nothing to check; the real
    // flow registers installed models with `set_installed` first, as done here.
    let im = InstalledModel {
        key: "test-installed-missing-nodes".into(),
        family: "sdxl".into(),
        label: "Test Installed".into(),
        files: [(Role::Checkpoint, "test.safetensors".to_owned())].into(),
        ..Default::default()
    };
    catalog::set_installed(vec![im]);
    let inst_preset = presets().iter().find(|p| p.installed && p.model.key() == "test-installed-missing-nodes").expect("installed preset");
    let empty = ObjectInfo(json!({}));
    let a = availability(inst_preset, &empty);
    catalog::set_installed(Vec::new());
    assert!(!a.available);
    assert!(a.reason.contains("Update ComfyUI"), "{a:?}");
    assert!(a.reason.contains("missing the nodes"), "{a:?}");
}

#[test]
fn model_info_tags_reflect_capabilities() {
    let qwen = ModelId::Qwen.info();
    let tags = qwen.tags();
    assert_eq!(tags.contains(&"Create".to_owned()), qwen.text_to_image);
    assert_eq!(tags.contains(&"Edit".to_owned()), qwen.edit);
    assert_eq!(tags.contains(&"Alpha".to_owned()), qwen.transparent);
    assert_eq!(tags.contains(&"Fill".to_owned()), qwen.inpaint);
    assert_eq!(tags.contains(&"Upscale".to_owned()), qwen.upscale);
    if qwen.max_references > 0 && !qwen.init_image {
        let expected_ref = format!("{} ref{}", qwen.max_references, if qwen.max_references == 1 { "" } else { "s" });
        assert!(tags.contains(&expected_ref), "expected {expected_ref} in {tags:?}");
    } else {
        assert!(!tags.iter().any(|t| t.ends_with("refs")));
    }
    let seed = ModelId::SeedVr2.info();
    let seed_tags = seed.tags();
    assert_eq!(seed_tags.contains(&"Upscale".to_owned()), seed.upscale);
    let klein_remove = ModelId::KleinRemove.info();
    let kr_tags = klein_remove.tags();
    assert_eq!(kr_tags.contains(&"Create".to_owned()), klein_remove.text_to_image);
}

#[test]
fn build_skips_installed_duplicate_of_existing_preset() {
    let cat_original = current_catalog();
    let some_preset = cat_original
        .presets
        .iter()
        .find(|p| !p.installed && p.files.iter().any(|f| f.role == Role::Unet || f.role == Role::Checkpoint))
        .expect("preset with main weights");
    let main_file = some_preset.files.iter().find(|f| f.role == Role::Unet || f.role == Role::Checkpoint).expect("main file");
    let main_name = main_file.name.clone();
    let main_role = main_file.role;
    let model_info = some_preset.model.try_info().unwrap();
    let im = InstalledModel {
        key: "ckpt:duplicate.safetensors".into(),
        family: model_info.family.clone(),
        label: "Duplicate".into(),
        files: [(main_role, main_name)].into(),
        ..Default::default()
    };
    let reg = li_ai::family::registry();
    let cat = catalog::build(reg, &[im]);
    assert!(cat.models.iter().all(|m| m.id.key() != "ckpt:duplicate.safetensors"));
}

/// An `/object_info` with every node the builders use and these loader files.
fn info_with(files: &[(&str, &str, &[&str])]) -> ObjectInfo {
    let mut info = li_ai::mock::object_info();
    for (class, input, names) in files {
        info[*class]["input"]["required"][*input] = json!([names]);
        if *class == "CLIPLoader" {
            info["DualCLIPLoader"]["input"]["required"]["clip_name1"] = json!([names]);
        }
    }
    ObjectInfo(info)
}

/// Registers installed models, runs `f` on the live catalogue, then clears them.
fn with_installed<T>(installed: Vec<InstalledModel>, f: impl FnOnce() -> T) -> T {
    let _state = CATALOG_STATE.lock().unwrap_or_else(|e| e.into_inner());
    catalog::set_installed(installed);
    let out = f();
    catalog::set_installed(Vec::new());
    out
}

fn installed(key: &str, family: &str, role: Role, file: &str, variant: Option<&str>) -> InstalledModel {
    InstalledModel {
        key: key.into(),
        family: family.into(),
        label: file.into(),
        files: [(role, file.to_owned())].into(),
        variant: variant.map(str::to_owned),
        ..Default::default()
    }
}

/// Finding 63: a checkpoint no family describes is not blocked by "missing the nodes ." (the
/// refine checker validates its own recipe).
#[test]
fn unknown_installed_checkpoints_are_available_for_the_checker() {
    let a = with_installed(vec![installed("ckpt:new-model.safetensors", "", Role::Checkpoint, "new-model.safetensors", None)], || {
        let p = presets().iter().find(|p| p.model.key() == "ckpt:new-model.safetensors").unwrap();
        availability(p, &info_with(&[("CheckpointLoaderSimple", "ckpt_name", &["new-model.safetensors"])]))
    });
    assert!(a.available, "{a:?}");
    assert!(li_ai::builders::required_nodes(&li_ai::refine::unknown_profile()).is_empty());
}

/// Finding 64: Klein 9B files outside the curated preset get the 8B text encoder; the base
/// sampling override still applies (overrides are cumulative).
#[test]
fn klein_9b_files_get_the_8b_text_encoder() {
    let info = info_with(&[
        ("UNETLoader", "unet_name", &["flux-2-klein-base-9b.safetensors", "model.safetensors", "flux-2-klein-base-4b-custom.safetensors"]),
        ("CLIPLoader", "clip_name", &["qwen_3_4b.safetensors", "qwen_3_8b_fp8mixed.safetensors"]),
        ("VAELoader", "vae_name", &["flux2-vae.safetensors"]),
    ]);
    let models = vec![
        installed("unet:flux-2-klein-base-9b.safetensors", "flux2-klein", Role::Unet, "flux-2-klein-base-9b.safetensors", Some("9b")),
        installed("unet:model.safetensors", "flux2-klein", Role::Unet, "model.safetensors", Some("9b")),
        installed("unet:flux-2-klein-base-4b-custom.safetensors", "flux2-klein", Role::Unet, "flux-2-klein-base-4b-custom.safetensors", Some("4b")),
    ];
    let got = with_installed(models, || {
        ["unet:flux-2-klein-base-9b.safetensors", "unet:model.safetensors", "unet:flux-2-klein-base-4b-custom.safetensors"].map(|key| {
            let p = presets().iter().find(|p| p.model.key() == key).unwrap();
            let a = availability(p, &info);
            let steps = p.model.info().steps.default;
            (a.files.get(&Role::Clip).cloned(), steps)
        })
    });
    assert_eq!(got[0], (Some("qwen_3_8b_fp8mixed.safetensors".into()), 20.0), "base 9B: 8B encoder and base sampling");
    assert_eq!(got[1].0.as_deref(), Some("qwen_3_8b_fp8mixed.safetensors"), "renamed 9B by its detected variant");
    assert_eq!(got[2], (Some("qwen_3_4b.safetensors".into()), 20.0), "base 4B keeps the 4B encoder");
}

/// Finding 65: an installed Qwen Image 2.1 file resolves its text encoder (any precision), and
/// is unavailable rather than run with an empty `clip_name` when none is installed.
#[test]
fn qwen_21_installed_files_need_their_text_encoder() {
    let model = || vec![installed("unet:qwen_image_2.1_turbo_bf16.safetensors", "qwen-image-21", Role::Unet, "qwen_image_2.1_turbo_bf16.safetensors", None)];
    let with_te = info_with(&[
        ("UNETLoader", "unet_name", &["qwen_image_2.1_turbo_bf16.safetensors"]),
        ("CLIPLoader", "clip_name", &["qwen3vl_8b_int8_convrot.safetensors"]),
        ("VAELoader", "vae_name", &["qwen_image_2.1_vae_bf16.safetensors"]),
    ]);
    let a =
        with_installed(model(), || availability(presets().iter().find(|p| p.model.key() == "unet:qwen_image_2.1_turbo_bf16.safetensors").unwrap(), &with_te));
    assert!(a.available, "{a:?}");
    assert_eq!(a.files[&Role::Clip], "qwen3vl_8b_int8_convrot.safetensors");
    let without = info_with(&[
        ("UNETLoader", "unet_name", &["qwen_image_2.1_turbo_bf16.safetensors"]),
        ("CLIPLoader", "clip_name", &["qwen_3_4b.safetensors"]),
        ("VAELoader", "vae_name", &["qwen_image_2.1_vae_bf16.safetensors"]),
    ]);
    let a =
        with_installed(model(), || availability(presets().iter().find(|p| p.model.key() == "unet:qwen_image_2.1_turbo_bf16.safetensors").unwrap(), &without));
    assert!(!a.available && a.reason.contains("qwen3vl_8b"), "{a:?}");
}

/// Finding 72: `ae.safetensors` is matched by name or stem, never as a substring of another VAE.
#[test]
fn ae_vae_is_never_a_substring_match() {
    let model = || vec![installed("unet:z_image_bf16.safetensors", "z-image", Role::Unet, "z_image_bf16.safetensors", None)];
    let other_vaes = info_with(&[
        ("UNETLoader", "unet_name", &["z_image_bf16.safetensors"]),
        ("CLIPLoader", "clip_name", &["qwen_3_4b.safetensors"]),
        ("VAELoader", "vae_name", &["flux2-vae.safetensors", "qwen_image_vae.safetensors"]),
    ]);
    let a = with_installed(model(), || availability(presets().iter().find(|p| p.model.key() == "unet:z_image_bf16.safetensors").unwrap(), &other_vaes));
    assert!(!a.available && !a.files.contains_key(&Role::Vae) && a.reason.contains("ae.safetensors"), "{a:?}");
    let subfolder = info_with(&[
        ("UNETLoader", "unet_name", &["z_image_bf16.safetensors"]),
        ("CLIPLoader", "clip_name", &["qwen_3_4b.safetensors"]),
        ("VAELoader", "vae_name", &["sub/AE.safetensors", "flux2-vae.safetensors"]),
    ]);
    let a = with_installed(model(), || availability(presets().iter().find(|p| p.model.key() == "unet:z_image_bf16.safetensors").unwrap(), &subfolder));
    assert_eq!(a.files.get(&Role::Vae).map(String::as_str), Some("sub/AE.safetensors"), "{a:?}");
}
