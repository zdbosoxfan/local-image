//! A stand-in ComfyUI for demos and screenshots: `cargo run -p li-ai --example mock_comfy -- 8199`
//! then point Local Image at it with `LOCAL_IMAGE_COMFY=127.0.0.1:8199`.

fn main() {
    let port = std::env::args().nth(1).unwrap_or_else(|| "8199".into());
    let server = li_ai::mock::MockComfy::start_on(&format!("127.0.0.1:{port}")).expect("start the mock server");
    println!("mock ComfyUI at {}", server.host());
    loop {
        std::thread::sleep(std::time::Duration::from_secs(3600));
    }
}
