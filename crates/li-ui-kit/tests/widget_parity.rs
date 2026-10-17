//! Widget shape goldens plus genuine pointer/keyboard interactions.
use egui::{Color32, Rect, Sense, Vec2, vec2};
use li_ui_kit::{
    fonts, library, theme,
    widgets::{self, Profile, SelectionStyle},
};

fn gallery(ui: &mut egui::Ui, profile: Profile) {
    match profile {
        Profile::Editor => {
            widgets::heading(ui, "BASIC", profile, false);
            widgets::selection(ui, "Selected", true, SelectionStyle::Pill, None);
            widgets::selection(ui, "Other", false, SelectionStyle::Pill, None);
            widgets::primary_button(ui, "Apply", 70.0);
            widgets::secondary_button(ui, "Cancel", 70.0);
            widgets::checkbox(ui, &mut true, "Enabled", profile);
            widgets::checkbox(ui, &mut false, "Disabled", profile);
            widgets::separator(ui, profile, None);
            widgets::separator(ui, profile, Some(20.0));
            for color in [Color32::RED, Color32::from_rgba_unmultiplied(180, 90, 40, 70)] {
                let (r, _) = ui.allocate_exact_size(Vec2::splat(24.0), Sense::hover());
                widgets::color_tile(ui, r, color, profile);
            }
        }
        Profile::Library => {
            widgets::selection(ui, "Auto", false, SelectionStyle::Text, None);
            widgets::selection(ui, "B&W", true, SelectionStyle::Text, None);
            let width = ((ui.available_width() - 4.0) / 2.0).max(24.0);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                widgets::selection(ui, "One", true, SelectionStyle::Segment, Some(vec2(width, 24.0)));
                widgets::selection(ui, "Two", false, SelectionStyle::Segment, Some(vec2(width, 24.0)));
            });
            widgets::separator(ui, profile, None);
            widgets::choice_button(
                ui,
                "Long camera profile name elided at narrow width",
                egui::FontId::proportional(12.0),
                library::get(ui.ctx()).text,
                paint_arrow,
            );
            let (r, _) = ui.allocate_exact_size(Vec2::splat(24.0), Sense::hover());
            widgets::color_tile(ui, r, Color32::RED, profile);
        }
    }
}
fn paint_arrow(ui: &egui::Ui, rect: Rect, color: Color32) {
    let side = rect.width().min(rect.height());
    let ppp = ui.ctx().pixels_per_point();
    let px = (side * ppp).round().clamp(1.0, 512.0) as u32;
    if let Some(tex) = li_ui_kit::icons::texture(
        ui.ctx(),
        li_ui_kit::library_icons::svg(li_ui_kit::library_icons::Icon::ChevronDown),
        li_ui_kit::icons::StrokeStyle::Interface,
        px,
        Color32::WHITE,
        false,
    ) {
        ui.painter().image(
            tex,
            Rect::from_center_size(rect.center(), Vec2::splat(px as f32 / ppp)),
            Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
            color,
        );
    }
}
fn shapes(profile: Profile, kind: theme::ThemeKind, width: f32) -> String {
    let ctx = egui::Context::default();
    ctx.set_fonts(fonts::definitions(fonts::Profile::Editor));
    theme::apply(&ctx, kind);
    library::apply_tokens(&ctx, library::for_kind(kind));
    let mut out = String::new();
    for _ in 0..3 {
        let mut output = ctx
            .run_ui(egui::RawInput { screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, vec2(width, 600.0))), ..Default::default() }, |ui| {
                gallery(ui, profile)
            });
        out = format!("{:?}", output.shapes);
        output.textures_delta.clear();
    }
    out
}
/// FNV-1a of the painted shapes' Debug output for each theme, width and profile. Captured from the
/// frozen pre-consolidation (J18) Editor/Library painters, which the shared widgets reproduced
/// exactly; a mismatch is a visual change that needs explicit review.
const SHAPE_GOLDENS: &[(theme::ThemeKind, f32, Profile, u64)] = &[
    (theme::ThemeKind::Pro, 180.0, Profile::Editor, 0x30664f8b638c5108),
    (theme::ThemeKind::Pro, 180.0, Profile::Library, 0xebe13d7980f92e83),
    (theme::ThemeKind::Pro, 270.0, Profile::Editor, 0xf7c8ec826361c4b0),
    (theme::ThemeKind::Pro, 270.0, Profile::Library, 0x21879f781e285050),
    (theme::ThemeKind::ProMedium, 180.0, Profile::Editor, 0x3dea42616dcc97f1),
    (theme::ThemeKind::ProMedium, 180.0, Profile::Library, 0xf22202ec21811379),
    (theme::ThemeKind::ProMedium, 270.0, Profile::Editor, 0x3fcf4c5952ce9669),
    (theme::ThemeKind::ProMedium, 270.0, Profile::Library, 0x20627cb633f32784),
    (theme::ThemeKind::Studio, 180.0, Profile::Editor, 0x070bf7e80694a767),
    (theme::ThemeKind::Studio, 180.0, Profile::Library, 0x36203faae2c65e45),
    (theme::ThemeKind::Studio, 270.0, Profile::Editor, 0x1bf784f44d0b5291),
    (theme::ThemeKind::Studio, 270.0, Profile::Library, 0x4346c004644dc954),
    (theme::ThemeKind::StudioLight, 180.0, Profile::Editor, 0xcb9affbb7f58e12d),
    (theme::ThemeKind::StudioLight, 180.0, Profile::Library, 0xc9d4d36a08503837),
    (theme::ThemeKind::StudioLight, 270.0, Profile::Editor, 0xff57e9d52e7a54a3),
    (theme::ThemeKind::StudioLight, 270.0, Profile::Library, 0x4a3a69e02bc6dbec),
    (theme::ThemeKind::Classic, 180.0, Profile::Editor, 0xab5b180b7db706d3),
    (theme::ThemeKind::Classic, 180.0, Profile::Library, 0x64ea6cfe77be0fc9),
    (theme::ThemeKind::Classic, 270.0, Profile::Editor, 0xba3bb361cc462471),
    (theme::ThemeKind::Classic, 270.0, Profile::Library, 0xe44dd8f1f18f3432),
];

#[test]
fn j18_widget_shapes_match_goldens_for_all_themes_and_widths() {
    let fnv1a = |s: String| s.bytes().fold(0xcbf29ce484222325_u64, |h, b| (h ^ u64::from(b)).wrapping_mul(0x100000001b3));
    assert_eq!(SHAPE_GOLDENS.len(), theme::ThemeKind::ALL.len() * 4, "one golden per theme, width and profile");
    for &(kind, width, profile, golden) in SHAPE_GOLDENS {
        assert_eq!(fnv1a(shapes(profile, kind, width)), golden, "{kind:?} {profile:?} width{width}");
    }
}

#[test]
fn j18_widget_checkbox_selection_and_menu_receive_real_clicks() {
    for profile in [Profile::Editor, Profile::Library] {
        let ctx = egui::Context::default();
        ctx.set_fonts(fonts::definitions(fonts::Profile::Editor));
        theme::apply(&ctx, theme::ThemeKind::ProMedium);
        library::apply_tokens(&ctx, library::for_kind(theme::ThemeKind::ProMedium));
        let mut checked = false;
        let mut selected = false;
        let mut choice = false;
        let mut rects = [Rect::ZERO; 4];
        let mut frame = |events: Vec<egui::Event>| {
            ctx.run_ui(egui::RawInput { screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, vec2(270.0, 400.0))), events, ..Default::default() }, |ui| {
                rects[0] = widgets::checkbox(ui, &mut checked, "Enable", profile).rect;
                let r =
                    widgets::selection(ui, "Select", selected, if profile == Profile::Editor { SelectionStyle::Pill } else { SelectionStyle::Segment }, None);
                rects[1] = r.rect;
                if r.clicked() {
                    selected = !selected;
                }
                let r = widgets::choice_button(ui, "Choose", egui::FontId::proportional(12.0), library::get(ui.ctx()).text, paint_arrow);
                rects[2] = r.rect;
                egui::Popup::menu(&r).show(|ui| {
                    let r = ui.selectable_label(choice, "Chosen");
                    rects[3] = r.rect;
                    if r.clicked() {
                        choice = true;
                    }
                });
            })
            .textures_delta
            .clear();
            rects
        };
        let rects = frame(vec![]);
        frame(vec![]);
        for r in &rects[..3] {
            let at = r.center();
            frame(vec![egui::Event::PointerMoved(at)]);
            for pressed in [true, false] {
                frame(vec![egui::Event::PointerButton { pos: at, button: egui::PointerButton::Primary, pressed, modifiers: egui::Modifiers::NONE }]);
            }
        }
        let popup = frame(vec![])[3];
        assert!(popup.is_positive());
        let at = popup.center();
        frame(vec![egui::Event::PointerMoved(at)]);
        for pressed in [true, false] {
            frame(vec![egui::Event::PointerButton { pos: at, button: egui::PointerButton::Primary, pressed, modifiers: egui::Modifiers::NONE }]);
        }
        assert!(checked);
        assert!(selected);
        assert!(choice);
    }
}

#[test]
fn j18_widget_header_enable_and_alpha_swatch_receive_real_clicks() {
    for kind in [theme::ThemeKind::ProMedium, theme::ThemeKind::StudioLight] {
        for width in [180.0, 270.0] {
            let ctx = egui::Context::default();
            ctx.set_fonts(fonts::definitions(fonts::Profile::Editor));
            library::apply_tokens(&ctx, library::for_kind(kind));
            let mut enabled = false;
            let mut selected = false;
            let mut frame = |events| {
                let mut swatch = Rect::ZERO;
                let mut output = ctx.run_ui(
                    egui::RawInput { screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, vec2(width, 200.0))), events, ..Default::default() },
                    |ui| {
                        let (_, toggled) =
                            widgets::library_header(ui, "basic", "Basic", true, Some(enabled), |ui, rect, _, color| paint_arrow(ui, rect, color));
                        if let Some(value) = toggled {
                            enabled = value;
                        }
                        let response =
                            widgets::color_swatch(ui, "swatch", Color32::from_rgba_unmultiplied(180, 80, 30, 90), Vec2::splat(24.0), Some(selected), "Colour");
                        swatch = response.rect;
                        if response.clicked() {
                            selected = !selected;
                        }
                    },
                );
                // A translucent tile must paint a checker under its content/outline.
                assert!(output.shapes.len() > 5);
                output.textures_delta.clear();
                (widgets::take_registry(&ctx), swatch)
            };
            let (registry, swatch) = frame(vec![]);
            let eye = registry.iter().find(|(id, _)| id == "sectionEye:basic").unwrap().1;
            for at in [eye.center(), swatch.center()] {
                frame(vec![egui::Event::PointerMoved(at)]);
                for pressed in [true, false] {
                    frame(vec![egui::Event::PointerButton { pos: at, button: egui::PointerButton::Primary, pressed, modifiers: egui::Modifiers::NONE }]);
                }
            }
            assert!(enabled && selected);
        }
    }
}
