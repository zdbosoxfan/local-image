# Codex report: develop colour and tone

Continued from `aecc1d4` (`WIP: Codex partial work`), including its generated data and schema changes. All changes remain **uncommitted**. Work was offline; Cargo builds/tests/lints ran one at a time. No new dependencies, C/C++ build integration or unsafe Rust were added. The excluded demosaic/highlight, colour-ops, spatial adjustment, mask, pc-* and li-* implementation files were not changed. No desktop app was opened or killed.

## 1. One renderer and old settings

Removed ProcessVersion, the process field/selector/command, v2026 and all rendering branches. Old JSON's unknown `process` field is ignored; missing look/curve fields take serde defaults. The compatibility test and existing historical-settings fixtures load successfully. `crs:ProcessVersion` remains only as an external Adobe XMP field handled by the importer; it does not select a rendering path.

Intentional output changes have new golden hashes below. Settings hashes remain unchanged in the toolset fixtures. Render-cache version is now 13, and source/stage keys include camera calibration, profile contents and baseline gain.

## 2. Three looks and variants

- **Soft Film** (internal JSON id `adobe`): our independent parametric toe/midtone/shoulder calibration; never substitutes a proprietary SDK/ACR table or a DNG profile tone curve.
- **Sigmoid**: darktable's analytic parameter calculation, log-logistic evaluator, negative desaturation, channel order and hue/energy preservation. Default curve plus +0.7 EV exposure; Hue Preservation defaults to 75%.
- **Camera**: attempts a bounded, file-local sensor/JPEG correspondence fit for every decoded RAW format with a usable preview. A calibrated camera keeps its colour matrix and fits tone/chroma; otherwise the existing pooled/fitted colour model can help. The fit now sees DCP-corrected colours and is validated against the actual renderer. Invalid, mismatched or insufficient previews use darktable camera/maker presets, then the generic preset. Neutral-only images can fit tone when colour calibration is known.

Standard, Extra Shadow, High Contrast and Linear are selectable per photo. Linear is identity in scene luminance through 0.8, then has a continuous shoulder. Camera uses a luminance norm with fitted chroma; its hue slider blends toward channel mapping at the same luminance. All tone paths contain gamut excursions smoothly and approach white without a hard knee. Whites/Blacks operate through continuous shaping.

## 3. Matrices, profiles and chosen-white WB

Generated **1302 normalized matrix keys**: 1188 from rawler v0.8.0 and 114 RawTherapee gaps. Available A/D65 matrices are retained independently; missing illuminants are not fabricated. File matrices win over the database. Six-maker A/D65 fixtures independently check exact TOML values and precedence.

The DCP reader accepts bounded profile containers and optional hue/saturation maps, look tables, tone curves, ForwardMatrix, baseline offset and black-render metadata. **55 unmodified public-domain/CC0 DCPs** are bundled, each with embedded rights and SHA-256 in `crates/lc-raw/data/dcp/manifest.json`. Regional filename aliases handle bodies such as EOS 250D / Rebel SL3. Dworak, Dr Slony, unclear-rights and Adobe profiles are excluded.

Settings contains a persistent **Camera profiles folder**. Its reader only reads files, accepts case-insensitive .dcp, bounds file sizes/counts, ignores invalid files and deterministically matches embedded camera models. Tests verify unchanged bytes/mtime/directory contents. Changing the folder replaces the registry atomically and invalidates decoded/rendered profile keys.

The decoded source retains its calibration and an undo matrix. Each chosen white re-derives the camera transform with inverse-CCT interpolation, including ForwardMatrix/CameraCalibration/AnalogBalance. DCP tables are resolved for that white and applied after WB, before look/tone. They are no longer baked irreversibly into the source. Smart previews retain the new source interpretation and still accept old tone-only headers; stored table/curve deserializers reject invalid dimensions and knots.

## 4. Curves, gamut and output

Master/parametric curves have Luminance and RGB modes and independent R/G/B tables in working Rec.2020. Luminance preserves channel ratios; RGB keeps Refine Saturation. Layers blend curves in working linear RGB. Output has soft achromatic-distance compression followed by hue-preserving gamut containment. Same-space soft proofing now matches ordinary output.

Deterministic position-hashed TPDF dithering is shared between CPU and WGSL for 8-bit output, uses the same noise in all channels to keep neutrals neutral, and fades at black/white. Tests check repeatability, neutral equality, bias, endpoints and compression continuity. Precision tests count unique levels after sorting, as dithering need not produce an ordered staircase.

## 5. Upstream reference vectors

`scripts/engine_colour_refvec.py` extracts upstream C/C++ functions and creates/builds/runs harnesses only under ignored `target/refvec/`. Cargo remains all Rust. Committed fixtures and READMEs include offline regeneration commands, pins and tolerances:

| Fixture | Rows | Assertion |
|---|---:|---|
| pipeline sigmoid | 66 | absolute error ≤ 2e-6; parameter sets, extremes, channel ties, negative inputs, hue blend |
| pipeline basecurve | 1023 (31 × 33) | absolute error < 8e-7; exact upstream monotone Hermite |
| color DCP | 48 | absolute hue/saturation/value error < 4e-5; wrapped hue, 2.5D/3D linear-value tables |
| color illuminants | 7 | weight/matrix error < 1e-6; A/D65 endpoints, interior and temperature bounds |
| raw camera matrices | 12 | exact A/D65 coefficients from six maker TOMLs |

All assertions pass. The data generator also records exact path/byte archive hashes in `colour-sources.json`.

## 6. GPU implementation and validation

Tone LUTs, Camera luminance/chroma/hue handling, working-space curves, soft gamut containment and deterministic dither have WGSL/host twins. Chosen-white camera matrices are calculated on the host and uploaded. Nonlinear DCP tables use a **CPU scene-linear stage inside the GPU render** (readback/upload with stage caching); remaining pixel/output work stays on the GPU. This is not a whole-render fallback.

New GPU comparisons (each asserts mean |Δ| < 0.5 LSB and max ≤ 3 LSB):

| Exact test | Coverage |
|---|---|
| `equivalence::colour_tone_looks_bases_hue_and_rolloff` | 73 comparisons: 3 looks × 4 variants × 3 hue settings × neutral/extreme sliders, plus Camera chroma; EV ramps, primaries and negative channels |
| `equivalence::working_curves_gamut_and_dither` | 37 comparisons: raw/rendered × Luminance/RGB × 3 refinement values × 3 output spaces, plus shallow ramp; repeated renders/endpoint checks |
| `toolset::dual_illuminant_wb_and_dcp_inside_gpu_render` | 24 comparisons: profile off/on × 4 temperatures × 3 tints; each also compares shared-cache and fresh GPU renders |

The existing `equivalence::camera_tone_and_relative_wb` now explicitly selects Camera so its four comparisons actually exercise the new Camera path.

**Coordinator hardware review (RTX 5090, Vulkan):** all GPU tests passed except the existing `equivalence::gpu_matches_cpu` case `masks (cpu shapes, ops, amount)` at 720×480: mean **0.0004 LSB**, max **10 LSB**, **0.0087%** of channels above 1 LSB (previously max 1). This includes successful review of the 134 new comparisons above. The masked-dehaze cause and fix are documented below; hardware measurements after that fix are still pending.

This sandbox exposes only llvmpipe, which the GPU path intentionally rejects; adapter-dependent tests print `skipped: no GPU adapter`. Sandbox success for those tests is not a hardware equivalence measurement. `ctx::shader_validation::colour_tone_finish_wgsl_validates_without_an_adapter` independently parses and validates the assembled finish WGSL using Naga.

## 7. Ports, licences and deviations

`docs/PORTS.md`, `licenses/darktable-NOTICE.md`, `licenses/rawler-NOTICE.md` and `licenses/rawtherapee-NOTICE.md` record paths, authors, pinned sources, rights and differences. Rawler data is elected GPL-3 via LGPL-2.1 §3; no RawSpeed/CC-BY-SA data is used.

Explicit deviations/extensions:

- Sigmoid runtime uses the shared 4096-entry log table rather than per-pixel analytic evaluation; black is continuously extended to zero. Default work primaries yield identity transforms; optional darktable primaries controls are not exposed.
- Basecurve polynomials/preset points match upstream; the shoulder above 90% replaces unbounded extrapolation and display clipping to keep highlight headroom. Exposure variants and the Camera same-luminance hue blend are our extensions.
- RT DCP uses analytic sRGB transfer instead of its quantized gamma tables. Reference harness vectors isolate linear encoding; analytic sRGB and headroom extensions have Rust tests. Above-white inputs retain scene headroom. DNG profile tone curves use the Camera soft continuation rather than display clipping.
- `histmatching.cc` was inspected as a robustness reference, but **no code was ported**. Existing spatial correspondences, median bins, isotonic fitting and held-out validation remain; hence no histmatching-port row or false upstream parity claim.
- The supplied rawler archive has **no commit metadata**. It is pinned to v0.8.0, immutable Git tree `ae01bcb2d0f8a74f9dfbb9f7b5c7e315c8e668b7`, and exact TOML archive hash. The tree is not mislabeled as a commit.
- The inline 32-knot BaseCurve intentionally avoids heap allocation (local large-enum lint allowance). Generated calibration coefficients retain exact source values even when they resemble mathematical constants (data-only approx-constant allowance).

## Tests and checks

Initial completion validation, before the coordinator follow-up (plus the changed colour crate):

```sh
~/.cargo/bin/cargo +1.98.1 test --offline -p lightcraft-develop -p lightcraft-pipeline -p lightcraft-gpu -p lightcraft-engine -p lightcraft-raw -p lightcraft-ui-egui -p lightcraft-color -- --nocapture
```

**Passed, exit status 0.** Final output: `target/codex-required-tests.log`; serial catalog output: `target/codex-catalog-serial.log`; lint output: `target/codex-clippy.log`.

| Package | Passed | Ignored | Notes |
|---|---:|---:|---|
| develop | 28 | 0 | old settings/schema compatibility |
| pipeline | 158 | 2 | includes all re-recorded goldens and C reference fixtures |
| engine | 303 | 2 | includes DCP directory, Camera fit, smart-preview and loader goldens |
| gpu | 40 | 2 | unit 11; equivalence 12; fallback 3; memory 1; toolset 13; adapter-dependent work skipped |
| raw | 116 | 2 | unit 87; corpus 7; DNG 14; robust 5; vendor robust 3 |
| ui-egui | 166 | 1 | headless tests; no GUI opened |
| color (additional) | 30 | 0 | DCP and interpolation fixtures |
| catalog (additional, serial) | 73 | 1 | schema consumer regression suite |

Required six-package total: **811 passed, 9 ignored** (includes adapter/corpus-dependent tests that returned early). Additional colour/catalog: 103 passed, 1 ignored. Doc tests pass (no examples). External RAW corpus and CRAFT_FONTS_DIR are absent; corpus/font-specific checks skip. An initial combined run's optional catalog test `tests_lock::second_opener_is_refused_until_the_first_lets_go` failed on immediate lock reacquisition (`InUse(None)`); the full catalog suite passes with `--test-threads=1`. No unrelated lock code was changed.

Formatting passes for all eight touched packages. Rustup Clippy 1.98.1 passes offline for those packages with `--all-targets -- -D warnings` in `target/clippy`. The first lint attempt mixed Fedora rustc dependencies with rustup Clippy; cleaning only the separate lint directory and prepending `~/.cargo/bin` to PATH corrected this.

Reference/data generators run successfully offline and reproduce all 65 checked artifacts byte for byte after rustfmt; every bundled DCP rights/hash entry verifies. `git diff --check` passes. Logs remain under ignored `target/codex-*.log`.

## Re-recorded golden hashes

These replace the old renderer's expected pixels intentionally: one tone path, working-space curves, smooth gamut handling and 8-bit dither. They remain unconditional regression guards on x86_64 Linux, not disabled tests. Toolset settings hashes are unchanged.

### Pipeline toolset

| Case | Before | After |
|---|---|---|
| default/rendered | `11aba23997cf23ea` | `9c4d1d077ec4855f` |
| default/raw | `c2082c862dbb6ee9` | `e4cadb152e11a72a` |
| raw defaults + edits/rendered | `1ff221ea991f326a` | `3ccf1e25fb86c471` |
| raw defaults + edits/raw | `bc9c429bb9ac900a` | `dffc0e02ab440efe` |
| optics + geometry + crop/rendered | `5626a087deaf3da3` | `c0c59a4c8cd4a92a` |
| optics + geometry + crop/raw | `40a3e4fc12596d81` | `ded90c5d0013a5d1` |
| profile look/rendered | `def219e6f08ead62` | `f340243918f5d0d7` |
| profile look/raw | `5718b047fd492719` | `bf9002730b346eb4` |
| b&w profile/rendered | `b0647da755d9d959` | `af0827daa56de037` |
| b&w profile/raw | `bfe5b0881505b6b4` | `1e627cb4c60f4827` |
| negative/rendered | `b71acfaa8b5e6ed7` | `30b5bb6ca4d2340b` |
| negative/raw | `b71acfaa8b5e6ed7` | `d4e80f8f014fb4b0` |
| layers/rendered | `ceba5c62420067ad` | `5d7d052c40911a42` |
| layers/raw | `686f18d0cb9cae25` | `ca6bb1ad16f271e4` |

### Pipeline layers

| Case | Before | After |
|---|---|---|
| default/rendered | `7cf22c2955ef36e6` | `a0f156d3ca352210` |
| default/raw | `f5f205c909712ac8` | `61dfdd1c1479866c` |
| global edits/rendered | `ef47f36286e6278d` | `bf0d6424cd534566` |
| global edits/raw | `59f01695407af702` | `00547bd4de761061` |
| old masks/rendered | `149cc67608a52dad` | `48acc164340c33b2` |
| old masks/raw | `e820b478f381c7b7` | `96a665e1707ccd35` |


Loader source: 200px `a0de2a874e354a66` → `1ce58e40d2f54b2d` because negative matrix channels are retained for downstream tone/gamut handling. 500px `1cc883f39dae2c30` and full-size `96a363c6ffd0f874` are unchanged. No demosaic or highlight reconstruction algorithm was edited.

Existing behavioral assertions were adjusted for working-linear layer blending, workspace rather than output-space saturation refinement, dither-safe unique-level counting/edge measurement, and bounded JPEG training targets. Camera chroma tests still require rendered highlight error to improve by more than 2×.

## Remaining review

The coordinator has completed the initial hardware GPU review. The masked-dehaze fix and its new targeted GPU comparisons still require a hardware rerun; the sandbox cannot measure them. Lightroom/Capture One visual quality is a target, not a demonstrated proprietary-render comparison: no comparative RAW/JPEG corpus or those applications is available here. RAW variants already unsupported by the decoders remain unsupported; they were outside this task.


## Coordinator follow-up: masked dehaze regression

The new scene-linear path retains negative channels for the tone stage's hue-preserving desaturation. The existing 960×640 demo source, Mitchell-resampled to 720×480, contains channels as low as −0.04126. This exposed a discontinuity in the old dehaze branch: **any positive dehaze strength entered a channel-clipping operation even when its computed transmission was exactly 1** (no representable veil removal).

Sky/Subject/Background shapes are evaluated on the CPU in both renderers, but the hybrid renderer supplies GPU-computed image/log-luminance readbacks. Small floating-point differences can change the sign of tiny running-sum blur residues outside Sky's support. A nonpositive alpha skips the adjustment; an arbitrarily small positive alpha selected dehaze and clipped negative channels to zero. The subsequent tone desaturation therefore saw different colours. Background saturation and the final dither made the discrepancy visible in a few output channels. Mask amount, compositing/inversion, adjustment order and dither coordinates were checked and agree; no mask or colour-ops algorithm was changed.

**Fix:** `lc-pipeline/src/finish.rs::dehaze_px` returns the input unchanged when positive dehaze's transmission rounds to 1; `lc-gpu/src/wgsl/finish.wgsl` applies the dehaze/clipping operation only when transmission is below 1. The shared CPU helper also covers layer dehaze. Both paths now leave these negative channels for the same downstream tone/gamut stage. Actual veil removal retains its existing formula. This uses the computed transmission, with no arbitrary alpha cutoff, new fallback, tolerance change, or skipped case. `gpu_matches_cpu` and its existing failing case are unchanged.

Reproduction and regression coverage:

- Before the fix, the new unit-transmission test failed: input `[-0.0041147103, 0.0013219203, 0.0043006097]` became `[0, 0.0013219203, 0.0043006097]` at a positive strength as small as `f32::MIN_POSITIVE`.
- An adapter-free diagnostic using the original scene/three masks and log-luminance perturbations of ±1e-6 reproduced the same rare large errors: old branch max **10 / 12 LSB**, mean **0.000271 / 0.000225 LSB**, 60 channels above 1 LSB in each direction. With the fix, max **1 / 1 LSB**, mean **0.000005 / 0.000007 LSB**, zero channels above 1 LSB. These are CPU perturbation measurements, not RTX measurements. Diagnostic source/logs remain under ignored `target/mask-debug/` and `target/codex-mask-probe-signed-alpha.log`.
- `pipeline::finish::colour_tone_tests::dehaze_unit_transmission_preserves_negative_scene_channels` guards identity at tiny positive strengths and zero dark channel, plus unchanged actual dehaze.
- `pipeline::finish::colour_tone_tests::cpu_shape_roundoff_does_not_clip_unselected_scene_channels` uses the real 720×480 resampled scene, Sky and Background, verifies signed alpha flips on negative pixels, and requires at most 1 LSB under ±1e-6 mask-input perturbations.
- `equivalence::masked_dehaze_unit_transmission_is_identity` adds **15 GPU comparisons** (3 looks × zero/tiny/full opacity), retaining mean < 0.5 LSB and max ≤ 3 LSB. Its CPU identity and nontrivial-dehaze control assertions also run without an adapter.

Follow-up validation completed successfully, with Cargo operations run one at a time:

```sh
~/.cargo/bin/cargo +1.98.1 test --offline -p lightcraft-develop -p lightcraft-pipeline -p lightcraft-gpu -p lightcraft-engine -p lightcraft-raw -p lightcraft-ui-egui -p lightcraft-color -- --nocapture
~/.cargo/bin/cargo +1.98.1 fmt -p lightcraft-pipeline -p lightcraft-gpu -- --check
PATH="$HOME/.cargo/bin:$PATH" CARGO_TARGET_DIR=target/clippy ~/.cargo/bin/cargo +1.98.1 clippy --offline -p lightcraft-pipeline -p lightcraft-gpu --all-targets -- -D warnings
```

All three commands exited 0. Required six-package suite: **814 passed, 9 ignored** (develop 28, pipeline 160, GPU 41, engine 303, raw 116, UI 166); additional color suite: **30 passed**. All doc tests passed. GPU totals include adapter-dependent tests returning early; the new masked-dehaze test's CPU assertions and Naga finish-shader validation actually ran. Existing golden hashes all passed without any follow-up re-recording. `git diff --check` passed. Logs: `target/codex-mask-required-tests.log`, `target/codex-mask-clippy.log`; the deliberate pre-fix regression failure is in `target/codex-mask-regression-before.log`.

No commit was made. Post-fix RTX 5090 validation of the unchanged failing case and the 15 new targeted comparisons remains for the coordinator.
