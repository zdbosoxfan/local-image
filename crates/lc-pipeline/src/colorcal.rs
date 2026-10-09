//! Colour calibration: chromatic adaptation of the scene from its illuminant to the working
//! space's white (D65), with a choice of adaptation space (CAT16, Bradford — linear or the
//! original non-linear one — or plain XYZ scaling), and a gamut compression that pulls colours
//! the adaptation pushed outside the spectral locus back towards white.
//!
//! Ported from darktable's `src/iop/channelmixerrgb.c` (`_gamut_mapping`, the adaptation part of
//! `_loop_switch`) and `src/common/chromatic_adaptation.h` (CAT16 / Bradford LMS matrices,
//! `bradford_adapt_*`, `CAT16_adapt_*`, `XYZ_adapt_*`; GPL-3.0-or-later, Aurélien Pierre and
//! the darktable developers), see `docs/PORTS.md`. Differences: the target white is D65 (this
//! pipeline's Rec.2020 working space; darktable adapts to D50), adaptation is always complete,
//! and the channel mixer part of the module is not ported. Standard illuminant chromaticities
//! are the CIE's (1931 2° observer).

use lightcraft_color::{Mat3, REC2020, Xy};

/// Space the adaptation scales the white in.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Cat {
    /// CIECAM16's CAT16 (darktable's default).
    #[default]
    Cat16,
    /// Linear Bradford (ICC v4).
    LinearBradford,
    /// The original Bradford with its non-linear blue response.
    FullBradford,
    /// von Kries scaling in XYZ.
    Xyz,
}

const BRADFORD: [[f64; 3]; 3] = [[0.8951, 0.2664, -0.1614], [-0.7502, 1.7135, 0.0367], [0.0389, -0.0685, 1.0296]];
const CAT16: [[f64; 3]; 3] = [[0.401288, 0.650173, -0.051461], [-0.250268, 1.204414, 0.045854], [-0.002079, 0.048952, 0.953127]];

impl Cat {
    fn lms(self) -> Mat3 {
        match self {
            Cat::Cat16 => Mat3(CAT16),
            Cat::LinearBradford | Cat::FullBradford => Mat3(BRADFORD),
            Cat::Xyz => Mat3::IDENTITY,
        }
    }
}

/// CIE standard illuminants (1931 2° chromaticities).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Standard {
    A,
    D50,
    D55,
    D65,
    D75,
    /// Cool white fluorescent.
    F2,
    /// Broadband daylight fluorescent.
    F7,
    /// Narrow tri-band fluorescent.
    F11,
    E,
}

impl Standard {
    pub const ALL: [Standard; 9] =
        [Standard::A, Standard::D50, Standard::D55, Standard::D65, Standard::D75, Standard::F2, Standard::F7, Standard::F11, Standard::E];

    pub fn xy(self) -> Xy {
        match self {
            Standard::A => Xy::new(0.44757, 0.40745),
            Standard::D50 => Xy::new(0.34567, 0.35850),
            Standard::D55 => Xy::new(0.33242, 0.34743),
            Standard::D65 => Xy::new(0.31271, 0.32902),
            Standard::D75 => Xy::new(0.29902, 0.31485),
            Standard::F2 => Xy::new(0.37208, 0.37529),
            Standard::F7 => Xy::new(0.31292, 0.32933),
            Standard::F11 => Xy::new(0.38052, 0.37713),
            Standard::E => Xy::new(1.0 / 3.0, 1.0 / 3.0),
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Standard::A => "A (incandescent)",
            Standard::D50 => "D50",
            Standard::D55 => "D55",
            Standard::D65 => "D65",
            Standard::D75 => "D75",
            Standard::F2 => "F2 (cool white fluorescent)",
            Standard::F7 => "F7 (daylight fluorescent)",
            Standard::F11 => "F11 (tri-band fluorescent)",
            Standard::E => "E (equal energy)",
        }
    }
}

/// CIE daylight locus chromaticity at correlated colour temperature `t` (4000–25000 K, clamped).
pub fn daylight_xy(t: f64) -> Xy {
    let t = t.clamp(4000.0, 25000.0);
    let x = if t <= 7000.0 {
        -4.6070e9 / t.powi(3) + 2.9678e6 / t.powi(2) + 0.09911e3 / t + 0.244063
    } else {
        -2.0064e9 / t.powi(3) + 1.9018e6 / t.powi(2) + 0.24748e3 / t + 0.237040
    };
    Xy::new(x, -3.0 * x * x + 2.87 * x - 0.275)
}

/// A colour calibration for one render.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ColorCal {
    /// Linear Rec.2020 → linear Rec.2020: the whole adaptation when it is linear; for the full
    /// Bradford only RGB → XYZ.
    pub matrix: [[f32; 3]; 3],
    cat: Cat,
    /// Full Bradford: the source white in Bradford LMS, its blue exponent, the D65 white in LMS.
    full: Option<([f32; 3], f32, [f32; 3])>,
    to_xyz: [[f32; 3]; 3],
    from_xyz: [[f32; 3]; 3],
    lms: [[f32; 3]; 3],
    lms_inv: [[f32; 3]; 3],
    /// Gamut compression exponent (darktable's `1 / gamut`; 0 = off) and negative clipping.
    pub compression: f32,
    pub clip: bool,
}

#[inline]
fn mul(m: &[[f32; 3]; 3], v: [f32; 3]) -> [f32; 3] {
    std::array::from_fn(|r| m[r][0] * v[0] + m[r][1] * v[1] + m[r][2] * v[2])
}

/// D65 in CIE 1976 u′v′ (the gamut compression's white).
const WHITE_UV: [f32; 2] = [0.197_83, 0.468_32];

impl ColorCal {
    /// Adapt from `illuminant` to D65 with `cat`; `gamut` 0..12 (darktable's slider: 0 = off,
    /// 1 = default), `clip` negative RGB.
    pub fn new(cat: Cat, illuminant: Xy, gamut: f64, clip: bool) -> ColorCal {
        let to_xyz = REC2020.to_xyz();
        let from_xyz = REC2020.from_xyz();
        let lms = cat.lms();
        let lms_inv = lms.inverse().unwrap_or(Mat3::IDENTITY);
        let norm = |xy: Xy| {
            let v = xy.to_xyz();
            [v[0] / v[1], 1.0, v[2] / v[1]]
        };
        let src = lms.apply(norm(illuminant));
        let dst = lms.apply(norm(REC2020.white));
        let matrix = match cat {
            Cat::FullBradford => to_xyz,
            _ => from_xyz.mul(&lms_inv).mul(&Mat3::diag(dst[0] / src[0], dst[1] / src[1], dst[2] / src[2])).mul(&lms).mul(&to_xyz),
        };
        let full = (cat == Cat::FullBradford).then(|| {
            let p = (src[2] / dst[2]).powf(0.0834) as f32;
            (src.map(|v| v as f32), p, dst.map(|v| v as f32))
        });
        ColorCal {
            matrix: matrix.to_f32(),
            cat,
            full,
            to_xyz: to_xyz.to_f32(),
            from_xyz: from_xyz.to_f32(),
            lms: lms.to_f32(),
            lms_inv: lms_inv.to_f32(),
            compression: if gamut > 0.0 { (1.0 / gamut) as f32 } else { 0.0 },
            clip,
        }
    }

    /// Whether the whole calibration is one matrix (it can then be folded into white balance).
    pub fn is_linear(&self) -> bool {
        self.full.is_none() && self.compression == 0.0 && !self.clip
    }

    /// Calibrate one scene-linear Rec.2020 pixel.
    pub fn apply(&self, c: [f32; 3]) -> [f32; 3] {
        let c = if self.clip { c.map(|v| v.max(0.0)) } else { c };
        let mut xyz = match self.full {
            None => mul(&self.to_xyz, mul(&self.matrix, c)),
            Some((src, p, dst)) => {
                let xyz = mul(&self.matrix, c);
                let y = xyz[1];
                if y.abs() < 1e-9 {
                    xyz
                } else {
                    let lms = mul(&self.lms, xyz.map(|v| v / y));
                    let mut t = [lms[0] / src[0], lms[1] / src[1], lms[2] / src[2]];
                    if t[2] > 0.0 {
                        t[2] = t[2].powf(p);
                    }
                    let out = [dst[0] * t[0], dst[1] * t[1], dst[2] * t[2]];
                    mul(&self.lms_inv, out).map(|v| v * y)
                }
            }
        };
        if self.compression > 0.0 {
            xyz = gamut_map(xyz, self.compression, self.clip);
        }
        let rgb = mul(&self.from_xyz, xyz);
        if self.clip { rgb.map(|v| v.max(0.0)) } else { rgb }
    }

    pub fn cat(&self) -> Cat {
        self.cat
    }
}

/// darktable's `_gamut_mapping` around D65: in u′v′Y, chroma moves towards white by
/// `Y·|Δuv|²` raised to `compression`, never past white; xy kept inside the triangle.
fn gamut_map(xyz: [f32; 3], compression: f32, clip: bool) -> [f32; 3] {
    let sum = xyz[0] + xyz[1] + xyz[2];
    let y = xyz[1];
    let (x0, y0) = if sum > 0.0 { (xyz[0] / sum, xyz[1] / sum) } else { (0.31271, 0.32902) };
    let d = -2.0 * x0 + 12.0 * y0 + 3.0;
    let (mut u, mut v) = (4.0 * x0 / d, 9.0 * y0 / d);
    let delta = [WHITE_UV[0] - u, WHITE_UV[1] - v];
    let big = y * (delta[0] * delta[0] + delta[1] * delta[1]);
    let corr = if big > 0.0 { big.powf(compression) } else { 0.0 };
    let toward = |val: f32, dl: f32, w: f32| {
        let t = corr * dl + val;
        if val > w { t.max(w) } else { t.min(w) }
    };
    u = toward(u, delta[0], WHITE_UV[0]);
    v = toward(v, delta[1], WHITE_UV[1]);
    // back to xy
    let dd = 6.0 * u - 16.0 * v + 12.0;
    let (mut x, mut yy) = (9.0 * u / dd, 4.0 * v / dd);
    if clip {
        x = x.max(0.0);
        yy = yy.max(0.0);
    }
    yy = yy.max(1.525_878_9e-5);
    let s = x + yy;
    if s >= 1.0 {
        x /= s;
        yy /= s;
    }
    [x * y / yy, y, (1.0 - x - yy) * y / yy]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rgb_of(xy: Xy) -> [f32; 3] {
        let v = xy.to_xyz();
        let v = [v[0] / v[1], 1.0, v[2] / v[1]];
        REC2020.from_xyz().apply(v).map(|c| c as f32)
    }

    #[test]
    fn the_illuminant_becomes_white() {
        for cat in [Cat::Cat16, Cat::LinearBradford, Cat::FullBradford, Cat::Xyz] {
            for il in [Standard::A, Standard::F11, Standard::D50] {
                let cc = ColorCal::new(cat, il.xy(), 0.0, false);
                let w = cc.apply(rgb_of(il.xy()));
                for c in w {
                    assert!((c - 1.0).abs() < 2e-3, "{cat:?} {il:?}: {w:?}");
                }
            }
        }
    }

    #[test]
    fn d65_is_the_identity_and_methods_differ_off_white() {
        let cc = ColorCal::new(Cat::Cat16, REC2020.white, 0.0, false);
        assert!(cc.is_linear());
        let p = [0.2, 0.5, 0.1];
        let q = cc.apply(p);
        assert!(p.iter().zip(q).all(|(a, b)| (a - b).abs() < 1e-5), "{q:?}");
        let a = ColorCal::new(Cat::Cat16, Standard::A.xy(), 0.0, false).apply([0.6, 0.2, 0.1]);
        let b = ColorCal::new(Cat::Xyz, Standard::A.xy(), 0.0, false).apply([0.6, 0.2, 0.1]);
        assert!(a.iter().zip(b).any(|(x, y)| (x - y).abs() > 1e-3));
        // the linear methods are one matrix
        let m = ColorCal::new(Cat::LinearBradford, Standard::A.xy(), 0.0, false);
        let lin = mul(&m.matrix, [0.6, 0.2, 0.1]);
        assert!(lin.iter().zip(m.apply([0.6, 0.2, 0.1])).all(|(x, y)| (x - y).abs() < 1e-5));
        assert!(!ColorCal::new(Cat::FullBradford, Standard::A.xy(), 0.0, false).is_linear());
    }

    #[test]
    fn gamut_compression_tames_saturated_colours_only() {
        let off = ColorCal::new(Cat::Cat16, REC2020.white, 0.0, false);
        let on = ColorCal::new(Cat::Cat16, REC2020.white, 1.0, true);
        // a neutral is unchanged
        let g = on.apply([0.3, 0.3, 0.3]);
        assert!(g.iter().all(|v| (v - 0.3).abs() < 1e-4), "{g:?}");
        // a colour outside the gamut (negative green) is pulled in
        let sat = [1.2, -0.05, 0.02];
        let (a, b) = (off.apply(sat), on.apply(sat));
        assert!(a[1] < 0.0 && b.iter().all(|v| *v >= 0.0), "{a:?} {b:?}");
        // a saturated colour moves towards white, its luminance kept
        let y = |c: [f32; 3]| lightcraft_color::luminance_2020(c);
        let chroma = |c: [f32; 3]| (c[0].max(c[1]).max(c[2]) - c[0].min(c[1]).min(c[2])) / c[0].max(c[1]).max(c[2]);
        let soft = ColorCal::new(Cat::Cat16, REC2020.white, 1.0, false);
        let red = [0.9, 0.05, 0.05];
        let r = soft.apply(red);
        assert!(chroma(r) < chroma(red) && chroma(r) > 0.3, "{r:?}");
        assert!((y(r) - y(red)).abs() < 1e-4, "{} {}", y(r), y(red));
        assert!((daylight_xy(6504.0).x - 0.3127).abs() < 2e-3);
        assert_eq!(Standard::ALL.len(), 9);
    }
}
