//! ICC profile summaries for the Library over the shared pure-Rust ICC parser, writer and CMS.
//! Library keeps its serialized matrix/TRC summaries; both halves use photocraft-cms.
use crate::space::{NamedSpace, Trc, rgb_to_xyz_d50};
use li_color::{D50, Mat3, RgbSpace, bradford, curve::Curve};
use photocraft_cms::{Builtin, ColorSpace, Intent, Pcs, Profile, Transform};
use serde::{Deserialize, Serialize};

/// Colour model of the profile's device space.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum IccColorModel {
    Rgb,
    Gray,
    Cmyk,
    Other,
}

/// How a profile can be applied.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum IccKind {
    /// RGB matrix/TRC (shaper) profile: exact, applied by us.
    MatrixTrc { to_xyz_d50: Mat3, trc: [Trc; 3] },
    /// Grayscale profile with a `kTRC`.
    Gray { trc: Trc },
    /// LUT-based (A2B), CMYK or otherwise needs a full CMS conversion.
    NeedsCms,
}

/// Summary of a parsed ICC profile.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct IccInfo {
    pub model: IccColorModel,
    pub kind: IccKind,
    /// Recognised well-known RGB space (by colorants), if any.
    pub named: Option<NamedSpace>,
    pub description: Option<String>,
}

/// Parse ICC v2/v4 with the same parser as Compositing. Invalid input returns `None`.
pub fn parse(bytes: &[u8]) -> Option<IccInfo> {
    let p = Profile::parse(bytes).ok()?;
    let model = match p.color_space {
        ColorSpace::Rgb => IccColorModel::Rgb,
        ColorSpace::Gray => IccColorModel::Gray,
        ColorSpace::Cmyk => IccColorModel::Cmyk,
        _ => IccColorModel::Other,
    };
    let has_lut = p.a2b.iter().any(Option::is_some);
    let kind = if model == IccColorModel::Rgb && p.is_matrix_shaper() && p.pcs == Pcs::Xyz {
        let to_xyz_d50 = Mat3(p.matrix?);
        let trc = p.trc.as_ref()?.each_ref().map(from_curve);
        if to_xyz_d50.determinant().abs() > 1e-6 && trc.iter().all(valid_trc) { IccKind::MatrixTrc { to_xyz_d50, trc } } else { IccKind::NeedsCms }
    } else if model == IccColorModel::Gray && !has_lut && p.gray_trc.is_some() {
        let trc = from_curve(p.gray_trc.as_ref()?);
        if valid_trc(&trc) { IccKind::Gray { trc } } else { IccKind::NeedsCms }
    } else {
        IccKind::NeedsCms
    };
    let named = if let IccKind::MatrixTrc { to_xyz_d50, .. } = &kind { NamedSpace::recognize(to_xyz_d50) } else { None };
    let description = p.description.trim_end_matches('\0').trim().to_string();
    Some(IccInfo { model, kind, named, description: (!description.is_empty()).then_some(description) })
}
fn valid_trc(t: &Trc) -> bool {
    match t {
        Trc::Parametric(p) => matches!(p.len(), 1 | 3 | 4 | 5 | 7) && p.iter().all(|v| v.is_finite()),
        Trc::Gamma(g) => g.is_finite() && *g > 0.0,
        _ => true,
    }
}
fn from_curve(t: &Curve) -> Trc {
    Trc::from_curve(t)
}
fn to_curve(t: &Trc) -> Curve {
    t.to_curve()
}
/// Build a deterministic ICC v4 matrix/TRC profile with the shared ICC writer.
pub fn write_matrix_trc(space: &RgbSpace, trc: &Trc) -> Vec<u8> {
    let mut p = Builtin::Srgb.profile().clone();
    p.matrix = Some(rgb_to_xyz_d50(space).0);
    let curve = to_curve(trc);
    p.trc = Some([curve.clone(), curve.clone(), curve]);
    p.chad = Some(bradford(space.white, D50).0);
    p.description = if trc.is_linear() { format!("{} (linear)", space.name) } else { space.name.to_string() };
    p.with_encoded_bytes().to_bytes().as_ref().clone()
}
/// Standard named profiles are byte-identical in Library and Compositing.
pub fn write_named(n: NamedSpace) -> Vec<u8> {
    let builtin = Builtin::from_named(n);
    builtin.profile().to_bytes().as_ref().clone()
}
/// LUT/CMYK fallback uses the same CMS, intent and working profile as the editor.
pub(crate) fn cms_to_linear_rec2020(icc: &[u8], samples: &[f32], channels: usize) -> Option<Vec<[f32; 3]>> {
    let src = Profile::parse(icc).ok()?;
    if src.channels() != channels
        || !matches!(src.color_space, ColorSpace::Rgb | ColorSpace::Gray | ColorSpace::Cmyk)
        || !samples.len().is_multiple_of(channels)
    {
        return None;
    }
    let t = Transform::new(&src, photocraft_cms::builtin::linear_rec2020(), Intent::RelativeColorimetric, true).ok()?;
    let mut out = vec![0.0; samples.len() / channels * 3];
    t.convert_f32(samples, channels, &mut out, 3, false);
    Some(out.as_chunks::<3>().0.to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;
    use li_color::{DISPLAY_P3, PROPHOTO, SRGB};

    #[test]
    fn write_then_parse_recognises() {
        for n in NamedSpace::ALL {
            let bytes = write_named(n);
            assert!(bytes.len() > 128, "{n:?}");
            let info = parse(&bytes).expect("parses");
            assert_eq!(info.named, Some(n), "{n:?}");
            match info.kind {
                IccKind::MatrixTrc { trc, .. } => assert!(trc[0].approx_eq(&n.trc(), 2e-4), "{n:?} {:?}", trc[0]),
                k => panic!("{k:?}"),
            }
        }
    }

    #[test]
    fn tags_are_aligned() {
        for n in NamedSpace::ALL {
            for trc in [n.trc(), Trc::Linear] {
                let b = write_matrix_trc(&n.rgb_space(), &trc);
                assert_eq!(b.len() % 4, 0, "{n:?}");
                assert_eq!(u32::from_be_bytes(b[0..4].try_into().unwrap()) as usize, b.len());
                let count = u32::from_be_bytes(b[128..132].try_into().unwrap()) as usize;
                for t in 0..count {
                    let off = u32::from_be_bytes(b[132 + 12 * t + 4..132 + 12 * t + 8].try_into().unwrap());
                    assert_eq!(off % 4, 0, "{n:?} tag {t}");
                }
                assert!(parse(&b).is_some_and(|i| i.named == Some(n)), "{n:?}");
            }
        }
    }

    #[test]
    fn linear_profile() {
        let bytes = write_matrix_trc(&PROPHOTO, &Trc::Linear);
        let info = parse(&bytes).unwrap();
        assert_eq!(info.named, Some(NamedSpace::ProPhoto));
        let IccKind::MatrixTrc { trc, .. } = info.kind else { panic!() };
        assert!(trc[1].is_linear());
    }

    #[test]
    fn cms_path_matches_matrix_path() {
        let bytes = write_named(NamedSpace::DisplayP3);
        let px = [0.2f32, 0.5, 0.8];
        let out = cms_to_linear_rec2020(&bytes, &px, 3).unwrap()[0];
        let lin = px.map(|v| Trc::Srgb.to_linear(v));
        let m = DISPLAY_P3.to_space(&li_color::REC2020);
        let want = m.apply_f32(lin);
        for i in 0..3 {
            assert!((out[i] - want[i]).abs() < 2e-3, "{out:?} vs {want:?}");
        }
        let _ = SRGB;
    }

    #[test]
    fn garbage_is_none() {
        assert!(parse(&[]).is_none());
        assert!(parse(&[0u8; 200]).is_none());
    }
}
