# Vector package 00 — serial integration report

Branch: `wip/vec-00-integration`; coordinator merge destination: `wip/vector`.
Continued the previous builder at
`2e8c671fb3733c7b8b5c2cf85b136b2f5578183c`. Changes remain uncommitted.
The current serial V0/bootstrap integration acceptance passes. Incoming feature
package integration remains dependent on the artifacts listed below.

## Scope completed

**Retained and verified V0.** The previous builder had already ported #1321 and
its independent tests. The architecture's statement that #1321 is missing
describes the earlier baseline. The Paint/gradient block matches PhotoCraft
`d438f842256435dba7aa41ca306656a216b98fbf` exactly, Git blob
`55ccce55128fa00873bd5fdbcbdf5c7c9658ef93`; four upstream analytic/fill-layer
parity functions also match exactly. Sorting, midpoints, opacity stops, offset,
dither, align/canvas frames, endpoint calibration and all five PSD layouts are
retained. The four #865 Direct Selection files and the #747 raster-bounds file
still match their architecture pins. Per-file blobs and test SHA-256 values are published in
[00-provenance.tsv](vector-tasks/integration/00-provenance.tsv) and
[00-gradient-tests.tsv](vector-tasks/integration/00-gradient-tests.tsv).
No renderer, Move, GPU, People, Draft or Refinement algorithm was replaced.
The previous builder's [V0 report](CODEX-REPORT-vector-v0.md) remains applicable.

**Owner-approved dependency bootstrap.** Added exact workspace pins for
linesweeper 0.4.0, kurbo 0.13.1, polycool 0.4.0, krilla 0.8.2,
hayro-interpret 0.7.0 and hayro-syntax 0.7.2. PDF features are exactly those in
the existing selected-feature audit: krilla `raster-images`, interpret
`embed-fonts,embed-cmaps`, syntax `std,images`, with defaults disabled.
The optional `xtask/vector-bootstrap` feature anchors and compile-checks these
dependencies while their consumer packages are not merged. It adds no runtime
app dependency or placeholder feature implementation.

The lock adds exactly the 19 approved PDF version pairs plus linesweeper.
Existing versions, sources and checksums are retained, including yoke-derive
0.8.3. The unified selected closure contains 88 registry packages. Compared with
the prior PDF closure, linesweeper also selects already-locked foldhash, libm and
serde/serde_core/serde_derive version pairs. All 88 archive SHA-256 values match
the lock. No addition has a build script, native links, C dependency or `-sys`
crate. The seven preexisting selected Rust build scripts were inspected; their
configuration/version detection does not add native code. flate2 retains its
Rust backend. Dependency-internal unsafe in hayro-syntax is the architecture's
documented exception. All added workspace Rust code uses safe Rust.

[00-dependencies.tsv](vector-tasks/integration/00-dependencies.tsv) publishes
versions, license expressions, selected features, checksums and build-script
status. [00-audit.py](vector-tasks/integration/00-audit.py) reproducibly enforces
the pins, approved additions, cache checksums and selected-feature restrictions.
An isolated six-root prefetch manifest seeded from the workspace lock completed
`cargo fetch --offline`; its 86 registry pairs are a subset of the unified
allowlist. All archives were already cached. No workspace-wide fetch, download
or out-of-worktree reference edit was needed. The bootstrap compile check passed.

**Registration and attribution.** All 26 existing engine vector package modules
and `vector_api` are aggregated by the current command registry. Added a
regression that rejects duplicate command IDs, checks every registered ID's
lookup identity, and retains the existing path/shape, AI, toolset and file
routes. Exact tool-registry coverage and actual picker/toolbar interaction are
covered for all 53 Tool variants by the existing UI tests. Registered the new
app GPU module in the aggregate GPU test binary.

Regenerated `assets/attributions.json` with the existing xtask discovery.
Port entries increase from 70 to 88, including the three previously recorded
#1321 per-file rows. Runtime crate entries remain 738, with zero unknown
licenses. Existing curated entries, per-file PORTS rows and notices are retained;
discovery needs no code extension. The optional bootstrap is not attributed as
an app runtime dependency; its entire reserved graph is published separately.
Package 00 introduces no product strings. Its comment-only
[i18n/00.tsv](vector-tasks/i18n/00.tsv) records that there are no catalog rows to
merge, and existing catalogs remain intact.

**Actual app-canvas GPU regression.** Added
`vector_00_gradient_app_canvas_preview_release_and_undo` to the offscreen
egui_kittest app GPU suite. It asserts NVIDIA RTX 5090, Vulkan, DiscreteGpu,
Rgba32Float support and the app compositor's GPU route, accepting no adapter skip
or CPU fallback. Real pointer events drive two detached Move previews, release,
undo and focus-loss cancellation. Preview retains the same document Arc and
history; release creates one undo step; cancellation creates none.

The matrix covers U8/U16/F32 × five gradient styles × two alignments, with
fractional geometry, gradient fill/stroke, unsorted stops, opacity, midpoints,
reverse, offset and dither. A visible stroke effect selects the compositor's
live preview route. Existing Move previews intentionally translate cached
pixels until release. The established `layout_perf` oracle mints fresh surface
identities while preserving those preview pixels, preventing reuse of the app's
split-cache entry. Released pixels are additionally checked against freshly
rasterized geometry and paint. Seven phases per combination produce 210
premultiplied RGBA comparisons at max difference ≤1/255. Existing independent
Move-plane tests cover the screen-space preview route separately. No interactive
performance claim is made.

## Validation

Final Cargo builds, tests and lints use Rust 1.98.1, `--offline --locked`,
`CARGO_BUILD_JOBS=3`, and one Cargo build at a time. Tests use Vulkan and three
test threads, except the UI retry described below. Full suites run for the
directly changed crates and embedded attribution crate; only UI was repeated
after its failure. Unchanged format/render/I/O crates receive focused
compatibility gates. Clippy uses `CARGO_TARGET_DIR=target/clippy`, `--all-targets`
and `-D warnings`. Logs and the sequential validation scripts are under
`target/refvec/vector-00/` (gitignored).

| Gate | Passed | Failed | Ignored | Filtered | Result |
| --- | ---: | ---: | ---: | ---: | --- |
| New app GPU regression (focused) | 1 | 0 | 0 | 63 | Pass |
| Command uniqueness/lookup (focused) | 1 | 0 | 0 | 862 | Pass |
| Vector gradient tests | 4 | 0 | 0 | 78 | Pass |
| Compose shape vs independent fill-layer parity | 1 | 0 | 0 | 197 | Pass |
| Existing GPU gradient parity | 1 | 0 | 0 | 45 | Pass |
| Frozen native V1 fixture | 1 | 0 | 0 | 65 | Pass |
| PSD vector/corpus | 5 | 0 | 0 | 216 | Pass |
| xtask, approved bootstrap enabled (full) | 15 | 0 | 0 | 0 | Pass |
| Engine (full) | 1481 | 0 | 16 | 0 | Pass |
| UI (full, serialized retry) | 1065 | 0 | 9 | 0 | Pass |
| Embedded attribution crate (full) | 4 | 0 | 0 | 0 | Pass |

The final full-suite totals are 2,565 passed, zero failed and 25 existing ignored;
focused runs and the initial failed UI run are separate from those totals.

The focused app regression passed all 30 combinations and 210 comparisons in
9.39 s. The existing GPU gradient gate passed 120 ordinary/paged comparisons
over 60 pre/post-translation scenes in 2.99 s. The measured adapter identifies
`NVIDIA GeForce RTX 5090`, Vulkan, DiscreteGpu, driver `615.71.09`, PCI bus
`0000:01:00.0`. These are correctness measurements, not pointer-frame performance
claims. No desktop app window was opened and no owner process was stopped.

Dependency metadata/allowlist audit, offline prefetch, bootstrap compile check,
changed-crate formatting and `git diff --check` pass. All-target clippy with
`-D warnings` passes for xtask (with `vector-bootstrap`), photocraft-engine,
photocraft-ui-egui and li-attributions.
Clippy initially rejected the new registry test module's position before
production helpers (`items_after_test_module`). The unchanged test block was
moved to the end of the file; formatting and its focused rerun pass (1 passed,
862 filtered). Production code and test logic are unchanged by that correction.

Engine totals comprise 852 unit tests, 582 coverage tests and 47 integration
tests. Its 16 existing ignored tests are 12 timing/benchmark tests, two slow
panic-hunt fuzz/child entry points, one user-supplied Camera Raw PSD gate and
one manual Photoshop-output check. Those ignored/external checks are not
counted as passing gates.

The first UI unit run had 990 passes, one failure and five existing ignores:
the unchanged Draft/Refinement mock-service test had no result at its ten-second
deadline. Its isolated retry passed in 8.91 s for the entire test (1 pass,
995 filtered); the cause of the initial timeout was not established. The full UI
retry with `RUST_TEST_THREADS=1` passed all 991 unit tests, 60 GPU tests and 14
integration tests. No Draft or Refinement code, assertion or timeout was
changed. Previously completed suites
were not repeated. Both initial and retry logs are retained.

UI's nine existing ignored tests are seven benchmarks, a tool-icon review-sheet
writer and an explicit hardware tool-icon test. The icon test was not run or
counted as a passing gate; the mandatory app-canvas gate asserts its own real
adapter and float format and passed in both the focused and full runs. Engine
and UI ignored counts therefore total 25; no missing external/manual fixture is
reported as successful.

## Fixtures, references and remaining integrations

[00-SOURCES.md](vector-tasks/integration/00-SOURCES.md) records the exact upstream
commits, licenses, cache/manifest sources, Rust API references and synthetic app
parameters. Established repository solutions were followed: the previous V0
port; pinned PhotoCraft sources; pinned VectorCraft manifests/lock; the approved
PDF audit; `layout_perf` app readback and preview reference; current Move command
semantics; cached egui_kittest/wgpu sources. No new feature algorithm was designed.

The native fixture and synthetic legacy preference tests are Local Image's own
GPL-3.0-or-later inputs. The previous builder's read-only sibling PSD corpus
contains 170 checksum-verified inputs from pinned MIT psd-tools/ag-psd sources;
their license files accompany it. All 116 eligible plain shape layers exceed
IoU 0.97 and 587 vector blocks survive round-trip byte-identically. The three
synthetic vector round-trip tests also pass. The temporary worktree-local corpus
symlink was removed after the focused run. Proprietary owner fixtures are absent
and are not counted as successful gates. Future PDF/pathops and package-specific
external fixtures are also not supplied here.

There are no incoming package integration or i18n fragments in this branch.
The real V1 crate remains on its independent branch; packages 01–31 must still
supply their real module, input/context/paint/menu and command hooks. Their
feature acceptance cannot close before wiring lands. No duplicate pathops
crate, dummy engine command, invented tool or unused UI switch was added.
[integration/00.md](vector-tasks/integration/00.md) publishes the exact renderer
handoff, bootstrap commands and the checklist for those incoming fragments.
Package 08 can take shared renderer ownership after this serial lane's validated
handoff. Workspace milestone suites and acceptance of future merged features
remain with the coordinator under the testing budget.
