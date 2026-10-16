//! Theme choice changes rendering, never available tool actions.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use super::*;
use crate::theme::ThemeKind;
use egui_kittest::{
    Harness,
    kittest::{By, Queryable},
};
use photocraft_doc::{LayerContent, text::TextAlign};

fn fixture(theme: ThemeKind, width: f32, toolbar: bool, ppp: f32) -> Harness<'static, PhotocraftApp> {
    let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), Default::default());
    app.run("file.new", json!({"width": 64, "height": 64})).unwrap();
    app.ui.theme = theme;
    let height = if width < 1000.0 { 480.0 } else { 900.0 };
    let mut h = Harness::builder().with_size(vec2(width, height)).with_pixels_per_point(ppp).build_ui_state(
        move |ui, app| {
            if ui.ctx().fonts(|fonts| fonts.families().contains(&egui::FontFamily::Name("medium".into()))) {
                options_bar(app, ui);
                if toolbar {
                    super::toolbar(app, ui);
                }
            }
        },
        app,
    );
    PhotocraftApp::setup_context(&h.ctx, theme);
    h.run_steps(4);
    h
}
fn select(h: &mut Harness<'_, PhotocraftApp>, tool: Tool) {
    h.state_mut().ui.tool = tool;
    h.run_steps(3);
}
fn viewport(h: &Harness<'_, PhotocraftApp>) -> Rect {
    h.ctx.content_rect()
}
fn options_clip(h: &Harness<'_, PhotocraftApp>) -> Rect {
    h.ctx.data(|data| data.get_temp(egui::Id::new(("theme-options-clip", h.state().ui.tool)))).unwrap()
}
/// Reach a real painted/hittable rectangle using the visible overflow controls.
/// Merely retaining an off-screen accessibility node does not satisfy this test.
fn click_option(h: &mut Harness<'_, PhotocraftApp>, label: &str) {
    click_option_matching(h, label, By::new().label(label));
}
fn click_option_matching(h: &mut Harness<'_, PhotocraftApp>, label: &str, by: By<'_>) {
    for _ in 0..8 {
        let clip = options_clip(h).intersect(viewport(h));
        if let Some(node) = h.query(by.clone()) {
            let rect = node.rect();
            if rect.width() > 1.0 && rect.height() > 1.0 && clip.contains_rect(rect) {
                node.click();
                h.run_steps(3);
                return;
            }
            assert!(
                rect.top() >= clip.top() && rect.bottom() <= clip.bottom(),
                "{:?}/{:?} at {} px/pt: {label} vertically clipped at {rect:?} by {clip:?}",
                h.state().ui.theme,
                h.state().ui.tool,
                h.ctx.pixels_per_point()
            );
            if rect.left() < clip.left() {
                h.get_by_label("Previous tool options").click();
                h.run_steps(3);
                continue;
            }
        }
        let next = h.get_by_label("More tool options");
        assert!(viewport(h).contains_rect(next.rect()), "overflow control must itself remain visible");
        next.click();
        h.run_steps(3);
    }
    panic!(
        "{:?}/{:?} in {:?} at {} px/pt: {label} has no fully visible actionable rectangle; node {:?}, clip {:?}",
        h.state().ui.theme,
        h.state().ui.tool,
        viewport(h),
        h.ctx.pixels_per_point(),
        h.query(by).map(|node| node.rect()),
        options_clip(h)
    );
}
#[test]
fn theme_parity_selection_bucket_gradient_crop_and_move_actions() {
    for theme in ThemeKind::ALL {
        for (width, ppp) in [(1440.0, 1.0), (760.0, 1.0), (760.0, 1.25), (760.0, 2.0)] {
            let mut h = fixture(theme, width, false, ppp);
            select(&mut h, Tool::RectMarquee);
            let add = li_i18n::fmt("Add to selection  ({key})", &[("key", &crate::shortcuts::pretty("Shift"))]);
            click_option(&mut h, &add);
            assert_eq!(h.state().ui.selection_mode, 1);
            click_option_matching(&mut h, "Normal", By::new().role(egui::accesskit::Role::ComboBox).value("Normal"));
            h.get_by_label("Fixed Ratio").click();
            h.run_steps(3);
            assert_eq!(h.state().ui.tool_options.marquee_style, "fixedRatio");
            for tool in [Tool::EllipseMarquee, Tool::Lasso, Tool::PolygonLasso] {
                select(&mut h, tool);
                let before = h.state().ui.tool_options.anti_alias;
                click_option(&mut h, "Anti-alias");
                assert_ne!(h.state().ui.tool_options.anti_alias, before, "{theme:?}/{tool:?}");
            }
            select(&mut h, Tool::MagneticLasso);
            let before = h.state().ui.tool_options.magnetic_pressure;
            click_option(&mut h, "Use tablet pressure to change pen width");
            assert_ne!(h.state().ui.tool_options.magnetic_pressure, before);
            select(&mut h, Tool::PaintBucket);
            let before = h.state().ui.tool_options.sample_all_layers;
            click_option(&mut h, "All Layers");
            assert_ne!(h.state().ui.tool_options.sample_all_layers, before);
            select(&mut h, Tool::Gradient);
            let before = h.state().ui.tool_options.gradient_dither;
            click_option(&mut h, "Dither");
            assert_ne!(h.state().ui.tool_options.gradient_dither, before);
            select(&mut h, Tool::Crop);
            let before = h.state().ui.tool_options.crop_delete;
            click_option(&mut h, "Delete Cropped Pixels");
            assert_ne!(h.state().ui.tool_options.crop_delete, before);
            h.state_mut().ui.crop_rect = Some([3., 4., 20., 18.]);
            click_option(&mut h, "Cancel current crop operation  (Esc)");
            assert!(h.state().ui.crop_rect.is_none());
            select(&mut h, Tool::Move);
            let before = h.state().ui.tool_options.move_show_transform;
            click_option(&mut h, "Show Transform Controls");
            assert_ne!(h.state().ui.tool_options.move_show_transform, before);
            let before = h.state().ui.tool_options.move_auto_select;
            click_option(&mut h, "Auto-Select:");
            assert_ne!(h.state().ui.tool_options.move_auto_select, before);
            let panel: Rect = h.ctx.data(|data| data.get_temp(egui::Id::new("theme-options-panel"))).unwrap();
            // The panel includes its separator; the painted control clip is its interior.
            assert_eq!(panel.height(), if matches!(theme, ThemeKind::Pro | ThemeKind::ProMedium) { 36.0 } else { 42.0 });
            assert!(panel.contains_rect(options_clip(&h)));
        }
    }
}
#[test]
fn theme_parity_type_and_brush_controls_edit_real_state() {
    for theme in ThemeKind::ALL {
        for (width, ppp) in [(1440.0, 1.0), (760.0, 1.0), (760.0, 1.25), (760.0, 2.0)] {
            let mut h = fixture(theme, width, false, ppp);
            let id = h.state_mut().run("type.create", json!({"text":"Theme parity", "x":10, "y":30})).unwrap()["layer"].as_u64().unwrap();
            for (tool, label, expected) in [(Tool::Type, "Center text", TextAlign::Center), (Tool::VerticalType, "Right align text", TextAlign::Right)] {
                select(&mut h, tool);
                click_option(&mut h, label);
                let LayerContent::Text(text) = &h.state().session.active().unwrap().doc.layer(photocraft_doc::LayerId(id)).unwrap().content else {
                    panic!("text")
                };
                assert_eq!(text.paragraph_runs()[0].style.align, expected, "{theme:?}/{tool:?}");
            }
            select(&mut h, Tool::Brush);
            let before = h.state().session.tools.brush.build_up;
            click_option(&mut h, "Enable airbrush-style build-up effects");
            assert_ne!(h.state().session.tools.brush.build_up, before);
            select(&mut h, Tool::Pencil);
            let before = h.state().ui.tool_options.pencil_auto_erase;
            click_option(&mut h, "Auto Erase");
            assert_ne!(h.state().ui.tool_options.pencil_auto_erase, before);
        }
    }
}
fn click_footer(h: &mut Harness<'_, PhotocraftApp>, label: &str) {
    let clip: Rect = h.ctx.data(|data| data.get_temp(egui::Id::new("theme-toolbar-clip"))).unwrap();
    let node = h.get_by_label(label);
    assert!(clip.intersect(viewport(h)).contains_rect(node.rect()), "{:?}: {label} clipped at {:?} by {clip:?}", h.state().ui.theme, node.rect());
    node.click();
    h.run_steps(3);
}
#[test]
fn theme_parity_toolbar_footer_actions_stay_visible_and_work() {
    for theme in ThemeKind::ALL {
        for (width, ppp) in [(1440.0, 1.0), (760.0, 1.0), (760.0, 1.25), (760.0, 2.0)] {
            let mut h = fixture(theme, width, true, ppp);
            click_footer(&mut h, "Edit in Quick Mask Mode  (Q)");
            assert!(h.state().session.active().unwrap().doc.quick_mask.is_some());
            click_footer(&mut h, "Edit in Standard Mode  (Q)");
            assert!(h.state().session.active().unwrap().doc.quick_mask.is_none());
            click_footer(&mut h, "Change Screen Mode  (F)");
            assert_eq!(h.state().ui.view.screen_mode, "fullScreenWithMenuBar");
            click_footer(&mut h, "Edit Toolbar…");
            assert!(h.state().ui.dialogs.iter().any(|d| d.fields.get("tab") == Some(&json!(2)) && d.fields.contains_key("toolbarOriginal")));
        }
    }
}

#[test]
fn persona_switch_labels_are_painted_and_reachable_outside_tool_scrolling() {
    fn text(shape: &egui::Shape, out: &mut Vec<String>) {
        match shape {
            egui::Shape::Text(t) => out.push(t.galley.job.text.clone()),
            egui::Shape::Vec(shapes) => shapes.iter().for_each(|s| text(s, out)),
            _ => {}
        }
    }
    for theme in ThemeKind::ALL {
        for ppp in [1.0, 1.25, 2.0] {
            let mut h = fixture(theme, 760.0, false, ppp);
            select(&mut h, Tool::Brush);
            h.get_by_label("More tool options").click();
            h.run_steps(3);
            let mut painted = Vec::new();
            for shape in &h.output().shapes {
                text(&shape.shape, &mut painted);
            }
            for label in ["Pixel", "Vector"] {
                assert!(painted.iter().any(|s| s == label), "{theme:?}/{ppp}: visible {label} text");
                let tip = format!("{label} persona");
                let button = h.get_by_label(&tip);
                assert!(viewport(&h).contains_rect(button.rect()));
                assert!(button.rect().right() < options_clip(&h).left(), "persona stays outside scrolling");
            }
            h.get_by_label("Vector persona").click();
            h.run_steps(3);
            assert_eq!(h.state().session.prefs().toolbar.persona, photocraft_engine::toolsets::Persona::Vector);
            h.get_by_label("Pixel persona").click();
            h.run_steps(3);
            assert_eq!(h.state().session.prefs().toolbar.persona, photocraft_engine::toolsets::Persona::Pixel);
        }
    }
}
