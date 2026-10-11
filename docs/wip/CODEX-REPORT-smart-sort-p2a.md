# Smart Sort & Export — Phase 2a report

The original Phase 2a implementation below continued the merged Phase 1 engine and faces core. That implementation added no dependencies or Cargo.lock packages and used Rust without unsafe code. The subsequent people-engine merge and its verification are recorded in the final section.

## Implemented

- **Entry points and dialog:** File → Smart Sort & Export… and the grid's Smart Sort Selected… open one resizable, three-step modal with the primary action always visible in the footer. The source follows the selected photos/current view, rejects are excluded unless included, and videos are counted and skipped. Automation ids cover the step controls, folder/tag editors, tag-set actions, review thumbnails/corrections, sensitivity, destination, folder layout and export controls. `ui.inspect` includes the complete dialog state and live analysis status; the control protocol's dialog-confirm action follows the wizard's primary action.
- **Tag folders and matching:** editable, removable, draggable tag chips, comma-separated paste/Enter and type-ahead suggestions; per-folder Any/All matching. Category scores use the maximum/minimum of individual tag prototypes, with the exemplar boost applied to each prototype. Built-in presets now contain short tags. CLIP text embeddings are cached only while the dialog is open, so sensitivity and correction changes reuse text inference.
- **Tag sets and presets:** built-in tag sets derive from built-in categories; user tag sets support save, replace/add, rename and delete through the UI and four engine commands. `smart-sort-tagsets.json` uses the library's existing atomic settings store and damaged-file protection. Named presets retain matching, overlap policy, enabled/renamed/reordered folder rows and custom rules. Missing persisted fields retain backward-compatible defaults.
- **Background analysis:** model loading, input decoding and image embedding run off the UI thread, with a dedicated half-core inference pool. Results arrive over a channel, enter the session cache on the UI thread, and are atomically saved as work completes and on cancellation. Resume skips completed content keys, including virtual copies. Model references are released on closing the dialog. The Library download/progress/cancel service is wired to the existing Compositing downloader in `library_host.rs`; the UI crate has no li-ai dependency. Without the service, the download button is hidden and the Compositing setup notice is shown.
- **Review:** live folder counts plus Unsorted; normal thumbnail rendering, visible-row virtualization, least-sure/capture-time ordering, confidence bars, selection, Shift/Command multi-selection, drag-to-folder and Move to / Also add to / Remove context actions. The initial review opens a populated category when Unsorted is empty. Manual badges and learned exemplars accompany corrections and re-sort the other photos. Advancing to Export applies category keywords as one catalog undo step.
- **Export:** Desktop destination default with editable path and host chooser; default category rows plus disabled Unsorted, renaming/reordering, custom category/tag pickers with AND/OR matching, and the existing advanced rules editor. Copy to every matching folder is the default; first-match respects row order. Rendered JPEG is the default, copy originals and other export presets are available, and Edit Settings reuses the normal export editor. The summary reports files/folders/photos. Optional smart albums use the same keyword rules, source ids, first-match exclusions and Unsorted fallback as the plan.
- **Export engine:** per-item `PreparedExport.subfolder`, sanitized relative/nested names, duplicate-name suffixes and folder-aware conflict resolution; `smartSort.plan` accepts first-match/Unsorted options and `smartSort.export` runs one normal export batch. Existing atomic writers create directories. Destinations inside the library, originals folders, traversal and symlink aliases into protected locations are refused. UI export uses the ordinary background progress/cancel panel and Show in folder post-action.
- **Translations:** all 62 new Library labels/errors/formatted messages (55 basic, 7 formatted) are present in Japanese, Brazilian Portuguese, simplified Chinese and traditional Chinese JSON catalogs. Existing catalog entries/order were preserved; format placeholders match in every language.
- **Extension points:** folder rules retain stable person ids, the preset reserves a folder pattern, and the dialog keeps its state in a dedicated type. Faces/people controls and Phase 2b sessions, tokens, examples, bursts and keyboard review are intentionally deferred as the task requires.

## Phase 2a verification (before the people-engine merge)

All in-scope Phase 2a work is complete. Final results: **1,682 passed, 14 ignored, 0 failed**, counting each package once. All builds were offline on toolchain 1.98.1, with three build jobs and one Cargo build at a time.

| Check | Passed | Ignored | Result |
|---|---:|---:|---|
| `lightcraft-engine` | 342 | 2 | Pass |
| `lightcraft-ui-egui` | 181 | 1 | Pass, including all four new UI tests |
| `lightcraft-catalog` | 73 | 1 | Pass |
| `li-seg` | 43 | 4 | Pass |
| `local-image` | 67 | 0 | Pass |
| `photocraft-ui-egui`, unit and integration | 976 | 6 | Pass; allow-list, attributions and i18n unchanged |
| Final Smart Sort rerun after lint/localization edits | 5 | 0 | Pass: four UI tests and the existing host model-flow test; already included above |
| Changed-crate Clippy, all targets, `-D warnings` | | | Pass |
| Workspace formatting, `git diff --check` | | | Pass |
| Existing translations, placeholders, unchanged lockfile | | | Pass |

Commands used with `PATH="$HOME/.cargo/bin:$PATH"` and `CARGO_BUILD_JOBS=3`:

```sh
CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_RUNNER=/tmp/p2-test-runner cargo +1.98.1 test --offline -p lightcraft-engine -p lightcraft-ui-egui -p lightcraft-catalog -p li-seg -p local-image
CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_RUNNER=/tmp/p2-test-runner cargo +1.98.1 test --offline -p lightcraft-ui-egui -p photocraft-ui-egui -- --test-threads=1
CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_RUNNER=/tmp/p2-test-runner cargo +1.98.1 test --offline -p lightcraft-ui-egui -p photocraft-ui-egui smart_sort -- --test-threads=1
CARGO_TARGET_DIR=target/clippy cargo +1.98.1 clippy --offline -p lightcraft-engine -p lightcraft-ui-egui -p local-image --all-targets -- -D warnings
cargo +1.98.1 fmt --all -- --check
git diff --check
```

The final UI/Compositing run used one test thread to avoid the existing shared `REMOVE_NOTE` race documented by the Phase 1 builder. The required five-crate run also passed with normal test parallelism. Logs and the sandbox runner are retained under ignored `target/smart-sort-p2a/`.

The added tests exercise actual UI text entry/clicks, tag-chip drag/reorder, tag-set save/apply/rename/delete, Any/All, analysis, photo drag/context corrections, keyword menu Undo/Redo, disabling a default row, a custom folder with two tags, both overlap export modes, actual folder/file counts, smart-album counts, host download progress/cancel and cancellation with a persisted partial cache/resume. Engine tests cover matching/exemplar behavior, tag-set/layout persistence, whitespace consistency, cached text inference, real PNG folder export, overlap, Unsorted, custom AND/OR rules and guarded destinations.

Headless screenshots for Sort/Review/Export at 1280×800 and 1920×1080 are generated under `target/smart-sort-p2a/screenshots/`; layout tests assert widgets stay inside the modal and screenshots contain painted content. All six screenshots were visually inspected, including populated review thumbnails/confidence bars and the fixed Analyse footer.

## GPU and real weights

No GPU code or GPU tests were added: tagging/classification orchestration is CPU work and export reuses the existing pipeline. No models were downloaded and no real weights or client photos were required by tests. Existing real-weight tests remain ignored for the coordinator.

Tests use the previous Phase 1 Rust test runner, copied to `/tmp/p2-test-runner` and retained in `target/smart-sort-p2a/`, to disable software Vulkan discovery (`VK_DRIVER_FILES=/dev/null`, `VK_ICD_FILENAMES=/dev/null`). The existing `gpu_canvas_perf`, `gpu_device_loss` and `live_stroke_canvas` executables use Vulkan-only discovery and take their no-adapter skip paths; other tests can use software GL where supported. Runtime environment skips are included in Cargo's passed counts, rather than ignored counts. The existing socket-restriction guards remain intact. Hardware GPU and unrestricted socket checks need the coordinator without this runner/sandbox. The build's existing missing craft-fonts warning is unchanged; Japanese/Chinese font coverage requires that external font bundle.

No GUI windows were opened and the running desktop app/owner's files were not touched. Real-gallery accuracy, model throughput/RSS and hardware export checks remain the coordinator's spec §9 validation, requiring real weights and the appropriate hardware.

## Merge with people engine

Continued the committed checkpoint `3a138026` ("Checkpoint after the machine crash") rather than redoing the merge. The initial offline check passed for `lightcraft-engine`, `lightcraft-ui-egui` and `local-image`. The coordinator subsequently checkpointed this continuation in `053cdde8` and synchronized the working branch in `5b359301`; the required three-crate check passed again after that synchronization. This agent made no commits; the final report edits remain uncommitted.

- **Unified folders and planning:** the checkpoint's one serde-defaulted `FolderDef` retains `unsorted`, `custom`, `tags` and `combineAll`, alongside `peopleEnabled`, one stable `personIds` field, `everyone`, `peopleOr` and `useRules`. The shared planner supports ordered first-match/copy-in-each, Any/All tag rules, people AND/OR rules, duplicate-input and video filtering, sanitized names and Unsorted after all enabled conditions. `smartSort.plan` calls `smart_sort_people::prepare_folders` and returns both `folders` and `notices`, including unknown/ignored IDs and resolution of retired IDs after merges.
- **Compatibility and analysis:** presets retain `firstMatch`, `folders`, `folderPattern` and `peopleLayout`; both older JSON forms and combined roundtrips are covered. Combined `smartSort.analyze {faces:true}` calls `require_faces()` before tag inference, runs both analysis paths and reuses both caches on repeat calls. Both stores (`tag_sets`, `people`), the sessions/tokens/bursts/examples groundwork and both UI modules (`smart_sort_task`, `smart_sort_keys`) remain available.
- **Additional integration fix:** person preparation previously inspected disabled people rows, Unsorted rows and unused advanced rules, so a saved people folder could block tag-only planning after face opt-out. A new regression reproduced that error before the fix. Preparation now checks only enabled ordinary folders and their active people/rule conditions. Inactive selections no longer demand opt-in or emit unknown-ID notices; active conditions still enforce opt-in and resolve IDs. Phase 2a's reserved IDs remain harmless when the people picker is off.
- **Added coverage:** three engine regressions exercise nonempty combined tag/face analysis with opt-in, duplicate inputs and cache reuse; planning after opt-out with disabled/Unsorted/unused people conditions; and nested numeric person rules with retired/unknown IDs and inactive selections. Existing merge regressions cover legacy layouts, mixed tags/people, overlap and Unsorted. No UI behavior, dependencies, lockfile, translations, GPU code or GPU tests were added in this continuation.

Merge verification is complete: **599 passed, 4 ignored, 0 failed**, counting each engine/UI test once. The current test inventories contain 411 engine cases and 192 UI cases; every case is accounted for in `target/smart-sort-merge/coverage.json`. Filtered Smart Sort runs and repeated credits checks are included in these totals, not counted again.

| Check | Passed | Ignored | Result |
|---|---:|---:|---|
| Latest `cargo check`: engine, UI and app | | | Pass after synchronization |
| Engine `smart_sort` filter | 86 | 1 | Pass, including all three added regressions |
| UI `smart_sort` filter | 13 | 0 | Pass; four actual UI interaction tests plus nine keyboard tests |
| Full engine suite before synchronization | 407 | 3 | Pass; zero doctests |
| Engine GPU test added by synchronization | 1 | 0 | Pass through its no-adapter skip |
| Full UI coverage across interrupted/resumed runs | 191 | 1 | Pass; zero doctests |
| Latest Clippy: engine, UI and app, all targets, `-D warnings` | | | Pass |
| Latest formatting of those three crates; `git diff --check` | | | Pass |
| Required attributions regeneration | | | Pass; regenerated 690 crate entries with no diff |

The full engine run completed before the coordinator synchronized `5b359301`. That synchronization added one engine GPU test and changed GPU dispatch; the CPU pixel algorithms and Smart Sort implementation remained unchanged. The added `tests_gpu::native_tone_equalizer_layers_and_depth_use_engine_gpu_stages` test was checked separately afterward. The synchronized workspace/dependency graph also passed the required three-crate check and strict Clippy.

The first full UI run was stopped by the machine-wide OOM shutdown after 26 passing cases. Its continuation was stopped by the coordinator's synchronization after another 157 passing cases. Neither interruption was a test failure. With UI Rust sources unchanged by synchronization, the final run skipped 177 completed, unaffected cases and ran the eight unfinished cases plus six credits rechecks: **14 passed, 1 ignored, 177 filtered out**. The six rechecks cover the regenerated attribution data, including the About dialog. The union of all three runs covers every current UI test. No test was killed by the memory watchdog.

All continuation builds used offline toolchain 1.98.1, three build jobs and one Cargo build at a time. Tests used one test thread, with `VK_DRIVER_FILES`, `VK_ICD_FILENAMES` and `__EGL_VENDOR_LIBRARY_FILENAMES` set to `/dev/null` for this sandbox. The final runs used the configured memory watchdog. Commands included:

```sh
cargo +1.98.1 check --offline -p lightcraft-engine -p lightcraft-ui-egui -p local-image
cargo +1.98.1 test --offline -p lightcraft-engine smart_sort -- --test-threads=1
cargo +1.98.1 test --offline -p lightcraft-ui-egui smart_sort -- --test-threads=1
cargo +1.98.1 test --offline -p lightcraft-engine -- --test-threads=1 --nocapture
cargo +1.98.1 test --offline -p lightcraft-ui-egui -- --test-threads=1 --nocapture
cargo +1.98.1 run -q --offline -p xtask -- attributions
cargo +1.98.1 test --offline -p lightcraft-engine native_tone_equalizer_layers_and_depth_use_engine_gpu_stages -- --test-threads=1 --nocapture
CARGO_TARGET_DIR=target/clippy cargo +1.98.1 clippy --offline -p lightcraft-engine -p lightcraft-ui-egui -p local-image --all-targets -- -D warnings
cargo +1.98.1 fmt -p lightcraft-engine -p lightcraft-ui-egui -p local-image -- --check
git diff --check
```

The resumed UI commands additionally supplied `--skip` for previously completed tests; the final exact argument list is retained in `target/smart-sort-merge/ui-resume-latest-argv.json`. Validation logs, the completed-test manifests and the regression's failure before the fix are retained under `target/smart-sort-merge/`. Current inventories were listed without executing tests to verify the coverage union. No whole-workspace or GPU suite was run in this continuation.

**GPU tests added by this Smart Sort task: none.** Six existing/synchronized engine GPU cases took no-adapter skips, included in the passed count. The ignored cases are the people real-weight test and three existing timing/100k benchmarks. Hardware execution of the synchronized GPU test and real-weight people accuracy/throughput remain coordinator checks; no models were downloaded. Further Phase 2b and people UI controls remain outside this merge task; their groundwork was preserved. No required merge work remains. No GUI windows were opened, and the running app and owner's files were not touched.
