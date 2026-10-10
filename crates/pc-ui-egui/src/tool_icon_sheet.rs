//! Reproducible contact sheet: `cargo xtask tool-icons`. No GUI, no GPU.

use crate::color_icon_data::COLOR_ICONS;

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
                            sheet.put_pixel(x + (i as u32 * 6 + col) * scale + dx, y + row as u32 * scale + dy, image::Rgba([218, 222, 231, 255]));
                        }
                    }
                }
            }
        }
    }
}

fn tile(sheet: &mut image::RgbaImage, x: u32, y: u32, size: u32, bg: [u8; 4], bytes: &[u8], mono: bool) {
    let svg = String::from_utf8_lossy(bytes).replace("currentColor", if bg[0] < 128 { "#dedede" } else { "#343434" });
    let icon = egui_extras::image::load_svg_bytes_with_size(svg.as_bytes(), egui::load::SizeHint::Width(size), &Default::default()).unwrap();
    assert_eq!(icon.size, [size as usize; 2]);
    let pixels: Vec<_> = icon.pixels.iter().flat_map(|p| p.to_srgba_unmultiplied()).collect();
    let icon = image::RgbaImage::from_raw(size, size, pixels).unwrap();
    let mut tile = image::RgbaImage::from_pixel(52, 54, image::Rgba(bg));
    image::imageops::overlay(&mut tile, &icon, i64::from((52 - size) / 2), i64::from((54 - size) / 2));
    image::imageops::overlay(sheet, &tile, i64::from(x), i64::from(y));
    if mono {
        assert!(icon.pixels().all(|p| p[0].abs_diff(p[1]) < 12 && p[1].abs_diff(p[2]) < 12));
    }
}

fn pages(sheet: &image::RgbaImage, name: &str, root: &std::path::Path) {
    // Unscaled inspection panels: each row retains its actual 20/40 px raster sizes.
    let dir = root.join("target/icon-review");
    std::fs::create_dir_all(&dir).unwrap();
    for (i, y) in (0..sheet.height()).step_by(640).enumerate() {
        image::imageops::crop_imm(sheet, 0, y, sheet.width(), 640.min(sheet.height() - y)).to_image().save(dir.join(format!("{name}-{i:02}.png"))).unwrap();
    }
}

#[test]
#[ignore = "writes both review sheets; run cargo xtask tool-icons"]
fn write_tool_icon_contact_sheet() {
    use crate::tool_icon_data::{TOOL_ICONS, TOOL_LABELS};
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let before: std::collections::BTreeMap<String, String> = serde_json::from_str(include_str!("../../../docs/design/tool-icons-before.json")).unwrap();
    assert_eq!(before.len(), COLOR_ICONS.len());
    assert_eq!(TOOL_ICONS.len(), COLOR_ICONS.len());
    let rows = COLOR_ICONS.len() as u32;
    let mut sheet = image::RgbaImage::from_pixel(912, rows * 80 + 88, image::Rgba([31, 32, 35, 255]));
    let mut comparison = image::RgbaImage::from_pixel(912, rows * 80 + 88, image::Rgba([31, 32, 35, 255]));
    label(&mut sheet, 16, 16, "local image / original tool icons", 3);
    label(&mut comparison, 16, 16, "local image / before and after", 3);
    label(&mut sheet, 224, 52, "dark 20 40 mono 20 40 / light 20 40 mono 20 40", 1);
    label(&mut comparison, 224, 48, "dark", 1);
    label(&mut comparison, 564, 48, "light", 1);
    for start in [224, 564] {
        for (i, text) in ["old20", "new20", "mono20", "old40", "new40", "mono40"].into_iter().enumerate() {
            label(&mut comparison, start + i as u32 * 52 + 6, 66, text, 1);
        }
    }
    for (i, (name, colour)) in COLOR_ICONS.iter().enumerate() {
        let y = 88 + i as u32 * 80;
        let title = TOOL_LABELS.iter().find(|(n, _)| n == name).unwrap().1;
        let mono = TOOL_ICONS.iter().find(|(n, _)| n == name).unwrap().1;
        let old = before.get(*name).unwrap().as_bytes();
        for out in [&mut sheet, &mut comparison] {
            label(out, 12, y + 20, title, 1);
            label(out, 12, y + 37, name, 1);
        }
        for (theme, bg) in [[31, 32, 35, 255], [242, 242, 244, 255]].into_iter().enumerate() {
            let x = 224 + theme as u32 * 340;
            for (j, (size, bytes, is_mono)) in [(20, *colour, false), (40, *colour, false), (20, mono, true), (40, mono, true)].into_iter().enumerate() {
                tile(&mut sheet, x + j as u32 * 52, y + 8, size, bg, bytes, is_mono);
            }
            for (j, (size, bytes, is_mono)) in
                [(20, old, false), (20, *colour, false), (20, mono, true), (40, old, false), (40, *colour, false), (40, mono, true)].into_iter().enumerate()
            {
                tile(&mut comparison, x + j as u32 * 52, y + 8, size, bg, bytes, is_mono);
            }
        }
    }
    for (name, out) in [("tool-icons", sheet), ("tool-icons-before-after", comparison)] {
        let path = root.join(format!("docs/design/{name}.png"));
        out.save(&path).unwrap();
        pages(&out, name, &root);
        println!("{} tools, 20/40 px on dark/light: {}", rows, path.display());
    }
}
