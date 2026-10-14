# J21 — shared commands and native Library menus

Implemented D27 on the existing branch, retaining the merged LRC-UX/J20 behavior. Changes are uncommitted.

## Consolidation

- Added `li-commands`: generic `CommandSpec<Session, Error>`, callback aliases and `CommandInfo`.
  Both engine APIs remain thin aliases/reexports. IDs, parameters, enablement callbacks, journals,
  command implementations and Library JSON omission rules retain their existing behavior.
- Moved the editor's native menu tree, roles, portable chord reexport, flat-row conversion,
  separator cleanup, structure identity, events/backend contract and macOS layout into that crate.
  The old editor public paths and `mac_layout` signature remain compatibility adapters.
- Moved the Library's parameterized/serialized node type into the shared model with its old API
  and JSON shape intact. Parameterized native identities retain the full JSON parameters.
- One optional-egui command-row renderer now serves both menu bars and editor command/context rows.
  It inherits each shell's theme/spacing and existing monochrome icon painter, preserves editor
  navigation, scrolling, tints and tooltips, and gives Library checks accessible selected state.
  Separate Library/editor command sets and their ordering adapters intentionally remain (audit §5).
- Library/Develop now supply the native macOS bar using their actual live model. The host switches
  the bar with the active module, initializes it before the first frame, refreshes after UI actions,
  routes clicks to Library history/parameters, and blocks modal/stale/disabled actions. Raw key
  equivalents still return through egui and the existing shortcut dispatch/focus rules.
- Native layout consumes all effective shortcuts, including hidden commands. Expanded Library
  rating/flag/label rows now show active-profile bindings and explicit unbinding. Native Settings
  and Quit respect supplied bindings. A conflicting Hide Others system item is omitted rather
  than taking the application's key; legacy layout API keeps its original policy.
- Full-state hashing skips redundant native backend updates while keeping live-model refreshes
  for asynchronous state. Switching back to the editor invalidates its native cache correctly.
- Saved language is applied at initial native-menu construction. Library's in-window menus return
  automatically when no native backend is installed. App-wide Appearance remains available in
  Library/Develop through the canonical editor theme declarations and the shared host palette.
  The shared application Language menu retains its system-language/Auto choice in every module.
- Native in-place updates now match each item's depth-first position, preserving different labels
  and enabled/check state when the same command appears in multiple menu locations. This also
  removes the per-update command-id hash map allocation.

## Quality and performance evidence

| Evidence | Before/control | After |
| --- | --- | --- |
| Library action rows represented in its native menus | 0 | 244/244 source rows |
| Editor action rows retained by native adapter | 705 source rows | 705/705 retained |
| Shared cache, 10 identical snapshots | 10 submitted snapshots | 1 backend call |
| Real Library View → Filmstrip menu open/click, one headless sample | 51.755029 ms, original renderer | 26.049962 ms, shared renderer |
| Default Library native model, one sample | feature absent | 245 native items in 973.451 µs |

The interactive sample drives actual `Headless` app frames and registered pointer clicks, verifies
Filmstrip toggles, and uses the same new test for both versions. Only `nodes_ui` was temporarily
replaced with the original HEAD implementation for the baseline; the shared version was restored
before final verification. Reference snippets and logs stay under `target/refvec/j21/`. These are
small CPU UI samples under shared machine load, not a general speedup or GPU-performance claim.
The deterministic cache test also verifies a changed check and an auto-toggled native item both
force updates. Native in-place updates no longer allocate a command-id lookup map.

Before → after structural counts: command-spec implementations **2 → 1**; command-row rendering
implementations **2 → 1**; Library native command coverage **0 → the complete Library model**.
No pixel algorithm, GPU rendering, persistent document/catalog format or stored setting changes.

## Validation

All required checks passed offline with toolchain **1.98.1**, `CARGO_BUILD_JOBS=4`, and the same
retained worktree target directory:

- `cargo +1.98.1 check --offline` on all six changed crates: **passed**.
- `cargo +1.98.1 clippy --offline --all-targets -- -D warnings` on all six: **passed**.
- `cargo +1.98.1 test --offline -p li-commands -p lightcraft-ui-egui -p photocraft-ui-egui -p local-image j21_ -- --nocapture`:
  **20 passed**, **0 failed** (13 shared, 3 Library, 3 editor, 1 host). Other integration targets
  selected by Cargo ran zero tests because of the filter.
- After replacing the host's prohibited `expect()` with a guard, the new host test was rerun
  specifically: **1 passed**, **0 failed**. The first Clippy diagnostic is preserved in
  `target/j21-clippy-expect-failure.log`.
- Formatting and diff whitespace checks passed for our edits. The pre-existing task file's final
  blank-line warning was left untouched.
- Lockfile package-set comparison: exactly **1 added workspace package**, `li-commands`; **0 added
  external packages**, **0 removed packages**.

Final logs: `target/j21-final-{check,clippy,tests,host-test}.log`. Check/Clippy emit the existing
build-script notice about this environment's missing embedded craft-fonts; no Rust/Clippy warning
was suppressed. Only new filtered tests were run, as required by the final minimum-testing override.
No full crate/workspace suite, GPU acceptance run or benchmark was run.

The first row-parity test exposed duplicate presentations of `dialog.export`: the test was corrected
to check each label/presentation, and the underlying native updater was fixed to preserve those
presentations during in-place updates. The failing log is retained at `target/j21-focused-tests.log`;
the corrected UI test run passed (3 Library, 3 editor, 1 host). No existing assertion was weakened.

## References and implementation choices

- Existing sources: `docs/wip/AUDIT-duplicate-systems-fable.md` §§1, 3 D27, 4 J21, 5;
  `docs/DESIGN-SYSTEM.md`; the existing native menu implementation and J20 shortcut catalog.
  Eleven moved core tree/layout helper bodies preserve the original implementation apart from
  their new shared location; translation and effective-binding policy use explicit adapters.
- [Adobe Lightroom Classic keyboard shortcuts](https://helpx.adobe.com/lightroom-classic/desktop/introduction-to-lightroom-classic/keyboard-shortcuts.html):
  retained rating/flag/label actions and familiar default keys; customized profiles remain authoritative.
- [Apple menu guidance](https://developer.apple.com/design/human-interface-guidelines/menus):
  app-wide command availability, enabled state and menu conventions. The detailed Apple menu-bar
  page requires JavaScript; role placement follows the already-tested repository implementation.
- egui 0.36's existing `Button`, `MenuConfig`, `Response::widget_info` and menu navigation APIs,
  as used by the repository's shared widgets, remain the UI primitives.

## Scope, compatibility and limitations

Unavoidable supporting edits outside the task's listed source files: workspace/crate manifests and
`Cargo.lock` to wire the new workspace crate; `pc-ui-egui/src/widgets.rs` to retain shared-widget
compatibility adapters; `apps/local-image/src/{library_host,main}.rs` to integrate active-module native
menus; `docs/DESIGN-SYSTEM.md` to document the reusable renderer; the J21 i18n pending list and this report.
The pre-existing `docs/wip/CODEX-TASK.md` edit was preserved.

No new registry dependencies or C/`-sys` packages were introduced. The new workspace crate uses existing
serde, serde_json, li-keys and optional egui dependencies; tests reuse egui_kittest. No unsafe code.
No Match Look files, owner photos, owner settings, running GUI or external services were touched.

User-visible changes: native Library menus replace the dormant editor bar while Library/Develop is
active; Library menu checks now expose accessible state without leading spaces in accessible labels;
expanded shortcut labels follow profiles; conflicting system Hide Others is hidden. New fallback
strings are listed in `docs/wip/i18n-pending-j21-li-commands.md`.

GPU tests added: **0** (menu-only work). AppKit cannot run on this Linux host; a new macOS-only
Library accelerator-mapping test is supplied, and platform-independent native models/event routing
are covered by the focused tests. No GPU or macOS runtime performance claim is made.

TEST-CHANGED: none. Existing assertions and tests were retained; only new regression/parity tests added.
