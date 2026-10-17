//! Real-input coverage of Lightroom Classic order and names; legacy command/widget IDs remain.
use crate::{
    LightcraftApp, Services,
    headless::Headless,
    state::{RightPanel, ViewMode},
};
use egui::{Rect, epaint::Shape};
use serde_json::json;
use std::time::Duration;
const WAIT: Duration = Duration::from_secs(20);
fn demo(width: f32, dark: bool, scale: f32) -> Headless {
    let mut app = LightcraftApp::new(lightcraft_engine::Session::with_demo(), Services::default());
    app.ui.settings.gpu = false;
    app.ui.view = ViewMode::Detail;
    app.ui.right = RightPanel::Edit;
    app.ui.histogram = false;
    app.ui.filmstrip = false;
    app.ui.left_panel = false;
    app.ui.right_width = width;
    app.set_host_theme(if dark { Default::default() } else { super::tests_develop_c::light() });
    let mut h = Headless::new(app, [1200.0, 1500.0], scale);
    h.step();
    h.step();
    h
}
fn rect(h: &Headless, id: &str) -> Rect {
    h.app.widgets.iter().rev().find(|(name, _)| name == id).unwrap_or_else(|| panic!("missing {id}")).1
}
fn click(h: &mut Headless, id: &str) {
    let r = h.request("ui.clickWidget", json!({"id":id}), WAIT);
    assert_eq!(r["ok"], true, "{id}: {r}");
    h.step();
}
fn texts(h: &Headless) -> String {
    fn collect(shape: &Shape, output: &mut String) {
        match shape {
            Shape::Text(text) => {
                output.push_str(text.galley.text());
                output.push('\n');
            }
            Shape::Vec(children) => {
                for shape in children {
                    collect(shape, output);
                }
            }
            _ => {}
        }
    }
    let mut output = String::new();
    for shape in h.view.shapes() {
        collect(&shape.shape, &mut output);
    }
    output
}
#[test]
fn lrc_basic_white_balance_precedes_tone_and_optional_negative_follows_optics() {
    for dark in [true, false] {
        for width in [250.0, 400.0] {
            for scale in [1.0, 1.5] {
                let mut h = demo(width, dark, scale);
                assert!(texts(&h).contains("Basic"));
                assert!(rect(&h, "slider:wb.tempRel").bottom() < rect(&h, "slider:light.exposure").top());
                assert!(rect(&h, "slider:wb.tintRel").bottom() < rect(&h, "slider:light.exposure").top());
                assert!(!h.app.widgets.iter().any(|(id, _)| id == "section:negative"));
                click(&mut h, "icon:wbPicker");
                assert_eq!(h.app.ui.tool, "wbPicker");
                click(&mut h, "icon:wbPicker");
                assert!(h.app.ui.tool.is_empty());
                let before = h.app.session.develop_of(h.app.session.active().unwrap()).unwrap().wb.temp;
                let r = h.request("ui.dragWidget", json!({"id":"slider:wb.tempRel","dx":25.0,"dy":0.0,"steps":4}), WAIT);
                assert_eq!(r["ok"], true, "{r}");
                h.step();
                assert_ne!(h.app.session.develop_of(h.app.session.active().unwrap()).unwrap().wb.temp, before);
                click(&mut h, "section:light");
                assert!(!h.app.widgets.iter().any(|(id, _)| id == "icon:wbPicker"));
                click(&mut h, "button:addTool");
                click(&mut h, "button:addNegative");
                assert!(h.app.ui.negative_tool_visible);
                assert!(rect(&h, "section:negative").top() > rect(&h, "section:optics").bottom());
                assert!(
                    !h.app.session.develop_of(h.app.session.active().unwrap()).unwrap().negative.enabled,
                    "revealing a tool does not change settings"
                );
                click(&mut h, "negative:enabled");
                assert!(h.app.session.develop_of(h.app.session.active().unwrap()).unwrap().negative.enabled);
                h.app.ui.negative_tool_visible = false;
                h.step();
                assert!(h.app.widgets.iter().any(|(id, _)| id == "section:negative"), "saved conversion always appears");
                let old: crate::UiState = serde_json::from_value(json!({"openSections":["light","color"]})).unwrap();
                assert!(!old.negative_tool_visible);
            }
        }
    }
}
#[test]
fn lrc_catalog_collections_and_loupe_keep_legacy_navigation_ids() {
    let mut h = demo(400.0, true, 1.0);
    h.app.ui.view = ViewMode::PhotoGrid;
    h.app.ui.right = RightPanel::None;
    h.app.ui.left_panel = true;
    h.step();
    h.step();
    let text = texts(&h);
    assert!(text.contains("Catalog"));
    assert!(text.contains("Collections"));
    assert!(!text.contains("My Photos"));
    assert!(!text.contains("Albums"));
    click(&mut h, "sidebarSection:albums");
    assert!(h.app.ui.sidebar_section_collapsed("albums"));
    click(&mut h, "sidebarSection:albums");
    assert!(!h.app.ui.sidebar_section_collapsed("albums"));
    click(&mut h, "icon:detail");
    assert_eq!(h.app.ui.view, ViewMode::Detail);
    assert_eq!(crate::menus::ui_commands().find(|(id, _, _, _)| *id == "view.detail").unwrap().1, "Loupe");
    assert!(crate::menus::menu_entries(&h.app).iter().all(|entry| !entry.label.contains("Album")));
    assert_eq!(crate::icons::svg(crate::icons::Icon::Collection), include_bytes!("../../../assets/icons/images.svg").as_slice());
}

#[test]
#[ignore = "offscreen Preferences fallback screenshots"]
fn lrc_standalone_preferences_screens() {
    let mut h = demo(400.0, true, 1.0);
    h.size = egui::vec2(1300.0, 820.0);
    h.request("ui.key", json!({"key":",","cmd":true}), WAIT);
    for _ in 0..4 {
        h.step();
    }
    eprintln!("tabs: {:?}", h.app.widgets.iter().filter(|(id, _)| id.contains("settingsTab")).collect::<Vec<_>>());
    let image = h.paint();
    let rgba = li_raster::Rgba8::from_fn(image.size[0], image.size[1], |x, y| image.pixels[y * image.size[0] + x].to_array());
    let png = lightcraft_codecs::encode_png(&lightcraft_codecs::EncodeImage::rgba8(&rgba), &Default::default()).unwrap();
    let out = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/lrc-ux");
    std::fs::create_dir_all(&out).unwrap();
    std::fs::write(out.join("after-standalone-preferences.png"), png).unwrap();
    click(&mut h, "button:settingsTab-interface");
    assert!(matches!(&h.app.ui.dialog,Some(crate::state::Dialog::Settings {tab}) if tab=="interface"));
}
