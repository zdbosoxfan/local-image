# Package 38 — appearance Eyedropper, Style Picker and Graphic Styles

Status: owned implementation and integration handoff supplied; **integrated acceptance is
still open** for package 00's native data/format/registration/router/dock hooks. No commit.
The initial branch had the prior vector foundations, appearance/width/brush/path-text work
and only an untracked CODEX-TASK.md; there was no package 38 implementation to replace.
All work continues those native APIs and renderer paths.

## Implemented

- `pc-doc/vector_design/styles.rs`: stable style IDs; sampled appearance/FX/transparency,
  character/paragraph and object settings; separate Pick/Apply masks; link baselines with
  every text run; three original starter paint stacks. No external artwork/dependencies.
- `pc-engine/vector_packages/styles.rs`: exact full-stack replacement, selected-category
  transfer, top-paint colour sampling, append, library create/apply/redefine/rename/duplicate/
  delete/break-link, idempotent starter import and Select Same Graphic Style reader/query.
  Missing and locally modified links do not match; redefine prunes local edits and updates
  the remaining targets atomically. Rename preserves IDs; delete preserves object appearance.
- Whole-set validation, document/revision capture, inherited lock/duplicate/missing-target
  rejection, numeric/text/library budgets, 64 MiB metadata and 512 MiB prepared padded-tile
  cache cap. Compile all caches before rasterizing/publishing; failures publish no history.
  Detached previews are cancellable, non-journaled; one Session edit publishes a release.
- Reuses 08's CompiledAppearance, existing Fill gradient midpoint/opacity/offset/dither
  evaluator, 11/17 StrokeProjection, and 12/pc-text native layout and glyph outlines. Ordinary
  character/paragraph-only changes retain multicolour native text. UTF-8 sampled byte picks
  the clicked run. Raster samples use document-space cached pixels. Painted shape preparation
  uses 13's existing live-corner projection while preserving editable base geometry/metadata.
- `pc-ui-egui/vector_packages/eyedropper.rs`: I click samples/applies to captured selection;
  Alt applies loaded attributes (or first selected object when not loaded); Shift samples
  colour without discarding other items; Alt+Shift appends. Options respect both masks.
  Style Picker loads, applies chosen categories by click/marquee, and Unload clears it.
  No selection can load without a document edit. Captured selection/revision/focus/tool/
  persona/module/modal changes cancel. Press previews, release commits, Escape cancels.
- `styles_panel.rs`: real create/apply/Alt-add/redefine/name/duplicate/delete/break-link/
  starter controls, captured Apply preview, stale release rejection. Both UI modules include
  transient AppBridge/frame/route/context/panel/display APIs for the existing app canvas.
- Minimal compiled module declarations in pc-doc, pc-engine and pc-ui-egui. Shared command
  aggregator/data/format/menu/router/catalog files remain untouched. Pixel Eyedropper,
  layer_style.rs and Photoshop layer-style copy/paste are untouched; Cargo.lock unchanged.

## Verification

Owner's final SPEED RULES override the older full-suite/GPU/release requirements. All builds
use cargo +1.98.1 --offline, CARGO_BUILD_JOBS=3 and the inherited mandatory li-rustc-gate.
No overlapping cargo builds were run by this job. Focused new tests only; no whole-crate or
whole-workspace test suite, GPU acceptance run, release build, benchmark or desktop window.

- Formatting: cargo fmt on the three changed crates.
- Workspace all-targets check: passed (`target/styles38-workspace-check-final.log`). Initial
  check caught three API field/visibility mistakes; final code was checked after correcting
  them (`target/styles38-workspace-check.log` preserves that initial failure).
- New CPU/data/UI tests: **19 unique tests passed**: 1 pc-doc, 12 pc-engine and 6 pc-ui-egui.
  Four acceptance tests are ignored/deferred (3 GPU, 1 native-provider), not passes.
  The first execution exposed three fixture/oracle failures: gradient pixel-centre position,
  geometry-limit rejection preceding the intended cache-limit check, and invisible RGB in
  zero-alpha text pixels. Corrected those independent checks, then ran only outstanding/new
  tests while skipping the eight passing engine tests. A late corner call used the wrong
  module path; corrected it to the existing corners::project_shape API before execution.
  One UI fixture needed a frame to settle the context/canvas layout after actual I activation;
  corrected and reran just that failed click test. Passing tests were not repeatedly executed.
  Logs: target/styles38-new-tests.log (initial results), styles38-engine-corrections.log
  (module-path compile correction), styles38-focused-final-tests.log (4 engine + 5 UI passes,
  4 deferred gates), styles38-ui-correction.log (last UI pass).
- Strict clippy all-targets with -D warnings on photocraft-doc/engine/ui-egui: **passed on
  the final code**, target/styles38-clippy-latest.log. Earlier lint logs are retained.
- Final git diff --check passed. Registration edits are append-only; the formatter's
  unrelated existing symbol-module reorder was restored. No dependency/lock/shared-router
  edits or commits.
- Build warning: missing external craft-fonts (CJK boxes). No CJK rendering result is claimed;
  generated Latin fixtures use pc-text's bundled fallback. No required proprietary/external
  artwork corpus is missing: these are original generated fixtures, not a corpus gate.

GPU tests supplied, not executed:

1. `styles38_gpu_rtx5090_preview_release_redefine_parity`: real compositor preview/release,
   redefine and unchanged local edit; finite premultiplied CPU/GPU max difference <=1/255.
2. `styles38_gpu_ui_rtx5090_offscreen_pointer_preview_release`: real egui pointer capture,
   styled document rendered through the GPU compositor and offscreen egui renderer.
3. `styles38_gpu_full_app_i_alt_shift_preview_release_rtx5090`: full PhotocraftApp eframe
   harness with **no test router injection**, live GPU screenshot interior oracles, press/
   release and journal counts; depends on 00's persona/input/display hooks.

All three assert RTX 5090, Vulkan, DiscreteGpu and Rgba32Float; CPU/fallback adapters fail.
One additional ignored native-field acceptance fixture requires 00's actual provider;
its save/load/link/history assertions cannot be satisfied by the synthetic test XMP adapter.
Ignored/dependency gates do not count as passes. No performance claim is made.

## Remaining integration and limits

Exact native fields, serde defaults/skip rules, DocM/convert mirror, provider implementation,
Session::edit pruning call, command registration, package 32 reader, toolbar/icons/context/
canvas/display/dock hooks and native acceptance call are in
`docs/wip/vector-tasks/integration/38.md`; labels are in `i18n/38.tsv`.
FORMAT_VERSION 2 is already present from 15, and needs no further bump for these defaults.

Until 00 installs these fragments, the live app does not expose this package's tool/panel,
and native documents cannot save the new style definitions/links. This is an ownership/
provider dependency, **not passed integrated acceptance**. The synthetic XMP fixture Hooks
exercise transaction/history behavior only; existing appearance native round-trip tests
exercise appearance fields only. Neither is presented as native Graphic Styles persistence.

Decorated glyph-stack projection follows pc-text's Create Work Path outline contract: faux
bold is omitted, warped curves use its documented approximation. Ordinary one-solid-fill
text uses native rendering and preserves faux bold. Group-owned fill/stroke requires
08's group projection and is rejected explicitly; group/raster opacity/FX/object-only
transfers are supported. Root document targets are editable; symbol-master template edits
need their symbol provider's style-link handling. These are stated limits, not silent paint
approximations or placeholder successes.

The final acceptance and GPU/app release gates must run in integration before this package
is called accepted. Central attributions/i18n regeneration and menu/router wiring belong to
00; no user permission or additional owner action was requested by this job.

## Sources and attribution

Read the architecture, persona spec including Owner decisions, quality plan, task ownership
README and vector gap audit; inspected existing appearance/width/brush/path-text/native/style
and egui_kittest code before implementation. Followed the official Affinity Style Picker
and Illustrator Eyedropper/Graphic Styles docs, and the local VectorCraft clone pinned at
`d522c1d7be4035bd4f4a84cd6ebfca44f5155092`. Source URLs, fixture parameters/licences/tool versions,
independent oracles and test-adapter distinctions are in
`crates/pc-engine/src/vector_packages/styles/SOURCES.md`.

Retained copyright/license module notices and existing licenses/vectorcraft-* files. The
handoff contains one PORTS row per adapted local file for coordinator regeneration. Data,
text adapter, panel and fixtures are original Local Image GPL-3.0-or-later code. No new
registry/crate dependencies, unsafe, C/C++/-sys addition, branded artwork, owner-file edits,
ComfyUI job, GUI window, process killing or commits.
