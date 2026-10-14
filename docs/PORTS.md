# Ported algorithms

Code in this repository that is a port (a re-implementation, usually in Rust) of an algorithm from
another project. Each ported file also names its source in its own module documentation, and each
upstream project has a notice in [`licenses/`](../licenses).

To add a port: add one row per ported upstream file (several rows may share our file), with the
full commit hash the port was made from (`git log -1 --format=%H -- <upstream path>` in a clone),
the upstream licence as an SPDX identifier, and the date of the port (YYYY-MM-DD). When a port is
updated to a newer upstream commit, update its row (commit and date) rather than adding another.

| Our file | Upstream project | Upstream path | Upstream commit | Licence | Date | Used for |
|---|---|---|---|---|---|---|
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
| `crates/lc-pipeline/src/lensdb.rs` | [LensFun](https://github.com/lensfun/lensfun) (via the [`lensfun`](https://crates.io/crates/lensfun) crate 0.7.0) | `libs/lensfun/mod-coord.cpp`, `mod-subpix.cpp`, `mod-color.cpp` (correction models) | `43f9e001ab66c4fcdd4f400463f13b72f94b2288` | LGPL-3.0-or-later | 2026-10-09 |
| `crates/lc-engine/src/lens_db.rs` | [LensFun](https://github.com/lensfun/lensfun) (via the [`lensfun`](https://crates.io/crates/lensfun) crate 0.7.0) | `libs/lensfun/mod-coord.cpp`, `mod-subpix.cpp`, `mod-color.cpp` (`rescale_polynomial_coefficients`), `modifier.cpp` | `43f9e001ab66c4fcdd4f400463f13b72f94b2288` | LGPL-3.0-or-later | 2026-10-09 |
| `crates/lc-pipeline/src/tone2.rs`, `crates/lc-gpu/src/wgsl/finish.wgsl` | [darktable](https://github.com/darktable-org/darktable) | `src/iop/sigmoid.c` | `733bd69f32cac7ff5e41025115942772add1f088` | GPL-3.0-or-later | 2026-10-09 |
| `crates/lc-pipeline/src/base_curve_data.rs`, `basecurves.rs` | [darktable](https://github.com/darktable-org/darktable) | `src/iop/basecurve.c` | `733bd69f32cac7ff5e41025115942772add1f088` | GPL-3.0-or-later | 2026-10-09 |
| `crates/lc-pipeline/src/basecurves.rs` | [darktable](https://github.com/darktable-org/darktable) | `src/common/curve_tools.c` | `733bd69f32cac7ff5e41025115942772add1f088` | GPL-3.0-or-later | 2026-10-09 |
| `crates/lc-raw/src/camera_matrices.rs` | [rawler](https://github.com/dnglab/dnglab) | `rawler/data/cameras/**/*.toml` | v0.8.0; immutable tree `ae01bcb2d0f8a74f9dfbb9f7b5c7e315c8e668b7` (offline archive has no commit metadata) | LGPL-2.1-only → GPL-3.0-or-later via §3 | 2026-10-09 |
| `crates/lc-raw/src/camera_matrices.rs` | [RawTherapee](https://github.com/RawTherapee/RawTherapee) | `rtengine/camconst.json` | `5f486d3678b34c74ba0c63571c17babe20935019` | GPL-3.0-or-later | 2026-10-09 |
| `crates/lc-color/src/profile.rs`, `camera.rs` | [RawTherapee](https://github.com/RawTherapee/RawTherapee) | `rtengine/dcp.cc` (table interpolation, matrix blending) | `5f486d3678b34c74ba0c63571c17babe20935019` | GPL-3.0-or-later | 2026-10-09 |
| `crates/lc-raw/data/dcp/` | [RawTherapee](https://github.com/RawTherapee/RawTherapee) | `rtdata/dcpprofiles/` (explicit embedded-rights allowlist) | `5f486d3678b34c74ba0c63571c17babe20935019` | CC0-1.0 / public domain; per-file rights + SHA-256 in manifest | 2026-10-09 |
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
[The raw quality report](wip/CODEX-REPORT.md) records tolerances, measured discrepancies,
intentional host differences, synthetic quality metrics, timings and remaining reference limitations.

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
| `llf.rs` | `common/locallaplacian.c` | Same padded reductions, remap, interpolation and reconstruction. Tiny images return identity; the unused fourth SIMD lane is bounded rather than reading beyond the row. Weighted log detail extension subtracts weighted identity. |
| `eigf.rs` | `common/eigf.h`, `gaussian.c`, `fast_guided_filter.h` | Quantized/self-guided paths, Deriche endpoints/clamps, upstream bilinear coordinates, normalized variance/covariance and linear/geometric final blend. Zero-size downsample protection. |
| `balance.rs` | `iop/colorbalancergb.c`, `common/math.h` | Full selected pixel algorithm: Yrg chroma/vibrance, masked wheels, polynomial power, UCS and alternate Jz saturation/brilliance, hue rotation and gamut mapping. LR slider calibration is our own. Jz PQ is evaluated in f64 to avoid neutral cancellation; C HDR vectors check it. |
| `colorequal.rs` | `iop/colorequal.c`, `iop/choleski.h` | UCS chromaticity prefilter, complete 2×2 covariance, correction filter, saturation weights, Scharr halo suppression and periodic cosine RBF. LR eight hue nodes replace upstream's evenly spaced nodes. The f64 direct solve avoids squaring the matrix condition number and is fixture checked. |
| `toneeq.rs` | `iop/toneequal.c`, `common/luminance_mask.h`, `common/eigf.h` | The formerly simplified log mask now uses compensated linear norm, quantized EIGF and geometric blend. Existing fitted/interpolated exposure curve remains. |
| `primary.rs` | vkdt `llap/reduce.comp`, `assemble.comp`, sampled Laplacian construction | vkdt `afc34256fb22bcaf6dcf6503d19d5f9a6f477af5`, BSD-2-Clause. Ten gamma samples; log-EV remap, Hermite tone bands, clamp-edge rectangular pyramid to 1 px, fixed 2 MP proxy, guided gain upsampling and sensor clipping protection are LI extensions. |

See [vkdt notice](../licenses/vkdt-NOTICE.md), [darktable notice](../licenses/darktable-NOTICE.md),
[reference fixtures](../crates/lc-pipeline/tests/fixtures/sliders/README.md), and
[slider report](wip/CODEX-REPORT-sliders.md). Skin Tone is our own UCS low-frequency
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
| `crates/pc-affinity/src/container.rs` | [VectorCraft](https://github.com/storytold/vectorcraft) | `crates/affinity/src/container.rs` | `d522c1d7be4035bd4f4a84cd6ebfca44f5155092` | MIT OR Apache-2.0 | 2026-10-10 |
| `crates/pc-affinity/src/geometry.rs` | [VectorCraft](https://github.com/storytold/vectorcraft) | `crates/affinity/src/geometry.rs` | `d522c1d7be4035bd4f4a84cd6ebfca44f5155092` | MIT OR Apache-2.0 | 2026-10-10 |
| `crates/pc-affinity/src/lib.rs` | [VectorCraft](https://github.com/storytold/vectorcraft) | `crates/affinity/src/lib.rs` | `d522c1d7be4035bd4f4a84cd6ebfca44f5155092` | MIT OR Apache-2.0 | 2026-10-10 |
| `crates/pc-affinity/src/model.rs` | [VectorCraft](https://github.com/storytold/vectorcraft) | `crates/affinity/src/model.rs` | `d522c1d7be4035bd4f4a84cd6ebfca44f5155092` | MIT OR Apache-2.0 | 2026-10-10 |
| `crates/pc-affinity/src/paint.rs` | [VectorCraft](https://github.com/storytold/vectorcraft) | `crates/affinity/src/paint.rs` | `d522c1d7be4035bd4f4a84cd6ebfca44f5155092` | MIT OR Apache-2.0 | 2026-10-10 |
| `crates/pc-affinity/src/preview.rs` | [VectorCraft](https://github.com/storytold/vectorcraft) | `crates/affinity/src/preview.rs` | `d522c1d7be4035bd4f4a84cd6ebfca44f5155092` | MIT OR Apache-2.0 | 2026-10-10 |
| `crates/pc-affinity/src/raster.rs` | [VectorCraft](https://github.com/storytold/vectorcraft) | `crates/affinity/src/raster.rs` | `d522c1d7be4035bd4f4a84cd6ebfca44f5155092` | MIT OR Apache-2.0 | 2026-10-10 |
| `crates/pc-affinity/src/shapes.rs` | [VectorCraft](https://github.com/storytold/vectorcraft) | `crates/affinity/src/shapes.rs` | `d522c1d7be4035bd4f4a84cd6ebfca44f5155092` | MIT OR Apache-2.0 | 2026-10-10 |
| `crates/pc-affinity/src/stream.rs` | [VectorCraft](https://github.com/storytold/vectorcraft) | `crates/affinity/src/stream.rs` | `d522c1d7be4035bd4f4a84cd6ebfca44f5155092` | MIT OR Apache-2.0 | 2026-10-10 |
| `crates/pc-affinity/src/synth.rs` | [VectorCraft](https://github.com/storytold/vectorcraft) | `crates/affinity/src/synth.rs` | `d522c1d7be4035bd4f4a84cd6ebfca44f5155092` | MIT OR Apache-2.0 | 2026-10-10 |
| `crates/pc-affinity/src/tests.rs` | [VectorCraft](https://github.com/storytold/vectorcraft) | `crates/affinity/src/tests.rs` | `d522c1d7be4035bd4f4a84cd6ebfca44f5155092` | MIT OR Apache-2.0 | 2026-10-10 |
| `crates/pc-affinity/src/text.rs` | [VectorCraft](https://github.com/storytold/vectorcraft) | `crates/affinity/src/text.rs` | `d522c1d7be4035bd4f4a84cd6ebfca44f5155092` | MIT OR Apache-2.0 | 2026-10-10 |
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
(per-item notes in `docs/wip/UPSTREAM-PORT-up-*.md` and `docs/wip/FIXUP-fu-core.md`). Each row is
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
| `crates/lc-fetch/src/lib.rs` | [LightCraft](https://github.com/storytold/lightcraft) | `crates/fetch/src/lib.rs` | `4b57d5dc5a111f761e5a7abf0e1ae6cf6baebe3f` | Apache-2.0 | 2026-10-13 |
| `crates/lc-fetch/src/tests.rs` | [LightCraft](https://github.com/storytold/lightcraft) | `crates/fetch/src/tests.rs` | `4b57d5dc5a111f761e5a7abf0e1ae6cf6baebe3f` | Apache-2.0 | 2026-10-13 |
| `crates/lc-fetch/src/http.rs` | [LightCraft](https://github.com/storytold/lightcraft) | `crates/fetch/src/http.rs` | `8dd3afce51d27d2625f8e094e6dd11b713cede01` | Apache-2.0 | 2026-10-13 |
| `crates/lc-fetch/src/lib.rs` | [LightCraft](https://github.com/storytold/lightcraft) | `crates/fetch/src/lib.rs` | `8dd3afce51d27d2625f8e094e6dd11b713cede01` | Apache-2.0 | 2026-10-13 |
| `crates/lc-fetch/src/tests.rs` | [LightCraft](https://github.com/storytold/lightcraft) | `crates/fetch/src/tests.rs` | `8dd3afce51d27d2625f8e094e6dd11b713cede01` | Apache-2.0 | 2026-10-13 |
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
in the offline quality example only. See `docs/wip/CODEX-REPORT-demosaic.md` for measured choices.
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
