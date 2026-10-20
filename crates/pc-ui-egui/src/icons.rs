//! Icon rendering. Generic SVGs (`li_ui_kit::icon_data`) and the line tool icons (`li_ui_kit::tool_icon_data`:
//! Lucide, PhotoCraft, VectorCraft, Tabler and Local Image originals) are embedded, recoloured to
//! white, rasterized by the
//! egui_extras SVG loader at the exact on-screen pixel size (crisp at any DPI), and tinted per use.
//!
//! (History: the first shell used Unicode glyphs; half of them rendered as tofu boxes because the
//! bundled fonts lacked them. Vector icons fix that for good.)

use crate::state::Tool;
use crate::theme::Tokens;
use egui::{Color32, Rect, Response, Sense, Vec2};
#[cfg(test)]
use li_ui_kit::icon_data::ICONS;
#[cfg(test)]
use li_ui_kit::icons::exists;
use li_ui_kit::icons::image;
/// Paint an icon centred in `rect`.
pub fn paint(ui: &egui::Ui, rect: Rect, name: &str, size: f32, tint: Color32) {
    li_ui_kit::icons::install(ui.ctx());
    let r = Rect::from_center_size(rect.center(), Vec2::splat(size));
    image(name, size, tint).paint_at(ui, r);
}

/// An icon as the pointer, above every window: `name` with its hotspot `hot` (a fraction of the
/// icon box) on `p`, white with a dark outline so it reads on any image. The caller hides the OS
/// cursor (`CursorIcon::None`).
pub fn cursor(ctx: &egui::Context, name: &str, p: egui::Pos2, hot: Vec2, size: f32) {
    let name = if name == "pipette" { "cursor-pipette" } else { name };
    let rect = Rect::from_min_size(p - hot * size, Vec2::splat(size));
    let area = egui::Area::new(egui::Id::new("pc-icon-cursor")).order(egui::Order::Tooltip).fixed_pos(rect.min).constrain(false).interactable(false);
    area.show(ctx, |ui| {
        let outline = image(name, size, Color32::from_black_alpha(200));
        for (dx, dy) in [(-1.0, 0.0), (1.0, 0.0), (0.0, -1.0), (0.0, 1.0), (-1.0, -1.0), (1.0, 1.0), (-1.0, 1.0), (1.0, -1.0)] {
            outline.paint_at(ui, rect.translate(egui::vec2(dx, dy)));
        }
        image(name, size, Color32::WHITE).paint_at(ui, rect);
    });
}

pub fn tool_icon_name_for(t: Tool, persona: photocraft_engine::toolsets::Persona) -> &'static str {
    if persona == photocraft_engine::toolsets::Persona::Vector || t.key() == '\0' {
        let id = format!("{t:?}");
        if let Some((_, name, _)) = li_ui_kit::vector_icon_data::ICONS.iter().find(|(tool, _, _)| *tool == id) {
            return name;
        }
    }
    tool_icon_name(t)
}
pub fn tool_icon_name(t: Tool) -> &'static str {
    if t.key() == '\0' {
        let id = format!("{t:?}");
        if let Some((_, name, _)) = li_ui_kit::vector_icon_data::ICONS.iter().find(|(tool, _, _)| *tool == id) {
            return name;
        }
    }
    match t {
        Tool::Move => "move",
        Tool::PointTransform => "move",
        Tool::VectorPencil => "pencil",
        Tool::VectorBrush => "brush",
        Tool::Smooth => "spline",
        Tool::PathEraser => "eraser",
        Tool::Join => "spline",
        Tool::Corner => "rectangle-horizontal",
        Tool::RoundedRectangle => "square",
        Tool::Star => "star",
        Tool::DoubleStar => "star",
        Tool::SquareStar => "star",
        Tool::Trapezoid => "square",
        Tool::Diamond => "diamond",
        Tool::Donut => "circle",
        Tool::Pie => "circle",
        Tool::Segment => "circle",
        Tool::Arrow => "arrow-left-right",
        Tool::Callout => "message-square",
        Tool::Cog => "settings",
        Tool::Heart => "heart",
        Tool::Tear => "droplet",
        Tool::Crescent => "moon",
        Tool::Cloud => "cloud",
        Tool::Spiral => "spline",
        Tool::Arc => "circle",
        Tool::RectGrid => "grid-2x2",
        Tool::PolarGrid => "circle",
        Tool::VectorFill => "blend",
        Tool::Transparency => "blend",
        Tool::ShapeBuilder => "layers",
        Tool::Contour => "circle",
        Tool::Knife => "scissors",
        Tool::Scissors => "scissors",
        Tool::LineCut => "scissors",
        Tool::Width => "arrow-left-right",
        Tool::ArtText => "type",
        Tool::FrameText => "type",
        Tool::TextOnPath => "type",
        Tool::Blend => "blend",
        Tool::Repeat => "copy",
        Tool::Envelope => "square",
        Tool::VectorCrop => "crop",
        Tool::Artboard => "square",
        Tool::GroupSelection => "mouse-pointer-2",
        Tool::VectorMagicWand => "wand",
        Tool::VectorLasso => "lasso",
        Tool::Curvature => "pen-tool",
        Tool::AddAnchor => "plus",
        Tool::DeleteAnchor => "minus",
        Tool::AnchorPoint => "pen-tool",
        Tool::Rotate => "rotate-cw",
        Tool::Reflect => "flip-horizontal",
        Tool::Scale => "maximize",
        Tool::Shear => "move",
        Tool::Reshape => "spline",
        Tool::FreeTransform => "move",
        Tool::StylePicker => "pipette",
        Tool::BlobBrush => "brush",
        Tool::VectorEraser => "eraser",
        Tool::VectorWarp => "move",
        Tool::VectorTwirl => "rotate-cw",
        Tool::VectorPucker => "minimize",
        Tool::VectorBloat => "maximize",
        Tool::Scallop => "circle",
        Tool::Crystallize => "star",
        Tool::Wrinkle => "spline",
        Tool::VectorPuppetWarp => "move",
        Tool::LivePaintBucket => "paint-bucket",
        Tool::VectorFloodFill => "paint-bucket",
        Tool::LivePaintSelection => "mouse-pointer-2",
        Tool::Mesh => "grid-2x2",
        Tool::Measure => "ruler",
        Tool::Area => "square",

        Tool::RectMarquee => "square-dashed",
        Tool::EllipseMarquee => "circle-dashed",
        Tool::Brush => "brush",
        Tool::Pencil => "pencil",
        Tool::MixerBrush => "palette",
        Tool::Eraser => "eraser",
        Tool::BackgroundEraser => "eraser-background",
        Tool::MagicEraser => "eraser-magic",
        Tool::Eyedropper => "pipette",
        Tool::Ruler => "ruler",
        Tool::Note => "message-square",
        Tool::Count => "circle-dot",
        Tool::Lasso => "lasso",
        Tool::PolygonLasso => "pentagon",
        Tool::MagneticLasso => "lasso-magnetic",
        Tool::MagicWand => "wand-sparkles",
        Tool::Crop => "crop",
        Tool::PerspectiveCrop => "perspective-crop",
        Tool::Slice => "slice-knife",
        Tool::SliceSelect => "slice-select",
        Tool::Gradient => "blend",
        Tool::PaintBucket => "paint-bucket",
        Tool::Type => "type",
        Tool::VerticalType => "vertical-type",
        Tool::Hand => "hand",
        Tool::RotateView => "rotate-view",
        Tool::Zoom => "zoom-in",
        Tool::SpotHealing => "spot-healing",
        Tool::Healing => "healing",
        Tool::Patch => "lasso-select",
        Tool::ContentAwareMove => "arrow-left-right",
        Tool::RedEye => "red-eye",
        Tool::CloneStamp => "stamp",
        Tool::HistoryBrush => "history-brush",
        Tool::Blur => "blur",
        Tool::Sharpen => "sharpen",
        Tool::Smudge => "smudge",
        Tool::Dodge => "dodge",
        Tool::Burn => "flame",
        Tool::Sponge => "sponge",
        Tool::QuickSelection => "quick-selection",
        Tool::ObjectSelection => "object-selection",
        Tool::Pen => "pen-tool",
        Tool::PathSelection => "mouse-pointer-2",
        Tool::DirectSelection => "direct-select",
        Tool::Rectangle => "rectangle-horizontal",
        Tool::EllipseShape => "circle",
        Tool::Triangle => "shape-triangle",
        Tool::Polygon => "shape-polygon",
        Tool::Line => "slash",
        Tool::CustomShape => "custom-shape",
        Tool::AiRemove => "sparkles",
        Tool::AiCutout => "scissors",
    }
}

/// Selected and hover fill shared by square icon buttons. Returns the icon tint.
pub fn button_chrome(ui: &egui::Ui, rect: Rect, selected: bool, hovered: bool) -> Color32 {
    let t = Tokens::get(ui.ctx());
    if selected {
        ui.painter().rect_filled(rect, t.radius_sm, t.accent_soft);
        ui.painter().rect_stroke(rect, t.radius_sm, egui::Stroke::new(1.0, t.accent_border), egui::StrokeKind::Inside);
    } else if hovered {
        ui.painter().rect_filled(rect, t.radius_sm, t.hover);
    }
    if selected {
        t.accent_text
    } else if hovered {
        t.text
    } else {
        t.icon
    }
}

/// Square icon button: transparent until hovered; `selected` gets the accent treatment.
pub fn button(ui: &mut egui::Ui, name: &str, box_size: f32, selected: bool, tooltip: &str) -> Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(box_size), Sense::click());
    let tint = button_chrome(ui, rect, selected, resp.hovered());
    paint(ui, rect, name, (box_size * 0.52).round(), tint);
    if tooltip.is_empty() {
        resp
    } else {
        let label = tl!(tooltip);
        resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, resp.enabled(), label));
        resp.on_hover_text(label)
    }
}

/// Rail toggle: "on" gets a quiet filled background and full-strength icon (no accent).
pub fn rail_button(ui: &mut egui::Ui, name: &str, box_size: f32, on: bool, tooltip: &str) -> Response {
    let t = Tokens::get(ui.ctx());
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(box_size), Sense::click());
    if on {
        ui.painter().rect_filled(rect, t.radius_sm, t.card);
        ui.painter().rect_stroke(rect, t.radius_sm, egui::Stroke::new(1.0, t.card_border), egui::StrokeKind::Inside);
    } else if resp.hovered() {
        ui.painter().rect_filled(rect, t.radius_sm, t.hover);
    }
    let tint = if on || resp.hovered() { t.text } else { t.text_faint };
    paint(ui, rect, name, (box_size * 0.52).round(), tint);
    resp.on_hover_text(tl!(tooltip))
}

fn monochrome_tool_image(name: &str, size: f32, tint: Color32) -> egui::Image<'static> {
    li_ui_kit::mono_tool_icons::image(name, size, tint).unwrap_or_else(|| image(name, size, tint))
}

/// Tool image for toolbar buttons and the rich tooltip's 32 px slot. Monochrome (the default) uses the
/// theme tint; colour mode tints the same line icon by family. Set from preferences each frame.
pub fn tool_icon(ui: &egui::Ui, name: &str, size: f32) -> egui::Image<'static> {
    if li_ui_kit::color_icons::enabled(ui.ctx())
        && let Some(image) = crate::color_icons::image(ui, name, size, !ui.is_enabled())
    {
        return image;
    }
    monochrome_tool_image(name, size, if ui.is_enabled() { Tokens::get(ui.ctx()).icon } else { Tokens::get(ui.ctx()).text_faint })
}

pub fn paint_tool(ui: &egui::Ui, rect: Rect, name: &str, size: f32, tint: Color32) {
    let tint = if ui.is_enabled() { tint } else { Tokens::get(ui.ctx()).text_faint };
    if li_ui_kit::color_icons::enabled(ui.ctx())
        && let Some(image) = crate::color_icons::image(ui, name, size, !ui.is_enabled())
    {
        image.paint_at(ui, Rect::from_center_size(rect.center(), Vec2::splat(size)));
    } else {
        monochrome_tool_image(name, size, tint).paint_at(ui, Rect::from_center_size(rect.center(), Vec2::splat(size)));
    }
}

/// The same button chrome as other icons; only the tool image changes.
pub fn tool_button(ui: &mut egui::Ui, name: &str, box_size: f32, selected: bool, tooltip: &str) -> Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(box_size), Sense::click());
    resp.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Button, ui.is_enabled(), selected, tl!(tooltip)));
    let tint = button_chrome(ui, rect, selected, resp.hovered());
    paint_tool(ui, rect, name, (box_size * 0.62).round().clamp(20.0, 24.0), tint);
    if tooltip.is_empty() { resp } else { resp.on_hover_text(tl!(tooltip)) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cursor_hotspot_artwork_remains_separate_from_tool_artwork() {
        let ctx = egui::Context::default();
        let cursor = image("cursor-pipette", 20.0, Color32::WHITE);
        let tool = monochrome_tool_image("pipette", 20.0, Color32::WHITE);
        assert_ne!(cursor.uri(), tool.uri());
        for (icon, source, style) in [
            (cursor, ICONS.iter().find(|(n, _)| *n == "cursor-pipette").unwrap().1, li_ui_kit::icons::StrokeStyle::Interface),
            (tool, li_ui_kit::mono_tool_icons::svg("pipette").unwrap(), li_ui_kit::icons::StrokeStyle::Tool),
        ] {
            match icon.source(&ctx) {
                egui::ImageSource::Bytes { bytes, .. } => assert_eq!(bytes.as_ref(), li_ui_kit::icons::prepared(source, style).as_ref()),
                other => panic!("unexpected source: {other:?}"),
            }
        }
    }

    #[test]
    fn ds_finding_disabled_monochrome_tool_has_faint_tint_and_ignores_click() {
        use egui_kittest::kittest::Queryable;
        let mut h = egui_kittest::Harness::builder().with_size(egui::vec2(120., 80.)).build_ui_state(
            |ui, clicked: &mut bool| {
                egui_extras::install_image_loaders(ui.ctx());
                li_ui_kit::color_icons::set_enabled(ui.ctx(), false);
                ui.add_enabled_ui(false, |ui| {
                    *clicked |= tool_button(ui, "crop", 40., false, "Disabled tool").clicked();
                });
            },
            false,
        );
        h.run_steps(3);
        let faint = Tokens::get(&h.ctx).text_faint;
        let tint = h.output().shapes.iter().find_map(|s| if let egui::Shape::Rect(r) = &s.shape { r.brush.as_ref().map(|_| r.fill) } else { None }).unwrap();
        assert_eq!(tint, faint.gamma_multiply(0.5), "egui also applies disabled UI opacity");
        h.get_by_label("Disabled tool").click();
        h.run_steps(2);
        assert!(!h.state());
    }

    #[test]
    fn generic_tool_ids_embed_the_same_sourced_monochrome_glyph() {
        for (name, tool) in li_ui_kit::tool_icon_data::TOOL_ICONS {
            if let Some((_, generic)) = ICONS.iter().find(|(id, _)| id == name) {
                assert_eq!(generic, tool, "generic tool ID uses a different silhouette: {name}");
            }
        }
    }

    #[test]
    fn every_tool_has_an_icon() {
        for t in Tool::ALL {
            assert!(exists(tool_icon_name(t)), "{t:?}");
        }
    }

    #[test]
    fn icons_are_recoloured() {
        assert!(ICONS.len() >= 60);
        for (name, source) in ICONS {
            let b = li_ui_kit::icons::prepared(source, li_ui_kit::icons::StrokeStyle::Interface);
            let s = std::str::from_utf8(&b).unwrap();
            assert!(!s.contains("currentColor"), "{name}");
        }
    }
}

#[cfg(test)]
#[test]
fn editor_icon_controls_keep_accessible_clicks_and_disabled_behavior() {
    use egui_kittest::kittest::Queryable;
    for kind in crate::theme::ThemeKind::ALL {
        for colored in [false, true] {
            let mut h = egui_kittest::Harness::builder().with_size(egui::vec2(240., 100.)).build_ui_state(
                |ui, clicked: &mut [usize; 3]| {
                    crate::theme::apply(ui.ctx(), kind);
                    li_ui_kit::color_icons::set_enabled(ui.ctx(), colored);
                    ui.horizontal(|ui| {
                        clicked[0] += usize::from(button(ui, "crop", 40., false, "Crop").clicked());
                        clicked[1] += usize::from(tool_button(ui, "brush", 40., false, "Brush Tool").clicked());
                        ui.add_enabled_ui(false, |ui| {
                            clicked[2] += usize::from(tool_button(ui, "crop", 40., false, "Disabled tool").clicked());
                        });
                    });
                },
                [0usize; 3],
            );
            h.run_steps(3);
            for label in ["Crop", "Brush Tool", "Disabled tool"] {
                h.get_by_label(label).click();
                h.run_steps(2);
            }
            assert_eq!(*h.state(), [1, 1, 0], "{kind:?}/{colored}");
        }
    }
}
