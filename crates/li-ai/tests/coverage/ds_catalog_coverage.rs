use li_ai::catalog;
use li_ai::catalog::catalog as current_catalog;
use li_ai::catalog::{InstalledModel, ModelId, Origin, Preset, Role, VRAM_GUIDE, availability, default_variant, preset, presets, presets_for};
use li_ai::comfy::ObjectInfo;
use serde_json::json;

#[test]
fn original_models_have_presets_and_valid_hashes() {
    let cat = current_catalog();
    for m in ModelId::ORIGINAL {
        assert!(cat.presets.iter().any(|p| p.model == m), "{m:?}");
        assert_eq!(ModelId::from_key(m.key()), Some(m));
    }
    for p in cat.presets.iter().filter(|p| !p.installed) {
        for f in &p.files {
            assert_eq!(f.sha256.len(), 64);
            assert!(f.sha256.bytes().all(|b| b.is_ascii_hexdigit()));
            assert!(f.url.starts_with("https://"));
            assert!(f.bytes > 0);
        }
    }
}

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
fn availability_reports_missing_nodes_then_files() {
    let p = preset(ModelId::Ernie, "bf16").expect("Ernie bf16 preset exists");
    let empty = ObjectInfo(json!({}));
    let a = availability(p, &empty);
    assert!(!a.available);
    assert!(a.reason.contains("Update ComfyUI"));
    let model = p.model.try_info().expect("Ernie info");
    let mut info = json!({});
    for n in li_ai::builders::required_nodes(&model.resolved) {
        info[n] = json!({"input": {"required": {}}});
    }
    info["CLIPLoader"] = json!({"input": {"required": {"type": [["flux2"]], "clip_name": [["sub/Ministral-3-3B.safetensors"]]}}});
    info["UNETLoader"] = json!({"input": {"required": {"unet_name": [["ernie-image.safetensors"]]}}});
    let a = availability(p, &ObjectInfo(info.clone()));
    assert!(!a.available);
    assert!(a.reason.contains("flux2-vae"), "{a:?}");
    info["VAELoader"] = json!({"input": {"required": {"vae_name": [["flux2-vae.safetensors"]]}}});
    let a = availability(p, &ObjectInfo(info));
    assert!(a.available, "{a:?}");
    assert_eq!(a.files[&Role::Clip], "sub/Ministral-3-3B.safetensors");
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
fn vram_guide_is_sorted_ascending_and_non_empty() {
    assert!(!VRAM_GUIDE.is_empty());
    for w in VRAM_GUIDE.windows(2) {
        assert!(w[0].1 < w[1].1, "vram should increase");
        assert!(!w[0].0.is_empty() && !w[1].0.is_empty());
    }
    assert!(VRAM_GUIDE.iter().all(|&(_, gb)| gb > 0));
}

#[test]
fn origin_eq_and_debug() {
    assert_eq!(Origin::Profile, Origin::Profile);
    assert_ne!(Origin::Profile, Origin::Installed);
    let s = format!("{:?}", Origin::Profile);
    assert!(s.contains("Profile"));
    let s2 = format!("{:?}", Origin::Installed);
    assert!(s2.contains("Installed"));
    let cloned = Origin::Profile.clone();
    assert_eq!(cloned, Origin::Profile);
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
fn build_with_installed_model_creates_installed_preset() {
    let reg = li_ai::family::registry();
    let im = InstalledModel {
        key: "ckpt:test_model.safetensors".into(),
        family: "sdxl".into(),
        label: "Test Model".into(),
        files: [(Role::Checkpoint, "test_model.safetensors".to_owned())].into(),
    };
    let cat = catalog::build(reg, &[im]);
    let model = cat.models.iter().find(|m| m.id.key() == "ckpt:test_model.safetensors").expect("installed model in catalog");
    assert_eq!(model.origin, Origin::Installed);
    assert_eq!(model.family, "sdxl");
    let p = cat.presets.iter().find(|p| p.model == model.id).expect("preset for installed model");
    assert!(p.installed);
    assert_eq!(p.variant, "installed");
    assert_eq!(p.files.len(), 1);
    assert_eq!(p.files[0].name, "test_model.safetensors");
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
    };
    let reg = li_ai::family::registry();
    let cat = catalog::build(reg, &[im]);
    assert!(cat.models.iter().all(|m| m.id.key() != "ckpt:duplicate.safetensors"));
}

#[test]
fn build_with_registry_never_empty() {
    let reg = li_ai::family::registry();
    let cat = catalog::build(reg, &[]);
    assert!(!cat.models.is_empty());
    assert!(!cat.presets.is_empty());
}
