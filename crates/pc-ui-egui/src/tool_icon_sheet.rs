//! Reproducible tool icon review sheets: `cargo xtask tool-icons`. No GUI, no GPU; resvg at the real
//! pixel sizes, monochrome in the theme icon grey and colour mode's family tint, on dark and light.

use li_ui_kit::tool_icon_data::{TOOL_ICONS, TOOL_LABELS};

// A small original 5×7 label alphabet keeps the review artifact independent of host fonts.
fn glyph(ch: char) -> [u8; 7] {
    match ch.to_ascii_lowercase() {
        'a' => [14, 17, 17, 31, 17, 17, 17],
        'b' => [30, 17, 17, 30, 17, 17, 30],
        'c' => [14, 17, 16, 16, 16, 17, 14],
        'd' => [30, 17, 17, 17, 17, 17, 30],
        'e' => [31, 16, 16, 30, 16, 16, 31],
        'f' => [31, 16, 16, 30, 16, 16, 16],
        'g' => [14, 17, 16, 23, 17, 17, 14],
        'h' => [17, 17, 17, 31, 17, 17, 17],
        'i' => [14, 4, 4, 4, 4, 4, 14],
        'j' => [7, 2, 2, 2, 2, 18, 12],
        'k' => [17, 18, 20, 24, 20, 18, 17],
        'l' => [16, 16, 16, 16, 16, 16, 31],
        'm' => [17, 27, 21, 21, 17, 17, 17],
        'n' => [17, 25, 21, 19, 17, 17, 17],
        'o' => [14, 17, 17, 17, 17, 17, 14],
        'p' => [30, 17, 17, 30, 16, 16, 16],
        'q' => [14, 17, 17, 17, 21, 18, 13],
        'r' => [30, 17, 17, 30, 20, 18, 17],
        's' => [15, 16, 16, 14, 1, 1, 30],
        't' => [31, 4, 4, 4, 4, 4, 4],
        'u' => [17, 17, 17, 17, 17, 17, 14],
        'v' => [17, 17, 17, 17, 17, 10, 4],
        'w' => [17, 17, 17, 21, 21, 21, 10],
        'x' => [17, 17, 10, 4, 10, 17, 17],
        'y' => [17, 17, 10, 4, 4, 4, 4],
        'z' => [31, 1, 2, 4, 8, 16, 31],
        '0' => [14, 17, 19, 21, 25, 17, 14],
        '1' => [4, 12, 4, 4, 4, 4, 14],
        '2' => [14, 17, 1, 2, 4, 8, 31],
        '3' => [30, 1, 1, 14, 1, 1, 30],
        '4' => [2, 6, 10, 18, 31, 2, 2],
        '5' => [31, 16, 16, 30, 1, 1, 30],
        '6' => [14, 16, 16, 30, 17, 17, 14],
        '7' => [31, 1, 2, 4, 8, 8, 8],
        '8' => [14, 17, 17, 14, 17, 17, 14],
        '9' => [14, 17, 17, 15, 1, 1, 14],
        '-' => [0, 0, 0, 31, 0, 0, 0],
        '/' => [1, 2, 2, 4, 8, 8, 16],
        '(' => [2, 4, 8, 8, 8, 4, 2],
        ')' => [8, 4, 2, 2, 2, 4, 8],
        _ => [0; 7],
    }
}

fn label(sheet: &mut image::RgbaImage, x: u32, y: u32, text: &str, scale: u32) {
    for (i, ch) in text.chars().enumerate() {
        for (row, bits) in glyph(ch).iter().enumerate() {
            for col in 0..5 {
                if bits & (1 << (4 - col)) != 0 {
                    for dy in 0..scale {
                        for dx in 0..scale {
                            let (px, py) = (x + (i as u32 * 6 + col) * scale + dx, y + row as u32 * scale + dy);
                            if px < sheet.width() && py < sheet.height() {
                                sheet.put_pixel(px, py, image::Rgba([218, 222, 231, 255]));
                            }
                        }
                    }
                }
            }
        }
    }
}

const DARK: [u8; 4] = [31, 32, 35, 255];
const LIGHT: [u8; 4] = [242, 242, 244, 255];

/// How a tile is drawn: the SVG as authored (retired colour artwork), or a line icon in one colour.
#[derive(Clone, Copy)]
enum Ink<'a> {
    AsAuthored,
    Mono,
    Family(&'a str),
}

fn tile(sheet: &mut image::RgbaImage, x: u32, y: u32, size: u32, bg: [u8; 4], bytes: &[u8], ink: Ink) {
    let dark = bg[0] < 128;
    let icon = egui::Color32::from_gray(if dark { 222 } else { 52 });
    let colour = match ink {
        Ink::AsAuthored | Ink::Mono => icon,
        Ink::Family(name) => li_ui_kit::color_icons::tint(name, icon, dark),
    };
    let svg = String::from_utf8_lossy(bytes).replace("currentColor", &format!("#{:02x}{:02x}{:02x}", colour.r(), colour.g(), colour.b()));
    let mut icon = egui_extras::image::load_svg_bytes_with_size(svg.as_bytes(), egui::load::SizeHint::Width(size), &Default::default()).unwrap();
    assert_eq!(icon.size, [size as usize; 2]);
    if matches!(ink, Ink::Mono) {
        // Like egui's tint: the retired monochrome files bake white paint instead of currentColor.
        for pixel in &mut icon.pixels {
            *pixel = egui::Color32::from_rgba_unmultiplied(colour.r(), colour.g(), colour.b(), pixel.a());
        }
    }
    let pixels: Vec<_> = icon.pixels.iter().flat_map(|p| p.to_srgba_unmultiplied()).collect();
    let icon = image::RgbaImage::from_raw(size, size, pixels).unwrap();
    let mut tile = image::RgbaImage::from_pixel(52, 54, image::Rgba(bg));
    image::imageops::overlay(&mut tile, &icon, i64::from((52 - size) / 2), i64::from((54 - size) / 2));
    image::imageops::overlay(sheet, &tile, i64::from(x), i64::from(y));
}

fn pages(sheet: &image::RgbaImage, name: &str, dir: &std::path::Path) {
    // Unscaled inspection panels: each row retains its actual raster sizes.
    std::fs::create_dir_all(dir).unwrap();
    for (i, y) in (0..sheet.height()).step_by(640).enumerate() {
        image::imageops::crop_imm(sheet, 0, y, sheet.width(), 640.min(sheet.height() - y)).to_image().save(dir.join(format!("{name}-{i:02}.png"))).unwrap();
    }
}

/// Rows of (label, id, before, after) drawn as `columns` (heading, before?, size, ink) per theme.
fn sheet(title: &str, rows: &[(&str, &str, Option<Vec<u8>>, &[u8])], columns: &[(&str, bool, u32, bool)]) -> image::RgbaImage {
    let theme_w = columns.len() as u32 * 52 + 20;
    let mut out = image::RgbaImage::from_pixel(224 + 2 * theme_w, rows.len() as u32 * 60 + 88, image::Rgba(DARK));
    label(&mut out, 16, 16, title, 2);
    for (theme, text) in ["dark", "light"].into_iter().enumerate() {
        let x0 = 224 + theme as u32 * theme_w;
        label(&mut out, x0, 44, text, 1);
        for (i, (heading, ..)) in columns.iter().enumerate() {
            label(&mut out, x0 + i as u32 * 52 + 4, 62, heading, 1);
        }
    }
    for (r, (title, name, before, after)) in rows.iter().enumerate() {
        let y = 88 + r as u32 * 60;
        label(&mut out, 12, y + 18, title, 1);
        label(&mut out, 12, y + 34, name, 1);
        for (theme, bg) in [DARK, LIGHT].into_iter().enumerate() {
            for (i, (_, old, size, colour)) in columns.iter().enumerate() {
                let x = 224 + theme as u32 * theme_w + i as u32 * 52;
                match (old, before) {
                    (true, Some(bytes)) => tile(&mut out, x, y + 3, *size, bg, bytes, if *colour { Ink::AsAuthored } else { Ink::Mono }),
                    (true, None) => label(&mut out, x + 20, y + 24, "-", 1),
                    (false, _) => tile(&mut out, x, y + 3, *size, bg, after, if *colour { Ink::Family(name) } else { Ink::Mono }),
                }
            }
        }
    }
    out
}

#[test]
#[ignore = "writes the tool icon review sheets; run cargo xtask tool-icons"]
fn write_tool_icon_contact_sheet() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let title = |name: &str| TOOL_LABELS.iter().find(|(n, _)| *n == name).map_or(name.to_owned(), |(_, l)| (*l).to_owned());
    // docs/design: the Compositing/Develop tools, monochrome (default) and colour mode.
    let rows: Vec<_> = TOOL_ICONS.iter().map(|(name, bytes)| (title(name), *name, None, *bytes)).collect();
    let rows: Vec<_> = rows.iter().map(|(t, n, b, a)| (t.as_str(), *n, b.clone(), *a)).collect();
    let current = sheet(
        "local image / tool icons",
        &rows,
        &[("mono16", false, 16, false), ("mono20", false, 20, false), ("mono40", false, 40, false), ("col20", false, 20, true), ("col40", false, 40, true)],
    );
    // Before/after against the retired Glossy Duotone colour set (docs/design/tool-icons-pre-sourced.json).
    let before: std::collections::BTreeMap<String, String> = serde_json::from_str(include_str!("../../../docs/design/tool-icons-pre-sourced.json")).unwrap();
    let rows: Vec<_> = TOOL_ICONS.iter().map(|(name, bytes)| (title(name), *name, before.get(*name).map(|s| s.clone().into_bytes()), *bytes)).collect();
    let rows: Vec<_> = rows.iter().map(|(t, n, b, a)| (t.as_str(), *n, b.clone(), *a)).collect();
    let comparison = sheet(
        "local image / before and after",
        &rows,
        &[("old20", true, 20, true), ("old40", true, 40, true), ("new20", false, 20, false), ("new40", false, 40, false), ("col20", false, 20, true)],
    );
    for (name, out) in [("tool-icons", current), ("tool-icons-before-after", comparison)] {
        let path = root.join(format!("docs/design/{name}.png"));
        out.save(&path).unwrap();
        pages(&out, name, &root.join("target/icon-review"));
        println!("{} tools, dark/light: {}", TOOL_ICONS.len(), path.display());
    }
    // target/icons-ux: every tool icon (Compositing, Develop, Liquify and Vector) at 16/20/40 px against
    // the previous artwork when it was exported to target/icons-ux/before (see docs/wip/BUILD-icons.md).
    let old = root.join("target/icons-ux/before/assets/icons");
    let all: Vec<(&str, &[u8])> = TOOL_ICONS
        .iter()
        .copied()
        .chain(li_ui_kit::vector_icon_data::ICONS.iter().filter(|(_, n, _)| n.starts_with("vector-")).map(|(_, n, svg)| (*n, *svg)))
        .collect::<std::collections::BTreeMap<_, _>>()
        .into_iter()
        .collect();
    let rows: Vec<_> = all
        .iter()
        .map(|(name, bytes)| {
            let label = title(name);
            let colour = std::fs::read(old.join(format!("color/{name}.svg"))).ok();
            let mono = std::fs::read(old.join(format!("tools/{name}.svg"))).ok();
            (label, *name, colour, mono, *bytes)
        })
        .collect();
    let ux = root.join("target/icons-ux");
    for (suffix, old_colour) in [("mono", false), ("colour", true)] {
        let rows: Vec<_> = rows.iter().map(|(t, n, c, m, a)| (t.as_str(), *n, if old_colour { c.clone() } else { m.clone() }, *a)).collect();
        let columns: &[(&str, bool, u32, bool)] = &[
            ("old16", true, 16, old_colour),
            ("old20", true, 20, old_colour),
            ("old40", true, 40, old_colour),
            ("new16", false, 16, old_colour),
            ("new20", false, 20, old_colour),
            ("new40", false, 40, old_colour),
        ];
        let out = sheet(&format!("tool icons before/after ({suffix})"), &rows, columns);
        std::fs::create_dir_all(&ux).unwrap();
        out.save(ux.join(format!("tools-before-after-{suffix}.png"))).unwrap();
        pages(&out, &format!("tools-before-after-{suffix}"), &ux.join("pages"));
        println!("{} tool icons before/after ({suffix}): {}", rows.len(), ux.display());
    }
}
