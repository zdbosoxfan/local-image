# Package 24 — bounded Affinity import

Date: 2026-10-10. Base: `b17bcaf8`. Changes remain uncommitted. Existing scene,
engine, appearance and UI foundations were continued in place. Only package-owned
sources/tests, original fixtures, source records and integration/i18n notes change.

## Implemented

- Ported the pinned VectorCraft container, object stream, access, model, geometry,
  shapes, paints, text, raster and preview reader. Preserved CRC, container revisions,
  zlib/zstd/predictor decoding, shared references, source-backed pixels and strict PNG
  preview validation. Added caller allocation/object/value/depth/geometry/pixel budgets,
  finite source/scene checks, bounded graph expansion and cancellation between archive,
  stream, model and cache work. Cycles/limits/cancellation fail before publication.
- Added the lower-level `exchange::ImportOptions -> ImportedScene` adapter. No engine,
  UI, filesystem, network or linked-resource dependency exists in the reader. Curves,
  holes, groups, artboard outlines, masks, supported paints/strokes, native text and
  rasters become ordinary layers. Prepared tiled RGBA32F caches are bounded by document
  and ink bounds. Unicode scalar source ranges become native UTF-8 byte runs; family,
  PostScript name, size, caps and features survive. Missing fonts warn and retain text.
- Source DPI is converted to requested DPI once. Explicit page indexes select spreads;
  defaults select the first, multiple selections retain page clips side-by-side, and
  invalid/duplicate indexes fail. Unknown records retain supported children or recover
  their cached pixels. Unsupported effects/geometry and CMYK profile conversions warn. A paper background
  or mask cannot turn wholly unsupported artwork into successful native import.
- Filled the existing engine command slot with `file.affinity.import` and
  `file.affinity.place`. Placement captures document/revision/anchor, rechecks inherited
  destination locks and every imported target/path/cache, translates a detached scene,
  then publishes through one `Session::edit`. Command execution journals once; UI
  release uses the same prepared scene and edit without reparsing. Undo/redo work.
  Ordinary shapes retain native `shape.applyPaths` editing.
- Added transient egui Open/Preview/Place/Cancel/name/warning controls and the captured
  canvas press/move/release path. Preview does not edit history. Staleness, Escape,
  focus/pointer loss and context changes cancel. Warning selection is restricted to
  the imported document/IDs, including when destination names collide or undo removes
  the import. No persistent settings fields or document schema were added.
- Open can explicitly recover an unsupported native layout as its embedded thumbnail
  at thumbnail dimensions. Place refuses that recovery. Malformed/CRC, budget and
  cancellation errors never recover. `read_open`/`decode_with` accept worker cancellation;
  `open_bytes` avoids expanding binary files into JSON arrays. No Affinity exporter or
  proprietary source writeback exists.

## Ownership and integration

The task reserves manifests/lock, shared registries, normal file/job routers, app fields,
GPU aggregate registration, catalogs and central attributions for package 00.
[integration/24.md](vector-tasks/integration/24.md) supplies exact manifest/API/canvas
fragments, ordinary-file call sites, cancellation accessor, native-save-path completion
policy, GPU test registration and one PORTS row per adapted file.
[i18n/24.tsv](vector-tasks/i18n/24.tsv) supplies all new UI literals. Existing licenses
remain in `licenses/`; central attribution regeneration and translations are 00's work.

Verification uses an isolated `git archive HEAD` checkout at
`target/affinity24/worktree`, with only owned files symlinked to this worktree and the
documented manifest/app/canvas/dialog/test hooks applied to the scratch shared files.
Scratch catalog rows use English lookup fallbacks, not completed translations. Its
lock adds exactly one internal package and no external package/version. It exercises
the actual offscreen `PhotocraftApp` compositor and input path. Ordinary central file
picker/background-job routes still need 00's registration and follow-up acceptance on
`wip/vector`; this report does **not** claim the package is fully integrated there.

## Verification

All builds are offline, Rust 1.98.1 (`48a229cea`, 2026-09-01), with
`CARGO_BUILD_JOBS=3`, one cargo build at a time. No desktop window was opened and no
owner process was stopped. No new external dependency, unsafe or C/-sys package was
introduced. The full suites cover only the three changed crates.

Clippy passed for all three changed crates with `--features photocraft-affinity/synth
--all-targets -- -D warnings`. The only build-script notice is the existing missing
external craft-fonts CJK bundle; no Rust/clippy warning was allowed. Formatting and
`git diff --check` pass.

| Changed crate | Passed | Failed | Ignored | Result |
|---|---:|---:|---:|---|
| `photocraft-affinity`, including synth | 49 | 0 | 1 | Full suite passed; external test passed separately |
| `photocraft-engine` | 1655 | 0 assertions | 19 | Unique completed tests; full parallel unit process crashed, unfinished tests passed serially |
| `photocraft-ui-egui` | 1147 | 2 | 18 | Affinity controls and live GPU gate passed; unowned catalog/snapping gates failed |

Full command (one run, no whole-workspace suite):

```sh
PATH=$HOME/.cargo/bin:$PATH CARGO_BUILD_JOBS=3 CARGO_TARGET_DIR=$PWD/target \
AFFINITY_CORPUS_DIR=$PWD/target/affinity24/corpus \
cargo +1.98.1 test --offline --manifest-path target/affinity24/worktree/Cargo.toml \
  -p photocraft-affinity -p photocraft-engine -p photocraft-ui-egui \
  --features photocraft-affinity/synth --no-fail-fast -- --nocapture
```

Evidence: `target/affinity24/full-tests.log`, `clippy.log`, `focused-reader-final.log`,
`focused-scene.log`, `focused-engine-final.log`, `focused-ui.log`,
`engine-unfinished-results.log` and `snapping-isolated.log`. Zero doctests are registered.
No ignored tests are counted as passes in the table.

The engine unit binary registered 1027 cases: 971 passed and 13 were ignored before
exit 139 (segmentation fault, with test-runner arithmetic diagnostics). All six new
Affinity tests had already passed. I compared `--list` with completed test names and
ran only the **43 unfinished cases** through the existing owner test runner with exact
filters and `--test-threads=1`: all 43 passed. Thus all 1014 nonignored unit cases and
the 641 nonignored integration cases completed, but the parallel suite is not a clean
pass. Concurrent GPU device tests are a plausible cause, consistent with the existing
UI GPU aggregate's process-lock rationale; this is an inference, not a native backtrace
diagnosis. 00 should serialize the engine GPU gates or run its engine suite with
`RUST_TEST_THREADS=1`. No runner/shared-test configuration was edited.

UI failures are outside this package's ownership:

- `i18n::tests::every_tl_literal_is_translated` reports missing Japanese literals from
  existing vector packages (including Open SVG, persona, appearance and node controls).
  None of this package's five literals is missing in the registered scratch catalogs;
  source rows are supplied to 00 for real translations. Shared catalogs were preserved.
- `gpu::snapping_full_app_gpu_guide_preview_release_no_jump_and_override` fails at
  2× with preview/release max difference 255/255, at
  `pc-ui-egui/src/vector_packages/snapping/tests_gpu.rs:119`. Its single exact-filter
  rerun on RTX 5090 reproduced the failure. Snapping sources were not modified and
  Affinity state is inactive in that scenario; 00/package 07 must resolve this gate.

The engine's 19 and UI's 18 ignored cases are existing performance, provider, external
fixture or manual gates. In particular the two V1 node-adapter tests and a user-supplied
Camera Raw PSD test remain ignored; these are not Affinity corpus successes. The one
reader ignore requires the four licensed real files and was actually selected and
passed separately. No licensed real `.afphoto` corpus gate was available.

Focused development gates:

- Native import tests: 7 passed, including the separately selected external-corpus
  test. Scene tests: 8 passed. The external test is explicitly ignored by default and
  fails if selected without `AFFINITY_CORPUS_DIR`; it never silently skips.
- Engine acceptance: 6 passed, including the strict GPU gate.
- UI controls: 3 passed. Actual app GPU gate: 1 passed, running both 1× and 2×.
  Type/click/drag tests cover Open, Place, warning selection, native editing, detached
  preview, release, cancel, stale revision, focus loss and undo.
- Original reader properties use 256 cases each; corrupt/archive/PNG/stream properties
  are independent of scene acceptance. Analytic rectangle area is 1600, hole area is
  1200, ellipse area is checked against π·400, and symmetric gradient samples against
  independently specified stop colors.

GPU identity: NVIDIA GeForce RTX 5090, PCI `0000:01:00.0`, driver `615.71.09`,
Vulkan `DiscreteGpu`, 32607 MiB. Both new GPU gates assert this identity/backend/type
and `Rgba32Float`; adapter absence or llvmpipe cannot pass. Engine compares full native
and fallback frames in premultiplied RGBA with max difference ≤1/255. Observed native
difference: 0; flattened thumbnail: 0.00013405085. The live app's pointer preview/release
and CPU/canvas opaque ink differences were 0/255 at both scales, with at least 120
unobstructed samples (the harness's synthetic mouse cursor is excluded). No performance
or large-document latency claim is made; no release pointer-frame benchmark was run.

## Sources and corpus

Followed the established project APIs and owner decision 5 in
`docs/specs/vector-architecture.md`, `vector-persona-spec.md`, `vector-quality-plan.md`
and the work-package ownership contract. Engine/UI state and GPU harness patterns reuse
the existing package 13/15 implementation, rather than a second app architecture.

Reader source: [VectorCraft affinity at d522c1d7](https://github.com/storytold/vectorcraft/tree/d522c1d7be4035bd4f4a84cd6ebfca44f5155092/crates/affinity),
MIT OR Apache-2.0, copyright 2026 ArtCraft Team and VectorCraft contributors. Source
files and adapted tests retain those notices. Compared read-only against
[PhotoCraft affinity at d1a062e0](https://github.com/storytold/photocraft/tree/d1a062e04476256161f8c9b0aeb618bd832709fd/crates/affinity).
At these pins, stream and paint are identical, and container differences are equivalent
array-copy idioms. PhotoCraft's model omits retained VC all-caps/boolean OpenType features
and imports some artboards by their bounding box; VC retains their actual outlines and
strokes. VC's stricter PNG metadata/animation checks and checked source-backed raster
recovery were retained. Local adaptations add strict caller budgets/cancellation and
the native scene/cache/transaction boundary. Comparison evidence is in
`target/affinity24/reference/comparison.md`.

Three tiny checked-in original GPL-3.0-or-later archives total 1050 bytes; hashes and
record layout are in [fixture sources](../../crates/pc-affinity/tests/fixtures/SOURCES.md)
and [hashes.json](../../crates/pc-affinity/tests/fixtures/hashes.json). They were assembled
independently using Python 3.14.8 `struct` and zlib-ng 1.3.1, level-1 doc.dat zlib and
default PNG IDAT compression. Rust synthetic fixture helpers test accepted records;
neither is an exporter or evidence that Affinity opens synthetic data.

Four real files were downloaded read-only, their pinned SHA-256 checked, and imported:

| File | Bytes | Imported canvas | License |
|---|---:|---|---|
| AFDesignLoad `shape_test.afdesign` | 49163 | 180×180 | MIT |
| AFDesignLoad `raster_test.afdesign` | 24569 | 180×180 | MIT |
| Patchy `tiny-text-runs.af` | 20175 | 360×140 | MIT |
| Patchy `tiny-rgba8.af` | 18159 | 64×48 | MIT |

Exact URLs/pins/hashes are durable in
[EXTERNAL-SOURCES.tsv](../../crates/pc-affinity/tests/fixtures/EXTERNAL-SOURCES.tsv).
Upstream MIT licenses were retrieved into `target/affinity24/corpus`; those external
files are not committed and tests never fetch them. Real shape import warns about
unsupported special shapes projected to bounding ellipses; text warns about missing
fonts, width variants, fallback font and paragraph defaults. The other two import
without warnings. These are successful reads, not independently validated proprietary
rendering parity.

## Remaining boundaries

No licensed real `.afphoto` file, exhaustive proprietary effect/embedded-document corpus
or matching external font corpus was available. Synthetic `.afphoto` cases exercise
the shared reader, unknown-record and preview policies; they do not establish complete
Photo support. Arbitrary affine gradients/conical gradients, full source color profiles,
special shapes, compound boolean semantics and some typography use explicitly warned
projections. Raster affine sampling is nearest-neighbor; no full proprietary resampling
claim is made. Nonlegacy enhanced appearance path edits await package 08 integration.
RGB/gray channels use native RGB assumptions; custom embedded RGB ICC profile
extraction/detection is not claimed. CMYK shape, gradient, text and pixel conversions
warn explicitly about missing source-profile conversion.

00 must apply the reserved hooks and rerun integrated file/worker/save-path acceptance,
resolve the shared test issues above, complete catalog translations and regenerate
attributions. These dependencies are listed for the
coordinator; no ownership rule was bypassed or dependency fixture counted as a pass.
