use super::*;

fn choose(settings: &mut AiSettings, server: &li_ai::discovery::Server) {
    settings.setup_mode = "remote".into();
    settings.comfy_host = server.address.to_string();
    settings.comfy_port = server.port;
}

pub(super) fn row(ui: &mut egui::Ui, settings: &mut AiSettings) {
    let snapshot = dialogs_mut(|d| d.lan.as_ref().map(li_ai::discovery::Scan::snapshot));
    let running = snapshot.as_ref().is_some_and(|s| s.running);
    ui.horizontal(|ui| {
        if ui.add_enabled_ui(!running, |ui| widgets::secondary_button(ui, tl!("Find on LAN"), 0.0)).inner.clicked() {
            match li_ai::discovery::Scan::start(settings.comfy_port) {
                Ok(scan) => dialogs_mut(|d| d.lan = Some(scan)),
                Err(error) => dialogs_mut(|d| d.message = error.to_string()),
            }
        }
        if running {
            ui.spinner();
            if widgets::secondary_button(ui, tl!("Cancel"), 0.0).clicked() {
                dialogs_mut(|d| {
                    if let Some(scan) = &d.lan {
                        scan.cancel();
                    }
                });
            }
        }
    });
    ui.label(tl!("Search nearby LAN addresses on port 8188 and the selected port. For other networks, enter the address manually."));
    if cfg!(target_os = "macos") {
        ui.label(tl!("On macOS, allow Local Image in System Settings > Privacy & Security > Local Network."));
    }
    if let Some(snapshot) = snapshot {
        if running {
            ui.label(crate::i18n::fmt(
                tl!("Searching LAN… {checked}/{total}"),
                &[("checked", &snapshot.checked.to_string()), ("total", &snapshot.total.to_string())],
            ));
            ui.ctx().request_repaint_after(Duration::from_millis(150));
        }
        for server in &snapshot.servers {
            let label = format!("{}:{} · {}", server.address, server.port, server.device.chars().take(80).collect::<String>());
            if widgets::secondary_button(ui, &label, 0.0).on_hover_text(tl!("Use this ComfyUI server")).clicked() {
                choose(settings, server);
                dialogs_mut(|d| d.lan = None);
            }
        }
        if let Some(error) = &snapshot.error {
            ui.label(error);
        }
        if !running && snapshot.servers.is_empty() && snapshot.error.is_none() {
            ui.label(tl!(
                "No ComfyUI server found. Start ComfyUI with --listen 0.0.0.0 on the server and allow its port through the firewall, then search again."
            ));
        }
        if !snapshot.networks.is_empty() {
            ui.small(snapshot.networks.join(", "));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn selecting_lan_server_enables_remote_without_changing_local_models() {
        let mut settings = AiSettings::default();
        let before = settings.clone();
        choose(&mut settings, &li_ai::discovery::Server { address: "192.168.0.31".parse().unwrap(), port: 8000, device: "GPU".into() });
        assert!(settings.is_remote());
        assert_eq!(settings.comfy_host, "192.168.0.31");
        assert_eq!(settings.comfy_port, 8000);
        assert_eq!(settings.comfy_directory, before.comfy_directory);
    }
}
