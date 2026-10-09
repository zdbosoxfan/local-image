# RawTherapee colour calibration and DCP data

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
