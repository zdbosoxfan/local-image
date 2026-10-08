//! A stand-in ComfyUI server for tests, demos and screenshots on machines without a GPU.
//!
//! It speaks the same HTTP API the real server does (`/object_info`, `/upload/image`, `/prompt`,
//! `/history`, `/view`, `/queue`, `/system_stats`, `/free`, job cancel) and advertises every node
//! and model file Local Image uses. Results are cheap, deterministic stand-ins: a generation is a
//! gradient coloured from the prompt, an AI Remove fills the outlined area from its surroundings,
//! a cutout keeps the pixels that differ from the border colour. Start it with
//! [`MockComfy::start`] and point [`crate::Ai::new`] at [`MockComfy::host`].

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use image::{Rgba, RgbaImage};
use serde_json::{Value, json};

use crate::catalog::{PRESETS, Role};

#[derive(Default)]
struct State {
    uploads: HashMap<String, Vec<u8>>,
    history: HashMap<String, Value>,
    outputs: HashMap<String, Vec<u8>>,
    prompts: Vec<Value>,
    next: u64,
}

/// A running mock server; stops when dropped.
pub struct MockComfy {
    host: String,
    state: Arc<Mutex<State>>,
    stop: Arc<AtomicBool>,
    /// Milliseconds each job takes (so progress and cancel can be observed).
    pub delay_ms: Arc<AtomicU64>,
    fail_next: Arc<AtomicBool>,
}

impl MockComfy {
    pub fn start() -> std::io::Result<Self> {
        Self::start_on("127.0.0.1:0")
    }

    pub fn start_on(addr: &str) -> std::io::Result<Self> {
        let server = tiny_http::Server::http(addr).map_err(std::io::Error::other)?;
        let host = server.server_addr().to_ip().map(|a| a.to_string()).unwrap_or_default();
        let state = Arc::new(Mutex::new(State::default()));
        let stop = Arc::new(AtomicBool::new(false));
        let delay_ms = Arc::new(AtomicU64::new(150));
        let fail_next = Arc::new(AtomicBool::new(false));
        let (st, sp, dl, fl) = (state.clone(), stop.clone(), delay_ms.clone(), fail_next.clone());
        std::thread::Builder::new().name("mock-comfy".into()).spawn(move || {
            while !sp.load(Ordering::SeqCst) {
                let Ok(Some(req)) = server.recv_timeout(Duration::from_millis(100)) else { continue };
                handle(req, &st, &dl, &fl);
            }
        })?;
        Ok(Self { host, state, stop, delay_ms, fail_next })
    }

    pub fn host(&self) -> &str {
        &self.host
    }

    /// The graphs queued so far.
    pub fn prompts(&self) -> Vec<Value> {
        self.state.lock().map(|s| s.prompts.clone()).unwrap_or_default()
    }

    /// Makes the next job end with an execution error (e.g. out of memory).
    pub fn fail_next(&self) {
        self.fail_next.store(true, Ordering::SeqCst);
    }
}

impl Drop for MockComfy {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
    }
}

/// Everything Local Image asks for: every node class used by the workflows, every model file.
pub fn object_info() -> Value {
    let mut files: HashMap<Role, Vec<String>> = HashMap::new();
    for p in PRESETS {
        for f in p.files {
            let v = files.entry(f.role).or_default();
            if !v.iter().any(|n| n == f.name) {
                v.push(f.name.to_owned());
            }
        }
    }
    let combo = |v: Vec<String>| json!([v]);
    let mut info = serde_json::Map::new();
    let nodes = [
        "UNETLoader",
        "CLIPLoader",
        "VAELoader",
        "TextEncodeQwenImage21",
        "KSampler",
        "VAEDecode",
        "SaveImage",
        "PreviewImage",
        "LoadImage",
        "JoinImageWithAlpha",
        "EmptyLatentImage",
        "QwenImage21Cache",
        "CLIPTextEncode",
        "ConditioningZeroOut",
        "ModelSamplingAuraFlow",
        "VAEEncode",
        "EmptySD3LatentImage",
        "EmptyFlux2LatentImage",
        "Flux2Scheduler",
        "RandomNoise",
        "KSamplerSelect",
        "SamplerCustomAdvanced",
        "CFGGuider",
        "ReferenceLatent",
        "LoraLoaderModelOnly",
        "SeedVR2Preprocess",
        "VAEEncodeTiled",
        "SeedVR2Conditioning",
        "VAEDecodeTiled",
        "SeedVR2PostProcessing",
    ];
    for n in nodes {
        info.insert(n.to_owned(), json!({ "input": { "required": {} } }));
    }
    let mut loras = files.remove(&Role::Lora).unwrap_or_default();
    loras.push("local-image/qwen-watercolor.safetensors".into());
    info.insert("UNETLoader".into(), json!({ "input": { "required": { "unet_name": combo(files.remove(&Role::Unet).unwrap_or_default()) } } }));
    info.insert(
        "CLIPLoader".into(),
        json!({ "input": { "required": { "clip_name": combo(files.remove(&Role::Clip).unwrap_or_default()), "type": ["COMBO", { "options": ["qwen_image", "flux2", "lumina2"] }] } } }),
    );
    info.insert("VAELoader".into(), json!({ "input": { "required": { "vae_name": combo(files.remove(&Role::Vae).unwrap_or_default()) } } }));
    info.insert("LoraLoaderModelOnly".into(), json!({ "input": { "required": { "lora_name": combo(loras) } } }));
    info.insert("KSampler".into(), json!({ "input": { "required": { "sampler_name": [["euler", "res_multistep"]] } } }));
    info.insert("KSamplerSelect".into(), json!({ "input": { "required": { "sampler_name": [["euler"]] } } }));
    Value::Object(info)
}

fn respond(req: tiny_http::Request, status: u16, body: Vec<u8>, ctype: &str) {
    let header = tiny_http::Header::from_bytes(&b"Content-Type"[..], ctype.as_bytes()).expect("static header");
    let _ = req.respond(tiny_http::Response::from_data(body).with_status_code(status).with_header(header));
}

fn json_resp(req: tiny_http::Request, status: u16, v: Value) {
    respond(req, status, v.to_string().into_bytes(), "application/json");
}

fn handle(mut req: tiny_http::Request, st: &Arc<Mutex<State>>, delay: &Arc<AtomicU64>, fail: &Arc<AtomicBool>) {
    let url = req.url().to_owned();
    let path = url.split('?').next().unwrap_or("").to_owned();
    let mut body = Vec::new();
    let _ = req.as_reader().read_to_end(&mut body);
    match (req.method().as_str(), path.as_str()) {
        ("GET", "/object_info") => json_resp(req, 200, object_info()),
        ("GET", "/system_stats") => json_resp(
            req,
            200,
            json!({ "system": { "comfyui_version": "mock" }, "devices": [{ "name": "cuda:0 Mock GPU 32 GB : cudaMallocAsync", "type": "cuda", "vram_total": 34359738368u64, "vram_free": 30064771072u64 }] }),
        ),
        ("GET", "/queue") => json_resp(req, 200, json!({ "queue_running": [], "queue_pending": [] })),
        ("POST", "/free") => json_resp(req, 200, json!({})),
        ("POST", "/upload/image") => {
            let (name, data) = parse_multipart(&body);
            if let Ok(mut s) = st.lock() {
                s.uploads.insert(name.clone(), data);
            }
            json_resp(req, 200, json!({ "name": name, "subfolder": "local-image", "type": "input" }))
        }
        ("POST", "/prompt") => {
            let v: Value = serde_json::from_slice(&body).unwrap_or(Value::Null);
            let graph = v.get("prompt").cloned().unwrap_or(Value::Null);
            let id = {
                let Ok(mut s) = st.lock() else { return };
                s.next += 1;
                s.prompts.push(graph.clone());
                format!("mock-{}", s.next)
            };
            json_resp(req, 200, json!({ "prompt_id": id, "number": 1 }));
            let (st, ms, fail_now) = (st.clone(), delay.load(Ordering::SeqCst), fail.swap(false, Ordering::SeqCst));
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_millis(ms));
                let Ok(mut s) = st.lock() else { return };
                if fail_now {
                    s.history.insert(id.clone(), json!({ "status": { "status_str": "error", "completed": false, "messages": [["execution_error", { "exception_message": "CUDA out of memory. Tried to allocate 2.00 GiB" }]] }, "outputs": {} }));
                    return;
                }
                let out = render(&graph, &s.uploads);
                let file = format!("{id}.png");
                s.outputs.insert(file.clone(), out);
                s.history.insert(id, json!({ "status": { "status_str": "success", "completed": true, "messages": [] }, "outputs": { "8": { "images": [{ "filename": file, "subfolder": "", "type": "output" }] } } }));
            });
        }
        ("GET", p) if p.starts_with("/history/") => {
            let id = &p["/history/".len()..];
            let entry = st.lock().ok().and_then(|s| s.history.get(id).cloned());
            json_resp(req, 200, entry.map(|e| json!({ id: e })).unwrap_or_else(|| json!({})))
        }
        ("GET", "/view") => {
            let file = url.split("filename=").nth(1).and_then(|s| s.split('&').next()).unwrap_or("").to_owned();
            match st.lock().ok().and_then(|s| s.outputs.get(&file).cloned()) {
                Some(b) => respond(req, 200, b, "image/png"),
                None => respond(req, 404, Vec::new(), "text/plain"),
            }
        }
        ("POST", p) if p.starts_with("/api/jobs/") && p.ends_with("/cancel") => {
            let id = p.trim_start_matches("/api/jobs/").trim_end_matches("/cancel").to_owned();
            if let Ok(mut s) = st.lock() {
                s.history
                    .insert(id, json!({ "status": { "status_str": "error", "completed": false, "messages": [["execution_interrupted", {}]] }, "outputs": {} }));
            }
            json_resp(req, 200, json!({ "cancelled": true }))
        }
        _ => respond(req, 404, b"not found".to_vec(), "text/plain"),
    }
}

fn parse_multipart(body: &[u8]) -> (String, Vec<u8>) {
    let text = String::from_utf8_lossy(body);
    let name = text.split("filename=\"").nth(1).and_then(|s| s.split('"').next()).unwrap_or("upload.png").to_owned();
    // The file part's data follows its header block and ends at the closing boundary.
    let marker = b"Content-Type: image/png\r\n\r\n";
    let start = body.windows(marker.len()).position(|w| w == marker).map(|i| i + marker.len()).unwrap_or(0);
    let tail = &body[start..];
    let end = tail.windows(4).rposition(|w| w == b"\r\n--").unwrap_or(tail.len());
    (name, tail[..end].to_vec())
}

fn input_of<'a>(graph: &'a Value, class: &str) -> Option<&'a Value> {
    graph.as_object()?.values().find(|n| n.get("class_type").and_then(Value::as_str) == Some(class)).map(|n| &n["inputs"])
}

fn uploaded(graph: &Value, uploads: &HashMap<String, Vec<u8>>, index: usize) -> Option<RgbaImage> {
    let mut loads: Vec<(i64, String)> = graph
        .as_object()?
        .iter()
        .filter(|(_, n)| n.get("class_type").and_then(Value::as_str) == Some("LoadImage"))
        .filter_map(|(k, n)| Some((k.parse().ok()?, n["inputs"]["image"].as_str()?.to_owned())))
        .collect();
    loads.sort();
    let value = &loads.get(index)?.1;
    let file = value.trim_end_matches(" [input]").rsplit('/').next()?;
    image::load_from_memory(uploads.get(file)?).ok().map(|i| i.to_rgba8())
}

fn png(img: &RgbaImage) -> Vec<u8> {
    crate::imaging::encode_png(img).unwrap_or_default()
}

fn hash_color(s: &str) -> [u8; 3] {
    let h = s.bytes().fold(2166136261u32, |h, b| (h ^ b as u32).wrapping_mul(16777619));
    [(h & 0xff) as u8 / 2 + 80, ((h >> 8) & 0xff) as u8 / 2 + 60, ((h >> 16) & 0xff) as u8 / 2 + 70]
}

fn render(graph: &Value, uploads: &HashMap<String, Vec<u8>>) -> Vec<u8> {
    let prompt = input_of(graph, "TextEncodeQwenImage21")
        .and_then(|i| i["prompt"].as_str())
        .or_else(|| input_of(graph, "CLIPTextEncode").and_then(|i| i["text"].as_str()))
        .unwrap_or("")
        .to_owned();
    // AI Remove (red outline input): fill the outlined box from the border average.
    if prompt == crate::workflows::KLEIN_REMOVE_PROMPT
        && let Some(mut img) = uploaded(graph, uploads, 0)
    {
        let reds: Vec<(u32, u32)> = img.enumerate_pixels().filter(|(_, _, p)| p.0 == [255, 0, 0, 255]).map(|(x, y, _)| (x, y)).collect();
        if let (Some(x0), Some(x1), Some(y0), Some(y1)) =
            (reds.iter().map(|p| p.0).min(), reds.iter().map(|p| p.0).max(), reds.iter().map(|p| p.1).min(), reds.iter().map(|p| p.1).max())
        {
            let ring: Vec<[u8; 4]> =
                (x0.saturating_sub(4)..=(x1 + 4).min(img.width() - 1)).map(|x| img.get_pixel(x, y1.saturating_add(5).min(img.height() - 1)).0).collect();
            let avg = |k: usize| (ring.iter().map(|p| p[k] as u32).sum::<u32>() / ring.len().max(1) as u32) as u8;
            let fill = Rgba([avg(0), avg(1), avg(2), 255]);
            for y in y0..=y1 {
                for x in x0..=x1 {
                    img.put_pixel(x, y, fill);
                }
            }
        }
        return png(&img);
    }
    // Background removal: keep what differs from the border colour.
    if prompt.starts_with("Remove the background")
        && let Some(src) = uploaded(graph, uploads, 0)
    {
        let b = src.get_pixel(0, 0).0;
        let out = RgbaImage::from_fn(src.width(), src.height(), |x, y| {
            let p = src.get_pixel(x, y).0;
            let d: i32 = (0..3).map(|k| (p[k] as i32 - b[k] as i32).abs()).sum();
            Rgba([p[0], p[1], p[2], if d > 60 { 255 } else { 0 }])
        });
        return png(&out);
    }
    // Instruction edits and upscales: return the (first) input, tinted slightly.
    if input_of(graph, "SeedVR2Preprocess").is_some() || input_of(graph, "TextEncodeQwenImage21").is_some_and(|i| i["resolution"] == json!(0)) {
        if let Some(mut src) = uploaded(graph, uploads, 0) {
            let tint = hash_color(&prompt);
            for p in src.pixels_mut() {
                for k in 0..3 {
                    p[k] = ((p[k] as u32 * 3 + tint[k] as u32) / 4) as u8;
                }
            }
            return png(&src);
        }
    }
    let size = ["EmptyLatentImage", "EmptyFlux2LatentImage", "EmptySD3LatentImage"]
        .iter()
        .find_map(|c| input_of(graph, c).map(|i| (i["width"].as_u64().unwrap_or(1024) as u32, i["height"].as_u64().unwrap_or(1024) as u32)))
        .or_else(|| uploaded(graph, uploads, 0).map(|i| i.dimensions()))
        .unwrap_or((1024, 1024));
    let c = hash_color(&prompt);
    let transparent = prompt.contains("fully transparent background");
    let img = RgbaImage::from_fn(size.0, size.1, |x, y| {
        let (fx, fy) = (x as f32 / size.0 as f32, y as f32 / size.1 as f32);
        let r = ((fx - 0.5).powi(2) + (fy - 0.55).powi(2)).sqrt();
        let a = if transparent && r > 0.35 { 0 } else { 255 };
        let shade = 1.0 - 0.6 * r;
        Rgba([(c[0] as f32 * shade + 40.0 * fy) as u8, (c[1] as f32 * shade) as u8, (c[2] as f32 * shade + 50.0 * fx) as u8, a])
    });
    png(&img)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::ModelId;
    use crate::ops::{Ai, GenerateMode, GenerateRequest, RemoveEngine};
    use crate::{Cancelled, JobControl};
    use image::{GrayImage, Luma, RgbImage};

    #[test]
    fn generate_edit_remove_cutout_upscale_against_the_mock() {
        let server = MockComfy::start().unwrap();
        server.delay_ms.store(20, Ordering::SeqCst);
        let ai = Ai::new(server.host());
        let ctl = JobControl::new();
        // Create.
        let mut req = GenerateRequest::new(ModelId::Klein4B, "a red fox in snow");
        req.width = 768;
        req.height = 512;
        let img = ai.generate(&req, &ctl).unwrap();
        assert_eq!(img.dimensions(), (768, 512));
        // Transparent Qwen output keeps alpha.
        let mut req = GenerateRequest::new(ModelId::Qwen, "a shield");
        req.transparent = true;
        let t = ai.generate(&req, &ctl).unwrap();
        assert!(t.pixels().any(|p| p[3] == 0) && t.pixels().any(|p| p[3] == 255));
        // Edit keeps the source size.
        let src = RgbaImage::from_pixel(300, 200, Rgba([200, 30, 30, 255]));
        let mut req = GenerateRequest::new(ModelId::Qwen, "make it blue");
        req.mode = GenerateMode::Edit;
        req.references = vec![src.clone()];
        assert_eq!(ai.generate(&req, &ctl).unwrap().dimensions(), (300, 200));
        // LoRA names are resolved against the inventory.
        let mut req = GenerateRequest::new(ModelId::Qwen, "styled");
        req.loras = vec![crate::workflows::LoraUse { name: "local-image/qwen-watercolor.safetensors".into(), strength: 0.7 }];
        ai.generate(&req, &ctl).unwrap();
        assert!(server.prompts().last().unwrap().get("100").is_some());
        req.loras[0].name = "missing.safetensors".into();
        assert!(ai.generate(&req, &ctl).unwrap_err().to_string().contains("not installed"));
        // AI Remove: only the selection changes.
        let photo = RgbImage::from_fn(900, 600, |x, _| image::Rgb([(x / 4) as u8, 120, 90]));
        let mut mask = GrayImage::new(900, 600);
        for y in 250..330 {
            for x in 400..470 {
                mask.put_pixel(x, y, Luma([255]));
            }
        }
        let out = ai.remove_objects(&photo, &mask, &RemoveEngine::Klein, 1, &ctl).unwrap();
        assert_eq!(out.get_pixel(10, 10), photo.get_pixel(10, 10));
        assert!(out.enumerate_pixels().filter(|(x, y, _)| mask.get_pixel(*x, *y)[0] > 0).any(|(x, y, p)| p != photo.get_pixel(x, y)));
        // Qwen removal path too.
        ai.remove_objects(&photo, &mask, &RemoveEngine::Qwen { variant: "int8".into() }, 1, &ctl).unwrap();
        // Cutout returns a matte the size of the input.
        let subject =
            RgbaImage::from_fn(
                200,
                160,
                |x, y| if (60..140).contains(&x) && (40..120).contains(&y) { Rgba([250, 200, 0, 255]) } else { Rgba([20, 20, 20, 255]) },
            );
        let matte = ai.cutout(&subject, "int8", "", 3, &ctl).unwrap();
        assert_eq!(matte.dimensions(), (200, 160));
        assert_eq!(matte.get_pixel(100, 80)[0], 255);
        assert_eq!(matte.get_pixel(5, 5)[0], 0);
        // Upscale.
        let up = ai.upscale(&subject, 400, 320, 1, &ctl).unwrap();
        assert_eq!(up.dimensions(), (400, 320));
    }

    #[test]
    fn errors_and_cancellation_are_reported() {
        let server = MockComfy::start().unwrap();
        let ai = Ai::new(server.host());
        server.fail_next();
        let err = ai.generate(&GenerateRequest::new(ModelId::ZImageTurbo, "x"), &JobControl::new()).unwrap_err();
        assert!(err.to_string().contains("ran out of memory"), "{err}");
        server.delay_ms.store(3000, Ordering::SeqCst);
        let ctl = JobControl::new();
        let c2 = ctl.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(300));
            c2.cancel();
        });
        let err = ai.generate(&GenerateRequest::new(ModelId::ZImageTurbo, "x"), &ctl).unwrap_err();
        assert!(err.is::<Cancelled>(), "{err:#}");
        // Unreachable server: a clear, quick error.
        let gone = Ai::new("127.0.0.1:9");
        let t = std::time::Instant::now();
        let err = gone.generate(&GenerateRequest::new(ModelId::ZImageTurbo, "x"), &JobControl::new()).unwrap_err();
        assert!(err.to_string().contains("Cannot reach ComfyUI"), "{err}");
        assert!(t.elapsed() < Duration::from_secs(5));
    }
}
