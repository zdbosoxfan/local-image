use image::{GrayImage, Luma, Rgba, RgbaImage};
use li_ai::cloud::{self, CloudMode, CloudRequest, MODELS, Provider, api_key, describe_size, generate, host_allowed, mask_for, model};
use li_ai::{AiSettings, JobControl};
use std::collections::HashSet;

fn base_req(model: &'static cloud::CloudModel, mode: CloudMode) -> CloudRequest {
    CloudRequest {
        model,
        mode,
        prompt: String::new(),
        negative: String::new(),
        width: 1024,
        height: 1024,
        seed: 0,
        count: 1,
        source: None,
        mask: None,
        refs: Vec::new(),
        strength: 0.5,
    }
}

#[test]
fn provider_constants_are_sane() {
    assert_eq!(Provider::ALL.len(), 4);
    for p in Provider::ALL {
        assert!(!p.label().is_empty());
        assert!(!p.key_name().is_empty());
        assert!(!p.keys_url().is_empty());
        assert!(p.keys_url().starts_with("https://"));
    }
}

#[test]
fn model_lookup_works() {
    for m in MODELS {
        let found = model(m.key).expect("model should be found");
        assert_eq!(*found, *m);
    }
    assert!(model("nonexistent:model").is_none());
}

#[test]
fn models_have_unique_keys() {
    let keys: HashSet<_> = MODELS.iter().map(|m| m.key).collect();
    assert_eq!(keys.len(), MODELS.len());
}

#[test]
fn models_declare_at_least_one_capability() {
    assert!(MODELS.iter().all(|m| m.create || m.edit || m.fill));
}

#[test]
fn model_keys_match_provider_prefixes() {
    for m in MODELS {
        let prefix = match m.provider {
            Provider::OpenAi => "openai:",
            Provider::Google => "google:",
            Provider::Bfl => "bfl:",
            Provider::Stability => "stability:",
        };
        assert!(m.key.starts_with(prefix), "{} should start with {}", m.key, prefix);
    }
}

#[test]
fn host_allowed_allows_expected_https() {
    assert!(host_allowed("https://api.openai.com/v1/images/generations"));
    assert!(host_allowed("https://generativelanguage.googleapis.com/v1beta/models/x:generateContent"));
    assert!(host_allowed("https://api.bfl.ai/v1/flux-pro-1.1"));
    assert!(host_allowed("https://api.stability.ai/v2beta/stable-image/generate/sd3"));
    assert!(host_allowed("https://delivery-eu1.bfl.ai/results/abc.png"));
}

#[test]
fn host_allowed_rejects_unknown_https() {
    assert!(!host_allowed("https://evil.example/v1"));
    assert!(!host_allowed("https://api.openai.com.evil.example/v1"));
}

#[test]
fn host_allowed_rejects_non_https_non_http() {
    assert!(!host_allowed("ftp://api.openai.com/v1"));
    assert!(!host_allowed("javascript:alert(1)"));
}

#[test]
fn host_allowed_rejects_plain_http_provider() {
    assert!(!host_allowed("http://api.openai.com/v1"));
}

#[test]
fn cloud_request_validate_rejects_unsupported_create() {
    let m = model("bfl:flux-pro-1.0-fill").unwrap();
    let req = base_req(m, CloudMode::Create);
    let err = req.validate().unwrap_err().to_string();
    assert!(err.contains("doesn't create"), "{err}");
}

#[test]
fn cloud_request_validate_rejects_unsupported_edit() {
    let m = model("bfl:flux-pro-1.1").unwrap();
    let req = base_req(m, CloudMode::Edit);
    let err = req.validate().unwrap_err().to_string();
    assert!(err.contains("doesn't edit"), "{err}");
}

#[test]
fn cloud_request_validate_rejects_unsupported_fill() {
    let m = model("stability:sd3.5-large").unwrap();
    let req = base_req(m, CloudMode::Fill);
    let err = req.validate().unwrap_err().to_string();
    assert!(err.contains("doesn't fill"), "{err}");
}

#[test]
fn cloud_request_validate_requires_source_for_edit() {
    let m = model("openai:gpt-image-1").unwrap();
    let req = base_req(m, CloudMode::Edit);
    let err = req.validate().unwrap_err().to_string();
    assert!(err.contains("Open an image first."), "{err}");
}

#[test]
fn cloud_request_validate_requires_source_for_fill() {
    let m = model("bfl:flux-pro-1.0-fill").unwrap();
    let req = base_req(m, CloudMode::Fill);
    let err = req.validate().unwrap_err().to_string();
    assert!(err.contains("Open an image first."), "{err}");
}

#[test]
fn cloud_request_validate_requires_mask_for_fill() {
    let m = model("bfl:flux-pro-1.0-fill").unwrap();
    let src = RgbaImage::from_pixel(1, 1, Rgba([0, 0, 0, 255]));
    let mut req = base_req(m, CloudMode::Fill);
    req.source = Some(src);
    let err = req.validate().unwrap_err().to_string();
    assert!(err.contains("Make a selection to fill."), "{err}");
}

#[test]
fn cloud_request_validate_rejects_too_many_refs() {
    let m = model("bfl:flux-pro-1.1").unwrap(); // max_refs = 0
    let mut req = base_req(m, CloudMode::Create);
    req.refs.push(RgbaImage::from_pixel(1, 1, Rgba([0, 0, 0, 255])));
    let err = req.validate().unwrap_err().to_string();
    assert!(err.contains("at most 0 reference images"), "{err}");
}

#[test]
fn cloud_request_validate_requires_prompt_for_create_and_edit() {
    let m_create = model("openai:gpt-image-1").unwrap();
    let req = base_req(m_create, CloudMode::Create);
    let err = req.validate().unwrap_err().to_string();
    assert!(err.contains("Describe what you want first."), "{err}");

    let m_edit = model("openai:gpt-image-1").unwrap();
    let src = RgbaImage::from_pixel(1, 1, Rgba([0, 0, 0, 255]));
    let mut req = base_req(m_edit, CloudMode::Edit);
    req.source = Some(src);
    let err = req.validate().unwrap_err().to_string();
    assert!(err.contains("Describe what you want first."), "{err}");
}

#[test]
fn cloud_request_validate_allows_empty_prompt_for_fill() {
    let m = model("bfl:flux-pro-1.0-fill").unwrap();
    let src = RgbaImage::from_pixel(1, 1, Rgba([0, 0, 0, 255]));
    let mask = GrayImage::from_pixel(1, 1, Luma([255]));
    let mut req = base_req(m, CloudMode::Fill);
    req.source = Some(src);
    req.mask = Some(mask);
    req.prompt.clear();
    assert!(req.validate().is_ok());
}

#[test]
fn cloud_request_validate_accepts_all_supported_modes() {
    let src = RgbaImage::from_pixel(1, 1, Rgba([0, 0, 0, 255]));
    let mask = GrayImage::from_pixel(1, 1, Luma([255]));

    for m in MODELS {
        for mode in [CloudMode::Create, CloudMode::Edit, CloudMode::Fill] {
            let supported = match mode {
                CloudMode::Create => m.create,
                CloudMode::Edit => m.edit,
                CloudMode::Fill => m.fill,
            };
            if !supported {
                continue;
            }

            let mut req = base_req(m, mode);
            match mode {
                CloudMode::Create => {
                    req.prompt = "x".into();
                }
                CloudMode::Edit => {
                    req.prompt = "x".into();
                    req.source = Some(src.clone());
                }
                CloudMode::Fill => {
                    req.prompt.clear();
                    req.source = Some(src.clone());
                    req.mask = Some(mask.clone());
                }
            }

            assert!(req.validate().is_ok(), "{} {:?} should validate", m.key, mode);
        }
    }
}

#[test]
fn mask_for_identity_dimensions_returns_clone() {
    let mask = GrayImage::from_fn(4, 3, |x, y| Luma([(x * 10 + y) as u8]));
    let scaled = mask_for(&mask, 4, 3);
    assert_eq!(scaled, mask);
}

#[test]
fn mask_for_downscale_maps_coordinates() {
    let mask = GrayImage::from_fn(4, 2, |x, y| Luma([(x + y * 4) as u8]));
    let scaled = mask_for(&mask, 2, 1);
    assert_eq!(scaled.dimensions(), (2, 1));
    assert_eq!(scaled.get_pixel(0, 0)[0], mask.get_pixel(0, 0)[0]);
    assert_eq!(scaled.get_pixel(1, 0)[0], mask.get_pixel(2, 0)[0]);
}

#[test]
fn mask_for_upscale_maps_coordinates() {
    let mask = GrayImage::from_fn(2, 1, |x, _| Luma([x as u8]));
    let scaled = mask_for(&mask, 4, 1);
    assert_eq!(scaled.dimensions(), (4, 1));
    assert_eq!(scaled.get_pixel(0, 0)[0], mask.get_pixel(0, 0)[0]);
    assert_eq!(scaled.get_pixel(1, 0)[0], mask.get_pixel(0, 0)[0]);
    assert_eq!(scaled.get_pixel(2, 0)[0], mask.get_pixel(1, 0)[0]);
    assert_eq!(scaled.get_pixel(3, 0)[0], mask.get_pixel(1, 0)[0]);
}

#[test]
fn describe_size_openai() {
    let m = model("openai:gpt-image-1").unwrap();
    assert_eq!(describe_size(m, 1920, 1080), "1536 × 1024");
    assert_eq!(describe_size(m, 800, 1200), "1024 × 1536");
    assert_eq!(describe_size(m, 1000, 1000), "1024 × 1024");
}

#[test]
fn describe_size_google_ratio() {
    let m = model("google:gemini-2.5-flash-image").unwrap();
    assert_eq!(describe_size(m, 1920, 1080), "16:9");
    assert_eq!(describe_size(m, 800, 1200), "2:3");
}

#[test]
fn describe_size_bfl_ratio() {
    let m = model("bfl:flux-pro-1.1").unwrap();
    assert_eq!(describe_size(m, 1920, 1080), "16:9");
    assert_eq!(describe_size(m, 1000, 1000), "1:1");
}

#[test]
fn describe_size_stability_ratio() {
    let m = model("stability:ultra").unwrap();
    assert_eq!(describe_size(m, 1920, 1080), "16:9");
    assert_eq!(describe_size(m, 1000, 1000), "1:1");
}

#[test]
fn api_key_returns_set_value() {
    let mut settings = AiSettings::default();
    settings.set_extra_string("openai_api_key", "my-key");
    assert_eq!(api_key(&settings, Provider::OpenAi).as_deref(), Some("my-key"));
}

#[test]
fn api_key_returns_none_by_default() {
    let settings = AiSettings::default();
    assert_eq!(api_key(&settings, Provider::Google), None);
}

#[test]
fn generate_missing_api_key_returns_error() {
    let m = model("openai:gpt-image-1").unwrap();
    let mut req = base_req(m, CloudMode::Create);
    req.prompt = "test prompt".into();
    let settings = AiSettings::default();
    let ctl = JobControl::new();
    let err = generate(&req, &settings, &ctl).unwrap_err().to_string();
    assert!(err.contains("Add your") && err.contains("Local AI › Cloud"), "{err}");
}

#[test]
fn generate_rejects_invalid_request_before_checking_key() {
    let m = model("bfl:flux-pro-1.0-fill").unwrap();
    let req = base_req(m, CloudMode::Create);
    let settings = AiSettings::default();
    let ctl = JobControl::new();
    let err = generate(&req, &settings, &ctl).unwrap_err().to_string();
    assert!(err.contains("doesn't create"), "{err}");
}
