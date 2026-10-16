use super::*;
pub(super) fn draw(ui: &mut egui::Ui, settings: &mut AiSettings, t: &Tokens) {
    widgets::section_label(ui, tl!("CLOUD (OPTIONAL)"));
    ui.label(
                    RichText::new(tl!("With your own API key, Generate can also use cloud models (they appear under Cloud in the model picker). Your prompt and images then go from this computer to that provider and are billed to your account; nothing else is sent anywhere."))
                        .color(t.text_faint)
                        .size(11.0),
                );
    for p in li_ai::cloud::Provider::ALL {
        ui.horizontal(|ui| {
            let label = ui.add_sized([128.0, 20.0], egui::Label::new(RichText::new(format!("{}:", p.label())).color(t.text_dim)));
            let mut v = settings.extra_string(p.key_name()).unwrap_or_default();
            let width = (ui.available_width() - 70.0).clamp(100.0, 220.0);
            if ui.add(egui::TextEdit::singleline(&mut v).password(true).hint_text(tl!("API key")).desired_width(width)).labelled_by(label.id).changed() {
                settings.set_extra_string(p.key_name(), &v);
            }
            if ui.link(RichText::new(tl!("Get a key")).size(11.0)).clicked() {
                ui.ctx().open_url(egui::OpenUrl::new_tab(p.keys_url()));
            }
        });
    }
    widgets::checkbox(ui, &mut settings.cloud_confirm_each_run, tl!("Confirm before each cloud generation"));
    ui.horizontal(|ui| {
        let label = ui.label(RichText::new(tl!("Session limit (images):")).color(t.text_dim));
        ui.add(egui::DragValue::new(&mut settings.cloud_session_cap_images).range(0..=10_000).speed(1.0)).labelled_by(label.id);
        ui.label(RichText::new(tl!("0 means no limit")).color(t.text_faint).size(11.0));
    });
    ui.add_space(8.0);
    widgets::section_label(ui, tl!("ACCOUNTS (OPTIONAL)"));
    ui.label(
        RichText::new(tl!("Tokens let the Model Browser download gated or sign-in-only models. They're sent only to their own site."))
            .color(t.text_faint)
            .size(11.0),
    );
    for (key, label, hint) in [("hf_token", tl!("Hugging Face token:"), "hf_…"), ("civitai_token", tl!("Civitai API key:"), "")] {
        ui.horizontal(|ui| {
            let label = ui.add_sized([128.0, 20.0], egui::Label::new(RichText::new(label).color(t.text_dim)));
            let mut v = settings.extra_string(key).unwrap_or_default();
            let width = ui.available_width().clamp(100.0, 260.0);
            if ui.add(egui::TextEdit::singleline(&mut v).password(true).hint_text(hint).desired_width(width)).labelled_by(label.id).changed() {
                settings.set_extra_string(key, &v);
            }
        });
    }
}
