# Node editing (package 03) report — 2026-10-10

Status: owned implementation and verification finished; **integrated acceptance is not
complete**. The UI catalog-completeness check fails until 00 merges the supplied keys;
00 also owns the live-app hooks and V1/provider registration. The
precise handoff is [integration/03.md](vector-tasks/integration/03.md). Changes remain
uncommitted. This continues the existing PhotoCraft #865 Direct Selection code and
foundation command slot; it does not replace the app, document model or renderer.

## Implemented behavior

| Scope | Result |
|---|---|
| Selection across selected curves | Selected shape layers plus the existing targeted mask/work path are candidates. Node selections carry explicit target IDs; Shift adds/toggles anchors, and marquee unions hits across candidates. The marquee snapshots its candidates. |
| Segment click/bend | Click selects both segment endpoints and shows handles. Curved segments bend through the picked parameter; straight segments move both anchors. Handles remain collinear on smooth knots; Alt breaks them. Modifiers are sampled on motion and release. |
| Double-click insertion | Raw captured canvas input detects a double click and inserts through exact de Casteljau subdivision. No geometry changes on the first click. Closing segments are supported. |
| Delete preserving shape | Facing handles refit along end tangents using a bounded join-parameter search. Inserted cubic knots recover their original cubic within 1e-6. General corners have a best-fit replacement; arbitrary two-cubic geometry cannot always be represented exactly by one cubic. |
| Sharp/Smooth/Smart | Sharp retracts handles. Smooth retains existing lengths where present and aligns them. Smart recomputes neighbor-chord lengths/tangents from a common snapshot. |
| Break/join/close/reverse/align | Close, component-aware reverse and node alignment are implemented. Break and Join call injected adapters to actual V1 exports; missing V1 is an error without an edit. Break handles multiple nodes, duplicating provenance at cut ends. Join transfers corner metadata and removes consumed shape layers atomically, retaining receiver paint. V1 success replays await its registration. |
| Selected-node transform box | Context toggle draws the selected-node box. Corner drag scales; Ctrl/Command rotates, Shift preserves proportions, Alt uses the center. Anchors and handles transform together, with unselected knots unchanged. |
| Corner indices/live shapes | Insert/delete/reverse/break/join remap corner references for both shape paths and vector masks in the same document transaction. Live shape changes require the explicit Convert to curves modal, with cancellation and native undo restoration. Existing single-target automation IDs retain their prior explicit path-conversion behavior. |
| Preview/commit lifecycle | Press captures document ID/revision/tool and target IDs. Preview is a detached document with prepared bounds-clipped caches, never journaled or recorded in history. Release validates staleness and the complete target set and executes one command/Session::edit. Escape/focus/tool/document changes cancel capture. Unchanged pointer frames reuse the preview. |
| Integration APIs | Shape targets use `vector_api::snapshot_shapes` and shape-only commits use `apply_shape_edits`. Mixed work/saved paths, masks and donor-consuming joins use one prepared document/Session::edit because that shared helper only accepts shape replacements. Owned module supplies `frame`, `canvas_input`, `context_ui`, `display_doc`; V1, read-only 07 snapping and shared 06/08 renderer callbacks have exact registration signatures. Existing Tool/state/router/manifests/catalogs remain untouched. |

## Verification

All build/test commands use Rust 1.98.1, `--offline`, and `CARGO_BUILD_JOBS=3`.
Only focused changed-code tests are used during development. The full suites of the
three changed crates are attempted once at the end. All-target clippy was rerun after
correcting its benchmark guard lint; results are below. Logs stay under `target/`.

- Initial `cargo check --offline -p photocraft-ui-egui`: passed.
- Focused vector insertion/deletion/conversion/bounds tests: 4 passed (before the
  added component-hole coverage regression); no failures or ignored cases.
- Focused engine node tests: 8 passed, 1 dependency-ignored V1 replay, no failures.
  Includes 256 seeds × 16 edit operations with finite geometry, valid corner indices,
  smooth collinearity, and byte-identical native `.pcraft` state after undo.
- Added vector-mask metadata/conversion/native undo regression: 1 passed.
- Focused UI Direct Selection tests: 16 passed, 0 failed, 2 dependency-ignored.
  Includes raw pointer double clicks, marquee/Delete, bend/Shift drags, context clicks,
  live conversion modal actions, and Alt changes without pointer movement.
- Full `photocraft-vector`: 86 passed, 0 failed, 1 pre-existing ignored benchmark
  (`perf_6016`); includes all five new node geometry tests.
- Full `photocraft-engine`: 1,489 passed, 0 failed, 18 ignored (unit 860/0/13;
  coverage 582/0/0; integration 47/0/5). Two ignores are the new V1 replays;
  the other 16 are existing benchmarks, adversarial registry fuzz/helper,
  user-supplied Camera Raw PSD and manual Photoshop checks.
- Full `photocraft-ui-egui` attempt: library 999 passed, 1 failed, 7 ignored.
  The sole failure is `i18n::tests::every_tl_literal_is_translated`: 11 new
  Japanese Node keys are absent from the central catalogs. The task explicitly
  reserves catalog edits to 00; all keys are supplied in `i18n/03.tsv`, with no
  scanner bypass or test weakening. Other complete languages still need the
  coordinator's catalog merge. Two ignores are the new node full-app gates;
  five are existing benchmarks, icon review sheets and the explicit GPU-icon gate.
  Cargo stopped before UI integration/doc targets; those were then run once separately:
  GPU integration 59 passed/0 failed/4 pre-existing ignored benchmarks; CPU integration
  14 passed/0 failed/0 ignored; doc tests 0/0/0. Combined UI result: 1,072 passed,
  1 failed, 11 ignored. GPU logs identify RTX 5090/Vulkan/DiscreteGpu; no adapter
  skip message occurred.
- All-target clippy on the three changed crates: **passed**, exit 0, using
  `CARGO_TARGET_DIR=target/clippy` and `--all-targets -- -D warnings`. The ignored
  release benchmark retains its
  runtime debug-profile refusal, with a precisely scoped lint allowance.

Total executed tests: **2,647 passed, 1 failed, 30 ignored**. Four new ignores are
listed below; the other 26 predate this work. Formatting of the three changed crates
and `git diff --check` pass. No package-complete or interactive-speed claim is made.

Final command logs: `target/node-final-test-photocraft-{vector,engine,ui-egui}.log`,
`target/node-final-ui-remaining-integration.log`, `target/node-final-ui-doc-tests.log`,
and `target/node-final-clippy-verified.log`. Full crate suites were attempted once;
the UI targets skipped by Cargo after its catalog failure were run once separately.
No whole-workspace run or full-suite repeat was used. Initial full results and remaining
target results are recorded in `target/node-final-results.json` and
`target/node-final-remaining-results.json`. The external machine-wide `li-test-runner`
printed arithmetic warnings before the UI `it` binary; all 14 cases actually ran and
the command exited 0. No owner runner/configuration was modified or bypassed.

The engine real-GPU test passed on NVIDIA GeForce RTX 5090, driver 615.71.09,
PCI 0000:01:00.0, with explicit Vulkan,
DiscreteGpu and Rgba32Float assertions. It compares detached preview, commit and undo
pixels against the independent CPU compositor, premultiplied max difference <=1/255,
and checks that dropping a preview does not change history, revision or journal.
Measured max differences: preview 2.9802322e-8, commit 2.9802322e-8, undo 0.
No adapter absence or software adapter is a successful skip.

Supplied gates that remain **dependency-ignored, not passed**:

- `node_v1_break_join_close_corner_replay_coverage_and_native_undo`
- `node_v1_join_consumes_shape_layers_remaps_corners_and_native_undo`
- `node_full_app_gpu_drag_preview_release_escape_pixels_rtx5090`
- `node_full_app_release_pointer_frame_benchmark_rtx5090`

The full-app gate uses an offscreen complete eframe app, real pointer events and GPU
readback. It asserts live/release interior pixels, one journal/undo and Escape. It does
not inject the test host's routing hooks. The release benchmark measures the real app
on 6000×4000 with 500 shape layers, recording first/median/p95/p99 pointer+GPU/readback
frames, release cost, uploads and hardware identity. No interactive speed claim is made
before that gate runs.

## Remaining integration requirements

00 must install the documented raw input, frame/shortcut, display-document and context
hooks, register actual V1 nearest/join/split adapters, and remove/run the three correctness
dependency ignores. The performance benchmark stays ignored in ordinary debug suites;
run it explicitly with `--release --ignored`. 07 supplies the read-only snapping query.
06/08 supply the enhanced cache
renderer for metadata-backed corners/appearance; legacy caches use the existing renderer
and appearance edits are explicitly refused without that renderer. These are shared
owned files/providers, so modifying them in this package would violate the task's
no-competing-edits rule. Isolated kittest and engine GPU success is not substituted for
integrated app acceptance.

00 must also merge the supplied i18n keys into the central catalogs and rerun the
translation completeness check before the UI suite can pass.

No external/proprietary node fixture corpus was supplied or counted as a successful
gate. Synthetic fixtures and parameters/licenses are recorded in
[the fixture source record](../../crates/pc-engine/src/vector_packages/node/SOURCES.md).
Build scripts report absent craft-fonts CJK assets; those are not new dependencies or
node fixtures. No font-related failure appeared in the full suites.

## Sources and attribution

Read the architecture, persona spec (including Owner decisions), quality plan and package
index before editing. Algorithm/gesture behavior follows the pinned read-only
[VectorCraft source](https://github.com/storytold/vectorcraft/tree/d522c1d7be4035bd4f4a84cd6ebfca44f5155092),
`tools/direct.rs`, `pathops/edit.rs`, and `pathops/fit.rs`; the checkout hash was verified.
The type adaptation retains the existing Knot/Path model and least-squares/Bernstein math.
Node context actions follow the official
[Affinity Node Tool documentation](https://affinity.help/designer2/en-US.lproj/pages/Tools/tools_node.html).
Existing Direct Selection tests and the Move offscreen harness are retained/reused as
repository patterns. Component-hole coverage and native undo are independent oracles.

New/adapted modules retain the VectorCraft copyright notice. Existing MIT/Apache licenses
and NOTICE are preserved. Per-file PORTS rows are supplied to 00 in the integration
fragment, retaining the original PhotoCraft rows; centralized attribution regeneration
and i18n catalog merges belong to 00. UI strings go through `tl!`; merge-ready keys are
in [i18n/03.tsv](vector-tasks/i18n/03.tsv). No brand assets, dependency additions,
manifest/lock edits, shared registry edits, unsafe code, desktop windows or commits.
