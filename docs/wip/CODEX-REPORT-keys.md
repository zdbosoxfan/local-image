# Keyboard shortcut profiles — B1, B2, B3

Continued the branch's existing shared shortcut editor, command catalogs, exact-chord matching and host routing. Changes are uncommitted. The supplied, already-modified `CODEX-TASK.md` is untouched.

## B1: preferences and migration

- Added `shortcutProfiles` to the existing camelCase preferences schema. Library+Develop defaults to Lightroom; Editor defaults to Photoshop. Each workspace independently stores an active built-in or named custom profile. Custom profiles retain their built-in parent and sparse command overrides.
- Duplicate, rename and delete preserve bindings and workspace independence. Deleting the active custom profile selects its built-in parent. Built-ins remain code tables/catalogs rather than editable preferences.
- Existing flat `shortcuts` overrides migrate by the `library::` prefix to active `Custom (migrated)` profiles. Migration retains the legacy map; the pre-existing loader's canonicalization of recognized key spellings is retained. A persisted migration marker prevents stale legacy bindings from resurrecting a deleted profile after restart.
- Legacy preference/shortcut-command writes remain usable after migration; changed workspace assignments update a custom profile. Profile writes validate workspace membership, parents, names and chords atomically. Dotted command IDs and profile names are supported in keyed preference paths.
- Migration and a frozen pre-profile serde reader verify that the saved legacy map remains readable by an older settings reader. This test uses the old shape, rather than the new profile resolver; no older executable was launched.

## B2: dispatch and focus

- Cmd/Ctrl+K is the fixed Global Preferences key. Cmd/Ctrl+, remains scoped: Library Settings and Photoshop layer visibility can use it independently without a conflict.
- Both workspace dispatchers resolve through their own active profile. Global quit, preferences, undo/redo, workspace switching and screen/fullscreen chords have precedence and are fixed. Global collisions remain visible in custom profiles and cannot execute their assigned action, including a spring-loaded tool assignment.
- Capture One's B/E/G/M apply while Develop's Edit or Mask panel is active: draw, erase, linear gradient, mask overlay. E/G and the target-collection B retain their Library context. The sourced current L gradient alias and T radial gradient are supported. Lightroom retains O/K/M/Shift+M and its existing Library commands.
- The host mirrors only the Library profile's current overrides and parent. Hidden Editor shortcuts cannot fire during Library/Develop focus, including the host's first startup frame; hidden Library tools do not fire while Editor is active. Text/widget/modal focus and hold-key lifecycle rules are retained.
- Existing tests that expected to steal Global Undo/Preferences now assert fixed Global precedence and retained legacy data. Ordinary bulk-reassignment tests use Layer via Copy's Cmd/Ctrl+J, preserving their conflict/transfer assertions. A prior persona test now counts recorded Layers rows, since the shared Color panel legitimately also labels a Background paint button.
- Two small supporting seams were necessary beyond the named dispatch files: `shortcuts_catalog.rs` exposes the erase/current-gradient rows and computes contextual effective labels; `vector_integration.rs`'s existing direct tool-override loop now reads the Editor profile and rejects Global collisions. No brush/tablet, paint, segmentation, sysmem or pool code changed.

## B3: shortcut editor

- The existing dialog now has Global / Library + Develop / Editor scope selection and an independent profile dropdown. Global shows the fixed list with no edit controls.
- Named duplication, rename, delete, inline Global/workspace conflicts and key-event capture use the existing dialog. Editing a built-in creates a named custom copy. Scope changes cancel outstanding capture; resetting a profile preserves the other workspace's bindings.
- The profile name field has an explicit accessibility label. Controls wrap at narrow widths; long command names truncate with full hover text; the list scrolls in both directions if needed.
- Preferences includes a tested Keyboard Shortcuts entry. Draft changes stay in the dialog until OK; cancellation and malformed drafts preserve saved preferences.
- Added clean-room translations for the new scope/reset labels in all twelve non-English catalogs. The required completeness check also exposed three existing color-control labels; those received translations without changing the controls or weakening the check.
- Optional JSON import/export was not added.

## Sources and supported mapping limits

- [Adobe Lightroom Classic published shortcut list](https://helpx.adobe.com/lightroom-classic/desktop/introduction-to-lightroom-classic/keyboard-shortcuts.html): retained Lightroom command/alias catalog, tool letters, overlay, target collection, rating and navigation conventions.
- [Capture One's published adjustment-layer shortcuts](https://www.captureone.com/blog/discover-the-shortcuts-of-the-adjustments-layers-tool): B/E/G/M and their mask-tool context. [Current Capture One shortcut changes](https://support.captureone.com/hc/en-us/articles/360003399778-What-are-the-hotkeys-in-Capture-One) identify L as linear gradient in newer releases; G is retained as the owner-required older compatibility shortcut. [Capture One radial-gradient documentation](https://support.captureone.com/hc/en-us/articles/360002627458-Editing-the-Radial-Gradient-Mask) supplies T.
- [Adobe's published Photoshop reference](https://helpx.adobe.com/pdf/cs6/photoshop_reference.pdf), the existing Photoshop command/menu/tool catalogs, and [Adobe's current published shortcut list](https://helpx.adobe.com/photoshop/web/get-set-up/preferences-and-settings/keyboard-shortcuts.html). The former desktop shortcut URL now redirects to [instructions for viewing shortcuts](https://helpx.adobe.com/photoshop/desktop/get-started/settings-and-preferences/view-keyboard-shortcuts.html), so it is not treated as fresh evidence for a new binding.
- [Affinity Photo desktop shortcut documentation](https://affinity.help/photo2/en-US.lproj/pages/Workspace/shortcuts.html): only supported file/edit and equivalent tool keys are assigned. Unsupported Photoshop-only operations are unassigned in that profile. The UI explicitly marks the subset and the owner's R = Rotate View / J = Healing and AI Remove decisions; Affinity's Color Replacement R and unsupported personas/tools are not claimed as implemented. Existing Vector keyboard presets continue through their established persona routing.
- Layout, key-event ownership, capture and GPU screenshot implementation follow the repository's existing egui 0.36 shortcut, shared-host and swatches tests; the RAW fixture follows `lc-raw/tests/it/dng.rs` and its native Rust DNG writer.

## Validation

All cargo commands used the supplied `claude-env.sh`, pinned Rust 1.98.1, offline mode, four build jobs and the retained worktree target directory. No workspace-wide suite was run.

| Crate | Full-suite passed | Ignored | Evidence |
|---|---:|---:|---|
| `lightcraft-ui-egui` | 328 | 5 | `target/keys-full-tests.log` |
| `local-image` | 143 | 2 | `target/keys-full-tests.log` |
| `photocraft-engine` | 2,092 | 33 | `target/keys-pc-full-tests.log` |
| `photocraft-ui-egui` | 1,648 | 66 | `target/keys-editor-complete.log` |

- Clippy: all four changed crates, `--all-targets -- -D warnings`, clean; `target/keys-clippy-verified.log`.
- Nine new acceptance tests: three preference/migration/Global tests, four egui_kittest Editor tests (including the GPU capture), one Library/Develop real-key walkthrough, one real host focus/modal walkthrough. Final Editor/engine/host cases passed in `target/keys-acceptance-final.log`; the Library RAW/native-event case passed in `target/keys-library-acceptance-final.log`.
- The Library test delivers modal key events through native egui input. Its earlier use of the control channel while the host had suspended that queue timed out and replayed the queued key after closing the modal; the corrected test verifies native input ownership and then sends a real Quit chord over the conflicting C1 brush assignment. The Editor counterpart verifies the actual dispatch log with side effects disabled.
- The 95 existing shortcut regression cases passed after preserving ordinary key transfers and updating obsolete Global-key expectations: `target/keys-shortcut-regressions-3.log`.
- Localization: 22/22 checks passed after fixing the missing catalog entries: `target/keys-i18n-tests.log`.
- Initial failures and intermediate logs are retained in `target/keys-*tests*.log`; no failing assertion was suppressed. One focused cargo process returned signal status 143 after its selected tests had all passed; final full-suite and acceptance checks are recorded separately.
- The Editor suite was repeated after localization failures: first missing catalog entries, then an ambiguous Japanese color-control label. Qualified color-control translations keep those controls distinct from Camera Raw labels; the integration check passed across all supported languages (`target/keys-localization-integration-final.log`) and the final full Editor run passed. The Library/Develop and host full suites were not repeated. Incidental icons/screenshot outputs from existing full-suite tests are kept under `target/` or restored to their original tracked state.

## Artifacts

`target/keys-ux/` contains 12 RTX 5090/Vulkan-rendered shortcut-editor screenshots: each scope in dark/light at 480 and 900 points wide, with a Global conflict visible in both editable workspace scopes. All 12 were visually inspected for readable bindings, profile controls, conflict labels and narrow-width wrapping. Global is deliberately read-only.

`target/keys-ux/reference/generate.rs` and `45mp.dng` contain the reproducible synthetic 8256 × 5504 Bayer RAW fixture (45.44 MP). The Library/Develop real-key walkthrough was also run against this actual decoded RAW. It is a generated sensor fixture, not an owner's photograph or a photographic image-quality benchmark.

`target/keys-ux/runtime.txt` records one warm, real shortcut-editor Duplicate widget interaction plus profile update and RTX GPU render/readback. It is a headless UI sample, not continuous frame rate or a desktop compositor measurement. No photo-processing speed claim is made.

Final warm Duplicate sample: **24.692 ms**, including GPU readback.

All mandatory B1–B3 work is complete. The documented Affinity subset and optional omitted JSON import/export are the only feature limits. No GPU compute or photographic pipeline changes were required.
