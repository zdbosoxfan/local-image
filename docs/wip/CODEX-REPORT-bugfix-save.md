# Saving, round trip and data-safety final review

Continued the current branch at `c8ca0b63` and retained the existing hotfixes and known-test fixes. The owner's pre-existing `CODEX-TASK.md` edit is preserved. Changes remain uncommitted. No GUI windows were opened; neither Local Image nor ComfyUI was stopped.

| Item | Verdict after current-code re-check | Fix | Regression |
| --- | --- | --- | --- |
| O-007 | Confirmed | TIFF prompt owns its document ID. Close/Quit waits for successful document-save completion; copy, failure and cancellation retain protection. Save & Next and Save and Return resume after TIFF completion. | `bugfix_save_o007_*`: actual layered TIFF reopen, tab switch, close/quit, filmstrip deferred navigation. |
| O-008 / F01 | Confirmed | Flat Save As and automation output behave as copies: retain working path and saved revision. Suggest a sibling `.psd` for flat originals. A flat TIFF copy cannot satisfy the close guard. | `bugfix_save_o008_*`: PNG/JPEG/WebP/flat TIFF plus close protection and original-name suggestion. |
| O-009 | Confirmed | Overwrite reload resets Develop on the master and its virtual copies in the same catalogue undo transaction and syncs the linked Develop layer. Photoshop originals always show a flattening warning even when Don't Ask Again was saved. | `bugfix_save_o009_*`: non-default exposure reset/undo; real host master/virtual-copy reset and linked Develop refresh; layered-original confirmation guard. |
| O-010 | Confirmed | Recompute dirty reach after answers; remember answered document revisions so a child save can require another answer for its parent. | `bugfix_save_o010_*`: actual Edit Contents child save, Close All and Quit. |
| F03 | Confirmed | Use the existing rendered fallback for a shape with a separate vector mask. Bake clipping into layer pixels; remove Photoshop fill/shape proxy blocks; declare editability loss. The in-memory shape/mask remain editable. | `bugfix_save_f03_*`: PSD and PSB pixels, compatible raster records, explicit warning, second round trip, source unchanged. |
| O-020 | Confirmed | Group only adjacent undo entries belonging to that import. Create a requested album with its first batch. Intervening user edits split the import into separate history groups; no history reordering. | `bugfix_save_o020_*`: two real prepared batches with intervening rating and exposure; uninterrupted batches remain one undo step. Existing `import_review_dialog` verifies album/keyword import and one-step Undo. |
| O-021 | Confirmed | Album assignment no longer changes selection. Auto Import never restores an old selection; manual completion respects changed selection and active interactions. | `bugfix_save_o021_*` and import regression: selection/interaction retained during real album import and Auto Import while its worker is gated. |
| O-022 | Confirmed | Per-field Library settings recovery, byte-preserving `.corrupt-<timestamp>` backup and notice. Syntax corruption or an unrecoverable path stops default-library opening and settings replacement until a Library folder is chosen. Use durable `safe_file::write_atomic`. | `bugfix_save_o022_*`: obsolete language, valid path/settings, backups, truncated JSON retained. |
| O-029 | Confirmed | Overwrite applies only to pre-existing files. A clash with this batch's output or sidecar uses a unique name. | `bugfix_save_o029_*`: duplicate names and raw/JPEG XMP sidecar collision. |
| O-031 | Confirmed | Prune missing paint targets independently of locks; edit validation still respects locks. | `bugfix_save_o031_*`: locked link retained, unlock valid, deleted target pruned. |
| O-045 | Confirmed | A moved floating selection counts as dirty; UI, TIFF, and automation save paths drop it before encoding. | `bugfix_save_o045_*`: floating return/drop dirty state, pixel Undo, and actual PSD reopen pixels. |
| O-046 | Confirmed | Recover valid nested Compositing preferences independently, report fields defaulted, persist the repaired state once. Runtime preference commands remain strict. | `bugfix_save_o046_*`: bad enum retains other fields and valid colour settings; stored repaired JSON reloads. |
| O-047 | Confirmed | Cancellation overrides a successful worker result after Saving. | `bugfix_save_o047_*`: worker cancels during Saving and returns success; completion reports cancelled. |
| O-048 | Confirmed | Export As closes only on successful write. Picker cancellation and write errors keep all fields; UI displays the failure. | `bugfix_save_o048_*`: cancel, disk-full display, fields retained, successful retry. |
| U-006 | Confirmed | Every keyboard/menu/context deletion asks Remove from Library / Delete from Disk / Cancel. Capture IDs at prompt creation. Remove keeps source files; disk deletion uses recoverable host Trash, protects virtual/shared copies, and keeps failed records. Rename catalogue-only removal and add Empty Recently Deleted. | `bugfix_save_u006_*`: default prompt, all three controls, captured targets, context route, Trash success/failure, shared-file protection, consistent multi-photo album removal/undo, and no Trash call for Remove. |

The F03 choice deliberately preserves appearance through a compatible raster fallback rather than inventing a second PSD `vmsk` slot. PSD/PSB export therefore declares that independent shape-mask editability is lost. Native `.pcraft` remains the editable format. Normal reloads preserve Develop; only the explicit overwrite event requests reset.

The host bridge, common preferences parser, Library UI delete controls, and one vector metadata prune line are the minimal cross-boundary changes needed for these assigned findings. New user-visible keys are listed in Compositing's `i18n/pending_vector.rs` and Library's `PENDING_SAVE_TRANSLATIONS`.

Reference behavior: Photoshop documents [Save a Copy](https://helpx.adobe.com/photoshop/desktop/save-and-export/save-files/save-your-work.html) for formats that cannot preserve the work and [Discard Layers and Save a Copy for TIFF](https://helpx.adobe.com/photoshop/desktop/save-and-export/save-files/save-in-tiff-format.html). Adobe's [Lightroom Classic help](https://www.adobe.com/learn/lightroom-classic/web/common-questions-lightroom-classic) distinguishes Remove (keep the file) from Delete from Disk. These procedures were documented; the commercial applications were not launched.

TEST-CHANGED: `headless::tests::settings_dialog_by_keyboard_and_clicks` now verifies deletion prompts with the old optional-confirmation flag off, checks captured IDs, and expects Remove from Library to remove the catalogue record. The optional checkbox and soft-delete expectation are obsolete under the requested Lightroom Classic behavior. No test was removed or ignored.

TEST-CHANGED: `filmstrip_ui::tests::save_and_next_works_through_a_folder` now expects the unsaved working tab to remain open after saving a PNG copy and verifies its original path and complete document on return. The old expectation closed the working tab and reopened flattened pixels, which is the O-008 data-loss behavior. Output bytes, untouched-original, navigation, badge, clean-tab closing and unsaved-tab checks are retained.

## Validation

Rust/cargo 1.98.1 (Fedora binaries). `RUSTC_WRAPPER=/home/zdavidson/.local/share/local-image-dev/bin/li-rustc-gate` remains set for every Rust build. All build/test/check/Clippy commands use `--offline`; `CARGO_BUILD_JOBS` is unset. Headless tests only. Versions and environment: [environment.log](bugfix-save-validation/environment.log).

Initial invocation `RUSTC_WRAPPER=li-rustc-gate cargo +1.98.1 check --offline -p photocraft-ui-egui -p lightcraft-ui-egui -p local-image` failed because this shell exposes the native Cargo rather than the rustup proxy (`no such command: +1.98.1`). A plain-Cargo attempt with the short wrapper name failed because that name is not on PATH. Subsequent commands retain the already-configured absolute wrapper and use plain Cargo, which is 1.98.1. The first compile also caught the Library's generated translation macro rejecting new pending keys; the messages now use translated runtime templates.

The initial regression invocation stopped after two new-fixture failures: the album test used the intentionally selecting manual-import command, and the export fixture had no demo decoder. Both fixtures were corrected to exercise the real batch boundary and valid demo sources. Later fixture corrections supplied actual pixels for smart-object conversion, cleared intentionally unused headless texture deltas, and checked the engine's existing monotonic revision behavior after Undo. No existing test expectation changed for these corrections.

The preliminary new-test invocation passed **27 tests** across all six touched crates (0 failed). The final run adds clean-document TIFF Save and Return coverage and expands filmstrip navigation to flat TIFF copies. Output: [new-tests.log](bugfix-save-validation/new-tests.log).

```sh
cargo test --offline -p photocraft-engine -p photocraft-ui-egui -p photocraft-io -p lightcraft-engine -p lightcraft-ui-egui -p local-image --no-fail-fast bugfix_save_
```

Before-fix verification:

```sh
python3 docs/wip/bugfix-save-validation/baseline.py > docs/wip/bugfix-save-validation/baseline.log 2>&1
```

The script temporarily runs HEAD production code with the new existing-API regressions, using the Cargo test command above. Output: **24 runtime failures, 0 passes; Cargo exit 101 (expected)**, covering every assigned finding. All final source bytes were restored in `finally` and verified identical. The new-only preferences reader's baseline adapter reproduces HEAD's whole-`UiState` deserialization boundary; the new-only deletion helpers are omitted and U-006 is demonstrated by the existing context-menu API. [baseline.py](bugfix-save-validation/baseline.py), [baseline.log](bugfix-save-validation/baseline.log).

Final new and existing changed-file tests:

```sh
python3 docs/wip/bugfix-save-validation/existing.py > docs/wip/bugfix-save-validation/final-tests.log 2>&1
python3 docs/wip/bugfix-save-validation/additional.py > docs/wip/bugfix-save-validation/additional-tests.log 2>&1
```

The first script builds the six touched crates once with:

```sh
cargo test --offline --no-run --message-format=json -p photocraft-engine -p photocraft-ui-egui -p photocraft-io -p lightcraft-engine -p lightcraft-ui-egui -p local-image
```

It then runs each compiled test binary with the exact selected names, recorded verbatim in [final-tests.log](bugfix-save-validation/final-tests.log). Build exit 0; **28 new regressions passed**. Of 446 selected existing tests, 441 passed, one failed, and four retained their pre-existing ignored status. The failed `headless::tests::import_review_dialog` correctly exposed an album created outside the import undo group; production code now defers creation into the first batch. Its expectation is unchanged. The second script ran the 13 inline tests omitted by the initial selection: **13 passed, exit 0**; exact binary commands and output: [additional-tests.log](bugfix-save-validation/additional-tests.log).

Only the failed test and the two directly related import regressions were rerun:

```sh
cargo test --offline -p lightcraft-ui-egui --lib -- --exact headless::tests::import_review_dialog import::bugfix_save_tests::bugfix_save_o020_batches_never_absorb_intervening_ratings_or_develop_edits import::bugfix_save_tests::bugfix_save_o021_auto_import_keeps_selection_changed_while_worker_was_reading > docs/wip/bugfix-save-validation/import-recheck.log 2>&1
```

Output: **3 passed, 0 failed, exit 0**, including the unchanged import-review expectation. [import-recheck.log](bugfix-save-validation/import-recheck.log).

Final unique-test totals: **28 new regressions and 455 existing tests passed; four pre-existing ignored tests retained**. No test attributes were removed and no new `ignore` attributes were introduced. The ignored cases are the 12 GB PSB stress case, two opt-in host benchmark/render probes, and the floating-selection benchmark. The changed-file test attribute count increased from 274 to 302 (+28).

Workspace and strict lint commands:

```sh
cargo check --offline --workspace > docs/wip/bugfix-save-validation/workspace-check.log 2>&1
cargo clippy --offline -p photocraft-engine -p photocraft-ui-egui -p photocraft-io -p lightcraft-engine -p lightcraft-ui-egui -p local-image --all-targets -- -D warnings > docs/wip/bugfix-save-validation/clippy.log 2>&1
cargo fmt --all -- --check > docs/wip/bugfix-save-validation/fmt-check.log 2>&1
git diff --check
```

Workspace check: **exit 0**, `Finished dev profile … in 3m 06s`; [workspace-check.log](bugfix-save-validation/workspace-check.log). Formatting and diff checks pass (exit 0, no output); [fmt-check.log](bugfix-save-validation/fmt-check.log).

The first strict Clippy invocation exited 101 with two new Library style diagnostics (`collapsible_if`, `items_after_test_module`), recorded in [clippy.log](bugfix-save-validation/clippy.log). Collapsed the nested condition and moved the pending translation list before the test module, with no behavior or test changes. Recheck:

```sh
cargo clippy --offline -p photocraft-engine -p photocraft-ui-egui -p photocraft-io -p lightcraft-engine -p lightcraft-ui-egui -p local-image --all-targets -- -D warnings > docs/wip/bugfix-save-validation/clippy-recheck.log 2>&1
cargo fmt --all > docs/wip/bugfix-save-validation/fmt-final.log 2>&1
cargo fmt --all -- --check > docs/wip/bugfix-save-validation/fmt-check-final.log 2>&1
git diff --check
```

Strict Clippy: **exit 0**, `Finished dev profile … in 53.04s`; [clippy-recheck.log](bugfix-save-validation/clippy-recheck.log). The pre-existing optional embedded-font build-script notice remains in the build logs; no Rust lint diagnostic remains. Final formatting and diff checks: **exit 0, no output**.
