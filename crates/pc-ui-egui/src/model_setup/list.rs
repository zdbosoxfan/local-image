//! The model list: every main model file grouped by what it needs, each row with its status
//! icon; text encoders and VAEs and ignored files fold away below. Up and Down move between rows.

use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Group {
    New,
    Attention,
    Verified,
    Builtin,
    Ignored,
}

fn group(status: Status) -> Group {
    match status {
        Status::New | Status::Ready => Group::New,
        Status::Blocked | Status::Unmatched | Status::Failed | Status::NeedsReverify | Status::PartiallyVerified => Group::Attention,
        Status::Verified => Group::Verified,
        Status::Builtin => Group::Builtin,
        Status::Ignored => Group::Ignored,
    }
}

const GROUPS: [Group; 4] = [Group::New, Group::Attention, Group::Verified, Group::Builtin];

fn title(g: Group, n: usize) -> String {
    let text = match g {
        Group::New => tl!("NEW · {n}"),
        Group::Attention => tl!("NEEDS ATTENTION · {n}"),
        Group::Verified => tl!("VERIFIED · {n}"),
        Group::Builtin => tl!("BUILT-IN · {n}"),
        Group::Ignored => tl!("IGNORED · {n}"),
    };
    li_i18n::fmt(text, &[("n", &n.to_string())])
}

fn keys_in(scan: &Scan, g: Group) -> Vec<String> {
    let mut v: Vec<(&String, &ModelState)> = scan.server.models.iter().filter(|(_, m)| group(m.status) == g).collect();
    v.sort_by(|a, b| a.1.label.to_lowercase().cmp(&b.1.label.to_lowercase()).then(a.0.cmp(b.0)));
    v.into_iter().map(|(k, _)| k.clone()).collect()
}

/// The model keys in display order (what Up and Down walk).
pub(super) fn order(scan: &Scan) -> Vec<String> {
    GROUPS.iter().chain([Group::Ignored].iter()).flat_map(|g| keys_in(scan, *g)).collect()
}

pub(super) fn show(ui: &mut egui::Ui, s: &mut State, t: &Tokens, out: &mut Vec<Action>) {
    let Some(scan) = s.scan.clone() else {
        ui.label(RichText::new(tl!("Checking models…")).color(t.text_dim));
        return;
    };
    if scan.server.models.is_empty() {
        let folder = options().model_dir.map(|d| d.display().to_string()).unwrap_or_else(|| tl!("ComfyUI's model folder").into());
        ui.add(
            egui::Label::new(
                RichText::new(li_i18n::fmt(tl!("No new models. Add model files to {folder} or install one from the Model Browser."), &[("folder", &folder)]))
                    .color(t.text_dim),
            )
            .wrap(),
        );
        if widgets::secondary_button_translated(ui, tl!("Open Model Browser"), 0.0).clicked() {
            out.push(Action::OpenModelBrowser);
        }
        return;
    }
    let order: Vec<String> = order(&scan).into_iter().filter(|key| s.show_ignored || scan.server.models[key].status != Status::Ignored).collect();
    for g in GROUPS {
        let keys = keys_in(&scan, g);
        if keys.is_empty() {
            continue;
        }
        ui.add_space(6.0);
        widgets::section_label_translated(ui, &title(g, keys.len()));
        for key in &keys {
            row(ui, s, t, &order, key, &scan.server.models[key], out);
        }
    }
    if !scan.other_files.is_empty() {
        ui.add_space(6.0);
        let label = li_i18n::fmt(tl!("OTHER FILES · {n}"), &[("n", &scan.other_files.len().to_string())]);
        let other = egui::CollapsingHeader::new(RichText::new(label).font(crate::theme::medium(10.5)).color(t.text_faint))
            .id_salt("model-setup-other")
            .open(Some(s.show_other))
            .show(ui, |ui| {
                for f in &scan.other_files {
                    let label = format!("{} · {}", f.file, li_i18n::t(&f.kind));
                    let r = ui.add(egui::Label::new(RichText::new(&label).size(11.5).color(t.text_dim)).truncate()).on_hover_text(&label);
                    if !f.used_by.is_empty() {
                        r.on_hover_text(li_i18n::fmt(tl!("Used by {models}"), &[("models", &f.used_by.join(", "))]));
                    }
                }
            });
        if other.header_response.clicked() {
            s.show_other = !s.show_other;
        }
    }
    let ignored = keys_in(&scan, Group::Ignored);
    if !ignored.is_empty() {
        let label = title(Group::Ignored, ignored.len());
        let header = egui::CollapsingHeader::new(RichText::new(label).font(crate::theme::medium(10.5)).color(t.text_faint))
            .id_salt("model-setup-ignored")
            .open(Some(s.show_ignored))
            .show(ui, |ui| {
                for key in &ignored {
                    row(ui, s, t, &order, key, &scan.server.models[key], out);
                }
            });
        if header.header_response.clicked() {
            s.show_ignored = !s.show_ignored;
        }
    }
}

fn row(ui: &mut egui::Ui, s: &mut State, t: &Tokens, order: &[String], key: &str, m: &ModelState, out: &mut Vec<Action>) {
    let selected = s.selected.as_deref() == Some(key);
    let (icon, _, tone) = chip(m.status);
    let r = widgets::nav_row(ui, icon, Some(tone.ink(t)), &m.label, "", selected);
    let spoken_label = format!("{}, {}", m.label, spoken(m.status));
    r.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Button, true, selected, &spoken_label));
    if r.clicked() {
        s.selected = Some(key.to_owned());
    }
    if r.has_focus() {
        let step = ui.input_mut(|i| {
            if i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown) {
                1
            } else if i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp) {
                -1
            } else {
                0
            }
        });
        let at = order.iter().position(|k| k == key);
        if let Some(next) = at.and_then(|i| i.checked_add_signed(step)).filter(|_| step != 0).and_then(|i| order.get(i)) {
            s.selected = Some(next.clone());
            ui.memory_mut(|mem| mem.data.insert_temp(egui::Id::new("model-setup-focus"), next.clone()));
        }
        if ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Enter)) {
            s.selected = Some(key.to_owned());
            s.step = Step::Check;
        }
    }
    if ui.memory(|mem| mem.data.get_temp::<String>(egui::Id::new("model-setup-focus"))).as_deref() == Some(key) {
        r.request_focus();
        r.scroll_to_me(Some(egui::Align::Center));
        ui.memory_mut(|mem| mem.data.remove::<String>(egui::Id::new("model-setup-focus")));
    }
    let ignored = m.status == Status::Ignored;
    r.context_menu(|ui| {
        let label = if ignored { tl!("Show again") } else { tl!("Ignore") };
        if widgets::command_menu_item(ui, label, None, None, s.busy().then_some(tl!("Wait for the current task to finish."))).clicked() {
            out.push(Action::Choose(key.to_owned(), Choice::Ignore(!ignored)));
            ui.close();
        }
    });
}
