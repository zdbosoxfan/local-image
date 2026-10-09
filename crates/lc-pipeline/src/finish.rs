//! The per-pixel stage: everything after the spatial planes are ready, in one parallel pass.

use lightcraft_color::luminance_2020;
use lightcraft_color::spline::{Lut1, MonotoneCurve};
use lightcraft_color::transfer::linear_to_srgb;
use lightcraft_develop::{DevelopSettings, LocalAdjustments, ToneCurve, VignetteStyle};
use lightcraft_geom::Point;
use lightcraft_raster::Rgba8;

use crate::colorops::ColorOps;
use crate::geometry::Frame;
use crate::local::log_lum;
use crate::output::{DeepImage, DeepSamples, OutputDepth, OutputSpace, OutputTrc};
use crate::tone::ToneMap;
use crate::{Prepared, SourceInfo, for_rows};

#[inline]
fn smooth(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Parametric region curve (encoded domain) composed with the master point curve.
pub(crate) fn curve_luts(c: &ToneCurve) -> Option<CurveTables> {
    let parametric = c.highlights != 0.0 || c.lights != 0.0 || c.darks != 0.0 || c.shadows != 0.0;
    let master = !ToneCurve::point_curve_is_identity(&c.master);
    let chans = [&c.red, &c.green, &c.blue].map(|p| !ToneCurve::point_curve_is_identity(p));
    if !parametric && !master && !chans.iter().any(|b| *b) {
        return None;
    }
    const N: usize = 1024;
    let (s1, s2, s3) = ((c.split_shadows / 100.0) as f32, (c.split_mid / 100.0) as f32, (c.split_highlights / 100.0) as f32);
    let regions = [(0.0, s1, c.shadows), (s1, s2, c.darks), (s2, s3, c.lights), (s3, 1.0, c.highlights)];
    let mut base = Lut1::from_fn(N, |x| {
        let mut d = 0.0;
        for (a, b, amt) in regions {
            if amt == 0.0 {
                continue;
            }
            let (ctr, half) = ((a + b) / 2.0, (b - a) * 0.75 + 0.05);
            let t = ((x - ctr) / half).clamp(-1.0, 1.0);
            let win = 0.5 + 0.5 * (t * std::f32::consts::PI).cos();
            d += (amt / 100.0) as f32 * 0.22 * win;
        }
        (x + d * 4.0 * x * (1.0 - x)).clamp(0.0, 1.0)
    });
    // keep monotone
    for i in 1..N {
        base.v[i] = base.v[i].max(base.v[i - 1]);
    }
    let to_pts = |p: &[Point]| p.iter().map(|q| (q.x, q.y)).collect::<Vec<_>>();
    if master {
        base = MonotoneCurve::new(&to_pts(&c.master)).to_lut(N).compose(&base);
    }
    let per = [&c.red, &c.green, &c.blue];
    Some(CurveTables {
        tables: std::array::from_fn(|i| {
            if i == 0 {
                base.clone()
            } else if chans[i - 1] {
                MonotoneCurve::new(&to_pts(per[i - 1])).to_lut(N)
            } else {
                Lut1::identity(N)
            }
        }),
        luminance: c.mode == lightcraft_develop::CurveMode::Luminance,
    })
}

/// Master/parametric table followed by three independent channel tables, in working RGB.
pub struct CurveTables {
    pub tables: [Lut1; 4],
    pub luminance: bool,
}

impl CurveTables {
    /// Linear Rec.2020 in/out. The master uses encoded luminance or encoded RGB; channel
    /// curves follow it. Keeping this before the output conversion makes export spaces agree.
    pub fn apply(&self, c: [f32; 3], refine: f32) -> [f32; 3] {
        use lightcraft_color::transfer::srgb_to_linear;
        let mut d = if self.luminance {
            let y = luminance_2020(c).max(0.0);
            let z = srgb_to_linear(self.tables[0].eval(linear_to_srgb(y)));
            if y > 1e-9 { c.map(|x| x * z / y) } else { [z; 3] }
        } else {
            let e = c.map(linear_to_srgb);
            let q = e.map(|x| self.tables[0].eval(x));
            refine_saturation(e, q, refine).map(srgb_to_linear)
        };
        // Smooth hue-preserving path to white when a luminance lift exceeds the RGB cube.
        let y = luminance_2020(d).clamp(0.0, 1.0);
        let mx = d[0].max(d[1]).max(d[2]);
        if mx > 1.0 {
            let k = (1.0 - y) / (mx - y).max(1e-9);
            d = d.map(|x| y + (x - y) * k);
        }
        std::array::from_fn(|i| srgb_to_linear(self.tables[i + 1].eval(linear_to_srgb(d[i].max(0.0)))))
    }
}

/// Soft compression of channel distances from the achromatic axis (threshold 0.8,
/// input distance 1.3 maps to 1.0, power 1.2), followed by final gamut containment.
pub fn soft_gamut(r: [f32; 3]) -> [f32; 3] {
    let a = r[0].max(r[1]).max(r[2]);
    if a <= 1e-9 {
        return r;
    }
    let threshold = 0.8f32;
    let power = 1.2f32;
    let scale = (1.3f32 - threshold) / (((1.0 - threshold) / (1.3 - threshold)).powf(-power) - 1.0).powf(1.0 / power);
    r.map(|x| {
        let distance = (a - x) / a;
        if distance <= threshold {
            x
        } else {
            let u = (distance - threshold) / scale;
            let compressed = threshold + scale * u / (1.0 + u.powf(power)).powf(1.0 / power);
            a * (1.0 - compressed)
        }
    })
}

/// Position-hashed TPDF noise shared with WGSL; exactly representable 16-bit fractions.
pub const DITHER_HASH: [u32; 3] = [0x7feb_352d, 0x846c_a68b, 0x9e37_79b9];

pub fn dither8(v: f32, x: usize, y: usize, _channel: usize) -> u8 {
    let mut h = (x as u32).wrapping_mul(DITHER_HASH[2]) ^ (y as u32).wrapping_mul(DITHER_HASH[0]);
    h ^= h >> 16;
    h = h.wrapping_mul(DITHER_HASH[0]);
    h ^= h >> 15;
    h = h.wrapping_mul(DITHER_HASH[1]);
    h ^= h >> 16;
    let noise = (h & 65535) as f32 / 65536.0 - (h >> 16) as f32 / 65536.0;
    let q = v.clamp(0.0, 1.0) * 255.0;
    let fade = q.min(255.0 - q).clamp(0.0, 1.0);
    (q + noise * fade + 0.5).clamp(0.0, 255.0) as u8
}

/// Vignette (post-crop) parameters.
#[derive(Clone, Copy, Debug)]
pub struct Vig {
    pub amount: f32,
    pub start: f32,
    pub width: f32,
    pub aspect_mix: f32,
    pub power: f32,
    pub highlights: f32,
    pub style: VignetteStyle,
}

pub(crate) fn vignette(s: &DevelopSettings) -> Option<Vig> {
    let v = &s.vignette;
    (v.amount != 0.0).then(|| {
        let r = (v.roundness / 100.0) as f32;
        Vig {
            amount: (v.amount / 100.0) as f32,
            start: 0.15 + (v.midpoint / 100.0) as f32 * 0.95,
            width: 0.05 + (v.feather / 100.0) as f32 * 1.1,
            aspect_mix: ((r + 1.0) / 2.0).clamp(0.0, 1.0),
            power: if r >= 0.0 { 2.0 } else { 2.0 + (-r) * 6.0 },
            highlights: (v.highlights / 100.0) as f32,
            style: v.style,
        }
    })
}

/// Grain parameters of `s` (amount, cell size px, roughness, seed) for a `px_per_long` render.
pub(crate) fn grain(s: &DevelopSettings, px_per_long: f64) -> Option<(f32, f32, f32, u32)> {
    (s.grain.amount > 0.0).then(|| {
        let cell = (0.0006 + (s.grain.size / 100.0) as f32 * 0.0024) * px_per_long as f32;
        ((s.grain.amount / 100.0) as f32 * 0.13, cell.max(0.6), (s.grain.roughness / 100.0) as f32, s.grain.seed)
    })
}

/// The tone map for `s` on `info`'s source with these contrast / whites / blacks.
pub(crate) fn tone_map(s: &DevelopSettings, info: &SourceInfo, contrast: f64, whites: f64, blacks: f64) -> ToneMap {
    crate::tone2::tone_map(s, info, contrast, whites, blacks)
}

/// Hash constants of [`grain_noise`] (shared with the GPU kernel).
pub const GRAIN_HASH: [u32; 4] = [0x8da6_b343, 0xd816_3841, 0xcb1a_b31f, 0x5bd1_e995];

#[inline]
fn grain_noise(x: f32, y: f32, seed: u32) -> f32 {
    let (x0, y0) = (x.floor(), y.floor());
    let (fx, fy) = (x - x0, y - y0);
    let h = |i: i32, j: i32| {
        let mut v = (i as u32).wrapping_mul(GRAIN_HASH[0]) ^ (j as u32).wrapping_mul(GRAIN_HASH[1]) ^ seed.wrapping_mul(GRAIN_HASH[2]);
        v ^= v >> 13;
        v = v.wrapping_mul(GRAIN_HASH[3]);
        v ^= v >> 15;
        (v & 0xffff) as f32 / 32768.0 - 1.0
    };
    let (i, j) = (x0 as i32, y0 as i32);
    let (u, v) = (fx * fx * (3.0 - 2.0 * fx), fy * fy * (3.0 - 2.0 * fy));
    let a = h(i, j) + (h(i + 1, j) - h(i, j)) * u;
    let b = h(i, j + 1) + (h(i + 1, j + 1) - h(i, j + 1)) * u;
    a + (b - a) * v
}

/// Number of per-mask terms in [`mask_terms`].
pub const MASK_TERMS: usize = 21;

/// Number of alpha-weighted terms at the start of [`mask_terms`].
pub const MASK_SUMS: usize = 17;

/// A mask's local adjustments as the per-pixel stage uses them (alpha-weighted sums), in order:
/// exposure, temp, tint, contrast, highlights, shadows, whites, blacks, texture, clarity, dehaze,
/// saturation, hue, sharpness, noise, moiré, defringe, then the colour overlay: on (0/1), cos and
/// sin of its OkLCh hue, and its strength.
pub fn mask_terms(j: &LocalAdjustments) -> [f32; MASK_TERMS] {
    let (on, cos, sin, amt) = if j.color_sat > 0.0 {
        let hue = crate::colorops::oklch_hue_of_srgb_hue(j.color_hue);
        (1.0, hue.cos(), hue.sin(), (j.color_sat / 100.0) as f32)
    } else {
        (0.0, 0.0, 0.0, 0.0)
    };
    [
        j.exposure as f32,
        (j.temp / 100.0) as f32,
        (j.tint / 100.0) as f32,
        (j.contrast / 100.0) as f32,
        (j.highlights / 100.0) as f32,
        (j.shadows / 100.0) as f32,
        (j.whites / 100.0) as f32,
        (j.blacks / 100.0) as f32,
        (j.texture / 100.0) as f32,
        (j.clarity / 100.0) as f32,
        (j.dehaze / 100.0) as f32,
        (j.saturation / 100.0) as f32,
        (j.hue / 100.0) as f32 * 0.6,
        (j.sharpness / 100.0) as f32,
        (j.noise / 100.0) as f32,
        (j.moire / 100.0) as f32,
        (j.defringe / 100.0) as f32,
        on,
        cos,
        sin,
        amt,
    ]
}

/// Everything the per-pixel stage computes once per render: the CPU loop below and the GPU kernel
/// (`lightcraft-gpu`) both read their parameters from here, so the two cannot drift apart.
pub struct FinishParams {
    /// A LUT profile and its amount (0..2), applied to the display-encoded colour.
    pub lut: Option<(std::sync::Arc<crate::lut::Lut3d>, f32)>,
    pub tone: ToneMap,
    pub ops: ColorOps,
    /// Calibration: primaries matrix (row-major, linear Rec.2020) and shadows tint (−1..1).
    pub calib: Option<[[f32; 3]; 3]>,
    pub shadow_tint: f32,
    /// Tone curves (parametric ∘ point, per channel) on encoded values, 1024 entries each.
    pub curves: Option<CurveTables>,
    /// Refine Saturation as 0..1 (1 = the curves' own saturation).
    pub refine_sat: f32,
    pub soft_gamut: bool,
    pub vig: Option<Vig>,
    /// Linear Rec.2020 → linear output RGB, the output's luminance weights (gamut mapping) and its
    /// encoding curve (see [`crate::output`]).
    pub to_out: [[f32; 3]; 3],
    pub out_luma: [f32; 3],
    pub out_trc: OutputTrc,
    /// Soft proofing (CPU only; the GPU path declines proof renders).
    pub proof: Option<crate::output::ProofParams>,
    pub hl: f32,
    pub sh: f32,
    pub clar: f32,
    pub tex: f32,
    pub dehaze: f32,
    pub sharpen: f32,
    pub sharpen_mask: f32,
    /// Process version 2026's detail tools (see [`crate::detail`]): sharpening Amount / 100,
    /// Detail and Masking (0..1), the sharpening planes' blur radius (output px), and the
    /// dehaze airlight before exposure (the last two from the prepared planes, see
    /// [`FinishParams::with_planes`]).
    pub v2026: bool,
    pub sharp_amount: f32,
    pub sharp_detail: f32,
    pub sharp_masking: f32,
    pub sharp_sigma: f32,
    pub haze_air: [f32; 3],
    /// Airlight after and before exposure, exposure gain and EV (see [`crate::Prepared`]).
    pub air: f32,
    pub air_pre: f32,
    pub gain: f32,
    pub ev: f32,
    /// Grain: amount, cell size (px), roughness, seed.
    pub grain: Option<(f32, f32, f32, u32)>,
    /// Output px → normalized oriented coordinates.
    pub out_to_norm: lightcraft_geom::Affine,
    pub ow: f64,
    pub oh: f64,
    pub w: usize,
    pub h: usize,
    pub px_per_long: f64,
    /// Develop layers that change something (CPU only: the GPU path declines them).
    pub layers: Vec<crate::layers::LayerK>,
    /// The tone equalizer's curve and mask compensation (CPU only: the GPU path declines it).
    pub tone_eq: Option<(crate::toneeq::Curve, crate::toneeq::MaskAdjust)>,
}

impl FinishParams {
    /// Parameters for a `w × h` render into `space`; `ev`/`air_pre` as in [`crate::Prepared`].
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        s: &DevelopSettings,
        frame: &Frame,
        info: &SourceInfo,
        w: usize,
        h: usize,
        px_per_long: f64,
        air_pre: f32,
        space: OutputSpace,
    ) -> FinishParams {
        let effects = s.section_enabled("effects");
        let (clar, tex, dehaze) = if effects {
            ((s.effects.clarity / 100.0) as f32, (s.effects.texture / 100.0) as f32, (s.effects.dehaze / 100.0) as f32)
        } else {
            (0.0, 0.0, 0.0)
        };
        let ev = s.light.exposure as f32;
        let gain = 2f32.powf(ev);
        let grain = if effects { grain(s, px_per_long) } else { None };
        let calibration = s.section_enabled("calibration");
        FinishParams {
            calib: if calibration { crate::colorops::calibration_matrix(&s.calibration) } else { None },
            shadow_tint: if calibration { (s.calibration.shadows_tint / 100.0) as f32 } else { 0.0 },
            tone: tone_map(s, info, s.light.contrast, s.light.whites, s.light.blacks),
            layers: crate::layers::resolve(s, info, px_per_long),
            tone_eq: crate::toneeq::of(s),
            lut: crate::lut::get(&s.profile.id).map(|l| (l, (s.profile.amount / 100.0).clamp(0.0, 2.0) as f32)),
            ops: ColorOps::new(s),
            curves: curve_luts(&s.curve),
            soft_gamut: info.raw,
            refine_sat: (s.curve.refine_saturation / 100.0).clamp(0.0, 1.0) as f32,
            vig: if effects { vignette(s) } else { None },
            to_out: space.from_working(),
            out_luma: space.luma(),
            out_trc: space.trc(),
            proof: None,
            hl: (s.light.highlights / 100.0) as f32,
            sh: (s.light.shadows / 100.0) as f32,
            clar,
            tex,
            dehaze,
            sharpen: if s.v2026() { 0.0 } else { (s.detail.sharpen_amount / 150.0) as f32 },
            sharpen_mask: (s.detail.sharpen_masking / 100.0) as f32,
            v2026: s.v2026(),
            sharp_amount: if s.v2026() { (s.detail.sharpen_amount / 100.0).clamp(0.0, 1.5) as f32 } else { 0.0 },
            sharp_detail: (s.detail.sharpen_detail / 100.0).clamp(0.0, 1.0) as f32,
            sharp_masking: (s.detail.sharpen_masking / 100.0).clamp(0.0, 1.0) as f32,
            sharp_sigma: 0.0,
            haze_air: [1.0; 3],
            air: air_pre * gain,
            air_pre,
            gain,
            ev,
            grain,
            out_to_norm: frame.out_to_norm(w, h),
            ow: frame.ow,
            oh: frame.oh,
            w,
            h,
            px_per_long,
        }
    }
}

impl FinishParams {
    /// The process 2026 values that come with the planes: the sharpening radius (output px; 0
    /// when the planes aren't there) and the dehaze airlight (before exposure).
    pub fn with_planes(mut self, sharp_sigma: Option<f32>, haze_air: Option<[f32; 3]>) -> FinishParams {
        self.sharp_sigma = sharp_sigma.unwrap_or(0.0);
        self.haze_air = haze_air.unwrap_or([1.0; 3]);
        self
    }
}

pub(crate) fn finish(p: &Prepared, s: &DevelopSettings, frame: &Frame, info: &SourceInfo, space: OutputSpace, proof: Option<crate::Proof>) -> Rgba8 {
    let (w, h) = (p.img.width, p.img.height);
    let mut fp = FinishParams::new(s, frame, info, w, h, p.px_per_long, p.air, space).with_planes(p.sharp.as_ref().map(|x| x.2), p.haze.as_ref().map(|x| x.1));
    fp.proof = proof.map(|pr| pr.params(space));
    let trc = fp.out_trc;
    let data = finish_with(p, &fp, false, |e, x, y| {
        let e = match trc {
            OutputTrc::Srgb => e,
            t => e.map(|v| t.encode(lightcraft_color::transfer::srgb_to_linear(v.clamp(0.0, 1.0)))),
        };
        [dither8(e[0], x, y, 0), dither8(e[1], x, y, 1), dither8(e[2], x, y, 2), 255]
    });
    Rgba8 { width: w, height: h, data }
}

/// [`finish`] into 16-bit display-encoded or 32-bit float linear samples (output primaries).
#[allow(clippy::too_many_arguments)]
pub(crate) fn finish_deep(
    p: &Prepared,
    s: &DevelopSettings,
    frame: &Frame,
    info: &SourceInfo,
    space: OutputSpace,
    depth: OutputDepth,
    proof: Option<crate::Proof>,
) -> DeepImage {
    use lightcraft_color::transfer::srgb_to_linear;
    let (w, h) = (p.img.width, p.img.height);
    let mut fp = FinishParams::new(s, frame, info, w, h, p.px_per_long, p.air, space).with_planes(p.sharp.as_ref().map(|x| x.2), p.haze.as_ref().map(|x| x.1));
    fp.proof = proof.map(|pr| pr.params(space));
    let trc = fp.out_trc;
    let samples = match depth {
        OutputDepth::F32Linear => {
            let v = finish_with(p, &fp, true, |e, _, _| e.map(|v| srgb_to_linear(v.clamp(0.0, 1.0))));
            DeepSamples::F32(v.into_flattened())
        }
        _ => {
            let q = |v: f32| (v.clamp(0.0, 1.0) * 65535.0 + 0.5) as u16;
            let v = finish_with(p, &fp, true, |e, _, _| match trc {
                OutputTrc::Srgb => e.map(q),
                t => e.map(|v| q(t.encode(srgb_to_linear(v.clamp(0.0, 1.0))))),
            });
            DeepSamples::U16(v.into_flattened())
        }
    };
    DeepImage { width: w, height: h, space, samples }
}

/// The per-pixel stage: every output pixel's colour in the output primaries, encoded with the sRGB
/// curve (tone curves and grain applied; not clamped), handed to `store` for the final encoding.
/// `exact`: encode with the exact sRGB curve instead of the (8/10-bit accurate) table.
pub(crate) fn finish_with<T: Copy + Default + Send>(
    p: &Prepared,
    fp: &FinishParams,
    exact: bool,
    store: impl Fn([f32; 3], usize, usize) -> T + Sync + Send,
) -> Vec<T> {
    let (w, h) = (p.img.width, p.img.height);
    let p_lut = fp.lut.clone();
    let FinishParams {
        tone,
        ops,
        curves,
        vig,
        to_out,
        out_luma,
        hl,
        sh,
        clar,
        tex,
        dehaze,
        sharpen,
        sharpen_mask,
        air,
        air_pre,
        gain,
        ev,
        grain,
        ..
    } = fp;
    let (hl, sh, clar, tex, dehaze, sharpen, sharpen_mask, air, air_pre, gain, ev) =
        (*hl, *sh, *clar, *tex, *dehaze, *sharpen, *sharpen_mask, *air, *air_pre, *gain, *ev);
    let terms: Vec<[f32; MASK_TERMS]> = p.masks.iter().map(|m| mask_terms(&m.adjust)).collect();
    let out_to_norm = fp.out_to_norm;
    let long = fp.ow.max(fp.oh);
    let aspect = w as f32 / h as f32;
    // process 2026 (see `crate::detail`)
    let v2026 = fp.v2026;
    let sharp_k = crate::detail::SharpK::new(fp.sharp_detail, fp.sharp_masking, fp.sharp_sigma);
    let haze_air = fp.haze_air.map(|v| v * gain);

    let srgb = srgb_lut();
    let mut out = vec![T::default(); w * h];
    for_rows(&mut out, w, |y, row| {
        for (x, px) in row.iter_mut().enumerate() {
            let i = y * w + x;
            let mut raw = p.img.data[i];
            // develop layers' noise reduction (see `crate::layers`)
            for (k, im) in &p.layer_nr {
                let a = p.masks[*k].alpha.data[i];
                if a > 0.0 {
                    raw = mix3(raw, im.data[i], a);
                }
            }
            let mut c = if gain == 1.0 { raw } else { raw.map(|v| v * gain) };
            let l_pre = p.log_l.data[i];
            let l0 = l_pre + ev;

            // --- local (mask) contributions: alpha-weighted sums of the masks' terms
            let mut l = [0.0f32; MASK_SUMS];
            let mut tint_col: Option<([f32; 3], f32)> = None;
            for (m, t) in p.masks.iter().zip(&terms) {
                let a = m.alpha.data[i];
                if a <= 0.0 {
                    continue;
                }
                for k in 0..MASK_SUMS {
                    l[k] += a * t[k];
                }
                if t[MASK_SUMS] > 0.0 {
                    tint_col = Some(([t[MASK_SUMS + 1], t[MASK_SUMS + 2], 0.0], a * t[MASK_SUMS + 3]));
                }
            }
            let [l_exp, l_temp, l_tint, l_con, l_hl, l_sh, l_wh, l_bl, l_tex, l_clar, l_dehaze, l_sat, l_hue, l_sharp, l_noise, l_moire, l_defringe] =
                l;

            // --- local Moiré (and the colour part of Noise): chromaticity towards its blur
            let mt = (l_moire + 0.5 * l_noise.max(0.0)).clamp(-1.0, 1.0);
            if mt != 0.0
                && let Some(cb) = &p.chroma_blur
            {
                let (y, ch0, cb) = (luminance_2020(c), crate::masks::chromaticity(raw), cb.data[i]);
                c = std::array::from_fn(|k| ((ch0[k] + (cb[k] - ch0[k]) * mt) * y).max(0.0));
            }
            // --- local Defringe: desaturate purple / green fringes along edges
            let df = l_defringe.clamp(0.0, 1.0);
            if df > 0.0
                && let Some(b) = &p.texture_blur
            {
                let k = df * defringe_weight(c, p.log_l.data[i] - b.data[i]);
                if k > 0.0 {
                    let y = luminance_2020(c);
                    c = c.map(|v| v + (y - v) * k);
                }
            }

            // --- dehaze (scene linear)
            let dz = dehaze + l_dehaze;
            if dz != 0.0
                && let Some(dark) = &p.dark
            {
                c = dehaze_px(c, dz, dark.data[i], air_pre, air);
            }
            if dz != 0.0
                && let Some((hz, _)) = &p.haze
            {
                c = crate::detail::dehaze_px(c, dz, hz.data[i], haze_air);
            }
            // layers: their dehaze, then (below) their white balance and exposure
            let mut layer_scene = false;
            for l in fp.layers.iter().filter(|l| l.scene()) {
                let a = p.masks[l.mask].alpha.data[i];
                if a <= 0.0 {
                    continue;
                }
                layer_scene = true;
                if l.dehaze != 0.0
                    && let Some(dark) = &p.dark
                {
                    c = mix3(c, dehaze_px(c, l.dehaze, dark.data[i], air_pre, air), a);
                }
                if l.dehaze != 0.0
                    && let Some((hz, _)) = &p.haze
                {
                    c = mix3(c, crate::detail::dehaze_px(c, l.dehaze, hz.data[i], haze_air), a);
                }
            }

            // --- local exposure / temp / tint
            if l_exp != 0.0 {
                let g = l_exp.exp2();
                c = c.map(|v| v * g);
            }
            if l_temp != 0.0 || l_tint != 0.0 {
                let y0 = luminance_2020(c);
                c = [c[0] * (1.0 + 0.3 * l_temp), c[1] * (1.0 - 0.22 * l_tint), c[2] * (1.0 - 0.3 * l_temp).max(0.0)];
                let y1 = luminance_2020(c).max(1e-9);
                c = c.map(|v| v * y0 / y1);
            }
            if layer_scene {
                for l in fp.layers.iter().filter(|l| l.wb.is_some() || l.gain.is_some()) {
                    let a = p.masks[l.mask].alpha.data[i];
                    if a <= 0.0 {
                        continue;
                    }
                    let mut q = match &l.wb {
                        Some(m) => mul3(m, c).map(|v| v.max(0.0)),
                        None => c,
                    };
                    if let Some(g) = l.gain {
                        q = q.map(|v| v * g);
                    }
                    c = mix3(c, q, a);
                }
            }

            // --- tone equalizer: exposure by luminance zone (its mask is pre-exposure, see `toneeq`)
            let mut teq = false;
            if let (Some((curve, adj)), Some(m)) = (&fp.tone_eq, &p.tone_eq) {
                let g = curve.gain(adj.zone_ev(m.data[i], ev));
                if g != 1.0 {
                    c = c.map(|v| v * g);
                    teq = true;
                }
            }

            // --- local tone in log luminance
            let l1 = if dz != 0.0 || l_exp != 0.0 || layer_scene || teq { log_lum(c) } else { l0 };
            let shift = l1 - l0;
            let base = p.base.data[i] + ev + shift;
            let mut delta = 0.0f32;
            let (hh, ss) = (hl + l_hl, sh + l_sh);
            if hh != 0.0 || ss != 0.0 {
                let ws = 1.0 - smooth(-4.8, 0.3, base);
                let wh = smooth(-1.0, 2.8, base);
                delta += ss * 1.7 * ws * ws.sqrt() + hh * 1.7 * wh;
            }
            if l_wh != 0.0 {
                delta += l_wh * 0.8 * smooth(0.5, 3.0, l1);
            }
            if l_bl != 0.0 {
                delta += l_bl * 0.8 * (1.0 - smooth(-6.0, -1.5, l1));
            }
            if l_con != 0.0 {
                delta += l_con * 0.14 * l1.clamp(-6.0, 4.0);
            }
            let cl = clar + l_clar;
            if cl != 0.0
                && let Some(b) = &p.clarity_blur
            {
                let det = (l_pre - b.data[i]).clamp(-2.5, 2.5);
                let mid = (-(base / 3.2).powi(2)).exp();
                delta += cl * 0.85 * det * (0.35 + 0.65 * mid);
            }
            let tx = tex + l_tex;
            let sp = if v2026 { 0.0 } else { l_sharp * 0.6 + sharpen };
            if (tx != 0.0 || sp != 0.0)
                && let Some(b) = &p.texture_blur
            {
                let det = l_pre - b.data[i];
                let tame = 1.0 - 0.6 * smooth(0.4, 1.6, det.abs());
                delta += tx * 1.1 * det.clamp(-1.0, 1.0) * tame;
                if sp != 0.0 {
                    let m = if sharpen_mask > 0.0 { smooth(sharpen_mask * 0.25, sharpen_mask * 0.25 + 0.15, det.abs()) } else { 1.0 };
                    delta += sp * 1.3 * det.clamp(-0.8, 0.8) * m;
                }
            }
            // process 2026 sharpening: pixel-scale, on its own planes (local Sharpness too)
            let a26 = fp.sharp_amount + l_sharp * 0.9;
            if a26 != 0.0
                && let Some((b1, b2, _)) = &p.sharp
            {
                delta += crate::detail::sharpen_at(a26, &sharp_k, &p.log_l.data, &b1.data, &b2.data, w, h, x, y);
            }
            // local Noise: smooth (or, negative, boost) small-amplitude detail, keep edges
            if l_noise != 0.0
                && let Some(b) = &p.texture_blur
            {
                let det = l_pre - b.data[i];
                delta -= l_noise.clamp(-1.0, 1.0) * 0.9 * det * (1.0 - smooth(0.1, 0.5, det.abs()));
            }
            if delta != 0.0 {
                let g = delta.exp2();
                c = c.map(|v| v * g);
            }
            for l in &fp.layers {
                let Some(lt) = &l.local else { continue };
                let a = p.masks[l.mask].alpha.data[i];
                if a <= 0.0 {
                    continue;
                }
                let mut dl = layer_tone_delta(lt, base, l_pre, p.clarity_blur.as_ref().map(|b| b.data[i]), p.texture_blur.as_ref().map(|b| b.data[i]));
                if lt.sharpen_2026 != 0.0
                    && let Some((b1, b2, sg)) = &p.sharp
                {
                    let k = crate::detail::SharpK::new(lt.sharpen_detail, lt.sharpen_mask, *sg);
                    dl += crate::detail::sharpen_at(lt.sharpen_2026, &k, &p.log_l.data, &b1.data, &b2.data, w, h, x, y);
                }

                if dl != 0.0 {
                    let g = dl.exp2();
                    c = mix3(c, c.map(|v| v * g), a);
                }
            }

            // --- calibration (scene linear, before the tone map)
            if fp.calib.is_some() || fp.shadow_tint != 0.0 {
                c = crate::colorops::calibrate(c, fp.calib.as_ref(), fp.shadow_tint);
            }

            // --- tone map on luminance, highlight desaturation
            let mut d = tone_px(tone, c);
            for l in &fp.layers {
                if let Some(t) = &l.tone {
                    let a = p.masks[l.mask].alpha.data[i];
                    if a > 0.0 {
                        d = mix3(d, tone_px(t, c), a);
                    }
                }
            }

            // --- colour
            d = ops.apply(d, l_sat, l_hue);
            if let Some((dir, amt)) = tint_col {
                let lab = lightcraft_color::perceptual::oklab_from_2020(d);
                d = lightcraft_color::perceptual::oklab_to_2020([lab[0], lab[1] + dir[0] * 0.08 * amt, lab[2] + dir[1] * 0.08 * amt]);
            }
            for l in &fp.layers {
                if let Some(o) = &l.ops {
                    let a = p.masks[l.mask].alpha.data[i];
                    if a > 0.0 {
                        d = mix3(d, o.apply(d, 0.0, 0.0), a);
                    }
                }
            }

            // --- vignette (display linear, post-crop)
            if let Some(v) = vig {
                d = vignette_px(v, d, x, y, w, h, aspect);
            }
            for l in &fp.layers {
                if let Some(v) = &l.vig {
                    let a = p.masks[l.mask].alpha.data[i];
                    if a > 0.0 {
                        d = mix3(d, vignette_px(v, d, x, y, w, h, aspect), a);
                    }
                }
            }

            if let Some(l) = curves {
                d = l.apply(d, fp.refine_sat);
            }
            for l in &fp.layers {
                if let Some((lut, refine)) = &l.curves {
                    let a = p.masks[l.mask].alpha.data[i];
                    if a > 0.0 {
                        d = mix3(d, lut.apply(d, *refine), a);
                    }
                }
            }

            // --- gamut map to the output space (desaturate towards luminance until in range);
            // soft proofing maps into the proof space first and shows that in the output space
            let mut warn = None;
            let mut r = match &fp.proof {
                Some(pp) => {
                    let q0 = mul3(&pp.to_proof, d);
                    let (_, t) = gamut_map(q0, pp.luma);
                    let q = gamut_map(if fp.soft_gamut { soft_gamut(q0) } else { q0 }, pp.luma).0;
                    if pp.dest_warning && out_of_gamut(q0, t) {
                        warn = Some(crate::output::PROOF_DEST_WARNING);
                    }
                    if pp.to_proof == *to_out { q } else { mul3(&pp.proof_to_out, q) }
                }
                None => mul3(to_out, d),
            };
            if fp.soft_gamut && fp.proof.is_none() {
                r = soft_gamut(r);
            }
            let (mapped, t) = gamut_map(r, *out_luma);
            if fp.proof.is_some_and(|pp| pp.display_warning) && warn.is_none() && out_of_gamut(r, t) {
                warn = Some(crate::output::PROOF_DISPLAY_WARNING);
            }
            r = mapped;

            // --- encode, curves, grain
            let mut e = if exact { r.map(|v| linear_to_srgb(v.clamp(0.0, 1.0))) } else { r.map(|v| encode_srgb(srgb, v)) };
            let n_at = || out_to_norm.apply(Point::new(x as f64 + 0.5, y as f64 + 0.5));
            if let Some(g) = *grain {
                e = grain_px(e, g, n_at(), fp, long);
            }
            for l in &fp.layers {
                if let Some(g) = l.grain {
                    let a = p.masks[l.mask].alpha.data[i];
                    if a > 0.0 {
                        e = mix3(e, grain_px(e, g, n_at(), fp, long), a);
                    }
                }
            }
            let e = match &p_lut {
                Some((l, k)) => {
                    let m = l.apply(e.map(|v| v.clamp(0.0, 1.0)));
                    [e[0] + (m[0] - e[0]) * k, e[1] + (m[1] - e[1]) * k, e[2] + (m[2] - e[2]) * k]
                }
                None => e,
            };
            *px = store(warn.unwrap_or(e), x, y);
        }
    });
    out
}

use crate::layers::mix3;

/// Dehaze of scene-linear `c` by `dz` (−1..1) with dark channel `dark` (before exposure),
/// airlight before / after exposure.
#[inline]
fn dehaze_px(c: [f32; 3], dz: f32, dark: f32, air_pre: f32, air: f32) -> [f32; 3] {
    let d = (dark / air_pre).clamp(0.0, 1.0);
    if dz > 0.0 {
        let t = (1.0 - 0.95 * dz.min(1.0) * d).max(0.12);
        // A tiny mask-alpha residue can select this branch with no representable
        // veil removal. Keep negative scene channels for tone/gamut handling,
        // just as when dehaze is not selected at all (GPU: `finish.wgsl`).
        if t == 1.0 {
            return c;
        }
        c.map(|v| ((v - air * (1.0 - t)) / t).max(0.0))
    } else {
        let k = (-dz).min(1.0) * 0.7 * (0.35 + 0.65 * d);
        c.map(|v| v + (air * 0.9 - v) * k)
    }
}

/// The tone map on scene-linear `c`'s luminance, then the curve's chroma scale and highlight
/// desaturation (display linear).
#[inline]
fn tone_px(tone: &ToneMap, c: [f32; 3]) -> [f32; 3] {
    crate::tone2::tone_px(tone, &tone.method(), c)
}

/// The post-crop vignette at output pixel (x, y) of a `w × h` render on display-linear `d`.
#[inline]
fn vignette_px(v: &Vig, mut d: [f32; 3], x: usize, y: usize, w: usize, h: usize, aspect: f32) -> [f32; 3] {
    let u = (x as f32 + 0.5) / w as f32 * 2.0 - 1.0;
    let vv = (y as f32 + 0.5) / h as f32 * 2.0 - 1.0;
    let sx = 1.0 + (aspect - 1.0) * v.aspect_mix;
    let sy = 1.0 + (1.0 / aspect - 1.0) * v.aspect_mix;
    let (ax, ay) = ((u * sx.max(1.0) / sx.max(sy)).abs(), (vv * sy.max(1.0) / sx.max(sy)).abs());
    let dist = (ax.powf(v.power) + ay.powf(v.power)).powf(1.0 / v.power);
    let t = smooth(v.start, v.start + v.width, dist);
    if t > 0.0 {
        let lum = luminance_2020(d).clamp(0.0, 1.0);
        if v.amount < 0.0 {
            let mut f = 1.0 + v.amount * t;
            if v.style == VignetteStyle::HighlightPriority {
                f += (1.0 - f) * v.highlights * smooth(0.4, 1.0, lum);
            }
            if v.style == VignetteStyle::PaintOverlay {
                d = d.map(|c| c * (1.0 - (-v.amount) * t) + 0.0);
            } else {
                d = d.map(|c| c * f);
            }
        } else {
            d = d.map(|c| c + (1.0 - c) * v.amount * t * 0.85);
        }
    }
    d
}

/// Grain `(amount, cell px, roughness, seed)` on encoded `e` at normalized image point `n`.
#[inline]
fn grain_px(e: [f32; 3], (amt, cell, rough, seed): (f32, f32, f32, u32), n: Point, fp: &FinishParams, long: f64) -> [f32; 3] {
    let (gx, gy) = ((n.x * fp.ow) as f32 / long as f32, (n.y * fp.oh) as f32 / long as f32);
    let sc = fp.px_per_long as f32 / cell;
    let mut g = grain_noise(gx * sc, gy * sc, seed);
    g = g * (1.0 - rough * 0.5) + grain_noise(gx * sc * 2.3, gy * sc * 2.3, seed ^ 0x55) * rough * 0.7;
    let lum = 0.2126 * e[0] + 0.7152 * e[1] + 0.0722 * e[2];
    let k = amt * g * (0.35 + 2.6 * lum * (1.0 - lum));
    e.map(|v| v + k)
}

/// A develop layer's local tone change (log2 gain) at a pixel: its highlights/shadows on the
/// edge-aware `base`, clarity and texture / sharpening on the detail of `l_pre` against the
/// photo's clarity and texture blurs (the photo's formulas, see `finish_with`).
#[inline]
fn layer_tone_delta(t: &crate::layers::LocalTone, base: f32, l_pre: f32, clar_b: Option<f32>, tex_b: Option<f32>) -> f32 {
    let mut delta = 0.0f32;
    if t.hl != 0.0 || t.sh != 0.0 {
        let ws = 1.0 - smooth(-4.8, 0.3, base);
        let wh = smooth(-1.0, 2.8, base);
        delta += t.sh * 1.7 * ws * ws.sqrt() + t.hl * 1.7 * wh;
    }
    if t.clar != 0.0
        && let Some(b) = clar_b
    {
        let det = (l_pre - b).clamp(-2.5, 2.5);
        let mid = (-(base / 3.2).powi(2)).exp();
        delta += t.clar * 0.85 * det * (0.35 + 0.65 * mid);
    }
    if (t.tex != 0.0 || t.sharpen != 0.0)
        && let Some(b) = tex_b
    {
        let det = l_pre - b;
        let tame = 1.0 - 0.6 * smooth(0.4, 1.6, det.abs());
        delta += t.tex * 1.1 * det.clamp(-1.0, 1.0) * tame;
        if t.sharpen != 0.0 {
            let m = if t.sharpen_mask > 0.0 { smooth(t.sharpen_mask * 0.25, t.sharpen_mask * 0.25 + 0.15, det.abs()) } else { 1.0 };
            delta += t.sharpen * 1.3 * det.clamp(-0.8, 0.8) * m;
        }
    }
    delta
}

/// A colour needing more desaturation than this (scale < `GAMUT_WARN`) to fit is out of gamut for
/// the gamut warnings (tolerates rounding at the gamut boundary).
const GAMUT_WARN: f32 = 0.995;

/// For the gamut warnings: `r` needed desaturating by `t` to fit, and isn't simply a neutral
/// beyond white or below black (that's clipping, which the clipping warnings show).
#[inline]
fn out_of_gamut(r: [f32; 3], t: f32) -> bool {
    let (lo, hi) = (r[0].min(r[1]).min(r[2]), r[0].max(r[1]).max(r[2]));
    t < GAMUT_WARN && (hi - lo) > 0.02 * hi.abs().max(1e-3) && hi - lo > 1e-3 && (lo < -1e-3 || (hi > 1.0 && lo < 1.0))
}

#[inline]
fn mul3(m: &[[f32; 3]; 3], d: [f32; 3]) -> [f32; 3] {
    [
        m[0][0] * d[0] + m[0][1] * d[1] + m[0][2] * d[2],
        m[1][0] * d[0] + m[1][1] * d[1] + m[1][2] * d[2],
        m[2][0] * d[0] + m[2][1] * d[1] + m[2][2] * d[2],
    ]
}

/// Desaturate linear `r` towards its luminance (weights `luma`) until every channel is in 0..1;
/// returns the mapped colour and the chroma scale used (1 = already in gamut).
#[inline]
pub fn gamut_map(r: [f32; 3], luma: [f32; 3]) -> ([f32; 3], f32) {
    let yy = (luma[0] * r[0] + luma[1] * r[1] + luma[2] * r[2]).clamp(0.0, 1.0);
    let mut t = 1.0f32;
    for c in r {
        if c < 0.0 {
            t = t.min(yy / (yy - c).max(1e-9));
        } else if c > 1.0 {
            t = t.min((1.0 - yy) / (c - yy).max(1e-9));
        }
    }
    if t < 1.0 { (r.map(|c| yy + (c - yy) * t), t) } else { (r, 1.0) }
}

/// Local Defringe weight of a scene-linear colour `c` whose log luminance differs by `det` from
/// its fine blur: high on strong edges with a purple or green cast.
#[inline]
pub fn defringe_weight(c: [f32; 3], det: f32) -> f32 {
    let y = luminance_2020(c).max(1e-6);
    let purple = (c[0].min(c[2]) - c[1]) / y;
    let green = (c[1] - c[0].max(c[2])) / y;
    smooth(0.04, 0.3, det.abs()) * smooth(0.02, 0.2, purple).max(smooth(0.02, 0.2, green))
}

/// Refine Saturation: scale the curved colour's chroma (around its luma, encoded values) so its
/// saturation (chroma / luma) moves from the curve's towards the pre-curve one: the ratio is
/// `(s_curve / s_before)^refine`, so 1 keeps the curve, 0 restores the original saturation.
#[inline]
pub fn refine_saturation(before: [f32; 3], after: [f32; 3], refine: f32) -> [f32; 3] {
    let luma = |e: [f32; 3]| 0.2126 * e[0] + 0.7152 * e[1] + 0.0722 * e[2];
    let chroma = |e: [f32; 3]| e[0].max(e[1]).max(e[2]) - e[0].min(e[1]).min(e[2]);
    let (y0, y1) = (luma(before), luma(after));
    let s0 = chroma(before) / y0.max(1e-4);
    let s1 = chroma(after) / y1.max(1e-4);
    if s1 <= 1e-6 || s0 <= 1e-6 {
        return after;
    }
    let k = (s0 / s1).powf(1.0 - refine).clamp(0.0, 4.0);
    after.map(|v| y1 + (v - y1) * k)
}

pub const SRGB_LUT_N: usize = 4096;

/// Linear → sRGB-encoded table (`SRGB_LUT_N + 1` entries over 0..1), interpolated linearly.
pub fn srgb_lut() -> &'static [f32; SRGB_LUT_N + 1] {
    static LUT: std::sync::OnceLock<Box<[f32; SRGB_LUT_N + 1]>> = std::sync::OnceLock::new();
    LUT.get_or_init(|| {
        let mut t = Box::new([0.0f32; SRGB_LUT_N + 1]);
        for (i, v) in t.iter_mut().enumerate() {
            *v = linear_to_srgb(i as f32 / SRGB_LUT_N as f32);
        }
        t
    })
}

/// Linear → sRGB-encoded (clamped to 0..1) by an interpolated table: within 2e-5 of the exact
/// curve (≪ one 8-bit or 10-bit step), several times faster than `powf`.
#[inline]
fn encode_srgb(lut: &[f32; SRGB_LUT_N + 1], v: f32) -> f32 {
    let f = v.clamp(0.0, 1.0) * SRGB_LUT_N as f32;
    let i = (f as usize).min(SRGB_LUT_N - 1);
    let t = f - i as f32;
    lut[i] + (lut[i + 1] - lut[i]) * t
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refine_saturation_restores_the_pre_curve_saturation() {
        let before = [0.5, 0.3, 0.2];
        let after = [0.7, 0.35, 0.15]; // a contrasty curve: more saturated
        assert_eq!(refine_saturation(before, after, 1.0), after);
        let r = refine_saturation(before, after, 0.0);
        let sat = |e: [f32; 3]| (e[0] - e[2]) / (0.2126 * e[0] + 0.7152 * e[1] + 0.0722 * e[2]);
        assert!((sat(r) - sat(before)).abs() < 1e-4, "{r:?}");
        let half = refine_saturation(before, after, 0.5);
        assert!(sat(half) > sat(before) && sat(half) < sat(after));
        // luma is kept
        let y = |e: [f32; 3]| 0.2126 * e[0] + 0.7152 * e[1] + 0.0722 * e[2];
        assert!((y(r) - y(after)).abs() < 1e-5);
    }

    #[test]
    fn srgb_table_matches_the_exact_curve() {
        let lut = srgb_lut();
        let mut worst = 0.0f32;
        for i in 0..=200_000 {
            // dense near black, where the curve bends most
            let v = (i as f32 / 200_000.0).powi(3);
            worst = worst.max((encode_srgb(lut, v) - linear_to_srgb(v)).abs());
        }
        assert!(worst < 2e-5, "{worst}");
        assert_eq!(encode_srgb(lut, -1.0), 0.0);
        assert!((encode_srgb(lut, 2.0) - 1.0).abs() < 1e-6);
    }
}

#[cfg(test)]
mod colour_tone_tests {
    use super::*;

    #[test]
    fn dehaze_unit_transmission_preserves_negative_scene_channels() {
        // Sky's blurred alpha can have tiny signed residues outside its support. Once
        // transmission rounds to 1, dehaze must be an identity even on negative
        // resampling/profile channels, which the tone stage desaturates later.
        for c in [[-0.004_114_710_3, 0.001_321_920_3, 0.004_300_609_7], [-0.1, 0.2, 0.3], [1.0, -0.2, 0.1]] {
            for dz in [0.0, f32::MIN_POSITIVE, 1e-14, 1e-8, f32::EPSILON / 4.0] {
                assert_eq!(dehaze_px(c, dz, 1.0, 1.0, 1.0), c, "{c:?}, strength {dz}");
            }
            assert_eq!(dehaze_px(c, 0.4, 0.0, 1.0, 1.0), c);
        }
        // Actual veil removal still follows the existing dehaze formula.
        let c = [-0.1, 0.2, 0.3];
        let t = 1.0 - 0.95 * 0.4 * 0.5;
        assert_eq!(dehaze_px(c, 0.4, 0.5, 1.0, 1.0), c.map(|v| ((v - (1.0 - t)) / t).max(0.0)));
    }

    #[test]
    fn cpu_shape_roundoff_does_not_clip_unselected_scene_channels() {
        use lightcraft_develop::{LocalAdjustments, Mask, MaskComponent, MaskOp, MaskShape};
        use std::sync::Arc;

        // The resampled scene and masked adjustments from GPU equivalence's
        // "masks (cpu shapes, ops, amount)" failure. A small log-luminance change
        // models CPU/GPU rounding before the CPU-evaluated Sky/Background shapes.
        let src = lightcraft_scenes::demo_library()[0].render(960, 640);
        let info = SourceInfo { raw: true, ..Default::default() };
        let s = DevelopSettings {
            masks: vec![
                Mask {
                    components: vec![MaskComponent { name: None, op: MaskOp::Add, invert: false, shape: MaskShape::Sky { seg: None } }],
                    adjust: LocalAdjustments { exposure: -0.5, dehaze: 40.0, amount: 70.0, ..Default::default() },
                    ..Default::default()
                },
                Mask {
                    components: vec![MaskComponent { name: None, op: MaskOp::Add, invert: true, shape: MaskShape::Background { seg: None } }],
                    adjust: LocalAdjustments { saturation: -60.0, ..Default::default() },
                    invert: true,
                    ..Default::default()
                },
            ],
            ..Default::default()
        };
        let req = crate::RenderRequest::fit(720, 720);
        let plan = crate::plan(&src, &info, &s, &req);
        let img = Arc::new(plan.frame.sample(&src, plan.w, plan.h));
        assert!(img.data.iter().any(|c| c.iter().any(|v| *v < 0.0)), "fixture must include negative resampling channels");
        let mut prep = crate::local::prepare(
            img.clone(),
            &s,
            &info,
            &plan.frame,
            plan.px_per_long,
            plan.src_long,
            req.quality,
            &mut crate::local::Planes::default(),
        );
        let reference = finish(&prep, &s, &plan.frame, &info, req.space, None);
        let alpha = prep.masks[0].alpha.clone();
        for delta in [-1e-6, 1e-6] {
            let log_l = prep.log_l.map(|v| v + delta);
            prep.masks = crate::masks::evaluate(&s.masks, &plan.frame, plan.w, plan.h, &img, &log_l, 0.0);
            assert!(
                alpha
                    .data
                    .iter()
                    .zip(&prep.masks[0].alpha.data)
                    .zip(&img.data)
                    .any(|((a, b), c)| { (*a > 0.0) != (*b > 0.0) && a.abs().max(b.abs()) < 1e-8 && c.iter().any(|v| *v < 0.0) }),
                "fixture must flip tiny alpha residues on negative scene colours"
            );
            let actual = finish(&prep, &s, &plan.frame, &info, req.space, None);
            let max = reference.data.iter().zip(&actual.data).flat_map(|(a, b)| (0..3).map(move |k| a[k].abs_diff(b[k]))).max().unwrap();
            assert!(max <= 1, "mask log-luminance perturbation {delta}: {max} LSB");
        }
    }

    #[test]
    fn dither_is_deterministic_neutral_unbiased_and_keeps_endpoints() {
        let mut sum = 0u64;
        let mut lo = 255u8;
        let mut hi = 0u8;
        for y in 0..128 {
            for x in 0..128 {
                assert_eq!(dither8(0., x, y, 0), 0);
                assert_eq!(dither8(1., x, y, 0), 255);
                let v = dither8(0.501, x, y, 0);
                assert_eq!(v, dither8(0.501, x, y, 1));
                assert_eq!(v, dither8(0.501, x, y, 2));
                assert_eq!(v, dither8(0.501, x, y, 0));
                sum += u64::from(v);
                lo = lo.min(v);
                hi = hi.max(v);
            }
        }
        assert!((sum as f64 / 16384. - 0.501 * 255.).abs() < 0.02);
        assert!(hi > lo && hi - lo <= 2, "TPDF quantization support: {lo}..{hi}");
    }
    #[test]
    fn soft_gamut_is_identity_interior_monotone_and_continuous() {
        assert_eq!(soft_gamut([0.3; 3]), [0.3; 3]);
        assert_eq!(soft_gamut([0.5, 0.2, 0.3]), [0.5, 0.2, 0.3]);
        let mut previous = 1.0f32;
        for i in 0..3000 {
            let d = i as f32 / 1000.;
            let mapped = soft_gamut([1., 1. - d, 0.8]);
            assert!(mapped.iter().all(|v| v.is_finite()));
            assert!(mapped[1] <= previous + 1e-6);
            assert!((mapped[1] - previous).abs() < 0.0011);
            previous = mapped[1];
        }
        let e = 1e-5;
        assert!((soft_gamut([1., 0.2 - e, 0.8])[1] - soft_gamut([1., 0.2 + e, 0.8])[1]).abs() < 3e-5);
    }
    #[test]
    fn luminance_master_keeps_channel_ratios_and_rgb_mode_refines_saturation() {
        use lightcraft_geom::Point;
        let mut curve = ToneCurve { master: vec![Point::new(0., 0.), Point::new(0.4, 0.5), Point::new(1., 1.)], ..Default::default() };
        let c = [0.16, 0.08, 0.04];
        let t = curve_luts(&curve).unwrap();
        let a = t.apply(c, 1.);
        assert!((a[0] / a[1] - c[0] / c[1]).abs() < 1e-5);
        assert!((a[1] / a[2] - c[1] / c[2]).abs() < 1e-5);
        assert_eq!(t.apply(c, 0.), a, "luminance curve already preserves chroma");
        curve.mode = lightcraft_develop::CurveMode::Rgb;
        let t = curve_luts(&curve).unwrap();
        assert_ne!(t.apply(c, 0.), t.apply(c, 1.));
    }
}
