use image::{GrayImage, Luma, RgbaImage};
use li_ai::ops::new_seed;
use li_ai::{GenerateMode, GenerateRequest, ModelId, RemoveEngine};

fn qwen_request(prompt: &str) -> GenerateRequest {
    GenerateRequest::new(ModelId::Qwen, prompt)
}

fn rgba_1x1() -> RgbaImage {
    RgbaImage::from_pixel(1, 1, image::Rgba([10, 20, 30, 255]))
}

fn gray_1x1_white() -> GrayImage {
    GrayImage::from_pixel(1, 1, Luma([255]))
}

#[test]
fn new_seed_is_48_bits() {
    for _ in 0..100 {
        let s = new_seed();
        assert!(s < (1 << 48), "seed {s} exceeds 48 bits");
    }
}

#[test]
fn new_seed_varies() {
    let seeds: Vec<u64> = (0..10).map(|_| new_seed()).collect();
    assert!(seeds.iter().any(|&s| s != seeds[0]), "seeds should vary");
}

#[test]
fn remove_engine_eq() {
    assert_eq!(RemoveEngine::Klein, RemoveEngine::Klein);
    let q1 = RemoveEngine::Qwen { variant: "bf16".into() };
    let q2 = RemoveEngine::Qwen { variant: "bf16".into() };
    assert_eq!(q1, q2);
    assert_ne!(RemoveEngine::Klein, q1);
}

#[test]
fn generate_mode_default_is_create() {
    assert_eq!(GenerateMode::default(), GenerateMode::Create);
    let mut m = GenerateMode::Edit;
    let m2 = m;
    m = GenerateMode::Refine;
    assert_eq!(m2, GenerateMode::Edit);
    assert_ne!(m, m2);
}

#[test]
fn request_new_sets_defaults() {
    let req = qwen_request("hello");
    assert_eq!(req.model, ModelId::Qwen);
    assert!(!req.variant.is_empty());
    assert_eq!(req.mode, GenerateMode::Create);
    assert_eq!(req.prompt, "hello");
    assert_eq!(req.negative, "");
    assert_eq!(req.width, 1024);
    assert_eq!(req.height, 1024);
    assert!(!req.transparent);
    assert_eq!(req.denoise, 0.6);
    assert!(req.source.is_none());
    assert!(req.mask.is_none());
    assert!(req.references.is_empty());
    assert!(req.loras.is_empty());
    assert!(req.sampler.is_none());
    assert!(req.refine.is_none());
    assert_eq!(req.scale, 2.0);
    assert!(req.steps > 0);
    assert!(req.guidance.is_finite());
}

#[test]
fn request_clone_is_independent() {
    let mut req = qwen_request("a");
    req.references.push(rgba_1x1());
    let clone = req.clone();
    req.references.clear();
    assert_eq!(clone.references.len(), 1);
    assert!(req.references.is_empty());
}

#[test]
fn valid_create_request_passes_validation() {
    let req = qwen_request("a cat");
    assert!(req.validate().is_ok());
}

#[test]
fn empty_prompt_is_rejected() {
    let req = qwen_request("");
    let err = req.validate().unwrap_err();
    assert!(err.to_string().contains("Describe the image first."));
}

#[test]
fn overlong_prompt_is_rejected() {
    let long = "x".repeat(4001);
    let req = qwen_request(&long);
    let err = req.validate().unwrap_err();
    assert!(err.to_string().contains("longer than 4000"));
}

#[test]
fn edit_without_source_is_rejected() {
    let mut req = qwen_request("edit me");
    req.mode = GenerateMode::Edit;
    let err = req.validate().unwrap_err();
    assert!(err.to_string().contains("Open an image first."));
}

#[test]
fn inpaint_without_mask_is_rejected() {
    let mut req = qwen_request("fill this");
    req.mode = GenerateMode::Inpaint;
    req.source = Some(rgba_1x1());
    let err = req.validate().unwrap_err();
    assert!(err.to_string().contains("Make a selection to fill."));
}

#[test]
fn mask_size_mismatch_is_rejected() {
    let mut req = qwen_request("fill this");
    req.mode = GenerateMode::Inpaint;
    req.source = Some(RgbaImage::from_pixel(2, 2, image::Rgba([0, 0, 0, 255])));
    req.mask = Some(gray_1x1_white());
    let err = req.validate().unwrap_err();
    assert!(err.to_string().contains("does not match the image size"));
}

/// Steps have no built-in range (ComfyUI checks its own limits); only zero is refused.
#[test]
fn steps_beyond_the_model_recommendation_are_accepted_and_zero_is_rejected() {
    let mut req = qwen_request("a cat");
    req.steps = 500;
    req.validate().unwrap();
    req.steps = 0;
    let err = req.validate().unwrap_err();
    assert!(err.to_string().contains("Steps"));
}

#[test]
fn too_many_references_is_rejected() {
    let mut req = qwen_request("a cat");
    req.references = (0..20).map(|_| rgba_1x1()).collect();
    let err = req.validate().unwrap_err();
    assert!(err.to_string().contains("reference"));
}

#[test]
fn valid_edit_request_passes_validation() {
    let mut req = qwen_request("edit this");
    req.mode = GenerateMode::Edit;
    req.source = Some(rgba_1x1());
    assert!(req.validate().is_ok());
}

#[test]
fn valid_inpaint_request_passes_validation() {
    let mut req = qwen_request("fill this");
    req.mode = GenerateMode::Inpaint;
    req.source = Some(rgba_1x1());
    req.mask = Some(gray_1x1_white());
    assert!(req.validate().is_ok());
}

#[test]
fn refine_without_source_is_rejected() {
    let mut req = qwen_request("");
    req.mode = GenerateMode::Refine;
    let err = req.validate().unwrap_err();
    assert!(err.to_string().contains("Open an image first."));
}

#[test]
fn upscalerefine_without_source_is_rejected() {
    let mut req = qwen_request("");
    req.mode = GenerateMode::UpscaleRefine;
    let err = req.validate().unwrap_err();
    assert!(err.to_string().contains("Open an image first."));
}

#[test]
fn edit_uses_first_reference_as_source() {
    let mut req = qwen_request("edit");
    req.mode = GenerateMode::Edit;
    req.source = None;
    req.references = vec![rgba_1x1()];
    assert!(req.validate().is_ok());
}

#[test]
fn validate_does_not_panic_on_extreme_values() {
    let mut req = qwen_request("a cat");
    req.width = 0;
    req.height = 0;
    req.guidance = f32::NAN;
    req.denoise = f32::INFINITY;
    req.scale = f32::NEG_INFINITY;
    let _ = req.validate();
}
