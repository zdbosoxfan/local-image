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
                o.extend_from_slice(&f32_to_f16(*v).to_le_bytes());
            }
        }
        o
    }

    /// RGBA8 texels (for backends without float 3D textures).
    pub fn to_rgba8(&self) -> Vec<u8> {
        self.data.iter().flat_map(|p| p.map(|v| (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8)).collect()
    }
}

/// IEEE 754 binary16 bits of `v` (round to nearest even; overflow → ±inf, NaN kept).
pub fn f32_to_f16(v: f32) -> u16 {
    let x = v.to_bits();
    let sign = ((x >> 16) & 0x8000) as u16;
    let exp = ((x >> 23) & 0xFF) as i32;
    let man = x & 0x7F_FFFF;
    if exp == 0xFF {
        return sign | 0x7C00 | if man != 0 { 0x200 } else { 0 };
    }
    let e = exp - 127 + 15;
    if e >= 0x1F {
        return sign | 0x7C00;
    }
    if e <= 0 {
        if e < -10 {
            return sign;
        }
        let m = man | 0x80_0000;
        let shift = (14 - e) as u32;
        let half = 1u32 << (shift - 1);
        let rem = m & ((1 << shift) - 1);
        let mut r = m >> shift;
        if rem > half || (rem == half && r & 1 == 1) {
            r += 1;
        }
        return sign | r as u16;
    }
    let mut r = ((e as u32) << 10) | (man >> 13);
    let rem = man & 0x1FFF;
    if rem > 0x1000 || (rem == 0x1000 && r & 1 == 1) {
        r += 1;
    }
    sign | r as u16
}

/// binary16 bits → f32.
pub fn f16_to_f32(h: u16) -> f32 {
    let sign = if h & 0x8000 != 0 { -1.0 } else { 1.0 };
    let e = ((h >> 10) & 0x1F) as i32;
    let m = (h & 0x3FF) as f32;
    match e {
        0 => sign * m * 2f32.powi(-24),
        0x1F => {
            if m == 0.0 {
                sign * f32::INFINITY
            } else {
                f32::NAN
            }
        }
        _ => sign * (1.0 + m / 1024.0) * 2f32.powi(e - 15),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn half_float_roundtrip() {
        for v in [0.0f32, 1.0, 0.5, 0.333, 1e-5, 65504.0, -2.25, 0.1] {
            let back = f16_to_f32(f32_to_f16(v));
            assert!((back - v).abs() <= v.abs() * 1e-3 + 1e-7, "{v} -> {back}");
        }
        assert_eq!(f32_to_f16(1.0), 0x3C00);
        assert_eq!(f32_to_f16(1e6), 0x7C00);
    }
}
