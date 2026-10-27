//! Synthetic scenes for the offscreen snapshot example, excluded from normal app builds.

use super::fixtures::*;
use super::*;

/// Select a reproducible wizard state without making network requests or starting work.
/// Names: ready-identify/check/verify/use, blocked-check, verified-verify, failed-verify,
/// needs-reverify-check, missing-node-check, unmatched-check, builtin-identify, partial-verify,
/// ignored, other-files, running, move, download, template, offline, empty, new-identify,
/// loading, task-error, companion-check, remote-check, generate, browser, preferences.
pub fn snapshot_scene(scene: &str) -> Result<(), String> {
    let scan = fixtures::fixture();
    crate::ai_ui::TEST_STATUS.with(|status| {
        *status.borrow_mut() = Some(crate::ai_ui::EngineStatus {
            checked: true,
            connected: !matches!(scene, "offline" | "loading"),
            host: "127.0.0.1:8189".into(),
            comfyui_version: "0.43.0".into(),
            gpu: Some(li_ai::setup::Gpu { name: "NVIDIA GeForce RTX 5090".into(), vram_total: 33_731_248_128, vram_used: None, utilization: None }),
            presets: if matches!(scene, "empty" | "offline" | "loading") {
                Default::default()
            } else {
                li_ai::catalog::preset(li_ai::catalog::ModelId::Qwen, "bf16").map(|p| (p.id(), Ok(()))).into_iter().collect()
            },
            setup: li_ai::modelsetup::summary::summarise(Some(&scan.server), &[], Some("0.43.0")),
            ..Default::default()
        })
    });
    if scene == "generate" {
        let mut installed = Vec::new();
        let mut templates = std::collections::BTreeMap::new();
        for key in [FLUX3, KREA, TURBO] {
            let m = &scan.server.models[key];
            let arch = m.identity.as_ref().and_then(|id| id.arch.as_deref()).ok_or("fixture model has an architecture")?;
            let mut family = li_ai::family::registry().family(arch).cloned().ok_or("fixture architecture has a family")?;
            family.kind = li_ai::family::FamilyKind::Image;
            family.capabilities.create = true;
            installed.push(li_ai::catalog::InstalledModel {
                key: key.into(),
                family: arch.into(),
                label: m.label.clone(),
                files: [(li_ai::family::Role::Unet, m.file.clone())].into(),
                ..Default::default()
            });
            templates.insert(
                key.into(),
                li_ai::catalog::TemplateModel {
                    recipe: Arc::new(li_ai::template::TemplateRecipe { create: Some(Default::default()), ..Default::default() }),
                    family,
                    label: m.label.clone(),
                },
            );
        }
        li_ai::catalog::set_installed(installed);
        li_ai::catalog::set_templates(templates);
        crate::ai_ui::TEST_STATUS.with(|status| {
            if let Some(status) = status.borrow_mut().as_mut() {
                status.presets = li_ai::catalog::presets().iter().map(|p| (p.id(), Ok(()))).collect();
            }
        });
    }
    if matches!(scene, "browser" | "preferences" | "generate") {
        with(|s| *s = State::default());
        if scene == "browser" {
            crate::model_browser::snapshot_scene();
        }
        return Ok(());
    }
    let (key, step) = match scene {
        "ready-identify" | "new-identify" | "other-files" | "builtin-identify" => (if scene == "builtin-identify" { QWEN21 } else { FLUX3 }, Step::Identify),
        "ignored" => ("unet:old_test.safetensors", Step::Identify),
        "ready-check" | "companion-check" => (FLUX3, Step::Check),
        "ready-verify" | "running" => (FLUX3, Step::Verify),
        "ready-use" => (FLUX3, Step::Use),
        "blocked-check" | "remote-check" | "task-error" | "move" | "download" => (MOSS, Step::Check),
        "verified-verify" | "partial-verify" => (KREA, Step::Verify),
        "failed-verify" => (TURBO, Step::Verify),
        "needs-reverify-check" => (KLEIN9B, Step::Check),
        "missing-node-check" => (HIDREAM, Step::Check),
        "unmatched-check" | "template" => (MING, Step::Check),
        "offline" | "empty" | "loading" => ("", Step::Identify),
        _ => return Err(format!("Unknown model setup scene: {scene}")),
    };
    with(|s| -> Result<(), String> {
        *s = State { open: true, snapshot_only: true, ..Default::default() };
        let mut scan = fixtures::fixture();
        if scene == "companion-check" {
            let model = scan.server.models.get_mut(FLUX3).ok_or("fixture model is present")?;
            for (role, file) in [("style_model", "reference_style.safetensors"), ("controlnet", "edge_control.safetensors")] {
                model.bindings.push(li_ai::companions::Binding { role: role.into(), declared: file.into(), used: Some(file.into()), ..Default::default() });
            }
        }
        if scene == "remote-check" {
            let snapshot = scan.snapshot.as_mut().ok_or("fixture server is present")?;
            snapshot.remote = true;
            snapshot.host = "192.0.2.10:8189".into();
        }
        if scene == "partial-verify" {
            let m = scan.server.models.get_mut(KREA).ok_or("fixture model is present")?;
            m.status = Status::PartiallyVerified;
            m.verified.as_mut().ok_or("fixture result is present")?.capabilities.insert(
                "style".into(),
                CapabilityResult {
                    pass: false,
                    failure: Some("VerifyFailed:error".into()),
                    message: Some("Style reference test failed. Check the LoRA in Set Up Models.".into()),
                    ..Default::default()
                },
            );
        }
        if matches!(scene, "offline" | "empty") {
            scan = Scan::default();
            if scene == "empty" {
                scan.snapshot = Some(fixtures::snapshot("0.43.0"));
            }
        }
        s.set_scan(scan);
        s.selected = Some(key.into());
        s.step = step;
        if scene == "new-identify" {
            let mut scan = fixtures::fixture();
            let model = scan.server.models.get_mut(FLUX3).ok_or("fixture model is present")?;
            model.status = Status::New;
            model.identity = None;
            model.engine = "unsupported".into();
            model.capabilities.clear();
            model.issues.clear();
            s.set_scan(scan);
        }
        if scene == "loading" {
            s.scan = None;
            s.selected = None;
            s.task = Some(Arc::new(Mutex::new(Task { label: "Checking models…".into(), ..Default::default() })));
        }
        if scene == "task-error" {
            s.message = Some(("The download did not finish. Check the connection and try again.".into(), true));
        }
        s.show_ignored = scene == "ignored";
        s.show_other = scene == "other-files";
        match scene {
            "move" => s.confirm_move = Some(MoveAsk { file: "flux2-vae.safetensors".into(), to_folder: "vae".into(), kind: "VAE".into() }),
            "template" => s.choose_template = Some(ChooseTemplate { key: MING.into(), ..Default::default() }),
            "download" => {
                s.confirm_download = Some(li_ai::browser::Plan {
                    files: vec![li_ai::browser::PlannedFile {
                        name: "qwen3.5_9b_bf16.safetensors".into(),
                        folder: "text_encoders".into(),
                        url: "https://huggingface.co/Comfy-Org/MOSS-Image/resolve/main/text_encoders/qwen3.5_9b_bf16.safetensors".into(),
                        bytes: 19_306_312_328,
                        sha256: "a".repeat(64),
                    }],
                    license: "Apache-2.0".into(),
                    ..Default::default()
                })
            }
            "running" => {
                s.task = Some(Arc::new(Mutex::new(Task {
                    label: "Testing…".into(),
                    verifying: vec![FLUX3.into()],
                    current: Some(FLUX3.into()),
                    steps: vec![(FLUX3.into(), "create".into(), CapabilityResult { pass: true, seconds: 9.8, cold: true, ..Default::default() })],
                    ..Default::default()
                })))
            }
            _ => {}
        }
        Ok(())
    })
}
