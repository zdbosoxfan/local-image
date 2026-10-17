use li_ai::browser::{Cache, CatalogFile, Config, Kind, Query, Source, View, items, lora_file_name};
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
