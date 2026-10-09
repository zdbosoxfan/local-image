//! Scene-linear preparation: white balance + exposure, and the spatial planes the per-pixel stage
//! needs (edge-aware base layer for highlights/shadows, clarity/texture bands, dehaze veil).

use lightcraft_color::cct::{temp_tint_to_xy, wb_matrix};
use lightcraft_color::{REC2020, luminance_2020};
use lightcraft_develop::DevelopSettings;
use lightcraft_raster::blur::gaussian;
use std::sync::Arc;

use lightcraft_raster::{Plane, Rgb32f, par_join};

use crate::geometry::Frame;
use crate::{Prepared, Quality, SourceInfo, for_rows, masks, timed};

/// White balance (relative to the source's as-shot white), the film negative conversion and
/// exposure, in place.
pub fn scene_linear_pre(img: &mut Rgb32f, info: &SourceInfo, s: &DevelopSettings) {
    let gain = 2f32.powf(s.light.exposure as f32);
    match crate::negative::params(s) {
        Some(k) => {
            wb_gain(img, info, s, 1.0);
            crate::negative::apply_params(img, &k);
            img.map_in_place(|p| p.map(|v| v * gain));
        }
        None => wb_gain(img, info, s, gain),
    }
}

/// White balance only (exposure is applied by the per-pixel stage, see [`crate::Prepared`]).
pub fn white_balance(img: &mut Rgb32f, info: &SourceInfo, s: &DevelopSettings) {
    wb_gain(img, info, s, 1.0);
}

/// The white-balance matrix (linear Rec.2020, luminance-preserving) for the settings, or `None`
/// when the as-shot white is kept.
pub fn wb_matrix_for(info: &SourceInfo, s: &DevelopSettings) -> Option<[[f32; 3]; 3]> {
    if let Some(cc) = crate::colorcal::of(info, s) {
        return Some(colorcal_matrix(info, &cc));
    }
    let (t, tint) = effective_wb(info, s);
    if (t - info.as_shot_temp).abs() < 1e-6 && (tint - info.as_shot_tint).abs() < 1e-6 {
        return None;
    }
    let m = match info.camera_color {
        Some(camera) => camera.change(temp_tint_to_xy(t, tint)),
        None => {
            let set = wb_matrix(&REC2020, temp_tint_to_xy(t, tint));
            let shot = wb_matrix(&REC2020, temp_tint_to_xy(info.as_shot_temp, info.as_shot_tint));
            set.mul(&shot.inverse().unwrap_or(lightcraft_color::Mat3::IDENTITY))
        }
    };
    // Normalize so neutral luminance is preserved (WB shouldn't change exposure).
    let g = m.apply([1.0, 1.0, 1.0]);
    let y = g[0] * 0.2627 + g[1] * 0.6780 + g[2] * 0.0593;
    Some(m.mul(&lightcraft_color::Mat3::diag(1.0 / y, 1.0 / y, 1.0 / y)).to_f32())
}

/// With colour calibration: undo the as-shot white balance, then (when it is linear) the
/// calibration's adaptation; the per-pixel part of a non-linear calibration follows in
/// [`white_balance`]. Neutral luminance is kept.
fn colorcal_matrix(info: &SourceInfo, cc: &crate::colorcal::ColorCal) -> [[f32; 3]; 3] {
    let shot = wb_matrix(&REC2020, temp_tint_to_xy(info.as_shot_temp, info.as_shot_tint));
    let undo = shot.inverse().unwrap_or(lightcraft_color::Mat3::IDENTITY);
    let m = if cc.is_linear() { lightcraft_color::Mat3(cc.matrix.map(|r| r.map(f64::from))).mul(&undo) } else { undo };
    let g = m.apply([1.0, 1.0, 1.0]);
    let y = g[0] * 0.2627 + g[1] * 0.6780 + g[2] * 0.0593;
    m.mul(&lightcraft_color::Mat3::diag(1.0 / y, 1.0 / y, 1.0 / y)).to_f32()
}

/// The white-balance change from white `from` to white `to` (temperature K, tint), as
/// [`wb_matrix_for`] builds it (linear Rec.2020, neutral luminance kept); `None` when they match.
pub fn wb_change(from: (f64, f64), to: (f64, f64)) -> Option<[[f32; 3]; 3]> {
    if (from.0 - to.0).abs() < 1e-6 && (from.1 - to.1).abs() < 1e-6 {
        return None;
    }
    let set = wb_matrix(&REC2020, temp_tint_to_xy(to.0, to.1));
    let shot = wb_matrix(&REC2020, temp_tint_to_xy(from.0, from.1));
    let m = set.mul(&shot.inverse().unwrap_or(lightcraft_color::Mat3::IDENTITY));
    let g = m.apply([1.0, 1.0, 1.0]);
    let y = g[0] * 0.2627 + g[1] * 0.6780 + g[2] * 0.0593;
    Some(m.mul(&lightcraft_color::Mat3::diag(1.0 / y, 1.0 / y, 1.0 / y)).to_f32())
}

fn wb_gain(img: &mut Rgb32f, info: &SourceInfo, s: &DevelopSettings, gain: f32) {
    let m = wb_matrix_for(info, s);
    // a non-linear colour calibration: its per-pixel part after the matrix
    let cc = crate::colorcal::of(info, s).filter(|c| !c.is_linear());
    let tables = info.camera_profile.as_ref().and_then(|profile| {
        let (t, tint) = effective_wb(info, s);
        let g = info.camera_color.map_or(1.0, |camera| lightcraft_color::camera::illuminant_weight(&camera.color, temp_tint_to_xy(t, tint)));
        lightcraft_color::profile::ProfileTables::new(profile, g)
    });
    let baseline = info.baseline_gain.max(1e-9);
    let w = img.width;
    for_rows(&mut img.data, w, |_, row| {
        for p in row.iter_mut() {
            let mut c = *p;
            if let Some(m) = &m {
                c = [
                    m[0][0] * c[0] + m[0][1] * c[1] + m[0][2] * c[2],
                    m[1][0] * c[0] + m[1][1] * c[1] + m[1][2] * c[2],
                    m[2][0] * c[0] + m[2][1] * c[1] + m[2][2] * c[2],
                ];
            }
            if let Some(tables) = &tables {
                c = tables.apply(c.map(|v| v / baseline), baseline);
            }
            if let Some(cc) = &cc {
                c = cc.apply(c);
            }
            *p = c.map(|v| v * gain);
        }
    });
}

/// The white balance actually in effect (presets resolve to their Kelvin values for raw files).
pub fn effective_wb(info: &SourceInfo, s: &DevelopSettings) -> (f64, f64) {
    use lightcraft_develop::WbMode;
    match s.wb.mode {
        WbMode::AsShot => (info.as_shot_temp, info.as_shot_tint),
        m if info.raw && !info.relative_wb => m.preset().unwrap_or((s.wb.temp, s.wb.tint)),
        _ => (s.wb.temp, s.wb.tint),
    }
}

/// Self-guided filter (He et al.) on a single plane with Gaussian windows of `sigma` px.
pub fn guided(p: &Plane, sigma: f32, eps: f32) -> Plane {
    let (ma, mb) = guided_coeffs(p, sigma, eps);
    guided_apply(p, &ma, &mb)
}

/// Blurred guided-filter coefficients (mean a, mean b) of `p`; every pass row-parallel, and the
/// two independent blurs of each step run side by side.
fn guided_coeffs(p: &Plane, sigma: f32, eps: f32) -> (Plane, Plane) {
    let (mean, corr) = par_join(|| gaussian(p, sigma), || gaussian(&p.map(|v| v * v), sigma));
    let a = corr.zip_map(&mean, |c, m| {
        let var = (c - m * m).max(0.0);
        var / (var + eps)
    });
    let b = mean.zip_map(&a, |m, a| m - a * m);
    par_join(|| gaussian(&a, sigma), || gaussian(&b, sigma))
}

/// `q = a·p + b` per pixel.
fn guided_apply(p: &Plane, a: &Plane, b: &Plane) -> Plane {
    let mut q = Plane::new(p.width, p.height);
    let w = p.width;
    for_rows(&mut q.data, w, |y, row| {
        for ((v, pv), (av, bv)) in row.iter_mut().zip(p.row(y)).zip(a.row(y).iter().zip(b.row(y))) {
            *v = av * pv + bv;
        }
    });
    q
}

/// Fast guided filter (He & Sun 2015): coefficients computed on a `s×` subsampled plane and
/// bilinearly upsampled; the output keeps full-resolution edges. Equivalent to [`guided`] for
/// large windows at a fraction of the cost.
pub fn guided_fast(p: &Plane, sigma: f32, eps: f32) -> Plane {
    use lightcraft_raster::resample::{Filter, resize};
    let s = guided_fast_step(sigma);
    if s <= 1 {
        return guided(p, sigma, eps);
    }
    let (lw, lh) = (p.width.div_ceil(s).max(1), p.height.div_ceil(s).max(1));
    let lo = resize(p, lw, lh, Filter::Box);
    let (ma, mb) = guided_coeffs(&lo, sigma / s as f32, eps);
    let up = |c: &Plane| resize(c, p.width, p.height, Filter::Bilinear);
    guided_apply(p, &up(&ma), &up(&mb))
}

/// Subsampling step of [`guided_fast`] for `sigma` (1 = no subsampling: the plain guided filter).
pub fn guided_fast_step(sigma: f32) -> usize {
    (sigma / 3.0).floor().clamp(1.0, 16.0) as usize
}

/// Wavelet noise reduction at the output's original-pixel scale.
pub fn denoise(img: &mut Rgb32f, s: &DevelopSettings, src_long: usize, px_per_long: f64, sensor_scale: f32) {
    let scale = crate::detail::out_per_orig(px_per_long, src_long, sensor_scale);
    if let Some(p) = crate::detail::nr_params(s, scale) {
        crate::timed("wavelet NR", || crate::detail::denoise(img, &p));
    }
}

pub fn log_lum(c: [f32; 3]) -> f32 {
    (luminance_2020(c).max(1e-7) / crate::tone::GREY).log2()
}

/// Spatial planes of one (pre-exposure) image, each tagged with the radius it was computed at.
/// `key` identifies the image; see [`crate::StageCache`].
#[derive(Clone, Default)]
pub(crate) struct Planes {
    pub key: u64,
    pub log_l: Option<Arc<Plane>>,
    pub base: Option<(u32, Arc<Plane>)>,
    pub clarity: Option<(u32, Arc<Plane>)>,
    pub texture: Option<(u32, Arc<Plane>)>,
    ///  log luminance blurred once and twice by the sharpening radius.
    pub sharp: Option<(u32, Arc<Plane>, Arc<Plane>)>,
    ///  the refined dark channel and its coloured airlight (see [`crate::detail`]).
    pub haze: Option<Arc<crate::detail::Haze>>,
    /// Blurred chromaticity (local Moiré / Noise).
    pub chroma: Option<(u32, Arc<Rgb32f>)>,
    /// Develop layers' noise reduction: the image denoised again with a layer's settings, by
    /// their key (see [`crate::layers::nr_images`]).
    pub layer_nr: Vec<(u64, Arc<Rgb32f>)>,
    pub layer_sharp: Vec<(u32, Arc<Plane>, Arc<Plane>)>,
    /// The tone equalizer's mask (pre-exposure zones, see [`crate::toneeq::mask_plane`]) by the
    /// key of its radius and epsilon.
    pub tone_eq: Option<(u64, Arc<Plane>)>,
}

/// Reuse `slot` if it was computed at `sigma`, else compute and store it.
fn plane_at(slot: &mut Option<(u32, Arc<Plane>)>, sigma: f32, f: impl FnOnce() -> Plane) -> Arc<Plane> {
    match slot {
        Some((k, p)) if *k == sigma.to_bits() => p.clone(),
        _ => {
            let p = Arc::new(f());
            *slot = Some((sigma.to_bits(), p.clone()));
            p
        }
    }
}

/// Guided-filter epsilons (EV²) of the highlights/shadows base and the clarity band.
pub const BASE_EPS: f32 = 0.35;
pub const CLARITY_EPS: f32 = 0.8;

/// Radius of the local Moiré / colour-noise chromaticity blur, as a fraction of the long edge.
pub const CHROMA_SIGMA: f32 = 0.004;

/// Radii (px) of the spatial planes the settings need (`None` = not needed).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PlaneSigmas {
    /// Edge-aware base for highlights/shadows (fast guided filter, [`BASE_EPS`]).
    pub base: Option<f32>,
    /// Clarity band (fast guided filter, [`CLARITY_EPS`]).
    pub clarity: Option<f32>,
    /// Texture / sharpening band (Gaussian).
    pub texture: Option<f32>,
    /// Chromaticity blur for local Moiré / colour noise (Gaussian).
    pub chroma: Option<f32>,
    ///  sharpening blur (true Gaussian, output px).
    pub sharp: Option<f32>,
    ///  the dehaze plane (dark channel prior) is needed.
    pub haze: bool,
}

/// The planes `s` needs at `px_per_long`; `out_per_orig` (see [`crate::detail::out_per_orig`])
/// sizes the sharpening radius.
pub fn plane_sigmas(s: &DevelopSettings, px_per_long: f64, out_per_orig: f32, q: Quality) -> PlaneSigmas {
    let ppl = px_per_long as f32;
    let local_any = |f: fn(&lightcraft_develop::LocalAdjustments) -> f64| s.masks.iter().any(|m| f(&m.adjust) != 0.0);
    let tone_active =
        s.light.highlights != 0.0 || s.light.shadows != 0.0 || s.masks.iter().any(|m| m.adjust.highlights != 0.0 || m.adjust.shadows != 0.0);
    // Edge-aware base at ~1.5% of the long edge (EV² epsilon: edges of > ~0.6 EV are preserved).
    let (l_base, l_clarity, l_texture, l_dark, l_sharp) = crate::layers::planes_needed(s);
    let tone_active = tone_active || l_base;
    let base = tone_active.then(|| {
        let sigma = (0.015 * ppl).max(1.0);
        if q == Quality::Draft { sigma.min(24.0) } else { sigma }
    });
    let clarity = (s.effects.clarity != 0.0 || local_any(|a| a.clarity) || l_clarity).then(|| (0.012 * ppl).max(1.0));
    // local Noise and Defringe read the fine detail band too
    // (sharpening has planes of its own)
    let texture = (s.effects.texture != 0.0
        || local_any(|a| a.texture)
        || local_any(|a| a.sharpness)
        || local_any(|a| a.noise)
        || s.masks.iter().any(|m| m.adjust.defringe > 0.0)
        || l_texture)
        .then(|| (0.0018 * ppl).max(0.6));
    let dehaze = s.effects.dehaze != 0.0 || local_any(|a| a.dehaze) || l_dark;
    let chroma = (local_any(|a| a.moire) || s.masks.iter().any(|m| m.adjust.noise > 0.0)).then(|| (CHROMA_SIGMA * ppl).max(1.0));
    let sharp = (s.section_enabled("detail") && s.detail.sharpen_amount != 0.0 || local_any(|a| a.sharpness) || l_sharp)
        .then(|| crate::detail::sharp_sigma(s, out_per_orig))
        .filter(|sg| *sg >= crate::detail::SHARP_MIN_SIGMA);
    PlaneSigmas { base, clarity, texture, chroma, sharp, haze: dehaze }
}

/// The spatial planes the per-pixel stage needs for `img` (white-balanced, before exposure),
/// reusing whatever `planes` already holds for it. Missing planes are computed side by side (each
/// one alone scales poorly: the guided filters work on small subsampled grids).
#[allow(clippy::too_many_arguments)]
pub(crate) fn prepare(
    img: Arc<Rgb32f>,
    s: &DevelopSettings,
    info: &SourceInfo,
    frame: &Frame,
    px_per_long: f64,
    src_long: usize,
    q: Quality,
    planes: &mut Planes,
) -> Prepared {
    let log_l = planes.log_l.get_or_insert_with(|| Arc::new(timed("log_l", || img.map(log_lum)))).clone();
    let opo = crate::detail::out_per_orig(px_per_long, src_long, info.sensor_scale);
    let PlaneSigmas { base: base_sigma, clarity: clarity_sigma, texture: texture_sigma, chroma: chroma_sigma, sharp: sharp_sigma, haze: haze_on } =
        plane_sigmas(s, px_per_long, opo, q);

    let Planes { base: sb, clarity: sc, texture: st, chroma: sch, sharp: ssh, haze: shz, .. } = planes;
    // Detail: the sharpening blurs and the dehaze plane
    let sharp = sharp_sigma.map(|sg| match ssh {
        Some((k, b1, b2)) if *k == sg.to_bits() => (b1.clone(), b2.clone(), sg),
        _ => {
            let (b1, b2) = timed("sharpen", || crate::detail::sharp_planes(&log_l, sg));
            let (b1, b2) = (Arc::new(b1), Arc::new(b2));
            *ssh = Some((sg.to_bits(), b1.clone(), b2.clone()));
            (b1, b2, sg)
        }
    });
    let haze = haze_on.then(|| shz.get_or_insert_with(|| Arc::new(timed("dehaze", || crate::detail::haze_plane(&img, opo)))).clone());
    let chroma_blur = chroma_sigma.map(|sg| match sch {
        Some((k, c)) if *k == sg.to_bits() => c.clone(),
        _ => {
            let c = timed("chroma", || Arc::new(gaussian(&img.map(masks::chromaticity), sg)));
            *sch = Some((sg.to_bits(), c.clone()));
            c
        }
    });
    let l = &log_l;
    let (base, (clarity_blur, texture_blur)) = par_join(
        || base_sigma.map(|sg| plane_at(sb, sg, || timed("base", || guided_fast(l, sg, BASE_EPS)))),
        || {
            par_join(
                || clarity_sigma.map(|sg| plane_at(sc, sg, || timed("clarity", || guided_fast(l, sg, CLARITY_EPS)))),
                || texture_sigma.map(|sg| plane_at(st, sg, || timed("texture", || gaussian(l, sg)))),
            )
        },
    );
    let base = base.unwrap_or_else(|| log_l.clone());
    let ev = s.light.exposure as f32;
    let masks = timed("masks", || {
        masks::evaluate_with_tone(
            &s.masks,
            frame,
            img.width,
            img.height,
            &img,
            &log_l,
            ev,
            &crate::tone2::tone_map(s, info, s.light.contrast, s.light.whites, s.light.blacks),
        )
    });
    let mut layer_sharp = Vec::new();
    let mut wanted = Vec::new();
    for (mask, m) in s.masks.iter().filter(|m| m.visible && !m.components.is_empty()).enumerate() {
        if let Some(d) = m.tools.detail.filter(|d| d.sharpen_amount != 0.0) {
            let sg = d.sharpen_radius.clamp(0.5, 3.0) as f32 * opo;
            if sg < crate::detail::SHARP_MIN_SIGMA {
                continue;
            }
            let key = sg.to_bits();
            wanted.push(key);
            let pair = match planes.layer_sharp.iter().find(|p| p.0 == key) {
                Some((_, a, b)) => (a.clone(), b.clone()),
                None => {
                    let (a, b) = crate::detail::sharp_planes(&log_l, sg);
                    let pair = (Arc::new(a), Arc::new(b));
                    planes.layer_sharp.push((key, pair.0.clone(), pair.1.clone()));
                    pair
                }
            };
            layer_sharp.push((mask, pair.0, pair.1, sg));
        }
    }
    planes.layer_sharp.retain(|p| wanted.contains(&p.0));
    let layer_nr = timed("layer nr", || crate::layers::nr_images(&img, s, info, src_long, px_per_long, &mut planes.layer_nr));
    let tone_eq = crate::toneeq::mask_wanted(s).then(|| {
        let (sg, eps) = crate::toneeq::mask_sigma_eps(s, px_per_long);
        let k = crate::hash_of((
            sg.to_bits(),
            eps.to_bits(),
            s.light.exposure.to_bits(),
            s.tone_eq.mask_exposure.to_bits(),
            s.tone_eq.mask_contrast.to_bits(),
        ));
        match &planes.tone_eq {
            Some((kk, p)) if *kk == k => p.clone(),
            _ => {
                let p = Arc::new(timed("tone eq mask", || {
                    crate::toneeq::mask_plane_adjusted(
                        &img,
                        sg,
                        eps,
                        (s.light.exposure + s.tone_eq.mask_exposure) as f32,
                        s.tone_eq.mask_contrast as f32,
                    )
                }));
                planes.tone_eq = Some((k, p.clone()));
                p
            }
        }
    });
    Prepared { img, log_l, base, clarity_blur, texture_blur, chroma_blur, masks, layer_nr, px_per_long, tone_eq, sharp, haze, layer_sharp }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guided_preserves_step_edges_and_flattens_texture() {
        let p = Plane::from_fn(80, 20, |x, y| if x < 40 { -2.0 } else { 2.0 } + if (x + y) % 2 == 0 { 0.05 } else { -0.05 });
        let q = guided(&p, 4.0, 0.3);
        // edge kept
        assert!(q.get(35, 10) < -1.5 && q.get(45, 10) > 1.5);
        // checkerboard texture removed away from the edge
        assert!((q.get(10, 10) - q.get(11, 10)).abs() < 0.02);
    }

    #[test]
    fn wb_as_shot_is_identity_and_warming_adds_red() {
        let s = DevelopSettings::default();
        let info = SourceInfo::default();
        let mut img = Rgb32f::filled(4, 4, [0.3, 0.3, 0.3]);
        scene_linear_pre(&mut img, &info, &s);
        assert!((img.get(0, 0)[0] - 0.3).abs() < 1e-6);
        let mut warm = s.clone();
        warm.wb.mode = lightcraft_develop::WbMode::Custom;
        warm.wb.temp = 9000.0;
        let mut img2 = Rgb32f::filled(4, 4, [0.3, 0.3, 0.3]);
        scene_linear_pre(&mut img2, &info, &warm);
        let c = img2.get(0, 0);
        assert!(c[0] > c[2], "{c:?}");
        // luminance preserved
        assert!((luminance_2020(c) - 0.3).abs() < 0.01);
    }

    #[test]
    fn exposure_doubles() {
        let mut s = DevelopSettings::default();
        s.light.exposure = 1.0;
        let mut img = Rgb32f::filled(2, 2, [0.1, 0.2, 0.3]);
        scene_linear_pre(&mut img, &SourceInfo::default(), &s);
        assert!((img.get(1, 1)[2] - 0.6).abs() < 1e-5);
    }
}

#[cfg(test)]
mod fast_tests {
    use super::*;

    #[test]
    fn fast_guided_close_to_reference() {
        let p = Plane::from_fn(160, 120, |x, y| ((x as f32 * 0.13).sin() + (y as f32 * 0.07).cos()) * 2.0 + if x > 80 { 1.5 } else { 0.0 });
        let a = guided(&p, 12.0, 0.3);
        let b = guided_fast(&p, 12.0, 0.3);
        let err: f32 = a.data.iter().zip(&b.data).map(|(u, v)| (u - v).abs()).sum::<f32>() / a.len() as f32;
        assert!(err < 0.08, "{err}");
    }
}

#[cfg(test)]
mod nr_tests {
    use super::*;

    #[test]
    fn luminance_nr_reduces_noise_keeps_mean() {
        let mut img = Rgb32f::from_fn(64, 64, |x, y| {
            let n = (((x * 7919 + y * 104729) % 97) as f32 / 97.0 - 0.5) * 0.06;
            [0.2 + n; 3]
        });
        let var = |im: &Rgb32f| {
            let m = im.data.iter().map(|p| p[1]).sum::<f32>() / im.len() as f32;
            (im.data.iter().map(|p| (p[1] - m).powi(2)).sum::<f32>() / im.len() as f32, m)
        };
        let (v0, m0) = var(&img);
        let mut s = DevelopSettings::default();
        s.detail.nr_luminance = 80.0;
        denoise(&mut img, &s, 64, 64.0, 1.0);

        let (v1, m1) = var(&img);
        assert!(v1 < v0 * 0.5, "{v0} -> {v1}");
        assert!((m1 - m0).abs() < 0.01, "mean {m0} -> {m1}");
    }
}
