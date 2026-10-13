//! GIMP GBR/GIH/VBR, per developer.gimp.org/core/standards/{gbr,gih,vbr}/ and
//! GIMP app/core/gimpbrush-generated.c. Grayscale GBR stores coverage (white = paint).
use super::brush_import::{self as common, BrushImport};
use photocraft_paint::{BrushSettings, Control, Dynamic, GrayTile, TipShape};
use photocraft_raster::Interrupt;

fn be(b: &[u8], i: usize) -> Result<u32, String> {
    b.get(i..i + 4).and_then(|s| s.try_into().ok()).map(u32::from_be_bytes).ok_or_else(|| "truncated GBR header".into())
}
pub(crate) fn gbr(bytes: &[u8], group: &str) -> Result<(photocraft_paint::BrushPreset, usize), String> {
    let header = be(bytes, 0)? as usize;
    let version = be(bytes, 4)?;
    let base = match version {
        1 => 20,
        2 => 28,
        _ => return Err(format!("unsupported GBR version {version}")),
    };
    if header < base || header > base + 4096 || header > bytes.len() {
        return Err("invalid GBR header length".into());
    }
    let (w, h, channels) = (be(bytes, 8)?, be(bytes, 12)?, be(bytes, 16)?);
    let n = common::dimensions(w, h)?;
    if channels != 1 && channels != 4 {
        return Err("GBR depth must be grayscale or RGBA".into());
    }
    if version == 2 && be(bytes, 20)? != 0x47494d50 {
        return Err("bad GBR magic".into());
    }
    let len = n.checked_mul(channels as usize).ok_or("GBR size overflow")?;
    let end = header.checked_add(len).ok_or("GBR size overflow")?;
    let body = bytes.get(header..end).ok_or("truncated GBR pixels")?;
    let name = String::from_utf8_lossy(&bytes[base..header]).trim_end_matches('\0').to_string();
    let spacing = if version == 2 { be(bytes, 24)? as f32 / 100.0 } else { 0.25 };
    let mut brush = BrushSettings { size: w.max(h) as f32, spacing: spacing.clamp(0.01, 10.0), pressure_size: false, ..Default::default() };
    let data = if channels == 1 {
        body.iter().map(|&p| u16::from(p) * 257).collect()
    } else {
        brush.import_notes.push("RGBA artwork imported as an alpha tip; source colours are retained in the original PNG".into());
        // Preserve colour artwork exactly as a lossless PNG alongside the coverage mask.
        let im = photocraft_codecs::Image::from_raw(w, h, photocraft_codecs::ChannelLayout::Rgba, photocraft_codecs::SampleType::U8, body.to_vec())
            .map_err(|e| e.to_string())?;
        let png = photocraft_codecs::encode(&im, photocraft_codecs::Format::Png, &Default::default()).map_err(|e| e.to_string())?;
        brush.imported_images.push(photocraft_paint::ImportedBrushImage { name: "RGBA tip".into(), png: photocraft_paint::tile::b64_encode(&png) });
        body.as_chunks::<4>().0.iter().map(|p| u16::from(p[3]) * 257).collect()
    };
    brush.size = brush.size.min(photocraft_paint::MAX_BRUSH_SIZE);
    brush.tip = TipShape::Sampled(GrayTile { width: w, height: h, data });
    Ok((common::preset(&name, group, brush), end))
}

pub(crate) fn gih(bytes: &[u8], group: &str, ctl: &Interrupt<'_>) -> Result<BrushImport, String> {
    let line = |from: usize| {
        bytes.get(from..).and_then(|s| s.iter().position(|&c| c == b'\n')).filter(|n| *n <= 8192).map(|n| from + n).ok_or("invalid GIH text header")
    };
    let a = line(0)?;
    let b = line(a + 1)?;
    let name = String::from_utf8_lossy(&bytes[..a]);
    let params = std::str::from_utf8(&bytes[a + 1..b]).map_err(|_| "invalid GIH parameters")?;
    let count = params
        .split_whitespace()
        .next()
        .and_then(|s| s.parse::<usize>().ok())
        .filter(|n| *n > 0 && *n <= common::MAX_BRUSHES)
        .ok_or("invalid GIH brush count")?;
    let fields: std::collections::BTreeMap<_, _> = params.split_whitespace().filter_map(|s| s.split_once(':')).collect();
    let modes: Vec<_> = fields.iter().filter(|(k, _)| k.starts_with("sel")).map(|(_, v)| *v).collect();
    let spacing = fields.get("step").map(|s| common::finite(s, "GIH step")).transpose()?.map(|s| s / 100.0);
    let mut pos = b + 1;
    let mut out = BrushImport::default();
    let mut total = 0usize;
    for i in 0..count {
        ctl.check().map_err(|e| e.to_string())?;
        // GIMP exports each grid cell as its own GBR, not a composite sheet.
        let (mut p, n) = gbr(bytes.get(pos..).ok_or("truncated GIH")?, group)?;
        pos = pos.checked_add(n).ok_or("GIH overflow")?;
        total = total.checked_add(n).filter(|n| *n <= common::MAX_TOTAL).ok_or("GIH artwork budget exceeded")?;
        p.name = format!("{} · {}", name.trim(), i + 1);
        if let Some(spacing) = spacing {
            p.brush.spacing = spacing.clamp(0.01, 10.0);
        }
        if count > 1 {
            p.brush.import_notes.push(format!(
                "Image pipe cell {}/{}; {} selection imported as separately selectable tip variants",
                i + 1,
                count,
                modes.join(", ")
            ));
        }
        if modes.contains(&"angular") {
            p.brush.shape_dynamics.enabled = true;
            p.brush.shape_dynamics.angle = Dynamic::controlled(Control::Direction);
        }
        out.add(p)?;
        ctl.progress((i + 1) as f32 / count as f32);
    }
    if pos != bytes.len() {
        out.warnings.push("Trailing GIH data ignored".into());
    }
    Ok(out)
}

pub(crate) fn vbr(bytes: &[u8], group: &str) -> Result<photocraft_paint::BrushPreset, String> {
    if bytes.len() > 16384 {
        return Err("VBR header too large".into());
    }
    let text = std::str::from_utf8(bytes).map_err(|_| "invalid VBR UTF-8")?;
    let mut lines = text.lines().map(str::trim);
    if lines.next() != Some("GIMP-VBR") {
        return Err("bad VBR magic".into());
    }
    let v = lines.next().ok_or("missing VBR version")?;
    let shaped = match v {
        "1.0" => false,
        "1.5" => true,
        _ => return Err(format!("unsupported VBR version {v}")),
    };
    let name = lines.next().ok_or("missing VBR name")?;
    let shape = if shaped { lines.next().ok_or("missing VBR shape")? } else { "circle" };
    if !matches!(shape, "circle" | "square" | "diamond") {
        return Err("unknown VBR shape".into());
    }
    let mut number = |what| common::finite(lines.next().ok_or_else(|| format!("missing {what}"))?, what);
    let spacing = number("spacing")? / 100.0;
    let radius = number("radius")?;
    let spikes = if shaped { number("spikes")? } else { 2.0 };
    let hard = number("hardness")?;
    let aspect = number("aspect")?;
    let angle = number("angle")?;
    if !(0.0..=4000.0).contains(&radius)
        || radius == 0.0
        || !(0.0..=1.0).contains(&hard)
        || !(1.0..=1000.0).contains(&aspect)
        || !(2.0..=20.0).contains(&spikes)
        || spikes.fract() != 0.0
    {
        return Err("VBR parameters out of range".into());
    }
    let (sn, cs) = angle.to_radians().sin_cos();
    let short = radius / aspect;
    let half = if spikes > 2.0 {
        (radius * radius + short * short).sqrt()
    } else {
        match shape {
            "square" => (cs.abs() * radius + sn.abs() * short).max(sn.abs() * radius + cs.abs() * short),
            "diamond" => (cs.abs() * radius).max(sn.abs() * short).max(sn.abs() * radius).max(cs.abs() * short),
            _ => (cs * cs * radius * radius + sn * sn * short * short).sqrt().max((sn * sn * radius * radius + cs * cs * short * short).sqrt()),
        }
    };
    let size = half.ceil() as u32 * 2 + 1;
    common::dimensions(size, size)?;
    let center = (size - 1) as f32 / 2.0;
    // Generated tips are sampled so square, diamond and star geometry survives import.
    let tip = GrayTile::from_fn(size, size, |x, y| {
        let (x, y) = (x as f32 - center, y as f32 - center);
        let (mut u, mut v) = ((cs * x - sn * y) / radius, ((sn * x + cs * y) / radius).abs());
        if spikes > 2.0 {
            let sector = std::f32::consts::PI / spikes;
            let a = v.atan2(u);
            let a = (a + sector).rem_euclid(2.0 * sector) - sector;
            let r = u.hypot(v);
            u = r * a.cos();
            v = r * a.sin();
        }
        v *= aspect;
        let r = match shape {
            "square" => u.abs().max(v.abs()),
            "diamond" => u.abs() + v.abs(),
            _ => u.hypot(v),
        };
        if r >= 1.0 {
            0.0
        } else if hard >= 1.0 || r <= hard {
            1.0
        } else {
            let t = (1.0 - r) / (1.0 - hard);
            t * t * (3.0 - 2.0 * t)
        }
    });
    let brush = BrushSettings {
        size: size.min(photocraft_paint::MAX_BRUSH_SIZE as u32) as f32,
        tip: TipShape::Sampled(tip),
        hardness: hard,
        spacing: spacing.clamp(0.01, 10.0),
        pressure_size: false,
        import_notes: vec!["GIMP generated hardness profile mapped approximately to a sampled tip".into()],
        ..Default::default()
    };
    Ok(common::preset(name, group, brush))
}
