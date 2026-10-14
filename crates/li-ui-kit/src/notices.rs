//! Shared non-blocking operation notices. Both frontends retain complete session details after
//! a visible notice or HUD expires; active notices and history have independently bounded lifetimes.

use crate::widget_style as metrics;
use serde::{Deserialize, Serialize};

/// At most three notices remain active; older entries stay in session history.
pub const MAX_NOTICES: usize = 3;
/// Disclose truncation rather than silently losing older diagnostics.
pub const MAX_HISTORY: usize = 256;

/// Stable wire representation used by the editor's control channel.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Notice {
    pub id: u64,
    pub title: String,
    #[serde(default)]
    pub lines: Vec<String>,
    #[serde(default)]
    /// An error rather than an informational result.
    pub error: bool,
    #[serde(default)]
    /// Status-only results enter history without adding an active layout row.
    pub status_only: bool,
    #[serde(default)]
    pub warning_count: usize,
    #[serde(default)]
    /// Frontend preference key persisted when guidance is dismissed.
    pub dismiss_pref: Option<String>,
}

/// Insert or update one operation while retaining all actionable active notices.
pub fn remember(history: &mut Vec<Notice>, active: &[Notice], omitted: &mut usize, notice: Notice) {
    if let Some(previous) = history.iter_mut().find(|entry| entry.id == notice.id) {
        *previous = notice;
    } else {
        history.push(notice);
    }
    while history.len() > MAX_HISTORY {
        let oldest = history.iter().position(|entry| !active.iter().any(|notice| notice.id == entry.id)).unwrap_or(0);
        history.remove(oldest);
        *omitted += 1;
    }
}

/// Retain persistent guidance when possible, always keeping the newest operation visible.
pub fn cap_active(active: &mut Vec<Notice>) {
    while active.len() > MAX_NOTICES {
        let newest = active.last().map(|notice| notice.id);
        let oldest = active.iter().position(|notice| notice.dismiss_pref.is_none() && Some(notice.id) != newest).unwrap_or(0);
        active.remove(oldest);
    }
}

/// Finish an operation without losing encoder warnings or duplicating its detail lines.
pub fn complete(notice: &mut Notice, title: String, detail: String, error: bool) {
    notice.title = title;
    notice.error |= error;
    if !detail.is_empty() && !notice.lines.contains(&detail) {
        notice.lines.push(detail);
    }
}

/// The Library HUD's notice history. Library code writes the `(text, expiry)` HUD tuple; observing
/// it each frame records those messages here as well.
#[derive(Clone, Debug, Default)]
pub struct ToastHistory {
    pub history: Vec<Notice>,
    pub omitted: usize,
    pub open: bool,
    next_id: u64,
    observed: Option<(String, f64)>,
}

impl ToastHistory {
    /// Capture a new HUD immediately, even if another operation replaces it before painting.
    pub fn post(&mut self, text: String, expiry: f64, error: bool) {
        self.next_id += 1;
        let notice = Notice { id: self.next_id, title: text.clone(), error, status_only: true, ..Default::default() };
        remember(&mut self.history, &[], &mut self.omitted, notice);
        self.observed = Some((text, expiry));
    }

    /// Record a message written straight to the HUD tuple (once per distinct message).
    pub fn observe(&mut self, toast: Option<&(String, f64)>) {
        match toast {
            Some(toast) if self.observed.as_ref() != Some(toast) => self.post(toast.0.clone(), toast.1, false),
            None => self.observed = None,
            _ => {}
        }
    }
}

/// Last visible shared notice control bounds, clipped to the actual viewport.
pub fn control_rect(ctx: &egui::Context, scope: &str, name: &str) -> Option<egui::Rect> {
    ctx.data(|data| data.get_temp(egui::Id::new(("shared-notice-control", scope, name))))
}

fn button(ui: &mut egui::Ui, scope: &str, name: &str, label: &str, profile: crate::widgets::Profile) -> egui::Response {
    let response = match profile {
        crate::widgets::Profile::Editor => crate::widgets::secondary_button(ui, label, 0.0),
        crate::widgets::Profile::Library => crate::widgets::selection(ui, label, false, crate::widgets::SelectionStyle::Text, None),
    };
    ui.ctx().data_mut(|data| data.insert_temp(egui::Id::new(("shared-notice-control", scope, name)), response.rect.intersect(ui.clip_rect())));
    response
}

/// Translated labels supplied by the frontend, keeping its existing translation backend.
pub struct Labels<'a> {
    pub profile: crate::widgets::Profile,
    pub history: &'a str,
    pub title: &'a str,
    pub copy: &'a str,
    pub dismiss: &'a str,
    pub error: &'a str,
    pub warnings: &'a str,
}

/// Accessible HUD plus persistent History action. This is an overlay, so it never moves the photo.
/// Returns whether the current transient message was dismissed.
pub fn toast_overlay(ctx: &egui::Context, canvas: egui::Rect, scope: &str, toast: Option<&str>, open: &mut bool, labels: &Labels<'_>) -> bool {
    ctx.data_mut(|data| {
        for name in ["history", "dismiss", "text", "body"] {
            data.remove::<egui::Rect>(egui::Id::new(("shared-notice-control", scope, name)));
        }
    });
    let t = labels.profile.tokens(ctx);
    let spacing = ctx.global_style().spacing.clone();
    let mut dismissed = false;
    let gap = spacing.item_spacing;
    let width = (canvas.width() - gap.x * 2.0).clamp(spacing.interact_size.x, metrics::NOTICE_HUD_MAX_WIDTH);
    egui::Area::new(egui::Id::new((scope, "notice-hud")))
        .order(egui::Order::Foreground)
        .pivot(egui::Align2::CENTER_BOTTOM)
        .fixed_pos(egui::pos2(canvas.center().x, canvas.bottom() - metrics::NOTICE_HUD_BOTTOM_OFFSET))
        .show(ctx, |ui| {
            ui.set_max_width(width);
            let frame = egui::Frame::new()
                .fill(t.library.overlay_bg)
                .corner_radius(t.radius)
                .inner_margin(egui::Margin::symmetric(gap.x.round() as i8, gap.y.round() as i8))
                .show(ui, |ui| {
                    if let Some(text) = toast {
                        let response = ui.add(egui::Label::new(egui::RichText::new(text).color(t.library.overlay_fg)).wrap());
                        ctx.data_mut(|data| data.insert_temp(egui::Id::new(("shared-notice-control", scope, "text")), response.rect));
                    }
                    ui.horizontal_wrapped(|ui| {
                        if button(ui, scope, "history", labels.history, labels.profile).clicked() {
                            *open = true;
                        }
                        if toast.is_some() && button(ui, scope, "dismiss", labels.dismiss, labels.profile).clicked() {
                            dismissed = true;
                        }
                    });
                });
            if toast.is_some() {
                ctx.data_mut(|data| data.insert_temp(egui::Id::new(("shared-notice-control", scope, "body")), frame.response.rect.intersect(ui.clip_rect())));
            }
        });
    dismissed
}

/// Shared bounded, scrollable details view, used by both frontends. Dismissal preferences remain
/// frontend-owned; returned ids identify the notices whose Dismiss actions were clicked.
pub fn history_window(
    ctx: &egui::Context,
    scope: &str,
    history: &[Notice],
    omitted_text: Option<&str>,
    open: &mut bool,
    labels: &Labels<'_>,
    dismissible: impl Fn(&Notice) -> bool,
) -> Vec<u64> {
    let mut dismissed = Vec::new();
    let was_open = ctx.data_mut(|data| {
        let id = egui::Id::new((scope, "notice-history-was-open"));
        let was_open = data.get_temp::<bool>(id).unwrap_or(false);
        data.insert_temp(id, *open);
        was_open
    });
    if !*open && !was_open {
        return dismissed;
    }
    ctx.data_mut(|data| {
        for notice in history {
            for name in [format!("copy:{}", notice.id), format!("dismiss:{}", notice.id)] {
                data.remove::<egui::Rect>(egui::Id::new(("shared-notice-control", scope, name.as_str())));
            }
        }
    });
    if !*open {
        return dismissed;
    }
    let spacing = ctx.global_style().spacing.clone();
    let size = ctx.content_rect().size();
    let viewport_width = (size.x - spacing.window_margin.sum().x).max(spacing.interact_size.x);
    let width = (size.x - metrics::NOTICE_HISTORY_VIEWPORT_MARGIN).max(metrics::NOTICE_HISTORY_MIN_WIDTH).min(viewport_width);
    let t = labels.profile.tokens(ctx);
    egui::Window::new(labels.title)
        .id(egui::Id::new((scope, "notice-history")))
        .open(open)
        .resizable(true)
        .default_width(metrics::NOTICE_HISTORY_WIDTH.min(width))
        .max_width(width)
        .show(ctx, |ui| {
            if let Some(text) = omitted_text {
                ui.label(text);
            }
            egui::ScrollArea::vertical().max_height((size.y - metrics::NOTICE_HISTORY_BOTTOM_RESERVE).max(metrics::NOTICE_HISTORY_MIN_HEIGHT)).show(ui, |ui| {
                for notice in history.iter().rev() {
                    ui.push_id(notice.id, |ui| {
                        ui.add(
                            egui::Label::new(egui::RichText::new(&notice.title).strong().color(if notice.error || notice.warning_count > 0 {
                                match labels.profile {
                                    crate::widgets::Profile::Editor => t.warning,
                                    crate::widgets::Profile::Library => t.library.caution,
                                }
                            } else {
                                t.text
                            }))
                            .wrap(),
                        );
                        ui.horizontal_wrapped(|ui| {
                            if notice.error {
                                ui.label(labels.error);
                            }
                            if notice.warning_count > 0 {
                                ui.label(format!("{} {}", notice.warning_count, labels.warnings));
                            }
                            if button(ui, scope, &format!("copy:{}", notice.id), labels.copy, labels.profile).clicked() {
                                ctx.copy_text(format!("{}\n{}", notice.title, notice.lines.join("\n")));
                            }
                            if dismissible(notice) && button(ui, scope, &format!("dismiss:{}", notice.id), labels.dismiss, labels.profile).clicked() {
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
    dismissed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn j18_shared_notice_parity_retains_warnings_and_persistent_guidance() {
        let mut active = vec![Notice { id: 1, dismiss_pref: Some("guidance".into()), ..Default::default() }];
        let mut history = Vec::new();
        let mut omitted = 0;
        for id in 2..=8 {
            let notice = Notice { id, title: format!("Result {id}"), lines: vec!["Encoder warning".into()], warning_count: 1, ..Default::default() };
            active.push(notice.clone());
            remember(&mut history, &active, &mut omitted, notice);
            cap_active(&mut active);
        }
        assert_eq!(active.iter().map(|notice| notice.id).collect::<Vec<_>>(), [1, 7, 8]);
        let notice = &mut history[0];
        complete(notice, "Saved".into(), "Added to Library".into(), true);
        complete(notice, "Saved".into(), "Added to Library".into(), false);
        assert_eq!(notice.lines, ["Encoder warning", "Added to Library"]);
        assert_eq!(notice.warning_count, 1);
        assert!(notice.error);
        active.clear();
        assert_eq!(history.len(), 7);
        assert_eq!(omitted, 0);
    }

    #[test]
    fn j18_shared_notice_history_bounds_disclose_truncation_and_preserve_active() {
        let active = vec![Notice { id: 1, ..Default::default() }];
        let mut history = active.clone();
        let mut omitted = 0;
        for id in 2..=300 {
            remember(&mut history, &active, &mut omitted, Notice { id, ..Default::default() });
        }
        assert_eq!(history.len(), MAX_HISTORY);
        assert_eq!(omitted, 44);
        assert_eq!(history[0].id, 1);
        assert_eq!(history.last().unwrap().id, 300);
    }

    #[test]
    fn j18_shared_notice_toasts_retain_replaced_and_expired_diagnostics() {
        let mut state = ToastHistory::default();
        state.post("First import warning".into(), 1.4, false);
        state.post("Save failed".into(), 6.0, true);
        state.observe(Some(&("Save failed".into(), 6.0)));
        state.observe(None);
        state.observe(Some(&("Legacy notice".into(), 8.0)));
        assert_eq!(state.history.len(), 3);
        assert!(state.history.iter().all(|notice| notice.status_only));
        assert!(state.history[1].error);
        assert_eq!(state.history[0].title, "First import warning");
    }

    #[test]
    fn j18_shared_notice_old_wire_shape_loads_with_defaults() {
        let old = serde_json::json!({"id": 7, "title": "Old notice"});
        let notice: Notice = serde_json::from_value(old).unwrap();
        assert_eq!(notice.id, 7);
        assert_eq!(notice, Notice { id: 7, title: "Old notice".into(), ..Default::default() });
    }
}
