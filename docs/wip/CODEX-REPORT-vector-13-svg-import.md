# Package 13 — SVG import report

Branch `wip/vec-13-svg-import`, base `c4e4dd08`. No commits. Shared foundation,
node/pen work and existing settings/document schemas are preserved. Only the
listed owned import modules, fixtures, tests and handoff records are changed.

## Delivered behavior

- `pc-svg::import/import_with` returns the shared detached `ImportedScene`.
  Locked usvg 0.45.1 normalizes CSS, use, primitive/arc geometry, units/DPI,
  transforms, groups and effects. XML text placeholders retain editable pc-text
  runs, draw order, transformed placement and cloned use instances without
  enabling usvg fontdb/text or scanning/fetching fonts. Source CSS is scoped away
  from synthetic paths to handle 0.45's important-declaration precedence.
- Shapes keep native cubic knots, compound fill rules and editable fill/stroke
  mirrors. Native pc-vector coverage prepares caches for SVG gradients, including
  transform, reflect/repeat and focal radial geometry. Stroke polygons are made
  in source space before the final affine. Clips become native vector masks;
  SVG empty clips hide all. Alpha/luminance masks become editable pixel masks.
  Embedded PNG/JPEG/WebP become editable raster layers.
- Source, decompression, XML depth/object count, reference expansion, path
  segments, polygon point expansion, coordinates, text characters before glyph
  layout, decoded image pixels and caches are bounded. Cache
  accounting includes complete tile allocations, preventing thin/wide images
  from evading the byte limit. Tiny dash expansion is rejected before stroking.
  DTD/entities, duplicate IDs and reference cycles are errors. String image
  resolution is disabled; unsupported nested SVG data URLs are not recursively
  normalized. There are no network-dependent tests or implicit linked file reads.
- Open/Place/Paste commands use the common exchange adapter. Place/Paste capture
  document/revision/anchor, check staleness and locked destination parents, prepare
  a complete detached document, and publish one Session edit/journal entry.
  Integer translations move paths, text, clips and all caches together. Preview
  stays detached and cancellation publishes nothing.
- The transient egui adapter supports typed SVG, Open/Preview/Place/Paste, canvas
  drag/release, Escape/focus/pointer-loss cancellation and warning navigation.
  Warning object IDs resolve to the same source labels as editable layers; text
  and raster warnings prefer their editable child over a same-named wrapper.
  Tests use real typing, clicks and pointer events.

## Validation status

All SVG-specific tests pass with no ignored tests or adapter fallback. All 15
expected trees, native geometry/unit/text/image oracles, hostile inputs within
one second, external-image isolation and text/use/CSS checks pass. Twelve original
geometry/paint cases pass independent resvg renders at 8x; both gradients pass
off-edge Oklab mean <0.5/max <2. The fixed 400-case resvg subset scores 379/400
(94.75%). Its unchanged ratchet has six malformed path/transform rejections,
three duplicate-ID rejections, ten cyclic-reference rejections, one invalid
negative viewport and one crispEdges/native antialiasing difference. No failed
case was removed from the denominator.

All 40 supplied Lucide icons import successfully without panic. All five
additional public-domain maps/illustrations are non-panicking: four successful
imports, one bounded cache-byte rejection (`blank-world-compact.svg`). That
rejection is not an import-success pass. This is a scoped sample, rather than
complete resvg text/pattern/filter coverage or a broad Illustrator corpus.
Proprietary owner fixtures are unavailable and count as no passes.

| Gate | Result |
|---|---|
| pc-svg full suite | 13 passed, 0 failed, 0 ignored (3 CSS unit + 10 import integration tests); subsequent focused run passed all 11 import tests, including the added warning test |
| pc-engine full suite | 1,518 passed, 1 scratch-layout failure, 19 ignored; failed preference scan passed its focused recheck after restoring source links |
| pc-ui-egui full suite | 1,083 passed, 2 failed, 11 ignored; plugin fixture failure passed its focused recheck; catalogue coverage remains pending 00 |
| SVG engine/UI behavior tests | 5 engine + 3 real typing/click/drag UI tests passed |
| RTX 5090 whole-frame engine CPU/GPU parity | Passed; final gradient-filled curve max difference 0.00038290024, below 1/255; GPU preview and commit identical |
| RTX 5090 actual-app pointer/live/release, warning/edit/Undo, 1x/2x | Passed, 1 test/2 scales; preview/release and CPU/canvas differences both 0/255 |
| Clippy all-targets, -D warnings | All three changed crates passed |
| cargo fmt --check; git diff --check | Passed |

Full suites were run once with `--no-fail-fast`. The two failed layout gates
passed focused rechecks after fixing ignored scratch symlinks. A final warning-only
change exposes approximated percentage text positions and root rotation, with
one added test; the whole-frame GPU fixture was also strengthened to a gradient
curve. The final `svg_import_` run passed 20 tests (11 importer, 5 engine,
3 UI, 1 actual-app), with 0 failed/ignored. Full unaffected suites were not
repeated. Engine full-run breakdown: lib
878/0/13, coverage 582/0/0, integration 46/1/5, V1 12/0/1
(passed/failed/ignored). UI breakdown: lib 1001/2/7, GPU 59/0/4,
integration 14/0/0, pen 8/0/0, SVG full-app 1/0/0. Doc tests have zero cases.
The existing CJK font test also reports an internal skip because craft-fonts is
absent; it is not SVG evidence. Existing ignored performance, provider and
user-supplied PSD cases are not counted as completed gates.

The remaining `i18n::tests::every_tl_literal_is_translated` failure lists five
SVG labels (`SVG source`, `Open SVG`, `Preview SVG`, `Place SVG`, `Paste SVG`)
and fourteen existing node/pen labels. The supplied `i18n/13.tsv` covers the five
owned labels; shared catalogues belong to 00 and were not altered or exempted.
Therefore the entire UI crate suite is not yet green, and the package is not
claimed fully integrated.

All builds are offline with Rust 1.98.1, CARGO_BUILD_JOBS=3 and one cargo build
at a time. No desktop windows are opened. The GPU gates assert Vulkan, discrete
RTX 5090 and Rgba32Float; a missing/fallback adapter is a failure. Premultiplied
CPU/GPU maximum difference is bounded by 1/255. Hardware: AMD Ryzen 7 9800X3D
(8 cores/16 threads), NVIDIA GeForce RTX 5090, driver 615.71.09, 32,607 MiB,
PCI 0000:01:00.0.

Release real-app measurement passed (1 test, 0 failed/ignored), using thin LTO,
one codegen unit and 120 real PointerMoved/app/GPU frames per scale. The test
uses a 48x48 gradient SVG, a 128x128 document at zoom 1, and an 1100x800-point
offscreen app viewport. Times below are milliseconds on the shared owner PC.
Combined time includes GPU completion and screenshot readback, rather than a
GPU-only timer; these measurements establish no large-document speed claim.

| Scale / measurement | Median | p95 | p99 | Max |
|---|---:|---:|---:|---:|
| 1x app pointer frame | 1.967 | 2.146 | 2.254 | 2.490 |
| 1x pointer + GPU + screenshot readback | 3.862 | 4.646 | 4.792 | 5.409 |
| 2x app pointer frame | 2.201 | 2.501 | 2.613 | 2.690 |
| 2x pointer + GPU + screenshot readback | 9.225 | 11.245 | 13.148 | 13.567 |

Release preview/release and CPU/canvas differences remained 0/255 at both scales,
with warning selection, shape editing, Undo, Escape and tool-change cancellation
also passing. The screenshot oracle excludes only the harness's synthetic cursor
and requires at least 120 unobstructed gradient samples. The final engine oracle
covers the entire float frame, including gradient-filled curve edges. The release
timing preceded the final warning-only text correction; that correction leaves
the measured no-text shape path unchanged.

Validation commands (each uses `PATH="$HOME/.cargo/bin:$PATH"`):

```sh
CARGO_BUILD_JOBS=3 CARGO_TARGET_DIR=target RUST_TEST_THREADS=3 cargo +1.98.1 test --offline --no-fail-fast --manifest-path target/refvec/svg-import-build/Cargo.toml -p photocraft-svg -p photocraft-engine -p photocraft-ui-egui -- --nocapture
CARGO_BUILD_JOBS=3 CARGO_TARGET_DIR=target/clippy cargo +1.98.1 clippy --offline --manifest-path target/refvec/svg-import-build/Cargo.toml -p photocraft-svg -p photocraft-engine -p photocraft-ui-egui --all-targets -- -D warnings
cargo +1.98.1 fmt --manifest-path target/refvec/svg-import-build/Cargo.toml -p photocraft-svg -p photocraft-engine -p photocraft-ui-egui -- --check
CARGO_BUILD_JOBS=3 CARGO_TARGET_DIR=target RUST_TEST_THREADS=3 cargo +1.98.1 test --offline --manifest-path target/refvec/svg-import-build/Cargo.toml -p photocraft-svg -p photocraft-engine -p photocraft-ui-egui svg_import_ -- --nocapture
CARGO_BUILD_JOBS=3 CARGO_TARGET_DIR=target RUST_TEST_THREADS=1 cargo +1.98.1 test --release --offline --manifest-path target/refvec/svg-import-build/Cargo.toml -p photocraft-ui-egui --test svg_import_app svg_import_ -- --nocapture
```

Local evidence: `target/svg-import-final-tests.log`,
`target/svg-import-final-prefs-recheck.log`,
`target/svg-import-final-plugin-recheck.log`,
`target/svg-import-final-focused.log`, `target/svg-import-clippy-final.log` and
`target/svg-import-app-release.log`. The two layout rechecks filter the exact
failing test names. Earlier development failures were corrected before the final
suite. The shared `li-test-runner` printed arithmetic-summary warnings after the
final focused run; every Rust harness returned success and reports the counts above.

The actual branch lacks the pc-svg crate root/manifest and app registrations,
which package 00 owns. Validation therefore uses a scratch workspace under
`target/refvec/svg-import-build` with the exact manifest/root/app hooks supplied
in [integration/13.md](vector-tasks/integration/13.md). SVG and engine modules
are the actual owned files; UI is a copy of the real PhotocraftApp with these
hooks and references the actual owned UI adapter. These results do not claim
that production routing/catalogues are already merged. Re-run integrated gates
after 00 applies the registrations.

## Integration and remaining provider work

[Exact registration, call sites, aggregate test path, PORTS rows and i18n fragment](vector-tasks/integration/13.md)
are supplied for package 00. Actual shared manifests, Cargo.lock, core routing,
aggregates, catalogues and attribution generation were not edited. The locked
registry package/version set is audited against the repository lock.

Package 08 still owns enhanced SVG paint projection after edits and the
stroked-shape compositor split. The read-only native Fill has no arbitrary SVG
paint affine/spread/focal fields; the imported fill-only cache preserves them,
but native mirrors approximate these semantics after edits, and the existing
stroked-shape split can approximate its reconstructed fill even initially.
An aggregated gradient-projection warning makes that limitation visible.
This job does not invent shared schema fields. Patterns, filters and foreign
content have warning/fallback semantics. Image resampling/placement and mask
source vectors are not retained as SVG objects.

Package 12's text-path mapping provider is not present on this base. Text-path,
per-character layout/rotation and percentage text positions are warned
approximations; solid editable text uses
explicitly supplied or bundled fonts and reports unavailable font substitution.
Package 14 supplies export/round-trip acceptance. Those provider gates and real
00 registration remain open; the package must not be marked fully integrated
or release-complete until they land and their enhanced gates pass.

## Sources and attribution

The adaptation follows read-only VectorCraft SVG import/text/CSS sources at
`d522c1d7be4035bd4f4a84cd6ebfca44f5155092`, MIT OR Apache-2.0. Notices are retained,
and one exact PORTS row per adapted file is in the integration note. API/source
records and the 0.48-to-0.45 differences are in
[import/SOURCES.md](../../crates/pc-svg/src/import/SOURCES.md): no `fr()` on
0.45 radial gradients; roxmltree 0.20 APIs; no text/fontdb feature; decoded data
callback and the default filesystem image resolver require bounded preflight and
an explicit resolver override.

Primary specifications: [usvg 0.45.1](https://docs.rs/usvg/0.45.1/usvg/),
[SVG paths](https://www.w3.org/TR/SVG11/paths.html),
[painting](https://www.w3.org/TR/SVG11/painting.html),
[text](https://www.w3.org/TR/SVG11/text.html) and
[masking](https://www.w3.org/TR/SVG11/masking.html).
Native reference-graph guards, budgets, transaction validation and interaction
adapters are own work against the shared contract.

[Fixture source records](../../crates/pc-svg/fixtures/import/SOURCES.md) pin
resvg v0.45.1 (`1b6c2fddbcbeffa8135df4323b02aaae84890907`, MIT/Apache), Lucide
0.468.0 (`f12b0de177fbc2a6795e99be065887e72b237123`, ISC), and unchanged
CC0/public-domain Commons/Openclipart source snapshots. Every external file has
its SHA-256 and license/source record. All 845 pinned files match their recorded
hashes. Fixtures total 2,563,454 bytes (2.44 MiB),
under the 15 MiB corpus budget. Fifteen small original fixtures are CC0-1.0.
