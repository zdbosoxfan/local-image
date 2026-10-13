//! Krita KPP PNG text "preset" XML and .bundle manifest/resources.
//! Mapping follows KDE Krita kis_paintop_preset.cpp, kis_brush.cpp, and libpaintop options.
use super::brush_import::{self as common, BrushImport, SvgTip};
use super::brush_import_xml as xml;
use photocraft_paint::{BrushSettings, Control, Dynamic, GrayTile, Pattern, TipShape};
use photocraft_raster::Interrupt;
use std::collections::BTreeMap;

pub(crate) fn bundle(bytes: &[u8], group: &str, ctl: &Interrupt<'_>, svg: &SvgTip<'_>) -> Result<BrushImport, String> {
    let files = common::archive(bytes, ctl)?;
    let manifest = files.get("META-INF/manifest.xml").or_else(|| files.get("manifest.xml")).ok_or("Krita bundle has no manifest")?;
    let manifest = xml::parse(std::str::from_utf8(manifest).map_err(|_| "invalid bundle manifest")?)?;
    let names: Vec<_> = manifest
        .iter()
        .filter_map(|n| n.attrs.get("manifest:full-path").or_else(|| n.attrs.get("full-path")))
        .filter(|n| n.ends_with(".kpp"))
        .cloned()
        .collect();
    if names.len() > common::MAX_BRUSHES {
        return Err("bundle preset count exceeds limit".into());
    }
    let mut out = BrushImport::default();
    for (i, n) in names.iter().enumerate() {
        ctl.check().map_err(|e| e.to_string())?;
        let b = files.get(n).or_else(|| files.get(&format!("paintoppresets/{n}")));
        let r = b.ok_or_else(|| format!("missing manifest entry {n}")).and_then(|b| kpp_variants(b, group, &files, svg));
        match r {
            Ok(presets) => {
                for p in presets {
                    out.add(p)?;
                }
            }
            Err(e) => out.skipped.push(format!("{n}: {e}")),
        }
        ctl.progress(0.3 + 0.7 * (i + 1) as f32 / names.len().max(1) as f32);
    }
    if names.is_empty() {
        out.skipped.push("Bundle contains no brush presets".into());
    }
    Ok(out)
}

fn resource<'a>(files: &'a BTreeMap<String, Vec<u8>>, folder: &str, name: &str) -> Result<&'a [u8], String> {
    if name.is_empty() || name.contains(['\\', '\0', ':']) || name.starts_with('/') || name.split('/').any(|s| s == ".." || s == ".") {
        return Err("invalid Krita resource reference".into());
    }
    if let Some(b) = files.get(&format!("{folder}/{name}")).or_else(|| files.get(name)) {
        return Ok(b);
    }
    let basename = name.rsplit('/').next().unwrap_or(name);
    let mut candidates = files.iter().filter(|(n, _)| n.starts_with(&format!("{folder}/")) && n.rsplit('/').next() == Some(basename));
    let first = candidates.next().ok_or_else(|| format!("missing Krita resource {name}; import its bundle"))?;
    if candidates.next().is_some() {
        return Err(format!("ambiguous Krita resource {name}"));
    }
    Ok(first.1)
}
fn auto(def: &xml::Node, nodes: &[xml::Node], b: &mut BrushSettings) -> Result<(), String> {
    let mask = def.children.iter().filter_map(|&i| nodes.get(i)).find(|n| n.name == "MaskGenerator").ok_or("auto brush has no mask generator")?;
    let num = |k, d| if mask.attr(k).is_empty() { Ok(d) } else { common::finite(mask.attr(k), k) };
    b.size = num("diameter", 20.0)?.clamp(0.5, photocraft_paint::MAX_BRUSH_SIZE);
    b.roundness = num("ratio", 1.0)?.clamp(0.01, 1.0);
    let hard = 1.0 - num("hfade", 0.5)?.max(num("vfade", 0.5)?).clamp(0.0, 1.0);
    b.hardness = hard;
    let spikes = num("spikes", 2.0)?.clamp(2.0, 20.0);
    if mask.attr("type") != "circle" || spikes > 2.0 {
        let shape = match mask.attr("type") {
            "rectangle" => "square",
            "circle" => "circle",
            o => return Err(format!("unsupported Krita auto shape {o}")),
        };
        let vbr = format!("GIMP-VBR\n1.5\nAuto tip\n{shape}\n25\n{}\n{}\n{hard}\n{}\n0\n", b.size / 2.0, spikes as u32, 1.0 / b.roundness);
        b.tip = super::brush_import_gimp::vbr(vbr.as_bytes(), "")?.brush.tip;
        b.roundness = 1.0;
    }
    b.import_notes.push(format!("Krita {} auto-mask hardness mapped approximately", mask.attr("id")));
    Ok(())
}
fn sensor(params: &BTreeMap<String, String>, prefix: &str) -> Control {
    let active = params.get(&format!("Pressure{prefix}")).or_else(|| params.get(&format!("{prefix}UseCurve"))).is_some_and(|s| s == "true" || s == "1");
    if !active {
        return Control::Off;
    }
    let s = params.get(&format!("{prefix}Sensor")).map_or("", String::as_str);
    if s.contains("id=\"drawingangle\"") {
        Control::Direction
    } else if s.contains("id=\"tilt") {
        Control::PenTilt
    } else if s.contains("id=\"rotation\"") {
        Control::Rotation
    } else if s.is_empty() || s.contains("id=\"pressure\"") {
        Control::PenPressure
    } else {
        Control::Off
    }
}

pub(crate) fn kpp(bytes: &[u8], group: &str, resources: &BTreeMap<String, Vec<u8>>, svg: &SvgTip<'_>) -> Result<photocraft_paint::BrushPreset, String> {
    let im = common::image(bytes)?;
    let text = im.meta.text.iter().find(|(k, _)| k == "preset").map(|(_, v)| v.as_str()).ok_or("KPP PNG has no preset XML")?;
    let nodes = xml::parse(text)?;
    let root = nodes.get(1).ok_or("missing preset root")?;
    if root.name != "Preset" {
        return Err("KPP XML root is not Preset".into());
    }
    let params: BTreeMap<String, String> =
        root.children.iter().filter_map(|&i| nodes.get(i)).filter(|n| n.name == "param").map(|n| (n.attr("name").into(), n.text.trim().into())).collect();
    let mut embedded = BTreeMap::new();
    // Krita 5.0 standalone presets embed linked resources as base64 in the XML.
    let mut embedded_bytes = 0usize;
    for n in nodes.iter().filter(|n| n.name == "resource") {
        let folder = match n.attr("type") {
            "brushes" => "brushes",
            "patterns" => "patterns",
            _ => continue,
        };
        if n.text.len() > common::MAX_ENTRY / 3 * 4 + 4 {
            return Err("embedded Krita resource too large".into());
        }
        let data = photocraft_paint::tile::b64_decode(n.text.trim()).ok_or("invalid embedded resource base64")?;
        embedded_bytes = embedded_bytes.checked_add(data.len()).filter(|n| *n <= common::MAX_ENTRY).ok_or("embedded resource budget exceeded")?;
        embedded.insert(format!("{folder}/{}", n.attr("filename")), data);
    }
    let engine = if root.attr("paintopid").is_empty() { params.get("paintop").map_or("", String::as_str) } else { root.attr("paintopid") };
    if !matches!(engine, "paintbrush" | "roundmarker" | "colorsmudge" | "smudge" | "hairy" | "bristle") {
        return Err(format!("unsupported Krita engine {engine}"));
    }
    let num = |k: &str, d: f32| params.get(k).map_or(Ok(d), |s| common::finite(s, k));
    let mut b = BrushSettings {
        size: num("diameter", 20.0)?.clamp(0.5, photocraft_paint::MAX_BRUSH_SIZE),
        opacity: num("OpacityValue", 1.0)?.clamp(0.0, 1.0),
        flow: num("FlowValue", 1.0)?.clamp(0.0, 1.0),
        spacing: num("spacing", 0.1)?.clamp(0.01, 10.0),
        pressure_size: false,
        ..Default::default()
    };
    b.erase = params.get("EraserMode").is_some_and(|v| v == "true");
    if engine != "paintbrush" {
        b.import_notes.push(format!("Krita {engine} engine approximated with a pixel brush"));
    }
    if let Some(def) = params.get("brush_definition") {
        let defs = xml::parse(def)?;
        let def = defs.get(1).ok_or("missing Brush definition")?;
        if !def.attr("spacing").is_empty() {
            b.spacing = common::finite(def.attr("spacing"), "spacing")?.clamp(0.01, 10.0);
        }
        if !def.attr("angle").is_empty() {
            b.angle = common::finite(def.attr("angle"), "rotation")?.to_degrees();
        }
        match def.attr("type") {
            "auto_brush" => auto(def, &defs, &mut b)?,
            "gbr_brush" | "gih_brush" | "png_brush" | "svg_brush" => {
                let filename = def.attr("filename");
                let bytes = resource(&embedded, "brushes", filename).or_else(|_| resource(resources, "brushes", filename))?;
                match filename.rsplit('.').next().unwrap_or("").to_ascii_lowercase().as_str() {
                    "gbr" => {
                        let p = super::brush_import_gimp::gbr(bytes, group)?.0;
                        b.tip = p.brush.tip;
                        b.size = p.brush.size;
                        b.import_notes.extend(p.brush.import_notes);
                        b.imported_images.extend(p.brush.imported_images);
                    }
                    "gih" => {
                        let imp = super::brush_import_gimp::gih(bytes, group, &Interrupt::NONE)?;
                        let p = imp.presets.into_iter().next().ok_or("empty Krita image pipe")?;
                        b.tip = p.brush.tip;
                        b.size = p.brush.size;
                        b.import_notes.push("Krita image pipe uses its first cell".into());
                    }
                    "png" => {
                        let g = common::image_mask(bytes, true)?;
                        b.size = g.width.max(g.height).min(photocraft_paint::MAX_BRUSH_SIZE as u32) as f32;
                        b.tip = TipShape::Sampled(g);
                    }
                    "svg" => {
                        let g = svg(bytes)?;
                        b.size = g.width.max(g.height).min(photocraft_paint::MAX_BRUSH_SIZE as u32) as f32;
                        b.tip = TipShape::Sampled(g);
                    }
                    other => return Err(format!("unsupported Krita tip extension {other}")),
                }
                if !def.attr("scale").is_empty() {
                    b.size = (b.size * common::finite(def.attr("scale"), "tip scale")?).clamp(0.5, photocraft_paint::MAX_BRUSH_SIZE);
                }
            }
            other => return Err(format!("unsupported Krita brush tip {other}")),
        }
    }
    b.pressure_size = sensor(&params, "Size") == Control::PenPressure;
    b.pressure_opacity = sensor(&params, "Opacity") == Control::PenPressure;
    let size = sensor(&params, "Size");
    let angle = sensor(&params, "Rotation");
    let flow = sensor(&params, "Flow");
    if size != Control::Off || angle != Control::Off {
        b.shape_dynamics.enabled = true;
        b.shape_dynamics.size = Dynamic::controlled(size);
        b.shape_dynamics.angle = Dynamic::controlled(angle);
    }
    if flow != Control::Off {
        b.transfer.enabled = true;
        b.transfer.flow = Dynamic::controlled(flow);
    }
    if params.get("Texture/Pattern/Enabled").is_some_and(|v| v == "true") {
        let filename = params.get("Texture/Pattern/PatternFileName").or_else(|| params.get("Texture/Pattern/Name")).ok_or("texture has no filename")?;
        let bytes = resource(&embedded, "patterns", filename).or_else(|_| resource(resources, "patterns", filename))?;
        let pattern = if filename.to_ascii_lowercase().ends_with(".pat") {
            // GIMP pattern: big-endian header length/version/width/height/channels/magic, then pixels.
            pattern_pat(bytes)?
        } else {
            common::image_mask(bytes, false)?
        };
        b.texture.enabled = true;
        b.texture.pattern = Pattern::Tile(pattern);
        b.texture.scale = num("Texture/Pattern/Scale", 1.0)?.clamp(0.01, 100.0);
        b.texture.depth = num("Texture/Strength/Strength", 1.0)?.clamp(0.0, 1.0);
    }
    b.import_notes.push("Krita sensors, curves and compositing mapped to the nearest brush dynamics".into());
    Ok(common::preset(root.attr("name"), group, b))
}
fn pattern_pat(bytes: &[u8]) -> Result<GrayTile, String> {
    let be = |i| bytes.get(i..i + 4).and_then(|s| s.try_into().ok()).map(u32::from_be_bytes).ok_or("truncated GIMP pattern");
    let header = be(0)? as usize;
    let (w, h, c) = (be(8)?, be(12)?, be(16)?);
    let n = common::dimensions(w, h)?;
    if be(4)? != 1 || be(20)? != 0x47504154 || !(24..=4096).contains(&header) || !(1..=4).contains(&c) {
        return Err("unsupported pattern header".into());
    }
    let body = bytes.get(header..header + n * c as usize).ok_or("truncated pattern pixels")?;
    let data = body
        .chunks_exact(c as usize)
        .map(|p| {
            let l = if c < 3 { u32::from(p[0]) * 257 } else { (u32::from(p[0]) * 2126 + u32::from(p[1]) * 7152 + u32::from(p[2]) * 722) * 257 / 10000 };
            let a = if c == 2 || c == 4 { u32::from(p[c as usize - 1]) } else { 255 };
            (l * a / 255) as u16
        })
        .collect();
    Ok(GrayTile { width: w, height: h, data })
}

/// A Krita image pipe becomes selectable tip variants, retaining every cell's artwork.
pub(crate) fn kpp_variants(
    bytes: &[u8],
    group: &str,
    resources: &BTreeMap<String, Vec<u8>>,
    svg: &SvgTip<'_>,
) -> Result<Vec<photocraft_paint::BrushPreset>, String> {
    let base = kpp(bytes, group, resources, svg)?;
    let im = common::image(bytes)?;
    let text = im.meta.text.iter().find(|(k, _)| k == "preset").map(|(_, v)| v.as_str()).ok_or("missing preset XML")?;
    let nodes = xml::parse(text)?;
    let Some(def) = nodes.iter().find(|n| n.name == "param" && n.attr("name") == "brush_definition") else {
        return Ok(vec![base]);
    };
    let defs = xml::parse(&def.text)?;
    let brush = defs.get(1).ok_or("missing brush definition")?;
    if brush.attr("type") != "gih_brush" {
        return Ok(vec![base]);
    }
    let name = brush.attr("filename");
    let embedded = nodes
        .iter()
        .find(|n| n.name == "resource" && n.attr("filename") == name)
        .map(|n| photocraft_paint::tile::b64_decode(n.text.trim()).ok_or("bad embedded image pipe"))
        .transpose()?;
    let source = if let Some(b) = embedded.as_deref() { b } else { resource(resources, "brushes", name)? };
    let pipe = super::brush_import_gimp::gih(source, group, &Interrupt::NONE)?;
    let count = pipe.presets.len();
    let mut out = BrushImport::default();
    for (i, p) in pipe.presets.into_iter().enumerate() {
        let mut variant = base.clone();
        variant.name = format!("{} · {}", base.name, i + 1);
        variant.brush.tip = p.brush.tip;
        variant.brush.import_notes.retain(|s| s != "Krita image pipe uses its first cell");
        variant.brush.import_notes.push(format!("Krita image pipe cell {}/{} imported as a selectable tip variant", i + 1, count));
        out.add(variant)?;
    }
    Ok(out.presets)
}
