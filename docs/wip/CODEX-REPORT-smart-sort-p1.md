# Smart Sort Phase 1 report

Date: 2026-10-09. Scope: Phase 1 only. Implementation and sandbox verification are complete.

## Implementation

1. **Model bundle and installation (`li-seg`).** Added `Companion`, companions on every existing model, `Group::Tagging`, and `Task::ImageText`. The CLIP entry copies the four pinned URLs, sizes and SHA-256 values from section 4.1. Installation requires every bundle file, its expected size and its checksum. Checksums are cached by path, size and modification time so the manager does not hash 607 MB every frame. Removal disposes all companion files and also cleans incomplete bundles. Face models/groups are reserved for Phase 3.
2. **CLIP and BPE (`li-seg`).** Adapted the existing Apache-2.0 tokenizer, retained its notice, added context length and zero padding, and recorded the source commit in `docs/PORTS.md`. CLIP uses shared CPU tract plans, detects i32/i64 text inputs, fixes the 77-token and 224-pixel input shapes, and normalises embeddings. Image inputs use short-edge Catmull-Rom resizing, the specified RGB mean/std, NCHW layout, centre/three crops, and normalised crop averaging. The shared cache uses weak ownership so the final caller can release the model memory.
3. **Compositing manager.** Downloads all four files sequentially with one cumulative byte counter. Retries preserve already verified files and repair invalid files; cancellation stops subsequent files. The existing removal path now removes the whole bundle. Updated download-size labels and installation summaries, companion URL allow-list coverage, and all 12 Compositing translation catalogs. Added a headless test that clicks the actual Smart Sort group open.
4. **Attributions.** Added the `ImageText` purpose mapping, taught the source scanner to ignore nested companion fields, regenerated `assets/attributions.json`, and tested that CLIP receives one model attribution with its sorting purpose. The generated diff adds the tokenizer port and CLIP model entries.
5. **Engine, storage, presets and commands.** Added `Session.smart`, the `Tagger` boundary, real CLIP wrapper and deterministic `MockTagger`. Added editable Conference, Wedding, Sports and Custom presets, user preset persistence, and defaulted `smartSort` preferences. Classification implements prompt prototypes, background softmax, sensitivity thresholds, few-shot exemplars, overlap, deterministic ties and manual overrides. Overrides also update exemplars in the saved last preset.

Implemented commands: `smartSort.status`, `smartSort.presets`, `smartSort.savePreset`, `smartSort.deletePreset`, `smartSort.analyze`, `smartSort.classify`, `smartSort.applyKeywords`, and `smartSort.plan`. Command descriptions are discoverable through the existing registry.

Embeddings load lazily from `<library>/AI/embeddings-<model>.bin`, using the specified `LIEMB1` header and little-endian records. Completed batches atomically append records through `safe_file::write_atomic_with`. Corrupt/truncated caches are ignored with a warning. A failed save retains pending records, and an idempotent retry flushes them even if every photo is already cached in memory. Sessions without a disk library use memory only. Embeddings never enter catalog snapshots or the op log.

Analysis prepares developed, uncropped 1024-pixel jobs; unedited raws prefer their oriented embedded JPEG, and offline originals can use smart previews or cached thumbnails. A dedicated half-core pool processes batches of at most 32, saving each completed batch. The public cancellation/progress interface preserves completed work. Virtual copies share content-key embeddings but retain individual assignments and keywords. Rejected photos are excluded by default (`includeRejected` opts in); videos are skipped/countable and unreadable inputs are returned in `failed`.

Keyword application validates the full assignment batch before committing one `Op::Batch` labelled “Smart Sort”. Replacement removes category descendants of the chosen parent and preserves other keywords and the parent itself. Plans use the existing catalog rule evaluator, including whole-keyword case-insensitive matching. Folder names are sanitised, nested relative names are supported, traversal is rejected, and duplicates receive deterministic suffixes.

The previous builder's `docs/specs/smart-sort.md` was preserved. Its complete task-brief spec is unchanged, and its appended section 11 owner decisions remain intact. No commits were made. No Cargo package entries were added or removed; the only lockfile change adds the already-present `serde_json` dependency to `li-seg`.

## Verification

All builds use `PATH="$HOME/.cargo/bin:$PATH"`, `cargo +1.98.1`, `--offline`, and `CARGO_BUILD_JOBS=3`. Cargo builds run sequentially.

Final total: 1,700 passed, 12 ignored, 0 failed. Runtime environment skips are described below. The final engine rerun passed (337/2), the application rebuilt successfully, workspace formatting is clean, and all-target Clippy passes with warnings denied.

| Check | Passed | Ignored | Result |
|---|---:|---:|---|
| `li-seg` tests | 14 | 2 | Pass |
| `xtask` tests | 10 | 0 | Pass |
| `lightcraft-engine` tests | 337 | 2 | Pass |
| `photocraft-ui-egui` unit + integration tests | 969 | 6 | Pass with environment skips described below |
| `lightcraft-catalog` tests | 73 | 1 | Pass |
| `lightcraft-meta` unit + integration tests | 53 | 0 | Pass |
| `lightcraft-ui-egui` tests | 177 | 1 | Pass |
| `local-image` build | | | Pass |
| `local-image` tests | 67 | 0 | Pass with the socket skip described below |
| Workspace formatting check | | | Pass |
| Changed-crate Clippy, all targets, warnings denied | | | Pass |

Attribution generation completed successfully offline. The generator reported 137 existing cache entries with missing license metadata; the resulting attribution diff contains only the two intended entries. `git diff --check` passes, and the locked package-name/version set is unchanged.

Clippy command: `CARGO_TARGET_DIR=target/clippy cargo +1.98.1 clippy --offline -p li-seg -p lightcraft-engine -p photocraft-ui-egui -p lightcraft-ui-egui -p local-image -p xtask --all-targets -- -D warnings` (also using the PATH and job limit above). Test/build logs are retained under ignored `target/test-*.log`, `target/build-app.log`, `target/clippy.log`, and `target/fmt.log`.

Added acceptance coverage includes BPE fixtures and padding/truncation; preprocessing and crop positions; mock inference plans checking both integer text types and crop averaging; bundle completeness/checksum/removal; cumulative downloads, retry and cancellation; classification, exemplars, overlap, ties, manual overrides; exact keyword undo/replacement and atomic invalid-parameter handling; library/store/preset round trips; old preferences; damaged cache recovery; atomic write failure/retry; interrupted analysis/resume; raw, offline and cached inputs; rules and folder sanitisation.

The existing Library About-tab click test now waits for the headless harness to settle after opening/changing tabs. The regenerated credits change tab content size; the anchored dialog repositions across frames, so a fixed one-frame wait left stale button positions. Every actual tab click and selected-tab assertion remains covered. Library product UI behavior is unchanged.

Rustfmt also wrapped the existing tab constants in `lc-ui-egui/src/panels/{dialogs,settings}.rs`. The required workspace-wide formatting check found six pre-existing extra blank lines in `apps/local-image/src/develop_ai.rs`; rustfmt removed them. Application tests and all-target Clippy include `local-image` as a result.

## GPU and real-weight checks

No GPU code or GPU tests were added: this phase runs inference on the CPU. This sandbox exposes a Mesa software adapter even though it has no hardware GPU. Default Vulkan discovery crashed an existing GPU test; the GL software adapter also fails existing GPU canvas expectations because it lacks required float render formats. Verification uses an ignored `target/smart-sort-test-runner` through `CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_RUNNER`. It disables Vulkan discovery for all test executables (`VK_DRIVER_FILES=/dev/null VK_ICD_FILENAMES=/dev/null`) and restricts `gpu_canvas_perf`, `gpu_device_loss` and `live_stroke_canvas` to the Vulkan backend so they take their no-adapter skip paths. Other executables can use software GL for headless egui screenshots; hardware checks that require float targets use their existing unsupported-format skip paths. The coordinator must rerun existing GPU checks on the RTX 5090 without this runner or these variables. No GUI application was launched and no model files were downloaded during verification.

Two existing Compositing tests received narrow environment guards. The mock-server context-bar test prints a skip only when socket creation returns `PermissionDenied` (sandbox networking restriction), as happened in the final run. The shortcut-capture test still executes all key-capture assertions and skips its optional screenshot only for the renderer's specific `No adapter found` panic; other panics remain failures. In the final run its screenshots used software GL. Any runtime skips are included in Cargo's passed counts, rather than its ignored counts. The coordinator should exercise the mock-server action and shortcut screenshot without these sandbox restrictions.

The existing application control-server framing test has the same socket restriction. It now skips only a `PermissionDenied` listener bind; other bind failures still fail. The coordinator should rerun `control_server::tests::oversized_reply_preserves_framing_id_and_the_next_control_request` where loopback sockets are permitted.

Two new ignored tests require the coordinator's pinned CLIP bundle:

- `clip::tests::real_bpe_golden_tokens`: checks all five reference token sequences and trailing zero padding.
- `clip::tests::real_zero_shot_reference_rankings`: checks Tetons, Earthrise and Migrant Mother against all eight section 9 prompts, for both centre and three crops.

Run with all four files under `<models-dir>/segmentation/`:

```bash
PATH="$HOME/.cargo/bin:$PATH" \
LOCAL_IMAGE_MODELS_DIR=<models-dir> CARGO_BUILD_JOBS=3 \
cargo +1.98.1 test --offline -p li-seg -- --ignored
```

The coordinator still needs the section 9 real-gallery accuracy/threshold tuning, throughput and RSS checks. These require real weights and owner-authorised reference galleries, unavailable in this sandbox. Library dialog/review/export, face recognition and SigLIP remain in their specified later phases.

## Merge with one-model-per-function

Combined the latest working branch's official/custom model APIs and per-function Settings rows with Phase 1's CLIP bundle. `MODELS` has IS-Net, PP-MobileSeg, Depth Anything V2 Small and CLIP, one official model per function. The three segmentation models have empty companions; CLIP retains all four files, cumulative download/retry, checksum caching and bundle removal. Tagging has its own row and supports only the official bundle; custom controls and custom-list activation are restricted to Subject/Sky/Depth. Preserved `CustomModelError`, UI-language error translation and all working-branch changes. Added the new unsupported-function reason to all 12 TSV catalogs.

Regression coverage checks four function rows, no custom tagging flow, refusal of custom tagging through the API/custom list, partial-bundle availability, cumulative downloads, localized reasons and cleanup preserving every CLIP file. Regenerated attributions offline, preserving the working branch's recorded license metadata missing from the local cache; relative to that branch the artifact adds only the tokenizer and CLIP entries.

Merge verification passed offline with toolchain 1.98.1, three build jobs, one Cargo build at a time and the existing sandbox test runner described above. Tests: `li-seg` 22 passed / 2 ignored; `xtask` 10 / 0; `photocraft-ui-egui` 974 / 6; `lightcraft-engine` 337 / 2; `lightcraft-ui-egui` 177 / 1; `local-image` 67 / 0. Total: **1,587 passed, 11 ignored, 0 final failures**. `every_tl_literal_is_translated`, the application build, workspace formatting and all-target Clippy for all six requested packages with `-D warnings` passed. Compositing ran with `--test-threads=1` after a parallel run exposed an existing shared `REMOVE_NOTE` race in the old-model cleanup test; all assertions remain intact. Logs are under ignored `target/merge-smart-sort/`. No lockfile packages were added or removed relative to either branch, and `git diff --check` is clean. The attribution generator reported 139 cache entries without license metadata; their existing recorded metadata was retained.

`git add` was attempted, but Git cannot create `/home/zdavidson/Documents/Local Image - Testing/.git/worktrees/codex-smart-sort/index.lock`: the Git directory is mounted read-only outside the writable worktree. File contents are resolved; the index still needs staging by the coordinator. No merge commit was made.
