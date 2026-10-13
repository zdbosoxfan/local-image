# Match Look implementation report

2026-10-13. Continued the inherited implementation and completed the Analyze panel, global
Saved Looks, independent review fixes and deterministic Solver v2 / ranked UX requirements.
Everything remains **uncommitted**. No pc-*, li-ai or li-seg source, dependencies, queue tooling,
owner photographs or running desktop applications were changed.

## Delivered workflow

Develop has a collapsible Match Look section near Sync, with up to ten removable reference
thumbnails, Analyze / Re-analyze, progress / Cancel, finished-look summaries and a saved-look
picker. References are rendered with their complete edits, crop and layer stack. Changes to
references invalidate the current analysis and disable matching until re-analysis. Saved looks
remain usable after their reference photographs have been removed.

Match Look solves each target's own editable settings, rather than copying reference slider
values. The four groups, independent Normalize, Strength 0–125%, WB / exposure locks, skin
protection, contrast guard, Colour only preset, consistent batch and new-layer output are
available in the dialog. Preview is optional; Apply works directly. Each batch is one undo step.

The preview offers a draggable Before / Result wipe and a Reference / Result wipe when the
matching live references are available. Saved looks contain no pictures, so comparison falls
back to Before / Result when their live references are unavailable. Scene similarity and a
“Very different from the references — results may be weak” warning appear in preview and in
per-photo results. Reference weights and a “What changed” summary make the result inspectable.
After applying, each photo has its own Strength and per-group reset actions. Later manual edits
are protected from an obsolete amount baseline.

Saved looks support Save Look…, select, rename, duplicate, delete, confirmed Update from
current references…, and Import / Export Look… through the existing host file-dialog helpers.
The picker scrolls / virtualizes its rows; the global collection has no look-count cap.

## Review fixes

| Finding | Implemented fix and evidence |
|---|---|
| **B7 / B5, endpoints** | Solver v2: log-percentile warm start; calibrated neutral / skin chromaticity WB gap; populated-CDF tone mapping; fixed-order slider projection; bounded regularized LM / Gauss–Newton; Huber / reference-spread weights; sparse prior; penalty continuation; coarse / fine proxies. Large corrections are bounded by slider domains rather than two fixed sweeps. Actual fitted regional curves are projected into a slope cone, preventing the editor's monotonic repair from producing flat plateaus. Metrics and rail audit below. |
| **B1** | Every non-Custom preset starts from `auto::current_wb`, the same effective WB used by rendering. Tests cover raw Tungsten, Daylight, Auto and AsShot, compare with explicit effective Custom WB, and verify locks. Normalize is now independent of Strength. |
| **B2** | Preserve the established single `Op::Batch` transaction, but check identity / source / settings per target. Skip only changed photographs; unchanged targets finish normally. Return skipped IDs, show count in the toast and names in the panel. A library switch still rejects the old batch. Cancel commits nothing. Toast numbers use a numeric default and never print `null`. This protects edits without wasting the other 499 solves. |
| **B4** | New-layer WB is relative `mask.adjust.temp/tint`, with `tools.wb` unset. Later global WB remains effective. CPU test and strict physical-GPU test cover this, including a later global WB edit. |
| **Import** | Bound bytes **before parsing** (2 MiB per imported file); names ≤128 Unicode characters / 512 UTF-8 bytes; IDs ≤128 bytes; date strings ≤64 bytes; 1–10 references per look; finite, bounded statistics / support / signatures / prototypes; ordered luminance quantiles; valid version and distinct IDs. No cap on total saved looks: tested 1,051 global entries. Panel Import / Export uses native JSON pickers; headless tests inject the same existing dialog-helper interfaces and click both menu actions. |
| **Design system** | Documented checkbox, text action, reference tray, summary chips and image preview in the Library catalogue, plus translated text button, shared job progress, comparison wipe and change-summary widgets. Progress is shared with culling. Tokens / accessibility / stable automation IDs are used throughout. Narrow-panel chip wrapping was corrected after visual inspection. |
| **B3** | Reconcile missing or failed-store selected IDs on library open / panel refresh. Resolve the management row only for an existing look; current analysis can be selected again. |
| **B9** | Analysis snapshots live in `match-look-analysis.json`, separate from preferences. Preference Match Look decoding is tolerant; malformed analysis / options cannot discard export or other preferences. Valid legacy embedded analysis migrates to its own file. Per-photo amount baselines use a separate versioned `match-look-applied.json`, saved on normal catalog close; no snapshot arrays are placed in `prefs.json`. |
| **B8** | Apply requires a ready look and target selection, without a manual Preview. Real-click test applies directly. Preview never commits edits. |
| **B10** | Removed duplicate top-level Develop and duplicate Edit entries. Photo menu items sit near Sync; photo context menu and Develop panel expose the same workflow. Lightroom / Capture One profile defaults remain unassigned, without shortcut conflicts. |
| **Labels** | “Add to Match Look References”; “Analyzed references”; summary units / warm-cool meaning with explanatory hover text; Normalize explanation; confirmed “Update from current references…”; options remembered between opens. |
| **B6** | Dropped the incidental Auto-tone port and restored `auto.rs` exactly to the branch base, including the original dark-image assertion **>1.5 EV**. The documented LightCraft calibration accuracy was not independently established for this fork, so its Auto-button change is outside this job. No assertion was loosened. |

An additional editability fix expands the **displayed** JPEG temperature scale to cover the
existing 2,000–50,000 K storage domain. It keeps the familiar relative ±100 / neutral-zero
presentation through a logarithmic mapping. The former mired mapping silently displayed
corrections above about 13,542 K at +100 and made the next manual adjustment discontinuous.
**TEST-CHANGED:** Strength tests now assert full / half / zero keep Normalize identical and
style sliders interpolate exactly. The former whole-image error-ordering assertion is not a
Strength contract when normalization is fixed and the fit has a prior; 50% can be closer to
a distribution than 100%. Independent quality thresholds and Auto assertions are retained.

Stored WB settings and renders are unchanged by this presentation fix. Round-trip and large-WB
editability tests cover it.

## Solver and storage

The original Rust dense solver uses normalized finite-difference columns, ridge costs toward
**current target settings**, an IRLS sparsity term, LM damping, Huber residuals scaled by
reference spread and tanh bounded updates. Exposure / WB are inexpensive; Contrast and global
Saturation have higher costs. Whites / Blacks, Shadows / Highlights, Contrast and regional
curves split the tonal work. The CDF seed is projected in that order after Exposure. Its sparse
populated quantile map is monotone with 0.5–2 slopes; the incremental fitted regional curve is
also guarded, including at 125% Strength.

Measurements include L5/25/50/75/95, relative luminance-zone a/b and chroma, neutral chromaticity,
conditional hue-band chroma, skin and a small global a/b term. Deterministic soft a/b prototypes
use Shepard weights and supply a neutral fallback. Similarity-weighted per-statistic medians
use sample-center interpolation (two equally weighted references have their midpoint) and
3-MAD outlier rejection. Saved looks retain each reference's statistics and signature, allowing
new targets to compute fresh similarity weights without IDs, source paths or pictures.

Skin protection uses an already-loaded People face cache when available, maps face rectangles
through the real orientation / warp / crop, prioritizes skin statistics and constrains creative
hue movement around the normalized skin anchor. The fit leaves headroom for 125% Strength.
A conservative skin-colour window is the fallback. No model loads, downloads or segmentation
are required. Tests cover dark skin, cached-face exclusion of a brown background and hue bounds.

Global data is `Presets/match-looks.json` in the same application config directory as presets,
with format version / serde defaults and atomic writes. Existing 25- and 27-feature looks load
with newly added features unsupported. Analysis / preferences remain catalog-local. Amount
baselines survive a normal catalog close / reload; hard termination before close can lose that
amount history, while the editable Develop changes remain journaled. New-layer WB retains the
existing relative local-control domain; it never cancels subsequent global WB.

Workers use Utility priority, cancellable `heavy_jobs` admission and sequential deterministic
reductions. The fit uses 60 px coarse sources / outputs and 128 px acceptance sources / outputs,
with sensor-scale compensation; previews use 400 px. Exact byte-to-linear decoding is cached,
probe renders omit unused descriptors / clustering, and repeated chroma calculations are shared.
A test checks identical analysis / probe features and every decode-table entry. No optimizer or
other dependency was added.

## Quality and rail audit

Re-ran the reviewer's unchanged scenarios / metric functions on the six CC0 model-eval JPEGs,
three looks and four perturbed targets, at 512 px. Both similarity and consistent modes run.
The original harness was also re-run without instrumentation. Distribution ΔE is the reviewer's
Oklab×100 metric (L50 + mean a/b), not a perceptual claim about paired pixels or semantic objects.

| Similarity mode, 12 target × look pairs | Before review fixes | Final Solver v2 |
|---|---:|---:|
| Mean distribution ΔE | 3.058 | **1.591** |
| Mean absolute ΔL5 | 8.233 | **6.175** |
| Mean absolute ΔL50 | 2.817 | **1.292** |
| Mean absolute ΔL95 | 12.225 | **9.367** |
| Targets at Contrast ±100 | 8 / 12 | **0 / 12** |
| Five common controls in outer 10% of legal linear ranges | 13 / 60 (21.7%) | **3 / 60 (5.0%)** |

Both endpoints improve by about a quarter; mean ΔE improves about 48%. Consistent mode mean
ΔE is **1.835**. The common-control rail audit uses Exposure [-5,5], Temp [2000,50000], Tint
[-150,150], Contrast / Saturation [-100,100]. All three final near-edge observations are ordinary
cool WB values in the lower 10% of the **linear Kelvin** range; zero are near an edge in the
solver's log-temperature coordinates or the displayed relative JPEG scale. No final Contrast,
Saturation, Tint or Exposure in the common audit is near a rail. Neutral zero-saturation grading
wheels and cyclic hue angles are not treated as undesirable rails.

The −3 EV targets now solve beyond +2 EV; WB corrections can exceed the old +3,200 K allowance.
The completed fit still needs manual finishing for some dissimilar scenes and cannot recover
clipped content. All six contact sheets were inspected. A pre-final curve plateau on the airy
product was caught visually, fixed through the slope projection, and its ΔE fell from about 6.3
to 0.7 in similarity mode without the posterized patches.

| Technique ablation, same 12 similarity pairs | Mean ΔE | Mean abs ΔL5 | Mean abs ΔL95 |
|---|---:|---:|---:|
| Full v2 | 1.591 | 6.175 | 9.367 |
| Without warm start | 3.747 | 8.142 | 7.908 |
| Without CDF projection | 2.088 | 7.425 | 8.075 |
| Without joint refinement | 1.883 | 7.567 | 11.267 |

These are interacting ablations, not additive gains. Warm starts substantially improve ΔE and
shadow matching; CDF projection improves ΔE / L5 with a highlight tradeoff; joint refinement
improves both endpoints relative to stopping at the warm / projected seed. The two-reference
quality set cannot quantify median outlier robustness or cached-face gains; dedicated tests
cover those cases. Prototype / scene-statistic contributions were not separately isolated.

Artifacts: `target/matchlook-review/before/`, current `metrics.csv`, `summary.txt`, six `sheets/`,
`v2-final/solved-controls.json`, `ablation-*`, `review-fixes-audit.json`, and instrumented
`zz_matchlook_review-v2.rs`. The temporary evaluation test was removed from crate sources.

## Validation and performance

Final validation counts and final native / low-end timing are recorded below after completion.
Logs are retained under `target/matchlook-logs/review/`. Builds use the supplied environment,
pinned Rust 1.98.1, offline mode and unchanged CARGO_BUILD_JOBS=4, one cargo process at a time.
Only affected tests ran during development; the full suites of the four changed packages run
once at the end. Strict native-GPU acceptance does not skip unavailable / software adapters.

The benchmark follows real UI clicks through Analyze / Preview / Apply, the app task and file
loader paths, 500 synthetic 512 px proxies, one undo batch and a real GPU updated texture. It
measures the solver / app path, not a claim about decoding 500 full camera raws from disk.
Earlier runs and failures are retained, including the initial 83 s v2 run and later slower
uncached runs; they are superseded by the final measured source revision.

Screenshots: 28 panel, optional comparison dialog, reference wipe, scroll-to-summary, applied
per-photo, stale-analysis and Import / Export menu captures in dark / light at 250 / 400 px,
under `target/matchlook-ux/`. The preview / summary scroll body retains a reachable Cancel / Apply
footer. Narrow chips wrap as complete chips rather than vertical letters or clipped chrome.

## Provenance

Read the task, independent review, all three algorithm / open-source / product research reports,
Solver v2 plan, design system, existing Sync / jobs / library / preset / file-dialog / layer
patterns, and the pinned upstream source files. Added three PORTS rows and GPL notices:

- darktable `src/iop/exposure.c`, `0ef3dc0a9c81fb09e1e6c47ad598f9b0788cfa6f`: percentile / log exposure.
- RawTherapee `rtengine/histmatching.cc`, `0fdac3797aebbba4a751184e60278e5460bc56b7`, Alberto Griggio 2018: populated-CDF sparse tone mapping, adapted to quantile storage / editable-slider projection.
- darktable `src/iop/colormapping.c`, `efd77b42de8304cbc0368835802aeff605c49933`: soft a/b clusters / Shepard weighting, adapted to deterministic Oklab / luminance-zone targets.

All three are GPL-3.0-or-later. The slope projection, weighted-median aggregation, bounded
LM / IRLS solver and product orchestration are original Rust adaptations, not claims about
Capture One's internal algorithm. No upstream C/C++ is compiled or shipped.
