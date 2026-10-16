# Vector toolbar package 46 — 2026-10-10

Continued the inherited wip/vector branch at 529619af without resetting previous builders'
code. The initial tree had only the untracked task brief; no separate 46 implementation
was present. All edits remain uncommitted. No dependencies, manifests or lock changes.

Package-owned implementation and minimal CPU validation are complete; **production integration and integrated GPU
acceptance remain outstanding with 00/01** under the explicit shared-file ownership rule.
Exact wiring is in [integration/46.md](vector-tasks/integration/46.md). This report does not
call the package fully done based on isolated widgets.

## Delivered

- Availability-filtered 22 compact Vector flyout families in vector_toolbar_layout.rs.
  Existing ToolSet model supplies All Tools, Essentials, Illustration, Layout & Type and
  the unchanged paired Photographer list. Registered tools appear once, unavailable
  providers vanish, and future registered IDs receive tail slots. Existing shared tools
  and Repeat/Envelope remain reachable. No fake placeholder tool/provider was added.
- Illustrator (default) and Affinity settings/keymaps, conflict-free including proxy/menu
  chords. Affinity M cycles available rectangle/rounded/ellipse members. Preset lookup
  changes S from Scale to Shape Builder and Y from object Wand to Transparency. Settings
  serde-default to Illustrator; no competing persistent store was created.
- Persona-only accelerator replacement data for all agreed conflicts. Audit found
  Cmd+2/3/7/8 are already engine channel targeting keys despite being absent from the menu
  catalog. Vector replaces every existing owner of those chords. Pixel table is returned
  byte-identically; frozen menu/tool/command snapshots protect legacy defaults. Captured
  semantic UI adapters for 35/36/37 and lock/hide/smart guides are explicitly handed to 00.
- Compiled toolbar metadata slot (`vector.toolbar.keymap`, journal=false), with its
  aggregate append fragment. Paint edits reuse 08's journaled appearance command.
- Compiled fill/stroke proxy: overlapping current swatches, active paint (X), swap
  (Shift+X), white fill/black stroke defaults (D), none (/), captured Colour hex editor,
  gradient and none buttons, and 35's draw-mode widget. The proxy fits a 46px column; colour, draw modes and flyouts open in app popups. Paint press displays a detached preview and release commits it once. Swaps preserve stroke metadata;
  none hides the paint-kind stack without deleting it. Selection/document/revision changes
  reject captured edits. Public previews are detached and cancellable, release uses one
  08 transaction with validated targets and bounds/cache budgets. Press-and-hold flyout
  host and explicit special-character key parsing are compiled and exercised by kittest.
- 70 original new 24px Glossy Duotone geometries and 140 new SVGs, both styles. Existing
  assets remain byte-identical. 96 mappings include all existing Vector registry IDs,
  new feature IDs and proxy/draw-mode controls; none register new tools independently.
  Recognition TSV has exactly 70 rows, one per new asset ID. Reproducible labelled contact
  sheet renderer uses actual resvg 16/20/40px output in both styles on dark/light backgrounds.
- i18n/46.tsv, fixture/source records and exact attribution fragment. Upstream catalogue
  notice and existing VectorCraft licenses retained; icon paths are original GPL artwork.

## Validation

All cargo commands use Rust 1.98.1, offline, CARGO_BUILD_JOBS=3, the inherited required
`RUSTC_WRAPPER=/home/zdavidson/.local/share/local-image-dev/bin/li-rustc-gate`, and one Cargo
build at a time. Gates:

| Gate | Result |
|---|---|
| cargo check --offline --workspace --all-targets | Passed (41.37s); before the subsequent compact UI refinement, which final UI all-target clippy rechecked |
| Strict clippy --all-targets -D warnings, photocraft-engine / photocraft-ui-egui / xtask | Passed (15.11s after cache warm-up); final UI all-target recheck also passed (19.35s) |
| Package-filtered toolbar46 tests | Passed: engine 4, UI 8, xtask 1 (13 CPU tests); 2 GPU tests ignored |
| New narrow-toolbar interaction test | Passed: 1; compact paint controls, typed colour, draw-mode menu, held flyout, stale release |
| New held-preview test | Passed: 1; preview paint oracle, unchanged document/history while held, one commit on release |
| Total unique passing CPU tests | 15: engine 4, UI 10, xtask 1; 2 additional GPU tests ignored |
| Format on three touched crates | Passed |
| Inherited SVG byte check | 188 existing SVGs preserved byte-for-byte |
| Dependency/manifest/lock diff | Empty |

The first narrow-popup test caught anchor release closing its flyout; the release frame now preserves the flyout and the failing test passes. Only that failing test was rerun.

The first clippy attempt caught one collapsible-if in the new Pixel snapshot test; it was
fixed without weakening assertions. The strict invocation passed; the final UI-only invocation also passed and covers the subsequent compact popup/preview refinement. The existing
optional Chinese/Japanese craft-font warning remains; no font corpus was installed.

An initial workspace check overlapped the last API additions and saw the previous engine
metadata (missing tool_for_key_after in the UI); it failed. The corrected workspace check passed; subsequent UI refinements passed final strict clippy.
The incomplete first attempt is not counted as passing validation.

Each of the original 13 CPU tests ran once; the subsequent narrow-toolbar and held-preview tests are focused additions for the final UI refinement. Full-crate/workspace test suites, GPU execution, release builds and benchmarks are deliberately
not run: the task's final owner SPEED RULES explicitly override those gates. The two new GPU
tests are compiled, ignored pending integration and require real RTX 5090/Vulkan/discrete
plus float formats when invoked. Their ignored status is not a passed hardware gate. The
full-app gate asserts production proxy and loader registration, so an isolated host cannot
pass it. No llvmpipe/adapter skip, missing corpus or performance result is claimed as success.

## Integration dependencies and limits

00/01 must apply the handoff to their tool registry, prefs, built-in sets, production flyout,
keys/menu dispatch and labels, proxy host/lifecycle/display provider, icon loader tables,
xtask CLI, attribution and i18n catalogs. These central edits are explicitly outside 46's
owned lane; only minimal append-only module declarations are changed. The semantic menu
adapter IDs are requests backed by named feature APIs, not successful empty commands.
08's registered appearance command presently uses NoProjection; 11/17 projection hooks
are required before enhanced stroke/brush paint acceptance. The isolated tests use legacy
solid/gradient shape paint and do not claim those provider gates passed.

No desktop windows opened, running app killed, ComfyUI job submitted, owner image files
modified or commits made. Reference-source reads were read-only. No empirical recognition
study was performed; recognition TSV records the agreed metaphors and new artwork intent.

Sources: pinned VectorCraft catalogue d522c1d7be4035bd4f4a84cd6ebfca44f5155092;
[Illustrator shortcuts](https://helpx.adobe.com/illustrator/using/default-keyboard-shortcuts.html),
[Affinity Tools panel](https://affinity.help/designer2/en-US.lproj/pages/Panels/toolsPanel.html),
[Affinity Move](https://affinity.help/designer2/en-US.lproj/pages/Tools/tools_move.html),
repository gap audit §5, icons-standard report, existing renderer/transactions.
Full provenance, parameters and fixture licenses are in
[fill_stroke_proxy/SOURCES.md](../../crates/pc-ui-egui/src/vector_packages/fill_stroke_proxy/SOURCES.md).
