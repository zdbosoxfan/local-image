# Develop Detail task report — 2026-10-09

Changes remain uncommitted. Work used Cargo 1.98.1 offline, with one Cargo invocation
at a time. No dependencies or unsafe Rust were added. No GUI was opened or stopped.
The raw kernels, `colorops.rs`, `masks.rs`, and Highlights/Shadows/Clarity/Texture
algorithms were preserved. Their shared finish plumbing was adjusted to separate
sharpening from Texture; the Texture formula is unchanged.

**Scope conflict awaiting owner clarification:** the requested deletion of the
`li-seg` Denoise API leaves two obsolete references in
`crates/pc-ui-egui/src/ai_ui.rs:265` and `:280`. The task explicitly forbids changing
`crates/pc-*`, so that file is untouched. A minimal deletion patch is prepared at
`/tmp/detail-pc-download-cleanup.patch`. The six required packages build and pass;
the combined desktop app needs this cleanup before it can build against the removed
API. No dormant Denoise implementation or compatibility stub was kept to hide this.

## 1. One renderer and intentional goldens

Removed `ProcessVersion`, the remaining helpers and all `v2026()` branches, including
legacy NR, dehaze and sharpening code/kernels. The new Detail renderer is unconditional.
Old JSON's `process`, `enhance.denoise` and `enhance.ai` fields are ignored; missing
fields retain serde defaults. Compatibility tests cover both old process names and
malformed/obsolete AI references. Render-cache version increased from 13 to 14.
NR stage keys include Contrast, section enable state and sensor scale; sharpening
planes are keyed by radius. Layer sharpening now uses each layer's own Radius.

Re-recorded pixel hashes on x86_64 Linux:

| Fixture | Case | Before | After |
|---|---|---|---|
| pipeline toolset | raw defaults + edits/rendered | `3ccf1e25fb86c471` | `f2364afc56909ea9` |
| pipeline toolset | raw defaults + edits/raw | `dffc0e02ab440efe` | `c5693c7dca173f4d` |
| pipeline layers | global edits/rendered | `bf0d6424cd534566` | `de4f12b360deb639` |
| pipeline layers | global edits/raw | `00547bd4de761061` | `57ebbae81d3c021a` |
| pipeline layers | old masks/rendered | `48acc164340c33b2` | `f6cacf8f24c584fe` |
| pipeline layers | old masks/raw | `96a665e1707ccd35` | `2db505da57c14063` |

These fixtures use sharpening, NR, dehaze or local Sharpness whose old behavior was
intentionally replaced. All other toolset/layer pixel hashes remain unchanged.
Removing the always-serialized `enhance.denoise: 0` field changes every toolset
settings hash (same hash for its raw/rendered pair):

| Settings case | Before | After |
|---|---|---|
| default | `12a719f1bc9181a3` | `97d20305a4beb718` |
| raw defaults + edits | `4de6ce922ddbca2c` | `15c51fec88c0f59f` |
| optics + geometry + crop | `c35ab3adf97809b0` | `0d608f6981034f4f` |
| profile look | `2e1a0b037741d7c4` | `67aea33bdcc2fa0b` |
| B&W profile | `e03778c72eb700fb` | `b6159ee7a2d437b0` |
| negative | `6c1203e7c3fdbcb3` | `b00a5805f4d46110` |
| layers | `47486480d432991f` | `72a3db4f1a1900ec` |

The engine's three raw-loader hashes were checked and remain unchanged:
200 px `1ce58e40d2f54b2d`, 500 px `1cc883f39dae2c30`, full size
`96a363c6ffd0f874`. Detail runs after source decoding, so no source golden was
changed merely to satisfy the task.

## 2. Sharpening

Completed pixel-scale log-luminance USM, two small-radius Gaussian blurs, Detail's
fine-band term, local 3x3 extrema halo suppression and smoothed-gradient Masking.
Halo retention was reduced from the interim 15–100% to 3–15%; excessive wide-radius
overshoot is removed while Amount and Detail retain useful frequency response.
Original-pixel scaling includes crop/output geometry and binned-raw sensor scale.
Global/local Sharpness and develop-layer Radius work. The CPU and GPU Alt Masking
overlay remains available even at subpixel preview scales and Amount zero; the UI
shows it during Alt-drag and ends it with the drag.

Measured slanted-edge MTF50 in cycles/pixel, Amount 100 / Detail 100 / Masking 0:

| Radius | MTF50 | Overshoot of step |
|---|---:|---:|
| none | 0.189 | 0.3% |
| 0.5 | 0.216 | 0.3% |
| 1 | 0.296 | 0.7% |
| 2 | 0.373 | 4.7% |
| 3 | 0.404 | 8.8% |

At Amount 120 / Radius 1, Detail 0/100 gives MTF50 0.283/0.311 and overshoot
0.3/0.8%. Flat-area noise sigma is 0.94 LSB unsharpened, 1.13 at Masking 0,
and 0.94 at Masking 100. Sharpening effect after downscaling full size is 2.60 LSB,
versus 2.75 in the correctly scaled preview (3.87 using an incorrect 1.5x radius).

## 3. Wavelet noise reduction

Replaced interim bilateral/guided NR with pinned darktable Y0U0V0 wavelets:
newer VST forward/inverse, exact conversion matrices, 5x5 edge-aware B3 decomposition,
`fast_mexp2f`, BayesShrink thresholds and soft-threshold synthesis. No camera database
is shipped. A robust tile MAD on 2x2 diagonal high-pass samples estimates Poisson
noise slope, with the generic profile fallback and zero read-noise intercept.

All six sliders are used. Amount maps to upstream strength; Detail controls fine
wavelet bands, Contrast controls mid/coarse luminance bands, and Colour Smoothness
controls coarse chroma bands. Preview scale shifts bands to original-pixel scale.
The exact formulas are in `detail/nr.rs` and `licenses/darktable-NOTICE.md`.

Documented host extensions: deterministic wide variance sums; safe clamped taps on
tiny images; colour-only NR preserves Rec.2020 luminance; luminance-only preserves
RGB ratios. Neutral generic/manual shadow bias avoids an uncalibrated camera-bias
inference that shifted flat-image brightness by 0.012. The existing noise/mean test
passes with the neutral bias. Numerical kernels themselves match upstream fixtures.

Quality measurements: flat luminance noise 2.20 → 0.55 LSB; increasing Contrast
0 → 100 raises texture correlation 0.803 → 0.843 while retaining the same flat
noise level. Shadow noise at +3 EV is 5.69 → 0.54 LSB. Colour-edge noise is
13.46 → 1.57 LSB; edge colour leakage is 3.1% (test bound 8%). NR preview versus
downscaled full size differs by 0.72 LSB, against 0.69 from resampling alone.

## 4. Dehaze

Ported darktable's modern airlight quantile ordering, dark-channel morphology,
RGB-guided transmission refinement, covariance/Cramer's solve, cropped Kahan box
means and scene-linear inversion. Both slider signs are supported and cached.
Dehaze/100 maps to strength and its absolute value to distance. Linearity permits
filtering normalized dark-channel planes once per source/scale, then changing the
strength without recomputing guidance. Zero airlight is guarded for black images;
airlight summation uses f64. No scene-linear output clipping is added.

On a strong tower/sky edge, contrast rises from 61.1 to 87.6 LSB at Dehaze 70;
maximum sky-ring excursion remains 1.1 LSB, identical to the source (bound 3 LSB).
Negative Dehaze reduces contrast. Preview error is 0.67 LSB versus 0.51 for
resampling alone. Tiny, black, negative and HDR inputs remain finite.

## 5. AI Denoise removal

Deleted `li-seg/src/denoise.rs`, the RawNIND model entry and Denoise task variant;
deleted the engine denoiser, jobs, commands, source substitution, settings fields,
host download API and UI controls/translations. Removed Denoise-specific tests,
`darktable-ai-NOTICE.md`, its ports row and current product documentation claims.
AI Remove remains: stored patches, source-bound validation, progress/cancel,
Regenerate, undo and GPU patch rendering. Its engine/headless tests pass.
The excluded Compositing download branch is the scope conflict described above.

## 6. Independent reference vectors

Pinned darktable commit: `733bd69f32cac7ff5e41025115942772add1f088`.
Standalone extracted numeric C/C++ harnesses run under ignored
`target/refvec/{nr,haze}` using gcc/g++ with `-O2 -ffp-contract=off`. Cargo neither
runs nor links them. Only 122 NR CSV rows, 106 haze CSV rows, the regeneration
script and README are tracked. Re-running extraction produced identical checksums:

- NR: `ef92180ad04f4919ced50e50030bf0a0dd6109b1164abaef83a106a3daa33cde`
- haze: `78570057ee6b2f47eb6b5b3b7a62a1a3cae1eee65baf4a8f766d6bee4be945fd`

| Reference comparison | Measured maximum absolute difference | Test tolerance |
|---|---:|---|
| conversion matrices / VST | 0 (printed to 8 decimals) | 1e-6 / forward 2e-4 / inverse 2e-6 |
| historical fast exponent | bit-exact | exact |
| EAW coarse/detail and synthesized bands | 0 (printed to 8 decimals) | 2e-5 / 3e-5 |
| band squared sums / thresholds | 0.00089264 | relative 1e-4, magnitude floor 1 |
| ambient/depth, guided transmission/output, box means | 1.2e-7 | 2e-6 / 3e-5 / 2e-7 |

The band-statistic discrepancy comes from the intentional, more stable wide sum.
These are numerical kernel comparisons, not proprietary-renderer visual parity.

## 7. GPU stages, regression cases and 24 MP timings

Native WGSL implements VST, EAW decomposition, band reduction, threshold synthesis,
residual/inverse VST, luminance/chroma recombination; sharpening Gaussian/USM/halo
control/mask preview; haze morphology, arbitrary-channel cropped Kahan boxes,
RGB covariance solve, coefficient averaging and inversion. Host stages estimate
noise and ambient light and combine reduced statistics. If the 9-channel haze
buffer exceeds a device binding limit, only haze preparation uses the CPU inside
the GPU render. No new whole-render fallback was introduced for Detail tools.
Existing develop-layer fallback behavior remains outside this task.

Four added `lc-gpu/tests/toolset.rs` tests define 87 CPU/GPU comparisons:

- `detail_sharpen_radius_detail_masking_and_alt_preview_match`: 36 renders;
  Radius/Detail/Masking endpoints, two sensor scales, three sizes and Alt preview.
- `detail_y0u0v0_wavelets_six_sliders_and_preview_match`: 15 renders;
  luminance-only/chroma-only/combined NR, slider endpoints, three sizes and exposure.
- `detail_dark_channel_rgb_guidance_positive_negative_match`: 24 renders;
  both signs, two sensor scales and three sizes.
- `detail_slider_and_sensor_scale_changes_invalidate_cached_planes`: 12 comparisons,
  with cached == fresh checks through a slider session.

Scenes include odd dimensions, noise, colour edges and HDR values. Every comparison
requires mean absolute difference <0.5 LSB and maximum <=3 LSB. They print
“skipped: no GPU adapter” here: **none of these numeric GPU comparisons ran**.
All assembled WGSL modules independently pass Naga parsing and validation without
an adapter. The coordinator must execute the tests on the RTX 5090.

The ignored `bench_toolset_24mp` now has three independent Detail rows and permits
CPU timings without an adapter. `LC_DETAIL_BENCH_ONLY=1` selects those rows.
6000x4000 synthetic scene, full pipeline, AMD Ryzen 7 9800X3D, Cargo development/test
profile (opt-level 1 for workspace code, 2 for dependencies), one CPU render per row:

| Tool at 24 MP | CPU | GPU |
|---|---:|---|
| sharpening Amount 100 / Radius 1 / Detail 70 | 478 ms | unavailable |
| wavelet NR Luminance 60 / Colour 50 | 3003 ms | unavailable |
| Dehaze 60 | 2870 ms | unavailable |

These are optimized development-profile timings, not release or RTX measurements.
Release benchmark command remains documented in `toolset.rs`.

## 8. Lightroom XMP and notices

All ten requested fields map to their Detail settings. Import clamps Amount to
0–150, Radius to 0.5–3 original pixels and the remaining eight sliders to 0–100.
Tests cover ordinary values, both out-of-range directions and invalid/non-finite
values for every field. Existing XMP/preset tests pass. Adobe's external
`crs:ProcessVersion` bookkeeping remains; it selects no Local Image rendering path.
`docs/PORTS.md`, the darktable notice, GPU validation notes and fixture README list
provenance, GPL notices, regeneration, slider mappings and host extensions.

## Validation

Final required command passes: **732 tests passed, 7 ignored, zero failures**.
GPU device comparisons count as passed skips in this sandbox; static WGSL validation
does execute. The focused 16-test Detail/reference suite passes separately.

| Package/suite | Passed | Ignored |
|---|---:|---:|
| li-seg | 5 | 0 |
| lightcraft-develop | 29 | 0 |
| lightcraft-engine | 306 | 2 |
| lightcraft-gpu unit tests | 11 | 1 |
| GPU equivalence / fallback / memory / toolset integration | 33 | 1 |
| lightcraft-pipeline | 180 | 2 |
| lightcraft-ui-egui | 168 | 1 |

```sh
PATH="$HOME/.cargo/bin:$PATH" cargo +1.98.1 test --offline -p lightcraft-develop -p lightcraft-pipeline -p lightcraft-gpu -p lightcraft-engine -p lightcraft-ui-egui -p li-seg
```

Strict offline Clippy with `CARGO_TARGET_DIR=target/clippy`, all targets and
`-D warnings`, passes all six requested packages. Package formatting and
`git diff --check` pass. The existing missing `craft-fonts` build warning remains.

Logs (ignored): `target/detail-test-final.log`, `target/detail-clippy.log`,
`target/detail-fidelity-quality.log`, `target/detail-bench-24mp.log`,
`target/refvec/regeneration.log`.

Remaining work: owner clarification for the forbidden Compositing references;
real-adapter GPU numeric tests/release timings; real-camera visual review.
No client photographs or proprietary comparisons were used.
