//! local-image: **Filter › Camera Raw Filter…** (`filter.develop`) on layer pixels, through the
//! Library's develop engine ([`lightcraft_engine::media::develop`]), so a layer develops exactly like a
//! rendered photo (a JPEG or TIFF) in the Library's Develop module.
//!
//! ```text
//! layer RGBA (document colour space, 8/16/32-bit)
//!   ──document profile → linear Rec.2020──► RgbFrame (alpha set aside)
//!   ──shared GPU Develop (automatic CPU fallback)──► linear Rec.2020
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
//! The same bridge gives the editor Develop's other photo tools on layer pixels: the lens
//! database profile ([`LensProfile`], for Filter › Lens Correction) and red eye / pet eye
//! ([`red_eye_surface`], for the Red Eye tool).
//!
//! [`to_camera_raw`] / [`from_camera_raw`] map the subset Photoshop's Camera Raw Filter descriptor
//! understands (the older [`CameraRaw`] model) to and from [`DevelopSettings`] (PSD import/export
//! and the editor's fallback dialog).

use std::sync::{Arc, OnceLock};

use li_color::cms_math;
use li_color::curve::Curve;
pub use lightcraft_develop::DevelopSettings;
use lightcraft_develop::{Hsl, VignetteStyle, WbMode, Wheel as LcWheel};
use lightcraft_geom::Point;
use photocraft_algo::camera_raw::{CameraRaw, Wheel};
use photocraft_algo::lens::LensCorrection;
use photocraft_cms::{ColorSpace, Profile, Transform, TransformOptions};
use photocraft_geom::Rect;
use photocraft_raster::{Surface, from_rgba_into};
use serde_json::Value;

/// The smart-filter / command id.
pub const COMMAND: &str = "filter.develop";

/// Params keys that are bookkeeping, not develop settings.
pub const META_KEYS: [&str; 4] = ["layer", "convert", "index", "target"];

// Settings

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
/// orientation, Super Resolution) or need raw data (camera calibration, AI Denoise / Raw Details).
///
/// The lens profile is off too: a layer carries no lens identity here. The editor's lens
/// profile is Filter › Lens Correction's Auto Correction, which resolves the same lens database
/// and correction model through [`LensProfile`] — and only on the camera's whole frame
/// ([`LensProfile::from_exif`]), the one case where a profile fits a layer.
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

// Colour

/// Linear Rec.2020 (the develop engine's working space) as an ICC profile.
pub fn linear_rec2020() -> &'static Profile {
    photocraft_cms::builtin::linear_rec2020()
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

fn f32m(m: &cms_math::Mat3) -> [[f32; 3]; 3] {
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
            let wi = cms_math::invert(&w).ok_or("singular working-space matrix")?;
            let to = cms_math::mul(&wi, &m);
            return Ok(match cms_math::invert(&to) {
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
                    let y = li_color::luminance_2020(*p);
                    *p = [trc.eval_inverse(y.max(0.0)); 3];
                }
            }
            Conv::Cms { from, .. } => from.apply(px.as_flattened_mut(), 3),
        }
    }
}

// Rendering

/// Develops linear Rec.2020 pixels (`w × h`) with `s` as a rendered (non-raw) photo, at full
/// size. Uses Library Develop's GPU/DeviceHealth path with automatic CPU fallback.
/// The result is linear Rec.2020 (0..1).
pub fn render_working(rgb: Vec<[f32; 3]>, w: usize, h: usize, s: &DevelopSettings) -> Result<Vec<[f32; 3]>, String> {
    render_working_scaled(rgb, w, h, s, 1.0)
}

fn render_working_scaled(rgb: Vec<[f32; 3]>, w: usize, h: usize, s: &DevelopSettings, pixel_scale: f32) -> Result<Vec<[f32; 3]>, String> {
    use lightcraft_pipeline::{DeepSamples, OutputDepth, OutputSpace, RenderRequest, SourceInfo};
    if w.checked_mul(h) != Some(rgb.len()) || w == 0 || h == 0 {
        return Err("develop: bad image size".into());
    }
    let src = Arc::new(li_raster::RgbFrame { width: w, height: h, data: rgb });
    let req = RenderRequest { space: OutputSpace::Rec2020, depth: OutputDepth::F32Linear, ..RenderRequest::fit(w, h) };
    let info = SourceInfo { sensor_scale: pixel_scale.recip(), ..Default::default() };
    let out = lightcraft_engine::media::develop(&src, &info, s, &req, None, true);
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
/// left alone. Identity settings preserve the input exactly, as in [`develop_surface`].
pub fn develop_rgba(px: &mut [[f32; 4]], w: usize, h: usize, s: &DevelopSettings, profile: &Profile) -> Result<(), String> {
    develop_rgba_scaled(px, w, h, s, profile, 1.0)
}

/// The same engine on a proxy: `pixel_scale` is proxy pixels per original pixel.
/// Spatial/detail stages use the pipeline's existing original-pixel scale; colour settings,
/// normalized masks and the filter frame are unchanged. A proxy remains approximate.
pub fn develop_rgba_scaled(px: &mut [[f32; 4]], w: usize, h: usize, s: &DevelopSettings, profile: &Profile, pixel_scale: f32) -> Result<(), String> {
    if w.checked_mul(h) != Some(px.len()) || w == 0 || h == 0 {
        return Err("develop: bad image size".into());
    }
    if !pixel_scale.is_finite() || !(0.0..=1.0).contains(&pixel_scale) || pixel_scale == 0.0 {
        return Err("develop: bad proxy scale".into());
    }
    if s == identity() {
        return Ok(());
    }
    let conv = Conv::new(profile)?;
    let mut rgb: Vec<[f32; 3]> = px.iter().map(|p| [p[0], p[1], p[2]]).collect();
    conv.to_working(&mut rgb);
    // Colours outside Rec.2020 (saturated ProPhoto) and above white (32-bit) develop at the gamut
    // edge; the part outside it is added back, so unchanged settings keep them.
    let clipped: Vec<[f32; 3]> = rgb.iter().map(|p| p.map(|v| v.clamp(0.0, 1.0))).collect();
    let residual: Vec<[f32; 3]> = rgb.iter().zip(&clipped).map(|(p, c)| [p[0] - c[0], p[1] - c[1], p[2] - c[2]]).collect();
    let mut out = render_working_scaled(clipped, w, h, s, pixel_scale)?;
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

/// Legacy Camera Raw descriptor rendered by the shared Develop engine, in a known RGB profile.
/// The descriptor remains a serialization model; all processing lives in the Develop pipeline.
pub fn camera_raw_surface(surf: &Surface, area: Rect, cr: &CameraRaw, profile: &Profile) -> Result<Surface, String> {
    if cr.is_identity() || area.is_empty() {
        return Ok(surf.clone());
    }
    let mut settings = from_camera_raw(cr, identity());
    sanitize(&mut settings);
    develop_surface(surf, area, &settings, profile)
}

/// Point-curve graph sampled from the pipeline's own curve tables (including parametric regions).
pub fn camera_raw_curve_lut(cr: &CameraRaw, n: usize) -> Vec<f32> {
    let s = from_camera_raw(cr, identity());
    let tables = lightcraft_pipeline::finish::curve_luts(&s.curve);
    (0..n)
        .map(|i| {
            let x = i as f32 / n.saturating_sub(1).max(1) as f32;
            tables.as_ref().map_or(x, |t| t.tables[0].eval(x))
        })
        .collect()
}

// Lens profile

/// A lens or camera of the lens database (maker and model), as Develop stores it.
pub use lightcraft_engine::lens_db::Named as LensName;

/// A layer's lens in the Library's lens database: the same lookup ([`lightcraft_engine::lens_db`])
/// and the same correction model ([`lightcraft_pipeline::lensdb`]) Develop uses as its lensfun
/// fallback, so the editor has no lens database of its own. (Embedded vendor/DNG data and
/// imported LCP profiles describe raw sensor data, which an editor layer no longer has.)
///
/// Only meaningful on the camera's whole frame: a profile is centred on the optical axis and
/// scaled to the frame diagonal, so [`LensProfile::from_exif`] refuses a cropped image.
/// Resizing is fine (the correction is resolution independent).
#[derive(Clone, Debug, PartialEq)]
pub struct LensProfile {
    /// The database camera (its crop factor), when detected.
    pub camera: Option<LensName>,
    pub lens: LensName,
    /// Focal length in millimetres and f-number of the shot.
    pub focal: f32,
    pub aperture: Option<f32>,
    /// EXIF orientation (1..8) the image is shown in, for an off-centre optical axis.
    pub orientation: u16,
}

impl LensProfile {
    /// The profile of a camera image from its EXIF, when the database knows the lens, the shot
    /// records a focal length, the image `w × h` still has the recorded frame's shape (either way
    /// round) and the profile corrects something.
    pub fn from_exif(exif: &[u8], w: u32, h: u32) -> Option<LensProfile> {
        if w == 0 || h == 0 {
            return None;
        }
        let m = lightcraft_meta::read_exif(exif);
        let focal = m.focal_length.filter(|f| f.is_finite() && *f > 0.0)? as f32;
        let lens_name = m.lens_model.as_deref().map(str::trim).filter(|s| !s.is_empty())?;
        let camera_name = format!("{} {}", m.make.as_deref().unwrap_or(""), m.model.as_deref().unwrap_or(""));
        let found = lightcraft_engine::lens_db::detect(camera_name.trim(), lens_name);
        let o = m.orientation.unwrap_or_default();
        let shape = |a: f64, b: f64| (a / b - 1.0).abs() < 0.005;
        let aspect = f64::from(w) / f64::from(h);
        let orientation = match (m.width, m.height) {
            (Some(ew), Some(eh)) if ew > 0 && eh > 0 => {
                let recorded = f64::from(ew) / f64::from(eh);
                let shown = if o.swaps_axes() { recorded.recip() } else { recorded };
                if shape(aspect, shown) {
                    o
                } else if shape(aspect, shown.recip()) {
                    // turned a quarter from how the camera meant it: the axis offset is unknown
                    lightcraft_geom::Orientation::Normal
                } else {
                    return None; // cropped: the profile no longer fits the frame
                }
            }
            _ => o,
        };
        let p = LensProfile {
            camera: found.camera,
            lens: found.lens?,
            focal,
            aperture: m.f_number.map(|v| v as f32).filter(|v| *v > 0.0),
            orientation: orientation.to_exif(),
        };
        p.correction().is_some().then_some(p)
    }

    /// The database's correction for the shot (`None`: the lens has no usable calibration).
    pub fn correction(&self) -> Option<lightcraft_pipeline::lensdb::LensCorrection> {
        lightcraft_engine::lens_db::correction(&lightcraft_engine::lens_db::Query {
            exif_camera: String::new(),
            exif_lens: String::new(),
            camera: self.camera.clone(),
            lens: Some(self.lens.clone()),
            focal: self.focal,
            aperture: self.aperture,
            distance: None,
        })
    }

    /// The lens as the UI shows it.
    pub fn label(&self) -> String {
        self.lens.label()
    }

    /// The correction laid on a `w × h` image, with each part on (1) or off (0).
    pub fn map(&self, w: f64, h: f64, distortion: bool, tca: bool, vignetting: bool) -> Option<LensProfileMap> {
        let c = self.correction()?;
        let o = lightcraft_geom::Orientation::from_exif(self.orientation);
        let (sw, sh) = if o.swaps_axes() { (h, w) } else { (w, h) };
        let mut map = c.on(sw, sh);
        (map.cx, map.cy) = o.map(map.cx, map.cy, sw, sh);
        let on = |b: bool| if b { 1.0 } else { 0.0 };
        let m = LensProfileMap { map, dist: on(distortion), tca: on(tca), vig: on(vignetting) };
        (m.moves_pixels() || m.has_vignetting()).then_some(m)
    }

    /// As stored in command / smart-filter params.
    pub fn to_json(&self) -> Value {
        let mut v = serde_json::json!({"lens": self.lens, "focal": self.focal});
        if let Some(c) = &self.camera {
            v["camera"] = serde_json::json!(c);
        }
        if let Some(a) = self.aperture {
            v["aperture"] = serde_json::json!(a);
        }
        if self.orientation != 1 {
            v["orientation"] = serde_json::json!(self.orientation);
        }
        v
    }

    /// Inverse of [`LensProfile::to_json`].
    pub fn from_json(v: &Value) -> Result<LensProfile, String> {
        let name = |k: &str| -> Option<LensName> { serde_json::from_value(v.get(k)?.clone()).ok() };
        let num = |k: &str| v.get(k).and_then(Value::as_f64).filter(|f| f.is_finite() && *f > 0.0).map(|f| f as f32);
        Ok(LensProfile {
            camera: name("camera"),
            lens: name("lens").ok_or("lens profile: needs `lens` {maker, model}")?,
            focal: num("focal").ok_or("lens profile: needs a positive `focal` (mm)")?,
            aperture: num("aperture"),
            orientation: v.get("orientation").and_then(Value::as_u64).filter(|o| (1..=8).contains(o)).unwrap_or(1) as u16,
        })
    }
}

/// A [`LensProfile`] on an image of known size: the inverse map and the vignetting gain, in the
/// image's continuous pixel coordinates (pixel centres at `+0.5`), exactly as Develop evaluates
/// them ([`lightcraft_pipeline::optics`]).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LensProfileMap {
    map: lightcraft_pipeline::lensdb::LensMap,
    dist: f64,
    tca: f64,
    vig: f64,
}

impl LensProfileMap {
    /// Whether pixels move (distortion or lateral CA).
    pub fn moves_pixels(&self) -> bool {
        (self.dist != 0.0 && !self.map.c.distortion.is_identity()) || (self.tca != 0.0 && self.map.c.per_channel())
    }

    pub fn has_vignetting(&self) -> bool {
        self.vig != 0.0 && self.map.c.vignetting.is_some_and(|k| k != [0.0; 3])
    }

    /// Source position of corrected position `(x, y)` for colour plane `ch` (0 R, 1 G, 2 B).
    pub fn to_source(&self, x: f64, y: f64, ch: usize) -> (f64, f64) {
        self.map.to_source(x, y, ch, self.dist, self.tca)
    }

    /// Linear-light vignetting gain at source position `(x, y)` (1 when vignetting is off).
    pub fn gain(&self, x: f64, y: f64) -> f64 {
        if self.vig == 0.0 { 1.0 } else { self.map.gain(x, y, self.vig) }
    }
}

/// Divides the lens profile's vignetting out of `surf` over `frame` (the image the profile was
/// laid on), in linear light like Develop (the document profile's TRC undone), pixel for pixel.
/// RGB / Gray surfaces; alpha and pixels outside `frame` keep their exact values.
pub fn lens_vignetting_surface(surf: &Surface, frame: Rect, map: &LensProfileMap, profile: &Profile) -> Result<Surface, String> {
    use rayon::prelude::*;
    let area = frame.intersect(&surf.content_bounds());
    if !map.has_vignetting() || area.is_empty() {
        return Ok(surf.clone());
    }
    let conv = Conv::new(profile)?;
    let fmt = surf.format();
    let n = fmt.channels();
    let w = area.width() as usize;
    const BAND: i32 = 64;
    let bands: Vec<Rect> = (area.y0..area.y1).step_by(BAND as usize).map(|y| Rect::new(area.x0, y, area.x1, (y + BAND).min(area.y1))).collect();
    let out: Vec<(Rect, Vec<f32>)> = bands
        .into_par_iter()
        .map(|band| {
            let mut px = vec![[0.0f32; 4]; w * band.height() as usize];
            surf.read_rgba_into(band, &mut px);
            let mut rgb: Vec<[f32; 3]> = px.iter().map(|p| [p[0], p[1], p[2]]).collect();
            conv.to_working(&mut rgb);
            for (i, p) in rgb.iter_mut().enumerate() {
                let x = (area.x0 - frame.x0) as f64 + (i % w) as f64 + 0.5;
                let y = (band.y0 - frame.y0) as f64 + (i / w) as f64 + 0.5;
                let g = map.gain(x, y) as f32;
                *p = p.map(|v| v * g);
            }
            conv.from_working(&mut rgb);
            let mut data = surf.read_region(band);
            for (o, (q, a)) in data.chunks_exact_mut(n).zip(rgb.iter().zip(&px)) {
                let alpha = fmt.alpha.then(|| o[n - 1]);
                from_rgba_into(&fmt, [q[0], q[1], q[2], a[3]], o);
                if let Some(a) = alpha {
                    o[n - 1] = a;
                }
            }
            (band, data)
        })
        .collect();
    let mut s = surf.clone();
    for (band, data) in out {
        s.write_region(band, &data);
    }
    s.prune();
    Ok(s)
}

/// Filter › Lens Correction with the lens database: `db`'s profile (Photoshop's Auto
/// Correction checkboxes in `manual` pick its parts) corrected first, then Photoshop's manual
/// terms (`manual`, whose own profile is only the generic fallback). The profile's vignetting is
/// divided out in linear light on RGB / Gray surfaces ([`lens_vignetting_surface`]); its
/// distortion and lateral CA are composed with the manual map so the image is resampled once,
/// with the manual stage's edge modes and premultiplied alpha.
pub fn lens_correction_surface(surf: &Surface, canvas: Rect, manual: &LensCorrection, db: Option<&LensProfile>, profile: &Profile) -> Result<Surface, String> {
    let Some(map) = db.and_then(|d| profile_on(d, manual, canvas)) else {
        return Ok(photocraft_algo::lens::correct(surf, canvas, manual));
    };
    let linear = matches!(surf.format().mode, photocraft_color::ColorMode::Rgb | photocraft_color::ColorMode::Grayscale);
    let src = if linear { lens_vignetting_surface(surf, canvas, &map, profile)? } else { surf.clone() };
    let lm = photocraft_algo::lens::LensMap::new(manual, canvas);
    Ok(photocraft_algo::lens::remap(&src, canvas, canvas, manual.edge, &|x, y| lm.sample(x, y).map(|s| through_profile(s, &map, canvas, !linear))))
}

/// Auto Scale for [`lens_correction_surface`]: the smallest scale at which the image corrected
/// by both stages covers the frame.
pub fn lens_auto_scale(manual: &LensCorrection, frame: Rect, db: Option<&LensProfile>) -> f64 {
    match db.and_then(|d| profile_on(d, manual, frame)) {
        Some(map) => photocraft_algo::lens::auto_scale_with(manual, frame, &|s| through_profile(s, &map, frame, false)),
        None => photocraft_algo::lens::auto_scale(manual, frame),
    }
}

fn profile_on(db: &LensProfile, lc: &LensCorrection, frame: Rect) -> Option<LensProfileMap> {
    db.map(f64::from(frame.width()), f64::from(frame.height()), lc.correct_distortion, lc.correct_ca, lc.correct_vignette)
}

/// A manual-stage sample continued through the profile's inverse map (output → manual terms →
/// profile-corrected image → camera image), optionally with the profile's vignetting as a gain.
fn through_profile(s: photocraft_algo::lens::Sample, m: &LensProfileMap, frame: Rect, gain: bool) -> photocraft_algo::lens::Sample {
    let (ox, oy) = (f64::from(frame.x0), f64::from(frame.y0));
    let at = |p: [f64; 2], ch: usize| {
        let (x, y) = m.to_source(p[0] - ox, p[1] - oy, ch);
        [x + ox, y + oy]
    };
    let p = at(s.p, 1);
    let g = if gain { m.gain(p[0] - ox, p[1] - oy) as f32 } else { 1.0 };
    photocraft_algo::lens::Sample { p, pr: at(s.pr, 0), pb: at(s.pb, 2), gain: s.gain * g }
}

// Red eye

/// One eye for [`red_eye_surface`]: where to look (an ellipse, document pixels) and how to fix it
/// (Develop's red eye / pet eye controls).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EyeFix {
    pub center: [f64; 2],
    pub radii: [f64; 2],
    /// 1..100 (50 = the detected pupil).
    pub pupil_size: f64,
    /// 0..100.
    pub darken: f64,
    /// Pet eye: find the tapetum's glow (any colour) instead of red.
    pub pet: bool,
    /// Pet eye: add a catchlight (up-left, as Develop places it).
    pub catchlight: bool,
}

/// [`lightcraft_pipeline::redeye::confidence`] below which a click found no pupil.
const MIN_PUPIL_CONFIDENCE: f32 = 0.35;

/// Develop's red eye / pet eye correction ([`lightcraft_pipeline::redeye`]: the pupil found as a
/// weighted centroid on a resolution-independent grid, then neutralized and darkened in linear
/// light) on a layer, in the document's RGB `profile`. `bounds` limits the search (the canvas).
/// A first detection over the whole ellipse is refined once on an eye-sized ellipse around the
/// pupil, as if the user had drawn it there, so a generous click radius doesn't bias the size.
/// `None` when no pupil is found. Only pixels the correction changes are rewritten; alpha keeps
/// its exact values.
pub fn red_eye_surface(surf: &Surface, bounds: Rect, eye: &EyeFix, profile: &Profile) -> Result<Option<Surface>, String> {
    use lightcraft_pipeline::redeye;
    let reach = eye.radii[0].max(eye.radii[1]).max(1.0) * 1.6 + 2.0;
    let r = |v: f64| v.round() as i32;
    let area = Rect::new(r(eye.center[0] - reach), r(eye.center[1] - reach), r(eye.center[0] + reach), r(eye.center[1] + reach)).intersect(&bounds);
    if area.is_empty() {
        return Ok(None);
    }
    let (w, h) = (area.width() as usize, area.height() as usize);
    let (wf, hf) = (w as f64, h as f64);
    let long = wf.max(hf);
    let mut px = vec![[0.0f32; 4]; w * h];
    surf.read_rgba_into(area, &mut px);
    let mut rgb: Vec<[f32; 3]> = px.iter().map(|p| [p[0], p[1], p[2]]).collect();
    let conv = Conv::new(profile)?;
    {
        use rayon::prelude::*;
        rgb.par_chunks_mut(8192).for_each(|c| conv.to_working(c));
    }
    // Transparent pixels are no pupil.
    let seen = li_raster::RgbFrame {
        width: w,
        height: h,
        data: rgb.iter().zip(&px).map(|(c, p)| if p[3] < 0.5 { [0.0; 3] } else { c.map(|v| v.max(0.0)) }).collect(),
    };
    let mut want = lightcraft_develop::RedEye {
        center: Point::new((eye.center[0] - f64::from(area.x0)) / wf, (eye.center[1] - f64::from(area.y0)) / hf),
        rx: eye.radii[0].max(1.0) / long,
        ry: eye.radii[1].max(1.0) / long,
        pupil_size: eye.pupil_size.clamp(0.0, 100.0),
        darken: eye.darken.clamp(0.0, 100.0),
        pet: eye.pet,
        catchlight: (eye.pet && eye.catchlight).then_some(Point::new(-0.35, -0.35)),
    };
    let o = lightcraft_geom::Orientation::Normal;
    let mut found = redeye::detect(&seen, o, wf, hf, &want);
    if !found.found {
        return Ok(None);
    }
    let fit = (found.radius * 2.5).max(2.0 / long);
    if fit < want.rx.max(want.ry) {
        want.center = found.center;
        (want.rx, want.ry) = (fit.min(want.rx), fit.min(want.ry));
        found = redeye::detect(&seen, o, wf, hf, &want);
    }
    // Develop always corrects inside the ellipse the user drew; a click must land on a pupil.
    if redeye::confidence(&seen, o, wf, hf, &want, &found) < MIN_PUPIL_CONFIDENCE {
        return Ok(None);
    }
    let frame = lightcraft_pipeline::geometry::Frame::new(w, h, identity(), false);
    let eyes = redeye::resolve(&seen, &[want], o, &frame, w, h, frame.px_per_long(w));
    // Only pixels near an eye can change (the disc's soft rim reaches 1.35 radii).
    let reach = |e: &redeye::EyeK| {
        let r = e.r * 1.35 + 1.0;
        let (lo, hi) = ((e.cx - r, e.cy - r), (e.cx + r, e.cy + r));
        match e.catchlight {
            Some((lx, ly, lr)) => ((lo.0.min(lx - lr - 1.0), lo.1.min(ly - lr - 1.0)), (hi.0.max(lx + lr + 1.0), hi.1.max(ly + lr + 1.0))),
            None => (lo, hi),
        }
    };
    let ((bx0, by0), (bx1, by1)) = reach(&eyes[0]);
    let clampi = |v: f32, n: usize| (v.max(0.0) as usize).min(n);
    let (bx0, by0, bx1, by1) = (clampi(bx0.floor(), w), clampi(by0.floor(), h), clampi(bx1.ceil(), w), clampi(by1.ceil(), h));
    let (changed, mut fixed): (Vec<usize>, Vec<[f32; 3]>) = (by0..by1)
        .flat_map(|y| (bx0..bx1).map(move |x| y * w + x))
        .filter(|&i| px[i][3] > 0.0)
        .map(|i| (i, redeye::eye_pixel(rgb[i], (i % w) as f32 + 0.5, (i / w) as f32 + 0.5, &eyes)))
        .filter(|(i, c)| *c != rgb[*i])
        .unzip();
    if changed.is_empty() {
        return Ok(None);
    }
    conv.from_working(&mut fixed);
    let fmt = surf.format();
    let n = fmt.channels();
    let mut data = surf.read_region(area);
    for (&i, q) in changed.iter().zip(&fixed) {
        let o = &mut data[i * n..(i + 1) * n];
        let alpha = fmt.alpha.then(|| o[n - 1]);
        from_rgba_into(&fmt, [q[0], q[1], q[2], px[i][3]], o);
        if let Some(a) = alpha {
            o[n - 1] = a;
        }
    }
    let mut out = surf.clone();
    out.write_region(area, &data);
    out.prune();
    Ok(Some(out))
}

// Camera Raw subset

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

/// Apply only edited descriptor controls, preserving the precision and range of untouched
/// full Develop settings (including custom WB and tools outside the PSD descriptor).
pub fn edit_camera_raw(cr: &CameraRaw, base: &DevelopSettings) -> DevelopSettings {
    let original = to_camera_raw(base);
    let mut s = from_camera_raw(cr, base);
    macro_rules! keep {
        ($section:ident.$field:ident, $control:ident) => {
            if cr.$control == original.$control {
                s.$section.$field.clone_from(&base.$section.$field);
            }
        };
    }
    if cr.temperature == original.temperature && cr.tint == original.tint {
        s.wb = base.wb;
    } else {
        if cr.temperature == original.temperature {
            s.wb.temp = base.wb.temp;
        }
        if cr.tint == original.tint {
            s.wb.tint = base.wb.tint;
        }
    }
    keep!(light.exposure, exposure);
    keep!(light.contrast, contrast);
    keep!(light.highlights, highlights);
    keep!(light.shadows, shadows);
    keep!(light.whites, whites);
    keep!(light.blacks, blacks);
    keep!(effects.texture, texture);
    keep!(effects.clarity, clarity);
    keep!(effects.dehaze, dehaze);
    keep!(color.vibrance, vibrance);
    keep!(color.saturation, saturation);
    keep!(curve.highlights, curve_highlights);
    keep!(curve.lights, curve_lights);
    keep!(curve.darks, curve_darks);
    keep!(curve.shadows, curve_shadows);
    keep!(curve.master, point_curve);
    keep!(curve.red, point_curve_red);
    keep!(curve.green, point_curve_green);
    keep!(curve.blue, point_curve_blue);
    if cr.curve_splits[0] == original.curve_splits[0] {
        s.curve.split_shadows = base.curve.split_shadows;
    }
    if cr.curve_splits[1] == original.curve_splits[1] {
        s.curve.split_mid = base.curve.split_mid;
    }
    if cr.curve_splits[2] == original.curve_splits[2] {
        s.curve.split_highlights = base.curve.split_highlights;
    }
    for (i, (h, old)) in bands_mut(&mut s.mixer).into_iter().zip(bands(&base.mixer)).enumerate() {
        if cr.hsl_hue[i] == original.hsl_hue[i] {
            h.hue = old.hue;
        }
        if cr.hsl_sat[i] == original.hsl_sat[i] {
            h.sat = old.sat;
        }
        if cr.hsl_lum[i] == original.hsl_lum[i] {
            h.lum = old.lum;
        }
    }
    if cr.grade_shadows.hue == original.grade_shadows.hue {
        s.grading.shadows.hue = base.grading.shadows.hue;
    }
    if cr.grade_shadows.sat == original.grade_shadows.sat {
        s.grading.shadows.sat = base.grading.shadows.sat;
    }
    if cr.grade_shadows.lum == original.grade_shadows.lum {
        s.grading.shadows.lum = base.grading.shadows.lum;
    }
    if cr.grade_midtones.hue == original.grade_midtones.hue {
        s.grading.midtones.hue = base.grading.midtones.hue;
    }
    if cr.grade_midtones.sat == original.grade_midtones.sat {
        s.grading.midtones.sat = base.grading.midtones.sat;
    }
    if cr.grade_midtones.lum == original.grade_midtones.lum {
        s.grading.midtones.lum = base.grading.midtones.lum;
    }
    if cr.grade_highlights.hue == original.grade_highlights.hue {
        s.grading.highlights.hue = base.grading.highlights.hue;
    }
    if cr.grade_highlights.sat == original.grade_highlights.sat {
        s.grading.highlights.sat = base.grading.highlights.sat;
    }
    if cr.grade_highlights.lum == original.grade_highlights.lum {
        s.grading.highlights.lum = base.grading.highlights.lum;
    }
    if cr.grade_global.hue == original.grade_global.hue {
        s.grading.global.hue = base.grading.global.hue;
    }
    if cr.grade_global.sat == original.grade_global.sat {
        s.grading.global.sat = base.grading.global.sat;
    }
    if cr.grade_global.lum == original.grade_global.lum {
        s.grading.global.lum = base.grading.global.lum;
    }
    keep!(grading.blending, grade_blending);
    keep!(grading.balance, grade_balance);
    keep!(detail.sharpen_amount, sharpen_amount);
    keep!(detail.sharpen_radius, sharpen_radius);
    keep!(detail.sharpen_detail, sharpen_detail);
    keep!(detail.sharpen_masking, sharpen_masking);
    keep!(detail.nr_luminance, noise_luminance);
    keep!(detail.nr_detail, noise_luminance_detail);
    keep!(detail.nr_color, noise_color);
    keep!(detail.nr_color_detail, noise_color_detail);
    keep!(grain.amount, grain_amount);
    keep!(grain.size, grain_size);
    keep!(grain.roughness, grain_roughness);
    keep!(grain.seed, seed);
    keep!(vignette.amount, vignette_amount);
    keep!(vignette.midpoint, vignette_midpoint);
    keep!(vignette.roundness, vignette_roundness);
    keep!(vignette.feather, vignette_feather);
    keep!(vignette.highlights, vignette_highlights);
    keep!(vignette.style, vignette_style);
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
    use photocraft_cms::Builtin;
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
                for (p, q) in a.as_chunks::<4>().0.iter().zip(b.as_chunks::<4>().0.iter()) {
                    assert_eq!(p[3].to_bits(), q[3].to_bits(), "{name} {sample:?}: alpha changed");
                    // linear data is compared as it displays (sRGB-encoded)
                    let enc = |v: f32| if name.starts_with("linear") { li_color::curve::srgb_trc().eval_inverse(v.clamp(0.0, 1.0)) } else { v };
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
        for (p, q) in a.as_chunks::<4>().0.iter().zip(b.as_chunks::<4>().0.iter()) {
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
        let mut img = li_raster::Rgba8::new(w, h);
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

#[cfg(test)]
mod settings_tests {
    use super::*;

    #[test]
    fn edits_preserve_unrepresented_and_precise_settings() {
        let mut base = identity().clone();
        base.wb.mode = WbMode::Custom;
        base.wb.temp = 30000.123456789;
        base.wb.tint = 145.123456789;
        base.curve.master = vec![Point::new(0.0, 0.0), Point::new(0.345678912345, 0.456789123456), Point::new(1.0, 1.0)];
        base.treatment = lightcraft_develop::Treatment::Bw;
        base.detail.nr_contrast = 42.123456789;
        base.curve.split_highlights = 80.123456789;
        let mut controls = to_camera_raw(&base);
        assert_eq!(edit_camera_raw(&controls, &base), base);
        controls.exposure = 0.75;
        let mut expected = base.clone();
        expected.light.exposure = 0.75;
        assert_eq!(edit_camera_raw(&controls, &base), expected);
        controls.grade_shadows.sat = 25.0;
        expected.grading.shadows.sat = 25.0;
        assert_eq!(edit_camera_raw(&controls, &base), expected);
        controls.curve_splits[0] = 30.0;
        expected.curve.split_shadows = 30.0;
        assert_eq!(edit_camera_raw(&controls, &base), expected);
    }

    #[test]
    fn proxy_scale_matches_pipeline_and_rejects_bad_sizes() {
        use lightcraft_pipeline::{DeepSamples, OutputDepth, OutputSpace, RenderRequest, SourceInfo};
        let (w, h) = (32, 24);
        let px: Vec<_> = (0..w * h).map(|i| [0.2 + (i % 7) as f32 * 0.08, 0.3, 0.6, 0.7]).collect();
        let mut settings = identity().clone();
        settings.detail.sharpen_amount = 60.0;
        settings.detail.sharpen_radius = 2.0;
        settings.detail.nr_luminance = 35.0;
        let profile = linear_rec2020();
        let mut preview = px.clone();
        develop_rgba_scaled(&mut preview, w, h, &settings, profile, 0.25).unwrap();
        let source = li_raster::RgbFrame { width: w, height: h, data: px.iter().map(|p| [p[0], p[1], p[2]]).collect() };
        let request = RenderRequest { space: OutputSpace::Rec2020, depth: OutputDepth::F32Linear, ..RenderRequest::fit(w, h) };
        let reference = lightcraft_pipeline::render(&source, &SourceInfo { sensor_scale: 4.0, ..Default::default() }, &settings, &request);
        let DeepSamples::F32(reference) = reference.deep.unwrap().samples else { panic!() };
        for (p, q) in preview.iter().zip(reference.as_chunks::<3>().0) {
            for c in 0..3 {
                assert!((p[c] - q[c]).abs() < 2e-6);
            }
            assert_eq!(p[3], 0.7);
        }
        for scale in [0.0, -1.0, 2.0, f32::NAN] {
            assert!(develop_rgba_scaled(&mut preview, w, h, &settings, profile, scale).is_err());
        }
        assert!(develop_rgba_scaled(&mut preview, w + 1, h, identity(), profile, 1.0).is_err());
        let mut unchanged = px.clone();
        develop_rgba(&mut unchanged, w, h, identity(), profile).unwrap();
        assert_eq!(unchanged, px);
    }

    #[test]
    fn legacy_curves_render_with_shared_interpolation() {
        let cr = CameraRaw { point_curve: vec![[255.0, 255.0], [128.0, 200.0], [128.0, 150.0], [0.0, 0.0]], exposure: 0.25, ..Default::default() };
        assert!(cr.validate().is_err());
        let lut = camera_raw_curve_lut(&cr, 256);
        assert!(lut.iter().all(|v| v.is_finite()));
        assert!(lut.windows(2).all(|p| p[0] <= p[1]));
        let mut source = Surface::new(photocraft_color::PixelFormat::RGBA32F);
        source.write_region(Rect::new(0, 0, 4, 4), &[0.25, 0.4, 0.6, 0.7].repeat(16));
        let result = camera_raw_surface(&source, Rect::new(0, 0, 4, 4), &cr, photocraft_cms::Builtin::Srgb.profile()).unwrap();
        assert!(result.read_region(Rect::new(0, 0, 4, 4)).iter().all(|v| v.is_finite()));
        assert_eq!(result.rgba(1, 1)[3], source.rgba(1, 1)[3]);
    }
}

#[cfg(test)]
#[path = "develop_filter_tests.rs"]
mod retouch_tests;
