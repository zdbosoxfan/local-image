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
