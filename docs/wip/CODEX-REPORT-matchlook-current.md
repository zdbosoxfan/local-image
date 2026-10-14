# Match Look continuation checkpoint — 2026-10-14

Status: implementation in progress; photographic quality and large-batch acceptance are
not complete. Work stays on `wip/cx-matchlook`, targeting `v2` through draft PR #14.
Historical reports describe earlier checkpoints and are not current completion claims.

## Integration and workflow

The feature branch incorporates published v2 through `dda820a4`, including the shared
codec/CMS and RAW Develop path, camera data, DeviceHealth recovery, Camera Raw preview
unification, library safety, shared `li-color` and shared `li-raster`. J2 was independently
submitted and merged as PR #15; its report is `CODEX-REPORT-j02-li-color.md`.

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

## Photographic evidence and rejected experiments

The owner's session supplies original RAWs, merged LinearRaw DNGs, finished reference
images and held-out target exports. Originals are read only. Private photographs,
session databases, annotations, extracted binaries/models and experimental runners are
not included in this checkpoint.

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
must be assessed independently before any solver integration.

## Validation and remaining gates

At the shared-raster integration checkpoint:

- Full `local-image` app check passed after the v2 merge.
- Shared/Library raster suites: thirty-four passed, one intentionally ignored.
- Match Look pipeline suite: sixty passed.
- Native-RAW admission: three focused tests passed.
- Native benchmark extrapolation corrected: parallel batch wall time must not be divided
  by worker count a second time. An extrapolation is not an actual five-hundred-photo run.

Remaining work includes automatic evidence that separates material, composition and
illumination; stable color/tone fitting with regional and output-resolution safeguards;
held-out native photographic acceptance; bounded decoded-source retention; exact
shared-style caching across Preview/Apply; and current native, proxy, low-memory and UI
responsiveness measurements. Historical eighty/ninety-second targets remain unresolved;
a longer watchdog does not replace them.
