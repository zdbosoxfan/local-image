use li_ai::browser::{
    Cache, CatalogFile, Config, Kind, Query, Source, TemplateEntry, View, family_for_label, items, lora_file_name, model_folder, parse_civitai, parse_hf_list,
    parse_hf_tree, parse_manager, parse_template_index, template_files, template_item, urlencode, urls,
};
use li_ai::family::registry;
use serde_json::json;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

fn unique_temp_dir(tag: &str) -> PathBuf {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("li-browser-test-{tag}-{}-{}", std::process::id(), n));
    std::fs::create_dir_all(&dir).ok();
    dir
}

#[test]
fn lora_file_name_edge_cases() {
    assert_eq!(lora_file_name("me/cool-style", "adapter_model.safetensors"), "cool-style-adapter_model.safetensors");
    assert_eq!(lora_file_name("me/cool-style", "v2/pytorch_lora_weights.safetensors"), "cool-style-v2-pytorch_lora_weights.safetensors");
    assert_eq!(lora_file_name("me/Cool-Style", "cool-style_v1.safetensors"), "cool-style_v1.safetensors");
    assert_ne!(lora_file_name("a/x", "adapter_model.safetensors"), lora_file_name("b/y", "adapter_model.safetensors"));
    assert_eq!(lora_file_name("me/repo", "folder/name with spaces.safetensors"), "repo-folder-name_with_spaces.safetensors");
}

#[test]
fn hf_path_parses_resolve_urls() {
    let f = CatalogFile {
        url: "https://hf.co/Comfy-Org/z_image_turbo/resolve/main/split_files/diffusion_models/z_image_turbo_bf16.safetensors?download=true".into(),
        ..Default::default()
    };
    assert_eq!(f.hf_path("https://hf.co"), Some(("Comfy-Org/z_image_turbo".into(), "split_files/diffusion_models/z_image_turbo_bf16.safetensors".into())));
    let bad = CatalogFile { url: "https://hf.co/Comfy-Org/z_image_turbo".into(), ..Default::default() };
    assert_eq!(bad.hf_path("https://hf.co"), None);
    let base = "https://hf.co/";
    assert_eq!(f.hf_path(base), Some(("Comfy-Org/z_image_turbo".into(), "split_files/diffusion_models/z_image_turbo_bf16.safetensors".into())));
}

#[test]
fn hf_tree_masks_and_valid_entries() {
    let sha = "a".repeat(64);
    let v = json!([
        {"path": "a.safetensors", "lfs": {"oid": sha, "size": 10}},
        {"path": "b.safetensors", "lfs": {"oid": "*".repeat(64), "size": 20}},
        {"path": "c.safetensors", "size": 30}
    ]);
    let map = parse_hf_tree(&v);
    assert_eq!(map.len(), 1);
    assert_eq!(map.get("a.safetensors"), Some(&(10, sha)));
    assert!(!map.contains_key("b.safetensors"));
    assert!(!map.contains_key("c.safetensors"));
}

#[test]
fn hf_list_filters_non_image_and_fields() {
    let reg = registry();
    let v = json!([
        {
            "id": "Comfy-Org/z_image_turbo",
            "author": "Comfy-Org",
            "likes": 900,
            "downloads": 45000000,
            "trendingScore": 80.0,
            "createdAt": "2025-11-27T00:00:00Z",
            "tags": ["comfyui", "diffusion-single-file", "base_model:Tongyi-MAI/Z-Image-Turbo", "license:apache-2.0", "text-to-image"],
            "cardData": {"license": "apache-2.0"},
            "gated": false,
            "siblings": [{"rfilename": "split_files/diffusion_models/z_image_turbo_bf16.safetensors"}]
        },
        {
            "id": "x/text-gen",
            "tags": ["text-generation"],
            "pipeline_tag": "text-generation",
            "siblings": [{"rfilename": "model.safetensors"}]
        }
    ]);
    let items = parse_hf_list(reg, "https://huggingface.co", &v, Kind::Model);
    assert_eq!(items.len(), 1);
    let z = &items[0];
    assert_eq!(z.id, "hf:Comfy-Org/z_image_turbo");
    assert_eq!(z.title, "z_image_turbo");
    assert_eq!(z.author, "Comfy-Org");
    assert_eq!(z.family.as_deref(), Some("z-image"));
    assert_eq!(z.license, "apache-2.0");
    assert_eq!(z.files.len(), 1);
    assert_eq!(z.files[0].folder, "diffusion_models");
    assert_eq!(z.files[0].url, "https://huggingface.co/Comfy-Org/z_image_turbo/resolve/main/split_files/diffusion_models/z_image_turbo_bf16.safetensors");
    assert!(!z.gated);
}

#[test]
fn hf_list_untagged_loras_and_mixed_folders() {
    let reg = registry();
    let v = json!([
        {
            "id": "XLabs-AI/flux-lora-collection",
            "tags": ["text-to-image"],
            "siblings": [{"rfilename": "realism_lora.safetensors"}, {"rfilename": "art_lora.safetensors"}]
        },
        {
            "id": "someone/ink-styles",
            "tags": [],
            "siblings": [{"rfilename": "sdxl/ink_lora.safetensors"}, {"rfilename": "sdxl/pencil-LoRA.safetensors"}]
        },
        {
            "id": "someone/qwen-image-pack",
            "tags": ["text-to-image"],
            "siblings": [{"rfilename": "qwen_image_bf16.safetensors"}, {"rfilename": "Qwen-Image-Lightning-8steps-lora.safetensors"}]
        }
    ]);
    let loras = parse_hf_list(reg, "https://huggingface.co", &v, Kind::Lora);
    assert_eq!(loras.len(), 2);
    assert!(loras.iter().all(|i| i.files.iter().all(|f| f.folder == "loras")));
    let models = parse_hf_list(reg, "https://huggingface.co", &v, Kind::Model);
    assert_eq!(models.len(), 1);
    let pack = &models[0];
    assert_eq!(pack.files[0].folder, "diffusion_models");
    assert_eq!(pack.files[1].folder, "loras");
    assert_eq!(pack.family.as_deref(), Some("qwen-image"));
}

#[test]
fn civitai_parses_skips_nsfw_and_non_weights() {
    let reg = registry();
    let v = json!({
        "items": [
            {
                "id": 101,
                "name": "Juggernaut XL",
                "type": "Checkpoint",
                "nsfw": false,
                "allowCommercialUse": ["Image"],
                "creator": {"username": "KandooAI"},
                "stats": {"downloadCount": 900000, "thumbsUpCount": 30000},
                "tags": ["photorealistic"],
                "modelVersions": [{
                    "baseModel": "SDXL 1.0",
                    "publishedAt": "2025-01-01",
                    "files": [{
                        "name": "juggernautXL_v9.safetensors",
                        "sizeKB": 6775430.0,
                        "type": "Model",
                        "hashes": {"SHA256": "AA11AA11AA11AA11AA11AA11AA11AA11AA11AA11AA11AA11AA11AA11AA11AA11"},
                        "downloadUrl": "https://civitai.com/api/download/models/101"
                    }],
                    "images": [
                        {"url": "https://image.civitai.com/x/nsfw.jpeg", "nsfwLevel": 8},
                        {"url": "https://image.civitai.com/x/ok.jpeg", "nsfwLevel": 1}
                    ]
                }]
            },
            {
                "id": 102,
                "name": "Pony style",
                "type": "LORA",
                "nsfw": false,
                "modelVersions": [{
                    "baseModel": "Pony",
                    "files": [{
                        "name": "style.safetensors",
                        "sizeKB": 100.0,
                        "hashes": {"SHA256": "BB"},
                        "downloadUrl": "https://civitai.com/api/download/models/102"
                    }]
                }]
            },
            {
                "id": 103,
                "name": "Hidden",
                "type": "Checkpoint",
                "nsfw": true,
                "modelVersions": [{
                    "baseModel": "SD 1.5",
                    "files": [{"name": "x.safetensors", "downloadUrl": "https://civitai.com/api/download/models/103"}]
                }]
            },
            {
                "id": 104,
                "name": "Pickle",
                "type": "Checkpoint",
                "modelVersions": [{
                    "baseModel": "SD 1.5",
                    "files": [{"name": "x.ckpt", "downloadUrl": "https://civitai.com/api/download/models/104"}]
                }]
            }
        ]
    });
    let items = parse_civitai(reg, &v);
    assert_eq!(items.len(), 2);
    let j = &items[0];
    assert_eq!(j.family.as_deref(), Some("sdxl"));
    assert_eq!(j.preview.as_deref(), Some("https://image.civitai.com/x/ok.jpeg"));
    assert_eq!(j.files[0].folder, "checkpoints");
    assert_eq!(j.files[0].sha256.as_deref().map(str::len), Some(64));
    assert_eq!(j.commercial, Some(true));
    let p = &items[1];
    assert_eq!(p.family.as_deref(), Some("pony"));
    assert_eq!(p.kind, Some(Kind::Lora));
}

#[test]
fn manager_parses_types_bases_and_sizes() {
    let reg = registry();
    let v = json!({
        "models": [
            {
                "name": "Qwen-Image Diffusion Model (bf16)",
                "type": "diffusion_model",
                "base": "Qwen-Image",
                "save_path": "diffusion_models/qwen-image",
                "description": "d",
                "reference": "https://huggingface.co/Comfy-Org/Qwen-Image_ComfyUI",
                "filename": "qwen_image_bf16.safetensors",
                "url": "https://huggingface.co/Comfy-Org/Qwen-Image_ComfyUI/resolve/main/split_files/diffusion_models/qwen_image_bf16.safetensors",
                "size": "9.78GB"
            },
            {
                "name": "RealESRGAN x2",
                "type": "upscale",
                "base": "upscale",
                "save_path": "default",
                "filename": "RealESRGAN_x2.pth",
                "url": "https://x/RealESRGAN_x2.pth",
                "size": "67.1MB"
            }
        ]
    });
    let items = parse_manager(reg, &v);
    assert_eq!(items.len(), 1);
    let q = &items[0];
    assert_eq!(q.family.as_deref(), Some("qwen-image"));
    assert_eq!(q.files[0].folder, "diffusion_models/qwen-image");
    assert_eq!(q.files[0].approx_bytes, Some(9_780_000_000));
}

#[test]
fn template_index_parses_open_source_image_only() {
    let idx = json!([
        {
            "moduleName": "default",
            "category": "Foundation",
            "title": "Image",
            "type": "image",
            "templates": [
                {
                    "name": "image_qwen_image",
                    "title": "Qwen-Image: Text to Image",
                    "mediaType": "image",
                    "tags": ["Image", "Text to Image"],
                    "models": ["Qwen-Image"],
                    "date": "2025-08-05",
                    "size": 31782757990u64,
                    "openSource": true
                },
                {
                    "name": "api_flux2",
                    "title": "API",
                    "mediaType": "image",
                    "tags": ["API"],
                    "openSource": false
                }
            ]
        },
        {
            "moduleName": "default",
            "category": "Foundation",
            "title": "Video",
            "type": "video",
            "templates": [{"name": "video_wan", "mediaType": "image"}]
        }
    ]);
    let entries = parse_template_index(&idx);
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].name, "image_qwen_image");
    assert_eq!(entries[0].size, 31782757990);
}

#[test]
fn template_files_collects_and_dedups() {
    let t = json!({
        "nodes": [
            {
                "type": "UNETLoader",
                "properties": {
                    "models": [
                        {
                            "name": "qwen_image_fp8_e4m3fn.safetensors",
                            "url": "https://huggingface.co/Comfy-Org/Qwen-Image_ComfyUI/resolve/main/split_files/diffusion_models/qwen_image_fp8_e4m3fn.safetensors",
                            "directory": "diffusion_models"
                        }
                    ]
                }
            },
            {
                "type": "VAELoader",
                "properties": {
                    "models": [
                        {
                            "name": "qwen_image_vae.safetensors",
                            "url": "https://huggingface.co/Comfy-Org/Qwen-Image_ComfyUI/resolve/main/split_files/vae/qwen_image_vae.safetensors",
                            "directory": "vae",
                            "hash": "CC",
                            "hash_type": "SHA256"
                        }
                    ]
                }
            }
        ],
        "definitions": {
            "subgraphs": [
                {
                    "nodes": [
                        {
                            "type": "UNETLoader",
                            "properties": {
                                "models": [
                                    {
                                        "name": "qwen_image_fp8_e4m3fn.safetensors",
                                        "url": "https://huggingface.co/Comfy-Org/Qwen-Image_ComfyUI/resolve/main/split_files/diffusion_models/qwen_image_fp8_e4m3fn.safetensors",
                                        "directory": "diffusion_models"
                                    }
                                ]
                            }
                        }
                    ]
                }
            ]
        }
    });
    let files = template_files(&t);
    assert_eq!(files.len(), 2);
    assert_eq!(files[0].folder, "diffusion_models");
    assert_eq!(files[1].folder, "vae");
    assert_eq!(files[1].sha256.as_deref(), Some("cc"));
}

#[test]
fn template_item_family_and_basic_controls() {
    let reg = registry();
    let e = TemplateEntry { name: "image_qwen_image".into(), title: "Qwen-Image: Text to Image".into(), size: 30_000_000_000, ..Default::default() };
    let files = vec![CatalogFile { name: "qwen_image_fp8_e4m3fn.safetensors".into(), folder: "diffusion_models".into(), ..Default::default() }];
    let item = template_item(reg, &e, files.clone(), "https://example.org/templates");
    assert_eq!(item.family.as_deref(), Some("qwen-image"));
    assert!(!item.basic_controls());
    assert_eq!(item.preview.as_deref(), Some("https://example.org/templates/image_qwen_image-1.webp"));

    let unknown_e = TemplateEntry { name: "image_omnigen2_t2i".into(), ..Default::default() };
    let unknown_files = vec![CatalogFile { name: "omnigen2_fp16.safetensors".into(), folder: "diffusion_models".into(), ..Default::default() }];
    let unknown_item = template_item(reg, &unknown_e, unknown_files, "x");
    assert!(unknown_item.family.is_none());
    assert!(unknown_item.basic_controls());
}

#[test]
fn urlencode_encodes_space_and_special_chars() {
    assert_eq!(urlencode("a b"), "a%20b");
    assert_eq!(urlencode("a&b=c"), "a%26b%3Dc");
    assert_eq!(urlencode("abc-123_.~"), "abc-123_.~");
    assert_eq!(urlencode(""), "");
}

#[test]
fn urls_builds_for_sources_views_and_kinds() {
    let reg = registry();
    let cfg = Config::default();
    let q = Query { view: View::Family("pony".into()), kind: Kind::Lora, search: "ink style".into(), sources: vec![] };
    let u = urls(&cfg, reg, &q);
    assert!(
        u.iter()
            .any(|(s, url)| { *s == Source::Civitai && url.contains("baseModels=Pony") && url.contains("types=LORA") && url.contains("query=ink%20style") })
    );
    assert!(u.iter().any(|(s, url)| *s == Source::HuggingFace && url.contains("filter=lora")));
    assert!(!u.iter().any(|(s, _)| *s == Source::Template));
}

#[test]
fn items_filters_by_search_family_gpu_and_sorts() {
    let reg = registry();
    let cfg = Config::default();
    let hf_v = json!([
        {
            "id": "x/alpha",
            "tags": ["pony", "text-to-image"],
            "siblings": [{"rfilename": "alpha.safetensors"}]
        },
        {
            "id": "y/beta",
            "tags": ["lora", "pony"],
            "siblings": [{"rfilename": "beta.safetensors"}]
        }
    ]);
    let civ_v = json!({"items": [
        {
            "id": 200,
            "name": "Gamma",
            "type": "LORA",
            "nsfw": false,
            "modelVersions": [{
                "baseModel": "Pony",
                "files": [{"name": "gamma.safetensors", "sizeKB": 100.0, "hashes": {"SHA256": "A".repeat(64)}, "downloadUrl": "https://civitai.com/api/download/models/200"}]
            }]
        }
    ]});
    let bodies = vec![(Source::HuggingFace, hf_v), (Source::Civitai, civ_v)];
    let q = Query { view: View::Family("pony".into()), kind: Kind::Lora, search: "a".into(), sources: vec![] };
    let result = items(reg, &cfg, &q, &bodies);
    assert_eq!(result.len(), 2);
    assert!(result.iter().any(|i| i.title == "beta"));
    assert!(result.iter().any(|i| i.title == "Gamma"));
    assert!(!result.iter().any(|i| i.title == "alpha"));
}

#[test]
fn cache_put_get_and_overwrite() {
    let dir = unique_temp_dir("cache");
    let cache = Cache::new(&dir);
    let url = "https://example.com/data.json";
    let body = b"{\"a\":1}".to_vec();
    cache.put(url, &body).unwrap();

    let (got, age) = cache.get(url).unwrap();
    assert_eq!(got, body);
    assert!(age < 5);

    let new_body = b"{\"a\":2}".to_vec();
    cache.put(url, &new_body).unwrap();
    let (got2, _) = cache.get(url).unwrap();
    assert_eq!(got2, new_body);

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn model_folder_gguf_and_unet_family() {
    let reg = registry();
    assert_eq!(model_folder(reg, None, "x.gguf"), "diffusion_models");
    assert_eq!(model_folder(reg, None, "x.safetensors"), "checkpoints");
    assert_eq!(model_folder(reg, Some("z-image"), "x.safetensors"), "diffusion_models");
    assert_eq!(model_folder(reg, Some("sdxl"), "x.safetensors"), "checkpoints");
}

#[test]
fn family_for_label_maps_known_and_empty() {
    let reg = registry();
    assert_eq!(family_for_label(reg, "SDXL"), Some("sdxl".into()));
    assert_eq!(family_for_label(reg, "Pony"), Some("pony".into()));
    assert_eq!(family_for_label(reg, ""), None);
    assert_eq!(family_for_label(reg, "unknown"), None);
}
