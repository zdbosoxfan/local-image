# Develop primary sliders — 2026-10-09

Continued the previous builder's UCS 22 and local-Laplacian files, compiled and wired
them, and completed the primary tone/detail/colour stages. The performance revision below
replaces their CPU bridge with native WGSL. Changes are uncommitted.
No dependencies, unsafe Rust, or C build integration were added. C reference programs
are generated only under ignored `target/refvec/sliders/`. All Cargo commands use
1.98.1, offline mode and three build jobs, with one Cargo invocation at a time.
No desktop app was opened or stopped. The three client RAWs were read in place;
only the explicitly requested derivative JPEGs/HTML were written under `target/`.

## Performance revision after RTX 5090 review

The coordinator's release/Vulkan run found excellent initial correctness (all
comparisons, max <=2 LSB) but unacceptable performance: typical 4457 ms, A 720 ms,
B 837 ms, detail 5569 ms, UCS colour 4957 ms and skin 2386 ms. That implementation
read back the WB/NR image, ran primary processing on the CPU, and uploaded it.
**That bridge is removed. All requested primary stages now execute in WGSL.**
The CPU algorithms remain the reference. Existing upstream fixtures, expected
pixel hashes and equivalence thresholds are unchanged in this revision.

| Stage | Native implementation | Cached slider work |
|---|---|---|
| A — LI Tone | Fixed geometry-matched ~2 MP GPU proxy; GPU bounds reduction; ten gamma samples; five-tap pyramids to 1 px; gain clamp and guided coefficients | Four linear control-response fields are built together and retained. A drag combines them, clamps the gain and updates guided coefficients; it never rebuilds the pyramid. Clip taper combines the same fields with Highlights zero. |
| B — EIGF | GPU quantization, upstream corner-aligned interpolation, moment bounds reduction, faithful Deriche endpoint/clamp/recurrence and three iterations at each scale | Three filtered proxy bases retained independently of tone slider values. |
| Clarity | Both complete padded darktable LLF remappings, twelve gamma samples, level weights, weighted identity subtraction, guided gain upsample, Natural/Punch/Neutral UCS handling | Proxy coefficients retained independently of amount/mode. |
| Texture / Structure | Signed subpixel covariance path or two-iteration EIGF at sensor-scaled 2/4/16 px, bias and bounded log bands | Filtered 2/4/16 px planes reused; the shared 4 px plane is built once. |
| Vibrance / Saturation / Grading | f32 Yrg, luminance masks, polynomial power, UCS22 saturation/brilliance and gamut mapping | Per-pixel pass; unchanged preceding stages are reused. |
| Colour equalizer / B&W | Native UCS conversions, saturation blur/LUT, both full UV guided filters, RBF lookups, Scharr suppression and brightness/B&W output | UV/HSB fields and correction guidance covariance/means retained; only changed corrections and downstream pixels rerun. |
| Skin Tone | Native signed EIGF of J and Cartesian chroma with shared guide means/variance, 3D window, preserved residual, lip protection, UCS gamut/output | Original JCH and low fields retained independently of reference, strength and window values. |
| Primary layers | Same ordering and alpha blending as CPU, native CAT and each primary kernel | Keys include upstream tools, mask geometry/refinement/inversion/opacity, selection tone and source metadata. |

Gaussian reuse preserves the actual reference: existing box-pass kernels for gain
upsampling, extended to four channels; faithful Deriche kernels for EIGF, colour
filters and skin. Both Deriche sweeps operate on coalesced columns through a tiled
transpose and retain the CPU recurrence/endpoint order. No Gaussian approximation
or reduced filter quality was substituted. Bilinear downsampling of proxy
coefficients uses the existing antialiased resampler; upsampling is fused with evaluation of the guided gain. Gamut/RBF/saturation LUTs and matrices are small CPU control metadata,
not image processing or image round trips.

Neutral guards are shared by both renderers. Defaults, every primary strength at
zero, selected-but-neutral Skin Tone references and zero-strength grading wheels
skip their primary stages. Colour/skin-only work does not construct a proxy or
enter tone/detail. B&W treatment remains an actual conversion even with neutral
mix coefficients. Test dispatch counters verify zero primary dispatches when
controls return to neutral. Device cache memory includes every retained primary
field. The production primary code performs no image readback/upload; test-only
reads exercise its numerical comparisons. Full-resolution intermediates use at most
four channels per buffer (384 MB at 24 MP), avoiding 576 MB six-channel bindings.
Colour-range masks also use the GPU photo tone map, and mask edge refinement uses
the matching existing Gaussian/guided kernels.

Hardware targets remain **unmeasured in this sandbox**: typical <=~200 ms at
24 MP, primary increment <=~100 ms and cached single-tool drags well below 100 ms.
The retained 6000x4000 benchmark now adds primary identity, incremental costs
relative to that baseline, and median/worst cached single-slider drags. The primary
filter includes the typical edit row. Reproduce the coordinator run with:

```sh
LC_PRIMARY_BENCH_ONLY=1 PATH=$HOME/.cargo/bin:$PATH CARGO_BUILD_JOBS=3 \
  cargo +1.98.1 test --offline --release -p lightcraft-gpu --test toolset \
  bench_toolset_24mp -- --ignored --exact --nocapture --test-threads=1
```

None of the requested primary sliders remains CPU. Existing non-primary work can
still use the host: capture sharpening before sampling; unsupported lens geometry;
film-negative conversion, defringe, retouching and non-linear camera profile/calibration;
NR noise estimation/band statistics (including the same optional NR on a cold
proxy); dehaze airlight statistics; stored/heuristic segmentation or oversized brush
masks; the separate Tone Equalizer's spatial/finish stage; and established non-primary
layer/proof/deep-output fallbacks. These are existing stages outside the primary
ports. Cached primary drags reuse their preceding linear/proxy buffers. Uncached proxy
sampling also reuses the original source upload from the geometry stage. The alternate
Jz saturation formula remains a fixture-tested CPU reference helper; renderer
settings select UCS22, so no rendered primary tool uses that helper.

The production device still rejects software adapters. A test-only CPU Vulkan
device was available here and executed WGSL numerically: Deriche, both EIGF paths,
LI Tone response caching, UV guidance, complete primary renders, odd preview
resampling/clipping, colour-range masks and layer cache edits, actual skin movement,
every neutral control and a constrained binding-limit case. Render assertions retain mean <0.5 LSB/max <=3 LSB.
This execution is additional correctness coverage, **not an RTX performance claim**.

## Highlights / Shadows

- A **LI Tone** implements vkdt's five-tap reduce/expand and sampled Laplacian
  construction on log EV, ten remap samples, rectangular pyramids through 1×1,
  smooth Hermite highlight/shadow/white/black bands, and small-detail preservation.
- B uses three scales of the faithful darktable EIGF. Both compute a gain on a
  geometry-matched, fixed approximately two-megapixel proxy, guide its upsampling
  with full-resolution log luminance, and multiply all three RGB channels by the
  same gain. HDR values remain unclipped. `primary::DEFAULT_HS_METHOD` is the
  one-line internal choice; A is the current default. No choice is persisted or
  exposed in the UI.
- RAW decoding measures clipping on the original normalized sensor before
  reconstruction, averages each CFA colour's clipped fraction, and carries it
  through fitting/orientation into `SourceInfo`. Fully clipped pixels suppress
  negative Highlight gain while preserving other tone bands. Metadata is losslessly
  run-length encoded in smart previews, and missing old metadata defaults safely.
- CPU/device stage caches include the candidate, source clipping information and
  all primary controls. The new cached image is counted in memory accounting.

Owner comparison: [self-contained HTML](../../target/compare-highlights/index.html),
`target/compare-highlights/metrics.csv`, and 96 labelled JPEG crops. The example is
`crates/lc-engine/examples/compare_highlights.rs`:

```sh
PATH=$HOME/.cargo/bin:$PATH CARGO_BUILD_JOBS=3 cargo +1.98.1 run --offline \
  -p lightcraft-engine --example compare_highlights
```

It renders each slider separately at −100/−50/+50/+100 with both candidates on
three synthetic HDR/window/edge/skin scenes and `_DSC5041`, `_DSC4601`, `_DSC5420`.
Synthetic export/preview long edges are 768/384; RAWs use 2048/512 from a 4096-edge
decoded source. Metrics include halo peaks, connected 10%-peak width, integrated
absolute EV error, reversed lightness-pair fraction, chromaticity-normalized CIELab
hue drift, ΔE2000 frame percentiles and explicit six-patch colour/skin ROI means.
RAW portraits have no ideal reference step, so halo metrics are marked unavailable.
Colour/skin ΔE includes the intentional slider lightness change; it is not claimed
to measure colour drift alone. Preview/export comparison downsamples display-linear
RGB using the same Mitchell filter as geometry.

All **96 cases pass** Δh <2° and preview/export p95 ΔE2000 <1. The final
comparison was regenerated after the parallel optimization; its complete metrics
CSV is byte-identical to the earlier passing run. The HTML
contains 96 embedded JPEGs and needs no external images/scripts/styles. JPEG crops
were visually inspected. Independent maxima across the cases (the peak/width/energy
maxima need not come from the same row):

| Metric | A — LI Tone | B — EIGF |
|---|---:|---:|
| Hue drift p95, maximum over all cases | 0.000307° | 0.000307° |
| Preview/export p95 ΔE2000, maximum | 0.856026 | 0.832828 |
| Synthetic overshoot peak | 0.122532 EV | 0.747730 EV |
| Synthetic undershoot peak | 0.122534 EV | 0.747730 EV |
| Synthetic 10%-peak width, maximum | 432 px | 163 px |
| Synthetic halo energy, maximum | 13.348621 EV·px | 53.195987 EV·px |
| Lightness order error, maximum over all cases | 0.038268 | 0.024668 |

A has smaller worst synthetic halo peaks/energy and a broader low-amplitude halo.
B has larger peaks on extreme shadow moves, but narrower support and smaller worst
RAW lightness-order error. The step-property tests show no boundary gradient reversal;
LOE also counts order changes among textured samples. A remains the review default;
the owner can inspect both candidates in the page before choosing.

An initial harness run incorrectly compared a Box-downsampled export with the
renderer’s Mitchell-prefiltered preview (RAW p95 up to 1.590). Matching the filters
resolved that discrepancy; the final full run above uses Mitchell and exits nonzero
if any hue/preview limit fails. No renderer adjustment or metric exclusion was used
to obtain the passing result.

## Clarity / Texture / Structure

Clarity uses darktable's complete padded local-Laplacian kernel, mapped from log EV,
with medium-scale bands and midtone weighting. Weighted identity is subtracted to
avoid an unintended baseline change. Natural rolls off highlight chroma, Punch
adds chroma with local contrast, and Neutral changes only luminance. The Effects
panel has the three-mode selector and a new −100…100 Structure slider.

Texture and Structure use 4–16 and 2–4 original sensor-pixel bands, including crop,
output and binned-sensor scale. A luminance bias limits shadow-noise amplification.
For subpixel bands a signed guided filter avoids the upstream EIGF's minimum-one-
pixel radius collapsing both scales to the same filter.

## Colour, Skin Tone and layers

- Vibrance, Saturation and four grading wheels use the complete selected darktable
  color-balance RGB pixel path: Yrg chroma, luminance masks, polynomial power,
  UCS 22 saturation/brilliance and gamut handling. The alternate JzAzBz formula
  is implemented and reference-tested as an internal mode. Slider-to-parameter
  calibration is LI's own, rather than a claim of identical darktable presets.
- Eight LR-style Colour Mixer nodes feed the colour equalizer's UCS prefilter,
  complete two-channel covariance, correction guided filter, saturation weighting,
  Scharr suppression and periodic cosine RBF. The direct f64 solve improves
  conditioning over normal equations; independent C vectors bound the difference.
  B&W mixing uses UCS brightness and these eight hue weights. Point Color retains
  its existing OkLCh implementation.
- Skin Tone stores an engine-picked UCS JCH reference and a soft three-dimensional
  colour window. Guided low-frequency chroma moves toward the reference while the
  original high-frequency residual remains. Lightness uniformity is limited;
  optional redward hue protection leaves lips unchanged. The new flyout exposes
  reference picking, uniformity, lightness, window ranges and Protect Lips.
  Picker changes support undo/history and active develop layers. Copy/reset and
  colour presets include the new controls; old JSON uses serde defaults.
- Local Temp/Tint and layer WB use the existing proper chromatic adaptation
  matrix. Colour-range masks/picking now use the photo's actual tone map on
  exposed scene-linear colours, rather than the former rational approximation.
- New primary tools run in global and visible-mask/layer stages with the shared
  opacity/alpha. Existing curve, Point Color, detail, dehaze and other layer stages
  continue to work. Remaining non-primary layer tools retain their pre-existing
  CPU fallback; none of the new primary tools requires that whole-render fallback.

## Fidelity and device integration

The Gaussian intermediate is transposed to let independent column and row
recurrences run in parallel using safe contiguous slices. Colour equalizer's
independent conversion/output passes also run in parallel. Per-channel arithmetic
order is retained: all extracted C fixtures and every existing pixel golden pass
without changing expected values after this optimization.

The simplified log-luminance tone-equalizer mask was replaced by compensated linear
luminance, faithful quantized EIGF and the upstream geometric final blend. The
existing independently fixture-tested exposure-curve fitting remains.

Seven committed fixture sets are extracted from unchanged upstream C function
bodies at darktable `733bd69f32cac7ff5e41025115942772add1f088`. They cover 4,292 EIGF
samples (self/quantized guidance and linear/geometric blends), 5,550 complete LLF
outputs, HDR UCS roundtrips, 2,146 colour-filter pixels, 1,024 RBF LUT samples,
82 full colour-balance RGB pixels across UCS/Jz, and HDR Jz forward/inverse vectors.
[Fixture README](../../crates/lc-pipeline/tests/fixtures/sliders/README.md) records
every tolerance, adapter and deviation; its manifest records hashes. Regeneration
uses `scripts/engine_sliders_refvec.py`, GCC `-O0 -ffp-contract=off`, and the local
read-only pinned source. LI Tone additionally has step-order, fine-detail, RGB-ratio
and clipped-specular properties. Skin tests verify low-frequency uniformity,
preserved pores and lip protection.

The GPU render now uses the native primary stages described above between WB/NR
and the established spatial/finish stages. The separate faithful tone-equalizer
processing retains its CPU spatial/finish integration. Device caches distinguish
A/B and independently retain amount-independent primary fields. Existing equivalence
cases and mean <0.5/max <=3 LSB bounds are unchanged. Real RTX execution of this
native revision remains the coordinator's check; the previous RTX correctness
results describe the superseded CPU bridge.

PORTS.md, darktable and vkdt notices, and GPU-VALIDATION.md are updated. vkdt is
pinned to `afc34256fb22bcaf6dcf6503d19d5f9a6f477af5`; its complete BSD-2 licence is
in `licenses/vkdt-NOTICE.md`. No Local Image ProcessVersion or `v2026()` branch
remains. Adobe `crs:ProcessVersion` metadata names remain in import fixtures and
the import whitelist; they do not choose a renderer.

## Validation

Full offline native-revision run: **756 passed, 0 failed, 7 ignored**, plus all doc-tests
passing. The native 24 MP hardware benchmark is retained for the coordinator;
historical CPU-only timings are listed separately below.

| Changed crate / target | Passed | Ignored |
|---|---:|---:|
| lightcraft-develop | 29 | 0 |
| lightcraft-engine | 307 | 2 |
| lightcraft-pipeline | 199 | 2 |
| lightcraft-gpu lib | 16 | 1 |
| GPU equivalence / failures / sampling / toolset | 16 / 3 / 1 / 16 | 0 / 0 / 0 / 1 |
| lightcraft-ui-egui | 169 | 1 |

The hardware integration tests contain 39 explicit no-adapter skips. Five new
internal numerical tests additionally execute native WGSL on CPU Vulkan, as
described above; the other host/shader validations still run. The new UI
test actually clicks Punch, drags Structure, opens Skin Tone, activates its picker
and clicks the photo. Engine tests cover reference picking, clamping, history,
JSON and layers; pipeline properties cover cache edits and backward defaults.

Validation commands (all with `PATH=$HOME/.cargo/bin:$PATH CARGO_BUILD_JOBS=3`):

```sh
cargo +1.98.1 fmt -p lightcraft-develop -p lightcraft-pipeline -p lightcraft-gpu \
  -p lightcraft-engine -p lightcraft-ui-egui
CARGO_TARGET_DIR=target/clippy cargo +1.98.1 clippy --offline \
  -p lightcraft-develop -p lightcraft-pipeline -p lightcraft-gpu \
  -p lightcraft-engine -p lightcraft-ui-egui --all-targets -- -D warnings
cargo +1.98.1 test --offline --no-fail-fast \
  -p lightcraft-develop -p lightcraft-pipeline -p lightcraft-gpu \
  -p lightcraft-engine -p lightcraft-ui-egui -- --test-threads=3 --nocapture
```

All-target Clippy passes with warnings denied. No manifest/lockfile change was
needed. `git diff --check` passes for task-owned changes; the pre-existing user
edit in CODEX-TASK.md is preserved. All seven fixture hashes match their manifest.
Logs: `target/sliders-native-tests.log`, `target/sliders-native-tests-final.log`,
`target/sliders-native-gpu-final.log`, `target/sliders-native-clippy.log`,
`target/sliders-native-clippy-final.log`,
`target/sliders-comparison-final.log` and `target/sliders-bench-24mp.log`.
GPU tests and all-target GPU Clippy were repeated successfully after the final
original-source upload reuse change; the five-crate format check also passes.

The first highly concurrent UI run timed out waiting for a thumbnail; the isolated
snapshot and complete three-thread UI run pass without changing the test or its
timeout. A terminated build left invalid incremental link objects; discarding only
generated pipeline artifacts and rebuilding produced the passing run above.

The original integration changed pixel goldens for tone, colour, clarity and local
mask behavior; defaults remained stable and render-cache version became 15.
This performance revision makes **no further golden changes**. Original updates:

| Fixture / case | Before | After |
|---|---|---|
| layers: global edits/rendered | `de4f12b360deb639` | `ed30a70ce89869f7` |
| layers: global edits/raw | `57ebbae81d3c021a` | `a184c372831bc0e3` |
| layers: old masks/rendered | `f6cacf8f24c584fe` | `fb0b81c2819f95f1` |
| layers: old masks/raw | `2db505da57c14063` | `9b24de949a7b4566` |
| toolset: raw defaults + edits/rendered | `f2364afc56909ea9` | `35b2897296c0abde` |
| toolset: raw defaults + edits/raw | `c5693c7dca173f4d` | `27515ff61c81ba78` |
| toolset: profile look/rendered | `f340243918f5d0d7` | `3afbb0e76b323622` |
| toolset: profile look/raw | `bf9002730b346eb4` | `8352318c42cfb5c8` |
| toolset: b&w profile/rendered | `af0827daa56de037` | `13a8484c4988917f` |
| toolset: b&w profile/raw | `1e627cb4c60f4827` | `c3d16db57db48b3d` |
| toolset: layers/rendered | `5d7d052c40911a42` | `707864fed6cf7fd2` |
| toolset: layers/raw | `ca6bb1ad16f271e4` | `7a6a3c0ba4313a72` |


## Historical CPU-only timings

Before the native revision, the ignored primary benchmark also ran in the
optimized development/test profile on this shared machine. These numbers are
historical CPU costs, not the new GPU path or release/RTX results:

| Row | CPU before parallel passes | CPU after parallel passes |
|---|---:|---:|
| A Highlights/Shadows | 4,245 ms | 1,150 ms |
| B EIGF candidate | 5,079 ms | 1,246 ms |
| Clarity / Texture / Structure | 23,526 ms | 5,900 ms |
| UCS colour | 56,472 ms | 7,618 ms |
| Skin uniformity | 17,067 ms | 3,232 ms |

The coordinator's unacceptable release/RTX measurements and the native replacement
are recorded at the start of this report. Those CPU host costs are no longer part
of requested primary processing in a GPU render. New RTX timings remain pending.

## Merge with the working branch

Resolved the four conflicted files against `claude/sleepy-franklin-egimjb`, preserving
native lens-database distortion/TCA/vignetting, zero-strength bypass, native capture
sharpening and their tests/benchmarks. The full render and primary proxy share the
same uploaded/capture-sharpened source; capture keys invalidate all downstream
primary fields, and memory accounting retains both capture and primary buffers.
Pupil/Upright analysis and proxy pupil resolution use the sharpened pixels.

Kept `quiesce(timeout)`, the running-render counter and JobPool worker joins.
Restored `tools_need_cpu`: the separate Tone Equalizer and its mask overlay request
the CPU renderer because they have no native kernel. The primary EIGF candidate
remains native. Tone Equalizer tests assert this fallback; numerical bounds,
fixtures and goldens are unchanged. GPU validation retains both branches' sections.

A new software Vulkan regression combines capture, lens and primary edits, both
HS candidates, preview resizing and pupil analysis; cached output matches fresh
output and the CPU reference within the original bounds. Numerical-test device
creation now shares the existing test initialization lock. An initial parallel GPU
process exited with SIGSEGV; the GPU suite and full rerun pass after this change.
The index is untouched; the coordinator stages and commits the resolved files.

Final offline five-crate regression: **816 passed, 0 failed, 9 ignored**, all doc-tests
pass. Six native numerical tests execute on software Vulkan; hardware-only tests
report 51 explicit no-adapter skips. Five-crate format checking and all-target Clippy
with `-D warnings` pass. Logs: `target/sliders-merge-tests-final.log` and
`target/sliders-merge-clippy-final.log`. Fixture SHA-256 values still match the manifest.
