//! The mock server's stand-ins for the model sources: Hugging Face (`/hf`), Civitai (`/civitai`,
//! downloads redirected through `/cdn`), ComfyUI-Manager's list (`/manager/model-list.json`) and
//! the official workflow templates (`/templates`, served from `tests/fixtures/templates` with
//! their Hugging Face links pointed back at `/hf`). Files are tiny, deterministic safetensors so
//! installs can be verified end to end without the network.

use std::collections::BTreeSet;
use std::path::PathBuf;

use serde_json::{Value, json};
use sha2::Digest;

/// The template fixtures (index, workflows, thumbnails).
pub fn fixtures_dir() -> PathBuf {
    PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/templates"))
}

/// A small, valid safetensors file standing in for `path` (its name in the metadata).
pub fn synth(path: &str) -> Vec<u8> {
    let header = json!({ "__metadata__": { "mock": path }, "marker": { "dtype": "F16", "shape": [1], "data_offsets": [0, 2] } }).to_string();
    let mut h = header.into_bytes();
    while h.len() % 8 != 0 {
        h.push(b' ');
    }
    let mut out = (h.len() as u64).to_le_bytes().to_vec();
    out.extend(h);
    out.extend([0u8, 0]);
    out
}

pub fn sha(bytes: &[u8]) -> String {
    hex::encode(sha2::Sha256::digest(bytes))
}

const HF: &str = "https://huggingface.co/";

/// `(repo, path)` of every file the mock's Hugging Face serves a tree entry for.
fn known_files() -> BTreeSet<(String, String)> {
    let mut out = BTreeSet::new();
    let mut add_url = |url: &str| {
        if let Some(rest) = url.strip_prefix(HF)
            && let Some((repo, tail)) = rest.split_once("/resolve/")
            && let Some((_, path)) = tail.split_once('/')
        {
            out.insert((repo.to_owned(), path.split('?').next().unwrap_or(path).to_owned()));
        }
    };
    for e in std::fs::read_dir(fixtures_dir()).into_iter().flatten().flatten() {
        let p = e.path();
        if p.extension().is_some_and(|x| x == "json")
            && let Ok(v) = serde_json::from_slice::<Value>(&std::fs::read(&p).unwrap_or_default())
        {
            for f in crate::browser::template_files(&v) {
                add_url(&f.url);
            }
        }
    }
    for item in hf_models().as_array().into_iter().flatten() {
        let id = item["id"].as_str().unwrap_or("");
        for s in item["siblings"].as_array().into_iter().flatten() {
            if let Some(f) = s["rfilename"].as_str() {
                out.insert((id.to_owned(), f.to_owned()));
            }
        }
    }
    // Component files the families fetch from Hugging Face.
    for f in &crate::family::registry().families {
        for c in &f.components {
            if let Some(src) = c.source.as_deref().and_then(|s| s.strip_prefix("hf:")) {
                let mut parts = src.splitn(3, '/');
                if let (Some(o), Some(r), Some(path)) = (parts.next(), parts.next(), parts.next()) {
                    out.insert((format!("{o}/{r}"), path.to_owned()));
                }
            }
        }
    }
    out
}

fn hf_models() -> Value {
    json!([
        {"id": "mock-org/z-image-turbo-mini", "author": "mock-org", "likes": 940, "downloads": 120000, "trendingScore": 88, "createdAt": "2026-09-30T10:00:00Z",
         "tags": ["diffusers", "text-to-image", "base_model:Tongyi-MAI/Z-Image-Turbo", "license:apache-2.0"], "cardData": {"license": "apache-2.0"}, "gated": false,
         "siblings": [{"rfilename": "split_files/diffusion_models/z_image_turbo_mini.safetensors"}, {"rfilename": "README.md"}]},
        {"id": "mock-org/sdxl-filmic", "author": "mock-org", "likes": 310, "downloads": 54000, "trendingScore": 12, "createdAt": "2026-06-02T10:00:00Z",
         "tags": ["text-to-image", "stable-diffusion-xl", "license:openrail++"], "cardData": {"license": "openrail++"}, "gated": false,
         "siblings": [{"rfilename": "sdxlFilmic_v2.safetensors"}, {"rfilename": "sdxlFilmic_v2.ckpt"}]},
        {"id": "mock-labs/FLUX.1-mock-dev", "author": "mock-labs", "likes": 12000, "downloads": 2000000, "trendingScore": 40, "createdAt": "2025-08-01T10:00:00Z",
         "tags": ["text-to-image", "flux", "license:other"], "cardData": {"license": "other", "license_name": "flux-1-dev-non-commercial-license"}, "gated": "auto",
         "siblings": [{"rfilename": "flux1-mock-dev.safetensors"}]},
        {"id": "mock-org/qwen-ink-lora", "author": "mock-org", "likes": 75, "downloads": 3100, "trendingScore": 30, "createdAt": "2026-09-12T10:00:00Z",
         "tags": ["lora", "base_model:adapter:Qwen/Qwen-Image", "license:apache-2.0"], "cardData": {"license": "apache-2.0"}, "gated": false,
         "siblings": [{"rfilename": "qwen_ink.safetensors"}]},
        {"id": "mock-org/klein-sketch-lora", "author": "mock-org", "likes": 41, "downloads": 900, "trendingScore": 55, "createdAt": "2026-10-01T10:00:00Z",
         "tags": ["lora", "base_model:adapter:black-forest-labs/FLUX.2-klein-4B", "license:apache-2.0"], "cardData": {"license": "apache-2.0"}, "gated": false,
         "siblings": [{"rfilename": "klein4b_sketch.safetensors"}]}
    ])
}

fn civitai_file(host: &str, version: u64, name: &str, with_hash: bool) -> Value {
    let bytes = synth(name);
    let mut f = json!({"name": name, "sizeKB": bytes.len() as f64 / 1024.0, "type": "Model", "metadata": {"format": "SafeTensor"},
        "downloadUrl": format!("http://{host}/civitai/api/download/models/{version}")});
    if with_hash {
        f["hashes"] = json!({"SHA256": sha(&bytes).to_ascii_uppercase()});
    }
    f
}

fn civitai_models(host: &str) -> Vec<Value> {
    let img = |n: &str| format!("http://{host}/templates/{n}-1.webp");
    vec![
        json!({"id": 9001, "name": "Mockernaut XL", "type": "Checkpoint", "nsfw": false, "allowCommercialUse": ["Image", "RentCivit"],
            "creator": {"username": "mockmaker"}, "stats": {"downloadCount": 880000, "thumbsUpCount": 31000}, "tags": ["photorealistic", "base model"],
            "modelVersions": [{"id": 501, "baseModel": "SDXL 1.0", "publishedAt": "2026-08-20T00:00:00Z", "files": [civitai_file(host, 501, "mockernautXL_v1.safetensors", true)],
                "images": [{"url": img("image_sdxl_simple"), "nsfwLevel": 1, "type": "image"}]}]}),
        json!({"id": 9002, "name": "Ink Wash (Pony)", "type": "LORA", "nsfw": false, "allowCommercialUse": ["None"],
            "creator": {"username": "brushes"}, "stats": {"downloadCount": 12000, "thumbsUpCount": 900}, "tags": ["style"],
            "modelVersions": [{"id": 502, "baseModel": "Pony", "publishedAt": "2026-09-28T00:00:00Z", "files": [civitai_file(host, 502, "pony_inkwash.safetensors", true)],
                "images": [{"url": img("image_z_image"), "nsfwLevel": 1, "type": "image"}]}]}),
        json!({"id": 9003, "name": "Unverified Mix", "type": "Checkpoint", "nsfw": false,
            "creator": {"username": "anon"}, "stats": {"downloadCount": 10, "thumbsUpCount": 1},
            "modelVersions": [{"id": 503, "baseModel": "SD 1.5", "publishedAt": "2026-09-01T00:00:00Z", "files": [civitai_file(host, 503, "unverified_mix.safetensors", false)]}]}),
        json!({"id": 9004, "name": "Hidden", "type": "Checkpoint", "nsfw": true,
            "modelVersions": [{"id": 504, "baseModel": "SDXL 1.0", "files": [civitai_file(host, 504, "hidden.safetensors", true)]}]}),
        json!({"id": 9005, "name": "Qwen Edit Relight", "type": "LORA", "nsfw": false, "allowCommercialUse": ["Image"],
            "creator": {"username": "lightlab"}, "stats": {"downloadCount": 4400, "thumbsUpCount": 520}, "tags": ["lighting"],
            "modelVersions": [{"id": 505, "baseModel": "Qwen", "publishedAt": "2026-10-02T00:00:00Z", "files": [civitai_file(host, 505, "qwen_edit_relight.safetensors", true)],
                "images": [{"url": img("image_qwen_image_edit_2511"), "nsfwLevel": 1, "type": "image"}]}]})
    ]
}

fn civitai_name(version: u64) -> Option<&'static str> {
    Some(match version {
        501 => "mockernautXL_v1.safetensors",
        502 => "pony_inkwash.safetensors",
        503 => "unverified_mix.safetensors",
        504 => "hidden.safetensors",
        505 => "qwen_edit_relight.safetensors",
        _ => return None,
    })
}

fn manager_list(host: &str) -> Value {
    json!({"models": [
        {"name": "Qwen-Image Diffusion Model (bf16, mock)", "type": "diffusion_model", "base": "Qwen-Image", "save_path": "default", "description": "Qwen-Image for ComfyUI.",
         "reference": "https://huggingface.co/Comfy-Org/Qwen-Image_ComfyUI", "filename": "qwen_image_mock_bf16.safetensors",
         "url": format!("http://{host}/hf/mock-org/sdxl-filmic/resolve/main/qwen_image_mock_bf16.safetensors"), "size": "40.9GB"},
        {"name": "4x-UltraSharp", "type": "upscale", "base": "upscale", "save_path": "default", "filename": "4x-UltraSharp.pth", "url": "https://x/4x.pth", "size": "67MB"}
    ]})
}

fn query(url: &str, key: &str) -> Vec<String> {
    url.split_once('?')
        .map(|(_, q)| q)
        .unwrap_or("")
        .split('&')
        .filter_map(|kv| kv.split_once('='))
        .filter(|(k, _)| *k == key)
        .map(|(_, v)| v.replace("%20", " ").replace('+', " "))
        .collect()
}

pub enum Reply {
    Json(Value),
    Bytes(Vec<u8>, &'static str),
    Redirect(String),
    Status(u16),
    NotFound,
}

/// Handles the hub paths (`None` for anything else).
pub fn handle(host: &str, url: &str) -> Option<Reply> {
    let path = url.split('?').next().unwrap_or("");
    if path == "/manager/model-list.json" {
        return Some(Reply::Json(manager_list(host)));
    }
    if let Some(rest) = path.strip_prefix("/templates/") {
        let file = fixtures_dir().join(rest.replace("..", ""));
        let Ok(bytes) = std::fs::read(&file) else { return Some(Reply::NotFound) };
        if rest.ends_with(".webp") {
            return Some(Reply::Bytes(bytes, "image/webp"));
        }
        let text = String::from_utf8_lossy(&bytes).replace(HF, &format!("http://{host}/hf/"));
        return Some(Reply::Bytes(text.into_bytes(), "application/json"));
    }
    if path == "/hf/api/models" {
        let filters = query(url, "filter");
        let search = query(url, "search").first().map(|s| s.to_ascii_lowercase()).unwrap_or_default();
        let lora = filters.iter().any(|f| f == "lora");
        let list: Vec<Value> = hf_models()
            .as_array()
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .filter(|m| m["tags"].as_array().is_some_and(|t| t.iter().any(|x| x == "lora")) == lora)
            .filter(|m| search.is_empty() || m["id"].as_str().unwrap_or("").to_ascii_lowercase().contains(&search))
            .collect();
        return Some(Reply::Json(Value::Array(list)));
    }
    if let Some(rest) = path.strip_prefix("/hf/api/models/") {
        let repo = rest.split("/tree/").next().unwrap_or("");
        let entries: Vec<Value> = known_files()
            .into_iter()
            .filter(|(r, _)| r == repo)
            .map(|(_, p)| {
                let b = synth(&p);
                json!({"type": "file", "path": p, "size": b.len(), "lfs": {"oid": sha(&b), "size": b.len()}})
            })
            .collect();
        return Some(if entries.is_empty() { Reply::NotFound } else { Reply::Json(Value::Array(entries)) });
    }
    if let Some(rest) = path.strip_prefix("/hf/") {
        let (_, tail) = rest.split_once("/resolve/")?;
        let (_, file) = tail.split_once('/')?;
        if file.contains("FLUX.1-mock") || rest.starts_with("mock-labs/") {
            // A gated repository: the licence has to be accepted on the website first.
            return Some(Reply::Status(403));
        }
        return Some(Reply::Bytes(synth(file), "application/octet-stream"));
    }
    if path == "/civitai/api/v1/models" {
        let types = query(url, "types");
        let bases = query(url, "baseModels");
        let search = query(url, "query").first().map(|s| s.to_ascii_lowercase()).unwrap_or_default();
        let items: Vec<Value> = civitai_models(host)
            .into_iter()
            .filter(|m| types.is_empty() || types.iter().any(|t| m["type"] == json!(t)))
            .filter(|m| bases.is_empty() || bases.iter().any(|b| m["modelVersions"][0]["baseModel"] == json!(b)))
            .filter(|m| search.is_empty() || m["name"].as_str().unwrap_or("").to_ascii_lowercase().contains(&search))
            .collect();
        return Some(Reply::Json(json!({"items": items, "metadata": {}})));
    }
    if let Some(v) = path.strip_prefix("/civitai/api/download/models/") {
        let name = v.parse().ok().and_then(civitai_name)?;
        return Some(Reply::Redirect(format!("http://{host}/cdn/{name}")));
    }
    if let Some(name) = path.strip_prefix("/cdn/") {
        return Some(Reply::Bytes(synth(name), "application/octet-stream"));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn synthetic_files_are_safetensors() {
        let b = synth("x.safetensors");
        let h = crate::arch::parse_safetensors_json(&b[8..8 + u64::from_le_bytes(b[..8].try_into().unwrap()) as usize]);
        assert!(h.is_ok());
        assert!(known_files().iter().any(|(r, p)| r == "Comfy-Org/Qwen-Image_ComfyUI" && p.ends_with("qwen_image_vae.safetensors")));
    }
}
