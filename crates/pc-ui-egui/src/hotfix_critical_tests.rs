//! Headless regressions from the f73bc0ca test build.
#![allow(clippy::unwrap_used)]
use crate::{
    PhotocraftApp,
    state::{TextEdit, Tool},
};
use egui::{Key, Modifiers, vec2};
use egui_kittest::{Harness, kittest::Queryable};
use photocraft_engine::{Session, toolsets::Persona, vector_toolbar_layout::Preset};
use serde_json::json;

fn app() -> PhotocraftApp {
    let mut app = PhotocraftApp::new(Session::new(), Default::default());
    app.run("file.new", json!({"width": 200, "height": 100})).unwrap();
    app
}
#[test]
fn hotfix_options_bar_controls_are_inside_the_visible_row() {
    for persona in [Persona::Pixel, Persona::Vector] {
        for theme in [crate::theme::ThemeKind::default(), crate::theme::ThemeKind::Studio] {
            let mut app = app();
            app.ui.tool = if persona == Persona::Pixel { Tool::Brush } else { Tool::Reshape };
            app.vector_transform_tools.kind = crate::vector_transform_tools::Kind::Reshape;
            app.session.prefs.edit(|p| p.toolbar.persona = persona);
            let mut h = Harness::builder().with_size(vec2(1400., 90.)).build_ui_state(
                |ui, app: &mut PhotocraftApp| {
                    if ui.ctx().fonts(|f| f.families().contains(&egui::FontFamily::Name("medium".into()))) {
                        crate::panels::options_bar(app, ui);
                    }
                },
                app,
            );
            PhotocraftApp::setup_context(&h.ctx, theme);
            h.run_steps(4);
            let label =
                if persona == Persona::Pixel { if theme == crate::theme::ThemeKind::Studio { "Opacity" } else { "Opacity:" } } else { "Falloff radius" };
            let control = h.get_by_label(label).rect();
            assert_eq!(h.state().session.prefs().toolbar.persona, persona);
            assert!(egui::Rect::from_min_size(egui::Pos2::ZERO, vec2(1400., 60.)).contains_rect(control), "{persona:?}/{theme:?}: {control:?}");
        }
    }
}
#[test]
fn hotfix_inline_vector_text_owns_tool_and_paint_shortcut_letters() {
    for persona in [Persona::Pixel, Persona::Vector] {
        for preset in [Preset::Illustrator, Preset::Affinity] {
            let mut app = app();
            app.session.prefs.edit(|p| {
                p.toolbar.persona = persona;
                p.toolbar.vector_keyboard.preset = preset;
            });
            let id = app.run("type.create", json!({"text": "", "x": 20, "y": 40})).unwrap()["layer"].as_u64().unwrap();
            let tool = if persona == Persona::Vector { Tool::ArtText } else { Tool::Type };
            app.ui.tool = tool;
            app.ui.text_edit =
                Some(TextEdit { layer: id, caret: 0, anchor: 0, session: "hotfix".into(), created: false, dragging: false, resize: None, preedit: None });
            let mut h = Harness::builder().build_ui_state(|ui, app: &mut PhotocraftApp| crate::shortcuts::handle(app, ui.ctx()), app);
            let paint = h.state().session.tools.brush.clone();
            for (key, text, mods) in [
                (Key::O, "O", Modifiers::NONE),
                (Key::K, "K", Modifiers::NONE),
                (Key::H, "H", Modifiers::NONE),
                (Key::X, "X", Modifiers::NONE),
                (Key::D, "D", Modifiers::NONE),
                (Key::Slash, "/", Modifiers::NONE),
                (Key::D, "D", Modifiers::SHIFT),
            ] {
                h.event(egui::Event::Key { key, physical_key: None, pressed: true, repeat: false, modifiers: mods });
                h.event(egui::Event::Text(text.into()));
                h.run_steps(1);
                assert_eq!(h.state().ui.tool, tool, "{persona:?}/{preset:?}: {text}");
                assert!(h.state().ui.text_edit.is_some());
                h.event(egui::Event::Key { key, physical_key: None, pressed: false, repeat: false, modifiers: mods });
                h.run_steps(1);
            }
            let doc = &h.state().session.active().unwrap().doc;
            let photocraft_doc::LayerContent::Text(t) = &doc.layer(photocraft_doc::LayerId(id)).unwrap().content else { panic!("text layer") };
            assert_eq!(t.text, "OKHXD/D");
            assert_eq!(h.state().session.tools.brush, paint);
            h.key_press(Key::Escape);
            h.run_steps(1);
            assert!(h.state().ui.text_edit.is_none());
        }
    }
}
