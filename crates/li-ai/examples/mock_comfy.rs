//! A stand-in ComfyUI for demos and screenshots, with the model sources (Hugging Face, Civitai,
//! the templates and the community list) under the same address:
//!
//! ```sh
//! cargo run -p li-ai --example mock_comfy -- 8199 /tmp/li-models
//! LOCAL_IMAGE_COMFY=127.0.0.1:8199 LOCAL_IMAGE_TEST_DOWNLOAD_HOST=127.0.0.1:8199 \
//!   LOCAL_IMAGE_HF_BASE=http://127.0.0.1:8199/hf LOCAL_IMAGE_CIVITAI_BASE=http://127.0.0.1:8199/civitai \
//!   LOCAL_IMAGE_MANAGER_LIST=http://127.0.0.1:8199/manager/model-list.json local-image
//! ```
//!
//! The optional folder is listed as ComfyUI's model folder, so installs from the Model Browser
//! show up (set Local AI › Model folder to the same path).

fn main() {
    let mut args = std::env::args().skip(1);
    let port = args.next().unwrap_or_else(|| "8199".into());
    let server = li_ai::mock::MockComfy::start_on(&format!("127.0.0.1:{port}")).expect("start the mock server");
    if let Some(dir) = args.next() {
        server.set_model_dir(dir);
    }
    println!("mock ComfyUI at {}", server.host());
    loop {
        std::thread::sleep(std::time::Duration::from_secs(3600));
    }
}
