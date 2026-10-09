# Vector persona V1 report

Implemented in the existing `wip/codex-vector-v1` worktree, without resetting the branch or committing. Source reference: VectorCraft `d522c1d7be4035bd4f4a84cd6ebfca44f5155092` (MIT OR Apache-2.0). The existing Local Image document model, PSD coverage rasterizer, compositor, cache, history and command registry remain the integration points.

## Implementation

| V1 requirement | Delivered |
| --- | --- |
| Geometry model and adapter | `pc-pathops`, with `geom/{path,bez,arc,hit,shapes}` and the rounded-corner prerequisite. Document-facing APIs accept and return `pc-doc::Path`, preserving cubic handles and smooth flags. Upstream calculation types are transient and have no document serialization. |
| Constructive geometry | Root `boolean`, `pathfinder`, `planar`, `offset`, `edit`, and `fit` modules, adapted from the pinned source. Curve-preserving Add/Subtract/Intersect/Xor, Divide, Trim/Merge/Crop/Outline/Minus Back, Shape Builder arrangements, open cutting edges and face merging. |
| Stroke expansion and offsets | Native outline geometry uses the existing `pc-vector::stroke_polygons` with its flattening contract, width-relative dashes, caps, joins, miter limits and alignment. The upstream kurbo stroker remains available in the calculation kernel. Offset/contour supports miter, round, bevel and negative erosion. |
| Editing and queries | Simplify, smooth, reverse, join, segment splitting with exact cubic subdivision, curve fitting, flattening, hit testing, nearest-point queries and area. |
| Analytic bounds | `pc-doc::Path::bounds()` uses kurbo cubic extrema, includes singleton anchors and excludes unused endpoint handles. No persisted fields or format-version changes. |
| Engine commands | All ten requested `path.*` commands registered, journaled and undoable in one transaction. Inputs follow document stacking; combining operations inherit the bottom layer's appearance. Invalid/locked/non-shape/duplicate inputs fail atomically. Caches refresh and obsolete live-shape/PSD blocks clear. Divide/split select their outputs with undo/redo history targets. Automation's registry audit classifies `splitAt`'s `subpath` index as document vector data, alongside the existing path commands. |
| Outline appearance | A shape with an existing fill expands into an isolated group containing the original fill below the outlined stroke. Original opacity, masks and effects apply once; stroke paint and opacity move into the outline. Stroke-only shapes retain their original layer ID. |
| Compatibility | Checked-in 1,531-byte `pc-engine/tests/fixtures/vector-v1.pcraft` contains a shape, saved path and vector mask, with optional path fields omitted to verify legacy defaults. PSD export/import of a boolean hole preserves coverage. |
| Attribution | Original MIT/Apache license texts and NOTICE, source headers, 24 PORTS rows, curated upstream attribution and `assets/ATTRIBUTION-vectorcraft.md`. Regenerated `assets/attributions.json` with `cargo xtask attributions`. |
| Dependencies | Added pinned workspace `linesweeper = 0.4.0`, `polycool = 0.4.0`, and direct already-locked `kurbo = 0.13.1`. Only new external lock package is linesweeper; polycool was already locked. No new C or `-sys` packages. New crate forbids unsafe and denies unwrap/expect/panic in shipped code. |

### Adaptation details

The specification's suggested all-`Combine` mapping cannot retain holes in this document model: independent PSD components union their coverage. Normalized results therefore use a leading `Combine` and `Join` contours, preserving winding holes and disjoint outer contours. `finish_compound` bakes ordered component operations and fill rules; inverted output requires a finite clip, supplied by commands from the document bounds. This is covered by geometric area, native coverage and PSD round-trip tests.

Native Outline Stroke follows Local Image's PSD-calibrated stroke tessellation rather than replacing it with VectorCraft's renderer or stroke paint semantics. Tests compare native and expanded coverage at 4× AA for curves and polylines, all cap/join combinations, dashes and aligned strokes. Stroke cleanup uses a precision far below rasterization tolerance so the general Pathfinder sliver filter does not discard small dash/cap contours; alignment clipping retains that precision. Inverted fills are supported for aligned strokes by intersecting/subtracting their finite stroke band, without requiring an infinite filled region.

No V0 port was necessary for V1. Existing `pixel_bounds()` already uses saturating arithmetic. The V1 implementation does not require the other PhotoCraft renderer changes. Two pre-existing IO test loops were changed from `chunks_exact(4)` to equivalent `as_chunks::<4>()` iteration to pass the required Rust 1.98.1 workspace Clippy gate. No IO behavior changed. An existing `lc-raster` min-filter test also needed explicit `usize` loop bounds to resolve ambiguous `saturating_sub` calls on the required toolchain; production raster code is unchanged. No V2 interface or gestures were added; command labels were translated through all 12 existing non-English catalogs.

## Validation

All cargo work uses Rust 1.98.1, offline resolution and `CARGO_BUILD_JOBS=3`, with one cargo build at a time. Test logs are under `target/vector-v1-*.log` (not committed).

| Check | Result |
| --- | --- |
| `pc-pathops` tests | 163 passed, 2 ignored: 58 module unit tests; 14 V1 acceptance tests; 43 upstream operations; 19 geometry properties; 17 concave/curve pathops properties; 9 additional properties; 3 regressions. |
| `pc-vector` tests | 37 passed, 1 ignored, including analytic curve bounds and bounds-limited sparse cache. |
| `pc-doc` tests | 49 passed. |
| `pc-engine` tests (known socket test filtered) | 857 passed, 15 ignored, 1 filtered; includes 11 V1 command/PSD/compatibility tests. |
| Final expanded stroke acceptance rerun | 14 passed, 1 ignored; includes all cap/join combinations, dashes, and solid/dashed aligned curves at 4× AA. |
| Ignored release benchmark | Passed; **9.331894 ms** for the union of two 1,000-segment paths (1,164 output nodes), under the 50 ms budget on this machine. |
| Workspace Clippy | Passed: `CARGO_TARGET_DIR=target/clippy cargo +1.98.1 clippy --offline --workspace --all-targets -- -D warnings`. |
| Workspace formatting and whitespace | `cargo +1.98.1 fmt --all --check` and `git diff --check` passed. |
| Attribution regeneration | Passed; 691 dependency crate entries, VectorCraft project and all port rows included. |
| Lock audit | Exact new packages: workspace `photocraft-pathops` and approved external `linesweeper 0.4.0`; no new C/`-sys` packages. |

WORKSPACE_RESULTS_PENDING

## External validation limits

The initial engine run passed 808 tests and failed the existing `ai_cmds::tests::ai_commands_against_the_mock_server`: its local TCP listener is denied by the sandbox (`PermissionDenied`, OS error 1). This is independent of vector geometry. The subsequent engine run filters only that test; no existing tests were edited to suppress it.

The two required Photoshop corpus tests are feature-gated and require `corpus/psd`, which is absent from this worktree and the available local reference sources. Their existing missing-corpus assertion is preserved. The coordinator must run `corpus_shape_coverage_matches_photoshop` and `corpus_vector_blocks_survive_roundtrip` with the real corpus.

GPU_VALIDATION_PENDING

All implementation changes are left uncommitted in this worktree.
