//! Importing Photoshop preset files.
//!
//! - `brush.presets.importAbr` adds the presets of an `.abr` file (v1, v2, v6+) to the brush
//!   library as one group (Brushes panel › Import Brushes…, Preset Manager). Parsing lives in
//!   `photocraft-psd` (`abr`), the mapping onto [`BrushSettings`] in `photocraft-io` (`abr_map`).
//! - `gradient.presets.importGrd` adds the gradients of a `.grd` file (version 5) to the
//!   Gradients panel as one group.
//!
//! [`BrushSettings`]: photocraft_paint::BrushSettings

use serde_json::{Value, json};

use crate::commands::CommandSpec;
use crate::{EngineError, Result, Session};

fn bad(cmd: &str, msg: impl Into<String>) -> EngineError {
    EngineError::BadParams { cmd: cmd.into(), msg: msg.into() }
}

fn always(_: &Session) -> std::result::Result<(), String> {
    Ok(())
}

/// Largest accepted file (Photoshop's biggest bundled sets are well under this).
pub const MAX_FILE_BYTES: u64 = 512 << 20;

/// File bytes from `data` (base64) or `path`, plus a default group name.
pub(crate) fn file_bytes(p: &Value, cmd: &str, what: &str) -> Result<(Vec<u8>, String)> {
    if let Some(d) = p.get("data") {
        let s = d.as_str().ok_or_else(|| bad(cmd, "`data` must be a base64 string"))?;
        if s.len() as u64 > MAX_FILE_BYTES / 3 * 4 + 4 {
            return Err(bad(cmd, "`data` is too large"));
        }
        let bytes = photocraft_paint::tile::b64_decode(s).ok_or_else(|| bad(cmd, "`data` is not valid base64"))?;
        return Ok((bytes, String::new()));
    }
    let path =
        p.get("path").and_then(Value::as_str).filter(|s| !s.trim().is_empty()).ok_or_else(|| bad(cmd, format!("give `path` ({what}) or `data` (base64)")))?;
    let len = std::fs::metadata(path).map_err(|e| EngineError::Other(format!("{path}: {e}")))?.len();
    if len > MAX_FILE_BYTES {
        return Err(bad(cmd, format!("{path}: file too large ({len} bytes)")));
    }
    let bytes = photocraft_format::read_file(std::path::Path::new(path)).map_err(|e| EngineError::Other(format!("{path}: {e}")))?;
    let stem = std::path::Path::new(path).file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
    Ok((bytes, stem))
}

/// A name not used by any preset in `taken` (case-insensitive): `name`, `name 2`, `name 3`…
#[derive(Default)]
struct Names {
    taken: std::collections::HashSet<String>,
    next: std::collections::HashMap<String, usize>,
}
impl Names {
    fn add(&mut self, name: &str) -> String {
        let key = name.to_ascii_lowercase();
        if self.taken.insert(key.clone()) {
            return name.to_string();
        }
        let next = self.next.entry(key).or_insert(2);
        loop {
            let candidate = format!("{name} {next}");
            *next += 1;
            if self.taken.insert(candidate.to_ascii_lowercase()) {
                return candidate;
            }
        }
    }
}
#[cfg(test)]
fn unique(name: &str, taken: &[String]) -> String {
    let mut names = Names::default();
    names.taken.extend(taken.iter().map(|n| n.to_ascii_lowercase()));
    names.add(name)
}

fn import_abr(s: &mut Session, p: &Value) -> Result<Value> {
    let extension =
        p.get("format").and_then(Value::as_str).map(str::to_ascii_lowercase).or_else(|| {
            p.get("path").and_then(Value::as_str).and_then(|s| std::path::Path::new(s).extension()).map(|s| s.to_string_lossy().to_ascii_lowercase())
        });
    if extension.as_deref().is_some_and(|e| e != "abr") {
        return import_brushes(s, p);
    }

    let cmd = "brush.presets.importAbr";
    let label = "Import Brushes";
    // A background job when started with `Session::start` (#210): reading and decoding the
    // file run on a worker, cancellable before each brush; the library changes on apply.
    let params = p.clone();
    let params_apply = p.clone();
    crate::jobs::run(
        s,
        label,
        false,
        move |ctx| {
            ctx.report(0.0, "Importing brushes");
            let p = &params;
            let (bytes, stem) = file_bytes(p, cmd, ".abr file")?;
            let group = match p.get("group") {
                Some(v) => v.as_str().map(str::trim).filter(|g| !g.is_empty()).ok_or_else(|| bad(cmd, "`group` must be a non-empty string"))?.to_string(),
                None if !stem.is_empty() => stem,
                None => "Imported Brushes".to_string(),
            };
            ctx.check()?;
            let imp = ctx
                .stage(0.05, 1.0, "Importing brushes", |ctl| photocraft_io::abr_map::read_abr_with(&bytes, &group, ctl))
                .map_err(|e| if ctx.is_cancelled() { EngineError::Cancelled } else { bad(cmd, e) })?;
            Ok((group, imp))
        },
        move |s, (group, imp)| add_abr_presets(s, &params_apply, group, imp),
    )
}

fn add_abr_presets(s: &mut Session, p: &Value, group: String, imp: photocraft_io::abr_map::AbrImport) -> Result<Value> {
    // Re-importing a file replaces its group instead of duplicating it.
    if p.get("replace").and_then(Value::as_bool).unwrap_or(true) {
        s.tools.presets.retain(|x| x.builtin || x.group != group);
    }
    let mut taken = Names::default();
    taken.taken.extend(s.tools.presets.iter().map(|x| x.name.to_ascii_lowercase()));
    let mut names = Vec::with_capacity(imp.presets.len());
    for mut preset in imp.presets {
        preset.name = taken.add(&preset.name);
        names.push(preset.name.clone());
        s.tools.presets.push(preset);
    }
    s.brush_presets_changed();
    if p.get("select").and_then(Value::as_bool).unwrap_or(false)
        && let Some(first) = names.first()
        && let Some(pr) = photocraft_paint::presets::find(&s.tools.presets, first)
    {
        s.tools.brush = pr.brush.clone().picked_over(&s.tools.brush);
    }
    Ok(json!({ "group": group, "imported": names, "count": names.len(), "version": imp.version, "warnings": imp.warnings }))
}

/// All supported packs, sharing the existing cancellable job/apply and preset store path.
fn import_brushes(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "brush.presets.importAbr";
    if p.get("group").is_some_and(|group| group.as_str().is_none_or(|s| s.trim().is_empty())) {
        return Err(bad(cmd, "`group` must be a non-empty string"));
    }
    let params = p.clone();
    let apply = p.clone();
    crate::jobs::run(
        s,
        "Import Brushes",
        false,
        move |ctx| {
            let p = &params;
            let extension = p
                .get("format")
                .and_then(Value::as_str)
                .map(str::to_ascii_lowercase)
                .or_else(|| {
                    p.get("path").and_then(Value::as_str).and_then(|s| std::path::Path::new(s).extension()).map(|s| s.to_string_lossy().to_ascii_lowercase())
                })
                .ok_or_else(|| bad(cmd, "give a file path or `format` with base64 data"))?;
            // Preflight the byte budget before reading/decoding the supplied source.
            if let Some(path) = p.get("path").and_then(Value::as_str)
                && std::fs::metadata(path).map_err(|e| bad(cmd, e.to_string()))?.len() > photocraft_io::brush_import::MAX_FILE as u64
            {
                return Err(bad(cmd, "brush file exceeds 128 MiB"));
            }
            if p.get("data").and_then(Value::as_str).is_some_and(|s| s.len() > photocraft_io::brush_import::MAX_FILE / 3 * 4 + 4) {
                return Err(bad(cmd, "brush data exceeds 128 MiB"));
            }
            let (bytes, stem) = file_bytes(p, cmd, "brush pack")?;
            let group = p
                .get("group")
                .and_then(Value::as_str)
                .filter(|s| !s.trim().is_empty())
                .unwrap_or(if stem.is_empty() { "Imported Brushes" } else { &stem })
                .to_string();
            let imp = ctx
                .stage(0.05, 1.0, "Importing brushes", |ctl| photocraft_io::brush_import::read_with_svg(&bytes, &extension, &group, ctl, &svg_brush_tip))
                .map_err(|e| if ctx.is_cancelled() { EngineError::Cancelled } else { bad(cmd, e) })?;
            Ok((group, imp))
        },
        move |s, (group, imp)| {
            let approximate = imp.approximate();
            let skipped = imp.skipped;
            // A wholly unreadable pack must not replace a previously imported group.
            if imp.presets.is_empty() {
                return Ok(json!({"group":group,"count":0,"approximate":0,"skipped":skipped,"warnings":imp.warnings}));
            }
            let mut result = add_abr_presets(s, &apply, group, photocraft_io::abr_map::AbrImport { presets: imp.presets, warnings: imp.warnings, version: 0 })?;
            result["approximate"] = json!(approximate);
            result["skipped"] = json!(skipped);
            Ok(result)
        },
    )
}

/// Reuse the application's bounded, offline SVG importer; the IO crate needs no new dependency.
fn svg_brush_tip(bytes: &[u8]) -> std::result::Result<photocraft_paint::GrayTile, String> {
    use photocraft_doc::vector_design::exchange::{ImportLimits, ImportOptions};
    let options = ImportOptions {
        limits: ImportLimits { bytes: 2 << 20, pixels: 16 << 20, objects: 4096, depth: 32, segments: 100_000, decompressed_bytes: 32 << 20 },
        ..Default::default()
    };
    let scene = photocraft_svg::import(bytes, &options).map_err(|e| e.to_string())?;
    if scene.size.width > 8192 || scene.size.height > 8192 {
        return Err("SVG tip dimensions exceed limits".into());
    }
    let (doc, _) = crate::vector_packages::svg_import::document(scene, "Brush tip");
    let image = photocraft_compose::render(&doc, doc.bounds());
    let mut data = Vec::with_capacity(image.px.len());
    for p in &image.px {
        let alpha = p[3];
        let light = li_color::luminance_709([p[0], p[1], p[2]]);
        data.push(((alpha - light).clamp(0.0, 1.0) * 65535.0).round() as u16);
    }
    Ok(photocraft_paint::GrayTile { width: doc.size.width, height: doc.size.height, data })
}

/// Brush-file import command specs.
pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec {
            id: "brush.presets.importAbr",
            label: "Import Brushes…",
            menu: &[],
            shortcut: None,
            params: r##"{"path":"brush file (.abr, .gbr, .gih, .vbr, .bundle, .kpp, .brush, .brushset, .myb)"?,"data":base64 bytes?,"format":string? (extension for base64 data),"group":string?=file name,"replace":bool=true (replace a group of the same name),"select":bool=false (make the first imported preset current)}"##,
            enabled: always,
            run: import_abr,
            journal: true,
        },
        CommandSpec {
            id: "gradient.presets.importGrd",
            label: "Import Gradients…",
            menu: &[],
            shortcut: None,
            params: r##"{"path":".grd file"?,"data":base64 bytes?,"group":string?=file name,"replace":bool=true (replace a group of the same name)}"##,
            enabled: always,
            run: import_grd,
            journal: true,
        },
    ]
}

// Gradients (.grd)

fn hsb_to_rgb([h, s, v]: [f32; 3]) -> [f32; 3] {
    li_color::perceptual::hsv_to_rgb(h, s, v)
}

/// CIE Lab (D50, as Photoshop stores it) to sRGB through XYZ with Bradford adaptation to D65.
fn lab_to_rgb([l, a, b]: [f32; 3]) -> [f32; 3] {
    li_color::convert::lab_to_srgb_with_white([l, a, b], li_color::cms_math::D50)
}

fn stop_color(c: photocraft_psd::grd::GrdColor) -> crate::presets::gradients::StopColor {
    use crate::presets::gradients::StopColor;
    use photocraft_psd::grd::GrdColor;
    let clamp = |c: [f32; 3]| StopColor::Rgb(c.map(|v| if v.is_finite() { v.clamp(0.0, 1.0) } else { 0.0 }));
    match c {
        GrdColor::Foreground => StopColor::Foreground,
        GrdColor::Background => StopColor::Background,
        GrdColor::Rgb(c) => clamp(c),
        GrdColor::Hsb(c) => clamp(hsb_to_rgb(c)),
        // Naive (profile-free) ink → RGB, like an uncalibrated preview.
        GrdColor::Cmyk([c, m, y, k]) => clamp([(1.0 - c) * (1.0 - k), (1.0 - m) * (1.0 - k), (1.0 - y) * (1.0 - k)]),
        GrdColor::Lab(c) => clamp(lab_to_rgb(c)),
        GrdColor::Gray(g) => clamp([1.0 - g; 3]),
    }
}

fn import_grd(s: &mut Session, p: &Value) -> Result<Value> {
    use crate::presets::{Group, gradients::GradientPreset};
    let cmd = "gradient.presets.importGrd";
    let (bytes, stem) = file_bytes(p, cmd, ".grd file")?;
    let group = match p.get("group") {
        Some(v) => v.as_str().map(str::trim).filter(|g| !g.is_empty()).ok_or_else(|| bad(cmd, "`group` must be a non-empty string"))?.to_string(),
        None if !stem.is_empty() => stem,
        None => "Imported Gradients".to_string(),
    };
    let grads = photocraft_psd::grd::parse(&bytes).map_err(|e| bad(cmd, format!("not a readable Photoshop gradient file: {e}")))?;
    let mut warnings = Vec::new();
    let mut items = Vec::new();
    let (mut noise, mut midpoints) = (0, false);
    for (i, g) in grads.iter().enumerate() {
        if g.noise || g.stops.len() < 2 {
            noise += 1;
            continue;
        }
        midpoints |= g.stops.iter().any(|st| (st.midpoint - 0.5).abs() > 0.01) || g.opacity.iter().any(|o| (o.2 - 0.5).abs() > 0.01);
        let mut stops: Vec<(f32, _)> = g.stops.iter().map(|st| (st.location, stop_color(st.color))).collect();
        stops.sort_by(|a, b| a.0.total_cmp(&b.0));
        let mut opacity: Vec<(f32, f32)> = g.opacity.iter().map(|o| (o.0, o.1)).collect();
        opacity.sort_by(|a, b| a.0.total_cmp(&b.0));
        if opacity.iter().all(|o| o.1 >= 1.0) {
            opacity.clear();
        }
        let name = if g.name.trim().is_empty() { format!("Gradient {}", i + 1) } else { g.name.trim().to_string() };
        items.push(GradientPreset { name, stops, opacity });
    }
    if noise > 0 {
        warnings.push(format!("{noise} noise gradients are not supported and were skipped"));
    }
    if midpoints {
        warnings.push("colour and opacity midpoints other than 50 % are drawn at 50 %".to_string());
    }
    if items.is_empty() {
        return Err(bad(cmd, "the file holds no gradients PhotoCraft can use (noise gradients only)"));
    }
    let names: Vec<String> = items.iter().map(|g| g.name.clone()).collect();
    let st = &mut s.presets;
    if p.get("replace").and_then(Value::as_bool).unwrap_or(true) {
        st.gradients.retain(|g| g.name != group);
    }
    st.gradients.push(Group::new(&group, items));
    s.presets_changed();
    Ok(json!({"group": group, "imported": names, "count": names.len(), "warnings": warnings}))
}

#[cfg(test)]
mod tests {
    use super::*;
    use photocraft_psd::abr::{AbrSample, LegacyBrush, LegacyTip, write_v6, write_v12};
    use photocraft_psd::descriptor::{Descriptor, UnicodeString, Value as DV};

    fn abr_v2() -> Vec<u8> {
        let tip = AbrSample { id: String::new(), width: 6, height: 4, depth: 8, data: vec![255; 24] };
        write_v12(
            2,
            &[
                LegacyBrush { name: "Hard Round".into(), spacing: 25, anti_alias: true, tip: LegacyTip::Sampled(tip) },
                LegacyBrush {
                    name: String::new(),
                    spacing: 10,
                    anti_alias: true,
                    tip: LegacyTip::Computed { diameter: 9, hardness: 100, angle: 0, roundness: 100 },
                },
            ],
            true,
        )
        .unwrap()
    }

    #[test]
    fn duplicate_name_suffixes_advance_without_rescanning_the_group() {
        let mut names = Names::default();
        assert_eq!(names.add("Tip"), "Tip");
        for n in 2..=20_000 {
            assert_eq!(names.add("Tip"), format!("Tip {n}"));
        }
        assert_eq!(names.next.get("tip"), Some(&20_001));
    }
    #[test]
    fn import_adds_a_group_and_paints() {
        let mut s = Session::new();
        let before = s.tools.presets.len();
        let data = photocraft_paint::tile::b64_encode(&abr_v2());
        let r = s.execute("brush.presets.importAbr", json!({"data": data, "group": "Legacy", "select": true})).unwrap();
        assert_eq!(r["count"], 2);
        // The name clashing with the built-in Hard Round is made unique.
        assert_eq!(r["imported"], json!(["Hard Round 2", "9 px Round"]));
        assert_eq!(s.tools.presets.len(), before + 2);
        assert!(s.tools.presets.iter().filter(|p| p.group == "Legacy").all(|p| !p.builtin));
        assert!(matches!(s.tools.brush.tip, photocraft_paint::TipShape::Sampled(_)));
        // Re-import replaces the group.
        s.execute("brush.presets.importAbr", json!({"data": data, "group": "Legacy"})).unwrap();
        assert_eq!(s.tools.presets.len(), before + 2);
        // Imported presets paint.
        s.execute("file.new", json!({"width": 64, "height": 64})).unwrap();
        s.execute("paint.stroke", json!({"points": [[5, 30], [60, 30]], "preset": "9 px Round"})).unwrap();
        // From a file on disk, the group defaults to the file name.
        let dir = std::env::temp_dir().join(format!("pc-abr-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("My Set.abr");
        let preset = Descriptor::new("brushPreset")
            .with("Nm  ", DV::Text(UnicodeString::new_nul("Speckle")))
            .with("Brsh", DV::Descriptor(Descriptor::new("sampledBrush").with("sampledData", DV::Text(UnicodeString::new_nul("$s")))));
        let tip = AbrSample { id: "$s".into(), width: 5, height: 5, depth: 16, data: vec![0x80; 50] };
        std::fs::write(&path, write_v6(2, &[tip], &[], &[preset], true).unwrap()).unwrap();
        let r = s.execute("brush.presets.importAbr", json!({"path": path.to_string_lossy()})).unwrap();
        assert_eq!((r["group"].as_str(), r["imported"][0].as_str(), r["version"].as_u64()), (Some("My Set"), Some("Speckle"), Some(6)));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn bad_params_fail_gracefully() {
        let mut s = Session::new();
        let n = s.tools.presets.len();
        for p in [
            json!({}),
            json!({"path": ""}),
            json!({"path": "/nonexistent/brushes.abr"}),
            json!({"data": 42}),
            json!({"data": "%%%not base64"}),
            json!({"data": ""}),
            json!({"data": "AAAA"}),
            json!({"data": photocraft_paint::tile::b64_encode(b"8BPS\0\x01junk")}),
            json!({"data": photocraft_paint::tile::b64_encode(&abr_v2()), "group": 5}),
            json!({"data": photocraft_paint::tile::b64_encode(&abr_v2()), "group": "  "}),
            json!({"data": photocraft_paint::tile::b64_encode(&abr_v2()[..20])}),
        ] {
            assert!(s.execute("brush.presets.importAbr", p.clone()).is_err(), "{p}");
        }
        assert_eq!(s.tools.presets.len(), n);
    }

    #[test]
    fn grd_import_adds_a_gradient_group() {
        use crate::presets::gradients::StopColor;
        use photocraft_psd::grd::{GrdColor, GrdGradient, GrdStop, write};
        let grads = vec![
            GrdGradient {
                name: "Ember".into(),
                noise: false,
                stops: vec![
                    GrdStop { location: 1.0, midpoint: 0.5, color: GrdColor::Rgb([1.0, 0.0, 0.0]) },
                    GrdStop { location: 0.0, midpoint: 0.5, color: GrdColor::Foreground },
                ],
                opacity: vec![(0.0, 1.0, 0.5), (1.0, 0.0, 0.5)],
            },
            GrdGradient { name: "Static".into(), noise: true, ..Default::default() },
        ];
        let mut s = Session::new();
        let data = photocraft_paint::tile::b64_encode(&write(&grads));
        let r = s.execute("gradient.presets.importGrd", json!({"data": data, "group": "Fire"})).unwrap();
        assert_eq!(r["imported"], json!(["Ember"]));
        assert_eq!(r["warnings"].as_array().map(Vec::len), Some(1));
        let g = s.presets.gradients.iter().find(|g| g.name == "Fire").unwrap();
        assert_eq!(g.items[0].stops, vec![(0.0, StopColor::Foreground), (1.0, StopColor::Rgb([1.0, 0.0, 0.0]))]);
        assert_eq!(g.items[0].opacity, vec![(0.0, 1.0), (1.0, 0.0)]);
        // Re-import replaces the group.
        let n = s.presets.gradients.len();
        s.execute("gradient.presets.importGrd", json!({"data": data, "group": "Fire"})).unwrap();
        assert_eq!(s.presets.gradients.len(), n);
        for p in [json!({}), json!({"data": "@@"}), json!({"data": photocraft_paint::tile::b64_encode(b"8BGR\0\x03")}), json!({"path": "/nope.grd"})] {
            assert!(s.execute("gradient.presets.importGrd", p.clone()).is_err(), "{p}");
        }
        let only_noise = photocraft_paint::tile::b64_encode(&write(&grads[1..]));
        assert!(s.execute("gradient.presets.importGrd", json!({"data": only_noise})).is_err());
    }

    #[test]
    fn colour_conversions() {
        assert_eq!(hsb_to_rgb([120.0, 1.0, 1.0]), [0.0, 1.0, 0.0]);
        let w = lab_to_rgb([100.0, 0.0, 0.0]);
        assert!(w.iter().all(|v| (v - 1.0).abs() < 0.01), "{w:?}");
        let k = lab_to_rgb([0.0, 0.0, 0.0]);
        assert!(k.iter().all(|v| *v < 0.01), "{k:?}");
    }

    #[test]
    fn unique_names() {
        let taken = vec!["A".to_string(), "a 2".to_string()];
        assert_eq!(unique("a", &taken), "a 3");
        assert_eq!(unique("B", &taken), "B");
    }
}

#[cfg(test)]
mod pack_tests {
    use super::*;
    const BRUSH: &[u8] = include_bytes!("../../pc-io/tests/fixtures/brushes/plain-stitch.brush");
    #[test]
    fn brush_batch_pack_import_summary_groups_and_store_roundtrip() {
        let mut s = Session::new();
        let backend = crate::preset_store::MemBackend::default();
        s.attach_preset_store(crate::preset_store::open(Box::new(backend.clone())));
        let p = json!({"data":photocraft_paint::tile::b64_encode(BRUSH),"format":"brush","group":"Artist Pack","select":true});
        let out = s.execute("brush.presets.importAbr", p.clone()).unwrap();
        assert_eq!(out["count"], 1);
        assert_eq!(out["approximate"], 1);
        assert_eq!(out["skipped"], json!([]));
        let imported = s.tools.presets.iter().find(|p| p.group == "Artist Pack").unwrap().clone();
        assert!(!imported.brush.import_notes.is_empty());
        assert_eq!(s.tools.brush.tip, imported.brush.tip);
        s.sync_preset_store();
        let mut reopened = Session::new();
        reopened.attach_preset_store(crate::preset_store::open(Box::new(backend)));
        assert_eq!(reopened.tools.presets.iter().find(|p| p.group == "Artist Pack"), Some(&imported));
        s.execute("brush.presets.importAbr", p).unwrap();
        assert_eq!(s.tools.presets.iter().filter(|p| p.group == "Artist Pack").count(), 1);
        let before = s.tools.presets.clone();
        assert!(s.execute("brush.presets.importAbr", json!({"data":"AA==","format":"afbrushes","group":"Artist Pack"})).is_err());
        assert_eq!(s.tools.presets, before);
    }
    #[test]
    fn brush_batch_tablet_preference_defaults_and_validation() {
        let tools: crate::prefs::Tools = serde_json::from_value(json!({"useTabletPressure":true})).unwrap();
        assert_eq!(tools.tablet_compatibility_mode, "auto");
        let mut s = Session::new();
        for mode in ["auto", "on", "off"] {
            s.execute("prefs.set", json!({"path":"tools.tabletCompatibilityMode","value":mode})).unwrap();
        }
        assert!(s.execute("prefs.set", json!({"path":"tools.tabletCompatibilityMode","value":"bad"})).is_err());
    }
    #[test]
    fn brush_batch_pressure_curve_checks_missing_upstream_limits() {
        let mut s = Session::new();
        for value in [json!([]), json!([[0, 0]]), json!([[0, 0], [1, 1.1]]), json!([[0, 0], [1, null]]), json!(vec![[0.0, 0.0]; 17])] {
            assert!(s.execute("prefs.set", json!({"path":"tools.penPressureCurve","value":value})).is_err());
        }
        s.execute("prefs.set", json!({"path":"tools.penPressureCurve","value":[[0,0],[0.3,0.6],[1,1]]})).unwrap();
    }

    #[test]
    fn brush_batch_svg_tip_uses_bounded_app_renderer() {
        let tip =
            svg_brush_tip(br##"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16"><rect x="2" y="2" width="12" height="12" fill="#000"/></svg>"##)
                .unwrap();
        assert_eq!((tip.width, tip.height), (16, 16));
        assert!(tip.get(8, 8) > 0.99);
        assert!(tip.get(0, 0) < 0.01);
        assert!(svg_brush_tip(br#"<svg xmlns="http://www.w3.org/2000/svg" width="9000" height="9000"/>"#).is_err());
    }
}

#[test]
fn dsfix_non_abr_import_rejects_invalid_group_before_import() {
    let data = photocraft_paint::tile::b64_encode(include_bytes!("../../pc-io/tests/fixtures/brushes/krita-basic.kpp"));
    let mut s = Session::new();
    let before = s.tools.presets.clone();
    for group in [json!(3), json!(null), json!("   "), json!([])] {
        let err = s.execute("brush.presets.importAbr", json!({"data":data,"format":"kpp","group":group})).unwrap_err();
        assert!(err.to_string().contains("`group` must be a non-empty string"), "{err}");
        assert_eq!(s.tools.presets, before);
    }
}
