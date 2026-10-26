use super::*;

/// Photoshop filter → PhotoCraft command, keyed by the `Fltr` class (and `filterID`).
pub(super) struct Known {
    pub(super) class: &'static str,
    pub(super) filter_id: i32,
    /// `Nm  ` as Photoshop writes it (the menu item, with its ellipsis and mnemonic).
    pub(super) name: &'static str,
    pub(super) command: &'static str,
}

/// A four-character PSD code as the big-endian integer Photoshop stores it as.
pub(super) const fn code(c: &[u8; 4]) -> i32 {
    i32::from_be_bytes(*c)
}

pub(super) const KNOWN: [Known; 46] = [
    Known { class: "GsnB", filter_id: code(b"GsnB"), name: "Gaussian Blur...", command: "filter.blur.gaussianBlur" },
    Known { class: "boxblur", filter_id: 840, name: "Box Blur...", command: "filter.blur.boxBlur" },
    Known { class: "MtnB", filter_id: code(b"MtnB"), name: "Motion Blur...", command: "filter.blur.motionBlur" },
    // Surface Blur, Smart Blur and Radial Blur: keys, value types and enums measured in Photoshop.
    Known { class: "surfaceBlur", filter_id: 1135, name: "Surface Blur...", command: "filter.blur.surfaceBlur" },
    Known { class: "SmrB", filter_id: code(b"SmrB"), name: "Smart Blur...", command: "filter.blur.smartBlur" },
    // Radial Blur at its defaults stores no `Fltr` (`hasoptions` false); that entry is kept verbatim.
    Known { class: "RdlB", filter_id: code(b"RdlB"), name: "Radial Blur...", command: "filter.blur.radialBlur" },
    Known { class: "UnsM", filter_id: code(b"UnsM"), name: "Unsharp Mask...", command: "filter.sharpen.unsharpMask" },
    Known { class: "AdNs", filter_id: code(b"AdNs"), name: "Add Noise...", command: "filter.noise.addNoise" },
    Known { class: "Mdn ", filter_id: code(b"Mdn "), name: "Median...", command: "filter.noise.median" },
    Known { class: "HghP", filter_id: code(b"HghP"), name: "High Pass...", command: "filter.other.highPass" },
    Known { class: "Mxm ", filter_id: code(b"Mxm "), name: "Maximum...", command: "filter.other.maximum" },
    Known { class: "Mnm ", filter_id: code(b"Mnm "), name: "Minimum...", command: "filter.other.minimum" },
    Known { class: "Ofst", filter_id: code(b"Ofst"), name: "Offset...", command: "filter.other.offset" },
    Known { class: "Cstm", filter_id: code(b"Cstm"), name: "Custom...", command: "filter.other.custom" },
    Known { class: "Msc ", filter_id: code(b"Msc "), name: "Mosaic...", command: "filter.pixelate.mosaic" },
    Known { class: "Mztn", filter_id: code(b"Mztn"), name: "Mezzotint...", command: "filter.pixelate.mezzotint" },
    Known { class: "ClrH", filter_id: code(b"ClrH"), name: "Color Halftone...", command: "filter.pixelate.colorHalftone" },
    Known { class: "Embs", filter_id: code(b"Embs"), name: "Emboss...", command: "filter.stylize.emboss" },
    Known { class: "Dfs ", filter_id: code(b"Dfs "), name: "Diffuse...", command: "filter.stylize.diffuse" },
    Known { class: "TrcC", filter_id: code(b"TrcC"), name: "Trace Contour...", command: "filter.stylize.traceContour" },
    Known { class: "Crst", filter_id: code(b"Crst"), name: "Crystallize...", command: "filter.pixelate.crystallize" },
    Known { class: "Pntl", filter_id: code(b"Pntl"), name: "Pointillize...", command: "filter.pixelate.pointillize" },
    Known { class: "Wnd ", filter_id: code(b"Wnd "), name: "Wind...", command: "filter.stylize.wind" },
    Known { class: "Extr", filter_id: code(b"Extr"), name: "Extrude...", command: "filter.stylize.extrude" },
    Known { class: "Tls ", filter_id: code(b"Tls "), name: "Tiles...", command: "filter.stylize.tiles" },
    Known { class: "Lvls", filter_id: code(b"Lvls"), name: "Levels...", command: "image.adjustments.levels" },
    Known { class: "Crvs", filter_id: code(b"Crvs"), name: "Curves...", command: "image.adjustments.curves" },
    // Shadows/Highlights at its defaults stores no `Fltr` (`hasoptions` false).
    Known { class: "adaptCorrect", filter_id: 781, name: "Shado&ws/Highlights...", command: "image.adjustments.shadowsHighlights" },
    // Filters without options (measured in Photoshop): `hasoptions` false, no `Fltr`, only the
    // four-char `filterID`. The class is that code; see `NO_OPTIONS`.
    Known { class: "Blr ", filter_id: code(b"Blr "), name: "Blur", command: "filter.blur.blur" },
    Known { class: "BlrM", filter_id: code(b"BlrM"), name: "Blur More", command: "filter.blur.blurMore" },
    Known { class: "Dspc", filter_id: code(b"Dspc"), name: "Despeckle", command: "filter.noise.despeckle" },
    Known { class: "FndE", filter_id: code(b"FndE"), name: "Find Edges", command: "filter.stylize.findEdges" },
    Known { class: "Fct ", filter_id: code(b"Fct "), name: "Facet", command: "filter.pixelate.facet" },
    Known { class: "Frgm", filter_id: code(b"Frgm"), name: "Fragment", command: "filter.pixelate.fragment" },
    Known { class: "Shrp", filter_id: code(b"Shrp"), name: "Sharpen", command: "filter.sharpen.sharpen" },
    Known { class: "ShrM", filter_id: code(b"ShrM"), name: "Sharpen More", command: "filter.sharpen.sharpenMore" },
    Known { class: "ShrE", filter_id: code(b"ShrE"), name: "Sharpen Edges", command: "filter.sharpen.sharpenEdges" },
    Known { class: "Slrz", filter_id: code(b"Slrz"), name: "Solarize", command: "filter.stylize.solarize" },
    // Smart Sharpen has a numeric filterID and a stringID class, like Box Blur.
    Known { class: SMART_SHARPEN, filter_id: 1131, name: "Smart Sharpen...", command: "filter.sharpen.smartSharpen" },
    Known { class: "DstS", filter_id: code(b"DstS"), name: "Dust & Scratches...", command: "filter.noise.dustAndScratches" },
    // Distort (Photoshop 27, `corpus`-style check: one smart object per filter saved from Photoshop
    // and read back). Shear isn't mapped yet: its curve lives on Photoshop's own grid.
    Known { class: "Pnch", filter_id: code(b"Pnch"), name: "Pinch...", command: "filter.distort.pinch" },
    Known { class: "Plr ", filter_id: code(b"Plr "), name: "Polar Coordinates...", command: "filter.distort.polarCoordinates" },
    Known { class: "Rple", filter_id: code(b"Rple"), name: "Ripple...", command: "filter.distort.ripple" },
    Known { class: "Sphr", filter_id: code(b"Sphr"), name: "Spherize...", command: "filter.distort.spherize" },
    Known { class: "ZgZg", filter_id: code(b"ZgZg"), name: "ZigZag...", command: "filter.distort.zigZag" },
    // Image › Adjustments › Invert on a smart object: no settings, so no `Fltr` (`hasoptions` false).
    Known { class: "Invr", filter_id: code(b"Invr"), name: "Invert", command: "image.adjustments.invert" },
];

/// Classes of the filters that have no options: Photoshop stores them without a `Fltr`.
const NO_OPTIONS: [&str; 10] = ["Blr ", "BlrM", "Dspc", "FndE", "Fct ", "Frgm", "Shrp", "ShrM", "ShrE", "Slrz"];

// Smart Sharpen and Dust & Scratches
//
// Measured in Photoshop (applied through Action Manager to a smart object, saved, `filterFX` read
// back):
//
// ```text
// Smart Sharpen (filterID 1131): smartSharpen { presetKind = presetKindType enum (only on the
//   default entry), useLegacy bool, Amnt UntF #Prc, Rds  UntF #Pxl, noiseReduction UntF #Prc,
//   blur = blurType enum (GsnB | lensBlur | MtnB), Angl long (Motion Blur only) }
// Dust & Scratches (filterID DstS): DstS { Rds  long, Thsh long }
// ```
//
// PhotoCraft's Smart Sharpen removes a Gaussian blur, non-legacy: Lens Blur, Motion Blur, legacy
// mode, Shadow/Highlight settings or any other key keep the whole entry verbatim. `presetKind`
// only names the dialog's preset: it is accepted on open but not kept (the engine takes no such
// parameter) and not written (Photoshop leaves it out of entries with explicit values).

const SMART_SHARPEN: &str = "smartSharpen";
const SMART_SHARPEN_KEYS: [&str; 6] = ["presetKind", "useLegacy", "Amnt", "Rds ", "noiseReduction", "blur"];

/// A `UntF` value in `unit`, finite and within `lo..=hi`.
fn unit_in(d: &Descriptor, key: &str, u: &[u8; 4], lo: f64, hi: f64) -> Option<f64> {
    match d.get(key)? {
        Value::UnitFloat { unit, value } if unit == u && value.is_finite() && (lo..=hi).contains(value) => Some(*value),
        _ => None,
    }
}

/// `filter.sharpen.smartSharpen` params; `None` for a setting PhotoCraft doesn't render.
fn smart_sharpen_params(f: &Descriptor) -> Option<J> {
    if f.items.iter().any(|(k, _)| !SMART_SHARPEN_KEYS.iter().any(|x| k.is(x))) {
        return None;
    }
    let Some(Value::Boolean(false)) = f.get("useLegacy") else { return None };
    let Some(Value::Enumerated { type_id, value }) = f.get("blur") else { return None };
    if !type_id.is("blurType") || !value.is("GsnB") {
        return None;
    }
    let mut out = Map::new();
    out.insert("amount".into(), json!(unit_in(f, "Amnt", b"#Prc", 1.0, 500.0)?));
    out.insert("radius".into(), json!(unit_in(f, "Rds ", b"#Pxl", 0.1, 64.0)?));
    out.insert("reduceNoise".into(), json!(unit_in(f, "noiseReduction", b"#Prc", 0.0, 100.0)?));
    match f.get("presetKind") {
        None => {}
        Some(Value::Enumerated { type_id, .. }) if type_id.is("presetKindType") => {}
        Some(_) => return None,
    }
    Some(J::Object(out))
}

/// Photoshop enum codes ↔ PhotoCraft parameter values, per filter setting.
const RIPPLE_SIZE: [(&[u8; 4], &str); 3] = [(b"Sml ", "small"), (b"Mdm ", "medium"), (b"Lrg ", "large")];
const SPHERIZE_MODE: [(&[u8; 4], &str); 3] = [(b"Nrml", "normal"), (b"HrzO", "horizontalOnly"), (b"VrtO", "verticalOnly")];
const POLAR_MODE: [(&[u8; 4], &str); 2] = [(b"RctP", "rectangularToPolar"), (b"PlrR", "polarToRectangular")];
const ZIGZAG_STYLE: [(&[u8; 4], &str); 3] = [(b"PndR", "pondRipples"), (b"OtFr", "outFromCenter"), (b"ArnC", "aroundCenter")];

/// The PSD filter record that PhotoCraft's `command` is written as, if it has one.
pub(super) fn known_by_command(command: &str) -> Option<&'static Known> {
    KNOWN.iter().find(|k| k.command == command)
}

/// RGB channel references in Levels/Curves (`Chnl` enum) and their parameter keys ("" = the
/// composite: top-level keys).
const CHANNELS: [(&[u8; 4], &str); 4] = [(b"Cmps", ""), (b"Rd  ", "red"), (b"Grn ", "green"), (b"Bl  ", "blue")];

/// Mezzotint's `MztT` enum codes and our `type` values (measured in Photoshop by rendering each).
const MEZZOTINT_TYPES: [(&[u8; 4], &str); 10] = [
    (b"FnDt", "fineDots"),
    (b"MdmD", "mediumDots"),
    (b"GrnD", "grainyDots"),
    (b"CrsD", "coarseDots"),
    (b"ShrL", "shortLines"),
    (b"MdmL", "mediumLines"),
    (b"LngL", "longLines"),
    (b"ShSt", "shortStrokes"),
    (b"MdmS", "mediumStrokes"),
    (b"LngS", "longStrokes"),
];

/// Color Halftone's `Rds ` (max radius) and `Ang1`..`Ang4` (screen angles) longs, read within
/// the dialog's ranges; anything else is kept verbatim.
fn color_halftone_params(f: &Descriptor) -> Option<J> {
    if !only_keys(f, &["Rds ", "Ang1", "Ang2", "Ang3", "Ang4"]) {
        return None;
    }
    let radius = int(f, "Rds ").filter(|r| (4..=127).contains(r))?;
    let angle = |k: &str| int(f, k).filter(|a| (-360..=360).contains(a));
    Some(json!({"maxRadius": radius, "channel1": angle("Ang1")?, "channel2": angle("Ang2")?, "channel3": angle("Ang3")?, "channel4": angle("Ang4")?}))
}

/// A numeric descriptor item as a finite number; `None` when it is absent, not a number, or not finite.
pub(super) fn px(d: &Descriptor, key: &str) -> Option<f64> {
    num(d.get(key)).filter(|v| v.is_finite())
}

/// An integer descriptor item; `None` when it is absent or of another type.
pub(super) fn int(d: &Descriptor, key: &str) -> Option<i32> {
    match d.get(key)? {
        Value::Integer(i) => Some(*i),
        _ => None,
    }
}

/// `v` when it lies in `lo..=hi` (the filter dialog's range); out-of-range settings are not
/// opened as editable filters, so a re-save keeps Photoshop's value instead of a clamped one.
fn in_range(v: f64, lo: f64, hi: f64) -> Option<f64> {
    (lo..=hi).contains(&v).then_some(v)
}

/// Integer `key` of `f` when it lies in `lo..=hi`.
fn int_in(f: &Descriptor, key: &str, lo: i32, hi: i32) -> Option<i32> {
    int(f, key).filter(|v| (lo..=hi).contains(v))
}

/// True when `f` holds only the given keys, each at most once. A filter with a setting we don't
/// know about (Radial Blur's off-centre centre, for one) or a repeated one is kept verbatim rather
/// than losing that setting.
fn only_keys(f: &Descriptor, keys: &[&str]) -> bool {
    f.items.iter().all(|(k, _)| keys.iter().any(|x| k.is(x))) && keys.iter().all(|key| f.items.iter().filter(|(k, _)| k.is(key)).count() <= 1)
}

/// Our name for enum `key`'s value in `f` (of enum type `type_id`), from `(PSD value, our name)`
/// pairs. An enum of another type is not one we know, so the filter stays verbatim.
fn enum_name(f: &Descriptor, key: &str, type_id: &str, table: &[(&[u8; 4], &'static str)]) -> Option<&'static str> {
    let Some(Value::Enumerated { type_id: t, value }) = f.get(key) else { return None };
    if t.as_bytes() != type_id.as_bytes() {
        return None;
    }
    table.iter().find(|(c, _)| value.as_bytes() == &c[..]).map(|(_, n)| *n)
}

/// True when `f` holds each of the given keys once and nothing else.
fn exact_keys(f: &Descriptor, keys: &[&str]) -> bool {
    only_keys(f, keys) && keys.iter().all(|x| f.items.iter().any(|(k, _)| k.is(x)))
}

/// Finite `key` of `f` stored as a plain double (the type Photoshop writes and we write back).
fn double(f: &Descriptor, key: &str) -> Option<f64> {
    match f.get(key)? {
        Value::Double(v) if v.is_finite() => Some(*v),
        _ => None,
    }
}

/// `p[key]` (`default` when absent) when it is a number in `lo..=hi`, the range the filter opens
/// with; anything else is an error, so a saved filter always opens as the same editable filter.
fn pf_range(p: &J, key: &str, default: f64, lo: f64, hi: f64) -> Result<f64, String> {
    match p.get(key) {
        None | Some(J::Null) => Ok(default),
        Some(v) => v.as_f64().filter(|x| (lo..=hi).contains(x)).ok_or_else(|| format!("setting `{key}` ({v}) is not a number in {lo}..={hi}")),
    }
}

/// Integer `p[key]` (rounded; `default` when absent) in `lo..=hi`, as a PSD long.
fn pi_range(p: &J, key: &str, default: f64, lo: i32, hi: i32) -> Result<Value, String> {
    let v = pf_range(p, key, default, f64::from(i32::MIN), f64::from(i32::MAX))?.round();
    let range = f64::from(lo)..=f64::from(hi);
    if !range.contains(&v) {
        return Err(format!("setting `{key}` ({v}) is not a number in {lo}..={hi}"));
    }
    Ok(Value::Integer(v as i32))
}

/// Enum `type_id` holding the PSD value of our name `p[key]` (`default` when absent).
fn enum_param(p: &J, key: &str, default: &str, type_id: &str, table: &[(&[u8; 4], &'static str)]) -> Result<Value, String> {
    let name = match p.get(key) {
        None | Some(J::Null) => default,
        Some(v) => v.as_str().ok_or_else(|| format!("setting `{key}` is not text"))?,
    };
    let code = table.iter().find(|(_, n)| *n == name).map(|(c, _)| **c).ok_or_else(|| format!("{key} `{name}` has no PSD equivalent"))?;
    Ok(Value::Enumerated { type_id: Id::new(type_id), value: Id::Code(code) })
}

/// Smart Blur `SmBQ` / `SmBM` and Radial Blur `BlrM` / `BlrQ` values (measured in Photoshop).
const SMART_BLUR_QUALITY: [(&[u8; 4], &str); 3] = [(b"SBQL", "low"), (b"SBQM", "medium"), (b"SBQH", "high")];
const SMART_BLUR_MODE: [(&[u8; 4], &str); 3] = [(b"SBMN", "normal"), (b"SBME", "edgeOnly"), (b"SBMO", "overlayEdge")];
const RADIAL_METHOD: [(&[u8; 4], &str); 2] = [(b"Spn ", "spin"), (b"Zm  ", "zoom")];
const RADIAL_QUALITY: [(&[u8; 4], &str); 3] = [(b"Drft", "draft"), (b"Gd  ", "good"), (b"Bst ", "best")];

fn channel_key(d: &Descriptor) -> Option<&'static str> {
    let Some(Value::Reference(items)) = d.get("Chnl") else { return None };
    let Some(ReferenceItem::Enumerated { value, .. }) = items.first() else { return None };
    CHANNELS.iter().find(|(c, _)| value.as_bytes() == &c[..]).map(|(_, k)| *k)
}

fn adjustment_list(d: &Descriptor) -> Option<Vec<&Descriptor>> {
    let Some(Value::List(items)) = d.get("Adjs") else { return None };
    items
        .iter()
        .map(|v| match v {
            Value::Descriptor(x) => Some(x),
            _ => None,
        })
        .collect()
}

fn levels_params(f: &Descriptor) -> Option<J> {
    let mut out = Map::new();
    for a in adjustment_list(f)? {
        let key = channel_key(a)?;
        let mut o = Map::new();
        let pair = |k: &str| match a.get(k) {
            Some(Value::List(v)) if v.len() == 2 => Some((int_value(&v[0])?, int_value(&v[1])?)),
            _ => None,
        };
        if let Some((lo, hi)) = pair("Inpt") {
            o.insert("inBlack".into(), json!(lo));
            o.insert("inWhite".into(), json!(hi));
        }
        if let Some(g) = px(a, "Gmm ") {
            o.insert("gamma".into(), json!(g));
        }
        if let Some((lo, hi)) = pair("Otpt") {
            o.insert("outBlack".into(), json!(lo));
            o.insert("outWhite".into(), json!(hi));
        }
        if key.is_empty() {
            out.extend(o);
        } else {
            out.insert(key.into(), J::Object(o));
        }
    }
    Some(J::Object(out))
}

fn int_value(v: &Value) -> Option<i32> {
    match v {
        Value::Integer(i) => Some(*i),
        _ => None,
    }
}

fn curves_params(f: &Descriptor) -> Option<J> {
    let mut out = Map::new();
    for a in adjustment_list(f)? {
        let key = channel_key(a)?;
        // A pencil-drawn curve is a mapping of 256 output levels (`Mpng`) instead of points.
        if let Some(Value::List(map)) = a.get("Mpng") {
            let map: Option<Vec<J>> = map.iter().map(|v| int_value(v).map(|o| json!(o.clamp(0, 255)))).collect();
            out.insert(if key.is_empty() { "points".into() } else { key.into() }, J::Array(map.filter(|m| m.len() == 256)?));
            out.insert("pencil".into(), J::Bool(true));
            continue;
        }
        let Some(Value::List(pts)) = a.get("Crv ") else { return None };
        let pts: Option<Vec<J>> = pts
            .iter()
            .map(|p| match p {
                Value::Descriptor(p) => Some(json!([px(p, "Hrzn")?, px(p, "Vrtc")?])),
                _ => None,
            })
            .collect();
        out.insert(if key.is_empty() { "points".into() } else { key.into() }, J::Array(pts?));
    }
    Some(J::Object(out))
}

/// Diffuse's `Md  ` (`DfsM` enum, measured in Photoshop) ↔ our `mode`.
const DIFFUSE_MODES: [(&str, &str); 4] = [("Nrml", "normal"), ("DrkO", "darkenOnly"), ("LghO", "lightenOnly"), ("anisotropic", "anisotropic")];

/// Trace Contour's `Edg ` (`CntE` enum, measured in Photoshop) ↔ our `edge`.
const TRACE_EDGES: [(&str, &str); 2] = [("Upr ", "upper"), ("Lwr ", "lower")];

/// Our name for the enum value of `key` (of type `type_id`) through `table` (Photoshop's value, ours).
fn enum_from(d: &Descriptor, key: &str, type_id: &str, table: &[(&str, &'static str)]) -> Option<&'static str> {
    let Some(Value::Enumerated { type_id: t, value }) = d.get(key) else { return None };
    if t.as_bytes() != type_id.as_bytes() {
        return None;
    }
    table.iter().find(|(psd, _)| value.as_bytes() == psd.as_bytes()).map(|(_, ours)| *ours)
}

// Wind, Extrude and Tiles, measured in Photoshop (each code changes the rendered result).

/// Wind's `WndM` (enum `WndM`) ↔ our `method`.
const WIND_METHODS: [(&str, &str); 3] = [("Wnd ", "wind"), ("Blst", "blast"), ("Stgr", "stagger")];
/// Wind's `Drct` (enum `Drct`) ↔ our `direction`. Photoshop's `Left` streaks extend to the left
/// of an edge, which is our wind from the right.
const WIND_DIRECTIONS: [(&str, &str); 2] = [("Left", "fromRight"), ("Rght", "fromLeft")];
/// Extrude's `ExtT` (enum `ExtT`) ↔ our `type`.
const EXTRUDE_TYPES: [(&str, &str); 2] = [("Blks", "blocks"), ("Pyrm", "pyramids")];
/// Extrude's `ExtR` (enum `ExtR`) ↔ our `depthMode`.
const EXTRUDE_DEPTHS: [(&str, &str); 2] = [("Rndm", "random"), ("LvlB", "levelBased")];
/// Tiles' `FlCl` (enum `FlCl`) ↔ our `fill`.
const TILE_FILLS: [(&str, &str); 4] = [("FlBc", "background"), ("FlFr", "foreground"), ("FlIn", "inverse"), ("FlSm", "unaltered")];
/// Extrude's size and depth (`ExtS`, `ExtD`) and Tiles' count and offset (`TlNm`, `TlOf`), all
/// `long`s, in the engine's (and Photoshop's dialog) ranges.
const EXTRUDE_SIZE: (i32, i32) = (2, 255);
const EXTRUDE_DEPTH: (i32, i32) = (1, 255);
const TILE_COUNT: (i32, i32) = (1, 99);
const TILE_OFFSET: (i32, i32) = (1, 99);

/// The `bool` at `key`.
fn bool_at(d: &Descriptor, key: &str) -> Option<bool> {
    match d.get(key)? {
        Value::Boolean(b) => Some(*b),
        _ => None,
    }
}

/// Tiles' params; background and foreground fills use the colours the entry carries (`FrgC`,
/// `BckC`). `None` when a setting or a colour that is there can't be read.
fn tiles_params(f: &Descriptor, item: &Descriptor) -> Option<J> {
    let mut out = Map::new();
    out.insert("count".into(), json!(int_in(f, "TlNm", TILE_COUNT.0, TILE_COUNT.1)?));
    out.insert("maxOffset".into(), json!(int_in(f, "TlOf", TILE_OFFSET.0, TILE_OFFSET.1)?));
    out.insert("fill".into(), json!(enum_from(f, "FlCl", "FlCl", &TILE_FILLS)?));
    for (psd, ours) in [("FrgC", "foreground"), ("BckC", "background")] {
        if item.get(psd).is_some() {
            out.insert(ours.into(), get_desc(item, psd).and_then(rgbc_colour)?);
        }
    }
    Some(J::Object(out))
}

/// Offset › Undefined Areas: the `FlMd` enum (`Fl  `) and our `undefinedAreas`. On a layer
/// Photoshop labels `Bckg` "Set to Transparent"; a smart object is always a layer.
const OFFSET_FILL: [(&str, &str); 3] = [("Wrp ", "wrap"), ("Rpt ", "repeat"), ("Bckg", "transparent")];

/// Custom's ranges in Photoshop's dialog: matrix cells, Scale, Offset.
const CUSTOM_CELL: (i32, i32) = (-999, 999);
const CUSTOM_SCALE: (i32, i32) = (1, 9999);
const CUSTOM_OFFSET: (i32, i32) = (-9999, 9999);

/// `v` when it lies in `lo..=hi`.
fn long_in_range(v: Option<i32>, (lo, hi): (i32, i32)) -> Option<i32> {
    v.filter(|v| (lo..=hi).contains(v))
}

/// `filter.other.custom` params for a `Cstm` filter: `Mtrx` is 25 longs, row-major 5 × 5. Values
/// outside Photoshop's ranges are not modelled (the filter stays verbatim).
fn custom_params(f: &Descriptor) -> Option<J> {
    if !only_keys(f, &["Mtrx", "Scl ", "Ofst"]) {
        return None;
    }
    let Some(Value::List(m)) = f.get("Mtrx") else { return None };
    if m.len() != 25 {
        return None;
    }
    let kernel: Option<Vec<J>> = m.iter().map(|v| long_in_range(int_value(v), CUSTOM_CELL).map(|v| json!(v))).collect();
    let scale = long_in_range(int(f, "Scl "), CUSTOM_SCALE)?;
    let offset = long_in_range(int(f, "Ofst"), CUSTOM_OFFSET)?;
    Some(json!({"kernel": kernel?, "scale": scale, "offset": offset}))
}

/// Our command and parameters for a known Photoshop filter; `None` when the filter (or one of its
/// settings) is not modelled. `item` is the whole `filterFX` entry (Pointillize reads its `BckC`,
/// Tiles its colours).
pub(super) fn known_params(class: &str, f: Option<&Descriptor>, item: &Descriptor) -> Option<(&'static str, J)> {
    let k = KNOWN.iter().find(|k| k.class == class)?;
    let params = match (class, f) {
        ("adaptCorrect" | "Invr", None) => json!({}),
        (c, None) if NO_OPTIONS.contains(&c) => json!({}),
        (_, None) => return None,
        // Exactly the settings Photoshop writes (and we write back); an extra or repeated one keeps
        // the filter verbatim rather than lose it on save.
        ("Pnch", Some(f)) if exact_keys(f, &["Amnt"]) => json!({"amount": int(f, "Amnt")?}),
        ("Rple", Some(f)) if exact_keys(f, &["Amnt", "RplS"]) => json!({"amount": int(f, "Amnt")?, "size": enum_name(f, "RplS", "RplS", &RIPPLE_SIZE)?}),
        ("Sphr", Some(f)) if exact_keys(f, &["Amnt", "SphM"]) => json!({"amount": int(f, "Amnt")?, "mode": enum_name(f, "SphM", "SphM", &SPHERIZE_MODE)?}),
        ("Plr ", Some(f)) if exact_keys(f, &["Cnvr"]) => json!({"mode": enum_name(f, "Cnvr", "Cnvr", &POLAR_MODE)?}),
        ("ZgZg", Some(f)) if exact_keys(f, &["Amnt", "NmbR", "ZZTy"]) => {
            json!({"amount": int(f, "Amnt")?, "ridges": int(f, "NmbR")?, "style": enum_name(f, "ZZTy", "ZZTy", &ZIGZAG_STYLE)?})
        }
        ("GsnB" | "boxblur" | "Mdn " | "HghP" | "Mxm " | "Mnm ", Some(f)) => json!({"radius": px(f, "Rds ")?}),
        ("MtnB", Some(f)) => json!({"angle": int(f, "Angl")?, "distance": px(f, "Dstn")?}),
        ("UnsM", Some(f)) => json!({"amount": px(f, "Amnt")?, "radius": px(f, "Rds ")?, "threshold": int(f, "Thsh")?}),
        ("AdNs", Some(f)) => {
            let dist = match enum_of(f, "Dstr")? {
                b"Unfr" => "uniform",
                b"Gsn " => "gaussian",
                _ => return None,
            };
            let mono = match f.get("Mnch") {
                Some(Value::Boolean(b)) => *b,
                _ => return None,
            };
            json!({"amount": px(f, "Nose")?, "distribution": dist, "monochromatic": mono, "seed": int(f, "FlRs").unwrap_or(0)})
        }
        ("surfaceBlur", Some(f)) if only_keys(f, &["Rds ", "Thsh"]) => json!({
            "radius": unit_in(f, "Rds ", b"#Pxl", 1.0, 100.0)?,
            "threshold": int_in(f, "Thsh", 2, 255)?,
        }),
        ("SmrB", Some(f)) if only_keys(f, &["Rds ", "Thsh", "SmBQ", "SmBM"]) => json!({
            "radius": in_range(double(f, "Rds ")?, 0.1, 100.0)?,
            "threshold": in_range(double(f, "Thsh")?, 0.1, 100.0)?,
            "quality": enum_name(f, "SmBQ", "SmBQ", &SMART_BLUR_QUALITY)?,
            "mode": enum_name(f, "SmBM", "SmBM", &SMART_BLUR_MODE)?,
        }),
        ("RdlB", Some(f)) if only_keys(f, &["Amnt", "BlrM", "BlrQ"]) => json!({
            "amount": int_in(f, "Amnt", 1, 100)?,
            "method": enum_name(f, "BlrM", "BlrM", &RADIAL_METHOD)?,
            "quality": enum_name(f, "BlrQ", "BlrQ", &RADIAL_QUALITY)?,
        }),
        ("Msc ", Some(f)) => json!({"cellSize": px(f, "ClSz")?}),
        ("Mztn", Some(f)) if only_keys(f, &["MztT"]) => json!({"type": enum_name(f, "MztT", "MztT", &MEZZOTINT_TYPES)?}),
        ("ClrH", Some(f)) => color_halftone_params(f)?,
        ("Embs", Some(f)) => json!({"angle": int(f, "Angl")?, "height": int(f, "Hght")?, "amount": int(f, "Amnt")?}),
        (SMART_SHARPEN, Some(f)) => smart_sharpen_params(f)?,
        ("DstS", Some(f)) => {
            if f.items.iter().any(|(k, _)| !k.is("Rds ") && !k.is("Thsh")) {
                return None;
            }
            json!({"radius": int_in(f, "Rds ", 1, 500)?, "threshold": int_in(f, "Thsh", 0, 255)?})
        }
        ("Dfs ", Some(f)) => json!({"mode": enum_from(f, "Md  ", "DfsM", &DIFFUSE_MODES)?}),
        ("TrcC", Some(f)) => json!({"level": int_in(f, "Lvl ", 0, 255)?, "edge": enum_from(f, "Edg ", "CntE", &TRACE_EDGES)?}),
        ("Crst", Some(f)) => json!({"cellSize": int_in(f, "ClSz", CELL_SIZE.0, CELL_SIZE.1)?}),
        ("Pntl", Some(f)) => {
            let mut p = json!({"cellSize": int_in(f, "ClSz", CELL_SIZE.0, CELL_SIZE.1)?});
            // Pointillize paints the gaps in the background colour the entry carries.
            if item.get("BckC").is_some() {
                p["background"] = get_desc(item, "BckC").and_then(rgbc_colour)?;
            }
            p
        }
        ("Wnd ", Some(f)) => json!({
            "method": enum_from(f, "WndM", "WndM", &WIND_METHODS)?,
            "direction": enum_from(f, "Drct", "Drct", &WIND_DIRECTIONS)?,
        }),
        ("Extr", Some(f)) => json!({
            "type": enum_from(f, "ExtT", "ExtT", &EXTRUDE_TYPES)?,
            "size": int_in(f, "ExtS", EXTRUDE_SIZE.0, EXTRUDE_SIZE.1)?,
            "depth": int_in(f, "ExtD", EXTRUDE_DEPTH.0, EXTRUDE_DEPTH.1)?,
            "depthMode": enum_from(f, "ExtR", "ExtR", &EXTRUDE_DEPTHS)?,
            "solidFrontFaces": bool_at(f, "ExtF")?,
            "maskIncompleteBlocks": bool_at(f, "ExtM")?,
        }),
        ("Tls ", Some(f)) => tiles_params(f, item)?,
        ("Ofst", Some(f)) if only_keys(f, &["Hrzn", "Vrtc", "Fl  "]) => {
            json!({"horizontal": int(f, "Hrzn")?, "vertical": int(f, "Vrtc")?, "undefinedAreas": enum_from(f, "Fl  ", "FlMd", &OFFSET_FILL)?})
        }
        ("Cstm", Some(f)) => custom_params(f)?,
        ("Lvls", Some(f)) => levels_params(f)?,
        ("Crvs", Some(f)) => curves_params(f)?,
        _ => return None,
    };
    Some((k.command, params))
}

fn rgbc_colour(d: &Descriptor) -> Option<J> {
    let c = |k: &str| px(d, k).map(|v| (v / 255.0).clamp(0.0, 1.0));
    Some(json!([c("Rd  ")?, c("Grn ")?, c("Bl  ")?, 1.0]))
}

fn colour_rgb(v: &J) -> Option<[f64; 3]> {
    match v {
        J::Array(a) => Some([a.first()?.as_f64()?, a.get(1)?.as_f64()?, a.get(2)?.as_f64()?]),
        J::String(h) if h.len() == 7 && h.starts_with('#') => {
            let ch = |j: usize| h.get(j..j + 2).and_then(|s| u8::from_str_radix(s, 16).ok()).map(|v| f64::from(v) / 255.0);
            Some([ch(1)?, ch(3)?, ch(5)?])
        }
        _ => None,
    }
}

/// A `[r, g, b(, a)]` (0–1) or `#rrggbb` colour param as an `RGBC` descriptor (0–255).
pub(super) fn colour_rgbc(v: Option<&J>, default: f64) -> Result<Value, String> {
    let Some(v) = v else { return Ok(rgbc(default)) };
    let rgb = colour_rgb(v).filter(|c| c.iter().all(|v| v.is_finite())).ok_or("a colour is not [r, g, b] or #rrggbb")?;
    let c = |v: f64| Value::Double(v.clamp(0.0, 1.0) * 255.0);
    Ok(Value::Descriptor(Descriptor::new("RGBC").with("Rd  ", c(rgb[0])).with("Grn ", c(rgb[1])).with("Bl  ", c(rgb[2]))))
}

fn pf(p: &J, key: &str, default: f64) -> f64 {
    p.get(key).and_then(J::as_f64).filter(|v| v.is_finite()).unwrap_or(default)
}

fn pi(p: &J, key: &str, default: f64) -> Value {
    Value::Integer(pf(p, key, default).round().clamp(f64::from(i32::MIN), f64::from(i32::MAX)) as i32)
}

/// A whole-number setting as a long, clamped to `range` like the engine clamps it; a fraction
/// has no PSD equivalent (Photoshop stores these as integers).
fn whole(v: f64, (lo, hi): (i32, i32), what: &str) -> Result<Value, String> {
    if v.fract() != 0.0 {
        return Err(format!("{what} ({v}) is not a whole number"));
    }
    Ok(Value::Integer(v.clamp(f64::from(lo), f64::from(hi)) as i32))
}

fn channel_ref(code: &[u8; 4]) -> Value {
    Value::Reference(vec![ReferenceItem::Enumerated {
        class: Class { name: UnicodeString::new_nul(""), class_id: Id::new("Chnl") },
        type_id: Id::new("Chnl"),
        value: Id::Code(*code),
    }])
}

/// The `Adjs` entries of a Levels or Curves filter: the composite first, then red, green, blue.
fn adjustments(p: &J, curves: bool) -> Result<Value, String> {
    let obj = p.as_object().ok_or("parameters are not an object")?;
    let allowed: &[&str] =
        if curves { &["points", "red", "green", "blue", "pencil"] } else { &["inBlack", "inWhite", "gamma", "outBlack", "outWhite", "red", "green", "blue"] };
    if let Some(k) = obj.keys().find(|k| !is_meta(k) && !allowed.contains(&k.as_str())) {
        return Err(format!("setting `{k}` has no PSD equivalent"));
    }
    let mut list = Vec::new();
    for (code, key) in CHANNELS {
        let o = if key.is_empty() { Some(p) } else { p.get(key) };
        let Some(o) = o else { continue };
        let mut a = Descriptor::new(if curves { "CrvA" } else { "LvlA" }).with("Chnl", channel_ref(code));
        if curves {
            let src = if key.is_empty() { p.get("points") } else { Some(o) };
            let Some(pts) = src.and_then(J::as_array) else { continue };
            if p.get("pencil").and_then(J::as_bool) == Some(true) {
                let map: Option<Vec<Value>> =
                    pts.iter().map(|o| o.as_f64().filter(|o| o.is_finite()).map(|o| Value::Integer(o.round().clamp(0.0, 255.0) as i32))).collect();
                a = a.with("Mpng", Value::List(map.filter(|m| m.len() == 256).ok_or("a pencil-drawn curve is not 256 output levels")?));
                list.push(Value::Descriptor(a));
                continue;
            }
            let pts: Option<Vec<Value>> = pts
                .iter()
                .map(|q| {
                    let x = q.get(0)?.as_f64().filter(|v| v.is_finite())?;
                    let y = q.get(1)?.as_f64().filter(|v| v.is_finite())?;
                    Some(Value::Descriptor(Descriptor::new("Pnt ").with("Hrzn", Value::Double(x)).with("Vrtc", Value::Double(y))))
                })
                .collect();
            a = a.with("Crv ", Value::List(pts.ok_or("a curve point is not [input, output]")?));
        } else {
            if key.is_empty() && !["inBlack", "inWhite", "gamma", "outBlack", "outWhite"].iter().any(|k| o.get(k).is_some()) {
                continue;
            }
            a = a.with("Inpt", Value::List(vec![pi(o, "inBlack", 0.0), pi(o, "inWhite", 255.0)])).with("Gmm ", Value::Double(pf(o, "gamma", 1.0)));
            if o.get("outBlack").is_some() || o.get("outWhite").is_some() {
                a = a.with("Otpt", Value::List(vec![pi(o, "outBlack", 0.0), pi(o, "outWhite", 255.0)]));
            }
        }
        list.push(Value::Descriptor(a));
    }
    Ok(Value::List(list))
}

/// Crystallize's and Pointillize's cell size range (`ClSz`, a `long`).
const CELL_SIZE: (i32, i32) = (3, 300);

/// Our `key` (a string) as Photoshop's enum value through `table` (Photoshop's value, ours).
fn enum_out(p: &J, key: &str, default: &str, type_id: &str, table: &[(&str, &str)]) -> Result<Value, String> {
    let ours = match p.get(key) {
        None => default,
        Some(v) => v.as_str().ok_or_else(|| format!("`{key}` is not a string"))?,
    };
    let (psd, _) = table.iter().find(|(_, o)| *o == ours).ok_or_else(|| format!("{key} `{ours}` has no PSD equivalent"))?;
    Ok(enumv(type_id, psd))
}

/// `key` rounded to a `long` in `lo..=hi`.
fn pi_in(p: &J, key: &str, default: f64, lo: i32, hi: i32) -> Value {
    Value::Integer(pf(p, key, default).round().clamp(f64::from(lo), f64::from(hi)) as i32)
}

/// Param `key` as a `bool` (false when absent or not a bool, as the engine reads it).
fn bool_param(p: &J, key: &str) -> Value {
    Value::Boolean(p.get(key).and_then(J::as_bool).unwrap_or(false))
}

/// The `Fltr` descriptor of a modelled filter (`None` = written without options).
pub(super) fn fltr_for(k: &Known, p: &J) -> Result<Option<Descriptor>, String> {
    let d = Descriptor::new(k.class);
    Ok(Some(match k.class {
        "GsnB" | "boxblur" | "Mdn " | "HghP" | "Mxm " | "Mnm " => {
            if p.get("preserve").and_then(J::as_str).is_some_and(|s| s == "roundness") {
                return Err("“preserve roundness” has no PSD equivalent".into());
            }
            d.with("Rds ", unit(b"#Pxl", pf(p, "radius", 1.0)))
        }
        "MtnB" => d.with("Angl", pi(p, "angle", 0.0)).with("Dstn", unit(b"#Pxl", pf(p, "distance", 10.0))),
        "UnsM" => d.with("Amnt", unit(b"#Prc", pf(p, "amount", 50.0))).with("Rds ", unit(b"#Pxl", pf(p, "radius", 1.0))).with("Thsh", pi(p, "threshold", 0.0)),
        "AdNs" => {
            let gauss = p.get("distribution").and_then(J::as_str) == Some("gaussian");
            d.with("Dstr", Value::Enumerated { type_id: Id::new("Dstr"), value: Id::new(if gauss { "Gsn " } else { "Unfr" }) })
                .with("Nose", unit(b"#Prc", pf(p, "amount", 12.5)))
                .with("Mnch", Value::Boolean(p.get("monochromatic").and_then(J::as_bool).unwrap_or(false)))
                .with("FlRs", pi(p, "seed", 0.0))
        }
        "surfaceBlur" => d.with("Rds ", unit(b"#Pxl", pf_range(p, "radius", 5.0, 1.0, 100.0)?)).with("Thsh", pi_range(p, "threshold", 15.0, 2, 255)?),
        "SmrB" => d
            .with("Rds ", Value::Double(pf_range(p, "radius", 3.0, 0.1, 100.0)?))
            .with("Thsh", Value::Double(pf_range(p, "threshold", 25.0, 0.1, 100.0)?))
            .with("SmBQ", enum_param(p, "quality", "high", "SmBQ", &SMART_BLUR_QUALITY)?)
            .with("SmBM", enum_param(p, "mode", "normal", "SmBM", &SMART_BLUR_MODE)?),
        "RdlB" => {
            // Photoshop wrote no centre key for a centred blur; an off-centre one is unmeasured.
            for k in ["centerX", "centerY"] {
                if (pf_range(p, k, 0.5, 0.0, 1.0)? - 0.5).abs() > 1e-9 {
                    return Err("an off-centre Radial Blur centre is not written to PSD".into());
                }
            }
            d.with("Amnt", pi_range(p, "amount", 10.0, 1, 100)?)
                .with("BlrM", enum_param(p, "method", "spin", "BlrM", &RADIAL_METHOD)?)
                .with("BlrQ", enum_param(p, "quality", "good", "BlrQ", &RADIAL_QUALITY)?)
        }
        "Msc " => d.with("ClSz", unit(b"#Pxl", pf(p, "cellSize", 10.0))),
        // Photoshop's Mezzotint has no seed (it is random on every render), so `seed` is not saved.
        "Mztn" => {
            let t = p.get("type").and_then(J::as_str).unwrap_or("fineDots");
            let code = MEZZOTINT_TYPES.iter().find(|(_, k)| *k == t).map_or(b"FnDt", |(c, _)| *c);
            d.with("MztT", Value::Enumerated { type_id: Id::new("MztT"), value: Id::Code(*code) })
        }
        "ClrH" => {
            // Longs within the dialog's ranges (what the engine accepts: radius 4..127, angles ±360).
            let long = |key: &str, default: f64, lo: f64, hi: f64| Value::Integer(pf(p, key, default).round().clamp(lo, hi) as i32);
            d.with("Rds ", long("maxRadius", 8.0, 4.0, 127.0))
                .with("Ang1", long("channel1", 108.0, -360.0, 360.0))
                .with("Ang2", long("channel2", 162.0, -360.0, 360.0))
                .with("Ang3", long("channel3", 90.0, -360.0, 360.0))
                .with("Ang4", long("channel4", 45.0, -360.0, 360.0))
        }
        "Embs" => d.with("Angl", pi(p, "angle", 135.0)).with("Hght", pi(p, "height", 3.0)).with("Amnt", pi(p, "amount", 100.0)),
        SMART_SHARPEN => {
            // The engine's defaults and ranges (it clamps the same way when rendering).
            d.with("useLegacy", Value::Boolean(false))
                .with("Amnt", unit(b"#Prc", pf(p, "amount", 100.0).clamp(1.0, 500.0)))
                .with("Rds ", unit(b"#Pxl", pf(p, "radius", 1.0).clamp(0.1, 64.0)))
                .with("noiseReduction", unit(b"#Prc", pf(p, "reduceNoise", 10.0).clamp(0.0, 100.0)))
                .with("blur", enumv("blurType", "GsnB"))
        }
        // Photoshop stores whole pixels and levels.
        "DstS" => {
            let whole = |key: &str, default: f64, lo: f64, hi: f64| Value::Integer(pf(p, key, default).round().clamp(lo, hi) as i32);
            d.with("Rds ", whole("radius", 1.0, 1.0, 500.0)).with("Thsh", whole("threshold", 0.0, 0.0, 255.0))
        }
        // Photoshop stores no random seed for these: `seed` is ours only and is not written.
        "Dfs " => d.with("Md  ", enum_out(p, "mode", "normal", "DfsM", &DIFFUSE_MODES)?),
        "TrcC" => d.with("Lvl ", pi_in(p, "level", 128.0, 0, 255)).with("Edg ", enum_out(p, "edge", "lower", "CntE", &TRACE_EDGES)?),
        "Crst" => d.with("ClSz", pi_in(p, "cellSize", 10.0, CELL_SIZE.0, CELL_SIZE.1)),
        "Pntl" => d.with("ClSz", pi_in(p, "cellSize", 5.0, CELL_SIZE.0, CELL_SIZE.1)),
        // Photoshop stores no random seed for these: our `seed` is not written.
        "Wnd " => {
            d.with("WndM", enum_out(p, "method", "wind", "WndM", &WIND_METHODS)?).with("Drct", enum_out(p, "direction", "fromRight", "Drct", &WIND_DIRECTIONS)?)
        }
        "Extr" => d
            .with("ExtT", enum_out(p, "type", "blocks", "ExtT", &EXTRUDE_TYPES)?)
            .with("ExtS", pi_in(p, "size", 30.0, EXTRUDE_SIZE.0, EXTRUDE_SIZE.1))
            .with("ExtD", pi_in(p, "depth", 30.0, EXTRUDE_DEPTH.0, EXTRUDE_DEPTH.1))
            .with("ExtR", enum_out(p, "depthMode", "random", "ExtR", &EXTRUDE_DEPTHS)?)
            .with("ExtF", bool_param(p, "solidFrontFaces"))
            .with("ExtM", bool_param(p, "maskIncompleteBlocks")),
        "Tls " => d
            .with("TlNm", pi_in(p, "count", 10.0, TILE_COUNT.0, TILE_COUNT.1))
            .with("TlOf", pi_in(p, "maxOffset", 10.0, TILE_OFFSET.0, TILE_OFFSET.1))
            .with("FlCl", enum_out(p, "fill", "background", "FlCl", &TILE_FILLS)?),
        "Ofst" => {
            // As the engine reads it: anything but repeat / transparent wraps.
            let ours = p.get("undefinedAreas").and_then(J::as_str).unwrap_or("wrap");
            let fill = OFFSET_FILL.iter().find(|(_, o)| *o == ours).map_or("Wrp ", |(psd, _)| *psd);
            d.with("Hrzn", pi(p, "horizontal", 0.0)).with("Vrtc", pi(p, "vertical", 0.0)).with("Fl  ", enumv("FlMd", fill))
        }
        "Cstm" => {
            // The engine's identity kernel when none is given; missing cells are 0.
            let kernel = p.get("kernel").and_then(J::as_array);
            let cell = |i: usize| match kernel {
                Some(a) => a.get(i).and_then(J::as_f64).filter(|v| v.is_finite()).unwrap_or(0.0),
                None => f64::from(u8::from(i == 12)),
            };
            let m = (0..25).map(|i| whole(cell(i), CUSTOM_CELL, "a kernel value")).collect::<Result<Vec<_>, _>>()?;
            d.with("Scl ", whole(pf(p, "scale", 1.0), CUSTOM_SCALE, "Scale")?)
                .with("Ofst", whole(pf(p, "offset", 0.0), CUSTOM_OFFSET, "Offset")?)
                .with("Mtrx", Value::List(m))
        }
        "Lvls" => d.with("Adjs", adjustments(p, false)?),
        "Crvs" => d.with("Adjs", adjustments(p, true)?),
        "adaptCorrect" => {
            if p.as_object().is_some_and(|o| o.keys().any(|k| !is_meta(k))) {
                return Err("only default Shadows/Highlights settings are written to PSD".into());
            }
            return Ok(None);
        }
        // No settings to write; any parameter keys an old record carries have no effect on the
        // filter either, so nothing is lost.
        c if NO_OPTIONS.contains(&c) => return Ok(None),
        // Keys and order as Photoshop writes them.
        "Pnch" => d.with("Amnt", pi(p, "amount", 50.0)),
        "Rple" => d.with("Amnt", pi(p, "amount", 100.0)).with("RplS", enum_param(p, "size", "medium", "RplS", &RIPPLE_SIZE)?),
        "Sphr" => d.with("Amnt", pi(p, "amount", 100.0)).with("SphM", enum_param(p, "mode", "normal", "SphM", &SPHERIZE_MODE)?),
        "Plr " => d.with("Cnvr", enum_param(p, "mode", "rectangularToPolar", "Cnvr", &POLAR_MODE)?),
        "ZgZg" => {
            d.with("Amnt", pi(p, "amount", 10.0)).with("NmbR", pi(p, "ridges", 5.0)).with("ZZTy", enum_param(p, "style", "pondRipples", "ZZTy", &ZIGZAG_STYLE)?)
        }
        "Invr" => return Ok(None),
        _ => return Err("not a recognized filter".into()),
    }))
}

#[cfg(test)]
#[path = "smart_filter_tests.rs"]
mod tests;
