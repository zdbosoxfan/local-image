# J28 — one dense `Frame<T>` (D3), Claude builder

Status: complete. Checks, fmt and strict clippy on every touched package pass; the 9 new j28
tests pass. One commit on `wip/cl-j28-dense-frame`. No helper subagents were used.

## What changed for the user

AI Remove (on-device LaMa, Klein, Qwen) and Generative Fill no longer round a 16-bit or float
document to 8 bits on the way to the model and back. The editor now sends the composite to
`li-ai` as a float frame; the on-device LaMa stays in float from the document to the new layer;
the ComfyUI engines quantize only the PNG they upload, then blend the returned patch against the
unquantized document. Smooth gradients (skies, studio backdrops) repaired in a 16-bit file keep
their 16-bit steps instead of showing 8-bit bands, and the repair meets the surrounding pixels
more closely.

Results from 8-bit documents are unchanged (byte-identical, or at most 1 level where a soft
edge is now rounded once instead of twice). See "Measured" below.

## What merged

1. **`li-raster::frame`** (`crates/li-raster/src/frame.rs`, re-exported from the crate root):
   `Frame<T>` (dense row-major; `RgbFrame`, `RgbaFrame`, `GrayFrame`), the shared `Edge` modes
   with `Edge::resolve`, `quantize_u8`/`quantize_u16` (the only quantizers for frame wires),
   map / map_in_place / zip_map / crop / paste, resize / half / gaussian on the J1 kernels
   (shared `Pixel` trait, premultiplied alpha), premultiply / unpremultiply / over-background,
   and exact `image`-crate adapters (u8 and u16 both ways; `from_dynamic` keeps 16-bit and
   float decodes). The adapters divide (`v / 255.0`), so they are bit-identical to the
   conversions they replace.
2. **`pc-algo::image`**: `Edge` is now `li_raster::frame::Edge` (same variants and derives, so
   every caller compiles unchanged); `Image::get_edge` resolves through `Edge::resolve`;
   `Image::to_frame::<N>()` and `Image::from_frame` move editor regions to and from the shared
   frame. The struct keeps its public `rect/ch/data` fields: well over a hundred editor sites build or read
   them directly, so a structural wrapper would be a large, unrelated churn.
3. **`li-seg::lama`**: the model closure takes and returns `Frame<[f32;3]>` (the ONNX model
   was always float; its output was being rounded to u8). Downsampling to the 512 model input,
   the upsample back and the coverage blend now run in float on the shared Lanczos
   (`li_raster`) instead of `image::imageops` on 8-bit samples. New `run_frame` /
   `run_patch_frame`; `run` / `run_patch` are 8-bit adapters. `Denoiser::denoise_frame` is a
   thin frame forward of the existing float denoiser.
4. **`li-ai`**: `remove_patch_frame`, `remove_objects_frame`, `fill_frame`;
   `imaging::poisson_blend_frame` (the solver was already float; it now takes the unquantized
   original and returns unrounded samples) and `imaging::decode_frame` (wire decode at the
   PNG's own bit depth). `remove_patch` / `remove_objects` / `fill` / `poisson_blend` are
   8-bit adapters with unchanged signatures, so Develop's `apps/local-image/src/develop_ai.rs`
   and every UI caller compile untouched. `li_ai` re-exports `Frame`, `RgbFrame`, `RgbaFrame`,
   `GrayFrame`, `Filter`.
5. **Editor boundary** (`pc-engine::ai_cmds`, outside edit): `composite_frame` (float
   composite over white; `composite_rgb` is now its 8-bit adapter with identical bytes),
   `surface_from` takes an `RgbFrame`, new public `layer_from_frame` (`layer_from_rgba`
   forwards). `ai.remove` and `ai.generativeFill` use the frame APIs; `ai.generateBackground`
   converts its 8-bit result once.

## Measured (deterministic synthetic data; no owner images)

| Gate | Before | After |
|---|---:|---:|
| Editor AI boundary, 16-bit doc, deep gradient, identity model: max error | 128.0 16-bit levels | 0.00 |
| Same, float doc | 128.6 | 0.34 |
| Same: distinct red levels (source has 512) | 6 | 512 |
| LaMa patch, identity model, 400×300 soft hole: max error | 128.5 16-bit levels | 0.00 |
| Same: distinct red levels in the patch | 2 | 119 |
| LaMa patch, 1400×1050 (crop resampled to 512 and back): max error | 131.2 | 1.34 |
| Same: distinct red levels | 4 | 573 |
| Poisson seam vs 16-bit document (8-bit generated patch) | 222.3 16-bit levels | 139.7 |
| Poisson 8-bit adapter vs verbatim pre-J28 blend (3 sizes) | — | 0 samples differ |
| LaMa 8-bit `run_patch` vs pre-J28, no resampling | — | byte-identical |
| LaMa 8-bit `run` vs pre-J28, soft coverage | — | ≤ 1 level (one rounding instead of two) |
| LaMa 8-bit `run_patch` vs pre-J28, resampled crop | — | max 1 level, mean 0.047 levels |
| Pixels outside coverage (all paths) | — | byte-/bit-identical |
| `composite_frame(..).to_rgb8()` vs old `composite_rgb` (wire bytes) | — | identical |

The remaining Poisson seam error is the 8-bit content of ComfyUI's PNG reply itself; it can only
go lower if the ComfyUI graphs return 16-bit images (`decode_frame` already accepts them).

Speed / memory: the editor AI jobs now hold the region as `RgbFrame` (12 bytes/pixel) instead of
`RgbImage` (3 bytes/pixel) while the job runs; peak during `composite_frame` is the 16 B/px
render buffer plus 12 B/px (was plus 3 B/px). LaMa's work is bounded by the 512² model plane
and the context crop, as before. No timing benchmark was run (owner's minimum-testing rule);
the per-pixel work is the same order (one conversion per sample, float resampler from J1).

## Tests (all new, filtered by `j28_`)

`cargo test --offline -p li-raster -p photocraft-algo -p li-seg -p li-ai -p photocraft-engine --lib j28_ -- --nocapture`
→ 9 passed, 0 failed:

- `li_raster::frame::tests::j28_frame_edges_match_the_editor_policy`,
  `j28_frame_u8_u16_round_trips_are_exact`, `j28_frame_resize_crop_paste_and_over`
- `photocraft_algo::image::j28_tests::j28_shared_edge_matches_previous_editor_reads` (golden:
  the previous inline edge policy, bit-compared over all modes)
- `li_seg::lama::tests::j28_lama_u8_adapters_match_the_pre_j28_path`,
  `j28_lama_float_patch_has_no_8bit_step` (golden: verbatim copy of the pre-J28 8-bit path)
- `li_ai::imaging::j28_tests::j28_poisson_u8_adapter_matches_the_pre_j28_blend`,
  `j28_poisson_float_meets_the_unquantized_document` (golden: verbatim pre-J28 blend)
- `photocraft_engine::ai_cmds::j28_tests::j28_ai_boundary_keeps_16bit_and_float_documents_out_of_8bit`

Other checks: `cargo check --all-targets` on the five touched packages; `cargo fmt` on them;
`cargo clippy --offline -p <pkg> --all-targets -- -D warnings` for li-raster, photocraft-algo,
li-seg, li-ai, photocraft-engine — all clean. No full suites, no GPU, no benchmarks.

## TEST-CHANGED (call sites only; no assertion changed or removed)

- `TEST-CHANGED: crates/li-seg/src/lama/tests.rs::fill` — the fake model helper returns a float
  `Frame` with the same colours (`[20,40,60]`/`[240,240,240]` / 255) because the model closure
  is now float. All assertions in the tests using it are unchanged.
- `TEST-CHANGED: crates/li-seg/src/lama/tests.rs::lama_thin_downsampled_mask_and_aspect_are_preserved`
  — calls `prepare(&Frame::from_rgb8(&rgb), &mask)`; assertions unchanged.
- `TEST-CHANGED: crates/li-ai/src/ops.rs::final_review_qwen_remove_blends_the_boundary_and_preserves_unmasked_pixels`
  — calls the private `remove_qwen` with `RgbFrame::from_rgb8(&src)` and converts the result
  with `.to_rgb8()`; assertions unchanged.

(These existing tests were not re-run, per the minimum-testing rule; the new golden tests cover
the same code paths.)

## Outside edits

- `crates/pc-engine/src/ai_cmds.rs` — the editor side of the AI boundary (required for the
  user-visible goal); private helpers changed, `layer_from_frame` added, no public signature
  removed or changed. Uses `li_ai::{RgbFrame, RgbaFrame}` re-exports, so pc-engine's
  `Cargo.toml` is untouched.
- `crates/li-seg/Cargo.toml` + `Cargo.lock` — li-seg now depends on the local `li-raster`
  crate (one lockfile edge; no registry packages, no C/-sys).
- `crates/li-seg/src/denoise.rs` — one 5-line `denoise_frame` forward.

## Left, and why

- **Develop AI Remove** still crosses 8 bits: `lc-engine::enhance::RemoveRequest` carries
  `Vec<[u8;3]>` (encoded through `AiView`) and `apps/local-image/src/develop_ai.rs` calls the
  8-bit `remove_patch`. Moving it to `remove_patch_frame` needs the `RemoveRequest` /
  `RemoveResult` / `AiHost` types in lc-engine (and its mock hosts) to carry floats — outside
  this job's files. It is a mechanical follow-up now that `remove_patch_frame` exists.
- **`lc-raster::Image<T>` → `Frame<T>` rename**: `Rgb32f`/`Rgba8` have inherent methods
  (`to_srgb8`, `luminance`, `sample_bilinear`, EXIF `oriented` via `lightcraft_geom`) used across
  the lc-* crates; making `Image` an alias of the foreign `Frame` needs those to become
  extension traits at every call site. Not in this job's file list; the types already share
  layout (`width/height/data`), so the move is mechanical.
- **pc-algo `Image` as a structural wrapper**: kept its public `rect/ch/data` fields (dynamic
  channel count, 100+ construction/read sites); it shares `Edge` and converts to/from `Frame`.
- **AI Enhance / Restoration / cutout**: Enhance is ComfyUI-only (CPU inference disabled) and
  produces a new 8-bit sRGB document from an 8-bit wire, so there is no 16-bit document to
  protect; restoration (`li-seg::restoration`) and cutout mattes still use `RgbaImage` /
  `GrayImage` (mattes are stored as 8-bit masks anyway). Candidates for a later slice with the
  same adapters.
- The ComfyUI graphs return 8-bit PNG; `decode_frame` is ready if they are switched to 16-bit.
