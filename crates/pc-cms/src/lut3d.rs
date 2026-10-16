//! 3D lookup tables for applying a display transform on the GPU.

use crate::Transform;

/// An `size³` RGBA lookup table: entry `r + g·size + b·size²` holds the output for input
/// `(r, g, b) / (size − 1)`, alpha 1. This is the memory order of a 3D texture whose x axis is
/// red, so it uploads directly (sample at `(c·(size−1) + 0.5) / size` with linear filtering).
#[derive(Clone, Debug, PartialEq)]
pub struct Lut3d {
    pub size: usize,
    pub data: Vec<[f32; 4]>,
}

impl Lut3d {
    /// Samples a 3-input transform (1-output transforms are replicated to gray).
    pub fn from_transform(t: &Transform, size: usize) -> Lut3d {
        assert!(t.inputs() == 3 && (t.outputs() == 3 || t.outputs() == 1), "display LUTs need a 3→3 or 3→1 transform");
        assert!(size >= 2, "LUT size");
        let mut data = vec![[0.0f32, 0.0, 0.0, 1.0]; size * size * size];
        let s = (size - 1) as f32;
        let mut out = [0.0f32; 16];
        for b in 0..size {
            for g in 0..size {
                for r in 0..size {
                    t.eval(&[r as f32 / s, g as f32 / s, b as f32 / s], &mut out);
                    let v = if t.outputs() == 1 { [out[0]; 3] } else { [out[0], out[1], out[2]] };
                    data[r + g * size + b * size * size] = [v[0], v[1], v[2], 1.0];
                }
            }
        }
        Lut3d { size, data }
    }

    /// Shared precise tetrahedral lookup. GPU texture filtering remains trilinear.
    pub fn sample(&self, rgb: [f32; 3]) -> [f32; 3] {
        let n = self.size;
        if n.checked_pow(3).is_none_or(|len| self.data.len() < len) {
            return rgb;
        }
        li_color::lut::sample(n, rgb, true, |r, g, b| {
            let c = self.data[r + g * n + b * n * n];
            [c[0], c[1], c[2]]
        })
    }

    /// Little-endian RGBA16F texels (8 bytes each), e.g. for `wgpu::TextureFormat::Rgba16Float`.
    pub fn to_rgba16f_bytes(&self) -> Vec<u8> {
        let mut o = Vec::with_capacity(self.data.len() * 8);
        for px in &self.data {
            for v in px {
                o.extend_from_slice(&half::f16::from_f32(*v).to_le_bytes());
            }
        }
        o
    }

    /// RGBA8 texels (for backends without float 3D textures).
    pub fn to_rgba8(&self) -> Vec<u8> {
        self.data.iter().flat_map(|p| p.map(|v| (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8)).collect()
    }
}
