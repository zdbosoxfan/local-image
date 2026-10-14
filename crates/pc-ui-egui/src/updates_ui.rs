//! Shared, read-only update check window and once-per-launch opt-in scheduling.
use std::sync::mpsc::{Receiver, TryRecvError};

use li_ai::updates::Update;

use crate::PhotocraftApp;

type Check = fn(&str) -> anyhow::Result<Option<Update>>;

pub struct State {
    startup_done: bool,
    notify: bool,
    pub open: bool,
    pending: Option<Receiver<Result<Option<Update>, String>>>,
    result: Option<Result<Option<Update>, String>>,
    check: Check,
}
impl Default for State {
    fn default() -> Self {
        Self { startup_done: false, notify: false, open: false, pending: None, result: None, check: li_ai::updates::check }
    }
}

/// Explicit requests reopen the current check rather than spawning duplicate requests.
pub fn start(app: &mut PhotocraftApp, ctx: &egui::Context, explicit: bool) {
    app.updates.open |= explicit;
    if explicit {
        app.updates.notify = false;
    }
    if app.updates.pending.is_some() {
        return;
    }
    app.updates.notify = !explicit;
    app.updates.result = None;
    let (tx, rx) = std::sync::mpsc::channel();
    app.updates.pending = Some(rx);
    let check = app.updates.check;
    let repaint = ctx.clone();
    #[cfg(not(target_arch = "wasm32"))]
    {
        if let Err(error) = std::thread::Builder::new().name("release-check".into()).spawn(move || {
            let result = check(env!("CARGO_PKG_VERSION")).map_err(|e| format!("{e:#}"));
            let _ = tx.send(result);
            repaint.request_repaint();
        }) {
            app.updates.pending = None;
            app.updates.result = Some(Err(error.to_string()));
        }
    }
    #[cfg(target_arch = "wasm32")]
    {
        let _ = (check, repaint);
        let _ = tx.send(Err(tl!("Update checks require the desktop app").into()));
    }
}

/// Called in every module after settings are loaded; silent startup failures do not interrupt work.
pub fn tick(app: &mut PhotocraftApp, ctx: &egui::Context) {
    if !app.updates.startup_done {
        app.updates.startup_done = true;
        if app.session.prefs().general.check_for_updates_at_startup {
            start(app, ctx, false);
        }
    }
    let received = app.updates.pending.as_ref().map(Receiver::try_recv);
    let result = match received {
        Some(Ok(result)) => Some(result),
        Some(Err(TryRecvError::Disconnected)) => Some(Err(tl!("Update check stopped unexpectedly.").into())),
        _ => None,
    };
    if let Some(result) = result {
        app.updates.open |= app.updates.notify && matches!(&result, Ok(Some(_)));
        app.updates.result = Some(result);
        app.updates.pending = None;
    }
}

pub fn show(app: &mut PhotocraftApp, ctx: &egui::Context) {
    if !app.updates.open {
        return;
    }
    if ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {
        app.updates.open = false;
        return;
    }
    let mut open = true;
    let mut retry = false;
    let mut visit = None;
    egui::Window::new(tl!("Check for Updates…")).collapsible(false).resizable(false).open(&mut open).show(ctx, |ui| {
        let t = crate::theme::Tokens::get(ctx);
        if app.updates.pending.is_some() {
            ui.label(tl!("Checking GitHub releases…"));
        } else {
            match &app.updates.result {
                Some(Ok(Some(update))) => {
                    ui.label(tl!("A newer release is available:"));
                    ui.label(egui::RichText::new(&update.version).color(t.text));
                    if crate::widgets::primary_button(ui, tl!("Open Release Page"), 0.0).clicked() {
                        visit = Some(update.url.clone());
                    }
                }
                Some(Ok(None)) => {
                    ui.label(tl!("No newer release is available for this build."));
                }
                Some(Err(error)) => {
                    ui.label(tl!("Could not check for updates."));
                    ui.label(egui::RichText::new(error).color(t.text_dim));
                    retry = crate::widgets::secondary_button(ui, tl!("Retry"), 0.0).clicked();
                }
                None => {}
            }
        }
        ui.label(egui::RichText::new(tl!("Opens a release page. Nothing is downloaded or installed.")).color(t.text_dim));
        if crate::widgets::dialog_buttons(ui, &[crate::widgets::DialogButton::new(crate::widgets::ButtonRole::Cancel, tl!("Close"), 0.0)])
            == Some(crate::widgets::ButtonRole::Cancel)
        {
            app.updates.open = false;
        }
    });
    app.updates.open &= open;
    if retry {
        start(app, ctx, true);
    }
    if let Some(url) = visit {
        crate::links::open(app, ctx, &url);
    }
}

#[cfg(test)]
fn test_fonts_ready(ui: &egui::Ui) -> bool {
    // Harness construction draws one frame before callers can install the app's font stack.
    // Match the existing screenshot fixtures: draw controls only once Inter medium is active.
    if ui.ctx().fonts(|f| f.families().contains(&egui::FontFamily::Name("medium".into()))) {
        true
    } else {
        PhotocraftApp::setup_context(ui.ctx(), crate::theme::ThemeKind::Pro);
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui_kittest::{Harness, kittest::Queryable};
    fn app() -> PhotocraftApp {
        let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
        app.updates.check = |_| Ok(None);
        app
    }
    #[test]
    fn release_update_startup_is_off_and_legacy_preferences_still_load() {
        let mut app = app();
        app.session.load_prefs_json(r#"{"general":{"zoomWithScrollWheel":true}}"#).unwrap();
        let ctx = egui::Context::default();
        tick(&mut app, &ctx);
        assert!(app.session.prefs().general.zoom_with_scroll_wheel);
        assert!(!app.session.prefs().general.check_for_updates_at_startup);
        assert!(app.updates.pending.is_none());
        app.session.edit_prefs(|p| p.general.check_for_updates_at_startup = true);
        tick(&mut app, &ctx);
        assert!(app.updates.pending.is_none(), "preference changes apply next launch");
        app.updates.startup_done = false;
        tick(&mut app, &ctx);
        assert!(app.updates.pending.is_some() || app.updates.result.is_some());
        assert!(!app.updates.open, "up-to-date startup checks stay silent");
    }
    #[test]
    fn release_update_menu_and_release_link_are_clicked_in_both_themes() {
        use std::sync::{Arc, Mutex};
        for (theme, scale) in [
            (crate::theme::ThemeKind::Pro, 1.0),
            (crate::theme::ThemeKind::Pro, 1.5),
            (crate::theme::ThemeKind::StudioLight, 1.0),
            (crate::theme::ThemeKind::StudioLight, 1.5),
        ] {
            let opened = Arc::new(Mutex::new(Vec::new()));
            let captured = opened.clone();
            let mut app = app();
            app.services.open_url = Some(Box::new(move |url| {
                captured.lock().unwrap().push(url.to_string());
                Ok(())
            }));
            let mut h = Harness::builder().with_size(egui::vec2(800.0, 600.0)).with_pixels_per_point(scale).build_ui_state(
                |ui, app| {
                    crate::menus::menu_bar(app, ui);
                    tick(app, ui.ctx());
                    show(app, ui.ctx());
                },
                app,
            );
            PhotocraftApp::setup_context(&h.ctx, theme);
            h.run_steps(3);
            h.get_by_label("Help").click();
            h.run_steps(3);
            h.get_by_label("Check for Updates…").click();
            h.run_steps(3);
            assert!(h.state().updates.open);
            // Inject deterministic release completion; never hit GitHub in UI tests.
            h.state_mut().updates.pending = None;
            h.state_mut().updates.result = Some(Ok(Some(Update {
                version: "v2.0.0-alpha.2".into(),
                url: "https://github.com/zdbosoxfan/local-image/releases/tag/v2.0.0-alpha.2".into(),
            })));
            h.run_steps(3);
            h.get_by_label("Open Release Page").click();
            h.run_steps(3);
            assert_eq!(opened.lock().unwrap().as_slice(), ["https://github.com/zdbosoxfan/local-image/releases/tag/v2.0.0-alpha.2"]);
            h.get_by_label("Close").click();
            h.run_steps(3);
            assert!(!h.state().updates.open);
            h.state_mut().updates.open = true;
            h.run_steps(2);
            h.key_press(egui::Key::Escape);
            h.run_steps(3);
            assert!(!h.state().updates.open);
        }
    }
    #[test]
    fn release_update_preference_checkbox_is_clicked_and_persisted() {
        let mut h = Harness::builder().with_size(egui::vec2(1000.0, 800.0)).build_ui_state(
            |ui, app| {
                crate::dialogs::show(app, ui.ctx());
            },
            app(),
        );
        PhotocraftApp::setup_context(&h.ctx, crate::theme::ThemeKind::Pro);
        crate::prefs_ui::open_preferences(h.state_mut(), "general");
        h.run_steps(4);
        h.get_by_label("Check for updates at startup").click();
        h.run_steps(2);
        h.get_by_label("OK").click();
        h.run_steps(3);
        assert!(h.state().session.prefs().general.check_for_updates_at_startup);
        let saved = h.state().session.prefs_to_json();
        let mut restored = app();
        restored.session.load_prefs_json(&saved).unwrap();
        assert!(restored.session.prefs().general.check_for_updates_at_startup);
    }
    #[test]
    fn release_update_background_completion_respects_silent_errors_and_close() {
        let ctx = egui::Context::default();
        for (notify, result, expected_open) in [
            (true, Err("offline".into()), false),
            (true, Ok(Some(Update { version: "v3.0.0".into(), url: "https://github.com/zdbosoxfan/local-image/releases/tag/v3.0.0".into() })), true),
            (false, Ok(Some(Update { version: "v3.0.0".into(), url: "https://github.com/zdbosoxfan/local-image/releases/tag/v3.0.0".into() })), false),
        ] {
            let mut app = app();
            app.updates.startup_done = true;
            app.updates.notify = notify;
            let (tx, rx) = std::sync::mpsc::channel();
            app.updates.pending = Some(rx);
            tx.send(result).unwrap();
            tick(&mut app, &ctx);
            assert_eq!(app.updates.open, expected_open);
            assert!(app.updates.pending.is_none());
        }
    }

    #[test]
    fn release_update_errors_retry_and_closing_keeps_worker_nonblocking() {
        let mut app = app();
        app.updates.open = true;
        app.updates.result = Some(Err("Offline".into()));
        let mut h = Harness::builder().with_size(egui::vec2(600.0, 400.0)).build_ui_state(
            |ui, app| {
                if test_fonts_ready(ui) {
                    show(app, ui.ctx());
                }
            },
            app,
        );
        PhotocraftApp::setup_context(&h.ctx, crate::theme::ThemeKind::Pro);
        h.run_steps(3);
        h.get_by_label("Retry").click();
        h.run_steps(3);
        assert!(h.state().updates.pending.is_some());
        h.get_by_label("Close").click();
        h.run_steps(3);
        assert!(!h.state().updates.open);
        assert!(h.state().updates.pending.is_some());
    }
}

#[cfg(test)]
#[test]
#[ignore = "offscreen native GPU screenshots; run explicitly with --ignored"]
fn release_update_window_screens() {
    use eframe::egui_wgpu::{WgpuSetup, WgpuSetupCreateNew};
    use eframe::wgpu;
    use egui_kittest::Harness;
    let _gpu_gate = crate::gpu_test_support::lock();
    let renderer = || {
        let mut setup = WgpuSetupCreateNew::without_display_handle();
        setup.instance_descriptor.backends = if cfg!(target_os = "macos") { wgpu::Backends::METAL } else { wgpu::Backends::VULKAN };
        setup.native_adapter_selector = Some(std::sync::Arc::new(|adapters, _| {
            let adapter = adapters
                .iter()
                .find(|a| matches!(a.get_info().device_type, wgpu::DeviceType::DiscreteGpu | wgpu::DeviceType::IntegratedGpu))
                .ok_or("native hardware GPU required for release screenshots")?;
            eprintln!("release screenshot adapter: {}", adapter.get_info().name);
            Ok(adapter.clone())
        }));
        egui_kittest::wgpu::WgpuTestRenderer::from_setup(WgpuSetup::CreateNew(setup))
    };
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/design-system/release");
    std::fs::create_dir_all(&dir).unwrap();
    for (theme, name) in [(crate::theme::ThemeKind::Pro, "dark"), (crate::theme::ThemeKind::StudioLight, "light")] {
        for width in [250.0, 400.0] {
            for scale in [1.0, 1.5] {
                // A fresh native scale/viewport avoids reusing cached renders after a resize.
                let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), Default::default());
                app.updates.open = true;
                app.updates.result = Some(Ok(Some(Update {
                    version: "v2.0.0-alpha.2".into(),
                    url: "https://github.com/zdbosoxfan/local-image/releases/tag/v2.0.0-alpha.2".into(),
                })));
                let mut h = Harness::builder().with_size(egui::vec2(width, 400.0)).with_pixels_per_point(scale).renderer(renderer()).build_ui_state(
                    |ui, app| {
                        if test_fonts_ready(ui) {
                            show(app, ui.ctx());
                        }
                    },
                    app,
                );
                PhotocraftApp::setup_context(&h.ctx, theme);
                for _ in 0..5 {
                    h.step();
                }
                let image = h.render().unwrap();
                assert_eq!(image.dimensions(), ((width * scale).round() as u32, (400.0 * scale).round() as u32));
                image.save(dir.join(format!("updates-{name}-{}-{scale}.png", width as u32))).unwrap();
            }
        }
    }
}
