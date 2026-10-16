//! Real-pointer acceptance for import safety, catalog preferences and PDF proof sheets.
use crate::{LightcraftApp, Services, headless::Headless, state::Dialog};
use lightcraft_engine::Session;
use serde_json::json;
use std::time::Duration;
const WAIT: Duration = Duration::from_secs(30);
fn click(h: &mut Headless, id: &str) {
    let r = h.request("ui.clickWidget", json!({"id":id}), WAIT);
    assert_eq!(r["ok"], true, "{id}: {r}");
    h.step();
}
fn diagnostic_shot(h: &Headless, name: &str) {
    let image = h.paint();
    let out = lightcraft_raster::Rgba8::from_fn(image.size[0], image.size[1], |x, y| image.pixels[y * image.size[0] + x].to_array());
    let bytes = lightcraft_codecs::encode_png(&lightcraft_codecs::EncodeImage::rgba8(&out), &Default::default()).unwrap();
    let target = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/library-safety");
    std::fs::create_dir_all(&target).unwrap();
    std::fs::write(target.join(name), bytes).unwrap();
}
fn temp(tag: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("li-safety-ui-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}
#[test]
fn safety_import_real_clicks_preview_and_second_copy_roundtrip() {
    let root = temp("import");
    let second = root.join("second");
    std::fs::create_dir(&second).unwrap();
    let photo = root.join("source.png");
    let img = lightcraft_raster::Rgba8::from_fn(48, 32, |x, y| [x as u8, y as u8, 70, 255]);
    let bytes = lightcraft_codecs::encode_png(&lightcraft_codecs::EncodeImage::rgba8(&img), &Default::default()).unwrap();
    std::fs::write(&photo, &bytes).unwrap();
    let mut s = Session::new().with_fs();
    s.open_library(root.join("library"), false).unwrap();
    let backup = second.to_string_lossy().to_string();
    let mut app = LightcraftApp::new(s, Services { pick_folder: Some(Box::new(move || Some(backup.clone()))), png: None, ..Default::default() });
    app.ui.settings.gpu = false;
    let candidate = lightcraft_engine::import::ImportCandidate {
        path: photo.display().to_string(),
        name: "source.png".into(),
        format: "PNG".into(),
        width: 48,
        height: 32,
        ..Default::default()
    };
    app.ui.dialog = Some(Dialog::Import { opts: Box::new(crate::import::ImportDialog::new(vec![candidate])) });
    let manual = std::sync::Arc::new(lightcraft_engine::cmd::previews::PreviewBuild { total: 77, ..Default::default() });
    app.session.preview_build = Some(manual.clone());
    let mut h = Headless::new(app, [1400., 1300.], 1.);
    for _ in 0..5 {
        h.step();
    }
    for (index, expected) in [(1, "standard"), (2, "full"), (3, "smart"), (0, "")] {
        click(&mut h, "dropdown:importPreviews");
        click(&mut h, &format!("button:importPreviews-{index}"));
        let Some(Dialog::Import { opts }) = &h.app.ui.dialog else { panic!("dialog disappeared") };
        assert_eq!(opts.previews, expected);
    }
    click(&mut h, "check:importSecondCopy");
    click(&mut h, "button:importSecondCopyFolder");
    click(&mut h, "dropdown:importPreviews");
    click(&mut h, "button:importPreviews-1");
    click(&mut h, "button:dialogOk");
    assert!(h.step_until(WAIT, |h| h.app.import.is_none()));
    let p = h.app.session.catalog.photos().next().unwrap();
    assert_eq!(p.file_name, "source.png");
    assert_eq!(std::fs::read(second.join("source.png")).unwrap(), bytes);
    assert!(std::sync::Arc::ptr_eq(h.app.session.preview_build.as_ref().unwrap(), &manual), "manual build must finish first");
    assert_eq!(h.app.import_preview_queue.len(), 1, "import must finish without waiting for previews");
    manual.finished.store(true, std::sync::atomic::Ordering::Relaxed);
    h.step();
    assert!(!std::sync::Arc::ptr_eq(h.app.session.preview_build.as_ref().unwrap(), &manual));
    assert_eq!(h.app.session.preview_build.as_ref().unwrap().total, 1);
    let prefs = &h.app.ui.settings.import_defaults;
    assert_eq!(prefs.previews, "standard");
    assert!(prefs.second_copy_enabled);
    assert_eq!(prefs.second_copy, second.display().to_string());
    let old: crate::import::ImportPreferences = serde_json::from_str("{}").unwrap();
    assert!(!old.second_copy_enabled);
    assert_eq!(old.previews, "");
    let decoded: crate::import::ImportPreferences = serde_json::from_value(serde_json::to_value(prefs).unwrap()).unwrap();
    assert_eq!(&decoded, prefs);
    assert!(h.step_until(WAIT, |h| h.app.session.preview_build.as_ref().is_none_or(|s| s.finished.load(std::sync::atomic::Ordering::Relaxed))));
    drop(h);
    std::fs::remove_dir_all(root).unwrap();
}
#[test]
fn safety_backup_preferences_real_clicks_and_persisted_reopen() {
    let root = temp("backup");
    let output = root.join("output");
    std::fs::create_dir(&output).unwrap();
    let folder = output.display().to_string();
    let mut s = Session::new();
    s.open_library(root.join("library"), false).unwrap();
    let mut app = LightcraftApp::new(s, Services { pick_folder: Some(Box::new(move || Some(folder.clone()))), png: None, ..Default::default() });
    app.ui.settings.gpu = false;
    app.ui.dialog = Some(Dialog::Settings { tab: "general".into() });
    let mut h = Headless::new(app, [1400., 1300.], 1.5);
    for _ in 0..5 {
        h.step();
    }
    click(&mut h, "dropdown:backupFrequency");
    click(&mut h, "button:backupFrequency-1");
    click(&mut h, "dropdown:backupKeep");
    click(&mut h, "button:backupKeep-1");
    click(&mut h, "button:backupFolder");
    click(&mut h, "button:verifyLibrary");
    click(&mut h, "button:backupNow");
    assert_eq!(h.app.session.backup_preferences.keep, 3);
    assert_eq!(h.app.session.backup_preferences.folder, output.display().to_string());
    assert_eq!(std::fs::read_dir(&output).unwrap().count(), 1);
    h.app.session.close_library().unwrap();
    drop(h);
    let mut s = Session::new();
    s.open_library(root.join("library"), false).unwrap();
    assert_eq!(s.backup_preferences.frequency, lightcraft_engine::library_safety::BackupFrequency::EveryQuit);
    assert_eq!(s.backup_preferences.keep, 3);
    drop(s);
    std::fs::remove_dir_all(root).unwrap();
}
#[test]
fn safety_proof_sheet_real_clicks_export_background_pdf() {
    let root = temp("proof");
    let folder = root.display().to_string();
    let mut app = LightcraftApp::new(
        Session::with_demo(),
        Services { pick_folder: Some(Box::new(move || Some(folder.clone()))), png: None, ..Default::default() },
    );
    app.ui.settings.gpu = false;
    app.run("dialog.proofSheet", json!({})).unwrap();
    let mut h = Headless::new(app, [1400., 1100.], 1.5);
    for _ in 0..5 {
        h.step();
    }
    let page = h.app.widgets.iter().find(|(id, _)| id == "dropdown:proofPage").unwrap().1;
    for _ in 0..3 {
        h.step();
        assert_eq!(
            h.app.widgets.iter().find(|(id, _)| id == "dropdown:proofPage").unwrap().1,
            page,
            "dialog targets must stay still between input frames"
        );
    }
    diagnostic_shot(&h, "proof-dialog-before.png");
    click(&mut h, "dropdown:proofPage");
    diagnostic_shot(&h, "proof-dialog-popup.png");
    click(&mut h, "button:proofPage-1");
    click(&mut h, "check:proofLandscape");
    click(&mut h, "dropdown:proofCaption");
    click(&mut h, "button:proofCaption-3");
    click(&mut h, "dropdown:proofColumns");
    click(&mut h, "button:proofColumns-1");
    click(&mut h, "dropdown:proofRows");
    click(&mut h, "button:proofRows-1");
    click(&mut h, "field:proofWatermark");
    h.request("ui.text", json!({"text":"CLIENT PROOF"}), WAIT);
    click(&mut h, "button:proofFolder");
    let Some(Dialog::ProofSheet { opts, .. }) = &h.app.ui.dialog else { panic!("no proof dialog") };
    assert_eq!(opts.page, lightcraft_engine::proof_sheet::PageSize::Letter);
    assert!(opts.landscape);
    assert_eq!(opts.watermark, "CLIENT PROOF");
    click(&mut h, "button:dialogOk");
    assert!(h.step_until(WAIT, |h| h.app.proof.is_none()));
    let bytes = std::fs::read(root.join("Proof Sheet.pdf")).unwrap();
    assert!(bytes.starts_with(b"%PDF-"));
    let destination = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/library-safety");
    std::fs::create_dir_all(&destination).unwrap();
    std::fs::write(destination.join("proof-ui.pdf"), bytes).unwrap();
    drop(h);
    std::fs::remove_dir_all(root).unwrap();
}
#[test]
#[ignore = "visual QA: run explicitly for dark/light, narrow/normal and HiDPI"]
fn safety_visual_screenshots() {
    use crate::{headless::HeadlessView, theme::Tokens};
    for dark in [true, false] {
        for width in [250., 400.] {
            for ppp in [1., 1.5] {
                let mut app = LightcraftApp::new(Session::new(), Services::default());
                let dir = temp(&format!("shots-{}-{width}-{ppp}", dark));
                app.session.open_library(&dir, false).unwrap();
                let tokens = if dark { Tokens::default() } else { super::tests_develop_c::light() };
                app.set_host_theme(tokens);
                for kind in ["proof", "backup", "import"] {
                    let mut view = HeadlessView::new();
                    let mut opts = Default::default();
                    let mut folder = String::new();
                    let mut name = "Proof Sheet.pdf".into();
                    let mut import = crate::import::ImportDialog { second_copy_enabled: true, ..Default::default() };
                    for frame in 0..3 {
                        view.run(HeadlessView::raw_input(egui::vec2(width, 950.), ppp, frame as f64 / 60., vec![]), |ui| {
                            crate::theme::apply_tokens(ui.ctx(), tokens);
                            egui::Frame::NONE.fill(tokens.chrome).inner_margin(ui.spacing().window_margin).show(ui, |ui| match kind {
                                "proof" => crate::proof_sheet::body(&mut app, ui, &mut opts, &mut folder, &mut name),
                                "backup" => crate::library_safety_ui::settings(&mut app, ui),
                                _ => crate::import::safety_options(&mut app, ui, &mut import),
                            });
                        });
                    }
                    let image = view.paint(&Default::default());
                    let out = lightcraft_raster::Rgba8::from_fn(image.size[0], image.size[1], |x, y| image.pixels[y * image.size[0] + x].to_array());
                    let bytes = lightcraft_codecs::encode_png(&lightcraft_codecs::EncodeImage::rgba8(&out), &Default::default()).unwrap();
                    let target = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/library-safety");
                    std::fs::create_dir_all(&target).unwrap();
                    std::fs::write(target.join(format!("{kind}-{}-{width}-{ppp}.png", if dark { "dark" } else { "light" })), bytes).unwrap();
                }
                drop(app);
                std::fs::remove_dir_all(dir).unwrap();
            }
        }
    }
}

#[test]
#[ignore = "requires a real hardware GPU; explicit interactive-path quality/timing evidence"]
fn safety_proof_sheet_gpu_quality_and_interactive_timing() {
    let root = temp("gpu");
    let photo = root.join("reference.png");
    let img = lightcraft_raster::Rgba8::from_fn(1024, 768, |x, y| {
        let c = if (x / 8 + y / 8) % 2 == 0 { 20 } else { 230 };
        [(x * 255 / 1023) as u8, (y * 255 / 767) as u8, c, 255]
    });
    let bytes = lightcraft_codecs::encode_png(&lightcraft_codecs::EncodeImage::rgba8(&img), &Default::default()).unwrap();
    std::fs::write(&photo, bytes).unwrap();
    let mut s = Session::new().with_fs();
    s.execute("library.import", &json!({"paths":[photo]})).unwrap();
    s.execute("develop.set", &json!({"control":"light.exposure","value":0.5})).unwrap();
    let id = s.catalog.photos().next().unwrap().id;
    let mut app = LightcraftApp::new(s, Services { png: None, ..Default::default() });
    app.ui.settings.gpu = false;
    let mut h = Headless::new(app, [1400., 1100.], 1.);
    h.step();
    let (reference_w, reference_h) =
        lightcraft_engine::proof_sheet::ProofOptions { rows: 1, columns: 1, ..Default::default() }.render_bounds().unwrap();
    let cpu = h
        .app
        .session
        .render_export(id, reference_w, reference_h, lightcraft_pipeline::OutputSpace::Srgb, lightcraft_pipeline::OutputDepth::U8)
        .unwrap()
        .image;
    h.app.ui.settings.gpu = true;
    h.step();
    assert!(lightcraft_engine::gpu::available(), "real GPU required: {:?}", lightcraft_engine::gpu::unavailable_reason());
    let adapter = lightcraft_engine::gpu::adapter_name().unwrap();
    assert!(!adapter.to_ascii_lowercase().contains("llvmpipe"), "{adapter}");
    let gpu = h
        .app
        .session
        .render_export(id, reference_w, reference_h, lightcraft_pipeline::OutputSpace::Srgb, lightcraft_pipeline::OutputDepth::U8)
        .unwrap()
        .image;
    let mut sum = 0u64;
    let mut max = 0u8;
    for (a, b) in cpu.data.iter().zip(&gpu.data) {
        for c in 0..3 {
            let d = a[c].abs_diff(b[c]);
            sum += u64::from(d);
            max = max.max(d);
        }
    }
    let mean = sum as f64 / (cpu.data.len() * 3) as f64;
    assert!(mean < 0.5 && max <= 3, "mean={mean} max={max}");
    let target = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/library-safety");
    std::fs::create_dir_all(&target).unwrap();
    for (name, img) in [("proof-reference-cpu.png", &cpu), ("proof-reference-gpu.png", &gpu)] {
        let bytes = lightcraft_codecs::encode_png(&lightcraft_codecs::EncodeImage::rgba8(img), &Default::default()).unwrap();
        std::fs::write(target.join(name), bytes).unwrap();
    }
    h.app.run("dialog.proofSheet", json!({})).unwrap();
    if let Some(Dialog::ProofSheet { opts, folder, name }) = &mut h.app.ui.dialog {
        opts.rows = 1;
        opts.columns = 1;
        *folder = target.display().to_string();
        *name = "proof-reference.pdf".into();
    }
    let _ = std::fs::remove_file(target.join("proof-reference.pdf"));
    for _ in 0..3 {
        h.step();
    }
    let start = std::time::Instant::now();
    click(&mut h, "button:dialogOk");
    let click_ms = start.elapsed().as_secs_f64() * 1000.;
    assert!(h.step_until(WAIT, |h| h.app.proof.is_none()));
    let finish_ms = start.elapsed().as_secs_f64() * 1000.;
    let evidence = format!(
        "adapter={adapter}\nRGB mean abs difference={mean:.6} LSB\nRGB maximum abs difference={max} LSB\nUI confirm returned={click_ms:.3} ms\nPDF complete={finish_ms:.3} ms\n"
    );
    eprintln!("{evidence}");
    std::fs::write(target.join("proof-quality.txt"), evidence).unwrap();
    assert!(target.join("proof-reference.pdf").is_file());
    drop(h);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn safety_proof_cancel_real_click_keeps_output_unpublished() {
    use std::sync::{Arc, Condvar, Mutex};
    let root = temp("cancel");
    let source = root.join("source.png");
    let img = lightcraft_raster::Rgba8::from_fn(48, 32, |x, y| [x as u8, y as u8, 110, 255]);
    let bytes = lightcraft_codecs::encode_png(&lightcraft_codecs::EncodeImage::rgba8(&img), &Default::default()).unwrap();
    std::fs::write(&source, bytes).unwrap();
    let mut s = Session::new().with_fs();
    s.execute("library.import", &json!({"paths":[source]})).unwrap();
    let load = s.media.file_loader.clone().unwrap();
    let gate = Arc::new((Mutex::new(false), Condvar::new()));
    let held = gate.clone();
    let (started, ready) = std::sync::mpsc::channel();
    s.media.file_loader = Some(Arc::new(move |path, edge| {
        if edge == usize::MAX {
            started.send(()).unwrap();
            let (lock, wake) = &*held;
            let (released, timeout) = wake.wait_timeout_while(lock.lock().unwrap(), Duration::from_secs(10), |released| !*released).unwrap();
            if timeout.timed_out() && !*released {
                return Err("fixture timed out".into());
            }
        }
        load(path, edge)
    }));
    let mut app = LightcraftApp::new(s, Services { png: None, ..Default::default() });
    app.ui.settings.gpu = false;
    app.ui.view = crate::state::ViewMode::PhotoGrid;
    app.ui.dialog = Some(Dialog::ProofSheet { opts: Default::default(), folder: root.display().to_string(), name: "cancelled.pdf".into() });
    let mut h = Headless::new(app, [1400., 1100.], 1.);
    for _ in 0..5 {
        h.step();
    }
    click(&mut h, "button:dialogOk");
    ready.recv_timeout(WAIT).unwrap();
    h.step();
    click(&mut h, "button:proofCancel");
    let (lock, wake) = &*gate;
    *lock.lock().unwrap() = true;
    wake.notify_all();
    assert!(h.step_until(WAIT, |h| h.app.proof.is_none()));
    assert!(!root.join("cancelled.pdf").exists());
    drop(h);
    std::fs::remove_dir_all(root).unwrap();
}
