//! Bounded brush-pack imports. Format references and fixture licences: see the brush report.
//! No archive entries are written to disk and no resources outside the supplied pack are read.

use photocraft_paint::{BrushPreset, BrushSettings, GrayTile, TipShape};
use photocraft_raster::Interrupt;
use std::collections::{BTreeMap, HashSet};

pub const EXTENSIONS: &[&str] = &["abr", "gbr", "gih", "vbr", "bundle", "kpp", "brushset", "brush", "myb", "afbrushes"];
pub const MAX_FILE: usize = 128 << 20;
pub(crate) const MAX_ENTRY: usize = 32 << 20;
pub(crate) const MAX_TOTAL: usize = 256 << 20;
pub(crate) const MAX_ENTRIES: usize = 4096;
pub(crate) const MAX_BRUSHES: usize = 2048;
pub(crate) const MAX_EDGE: u32 = 8192;
pub(crate) const MAX_PIXELS: usize = 16 << 20;

#[derive(Debug, Default)]
pub struct BrushImport {
    pub presets: Vec<BrushPreset>,
    pub skipped: Vec<String>,
    pub warnings: Vec<String>,
}
impl BrushImport {
    pub fn approximate(&self) -> usize {
        self.presets.iter().filter(|p| !p.brush.import_notes.is_empty()).count()
    }
    pub(crate) fn add(&mut self, preset: BrushPreset) -> Result<(), String> {
        if self.presets.len() >= MAX_BRUSHES {
            return Err("brush count limit exceeded".into());
        }
        let used = self.presets.iter().chain(std::iter::once(&preset)).fold(0usize, |sum, p| sum.saturating_add(artwork_cost(p)));
        if used > MAX_TOTAL {
            return Err("decoded brush artwork budget exceeded".into());
        }
        self.presets.push(preset);
        Ok(())
    }
}

pub type SvgTip<'a> = dyn Fn(&[u8]) -> Result<GrayTile, String> + 'a;

pub fn read(bytes: &[u8], extension: &str, group: &str, ctl: &Interrupt<'_>) -> Result<BrushImport, String> {
    read_with_svg(bytes, extension, group, ctl, &|_| Err("SVG tips require the application SVG renderer".into()))
}

pub fn read_with_svg(bytes: &[u8], extension: &str, group: &str, ctl: &Interrupt<'_>, svg: &SvgTip<'_>) -> Result<BrushImport, String> {
    if group.len() > 1024 {
        return Err("brush group name exceeds limit".into());
    }
    if bytes.len() > MAX_FILE {
        return Err("brush file exceeds 128 MiB limit".into());
    }
    ctl.check().map_err(|e| e.to_string())?;
    let out = match extension.to_ascii_lowercase().as_str() {
        "abr" => {
            let a = crate::abr_map::read_abr_with(bytes, group, ctl)?;
            BrushImport { presets: a.presets, warnings: a.warnings, ..Default::default() }
        }
        "gbr" => BrushImport { presets: vec![super::brush_import_gimp::gbr(bytes, group)?.0], ..Default::default() },
        "gih" => super::brush_import_gimp::gih(bytes, group, ctl)?,
        "vbr" => BrushImport { presets: vec![super::brush_import_gimp::vbr(bytes, group)?], ..Default::default() },
        "myb" => BrushImport { presets: vec![super::brush_import_mypaint::read(bytes, group)?], ..Default::default() },
        "brush" | "brushset" => super::brush_import_procreate::read(bytes, group, ctl)?,
        "bundle" => super::brush_import_krita::bundle(bytes, group, ctl, svg)?,
        "kpp" => {
            let mut out = BrushImport::default();
            match super::brush_import_krita::kpp_variants(bytes, group, &BTreeMap::new(), svg) {
                Ok(presets) => {
                    for p in presets {
                        out.add(p)?;
                    }
                }
                Err(e) => out.skipped.push(e),
            }
            out
        }
        "afbrushes" => return Err("Affinity brush files can't be read yet".into()),
        other => return Err(format!("unsupported brush extension: {other}")),
    };
    ctl.check().map_err(|e| e.to_string())?;
    // Aggregate decoded artwork budget, in addition to per-resource bounds.
    let used: usize = out
        .presets
        .iter()
        .map(|p| {
            let tip = match &p.brush.tip {
                TipShape::Sampled(g) => g.data.len() * 2,
                _ => 0,
            };
            let texture = match &p.brush.texture.pattern {
                photocraft_paint::Pattern::Tile(g) if p.brush.texture.enabled => g.data.len() * 2,
                _ => 0,
            };
            tip.saturating_add(texture).saturating_add(p.brush.imported_images.iter().map(|i| i.png.len()).sum::<usize>())
        })
        .sum();
    if used > MAX_TOTAL {
        return Err("decoded brush artwork budget exceeded".into());
    }
    Ok(out)
}

pub(crate) fn preset(name: &str, group: &str, brush: BrushSettings) -> BrushPreset {
    BrushPreset {
        name: if name.trim().is_empty() { group.into() } else { name.trim().chars().take(256).collect() },
        group: group.into(),
        brush,
        builtin: false,
    }
}
pub(crate) fn dimensions(w: u32, h: u32) -> Result<usize, String> {
    let n = u64::from(w) * u64::from(h);
    if w == 0 || h == 0 || w > MAX_EDGE || h > MAX_EDGE || n > MAX_PIXELS as u64 {
        return Err(format!("brush dimensions {w}x{h} exceed limits"));
    }
    Ok(n as usize)
}
pub(crate) fn image(bytes: &[u8]) -> Result<photocraft_codecs::Image, String> {
    if bytes.len() > MAX_ENTRY {
        return Err("brush image exceeds entry limit".into());
    }
    photocraft_codecs::decode_with(
        bytes,
        &photocraft_codecs::DecodeOptions {
            limits: photocraft_codecs::Limits { max_width: MAX_EDGE, max_height: MAX_EDGE, max_pixels: MAX_PIXELS as u64, max_alloc: 128 << 20 },
            ..Default::default()
        },
    )
    .map_err(|e| e.to_string())
}
pub(crate) fn image_mask(bytes: &[u8], invert: bool) -> Result<GrayTile, String> {
    let im = image(bytes)?;
    let (w, h) = im.dimensions();
    dimensions(w, h)?;
    let mut data = Vec::with_capacity(w as usize * h as usize);
    for px in im.to_rgba16().as_chunks::<4>().0.iter() {
        let l = (u32::from(px[0]) * 2126 + u32::from(px[1]) * 7152 + u32::from(px[2]) * 722 + 5000) / 10000;
        let l = if invert { 65535 - l } else { l };
        data.push(((l * u32::from(px[3]) + 32767) / 65535) as u16);
    }
    Ok(GrayTile { width: w, height: h, data })
}

pub(crate) fn archive(bytes: &[u8], ctl: &Interrupt<'_>) -> Result<BTreeMap<String, Vec<u8>>, String> {
    let zip = photocraft_format::zip::ZipReader::new(bytes).map_err(|e| e.to_string())?;
    if zip.entries.len() > MAX_ENTRIES {
        return Err("zip entry count exceeds limit".into());
    }
    let mut names = HashSet::new();
    let mut total = 0usize;
    let mut out = BTreeMap::new();
    for (i, e) in zip.entries.iter().enumerate() {
        ctl.check().map_err(|e| e.to_string())?;
        let path = e.name.trim_end_matches('/');
        if path.is_empty()
            || path.len() > 1024
            || path.contains(['\\', '\0', ':'])
            || path.starts_with('/')
            || path.split('/').any(|s| s.is_empty() || s == "." || s == "..")
        {
            return Err(format!("unsafe zip path: {}", e.name));
        }
        if !names.insert(e.name.to_ascii_lowercase()) {
            return Err(format!("duplicate zip path: {}", e.name));
        }
        if e.name.ends_with('/') {
            continue;
        }
        let b = zip.read(e, MAX_ENTRY.min(MAX_TOTAL.saturating_sub(total))).map_err(|e| e.to_string())?;
        total = total.checked_add(b.len()).filter(|n| *n <= MAX_TOTAL).ok_or("zip expansion budget exceeded")?;
        out.insert(e.name.clone(), b);
        ctl.progress(i as f32 / zip.entries.len().max(1) as f32 * 0.3);
    }
    Ok(out)
}

pub(crate) fn finite(s: &str, what: &str) -> Result<f32, String> {
    s.trim().parse::<f32>().ok().filter(|v| v.is_finite()).ok_or_else(|| format!("invalid {what}"))
}

fn artwork_cost(p: &BrushPreset) -> usize {
    let tip = match &p.brush.tip {
        TipShape::Sampled(g) => g.data.len() * 2,
        _ => 0,
    };
    let texture = match &p.brush.texture.pattern {
        photocraft_paint::Pattern::Tile(g) if p.brush.texture.enabled => g.data.len() * 2,
        _ => 0,
    };
    tip.saturating_add(texture).saturating_add(p.brush.imported_images.iter().map(|i| i.png.len()).sum::<usize>())
}

#[cfg(test)]
#[path = "brush_import_tests.rs"]
mod tests;
