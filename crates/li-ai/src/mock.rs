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

use crate::family::Role;

#[derive(Default)]
struct State {
    uploads: HashMap<String, Vec<u8>>,
    history: HashMap<String, Value>,
    outputs: HashMap<String, Vec<u8>>,
    prompts: Vec<Value>,
    next: u64,
    host: String,
    /// A model folder whose files `/object_info` lists too (installs show up after a refresh).
    model_dir: Option<std::path::PathBuf>,
}

/// A running mock server; stops when dropped.
pub struct MockComfy {
    host: String,
    state: Arc<Mutex<State>>,
    stop: Arc<AtomicBool>,
    /// Milliseconds each job takes (so progress and cancel can be observed).
    pub delay_ms: Arc<AtomicU64>,
    fail_next: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl MockComfy {
    pub fn start() -> std::io::Result<Self> {
        Self::start_on("127.0.0.1:0")
    }

    pub fn start_on(addr: &str) -> std::io::Result<Self> {
        let server = tiny_http::Server::http(addr).map_err(std::io::Error::other)?;
        let host = server.server_addr().to_ip().map(|a| a.to_string()).unwrap_or_default();
        crate::download::allow_test_host(&host);
        let state = Arc::new(Mutex::new(State { host: host.clone(), ..Default::default() }));
        let stop = Arc::new(AtomicBool::new(false));
        let delay_ms = Arc::new(AtomicU64::new(150));
        let fail_next = Arc::new(AtomicBool::new(false));
        let (st, sp, dl, fl) = (state.clone(), stop.clone(), delay_ms.clone(), fail_next.clone());
        let thread = std::thread::Builder::new().name("mock-comfy".into()).spawn(move || {
            while !sp.load(Ordering::SeqCst) {
                let Ok(Some(req)) = server.recv_timeout(Duration::from_millis(100)) else { continue };
                handle(req, &st, &dl, &fl);
            }
        })?;
        Ok(Self { host, state, stop, delay_ms, fail_next, thread: Some(thread) })
    }

    pub fn host(&self) -> &str {
        &self.host
    }

    /// The graphs queued so far.
    pub fn prompts(&self) -> Vec<Value> {
        self.state.lock().map(|s| s.prompts.clone()).unwrap_or_default()
    }

    /// Lists the model files under `dir` too (as ComfyUI does for its `models` folder).
    pub fn set_model_dir(&self, dir: impl Into<std::path::PathBuf>) {
        if let Ok(mut s) = self.state.lock() {
            s.model_dir = Some(dir.into());
        }
    }

    /// Makes the next job end with an execution error (e.g. out of memory).
    pub fn fail_next(&self) {
        self.fail_next.store(true, Ordering::SeqCst);
    }
}

impl Drop for MockComfy {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

/// Models the mock reports as installed beyond the catalogue's presets: one or more of every
/// family, so the generic graphs, detection and the model picker can be exercised.
pub const FIXTURE_CHECKPOINTS: &[&str] = &[
    "juggernautXL_v9.safetensors",
    "dreamshaper_8.safetensors",
    "ponyDiffusionV6XL.safetensors",
    "novaAnimeXL_ilV125.safetensors",
    "sd3.5_large_fp8_scaled.safetensors",
];
pub const FIXTURE_UNETS: &[&str] = &[
    "flux1-dev-fp8.safetensors",
    "flux1-dev-kontext_fp8_scaled.safetensors",
    "flux1-fill-dev.safetensors",
    "qwen_image_fp8_e4m3fn.safetensors",
    "qwen_image_edit_2511_bf16.safetensors",
    "hidream_i1_dev_fp8.safetensors",
    "flux2_dev_Q4_K_M.gguf",
];
const FIXTURE_CLIPS: &[&str] = &[
    "clip_l.safetensors",
    "clip_g.safetensors",
    "t5xxl_fp8_e4m3fn_scaled.safetensors",
    "qwen_2.5_vl_7b_fp8_scaled.safetensors",
    "clip_l_hidream.safetensors",
    "clip_g_hidream.safetensors",
    "llama_3.1_8b_instruct_fp8_scaled.safetensors",
    "mistral_3_small_flux2_fp8.safetensors",
];
const FIXTURE_VAES: &[&str] = &["ae.safetensors", "qwen_image_vae.safetensors"];
const FIXTURE_LORAS: &[&str] =
    &["local-image/qwen-watercolor.safetensors", "pony_style_v2.safetensors", "flux_realism_lora.safetensors", "sdxl_detail_tweaker.safetensors"];

/// Everything Local Image asks for: every node class the family graphs use, every model file.
pub fn object_info() -> Value {
    object_info_with(None)
}

/// Files under a model folder, relative with `/` (ComfyUI's listing).
fn files_in(dir: &std::path::Path) -> Vec<String> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).into_iter().flatten().flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else if p.extension().is_some_and(|x| x == "safetensors" || x == "gguf")
                && let Ok(rel) = p.strip_prefix(dir)
            {
                out.push(rel.to_string_lossy().replace('\\', "/"));
            }
        }
    }
    out.sort();
    out
}

/// The definitions of the core nodes the family graphs and the official templates use, in
/// ComfyUI's widget order (`input_order`), so editor-format workflows convert as they would
/// against a real server. `@x` is a file list, `INT*` an INT with `control_after_generate`.
const NODE_DEFS: &[(&str, &str, &str)] = &[
    (
        "KSampler",
        "model:MODEL seed:INT* steps:INT cfg:FLOAT sampler_name:@samplers scheduler:@schedulers positive:CONDITIONING negative:CONDITIONING latent_image:LATENT denoise:FLOAT",
        "",
    ),
    ("KSamplerSelect", "sampler_name:@samplers", ""),
    ("BasicScheduler", "model:MODEL scheduler:@schedulers steps:INT denoise:FLOAT", ""),
    ("CFGGuider", "model:MODEL positive:CONDITIONING negative:CONDITIONING cfg:FLOAT", ""),
    ("BasicGuider", "model:MODEL conditioning:CONDITIONING", ""),
    (
        "DualCFGGuider",
        "model:MODEL cond1:CONDITIONING cond2:CONDITIONING negative:CONDITIONING cfg_conds:FLOAT cfg_cond2_negative:FLOAT style:(regular|nested)",
        "",
    ),
    ("CFGNorm", "model:MODEL strength:FLOAT", ""),
    ("CLIPLoader", "clip_name:@clip type:(stable_diffusion|qwen_image|flux2|lumina2|chroma|hidream|sd3|ernie)", "device:(default|cpu)"),
    ("DualCLIPLoader", "clip_name1:@clip clip_name2:@clip type:(sdxl|sd3|flux|hidream)", "device:(default|cpu)"),
    ("TripleCLIPLoader", "clip_name1:@clip clip_name2:@clip clip_name3:@clip", ""),
    ("QuadrupleCLIPLoader", "clip_name1:@clip clip_name2:@clip clip_name3:@clip clip_name4:@clip", ""),
    ("CLIPTextEncode", "text:STRING clip:CLIP", ""),
    ("CheckpointLoaderSimple", "ckpt_name:@ckpt", ""),
    ("UNETLoader", "unet_name:@unet weight_dtype:(default|fp8_e4m3fn|fp8_e4m3fn_fast|fp8_e5m2)", ""),
    ("UnetLoaderGGUF", "unet_name:@gguf", ""),
    ("VAELoader", "vae_name:@vae", ""),
    ("VAEDecode", "samples:LATENT vae:VAE", ""),
    ("VAEEncode", "pixels:IMAGE vae:VAE", ""),
    ("ConditioningZeroOut", "conditioning:CONDITIONING", ""),
    ("ComfySwitchNode", "switch:BOOLEAN on_false:* on_true:*", ""),
    ("EmptyLatentImage", "width:INT height:INT batch_size:INT", ""),
    ("EmptySD3LatentImage", "width:INT height:INT batch_size:INT", ""),
    ("EmptyFlux2LatentImage", "width:INT height:INT batch_size:INT", ""),
    ("Flux2Scheduler", "steps:INT width:INT height:INT", ""),
    ("FluxGuidance", "conditioning:CONDITIONING guidance:FLOAT", ""),
    ("FluxKontextImageScale", "image:IMAGE", ""),
    ("FluxKontextMultiReferenceLatentMethod", "conditioning:CONDITIONING reference_latents_method:(offset|index|uxo/uno)", ""),
    (
        "ImageStitch",
        "image1:IMAGE direction:(right|down|left|up) match_image_size:BOOLEAN spacing_width:INT spacing_color:(white|black|red|green|blue)",
        "image2:IMAGE",
    ),
    ("LoadImage", "image:@image", ""),
    ("LoraLoaderModelOnly", "model:MODEL lora_name:@lora strength_model:FLOAT", ""),
    ("LoraLoader", "model:MODEL clip:CLIP lora_name:@lora strength_model:FLOAT strength_clip:FLOAT", ""),
    ("ModelSamplingAuraFlow", "model:MODEL shift:FLOAT", ""),
    ("ModelSamplingSD3", "model:MODEL shift:FLOAT", ""),
    ("PreviewAny", "source:*", ""),
    ("PreviewImage", "images:IMAGE", ""),
    ("PrimitiveBoolean", "value:BOOLEAN", ""),
    ("PrimitiveFloat", "value:FLOAT", ""),
    ("PrimitiveInt", "value:INT*", ""),
    ("PrimitiveStringMultiline", "value:STRING", ""),
    ("RandomNoise", "noise_seed:INT*", ""),
    ("ReferenceLatent", "conditioning:CONDITIONING", "latent:LATENT"),
    ("ResolutionSelector", "aspect_ratio:(1:1 (Square)|4:3|3:2|16:9) megapixels:FLOAT multiple_of:INT", ""),
    ("SamplerCustomAdvanced", "noise:NOISE guider:GUIDER sampler:SAMPLER sigmas:SIGMAS latent_image:LATENT", ""),
    ("SaveImage", "images:IMAGE filename_prefix:STRING", ""),
    ("SaveImageAdvanced", "images:IMAGE filename_prefix:STRING format:(png|jpg|webp) bit_depth:(8-bit|16-bit) color_space:(sRGB|linear)", ""),
    ("StringReplace", "string:STRING find:STRING replace:STRING", ""),
    ("T5TokenizerOptions", "clip:CLIP min_padding:INT min_length:INT", ""),
    ("TextEncodeQwenImageEditPlus", "clip:CLIP prompt:STRING", "vae:VAE image1:IMAGE image2:IMAGE image3:IMAGE"),
    (
        "TextGenerate",
        "clip:CLIP prompt:STRING max_length:INT sampling_mode:(on|off) temperature:FLOAT top_k:INT top_p:FLOAT min_p:FLOAT repetition_penalty:FLOAT seed:INT",
        "image:IMAGE",
    ),
];

fn node_def(required: &str, optional: &str, files: &HashMap<&str, Vec<String>>) -> Value {
    let mut def = json!({ "input": { "required": {}, "optional": {} }, "input_order": { "required": [], "optional": [] } });
    for (section, spec) in [("required", required), ("optional", optional)] {
        for item in spec.split(' ').filter(|s| !s.is_empty()) {
            let Some((name, ty)) = item.split_once(':') else { continue };
            let v = if let Some(list) = ty.strip_prefix('@') {
                if list == "image" {
                    json!([files.get("image").cloned().unwrap_or_default(), { "image_upload": true }])
                } else {
                    json!([files.get(list).cloned().unwrap_or_default()])
                }
            } else if let Some(opts) = ty.strip_prefix('(').and_then(|t| t.strip_suffix(')')) {
                json!([opts.split('|').collect::<Vec<_>>()])
            } else if ty == "INT*" {
                json!(["INT", { "default": 0, "control_after_generate": true }])
            } else if ty == "STRING" {
                json!(["STRING", { "multiline": true }])
            } else if matches!(ty, "INT" | "FLOAT" | "BOOLEAN") {
                json!([ty, {}])
            } else {
                json!([ty])
            };
            def["input"][section][name] = v;
            def["input_order"][section].as_array_mut().expect("array").push(json!(name));
        }
    }
    def
}

/// [`object_info`] plus the files under `model_dir` (`checkpoints`, `diffusion_models`, `loras`…).
pub fn object_info_with(model_dir: Option<&std::path::Path>) -> Value {
    let mut files: HashMap<Role, Vec<String>> = HashMap::new();
    let mut add = |role: Role, name: &str| {
        let role = match role {
            Role::Clip2 | Role::Clip3 | Role::Clip4 => Role::Clip,
            r => r,
        };
        let v = files.entry(role).or_default();
        if !v.iter().any(|n| n == name) {
            v.push(name.to_owned());
        }
    };
    for p in crate::catalog::presets().iter().filter(|p| !p.installed) {
        for f in &p.files {
            add(f.role, &f.name);
        }
    }
    for n in FIXTURE_CHECKPOINTS {
        add(Role::Checkpoint, n);
    }
    for n in FIXTURE_UNETS {
        add(Role::Unet, n);
    }
    for n in FIXTURE_CLIPS {
        add(Role::Clip, n);
    }
    for n in FIXTURE_VAES {
        add(Role::Vae, n);
    }
    for n in FIXTURE_LORAS {
        add(Role::Lora, n);
    }
    if let Some(dir) = model_dir {
        for (folder, role) in [
            ("checkpoints", Role::Checkpoint),
            ("diffusion_models", Role::Unet),
            ("unet", Role::Unet),
            ("loras", Role::Lora),
            ("text_encoders", Role::Clip),
            ("clip", Role::Clip),
            ("vae", Role::Vae),
        ] {
            for f in files_in(&dir.join(folder)) {
                add(role, &f);
            }
        }
    }
    let combo = |v: Vec<String>| json!([v]);
    let mut info = serde_json::Map::new();
    let mut nodes: Vec<String> = [
        "SaveImage",
        "PreviewImage",
        "LoadImage",
        "JoinImageWithAlpha",
        "QwenImage21Cache",
        "LoraLoader",
        "LoraLoaderModelOnly",
        "SeedVR2Preprocess",
        "VAEEncodeTiled",
        "SeedVR2Conditioning",
        "VAEDecodeTiled",
        "SeedVR2PostProcessing",
        "UpscaleModelLoader",
        "ImageUpscaleWithModel",
    ]
    .iter()
    .map(|s| (*s).to_owned())
    .collect();
    for f in &crate::family::registry().families {
        nodes.extend(crate::builders::required_nodes(f));
        for task in [crate::builders::Task::Refine { denoise: 0.5 }, crate::builders::Task::Inpaint { denoise: 1.0 }, crate::builders::Task::Edit] {
            nodes.extend(crate::builders::task_nodes(f, task, 1));
        }
    }
    nodes.sort();
    nodes.dedup();
    for n in nodes {
        info.insert(n, json!({ "input": { "required": {} } }));
    }
    let clips = files.remove(&Role::Clip).unwrap_or_default();
    info.insert("UNETLoader".into(), json!({ "input": { "required": { "unet_name": combo(files.remove(&Role::Unet).unwrap_or_default()) } } }));
    info.insert(
        "CheckpointLoaderSimple".into(),
        json!({ "input": { "required": { "ckpt_name": combo(files.remove(&Role::Checkpoint).unwrap_or_default()) } } }),
    );
    info.insert(
        "CLIPLoader".into(),
        json!({ "input": { "required": { "clip_name": combo(clips.clone()), "type": ["COMBO", { "options": ["stable_diffusion", "qwen_image", "flux2", "lumina2", "chroma"] }] } } }),
    );
    info.insert(
        "DualCLIPLoader".into(),
        json!({ "input": { "required": { "clip_name1": combo(clips.clone()), "clip_name2": combo(clips.clone()), "type": [["sdxl", "sd3", "flux", "hidream"]] } } }),
    );
    info.insert(
        "TripleCLIPLoader".into(),
        json!({ "input": { "required": { "clip_name1": combo(clips.clone()), "clip_name2": combo(clips.clone()), "clip_name3": combo(clips.clone()) } } }),
    );
    info.insert(
        "QuadrupleCLIPLoader".into(),
        json!({ "input": { "required": { "clip_name1": combo(clips.clone()), "clip_name2": combo(clips.clone()), "clip_name3": combo(clips.clone()), "clip_name4": combo(clips) } } }),
    );
    info.insert("VAELoader".into(), json!({ "input": { "required": { "vae_name": combo(files.remove(&Role::Vae).unwrap_or_default()) } } }));
    let loras = files.remove(&Role::Lora).unwrap_or_default();
    info.insert("LoraLoaderModelOnly".into(), json!({ "input": { "required": { "lora_name": combo(loras.clone()) } } }));
    info.insert("LoraLoader".into(), json!({ "input": { "required": { "lora_name": combo(loras) } } }));
    info.insert(
        "KSampler".into(),
        json!({ "input": { "required": { "sampler_name": [["euler", "euler_ancestral", "res_multistep", "dpmpp_2m", "dpmpp_2m_sde", "uni_pc"]], "scheduler": [["simple", "karras", "normal", "sgm_uniform", "beta"]] } } }),
    );
    info.insert("KSamplerSelect".into(), json!({ "input": { "required": { "sampler_name": [["euler"]] } } }));
    info.insert("UnetLoaderGGUF".into(), json!({ "input": { "required": { "unet_name": [["flux2_dev_Q4_K_M.gguf"]] } } }));
    // Full definitions (widget order) for the core nodes, with the file lists above.
    let list = |class: &str, input: &str| -> Vec<String> {
        info.get(class)
            .and_then(|d| d["input"]["required"][input][0].as_array())
            .map(|a| a.iter().filter_map(|v| v.as_str().map(str::to_owned)).collect())
            .unwrap_or_default()
    };
    let mut lists: HashMap<&str, Vec<String>> = HashMap::new();
    lists.insert("ckpt", list("CheckpointLoaderSimple", "ckpt_name"));
    lists.insert("unet", list("UNETLoader", "unet_name").into_iter().filter(|n| !n.ends_with(".gguf")).collect());
    lists.insert("gguf", list("UnetLoaderGGUF", "unet_name"));
    lists.insert("clip", list("CLIPLoader", "clip_name"));
    lists.insert("vae", list("VAELoader", "vae_name"));
    lists.insert("lora", list("LoraLoader", "lora_name"));
    lists.insert("samplers", list("KSampler", "sampler_name"));
    lists.insert("schedulers", list("KSampler", "scheduler"));
    lists.insert("image", vec!["example.png".into()]);
    for (class, req, opt) in NODE_DEFS {
        info.insert((*class).into(), node_def(req, opt, &lists));
    }
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
        ("GET", "/object_info") => {
            let dir = st.lock().ok().and_then(|s| s.model_dir.clone());
            json_resp(req, 200, object_info_with(dir.as_deref()))
        }
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
        (m @ ("GET" | "HEAD" | "POST"), _) => {
            let host = st.lock().map(|s| s.host.clone()).unwrap_or_default();
            let key = req
                .headers()
                .iter()
                .find(|h| ["authorization", "x-goog-api-key", "x-key"].iter().any(|k| h.field.equiv(k)))
                .map(|h| h.value.as_str().trim_start_matches("Bearer ").to_owned());
            let reply = if path.starts_with("/cloud/") {
                crate::mock_hub::handle_cloud(&host, m, &url, key.as_deref(), &body)
            } else if m == "POST" {
                None
            } else {
                crate::mock_hub::handle(&host, &url)
            };
            match reply {
                Some(crate::mock_hub::Reply::Json(v)) => json_resp(req, 200, v),
                Some(crate::mock_hub::Reply::Bytes(b, t)) => respond(req, 200, b, t),
                Some(crate::mock_hub::Reply::Redirect(to)) => {
                    let header = tiny_http::Header::from_bytes(&b"Location"[..], to.as_bytes()).expect("location header");
                    let _ = req.respond(tiny_http::Response::empty(302).with_header(header));
                }
                Some(crate::mock_hub::Reply::Status(code)) => respond(req, code, b"denied".to_vec(), "text/plain"),
                Some(crate::mock_hub::Reply::JsonStatus(code, v)) => json_resp(req, code, v),
                Some(crate::mock_hub::Reply::NotFound) | None => respond(req, 404, b"not found".to_vec(), "text/plain"),
            }
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
    let mut loads: Vec<(Vec<i64>, String)> = graph
        .as_object()?
        .iter()
        .filter(|(_, n)| n.get("class_type").and_then(Value::as_str) == Some("LoadImage"))
        .filter_map(|(k, n)| Some((k.split(':').map(|p| p.parse().unwrap_or(i64::MAX)).collect(), n["inputs"]["image"].as_str()?.to_owned())))
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
        .or_else(|| input_of(graph, "TextEncodeQwenImageEditPlus").and_then(|i| i["prompt"].as_str()))
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
    if (input_of(graph, "SeedVR2Preprocess").is_some() || input_of(graph, "TextEncodeQwenImage21").is_some_and(|i| i["resolution"] == json!(0)))
        && let Some(mut src) = uploaded(graph, uploads, 0)
    {
        let tint = hash_color(&prompt);
        for p in src.pixels_mut() {
            for k in 0..3 {
                p[k] = ((p[k] as u32 * 3 + tint[k] as u32) / 4) as u8;
            }
        }
        return png(&src);
    }
    // Refine, inpaint and generic edits (a source image, no empty latent): the source, tinted;
    // green-filled areas (instruction inpainting) are painted with the prompt colour.
    let empty_latent = ["EmptyLatentImage", "EmptyFlux2LatentImage", "EmptySD3LatentImage"].iter().any(|c| input_of(graph, c).is_some());
    let edits = input_of(graph, "SetLatentNoiseMask").is_some()
        || input_of(graph, "InpaintModelConditioning").is_some()
        || input_of(graph, "TextEncodeQwenImageEditPlus").is_some()
        || (!empty_latent && input_of(graph, "VAEEncode").is_some());
    if edits && let Some(mut src) = uploaded(graph, uploads, 0) {
        let tint = hash_color(&prompt);
        for p in src.pixels_mut() {
            if p.0[..3] == [0, 255, 0] {
                *p = Rgba([tint[0], tint[1], tint[2], 255]);
                continue;
            }
            for k in 0..3 {
                p[k] = ((p[k] as u32 * 3 + tint[k] as u32) / 4) as u8;
            }
        }
        return png(&src);
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

    fn classes_of(g: &Value) -> Vec<String> {
        g.as_object().unwrap().values().map(|n| n["class_type"].as_str().unwrap().to_owned()).collect()
    }

    #[test]
    fn every_family_runs_against_the_mock() {
        let server = MockComfy::start().unwrap();
        server.delay_ms.store(10, Ordering::SeqCst);
        let ai = Ai::new(server.host());
        let ctl = JobControl::new();
        // Installed models are found and join the catalogue.
        crate::inventory::refresh(&crate::comfy::ObjectInfo(object_info()), None);
        let key = |k: &str| ModelId::from_key(k).unwrap_or_else(|| panic!("{k} not in the catalogue"));
        let sdxl = key("ckpt:juggernautXL_v9.safetensors");
        assert_eq!(sdxl.info().family, "sdxl");
        // SDXL create with a LoRA through LoraLoader.
        let mut req = GenerateRequest::new(sdxl, "a lighthouse at dusk");
        req.variant = "installed".into();
        req.width = 832;
        req.height = 1216;
        req.loras = vec![crate::workflows::LoraUse { name: "sdxl_detail_tweaker.safetensors".into(), strength: 0.5 }];
        assert_eq!(ai.generate(&req, &ctl).unwrap().dimensions(), (832, 1216));
        let g = server.prompts().last().unwrap().clone();
        let c = classes_of(&g);
        assert!(c.contains(&"CheckpointLoaderSimple".into()) && c.contains(&"LoraLoader".into()));
        // Pony gets its score tags.
        let pony = key("ckpt:ponyDiffusionV6XL.safetensors");
        let mut req = GenerateRequest::new(pony, "a fox");
        req.variant = "installed".into();
        ai.generate(&req, &ctl).unwrap();
        let g = server.prompts().last().unwrap().clone();
        assert!(g.as_object().unwrap().values().any(|n| n["inputs"]["text"].as_str().is_some_and(|t| t.starts_with("score_9"))));
        // Every installed family creates an image.
        for k in [
            "unet:flux1-dev-fp8.safetensors",
            "unet:qwen_image_fp8_e4m3fn.safetensors",
            "unet:hidream_i1_dev_fp8.safetensors",
            "ckpt:sd3.5_large_fp8_scaled.safetensors",
        ] {
            let m = key(k);
            let mut req = GenerateRequest::new(m, "a teapot");
            req.variant = "installed".into();
            req.width = 512;
            req.height = 512;
            assert_eq!(ai.generate(&req, &ctl).unwrap_or_else(|e| panic!("{k}: {e:#}")).dimensions(), (512, 512), "{k}");
        }
        // Qwen Edit edits natively and keeps the size.
        let src = RgbaImage::from_pixel(640, 480, Rgba([200, 30, 30, 255]));
        let mut req = GenerateRequest::new(key("unet:qwen_image_edit_2511_bf16.safetensors"), "make it blue");
        req.variant = "installed".into();
        req.mode = GenerateMode::Edit;
        req.source = Some(src.clone());
        assert_eq!(ai.generate(&req, &ctl).unwrap().dimensions(), (640, 480));
        assert!(classes_of(server.prompts().last().unwrap()).contains(&"TextEncodeQwenImageEditPlus".into()));
        // SDXL "edit" becomes a refine of the image.
        let mut req = GenerateRequest::new(sdxl, "golden hour");
        req.variant = "installed".into();
        req.mode = GenerateMode::Edit;
        req.source = Some(src.clone());
        ai.generate(&req, &ctl).unwrap();
        let k = server.prompts().last().unwrap().as_object().unwrap().values().find(|n| n["class_type"] == "KSampler").unwrap().clone();
        assert!(k["inputs"]["denoise"].as_f64().unwrap() < 1.0);
        // Inpaint: only the selection (and its feathered edge) changes.
        let photo = RgbaImage::from_fn(1200, 800, |x, _| Rgba([(x / 5) as u8, 120, 90, 255]));
        let mut mask = GrayImage::new(1200, 800);
        for y in 300..420 {
            for x in 500..640 {
                mask.put_pixel(x, y, Luma([255]));
            }
        }
        for k in ["ckpt:juggernautXL_v9.safetensors", "unet:flux1-fill-dev.safetensors", "unet:qwen_image_edit_2511_bf16.safetensors"] {
            let mut req = GenerateRequest::new(key(k), "a stone");
            req.variant = "installed".into();
            req.mode = GenerateMode::Inpaint;
            req.denoise = 1.0;
            req.source = Some(photo.clone());
            req.mask = Some(mask.clone());
            let out = ai.generate(&req, &ctl).unwrap_or_else(|e| panic!("{k}: {e:#}"));
            assert_eq!(out.dimensions(), (1200, 800));
            assert_eq!(out.get_pixel(20, 20), photo.get_pixel(20, 20), "{k}");
            assert_ne!(out.get_pixel(570, 360), photo.get_pixel(570, 360), "{k}");
        }
        let c = classes_of(server.prompts().last().unwrap());
        assert!(c.contains(&"TextEncodeQwenImageEditPlus".into()), "instruction inpaint");
        // Draft → Refine: Klein drafts, SDXL refines.
        let mut req = GenerateRequest::new(ModelId::Klein4B, "a castle");
        req.width = 512;
        req.height = 512;
        req.refine = Some(crate::ops::RefineStep { model: sdxl, variant: "installed".into(), strength: 0.35, steps: None, guidance: None, scale: 1.5 });
        let before = server.prompts().len();
        let out = ai.generate(&req, &ctl).unwrap();
        assert_eq!(out.dimensions(), (768, 768));
        assert_eq!(server.prompts().len(), before + 2);
        // Upscale-refine: tiles cover a 2× enlargement.
        let mut req = GenerateRequest::new(sdxl, "");
        req.variant = "installed".into();
        req.mode = GenerateMode::UpscaleRefine;
        req.source = Some(RgbaImage::from_pixel(700, 500, Rgba([90, 90, 200, 255])));
        req.scale = 2.0;
        req.denoise = 0.3;
        let before = server.prompts().len();
        let out = ai.generate(&req, &ctl).unwrap();
        assert_eq!(out.dimensions(), (1400, 1000));
        assert!(server.prompts().len() - before >= 2);
        crate::catalog::set_installed(Vec::new());
    }
}
