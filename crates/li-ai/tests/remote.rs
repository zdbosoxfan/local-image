//! Server transport acceptance without downloading or running large models.
use image::{DynamicImage, Rgba, RgbaImage};
use li_ai::{AiSettings, ComfyClient, JobControl};
use serde_json::json;

#[test]
fn remote_mode_transfers_images_and_results_without_shared_model_folder() {
    let server = li_ai::mock::MockComfy::start().unwrap();
    let port = server.host().rsplit(':').next().unwrap().parse().unwrap();
    let mut settings = AiSettings::default();
    settings.comfy_host = "localhost".into();
    settings.comfy_port = port;
    settings.setup_mode = "remote".into();
    assert!(settings.is_remote(), "loopback SSH tunnels are explicitly remote");
    let client = ComfyClient::new(settings.host());
    assert!(client.ping());
    assert!(client.object_info().unwrap().has_node("LoadImage"));
    let source = DynamicImage::ImageRgba8(RgbaImage::from_pixel(16, 16, Rgba([30, 90, 150, 255])));
    let mut png = std::io::Cursor::new(Vec::new());
    source.write_to(&mut png, image::ImageFormat::Png).unwrap();
    let graph = json!({
        "1": {"class_type": "LoadImage", "inputs": {"image": "placeholder.png"}},
        "2": {"class_type": "SaveImage", "inputs": {"images": ["1", 0], "filename_prefix": "remote-test"}}
    });
    let result = client.run(graph, &[("1".into(), png.into_inner())], &JobControl::new()).unwrap();
    assert!(image::load_from_memory(&result).is_ok());
    assert_eq!(server.prompts().len(), 1);
}
