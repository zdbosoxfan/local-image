use li_ai::mock::{FIXTURE_CHECKPOINTS, FIXTURE_UNETS, MockComfy, object_info, object_info_with};
use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const PNG_SIGNATURE: &[u8] = &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];

// ---------------------------------------------------------------------------
// Minimal helpers
// ---------------------------------------------------------------------------

struct TempDir(PathBuf);
impl TempDir {
    fn new(tag: &str) -> Self {
        let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        let p = std::env::temp_dir().join(format!("li_ai_mock_{}_{}_{}", tag, std::process::id(), nanos));
        std::fs::create_dir_all(&p).unwrap();
        TempDir(p)
    }
    fn path(&self) -> &Path {
        &self.0
    }
}
impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn http_request(addr: &str, method: &str, path: &str, body: &[u8], headers: &[(&str, &str)]) -> Result<(u16, Vec<u8>), String> {
    let mut stream = TcpStream::connect(addr).map_err(|e| e.to_string())?;
    stream.set_read_timeout(Some(Duration::from_millis(1000))).ok();

    let mut req = format!("{} {} HTTP/1.1\r\nHost: {}\r\nConnection: close\r\n", method, path, addr);
    for (k, v) in headers {
        req.push_str(&format!("{}: {}\r\n", k, v));
    }
    if !body.is_empty() {
        req.push_str(&format!("Content-Length: {}\r\n", body.len()));
    }
    req.push_str("\r\n");
    stream.write_all(req.as_bytes()).map_err(|e| e.to_string())?;
    stream.write_all(body).map_err(|e| e.to_string())?;

    let mut buf = Vec::new();
    let mut tmp = [0u8; 1024];
    loop {
        let n = stream.read(&mut tmp).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        buf.extend_from_slice(&tmp[..n]);
        if buf.windows(4).any(|w| w == b"\r\n\r\n") {
            break;
        }
    }

    let sep = buf.windows(4).position(|w| w == b"\r\n\r\n").ok_or("no header/body separator")?;
    let head = String::from_utf8_lossy(&buf[..sep]).to_string();
    let status = head.lines().next().and_then(|line| line.split_whitespace().nth(1)).and_then(|s| s.parse::<u16>().ok()).unwrap_or(0);

    let mut body = buf[sep + 4..].to_vec();
    let content_length = head.lines().find_map(|line| {
        if line.to_ascii_lowercase().starts_with("content-length:") { line.split(':').nth(1).and_then(|s| s.trim().parse::<usize>().ok()) } else { None }
    });

    match content_length {
        Some(clen) => {
            while body.len() < clen {
                let n = stream.read(&mut tmp).map_err(|e| e.to_string())?;
                if n == 0 {
                    break;
                }
                body.extend_from_slice(&tmp[..n]);
            }
            body.truncate(clen);
        }
        None => loop {
            let n = stream.read(&mut tmp).map_err(|e| e.to_string())?;
            if n == 0 {
                break;
            }
            body.extend_from_slice(&tmp[..n]);
        },
    }

    Ok((status, body))
}

fn wait_for_history_contains(addr: &str, id: &str, needle: &str) -> Vec<u8> {
    let path = format!("/history/{id}");
    for _ in 0..200 {
        let (status, body) = http_request(addr, "GET", &path, b"", &[]).unwrap();
        assert_eq!(status, 200);
        let text = String::from_utf8_lossy(&body);
        if text.contains(needle) {
            return body;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!("history {id} never contained {needle:?}");
}

fn prompt_body(width: u32, height: u32, text: &str) -> Vec<u8> {
    format!(
        r#"{{"prompt":{{"1":{{"class_type":"EmptyLatentImage","inputs":{{"width":{width},"height":{height},"batch_size":1}}}},"2":{{"class_type":"CLIPTextEncode","inputs":{{"text":"{text}"}}}}}}}}"#
    )
    .into_bytes()
}

fn view_png(addr: &str, filename: &str) -> Vec<u8> {
    let path = format!("/view?filename={filename}");
    let (status, body) = http_request(addr, "GET", &path, b"", &[]).unwrap();
    assert_eq!(status, 200);
    body
}

fn post_prompt(addr: &str, body: &[u8]) -> String {
    let (status, resp) = http_request(addr, "POST", "/prompt", body, &[]).unwrap();
    assert_eq!(status, 200);
    String::from_utf8_lossy(&resp).to_string()
}

// ---------------------------------------------------------------------------
// object_info tests
// ---------------------------------------------------------------------------

#[test]
fn object_info_has_core_nodes() {
    let info = object_info();
    assert!(info.is_object());
    for node in [
        "KSampler",
        "SaveImage",
        "LoadImage",
        "UNETLoader",
        "CheckpointLoaderSimple",
        "CLIPLoader",
        "DualCLIPLoader",
        "VAELoader",
        "EmptyLatentImage",
        "LoraLoader",
        "TextEncodeQwenImageEditPlus",
        "SeedVR2Preprocess",
    ] {
        assert!(info.get(node).is_some(), "missing node {node}");
    }
}

#[test]
fn object_info_lists_fixture_checkpoints_and_unets() {
    let info = object_info();
    let text = info.to_string();

    for file in FIXTURE_CHECKPOINTS {
        assert!(text.contains(file), "checkpoint fixture {file} not listed in object_info");
    }
    for file in FIXTURE_UNETS {
        assert!(text.contains(file), "UNET fixture {file} not listed in object_info");
    }
}

#[test]
fn object_info_with_model_dir_adds_models() {
    let dir = TempDir::new("object_info_model_dir");
    std::fs::create_dir_all(dir.path().join("checkpoints")).unwrap();
    std::fs::create_dir_all(dir.path().join("diffusion_models")).unwrap();
    std::fs::create_dir_all(dir.path().join("loras")).unwrap();
    std::fs::create_dir_all(dir.path().join("text_encoders")).unwrap();
    std::fs::create_dir_all(dir.path().join("vae")).unwrap();

    std::fs::write(dir.path().join("checkpoints/custom_sdxl.safetensors"), b"x").unwrap();
    std::fs::write(dir.path().join("diffusion_models/custom_flux.safetensors"), b"x").unwrap();
    std::fs::write(dir.path().join("loras/custom_lora.safetensors"), b"x").unwrap();
    std::fs::write(dir.path().join("text_encoders/custom_clip.safetensors"), b"x").unwrap();
    std::fs::write(dir.path().join("vae/custom_vae.safetensors"), b"x").unwrap();

    let info = object_info_with(Some(dir.path()));
    let text = info.to_string();

    for needle in ["custom_sdxl.safetensors", "custom_flux.safetensors", "custom_lora.safetensors", "custom_clip.safetensors", "custom_vae.safetensors"] {
        assert!(text.contains(needle), "missing {needle} in object_info_with");
    }
}

#[test]
fn object_info_with_model_dir_dedupes_fixture_files() {
    let dir = TempDir::new("object_info_dedupe");
    std::fs::create_dir_all(dir.path().join("checkpoints")).unwrap();
    // Same file as an already advertised fixture.
    std::fs::write(dir.path().join("checkpoints/juggernautXL_v9.safetensors"), b"x").unwrap();

    let info = object_info_with(Some(dir.path()));
    let text = info.to_string();
    let count = text.matches("juggernautXL_v9.safetensors").count();
    assert_eq!(count, 1, "fixture file duplicated by model_dir listing");
}

// ---------------------------------------------------------------------------
// Server lifecycle and raw HTTP endpoint tests
// ---------------------------------------------------------------------------

#[test]
fn start_on_ephemeral_host_is_reachable_and_stop_cleans_up() {
    let server = MockComfy::start().unwrap();
    let addr = server.host().to_owned();
    assert!(!addr.is_empty());

    // A plain TCP connection succeeds. The temporary is dropped immediately,
    // closing the client connection before the server is stopped.
    let _ = TcpStream::connect(&addr).unwrap();

    drop(server);

    // After drop the server thread is joined and the listener should be closed.
    // To verify the port is free, try to bind a new listener to the same address.
    // If the old listener is still active, binding will fail with AddrInUse.
    let port = addr.rsplit(':').next().and_then(|p| p.parse::<u16>().ok()).expect("port");
    let mut bound = false;
    for _ in 0..50 {
        match std::net::TcpListener::bind(("127.0.0.1", port)) {
            Ok(_) => {
                bound = true;
                break;
            }
            Err(e) if e.kind() == std::io::ErrorKind::AddrInUse => {
                std::thread::sleep(Duration::from_millis(20));
            }
            Err(e) => panic!("unexpected bind error: {e}"),
        }
    }
    assert!(bound, "port {port} is still in use after server drop");
}

#[test]
fn get_object_info_endpoint_matches_public_function() {
    let server = MockComfy::start().unwrap();
    let (status, body) = http_request(server.host(), "GET", "/object_info", b"", &[]).unwrap();
    assert_eq!(status, 200);
    let text = String::from_utf8_lossy(&body);
    assert!(text.contains("\"KSampler\""));
    assert!(text.contains("juggernautXL_v9.safetensors"));
}

#[test]
fn get_system_stats_endpoint_has_mock_gpu() {
    let server = MockComfy::start().unwrap();
    let (status, body) = http_request(server.host(), "GET", "/system_stats", b"", &[]).unwrap();
    assert_eq!(status, 200);
    let text = String::from_utf8_lossy(&body);
    assert!(text.contains("\"comfyui_version\":\"mock\""));
    assert!(text.contains("\"vram_total\":34359738368"));
}

#[test]
fn get_queue_initially_empty() {
    let server = MockComfy::start().unwrap();
    let (status, body) = http_request(server.host(), "GET", "/queue", b"", &[]).unwrap();
    assert_eq!(status, 200);
    let text = String::from_utf8_lossy(&body);
    assert!(text.contains("\"queue_running\":[]"));
    assert!(text.contains("\"queue_pending\":[]"));
}

#[test]
fn upload_image_endpoint_returns_local_image_subfolder() {
    let server = MockComfy::start().unwrap();

    let boundary = "TESTBOUNDARY";
    let dummy_png: Vec<u8> = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0, 0, 0, 0];
    let mut body =
        format!("--{boundary}\r\nContent-Disposition: form-data; name=\"image\"; filename=\"test.png\"\r\nContent-Type: image/png\r\n\r\n").into_bytes();
    body.extend_from_slice(&dummy_png);
    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());

    let (status, resp) = http_request(server.host(), "POST", "/upload/image", &body, &[]).unwrap();
    assert_eq!(status, 200);
    let text = String::from_utf8_lossy(&resp);
    assert!(text.contains("\"name\":\"test.png\""));
    assert!(text.contains("\"subfolder\":\"local-image\""));
    assert!(text.contains("\"type\":\"input\""));
}

#[test]
fn prompt_endpoint_generates_png_and_history() {
    let server = MockComfy::start().unwrap();
    server.delay_ms.store(10, Ordering::SeqCst);

    let body = prompt_body(64, 48, "hello mock");
    let resp = post_prompt(server.host(), &body);
    assert!(resp.contains("\"prompt_id\":\"mock-1\""));

    wait_for_history_contains(server.host(), "mock-1", "\"status_str\":\"success\"");

    let png_bytes = view_png(server.host(), "mock-1.png");
    assert!(png_bytes.starts_with(PNG_SIGNATURE), "view returned non-PNG data");
}

#[test]
fn prompt_endpoint_is_deterministic_for_same_graph() {
    let server = MockComfy::start().unwrap();
    server.delay_ms.store(10, Ordering::SeqCst);

    let body = prompt_body(33, 41, "deterministic");

    post_prompt(server.host(), &body);
    post_prompt(server.host(), &body);

    wait_for_history_contains(server.host(), "mock-1", "\"status_str\":\"success\"");
    wait_for_history_contains(server.host(), "mock-2", "\"status_str\":\"success\"");

    let first = view_png(server.host(), "mock-1.png");
    let second = view_png(server.host(), "mock-2.png");
    assert_eq!(first, second, "identical prompts should render identical PNGs");
}

#[test]
fn fail_next_causes_execution_error() {
    let server = MockComfy::start().unwrap();
    server.delay_ms.store(10, Ordering::SeqCst);
    server.fail_next();

    let body = prompt_body(32, 32, "boom");
    let resp = post_prompt(server.host(), &body);
    assert!(resp.contains("\"prompt_id\":\"mock-1\""));

    wait_for_history_contains(server.host(), "mock-1", "\"status_str\":\"error\"");
    let hist = wait_for_history_contains(server.host(), "mock-1", "CUDA out of memory");
    let text = String::from_utf8_lossy(&hist);
    assert!(text.contains("\"status_str\":\"error\""));
}

#[test]
fn interrupt_endpoint_increments_counter_and_stops_running() {
    let server = MockComfy::start().unwrap();
    server.delay_ms.store(5000, Ordering::SeqCst);

    let body = prompt_body(64, 64, "slow");
    let resp = post_prompt(server.host(), &body);
    assert!(resp.contains("\"prompt_id\":\"mock-1\""));

    std::thread::sleep(Duration::from_millis(100));
    assert_eq!(server.interrupts(), 0);

    let (status, _) = http_request(server.host(), "POST", "/interrupt", b"", &[]).unwrap();
    assert_eq!(status, 200);
    assert_eq!(server.interrupts(), 1);

    wait_for_history_contains(server.host(), "mock-1", "execution_interrupted");
}

#[test]
fn cancel_job_endpoint_marks_history_interrupted() {
    let server = MockComfy::start().unwrap();
    server.delay_ms.store(5000, Ordering::SeqCst);

    let body = prompt_body(64, 64, "cancel me");
    let resp = post_prompt(server.host(), &body);
    assert!(resp.contains("\"prompt_id\":\"mock-1\""));

    let (status, resp) = http_request(server.host(), "POST", "/api/jobs/mock-1/cancel", b"", &[]).unwrap();
    assert_eq!(status, 200);
    assert!(String::from_utf8_lossy(&resp).contains("\"cancelled\":true"));

    wait_for_history_contains(server.host(), "mock-1", "execution_interrupted");
}

#[test]
fn view_missing_file_returns_404() {
    let server = MockComfy::start().unwrap();
    let (status, _) = http_request(server.host(), "GET", "/view?filename=missing.png", b"", &[]).unwrap();
    assert_eq!(status, 404);
}

#[test]
fn history_missing_returns_empty_object() {
    let server = MockComfy::start().unwrap();
    let (status, body) = http_request(server.host(), "GET", "/history/does-not-exist", b"", &[]).unwrap();
    assert_eq!(status, 200);
    assert_eq!(body, b"{}");
}

#[test]
fn unknown_endpoint_returns_404() {
    let server = MockComfy::start().unwrap();
    let (status, _) = http_request(server.host(), "GET", "/no-such-path", b"", &[]).unwrap();
    assert_eq!(status, 404);
}

#[test]
fn set_model_dir_endpoint_includes_files() {
    let dir = TempDir::new("endpoint_model_dir");
    std::fs::create_dir_all(dir.path().join("checkpoints")).unwrap();
    std::fs::write(dir.path().join("checkpoints/endpoint_custom.safetensors"), b"x").unwrap();

    let server = MockComfy::start().unwrap();
    server.set_model_dir(dir.path());

    let (status, body) = http_request(server.host(), "GET", "/object_info", b"", &[]).unwrap();
    assert_eq!(status, 200);
    let text = String::from_utf8_lossy(&body);
    assert!(text.contains("endpoint_custom.safetensors"));
}

#[test]
fn empty_and_odd_sized_prompts_render_without_panic() {
    let server = MockComfy::start().unwrap();
    server.delay_ms.store(10, Ordering::SeqCst);

    let tiny = prompt_body(1, 1, "tiny");
    let odd = prompt_body(7, 13, "odd");

    post_prompt(server.host(), &tiny);
    post_prompt(server.host(), &odd);

    wait_for_history_contains(server.host(), "mock-1", "\"status_str\":\"success\"");
    wait_for_history_contains(server.host(), "mock-2", "\"status_str\":\"success\"");

    let png1 = view_png(server.host(), "mock-1.png");
    let png2 = view_png(server.host(), "mock-2.png");
    assert!(png1.starts_with(PNG_SIGNATURE));
    assert!(png2.starts_with(PNG_SIGNATURE));
}

#[test]
fn prompts_records_submitted_graphs() {
    let server = MockComfy::start().unwrap();
    server.delay_ms.store(10, Ordering::SeqCst);

    let body = prompt_body(24, 16, "recorded");
    post_prompt(server.host(), &body);

    let prompts = server.prompts();
    assert_eq!(prompts.len(), 1);
    let class = prompts[0].get("1").and_then(|n| n.get("class_type")).and_then(|v| v.as_str());
    assert_eq!(class, Some("EmptyLatentImage"));
}
