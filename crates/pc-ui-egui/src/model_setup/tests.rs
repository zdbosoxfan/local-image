#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use li_ai::arch::FileKind;
use li_ai::companions::{Binding, Why};
use li_ai::identify::{Identity, Quant, QuantClass};
use li_ai::modelsetup::check::CapabilityCheck;
use li_ai::modelsetup::state::{Fingerprint, Verified};
use li_ai::modelsetup::{Gpu, Snapshot, TemplateChoice};
use li_ai::template::index::Version;

use super::*;

const FLUX3: &str = "unet:flux3-image-klein-bf16.safetensors";
const MOSS: &str = "unet:moss_image_sft_bf16.safetensors";
const KREA: &str = "unet:krea2_turbo_bf16.safetensors";
const TURBO: &str = "unet:qwen_image_2.1_turbo_bf16.safetensors";
const KLEIN9B: &str = "unet:flux-2-klein-9b-fp8.safetensors";
const HIDREAM: &str = "ckpt:hidream_o1_image_bf16.safetensors";
const MING: &str = "unet:ming_image_0.1_design_bf16.safetensors";
const QWEN21: &str = "unet:qwen_image_2.1_bf16.safetensors";

fn snapshot(version: &str) -> Snapshot {
    Snapshot {
        host: "127.0.0.1:8189".into(),
        comfyui_version: Version::parse(version),
        version_text: version.into(),
        templates_version: "0.11.96".into(),
        gpu: Some(Gpu { name: "NVIDIA GeForce RTX 5090".into(), vram_total: 33_731_248_128 }),
        ..Default::default()
    }
}

fn cap(name: &str, references: u32) -> CapabilityCheck {
    CapabilityCheck { name: name.into(), ok: true, references, detail: String::new() }
}

fn model(key: &str, label: &str, status: Status, arch: &str, variant: Option<&str>) -> ModelState {
    let (prefix, file) = key.split_once(':').unwrap();
    ModelState {
        status,
        file: file.into(),
        folder: if prefix == "ckpt" { "checkpoints".into() } else { "diffusion_models".into() },
        label: label.into(),
        engine: "template".into(),
        catalog_id: key.into(),
        fingerprint: Fingerprint { bytes: 11_097_404_512, ..Default::default() },
        identity: Some(Identity {
            kind: if prefix == "ckpt" { FileKind::Checkpoint } else { FileKind::DiffusionModel },
            arch: Some(arch.to_owned()).filter(|a| !a.is_empty()),
            variant: variant.map(str::to_owned),
            quant: Quant { class: Some(QuantClass::Bf16), ..Default::default() },
            evidence: "content_mode_blocks + single_stream_modulations (FLUX.3)".into(),
            ..Default::default()
        }),
        capabilities: vec![cap("create", 0), cap("edit", 10), cap("refine", 0), cap("inpaint", 0), cap("upscale", 0)],
        ..Default::default()
    }
}

fn issue(code: &str, key: &str, args: &[(&str, &str)]) -> Issue {
    Issue::new(code, Some(key), &args.iter().map(|(k, v)| (*k, (*v).to_owned())).collect::<Vec<_>>())
}

/// One model in each state Set Up Models shows.
fn fixture() -> Scan {
    let mut models = std::collections::BTreeMap::new();
    let mut flux3 = model(FLUX3, "FLUX.3 Klein · distilled · BF16", Status::Ready, "flux3-klein", Some("distilled"));
    flux3.templates = [("create".into(), "image_flux3_klein_distilled_t2i".into()), ("edit".into(), "image_flux3_klein_distilled_image_edit".into())].into();
    flux3.bindings = vec![
        Binding {
            role: "clip".into(),
            declared: "qwen3vl_4b_int8_convrot.safetensors".into(),
            used: Some("qwen3vl_4b_bf16.safetensors".into()),
            why: Why::SameStem,
            ..Default::default()
        },
        Binding {
            role: "vae".into(),
            declared: "flux2-vae.safetensors".into(),
            used: Some("flux2-vae.safetensors".into()),
            why: Why::Exact,
            ..Default::default()
        },
    ];
    flux3.issues = vec![
        issue("NotVerified", FLUX3, &[("model", "FLUX.3 Klein · distilled · BF16")]).fix(Fix::Verify { model: "flux3-image-klein-bf16.safetensors".into() }),
        issue("UpsamplerAvailable", FLUX3, &[("file", "qwen3.5_9b_flux3_prompt_upsampler_w4a8.safetensors"), ("size", "8.1 GB")]).fix(Fix::Download {
            files: vec!["qwen3.5_9b_flux3_prompt_upsampler_w4a8.safetensors".into()],
            folder: "text_encoders".into(),
            bytes: 8_100_000_000,
            url: "https://huggingface.co/Comfy-Org/flux3/resolve/main/text_encoders/qwen3.5_9b_flux3_prompt_upsampler_w4a8.safetensors".into(),
        }),
        issue("LicenceNotice", FLUX3, &[("licence", "FLUX Kommunity License v1.0"), ("summary", "Non-commercial.")]),
    ];
    models.insert(FLUX3.to_owned(), flux3);
    let mut moss = model(MOSS, "MOSS-Image · sft · BF16", Status::Blocked, "moss-image", Some("sft"));
    moss.bindings = vec![Binding {
        role: "vae".into(),
        declared: "flux2-vae.safetensors".into(),
        used: Some("flux2-vae.safetensors".into()),
        why: Why::WrongFolder,
        ..Default::default()
    }];
    moss.issues = vec![
        issue(
            "CompanionMissing",
            MOSS,
            &[
                ("model", "MOSS-Image · sft · BF16"),
                ("role_label", "text encoder"),
                ("file", "qwen3.5_9b_bf16.safetensors"),
                ("size", "19.3 GB"),
                ("folder", "text_encoders"),
                ("stem", "qwen3.5_9b"),
            ],
        )
        .fix(Fix::Download {
            files: vec!["qwen3.5_9b_bf16.safetensors".into()],
            folder: "text_encoders".into(),
            bytes: 19_306_312_328,
            url: "https://huggingface.co/Comfy-Org/MOSS-Image/resolve/main/text_encoders/qwen3.5_9b_bf16.safetensors".into(),
        })
        .fix(Fix::UseInstead { declared: "qwen3.5_9b_bf16.safetensors".into(), installed: "qwen3.5_9b_fp8.safetensors".into() }),
        issue(
            "WrongFolder",
            MOSS,
            &[("file", "flux2-vae.safetensors"), ("kind_label", "VAE"), ("kind_label_plural", "VAEs"), ("found", "diffusion_models"), ("expected", "vae")],
        )
        .fix(Fix::Move { from: "flux2-vae.safetensors".into(), to_folder: "vae".into() }),
        issue("ComfyTooOld", MOSS, &[("model", "MOSS-Image"), ("need", "0.41.3"), ("have", "0.38.0"), ("how", "Update ComfyUI.")])
            .fix(Fix::CopyCommand { label: "Update ComfyUI".into(), command: "git pull".into() }),
    ];
    moss.capabilities.clear();
    models.insert(MOSS.to_owned(), moss);
    let mut krea = model(KREA, "Krea 2 · turbo · BF16", Status::Verified, "krea2", Some("turbo"));
    let mut caps = std::collections::BTreeMap::new();
    caps.insert("create".to_owned(), CapabilityResult { pass: true, seconds: 5.5, cold: true, peak_vram_gb: Some(17.9), ..Default::default() });
    caps.insert("refine".to_owned(), CapabilityResult { pass: true, seconds: 3.9, difference: Some(11.6), ..Default::default() });
    caps.insert("upscale".to_owned(), CapabilityResult { pass: true, via: Some("refine".into()), ..Default::default() });
    krea.verified = Some(Verified {
        at: "2026-10-24T14:02:11-04:00".into(),
        seed: 518_877_231,
        comfyui_version: "0.42.5".into(),
        capabilities: caps,
        ..Default::default()
    });
    models.insert(KREA.to_owned(), krea);
    let mut turbo = model(TURBO, "Qwen Image 2.1 · turbo · BF16", Status::Failed, "qwen-image-21", Some("turbo"));
    let mut caps = std::collections::BTreeMap::new();
    caps.insert("create".to_owned(), CapabilityResult { pass: false, seconds: 7.0, failure: Some("VerifyFailed:noise".into()), ..Default::default() });
    turbo.verified = Some(Verified { at: "2026-10-24T14:10:00-04:00".into(), comfyui_version: "0.42.5".into(), capabilities: caps, ..Default::default() });
    turbo.issues = vec![issue("VerifyFailed:noise", TURBO, &[("capability", "Create"), ("vae", "qwen_image_vae.safetensors")])];
    models.insert(TURBO.to_owned(), turbo);
    let mut klein = model(KLEIN9B, "FLUX.2 Klein 9B · FP8", Status::NeedsReverify, "flux2-klein", Some("9b"));
    klein.issues = vec![issue("ReverifyNeeded", KLEIN9B, &[("old", "0.42.5"), ("new", "0.43.0"), ("model", "FLUX.2 Klein 9B · FP8")])];
    models.insert(KLEIN9B.to_owned(), klein);
    let mut hidream = model(HIDREAM, "HiDream O1 · BF16", Status::Blocked, "", None);
    hidream.issues = vec![issue("NodeMissing", HIDREAM, &[("have", "0.43.0"), ("class", "HiDreamO1Sampler"), ("model", "HiDream O1"), ("need_clause", "")])];
    models.insert(HIDREAM.to_owned(), hidream);
    let mut ming = model(MING, "Ming Image · BF16", Status::Unmatched, "", None);
    ming.engine = "unsupported".into();
    ming.issues = vec![issue("NoTemplate", MING, &[("file", "ming_image_0.1_design_bf16.safetensors")]).fix(Fix::CheckOnline).fix(Fix::UpdateProfiles)];
    ming.capabilities.clear();
    models.insert(MING.to_owned(), ming);
    let mut qwen = model(QWEN21, "Qwen Image 2.1 · BF16", Status::Builtin, "qwen-image-21", None);
    qwen.engine = "builtin".into();
    models.insert(QWEN21.to_owned(), qwen);
    models.insert("unet:old_test.safetensors".to_owned(), model("unet:old_test.safetensors", "Old test", Status::Ignored, "", None));
    Scan {
        snapshot: Some(snapshot("0.43.0")),
        server: li_ai::modelsetup::state::ServerState { comfyui_version: "0.43.0".into(), models, ..Default::default() },
        other_files: vec![li_ai::modelsetup::report::FileReport {
            file: "qwen3vl_4b_bf16.safetensors".into(),
            kind: "text encoder".into(),
            used_by: vec!["FLUX.3 Klein · distilled · BF16".into()],
        }],
        templates: vec![
            TemplateChoice { name: "image_flux3_klein_distilled_t2i".into(), title: "FLUX 3 Image Klein Distilled: Text to Image".into() },
            TemplateChoice { name: "image_moss_image_t2i".into(), title: "MOSS-Image: Text to Image".into() },
        ],
        ..Default::default()
    }
}

/// Puts the window on `scan`, recording actions instead of running them.
fn prepare(scan: Scan, selected: &str, step: Step, connected: bool) {
    with(|s| {
        *s = State { open: true, recorded: Some(Vec::new()), ..Default::default() };
        s.set_scan(scan);
        s.selected = Some(selected.to_owned());
        s.step = step;
    });
    crate::ai_ui::TEST_STATUS.with(|s| *s.borrow_mut() = Some(crate::ai_ui::EngineStatus { connected, checked: true, ..Default::default() }));
}

fn harness(scan: Scan, selected: &str, size: egui::Vec2) -> Harness<'static, PhotocraftApp> {
    prepare(scan, selected, Step::Identify, true);
    let mut first = true;
    let mut h = Harness::builder().with_size(size).with_max_steps(64).build_ui_state(
        move |ui, app: &mut PhotocraftApp| {
            if first {
                PhotocraftApp::setup_context(ui.ctx(), crate::theme::ThemeKind::Pro);
                first = false;
                ui.ctx().request_repaint();
                return;
            }
            window(app, ui.ctx());
        },
        PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default()),
    );
    h.run_steps(4);
    h
}

fn recorded() -> Vec<Action> {
    with(|s| s.recorded.clone().unwrap_or_default())
}

fn select(h: &mut Harness<'static, PhotocraftApp>, key: &str, step: Step) {
    with(|s| {
        s.selected = Some(key.to_owned());
        s.step = step;
    });
    h.run_steps(4);
}

#[test]
fn every_state_shows_its_status_reason_and_next_step() {
    let mut h = harness(fixture(), FLUX3, vec2(1100.0, 760.0));
    // The list groups models and names each row with its status.
    h.get_by_label("FLUX.3 Klein · distilled · BF16, ready, not tested");
    h.get_by_label("MOSS-Image · sft · BF16, needs attention");
    h.get_by_label("Krea 2 · turbo · BF16, verified");
    h.get_by_label("Qwen Image 2.1 · turbo · BF16, test failed");
    h.get_by_label("FLUX.2 Klein 9B · FP8, needs a re-check");
    h.get_by_label("HiDream O1 · BF16, needs attention");
    h.get_by_label("Ming Image · BF16, no matching template");
    h.get_by_label("Qwen Image 2.1 · BF16, built-in support");
    for heading in ["New (1)", "Needs attention (5)", "Verified (1)", "Built-in (1)"] {
        h.get_by_label(heading);
    }
    // Identify: what the file is and how it runs.
    h.get_by_label("Official ComfyUI templates");
    h.get_by_label("FLUX 3 Image Klein Distilled: Text to Image");
    assert!(h.query_by_label_contains("How Local Image knows: content_mode_blocks").is_some());
    // The update banner names the versions and counts the models to re-check.
    h.get_by_label("ComfyUI was updated from 0.42.5 to 0.43.0. 1 models need a quick re-check.");
    // Check (ready): nothing blocks; the optional enhancer is offered; what passed is listed.
    select(&mut h, FLUX3, Step::Check);
    h.get_by_label("Nothing is in the way.");
    h.get_by_label("Download 8.1 GB");
    assert!(h.query_by_label_contains("qwen3vl_4b_bf16.safetensors (the template names qwen3vl_4b_int8_convrot.safetensors)").is_some());
    // Blocked: the blockers with their fixes.
    select(&mut h, MOSS, Step::Check);
    assert!(h.query_by_label_contains("needs the text encoder qwen3.5_9b_bf16.safetensors (19.3 GB) in text_encoders/").is_some());
    h.get_by_label("Download 19.3 GB");
    h.get_by_label("Use qwen3.5_9b_fp8.safetensors instead");
    h.get_by_label("Move to vae/");
    h.get_by_label("Copy commands");
    select(&mut h, MOSS, Step::Use);
    h.get_by_label("MOSS-Image · sft · BF16 can't be used yet.");
    // Verified: each test with its time and the GPU memory it used.
    select(&mut h, KREA, Step::Verify);
    h.get_by_label("5.5 s · first run, includes loading");
    h.get_by_label("3.9 s · changed 11.6 of 255 on average");
    h.get_by_label("uses Refine");
    h.get_by_label("GPU memory in use during the test: up to 17.9 GB of 33.7 GB");
    h.get_by_label("Checked on ComfyUI 0.42.5 · 2026-10-24 14:02:11-04:00 · seed 518877231");
    // Failed: the failure in plain words.
    select(&mut h, TURBO, Step::Verify);
    assert!(h.query_by_label_contains("The Create test image is noise, not a picture. The VAE (qwen_image_vae.safetensors)").is_some());
    // A missing node, and no template: the reason and what to do.
    select(&mut h, HIDREAM, Step::Check);
    h.get_by_label("This ComfyUI (0.43.0) doesn't have the node HiDreamO1Sampler that HiDream O1 uses. Update ComfyUI.");
    select(&mut h, MING, Step::Check);
    h.get_by_label("Choose template…");
    h.get_by_label("Check online…");
    // Use: a usable model goes to Generate.
    select(&mut h, FLUX3, Step::Use);
    h.get_by_label("FLUX.3 Klein · distilled · BF16 is ready (not tested yet).");
    h.get_by_label("Use in Generate").click();
    h.run_steps(2);
    assert_eq!(recorded(), [Action::UseInGenerate(FLUX3.into())]);
}

#[test]
fn fix_buttons_dispatch_their_setup_calls() {
    let mut h = harness(fixture(), MOSS, vec2(1100.0, 760.0));
    select(&mut h, MOSS, Step::Check);
    h.get_by_label("Use qwen3.5_9b_fp8.safetensors instead").click();
    h.run_steps(2);
    h.get_by_label("Download 19.3 GB").click();
    h.run_steps(2);
    h.get_by_label("Copy commands").click();
    h.run_steps(2);
    // Moving asks first.
    h.get_by_label("Move to vae/").click();
    h.run_steps(3);
    h.get_by_label("Move flux2-vae.safetensors to vae/?");
    h.get_by_label("Move").click();
    h.run_steps(3);
    let got = recorded();
    assert_eq!(
        got[..3],
        [
            Action::Choose(MOSS.into(), Choice::UseInstead { declared: "qwen3.5_9b_bf16.safetensors".into(), installed: "qwen3.5_9b_fp8.safetensors".into() }),
            Action::PlanDownload(Fix::Download {
                files: vec!["qwen3.5_9b_bf16.safetensors".into()],
                folder: "text_encoders".into(),
                bytes: 19_306_312_328,
                url: "https://huggingface.co/Comfy-Org/MOSS-Image/resolve/main/text_encoders/qwen3.5_9b_bf16.safetensors".into(),
            }),
            Action::Copy("git pull".into()),
        ]
    );
    assert_eq!(got[3], Action::Move { file: "flux2-vae.safetensors".into(), to_folder: "vae".into() });
    // A model no template matched: choose one.
    select(&mut h, MING, Step::Check);
    h.get_by_label("Choose template…").click();
    h.run_steps(3);
    h.get_by_label("MOSS-Image: Text to Image").click();
    h.run_steps(3);
    assert_eq!(recorded().last(), Some(&Action::Choose(MING.into(), Choice::Template(Some("image_moss_image_t2i".into())))));
    assert!(with(|s| s.choose_template.is_none()));
    // A download plan waits for the go-ahead.
    let plan = li_ai::browser::Plan {
        files: vec![li_ai::browser::PlannedFile {
            name: "qwen3.5_9b_bf16.safetensors".into(),
            folder: "text_encoders".into(),
            url: "https://huggingface.co/x".into(),
            bytes: 19_306_312_328,
            sha256: "0".repeat(64),
        }],
        ..Default::default()
    };
    with(|s| s.confirm_download = Some(plan.clone()));
    h.run_steps(3);
    h.get_by_label("Download 19.3 GB?");
    h.get_by_label("Download").click();
    h.run_steps(3);
    assert_eq!(recorded().last(), Some(&Action::Download(plan)));
}

#[test]
fn verify_all_tests_the_untested_models_and_cancel_stops_it() {
    let mut h = harness(fixture(), FLUX3, vec2(1100.0, 760.0));
    h.get_by_label("Verify all ready (2)").click();
    h.run_steps(2);
    assert_eq!(recorded(), [Action::Verify(vec![KLEIN9B.into(), FLUX3.into()])]);
    // While a model is tested its page shows progress and Cancel; one task runs at a time.
    let task = Arc::new(Mutex::new(Task { label: "Testing…".into(), verifying: vec![FLUX3.into()], current: Some(FLUX3.into()), ..Default::default() }));
    task.lock().unwrap().steps.push((FLUX3.into(), "create".into(), CapabilityResult { pass: true, seconds: 9.8, cold: true, ..Default::default() }));
    with(|s| s.task = Some(task));
    select(&mut h, FLUX3, Step::Verify);
    h.get_by_label("Testing this model…");
    h.get_by_label("9.8 s · first run, includes loading");
    assert!(h.get_by_label("Verify all ready (2)").accesskit_node().is_disabled());
    h.get_all_by_label("Cancel").next().unwrap().click();
    h.run_steps(2);
    assert_eq!(recorded().last(), Some(&Action::Cancel));
}

#[test]
fn up_and_down_walk_the_list_and_ignore_is_in_the_context_menu() {
    let mut h = harness(fixture(), FLUX3, vec2(1100.0, 760.0));
    h.get_by_label("FLUX.3 Klein · distilled · BF16, ready, not tested").focus();
    h.run_steps(2);
    h.key_press(egui::Key::ArrowDown);
    h.run_steps(3);
    let order = list::order(&with(|s| s.scan.clone()).unwrap());
    let next = order[order.iter().position(|k| k == FLUX3).unwrap() + 1].clone();
    assert_eq!(with(|s| s.selected.clone()), Some(next));
    h.key_press(egui::Key::Enter);
    h.run_steps(3);
    assert_eq!(with(|s| s.step), Step::Check);
    h.get_by_label("FLUX.3 Klein · distilled · BF16, ready, not tested").click_secondary();
    h.run_steps(3);
    h.get_by_label("Ignore").click();
    h.run_steps(2);
    assert_eq!(recorded().last(), Some(&Action::Choose(FLUX3.into(), Choice::Ignore(true))));
}

#[test]
fn an_unreachable_engine_and_an_empty_server_say_what_to_do() {
    let off = Scan { issues: vec![Issue::new("ServerUnreachable", None, &[("host", "127.0.0.1:8189".into())])], ..Default::default() };
    let mut h = harness(off, "", vec2(1000.0, 640.0));
    crate::ai_ui::TEST_STATUS.with(|s| *s.borrow_mut() = Some(crate::ai_ui::EngineStatus { checked: true, ..Default::default() }));
    h.run_steps(3);
    h.get_by_label("Start the AI engine to check new models.");
    assert!(h.query_by_label_contains("No new models. Add model files to").is_some());
    h.get_by_label("Open Model Browser").click();
    h.run_steps(2);
    h.get_by_label("Start AI Engine").click();
    h.run_steps(2);
    assert_eq!(recorded(), [Action::OpenModelBrowser, Action::StartEngine]);
}

#[test]
fn a_rise_in_new_models_posts_one_notice() {
    let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
    with(|s| s.seen_new = None);
    let status = |new: &[&str]| {
        let mut st = crate::ai_ui::EngineStatus { connected: true, checked: true, ..Default::default() };
        st.setup.new = new.iter().map(|k| (*k).to_owned()).collect();
        crate::ai_ui::TEST_STATUS.with(|s| *s.borrow_mut() = Some(st));
    };
    status(&[FLUX3]);
    notice_new_models(&mut app);
    assert!(app.ui.notices.is_empty(), "the count at start-up is the banner's");
    status(&[FLUX3, MOSS]);
    notice_new_models(&mut app);
    notice_new_models(&mut app);
    assert_eq!(app.ui.notices.len(), 1);
    assert_eq!(app.ui.notices[0].title, "2 new models found");
}

/// Each state at 1440x900 and 1024x700 in both themes, into `MODEL_SETUP_SHOTS` (default
/// `target/design-system/`).
#[test]
#[ignore = "design-system screenshots: run explicitly with --ignored"]
fn model_setup_screens() {
    let dir = std::env::var_os("MODEL_SETUP_SHOTS")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/design-system"));
    std::fs::create_dir_all(&dir).unwrap();
    let shots: [(&str, &str, Step); 9] = [
        ("ready-identify", FLUX3, Step::Identify),
        ("ready-check", FLUX3, Step::Check),
        ("blocked-check", MOSS, Step::Check),
        ("verified-verify", KREA, Step::Verify),
        ("failed-verify", TURBO, Step::Verify),
        ("needs-reverify-check", KLEIN9B, Step::Check),
        ("missing-node-check", HIDREAM, Step::Check),
        ("unmatched-check", MING, Step::Check),
        ("ready-use", FLUX3, Step::Use),
    ];
    for (theme, name) in [(crate::theme::ThemeKind::Pro, "dark"), (crate::theme::ThemeKind::StudioLight, "light")] {
        for size in [vec2(1440.0, 900.0), vec2(1024.0, 700.0)] {
            for (shot, key, step) in shots {
                prepare(fixture(), key, step, true);
                let mut h = Harness::builder().with_size(size).with_max_steps(64).renderer(crate::prefs_ui::design_system_shots::renderer()).build_ui_state(
                    move |ui, app: &mut PhotocraftApp| {
                        if crate::prefs_ui::design_system_shots::fonts_ready(ui) {
                            crate::theme::apply(ui.ctx(), theme);
                            window(app, ui.ctx());
                        }
                    },
                    PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default()),
                );
                h.run_steps(8);
                h.render().unwrap().save(dir.join(format!("model-setup-{shot}-{name}-{}x{}.png", size.x as u32, size.y as u32))).unwrap();
            }
        }
    }
}
