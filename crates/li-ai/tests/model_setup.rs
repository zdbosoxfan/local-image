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

/// What ComfyUI returns for the fill test: the crop of the test picture the model sees, with the
/// selection painted by `paint`.
fn filled(paint: &dyn Fn(u32, u32) -> Rgb<u8>) -> RgbImage {
    let mut out = li_ai::refine::test_image();
    let mask = modelsetup::verify::fill_mask(out.width(), out.height());
    for (x, y, p) in out.enumerate_pixels_mut() {
        if mask.get_pixel(x, y)[0] > 0 {
            *p = paint(x, y);
        }
    }
    let c = li_ai::inpaint::plan(&mask).unwrap().context;
    image::imageops::crop_imm(&out, c.x0, c.y0, c.width(), c.height()).to_image()
}

/// A shaded red ball on the sky where the test picture's sun was (what the fill test asks for).
fn red_ball_on_sky(x: u32, y: u32) -> Rgb<u8> {
    let d = ((f64::from(x) - 400.0).powi(2) + (f64::from(y) - 66.0).powi(2)).sqrt();
    let v = f64::from(y) / 288.0;
    if d < 24.0 { Rgb([(230.0 * (1.0 - d / 60.0)) as u8, 30, 30]) } else { Rgb([105 + (v * 25.0) as u8, 166 + (v * 22.0) as u8, 205 + (v * 25.0) as u8]) }
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
    assert!(!info.upscale, "no dedicated upscaler: it upscales through Refine");
    assert_eq!(info.family_label, "MOSS-Image", "the picker's family column names the model, not its precision");
    // Verification renders through the app's generation path; ComfyUI's memory is freed once.
    let (w, h) = create_size(MOSS_KEY);
    let picture = li_ai::refine::test_image();
    s.mock.queue_outputs([png(&ball(w, h, [230, 30, 30])), png(&picture), png(&filled(&red_ball_on_sky))]);
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
    let main = m.bindings.iter().find(|b| b.role == "main").unwrap();
    assert_eq!(
        (main.used.as_deref(), main.declared.as_str(), main.why),
        (Some("flux3-image-klein-bf16.safetensors"), "flux3-image-klein-int8-convrot.safetensors", li_ai::companions::Why::SameStem),
        "the main file matched the template's by its stem"
    );
    assert_eq!(ModelId::from_key(FLUX3_KEY).unwrap().info().family_label, li_ai::family::registry().family("flux3-klein").unwrap().label);
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
    // The rescan the wizard runs after Verify keeps the explanation and doesn't call it untested.
    let again = s.scan();
    let m = &again.server.models[FLUX3_KEY];
    assert_eq!(m.status, Status::Failed);
    assert!(!m.issues.iter().any(|i| i.code == "NotVerified"), "{:?}", codes(&again, FLUX3_KEY));
    assert!(m.issues.iter().any(|i| i.code == "VerifyFailed:noise" && i.english().contains("probably doesn't match")));
    let summary = modelsetup::summary::summarise(Some(&again.server), &li_ai::catalog::catalog().models, Some("0.42.5"));
    assert_eq!(summary.models[FLUX3_KEY].issue.as_ref().map(|i| i.code.as_str()), Some("VerifyFailed:noise"), "the picker's tooltip names the failure");
}

#[test]
fn a_black_fill_turns_only_fill_off() {
    let _g = SERIAL.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let s = Setup::new(MOSS);
    let scan = s.scan();
    assert_eq!(scan.server.models[MOSS_KEY].status, Status::Ready, "{:?}", codes(&scan, MOSS_KEY));
    let (w, h) = create_size(MOSS_KEY);
    // A VAE that decodes the fill to black.
    s.mock.queue_outputs([png(&ball(w, h, [230, 30, 30])), png(&li_ai::refine::test_image()), png(&filled(&|_, _| Rgb([0, 0, 0])))]);
    let done = modelsetup::verify_models(&s.options(), &scan, &[MOSS_KEY.to_owned()], &JobControl::new(), &mut |_, _| {}).unwrap();
    let v = done[0].result.as_ref().unwrap();
    assert_eq!(v.capabilities["inpaint"].failure.as_deref(), Some("VerifyFailed:black"), "{:?}", v.capabilities["inpaint"]);
    assert!(v.capabilities["create"].pass);
    let again = s.scan();
    let m = &again.server.models[MOSS_KEY];
    assert_eq!(m.status, Status::PartiallyVerified);
    assert!(!m.issues.iter().any(|i| i.code == "NotVerified"), "{:?}", codes(&again, MOSS_KEY));
    assert!(m.issues.iter().any(|i| i.code == "VerifyFailed:black" && i.english().starts_with("The Fill test image came out black")));
    assert!(m.issues.iter().any(|i| i.english() == "Fill is turned off for this model until it passes its test."), "{:?}", codes(&again, MOSS_KEY));
    let info = ModelId::from_key(MOSS_KEY).unwrap().info();
    assert!(info.text_to_image && !info.inpaint, "Fill is off, Create stays on");
}

#[test]
fn verification_renders_create_with_a_reference_through_the_edit_graph() {
    let _g = SERIAL.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let s = Setup::new(FLUX3);
    let scan = s.scan();
    let (w, h) = create_size(FLUX3_KEY);
    let red = png(&ball(w, h, [230, 30, 30]));
    s.mock.queue_outputs([red.clone(), png(&ball(w, h, [30, 40, 230])), red, png(&li_ai::refine::test_image()), png(&filled(&red_ball_on_sky))]);
    let mut steps = Vec::new();
    let done = modelsetup::verify_models(&s.options(), &scan, &[FLUX3_KEY.to_owned()], &JobControl::new(), &mut |_, st| {
        steps.push((st.capability.to_owned(), st.result.failure.clone()))
    })
    .unwrap();
    assert!(done[0].result.is_ok(), "{:?}", done[0].result.as_ref().err());
    let names: Vec<&str> = steps.iter().map(|(c, _)| c.as_str()).collect();
    assert_eq!(names, ["create", "edit", "references", "refine", "inpaint"], "{steps:?}");
    assert!(steps.iter().all(|(_, f)| f.is_none()), "{steps:?}");
    let with_reference = &s.mock.prompts()[2];
    assert!(with_reference.to_string().contains(modelsetup::verify::CREATE_PROMPT), "{with_reference}");
    assert!(with_reference.as_object().unwrap().values().any(|n| n["class_type"] == "LoadImage"), "one reference image: {with_reference}");
    let r = modelsetup::report(&s.options(), &s.scan());
    let text = modelsetup::report::render_text(&r);
    assert!(text.contains("References 10, Edit 10, Refine, Fill, Upscale - verified\n"), "{text}");
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
fn verifying_one_precision_of_a_curated_model_renders_and_records_that_file() {
    let _g = SERIAL.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let s = Setup::new(&[
        ("diffusion_models/qwen_image_2.1_bf16.safetensors", "local__diffusion_models~qwen_image_2.1_bf16"),
        ("diffusion_models/qwen_image_2.1_int8_convrot.safetensors", "local__diffusion_models~qwen_image_2.1_int8_convrot"),
        ("text_encoders/qwen3vl_8b_bf16.safetensors", "local__text_encoders~qwen3vl_8b_bf16"),
        ("text_encoders/qwen3vl_8b_int8_convrot.safetensors", "local__text_encoders~qwen3vl_8b_int8_convrot"),
        ("vae/qwen_image_2.1_vae_bf16.safetensors", "local__vae~qwen_image_2.1_vae_bf16"),
    ]);
    // The curated Qwen builder's nodes, beyond what the official templates use.
    let mut info = object_info(&s.dir.0);
    for (class, node) in li_ai::mock::object_info().as_object().unwrap() {
        info.as_object_mut().unwrap().entry(class.clone()).or_insert_with(|| node.clone());
    }
    s.mock.set_object_info(info);
    let (bf16, int8) = ("unet:qwen_image_2.1_bf16.safetensors", "unet:qwen_image_2.1_int8_convrot.safetensors");
    let scan = s.scan();
    for (key, variant) in [(bf16, "bf16"), (int8, "int8")] {
        let m = &scan.server.models[key];
        assert_eq!((m.status, m.catalog_id.as_str(), m.variant.as_str()), (Status::Builtin, "qwen", variant), "{:?}", m.issues);
    }
    // A flat Create render: its message names the files that precision loads.
    let (w, h) = create_size("qwen");
    s.mock.queue_outputs([png(&RgbImage::from_pixel(w, h, Rgb([90, 140, 60])))]);
    let done = modelsetup::verify_models(&s.options(), &scan, &[bf16.to_owned()], &JobControl::new(), &mut |_, _| {}).unwrap();
    assert!(done[0].result.is_ok(), "{:?}", done[0].result.as_ref().err());
    let prompts = s.mock.prompts();
    assert!(!prompts.is_empty());
    for p in &prompts {
        let unets: Vec<&str> =
            p.as_object().unwrap().values().filter(|n| n["class_type"] == "UNETLoader").filter_map(|n| n["inputs"]["unet_name"].as_str()).collect();
        assert_eq!(unets, ["qwen_image_2.1_bf16.safetensors"], "every test renders the file being verified: {p}");
    }
    let stored = li_ai::modelsetup::state::SetupState::load(&s.options().state_path);
    let server = stored.server(s.mock.host()).unwrap();
    assert!(server.models[bf16].verified.is_some());
    let flat = server.models[bf16].issues.iter().find(|i| i.code == "VerifyFailed:flat").expect("the flat render failed Create").english();
    assert!(flat.contains("Check the text encoder (qwen3vl_8b_bf16.safetensors) and the VAE (qwen_image_2.1_vae_bf16.safetensors)."), "{flat}");
    assert_eq!((server.models[int8].status, server.models[int8].verified.as_ref()), (Status::Builtin, None), "the other precision is untouched");
    let summary = modelsetup::summary::summarise(Some(server), &li_ai::catalog::catalog().models, Some("0.42.5"));
    assert_eq!(summary.status_for("qwen", "int8"), Some(Status::Builtin));
    assert_eq!(summary.status_for("qwen", "bf16"), Some(server.models[bf16].status));
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
    let declared = m.bindings.iter().find(|b| b.role == "main").map(|b| b.declared.clone()).unwrap();
    assert!(!declared.contains("nvfp4"), "{declared}");
    assert!(
        matches!(&issue.fixes[..], [modelsetup::issue::Fix::Download { files, folder, url, .. }, modelsetup::issue::Fix::TryAnyway]
            if files == std::slice::from_ref(&declared) && folder == "diffusion_models" && url.ends_with(&format!("/{declared}"))),
        "the template's own precision is offered before Try anyway: {:?}",
        issue.fixes
    );
    // On a Blackwell GPU the same file is ready; Style references stay off until their LoRA
    // is installed (a warning, not a blocker).
    s.mock.set_system_stats(system_stats("0.42.5", "NVIDIA GeForce RTX 5090"));
    let scan = s.scan();
    let m = &scan.server.models[key];
    assert_eq!(m.status, Status::Ready, "{:?}", m.issues);
    let lora = m.issues.iter().find(|i| i.code == "CompanionMissing").unwrap();
    assert_eq!((lora.severity, lora.args["file"].as_str()), (modelsetup::issue::Severity::Warning, "krea2_style_reference.safetensors"));
    assert!(
        matches!(&lora.fixes[..], [modelsetup::issue::Fix::Download { url, folder, .. }] if url.ends_with("/krea2_style_reference.safetensors") && folder == "loras"),
        "a missing file the template declares is offered as a download: {:?}",
        lora.fixes
    );
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
    assert!(text.contains("READY    FLUX.3 Klein Distilled · BF16"), "{text}");
    assert!(text.contains("MOSS-Image SFT · BF16"), "{text}");
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

#[test]
fn the_prompt_enhancer_runs_only_when_asked_for_and_installed() {
    let _g = SERIAL.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let upsampler = ("text_encoders/qwen3.5_9b_flux3_prompt_upsampler_w4a8.safetensors", "Comfy-Org_MOSS-Image__text_encoders~qwen3.5_9b_bf16");
    let generate = |s: &Setup, enhance: bool| -> Value {
        let model = ModelId::from_key(FLUX3_KEY).unwrap();
        let (w, h) = create_size(FLUX3_KEY);
        s.mock.queue_outputs([png(&ball(w, h, [230, 30, 30]))]);
        let mut req = li_ai::ops::GenerateRequest::new(model, "a house by a lake");
        (req.width, req.height, req.enhance) = (w, h, enhance);
        li_ai::ops::Ai::new(s.mock.host()).generate(&req, &JobControl::new()).unwrap();
        s.mock.prompts().pop().unwrap()
    };
    let uses_enhancer = |g: &Value| g.as_object().unwrap().values().any(|n| n["class_type"] == "TextGenerate");
    // Not installed: offered as a download, and asking for it changes nothing.
    let s = Setup::new(FLUX3);
    let scan = s.scan();
    let offer = scan.server.models[FLUX3_KEY].issues.iter().find(|i| i.code == "UpsamplerAvailable").unwrap();
    assert!(
        matches!(&offer.fixes[..], [modelsetup::issue::Fix::Download { files, folder, .. }] if files[0] == "qwen3.5_9b_flux3_prompt_upsampler_w4a8.safetensors" && folder == "text_encoders"),
        "{offer:?}"
    );
    assert!(!uses_enhancer(&generate(&s, true)));
    drop(s);
    // Installed: off unless asked for.
    let s = Setup::new(&[FLUX3, &[upsampler]].concat());
    let scan = s.scan();
    assert!(codes(&scan, FLUX3_KEY).contains(&"UpsamplerInstalled".to_owned()));
    assert!(!uses_enhancer(&generate(&s, false)));
    let enhanced = generate(&s, true);
    assert!(uses_enhancer(&enhanced), "{enhanced}");
    assert!(enhanced.to_string().contains("a house by a lake"));
}

#[test]
fn the_persons_choices_apply_on_the_next_scan() {
    let _g = SERIAL.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let ming = "unet:ming_image_0.1_design_bf16.safetensors";
    let s = Setup::new(&[
        ("diffusion_models/ming_image_0.1_design_bf16.safetensors", "Comfy-Org_Ming-Image__diffusion_models~ming_image_0.1_design_bf16"),
        MOSS[0],
        MOSS[1],
        MOSS[2],
    ]);
    let scan = s.scan();
    assert_eq!(scan.server.models[ming].status, Status::Unmatched, "{:?}", codes(&scan, ming));
    assert_eq!(scan.server.models[ming].label, "Ming Image 0.1 Design · BF16", "the precision is named once");
    assert!(scan.templates.iter().any(|t| t.name == "image_moss_image_t2i" && t.title == "MOSS-Image: Text to Image"));
    let host = s.mock.host().to_owned();
    let state = s.options().state_path;
    modelsetup::fixes::choose(&state, &host, ming, modelsetup::fixes::Choice::Template(Some("image_moss_image_t2i".into()))).unwrap();
    let scan = s.scan();
    assert_eq!(scan.server.models[ming].templates.get("create").map(String::as_str), Some("image_moss_image_t2i"));
    // Ignored models stay ignored and are not offered for generation.
    modelsetup::fixes::choose(&state, &host, MOSS_KEY, modelsetup::fixes::Choice::Ignore(true)).unwrap();
    let scan = s.scan();
    assert_eq!(scan.server.models[MOSS_KEY].status, Status::Ignored);
    assert!(ModelId::from_key(MOSS_KEY).is_none_or(|id| id.info().recipe.is_none()));
    let summary = modelsetup::summary::summarise(scan.server.models.get(MOSS_KEY).map(|_| &scan.server), &li_ai::catalog::catalog().models, Some("0.42.5"));
    assert_eq!(summary.status(MOSS_KEY), Some(Status::Ignored));
    assert!(!summary.new.contains(&MOSS_KEY.to_owned()));
}

#[test]
fn a_template_this_comfyui_cannot_run_is_refused_before_anything_downloads() {
    struct Recording(Mutex<Vec<String>>);
    impl li_ai::browser::Net for Recording {
        fn get(&self, url: &str) -> anyhow::Result<Vec<u8>> {
            self.0.lock().unwrap().push(url.to_owned());
            let name = url.rsplit('/').next().unwrap_or_default();
            std::fs::read(fixtures().join("comfy-0.42.5/templates").join(name)).map_err(Into::into)
        }
        fn head_size(&self, url: &str) -> anyhow::Result<Option<u64>> {
            self.0.lock().unwrap().push(url.to_owned());
            Ok(None)
        }
    }
    let dir = model_dir(FLUX3);
    let mut info = object_info(&dir.0);
    let types = info["CLIPLoader"]["input"]["required"]["type"][0].as_array_mut().unwrap();
    types.retain(|t| t != "flux3");
    let old = modelsetup::Snapshot::from_parts(
        "127.0.0.1:8189",
        &system_stats("0.38.0", "NVIDIA GeForce RTX 5090"),
        li_ai::comfy::ObjectInfo::from_bytes(&serde_json::to_vec(&info).unwrap()).unwrap(),
    );
    let item = li_ai::browser::Item {
        id: "template:image_flux3_klein_distilled_t2i".into(),
        title: "FLUX 3 Image Klein Distilled: Text to Image".into(),
        template: Some("image_flux3_klein_distilled_t2i".into()),
        min_comfyui: Some("0.41.0".into()),
        ..Default::default()
    };
    let cfg = li_ai::browser::Config { templates: "http://templates.invalid/templates".into(), ..Default::default() };
    let net = Recording(Mutex::new(Vec::new()));
    let planned = modelsetup::fixes::plan_template(&item, &cfg, &net, &old, &[]).unwrap();
    assert!(planned.plan.is_none());
    let codes: Vec<&str> = planned.issues.iter().map(|i| i.code.as_str()).collect();
    assert_eq!(codes.first(), Some(&"ComfyTooOld"), "{codes:?}");
    assert!(codes.contains(&"ValueNotOffered"), "{codes:?}");
    assert_eq!(*net.0.lock().unwrap(), ["http://templates.invalid/templates/image_flux3_klein_distilled_t2i.json"], "nothing but the template was fetched");
    // The same template on ComfyUI 0.42.5 has nothing in the way.
    let now = modelsetup::Snapshot::from_parts(
        "127.0.0.1:8189",
        &system_stats("0.42.5", "NVIDIA GeForce RTX 5090"),
        li_ai::comfy::ObjectInfo::from_bytes(&serde_json::to_vec(&object_info(&dir.0)).unwrap()).unwrap(),
    );
    let entry = li_ai::template::index::TemplateEntry { name: "image_flux3_klein_distilled_t2i".into(), ..Default::default() };
    assert!(modelsetup::check::check_template(&entry, &planned.ui, &now, li_ai::companions::runtime()).is_empty());
    assert_eq!(
        modelsetup::fixes::template_main(&planned.ui, "image_flux3_klein_distilled_t2i").map(|(k, _)| k).as_deref(),
        Some("unet:flux3-image-klein-int8-convrot.safetensors")
    );
}

/// The fixture templates, and every Hugging Face file they and HiDream I1 name, listed with a
/// size and a SHA-256.
struct FixtureHub;

impl li_ai::browser::Net for FixtureHub {
    fn get(&self, url: &str) -> anyhow::Result<Vec<u8>> {
        if url.contains("/api/models/") {
            let paths = [
                "text_encoders/gemma4_e4b_it_fp8_scaled.safetensors",
                "checkpoints/hidream_o1_image_bf16.safetensors",
                "split_files/text_encoders/clip_l_hidream.safetensors",
                "split_files/text_encoders/clip_g_hidream.safetensors",
                "split_files/text_encoders/t5xxl_fp8_e4m3fn_scaled.safetensors",
                "split_files/text_encoders/llama_3.1_8b_instruct_fp8_scaled.safetensors",
                "split_files/vae/ae.safetensors",
                "diffusion_models/flux3-image-klein-int8-convrot.safetensors",
                "text_encoders/qwen3vl_4b_int8_convrot.safetensors",
                "text_encoders/qwen3.5_9b_flux3_prompt_upsampler_w4a8.safetensors",
                "vae/flux2-vae.safetensors",
            ];
            let tree: Vec<Value> = paths.iter().map(|p| json!({ "type": "file", "path": p, "lfs": { "oid": "ab".repeat(32), "size": 1000 } })).collect();
            return Ok(serde_json::to_vec(&tree)?);
        }
        let name = url.rsplit('/').next().unwrap_or_default();
        std::fs::read(fixtures().join("comfy-0.42.5/templates").join(name)).map_err(Into::into)
    }
    fn head_size(&self, _: &str) -> anyhow::Result<Option<u64>> {
        Ok(None)
    }
}

/// ComfyUI 0.42.5 on an RTX 5090, with no model files.
fn server_0_42_5() -> modelsetup::Snapshot {
    let dir = model_dir(&[]);
    modelsetup::Snapshot::from_parts(
        "127.0.0.1:8189",
        &system_stats("0.42.5", "NVIDIA GeForce RTX 5090"),
        li_ai::comfy::ObjectInfo::from_bytes(&serde_json::to_vec(&object_info(&dir.0)).unwrap()).unwrap(),
    )
}

#[test]
fn a_hidream_o1_template_card_plans_only_its_own_files() {
    use li_ai::browser::{Kind, Query, Source, View};
    let index: Value = serde_json::from_slice(&std::fs::read(fixtures().join("comfy-0.42.5/templates/index.json")).unwrap()).unwrap();
    let cfg = li_ai::browser::Config { templates: "http://templates.invalid/templates".into(), ..Default::default() };
    let q = Query { view: View::Trending, kind: Kind::Model, search: String::new(), sources: Vec::new() };
    let items = li_ai::browser::items(li_ai::family::registry(), &cfg, &q, &[(Source::Template, index)]);
    let card = |name: &str| items.iter().find(|i| i.template.as_deref() == Some(name)).unwrap().clone();
    // The index labels both "HiDream"; only E1 is the HiDream family the builder runs.
    assert_eq!(card("image_hidream_o1").family, None);
    assert_eq!(card("hidream_e1_1").family.as_deref(), Some("hidream"));
    let planned = modelsetup::fixes::plan_template(&card("image_hidream_o1"), &cfg, &FixtureHub, &server_0_42_5(), &[]).unwrap();
    let plan = planned.plan.unwrap_or_else(|| panic!("{:?}", planned.issues));
    let mut names: Vec<&str> = plan.files.iter().map(|f| f.name.as_str()).collect();
    names.sort_unstable();
    assert_eq!(names, ["gemma4_e4b_it_fp8_scaled.safetensors", "hidream_o1_image_bf16.safetensors"], "none of HiDream I1's encoders or VAE");
}

#[test]
fn a_template_card_plans_its_prompt_enhancer_only_when_ticked() {
    let cfg = li_ai::browser::Config { templates: "http://templates.invalid/templates".into(), ..Default::default() };
    let item = li_ai::browser::Item {
        id: "template:image_flux3_klein_distilled_t2i".into(),
        title: "FLUX 3 Image Klein Distilled: Text to Image".into(),
        template: Some("image_flux3_klein_distilled_t2i".into()),
        ..Default::default()
    };
    let upsampler = "qwen3.5_9b_flux3_prompt_upsampler_w4a8.safetensors";
    let names = |p: &modelsetup::fixes::TemplatePlan| {
        let mut v: Vec<String> = p.plan.as_ref().unwrap().files.iter().map(|f| f.name.clone()).collect();
        v.sort();
        v
    };
    let planned = modelsetup::fixes::plan_template(&item, &cfg, &FixtureHub, &server_0_42_5(), &[]).unwrap();
    assert_eq!(names(&planned), ["flux2-vae.safetensors", "flux3-image-klein-int8-convrot.safetensors", "qwen3vl_4b_int8_convrot.safetensors"]);
    assert_eq!(planned.optional.iter().map(|f| f.name.as_str()).collect::<Vec<_>>(), [upsampler]);
    let asked = modelsetup::fixes::plan_template(&item, &cfg, &FixtureHub, &server_0_42_5(), &[upsampler.to_owned()]).unwrap();
    assert!(names(&asked).iter().any(|n| n == upsampler), "{:?}", names(&asked));
}

#[test]
fn an_installed_template_runs_its_main_file_through_that_template() {
    let _g = SERIAL.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let s = Setup::new(&[
        ("diffusion_models/moss_image_sft_int8_convrot.safetensors", "Comfy-Org_MOSS-Image__diffusion_models~moss_image_sft_int8_convrot"),
        MOSS[1],
        MOSS[2],
    ]);
    let ui: Value = serde_json::from_slice(&std::fs::read(fixtures().join("comfy-0.42.5/templates/image_moss_image_t2i.json")).unwrap()).unwrap();
    let key = modelsetup::fixes::adopt_template(&s.options(), &ui, "image_moss_image_t2i").unwrap().expect("the template declares a main file");
    assert_eq!(key, "unet:moss_image_sft_int8_convrot.safetensors");
    let stored = li_ai::modelsetup::state::SetupState::load(&s.options().state_path);
    let m = &stored.server(s.mock.host()).unwrap().models[&key];
    assert_eq!(m.user.template_choice.as_deref(), Some("image_moss_image_t2i"));
    assert_eq!(m.templates.get("create").map(String::as_str), Some("image_moss_image_t2i"));
    assert!(ModelId::from_key(&key).is_some_and(|id| id.info().recipe.is_some()), "published as a template model, not a custom workflow");
}

#[test]
fn verification_tests_an_installed_prompt_enhancer_separately() {
    let _g = SERIAL.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let mut files = FLUX3.to_vec();
    files.push(("text_encoders/qwen3.5_9b_flux3_prompt_upsampler_w4a8.safetensors", MOSS[1].1));
    let s = Setup::new(&files);
    let scan = s.scan();
    assert!(scan.server.models[FLUX3_KEY].recipe.as_ref().unwrap().enhancer_ready());
    let (w, h) = create_size(FLUX3_KEY);
    let red = png(&ball(w, h, [230, 30, 30]));
    s.mock.queue_outputs([red.clone(), png(&ball(w, h, [30, 40, 230])), red.clone(), red, png(&li_ai::refine::test_image()), png(&filled(&red_ball_on_sky))]);
    let done = modelsetup::verify_models(&s.options(), &scan, &[FLUX3_KEY.to_owned()], &JobControl::new(), &mut |_, _| {}).unwrap();
    let result = done[0].result.as_ref().unwrap();
    assert!(result.capabilities.contains_key("prompt_enhancer"), "{result:?}");
    assert!(result.capabilities["prompt_enhancer"].pass);
    let prompts = s.mock.prompts();
    let has_enhancer = |g: &Value| g.as_object().unwrap().values().any(|n| n["class_type"] == "TextGenerate");
    assert!(!has_enhancer(&prompts[0]), "plain Create stays the default");
    assert!(has_enhancer(&prompts[3]), "the separate optional test uses the template's enhancer");
    assert!(prompts[3].to_string().contains(modelsetup::verify::CREATE_PROMPT));
    let report = modelsetup::report(&s.options(), &s.scan());
    assert!(report.models.iter().find(|m| m.key == FLUX3_KEY).unwrap().capabilities.iter().any(|c| c.name == "prompt_enhancer" && c.state == "verified"));
}

#[test]
fn an_unknown_checkpoint_is_verified_through_universal_refine() {
    let _g = SERIAL.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let s = Setup::new(&[]);
    let file = "future_checkpoint.safetensors";
    li_ai::mock::write_header_only(&s.dir.0.join("checkpoints").join(file), &json!({"mystery.weight":{"dtype":"F16","shape":[1],"data_offsets":[0,2]}}))
        .unwrap();
    let mut info = object_info(&s.dir.0);
    info["CheckpointLoaderSimple"]["input"]["required"]["ckpt_name"] = json!([[file]]);
    s.mock.set_object_info(info);
    let key = format!("ckpt:{file}");
    let scan = s.scan();
    let model = &scan.server.models[&key];
    assert_eq!(model.engine, "universal");
    assert!(model.capabilities.iter().any(|c| c.name == "refine" && c.ok), "{model:?}");
    s.mock.queue_outputs([png(&li_ai::refine::test_image())]);
    let done = modelsetup::verify_models(&s.options(), &scan, std::slice::from_ref(&key), &JobControl::new(), &mut |_, _| {}).unwrap();
    let verified = done[0].result.as_ref().unwrap();
    assert!(verified.capabilities["refine"].pass, "{verified:?}");
    assert_eq!(s.mock.prompts().len(), 1, "the preservation test must actually render");
    assert!(s.mock.prompts()[0].to_string().contains(file));
    assert_eq!(s.scan().server.models[&key].status, Status::Verified);
}

#[test]
fn unavailable_templates_keep_a_valid_previously_tested_recipe() {
    let _g = SERIAL.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let s = Setup::new(FLUX3);
    let first = s.scan();
    let recipe = first.server.models[FLUX3_KEY].recipe.clone().unwrap();
    li_ai::modelsetup::state::SetupState::update(&s.options().state_path, |state| {
        let model = state.servers.get_mut(s.mock.host()).unwrap().models.get_mut(FLUX3_KEY).unwrap();
        model.status = Status::Verified;
        model.verified = Some(li_ai::modelsetup::state::Verified {
            comfyui_version: "0.42.5".into(),
            recipe_sha256: modelsetup::recipe_sha256(&recipe),
            capabilities: [("create".into(), li_ai::modelsetup::state::CapabilityResult { pass: true, ..Default::default() })].into(),
            ..Default::default()
        });
    })
    .unwrap();
    let mut opts = s.options();
    opts.cache_dir = None;
    let empty = s.state.0.join("unavailable-templates");
    s.mock.set_templates_dir(&empty);
    for index_exists in [false, true] {
        if index_exists {
            std::fs::create_dir_all(&empty).unwrap();
            std::fs::write(empty.join("index.json"), b"[]").unwrap();
        }
        let scan = modelsetup::scan(&opts);
        let model = &scan.server.models[FLUX3_KEY];
        assert_eq!(model.status, Status::NeedsReverify, "{model:?}");
        assert_eq!(model.recipe.as_ref(), Some(&recipe));
        assert!(model.issues.iter().any(|i| i.code == "UsingLastGood"));
        assert!(model.capabilities.iter().any(|c| c.name == "create" && c.ok));
        assert!(ModelId::from_key(FLUX3_KEY).unwrap().info().recipe.is_some());
    }
    let mut info = object_info(&s.dir.0);
    info.as_object_mut().unwrap().remove("KSampler");
    s.mock.set_object_info(info);
    let scan = modelsetup::scan(&opts);
    assert!(!scan.server.models[FLUX3_KEY].status.usable(), "a removed server node still blocks the stored recipe");
}

#[test]
fn a_wrong_folder_issue_names_the_actual_folder() {
    let _g = SERIAL.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let mut files = FLUX3.to_vec();
    files[2].0 = "diffusion_models/flux2-vae.safetensors";
    let s = Setup::new(&files);
    let scan = s.scan();
    let issue = scan.server.models[FLUX3_KEY].issues.iter().find(|i| i.code == "WrongFolder").expect("misplaced VAE");
    assert_eq!(issue.args["found"], "diffusion_models");
    assert!(issue.english().contains("diffusion_models/"), "{}", issue.english());
}

#[test]
fn a_background_snapshot_rechecks_replaced_weights_without_rendering() {
    let _g = SERIAL.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let s = Setup::new(FLUX3);
    let first = s.scan();
    let recipe = first.server.models[FLUX3_KEY].recipe.as_ref().unwrap();
    li_ai::modelsetup::state::SetupState::update(&s.options().state_path, |state| {
        let model = state.servers.get_mut(s.mock.host()).unwrap().models.get_mut(FLUX3_KEY).unwrap();
        model.status = Status::Verified;
        model.verified = Some(li_ai::modelsetup::state::Verified {
            comfyui_version: "0.42.5".into(),
            recipe_sha256: modelsetup::recipe_sha256(recipe),
            capabilities: [("create".into(), li_ai::modelsetup::state::CapabilityResult { pass: true, ..Default::default() })].into(),
            ..Default::default()
        });
    })
    .unwrap();
    let file = std::fs::OpenOptions::new().write(true).open(s.dir.0.join(FLUX3[0].0)).unwrap();
    file.set_modified(std::time::SystemTime::now() + std::time::Duration::from_secs(60)).unwrap();
    let info = li_ai::comfy::ObjectInfo::from_bytes(&serde_json::to_vec(&object_info(&s.dir.0)).unwrap()).unwrap();
    let snapshot = modelsetup::Snapshot::from_parts(s.mock.host(), &system_stats("0.42.5", "NVIDIA GeForce RTX 5090"), info);
    let scan = modelsetup::scan_snapshot(&s.options(), snapshot);
    let summary = modelsetup::summary::summarise(Some(&scan.server), &li_ai::catalog::catalog().models, Some("0.42.5"));
    assert_eq!(summary.status(FLUX3_KEY), Some(Status::NeedsReverify));
    assert_eq!(summary.banner(), 1);
    assert!(s.mock.prompts().is_empty(), "automatic scans never submit test renders");
}

#[test]
fn published_models_honour_static_fill_checks() {
    let _g = SERIAL.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let s = Setup::new(MOSS);
    let mut scan = s.scan();
    scan.server.models.get_mut(MOSS_KEY).unwrap().capabilities.iter_mut().find(|c| c.name == "inpaint").unwrap().ok = false;
    modelsetup::publish(&scan.server, &scan.snapshot.as_ref().unwrap().info);
    let model = ModelId::from_key(MOSS_KEY).unwrap().info();
    assert!(model.text_to_image, "Create remains usable");
    assert!(!model.inpaint, "a static Fill failure must not be advertised as usable");
}

#[test]
fn published_krea_without_its_style_lora_can_create() {
    let _g = SERIAL.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let s = Setup::new(&[
        ("diffusion_models/krea2_turbo_bf16.safetensors", "Comfy-Org_Krea-2__diffusion_models~krea2_turbo_bf16"),
        ("text_encoders/qwen3vl_4b_bf16.safetensors", "Comfy-Org_Krea-2__text_encoders~qwen3vl_4b_bf16"),
        ("vae/qwen_image_vae.safetensors", "Comfy-Org_Krea-2__vae~qwen_image_vae"),
    ]);
    let scan = s.scan();
    let key = "unet:krea2_turbo_bf16.safetensors";
    assert_eq!(scan.server.models[key].status, Status::Ready);
    let model = ModelId::from_key(key).unwrap();
    let preset = li_ai::catalog::presets_for(model).next().unwrap();
    let available = li_ai::catalog::availability(preset, &scan.snapshot.as_ref().unwrap().info);
    assert!(available.available, "plain Create must not require the optional style LoRA: {}", available.reason);
    assert_eq!(model.info().max_references, 0, "style references stay off until their LoRA is installed");
    assert!(
        scan.server.models[key]
            .issues
            .iter()
            .any(|i| i.code == "CompanionMissing" && i.args.get("file").is_some_and(|s| s == "krea2_style_reference.safetensors"))
    );
    let (w, h) = create_size(key);
    s.mock.queue_outputs([png(&ball(w, h, [230, 30, 30]))]);
    let mut req = li_ai::GenerateRequest::new(model, "A red ball on white");
    req.width = w;
    req.height = h;
    let image = li_ai::Ai::new(s.mock.host()).generate(&req, &JobControl::new()).unwrap();
    assert_eq!(image.dimensions(), (w, h));
    assert_eq!(s.mock.prompts().len(), 1);
}

fn retry_template_test(failed: &str) {
    let _g = SERIAL.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let s = Setup::new(MOSS);
    let scan = s.scan();
    let (w, h) = create_size(MOSS_KEY);
    let create = png(&ball(w, h, [230, 30, 30]));
    let fill = png(&filled(&red_ball_on_sky));
    s.mock.queue_outputs([
        if failed == "create" { png(&noise(w, h)) } else { create.clone() },
        png(&li_ai::refine::test_image()),
        if failed == "inpaint" { png(&filled(&|_, _| Rgb([0, 0, 0]))) } else { fill.clone() },
    ]);
    modelsetup::verify_models(&s.options(), &scan, &[MOSS_KEY.into()], &JobControl::new(), &mut |_, _| {}).unwrap();
    let retry = s.scan();
    assert_eq!(retry.server.models[MOSS_KEY].status, if failed == "create" { Status::Failed } else { Status::PartiallyVerified });
    let before = s.mock.prompts().len();
    s.mock.queue_outputs([create, png(&li_ai::refine::test_image()), fill]);
    let done = modelsetup::verify_models(&s.options(), &retry, &[MOSS_KEY.into()], &JobControl::new(), &mut |_, _| {}).unwrap();
    let result = done[0].result.as_ref().unwrap();
    assert_eq!(s.mock.prompts().len() - before, 3, "Create, Refine and Fill must all actually run again: {result:?}");
    assert!(result.capabilities["create"].pass && result.capabilities["inpaint"].pass, "{result:?}");
    assert_eq!(s.scan().server.models[MOSS_KEY].status, Status::Verified);
}

#[test]
fn retrying_a_failed_create_uses_the_statically_checked_recipe() {
    retry_template_test("create");
}

#[test]
fn retrying_a_failed_fill_actually_renders_fill_again() {
    retry_template_test("inpaint");
}

#[test]
fn a_verification_cannot_overwrite_a_newer_scan() {
    let _g = SERIAL.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let s = Setup::new(MOSS);
    let scan = s.scan();
    let (w, h) = create_size(MOSS_KEY);
    s.mock.queue_outputs([png(&ball(w, h, [230, 30, 30])), png(&li_ai::refine::test_image()), png(&filled(&red_ball_on_sky))]);
    let opts = s.options();
    let mut changed = false;
    let done = modelsetup::verify_models(&opts, &scan, &[MOSS_KEY.into()], &JobControl::new(), &mut |_, _| {
        if !changed {
            changed = true;
            modelsetup::state::SetupState::update(&opts.state_path, |state| {
                let m = state.servers.get_mut(s.mock.host()).unwrap().models.get_mut(MOSS_KEY).unwrap();
                m.fingerprint.bytes += 1;
                m.status = Status::NeedsReverify;
            })
            .unwrap();
        }
    })
    .unwrap();
    assert!(done[0].result.as_ref().unwrap_err().contains("changed"), "a stale result must be reported instead of accepted");
    let state = modelsetup::state::SetupState::load(&opts.state_path);
    let current = &state.servers[s.mock.host()].models[MOSS_KEY];
    assert_eq!(current.status, Status::NeedsReverify);
    assert_eq!(current.fingerprint.bytes, scan.server.models[MOSS_KEY].fingerprint.bytes + 1);
    assert!(current.verified.is_none(), "the newer file was never tested");
}
