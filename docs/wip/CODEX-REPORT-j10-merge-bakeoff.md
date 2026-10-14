# J10 — D20 slice 0: merge bake-off

Completed 2026-10-14, on top of the inherited branch (`c061f5d5`), uncommitted.

## Decision for J19

Use **lc-merge's MOPS/ANMS detector and matcher as the initial shared registration core**.
On the fixed CC0 sample, MOPS passed **8/8** cases versus Harris/BRIEF's **7/8**;
median ground-truth transform error was **0.03423 px vs 0.47521 px**. Overall match
precision was effectively equal (**96.919% vs 96.936%**), but MOPS supplied **4,341**
correct correspondences versus **348**, with wider spatial coverage. Choosing the
binary descriptors for their smaller size would sacrifice measured quality here.

Preserve the **scene-linear input model and saturation-aware weighting** for raw
brackets, and a separate **sRGB/response-recovery input model** for rendered sources.
The small HDR sample strongly favours the linear path; feeding linear sensor samples
to the rendered-source merger would be the wrong comparison. Keep the editor's layer
transform/layout semantics when J19 introduces adapters. This test does **not** select
between bundle adjusters or blenders, or establish proprietary-product parity.

This is a controlled two-scene result, sufficient to select J19's starting point,
not proof of best-in-class handheld/raw HDR across cameras and scenes. In particular,
there are no independently captured overlapping views or moving people in this sample.

## What changed and scope

- New `crates/lc-merge/examples/bakeoff.rs`: native MOPS + native homography RANSAC,
  linear HDR merge, explicit conversion from LC's +0.5 pixel centres to integer centres,
  and a 32-bit float DNG precision regression test.
- Extended the existing `crates/pc-algo/examples/bench_photo.rs`; its original timing
  cases remain. The new `bakeoff` mode enters **before** the original 24 MP allocation
  and runs native Harris/steered-BRIEF + homography RANSAC and rendered-source HDR.
- **Unavoidable edit outside the two assigned files:** new
  `crates/lc-merge/examples/bakeoff/common.rs`. Both examples compile this exact module
  by path. It owns fixture decoding, synthesis and scoring, preventing separate
  evaluators from favouring their own backend. It avoids adding a dependency between
  the production crates or duplicating a third registration/HDR implementation.
- This required report is the only other authored file. The pre-existing modification
  to `docs/wip/CODEX-TASK.md` was left untouched. No earlier J10 example/report was present.

What was merged in this slice is the **measurement protocol**, not the production
backends. Production APIs, callers, catalog/settings formats and UI behaviour are
unchanged. J19 owns their consolidation. No Match Look files, owner shots, Desktop
folders, GUI, ComfyUI, dependencies or lockfile were touched. No commits were made.

## Fixed data and reproduction

Actual asset files, not web preview renders, downloaded to ignored `target/j10/data/`.
[Poly Haven licenses its assets CC0](https://polyhaven.com/license).

| Scene | Asset | File SHA-256 |
|---|---|---|
| Interior | [Lebombo](https://polyhaven.com/a/lebombo) | `a02c9615adeba91a753c3e4737c3ce7a719273df822eeaa85d24982bd43c9ecb` |
| Exterior | [Kiara 1 Dawn](https://polyhaven.com/a/kiara_1_dawn) | `ee70fb8c8fb3e34566802191d83b299e179ecc392b97639e6c750f66e161c8e2` |

```sh
mkdir -p target/j10/data
curl --fail --location https://dl.polyhaven.org/file/ph-assets/HDRIs/hdr/1k/lebombo_1k.hdr -o target/j10/data/lebombo_1k.hdr
curl --fail --location https://dl.polyhaven.org/file/ph-assets/HDRIs/hdr/1k/kiara_1_dawn_1k.hdr -o target/j10/data/kiara_1_dawn_1k.hdr
sha256sum target/j10/data/*.hdr
PATH="$HOME/.cargo/bin:$PATH" cargo +1.98.1 run --offline -p lightcraft-merge --example bakeoff -- target/j10/data/lebombo_1k.hdr target/j10/data/kiara_1_dawn_1k.hdr
PATH="$HOME/.cargo/bin:$PATH" cargo +1.98.1 run --offline -p photocraft-algo --example bench_photo -- bakeoff target/j10/data/lebombo_1k.hdr target/j10/data/kiara_1_dawn_1k.hdr
```

Evidence retained locally: `target/j10/lc.csv`, `target/j10/pc.csv`,
`target/j10/summary.json`, and `target/j10/logs/`. CSV includes separate tagged
registration/HDR headers; rows must be grouped by their first field. Schema was checked:
**8 registration + 6 HDR rows per backend**, with matching field counts.

The shared reader is intentionally limited to modern RLE RGBE, `-Y/+X`, unit exposure
and colour correction, at most 1 MP/16 MiB, and 1–4 files per invocation. It rejects
unsupported layouts, oversized headers/dimensions, invalid runs, truncation and trailing
data. It follows the published Radiance scanline format and midpoint reconstruction
in [Radiance's colour source](https://github.com/LBNL-ETA/Radiance/blob/master/src/common/color.c)
and [colour definitions](https://github.com/LBNL-ETA/Radiance/blob/master/src/common/color.h).
This example-only reader is not a new application decoder.

## Registration protocol and measured results

Both receive identical 1024×512 perceptual luminance arrays, bilinear inverse-warped
from the CC0 scene. Feature budget: 800 per image; mutual Lowe ratio: 0.8; homography
inlier threshold: 1.5 px; seed: 10. LC uses its native adaptive RANSAC (cap 2000);
PC uses its native fixed 2000 iterations. Both require at least eight fit inliers.
No retuning was done after viewing the results.

Four cases per scene:

1. Sub-pixel translation `(7.25, -4.5)` px.
2. 2° rotation around the image centre plus `(3.25, -2.5)` px translation.
3. 1.08× scale, −1° rotation, `(-3.5, 2.25)` px translation.
4. Horizontal shift of 18% width and 1.4× perceptual brightness (clipped at 1).

Correct putative matches lie within 1.5 px of the known transform. Transform error is
RMS on a 5×5 grid restricted to shared image coverage, independently of RANSAC's own
inliers. A case succeeds with ≥10 correct matches and grid RMS ≤1 px. Missing fits
report `NA`, never a fabricated zero. A new test explicitly demonstrates why a wrong
identity consensus with zero fitting residual must fail the ground-truth score.

| Scene / case | Correct matches LC / PC | Fit inliers LC / PC | Fit RMS px LC / PC | Truth grid RMS px LC / PC | Occupied 4×4 cells LC / PC |
|---|---:|---:|---:|---:|---:|
| Interior / subpixel | 645 / 33 | 646 / 33 | .41879 / .58824 | .02068 / .65877 | 13 / 7 |
| Interior / rotation | 656 / 37 | 656 / 37 | .38223 / .53558 | .05777 / .70751 | 13 / 8 |
| Interior / scale | 573 / 36 | 572 / 36 | .38005 / .53745 | .04017 / .37390 | 13 / 7 |
| Interior / overlap + gain | 206 / 7 | 207 / 8 | .25678 / .49408 | .05133 / **1.58091** | 11 / 4 |
| Exterior / subpixel | 594 / 20 | 595 / 20 | .31825 / .62282 | .07714 / .57653 | 12 / 10 |
| Exterior / rotation | 625 / 85 | 625 / 85 | .28928 / .55961 | .02830 / .13213 | 12 / 10 |
| Exterior / scale | 526 / 49 | 526 / 49 | .29184 / .54517 | .01552 / .34656 | 12 / 10 |
| Exterior / overlap + gain | 516 / 81 | 516 / 82 | .21044 / .52961 | .02619 / .32000 | 11 / 9 |

LC supplied 800 features in each image; PC supplied 27–215. The same requested cap
does not guarantee equal feature counts: these are the actual default detectors.
The native outer alignment fallback/plausibility policies and panorama pair acceptance
rules are excluded to compare the registration cores directly. J19 should retain their
caller semantics and validate them separately during integration.

## Small HDR sample

Each HDR scene is sampled to 256×128, normalised toward median green 0.18 and capped
so the shortest exposure preserves the brightest channel. Three simulated brackets
at **−4 / 0 / +4 EV**, exposures `[0.0625, 1, 16]`, with independent fixed-seed noise
`σ² = signal × 2e-5 + 1e-7` and sensor clipping at 1. These are **simulated linear
brackets derived from CC0 HDR radiance, not camera raw captures**. This is not a
calibrated camera noise model. LC receives linear float samples, explicit clip level
and EV metadata with raw weighting enabled; PC receives the same samples encoded and
quantised to sRGB8, native response estimation, and middle-frame ghost base.

No tone mapping or image-dependent post-fit rescaling is applied to output metrics.
Merger alignment is disabled to isolate radiometry/deghosting. Registration is measured
above, separately; full merge-path alignment is not established here.

Static green-channel errors:

| Scene | Truth / LC / PC DR stops (1%–99.9%) | Log RMSE stops LC / PC | Shadow relative RMSE LC / PC | Highlight log RMSE stops LC / PC | Edge log RMSE stops LC / PC |
|---|---:|---:|---:|---:|---:|
| Interior | 7.47325 / 7.46191 / 7.57532 | .01743 / .19897 | 1.085% / 10.977% | .02315 / .15548 | .01965 / .20943 |
| Exterior | 9.62333 / 9.59983 / 9.39862 | .02184 / .21307 | 1.237% / 13.121% | .02516 / .10258 | .02073 / .27828 |

Shadows are truth green <.02 (303 interior, 5400 exterior pixels); highlights >.8
(1296 / 421 pixels). Edge samples have >.5 stop horizontal adjacent truth change.
Edge RMSE is an artifact proxy, **not** a guarantee against window halos/banding.
The corresponding clipped/noisy middle-frame log RMSE was .17566 / .11192 stops;
linear merge lowered these to .01743 / .02184. Dynamic range alone can rise from error,
so the extra .102 stops in the PC interior result is not treated as quality improvement.

The motion variant puts different coloured 20×24 rectangles in the non-reference
frames at two positions (960 truth motion pixels, 2.93% of the image). LC's available
strongest `High` and PC's boolean deghosting are compared with their respective off
states; their thresholds/strengths are not equivalent.

| Scene | Motion-region log RMSE LC off → High | PC off → on | Ghost fraction LC / PC |
|---|---:|---:|---:|
| Interior | .04606 → .02420 | .39848 → .39087 | 5.762% / .064% |
| Exterior | .06053 → .04132 | .87869 → .87277 | 5.762% / 52.161% |

The PC exterior ghost fraction warrants investigation: it selects over half the image
for a 2.93% motion mask. Native response estimation also changes when moving pixels
enter its samples; the PC non-motion log RMSE grew from .21307 static to .69159 with
motion, versus LC .02184 → .02197. These are measured reasons to preserve the linear
raw input model and not blindly share the rendered-source deghost/response logic.
LC's residual motion error still needs stronger real-world evaluation.

## Speed before/after and hardware limits

There is **no production before/after change** in J10. These are one-shot diagnostic
CPU timings from the two existing backends on the same sample, including native
allocation/conversion where shown. Rust **1.98.1**, repository dev profile (`opt-level=1`,
third-party deps 2), Ryzen 7 9800X3D, inherited runner, default Rayon threading.
No hardware-specific tuning, concurrency changes or repeated benchmarks were done.

| Diagnostic operation | LC ms | PC ms |
|---|---:|---:|
| Median detection, two 1024×512 images | 54.140 | 67.782 |
| Median matching | 2.887 | .066 |
| Median fitting | .140 | 3.147 |
| Median sum of detection + matching + fitting | 57.147 | 71.331 |
| Static 256×128 HDR, interior | 2.372 | 19.944 |
| Static 256×128 HDR, exterior | 2.856 | 18.618 |
| Motion HDR with deghost, interior / exterior | 3.796 / 3.746 | 20.428 / 20.060 |

PC's matching is cheaper, but operates on far fewer detected points. Both fitters
have different adaptive/fixed iteration policies. These numbers are not a controlled
release benchmark, GPU measurement, interactive-app latency or memory measurement.
The algorithms measured are CPU-only; no GPU performance claim is made. No full-size
shoot, 30–60-set sweep, existing 24 MP benchmark or GPU acceptance suite was run.

## Owner HDR addendum: evidence and J19 handoff

The standards remain requirements for **J19's production work**, outside J10's assigned
example files. J10 does not claim to have implemented them:

- `lc-merge/src/frame.rs::load_frame` currently calls `raw.develop()` **before** merge:
  it preserves camera-RGB scene-linear samples/tags, but is not a CFA-domain merge.
  J19 must decide the CFA/alignment/demosaic design and verify real raw noise/edges.
- The writer supports `DngSamples::Float`. The new focused test proves a 32-bit,
  three-channel floating DNG decodes and recovers a 15-stop sample range using its
  baseline exposure, including a very small nonzero channel, within 2 ppm plus 1e-12.
  However, `lc-engine/src/merge.rs` currently selects **Half (16-bit float)** for HDR;
  the production caller still needs the requested 32-bit choice. This test uses
  memory only; it does not create a file or prove external Lightroom compatibility.
- Existing `Deghost::{None,Low,Medium,High}` and `HdrResult::ghost` support selectable
  strengths and an overlay. Keep these richer APIs. The measurements above exercise
  off/High only, not a sweep of all strengths or real people/trees/curtains/cars.
- Existing HDR-panorama grouping uses bracket size/EXIF. Automatic shoot-wide grouping
  by EV + capture time + scene similarity, and background batch HDR/progress for
  30–60 sets, remain production requirements for J19 and its caller integration.

Official reference behaviours reviewed before design:

- [HDRMerge](https://jcelaya.github.io/hdrmerge/) describes floating DNG output from
  less-noisy raw pixels and ghost editing. It is a behaviour reference only; no C/C++
  was introduced or linked, and no external HDRMerge numerical output was measured.
- [darktable 4.6 actions on selection](https://docs.darktable.org/usermanual/4.6/en/module-reference/utility-modules/lighttable/selected-image/)
  documents raw-only HDR DNG creation and requires aligned tripod images. This
  establishes its documented alignment limitation, not numerical superiority.
- [Adobe Lightroom Classic HDR merge](https://helpx.adobe.com/lightroom-classic/desktop/process-and-develop-photos/hdr-photo-merge.html)
  documents auto-align for handheld movement, None/Low/Medium/High deghost, a deghost
  overlay and DNG output. This page does not establish a 32-bit output specification;
  32-bit float is the owner's explicit requirement.

`hdrmerge` and `darktable-cli` were not installed on PATH. No proprietary reference
renders were available. External pixel-level parity, colour accuracy, real CFA raw
brackets, batch performance and window-edge artifact acceptance therefore remain
**unmeasured**, not silently passed. Source truth provides the numeric reference for
this small J10 sample. No established project protocol for this detector bake-off
existed; the ground-truth warps and metrics are our own example-only protocol, informed
by the native modules' published-paper implementations and the audit's requested
inlier/residual comparison.

## Verification

One Cargo process at a time, offline, inherited `CARGO_BUILD_JOBS=4`, same retained
`target/`; no dependency/Cargo.lock changes and no new `-sys` dependencies.

- `cargo +1.98.1 fmt -p lightcraft-merge -p photocraft-algo` — done.
- `cargo +1.98.1 check --offline -p lightcraft-merge -p photocraft-algo --examples` — pass.
- `cargo +1.98.1 clippy --offline -p lightcraft-merge -p photocraft-algo --all-targets -- -D warnings` — pass.
- `cargo +1.98.1 test --offline -p lightcraft-merge --example bakeoff j10_` — four
  passed initially; the new float-DNG test exposed a divide-by-zero in its own error
  calculation for a zero source channel. Fixed the metric to absolute + relative
  error, preserving the 2 ppm precision requirement and handling black correctly.
  Re-ran **only** `j10_float_dng_keeps_hdr_precision` — pass. First failure log retained
  as `target/j10/logs/test-lc-first-failure.log`. **All five new LC tests now pass.**
- `cargo +1.98.1 test --offline -p photocraft-algo --example bench_photo j10_` —
  **4 passed**, including the shared three protocol tests.
- Both small measurement commands above — pass; **16 registration / 12 HDR rows**
  total. No full crate/workspace suites, existing tests, GUI tests or GPU runs,
  following the task's ABSOLUTE MINIMUM TESTING override. GPU tests added: **0**.

TEST-CHANGED: no existing tests were changed or weakened. Six new test definitions:
three shared protocol tests (compiled in both examples), LC half-pixel conjugation,
LC float-DNG precision, and PC sRGB8 input encoding. Nine passing test instances
across the two example harnesses. No production migration remains within J10's scope;
J19's prerequisites and limitations are recorded above.
