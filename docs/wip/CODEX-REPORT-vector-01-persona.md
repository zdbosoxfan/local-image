# Package 01 — persona

Base: `2e8c671fb3733c7b8b5c2cf85b136b2f5578183c`. Continued the existing PhotoCraft
toolsets/prefs/toolbar implementation; no replacement store, dependencies, lock changes,
commits or desktop windows. Only the owned implementation, test/fixture and handoff files
are changed. Initial uncommitted state contained only `CODEX-TASK.md`.

Status: owned implementation and standalone verification finished; ready for package 00's
integration lane. Integrated acceptance is **not complete** while V1 and the 00-owned
router/Tool/catalog registrations are absent. The shared translation coverage gate still fails.

## Implemented scope

- Added Vector to the existing Persona enum. Old prefs still default to Pixel. Kept Pixel's
  `activeSet` and custom IDs; added independent Vector set and per-persona tool memories,
  with serde defaults and omission of empty/default values.
- Added five Vector presets: All Tools, Photographer, Essentials, Illustration, Layout & Type.
  Vector Photographer is separate from the exact original 28-tool Pixel list. Names and
  lookups are family-scoped; explicit IDs and copying across the catalog remain supported.
  Invalid tools/copies/names are preflighted before preference mutation; templates are read-only.
- Added the complete 59-entry Vector registry and 43 distinct new IDs (96-tool combined
  registry fragment for 00). Vector Pencil, Fill, Art Text and Frame Text never alias raster
  namesakes. Existing shared tools and Pixel's exact 53-tool order/flyouts remain intact.
- Added the segmented persona switcher, View > Persona items, common-dispatch Cmd+Shift+1/2
  commands with shortcut customization, family-filtered set menus and custom-set editing.
  Switching preserves the actual document Arc, selected layers, Layers rows, dock and history;
  it cancels detached Pen/node/Move/raster stroke/gradient/lasso/crop drag previews and flyouts.
  Pixel's prior Pen mode is restored; Vector enters Shape mode so vector drawing over pixels
  uses the existing new-layer-above-target path.
- Added Add/Subtract/Intersect/Xor/Divide, Pathfinder and Layer > Geometry UI through the
  actual V1 `path.boolean`/`path.pathfinder` signatures. Alt samples compound mode at click.
  Captured doc/revision/active/all-target checks reject stale or mixed/locked inputs before
  commands. Missing V1 disables controls; there is no placeholder boolean implementation.
- Reused the existing rasterize prompt and added tested capture/validation functions for
  delayed confirmations. Supplied a tested same-frame PointerGuard and per-feature pointer,
  context, overlay and cancellation hook signatures. These guard call sites and new Tool
  variants remain explicit 00 integration work, as required by ownership.
- Added the Photographer SVG Place context entry, disabled until package 13's actual command
  and dialog router are registered. No file adapter or unowned registry/catalog is modified.

## Verification

All cargo commands use Rust 1.98.1, `--offline`, `CARGO_BUILD_JOBS=3`, and one cargo build at
a time. Full suites are limited to changed crates and run once at the end. Logs are under
`target/persona-*.log` (untracked build artifacts).

| Check | Result |
|---|---|
| `check -p photocraft-ui-egui` (includes changed engine) | Passed |
| Engine focused `--lib toolsets::` | 13 passed, 0 failed, 0 ignored, 854 filtered |
| UI focused `--lib persona_` | 10 passed, 0 failed, 3 ignored, 996 filtered |
| UI focused toolbar/shortcut regression recheck after menu fix | 27 passed, 0 failed, 3 ignored, 979 filtered |
| Strict real-GPU persona preview/switch/release gate, explicitly `--ignored` | 1 passed, 0 failed, 0 ignored, 1008 filtered |
| V1 Add/Undo and GPU Geometry gates, explicitly `--ignored` | 0 passed, 2 failed, 0 ignored, 1007 filtered; actual V1 registration absent |
| Full engine suite, once | 1,485 passed, 0 failed, 16 ignored; cargo exit 0 |
| Full UI suite, once | 999 passed, 2 failed, 8 ignored; see regressions/catalog dependency below |
| Clippy engine, all targets, `-D warnings`, `target/clippy` | Passed |
| Clippy UI, all targets, `-D warnings`, `target/clippy` | Passed |
| Final `fmt -p` check and `git diff --check` | Passed |

New UI tests use real egui clicks, typing and held pointer/drag/release. They check document
identity and unchanged dock/selection/history, cancel a held Pen gesture 20 times, draw a
shape above pixels, cancel the existing rasterize prompt, and create/rename/restart/delete a
Vector custom set. Pure guard tests separately check same-frame staleness, nonfinite/bounded
pointer input and delayed prompt validation. The original Pixel/Photographer lists are frozen
in a baseline fixture rather than regenerated from the new implementation.

New GPU tests use a real full-app offscreen harness, require RTX 5090/Vulkan/discrete and
Rgba32Float accumulation/Rgba16Float canvas, and fail for absent/software adapters. They
read the live app canvas through its view transform and compare premultiplied pixels at
max diff <=1/255. The preview test also records 24 actual pointer-frame UI/GPU/readback timings;
debug diagnostics are not a release performance claim. No library-only timing is substituted.
The installed hardware query reports NVIDIA GeForce RTX 5090, driver 615.71.09, 32607 MiB.
The executed gate confirmed NVIDIA GeForce RTX 5090 / Vulkan / discrete, NVIDIA 615.71.09,
GPU canvas with no fallback/loss/recovery, and both required float formats. All parity checks
passed. Its 24 debug pointer frames included UI, GPU completion and readback: median 12.592 ms,
p95 19.765 ms, p99 22.407 ms. Release timing acceptance is not claimed.

During development, compilation caught an optional-cache access in the future V1 test.
Focused tests caught an incorrect rename-command name and a canvas point that moved with
chrome. The GPU test first pressed the shape's floating context bar; its final gesture starts
on empty canvas and asserts the held Pen state at every measured frame. These test setups
were corrected before the passing focused runs.

The actual Add/Undo union and GPU Geometry click gates were explicitly run and both failed
at their missing-V1 assertions (0 passed / 2 failed / 0 ignored). The GPU Geometry test first
confirmed RTX 5090 / Vulkan / discrete and the float formats. These gates require actual V1
command registration. They supply an independent 7200-pixel rectangle-union oracle and check
restoration of both original layers/selection with one Undo once integrated.

Engine full-suite breakdown: unit 856 passed / 11 ignored; coverage 582 passed / 0 ignored;
integration 47 passed / 5 ignored; doc-tests 0. The 16 ignores are existing timing/benchmark,
fuzz/child/manual gates and a user-supplied Camera Raw PSD gate. That missing proprietary
fixture is not a success. The shared `li-test-runner` emitted summary arithmetic diagnostics
after passing targets; each Rust summary had zero failures and cargo exited zero. The runner
is outside this worktree and was not changed.

The full UI run found `stamp_tests::keyboard_shortcuts_lists_the_stamps_with_layer`:
new View/Layer items had been appended after the other top-level menus. The owned menu
builder now inserts each addition within its existing top-level section. That regression and
all affected toolbar/shortcut tests passed in the 27-test focused recheck; final all-targets
clippy also passed. The full suite is not repeated per the testing budget.
The other failure, `i18n::tests::every_tl_literal_is_translated`, lists 13 new strings missing
from Japanese (the first checked language). All are in `i18n/01.tsv`; actual catalogs are
explicitly owned by 00. Catalog coverage remains unmet until 00 merges the fragment for every
supported language. The failed library target stopped cargo before later UI targets/doc-tests.
Five of the eight full-UI ignores are existing benchmarks/manual/icon GPU gates; three are the
new gates described above. The build also reports the absent optional Chinese/Japanese craft
font bundle; persona fixtures use English and do not claim coverage of that external corpus.

## Integration still required

See [the exact package 00 fragments](vector-tasks/integration/01.md). 00 must register the
new Tool variants/icons/provider tooltips, global post-shortcut frame sync (also with hidden
chrome), PointerGuard on shared canvas events, captured rasterize confirmation, the persona
menu catalog and i18n entries, and family-aware legacy `edit.toolbar` reset. Feature providers
register their actual hooks; missing tools never fall through to raster handlers. V1 is a
named dependency for Add/Undo and GPU Geometry acceptance. Package 13 supplies SVG Place;
future symbol rasterization belongs to package 15's shared integration.

No external/proprietary corpus is needed for these original persona fixtures. Wider PSD,
SVG, fonts and competitor corpora are not claimed as successful gates by this package.

## Sources and attribution

Followed the existing `toolsets_ui_tests`, toolbar/icon, rasterize prompt, vector creation and
GPU harness APIs; the locked egui/egui_kittest 0.36.2 and wgpu 30.0.1 source; the read-only V1
command signatures; and VectorCraft's tools action contract at
`d522c1d7be4035bd4f4a84cd6ebfca44f5155092`.
[Affinity's official Persona documentation](https://affinity.help/designer2/en-US.lproj/pages/Introduction/about_Personas.html)
informed the segmented switcher. Owner decision 2 keeps panels/workspace unchanged.

No upstream code, brand assets or new icons were copied. New orchestration is original Local
Image GPL-3.0-or-later code; existing copyright/license notices remain. PORTS/attribution
fragment for 00: no new upstream adaptation rows. Fixture provenance, license, versions,
parameters and SHA-256 are in
[SOURCES.md](../../crates/pc-engine/tests/fixtures/persona/SOURCES.md).
The source strings are supplied in [i18n/01.tsv](vector-tasks/i18n/01.tsv).
