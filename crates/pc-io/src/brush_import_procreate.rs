//! Procreate brush archives: NSKeyedArchiver plist plus lossless Shape.png / Grain.png.
//! Setting names verified against the licensed Jay Versluis brushes and NekoPhoto fixtures.
use super::brush_import::{self as common, BrushImport};
use super::brush_import_plist::{Plist, Scalar};
use photocraft_paint::{BrushSettings, Control, Dynamic, Pattern, TipShape};
use photocraft_raster::Interrupt;

pub(crate) fn read(bytes: &[u8], group: &str, ctl: &Interrupt<'_>) -> Result<BrushImport, String> {
    let files = common::archive(bytes, ctl)?;
    let archives: Vec<_> = files.iter().filter(|(n, _)| n.as_str() == "Brush.archive" || n.ends_with("/Brush.archive")).collect();
    if archives.is_empty() {
        return Err("no Brush.archive in Procreate pack".into());
    }
    if archives.len() > common::MAX_BRUSHES {
        return Err("Procreate brush count exceeds limit".into());
    }
    let mut out = BrushImport::default();
    for (i, (name, data)) in archives.iter().enumerate() {
        ctl.check().map_err(|e| e.to_string())?;
        let prefix = name.strip_suffix("Brush.archive").ok_or("bad brush archive name")?;
        let result = (|| {
            let settings = Plist::parse(data)?.brush()?;
            let n = |key: &str, default: f32| match settings.get(key) {
                Some(Scalar::Number(v)) => *v as f32,
                _ => default,
            };
            let flag = |key: &str| matches!(settings.get(key), Some(Scalar::Bool(true))) || n(key, 0.0) > 0.5;
            let title = match settings.get("name") {
                Some(Scalar::String(s)) => s.as_str(),
                _ => prefix.trim_end_matches('/'),
            };
            let shape = files.get(&format!("{prefix}Shape.png")).ok_or("Procreate brush has no Shape.png")?;
            let tip = common::image_mask(shape, flag("shapeInverted"))?;
            let mut b = BrushSettings {
                size: (n("maxSize", 0.1) * 1000.0).clamp(0.5, photocraft_paint::MAX_BRUSH_SIZE),
                opacity: n("maxOpacity", 1.0).clamp(0.0, 1.0),
                spacing: n("plotSpacing", 0.1).clamp(0.01, 10.0),
                tip: TipShape::Sampled(tip),
                pressure_size: n("dynamicsPressureSize", 0.0) != 0.0,
                pressure_opacity: n("dynamicsPressureOpacity", 0.0) != 0.0,
                roundness: n("shapeRoundness", 1.0).clamp(0.01, 1.0),
                ..Default::default()
            };
            b.imported_images.push(photocraft_paint::ImportedBrushImage { name: "Shape.png".into(), png: photocraft_paint::tile::b64_encode(shape) });
            let jitter = n("dynamicsJitterSize", 0.0).clamp(0.0, 1.0);
            let angle = n("shapeScatter", 0.0).clamp(0.0, 1.0);
            if jitter > 0.0 || angle > 0.0 || flag("shapeRandomise") {
                b.shape_dynamics.enabled = true;
                b.shape_dynamics.size.jitter = jitter;
                b.shape_dynamics.angle.jitter = if flag("shapeRandomise") { 1.0 } else { angle };
            }
            if n("shapeRotation", 0.0) != 0.0 || flag("oriented") {
                b.shape_dynamics.enabled = true;
                b.shape_dynamics.angle = Dynamic { jitter: b.shape_dynamics.angle.jitter, ..Dynamic::controlled(Control::Direction) };
            }
            let jitter = n("plotJitter", 0.0).clamp(0.0, 1.0);
            if jitter > 0.0 {
                b.scattering.enabled = true;
                b.scattering.scatter.jitter = jitter;
                b.scattering.both_axes = true;
            }
            let jitter = n("dynamicsJitterOpacity", 0.0).clamp(0.0, 1.0);
            if jitter > 0.0 {
                b.transfer.enabled = true;
                b.transfer.opacity.jitter = jitter;
            }
            if let Some(grain) = files.get(&format!("{prefix}Grain.png")) {
                b.texture.enabled = true;
                b.texture.pattern = Pattern::Tile(common::image_mask(grain, flag("textureInverted"))?);
                b.texture.scale = n("textureScale", 1.0).clamp(0.01, 100.0);
                b.texture.each_tip = n("textureMovement", 1.0) > 0.0;
                b.imported_images.push(photocraft_paint::ImportedBrushImage { name: "Grain.png".into(), png: photocraft_paint::tile::b64_encode(grain) });
            }
            b.import_notes.push("Procreate size, spacing, jitter and pressure dynamics mapped approximately; shape and grain PNGs retained losslessly".into());
            Ok::<_, String>(common::preset(title, group, b))
        })();
        match result {
            Ok(p) => out.add(p)?,
            Err(e) => out.skipped.push(format!("{name}: {e}")),
        }
        ctl.progress(0.3 + 0.7 * (i + 1) as f32 / archives.len() as f32);
    }
    Ok(out)
}
