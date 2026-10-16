//! 3D LUT profiles: `.cube` files (the plain-text Cube LUT format: `LUT_3D_SIZE`, optional
//! `DOMAIN_MIN` / `DOMAIN_MAX`, then size³ "r g b" lines with red changing fastest), applied to
//! the display-encoded output colour and blended by the profile amount. Profiles are registered
//! here by id (`lut:…`) and found by the finishing stage; LUT profiles render on the CPU.

use std::collections::HashMap;
use std::sync::{Arc, OnceLock, RwLock};

/// A 3D colour lookup table.
#[derive(Clone, Debug, PartialEq)]
pub struct Lut3d {
    pub size: usize,
    pub data: Vec<[f32; 3]>,
    pub domain_min: [f32; 3],
    pub domain_max: [f32; 3],
    pub title: Option<String>,
}

impl Lut3d {
    /// Parse a `.cube` file using the shared format parser.
    pub fn parse_cube(text: &str) -> Result<Lut3d, String> {
        li_color::lutfile::parse_cube_with_domain(text).map(Self::from_parsed).map_err(|e| e.to_string())
    }

    /// Parse a Cube, Autodesk 3DL or SpeedGrade Look file.
    pub fn parse(file_name: &str, bytes: &[u8]) -> Result<Lut3d, String> {
        li_color::lutfile::parse_with_domain(file_name, bytes).map(Self::from_parsed).map_err(|e| e.to_string())
    }

    fn from_parsed(parsed: li_color::lutfile::ParsedLut) -> Self {
        Self {
            size: parsed.lut.size,
            data: parsed.lut.data.as_chunks::<3>().0.to_vec(),
            domain_min: parsed.domain_min,
            domain_max: parsed.domain_max,
            title: parsed.title,
        }
    }

    /// Look up a colour with shared precise tetrahedral interpolation.
    pub fn apply(&self, c: [f32; 3]) -> [f32; 3] {
        let n = self.size;
        if n < 2 || n.checked_pow(3).is_none_or(|len| self.data.len() < len) {
            return c;
        }
        let normalized = li_color::lut::normalize_domain(c, self.domain_min, self.domain_max);
        li_color::lut::sample(n, normalized, true, |r, g, b| self.data[r + n * (g + n * b)])
    }
}

fn registry() -> &'static RwLock<HashMap<String, Arc<Lut3d>>> {
    static R: OnceLock<RwLock<HashMap<String, Arc<Lut3d>>>> = OnceLock::new();
    R.get_or_init(Default::default)
}

/// Make a LUT available to renders under profile id `id` (`lut:…`).
pub fn register(id: &str, lut: Lut3d) {
    registry().write().unwrap_or_else(|e| e.into_inner()).insert(id.to_string(), Arc::new(lut));
}

/// The LUT registered for profile `id`.
pub fn get(id: &str) -> Option<Arc<Lut3d>> {
    if !id.starts_with("lut:") {
        return None;
    }
    registry().read().unwrap_or_else(|e| e.into_inner()).get(id).cloned()
}

/// Forget a LUT profile.
pub fn unregister(id: &str) {
    registry().write().unwrap_or_else(|e| e.into_inner()).remove(id);
}

/// Profile ids that render with a LUT (and so on the CPU).
pub fn is_lut_profile(id: &str) -> bool {
    id.starts_with("lut:")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cube(f: impl Fn([f32; 3]) -> [f32; 3], n: usize) -> String {
        let mut s = format!("TITLE \"test\"\nLUT_3D_SIZE {n}\n");
        for b in 0..n {
            for g in 0..n {
                for r in 0..n {
                    let c = f([r as f32 / (n - 1) as f32, g as f32 / (n - 1) as f32, b as f32 / (n - 1) as f32]);
                    s.push_str(&format!("{} {} {}\n", c[0], c[1], c[2]));
                }
            }
        }
        s
    }

    #[test]
    fn identity_and_swap() {
        let id = Lut3d::parse_cube(&cube(|c| c, 17)).unwrap();
        assert_eq!(id.title.as_deref(), Some("test"));
        let v = id.apply([0.2, 0.55, 0.9]);
        assert!(v.iter().zip([0.2, 0.55, 0.9]).all(|(a, b)| (a - b).abs() < 1e-5), "{v:?}");
        let swap = Lut3d::parse_cube(&cube(|c| [c[2], c[1], c[0]], 9)).unwrap();
        let v = swap.apply([0.1, 0.5, 0.8]);
        assert!((v[0] - 0.8).abs() < 1e-5 && (v[2] - 0.1).abs() < 1e-5, "{v:?}");
        assert!(Lut3d::parse_cube("LUT_1D_SIZE 4\n0 0 0").is_err());
        assert!(Lut3d::parse_cube("LUT_3D_SIZE 2\n0 0 0\n").is_err(), "too few entries");
    }
    #[test]
    fn shared_parser_and_sampler_forward_bit_identically() {
        let text = format!("DOMAIN_MIN -1 -1 -1\nDOMAIN_MAX 1 1 1\n{}", cube(|c| [c[0] * c[1], c[1].sqrt(), c[2].powi(2)], 3));
        let parsed = li_color::lutfile::parse_cube_with_domain(&text).unwrap();
        let profile = Lut3d::parse_cube(&text).unwrap();
        assert_eq!(
            profile.data.as_flattened().iter().copied().map(f32::to_bits).collect::<Vec<_>>(),
            parsed.lut.data.iter().copied().map(f32::to_bits).collect::<Vec<_>>()
        );
        for c in [[-1.; 3], [1.; 3], [-0.5, 0.5, 0.], [0.75, -0.25, 0.5]] {
            let normalized = c.map(|v| (v + 1.) / 2.);
            assert_eq!(profile.apply(c).map(f32::to_bits), li_color::lut::sample_rgb(&parsed.lut.data, 3, normalized, true).map(f32::to_bits));
        }
    }

    #[test]
    fn develop_accepts_3dl_and_look_with_shared_data() {
        let id = li_color::lutfile::LutFile::identity(2);
        let mut three_dl = String::new();
        for r in 0..2 {
            for g in 0..2 {
                for b in 0..2 {
                    three_dl.push_str(&format!("{} {} {}\n", r * 4095, g * 4095, b * 4095));
                }
            }
        }
        let hex: String = id.data.iter().flat_map(|v| v.to_le_bytes()).map(|b| format!("{b:02X}")).collect();
        let look = format!("<look><size>2</size><data>{hex}</data></look>");
        for (name, text) in [("identity.3dl", three_dl), ("identity.look", look)] {
            let parsed = Lut3d::parse(name, text.as_bytes()).unwrap();
            assert_eq!(parsed.data.as_flattened(), id.data.as_slice());
            assert_eq!(parsed.apply([0.25, 0.5, 0.75]), [0.25, 0.5, 0.75]);
        }
    }
}
