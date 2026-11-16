# Ported algorithms

Code in this repository that is a port (a re-implementation, usually in Rust) of an algorithm from
another project. Each ported file also names its source in its own module documentation, and each
upstream project has a notice in [`licenses/`](../licenses).

## Upstream review cutoffs

Every commit up to and including this one has been triaged; newer ones have not.
Move a cutoff with `cargo xtask upstream-reviewed <repo> <sha>`.

| Upstream | Reviewed up to |
|---|---|
| PhotoCraft | `2fec95b0d91bcd45a1a17c93196761880af47322` |
| LightCraft | `6b2b7e6d1117e683a1701568e56f979d061e4699` |
| VectorCraft | `895e9493d2dada35bca88044e06b6f63883b2405` |

## Ported files

To add a port: add one row per ported upstream file (several rows may share our file), with the
full commit hash the port was made from (`git log -1 --format=%H -- <upstream path>` in a clone),
the upstream licence as an SPDX identifier, and the date of the port (YYYY-MM-DD). When a port is
updated to a newer upstream commit, update its row (commit and date) rather than adding another.
A whole PhotoCraft, LightCraft or VectorCraft commit is ported with `cargo xtask port <repo> <sha>`,
which applies it at our paths (`docs/upstream-paths.toml`) and adds its row to the commit table at
the end of this file.

| Our file | Upstream project | Upstream path | Upstream commit | Licence | Date | Used for |
|---|---|---|---|---|---|---|
| `crates/lc-raw/src/vendor/arw6.rs` | [LibRaw](https://github.com/LibRaw/LibRaw) | `src/decoders/sony_arw6.cpp` | `830ac963b40223c6a25add5be65036fa8d56e2d6` | LGPL-2.1-only (chosen over CDDL-1.0) | 2026-10-14 | Bounds-checked pure-Rust Sony ARW6 normal/HQ packet, entropy, wavelet and complete inverse curve implementation. |
| `crates/lc-raw/src/vendor/arw.rs` | [LibRaw](https://github.com/LibRaw/LibRaw) | `src/metadata/sony.cpp`, `src/decoders/decoders_libraw.cpp` | `b93f6e45c194f5df9b02a43b1af9a54b4f41f33f` (0.22.2) | LGPL-2.1-only (chosen over CDDL-1.0) | 2026-10-14 | Sony per-file SR2 calibration cipher, ARQ sample layout and Sony RGB conversion reference. |
| `crates/lc-meta/src/sony.rs`, `crates/lc-meta/src/dji.rs` | [ExifTool](https://github.com/exiftool/exiftool) | `lib/Image/ExifTool/Sony.pm`, `lib/Image/ExifTool/DJI.pm`, `lib/Image/ExifTool/Exif.pm` | `2200871d9cef988051d2a99d67df3bda6cbb30a8` (13.59 snapshot; 13.40 reference also retained) | GPL-1.0-or-later (Perl licence option) | 2026-10-14 | Independently implemented Sony field layouts/cipher and DJI namespace semantics; reference-only Perl tool. |
| `crates/lc-merge/src/pixelshift.rs` | [RawTherapee](https://github.com/RawTherapee/RawTherapee) | `rtengine/pixelshift.cc` | `5f486d3678b34c74ba0c63571c17babe20935019` | GPL-3.0-or-later | 2026-10-14 | Independent Rust noise-aware repeated-green motion comparison; measured/profiled variance and first-shot fallback. |
| `crates/lc-merge/src/pixelshift.rs` | [darktable](https://github.com/darktable-org/darktable) | `src/iop/denoiseprofile.c` | `733bd69f32cac7ff5e41025115942772add1f088` | GPL-3.0-or-later | 2026-10-14 | Documented normalized sensor variance a×signal+b for motion thresholds. |
| `crates/lc-merge/src/pixelshift.rs` | [make_arq](https://github.com/agriggio/make_arq) | `make_arq.py` | `912069a8e6d3c6738cd76ba0fe6c62ed4d314fe3` | GPL-3.0-or-later | 2026-10-14 | Sony source shot displacement and half-pixel phase ordering (independent Rust reconstruction/motion implementation). |
| `crates/li-ai/src/builders.rs`, `crates/li-ai/examples/zimage_variety_check.rs` | [SplitMix64](https://github.com/svaarala/duktape) | `misc/splitmix64.c` | `84131700dfd32d9d1063f99b5b0a900f5935a613` | CC0-1.0 | 2026-10-10 | Stable seed-derived alternate conditioning for Z-Image Turbo Variety. |

| Our file | Upstream project | Upstream path | Upstream commit | Licence | Date |
|---|---|---|---|---|---|
| `crates/li-seg/src/denoise.rs` | [darktable-ai](https://github.com/darktable-org/darktable-ai) | `models/rawdenoise-nind/demo.py` | `f1a46e2d3feebc917a94f14705d128316fcf8114` | GPL-3.0 | 2026-10-10 |
| `crates/lc-pipeline/src/primary.rs`, `crates/lc-gpu/src/wgsl/primary.wgsl` | [vkdt](https://github.com/hanatos/vkdt) | `src/pipe/modules/llap/reduce.comp`, `assemble.comp` | `afc34256fb22bcaf6dcf6503d19d5f9a6f477af5` | BSD-2-Clause | 2026-10-09 |
| `crates/li-seg/src/faces.rs` | [OpenCV](https://github.com/opencv/opencv) | `modules/objdetect/src/face_detect.cpp` | `52100328d82d0502534323e9524a701baa3a1e2a` | Apache-2.0 | 2026-10-09 |
| `crates/li-seg/src/faces/geometry.rs` | [OpenCV](https://github.com/opencv/opencv) | `modules/objdetect/src/face_recognize.cpp` | `13c571a801ad5c67a752e5cd58a8a7e7725f99d2` | Apache-2.0 | 2026-10-09 |
| `crates/li-seg/src/bpe.rs` | [Local Image / LightCraft](https://github.com/zdbosoxfan/local-image) (Apache-2.0 tokenizer port from Hugging Face Transformers) | `crates/lc-segment/src/tokenizer.rs` | `c18d18cf613422fe08d31001e1f803cf8ef7dd68` | Apache-2.0 | 2026-10-09 |
| `crates/lc-pipeline/src/detail/nr.rs`, `crates/lc-gpu/src/wgsl/detail.wgsl` | [darktable](https://github.com/darktable-org/darktable) | `src/iop/denoiseprofile.c` | `733bd69f32cac7ff5e41025115942772add1f088` | GPL-3.0-or-later | 2026-10-09 |
| `crates/lc-pipeline/src/detail/nr.rs`, `crates/lc-gpu/src/wgsl/detail.wgsl` | [darktable](https://github.com/darktable-org/darktable) | `src/common/eaw.c` | `733bd69f32cac7ff5e41025115942772add1f088` | GPL-3.0-or-later | 2026-10-09 |
| `crates/lc-pipeline/src/detail/nr.rs`, `crates/lc-gpu/src/wgsl/detail.wgsl` | [darktable](https://github.com/darktable-org/darktable) | `src/common/math.h` | `733bd69f32cac7ff5e41025115942772add1f088` | GPL-3.0-or-later | 2026-10-09 |
| `crates/lc-pipeline/src/detail/haze.rs`, `crates/lc-gpu/src/wgsl/detail.wgsl` | [darktable](https://github.com/darktable-org/darktable) | `src/iop/hazeremoval.c` | `733bd69f32cac7ff5e41025115942772add1f088` | GPL-3.0-or-later | 2026-10-09 |
| `crates/lc-pipeline/src/detail/haze.rs`, `crates/lc-gpu/src/wgsl/detail.wgsl` | [darktable](https://github.com/darktable-org/darktable) | `src/common/guided_filter.c` | `733bd69f32cac7ff5e41025115942772add1f088` | GPL-3.0-or-later | 2026-10-09 |
| `crates/lc-pipeline/src/detail/haze.rs`, `crates/lc-gpu/src/wgsl/detail.wgsl` | [darktable](https://github.com/darktable-org/darktable) | `src/common/box_filters.cc` | `733bd69f32cac7ff5e41025115942772add1f088` | GPL-3.0-or-later | 2026-10-09 |
| `crates/lc-pipeline/src/negative.rs` | [darktable](https://github.com/darktable-org/darktable) | `src/iop/negadoctor.c` | `733bd69f32cac7ff5e41025115942772add1f088` | GPL-3.0-or-later | 2026-10-08 |
| `crates/lc-raw/src/demosaic/markesteijn.rs` | [darktable](https://github.com/darktable-org/darktable) | `src/iop/demosaicing/xtrans.c` | `733bd69f32cac7ff5e41025115942772add1f088` | GPL-3.0-or-later | 2026-10-13 |
| `crates/lc-raw/src/demosaic/markesteijn.rs`, `crates/lc-raw/src/demosaic/dual.rs` | [darktable](https://github.com/darktable-org/darktable) | `src/iop/demosaicing/vng.c`, `src/iop/demosaicing/dual.c` | `733bd69f32cac7ff5e41025115942772add1f088` | GPL-3.0-or-later | 2026-10-13 |
| `crates/lc-raw/src/demosaic/lmmse.rs` | [darktable](https://github.com/darktable-org/darktable) | `src/iop/demosaicing/lmmse.c` | `733bd69f32cac7ff5e41025115942772add1f088` | GPL-3.0-or-later | 2026-10-13 |
| `crates/lc-raw/src/demosaic/dcb.rs` | [RawTherapee](https://github.com/RawTherapee/RawTherapee) | `rtengine/demosaic_algos.cc` | `5f486d3678b34c74ba0c63571c17babe20935019` | BSD-3-Clause AND GPL-3.0-or-later | 2026-10-13 |
| `crates/lc-raw/src/demosaic/vng.rs` | [darktable](https://github.com/darktable-org/darktable) | `src/iop/demosaicing/vng.c` | `733bd69f32cac7ff5e41025115942772add1f088` | GPL-3.0-or-later | 2026-10-09 |
| `crates/lc-raw/src/demosaic/vng.rs` | [darktable](https://github.com/darktable-org/darktable) | `src/iop/demosaicing/basics.c` | `733bd69f32cac7ff5e41025115942772add1f088` | GPL-3.0-or-later | 2026-10-09 |
| `crates/lc-raw/src/demosaic/amaze.rs` | [darktable](https://github.com/darktable-org/darktable) | `src/iop/demosaicing/amaze.cc` | `733bd69f32cac7ff5e41025115942772add1f088` | GPL-3.0-or-later | 2026-10-09 |
| `crates/lc-raw/src/highlight/segbased.rs` | [darktable](https://github.com/darktable-org/darktable) | `src/iop/hlreconstruct/segbased.c` | `733bd69f32cac7ff5e41025115942772add1f088` | GPL-3.0-or-later | 2026-10-09 |
| `crates/lc-raw/src/highlight/segmentation.rs` | [darktable](https://github.com/darktable-org/darktable) | `src/iop/hlreconstruct/segmentation.c` | `733bd69f32cac7ff5e41025115942772add1f088` | GPL-3.0-or-later | 2026-10-09 |
| `crates/lc-raw/src/highlight/segbased.rs` | [darktable](https://github.com/darktable-org/darktable) | `src/iop/hlreconstruct/opposed.c` | `733bd69f32cac7ff5e41025115942772add1f088` | GPL-3.0-or-later | 2026-10-09 |
| `crates/lc-raw/src/highlight/segbased.rs` | [darktable](https://github.com/darktable-org/darktable) | `src/common/distance_transform.c` | `733bd69f32cac7ff5e41025115942772add1f088` | GPL-3.0-or-later | 2026-10-09 |
| `crates/lc-raw/src/highlight/segbased.rs` | [darktable](https://github.com/darktable-org/darktable) | `src/common/box_filters.cc` | `733bd69f32cac7ff5e41025115942772add1f088` | GPL-3.0-or-later | 2026-10-09 |
| `crates/lc-raw/src/highlight/segbased.rs` | [darktable](https://github.com/darktable-org/darktable) | `src/develop/noise_generator.h` | `733bd69f32cac7ff5e41025115942772add1f088` | GPL-3.0-or-later | 2026-10-09 |
| `crates/lc-raw/src/numerics.rs` | [darktable](https://github.com/darktable-org/darktable) | `src/common/gaussian.c` | `733bd69f32cac7ff5e41025115942772add1f088` | GPL-3.0-or-later | 2026-10-09 |
| `crates/lc-raw/src/numerics.rs` | [darktable](https://github.com/darktable-org/darktable) | `src/common/math.h` | `733bd69f32cac7ff5e41025115942772add1f088` | GPL-3.0-or-later | 2026-10-09 |
| `crates/lc-raw/src/demosaic/rcd.rs` | [darktable](https://github.com/darktable-org/darktable) | `src/iop/demosaicing/rcd.c` | `733bd69f32cac7ff5e41025115942772add1f088` | GPL-3.0-or-later | 2026-10-09 |
| `crates/lc-raw/src/demosaic/dual.rs` | [darktable](https://github.com/darktable-org/darktable) | `src/iop/demosaicing/dual.c`, `src/develop/masks/detail.c` | `733bd69f32cac7ff5e41025115942772add1f088` | GPL-3.0-or-later | 2026-10-09 |
| `crates/lc-raw/src/highlight/opposed.rs` | [darktable](https://github.com/darktable-org/darktable) | `src/iop/hlreconstruct/opposed.c` | `733bd69f32cac7ff5e41025115942772add1f088` | GPL-3.0-or-later | 2026-10-09 |
| `crates/lc-raw/src/capture.rs` | [darktable](https://github.com/darktable-org/darktable) | `src/iop/demosaicing/capture.c` (radius estimation) | `733bd69f32cac7ff5e41025115942772add1f088` | GPL-3.0-or-later | 2026-10-09 |
| `crates/lc-pipeline/src/capture.rs`, `crates/lc-gpu/src/capture.rs`, `crates/lc-gpu/src/wgsl/capture.wgsl` | [darktable](https://github.com/darktable-org/darktable) | `src/iop/demosaicing/capture.c` (deconvolution, blend mask) | `733bd69f32cac7ff5e41025115942772add1f088` | GPL-3.0-or-later | 2026-10-09 |
| `crates/lc-pipeline/src/toneeq.rs` | [darktable](https://github.com/darktable-org/darktable) | `src/iop/toneequal.c`, `src/iop/gaussian_elimination.h`, `src/common/luminance_mask.h` | `733bd69f32cac7ff5e41025115942772add1f088` | GPL-3.0-or-later | 2026-10-09 |
| `crates/lc-pipeline/src/colorcal.rs` | [darktable](https://github.com/darktable-org/darktable) | `src/iop/channelmixerrgb.c`, `src/common/chromatic_adaptation.h` | `733bd69f32cac7ff5e41025115942772add1f088` | GPL-3.0-or-later | 2026-10-09 |
| `crates/lc-raw/src/embedded_lens.rs` | [darktable](https://github.com/darktable-org/darktable) | `src/common/exif.cc` (`_check_lens_correction_data`; vendor metadata layouts) | `733bd69f32cac7ff5e41025115942772add1f088` | GPL-3.0-or-later | 2026-10-14 |
| `crates/lc-engine/src/embedded_lens.rs`, `crates/lc-pipeline/src/optics.rs`, `crates/lc-gpu/src/wgsl/geom.wgsl` | [darktable](https://github.com/darktable-org/darktable) | `src/iop/lens.cc` (`_init_coeffs_md_v2`, `_process_md`; Sony/Fuji/Olympus/Panasonic radial models) | `733bd69f32cac7ff5e41025115942772add1f088` | GPL-3.0-or-later | 2026-10-14 |
| `crates/lc-pipeline/src/lensdb.rs` | [LensFun](https://github.com/lensfun/lensfun) (via the [`lensfun`](https://crates.io/crates/lensfun) crate 0.7.0) | `libs/lensfun/mod-coord.cpp`, `mod-subpix.cpp`, `mod-color.cpp` (correction models) | `43f9e001ab66c4fcdd4f400463f13b72f94b2288` | LGPL-3.0-or-later | 2026-10-09 |
| `crates/lc-engine/src/lens_db.rs` | [LensFun](https://github.com/lensfun/lensfun) (via the [`lensfun`](https://crates.io/crates/lensfun) crate 0.7.0) | `libs/lensfun/mod-coord.cpp`, `mod-subpix.cpp`, `mod-color.cpp` (`rescale_polynomial_coefficients`), `modifier.cpp` | `43f9e001ab66c4fcdd4f400463f13b72f94b2288` | LGPL-3.0-or-later | 2026-10-09 |
| `crates/lc-pipeline/src/tone2.rs`, `crates/lc-gpu/src/wgsl/finish.wgsl` | [darktable](https://github.com/darktable-org/darktable) | `src/iop/sigmoid.c` | `733bd69f32cac7ff5e41025115942772add1f088` | GPL-3.0-or-later | 2026-10-09 |
| `crates/lc-pipeline/src/base_curve_data.rs`, `basecurves.rs` | [darktable](https://github.com/darktable-org/darktable) | `src/iop/basecurve.c` | `733bd69f32cac7ff5e41025115942772add1f088` | GPL-3.0-or-later | 2026-10-09 |
| `crates/lc-pipeline/src/basecurves.rs` | [darktable](https://github.com/darktable-org/darktable) | `src/common/curve_tools.c` | `733bd69f32cac7ff5e41025115942772add1f088` | GPL-3.0-or-later | 2026-10-09 |
| `crates/lc-raw/src/camera_matrices.rs` | [rawler](https://github.com/dnglab/dnglab) | `rawler/data/cameras/**/*.toml` | v0.8.0; immutable tree `ae01bcb2d0f8a74f9dfbb9f7b5c7e315c8e668b7` (offline archive has no commit metadata) | LGPL-2.1-only → GPL-3.0-or-later via §3 | 2026-10-09 |
| `crates/lc-raw/src/camera_matrices.rs` | [RawTherapee](https://github.com/RawTherapee/RawTherapee) | `rtengine/camconst.json` | `5f486d3678b34c74ba0c63571c17babe20935019` | GPL-3.0-or-later | 2026-10-09 |
| `crates/li-color/src/profile.rs`, `camera.rs` | [RawTherapee](https://github.com/RawTherapee/RawTherapee) | `rtengine/dcp.cc` (table interpolation, matrix blending) | `5f486d3678b34c74ba0c63571c17babe20935019` | GPL-3.0-or-later | 2026-10-09 |
| `crates/lc-raw/data/dcp/` | [RawTherapee](https://github.com/RawTherapee/RawTherapee) | `rtdata/dcpprofiles/` (explicit embedded-rights allowlist) | `5f486d3678b34c74ba0c63571c17babe20935019` | CC0-1.0 / public domain; per-file rights + SHA-256 in manifest | 2026-10-09 |
| `crates/lc-raw/data/color/` | [CIE](https://cie.co.at/datatable/cie-1931-colour-matching-functions-2-degree-observer) | CIE 018:2019 Table 6, complete CIE_xyz_1931_2deg.csv and metadata | DOI 10.25039/CIE.DS.xvudnb9b; SHA-256 fa663e3535a7e0763a745993a1f0a192eb0275ac46ad2d1befd7626841e713c1 | CC-BY-SA-4.0 (separate data work) | 2026-10-14 |
| `crates/lc-raw/src/lcp.rs`, `crates/lc-pipeline/src/lcp.rs` | [RawTherapee](https://github.com/RawTherapee/RawTherapee) | rtengine/lcp.cc, interpretation of Adobe Camera Model (2010); original XML parser | `c6d04960e14fed89e531cbbdf680021d131b4685`; Adobe Camera Model 2010 PDF | GPL-3.0-or-later | 2026-10-14 |
| `crates/pc-pathops/src/geom/path.rs` | [VectorCraft](https://github.com/storytold/vectorcraft) | `crates/geom/src/path.rs` | `d522c1d7be4035bd4f4a84cd6ebfca44f5155092` | MIT OR Apache-2.0 | 2026-10-09 |
| `crates/pc-pathops/src/adapter.rs` | [VectorCraft](https://github.com/storytold/vectorcraft) | `crates/geom/src/path.rs` | `d522c1d7be4035bd4f4a84cd6ebfca44f5155092` | MIT OR Apache-2.0 | 2026-10-09 |
| `crates/pc-pathops/src/geom/bez.rs` | [VectorCraft](https://github.com/storytold/vectorcraft) | `crates/geom/src/bez.rs` | `d522c1d7be4035bd4f4a84cd6ebfca44f5155092` | MIT OR Apache-2.0 | 2026-10-09 |
| `crates/pc-pathops/src/geom/arc.rs` | [VectorCraft](https://github.com/storytold/vectorcraft) | `crates/geom/src/arc.rs` | `d522c1d7be4035bd4f4a84cd6ebfca44f5155092` | MIT OR Apache-2.0 | 2026-10-09 |
| `crates/pc-pathops/src/geom/hit.rs` | [VectorCraft](https://github.com/storytold/vectorcraft) | `crates/geom/src/hit.rs` | `d522c1d7be4035bd4f4a84cd6ebfca44f5155092` | MIT OR Apache-2.0 | 2026-10-09 |
| `crates/pc-pathops/src/geom/shapes.rs` | [VectorCraft](https://github.com/storytold/vectorcraft) | `crates/geom/src/shapes.rs` | `d522c1d7be4035bd4f4a84cd6ebfca44f5155092` | MIT OR Apache-2.0 | 2026-10-09 |
| `crates/pc-pathops/src/boolean.rs` | [VectorCraft](https://github.com/storytold/vectorcraft) | `crates/pathops/src/boolean.rs` | `d522c1d7be4035bd4f4a84cd6ebfca44f5155092` | MIT OR Apache-2.0 | 2026-10-09 |
| `crates/pc-pathops/src/pathfinder.rs` | [VectorCraft](https://github.com/storytold/vectorcraft) | `crates/pathops/src/pathfinder.rs` | `d522c1d7be4035bd4f4a84cd6ebfca44f5155092` | MIT OR Apache-2.0 | 2026-10-09 |
| `crates/pc-pathops/src/planar.rs` | [VectorCraft](https://github.com/storytold/vectorcraft) | `crates/pathops/src/planar.rs` | `d522c1d7be4035bd4f4a84cd6ebfca44f5155092` | MIT OR Apache-2.0 | 2026-10-09 |
| `crates/pc-pathops/src/offset.rs` | [VectorCraft](https://github.com/storytold/vectorcraft) | `crates/pathops/src/offset.rs` | `d522c1d7be4035bd4f4a84cd6ebfca44f5155092` | MIT OR Apache-2.0 | 2026-10-09 |
| `crates/pc-pathops/src/edit.rs` | [VectorCraft](https://github.com/storytold/vectorcraft) | `crates/pathops/src/edit.rs` | `d522c1d7be4035bd4f4a84cd6ebfca44f5155092` | MIT OR Apache-2.0 | 2026-10-09 |
| `crates/pc-pathops/src/fit.rs` | [VectorCraft](https://github.com/storytold/vectorcraft) | `crates/pathops/src/fit.rs` | `d522c1d7be4035bd4f4a84cd6ebfca44f5155092` | MIT OR Apache-2.0 | 2026-10-09 |
| `crates/pc-pathops/tests/upstream_ops.rs` | [VectorCraft](https://github.com/storytold/vectorcraft) | `crates/pathops/tests/ops.rs` | `d522c1d7be4035bd4f4a84cd6ebfca44f5155092` | MIT OR Apache-2.0 | 2026-10-09 |
| `crates/pc-pathops/tests/upstream_regressions.rs` | [VectorCraft](https://github.com/storytold/vectorcraft) | `crates/pathops/tests/regressions.rs` | `d522c1d7be4035bd4f4a84cd6ebfca44f5155092` | MIT OR Apache-2.0 | 2026-10-09 |
| `crates/pc-pathops/tests/upstream_props.rs` | [VectorCraft](https://github.com/storytold/vectorcraft) | `crates/pathops/tests/props.rs` | `d522c1d7be4035bd4f4a84cd6ebfca44f5155092` | MIT OR Apache-2.0 | 2026-10-09 |
| `crates/pc-pathops/tests/common/mod.rs` | [VectorCraft](https://github.com/storytold/vectorcraft) | `crates/pathops/tests/common/mod.rs` | `d522c1d7be4035bd4f4a84cd6ebfca44f5155092` | MIT OR Apache-2.0 | 2026-10-09 |
| `crates/pc-pathops/src/geom/corners.rs` | [VectorCraft](https://github.com/storytold/vectorcraft) | `crates/geom/src/corners.rs` | `d522c1d7be4035bd4f4a84cd6ebfca44f5155092` | MIT OR Apache-2.0 | 2026-10-09 |
| `crates/pc-pathops/tests/support/geom.rs` | [VectorCraft](https://github.com/storytold/vectorcraft) | `crates/testkit/src/geom.rs` | `d522c1d7be4035bd4f4a84cd6ebfca44f5155092` | MIT OR Apache-2.0 | 2026-10-09 |
| `crates/pc-pathops/tests/support/strategies.rs` | [VectorCraft](https://github.com/storytold/vectorcraft) | `crates/testkit/src/strategies.rs` | `d522c1d7be4035bd4f4a84cd6ebfca44f5155092` | MIT OR Apache-2.0 | 2026-10-09 |
| `crates/pc-pathops/tests/upstream_prop_pathops.rs` | [VectorCraft](https://github.com/storytold/vectorcraft) | `crates/pathops/tests/prop_pathops.rs` | `d522c1d7be4035bd4f4a84cd6ebfca44f5155092` | MIT OR Apache-2.0 | 2026-10-09 |
| `crates/pc-pathops/tests/upstream_prop_geom.rs` | [VectorCraft](https://github.com/storytold/vectorcraft) | `crates/geom/tests/prop_geom.rs` | `d522c1d7be4035bd4f4a84cd6ebfca44f5155092` | MIT OR Apache-2.0 | 2026-10-09 |
| `crates/pc-engine/tests/vector_v1.rs` | [VectorCraft](https://github.com/storytold/vectorcraft) | `crates/engine/src/tests_pathops.rs` | `d522c1d7be4035bd4f4a84cd6ebfca44f5155092` | MIT OR Apache-2.0 | 2026-10-09 |
| `crates/pc-engine/tests/vector_v1.rs` | [VectorCraft](https://github.com/storytold/vectorcraft) | `crates/engine/src/tests_outlinestroke.rs` | `d522c1d7be4035bd4f4a84cd6ebfca44f5155092` | MIT OR Apache-2.0 | 2026-10-09 |
| `crates/pc-pathops/src/kernel.rs` | [VectorCraft](https://github.com/storytold/vectorcraft) | `crates/pathops/src/lib.rs` | `d522c1d7be4035bd4f4a84cd6ebfca44f5155092` | MIT OR Apache-2.0 | 2026-10-09 |

| Our file | Upstream project | Upstream path | Upstream commit | Licence | Date |
|---|---|---|---|---|---|
| `crates/pc-trace/src/vc/color_clusters/builder.rs` | [visioncortex](https://github.com/visioncortex/visioncortex) | `src/color_clusters/builder.rs` | `0062088c89645aac76c00e066deb7e8f53980dd7` | MIT OR Apache-2.0 | 2026-10-09 |
| `crates/pc-trace/src/vc/color_clusters/cluster.rs` | [visioncortex](https://github.com/visioncortex/visioncortex) | `src/color_clusters/cluster.rs` | `0062088c89645aac76c00e066deb7e8f53980dd7` | MIT OR Apache-2.0 | 2026-10-09 |
| `crates/pc-trace/src/vc/color_clusters/container.rs` | [visioncortex](https://github.com/visioncortex/visioncortex) | `src/color_clusters/container.rs` | `0062088c89645aac76c00e066deb7e8f53980dd7` | MIT OR Apache-2.0 | 2026-10-09 |
| `crates/pc-trace/src/vc/color_clusters/runner.rs` | [visioncortex](https://github.com/visioncortex/visioncortex) | `src/color_clusters/runner.rs` | `0062088c89645aac76c00e066deb7e8f53980dd7` | MIT OR Apache-2.0 | 2026-10-09 |
| `crates/pc-trace/src/vc/color.rs` | [visioncortex](https://github.com/visioncortex/visioncortex) | `src/color.rs` | `0062088c89645aac76c00e066deb7e8f53980dd7` | MIT OR Apache-2.0 | 2026-10-09 |
| `crates/pc-trace/src/vc/bound.rs` | [visioncortex](https://github.com/visioncortex/visioncortex) | `src/bound.rs` | `0062088c89645aac76c00e066deb7e8f53980dd7` | MIT OR Apache-2.0 | 2026-10-09 |
| `crates/pc-trace/src/frontend/binary.rs` | [vtracer](https://github.com/visioncortex/vtracer) | `crates/vtracer/src/frontend/binary.rs` | `928ed0a6f654408e28fb741b6133d4c456bd0160` | MIT OR Apache-2.0 | 2026-10-09 |
| `crates/pc-trace/src/frontend/color_cluster.rs` | [vtracer](https://github.com/visioncortex/vtracer) | `crates/vtracer/src/frontend/color_cluster.rs` | `928ed0a6f654408e28fb741b6133d4c456bd0160` | MIT OR Apache-2.0 | 2026-10-09 |
| `crates/pc-trace/src/frontend/keying.rs` | [vtracer](https://github.com/visioncortex/vtracer) | `crates/vtracer/src/frontend/keying.rs` | `928ed0a6f654408e28fb741b6133d4c456bd0160` | MIT OR Apache-2.0 | 2026-10-09 |
| `crates/pc-trace/src/frontend/watershed.rs` | [vtracer](https://github.com/visioncortex/vtracer) | `crates/vtracer/src/frontend/watershed.rs` | `928ed0a6f654408e28fb741b6133d4c456bd0160` | MIT OR Apache-2.0 | 2026-10-09 |
| `crates/pc-trace/src/colorfit/oklab.rs` | [vtracer](https://github.com/visioncortex/vtracer) | `crates/vtracer/src/colorfit/oklab.rs` | `928ed0a6f654408e28fb741b6133d4c456bd0160` | MIT OR Apache-2.0 | 2026-10-09 |
| `crates/pc-trace/src/colorfit/quantize.rs` | [vtracer](https://github.com/visioncortex/vtracer) | `crates/vtracer/src/colorfit/quantize.rs` | `928ed0a6f654408e28fb741b6133d4c456bd0160` | MIT OR Apache-2.0 | 2026-10-09 |
| `crates/pc-trace/src/colorfit/palette.rs` | [vtracer](https://github.com/visioncortex/vtracer) | `crates/vtracer/src/colorfit/palette.rs` | `928ed0a6f654408e28fb741b6133d4c456bd0160` | MIT OR Apache-2.0 | 2026-10-09 |
| `crates/pc-trace/src/ir.rs` | [vtracer](https://github.com/visioncortex/vtracer) | `crates/vtracer/src/ir/region.rs` | `928ed0a6f654408e28fb741b6133d4c456bd0160` | MIT OR Apache-2.0 | 2026-10-09 |
| `crates/pc-testkit/src/vector/mod.rs` | [vtracer](https://github.com/visioncortex/vtracer) | `crates/vtracer-bench/src/lib.rs` | `928ed0a6f654408e28fb741b6133d4c456bd0160` | MIT OR Apache-2.0 | 2026-10-09 |
| `crates/pc-trace/src/spline.rs` | [visioncortex](https://github.com/visioncortex/visioncortex) | `src/path/simplify.rs` | `0062088c89645aac76c00e066deb7e8f53980dd7` | MIT OR Apache-2.0 | 2026-10-09 |
| `crates/pc-trace/src/spline.rs` | [visioncortex](https://github.com/visioncortex/visioncortex) | `src/path/smooth.rs` | `0062088c89645aac76c00e066deb7e8f53980dd7` | MIT OR Apache-2.0 | 2026-10-09 |
| `crates/pc-trace/src/spline.rs` | [visioncortex](https://github.com/visioncortex/visioncortex) | `src/path/spline.rs` | `0062088c89645aac76c00e066deb7e8f53980dd7` | MIT OR Apache-2.0 | 2026-10-09 |
| `crates/pc-trace/src/spline.rs` | [vtracer](https://github.com/visioncortex/vtracer) | `crates/vtracer/src/simplify.rs` | `928ed0a6f654408e28fb741b6133d4c456bd0160` | MIT OR Apache-2.0 | 2026-10-09 |
| `crates/pc-trace/src/potrace.rs` | [potrace](https://potrace.sourceforge.net/) | `src/trace.c` | `potrace-1.16.tar.gz; sha256 be8248a17dedd6ccbaab2fcc45835bb0502d062e40fbded3bc56028ce5eb7acc` | GPL-2.0-or-later | 2026-10-09 |

Colour/tone notices: [`rawler-NOTICE.md`](../licenses/rawler-NOTICE.md),
[`rawtherapee-NOTICE.md`](../licenses/rawtherapee-NOTICE.md), and the colour/tone section of
[`darktable-NOTICE.md`](../licenses/darktable-NOTICE.md). `histmatching.cc` was inspected as a
robustness reference; no code was ported from it. Calibration archive fingerprints are committed
in `crates/lc-raw/data/colour-sources.json`. Reference harnesses are generated offline under
`target/refvec/`; only the small numeric fixtures and regeneration script are tracked.


The AMaZE kernel originated in RawTherapee (Emil Martinec, Ingo Weyrich). The port follows
its darktable fork; independent scalar fixtures also compare against RawTherapee
`rtengine/amaze_demosaic_RT.cc` at `5f486d3678b34c74ba0c63571c17babe20935019`.
See [RawTherapee notice](../licenses/rawtherapee-NOTICE.md).

The upstream numerical fixtures and complete extraction/regeneration recipes are in
[raw fixtures](../crates/lc-raw/tests/fixtures/README.md) and
[pipeline fixtures](../crates/lc-pipeline/tests/fixtures/README.md).
Tolerances, measured discrepancies, intentional host differences, synthetic quality metrics,
timings and remaining reference limitations were recorded in a wip report that now lives only in git history.

Detail XMP import (`lc-engine/src/crs.rs`): Sharpness → Amount (0–150),
SharpenRadius → Radius (0.5–3 original pixels), SharpenDetail → Detail,
SharpenEdgeMasking → Masking, LuminanceSmoothing → Luminance Amount,
LuminanceNoiseReductionDetail → Luminance Detail,
LuminanceNoiseReductionContrast → Luminance Contrast,
ColorNoiseReduction → Colour Amount, ColorNoiseReductionDetail → Colour Detail,
ColorNoiseReductionSmoothness → Colour Smoothness (the last eight: 0–100).
Every field is clamped at import; absent/non-finite fields are ignored. Adobe
ProcessVersion metadata does not select a Local Image renderer.


## Primary sliders (2026-10-09)

All darktable references below are pinned to `733bd69f32cac7ff5e41025115942772add1f088`
(GPL-3.0-or-later). They continue the previous builder's UCS/local-Laplacian work.

| Rust stage | Upstream kernels | Integration / deliberate extensions |
|---|---|---|
| `ucs.rs` | `colorspaces_inline_conversions.h`, `darktable_ucs_22_helpers.h` | D65 Rec.2020 directly; once-built 512-bin gamut LUT; fill any empty sampled bins from neighbours. |
| `llf.rs` | `common/locallaplacian.c` | Same padded reductions, remap, interpolation and reconstruction. Tiny images return identity; the unused fourth SIMD lane is bounded rather than reading beyond the row. LI extensions: the γ sample count is a parameter, and samples bracketing no input value are skipped (exact); Clarity uses 48 γ samples over −12..+12 EV of log luminance with per-level weights and a 0.38 detail gain, computed in one pass as the Laplacians of upstream's midtone detail term on a zero residual (equal to the filter with and without that term, differenced). |
| `eigf.rs` | `common/eigf.h`, `gaussian.c`, `fast_guided_filter.h` | Quantized/self-guided paths, Deriche endpoints/clamps, upstream bilinear coordinates, normalized variance/covariance and linear/geometric final blend. Zero-size downsample protection. |
| `balance.rs` | `iop/colorbalancergb.c`, `common/math.h` | Full selected pixel algorithm: Yrg chroma/vibrance, masked wheels, polynomial power, UCS and alternate Jz saturation/brilliance, hue rotation and gamut mapping. LR slider calibration is our own. Jz PQ is evaluated in f64 to avoid neutral cancellation; C HDR vectors check it. |
| `colorequal.rs` | `iop/colorequal.c` | UCS chromaticity prefilter, complete 2×2 covariance, correction filter, saturation weights and Scharr halo suppression. LR eight hue nodes replace upstream's evenly spaced nodes, interpolated by a periodic monotone cubic instead of upstream's cosine RBF (which overshoots on uneven nodes); brightness is a bounded EV change instead of upstream's linear factor. |
| `toneeq.rs` | `iop/toneequal.c`, `common/luminance_mask.h`, `common/eigf.h` | The formerly simplified log mask now uses compensated linear norm, quantized EIGF and geometric blend. Existing fitted/interpolated exposure curve remains. |
| `primary.rs` | vkdt `llap/reduce.comp`, `assemble.comp`, sampled Laplacian construction | vkdt `afc34256fb22bcaf6dcf6503d19d5f9a6f477af5`, BSD-2-Clause. A fixed 19-sample γ grid from −14 to +8 EV (1.22 EV apart); log-EV remap, Hermite tone bands, clamp-edge rectangular pyramid to 1 px, fixed 2 MP proxy, guided gain upsampling and sensor clipping protection are LI extensions. |

See [vkdt notice](../licenses/vkdt-NOTICE.md), [darktable notice](../licenses/darktable-NOTICE.md),
[reference fixtures](../crates/lc-pipeline/tests/fixtures/sliders/README.md). Skin Tone is our own UCS low-frequency
uniformity design. Texture/Structure use 4–16 and 2–4 original sensor-pixel bands;
subpixel bands use a signed guided filter instead of EIGF's mandatory minimum radius.
Primary tone/colour/detail/skin/CAT layers have native WGSL twins in `lc-gpu/src/wgsl/primary.wgsl`, `primary_colour.wgsl` and `ucs.wgsl`. CPU algorithms and fixture tolerances are unchanged. LI Tone caches four linear response fields before its bounded gain clamp; this changes f32 association only, checked against the unchanged equivalence bounds. The Deriche sweeps use tiled transposes, preserving endpoint and recurrence order.
The faithful tone equalizer has a CPU spatial/finish stage after GPU geometry/WB/NR.

| Our file | Upstream project | Upstream path | Upstream commit | Licence | Date | Used for |
|---|---|---|---|---|---|---|
| `crates/li-seg/src/restoration.rs` | [DDColor](https://github.com/piddnad/DDColor) | `ddcolor/pipeline.py` | `2adb63f2656ac41cbdf7b894cddd94121a3faf13` | Apache-2.0 | 2026-10-10 | Neutral Lab input, original luminance + predicted chroma, nearest chroma resize. |
| `crates/li-seg/src/restoration.rs` | [Microsoft old-photo restoration](https://github.com/microsoft/Bringing-Old-Photos-Back-to-Life) | `Global/detection.py` | `33875eccf4ebcd3665cf38cc56f3a0ce563d3a9c` | MIT | 2026-10-10 | Grayscale normalisation, short-side detector scale, probability threshold and nearest mask. |
| `scripts/export_restore_models.py` | [DDColor](https://github.com/piddnad/DDColor) | `scripts/export_onnx.py` | `2adb63f2656ac41cbdf7b894cddd94121a3faf13` | Apache-2.0 | 2026-10-10 | Out-of-build FP32 export of the author's relicensed HF weights, with strict loading and frozen spectral weights. |
## Vector persona data

| Our file | Upstream project | Upstream path | Upstream commit | Licence | Date |
|---|---|---|---|---|---|
| `crates/pc-doc/src/vector_design/appearance.rs` | [VectorCraft](https://github.com/storytold/vectorcraft) | `crates/doc/src/appearance.rs` (data design, adapted to Local Image) | `d522c1d7be4035bd4f4a84cd6ebfca44f5155092` | MIT OR Apache-2.0 | 2026-10-10 |
| `crates/pc-doc/src/vector_design/live.rs` | [VectorCraft](https://github.com/storytold/vectorcraft) | `crates/doc/src/live.rs` (data design, adapted to Local Image) | `d522c1d7be4035bd4f4a84cd6ebfca44f5155092` | MIT OR Apache-2.0 | 2026-10-10 |

## PhotoCraft Vector V0 sync (2026-10-10)

The initial Local Image import (`c18d18cf613422fe08d31001e1f803cf8ef7dd68`)
already included #865 (Direct Selection) and #747 (saturated pixel bounds), including their
upstream tests. Those rows document existing ports audited during V0. #1321 was missing and is
ported here, with its vector tests and compositor parity test. The previous builder's #1495
sample-depth port is retained on the current translation-aware, bounded split cache.
The retained crash checkpoint (`bc6d7e28`) also contains upstream #1807 (issue #992),
which bakes feather/density into a rasterized vector mask. Its two rows use that newer
revision, which retains the #865 and #747 changes. These are pinned changes, rather than
claims that entire files match each revision.
The inherited PhotoCraft licence notices remain in `licenses/photocraft-*`.

| Our file | Upstream project | Upstream path | Upstream commit | Licence | Date |
|---|---|---|---|---|---|
| `crates/pc-vector/src/edit.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `crates/vector/src/edit.rs` | `75ff41a26beb445413f6ee9248eb40b42152bd77` | MIT OR Apache-2.0 | 2026-10-10 |
| `crates/pc-engine/src/path_edit_cmds.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `crates/engine/src/path_edit_cmds.rs` | `75ff41a26beb445413f6ee9248eb40b42152bd77` | MIT OR Apache-2.0 | 2026-10-10 |
| `crates/pc-engine/src/vector_cmds.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `crates/engine/src/vector_cmds.rs` | `d4884d33fa83486be13172dc7c9a0182a7ac94c3` | MIT OR Apache-2.0 | 2026-10-10 |
| `crates/pc-ui-egui/src/direct_select.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `crates/ui-egui/src/direct_select.rs` | `75ff41a26beb445413f6ee9248eb40b42152bd77` | MIT OR Apache-2.0 | 2026-10-10 |
| `crates/pc-ui-egui/src/direct_select_tests.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `crates/ui-egui/src/direct_select_tests.rs` | `75ff41a26beb445413f6ee9248eb40b42152bd77` | MIT OR Apache-2.0 | 2026-10-10 |
| `crates/pc-ui-egui/src/vector_ui.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `crates/ui-egui/src/vector_ui.rs` | `75ff41a26beb445413f6ee9248eb40b42152bd77` | MIT OR Apache-2.0 | 2026-10-10 |
| `crates/pc-ui-egui/src/state.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `crates/ui-egui/src/state.rs` | `75ff41a26beb445413f6ee9248eb40b42152bd77` | MIT OR Apache-2.0 | 2026-10-10 |
| `crates/pc-ui-egui/src/canvas.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `crates/ui-egui/src/canvas.rs` | `75ff41a26beb445413f6ee9248eb40b42152bd77` | MIT OR Apache-2.0 | 2026-10-10 |
| `crates/pc-vector/src/lib.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `crates/vector/src/lib.rs` | `d438f842256435dba7aa41ca306656a216b98fbf` | MIT OR Apache-2.0 | 2026-10-10 |
| `crates/pc-vector/src/tests.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `crates/vector/src/tests.rs` | `d438f842256435dba7aa41ca306656a216b98fbf` | MIT OR Apache-2.0 | 2026-10-10 |
| `crates/pc-compose/src/tests.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `crates/compose/src/tests.rs` | `d438f842256435dba7aa41ca306656a216b98fbf` | MIT OR Apache-2.0 | 2026-10-10 |
| `crates/pc-vector/src/raster.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `crates/vector/src/raster.rs` | `8302f57190c83439adb961ae23b9b110e819dea3` | MIT OR Apache-2.0 | 2026-10-10 |
| `crates/pc-engine/src/vector_cmds/tests.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `crates/engine/src/vector_cmds/tests.rs` | `d4884d33fa83486be13172dc7c9a0182a7ac94c3` | MIT OR Apache-2.0 | 2026-10-10 |
| `crates/pc-compose/src/lib.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `crates/compose/src/lib.rs` | `5355221f81e325a8682c7e3a7e03dd66cc00b5e6` | MIT OR Apache-2.0 | 2026-10-10 |
| `crates/pc-compose/src/shape_split.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `crates/compose/src/shape_split.rs` | `5355221f81e325a8682c7e3a7e03dd66cc00b5e6` | MIT OR Apache-2.0 | 2026-10-10 |
| `crates/pc-compose/src/stroke_tests.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `crates/compose/src/stroke_tests.rs` | `5355221f81e325a8682c7e3a7e03dd66cc00b5e6` | MIT OR Apache-2.0 | 2026-10-10 |
| `crates/pc-gpu/src/plan.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `crates/gpu/src/plan.rs` | `5355221f81e325a8682c7e3a7e03dd66cc00b5e6` | MIT OR Apache-2.0 | 2026-10-10 |
| `crates/pc-gpu/tests/gpu/parity.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `crates/gpu/tests/parity.rs` | `5355221f81e325a8682c7e3a7e03dd66cc00b5e6` | MIT OR Apache-2.0 | 2026-10-10 |

## Vector integration ports

| Our file | Upstream project | Upstream path | Upstream commit | Licence | Date |
|---|---|---|---|---|---|
| `crates/pc-vector/src/design_shapes.rs` | [VectorCraft](https://github.com/storytold/vectorcraft) | `crates/geom/src/shapes.rs` | `d522c1d7be4035bd4f4a84cd6ebfca44f5155092` | MIT OR Apache-2.0 | 2026-10-10 |
| `crates/pc-ui-egui/src/vector_packages/drawing.rs` | [VectorCraft](https://github.com/storytold/vectorcraft) | `crates/tools/src/shape.rs` | `d522c1d7be4035bd4f4a84cd6ebfca44f5155092` | MIT OR Apache-2.0 | 2026-10-10 |
| `crates/pc-ui-egui/src/direct_select.rs` | [VectorCraft](https://github.com/storytold/vectorcraft) | `crates/tools/src/direct.rs` (multi-curve node gestures) | `d522c1d7be4035bd4f4a84cd6ebfca44f5155092` | MIT OR Apache-2.0 | 2026-10-10 |
| `crates/pc-vector/src/edit.rs` | [VectorCraft](https://github.com/storytold/vectorcraft) | `crates/pathops/src/edit.rs`, `crates/pathops/src/fit.rs` (remove_anchor, smooth conversion; fitting adapted to existing Point) | `d522c1d7be4035bd4f4a84cd6ebfca44f5155092` | MIT OR Apache-2.0 | 2026-10-10 |
| `crates/pc-engine/src/vector_packages/node.rs` | [VectorCraft](https://github.com/storytold/vectorcraft) | `crates/tools/src/direct.rs`, `crates/pathops/src/edit.rs` (command replay semantics) | `d522c1d7be4035bd4f4a84cd6ebfca44f5155092` | MIT OR Apache-2.0 | 2026-10-10 |
| `crates/pc-ui-egui/src/direct_select_tests.rs` | [VectorCraft](https://github.com/storytold/vectorcraft) | `crates/tools/src/direct.rs` (gesture replay tests; original independent oracles) | `d522c1d7be4035bd4f4a84cd6ebfca44f5155092` | MIT OR Apache-2.0 | 2026-10-10 |
| `crates/pc-engine/src/vector_packages/node/tests.rs` | [VectorCraft](https://github.com/storytold/vectorcraft) | `crates/tools/src/direct.rs`, `crates/pathops/src/edit.rs` (command replay semantics; original native-state checks) | `d522c1d7be4035bd4f4a84cd6ebfca44f5155092` | MIT OR Apache-2.0 | 2026-10-10 |
| `crates/pc-engine/src/vector_packages/pen.rs` | [VectorCraft](https://github.com/storytold/vectorcraft) | `crates/tools/src/pen.rs` (transaction/gesture boundary adaptation) | `d522c1d7be4035bd4f4a84cd6ebfca44f5155092` | MIT OR Apache-2.0 | 2026-10-10 |
| `crates/pc-engine/src/vector_packages/pen/gesture.rs` | [VectorCraft](https://github.com/storytold/vectorcraft) | `crates/tools/src/pen.rs` (placement, Alt/Shift, closure and endpoint behavior) | `d522c1d7be4035bd4f4a84cd6ebfca44f5155092` | MIT OR Apache-2.0 | 2026-10-10 |
| `crates/pc-engine/src/vector_packages/pen/tests.rs` | [VectorCraft](https://github.com/storytold/vectorcraft) | `crates/tools/src/pen.rs` (selected replay scenarios, adapted independent assertions) | `d522c1d7be4035bd4f4a84cd6ebfca44f5155092` | MIT OR Apache-2.0 | 2026-10-10 |
| `crates/pc-ui-egui/src/vector_packages/pen.rs` | [VectorCraft](https://github.com/storytold/vectorcraft) | `crates/tools/src/pen.rs` (input adapter/overlay behavior) | `d522c1d7be4035bd4f4a84cd6ebfca44f5155092` | MIT OR Apache-2.0 | 2026-10-10 |
| `crates/pc-vector/src/pencil_fit.rs` | [VectorCraft](https://github.com/storytold/vectorcraft) | `crates/pathops/src/fit.rs` | `d522c1d7be4035bd4f4a84cd6ebfca44f5155092` | MIT OR Apache-2.0 | 2026-10-10 |
| `crates/pc-vector/src/pencil_fit.rs` | [VectorCraft](https://github.com/storytold/vectorcraft) | `crates/engine/src/cmd/draw2.rs` (continuation) | `d522c1d7be4035bd4f4a84cd6ebfca44f5155092` | MIT OR Apache-2.0 | 2026-10-10 |
| `crates/pc-engine/src/vector_packages/pencil.rs` | [VectorCraft](https://github.com/storytold/vectorcraft) | `crates/engine/src/cmd/draw2.rs` | `d522c1d7be4035bd4f4a84cd6ebfca44f5155092` | MIT OR Apache-2.0 | 2026-10-10 |
| `crates/pc-ui-egui/src/vector_packages/pencil.rs` | [VectorCraft](https://github.com/storytold/vectorcraft) | `crates/tools/src/draw2/gesture.rs` | `d522c1d7be4035bd4f4a84cd6ebfca44f5155092` | MIT OR Apache-2.0 | 2026-10-10 |
| `crates/pc-vector/src/live_corners.rs` | [VectorCraft](https://github.com/storytold/vectorcraft) | `crates/geom/src/corners.rs` | `d522c1d7be4035bd4f4a84cd6ebfca44f5155092` | MIT OR Apache-2.0 | 2026-10-10 |
| `crates/pc-ui-egui/src/vector_packages/corners.rs` | [VectorCraft](https://github.com/storytold/vectorcraft) | `crates/tools/src/corners.rs` | `d522c1d7be4035bd4f4a84cd6ebfca44f5155092` | MIT OR Apache-2.0 | 2026-10-10 |
| `crates/pc-engine/src/vector_snap.rs` | [VectorCraft](https://github.com/storytold/vectorcraft) | `crates/geom/src/snap.rs` (grid formula; remaining cache/query is original integration) | `d522c1d7be4035bd4f4a84cd6ebfca44f5155092` | MIT OR Apache-2.0 | 2026-10-10 |
| `crates/pc-vector/src/appearance.rs` | [VectorCraft](https://github.com/storytold/vectorcraft) | `crates/doc/src/appearance.rs` | `d522c1d7be4035bd4f4a84cd6ebfca44f5155092` | MIT OR Apache-2.0 | 2026-10-10 |
| `crates/pc-vector/src/appearance/geometry.rs` | [VectorCraft](https://github.com/storytold/vectorcraft) | `crates/effects/src/stroke/arrow.rs`, `crates/effects/src/stroke/dash.rs`, `crates/effects/src/stroke/gradient.rs` | `d522c1d7be4035bd4f4a84cd6ebfca44f5155092` | MIT OR Apache-2.0 | 2026-10-10 |
| `crates/pc-vector/src/appearance/paint.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `crates/pc-compose/src/pattern.rs` (Local Image fork) | `c18d18cf613422fe08d31001e1f803cf8ef7dd68` | MIT OR Apache-2.0 | 2026-10-10 |
| `crates/pc-text/src/view.rs` | [Local Image / PhotoCraft import](https://github.com/zdbosoxfan/local-image) | `crates/pc-text/src/render.rs` (glyph drawing, warp, faux style and color conversion) | `c18d18cf613422fe08d31001e1f803cf8ef7dd68` | MIT OR Apache-2.0; imported PhotoCraft source, project GPL-3.0-or-later | 2026-10-10 |
| `crates/pc-engine/src/vector_packages/constructive.rs` | [VectorCraft](https://github.com/storytold/vectorcraft) | `crates/tools/src/builder.rs` | `d522c1d7be4035bd4f4a84cd6ebfca44f5155092` | MIT OR Apache-2.0 | 2026-10-10 |
| `crates/pc-ui-egui/src/vector_packages/constructive.rs` | [VectorCraft](https://github.com/storytold/vectorcraft) | `crates/tools/src/builder.rs` | `d522c1d7be4035bd4f4a84cd6ebfca44f5155092` | MIT OR Apache-2.0 | 2026-10-10 |
| `crates/pc-ui-egui/src/vector_packages/constructive.rs` | [VectorCraft](https://github.com/storytold/vectorcraft) | `crates/tools/src/cut.rs` | `d522c1d7be4035bd4f4a84cd6ebfca44f5155092` | MIT OR Apache-2.0 | 2026-10-10 |
| `crates/pc-ui-egui/src/vector_packages/constructive.rs` | [VectorCraft](https://github.com/storytold/vectorcraft) | `crates/tools/src/draw2.rs` | `d522c1d7be4035bd4f4a84cd6ebfca44f5155092` | MIT OR Apache-2.0 | 2026-10-10 |
| `crates/pc-ui-egui/src/vector_packages/constructive.rs` | [VectorCraft](https://github.com/storytold/vectorcraft) | `crates/tools/src/draw2/gesture.rs` | `d522c1d7be4035bd4f4a84cd6ebfca44f5155092` | MIT OR Apache-2.0 | 2026-10-10 |
| `crates/pc-ui-egui/src/vector_packages/constructive.rs` | [VectorCraft](https://github.com/storytold/vectorcraft) | `crates/tools/src/draw2/anchor.rs` | `d522c1d7be4035bd4f4a84cd6ebfca44f5155092` | MIT OR Apache-2.0 | 2026-10-10 |
| `crates/pc-vecfx/src/width.rs` | [VectorCraft](https://github.com/storytold/vectorcraft) | `crates/effects/src/stroke/width.rs` | `d522c1d7be4035bd4f4a84cd6ebfca44f5155092` | MIT OR Apache-2.0 | 2026-10-10 |
| `crates/pc-vecfx/src/width.rs` | [VectorCraft](https://github.com/storytold/vectorcraft) | `crates/doc/src/appearance.rs` | `d522c1d7be4035bd4f4a84cd6ebfca44f5155092` | MIT OR Apache-2.0 | 2026-10-10 |
| `crates/pc-ui-egui/src/vector_packages/width.rs` | [VectorCraft](https://github.com/storytold/vectorcraft) | `crates/tools/src/distort/width.rs` | `d522c1d7be4035bd4f4a84cd6ebfca44f5155092` | MIT OR Apache-2.0 | 2026-10-10 |
| `crates/pc-ui-egui/src/vector_packages/width.rs` | [VectorCraft](https://github.com/storytold/vectorcraft) | `crates/doc/src/appearance.rs` | `d522c1d7be4035bd4f4a84cd6ebfca44f5155092` | MIT OR Apache-2.0 | 2026-10-10 |
| `crates/pc-text/src/on_path.rs` | [VectorCraft](https://github.com/storytold/vectorcraft) | `crates/text/src/layout.rs` (on-path subset) | `d522c1d7be4035bd4f4a84cd6ebfca44f5155092` | MIT OR Apache-2.0 | 2026-10-10 |
| `crates/pc-svg/src/import.rs` | [VectorCraft](https://github.com/storytold/vectorcraft) | `crates/svg/src/import.rs` | `d522c1d7be4035bd4f4a84cd6ebfca44f5155092` | MIT OR Apache-2.0 | 2026-10-10 |
| `crates/pc-svg/src/import/geometry.rs` | VectorCraft | `crates/svg/src/import.rs` | `d522c1d7be4035bd4f4a84cd6ebfca44f5155092` | MIT OR Apache-2.0 | 2026-10-10 |
| `crates/pc-svg/src/import/paint.rs` | VectorCraft | `crates/svg/src/import.rs` | `d522c1d7be4035bd4f4a84cd6ebfca44f5155092` | MIT OR Apache-2.0 | 2026-10-10 |
| `crates/pc-svg/src/import/tree.rs` | VectorCraft | `crates/svg/src/import.rs` | `d522c1d7be4035bd4f4a84cd6ebfca44f5155092` | MIT OR Apache-2.0 | 2026-10-10 |
| `crates/pc-svg/src/import/text.rs` | VectorCraft | `crates/svg/src/import/text.rs` | `d522c1d7be4035bd4f4a84cd6ebfca44f5155092` | MIT OR Apache-2.0 | 2026-10-10 |
| `crates/pc-svg/src/import/css.rs` | VectorCraft | `crates/svg/src/import/css.rs` | `d522c1d7be4035bd4f4a84cd6ebfca44f5155092` | MIT OR Apache-2.0 | 2026-10-10 |
| `crates/pc-svg/src/export.rs` | [VectorCraft](https://github.com/storytold/vectorcraft) | `crates/svg/src/export.rs` (path/stack/resource writer adaptation) | `d522c1d7be4035bd4f4a84cd6ebfca44f5155092` | MIT OR Apache-2.0 | 2026-10-10 |
| `crates/pc-svg/src/export/paint.rs` | [VectorCraft](https://github.com/storytold/vectorcraft) | `crates/svg/src/export.rs` (gradient defs/paint adaptation) | `d522c1d7be4035bd4f4a84cd6ebfca44f5155092` | MIT OR Apache-2.0 | 2026-10-10 |
| `crates/pc-svg/src/export/text.rs` | [VectorCraft](https://github.com/storytold/vectorcraft) | `crates/svg/src/export.rs` (text/outlines writer organization) | `d522c1d7be4035bd4f4a84cd6ebfca44f5155092` | MIT OR Apache-2.0 | 2026-10-10 |
| `crates/pc-vecfx/src/blend.rs` | [VectorCraft](https://github.com/storytold/vectorcraft) | `crates/doc/src/blend.rs`, `crates/doc/src/live.rs` | `d522c1d7be4035bd4f4a84cd6ebfca44f5155092` | MIT OR Apache-2.0 | 2026-10-10 |
| `crates/pc-vecfx/src/blend/correspondence.rs` | [VectorCraft](https://github.com/storytold/vectorcraft) | `crates/doc/src/blend.rs` | `d522c1d7be4035bd4f4a84cd6ebfca44f5155092` | MIT OR Apache-2.0 | 2026-10-10 |
| `crates/pc-vecfx/src/blend/paint.rs` | [VectorCraft](https://github.com/storytold/vectorcraft) | `crates/doc/src/blend.rs` | `d522c1d7be4035bd4f4a84cd6ebfca44f5155092` | MIT OR Apache-2.0 | 2026-10-10 |
| `crates/pc-vecfx/src/blend/paint.rs` | [Oklab](https://bottosson.github.io/posts/oklab/) | linear sRGB/Oklab conversion formulas | 2021-01-25 matrices; page SHA above | MIT | 2026-10-10 |
| `crates/pc-vecfx/src/blend/spine.rs` | [VectorCraft](https://github.com/storytold/vectorcraft) | `crates/doc/src/blend.rs` | `d522c1d7be4035bd4f4a84cd6ebfca44f5155092` | MIT OR Apache-2.0 | 2026-10-10 |
| `crates/pc-ui-egui/src/vector_packages/blend.rs` | [VectorCraft](https://github.com/storytold/vectorcraft) | `crates/engine/src/tests_blendopts.rs`, `tests_blendspine.rs` (interaction adaptation) | `d522c1d7be4035bd4f4a84cd6ebfca44f5155092` | MIT OR Apache-2.0 | 2026-10-10 |
| `crates/pc-vecfx/src/repeat.rs` | [VectorCraft](https://github.com/storytold/vectorcraft) | `crates/doc/src/pattern.rs` | `5afc78a1002cc217f743f74cea9dcdf640318b0a` | MIT OR Apache-2.0 | 2026-10-10 |
| `crates/pc-vecfx/src/repeat/projection.rs` | [VectorCraft](https://github.com/storytold/vectorcraft) | `crates/doc/src/pattern.rs` | `5afc78a1002cc217f743f74cea9dcdf640318b0a` | MIT OR Apache-2.0 | 2026-10-10 |
| `crates/pc-vecfx/src/envelope.rs` | [VectorCraft](https://github.com/storytold/vectorcraft) | `crates/doc/src/live.rs`, `crates/effects/src/warp.rs` | `d522c1d7be4035bd4f4a84cd6ebfca44f5155092` | MIT OR Apache-2.0 | 2026-10-10 |
| `crates/pc-vecfx/src/envelope/warp.rs` | [VectorCraft](https://github.com/storytold/vectorcraft) | `crates/doc/src/live.rs`, `crates/effects/src/warp.rs` | `d522c1d7be4035bd4f4a84cd6ebfca44f5155092` | MIT OR Apache-2.0 | 2026-10-10 |
| `crates/pc-vecfx/src/envelope/mesh.rs` | [VectorCraft](https://github.com/storytold/vectorcraft) | `crates/doc/src/live.rs` | `d522c1d7be4035bd4f4a84cd6ebfca44f5155092` | MIT OR Apache-2.0 | 2026-10-10 |
| `crates/pc-vecfx/src/envelope/coons.rs` | [VectorCraft](https://github.com/storytold/vectorcraft) | `crates/doc/src/live.rs` | `d522c1d7be4035bd4f4a84cd6ebfca44f5155092` | MIT OR Apache-2.0 | 2026-10-10 |
| `crates/pc-pdf/src/export.rs` | [VectorCraft](https://github.com/storytold/vectorcraft) | `crates/pdf/src/export.rs` (krilla tree/graphics-state organization) | `d522c1d7be4035bd4f4a84cd6ebfca44f5155092` | MIT OR Apache-2.0 | 2026-10-10 |
| `crates/pc-pdf/src/export/paint.rs` | [VectorCraft](https://github.com/storytold/vectorcraft) | `crates/pdf/src/export.rs` (krilla shading organization) | `d522c1d7be4035bd4f4a84cd6ebfca44f5155092` | MIT OR Apache-2.0 | 2026-10-10 |
| `crates/pc-pdf/src/export/text.rs` | [VectorCraft](https://github.com/storytold/vectorcraft) | `crates/pdf/src/export.rs` (positioned-font/outline writer organization) | `d522c1d7be4035bd4f4a84cd6ebfca44f5155092` | MIT OR Apache-2.0 | 2026-10-10 |
| `crates/pc-pdf/src/import.rs` | [VectorCraft](https://github.com/storytold/vectorcraft) | `crates/pdf/src/import.rs` | `d522c1d7be4035bd4f4a84cd6ebfca44f5155092` | MIT OR Apache-2.0 | 2026-10-10 |
| `crates/pc-pdf/src/import.rs` | [VectorCraft](https://github.com/storytold/vectorcraft) | `crates/pdf/src/pages.rs` | `d522c1d7be4035bd4f4a84cd6ebfca44f5155092` | MIT OR Apache-2.0 | 2026-10-10 |
| `crates/pc-pdf/src/import/device.rs` | [VectorCraft](https://github.com/storytold/vectorcraft) | `crates/pdf/src/import.rs` | `d522c1d7be4035bd4f4a84cd6ebfca44f5155092` | MIT OR Apache-2.0 | 2026-10-10 |
| `crates/pc-pdf/src/import/device.rs` | [VectorCraft](https://github.com/storytold/vectorcraft) | `crates/pdf/src/import_mask.rs` | `d522c1d7be4035bd4f4a84cd6ebfca44f5155092` | MIT OR Apache-2.0 | 2026-10-10 |
| `crates/pc-pdf/src/import/pages.rs` | [VectorCraft](https://github.com/storytold/vectorcraft) | `crates/pdf/src/pages.rs` | `d522c1d7be4035bd4f4a84cd6ebfca44f5155092` | MIT OR Apache-2.0 | 2026-10-10 |
| `crates/pc-pdf/src/import/paint.rs` | [VectorCraft](https://github.com/storytold/vectorcraft) | `crates/pdf/src/import.rs` | `d522c1d7be4035bd4f4a84cd6ebfca44f5155092` | MIT OR Apache-2.0 | 2026-10-10 |
| `crates/pc-pdf/src/import/paint.rs` | [VectorCraft](https://github.com/storytold/vectorcraft) | `crates/pdf/src/import_shading.rs` | `d522c1d7be4035bd4f4a84cd6ebfca44f5155092` | MIT OR Apache-2.0 | 2026-10-10 |
| `crates/pc-pdf/src/import/geometry.rs` | [VectorCraft](https://github.com/storytold/vectorcraft) | `crates/geom/src/path.rs` | `d522c1d7be4035bd4f4a84cd6ebfca44f5155092` | MIT OR Apache-2.0 | 2026-10-10 |
| `crates/pc-affinity/src/access.rs` | [VectorCraft](https://github.com/storytold/vectorcraft) | `crates/affinity/src/access.rs` | `d522c1d7be4035bd4f4a84cd6ebfca44f5155092` | MIT OR Apache-2.0 | 2026-10-10 |
| `crates/pc-affinity/src/container.rs` | [VectorCraft](https://github.com/storytold/vectorcraft) | `crates/affinity/src/container.rs` | `b3ccdf01a7482fc4fd7545ed9e302911824a2a3c` (including `fd50bd6f843e80e80b4db1ad7c25c863bff90a33` AF.5 records and `f73a3df974b990fd4d73d5addf6a95c48ea3cc2b` resource limits) | MIT OR Apache-2.0 | 2026-10-26 |
| `crates/pc-affinity/src/geometry.rs` | [VectorCraft](https://github.com/storytold/vectorcraft) | `crates/affinity/src/geometry.rs` | `d522c1d7be4035bd4f4a84cd6ebfca44f5155092` | MIT OR Apache-2.0 | 2026-10-10 |
| `crates/pc-affinity/src/lib.rs` | [VectorCraft](https://github.com/storytold/vectorcraft) | `crates/affinity/src/lib.rs` | `d522c1d7be4035bd4f4a84cd6ebfca44f5155092` | MIT OR Apache-2.0 | 2026-10-10 |
| `crates/pc-affinity/src/import.rs` | [VectorCraft](https://github.com/storytold/vectorcraft) | `crates/engine/src/cmd/fileio/affinity.rs` (local native scene adaptation) | `b3ccdf01a7482fc4fd7545ed9e302911824a2a3c` (including `fd50bd6f843e80e80b4db1ad7c25c863bff90a33` AF.5 records and `f73a3df974b990fd4d73d5addf6a95c48ea3cc2b` resource limits) | MIT OR Apache-2.0 | 2026-10-26 |
| `crates/pc-affinity/src/import/scene.rs` | [VectorCraft](https://github.com/storytold/vectorcraft) | `crates/engine/src/cmd/fileio/affinity.rs` (local native scene adaptation) | `b3ccdf01a7482fc4fd7545ed9e302911824a2a3c` (including `fd50bd6f843e80e80b4db1ad7c25c863bff90a33` AF.5 records and `f73a3df974b990fd4d73d5addf6a95c48ea3cc2b` resource limits) | MIT OR Apache-2.0 | 2026-10-26 |
| `crates/pc-affinity/src/import/text.rs` | [VectorCraft](https://github.com/storytold/vectorcraft) | `crates/affinity/src/text.rs` (local native scene adaptation) | `b3ccdf01a7482fc4fd7545ed9e302911824a2a3c` (including `fd50bd6f843e80e80b4db1ad7c25c863bff90a33` AF.5 records and `f73a3df974b990fd4d73d5addf6a95c48ea3cc2b` resource limits) | MIT OR Apache-2.0 | 2026-10-26 |
| `crates/pc-affinity/src/import/effects.rs` | [VectorCraft](https://github.com/storytold/vectorcraft) | `crates/affinity/src/overlay.rs` (local native scene adaptation) | `b3ccdf01a7482fc4fd7545ed9e302911824a2a3c` (including `fd50bd6f843e80e80b4db1ad7c25c863bff90a33` AF.5 records and `f73a3df974b990fd4d73d5addf6a95c48ea3cc2b` resource limits) | MIT OR Apache-2.0 | 2026-10-26 |
| `crates/pc-affinity/src/model.rs` | [VectorCraft](https://github.com/storytold/vectorcraft) | `crates/affinity/src/model.rs` | `b3ccdf01a7482fc4fd7545ed9e302911824a2a3c` (including `f73a3df974b990fd4d73d5addf6a95c48ea3cc2b` mask extent) | MIT OR Apache-2.0 | 2026-10-26 |
| `crates/pc-affinity/src/paint.rs` | [VectorCraft](https://github.com/storytold/vectorcraft) | `crates/affinity/src/paint.rs` | `b3ccdf01a7482fc4fd7545ed9e302911824a2a3c` (including `fd50bd6f843e80e80b4db1ad7c25c863bff90a33` AF.5 records and `f73a3df974b990fd4d73d5addf6a95c48ea3cc2b` resource limits) | MIT OR Apache-2.0 | 2026-10-26 |
| `crates/pc-affinity/src/preview.rs` | [VectorCraft](https://github.com/storytold/vectorcraft) | `crates/affinity/src/preview.rs` | `d522c1d7be4035bd4f4a84cd6ebfca44f5155092` | MIT OR Apache-2.0 | 2026-10-10 |
| `crates/pc-affinity/src/raster.rs` | [VectorCraft](https://github.com/storytold/vectorcraft) | `crates/affinity/src/raster.rs` | `b3ccdf01a7482fc4fd7545ed9e302911824a2a3c` (including `f73a3df974b990fd4d73d5addf6a95c48ea3cc2b` cropped masks) | MIT OR Apache-2.0 | 2026-10-26 |
| `crates/pc-affinity/src/shapes.rs` | [VectorCraft](https://github.com/storytold/vectorcraft) | `crates/affinity/src/shapes.rs` | `d522c1d7be4035bd4f4a84cd6ebfca44f5155092` | MIT OR Apache-2.0 | 2026-10-10 |
| `crates/pc-affinity/src/stream.rs` | [VectorCraft](https://github.com/storytold/vectorcraft) | `crates/affinity/src/stream.rs` | `d522c1d7be4035bd4f4a84cd6ebfca44f5155092` | MIT OR Apache-2.0 | 2026-10-10 |
| `crates/pc-affinity/src/synth.rs` | [VectorCraft](https://github.com/storytold/vectorcraft) | `crates/affinity/src/synth.rs` | `d522c1d7be4035bd4f4a84cd6ebfca44f5155092` | MIT OR Apache-2.0 | 2026-10-10 |
| `crates/pc-affinity/src/tests.rs` | [VectorCraft](https://github.com/storytold/vectorcraft) | `crates/affinity/src/tests.rs` | `d522c1d7be4035bd4f4a84cd6ebfca44f5155092` | MIT OR Apache-2.0 | 2026-10-10 |
| `crates/pc-affinity/src/text.rs` | [VectorCraft](https://github.com/storytold/vectorcraft) | `crates/affinity/src/text.rs` | `b3ccdf01a7482fc4fd7545ed9e302911824a2a3c` (including `fd50bd6f843e80e80b4db1ad7c25c863bff90a33` AF.5 records and `f73a3df974b990fd4d73d5addf6a95c48ea3cc2b` resource limits) | MIT OR Apache-2.0 | 2026-10-26 |
| `crates/pc-affinity/tests/text.rs` | VectorCraft | `crates/affinity/tests/text.rs` | `d522c1d7be4035bd4f4a84cd6ebfca44f5155092` | MIT OR Apache-2.0 | 2026-10-10 |
| `crates/pc-affinity/tests/support/text_fixture.rs` | VectorCraft | `crates/affinity/tests/support/text_fixture.rs` | `d522c1d7be4035bd4f4a84cd6ebfca44f5155092` | MIT OR Apache-2.0 | 2026-10-10 |
| crates/pc-trace/src/centerline.rs | [VectorCraft](https://github.com/storytold/vectorcraft) | crates/trace/src/centerline.rs | d522c1d7be4035bd4f4a84cd6ebfca44f5155092 | MIT OR Apache-2.0 | 2026-10-10 |
| crates/pc-trace/src/centerline/skeleton.rs | [VectorCraft](https://github.com/storytold/vectorcraft) | crates/trace/src/centerline.rs | d522c1d7be4035bd4f4a84cd6ebfca44f5155092 | MIT OR Apache-2.0 | 2026-10-10 |
| crates/pc-trace/src/centerline/graph.rs | [VectorCraft](https://github.com/storytold/vectorcraft) | crates/trace/src/centerline.rs | d522c1d7be4035bd4f4a84cd6ebfca44f5155092 | MIT OR Apache-2.0 | 2026-10-10 |
| crates/pc-trace/src/centerline/mask.rs | [vtracer](https://github.com/visioncortex/vtracer) | crates/vtracer/src/frontend/binary.rs | 928ed0a6f654408e28fb741b6133d4c456bd0160 | MIT OR Apache-2.0 | 2026-10-10 |
| crates/pc-trace/tests/centerline/algorithm.rs | [VectorCraft](https://github.com/storytold/vectorcraft) | crates/trace/src/tests.rs | d522c1d7be4035bd4f4a84cd6ebfca44f5155092 | MIT OR Apache-2.0 | 2026-10-10 |
| `crates/pc-trace/src/cleanup/recognize.rs` | [VectorCraft](https://github.com/storytold/vectorcraft) | `crates/geom/src/recognize.rs` (resample, covariance/oriented bounds, simplify/closed-ring splitting; adapted with strict residual checks) | `d522c1d7be4035bd4f4a84cd6ebfca44f5155092` | MIT OR Apache-2.0 | 2026-10-10 |
| `crates/pc-trace/src/cutout.rs` | [vtracer](https://github.com/visioncortex/vtracer) | `crates/vtracer/src/mosaic.rs` | `ef9496f7926a85ae25761cd7d00e6f6d6364e562` | MIT OR Apache-2.0 | 2026-10-10 |
| `crates/pc-trace/src/cutout/graph.rs` | [vtracer](https://github.com/visioncortex/vtracer) | `crates/vtracer/src/mosaic/graph.rs` | `ba2b80455e6bbc48b6ddb2004e562e9cb4c141d6` | MIT OR Apache-2.0 | 2026-10-10 |
| `crates/pc-trace/src/cutout/face.rs` | [vtracer](https://github.com/visioncortex/vtracer) | `crates/vtracer/src/mosaic/face.rs` | `ba2b80455e6bbc48b6ddb2004e562e9cb4c141d6` | MIT OR Apache-2.0 | 2026-10-10 |
| `crates/pc-trace/src/cutout/fit.rs` | [vtracer](https://github.com/visioncortex/vtracer) | `crates/vtracer/src/mosaic/fit.rs` | `ef9496f7926a85ae25761cd7d00e6f6d6364e562` | MIT OR Apache-2.0 | 2026-10-10 |
| `crates/pc-trace/src/cutout/compose.rs` | [vtracer](https://github.com/visioncortex/vtracer) | `crates/vtracer/src/mosaic/compose.rs` | `ef9496f7926a85ae25761cd7d00e6f6d6364e562` | MIT OR Apache-2.0 | 2026-10-10 |
| `crates/pc-trace/src/cutout/merge.rs` | [vtracer](https://github.com/visioncortex/vtracer) | `crates/vtracer/src/mosaic.rs` | `ef9496f7926a85ae25761cd7d00e6f6d6364e562` | MIT OR Apache-2.0 | 2026-10-10 |
| `crates/pc-text/src/frame_compose/breaker.rs` | [DesignCraft](https://github.com/storytold/designcraft) | `crates/compose/src/breaker.rs` | `66c7ce7e96ec286a091878cc2541ee8369aa79d6` | MIT OR Apache-2.0 | 2026-10-10 |
| `crates/pc-engine/src/vector_packages/selection.rs` | [VectorCraft](https://github.com/storytold/vectorcraft) | `crates/engine/src/cmd/select.rs`, `crates/engine/src/cmd/wand.rs`, `crates/tools/src/xform/wand.rs` (attribute queries, conjunction, polygon enclosure; Oklab is the task's change) | `d522c1d7be4035bd4f4a84cd6ebfca44f5155092` | MIT OR Apache-2.0 | 2026-10-10 |
| `crates/pc-ui-egui/src/vector_packages/selection.rs` | [VectorCraft](https://github.com/storytold/vectorcraft) | `crates/tools/src/select.rs`, `crates/tools/src/xform/wand.rs` (gesture/modifier behavior) | `d522c1d7be4035bd4f4a84cd6ebfca44f5155092` | MIT OR Apache-2.0 | 2026-10-10 |
| `crates/pc-vector/src/curvature_fit.rs` | [VectorCraft](https://github.com/storytold/vectorcraft) | `crates/tools/src/draw2.rs::catmull_rom` | `d522c1d7be4035bd4f4a84cd6ebfca44f5155092` | MIT OR Apache-2.0 | 2026-10-10 |
| `crates/pc-engine/src/vector_packages/anchor_tools.rs` | [VectorCraft](https://github.com/storytold/vectorcraft) | `crates/tools/src/draw2/{anchor,curvature}.rs` (interaction contracts; native transactions are local) | `d522c1d7be4035bd4f4a84cd6ebfca44f5155092` | MIT OR Apache-2.0 | 2026-10-10 |
| `crates/pc-ui-egui/src/vector_packages/anchor_tools.rs` | [VectorCraft](https://github.com/storytold/vectorcraft) | `crates/tools/src/draw2/{anchor,curvature}.rs` | `d522c1d7be4035bd4f4a84cd6ebfca44f5155092` | MIT OR Apache-2.0 | 2026-10-10 |
| `crates/pc-engine/src/vector_packages/clip_compound.rs` | [VectorCraft](https://github.com/storytold/vectorcraft) | `crates/engine/src/cmd/object.rs` | `7e82dbc05b972d8b32b68e7b31444a87ed8e6d48` | MIT OR Apache-2.0 | 2026-10-10 |
| `crates/pc-engine/src/vector_packages/styles.rs` | [VectorCraft](https://github.com/storytold/vectorcraft) | `crates/engine/src/cmd/{style,stylelib}.rs` (behaviour; native transaction/provider code is local) | `d522c1d7be4035bd4f4a84cd6ebfca44f5155092` | MIT OR Apache-2.0 | 2026-10-10 |
| `crates/pc-ui-egui/src/vector_packages/eyedropper.rs` | [VectorCraft](https://github.com/storytold/vectorcraft) | `crates/tools/src/xform/eyedropper.rs` (modifiers; egui capture/app bridge is local) | `d522c1d7be4035bd4f4a84cd6ebfca44f5155092` | MIT OR Apache-2.0 | 2026-10-10 |
| `crates/pc-color/src/ase.rs` | [VectorCraft](https://github.com/storytold/vectorcraft) | `crates/color/src/palette_io.rs` | `d522c1d7be4035bd4f4a84cd6ebfca44f5155092` | MIT OR Apache-2.0 | 2026-10-10 |
| `crates/pc-doc/src/vector_design/swatches.rs` | [VectorCraft](https://github.com/storytold/vectorcraft) | `crates/color/src/swatch.rs` | `d522c1d7be4035bd4f4a84cd6ebfca44f5155092` | MIT OR Apache-2.0 | 2026-10-10 |
| `crates/pc-doc/src/vector_design/swatches.rs` | [VectorCraft](https://github.com/storytold/vectorcraft) | `crates/doc/src/swatches.rs` | `d522c1d7be4035bd4f4a84cd6ebfca44f5155092` | MIT OR Apache-2.0 | 2026-10-10 |
| `crates/pc-engine/src/vector_packages/colours.rs` | [VectorCraft](https://github.com/storytold/vectorcraft) | `crates/engine/src/cmd/swatch.rs` | `d522c1d7be4035bd4f4a84cd6ebfca44f5155092` | MIT OR Apache-2.0 | 2026-10-10 |
| `crates/pc-engine/src/vector_packages/recolor.rs` | [VectorCraft](https://github.com/storytold/vectorcraft) | `crates/color/src/recolor.rs` | `d522c1d7be4035bd4f4a84cd6ebfca44f5155092` | MIT OR Apache-2.0 | 2026-10-10 |
| `crates/pc-engine/src/vector_packages/recolor.rs` | [VectorCraft](https://github.com/storytold/vectorcraft) | `crates/engine/src/cmd/recolor.rs` | `d522c1d7be4035bd4f4a84cd6ebfca44f5155092` | MIT OR Apache-2.0 | 2026-10-10 |
| `crates/pc-engine/src/vector_packages/blob_eraser.rs` | [VectorCraft](https://github.com/storytold/vectorcraft) | `crates/engine/src/cmd/draw2.rs` | `d522c1d7be4035bd4f4a84cd6ebfca44f5155092` | MIT OR Apache-2.0 | 2026-10-10 |
| `crates/pc-ui-egui/src/vector_packages/blob_eraser.rs` | [VectorCraft](https://github.com/storytold/vectorcraft) | `crates/tools/src/draw2/gesture.rs` | `d522c1d7be4035bd4f4a84cd6ebfca44f5155092` | MIT OR Apache-2.0 | 2026-10-10 |
| crates/pc-vecfx/src/liquify/kernel.rs | [VectorCraft](https://github.com/storytold/vectorcraft) | crates/tools/src/distort/liquify.rs | d522c1d7be4035bd4f4a84cd6ebfca44f5155092 | MIT OR Apache-2.0 | 2026-10-10 |
| crates/pc-vecfx/src/liquify/mod.rs | [VectorCraft](https://github.com/storytold/vectorcraft) | crates/tools/src/distort/liquify.rs | d522c1d7be4035bd4f4a84cd6ebfca44f5155092 | MIT OR Apache-2.0 | 2026-10-10 |
| crates/pc-vecfx/src/puppet/arap.rs | [VectorCraft](https://github.com/storytold/vectorcraft) | crates/tools/src/distort/arap.rs | d522c1d7be4035bd4f4a84cd6ebfca44f5155092 | MIT OR Apache-2.0 | 2026-10-10 |
| crates/pc-vecfx/src/puppet/mod.rs | [VectorCraft](https://github.com/storytold/vectorcraft) | crates/tools/src/distort/puppet.rs | d522c1d7be4035bd4f4a84cd6ebfca44f5155092 | MIT OR Apache-2.0 | 2026-10-10 |
| crates/pc-engine/src/vector_packages/distort_tools.rs | [VectorCraft](https://github.com/storytold/vectorcraft) | crates/engine/src/cmd/distortcmds.rs | d522c1d7be4035bd4f4a84cd6ebfca44f5155092 | MIT OR Apache-2.0 | 2026-10-10 |
| crates/pc-ui-egui/src/vector_packages/distort_tools.rs | [VectorCraft](https://github.com/storytold/vectorcraft) | crates/tools/src/distort/liquify.rs | d522c1d7be4035bd4f4a84cd6ebfca44f5155092 | MIT OR Apache-2.0 | 2026-10-10 |
| crates/pc-ui-egui/src/vector_packages/distort_tools.rs | [VectorCraft](https://github.com/storytold/vectorcraft) | crates/tools/src/distort/puppet.rs | d522c1d7be4035bd4f4a84cd6ebfca44f5155092 | MIT OR Apache-2.0 | 2026-10-10 |
| `crates/pc-vector/src/mesh_paint.rs` | [VectorCraft](https://github.com/storytold/vectorcraft) | `crates/doc/src/live.rs` (GradientMesh) | `d522c1d7be4035bd4f4a84cd6ebfca44f5155092` | MIT OR Apache-2.0 | 2026-10-10 |
| `crates/pc-engine/src/vector_packages/mesh.rs` | [VectorCraft](https://github.com/storytold/vectorcraft) | `crates/engine/src/cmd/live.rs` (object.mesh.*) | `d522c1d7be4035bd4f4a84cd6ebfca44f5155092` | MIT OR Apache-2.0 | 2026-10-10 |
| `crates/pc-ui-egui/src/vector_packages/mesh.rs` | [VectorCraft](https://github.com/storytold/vectorcraft) | `crates/tools/src/meshblend.rs`, `crates/tools/src/meshedit.rs` | `d522c1d7be4035bd4f4a84cd6ebfca44f5155092` | MIT OR Apache-2.0 | 2026-10-10 |
| `crates/pc-doc/src/vector_design/effects.rs` | [VectorCraft](https://github.com/storytold/vectorcraft) | `crates/effects/src/distort.rs`, `crates/effects/src/stylize.rs` (parameter model) | `d522c1d7be4035bd4f4a84cd6ebfca44f5155092` | MIT OR Apache-2.0 | 2026-10-10 |
| `crates/pc-vecfx/src/path_effects/mod.rs` | [VectorCraft](https://github.com/storytold/vectorcraft) | `crates/effects/src/stylize.rs` (Convert to Shape) | `d522c1d7be4035bd4f4a84cd6ebfca44f5155092` | MIT OR Apache-2.0 | 2026-10-10 |
| `crates/pc-vecfx/src/path_effects/distort.rs` | [VectorCraft](https://github.com/storytold/vectorcraft) | `crates/effects/src/distort.rs`, `crates/effects/src/util.rs` | `d522c1d7be4035bd4f4a84cd6ebfca44f5155092` | MIT OR Apache-2.0 | 2026-10-10 |
| `crates/pc-engine/src/vector_packages/path_effects.rs` | [VectorCraft](https://github.com/storytold/vectorcraft) | `crates/engine/src/cmd/effectcmd.rs` (transaction semantics) | `d522c1d7be4035bd4f4a84cd6ebfca44f5155092` | MIT OR Apache-2.0 | 2026-10-10 |
| crates/pc-engine/src/vector_toolbar_layout.rs | [VectorCraft](https://github.com/storytold/vectorcraft) | crates/tools/src/catalog.rs (order and Illustrator keymap adapted to Local Image IDs) | d522c1d7be4035bd4f4a84cd6ebfca44f5155092 | MIT OR Apache-2.0 | 2026-10-10 |

### Affinity AF.5 provenance (2026-10-26)

The file rows above reconcile the AF.5 reader and local native scene adapters. The following
security adaptations are recorded by local commit `d0014628`.

| Upstream commit | Subject | Our files | Licence |
|---|---|---|---|
| [storytold/photocraft@`7c07280b660ca937529a97f4e9176c79861e2667`](https://github.com/storytold/photocraft/commit/7c07280b660ca937529a97f4e9176c79861e2667) | security(affinity): bound raw child-slot traversal work | `crates/pc-affinity/src/access.rs`, `crates/pc-affinity/src/model.rs` | MIT OR Apache-2.0 |
| [storytold/photocraft@`b11ecc6ebf6699f53d0982c3f6c37a975ea9b17d`](https://github.com/storytold/photocraft/commit/b11ecc6ebf6699f53d0982c3f6c37a975ea9b17d) | security(affinity): charge shared-object expansion against document-wide budgets (#8938) | `crates/pc-affinity/src/model.rs`, `crates/pc-affinity/src/geometry.rs`, `crates/pc-affinity/src/raster.rs` | MIT OR Apache-2.0 |
| [storytold/photocraft@`29e555c2f655676c71c26ccea20e6371da9248b2`](https://github.com/storytold/photocraft/commit/29e555c2f655676c71c26ccea20e6371da9248b2) | security(affinity): index live-corner radii by record | `crates/pc-affinity/src/geometry.rs` | MIT OR Apache-2.0 |
| [storytold/vectorcraft@`4c82749b08575cec1e6a0fe2e2bca1ef157151e7`](https://github.com/storytold/vectorcraft/commit/4c82749b08575cec1e6a0fe2e2bca1ef157151e7) | Security: Affinity import and brush expansion bounds (#3887) | `crates/pc-affinity/src/geometry.rs`, `crates/pc-affinity/src/stream.rs` | MIT OR Apache-2.0 |
| [storytold/photocraft@`5adebc5d6df46a104547f1b9d872ee314ae1945c`](https://github.com/storytold/photocraft/commit/5adebc5d6df46a104547f1b9d872ee314ae1945c) | security(affinity): index object fields for O(1) lookup | `crates/pc-affinity/src/stream.rs`, `crates/pc-affinity/src/access.rs`, `crates/pc-affinity/src/shapes.rs` | MIT OR Apache-2.0 |
| [storytold/vectorcraft@`fd50bd6f843e80e80b4db1ad7c25c863bff90a33`](https://github.com/storytold/vectorcraft/commit/fd50bd6f843e80e80b4db1ad7c25c863bff90a33) | AF.5: Affinity import — strokes, text decoration, text placement, outline effects, CMYK photos | `crates/pc-affinity/src/paint.rs` | MIT OR Apache-2.0 |
| [storytold/vectorcraft@`2d7004f972fc3e97feda7775853026c63c922331`](https://github.com/storytold/vectorcraft/commit/2d7004f972fc3e97feda7775853026c63c922331) | AF.5: cap dash patterns at 64 lengths | `crates/pc-affinity/src/paint.rs` | MIT OR Apache-2.0 |

Local adaptations: `4c82749b` contributes corner-radius indexing and zero-width record guards
only; `fd50bd6f` contributes None-style stroke suppression here. `2d7004f9` imports longer dash
patterns as solid with a warning. The native scene adapters use Local Image layers and budgets;
font metadata retains the local 16 KiB limit rather than the upstream 256-byte limit.

### Bounded PDF CCITT admission (2026-10-12)

Pinned registry `.cargo_vcs_info.json` verifies commit below for hayro-ccitt 0.3.0.
The MIT license option is selected; notice: `licenses/hayro-ccitt-MIT.txt`.
No dependency version changes. Tables/decoding organization are retained, with
fallible output, strict rejection of malformed/overlong rows, per-bit and
reference-search work/cancel checks. No panic-based cancellation.

| Our file | Upstream project | Upstream path | Upstream commit | Licence | Date | Used for |
|---|---|---|---|---|---|---|
| `crates/pc-pdf/src/import/preflight/ccitt/mod.rs` | [hayro](https://github.com/LaurenzV/hayro) | `hayro-ccitt/src/lib.rs` | `4ad1532e1cd719170c4f4baf669f13ec0f406abb` | MIT | 2026-10-12 | Fallible, bounded CCITT Group 3/4 admission. |
| `crates/pc-pdf/src/import/preflight/ccitt/bit_reader.rs` | [hayro](https://github.com/LaurenzV/hayro) | `hayro-ccitt/src/bit_reader.rs` | `4ad1532e1cd719170c4f4baf669f13ec0f406abb` | MIT | 2026-10-12 | Shared work budget and cancellation, including peeks. |
| `crates/pc-pdf/src/import/preflight/ccitt/decode.rs` | [hayro](https://github.com/LaurenzV/hayro) | `hayro-ccitt/src/decode.rs` | `4ad1532e1cd719170c4f4baf669f13ec0f406abb` | MIT | 2026-10-12 | Strict Huffman/EOL decoding without partial repair. |
| `crates/pc-pdf/src/import/preflight/ccitt/state_machine.rs` | [hayro](https://github.com/LaurenzV/hayro) | `hayro-ccitt/src/state_machine.rs` | `4ad1532e1cd719170c4f4baf669f13ec0f406abb` | MIT | 2026-10-12 | Pinned T.4/T.6 code tables. |

## PhotoCraft and LightCraft upstream fixes (2026-10-13)

Fixes ported by hand from PhotoCraft (after `e5e3e397`) and LightCraft (after `b8df5f4`) in the
up-raw, up-formats, up-engine, up-library and up-linux batches and their fu-core follow-ups
(per-item notes were kept in wip reports that now live only in git history). Each row is
one upstream file of one fix commit and the file of ours that carries it; most rows are partial
ports of that commit (our code had diverged), not claims that the files match. LightCraft's
`crates/fetch` is Apache-2.0 only; the rest of both projects is MIT OR Apache-2.0 (notices in
`licenses/photocraft-*` and `licenses/lightcraft-*`).

| Our file | Upstream project | Upstream path | Upstream commit | Licence | Date |
|---|---|---|---|---|---|
| `crates/lc-raw/src/opcodes.rs` | [LightCraft](https://github.com/storytold/lightcraft) | `crates/raw/src/opcodes.rs` | `a4436597ad94546eb5d1ef538d1ae9355c5a437a` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/lc-raw/src/vendor/rw2.rs` | [LightCraft](https://github.com/storytold/lightcraft) | `crates/raw/src/vendor/rw2.rs` | `1ba0421a85702bf0142bdcbd33fd2734fa923df4` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/lc-raw/src/vendor/nefc.rs` | [LightCraft](https://github.com/storytold/lightcraft) | `crates/raw/src/vendor/nefc.rs` | `ff8d0277665304957bc2ef5d44fb07c60232bfd5` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/lc-raw/src/vendor/cr2.rs` | [LightCraft](https://github.com/storytold/lightcraft) | `crates/raw/src/vendor/cr2.rs` | `b5440f167ffc9d87e8c39860cf63979a2d6a46d5` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/lc-raw/src/vendor/pef.rs` | [LightCraft](https://github.com/storytold/lightcraft) | `crates/raw/src/vendor/pef.rs` | `1399c445ef8f4ba11e26e1fd9c9bf1272c8ea757` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/lc-raw/src/vendor/pef.rs` | [LightCraft](https://github.com/storytold/lightcraft) | `crates/raw/src/vendor/pef.rs` | `c7719ddcbf60882329af0eb048d3acea08f403af` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/lc-raw/src/vendor/mod.rs` | [LightCraft](https://github.com/storytold/lightcraft) | `crates/raw/src/vendor/mod.rs` | `b59ffcc4c9d920162720fded7cbb9e4bfc507920` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/lc-raw/src/vendor/cr2.rs` | [LightCraft](https://github.com/storytold/lightcraft) | `crates/raw/src/vendor/cr2.rs` | `34ecea4f2452d03d790aa6508c30db0924544fbd` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/lc-raw/src/lib.rs` | [LightCraft](https://github.com/storytold/lightcraft) | `crates/raw/src/lib.rs` | `b4c4a436cb41169122a5e1435b2d9df382b705f2` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/lc-raw/src/saturation.rs` | [LightCraft](https://github.com/storytold/lightcraft) | `crates/raw/src/saturation.rs` | `b4c4a436cb41169122a5e1435b2d9df382b705f2` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/lc-raw/src/saturation.rs` | [LightCraft](https://github.com/storytold/lightcraft) | `crates/raw/src/saturation.rs` | `010d9bb5daac705d58bf77467f5440887df2b10f` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/lc-raw/src/vendor/arw.rs` | [LightCraft](https://github.com/storytold/lightcraft) | `crates/raw/src/vendor/arw.rs` | `3100cc1a5c72c8a5f7c9702f70d0be629d301707` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/lc-raw/src/vendor/arw.rs` | [LightCraft](https://github.com/storytold/lightcraft) | `crates/raw/src/vendor/arw.rs` | `179910c020d0359eb549e9326c6911259b51ee34` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/lc-raw/src/vendor/mod.rs` | [LightCraft](https://github.com/storytold/lightcraft) | `crates/raw/src/vendor/mod.rs` | `179910c020d0359eb549e9326c6911259b51ee34` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/lc-raw/src/vendor/sr2.rs` | [LightCraft](https://github.com/storytold/lightcraft) | `crates/raw/src/vendor/sr2.rs` | `179910c020d0359eb549e9326c6911259b51ee34` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/lc-raw/src/vendor/nef.rs` | [LightCraft](https://github.com/storytold/lightcraft) | `crates/raw/src/vendor/nef.rs` | `5bcedbe6fc11ed9e23f47b18328a602b19dd5525` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/lc-raw/src/vendor/nef.rs` | [LightCraft](https://github.com/storytold/lightcraft) | `crates/raw/src/vendor/nef.rs` | `3d9f8492da75d152211d071405aee9c1297b3af9` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/lc-raw/src/tiffraw.rs` | [LightCraft](https://github.com/storytold/lightcraft) | `crates/raw/src/tiffraw.rs` | `a12f1697a162a4af2a4d8fef9c5226c097c21aee` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/lc-raw/src/unpack.rs` | [LightCraft](https://github.com/storytold/lightcraft) | `crates/raw/src/unpack.rs` | `a12f1697a162a4af2a4d8fef9c5226c097c21aee` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/lc-raw/src/vendor/nef.rs` | [LightCraft](https://github.com/storytold/lightcraft) | `crates/raw/src/vendor/nef.rs` | `a12f1697a162a4af2a4d8fef9c5226c097c21aee` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/lc-raw/src/vendor/raf.rs` | [LightCraft](https://github.com/storytold/lightcraft) | `crates/raw/src/vendor/raf.rs` | `a12f1697a162a4af2a4d8fef9c5226c097c21aee` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/lc-raw/src/vendor/arw.rs` | [LightCraft](https://github.com/storytold/lightcraft) | `crates/raw/src/vendor/arw.rs` | `f1fa336ab13b5fb5224391e6fb50ca33041a77f3` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/lc-raw/src/vendor/arw.rs` | [LightCraft](https://github.com/storytold/lightcraft) | `crates/raw/src/vendor/arw.rs` | `cfca2999451e5514474ee4cbac1a4f301ebc192c` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/lc-raw/src/vendor/arw.rs` | [LightCraft](https://github.com/storytold/lightcraft) | `crates/raw/src/vendor/arw.rs` | `5ed59252e87a653741deada5155544735d337d5f` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/lc-raw/src/lib.rs` | [LightCraft](https://github.com/storytold/lightcraft) | `crates/raw/src/lib.rs` | `169a4ed059b2227e320c4c432654f8f7cfc41a42` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/lc-raw/src/vendor/orf.rs` | [LightCraft](https://github.com/storytold/lightcraft) | `crates/raw/src/vendor/orf.rs` | `169a4ed059b2227e320c4c432654f8f7cfc41a42` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/lc-raw/src/vendor/orf.rs` | [LightCraft](https://github.com/storytold/lightcraft) | `crates/raw/src/vendor/orf.rs` | `37d36215056838bc705900984a5f503961a1b14d` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/lc-raw/src/lib.rs` | [LightCraft](https://github.com/storytold/lightcraft) | `crates/raw/src/lib.rs` | `44b126274d3a4e0e8237de3eec67a8ad1064a5f4` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/lc-raw/src/vendor/orf.rs` | [LightCraft](https://github.com/storytold/lightcraft) | `crates/raw/src/vendor/orf.rs` | `44b126274d3a4e0e8237de3eec67a8ad1064a5f4` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/lc-raw/src/lib.rs` | [LightCraft](https://github.com/storytold/lightcraft) | `crates/raw/src/lib.rs` | `989422398c3044b39b640111dc10cafebf5ba92e` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/lc-raw/src/vendor/mod.rs` | [LightCraft](https://github.com/storytold/lightcraft) | `crates/raw/src/vendor/mod.rs` | `989422398c3044b39b640111dc10cafebf5ba92e` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/lc-raw/src/vendor/orf.rs` | [LightCraft](https://github.com/storytold/lightcraft) | `crates/raw/src/vendor/orf.rs` | `989422398c3044b39b640111dc10cafebf5ba92e` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/lc-raw/src/vendor/pef.rs` | [LightCraft](https://github.com/storytold/lightcraft) | `crates/raw/src/vendor/pef.rs` | `989422398c3044b39b640111dc10cafebf5ba92e` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/lc-raw/src/vendor/orf.rs` | [LightCraft](https://github.com/storytold/lightcraft) | `crates/raw/src/vendor/orf.rs` | `82566817e6b16bd82bcdd3a60551b516394f5faf` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/pc-format/src/finite.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `crates/format/src/finite.rs` | `9e6bfdc76cde3be0ece15132fc1dfb342f8adaaa` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/pc-format/src/lib.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `crates/format/src/lib.rs` | `9e6bfdc76cde3be0ece15132fc1dfb342f8adaaa` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/pc-format/src/store.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `crates/format/src/store.rs` | `9e6bfdc76cde3be0ece15132fc1dfb342f8adaaa` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/pc-format/tests/it/roundtrip.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `crates/format/tests/roundtrip.rs` | `9e6bfdc76cde3be0ece15132fc1dfb342f8adaaa` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/pc-format/fuzz/fuzz_targets/load.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `crates/format/fuzz/fuzz_targets/load.rs` | `6e9ba6511d507dafc7809927f153d818f592f8a8` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/pc-format/src/lib.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `crates/format/src/lib.rs` | `6e9ba6511d507dafc7809927f153d818f592f8a8` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/pc-format/src/store.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `crates/format/src/store.rs` | `6e9ba6511d507dafc7809927f153d818f592f8a8` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/pc-format/tests/it/corruption.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `crates/format/tests/corruption.rs` | `6e9ba6511d507dafc7809927f153d818f592f8a8` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/pc-format/src/store.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `crates/format/src/store.rs` | `73a56d76032a9d94cda7c838d110f82c3914d8be` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/pc-format/src/zip.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `crates/format/src/zip.rs` | `73a56d76032a9d94cda7c838d110f82c3914d8be` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/pc-format/tests/it/corruption.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `crates/format/tests/corruption.rs` | `73a56d76032a9d94cda7c838d110f82c3914d8be` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/pc-psd/src/path.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `crates/psd/src/path.rs` | `7c77143b97d2e4530dda666dbdd8677483f858e3` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/pc-codecs/src/codecs/png.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `crates/codecs/src/codecs/png.rs` | `21f1012ce2ef3d90c2f6e867dd84c0edc13d6401` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/pc-codecs/src/codecs/webp.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `crates/codecs/src/codecs/webp.rs` | `21f1012ce2ef3d90c2f6e867dd84c0edc13d6401` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/pc-codecs/src/options.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `crates/codecs/src/options.rs` | `21f1012ce2ef3d90c2f6e867dd84c0edc13d6401` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/pc-codecs/tests/it/limits.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `crates/codecs/tests/limits.rs` | `21f1012ce2ef3d90c2f6e867dd84c0edc13d6401` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/pc-codecs/src/codecs/tiff_ifd.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `crates/codecs/src/codecs/tiff_ifd.rs` | `476f9838765f619dd3c5d93ddac8e6a5d2e10f5d` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/pc-codecs/src/codecs/png.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `crates/codecs/src/codecs/png.rs` | `7fd610741bc207b4dcc6b4dbdd78a4c23235f2ba` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/pc-codecs/src/options.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `crates/codecs/src/options.rs` | `7fd610741bc207b4dcc6b4dbdd78a4c23235f2ba` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/pc-codecs/tests/it/limits.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `crates/codecs/tests/limits.rs` | `7fd610741bc207b4dcc6b4dbdd78a4c23235f2ba` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/pc-raw/src/sensor.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `crates/raw/src/sensor.rs` | `308ef6329471259e096e1ede1a92ab91f0f8d6e1` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/pc-raw/tests/it/hostile.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `crates/raw/tests/hostile.rs` | `308ef6329471259e096e1ede1a92ab91f0f8d6e1` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/pc-io/src/psd_export.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `crates/io/src/psd_export.rs` | `3f72158f0af959674651bfb8086ee000bf8f5157` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/pc-io/tests/it/roundtrip.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `crates/io/tests/roundtrip.rs` | `3f72158f0af959674651bfb8086ee000bf8f5157` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/pc-psd/src/compression.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `crates/psd/src/compression.rs` | `0fb4bbf1dbf25a56bd1e11e412747bc2a07559fe` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/pc-psd/tests/it/malformed.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `crates/psd/tests/malformed.rs` | `0fb4bbf1dbf25a56bd1e11e412747bc2a07559fe` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/pc-psd/src/descriptor.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `crates/psd/src/descriptor.rs` | `64a02107bad9281e0cebfad3afab66481d79cbb8` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/pc-psd/src/patterns.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `crates/psd/src/patterns.rs` | `16341720c0166050b43b64c13f1d106577853b9c` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/pc-psd/src/pixels.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `crates/psd/src/pixels.rs` | `04396dd85f7e234702cd09ef48b0e8b2f58b7aed` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/pc-io/src/lib.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `crates/io/src/lib.rs` | `fd351918e52f31df3f692a063c5db959bc5c9365` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/pc-io/src/psd_import.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `crates/io/src/psd_import.rs` | `fd351918e52f31df3f692a063c5db959bc5c9365` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/pc-io/src/tiff_layers.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `crates/io/src/tiff_layers.rs` | `fd351918e52f31df3f692a063c5db959bc5c9365` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/pc-io/tests/it/tiff_layers.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `crates/io/tests/tiff_layers.rs` | `fd351918e52f31df3f692a063c5db959bc5c9365` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/pc-psd/src/tree.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `crates/psd/src/tree.rs` | `fd351918e52f31df3f692a063c5db959bc5c9365` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/pc-psd/tests/it/tree.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `crates/psd/tests/tree.rs` | `fd351918e52f31df3f692a063c5db959bc5c9365` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/pc-io/src/psd_import.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `crates/io/src/psd_import.rs` | `dc256b8b39ddba16241c0db2ba07a0371f2a762c` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/pc-io/tests/it/psd_structure.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `crates/io/tests/psd_structure.rs` | `dc256b8b39ddba16241c0db2ba07a0371f2a762c` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/pc-codecs/src/codecs/pnm.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `crates/codecs/src/codecs/pnm.rs` | `59820ecafbd74aac5d4d80258eb69b482bd1d8ce` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/pc-codecs/tests/pnm_peak_memory.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `crates/codecs/tests/pnm_peak_memory.rs` | `59820ecafbd74aac5d4d80258eb69b482bd1d8ce` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/pc-codecs/src/codecs/pnm.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `crates/codecs/src/codecs/pnm.rs` | `9af9df82756c2f80b764400590bb957c5c3e2e48` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/pc-codecs/tests/it/limits.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `crates/codecs/tests/limits.rs` | `9af9df82756c2f80b764400590bb957c5c3e2e48` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/pc-format/src/convert.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `crates/format/src/convert.rs` | `b2003a4329bfd3b52ac61d0a00781a64ada38668` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/pc-format/tests/it/corruption.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `crates/format/tests/corruption.rs` | `b2003a4329bfd3b52ac61d0a00781a64ada38668` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/pc-format/src/zip.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `crates/format/src/zip.rs` | `3e9359071c1cd3c8acc413def68d23e37cbfb463` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/pc-format/src/convert.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `crates/format/src/convert.rs` | `fd21e734111ef0af3f26b77e3e59d3d651c50f91` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/pc-format/tests/it/corruption.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `crates/format/tests/corruption.rs` | `fd21e734111ef0af3f26b77e3e59d3d651c50f91` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/pc-format/src/convert.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `crates/format/src/convert.rs` | `0050f29a85745642ba88afc80143674e92a55b4b` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/pc-raw/src/cr2.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `crates/raw/src/cr2.rs` | `86a629939a14f3886c042954c28729a20a79e1e2` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/pc-raw/src/dng.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `crates/raw/src/dng.rs` | `86a629939a14f3886c042954c28729a20a79e1e2` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/pc-raw/src/orf.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `crates/raw/src/orf.rs` | `86a629939a14f3886c042954c28729a20a79e1e2` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/pc-raw/src/rw2.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `crates/raw/src/rw2.rs` | `86a629939a14f3886c042954c28729a20a79e1e2` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/pc-raw/src/tiff.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `crates/raw/src/tiff.rs` | `86a629939a14f3886c042954c28729a20a79e1e2` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/pc-raw/src/tiffep.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `crates/raw/src/tiffep.rs` | `86a629939a14f3886c042954c28729a20a79e1e2` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/pc-raw/tests/it/dng.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `crates/raw/tests/dng.rs` | `86a629939a14f3886c042954c28729a20a79e1e2` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/pc-raw/src/orf.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `crates/raw/src/orf.rs` | `9d9e512d40a25a29dc51eebec4396e80220019a1` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/pc-raw/tests/it/vendor_codecs.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `crates/raw/tests/vendor_codecs.rs` | `9d9e512d40a25a29dc51eebec4396e80220019a1` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/pc-raw/src/dng.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `crates/raw/src/dng.rs` | `0bb46df63859714cf1c0ea90fd6f2e990da63d2a` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/pc-raw/src/testgen.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `crates/raw/src/testgen.rs` | `0bb46df63859714cf1c0ea90fd6f2e990da63d2a` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/pc-raw/tests/it/dng.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `crates/raw/tests/dng.rs` | `0bb46df63859714cf1c0ea90fd6f2e990da63d2a` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/pc-raw/src/lib.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `crates/raw/src/lib.rs` | `88f84aab59b2bf4b2acd959129600bedddbb1a77` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/pc-raw/src/saturation.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `crates/raw/src/saturation.rs` | `88f84aab59b2bf4b2acd959129600bedddbb1a77` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/pc-raw/tests/it/dng.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `crates/raw/tests/dng.rs` | `88f84aab59b2bf4b2acd959129600bedddbb1a77` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/pc-io/src/annotations_map.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `crates/io/src/annotations_map.rs` | `4f33c8152485cc940785a74db81fbd6b54ac9a08` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/pc-io/src/psd_import.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `crates/io/src/psd_import.rs` | `4f33c8152485cc940785a74db81fbd6b54ac9a08` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/pc-io/tests/it/annotations.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `crates/io/tests/annotations.rs` | `4f33c8152485cc940785a74db81fbd6b54ac9a08` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/pc-io/src/comps_map.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `crates/io/src/comps_map.rs` | `f86517a029d96f887cd82ff8062a5e7759372c5e` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/pc-io/src/smart_map.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `crates/io/src/smart_map.rs` | `f86517a029d96f887cd82ff8062a5e7759372c5e` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/pc-psd/src/descriptor.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `crates/psd/src/descriptor.rs` | `f86517a029d96f887cd82ff8062a5e7759372c5e` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/pc-io/src/psd_export.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `crates/io/src/psd_export.rs` | `e2aa19341f73a254f901c7c8c8c2e3a7e919ea44` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/pc-io/tests/it/psd_block_alignment.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `crates/io/tests/psd_block_alignment.rs` | `e2aa19341f73a254f901c7c8c8c2e3a7e919ea44` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/pc-io/src/psd_export.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `crates/io/src/psd_export.rs` | `c833294e34124eb8025de91627eb5a2d3a50f422` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/pc-io/tests/it/psd_empty_channels.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `crates/io/tests/psd_empty_channels.rs` | `c833294e34124eb8025de91627eb5a2d3a50f422` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/pc-io/src/adjust_map.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `crates/io/src/adjust_map.rs` | `0d058f4c29dee846bc48635014366922ba81011e` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/pc-io/src/psd_export.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `crates/io/src/psd_export.rs` | `0d058f4c29dee846bc48635014366922ba81011e` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/pc-psd/src/resources.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `crates/psd/src/resources.rs` | `6f1c4481bfe158eccd5ed9b0bae344145f74e165` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/pc-text/src/psd.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `crates/text/src/psd.rs` | `6f1c4481bfe158eccd5ed9b0bae344145f74e165` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/pc-io/src/multichannel_map.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `crates/io/src/multichannel_map.rs` | `6547ca0b8459ef37e1dfb373cc00a81740ed74a4` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/pc-io/src/psd_export.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `crates/io/src/psd_export.rs` | `6547ca0b8459ef37e1dfb373cc00a81740ed74a4` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/pc-engine/src/retouch_cmds.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `crates/engine/src/retouch_cmds.rs` | `af3c99d817609c981ec2fc0296cd81b2ba9aa450` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/pc-engine/src/retouch_cmds/tests.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `crates/engine/src/retouch_cmds/tests.rs` | `af3c99d817609c981ec2fc0296cd81b2ba9aa450` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/pc-algo/src/segment/quick.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `crates/algo/src/segment/quick.rs` | `5049a1d06218211bf482e48d9070a9bd414dec3f` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/pc-algo/src/segment/tests.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `crates/algo/src/segment/tests.rs` | `5049a1d06218211bf482e48d9070a9bd414dec3f` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/pc-algo/src/vanishing.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `crates/algo/src/vanishing.rs` | `5049a1d06218211bf482e48d9070a9bd414dec3f` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/pc-compose/src/adjust.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `crates/compose/src/adjust.rs` | `5049a1d06218211bf482e48d9070a9bd414dec3f` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/pc-engine/src/layer_multi_cmds.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `crates/engine/src/layer_multi_cmds.rs` | `5049a1d06218211bf482e48d9070a9bd414dec3f` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/pc-compose/src/effects.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `crates/compose/src/effects.rs` | `f82a55f0560122ec8a12730a6e0baf9e57039f77` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/pc-engine/src/layer_style.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `crates/engine/src/layer_style.rs` | `f82a55f0560122ec8a12730a6e0baf9e57039f77` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/pc-compose/src/adjust.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `crates/compose/src/adjust.rs` | `2842973dc01c437e7318e37faf1fa90aa2e4545d` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/pc-compose/src/effects.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `crates/compose/src/effects.rs` | `2842973dc01c437e7318e37faf1fa90aa2e4545d` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/pc-compose/src/lib.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `crates/compose/src/lib.rs` | `2842973dc01c437e7318e37faf1fa90aa2e4545d` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/pc-compose/src/masks.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `crates/compose/src/masks.rs` | `2842973dc01c437e7318e37faf1fa90aa2e4545d` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/pc-compose/src/pattern.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `crates/compose/src/pattern.rs` | `2842973dc01c437e7318e37faf1fa90aa2e4545d` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/pc-compose/src/proxy.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `crates/compose/src/proxy.rs` | `2842973dc01c437e7318e37faf1fa90aa2e4545d` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/pc-compose/src/tests.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `crates/compose/src/tests.rs` | `2842973dc01c437e7318e37faf1fa90aa2e4545d` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/pc-engine/src/web_cmds.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `crates/engine/src/web_cmds.rs` | `61b6276c78f1df34e0967ba78e235b3b0c055ba8` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/pc-engine/src/web_cmds/tests.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `crates/engine/src/web_cmds/tests.rs` | `61b6276c78f1df34e0967ba78e235b3b0c055ba8` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/pc-engine/src/extra_cmds.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `crates/engine/src/extra_cmds.rs` | `81f974503358e57a34f920f7aa4924ad84f85a39` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/pc-compose/src/lib.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `crates/compose/src/lib.rs` | `f1433f4ab2968931905e5c4cda7e13a143572829` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/pc-compose/src/psblend.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `crates/compose/src/psblend.rs` | `f1433f4ab2968931905e5c4cda7e13a143572829` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/pc-compose/src/tests.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `crates/compose/src/tests.rs` | `f1433f4ab2968931905e5c4cda7e13a143572829` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/pc-engine/src/file_cmds.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `crates/engine/src/file_cmds.rs` | `3cc0d48410b00d21da06a9f9b391d52a79f6a938` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/pc-engine/src/file_cmds/tests.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `crates/engine/src/file_cmds/tests.rs` | `3cc0d48410b00d21da06a9f9b391d52a79f6a938` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/pc-engine/src/preset_store.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `crates/engine/src/preset_store.rs` | `fb36a3fdc49b1a30f433339ffcd7170a67f72ffa` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/pc-engine/src/preset_store/tests.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `crates/engine/src/preset_store/tests.rs` | `fb36a3fdc49b1a30f433339ffcd7170a67f72ffa` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/pc-engine/src/channel_cmds.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `crates/engine/src/channel_cmds.rs` | `278e2941315957c6860f0638ab2196fdf00f5064` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/pc-engine/src/channel_cmds/tests.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `crates/engine/src/channel_cmds/tests.rs` | `278e2941315957c6860f0638ab2196fdf00f5064` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/pc-engine/src/commands.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `crates/engine/src/commands.rs` | `2425431c78c40b12593b5e6ec79dc309de8369ff` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/pc-engine/src/layer_multi_cmds.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `crates/engine/src/layer_multi_cmds.rs` | `0954b8c0658f66a2c121d8df0e37e2fa0ec7bef2` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/pc-engine/src/timeline_cmds.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `crates/engine/src/timeline_cmds.rs` | `e1a874d9e9374c94e24adaa80a00b6071068b970` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/pc-engine/src/image_cmds.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `crates/engine/src/image_cmds.rs` | `0c2c268c49ee29c376315b024f066bd67c56b2f0` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/pc-engine/src/image_cmds.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `crates/engine/src/image_cmds.rs` | `fd74419aeb1e9a6d280678e09b2ac0c5049687c3` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/pc-engine/src/edit_menu_cmds.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `crates/engine/src/edit_menu_cmds.rs` | `3ac39e1edefb6847de06f7174bf6c9caf570c4d6` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/pc-engine/src/edit_menu_cmds/tests.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `crates/engine/src/edit_menu_cmds/tests.rs` | `3ac39e1edefb6847de06f7174bf6c9caf570c4d6` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/pc-engine/src/fill_cmds.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `crates/engine/src/fill_cmds.rs` | `3ac39e1edefb6847de06f7174bf6c9caf570c4d6` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/pc-engine/src/retouch_cmds/content_aware_move.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `crates/engine/src/retouch_cmds/content_aware_move.rs` | `3ac39e1edefb6847de06f7174bf6c9caf570c4d6` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/pc-algo/src/puppet.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `crates/algo/src/puppet.rs` | `64db46f5abab516ed01ccd2efff009049274e450` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/pc-engine/src/distort_cmds/tests.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `crates/engine/src/distort_cmds/tests.rs` | `64db46f5abab516ed01ccd2efff009049274e450` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/pc-algo/src/puppet.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `crates/algo/src/puppet.rs` | `3f7d8afc7886b4732a48667959d9f291e5928ac4` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/pc-engine/src/distort_cmds.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `crates/engine/src/distort_cmds.rs` | `3f7d8afc7886b4732a48667959d9f291e5928ac4` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/pc-engine/src/distort_cmds/tests.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `crates/engine/src/distort_cmds/tests.rs` | `3f7d8afc7886b4732a48667959d9f291e5928ac4` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/pc-engine/src/brush_cmds.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `crates/engine/src/brush_cmds.rs` | `c88dbd26cd272f14ef54afcc18d18a5d6947b7b7` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/pc-engine/src/brush_cmds/tests.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `crates/engine/src/brush_cmds/tests.rs` | `c88dbd26cd272f14ef54afcc18d18a5d6947b7b7` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/pc-engine/src/brush_cmds.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `crates/engine/src/brush_cmds.rs` | `7fec6e9469cee47d0090589ec51be885c7c58b8e` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/pc-engine/src/brush_cmds/tests.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `crates/engine/src/brush_cmds/tests.rs` | `7fec6e9469cee47d0090589ec51be885c7c58b8e` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/pc-engine/src/retouch_cmds.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `crates/engine/src/retouch_cmds.rs` | `7fec6e9469cee47d0090589ec51be885c7c58b8e` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/pc-engine/src/vector_cmds.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `crates/engine/src/vector_cmds.rs` | `7fec6e9469cee47d0090589ec51be885c7c58b8e` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/pc-paint/src/brush.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `crates/paint/src/brush.rs` | `7fec6e9469cee47d0090589ec51be885c7c58b8e` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/pc-paint/src/brush_tests.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `crates/paint/src/brush_tests.rs` | `7fec6e9469cee47d0090589ec51be885c7c58b8e` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/pc-paint/src/lib.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `crates/paint/src/lib.rs` | `7fec6e9469cee47d0090589ec51be885c7c58b8e` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/pc-paint/src/render.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `crates/paint/src/render.rs` | `7fec6e9469cee47d0090589ec51be885c7c58b8e` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/pc-paint/src/retouch.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `crates/paint/src/retouch.rs` | `7fec6e9469cee47d0090589ec51be885c7c58b8e` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/pc-engine/src/tests.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `crates/engine/src/tests.rs` | `874867461d8af843802f28b8522a6096cfc44e09` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/pc-raster/src/lib.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `crates/raster/src/lib.rs` | `874867461d8af843802f28b8522a6096cfc44e09` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/pc-compose/src/lib.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `crates/compose/src/lib.rs` | `f586e7133ef302d241b42c3eaabe1881d38d55cc` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/pc-compose/src/masks.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `crates/compose/src/masks.rs` | `576811263ea2d65367e3c1c09a9276780e7e13d3` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/pc-compose/src/lib.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `crates/compose/src/lib.rs` | `980cf05be49231490a6a4687a035d5dc8cb0fa35` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/pc-compose/src/tests.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `crates/compose/src/tests.rs` | `980cf05be49231490a6a4687a035d5dc8cb0fa35` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/pc-algo/src/selection.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `crates/algo/src/selection.rs` | `3953d78378fd78c3680c78423267f708505a95d3` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/pc-algo/src/selection/distance.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `crates/algo/src/selection/distance.rs` | `3953d78378fd78c3680c78423267f708505a95d3` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/pc-algo/src/selection/distance/tests.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `crates/algo/src/selection/distance/tests.rs` | `3953d78378fd78c3680c78423267f708505a95d3` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/lc-meta/src/xmp_merge.rs` | [LightCraft](https://github.com/storytold/lightcraft) | `crates/meta/src/xmp_merge.rs` | `87cd873d78c68de4514d2d911b98aa82ee9a9015` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/lc-catalog/src/lib.rs` | [LightCraft](https://github.com/storytold/lightcraft) | `crates/catalog/src/lib.rs` | `0a474cc06e98778f1f7cfeb7d36ab41d1bb66a18` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/lc-engine/src/cmd/library.rs` | [LightCraft](https://github.com/storytold/lightcraft) | `crates/engine/src/cmd/library.rs` | `0a474cc06e98778f1f7cfeb7d36ab41d1bb66a18` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/lc-engine/src/import.rs` | [LightCraft](https://github.com/storytold/lightcraft) | `crates/engine/src/import.rs` | `790a3417765183138afd898ea0f9848a08820da1` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/lc-engine/src/lib.rs` | [LightCraft](https://github.com/storytold/lightcraft) | `crates/engine/src/lib.rs` | `790a3417765183138afd898ea0f9848a08820da1` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/lc-engine/src/tests_import.rs` | [LightCraft](https://github.com/storytold/lightcraft) | `crates/engine/src/tests_import.rs` | `790a3417765183138afd898ea0f9848a08820da1` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/lc-engine/src/walk.rs` | [LightCraft](https://github.com/storytold/lightcraft) | `crates/engine/src/walk.rs` | `790a3417765183138afd898ea0f9848a08820da1` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/lc-catalog/src/keywords.rs` | [LightCraft](https://github.com/storytold/lightcraft) | `crates/catalog/src/keywords.rs` | `cdca2b9cf835816babc1b05938b3b2272c5f53ac` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/lc-engine/src/cmd/keywords.rs` | [LightCraft](https://github.com/storytold/lightcraft) | `crates/engine/src/cmd/keywords.rs` | `cdca2b9cf835816babc1b05938b3b2272c5f53ac` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/lc-catalog/src/keywords.rs` | [LightCraft](https://github.com/storytold/lightcraft) | `crates/catalog/src/keywords.rs` | `cd8dfa72c262e44568c4b76f164be6f0784f950d` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/lc-ui-egui/src/lib.rs` | [LightCraft](https://github.com/storytold/lightcraft) | `crates/ui-egui/src/lib.rs` | `e1267a713949277e9ceac712cfc8ae18b26a578e` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/lc-ui-egui/src/shortcuts.rs` | [LightCraft](https://github.com/storytold/lightcraft) | `crates/ui-egui/src/shortcuts.rs` | `e1267a713949277e9ceac712cfc8ae18b26a578e` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/lc-ui-egui/src/tests_dialogs.rs` | [LightCraft](https://github.com/storytold/lightcraft) | `crates/ui-egui/src/tests_dialogs.rs` | `e1267a713949277e9ceac712cfc8ae18b26a578e` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/lc-ui-egui/src/lib.rs` | [LightCraft](https://github.com/storytold/lightcraft) | `crates/ui-egui/src/lib.rs` | `2c8520371ca6c88e9f79a5cd22f02bdb09c98a81` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/lc-engine/src/cmd/manage.rs` | [LightCraft](https://github.com/storytold/lightcraft) | `crates/engine/src/cmd/manage.rs` | `7ba1470940bc588734affb959b5c54daacfa240b` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/lc-engine/src/cmd/manage.rs` | [LightCraft](https://github.com/storytold/lightcraft) | `crates/engine/src/cmd/manage.rs` | `11e36bf060ce4a821b1c0d21626533101cd3483f` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/lc-engine/src/cmd/manage.rs` | [LightCraft](https://github.com/storytold/lightcraft) | `crates/engine/src/cmd/manage.rs` | `4266b0ad7f62e5c05e1d3dc0c5ea55319a0af483` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/lc-engine/src/tests_libops.rs` | [LightCraft](https://github.com/storytold/lightcraft) | `crates/engine/src/tests_libops.rs` | `4266b0ad7f62e5c05e1d3dc0c5ea55319a0af483` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/lc-catalog/src/dates.rs` | [LightCraft](https://github.com/storytold/lightcraft) | `crates/catalog/src/dates.rs` | `a08d0f6b03cfee22ce8fec01088511e12d0fd663` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/lc-catalog/src/rules.rs` | [LightCraft](https://github.com/storytold/lightcraft) | `crates/catalog/src/rules.rs` | `a08d0f6b03cfee22ce8fec01088511e12d0fd663` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/lc-engine/src/export.rs` | [LightCraft](https://github.com/storytold/lightcraft) | `crates/engine/src/export.rs` | `db7e37843eb018bff358ed7623afbf910dc04d29` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/lc-engine/src/tests_export.rs` | [LightCraft](https://github.com/storytold/lightcraft) | `crates/engine/src/tests_export.rs` | `db7e37843eb018bff358ed7623afbf910dc04d29` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/lc-engine/src/cmd/browse.rs` | [LightCraft](https://github.com/storytold/lightcraft) | `crates/engine/src/cmd/browse.rs` | `3285e0282d9bddd9e24263dfdb0d8e0f83d78ea6` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/lc-engine/src/cmd/library.rs` | [LightCraft](https://github.com/storytold/lightcraft) | `crates/engine/src/cmd/library.rs` | `3285e0282d9bddd9e24263dfdb0d8e0f83d78ea6` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/lc-engine/src/import.rs` | [LightCraft](https://github.com/storytold/lightcraft) | `crates/engine/src/import.rs` | `3285e0282d9bddd9e24263dfdb0d8e0f83d78ea6` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/lc-engine/src/tests_import.rs` | [LightCraft](https://github.com/storytold/lightcraft) | `crates/engine/src/tests_import.rs` | `3285e0282d9bddd9e24263dfdb0d8e0f83d78ea6` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/lc-ui-egui/src/headless.rs` | [LightCraft](https://github.com/storytold/lightcraft) | `crates/ui-egui/src/headless.rs` | `3285e0282d9bddd9e24263dfdb0d8e0f83d78ea6` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/lc-ui-egui/src/import.rs` | [LightCraft](https://github.com/storytold/lightcraft) | `crates/ui-egui/src/import.rs` | `3285e0282d9bddd9e24263dfdb0d8e0f83d78ea6` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/lc-engine/src/cmd/library.rs` | [LightCraft](https://github.com/storytold/lightcraft) | `crates/engine/src/cmd/library.rs` | `2928880e8120e91465a7dd4ed2c18bfbdecc6e11` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/lc-engine/src/import.rs` | [LightCraft](https://github.com/storytold/lightcraft) | `crates/engine/src/import.rs` | `2928880e8120e91465a7dd4ed2c18bfbdecc6e11` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/lc-engine/src/tests_import.rs` | [LightCraft](https://github.com/storytold/lightcraft) | `crates/engine/src/tests_import.rs` | `2928880e8120e91465a7dd4ed2c18bfbdecc6e11` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/lc-engine/src/import.rs` | [LightCraft](https://github.com/storytold/lightcraft) | `crates/engine/src/import.rs` | `5a21fc9d9733a58de997053fd99825f44ea69063` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/lc-engine/src/tests_import.rs` | [LightCraft](https://github.com/storytold/lightcraft) | `crates/engine/src/tests_import.rs` | `5a21fc9d9733a58de997053fd99825f44ea69063` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/lc-engine/src/segment/mod.rs` | [LightCraft](https://github.com/storytold/lightcraft) | `crates/engine/src/segment/mod.rs` | `4b57d5dc5a111f761e5a7abf0e1ae6cf6baebe3f` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/lc-segment/src/fetch/mirror.rs` | [LightCraft](https://github.com/storytold/lightcraft) | `crates/fetch/src/lib.rs` | `4b57d5dc5a111f761e5a7abf0e1ae6cf6baebe3f` | Apache-2.0 | 2026-10-13 |
| `crates/lc-segment/src/fetch/mirror/tests.rs` | [LightCraft](https://github.com/storytold/lightcraft) | `crates/fetch/src/tests.rs` | `4b57d5dc5a111f761e5a7abf0e1ae6cf6baebe3f` | Apache-2.0 | 2026-10-13 |
| `crates/lc-segment/src/fetch/mirror.rs` | [LightCraft](https://github.com/storytold/lightcraft) | `crates/fetch/src/lib.rs` | `8dd3afce51d27d2625f8e094e6dd11b713cede01` | Apache-2.0 | 2026-10-13 |
| `crates/lc-segment/src/fetch/mirror/tests.rs` | [LightCraft](https://github.com/storytold/lightcraft) | `crates/fetch/src/tests.rs` | `8dd3afce51d27d2625f8e094e6dd11b713cede01` | Apache-2.0 | 2026-10-13 |
| `apps/local-image/src/services.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `apps/photocraft/src/services.rs` | `9b4562f2a054a104712d03c2ba11a3aa07e93614` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/pc-ui-egui/src/type_tool.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `crates/ui-egui/src/type_tool.rs` | `c1cb455c7aa94d436f1cae20786ac071a0ca0c29` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/pc-ui-egui/src/theme.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `crates/ui-egui/src/theme.rs` | `9cdcfbb1b7b204b57e04bd6bc3a4a3815a0225a1` | MIT OR Apache-2.0 | 2026-10-13 |
| `apps/local-image/src/services.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `apps/photocraft/src/services.rs` | `3952fcef792651c79972a6a8c398f0570847d0d7` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/pc-ui-egui/src/jobs_ui.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `crates/ui-egui/src/jobs_ui.rs` | `be877c27d0cb9db43a11fad5c74615eec388664c` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/pc-ui-egui/src/jobs_ui_tests.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `crates/ui-egui/src/jobs_ui_tests.rs` | `be877c27d0cb9db43a11fad5c74615eec388664c` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/pc-ui-egui/src/lib.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `crates/ui-egui/src/lib.rs` | `be877c27d0cb9db43a11fad5c74615eec388664c` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/pc-ui-egui/src/prefs_ui.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `crates/ui-egui/src/prefs_ui.rs` | `57be0f58d8103f9f90b8252e0e4730133618bb3a` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/pc-format/src/autosave.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `crates/format/src/autosave.rs` | `3ef631619e08607fb338b68b45455f59c230d226` | MIT OR Apache-2.0 | 2026-10-13 |
| `apps/local-image/src/services.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `apps/photocraft/src/services.rs` | `83d5c5436dc11e2327747a7b67704e29aba789ba` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/pc-format/src/autosave.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `crates/format/src/autosave.rs` | `83d5c5436dc11e2327747a7b67704e29aba789ba` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/pc-ui-egui/src/lib.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `crates/ui-egui/src/lib.rs` | `83d5c5436dc11e2327747a7b67704e29aba789ba` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/pc-ui-egui/src/prefs_ui.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `crates/ui-egui/src/prefs_ui.rs` | `83d5c5436dc11e2327747a7b67704e29aba789ba` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/pc-format/src/autosave.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `crates/format/src/autosave.rs` | `c20b81b25168057e9cbffa2f45c36ec02dacf756` | MIT OR Apache-2.0 | 2026-10-13 |
| `apps/local-image/src/linux_libs.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `apps/photocraft/src/linux_libs.rs` | `d29d81d5c0be43acd8e8e1c9e3abeff119fde6db` | MIT OR Apache-2.0 | 2026-10-13 |
| `apps/local-image/src/file_dialog_probe.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `apps/photocraft/src/file_dialog_probe.rs` | `b3993e89b351b4dda9276de9eb16a3bf6bbf5580` | MIT OR Apache-2.0 | 2026-10-13 |
| `apps/local-image/src/linux_libs.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `apps/photocraft/src/linux_libs.rs` | `b3993e89b351b4dda9276de9eb16a3bf6bbf5580` | MIT OR Apache-2.0 | 2026-10-13 |
| `apps/local-image/src/main.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `apps/photocraft/src/main.rs` | `b3993e89b351b4dda9276de9eb16a3bf6bbf5580` | MIT OR Apache-2.0 | 2026-10-13 |
| `apps/local-image/src/services.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `apps/photocraft/src/services.rs` | `b3993e89b351b4dda9276de9eb16a3bf6bbf5580` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/pc-text/src/cjk.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `crates/text/src/cjk.rs` | `1a9a39eadf86adcadb229dd646ed9408213c9793` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/pc-text/src/fonts.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `crates/text/src/fonts.rs` | `82c58fabab60890697fc4bb492399b7f9f279618` | MIT OR Apache-2.0 | 2026-10-13 |
| `apps/local-image/src/main.rs` | [LightCraft](https://github.com/storytold/lightcraft) | `apps/lightcraft/src/main.rs` | `9c2bad114662634deb6aedd4fce4680066b66349` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/pc-format/src/store.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `crates/format/src/store.rs` | `c20b81b25168057e9cbffa2f45c36ec02dacf756` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/pc-format/tests/it/autosave.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `crates/format/tests/autosave.rs` | `c20b81b25168057e9cbffa2f45c36ec02dacf756` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/pc-gpu/src/plan.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `crates/gpu/src/plan.rs` | `5049a1d06218211bf482e48d9070a9bd414dec3f` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/pc-gpu/src/bounds.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `crates/gpu/src/bounds.rs` | `980cf05be49231490a6a4687a035d5dc8cb0fa35` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/pc-gpu/tests/gpu/parity.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `crates/gpu/tests/parity.rs` | `980cf05be49231490a6a4687a035d5dc8cb0fa35` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/pc-doc/src/video.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `crates/doc/src/video.rs` | `e1a874d9e9374c94e24adaa80a00b6071068b970` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/pc-compose/src/pattern.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `crates/compose/src/pattern.rs` | `fd21e734111ef0af3f26b77e3e59d3d651c50f91` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/pc-doc/src/pattern.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `crates/doc/src/pattern.rs` | `fd21e734111ef0af3f26b77e3e59d3d651c50f91` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/pc-engine/src/fill_cmds.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `crates/engine/src/fill_cmds.rs` | `fd21e734111ef0af3f26b77e3e59d3d651c50f91` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/pc-engine/src/paint_cmds.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `crates/engine/src/paint_cmds.rs` | `fd21e734111ef0af3f26b77e3e59d3d651c50f91` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/pc-engine/src/pattern_cmds.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `crates/engine/src/pattern_cmds.rs` | `fd21e734111ef0af3f26b77e3e59d3d651c50f91` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/pc-gpu/src/fx.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `crates/gpu/src/fx.rs` | `fd21e734111ef0af3f26b77e3e59d3d651c50f91` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/pc-gpu/src/plan.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `crates/gpu/src/plan.rs` | `fd21e734111ef0af3f26b77e3e59d3d651c50f91` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/pc-ui-egui/src/dialog_filter.rs` | [LightCraft](https://github.com/storytold/lightcraft) | `apps/lightcraft/src/dialog_filter.rs` | `e2d35de867319d3b36704a3ebf7337be2f900673` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/pc-ui-egui/src/dialog_filter.rs` | [LightCraft](https://github.com/storytold/lightcraft) | `apps/lightcraft/src/main.rs` | `9c2bad114662634deb6aedd4fce4680066b66349` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/pc-engine/src/commands.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `crates/engine/src/document_preset_cmds.rs` | `6e9ba6511d507dafc7809927f153d818f592f8a8` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/pc-engine/src/tests.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `crates/engine/src/document_preset_cmds.rs` | `6e9ba6511d507dafc7809927f153d818f592f8a8` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/pc-ui-egui/src/puppet_ui.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `crates/ui-egui/src/puppet_ui.rs` | `64db46f5abab516ed01ccd2efff009049274e450` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/pc-ui-egui/src/puppet_ui.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `crates/algo/src/puppet.rs` | `3f7d8afc7886b4732a48667959d9f291e5928ac4` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/pc-text/src/tests.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `crates/text/src/tests.rs` | `6f1c4481bfe158eccd5ed9b0bae344145f74e165` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/pc-text/tests/coverage/ds_psd_coverage.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `crates/text/src/tests.rs` | `6f1c4481bfe158eccd5ed9b0bae344145f74e165` | MIT OR Apache-2.0 | 2026-10-13 |
| `crates/pc-io/tests/it/psd_type_bounds.rs` | [PhotoCraft](https://github.com/storytold/photocraft) | `crates/io/tests/psd_type_bounds.rs` | `6f1c4481bfe158eccd5ed9b0bae344145f74e165` | MIT OR Apache-2.0 | 2026-10-13 |

## Unmodified RAW engine dependency

This row records delegation and licence provenance, **not copied or patched decoder source**.
All glue is in one adapter; rawler remains an ordinary crates.io dependency.

| Our file | Upstream project | Upstream path | Upstream commit | Licence | Date | Used for |
|---|---|---|---|---|---|---|
| `crates/lc-raw/src/rawler_adapter.rs` | [rawler](https://github.com/dnglab/dnglab) | `rawler/Cargo.toml` | `ae01bcb2d0f8a74f9dfbb9f7b5c7e315c8e668b7` | LGPL-2.1-only | 2026-10-13 | Unmodified crates.io 0.8.0 dependency for Canon CR3 RAW/C-RAW/dual-pixel, compressed Fujifilm RAF and Olympus/OM ORF; fallback for any native Unsupported variant, and runtime camera calibration missing from our first-choice table. |

## Whole camera calibration data (2026-10-13)

| Our file | Upstream project | Upstream path | Upstream commit | Licence | Used for |
|---|---|---|---|---|---|
| `crates/lc-raw/data/cameras/camconst.json` | [RawTherapee](https://github.com/RawTherapee/RawTherapee) | `rtengine/camconst.json` | `5f486d3678b34c74ba0c63571c17babe20935019` | GPL-3.0-or-later | Whole camera white/black levels, crops and PDAF patterns |
| `crates/lc-raw/data/cameras/cameras.xml` | [rawspeed](https://github.com/darktable-org/rawspeed) | `data/cameras.xml` | `0c16fee7772594e98897bded01fc31bc3409b4cb` | CC-BY-SA-3.0 | Whole ISO sensor calibration, crops, black areas and matrices |
| `crates/lc-raw/data/cameras/noiseprofiles.json` | [darktable](https://github.com/darktable-org/darktable) | `data/noiseprofiles.json` | `733bd69f32cac7ff5e41025115942772add1f088` | GPL-3.0-or-later | Whole per-camera measured noise fits |
| `crates/lc-raw/data/cameras/wb_presets.json` | [darktable](https://github.com/darktable-org/darktable) | `data/wb_presets.json` | `733bd69f32cac7ff5e41025115942772add1f088` | GPL-2.0-or-later | Whole camera WB presets |


Files remain whole and byte-identical to upstream. [Camera-data contract](CAMERA-DATA.md) defines
precedence, mode guards, refresh steps and validation. `camera_data.rs` follows RT `camconst.cc` and
`pdaflinesfilter.cc`, rawspeed `Camera.cpp` and darktable `noiseprofiles.c`; the runtime and algorithms
remain Rust. Notices are in `licenses/{rawtherapee,darktable,rawspeed}-NOTICE.md`.

| Our file | Upstream project | Upstream path | Upstream commit | Licence | Used for |
|---|---|---|---|---|---|
| `crates/lc-raw/src/camera_data.rs` | [RawTherapee](https://github.com/RawTherapee/RawTherapee) | `rtengine/camconst.cc` | `5f486d3678b34c74ba0c63571c17babe20935019` | GPL-3.0-or-later | ISO/aperture camera constant semantics |
| `crates/lc-raw/src/camera_data.rs` | [RawTherapee](https://github.com/RawTherapee/RawTherapee) | `rtengine/rawimage.cc` | `5f486d3678b34c74ba0c63571c17babe20935019` | GPL-3.0-or-later | Integer TIFF-bit-depth white mapping and absolute black flags |
| `crates/lc-raw/src/camera_data.rs` | [RawTherapee](https://github.com/RawTherapee/RawTherapee) | `rtengine/pdaflinesfilter.cc` | `5f486d3678b34c74ba0c63571c17babe20935019` | GPL-3.0-or-later | PDAF periods and green-triplet detection; host clean-row interpolation |
| `crates/lc-raw/src/camera_data.rs` | [darktable](https://github.com/darktable-org/darktable) | `src/common/noiseprofiles.c` | `733bd69f32cac7ff5e41025115942772add1f088` | GPL-3.0-or-later | Profile skip/interpolation semantics |
### Demosaic extension (2026-10-13)
Markesteijn three-pass uses upstream YPbPr homogeneity voting and eight directional buffers.
DCB uses three correction iterations with standard final chroma interpolation. LMMSE uses three
median passes, without EECI refinement (upstream mode 2). These are safe Rust ports, with no
C/C++ linkage, new dependencies, or modified rawler source. rawler 0.8.0 is a second reference
in the offline quality example only.
Update procedure: change the pin in `scripts/demosaic_refvec.py`, download whole upstream source
files into `target/refvec/demosaic/upstream`, inspect algorithm changes, rerun the scalar harness
and measured noisy re-mosaic curves, then update ports, licence notices and reviewed fixtures.
Do not regenerate existing fixtures without investigating differences. Reference exports are
pinned to the installed darktable 5.6.2 and retained under `target/`; they are not runtime data.
The reference-only Poisson-Gaussian harness reads darktable's whole, unmodified
`data/noiseprofiles.json` at the same pin (GPL-3.0-or-later, SHA-256
`e22730cf69ead3534b2d2e7500ab24af6da99834250b852ca1831002ba2450f8`).
`scripts/demosaic_profiles.py` retains and verifies that JSON under `target/refvec/demosaic/`
and extracts eight D750 ISO entries into a reference TSV. It is not runtime camera data.
Updating it requires reviewing the source pin/hash and rerunning the profile curves at a
coordinator-approved benchmark milestone. The ten-file CC0 camera corpus is pinned separately
in `crates/lc-raw/tests/fixtures/demosaic-corpus.tsv`; `scripts/demosaic_samples.py` verifies every
source SHA-256 and stores files under `target/demosaic-corpus/`.
## User camera/lens profile imports (2026-10-14)
Adobe DCP/LCP data is **never bundled or redistributed**. Runtime import reads the user's own
DNG Converter/Camera Raw install or chosen folder; original files remain whole and unchanged.
Keep the user's installer/version and originals for reproducibility, replace with a new licensed
install to update, then use Settings → Import → Reload Profiles. Content-based ids invalidate
changed selections/caches. See [CAMERA-PROFILES.md](CAMERA-PROFILES.md) for locations and custom
GPL-3 dcamprof profiles; no dcamprof/Adobe SDK/C reference dependency enters Cargo.lock.
Triple-illuminant matrices and hue/saturation weighting follow DNG 1.6/1.7 chapter 6 and
[Adobe SDK](https://android.googlesource.com/platform/external/dng_sdk/+/de700ad461e35af50b28b861943a0b0753b10929/source/dng_color_spec.cpp)
reference algorithms at `de700ad461e35af50b28b861943a0b0753b10929`. SDK numeric reference
vectors are produced externally under target/refvec; no SDK source is compiled in the app.
CIE data update instructions are [with the whole pinned data](../crates/lc-raw/data/color/README.md).

## PhotoCraft upstream port (2026-10-17)

Hand ports from [PhotoCraft](https://github.com/storytold/photocraft) `72b499a2..efa1edd0`, triaged MUST/SHOULD. One row per upstream commit with the files it changed here.

| Upstream commit | Subject | Our files | Licence |
|---|---|---|---|
| [`bd3b5d9192e6`](https://github.com/storytold/photocraft/commit/bd3b5d9192e6a0a5fa70d3f780a8abe19679328d) | fix(automation): refuse a group/world-readable control-token file (#4966) (#5459) | `crates/pc-automation/src/security.rs` | MIT OR Apache-2.0 |
| [`64520f645e07`](https://github.com/storytold/photocraft/commit/64520f645e073b23960b774a677d54f504094843) | Control channel: connections must authenticate within 10 s, however active (#4965) (#5229) | `apps/local-image/src/control_server.rs`<br>`crates/pc-automation/src/rpc.rs`<br>`crates/pc-automation/src/security.rs` | MIT OR Apache-2.0 |
| [`b630d7cb9aeb`](https://github.com/storytold/photocraft/commit/b630d7cb9aeb2a79ab5224ebf6e8266f1d858873) | Security: enforce the automation read boundary on stored smart-filter params (#5684) | `crates/pc-automation/tests/it/main.rs`<br>`crates/pc-automation/tests/it/smart_filter_authorization.rs`<br>`crates/pc-engine/src/filters.rs`<br>`crates/pc-engine/src/jobs.rs`<br>`crates/pc-engine/src/lens_cmds.rs`<br>`crates/pc-engine/src/lib.rs`<br>`crates/pc-engine/src/smart_cmds.rs` | MIT OR Apache-2.0 |
| [`57437ea1a982`](https://github.com/storytold/photocraft/commit/57437ea1a9828298b46673bda891f2a520de92fd) | fix(ui-egui): canvas textures fit the device's texture limit; GPU tests hold the GPU lock on the app's device (#5954) | `crates/pc-ui-egui/src/canvas.rs`<br>`crates/pc-ui-egui/src/canvas/display.rs`<br>`crates/pc-ui-egui/src/canvas/tests.rs` | MIT OR Apache-2.0 |
| [`33728243fb47`](https://github.com/storytold/photocraft/commit/33728243fb4738d8e3363343bf00f52fa3d5083b) | perf(gpu): closing a document frees what the compositor holds for it (#4529) (#4772) | `crates/pc-gpu/src/lib.rs`<br>`crates/pc-gpu/tests/gpu/parity.rs`<br>`crates/pc-ui-egui/src/gpu_canvas.rs` | MIT OR Apache-2.0 |
| [`94fdd279e801`](https://github.com/storytold/photocraft/commit/94fdd279e801e996de86cbcc99b8befdcc771087) | fix(puppet-warp): drop open Puppet Warp and other distort sessions when the active document changes (#5092) (#5123) | `crates/pc-ui-egui/src/canvas.rs`<br>`crates/pc-ui-egui/src/distort_ui.rs`<br>`crates/pc-ui-egui/src/puppet_ui.rs` | MIT OR Apache-2.0 |
| [`cd35577c45ee`](https://github.com/storytold/photocraft/commit/cd35577c45ee943e6b862f0227a7317b1745c0bf) | Polygonal Lasso: drop the polygon in progress when another document becomes active (#5015) | `crates/pc-ui-egui/src/canvas/tools.rs`<br>`crates/pc-ui-egui/src/lib.rs`<br>`crates/pc-ui-egui/src/polygon_lasso_tests.rs`<br>`crates/pc-ui-egui/src/state.rs` | MIT OR Apache-2.0 |
| [`9b59a8ca6e0b`](https://github.com/storytold/photocraft/commit/9b59a8ca6e0b687450392b290f53b26b23fa878c) | fix(pen): keep unfinished paths in their original document (#5460) | `crates/pc-ui-egui/src/direct_select_tests.rs`<br>`crates/pc-ui-egui/src/lib.rs`<br>`crates/pc-ui-egui/src/vector_ui.rs` | MIT OR Apache-2.0 |
| [`56d3b7f983af`](https://github.com/storytold/photocraft/commit/56d3b7f983afa2d85a53b1f1e4fc8cc2436faefa) | Pen and Path Selection: Delete deletes the finished work path (#5702) (#5732) | `crates/pc-ui-egui/src/shortcuts.rs`<br>`crates/pc-ui-egui/src/vector_ui.rs` | MIT OR Apache-2.0 |
| [`cb95e17fc1c0`](https://github.com/storytold/photocraft/commit/cb95e17fc1c071debd043c42e7512b83b462edf6) | fix(type): preserve keyboard focus when searching for fonts (#4745) | `crates/pc-ui-egui/src/canvas/tools.rs`<br>`crates/pc-ui-egui/src/lib.rs`<br>`crates/pc-ui-egui/src/state.rs`<br>`crates/pc-ui-egui/src/type_tool.rs`<br>`crates/pc-ui-egui/src/type_tool_tests.rs` | MIT OR Apache-2.0 |
| [`aecac608b46a`](https://github.com/storytold/photocraft/commit/aecac608b46a46454033cdac3721e3b8fe5299af) | fix(ui): Type tool caret stays solid after typing, navigation or a click, then resumes blinking (#5511) (#5512) | `crates/pc-ui-egui/src/canvas/tools.rs`<br>`crates/pc-ui-egui/src/lib.rs`<br>`crates/pc-ui-egui/src/state.rs`<br>`crates/pc-ui-egui/src/type_tool.rs`<br>`crates/pc-ui-egui/src/type_tool_tests.rs` | MIT OR Apache-2.0 |
| [`19227022ca5a`](https://github.com/storytold/photocraft/commit/19227022ca5a3f908c4412a8acde5ef806825433) | Histogram: scale bar heights linearly to the tallest bin, not sorted.get(250) (#6638) | `crates/pc-ui-egui/src/tone.rs` | MIT OR Apache-2.0 |
| [`08d9c1d67dbb`](https://github.com/storytold/photocraft/commit/08d9c1d67dbb49283b94ffe81c2a4dbb824bdce1) | fix(transform): keep masked objects visible during transform (#5239) (#5319) | `crates/pc-ui-egui/src/transform_tool.rs` | MIT OR Apache-2.0 |
| [`49f878d91b51`](https://github.com/storytold/photocraft/commit/49f878d91b51867fc506fd3e9c8ba5d53f416f34) | Scaling around reference point no longer snaps center of image to reference point (#5510) | `crates/pc-ui-egui/src/transform_tool.rs` | MIT OR Apache-2.0 |
| [`f79f21931ec7`](https://github.com/storytold/photocraft/commit/f79f21931ec7e830060a2b1f5bd3906dd83cac85) | fix(ui): smart filter adjustments open their own editor on double-click (#5812) | `crates/pc-ui-egui/src/smart_ui.rs` | MIT OR Apache-2.0 |
| [`c72dda19142b`](https://github.com/storytold/photocraft/commit/c72dda19142bc3187c3d6905e675574632d81026) | Assign Profile: list only profiles of the document's colour mode (#4951) (#5020) | `crates/pc-ui-egui/src/filter_dialog.rs` | MIT OR Apache-2.0 |
| [`e4b2868a02c9`](https://github.com/storytold/photocraft/commit/e4b2868a02c99afb30ed5c61022e03b13574e873) | Check deep EXR tile count tables against max_alloc (#5632) | `crates/pc-codecs/src/codecs/deep_exr.rs`<br>`crates/pc-codecs/tests/it/deep_exr.rs` | MIT OR Apache-2.0 |
| [`3888e95e477f`](https://github.com/storytold/photocraft/commit/3888e95e477f5f1e62581d8360057267b4dae9b9) | Text: non-finite numbers and negative indices from PSD files are sanitized (#4429) (#4433) | `crates/pc-text/src/engine_data.rs`<br>`crates/pc-text/src/layout.rs`<br>`crates/pc-text/src/lib.rs`<br>`crates/pc-text/src/psd.rs`<br>`crates/pc-text/src/raster.rs` | MIT OR Apache-2.0 |
| [`3d2914368d38`](https://github.com/storytold/photocraft/commit/3d2914368d38aa8e5c336415241ceadc88f5ab00) | PSD type: clamp warp bend and distortion before the f32 cast (#6856) (#6857) | `crates/pc-text/src/psd.rs` | MIT OR Apache-2.0 |
| [`339397aabb1e`](https://github.com/storytold/photocraft/commit/339397aabb1ecb746bb0021a7bf6c0eb943dc23f) | Reject negative PSD style-sheet indexes and a non-finite dpi. (#4631) | `crates/pc-text/src/psd.rs`<br>`crates/pc-text/src/psd_styles.rs` | MIT OR Apache-2.0 |
| [`1bcb662828b4`](https://github.com/storytold/photocraft/commit/1bcb662828b4addfe721191b849ffe3bd1c4bde0) | Deep EXR: open cropped renders on their full display-window stage (#4907) | `crates/pc-codecs/src/codecs/deep_exr.rs`<br>`crates/pc-codecs/src/image.rs`<br>`crates/pc-codecs/tests/it/deep_exr.rs` | MIT OR Apache-2.0 |
| [`7b0dbf182bfc`](https://github.com/storytold/photocraft/commit/7b0dbf182bfc92089ade61b2d2814b89f03fa2dd) | PNG: a whole-ppi pHYs reads back whole (300 ppi stays 300) (#4857) | `crates/pc-codecs/src/codecs/png.rs`<br>`crates/pc-codecs/tests/it/metadata.rs` | MIT OR Apache-2.0 |
| [`f1c45b887ee9`](https://github.com/storytold/photocraft/commit/f1c45b887ee9fedbcd00613c8fdb804f32eb75ec) | fix(codecs): respect PAM ENDHDR line boundaries (#5874) | `crates/pc-codecs/src/codecs/pnm.rs`<br>`crates/pc-codecs/tests/it/main.rs`<br>`crates/pc-codecs/tests/it/pnm_headers.rs` | MIT OR Apache-2.0 |
| [`88ed837a86e5`](https://github.com/storytold/photocraft/commit/88ed837a86e5c8489c504f0c3aefb5756abece26) | Save for Web: JPEG chroma is full (4:4:4) above quality 50, as in Photoshop (#4823) | `crates/pc-codecs/src/web.rs` | MIT OR Apache-2.0 |
| [`ad6463b50f68`](https://github.com/storytold/photocraft/commit/ad6463b50f6844001c7927a0d36fbf6acfae783b) | Paragraph text: a word wider than the box breaks at the box edge (#6694) (#6696) | `crates/pc-text/src/layout.rs`<br>`crates/pc-text/src/tests.rs` | MIT OR Apache-2.0 |
| [`a1e19a0b878c`](https://github.com/storytold/photocraft/commit/a1e19a0b878c1bb4bc0609e1bd7db56f3688512c) | fix(text): break paragraph lines at the horizontally scaled width (#5611) | `crates/pc-text/src/layout.rs`<br>`crates/pc-text/src/tests.rs` | MIT OR Apache-2.0 |
| [`8a4ed48354fb`](https://github.com/storytold/photocraft/commit/8a4ed48354fb893034938e299b9ec61db20d09f9) | fix(ui): numeric fields drop oversized text, paste and IME events before layout (part of #2150) (#2258) | `crates/pc-ui-egui/src/new_doc_ui.rs`<br>`crates/pc-ui-egui/src/widgets.rs`<br>`crates/pc-ui-egui/src/widgets/numeric_input_tests.rs` | MIT OR Apache-2.0 |
| [`8d8db22c94f8`](https://github.com/storytold/photocraft/commit/8d8db22c94f8f9058e62fb283c136049c91bfd4d) | Type: a scaled type layer keeps its size when the Character fields show it (#5717) (#5718) | `crates/pc-ui-egui/src/new_doc_ui.rs`<br>`crates/pc-ui-egui/src/widgets.rs`<br>`crates/pc-ui-egui/src/widgets/numeric_input_tests.rs` | MIT OR Apache-2.0 |
| [`1025914b0e3e`](https://github.com/storytold/photocraft/commit/1025914b0e3eca1d5551b96418f4fd6ce7bb108e) | Properties / New Document: tiny sizes in physical units no longer resize the document (#5661) (#5662) | `crates/pc-ui-egui/src/new_doc_ui.rs`<br>`crates/pc-ui-egui/src/widgets.rs`<br>`crates/pc-ui-egui/src/widgets/numeric_input_tests.rs` | MIT OR Apache-2.0 |
| [`f053a924a668`](https://github.com/storytold/photocraft/commit/f053a924a668ec1b43d9bafcb473930cf0c65824) | fix(ui): Color Picker hex field accepts repeated pastes (#5398) | `crates/pc-ui-egui/src/color_picker_ui.rs` | MIT OR Apache-2.0 |
| [`6a7fc88c79d8`](https://github.com/storytold/photocraft/commit/6a7fc88c79d81a8d7c6c6c1ec8c06326fb43b3e8) | Crop tool: custom ratios keep three decimals, 1.414 : 1 no longer becomes 1.4 : 1 (fixes #6487) (#6503) | `crates/pc-ui-egui/src/panels/options_bar.rs`<br>`crates/pc-ui-egui/src/panels/tests.rs`<br>`crates/pc-ui-egui/src/widgets.rs` | MIT OR Apache-2.0 |
| [`54f4baf6f96f`](https://github.com/storytold/photocraft/commit/54f4baf6f96fd42a5ad0873fc162b862cc7b7a4f) | Layers panel: blend mode, Opacity, Fill and locks change every selected layer; Photoshop's History names (#5856) | `crates/pc-engine/src/commands.rs`<br>`crates/pc-engine/tests/it/layer_props_several.rs`<br>`crates/pc-engine/tests/it/main.rs`<br>`crates/pc-ui-egui/src/panels/docks.rs`<br>`crates/pc-ui-egui/src/panels/layers.rs`<br>`crates/pc-ui-egui/src/panels/tests.rs` | MIT OR Apache-2.0 |
| [`a0627311ab35`](https://github.com/storytold/photocraft/commit/a0627311ab350ad151ee8986c44f5dc7a92233ce) | Fix window dragging with modal dialogs (#4913) | `crates/pc-ui-egui/src/dialogs.rs`<br>`crates/pc-ui-egui/src/lib.rs`<br>`crates/pc-ui-egui/src/panels/title.rs`<br>`crates/pc-ui-egui/src/titlebar.rs` | MIT OR Apache-2.0 |
| [`3887d8f738d2`](https://github.com/storytold/photocraft/commit/3887d8f738d21b6b4e431d50bcba2c9a59f9004f) | Title bar: dialogs no longer stop the window dragging on macOS (#5826) (#5860) | `crates/pc-ui-egui/src/dialogs.rs`<br>`crates/pc-ui-egui/src/lib.rs`<br>`crates/pc-ui-egui/src/panels/title.rs`<br>`crates/pc-ui-egui/src/titlebar.rs` | MIT OR Apache-2.0 |
| [`a7f7f1fd1e32`](https://github.com/storytold/photocraft/commit/a7f7f1fd1e327fcddb2697f0f03236fc3db2c2cd) | Edit › Menus: Menus, Keyboard Shortcuts, Preferences, Save and Exit can't be hidden (#5096) (#5183) | `crates/pc-engine/src/prefs.rs`<br>`crates/pc-engine/src/prefs/tests.rs`<br>`crates/pc-ui-egui/src/menus.rs`<br>`crates/pc-ui-egui/src/prefs_ui/shortcut_dialog.rs`<br>`crates/pc-ui-egui/src/prefs_ui/tests.rs` | MIT OR Apache-2.0 |
| [`2e242bde67c2`](https://github.com/storytold/photocraft/commit/2e242bde67c202e4cd16bf3d6f88088b521ab423) | Fix Export As freeze on large layered PSDs (#5104) | `crates/pc-ui-egui/src/export_dialog.rs` | MIT OR Apache-2.0 |
| [`33d9cb6ac479`](https://github.com/storytold/photocraft/commit/33d9cb6ac479158bd757b9ab1e36e51fbb8adade) | UI fonts: draw a variable CJK fallback at Regular, not Thin (#4820) (#4830) | `crates/pc-ui-egui/src/cjk_fonts.rs` | MIT OR Apache-2.0 |
| [`db46713c2ca0`](https://github.com/storytold/photocraft/commit/db46713c2ca0140e119956ed695cf92837fc50e3) | Unsaved-changes prompt: the default answer takes keyboard focus, so arrows and Enter work (part of #6441) (#6466) | `crates/pc-ui-egui/src/discard_ui.rs`<br>`crates/pc-ui-egui/src/widgets.rs` | MIT OR Apache-2.0 |
| [`0df333f62318`](https://github.com/storytold/photocraft/commit/0df333f6231816f7b892b45cfab433019dbb0c1d) | fix(gpu): require consent to persist CPU recovery (#3539) | `apps/local-image/src/gpu_startup.rs`<br>`apps/local-image/src/main.rs` | MIT OR Apache-2.0 |
| [`fec0573e718b`](https://github.com/storytold/photocraft/commit/fec0573e718bcd023eb1b2e7ad19801bc0d687fa) | fix(window): reopen at the saved window position instead of re-centring (#4826) (#4850) | `apps/local-image/src/main.rs`<br>`apps/local-image/src/ui_state.rs` | MIT OR Apache-2.0 |
| [`8e2c5ddb40ff`](https://github.com/storytold/photocraft/commit/8e2c5ddb40ffe9780f55afbe75996a24d52eb33c) | macOS: a bundled app keeps its own Dock icon (#5849) (#5879) | `apps/local-image/src/app_icon.rs`<br>`apps/local-image/src/main.rs` | MIT OR Apache-2.0 |
| [`5c78d0848ff0`](https://github.com/storytold/photocraft/commit/5c78d0848ff0094929f7a22da25a5f749eb7727d) | fix: limit Render Video timeline and GIF staging allocations (#5670) | `crates/pc-engine/src/timeline_cmds.rs`<br>`crates/pc-engine/src/video_cmds.rs`<br>`crates/pc-engine/src/video_cmds/tests.rs` | MIT OR Apache-2.0 |
| [`7c916d9ecdd8`](https://github.com/storytold/photocraft/commit/7c916d9ecdd8a36c1e1f6d5ac9691fe604cd2bd7) | Validate selection modification limits before allocating the canvas mask. (#4591) | `crates/pc-engine/src/selection_cmds.rs` | MIT OR Apache-2.0 |
| [`759780e40d22`](https://github.com/storytold/photocraft/commit/759780e40d224406d7f4294ae70da06bf422f70c) | Clip Fill source reads to the canvas before allocating pixels. (#4590) | `crates/pc-engine/src/fill_cmds.rs` | MIT OR Apache-2.0 |
| [`28ed1c114789`](https://github.com/storytold/photocraft/commit/28ed1c1147894a639b472dffcc0ae5f1e7067085) | Handle allocation failures in large pixel operations (#2530) | `crates/pc-engine/src/allocation.rs`<br>`crates/pc-engine/src/fill_cmds.rs`<br>`crates/pc-engine/src/lib.rs`<br>`crates/pc-raster/src/lib.rs` | MIT OR Apache-2.0 |
| [`f769c3735dc7`](https://github.com/storytold/photocraft/commit/f769c3735dc797416200946dd40d5e3560fd9d5d) | perf(engine): Fill, Stroke and Clear keep tiles the selection doesn't reach (#4517) (#4677) | `crates/pc-engine/src/fill_cmds.rs` | MIT OR Apache-2.0 |
| [`c4bf9eb04526`](https://github.com/storytold/photocraft/commit/c4bf9eb04526f7c261a1834d24639ee7166d7c48) | Bound selection sampling buffers and report allocation failures. (#4594) | `crates/pc-engine/src/selection_cmds.rs` | MIT OR Apache-2.0 |
| [`dd69f0797dd2`](https://github.com/storytold/photocraft/commit/dd69f0797dd2c0dc82a206f5e7eddd21b56ff048) | fix(engine): reserve the define-brush tip and selection-mask buffers fallibly (#212) (#5469) | `crates/pc-engine/src/brush_cmds.rs`<br>`crates/pc-engine/src/brush_cmds/tests.rs` | MIT OR Apache-2.0 |
| [`b52115fc0ae7`](https://github.com/storytold/photocraft/commit/b52115fc0ae7601d203aafff0bea230e81b573eb) | Preserve document and history when Fade cannot allocate its buffers. (#4593) | `crates/pc-engine/src/edit_menu_cmds.rs`<br>`crates/pc-engine/src/edit_menu_cmds/tests.rs` | MIT OR Apache-2.0 |
| [`9cc8be43929b`](https://github.com/storytold/photocraft/commit/9cc8be43929b70d9f352b14b293b2ba2089429b2) | fix(algo): keep clipped warp bounds from overflowing (#5451) | `crates/pc-algo/src/transform.rs`<br>`crates/pc-algo/src/warp.rs` | MIT OR Apache-2.0 |
| [`94b1f0a4cb71`](https://github.com/storytold/photocraft/commit/94b1f0a4cb71e3f9b5a59db281bf19e9e0b67be1) | Warp: a thin layer stretched and bent keeps its curve on commit (#5793) (#5810) | `crates/pc-algo/src/warp.rs` | MIT OR Apache-2.0 |
| [`42b399881a7e`](https://github.com/storytold/photocraft/commit/42b399881a7e428055da72e15a6b9f337e137afa) | Handle signed coordinate limits in the Offset filter (#4738) | `crates/pc-algo/src/other.rs` | MIT OR Apache-2.0 |
| [`47cf5af9fd93`](https://github.com/storytold/photocraft/commit/47cf5af9fd935da2180d6afb383005d2432659b2) | perf: keep effect-cache pins and History snapshots inside the memory budgets (#5336) | `crates/pc-compose/src/lib.rs`<br>`crates/pc-compose/src/tests.rs`<br>`crates/pc-engine/src/analysis_cmds.rs`<br>`crates/pc-engine/src/edit_menu_cmds.rs`<br>`crates/pc-engine/src/history_cmds.rs`<br>`crates/pc-engine/src/lib.rs`<br>`crates/pc-engine/src/prefs.rs`<br>`crates/pc-engine/src/vector_cmds.rs`<br>+4 more | MIT OR Apache-2.0 |
| [`2807deb20971`](https://github.com/storytold/photocraft/commit/2807deb20971cf84f52bf7397fbafafa89ac2d26) | fix(psd): write the layer block after a kept merged-transparency marker (#5673) | `crates/pc-io/src/psd_export.rs` | MIT OR Apache-2.0 |
| [`3260f6abb974`](https://github.com/storytold/photocraft/commit/3260f6abb974660bf93af8c14f1c731d29d73cde) | fix(psd): prioritize 16-bit and 32-bit global layer blocks over compatibility section (#4928) (#4950) | `crates/pc-io/src/psd_export.rs`<br>`crates/pc-io/src/psd_import.rs`<br>`crates/pc-psd/src/file.rs` | MIT OR Apache-2.0 |
| [`ff57499774f6`](https://github.com/storytold/photocraft/commit/ff57499774f65a733413b45a645516476741c43b) | PSD: Mt16/Mt32 blocks mark the merged image's transparency (#5178) | `crates/pc-io/tests/it/psd32_export.rs`<br>`crates/pc-psd/src/file.rs` | MIT OR Apache-2.0 |
| [`78c611007be0`](https://github.com/storytold/photocraft/commit/78c611007be069535d76813b339461bc2d75114e) | PSD: alpha channel resources count the merged transparency, 13-byte DisplayInfo (#3577) | `crates/pc-compose/src/multichannel.rs`<br>`crates/pc-io/src/channel_map.rs`<br>`crates/pc-io/src/psd_export.rs`<br>`crates/pc-io/src/psd_import.rs` | MIT OR Apache-2.0 |
| [`6547ca0b8459`](https://github.com/storytold/photocraft/commit/6547ca0b8459ef37e1dfb373cc00a81740ed74a4) | fix(psd): Multichannel alpha channels stay alpha; a save keeps resources and global blocks (#4031) | `crates/pc-compose/src/multichannel.rs`<br>`crates/pc-io/src/channel_map.rs`<br>`crates/pc-io/src/psd_export.rs`<br>`crates/pc-io/src/psd_import.rs` | MIT OR Apache-2.0 |
| [`3b45e8baee71`](https://github.com/storytold/photocraft/commit/3b45e8baee7117e53847523b5bc3ae2b337966d4) | PSD import: flat fallback respects the decode budget; checked sizes (#5200, #5202) (#5206) | `crates/pc-io/src/psd_import.rs` | MIT OR Apache-2.0 |
| [`a7301f1db89b`](https://github.com/storytold/photocraft/commit/a7301f1db89b2f24b0fb6d1334b424da898faf0b) | PSD: cap pattern memory across a parse, reject mixed-depth pattern channels (#5198) (#5207) | `crates/pc-io/src/pattern_map.rs`<br>`crates/pc-psd/src/abr.rs`<br>`crates/pc-psd/src/patterns.rs`<br>`crates/pc-psd/src/slices.rs` | MIT OR Apache-2.0 |
| [`481b939fdf68`](https://github.com/storytold/photocraft/commit/481b939fdf6841e349f71e941aa144e6e53b6dbf) | PSD save: keep the Lr16/Lr32 padding outside the block length (part of #4654) (#4663) | `crates/pc-io/src/psd_export.rs`<br>`crates/pc-io/tests/it/common/mod.rs`<br>`crates/pc-io/tests/it/psd_block_alignment.rs` | MIT OR Apache-2.0 |
| [`ccb33741f9dc`](https://github.com/storytold/photocraft/commit/ccb33741f9dc51ac070aa9fe661f6f7cd4a66352) | fix(psd): reserve imported layer IDs when allocating section dividers (#4885) | `crates/pc-io/src/psd_export.rs` | MIT OR Apache-2.0 |
| [`a95419427712`](https://github.com/storytold/photocraft/commit/a954194277121a1673cfd579d0182929bb21a705) | PSD import: ResolutionInfo stays pixels per inch when the display unit is cm (#5280) | `crates/pc-io/src/psd_import.rs`<br>`crates/pc-io/tests/it/psd_structure.rs`<br>`crates/pc-psd/src/resources.rs` | MIT OR Apache-2.0 |
| [`41e578b4d341`](https://github.com/storytold/photocraft/commit/41e578b4d341496ea12b886b6dcb35b34e3d636c) | fix(io): a version 3 Photo Filter colour is XYZ relative to the D50 white (#4987) | `crates/pc-io/src/adjust_map.rs` | MIT OR Apache-2.0 |
| [`87aed12d6243`](https://github.com/storytold/photocraft/commit/87aed12d6243818f2e05aff65c106b5a009d9b93) | Brush import: Dual Brush spacing comes from its tip (#4809) (#4889) | `crates/pc-io/src/abr_map.rs`<br>`crates/pc-io/src/abr_map/tests.rs` | MIT OR Apache-2.0 |
| [`b8ae1cd92fcc`](https://github.com/storytold/photocraft/commit/b8ae1cd92fccacfc5b30566e64c22bdebe0a75eb) | fix(brush): preserve Dual Brush tip spacing and scale its size with the primary brush (#4856) | `crates/pc-io/src/abr_map.rs`<br>`crates/pc-io/src/abr_map/tests.rs` | MIT OR Apache-2.0 |
| [`57624bd02eae`](https://github.com/storytold/photocraft/commit/57624bd02eaed7e3a510b8cd7c7d0b5cc2909193) | PSD: Backgrounds survive a round trip through PhotoCraft (#3560) | `crates/pc-io/src/psd_export.rs`<br>`crates/pc-io/src/psd_import.rs` | MIT OR Apache-2.0 |
| [`ed7df56a2455`](https://github.com/storytold/photocraft/commit/ed7df56a2455375240959bae6e5273a71d858d84) | fix(format): recover autosaves with missing sidecars | `apps/local-image/src/services.rs`<br>`crates/pc-format/src/autosave.rs` | MIT OR Apache-2.0 |
| [`5cff9ebed2f2`](https://github.com/storytold/photocraft/commit/5cff9ebed2f200cf0b0fdb7be7f88dce87d6c74e) | fix(clipboard): prompt to save new documents containing pasted pixels (#4683) | `crates/pc-engine/src/edit_cmds.rs` | MIT OR Apache-2.0 |
| [`c61cf9f0bd66`](https://github.com/storytold/photocraft/commit/c61cf9f0bd668ded8308e3c802d5439e7d62e02f) | fix(engine): load each persisted preset set independently (#5616) | `crates/pc-engine/src/presets/mod.rs`<br>`crates/pc-engine/src/presets/tests.rs` | MIT OR Apache-2.0 |
| [`4fdf1a723ba8`](https://github.com/storytold/photocraft/commit/4fdf1a723ba89bbf075ecd6dd87f496be264dc97) | fix(batch): skip recorded Quick Export steps when writing batch output (#4747) | `crates/pc-engine/src/file_cmds.rs`<br>`crates/pc-engine/src/file_cmds/tests.rs` | MIT OR Apache-2.0 |
| [`f360cb0b4a9c`](https://github.com/storytold/photocraft/commit/f360cb0b4a9c31085d3dcbe7877b64c8de5fed62) | fix(engine): give artboard exports unique indexed filenames (#6655) | `crates/pc-engine/src/artboard_cmds.rs`<br>`crates/pc-engine/src/artboard_cmds/tests.rs` | MIT OR Apache-2.0 |
| [`33b360d5ee2a`](https://github.com/storytold/photocraft/commit/33b360d5ee2a45e22053265239f52875ce3110ce) | Reject clipboard origins that cannot form a new document (#4586) | `crates/pc-engine/src/edit_cmds.rs` | MIT OR Apache-2.0 |
| [`57d488032ce5`](https://github.com/storytold/photocraft/commit/57d488032ce5acc71bb816c6a31b948845626fc7) | fix(doc): variable definitions follow layer id remapping on copy and load (#5023) | `crates/pc-engine/src/image_cmds.rs`<br>`crates/pc-format/src/convert.rs`<br>`crates/pc-format/tests/it/roundtrip.rs` | MIT OR Apache-2.0 |
| [`663863e62bcd`](https://github.com/storytold/photocraft/commit/663863e62bcdd0a7c157b0c2d4b8a9904db3469e) | Smart Objects > Export Contents: another extension exports the contents in that format (#4847) | `crates/pc-engine/src/smart_cmds.rs`<br>`crates/pc-engine/src/smart_cmds/tests.rs` | MIT OR Apache-2.0 |
| [`2d5a7497fd1e`](https://github.com/storytold/photocraft/commit/2d5a7497fd1e1891c796f1a93741b27a1d904604) | fix(format): harden directory-bundle cleanup | `Cargo.lock`<br>`crates/pc-format/Cargo.toml`<br>`crates/pc-format/src/store.rs`<br>`crates/pc-format/tests/it/directory_cleanup.rs`<br>`crates/pc-format/tests/it/main.rs` | MIT OR Apache-2.0 |
| [`26e003059bc0`](https://github.com/storytold/photocraft/commit/26e003059bc0f9599acfd4418a44ea3d57d5862e) | fix(ui): a save name without an extension gets the suggested one (#5039) (#5049) | `apps/local-image/src/services.rs`<br>`apps/local-image/src/services/portal_save.rs`<br>`apps/local-image/src/services/save_dialog.rs`<br>`apps/local-image/src/services/save_dialog/tests.rs` | MIT OR Apache-2.0 |
| [`58f855a40f67`](https://github.com/storytold/photocraft/commit/58f855a40f67a9b6a7e9168ea03c261226845705) | Save As on Linux: the file type picked sets the extension (#6121, #4328, #4394) (#6151) | `apps/local-image/src/services.rs`<br>`apps/local-image/src/services/portal_save.rs`<br>`apps/local-image/src/services/save_dialog.rs`<br>`apps/local-image/src/services/save_dialog/tests.rs` | MIT OR Apache-2.0 |
| [`fbcb70b31733`](https://github.com/storytold/photocraft/commit/fbcb70b317337d17614c94db08250de280793a34) | Save: ask before overwriting a file another app changed (#6596) (#6652) | `apps/local-image/src/services.rs`<br>`crates/li-i18n/catalogs/editor/cs.tsv`<br>`crates/li-i18n/catalogs/editor/de.tsv`<br>`crates/li-i18n/catalogs/editor/es.tsv`<br>`crates/li-i18n/catalogs/editor/fr.tsv`<br>`crates/li-i18n/catalogs/editor/id.tsv`<br>`crates/li-i18n/catalogs/editor/it.tsv`<br>`crates/li-i18n/catalogs/editor/ja.tsv`<br>+12 more | MIT OR Apache-2.0 |

## PhotoCraft upstream port, Codex batch (2026-10-18)

Ported by OpenAI Codex (gpt-6-astra) and reviewed, corrected and verified by Claude before merge. Same range as the section above.

| Upstream commit | Subject | Our files | Licence |
|---|---|---|---|
| [`9cc8be43929b`](https://github.com/storytold/photocraft/commit/9cc8be43929b70d9f352b14b293b2ba2089429b2) | fix(algo): keep clipped warp bounds from overflowing (#5451) | `crates/pc-algo/src/liquify.rs` | MIT OR Apache-2.0 |
| [`42b399881a7e`](https://github.com/storytold/photocraft/commit/42b399881a7e428055da72e15a6b9f337e137afa) | Handle signed coordinate limits in the Offset filter (#4738) | `crates/pc-engine/src/filters.rs` | MIT OR Apache-2.0 |
| [`2a0bebb49a77`](https://github.com/storytold/photocraft/commit/2a0bebb49a778d4115222efec02c11b33ebe8850) | fix(engine): validate bounded extended filter parameters (#5586) | `crates/pc-engine/src/filters.rs`<br>`crates/pc-engine/src/jobs.rs`<br>`crates/pc-ui-egui/src/filter_dialog.rs` | MIT OR Apache-2.0 |
| [`b8ae1cd92fcc`](https://github.com/storytold/photocraft/commit/b8ae1cd92fccacfc5b30566e64c22bdebe0a75eb) | fix(brush): preserve Dual Brush tip spacing and scale its size with the primary brush (#4856) | `crates/pc-engine/src/brush_cmds.rs`<br>`crates/pc-engine/src/brush_cmds/tests.rs`<br>`crates/pc-paint/src/brush.rs` | MIT OR Apache-2.0 |
| [`cf804c4b27ca`](https://github.com/storytold/photocraft/commit/cf804c4b27cad5a1fc79d8b715ba6320c2f5b369) | feat(brush): Photoshop's measured round tip (#4617) | `crates/pc-engine/src/brush_cmds/tests.rs`<br>`crates/pc-paint/src/brush_tests.rs`<br>`crates/pc-paint/src/continuous.rs`<br>`crates/pc-paint/src/lib.rs`<br>`crates/pc-paint/src/render.rs`<br>`crates/pc-paint/src/retouch.rs`<br>`crates/pc-paint/tests/coverage/continuous_coverage.rs`<br>`crates/pc-ui-egui/src/canvas/tests.rs` | MIT OR Apache-2.0 |
| [`cda46050f7cc`](https://github.com/storytold/photocraft/commit/cda46050f7ccd7adda621daea9e0ab730b0cb9c9) | Background Eraser: sample and match against the pre-stroke pixels (#4552) | `crates/pc-paint/src/bg_erase.rs` | MIT OR Apache-2.0 |
| [`077e513686ce`](https://github.com/storytold/photocraft/commit/077e513686ce4f79e957d3ce71340495cbcdb0af) | Color Range presets: weigh dark colours like Photoshop (#5243) (#5293) | `crates/pc-algo/src/selection.rs` | MIT OR Apache-2.0 |
| [`cb8ed7620e3e`](https://github.com/storytold/photocraft/commit/cb8ed7620e3e8499b75d4b2f574027e4f628147b) | Content-Aware Fill: no ghost edges or object colour around thin removed objects (#5290) | `crates/pc-algo/src/inpaint.rs` | MIT OR Apache-2.0 |
| [`d740ea638d4c`](https://github.com/storytold/photocraft/commit/d740ea638d4c1f6ede3dff166115ea153d2310b6) | Puppet Warp: a linked layer mask moves with the pixels (#4910) (#4989) | `crates/pc-algo/src/puppet.rs`<br>`crates/pc-engine/src/distort_cmds.rs`<br>`crates/pc-engine/src/distort_cmds/tests.rs` | MIT OR Apache-2.0 |
| [`5f2c23e14249`](https://github.com/storytold/photocraft/commit/5f2c23e14249038d4c49ef62c87e8257cb46a1a6) | Filters: Photoshop's Blur More kernel, Find Edges strength and Polar Coordinates direction (#4652) | `crates/pc-algo/src/distort.rs`<br>`crates/pc-algo/src/stylize.rs`<br>`crates/pc-algo/src/tests.rs`<br>`crates/pc-engine/src/filters.rs` | MIT OR Apache-2.0 |
| [`96c0241b26e9`](https://github.com/storytold/photocraft/commit/96c0241b26e9e5d7fd48ae2fe6b7149a246a8f4a) | Distort: a quad dragged past the horizon draws only its front side, in the preview and the result (#4863) (#4870) | `crates/pc-algo/src/transform.rs` | MIT OR Apache-2.0 |
| [`9c3f96649f79`](https://github.com/storytold/photocraft/commit/9c3f96649f7990e973ee1a41b1026c416a671f29) | fix(transform): Photoshop's Bicubic for Free Transform and Warp, with a reduction prefilter (#4635) | `crates/pc-algo/src/transform.rs`<br>`crates/pc-algo/src/warp.rs` | MIT OR Apache-2.0 |
| [`45f8921ae84e`](https://github.com/storytold/photocraft/commit/45f8921ae84e0583b69460d4c62b7c1f530a05fd) | Free Transform: solid pixel stretch (edge-repeat sampling, selected-pixel bounds) (#3867) | `crates/pc-algo/src/transform.rs`<br>`crates/pc-engine/src/transform_cmds.rs` | MIT OR Apache-2.0 |
| [`65e8d5f02049`](https://github.com/storytold/photocraft/commit/65e8d5f02049c4a399ac49ea4b7f6faec85f3ebd) | GPU canvas: price the display textures before building them (#2934) (#3299) | `crates/li-gpu/src/lib.rs`<br>`crates/li-gpu/src/memory.rs`<br>`crates/li-i18n/catalogs/editor/cs.tsv`<br>`crates/li-i18n/catalogs/editor/de.tsv`<br>`crates/li-i18n/catalogs/editor/es.tsv`<br>`crates/li-i18n/catalogs/editor/fr.tsv`<br>`crates/li-i18n/catalogs/editor/id.tsv`<br>`crates/li-i18n/catalogs/editor/it.tsv`<br>+14 more | MIT OR Apache-2.0 |
| [`43b4742e3f6c`](https://github.com/storytold/photocraft/commit/43b4742e3f6c87711baff1564d679de8b8cf4943) | GPU startup: a failed CPU start no longer pins later starts to CPU | `apps/local-image/src/gpu_startup.rs`<br>`apps/local-image/src/main.rs`<br>`crates/pc-ui-egui/src/gpu_status.rs` | MIT OR Apache-2.0 |
| [`95dbb01ad9ef`](https://github.com/storytold/photocraft/commit/95dbb01ad9ef770bc9a83c1fd8d2b35e913316fb) | fix(gpu): preflight compositor uniform buffer limits (#3931) | `crates/pc-gpu/src/lib.rs`<br>`crates/pc-gpu/tests/gpu/main.rs`<br>`crates/pc-gpu/tests/gpu/uniform_limits.rs` | MIT OR Apache-2.0 |
| [`2e242bde67c2`](https://github.com/storytold/photocraft/commit/2e242bde67c202e4cd16bf3d6f88088b521ab423) | Fix Export As freeze on large layered PSDs (#5104) | `CHANGELOG.md`<br>`apps/local-image/src/library_host/tests.rs`<br>`apps/local-image/src/services.rs`<br>`crates/pc-io/tests/vector_fallback/app_gpu.rs`<br>`crates/pc-pdf/tests/export/app_gpu.rs`<br>`crates/pc-ui-egui/examples/snapshot.rs`<br>`crates/pc-ui-egui/src/bugfix_save_tests.rs`<br>`crates/pc-ui-egui/src/control.rs`<br>+8 more | MIT OR Apache-2.0 |
| [`3cd0f6c95495`](https://github.com/storytold/photocraft/commit/3cd0f6c954952aaf1e4b5f9db4bb344ae0b86b20) | Keyboard Shortcuts: Accept and Go to Conflict / Undo Changes, and reserved keys (K4a) (#5349) | `CHANGELOG.md`<br>`crates/li-i18n/catalogs/editor/cs.tsv`<br>`crates/li-i18n/catalogs/editor/de.tsv`<br>`crates/li-i18n/catalogs/editor/es.tsv`<br>`crates/li-i18n/catalogs/editor/fr.tsv`<br>`crates/li-i18n/catalogs/editor/id.tsv`<br>`crates/li-i18n/catalogs/editor/it.tsv`<br>`crates/li-i18n/catalogs/editor/ja.tsv`<br>+7 more | MIT OR Apache-2.0 |
| [`6f85054288a5`](https://github.com/storytold/photocraft/commit/6f85054288a564f9fbbccb99268ffdb34e581584) | fix(clipboard): paste JPEG, BMP and other non-PNG images on Wayland (#2546) (#4752) | `CHANGELOG.md`<br>`Cargo.lock`<br>`apps/local-image/Cargo.toml`<br>`apps/local-image/src/services.rs`<br>`apps/local-image/src/services/clipboard.rs` | MIT OR Apache-2.0 |
| [`13f893f9987a`](https://github.com/storytold/photocraft/commit/13f893f9987a88b0048e2b43b3e44b1b955c0b6f) | fix(export): prompt for a Render Video output directory (#3365) (#3378) | `CHANGELOG.md`<br>`apps/local-image/src/services.rs`<br>`crates/pc-ui-egui/src/lib.rs`<br>`crates/pc-ui-egui/src/menus.rs`<br>`crates/pc-ui-egui/src/menus/video_tests.rs` | MIT OR Apache-2.0 |
| [`b21956aef76e`](https://github.com/storytold/photocraft/commit/b21956aef76e84f2326d5cda43b1bec92ad88868) | fix(ui): Match Color's preview reads the Source document (#4851) (#4886) | `CHANGELOG.md`<br>`crates/pc-ui-egui/src/canvas/display.rs`<br>`crates/pc-ui-egui/src/filter_dialog.rs` | MIT OR Apache-2.0 |
| [`be32a7c651e4`](https://github.com/storytold/photocraft/commit/be32a7c651e4066c283284050b12ca8896f6f7ef) | fix: commit type editing on Layers-panel selection (#5584) (#5612) | `CHANGELOG.md`<br>`crates/pc-ui-egui/src/layer_row_ui/edit_tests.rs`<br>`crates/pc-ui-egui/src/panels/layers.rs`<br>`crates/pc-ui-egui/src/panels/tests.rs` | MIT OR Apache-2.0 |
| [`ce6fccc3f5e6`](https://github.com/storytold/photocraft/commit/ce6fccc3f5e65d10af8b5eca4a9e7b441640a29b) | fix(type): missing-font checks follow layout's PostScript resolution; split hyphenless style suffixes (#608) | `crates/pc-engine/src/type_extra_cmds.rs`<br>`crates/pc-engine/src/type_extra_cmds/tests.rs`<br>`crates/pc-text/src/fonts.rs` | MIT OR Apache-2.0 |
| [`af21e1e21b0c`](https://github.com/storytold/photocraft/commit/af21e1e21b0cea60a46f12452b689efa83edbe03) | fix(text): preserve hyphenated PSD font identity on reopen (#3878) | `crates/pc-io/tests/it/main.rs`<br>`crates/pc-io/tests/it/psd_font_identity.rs`<br>`crates/pc-text/src/fonts.rs` | MIT OR Apache-2.0 |
| [`2819f370332c`](https://github.com/storytold/photocraft/commit/2819f370332c7ca8a5df72e93c3076aa16aa052a) | PSD export: pattern fill layers keep their pattern in CMYK and Lab (#1917) | `crates/pc-io/src/psd_export.rs`<br>`crates/pc-io/tests/it/main.rs`<br>`crates/pc-io/tests/it/pattern_fill_psd.rs` | MIT OR Apache-2.0 |
| [`4e1e325b1713`](https://github.com/storytold/photocraft/commit/4e1e325b171343235fce5860a5fea40749957895) | PSD: a smart object's filter mask shows when the file opens (#5005) | `crates/pc-compose/src/filter_mask.rs`<br>`crates/pc-compose/src/lib.rs`<br>`crates/pc-engine/src/smart_cmds.rs`<br>`crates/pc-engine/src/smart_cmds/colour_tests.rs`<br>`crates/pc-io/src/psd_export.rs`<br>`crates/pc-io/src/psd_import.rs`<br>`crates/pc-io/src/smart_map.rs` | MIT OR Apache-2.0 |
| [`6547ca0b8459`](https://github.com/storytold/photocraft/commit/6547ca0b8459ef37e1dfb373cc00a81740ed74a4) | fix(psd): Multichannel alpha channels stay alpha; a save keeps resources and global blocks (#4031) | `crates/pc-io/src/channel_map.rs`<br>`crates/pc-io/src/multichannel_map.rs`<br>`crates/pc-io/src/psd_export.rs`<br>`crates/pc-io/src/psd_import.rs` | MIT OR Apache-2.0 |
| [`57624bd02eae`](https://github.com/storytold/photocraft/commit/57624bd02eaed7e3a510b8cd7c7d0b5cc2909193) | PSD: Backgrounds survive a round trip through PhotoCraft (#3560) | `crates/pc-io/src/flat.rs`<br>`crates/pc-io/src/psd_export.rs`<br>`crates/pc-io/src/psd_export/tests.rs` | MIT OR Apache-2.0 |
| [`3cb3580a46ba`](https://github.com/storytold/photocraft/commit/3cb3580a46ba080d7ce4a037efc0a921724bd939) | fix(psd): recover layered imports with damaged merged data (#6170) | `crates/pc-io/src/lib.rs`<br>`crates/pc-io/src/psd_import.rs`<br>`crates/pc-io/tests/it/corpus.rs`<br>`crates/pc-io/tests/it/main.rs`<br>`crates/pc-io/tests/it/psd_damaged_merged.rs`<br>`crates/pc-io/tests/it/psd_structure.rs`<br>`crates/pc-psd/src/file.rs`<br>`crates/pc-psd/tests/it/corpus.rs`<br>+2 more | MIT OR Apache-2.0 |
| [`d8217b3476e7`](https://github.com/storytold/photocraft/commit/d8217b3476e75ec1fe78408b713773c8d6f738d0) | fix(image): keep smart-filter masks aligned through Image Size, Canvas Size and Crop (#3195) | `crates/pc-engine/src/image_cmds.rs`<br>`crates/pc-engine/src/layer_command_tests.rs` | MIT OR Apache-2.0 |
| [`6cfed3a7dc00`](https://github.com/storytold/photocraft/commit/6cfed3a7dc00fa789cbcb974020457423b870fdb) | fix(masks): bake feathered coverage when applying pixel masks (#3044) (#3191) | `crates/pc-engine/src/layer_menu_cmds.rs`<br>`crates/pc-engine/src/layer_menu_cmds/tests.rs` | MIT OR Apache-2.0 |
| [`06dcdc12674f`](https://github.com/storytold/photocraft/commit/06dcdc12674f4f9e63858ea25c77df0ce4a962da) | fix(mode): restore indexed pixels after clearing transparency (#3080) (#3153) | `crates/pc-engine/src/layer_command_tests.rs`<br>`crates/pc-engine/src/mode_cmds.rs` | MIT OR Apache-2.0 |
| [`37929405df92`](https://github.com/storytold/photocraft/commit/37929405df92173efd3082e7baa09e8de9705c06) | fix(history): restore empty layer target on undo and redo (#3042) (#3179) | `crates/pc-engine/src/layer_command_tests.rs`<br>`crates/pc-engine/src/lib.rs` | MIT OR Apache-2.0 |
| [`40d7e6a1d8a9`](https://github.com/storytold/photocraft/commit/40d7e6a1d8a9ca352af1b5c5f2ad78b197c5e9aa) | fix(history): read pixels from locked history states (#3076) (#3186) | `crates/pc-engine/src/channel_cmds.rs`<br>`crates/pc-engine/src/fill_cmds.rs`<br>`crates/pc-engine/src/layer_command_tests.rs`<br>`crates/pc-engine/src/retouch_cmds.rs` | MIT OR Apache-2.0 |
| [`9c554e2eadf0`](https://github.com/storytold/photocraft/commit/9c554e2eadf0d086f918f4a5439cd51ca3595ae7) | Fix #1544: Image Size refuses a resample past the raster budget; resampling writes bands as it goes (#1569) | `crates/pc-engine/src/image_cmds.rs`<br>`crates/pc-engine/src/layer_command_tests.rs` | MIT OR Apache-2.0 |
| [`8157d1a8517b`](https://github.com/storytold/photocraft/commit/8157d1a8517b381384aa63397c431ff4b0018c15) | Free Transform: refuse outputs over a pixel budget instead of building them (#4293) | `crates/pc-algo/src/transform.rs`<br>`crates/pc-engine/src/layer_command_tests.rs`<br>`crates/pc-engine/src/select_extra_cmds.rs`<br>`crates/pc-engine/src/transform_cmds.rs` | MIT OR Apache-2.0 |
| [`d12dc6b71882`](https://github.com/storytold/photocraft/commit/d12dc6b7188204889b6bac4d5fca7e1fa24f8af3) | fix(layers): merging layers keeps pixels that lie outside the canvas (#5067) (#5124) | `crates/pc-engine/src/layer_command_tests.rs`<br>`crates/pc-engine/src/layer_multi_cmds.rs` | MIT OR Apache-2.0 |
| [`28ed1c114789`](https://github.com/storytold/photocraft/commit/28ed1c1147894a639b472dffcc0ae5f1e7067085) | Handle allocation failures in large pixel operations (#2530) | `crates/pc-engine/src/allocation.rs`<br>`crates/pc-engine/src/edit_cmds.rs`<br>`crates/pc-engine/src/edit_menu_cmds.rs`<br>`crates/pc-engine/src/extra_cmds.rs`<br>`crates/pc-engine/src/fill_cmds.rs`<br>`crates/pc-engine/src/layer_command_tests.rs`<br>`crates/pc-engine/src/pixels.rs`<br>`crates/pc-engine/src/warp_cmds.rs` | MIT OR Apache-2.0 |
| [`f02c0cbef704`](https://github.com/storytold/photocraft/commit/f02c0cbef7048d859189f3a333b6bec95d8fca16) | File › Scripts › Browse: reject Photoshop ExtendScript (#5406) (#5435) | `crates/pc-engine/src/automate_cmds.rs`<br>`crates/pc-engine/src/layer_command_tests.rs` | MIT OR Apache-2.0 |
| [`93397b08c63a`](https://github.com/storytold/photocraft/commit/93397b08c63a29cf93fb43d83af271ee825d676c) | Fill keys: Alt/Cmd+Backspace recolour the text of a type layer (fixes #4580) (#4606) | `crates/pc-engine/src/fill_key_cmds.rs`<br>`crates/pc-engine/src/layer_command_tests.rs` | MIT OR Apache-2.0 |
| [`adaebc084d65`](https://github.com/storytold/photocraft/commit/adaebc084d6537d7f70738a14cf2ecaa690fc8f8) | Channels: clicking RGB hides the alpha channel overlay (fixes #4538) (#4564) | `crates/pc-engine/src/channel_cmds.rs`<br>`crates/pc-engine/src/channel_cmds/tests.rs` | MIT OR Apache-2.0 |
| [`7b8e5034d04b`](https://github.com/storytold/photocraft/commit/7b8e5034d04b4171da5ea0aa9bb0fe3e397099f3) | Fix/scale marker rollback layer state 4975 (#5391) | `crates/pc-engine/src/analysis_cmds.rs`<br>`crates/pc-engine/src/analysis_cmds/tests.rs` | MIT OR Apache-2.0 |
| [`25dd3b4794c0`](https://github.com/storytold/photocraft/commit/25dd3b4794c080cf9890fcd25285da8bb7a42bd5) | Save for Web: no matte by default, as Photoshop's scripted export (#4115) (#4578) | `crates/pc-engine/src/web_cmds.rs`<br>`crates/pc-engine/src/web_cmds/tests.rs` | MIT OR Apache-2.0 |
| [`964f9fb9d09c`](https://github.com/storytold/photocraft/commit/964f9fb9d09c20937280df7d0dac4186d5564a44) | Sample every visible layer in Content-Aware Fill by default. (#4133) | `crates/pc-engine/src/edit_menu_cmds.rs`<br>`crates/pc-engine/src/edit_menu_cmds/tests.rs` | MIT OR Apache-2.0 |
| [`15f7618504d9`](https://github.com/storytold/photocraft/commit/15f7618504d9eaeb5ed910de9baa7a64c171a288) | Object Selection: say so when the sampled layer is empty (#5246) (#5298) | `crates/pc-engine/src/layer_command_tests.rs`<br>`crates/pc-engine/src/smartselect_cmds.rs` | MIT OR Apache-2.0 |
| [`4bbcede5725f`](https://github.com/storytold/photocraft/commit/4bbcede5725fdb8bdb4bfd501eef58b5af140834) | Fibers: fill an empty layer, as Photoshop does (#5241) (#5289) | `crates/pc-algo/src/lib.rs`<br>`crates/pc-engine/src/layer_command_tests.rs` | MIT OR Apache-2.0 |
| [`4062d40070d2`](https://github.com/storytold/photocraft/commit/4062d40070d248c957a7aea7927c510481c98879) | Color: Dot Gain 20%-compatible default working gray, as in Photoshop (#4610) | `crates/pc-cms/src/builtin.rs`<br>`crates/pc-cms/tests/coverage/builtin_coverage.rs`<br>`crates/pc-engine/src/color_cmds.rs`<br>`crates/pc-engine/src/display_color_tests.rs`<br>`crates/pc-engine/src/layer_command_tests.rs`<br>`crates/pc-ui-egui/src/chrome_ui.rs` | MIT OR Apache-2.0 |
| [`0bdaf905cc95`](https://github.com/storytold/photocraft/commit/0bdaf905cc9548d176c13e09f31a161a5745e7e2) | Curves: points outside 0..255 are refused, as the message says (#4868) | `crates/pc-engine/src/adjust_params.rs` | MIT OR Apache-2.0 |
| [`99083fca4c15`](https://github.com/storytold/photocraft/commit/99083fca4c151f3f02c808de6ca865b810c3cac4) | Quick Mask: ⌘I inverts the whole mask, unstored black included (#6095) (#6108) | `crates/pc-engine/src/commands.rs`<br>`crates/pc-engine/src/layer_command_tests.rs` | MIT OR Apache-2.0 |
| [`2307acef92ed`](https://github.com/storytold/photocraft/commit/2307acef92ed3ec0663b86d2fbd574fc87b3f7a5) | feat(select): preserve existing mask metadata and allow explicit target in refineEdge (#4499) | `crates/pc-engine/src/commands.rs`<br>`crates/pc-engine/src/cutout_cmds.rs`<br>`crates/pc-engine/src/extra_cmds.rs`<br>`crates/pc-engine/src/layer_command_tests.rs`<br>`crates/pc-engine/src/layer_menu_cmds.rs`<br>`crates/pc-engine/src/smartselect_cmds.rs` | MIT OR Apache-2.0 |
| [`7e28900df31a`](https://github.com/storytold/photocraft/commit/7e28900df31a6a64d16b9d252cfd7551b57b7352) | layer.setProps: refuse opacity and fill outside 0..1 (#5242) (#5291) | `crates/pc-engine/src/commands.rs`<br>`crates/pc-engine/src/layer_command_tests.rs` | MIT OR Apache-2.0 |
| [`ef8dc9ea3015`](https://github.com/storytold/photocraft/commit/ef8dc9ea30157d3d5c0499bd7a551bded9e899df) | Layer locks: locking the pixels clears the transparency lock, as in Photoshop (#4892) (#4898) | `crates/pc-engine/src/commands.rs`<br>`crates/pc-engine/src/gradient_fill_cmds.rs`<br>`crates/pc-engine/src/gradient_fill_cmds/tests.rs`<br>`crates/pc-engine/src/layer_command_tests.rs`<br>`crates/pc-engine/src/layer_multi_cmds.rs`<br>`crates/pc-engine/src/type_cmds.rs` | MIT OR Apache-2.0 |
| [`6a766345d37b`](https://github.com/storytold/photocraft/commit/6a766345d37b2096f59a7724c735465510130d98) | Layer styles: editing effects no longer re-enables a disabled effect list (#3648) (#3799) | `crates/pc-engine/src/layer_style.rs` | MIT OR Apache-2.0 |
| [`c11ef015b764`](https://github.com/storytold/photocraft/commit/c11ef015b7640b4dad43bf775373fd345902e382) | Paste Layer Style carries Blend If and channels; align to an active selection with several layers (#5881) | `crates/pc-engine/src/extra_cmds.rs`<br>`crates/pc-engine/src/layer_command_tests.rs`<br>`crates/pc-engine/src/layer_multi_cmds.rs`<br>`crates/pc-engine/src/lib.rs` | MIT OR Apache-2.0 |
| [`25c0ec78d574`](https://github.com/storytold/photocraft/commit/25c0ec78d5748d3f22de2628f9f5e0d63bc43587) | Keep CMYK ink values when flattening and merging (#3855) | `crates/pc-compose/src/lib.rs`<br>`crates/pc-compose/src/native.rs`<br>`crates/pc-engine/src/commands.rs`<br>`crates/pc-engine/src/pixels.rs` | MIT OR Apache-2.0 |
| [`06ad07ab9b1a`](https://github.com/storytold/photocraft/commit/06ad07ab9b1a90b9ab2ec8e923e74a5b1640b4c1) | fix(io): color-manage grayscale WebP exports (#5875) | `crates/pc-io/src/flat.rs`<br>`crates/pc-io/tests/it/flat.rs` | MIT OR Apache-2.0 |
| [`1a4f86795887`](https://github.com/storytold/photocraft/commit/1a4f867958878e6468cb29bdd7e33164292975da) | TIFF: read and write CIE L*a*b* (CIELab, ICCLab); Lab documents stay Lab (#4949) | `crates/lc-codecs/src/convert.rs`<br>`crates/pc-codecs/README.md`<br>`crates/pc-codecs/src/codecs/tiff.rs`<br>`crates/pc-codecs/src/fidelity.rs`<br>`crates/pc-codecs/src/format.rs`<br>`crates/pc-codecs/src/image.rs`<br>`crates/pc-codecs/tests/it/fidelity.rs`<br>`crates/pc-codecs/tests/it/main.rs`<br>+6 more | MIT OR Apache-2.0 |
| [`adfeab175069`](https://github.com/storytold/photocraft/commit/adfeab175069782a7dca7e36dc144544ed0e1c0c) | Composited CMYK documents now export as CMYK TIFF and JPEG with their profile (#2220) (#4199) | `crates/pc-io/src/flat.rs`<br>`crates/pc-io/src/lib.rs`<br>`crates/pc-io/tests/it/cmyk_export.rs`<br>`crates/pc-io/tests/it/main.rs` | MIT OR Apache-2.0 |
| [`5064d80b1e2d`](https://github.com/storytold/photocraft/commit/5064d80b1e2d5cebaa32362d6bee6285c186b391) | TIFF: write and read EXIF as the file's own Exif and GPS directories (#4876) | `crates/pc-codecs/README.md`<br>`crates/pc-codecs/src/codecs/mod.rs`<br>`crates/pc-codecs/src/codecs/tiff.rs`<br>`crates/pc-codecs/src/codecs/tiff_exif.rs`<br>`crates/pc-codecs/src/format.rs`<br>`crates/pc-codecs/tests/it/main.rs`<br>`crates/pc-codecs/tests/it/metadata.rs`<br>`crates/pc-codecs/tests/it/tiff_exif.rs` | MIT OR Apache-2.0 |
| [`75978a72f04e`](https://github.com/storytold/photocraft/commit/75978a72f04ec9b9cefcfcd5ac365d36d74cf8b9) | Targa: RLE packets end with their scanline (#6382) (#6386) | `crates/pc-codecs/README.md`<br>`crates/pc-codecs/src/codecs/mod.rs`<br>`crates/pc-codecs/src/codecs/tga.rs`<br>`crates/pc-codecs/src/codecs/via_image.rs`<br>`crates/pc-codecs/tests/it/main.rs`<br>`crates/pc-codecs/tests/it/tga.rs` | MIT OR Apache-2.0 |
| [`bacc886a6d41`](https://github.com/storytold/photocraft/commit/bacc886a6d41b14ee68f83de9f4a855b56bb8820) | Preserve signed HDR colors during surface transforms (#3857) | `crates/pc-algo/src/transform.rs` | MIT OR Apache-2.0 |
| [`4543ef35b975`](https://github.com/storytold/photocraft/commit/4543ef35b975b2c177ff55b3acd5d92fea12387c) | Preserve signed HDR colors in float text rasterization (#4379) | `crates/pc-text/src/render.rs`<br>`crates/pc-text/src/tests.rs` | MIT OR Apache-2.0 |
| [`7e94c507e933`](https://github.com/storytold/photocraft/commit/7e94c507e9336454a81d740eab58c20e69c19a3e) | Smart Objects: Curves preview no longer zooms in on a reduced proxy (#5766) (#5779) | `crates/pc-compose/src/proxy.rs` | MIT OR Apache-2.0 |
| [`c4f0b8aa8b1a`](https://github.com/storytold/photocraft/commit/c4f0b8aa8b1a027d3d7e15821a64e166cd8675b0) | fix(compose): redraw shapes crossing canvas bounds and include vector bounds in damage (#3946) (#4493) | `crates/pc-compose/src/lib.rs`<br>`crates/pc-compose/src/tests/layer_rendering.rs` | MIT OR Apache-2.0 |
| [`3feebb9b131a`](https://github.com/storytold/photocraft/commit/3feebb9b131a120fc9badc77d11d029ec1d533bb) | Groups: a pass-through clipping base with an adjustment renders isolated (passthrough_clipping_mask_adjustment passes) (#5272) | `crates/pc-compose/src/lib.rs`<br>`crates/pc-compose/src/tests/layer_rendering.rs`<br>`crates/pc-gpu/src/plan.rs`<br>`crates/pc-gpu/tests/gpu/parity.rs` | MIT OR Apache-2.0 |
| [`c7366b8938cf`](https://github.com/storytold/photocraft/commit/c7366b8938cf722af8e098bea25c8cc55abb4f37) | Drop shadow: knocked out beneath the whole shape when the layer's mode blends with it (#5304) | `crates/pc-compose/src/effects.rs`<br>`crates/pc-compose/src/tests/layer_rendering.rs`<br>`crates/pc-gpu/src/plan.rs`<br>`crates/pc-gpu/tests/gpu/parity.rs` | MIT OR Apache-2.0 |
| [`99e1138877e6`](https://github.com/storytold/photocraft/commit/99e1138877e6aa7e856c6883b415e708d042fc59) | Stroke layer effect: the outer edge stays round at wide sizes (#3646) (#3650) | `crates/pc-compose/src/effects.rs`<br>`crates/pc-compose/src/tests.rs`<br>`crates/pc-compose/src/tests/layer_rendering.rs`<br>`crates/pc-gpu/src/fx.rs` | MIT OR Apache-2.0 |
| [`8da02bf5400d`](https://github.com/storytold/photocraft/commit/8da02bf5400d7394da0d3d4611adee5f7d3e75cd) | Hue/Saturation: colour ranges apply in full to dull colours (part of #4089) (#4645) | `crates/pc-compose/src/adjust.rs`<br>`crates/pc-gpu/src/compose.wgsl`<br>`crates/pc-gpu/tests/gpu/parity.rs` | MIT OR Apache-2.0 |
| [`8fb32981611a`](https://github.com/storytold/photocraft/commit/8fb32981611a1bdb34ff68ca7e52169f5975b732) | Color Balance: fold the tones into one Levels per channel (#3423) (#5339) | `crates/pc-compose/src/adjust.rs`<br>`crates/pc-gpu/src/compose.wgsl`<br>`crates/pc-gpu/src/plan.rs` | MIT OR Apache-2.0 |
| [`93a16218de5d`](https://github.com/storytold/photocraft/commit/93a16218de5d6567e41f7edc909360e0a3a73b10) | Gradient Map is driven by the W3C luminosity, as in Photoshop (#5018) | `crates/pc-compose/src/adjust.rs`<br>`crates/pc-gpu/src/compose.wgsl` | MIT OR Apache-2.0 |
| [`9954c2935e58`](https://github.com/storytold/photocraft/commit/9954c2935e58508c6dfa2e9e7c076d44bf3846b8) | Threshold: split on Photoshop's luminosity of whole samples (threshold_rgb, threshold_16bits_rgb pass) (#5263) | `crates/pc-compose/src/adjust.rs`<br>`crates/pc-gpu/src/compose.wgsl`<br>`crates/pc-gpu/src/plan.rs` | MIT OR Apache-2.0 |
| [`d24f6e9d5fac`](https://github.com/storytold/photocraft/commit/d24f6e9d5faca46b512162690e10537512377614) | Levels: whole levels at every step and Photoshop's shadow toe (levels_rgb + stack oracle pass) (#5143) | `crates/pc-compose/src/adjust.rs`<br>`crates/pc-compose/src/tests.rs`<br>`crates/pc-compose/src/tests/tone_curves.rs` | MIT OR Apache-2.0 |
| [`00c16f2c6d42`](https://github.com/storytold/photocraft/commit/00c16f2c6d42c46605521ab8351b03416cb6165f) | Modern Brightness/Contrast: Photoshop's brightness curve (six oracles pass) (#4722) | `crates/pc-compose/src/adjust.rs`<br>`crates/pc-compose/src/tests.rs`<br>`crates/pc-compose/src/tests/tone_curves.rs`<br>`crates/pc-gpu/src/compose.wgsl`<br>`crates/pc-gpu/src/plan.rs` | MIT OR Apache-2.0 |

## PhotoCraft upstream port (2026-10-19)

| Upstream commit | Subject | Our files | Licence |
|---|---|---|---|
| [`b97a72ac794b`](https://github.com/storytold/photocraft/commit/b97a72ac794b0ac6849b7b33f07eadd8e298615f) | File dialogs no longer freeze or crash the app (#673, #574) (#1071) | `apps/local-image/src/main.rs`<br>`apps/local-image/src/services.rs`<br>`crates/pc-ui-egui/src/file_dialog.rs`<br>`crates/pc-ui-egui/src/file_dialog/tests.rs`<br>`crates/pc-ui-egui/src/dialog_filter.rs`<br>`crates/pc-ui-egui/src/lib.rs`<br>`crates/pc-ui-egui/src/menus.rs`<br>`crates/pc-ui-egui/src/discard_ui.rs`<br>`crates/pc-ui-egui/src/dialogs.rs`<br>`crates/pc-ui-egui/src/export_dialog.rs`<br>`crates/pc-ui-egui/src/file_ui.rs`<br>`crates/pc-ui-egui/src/view_cmds.rs`<br>+28 more | MIT OR Apache-2.0 |

## PhotoCraft and LightCraft hardening fixes (2026-10-22)

| Upstream commit | Subject | Our files | Licence |
|---|---|---|---|
| [`e3224000cac1`](https://github.com/storytold/photocraft/commit/e3224000cac1a30a19e239d67dcad110ccb7c8b2) | fix(psd): make packbits::decode_into crate-private and overflow-safe (#6950) | `crates/pc-psd/src/compression.rs` | MIT OR Apache-2.0 |
| [`035d572ac81a`](https://github.com/storytold/lightcraft/commit/035d572ac81ae8bfa87817bb189fb014e7a6018c) | Refuse catalog snapshots with an out-of-range seq | `crates/lc-catalog/src/journal.rs`<br>`crates/lc-catalog/src/tests_journal.rs` | MIT OR Apache-2.0 |
| [`14d321c107c6`](https://github.com/storytold/lightcraft/commit/14d321c107c690f1de7f2d97f880a9cd771adff2) | catalog: slice captured dates on char boundaries so non-ASCII dates don't panic | `crates/lc-catalog/src/query.rs`<br>`crates/lc-catalog/src/tests.rs` | MIT OR Apache-2.0 |
| [`6e97193b00cb`](https://github.com/storytold/lightcraft/commit/6e97193b00cbe6af249336e6f4f93f8ff44d102a) | Reject impossible hour/minute/second in normalize_iso | `crates/lc-catalog/src/stacks.rs`<br>`crates/lc-catalog/src/dates.rs` | MIT OR Apache-2.0 |
| [`80e439f6dc6d`](https://github.com/storytold/lightcraft/commit/80e439f6dc6d47c8eb9b646d93c0bc11f5ae5a12) | Reject signed hex digits in #rrggbb colour parsing | `crates/li-color/src/lib.rs` | MIT OR Apache-2.0 |

## LightCraft RAW and TIFF hardening (2026-10-23)

| Upstream commit | Subject | Our files | Licence |
|---|---|---|---|
| [`6d080de44702`](https://github.com/storytold/lightcraft/commit/6d080de4470221e2289203c86d51023a9d0c7790) | Never crash: Check raw data size before allocating in PEF, TIFF tiles and ARW6 (#1512) | `crates/lc-raw/src/tiffraw.rs`<br>`crates/lc-raw/src/vendor/pef.rs` | MIT OR Apache-2.0 |
| [`b62b51378e38`](https://github.com/storytold/lightcraft/commit/b62b51378e380b65d31e2a57ae283f2e1ecc342c) | Never crash: Fix i32 overflow in the NEF and PEF Huffman predictors (#1515) | `crates/lc-raw/src/vendor/nefc.rs`<br>`crates/lc-raw/src/vendor/pef.rs` | MIT OR Apache-2.0 |
| [`3bb13afdead2`](https://github.com/storytold/lightcraft/commit/3bb13afdead2ceb4f97850726e1532e1f03353fb) | raw: check maker-note crop bounds with checked_add | `crates/lc-raw/src/lib.rs`<br>`crates/lc-raw/src/vendor/orf.rs`<br>`crates/lc-raw/src/vendor/pef.rs` | MIT OR Apache-2.0 |
| [`555eb4c9fd28`](https://github.com/storytold/lightcraft/commit/555eb4c9fd28b433b10a8fb162b532a66109079d) | tiff: bound total entries across a parse (#2136) | `crates/lc-tiff/src/reader.rs` | MIT OR Apache-2.0 |
| [`cef6f2d4b8da`](https://github.com/storytold/lightcraft/commit/cef6f2d4b8da3c1c81c2351eea90f28f44940d32) | Never crash: Fix demosaic panic on a NaN black level | `crates/lc-raw/src/dng.rs`<br>`crates/lc-raw/src/lib.rs` | MIT OR Apache-2.0 |
| [`89fa4e4b59d4`](https://github.com/storytold/lightcraft/commit/89fa4e4b59d43bf45410b31e4b93df7cd29aeb04) | LR-IMP-FORMATS: a float raw with NaN or infinite samples no longer panics the demosaic | `crates/lc-raw/src/demosaic/ahd.rs`<br>`crates/lc-raw/src/demosaic/mod.rs`<br>`crates/lc-raw/src/demosaic/ppg.rs`<br>`crates/lc-raw/src/dng.rs` | MIT OR Apache-2.0 |
| [`3624a50423ae`](https://github.com/storytold/lightcraft/commit/3624a50423aefedcc09adc207d92e874c3f153ab) | Never crash: Refuse raw data with two samples per pixel (#1511) | `crates/lc-raw/src/demosaic/mod.rs`<br>`crates/lc-raw/src/dng.rs`<br>`crates/lc-raw/src/lib.rs` | MIT OR Apache-2.0 |
| [`8d96dab1afe5`](https://github.com/storytold/lightcraft/commit/8d96dab1afe5a6d8603b4daa190823d72a143074) | LR-IMP-FORMATS: checked sum of the sRAW slice widths (an overflowing or zero sum is an error, not a panic) | `crates/lc-raw/src/vendor/cr2.rs` | MIT OR Apache-2.0 |
| [`3946688f49eb`](https://github.com/storytold/lightcraft/commit/3946688f49eb81ebf23888d998943231017948bf) | Reject RAW TIFFs with missing or undecodable image chunks (#1513) | `crates/lc-raw/src/tiffraw.rs` | MIT OR Apache-2.0 |
| [`df914dca41a0`](https://github.com/storytold/lightcraft/commit/df914dca41a0b18dfe21b248fa90c81e7891908a) | Reject PEF streams that end before the pixels do instead of decoding zero padding (#1514) | `crates/lc-raw/src/vendor/pef.rs` | MIT OR Apache-2.0 |

## LightCraft Library and Develop crash fixes (2026-10-23)

| Upstream commit | Subject | Our files | Licence |
|---|---|---|---|
| [`b43b7169aa2a`](https://github.com/storytold/lightcraft/commit/b43b7169aa2a85759aed3875e15d905c01803b78) | Never crash: Limit how much a preset zip can unpack | `crates/lc-engine/src/preset_import.rs` | MIT OR Apache-2.0 |
| [`1f87328c6d08`](https://github.com/storytold/lightcraft/commit/1f87328c6d083f921969605d5846b412a1b56572) | Never crash: Write preview cache files from one bounded writer (#1522) | `crates/lc-preview/src/lib.rs`<br>`crates/lc-preview/src/tests.rs` | MIT OR Apache-2.0 |
| [`935fabeb1828`](https://github.com/storytold/lightcraft/commit/935fabeb18286a1f4368196f2fb5151874ff87e5) | Preview disk cache: ignore symlinked shard directories (#1504) | `crates/lc-preview/src/disk.rs`<br>`crates/lc-preview/src/tests.rs` | MIT OR Apache-2.0 |
| [`414764ef77fd`](https://github.com/storytold/lightcraft/commit/414764ef77fd230d6ac54a59321bfcb36752790f) | Stop export lanes when a panic unwinds the batch thread | `crates/lc-engine/src/export.rs`<br>`crates/lc-engine/src/export/tests.rs` | MIT OR Apache-2.0 |
| [`27d1e9b04d7c`](https://github.com/storytold/lightcraft/commit/27d1e9b04d7c0415bcdb8130024d322da1e3ed98) | Never crash: Cap enlarged exports at 512 MP and check memory first | `crates/lc-engine/src/memory.rs`<br>`crates/lc-engine/src/export/options.rs`<br>`crates/lc-engine/src/export/prepared.rs`<br>`crates/lc-engine/src/export/tests.rs` | MIT OR Apache-2.0 |
| [`337f02b4dc6a`](https://github.com/storytold/lightcraft/commit/337f02b4dc6ad9d82f0caf166849ba2975e2d65b) | Never crash: Fix export size panic for very long strips | `crates/lc-engine/src/export/options.rs`<br>`crates/lc-engine/src/export/tests.rs` | MIT OR Apache-2.0 |
| [`f753778f450d`](https://github.com/storytold/lightcraft/commit/f753778f450d87a0d2f695d10b51ad1d9e4ab5f0) | Never crash: Fix blur overflow and hangs on sliver crops | `crates/lc-pipeline/src/tests.rs` | MIT OR Apache-2.0 |
| [`4a07254e061f`](https://github.com/storytold/lightcraft/commit/4a07254e061fd6db5b18a8a779eb00bc541899ab) | Never crash: Read the edge pixel for bilinear samples outside the picture | `crates/lc-engine/src/tests.rs`<br>`crates/li-raster/src/frame.rs` | MIT OR Apache-2.0 |
| [`57487875b541`](https://github.com/storytold/lightcraft/commit/57487875b54127fb663f879e2f49b2d7e1eca3db) | Never crash: Cap the spot radius so huge spot sizes render (#1519) | `crates/lc-engine/src/tests.rs`<br>`crates/lc-pipeline/src/spots.rs` | MIT OR Apache-2.0 |
| [`307843b2adf9`](https://github.com/storytold/lightcraft/commit/307843b2adf98663e72639d9e20de5a3d922aa7f) | Never crash: Fix defringe radius overflow for amounts past the sliders | `crates/lc-pipeline/src/optics.rs` | MIT OR Apache-2.0 |
| [`d770c5231377`](https://github.com/storytold/lightcraft/commit/d770c5231377ccc1fd8c5c046665b6274ee8b3df) | Masks: bound the total dabs of one brush stroke (#1520) | `crates/lc-pipeline/src/masks.rs` | MIT OR Apache-2.0 |
| [`389a2cdd6581`](https://github.com/storytold/lightcraft/commit/389a2cdd6581bcf4157cfc5391bba8bc184c0a91) | GPU: a lost device is not polled again, and a poll that panics fails the render (#1858) | `crates/lc-gpu/src/ctx.rs`<br>`crates/lc-gpu/src/render.rs` | MIT OR Apache-2.0 |
| [`fdccb413cd88`](https://github.com/storytold/lightcraft/commit/fdccb413cd8887fce4e848ea396c50bdbcab11ed) | fix(import): avoid scanning symlink aliases into the library | `crates/lc-engine/src/walk.rs` | MIT OR Apache-2.0 |
| [`e4f92df78c6f`](https://github.com/storytold/lightcraft/commit/e4f92df78c6fcd2ca0810f0c60bc02eefbce9888) | fix(meta): reject incomplete and mismatched GPX documents | `crates/lc-meta/src/gpx.rs` | MIT OR Apache-2.0 |
| [`a77b0cc472ab`](https://github.com/storytold/lightcraft/commit/a77b0cc472abf8a8b2c387bff8c78784d780768c) | fix(meta): reject impossible Gregorian capture dates | `crates/lc-meta/src/datetime.rs` | MIT OR Apache-2.0 |
| [`e1860067a688`](https://github.com/storytold/lightcraft/commit/e1860067a6889b9591758167b64654606f45d4c4) | Never crash: Render demo scenes at most at their own size | `crates/lc-scenes/src/lib.rs` | MIT OR Apache-2.0 |
| [`911d7df32139`](https://github.com/storytold/lightcraft/commit/911d7df32139010230aed248e1326976dd2d7902) | Never crash: Fix panic dragging a tone curve point between close neighbours | `crates/lc-engine/src/cmd/develop.rs`<br>`crates/lc-engine/src/crs.rs`<br>`crates/lc-ui-egui/src/panels/edit.rs`<br>`crates/lc-ui-egui/src/tests_curve.rs` | MIT OR Apache-2.0 |
| [`e9d36451e6bc`](https://github.com/storytold/lightcraft/commit/e9d36451e6bc3eb6060789957c4159bda42ccab9) | Never crash: Fix overflow in mask.move with a huge delta | `crates/lc-engine/src/cmd/masks.rs`<br>`crates/lc-engine/src/tests.rs` | MIT OR Apache-2.0 |
| [`4c088c32846b`](https://github.com/storytold/lightcraft/commit/4c088c32846b989ce4b631d504dc135459533e89) | Never crash: Give the loupe a finite aspect for a zero-size crop | `crates/lc-pipeline/src/geometry.rs` | MIT OR Apache-2.0 |
| [`c41b1e311d47`](https://github.com/storytold/lightcraft/commit/c41b1e311d4788c14aa52ebf157c01399e74dd9f) | fix(geom): reject non-finite affine and projective transforms | `crates/lc-geom/src/homography.rs`<br>`crates/lc-geom/src/lib.rs` | MIT OR Apache-2.0 |

Partial ports: `f753778f` ports only its test, since `li-raster`'s box radii already cap the
sigma and compute the square in floating point. `389a2cdd` guards the readback and profiling
polls through `DeviceHealth`; its lost-device regression tests need a real GPU and were not
ported.

## PhotoCraft editor file safety (2026-10-23)

| Upstream commit | Subject | Our files | Licence |
|---|---|---|---|
| [`4615a8bc26c7`](https://github.com/storytold/photocraft/commit/4615a8bc26c78805debbdacb6e8b92c5c3a3585c) | Survive a lost window GPU device: draw nothing, save unsaved work for recovery, close (#3438, #1215) | `crates/pc-format/src/autosave.rs`<br>`crates/pc-ui-egui/src/control.rs`<br>`crates/pc-ui-egui/src/gpu_status.rs`<br>`crates/pc-ui-egui/src/lib.rs`<br>`crates/li-gpu/src/health.rs`<br>`crates/pc-format/tests/it/autosave.rs`<br>`apps/local-image/src/main.rs`<br>`apps/local-image/src/services.rs` | MIT OR Apache-2.0 |
| [`7bb7819b0050`](https://github.com/storytold/photocraft/commit/7bb7819b0050f485370aa5d6d92f6e0d2eadd1de) | Lost window device: close only once every unsaved document is saved for recovery, retrying every 2 s; also check the device on the screen-picker frame (#6464 review) | `crates/pc-format/src/autosave.rs`<br>`crates/pc-ui-egui/src/gpu_status.rs`<br>`crates/pc-ui-egui/src/lib.rs`<br>`crates/pc-format/tests/it/autosave.rs` | MIT OR Apache-2.0 |
| [`589941a536bf`](https://github.com/storytold/photocraft/commit/589941a536bff8bd7bdea6d4b9be765ab94644a3) | Save: refuse to replace a read-only file (#6600) (#6643) | `crates/li-fs/src/lib.rs`<br>`crates/li-fs/src/tests.rs`<br>`apps/local-image/src/services.rs`<br>`crates/pc-engine/src/file_cmds.rs` | MIT OR Apache-2.0 |
| [`921637322cc6`](https://github.com/storytold/photocraft/commit/921637322cc623303d40dd902aaa6da11ebac3a0) | Native format: refuse a .pcraft with out-of-range numbers when it is opened (#6862) | `crates/pc-format/src/store.rs`<br>`crates/pc-format/tests/it/corruption.rs` | MIT OR Apache-2.0 |
| [`0f0f58c2ab36`](https://github.com/storytold/photocraft/commit/0f0f58c2ab369a07241e0dcfb3d1cf6792829488) | PSD: layer_tree_max_depth hoists layers under unclosed dividers past the cap (#5758) (#5759) | `crates/pc-psd/src/tree.rs`<br>`crates/pc-psd/tests/it/tree.rs` | MIT OR Apache-2.0 |
| [`205c9c173721`](https://github.com/storytold/photocraft/commit/205c9c173721fadceeeb3695794387eceabe8950) | History Brush: bound the stroke before its dense buffers (#4961) | `crates/pc-engine/src/retouch_cmds.rs`<br>`crates/pc-engine/src/retouch_cmds/tests.rs` | MIT OR Apache-2.0 |
| [`3f216a0cd1b9`](https://github.com/storytold/photocraft/commit/3f216a0cd1b9549a3ec81db75f5d160badf14d89) | fix(engine): reject numeric overflow before document edits | `crates/pc-engine/src/adjust_cmds.rs`<br>`crates/pc-engine/src/analysis_cmds.rs`<br>`crates/pc-engine/src/artboard_cmds.rs`<br>`crates/pc-engine/src/channel_cmds.rs`<br>`crates/pc-engine/src/commands.rs`<br>`crates/pc-engine/src/extra_cmds.rs`<br>`crates/pc-engine/src/fill_cmds.rs`<br>`crates/pc-engine/src/layer_style.rs`<br>`crates/pc-engine/src/lens_cmds.rs`<br>`crates/pc-engine/src/notes_cmds.rs`<br>`crates/pc-engine/src/type_cmds.rs`<br>`crates/pc-engine/src/cutout_cmds.rs`<br>`crates/pc-engine/tests/it/finite_command_params.rs`<br>`crates/pc-engine/tests/it/main.rs` | MIT OR Apache-2.0 |
| [`ec56c286ddf2`](https://github.com/storytold/photocraft/commit/ec56c286ddf21fe0b4b62ad491f31eeeed869213) | Fix mixed-order TIFF type transforms and reject unsafe geometry | `crates/pc-io/src/psd_import.rs`<br>`crates/pc-psd/src/tiff.rs`<br>`crates/pc-text/src/psd.rs`<br>`crates/pc-io/src/blocks.rs` | MIT OR Apache-2.0 |

Adapted ports: `589941a5` adds `li_fs::refuse_read_only` at the document save points (Save, Save
As, background save, engine writes) instead of inside the shared atomic writer, so Library
sidecars and catalog files keep their behaviour. `205c9c17` guards the clone, healing, spot
healing and history brushes, which all render dense coverage. `ec56c286` takes the TIFF type-block
byte-order fix and the transform check; we have no retained foreign-order blocks to skip.

Already covered or not applicable: `6cec0cab` (a test adjustment for 589941a5), `cc772ad9` (our
`li-fs` refuses to save through a dangling symlink instead of writing through it, which is the
safer choice), `713dbbed` (we have no ATN save), `bf321f0f` and `9c914a9f` (our Affinity importer
already bounds compound expansion, rejects cycles and limits nested spot colours), `e86e6910` (our
PSD writer has no `Txt2` style runs).


## VectorCraft importer hardening (2026-10-23)

| Upstream commit | Subject | Our files | Licence |
|---|---|---|---|
| [`c10da25ce4fc`](https://github.com/storytold/vectorcraft/commit/c10da25ce4fca0b1c60e3c376f2af450f85d3307) | Importers: nesting is capped, so a deeply nested file can't overflow the stack | `crates/pc-svg/src/import/preflight.rs`<br>`crates/pc-svg/tests/import/main.rs` | MIT OR Apache-2.0 |
| [`17d249361663`](https://github.com/storytold/vectorcraft/commit/17d2493616637925cc3ad8e53876b6522c0f0e52) | Importers: PDF nesting inside compressed streams no longer crashes (hayro 0.8) | `crates/pc-pdf/tests/import/hostile.rs`<br>`crates/pc-pdf/tests/import/main.rs` | MIT OR Apache-2.0 |
| [`18f8a1bdcf5d`](https://github.com/storytold/vectorcraft/commit/18f8a1bdcf5d94f8586f52943c2ab7c3bbec4d1d) | PDF import: a damaged array is refused as damaged, not as functions built on one another | `crates/pc-pdf/tests/import/hostile.rs` | MIT OR Apache-2.0 |

Adapted ports: `c10da25c` takes the text-level nesting scan that runs before any XML reader, at
our existing XML depth limit (roxmltree reads elements by recursion, so a deeply nested SVG
aborted the process before the tree check could refuse it). Reference depth is already bounded by
our resource walk, so the reference-following part and the 16 MB import thread were not needed.
`17d24934` and `18f8a1bd` port their hostile-file regression tests only: we are already on hayro
0.8, refuse calculator functions and cap resource reference chains at 64 with cycle detection.

Already covered or not applicable: `ad165a5a` (our CSS matcher already memoizes), `a2e2eba2`
(`pc-trace` already validates the buffer length), `0ad92e46` (our Affinity importer already shares
one pixel allowance), `95d27fe3`, `9e90464f`, `a69ddea9`, `991ce2c6`, `e01a65d8` (our PDF
preflight has its own bounded reader) and `830cefca` (we don't read XMP from PDFs).

## PhotoCraft and LightCraft command and GPU fixes (2026-10-23)

| Upstream commit | Subject | Our files | Licence |
|---|---|---|---|
| [`436ca3569cb2`](https://github.com/storytold/photocraft/commit/436ca3569cb2305650de7fafc9e8e13d25ed1605) | fix(gpu): clamp device limits to adapter support (#5986) (#6050) | `crates/li-gpu/src/device.rs` | MIT OR Apache-2.0 |
| [`80cd36846e51`](https://github.com/storytold/photocraft/commit/80cd36846e51e1cc44ebb04440490139141436af) | GPU canvas: redraw mip levels of 128 px or less whole; fixes AMD Vulkan corruption after partial updates (#6418) (#6438) | `crates/pc-ui-egui/src/gpu_canvas/resources.rs`<br>`crates/pc-ui-egui/src/gpu_canvas/tests.rs` | MIT OR Apache-2.0 |
| [`c6ea59461f9e`](https://github.com/storytold/photocraft/commit/c6ea59461f9e7e89f422ae75b47bf1b2d64b96d9) | Automation: doc.save rejects a bad quality instead of ignoring or clamping it (#7023) | `crates/pc-automation/src/rpc.rs`<br>`crates/pc-automation/src/server.rs`<br>`crates/pc-automation/tests/it/mcp.rs`<br>`crates/pc-automation/tests/coverage/server_coverage.rs` | MIT OR Apache-2.0 |
| [`502d055c6c8c`](https://github.com/storytold/photocraft/commit/502d055c6c8cb5d86850d02bee32a2ff1da1e400) | file.saveACopy refuses the document's own file without "overwrite" (#7437) (#7456) | `crates/pc-engine/src/file_cmds.rs`<br>`crates/pc-engine/src/file_cmds/tests.rs` | MIT OR Apache-2.0 |
| [`8b61a8b18666`](https://github.com/storytold/photocraft/commit/8b61a8b18666a2018057b0a5ad6a52aeaf3ad12d) | Control: ui.set rejects zoom, rotation, center and dockWidth beyond f32 range (#6988) | `crates/pc-ui-egui/src/control.rs` | MIT OR Apache-2.0 |
| [`ea826b11cc5b`](https://github.com/storytold/lightcraft/commit/ea826b11cc5bec456c2b71e6bc6ea59a4f91af45) | engine: refuse negative indices instead of targeting element 0 | `crates/lc-engine/src/cmd/color.rs`<br>`crates/lc-engine/src/cmd/develop.rs`<br>`crates/lc-engine/src/cmd/masks.rs`<br>`crates/lc-engine/src/cmd/mod.rs`<br>`crates/lc-engine/src/tests.rs` | MIT OR Apache-2.0 |
| [`de44200a8ddd`](https://github.com/storytold/lightcraft/commit/de44200a8dddfa2677f71a48ef978d1c8a616266) | Issue #2097: reject fractional command indices | `crates/lc-engine/src/cmd/develop.rs`<br>`crates/lc-engine/src/cmd/mod.rs`<br>`crates/lc-engine/src/tests.rs` | MIT OR Apache-2.0 |
| [`1162c2a01832`](https://github.com/storytold/lightcraft/commit/1162c2a01832cd424704f17aa9405fd48ffed445) | crop.set refuses a crop smaller than the crop tool allows, instead of exporting 1×1 (part of #1952) | `crates/lc-engine/src/cmd/develop.rs`<br>`crates/lc-engine/src/tests.rs`<br>`crates/lc-geom/src/crop.rs`<br>`crates/lc-geom/src/lib.rs`<br>`crates/lc-ui-egui/src/panels/detail.rs` | MIT OR Apache-2.0 |
| [`9ef40b98866c`](https://github.com/storytold/lightcraft/commit/9ef40b98866ca7270fa4621c83028357c7ed79b8) | Commands: Refuse malformed photo ids instead of using the selection (#1506) | `crates/lc-engine/src/cmd/browse.rs`<br>`crates/lc-engine/src/cmd/convert.rs`<br>`crates/lc-engine/src/cmd/cull.rs`<br>`crates/lc-engine/src/cmd/develop.rs`<br>`crates/lc-engine/src/cmd/keywords.rs`<br>`crates/lc-engine/src/cmd/library.rs`<br>`crates/lc-engine/src/cmd/manage.rs`<br>`crates/lc-engine/src/cmd/merge.rs`<br>`crates/lc-engine/src/cmd/mod.rs`<br>`crates/lc-engine/src/cmd/organize.rs`<br>`crates/lc-engine/src/cmd/previews.rs`<br>`crates/lc-engine/src/cmd/xmp.rs`<br>`crates/lc-engine/src/lib.rs`<br>`crates/lc-engine/src/tests.rs`<br>`crates/lc-ui-egui/src/menus.rs`<br>`crates/lc-ui-egui/src/merge.rs`<br>`crates/lc-ui-egui/src/panels/dialogs.rs`<br>`crates/lc-ui-egui/src/panels/grid.rs`<br>`crates/lc-ui-egui/src/tasks.rs` | MIT OR Apache-2.0 |

Adapted ports: `436ca356` applies the clamp in `li_gpu::device_limits`, where both editors take
their limits; only the GL base can exceed a weak adapter there. `80cd3684` ports the redraw rule and
its unit test; its GPU sweep test needs the upstream mip readback harness. `502d055c` uses
`li_fs::same_file`; its Windows file-identity change does not apply. `8b61a8b1` checks `zoom`,
`center` and `dockWidth` (our `ui.set` has no `rotation`) before anything changes. `de44200a` also
moves our `history.restore` onto the checked index. `1162c2a0` adds `MIN_CROP_FRACTION` to
`lc-geom`, shared by `crop.set` and the crop tool's drag.

Not applicable or deferred: `e14db894` (our export options fall back to the format's own depth by
design and have no strict parameter validator), VectorCraft `025ce255` and `32dbc4b6` (our PDF
exporter writes no CMYK images or output intents). The DesignCraft breaker changes since
`66c7ce7e` build on CJK composition structures `pc-text` doesn't have (mojikumi, kinsoku priority,
locked advances), so the re-sync is left for a feature port.

## Selection snapping (2026-10-26)

| Upstream commit | Subject | Our files | Licence |
|---|---|---|---|
| [storytold/photocraft@`5c039af58161a13b0472ac711b5ca0490438b305`](https://github.com/storytold/photocraft/commit/5c039af58161a13b0472ac711b5ca0490438b305) | Selection: moving a selection outline snaps its bounds, not the pointer (#2736) (#2740) | `crates/pc-ui-egui/src/snap_ui.rs` | MIT OR Apache-2.0 |
| [storytold/photocraft@`5f13491411c0f0eb76d37192c77efd46841d916b`](https://github.com/storytold/photocraft/commit/5f13491411c0f0eb76d37192c77efd46841d916b) | Fix Fixed Size marquee positioning while dragging | `crates/pc-ui-egui/src/canvas.rs`, `crates/pc-ui-egui/src/marquee_tests.rs`; `crates/pc-ui-egui/src/snap_ui.rs` adapted in `57db1c64` | MIT OR Apache-2.0 |

Local adaptation `488d2e4c` uses selection bounds for outline drags and retains contained
objects as snap targets. Lasso outline drags now snap too; Lasso was not a snapping tool on main.

## Perceptual gradients (2026-10-26)

| Upstream commit | Subject | Our files | Licence |
|---|---|---|---|
| [`2bad6468213d`](https://github.com/storytold/vectorcraft/commit/2bad6468213df3250963f69a5d6488af2868725e) | Gradients: a Perceptual gradient mixes its colours in OKLab, and SVG and PDF write it as an image | `crates/pc-doc/src/effects.rs` (native methods), `crates/pc-doc/src/lib.rs` (method persistence), `crates/pc-doc/tests/coverage/doc_model_coverage.rs` (persistence regression), `crates/pc-vector/src/gradient_method.rs` (OKLab mixing, non-finite RGB guard and bounded expansion), `crates/pc-vector/src/gradient_method/tests.rs` (mixing and expansion regressions), `crates/pc-vector/src/lib.rs` (method-aware paints), `crates/pc-vector/src/tests.rs` (shared sampler adaptation), `crates/pc-compose/src/gradient_fill.rs` (method-aware ramps), `crates/pc-compose/src/gradient_fill/tests_method.rs` (shape/fill parity), `crates/pc-compose/src/shape_split.rs` (method cache identity and regression), `crates/pc-engine/src/gradient_fill_cmds.rs` (method preservation and description), `crates/pc-export-common/src/gradient.rs` (bounded vector expansion and regression), `crates/pc-svg/tests/export/main.rs` (Perceptual vector export), `crates/pc-pdf/tests/export/main.rs` (Perceptual vector export), `crates/pc-gpu/tests/gpu/parity.rs` (parity module registration), `crates/pc-gpu/tests/gpu/parity/gradient_methods.rs` (CPU/GPU parity), `crates/pc-io/src/blocks.rs` (editable Perceptual stops and round-trip regressions) | MIT OR Apache-2.0 |
| [`9237a8bdde35`](https://github.com/storytold/photocraft/commit/9237a8bdde35f5365154b9eaf713bd5bb66fe0da) | PSD gradients: read the interpolation method Photoshop writes; add Smooth (#3442) | `crates/pc-io/src/blocks.rs` (current-key precedence, Smooth baking and regressions), `crates/pc-io/src/effects_map.rs` (current method key for effects), `crates/pc-io/src/gradient_bake.rs` (baked Smooth easing, measured Smooth test and module documentation) | MIT OR Apache-2.0 |
| [`78ae50f7b2c4`](https://github.com/storytold/photocraft/commit/78ae50f7b2c470a062506cce8ddb429db802c606) | security: bisect flat-table and gradient-stop lookups; charge ICC curve samples; bound PSD planes, RAW ljpeg retention and TIFF dump preview | `crates/pc-vector/src/gradient_method.rs` (colour-stop bisection), `crates/pc-vector/src/lib.rs` (opacity-stop bisection), `crates/pc-vector/src/gradient_method/tests.rs` (bisection and hard-stop regressions), `crates/pc-vector/src/tests.rs` (shared bisected sampler adaptation), `crates/pc-compose/src/gradient_fill.rs` (shared bisected ramp lookup) | MIT OR Apache-2.0 |
| [`6beb68f934c4`](https://github.com/storytold/vectorcraft/commit/6beb68f934c49f792e2d05560c692e04b3c1af98) | Gradients: the Gradient panel's Method menu and paint.editGradient's `method` choose Classic or Perceptual | `crates/pc-engine/src/gradient_fill_cmds.rs` (live method commands), `crates/pc-engine/src/gradient_fill_cmds/tests.rs` (command regressions), `crates/pc-engine/src/paint_cmds.rs` (destructive method and validation), `crates/pc-engine/src/vector_cmds.rs` (shape paint method), `crates/pc-ui-egui/src/canvas/tools.rs` (drag method), `crates/pc-ui-egui/src/gradient_ui.rs` (Properties and previews), `crates/pc-ui-egui/src/gradient_ui/method_tests.rs` (method controls and drags), `crates/pc-ui-egui/src/i18n_tests.rs` (Method label coverage), `crates/pc-ui-egui/src/panels/options_bar.rs` (Method control), `crates/pc-ui-egui/src/state.rs` (Classic tool default), `crates/pc-ui-egui/src/vector_packages/appearance.rs` (Method control), `crates/pc-ui-egui/src/vector_packages/appearance/method_tests.rs` (keyboard and undo regression) | MIT OR Apache-2.0 |

These four commits form the Perceptual gradients feature port under decision D10: upstream's two
native methods only, Classic and Perceptual. Local adaptations keep SVG and PDF vector and fail
expansion rather than truncating; take only gradient colour and opacity bisection from 78ae50f7,
current-key precedence and baked Smooth from 9237a8bd, and place 6beb68f9 on Local Image's existing
surfaces and command names. Unsupported Linear appearances retain their existing baking.
The literal/pattern-only method-field adaptations in dc209948 are local: `crates/pc-affinity/src/import/paint.rs`,
`crates/pc-compose/src/tests.rs`, `crates/pc-engine/src/vector_packages/brushes/tests.rs`,
`crates/pc-engine/tests/coverage/gradient_fill_cmds_coverage.rs`, `crates/pc-export-common/cases/clipping.rs`,
`crates/pc-format/tests/it/common/mod.rs`, `crates/pc-pdf/src/import/paint.rs`,
`crates/pc-svg/src/import/paint.rs`, `crates/pc-svg/tests/export/roundtrip.rs`,
`crates/pc-ui-egui/src/vector_packages/appearance.rs`, `crates/pc-ui-egui/tests/gpu/vector_integration.rs`
and `crates/pc-vector/src/profile_tests.rs`.
The Smooth measurements retain upstream's black-to-white and red-to-blue rows and two-level tolerance.

## PhotoCraft PSD export safety (2026-10-30)

| Upstream commit | Subject | Our files | Licence |
|---|---|---|---|
| [`a8e5cb3e8e79bad623f9177ee21614f4dca7a4ad`](https://github.com/storytold/photocraft/commit/a8e5cb3e8e79bad623f9177ee21614f4dca7a4ad) | PSD: drop a kept merged-transparency marker when the composite is opaque | `crates/pc-io/src/psd_export.rs`<br>`crates/pc-io/src/psd_export/tests.rs` | MIT OR Apache-2.0 |

The opaque-composite export drops preserved `MTrn`, `Mtrn`, `Mt16` and `Mt32` markers, while alpha-bearing composites retain them. The layer-info block placement follows any retained marker.

## PhotoCraft Affinity importer bounds (2026-10-30)

| Upstream commit | Subject | Our files | Licence |
|---|---|---|---|
| [`b278ff72459cbf9754863365edd822ba2248aa07`](https://github.com/storytold/photocraft/commit/b278ff72459cbf9754863365edd822ba2248aa07) | security(affinity): charge archive names, failed reads, corner arrays, attachments, pages and font names (#9257) | `crates/pc-affinity/src/container.rs`<br>`crates/pc-affinity/src/geometry.rs`<br>`crates/pc-affinity/src/model.rs`<br>`crates/pc-affinity/src/paint.rs`<br>`crates/pc-affinity/src/raster.rs`<br>`crates/pc-affinity/src/tests.rs`<br>`crates/pc-affinity/src/text.rs`<br>`crates/pc-affinity/tests/scene/af5.rs`<br>`crates/pc-affinity/tests/support/text_fixture.rs` | MIT OR Apache-2.0 |

Already covered here: failed archive reads spend the extraction allowance before decoding, attached-mask arrays use `Reader::charge_slots`, and that slot-budget method is already crate-visible. The port adds aggregate archive-name and page limits, a document-wide font-name budget, and accounting for curve-corner, gradient and pixel-state scans. The 16 MiB font-name budget covers every name the model keeps, including each table cell's copy; the imported scene has the same cap on its own copies, which covers repeated master pages. Names past either cap are left empty with a warning. Curve-corner work uses the existing `OutlineBudget`. The existing 16 KiB per-name font limit remains: reducing it to 256 bytes conflicts with existing text tests, so that upstream reduction is flagged for human decision.

## PhotoCraft Vanishing Point canvas clamp (2026-10-30)

| Upstream commit | Subject | Our files | Licence |
|---|---|---|---|
| [`248972cfa343ef46cee8f06ea16173310b2abba2`](https://github.com/storytold/photocraft/commit/248972cfa343ef46cee8f06ea16173310b2abba2) | Integration: clip VP clone copy-back to the canvas, paste in the stamp preview (#3281) | `crates/pc-engine/src/vp_cmds.rs` | MIT OR Apache-2.0 |
| [`0f8521851eda61b05f9c5a1201e313e2cbb3a199`](https://github.com/storytold/photocraft/commit/0f8521851eda61b05f9c5a1201e313e2cbb3a199) | Integration: Vanishing Point clone dabs stay on the canvas | `crates/pc-algo/src/vanishing.rs`<br>`crates/pc-algo/tests/coverage/vanishing_coverage.rs`<br>`crates/pc-engine/src/vp_cmds.rs` | MIT OR Apache-2.0 |

Only canvas clamping is ported: dab rectangles are clipped before surface access, and command copy-back is clipped to the document bounds. The stamp-preview and paste changes are not included.

## PhotoCraft package links (2026-10-30)

| Upstream commit | Subject | Our files | Licence |
|---|---|---|---|
| [`1e5b5f76170e7fdfc5cdc8aaa1a9ecfc6e2ca81f`](https://github.com/storytold/photocraft/commit/1e5b5f76170e7fdfc5cdc8aaa1a9ecfc6e2ca81f) | Package: never overwrite one link's copy with another, list a missing link once (#9185) | `crates/pc-engine/src/print_cmds.rs`<br>`crates/pc-engine/src/print_cmds/tests.rs` | MIT OR Apache-2.0 |

Package link names are unique case-insensitively across both the current run and files already in `Links`; repeated missing paths are reported once. Contributor metadata was not part of this code port.

## PhotoCraft native manifest input bound (2026-10-30)

| Upstream commit | Subject | Our files | Licence |
|---|---|---|---|
| [`92a15591381b0967d6276ff2e8b65a8bbd968a18`](https://github.com/storytold/photocraft/commit/92a15591381b0967d6276ff2e8b65a8bbd968a18) | Refuse .pcraft manifests with too many JSON values before parsing (#8893) | `crates/pc-format/src/store.rs`<br>`crates/pc-format/tests/it/corruption.rs` | MIT OR Apache-2.0 |

The existing linear manifest scan now caps the counts of opened arrays/objects and comma-separated values at 4,194,304 before deserializing the JSON tree. The same scanner protects serialized manifests on save.

## PhotoCraft data-set export filenames (2026-10-30)

| Upstream commit | Subject | Our files | Licence |
|---|---|---|---|
| [`2c4982f2d2981c1685212dd951d7466e4b39fe49`](https://github.com/storytold/photocraft/commit/2c4982f2d2981c1685212dd951d7466e4b39fe49) | Export Data Sets as Files keeps outputs in the chosen folder (#9420) | `crates/pc-engine/src/variables_cmds.rs`<br>`crates/pc-engine/src/variables_cmds/tests.rs`<br>`crates/pc-engine/src/web_cmds.rs` | MIT OR Apache-2.0 |

Document-derived stems replace path separators, controls and reserved filename characters; empty, `.` and `..` stems become `_`. Each template path component also uses the existing Windows device-name handling from the web asset exporter, preserving file extensions.

## PhotoCraft parser hardening (2026-10-30)

| Upstream commit | Subject | Our files | Licence |
|---|---|---|---|
| [`1121b7979bc6`](https://github.com/storytold/photocraft/commit/1121b7979bc61f398619c2951c9ece1fb8c8fc08) | fix(format): bound ZIP expansion by declared size | `crates/pc-format/src/zip.rs` | MIT OR Apache-2.0 |
| [`8f782cab1cf9`](https://github.com/storytold/photocraft/commit/8f782cab1cf98fecca7af994ea574cc547e07b8c) | fix(codecs): bound complete JPEGs by their scan data too | `crates/pc-codecs/src/codecs/jpeg.rs` | MIT OR Apache-2.0 |
| [`68c0f034d507`](https://github.com/storytold/photocraft/commit/68c0f034d507d104a3aff2ed46f01bd800a0af6a) | Plug-ins: refuse a code section with no functions but many bytes instead of panicking (#9679) | `crates/pc-plugins/src/runtime.rs`, `crates/pc-plugins/tests/abi.rs` | MIT OR Apache-2.0 |

Local adaptations: 8f782cab1 edits a JPEG frame-size guard from c9892c8f8 and 07f91c6d3, which are not
ported. Local Image adds only the guard on files whose walk reached an EOI, so a truncated JPEG still
opens with its warning. Where upstream counts the unread tail of a walk stopped by an odd layout, Local
Image leaves that file unmeasured (`DataEnd::Unread`), so it opens as before the port, whether or not an
EOI follows. A stray EOI inside a scan counts as the end, so a damaged file can fail the guard, as upstream.
Scan bytes are counted in the existing `scan_metadata` walker instead of upstream's `data_end`, and that
walker keeps skipping stray bytes before the first scan, so header padding never counts. The ZIP reader
keeps `flate2::read::DeflateDecoder`; upstream's `bufread` decoder comes from 51788e665, which is not ported.

## LightCraft HEIF hardening (2026-10-30)

| Upstream commit | Subject | Our files | Licence |
|---|---|---|---|
| [`b88f72b9df68`](https://github.com/storytold/lightcraft/commit/b88f72b9df68f07a080772d7fb16a92d6f7db837) | sec: bound heic-rs 0.1.4 work around hostile HEIF containers | `crates/pc-heif/src/census.rs`, `crates/pc-heif/src/lib.rs`, `crates/pc-heif/src/tests_hardening.rs` | MIT OR Apache-2.0 |
| [`178eef790bd8`](https://github.com/storytold/lightcraft/commit/178eef790bd8872a6f60f7b1a0fb7b67499acc59) | heif: bound iloc item-extent visits before reconstructing in probe | `crates/pc-heif/src/lib.rs`, `crates/pc-heif/src/tests_hardening.rs` | MIT OR Apache-2.0 |
| [`269495cd3867`](https://github.com/storytold/lightcraft/commit/269495cd3867e9feee1876ac641627ba23c7dabb) | security(heif): budget every reconstruction before it runs | `crates/pc-heif/src/lib.rs`, `crates/pc-heif/src/tests_hardening.rs` | MIT OR Apache-2.0 |
| [`0c71f75325b5`](https://github.com/storytold/lightcraft/commit/0c71f75325b5f707bbe8e7ef6324e6e14a8f51e6) | security(heif): scan the primary item before heic_rs::probe, the alpha plane, and each thumbnail candidate | `crates/pc-heif/src/lib.rs`, `crates/pc-heif/src/tests_hardening.rs` | MIT OR Apache-2.0 |

Partial port into the pinned `heic-rs = 0.1.4` wrapper: `ipma` payloads are capped at 64 KiB in
aggregate, `iloc` item and extent records at 65,536 each, item extent visits at 4096, coded-item
reconstruction at the file size, transform properties at 12, and output samples at 2³⁰ even when a
caller raises `max_pixels`.
The wrapper has no thumbnail API, so thumbnail-candidate changes do not apply. `heic-rs` already
checks a grid's actual canvas against the decode limit before allocating the output image.

Local extension: alpha-grid tile extent chains share the 4096-read budget with primary reconstruction.
Local extension: `iloc` record counts replace upstream's 64 KiB `iloc` payload cap, which is not
ported: zero-width extents cost no bytes, so payload size did not price the allocation. The census
walks every `iloc` box as `heic-rs` 0.1.4 parses it and totals the records across all of them, so a
list cut short or using unknown field widths still pays for every record `heic-rs` builds from it.
`Context::open` therefore builds at most 65,536 item records and 65,536 extent records; a file
holding more is refused first. Re-check the census against `heic-rs`'s `iloc` parser when the pin
moves.

## Path operation budgets (2026-10-30)

| Upstream commit | Subject | Our files | Licence |
|---|---|---|---|
| [`cd80d1d859cd`](https://github.com/storytold/vectorcraft/commit/cd80d1d859cdefb4f095c362c52c9cae63f41f62) | Security: planar map budgets for booleans, Live Paint, Offset Path and 3D effects | `crates/pc-pathops/src/budget.rs`<br>`crates/pc-pathops/src/boolean.rs`<br>`crates/pc-pathops/src/lib.rs` (error variant)<br>`crates/pc-pathops/src/planar.rs` | MIT OR Apache-2.0 |
| [`0eebd533c58c`](https://github.com/storytold/vectorcraft/commit/0eebd533c58c357a1b5b09769b6890066b05d80e) | security(pathops): signed zeros, reported budget rejections, and a fill-only fallback | `crates/pc-pathops/src/budget.rs` (signed-zero bounds)<br>`crates/pc-engine/src/pathops_cmds.rs` (command-boundary regression) | MIT OR Apache-2.0 |
| [`caf10cc5816d`](https://github.com/storytold/vectorcraft/commit/caf10cc5816d5f26bee02837650ce6952b69b878) | security(pathops): count the sweep budget on the snapped geometry | `crates/pc-pathops/src/boolean.rs`<br>`crates/pc-pathops/src/planar.rs` | MIT OR Apache-2.0 |

Adapted ports: `cd80d1d8` counts overlapping segment bounds before every arrangement sweep and
the Live Paint map sweep, and stores Live Paint crossing counts sparsely. In Local Image the
arrangement already snaps horizontals before sweeping, so the shared preflight runs after that
snap and also covers snap-round and flatten retries (`caf10cc5`). `0eebd533` canonicalizes signed
zero in bounds; existing fallible APIs and `pathops_cmds` error propagation surface the refusal
before editing, verified by the command regression. Its `try_boolean_n` and `try_area` additions
are unnecessary here: no workspace caller uses the infallible N-ary helper, and the document
adapter already returns errors for boolean and area operations. The fill-only Shape Builder
fallback is not used because Local Image's checked builder contract reports incomplete geometry
instead of silently dropping cutting lines.
The gap grouping (`gap_from`) and 3D-effect hunks of `cd80d1d8` are not ported: neither piece of
code exists in this tree.
Local addition: `WorkBudget` caps the pairs of all sweeps in one path operation at four sweep
limits (16,777,216). Upstream limits each sweep only, which leaves a many-component path to run
every sweep up to the limit. `check_sweep_size` in `planar.rs` is also local: a trial sweep that
stops a planar map or any arrangement once it would add too many edges or winding values, so it
covers booleans, normalize, offset, `finish_compound` and Pathfinder Unite as well. Art with too
few segments close together to reach those limits skips it. `RefusalWatch` is local too: it lets
the canvas skip a refused effect and lets output fail naming the layer, without sharing a budget.
The trial sweep counts only edges a sweep adds, so art that nothing cuts passes it. The pair limit
and the operation budget count overlapping bounds instead, so they do refuse art that never
crosses: 730 concentric circles in one component, or 2,400 circles held as separate components.

## RAW and metadata memory bounds (2026-10-30)

| Upstream commit | Subject | Our files | Licence |
|---|---|---|---|
| [storytold/lightcraft@`d00df6319cd382b67c25f2d4bd540b87261318eb`](https://github.com/storytold/lightcraft/commit/d00df6319cd382b67c25f2d4bd540b87261318eb) | security/raw: bound hostile-memory paths in raw decoders | `crates/lc-raw/src/dngwrite.rs`<br>`crates/lc-raw/src/opcodes.rs`<br>`crates/lc-raw/src/preview.rs`<br>`crates/lc-raw/src/tiffraw.rs`<br>`crates/lc-raw/tests/it/dng.rs`<br>`crates/lc-tiff/src/image.rs` | MIT OR Apache-2.0 |
| [storytold/lightcraft@`d8ceb3a6af2b5bd86fdcb947eb8a4f5b1c372125`](https://github.com/storytold/lightcraft/commit/d8ceb3a6af2b5bd86fdcb947eb8a4f5b1c372125) | security/meta,merge,raw,tiff,denoise: bound memory sized by attacker-shaped inputs | `crates/lc-meta/src/xmp.rs`<br>`crates/lc-tiff/src/makernote.rs`<br>`crates/lc-merge/src/lib.rs`<br>`crates/lc-merge/src/pano/camera.rs`<br>`crates/lc-merge/src/pano/mod.rs`<br>`crates/lc-merge/tests/it/pano_coverage.rs`<br>`crates/lc-engine/src/merge.rs`<br>`crates/lc-engine/src/sidecar.rs`<br>`crates/lc-engine/src/preset_import.rs`<br>`crates/lc-engine/src/crs.rs`<br>`crates/lc-meta/src/xmp_merge.rs`<br>`crates/lc-meta/src/file_info.rs`<br>`crates/lc-meta/src/exif.rs`<br>`crates/lc-meta/src/lib.rs`<br>`crates/lc-tiff/src/reader.rs`<br>`crates/pc-engine/src/vector_packages/pdf_export.rs` | MIT OR Apache-2.0 |
| [storytold/lightcraft@`3022baa8615a2956e239b7febf3fe4ff98289885`](https://github.com/storytold/lightcraft/commit/3022baa8615a2956e239b7febf3fe4ff98289885) | security(raw,meta): follow-ups from review | `crates/lc-raw/src/preview.rs`<br>`crates/lc-raw/src/tiffraw.rs` | MIT OR Apache-2.0 |
| [storytold/lightcraft@`ca69e74b62c819e4dee07fb50eb674ef39b8557a`](https://github.com/storytold/lightcraft/commit/ca69e74b62c819e4dee07fb50eb674ef39b8557a) | fix(raw): check opcode byte counts without overflow | `crates/lc-raw/src/opcodes.rs` | MIT OR Apache-2.0 |
| [storytold/photocraft@`dcf5c61a64716bec507ee79735ef18ce0837fbce`](https://github.com/storytold/photocraft/commit/dcf5c61a64716bec507ee79735ef18ce0837fbce) | Integration: cap MapPolynomial opcodes, finite WhiteLevel, RGB-only JPEG XL preview (#8402) | `crates/lc-raw/src/dng.rs`<br>`crates/lc-raw/src/opcodes.rs` | MIT OR Apache-2.0 |

Adapted ports: `d00df631` uses `ImageInfo::single_chunk` for TIFF previews, which builds its one chunk itself rather than calling `chunks()` so a long offset table is not cloned, retains the chunk-scoped decoder budget already used by raw imports, checks Deflate plausibility against the in-image span while keeping inflation bounded by the nominal tile, and charges `WarpFisheye`, `FixVignetteRadial` and the binned `GainMap` to the `OpcodeList3` pass cap, each being a full-image pass here; `d8ceb3a6` reports omissions from the 1 MiB flattened-key retention budget through `XmpData::property_keys_truncated`, which preset imports log, while retaining the existing 64 MiB `TooComplex` ceiling, keeps that budget only on `parse_xmp`, whose remaining callers (presets, `photo.allMetadata` and the validation passes in `xmp_merge`, which discard the result) decide nothing a catalog, sidecar, save or export keeps, parses everything else with `parse_whole_xmp`, which stops only at that ceiling, so `extract`, `try_extract`, `from_tiff`, `parse_sidecar`, `merge_xmp`'s read-back, `copyright_xmp`, File Info, PDF File Info and a DNG's Exif capture time lose nothing to the retention budget, has `try_extract` report a packet past that ceiling so Remove Location Info refuses what it cannot read, and lc-engine asks `pano::check_frame_count` before it loads panorama frames; `3022baa8` skips a CR3 candidate that exceeds the remaining scan budget so a later smaller box can still be checked, and its tile-grid check lives in `check_image` in `crates/lc-raw/src/tiffraw.rs` rather than `read_image`, so header-only reads and Sony's `read_quad_tiles` and `read_ycbcr_tiles` refuse the same grids. `dcf5c61a` keeps a `MapPolynomial` past the cap as `Opcode::Unknown` with empty parameters, as `OpcodeLists` has no `skipped` record: it is not applied, and DNG export writes it back with its flags and no coefficients. The `log` dependency in `crates/lc-tiff` and `crates/lc-raw` carries one warning per read for each cap that drops data: `warn_capped` for polynomials and `OpcodeList3` passes, `parse_makernote` for maker-note entries, and `cr3_preview_boxes` for skipped candidates. `is_new_block` keeps one read that parses a maker note several times to one line. The `dcf5c61a` JPEG XL preview filter is out of scope.

Already covered: `d00df631`'s lossy-JPEG size ceiling by the tile-bounded JPEG header (`crates/lc-raw/src/tiffraw.rs:491`), and `d8ceb3a6`'s rows-sized lossy buffer by `chunk_working_bytes` (`tiffraw.rs:206`). `d8ceb3a6`'s chunk grouping and `3022baa8`'s saturating grouping by `chunk_batches` (`tiffraw.rs:227`, its overflow-free comparison at `tiffraw.rs:236`), which bounds live memory; total decode work is bounded by the tile-grid check (`tiffraw.rs:97`). `3022baa8`'s document-wide flattened-key budget by the parse-wide budget (`crates/lc-meta/src/xmp.rs:539`). `d8ceb3a6`'s test-only `crates/tiff/src/reader.rs` hunk by `maker_note_values_stay_within_a_fixed_budget` in `crates/lc-tiff/src/makernote.rs`.

Not applicable, the upstream file having no counterpart here as of `6f6d8ed8`: `d00df631`'s and `3022baa8`'s `crates/raw/src/sparse.rs` (no sparse reader), `d00df631`'s `crates/raw/src/vendor/mef.rs` (no MEF decoder) and `crates/raw/src/wb_estimate.rs`, and `d8ceb3a6`'s `crates/meta/src/focus_read.rs` (no focus reader), `crates/denoise/src/reference.rs` and `crates/denoise-core/src/product.rs`. `d8ceb3a6`'s `try_read_exif_with` in `crates/meta/src/exif.rs` exists upstream for that focus reader and has no caller here.

Local limit on a ported guard: the refusal of an Original export under Remove Location Info from `db7e3784` (`crates/lc-engine/src/export/prepared.rs`) only trusts the containers `reads_container` allows (`crates/lc-meta/src/container.rs`). Everything else is refused, including HEIC, HEIF, HIF and JPEG XL, which have no metadata reader here yet. Within those containers `try_extract` refuses a malformed XMP packet in any of them, and any block `Embedded::unread` names: compressed or `zTXt`/`tEXt` PNG XMP, a raw metadata profile, a second Exif block or XMP packet, a cut-off block, and XMP that is not UTF-8. JPEG extended XMP is read, and `Metadata::other_location` adds the location fields `Metadata` has no field for. A local adaptation with no upstream match. This is best-effort, not a guarantee that a copied file holds no location: corrupt Exif outside PSD/PSB reads as no Exif (`try_extract_keeps_non_psd_exif_best_effort`), so a location inside it goes unseen, and nothing looks at JPEG data past the first scan, IPTC past the first APP13 block that holds it, PSD resources other than Exif and XMP, XMP or IPTC outside TIFF IFD0, CR3 preview boxes, or PNG text chunks under other keywords.

## PhotoCraft document files, text and importer budgets (2026-11-15)

| Upstream commit | Subject | Our files | Licence |
|---|---|---|---|
| [storytold/photocraft@`bf162c2215e84f7f96f4894bc7fa2008c45ef4d8`](https://github.com/storytold/photocraft/commit/bf162c2215e84f7f96f4894bc7fa2008c45ef4d8) | automation: save_bytes never reads linked files (#11233) | `crates/pc-automation/src/files.rs`<br>`crates/pc-automation/src/headless.rs`<br>`crates/pc-automation/tests/it/linked_save_bytes.rs`<br>`crates/pc-automation/tests/it/main.rs` | MIT OR Apache-2.0 |
| [storytold/photocraft@`7426cb9858315123eaedb702d2c20d5b645be348`](https://github.com/storytold/photocraft/commit/7426cb9858315123eaedb702d2c20d5b645be348) | fix(format): a .pcraft load's decode budget follows the machine (#8595) | `crates/pc-format/Cargo.toml`<br>`crates/pc-format/src/lib.rs`<br>`crates/pc-format/src/store.rs`<br>`crates/pc-format/tests/it/corruption.rs` | MIT OR Apache-2.0 |
| [storytold/photocraft@`a1d656a0f0061cbe3fd6e094aadd54da933054b4`](https://github.com/storytold/photocraft/commit/a1d656a0f0061cbe3fd6e094aadd54da933054b4) | fix(text): write raw EngineData strings back as the same bytes | `crates/pc-text/src/engine_data.rs` | MIT OR Apache-2.0 |
| [storytold/photocraft@`d51e4bce94d89f4e208f669223cf5050a5d70660`](https://github.com/storytold/photocraft/commit/d51e4bce94d89f4e208f669223cf5050a5d70660) | text: charge the rasterizer's output tiles, not only its pixels (#11047) | `crates/pc-text/src/render.rs` | MIT OR Apache-2.0 |
| [storytold/photocraft@`209bce8d4671708364842ff0ff5cda76d3153fc3`](https://github.com/storytold/photocraft/commit/209bce8d4671708364842ff0ff5cda76d3153fc3) | Budget PSD pattern expansion and bound multichannel conversion (#10541) | `crates/pc-io/src/multichannel_map.rs`<br>`crates/pc-io/src/psd_import.rs` | MIT OR Apache-2.0 |
| [storytold/photocraft@`9779daf3577eb1d831fa078e3094a7c70b7b23cf`](https://github.com/storytold/photocraft/commit/9779daf3577eb1d831fa078e3094a7c70b7b23cf) | security(affinity): charge allocation records, empty subpaths, paint lists, tile indexes and shape lookups (#10966) | `crates/pc-affinity/src/container.rs`<br>`crates/pc-affinity/src/paint.rs`<br>`crates/pc-affinity/src/shapes.rs` | MIT OR Apache-2.0 |

## Upstream commit ports (2026-11-15)

| Upstream commit | Subject | Our files | Licence |
|---|---|---|---|
| [`4bbeaaa560c4`](https://github.com/storytold/lightcraft/commit/4bbeaaa560c4f880b3f8b5a60cccd0ef68c52f1b) | LR-EXP-LOCATION: an export subfolder never leaves the destination folder (#2571) | `crates/lc-engine/src/export.rs`<br>`crates/lc-engine/src/export/options.rs`<br>`crates/lc-engine/src/tests_export.rs` | MIT OR Apache-2.0 |
| [`a93172c2f7c6`](https://github.com/storytold/lightcraft/commit/a93172c2f7c6b570669281c6b106208463cf46f2) | security(engine): Find Missing Photos hashes only regular files, no further than their size (#3704) | `crates/lc-engine/src/cmd/missing.rs` | MIT OR Apache-2.0 |
| [`081190c92f4f`](https://github.com/storytold/lightcraft/commit/081190c92f4f74340a7fc4125fce1fe6d2b60ed9) | Match renamed missing files by content when size is unknown (#3852) | `crates/lc-engine/src/cmd/missing.rs`<br>`crates/lc-engine/src/tests_import.rs` | MIT OR Apache-2.0 |
| [`9eb7b25e317b`](https://github.com/storytold/lightcraft/commit/9eb7b25e317b6e66beb6b9c8f862da73f3de0aae) | security(engine): sidecar copies, backups and version checks read regular files only (#3677) | `crates/lc-engine/src/rename.rs`<br>`crates/lc-engine/src/import_move.rs`<br>`crates/lc-engine/src/import_move/tests.rs`<br>`crates/lc-engine/src/sidecar.rs`<br>`crates/lc-engine/src/tests_xmp.rs` | MIT OR Apache-2.0 |
| [`7e409a3706ec`](https://github.com/storytold/lightcraft/commit/7e409a3706eca1007eb774213d91f42b8ba0e1ed) | security(engine): one unpack budget across a preset zip's packs; linear comment skipping; small zip errors (#3675) | `crates/lc-engine/src/preset_import.rs`<br>`crates/lc-engine/src/preset_luminar.rs` | MIT OR Apache-2.0 |
| [`0d2dd9a97413`](https://github.com/storytold/lightcraft/commit/0d2dd9a974134877f3930c2efc61e58ca99bde89) | A library's files were written by path, so a path that came to lead somewhere else (a share mounted over the folder, the folder moved away and another put in its place) made the session write into that other folder: its catalog.snap and catalog.log were replaced, and dropping the lock removed the owner note of the program that had the library open. | `crates/lc-catalog/src/opened_folder.rs`<br>`crates/lc-catalog/src/lock.rs`<br>`crates/lc-catalog/src/store.rs`<br>`crates/lc-catalog/src/lib.rs`<br>`crates/lc-catalog/src/tests_opened_folder.rs`<br>`crates/lc-engine/src/library.rs`<br>`crates/lc-engine/src/tests_library.rs`<br>`crates/lc-preview/src/disk.rs`<br>`crates/lc-preview/src/lib.rs`<br>`crates/lc-preview/src/tests.rs` | MIT OR Apache-2.0 |
| [`690f6cfb5857`](https://github.com/storytold/lightcraft/commit/690f6cfb5857c90719a0898dc102998558bfaef9) | fix(catalog): verify staging identity after streamed writes | `crates/lc-catalog/src/store.rs`<br>`crates/lc-catalog/src/tests_opened_folder.rs` | MIT OR Apache-2.0 |
| [`19985a3f19ad`](https://github.com/storytold/lightcraft/commit/19985a3f19adc3a91eb5cf51e61e9f03a4fe51e8) | fix(catalog): reopen fallback appends in the accepted folder | `crates/lc-catalog/src/store.rs`<br>`crates/lc-catalog/src/tests_opened_folder.rs` | MIT OR Apache-2.0 |
| [`e689453e578a`](https://github.com/storytold/lightcraft/commit/e689453e578a9f3dd4cb3cbec69cb83957f4a4b0) | fix(catalog): refuse gaps in synchronous fallback logs | `crates/lc-catalog/src/store.rs`<br>`crates/lc-catalog/src/tests_opened_folder.rs` | MIT OR Apache-2.0 |
| [`5c5981898c57`](https://github.com/storytold/lightcraft/commit/5c5981898c57114fd48c95b6df5a437da91a26f2) | fix(catalog): bind log recovery to observed files | `crates/lc-catalog/src/store.rs`<br>`crates/lc-catalog/src/tests_opened_folder.rs` | MIT OR Apache-2.0 |
| [`d7372407bb5a`](https://github.com/storytold/lightcraft/commit/d7372407bb5a335ad97575f2ab8b7105fda8cdf4) | fix(catalog): make held caller logs writable for rollback | `crates/lc-catalog/src/store.rs`<br>`crates/lc-catalog/src/tests_opened_folder.rs` | MIT OR Apache-2.0 |
| [`1aea62dde04c`](https://github.com/storytold/lightcraft/commit/1aea62dde04c96912b690f6ed481e166870f81b6) | LC-SAFETY: Refuse to create a library photo's XMP sidecar | `crates/lc-engine/src/originals.rs` | MIT OR Apache-2.0 |
| [`d39550486e93`](https://github.com/storytold/lightcraft/commit/d39550486e93f8880f1ed8edb47767344434dd3a) | LRC-LIB-FOLDERS: folder.rename and folder.move refuse the home folder and its standard places | `crates/lc-engine/src/cmd/browse.rs` | MIT OR Apache-2.0 |
| [`9b4c38fd7fa0`](https://github.com/storytold/lightcraft/commit/9b4c38fd7fa0581248f3b31d7b477c6df55a7f7c) | LRC-LIB-FOLDERS: the protected-places check reads relative paths, linked parents and volume case | `crates/lc-engine/src/cmd/browse.rs` | MIT OR Apache-2.0 |
| [`d0a64f8e3043`](https://github.com/storytold/lightcraft/commit/d0a64f8e3043531ee82fe13dc56a4ad6564379ca) | LRC-LIB-FOLDERS: the protected-places check also compares the folder itself | `crates/lc-engine/src/cmd/browse.rs` | MIT OR Apache-2.0 |
| [`1c5cbce83520`](https://github.com/storytold/lightcraft/commit/1c5cbce83520d36cf77003f8bc1f70e718689645) | security(engine): the library folder is protected from folder.move however its path is spelled (#3676) | `crates/lc-engine/src/cmd/browse.rs`<br>`crates/lc-engine/src/tests_import.rs` | MIT OR Apache-2.0 |
| [`f618fc06a755`](https://github.com/storytold/lightcraft/commit/f618fc06a7551156d7c249eda5ab4dae20c99e26) | security: staging and probe files are created new, never written through an existing entry (#3673) | `crates/lc-engine/src/smart.rs`<br>`crates/lc-engine/src/cmd/lut_profiles.rs` | MIT OR Apache-2.0 |
| [`d6ae2e9e7e77`](https://github.com/storytold/lightcraft/commit/d6ae2e9e7e7717a58dad8b160d19e81945d26486) | security(engine): read user-named files through the opened regular file under a byte cap (#3703) | `crates/lc-engine/src/cmd/manage.rs`<br>`crates/lc-engine/src/export/watermark.rs`<br>`crates/lc-engine/src/tests_libops.rs` | MIT OR Apache-2.0 |
| [`49c9d4e0ca54`](https://github.com/storytold/lightcraft/commit/49c9d4e0ca545bb4c5df6a36d66b80791a776f55) | security: bound token expansion, sanitize the name fallback, regular-file sidecar reads, recased sidecar guard | `crates/lc-engine/src/export/options.rs`<br>`crates/lc-engine/src/export/tests.rs`<br>`crates/lc-engine/src/rename.rs`<br>`crates/lc-engine/src/sidecar.rs`<br>`crates/lc-engine/src/originals.rs` | MIT OR Apache-2.0 |
| [`0d88706ec551`](https://github.com/storytold/lightcraft/commit/0d88706ec55147cb7a7e481bc706ba31e29003ca) | security(meta): bound namespace declarations in scope while parsing XMP | `crates/lc-meta/src/xmp.rs`<br>`crates/lc-meta/src/xmp_merge.rs`<br>`crates/lc-engine/src/sidecar.rs` | MIT OR Apache-2.0 |
| [`be7d68167357`](https://github.com/storytold/photocraft/commit/be7d68167357624421baf3add3153914ba23510d) | Free Transform: lift a pixel's coverage up to all of it, as Move does | `crates/pc-engine/src/transform_cmds.rs` | MIT OR Apache-2.0 |
| [`dce32b66d87a`](https://github.com/storytold/photocraft/commit/dce32b66d87a7bda667c502e55802bd9877eb710) | engine: Free Transform takes soft edges whole after a move (#10461) | `crates/pc-engine/src/transform_cmds.rs` | MIT OR Apache-2.0 |
| [`baf8e654e886`](https://github.com/storytold/photocraft/commit/baf8e654e8860862d44c8471a52077d0f84317aa) | Keep a layer's place on its board when dragged to another artboard (#10489) | `crates/pc-engine/src/commands.rs`<br>`crates/pc-engine/src/artboard_cmds/tests.rs` | MIT OR Apache-2.0 |
| [`b800c40cfdf1`](https://github.com/storytold/photocraft/commit/b800c40cfdf115ac831f2f4db5c291fa12e2e4e2) | Don't auto-select off-canvas layers from the pasteboard (#10665) | `crates/pc-engine/src/pick_cmds.rs` | MIT OR Apache-2.0 |
| [`c2acddae8c7b`](https://github.com/storytold/photocraft/commit/c2acddae8c7b72c4fa8eef5bd37d665521eee948) | Transform: Distort side handles move their edge, Perspective's skew (#10085) | `crates/pc-ui-egui/src/transform_tool.rs` | MIT OR Apache-2.0 |
| [`0125925ee397`](https://github.com/storytold/photocraft/commit/0125925ee39723c6d61a54e67db961161982623e) | Crop: no frame around the cropped image until the next press | `crates/pc-ui-egui/src/crop_ui.rs`<br>`crates/pc-ui-egui/src/canvas/tools.rs` | MIT OR Apache-2.0 |
| [`bed677b3aa9a`](https://github.com/storytold/photocraft/commit/bed677b3aa9a81c3608c0b16a5ce1422e0bd0ead) | fix(ui): make the Histogram panel follow the selection | `crates/pc-ui-egui/src/tone.rs`<br>`crates/pc-ui-egui/src/lib.rs` | MIT OR Apache-2.0 |
| [`abb855881523`](https://github.com/storytold/photocraft/commit/abb8558815232b0acdee419809d3695e13904869) | Transform: Perspective side-handle slides stop before the box folds | `crates/pc-ui-egui/src/transform_tool.rs` | MIT OR Apache-2.0 |
| [`9891eaeb2efc`](https://github.com/storytold/photocraft/commit/9891eaeb2efcae43002d7829438ba092c0c15711) | Crop: History jumps wait for a pending crop (#9285) | `crates/pc-ui-egui/src/crop_ui.rs`<br>`crates/pc-ui-egui/src/lib.rs`<br>`crates/pc-ui-egui/src/panels/docks.rs` | MIT OR Apache-2.0 |
| [`1b810052bc3a`](https://github.com/storytold/photocraft/commit/1b810052bc3a876c1eaf1a8766870ffdba9f8a80) | fix(engine): guard trim and duplicate merged against documents exceeding render limits (#7223) | `crates/pc-engine/src/image_cmds.rs` | MIT OR Apache-2.0 |
| [`f4adb8fb9b9a`](https://github.com/storytold/photocraft/commit/f4adb8fb9b9a0608dae983338467b3a0c66658e2) | Render Video: cap the total an animated GIF keeps staged (#6103) | `crates/pc-engine/src/video_cmds.rs` | MIT OR Apache-2.0 |
| [`c6a71929efd1`](https://github.com/storytold/photocraft/commit/c6a71929efd1163418a287d21749aa9444b6e916) | fix(cms): charge decoded CLUT values against a per-profile budget (#11055) | `crates/pc-cms/src/profile.rs` | MIT OR Apache-2.0 |
| [`7f5e103ff1e9`](https://github.com/storytold/photocraft/commit/7f5e103ff1e9fd02fe700f3c04a6df4a90152129) | fix(algo): Filter Gallery reach no longer overflows on out-of-range stored effect values | `crates/pc-algo/src/artistic.rs`<br>`crates/pc-algo/src/artistic_fx.rs` | MIT OR Apache-2.0 |
| [`4777b390c9bb`](https://github.com/storytold/photocraft/commit/4777b390c9bbfb8cc8f882b8f2343f11930cfdc8) | fix(algo): Filter Gallery draws pixels from the same clamped values as its reach | `crates/pc-algo/src/artistic_fx.rs` | MIT OR Apache-2.0 |
| [`3b43cb451a66`](https://github.com/storytold/photocraft/commit/3b43cb451a667a17cfcb14870abeeefa676df01c) | fix(algo): hold raw filter parameters to their ranges (Water Paper, Box Blur, Lanczos, Camera Raw noise) (#11199) | `crates/pc-algo/src/blur.rs`<br>`crates/li-raster/src/resample.rs`<br>`crates/pc-algo/src/artistic_fx.rs` | MIT OR Apache-2.0 |
| [`648d742a508c`](https://github.com/storytold/photocraft/commit/648d742a508c77b935952c90ce42c26802a87205) | fix(algo): bound EXIF text decoding, warp scans, gallery windows, inpaint sizes, pyramid depth (#11161) | `crates/pc-algo/src/inpaint.rs`<br>`crates/pc-algo/src/warp.rs` | MIT OR Apache-2.0 |
| [`ce304774e070`](https://github.com/storytold/photocraft/commit/ce304774e0701019495f224ab574fb26b0d46ce0) | fix(blur): Gaussian Blur radii below 1 blur by the radius asked for | `crates/pc-algo/src/blur.rs`<br>`crates/pc-algo/src/lib.rs` | MIT OR Apache-2.0 |
| [`eb1c6d291fd3`](https://github.com/storytold/photocraft/commit/eb1c6d291fd3dc3929d646cf20c3caacdcde9489) | fix(blur): exact variance right up to radius 1 | `crates/pc-algo/src/blur.rs`<br>`crates/pc-algo/src/lib.rs` | MIT OR Apache-2.0 |
| [`9a570c5ed5c9`](https://github.com/storytold/photocraft/commit/9a570c5ed5c98748f210c515386abc1b92087d83) | fix(blur): one kernel width for every Gaussian radius below 1 | `crates/pc-algo/src/blur.rs`<br>`crates/pc-algo/src/lib.rs` | MIT OR Apache-2.0 |
| [`e01fa8ba4e20`](https://github.com/storytold/photocraft/commit/e01fa8ba4e208c20212c1758d298477bcf1483ae) | Image: Trim (Transparent Pixels) ignores alpha that rounds to 0 at the document's depth (#9299) | `crates/pc-engine/src/image_cmds.rs` | MIT OR Apache-2.0 |
| [`b2b402939a8c`](https://github.com/storytold/photocraft/commit/b2b402939a8cfe3fd2969ab42918a04f8ac27d12) | Crop W x H budgets the hidden pixels it resamples (#9212) | `crates/pc-engine/src/image_cmds.rs` | MIT OR Apache-2.0 |
| [`945cf2218d67`](https://github.com/storytold/photocraft/commit/945cf2218d67b6c748329bd463c352c0c00198f4) | Crop with Delete Cropped Pixels off turns the Background into Layer 0 (#10867) | `crates/pc-engine/src/image_cmds.rs`<br>`crates/pc-engine/src/extra_cmds.rs`<br>`crates/pc-engine/src/eraser_cmds.rs` | MIT OR Apache-2.0 |
| [`316e00a97eab`](https://github.com/storytold/photocraft/commit/316e00a97eab730e60584df578c295b5c2539d95) | Bound Photomerge memory and safely reject out-of-budget jobs (#7851) (#10463) | `crates/pc-engine/src/photo_cmds.rs` | MIT OR Apache-2.0 |
| [`ab033734d22d`](https://github.com/storytold/photocraft/commit/ab033734d22dbb7730e263dc4849441b4f6222ed) | Photomerge: merging the open documents no longer scans the dialog's folder (#13580) | `crates/pc-engine/src/photo_cmds.rs` | MIT OR Apache-2.0 |
| [`6f5e9e9d3433`](https://github.com/storytold/photocraft/commit/6f5e9e9d343374615e74f592b9e9002cd7547c62) | Photomerge: the first source is the top layer (#14092) | `crates/pc-engine/src/photo_cmds.rs` | MIT OR Apache-2.0 |
| [`bf5c479d29df`](https://github.com/storytold/photocraft/commit/bf5c479d29dfb35f706e9a84e5b41eeb1274e148) | fix(layers): refuse to ungroup the sole empty layer group (#11118) (#11163) | `crates/pc-engine/src/extra_cmds.rs` | MIT OR Apache-2.0 |
| [`5a935b2830c2`](https://github.com/storytold/photocraft/commit/5a935b2830c2eb48842e8bfa999c19dea817eba4) | Place: refuse a scale that makes the placed image exceed the render pixel limit (#11229) | `crates/pc-engine/src/file_cmds.rs` | MIT OR Apache-2.0 |
| [`b6bb43bc1975`](https://github.com/storytold/photocraft/commit/b6bb43bc1975848efea4e63252b2625de0f7c91c) | Handle edited Stack Modes and cap video frame growth | `crates/pc-engine/src/video_cmds.rs` | MIT OR Apache-2.0 |
| [`d3cd28f14bff`](https://github.com/storytold/photocraft/commit/d3cd28f14bffaf49d41c9cf9d984ce05c17aaf6e) | Rotate Canvas: the selection is dropped, one undo brings it back (#14507) | `crates/pc-engine/src/mode_cmds.rs` | MIT OR Apache-2.0 |
| [`d1fcc176fa4e`](https://github.com/storytold/photocraft/commit/d1fcc176fa4e167dab68a377d36900ba7aea1b87) | Canvas Size: the selection is dropped, one undo brings it back (#14539) | `crates/pc-engine/src/image_cmds.rs` | MIT OR Apache-2.0 |
| [`bfd9a2f0c5c5`](https://github.com/storytold/photocraft/commit/bfd9a2f0c5c5e2247e727ab968a48cdee91ca41f) | Image > Duplicate: the copy starts without a selection (#14593) | `crates/pc-engine/src/image_cmds.rs` | MIT OR Apache-2.0 |
| [`1c606fddfd92`](https://github.com/storytold/photocraft/commit/1c606fddfd92e14ed4a9860caea6230ac774e2c9) | lens: Auto Scale keeps the border pixels clear of the Edge fill | `crates/pc-algo/src/lens.rs` | MIT OR Apache-2.0 |
| [`0efeadc1ee04`](https://github.com/storytold/photocraft/commit/0efeadc1ee0408ec8f88f463e975d65291beff14) | perf(gpu): NVIDIA's own memory on Linux, so many-layer documents stay off the CPU budget penalty | `crates/li-gpu/src/memory.rs` | MIT OR Apache-2.0 |
| [`ffd4d480bb7c`](https://github.com/storytold/photocraft/commit/ffd4d480bb7ca1a411debdebae3f9be59537dc7c) | Integration: bound the nvidia-smi query, smallest of same-id cards (#9042) | `crates/li-gpu/src/memory.rs` | MIT OR Apache-2.0 |
| [`57a8691fb3a1`](https://github.com/storytold/photocraft/commit/57a8691fb3a15450a941fd747d4d0975077f47c8) | Respect mask targets in Fill and Stroke dialogs | `crates/pc-engine/src/extra_cmds.rs` | MIT OR Apache-2.0 |
| [`78b69911243e`](https://github.com/storytold/photocraft/commit/78b69911243ef6c7e38f512e7e1543bb25bef842) | fix(engine): keep missing-font type pixels through Image Size (#8860) | `crates/pc-engine/src/canvas_geom.rs`<br>`crates/pc-engine/src/mode_cmds.rs`<br>`crates/pc-engine/src/type_extra_cmds.rs`<br>`crates/pc-engine/src/image_cmds.rs` | MIT OR Apache-2.0 |
| [`dd19c295d1e5`](https://github.com/storytold/photocraft/commit/dd19c295d1e58fba692445bdf1ff8568d60e29d5) | fix(engine): bound sparse remap, scaled clone sources, path reads, export format, name lookups (#11181) | `crates/pc-engine/src/pixels.rs`<br>`crates/pc-engine/src/retouch_cmds.rs`<br>`crates/pc-engine/src/file_cmds.rs`<br>`crates/pc-engine/src/adjust_cmds.rs` | MIT OR Apache-2.0 |
| [`f7771c8545a5`](https://github.com/storytold/photocraft/commit/f7771c8545a5a3a8e9694e3af42b4ad8082731b6) | Engine: check dense working buffers against a budget before expanding sparse content (#10964) | `crates/pc-engine/src/allocation.rs`<br>`crates/pc-engine/src/adjust_cmds.rs`<br>`crates/pc-engine/src/align_cmds.rs`<br>`crates/pc-engine/src/channel_cmds.rs`<br>`crates/pc-engine/src/distort_cmds.rs`<br>`crates/pc-engine/src/float_cmds.rs`<br>`crates/pc-engine/src/layer_menu_cmds.rs`<br>`crates/pc-engine/src/multichannel_cmds.rs`<br>`crates/pc-engine/src/filters.rs`<br>`crates/pc-engine/src/image_cmds.rs`<br>`crates/pc-engine/src/dense_budget_tests.rs` | MIT OR Apache-2.0 |
| [`f373ec181c48`](https://github.com/storytold/photocraft/commit/f373ec181c482b2a88b2f375fd4b20bda8194f3c) | fix(format,engine): limit zstd windows, set bundle modes via the handle, refuse symlinked presets (#11145) | `crates/pc-format/src/store.rs`<br>`crates/li-fs/src/lib.rs`<br>`crates/li-fs/src/tests.rs`<br>`crates/pc-engine/src/preset_store.rs`<br>`crates/pc-engine/src/preset_store/tests.rs` | MIT OR Apache-2.0 |
| [`87da091f220e`](https://github.com/storytold/photocraft/commit/87da091f220e089fe096f67c6cb69e9d9878e637) | fix(format): reject duplicate native layer identities | `crates/pc-format/src/convert.rs`<br>`crates/pc-format/tests/it/corruption.rs` | MIT OR Apache-2.0 |
| [`6e2e02fa2663`](https://github.com/storytold/photocraft/commit/6e2e02fa2663ebd062ad1409f39e04d60655d1d7) | Read layer-record blend keys in PSD layer effects | `crates/pc-io/src/effects_map.rs` | MIT OR Apache-2.0 |
| [`e614f7d39b3e`](https://github.com/storytold/photocraft/commit/e614f7d39b3e8273df14cca3d5f6c28fa008fc47) | fix(io): PSD layer-effect contours keep at most 16 points (#9556) | `crates/pc-io/src/effects_map.rs` | MIT OR Apache-2.0 |
| [`a9744ed44dc7`](https://github.com/storytold/photocraft/commit/a9744ed44dc782613b19a97b8c159a091f73e669) | PSD contours: inputs a few ulps apart still span a segment | `crates/pc-io/src/effects_map.rs` | MIT OR Apache-2.0 |
| [`3fd5304dfe5b`](https://github.com/storytold/photocraft/commit/3fd5304dfe5bc84c317fc50021d9cf30d34f1ae9) | text: find paragraphs and trailing blanks without rescanning (#11045) | `crates/pc-text/src/layout.rs`<br>`crates/pc-text/src/tests.rs` | MIT OR Apache-2.0 |
| [`37a01ca8746c`](https://github.com/storytold/photocraft/commit/37a01ca8746c121170644a07968d11263ef64407) | format: bound blob inflation by the load's remaining budget; browser budgets (#10984) | `crates/pc-format/src/store.rs` | MIT OR Apache-2.0 |
| [`9a390661b5e2`](https://github.com/storytold/photocraft/commit/9a390661b5e265d756233a59c5c20c9f7e9dfbcd) | PSD: flat files keep their spot and alpha channels (#10260) | `crates/pc-psd/src/file.rs` | MIT OR Apache-2.0 |
| [`fc94ac59e263`](https://github.com/storytold/photocraft/commit/fc94ac59e2639130908ef6fc6e7839ae1b88750a) | Saving: the temporary file is no longer a dot file (#11624) | `crates/li-fs/src/lib.rs`<br>`crates/li-fs/src/tests.rs`<br>`crates/pc-format/src/autosave.rs`<br>`crates/lc-preview/src/disk.rs`<br>`crates/lc-engine/src/smart_sort/faces_store.rs` | MIT OR Apache-2.0 |
| [`8ef148ace961`](https://github.com/storytold/photocraft/commit/8ef148ace961d2589dde13df55c8be4ded9917d2) | Recovery: replace a symlinked sidecar and do not sweep through symlinked bundle folders (#11230) | `crates/pc-format/src/autosave.rs` | MIT OR Apache-2.0 |
| [`7348e060b4be`](https://github.com/storytold/photocraft/commit/7348e060b4be7204fb69b59079c3b9d808e9aad3) | fix(paint): scale blur sigma with brush size (#5892) | `crates/pc-engine/src/retouch_cmds.rs`<br>`crates/pc-engine/src/retouch_cmds/tests.rs` | MIT OR Apache-2.0 |
| [`b4487ee59998`](https://github.com/storytold/photocraft/commit/b4487ee599980917b0407a165c484fd003bba9db) | fix(paint): a bucket fill clicked on transparency fills the transparency in full (#11311) | `crates/pc-algo/src/paint.rs` | MIT OR Apache-2.0 |
| [`66f1e8b8aa91`](https://github.com/storytold/photocraft/commit/66f1e8b8aa918547b8551543190cd48fcdb8b912) | Selection: Grow and Similar match only selected colours, Grow starts from the frontier (#9673) | `crates/pc-algo/src/selection.rs`<br>`crates/pc-engine/src/selection_cmds.rs` | MIT OR Apache-2.0 |
| [`0350b5de16e3`](https://github.com/storytold/photocraft/commit/0350b5de16e37c1ff3765220ebd910affceca177) | Select sky beneath an obstructed top border (#9693) | `crates/pc-algo/src/segment/sky.rs` | MIT OR Apache-2.0 |
| [`a3c0b7258c16`](https://github.com/storytold/photocraft/commit/a3c0b7258c16b312b51bca82904be413bbafda8f) | Select Subject: keep parts that run off the image edge and detached parts near the subject (#10160) | `crates/pc-algo/src/segment/subject.rs` | MIT OR Apache-2.0 |
| [`9b8536f5e6ac`](https://github.com/storytold/photocraft/commit/9b8536f5e6ac585d4adcd4c6feac7b03f4007195) | Fix Clone Stamp sources across layers and documents | `crates/pc-engine/src/lib.rs`<br>`crates/pc-engine/src/presets/clone_source.rs`<br>`crates/pc-engine/src/retouch_cmds/clone_sampling.rs`<br>`crates/pc-engine/src/retouch_cmds.rs`<br>`crates/pc-engine/src/retouch_cmds/tests.rs`<br>`crates/pc-ui-egui/src/preset_panels.rs`<br>`crates/pc-ui-egui/src/retouch_ui.rs` | MIT OR Apache-2.0 |
| [`b71433249790`](https://github.com/storytold/photocraft/commit/b714332497905a4485bbd9e29f4727aa0b526670) | Keep Clone Source offsets consistent with preview and painting | `crates/pc-engine/src/presets/clone_source.rs`<br>`crates/pc-engine/src/retouch_cmds/clone_sampling.rs`<br>`crates/pc-engine/src/retouch_cmds.rs`<br>`crates/pc-engine/src/retouch_cmds/tests.rs` | MIT OR Apache-2.0 |
| [`35e45defd785`](https://github.com/storytold/photocraft/commit/35e45defd785bb6197b85c97202a20a1c280d192) | Avoid overflow in off-canvas clone resampling | `crates/pc-engine/src/presets/clone_source.rs` | MIT OR Apache-2.0 |
| [`967e987edde4`](https://github.com/storytold/photocraft/commit/967e987edde4bd683077b1e0f3273d32dfa2090f) | Preserve clone canvas coverage for sources without alpha | `crates/pc-engine/src/retouch_cmds/clone_sampling.rs`<br>`crates/pc-engine/src/retouch_cmds.rs`<br>`crates/pc-engine/src/retouch_cmds/tests.rs` | MIT OR Apache-2.0 |
| [`0f205f030bc5`](https://github.com/storytold/photocraft/commit/0f205f030bc52fb484c5bc06b83cc66d7740a830) | Sharpen tool: Protect Detail sharpens gently instead of doing nothing (#12938) | `crates/pc-algo/src/retouch.rs`<br>`crates/pc-engine/src/retouch_cmds.rs`<br>`crates/pc-engine/src/retouch_cmds/tests.rs` | MIT OR Apache-2.0 |
| [`d0b24e1abdad`](https://github.com/storytold/photocraft/commit/d0b24e1abdad2e04eef2d7fe7ad413e9a70bccb8) | Round the Magic Wand's Tolerance field to whole numbers | `crates/pc-ui-egui/src/panels/options_bar.rs`<br>`crates/pc-ui-egui/src/widgets.rs` | MIT OR Apache-2.0 |
| [`20f92ee405dc`](https://github.com/storytold/photocraft/commit/20f92ee405dc1c195a7a10ed54d2af9ca2ecef06) | codecs: leave Extended XMP fragments out of zune-jpeg's header parse (#10981) | `crates/pc-codecs/src/codecs/jpeg.rs` | MIT OR Apache-2.0 |
| [`42929264805c`](https://github.com/storytold/photocraft/commit/42929264805c70be29813fabd738c905c5b07460) | Codecs: a PNG's iCCP profile inflates to at most 16.7 MB (#8599) | `crates/pc-codecs/src/codecs/png.rs` | MIT OR Apache-2.0 |
| [`acd57a36af62`](https://github.com/storytold/photocraft/commit/acd57a36af62ccba3c21004052113716605bb937) | Codecs: a PNG's iCCP profile also keeps to the caller's allocation limit (#8599) | `crates/pc-codecs/src/codecs/png.rs` | MIT OR Apache-2.0 |
| [`d82e166494f9`](https://github.com/storytold/photocraft/commit/d82e166494f95f9805af18d02e2121f29edd5ef5) | codecs: charge dropped PNG text against the metadata budget (#10980) | `crates/pc-codecs/src/codecs/png.rs` | MIT OR Apache-2.0 |
| [`4a65ae3fb0b8`](https://github.com/storytold/photocraft/commit/4a65ae3fb0b8a1518f2e1da7051a6285f82af4e5) | codecs: refuse EXR preview attributes larger than their value (#10982) | `crates/pc-codecs/src/codecs/exr.rs`<br>`crates/pc-codecs/src/lib.rs` | MIT OR Apache-2.0 |
| [`81ce14c1b4ac`](https://github.com/storytold/photocraft/commit/81ce14c1b4ac178efa328f23d88b5a6e18cfc691) | codecs: count TIFF strip/tile scratch against max_alloc | `crates/pc-codecs/src/codecs/tiff.rs`<br>`crates/pc-codecs/src/options.rs`<br>`crates/pc-codecs/tests/it/tiff_pages.rs` | MIT OR Apache-2.0 |
| [`2d3f6dc91a92`](https://github.com/storytold/photocraft/commit/2d3f6dc91a92a6b0b9f9374c968256dfc85fc495) | TIFF: reserve a strip's scratch buffer fallibly in decode_band | `crates/pc-codecs/src/codecs/tiff.rs` | MIT OR Apache-2.0 |
| [`f4d4524161b2`](https://github.com/storytold/photocraft/commit/f4d4524161b25b432b32537c7a9ea9be2a875349) | codecs: refuse TIFF chunk tables over max_alloc before the decoder opens | `crates/pc-codecs/src/codecs/tiff.rs`<br>`crates/pc-codecs/tests/it/tiff_pages.rs` | MIT OR Apache-2.0 |
| [`eef1638eff87`](https://github.com/storytold/photocraft/commit/eef1638eff87d9fef5a6a28ee01acdaf706ee55b) | codecs: bound the TIFF JPEG header walk by the file length | `crates/pc-codecs/src/codecs/tiff.rs`<br>`crates/pc-codecs/tests/it/tiff_pages.rs` | MIT OR Apache-2.0 |
| [`f45711c74937`](https://github.com/storytold/photocraft/commit/f45711c74937cb5d723758a81f2cd9e069f4c67f) | codecs: check TIFF structural tags, predicted rows and JPEG chunk frames (#10983) | `crates/pc-codecs/src/codecs/tiff.rs`<br>`crates/pc-codecs/tests/it/tiff_regressions.rs` | MIT OR Apache-2.0 |
| [`129e6aec9000`](https://github.com/storytold/photocraft/commit/129e6aec9000bad90c935ae4b3da62c4708a3040) | Integration: blank Group 4 fax pages pass the TIFF bomb guard (#8536) | `crates/pc-codecs/src/codecs/tiff.rs`<br>`crates/pc-codecs/tests/it/tiff_pages.rs` | MIT OR Apache-2.0 |
| [`fb04b02893c5`](https://github.com/storytold/photocraft/commit/fb04b02893c55e359ab86f4d0fdcc6165e8b0b47) | fix(codecs): check header dimensions against the file size before allocating | `crates/pc-codecs/src/codecs/via_image.rs`<br>`crates/pc-codecs/src/codecs/exr.rs`<br>`crates/pc-codecs/tests/it/limits.rs` | MIT OR Apache-2.0 |
| [`c65b7d93d650`](https://github.com/storytold/photocraft/commit/c65b7d93d65027e14a608f3407b9fd99f5cef5b1) | Integration: file-size bound fits DWA EXR, RLE BMP and GIF screens (#8704) | `crates/pc-codecs/src/codecs/via_image.rs`<br>`crates/pc-codecs/src/codecs/exr.rs`<br>`crates/pc-codecs/tests/it/limits.rs` | MIT OR Apache-2.0 |
| [`3225307e5780`](https://github.com/storytold/photocraft/commit/3225307e57802c76b540ee7a8691b5d635e447d8) | codecs: admit animated GIF screens, sparse RLE BMPs and icon PNG profiles (#10979) | `crates/pc-codecs/src/codecs/via_image.rs`<br>`crates/pc-codecs/tests/it/limits.rs` | MIT OR Apache-2.0 |
| [`ddae31bad466`](https://github.com/storytold/vectorcraft/commit/ddae31bad4662005d2b04a6d85db4217b6173d1e) | TIFF: alpha after another extra sample places and opens as transparency | `crates/pc-codecs/src/codecs/tiff.rs`<br>`crates/pc-codecs/tests/it/tiff_pages.rs` | MIT OR Apache-2.0 |
| [`ab4ba9f37cf2`](https://github.com/storytold/photocraft/commit/ab4ba9f37cf2428cba5b28426fde228e5c4d1b04) | fix(codecs): checked sizes in encoders, EXR name lookup, FITS float limit, VP8 plane copies, EXIF thumbnail bytes (#11164) | `crates/pc-codecs/src/codecs/vp8/mod.rs` | MIT OR Apache-2.0 |
| [`9034223c4b6d`](https://github.com/storytold/photocraft/commit/9034223c4b6d80fe15a12b9e0d173786ac5f7a4c) | TIFF: open YCbCr images as RGB (#10316) (#10319) | `crates/pc-codecs/src/codecs/tiff.rs`<br>`crates/pc-codecs/src/codecs/tiff_ycbcr.rs`<br>`crates/pc-codecs/src/codecs/mod.rs`<br>`crates/pc-codecs/tests/it/main.rs`<br>`crates/pc-codecs/tests/it/tiff_ycbcr.rs` | MIT OR Apache-2.0 |
| [`cf07f076b502`](https://github.com/storytold/photocraft/commit/cf07f076b502cd167fde951d7bd384edf1ab42d1) | Move tool: a huge layer isn't copied for every frame of a live drag (#6114, #6163) | `crates/pc-ui-egui/src/move_ui.rs` | MIT OR Apache-2.0 |
| [`f09757e4b6d6`](https://github.com/storytold/photocraft/commit/f09757e4b6d6e6c24d399400cece5e1c164e27a5) | Move: bound the live-drag estimate by the worst case of four written tiles per stored tile; test the limit through preview initialization | `crates/pc-ui-egui/src/move_ui.rs` | MIT OR Apache-2.0 |
| [`ff0f1ba0c551`](https://github.com/storytold/photocraft/commit/ff0f1ba0c55136b73e96f2fef17c1eece80da79c) | fix(algo): Liquify replays a stroke with an absurd brush size or position without overflowing | `crates/pc-algo/src/liquify.rs`<br>`crates/pc-algo/tests/coverage/liquify_coverage.rs` | MIT OR Apache-2.0 |
| [`200f38f7b606`](https://github.com/storytold/photocraft/commit/200f38f7b60600096627762f1d6c5f320ad1274a) | Liquify: a huge far-off dab writes no NaN and reports the area it changed | `crates/pc-algo/src/liquify.rs`<br>`crates/pc-algo/tests/coverage/liquify_coverage.rs` | MIT OR Apache-2.0 |
| [`c2cc5eee54a6`](https://github.com/storytold/photocraft/commit/c2cc5eee54a6cf3cc020cf04da4a92e0752d337a) | fix(ui): the Liquify preview shows the effect only inside the selection (#9541) | `crates/pc-ui-egui/src/liquify_ui.rs` | MIT OR Apache-2.0 |
| [`d98b9ea17be9`](https://github.com/storytold/photocraft/commit/d98b9ea17be9b1b96aa3e052fb33e981b36928b9) | paint: stop a stroke past a dab-area budget and skip dabs outside the paintable area | `crates/pc-paint/src/render.rs`<br>`crates/pc-paint/src/render/budget_tests.rs` | MIT OR Apache-2.0 |
| [`efd725cc89aa`](https://github.com/storytold/photocraft/commit/efd725cc89aa0cea7844319d8a6561d914ec92cf) | paint: charge the dual brush apart, skip (not end on) a dab that does not fit the budget | `crates/pc-paint/src/render.rs`<br>`crates/pc-paint/src/render/budget_tests.rs` | MIT OR Apache-2.0 |
| [`f5e92a6b3d03`](https://github.com/storytold/photocraft/commit/f5e92a6b3d0350a0beb0a99faec87c48a229d8cc) | paint: stream the dab generator's steps and keep non-finite brush values and points out | `crates/pc-paint/src/dynamics.rs`<br>`crates/pc-paint/src/brush.rs` | MIT OR Apache-2.0 |
| [`af025e18d514`](https://github.com/storytold/photocraft/commit/af025e18d51418ad5569111eddb0a782151368d2) | paint: replace only NaN brush values, test a long pixel-grid segment | `crates/pc-paint/src/dynamics.rs`<br>`crates/pc-paint/src/brush.rs` | MIT OR Apache-2.0 |
| [`0ff76d6f0c08`](https://github.com/storytold/photocraft/commit/0ff76d6f0c089157ddf9c380abd6381a92946311) | fix(paint): cap the total steps one stroke emits (#11195) | `crates/pc-paint/src/dynamics.rs` | MIT OR Apache-2.0 |
| [`abeb16c96bb6`](https://github.com/storytold/photocraft/commit/abeb16c96bb6c55c70f11a50f57138d17d546db1) | paint: refuse a sequential-dab stroke whose working copy is too large (#11070) | `crates/pc-paint/src/retouch.rs`<br>`crates/pc-paint/src/mixer.rs`<br>`crates/pc-paint/src/bg_erase.rs`<br>`crates/pc-paint/src/replace.rs`<br>`crates/pc-engine/src/brush_cmds.rs`<br>`crates/pc-engine/src/eraser_cmds.rs`<br>`crates/pc-engine/src/retouch_cmds.rs` | MIT OR Apache-2.0 |
| [`acd35e12027d`](https://github.com/storytold/photocraft/commit/acd35e12027dffe870fc04100c70ed8127ebd430) | Guard zero-channel regions and saturate the pick-up bounds in Smudge | `crates/pc-paint/src/retouch.rs` | MIT OR Apache-2.0 |
| [`22882f690c17`](https://github.com/storytold/photocraft/commit/22882f690c1775a9447715e524a7849da17bea67) | Background Eraser: alpha falls off over the whole Tolerance (#11901) | `crates/pc-paint/src/bg_erase.rs`<br>`crates/pc-paint/src/bg_erase/graded_alpha_tests.rs` | MIT OR Apache-2.0 |
| [`6bd3773e0743`](https://github.com/storytold/photocraft/commit/6bd3773e0743d21bb28d76ca7026298462f967ea) | Address CodeRabbit review: match the foreground at the document's precision, pin the protect-off row | `crates/pc-paint/src/bg_erase.rs` | MIT OR Apache-2.0 |
| [`23f63c685eb3`](https://github.com/storytold/photocraft/commit/23f63c685eb3053f6434cdacc4b27afeb32e39c8) | Background Eraser: Find Edges erases soft edges graded instead of cutting them hard (#11904) | `crates/pc-paint/src/bg_erase.rs`<br>`crates/pc-paint/src/bg_erase/find_edges_tests.rs` | MIT OR Apache-2.0 |
| [`6d3fe6f96f4e`](https://github.com/storytold/photocraft/commit/6d3fe6f96f4ee9bb670bf3903a44fc22de0c64d2) | Background Eraser: partly erased pixels lose the sampled colour's tint (#11902) | `crates/pc-paint/src/bg_erase.rs`<br>`crates/pc-paint/src/bg_erase/decontamination_tests.rs` | MIT OR Apache-2.0 |
| [`7efd224081a1`](https://github.com/storytold/photocraft/commit/7efd224081a190521f6634e766df02d75de77ab7) | Color Replacement: in Lab, Color keeps L and takes the foreground's a and b (#11922) | `crates/pc-paint/src/replace.rs`<br>`crates/pc-paint/src/replace/lab_tests.rs` | MIT OR Apache-2.0 |
| [`1cf1e15da568`](https://github.com/storytold/photocraft/commit/1cf1e15da5684388e43a17ecbca5fe179f248c20) | Keep Lab/16 Color Replacement on the RGB blend (CodeRabbit review on crates/paint/src/replace.rs:101) | `crates/pc-paint/src/replace.rs`<br>`crates/pc-paint/src/replace/lab_tests.rs` | MIT OR Apache-2.0 |
| [`cfd894aad3ba`](https://github.com/storytold/photocraft/commit/cfd894aad3bab1d257238812f944daf359934f74) | security(paint): cap spaced steps and procedural primitives (#8957) | `crates/pc-paint/src/dynamics.rs` | MIT OR Apache-2.0 |
| [`08ff029ac228`](https://github.com/storytold/vectorcraft/commit/08ff029ac22860f47cf0e0b427a932f7d675003e) | fix(pathops): bound offset path complexity and handle arrangement failure safely (#4145) | `crates/pc-pathops/src/offset.rs`<br>`crates/pc-pathops/src/boolean.rs`<br>`crates/pc-pathops/src/budget.rs`<br>`crates/pc-pathops/src/planar.rs` | MIT OR Apache-2.0 |
| [`825620eb6cd1`](https://github.com/storytold/vectorcraft/commit/825620eb6cd17a4298b4fa9041013abe3d35190c) | Clipboard: Linux and BSD reads wait 2 s at most for the app that copied | `apps/local-image/src/services.rs`<br>`apps/local-image/src/services/clipboard.rs` | MIT OR Apache-2.0 |
| [`29a98b750ac9`](https://github.com/storytold/vectorcraft/commit/29a98b750ac90e530d6bd03b45ea56b8cae2ca72) | Respect quotes and parentheses when splitting SVG CSS declarations | `crates/pc-svg/src/import/css.rs` | MIT OR Apache-2.0 |
| [`2b37302fe5f7`](https://github.com/storytold/vectorcraft/commit/2b37302fe5f7570238836d2aee943843673bf815) | Preserve comment markers inside CSS strings | `crates/pc-svg/src/import/css.rs` | MIT OR Apache-2.0 |
| [`194364304bec`](https://github.com/storytold/vectorcraft/commit/194364304bec7b677a8d8a7118ecaaf6a865fb71) | Strip CSS comments from style attributes and rule declarations | `crates/pc-svg/src/import/css.rs` | MIT OR Apache-2.0 |
| [`89f20c45c45c`](https://github.com/storytold/vectorcraft/commit/89f20c45c45c6b6eb2127d1dbb1ec7f53bb29488) | SVG import: bound selector matching and share the limits with linked and embedded SVGs | `crates/pc-svg/src/import/preflight.rs` | MIT OR Apache-2.0 |
| [`60267727656f`](https://github.com/storytold/vectorcraft/commit/60267727656f64fc138d66b592c1bc1d0e7d0e76) | Fix #3773: Offset Path offsets a path whose ends meet as a closed shape | `crates/pc-pathops/src/offset.rs` | MIT OR Apache-2.0 |
| [`1574dfe7293c`](https://github.com/storytold/vectorcraft/commit/1574dfe7293c3acf0f5d66b47a628e6483a96260) | fix: anchor positions and Simplify measure curves through the guarded arc length | `crates/pc-pathops/src/geom/path.rs`<br>`crates/pc-pathops/src/edit.rs` | MIT OR Apache-2.0 |
| [`e9510956172c`](https://github.com/storytold/vectorcraft/commit/e9510956172c5e48f555db7a93e2d139d757830f) | Path booleans: refuse out-of-range input, guard the contour pass, cuts report refusals (#5465) | `crates/pc-pathops/src/boolean.rs` | MIT OR Apache-2.0 |
| [`0a2935cf8f98`](https://github.com/storytold/vectorcraft/commit/0a2935cf8f984ddbfcb1959f4ff44166dd9efcc9) | Booleans: limit of one unit, crop outside shortcut, tests (#5465) | `crates/pc-pathops/src/boolean.rs` | MIT OR Apache-2.0 |
| [`0e64aca55746`](https://github.com/storytold/vectorcraft/commit/0e64aca557462bb6507f6a2e8e2a50a86122f8db) | Bound work: flattening of huge curves, print layout past the page limit, script-check file names | `crates/pc-pathops/src/geom/bez.rs`<br>`crates/pc-pathops/src/boolean.rs`<br>`crates/pc-pathops/src/planar.rs`<br>`crates/pc-pathops/src/offset.rs` | MIT OR Apache-2.0 |
| [`9cd997b9aac0`](https://github.com/storytold/vectorcraft/commit/9cd997b9aac07c8dad19c2f830407a346922d027) | address review: draw a line for curves with far-out start or control points in flatten | `crates/pc-pathops/src/geom/bez.rs`<br>`crates/pc-pathops/src/boolean.rs`<br>`crates/pc-pathops/src/planar.rs`<br>`crates/pc-pathops/src/offset.rs` | MIT OR Apache-2.0 |
| [`bc19dd64db45`](https://github.com/storytold/vectorcraft/commit/bc19dd64db458f9b6d6a57c029b73d4ac965fcdc) | Simplify: Convert to Straight Lines keeps the tip of a path that turns back past its end | `crates/pc-pathops/src/edit.rs` | MIT OR Apache-2.0 |
| [`e6add18663a0`](https://github.com/storytold/vectorcraft/commit/e6add18663a01ce563f19e2698e6084eff6840b4) | Fix #6634: Pen closing drag no longer gives the first anchor symmetric handles | `crates/pc-engine/src/vector_packages/pen/gesture.rs` | MIT OR Apache-2.0 |
| [`85e0d59bfa88`](https://github.com/storytold/vectorcraft/commit/85e0d59bfa88b145f38658037e96d2a4884e4f9a) | Brushes: Scale Proportionately art grows with the stroke weight | `crates/pc-vecfx/src/brush/warp.rs` | MIT OR Apache-2.0 |
| [`1ce6431a99fb`](https://github.com/storytold/vectorcraft/commit/1ce6431a99fb188e92d3992c45204a6f59125d2e) | SVG import: raster images follow their EXIF or TIFF orientation | `crates/pc-svg/src/import.rs` | MIT OR Apache-2.0 |
| [`6725f24a80c6`](https://github.com/storytold/vectorcraft/commit/6725f24a80c697c803b40685510bdc6d4a38d7b0) | Path booleans: check the windings against the operands' areas and resolve the operands when they don't add up (#5478) | `crates/pc-pathops/src/boolean.rs` | MIT OR Apache-2.0 |
| [`f4a30c1a94d9`](https://github.com/storytold/vectorcraft/commit/f4a30c1a94d9124236f0c7afd2b5b1e2d29ce0ef) | Audit: geometry length measurement refuses coordinates the integrator cannot handle | `crates/pc-pathops/src/geom/path.rs` | MIT OR Apache-2.0 |
| [`e2a870e60202`](https://github.com/storytold/vectorcraft/commit/e2a870e60202abd5216375de76b5f07e9df41d6c) | Security: bounded flattening, plug-in budgets and caches, loaded textures, Revolve paint copies, color book names and ICC names | `crates/pc-pathops/src/geom/bez.rs` | MIT OR Apache-2.0 |

PhotoCraft document files (2026-11-15). `f373ec18`: the no-follow writer is `li_fs::atomic_write_no_follow`, an `AtomicFs` adapter over `RealFs`, where upstream adds a flag to its own `atomic.rs`. Not ported: its `write_bundle_file` hunk, which has no counterpart here; directory bundles are written through `li_fs::atomic_write`, whose `RealFs::preserve_permissions` sets the mode through a handle, though one it reopens on the staging path. `87da091f` runs after the symbol preflight, which keeps its own duplicate-id error for documents with symbols, and leaves our `max_id` reservation as it is. `e614f7d3` and `a9744ed4` are one port: the point selection is added with the second commit's span test already in it, and without upstream's corner flags, which our `Contour::Custom` does not carry.

LightCraft file safety (2026-11-15). `0d2dd9a9` and its catalog follow-ups: ours has no log thread, so upstream's `log_writer.rs` and the Move-import flush have no counterpart. `690f6cfb` is ported only in part: after a streamed snapshot is staged the folder is asked again, but the staging file's own identity is not compared, as `li-fs` holds that file. The log-length check applies only in a folder accepted by its note, where upstream applies it always.

## PhotoCraft Magic Wand and Magic Eraser parity (2026-11-15)

| Upstream commit | Subject | Our files | Licence |
|---|---|---|---|
| [storytold/photocraft@`0e0d99d0b2b31cdc7809d2ffaf50927f09b6b853`](https://github.com/storytold/photocraft/commit/0e0d99d0b2b31cdc7809d2ffaf50927f09b6b853) | fix(engine): Magic Wand Subtract and Intersect grow from the seed through the selection (#11779) | `crates/pc-algo/src/selection.rs`<br>`crates/pc-engine/src/selection_cmds.rs` | MIT OR Apache-2.0 |
| [storytold/photocraft@`ca005139f201c50c0dcfdacb406aadc983e35981`](https://github.com/storytold/photocraft/commit/ca005139f201c50c0dcfdacb406aadc983e35981) | Shift-click lines aren't smoothed, so they end at the click (#11887) | `crates/pc-ui-egui/src/canvas.rs`<br>`crates/pc-ui-egui/src/canvas/stroke.rs`<br>`crates/pc-ui-egui/src/canvas/tools.rs`<br>`crates/pc-ui-egui/src/stroke_constraint.rs` | MIT OR Apache-2.0 |
| [storytold/photocraft@`66cbb2a11792283a17f7599bed2cfa6e923428fb`](https://github.com/storytold/photocraft/commit/66cbb2a11792283a17f7599bed2cfa6e923428fb) | Retouching tools: a Shift-click line isn't smoothed either (#11887) | `crates/pc-engine/src/retouch_cmds.rs`<br>`crates/pc-engine/src/retouch_cmds/tests.rs`<br>`crates/pc-ui-egui/src/canvas/stroke.rs`<br>`crates/pc-ui-egui/src/canvas/tools.rs`<br>`crates/pc-ui-egui/src/eraser_ui.rs`<br>`crates/pc-ui-egui/src/retouch_ui.rs`<br>`crates/pc-ui-egui/src/stroke_constraint.rs` | MIT OR Apache-2.0 |
| [storytold/photocraft@`e0ca582b97ff5ecfeea81421e60f2642502b00ae`](https://github.com/storytold/photocraft/commit/e0ca582b97ff5ecfeea81421e60f2642502b00ae) | fix(engine): the Magic Eraser measures Tolerance by colour spread (#11995) | `crates/pc-algo/src/selection.rs`<br>`crates/pc-engine/src/eraser_cmds.rs`<br>`crates/pc-engine/src/eraser_cmds/tests.rs`<br>`crates/pc-engine/src/selection_cmds.rs` | MIT OR Apache-2.0 |
| [storytold/photocraft@`25d6041ab52d849fca1f3dd12cf2eb63c1cff45f`](https://github.com/storytold/photocraft/commit/25d6041ab52d849fca1f3dd12cf2eb63c1cff45f) | fix(algo): partly transparent colours keep the per-channel limit (#11995) | `crates/pc-algo/src/selection.rs`<br>`crates/pc-engine/src/eraser_cmds/tests.rs` | MIT OR Apache-2.0 |

Adapted ports: `0e0d99d0` adds the `within` mask to our Sample Size wand (`wand_region_within`, which `wand_region_sampled` calls). `e0ca582b` and `25d6041a` add `ColorDistance` as a parameter of that same function instead of a further wrapper; our Magic Eraser fringe is still the 3x3 mask average (`e25be5e7` is not ported), so upstream's colour-distance fringe hunk has no counterpart. `ca005139` and `66cbb2a1` mark a Shift-click press as `Drag::line` and add `smoothing: 0` through `canvas::unsmoothed_line`; retouch `parse_brush` reads `smoothing` as 0..100. Our UI has no live retouch previews, so `66cbb2a1`'s preview hunks do not apply.

Codec hostile-input bounds (2026-11-15). Adapted ports: `81ce14c1` charges band-worker scratch,
the JPEG copy and the chunk tables next to the output, without upstream's extra-sample accounting,
which this decoder does not have. `eef1638e` takes only the step budget, in our `jpeg_dimensions`
walk; its working-memory and float-predictor charges extend accounting this decoder does not have.
`f45711c7` keeps our existing refusal of JPEG chunks larger than their strip or tile, or without
SOI, and adds the repeated-tag, predictor-row and decoded-frame checks. `129e6aec` sets the Group 4
ratio on our file-length bound. `fb04b028`, `c65b7d93` and `3225307e` port the image-crate and EXR
plausibility checks and the ICO profile check; `dimensions` runs the ICO check too. `ddae31ba`
(VectorCraft) is ported as the TIFF codec reading an alpha that follows an unspecified extra sample,
where VectorCraft fixed its CMYK document loader. `9034223c` reads its float tags in
`tiff_ycbcr.rs`, as our directory type comes from `lightcraft-tiff`. Not ported: `fb04b028`'s JPEG
hunk and `07f91c6d`, whose truncated-file guard was declined in the October parser ports;
`fb04b028`'s PCX and XPM, `3225307e`'s animated-GIF, `4a65ae3f`'s Cryptomatte and `ab4ba9f3`'s
FITS, EXR channel-lookup and EXIF-thumbnail hunks, which have no counterpart here; `ab4ba9f3`'s
checked multiplications in the PNG and web encoders, which cannot overflow on 64-bit targets.
`d03c23e1` and `f98983ff` are already covered before the first scan, where our walker skips stray
bytes without counting them; after a scan our walker stops (`DataEnd::Unread`) by design.

## Partial PhotoCraft paint ports (2026-11-15)

For the paint rows of the table above: `d98b9ea1` brings only the dab-area budget; its clip (skip dabs outside the canvas plus the layer's pixels) is not ported, because it makes `out_of_range_scatter_is_rejected_and_the_maximum_still_paints` fail (a 1000 % scattered dab lands off the 32 px canvas and reports empty damage) and upstream relaxed that test, which needs a human decision. Until then the budget bounds time but not, in practice, memory: coverage tiles are dense `f32`, so dabs that land off the canvas can still allocate up to about 8 GiB of coverage (2^31 px), and as much again under the Dual Brush's separate budget, before the budget refuses; the clip exists to stop those never-read tiles. Under continuous coverage (a local adaptation) the budget is charged per segment and an undrawn segment owes no end cap. `f5e92a6b`/`0ff76d6f` also bring the per-segment step cap of `cfd894aa` (its procedural-primitive caps are not ported); `af025e18`'s pixel-grid test does not apply (we have no pixel-grid walk). `c2cc5eee` ports the proxy half only: our Liquify dialog has no source-resolution crop or smart-filter editing. `6bd3773e` ports the foreground-precision half; the test it re-pins belongs to upstream's distance-faded protection (`a11204c6`), which we do not have. `acd35e12` is adapted to our Smudge pick-up loop, saturating the square positions. `abeb16c9` also builds `apply_dab_stroke`'s halo rectangle (`crates/pc-paint/src/retouch.rs`) with saturating arithmetic, a local hardening upstream does not have.

Partial ports: `89f20c45` brings only the estimate that holds for every `<use>` copy; its per-test
weighting by attributes scanned and value bytes compared (our `class_work` covers class lists only),
its counting of `:not()` alternatives and of kept declarations toward the 64 MB, its linked-file
sharing and its embedded-SVG budget are not ported. `1ce6431a` turns embedded PNG, JPEG and WebP
only, within one decode allowance shared by every use; its linked files, TIFF data URLs and CMYK
handling have no counterpart. `e6add186` brings the closing drag; its closing click that retracts
the incoming handle and makes a corner is not ported (a click still keeps the first anchor's
handles), and its script `path.close` parameters have no counterpart. `0e64aca5` brings only the
flatten length cap and `9cd997b9` the far-point checks; the 4 pathops call sites that use the
wrapper have no upstream hunk, and upstream's brush `track.rs` and `warp.rs`, metafile `wmf.rs`,
print-layout and script-check hunks are not ported.

Local deviation: `e9510956` and `0a2935cf` refuse a sweep whose tolerance passes one unit; here
only the coordinate rounding term is capped, so a drawing more than about 1e8 units across still
sweeps with a tolerance wider than one unit.

Transform and Crop partial ports: `be7d6816`/`dce32b66` change Free Transform's split only; upstream's
matching Move change (`cut_piece`, #7655) is not ported, so Move still lifts `a * k` where Free
Transform now lifts `min(a, k)`. `c2acddae` brings the Perspective side-handle slide; its Distort
side-handle half (it would change `distort_mode_moves_one_corner_and_ignores_rotate_and_edges`) and
its handle-cursor hunk are not ported. `b800c40c`'s layer-marquee half has no counterpart here.
`bed677b3` counts the Pixels statistic on the UI thread, not on a histogram worker. `abb85588` does
not carry the pivot. `9891eaeb` holds History restore and delete only; its selectState, Step
Forward/Backward and menu greying have no counterpart. `ea8ba0b3` (re-showing the frame) was
reviewed and nothing from it applies: its press change, `show_frame`, is not here.

## RAW, HEIF and metadata hardening (2026-11-15)

Not ported from `b0c4aa8c`: the distinct-byte allocation bound in `check_image`, because the batch test
`hundred_megapixel_rgb_hdr_has_bounded_tile_batches` (`crates/lc-raw/src/tiffraw.rs`) plans its image from tiles
all pointing at one 4096-byte payload, which that bound refuses (whether to keep the test or take the bound is left to Zach); and the sparse planner (no
sparse reader here). Already covered: `a4a797e2`'s tile-sized lossy JPEG budget by `lossy_jpeg`'s tile-bounded
header and its Deflate padding bound by `check_image`'s tile-grid bound; `a42ee704`'s alpha-grid tiles by the
shared extent-read budget; `f786b875`'s XMP path building by `property_key`'s key budget. Not applicable:
`1c906ea3`'s PEF and SRW halves (PEF never sums strip counts here, no SRW decoder) and `cb86952c`'s CR3 half
(CR3 goes through rawler) and `f786b875`'s lensdb XML half (lens profiles come from the `lensfun` crate's
bundled database; we have no lens XML reader). `5d4b68f0`'s craft-heif git dependency is not taken, nor its HEIC colour changes (VUI range and matrix, Apple
chroma upsampling, VUI-based CICP tagging) or its HDR conversion peak: decoding goes through the heic-rs 0.1.4
wrapper, and there is no HDR conversion path here. `a4a797e2`'s second budget in `bound_candidates`, on the total
length of validated ranges, is not ported: candidates stay borrowed slices and only the largest is copied. Not ported
here: `0352b420`'s decode side (the lone or incoherent ICC chunk refusal in `crates/pc-codecs/src/codecs/jpeg.rs`
and the IccUnsupported origin for an invalid profile), left to the package that owns pc-codecs. `0133e139` opens the preview only when
the raw decoder itself reports `RawError::Corrupt`, narrower than upstream's `Malformed`: a raw cut short inside its
TIFF structure (`RawError::Tiff`) still fails, and an editor load that fails while the raw decodes stays an error.
Local extension of
`17248a20`: the Library's embedded-preview paths (`lc-engine` `load_embedded_preview`, `embedded_preview_srgb`) take
the same EXIF colour-space tag, and a preview with EXIF of its own is read by that EXIF, not the raw's.

| Upstream commit | Subject | Our files | Licence |
|---|---|---|---|
| [`b0c4aa8cc8ac`](https://github.com/storytold/lightcraft/commit/b0c4aa8cc8acac043af6d767b3bfec30d0ab0c0a) | security(raw): linear sparse planning, distinct-byte allocation bound, opcode list budget, ORF strip length (#3679) | `crates/lc-raw/src/opcodes.rs`<br>`crates/lc-raw/src/vendor/orf.rs` | MIT OR Apache-2.0 |
| [`a42ee7042fee`](https://github.com/storytold/lightcraft/commit/a42ee7042fee0f18e929b0f54e3c9d7a6369cf43) | security(heif): count alpha-grid tiles and bound hvcC parameter sets before heic-rs (#3645) | `crates/pc-heif/src/lib.rs`<br>`crates/pc-heif/src/pps.rs` | MIT OR Apache-2.0 |
| [`f786b87533e5`](https://github.com/storytold/lightcraft/commit/f786b87533e58c6d31c7f9f2897814904448f39a) | security(meta): linear entity decoding, indexed keyword de-duplication, bounded XMP path building (#3672) | `crates/lc-meta/src/xmp.rs`<br>`crates/lc-meta/src/iptc.rs`<br>`crates/lc-engine/src/preset_luminar.rs` | MIT OR Apache-2.0 |
| [`a4a797e2f2e3`](https://github.com/storytold/lightcraft/commit/a4a797e2f2e381fcb425d78394c0c4dd4f1ab6a8) | security(raw): shared header-scan budget for tagged previews, tile-sized lossy JPEG budget, bound deflate tile padding (#3695) | `crates/lc-raw/src/preview.rs` | MIT OR Apache-2.0 |
| [`8b3ce31b665c`](https://github.com/storytold/lightcraft/commit/8b3ce31b665c16983129649ebcab298e8b893ba7) | Never crash: a UTC offset of i16::MIN formats and validates without overflow (#2579) | `crates/lc-meta/src/datetime.rs` | MIT OR Apache-2.0 |
| [`07d1f0ad5d1e`](https://github.com/storytold/lightcraft/commit/07d1f0ad5d1e2ce71ff5d22749e92893f958c994) | LR-META-XMP: reject incomplete ISO timestamps | `crates/lc-meta/src/datetime.rs` | MIT OR Apache-2.0 |
| [`a1e1b4cc0e37`](https://github.com/storytold/lightcraft/commit/a1e1b4cc0e37a8016b933eba099460ed99b00b4c) | LR-META-EXIF-FULL: reject malformed EXIF clocks | `crates/lc-meta/src/datetime.rs` | MIT OR Apache-2.0 |
| [`84951ff57967`](https://github.com/storytold/lightcraft/commit/84951ff57967f3778a6095663892ac59fb010bee) | Never crash: rational tag lists show 16 values like every other list (#2579) | `crates/lc-meta/src/tags.rs` | MIT OR Apache-2.0 |
| [`1c906ea3db25`](https://github.com/storytold/lightcraft/commit/1c906ea3db250369860ba1357ed1dc50935bbf27) | Never crash: strip byte counts near u64::MAX don't overflow their sum in ORF, SRW, ARW and PEF | `crates/lc-raw/src/vendor/arw.rs`<br>`crates/lc-raw/src/vendor/orf.rs`<br>`crates/lc-raw/src/vendor/mod.rs` | MIT OR Apache-2.0 |
| [`cb86952cbf24`](https://github.com/storytold/lightcraft/commit/cb86952cbf246b5994f5f1116e94e65db63f05c8) | security(raw): CR3 timed maker notes are parsed once per distinct range under a shared budget; Sony quad tiles' frames may not exceed the tile (#3700) | `crates/lc-raw/src/vendor/arw.rs` | MIT OR Apache-2.0 |
| [`0352b420a7df`](https://github.com/storytold/lightcraft/commit/0352b420a7dfbaeea5ea67aaaae7808527b25075) | fix(codecs): validate JPEG ICC APP2 chunk sequences | `crates/lc-meta/src/container.rs` | MIT OR Apache-2.0 |
| [`0133e139bb68`](https://github.com/storytold/photocraft/commit/0133e139bb6857af2d4a2749ffe0ba99a7697e8d) | A raw with damaged sensor data opens its camera JPEG instead of failing | `crates/pc-io/src/raw.rs` | MIT OR Apache-2.0 |
| [`17248a205dd7`](https://github.com/storytold/photocraft/commit/17248a205dd71c6ae12931b53028e4ac0eb918df) | A preview fallback keeps the colour space the raw's EXIF states | `crates/lc-codecs/src/jpeg.rs`<br>`crates/lc-codecs/src/lib.rs`<br>`crates/lc-engine/src/files.rs` | MIT OR Apache-2.0 |
| [`5d4b68f04cfe`](https://github.com/storytold/photocraft/commit/5d4b68f04cfefada1404fb709eb9ca2d3575f303) | Use craft-heif from craft-libs for HEIC/HEIF (#13076) | `crates/pc-codecs/src/codecs/heif.rs` | MIT OR Apache-2.0 |

## LightCraft Develop ports (2026-11-16)

Local adaptations: `47e2c0d2` takes the analysis image from our `analysis_plane`, which keeps the
small-source resampling of `analyze_lines`; upstream's unused `level_plane` is not ported.
`9ac38222` adds only what was missing: Masking's preview, its GPU kernel and `SharpK` existed. The
detail band is our sharpening's (`detail::detail_band`, from the Radius planes `prep.sharp`, with a
GPU twin `sharp_detail_preview`), not upstream's texture blur; `sharpen_mask_weight` and
`texture_sigma` have no counterpart. The UI keeps our per-slider Alt flag (`alt_preview`) instead
of a global Alt state, so an Alt-drag on the histogram shows no preview. Not ported: `per_pixel`
(our loupe has no window render to gate) and the unrelated `spots.rs` cleanup.
`abf0caf2` dispatches through our `take_pressed` loop, which drops each key event it takes, so the
once-per-press check reads the frame's fresh Space press before the loop. Its tests use Clone in
Remove (our Remove and Heal make shared patches, not `develop.spots`), set the zoom directly (we
have no `view.navigate`) and check `app.gesture` for the stroke in progress (we have no
`brush_live`). Space takes no hand over the Edit panel's mask tools or the Skin Tone and film base
pickers, which upstream does not have or list. Not ported: the `ui.key` row of upstream's control
protocol (ours documents the PhotoCraft side) and the parity checklists.
`c9c3870a`: our primary binding (no params) for Toggle Zoom on Space takes the alias's path in the
same loop, slideshow pause included; the test sets `shortcut_overrides` (we have no
`shortcuts::assign`).

| Upstream commit | Subject | Our files | Licence |
|---|---|---|---|
| [`47e2c0d248ad`](https://github.com/storytold/lightcraft/commit/47e2c0d248adf7f555163c0efde8d0f4b2d5485f) | LR-CROP-AUTO: Auto straighten reads the vertical vanishing point and the horizon | `crates/lc-pipeline/src/straighten.rs`<br>`crates/lc-pipeline/src/upright.rs`<br>`crates/lc-pipeline/src/lib.rs`<br>`crates/lc-pipeline/src/tests_geometry.rs` | MIT OR Apache-2.0 |
| [`9ac38222e12e`](https://github.com/storytold/lightcraft/commit/9ac38222e12e9bec2bb171d2a30aa8b69aeb4b9a) | Alt/Option-drag previews for clipping and sharpening | `crates/lc-pipeline/src/visualize.rs`<br>`crates/lc-pipeline/src/detail.rs`<br>`crates/lc-pipeline/src/lib.rs`<br>`crates/lc-gpu/src/render.rs`<br>`crates/lc-gpu/src/ctx.rs`<br>`crates/lc-gpu/src/wgsl/detail.wgsl`<br>`crates/lc-gpu/tests/it/equivalence.rs`<br>`crates/lc-ui-egui/src/state.rs`<br>`crates/lc-ui-egui/src/panels/edit.rs`<br>`crates/lc-ui-egui/src/panels/detail.rs`<br>`crates/lc-ui-egui/src/tests_panels.rs` | MIT OR Apache-2.0 |
| [`abf0caf20170`](https://github.com/storytold/lightcraft/commit/abf0caf2017e79108fa9a7d67efabad27232b269) | KEY-TOGGLEZOOM: hold Space over a tool to pan and click-zoom | `crates/lc-ui-egui/src/space_hand.rs`<br>`crates/lc-ui-egui/src/tests_space_hand.rs`<br>`crates/lc-ui-egui/src/control.rs`<br>`crates/lc-ui-egui/src/lib.rs`<br>`crates/lc-ui-egui/src/shortcuts.rs`<br>`crates/lc-ui-egui/src/panels/detail.rs` | MIT OR Apache-2.0 |
| [`9e305d642e3e`](https://github.com/storytold/lightcraft/commit/9e305d642e3e091235e8a651079f6dafd798c224) | KEY-TOGGLEZOOM: a click down and up within one frame keeps its zoom on Space's release | `crates/lc-ui-egui/src/space_hand.rs`<br>`crates/lc-ui-egui/src/tests_space_hand.rs` | MIT OR Apache-2.0 |
| [`c9c3870ab387`](https://github.com/storytold/lightcraft/commit/c9c3870ab38718a63c5faaa7b677b39cbbc002eb) | KEY-TOGGLEZOOM: Toggle Zoom bound to Space in the keymap takes the hand and toggles once | `crates/lc-ui-egui/src/shortcuts.rs`<br>`crates/lc-ui-egui/src/tests_space_hand.rs` | MIT OR Apache-2.0 |
