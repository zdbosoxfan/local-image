//! Source colour spaces, transfer curves and the conversion to the working space.

use crate::{ADOBE_RGB, D50, DISPLAY_P3, Mat3, PROPHOTO, REC2020, RgbSpace, SRGB, bradford};
use serde::{Deserialize, Serialize};

/// A transfer curve (encoded value → linear light), as found in ICC `curv`/`para` tags and container
/// colour descriptions.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Trc {
    /// Identity (already linear).
    Linear,
    /// IEC 61966-2-1 sRGB curve (also used by Display P3).
    Srgb,
    /// Pure power law `y = x^g`.
    Gamma(f32),
    /// ITU-R BT.709 / BT.2020 OETF inverse.
    Rec709,
    /// ICC parametric curve parameters `[g, a, b, c, d, e, f]` (1, 3, 4, 5 or 7 values; ICC function types 0–4).
    Parametric(Vec<f32>),
    /// Sampled curve over `[0, 1]` (ICC `curv` with ≥ 2 entries), linearly interpolated.
    Table(Vec<u16>),
}

impl Trc {
    /// Encoded value → linear. Negative values are mirrored and values above 1 are extrapolated,
    /// so extended-range float data survives.
    pub fn to_linear(&self, v: f32) -> f32 {
        if v < 0.0 {
            return -self.to_linear(-v);
        }
        match self {
            Trc::Linear => v,
            Trc::Srgb => crate::transfer::srgb_to_linear(v),
            Trc::Gamma(g) => v.powf(*g),
            Trc::Rec709 => crate::transfer::rec709_to_linear(v),
            Trc::Parametric(p) => eval_parametric(p, v),
            Trc::Table(t) => eval_table(t, v),
        }
    }

    /// Linear → encoded value (inverse of [`Trc::to_linear`]); tables are inverted numerically.
    pub fn from_linear(&self, v: f32) -> f32 {
        if v < 0.0 {
            return -self.from_linear(-v);
        }
        match self {
            Trc::Linear => v,
            Trc::Srgb => crate::transfer::linear_to_srgb(v),
            Trc::Gamma(g) => {
                if *g > 0.0 {
                    v.powf(1.0 / g)
                } else {
                    v
                }
            }
            Trc::Rec709 => crate::transfer::linear_to_rec709(v),
            Trc::Parametric(_) | Trc::Table(_) => {
                // Monotone curves: bisection on [0, max(1, v)].
                let (mut lo, mut hi) = (0.0f32, 1.0f32);
                while self.to_linear(hi) < v && hi < 1e6 {
                    hi *= 2.0;
                }
                for _ in 0..40 {
                    let mid = 0.5 * (lo + hi);
                    if self.to_linear(mid) < v {
                        lo = mid;
                    } else {
                        hi = mid;
                    }
                }
                0.5 * (lo + hi)
            }
        }
    }

    /// Output encoding policy: clamp to the display range and preserve the historic
    /// power-law behavior even for caller-supplied degenerate exponents.
    pub fn encode_clamped(&self, v: f32) -> f32 {
        let v = v.clamp(0.0, 1.0);
        match self {
            Self::Gamma(g) => v.powf(1.0 / g),
            _ => self.from_linear(v),
        }
    }

    /// Convert an ICC analytical/table curve into the legacy serialized summary.
    /// ICC samples are quantized to u16 only at this compatibility boundary.
    pub fn from_curve(t: &crate::curve::Curve) -> Self {
        use crate::curve::Curve;
        match t {
            Curve::Identity => Self::Linear,
            Curve::Gamma(g) => Self::Gamma(*g as f32),
            Curve::Parametric { kind, p } => Self::Parametric(p[..Curve::param_count(*kind)].iter().map(|v| *v as f32).collect()),
            Curve::Table(t) => Self::Table(t.iter().map(|v| (v * 65535.0).round().clamp(0.0, 65535.0) as u16).collect()),
        }
    }

    /// Convert the serialized summary to the full-precision ICC curve representation.
    pub fn to_curve(&self) -> crate::curve::Curve {
        use crate::curve::{Curve, srgb_trc};
        match self {
            Self::Linear => Curve::Identity,
            Self::Srgb => srgb_trc(),
            Self::Gamma(g) => Curve::Gamma(*g as f64),
            Self::Rec709 => Curve::Parametric { kind: 3, p: [1.0 / 0.45, 1.0 / 1.099, 0.099 / 1.099, 1.0 / 4.5, 0.081, 0.0, 0.0] },
            Self::Table(t) => Curve::Table(t.iter().map(|v| *v as f32 / 65535.0).collect()),
            Self::Parametric(params) => {
                let kind = match params.len() {
                    1 => 0,
                    3 => 1,
                    4 => 2,
                    5 => 3,
                    7 => 4,
                    _ => return Curve::Identity,
                };
                let mut p = [0.0; 7];
                for (out, value) in p.iter_mut().zip(params) {
                    *out = *value as f64;
                }
                Curve::Parametric { kind, p }
            }
        }
    }

    /// True for the identity curve (including degenerate ICC encodings of it).
    pub fn is_linear(&self) -> bool {
        match self {
            Trc::Linear => true,
            Trc::Gamma(g) => (*g - 1.0).abs() < 1e-4,
            Trc::Parametric(p) => p.len() == 1 && (p[0] - 1.0).abs() < 1e-4,
            Trc::Table(t) => t.is_empty(),
            _ => false,
        }
    }

    /// Whether two curves agree to within `tol` on a sample grid.
    pub fn approx_eq(&self, other: &Trc, tol: f32) -> bool {
        (0..=32).all(|i| {
            let x = i as f32 / 32.0;
            (self.to_linear(x) - other.to_linear(x)).abs() <= tol
        })
    }

    /// 256-entry linearization table for 8-bit data.
    pub fn lut8(&self) -> Vec<f32> {
        (0..256).map(|i| self.to_linear(i as f32 / 255.0)).collect()
    }

    /// 65536-entry linearization table for 16-bit data.
    pub fn lut16(&self) -> Vec<f32> {
        if self.is_linear() {
            return (0..65536).map(|i| i as f32 / 65535.0).collect();
        }
        (0..65536).map(|i| self.to_linear(i as f32 / 65535.0)).collect()
    }
}

fn eval_parametric(p: &[f32], x: f32) -> f32 {
    crate::curve::parametric_f32(p, x)
}

fn eval_table(t: &[u16], x: f32) -> f32 {
    match t.len() {
        0 => x,
        1 => x.powf(t[0] as f32 / 256.0),
        n => crate::curve::table_sample_f32(n, x, |i| t[i] as f32 / 65535.0, false),
    }
}

/// Well-known RGB spaces we recognise (from ICC colorants or container tags).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum NamedSpace {
    Srgb,
    DisplayP3,
    AdobeRgb,
    ProPhoto,
    Rec2020,
}

impl NamedSpace {
    pub const ALL: [NamedSpace; 5] = [NamedSpace::Srgb, NamedSpace::DisplayP3, NamedSpace::AdobeRgb, NamedSpace::ProPhoto, NamedSpace::Rec2020];

    pub fn rgb_space(self) -> RgbSpace {
        match self {
            NamedSpace::Srgb => SRGB,
            NamedSpace::DisplayP3 => DISPLAY_P3,
            NamedSpace::AdobeRgb => ADOBE_RGB,
            NamedSpace::ProPhoto => PROPHOTO,
            NamedSpace::Rec2020 => REC2020,
        }
    }

    /// The space's standard encoding curve.
    pub fn trc(self) -> Trc {
        match self {
            NamedSpace::Srgb | NamedSpace::DisplayP3 => Trc::Srgb,
            NamedSpace::AdobeRgb => Trc::Gamma(563.0 / 256.0),
            NamedSpace::ProPhoto => Trc::Gamma(1.8),
            NamedSpace::Rec2020 => Trc::Rec709,
        }
    }

    /// ICC built-in profile colorimetry. ROMM's published four-decimal
    /// chromaticities and the encoded ICC D50 XYZ white are explicit profile
    /// compatibility policies; Develop retains its historic chromaticities.
    pub fn icc_colorimetry(self) -> ([[f64; 2]; 3], [f64; 3]) {
        let space = self.rgb_space();
        let primaries = if self == Self::ProPhoto {
            [[0.7347, 0.2653], [0.1596, 0.8404], [0.0366, 0.0001]]
        } else {
            [space.r, space.g, space.b].map(|xy| [xy.x, xy.y])
        };
        let white = if self == Self::ProPhoto { crate::cms_math::D50 } else { crate::cms_math::xy_to_xyz(crate::cms_math::D65_XY) };
        (primaries, white)
    }

    /// Analytical ICC curve, preserving the coefficient arithmetic used by
    /// the shared CMS's generated built-in profile bytes and float transforms.
    pub fn icc_curve(self) -> crate::curve::Curve {
        use crate::curve::{Curve, srgb_trc};
        match self {
            Self::Srgb | Self::DisplayP3 => srgb_trc(),
            Self::AdobeRgb => Curve::Gamma(563.0 / 256.0),
            Self::ProPhoto => Curve::Gamma(1.8),
            Self::Rec2020 => {
                let a = 1.099;
                Curve::Parametric { kind: 3, p: [1.0 / 0.45, 1.0 / a, (a - 1.0) / a, 1.0 / 4.5, 0.081, 0.0, 0.0] }
            }
        }
    }

    /// Linear RGB → XYZ (D50 PCS, Bradford-adapted), as an ICC matrix/TRC profile would store it.
    pub fn to_xyz_d50(self) -> Mat3 {
        rgb_to_xyz_d50(&self.rgb_space())
    }

    pub fn name(self) -> &'static str {
        self.rgb_space().name
    }

    /// Recognise a space from its D50-adapted colorant matrix (tolerant to s15Fixed16 rounding and
    /// the small variations between vendors' profiles).
    pub fn recognize(to_xyz_d50: &Mat3) -> Option<NamedSpace> {
        NamedSpace::ALL.into_iter().find(|n| {
            let m = n.to_xyz_d50();
            (0..3).all(|r| (0..3).all(|c| (m.0[r][c] - to_xyz_d50.0[r][c]).abs() < 0.004))
        })
    }
}

/// `space`'s linear RGB → XYZ relative to D50 (Bradford), the ICC PCS.
pub fn rgb_to_xyz_d50(space: &RgbSpace) -> Mat3 {
    bradford(space.white, D50).mul(&space.to_xyz())
}

/// Compatibility output adapters retain camelCase serialization and clamped rendering.
/// The RGB space an image is rendered into.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum OutputSpace {
    #[default]
    Srgb,
    DisplayP3,
    /// Primaries and gamma of the published Adobe RGB (1998) specification ("compatible": our own
    /// profile, built from the published numbers).
    AdobeRgb,
    ProPhoto,
    Rec2020,
}

/// The encoding curve of an [`OutputSpace`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum OutputTrc {
    /// IEC 61966-2-1 (sRGB, Display P3).
    Srgb,
    /// Pure power law: encoded = linear^(1/g).
    Gamma(f32),
    /// ITU-R BT.709 / BT.2020 OETF.
    Rec709,
}

impl OutputTrc {
    fn trc(self) -> Trc {
        match self {
            Self::Srgb => Trc::Srgb,
            Self::Gamma(g) => Trc::Gamma(g),
            Self::Rec709 => Trc::Rec709,
        }
    }

    /// Linear → encoded (input clamped to 0..1).
    pub fn encode(self, v: f32) -> f32 {
        self.trc().encode_clamped(v)
    }

    /// Encoded → linear (input clamped to 0..1).
    pub fn decode(self, e: f32) -> f32 {
        self.trc().to_linear(e.clamp(0.0, 1.0))
    }

    /// Code for the GPU kernel: 0 sRGB, 1 gamma, 2 Rec.709.
    pub fn code(self) -> (u32, f32) {
        match self {
            OutputTrc::Srgb => (0, 1.0),
            OutputTrc::Gamma(g) => (1, g),
            OutputTrc::Rec709 => (2, 1.0),
        }
    }
}

impl OutputSpace {
    pub const ALL: [OutputSpace; 5] = [OutputSpace::Srgb, OutputSpace::DisplayP3, OutputSpace::AdobeRgb, OutputSpace::ProPhoto, OutputSpace::Rec2020];

    pub fn named(self) -> NamedSpace {
        match self {
            Self::Srgb => NamedSpace::Srgb,
            Self::DisplayP3 => NamedSpace::DisplayP3,
            Self::AdobeRgb => NamedSpace::AdobeRgb,
            Self::ProPhoto => NamedSpace::ProPhoto,
            Self::Rec2020 => NamedSpace::Rec2020,
        }
    }

    pub fn rgb_space(self) -> RgbSpace {
        self.named().rgb_space()
    }

    pub fn trc(self) -> OutputTrc {
        match self {
            OutputSpace::Srgb | OutputSpace::DisplayP3 => OutputTrc::Srgb,
            OutputSpace::AdobeRgb => OutputTrc::Gamma(563.0 / 256.0),
            OutputSpace::ProPhoto => OutputTrc::Gamma(1.8),
            OutputSpace::Rec2020 => OutputTrc::Rec709,
        }
    }

    /// User-facing name.
    pub fn label(self) -> &'static str {
        match self {
            OutputSpace::Srgb => "sRGB",
            OutputSpace::DisplayP3 => "Display P3",
            OutputSpace::AdobeRgb => "Adobe RGB (1998) compatible",
            OutputSpace::ProPhoto => "ProPhoto RGB",
            OutputSpace::Rec2020 => "Rec. 2020",
        }
    }

    pub fn parse(s: &str) -> Option<OutputSpace> {
        let k: String = s.chars().filter(|c| c.is_ascii_alphanumeric()).collect::<String>().to_ascii_lowercase();
        Some(match k.as_str() {
            "srgb" => OutputSpace::Srgb,
            "displayp3" | "p3" => OutputSpace::DisplayP3,
            "adobergb" | "adobergb1998" | "adobergb1998compatible" | "widegamutrgb" => OutputSpace::AdobeRgb,
            "prophoto" | "prophotorgb" | "rommrgb" => OutputSpace::ProPhoto,
            "rec2020" | "bt2020" | "itur2020" => OutputSpace::Rec2020,
            _ => return None,
        })
    }

    /// Linear Rec.2020 → linear target RGB (Bradford-adapted for D50 targets), row-major.
    pub fn from_working(self) -> [[f32; 3]; 3] {
        REC2020.to_space(&self.rgb_space()).to_f32()
    }

    /// Luminance weights of the target's linear RGB, as the gamut mapper uses them. sRGB keeps the
    /// rounded BT.709 weights the pipeline has always used (byte-identical previews and exports).
    pub fn luma(self) -> [f32; 3] {
        match self {
            OutputSpace::Srgb => [0.2126, 0.7152, 0.0722],
            o => o.rgb_space().luma().map(|v| v as f32),
        }
    }
}
