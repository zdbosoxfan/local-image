//! Icon rendering. SVGs are embedded (see `icon_data.rs`), recoloured to white, rasterized by the
//! egui_extras SVG loader at the exact on-screen pixel size (crisp at any DPI), and tinted per use.
//!
//! (History: the first shell used Unicode glyphs; half of them rendered as tofu boxes because the
//! bundled fonts lacked them. Vector icons fix that for good.)

use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

use egui::{Color32, Rect, Response, Sense, Vec2};

use crate::icon_data::ICONS;
use crate::state::Tool;
use crate::theme::Tokens;

fn white_icons() -> &'static HashMap<&'static str, Arc<[u8]>> {
    static MAP: OnceLock<HashMap<&'static str, Arc<[u8]>>> = OnceLock::new();
    MAP.get_or_init(|| {
        ICONS
            .iter()
            .map(|(name, bytes)| {
                let svg = String::from_utf8_lossy(bytes).replace("currentColor", "#ffffff").replace("stroke-width=\"2\"", "stroke-width=\"1.75\"");
                (*name, Arc::from(svg.into_bytes().into_boxed_slice()))
            })
            .collect()
    })
}

fn monochrome_name(name: &str) -> &str {
    match name {
        "vertical-type" => "type",
        "spot-healing" => "bandage",
        "healing" => "bandage",
        "slice-select" => "square-dashed-mouse-pointer",
        "red-eye" => "eye",
        "history-brush" => "clock",
        "blur" => "droplet",
        "sharpen" => "triangle",
        "smudge" => "pointer",
        "dodge" => "lollipop",
        "sponge" => "cloud",
        "quick-selection" => "circle-dashed",
        "object-selection" => "square-dashed-mouse-pointer",
        "shape-triangle" => "triangle",
        "shape-polygon" => "pentagon",
        "custom-shape" => "cloud",
        "forward-warp" => "pointer",
        "reconstruct" => "brush-cleaning",
        "smooth" => "droplet",
        "twirl-cw" => "rotate-cw",
        "twirl-ccw" => "undo-2",
        "pucker" => "scan",
        "bloat" => "maximize-2",
        "push-left" => "chevrons-left",
        "freeze" => "lock",
        "thaw" => "lock-open",
        "freeze-lasso" => "lasso",
        "reconstruct-all" => "brush-cleaning",
        "freeze-all" => "lock",
        "thaw-all" => "lock-open",
        "invert-freeze" => "circle",
        "edit-toolbar" => "ellipsis",
        "quick-mask" => "square-dashed",
        name => name,
    }
}

pub fn exists(name: &str) -> bool {
    white_icons().contains_key(monochrome_name(name))
}

/// An egui image for an icon, tinted.
pub fn image(name: &str, size: f32, tint: Color32) -> egui::Image<'static> {
    let name = monochrome_name(name);
    let bytes = white_icons().get(name).or_else(|| white_icons().get("square")).cloned().unwrap_or_default();
    egui::Image::from_bytes(format!("bytes://icons/{name}.svg"), egui::load::Bytes::Shared(bytes)).fit_to_exact_size(Vec2::splat(size)).tint(tint)
}

/// Paint an icon centred in `rect`.
pub fn paint(ui: &egui::Ui, rect: Rect, name: &str, size: f32, tint: Color32) {
    let r = Rect::from_center_size(rect.center(), Vec2::splat(size));
    image(name, size, tint).paint_at(ui, r);
}

/// An icon as the pointer, above every window: `name` with its hotspot `hot` (a fraction of the
/// icon box) on `p`, white with a dark outline so it reads on any image. The caller hides the OS
/// cursor (`CursorIcon::None`).
pub fn cursor(ctx: &egui::Context, name: &str, p: egui::Pos2, hot: Vec2, size: f32) {
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

pub fn tool_icon_name(t: Tool) -> &'static str {
    match t {
        Tool::Move => "move",
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
    if tooltip.is_empty() { resp } else { resp.on_hover_text(tl!(tooltip)) }
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

/// Tool image for toolbar buttons and the rich tooltip's 32 px slot. Colour keeps its SVG
/// materials; monochrome uses the existing theme tint. Set from preferences each frame.
pub fn tool_icon(ui: &egui::Ui, name: &str, size: f32) -> egui::Image<'static> {
    if crate::color_icons::enabled(ui.ctx())
        && let Some(image) = crate::color_icons::image(ui, name, size, !ui.is_enabled())
    {
        return image;
    }
    image(name, size, if ui.is_enabled() { Tokens::get(ui.ctx()).icon } else { Tokens::get(ui.ctx()).text_faint })
}

pub fn paint_tool(ui: &egui::Ui, rect: Rect, name: &str, size: f32, tint: Color32) {
    if crate::color_icons::enabled(ui.ctx())
        && let Some(image) = crate::color_icons::image(ui, name, size, !ui.is_enabled())
    {
        image.paint_at(ui, Rect::from_center_size(rect.center(), Vec2::splat(size)));
    } else {
        paint(ui, rect, name, size, tint);
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
    fn every_tool_has_an_icon() {
        for t in Tool::ALL {
            assert!(exists(tool_icon_name(t)), "{t:?}");
        }
    }

    #[test]
    fn icons_are_recoloured() {
        let m = white_icons();
        assert!(m.len() >= 60);
        for (name, b) in m.iter() {
            let s = std::str::from_utf8(b).unwrap();
            assert!(!s.contains("currentColor"), "{name}");
        }
    }
}
