use super::*;
use crate::family::registry;
use serde_json::json;

#[test]
fn lora_files_are_named_after_their_repo() {
    assert_eq!(lora_file_name("me/cool-style", "adapter_model.safetensors"), "cool-style-adapter_model.safetensors");
    assert_eq!(lora_file_name("me/cool-style", "v2/pytorch_lora_weights.safetensors"), "cool-style-v2-pytorch_lora_weights.safetensors");
    assert_eq!(lora_file_name("me/Cool-Style", "cool-style_v1.safetensors"), "cool-style_v1.safetensors");
    assert_ne!(lora_file_name("a/x", "adapter_model.safetensors"), lora_file_name("b/y", "adapter_model.safetensors"));
}

#[test]
fn gated_and_text_loras() {
    let tree = json!([{"path": "a.safetensors", "size": 10, "lfs": {"oid": "****************************************************************", "size": 10}}]);
    assert!(parse_hf_tree(&tree).is_empty(), "masked hashes are not hashes");
    let list = json!([
        {"id": "x/text-lora", "tags": ["lora"], "pipeline_tag": "text-generation", "siblings": [{"rfilename": "adapter_model.safetensors"}]},
        {"id": "x/img-lora", "tags": ["lora"], "pipeline_tag": "text-to-image", "siblings": [{"rfilename": "adapter_model.safetensors"}]}
    ]);
    let items = parse_hf_list(registry(), "https://huggingface.co", &list, Kind::Lora);
    assert_eq!(items.iter().map(|i| i.title.as_str()).collect::<Vec<_>>(), ["img-lora"]);
    assert_eq!(items[0].files[0].name, "img-lora-adapter_model.safetensors");
}

fn hf_list() -> Value {
    json!([
        {"id": "Comfy-Org/z_image_turbo", "author": "Comfy-Org", "likes": 900, "downloads": 45000000, "trendingScore": 80, "createdAt": "2025-11-27T00:00:00Z",
         "tags": ["comfyui", "diffusion-single-file", "base_model:Tongyi-MAI/Z-Image-Turbo", "license:apache-2.0", "text-to-image"],
         "cardData": {"license": "apache-2.0"}, "gated": false,
         "siblings": [{"rfilename": "split_files/diffusion_models/z_image_turbo_bf16.safetensors"}, {"rfilename": "README.md"}]},
        {"id": "someone/qwen-watercolor", "likes": 12, "tags": ["lora", "base_model:adapter:Qwen/Qwen-Image"], "siblings": [{"rfilename": "watercolor.safetensors"}]},
        {"id": "black-forest-labs/FLUX.1-dev", "likes": 12000, "gated": "auto", "tags": ["text-to-image", "license:other"], "cardData": {"license": "other", "license_name": "flux-1-dev-non-commercial-license"},
         "siblings": [{"rfilename": "flux1-dev.safetensors"}, {"rfilename": "ae.safetensors"}]}
    ])
}

#[test]
fn hugging_face_cards() {
    let reg = registry();
    let models = parse_hf_list(reg, "https://huggingface.co", &hf_list(), Kind::Model);
    assert_eq!(models.len(), 2);
    let z = &models[0];
    assert_eq!(z.family.as_deref(), Some("z-image"));
    assert_eq!(z.files[0].folder, "diffusion_models");
    assert_eq!(z.files[0].url, "https://huggingface.co/Comfy-Org/z_image_turbo/resolve/main/split_files/diffusion_models/z_image_turbo_bf16.safetensors");
    assert_eq!(
        z.files[0].hf_path("https://huggingface.co"),
        Some(("Comfy-Org/z_image_turbo".into(), "split_files/diffusion_models/z_image_turbo_bf16.safetensors".into()))
    );
    assert_eq!(z.license, "apache-2.0");
    let flux = &models[1];
    assert!(flux.gated);
    assert_eq!(flux.family.as_deref(), Some("flux1"));
    assert_eq!(flux.license, "flux-1-dev-non-commercial-license");
    let loras = parse_hf_list(reg, "https://huggingface.co", &hf_list(), Kind::Lora);
    assert_eq!(loras.len(), 1);
    assert_eq!(loras[0].files[0].folder, "loras");
    assert_eq!(loras[0].family.as_deref(), Some("qwen-image"));
}

/// LoRA collections without the `lora` tag are LoRAs (not models installing into
/// `diffusion_models`), and a LoRA shipped beside a model installs into `loras`.
#[test]
fn untagged_lora_repos_and_files_go_to_the_loras() {
    let reg = registry();
    let list = json!([
        {"id": "XLabs-AI/flux-lora-collection", "tags": ["text-to-image"], "siblings": [{"rfilename": "realism_lora.safetensors"}, {"rfilename": "art_lora.safetensors"}]},
        {"id": "someone/ink-styles", "tags": [], "siblings": [{"rfilename": "sdxl/ink_lora.safetensors"}, {"rfilename": "sdxl/pencil-LoRA.safetensors"}]},
        {"id": "someone/qwen-image-pack", "tags": ["text-to-image"],
         "siblings": [{"rfilename": "qwen_image_bf16.safetensors"}, {"rfilename": "Qwen-Image-Lightning-8steps-lora.safetensors"}]}
    ]);
    let models = parse_hf_list(reg, "https://huggingface.co", &list, Kind::Model);
    assert_eq!(models.iter().map(|m| m.id.as_str()).collect::<Vec<_>>(), vec!["hf:someone/qwen-image-pack"]);
    let pack = &models[0];
    assert_eq!(pack.files[0].folder, "diffusion_models");
    assert_eq!(pack.files[1].folder, "loras", "{:?}", pack.files);
    let loras = parse_hf_list(reg, "https://huggingface.co", &list, Kind::Lora);
    assert_eq!(loras.len(), 2);
    assert!(loras.iter().all(|l| l.files.iter().all(|f| f.folder == "loras")));
    assert_eq!(loras[0].family.as_deref(), Some("flux1"));
}

fn civitai() -> Value {
    json!({"items": [
        {"id": 101, "name": "Juggernaut XL", "type": "Checkpoint", "nsfw": false, "allowCommercialUse": ["Image", "RentCivit"],
         "creator": {"username": "KandooAI"}, "stats": {"downloadCount": 900000, "thumbsUpCount": 30000}, "tags": ["photorealistic"],
         "modelVersions": [{"id": 5, "baseModel": "SDXL 1.0", "publishedAt": "2025-01-01",
            "files": [{"name": "juggernautXL_v9.safetensors", "sizeKB": 6775430.0, "type": "Model", "hashes": {"SHA256": "AA11AA11AA11AA11AA11AA11AA11AA11AA11AA11AA11AA11AA11AA11AA11AA11"}, "downloadUrl": "https://civitai.com/api/download/models/5"}],
            "images": [{"url": "https://image.civitai.com/x/nsfw.jpeg", "nsfwLevel": 8}, {"url": "https://image.civitai.com/x/ok.jpeg", "nsfwLevel": 1}]}]},
        {"id": 102, "name": "Pony style", "type": "LORA", "nsfw": false, "modelVersions": [{"id": 6, "baseModel": "Pony", "files": [{"name": "style.safetensors", "sizeKB": 100.0, "hashes": {"SHA256": "BB"}, "downloadUrl": "https://civitai.com/api/download/models/6"}]}]},
        {"id": 103, "name": "Hidden", "type": "Checkpoint", "nsfw": true, "modelVersions": [{"id": 7, "baseModel": "SD 1.5", "files": [{"name": "x.safetensors", "downloadUrl": "https://civitai.com/api/download/models/7"}]}]},
        {"id": 104, "name": "Pickle", "type": "Checkpoint", "modelVersions": [{"id": 8, "baseModel": "SD 1.5", "files": [{"name": "x.ckpt", "downloadUrl": "https://civitai.com/api/download/models/8"}]}]}
    ]})
}

#[test]
fn civitai_cards_skip_nsfw_and_pickles() {
    let items = parse_civitai(registry(), &civitai());
    assert_eq!(items.len(), 2);
    let j = &items[0];
    assert_eq!(j.family.as_deref(), Some("sdxl"));
    assert_eq!(j.preview.as_deref(), Some("https://image.civitai.com/x/ok.jpeg"));
    assert_eq!(j.files[0].folder, "checkpoints");
    assert_eq!(j.files[0].sha256.as_deref().map(str::len), Some(64));
    assert_eq!(j.commercial, Some(true));
    assert_eq!(items[1].family.as_deref(), Some("pony"));
    assert_eq!(items[1].kind, Some(Kind::Lora));
}

#[test]
fn templates_list_their_files_including_subgraphs() {
    let t = json!({
        "nodes": [{"id": 1, "type": "SaveImage", "properties": {}}],
        "definitions": {"subgraphs": [{"nodes": [
            {"type": "UNETLoader", "properties": {"models": [{"name": "qwen_image_fp8_e4m3fn.safetensors", "url": "https://huggingface.co/Comfy-Org/Qwen-Image_ComfyUI/resolve/main/split_files/diffusion_models/qwen_image_fp8_e4m3fn.safetensors", "directory": "diffusion_models"}]}},
            {"type": "VAELoader", "properties": {"models": [{"name": "qwen_image_vae.safetensors", "url": "https://huggingface.co/Comfy-Org/Qwen-Image_ComfyUI/resolve/main/split_files/vae/qwen_image_vae.safetensors", "directory": "vae", "hash": "CC", "hash_type": "SHA256"}]}}
        ]}]}
    });
    let files = template_files(&t);
    assert_eq!(files.len(), 2);
    assert_eq!(files[1].sha256.as_deref(), Some("cc"));
    let e = TemplateEntry { name: "image_qwen_image".into(), title: "Qwen-Image: Text to Image".into(), size: 30_000_000_000, ..Default::default() };
    let item = template_item(registry(), &e, files, "https://example.org/templates");
    assert_eq!(item.family.as_deref(), Some("qwen-image"));
    assert!(!item.basic_controls());
    assert_eq!(item.preview.as_deref(), Some("https://example.org/templates/image_qwen_image-1.webp"));
    let unknown = TemplateEntry { name: "image_omnigen2_t2i".into(), ..Default::default() };
    let files = vec![CatalogFile { name: "omnigen2_fp16.safetensors".into(), folder: "diffusion_models".into(), ..Default::default() }];
    assert!(template_item(registry(), &unknown, files, "x").basic_controls());
}

#[test]
fn real_template_index_parses() {
    // A trimmed copy of the official index.json shape.
    let idx = json!([
        {"moduleName": "default", "category": "Foundation", "title": "Image", "type": "image", "templates": [
            {"name": "image_qwen_image", "title": "Qwen-Image: Text to Image", "mediaType": "image", "tags": ["Image", "Text to Image"], "models": ["Qwen-Image"], "date": "2025-08-05", "size": 31782757990u64, "openSource": true},
            {"name": "api_flux2", "title": "API", "mediaType": "image", "tags": ["API"], "openSource": false}
        ]},
        {"moduleName": "default", "category": "Foundation", "title": "Video", "type": "video", "templates": [{"name": "video_wan", "mediaType": "image"}]}
    ]);
    let t = parse_template_index(&idx);
    assert_eq!(t.len(), 1);
    assert_eq!(t[0].size, 31782757990);
}

#[test]
fn manager_list_maps_types_bases_and_sizes() {
    let v = json!({"models": [
        {"name": "Qwen-Image Diffusion Model (bf16)", "type": "diffusion_model", "base": "Qwen-Image", "save_path": "diffusion_models/qwen-image", "description": "d", "reference": "https://huggingface.co/Comfy-Org/Qwen-Image_ComfyUI", "filename": "qwen_image_bf16.safetensors", "url": "https://huggingface.co/Comfy-Org/Qwen-Image_ComfyUI/resolve/main/split_files/diffusion_models/qwen_image_bf16.safetensors", "size": "9.78GB"},
        {"name": "RealESRGAN x2", "type": "upscale", "base": "upscale", "save_path": "default", "filename": "RealESRGAN_x2.pth", "url": "https://x/RealESRGAN_x2.pth", "size": "67.1MB"}
    ]});
    let items = parse_manager(registry(), &v);
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].family.as_deref(), Some("qwen-image"));
    assert_eq!(items[0].files[0].folder, "diffusion_models/qwen-image");
    assert_eq!(items[0].files[0].approx_bytes, Some(9_780_000_000));
}

struct FakeNet(BTreeMap<String, Vec<u8>>, Option<u64>);
impl Net for FakeNet {
    fn get(&self, url: &str) -> Result<Vec<u8>> {
        self.0.get(url).cloned().with_context(|| format!("404 {url}"))
    }
    fn head_size(&self, _url: &str) -> Result<Option<u64>> {
        Ok(self.1)
    }
}

#[test]
fn plans_verify_add_components_and_refuse_unverified() {
    let reg = registry();
    let cfg = Config::default();
    let info = ObjectInfo(json!({}));
    let sha = "a".repeat(64);
    let tree = json!([
        {"type": "file", "path": "split_files/diffusion_models/z_image_turbo_bf16.safetensors", "size": 12309866400u64, "lfs": {"oid": sha, "size": 12309866400u64}},
    ]);
    let net = FakeNet(
        [("https://huggingface.co/api/models/Comfy-Org/z_image_turbo/tree/main?recursive=true".to_owned(), serde_json::to_vec(&tree).unwrap())].into(),
        None,
    );
    let items = parse_hf_list(reg, &cfg.hf, &hf_list(), Kind::Model);
    let z = &items[0];
    let p = plan(z, &z.files, reg.family("z-image"), &info, &cfg, &net);
    // Z-Image's text encoder and VAE have no pinned download or source in its profile: only
    // the model is planned.
    let p = p.unwrap();
    assert_eq!(p.files[0].bytes, 12309866400);
    assert_eq!(p.files[0].sha256, sha);
    // Civitai: hash published, size from HEAD (must agree with the listing).
    let civ = parse_civitai(reg, &civitai());
    let j = &civ[0];
    let ok = FakeNet(BTreeMap::new(), Some((6775430.0f64 * 1024.0) as u64));
    let p = plan(j, &j.files, reg.family("sdxl"), &info, &cfg, &ok).unwrap();
    assert_eq!(p.files.len(), 1);
    let wrong = FakeNet(BTreeMap::new(), Some(10));
    assert!(plan(j, &j.files, reg.family("sdxl"), &info, &cfg, &wrong).is_err());
    // No hash: refused.
    let bad = CatalogFile {
        name: "x.safetensors".into(),
        folder: "checkpoints".into(),
        url: "https://civitai.com/api/download/models/9".into(),
        ..Default::default()
    };
    let e = plan(j, &[bad], None, &info, &cfg, &ok).unwrap_err().to_string();
    assert!(e.contains("no published size and SHA-256"), "{e}");
    // Already installed: skipped.
    let have = ObjectInfo(json!({"CheckpointLoaderSimple": {"input": {"required": {"ckpt_name": [["juggernautXL_v9.safetensors"]]}}}}));
    let p = plan(j, &j.files, reg.family("sdxl"), &have, &cfg, &ok).unwrap();
    assert!(p.files.is_empty() && p.present == vec!["juggernautXL_v9.safetensors"]);
    // FLUX.1 adds its text encoders and VAE from their Hugging Face sources.
    let flux_item = Item { id: "x".into(), kind: Some(Kind::Model), family: Some("flux1".into()), ..Default::default() };
    let e = plan(&flux_item, &[], reg.family("flux1"), &info, &cfg, &FakeNet(BTreeMap::new(), None)).unwrap_err().to_string();
    assert!(e.contains("comfyanonymous/flux_text_encoders"), "{e}");
}

#[test]
fn queries_build_urls_and_filter() {
    let reg = registry();
    let cfg = Config::default();
    let q = Query { view: View::Family("pony".into()), kind: Kind::Lora, search: "ink style".into(), sources: vec![] };
    let u = urls(&cfg, reg, &q);
    assert!(u.iter().any(|(s, u)| *s == Source::Civitai && u.contains("baseModels=Pony") && u.contains("types=LORA") && u.contains("query=ink%20style")));
    assert!(u.iter().any(|(s, u)| *s == Source::HuggingFace && u.contains("filter=lora")));
    assert!(!u.iter().any(|(s, _)| *s == Source::Template));
    let q = Query { view: View::FitsGpu(12.0), kind: Kind::Model, search: String::new(), sources: vec![Source::Civitai] };
    let bodies = vec![(Source::Civitai, civitai())];
    let all = items(reg, &cfg, &q, &bodies);
    assert!(all.iter().all(|i| i.vram_gb().unwrap() <= 12.0));
    let cache = Cache::new(std::env::temp_dir().join(format!("li-cat-{}", uuid::Uuid::new_v4().simple())));
    let net = FakeNet([("https://x/a".to_owned(), b"[1]".to_vec())].into(), None);
    assert_eq!(cache.fetch(&net, "https://x/a", true).unwrap(), (b"[1]".to_vec(), false));
    let offline = FakeNet(BTreeMap::new(), None);
    assert_eq!(cache.fetch(&offline, "https://x/a", true).unwrap(), (b"[1]".to_vec(), true));
    assert!(cache.fetch(&offline, "https://x/b", false).is_err());
    let _ = std::fs::remove_dir_all(&cache.dir);
}

#[test]
fn install_rejects_unsafe_catalogue_paths_before_io() {
    let root = std::env::temp_dir().join(format!("li-model-paths-{}", uuid::Uuid::new_v4()));
    let cfg = Config::default();
    for (folder, name) in [
        ("checkpoints", "../escape.safetensors"),
        ("checkpoints", "/escape.gguf"),
        ("../escape", "model.gguf"),
        ("/tmp", "model.gguf"),
        ("safe/../escape", "model.gguf"),
        ("safe", "C:\\escape.gguf"),
        ("C:escape", "model.gguf"),
        ("safe\\escape", "model.gguf"),
        ("safe//nested", "model.gguf"),
        ("safe", "NUL.safetensors"),
        ("safe", "model.gguf "),
        ("safe", "sub/model.gguf"),
        ("safe", ""),
        ("safe", "x\0.gguf"),
    ] {
        let file = PlannedFile { folder: folder.into(), name: name.into(), url: "https://huggingface.co/unused".into(), bytes: 1, sha256: "0".repeat(64) };
        let plan = Plan { files: vec![file], ..Default::default() };
        let error = install(&plan, &root, &cfg, &crate::comfy::JobControl::new(), &|_, _, _| {}).unwrap_err();
        assert!(error.to_string().contains("Invalid model destination"), "{folder}/{name}: {error}");
        assert!(!root.exists(), "a rejected catalogue path created files");
        let file = CatalogFile { folder: folder.into(), name: name.into(), ..Default::default() };
        assert!(
            super::plan(&Item::default(), &[file], None, &ObjectInfo(Value::Null), &cfg, &FakeNet(BTreeMap::new(), None))
                .unwrap_err()
                .to_string()
                .contains("Invalid model destination")
        );
    }
    for (folder, name) in [("checkpoints", "my model.safetensors"), ("loras/style", "モデル.gguf"), ("", "root.gguf")] {
        validate_install_path(folder, name).unwrap();
    }
}

#[cfg(unix)]
#[test]
fn install_rejects_escaping_and_dangling_symlinks() {
    use std::os::unix::fs::symlink;
    let base = std::env::temp_dir().join(format!("li-model-links-{}", uuid::Uuid::new_v4()));
    let root = base.join("models");
    let outside = base.join("outside");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::create_dir_all(&outside).unwrap();
    symlink(&outside, root.join("checkpoints")).unwrap();
    let file =
        PlannedFile { folder: "checkpoints".into(), name: "model.gguf".into(), url: "https://huggingface.co/unused".into(), bytes: 1, sha256: "0".repeat(64) };
    let cfg = Config::default();
    let ctl = crate::comfy::JobControl::new();
    let plan = Plan { files: vec![file.clone()], ..Default::default() };
    assert!(install(&plan, &root, &cfg, &ctl, &|_, _, _| {}).unwrap_err().to_string().contains("leaves the selected"));
    assert_eq!(std::fs::read_dir(&outside).unwrap().count(), 0);
    std::fs::remove_file(root.join("checkpoints")).unwrap();
    std::fs::create_dir(root.join("checkpoints")).unwrap();
    symlink(outside.join("missing.gguf"), root.join("checkpoints/model.gguf")).unwrap();
    assert!(install(&plan, &root, &cfg, &ctl, &|_, _, _| {}).unwrap_err().to_string().contains("symbolic link"));
    assert_eq!(std::fs::read_dir(&outside).unwrap().count(), 0);
    std::fs::remove_file(root.join("checkpoints/model.gguf")).unwrap();
    // Safe nested paths and an explicitly chosen root link remain usable.
    let root_link = base.join("chosen-root");
    symlink(&root, &root_link).unwrap();
    let nested = PlannedFile { folder: "checkpoints/nested".into(), ..file };
    // macOS canonicalizes /var to /private/var; compare both roots in the same form.
    assert_eq!(confined_destination(&root_link.canonicalize().unwrap(), &nested).unwrap(), root.canonicalize().unwrap().join("checkpoints/nested/model.gguf"));
    std::fs::remove_dir_all(base).unwrap();
}

/// The whole flow against the mock server: browse every source, install a Hugging Face
/// model, a Civitai checkpoint (through its CDN redirect) and an official template, refuse the
/// unverified and gated ones, and see the installs in ComfyUI's refreshed listing.
#[test]
fn browse_and_install_against_the_mock() {
    let mock = crate::mock::MockComfy::start().unwrap();
    let host = mock.host().to_owned();
    let models = std::env::temp_dir().join(format!("li-browser-{}", uuid::Uuid::new_v4().simple()));
    mock.set_model_dir(&models);
    let cfg = Config {
        hf: format!("http://{host}/hf"),
        civitai: format!("http://{host}/civitai"),
        manager_list: format!("http://{host}/manager/model-list.json"),
        templates: format!("http://{host}/templates"),
        hf_token: None,
        civitai_token: None,
    };
    let net = HttpNet::new(cfg.clone());
    let reg = registry();
    let info =
        || ObjectInfo(serde_json::from_slice(&ureq::get(&format!("http://{host}/object_info")).call().unwrap().body_mut().read_to_vec().unwrap()).unwrap());
    let cache = Cache::new(models.join(".cache"));
    let fetch = |q: &Query| {
        let bodies: Vec<(Source, Value)> =
            urls(&cfg, reg, q).into_iter().map(|(s, u)| (s, serde_json::from_slice(&cache.fetch(&net, &u, true).unwrap().0).unwrap())).collect();
        items(reg, &cfg, q, &bodies)
    };
    let trending = fetch(&Query { view: View::Trending, kind: Kind::Model, search: String::new(), sources: vec![] });
    let titles: Vec<&str> = trending.iter().map(|i| i.title.as_str()).collect();
    assert!(titles.contains(&"z-image-turbo-mini") && titles.contains(&"Mockernaut XL") && titles.contains(&"Qwen-Image: Text to Image"), "{titles:?}");
    assert!(!titles.contains(&"Hidden"));
    let pony_loras = fetch(&Query { view: View::Family("pony".into()), kind: Kind::Lora, search: String::new(), sources: vec![] });
    assert_eq!(pony_loras.iter().map(|i| i.title.as_str()).collect::<Vec<_>>(), vec!["Ink Wash (Pony)"]);

    let ctl = crate::comfy::JobControl::new();
    let install_item = |item: &Item, fam: Option<&Family>| -> Result<Plan> {
        let p = plan(item, &item.files, fam, &info(), &cfg, &net)?;
        install(&p, &models, &cfg, &ctl, &|_, _, _| {})?;
        Ok(p)
    };
    // Hugging Face: size and hash from the repo tree.
    let z = trending.iter().find(|i| i.title == "z-image-turbo-mini").unwrap();
    let p = install_item(z, reg.family("z-image")).unwrap();
    assert!(p.files.iter().any(|f| f.name == "z_image_turbo_mini.safetensors"));
    assert!(models.join("diffusion_models/z_image_turbo_mini.safetensors").exists());
    assert!(info().find_file("UNETLoader", "unet_name", "z_image_turbo_mini.safetensors").is_some());
    // Civitai: published hash, size from the download's headers, CDN redirect.
    let civ = trending.iter().find(|i| i.title == "Mockernaut XL").unwrap();
    install_item(civ, None).unwrap();
    assert!(models.join("checkpoints/mockernautXL_v1.safetensors").exists());
    assert!(plan(civ, &civ.files, None, &info(), &cfg, &net).unwrap().files.is_empty(), "installed files are skipped");
    // No published hash: refused before anything downloads.
    let fresh = fetch(&Query { view: View::New, kind: Kind::Model, search: "unverified".into(), sources: vec![Source::Civitai] });
    let e = install_item(&fresh[0], None).unwrap_err().to_string();
    assert!(e.contains("no published size and SHA-256"), "{e}");
    assert!(!models.join("checkpoints/unverified_mix.safetensors").exists());
    // Gated: the licence must be accepted on the publisher's page.
    let gated = trending.iter().find(|i| i.gated).unwrap();
    let e = install_item(gated, None).unwrap_err().to_string();
    assert!(e.contains("accept its licence"), "{e}");
    // An official template for an unknown family: its files, then basic controls.
    let (ui, files) = fetch_template(&cfg, &net, "image_omnigen2_t2i").unwrap();
    assert!(!files.is_empty());
    let item = Item { id: "template:image_omnigen2_t2i".into(), template: Some("image_omnigen2_t2i".into()), ..Default::default() };
    let p = plan(&item, &files, None, &info(), &cfg, &net).unwrap();
    install(&p, &models, &cfg, &ctl, &|_, _, _| {}).unwrap();
    for f in &p.files {
        assert!(models.join(&f.folder).join(&f.name).exists(), "{}", f.name);
    }
    let w = crate::custom::import("OmniGen2", &ui, Some(&info())).unwrap();
    assert!(w.basic && w.has(&crate::custom::FieldKind::Prompt));
    // Offline: the cached catalogue still answers.
    let q = Query { view: View::Trending, kind: Kind::Model, search: String::new(), sources: vec![Source::HuggingFace] };
    let (_, u0) = urls(&cfg, reg, &q).remove(0);
    let cached = cache.get(&u0).unwrap().0;
    // Close the client's pooled keep-alive connection before stopping the mock.
    // Reusing that dead connection tests a 60-second HTTP timeout, not cache fallback.
    drop(net);
    drop(mock);
    let offline = HttpNet::new(cfg);
    assert_eq!(cache.fetch(&offline, &u0, true).unwrap(), (cached, true), "served from the cache after a real connection failure");
    let _ = std::fs::remove_dir_all(&models);
}
