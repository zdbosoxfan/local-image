# Match Look findings and handoff — 2026-10-14

## Disposition

Paused at the owner's request. This is an archived implementation and research
checkpoint, not a completed or photographically accepted feature. Draft PR #14
targets v2 from `wip/cx-matchlook`. Shared color/LUT job J2 is already merged through
PR #15. The rendering compatibility fix is submitted separately so v2 does not need
the unfinished matching feature to receive it.

The feature checkpoint incorporates v2 through `c061f5d5`. v2 advanced to `d4d24ead`
while this work was being closed out; that newer integration is used for the separate
rendering PR, not claimed as tested in the archived Match Look implementation.
The feature branch also inherits the high-resolution prerequisite separately submitted
in PR #13. Current matching/color/cache work did not author RAW decoder changes.

## What works mechanically

Twenty mixed catalog/external references; version-three portable looks with previews
and options; nineteen adjustment exclusions; ordinary editable Develop output;
normalization independent of Strength; managed-layer replacement; global B&W;
Amount and exact Undo/Redo; source/edit/generation guards; and atomic cancellation.
Consistent Preview uses the complete selected batch. Exact source witnesses, bounded
retention and a metadata-only shared-style cache preserve settings across fresh and
reused execution. These mechanics are not evidence of excellent photographic transfer.

Native Sony ARW, DJI sensor DNG and merged HDR DNG pairs passed repeatability, layer,
opacity and global-B&W checks. Twenty-four changed transactions passed exact Undo/Redo,
and nine repeat no-ops added no Undo. The behavioral checks used Preview sources up
to 2560 pixels and 1024-pixel output, not full-resolution quality acceptance.

## Findings that prevented completion

### Photographic transfer

Global statistics do not reliably distinguish scene content, illumination and style.
Visual evaluation found unwanted casts and drift; a smaller fitting loss can accompany
a worse image. Five alternating cross-view transfers produced the following mean/max
OkLab x100 changes (each solve starts at camera defaults):

| Source | A4 versus A0 | B5 versus B1 |
| --- | ---: | ---: |
| Sony ARW | 3.264 / 6.131 | 3.096 / 7.615 |
| DJI DNG | 4.157 / 9.066 | 3.785 / 12.128 |
| Merged HDR DNG | 1.349 / 16.617 | 1.078 / 14.204 |

Rejected experiments, kept out of production:

| Experiment | Result and limitation |
| --- | --- |
| Forty-axis manually paired materials | Fitting loss improved while wall/sky/color regions regressed; material brightness requirements conflicted under different illumination. |
| Eleven-axis tone-only fit | Some sofa brightness improved, but chroma error and exterior regions worsened. Frozen color coefficients did not imply unchanged rendered chromaticity. |
| Depth-trained DINO patch correspondence | Reciprocal/margin/neighborhood gates admitted unrelated structures and cross-scene matches; useful sofa/floor support was absent. |
| Pristine DINOv2-S/14 correspondence | Official tensor parity and bounded Rust runtime passed; frozen photographic gates still admitted ceiling/wall and foliage/wall errors. No solver integration. |
| Original local style-prior head | 95,180 parameters; one fixed seed/configuration; 14 train, 5 dev, 5 test captures separated by room/view family. All five identity-style cases failed: mean OkLab 0.0277–0.1000 against 0.01, p95 0.0557–0.2143 against 0.03. Stopped before nonidentity test generation; no retuning or publication of weights. |
| Bounded Hellinger histogram audit | Paired dev-family AUC 0.7275 / 0.6372 against >=0.80 each; nuisance/style median 0.36425 against <=0.25. Numerical boundedness did not yield content/illumination invariance. No new head trained. |

The style-prior runtime passed at 118.447 ms warm p95, 174.684 MiB process peak and
2.149 ms cancellation acknowledgment. Those technical results did not overcome its
quality failure. Its small held-out set spans only two Sony room families; it is not
a broad camera benchmark. Train/dev-only postmortem found no binding or coordinate
bug; rare chroma-bin standardization dominated dev feature energy, and dev prediction
was worse than zero/mean baselines. No failed test images were reused for model tuning.

The public encoder inputs were Meta DINOv2 source revision
`7764ea0f912e53c92e82eb78a2a1631e92725fc8` and `facebook/dinov2-small` revision
`ed25f3a31f01632728cabb09d1542f84ab7b0056`, Apache-2.0. See the existing public
algorithm/open-source reports and `docs/PORTS.md` for production port attribution.
No experiment proves that all semantic or learned style methods fail.

### Throughput

These are actual app-path runs on 500 synthetic 512-pixel proxies with seven automatic
workers and physical Vulkan output. They are not 500 native decodes or a physical
four-gigabyte-machine test. The acceptance gate remains 90 seconds per measured phase.

| Workflow | Measured result | Outcome |
| --- | --- | --- |
| Consistent Preview, then cached Apply | 68.874 s / 66.338 s; UI p95 1.992 / 2.821 ms; peak 1011.5 MiB | Pass for these phases/configuration only |
| Direct consistent Apply | 142.069 s; later exact-chroma checkpoint 144.058 s | Fail |
| Direct independent Apply, 20 references, four-GB planning | 300-second watchdog; last progress 400 targets at 262.417 s | Fail; no completed 500 result |

Source-slot release and options-aware statistics preserve exact decisions/settings.
Isolated measurements improve, but no later completed 500-photo run establishes an
end-to-end gain. Do not lower admission reservations, substitute downsampled sources
or loosen the wall gate to turn these failures into passes.

### Native exports and memory

Five full-resolution F32 TIFF exports passed exact decoded sample, ICC and checked
metadata roundtrips: two Sony 50-MP captures, one DJI 100-MP reference and two merged
HDR 50-MP captures. The output domain was the app's SDR linear-sRGB rendition; this is
not HDR appearance acceptance. The layered 100-MP DJI target was not launched because
its conservative projected memory exceeded the private diagnostic bounds.

Releasing dead palette-filter vectors reduced a bounded fixture's peak by 34.63%.
For the same actual Sony layered export, sampled peak fell from 14.591 to 11.067 GiB
(24.15%). All 149,299,200 float sample bits, ICC/metadata and the complete TIFF remained
identical. Actual memory still exceeds the app's 2.781-GiB estimate and the private
8.556-GiB forecast. Neither physical low-memory acceptance nor 100-MP layered-target
acceptance is complete. Further candidates include eliminating full-frame temporary
copies and avoiding confidence-plane RGB expansion; they were not implemented.

### Rendering regression isolated during testing

Four unchanged Develop tests failed before the palette lifetime optimization. The
shared Gaussian policy change, rather than the palette change or RAW decoding, caused
the failures. An explicit Develop policy selecting the existing box engine restores
the expected rendering while keeping Library/editor sampled defaults. CPU and GPU use
the same plan; no expected values or thresholds were changed. See
`REPORT-develop-gaussian-compat.md` for scoped implementation and validation.

## Rea and implementation provenance

Rea investigation did not produce a validated Capture One-equivalent implementation.
Proprietary inspection stopped; exposed contexts did not author new matching algorithms
from those internals. Subsequent independent algorithm experiments used public/app inputs
and an unexposed implementation agent. Ordinary licensed-UI behavioral experiments on
Capture One were proposed, but no such automated experiment was completed. Do not treat
the investigation as a recovered specification or a legal-clearance result.

`MATCHLOOK-CLEAN-ROOM.md` records the process and exposure boundary. No recovered code,
weights, tables, proprietary analysis or vendor resources are included in this handoff.
Owner RAWs, session files, finals, annotations, private trained weights and photographic
proof sheets remain local. Original owner files were read only.

## Evidence retained locally

Detailed logs, source/binary hashes and measurements remain alongside this checkout
in the parent workspace's `work/` directories: `completion-audit/`,
`matchlook-behavior-acceptance/`, `matchlook-full-export/`,
`matchlook-full-export-palette-scoped/`, `matchlook-benchmark-checkpoints/`,
`clean-matchlook/pilot/` and `dense-feature-probe/`. These are local evidence locations,
not repository assets or publicly reproducible photo datasets. The public branch
contains the findings and provenance summaries, not those private files.

## If the owner resumes this work

1. Reconcile with current v2 and the separate rendering PR; preserve newer RAW, profile,
   lens and codec behavior. Avoid resetting the cache version to an older value.
2. Select a fresh independent representation/objective proposal and frozen evaluation
   before training or fitting. Keep the failed five-case test set out of retuning.
3. Phase B's control-response/rank/alias audit is prepared only: 36 joint parameters,
   24 palette output axes and discrete B&W; a proposed 726-render bounded probe was
   never run. It cannot alone repair reference-style identification.
4. Require held-out native visual/regional quality, repeat-transfer stability, actual
   direct/20-reference/native throughput and measured memory/full-export acceptance.
   Historical distribution tests, microbenchmarks and green unit tests are insufficient.

No further experiments or feature-completion claim are part of this closeout.
