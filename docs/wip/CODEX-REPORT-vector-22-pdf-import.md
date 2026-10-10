# Package 22 — PDF import

Date: 2026-10-10. Base `af01845666e88f2ef12bbed25450c33adf8e8f7f`.
Changes are uncommitted and restricted to package 22's owned paths. Existing
foundation/WIP is retained. This base had the compiled empty PDF import engine
slot and approved dependencies, but no pc-pdf import implementation or UI.

## Implemented

- hayro-syntax **0.7.2** / hayro-interpret **0.7.0** Device adapter returns the
  shared `ImportedScene`. Native bottom-to-top paths, cubic/quadratic handles,
  compound holes/fill rules, isotropic strokes/dashes, clips, isolated
  transparency, all PDF blend modes, opacity and page clipping remain editable.
  pc-vector is the only path rasterizer; no hayro/vello renderer or unsafe code.
- Embedded-font and bundled-standard glyphs become editable curves; Type 3
  artwork becomes an editable group. Text reconstruction is explicitly warned.
  Unembedded nonstandard fonts use bundled substitution, with a warning. No font
  scanning, external resource resolution or network access.
- Raster image layers preserve draw order and orientation; placement is baked
  and warned. Alpha/luminosity soft masks use native float pixel masks, PDF
  background/transfer rules and a warning about discarded source editability.
  Axial/radial shadings retain an editable gradient mirror plus exact initial
  sampled cache; geometry/function simplification on later edits is warned.
  Tiling/mesh/function paints have bounded pixel fallbacks with warnings.
  Nonuniform strokes become editable outlines with a warning.
- Zero-based page selection, CropBox/MediaBox, normal/suppressed rotation,
  UserUnit, DPI, ordered multiple pages and editable per-page viewports. Open
  uses chosen DPI; Place uses destination DPI.
- Preflight bounds input/decompressed bytes, object numbers/counts/expanded
  resources, graph/content/save depth, sample/image/xref declarations, expanded
  work, paths/dashes, tiled cache memory and paint sampling. Bounded zlib/JPEG
  admission precedes hayro's unrestricted decoders. Recursion through inherited
  Form resources is checked. Decoder panics become typed errors; cancellation
  is polled. No partial document is published on failures.
- Encrypted PDFs return an explicit decrypted-copy-required error; no password
  API or password data enters history/journals. Unsupported decoder features
  have explicit bounded refusals rather than silent appearance changes.
- Captured document ID/revision, active anchor and complete target IDs govern
  detached preview. Missing/stale/changed targets, ancestor locks, coordinates
  and depth are validated. Prepared placement publishes one Session::edit;
  command invocation journals once. Release reuses decoded scene/caches. Undo
  and Redo work through the existing engine history.
- Page chooser UI has typed page/DPI, Crop/Media/rotation options,
  Open/Preview/Place/Cancel and warning selection actions. Pointer previews are
  detached; Escape, focus/pointer loss, chooser close, target/document/anchor,
  module/tool changes cancel. PDF byte/path entry points precede raster routing.
  Fields are transient, so old documents/preferences need no schema migration.

## Shared integration contract

The exact manifest/root/App/file/canvas/test registration and PORTS rows are in
[integration/22.md](vector-tasks/integration/22.md). English source rows and the scoped pending-translation registration are in
[i18n/22.tsv](vector-tasks/i18n/22.tsv) and the integration note. The owner has
paused translation work; these rows are not claimed as non-English translations.
Shared real files were not modified:
package 00 must carry those fragments into `wip/vector`. The imported engine slot
already participates in the foundation's command registry.

Integrated acceptance is exercised in `target/pdf22-registration/worktree`, a
disposable archive of this base with those registrations and symlinked owned
sources. It uses the actual PhotocraftApp/canvas/GpuCanvas/engine, not a replacement
renderer or a standalone widget presented as the app. Package 21's unmerged
exporter was read-only snapshotted for its public API round trip; source hashes
are in [fixture SOURCES.md](../../crates/pc-pdf/fixtures/import/SOURCES.md).
No export source changes are part of this package.

## Verification

All cargo commands use Rust 1.98.1, `--offline`, `CARGO_BUILD_JOBS=3`, one cargo
build at a time. The disposable manifest is passed explicitly. Logs live under
`target/pdf22-registration/`. Focused tests are used during development; final
full suites are only the three changed crates, with all-target clippy
with `-D warnings` and `CARGO_TARGET_DIR=target/clippy`.

| Changed crate / target | Passed | Ignored | Result |
|---|---:|---:|---|
| pc-pdf: import fixtures; lib/doc targets contain no tests | 18 | 0 | Pass |
| pc-engine: lib | 982 | 13 | Pass |
| pc-engine: coverage | 582 | 0 | Pass |
| pc-engine: it | 47 | 5 | Pass |
| pc-engine: vector_v1 | 12 | 1 | Pass |
| pc-engine total; doc target contains no tests | **1,623** | **19** | Pass |
| pc-ui-egui: lib, including repaired i18n check | 1,030 | 12 | Pass under documented translation-pause registration |
| pc-ui-egui: appearance | 11 | 0 | Pass |
| pc-ui-egui: gpu | 61 | 5 | Pass |
| pc-ui-egui: it | 14 | 0 | Pass |
| pc-ui-egui: path_text | 5 | 1 | Pass |
| pc-ui-egui: pdf_import_app_gpu | 1 | 0 | Pass |
| pc-ui-egui: pen | 8 | 0 | Pass |
| pc-ui-egui: snapping | 9 | 0 | **1 pre-existing failure**, reproduced with PDF hooks removed |
| pc-ui-egui total | **1,139** | **18** | **1 pre-existing failure**; doc target has 0 tests (pass) |

The full UI library run initially had 1,029 passed / 1 failed / 12 ignored. Its
sole failure was `i18n::tests::every_tl_literal_is_translated`: 65 keys were from
already-present vector modules and 21 from PDF import. All 86 have supplied
English rows in the package handoffs. Registered only those supplied handoff
rows as pending under the existing owner translation-pause policy in the
disposable harness; the exact failing check then passed (1 passed, 1,041
filtered). This does not claim those strings have translations. The remaining
UI binaries are run separately, so the full passing library is not repeated.

The complete UI run also exposes the existing
`gpu::snapping_full_app_gpu_guide_preview_release_no_jump_and_override` failure
at **PPP=2**, `tests_gpu.rs:119`, with preview/release max difference **255/255**.
The isolated exact-name rerun reproduces it. For independent isolation, temporarily
removed only the PDF UI-frame, `display_doc` and canvas-input calls from the
disposable shared files; the same exact test still fails with the same mismatch.
Restored those calls immediately afterward. This is a real baseline failure,
not an ignored/passing gate and not a PDF adapter failure; logs are
`ui-snapping-recheck.log` and `ui-snapping-baseline.log`. Fixing Move/scaled-view
or snapping shared routing belongs to 00/07/09, outside package 22's owned paths.
The task's all-crates-pass condition is therefore **not satisfied by the shared
UI baseline**, despite all package 22 acceptance gates passing. It is explicitly
handed off for the coordinator milestone rather than hidden or weakened.

All **25 package 22 tests** pass: 18 parser/fixture tests, 4 engine tests (including
strict GPU), 2 UI interaction tests and 1 strict full-app GPU test. The largest
GPU differences remain below 1/255. No own test is ignored or skips its adapter.

Final all-target clippy is clean with `-D warnings` for all three changed
crates, including UI registration after the translation-policy wiring. Format
and `git diff --check` are clean. Full logs: `pdf-full.log`, `engine-full.log`,
`ui-full.log`, `ui-i18n-registration.log`, `ui-remaining-targets.log`,
`ui-doc.log`, the baseline isolation logs above, and the three `*-clippy.log` files in the validation directory.

Commands, each invoked separately with the environment/manifest above:

```sh
cargo +1.98.1 test --offline -p photocraft-pdf
cargo +1.98.1 test --offline -p photocraft-engine
cargo +1.98.1 test --offline -p photocraft-ui-egui
# Following the single UI library failure, rerun only that repaired check:
cargo +1.98.1 test --offline -p photocraft-ui-egui --lib every_tl_literal_is_translated
# Complete the integration targets the stopped full run had not reached:
cargo +1.98.1 test --offline -p photocraft-ui-egui --test appearance --test gpu --test it --test path_text --test pen --test snapping --test pdf_import_app_gpu -- --test-threads=1 --nocapture
cargo +1.98.1 test --offline -p photocraft-ui-egui --doc
CARGO_TARGET_DIR=target/clippy cargo +1.98.1 clippy --offline -p <changed-crate> --all-targets -- -D warnings
```

Ignored tests are pre-existing timing/benchmark, manual/proprietary fixture,
slow fuzz, and other packages' absent V1 or app-hook milestone gates. All tests
added by 22 run without ignores or adapter-skips. No whole-workspace suite is
run. The build's pre-existing missing `craft-fonts` warning is recorded; CJK
external font coverage is not claimed. The owner's external test runner emitted
a nonfatal arithmetic warning after the focused app test (cargo exit 0); that
runner is outside the worktree and was not changed.

The own fixture suite uses independent geometric/colour oracles: analytic cubic
area, exact rectangle/clip/opacity/stroke bounds, radial distance, triangle
barycentrics, luminosity versus alpha and four-corner image orientation. Tests
also edit imported paths/glyphs, rebuild caches, export 21→import→render at both
compression settings and exercise embedded font curves. Hostile cases include
bounded zlib expansion, sparse xrefs, resource cycles, deep saves, tiny dashes,
oversized images/cache budgets, sample/function/predictor expansion, and 256
randomized byte arrays both raw and wrapped in valid PDF streams. A shared mask source is tested with both
alpha and luminosity modes; the cache includes mode/background and bypasses
unidentifiable transfer functions.

UI kittest clicks/types page selection and Open/Place/warning actions, drags
preview/release and cancels. The mandatory GPU tests assert RTX 5090, Vulkan,
discrete hardware and Rgba32Float accumulation, with premultiplied max difference
≤1/255. The app uses its existing Rgba16Float premultiplied display tiles after
32-bit accumulation; the test reads those actual tiles and checks their format.
Hardware: NVIDIA GeForce RTX 5090, discrete Vulkan, PCI `0000:01:00.0`,
NVIDIA driver `615.71.09`. Engine imported-scene GPU fixtures have maximum
premultiplied difference **0.00022017956**; the focused actual app test has
maximum **0.000007688999**, both below **1/255**. Its observed pointer update
plus GPU presentation wall times were **7.69–22.30 ms** across focused/final
runs in the test profile;
these include two harness frames and are not release performance claims.
The full-app test goes through actual file-byte routing and canvas presentation,
press/motion/release, normal Move selection/edit, Undo and focus-loss cancellation.
It never creates a desktop window or kills a running app.

No release performance claim is made. Offscreen test pointer update/presentation
wall times are diagnostics that include harness overhead, not a release latency
benchmark. Proprietary/external PDF corpora are **missing**, not passed; own
fixtures and generated public-export round trips are reported separately.

## Limits and remaining integration

This importer deliberately refuses encryption, indirect stream lengths/filters,
indirect decoder declarations, LZW/JPX/JBIG2 and other codecs without bounded decoders, inline images, calculator
functions, external streams, nonuniform dashed hairlines, knockout/nonisolated transparency groups, and
cyclic resources. Sampled functions are limited to four dimensions; packed
mesh/function expansion and predictor row allocation have explicit budgets.
It does not claim complete PDF standards coverage. Conservative
global resolution of inherited XObject names can reject unusual unrelated
same-name resources. Image sampling is nearest center. Annotations/private editor
data and font/image decode omissions are warned. Gradients and soft-mask source
editability have the documented simplifications above.

The existing full-app snapping HiDPI failure described above remains for the
shared integration lane. Package 22 implementation and integrated PDF acceptance
are complete, but the overall shared UI suite is not reported as green.

UI staging is synchronous and bounded. A future shared jobs/progress route may
use the public cancellable parser; this package does not change that coordinator
owned facility. 00 still owns real shared registration/catalog/attribution merge,
file-picker/drop route convergence and final milestone suites. Exact exercised
registration is supplied, rather than editing those shared files competitively.

## Sources

Read the architecture, persona spec including Owner decisions, quality plan and
task README. Followed VectorCraft at
`d522c1d7be4035bd4f4a84cd6ebfca44f5155092`: PDF Device/page/mask/shading mapping
and geom `from_bezpath`; native layer/cache implementation follows existing
pc-vector/pc-compose and SVG placement boundaries. Existing MIT/Apache license
texts and VectorCraft notice remain; per-adapted-file PORTS fragments are supplied
for 00. No branded material is copied.

Official API/source references: [hayro Device 0.7.0](https://docs.rs/hayro-interpret/0.7.0/hayro_interpret/trait.Device.html),
the cached exact-version hayro-syntax/interpreter sources, and
[Adobe PDF 1.7 reference](https://opensource.adobe.com/dc-acrobat-sdk-docs/pdfstandards/pdfreference1.7old.pdf)
(objects/pages, graphics, fonts and transparency). Original bounded admission and
sampling adaptations are identified in source records; fixture generation,
versions/parameters/licenses are in fixture SOURCES.md. No new dependencies.
