use super::*;
use crate::mock::MockComfy;
use image::Rgba;

fn sampler(graph: &serde_json::Value) -> &serde_json::Value {
    &graph.as_object().unwrap().values().find(|node| node["class_type"] == "KSampler").unwrap()["inputs"]
}

#[test]
fn review_generation_controls_match_submitted_graph_and_metadata() {
    let server = MockComfy::start().unwrap();
    let ai = Ai::new(server.host());
    let ctl = JobControl::new();
    let source = RgbaImage::from_pixel(32, 32, Rgba([80, 120, 200, 255]));
    for mode in [GenerateMode::Create, GenerateMode::Edit] {
        let mut request = GenerateRequest::new(ModelId::Qwen, "synthetic edit");
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
    let mut request = GenerateRequest::new(ModelId::ZImageTurbo, "synthetic variation");
    request.width = 256;
    request.height = 256;
    request.references = vec![source];
    for denoise in [0.05, 0.95] {
        request.denoise = denoise;
        let generated = ai.generate_detailed(&request, &ctl).unwrap();
        let graphs = server.prompts();
        let graph = graphs.last().unwrap();
        assert!(graph.as_object().unwrap().values().any(|node| node["class_type"] == "VAEEncode"));
        assert_eq!(sampler(graph)["denoise"], serde_json::json!(denoise));
        assert_eq!(generated.parameters["denoise"], serde_json::json!(denoise));
    }
    request.references.clear();
    let generated = ai.generate_detailed(&request, &ctl).unwrap();
    let graphs = server.prompts();
    assert_eq!(sampler(graphs.last().unwrap())["denoise"], 1.0);
    assert_eq!(generated.parameters["denoise"], 1.0);
}

#[test]
fn review_qwen_patch_applies_soft_coverage_exactly_once() {
    let server = MockComfy::start().unwrap();
    let ai = Ai::new(server.host());
    let source = RgbFrame::from_fn(8, 8, |_, _| [0.0; 3]);
    let mask = GrayImage::from_fn(8, 8, |x, y| Luma([if (3..5).contains(&x) && (3..5).contains(&y) { 128 } else { 0 }]));
    let image = RgbaImage::from_pixel(8, 8, Rgba([200, 100, 0, 255]));
    let engine = RemoveEngine::Qwen { variant: "int8".into() };
    server.next_output(imaging::encode_png(&image).unwrap());
    let patch = ai.remove_patch_frame(&source, &mask, &engine, 1, &JobControl::new()).unwrap();
    server.next_output(imaging::encode_png(&image).unwrap());
    let standalone = ai.remove_objects_frame(&source, &mask, &engine, 1, &JobControl::new()).unwrap();
    for y in 0..8 {
        for x in 0..8 {
            let m = f32::from(mask.get_pixel(x as u32, y as u32)[0]) / 255.0;
            for c in 0..3 {
                assert!((standalone.get(x, y)[c] - patch.get(x, y)[c] * m).abs() < 1e-6);
            }
        }
    }
    assert!((patch.get(3, 3)[0] - 200.0 / 255.0).abs() < 1e-6);
    assert_eq!(patch.get(0, 0), source.get(0, 0));
}

#[test]
fn review_empty_background_validates_raw_alpha_before_flattening() {
    let server = MockComfy::start().unwrap();
    let ai = Ai::new(server.host());
    for alpha in [0, 128] {
        let image = RgbaImage::from_pixel(16, 16, Rgba([90, 120, 150, alpha]));
        server.next_output(imaging::encode_png(&image).unwrap());
        let error = ai.generate_background("empty room", 256, 256, "int8", 1, &JobControl::new()).unwrap_err();
        assert!(error.to_string().contains("transparent subject"));
    }
    let image = RgbaImage::from_fn(16, 16, |x, y| Rgba([90, 120, 150, if x == 0 && y == 0 { 0 } else { 255 }]));
    server.next_output(imaging::encode_png(&image).unwrap());
    let out = ai.generate_background("empty room", 256, 256, "int8", 1, &JobControl::new()).unwrap();
    assert!(out.pixels().all(|p| p[3] == 255));
    assert_eq!(out.get_pixel(1, 1), image.get_pixel(1, 1));
    // Ordinary opaque Create retains its established white-flattening behavior.
    server.next_output(imaging::encode_png(&RgbaImage::new(16, 16)).unwrap());
    let out = ai.generate(&GenerateRequest::new(ModelId::Qwen, "ordinary create"), &JobControl::new()).unwrap();
    assert!(out.pixels().all(|p| *p == Rgba([255; 4])));
}
