# Match Look continuation checkpoint — 2026-10-14

Status: paused at the owner's request on 2026-10-14. Photographic quality and large-batch
acceptance are not complete. Work is preserved on `wip/cx-matchlook` through draft PR #14.
Start with `MATCHLOOK-FINDINGS-HANDOFF.md` for the final findings and resumption checklist.
Historical reports describe earlier checkpoints and are not current completion claims.

## Integration and workflow

The feature branch incorporates published v2 through `c061f5d5`, including the shared
codec/CMS and RAW Develop path, camera data, DeviceHealth recovery, Camera Raw preview
unification and full-precision GPU bridge, library safety, shared `li-color` and shared `li-raster`. J2 was independently
submitted and merged as PR #15; its report is `CODEX-REPORT-j02-li-color.md`.
The latest merge preserves v2's decoder-format routing and rawler CFA row/column fix.
The full app check passed after that merge. Three representative native Sony ARW,
DJI DNG and HDR DNG neutral previews remain PNG-byte-identical to their frozen prior
checkpoint, with unchanged original content hashes; this is not whole-corpus parity.
The branch inherits the earlier high-resolution RAW prerequisite `af3829f0`, separately
submitted as open PR #13. Current color/cache/measurement work does not edit `lc-raw`,
but the cumulative feature-branch diff against v2 still contains that prerequisite.

Current Match Look workflow includes twenty mixed catalog/external references, portable
version-three saved looks with preview images and tool options, nineteen individual
adjustment exclusions, managed-layer reapplication, amount/Undo/Redo, and generation,
source and edit guards. Consistent-batch Preview fits the full selected shared-style
context before rendering the active target, using the same solving path as Apply.

The rank-statistics optimization preserves every measured float bit against the former
stable-sort selector. Its isolated speed measurements do not establish app throughput.
The shared raster merge required forwarding Match Look's existing scoped sequential
execution to `li-raster`, so outer image parallelism does not recreate nested row work.
Default raster execution and formula outputs remain unchanged.

Uncached native RAW admission now forecasts full-frame decoder work even for a small
requested source. A cached thumbnail no longer hides the separate cost of cold native
verification. Catalog dimensions remain an estimate; this is not a universal allocation
cap or proof of four-gigabyte native decoding.

Temporary decoded-source retention is now byte-bounded before parallel results accumulate,
as well as in sequential work and between stages. Source cost includes confidence/profile
and white-balance allocations. Retention eviction is deterministic under differing completion
orders. Evicted stage-two sources must match recorded pixel and effective-decoder evidence;
changed or unverifiable sources discard the shared-style result. No source is downsampled
to obtain these savings. A single metadata-only shared-style cache now reuses the complete
successful analysis across Preview and Apply. It validates every selected source against
exact finite pixel and effective-decoder witnesses before use; any late mismatch discards
the speculative result. Cached and fresh paths preserve exact settings, including Amount
and active-photo changes. Partial cancellation applies no edits and creates no Undo entry.

## Photographic evidence and rejected experiments

The owner's session supplies original RAWs, merged LinearRaw DNGs, finished reference
images and held-out target exports. Originals are read only. Private photographs,
session databases, annotations, extracted binaries/models and experimental runners are
not included in this checkpoint.

New private behavioral runs use six preexisting native captures: Sony A1 ARW,
100-MP DJI/Hasselblad DNG and merged HDR DNG, with references edited through this app.
All three pairs decoded natively. Four fixed-reference applications produced exact
settings/pixel equality and kept the same managed layer. Opacity zero restored pristine
float/display output, half opacity attenuated normalization, and global B&W survived
zero opacity exactly. Twenty-four changed Apply transactions passed exact Undo/Redo;
nine repeat no-ops added no Undo. These are Preview sources up to 2560 pixels and
1024-pixel output checks, not full-resolution export or photographic quality acceptance.

Five alternating cross-view transfers, each starting at camera defaults, produced drift:

| Source class | A4 versus original A0 mean/max OkLab ×100 | B5 versus first B1 mean/max |
| --- | ---: | ---: |
| Sony ARW | 3.264 / 6.131 | 3.096 / 7.615 |
| DJI DNG | 4.157 / 9.066 | 3.785 / 12.128 |
| Merged HDR DNG | 1.349 / 16.617 | 1.078 / 14.204 |

This records actual drift, without asserting mathematical inversion or zero-drift parity.
Assistant visual review also found wall-cast and later cool/cyan shifts in the Sony pair;
it is not owner acceptance. Private settings, source/binary hashes, panels, regional
diagnostics and original-file stat checks are retained outside the repository.

Two private experiments tested whether manually paired materials could repair the
global-statistics objective. Both used equal render budgets against control variants;
target finals were opened only after fitting. Neither is production behavior.

- The forty-axis experiment improved its fitting loss while introducing unsupported
  wall, sky and color regressions. Same material labels demanded contradictory
  brightness changes under different illumination.
- The eleven-axis tone-only experiment froze all color coefficients and used exactly
  eighty-nine evaluations per variant. Interior sofa brightness improved substantially,
  but its localized chroma error increased and contrast flattened. The exterior's
  excluded-region color difference worsened. Fixed channel curves and tonal grading can
  change rendered chromaticity when their luminance input changes; frozen coefficients
  do not establish frozen output color. Per-stage causal attribution is unproven.

These failures reject those specific objectives, not the requirement for excellent
matching. A smaller optimized training loss is insufficient photographic acceptance.

The installed Depth Anything V2 Small backbone executes through the existing pure-Rust
tract stack with intermediate token outputs and a pruned depth head. Synthetic runtime
checks establish deterministic, finite outputs and technical feasibility only. Its
depth-trained descriptors are not unchanged standard DINOv2, and correspondence quality
was subsequently assessed independently. The tested cosine/reciprocal/margin/neighborhood
gates admitted structural false matches across unrelated scenes and did not establish useful
sofa/floor support. That specific depth-trained route stays out of the solver.

A separate pristine standard DINOv2-S/14 diagnostic uses a backbone-only implementation with
the existing Candle CPU dependencies and correct positional interpolation. Synthetic comparisons
against pinned official source passed for final and selected intermediate patch tensors,
including rectangular inputs. Four-thread warm p95 at 224 pixels was 79.1 ms, with approximately
174 MiB process peak memory and cancellation acknowledgment below 3.4 ms. These are diagnostic
measurements, not app throughput or evidence of photographic matching quality. Subsequent
tests on four distinct photographic inputs passed pinned official tensor parity but failed
the frozen correspondence gates: ceiling wood matched walls, a cross-scene foliage/wall
match passed, and useful sofa/floor support was absent. This route also stays out of the
solver. No model or experimental runner is included in this checkpoint's production behavior.

Implementation input separation and provenance are documented in
`MATCHLOOK-CLEAN-ROOM.md`. Feature work remains in the existing fork; target-derived proprietary
research material is excluded from the implementation inputs.

## Validation and remaining gates

At the shared-raster integration checkpoint:

- Full `local-image` app check passed after the v2 merge.
- Shared/Library raster suites: thirty-four passed, one intentionally ignored.
- Match Look pipeline suite: sixty passed.
- Native-RAW admission: three focused tests passed.
- Engine Match Look/source-retention/key checks: forty passed, four intentionally ignored.
- Source metadata heap-cost regression: one passed. UI Match Look checks: sixteen passed,
  four intentionally ignored. The full app check passed after the latest GPU bridge merge.
- Native benchmark extrapolation corrected: parallel batch wall time must not be divided
  by worker count a second time. An extrapolation is not an actual five-hundred-photo run.

At shared-style cache checkpoint `3fb6eaeb`, the focused engine suite passed forty-seven
tests (five intentionally ignored), the UI suite passed sixteen (four intentionally
ignored), and the full app check passed. Both physical Vulkan GPU regressions passed;
solved global/layer and later-WB comparisons differed by at most one output code value.
Typed source-witness hashing reduced the isolated 2560-by-1920 cold measurement from
about 207 ms to 62 ms without changing its exact evidence contract.

Two actual application-path benchmarks used five hundred synthetic 512-pixel proxies,
seven automatic workers, and an RTX 5090 Vulkan renderer. These are not native-RAW or
low-memory acceptance runs. Each measured phase retains the ninety-second gate:

| Workflow | Preview | Apply | UI frame p95 | Sampled peak RSS | Result |
| --- | ---: | ---: | ---: | ---: | --- |
| Preview, then cached Apply | 68.874 s | 66.338 s | 1.992 / 2.821 ms | 1011.5 MiB | Pass |
| Direct Apply, no Preview | — | 142.069 s | 2.679 ms | 939.3 MiB | Fail |

Both workflows changed all five hundred targets in one Undo entry and produced an actual
GPU texture. Cached Apply used 26,500 renders; direct Apply used 83,000. The new ignored
direct-Apply benchmark deliberately exposes the remaining throughput failure; the cached
pass does not establish completion of the general performance requirement.

At `0a61f9bd`, exact chroma reuse removes two repeated hypot calculations per measured
pixel. All sixty-two focused pipeline tests pass, including a full-Look bit oracle against
the original formula. Isolated 60/256/512-edge measurements improved by approximately
7–16%, with at most 2 MiB additional transient solver scratch. A further actual direct
five-hundred-proxy run took 144.058 s, with 83,000 renders, 2.779 ms UI frame p95 and
930.5 MiB sampled peak RSS. It still fails the unchanged ninety-second gate. These single
runs show no demonstrated end-to-end gain; they do not establish a statistical regression.
The narrow-panel workflow test now analyzes all twenty references, checks every tile
against the actual panel bounds, pins/compares references, matches an underexposed target,
and reaches its quality summary and weak-match review through real UI actions. This
expanded focused test passes; the prior ten-reference coverage was insufficient for the cap.

At `e4c78800`, target identification skips appearance statistics that the solver discarded.
Its signature/content bits, every solved field, logical render budget and cancellation
poll boundary match the original full-measure oracle. Public reference analysis is
unchanged. The focused pipeline suite passes sixty-four tests with two timing diagnostics
intentionally ignored. A contended isolated 128-by-96 descriptor measurement improved
from 7.142 ms to 1.835 ms; no end-to-end throughput claim follows.

The ignored app benchmarks now accept `LC_MATCH_LOOK_BENCH_REFERENCES=1..20` and
`LC_MATCH_LOOK_BENCH_MODE=independent|consistent`, in addition to target count, thread,
memory-planning and RAW-directory options. Output records the mode/reference count,
actual target source count/repetition and planning override, with separate configuration
filenames. Five hundred native target instances now face the same ninety-second wall
assertion; they no longer bypass it. Actual native five-hundred acceptance is still absent.
Two three-target configuration smoke checks passed on the physical GPU: independent
Apply with twenty references/four-GB planning and consistent Preview/Apply with one
reference/eight-GB planning, both with two workers. These small runs validate configuration
and GPU/Undo paths, not five-hundred throughput or a physical low-memory machine.

A subsequent quiet five-hundred-target independent run with twenty references and
four-GB memory planning failed the 300-second watchdog. Its last progress entry was
400 targets at 262.417 seconds; it has no completed five-hundred result. The seven-target
diagnostic confirms all seven targets require larger near-match verification. That path
held a heavy decode slot through CPU hashing and two render/measurement calls. The new
resource transition releases the decode slot after loading but retains every admitted
byte. Forty-nine affected engine tests pass, including exact near-match/failure/cancel
and memory-pressure regressions. Seven-target old/new metadata and logical renders match,
but wall time was 3.738 versus 3.753 seconds: no speedup is demonstrated. Conservative
byte admission remains a likely bottleneck; no reservation was weakened to pass a gate.

Independent style-prior capability research now uses a visually reviewed, frozen
14-train/5-dev/5-test split, with whole room/view families separated and Kaufman held
out from training/development. Earlier timestamp-only split labels were discarded after
overlapping rooms were identified. The five test captures span only two Sony room
families; this limited experiment cannot establish broad camera or photographic quality.
Training/evaluation uses the original public-input protocol, ordinary editable controls,
one configuration/seed and private local weights; it is not production behavior.
That single candidate is now stopped: Rust parity passed, runtime p95 was 118.447 ms,
peak working set 174.684 MiB and cancellation 2.149 ms, but all five identity-style
targets failed the frozen quality gate. Mean OKLab errors were 0.0277–0.1000 versus
the 0.01 limit. No 320 nonidentity target oracles were generated after this failure.
Postmortem found no row/coordinate/checkpoint binding bug. Dev error was worse than
zero/mean prediction baselines, with rare chroma-bin standardization a major contributor.
A subsequent fixed, bounded histogram-response audit also failed its invariance gates;
neither method is promoted or retuned on the failed photographic tests.

Five actual full-resolution exports passed lossless F32 TIFF roundtrip: both Sony
50-MP captures, the 100-MP DJI reference, and both 50-MP HDR captures. Every decoded
float sample in the linear-sRGB output domain, ICC and checked metadata remained exact.
Native HDR source maxima reached 16.15/23.66; the exported floats represent the app's
SDR rendition, so these checks do not certify HDR highlight appearance. The 100-MP
DJI layered target was not launched under the diagnostic memory bounds.

These exports exposed a path-dependent memory gap. Sony reference used 4.16 GiB,
Sony layered target 14.59 GiB, DJI reference 8.31 GiB, and HDR reference/target
4.14/5.95 GiB. The palette filter retained dead full-frame vectors. At `4b615d60`,
earlier release of those vectors reduced bounded-fixture peak RSS by 34.63% with the
same output bits. Repeating the identical Sony layered export reduced sampled peak
from 14.591 to 11.067 GiB (24.15%); all 149,299,200 float samples, ICC/metadata and
the complete TIFF SHA remained identical. This still exceeds both the app's 2.781-GiB
estimate and the private 8.556-GiB forecast. Native low-memory acceptance remains open.

The broader pipeline suite exposed four failures already present on its baseline:
capture clipped/border preservation, Upright vertical precision, and two unchanged
old-settings golden tests. The palette change reproduces identical failures and all
twenty printed golden rows. A controlled Gaussian-policy experiment restores all four
tests and all twenty expected rows using the existing box engine. An explicit Develop
compatibility policy is now implemented and validated: 291 pipeline tests and 36 raster
tests pass; all twenty expected render rows and fourteen settings hashes are unchanged.
Physical RTX 5090/Vulkan checks pass 32 Gaussian cases and eighteen full-render cases.
The full app check, formatting and diff checks pass. Library/PC sampled defaults and
small-sigma behavior remain intact. No golden values, thresholds or ignores were changed.
Strict Clippy is unavailable because the installed driver and Fedora compiler metadata
do not match. See `REPORT-develop-gaussian-compat.md`; the fix is being submitted
independently of the unfinished Match Look feature.

Phase B (editable-control response rank/alias analysis) was prepared but never executed.
No further experiments are authorized by this paused checkpoint. Private originals,
research runners, model weights, annotations and detailed logs remain local.

Remaining work includes automatic evidence that separates material, composition and
illumination; stable color/tone fitting with regional and output-resolution safeguards;
held-out native photographic acceptance; measured low-memory acceptance of bounded retention;
faster direct Apply; and current native and low-memory measurements. Historical
eighty/ninety-second targets remain unresolved for all required workflows;
a longer watchdog does not replace them.
