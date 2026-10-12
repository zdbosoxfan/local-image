use super::*;
use crate::browser::{Cache, Config, Net};
use crate::mock::MockComfy;
use std::sync::atomic::Ordering;

struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        let p = std::env::temp_dir().join(format!("li-refine-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(p.join("checkpoints")).unwrap();
        Self(p)
    }
    fn file(&self, name: &str) -> PathBuf {
        let p = self.0.join("checkpoints").join(name);
        let header = serde_json::to_vec(&json!({"new_architecture.weight":{"dtype":"F16","shape":[1],"data_offsets":[0,2]}})).unwrap();
        let mut body = (header.len() as u64).to_le_bytes().to_vec();
        body.extend(header);
        body.extend([0, 0]);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(&p, body).unwrap();
        p
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
struct Offline;
impl Net for Offline {
    fn get(&self, _: &str) -> Result<Vec<u8>> {
        bail!("offline")
    }
    fn head_size(&self, _: &str) -> Result<Option<u64>> {
        bail!("offline")
    }
}
fn fixture() -> Value {
    serde_json::from_str(include_str!("../../tests/fixtures/refine/unknown-checkpoint.json")).unwrap()
}
fn target_file(name: &str) -> InstalledModel {
    InstalledModel { key: format!("ckpt:{name}"), family: String::new(), label: name.into(), files: [(Role::Checkpoint, name.into())].into() }
}
fn mock_info(name: &str) -> ObjectInfo {
    let mut info = crate::mock::object_info();
    info["CheckpointLoaderSimple"]["input"]["required"]["ckpt_name"][0].as_array_mut().unwrap().push(json!(name));
    ObjectInfo(info)
}

#[test]
fn autodetect_template_match_checks_exact_file_role_size_and_hash() {
    let dir = Temp::new();
    let file = dir.file("new-model.safetensors");
    let checker = Checker::new(dir.0.join("state.json"));
    let (hash, bytes) = checker.hash_file(&file, &JobControl::new()).unwrap();
    let mut template = fixture();
    assert!(template_matches(&template, "sub\\new-model.safetensors", bytes, &hash, true));
    assert!(!template_matches(&template, "other-new-model.safetensors", bytes, &hash, true));
    assert!(!template_matches(&template, "new-model.safetensors", bytes, &hash, false));
    template["nodes"][0]["properties"]["models"][0]["size"] = json!(bytes);
    template["nodes"][0]["properties"]["models"][0]["hash"] = json!(hash.to_uppercase());
    assert!(template_matches(&template, "new-model.safetensors", bytes, &hash, true));
    assert!(!template_matches(&template, "new-model.safetensors", bytes + 1, &hash, true));
    assert!(!template_matches(&template, "new-model.safetensors", bytes, "bad-hash", true));
    template["nodes"][0]["properties"]["models"][0]["hash_type"] = json!("sha1");
    assert!(!template_matches(&template, "new-model.safetensors", bytes, &hash, true));
    template["nodes"][0]["properties"]["models"][0]["hash_type"] = json!("sha256");
    template["nodes"][0]["properties"]["models"][0]["size"] = json!("invalid");
    assert!(!template_matches(&template, "new-model.safetensors", bytes, &hash, true));
    template["nodes"][0]["properties"]["models"][0]["size"] = json!(bytes);
    template["nodes"][0]["properties"]["models"][0].as_object_mut().unwrap().remove("url");
    assert!(template_matches(&template, "new-model.safetensors", bytes, &hash, true));
    let subgraph = json!({"nodes":[],"definitions":{"subgraphs":[{"nodes":template["nodes"]}]}});
    assert!(template_matches(&subgraph, "new-model.safetensors", bytes, &hash, true));
}

#[test]
fn autodetect_universal_graph_uses_node_defaults_and_standard_metadata() {
    let info = mock_info("new-model.safetensors");
    let meta = [("modelspec.resolution".into(), "768x512".into()), ("ss_clip_skip".into(), "2".into()), ("ss_steps".into(), "500000".into())].into();
    let recipe = universal("new-model.safetensors", &info, &meta).unwrap();
    let d = recipe.defaults();
    assert_eq!((d.steps, d.cfg, d.denoise, d.grid), (20, 8.0, 0.35, 8));
    assert_eq!(d.resolution, Some((768, 512)));
    assert_eq!(d.encoder_layer, Some(2));
    let p = Params { task: Task::Refine { denoise: 0.4 }, ..Params::create("refine", 512, 288, 42, d.steps, d.cfg) };
    let (g, images) = recipe.graph(&p).unwrap();
    assert_eq!(images, ["2"]);
    assert_eq!(g["1"]["class_type"], "CheckpointLoaderSimple");
    assert_eq!(g["6"]["inputs"]["latent_image"], json!(["3", 0]));
    assert_eq!(g["3"]["inputs"]["vae"], json!(["1", 2]));
    assert_eq!(g["7"]["inputs"]["samples"], json!(["6", 0]));
    assert_eq!(g["9"]["inputs"]["stop_at_clip_layer"], -2);
    assert!((g["6"]["inputs"]["denoise"].as_f64().unwrap() - 0.4).abs() < 1e-6);
    for n in g.as_object().unwrap().values() {
        for input in n["inputs"].as_object().unwrap().values() {
            if let Some(link) = input.as_array() {
                assert!(g.get(link[0].as_str().unwrap()).is_some());
            }
        }
    }
}

#[test]
fn autodetect_template_preserves_wiring_and_marks_refine_controls() {
    let info = mock_info("sub/new-model.safetensors");
    let recipe = template_recipe("fixture-edit", &fixture(), &info, &target_file("sub/new-model.safetensors")).unwrap();
    let d = recipe.defaults();
    assert_eq!((d.steps, d.cfg, d.denoise), (12, 5.0, 0.3));
    let Recipe::Template { workflow, .. } = &recipe else { panic!() };
    for field in [FieldKind::Image(0), FieldKind::Denoise, FieldKind::Steps, FieldKind::Cfg, FieldKind::Prompt] {
        assert!(workflow.has(&field));
    }
    let (g, images) = recipe.graph(&Params { task: Task::Refine { denoise: 0.5 }, ..Params::create("test", 512, 288, 5, 15, 6.0) }).unwrap();
    assert_eq!(images, ["2"]);
    assert_eq!(g["1"]["inputs"]["ckpt_name"], "sub/new-model.safetensors");
    assert_eq!(g["6"]["inputs"]["steps"], 15);
    assert_eq!(g["6"]["inputs"]["cfg"], 6.0);
    assert_eq!(g["6"]["inputs"]["latent_image"], json!(["3", 0]));
}

#[test]
fn autodetect_text_template_becomes_img2img_with_its_decoder_vae() {
    let info = mock_info("new-model.safetensors");
    let p = Params::create("test", 512, 288, 0, 20, 8.0);
    let (mut graph, _) = universal("new-model.safetensors", &info, &BTreeMap::new()).unwrap().graph(&p).unwrap();
    graph.as_object_mut().unwrap().remove("2");
    graph["3"] = json!({"class_type":"EmptyLatentImage","inputs":{"width":512,"height":288,"batch_size":1}});
    let recipe = template_recipe("text", &graph, &info, &target_file("new-model.safetensors")).unwrap();
    let Recipe::Template { workflow, .. } = recipe else { panic!() };
    assert_eq!(workflow.graph["6"]["inputs"]["latent_image"], json!(["li_refine_encode", 0]));
    assert_eq!(workflow.graph["li_refine_encode"]["inputs"]["vae"], json!(["1", 2]));
    assert_eq!(workflow.graph["6"]["inputs"]["denoise"], 0.35);
}

#[test]
fn autodetect_cached_templates_work_offline_and_prefer_edit_variants() {
    let dir = Temp::new();
    let cfg = Config { templates: "https://fixtures/templates".into(), ..Default::default() };
    let cache = Cache::new(dir.0.join("cache"));
    let index = json!([{"type":"image","templates":[
        {"name":"text","tags":["Text to Image"]}, {"name":"edit","tags":["Image Edit"]}
    ]}]);
    cache.put("https://fixtures/templates/index.json", &serde_json::to_vec(&index).unwrap()).unwrap();
    for name in ["edit", "text"] {
        cache.put(&format!("https://fixtures/templates/{name}.json"), &serde_json::to_vec(&fixture()).unwrap()).unwrap();
    }
    let found = discover_templates(
        &cfg,
        &cache,
        &Offline,
        false,
        &target_file("new-model.safetensors"),
        10,
        "hash",
        &mock_info("new-model.safetensors"),
        &JobControl::new(),
    )
    .unwrap();
    assert_eq!(found.len(), 2);
    assert_eq!(found[0].label(), "template:edit");
    // Old bodies must refresh with the index, while retaining usable offline copies.
    for entry in std::fs::read_dir(&cache.dir).unwrap() {
        std::fs::File::options().write(true).open(entry.unwrap().path()).unwrap().set_modified(SystemTime::UNIX_EPOCH).unwrap();
    }
    struct RefreshOffline(Mutex<Vec<String>>);
    impl Net for RefreshOffline {
        fn get(&self, url: &str) -> Result<Vec<u8>> {
            self.0.lock().unwrap().push(url.into());
            bail!("temporarily offline")
        }
        fn head_size(&self, _: &str) -> Result<Option<u64>> {
            bail!("unexpected HEAD")
        }
    }
    let refresh = RefreshOffline(Mutex::new(Vec::new()));
    let found = discover_templates(
        &cfg,
        &cache,
        &refresh,
        true,
        &target_file("new-model.safetensors"),
        10,
        "hash",
        &mock_info("new-model.safetensors"),
        &JobControl::new(),
    )
    .unwrap();
    assert_eq!(found.len(), 2);
    assert_eq!(refresh.0.lock().unwrap().len(), 3, "refresh the index and both expired template bodies");
    let mut invalid = fixture();
    invalid["nodes"][0]["properties"]["models"][0]["hash"] = json!("wrong");
    cache.put("https://fixtures/templates/edit.json", &serde_json::to_vec(&invalid).unwrap()).unwrap();
    let found = discover_templates(
        &cfg,
        &cache,
        &Offline,
        false,
        &target_file("new-model.safetensors"),
        10,
        "hash",
        &mock_info("new-model.safetensors"),
        &JobControl::new(),
    )
    .unwrap();
    assert_eq!(found.len(), 1);
}

#[test]
fn autodetect_fallback_order_is_family_template_universal() {
    let info = mock_info("new-model.safetensors");
    let u = universal("new-model.safetensors", &info, &BTreeMap::new()).unwrap();
    let f = Recipe::Family { profile: Box::new(crate::family::registry().family("sdxl").unwrap().clone()), files: Files::new(), denoise: 0.35 };
    let t = template_recipe("edit", &fixture(), &info, &target_file("new-model.safetensors")).unwrap();
    let result = fallback_order(Some(f), vec![t], Some(u));
    assert_eq!(result.iter().map(Recipe::label).collect::<Vec<_>>(), ["family:sdxl", "template:edit", "universal"]);
}

#[test]
fn autodetect_verdict_roundtrip_defaults_and_retest_preserves_other_hashes() {
    let dir = Temp::new();
    let recipe = universal("new-model.safetensors", &mock_info("new-model.safetensors"), &BTreeMap::new()).unwrap();
    let verified = Verdict::Verified { recipe: Box::new(recipe), difference: 1.0 };
    let failed = Verdict::Failed { reason: "not an image model".into() };
    let state = AiState { refine_verdicts: [("hash1".into(), verified.clone()), ("hash2".into(), failed.clone())].into() };
    assert_eq!(serde_json::from_value::<AiState>(serde_json::to_value(&state).unwrap()).unwrap(), state);
    assert_eq!(serde_json::from_str::<AiState>("{}").unwrap(), AiState::default());
    std::fs::write(dir.0.join("state.json"), serde_json::to_vec(&state).unwrap()).unwrap();
    let checker = Checker::new(dir.0.join("state.json"));
    checker.store("hash3", &verified).unwrap();
    let state: AiState = serde_json::from_slice(&std::fs::read(dir.0.join("state.json")).unwrap()).unwrap();
    assert_eq!(state.refine_verdicts.len(), 3);
    assert_eq!(state.refine_verdicts["hash2"], failed);
}

#[test]
fn autodetect_hash_changes_when_file_content_changes_and_cancellation_is_not_cached() {
    let dir = Temp::new();
    let p = dir.file("new-model.safetensors");
    let checker = Checker::new(dir.0.join("state.json"));
    let first = checker.hash_file(&p, &JobControl::new()).unwrap().0;
    std::fs::write(&p, b"different file bytes").unwrap();
    assert_ne!(checker.hash_file(&p, &JobControl::new()).unwrap().0, first);
    let ctl = JobControl::new();
    ctl.cancel();
    let p = dir.file("other.safetensors");
    assert!(checker.hash_file(&p, &ctl).unwrap_err().is::<Cancelled>());
    assert!(!dir.0.join("state.json").exists());
}

#[test]
fn autodetect_metric_rejects_destroyed_composition() {
    let source = test_image();
    assert_eq!(mean_abs_difference(&source, &source), 0.0);
    assert!(mean_abs_difference(&source, &RgbImage::new(64, 36)) > PASS_LIMIT);
    let shifted = RgbImage::from_fn(512, 288, |x, y| *source.get_pixel((x + 256) % 512, y));
    assert!(mean_abs_difference(&source, &shifted) > 5.0);
}

#[test]
fn autodetect_mock_unknown_checkpoint_verifies_once_persists_and_retests() {
    let _inventory = crate::inventory::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = Temp::new();
    dir.file("new-model.safetensors");
    let server = MockComfy::start().unwrap();
    server.delay_ms.store(1, Ordering::SeqCst);
    server.set_model_dir(&dir.0);
    let client = ComfyClient::new(server.host());
    crate::inventory::refresh(&client.object_info().unwrap(), Some(&dir.0));
    let model = ModelId::from_key("ckpt:new-model.safetensors").unwrap();
    let checker = Checker::new(dir.0.join("state.json"));
    let v = checker.check_in(&client, model, "installed", &dir.0, false, &JobControl::new()).unwrap().unwrap();
    let Verdict::Verified { recipe, difference } = &v else { panic!("{v:?}") };
    assert_eq!(recipe.label(), "universal");
    assert!(*difference < PASS_LIMIT);
    assert_eq!(checker.status(&client, model), Status::Verified);
    assert_eq!(server.prompts().len(), 1);
    let restarted = Checker::new(dir.0.join("state.json"));
    assert_eq!(restarted.check_in(&client, model, "installed", &dir.0, false, &JobControl::new()).unwrap(), Some(v));
    assert_eq!(server.prompts().len(), 1, "saved hash verdict avoids another synthesis");
    std::fs::write(dir.0.join("checkpoints/new-model.safetensors"), b"replacement weights").unwrap();
    restarted.invalidate_changed_files();
    assert_eq!(restarted.status(&client, model), Status::Unchecked, "cached verdicts also track file replacement after restart");
    restarted.check_in(&client, model, "installed", &dir.0, true, &JobControl::new()).unwrap();
    assert_eq!(server.prompts().len(), 2);
    crate::catalog::set_installed(Vec::new());
}

#[test]
fn autodetect_mock_family_failure_falls_back_to_universal() {
    let _inventory = crate::inventory::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = Temp::new();
    dir.file("juggernautXL_v9.safetensors");
    let server = MockComfy::start().unwrap();
    server.delay_ms.store(1, Ordering::SeqCst);
    server.set_model_dir(&dir.0);
    let client = ComfyClient::new(server.host());
    crate::inventory::refresh(&client.object_info().unwrap(), Some(&dir.0));
    let model = ModelId::from_key("ckpt:juggernautXL_v9.safetensors").unwrap();
    server.fail_next();
    let checker = Checker::new(dir.0.join("state.json"));
    let v = checker.check_in(&client, model, "installed", &dir.0, false, &JobControl::new()).unwrap().unwrap();
    let Verdict::Verified { recipe, .. } = v else { panic!("{v:?}") };
    assert_eq!(recipe.label(), "universal");
    assert_eq!(server.prompts().len(), 2);
    crate::catalog::set_installed(Vec::new());
}

#[test]
fn autodetect_mock_all_recipes_fail_is_unsupported_and_cancellation_is_transient() {
    let _inventory = crate::inventory::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = Temp::new();
    dir.file("new-model.safetensors");
    let server = MockComfy::start().unwrap();
    server.delay_ms.store(1, Ordering::SeqCst);
    server.set_model_dir(&dir.0);
    let client = ComfyClient::new(server.host());
    crate::inventory::refresh(&client.object_info().unwrap(), Some(&dir.0));
    let model = ModelId::from_key("ckpt:new-model.safetensors").unwrap();
    let checker = Checker::new(dir.0.join("state.json"));
    let ctl = JobControl::new();
    ctl.cancel();
    assert!(checker.check_in(&client, model, "installed", &dir.0, false, &ctl).unwrap_err().is::<Cancelled>());
    assert!(!dir.0.join("state.json").exists());
    server.fail_next();
    let v = checker.check_in(&client, model, "installed", &dir.0, false, &JobControl::new()).unwrap().unwrap();
    let Verdict::Failed { reason } = &v else { panic!("{v:?}") };
    assert!(reason.contains("universal:"));
    assert!(matches!(checker.status(&client, model), Status::Failed(_)));
    assert_eq!(checker.check_in(&client, model, "installed", &dir.0, false, &JobControl::new()).unwrap(), Some(v));
    assert_eq!(server.prompts().len(), 1);
    crate::catalog::set_installed(Vec::new());
}

#[test]
fn autodetect_custom_sampler_template_prunes_other_models_and_splits_sigmas() {
    let info = mock_info("new-model.safetensors");
    let p = Params::create("test", 512, 288, 0, 4, 5.0);
    let (mut graph, _) = universal("new-model.safetensors", &info, &BTreeMap::new()).unwrap().graph(&p).unwrap();
    graph.as_object_mut().unwrap().remove("2");
    graph["3"] = json!({"class_type":"EmptyFlux2LatentImage","inputs":{"width":1024,"height":1024,"batch_size":1}});
    graph["6"] = json!({"class_type":"SamplerCustomAdvanced","inputs":{"noise":["noise",0],"guider":["guider",0],"sampler":["sampler",0],"sigmas":["schedule",0],"latent_image":["3",0]}});
    graph["noise"] = json!({"class_type":"RandomNoise","inputs":{"noise_seed":42}});
    graph["guider"] = json!({"class_type":"CFGGuider","inputs":{"model":["1",0],"positive":["4",0],"negative":["5",0],"cfg":5.0}});
    graph["sampler"] = json!({"class_type":"KSamplerSelect","inputs":{"sampler_name":"euler"}});
    graph["schedule"] = json!({"class_type":"Flux2Scheduler","inputs":{"steps":4,"width":1024,"height":1024}});
    // A second comparison branch requiring an absent checkpoint must never be loaded.
    graph["other_model"] = json!({"class_type":"CheckpointLoaderSimple","inputs":{"ckpt_name":"absent.safetensors"}});
    graph["other_decode"] = json!({"class_type":"VAEDecode","inputs":{"samples":["3",0],"vae":["other_model",2]}});
    graph["other_output"] = json!({"class_type":"SaveImage","inputs":{"images":["other_decode",0],"filename_prefix":"Other"}});
    let recipe = template_recipe("custom", &graph, &info, &target_file("new-model.safetensors")).unwrap();
    assert_eq!(recipe.defaults().grid, 16);
    assert_eq!(recipe.defaults().denoise, 0.35);
    let (g, slots) = recipe.graph(&Params { task: Task::Refine { denoise: 0.4 }, ..p }).unwrap();
    assert_eq!(slots, ["li_refine_image"]);
    assert!(g.get("other_model").is_none());
    assert!(g.get("other_output").is_none());
    assert!(g.get("3").is_none());
    assert_eq!(g["6"]["inputs"]["sigmas"], json!(["li_refine_sigmas_6", 1]));
    assert!((g["li_refine_sigmas_6"]["inputs"]["denoise"].as_f64().unwrap() - 0.4).abs() < 1e-6);
    assert_eq!(g["schedule"]["inputs"]["width"], 512);
    assert_eq!(g["schedule"]["inputs"]["height"], 288);
}

#[test]
fn autodetect_mock_busy_queue_defers_without_a_verdict_or_second_prompt() {
    let _inventory = crate::inventory::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = Temp::new();
    dir.file("new-model.safetensors");
    let server = MockComfy::start().unwrap();
    server.set_model_dir(&dir.0);
    server.delay_ms.store(600, Ordering::SeqCst);
    let client = ComfyClient::new(server.host());
    crate::inventory::refresh(&client.object_info().unwrap(), Some(&dir.0));
    let model = ModelId::from_key("ckpt:new-model.safetensors").unwrap();
    let checker = Checker::new(dir.0.join("state.json"));
    let graph = universal("new-model.safetensors", &client.object_info().unwrap(), &BTreeMap::new())
        .unwrap()
        .graph(&Params::create("external", 512, 288, 0, 20, 8.0))
        .unwrap()
        .0;
    client.queue_prompt(&graph, "external-owner-job").unwrap();
    assert!(!queue_idle(&client).unwrap());
    assert!(checker.check_in(&client, model, "installed", &dir.0, false, &JobControl::new()).unwrap().is_none());
    assert_eq!(server.prompts().len(), 1);
    assert!(!dir.0.join("state.json").exists());
    assert_eq!(checker.status(&client, model), Status::Unchecked);
    let _job = TEST_JOB.lock().unwrap_or_else(|e| e.into_inner());
    assert!(checker.check_in(&client, model, "installed", &dir.0, false, &JobControl::new()).unwrap().is_none(), "another checker owns the single test slot");
    crate::catalog::set_installed(Vec::new());
}

#[test]
fn autodetect_mock_unknown_split_weights_require_a_template_without_universal_guessing() {
    let _inventory = crate::inventory::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = Temp::new();
    std::fs::create_dir_all(dir.0.join("diffusion_models")).unwrap();
    std::fs::write(dir.0.join("diffusion_models/new-split.safetensors"), b"new split weights").unwrap();
    let server = MockComfy::start().unwrap();
    server.set_model_dir(&dir.0);
    let client = ComfyClient::new(server.host());
    crate::inventory::refresh(&client.object_info().unwrap(), Some(&dir.0));
    let model = ModelId::from_key("unet:new-split.safetensors").unwrap();
    let checker = Checker::new(dir.0.join("state.json"));
    let v = checker.check_in(&client, model, "installed", &dir.0, false, &JobControl::new()).unwrap().unwrap();
    let Verdict::Failed { reason } = v else { panic!() };
    assert!(reason.contains("Split diffusion weights need"));
    assert_eq!(server.prompts().len(), 0);
    assert!(matches!(checker.status(&client, model), Status::Failed(_)));
    crate::catalog::set_installed(Vec::new());
}

#[test]
fn autodetect_bypassed_template_model_and_its_output_are_activated() {
    let mut ui = fixture();
    ui["nodes"][0]["mode"] = json!(4);
    ui["nodes"][7]["mode"] = json!(4);
    let recipe = template_recipe("bypassed", &ui, &mock_info("new-model.safetensors"), &target_file("new-model.safetensors")).unwrap();
    let Recipe::Template { workflow, .. } = recipe else { panic!() };
    assert_eq!(workflow.graph["1"]["class_type"], "CheckpointLoaderSimple");
    assert_eq!(workflow.graph["8"]["class_type"], "SaveImage");
}

#[test]
fn autodetect_cached_template_rebinds_only_its_main_model_after_rename() {
    let _inventory = crate::inventory::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = Temp::new();
    let file = dir.file("new-model.safetensors");
    let server = MockComfy::start().unwrap();
    server.set_model_dir(&dir.0);
    let client = ComfyClient::new(server.host());
    let info = client.object_info().unwrap();
    crate::inventory::refresh(&info, Some(&dir.0));
    let model = ModelId::from_key("ckpt:new-model.safetensors").unwrap();
    let p = Params::create("test", 512, 288, 0, 20, 8.0);
    let (mut graph, _) = universal("new-model.safetensors", &info, &BTreeMap::new()).unwrap().graph(&p).unwrap();
    graph["1"]["_meta"] = json!("malformed optional metadata");
    graph["9"] = json!({"class_type":"CheckpointLoaderSimple","inputs":{"ckpt_name":"juggernautXL_v9.safetensors"}});
    graph["7"]["inputs"]["vae"] = json!(["9", 2]);
    let recipe = template_recipe("two-loaders", &graph, &info, &target_file("new-model.safetensors")).unwrap();
    let checker = Checker::new(dir.0.join("state.json"));
    let (hash, _) = checker.hash_file(&file, &JobControl::new()).unwrap();
    checker.store(&hash, &Verdict::Verified { recipe: Box::new(recipe), difference: 0.0 }).unwrap();
    let cached = checker.check_in(&client, model, "installed", &dir.0, false, &JobControl::new()).unwrap().unwrap();
    let Verdict::Verified { recipe, .. } = cached else { panic!() };
    let (graph, _) = recipe.graph(&p).unwrap();
    assert_eq!(graph["9"]["inputs"]["ckpt_name"], "juggernautXL_v9.safetensors", "auxiliary loader survives cache reuse");
    std::fs::rename(file, dir.0.join("checkpoints/renamed.safetensors")).unwrap();
    crate::inventory::refresh(&client.object_info().unwrap(), Some(&dir.0));
    let renamed = ModelId::from_key("ckpt:renamed.safetensors").unwrap();
    let restarted = Checker::new(dir.0.join("state.json"));
    let cached = restarted.check_in(&client, renamed, "installed", &dir.0, false, &JobControl::new()).unwrap().unwrap();
    let Verdict::Verified { recipe, .. } = cached else { panic!() };
    let (graph, _) = recipe.graph(&p).unwrap();
    assert_eq!(graph["1"]["inputs"]["ckpt_name"], "renamed.safetensors");
    assert_eq!(graph["9"]["inputs"]["ckpt_name"], "juggernautXL_v9.safetensors");
    assert!(server.prompts().is_empty(), "unchanged file hash needs no new synthesis");
    crate::catalog::set_installed(Vec::new());
}

#[test]
fn final_review_checker_rejects_model_swap_after_submission() {
    let _inventory = crate::inventory::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = Temp::new();
    let file = dir.file("review-swap.safetensors");
    let server = MockComfy::start().unwrap();
    server.delay_ms.store(250, Ordering::SeqCst);
    server.set_model_dir(&dir.0);
    let client = ComfyClient::new(server.host());
    crate::inventory::refresh(&client.object_info().unwrap(), Some(&dir.0));
    let model = ModelId::from_key("ckpt:review-swap.safetensors").unwrap();
    let checker = Checker::new(dir.0.join("state.json"));
    let (original_hash, _) = checker.hash_file(&file, &JobControl::new()).unwrap();
    std::thread::scope(|scope| {
        let checked = scope.spawn(|| checker.check_in(&client, model, "installed", &dir.0, false, &JobControl::new()));
        let until = Instant::now() + Duration::from_secs(5);
        while server.prompts().is_empty() && Instant::now() < until {
            std::thread::sleep(Duration::from_millis(2));
        }
        assert_eq!(server.prompts().len(), 1);
        let mut bytes = std::fs::read(&file).unwrap();
        bytes.push(42);
        std::fs::write(&file, bytes).unwrap();
        assert!(checked.join().unwrap().is_err(), "changed weights must not publish a verdict");
    });
    assert!(checker.saved(&original_hash).is_none());
    assert_eq!(checker.status(&client, model), Status::Unchecked);
    assert!(model_defaults(model).is_none());
}

#[test]
fn final_review_recipes_never_silently_drop_styles() {
    let info = mock_info("new-model.safetensors");
    let template = template_recipe("edit", &fixture(), &info, &target_file("new-model.safetensors")).unwrap();
    let universal = universal("new-model.safetensors", &info, &BTreeMap::new()).unwrap();
    for recipe in [template, universal] {
        for name in ["flux-2-klein-style.safetensors", "sdxl-style.safetensors", "unidentified.safetensors"] {
            let loras = [LoraUse { name: name.into(), strength: 0.7 }];
            let p = Params { task: Task::Refine { denoise: 0.35 }, loras: &loras, ..Params::create("refine", 512, 288, 4, 20, 8.0) };
            assert!(recipe.graph(&p).is_err(), "{} must reject unsupported styles before submission", recipe.label());
        }
    }
}

#[test]
fn final_review_refine_keeps_explicit_values_equal_to_nominal_defaults() {
    let _inventory = crate::inventory::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = Temp::new();
    // Deliberately no local weights: cached recipes for remote servers are session-only.
    let server = MockComfy::start().unwrap();
    server.delay_ms.store(1, Ordering::SeqCst);
    let name = "review-effective-remote.safetensors";
    dir.file(name);
    server.set_model_dir(&dir.0);
    let info = mock_info(name);
    crate::catalog::set_installed(vec![target_file(name)]);
    let model = ModelId::from_key(&format!("ckpt:{name}")).unwrap();
    let mut recipe = universal(name, &info, &BTreeMap::new()).unwrap();
    if let Recipe::Universal { defaults, .. } = &mut recipe {
        defaults.steps = 12;
        defaults.cfg = 5.0;
        defaults.denoise = 0.25;
    }
    let client = ComfyClient::new(server.host());
    checker()
        .session_verdicts
        .lock()
        .unwrap()
        .insert(format!("{}:{name}", Checker::key(&client, model, "installed")), Verdict::Verified { recipe: Box::new(recipe), difference: 0.0 });
    let mut req = crate::GenerateRequest::new(model, "refine");
    req.mode = crate::GenerateMode::Refine;
    req.source = Some(image::DynamicImage::ImageRgb8(test_image()).to_rgba8());
    req.denoise = 0.35;
    let requested = (req.steps, req.guidance, req.denoise);
    crate::Ai::new(server.host()).generate(&req, &JobControl::new()).unwrap();
    let graphs = server.prompts();
    let sampler = graphs.last().unwrap().as_object().unwrap().values().find(|n| n["class_type"] == "KSampler").unwrap();
    assert_eq!(sampler["inputs"]["steps"], requested.0, "explicit steps must not be guessed from numeric equality");
    assert_eq!(sampler["inputs"]["cfg"], requested.1);
    assert!((sampler["inputs"]["denoise"].as_f64().unwrap() - f64::from(requested.2)).abs() < 1e-6);
    drop(dir);
}

#[test]
fn final_review_checker_rejects_model_swap_before_cached_publication() {
    let _inventory = crate::inventory::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = Temp::new();
    let file = dir.file("review-before-swap.safetensors");
    let server = MockComfy::start().unwrap();
    server.set_model_dir(&dir.0);
    let client = ComfyClient::new(server.host());
    crate::inventory::refresh(&client.object_info().unwrap(), Some(&dir.0));
    let model = ModelId::from_key("ckpt:review-before-swap.safetensors").unwrap();
    let checker = Checker::new(dir.0.join("state.json"));
    let (hash, _) = checker.hash_file(&file, &JobControl::new()).unwrap();
    let recipe = universal("review-before-swap.safetensors", &client.object_info().unwrap(), &BTreeMap::new()).unwrap();
    checker.store(&hash, &Verdict::Verified { recipe: Box::new(recipe), difference: 0.0 }).unwrap();
    let publication = checker.state.lock().unwrap();
    std::thread::scope(|scope| {
        let checked = scope.spawn(|| checker.check_in(&client, model, "installed", &dir.0, false, &JobControl::new()));
        let until = Instant::now() + Duration::from_secs(5);
        while checker.runtime.lock().unwrap().values().all(|e| e.signature.is_none()) && Instant::now() < until {
            std::thread::sleep(Duration::from_millis(2));
        }
        assert!(checker.runtime.lock().unwrap().values().any(|e| e.signature.is_some()));
        let mut bytes = std::fs::read(&file).unwrap();
        bytes.push(43);
        std::fs::write(&file, bytes).unwrap();
        drop(publication);
        assert!(checked.join().unwrap().is_err(), "changed weights must invalidate cached publication");
    });
    assert!(server.prompts().is_empty());
    assert_eq!(checker.status(&client, model), Status::Unchecked);
    assert!(model_defaults(model).is_none());
}

#[test]
fn final_review_checker_rejects_inventory_change_after_submission() {
    let _inventory = crate::inventory::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = Temp::new();
    dir.file("review-inventory.safetensors");
    let server = MockComfy::start().unwrap();
    server.delay_ms.store(250, Ordering::SeqCst);
    server.set_model_dir(&dir.0);
    let client = ComfyClient::new(server.host());
    crate::inventory::refresh(&client.object_info().unwrap(), Some(&dir.0));
    let model = ModelId::from_key("ckpt:review-inventory.safetensors").unwrap();
    let checker = Checker::new(dir.0.join("state.json"));
    std::thread::scope(|scope| {
        let checked = scope.spawn(|| checker.check_in(&client, model, "installed", &dir.0, false, &JobControl::new()));
        let until = Instant::now() + Duration::from_secs(5);
        while server.prompts().is_empty() && Instant::now() < until {
            std::thread::sleep(Duration::from_millis(2));
        }
        assert_eq!(server.prompts().len(), 1);
        crate::catalog::set_installed(vec![]);
        assert!(checked.join().unwrap().is_err(), "changed inventory must not publish a verdict");
    });
    assert!(!dir.0.join("state.json").exists());
    assert_eq!(checker.status(&client, model), Status::Unchecked);
}

#[test]
fn final_review_family_styles_are_preserved_or_rejected_explicitly() {
    let profile = crate::family::registry().family("sdxl").unwrap().clone();
    let recipe = Recipe::Family { profile: Box::new(profile), files: [(Role::Checkpoint, "sdxl.safetensors".into())].into(), denoise: 0.35 };
    for (name, compatible) in [("sdxl_style.safetensors", true), ("unidentified.safetensors", true), ("flux1_style.safetensors", false)] {
        let loras = [LoraUse { name: name.into(), strength: 0.7 }];
        let p = Params { task: Task::Refine { denoise: 0.35 }, loras: &loras, ..Params::create("refine", 512, 288, 42, 20, 6.0) };
        let graph = recipe.graph(&p);
        if compatible {
            let graph = graph.unwrap().0;
            let lora = graph.as_object().unwrap().values().find(|n| n["class_type"] == "LoraLoader").unwrap();
            assert_eq!(lora["inputs"]["lora_name"], name);
            assert!((lora["inputs"]["strength_model"].as_f64().unwrap() - 0.7).abs() < 1e-6);
        } else {
            assert!(graph.is_err(), "incompatible styles must be rejected, not filtered");
        }
    }
}

#[test]
fn final_review_service_rejects_unsupported_styles_for_both_refine_routes() {
    let _inventory = crate::inventory::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = Temp::new();
    let name = "review-style-remote.safetensors";
    dir.file(name);
    let server = MockComfy::start().unwrap();
    server.delay_ms.store(1, Ordering::SeqCst);
    server.set_model_dir(&dir.0);
    let client = ComfyClient::new(server.host());
    crate::catalog::set_installed(vec![target_file(name)]);
    let model = ModelId::from_key(&format!("ckpt:{name}")).unwrap();
    let info = client.object_info().unwrap();
    let checkpoint = universal(name, &info, &BTreeMap::new()).unwrap();
    let (graph, _) = checkpoint.graph(&Params::create("refine", 512, 288, 42, 20, 8.0)).unwrap();
    let template = template_recipe("edit", &graph, &info, &target_file(name)).unwrap();
    for recipe in [checkpoint, template] {
        checker()
            .session_verdicts
            .lock()
            .unwrap()
            .insert(format!("{}:{name}", Checker::key(&client, model, "installed")), Verdict::Verified { recipe: Box::new(recipe), difference: 0.0 });
        for style in ["flux-2-klein-style.safetensors", "sdxl-style.safetensors", "unidentified.safetensors"] {
            let loras = vec![LoraUse { name: style.into(), strength: 0.7 }];
            let mut req = crate::GenerateRequest::new(model, "refine");
            req.mode = crate::GenerateMode::Refine;
            req.source = Some(image::DynamicImage::ImageRgb8(test_image()).to_rgba8());
            req.loras = loras.clone();
            assert!(crate::Ai::new(server.host()).generate(&req, &JobControl::new()).is_err());
            let mut draft = crate::GenerateRequest::new(ModelId::Klein4B, "draft");
            draft.refine = Some(crate::RefineStep {
                model,
                variant: "installed".into(),
                strength: 0.35,
                default_strength: false,
                steps: None,
                guidance: None,
                loras,
                scale: 1.0,
            });
            assert!(crate::Ai::new(server.host()).generate(&draft, &JobControl::new()).is_err());
        }
    }
    assert!(server.prompts().is_empty(), "unsupported styles must be rejected before either job is submitted");
}

#[cfg(unix)]
#[test]
fn final_review_hash_detects_same_size_rewrite_with_restored_mtime() {
    let dir = Temp::new();
    let file = dir.file("review-rewrite.safetensors");
    let checker = Checker::new(dir.0.join("state.json"));
    let old_time = std::fs::metadata(&file).unwrap().modified().unwrap();
    let original = checker.hash_file(&file, &JobControl::new()).unwrap().0;
    std::thread::sleep(Duration::from_millis(2));
    let mut bytes = std::fs::read(&file).unwrap();
    *bytes.last_mut().unwrap() = 1;
    std::fs::write(&file, bytes).unwrap();
    std::fs::File::open(&file).unwrap().set_modified(old_time).unwrap();
    assert_ne!(checker.hash_file(&file, &JobControl::new()).unwrap().0, original, "filesystem change identity must invalidate the hash cache");
}

#[test]
fn final_review_effective_metadata_resolves_saved_defaults_and_honours_explicit_values() {
    let _inventory = crate::inventory::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = Temp::new();
    let name = "review-metadata-remote.safetensors";
    dir.file(name);
    let server = MockComfy::start().unwrap();
    server.delay_ms.store(1, Ordering::SeqCst);
    server.set_model_dir(&dir.0);
    let client = ComfyClient::new(server.host());
    crate::catalog::set_installed(vec![target_file(name)]);
    let model = ModelId::from_key(&format!("ckpt:{name}")).unwrap();
    let info = client.object_info().unwrap();
    let checkpoint = universal(name, &info, &BTreeMap::new()).unwrap();
    let (graph, _) = checkpoint.graph(&Params::create("refine", 512, 288, 42, 20, 8.0)).unwrap();
    let template = template_recipe("edit", &graph, &info, &target_file(name)).unwrap();
    for mut recipe in [checkpoint, template] {
        match &mut recipe {
            Recipe::Universal { defaults, .. } | Recipe::Template { defaults, .. } => {
                defaults.steps = 12;
                defaults.cfg = 5.0;
                defaults.denoise = 0.25;
            }
            _ => unreachable!(),
        }
        checker()
            .session_verdicts
            .lock()
            .unwrap()
            .insert(format!("{}:{name}", Checker::key(&client, model, "installed")), Verdict::Verified { recipe: Box::new(recipe), difference: 0.0 });
        let mut req = crate::GenerateRequest::new(model, "refine");
        req.mode = crate::GenerateMode::Refine;
        req.source = Some(image::DynamicImage::ImageRgb8(test_image()).to_rgba8());
        for (flags, values) in [([true; 3], (12, 5.0, 0.25)), ([false; 3], (20, 8.0, 0.35)), ([false; 3], (14, 4.0, 0.7))] {
            req.refine_defaults = flags;
            if flags == [false; 3] {
                (req.steps, req.guidance, req.denoise) = values;
            }
            let out = crate::Ai::new(server.host()).generate_detailed(&req, &JobControl::new()).unwrap();
            assert_eq!(out.parameters["steps"], values.0);
            assert!((out.parameters["guidance"].as_f64().unwrap() - f64::from(values.1)).abs() < 1e-6);
            assert!((out.parameters["denoise"].as_f64().unwrap() - f64::from(values.2)).abs() < 1e-6);
            let graph = server.prompts().last().unwrap().clone();
            let sampler = graph.as_object().unwrap().values().find(|n| n["class_type"] == "KSampler").unwrap();
            for (param, input) in [("steps", "steps"), ("guidance", "cfg"), ("denoise", "denoise")] {
                assert_eq!(out.parameters[param], sampler["inputs"][input]);
            }
            assert_eq!(out.image.dimensions(), (512, 288));
        }
    }
}

#[test]
fn final_review_second_pass_metadata_uses_resolved_strength_and_explicit_options() {
    let _inventory = crate::inventory::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = Temp::new();
    let name = "review-second-pass-remote.safetensors";
    dir.file(name);
    let server = MockComfy::start().unwrap();
    server.delay_ms.store(1, Ordering::SeqCst);
    server.set_model_dir(&dir.0);
    let client = ComfyClient::new(server.host());
    crate::catalog::set_installed(vec![target_file(name)]);
    let model = ModelId::from_key(&format!("ckpt:{name}")).unwrap();
    let mut recipe = universal(name, &client.object_info().unwrap(), &BTreeMap::new()).unwrap();
    if let Recipe::Universal { defaults, .. } = &mut recipe {
        defaults.steps = 12;
        defaults.cfg = 5.0;
        defaults.denoise = 0.25;
    }
    checker()
        .session_verdicts
        .lock()
        .unwrap()
        .insert(format!("{}:{name}", Checker::key(&client, model, "installed")), Verdict::Verified { recipe: Box::new(recipe), difference: 0.0 });
    for (automatic, values) in [(true, (12, 5.0, 0.25)), (false, (20, 8.0, 0.35))] {
        let mut req = crate::GenerateRequest::new(ModelId::Klein4B, "a house");
        (req.width, req.height) = (512, 288);
        req.refine = Some(crate::RefineStep {
            model,
            variant: "installed".into(),
            strength: 0.35,
            default_strength: automatic,
            steps: (!automatic).then_some(values.0),
            guidance: (!automatic).then_some(values.1),
            loras: vec![],
            scale: 1.0,
        });
        let generated = crate::Ai::new(server.host()).generate_detailed(&req, &JobControl::new()).unwrap();
        let effective = &generated.parameters["refine"];
        assert_eq!(effective["steps"], values.0);
        assert_eq!(effective["guidance"], values.1);
        assert!((effective["strength"].as_f64().unwrap() - values.2).abs() < 1e-6);
        let graph = server.prompts().last().unwrap().clone();
        let sampler = graph.as_object().unwrap().values().find(|n| n["class_type"] == "KSampler").unwrap();
        assert_eq!(effective["steps"], sampler["inputs"]["steps"]);
        assert_eq!(effective["guidance"], sampler["inputs"]["cfg"]);
        assert_eq!(effective["strength"], sampler["inputs"]["denoise"]);
    }
}

#[test]
fn final_review_checker_accepts_unchanged_inventory_refresh_after_submission() {
    let _inventory = crate::inventory::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = Temp::new();
    dir.file("review-initial-refresh.safetensors");
    let server = MockComfy::start().unwrap();
    server.delay_ms.store(250, Ordering::SeqCst);
    server.set_model_dir(&dir.0);
    let client = ComfyClient::new(server.host());
    crate::catalog::set_installed(vec![target_file("review-initial-refresh.safetensors")]);
    let model = ModelId::from_key("ckpt:review-initial-refresh.safetensors").unwrap();
    let checker = Checker::new(dir.0.join("state.json"));
    std::thread::scope(|scope| {
        let checked = scope.spawn(|| checker.check_in(&client, model, "installed", &dir.0, false, &JobControl::new()));
        let until = Instant::now() + Duration::from_secs(5);
        while server.prompts().is_empty() && Instant::now() < until {
            std::thread::sleep(Duration::from_millis(2));
        }
        assert_eq!(server.prompts().len(), 1);
        crate::inventory::refresh(&client.object_info().unwrap(), Some(&dir.0));
        assert!(matches!(checked.join().unwrap().unwrap(), Some(Verdict::Verified { .. })), "unchanged loader files must allow publication");
    });
    assert_eq!(checker.status(&client, model), Status::Verified);
    assert!(dir.0.join("state.json").is_file());
}

#[test]
fn final_review_inventory_identity_ignores_uploaded_images_and_retains_weights() {
    let mut before = mock_info("review-signature.safetensors");
    before.0["LoadImage"]["input"]["required"]["image"] = json!([["existing.png"], {"image_upload":true}]);
    let mut uploaded = before.clone();
    uploaded.0["LoadImage"]["input"]["required"]["image"][0].as_array_mut().unwrap().push(json!("checker-source.png"));
    assert_eq!(inventory_signature(&before), inventory_signature(&uploaded), "the check's own upload must not invalidate its verdict");
    uploaded.0["CheckpointLoaderSimple"]["input"]["required"]["ckpt_name"][0].as_array_mut().unwrap().push(json!("changed-model.safetensors"));
    assert_ne!(inventory_signature(&before), inventory_signature(&uploaded), "loader-file changes must still invalidate the verdict");
    let mut schema = before.clone();
    schema.0.as_object_mut().unwrap().remove("VAEDecode");
    assert_ne!(inventory_signature(&before), inventory_signature(&schema), "required-node changes must still invalidate the verdict");
}
