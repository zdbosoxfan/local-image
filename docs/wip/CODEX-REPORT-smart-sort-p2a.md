# Smart Sort & Export — Phase 2a report

Continues the merged Phase 1 engine and faces core on this branch. No commits were made. No dependencies or Cargo.lock packages were added; the lockfile is unchanged. All implementation changes are Rust, with no unsafe code.

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

## Verification

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
