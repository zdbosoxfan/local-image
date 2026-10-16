# Remaining Linux UX audit at ddd5411e

Read-only source/report review by state_color_triage; no new tests or native UI execution. Original specifications remain authoritative, especially ~/.local/share/local-image-dev/specs/bugfix/ux.md and reviews/opus-final-review.md / astra-final-review.md. The earlier consolidated milestone fixed SF05–07 and notices/Generate, not the entire U/UX inventory. No Linux requirement is waived. Full translation prose, macOS/Windows native integration and large-library performance are separately deferred; visible Linux controls remain required.

## Accepted milestones since this source audit

The source-confirmed table below is the original baseline, not a claim that every listed defect remains unchanged. Preserve its whole scope and native acceptance requirements.

- O059: committed `6acff845`; logical-point Pen/node hit targets, fractional/2x regressions and explicit RTX node gate passed. Broader native workflow sweep remains.
- U013/U014: committed `3b4604a1`; unified document Export As, successful-setting persistence, theme control parity and focused geometry/accessibility regressions passed. Whole native shell/layout sweep remains.
- U024/U025/U033/U035: committed `ffa8f3b3`; Library terminology/startup/import/grouping/navigation/status changes and focused regressions passed. Large Library scaling remains an explicitly deferred follow-up.
- U011/U016/U034: committed `0857b9b7`; visible personas, stable admission-time document identity, Move default and shortcut conventions passed focused/affected checks and explicit persona GPU gates. Tab and actual Rotate View remain required.
- U026/U027/UX05: committed `e0f57cc0`; full-size export default, native-source final export geometry, shared themes and photo-overlay contrast passed focused/affected checks and rendered inspection. Remaining navigation/input/sorting/download clauses and unified title/menu/settings/shortcut surfaces remain required.

- U026 navigation/input clauses: committed `bd5fac02`; held Original, visual-row arrow navigation, auxiliary panel/module return, captured typed-value edits with undo, Smart Sort destination handling and Denoise download state passed 13 focused and 176 affected checks. Narrow footer and light-theme toast screenshots inspected. Strict LC lint passed; desktop-union lint remains pending the separate Editor correction. Final native workflow sweep remains. U027 shared title/menu/preferences/shortcut editor is actively being implemented in library-shared-shell.

## Implemented; final native acceptance still required

- U001/UX01: hotfix options-row clipping, hotfix_options_bar_controls_are_inside_the_visible_row. Theme parity separately open.
- U002: inline vector text owns tool/paint shortcut letters, both personas/presets.
- U003/U004/U005/UX04: Develop classic keys, command chords, Sync Settings and pick-and-advance implemented in recovered Develop work.
- U006: captured-target delete/Trash/Remove/Cancel; native Trash remains.
- U007: import preferences/folder picker/drop review; native picker/drop remains.
- U008: Library startup and Develop restore implemented; empty Library Detail default is separate U025.
- U009: no-op/error feedback and cancellable culling implemented; native check, large-library performance separate.
- U010: live shortcut lookup and preset editor implemented; Linux labels sweep remains, native mac menu deferred.
- U012: Pen/path-text completion, focus retention and local AI-mask undo implemented.
- U015: projection/resource preservation/XC03 and native/PSD persistence implemented; actual Library-edit-save-return remains.
- UX02: Corners context editing in both personas implemented.
- UX03: Library export/dialog reserved action height tested; native viewport sweep remains.
- UX06: Liquify action wrapping implemented; native bounds/hits remain.
- UX08: transform lifecycle no longer depends only on current tool; six-dialog multi-frame/Escape check remains.

## Source-confirmed remaining work

| ID | Current issue / source | Required acceptance |
|---|---|---|
| O059 | shared_tools pen supplies zoom*ppp; direct_select pick uses 6/(zoom*ppp) | Same logical hit target at 1x/fractional/2x, plus outside-radius negative |
| U011 | persona.rs icon-only switch, global persona, Vector key fallthrough to Pixel | Labelled Pixel/Vector, intentional photo-open behavior, no silent key-driven persona switch |
| U013 | export_dialog.rs raster-only/reset each open; duplicate PDF menu | Unified remembered export choices, document name, modal/Escape, preserved vector safety |
| U014 | panels.rs pro-only options and Quick Mask/screen-mode/edit-toolbar controls | Every theme exposes same working tools, visible at supported widths |
| U016 | Shift-to-cycle false default; Ctrl+K Search; no Rotate View registry tool; Tab needs check | Retain original convention/Rotate View/panel acceptance; do not waive from generic shortcut fixes |
| U017 | filter_dialog factory defaults; state Untitled-1/1920x1080; hard-coded Recent | Remember successful choices, increment names, cancel leaves defaults unchanged |
| U018 | Object/Effect appended after Help; separate Vector menu | Logical Linux menu order and original shell integration |
| U019 | SHARED_ADDITIONS appended as separate Pixel slots | Merge equivalent shape/type/crop flyouts without removing vector functions |
| U020 | Geometry appended after whole Layer menu | Place immediately below Combine Shapes |
| U021 | All menu labels rendered without overflow | Reachable compact/overflow menu at 760x480 |
| U022 | Preferences Integrations is empty placeholder | Route to actual setup or remove empty navigation |
| U023 | Engine errors leak command IDs/BadParams/NoLayer | Plain-English reachable errors, retain detailed diagnostics separately |
| U024 | Library Show in Finder and Cmd chord on Linux; vocabulary differences | Correct Linux file-manager/Ctrl wording and original Classic terminology |
| U025 | Library defaults Detail; empty grid has text and wrong chord but no Import action | Useful initial grid and clickable import |
| U026 | First export defaults 2048, not full size; remaining view/navigation/sort/history/typed-value/denoiser clauses | Preserve whole original checklist; Develop milestone not blanket closure |
| U027/UX05 | Library theme always default dark; preferences/help remain separate | Consistent host theme and focused title/menu/settings behavior |
| U028 | Floating panels/no Vector workspace; Symbols exposes 0..1 colour values | Retain panel persistence/name/selection/Unlock All/Show All/release checklist |
| U029 | Most disabled commands lack reasons; basic context tool menus | Original object/right-click AI/Vector picker and disabled explanation flows |
| U030 | Remove mask dropped on tool switch; Clear and Cancel both close; remove_immediately opt-in | Original mask retention/clear/cancel/default behavior, keep local undo |
| U031 | Menu checks missing filmstrip/draft-refine/Generate/models; Draft duplicate | Correct check gutter and one discoverable action per function |
| U032 | New shapes named Live Shape | Descriptive shape and boolean names |
| U033 | Date groups force new rows/default Day at large thumbs | Original non-sparse default layout; separate scale benchmark remains deferred |
| U034 | Default tool Brush | Default Move |
| U035 | Library Forward always disabled; cloud no-op; Activity opens History; Copy/Paste toggles | Meaningful navigation/status and stable actions |
| UX07 | Liquify field px/node and dab timing always visible | Diagnostics opt-in; ordinary footer text |
| UX09 | Frame Composition silently empty for non-box text | Explicit select-frame-text empty state |
| UX10 | Restoration ordinary Window; context bar draws over preview | Modal/overlay ownership prevents preview obstruction |

## Ownership and sequencing

Root owns O059 then shared persona/theme/shell paths after current release benchmark. format_metadata owns U013 export unification in isolated checkout with narrow dialogs/vector_integration snippets and vector_export basename helper. Frozen 35-file PDF encryption handoff remains separately reviewed, not integrated during release compilation. One Cargo lane; focused regressions and affected suites at coherent milestones. Final native UX, full workspace/hardware/model/package/upgrade gates and normal v2 push/install remain required.
