//! Output encoding: the colour space (primaries + transfer curve) and sample format a render is
//! delivered in. Previews are always 8-bit sRGB; exports may ask for a wide-gamut space and/or
//! 16-bit or linear float samples.
//!
//! The per-pixel stage works in scene-linear Rec.2020 and ends by converting to the target's
//! primaries, gamut mapping into the *target* gamut, and encoding. Tone curves and grain operate on
//! sRGB-curve-encoded values of the target primaries (identical to the sRGB path when the target is
//! sRGB); afterwards the values are re-encoded with the target's own curve.

pub use lightcraft_color::space::{OutputSpace, OutputTrc};
use serde::{Deserialize, Serialize};

/// Soft proofing: render as if the result were converted to `space` (colours outside its gamut
/// are mapped into it, then shown in the render's own space), optionally painting what the
/// destination can't hold (`dest_warning`, red) and what the display can't show
/// (`display_warning`, blue) — Lightroom's destination and monitor gamut warnings.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Proof {
    pub space: OutputSpace,
    pub dest_warning: bool,
    pub display_warning: bool,
}

/// Warning colours (sRGB-encoded): destination gamut red, display gamut blue.
pub const PROOF_DEST_WARNING: [f32; 3] = [1.0, 0.0, 0.0];
pub const PROOF_DISPLAY_WARNING: [f32; 3] = [0.0, 0.25, 1.0];

/// [`Proof`] as the per-pixel stage uses it.
#[derive(Clone, Copy, Debug)]
pub struct ProofParams {
    /// Linear Rec.2020 → linear proof RGB, its luminance weights, linear proof → linear output RGB.
    pub to_proof: [[f32; 3]; 3],
    pub luma: [f32; 3],
    pub proof_to_out: [[f32; 3]; 3],
    pub dest_warning: bool,
    pub display_warning: bool,
}

impl Proof {
    pub fn key(self) -> u64 {
        (self.space as u64 + 1) | (self.dest_warning as u64) << 8 | (self.display_warning as u64) << 9
    }

    pub fn params(self, out: OutputSpace) -> ProofParams {
        ProofParams {
            to_proof: self.space.from_working(),
            luma: self.space.luma(),
            proof_to_out: self.space.rgb_space().to_space(&out.rgb_space()).to_f32(),
            dest_warning: self.dest_warning,
            display_warning: self.display_warning,
        }
    }
}

/// Sample format of a render.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum OutputDepth {
    /// 8-bit display-encoded ([`crate::Rendered::image`] only).
    #[default]
    U8,
    /// 16-bit display-encoded RGB in [`crate::Rendered::deep`].
    U16,
    /// 32-bit float *linear* RGB (target primaries, 0..1) in [`crate::Rendered::deep`].
    F32Linear,
}

/// High-bit-depth RGB samples (3 per pixel, interleaved, row-major).
#[derive(Clone, Debug, PartialEq)]
pub enum DeepSamples {
    U16(Vec<u16>),
    F32(Vec<f32>),
}

/// A high-bit-depth render ([`OutputDepth::U16`] / [`OutputDepth::F32Linear`]).
#[derive(Clone, Debug, PartialEq)]
pub struct DeepImage {
    pub width: usize,
    pub height: usize,
    pub space: OutputSpace,
    pub samples: DeepSamples,
}

impl DeepImage {
    /// The same image reduced to 8 bits, display-encoded with the space's curve.
    pub fn to_rgba8(&self) -> lightcraft_raster::Rgba8 {
        let mut out = lightcraft_raster::Rgba8::new(self.width, self.height);
        let trc = self.space.trc();
        match &self.samples {
            DeepSamples::U16(v) => {
                for (p, c) in out.data.iter_mut().zip(v.as_chunks::<3>().0) {
                    *p = [c[0], c[1], c[2]].map(|x| ((x as u32 * 255 + 32767) / 65535) as u8).into_rgba();
                }
            }
            DeepSamples::F32(v) => {
                for (p, c) in out.data.iter_mut().zip(v.as_chunks::<3>().0) {
                    *p = [c[0], c[1], c[2]].map(|x| (trc.encode(x) * 255.0 + 0.5) as u8).into_rgba();
                }
            }
        }
        out
    }
}

trait IntoRgba {
    fn into_rgba(self) -> [u8; 4];
}

impl IntoRgba for [u8; 3] {
    fn into_rgba(self) -> [u8; 4] {
        [self[0], self[1], self[2], 255]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trcs_invert() {
        for o in OutputSpace::ALL {
            let t = o.trc();
            for i in 0..=100 {
                let x = i as f32 / 100.0;
                assert!((t.decode(t.encode(x)) - x).abs() < 1e-5, "{o:?} {x}");
            }
        }
    }

    #[test]
    fn parse_and_serde_names() {
        for o in OutputSpace::ALL {
            let s = serde_json::to_value(o).unwrap();
            assert_eq!(OutputSpace::parse(s.as_str().unwrap()), Some(o));
            assert_eq!(OutputSpace::parse(o.label()), Some(o), "{}", o.label());
        }
    }

    #[test]
    fn white_maps_to_white() {
        for o in OutputSpace::ALL {
            let m = o.from_working();
            for row in m {
                assert!((row.iter().sum::<f32>() - 1.0).abs() < 1e-4, "{o:?}");
            }
            assert!((o.luma().iter().sum::<f32>() - 1.0).abs() < 1e-4);
        }
    }
}
