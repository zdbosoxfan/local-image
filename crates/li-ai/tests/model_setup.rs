//! Model setup end to end against the mock ComfyUI serving ComfyUI 0.42.5's nodes and official
//! templates (`tests/fixtures/comfy-0.42.5`), with a temporary model folder of header-only
//! weights: identification, template matching, static checks, the stored state, the catalogue,
//! verification renders and the report.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use image::{Rgb, RgbImage};
use li_ai::catalog::ModelId;
use li_ai::comfy::JobControl;
use li_ai::family::{self, Registry};
use li_ai::mock::MockComfy;
use li_ai::modelsetup::{self, Options, Scan, state::Status};
use serde_json::{Value, json};

static SERIAL: Mutex<()> = Mutex::new(());

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

struct Dir(PathBuf);

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// A model folder with these `folder/file` ← header fixture entries.
fn model_dir(files: &[(&str, &str)]) -> Dir {
    let d = std::env::temp_dir().join(format!("li-setup-{}", uuid::Uuid::new_v4().simple()));
    for (file, fixture) in files {
        li_ai::mock::write_header_only(&d.join(file), &li_ai::mock::header_fixture(fixture)).unwrap();
    }
    Dir(d)
}

/// ComfyUI 0.42.5's nodes listing the folder's files.
fn object_info(dir: &Path) -> Value {
    let mut info: Value = serde_json::from_slice(&std::fs::read(fixtures().join("comfy-0.42.5/object_info.min.json")).unwrap()).unwrap();
    let list = |folder: &str| -> Vec<String> {
        std::fs::read_dir(dir.join(folder)).map(|r| r.filter_map(|e| e.ok()?.file_name().into_string().ok()).collect()).unwrap_or_default()
    };
    info["UNETLoader"]["input"]["required"]["unet_name"] = json!([list("diffusion_models")]);
    info["CLIPLoader"]["input"]["required"]["clip_name"] = json!([list("text_encoders")]);
    info["VAELoader"]["input"]["required"]["vae_name"] = json!([list("vae")]);
    info["LoraLoaderModelOnly"]["input"]["required"]["lora_name"] = json!([list("loras")]);
    info
}

fn system_stats(version: &str, gpu: &str) -> Value {
    let mut s: Value = serde_json::from_slice(&std::fs::read(fixtures().join("comfy-0.42.5/system_stats.json")).unwrap()).unwrap();
    s["system"]["comfyui_version"] = json!(version);
    s["devices"][0]["name"] = json!(format!("cuda:0 {gpu} : cudaMallocAsync"));
    s
}

struct Setup {
    mock: MockComfy,
    dir: Dir,
    state: Dir,
}

impl Setup {
    fn new(files: &[(&str, &str)]) -> Self {
        let dir = model_dir(files);
        let mock = MockComfy::start().unwrap();
        mock.delay_ms.store(20, std::sync::atomic::Ordering::SeqCst);
        mock.set_object_info(object_info(&dir.0));
        mock.set_system_stats(system_stats("0.42.5", "NVIDIA GeForce RTX 5090"));
        mock.set_templates_dir(fixtures().join("comfy-0.42.5/templates"));
        let state = Dir(std::env::temp_dir().join(format!("li-setup-state-{}", uuid::Uuid::new_v4().simple())));
        Self { mock, dir, state }
    }

    fn options(&self) -> Options {
        Options {
            host: self.mock.host().to_owned(),
            model_dir: Some(self.dir.0.clone()),
            state_path: self.state.0.join("ai-models.json"),
            cache_dir: Some(self.state.0.join("templates")),
            templates_dir: None,
        }
    }

    fn scan(&self) -> Scan {
        modelsetup::scan(&self.options())
    }
}

fn without(ids: &[&str]) -> Registry {
    let raw: BTreeMap<String, Value> =
        family::EMBEDDED.iter().filter(|(k, _)| !ids.contains(k)).map(|(k, t)| ((*k).to_owned(), serde_json::from_str(t).unwrap())).collect();
    Registry { families: family::build(&raw).unwrap(), ..Default::default() }
}

fn png(img: &RgbImage) -> Vec<u8> {
    let mut out = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgb8(img.clone()).write_to(&mut out, image::ImageFormat::Png).unwrap();
    out.into_inner()
}

/// A shaded ball on a light background (what the create test asks for, in `r g b`).
fn ball(w: u32, h: u32, colour: [u8; 3]) -> RgbImage {
    RgbImage::from_fn(w, h, |x, y| {
        let (dx, dy) = (x as f32 - w as f32 / 2.0, y as f32 - h as f32 / 2.0);
        let r = w.min(h) as f32 * 0.25;
        if dx * dx + dy * dy < r * r {
            let shade = (1.0 - (dx * dx + dy * dy).sqrt() / (1.5 * r)).clamp(0.4, 1.0);
            Rgb(colour.map(|c| (f32::from(c) * shade) as u8))
        } else {
            let v = 240 - (y * 12 / h.max(1)) as u8;
            Rgb([v, v, v.saturating_add(4)])
        }
    })
}

/// Noise at the latent's scale (8-pixel cells), what a VAE that doesn't fit decodes.
fn noise(w: u32, h: u32) -> RgbImage {
    let mut s = 0x9e37_79b9_u32;
    let cells = RgbImage::from_fn(w.div_ceil(8), h.div_ceil(8), |_, _| {
        s ^= s << 13;
        s ^= s >> 17;
        s ^= s << 5;
        Rgb([(s & 0xff) as u8, (s >> 8 & 0xff) as u8, (s >> 16 & 0xff) as u8])
    });
    image::imageops::resize(&cells, w, h, image::imageops::FilterType::Nearest)
}

fn create_size(key: &str) -> (u32, u32) {
    let f = &ModelId::from_key(key).unwrap().info().resolved;
    modelsetup::verify::test_size(f.sizes.native, f.sizes.multiple)
}

fn codes(scan: &Scan, key: &str) -> Vec<String> {
    scan.server.models[key].issues.iter().map(|i| i.code.clone()).collect()
}

const MOSS: &[(&str, &str)] = &[
    ("diffusion_models/moss_image_sft_bf16.safetensors", "owner__diffusion_models~moss_image_sft_bf16"),
    ("text_encoders/qwen3.5_9b_bf16.safetensors", "Comfy-Org_MOSS-Image__text_encoders~qwen3.5_9b_bf16"),
    ("vae/flux2-vae.safetensors", "local__vae~flux2-vae"),
];
const MOSS_KEY: &str = "unet:moss_image_sft_bf16.safetensors";

const FLUX3: &[(&str, &str)] = &[
    ("diffusion_models/flux3-image-klein-bf16.safetensors", "owner__diffusion_models~flux3-image-klein-bf16"),
    ("text_encoders/qwen3vl_4b_bf16.safetensors", "Comfy-Org_Krea-2__text_encoders~qwen3vl_4b_bf16"),
    ("vae/flux2-vae.safetensors", "local__vae~flux2-vae"),
];
const FLUX3_KEY: &str = "unet:flux3-image-klein-bf16.safetensors";

#[test]
fn a_model_no_profile_describes_runs_and_verifies_through_its_official_template() {
    let _g = SERIAL.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let s = Setup::new(MOSS);
    let scan = modelsetup::scan_with(&s.options(), &without(&["moss-image"]));
    let m = &scan.server.models[MOSS_KEY];
    assert_eq!((m.status, m.engine.as_str()), (Status::Ready, "template"), "{:?}", m.issues);
    assert_eq!(m.templates.get("create").map(String::as_str), Some("image_moss_image_t2i"));
    assert_eq!(m.identity.as_ref().unwrap().arch, None);
    let bind = |role: &str| m.bindings.iter().find(|b| b.role == role).unwrap();
    assert_eq!(bind("clip").used.as_deref(), Some("qwen3.5_9b_bf16.safetensors"));
    assert_eq!((bind("vae").used.as_deref(), bind("vae").why), (Some("flux2-vae.safetensors"), li_ai::companions::Why::Equivalent));
    assert!(codes(&scan, MOSS_KEY).contains(&"UsingEquivalent".to_owned()));
    // The catalogue lists it with the template's capabilities.
    let model = ModelId::from_key(MOSS_KEY).expect("published to the catalogue");
    let info = model.info();
    assert!(info.recipe.is_some() && info.text_to_image && info.resolved.capabilities.inpaint);
    // Verification renders through the app's generation path; ComfyUI's memory is freed once.
    let (w, h) = create_size(MOSS_KEY);
    let picture = li_ai::refine::test_image();
    let mut filled = picture.clone();
    for (x, y, p) in filled.enumerate_pixels_mut() {
        if modelsetup::verify::fill_mask(picture.width(), picture.height()).get_pixel(x, y)[0] > 0 {
            *p = Rgb([210, 20, 20]);
        }
    }
    s.mock.queue_outputs([png(&ball(w, h, [230, 30, 30])), png(&picture), png(&filled)]);
    let mut steps = Vec::new();
    let done = modelsetup::verify_models(&s.options(), &scan, &[MOSS_KEY.to_owned()], &JobControl::new(), &mut |_, st| {
        steps.push((st.capability.to_owned(), st.result.pass, st.result.failure.clone()))
    })
    .unwrap();
    assert!(done[0].result.is_ok(), "{:?}", done[0].result.as_ref().err());
    assert_eq!(steps.iter().map(|(c, p, _)| (c.as_str(), *p)).collect::<Vec<_>>(), [("create", true), ("refine", true), ("inpaint", true)], "{steps:?}");
    assert_eq!(s.mock.frees(), 1);
    let prompts = s.mock.prompts();
    let create = &prompts[0];
    let node = |class: &str| create.as_object().unwrap().values().find(|n| n["class_type"] == class).cloned().unwrap();
    assert_eq!(node("UNETLoader")["inputs"]["unet_name"], "moss_image_sft_bf16.safetensors");
    assert_eq!(
        (node("CLIPLoader")["inputs"]["clip_name"].as_str(), node("CLIPLoader")["inputs"]["type"].as_str()),
        (Some("qwen3.5_9b_bf16.safetensors"), Some("moss_image"))
    );
    assert_eq!(node("VAELoader")["inputs"]["vae_name"], "flux2-vae.safetensors");
    assert!(create.to_string().contains(modelsetup::verify::CREATE_PROMPT));
    let stored = li_ai::modelsetup::state::SetupState::load(&s.options().state_path);
    let m = &stored.server(s.mock.host()).unwrap().models[MOSS_KEY];
    assert_eq!(m.status, Status::Verified);
    assert!(!m.issues.iter().any(|i| i.code == "NotVerified"));
    // A later scan keeps the verification while nothing changed.
    let again = modelsetup::scan_with(&s.options(), &without(&["moss-image"]));
    assert_eq!(again.server.models[MOSS_KEY].status, Status::Verified);
}

#[test]
fn a_noise_render_fails_verification_and_names_the_vae() {
    let _g = SERIAL.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let s = Setup::new(FLUX3);
    let scan = s.scan();
    let m = &scan.server.models[FLUX3_KEY];
    assert_eq!(m.status, Status::Ready, "{:?}", m.issues);
    assert_eq!(m.identity.as_ref().unwrap().arch.as_deref(), Some("flux3-klein"));
    assert_eq!(m.templates.get("edit").map(String::as_str), Some("image_flux3_klein_distilled_image_edit"));
    let (w, h) = create_size(FLUX3_KEY);
    s.mock.queue_outputs([png(&noise(w, h))]);
    let done = modelsetup::verify_models(&s.options(), &scan, &[FLUX3_KEY.to_owned()], &JobControl::new(), &mut |_, _| {}).unwrap();
    let v = done[0].result.as_ref().unwrap();
    assert_eq!(v.capabilities["create"].failure.as_deref(), Some("VerifyFailed:noise"));
    let stored = li_ai::modelsetup::state::SetupState::load(&s.options().state_path);
    let m = &stored.server(s.mock.host()).unwrap().models[FLUX3_KEY];
    assert_eq!(m.status, Status::Failed);
    let issue = m.issues.iter().find(|i| i.code == "VerifyFailed:noise").unwrap();
    assert!(issue.english().contains("The VAE (flux2-vae.safetensors) probably doesn't match"), "{}", issue.english());
}

#[test]
fn a_shape_mismatch_names_the_installed_and_declared_text_encoders() {
    let _g = SERIAL.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let s = Setup::new(FLUX3);
    let scan = s.scan();
    s.mock.fail_next_with("70", "SamplerCustomAdvanced", "mat1 and mat2 shapes cannot be multiplied (512x2560 and 7680x3072)");
    let done = modelsetup::verify_models(&s.options(), &scan, &[FLUX3_KEY.to_owned()], &JobControl::new(), &mut |_, _| {}).unwrap();
    let create = &done[0].result.as_ref().unwrap().capabilities["create"];
    let message = create.message.as_deref().unwrap_or_default();
    assert!(
        message.contains("The text encoder qwen3vl_4b_bf16.safetensors doesn't fit") && message.contains("qwen3vl_4b_int8_convrot.safetensors"),
        "{message}"
    );
}

#[test]
fn an_older_comfyui_is_reported_before_anything_runs() {
    let _g = SERIAL.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let s = Setup::new(FLUX3);
    let mut info = object_info(&s.dir.0);
    let types = info["CLIPLoader"]["input"]["required"]["type"][0].as_array_mut().unwrap();
    types.retain(|t| t != "flux3" && t != "moss_image");
    s.mock.set_object_info(info);
    s.mock.set_system_stats(system_stats("0.38.0", "NVIDIA GeForce RTX 5090"));
    let scan = s.scan();
    let m = &scan.server.models[FLUX3_KEY];
    assert_eq!(m.status, Status::Blocked);
    let c = codes(&scan, FLUX3_KEY);
    assert!(c.contains(&"ComfyTooOld".to_owned()) && c.contains(&"ValueNotOffered".to_owned()), "{c:?}");
    let too_old = m.issues.iter().find(|i| i.code == "ComfyTooOld").unwrap().english();
    assert!(too_old.contains("needs ComfyUI 0.41.0 or newer. This server runs 0.38.0."), "{too_old}");
    assert!(s.mock.prompts().is_empty());
    assert!(ModelId::from_key(FLUX3_KEY).is_none_or(|id| id.info().recipe.is_none()));
}

#[test]
fn blackwell_only_precisions_are_blocked_on_older_gpus() {
    let _g = SERIAL.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let s = Setup::new(&[
        ("diffusion_models/krea2_turbo_nvfp4.safetensors", "Comfy-Org_Krea-2__diffusion_models~krea2_turbo_nvfp4"),
        ("text_encoders/qwen3vl_4b_bf16.safetensors", "Comfy-Org_Krea-2__text_encoders~qwen3vl_4b_bf16"),
        ("vae/qwen_image_vae.safetensors", "Comfy-Org_Krea-2__vae~qwen_image_vae"),
    ]);
    s.mock.set_system_stats(system_stats("0.42.5", "NVIDIA GeForce RTX 4090"));
    let scan = s.scan();
    let key = "unet:krea2_turbo_nvfp4.safetensors";
    let m = &scan.server.models[key];
    let issue = m.issues.iter().find(|i| i.code == "QuantUnsupported").unwrap_or_else(|| panic!("{:?}", m.issues));
    assert!(issue.english().contains("uses NVFP4, which needs an NVIDIA Blackwell"), "{}", issue.english());
    assert_eq!(m.status, Status::Blocked);
    // On a Blackwell GPU the same file is ready; Style references stay off until their LoRA
    // is installed (a warning, not a blocker).
    s.mock.set_system_stats(system_stats("0.42.5", "NVIDIA GeForce RTX 5090"));
    let scan = s.scan();
    let m = &scan.server.models[key];
    assert_eq!(m.status, Status::Ready, "{:?}", m.issues);
    let lora = m.issues.iter().find(|i| i.code == "CompanionMissing").unwrap();
    assert_eq!((lora.severity, lora.args["file"].as_str()), (modelsetup::issue::Severity::Warning, "krea2_style_reference.safetensors"));
    assert!(m.capabilities.iter().any(|c| c.name == "style" && !c.ok));
}

#[test]
fn the_report_counts_models_and_renders_text_and_json() {
    let _g = SERIAL.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let s = Setup::new(&[MOSS, FLUX3].concat());
    let scan = s.scan();
    let r = modelsetup::report(&s.options(), &scan);
    assert_eq!(r.report_version, 1);
    assert_eq!(r.summary.not_tested, 2, "{:?}", r.models.iter().map(|m| (&m.key, &m.status)).collect::<Vec<_>>());
    assert_eq!(r.blockers(), 0, "{:?}", r.issues);
    let text = modelsetup::report::render_text(&r);
    assert!(text.contains("READY    FLUX.3 Klein · distilled · BF16"), "{text}");
    assert!(text.contains("MOSS-Image · sft · BF16") || text.contains("MOSS-Image · SFT · BF16"), "{text}");
    assert!(text.contains("Licence: FLUX Kommunity License"), "{text}");
    let back: modelsetup::report::Report = serde_json::from_str(&serde_json::to_string(&r).unwrap()).unwrap();
    assert_eq!(back, r);
}

#[test]
fn an_unreachable_server_reports_one_blocker() {
    let _g = SERIAL.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let opts = Options {
        host: "127.0.0.1:9".into(),
        model_dir: None,
        state_path: std::env::temp_dir().join(format!("li-setup-none-{}/ai-models.json", uuid::Uuid::new_v4().simple())),
        cache_dir: None,
        templates_dir: None,
    };
    let scan = modelsetup::scan(&opts);
    assert!(scan.snapshot.is_none());
    assert_eq!(scan.issues.iter().map(|i| i.code.as_str()).collect::<Vec<_>>(), ["ServerUnreachable"]);
    assert!(scan.issues[0].english().starts_with("ComfyUI is not reachable at 127.0.0.1:9."));
}
