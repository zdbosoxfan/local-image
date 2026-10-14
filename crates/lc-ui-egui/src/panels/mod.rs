//! Window regions and panels.

pub mod attributions;
pub mod bottombar;
pub mod chips;
pub mod compare;
pub mod crop_overlay;
pub mod detail;
pub mod develop_tools;
pub mod dialogs;
pub mod doc_layers;
pub mod edit;
pub mod enhance;
pub mod export_dialog;
pub mod filterbar;
pub mod grid;
pub mod host_composite;
pub mod host_session;
pub mod layer_bar;
pub mod left;
pub mod library_problem;
pub mod masking;
pub mod notices;
pub mod people;
pub mod people_workspace;
pub mod presets;
pub mod process;
pub mod profiles;
pub mod right;
pub mod rules_editor;
pub mod second;
pub mod settings;
pub mod strip;
pub mod tool_tips;
pub mod topbar;

use egui::{Align2, Rect, vec2};

use crate::LightcraftApp;
use crate::theme::Tokens;

/// Show a side panel the user resizes by dragging its inner edge. `width` (in [`crate::UiState`],
/// so it survives navigation and restarts) is the source of truth: egui's own remembered size is
/// dropped every frame, and a drag of the edge returns the new width to store. The panel never
/// takes more than `available − reserve − MIN_PHOTO_WIDTH` (its minimum permitting), so the photo
/// stays usable; a narrower window shrinks it without forgetting the chosen width.
pub fn resizable_side(
    ui: &mut egui::Ui,
    left: bool,
    id: &str,
    frame: egui::Frame,
    width: f32,
    limits: crate::state::PanelWidth,
    reserve: f32,
    add: impl FnOnce(&mut egui::Ui),
) -> Option<f32> {
    let pid = egui::Id::new(id);
    ui.ctx().data_mut(|d| d.remove::<egui::containers::panel::PanelState>(pid));
    let avail = ui.available_rect_before_wrap();
    let max = (avail.width() - reserve - crate::state::MIN_PHOTO_WIDTH).clamp(limits.min, limits.max);
    let panel = if left { egui::Panel::left(pid) } else { egui::Panel::right(pid) };
    let r = panel.frame(frame).resizable(true).size_range(limits.min..=max).default_size(limits.clamp(width).min(max)).show(ui, add);
    crate::widgets::register(ui.ctx(), format!("panel:{id}"), r.response.rect);
    // egui names the edge's drag widget `<panel id>.with("__resize")`; the width is measured from
    // the panel's fixed edge to the pointer (contents wider than the panel don't count)
    if !ui.ctx().is_being_dragged(pid.with("__resize")) {
        return None;
    }
    let x = ui.ctx().pointer_interact_pos()?.x;
    let w = if left { x - avail.left() } else { avail.right() - x };
    Some(w.round().clamp(limits.min, max))
}

/// The shared HUD and its persistent session History action, overlaid on the canvas.
pub fn toast(app: &mut LightcraftApp, ctx: &egui::Context) {
    let now = ctx.input(|i| i.time);
    app.ui.toast_history.observe(app.ui.toast.as_ref());
    if app.ui.toast.as_ref().is_some_and(|(_, until)| now > *until) {
        app.ui.toast = None;
    }
    let labels = li_ui_kit::notices::Labels {
        profile: li_ui_kit::widgets::Profile::Library,
        history: crate::i18n::tr("Details / History"),
        title: crate::i18n::tr("Operation details and history"),
        copy: crate::i18n::tr("Copy details"),
        dismiss: crate::i18n::tr("Dismiss"),
        error: crate::i18n::tr("Error"),
        warnings: crate::i18n::tr("warnings"),
    };
    if !app.ui.toast_history.history.is_empty() {
        let canvas = app.canvas_rect.unwrap_or_else(|| ctx.content_rect());
        let dismissed = li_ui_kit::notices::toast_overlay(
            ctx,
            canvas,
            "library",
            app.ui.toast.as_ref().map(|(text, _)| text.as_str()),
            &mut app.ui.toast_history.open,
            &labels,
        );
        if dismissed {
            app.ui.toast = None;
        }
        for (name, old) in [("history", "toast:history"), ("dismiss", "toast:dismiss"), ("text", "toast:text"), ("body", "toast:body")] {
            if let Some(rect) = li_ui_kit::notices::control_rect(ctx, "library", name) {
                crate::widgets::register(ctx, old, rect);
            }
        }
    }
    let omitted = (app.ui.toast_history.omitted > 0)
        .then(|| format!("Showing recent results; {} older notices were removed from this session history.", app.ui.toast_history.omitted));
    li_ui_kit::notices::history_window(
        ctx,
        "library",
        &app.ui.toast_history.history,
        omitted.as_deref(),
        &mut app.ui.toast_history.open,
        &labels,
        |_| false,
    );
    if app.ui.toast_history.open {
        for notice in &app.ui.toast_history.history {
            if let Some(rect) = li_ui_kit::notices::control_rect(ctx, "library", &format!("copy:{}", notice.id)) {
                crate::widgets::register(ctx, format!("toast:copy:{}", notice.id), rect);
            }
        }
    }
    if let Some((_, until)) = app.ui.toast.as_ref() {
        ctx.request_repaint_after(std::time::Duration::from_secs_f64((until - now).max(0.0)));
    }
}

/// Paint a centred, dimmed message (empty states).
pub fn empty_message(ui: &egui::Ui, rect: Rect, title: &str, body: &str) {
    let t = Tokens::get(ui.ctx());
    let p = ui.painter();
    p.text(rect.center() - vec2(0.0, 12.0), Align2::CENTER_CENTER, crate::i18n::tr(title), t.semibold(18.0), t.text_label);
    p.text(rect.center() + vec2(0.0, 14.0), Align2::CENTER_CENTER, crate::i18n::tr(body), t.font(13.0), t.text_dim);
}

pub mod smart_sort;

#[cfg(test)]
mod toast_tests {
    use super::*;
    use crate::headless::HeadlessView;
    use std::collections::HashMap;

    #[test]
    fn j18_shared_notice_library_expired_history_is_clickable_in_both_themes_and_widths() {
        fn frame(view: &mut HeadlessView, app: &mut LightcraftApp, size: egui::Vec2, time: f64, events: Vec<egui::Event>) {
            view.run(HeadlessView::raw_input(size, 1.0, time, events), |ui| {
                app.canvas_rect = Some(ui.max_rect());
                toast(app, ui.ctx());
                let copied = ui.ctx().output(|output| {
                    output.commands.iter().find_map(
                        |command| {
                            if let egui::OutputCommand::CopyText(text) = command { Some(text.clone()) } else { None }
                        },
                    )
                });
                if let Some(text) = copied {
                    ui.ctx().data_mut(|data| data.insert_temp(egui::Id::new("j18-test-toast-copy"), text));
                }
            });
        }
        fn click(view: &mut HeadlessView, app: &mut LightcraftApp, size: egui::Vec2, time: f64, at: egui::Pos2) {
            frame(view, app, size, time, vec![egui::Event::PointerMoved(at)]);
            for pressed in [true, false] {
                frame(
                    view,
                    app,
                    size,
                    time,
                    vec![egui::Event::PointerButton { pos: at, button: egui::PointerButton::Primary, pressed, modifiers: egui::Modifiers::NONE }],
                );
            }
            frame(view, app, size, time, vec![]);
        }
        for light in [false, true] {
            for width in [250.0, 400.0] {
                let mut app = LightcraftApp::new(lightcraft_engine::Session::new(), Default::default());
                let mut view = HeadlessView::new();
                let palette =
                    crate::theme::host_palette(if light { li_ui_kit::theme::ThemeKind::StudioLight } else { li_ui_kit::theme::ThemeKind::Pro });
                crate::theme::apply_tokens(&view.ctx, palette);
                // Deterministic snapshots include fully visible Areas and windows.
                view.ctx.global_style_mut(|style| style.animation_time = 0.0);
                let size = egui::vec2(width, 480.0);
                app.toast(&view.ctx, "Import warning: unsupported metadata was retained in operation details for review.");
                app.toast_error(&view.ctx, "Save failed: full diagnostic details remain available after the HUD expires.");
                for _ in 0..4 {
                    frame(&mut view, &mut app, size, 0.0, vec![]);
                }
                let hud = li_ui_kit::notices::control_rect(&view.ctx, "library", "text").unwrap();
                let body = li_ui_kit::notices::control_rect(&view.ctx, "library", "body").unwrap();
                assert!(body.contains_rect(hud), "legacy toast:body remains the full HUD frame");
                assert!(egui::Rect::from_min_size(egui::Pos2::ZERO, size).contains_rect(hud), "HUD wraps within the viewport: {hud:?}");
                let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/notices-ux");
                std::fs::create_dir_all(&dir).unwrap();
                let save = |view: &HeadlessView, name: &str| {
                    let image = view.paint(&HashMap::new());
                    let out = lightcraft_raster::Rgba8::from_fn(image.size[0], image.size[1], |x, y| image.pixels[y * image.size[0] + x].to_array());
                    let bytes = lightcraft_codecs::encode_png(&lightcraft_codecs::EncodeImage::rgba8(&out), &Default::default()).unwrap();
                    std::fs::write(dir.join(format!("library-{name}-{}-{}.png", if light { "light" } else { "dark" }, width as u32)), bytes).unwrap();
                };
                save(&view, "hud");
                for _ in 0..4 {
                    frame(&mut view, &mut app, size, 7.0, vec![]);
                }
                assert!(app.ui.toast.is_none());
                assert!(li_ui_kit::notices::control_rect(&view.ctx, "library", "body").is_none(), "expired HUD has no stale body bounds");
                assert_eq!(app.ui.toast_history.history.len(), 2, "replacement and expiry both preserve diagnostics");
                let history = li_ui_kit::notices::control_rect(&view.ctx, "library", "history").unwrap();
                assert!(egui::Rect::from_min_size(egui::Pos2::ZERO, size).contains_rect(history));
                click(&mut view, &mut app, size, 7.0, history.center());
                assert!(app.ui.toast_history.open, "real History click opens shared details");
                for _ in 0..4 {
                    frame(&mut view, &mut app, size, 7.0, vec![]);
                }
                let id = app.ui.toast_history.history.last().unwrap().id;
                let copy = li_ui_kit::notices::control_rect(&view.ctx, "library", &format!("copy:{id}")).unwrap();
                assert!(copy.width() > 0.0 && copy.height() > 0.0);
                click(&mut view, &mut app, size, 7.0, copy.center());
                let copied = view
                    .ctx
                    .data(|data| data.get_temp::<String>(egui::Id::new("j18-test-toast-copy")))
                    .expect("actual Copy details click emits a clipboard command");
                assert!(copied.contains("Save failed: full diagnostic details"));
                save(&view, "history");
                assert!(app.ui.toast_history.history[1].error);
            }
        }
    }
}
