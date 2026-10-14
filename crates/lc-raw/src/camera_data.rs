//! Whole, pinned upstream camera data. See docs/CAMERA-DATA.md for provenance and precedence.
use crate::{BlackLevel, Mat3, Mode, RawData, RawFormat, RawImage, Rect};
use serde::Deserialize;
use serde_json::Value;
use std::{collections::BTreeMap, sync::OnceLock};

/// Camera-space coefficients used by darktable's profiled denoiser.
/// `b` retains darktable's fitted offset and is passed to its new VST unchanged.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, Deserialize)]
pub struct NoiseProfile {
    pub a: [f32; 3],
    pub b: [f32; 3],
}
#[derive(Clone, Debug, PartialEq)]
pub struct WbPreset {
    pub name: String,
    pub tuning: i32,
    pub multipliers: [f32; 3],
}

#[derive(Deserialize)]
struct Constants {
    camera_constants: Vec<Value>,
}
#[derive(Deserialize)]
struct Makers {
    #[serde(alias = "wb_presets")]
    noiseprofiles: Vec<Maker>,
}
#[derive(Deserialize)]
struct Maker {
    maker: String,
    models: Vec<Model>,
}
#[derive(Deserialize)]
struct Model {
    model: String,
    #[serde(default)]
    profiles: Vec<Profile>,
    #[serde(default)]
    presets: Vec<Preset>,
}
#[derive(Deserialize)]
struct Profile {
    iso: u32,
    a: [f32; 3],
    b: [f32; 3],
    #[serde(default)]
    skip: bool,
}
#[derive(Deserialize)]
struct Preset {
    name: String,
    #[serde(default)]
    tuning: i32,
    channels: [f32; 4],
}
#[derive(Deserialize, Default, Clone, PartialEq)]
#[serde(default)]
struct SpeedCamera {
    #[serde(rename = "@make")]
    make: String,
    #[serde(rename = "@model")]
    model: String,
    #[serde(rename = "@mode")]
    mode: String,
    #[serde(rename = "Crop")]
    crop: Option<Crop>,
    #[serde(rename = "Sensor")]
    sensors: Vec<Sensor>,
    #[serde(rename = "BlackAreas")]
    areas: Areas,
    #[serde(rename = "ColorMatrices")]
    matrices: Matrices,
    #[serde(rename = "Aliases")]
    aliases: Aliases,
}
#[derive(Deserialize, Default, Clone, PartialEq)]
struct Aliases {
    #[serde(rename = "Alias", default)]
    names: Vec<String>,
}
#[derive(Deserialize, Default, Clone, PartialEq)]
struct Matrices {
    #[serde(rename = "ColorMatrix", default)]
    matrices: Vec<Matrix>,
}
#[derive(Deserialize, Default, Clone, PartialEq)]
struct Matrix {
    #[serde(rename = "@planes")]
    planes: usize,
    #[serde(rename = "ColorMatrixRow", default)]
    rows: Vec<MatrixRow>,
}
#[derive(Deserialize, Default, Clone, PartialEq)]
struct MatrixRow {
    #[serde(rename = "@plane")]
    plane: usize,
    #[serde(rename = "$text")]
    values: String,
}
#[derive(Deserialize, Default, Clone, PartialEq)]
#[serde(default)]
struct Crop {
    #[serde(rename = "@x")]
    x: i64,
    #[serde(rename = "@y")]
    y: i64,
    #[serde(rename = "@width")]
    width: i64,
    #[serde(rename = "@height")]
    height: i64,
}
#[derive(Deserialize, Default, Clone, PartialEq)]
#[serde(default)]
struct Sensor {
    #[serde(rename = "@black")]
    black: f32,
    #[serde(rename = "@white")]
    white: f32,
    #[serde(rename = "@black_colors")]
    black_colors: String,
    #[serde(rename = "@iso_list")]
    iso_list: String,
    #[serde(rename = "@iso_min")]
    iso_min: u32,
    #[serde(rename = "@iso_max")]
    iso_max: u32,
}
#[derive(Deserialize, Default, Clone, PartialEq)]
struct Areas {
    #[serde(rename = "Vertical", default)]
    vertical: Vec<Vertical>,
    #[serde(rename = "Horizontal", default)]
    horizontal: Vec<Horizontal>,
}
#[derive(Deserialize, Clone, PartialEq)]
struct Vertical {
    #[serde(rename = "@x")]
    x: usize,
    #[serde(rename = "@width")]
    width: usize,
}
#[derive(Deserialize, Clone, PartialEq)]
struct Horizontal {
    #[serde(rename = "@y")]
    y: usize,
    #[serde(rename = "@height")]
    height: usize,
}
#[derive(Deserialize)]
struct Speed {
    #[serde(rename = "Camera")]
    cameras: Vec<SpeedCamera>,
}
struct Database {
    constants: BTreeMap<String, Value>,
    speed: Vec<SpeedCamera>,
    noise: BTreeMap<String, Vec<Profile>>,
    wb: BTreeMap<String, Vec<WbPreset>>,
}
fn key(make: &str, model: &str) -> String {
    let make = make.trim().to_ascii_uppercase();
    let model = model.trim().to_ascii_uppercase();
    // EXIF spells Nikon and Olympus with a corporate suffix; don't use fuzzy model matching.
    let make = match make.as_str() {
        "NIKON CORPORATION" => "NIKON",
        "OLYMPUS IMAGING CORP." | "OLYMPUS CORPORATION" | "OLYMPUS OPTICAL CO.,LTD" | "OLYMPUS OPTICAL CO., LTD." => "OLYMPUS",
        "OM DIGITAL SOLUTIONS" | "OM DIGITAL SOLUTIONS CORPORATION" => "OM SYSTEM",
        "RICOH IMAGING COMPANY, LTD." | "RICOH IMAGING COMPANY LTD." | "RICOH" if model.starts_with("PENTAX ") => "PENTAX",
        "RICOH IMAGING COMPANY, LTD." | "RICOH IMAGING COMPANY LTD." => "RICOH",
        _ => &make,
    };
    if make.is_empty() {
        return model;
    }
    if model.starts_with(&format!("{make} ")) { model } else { format!("{make} {model}") }
}
/// Remove JSON comments without treating URL slashes or escaped quotes inside strings as comments.
fn uncomment(input: &str) -> String {
    let b = input.as_bytes();
    let mut out = b.to_vec();
    let (mut i, mut quoted, mut escaped) = (0, false, false);
    while i < b.len() {
        if quoted {
            if escaped {
                escaped = false;
            } else if b[i] == b'\\' {
                escaped = true;
            } else if b[i] == b'"' {
                quoted = false;
            }
            i += 1;
        } else if b[i] == b'"' {
            quoted = true;
            i += 1;
        } else if b.get(i..i + 2) == Some(b"//") {
            while i < b.len() && b[i] != b'\n' {
                out[i] = b' ';
                i += 1;
            }
        } else if b.get(i..i + 2) == Some(b"/*") {
            out[i..i + 2].fill(b' ');
            i += 2;
            while i < b.len() && b.get(i..i + 2) != Some(b"*/") {
                if b[i] != b'\n' {
                    out[i] = b' ';
                }
                i += 1;
            }
            if i + 2 <= b.len() {
                out[i..i + 2].fill(b' ');
                i += 2;
            }
        } else {
            i += 1;
        }
    }
    String::from_utf8(out).unwrap_or_default()
}
fn parse() -> Result<Database, String> {
    let constants: Constants = serde_json::from_str(&uncomment(include_str!("../data/cameras/camconst.json"))).map_err(|e| e.to_string())?;
    let speed: Speed = quick_xml::de::from_str(include_str!("../data/cameras/cameras.xml")).map_err(|e| e.to_string())?;
    let noise: Makers = serde_json::from_str(include_str!("../data/cameras/noiseprofiles.json")).map_err(|e| e.to_string())?;
    let wb: Makers = serde_json::from_str(include_str!("../data/cameras/wb_presets.json")).map_err(|e| e.to_string())?;
    let mut db = Database { constants: BTreeMap::new(), speed: speed.cameras, noise: BTreeMap::new(), wb: BTreeMap::new() };
    for c in constants.camera_constants {
        let names = c["make_model"].as_array().cloned().unwrap_or_else(|| vec![c["make_model"].clone()]);
        for name in names.iter().filter_map(Value::as_str) {
            let name = name.to_ascii_uppercase();
            let name = name.strip_prefix("OM DIGITAL SOLUTIONS ").map(|m| key("OM SYSTEM", m)).unwrap_or(name);
            db.constants.insert(name, c.clone());
        }
    }
    for maker in noise.noiseprofiles {
        for mut m in maker.models {
            m.profiles.retain(|p| !p.skip && p.a.iter().all(|a| a.is_finite() && *a > 0.0) && p.b.iter().all(|b| b.is_finite()));
            m.profiles.sort_by_key(|p| p.iso);
            db.noise.insert(key(&maker.maker, &m.model), m.profiles);
        }
    }
    for maker in wb.noiseprofiles {
        for m in maker.models {
            let presets = m
                .presets
                .into_iter()
                .filter(|p| p.channels[..3].iter().all(|c| c.is_finite() && *c > 0.0))
                .map(|p| WbPreset { name: p.name, tuning: p.tuning, multipliers: std::array::from_fn(|i| p.channels[i] / p.channels[1]) })
                .collect();
            db.wb.insert(key(&maker.maker, &m.model), presets);
        }
    }
    Ok(db)
}
fn db() -> Option<&'static Database> {
    static DB: OnceLock<Result<Database, String>> = OnceLock::new();
    DB.get_or_init(parse).as_ref().ok()
}
/// Exact camera match, linear interpolation between measured ISO profiles; no extrapolation.
pub fn noise_profile(make: &str, model: &str, iso: u32) -> Option<NoiseProfile> {
    let profiles = db()?.noise.get(&key(make, model))?;
    let lo = profiles.iter().rev().find(|p| p.iso <= iso)?;
    let hi = profiles.iter().find(|p| p.iso >= iso)?;
    let t = if hi.iso == lo.iso { 0.0 } else { (iso - lo.iso) as f32 / (hi.iso - lo.iso) as f32 };
    Some(NoiseProfile {
        a: std::array::from_fn(|i| lo.a[i] + t * (hi.a[i] - lo.a[i])),
        b: std::array::from_fn(|i| lo.b[i] + t * (hi.b[i] - lo.b[i])),
    })
}
pub fn wb_presets(make: &str, model: &str) -> &'static [WbPreset] {
    db().and_then(|db| db.wb.get(&key(make, model))).map(Vec::as_slice).unwrap_or_default()
}
fn levels(v: &Value) -> Option<[f32; 4]> {
    if let Some(n) = v.as_f64() {
        return Some([n as f32; 4]);
    }
    let a = v.as_array()?;
    let values = a.iter().map(|v| v.as_f64().map(|v| v as f32)).collect::<Option<Vec<_>>>()?;
    match values.as_slice() {
        [v] => Some([*v; 4]),
        [r, g, b] => Some([*r, *g, *b, *g]),
        [r, g, b, g2] => Some([*r, *g, *b, *g2]),
        _ => None,
    }
}
fn iso_levels(v: &Value, iso: u32) -> Option<[f32; 4]> {
    if let Some(v) = levels(v) {
        return Some(v);
    }
    v.as_array()?
        .iter()
        .flat_map(|e| {
            let isos = e["iso"].as_array().cloned().unwrap_or_else(|| vec![e["iso"].clone()]);
            isos.into_iter().filter_map(|v| v.as_u64()).map(move |i| (i, &e["levels"]))
        })
        .min_by_key(|(i, _)| (i.abs_diff(iso as u64), std::cmp::Reverse(*i)))
        .and_then(|(_, v)| levels(v))
}
fn frame_value<'a>(v: &'a Value, field: &str, w: usize, h: usize) -> &'a Value {
    if let Some(entries) = v.as_array().filter(|v| v.first().is_some_and(Value::is_object)) {
        let find = |fw, fh| entries.iter().find(|e| e["frame"][0].as_u64() == Some(fw) && e["frame"][1].as_u64() == Some(fh));
        return find(w as u64, h as u64).or_else(|| find(0, 0)).map(|e| &e[field]).unwrap_or(&Value::Null);
    }
    v
}
fn crop_rect(c: &Crop, w: usize, h: usize) -> Option<Rect> {
    if c.x < 0 || c.y < 0 {
        return None;
    }
    let extent = |size: i64, offset: i64, full: usize| if size <= 0 { full as i64 + size - offset } else { size };
    let (width, height) = (extent(c.width, c.x, w), extent(c.height, c.y, h));
    if width <= 1 || height <= 1 || c.x + width > w as i64 || c.y + height > h as i64 {
        return None;
    }
    Some(Rect::new(c.x as usize, c.y as usize, width as usize, height as usize))
}
fn phase_levels(raw: &RawImage, values: [f32; 4]) -> BlackLevel {
    let Some(cfa) = &raw.cfa else {
        return BlackLevel::uniform(values[0]);
    };
    let cfa = cfa.shifted(raw.active_area.x, raw.active_area.y);
    let values = (0..cfa.height)
        .flat_map(|y| (0..cfa.width).map(move |x| (x, y)))
        .map(|(x, y)| {
            let c = cfa.color_at(x, y) as usize;
            values[if c == 1 && cfa.is_bayer() && (raw.active_area.y + y) % 2 == 1 { 3 } else { c.min(2) }]
        })
        .collect();
    BlackLevel { repeat_rows: cfa.height, repeat_cols: cfa.width, values, delta_h: vec![], delta_v: vec![] }
}
fn measured_black(raw: &RawImage, areas: &[Rect]) -> Option<BlackLevel> {
    let cfa = raw.cfa.as_ref()?;
    let mut samples: Vec<Vec<f32>> = vec![vec![]; cfa.width * cfa.height];
    for rect in areas {
        let rect = rect.clipped(raw.width, raw.height);
        // Keep light leaks and boundary defects out of the estimate.
        if rect.width <= 8 || rect.height <= 8 {
            continue;
        }
        let area = raw.active_area;
        let c = raw.crop.clipped(area.width, area.height);
        let a = if c.width > 1 && c.height > 1 { Rect::new(area.x + c.x, area.y + c.y, c.width, c.height) } else { area };
        if rect.x < a.x + a.width && a.x < rect.x + rect.width && rect.y < a.y + a.height && a.y < rect.y + rect.height {
            continue;
        }
        for y in rect.y + 4..rect.y + rect.height - 4 {
            for x in rect.x + 4..rect.x + rect.width - 4 {
                let v = raw.data.get(y * raw.width + x);
                if v.is_finite() && v >= 0.0 && v < raw.white_at(0) * 0.25 {
                    samples[(y % cfa.height) * cfa.width + x % cfa.width].push(v);
                }
            }
        }
    }
    let mut sensor = Vec::new();
    for mut s in samples {
        if s.len() < 32 {
            return None;
        }
        let k = s.len() / 2;
        s.select_nth_unstable_by(k, f32::total_cmp);
        sensor.push(s[k]);
    }
    let a = raw.active_area;
    let values = (0..cfa.height)
        .flat_map(|y| (0..cfa.width).map(move |x| (x, y)))
        .map(|(x, y)| sensor[((a.y + y) % cfa.height) * cfa.width + (a.x + x) % cfa.width])
        .collect();
    Some(BlackLevel { repeat_rows: cfa.height, repeat_cols: cfa.width, values, delta_h: vec![], delta_v: vec![] })
}
fn speed_camera<'a>(db: &'a Database, key: &str, bits: u32) -> Option<&'a SpeedCamera> {
    let matches = |c: &&SpeedCamera| {
        crate::camera_data::key(&c.make, &c.model) == key || c.aliases.names.iter().any(|a| crate::camera_data::key(&c.make, a) == key)
    };
    if let Some(c) = db.speed.iter().filter(matches).find(|c| c.mode.is_empty()) {
        return Some(c);
    }
    // Compression modes can differ. Use their calibration only when all matching bit-depth
    // modes agree; never apply a crop from sRaw, DNG, CHDK or an unknown aspect mode.
    let candidates: Vec<_> = db.speed.iter().filter(matches).filter(|c| c.mode.starts_with(&format!("{bits}bit"))).collect();
    let first = *candidates.first()?;
    candidates
        .iter()
        .all(|c| c.sensors == first.sensors && c.crop == first.crop && c.areas == first.areas && c.matrices == first.matrices)
        .then_some(first)
}
fn scaling(ranges: &Value, f: f64) -> f32 {
    if !f.is_finite() || f <= 0.0 {
        return 1.0;
    }
    // Translate exact APEX apertures to the traditional third-stop rounded values.
    let table = [0.7, 0.8, 0.9, 1.0, 1.1, 1.2, 1.4, 1.6, 1.8, 2.0, 2.2, 2.5, 2.8, 3.2, 3.5, 4.0, 4.5, 5.0, 5.6, 6.3, 7.1, 8.0, 9.0, 10.0];
    let f = table
        .iter()
        .enumerate()
        .find(|(i, _)| {
            let exact = 2f64.powf((*i as f64 / 3.0 - 1.0) * 0.5);
            f > exact * 0.97 && f < exact / 0.97
        })
        .map(|(_, v)| *v)
        .unwrap_or(f);
    let Some(entries) = ranges["aperture_scaling"].as_array() else {
        return 1.0;
    };
    entries
        .iter()
        .filter_map(|e| Some((e["aperture"].as_f64()?, e["scale_factor"].as_f64()?)))
        .filter(|(ap, _)| *ap >= f)
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, v)| v.max(1.0) as f32)
        .unwrap_or(1.0)
}
/// Apply calibration once at the shared decode boundary, for native and rawler decodes.
pub(crate) fn apply(raw: &mut RawImage, mode: Mode) {
    if raw.format == RawFormat::Dng || raw.cpp != 1 || raw.cfa.is_none() {
        return;
    }
    let Some(db) = db() else {
        return;
    };
    let make = raw.metadata.make.as_deref().unwrap_or_default();
    let name = key(make, raw.metadata.model.as_deref().unwrap_or_default());
    let iso = raw.metadata.iso.unwrap_or(0);
    let rt = db.constants.get(&name);
    let stored_max = 2f32.powi(raw.bits.min(16) as i32) - 1.0;
    let rs = speed_camera(db, &name, raw.bits).filter(|c| c.sensors.is_empty() || c.sensors.iter().any(|s| s.white > 0.0 && s.white <= stored_max));
    let a = raw.active_area;
    let custom_crop =
        (raw.crop != Rect::new(0, 0, a.width, a.height)).then_some(Rect::new(a.x + raw.crop.x, a.y + raw.crop.y, raw.crop.width, raw.crop.height));
    let decoder_black = raw.black.clone();
    let mut areas = Vec::new();
    let mut crop = None;
    if let Some(rs) = rs {
        crop = rs.crop.as_ref().and_then(|c| crop_rect(c, raw.width, raw.height));
        let sensor = rs
            .sensors
            .iter()
            .filter(|s| {
                if !s.iso_list.is_empty() {
                    s.iso_list.split_whitespace().filter_map(|i| i.parse::<u32>().ok()).any(|i| i == iso)
                } else {
                    iso >= s.iso_min && (s.iso_max == 0 || iso <= s.iso_max)
                }
            })
            .min_by_key(|s| s.iso_list.is_empty() && s.iso_min == 0 && s.iso_max == 0);
        if let Some(sensor) = sensor {
            // A generic table must not replace a decoder's measured/file white.
            // Explicit ISO entries can correct a static fallback; nominal bit maxima
            // are also fallbacks. RT's detailed overrides remain the next priority.
            let calibrated_iso = !sensor.iso_list.is_empty() || sensor.iso_min != 0 || sensor.iso_max != 0;
            if sensor.white > 0.0 && sensor.white <= stored_max && (calibrated_iso || raw.white.iter().all(|w| *w == stored_max)) {
                raw.white = vec![sensor.white];
            }
            if sensor.black >= 0.0 && sensor.black < raw.white_at(0) {
                let values: Vec<f32> = sensor.black_colors.split_whitespace().filter_map(|s| s.parse().ok()).collect();
                raw.black = if values.len() == 4 {
                    phase_levels(raw, [values[0], values[1], values[2], values[3]])
                } else {
                    BlackLevel::uniform(sensor.black)
                };
            }
        }
        areas.extend(rs.areas.vertical.iter().map(|a| Rect::new(a.x, 0, a.width, raw.height)));
        areas.extend(rs.areas.horizontal.iter().map(|a| Rect::new(0, a.y, raw.width, a.height)));
        if raw.color.color_matrix.iter().all(Option::is_none)
            && let Some(matrix) = rs.matrices.matrices.iter().find(|m| m.planes == 3 && m.rows.len() == 3)
        {
            let mut values = [[0.0; 3]; 3];
            let mut valid = true;
            for row in &matrix.rows {
                let v: Vec<f64> = row.values.split_whitespace().filter_map(|s| s.parse::<f64>().ok()).collect();
                if row.plane >= 3 || v.len() != 3 {
                    valid = false;
                    break;
                }
                values[row.plane] = std::array::from_fn(|i| v[i] / 10000.0);
            }
            if valid {
                raw.color.color_matrix[0] = Some(Mat3(values));
                raw.color.illuminant[0] = 21;
            }
        }
    }
    if let Some(rt) = rt {
        let ranges = &rt["ranges"];
        if let Some(mut white) = iso_levels(&ranges["white"], iso) {
            let factor = scaling(ranges, raw.metadata.f_number.unwrap_or(0.0));
            let max = ranges["white_max"].as_f64().unwrap_or(65535.0) as f32;
            let stored_max = 2f32.powi(raw.bits.min(16) as i32) - 1.0;
            // RT rawimage.cc shifts each integer white until it fits TIFF bps.
            // This maps the same sensor's 14-bit tables to its 12-bit capture modes.
            white = white.map(|v| {
                let mut v = (v * factor).min(max).trunc();
                while v > stored_max && v.is_finite() {
                    v = (v * 0.5).floor();
                }
                v
            });
            if white.iter().all(|v| v.is_finite() && *v > raw.black.mean() && *v <= stored_max) {
                raw.white = white.to_vec();
            }
        }
        if let Some(black) = iso_levels(&ranges["black"], iso) {
            // RT encodes absolute overrides as 65535+black. Nikon/Panasonic plain
            // entries are offsets from the decoder's frame black, not absolute values.
            let additive = name.starts_with("NIKON ") || name.starts_with("PANASONIC ");
            let values = black.map(|v| if v > 65535.0 { (v as u32 & 0xffff) as f32 } else { v });
            let mut level = phase_levels(raw, values);
            if additive && let Some(cfa) = &raw.cfa {
                let a = raw.active_area;
                for y in 0..level.repeat_rows {
                    for x in 0..level.repeat_cols {
                        let c = cfa.color_at(a.x + x, a.y + y) as usize;
                        let channel = if c == 1 && cfa.is_bayer() && (a.y + y) % 2 == 1 { 3 } else { c.min(2) };
                        if black[channel] <= 65535.0 {
                            level.values[y * level.repeat_cols + x] += decoder_black.at(x, y, 0, 1);
                        }
                    }
                }
                if black.iter().all(|v| *v <= 65535.0) {
                    level.delta_h = decoder_black.delta_h.clone();
                    level.delta_v = decoder_black.delta_v.clone();
                }
            }
            if level.values.iter().all(|v| v.is_finite() && *v >= 0.0 && *v < raw.white_at(0)) {
                raw.black = level;
            }
        }
        let c = frame_value(&rt["raw_crop"], "crop", raw.width, raw.height);
        if let Some(v) = c.as_array().filter(|v| v.len() == 4) {
            let v: Option<Vec<i64>> = v.iter().map(Value::as_i64).collect();
            if let Some(v) = v {
                crop = crop_rect(&Crop { x: v[0], y: v[1], width: v[2], height: v[3] }, raw.width, raw.height).or(crop);
            }
        }
        let a = frame_value(&rt["masked_areas"], "areas", raw.width, raw.height);
        if let Some(a) = a.as_array() {
            let a: Vec<usize> = a.iter().filter_map(Value::as_u64).map(|v| v as usize).collect();
            for r in a.as_chunks::<4>().0 {
                if r[2] > r[0] && r[3] > r[1] {
                    areas.push(Rect::new(r[1], r[0], r[3] - r[1], r[2] - r[0]));
                }
            }
        }
    }
    // The decoder's explicit default crop is file/measured calibration. Tables
    // fill the absence of one; they must not change its coordinate representation.
    if let Some(crop) = crop.filter(|_| custom_crop.is_none()) {
        // Black pattern is relative to the active origin. Reanchor when tables crop it.
        let old = raw.black.clone();
        let a = raw.active_area;
        raw.black.values = (0..old.repeat_rows)
            .flat_map(|y| (0..old.repeat_cols).map(move |x| (x, y)))
            .map(|(x, y)| {
                let sx = (crop.x + x + old.repeat_cols - a.x % old.repeat_cols) % old.repeat_cols;
                let sy = (crop.y + y + old.repeat_rows - a.y % old.repeat_rows) % old.repeat_rows;
                old.at(sx, sy, 0, 1)
            })
            .collect();
        raw.black.delta_h.clear();
        raw.black.delta_v.clear();
        raw.active_area = crop;
        raw.crop = Rect::new(0, 0, crop.width, crop.height);
    }
    if mode == Mode::Full {
        if let Some(black) = measured_black(raw, &areas) {
            raw.black = black;
        }
        if let Some(rt) = rt {
            repair_pdaf(raw, rt);
        }
        // Last resort when the strict per-frame detector finds a real lower
        // plateau: use its clip coordinate, without changing unpacked samples.
        let points = raw.saturation_points();
        if points.iter().any(Option::is_some) {
            let mut white = if raw.white.len() == 4 { [raw.white[0], raw.white[1], raw.white[2], raw.white[3]] } else { [raw.white_at(0); 4] };
            for c in 0..3 {
                if let Some(p) = points[c] {
                    white[c] = p as f32;
                    if c == 1 {
                        white[3] = p as f32;
                    }
                }
            }
            raw.white = white.to_vec();
        }
    }
}
/// Static calibration for the quality harness; it deliberately omits frame measurement and repair.
#[cfg(feature = "testgen")]
pub fn apply_tables_for_audit(raw: &mut RawImage) {
    apply(raw, Mode::Header);
}
// RT's green-triplet detector, followed by same-colour interpolation from clean rows.
// Detection is computed before any write; legitimate isolated detail stays untouched.
fn repair_pdaf(raw: &mut RawImage, rt: &Value) -> usize {
    let Some(pattern) = rt["pdaf_pattern"].as_array() else {
        return 0;
    };
    let pattern: Vec<i64> = pattern.iter().filter_map(Value::as_i64).collect();
    let Some(&period) = pattern.last().filter(|&&p| p > 0) else {
        return 0;
    };
    let offset = rt["pdaf_offset"].as_i64().unwrap_or(0);
    let Some(cfa) = raw.cfa.as_ref().filter(|c| c.is_bayer()) else {
        return 0;
    };
    let a = raw.active_area;
    let is_row = |y: usize| pattern[..pattern.len() - 1].contains(&((y as i64 - offset).rem_euclid(period)));
    let mut corrections = Vec::new();
    for y in a.y.max(3)..(a.y + a.height).min(raw.height.saturating_sub(3)) {
        if !(y.saturating_sub(1)..=y + 1).any(is_row) {
            continue;
        }
        let mut marked = vec![false; raw.width];
        for (x, m) in marked.iter_mut().enumerate().take((a.x + a.width).min(raw.width - 1)).skip(a.x.max(1)) {
            if cfa.color_at(x, y) != 1 {
                continue;
            }
            let g0 = raw.data.get(y * raw.width + x) - raw.black.mean();
            let g = [(-1, -1), (-1, 1), (1, -1), (1, 1)]
                .map(|(dy, dx)| raw.data.get((y as isize + dy) as usize * raw.width + (x as isize + dx) as usize) - raw.black.mean());
            let (gu, gd) = (g[0] + g[1], g[2] + g[3]);
            let (max, min) = (gu.max(gd), gu.min(gd));
            let d = (max - min) / max.max(1e-6);
            *m = g0 > g.into_iter().fold(f32::MIN, f32::max) && d < 0.2 && 1.0 - (gu + gd) / (4.0 * g0.max(1e-6)) > d.min(0.1);
        }
        for x in a.x.max(3)..(a.x + a.width).min(raw.width.saturating_sub(3)) {
            if !(x.saturating_sub(2)..=x + 2).any(|g| g >= 2 && g + 2 < raw.width && marked[g - 2] && marked[g] && marked[g + 2]) {
                continue;
            }
            let mut taps = Vec::new();
            for dy in [-4isize, -2, 2, 4] {
                let yy = y as isize + dy;
                if yy >= a.y as isize && yy < (a.y + a.height) as isize && !is_row(yy as usize) && cfa.color_at(x, yy as usize) == cfa.color_at(x, y)
                {
                    taps.push(raw.data.get(yy as usize * raw.width + x));
                }
            }
            if taps.len() >= 2 {
                corrections.push((y * raw.width + x, taps.iter().sum::<f32>() / taps.len() as f32));
            }
        }
    }
    for &(i, v) in &corrections {
        match &mut raw.data {
            RawData::U16(d) => d[i] = v.round().clamp(0.0, 65535.0) as u16,
            RawData::F32(d) => d[i] = v,
        }
    }
    corrections.len()
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    pub(super) fn raw(w: usize, h: usize) -> RawImage {
        RawImage {
            format: RawFormat::Arw,
            width: w,
            height: h,
            cpp: 1,
            data: RawData::U16(vec![1000; w * h]),
            cfa: crate::Cfa::bayer("RGGB"),
            bits: 14,
            black: BlackLevel::uniform(100.0),
            white: vec![16383.0],
            active_area: Rect::new(0, 0, w, h),
            crop: Rect::new(0, 0, w, h),
            orientation: crate::Orientation::Normal,
            color: Default::default(),
            wb_multipliers: None,
            linearized: false,
            opcodes: Default::default(),
            metadata: Default::default(),
        }
    }
    #[test]
    fn camdata_whole_files_parse_and_aliases() {
        let db = parse().unwrap();
        assert!(db.constants.len() > 350);
        assert!(db.speed.len() > 1400);
        assert!(db.noise.len() > 300);
        assert_eq!(db.wb.len(), 485);
        assert!(db.constants.contains_key(&key("Canon", "Canon EOS Rebel T2i")));
        assert!(!wb_presets("NIKON CORPORATION", "NIKON D850").is_empty());
        assert!(!wb_presets("OM DIGITAL SOLUTIONS", "OM-1").is_empty());
        assert!(noise_profile("OM DIGITAL SOLUTIONS", "OM-1", 200).is_some());
        assert!(!wb_presets("RICOH IMAGING COMPANY, LTD.", "RICOH GR III").is_empty());
        assert!(noise_profile("not a maker", "D850", 100).is_none());
        assert_eq!(uncomment(r#"{"url":"https://x","text":"a\"//b"} //end"#), r#"{"url":"https://x","text":"a\"//b"}      "#);
    }
    #[test]
    fn camdata_iso_aperture_and_frame_semantics() {
        let db = parse().unwrap();
        let c = &db.constants["CANON EOS 5D MARK II"]["ranges"];
        assert_eq!(iso_levels(&c["white"], 160), Some([12700.0; 4]));
        assert_eq!(iso_levels(&c["white"], 180), Some([15600.0; 4]));
        assert!((scaling(c, 2f64.sqrt()) - 1.077).abs() < 1e-6);
        assert_eq!(scaling(c, 5.6), 1.0);
        assert_eq!(crop_rect(&Crop { x: 4, y: 2, width: -6, height: 0 }, 100, 80), Some(Rect::new(4, 2, 90, 78)));
        let frames = serde_json::json!([{"frame":[100,80],"crop":[2,4,90,70]},{"frame":[0,0],"crop":[0,0,-2,-2]}]);
        assert_eq!(frame_value(&frames, "crop", 100, 80), &serde_json::json!([2, 4, 90, 70]));
        let lo = noise_profile("Canon", "EOS 5D Mark II", 160).unwrap();
        let hi = noise_profile("Canon", "EOS 5D Mark II", 200).unwrap();
        let mid = noise_profile("Canon", "EOS 5D Mark II", 180).unwrap();
        for i in 0..3 {
            assert!((mid.a[i] - (lo.a[i] + hi.a[i]) * 0.5).abs() < 1e-10);
        }
        assert!(noise_profile("Canon", "EOS 5D Mark II", 1).is_none());
    }
    #[test]
    fn camdata_per_channel_full_lazy_and_binned_normalization() {
        let mut r = raw(32, 32);
        r.white = vec![1100.0, 2100.0, 3100.0, 4100.0];
        for y in 0..32 {
            for x in 0..32 {
                if let RawData::U16(d) = &mut r.data {
                    d[y * 32 + x] = match (x % 2, y % 2) {
                        (0, 0) => 600,
                        (1, 0) => 1100,
                        (0, 1) => 2100,
                        _ => 1600,
                    };
                }
            }
        }
        let n = r.normalized().unwrap();
        let lazy = r.normalized_samples().unwrap();
        assert!(n.data.iter().all(|v| (*v - 0.5).abs() < 1e-6));
        for y in 0..32 {
            for x in 0..32 {
                assert!((lazy.get(x, y, 0) - 0.5).abs() < 1e-6);
            }
        }
        let b = r.develop_binned(2, 0.99).unwrap().unwrap();
        assert!(b.data.iter().flatten().all(|v| (*v - 0.5).abs() < 1e-6));
    }
    #[test]
    fn camdata_black_measurement_reanchors_and_rejects_hot_pixels() {
        let mut r = raw(64, 64);
        r.active_area = Rect::new(17, 19, 32, 32);
        if let RawData::U16(d) = &mut r.data {
            for y in 0..64 {
                for x in 0..16 {
                    d[y * 64 + x] = [101, 202, 303, 404][(y % 2) * 2 + x % 2];
                }
            }
            d[8 * 64 + 8] = 9999;
        }
        let b = measured_black(&r, &[Rect::new(0, 0, 16, 64)]).unwrap();
        assert_eq!(b.values, vec![404.0, 303.0, 202.0, 101.0]);
        assert!(measured_black(&r, &[Rect::new(0, 0, 4, 64)]).is_none());
    }
    #[test]
    fn camdata_pdaf_repairs_stripes_and_preserves_isolated_detail() {
        let mut r = raw(64, 64);
        if let RawData::U16(d) = &mut r.data {
            for x in 0..64 {
                d[24 * 64 + x] = 1400;
            }
            d[36 * 64 + 21] = 2000;
        }
        let c = serde_json::json!({"pdaf_pattern":[0,12],"pdaf_offset":0});
        let count = repair_pdaf(&mut r, &c);
        assert!(count > 30, "{count}");
        assert_eq!(r.data.get(24 * 64 + 20), 1000.0);
        assert_eq!(r.data.get(36 * 64 + 21), 2000.0);
    }
    #[test]
    fn camdata_explicit_crop_and_measured_white_precede_generic_tables() {
        let mut r = raw(32, 32);
        r.width = 4992;
        r.height = 3292;
        r.data = RawData::U16(vec![]);
        r.metadata.make = Some("NIKON CORPORATION".into());
        r.metadata.model = Some("NIKON D4".into());
        r.active_area = Rect::new(0, 0, 4992, 3292);
        r.crop = Rect::new(8, 6, 4928, 3280);
        r.white = vec![15520.0];
        r.bits = 14;
        apply(&mut r, Mode::Header);
        assert_eq!(r.crop, Rect::new(8, 6, 4928, 3280));
        assert_eq!(r.white, vec![15520.0]);
        r.crop = Rect::new(0, 0, 4992, 3292);
        r.white = vec![4095.0];
        r.bits = 12;
        apply(&mut r, Mode::Header);
        assert_eq!(r.active_area, Rect::new(2, 0, 4940, 3292));
        assert_eq!(r.white, vec![3880.0]);
    }
    #[test]
    fn camdata_dng_and_unknown_camera_remain_unchanged() {
        let mut r = raw(32, 32);
        r.format = RawFormat::Dng;
        r.metadata.make = Some("Canon".into());
        r.metadata.model = Some("EOS 5D Mark II".into());
        let before = r.clone();
        apply(&mut r, Mode::Full);
        assert_eq!(before, r);
        r.format = RawFormat::Arw;
        r.metadata.model = Some("Unknown".into());
        let before = r.clone();
        apply(&mut r, Mode::Full);
        assert_eq!(before, r);
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod quality_tests {
    use super::*;
    #[test]
    fn camdata_measured_white_recognizes_clipped_plateau() {
        let mut r = super::tests::raw(64, 64);
        r.format = RawFormat::Cr2;
        r.metadata.make = Some("Canon".into());
        r.metadata.model = Some("EOS 5D Mark II".into());
        r.metadata.iso = Some(160);
        r.black = BlackLevel::uniform(512.0);
        r.data = RawData::U16(vec![12700; 64 * 64]);
        assert_eq!(r.saturation_points(), [Some(12700); 3]);
        let before = r.normalized().unwrap();
        assert!(before.data.iter().all(|v| *v < 0.8));
        apply(&mut r, Mode::Full);
        assert_eq!(r.white, vec![12700.0; 4]);
        assert!(r.normalized().unwrap().data.iter().all(|v| (*v - 1.0).abs() < 1e-6));
    }
    #[test]
    fn camdata_six_stop_pushed_dark_frame_stays_neutral() {
        let mut r = super::tests::raw(64, 64);
        r.active_area = Rect::new(20, 0, 44, 64);
        r.crop = Rect::new(0, 0, 44, 64);
        r.black = BlackLevel::uniform(90.0);
        if let RawData::U16(d) = &mut r.data {
            for y in 0..64 {
                for x in 0..64 {
                    d[y * 64 + x] = [100, 120, 110, 130][(y % 2) * 2 + x % 2];
                }
            }
        }
        let before = r.normalized().unwrap();
        let old_bias = before.data.iter().sum::<f32>() / before.data.len() as f32 * 64.0;
        r.black = measured_black(&r, &[Rect::new(0, 0, 16, 64)]).unwrap();
        let after = r.normalized().unwrap();
        let new_bias = after.data.iter().sum::<f32>() / after.data.len() as f32 * 64.0;
        assert!(old_bias > 0.09, "before +6EV {old_bias}");
        assert!(new_bias.abs() < 1e-8, "after +6EV {new_bias}");
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod normalization_tests {
    use super::*;
    #[test]
    fn camdata_dng_writer_does_not_truncate_channel_whites() {
        let mut raw = super::tests::raw(32, 32);
        raw.white = vec![1100.0, 2100.0, 3100.0, 2100.0];
        assert!(matches!(crate::write_dng(&raw, &Default::default()), Err(crate::RawError::Unsupported(_))));
        raw.white = vec![3100.0; 4];
        let bytes = crate::write_dng(&raw, &Default::default()).unwrap();
        assert_eq!(crate::decode(&bytes).unwrap().white, vec![3100.0]);
    }

    #[test]
    fn camdata_normalized_color_preserves_file_facts_and_unclipped_chroma() {
        let mut raw = super::tests::raw(32, 32);
        raw.white = vec![1100.0, 2100.0, 3100.0, 2100.0];
        raw.black = BlackLevel { repeat_rows: 2, repeat_cols: 2, values: vec![100.0, 110.0, 120.0, 130.0], delta_h: vec![], delta_v: vec![] };
        raw.color.color_matrix[0] = Some(Mat3::IDENTITY);
        raw.color.illuminant[0] = 21;
        raw.wb_multipliers = Some([2.0, 1.0, 1.5]);
        let mut header = raw.clone();
        header.black = BlackLevel::uniform(100.0);
        header.data = RawData::U16(vec![]);
        apply(&mut raw, Mode::Full);
        apply(&mut header, Mode::Header);
        assert_eq!(raw.info(), header.info(), "measured black must not change header metadata");
        assert_eq!(raw.wb_multipliers, Some([2.0, 1.0, 1.5]));
        assert_eq!(raw.color.analog_balance, None);
        let ratio = crate::color::normalization_ratios(&raw);
        let mut baseline = raw.clone();
        baseline.white = vec![2100.0];
        let xy = crate::color::as_shot_white_xy(&baseline);
        let old = crate::color::camera_transform(&baseline, xy);
        let new = crate::color::camera_transform(&raw, xy);
        let original = [0.2, 0.25, 0.3];
        let normalized = std::array::from_fn(|i| original[i] * ratio[i]);
        let render = |t: crate::color::CameraTransform, p: [f64; 3]| t.matrix.apply(std::array::from_fn(|i| p[i] * t.wb[i] as f64));
        let a = render(old, original);
        let b = render(new, normalized);
        for i in 0..3 {
            assert!((a[i] / a[1] - b[i] / b[1]).abs() < 1e-6, "chroma {a:?} -> {b:?}");
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod fallback_tests {
    use super::*;
    #[test]
    fn camdata_nikon_twelve_bit_maps_channel_tables_like_rawtherapee() {
        let mut r = super::tests::raw(32, 32);
        r.bits = 12;
        r.white = vec![4095.0];
        r.metadata.make = Some("NIKON CORPORATION".into());
        r.metadata.model = Some("NIKON D800".into());
        r.metadata.iso = Some(100);
        apply(&mut r, Mode::Header);
        assert_eq!(r.white, vec![4075.0, 3925.0, 4075.0, 3925.0]);
    }
    #[test]
    fn camdata_strict_plateau_fallback_keeps_sensor_samples_and_header_facts() {
        let mut r = super::tests::raw(64, 64);
        r.data = RawData::U16(vec![14700; 64 * 64]);
        let data = r.data.clone();
        let info = r.info();
        apply(&mut r, Mode::Full);
        assert_eq!(r.white, vec![14700.0; 4]);
        assert_eq!(r.data, data);
        assert_eq!(r.info(), info);
        assert!(r.normalized().unwrap().data.iter().all(|v| (*v - 1.0).abs() < 1e-6));
    }
}

#[cfg(test)]
mod pdaf_metric_tests {
    use super::*;
    #[test]
    fn camdata_pdaf_row_fft_peak_drops_twenty_db() {
        let mut r = super::tests::raw(64, 84);
        if let RawData::U16(d) = &mut r.data {
            for y in (12..84).step_by(12) {
                for x in 0..64 {
                    d[y * 64 + x] = 1400;
                }
            }
        }
        let peak = |r: &RawImage| {
            let (mut re, mut im) = (0.0f64, 0.0f64);
            for y in 0..r.height {
                let row = (0..r.width).map(|x| r.data.get(y * r.width + x) as f64).sum::<f64>() / r.width as f64;
                let phase = std::f64::consts::TAU * y as f64 / 12.0;
                re += row * phase.cos();
                im += row * phase.sin();
            }
            re.hypot(im)
        };
        let before = peak(&r);
        repair_pdaf(&mut r, &serde_json::json!({"pdaf_pattern":[0,12],"pdaf_offset":0}));
        let reduction = 20.0 * (before / peak(&r)).log10();
        eprintln!("PDAF row-spacing Fourier peak reduction {reduction:.3} dB");
        assert!(reduction > 20.0, "{reduction}");
    }
}
