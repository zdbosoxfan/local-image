# J20 / D25 — one shortcut backend

Continued the branch's existing workspace profiles, shared shortcut editor, migration and host routing. Added `li-keys` and replaced the duplicate production implementations with compatibility exports/adapters. Changes are uncommitted. The supplied, already-modified `docs/wip/CODEX-TASK.md` was not edited.

## What merged

- One portable `Chord` parser and persisted key vocabulary, including case-insensitive modifiers, named punctuation, arrow/numpad aliases, F1–F35 and the existing Vector `Shift+~` alias.
- One platform-aware matcher and event-overlap definition for dispatch, conflict reporting, reassignment, fixed Global protection and native key events. Linux/Windows fold physical Control into logical Command; macOS preserves physical Control separately.
- One Capture One delta table, one Affinity subset table and one immutable Global table. Photoshop/Lightroom defaults remain in their established command/menu catalogs. Capture One B/E/G/M remains contextual to Develop Edit/Mask, with L/T retained; Library B/E/G, Affinity temporary tools, R = Rotate View and J = Healing / AI Remove retain their previous behavior.
- Shared `ShortcutProfile`, `ShortcutWorkspace` and `ShortcutProfiles`, with the original serde shape, defaults, migration marker, sparse overrides and unchanged invalid legacy binding preservation. The engine continues to own preference persistence, recovery and atomic preference updates.
- Shared press consumption and ordered event extraction. Both frontends retain their own command handlers, modal/focus policy, contextual tools, grid navigation and held-tool lifecycle. Library now processes events in arrival order and stops polling when a command opens a modal.
- Shortcut capture uses the same platform mapping; the previous cross-crate `#[path]` keymap include and Vector parser extension are gone. Old public paths (`prefs::*`, both `keymaps` modules, parser/matcher functions and `native_menu::Chord`) remain available as thin adapters/re-exports.

The core depends only on the already-locked serde package. Its optional `egui` feature uses the existing egui dependency for input adapters; serde_json is an existing test dependency. Cargo.lock adds only the internal `li-keys` package and three dependency edges. No external package, C/-sys dependency or unsafe code was added.

## User-visible changes

- Saved lowercase/modifier aliases and shifted punctuation execute consistently in Library and Editor. Equals/Plus zoom aliases and Delete/Backspace equivalence use the existing editor behavior.
- Event-overlapping spellings such as `Cmd+=` / `Cmd++` and `Shift+[` / `Shift+{` now report a conflict and obey Global/custom/default precedence. Capturing or transferring a conflicting chord removes the previous assignment according to the existing dialog/engine rules; the dialog and dispatch share the same collision definition.
- On macOS, an extra physical Control key cannot invoke a Command-only shortcut. Capturing Control+Command now retains both modifiers instead of silently dropping Control.
- User-assigned F16–F35 receive the same text-field shortcut policy as F1–F15. Native menu accelerators retain their existing F1–F24 limit.
- Library handles E then B in that order, including two events in one frame; opening Export leaves later keys to its modal. Previously Library gathered commands in catalog order and did not consume the key events.

Native `portable()` retains Ctrl-before-Cmd spelling, while persisted normalization retains Cmd-before-Ctrl. Native backquote fields retain literal `` ` `` for the existing macOS accelerator consumer; persisted spellings remain `Backtick`. No controls or styles were added, and no new product strings require an i18n pending entry.

## Parity and measured quality

Frozen pre-merge engine normalizer, Library/editor parsers/matchers and native Chord live only in `li-keys/src/legacy_tests.rs`, compiled only for tests. Production has one parser/matcher; the frozen references are deliberately retained for future regression gates.

- All **117 egui logical keys × 16 modifier combinations = 1,872 cases per old implementation** match their previous parser/key-event behavior (engine, Library, Editor and native), with normalization and Delete adapters checked separately.
- **3,504,384** key/modifier comparisons preserve all of Library's previous positive matches and the editor's layout-aware matches after platform modifier canonicalization. On the tested Ctrl platform, the old union recognizes **4,952** positive combinations; the shared matcher recognizes **5,080**, adding consistent physical/logical Control aliases. Separate explicit Mac/Ctrl tests reject the old macOS extra-Control ambiguity.
- Core/egui collision parity covers **140 chords × 140 chords × 2 platforms = 39,200 pairs**, including punctuation, Delete aliases and function keys. Connected three-owner collision groups are checked too.
- Frozen JSON verifies migration, disabled bindings, invalid unchanged legacy values, profile duplication/rename/deletion, restart and workspace independence. Engine acceptance additionally transfers an overlapping key from Copy to Cut through the actual command API.
- Library acceptance delivers real egui key events for Capture One tool order, saved aliases, shifted punctuation, Global protection and modal ownership. Editor acceptance checks both profile catalogs and synthesized native events on both platforms.
- The egui_kittest acceptance test clicks the actual scope picker and shortcut-capture button, captures a shifted symbol, verifies the displaced Library assignment and confirms the custom profile while preserving Editor's profile.

## Speed and memory

Small samples use the actual frontend shortcut handlers inside egui event frames: Library changes the mask-overlay state; Editor resolves the real Preferences command with its existing side-effect-free dispatch seam. Each takes five warm-up frames and 20 measured frames. These measure CPU shortcut-event handling, not rendering, GPU throughput or desktop frame rate. The PC is shared with other jobs; samples do not establish portable performance guarantees.

| CPU event-frame path | Before median (min–max) | After median (min–max) | Median change |
| --- | ---: | ---: | ---: |
| Library mask-overlay command | 294.067 µs (289.037–375.736) | 383.736 µs (380.976–387.256) | +89.669 µs / +30.5% |
| Editor Preferences command | 552.664 µs (542.994–592.044) | 242.558 µs (238.527–302.207) | −310.106 µs / −56.1% |

Before logs: `target/j20-baseline-library.log`, `target/j20-baseline-editor.log`. Final serial samples: `target/j20-samples.log`. Both paths stay below 0.4 ms median in this small sample. Library's additional normalization, collision and ordered-consumption work costs more than its previous literal/exact matcher; that measured tradeoff is retained for consistent behavior and conflict safety. A measured intermediate Library sample was 575.914 µs; rejecting unrelated key/modifier pairs early and removing redundant parsing reduced it to 383.736 µs (33.4% lower), without changing collision results.

The matcher allocates nothing per comparison, rejects unrelated key/modifier pairs before shifted-symbol checks, and caches the nine parsed fixed Global chords once. There is no hardware-specific tuning or image-buffer/GPU allocation change; process RSS was not measured.

## Validation

All cargo commands use Rust 1.98.1, offline mode, the retained worktree `target/` and the supplied four build jobs. Only the new J20 tests were executed, as required by the task's overriding minimum-testing rule. No full suites, GPU acceptance runs, GUI windows, external services or benchmarks were run.

| Changed crate | New tests passed | Final evidence |
| --- | ---: | --- |
| `li-keys` | 8 | `target/j20-final-tests.log` |
| `lightcraft-ui-egui` | 3 | `target/j20-final-tests.log` |
| `photocraft-engine` | 1 | `target/j20-final-tests.log` |
| `photocraft-ui-egui` | 3 | `target/j20-final-tests.log` |

**15/15 new tests passed; 0 failed or ignored.** The serial measurement rerun executes only the two newly written sample tests. No existing tests were selected.

- `cargo +1.98.1 check --offline` on all four changed crates: passed (`target/j20-check-final.log`).
- `cargo +1.98.1 clippy --offline` on all four changed crates, `--all-targets -- -D warnings`: passed (`target/j20-clippy.log`).
- `cargo +1.98.1 check --offline -p li-keys --no-default-features`: passed, confirming the core checks without egui (`target/j20-core-no-egui.log`).
- Package formatting and diff whitespace checks: passed for job edits. The supplied task file's pre-existing trailing blank line is untouched.
- GPU tests added/executed: **0**; this is CPU key routing and serde work, and the overriding task rule prohibits GPU acceptance runs. macOS behavior is checked through explicit platform tests and native-event synthesis on the current Linux host; no macOS executable was launched.

Initial failure logs are retained: the engine test initially named a UI-only zoom command, then was corrected to actual engine Copy/Cut commands; UI compilation exposed an inherited test-only Vector modifier import; the Library Global assertion initially included the independent valid L alias, then was corrected to target the conflicting primary binding. These were fixture/import corrections, not weakened behavior checks. Review also caught native backquote representation compatibility, now covered by frozen assertions.

`TEST-CHANGED: none` — existing tests and expectations were not modified.

## Sources and scope

Implementation follows the installed egui **0.36.2** primary source: `src/data/key.rs` (logical keys and aliases), `src/data/input/event.rs` (layout-aware logical keys versus physical keys), and `src/data/input/modifiers.rs` (platform Command conventions and the permissive matcher that motivated exact physical-Control matching). Public documentation: [Key](https://docs.rs/egui/0.36.2/egui/enum.Key.html), [Event](https://docs.rs/egui/0.36.2/egui/enum.Event.html), [Modifiers](https://docs.rs/egui/0.36.2/egui/struct.Modifiers.html).

Mappings retain the previous builder's sourced tables and [report](CODEX-REPORT-keys.md): [Adobe Lightroom Classic shortcuts](https://helpx.adobe.com/lightroom-classic/desktop/introduction-to-lightroom-classic/keyboard-shortcuts.html), [Adobe Photoshop reference](https://helpx.adobe.com/pdf/cs6/photoshop_reference.pdf), [Affinity Photo shortcuts](https://affinity.help/photo2/en-US.lproj/pages/Workspace/shortcuts.html), [Capture One adjustment-layer shortcuts](https://www.captureone.com/blog/discover-the-shortcuts-of-the-adjustments-layers-tool), [Capture One current shortcuts](https://support.captureone.com/hc/en-us/articles/360003399778-What-are-the-hotkeys-in-Capture-One), and [radial-gradient documentation](https://support.captureone.com/hc/en-us/articles/360002627458-Editing-the-Radial-Gradient-Mask). Lightroom/Affinity official help was reopened during this job. No new product profile mapping is inferred from another product.

Unavoidable edits outside the listed Rust files:

- New `crates/li-keys/{Cargo.toml,src/*}`, workspace/dependent manifests and Cargo.lock to introduce and connect the shared backend.
- `pc-ui-egui/src/prefs_ui.rs`: replace capture, reserved-chord and same-chord helpers with shared policy; add the actual capture-widget acceptance test. No layout/style edits.
- `pc-ui-egui/src/vector_packages/fill_stroke_proxy.rs`: delete its local parser extension, retain the existing API as an export, and retain its test-only modifier import.
- This required report.

No Match Look, catalog/document schemas, rendering, owner photos, desktop files or coordinator tooling were changed. A read-only subagent reviewed preservation risks and the final implementation without edits or builds. J20 is complete, with no remaining required work. Full merged suites remain the coordinator's responsibility under the owner's minimum-testing rule.
