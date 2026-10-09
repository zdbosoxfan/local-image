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

#[test]
#[ignore = "writes docs/design/tool-icons.png; run cargo xtask tool-icons"]
fn write_tool_icon_contact_sheet() {
    let cols = 4u32;
    let rows = COLOR_ICONS.len().div_ceil(cols as usize) as u32;
    let mut sheet = image::RgbaImage::from_pixel(cols * 360 + 32, rows * 104 + 88, image::Rgba([31, 32, 35, 255]));
    label(&mut sheet, 24, 20, "local image / glossy duotone tools", 3);
    label(&mut sheet, 24, 55, "each tool - dark 24 48 / light 24 48 - resvg actual pixels", 2);
    for (i, (name, _)) in COLOR_ICONS.iter().enumerate() {
        let x = 16 + (i as u32 % cols) * 360;
        let y = 88 + (i as u32 / cols) * 104;
        label(&mut sheet, x + 8, y, name, 2);
        for (j, size) in [24, 48, 24, 48].into_iter().enumerate() {
            let bg = if j < 2 { [31, 32, 35, 255] } else { [242, 242, 244, 255] };
            let mut tile = image::RgbaImage::from_pixel(80, 72, image::Rgba(bg));
            let icon = crate::color_icons::raster(name, size, false).unwrap();
            let pixels: Vec<_> = icon.pixels.iter().flat_map(|p| p.to_srgba_unmultiplied()).collect();
            let icon = image::RgbaImage::from_raw(size, size, pixels).unwrap();
            image::imageops::overlay(&mut tile, &icon, i64::from((80 - size) / 2), i64::from((72 - size) / 2));
            image::imageops::overlay(&mut sheet, &tile, i64::from(x + j as u32 * 80), i64::from(y + 20));
        }
    }
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/design/tool-icons.png");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    sheet.save(&path).unwrap();
    println!("{} icons, 24/48 px on dark/light: {}", COLOR_ICONS.len(), path.display());
}
