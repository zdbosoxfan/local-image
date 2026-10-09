//! local-image: **Filter › Camera Raw Filter…** (`filter.develop`) on layer pixels, through the
//! Library's develop engine ([`lightcraft_pipeline::render`]), so a layer develops exactly like a
//! rendered photo (a JPEG or TIFF) in the Library's Develop module.
//!
//! ```text
//! layer RGBA (document colour space, 8/16/32-bit)
//!   ──document profile → linear Rec.2020──► Rgb32f (alpha set aside)
//!   ──lightcraft_pipeline::render(SourceInfo::default(), settings)──► linear Rec.2020
//!   ──linear Rec.2020 → document profile──► layer RGB, alpha copied back unchanged
//! ```
//!
//! Matrix/TRC profiles (sRGB, Adobe RGB, ProPhoto, Display P3, linear spaces) convert
//! analytically; other profiles go through a precise `photocraft-cms` transform.
//!
//! A filter can't change the frame or use raw data, so [`sanitize`] turns those tools off (crop,
//! Geometry/Upright, orientation, lens profile, camera calibration, Enhance), as Photoshop's
//! Camera Raw Filter leaves them out. The film Negative conversion, masks, point colour,
//! retouching spots and manual optics stay.
//!
//! [`to_camera_raw`] / [`from_camera_raw`] map the subset Photoshop's Camera Raw Filter descriptor
//! understands (the older [`CameraRaw`] model) to and from [`DevelopSettings`] (PSD import/export
//! and the editor's fallback dialog).

use std::sync::{Arc, OnceLock};

pub use lightcraft_develop::DevelopSettings;
use lightcraft_develop::{Hsl, VignetteStyle, WbMode, Wheel as LcWheel};
use lightcraft_geom::Point;
use photocraft_algo::camera_raw::{CameraRaw, Wheel};
use photocraft_cms::{Builtin, ColorSpace, Curve, Profile, Transform, TransformOptions, math};
use photocraft_geom::Rect;
use photocraft_raster::{Surface, from_rgba_into};
use serde_json::Value;

/// The smart-filter / command id.
pub const COMMAND: &str = "filter.develop";

/// Params keys that are bookkeeping, not develop settings.
pub const META_KEYS: [&str; 4] = ["layer", "convert", "index", "target"];

// ------------------------------------------------------------------------------ settings

/// Develop settings from command / smart-filter params (missing fields take their defaults;
/// bookkeeping keys and `__`-prefixed private keys are ignored).
pub fn parse_settings(params: &Value) -> Result<DevelopSettings, String> {
    let mut v = params.clone();
    if let Value::Object(m) = &mut v {
        m.retain(|k, _| !k.starts_with("__") && !META_KEYS.contains(&k.as_str()));
    } else if v.is_null() {
        v = Value::Object(Default::default());
    }
    serde_json::from_value(v).map_err(|e| format!("develop settings: {e}"))
}

/// Turns off what a filter can't do: tools that change the frame (crop, Geometry/Upright,
/// orientation, Super Resolution) or need raw data (lens profile, camera calibration, AI Denoise /
/// Raw Details).
pub fn sanitize(s: &mut DevelopSettings) {
    let d = DevelopSettings::default();
    s.crop = d.crop;
    s.geometry = d.geometry.clone();
    s.orientation = d.orientation;
    s.enhance = d.enhance;
    s.calibration = d.calibration;
    s.optics.lens_profile = false;
}

/// [`parse_settings`] then [`sanitize`].
pub fn filter_settings(params: &Value) -> Result<DevelopSettings, String> {
    let mut s = parse_settings(params)?;
    sanitize(&mut s);
    Ok(s)
}

/// The settings that leave pixels alone.
pub fn identity() -> &'static DevelopSettings {
    static ID: OnceLock<DevelopSettings> = OnceLock::new();
    ID.get_or_init(|| {
        let mut d = DevelopSettings::default();
        sanitize(&mut d);
        d
    })
}

/// [`identity`] as JSON.
pub fn identity_json() -> Value {
    serde_json::to_value(identity()).unwrap_or_else(|_| Value::Object(Default::default()))
}

/// Section ids of the Develop panels hidden for a filter (see [`sanitize`]).
pub const HIDDEN_SECTIONS: [&str; 4] = ["calibration", "crop", "geometry", "lensProfile"];

// ------------------------------------------------------------------------------ colour

/// Linear Rec.2020 (the develop engine's working space) as an ICC profile.
pub fn linear_rec2020() -> &'static Profile {
    static P: OnceLock<Profile> = OnceLock::new();
    P.get_or_init(|| {
        let mut p = Builtin::Rec2020.profile().clone();
        p.trc = Some([Curve::Identity, Curve::Identity, Curve::Identity]);
        p.description = "Linear Rec. 2020".into();
        p.with_encoded_bytes()
    })
}

/// Converts between a document's RGB profile and linear Rec.2020.
pub enum Conv {
    /// Matrix/TRC: `lin = M · trc(rgb)`.
    Matrix {
        trc: Box<[Curve; 3]>,
        to: [[f32; 3]; 3],
        from: [[f32; 3]; 3],
    },
    /// Gray (a grayscale document's RGB view): neutral in, Rec.2020 luminance out.
    Gray {
        trc: Box<Curve>,
    },
    Cms {
        to: Arc<Transform>,
        from: Arc<Transform>,
    },
}

fn f32m(m: &math::Mat3) -> [[f32; 3]; 3] {
    m.map(|r| r.map(|v| v as f32))
}

#[inline]
fn mat(m: &[[f32; 3]; 3], p: [f32; 3]) -> [f32; 3] {
    [m[0][0] * p[0] + m[0][1] * p[1] + m[0][2] * p[2], m[1][0] * p[0] + m[1][1] * p[1] + m[1][2] * p[2], m[2][0] * p[0] + m[2][1] * p[1] + m[2][2] * p[2]]
}

impl Conv {
    /// For an RGB profile (a document's `composite_profile`).
    pub fn new(profile: &Profile) -> Result<Conv, String> {
        let work = linear_rec2020();
        if profile.color_space == ColorSpace::Rgb
            && profile.is_matrix_shaper()
            && let (Some(m), Some(trc), Some(w)) = (profile.matrix, profile.trc.clone(), work.matrix)
        {
            let wi = math::invert(&w).ok_or("singular working-space matrix")?;
            let to = math::mul(&wi, &m);
            return Ok(match math::invert(&to) {
                Some(from) => Conv::Matrix { trc: Box::new(trc), to: f32m(&to), from: f32m(&from) },
                // a gray profile seen as RGB: every colorant is the white point
                None => Conv::Gray { trc: Box::new(trc[1].clone()) },
            });
        }
        if let Some(trc) = &profile.gray_trc {
            return Ok(Conv::Gray { trc: Box::new(trc.clone()) });
        }
        let opts = TransformOptions { bpc: false, ..Default::default() };
        let to = photocraft_cms::cached(profile, work, opts).map_err(|e| e.to_string())?;
        let from = photocraft_cms::cached(work, profile, opts).map_err(|e| e.to_string())?;
        Ok(Conv::Cms { to, from })
    }

    /// Document-encoded RGB → linear Rec.2020, in place.
    pub fn to_working(&self, px: &mut [[f32; 3]]) {
        match self {
            Conv::Matrix { trc, to, .. } => {
                for p in px.iter_mut() {
                    let l = [trc[0].eval(p[0]), trc[1].eval(p[1]), trc[2].eval(p[2])];
                    *p = mat(to, l);
                }
            }
            Conv::Gray { trc } => {
                for p in px.iter_mut() {
                    *p = [trc.eval(p[1]); 3];
                }
            }
            Conv::Cms { to, .. } => to.apply(px.as_flattened_mut(), 3),
        }
    }

    /// Linear Rec.2020 → document-encoded RGB, in place.
    pub fn from_working(&self, px: &mut [[f32; 3]]) {
        match self {
            Conv::Matrix { trc, from, .. } => {
                for p in px.iter_mut() {
                    let l = mat(from, *p);
                    *p = [trc[0].eval_inverse(l[0]), trc[1].eval_inverse(l[1]), trc[2].eval_inverse(l[2])];
                }
            }
            Conv::Gray { trc } => {
                for p in px.iter_mut() {
                    let y = 0.2627 * p[0] + 0.6780 * p[1] + 0.0593 * p[2];
                    *p = [trc.eval_inverse(y.max(0.0)); 3];
                }
            }
            Conv::Cms { from, .. } => from.apply(px.as_flattened_mut(), 3),
        }
    }
}

// ------------------------------------------------------------------------------ rendering

/// Develops linear Rec.2020 pixels (`w × h`) with `s` as a rendered (non-raw) photo, at full
/// size. The result is linear Rec.2020 (0..1).
pub fn render_working(rgb: Vec<[f32; 3]>, w: usize, h: usize, s: &DevelopSettings) -> Result<Vec<[f32; 3]>, String> {
    use lightcraft_pipeline::{DeepSamples, OutputDepth, OutputSpace, RenderRequest, SourceInfo};
    if rgb.len() != w * h || w == 0 || h == 0 {
        return Err("develop: bad image size".into());
    }
    let src = lightcraft_raster::Rgb32f { width: w, height: h, data: rgb };
    let req = RenderRequest { space: OutputSpace::Rec2020, depth: OutputDepth::F32Linear, ..RenderRequest::fit(w, h) };
    let out = lightcraft_pipeline::render(&src, &SourceInfo::default(), s, &req);
    match out.deep {
        Some(d) if d.width == w && d.height == h => match d.samples {
            DeepSamples::F32(v) => Ok(v.as_chunks::<3>().0.to_vec()),
            DeepSamples::U16(_) => Err("develop: the pipeline returned 16-bit samples".into()),
        },
        Some(d) => Err(format!("develop: the pipeline changed the frame ({}×{} → {}×{})", w, h, d.width, d.height)),
        None => Err("develop: the pipeline produced no float output".into()),
    }
}

/// Develops straight RGBA pixels in the document's colour space (`profile`) in place; alpha is
/// left alone. Always runs the pipeline (previews; see [`develop_surface`] for the identity
/// shortcut).
pub fn develop_rgba(px: &mut [[f32; 4]], w: usize, h: usize, s: &DevelopSettings, profile: &Profile) -> Result<(), String> {
    let conv = Conv::new(profile)?;
    let mut rgb: Vec<[f32; 3]> = px.iter().map(|p| [p[0], p[1], p[2]]).collect();
    conv.to_working(&mut rgb);
    // Colours outside Rec.2020 (saturated ProPhoto) and above white (32-bit) develop at the gamut
    // edge; the part outside it is added back, so unchanged settings keep them.
    let clipped: Vec<[f32; 3]> = rgb.iter().map(|p| p.map(|v| v.clamp(0.0, 1.0))).collect();
    let residual: Vec<[f32; 3]> = rgb.iter().zip(&clipped).map(|(p, c)| [p[0] - c[0], p[1] - c[1], p[2] - c[2]]).collect();
    let mut out = render_working(clipped, w, h, s)?;
    for (o, r) in out.iter_mut().zip(&residual) {
        for c in 0..3 {
            o[c] += r[c];
        }
    }
    conv.from_working(&mut out);
    for (p, q) in px.iter_mut().zip(out) {
        p[0] = q[0];
        p[1] = q[1];
        p[2] = q[2];
    }
    Ok(())
}

/// Layer pixels over `area` as linear Rec.2020 RGB plus alpha (the Develop session's source).
pub fn surface_to_working(surf: &Surface, area: Rect, profile: &Profile) -> Result<(Vec<[f32; 3]>, Vec<f32>), String> {
    let (w, h) = (area.width() as usize, area.height() as usize);
    let mut px = vec![[0.0f32; 4]; w * h];
    surf.read_rgba_into(area, &mut px);
    let mut rgb: Vec<[f32; 3]> = px.iter().map(|p| [p[0], p[1], p[2]]).collect();
    Conv::new(profile)?.to_working(&mut rgb);
    rgb.iter_mut().for_each(|p| *p = p.map(|v| v.max(0.0)));
    Ok((rgb, px.iter().map(|p| p[3]).collect()))
}

/// The filter on a surface over `area` (RGB / Gray at the surface's depth; alpha bit-identical).
/// Identity settings return the surface unchanged.
pub fn develop_surface(surf: &Surface, area: Rect, s: &DevelopSettings, profile: &Profile) -> Result<Surface, String> {
    if area.is_empty() || s == identity() {
        return Ok(surf.clone());
    }
    develop_surface_always(surf, area, s, profile)
}

/// [`develop_surface`] without the identity shortcut.
pub fn develop_surface_always(surf: &Surface, area: Rect, s: &DevelopSettings, profile: &Profile) -> Result<Surface, String> {
    let fmt = surf.format();
    let (w, h) = (area.width() as usize, area.height() as usize);
    if w.checked_mul(h).is_none_or(|n| n > 512_000_000) {
        return Err("Camera Raw Filter: the layer is too large".into());
    }
    let mut px = vec![[0.0f32; 4]; w * h];
    surf.read_rgba_into(area, &mut px);
    develop_rgba(&mut px, w, h, s, profile)?;
    let n = fmt.channels();
    let orig = surf.read_region(area);
    let mut data = vec![0.0f32; w * h * n];
    for (i, (q, o)) in px.iter().zip(data.chunks_exact_mut(n)).enumerate() {
        from_rgba_into(&fmt, *q, o);
        if fmt.alpha {
            o[n - 1] = orig[i * n + n - 1];
        }
    }
    let mut out = surf.clone();
    out.write_region(area, &data);
    out.prune();
    Ok(out)
}

// ------------------------------------------------------------------------------ Camera Raw subset

/// Relative temperature (−100..100, rendered files) → Kelvin, the Library's scale.
pub fn rel_to_kelvin(r: f64) -> f64 {
    1e6 / (1e6 / 6500.0 - r.clamp(-100.0, 100.0) * 0.8)
}

/// Kelvin → relative temperature.
pub fn kelvin_to_rel(k: f64) -> f64 {
    ((1e6 / 6500.0 - 1e6 / k.max(1.0)) / 0.8).clamp(-100.0, 100.0)
}

fn lc_wheel(w: &Wheel) -> LcWheel {
    LcWheel { hue: f64::from(w.hue), sat: f64::from(w.sat), lum: f64::from(w.lum) }
}

fn cr_wheel(w: &LcWheel) -> Wheel {
    Wheel { hue: w.hue as f32, sat: w.sat as f32, lum: w.lum as f32 }
}

fn lc_curve(c: &[[f32; 2]]) -> Vec<Point> {
    c.iter().map(|p| Point { x: f64::from(p[0]) / 255.0, y: f64::from(p[1]) / 255.0 }).collect()
}

fn cr_curve(c: &[Point]) -> Vec<[f32; 2]> {
    let lvl = |v: f64| ((v.clamp(0.0, 1.0) * 255.0 * 1e4).round() / 1e4) as f32;
    c.iter().map(|p| [lvl(p.x), lvl(p.y)]).collect()
}

fn bands(m: &lightcraft_develop::Mixer) -> [&Hsl; 8] {
    [&m.red, &m.orange, &m.yellow, &m.green, &m.aqua, &m.blue, &m.purple, &m.magenta]
}

fn bands_mut(m: &mut lightcraft_develop::Mixer) -> [&mut Hsl; 8] {
    [&mut m.red, &mut m.orange, &mut m.yellow, &mut m.green, &mut m.aqua, &mut m.blue, &mut m.purple, &mut m.magenta]
}

/// The [`CameraRaw`] subset of `s` (what Photoshop's Camera Raw Filter descriptor holds).
pub fn to_camera_raw(s: &DevelopSettings) -> CameraRaw {
    let f = |v: f64| v as f32;
    let temperature = match s.wb.mode {
        WbMode::Custom => f(kelvin_to_rel(s.wb.temp)),
        _ => 0.0,
    };
    let tint = match s.wb.mode {
        WbMode::Custom => f(s.wb.tint.clamp(-100.0, 100.0)),
        _ => 0.0,
    };
    let b = bands(&s.mixer);
    CameraRaw {
        temperature,
        tint,
        exposure: f(s.light.exposure),
        contrast: f(s.light.contrast),
        highlights: f(s.light.highlights),
        shadows: f(s.light.shadows),
        whites: f(s.light.whites),
        blacks: f(s.light.blacks),
        texture: f(s.effects.texture),
        clarity: f(s.effects.clarity),
        dehaze: f(s.effects.dehaze),
        vibrance: f(s.color.vibrance),
        saturation: f(s.color.saturation),
        curve_highlights: f(s.curve.highlights),
        curve_lights: f(s.curve.lights),
        curve_darks: f(s.curve.darks),
        curve_shadows: f(s.curve.shadows),
        curve_splits: [f(s.curve.split_shadows), f(s.curve.split_mid), f(s.curve.split_highlights)],
        point_curve: cr_curve(&s.curve.master),
        point_curve_red: cr_curve(&s.curve.red),
        point_curve_green: cr_curve(&s.curve.green),
        point_curve_blue: cr_curve(&s.curve.blue),
        hsl_hue: b.map(|h| f(h.hue)),
        hsl_sat: b.map(|h| f(h.sat)),
        hsl_lum: b.map(|h| f(h.lum)),
        grade_shadows: cr_wheel(&s.grading.shadows),
        grade_midtones: cr_wheel(&s.grading.midtones),
        grade_highlights: cr_wheel(&s.grading.highlights),
        grade_global: cr_wheel(&s.grading.global),
        grade_blending: f(s.grading.blending),
        grade_balance: f(s.grading.balance),
        sharpen_amount: f(s.detail.sharpen_amount),
        sharpen_radius: f(s.detail.sharpen_radius),
        sharpen_detail: f(s.detail.sharpen_detail),
        sharpen_masking: f(s.detail.sharpen_masking),
        noise_luminance: f(s.detail.nr_luminance),
        noise_luminance_detail: f(s.detail.nr_detail),
        noise_color: f(s.detail.nr_color),
        noise_color_detail: f(s.detail.nr_color_detail),
        grain_amount: f(s.grain.amount),
        grain_size: f(s.grain.size),
        grain_roughness: f(s.grain.roughness),
        vignette_amount: f(s.vignette.amount),
        vignette_midpoint: f(s.vignette.midpoint),
        vignette_roundness: f(s.vignette.roundness),
        vignette_feather: f(s.vignette.feather),
        vignette_highlights: f(s.vignette.highlights),
        vignette_style: match s.vignette.style {
            VignetteStyle::HighlightPriority => "highlightPriority",
            VignetteStyle::ColorPriority => "colorPriority",
            VignetteStyle::PaintOverlay => "paintOverlay",
        }
        .into(),
        seed: s.grain.seed,
        pixel_scale: 1.0,
    }
}

/// `base` with the [`CameraRaw`] subset replaced by `cr` (everything else is kept).
pub fn from_camera_raw(cr: &CameraRaw, base: &DevelopSettings) -> DevelopSettings {
    let d = f64::from;
    let mut s = base.clone();
    if cr.temperature != 0.0 || cr.tint != 0.0 {
        s.wb.mode = WbMode::Custom;
        s.wb.temp = rel_to_kelvin(d(cr.temperature));
        s.wb.tint = d(cr.tint);
    } else if s.wb.mode == WbMode::Custom {
        s.wb = lightcraft_develop::WhiteBalance::default();
    }
    s.light.exposure = d(cr.exposure);
    s.light.contrast = d(cr.contrast);
    s.light.highlights = d(cr.highlights);
    s.light.shadows = d(cr.shadows);
    s.light.whites = d(cr.whites);
    s.light.blacks = d(cr.blacks);
    s.effects.texture = d(cr.texture);
    s.effects.clarity = d(cr.clarity);
    s.effects.dehaze = d(cr.dehaze);
    s.color.vibrance = d(cr.vibrance);
    s.color.saturation = d(cr.saturation);
    s.curve.highlights = d(cr.curve_highlights);
    s.curve.lights = d(cr.curve_lights);
    s.curve.darks = d(cr.curve_darks);
    s.curve.shadows = d(cr.curve_shadows);
    s.curve.split_shadows = d(cr.curve_splits[0]);
    s.curve.split_mid = d(cr.curve_splits[1]);
    s.curve.split_highlights = d(cr.curve_splits[2]);
    s.curve.master = lc_curve(&cr.point_curve);
    s.curve.red = lc_curve(&cr.point_curve_red);
    s.curve.green = lc_curve(&cr.point_curve_green);
    s.curve.blue = lc_curve(&cr.point_curve_blue);
    for (i, h) in bands_mut(&mut s.mixer).into_iter().enumerate() {
        h.hue = d(cr.hsl_hue[i]);
        h.sat = d(cr.hsl_sat[i]);
        h.lum = d(cr.hsl_lum[i]);
    }
    s.grading.shadows = lc_wheel(&cr.grade_shadows);
    s.grading.midtones = lc_wheel(&cr.grade_midtones);
    s.grading.highlights = lc_wheel(&cr.grade_highlights);
    s.grading.global = lc_wheel(&cr.grade_global);
    s.grading.blending = d(cr.grade_blending);
    s.grading.balance = d(cr.grade_balance);
    s.detail.sharpen_amount = d(cr.sharpen_amount);
    s.detail.sharpen_radius = d(cr.sharpen_radius);
    s.detail.sharpen_detail = d(cr.sharpen_detail);
    s.detail.sharpen_masking = d(cr.sharpen_masking);
    s.detail.nr_luminance = d(cr.noise_luminance);
    s.detail.nr_detail = d(cr.noise_luminance_detail);
    s.detail.nr_color = d(cr.noise_color);
    s.detail.nr_color_detail = d(cr.noise_color_detail);
    s.grain.amount = d(cr.grain_amount);
    s.grain.size = d(cr.grain_size);
    s.grain.roughness = d(cr.grain_roughness);
    s.grain.seed = cr.seed;
    s.vignette.amount = d(cr.vignette_amount);
    s.vignette.midpoint = d(cr.vignette_midpoint);
    s.vignette.roundness = d(cr.vignette_roundness);
    s.vignette.feather = d(cr.vignette_feather);
    s.vignette.highlights = d(cr.vignette_highlights);
    s.vignette.style = match cr.vignette_style.as_str() {
        "colorPriority" => VignetteStyle::ColorPriority,
        "paintOverlay" => VignetteStyle::PaintOverlay,
        _ => VignetteStyle::HighlightPriority,
    };
    s
}

/// Whether `s` holds anything the [`CameraRaw`] subset can't express (masks, point colour, a
/// profile, B&W, optics…): then the full settings must be stored beside a Photoshop descriptor.
pub fn beyond_camera_raw(s: &DevelopSettings) -> bool {
    from_camera_raw(&to_camera_raw(s), &DevelopSettings::default()) != *s
}

#[cfg(test)]
mod tests {
    use super::*;
    use photocraft_color::{ColorMode, PixelFormat, SampleType};

    fn pattern(fmt: PixelFormat, w: i32, h: i32) -> Surface {
        let mut s = Surface::new(fmt);
        let mut data = Vec::new();
        for y in 0..h {
            for x in 0..w {
                let (r, g, b) = ((x * 7 % 23) as f32 / 22.0, (y * 5 % 19) as f32 / 18.0, ((x + y) % 11) as f32 / 10.0);
                let a = if x < 3 { 0.0 } else { (x % 5) as f32 / 4.0 * 0.5 + 0.5 };
                let mut o = vec![0.0; fmt.channels()];
                from_rgba_into(&fmt, [r, g, b, a], &mut o);
                data.extend(o);
            }
        }
        s.write_region(Rect::new(0, 0, w, h), &data);
        s
    }

    fn profiles() -> Vec<(&'static str, Profile)> {
        vec![
            ("sRGB", Builtin::Srgb.profile().clone()),
            ("Adobe RGB", Builtin::AdobeRgbCompat.profile().clone()),
            ("ProPhoto", Builtin::ProPhotoCompat.profile().clone()),
            ("Display P3", Builtin::DisplayP3.profile().clone()),
            ("linear sRGB", Builtin::LinearSrgb.profile().clone()),
        ]
    }

    #[test]
    fn identity_settings_leave_pixels_within_one_level_and_alpha_untouched() {
        let area = Rect::new(0, 0, 40, 24);
        for (name, profile) in profiles() {
            for sample in [SampleType::U8, SampleType::U16, SampleType::F32] {
                let fmt = PixelFormat::new(ColorMode::Rgb, sample, true);
                let src = pattern(fmt, 40, 24);
                // the shortcut
                assert_eq!(develop_surface(&src, area, identity(), &profile).unwrap().read_region(area), src.read_region(area));
                // the full pipeline
                let out = develop_surface_always(&src, area, identity(), &profile).unwrap();
                let (a, b) = (src.read_region(area), out.read_region(area));
                let mut worst = 0.0f32;
                for (p, q) in a.as_chunks::<4>().0.iter().zip(b.as_chunks::<4>().0) {
                    assert_eq!(p[3].to_bits(), q[3].to_bits(), "{name} {sample:?}: alpha changed");
                    // linear data is compared as it displays (sRGB-encoded)
                    let enc = |v: f32| if name.starts_with("linear") { photocraft_cms::curve::srgb_trc().eval_inverse(v.clamp(0.0, 1.0)) } else { v };
                    for c in 0..3 {
                        worst = worst.max((enc(p[c]) - enc(q[c])).abs());
                    }
                }
                assert!(worst <= 1.0 / 255.0, "{name} {sample:?}: identity moved a channel by {worst}");
            }
        }
    }

    #[test]
    fn exposure_brightens_and_alpha_stays() {
        let fmt = PixelFormat::new(ColorMode::Rgb, SampleType::U16, true);
        let src = pattern(fmt, 32, 16);
        let area = Rect::new(0, 0, 32, 16);
        let mut s = identity().clone();
        s.light.exposure = 1.0;
        let out = develop_surface(&src, area, &s, Builtin::Srgb.profile()).unwrap();
        let (a, b) = (src.read_region(area), out.read_region(area));
        let mean = |v: &[f32]| v.as_chunks::<4>().0.iter().map(|p| p[1]).sum::<f32>() / (v.len() / 4) as f32;
        assert!(mean(&b) > mean(&a) + 0.05);
        for (p, q) in a.as_chunks::<4>().0.iter().zip(b.as_chunks::<4>().0) {
            assert_eq!(p[3].to_bits(), q[3].to_bits());
        }
    }

    #[test]
    fn grayscale_documents_develop() {
        let fmt = PixelFormat::new(ColorMode::Grayscale, SampleType::U8, false);
        let src = pattern(fmt, 16, 8);
        let p = Builtin::SGray.profile().gray_as_rgb().unwrap();
        let mut s = identity().clone();
        s.light.exposure = -1.0;
        let area = Rect::new(0, 0, 16, 8);
        let out = develop_surface(&src, area, &s, &p).unwrap();
        assert!(out.read_region(area).iter().sum::<f32>() < src.read_region(area).iter().sum::<f32>());
    }

    #[test]
    fn sanitize_turns_off_frame_and_raw_tools() {
        let mut s = DevelopSettings::default();
        s.crop.flip_h = true;
        s.geometry.rotate = 5.0;
        s.optics.lens_profile = true;
        s.enhance.super_resolution = true;
        s.calibration.shadows_tint = 10.0;
        s.light.exposure = 0.5;
        s.negative.enabled = true;
        sanitize(&mut s);
        assert!(!s.crop.flip_h && s.geometry.rotate == 0.0 && !s.optics.lens_profile && !s.enhance.super_resolution);
        assert_eq!(s.calibration, DevelopSettings::default().calibration);
        assert!(s.light.exposure == 0.5 && s.negative.enabled, "tools a filter can use stay");
        assert_eq!(*identity(), filter_settings(&serde_json::json!({"layer": 3, "__cameraRawPsd": "00"})).unwrap());
    }

    #[test]
    fn camera_raw_subset_round_trips_exactly() {
        let cr: CameraRaw = serde_json::from_value(serde_json::json!({
            "exposure": 1.15, "contrast": -38, "highlights": 35, "shadows": -50, "whites": 49, "blacks": -34,
            "curveDarks": -21, "curveLights": 38, "hslHue": [-50, 0, 29, 1, 0, 0, 0, 0],
            "temperature": -26, "tint": -25, "texture": -31, "clarity": 30, "dehaze": -34, "vibrance": 33, "saturation": 33,
            "sharpenAmount": 43, "sharpenRadius": 1.2, "sharpenDetail": 36, "sharpenMasking": 30,
            "noiseLuminance": 47, "noiseColor": 25,
            "gradeShadows": {"hue": 117, "sat": 52, "lum": -24}, "gradeGlobal": {"hue": 20, "sat": 61, "lum": 45},
            "gradeBlending": 36, "gradeBalance": 35, "grainAmount": 45, "vignetteAmount": 31, "vignetteStyle": "colorPriority",
            "pointCurve": [[0, 0], [146, 102], [255, 255]], "pointCurveBlue": [[0, 0], [129, 202], [255, 255]]
        }))
        .unwrap();
        let s = from_camera_raw(&cr, &DevelopSettings::default());
        assert_eq!(to_camera_raw(&s), cr);
        assert!(!beyond_camera_raw(&s));
        assert_eq!(to_camera_raw(&DevelopSettings::default()), CameraRaw::default());
        let mut more = s.clone();
        more.treatment = lightcraft_develop::Treatment::Bw;
        assert!(beyond_camera_raw(&more));
        // the rest of a base survives a subset edit
        assert_eq!(from_camera_raw(&to_camera_raw(&more), &more), more);
    }

    #[test]
    fn exposure_matches_the_library_develop_of_the_same_image() {
        // The same 8-bit sRGB pixels as a PNG in the Library and as a layer here: same working
        // pixels, same develop, same result.
        let (w, h) = (48usize, 32usize);
        let mut img = lightcraft_raster::Rgba8::new(w, h);
        for (i, p) in img.data.iter_mut().enumerate() {
            let (x, y) = (i % w, i / w);
            *p = [(x * 5) as u8, (y * 7) as u8, ((x + y) * 3) as u8, 255];
        }
        let png = lightcraft_codecs::encode_png(&lightcraft_codecs::EncodeImage::rgba8(&img), &lightcraft_codecs::EncodeMeta::default()).unwrap();
        let (src, info) = lightcraft_engine::files::load_bytes(&png, usize::MAX).unwrap();
        assert!(!info.raw);
        let mut s = identity().clone();
        s.light.exposure = 1.0;
        s.light.contrast = 20.0;
        s.color.vibrance = 30.0;
        let req = lightcraft_pipeline::RenderRequest {
            space: lightcraft_pipeline::OutputSpace::Rec2020,
            depth: lightcraft_pipeline::OutputDepth::F32Linear,
            ..lightcraft_pipeline::RenderRequest::fit(w, h)
        };
        let lib = lightcraft_pipeline::render(&src, &info, &s, &req);
        let Some(lightcraft_pipeline::DeepSamples::F32(lib)) = lib.deep.map(|d| d.samples) else { panic!("no float output") };
        let fmt = PixelFormat::new(ColorMode::Rgb, SampleType::U8, true);
        let mut surf = Surface::new(fmt);
        let data: Vec<f32> = img.data.iter().flat_map(|p| p.map(|v| f32::from(v) / 255.0)).collect();
        let area = Rect::new(0, 0, w as i32, h as i32);
        surf.write_region(area, &data);
        let (rgb, _) = surface_to_working(&surf, area, Builtin::Srgb.profile()).unwrap();
        let ours = render_working(rgb, w, h, &s).unwrap();
        let worst = lib.iter().zip(ours.as_flattened()).map(|(a, b)| (a - b).abs()).fold(0.0f32, f32::max);
        assert!(worst < 2e-3, "Library vs Camera Raw Filter differ by {worst}");
    }
}
