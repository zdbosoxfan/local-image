//! Source colour descriptions, using the shared colour formulas and named spaces.
pub use lightcraft_color::space::{NamedSpace, Trc, rgb_to_xyz_d50};
use lightcraft_color::{D50, Mat3, RgbSpace, WORKING, bradford};
use serde::{Deserialize, Serialize};

/// Where a decoded image's colour interpretation came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SpaceOrigin {
    /// No colour information in the file: assumed sRGB.
    Untagged,
    /// A container-level description (PNG `sRGB`/`cHRM`/`gAMA`, JPEG XL enum colour encoding, …).
    Container,
    /// An embedded ICC matrix/TRC profile, applied exactly.
    IccMatrixTrc,
    /// An embedded LUT-based (or CMYK/Lab) ICC profile, converted by the CMS to linear Rec.2020.
    IccCms,
    /// An embedded ICC profile we could not apply (malformed/unsupported): **fell back to sRGB**.
    IccUnsupported,
    /// A naive conversion without a profile (e.g. CMYK → RGB without an ICC profile), then sRGB.
    Naive,
}

/// The colour space of [`crate::Decoded::image`]: linear RGB with these primaries.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SourceSpace {
    /// Recognised well-known space, if the primaries match one.
    pub named: Option<NamedSpace>,
    /// Linear image RGB → CIE XYZ relative to D50 (ICC PCS; white Y = 1).
    pub to_xyz_d50: Mat3,
    /// The encoding curves that were removed during decode (R, G, B), if known.
    pub trc: Option<[Trc; 3]>,
    pub origin: SpaceOrigin,
}

impl SourceSpace {
    pub fn named(n: NamedSpace, origin: SpaceOrigin) -> SourceSpace {
        let t = n.trc();
        SourceSpace { named: Some(n), to_xyz_d50: n.to_xyz_d50(), trc: Some([t.clone(), t.clone(), t]), origin }
    }

    /// Linear source RGB → linear `dst` RGB (Bradford from D50 to `dst`'s white).
    pub fn to_space(&self, dst: &RgbSpace) -> Mat3 {
        if let Some(n) = self.named {
            let s = n.rgb_space();
            if s == *dst {
                return Mat3::IDENTITY;
            }
            return s.to_space(dst);
        }
        dst.from_xyz().mul(&bradford(D50, dst.white)).mul(&self.to_xyz_d50)
    }

    /// Linear source RGB → linear Rec.2020 D65 (the pipeline working space).
    pub fn to_working(&self) -> Mat3 {
        self.to_space(&WORKING)
    }

    /// True when the pixels need a matrix to reach the working space.
    pub fn is_working(&self) -> bool {
        self.named == Some(NamedSpace::Rec2020)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn named_recognition_roundtrip() {
        for n in NamedSpace::ALL {
            assert_eq!(NamedSpace::recognize(&n.to_xyz_d50()), Some(n));
        }
    }

    #[test]
    fn trc_inverse() {
        let curves = [
            Trc::Srgb,
            Trc::Gamma(2.2),
            Trc::Rec709,
            Trc::Parametric(vec![2.4, 1.0 / 1.055, 0.055 / 1.055, 1.0 / 12.92, 0.04045]),
            Trc::Table((0..1024).map(|i| ((i as f32 / 1023.0).powf(2.2) * 65535.0) as u16).collect()),
        ];
        for c in curves {
            for i in 0..=20 {
                let x = i as f32 / 20.0;
                let y = c.to_linear(x);
                assert!((c.from_linear(y) - x).abs() < 2e-3, "{c:?} {x}");
            }
        }
        assert!(Trc::Srgb.approx_eq(&Trc::Parametric(vec![2.4, 1.0 / 1.055, 0.055 / 1.055, 1.0 / 12.92, 0.04045]), 1e-5));
    }

    #[test]
    fn working_matrix_identity_for_rec2020() {
        let s = SourceSpace::named(NamedSpace::Rec2020, SpaceOrigin::Container);
        assert_eq!(s.to_working(), Mat3::IDENTITY);
        let p3 = SourceSpace::named(NamedSpace::DisplayP3, SpaceOrigin::Container);
        let w = p3.to_working().apply([1.0, 1.0, 1.0]);
        for c in w {
            assert!((c - 1.0).abs() < 1e-6);
        }
    }
}
