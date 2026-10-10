# Vector package 32 — selection

Date: 2026-10-10. Branch: `wip/vec-32-vector-selection`. Starting revision:
`22cf5a126e0c3fd92e63bf541f5623ee368c5257`. Changes are uncommitted.

## Status

Package-owned implementation and integration fragments are supplied. Production
registration and dependency integration remain required before this package can be
called complete. The branch's prior persona, shape, node, pen and snapping work was
preserved. No shared production registry, router, preferences, manifest, lockfile,
catalog, raster selection code or package 03 file was changed.

The required architecture, persona/owner decisions, quality plan, task README and gap
audit were read before implementation. This branch did not already contain a package
32 selection slot. Its existing session selection helpers increment document revision;
32 therefore publishes the existing session selection fields through one non-journaled
command, with preflight, rather than calling those helpers or making a document edit.

## Delivered behavior

| Task point | Implementation/evidence |
|---|---|
| Group Selection | Leaf-first hit, repeated clicks ascend one parent, stop before isolation root; Node+Alt temporary route; existing session selection and Layers highlight. |
| Vector Magic Wand | Conjunction of enabled fill/stroke colour, stroke weight, opacity and blend; continuous 100× Oklab distance; bounded per-attribute options; Shift add/Alt subtract. |
| Object/Node Lasso | Actual curve enclosure/contact using V1 contains/nearest/bounds and flattened edges; compound-hole test; nodes across selected curves go into existing 03 DirectSelection. |
| Select Same | All 14 requested criteria, ordered whole appearance stacks, item matching, five numeric comparisons, live shape kind and name. Missing style/rotation providers produce explicit errors. |
| Select Object | All 10 requested criteria, inherited locks/visibility, hidden toggle, same container scope, open/singleton components and clipping masks. Symbols use the named package 15 provider. |
| Other queries | All/deselect/inverse, no-wrap above/below, active-artboard overlap and validated Reselect. |
| Scope | Edit All Layers, active artboard/sibling fallback and isolation; authoritative 00/37 hooks used by both captures and commands. |
| Transaction discipline | Document/revision/selection/scope guards; entire target preflight; detached overlay bounds; one `vector.select` command at release; revision/journal/history/document Arc unchanged. |
| Cancellation/compatibility | Escape, focus/modal/tool/persona/module changes and stale targets; frozen pointer mapping; modifiers update during capture; serde defaults for old options; object publication clears obsolete 03 anchors. |
| Rendering/performance | Detached bounds overlays under all Vector tools, including ordinary Path Selection after menu/MCP commands; existing Perf span. Strict 500-object release gate passes in the registration copy with the two supplied shared geometry-bar fixes. |
| Handoff | Exact compiled slots, canvas/frame hooks, provider signatures, memory/prefs bridge, tool IDs/keys/icons, menu/context and attribution fragments in [integration/32.md](vector-tasks/integration/32.md); translations in [i18n/32.tsv](vector-tasks/i18n/32.tsv). |

The disabled Edit All Layers behavior follows Affinity's active-artboard scope, including
nested groups. Without artboards, immediate siblings are the documented fallback; this
implements the task's nested six-shape oracle. Position-only locks permit selection;
inherited pixel/all locks do not. Width/height use geometry bounds. Non-path text/symbol
contact uses the model's surface bounds; exact outlined glyph/instance contact awaits
those providers. Strokes use the visible top appearance item's width; contact flattening
tolerance is 0.025 document pixels. Effects are compared for Appearance, not expanded into
lasso geometry.

## Verification

No adapter skip or missing corpus is counted as a pass.

- Development compiled UI/engine check: passed. Initial engine type mismatches were fixed.
- Final focused engine run: **21 passed, 0 failed, 0 ignored, 922 filtered** (20 independent
  engine oracles + 1 strict real-GPU gate), including the active-artboard group boundary
  and detached command invariants. Log: `target/selection32/focused-engine-final.log`.
  An earlier broader name filter passed 65 related tests. The first disposable invocation
  lacked the shared `tests/support` path; the copy setup was corrected before this pass.
- Focused development UI run: **9 passed, 0 failed, 1 ignored, 1020 filtered**: seven
  real-input widget tests and two real PhotocraftApp GPU tests. Log:
  `target/selection32/focused-ui-complete.log`. Developing these tests exposed/fixed canvas clicks passing
  through menus; real Layers assertions use egui's actual toggle accessibility flag and
  the requested repaint. No test gate was disabled.
- Final focused **release** UI run (including the ignored benchmark): **10 passed,
  0 failed, 0 ignored, 1019 filtered**. Log: `target/selection32/release-focused-ui-final.log`.
  The 500-shape real-app test uses ordinary Path Selection, 30 single-event pointer frames,
  and GPU completion/readback: median **4.256 ms**, p95 **4.420 ms**, p99 **4.431 ms**.
  All **500/500** overlay edges were detected; sampled canvas interior max diff **0/255**;
  the document remains unchanged. Selection route typically costs 0.05 ms. This pass
  **requires the two proposed shared geometry-bar fixes below**, applied only in the copy.
- The original registration-only app fails performance: one-frame p95 **82.025 ms**.
  Its selection route is 0.05–0.06 ms and GPU/readback about 2.2 ms; the existing persona
  geometry bar repeatedly rebuilds full document walks for every selected layer. Debugger
  samples pinpoint `GeometryRequest::validate` and `GestureTarget::is_current`. A first
  linear validation fix reduces p95 to **32.289 ms**, still failing; removal of the redundant
  per-ID existence checks produces the passing final result. Logs:
  `release-500-diagnosis.log`, `release-ui-stack.log`, `release-500-with-bridge.log`,
  `release-ui-stack-with-bridge.log` under `target/selection32/`. An initial 164.637 ms result
  incorrectly included a separate modifier-event frame; the final benchmark queues exactly
  one real pointer event per frame. The 16 ms assertion was never relaxed.
- All-target clippy for both changed crates: **passed**, `-D warnings`, isolated
  `target/clippy`. The benchmark's release guard was changed from a constant assertion
  to an explicit runtime guard after the first clippy invocation identified that lint.
  Final log: `target/selection32/clippy.log`.
- Full tests of both changed crates were run **once**, with `--no-fail-fast` and two test
  threads: **2,681 passed, 1 failed, 34 ignored**. Engine: **1,571 passed, 0 failed,
  19 ignored**; UI: **1,110 passed, 1 failed, 15 ignored**. Every package 32 test passes,
  including both app GPU tests with the final diamond path/cache fixture. The release-only
  benchmark is ignored in this debug run and was explicitly passed in release above.
  Log: `target/selection32/full-changed-crates.log`.
  The sole failure, `i18n::tests::every_tl_literal_is_translated`, reports **27** existing
  Japanese persona/Node/pen keys. Every failing key already appears in unchanged root
  source; 32 adds no `tl!` literals. The earlier 03 report listed 11 Node keys, but this
  branch also contains prior persona/pen work, so the current failure is recorded as 27.
  Central catalogs are owned by 00; no test exception or dummy translation was added.
- Full-suite ignored tests are separate from successful gates: 33 pre-existing ignores
  plus 32's opt-in release benchmark. These include the user-supplied Camera Raw PSD
  corpus, manual Photoshop checks, 00/V1-dependent Node/persona app gates, and older
  benchmarks/fuzz/contact-sheet jobs. No missing external fixture is counted as a pass.
- Final formatting checks passed for both changed crates and both unregistered module
  trees. Log: `target/selection32/fmt-final.log`.
- GPU identity: NVIDIA GeForce RTX 5090, driver 615.71.09, Vulkan, DiscreteGpu,
  Rgba32Float. All 500 targets selected; before/after GPU document pixels equal exactly;
  CPU/GPU premultiplied max diff ≤1/255.

Owned modules/tests are compiled in `target/selection32/workspace`, a disposable copy of
only pc-engine and pc-ui-egui, with read-only dependency symlinks. It applies the supplied
module/command/canvas/frame fragments, 03's existing Delete frame hook, and the two proposed
persona geometry-validation fragments. This exercises
the real PhotocraftApp without editing shared production files. It proves that copy's
hooks, not production exposure of unregistered tools. Logs are in `target/selection32/`.

All cargo commands use Rust 1.98.1, `--offline`, `CARGO_BUILD_JOBS=3`, and one build at a
time. Focused tests use owned module/name filters. Final full tests are limited to the two
changed crates once; all-target clippy uses `target/clippy` with `-D warnings`.

Full-suite breakdown (passed / failed / ignored): engine library 930/0/13, coverage
582/0/0, integration 47/0/5, vector V1 12/0/1; UI library 1018/1/11, GPU integration
60/0/4, integration 14/0/0, pen 8/0/0, snapping 10/0/0. Both doc-test targets contain
zero tests. The full test command exits 101 because of the catalog failure, rather than
claiming an all-green crate suite.

## Integration still required

1. 00 registers commands/modules and applies the real canvas/frame bridge; 01/46 register
   groupSelection/vectorMagicWand/vectorLasso, Y/Q and the Select menu/context controls.
2. 00 persists Settings and per-document Memory in its existing view state, and installs
   authoritative scope hooks. 37 supplies real isolation; no second isolation store exists.
3. 08 supplies the authoritative appearance reader. Stored vector appearance/legacy paint
   fallback works now. 38 must supply the graphic-style link reader; 15 supplies symbol
   identification/model support. Both missing providers fail explicitly.
4. 00 merges translation and PORTS/attribution rows, including the existing 27 Japanese
   keys responsible for the full-suite failure; central catalogs/attribution remain
   read-only here. Repeat integrated acceptance after these dependencies land.
5. 00/01 must apply both exact geometry-bar performance fragments from the integration
   lane: one document walk for shape/lock validation, and removal of redundant existence
   checks already proven by selected-layer equality. The production persona file is
   unchanged; its 82 ms timing is not claimed as a pass. The fragments preserve all
   controls and repeat staleness/eligibility checks on click.
6. A pre-existing zero-height shape-properties defect was observed after rectangular
   two-of-four Node Delete: four extra `shape.edit` commands run merely when properties
   display the remaining collinear path, breaking one-step Undo. Exact journal evidence:
   `target/selection32/focused-delete-diagnosis.log`; reproduction and ownership request
   are in the integration lane. The final app oracle uses a four-node diamond and proves
   exact remaining nodes, a single Delete entry and Undo with properties still enabled.
   The small widget oracle retains the rectangle. 00/02 must fix/recheck the degenerate
   properties case; this report does not claim that case passed.

## Sources and fixture records

Pinned VectorCraft `d522c1d7be4035bd4f4a84cd6ebfca44f5155092` was verified in the local
reference checkout. The four requested source files were read; hashes, licenses and test
parameters are recorded in the engine/UI selection `SOURCES.md` files. Upstream copyright
headers and existing MIT/Apache-2.0/NOTICE files are retained; there is one proposed PORTS
row for each of the two adapted modules in the integration lane. No brand assets were used.

Behavior follows [Adobe Select by characteristics](https://helpx.adobe.com/illustrator/desktop/manage-objects/select-objects/select-objects-by-characteristics.html),
[Adobe Group Selection](https://helpx.adobe.com/illustrator/desktop/manage-objects/select-objects/select-object-groups-and-nested-object-groups.html),
[Adobe Magic Wand](https://helpx.adobe.com/illustrator/desktop/manage-objects/select-objects/select-objects-magic-wand-tool.html),
and [Affinity selecting by attribute](https://affinity.help/designer2/en-US.lproj/pages/ObjectControl/selectByAttribute.html).
Oklab follows [Ottosson's published definition](https://bottosson.github.io/posts/oklab/).
Local geometry mapping, sibling scope fallback and non-editing session publication were
worked out against the branch's established types where those sources do not define the
Local Image interface.

Fixtures are original synthetic Rust source, GPL-3.0-or-later. No proprietary/external
corpus was supplied or claimed as passed. Tests use independent expected IDs/anchors,
analytic gray ΔE boundaries and rectangle inequalities, not round-trip agreement alone.
GPU gates require RTX 5090/Vulkan/discrete/Rgba32Float and premultiplied max diff ≤1/255;
performance requires the release real-app pointer/GPU frame test, not a debug estimate.
