//! One progress + cancel readout for background jobs in both halves of the app (duplicate-systems
//! audit D14): the editor's status bar, job dialog and opening tabs, and the Library's task panel
//! (export, culling, Smart Sort, faces…) draw their bars and job rows here. Fed by a job's
//! fraction (`lightcraft_sysmem::JobCtl::fraction`: `None` = not reported yet, drawn as an
//! indeterminate sliding segment). Callers translate labels.
use crate::theme::Tokens;
use crate::widgets::{Profile, SelectionStyle};
use egui::{Rect, Response, RichText, Sense, Stroke, Ui, vec2};

/// Progress worth showing as a number: reported and above zero. `None` keeps the bar
/// indeterminate (a job that has not reported yet, or reported 0).
pub fn shown(fraction: Option<f32>) -> Option<f32> {
    fraction.filter(|f| *f > 0.0)
}

/// "45%", or empty while the bar is indeterminate.
pub fn percent(fraction: Option<f32>) -> String {
    shown(fraction).map(|f| format!("{:.0}%", f.clamp(0.0, 1.0) * 100.0)).unwrap_or_default()
}

/// A progress bar: `fraction` of `rect` filled with the accent; `None` = indeterminate (a sliding
/// segment), for jobs that haven't reported progress yet.
pub fn bar(ui: &Ui, rect: Rect, fraction: Option<f32>, t: &Tokens) {
    let p = ui.painter();
    let r = rect.height() / 2.0;
    p.rect_filled(rect, r, t.field);
    p.rect_stroke(rect, r, Stroke::new(1.0, t.field_border), egui::StrokeKind::Inside);
    let fill = match fraction {
        Some(f) => Rect::from_min_size(rect.min, vec2((rect.width() * f.clamp(0.0, 1.0)).max(rect.height()), rect.height())),
        None => {
            // The sliding segment animates: keep frames coming while it is on screen.
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(16));
            let time = ui.input(|i| i.time) as f32;
            let w = rect.width() * 0.3;
            let x = rect.left() + ((time * 0.8).fract() * (rect.width() + w)) - w;
            Rect::from_min_max(egui::pos2(x.max(rect.left()), rect.top()), egui::pos2((x + w).min(rect.right()), rect.bottom()))
        }
    };
    if fill.width() > 0.0 {
        p.rect_filled(fill, r, t.accent);
    }
}

/// One running job, as [`job_row`] shows it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct JobLine<'a> {
    /// What runs ("Exporting 3 of 12", "Assisted Culling"), translated.
    pub title: &'a str,
    /// The current item or stage (a file name, "Loading model…"); may be empty.
    pub detail: &'a str,
    /// Progress (`None` = indeterminate).
    pub fraction: Option<f32>,
    /// Cancel was asked for and the job is winding down: the button is disabled.
    pub stopping: bool,
}

/// A job row: title and the Cancel button on one line, the bar, then the detail and percentage.
/// `cancel` is the button's translated label. Returns the Cancel button's response.
pub fn job_row(ui: &mut Ui, line: &JobLine<'_>, cancel: &str, profile: Profile) -> Response {
    let t = profile.tokens(ui.ctx());
    // The Library's font stack has no medium weight.
    let title = match profile {
        Profile::Library => crate::fonts::semibold(12.0),
        Profile::Editor => crate::fonts::medium(12.5),
    };
    let mut cancel_response = None;
    ui.horizontal(|ui| {
        ui.label(RichText::new(line.title).font(title).color(t.text));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.add_enabled_ui(!line.stopping, |ui| {
                cancel_response = Some(match profile {
                    Profile::Library => crate::widgets::selection(ui, cancel, false, SelectionStyle::Text, None),
                    Profile::Editor => crate::widgets::secondary_button(ui, cancel, 76.0),
                });
            });
        });
    });
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 6.0), Sense::hover());
    bar(ui, rect, shown(line.fraction), &t);
    let percent = percent(line.fraction);
    if !line.detail.is_empty() || !percent.is_empty() {
        ui.horizontal(|ui| {
            ui.add(egui::Label::new(RichText::new(line.detail).size(11.0).color(t.text_dim)).truncate());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(RichText::new(percent).size(11.0).color(t.text_faint).monospace());
            });
        });
    }
    cancel_response.expect("the cancel button is always drawn")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn progress_percent_and_indeterminate() {
        assert_eq!(shown(None), None);
        assert_eq!(shown(Some(0.0)), None, "nothing reported yet: indeterminate, no number");
        assert_eq!(percent(Some(0.0)), "");
        assert_eq!(percent(Some(0.456)), "46%");
        assert_eq!(percent(Some(3.0)), "100%");
    }

    #[test]
    fn progress_job_row_draws_and_reports_cancel() {
        for profile in [Profile::Library, Profile::Editor] {
            let ctx = egui::Context::default();
            ctx.set_fonts(crate::fonts::definitions(match profile {
                Profile::Library => crate::fonts::Profile::Library,
                Profile::Editor => crate::fonts::Profile::Editor,
            }));
            for stopping in [false, true] {
                let mut output = ctx.run_ui(Default::default(), |ui| {
                    let line = JobLine { title: "Exporting 1 of 2", detail: "photo.jpg", fraction: Some(0.5), stopping };
                    let r = job_row(ui, &line, "Cancel", profile);
                    assert!(r.rect.width() > 0.0);
                    assert_eq!(r.enabled(), !stopping);
                    assert!(!r.clicked());
                });
                output.textures_delta.clear();
            }
        }
    }
}
