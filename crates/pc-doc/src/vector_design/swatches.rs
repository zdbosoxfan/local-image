//! Document palette and link data informed by VectorCraft `color/src/swatch.rs` and
//! `doc/src/swatches.rs`, d522c1d7be4035bd4f4a84cd6ebfca44f5155092 (MIT OR Apache-2.0).
//! Copyright (c) 2026 ArtCraft Team and the VectorCraft contributors.
//! Stable IDs make renaming independent of references. Document storage and paint-field
//! adapters live outside this module; it never stores palette data in XMP.
use super::exchange::{ExportedVector, ImportLimits, VectorWarning};
use crate::{Color, ColorMode, Fill, LayerId, SymbolId};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, HashSet};

pub const MAX_SWATCHES: usize = 4096;
pub const MAX_LINKS: usize = 100_000;
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct SwatchId(pub u64);
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Swatch {
    pub id: SwatchId,
    pub name: String,
    pub paint: Fill,
    #[serde(default)]
    pub global: bool,
    #[serde(default)]
    pub spot: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<u64>,
    /// Links on gradient-swatch stops. A global colour itself must be an unlinked solid.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub stops: Vec<StopLink>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SwatchGroup {
    pub id: u64,
    pub name: String,
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct SwatchRef {
    pub swatch: SwatchId,
    /// 0..=100 percent. The alpha of the base is retained; tint changes ink, not opacity.
    #[serde(default = "hundred")]
    pub tint: f32,
}
fn hundred() -> f32 {
    100.
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct StopLink {
    pub stop: usize,
    pub reference: SwatchRef,
}
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum PaintSlot {
    Fill,
    Stroke,
    Appearance(usize),
}
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PaintTarget {
    /// None=root layer tree, Some=symbol master tree. LayerId namespaces never alias.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub symbol: Option<SymbolId>,
    pub layer: LayerId,
    pub paint: PaintSlot,
    /// None=solid colour, Some=gradient stop index in stored order.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stop: Option<usize>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PaintLink {
    pub target: PaintTarget,
    pub reference: SwatchRef,
}
/// Portable snapshot of Document.swatches and optional references on solid paints/stops.
/// An adapter gathers/scatters links without requiring a new exhaustive legacy Fill variant.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SwatchBook {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub swatches: Vec<Swatch>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub groups: Vec<SwatchGroup>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub links: Vec<PaintLink>,
}
/// Application preferences wrap the same palette model plus panel choices.
/// Document palettes continue to use SwatchBook directly.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct UserSwatches {
    pub palette: SwatchBook,
    pub recent: Vec<Color>,
    pub list_view: bool,
    pub initialized: bool,
}
impl SwatchBook {
    pub fn is_empty(&self) -> bool {
        self.swatches.is_empty() && self.groups.is_empty() && self.links.is_empty()
    }
    pub fn swatch(&self, id: SwatchId) -> Option<&Swatch> {
        self.swatches.iter().find(|w| w.id == id)
    }
    pub fn next_id(&self) -> Result<u64, String> {
        self.swatches.iter().map(|w| w.id.0).chain(self.groups.iter().map(|g| g.id)).max().unwrap_or(0).checked_add(1).ok_or("swatch ID space exhausted".into())
    }
    pub fn free_name(&self, base: &str) -> String {
        let taken = |s: &str| self.swatches.iter().any(|w| w.name == s) || self.groups.iter().any(|g| g.name == s);
        if !taken(base) {
            return base.into();
        }
        let named = |suffix: &str| {
            let budget = 1024 - suffix.encode_utf16().count();
            let mut units = 0;
            let prefix: String = base
                .chars()
                .take_while(|c| {
                    units += c.len_utf16();
                    units <= budget
                })
                .collect();
            format!("{prefix}{suffix}")
        };
        (2..=MAX_SWATCHES + 2).map(|i| named(&format!(" {i}"))).find(|s| !taken(s)).unwrap_or_else(|| named(" copy"))
    }
    pub fn validate(&self) -> Result<(), String> {
        if self.swatches.len() + self.groups.len() > MAX_SWATCHES || self.links.len() > MAX_LINKS {
            return Err("document palette budget exceeded".into());
        }
        let mut ids = BTreeSet::new();
        let mut names = BTreeSet::new();
        for g in &self.groups {
            validate_name(&g.name)?;
            if g.id == 0 || !ids.insert(g.id) || !names.insert(g.name.as_str()) {
                return Err("duplicate palette ID/name".into());
            }
        }
        for w in &self.swatches {
            validate_name(&w.name)?;
            if w.id.0 == 0 || !ids.insert(w.id.0) || !names.insert(w.name.as_str()) {
                return Err("duplicate palette ID/name".into());
            }
            if w.group.is_some_and(|id| !self.groups.iter().any(|g| g.id == id)) {
                return Err("missing swatch group".into());
            }
            validate_paint(&w.paint)?;
            if (w.global || w.spot) && !matches!(w.paint, Fill::Solid(_)) {
                return Err("global/spot swatches must be solid".into());
            }
            if w.spot && !w.global {
                return Err("spot swatches must be global".into());
            }
            let mut stops = BTreeSet::new();
            for s in &w.stops {
                if !stops.insert(s.stop) || !matches!(&w.paint, Fill::Gradient { stops, .. } if s.stop < stops.len()) {
                    return Err("invalid swatch stop link".into());
                }
                self.resolve(s.reference)?;
            }
        }
        let mut targets = HashSet::new();
        for l in &self.links {
            if !targets.insert(&l.target) {
                return Err("duplicate global paint target".into());
            }
            self.resolve(l.reference)?;
        }
        Ok(())
    }
    /// Native loading, duplicate/paste and master cloning must remap references with layers.
    /// Missing IDs fail rather than leaving a link pointing at an unrelated object.
    pub fn remap_layers(&mut self, ids: &std::collections::HashMap<u64, LayerId>) -> Result<(), String> {
        let mut links = self.links.clone();
        for link in &mut links {
            link.target.layer = *ids.get(&link.target.layer.0).ok_or("missing colour-link layer remap")?;
        }
        self.links = links;
        Ok(())
    }
    pub fn resolve(&self, r: SwatchRef) -> Result<Color, String> {
        let w = self.swatch(r.swatch).filter(|w| w.global).ok_or("reference must name a global swatch")?;
        let Fill::Solid(c) = w.paint else {
            return Err("global swatch must be solid".into());
        };
        tint(c, r.tint)
    }
}
pub fn validate_name(s: &str) -> Result<(), String> {
    if s.trim().is_empty() || s.encode_utf16().count() > 1024 || s.contains('\0') { Err("palette name must contain 1..1024 characters".into()) } else { Ok(()) }
}
pub fn validate_color(c: Color) -> Result<(), String> {
    if c.c.iter().chain([&c.alpha]).any(|v| !v.is_finite() || !(0.0..=1.0).contains(v)) { Err("colour must be finite and bounded".into()) } else { Ok(()) }
}
pub fn validate_paint(p: &Fill) -> Result<(), String> {
    match p {
        Fill::Solid(c) => validate_color(*c),
        Fill::Gradient { stops, angle, scale, opacity_stops, midpoints, offset, .. } => {
            if stops.is_empty()
                || stops.len() > 1024
                || !angle.is_finite()
                || angle.abs() > 36000.
                || !scale.is_finite()
                || !(0.01..=10000.).contains(scale)
                || [offset.0, offset.1].iter().any(|v| !v.is_finite() || v.abs() > 100.)
                || opacity_stops.len() > 1024
                || midpoints.len() > 1024
                || opacity_stops.iter().any(|(t, a)| !t.is_finite() || !a.is_finite() || !(0.0..=1.0).contains(t) || !(0.0..=1.0).contains(a))
                || midpoints.iter().any(|v| !v.is_finite() || !(0.0..=1.0).contains(v))
            {
                return Err("gradient must be finite and bounded".into());
            }
            for (t, c) in stops {
                if !t.is_finite() || !(0.0..=1.0).contains(t) {
                    return Err("gradient stop outside 0..1".into());
                }
                validate_color(*c)?;
            }
            Ok(())
        }
        Fill::Pattern { .. } => Err("document swatches support solid colours and gradients".into()),
    }
}
/// Ink-space tint, matching VectorCraft Color::tinted: RGB/Gray mix with white,
/// CMYK scales inks, Lab mixes with paper white (L*=100,a*=b*=0). No gamma conversion.
pub fn tint(mut c: Color, percent: f32) -> Result<Color, String> {
    validate_color(c)?;
    if !percent.is_finite() || !(0.0..=100.0).contains(&percent) {
        return Err("tint must be 0..100 percent".into());
    }
    if percent == 100. {
        return Ok(c);
    }
    let t = percent / 100.;
    match c.mode {
        ColorMode::Cmyk => {
            c.c = c.c.map(|x| x * t);
        }
        ColorMode::Lab => {
            for (x, white) in c.c[..3].iter_mut().zip([1., 128. / 255., 128. / 255.]) {
                *x = white + (*x - white) * t;
            }
        }
        _ => {
            for x in &mut c.c[..c.mode.color_channels()] {
                *x = 1. - (1. - *x) * t;
            }
        }
    }
    Ok(c)
}
pub fn warning(feature: &str, message: impl Into<String>) -> VectorWarning {
    VectorWarning { feature: feature.into(), object: None, message: message.into() }
}
/// PSD always stores resolved plain colours; a PDF adapter opts in only when it actually
/// emits Separation colour spaces, never merely because spot flags are available.
pub fn export_warnings(book: &SwatchBook, separations: bool) -> Vec<VectorWarning> {
    let mut out = Vec::new();
    if !book.links.is_empty() {
        out.push(warning("global-colours", "Global colour links are resolved to local colours in this export"));
    }
    if !separations && book.swatches.iter().any(|w| w.spot) {
        out.push(warning("spot-colours", "Spot colours are exported as process colours; no Separation plates"));
    }
    out
}
pub fn import_ase(bytes: &[u8], limits: ImportLimits) -> Result<(SwatchBook, Vec<VectorWarning>), String> {
    if bytes.len() > limits.bytes {
        return Err("palette import byte budget exceeded".into());
    }
    let read = photocraft_color::ase::read(bytes)?;
    let count = read.palette.swatches.len() + read.palette.groups.iter().map(|g| g.swatches.len() + 1).sum::<usize>();
    if count > limits.objects {
        return Err("palette import object budget exceeded".into());
    }
    let mut book = SwatchBook::default();
    for (group_name, colors) in std::iter::once((None, read.palette.swatches)).chain(read.palette.groups.into_iter().map(|g| (Some(g.name), g.swatches))) {
        let group = if let Some(name) = group_name {
            let id = book.next_id()?;
            let name = book.free_name(if name.trim().is_empty() { "Colour Group" } else { &name });
            book.groups.push(SwatchGroup { id, name });
            Some(id)
        } else {
            None
        };
        for w in colors {
            let id = SwatchId(book.next_id()?);
            let name = book.free_name(if w.name.trim().is_empty() { "Colour" } else { &w.name });
            book.swatches.push(Swatch { id, name, paint: Fill::Solid(w.color), global: w.global, spot: w.spot, group, stops: Vec::new() });
        }
    }
    book.validate()?;
    Ok((book, read.warnings.into_iter().map(|m| warning("ase-unsupported", m)).collect()))
}
/// Adapt all exchange codecs through the established stable-ID swatch model.
pub fn from_palette(palette: photocraft_color::ase::Palette) -> Result<SwatchBook, String> {
    let bytes = photocraft_color::ase::write(&palette)?;
    import_ase(&bytes, ImportLimits::default()).map(|v| v.0)
}
pub fn to_palette(book: &SwatchBook) -> Result<photocraft_color::ase::ReadResult, String> {
    let output = export_ase(book)?;
    let mut result = photocraft_color::ase::read(&output.bytes)?;
    result.warnings.extend(output.warnings.into_iter().map(|w| w.message));
    Ok(result)
}
/// Move a stable ID before another swatch, or to the end of a folder.
/// Paint references survive because only order and folder membership change.
pub fn move_swatch(book: &mut SwatchBook, id: SwatchId, group: Option<u64>, before: Option<SwatchId>) -> Result<(), String> {
    if group.is_some_and(|g| !book.groups.iter().any(|v| v.id == g)) {
        return Err("missing destination group".into());
    }
    if before == Some(id) {
        return Ok(());
    }
    if before.is_some_and(|b| !book.swatches.iter().any(|w| w.id == b && w.group == group)) {
        return Err("missing destination swatch".into());
    }
    let index = book.swatches.iter().position(|w| w.id == id).ok_or("missing swatch")?;
    let mut w = book.swatches.remove(index);
    w.group = group;
    let at = before.and_then(|b| book.swatches.iter().position(|w| w.id == b)).unwrap_or(book.swatches.len());
    book.swatches.insert(at, w);
    Ok(())
}
pub fn export_ase(book: &SwatchBook) -> Result<ExportedVector, String> {
    book.validate()?;
    let mut palette = photocraft_color::ase::Palette::default();
    let mut warnings = Vec::new();
    for g in &book.groups {
        palette.groups.push(photocraft_color::ase::Group { name: g.name.clone(), swatches: Vec::new() });
    }
    for w in &book.swatches {
        let Fill::Solid(mut color) = w.paint else {
            warnings.push(warning("ase-gradient", format!("ASE omits gradient swatch {}", w.name)));
            continue;
        };
        if color.alpha != 1. {
            color.alpha = 1.;
            warnings.push(warning("ase-opacity", format!("ASE omits opacity of {}", w.name)));
        }
        let entry = photocraft_color::ase::Swatch { name: w.name.clone(), color, global: w.global, spot: w.spot };
        if let Some(i) = w.group.and_then(|id| book.groups.iter().position(|g| g.id == id)) {
            palette.groups[i].swatches.push(entry);
        } else {
            palette.swatches.push(entry);
        }
    }
    Ok(ExportedVector { bytes: photocraft_color::ase::write(&palette)?, warnings })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn colour39_duplicate_long_unicode_names_fit_ase_native_budget() {
        let base = "😀".repeat(512);
        let mut book = SwatchBook::default();
        book.groups.push(SwatchGroup { id: 1, name: base.clone() });
        let second = book.free_name(&base);
        assert!(second.ends_with(" 2"));
        assert!(second.encode_utf16().count() <= 1024);
        book.groups.push(SwatchGroup { id: 2, name: second.clone() });
        assert_ne!(book.free_name(&base), second);
        book.validate().unwrap();
        let bytes = export_ase(&book).unwrap().bytes;
        assert_eq!(import_ase(&bytes, ImportLimits::default()).unwrap().0, book);
    }
    #[test]
    fn colour39_defaults_tint_and_native_data_roundtrip() {
        assert_eq!(serde_json::from_str::<SwatchBook>("{}").unwrap(), SwatchBook::default());
        let c = Color::rgba(0.2, 0.4, 0.6, 0.3);
        assert_eq!(tint(c, 50.).unwrap(), Color::rgba(0.6, 0.7, 0.8, 0.3));
        let cmyk = Color { mode: ColorMode::Cmyk, c: [0.2, 0.4, 0.8, 0.6], alpha: 1. };
        assert_eq!(tint(cmyk, 50.).unwrap().c, [0.1, 0.2, 0.4, 0.3]);
        assert!(tint(c, f32::NAN).is_err());
        let mut book = SwatchBook::default();
        book.swatches.push(Swatch { id: SwatchId(1), name: "Brand".into(), paint: Fill::Solid(c), global: true, spot: true, group: None, stops: vec![] });
        book.validate().unwrap();
        assert_eq!(book, serde_json::from_str(&serde_json::to_string(&book).unwrap()).unwrap());
    }
    #[test]
    fn colour39_ase_adapter_groups_and_losses_are_explicit() {
        let mut book = SwatchBook::default();
        book.groups.push(SwatchGroup { id: 1, name: "Brand".into() });
        book.swatches.push(Swatch {
            id: SwatchId(2),
            name: "Ink".into(),
            paint: Fill::Solid(Color::BLACK),
            global: true,
            spot: true,
            group: Some(1),
            stops: vec![],
        });
        let exported = export_ase(&book).unwrap();
        assert_eq!(import_ase(&exported.bytes, ImportLimits::default()).unwrap().0, book);
        assert_eq!(export_warnings(&book, false)[0].feature, "spot-colours");
        assert!(export_warnings(&book, true).is_empty());
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod dsfix_tests {
    use super::*;
    #[test]
    fn dsfix_duplicate_maximum_utf16_name_stays_valid() {
        for name in ["a".repeat(1024), "😀".repeat(512), format!("a{}", "😀".repeat(511))] {
            let mut book = SwatchBook { groups: vec![SwatchGroup { id: 1, name: name.clone() }], ..Default::default() };
            let fresh = book.free_name(&name);
            assert_ne!(fresh, name);
            assert!(fresh.ends_with(" 2"));
            book.groups.push(SwatchGroup { id: 2, name: fresh });
            book.validate().unwrap();
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod shared_swatches_tests {
    use super::*;
    #[test]
    fn shared_swatches_reorder_keeps_ids_links_and_native_serialization() {
        let p = photocraft_color::gpl::read(b"GIMP Palette\nName: Test\n1 2 3 One\n4 5 6 Two\n7 8 9 Three\n").unwrap().palette;
        let mut b = from_palette(p).unwrap();
        let first = b.swatches[0].id;
        let third = b.swatches[2].id;
        let group = b.groups[0].id;
        move_swatch(&mut b, first, Some(group), Some(third)).unwrap();
        assert_eq!(b.swatches.iter().map(|w| w.name.as_str()).collect::<Vec<_>>(), ["Two", "One", "Three"]);
        move_swatch(&mut b, first, None, None).unwrap();
        assert_eq!(b.swatches[2].id, first);
        assert_eq!(b.swatches[2].group, None);
        let before = b.clone();
        assert!(move_swatch(&mut b, first, Some(u64::MAX), None).is_err());
        assert_eq!(b, before);
        assert_eq!(b, serde_json::from_str(&serde_json::to_string(&b).unwrap()).unwrap());
        b.validate().unwrap();
    }
}
