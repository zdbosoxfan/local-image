# V6 / package 12 path text report

Status: owned implementation and integration handoff checked; shared integration
and external acceptance gates remain incomplete.
Full package acceptance is **not complete** until the coordinator supplies the
external golden and merges the package 00 / SVG / PSD integration described below.
Branch: `wip/vec-12-path-text`; no commits made. Existing compiled slot was retained.

## Implemented

- `pc-text/src/on_path.rs`: existing TextEngine shaping, visual bidi order,
  glyph IDs/styles/vertical orientation retained; arc-length cluster midpoint
  placement with rigid tangent transforms; bracket span, closed seam, flip,
  paragraph alignment, baseline/ascender/center/descender and explicit overflow.
  Trailing spaces count toward advance; caps-expanded duplicate byte ranges keep
  separate cluster identity. Logical caret/hit boxes project through the same map.
- V1 ArcPath/nearest adapter, front-to-back native curve hit testing, inherited
  visibility/locks, finite bounded numeric input, one continuous evaluated curve.
- Bounds-limited caches use pc-text Coverage and outlines, preserving colors,
  faux italic/bold and decorations. Outline controls/segment count, flattening
  work, glyph count, text/run/style count and cache allocation are bounded.
- Detached draft documents retain captured document/revision/targets. Atomic
  guarded bulk `type.path.commit` prepares caches before one Session edit and
  journal entry. Invalid targets, duplicates, stale revisions and locks publish
  nothing. Existing typed text replacement retains surrounding styled runs.
- `type.path.convert` uses these same projected pc-text outlines and native
  curves, with exact quadratic elevation/nonzero counters. Multicolor type
  becomes a color-preserving group. The same target metadata survives except
  path-text/obsolete TySh metadata; undo restores the editable text.
- Native model `.pcraft` round-trip and old PathText defaults exercised.
- Actual egui pointer/key host: click ellipse, type ABC, preview/Enter commit,
  marker release outside, both brackets and baseline drag after flip,
  context flip/alignment, Unicode editing, Escape/focus/tool/staleness cancel,
  undo. Committed IME text and paste are handled; arrows navigate logical UTF-8
  characters. IME preedit and visual RTL arrow navigation are not implemented.
  State remains transient; no settings migration needed.
- Exact full PhotocraftApp canvas/context/shortcut/display hooks and offscreen
  RTX 5090 tests provided for package 00. No desktop windows opened or app killed.
- English localisation TSV, attribution row and adapter/PSD warning contracts
  provided in `vector-tasks/integration/12.md` without competing shared-file edits.

## Verification

All cargo builds use Rust 1.98.1, `--offline`, `CARGO_BUILD_JOBS=3`, and run
serially in this job through the existing machine-wide compiler gate. No new
manifest/lock dependency. Focused tests while developing; final full changed-crate
runs and all-target clippy results are recorded below when complete.

| Gate | Result |
|---|---|
| Focused pc-text on_path | 9 passed, 0 failed, 0 ignored (final source) |
| Focused engine path_text | 10 passed, 0 failed, 0 ignored, including strict GPU parity (final source) |
| Focused actual-egui UI | 5 passed, 0 failed, 1 ignored external golden |
| Full-app offscreen GPU integration | 1 passed; 4 zoom/HiDPI cases, preview/release max difference 0 |
| Full changed-crate suites | Once: 2,772 passed, 1 catalog failure, 35 ignored; breakdown below |
| All-target clippy, warnings denied | Passed for all three changed crates, registered app hooks and GPU test |
| Release real-app pointer-frame measurements | 1 passed; 80 actual app pointer/GPU readback frames; metrics below |
| External Inkscape/resvg OFL textPath golden | **Missing**, explicitly ignored corpus gate, not counted as a pass |

Focused engine evidence: `target/path-text-engine.log`; layout evidence:
`target/path-text-layout.log`; UI evidence: `target/path-text-ui.log`; actual app
evidence: `target/path-text-app.log`. Full suites use the registered versions of
only the three changed crates. The registration tree contains copies of those
crates, shared-module patches from the integration contract, unchanged root
manifests/lock, and the normal dependency source tree. Owned host tests in that
tree import the registered module to exercise the same implementation as the app.

Hardware actually exercised: NVIDIA GeForce RTX 5090, discrete, Vulkan,
PCI `0000:01:00.0`, driver `615.71.09`, required accumulation `Rgba32Float`.
Normal and flipped `text_on_path_layers` CPU/GPU premultiplied maximum difference
was **0**, limit 1/255. Actual PhotocraftApp preview/release frame comparisons
at (zoom, pixels-per-point) = (1,1), (2,2), (4,1), (8,1) also had maximum
difference **0**, with 92–103 dark glyph samples per case; transient caret and
bracket handle pixels are excluded, and document pixel equality is checked
independently before/after commit. Ink is asserted; absent adapter/llvmpipe is a hard failure.

Independent oracles: exact analytic line/circle positions, tangents and advance
arithmetic (including 256 span/flip cases); V1 cubic ellipse vs radius/tangent
oracle (0.5px / 0.5°); ordinary pc-text straight raster; native converted curved
text/counters/colors vs path-text raster. These are not the external SVG golden.

Full-suite breakdown (`target/path-text-full.log`):

| Crate | Passed | Failed | Ignored |
|---|---:|---:|---:|
| photocraft-text | 162 | 0 | 3 |
| photocraft-engine | 1,524 | 0 | 19 |
| photocraft-ui-egui | 1,086 | 1 | 13 |

UI unit tests: 999 passed / 1 failed / 7 ignored; GPU: 60 / 0 / 5;
UI integration: 14 / 0 / 0; path-text: 5 / 0 / 1; existing pen host: 8 / 0 / 0.
Doc tests all ran with zero cases. The full suites ran **once**, before equivalent
lint style cleanup; they were not repeated. Lint evidence is
`target/path-text-clippy.log`. Ignored counts include existing corpus/performance
tests, the missing external path-text golden and the release benchmark (run
separately below). Build scripts warn that LightCraft's separate craft-fonts
bundle is absent; package 12's required installed Noto Japanese fixture passed.

The sole failed test is `i18n::tests::every_tl_literal_is_translated`:
Japanese catalogs lack package 12's **Ascender, Descender, Flip path text,
Path text overflow**, plus existing node/pen labels. Baseline/Center already
exist. Catalogs are owned by 00; all package-12 labels and both command IDs are
provided in `i18n/12.tsv`. No catalog audit is disabled and no fallback is counted
as translation acceptance. The report preserves this failure instead of editing
another package's files or rerunning the entire suite.

## Release measurement

`target/path-text-release.log`: **1 passed, 0 failed, 0 ignored**. Default workspace
release profile (thin LTO, one codegen unit), Rust 1.98.1; no profile overrides.
Actual PhotocraftApp, document 256×256, viewport 1280×900, zoom 4, ppp 1,
text “ABC office”, 80 primary-pointer moves dragging the start bracket. Every
measured frame asserts a detached preview and runs the real GPU paint callbacks
plus synchronous image readback. No desktop window was opened.

| Measurement | Milliseconds |
|---|---:|
| Median | 3.091 |
| p95 (nearest rank) | 3.430 |
| p99 (nearest rank) | 8.047 |
| First move, including drag activation | 8.047 |
| Release, two settling frames and GPU readback | 5.249 |

Hardware: AMD Ryzen 7 9800X3D, 8 cores / 16 threads; NVIDIA GeForce RTX 5090,
Vulkan discrete, PCI 0000:01:00.0, driver 615.71.09, required Rgba32Float.
These are observations for this small document on the shared machine, including
UI work and GPU readback; they do not establish latency for larger documents.
The initial benchmark attempt correctly failed its live-preview precondition:
its first move was four screen pixels, below egui's native drag threshold. The
final input crosses that threshold at eight screen pixels and measures activation
in the first frame. The final benchmark was rerun alone after correcting input.

## Outstanding serial integration and corpus

Package 00 owns core declarations, routers, ordinary type edit/transform/cache
refresh, work-path/legacy conversion routing, test aggregate and catalogs. The
exact UI registration patch is in `vector-tasks/integration/12.md`; the temporary
workspace `target/path-text-registration` applies it only for offscreen validation.
The production app in this source tree does not acquire routing until 00 merges
those changes. Do not interpret isolated host tests as integrated app evidence.

The required externally generated `abc-circle-outlines.svg` does not exist here.
The quality plan explicitly says coordinator golden generation and “Codex never
runs these tools” (Inkscape/Vectorizer tools). Accordingly Inkscape was not run.
A synthetic SVG2 textPath input, OFL Inter hash, generation parameters and an
explicit missing-corpus resvg gate are supplied. It fails if invoked without the
external golden; an ignored count never means accepted SVG interoperability.
SVG 13/14 and PSD warning 23 remain named integration dependencies. File adapters
must use doc/exchange + pc-text and V1, never depend on engine/UI.

The existing shared TextLayout API exposes glyph positions rather than source
cluster membership. This package matches glyphs in original line space before
projection; this retains shaped glyph data and tested marks/ligatures/bidi/CJK.
For unusual font positioning crossing logical cluster/line extents, exact source
membership should be exposed by package 00's shaping bridge. No competing shared
layout/model expansion was made in this package.

## Sources and attribution

Read the architecture, persona (including Owner decisions), quality plan and
package README/task. Established services used: existing pc-text layout/render/
raster/fonts and run replacement, existing type conversion adapter, V1 ArcPath
and nearest, Session history/journal, existing egui_kittest/wgpu harnesses.

Placement adaptation: read-only VectorCraft source at
`~/.local/share/local-image-dev/engine-sources/vectorcraft`, verified HEAD
`d522c1d7be4035bd4f4a84cd6ebfca44f5155092`,
`crates/text/src/layout.rs` on-path subset. Copyright 2026 ArtCraft Team and the
VectorCraft contributors, MIT OR Apache-2.0 notice retained in the new file and
existing `licenses/vectorcraft-*`. One exact PORTS row supplied to 00.
Official [SVG 2 textPath layout rules](https://www.w3.org/TR/SVG2/text.html#TextPathLayoutRules)
were consulted for midpoint-tangent method=align. Geometry-free pc-text Curve
boundary and bounded adapters are original glue around these established services.

Fixtures/license/version/hash records: `pc-text/tests/on_path/SOURCES.md` and
`pc-ui-egui/tests/path_text/SOURCES.md`. Bundled Inter is SIL OFL 1.1. Japanese
Noto CJK is a read-only installed SIL OFL fixture; missing font is a hard failure,
never a passing skip. parley 0.11.1, harfrust 0.12.0, skrifa 0.44.0,
resvg 0.45.1 and egui_kittest 0.36.2 from the unchanged lock.
