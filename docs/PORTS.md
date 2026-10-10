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
| `crates/lc-pipeline/src/primary.rs`, `crates/lc-gpu/src/wgsl/primary.wgsl` | [vkdt](https://github.com/hanatos/vkdt) | `src/pipe/modules/llap/reduce.comp`, `assemble.comp` | `afc34256fb22bcaf6dcf6503d19d5f9a6f477af5` | BSD-2-Clause | 2026-10-09 |
| `crates/li-seg/src/faces.rs` | [OpenCV](https://github.com/opencv/opencv) | `modules/objdetect/src/face_detect.cpp` | `52100328d82d0502534323e9524a701baa3a1e2a` | Apache-2.0 | 2026-10-09 |
| `crates/li-seg/src/faces/geometry.rs` | [OpenCV](https://github.com/opencv/opencv) | `modules/objdetect/src/face_recognize.cpp` | `13c571a801ad5c67a752e5cd58a8a7e7725f99d2` | Apache-2.0 | 2026-10-09 |
| `crates/li-seg/src/bpe.rs` | [Local Image / LightCraft](https://github.com/zdbosoxfan/local-image) (Apache-2.0 tokenizer port from Hugging Face Transformers) | `crates/lc-segment/src/tokenizer.rs` | `c18d18cf613422fe08d31001e1f803cf8ef7dd68` | Apache-2.0 | 2026-10-09 |
| `crates/li-seg/src/unigram.rs` | [Hugging Face Transformers](https://github.com/huggingface/transformers) | `src/transformers/models/siglip/tokenization_siglip.py` | `cba5c4a6fcd0f6b73bf591505337ccf08ded3550` | Apache-2.0 | 2026-10-10 |
| `crates/li-seg/src/unigram.rs` | [transformers.js](https://github.com/huggingface/transformers.js) | `src/tokenizers.js` (Unigram lattice) | `da2688626d7812ad1ea47fd304c2072cc685051b` | Apache-2.0 | 2026-10-10 |
| `crates/li-seg/src/unigram.rs` | [spm_precompiled](https://github.com/huggingface/spm_precompiled) | `src/lib.rs` (Darts charsmap format) | `c00612e48dda9b8b31a5d2d09530f5caa666d5c0` | Apache-2.0 | 2026-10-10 |
| `crates/li-seg/src/siglip.rs` | [Hugging Face Transformers](https://github.com/huggingface/transformers) | `src/transformers/models/siglip/image_processing_siglip.py` | `cba5c4a6fcd0f6b73bf591505337ccf08ded3550` | Apache-2.0 | 2026-10-10 |
| `crates/li-seg/src/siglip.rs` | [Pillow](https://github.com/python-pillow/Pillow) | `src/libImaging/Resample.c` | `bb1d8e8ab8d29048624d96e3ee53cecf7c13d13d` | MIT-CMU | 2026-10-10 |
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
