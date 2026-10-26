//! Set Up Models' confirmations and the template chooser: moving a file, downloading one (after
//! its size and checksum were looked up) and choosing which official template a file runs through.

use widgets::{ButtonRole, DialogButton};

use super::*;

pub(super) fn any_open(s: &State) -> bool {
    s.confirm_move.is_some() || s.confirm_download.is_some() || s.choose_template.is_some()
}

fn buttons(ui: &mut egui::Ui, list: &[DialogButton]) -> Option<ButtonRole> {
    ui.add_space(10.0);
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| widgets::dialog_buttons(ui, list)).inner
}

/// Shows whichever dialog is open; its answers become actions.
pub(super) fn show(ctx: &egui::Context, s: &mut State, t: &Tokens, out: &mut Vec<Action>) {
    if let Some(ask) = s.confirm_move.clone() {
        let mut answer = None;
        let modal = egui::Modal::new(egui::Id::new("model-setup-move")).backdrop_color(egui::Color32::TRANSPARENT).show(ctx, |ui| {
            ui.set_width(460.0);
            ui.label(
                RichText::new(li_i18n::fmt(tl!("Move {file} to {folder}/?"), &[("file", &ask.file), ("folder", &ask.to_folder)]))
                    .font(crate::theme::semibold(15.0)),
            );
            widgets::hairline(ui);
            ui.add_space(6.0);
            ui.add(
                egui::Label::new(
                    RichText::new(li_i18n::fmt(
                        tl!("ComfyUI will list it as a {kind} after the move. You can undo it for 30 seconds."),
                        &[("kind", li_i18n::t(&ask.kind))],
                    ))
                    .color(t.text_dim),
                )
                .wrap(),
            );
            answer = buttons(ui, &[DialogButton::new(ButtonRole::Default, "Move", 84.0), DialogButton::new(ButtonRole::Cancel, "Cancel", 84.0)]);
        });
        let escape = modal.is_top_modal && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
        match answer {
            Some(ButtonRole::Default) => {
                out.push(Action::Move { file: ask.file, to_folder: ask.to_folder });
                s.confirm_move = None;
            }
            Some(_) => s.confirm_move = None,
            None if escape => s.confirm_move = None,
            None => {}
        }
    }
    if let Some(plan) = s.confirm_download.clone() {
        let mut answer = None;
        let modal = egui::Modal::new(egui::Id::new("model-setup-download")).backdrop_color(egui::Color32::TRANSPARENT).show(ctx, |ui| {
            ui.set_width(500.0);
            let size = li_ai::download::human_bytes(plan.total());
            ui.label(RichText::new(li_i18n::fmt(tl!("Download {size}?"), &[("size", &size)])).font(crate::theme::semibold(15.0)));
            widgets::hairline(ui);
            ui.add_space(6.0);
            for f in &plan.files {
                let host = f.url.split('/').nth(2).unwrap_or_default();
                ui.vertical(|ui| {
                    ui.add(egui::Label::new(RichText::new(&f.name).color(t.text)).wrap());
                    ui.label(
                        RichText::new(li_i18n::fmt(
                            tl!("→ {folder}/ · {size} · {host}"),
                            &[("folder", &f.folder), ("size", &li_ai::download::human_bytes(f.bytes)), ("host", host)],
                        ))
                        .size(11.0)
                        .color(t.text_faint),
                    );
                });
            }
            if !plan.license.is_empty() {
                ui.label(RichText::new(li_i18n::fmt(tl!("Licence: {licence}"), &[("licence", &plan.license)])).color(t.text_dim));
            }
            ui.label(RichText::new(tl!("Local Image checks each file's size and SHA-256 before ComfyUI uses it.")).size(11.5).color(t.text_faint));
            if let Some(dir) = options().model_dir
                && let Some(free) = li_ai::download::free_space(&dir)
                && free < plan.total() + (256 << 20)
            {
                ui.label(
                    RichText::new(li_i18n::fmt(
                        tl!("Not enough space in {folder} ({free} free)."),
                        &[("folder", &dir.display().to_string()), ("free", &li_ai::download::human_bytes(free))],
                    ))
                    .color(t.danger),
                );
            }
            answer = buttons(ui, &[DialogButton::new(ButtonRole::Default, "Download", 84.0), DialogButton::new(ButtonRole::Cancel, "Cancel", 84.0)]);
        });
        let escape = modal.is_top_modal && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
        match answer {
            Some(ButtonRole::Default) => {
                out.push(Action::Download(plan));
                s.confirm_download = None;
            }
            Some(_) => s.confirm_download = None,
            None if escape => s.confirm_download = None,
            None => {}
        }
    }
    let Some(mut chooser) = s.choose_template.clone() else { return };
    let Some(scan) = s.scan.clone() else {
        s.choose_template = None;
        return;
    };
    let current = scan.server.models.get(&chooser.key).and_then(|m| m.user.template_choice.clone());
    let mut picked: Option<Option<String>> = None;
    let mut close = false;
    let modal = egui::Modal::new(egui::Id::new("model-setup-template")).backdrop_color(egui::Color32::TRANSPARENT).show(ctx, |ui| {
        ui.set_width(520.0);
        ui.label(RichText::new(tl!("Choose a template")).font(crate::theme::semibold(15.0)));
        widgets::hairline(ui);
        ui.add(
            egui::Label::new(
                RichText::new(tl!("The file runs through the template you choose; only a passing test makes it verified.")).size(11.5).color(t.text_dim),
            )
            .wrap(),
        );
        ui.add_space(6.0);
        ui.add(egui::TextEdit::singleline(&mut chooser.search).hint_text(tl!("Search templates…")).desired_width(f32::INFINITY));
        let query = chooser.search.trim().to_lowercase();
        egui::ScrollArea::vertical().max_height(300.0).show(ui, |ui| {
            if widgets::nav_row(ui, "", None, tl!("Let Local Image choose"), "", current.is_none()).clicked() {
                picked = Some(None);
            }
            for c in scan.templates.iter().filter(|c| query.is_empty() || c.title.to_lowercase().contains(&query) || c.name.contains(&query)) {
                let on = current.as_deref() == Some(c.name.as_str());
                let r = widgets::nav_row(ui, "", None, &c.title, "", on);
                if r.on_hover_text(&c.name).clicked() {
                    picked = Some(Some(c.name.clone()));
                }
            }
        });
        close = buttons(ui, &[DialogButton::new(ButtonRole::Cancel, "Cancel", 84.0)]).is_some();
    });
    close |= modal.is_top_modal && !modal.any_popup_open && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
    if let Some(choice) = picked {
        out.push(Action::Choose(chooser.key.clone(), Choice::Template(choice)));
        close = true;
    }
    s.choose_template = (!close).then_some(chooser);
}
