# li-color

The shared pure-Rust colour formulas for Local Image. Production dependencies are limited to
`serde`. ICC profile parsing, transforms and profile-dependent CMYK state remain in `pc-cms`
and `pc-color`; this crate does not depend on either of them.

Use explicit colour domains:

- `perceptual::oklab_from_2020` takes **linear Rec.2020**. `srgb_perceptual` takes **linear
  sRGB**, with f32 and f64 entry points. Neither takes encoded image values.
- `cms_math::{xyz_to_lab,lab_to_xyz}` take an explicit reference white. ICC's nominal/stored
  D50, chromaticity-derived D50 and D65 are distinct inputs.
- HSV/HSL adapters name degrees or turns. Grey thresholds and remembered UI hue are caller
  policy. Native-unit adapters avoid unnecessary rounding through another hue unit.
- Rec.601, Rec.709 and Rec.2020 luminance helpers preserve their respective coefficients.
  The caller decides whether the input is linear luminance or encoded-domain luma.
- `curve::Curve` retains the ICC API; `space::Trc`, `OutputTrc` and `OutputSpace` retain
  existing serialized/Copy APIs. They use shared formula kernels with explicit precision,
  clamping and extrapolation policies.

`lutfile::parse_with_domain` reads Cube, 3DL and unmasked baked Look tables, retaining Cube
input domains. Nonlinear 3DL shapers and masked Look files are rejected explicitly. Use
`lut::{normalize_domain,sample_rgb}` for table evaluation. Tetrahedral accumulation rounds
once from f64; trilinear remains available for existing document choices. GPU execution uses
the corresponding native-f32 shader implementation.

`lc-color` is a compatibility re-export facade. New code should import `li-color` directly
instead of adding private copies. Compatibility oracles and independent accuracy comparisons
cover the migration; Develop's existing reference vectors and render hashes remain unchanged.
