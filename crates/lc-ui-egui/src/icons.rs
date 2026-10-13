//! Library/Develop interface icons: Lucide 0.468.0 line SVGs (ISC) and the shared tool icons
//! (assets/icons, provenance in assets/icons/SOURCES.md), rasterized white at the physical pixel size
//! and tinted per use (Smart Album: Lucide `book` with a Local Image funnel). Tool controls use the
//! shared monochrome tool icons. The LightCraft code-painted glyphs were retired on 2026-10-13.

#[cfg(test)]
use egui::vec2;
use egui::{Color32, Painter, Rect, Vec2, pos2};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Icon {
    Sidebar,
    Back,
    Forward,
    Search,
    Filter,
    Bell,
    Share,
    Help,
    Cloud,
    Sliders,
    Crop,
    Eraser,
    Mask,
    Eye,
    /// Eye with a slash (hidden).
    EyeOff,
    Presets,
    Versions,
    Activity,
    More,
    Tag,
    Info,
    GridPhoto,
    GridSquare,
    Single,
    Compare,
    Survey,
    Sort,
    Star,
    StarFilled,
    Circle,
    FlagPick,
    FlagReject,
    Gear,
    Filmstrip,
    BeforeAfter,
    ChevronDown,
    ChevronRight,
    ChevronLeft,
    TriangleLeft,
    Plus,
    Minus,
    Album,
    SmartAlbum,
    Stack,
    Folder,
    /// Edit (a pencil).
    Pencil,
    Photos,
    Clock,
    Trash,
    Curve,
    ProfileGrid,
    Close,
    Check,
    Brush,
    Linear,
    Radial,
    Sky,
    Subject,
    Picker,
    WhiteBalance,
    SkinTone,
    Defringe,
    Rotate,
    Flip,
    Invert,
    Dots,
    Video,
    Heart,
    Edited,
    /// Targeted adjustment: a ring with a centre dot and up/down arrows (drag vertically).
    Target,
    /// Chat: a speech bubble with three dots (no longer drawn in the top bar). Our own
    /// generic drawing, not any service's logo.
    Chat,
    /// Face boxes on/off: four corner brackets round a small face.
    FaceBox,
}

/// Every icon, for coverage tests and the review sheet.
#[cfg(test)]
pub(crate) const ALL: &[Icon] = &[
    Icon::Sidebar,
    Icon::Back,
    Icon::Forward,
    Icon::Search,
    Icon::Filter,
    Icon::Bell,
    Icon::Share,
    Icon::Help,
    Icon::Cloud,
    Icon::Sliders,
    Icon::Crop,
    Icon::Eraser,
    Icon::Mask,
    Icon::Eye,
    Icon::EyeOff,
    Icon::Presets,
    Icon::Versions,
    Icon::Activity,
    Icon::More,
    Icon::Tag,
    Icon::Info,
    Icon::GridPhoto,
    Icon::GridSquare,
    Icon::Single,
    Icon::Compare,
    Icon::Survey,
    Icon::Sort,
    Icon::Star,
    Icon::StarFilled,
    Icon::Circle,
    Icon::FlagPick,
    Icon::FlagReject,
    Icon::Gear,
    Icon::Filmstrip,
    Icon::BeforeAfter,
    Icon::ChevronDown,
    Icon::ChevronRight,
    Icon::ChevronLeft,
    Icon::TriangleLeft,
    Icon::Plus,
    Icon::Minus,
    Icon::Album,
    Icon::SmartAlbum,
    Icon::Stack,
    Icon::Folder,
    Icon::Pencil,
    Icon::Photos,
    Icon::Clock,
    Icon::Trash,
    Icon::Curve,
    Icon::ProfileGrid,
    Icon::Close,
    Icon::Check,
    Icon::Brush,
    Icon::Linear,
    Icon::Radial,
    Icon::Sky,
    Icon::Subject,
    Icon::Picker,
    Icon::WhiteBalance,
    Icon::SkinTone,
    Icon::Defringe,
    Icon::Rotate,
    Icon::Flip,
    Icon::Invert,
    Icon::Dots,
    Icon::Video,
    Icon::Heart,
    Icon::Edited,
    Icon::Target,
    Icon::Chat,
    Icon::FaceBox,
];

/// The line SVG that draws `icon`: Lucide 0.468.0 (ISC) in `assets/icons`, or the matching tool
/// icon in `assets/icons/tools` so a control and its tool share one glyph.
fn svg(icon: Icon) -> &'static [u8] {
    macro_rules! file {
        ($path:literal) => {
            include_bytes!(concat!("../../../assets/icons/", $path)).as_slice()
        };
    }
    use Icon::*;
    match icon {
        Sidebar => file!("panel-left.svg"),
        Back => file!("arrow-left.svg"),
        Forward => file!("arrow-right.svg"),
        Search => file!("search.svg"),
        Filter => file!("filter.svg"),
        Bell => file!("bell.svg"),
        Share => file!("share.svg"),
        Help => file!("circle-help.svg"),
        Cloud => file!("cloud.svg"),
        Sliders => file!("sliders-horizontal.svg"),
        Crop => file!("tools/crop.svg"),
        Eraser => file!("tools/eraser.svg"),
        Mask => file!("tools/mask.svg"),
        Eye => file!("eye.svg"),
        EyeOff => file!("eye-off.svg"),
        Presets => file!("tools/presets.svg"),
        Versions | Stack => file!("tools/versions.svg"),
        Activity => file!("tools/activity.svg"),
        More | Dots => file!("ellipsis.svg"),
        Tag => file!("tools/keywords.svg"),
        Info => file!("tools/info.svg"),
        GridPhoto => file!("layout-grid.svg"),
        GridSquare => file!("grid-2x2.svg"),
        Single => file!("rectangle-horizontal.svg"),
        Compare => file!("columns-2.svg"),
        Survey => file!("layout-dashboard.svg"),
        Sort => file!("arrow-down-wide-narrow.svg"),
        Star => file!("star.svg"),
        StarFilled => file!("star-filled.svg"),
        Circle => file!("circle.svg"),
        FlagPick => file!("flag.svg"),
        FlagReject => file!("flag-off.svg"),
        Gear => file!("settings.svg"),
        Filmstrip => file!("film.svg"),
        BeforeAfter => file!("square-split-horizontal.svg"),
        ChevronDown => file!("chevron-down.svg"),
        ChevronRight => file!("chevron-right.svg"),
        ChevronLeft | TriangleLeft => file!("chevron-left.svg"),
        Plus => file!("plus.svg"),
        Minus => file!("minus.svg"),
        Album => file!("book-image.svg"),
        SmartAlbum => file!("smart-album.svg"),
        Folder => file!("folder.svg"),
        Pencil | Edited => file!("pencil.svg"),
        Photos => file!("images.svg"),
        Clock => file!("clock.svg"),
        Trash => file!("trash-2.svg"),
        Curve => file!("chart-spline.svg"),
        ProfileGrid => file!("grid-3x3.svg"),
        Close => file!("x.svg"),
        Check => file!("check.svg"),
        Brush => file!("tools/brush.svg"),
        Linear => file!("tools/linear-mask.svg"),
        Radial => file!("tools/radial-mask.svg"),
        Sky => file!("tools/sky-mask.svg"),
        Subject => file!("tools/subject-mask.svg"),
        Picker => file!("tools/pipette.svg"),
        WhiteBalance => file!("tools/wb-picker.svg"),
        SkinTone => file!("tools/skin-picker.svg"),
        Defringe => file!("tools/defringe-picker.svg"),
        Rotate => file!("rotate-cw.svg"),
        Flip => file!("flip-horizontal-2.svg"),
        Invert => file!("contrast.svg"),
        Video => file!("video.svg"),
        Heart => file!("heart.svg"),
        Target => file!("tools/targeted-adjust.svg"),
        Chat => file!("message-circle.svg"),
        FaceBox => file!("scan-face.svg"),
    }
}

#[derive(Clone, Default)]
struct SvgTextures(std::collections::HashMap<(Icon, u32), egui::TextureHandle>);

/// White raster of `icon` at `px` physical pixels, cached per context. The source's 2 px stroke is
/// drawn at 1.75, like the editor's generic icons (`pc-ui-egui icons.rs`).
fn texture(ctx: &egui::Context, icon: Icon, bytes: &[u8], px: u32) -> Option<egui::TextureId> {
    let id = egui::Id::new("lc-icon-svg-textures");
    if let Some(t) = ctx.data_mut(|d| d.get_temp_mut_or_default::<SvgTextures>(id).0.get(&(icon, px)).map(|t| t.id())) {
        return Some(t);
    }
    let svg = String::from_utf8_lossy(bytes).replace("currentColor", "#ffffff").replace("stroke-width=\"2\"", "stroke-width=\"1.75\"");
    let image = egui_extras::image::load_svg_bytes_with_size(svg.as_bytes(), egui::load::SizeHint::Width(px), &Default::default()).ok()?;
    let handle = ctx.load_texture(format!("lc-icon/{icon:?}/{px}"), image, egui::TextureOptions::LINEAR);
    let tex = handle.id();
    ctx.data_mut(|d| {
        let cache = d.get_temp_mut_or_default::<SvgTextures>(id);
        // Scale changes should not keep an unbounded set of sizes.
        if cache.0.len() >= 1024 {
            cache.0.clear();
        }
        cache.0.insert((icon, px), handle);
    });
    Some(tex)
}

/// Paint `icon` centred in `rect` (square area used) with `color`.
pub fn paint(p: &Painter, rect: Rect, icon: Icon, color: Color32) {
    let side = rect.width().min(rect.height());
    let ppp = p.ctx().pixels_per_point();
    let px = (side * ppp).round().clamp(1.0, 512.0) as u32;
    if let Some(tex) = texture(p.ctx(), icon, svg(icon), px) {
        let r = Rect::from_center_size(rect.center(), Vec2::splat(px as f32 / ppp));
        p.image(tex, r, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), color);
    }
}

/// The tool image for rich tooltips: theme-tinted line icon, or its family tint in colour mode.
pub fn tool_icon(ui: &egui::Ui, name: &str, size: f32) -> Option<egui::Image<'static>> {
    egui_extras::install_image_loaders(ui.ctx());
    if crate::color_icons::enabled(ui.ctx()) {
        crate::color_icons::image(ui, name, size, !ui.is_enabled())
    } else {
        let t = crate::theme::Tokens::get(ui.ctx());
        crate::mono_tool_icons::image(name, size, if ui.is_enabled() { t.icon } else { t.text_disabled })
    }
}

/// Only tool controls opt into colour; the rest of the interface keeps its vector glyphs.
pub fn paint_tool(ui: &egui::Ui, rect: Rect, name: &str, fallback: Icon, tint: Color32, enabled: bool) {
    egui_extras::install_image_loaders(ui.ctx());
    let size = rect.width().min(rect.height());
    let rect = Rect::from_center_size(rect.center(), egui::vec2(size, size));
    if crate::color_icons::enabled(ui.ctx())
        && let Some(image) = crate::color_icons::image(ui, name, size, !enabled || !ui.is_enabled())
    {
        image.paint_at(ui, rect);
    } else if let Some(image) = crate::mono_tool_icons::image(name, size, tint) {
        image.paint_at(ui, rect);
    } else {
        paint(ui.painter(), rect, fallback, tint);
    }
}

/// Named tool controls in Develop (the app uses string tools and RightPanel, not a Tool enum).
pub fn tool_name(tool: &str) -> Option<&'static str> {
    crate::color_icon_data::DEVELOP_TOOL_ICONS
        .iter()
        .find(|(name, _)| *name == tool)
        .filter(|(_, icon)| crate::mono_tool_icons::exists(icon))
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
fn ds_finding_colored_tool_icon_is_square_in_a_wide_click_target() {
    let mut view = crate::headless::HeadlessView::new();
    let wide = Rect::from_min_size(pos2(20., 20.), vec2(100., 30.));
    let raw = crate::headless::HeadlessView::raw_input(vec2(200., 120.), 1., 0., vec![]);
    view.run(raw, |ui| {
        crate::color_icons::set_enabled(ui.ctx(), true);
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
