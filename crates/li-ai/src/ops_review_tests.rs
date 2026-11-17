use super::*;
use crate::mock::MockComfy;
use image::Rgba;

fn sampler(graph: &serde_json::Value) -> &serde_json::Value {
    &graph.as_object().unwrap().values().find(|node| node["class_type"] == "KSampler").unwrap()["inputs"]
}

#[test]
fn review_generation_controls_match_submitted_graph_and_metadata() {
    let _inventory = crate::inventory::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let server = MockComfy::start().unwrap();
    let ai = Ai::new(server.host());
    let ctl = JobControl::new();
    crate::inventory::refresh(&crate::comfy::ObjectInfo(crate::mock::object_info()), None);
    let edit_model = ModelId::from_key("unet:qwen_image_edit_2511_bf16.safetensors").unwrap();
    let source = RgbaImage::from_pixel(32, 32, Rgba([80, 120, 200, 255]));
    for mode in [GenerateMode::Create, GenerateMode::Edit] {
        let mut request = GenerateRequest::new(edit_model, "synthetic edit");
        request.variant = "installed".into();
        request.width = 256;
        request.height = 256;
        request.mode = mode;
        request.sampler = Some(("dpmpp_2m".into(), "karras".into()));
        if mode == GenerateMode::Edit {
            request.source = Some(source.clone());
        }
        let generated = ai.generate_detailed(&request, &ctl).unwrap();
        let graphs = server.prompts();
        let submitted = sampler(graphs.last().unwrap());
        for (input, metadata, expected) in [("sampler_name", "sampler", "dpmpp_2m"), ("scheduler", "scheduler", "karras")] {
            assert_eq!(submitted[input], expected);
            assert_eq!(generated.parameters[metadata], expected);
        }
        assert_eq!(generated.parameters["denoise"], 1.0);
    }
    crate::catalog::set_installed(Vec::new());
}

#[test]
fn generative_fill_runs_klein_inpaint_and_keeps_unselected_pixels_exact() {
    let server = MockComfy::start().unwrap();
    let ai = Ai::new(server.host());
    let source = RgbFrame::from_fn(64, 48, |x, y| [0.123_456 + x as f32 * 1e-4, 0.5, 0.25 + y as f32 * 1e-4]);
    let mask = GrayImage::from_fn(64, 48, |x, y| Luma([if (16..48).contains(&x) && (12..36).contains(&y) { 255 } else { 0 }]));
    let out = ai.fill_frame(&source, &mask, "a red ball", 7, &JobControl::new()).unwrap();
    let graph = server.prompts().last().unwrap().clone();
    let nodes = graph.as_object().unwrap();
    assert!(nodes.values().any(|n| n["class_type"] == "ReferenceLatent"), "Klein's instruction inpaint");
    let prompt = format!("{} a red ball", crate::inpaint::GREEN_INSTRUCTION);
    assert!(nodes.values().any(|n| n["class_type"] == "CLIPTextEncode" && n["inputs"]["text"] == prompt.as_str()));
    assert_eq!(out.dimensions(), source.dimensions());
    for y in 0..48 {
        for x in 0..64 {
            if mask.get_pixel(x as u32, y as u32)[0] == 0 {
                assert_eq!(out.get(x, y), source.get(x, y));
            }
        }
    }
    assert_ne!(out.get(32, 24), source.get(32, 24));
}

#[test]
fn generate_background_asks_klein_for_an_opaque_empty_plate() {
    let server = MockComfy::start().unwrap();
    let ai = Ai::new(server.host());
    let image = RgbaImage::from_fn(16, 16, |x, y| Rgba([90, 120, 150, if x == 0 && y == 0 { 0 } else { 255 }]));
    server.next_output(imaging::encode_png(&image).unwrap());
    let out = ai.generate_background("empty room", 256, 256, 1, &JobControl::new()).unwrap();
    assert!(out.pixels().all(|p| p[3] == 255));
    assert_eq!(out.get_pixel(1, 1), image.get_pixel(1, 1));
    let graph = server.prompts().last().unwrap().clone();
    let nodes = graph.as_object().unwrap();
    assert!(nodes.values().any(|n| n["class_type"] == "EmptyFlux2LatentImage"));
    let text = nodes.values().find_map(|n| n["inputs"]["text"].as_str().filter(|t| t.ends_with("Background description: empty room"))).unwrap();
    assert!(text.contains("Do not include people") && text.contains("furniture in the foreground"));
}
