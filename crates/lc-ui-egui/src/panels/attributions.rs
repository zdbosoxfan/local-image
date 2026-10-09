//! Settings › Attributions and About › Attributions: the shared page of [`li_attributions`] (every
//! project, model, asset and crate Local Image uses, with licences), with links opened through
//! this app's link service. Automation ids: `field:attributionsSearch` and `attribution:{name}`
//! for each entry drawn.

use crate::LightcraftApp;

/// State keys of the two places the page appears.
pub const SETTINGS: &str = "lc-settings";
pub const ABOUT: &str = "lc-about";

/// Draw the page; `salt` is [`SETTINGS`] or [`ABOUT`].
pub fn body(app: &mut LightcraftApp, ui: &mut egui::Ui, salt: &str) {
    let tr = |s: &'static str| crate::i18n::tr(s).to_string();
    let out = li_attributions::show(ui, salt, 400.0, &tr);
    if let Some(r) = out.search {
        crate::widgets::register(ui.ctx(), "field:attributionsSearch", r);
    }
    for (name, rect) in out.rows {
        crate::widgets::register(ui.ctx(), format!("attribution:{name}"), rect);
    }
    if let Some(url) = out.open_url
        && let Err(e) = crate::links::open(app, &url)
    {
        log::warn!("could not open {url}: {e}");
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use serde_json::json;

    use crate::LightcraftApp;
    use crate::headless::Headless;

    fn headless() -> Headless {
        let services = crate::Services { png: None, ..Default::default() };
        Headless::new(LightcraftApp::new(lightcraft_engine::Session::new(), services), [1300.0, 900.0], 1.0)
    }

    /// Search the page drawn under `salt` for each name and expect its row on screen.
    fn lists(h: &mut Headless, salt: &str) {
        let t = Duration::from_secs(10);
        for name in ["darktable", "LensFun", "Lucide icons", "egui", "wgpu"] {
            li_attributions::set_query(&h.view.ctx, salt, name);
            h.step();
            h.step();
            let r = h.request("ui.widgets", json!({"filter": "attribution:"}), t);
            let rows = r["result"].as_array().cloned().unwrap_or_default();
            assert!(rows.iter().any(|w| w["id"] == format!("attribution:{name}")), "{salt}: {name} is listed: {r}");
        }
        li_attributions::set_query(&h.view.ctx, salt, "");
    }

    #[test]
    fn settings_has_an_attributions_tab() {
        let mut h = headless();
        let t = Duration::from_secs(10);
        let r = h.request("ui.key", json!({"key": ",", "cmd": true}), t);
        assert_eq!(r["ok"], true, "{r}");
        h.step();
        let r = h.request("ui.clickWidget", json!({"id": "button:settingsTab-attributions"}), t);
        assert_eq!(r["ok"], true, "{r}");
        assert_eq!(h.app.ui.dialog, Some(crate::state::Dialog::Settings { tab: "attributions".into() }));
        h.step();
        let r = h.request("ui.widgets", json!({"filter": "field:attributionsSearch"}), t);
        assert_eq!(r["result"].as_array().map(Vec::len), Some(1), "{r}");
        lists(&mut h, super::SETTINGS);
    }

    #[test]
    fn about_has_an_attributions_tab() {
        let mut h = headless();
        let t = Duration::from_secs(10);
        let r = h.request("ui.menu.invoke", json!({"id": "app.about"}), t);
        assert_eq!(r["ok"], true, "{r}");
        h.step();
        h.step();
        let r = h.request("ui.clickWidget", json!({"id": "button:aboutTab-attributions"}), t);
        assert_eq!(r["ok"], true, "{r}");
        h.step();
        let tab = crate::panels::dialogs::ABOUT_TABS.iter().position(|(id, _)| *id == "attributions");
        assert_eq!(h.view.ctx.data_mut(|d| d.get_temp::<u8>(egui::Id::new("about_tab"))).map(usize::from), tab);
        lists(&mut h, super::ABOUT);
        assert_eq!(h.app.ui.dialog, Some(crate::state::Dialog::About));
    }
}
