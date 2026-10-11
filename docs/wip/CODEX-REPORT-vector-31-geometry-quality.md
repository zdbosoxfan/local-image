# Geometry quality V1-Q continuation — 2026-10-10

Implementation and permitted validation are finished in the working tree. **Final V1-Q
acceptance is incomplete:** no Inkscape outputs were supplied, the shared native tool/
menu/GPU registration belongs to 00/01/10, and the latest task speed rules prohibit
GPU acceptance, release builds, benchmarks and broad test runs in this package.
No changes were committed. No desktop window or external CLI was opened/run.

## Work completed per scope point

- Read the architecture, complete persona spec including Owner decisions, quality plan
  A.1–A.5/B, package index and task instructions. Inspected settled V1 owned sources,
  `quality.rs`, `quality_corpus.rs`, existing pc-testkit vector metrics and 01/10/09 app
  test/hook contracts before making changes. Continued the existing branch at `abef8e8c`.
- Retained the existing 200 random pairs x5 operations, area identities, 8x EDT offset/
  stroke tests, 256-case boundary proptest configuration, upstream tests and native
  corpus. `tests/ratchet/geometry.json` was already `[]`; no entries remained to remove.
  Existing tests and kernel files are unchanged.
- Confirmed V1 already implements snap-round retry, a <=1024-segment flattened fallback,
  and kurbo optimal refitting (`boolean.rs` Arrangement retry and optimal_refit;
  `edit.rs` fit_to_bezpath_opt). Added no duplicate retry wrapper or weaker geometry
  algorithm. Core-kernel requests: none.
- Added original `pc-testkit::vector_geometry` reference comparison and shrinking-ratchet
  helpers. They reuse established metrics; check area, sampled symmetric nearest-point
  Hausdorff, 8x coverage IoU, node budget and crossings; preserve fill rules; handle empty
  regions; reject nonfinite/singular views and raster allocations beyond 2048x2048.
  Missing references cannot be hidden in an algorithm ratchet. New/unknown/duplicate
  failures and stale passing entries fail the gate; blank reasons are invalid.
- Added 60 original CC0 binary input SVGs (20 tricky families at normal/tiny/large-origin
  coordinates) and 45 unary input SVGs (24 signed offsets, 18 cap/join/dash combinations,
  3 simplify tolerances). Manifest records all geometry, coordinates and parameters.
  Corpus is approximately 488 KB, well below the quality plan's 15 MB limit.
- Added an ignored, executable external acceptance gate for **345 real Inkscape SVGs
  and normalized sidecars**. It verifies tool/version identity, exact source/input SVG
  matching, operation and options, compares geometry rather than nodes, and enforces
  >=95% pass rate plus a separate empty shrinking external ratchet. Inkscape Division
  is bottom-only; the gate selects input A's regions from Pathfinder's all-face result.
  Simplify checks achieved deviation from the source tolerance and reference node
  budget, allowing different valid curve fits.
- Added a test-only usvg 0.45.1 sidecar normalizer using pc-testkit's existing resvg dev
  dependency. It invokes no CLI, requires coordinator generation provenance, resolves
  transforms, preserves winding and handles, rejects unsupported oracle content, and
  retains exact input/export SVGs to detect stale references. Its transform/hole/cubic
  regression uses analytical area and explicit independent coordinates.
- Supplied three offscreen full-app GPU test sources in the owned
  `pc-pathops/tests/quality/app_gpu.rs`, with exact 00 registration in `integration/31.md`.
  They click all five Geometry buttons, drag live +/-10 Contour previews, release,
  Escape, change document before release, click the real Outline Stroke menu, verify
  independent areas, compare actual production canvas textures to freshly rendered
  CPU shape caches, and check single history/journal publication plus Undo/selection.
  They require RTX 5090/Vulkan/discrete, Rgba32Float composition and actual Rgba16Float
  canvas readback, with premultiplied max diff<=1/255. Missing adapter/render output
  fails. No software-GPU/performance evidence is claimed.
- Wrote `integration/31.md`, `i18n/31.tsv`, and `tests/tricky/SOURCES.md` with source/licence,
  tool pins, parameter records, external generation schema and remaining hook signatures.
  The only shared registration edit is append-only `pub mod vector_geometry;`, allowed
  by the task's final registration override. New pathops tests use auto-discovered
  `tests/tricky/main.rs`; no existing aggregate test file is edited.

## Validation performed

All Cargo commands used Rust **+1.98.1**, `--offline`, `CARGO_BUILD_JOBS=3`, the inherited
`RUSTC_WRAPPER=/home/zdavidson/.local/share/local-image-dev/bin/li-rustc-gate`, and the
task-prescribed PATH. One Cargo build was active at a time.

| Check | Result |
|---|---|
| `cargo +1.98.1 check --offline --workspace --all-targets` | Passed, once (2m13s); registered CPU additions and workspace targets compile. |
| `CARGO_TARGET_DIR=target/clippy cargo +1.98.1 clippy --offline -p photocraft-testkit -p photocraft-pathops --all-targets -- -D warnings` | Passed. Initial run found two needless clones in new tests; corrected them, then the justified lint rerun passed. |
| `cargo +1.98.1 test --offline -p photocraft-testkit geometry31_` | **4 passed, 0 failed, 1 ignored**; run once. Existing tests: 11 filtered out across its binaries. |
| `cargo +1.98.1 test --offline -p photocraft-pathops --test tricky geometry31_` | **2 passed, 0 failed, 1 ignored**; run once. Existing V1 suites were not executed. |
| `cargo +1.98.1 fmt -p photocraft-testkit -p photocraft-pathops -- --check` | Passed. Supplied unregistered app GPU source was separately formatted with rustfmt +1.98.1. |
| `git diff --check` | Passed. |
| Manifests/lock/core registries | No changes; no new dependencies/packages. |

Logs are under this worktree's `target/geometry31/`. Normal new-test total: **6 passed,
0 failed**. Ignored external helper/gate: **2**, reported separately from passes.
Provided GPU tests: **3 sources**, unregistered/uncompiled/unrun against future shared
app wiring; they are not included in the workspace-check or clippy success claims.

## Remaining acceptance and why

1. **External corpus: incomplete, 0/345 outputs available.** Coordinator owns Inkscape
   1.4.4 installation and reference generation; this task explicitly forbids Codex from
   running the tool. Generate/export SVGs, record actual build version, commands,
   settings/parameters and licences, then normalize and run the ignored external gate.
   Offset LPE settings and Inkscape Simplify's relative-threshold mapping must be verified
   with that pinned tool. No fabricated mapping/golden success is supplied. Native and
   external ratchets remain empty; their emptiness is not external acceptance evidence.
2. **Full app: shared integration pending.** 00 must register the test module and 01/10
   native Contour field/tool/toolbar/input/preview hooks. Current branch lacks the native
   `Tool::Contour` enum variant and final `constructive_state` app route. 00 must add the
   real Layer > Geometry > Outline Stroke menu dispatch to existing `path.outlineStroke`,
   with captured doc/revision/target checks and translated label. The handoff supplies
   exact registration path, signatures and capture/dispatch fragment; no competing
   shared UI edits or rehearsal Pen sentinel were used.
3. **GPU, performance, broad/nightly/manual gates deferred by the final SPEED RULES.**
   Did not run GPU acceptance, release builds/benchmarks, whole-crate suites, repeated
   workspace suites, existing 200x5 coverage/EDT/properties, or 10000-case nightly runs.
   At integration run the supplied tests plus existing 01/10/09 live-preview gates,
   release real-app pointer-frame/hardware measurements and 1000-node <50ms gate,
   PSD/manual QA and full suites. No GPU adapter was queried and no speed is asserted.

No permission question or installation request is outstanding: the remaining work is
the explicitly assigned coordinator integration/generation lane, not an unauthorized
change to core files or a successful skipped acceptance gate.

## Sources/attribution

Followed local quality plan A.1–A.5, architecture sections 4–6, existing T1/V1 metrics,
the pinned VectorCraft API/attribution records at
`d522c1d7be4035bd4f4a84cd6ebfca44f5155092`, and existing 01/10/00 offscreen app tests.
Verified command semantics against the official
[Inkscape action documentation](https://wiki.inkscape.org/wiki/Action) and
[command-line guidance](https://wiki.inkscape.org/wiki/Using_the_Command_Line).
Used the [kurbo 0.13.1 BezPath API](https://docs.rs/kurbo/0.13.1/kurbo/struct.BezPath.html)
and cached pinned kurbo/usvg/tiny-skia source APIs. The shared gate/oracle plumbing is
original work because the established metrics did not include this external corpus
provenance/ratchet boundary; existing algorithms were reused rather than reimplemented.

All new Rust code is original Local Image GPL-3.0-or-later code; synthetic fixture data
is CC0-1.0. No upstream files, proprietary references, brand assets or notices were
copied/replaced. PORTS/attribution fragment for 00: **no new adapted files/rows**; retain
existing notices. Detailed corpus provenance and tool/parameter requirements are in
`crates/pc-pathops/tests/tricky/SOURCES.md`.
