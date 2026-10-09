//! About › Attributions and Preferences › Attributions: the shared page of [`li_attributions`]
//! (every project, model, asset and crate Local Image uses, with licences). Links open through
//! [`crate::links::open`].

use egui::{Sense, vec2};
use serde_json::{Map, Value};

use crate::PhotocraftApp;
use crate::theme::Tokens;

/// The Preferences section id of the page (shown after the engine's preference sections).
pub const SECTION: &str = "attributions";
/// State keys of the two places the page appears.
pub const ABOUT: &str = "pc-about";
pub const PREFS: &str = "pc-prefs";
/// Dialog field holding a link clicked in Preferences (the body has no app to open it with).
const OPEN_URL: &str = "__attributionsOpenUrl";

fn tr(s: &'static str) -> String {
    crate::i18n::t(s).to_string()
}

/// About › Attributions.
pub fn about(app: &mut PhotocraftApp, ui: &mut egui::Ui) {
    let out = li_attributions::show(ui, ABOUT, 360.0, &tr);
    if let Some(url) = out.open_url {
        crate::links::open(app, ui.ctx(), &url);
    }
}

/// The section list's "Attributions" row in Preferences, in the style of the rows above it.
pub fn prefs_nav(ui: &mut egui::Ui, section: &mut String) {
    let t = Tokens::get(ui.ctx());
    ui.add_space(6.0);
    let sel = section == SECTION;
    let (rect, resp) = ui.allocate_exact_size(vec2(170.0, 22.0), Sense::click());
    if sel {
        ui.painter().rect_filled(rect, t.radius_sm, t.row_selected);
    } else if resp.hovered() {
        ui.painter().rect_filled(rect, t.radius_sm, t.hover);
    }
    let color = if sel { t.text } else { t.text_dim };
    let label = tl!("Attributions");
    ui.painter().text(rect.left_center() + vec2(8.0, 0.0), egui::Align2::LEFT_CENTER, label, crate::theme::medium(12.5), color);
    resp.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::SelectableLabel, true, sel, label));
    if resp.clicked() {
        *section = SECTION.to_string();
    }
}

/// Preferences › Attributions: draws the page and returns true when `section` is this page.
pub fn prefs_page(ui: &mut egui::Ui, section: &str, f: &mut Map<String, Value>) -> bool {
    if section != SECTION {
        return false;
    }
    let t = Tokens::get(ui.ctx());
    ui.label(egui::RichText::new(tl!("Attributions")).font(crate::theme::semibold(14.0)).color(t.text));
    ui.add_space(6.0);
    let out = li_attributions::show(ui, PREFS, 360.0, &tr);
    if let Some(url) = out.open_url {
        f.insert(OPEN_URL.into(), Value::String(url));
    }
    true
}

/// Open a link clicked in Preferences › Attributions.
pub fn open_pending(app: &mut PhotocraftApp, ctx: &egui::Context, f: &mut Map<String, Value>) {
    if let Some(Value::String(url)) = f.remove(OPEN_URL) {
        crate::links::open(app, ctx, &url);
    }
}

#[cfg(test)]
mod tests {
    use egui_kittest::{Harness, kittest::Queryable};
    use serde_json::json;

    use crate::PhotocraftApp;
    use crate::state::DialogKind;

    fn harness(app: PhotocraftApp) -> Harness<'static, PhotocraftApp> {
        // The dialog is already open, and fonts set on the context only apply from the next frame:
        // draw it once the app's font families (semibold) exist, as panels.rs does.
        let mut h = Harness::builder().with_size(egui::vec2(1200.0, 900.0)).build_ui_state(
            |ui, app| {
                if ui.ctx().fonts(|f| f.families().contains(&egui::FontFamily::Name("semibold".into()))) {
                    crate::dialogs::show(app, ui.ctx());
                }
            },
            app,
        );
        PhotocraftApp::setup_context(&h.ctx, crate::theme::ThemeKind::ALL[0]);
        h.run_steps(3);
        h
    }

    /// Search the page drawn under `salt` for each name and expect an entry with it.
    fn lists(h: &mut Harness<'static, PhotocraftApp>, salt: &str) {
        for name in ["darktable", "LensFun", "Lucide icons", "egui", "wgpu"] {
            li_attributions::set_query(&h.ctx, salt, name);
            h.run_steps(3);
            assert!(h.query_all_by_label(name).next().is_some(), "{salt}: {name} is listed");
        }
        li_attributions::set_query(&h.ctx, salt, "");
    }

    #[test]
    fn about_has_an_attributions_tab() {
        let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
        app.ui.open_dialog(DialogKind::About, Default::default());
        let mut h = harness(app);
        h.get_by_label("Attributions").click();
        h.run_steps(3);
        assert_eq!(h.state().ui.dialogs[0].fields.get("tab"), Some(&json!("attributions")));
        lists(&mut h, super::ABOUT);
    }

    #[test]
    fn preferences_have_an_attributions_page() {
        let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
        crate::prefs_ui::open_preferences(&mut app, "general");
        let mut h = harness(app);
        h.get_by_label("Attributions").click();
        h.run_steps(3);
        assert_eq!(h.state().ui.dialogs[0].fields.get("section"), Some(&json!(super::SECTION)));
        lists(&mut h, super::PREFS);
        // and it opens there directly
        let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
        crate::prefs_ui::open_preferences(&mut app, super::SECTION);
        assert_eq!(app.ui.dialogs[0].fields.get("section"), Some(&json!(super::SECTION)));
    }

    /// Every segmentation model li-seg can download is attributed.
    #[test]
    fn every_li_seg_model_is_attributed() {
        let models = &li_attributions::data().section("models").unwrap().entries;
        for m in li_seg::MODELS {
            let name = m.label.rsplit_once(" (").map_or(m.label, |(n, _)| n);
            assert!(models.iter().any(|e| e.name == name), "{} is missing from assets/attributions.json — run `cargo xtask attributions`", m.label);
        }
    }
}
