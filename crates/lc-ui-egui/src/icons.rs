//! Library/Develop interface icons: Lucide 0.468.0 line SVGs (ISC) and the shared tool icons
//! (assets/icons, provenance in assets/icons/SOURCES.md), rasterized white at the physical pixel size
//! and tinted per use (Smart Collection: Lucide `book` with a Local Image funnel). Tool controls use the
//! shared monochrome tool icons. The LightCraft code-painted glyphs were retired on 2026-10-13.

#[cfg(test)]
use egui::vec2;
use egui::{Color32, Painter, Rect, Vec2, pos2};

#[cfg(test)]
pub(crate) use li_ui_kit::library_icons::ALL;
pub use li_ui_kit::library_icons::Icon;
pub(crate) use li_ui_kit::library_icons::svg;

/// Paint the shared SVG centred at the existing physical-pixel size.
pub fn paint(p: &Painter, rect: Rect, icon: Icon, color: Color32) {
    let side = rect.width().min(rect.height());
    let ppp = p.ctx().pixels_per_point();
    let px = (side * ppp).round().clamp(1.0, 512.0) as u32;
    if let Some(tex) = li_ui_kit::icons::texture(p.ctx(), svg(icon), li_ui_kit::icons::StrokeStyle::Interface, px, Color32::WHITE, false) {
        let r = Rect::from_center_size(rect.center(), Vec2::splat(px as f32 / ppp));
        p.image(tex, r, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), color);
    }
}
/// The tool image for rich tooltips: theme-tinted line icon, or its family tint in colour mode.
pub fn tool_icon(ui: &egui::Ui, name: &str, size: f32) -> Option<egui::Image<'static>> {
    egui_extras::install_image_loaders(ui.ctx());
    if li_ui_kit::color_icons::enabled(ui.ctx()) {
        crate::color_icons::image(ui, name, size, !ui.is_enabled())
    } else {
        let t = crate::theme::Tokens::get(ui.ctx());
        li_ui_kit::mono_tool_icons::image(name, size, if ui.is_enabled() { t.icon } else { t.text_disabled })
    }
}

/// Only tool controls opt into colour; the rest of the interface keeps its vector glyphs.
pub fn paint_tool(ui: &egui::Ui, rect: Rect, name: &str, fallback: Icon, tint: Color32, enabled: bool) {
    egui_extras::install_image_loaders(ui.ctx());
    let size = rect.width().min(rect.height());
    let rect = Rect::from_center_size(rect.center(), egui::vec2(size, size));
    if li_ui_kit::color_icons::enabled(ui.ctx())
        && let Some(image) = crate::color_icons::image(ui, name, size, !enabled || !ui.is_enabled())
    {
        image.paint_at(ui, rect);
    } else if let Some(image) = li_ui_kit::mono_tool_icons::image(name, size, tint) {
        image.paint_at(ui, rect);
    } else {
        paint(ui.painter(), rect, fallback, tint);
    }
}

/// Named tool controls in Develop (the app uses string tools and RightPanel, not a Tool enum).
pub fn tool_name(tool: &str) -> Option<&'static str> {
    li_ui_kit::color_icon_data::DEVELOP_TOOL_ICONS
        .iter()
        .find(|(name, _)| *name == tool)
        .filter(|(_, icon)| li_ui_kit::mono_tool_icons::exists(icon))
        .map(|(_, icon)| *icon)
}

pub fn mask_name(shape: &lightcraft_develop::MaskShape) -> &'static str {
    use lightcraft_develop::MaskShape::*;
    match shape {
        Full => "mask",
        Brush { .. } => "brush",
        Linear { .. } => "linear-mask",
        Radial { .. } => "radial-mask",
        ColorRange { .. } => "color-range",
        LuminanceRange { .. } => "luminance-range",
        DepthRange { .. } => "depth-range",
        Subject { .. } => "subject-mask",
        Sky { .. } => "sky-mask",
        Background { .. } => "background-mask",
        Object { .. } => "object-mask",
        Prompt { .. } => "prompt-mask",
        People { .. } => "people-mask",
        Landscape { .. } => "landscape-mask",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn navigation_arrowheads_have_both_halves() {
        for icon in [Icon::Back, Icon::Forward] {
            for scale in [1.0, 2.0] {
                let mut view = crate::headless::HeadlessView::new();
                let raw = crate::headless::HeadlessView::raw_input(vec2(64.0, 64.0), scale, 0.0, vec![]);
                view.run(raw, |ui| {
                    ui.painter().rect_filled(ui.max_rect(), 0.0, Color32::BLACK);
                    paint(ui.painter(), Rect::from_center_size(pos2(32.0, 32.0), vec2(18.6, 18.6)), icon, Color32::WHITE);
                });
                let image = view.paint(&Default::default());
                let middle = image.pixels.len() / 2;
                let upper: u64 = image.pixels[..middle].iter().map(|p| u64::from(p.r())).sum();
                let lower: u64 = image.pixels[middle..].iter().map(|p| u64::from(p.r())).sum();
                assert!(upper > 0 && lower > 0, "the arrow must render");
                assert!(upper.abs_diff(lower) < upper / 20, "arrowhead must be symmetric at scale {scale}: {upper} above, {lower} below");
            }
        }
    }
}

#[cfg(test)]
#[test]
fn hosted_library_paints_the_shared_icon_style_and_standalone_keeps_its_setting() {
    for hosted in [false, true] {
        for shared_colour in [false, true] {
            let mut app = crate::LightcraftApp::new(lightcraft_engine::Session::new(), Default::default());
            app.hosted_menus = hosted;
            app.fonts_ready = true;
            app.ui.settings.color_tool_icons = !shared_colour;
            let mut view = crate::headless::HeadlessView::new();
            let probe = Rect::from_min_size(pos2(120., 300.), vec2(30., 30.));
            let expected = if hosted { shared_colour } else { !shared_colour };
            for pass in 0..3 {
                let raw = crate::headless::HeadlessView::raw_input(vec2(800., 600.), 1., pass as f64 / 60., vec![]);
                view.run(raw, |ui| {
                    li_ui_kit::color_icons::set_enabled(ui.ctx(), shared_colour);
                    app.ui(ui);
                    assert_eq!(li_ui_kit::color_icons::enabled(ui.ctx()), expected, "the style must be correct before painting a tool");
                    ui.painter().rect_filled(probe, 0., Color32::from_gray(16));
                    paint_tool(ui, probe, "crop", Icon::Crop, crate::theme::Tokens::get(ui.ctx()).icon, true);
                });
            }
            let image = view.paint(&Default::default());
            let chromatic = (probe.top() as usize..probe.bottom() as usize).any(|y| {
                (probe.left() as usize..probe.right() as usize).any(|x| {
                    let pixel = image.pixels[y * image.size[0] + x];
                    pixel.r().abs_diff(pixel.g()).max(pixel.g().abs_diff(pixel.b())).max(pixel.r().abs_diff(pixel.b())) > 10
                })
            });
            assert_eq!(chromatic, expected, "hosted={hosted}, shared_colour={shared_colour}: verify painted pixels, not only context state");
        }
        let mut app = crate::LightcraftApp::new(lightcraft_engine::Session::new(), Default::default());
        app.hosted_menus = hosted;
        let mut view = crate::headless::HeadlessView::new();
        let mut tab = "interface".to_string();
        let mut controls = Vec::new();
        for pass in 0..3 {
            let raw = crate::headless::HeadlessView::raw_input(vec2(800., 900.), 1., pass as f64 / 60., vec![]);
            view.run(raw, |ui| {
                crate::panels::settings::body(&mut app, ui, &mut tab);
                controls = crate::widgets::take_registry(ui.ctx());
            });
        }
        assert_eq!(
            controls.iter().any(|(id, _)| id.starts_with("button:settingsToolIcons-")),
            !hosted,
            "hosted Preferences must not expose a second conflicting tool-icon setting"
        );
    }
}

#[cfg(test)]
#[test]
fn ds_finding_colored_tool_icon_is_square_in_a_wide_click_target() {
    let mut view = crate::headless::HeadlessView::new();
    let wide = Rect::from_min_size(pos2(20., 20.), vec2(100., 30.));
    let raw = crate::headless::HeadlessView::raw_input(vec2(200., 120.), 1., 0., vec![]);
    view.run(raw, |ui| {
        li_ui_kit::color_icons::set_enabled(ui.ctx(), true);
        let _ = ui.interact(wide, egui::Id::new("wide-tool"), egui::Sense::click());
        paint_tool(ui, wide, "crop", Icon::Crop, Color32::WHITE, true);
    });
    let bounds = view
        .shapes()
        .iter()
        .find_map(|s| if let egui::Shape::Rect(r) = &s.shape { r.brush.as_ref().map(|_| r.rect) } else { None })
        .expect("loaded SVG image");
    assert_eq!(bounds, Rect::from_center_size(wide.center(), vec2(30., 30.)));
    let at = wide.center();
    let mut clicked = false;
    for pressed in [true, false] {
        let events = vec![
            egui::Event::PointerMoved(at),
            egui::Event::PointerButton { pos: at, button: egui::PointerButton::Primary, pressed, modifiers: egui::Modifiers::NONE },
        ];
        view.run(crate::headless::HeadlessView::raw_input(vec2(200., 120.), 1., 1., events), |ui| {
            clicked |= ui.interact(wide, egui::Id::new("wide-tool"), egui::Sense::click()).clicked();
            paint_tool(ui, wide, "crop", Icon::Crop, Color32::WHITE, true);
        });
    }
    assert!(clicked);
}

/// Review sheet of every Library/Develop icon at 16/20/40 px on dark and light (`target/icons-ux/`).
/// `LI_ICON_SHEET_TAG` names the file (for example `before` and `after` around an artwork change).
#[cfg(test)]
#[test]
#[ignore = "writes target/icons-ux/ui-icons-<tag>.ppm; run explicitly"]
fn write_ui_icon_review_sheet() {
    let tag = std::env::var("LI_ICON_SHEET_TAG").unwrap_or_else(|_| "after".into());
    let (row_h, label_w) = (52.0, 120.0);
    let size = vec2(label_w + 6.0 * 52.0, ALL.len() as f32 * row_h);
    let mut view = crate::headless::HeadlessView::new();
    for frame in 0..2 {
        view.run(crate::headless::HeadlessView::raw_input(size, 1.0, f64::from(frame), vec![]), |ui| {
            let p = ui.painter();
            p.rect_filled(ui.max_rect(), 0.0, Color32::from_gray(31));
            for (row, icon) in ALL.iter().enumerate() {
                let y = row as f32 * row_h;
                p.text(
                    pos2(4.0, y + row_h / 2.0),
                    egui::Align2::LEFT_CENTER,
                    format!("{icon:?}"),
                    egui::FontId::proportional(11.0),
                    Color32::from_gray(220),
                );
                for (theme, (bg, fg)) in [(31, 222), (242, 52)].into_iter().enumerate() {
                    for (k, px) in [16.0, 20.0, 40.0].into_iter().enumerate() {
                        let cell = Rect::from_min_size(pos2(label_w + (theme * 3 + k) as f32 * 52.0, y), vec2(52.0, row_h));
                        p.rect_filled(cell, 0.0, Color32::from_gray(bg));
                        paint(p, Rect::from_center_size(cell.center(), vec2(px, px)), *icon, Color32::from_gray(fg));
                    }
                }
            }
        });
    }
    let image = view.paint(&Default::default());
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/icons-ux");
    std::fs::create_dir_all(&dir).unwrap();
    // Binary PPM: this crate links no image encoder; the sheet is opaque.
    let mut ppm = format!("P6\n{} {}\n255\n", image.size[0], image.size[1]).into_bytes();
    ppm.extend(image.pixels.iter().flat_map(|p| [p.r(), p.g(), p.b()]));
    std::fs::write(dir.join(format!("ui-icons-{tag}.ppm")), ppm).unwrap();
}

#[cfg(test)]
#[test]
fn j17_library_icon_controls_keep_palette_clicks_and_disabled_behavior() {
    use crate::headless::HeadlessView;
    for kind in li_ui_kit::theme::ThemeKind::ALL {
        for scale in [1.0, 1.25, 2.0] {
            for colored in [false, true] {
                let mut view = HeadlessView::new();
                crate::theme::apply_tokens(&view.ctx, crate::theme::host_palette(kind));
                li_ui_kit::color_icons::set_enabled(&view.ctx, colored);
                let mut bounds = [Rect::NOTHING; 3];
                let mut clicks = [0; 3];
                let mut frame = 0;
                {
                    let mut draw = |ui: &mut egui::Ui| {
                        ui.horizontal(|ui| {
                            for (index, enabled) in [(0, true), (1, true), (2, false)] {
                                let response = if index == 0 {
                                    crate::widgets::icon_button(ui, "j17-nav", Icon::Back, vec2(40., 40.), false, enabled, "Back")
                                } else {
                                    crate::widgets::tool_button(ui, "crop", Icon::Crop, vec2(40., 40.), false, enabled, "Crop")
                                };
                                bounds[index] = response.rect;
                                clicks[index] += usize::from(response.clicked());
                            }
                        });
                    };
                    view.run(HeadlessView::raw_input(vec2(240., 100.), scale, frame as f64 / 60., vec![]), &mut draw);
                }
                for index in 0..3 {
                    for pressed in [true, false] {
                        frame += 1;
                        let pos = bounds[index].center();
                        let events = vec![
                            egui::Event::PointerMoved(pos),
                            egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed, modifiers: egui::Modifiers::NONE },
                        ];
                        view.run(HeadlessView::raw_input(vec2(240., 100.), scale, frame as f64 / 60., events), |ui| {
                            ui.horizontal(|ui| {
                                let back = crate::widgets::icon_button(ui, "j17-nav", Icon::Back, vec2(40., 40.), false, true, "Back");
                                let crop = crate::widgets::tool_button(ui, "crop", Icon::Crop, vec2(40., 40.), false, true, "Crop");
                                let disabled = crate::widgets::tool_button(ui, "crop", Icon::Crop, vec2(40., 40.), false, false, "Crop");
                                for (i, response) in [back, crop, disabled].into_iter().enumerate() {
                                    assert_eq!(response.rect, bounds[i]);
                                    clicks[i] += usize::from(response.clicked());
                                }
                            });
                        });
                    }
                }
                assert_eq!(clicks, [1, 1, 0], "{kind:?}/{scale}/{colored}");
                let pixels = view.paint(&Default::default());
                assert!(pixels.pixels.iter().any(|p| *p != pixels.pixels[0]), "toolbar must render");
                assert_eq!(crate::theme::Tokens::get(&view.ctx), crate::theme::host_palette(kind));
            }
        }
    }
}
