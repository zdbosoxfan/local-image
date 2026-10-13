use super::*;
pub(super) fn draw(ui: &mut egui::Ui, settings: &mut AiSettings, st: &EngineStatus, t: &Tokens) {
    let mut remote_mode = settings.is_remote();
    let response = widgets::checkbox(ui, &mut remote_mode, tl!("Use a remote ComfyUI server"));
    response.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Checkbox, true, remote_mode, tl!("Use a remote ComfyUI server")));
    if response.changed() {
        settings.setup_mode = if remote_mode { "remote" } else { "discover" }.into();
        if !remote_mode {
            settings.comfy_host = "127.0.0.1".into();
        }
    }
    ui.horizontal_wrapped(|ui| {
        let address_label = ui.label(RichText::new(tl!("Address:")).color(t.text_dim));
        ui.add(egui::TextEdit::singleline(&mut settings.comfy_host).desired_width(240.0).hint_text("192.168.1.20 / gpu.example.ts.net"))
            .labelled_by(address_label.id);
        let port_label = ui.label(RichText::new(tl!("Port:")).color(t.text_dim));
        let mut port = settings.comfy_port as f32;
        if widgets::value_field(ui, &mut port, 1.0..=65535.0, "", 70.0).labelled_by(port_label.id).changed() {
            settings.comfy_port = port as u16;
        }
    });
    lan_discovery::row(ui, settings);
    let remote = settings.is_remote();
    if remote {
        ui.label(RichText::new(tl!("Models and generation run on the server; images and results transfer automatically. Use its LAN or Tailscale address, or 127.0.0.1 with an SSH tunnel.")).color(t.text_dim));
        ui.label(
            RichText::new(tl!("Start ComfyUI and install generative models on the server. On-device tools still run on this computer.")).color(t.text_faint),
        );
        if widgets::secondary_button(ui, tl!("Connect / Check Server"), 0.0).clicked() {
            let _ = save_connection(settings);
            refresh();
        }
    }
    if !remote {
        ui.horizontal(|ui| {
            opt(ui, tl!("Installation:"));
            let label =
                if settings.comfy_directory.is_empty() { tl!("Not chosen — detected automatically").to_owned() } else { settings.comfy_directory.clone() };
            ui.label(RichText::new(label).color(t.text_dim));
        });
        ui.horizontal(|ui| {
            if widgets::secondary_button(ui, tl!("Detect"), 0.0).clicked() {
                let found = li_ai::setup::detect(settings);
                dialogs_mut(|d| {
                    d.message = if found.is_empty() {
                        tl!("No ComfyUI installation found in the usual places.").into()
                    } else {
                        crate::i18n::fmt(tl!("Found {n} installation(s)."), &[("n", &found.len().to_string())])
                    };
                    d.detected = Some(found);
                });
            }
            #[cfg(not(target_arch = "wasm32"))]
            if widgets::secondary_button(ui, tl!("Choose Folder…"), 0.0).clicked()
                && let Some(dir) = crate::dialog_filter::file_dialog().set_title(tl!("Choose your ComfyUI folder")).pick_folder()
            {
                if li_ai::setup::installation(&dir, None).is_some() {
                    settings.comfy_directory = dir.display().to_string();
                    dialogs_mut(|d| d.message = tl!("ComfyUI installation found.").into());
                } else {
                    dialogs_mut(|d| d.message = tl!("That folder has no ComfyUI (main.py and a Python environment).").into());
                }
            }
            let running_ours = CHILD.is_running().unwrap_or(true);
            if !st.connected && !st.starting && widgets::primary_button(ui, tl!("Start AI Engine"), 0.0).clicked() {
                let m = match save_connection(settings).and_then(|_| start_engine()) {
                    Ok(l) => crate::i18n::fmt(tl!("Starting {name}…"), &[("name", &l)]),
                    Err(e) => e,
                };
                dialogs_mut(|d| d.message = m);
            }
            if running_ours && widgets::secondary_button(ui, tl!("Stop"), 0.0).clicked() {
                stop_engine();
            }
            if st.connected && widgets::secondary_button(ui, tl!("Free GPU Memory"), 0.0).clicked() {
                let m = match li_ai::service().client.free_memory() {
                    Ok(()) => tl!("Models unloaded.").to_owned(),
                    Err(e) => format!("{e:#}"),
                };
                dialogs_mut(|d| d.message = m);
            }
        });
        if let Some(found) = dialogs_mut(|d| d.detected.clone()) {
            for inst in found {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(inst.label()).color(t.text_dim));
                    if widgets::secondary_button(ui, tl!("Use"), 0.0).clicked() {
                        settings.comfy_directory = inst.code.display().to_string();
                        settings.comfy_python = inst.python.display().to_string();
                    }
                });
            }
        }
    }
}
