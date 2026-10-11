# Vector V1 completion report — 2026-10-10

Continued the paused builder at `bb96258c`; no reset or replacement of its implementation. Merged `wip/vector` at `a7cf38c78f6eb4259d5b85eeb4f757809116cb13` without committing. All merge conflicts are resolved. The merge and V1 changes remain uncommitted, as required by the task's final rule.

Today's attribution tooling/data, blur, develop filter, command registrations and i18n catalogs were preserved. Both `pathops_cmds` and `vectorize_cmds` remain registered. Only missing V1 history labels were restored in five catalogs. The prior builder's unrelated raster/IO test edits were replaced with the integration branch's versions. The automation audit also needed a vector-data exception for the newly merged `layer.vectorize.preview`: its description mentions returned vector paths, and it does not read host files. This changes only the test's parameter classifier, not production authorization.

## Delivered scope

| Requirement | Result |
| --- | --- |
| All-Rust geometry engine | Retained and completed `pc-pathops`: adapted `geom/{path,bez,arc,hit,shapes,corners}` and `boolean`, `pathfinder`, `planar`, `offset`, `edit`, `fit`, with document-facing adapters. Upstream calculation types are transient; persisted geometry remains `pc-doc::Path`/`Knot`. |
| Constructive operations | Add/Subtract/Intersect/Xor, Divide, Trim/Merge/Crop/Outline/Minus Back, arrangement regions, Shape Builder edges and face merging. Open fills close with a straight edge. Ordered PSD component operations and fill rules are resolved before constructive operations; normalized holes use `Join` in their owning component. Inversion requires a finite clip, supplied by engine commands. |
| Robustness | Fallible adapter APIs distinguish failed sweeps from legitimately empty results. Failed boolean sweeps retry on snap-rounded curves, then flattened polygons with kurbo refitting. Polygon retry is bounded at 1,024 edges; oversized inputs return `Degenerate`. Scale-aware sweep precision, translated area integrals and separate sliver/refit tolerances retain thin regions near coordinates of 1e6, including zero/tiny offsets and thin kernel strokes. Exact coincident/reversed boundaries use set identities to avoid sweep debris. Shape Builder and face-merger failures now propagate; excessive open cuts return an error instead of disappearing. |
| Offsets and strokes | Kurbo/VectorCraft offsets support round/miter/bevel joins, miter limits and negative erosion, with union cleanup. Native Outline Stroke deliberately uses `pc-vector::stroke_polygons`, retaining established PSD coverage, width-relative dashes, caps, joins and alignment. The kurbo curve stroker remains available through the calculation kernel. This is an adaptation from the quality plan's preferred curve stroker, chosen to meet V1's native stroke equivalence requirement. |
| Editing and queries | Simplify uses VectorCraft corner detection and kurbo optimal fitting of smooth runs; it keeps original subpaths if refitting would add nodes. Half of the requested fitting tolerance is reserved for kurbo's approximate error estimate. Smooth, reverse, join, exact cubic segment splitting, sampled curve fitting, flattening, containment, nearest queries and area are available. |
| Analytic bounds / architecture | Read the parallel `wip/codex-vector-arch` architecture document and aligned with its document-path API and layering contract. `pc-pathops::bounds(&Path)` uses kurbo extrema; importing `PathBounds` provides `path.bounds()`. Includes singleton anchors and excludes unused endpoint handles. Removed the paused builder's kurbo dependency and bounds implementation from pc-doc; pc-vector has no production dependency back on pc-pathops. No document schema or format-version change. Parallel architecture foundations remain in their own worktree for the coordinator's merge lane. |
| Commands | All ten requested `path.*` commands are registered, journaled and undoable. Preflight validates geometry/options, duplicates, locks and shape inputs before mutation. Input order follows document stacking; constructive output inherits the bottom object's appearance. Divide/split select all results. Live-shape/stale PSD blocks clear and native caches refresh. Automation's registry audit recognizes `splitAt`'s `subpath` as a vector-data index. |
| Expanded appearance | Stroke-only expansion retains the layer ID and transfers stroke paint/opacity to a fill. A shape with an existing fill becomes an isolated group with fill and outline children, applying original opacity, masks and effects once. |
| Compatibility | Checked-in 1,531-byte legacy `.pcraft` fixture covers shapes, saved paths and vector masks with optional fields omitted. Engine tests verify legacy defaults, editable result geometry, PSD hole coverage and undo/redo. No V2 UI or gestures were introduced. |
| Ports and dependencies | VectorCraft pinned to `d522c1d7be4035bd4f4a84cd6ebfca44f5155092`, MIT OR Apache-2.0. Source notices, 24 PORTS rows, attribution document, curated project entry and generated attribution entries are present. Both license texts and NOTICE are byte-identical to the pinned local upstream checkout. Only new external lock package is approved `linesweeper 0.4.0`; `polycool 0.4.0` and `kurbo 0.13.1` were already locked. No new C/C++ or `-sys` package, unsafe code, dssim-core or fuzzing dependency. |

## Geometry quality harness

Extended the integration branch's shared, dev-only `pc-testkit::vector` metrics with native coverage, exact cubic Green area for resolved simple nested contours, exact Felzenszwalb–Huttenlocher EDT and signed offset thresholds. Area intentionally requires resolved, nonintersecting contours rather than sharing topology code with linesweeper. Self-intersection checking now retains actual cubic parameter intervals during adaptive subdivision and refines crossing parameters against the original curves. Existing MS-SSIM, Oklab, fidelity, Hausdorff, finite/node and raster metrics are retained. Resvg 0.45.1 is a dev-dependency with default features disabled; it introduces no new lock packages.

The original CC0 corpus covers 16 named input pairs × four booleans (64 checks), plus tiny/huge scales and sliver regressions. It includes coincident/reversed edges, shared edges, tangent curves, bow self-intersections under both fill rules, winding holes, a touching hole, a 50-spike star, zero-area spikes, containment, empty input and open fills. Every native corpus check passes and `tests/ratchet/geometry.json` is `[]`; new, stale, unknown and duplicate entries fail the test. These are native coverage checks, not Inkscape goldens. Inputs, seeds, licenses and missing external references are documented in `tests/SOURCES.md`.

All normal properties use 256 cases and honor `PROPTEST_CASES` for nightly runs. The final full suite found a simplify deviation of 0.051583 px for a 0.05 px request. Tightening the fitting budget fixes it; the seed is retained in `pc-pathops/proptest-regressions/vector-v1.txt`. Simplify properties now check both directions at the requested tolerance. The deterministic quality case also checks decreasing node counts across increasing tolerances.

## Validation

Rust 1.98.1, `--offline`, `CARGO_BUILD_JOBS=3`, one cargo build at a time, on the owner's PC without a sandbox. Full suites were limited to the V1-changed crates and pc-doc rollback verification. Pathops was rerun after the simplify failure/fix and final thin-offset correction; automation was rerun after its audit failure/fix. Other full suites ran once. Additional checks were limited to affected tests: the final V1 command binary passes all 13 tests including the explicit performance gate, and the geometry GPU test passes again after the cleanup correction. No workspace suite was run in this completion session. Logs are under `target/vector-v1-*`.

| Crate (default full suite) | Passed | Ignored |
| --- | ---: | ---: |
| `photocraft-pathops` | 175 | 2 |
| `photocraft-testkit` | 11 | 0 |
| `photocraft-vector` | 79 | 1 |
| `photocraft-engine` | 1485 | 17 |
| `photocraft-gpu` | 90 | 0 |
| `photocraft-automation` | 89 | 0 |
| `photocraft-ui-egui` | 1064 | 9 |
| `photocraft-doc` | 150 | 0 |
| **Total** | **3143** | **29** |

Final Clippy passes for all eight listed crates with `--all-targets -- -D warnings`. Scoped `fmt --check`, V1 whitespace checking (`git diff --check MERGE_HEAD`) and the lock audit pass. The full staged merge inherits four whitespace warnings in another job's develop-layer report, the original Potrace COPYING text and vkdt NOTICE; those integration files were preserved. Attribution regeneration and its seven source-consistency tests pass. Generated attributions contain 739 runtime crates, the VectorCraft project and all 24 pinned VectorCraft port entries. The lock audit compares against the exact merged integration commit, not the older paused-builder HEAD.

| Quality gate | Observed result |
| --- | --- |
| 200 random pairs × five operations, 4× AA | Maximum native coverage error **0.0003991127**, below 2/255; cubic area identities at relative 1e-6 and finite/self-intersection checks pass. Divide face sums and reconstructed coverage pass. |
| Round offsets at 8× EDT | IoU: −10 = 1, −4 = 1, +4 = 0.9999111, +10 = 0.9998676; all exceed 0.999. Maximum independently sampled source-boundary distance error **0.00271994 px**, below 0.05 px. Area increases with delta. |
| Round-cap native outline vs EDT | IoU **0.9999317**. Separate native stroke equivalence tests cover caps, joins, dashes and aligned curved strokes at 4× AA. |
| Independent SVG rendering | Original cubic ellipse SVG rendered by resvg versus native coverage at 8×: IoU > 0.999 and area error < 0.1 px². Crossing-parameter and exhaustive small-grid EDT checks pass. |
| GPU parity | Full GPU suite passes on **NVIDIA GeForce RTX 5090, DiscreteGpu, Vulkan, driver 615.71.09**. New `constructive_vector_geometry_matches_cpu` covers 12 boolean/divide/offset/outline/edit results, maximum premultiplied channel difference ≤ 1/255. Run with `REQUIRE_VECTOR_GPU=1`; unavailable/software/wrong adapters fail. No skipped adapter results in the GPU log. |
| Interactive engine path | **30.049389 ms** (final verification; earlier isolated run **10.355036 ms**) for `Session::execute("path.boolean")` on two 1,000-node paths, including result-cache rendering, journal and history, on the owner machine; below 50 ms. This measures the same engine command path used by the app, on a 192×192 document. GUI presentation/frame latency is reserved for manual QA. |

## Coordinator-owned gates still pending

The implementation and executable checks above are complete; the entire release DoD is not signed off:

- **Inkscape 1.4.4 goldens:** none were supplied. Quality-plan A.1/A.5 assigns corpus/reference generation to the coordinator and explicitly says Codex never runs these tools. Therefore the ≥95% Inkscape corpus gate, <0.1% area / <0.05 px Hausdorff comparisons and ≤1.5× reference node budget are unverified. The passing native corpus is not a substitute for that gate.
- **Nightly:** coordinator runs `PROPTEST_CASES=10000`, as specified. This job ran the normal 256-case budgets and retained the regression seed.
- **Manual app QA:** coordinator must exercise text-outline union/subtract/divide, star offsets ±10, dashed stroke expansion and PSD reopen, then sign the merge note. No GUI windows were opened or owner app stopped.
- **Photoshop reference corpus:** `corpus/psd` is absent. Existing feature-gated `corpus_shape_coverage_matches_photoshop` and `corpus_vector_blocks_survive_roundtrip` remain available for the coordinator; their missing-corpus checks were not suppressed.

## Sources followed

- [VectorCraft at the required pin](https://github.com/storytold/vectorcraft/tree/d522c1d7be4035bd4f4a84cd6ebfca44f5155092): adapted geometry/pathops algorithms and operation/property tests. Read from the pinned local engine-sources checkout; file-level provenance is in `docs/PORTS.md`.
- [linesweeper 0.4.0 documentation](https://docs.rs/linesweeper/0.4.0/linesweeper/): curve sweep, topology and fallible arrangements. The retry limits, scale/sliver policy and exact-boundary fast path are Local Image adaptations proven by this harness.
- [kurbo optimal fitting](https://docs.rs/kurbo/0.13.1/kurbo/fn.fit_to_bezpath_opt.html), [SimplifyBezPath](https://docs.rs/kurbo/0.13.1/kurbo/simplify/struct.SimplifyBezPath.html) and [stroke](https://docs.rs/kurbo/0.13.1/kurbo/fn.stroke.html): smooth-run fitting, endpoint-derivative limitation, offset/stroke primitives and experimental error estimation; cached official crate sources were also read.
- [SVG 1.1 fill rules](https://www.w3.org/TR/SVG11/painting.html#FillRuleProperty): nonzero/evenodd semantics and straight closing of open fill subpaths.
- [Felzenszwalb–Huttenlocher distance-transform paper](https://cs.brown.edu/people/pfelzens/papers/dt-final.pdf): independent exact pixel-center EDT, implemented from the paper and cross-checked exhaustively against brute force. Raster boundary uncertainty is documented separately from sampled analytic boundary error.
- [Affinity joining objects](https://s3-eu-west-1.amazonaws.com/affinity-docs/help/designer/English.lproj/pages/ObjectControl/join.html) and [stroke panel](https://s3-eu-west-1.amazonaws.com/affinity-docs/help/designer/English.lproj/pages/Panels/strokePanel.html): operation names, stacking/compound behavior, stroke alignment and width-relative dash semantics. The task's bottom-appearance rule governs our constructive results.
- Repository `vector-persona-spec.md` §8/§11 and owner decisions, `vector-quality-plan.md` A.1–A.4, parallel architecture API/layering contract, and Local Image's existing PSD-calibrated renderer/history/serialization tests.
