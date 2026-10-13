//! Bounded Krita colorset.xml codec (KPL ZIP container is handled by the caller's
//! existing ZIP reader). Schema: Krita KoColorSet::saveKplGroup/loadKplGroup.
use crate::{
    Color, ColorMode,
    ase::{Group, MAX_BYTES, MAX_ENTRIES, MAX_NAME, Palette, ReadResult, Swatch},
};
use std::collections::BTreeMap;

fn valid_name(s: &str) -> Result<(), String> {
    if s.encode_utf16().count() > MAX_NAME || s.chars().any(|c| c == '\0' || (c < ' ' && !matches!(c, '\t' | '\n' | '\r'))) {
        return Err("invalid KPL name".into());
    }
    Ok(())
}
fn unescape(s: &str) -> Result<String, String> {
    let mut out = String::new();
    let mut rest = s;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        rest = &rest[i + 1..];
        let end = rest.find(';').ok_or("unterminated XML entity")?;
        let c = match &rest[..end] {
            "amp" => '&',
            "lt" => '<',
            "gt" => '>',
            "quot" => '"',
            "apos" => '\'',
            n if n.starts_with("#x") => char::from_u32(u32::from_str_radix(&n[2..], 16).map_err(|_| "invalid XML entity")?).ok_or("invalid XML scalar")?,
            n if n.starts_with('#') => char::from_u32(n[1..].parse().map_err(|_| "invalid XML entity")?).ok_or("invalid XML scalar")?,
            _ => return Err("unsupported XML entity".into()),
        };
        out.push(c);
        rest = &rest[end + 1..];
    }
    out.push_str(rest);
    valid_name(&out)?;
    Ok(out)
}
fn escape(s: &str) -> Result<String, String> {
    valid_name(s)?;
    Ok(s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;").replace('\'', "&apos;"))
}
/// Small XML tokenizer for this attribute-only schema. No DTD, external entities,
/// text nodes, CDATA or recursive expansion; quotes, duplicate attributes and
/// matching close tags are validated. All input is bounded before tokenization.
fn tag(s: &str) -> Result<(&str, BTreeMap<String, String>), String> {
    let end = s.find(char::is_whitespace).unwrap_or(s.len());
    let name = &s[..end];
    if name.is_empty() || !name.bytes().all(|c| c.is_ascii_alphanumeric() || matches!(c, b'_' | b'-' | b':')) {
        return Err("invalid XML tag".into());
    }
    let mut attrs = BTreeMap::new();
    let mut rest = s[end..].trim();
    while !rest.is_empty() {
        if attrs.len() >= 32 {
            return Err("XML attribute budget exceeded".into());
        }
        let end = rest.find(|c: char| c.is_whitespace() || c == '=').ok_or("missing XML attribute value")?;
        let key = &rest[..end];
        if key.is_empty() || !key.bytes().all(|c| c.is_ascii_alphanumeric() || matches!(c, b'_' | b'-' | b':')) {
            return Err("invalid XML attribute".into());
        }
        rest = rest[end..].trim_start().strip_prefix('=').ok_or("missing XML equals")?.trim_start();
        let quote = rest.chars().next().filter(|c| matches!(c, '\'' | '"')).ok_or("missing XML quote")?;
        rest = &rest[1..];
        let end = rest.find(quote).ok_or("unterminated XML attribute")?;
        if rest[..end].contains('<') {
            return Err("invalid XML attribute value".into());
        }
        if attrs.insert(key.into(), unescape(&rest[..end])?).is_some() {
            return Err("duplicate XML attribute".into());
        }
        rest = &rest[end + 1..];
        if !rest.is_empty() && !rest.starts_with(char::is_whitespace) {
            return Err("XML attributes need whitespace".into());
        }
        rest = rest.trim_start();
    }
    Ok((name, attrs))
}
fn component(a: &BTreeMap<String, String>, k: &str, min: f32, max: f32) -> Result<f32, String> {
    let v: f32 = a.get(k).ok_or_else(|| format!("missing KPL {k}"))?.parse().map_err(|_| "invalid KPL number")?;
    if !v.is_finite() || !(min..=max).contains(&v) {
        return Err("KPL component outside colour range".into());
    }
    Ok(v)
}
pub fn read_xml(bytes: &[u8]) -> Result<ReadResult, String> {
    read_xml_with_profiles(bytes, |_, _| Ok(None))
}
/// A container host can resolve embedded ICC profiles without a ZIP dependency
/// here. Returning None preserves the working-space interpretation and warning.
pub fn read_xml_with_profiles(bytes: &[u8], mut profile: impl FnMut(Color, &str) -> Result<Option<Color>, String>) -> Result<ReadResult, String> {
    if bytes.len() > MAX_BYTES {
        return Err("KPL XML byte budget exceeded".into());
    }
    let mut rest = std::str::from_utf8(bytes).map_err(|_| "KPL XML is not UTF-8")?.trim_start_matches('\u{feff}').trim();
    let mut stack = Vec::<String>::new();
    let mut root = false;
    let mut closed = false;
    let mut version = String::new();
    let mut group: Option<usize> = None;
    let mut pending: Option<Swatch> = None;
    let mut has_color = false;
    let mut out = ReadResult { palette: Palette::default(), warnings: vec![] };
    let mut entries = 0usize;
    let mut tokens = 0;
    while !rest.is_empty() {
        tokens += 1;
        if tokens > MAX_ENTRIES * 8 {
            return Err("KPL XML token budget exceeded".into());
        }
        if rest.starts_with("<?xml") && !root {
            let end = rest.find("?>").ok_or("unterminated XML declaration")?;
            rest = rest[end + 2..].trim_start();
            continue;
        }
        if rest.starts_with("<!--") {
            let end = rest.find("-->").ok_or("unterminated XML comment")?;
            rest = rest[end + 3..].trim_start();
            continue;
        }
        rest = rest.strip_prefix('<').ok_or("unexpected XML text")?;
        let mut quote = None;
        let end = rest
            .char_indices()
            .find_map(|(i, c)| {
                if let Some(q) = quote {
                    if c == q {
                        quote = None;
                    }
                } else if matches!(c, '\'' | '"') {
                    quote = Some(c);
                } else if c == '>' {
                    return Some(i);
                }
                None
            })
            .ok_or("unterminated XML tag")?;
        let raw = rest[..end].trim();
        rest = rest[end + 1..].trim_start();
        if let Some(name) = raw.strip_prefix('/') {
            let name = name.trim();
            if stack.pop().as_deref() != Some(name) {
                return Err("mismatched XML close tag".into());
            }
            match name {
                "ColorSetEntry" => {
                    if !has_color {
                        return Err("KPL entry has no colour".into());
                    }
                    let w = pending.take().ok_or("missing KPL entry")?;
                    if let Some(i) = group {
                        out.palette.groups[i].swatches.push(w);
                    } else {
                        out.palette.swatches.push(w);
                    }
                }
                "Group" => group = None,
                "ColorSet" => closed = true,
                _ => {}
            }
            continue;
        }
        if closed {
            return Err("multiple XML roots".into());
        }
        let empty = raw.ends_with('/');
        let raw = raw.strip_suffix('/').unwrap_or(raw).trim_end();
        let (name, a) = tag(raw)?;
        let parent = stack.last().map(String::as_str);
        match name {
            "ColorSet" if !root && parent.is_none() => {
                root = true;
                version = a.get("version").cloned().unwrap_or_else(|| "1.0".into());
                if !matches!(version.as_str(), "1.0" | "2.0") {
                    return Err("unsupported KPL version".into());
                }
            }
            "Group" if parent == Some("ColorSet") => {
                entries += 1;
                group = Some(out.palette.groups.len());
                out.palette.groups.push(Group { name: a.get("name").cloned().unwrap_or_else(|| "Colour Group".into()), swatches: vec![] });
            }
            "ColorSetEntry" if matches!(parent, Some("ColorSet" | "Group")) && !empty => {
                entries += 1;
                has_color = false;
                let spot = match a.get("spot").map(String::as_str) {
                    None | Some("false" | "0") => false,
                    Some("true" | "1") => true,
                    _ => return Err("invalid KPL spot flag".into()),
                };
                pending = Some(Swatch { name: a.get("name").cloned().unwrap_or_else(|| "Colour".into()), color: Color::BLACK, global: spot, spot });
            }
            "Position" if parent == Some("ColorSetEntry") && empty => {}
            "metadata" if parent == Some("ColorSetEntry") && empty => {
                out.warnings.push("KPL colour metadata is not stored in swatches".into());
            }
            "sRGB" | "RGB" | "CMYK" | "Lab" | "Gray" | "GRAY" if parent == Some("ColorSetEntry") && empty => {
                if has_color {
                    return Err("multiple KPL entry colours".into());
                }
                let mut c = match name {
                    "RGB" | "sRGB" => Color::rgb(component(&a, "r", 0., 1.)?, component(&a, "g", 0., 1.)?, component(&a, "b", 0., 1.)?),
                    "CMYK" => Color {
                        mode: ColorMode::Cmyk,
                        c: [component(&a, "c", 0., 1.)?, component(&a, "m", 0., 1.)?, component(&a, "y", 0., 1.)?, component(&a, "k", 0., 1.)?],
                        alpha: 1.,
                    },
                    "Lab" => {
                        let (l, aa, b) = if version == "1.0" {
                            let ab = |v: f32| if v <= 0.5 { (v - 0.5) * 256. } else { (v - 0.5) * 254. };
                            (component(&a, "L", 0., 1.)? * 100., ab(component(&a, "a", 0., 1.)?), ab(component(&a, "b", 0., 1.)?))
                        } else {
                            (component(&a, "L", 0., 100.)?, component(&a, "a", -128., 127.)?, component(&a, "b", -128., 127.)?)
                        };
                        Color { mode: ColorMode::Lab, c: [l / 100., (aa + 128.) / 255., (b + 128.) / 255., 0.], alpha: 1. }
                    }
                    _ => Color::gray(component(&a, "g", 0., 1.)?),
                };
                if let Some(space) = a.get("space").filter(|v| !v.is_empty()) {
                    if let Some(converted) = profile(c, space)? {
                        c = converted;
                    } else {
                        out.warnings.push(format!("KPL profile '{space}': interpreted in the working colour space"));
                    }
                }
                pending.as_mut().ok_or("missing KPL entry")?.color = c;
                has_color = true;
            }
            _ => return Err(format!("unsupported KPL element {name}")),
        }
        if entries > MAX_ENTRIES {
            return Err("KPL entry budget exceeded".into());
        }
        if !empty {
            stack.push(name.into());
            if stack.len() > 4 {
                return Err("KPL XML depth budget exceeded".into());
            }
        } else if name == "ColorSet" {
            closed = true;
        } else if name == "Group" {
            group = None;
        }
    }
    if !root || !closed || !stack.is_empty() {
        return Err("incomplete KPL XML".into());
    }
    Ok(out)
}
fn write_entries(text: &mut String, entries: &[Swatch]) -> Result<(), String> {
    for (i, w) in entries.iter().enumerate() {
        if w.color.alpha != 1. || w.color.c.iter().any(|x| !x.is_finite() || !(0.0..=1.0).contains(x)) {
            return Err("KPL requires opaque bounded colours".into());
        }
        text.push_str(&format!("<ColorSetEntry name=\"{}\" spot=\"{}\" bitdepth=\"F32\">", escape(&w.name)?, w.spot));
        let c = w.color.c;
        text.push_str(&match w.color.mode {
            ColorMode::Rgb => format!("<sRGB r=\"{}\" g=\"{}\" b=\"{}\"/>", c[0], c[1], c[2]),
            ColorMode::Cmyk => format!("<CMYK c=\"{}\" m=\"{}\" y=\"{}\" k=\"{}\"/>", c[0], c[1], c[2], c[3]),
            ColorMode::Lab => format!("<Lab L=\"{}\" a=\"{}\" b=\"{}\"/>", c[0] * 100., c[1] * 255. - 128., c[2] * 255. - 128.),
            ColorMode::Grayscale => format!("<Gray g=\"{}\"/>", c[0]),
            _ => return Err("unsupported KPL output model".into()),
        });
        text.push_str(&format!("<Position row=\"{}\" column=\"{}\"/></ColorSetEntry>\n", i / 8, i % 8));
    }
    Ok(())
}
pub fn write_xml(p: &Palette) -> Result<Vec<u8>, String> {
    if p.swatches.len() + p.groups.iter().map(|g| 1 + g.swatches.len()).sum::<usize>() > MAX_ENTRIES {
        return Err("KPL entry budget exceeded".into());
    }
    let mut text = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<ColorSet version=\"2.0\" name=\"Local Image\" columns=\"8\" rows=\"{}\">\n",
        p.swatches.len().div_ceil(8)
    );
    write_entries(&mut text, &p.swatches)?;
    for g in &p.groups {
        text.push_str(&format!("<Group name=\"{}\" rows=\"{}\">\n", escape(&g.name)?, g.swatches.len().div_ceil(8)));
        write_entries(&mut text, &g.swatches)?;
        text.push_str("</Group>\n");
    }
    text.push_str("</ColorSet>\n");
    if text.len() > MAX_BYTES {
        return Err("KPL byte budget exceeded".into());
    }
    Ok(text.into_bytes())
}

/// Krita profiles.xml lists the archive member for each named colour profile.
/// Names are opaque archive keys, never filesystem paths.
pub fn read_profiles_xml(bytes: &[u8]) -> Result<Vec<(String, String)>, String> {
    if bytes.len() > MAX_BYTES {
        return Err("KPL profile XML byte budget exceeded".into());
    }
    let mut rest = std::str::from_utf8(bytes).map_err(|_| "KPL profile XML is not UTF-8")?.trim();
    if rest.starts_with("<?xml") {
        let end = rest.find("?>").ok_or("invalid profile XML declaration")?;
        rest = rest[end + 2..].trim_start();
    }
    rest = rest.strip_prefix("<Profiles").ok_or("missing Profiles root")?;
    let end = rest.find('>').ok_or("unterminated Profiles root")?;
    if !rest[..end].trim().is_empty() && rest[..end].trim() != "/" {
        return Err("invalid Profiles root".into());
    }
    if rest[..end].trim() == "/" {
        if !rest[end + 1..].trim().is_empty() {
            return Err("trailing profile XML".into());
        }
        return Ok(vec![]);
    }
    rest = rest[end + 1..].trim_start();
    let mut out = vec![];
    loop {
        if let Some(tail) = rest.strip_prefix("</Profiles>") {
            if !tail.trim().is_empty() {
                return Err("trailing profile XML".into());
            }
            return Ok(out);
        }
        rest = rest.strip_prefix('<').ok_or("invalid profile XML")?;
        let mut quote = None;
        let end = rest
            .char_indices()
            .find_map(|(i, c)| {
                if let Some(q) = quote {
                    if c == q {
                        quote = None;
                    }
                } else if matches!(c, '\'' | '"') {
                    quote = Some(c);
                } else if c == '>' {
                    return Some(i);
                }
                None
            })
            .ok_or("unterminated profile XML tag")?;
        let body = rest[..end].trim().strip_suffix('/').ok_or("Profile must be empty")?.trim_end();
        let (name, attrs) = tag(body)?;
        if name != "Profile" {
            return Err("unsupported profile XML element".into());
        }
        let profile = attrs.get("name").ok_or("Profile needs name")?.clone();
        let file = attrs.get("filename").ok_or("Profile needs filename")?.clone();
        if profile.is_empty() || file.is_empty() || out.iter().any(|(n, _)| *n == profile) {
            return Err("invalid or duplicate KPL profile".into());
        }
        out.push((profile, file));
        if out.len() > MAX_ENTRIES {
            return Err("KPL profile entry budget exceeded".into());
        }
        rest = rest[end + 1..].trim_start();
    }
}
