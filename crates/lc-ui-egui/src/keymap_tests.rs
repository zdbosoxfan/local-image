#[cfg(test)]
mod tests {
    use crate::{
        LightcraftApp,
        headless::Headless,
        state::{RightPanel, ViewMode},
    };
    use li_keys::keymaps::CAPTURE_ONE;
    use serde_json::json;
    use std::time::Duration;
    #[test]
    fn keys_profiles_g1_g2_real_library_develop_events_and_global_precedence() {
        let session = match std::env::var("LC_KEYS_REFERENCE") {
            Ok(path) => {
                let mut s = lightcraft_engine::Session::new().with_fs();
                s.execute("library.import", &json!({"paths":[path]})).unwrap();
                s
            }
            Err(_) => lightcraft_engine::Session::with_demo(),
        };
        let mut h = Headless::new(LightcraftApp::new(session, Default::default()), [1200., 800.], 1.);
        let time = Duration::from_secs(10);
        h.request("ui.key", json!({"key":"d"}), time);
        assert_eq!(h.app.ui.right, RightPanel::Edit);
        let before = h.app.ui.mask_overlay;
        h.request("ui.key", json!({"key":"o"}), time);
        assert_ne!(h.app.ui.mask_overlay, before);
        h.request("ui.key", json!({"key":"b"}), time);
        assert_ne!(h.app.ui.tool, "brush", "Lightroom B belongs to target collection");
        let quick = h.app.session.catalog.albums().find(|a| a.quick).unwrap();
        assert!(!quick.photos.is_empty(), "Lightroom B adds the selected photo to Quick Collection");
        h.app.shortcut_overrides.insert("library::__profile".into(), "Capture One".into());
        for (key, tool, erase) in [("b", "brush", false), ("e", "brush", true), ("b", "brush", false), ("g", "linear", false)] {
            h.request("ui.key", json!({"key":key}), time);
            assert_eq!(h.app.ui.tool, tool, "{key}");
            assert_eq!(h.app.ui.brush_erase, erase, "{key}");
        }
        let before = h.app.ui.mask_overlay;
        h.request("ui.key", json!({"key":"m"}), time);
        assert_ne!(h.app.ui.mask_overlay, before);
        // Rebinding overlay leaves its parent command available on the new chord.
        h.app.shortcut_overrides.insert("library::view.maskOverlay".into(), "Shift+O".into());
        let before = h.app.ui.mask_overlay;
        h.request("ui.key", json!({"key":"m"}), time);
        assert_eq!(h.app.ui.mask_overlay, before);
        h.request("ui.key", json!({"key":"o","shift":true}), time);
        assert_ne!(h.app.ui.mask_overlay, before);
        h.app.shortcut_overrides.insert("library::tool.brush".into(), "Cmd+Q".into());
        assert!(!crate::shortcuts::catalog::effective(&h.app).iter().any(|(b, key)| b.command == "tool.brush" && key == "Cmd+Q"));
        // In Library, E/G are loupe/grid again. B retains the Library target-collection action.
        h.app.ui.tool.clear();
        h.app.ui.right = RightPanel::None;
        h.app.ui.view = ViewMode::PhotoGrid;
        h.request("ui.key", json!({"key":"e"}), time);
        assert_eq!(h.app.ui.right, RightPanel::None);
        assert_eq!(h.app.ui.view, ViewMode::Detail);
        h.request("ui.key", json!({"key":"g"}), time);
        assert_eq!(h.app.ui.view, ViewMode::PhotoGrid);
        // Existing host modal focus protects hidden Develop tools.
        h.app.host_modal = true;
        h.app.ui.right = RightPanel::Edit;
        h.app.ui.view = ViewMode::Detail;
        // The host suspends Library's control queue while a modal is open.
        // Deliver native events directly, just as the focused host viewport does.
        let now = h.view.ctx.input(|input| input.time);
        for pressed in [true, false] {
            let raw = crate::headless::HeadlessView::raw_input(
                egui::vec2(1200., 800.),
                1.,
                now,
                vec![egui::Event::Key { key: egui::Key::E, physical_key: None, pressed, repeat: false, modifiers: egui::Modifiers::NONE }],
            );
            let app = &mut h.app;
            h.view.run(raw, |ui| {
                app.logic(ui.ctx());
                app.ui(ui);
            });
        }
        assert!(h.app.ui.tool.is_empty());
        assert!(CAPTURE_ONE.iter().any(|(id, key)| *id == "view.maskOverlay" && *key == "M"));
        h.app.host_modal = false;
        h.request("ui.key", json!({"key":"q","cmd":true}), time);
        assert!(h.quit_requested(), "Global Quit receives the real event over a conflicting C1 brush binding");
        assert!(h.app.ui.tool.is_empty());
    }
}
