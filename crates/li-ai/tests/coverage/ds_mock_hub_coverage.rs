use li_ai::mock_hub::{Reply, fixtures_dir, handle, handle_cloud, sha, synth};
use serde_json::json;

#[test]
fn synth_is_deterministic_and_length_matches() {
    let a = synth("foo.safetensors");
    let b = synth("foo.safetensors");
    assert_eq!(a, b);
    assert!(a.len() >= 10);
    let header_len = u64::from_le_bytes(a[..8].try_into().unwrap()) as usize;
    assert_eq!(a.len(), 8 + header_len + 2);
}

#[test]
fn synth_header_has_metadata_and_marker() {
    let bytes = synth("model.safetensors");
    let header_len = u64::from_le_bytes(bytes[..8].try_into().unwrap()) as usize;
    let header_bytes = &bytes[8..8 + header_len];
    let v: serde_json::Value = serde_json::from_slice(header_bytes).unwrap();
    assert_eq!(v["__metadata__"]["mock"], "model.safetensors");
    assert_eq!(v["marker"]["dtype"], "F16");
    assert_eq!(v["marker"]["shape"][0], 1);
    assert_eq!(v["marker"]["data_offsets"][1], 2);
}

#[test]
fn sha_known_vectors() {
    assert_eq!(sha(b""), "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855");
    assert_eq!(sha(b"abc"), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
    assert_eq!(sha("hello".as_bytes()), "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824");
}

#[test]
fn handle_manager_list_returns_two_models() {
    let reply = handle("example.test", "/manager/model-list.json").expect("manager list should exist");
    let Reply::Json(v) = reply else {
        panic!("expected Json reply");
    };
    let models = v["models"].as_array().expect("models array");
    assert_eq!(models.len(), 2);
    assert!(models.iter().any(|m| m["type"] == "diffusion_model"));
    assert!(models.iter().any(|m| m["type"] == "upscale"));

    let diffusion = models.iter().find(|m| m["type"] == "diffusion_model").unwrap();
    assert!(diffusion["url"].as_str().unwrap().contains("example.test"));
}

#[test]
fn handle_templates_serves_json_and_rewrites_hf_links() {
    let base = fixtures_dir();
    let json_file = std::fs::read_dir(&base)
        .expect("fixtures dir")
        .filter_map(Result::ok)
        .map(|e| e.path())
        .find(|p| p.extension().is_some_and(|ext| ext == "json"))
        .expect("at least one template json fixture");

    let name = json_file.file_name().unwrap().to_str().unwrap();
    let reply = handle("example.test", &format!("/templates/{name}")).expect("template should serve");
    let Reply::Bytes(bytes, content_type) = reply else {
        panic!("expected Bytes reply");
    };
    assert_eq!(content_type, "application/json");

    let text = String::from_utf8(bytes).unwrap();
    assert!(text.contains("http://example.test/hf/"));
    assert!(!text.contains("https://huggingface.co/"));

    let original = std::fs::read_to_string(&json_file).unwrap();
    if original.contains("huggingface.co") {
        assert!(!text.contains("huggingface.co"));
    }
}

#[test]
fn handle_hf_api_models_filters_lora() {
    let reply = handle("example.test", "/hf/api/models?filter=lora").expect("hf api models");
    let Reply::Json(v) = reply else {
        panic!("expected Json reply");
    };
    let arr = v.as_array().expect("array");
    assert_eq!(arr.len(), 2);
    assert!(arr.iter().all(|m| m["tags"].as_array().unwrap().iter().any(|t| t == "lora")));
}

#[test]
fn handle_hf_api_models_search_is_case_insensitive() {
    let reply = handle("example.test", "/hf/api/models?search=FLUX").expect("hf search");
    let Reply::Json(v) = reply else {
        panic!("expected Json reply");
    };
    let arr = v.as_array().expect("array");
    assert_eq!(arr.len(), 1);
    assert_eq!(arr[0]["id"], "mock-labs/FLUX.1-mock-dev");
}

#[test]
fn handle_hf_tree_returns_entries_for_repo() {
    let reply = handle("example.test", "/hf/api/models/mock-org/sdxl-filmic/tree/main").expect("tree");
    let Reply::Json(v) = reply else {
        panic!("expected Json reply");
    };
    let entries = v.as_array().expect("array");
    assert!(!entries.is_empty());

    for e in entries {
        assert_eq!(e["type"], "file");
        assert!(!e["path"].as_str().unwrap().is_empty());
        assert!(e["size"].as_u64().unwrap() > 0);
    }
    assert!(entries.iter().any(|e| e["path"].as_str().unwrap().contains("sdxlFilmic")));
}

#[test]
fn handle_hf_file_serves_exact_synth_bytes() {
    let name = "sdxlFilmic_v2.safetensors";
    let url = format!("/hf/mock-org/sdxl-filmic/resolve/main/{name}");
    let reply = handle("example.test", &url).expect("file should download");
    let Reply::Bytes(bytes, content_type) = reply else {
        panic!("expected Bytes reply");
    };
    assert_eq!(content_type, "application/octet-stream");
    assert_eq!(bytes, synth(name));
}

#[test]
fn handle_hf_gated_repo_returns_403() {
    let reply = handle("example.test", "/hf/mock-labs/FLUX.1-mock-dev/resolve/main/flux1-mock-dev.safetensors");
    assert!(matches!(reply, Some(Reply::Status(403))));
}

#[test]
fn handle_civitai_models_filters_lora() {
    let reply = handle("example.test", "/civitai/api/v1/models?types=LORA").expect("civitai models");
    let Reply::Json(v) = reply else {
        panic!("expected Json reply");
    };
    let items = v["items"].as_array().expect("items array");
    assert_eq!(items.len(), 2);
    assert!(items.iter().all(|m| m["type"] == "LORA"));
}

#[test]
fn handle_civitai_models_search_is_case_insensitive() {
    let reply = handle("example.test", "/civitai/api/v1/models?query=InK").expect("civitai search");
    let Reply::Json(v) = reply else {
        panic!("expected Json reply");
    };
    let items = v["items"].as_array().expect("items array");
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["name"], "Ink Wash (Pony)");
}

#[test]
fn handle_civitai_download_redirects_to_cdn() {
    let reply = handle("example.test", "/civitai/api/download/models/501").expect("download redirect");
    assert!(matches!(
        reply,
        Reply::Redirect(url) if url == "http://example.test/cdn/mockernautXL_v1.safetensors"
    ));
}

#[test]
fn handle_cdn_serves_exact_synth_bytes() {
    let name = "pony_inkwash.safetensors";
    let reply = handle("example.test", &format!("/cdn/{name}")).expect("cdn file");
    let Reply::Bytes(bytes, content_type) = reply else {
        panic!("expected Bytes reply");
    };
    assert_eq!(content_type, "application/octet-stream");
    assert_eq!(bytes, synth(name));
}

#[test]
fn handle_unknown_path_returns_none() {
    assert!(handle("example.test", "/not/here").is_none());
}

#[test]
fn cloud_missing_api_key_returns_401() {
    let reply = handle_cloud("example.test", "POST", "/cloud/openai/v1/images/generations", None, b"{}");
    assert!(matches!(reply, Some(Reply::JsonStatus(401, _))));
}

#[test]
fn cloud_bad_api_key_returns_401() {
    let reply = handle_cloud("example.test", "POST", "/cloud/openai/v1/images/generations", Some("bad-key"), b"{}");
    assert!(matches!(reply, Some(Reply::JsonStatus(401, _))));
}

#[test]
fn cloud_openai_generations_returns_requested_count_and_png() {
    let body = json!({"prompt":"hello","n":2,"size":"4x4"}).to_string();
    let reply = handle_cloud("example.test", "POST", "/cloud/openai/v1/images/generations", Some("good-key"), body.as_bytes());
    let Some(Reply::Json(v)) = reply else {
        panic!("expected Json reply");
    };
    let data = v["data"].as_array().expect("data array");
    assert_eq!(data.len(), 2);
    for item in data {
        let b64 = item["b64_json"].as_str().expect("b64 json string");
        assert!(b64.starts_with("iVBOR"), "PNG base64 should start with iVBOR");
    }
}

#[test]
fn cloud_google_generate_content_returns_image_part() {
    let body = json!({"contents":[{"parts":[{"text":"hello"}]}]}).to_string();
    let reply = handle_cloud("example.test", "POST", "/cloud/google/v1beta/models/gemini:generateContent", Some("good-key"), body.as_bytes());
    let Some(Reply::Json(v)) = reply else {
        panic!("expected Json reply");
    };
    let candidates = v["candidates"].as_array().expect("candidates");
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0]["finishReason"], "STOP");
    let parts = candidates[0]["content"]["parts"].as_array().expect("parts");
    assert!(parts.iter().any(|p| p["inlineData"]["mimeType"] == "image/png"));
    let data = parts.iter().find_map(|p| p["inlineData"]["data"].as_str()).expect("inline image data");
    assert!(data.starts_with("iVBOR"));
}

#[test]
fn cloud_stability_returns_png_bytes() {
    let reply = handle_cloud("example.test", "POST", "/cloud/stability/v2beta/stable-image/generate", Some("good-key"), b"");
    let Some(Reply::Bytes(bytes, content_type)) = reply else {
        panic!("expected Bytes reply");
    };
    assert_eq!(content_type, "image/png");
    assert!(bytes.starts_with(&[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]));
}

#[test]
fn cloud_bfl_roundtrip_persists_and_serves_sample() {
    let body = json!({"prompt":"test"}).to_string();
    let reply = handle_cloud("example.test", "POST", "/cloud/bfl/v1/flux-pro-1.1", Some("good-key"), body.as_bytes());
    let Some(Reply::Json(v)) = reply else {
        panic!("expected Json reply");
    };
    let id = v["id"].as_str().expect("job id").to_owned();
    assert!(!id.is_empty());

    let result_reply = handle_cloud("example.test", "GET", &format!("/cloud/bfl/v1/get_result?id={id}"), Some("good-key"), b"");
    let Some(Reply::Json(result)) = result_reply else {
        panic!("expected Json result");
    };
    assert_eq!(result["id"], id);
    assert_eq!(result["status"], "Ready");
    assert!(result["result"]["sample"].as_str().unwrap().contains(&id));

    let sample_reply = handle_cloud("example.test", "GET", &format!("/cloud/bfl/sample/{id}.png"), None, b"");
    let Some(Reply::Bytes(bytes, content_type)) = sample_reply else {
        panic!("expected sample Bytes");
    };
    assert_eq!(content_type, "image/png");
    assert!(bytes.starts_with(&[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]));
}

#[test]
fn cloud_unknown_path_with_valid_key_returns_not_found() {
    let reply = handle_cloud("example.test", "POST", "/cloud/unknown", Some("good-key"), b"{}");
    assert!(matches!(reply, Some(Reply::NotFound)));
}

#[test]
fn cloud_malformed_json_does_not_panic() {
    let reply = handle_cloud("example.test", "POST", "/cloud/openai/v1/images/generations", Some("good-key"), b"not json");
    assert!(reply.is_some());
}
