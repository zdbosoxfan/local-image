# RawTherapee: algorithms and colour data


Local Image is GPL-3.0-or-later (see `LICENSE`). AMaZE, RCD, dual demosaicing and capture
sharpening originated in [RawTherapee](https://github.com/RawTherapee/RawTherapee),
GPL-3.0-or-later. Production Rust ports use the darktable forks listed in
[PORTS](../docs/PORTS.md) and [darktable notice](darktable-NOTICE.md).

AMaZE (Aliasing Minimization and Zipper Elimination): copyright (c) 2008-2010
Emil Martinec; optimized for speed by Ingo Weyrich; incorporating ideas of Luis Sanz Rodrigues
and Paul Lee. The upstream source credits code dated May 27, 2010 and Ingo Weyrich's
January 25, 2016 modification. This is free software under GNU GPL version 3 or any later
version, supplied without warranty; see the repository `LICENSE` for the licence text.

The port `crates/lc-raw/src/demosaic/amaze.rs` follows darktable's complete scalar kernel.
Independent out-of-build scalar fixtures also extract RawTherapee
`rtengine/amaze_demosaic_RT.cc` at `5f486d3678b34c74ba0c63571c17babe20935019`.
`rtengine/rt_math.h` / `sleef.h` supply interpolation and scalar exponent helper semantics.
Fixtures account for RawTherapee's output scaling and negative-output clamp.

RCD: Luis Sanz Rodríguez; tiling by Ingo Weyrich. Dual demosaicing and capture sharpening:
Ingo Weyrich. `rtengine/dual_demosaic_RT.cc` at the same revision was read to compare its
full-VNG4 low branch with darktable's VNG-linear + two median passes. RawTherapee's actual
VNG4 kernel is absent from the provided offline reference files, so an independent RT VNG4
numerical comparison remains pending; see [CODEX-REPORT](../docs/wip/CODEX-REPORT.md).

No RawTherapee C/C++ is compiled or shipped as a dependency. Upstream functions run only in
ignored `target/refvec` harnesses to produce deterministic numerical fixtures. Complete
regeneration steps are in [raw fixture README](../crates/lc-raw/tests/fixtures/README.md).

## Colour calibration and DCP data

Upstream: <https://github.com/RawTherapee/RawTherapee>, snapshot
`5f486d3678b34c74ba0c63571c17babe20935019` (the coordinator's offline source archive).

- `rtengine/camconst.json` → gaps in `crates/lc-raw/src/camera_matrices.rs`:
  Copyright RawTherapee developers and camera calibration contributors, **GPL-3.0-or-later**.
  The dcraw matrix integers are divided by 10000; only cameras absent from rawler are added.
  A single daylight matrix stays single; no second illuminant is inferred.
- `rtengine/dcp.cc` → DCP table interpolation in `crates/lc-color/src/profile.rs` and
  dual-illuminant blending in `crates/lc-color/src/camera.rs`: Copyright (c) 2012
  **Oliver Duis**, RawTherapee developers, **GPL-3.0-or-later**.
  DNG chapter 6 supplies the independent colour model/container definitions; our TIFF reader
  reads profile tags without an SDK. C++ extracted reference harnesses test numeric parity.
  Differences: analytic sRGB transfer instead of RT's quantized global gamma tables; bounded
  table dimensions and malformed-file rejection; outputs above white keep scene headroom
  instead of clipping/scaling it upwards. A missing illuminant uses the available matrix.
  CameraCalibration and AnalogBalance are honored by the DNG colour model. Tone curves use
  our soft shoulder and a camera luminance norm rather than RT's display clipping.
- `rtdata/dcpprofiles/*.dcp` → `crates/lc-raw/data/dcp/`: **55 profiles** whose embedded
  ProfileCopyright tag explicitly says `public domain`, `RawTherapee CC0` or `CC0`.
  `manifest.json` records each profile's exact embedded rights, camera model, file name and
  SHA-256. The profiles are copied without changes. No profile with Maciej Dworak, Dr Slony,
  ambiguous rights or Adobe authorship is bundled. User-selected folders are read only.

`rtengine/histmatching.cc` (Copyright (c) 2018 Alberto Griggio, GPL-3.0-or-later) was a
read-only robustness reference. **No code was ported from it.** Our existing spatially paired,
held-out JPEG fit uses median bins, isotonic regression and validation instead of global CDF
matching: this preserves correspondences and rejects unrelated previews. Low-colour photos
with valid camera calibration can fit tone/chroma without fitting a new colour matrix.
