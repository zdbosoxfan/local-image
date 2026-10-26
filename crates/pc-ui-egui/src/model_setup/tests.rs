#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use super::fixtures::*;
use super::*;
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};

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
    h.get_by_label("FLUX.3 Klein Distilled · BF16, ready, not tested");
    h.get_by_label("MOSS-Image SFT · BF16, needs attention");
    h.get_by_label("Krea 2 Turbo · BF16, verified");
    h.get_by_label("Qwen Image 2.1 Turbo · BF16, test failed");
    h.get_by_label("FLUX.2 Klein 9B · FP8, needs testing again");
    h.get_by_label("HiDream O1 · BF16, needs attention");
    h.get_by_label("Ming Image · BF16, no matching template");
    h.get_by_label("Qwen Image 2.1 · BF16, built-in support");
    for heading in ["NEW · 1", "NEEDS ATTENTION · 5", "VERIFIED · 1", "BUILT-IN · 1"] {
        h.get_by_label(heading);
    }
    // Identify: what the file is and how it runs.
    h.get_by_label("Official ComfyUI templates");
    h.get_by_label("FLUX 3 Image Klein Distilled: Text to Image");
    h.get_by_label("FLUX.3 Klein").hover();
    h.run_steps(60);
    assert!(h.query_by_label_contains("How Local Image knows: content_mode_blocks").is_some());
    // The update banner names the versions and counts the models to re-check.
    h.get_by_label("ComfyUI was updated from 0.42.5 to 0.43.0. 1 model needs testing again.");
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
    h.get_by_label("MOSS-Image SFT · BF16 can't be used yet.");
    // Verified: each test with its time and the GPU memory it used.
    select(&mut h, KREA, Step::Verify);
    h.get_by_label("Create passed: 5.5 s · first run, includes loading");
    h.get_by_label("Refine passed: 3.9 s · changed 11.6 of 255 on average");
    h.get_by_label("Upscale passed: uses Refine");
    h.get_by_label("GPU memory in use during the test: up to 17.9 GB of 33.7 GB");
    h.get_by_label("Checked on ComfyUI 0.42.5 · October 24, 2026 at 2:02:11 PM").hover();
    h.run_steps(60);
    h.get_by_label("Checked on ComfyUI 0.42.5 · 2026-10-24 14:02:11-04:00 · seed 518877231");
    // Failed: the failure in plain words.
    select(&mut h, TURBO, Step::Verify);
    assert!(h.query_by_label_contains("The Create test image is noise, not a picture. The VAE (qwen_image_vae.safetensors)").is_some());
    // A missing node, and no template: the reason and what to do.
    select(&mut h, HIDREAM, Step::Check);
    h.get_by_label("Needs attention: This ComfyUI (0.43.0) doesn't have the node HiDreamO1Sampler that HiDream O1 uses. Update ComfyUI.");
    select(&mut h, MING, Step::Check);
    h.get_by_label("Choose template…");
    h.get_by_label("Check online…");
    // Use: a usable model goes to Generate.
    select(&mut h, FLUX3, Step::Use);
    h.get_by_label("FLUX.3 Klein Distilled · BF16 is ready (not tested yet).");
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
    h.get_by_label("Create passed: 9.8 s · first run, includes loading");
    assert!(h.get_by_label("Verify all ready (2)").accesskit_node().is_disabled());
    h.get_all_by_label("Cancel").next().unwrap().click();
    h.run_steps(2);
    assert_eq!(recorded().last(), Some(&Action::Cancel));
}

#[test]
fn up_and_down_walk_the_list_and_ignore_is_in_the_context_menu() {
    let mut h = harness(fixture(), FLUX3, vec2(1100.0, 760.0));
    h.get_by_label("FLUX.3 Klein Distilled · BF16, ready, not tested").focus();
    h.run_steps(2);
    h.key_press(egui::Key::ArrowDown);
    h.run_steps(3);
    let order = list::order(&with(|s| s.scan.clone()).unwrap());
    let next = order[order.iter().position(|k| k == FLUX3).unwrap() + 1].clone();
    assert_eq!(with(|s| s.selected.clone()), Some(next));
    h.key_press(egui::Key::Enter);
    h.run_steps(3);
    assert_eq!(with(|s| s.step), Step::Check);
    h.get_by_label("FLUX.3 Klein Distilled · BF16, ready, not tested").click_secondary();
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

#[test]
fn the_window_and_footer_stay_put_across_repaints() {
    for size in [vec2(1440.0, 900.0), vec2(1024.0, 700.0), vec2(960.0, 600.0)] {
        let mut h = harness(fixture(), FLUX3, size);
        h.run_steps(8);
        let before = h.get_by_label("Copy report").rect();
        h.run_steps(120);
        let after = h.get_by_label("Copy report").rect();
        assert!((after.bottom() - before.bottom()).abs() < 0.5 && (after.right() - before.right()).abs() < 0.5, "footer drifted: {before:?} -> {after:?}");
        assert!(after.bottom() < size.y - 20.0, "window covers the status bar: {after:?}");
    }
}

#[test]
fn reverify_has_an_action_and_results_name_their_outcomes() {
    let mut h = harness(fixture(), KLEIN9B, vec2(1024.0, 700.0));
    select(&mut h, KLEIN9B, Step::Check);
    h.get_by_label("Verify again").click();
    h.run_steps(3);
    assert_eq!(with(|s| s.step), Step::Verify);
    select(&mut h, KREA, Step::Verify);
    h.get_by_label("Create passed");
    h.get_by_label("Refine passed");
    let create = h.get_by_label("Create").rect();
    let refine = h.get_by_label("Refine").rect();
    assert!((create.left() - refine.left()).abs() < 0.5, "result labels must align");
    select(&mut h, TURBO, Step::Verify);
    h.get_by_label_contains("Create failed: 7.0 s");
}

#[test]
fn a_single_test_render_uses_the_singular_label() {
    let mut scan = fixture();
    scan.server.models.get_mut(FLUX3).unwrap().capabilities.retain(|c| c.name == "create");
    let mut h = harness(scan, FLUX3, vec2(1024.0, 700.0));
    select(&mut h, FLUX3, Step::Verify);
    h.get_by_label("Verify · 1 short test render");
}

#[test]
fn remote_fixes_explain_why_they_are_disabled() {
    let mut scan = fixture();
    scan.snapshot.as_mut().unwrap().remote = true;
    let mut h = harness(scan, MOSS, vec2(1440.0, 900.0));
    select(&mut h, MOSS, Step::Check);
    let download = h.get_by_label("Download 19.3 GB");
    assert!(download.accesskit_node().is_disabled());
    download.hover();
    h.run_steps(60);
    h.get_by_label("The model folder is on another computer; install or move the file there.");
}

#[test]
fn identifying_a_model_cannot_replace_a_running_task() {
    let mut h = harness(fixture(), FLUX3, vec2(1100.0, 760.0));
    let task = Arc::new(Mutex::new(Task::default()));
    with(|s| s.task = Some(task.clone()));
    select(&mut h, FLUX3, Step::Identify);
    assert!(h.get_by_label("Change template…").accesskit_node().is_disabled());
    with(|s| start(s, "another task", Vec::new(), |_| panic!("overlapping work must not start")));
    assert!(with(|s| Arc::ptr_eq(s.task.as_ref().unwrap(), &task)));
    assert!(!task.lock().unwrap().ctl.is_cancelled());
    with(|s| {
        s.recorded = None;
        run(s, Action::Cancel);
    });
    assert!(task.lock().unwrap().ctl.is_cancelled());
}

#[test]
fn keyboard_navigation_reveals_rows_and_skips_folded_ignored_models() {
    let mut h = harness(fixture(), FLUX3, vec2(960.0, 600.0));
    h.get_by_label("FLUX.3 Klein Distilled · BF16, ready, not tested").focus();
    h.run_steps(2);
    for _ in 0..7 {
        h.key_press(egui::Key::ArrowDown);
        h.run_steps(6);
        let label = with(|s| {
            let m = s.model(s.selected.as_deref().unwrap()).unwrap();
            format!("{}, {}", m.label, spoken(m.status))
        });
        assert!(h.get_by_label(&label).accesskit_node().is_focused(), "focus follows the selected model");
    }
    h.run_steps(20);
    assert_eq!(with(|s| s.selected.clone()), Some(QWEN21.into()));
    let row = h.get_by_label("Qwen Image 2.1 · BF16, built-in support").rect();
    let footer = h.get_by_label("Copy report").rect();
    assert!(row.bottom() < footer.top(), "the focused row must be visible above the footer: {row:?}");
    assert!(row.top() > h.get_by_label("Re-check").rect().bottom());
    h.key_press(egui::Key::ArrowDown);
    h.run_steps(4);
    assert_eq!(with(|s| s.selected.clone()), Some(QWEN21.into()), "folded ignored rows are not keyboard targets");
    h.key_press(egui::Key::ArrowUp);
    h.run_steps(6);
    assert_eq!(with(|s| s.selected.clone()), Some(KREA.into()));
    assert!(h.get_by_label("Krea 2 Turbo · BF16, verified").accesskit_node().is_focused());
}

#[test]
fn verification_dates_follow_the_interface_language_and_preserve_the_stored_offset() {
    use li_i18n::Lang;
    let original = li_i18n::current();
    for lang in Lang::ALL {
        li_i18n::set_current(lang);
        let expected = li_i18n::dates::display_time("2026-10-24T14:02:11-04:00");
        let mut h = harness(fixture(), KREA, vec2(1100.0, 760.0));
        select(&mut h, KREA, Step::Verify);
        h.get_by_label(&format!("Checked on ComfyUI 0.42.5 · {expected}")).hover();
        h.run_steps(60);
        h.get_by_label("Checked on ComfyUI 0.42.5 · 2026-10-24 14:02:11-04:00 · seed 518877231");
    }
    li_i18n::set_current(original);
}

#[test]
fn keyboard_template_choice_takes_and_returns_focus() {
    let mut h = harness(fixture(), MING, vec2(1100.0, 760.0));
    select(&mut h, MING, Step::Check);
    h.get_by_label("Choose template…").focus();
    h.run_steps(2);
    h.key_press(egui::Key::Space);
    h.run_steps(4);
    assert!(with(|s| s.choose_template.is_some()));
    assert!(h.get_by_role(egui::accesskit::Role::TextInput).accesskit_node().is_focused(), "the chooser takes focus on open");
    for _ in 0..5 {
        h.key_press(egui::Key::Tab);
        h.run_steps(2);
    }
    assert!(h.get_by_role(egui::accesskit::Role::TextInput).accesskit_node().is_focused(), "Tab stays inside the chooser");
    h.key_press(egui::Key::Escape);
    h.run_steps(4);
    assert!(with(|s| s.choose_template.is_none()));
    assert!(h.get_by_label("Choose template…").accesskit_node().is_focused(), "Escape returns focus to the trigger");
    h.key_press(egui::Key::Space);
    h.run_steps(4);
    for _ in 0..3 {
        h.key_press(egui::Key::Tab);
        h.run_steps(2);
    }
    h.key_press(egui::Key::Enter);
    h.run_steps(4);
    assert_eq!(recorded(), [Action::Choose(MING.into(), Choice::Template(Some("image_moss_image_t2i".into())))]);
}
