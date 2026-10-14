//! Non-blocking notices: a reserved compact row and an explicit history window for things
//! the user should see but that must not interrupt work (import/export warnings such as "adjustment
//! flattened", files that couldn't be opened). Notices live in [`UiState`](crate::UiState) so the
//! control channel can read them (`ui.inspect`) and clear them (`ui.set`).

use crate::PhotocraftApp;
use li_ui_kit::widget_style as metrics;

const WAYLAND_FILE_DROP_DISMISSED: &str = "ui.waylandFileDropGuidanceDismissed";
pub use li_ui_kit::notices::{MAX_NOTICES, Notice};

/// Show a notice (newest last); returns its id.
pub fn post(app: &mut PhotocraftApp, title: impl Into<String>, lines: Vec<String>, error: bool, dismiss_pref: Option<&str>) -> u64 {
    let id = app.ui.alloc_id();
    let notice = Notice { id, title: title.into(), lines, error, warning_count: 0, dismiss_pref: dismiss_pref.map(str::to_owned), status_only: false };
    app.ui.notices.push(notice.clone());
    remember(app, notice);
    cap_notices(app);
    id
}

/// Retain a command failure in the existing details log without moving canvas or docks.
pub(crate) fn command_error(app: &mut PhotocraftApp, title: String, detail: String) {
    let notice = Notice { id: app.ui.alloc_id(), title, lines: vec![detail], error: true, status_only: true, ..Default::default() };
    remember(app, notice);
}

/// No file-result row is needed for command-only history; its button lives in the status bar.
pub(crate) fn status_history_only(app: &PhotocraftApp) -> bool {
    app.ui.notices.is_empty() && !app.ui.notice_history.is_empty() && app.ui.notice_history.iter().all(|notice| notice.status_only)
}

/// Compact Details / History action for the status bar, using the shared icon widget.
pub(crate) fn history_button(app: &mut PhotocraftApp, ui: &mut egui::Ui) {
    let label = tl!("Details / History");
    let response = crate::widgets::icon_action(ui, "info", ui.spacing().interact_size.y, label, label, None);
    record(ui, "details", &response);
    if response.clicked() {
        app.ui.notice_history_open = true;
    }
}

fn remember(app: &mut PhotocraftApp, notice: Notice) {
    li_ui_kit::notices::remember(&mut app.ui.notice_history, &app.ui.notices, &mut app.ui.notice_history_omitted, notice);
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
    li_ui_kit::notices::complete(&mut notice, title, detail, error);
    let id = notice.id;
    app.ui.notices.retain(|entry| entry.id != id);
    app.ui.notices.push(notice.clone());
    remember(app, notice);
    cap_notices(app);
    id
}

fn cap_notices(app: &mut PhotocraftApp) {
    li_ui_kit::notices::cap_active(&mut app.ui.notices);
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
    let title = crate::command_feedback::message(&message);
    app.ui.status = title.clone();
    app.ui.status_error = true;
    post(app, title, vec![message], true, None);
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
    if status_history_only(app) {
        ui.ctx().data_mut(|data| data.remove::<egui::Rect>(egui::Id::new(("notice-control", "dismiss"))));
        return false;
    }
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
    let button_height = if t.pro { metrics::PRO_BUTTON_HEIGHT } else { metrics::EDITOR_BUTTON_HEIGHT };
    let row_height = metrics::NOTICE_ROW_HEIGHT.max(button_height + metrics::NOTICE_ROW_MARGIN.sum().y);
    egui::Panel::bottom("notice-results").exact_size(row_height).frame(egui::Frame::NONE.fill(t.card).inner_margin(metrics::NOTICE_ROW_MARGIN)).show(
        ui,
        |ui| {
            ui.horizontal_centered(|ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if let Some(notice) = &latest {
                        let response = crate::widgets::secondary_button(ui, tl!("Dismiss"), 0.0);
                        record(ui, "dismiss", &response);
                        if response.clicked() {
                            dismissed = Some(notice.id);
                        }
                    }
                    let response = crate::widgets::secondary_button(ui, tl!("Details / History"), 0.0);
                    record(ui, "details", &response);
                    if response.clicked() {
                        app.ui.notice_history_open = true;
                    }
                    let text = latest
                        .as_ref()
                        .map(
                            |notice| {
                                if notice.warning_count > 0 { format!("{} · {} warnings", notice.title, notice.warning_count) } else { notice.title.clone() }
                            },
                        )
                        .unwrap_or_else(|| tl!("No new notices").to_owned());
                    let text = if errors > 0 && latest.as_ref().is_some_and(|notice| !notice.error) { format!("{errors} errors · {text}") } else { text };
                    let color = if errors > 0 || latest.as_ref().is_some_and(|notice| notice.warning_count > 0) { t.warning } else { t.text };
                    ui.add(egui::Label::new(egui::RichText::new(text).color(color)).truncate())
                        .on_hover_text(latest.as_ref().map(|n| n.title.as_str()).unwrap_or(""));
                });
            });
        },
    );
    if let Some(id) = dismissed {
        dismiss(app, id);
    }
    dismissed.is_some()
}

/// Full details appear only when requested, in a bounded, scrollable history window.
pub fn show(app: &mut PhotocraftApp, ctx: &egui::Context) {
    let omitted = (app.ui.notice_history_omitted > 0)
        .then(|| format!("Showing recent results; {} older notices were removed from this session history.", app.ui.notice_history_omitted));
    let labels = li_ui_kit::notices::Labels {
        profile: li_ui_kit::widgets::Profile::Editor,
        history: tl!("Details / History"),
        title: tl!("Operation details and history"),
        copy: tl!("Copy details"),
        dismiss: tl!("Dismiss notice"),
        error: tl!("Error"),
        warnings: tl!("warnings"),
    };
    let mut open = app.ui.notice_history_open;
    let dismissed = li_ui_kit::notices::history_window(ctx, "editor", &app.ui.notice_history, omitted.as_deref(), &mut open, &labels, |notice| {
        app.ui.notices.iter().any(|active| active.id == notice.id)
            || notice.dismiss_pref.as_ref().is_some_and(|key| app.session.prefs().dialogs.get(key).and_then(serde_json::Value::as_bool) != Some(true))
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

    /// The shared CPU kittest renderer (`li-testkit`): no GPU or window.
    fn cpu_renderer() -> li_testkit::SoftpaintRenderer {
        li_testkit::SoftpaintRenderer::with_clear_from(|ctx: &egui::Context| crate::theme::Tokens::get(ctx).canvas)
    }

    #[test]
    fn j18_editor_notice_shared_history_clicks_keep_file_warnings() {
        use egui_kittest::{Harness, kittest::Queryable};
        for kind in [crate::theme::ThemeKind::Pro, crate::theme::ThemeKind::StudioLight] {
            for width in [760.0, 1440.0] {
                let mut app = PhotocraftApp::new(Session::new(), Services::default());
                let warnings = vec!["Encoder warning: adjustment flattened".into()];
                let id = file_result(&mut app, "Saved", "/test/result.psd", &warnings);
                complete_file_result(&mut app, Some(id), "Saved result.psd".into(), "Added to Library".into(), false);
                let mut initialized = false;
                let mut h = Harness::builder().with_size(egui::vec2(width, 480.0)).with_max_steps(64).renderer(cpu_renderer()).build_ui_state(
                    move |ui, app: &mut PhotocraftApp| {
                        if !initialized {
                            PhotocraftApp::setup_context(ui.ctx(), Default::default());
                            crate::theme::apply(ui.ctx(), kind);
                            initialized = true;
                            ui.ctx().request_repaint();
                            return;
                        }
                        panel(app, ui);
                        show(app, ui.ctx());
                    },
                    app,
                );
                h.run_steps(4);
                let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/notices-ux");
                std::fs::create_dir_all(&dir).unwrap();
                let theme = if kind == crate::theme::ThemeKind::StudioLight { "light" } else { "dark" };
                h.render().unwrap().save(dir.join(format!("editor-row-{theme}-{}.png", width as u32))).unwrap();
                h.get_by_label("Details / History").click();
                h.run_steps(4);
                assert!(h.state().ui.notice_history_open);
                h.render().unwrap().save(dir.join(format!("editor-history-{theme}-{}.png", width as u32))).unwrap();
                h.get_by_label("Copy details").click();
                let mut copied = None;
                for _ in 0..4 {
                    h.run_steps(1);
                    for command in &h.output().platform_output.commands {
                        if let egui::OutputCommand::CopyText(text) = command {
                            copied = Some(text.clone());
                        }
                    }
                }
                let copied = copied.expect("actual Copy details click emits a clipboard command");
                assert!(copied.contains("Encoder warning: adjustment flattened"));
                assert!(copied.contains("Added to Library"));
                assert_eq!(h.state().ui.notice_history[0].warning_count, 1);
                assert_eq!(h.state().ui.notice_history[0].lines, ["/test/result.psd", "Encoder warning: adjustment flattened", "Added to Library"]);
                h.state_mut().ui.notice_history_open = false;
                h.run_steps(2);
                h.get_by_label("Dismiss").click();
                h.run_steps(2);
                assert!(h.state().ui.notices.is_empty());
                assert_eq!(h.state().ui.notice_history[0].id, id);
            }
        }
    }

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
            let mut h = Harness::builder().with_size(size).with_max_steps(64).renderer(cpu_renderer()).build_eframe(move |cc| {
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
