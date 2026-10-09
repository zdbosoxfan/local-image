# Rich tool tips and named tool sets

Continued the committed stopped-builder WIP and the machine-crash checkpoint (`79566c79`) on this branch. The coordinator checkpointed the review fixes as `7ecd44de` and merged the working branch as `5f1f1ae9`; this agent made no commits. No dependencies or GUI windows were added. Further changes are left uncommitted.

## A. Tool tips

- Compositing: exhaustive text for all 53 toolbar tools, all 11 Liquify strip tools, and Camera Raw Zoom, Hand, and Color Sampler. Flyout entries have accessible names and effective shortcut labels; tips are suppressed while a flyout is open.
- Library/Develop: one table covering the strip, all 11 mask-creation tiles, removal modes, red/pet-eye modes, straightening/guided tools, white-balance, targeted adjustment, Point Color and film-base pickers.
- Fixed a Library rich-tip deadlock: read the egui frame counter before taking the context data write lock.
- Rich cards use the theme tokens, a 32-point icon slot, a shortcut chip when bound, descriptions, 2–4 verified usage lines, and sibling names for tool groups. Width is bounded to about 320 points; the first hover waits 0.5 seconds, neighboring tools open immediately, and pointer holds/popups suppress cards.
- Rich/Simple/Off preferences default to Rich and survive old settings and JSON round trips. Existing global tooltip-disable preferences still apply in Compositing.
- Compositing's toolbar groups and modal tools now share their shortcut lookup with their key handlers and shortcut editor, including reassignment/unbinding. Library uses its existing UI/engine command lookup (Library has no shortcut-remapping preference).
- Corrected WIP claims against source code, including History Brush's opening-state source, Pen gestures, Crop versus Straighten, AI Remove's actual Remove button, Magic Eraser options, AI Cutout's Erase/Restore controls, and Pet Eye. Added Red Eye/Pet Eye sibling names and included Camera Raw in the complete-entry test. Library Simple-mode coverage checks an actual bound shortcut. The Hand card respects remapping and unbinding of temporary pan; unbinding retains two verified usage lines, including middle-button panning.

## B. Tool sets

- Read-only All Tools, Photographer, Essentials, Retouching, and AI templates; All Tools is the default. AI includes the actual learned-model tools, including Quick Selection's optional Subject Assist.
- Toolbar name switcher and Window > Tool Set menu. Edit Toolbar supports New from current, Duplicate, Rename, Delete custom, Save as New Set, reset, checkboxes, and Up/Down ordering. Cancel reverts the toolbar preview; OK keeps it.
- `toolset.list/select/save/rename/delete/reset` plus `toolset.duplicate`, persisted with Pixel persona defaults. Invalid tool arrays are rejected without changing preferences; empty custom layouts are allowed; deleting the active set falls back to All Tools. Reset keeps custom sets.
- Non-default legacy hidden/order preferences migrate to My Tools, preserving both membership and order. A canonical/default legacy order stays on All Tools. Hidden tools remain reachable by their effective key and group cycling.
- Removed the WIP production `unwrap()` from toolbar grouping; explicit append-or-create handling preserves every slot without a panic.
- Toolbar order applies to groups and their flyout members; canonical slot IDs preserve last-used tools across set switches. Parse tool names once per layout instead of repeatedly inside sort/rank comparisons, in both the toolbar and its editing dialog.

## Validation

- Initial offline `cargo +1.98.1 check` passed for all three affected crates; the recovery check after the owner-reported machine-wide OOM also passed. The post-merge recovery check passed for all three crates.
- Focused engine `toolsets::tests`: **8 passed**, including built-in protections, CRUD, reset, invalid input, empty sets, and migration/round trips.
- Compositing `tool_tips::tests`: **12 passed** (11 together plus the added Hand remapping/unbinding test); `toolsets_ui::tests`: **8 passed**. The initial rename test found two text inputs; adding an accessible Tool Set Name label and querying that field fixed it.
- Library `tool_tips` filter: **5 passed**, including actual hover/click/drag interactions. The white-balance scenario opens the folded Color section through the UI before hovering the picker.
- Compositing `every_tl_literal_is_translated`: **1 passed**. The pending Hand test and the translation coverage rerun passed after the working-branch merge. The existing Type flyout test now locates the Horizontal Type Tool by its accessible name: the new switcher had invalidated its position-based button lookup. Long press, release without selection, and clicking Vertical Type passed in the targeted rerun. Total unique focused checks: **35 passed**.
- Ran the full suites **once**, limited to the three changed crates, with `CARGO_BUILD_JOBS=3 RUST_TEST_THREADS=3 cargo +1.98.1 test --offline -p photocraft-ui-egui -p lightcraft-ui-egui -p photocraft-engine --no-fail-fast` (log: `target/tooltips-toolsets-tests.log`). Initial result: **2,041 passed, 22 ignored**, two failed tests and three GPU test binaries aborted with SIGSEGV.
- Resolved the position-based Type flyout test as described above. Reran only the three aborted GPU targets with `RUST_TEST_THREADS=1`: `cargo +1.98.1 test --offline -p photocraft-ui-egui --test adjust_preview_gpu --test color_managed_canvas --test drag_preview_canvas --no-fail-fast -- --nocapture`. **7 passed**; they rendered with the adapter available in this environment (log: `target/tooltips-toolsets-gpu-retry.log`).
- Final distinct test outcomes after those targeted reruns: **2,049 passed, 22 ignored, 1 environment-blocked test**. Library/Develop: **192 passed, 1 ignored**. Engine: **860 passed, 15 ignored**, plus the blocked mock-server test. Compositing UI: **997 passed, 6 ignored**. Existing ignored tests stayed ignored. No memory watchdog kills occurred.
- Final clippy **passed** for all three crates: `CARGO_BUILD_JOBS=3 CARGO_TARGET_DIR=target/clippy cargo +1.98.1 clippy --offline -p photocraft-ui-egui -p lightcraft-ui-egui -p photocraft-engine --all-targets -- -D warnings`. This includes all targets and the final Type flyout test edit (log: `target/tooltips-toolsets-clippy.log`).
- Formatting and `git diff --check` passed. `assets/attributions.json` was not involved in the merge or this work, so regeneration was unnecessary.
- One UI dependency build was interrupted by a transient syntax error while the external machine-wide compiler gate was being updated. The gate subsequently passed `bash -n`; the same filtered command succeeded on retry. The later owner-reported OOM stopped the pending Library build; it was resumed after the successful recovery check. No owner files outside this worktree were edited.

## GPU coverage and remaining work

No GPU code changed and no GPU tests were added. The new interaction tests use egui_kittest or Library's windowless CPU harness. Seven existing GPU tests passed on a serial retry; this does not establish RTX 5090 coverage, which remains the coordinator's responsibility. New Compositing translation entries are only the literals required by the existing translation-coverage test; new Library text may fall back to English.

The requested implementation is complete. **Full-suite validation has one remaining environment blocker:** `ai_cmds::tests::ai_commands_against_the_mock_server` fails at `MockComfy::start()` (`crates/pc-engine/src/ai_cmds.rs:764`) because socket creation/binding returns OS error 1, `PermissionDenied: Operation not permitted`, under this sandbox. It fails before issuing any engine command. Rerun this existing test in an environment that permits its local mock HTTP server:

```sh
PATH="$HOME/.cargo/bin:$PATH" CARGO_BUILD_JOBS=3 cargo +1.98.1 test --offline -p photocraft-engine --lib ai_commands_against_the_mock_server -- --nocapture
```
