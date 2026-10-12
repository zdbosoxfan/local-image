//! Non-blocking notices: a reserved compact row and an explicit history window for things
//! the user should see but that must not interrupt work (import/export warnings such as "adjustment
//! flattened", files that couldn't be opened). Notices live in [`UiState`](crate::UiState) so the
//! control channel can read them (`ui.inspect`) and clear them (`ui.set`).

use serde::{Deserialize, Serialize};

use crate::PhotocraftApp;

const WAYLAND_FILE_DROP_DISMISSED: &str = "ui.waylandFileDropGuidanceDismissed";
/// At most this many notices stay active; older entries remain in session history.
pub const MAX_NOTICES: usize = 3;
/// Keep a bounded session history; disclose truncation rather than silently losing diagnostics.
const MAX_HISTORY: usize = 256;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Notice {
    pub id: u64,
    pub title: String,
    #[serde(default)]
    pub lines: Vec<String>,
    /// An error (warning colour) rather than an informational notice.
    #[serde(default)]
    pub error: bool,
    #[serde(default)]
    pub warning_count: usize,
    /// Preference key to set when this notice is dismissed.
    #[serde(default)]
    pub dismiss_pref: Option<String>,
}

/// Show a notice (newest last); returns its id.
pub fn post(app: &mut PhotocraftApp, title: impl Into<String>, lines: Vec<String>, error: bool, dismiss_pref: Option<&str>) -> u64 {
    let id = app.ui.alloc_id();
    let notice = Notice { id, title: title.into(), lines, error, warning_count: 0, dismiss_pref: dismiss_pref.map(str::to_owned) };
    app.ui.notices.push(notice.clone());
    remember(app, notice);
    cap_notices(app);
    id
}

fn remember(app: &mut PhotocraftApp, notice: Notice) {
    if let Some(previous) = app.ui.notice_history.iter_mut().find(|entry| entry.id == notice.id) {
        *previous = notice;
    } else {
        app.ui.notice_history.push(notice);
    }
    while app.ui.notice_history.len() > MAX_HISTORY {
        // Never evict an actionable current notice; old details remain available until
        // the explicit, displayed recent-history limit is reached.
        let oldest = app.ui.notice_history.iter().position(|entry| !app.ui.notices.iter().any(|active| active.id == entry.id)).unwrap_or(0);
        app.ui.notice_history.remove(oldest);
        app.ui.notice_history_omitted += 1;
    }
}

/// One result for a file operation. The host updates this same id when Library work finishes.
pub fn file_result(app: &mut PhotocraftApp, verb: &str, path: &str, warnings: &[String]) -> u64 {
    let title = format!("{verb} {}", crate::file_open::display_name(path));
    let mut lines = vec![path.to_owned()];
    lines.extend_from_slice(warnings);
    let id = app.ui.alloc_id();
    app.ui.status = if warnings.is_empty() { title.clone() } else { format!("{title} · {} warnings", warnings.len()) };
    let notice = Notice { id, title, lines, warning_count: warnings.len(), ..Default::default() };
    app.ui.notices.push(notice.clone());
    remember(app, notice);
    cap_notices(app);
    if !warnings.is_empty() {
        app.ui.status_error = true;
    }
    id
}

/// Complete an operation without replacing its encoder warnings with a second success card.
pub fn complete_file_result(app: &mut PhotocraftApp, id: Option<u64>, title: String, detail: String, error: bool) -> u64 {
    let mut notice = id
        .and_then(|id| app.ui.notices.iter().chain(app.ui.notice_history.iter()).find(|notice| notice.id == id).cloned())
        .unwrap_or_else(|| Notice { id: app.ui.alloc_id(), ..Default::default() });
    notice.title = title;
    notice.error |= error;
    if !detail.is_empty() && !notice.lines.contains(&detail) {
        notice.lines.push(detail);
    }
    let id = notice.id;
    app.ui.notices.retain(|entry| entry.id != id);
    app.ui.notices.push(notice.clone());
    remember(app, notice);
    cap_notices(app);
    id
}

fn cap_notices(app: &mut PhotocraftApp) {
    while app.ui.notices.len() > MAX_NOTICES {
        let newest = app.ui.notices.last().map(|notice| notice.id);
        let oldest_temporary = app.ui.notices.iter().position(|notice| notice.dismiss_pref.is_none() && Some(notice.id) != newest).unwrap_or(0);
        app.ui.notices.remove(oldest_temporary);
    }
}

/// Show the Wayland-specific fallback guidance unless the user dismissed it in preferences.
pub fn wayland_file_drop_guidance(app: &mut PhotocraftApp) {
    if !app.services.is_wayland
        || app.session.prefs().dialogs.get(WAYLAND_FILE_DROP_DISMISSED).and_then(serde_json::Value::as_bool) == Some(true)
        || app.ui.notices.iter().any(|notice| notice.dismiss_pref.as_deref() == Some(WAYLAND_FILE_DROP_DISMISSED))
    {
        return;
    }
    post(
        app,
        tl!("Native file drag-and-drop is unavailable"),
        vec![
            tl!("Native file drag-and-drop is not supported on Wayland yet. Use File › Open, or run Local Image under XWayland with `WAYLAND_DISPLAY= local-image`.").into(),
        ],
        false,
        Some(WAYLAND_FILE_DROP_DISMISSED),
    );
}

fn dismiss(app: &mut PhotocraftApp, id: u64) {
    let dismiss_pref = app.ui.notices.iter().chain(app.ui.notice_history.iter()).find(|notice| notice.id == id).and_then(|notice| notice.dismiss_pref.clone());
    app.ui.notices.retain(|notice| notice.id != id);
    if let Some(key) = dismiss_pref {
        app.session.prefs.edit(|prefs| {
            prefs.dialogs.insert(key, serde_json::Value::Bool(true));
        });
    }
}

/// Report import/export `warnings` for the file operation `what` (e.g. "Opened a.psd"): the status
/// bar says how many there were, and a notice lists them. Nothing happens when there are none.
pub fn io_warnings(app: &mut PhotocraftApp, what: &str, warnings: &[String]) {
    let Some(first) = warnings.first() else { return };
    let n = warnings.len();
    app.ui.status = if n == 1 { format!("{what}: {first}") } else { format!("{what} with {n} warnings: {first} …") };
    app.ui.status_error = true;
    let id = post(app, what, warnings.to_vec(), false, None);
    for notice in app.ui.notices.iter_mut().chain(app.ui.notice_history.iter_mut()).filter(|notice| notice.id == id) {
        notice.warning_count = n;
    }
}

/// Report a failed file operation: the status bar shows it as an error and a notice keeps it on
/// screen until dismissed.
pub fn error(app: &mut PhotocraftApp, message: String) {
    app.ui.status = message.clone();
    app.ui.status_error = true;
    post(app, message, Vec::new(), true, None);
}

/// Last visible control rectangle, intersected with its actual clip region.
pub fn control_rect(ctx: &egui::Context, name: &str) -> Option<egui::Rect> {
    ctx.data(|data| data.get_temp(egui::Id::new(("notice-control", name))))
}
fn record(ui: &egui::Ui, name: &str, response: &egui::Response) {
    ui.ctx().data_mut(|data| data.insert_temp(egui::Id::new(("notice-control", name)), response.rect.intersect(ui.clip_rect())));
}

/// Reserve one row before canvas/docks are laid out. Returns whether a notice was dismissed.
pub fn panel(app: &mut PhotocraftApp, ui: &mut egui::Ui) -> bool {
    ui.ctx().data_mut(|data| {
        for name in ["details", "dismiss"] {
            data.remove::<egui::Rect>(egui::Id::new(("notice-control", name)));
        }
    });
    if app.ui.notices.is_empty() && app.ui.notice_history.is_empty() {
        return false;
    }
    let latest = app.ui.notices.last().cloned();
    let errors = app.ui.notices.iter().filter(|notice| notice.error).count();
    let t = crate::theme::Tokens::get(ui.ctx());
    let mut dismissed = None;
    egui::Panel::bottom("notice-results").exact_size(32.0).frame(egui::Frame::NONE.fill(t.card).inner_margin(egui::Margin::symmetric(8, 2))).show(ui, |ui| {
        ui.horizontal_centered(|ui| {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if let Some(notice) = &latest {
                    let response = ui.button(tl!("Dismiss"));
                    record(ui, "dismiss", &response);
                    if response.clicked() {
                        dismissed = Some(notice.id);
                    }
                }
                let response = ui.button(tl!("Details / History"));
                record(ui, "details", &response);
                if response.clicked() {
                    app.ui.notice_history_open = true;
                }
                let text = latest
                    .as_ref()
                    .map(|notice| if notice.warning_count > 0 { format!("{} · {} warnings", notice.title, notice.warning_count) } else { notice.title.clone() })
                    .unwrap_or_else(|| tl!("No new notices").to_owned());
                let text = if errors > 0 && latest.as_ref().is_some_and(|notice| !notice.error) { format!("{errors} errors · {text}") } else { text };
                let color = if errors > 0 || latest.as_ref().is_some_and(|notice| notice.warning_count > 0) { t.warning } else { t.text };
                ui.add(egui::Label::new(egui::RichText::new(text).color(color)).truncate())
                    .on_hover_text(latest.as_ref().map(|n| n.title.as_str()).unwrap_or(""));
            });
        });
    });
    if let Some(id) = dismissed {
        dismiss(app, id);
    }
    dismissed.is_some()
}

/// Full details appear only when requested, in a bounded, scrollable history window.
pub fn show(app: &mut PhotocraftApp, ctx: &egui::Context) {
    if !app.ui.notice_history_open {
        return;
    }
    let mut open = true;
    let size = ctx.content_rect().size();
    let mut dismissed = Vec::new();
    egui::Window::new(tl!("Operation details and history"))
        .id(egui::Id::new("notice-history"))
        .open(&mut open)
        .resizable(true)
        .default_width(620.0_f32.min((size.x - 40.0).max(240.0)))
        .max_width((size.x - 40.0).max(240.0))
        .show(ctx, |ui| {
            if app.ui.notice_history_omitted > 0 {
                ui.label(format!("Showing recent results; {} older notices were removed from this session history.", app.ui.notice_history_omitted));
            }
            egui::ScrollArea::vertical().max_height((size.y - 140.0).max(120.0)).show(ui, |ui| {
                for notice in app.ui.notice_history.iter().rev() {
                    ui.push_id(notice.id, |ui| {
                        ui.horizontal_wrapped(|ui| {
                            ui.strong(&notice.title);
                            if notice.error {
                                ui.label(tl!("Error"));
                            }
                            if notice.warning_count > 0 {
                                ui.label(format!("{} warnings", notice.warning_count));
                            }
                            if ui.button(tl!("Copy details")).clicked() {
                                ctx.copy_text(format!("{}\n{}", notice.title, notice.lines.join("\n")));
                            }
                            let dismissible = app.ui.notices.iter().any(|active| active.id == notice.id)
                                || notice
                                    .dismiss_pref
                                    .as_ref()
                                    .is_some_and(|key| app.session.prefs().dialogs.get(key).and_then(serde_json::Value::as_bool) != Some(true));
                            if dismissible && ui.button(tl!("Dismiss notice")).clicked() {
                                dismissed.push(notice.id);
                            }
                        });
                        for line in &notice.lines {
                            ui.add(egui::Label::new(line).wrap());
                        }
                        ui.separator();
                    });
                }
            });
        });
    for id in dismissed {
        dismiss(app, id);
    }
    app.ui.notice_history_open = open;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{PhotocraftApp, Services};
    use photocraft_engine::Session;
    use std::sync::{Arc, Mutex};

    #[test]
    fn wayland_guidance_is_only_shown_in_wayland_sessions() {
        let mut app = PhotocraftApp::new(Session::new(), Services { is_wayland: true, ..Default::default() });
        assert_eq!(app.ui.notices.len(), 1);
        assert_eq!(app.ui.notices[0].title, "Native file drag-and-drop is unavailable");
        let guidance = app.ui.notices[0].lines.join(" ");
        assert!(guidance.contains("not supported on Wayland yet"));
        assert!(guidance.contains("File › Open"));
        assert!(!guidance.contains("Ctrl+V"));
        assert!(guidance.contains("XWayland"));
        assert!(guidance.contains("WAYLAND_DISPLAY= local-image"));
        assert_eq!(app.ui.notices[0].dismiss_pref.as_deref(), Some(WAYLAND_FILE_DROP_DISMISSED));
        for i in 0..MAX_NOTICES {
            post(&mut app, format!("Transient {i}"), Vec::new(), false, None);
        }
        assert_eq!(app.ui.notices.len(), MAX_NOTICES);
        assert!(app.ui.notices.iter().any(|notice| notice.dismiss_pref.is_some()));

        let app = PhotocraftApp::new(Session::new(), Services::default());
        assert!(app.ui.notices.is_empty());
    }

    #[test]
    fn dismissing_wayland_guidance_persists_across_launches() {
        let saved = Arc::new(Mutex::new(None::<String>));
        let load_store = saved.clone();
        let save_store = saved.clone();
        let services = Services {
            is_wayland: true,
            load_prefs: Some(Box::new(move || load_store.lock().unwrap_or_else(|e| e.into_inner()).clone())),
            save_prefs: Some(Box::new(move |text| {
                *save_store.lock().unwrap_or_else(|e| e.into_inner()) = Some(text.to_owned());
                Ok(())
            })),
            ..Default::default()
        };
        let mut app = PhotocraftApp::new(Session::new(), services);
        let id = app.ui.notices[0].id;
        dismiss(&mut app, id);
        crate::prefs_ui::tick(&mut app, &egui::Context::default());

        let services =
            Services { is_wayland: true, load_prefs: Some(Box::new(move || saved.lock().unwrap_or_else(|e| e.into_inner()).clone())), ..Default::default() };
        let app = PhotocraftApp::new(Session::new(), services);
        assert!(app.ui.notices.is_empty());
    }

    #[test]
    fn dismissing_a_notice_writes_its_own_preference_key() {
        let mut app = PhotocraftApp::new(Session::new(), Services::default());
        post(&mut app, "Dismissible", Vec::new(), false, Some("ui.testNoticeDismissed"));
        let id = app.ui.notices[0].id;
        dismiss(&mut app, id);
        assert_eq!(app.session.prefs().dialogs.get("ui.testNoticeDismissed").and_then(serde_json::Value::as_bool), Some(true));
    }
    #[test]
    fn operation_notices_coalesce_keep_warnings_and_retain_dismissed_errors() {
        let mut app = PhotocraftApp::new(Session::new(), Services::default());
        let warnings = vec!["Unsupported adjustment rendered into pixels".into(), "Private field retained only in native format".into()];
        let id = file_result(&mut app, "Saved", "/photos/job/photo-Edit.psd", &warnings);
        assert_eq!(complete_file_result(&mut app, Some(id), "Saved photo-Edit.psd".into(), "Added to Library".into(), false), id);
        assert_eq!(app.ui.notices.len(), 1);
        assert_eq!(app.ui.notice_history.len(), 1);
        assert_eq!(app.ui.notices[0].warning_count, 2);
        assert!(warnings.iter().all(|warning| app.ui.notices[0].lines.contains(warning)));
        for n in 0..6 {
            error(&mut app, format!("Export failed {n}"));
        }
        assert_eq!(app.ui.notices.len(), MAX_NOTICES);
        assert_eq!(app.ui.notice_history.len(), 7);
        assert!(app.ui.notice_history.iter().any(|notice| notice.id == id && notice.lines.contains(&warnings[1])));
        let error = app.ui.notices.last().unwrap().clone();
        dismiss(&mut app, error.id);
        assert!(app.ui.notice_history.contains(&error), "dismissing the row keeps error details");
    }

    #[test]
    fn notice_row_and_details_are_clickable_at_normal_and_narrow_sizes() {
        use egui_kittest::{Harness, kittest::Queryable};
        #[derive(Default)]
        struct CpuRenderer(crate::tool_icon_tests::cpu::TextureStore);
        impl egui_kittest::TestRenderer for CpuRenderer {
            fn handle_delta(&mut self, delta: &mut egui::TexturesDelta) {
                self.0.apply(std::mem::take(delta));
            }
            fn render(&mut self, ctx: &egui::Context, output: &egui::FullOutput) -> Result<image::RgbaImage, String> {
                let scale = output.pixels_per_point;
                let size = ctx.content_rect().size() * scale;
                let primitives = ctx.tessellate(output.shapes.clone(), scale);
                let image =
                    crate::tool_icon_tests::cpu::paint(&primitives, &self.0, [size.x as usize, size.y as usize], scale, crate::theme::Tokens::get(ctx).canvas);
                image::RgbaImage::from_raw(image.size[0] as u32, image.size[1] as u32, image.pixels.iter().flat_map(|p| p.to_array()).collect())
                    .ok_or_else(|| "invalid CPU image".into())
            }
        }
        fn click(h: &mut Harness<'_, PhotocraftApp>, at: egui::Pos2) {
            h.hover_at(at);
            h.run_steps(1);
            for pressed in [true, false] {
                h.event(egui::Event::PointerButton { pos: at, button: egui::PointerButton::Primary, pressed, modifiers: egui::Modifiers::NONE });
                h.run_steps(1);
            }
            h.run_steps(2);
        }
        for size in [egui::vec2(1440.0, 900.0), egui::vec2(760.0, 480.0)] {
            let mut app = PhotocraftApp::new(Session::new(), Services::default());
            app.run("file.new", serde_json::json!({"width":64,"height":48})).unwrap();
            let warnings = (0..40).map(|i| format!("Encoder warning {i}: full details remain available even when the window is narrow.")).collect::<Vec<_>>();
            let id = file_result(&mut app, "Saved", "/a/long/project/path/with/multiple/subdirectories/photo-Edit.psd", &warnings);
            complete_file_result(&mut app, Some(id), "Saved photo-Edit.psd".into(), "Added to Library".into(), false);
            let mut h = Harness::builder().with_size(size).with_max_steps(64).renderer(CpuRenderer::default()).build_eframe(move |cc| {
                PhotocraftApp::setup_context(&cc.egui_ctx, Default::default());
                app
            });
            h.run_steps(6);
            for name in ["details", "dismiss"] {
                let rect = control_rect(&h.ctx, name).unwrap();
                assert!(h.ctx.content_rect().contains_rect(rect));
                assert!(rect.width() > 35.0 && rect.height() >= 18.0, "{name}: {rect:?}");
            }
            let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/notices-ux");
            std::fs::create_dir_all(&dir).unwrap();
            h.render().unwrap().save(dir.join(format!("notice-row-{}.png", size.x as u32))).unwrap();
            let at = control_rect(&h.ctx, "details").unwrap().center();
            click(&mut h, at);
            assert!(h.state().ui.notice_history_open);
            h.get_by_label("Copy details");
            assert_eq!(h.state().ui.notice_history[0].lines.len(), 42, "path, all40 warnings and Library result");
            h.render().unwrap().save(dir.join(format!("notice-details-{}.png", size.x as u32))).unwrap();
            h.state_mut().ui.notice_history_open = false;
            h.run_steps(3);
            let at = control_rect(&h.ctx, "dismiss").unwrap().center();
            click(&mut h, at);
            assert!(h.state().ui.notices.is_empty());
            assert!(control_rect(&h.ctx, "dismiss").is_none(), "hidden controls cannot retain stale click rectangles");
            assert_eq!(h.state().ui.notice_history.len(), 1);
        }
    }
    #[test]
    fn file_notice_with_all_persistent_slots_keeps_full_result_without_panicking() {
        let mut app = PhotocraftApp::new(Session::new(), Services::default());
        for i in 0..MAX_NOTICES {
            post(&mut app, format!("Persistent {i}"), vec![], false, Some(&format!("notice.{i}")));
        }
        let warnings = vec!["Encoder warning".into()];
        let id = file_result(&mut app, "Saved", "/photos/test.psd", &warnings);
        assert_eq!(app.ui.notices.len(), MAX_NOTICES);
        assert!(app.ui.notices.iter().any(|notice| notice.id == id), "new result remains visible even when previous slots were persistent");
        assert_eq!(complete_file_result(&mut app, Some(id), "Saved test.psd".into(), "Added to Library".into(), false), id);
        let saved = app.ui.notice_history.iter().find(|entry| entry.id == id).unwrap();
        assert_eq!(saved.warning_count, 1);
        assert!(saved.lines.contains(&warnings[0]) && saved.lines.iter().any(|line| line == "Added to Library"));
    }
}
