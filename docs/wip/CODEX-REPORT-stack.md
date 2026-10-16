# Develop layer stacking report

Date: 2026-10-13. Task: `CODEX-TASK.md`. Work continues the existing Develop tool implementations,
GPU kernels, cache infrastructure and A/C UI work in this branch. No commit was made.

## Result and representation

Background finishes first. Visible `Mask.tools` run in stored list order, bottom to top, on
completed linear Rec.2020 pixels from the preceding layer. Each full tool set blends **once**:

```text
out_0 = Background(source)
out_k = out_(k-1) + (Tools_k(out_(k-1)) - out_(k-1)) * mask_coverage * amount * opacity
output = OutputTransform(out_top)
```

The stack is **display-referred linear Rec.2020, f32 and unquantized**, with values above 1
and negative channels retained between layers. Background's camera look establishes developed
luminance; it is no longer scene-radiometric after that nonlinear tone response. Repeating a
RAW tone map on each layer would break neutral layers and exposure composition. Output gamut
containment, primaries conversion, proofing and transfer encoding occur once after the stack.
Background's creative tone response is still intentional compression; later layers cannot undo
RAW sensor clipping or recover data already removed by a creative tool. Layer exposure itself
never clips. Curves extend their encoded tails outside 0–1 instead of pinning all bright values
to the last LUT entry. Working tone evaluation omits display gamut containment.

CPU `render`, `render_cached`, the H/S candidate entry point and the prepared-scene helper use
the same stacking rule. GPU stacking uses floating-point device buffers, native primary/tool
passes, the shared packed parameters, and a final display pass/readback. Its cache keys mirror
the CPU's rolling Background/prefix keys. Export NR retains the **existing** precise host VST
forward/backward transforms around GPU wavelets for 16-bit accuracy; interactive NR stays native.
No new dependency, unsafe code, settings field or persistence migration was introduced.

## Research followed

The sequential composition requirement takes precedence over products with different engines.
These sources were read before choosing the representation and are the basis for the design:

- [Adobe: adjustment-layer scope and stacking](https://www.adobe.com/learn/photoshop/web/limit-adjustment-layers):
  adjustment layers affect the layers underneath, and several adjustments can be stacked over
  the same underlying content. This supports the requested compositional model.
- [Affinity Photo 2: adjustment layers](https://affinity.help/photo2/en-US.lproj/pages/Layers/adjustmentLayers.html):
  adjustments have mask properties, remain editable, and moving them changes which underlying
  layers receive the correction. Its below-layer scope supports the same model.
- [Capture One: working with multiple layers](https://support.captureone.com/hc/en-us/articles/360002615097-Working-with-multiple-Layers):
  documents the stack, visibility, opacity, masks and adding a layer at the top. This official
  article does **not** specify a universal adjustment-tool processing order. Searches of the
  official help articles found no contract that reordering adjustment layers runs an entire
  RAW pipeline per layer. I did not infer Photoshop semantics from Capture One's layer UI or
  treat community discussions as an authoritative processing-order specification.
- [darktable: pixelpipe and module order](https://docs.darktable.org/usermanual/development/en/darkroom/pixelpipe/the-pixelpipe-and-module-order/):
  processing order is module order, distinct from edit history. Its pipeline can reorder
  modules, but it does not group all instances belonging to one Develop adjustment layer into
  a complete finish before starting the next. That distinction is why this task replaces the
  previous interleaving of layer instances at canonical tool stages.
- [darktable: color pipeline](https://docs.darktable.org/usermanual/development/en/special-topics/color-pipeline/):
  distinguishes linear encoding from scene/display reference and explains unbounded floats
  and masking above display white. The choice of an unclipped **developed** linear buffer here
  is an application-specific inference from that distinction and the owner's full-Background
  requirement, rather than a claim that a finished camera look remains scene-linear.
- Existing project implementations were retained: `primary`, `tone2`, `llf`, `eigf`, `ucs`,
  `balance`, `colorequal`, `detail/nr`, `finish`, and their GPU twins. Their upstream algorithm
  provenance remains in module headers and `docs/PORTS.md`. Threading follows
  `REVIEW-threading-policy-fable.md` and the application's `lc-sysmem`-initialized global rayon
  compute pool; no additional layer worker pool or thread-per-layer fan-out was added.

## Per-tool semantics on developed input

| Tool | Layer behavior |
|---|---|
| Exposure | Multiply current linear RGB by `2^EV`, then blend once. A Filled +1 EV layer doubles what's below; two Filled +0.5 EV layers compose to +1 EV. Background RAW exposure occurs before its camera tone map, so it deliberately differs. |
| WB | Apply the existing luminance-normalized chromatic-adaptation change from Background's effective white to this layer's selected white, to the current pixels. It is a relative adaptation, not another RAW decode. |
| Highlights/Shadows | Existing log-EV tone fields, guided by a proxy built from this layer's developed input. Bright/dark weighting refers to that input. Sensor clip-confidence suppression and RAW reconstruction are Background-only. |
| Whites/Blacks | Developed-image EV gains on current luminance: Whites selects +0.5..+2.5 EV around 0.18; Blacks selects -6..-3 EV. Exposure shifts that selection. This keeps Whites responsive within ordinary display white, rather than only above the RAW scene threshold. Neither adds Background settings nor repeats its base curve. |
| Contrast | Existing slider slope (`1 + 0.55*c` positive, `1 + 0.4*c` negative), applied about linear middle grey 0.18. It has no display-white clipping shoulder. |
| Clarity | Existing local-Laplacian mid-scale contrast on the current layer input, including its clarity mode. |
| Texture/Structure | Existing EIGF detail bands on current pixels, retaining output-to-sensor scale for radius interpretation. |
| Dehaze | Recompute dark channel and airlight from this layer's input; apply the existing haze correction there. It cannot restore clipped RAW channels. |
| NR | Estimate noise and denoise the developed input using the existing VST/wavelet model. Upper layers denoise the edited image, not a saved copy of the original. |
| Sharpening | Build this layer's log-luminance blur/masking planes from its current input and its own radius/detail settings. |
| Curves | Existing master/parametric/channel and Refine Saturation behavior in encoded working primaries, returned to linear; unit-slope tails outside the tabulated range retain headroom. |
| HSL/mixer | Existing UCS22 color equalizer, selecting and changing the colors currently below this layer. |
| Point Color | Existing OkLCh selections/adjustments after this layer's primary color tools, evaluated on developed colors. |
| Skin Tone | Existing UCS22 reference window/uniformity/lightness on this layer's input. The layer range diagnostic uses the image below it, including on warm cache hits. |
| Color grading | Existing UCS22 balance/grading applied to current colors/luminance, independent of Background's settings. |
| Historical vignette/grain/B&W layer values | Remain loadable/renderable in each layer's complete tool pass. Their UI availability remains Background-only. Grain is returned to linear working RGB before the next layer. |

## Caching, memory and practical region work

- Background settings are separated from `tools`. Imported `adjust` masks stay in their original
  stages and still use their original sums/coverage; they are not applied twice. Neutral local
  adjustments do not make a layer mask edit invalidate Background's expensive stages.
- Completed Background and layer outputs are cached. Editing layer k recomputes k..top;
  Background edits recompute the full stack. Prefix keys include coverage, amount, opacity,
  tool settings, processing quality, geometry/source dependencies and the preceding prefix.
  GPU deep NR precision has a distinct key from preview precision.
- Filled masks use scalar coverage and have no full-image alpha/component allocation. Exposure-
  only layers skip all primary/spatial preparation. Zero scalar coverage skips tool work.
- Pixel finishing and exposure/blend arithmetic skip uncovered pixels where practical. Spatial
  filters retain full-image context for their neighborhoods and global statistics, avoiding
  mask-edge seams. This is not a promise of arbitrary tiled/ROI filtering for every tool.
- Stack retention is capped at 512 MiB per output size, with the existing two cached view sizes.
  Older prefixes/fields can be evicted on large exports; one current image remains retained
  even when it alone exceeds the cap. Preview prefixes fit the budget. Eviction preserves
  values and keys and affects reuse only. Cache byte reporting includes retained stack images,
  alpha planes, primary fields and finishing planes.
- CPU uses the existing global bounded compute pool; GPU uses the existing short row-band
  submissions. All layer execution is sequential and there is no new concurrency policy.

## Compatibility and tests changed

- The six historical no-layer golden hashes are unchanged, including legacy Lightroom masks.
  No-layer CPU/GPU rendering keeps its original path. Old settings still round-trip without
  new serialized fields.
- Layer documents intentionally change rendering. `RENDER_CACHE_VERSION` is bumped 16 → 17.
- `layers.rs`, `lib.rs` stage order and `DEVELOP-DESIGN.md` §3.2 describe the new model.
- **TEST-CHANGED:** the old CPU full-layer/global-tool equivalence test is replaced with a
  finished-RAW-Background +1 EV check and an explicit assertion that pre-tone-map RAW exposure
  differs. This equivalence encoded the rejected interleaved semantics.
- **TEST-CHANGED:** the old Background+layer exposure-sum assertion is replaced with two
  stacked half-stops equaling one full stop, retaining the composition requirement.
- **TEST-CHANGED:** Develop A's real Filled-layer/Background exposure UI assertion retains
  equivalence for its rendered demo and asserts distinct results for a RAW walkthrough.
  The clicks, drags, history and layer routing assertions remain.
- **TEST-CHANGED:** the two `tests_toolset` golden render hashes for its explicitly layered
  fixture are re-recorded for sequential processing. Its other fourteen render goldens and
  all sixteen settings hashes remain unchanged. Together with `tests_layers`, twenty no-layer
  render goldens remain pinned.
- New CPU tests cover the owner's heavy-Background → HSL → gradient scenario, curve/exposure
  ordering, reversible over-white intermediate exposure, prefix invalidation counters and
  warm spatial-mask diagnostics, and independent Whites/Blacks responsiveness within developed display range. Existing all-tool/masked-area/opacity tests still apply.
- New GPU tests cover the owner scenario, cache counters, order, half-stop composition,
  recoverable over-white exposure, 12 complete tool-set cases with spatial and Filled coverage
  plus U16 output (36 comparisons), and imported/hidden/warm mask diagnostics. Existing
  primary-layer and Develop C GPU tests remain enabled and pass on physical Vulkan hardware.
- New Develop A UI test creates two Filled layers, drags exposure, drags layer order, verifies
  that the **actual displayed preview pixels** change, then undoes and verifies pixel restoration.

## Verification and timings

Final validation passed; suite counts and interactive measurements follow.
Logs and generated benchmark input are retained under `target/stack-logs/` and
`target/develop-stack/reference/`. The generator is all Rust, using the existing DNG writer and
RAW fixture conventions, at `target/refvec/stack_raw.rs`. It produces an 8256 × 5504 Bayer DNG
(45.44 MP); this is a synthetic RAW decode/app-path benchmark, not a sensor-photograph benchmark.
The existing `develop_a_layer_drag_interactive_gpu_budget` exercises actual layer slider pointer
moves, the app's worker renderer, current textures, and the draft path on RTX 5090/Vulkan.
Six measured moves per case, editing layer 4 in the 4/8-layer cases; preview completion is
awaited per move. The reported p95 is the maximum of those six samples, not sustained drag FPS.


### Final checks

All commands sourced `~/.local/share/local-image-dev/claude-env.sh`, used the pinned 1.98.1
compiler, offline builds, the same worktree `target/`, four build jobs and one cargo build lane.
No whole-workspace suite was run. Normal suite results:

| Changed crate | Unit passes | Integration passes | Total passes | Existing ignores |
|---|---:|---:|---:|---:|
| lightcraft-pipeline | 219 | 201 | 420 | 3 |
| lightcraft-gpu | 47 | 50 | 97 | 11 |
| lightcraft-engine | 508 | 384 | 892 | 6 |
| lightcraft-ui-egui | 332 | 0 | 332 | 5 |
| **Total** | **1106** | **635** | **1741** | **25** |

All four crates' doc-test runs passed (zero examples). All-target clippy with `-D warnings`
passed for all four changed crates, and formatting/diff checks passed for the edited code/docs.
The initial combined suite stopped on `tests_toolset`'s old **layered** render golden; its failure
log is retained. Only its two layer render hashes were re-recorded, with TEST-CHANGED reasons;
no-layer render hashes and settings hashes were not changed. Pipeline/UI full suites and the
final GPU full suite passed afterward. Engine's full suite passed in the initial run; the only
engine edit is the cache-version constant. The final engine doc-test run also passed.

Additional explicit acceptance runs:

- **13 passed** RAW Develop A/C walkthrough tests, using the generated DNG through the real
  headless application; the benchmark remains ignored in that normal filtered run and was
  run explicitly below. This includes the new reorder/pixel/undo test.
- **1 passed** explicit interactive GPU benchmark, including 1/4/8 brush layers and 8 Filled
  layers. The physical adapter was **NVIDIA GeForce RTX 5090 (Vulkan)**. No software adapter or
  no-GPU skip is counted as GPU validation.
- The three added GPU tests pass in the final full suite. The full-tool-set test also passed
  independently after the final developed Whites/Blacks adjustment, including all 36
  spatial/Filled/U16 comparisons under mean <0.5 and max ≤3 display levels. The deep NR case
  permits only the pre-existing NR variance-transform host stage; preview cases require no
  host pixel stage.
- **20 no-layer render goldens** and all persisted-settings hashes remain unchanged. Two
  layer render goldens deliberately changed to the new processing model.

Final interactive measurements (six slider moves per case):

| Layer count/type | UI frame p95 | Change-to-texture p95 | Retained GPU stage-cache bytes |
|---|---:|---:|---:|
| 1 Brush, exposure + clarity | 1.41 ms | 14.96 ms | 252,363,384 |
| 4 Brush, exposure + clarity | 1.28 ms | 19.54 ms | 640,256,088 |
| 8 Brush, exposure + clarity | 1.97 ms | 22.63 ms | 858,511,168 |
| 8 Filled, exposure | 1.39 ms | 6.45 ms | 296,873,496 |

Every case passed the existing **16.7 ms UI-frame** assertion. Stage-cache bytes include source,
Background and both cached sizes as well as stack state; they are not total process VRAM or
transient allocation peaks. These timings describe the RTX 5090 app path and do not claim the
same latency on weaker GPUs. Structural safeguards for weaker hardware are bounded retention,
shared compute threads, scalar/zero coverage, skipped uncovered pixel work, cached prefixes,
and short GPU submissions.

Final logs:

- `target/stack-logs/full-tests.log` (engine full suite; initial layered-golden failure preserved).
- `target/stack-logs/full-pipeline-ui.log` (final full pipeline/UI suites).
- `target/stack-logs/full-gpu-final.log` (final physical-GPU full suite).
- `target/stack-logs/engine-doc-tests.log`.
- `target/stack-logs/clippy-final2.log`.
- `target/stack-logs/raw-walkthroughs.log`.
- `target/stack-logs/ui-benchmark-final.log`.
- `target/stack-logs/gpu-tools-final3.log` (final 36-case tool parity acceptance).

Reproduce the RAW runs after sourcing the build environment:

```sh
LC_REQUIRE_GPU=1 LC_DEVELOP_A_REFERENCE="$PWD/target/develop-stack/reference/synthetic-45mp.dng" LC_DEVELOP_C_REFERENCE="$PWD/target/develop-stack/reference/synthetic-45mp.dng" cargo test --offline -p lightcraft-ui-egui tests_develop_ -- --nocapture
LC_REQUIRE_GPU=1 LC_DEVELOP_A_RAW="$PWD/target/develop-stack/reference/synthetic-45mp.dng" cargo test --offline -p lightcraft-ui-egui develop_a_layer_drag_interactive_gpu_budget -- --ignored --nocapture
```

No required task work remains. Spatial filters still retain neighborhood/statistical context
rather than arbitrary mask-only tiles, large export caches may evict prefixes, and the existing
16-bit NR precision bridge remains as described above. No unrelated UI, pc-*, li-seg/li-ai,
queue tooling or owner's files were edited. The pre-existing modification to `CODEX-TASK.md`
was left untouched. All implementation/report changes remain uncommitted in the working tree.
