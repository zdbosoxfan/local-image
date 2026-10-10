use std::collections::BTreeMap;
use std::path::PathBuf;

use anyhow::Result;
use li_ai::browser::*;
use li_ai::comfy::ObjectInfo;
use li_ai::family::registry;
use serde_json::{Value, json};

// -----------------------------------------------------------------------------
// Test helpers
// -----------------------------------------------------------------------------

/// A minimal `Net` implementation for tests, serving canned responses.
struct FakeNet {
    bodies: BTreeMap<String, Result<Vec<u8>, String>>,
    head: Option<u64>,
}

impl Net for FakeNet {
    fn get(&self, url: &str) -> Result<Vec<u8>> {
        match self.bodies.get(url).cloned() {
            Some(Ok(body)) => Ok(body),
            Some(Err(e)) => Err(anyhow::anyhow!(e)),
            None => Err(anyhow::anyhow!("fake 404 for {url}")),
        }
    }

    fn head_size(&self, _url: &str) -> Result<Option<u64>> {
        Ok(self.head)
    }
}

fn unique_temp_dir(prefix: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("{}-{}", prefix, uuid::Uuid::new_v4().simple()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn cleanup(dir: &PathBuf) {
    let _ = std::fs::remove_dir_all(dir);
}

// -----------------------------------------------------------------------------
// Source
// -----------------------------------------------------------------------------

#[test]
fn source_labels_are_stable() {
    assert_eq!(Source::HuggingFace.label(), "Hugging Face");
    assert_eq!(Source::Civitai.label(), "Civitai");
    assert_eq!(Source::Template.label(), "ComfyUI template");
    assert_eq!(Source::Community.label(), "Community list");
    assert_eq!(Source::Installed.label(), "Installed");
}

// -----------------------------------------------------------------------------
// CatalogFile
// -----------------------------------------------------------------------------

#[test]
fn catalog_file_hf_path_edge_cases() {
    let base = "https://huggingface.co";
    let f = CatalogFile { url: format!("{}/user/repo/resolve/main/path/file.safetensors?download=true", base), ..Default::default() };
    assert_eq!(f.hf_path(base), Some(("user/repo".to_string(), "path/file.safetensors".to_string())));

    // Different host
    let f2 = CatalogFile { url: "https://example.com/user/repo/resolve/main/file.safetensors".to_string(), ..Default::default() };
    assert_eq!(f2.hf_path(base), None);

    // Not a /resolve/ URL
    let f3 = CatalogFile { url: format!("{}/user/repo/blob/main/file.safetensors", base), ..Default::default() };
    assert_eq!(f3.hf_path(base), None);

    // Empty path after resolve
    let f4 = CatalogFile { url: format!("{}/user/repo/resolve/main/", base), ..Default::default() };
    assert_eq!(f4.hf_path(base), Some(("user/repo".to_string(), "".to_string())));

    // Multiple query parameters
    let f5 = CatalogFile { url: format!("{}/user/repo/resolve/main/file.safetensors?x=1&y=2", base), ..Default::default() };
    assert_eq!(f5.hf_path(base), Some(("user/repo".to_string(), "file.safetensors".to_string())));
}

#[test]
fn catalog_file_size_prefers_exact() {
    let f1 = CatalogFile { bytes: Some(100), approx_bytes: Some(90), ..Default::default() };
    assert_eq!(f1.size(), Some(100));

    let f2 = CatalogFile { bytes: None, approx_bytes: Some(90), ..Default::default() };
    assert_eq!(f2.size(), Some(90));

    let f3 = CatalogFile::default();
    assert_eq!(f3.size(), None);
}

// -----------------------------------------------------------------------------
// Item
// -----------------------------------------------------------------------------

#[test]
fn item_total_bytes_and_vram_gb_edge_cases() {
    // Empty files
    let empty = Item::default();
    assert_eq!(empty.total_bytes(), None);
    assert_eq!(empty.vram_gb(), None);

    // Mixed exact and approximate sizes
    let item = Item {
        files: vec![
            CatalogFile { bytes: Some(1_000_000_000), ..Default::default() },
            CatalogFile { bytes: Some(2_000_000_000), ..Default::default() },
            CatalogFile { bytes: None, approx_bytes: Some(500_000_000), ..Default::default() },
        ],
        ..Default::default()
    };
    assert_eq!(item.total_bytes(), Some(3_500_000_000));

    // vram_gb uses only the largest exact/approx size
    let largest = 2_000_000_000u64;
    let expected = ((largest as f32 / 1e9) * 1.2 + 1.5).ceil();
    assert_eq!(item.vram_gb(), Some(expected));
}

#[test]
fn item_basic_controls_logic() {
    let tpl_only = Item { template: Some("some_template".to_string()), family: None, ..Default::default() };
    assert!(tpl_only.basic_controls());

    let tpl_and_family = Item { template: Some("some_template".to_string()), family: Some("some_family".to_string()), ..Default::default() };
    assert!(!tpl_and_family.basic_controls());

    let family_only = Item { template: None, family: Some("some_family".to_string()), ..Default::default() };
    assert!(!family_only.basic_controls());

    let default_item = Item::default();
    assert!(!default_item.basic_controls());
}

// -----------------------------------------------------------------------------
// family_for_label
// -----------------------------------------------------------------------------

#[test]
fn family_for_label_variants_and_unknown() {
    let reg = registry();
    // Empty / whitespace
    assert_eq!(family_for_label(reg, ""), None);
    assert_eq!(family_for_label(reg, "   "), None);

    // Exact matches
    assert_eq!(family_for_label(reg, "SDXL"), Some("sdxl".to_string()));
    assert_eq!(family_for_label(reg, "sd 1.5"), Some("sd15".to_string()));
    assert_eq!(family_for_label(reg, "pony"), Some("pony".to_string()));
    assert_eq!(family_for_label(reg, "illustrious"), Some("illustrious".to_string()));
    assert_eq!(family_for_label(reg, "noobai"), Some("illustrious".to_string()));

    // Prefix variants
    assert_eq!(family_for_label(reg, "flux.1"), Some("flux1".to_string()));
    assert_eq!(family_for_label(reg, "flux.2 klein"), Some("flux2-klein".to_string()));
    assert_eq!(family_for_label(reg, "qwen-image"), Some("qwen-image".to_string()));
    assert_eq!(family_for_label(reg, "qwen 2"), Some("qwen-image-21".to_string()));

    // Unknown label
    assert_eq!(family_for_label(reg, "totally-unknown"), None);
}

// -----------------------------------------------------------------------------
// model_folder
// -----------------------------------------------------------------------------

#[test]
fn model_folder_handles_gguf_and_loader() {
    let reg = registry();
    // Unknown family -> checkpoints
    assert_eq!(model_folder(reg, None, "model.safetensors"), "checkpoints");

    // .gguf always diffusion_models
    assert_eq!(model_folder(reg, Some("flux1"), "model.gguf"), "diffusion_models");

    // Known family with Unet loader -> diffusion_models
    assert_eq!(model_folder(reg, Some("flux1"), "model.safetensors"), "diffusion_models");

    // Known family without Unet loader -> checkpoints
    assert_eq!(model_folder(reg, Some("sdxl"), "model.safetensors"), "checkpoints");
}

// -----------------------------------------------------------------------------
// parse_hf_list
// -----------------------------------------------------------------------------

#[test]
fn parse_hf_list_only_weights_and_skips_wrong_kind() {
    let reg = registry();
    let base = "https://huggingface.co";
    let list = json!([
        {
            "id": "user/model1",
            "author": "author",
            "likes": 10,
            "downloads": 100,
            "trendingScore": 1.0,
            "createdAt": "2025-01-01",
            "tags": ["text-to-image", "base_model:SDXL"],
            "cardData": {"license": "mit"},
            "gated": false,
            "siblings": [
                {"rfilename": "model.safetensors"},
                {"rfilename": "model.bin"},
                {"rfilename": "vae.safetensors"},
                {"rfilename": "notes.txt"}
            ]
        },
        {
            "id": "user/lora1",
            "tags": ["lora"],
            "siblings": [{"rfilename": "style.safetensors"}]
        }
    ]);

    let models = parse_hf_list(reg, base, &list, Kind::Model);
    assert_eq!(models.len(), 1);
    let m = &models[0];
    assert_eq!(m.files.len(), 2); // model.safetensors and vae.safetensors, ignoring .bin and .txt
    assert!(m.files.iter().all(|f| f.name.ends_with(".safetensors")));

    let loras = parse_hf_list(reg, base, &list, Kind::Lora);
    assert_eq!(loras.len(), 1);
    let l = &loras[0];
    assert_eq!(l.files.len(), 1);
    assert_eq!(l.files[0].folder, "loras");
}

#[test]
fn parse_hf_list_handles_empty_and_malformed() {
    let reg = registry();
    assert_eq!(parse_hf_list(reg, "", &Value::Null, Kind::Model), Vec::new());
    assert_eq!(parse_hf_list(reg, "", &json!("not array"), Kind::Model), Vec::new());
    assert_eq!(parse_hf_list(reg, "", &json!([]), Kind::Model), Vec::new());

    // Entry with no `id` is skipped
    let list = json!([{}]);
    assert_eq!(parse_hf_list(reg, "https://huggingface.co", &list, Kind::Model), Vec::new());

    // Entry with only `id` (no siblings) still produces an item with no files.
    let list_with_id = json!([{"id": "user/model"}]);
    let items = parse_hf_list(reg, "https://huggingface.co", &list_with_id, Kind::Model);
    assert_eq!(items.len(), 1);
    assert!(items[0].files.is_empty());
}

// -----------------------------------------------------------------------------
// parse_hf_tree
// -----------------------------------------------------------------------------

#[test]
fn parse_hf_tree_only_lfs_entries() {
    let tree = json!([
        {"path": "model.safetensors", "size": 123, "lfs": {"oid": "abc", "size": 123}},
        {"path": "model.bin", "size": 456, "lfs": null},
        {"path": "vae.safetensors", "lfs": {"oid": "def", "size": 789}, "size": 789},
        {"path": "README.md", "size": 10}
    ]);

    let parsed = parse_hf_tree(&tree);
    assert_eq!(parsed.len(), 2);
    assert_eq!(parsed.get("model.safetensors"), Some(&(123, "abc".to_string())));
    assert_eq!(parsed.get("vae.safetensors"), Some(&(789, "def".to_string())));
    assert!(!parsed.contains_key("model.bin"));
    assert!(!parsed.contains_key("README.md"));
}

// -----------------------------------------------------------------------------
// parse_civitai
// -----------------------------------------------------------------------------

#[test]
fn parse_civitai_filters_nsfw_pickles_and_commercial() {
    let reg = registry();
    let civitai = json!({
        "items": [
            {
                "id": 101,
                "name": "Juggernaut XL",
                "type": "Checkpoint",
                "nsfw": false,
                "allowCommercialUse": ["Image", "RentCivit"],
                "creator": {"username": "KandooAI"},
                "stats": {"downloadCount": 1000, "thumbsUpCount": 50},
                "tags": ["photorealistic"],
                "modelVersions": [{
                    "id": 5,
                    "baseModel": "SDXL 1.0",
                    "publishedAt": "2025-01-01",
                    "files": [{
                        "name": "juggernautXL_v9.safetensors",
                        "sizeKB": 6775430.0,
                        "type": "Model",
                        "hashes": {"SHA256": "AB".repeat(32)},
                        "downloadUrl": "https://civitai.com/api/download/models/5"
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
                    "id": 6,
                    "baseModel": "Pony",
                    "files": [{
                        "name": "style.safetensors",
                        "sizeKB": 100.0,
                        "hashes": {"SHA256": "CD".repeat(32)},
                        "downloadUrl": "https://civitai.com/api/download/models/6"
                    }]
                }]
            },
            {
                "id": 103,
                "name": "Hidden",
                "type": "Checkpoint",
                "nsfw": true,
                "modelVersions": [{
                    "id": 7,
                    "baseModel": "SD 1.5",
                    "files": [{"name": "x.safetensors", "downloadUrl": "https://civitai.com/api/download/models/7"}]
                }]
            },
            {
                "id": 104,
                "name": "Pickle",
                "type": "Checkpoint",
                "modelVersions": [{
                    "id": 8,
                    "baseModel": "SD 1.5",
                    "files": [{"name": "x.ckpt", "downloadUrl": "https://civitai.com/api/download/models/8"}]
                }]
            }
        ]
    });

    let items = parse_civitai(reg, &civitai);
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

// -----------------------------------------------------------------------------
// parse_manager
// -----------------------------------------------------------------------------

#[test]
fn parse_manager_parses_sizes_and_rejects_non_weights() {
    let reg = registry();
    let v = json!({
        "models": [
            {
                "name": "Qwen-Image Diffusion",
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
            },
            {
                "name": "Some LoRA",
                "type": "lora",
                "base": "SDXL",
                "save_path": "default",
                "filename": "lora.safetensors",
                "url": "https://x/lora.safetensors",
                "size": "1.5GB"
            },
            {
                "name": "Bad filename",
                "type": "Checkpoint",
                "base": "SDXL",
                "save_path": "default",
                "filename": "model.ckpt",
                "url": "https://x/model.ckpt",
                "size": "5GB"
            },
            {
                "name": "Huggingface placeholder",
                "type": "checkpoint",
                "base": "SDXL",
                "save_path": "default",
                "filename": "<huggingface>",
                "url": "https://huggingface.co/user/repo/resolve/main/model.safetensors",
                "size": "4.2GB"
            }
        ]
    });

    let items = parse_manager(reg, &v);
    // Only safetensors/gguf entries are kept; .pth and .ckpt excluded
    // We expect: Qwen (diffusion_model), Some LoRA (lora), Huggingface placeholder (checkpoint)
    assert_eq!(items.len(), 3);

    let qwen = items.iter().find(|i| i.title == "Qwen-Image Diffusion").unwrap();
    assert_eq!(qwen.family.as_deref(), Some("qwen-image"));
    assert_eq!(qwen.files[0].folder, "diffusion_models/qwen-image");
    assert_eq!(qwen.files[0].approx_bytes, Some(9_780_000_000));

    let lora = items.iter().find(|i| i.title == "Some LoRA").unwrap();
    assert_eq!(lora.kind, Some(Kind::Lora));
    assert_eq!(lora.files[0].folder, "loras"); // default save_path for lora
    assert_eq!(lora.files[0].approx_bytes, Some(1_500_000_000));

    let hf_placeholder = items.iter().find(|i| i.title == "Huggingface placeholder").unwrap();
    assert_eq!(hf_placeholder.files[0].name, "model.safetensors"); // from url
    assert_eq!(hf_placeholder.files[0].approx_bytes, Some(4_200_000_000));
}

// -----------------------------------------------------------------------------
// parse_template_index
// -----------------------------------------------------------------------------

#[test]
fn parse_template_index_filters_out_api_and_video() {
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
                },
                {
                    "name": "image_closed",
                    "title": "Closed",
                    "mediaType": "image",
                    "tags": [],
                    "openSource": false
                },
                {
                    "name": "video_wan",
                    "title": "Video",
                    "mediaType": "video",
                    "tags": [],
                    "openSource": true
                }
            ]
        },
        {
            "moduleName": "default",
            "category": "Foundation",
            "title": "Video",
            "type": "video",
            "templates": [
                {"name": "video_wan", "mediaType": "video", "openSource": true}
            ]
        }
    ]);

    let parsed = parse_template_index(&idx);
    // Only the first entry (image, openSource true, no API tag) should survive
    assert_eq!(parsed.len(), 1);
    assert_eq!(parsed[0].name, "image_qwen_image");
    assert_eq!(parsed[0].size, 31782757990);
}

// -----------------------------------------------------------------------------
// template_files
// -----------------------------------------------------------------------------

#[test]
fn template_files_deduplicates_and_requires_sha256() {
    let t = json!({
        "nodes": [
            {
                "type": "UNETLoader",
                "properties": {
                    "models": [
                        {
                            "name": "model.safetensors",
                            "url": "https://example.com/model.safetensors",
                            "directory": "diffusion_models",
                            "hash": "AA",
                            "hash_type": "SHA256"
                        },
                        {
                            "name": "model.safetensors",
                            "url": "https://example.com/model.safetensors",
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
                            "name": "vae.safetensors",
                            "url": "https://example.com/vae.safetensors",
                            "directory": "vae",
                            "hash": "BB",
                            "hash_type": "md5"
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
                            "type": "VAELoader",
                            "properties": {
                                "models": [
                                    {
                                        "name": "vae.safetensors",
                                        "url": "https://example.com/vae.safetensors",
                                        "directory": "vae",
                                        "hash": "CC",
                                        "hash_type": "SHA256"
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
    // Deduplication: first occurrence in top-level wins. For vae, the top-level entry has hash_type=md5,
    // so its sha256 is None. The subgraph entry is skipped entirely because dedup already exists.
    assert_eq!(files.len(), 2);

    let model = files.iter().find(|f| f.name == "model.safetensors").unwrap();
    assert_eq!(model.folder, "diffusion_models");
    assert_eq!(model.sha256.as_deref(), Some("aa")); // lowercase

    let vae = files.iter().find(|f| f.name == "vae.safetensors").unwrap();
    assert_eq!(vae.folder, "vae");
    assert_eq!(vae.sha256.as_deref(), None); // first occurrence had md5 hash_type => no sha256
}

// -----------------------------------------------------------------------------
// template_item
// -----------------------------------------------------------------------------

#[test]
fn template_item_spreads_approx_bytes_and_preview_url() {
    let reg = registry();
    let base = "https://example.com/templates";

    // With size and files without sizes, approx_bytes should be distributed.
    let e = TemplateEntry { name: "image_test".to_string(), title: "Test Template".to_string(), size: 30_000_000_000, ..Default::default() };
    let files = vec![
        CatalogFile { name: "a.safetensors".into(), folder: "checkpoints".into(), ..Default::default() },
        CatalogFile { name: "b.safetensors".into(), folder: "vae".into(), ..Default::default() },
        CatalogFile { name: "c.safetensors".into(), folder: "loras".into(), ..Default::default() },
    ];
    let item = template_item(reg, &e, files, base);
    assert_eq!(item.files.len(), 3);
    let expected = 30_000_000_000 / 3;
    for f in &item.files {
        assert_eq!(f.approx_bytes, Some(expected));
    }
    assert_eq!(item.preview, Some(format!("{}/{}-1.webp", base, "image_test")));
    assert_eq!(item.page_url, "https://github.com/Comfy-Org/workflow_templates/blob/main/templates/image_test.json");
    assert_eq!(item.template, Some("image_test".to_string()));
    assert_eq!(item.source, Some(Source::Template));
    assert_eq!(item.kind, Some(Kind::Model));

    // Basic controls when family unknown
    assert!(item.basic_controls());

    // If size is 0, no approx bytes
    let e2 = TemplateEntry { name: "image_zero".into(), size: 0, ..Default::default() };
    let files2 = vec![CatalogFile { name: "a.safetensors".into(), folder: "checkpoints".into(), ..Default::default() }];
    let item2 = template_item(reg, &e2, files2, base);
    assert_eq!(item2.files[0].approx_bytes, None);
}

// -----------------------------------------------------------------------------
// urlencode
// -----------------------------------------------------------------------------

#[test]
fn urlencode_encodes_special_chars_and_spaces() {
    assert_eq!(urlencode("hello world"), "hello%20world");
    assert_eq!(urlencode("a/b?c=d&e+f"), "a%2Fb%3Fc%3Dd%26e%2Bf");
    assert_eq!(urlencode("héllo"), "h%C3%A9llo");
    assert_eq!(urlencode("safe_chars-.~"), "safe_chars-.~");
    assert_eq!(urlencode(""), "");
}

// -----------------------------------------------------------------------------
// urls
// -----------------------------------------------------------------------------

#[test]
fn urls_respect_sources_and_views() {
    let reg = registry();
    let cfg = Config::default();

    // Family "pony" should include Civitai baseModels and HF filter
    let q = Query { view: View::Family("pony".to_string()), kind: Kind::Lora, search: "ink style".to_string(), sources: vec![] };
    let u = urls(&cfg, reg, &q);
    let civitai_url = u.iter().find(|(s, _)| *s == Source::Civitai).map(|(_, url)| url).unwrap();
    assert!(civitai_url.contains("baseModels=Pony"));
    assert!(civitai_url.contains("types=LORA"));
    assert!(civitai_url.contains("query=ink%20style"));

    let hf_url = u.iter().find(|(s, _)| *s == Source::HuggingFace).map(|(_, url)| url).unwrap();
    assert!(hf_url.contains("filter=lora"));
    assert!(hf_url.contains("search=ink%20style"));

    // Template should be absent because kind is Lora
    assert!(!u.iter().any(|(s, _)| *s == Source::Template));

    // Only specific sources: use a view that includes Community (not Installed/New/Trending)
    let q2 = Query { view: View::FitsGpu(12.0), kind: Kind::Model, search: String::new(), sources: vec![Source::Civitai, Source::Community] };
    let u2 = urls(&cfg, reg, &q2);
    assert!(u2.iter().any(|(s, _)| *s == Source::Civitai));
    assert!(u2.iter().any(|(s, _)| *s == Source::Community));
    assert!(!u2.iter().any(|(s, _)| *s == Source::HuggingFace || *s == Source::Template));

    // View::Installed excludes HF, Civitai, Template, and Community
    let q3 = Query { view: View::Installed, kind: Kind::Model, search: String::new(), sources: vec![] };
    let u3 = urls(&cfg, reg, &q3);
    assert!(!u3.iter().any(|(s, _)| *s == Source::HuggingFace || *s == Source::Civitai || *s == Source::Template));
    assert!(!u3.iter().any(|(s, _)| *s == Source::Community));
}

// -----------------------------------------------------------------------------
// items
// -----------------------------------------------------------------------------

#[test]
fn items_filters_by_search_family_and_gpu() {
    let reg = registry();
    let cfg = Config::default();
    let bodies = vec![
        (
            Source::HuggingFace,
            json!([
                {
                    "id": "user/model1",
                    "author": "a",
                    "likes": 100,
                    "downloads": 500,
                    "trendingScore": 10.0,
                    "createdAt": "2025-01-01",
                    "tags": ["text-to-image", "base_model:SDXL"],
                    "siblings": [{"rfilename": "model.safetensors"}]
                },
                {
                    "id": "user/lora1",
                    "tags": ["lora"],
                    "siblings": [{"rfilename": "lora.safetensors"}]
                }
            ]),
        ),
        (
            Source::Civitai,
            json!({
                "items": [
                    {
                        "id": 101,
                        "name": "Big Model",
                        "type": "Checkpoint",
                        "nsfw": false,
                        "allowCommercialUse": ["Image"],
                        "creator": {"username": "a"},
                        "stats": {"downloadCount": 100, "thumbsUpCount": 10},
                        "tags": [],
                        "modelVersions": [{
                            "id": 5,
                            "baseModel": "SDXL 1.0",
                            "publishedAt": "2025-02-01",
                            "files": [{
                                "name": "big.safetensors",
                                "sizeKB": 20_000_000.0,
                                "hashes": {"SHA256": "AB".repeat(32)},
                                "downloadUrl": "https://civitai.com/api/download/models/5"
                            }]
                        }]
                    }
                ]
            }),
        ),
    ];

    // Search "big" should only return Civitai item
    let q_search = Query { view: View::New, kind: Kind::Model, search: "big".to_string(), sources: vec![] };
    let res = items(reg, &cfg, &q_search, &bodies);
    assert_eq!(res.len(), 1);
    assert_eq!(res[0].title, "Big Model");

    // Family "sdxl" should include the HF model (base_model:SDXL) and Civitai (SDXL 1.0)
    let q_family = Query { view: View::Family("sdxl".to_string()), kind: Kind::Model, search: String::new(), sources: vec![] };
    let res = items(reg, &cfg, &q_family, &bodies);
    // HF model has family sdxl via base_model tag, Civitai has family sdxl via baseModel
    assert_eq!(res.len(), 2);

    // FitsGpu threshold: Big Model should have vram > 12 GB? Let's compute:
    // sizeKB = 20_000_000 KB = 20_000_000 * 1024 bytes = 20_480_000_000 bytes ≈ 20.48 GB
    // vram = ceil(20.48 * 1.2 + 1.5) = ceil(24.576 + 1.5) = ceil(26.076) = 27 GB
    let q_gpu = Query { view: View::FitsGpu(12.0), kind: Kind::Model, search: String::new(), sources: vec![] };
    let res = items(reg, &cfg, &q_gpu, &bodies);
    // Both HF and Civitai? HF model has no size, so vram_gb None, so excluded. So only maybe none.
    assert!(res.is_empty());
}

// -----------------------------------------------------------------------------
// Cache
// -----------------------------------------------------------------------------

#[test]
fn cache_roundtrip_and_fetch() {
    let dir = unique_temp_dir("li-browser-cache");
    let cache = Cache::new(dir.clone());
    let url = "https://example.com/data.json";
    let body = b"[1,2,3]".to_vec();

    // put then get
    cache.put(url, &body).unwrap();
    let (cached, _age) = cache.get(url).unwrap();
    assert_eq!(cached, body);

    // fetch online (with fake net that returns body)
    let net_online = FakeNet { bodies: BTreeMap::from([(url.to_string(), Ok(body.clone()))]), head: None };
    let (fetched, from_cache) = cache.fetch(&net_online, url, true).unwrap();
    assert_eq!(fetched, body);
    assert!(!from_cache);

    // fetch offline (net that would fail) but cached -> should succeed from cache
    let net_offline = FakeNet { bodies: BTreeMap::new(), head: None };
    let (fetched_offline, from_cache_offline) = cache.fetch(&net_offline, url, false).unwrap();
    assert_eq!(fetched_offline, body);
    assert!(from_cache_offline);

    // fetch uncached offline -> error
    let new_url = "https://example.com/other.json";
    assert!(cache.fetch(&net_offline, new_url, false).is_err());

    // fetch online with net error but cache exists -> returns cache
    let net_error = FakeNet { bodies: BTreeMap::from([(url.to_string(), Err("network failure".to_string()))]), head: None };
    let (cached_from_error, from_cache2) = cache.fetch(&net_error, url, true).unwrap();
    assert_eq!(cached_from_error, body);
    assert!(from_cache2);

    cleanup(&dir);
}

// -----------------------------------------------------------------------------
// fetch_template
// -----------------------------------------------------------------------------

#[test]
fn fetch_template_rejects_bad_names() {
    let dir = unique_temp_dir("li-browser-template");
    let cfg = Config { templates: "https://example.com/templates".to_string(), ..Default::default() };
    let net = FakeNet { bodies: BTreeMap::new(), head: None };

    for bad in ["bad/name", "../bad", "ok/../bad", "with spaces"] {
        let result = fetch_template(&cfg, &net, bad);
        assert!(result.is_err(), "expected error for name {bad:?}");
        // error message should mention "bad template name"
        // (not checked to allow variation)
    }
    cleanup(&dir);
}

// -----------------------------------------------------------------------------
// plan
// -----------------------------------------------------------------------------

#[test]
fn plan_refuses_unverified_and_bad_host() {
    let cfg = Config::default();
    let info = ObjectInfo(json!({}));

    // For HF unverified file: we need to provide a tree response so that after fetching tree,
    // the file's size/hash remain None and the later check fails.
    let tree_url = "https://huggingface.co/api/models/user/repo/tree/main?recursive=true";
    let empty_tree = json!([]).to_string().into_bytes();
    let net_with_tree = FakeNet { bodies: BTreeMap::from([(tree_url.to_string(), Ok(empty_tree))]), head: None };

    let item = Item { id: "test".to_string(), kind: Some(Kind::Model), ..Default::default() };

    // No size, no sha, URL from allowed host -> should fail with "no published size and SHA-256"
    let unverified = CatalogFile {
        name: "model.safetensors".to_string(),
        folder: "checkpoints".to_string(),
        url: "https://huggingface.co/user/repo/resolve/main/model.safetensors".to_string(),
        ..Default::default()
    };
    let plan_res = plan(&item, &[unverified], None, &info, &cfg, &net_with_tree);
    assert!(plan_res.is_err());
    let err = plan_res.unwrap_err().to_string();
    assert!(err.contains("no published size and SHA-256"), "got: {err}");

    // Bad host (even with size and sha) -> should fail with "unexpected host"
    let net_no_bodies = FakeNet { bodies: BTreeMap::new(), head: None };
    let bad_host = CatalogFile {
        name: "model.safetensors".to_string(),
        folder: "checkpoints".to_string(),
        url: "https://evil.com/model.safetensors".to_string(),
        bytes: Some(100),
        sha256: Some("a".repeat(64)),
        ..Default::default()
    };
    let plan_res = plan(&item, &[bad_host], None, &info, &cfg, &net_no_bodies);
    assert!(plan_res.is_err());
    let err = plan_res.unwrap_err().to_string();
    assert!(err.contains("unexpected host"), "got: {err}");

    // Size mismatch for Civitai style (approx vs actual) -> should fail with "not installing"
    let civitai_file = CatalogFile {
        name: "model.safetensors".to_string(),
        folder: "checkpoints".to_string(),
        url: "https://civitai.com/api/download/models/123".to_string(),
        approx_bytes: Some(10_000_000),
        sha256: Some("a".repeat(64)),
        ..Default::default()
    };
    let net_wrong_head = FakeNet { bodies: BTreeMap::new(), head: Some(1000) };
    let plan_res = plan(&item, &[civitai_file], None, &info, &cfg, &net_wrong_head);
    assert!(plan_res.is_err());
    let err = plan_res.unwrap_err().to_string();
    assert!(err.contains("not installing"), "got: {err}");
}

#[test]
fn plan_skips_installed_and_uses_hf_tree() {
    let cfg = Config::default();
    let info = ObjectInfo(json!({
        "CheckpointLoaderSimple": {
            "input": {
                "required": {
                    "ckpt_name": [["already_installed.safetensors"]]
                }
            }
        }
    }));

    let sha = "a".repeat(64);
    let tree = json!([
        {
            "type": "file",
            "path": "model.safetensors",
            "size": 123456,
            "lfs": {"oid": sha, "size": 123456}
        }
    ]);
    let net = FakeNet {
        bodies: BTreeMap::from([("https://huggingface.co/api/models/user/repo/tree/main?recursive=true".to_string(), Ok(serde_json::to_vec(&tree).unwrap()))]),
        head: None,
    };

    let item = Item { id: "test".to_string(), kind: Some(Kind::Model), ..Default::default() };

    let files = vec![
        CatalogFile {
            name: "already_installed.safetensors".to_string(),
            folder: "checkpoints".to_string(),
            url: "https://huggingface.co/user/repo/resolve/main/already_installed.safetensors".to_string(),
            ..Default::default()
        },
        CatalogFile {
            name: "model.safetensors".to_string(),
            folder: "checkpoints".to_string(),
            url: "https://huggingface.co/user/repo/resolve/main/model.safetensors".to_string(),
            ..Default::default()
        },
    ];

    let p = plan(&item, &files, None, &info, &cfg, &net).unwrap();
    assert_eq!(p.present, vec!["already_installed.safetensors"]);
    assert_eq!(p.files.len(), 1);
    assert_eq!(p.files[0].name, "model.safetensors");
    assert_eq!(p.files[0].bytes, 123456);
    assert_eq!(p.files[0].sha256, sha);
}

// -----------------------------------------------------------------------------
// End of tests
// -----------------------------------------------------------------------------
