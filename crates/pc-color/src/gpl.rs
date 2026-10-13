//! GIMP palette text. See GIMP app/core/gimppalette-load.c.
use crate::{
    Color,
    ase::{Group, MAX_BYTES, MAX_ENTRIES, MAX_NAME, Palette, ReadResult, Swatch},
};
pub fn read(bytes: &[u8]) -> Result<ReadResult, String> {
    if bytes.len() > MAX_BYTES {
        return Err("GPL byte budget exceeded".into());
    }
    let text = std::str::from_utf8(bytes).map_err(|_| "GPL is not UTF-8")?;
    let mut lines = text.trim_start_matches('\u{feff}').lines();
    if lines.next().map(str::trim) != Some("GIMP Palette") {
        return Err("missing GIMP Palette header".into());
    }
    let mut group = Group { name: "Imported Palette".into(), swatches: vec![] };
    for line in lines {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(name) = line.strip_prefix("Name:") {
            group.name = name.trim().into();
            continue;
        }
        if let Some(cols) = line.strip_prefix("Columns:") {
            let _: u16 = cols.trim().parse().map_err(|_| "invalid GPL columns")?;
            continue;
        }
        let mut rest = line;
        let mut rgb = [0u8; 3];
        for c in &mut rgb {
            let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
            *c = rest[..end].parse().map_err(|_| "invalid GPL RGB component")?;
            rest = rest[end..].trim_start();
        }
        if group.swatches.len() + 1 >= MAX_ENTRIES {
            return Err("GPL entry budget exceeded".into());
        }
        let name = if rest.is_empty() { format!("Colour {}", group.swatches.len() + 1) } else { rest.into() };
        if name.encode_utf16().count() > MAX_NAME || name.contains('\0') {
            return Err("invalid GPL name".into());
        }
        group.swatches.push(Swatch {
            name,
            color: Color::rgb(f32::from(rgb[0]) / 255., f32::from(rgb[1]) / 255., f32::from(rgb[2]) / 255.),
            global: false,
            spot: false,
        });
    }
    if group.name.encode_utf16().count() > MAX_NAME || group.name.contains('\0') {
        return Err("invalid GPL group name".into());
    }
    Ok(ReadResult { palette: Palette { swatches: vec![], groups: vec![group] }, warnings: vec![] })
}
pub fn write(p: &Palette) -> Result<Vec<u8>, String> {
    let entries: Vec<_> = p.swatches.iter().chain(p.groups.iter().flat_map(|g| &g.swatches)).collect();
    if entries.len() + 1 > MAX_ENTRIES {
        return Err("GPL entry budget exceeded".into());
    }
    let mut text = "GIMP Palette\nName: Local Image Palette\nColumns: 8\n#\n".to_owned();
    for w in entries {
        if w.name.encode_utf16().count() > MAX_NAME || w.name.contains(['\0', '\n', '\r']) {
            return Err("GPL names cannot contain newlines".into());
        }
        if w.color.alpha != 1. || w.color.c.iter().any(|x| !x.is_finite() || !(0.0..=1.0).contains(x)) {
            return Err("GPL requires opaque bounded colours".into());
        }
        let [r, g, b, _] = w.color.to_rgba8();
        text.push_str(&format!("{r:3} {g:3} {b:3}\t{}\n", w.name));
    }
    if text.len() > MAX_BYTES {
        return Err("GPL byte budget exceeded".into());
    }
    Ok(text.into_bytes())
}
