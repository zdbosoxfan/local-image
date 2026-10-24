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
//!
//! `--comfy-0.42.5` serves ComfyUI 0.42.5's nodes and official templates instead, and fills an
//! empty model folder with header-only copies of an RTX 5090 workstation's models (FLUX.3 Klein,
//! MOSS-Image, Krea 2, Qwen Image 2.1, FLUX.2 Klein 9B and their text encoders and VAEs), so Set
//! Up Models has real files to identify, match and check.

/// `folder/file` ← header fixture (`tests/fixtures/headers`).
const WORKSTATION: &[(&str, &str)] = &[
    ("diffusion_models/flux3-image-klein-bf16.safetensors", "owner__diffusion_models~flux3-image-klein-bf16"),
    ("diffusion_models/flux3-image-klein-base-bf16.safetensors", "owner__diffusion_models~flux3-image-klein-base-bf16"),
    ("diffusion_models/moss_image_sft_bf16.safetensors", "owner__diffusion_models~moss_image_sft_bf16"),
    ("diffusion_models/krea2_turbo_bf16.safetensors", "Comfy-Org_Krea-2__diffusion_models~krea2_turbo_bf16"),
    ("diffusion_models/qwen_image_2.1_bf16.safetensors", "local__diffusion_models~qwen_image_2.1_bf16"),
    ("diffusion_models/flux-2-klein-9b-fp8.safetensors", "local__diffusion_models~flux-2-klein-9b-fp8"),
    ("diffusion_models/ming_image_0.1_design_bf16.safetensors", "Comfy-Org_Ming-Image__diffusion_models~ming_image_0.1_design_bf16"),
    ("text_encoders/qwen3vl_4b_bf16.safetensors", "Comfy-Org_Krea-2__text_encoders~qwen3vl_4b_bf16"),
    ("text_encoders/qwen3vl_8b_bf16.safetensors", "local__text_encoders~qwen3vl_8b_bf16"),
    ("text_encoders/qwen_3_8b_fp8mixed.safetensors", "local__text_encoders~qwen_3_8b_fp8mixed"),
    ("vae/flux2-vae.safetensors", "local__vae~flux2-vae"),
    ("vae/qwen_image_vae.safetensors", "Comfy-Org_Krea-2__vae~qwen_image_vae"),
    ("vae/qwen_image_2.1_vae_bf16.safetensors", "local__vae~qwen_image_2.1_vae_bf16"),
];

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let comfy_0_42_5 = args.iter().any(|a| a == "--comfy-0.42.5");
    let mut plain = args.iter().filter(|a| !a.starts_with("--"));
    let port = plain.next().cloned().unwrap_or_else(|| "8199".into());
    let server = li_ai::mock::MockComfy::start_on(&format!("127.0.0.1:{port}")).expect("start the mock server");
    if let Some(dir) = plain.next().map(std::path::PathBuf::from) {
        if comfy_0_42_5 {
            if std::fs::read_dir(&dir).map_or(true, |mut d| d.next().is_none()) {
                for (file, fixture) in WORKSTATION {
                    li_ai::mock::write_header_only(&dir.join(file), &li_ai::mock::header_fixture(fixture)).expect("write a header-only model file");
                }
            }
            server.serve_comfy_0_42_5(&dir);
        } else {
            server.set_model_dir(dir);
        }
    }
    println!("mock ComfyUI at {}", server.host());
    loop {
        std::thread::sleep(std::time::Duration::from_secs(3600));
    }
}
