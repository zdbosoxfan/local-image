//! darktable color balance RGB: Yrg grading, UCS22 perceptual saturation, alternate
//! JzAzBz saturation, four luminance-masked wheels. GPL-3.0-or-later, 733bd69f.
//! The LR slider → upstream parameter mapping is Local Image's calibration.
use crate::ucs::{self, mul};
use lightcraft_develop::DevelopSettings;
use std::sync::OnceLock;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Formula {
    Ucs,
    JzAzBz,
}
#[derive(Clone, Debug)]
pub struct Balance {
    pub vibrance: f32,
    pub chroma: f32,
    pub saturation: f32,
    pub chroma_masks: [f32; 3],
    pub saturation_masks: [f32; 3],
    pub brilliance_masks: [f32; 3],
    pub brilliance: f32,
    pub hue_angle: f32,
    pub global: [f32; 3],
    pub shadows: [f32; 3],
    pub highlights: [f32; 3],
    pub midtones: [f32; 3],
    pub midtones_y: f32,
    pub white: f32,
    pub grey: f32,
    pub contrast: f32,
    pub shadows_weight: f32,
    pub highlights_weight: f32,
    pub midtones_weight: f32,
    pub mask_grey: f32,
    pub formula: Formula,
}
impl Balance {
    pub fn new(s: &DevelopSettings) -> Self {
        let sat = s.color.saturation as f32 / 100.;
        let g = &s.grading;
        let norm = ucs::ych_to_grading(1., 0., 0.);
        // LR wheel hues are actual sRGB colours, converted to the upstream Yrg hue coordinates.
        let wheel = |w: &lightcraft_develop::Wheel, scale: f32| {
            let c = lightcraft_color::SRGB
                .to_space(&lightcraft_color::REC2020)
                .apply_f32(lightcraft_color::perceptual::hsv_to_rgb(w.hue as f32, 1., 1.).map(lightcraft_color::transfer::srgb_to_linear));
            let ych = ucs::yrg_to_ych(ucs::lms_to_yrg(mul(&ucs::mats().rgb_to_lms, c)));
            ucs::ych_to_grading(1., w.sat as f32 / 100. * scale, ych[3].atan2(ych[2]))
        };
        let (gw, sw, hw, mw) = (wheel(&g.global, 0.015), wheel(&g.shadows, 0.12), wheel(&g.highlights, 0.08), wheel(&g.midtones, 0.06));
        let weight = 2. + (1. - g.blending as f32 / 100.) * 4.;
        Self {
            vibrance: s.color.vibrance as f32 / 100.,
            chroma: if sat < 0. { sat } else { sat * 0.25 },
            saturation: sat.max(0.) * 0.75,
            chroma_masks: [0.; 3],
            saturation_masks: [0.; 3],
            brilliance_masks: [0.; 3],
            brilliance: 0.,
            hue_angle: 0.,
            global: std::array::from_fn(|i| gw[i] - norm[i] + norm[i] * g.global.lum as f32 / 100. * 0.04),
            shadows: std::array::from_fn(|i| 1. + (sw[i] - norm[i]) + g.shadows.lum as f32 / 100. * 0.5),
            highlights: std::array::from_fn(|i| 1. + (hw[i] - norm[i]) + g.highlights.lum as f32 / 100. * 0.3),
            midtones: std::array::from_fn(|i| 1. / (1. + (mw[i] - norm[i]))),
            midtones_y: 1. / (1. + g.midtones.lum as f32 / 100. * 0.2),
            white: 1.,
            grey: 0.18,
            contrast: 1.,
            shadows_weight: weight,
            highlights_weight: weight,
            midtones_weight: weight * weight / 2.,
            mask_grey: (0.18 * (g.balance as f32 / 100.).exp2()).powf(0.410_120_58),
            formula: Formula::Ucs,
        }
    }
    pub fn is_identity(&self) -> bool {
        self.chroma_masks == [0.; 3]
            && self.saturation_masks == [0.; 3]
            && self.brilliance_masks == [0.; 3]
            && self.brilliance == 0.
            && self.hue_angle == 0.
            && self.vibrance == 0.
            && self.chroma == 0.
            && self.saturation == 0.
            && self.global == [0.; 3]
            && self.shadows == [1.; 3]
            && self.highlights == [1.; 3]
            && self.midtones == [1.; 3]
            && self.midtones_y == 1.
            && self.contrast == 1.
    }
    pub fn apply(&self, c: [f32; 3]) -> [f32; 3] {
        self.apply_with_gamut(
            c,
            match self.formula {
                Formula::Ucs => ucs::gamut_lut(),
                Formula::JzAzBz => jz_gamut(),
            },
        )
    }
    pub fn apply_with_gamut(&self, c: [f32; 3], gamut: &[f32; 512]) -> [f32; 3] {
        if self.is_identity() {
            return c;
        }
        let lms = mul(&ucs::mats().rgb_to_lms, c.map(|v| v.max(0.)));
        let mut ych = ucs::yrg_to_ych(ucs::lms_to_yrg(lms));
        ych[0] = ych[0].max(0.);
        let [alpha, gamma, beta] =
            opacity_masks(ych[0].powf(0.410_120_58), self.shadows_weight, self.highlights_weight, self.midtones_weight, self.mask_grey);
        let dot = |v: [f32; 3]| alpha * v[0] + gamma * v[1] + beta * v[2];
        let (sn, co) = self.hue_angle.sin_cos();
        let (hc, hs) = (ych[2], ych[3]);
        ych[2] = co * hc - sn * hs;
        ych[3] = sn * hc + co * hs;
        let saturation = self.saturation + dot(self.saturation_masks);
        let brilliance = (1. + self.brilliance + dot(self.brilliance_masks)).max(0.);
        let vibrance = self.vibrance * (1. - ych[1].powf(self.vibrance.abs()));
        ych[1] *= (1. + self.chroma + dot(self.chroma_masks) + vibrance).max(0.);
        ucs::gamut_check_yrg(&mut ych);
        let mut rgb = mul(&ucs::LMS_TO_GRADING, ucs::yrg_to_lms(ucs::ych_to_yrg(ych)));
        for (i, v) in rgb.iter_mut().enumerate() {
            *v += self.global[i];
            *v *= (1. - beta) * ((1. - alpha) + alpha * self.shadows[i]) + beta * self.highlights[i];
            *v = vector_pow(v.abs() / self.white, self.midtones[i]) * (if *v < 0. { -1. } else { 1. }) * self.white;
        }
        let mut yrg = ucs::lms_to_yrg(mul(&ucs::GRADING_TO_LMS, rgb));
        yrg[0] = (yrg[0] / self.white).max(0.).powf(self.midtones_y) * self.white;
        yrg[0] = self.grey * (yrg[0] / self.grey).powf(self.contrast);
        let xyz = mul(&ucs::LMS_TO_XYZ, ucs::yrg_to_lms(yrg));
        let xyz = match self.formula {
            Formula::Ucs => {
                let lw = ucs::y_to_l_star(self.white);
                let mut hcb = ucs::jch_to_hcb(ucs::xyy_to_jch(ucs::xyz_to_xyy(xyz), lw));
                let r = (hcb[1] * hcb[1] + hcb[2] * hcb[2]).sqrt();
                let (sn, co) = if r > 0. { (hcb[1] / r, hcb[2] / r) } else { (0., 0.) };
                let p = hcb[1].max(f32::MIN_POSITIVE);
                let w = sn * hcb[1] + co * hcb[2];
                let max_a = (p * p + w * w).sqrt() / p;
                let a = ucs::soft_clip((1. + saturation).max(0.), 0.5 * max_a, max_a);
                let pp = (a - 1.) * p;
                let wp = (p * p * (1. - a * a) + w * w).max(0.).sqrt() * brilliance;
                hcb[1] = (co * pp + sn * wp).max(0.);
                hcb[2] = (-sn * pp + co * wp).max(0.);
                let mut hsb = ucs::jch_to_hsb(ucs::hcb_to_jch(hcb));
                ucs::gamut_map_hsb_with(&mut hsb, lw, gamut);
                ucs::xyy_to_xyz(ucs::jch_to_xyy(ucs::hsb_to_jch(hsb), lw))
            }
            Formula::JzAzBz => {
                let jab = xyz_to_jz(xyz);
                let (j, c) = (jab[0], (jab[1] * jab[1] + jab[2] * jab[2]).sqrt());
                let h = jab[2].atan2(jab[1]);
                let t = c.atan2(j);
                let (sn, co) = t.sin_cos();
                let so = j * co + c * sn;
                let ortho = so * (t * saturation).clamp(-t, std::f32::consts::FRAC_PI_2 - t);
                let so = so * brilliance;
                let mut j = (so * co - ortho * sn).max(0.);
                let mut c = (so * sn + ortho * co).max(0.);
                let maxsat = ucs::lookup_gamut(gamut, h);
                let sat = if j > 0. { ucs::soft_clip(c / j, 0.8 * maxsat, maxsat) } else { maxsat };
                let mc = j * sat;
                let mj = if sat > 0. { c / sat } else { j };
                j = (j + mj) / 2.;
                c = (c + mc) / 2.;
                let iz = ((j + 1.629_55e-11) / (0.44 + 0.56 * (j + 1.629_55e-11))).max(0.);
                for (a, b) in [(0.138_605_04, 0.058_047_317), (-0.138_605_04, -0.058_047_317), (-0.096_019_24, -0.811_891_9)] {
                    let d = a * h.cos() + b * h.sin();
                    if iz + c * d < 0. {
                        c = c.min(-iz / d);
                    }
                }
                jz_to_xyz([j, c * h.cos(), c * h.sin()])
            }
        };
        mul(&ucs::mats().xyz_to_rgb, xyz).map(|v| v.max(0.))
    }
}
pub fn opacity_masks(x: f32, sw: f32, hw: f32, mw: f32, grey: f32) -> [f32; 3] {
    let offset = x - grey;
    let n = offset / grey;
    let a = 1. / (1. + (n * sw).exp());
    let b = 1. / (1. + (-n * hw).exp());
    [a, (-offset * offset * mw / 4.).exp() * (1. - a).powi(2) * (1. - b).powi(2) * 8., b]
}
/// Upstream polynomial vector pow, including bit-extracted log2 and roundf exponent.
pub fn vector_pow(x: f32, power: f32) -> f32 {
    let m = f32::from_bits((x.to_bits() & 0x007f_ffff) | 0x3f80_0000);
    let exp = ((x.to_bits() & 0x7f80_0000) >> 23) as f32 - 127.;
    let log = ((((0.059_651_55 * m - 0.465_725_63) * m + 1.481_166_5) * m - 2.520_749_6) * m + 2.888_270_4) * (m - 1.) + exp;
    let x = (log * power).clamp(-126.99999, 129.);
    let ip = (x - 0.5).round();
    let f = x - ip;
    let mult = f32::from_bits(((127 + ip as i32) as u32) << 23);
    mult * ((((0.013_534_167 * f + 0.052_011_464) * f + 0.241_442_75) * f + 0.693_003_83) * f + 1.000_002_6)
}
// f64 directly evaluates the mathematically equivalent PQ transform. This deliberately avoids
// float32 near-neutral cancellation; tested against upstream's difference-based implementation.
pub fn xyz_to_jz(x: [f32; 3]) -> [f32; 3] {
    let [x, y, z] = x.map(f64::from);
    let xyz = [1.15 * x - 0.15 * z, 0.66 * y + 0.34 * x, z];
    let matrix = [[0.41478972, 0.579999, 0.014648], [-0.20151, 1.120649, 0.0531008], [-0.0166008, 0.2648, 0.6684799]];
    let l: [f64; 3] = std::array::from_fn(|i| {
        let v = (matrix[i][0] * xyz[0] + matrix[i][1] * xyz[1] + matrix[i][2] * xyz[2]).max(0.) * 1e-4;
        let t = v.powf(0.159301758);
        ((0.8359375 + 18.8515625 * t) / (1. + 18.6875 * t)).powf(134.034375)
    });
    let iz = (l[0] + l[1]) / 2.;
    [
        (0.44 * iz / (1. - 0.56 * iz) - 1.6295499532821566e-11) as f32,
        (3.524 * (l[0] - l[1]) + 0.542708 * (l[2] - l[1])) as f32,
        (0.199076 * (l[0] - l[2]) + 1.096799 * (l[1] - l[2])) as f32,
    ]
}
pub fn jz_to_xyz(j: [f32; 3]) -> [f32; 3] {
    let iz = ((j[0] + 1.629_55e-11) / (0.44 + 0.56 * (j[0] + 1.629_55e-11))).max(0.);
    let m = [[1., 0.138_605_04, 0.058_047_317], [1., -0.138_605_04, -0.058_047_317], [1., -0.096_019_24, -0.811_891_9]];
    let l = mul(&m, [iz, j[1], j[2]]).map(|v| {
        let p = v.max(0.).powf(1. / 134.034_38);
        10000. * ((0.8359375 - p) / (18.6875 * p - 18.851562)).max(0.).powf(1. / 0.159_301_76)
    });
    let xyz = mul(
        &[[1.924_226_4, -1.004_792_3, 0.037_651_405], [0.350_316_76, 0.726_481_2, -0.065_384_425], [-0.090_982_81, -0.312_728_3, 1.522_766_6]],
        l,
    );
    let x = (xyz[0] + 0.15 * xyz[2]) / 1.15;
    [x, (xyz[1] - 0.34 * x) / 0.66, xyz[2]]
}
fn jz_gamut() -> &'static [f32; 512] {
    static LUT: OnceLock<[f32; 512]> = OnceLock::new();
    LUT.get_or_init(|| {
        let mut samples = [0.0f32; 512];
        for r in 0..92 {
            for g in 0..92 {
                for b in 0..92 {
                    let j = xyz_to_jz(mul(&ucs::mats().rgb_to_xyz, [r as f32 / 91., g as f32 / 91., b as f32 / 91.]));
                    let c = (j[1] * j[1] + j[2] * j[2]).sqrt();
                    let h = j[2].atan2(j[1]);
                    let i = (511. * (h + std::f32::consts::PI) / std::f32::consts::TAU).round() as usize % 512;
                    samples[i] = samples[i].max(if j[0] > 0. { c / j[0] } else { 0. });
                }
            }
        }
        std::array::from_fn(|i| (-2isize..=2).map(|k| samples[(i as isize + k).rem_euclid(512) as usize]).sum::<f32>() / 5.)
    })
}
