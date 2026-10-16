# Package 36 — Object › Path and Expand

Implementation is left uncommitted on the existing branch, based on `571f0cd4`. Earlier V1
and vector-owner work was retained. Package-36 modules were absent in that baseline, so this
adds the owned implementation rather than replacing another owner's work. The task brief is
unchanged. Read the architecture, persona spec/Owner decisions, quality plan, package README
and gap audit before implementation.

The owned implementation and test gates are supplied; **integrated acceptance is
pending 00's production hooks and the coordinator's RTX run**. Those shared files are outside
this package's ownership, and the final speed override explicitly prohibits GPU acceptance
execution here. This report does not call isolated widget tests full-app acceptance.

## Delivered behavior

- Join uses V1: nearest open endpoints, coincident merge, straight bridge, and closing a single
  open path. Combining layers requires sibling/paint/transparency compatibility and preserves
  the first layer ID. Average supports horizontal/vertical/both with a global selected-anchor
  centroid and translated handles. Direct-selection indices address base knots before live
  corner projection. Selected corner references are remapped when removing nodes.
- Simplify uses V1's public precision/corner-aware kernel; the dialog shows original/new counts,
  precision and corner angle, with detached live preview. Precision 0–100 maps to tolerance
  5–0.01 document pixels. Add Anchor Points uses de Casteljau midpoints; Remove Anchor Points
  uses V1's refit-preserving removal. Compound fill rule/inversion/subpath operations survive.
- Clean Up removes seeded stray points, unpainted shapes and whitespace-only text. Split Into
  Grid creates exact cells with rows/columns/gutters and optional guides, retaining source paint
  and outer masks/transparency. Divide Objects Below splits visible ordinary closed shape
  leaves below the cutter in document paint order, retains paint and consumes the cutter.
- The Path fragment routes legacy Outline Stroke to V1, explaining the need for Expand
  Appearance for live/advanced paint. Offset Path delegates preview/commit to package 10's
  captured Contour gesture rather than introducing another offset kernel.
- One Expand family supplies Object/Fill/Stroke options and Expand Appearance. Four-paint
  stacks become four ordinary paths in paint order, with per-paint blend/opacity/visibility and
  outer masks/effects/transparency retained once. An unchecked ordinary stroke stays a stroke;
  ordinary native fills keep their native paint. No gradient/mesh approximation is invented.
  Live shapes (02), corners (06), width outlines (11), brushes (17), symbols (15), repeats (19)
  and path text (12) use public owner APIs. Path text's public conversion command runs in an
  isolated scratch Session whose history/journal is discarded, before one real publication.
- Missing blend/envelope evaluators return explicit 18/20 reasons; 42/45 variants/providers
  are absent in the baseline. Expand menu entries disable with the actual preparation reason.
  Unsupported clip projection (35), advanced symbol/repeat rendering (08/09), arrow/dash/paint
  frame baking (08) likewise explain the owner dependency and preserve source data on failure.
- Vector Copy and Paste in Front/Back use the existing native codec without modifying the
  raster/OS clipboard. Groups, masks, effects, live metadata and resources survive. Layer IDs
  are duplicated; symbol resources transfer through 15's public graph-transfer API, avoiding
  destination collisions. Paste preserves coordinates and inserts above the top or below the
  bottom selected sibling; without a selection it uses the root's ends and accepts an empty
  document. Different canvas/color/DPI/global-light/text-style contexts and
  conflicting patterns return a reason until a complete common projection rebake is available.

All gestures capture document ID/revision and target IDs. Entire target sets are checked for
locks/duplicates/ancestor conflicts, numeric and path budgets are enforced, and geometry/caches
are prepared on a clone. Publication executes one registered command/Session::edit. Preview is
non-journaled and cancellable by Cancel/close/Escape, lost focus, stale captures or owner changes.
Options have serde defaults; no shared persisted schema was modified. Prepared cache allocations
are bounded by document bounds and a 512 MiB aggregate budget. Clipboard payloads cap at 64 MiB.

## Integration and ownership

Minimal additive engine module/spec and UI module registration follows the task's final
registration override. No manifests, lockfile, document types, existing clipboard commands,
shared production menus/router, i18n catalogs or PORTS catalog were edited.

[Integration handoff](vector-tasks/integration/36.md) contains exact command JSON schemas,
compiled frame/menu/copy/paste/display hooks, canvas Context plumbing, provider boundaries,
Vector shortcuts/icon aliases, and all three GPU gate names. It also specifies that the actual
GPU/CPU canvas and Navigator consume the detached display document. The bridge lives in egui
context data and blocks underlying shortcuts while its dialog is open. It yields to foreign
modal/text/transform/node gestures; Direct Selection captures 03's selected base knots across
all additional curves rather than relying on the independent layer-panel selection.

[i18n fragment](vector-tasks/i18n/36.tsv) supplies labels and static error/reason keys.
[Engine sources/fixtures](../../crates/pc-engine/src/vector_packages/path_commands/SOURCES.md)
and [UI source record](../../crates/pc-ui-egui/src/vector_packages/path_commands/SOURCES.md)
record attribution, original synthetic fixtures, licenses, parameters and locked tool versions.
Four pinned VectorCraft adaptation rows are supplied to 00 for shared PORTS regeneration.
Existing MIT/Apache/NOTICE files are retained; no brand assets or new dependencies were added.

## Validation

Across first executions: 21 new CPU cases passed, 3 engine cases failed and were repaired but
not rerun, and 3 GPU cases were ignored. There are no claimed GPU/performance/full-app passes.
The repaired three engine cases remain execution gates for the coordinator, together with the
shared production integration and owner-provider dependencies described above.

Commands used Rust/Cargo 1.98.1, `--offline`, `CARGO_BUILD_JOBS=3`, and the supplied
`RUSTC_WRAPPER=/home/zdavidson/.local/share/local-image-dev/bin/li-rustc-gate`. One cargo build
ran at a time. Initial compile errors in the new code were repaired; no unrelated owner code
was changed to make validation pass.

- Format: `cargo +1.98.1 fmt -p photocraft-engine -p photocraft-ui-egui`.
- Workspace: `cargo +1.98.1 check --offline --workspace --all-targets` — passed after repairs.
- Strict clippy: `CARGO_TARGET_DIR=target/clippy cargo +1.98.1 clippy --offline
  -p photocraft-engine -p photocraft-ui-egui --all-targets -- -D warnings` — passed on
  the final sources, including the repaired engine cases and production hook selection/focus
  guards. Only the pre-existing optional CJK-font build-script warning remains.
- Engine: `cargo +1.98.1 test --offline -p photocraft-engine object36_` — 15 passed,
  3 failed, 1 ignored, 1,099 filtered out. Run once, as the final speed rule requires.
  The three findings were repaired: Join now tests active effects rather than comparing
  `Layer::new`'s enabled-empty effects with `Effects::default`; four-paint preview/commit
  compares geometry and pixels rather than separately allocated child IDs; path-text uses
  owner 12's existing filled-glyph IoU >0.97 conversion contract. Text and vector edge
  rasterizers differ, so exact AA pixel parity is not claimed for path text. The strict
  four-paint/live-shape/repeat/symbol pixel gates remain unchanged. Repairs are compiled by
  the final lint, but these tests were not re-executed under the once-only budget.
- UI: `cargo +1.98.1 test --offline -p photocraft-ui-egui object36_` — 5 passed,
  0 failed, 2 ignored, 1,075 filtered out in the library target. Other targets selected
  zero tests (123 filtered out); no old test was executed. Run once.
- Added after review: `cargo +1.98.1 test --offline -p photocraft-engine --lib
  object36_vector_paste_without_selection_uses_root_ends_and_empty_document` — 1 passed,
  0 failed, 0 ignored, 1,118 filtered out.
  This is the first execution of the new no-selection/empty-document/stale-capture case,
  not a re-run of the previously executed engine cases.

Engine tests supply independent Hausdorff/area/exact-coordinate and CPU premultiplied pixel
oracles, native round trips, atomic failures, stale/locked captures and undo checks. UI tests
really click menus/options, drag the Simplify slider, type grid inputs, cancel, and send
Join/Copy/Front/Back shortcuts. The full-app tests use actual PhotocraftApp menus, pointer
frames, GPU canvas texel readback, release/cancel and undo; they are compiled and ignored until
00 hooks land and integration executes them.

Three GPU gates were added (one compositor, two full-app), requiring RTX 5090/Vulkan/discrete,
Rgba32Float accumulation and required float canvas support; software/missing adapters and CPU
fallback fail. Pixel checks use premultiplied max diff <=1/255. **GPU execution, full-crate or
workspace test suites, release builds and benchmarks were skipped under the final speed rule.**
No performance claim is made. Proprietary/third-party visual corpus gates were not run and are
not counted as passing; fixtures here are original synthetic geometry and bundled Inter text.
The existing build warning about missing optional CJK craft-fonts is recorded; ASCII Inter
fixtures do not depend on that external corpus.
