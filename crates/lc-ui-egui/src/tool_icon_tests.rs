use std::collections::HashSet;
use std::time::Duration;

use egui::{Color32, Rect};
use serde_json::json;

use crate::{LightcraftApp, color_icon_data, color_icons, headless::Headless, icons, state::RightPanel};

#[test]
fn all_develop_tools_panels_and_mask_variants_have_icons() {
    for (tool, name) in color_icon_data::DEVELOP_TOOL_ICONS {
        assert_eq!(icons::tool_name(tool), Some(*name));
        assert!(color_icons::exists(name), "{tool}: {name}");
    }
    // An exhaustive match protects against additions to the panel enum.
    let panel_name = |panel| match panel {
        RightPanel::None => None,
        RightPanel::Edit | RightPanel::Profiles => icons::tool_name("edit"),
        RightPanel::Crop => icons::tool_name("crop"),
        RightPanel::Remove => icons::tool_name("remove"),
        RightPanel::Masking => icons::tool_name("masking"),
        RightPanel::RedEye => icons::tool_name("redeye"),
        RightPanel::Versions => icons::tool_name("versions"),
        RightPanel::Activity => icons::tool_name("activity"),
        RightPanel::Keywords => icons::tool_name("keywords"),
        RightPanel::Info => icons::tool_name("info"),
    };
    for panel in [
        RightPanel::Edit,
        RightPanel::Profiles,
        RightPanel::Crop,
        RightPanel::Remove,
        RightPanel::Masking,
        RightPanel::RedEye,
        RightPanel::Versions,
        RightPanel::Activity,
        RightPanel::Keywords,
        RightPanel::Info,
    ] {
        assert!(color_icons::exists(panel_name(panel).unwrap()));
    }
    let mut distinct = HashSet::new();
    for shape in [
        json!({"kind":"brush","strokes":[]}),
        json!({"kind":"linear","start":{"x":0.0,"y":0.0},"end":{"x":1.0,"y":1.0}}),
        json!({"kind":"radial","center":{"x":0.5,"y":0.5},"rx":0.2,"ry":0.2,"angle":0,"feather":50,"invert":false}),
        json!({"kind":"colorRange","samples":[],"refine":50}),
        json!({"kind":"luminanceRange","lo":0,"hi":1,"lo_feather":0,"hi_feather":0}),
        json!({"kind":"depthRange","lo":0,"hi":1,"feather":0}),
        json!({"kind":"subject"}),
        json!({"kind":"sky"}),
        json!({"kind":"background"}),
        json!({"kind":"object","hint":[]}),
        json!({"kind":"prompt","text":"sky"}),
        json!({"kind":"people","person":0,"parts":[]}),
        json!({"kind":"landscape","class":"sky"}),
    ] {
        let shape = serde_json::from_value(shape).unwrap();
        let name = icons::mask_name(&shape);
        assert!(color_icons::exists(name));
        assert!(distinct.insert(name), "different masks share {name}");
    }
}

pub(super) fn saturation(image: &egui::ColorImage, rect: Rect, scale: f32) -> usize {
    let min = rect.min * scale;
    let max = rect.max * scale;
    (min.y.max(0.0) as usize..(max.y as usize).min(image.size[1]))
        .flat_map(|y| (min.x.max(0.0) as usize..(max.x as usize).min(image.size[0])).map(move |x| image.pixels[y * image.size[0] + x]))
        .filter(|p| p.r().max(p.g()).max(p.b()) - p.r().min(p.g()).min(p.b()) > 40)
        .count()
}

fn rect(h: &Headless, id: &str) -> Rect {
    h.app.widgets.iter().rev().find(|(name, _)| name == id).unwrap().1
}

#[test]
fn settings_click_switches_tool_strip_pixels_and_round_trips_at_both_scales() {
    for scale in [1.0, 2.0] {
        let mut app = LightcraftApp::new(lightcraft_engine::Session::with_demo(), Default::default());
        app.ui.language = crate::i18n::Locale::En;
        let mut h = Headless::new(app, [1300.0, 820.0], scale);
        let timeout = Duration::from_secs(10);
        for _ in 0..4 {
            h.step();
        }
        let click = h.request("ui.clickWidget", json!({"id":"icon:crop"}), timeout);
        assert_eq!(click["ok"], true, "{click}");
        assert_eq!(h.app.ui.right, RightPanel::Crop);
        let crop = rect(&h, "icon:crop");
        let count = saturation(&h.paint(), crop, scale);
        assert!(count > (50.0 * scale * scale) as usize, "no colour crop pixels: {count}");
        h.request("ui.key", json!({"key":",", "cmd":true}), timeout);
        h.request("ui.clickWidget", json!({"id":"button:settingsTab-interface"}), timeout);
        let click = h.request("ui.clickWidget", json!({"id":"button:settingsToolIcons-1"}), timeout);
        assert_eq!(click["ok"], true, "{click}");
        assert!(!h.app.ui.settings.color_tool_icons);
        h.request("ui.key", json!({"key":"Escape"}), timeout);
        for _ in 0..3 {
            h.step();
        }
        assert_eq!(saturation(&h.paint(), rect(&h, "icon:crop"), scale), 0);
        let saved = serde_json::to_string(&h.app.ui.settings).unwrap();
        let restored: crate::state::AppSettings = serde_json::from_str(&saved).unwrap();
        assert!(!restored.color_tool_icons);
        let old: crate::state::AppSettings = serde_json::from_str("{}").unwrap();
        assert!(old.color_tool_icons);
        h.request("ui.key", json!({"key":",", "cmd":true}), timeout);
        h.request("ui.clickWidget", json!({"id":"button:settingsTab-interface"}), timeout);
        h.request("ui.clickWidget", json!({"id":"button:settingsToolIcons-0"}), timeout);
        h.request("ui.key", json!({"key":"Escape"}), timeout);
        for _ in 0..3 {
            h.step();
        }
        assert!(h.app.ui.settings.color_tool_icons);
        assert!(saturation(&h.paint(), rect(&h, "icon:crop"), scale) >= count / 2);
    }
}

#[test]
fn disabled_tool_button_is_dim_grey_and_cannot_be_clicked() {
    let mut view = crate::headless::HeadlessView::new();
    let mut clicked = false;
    let mut button = Rect::NOTHING;
    for i in 0..4 {
        let events = if i > 1 {
            vec![
                egui::Event::PointerMoved(button.center()),
                egui::Event::PointerButton {
                    pos: button.center(),
                    button: egui::PointerButton::Primary,
                    pressed: i == 2,
                    modifiers: Default::default(),
                },
            ]
        } else {
            vec![]
        };
        view.run(crate::headless::HeadlessView::raw_input(egui::vec2(64.0, 64.0), 1.0, i as f64 / 60.0, events), |ui| {
            ui.painter().rect_filled(ui.max_rect(), 0.0, Color32::from_gray(31));
            let response = crate::widgets::tool_button(ui, "crop", icons::Icon::Crop, egui::vec2(40.0, 40.0), false, false, "Crop");
            clicked |= response.clicked();
            button = response.rect;
        });
    }
    let image = view.paint(&Default::default());
    assert_eq!(saturation(&image, button, 1.0), 0);
    assert!(image.pixels.iter().any(|p| p.r() > 50));
    assert!(!clicked);
}

#[test]
fn retouch_icons_and_mask_tiles_select_their_tools_by_click() {
    let app = LightcraftApp::new(lightcraft_engine::Session::with_demo(), Default::default());
    let mut h = Headless::new(app, [1300.0, 820.0], 1.0);
    let timeout = Duration::from_secs(10);
    for _ in 0..4 {
        h.step();
    }
    let click = h.request("ui.clickWidget", json!({"id":"icon:remove"}), timeout);
    assert_eq!(click["ok"], true, "{click}");
    for tool in ["heal", "clone", "remove", "ai"] {
        let click = h.request("ui.clickWidget", json!({"id":format!("icon:removeMode-{tool}")}), timeout);
        assert_eq!(click["ok"], true, "{tool}: {click}");
        assert_eq!(h.app.ui.tool, tool);
    }
    assert_eq!(h.app.widgets.iter().filter(|(name, _)| name == "icon:remove").count(), 1, "strip control keeps its unique ID");
    let overlay = h.app.ui.remove_overlay;
    h.request("ui.clickWidget", json!({"id":"icon:remove"}), timeout);
    assert_eq!(h.app.ui.right, RightPanel::Remove, "strip keeps the retouch panel open");
    assert_ne!(h.app.ui.remove_overlay, overlay, "strip still toggles the spot overlay");
    h.request("ui.clickWidget", json!({"id":"icon:masking"}), timeout);
    for tool in ["brush", "linear", "radial"] {
        let click = h.request("ui.clickWidget", json!({"id":format!("maskNew:{tool}")}), timeout);
        assert_eq!(click["ok"], true, "{tool}: {click}");
        assert_eq!(h.app.ui.tool, tool);
    }
}
