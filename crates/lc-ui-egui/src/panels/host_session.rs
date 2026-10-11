//! local-image: the banner of a host session in Develop — Compositing's **Camera Raw Filter**
//! edits a layer with the real Develop module. The host opens the layer as a temporary photo,
//! sets [`LightcraftApp::host_session`], and reads [`HostSession::result`]: OK (Enter) or Cancel
//! (Esc). While the session is open the Library's own chrome (top bar, sidebar, bottom bar) is
//! hidden, Library-only shortcuts don't fire, and the tools a filter can't use are hidden
//! ([`HostSession::hides`]).

use egui::RichText;

use crate::LightcraftApp;
use crate::theme::Tokens;
use crate::widgets::register;

/// A host session shown in Develop.
#[derive(Clone, Debug, Default)]
pub struct HostSession {
    /// What is being edited (a layer name).
    pub name: String,
    /// Develop sections and tools to hide (`calibration`, `crop`, `geometry`, `lensProfile`…).
    pub hidden: Vec<String>,
    /// Set when the user answers: `Some(true)` OK, `Some(false)` Cancel. The host takes it.
    pub result: Option<bool>,
}

impl HostSession {
    pub fn new(name: impl Into<String>, hidden: Vec<String>) -> Self {
        Self { name: name.into(), hidden, result: None }
    }

    /// Is section / tool `id` hidden?
    pub fn hides(&self, id: &str) -> bool {
        self.hidden.iter().any(|h| h == id)
    }

    /// May command `id` run (from a shortcut) during the session? Library-only commands can't:
    /// browsing, importing, exporting, other photos, the grid views.
    pub fn allows(&self, id: &str) -> bool {
        const DENY: [&str; 12] = [
            "library.",
            "file.",
            "app.export",
            "export.",
            "photo.",
            "view.photoGrid",
            "view.squareGrid",
            "view.compare",
            "view.survey",
            "view.people",
            "view.back",
            "view.leftPanel",
        ];
        let tool_hidden = (id == "panel.crop" || id == "tool.crop") && self.hides("crop");
        !tool_hidden && !DENY.iter().any(|d| id.starts_with(d))
    }
}

/// Is a session open whose sections hide `id`?
pub fn hides(app: &LightcraftApp, id: &str) -> bool {
    app.host_session.as_ref().is_some_and(|h| h.hides(id))
}

/// Enter = OK, Esc = Cancel (not while typing, or while a tool is in use: Esc ends the tool first).
/// Runs before the Library's own shortcuts.
pub fn keys(app: &mut LightcraftApp, ctx: &egui::Context) {
    let Some(h) = app.host_session.as_mut() else { return };
    if h.result.is_some() || ctx.egui_wants_keyboard_input() {
        return;
    }
    let tool = !app.ui.tool.is_empty();
    ctx.input_mut(|i| {
        if i.consume_key(egui::Modifiers::NONE, egui::Key::Enter) {
            h.result = Some(true);
        } else if !tool && i.consume_key(egui::Modifiers::NONE, egui::Key::Escape) {
            h.result = Some(false);
        }
    });
}

/// The banner across the top: "Camera Raw Filter · ‹name›" with Cancel and OK.
pub fn banner(app: &mut LightcraftApp, ui: &mut egui::Ui) {
    let Some(h) = app.host_session.as_ref() else { return };
    let t = Tokens::get(ui.ctx());
    let title = format!("{} · {}", crate::i18n::tr("Camera Raw Filter"), h.name);
    let mut answer = None;
    egui::Panel::top("host-session-banner")
        .exact_size(t.top_bar_h)
        .frame(egui::Frame::NONE.fill(t.chrome).inner_margin(egui::Margin::symmetric(14, 0)))
        .show(ui, |ui| {
            ui.horizontal_centered(|ui| {
                let r = ui.label(RichText::new(title).font(t.semibold(13.5)).color(t.text_label));
                register(ui.ctx(), "indicator:hostSession", r.rect);
                ui.add_space(12.0);
                ui.label(
                    RichText::new(crate::i18n::tr("The layer changes when you click OK (Enter); Cancel (Esc) leaves it as it was."))
                        .size(11.5)
                        .color(t.text_dim),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let ok = ui
                        .add(egui::Button::new(RichText::new(crate::i18n::tr("OK")).color(t.canvas)).fill(t.accent).min_size(egui::vec2(84.0, 26.0)));
                    register(ui.ctx(), "button:hostSessionOk", ok.rect);
                    let cancel = ui.add(egui::Button::new(crate::i18n::tr("Cancel")).min_size(egui::vec2(84.0, 26.0)));
                    register(ui.ctx(), "button:hostSessionCancel", cancel.rect);
                    if ok.clicked() {
                        answer = Some(true);
                    } else if cancel.clicked() {
                        answer = Some(false);
                    }
                });
            });
        });
    if let (Some(a), Some(h)) = (answer, app.host_session.as_mut()) {
        h.result = Some(a);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn library_only_commands_and_hidden_tools_are_blocked() {
        let h = HostSession::new("Sky", vec!["crop".into(), "calibration".into()]);
        assert!(h.hides("calibration") && !h.hides("light"));
        for id in ["library.next", "file.import", "photo.delete", "view.photoGrid", "app.export"] {
            assert!(!h.allows(id), "{id}");
        }
        for id in ["develop.set", "develop.reset", "edit.undo", "view.zoomIn", "mask.add"] {
            assert!(h.allows(id), "{id}");
        }
    }

    #[test]
    fn enter_is_ok_and_escape_is_cancel() {
        let ctx = egui::Context::default();
        let mut app = LightcraftApp::new(lightcraft_engine::Session::new(), crate::Services::default());
        for (key, want) in [(egui::Key::Enter, true), (egui::Key::Escape, false)] {
            app.host_session = Some(HostSession::new("Sky", vec![]));
            let raw = egui::RawInput {
                events: vec![egui::Event::Key { key, physical_key: None, pressed: true, repeat: false, modifiers: egui::Modifiers::NONE }],
                ..Default::default()
            };
            let mut out = ctx.run_ui(raw, |ui| keys(&mut app, ui.ctx()));
            out.textures_delta.clear();
            assert_eq!(app.host_session.as_ref().unwrap().result, Some(want));
        }
    }
}
