# Ported algorithms

Code in this repository that is a port (a re-implementation, usually in Rust) of an algorithm from
another project. Each ported file also names its source in its own module documentation, and each
upstream project has a notice in [`licenses/`](../licenses).

To add a port: add one row per ported upstream file (several rows may share our file), with the
full commit hash the port was made from (`git log -1 --format=%H -- <upstream path>` in a clone),
the upstream licence as an SPDX identifier, and the date of the port (YYYY-MM-DD). When a port is
updated to a newer upstream commit, update its row (commit and date) rather than adding another.

| Our file | Upstream project | Upstream path | Upstream commit | Licence | Date |
|---|---|---|---|---|---|
| `crates/lc-pipeline/src/negative.rs` | [darktable](https://github.com/darktable-org/darktable) | `src/iop/negadoctor.c` | `733bd69f32cac7ff5e41025115942772add1f088` | GPL-3.0-or-later | 2026-10-08 |
| `crates/li-seg/src/denoise.rs` | [darktable-ai](https://github.com/darktable-org/darktable-ai) | `models/rawdenoise-nind/demo.py` (`_run_tiled`, `_match_gain`) | `6bcd41c6f296ca692e6f845b25cf7cdb8148305c` (tag `release-5.6.0`) | GPL-3.0-only | 2026-10-09 |
| `crates/lc-raw/src/demosaic/rcd.rs` | [darktable](https://github.com/darktable-org/darktable) | `src/iop/demosaicing/rcd.c` | `733bd69f32cac7ff5e41025115942772add1f088` | GPL-3.0-or-later | 2026-10-09 |
| `crates/lc-raw/src/demosaic/dual.rs` | [darktable](https://github.com/darktable-org/darktable) | `src/iop/demosaicing/dual.c`, `src/develop/masks/detail.c` | `733bd69f32cac7ff5e41025115942772add1f088` | GPL-3.0-or-later | 2026-10-09 |
| `crates/lc-raw/src/highlight/opposed.rs` | [darktable](https://github.com/darktable-org/darktable) | `src/iop/hlreconstruct/opposed.c` | `733bd69f32cac7ff5e41025115942772add1f088` | GPL-3.0-or-later | 2026-10-09 |
| `crates/lc-raw/src/capture.rs` | [darktable](https://github.com/darktable-org/darktable) | `src/iop/demosaicing/capture.c` (radius estimation) | `733bd69f32cac7ff5e41025115942772add1f088` | GPL-3.0-or-later | 2026-10-09 |
| `crates/lc-pipeline/src/capture.rs` | [darktable](https://github.com/darktable-org/darktable) | `src/iop/demosaicing/capture.c` (deconvolution, blend mask) | `733bd69f32cac7ff5e41025115942772add1f088` | GPL-3.0-or-later | 2026-10-09 |
| `crates/lc-pipeline/src/toneeq.rs` | [darktable](https://github.com/darktable-org/darktable) | `src/iop/toneequal.c`, `src/iop/gaussian_elimination.h`, `src/common/luminance_mask.h` | `733bd69f32cac7ff5e41025115942772add1f088` | GPL-3.0-or-later | 2026-10-09 |
| `crates/lc-pipeline/src/colorcal.rs` | [darktable](https://github.com/darktable-org/darktable) | `src/iop/channelmixerrgb.c`, `src/common/chromatic_adaptation.h` | `733bd69f32cac7ff5e41025115942772add1f088` | GPL-3.0-or-later | 2026-10-09 |
| `crates/lc-pipeline/src/lensdb.rs` | [LensFun](https://github.com/lensfun/lensfun) (via the [`lensfun`](https://crates.io/crates/lensfun) crate 0.7.0) | `libs/lensfun/mod-coord.cpp`, `mod-subpix.cpp`, `mod-color.cpp` (correction models) | `43f9e001ab66c4fcdd4f400463f13b72f94b2288` | LGPL-3.0-or-later | 2026-10-09 |
| `crates/lc-engine/src/lens_db.rs` | [LensFun](https://github.com/lensfun/lensfun) (via the [`lensfun`](https://crates.io/crates/lensfun) crate 0.7.0) | `libs/lensfun/mod-coord.cpp`, `mod-subpix.cpp`, `mod-color.cpp` (`rescale_polynomial_coefficients`), `modifier.cpp` | `43f9e001ab66c4fcdd4f400463f13b72f94b2288` | LGPL-3.0-or-later | 2026-10-09 |
| `crates/lc-pipeline/src/tone2.rs`, `crates/lc-gpu/src/wgsl/finish.wgsl` | [darktable](https://github.com/darktable-org/darktable) | `src/iop/sigmoid.c` | `733bd69f32cac7ff5e41025115942772add1f088` | GPL-3.0-or-later | 2026-10-09 |
| `crates/lc-pipeline/src/base_curve_data.rs`, `basecurves.rs` | [darktable](https://github.com/darktable-org/darktable) | `src/iop/basecurve.c` | `733bd69f32cac7ff5e41025115942772add1f088` | GPL-3.0-or-later | 2026-10-09 |
| `crates/lc-pipeline/src/basecurves.rs` | [darktable](https://github.com/darktable-org/darktable) | `src/common/curve_tools.c` | `733bd69f32cac7ff5e41025115942772add1f088` | GPL-3.0-or-later | 2026-10-09 |
| `crates/lc-raw/src/camera_matrices.rs` | [rawler/dnglab](https://github.com/dnglab/dnglab) | `rawler/data/cameras/**/*.toml` | v0.8.0; immutable tree `ae01bcb2d0f8a74f9dfbb9f7b5c7e315c8e668b7` (offline archive has no commit metadata) | LGPL-2.1-only → GPL-3.0-or-later via §3 | 2026-10-09 |
| `crates/lc-raw/src/camera_matrices.rs` | [RawTherapee](https://github.com/RawTherapee/RawTherapee) | `rtengine/camconst.json` | `5f486d3678b34c74ba0c63571c17babe20935019` | GPL-3.0-or-later | 2026-10-09 |
| `crates/lc-color/src/profile.rs`, `camera.rs` | [RawTherapee](https://github.com/RawTherapee/RawTherapee) | `rtengine/dcp.cc` (table interpolation, matrix blending) | `5f486d3678b34c74ba0c63571c17babe20935019` | GPL-3.0-or-later | 2026-10-09 |
| `crates/lc-raw/data/dcp/` | [RawTherapee](https://github.com/RawTherapee/RawTherapee) | `rtdata/dcpprofiles/` (explicit embedded-rights allowlist) | `5f486d3678b34c74ba0c63571c17babe20935019` | CC0-1.0 / public domain; per-file rights + SHA-256 in manifest | 2026-10-09 |

Colour/tone notices: [`rawler-NOTICE.md`](../licenses/rawler-NOTICE.md),
[`rawtherapee-NOTICE.md`](../licenses/rawtherapee-NOTICE.md), and the colour/tone section of
[`darktable-NOTICE.md`](../licenses/darktable-NOTICE.md). `histmatching.cc` was inspected as a
robustness reference; no code was ported from it. Calibration archive fingerprints are committed
in `crates/lc-raw/data/colour-sources.json`. Reference harnesses are generated offline under
`target/refvec/`; only the small numeric fixtures and regeneration script are tracked.
