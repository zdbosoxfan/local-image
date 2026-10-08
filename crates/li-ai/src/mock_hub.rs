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
                "images": [{"url": img("image_qwen_image_edit_2511"), "nsfwLevel": 1, "type": "image"}]}]}),
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

// ------------------------------------------------------------------------------ cloud APIs

/// Images a mock BFL job produced, by id.
static BFL_JOBS: std::sync::Mutex<Vec<(String, Vec<u8>)>> = std::sync::Mutex::new(Vec::new());

/// A stand-in result: the source inverted (so edits are visible), or a gradient coloured by the
/// prompt.
fn cloud_image(prompt: &str, source: Option<image::RgbaImage>, w: u32, h: u32) -> Vec<u8> {
    let img = match source {
        Some(mut s) => {
            for p in s.pixels_mut() {
                p.0 = [255 - p[0], 255 - p[1], 255 - p[2], 255];
            }
            s
        }
        None => {
            let hsh = prompt.bytes().fold(2166136261u32, |h, b| (h ^ b as u32).wrapping_mul(16777619));
            let c = [(hsh & 0xff) as u8, ((hsh >> 8) & 0xff) as u8, ((hsh >> 16) & 0xff) as u8];
            image::RgbaImage::from_fn(w, h, |x, _| {
                let t = x as f32 / w.max(1) as f32;
                image::Rgba([(c[0] as f32 * t) as u8, c[1], (c[2] as f32 * (1.0 - t)) as u8, 255])
            })
        }
    };
    crate::imaging::encode_png(&img).unwrap_or_default()
}

/// Every PNG inside a multipart body.
fn multipart_pngs(body: &[u8]) -> Vec<image::RgbaImage> {
    const SIG: &[u8] = b"\x89PNG\r\n\x1a\n";
    let mut out = Vec::new();
    let mut i = 0;
    while let Some(p) = body[i..].windows(SIG.len()).position(|w| w == SIG) {
        let start = i + p;
        let end = body[start..].windows(4).position(|w| w == b"\r\n--").map(|e| start + e).unwrap_or(body.len());
        if let Ok(img) = image::load_from_memory(&body[start..end]) {
            out.push(img.to_rgba8());
        }
        i = end.max(start + 1);
    }
    out
}

fn multipart_field(body: &[u8], name: &str) -> Option<String> {
    let text = String::from_utf8_lossy(body);
    let marker = format!("name=\"{name}\"\r\n\r\n");
    let rest = &text[text.find(&marker)? + marker.len()..];
    Some(rest.split("\r\n").next()?.to_owned())
}

fn unb64(s: &str) -> Option<image::RgbaImage> {
    use base64::Engine as _;
    let b = base64::engine::general_purpose::STANDARD.decode(s).ok()?;
    image::load_from_memory(&b).ok().map(|i| i.to_rgba8())
}

fn b64(b: &[u8]) -> String {
    use base64::Engine as _;
    base64::engine::general_purpose::STANDARD.encode(b)
}

/// The mock cloud APIs (`/cloud/{openai,google,bfl,stability}/…`). `key` is the request's API
/// key header; `bad-key` is rejected the way the providers do.
pub fn handle_cloud(host: &str, method: &str, url: &str, key: Option<&str>, body: &[u8]) -> Option<Reply> {
    let path = url.split('?').next().unwrap_or("");
    let rest = path.strip_prefix("/cloud/")?;
    if let Some(id) = rest.strip_prefix("bfl/sample/") {
        let id = id.trim_end_matches(".png");
        let jobs = BFL_JOBS.lock().ok()?;
        return Some(jobs.iter().find(|(j, _)| j == id).map(|(_, b)| Reply::Bytes(b.clone(), "image/png")).unwrap_or(Reply::NotFound));
    }
    match key {
        None | Some("") => return Some(Reply::JsonStatus(401, json!({"error": {"message": "Missing API key"}}))),
        Some("bad-key") => return Some(Reply::JsonStatus(401, json!({"error": {"message": "Incorrect API key provided"}}))),
        _ => {}
    }
    let v: Value = serde_json::from_slice(body).unwrap_or(Value::Null);
    match (method, rest) {
        ("POST", "openai/v1/images/generations") => {
            let n = v["n"].as_u64().unwrap_or(1);
            let (w, h) =
                v["size"].as_str().and_then(|s| s.split_once('x')).map(|(a, b)| (a.parse().unwrap_or(1024), b.parse().unwrap_or(1024))).unwrap_or((1024, 1024));
            let data: Vec<Value> = (0..n).map(|_| json!({"b64_json": b64(&cloud_image(v["prompt"].as_str().unwrap_or(""), None, w, h))})).collect();
            Some(Reply::Json(json!({"data": data})))
        }
        ("POST", "openai/v1/images/edits") => {
            let n: u64 = multipart_field(body, "n").and_then(|v| v.parse().ok()).unwrap_or(1);
            let src = multipart_pngs(body).into_iter().next();
            let data: Vec<Value> = (0..n).map(|_| json!({"b64_json": b64(&cloud_image("", src.clone(), 1024, 1024))})).collect();
            Some(Reply::Json(json!({"data": data})))
        }
        ("POST", r) if r.starts_with("google/v1beta/models/") && r.ends_with(":generateContent") => {
            let parts = v["contents"][0]["parts"].as_array().cloned().unwrap_or_default();
            let src = parts.iter().find_map(|p| p["inline_data"]["data"].as_str().and_then(unb64));
            let prompt = parts.iter().find_map(|p| p["text"].as_str()).unwrap_or("");
            let png = cloud_image(prompt, src, 1024, 1024);
            Some(Reply::Json(
                json!({"candidates": [{"content": {"parts": [{"text": "Here you go"}, {"inlineData": {"mimeType": "image/png", "data": b64(&png)}}]}, "finishReason": "STOP"}]}),
            ))
        }
        ("POST", r) if r.starts_with("bfl/v1/") => {
            let src = v["input_image"].as_str().or_else(|| v["image"].as_str()).and_then(unb64);
            let id = uuid::Uuid::new_v4().simple().to_string();
            let png = cloud_image(v["prompt"].as_str().unwrap_or(""), src, 1024, 1024);
            BFL_JOBS.lock().ok()?.push((id.clone(), png));
            Some(Reply::Json(json!({"id": id, "polling_url": format!("http://{host}/cloud/bfl/v1/get_result?id={id}")})))
        }
        ("GET", "bfl/v1/get_result") => {
            let id = url.split("id=").nth(1).unwrap_or("");
            Some(Reply::Json(json!({"id": id, "status": "Ready", "result": {"sample": format!("http://{host}/cloud/bfl/sample/{id}.png")}})))
        }
        ("POST", r) if r.starts_with("stability/v2beta/") => {
            let src = multipart_pngs(body).into_iter().next();
            let prompt = multipart_field(body, "prompt").unwrap_or_default();
            Some(Reply::Bytes(cloud_image(&prompt, src, 1024, 1024), "image/png"))
        }
        _ => Some(Reply::NotFound),
    }
}

pub enum Reply {
    Json(Value),
    Bytes(Vec<u8>, &'static str),
    Redirect(String),
    Status(u16),
    JsonStatus(u16, Value),
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
