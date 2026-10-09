//! Perceptual colour spaces of the Develop colour tools: Filmlight Yrg (Kirk 2019, through CIE 2006
//! LMS) and darktable UCS 22 (Aurélien Pierre: lightness L*, colourfulness, JCH / HSB / HCB), with
//! the gamut boundary of the working space (linear Rec.2020, D65) as colourfulness by hue.
//!
//! Ported from darktable's `src/common/colorspaces_inline_conversions.h` (`XYZ_to_LMS`,
//! `LMS_to_Yrg`, `Yrg_to_Ych`, `gamut_check_Yrg`, `Y_to_dt_UCS_L_star`, `xyY_to_dt_UCS_UV`,
//! `dt_UCS_JCH_to_xyY`, `dt_UCS_JCH_to_HSB` …) and `src/common/darktable_ucs_22_helpers.h`
//! (`dt_UCS_22_build_gamut_LUT`, `lookup_gamut`, `soft_clip`, `gamut_map_HSB`); GPL-3.0-or-later,
//! see `docs/PORTS.md`. Differences: the working space is D65 already (no D50 → D65 adaptation),
//! the gamut LUT is built once for Rec.2020, and the inverse UV → xy step is exposed for tools that
//! edit chromaticity directly (Skin Tone).

use std::sync::OnceLock;

use lightcraft_color::REC2020;

pub type M3 = [[f32; 3]; 3];

#[inline]
pub fn mul(m: &M3, v: [f32; 3]) -> [f32; 3] {
    [
        m[0][0] * v[0] + m[0][1] * v[1] + m[0][2] * v[2],
        m[1][0] * v[0] + m[1][1] * v[1] + m[1][2] * v[2],
        m[2][0] * v[0] + m[2][1] * v[1] + m[2][2] * v[2],
    ]
}

fn mm(a: &M3, b: &M3) -> M3 {
    std::array::from_fn(|r| std::array::from_fn(|c| a[r][0] * b[0][c] + a[r][1] * b[1][c] + a[r][2] * b[2][c]))
}

/// CIE 1931 XYZ (D65) → CIE 2006 LMS (Kirk's approximation).
pub const XYZ_TO_LMS: M3 = [[0.257085, 0.859943, -0.031061], [-0.394427, 1.175800, 0.106423], [0.064856, -0.076250, 0.559067]];
pub const LMS_TO_XYZ: M3 = [[1.80794659, -1.29971660, 0.34785879], [0.61783960, 0.39595453, -0.04104687], [-0.12546960, 0.20478038, 1.74274183]];
/// Filmlight grading RGB ↔ LMS (D65).
pub const GRADING_TO_LMS: M3 = [[0.95, 0.38, 0.00], [0.05, 0.62, 0.03], [0.00, 0.00, 0.97]];
pub const LMS_TO_GRADING: M3 = [[1.08771930, -0.66666667, 0.02061856], [-0.0877193, 1.66666667, -0.05154639], [0.0, 0.0, 1.03092784]];

/// The working space's conversion matrices (linear Rec.2020 ↔ XYZ D65 ↔ LMS 2006).
pub struct Mats {
    pub rgb_to_xyz: M3,
    pub xyz_to_rgb: M3,
    pub rgb_to_lms: M3,
    pub lms_to_rgb: M3,
}

pub fn mats() -> &'static Mats {
    static M: OnceLock<Mats> = OnceLock::new();
    M.get_or_init(|| {
        let rgb_to_xyz = REC2020.to_xyz().to_f32();
        let xyz_to_rgb = REC2020.from_xyz().to_f32();
        Mats { rgb_to_xyz, xyz_to_rgb, rgb_to_lms: mm(&XYZ_TO_LMS, &rgb_to_xyz), lms_to_rgb: mm(&xyz_to_rgb, &LMS_TO_XYZ) }
    })
}

// ------------------------------------------------------------------------------------------
// Filmlight Yrg

/// The r, g coordinates of D65 white in Yrg.
pub const YRG_WHITE: [f32; 2] = [0.21902143, 0.54371398];

#[inline]
pub fn lms_to_yrg(lms: [f32; 3]) -> [f32; 3] {
    let y = 0.68990272 * lms[0] + 0.34832189 * lms[1];
    let a = lms[0] + lms[1] + lms[2];
    let n = if a == 0.0 { [0.0; 3] } else { lms.map(|v| v / a) };
    let rgb = mul(&LMS_TO_GRADING, n);
    [y, rgb[0], rgb[1]]
}

#[inline]
pub fn yrg_to_lms(yrg: [f32; 3]) -> [f32; 3] {
    let rgb = [yrg[1], yrg[2], 1.0 - yrg[1] - yrg[2]];
    let lms = mul(&GRADING_TO_LMS, rgb);
    let denom = 0.68990272 * lms[0] + 0.34832189 * lms[1];
    let a = if denom == 0.0 { 0.0 } else { yrg[0] / denom };
    lms.map(|v| v * a)
}

/// Yrg → (Y, chroma, cos h, sin h).
#[inline]
pub fn yrg_to_ych(yrg: [f32; 3]) -> [f32; 4] {
    let r = yrg[1] - YRG_WHITE[0];
    let g = yrg[2] - YRG_WHITE[1];
    let c = (g * g + r * r).sqrt();
    let (co, si) = if c != 0.0 { (r / c, g / c) } else { (1.0, 0.0) };
    [yrg[0], c, co, si]
}

#[inline]
pub fn ych_to_yrg(ych: [f32; 4]) -> [f32; 3] {
    [ych[0], ych[1] * ych[2] + YRG_WHITE[0], ych[1] * ych[3] + YRG_WHITE[1]]
}

/// Clip the chroma of `ych` (at constant hue and luminance) so it stays inside Yrg.
#[inline]
pub fn gamut_check_yrg(ych: &mut [f32; 4]) {
    let yrg = ych_to_yrg(*ych);
    let (co, si) = (ych[2], ych[3]);
    let mut max_c = ych[1];
    if yrg[1] < 0.0 {
        max_c = max_c.min(-YRG_WHITE[0] / co);
    }
    if yrg[2] < 0.0 {
        max_c = max_c.min(-YRG_WHITE[1] / si);
    }
    if yrg[1] + yrg[2] > 1.0 {
        max_c = max_c.min((1.0 - YRG_WHITE[0] - YRG_WHITE[1]) / (co + si));
    }
    ych[1] = max_c;
}

/// Grading RGB of a (Y, chroma, hue in radians) triplet.
pub fn ych_to_grading(y: f32, c: f32, h: f32) -> [f32; 3] {
    let lms = yrg_to_lms(ych_to_yrg([y, c, h.cos(), h.sin()]));
    mul(&LMS_TO_GRADING, lms)
}

// ------------------------------------------------------------------------------------------
// darktable UCS 22

pub const L_STAR_RANGE: f32 = 2.098_883_8;
pub const L_STAR_UPPER: f32 = 2.09885;

#[inline]
pub fn y_to_l_star(y: f32) -> f32 {
    let yh = y.max(0.0).powf(0.631_651_35);
    L_STAR_RANGE * yh / (yh + 1.124_267_7)
}

#[inline]
pub fn l_star_to_y(l: f32) -> f32 {
    (1.124_267_7 * l / (L_STAR_RANGE - l)).powf(1.583_151_9)
}

/// CIE xy (D65) → darktable UCS U*′V*′.
#[inline]
pub fn xy_to_uv(x: f32, y: f32) -> [f32; 2] {
    let u = -0.783_941 * x + 0.277_513 * y + 0.153_836_58;
    let v = 0.745_273_54 * x - 0.205_375_87 * y - 0.165_478_38;
    let d = 0.318_707_28 * x + 2.167_436_9 * y + 0.291_320_55;
    let div = if d >= 0.0 { d.max(f32::MIN_POSITIVE) } else { d.min(-f32::MIN_POSITIVE) };
    let (u, v) = (u / div, v / div);
    let us = 1.396_562_3 * u / (u.abs() + 1.492_173_5);
    let vs = 1.451_395_4 * v / (v.abs() + 1.524_886_4);
    [-1.124_983_9 * us - 0.980_483_7 * vs, 1.863_233_2 * us + 1.971_853_1 * vs]
}

/// Inverse of [`xy_to_uv`].
#[inline]
pub fn uv_to_xy(uv: [f32; 2]) -> [f32; 2] {
    let us = -5.037_522_4 * uv[0] - 2.504_856_3 * uv[1];
    let vs = 4.760_029_4 * uv[0] + 2.874_013 * uv[1];
    let u = -1.492_173_5 * us / (us.abs() - 1.396_562_3);
    let v = -1.524_886_4 * vs / (vs.abs() - 1.451_395_4);
    let x = 0.167_171_47 * u + 0.141_299_8 * v - 0.008_015_313;
    let y = -0.150_959_09 * u - 0.155_185_06 * v - 0.008_433_124;
    let d = 0.940_254_74 * u + 1.0 * v - 0.025_632_597;
    let div = if d >= 0.0 { d.max(f32::MIN_POSITIVE) } else { d.min(-f32::MIN_POSITIVE) };
    [x / div, y / div]
}

/// xyY (D65) of an XYZ colour (negative XYZ clipped; black takes the white's chromaticity).
#[inline]
pub fn xyz_to_xyy(xyz: [f32; 3]) -> [f32; 3] {
    let v = xyz.map(|c| c.max(0.0));
    let s = v[0] + v[1] + v[2];
    if s > 0.0 { [v[0] / s, v[1] / s, v[1]] } else { [0.312_71, 0.329_02, v[1]] }
}

#[inline]
pub fn xyy_to_xyz(xyy: [f32; 3]) -> [f32; 3] {
    if xyy[1] == 0.0 {
        return [0.0; 3];
    }
    [xyy[2] * xyy[0] / xyy[1], xyy[2], xyy[2] * (1.0 - xyy[0] - xyy[1]) / xyy[1]]
}

/// (L*, U*′V*′) → JCH for the white's lightness `l_white`.
#[inline]
pub fn luv_to_jch(l_star: f32, l_white: f32, uv: [f32; 2]) -> [f32; 3] {
    let m2 = uv[0] * uv[0] + uv[1] * uv[1];
    [l_star / l_white, 15.932_994 * l_star.max(0.0).powf(0.652_399_75) * m2.powf(0.600_755_7) / l_white, uv[1].atan2(uv[0])]
}

#[inline]
pub fn xyy_to_jch(xyy: [f32; 3], l_white: f32) -> [f32; 3] {
    luv_to_jch(y_to_l_star(xyy[2]), l_white, xy_to_uv(xyy[0], xyy[1]))
}

#[inline]
pub fn jch_to_xyy(jch: [f32; 3], l_white: f32) -> [f32; 3] {
    let l = (jch[0] * l_white).clamp(0.0, L_STAR_UPPER);
    let m = if l != 0.0 { (jch[1] * l_white / (15.932_994 * l.powf(0.652_399_75))).max(0.0).powf(0.832_285_07) } else { 0.0 };
    let xy = uv_to_xy([m * jch[2].cos(), m * jch[2].sin()]);
    [xy[0], xy[1], l_star_to_y(l)]
}

#[inline]
pub fn jch_to_hsb(jch: [f32; 3]) -> [f32; 3] {
    let b = jch[0] * (jch[1].max(0.0).powf(1.336_542_2) + 1.0);
    [jch[2], if b > 0.0 { jch[1] / b } else { 0.0 }, b]
}

#[inline]
pub fn hsb_to_jch(hsb: [f32; 3]) -> [f32; 3] {
    let c = hsb[1] * hsb[2];
    [hsb[2] / (c.max(0.0).powf(1.336_542_2) + 1.0), c, hsb[0]]
}

#[inline]
pub fn jch_to_hcb(jch: [f32; 3]) -> [f32; 3] {
    [jch[2], jch[1], jch[0] * (jch[1].max(0.0).powf(1.336_542_2) + 1.0)]
}

#[inline]
pub fn hcb_to_jch(hcb: [f32; 3]) -> [f32; 3] {
    [hcb[2] / (hcb[1].max(0.0).powf(1.336_542_2) + 1.0), hcb[1], hcb[0]]
}

/// Exponential soft clip of `x` above `soft` towards `hard`.
#[inline]
pub fn soft_clip(x: f32, soft: f32, hard: f32) -> f32 {
    let norm = hard - soft;
    if x > soft { soft + (1.0 - (-(x - soft) / norm).exp()) * norm } else { x }
}

/// Entries of the gamut LUT over hue −π … π.
pub const GAMUT_N: usize = 512;

/// darktable's `dt_UCS_22_build_gamut_LUT` for linear Rec.2020: the squared colourfulness M² of
/// the gamut boundary (the primaries' triangle in xy) by UCS hue.
pub fn gamut_lut() -> &'static [f32; GAMUT_N] {
    static L: OnceLock<[f32; GAMUT_N]> = OnceLock::new();
    L.get_or_init(|| {
        let m = &mats().rgb_to_xyz;
        let prim = |i: usize| {
            let mut e = [0.0f32; 3];
            e[i] = 1.0;
            xyz_to_xyy(mul(m, e))
        };
        let (r, g, b) = (prim(0), prim(1), prim(2));
        let white = [0.312_71f32, 0.329_02f32];
        let dh = |a: f32, b: f32| {
            let mut d = a - b;
            if d < -std::f32::consts::PI {
                d += std::f32::consts::TAU;
            }
            if d > std::f32::consts::PI {
                d -= std::f32::consts::TAU;
            }
            d
        };
        let ang = |p: [f32; 3]| (p[1] - white[1]).atan2(p[0] - white[0]);
        let (hr, hg, hb) = (ang(r), ang(g), ang(b));
        let mut lut = [0.0f64; GAMUT_N];
        let mut cnt = [0.0f64; GAMUT_N];
        let steps = 50 * GAMUT_N;
        for i in 0..steps {
            let a = -std::f32::consts::PI + i as f32 / steps as f32 * std::f32::consts::TAU;
            let t = a.tan();
            let seg = |p0: [f32; 3], p1: [f32; 3]| {
                let tt = (white[1] - p0[1] + t * (p0[0] - white[0])) / (p1[1] - p0[1] + t * (p0[0] - p1[0]));
                [p0[0] + tt * (p1[0] - p0[0]), p0[1] + tt * (p1[1] - p0[1])]
            };
            let (t1, t2, t3) = (dh(a, hb) / dh(hr, hb), dh(a, hr) / dh(hg, hr), dh(a, hg) / dh(hb, hg));
            let xy = if (0.0..=1.0).contains(&t1) {
                seg(b, r)
            } else if (0.0..=1.0).contains(&t2) {
                seg(r, g)
            } else if (0.0..=1.0).contains(&t3) {
                seg(g, b)
            } else {
                continue;
            };
            let uv = xy_to_uv(xy[0], xy[1]);
            let hue = uv[1].atan2(uv[0]);
            let mut k = ((GAMUT_N - 1) as f32 * (hue + std::f32::consts::PI) / std::f32::consts::TAU).round() as i64;
            k = k.rem_euclid(GAMUT_N as i64);
            lut[k as usize] += (uv[0] * uv[0] + uv[1] * uv[1]) as f64;
            cnt[k as usize] += 1.0;
        }
        // fill the (rare) empty bins from their neighbours so the lookup never reads 0
        let mut out = [0.0f32; GAMUT_N];
        for k in 0..GAMUT_N {
            out[k] = (lut[k] / cnt[k].max(1.0)) as f32;
        }
        for _ in 0..4 {
            for k in 0..GAMUT_N {
                if cnt[k] == 0.0 {
                    out[k] = 0.5 * (out[(k + GAMUT_N - 1) % GAMUT_N] + out[(k + 1) % GAMUT_N]);
                }
            }
        }
        out
    })
}

/// darktable's `lookup_gamut`: the LUT linearly interpolated at `hue` (radians, −π … π).
#[inline]
pub fn lookup_gamut(lut: &[f32], hue: f32) -> f32 {
    let n = lut.len();
    let x = n as f32 * (hue + std::f32::consts::PI) / std::f32::consts::TAU;
    let x0 = x.floor();
    let i = (x0 as i64).rem_euclid(n as i64) as usize;
    let j = ((x.ceil()) as i64).rem_euclid(n as i64) as usize;
    let y0 = lut[i];
    if i != j { y0 + (x - x0) * (lut[j] - y0) } else { y0 }
}

/// darktable's `gamut_map_HSB`: soft-clip the saturation at constant brightness so the colour
/// fits Rec.2020.
#[inline]
pub fn gamut_map_hsb(hsb: &mut [f32; 3], l_white: f32) {
    gamut_map_hsb_with(hsb, l_white, gamut_lut());
}
pub fn gamut_map_hsb_with(hsb: &mut [f32; 3], l_white: f32, gamut: &[f32; 512]) {
    let jch = hsb_to_jch(*hsb);
    let max_m2 = lookup_gamut(gamut, jch[2]);
    let max_c = 15.932_994 * (jch[0] * l_white).max(0.0).powf(0.652_399_75) * max_m2.powf(0.600_755_7) / l_white;
    let b = jch_to_hsb([jch[0], max_c, jch[2]]);
    hsb[1] = soft_clip(hsb[1], 0.8 * b[1], b[1]);
}

/// Linear Rec.2020 → (UCS HSB, L_white) helpers for whole-pixel conversions.
#[inline]
pub fn rgb_to_hsb(rgb: [f32; 3], l_white: f32) -> [f32; 3] {
    let xyy = xyz_to_xyy(mul(&mats().rgb_to_xyz, rgb));
    jch_to_hsb(xyy_to_jch(xyy, l_white))
}

#[inline]
pub fn hsb_to_rgb(hsb: [f32; 3], l_white: f32) -> [f32; 3] {
    let xyy = jch_to_xyy(hsb_to_jch(hsb), l_white);
    mul(&mats().xyz_to_rgb, xyy_to_xyz(xyy))
}

/// UCS hue (radians) of a pure sRGB colour with HSV hue `deg`.
pub fn ucs_hue_of_srgb_hue(deg: f64) -> f32 {
    use lightcraft_color::{SRGB, perceptual::hsv_to_rgb};
    let c = hsv_to_rgb(deg as f32, 1.0, 1.0).map(lightcraft_color::transfer::srgb_to_linear);
    let m = SRGB.to_space(&REC2020).apply_f32(c);
    let xyy = xyz_to_xyy(mul(&mats().rgb_to_xyz, m));
    let uv = xy_to_uv(xyy[0], xyy[1]);
    uv[1].atan2(uv[0])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips() {
        for c in [[0.2f32, 0.3, 0.4], [0.8, 0.1, 0.05], [0.02, 0.5, 0.1], [0.18, 0.18, 0.18]] {
            let lms = mul(&mats().rgb_to_lms, c);
            let back = mul(&mats().lms_to_rgb, yrg_to_lms(ych_to_yrg(yrg_to_ych(lms_to_yrg(lms)))));
            assert!(c.iter().zip(back).all(|(a, b)| (a - b).abs() < 1e-4), "{c:?} {back:?}");
            let lw = y_to_l_star(1.0);
            let h = rgb_to_hsb(c, lw);
            let back = hsb_to_rgb(h, lw);
            assert!(c.iter().zip(back).all(|(a, b)| (a - b).abs() < 2e-3), "{c:?} {back:?}");
        }
        let uv = xy_to_uv(0.4, 0.35);
        let xy = uv_to_xy(uv);
        assert!((xy[0] - 0.4).abs() < 1e-4 && (xy[1] - 0.35).abs() < 1e-4, "{xy:?}");
    }

    #[test]
    fn gamut_lut_is_positive() {
        let l = gamut_lut();
        assert!(l.iter().all(|v| *v > 0.0 && v.is_finite()));
        // the primaries are on the boundary: their saturation is the LUT's
        let lw = y_to_l_star(1.0);
        let mut hsb = rgb_to_hsb([0.2, 0.0, 0.0], lw);
        let s0 = hsb[1];
        hsb[1] *= 3.0;
        gamut_map_hsb(&mut hsb, lw);
        assert!(hsb[1] < s0 * 3.0 && hsb[1] > s0 * 0.7, "{s0} {}", hsb[1]);
    }
}
