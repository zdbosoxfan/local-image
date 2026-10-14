# Library safety task report

Implemented QW3 / W1 / W2, QW4 / G10, and N14 on the existing branch. Changes are uncommitted.
The owner's updated testing instruction supersedes the task's full-suite/GPU testing rule:
final validation consists of cargo check, strict clippy on the three touched crates, and
filtered runs of the new tests only. Full crate suites and final GPU acceptance are deferred
until the jobs are merged.

## QW3 / W1 / W2: safer import

- Import Photos now offers **Build previews: Embedded / Standard / 1:1 / Smart**, and remembers
  the choice in the existing import preferences. Embedded uses the existing thumbnail path;
  Standard and 1:1 use `cmd/previews.rs`; Smart uses its existing smart-preview builder.
- Preview requests retain the imported/restored IDs, wait behind a current manual preview build,
  and use the existing background builder, progress and cancellation controls. An intervening
  selection change cannot redirect the build. Requests for a previous library are discarded.
- **Make a second copy to…** uses an existing absolute backup folder, the established
  `import_move::copy_new`, and the existing content hash/byte comparison. Copies are independent
  files, durably written and verified before importing or removing a Move source. Filename
  collisions receive suffixes; original XMP sidecars follow the resolved name. Identical shared
  sidecars are accepted. Copy as DNG backs up the original before conversion.
- A failed second copy prevents that photo's import and preserves its source. Local records
  promoted into the library use the same verification path. Ordinary duplicate files remain
  skipped. The UI distinguishes second-copy failures from unreadable photos.
- Original Add/Copy/Move, batching, undo grouping, stale-library safeguards and import defaults
  were extended rather than replaced.

## QW4 / G10: catalog backup and verification

- Preferences > General provides **Never / Every quit / Daily / Weekly**, backup folder, and
  retention choices. Defaults are weekly and five copies. The backend accepts keep counts 1–100.
  The default folder is `Backups/` in the library; a chosen external folder must already exist.
- The existing quit path calls `Session::close_library`. After its completed closing snapshot,
  the catalog snapshot and log are streamed through `lc-catalog::safe_file` into a unique hidden
  staging directory. Both files are read back, compared, and replayed with the existing journal.
  A complete manifest and pair are published with one directory rename. POSIX directory-sync
  failures are propagated before retention; Windows keeps the existing rename-journal policy.
- Retention applies only to published backups with this library's existing qualified identity,
  and only after successful publication. Interrupted staging folders and other libraries'
  backups are ignored. Daily/weekly intervals use the last published backup; a recorded failed
  publication forces a retry. No new content-identity hash scheme was added.
- **Back up now** bypasses the frequency interval. A quit-time backup failure is saved in the
  library preferences and shown in the backup controls after reopening; success clears it.
- **Verify / Optimize library** finishes a background snapshot, validates the persisted pair by
  replaying it in memory without repairing the source files, and then compacts a valid catalog.
  Damage prevents optimization.
- Restore instructions are in [`docs/LIBRARY-SAFETY.md`](../LIBRARY-SAFETY.md): preserve the original
  library, copy a complete backup pair into a new restore folder, open it, verify representative
  edits and metadata, and relink originals if necessary. Catalog backups do not include original
  photos, presets, preferences or smart previews.

## N14: PDF proof sheets

- File > **Export Proof Sheet…** exports the selection in grid order, or the active photo.
  Options include A4 / Letter / A3, landscape, rows/columns, filename/title/caption/filename and
  title metadata, and an optional text watermark. Missing title/caption falls back to filename.
- Uses the existing pure-Rust krilla stack, ordinary full-quality sRGB Develop export jobs, a
  300 dpi cell target, and lossless PNG embedding. Aspect ratio is preserved. Captions and page
  numbers reuse `pc-text` shaping/font fallback and vector outlines; long captions ellipsize at
  three lines. Outlining avoids embedding restricted installed fonts.
- Rendering, encoding and PDF construction run on a named Utility worker. Existing heavy-job
  memory admission is reused; photos render sequentially. Retained PDF data is checked against
  live headroom and an oversized selection fails cleanly rather than lowering print quality.
- Cancellation leaves no published output. The shared writer publishes only a complete PDF and
  refuses an existing destination, including originals. The new dialog stays open on input errors.

## Design system, compatibility and scope

- Added documented reusable `checkbox`, `choice`, `text_field`, and `field_with_action` widgets
  in `lc-ui-egui/src/widgets.rs`. They use the active theme, labelled accessibility information,
  automation IDs and native input behavior. Fields/actions stack at narrow widths. Shared button
  and dropdown disabled/focus states now follow the existing tokens.
- Real pointer tests caught an auto-sized dialog growing between mouse-down and mouse-up.
  The shared text field now accounts for frame padding and bounds its total width; a regression
  assertion checks that the dialog's click targets remain stationary.
- New strings are on `PENDING_LIBRARY_UX_TRANSLATIONS`. Icons and the owner's AI, whole-image
  effects, SAM and Match Look decisions were untouched.
- New preferences/import fields use serde defaults. Catalog serialization and journal format
  were not changed. No new external package versions or C/-sys packages were added or removed:
  `Cargo.lock` changes only reference four existing workspace packages from `lightcraft-engine`
  (`krilla`, `photocraft-text`, `photocraft-doc`, `photocraft-geom`), needed to reuse PDF and text
  rendering instead of implementing another stack.
- The small shared persistence change is `FsStore` delegating atomic snapshot/settings writes
  to `safe_file`, removing its duplicate replacement/counter code. Verified import copies also
  delegate streamed writing/publication to that module. No separate atomic-write helper was added.
- Touched crates: `lightcraft-catalog`, `lightcraft-engine`, `lightcraft-ui-egui`. Additional shared
  documentation: `docs/DESIGN-SYSTEM.md`. No application-host changes were needed; the existing
  quit hook supplies the backup lifecycle. Pre-existing task/review/research files were preserved.
- Test data is procedural PNGs and the project's generated demo scenes. No client photos or
  Desktop contents were opened, no GUI window was launched, and the running app/services were
  not stopped.

## Final validation under the updated owner rule

`PATH=~/.cargo/bin:$PATH`, toolchain `+1.98.1`, offline, existing `target/`, and the launcher's
`CARGO_BUILD_JOBS=4` were used. Cargo commands ran serially. Runtime tests used
`LIGHTCRAFT_GPU=0`; no GPU acceptance or benchmark was run after the updated instruction.

- `cargo +1.98.1 check --offline -p lightcraft-catalog -p lightcraft-engine -p lightcraft-ui-egui`: passed.
  Log: `target/library-safety-check-final-pass.log`.
- `cargo +1.98.1 clippy --offline -p lightcraft-catalog -p lightcraft-engine -p lightcraft-ui-egui --all-targets -- -D warnings`: passed.
  Log: `target/library-safety-clippy-final.log`.
- `git diff --check`: passed.

| Crate / filter | Passed | Coverage |
| --- | ---: | --- |
| catalog: `safe_file::streaming_tests::` | 1 | Verification before publication, collision refusal, failure cleanup |
| catalog: `safe_file::store_writer_tests::` | 1 | Snapshot/settings replacements use the common fault-injectable writer |
| engine: `library_safety::tests::` | 5 | Partial writes, retention, restore equivalence, cadence/retry, missing drive, non-mutating corruption checks, persisted quit failure |
| engine: `proof_sheet::tests::` | 2 | Pagination, caption bounds, grid validation, cancellation, destination protection, interrupted output |
| engine: `safety_second_copy_move_add_collision_and_failure_keep_source` | 1 | Add/Copy/Move, independent copies, sidecars, collisions, failures and Local promotion |
| UI: `library_safety_tests::` | 4 | Real clicks for import choices and queued previews, persisted backup controls, proof export/typing, and cancel while decoding |
| UI: `safety_visual_screenshots -- --ignored` | 1 | Dark/light × 250/400 pt × 1.0/1.5 scale for all three new control groups |

**14 new CPU/UI tests plus one visual test passed.** The backup test launches its ignored child
fixture at five publication cutpoints; each process exits abruptly and the parent verifies that
previous backups survive and any published new pair is complete. The child fixture is not an
unexecuted safety case. The normally ignored visual test was explicitly run; the GPU test was
left ignored under the updated rule. Logs: `target/library-safety-test-{streaming,store,backups,pdf,import,ui,shots}.log`.

Inspected all 24 screenshots through the three QA montages in `target/library-safety/`.
All themes/widths/scales are readable, with no clipped controls. Also rendered and inspected both
pages of `proof-layout.pdf` (A4, five photos, two pages), and `proof-ui.pdf` (Letter landscape,
metadata caption and watermark). Captions, margins, aspect ratio and page numbers are correct.
Artifacts and dependency audit remain under `target/library-safety/`.

## GPU evidence and deferred validation

One ignored GPU test was added: `safety_proof_sheet_gpu_quality_and_interactive_timing`. It asserts
a real adapter, compares CPU/GPU pixels, and drives the actual proof-sheet dialog/worker path.
It was run **before the pause and revised owner rule**, on NVIDIA GeForce RTX 5090 (Vulkan):
1024×768 procedural reference, mean RGB absolute difference 0.000037 LSB, maximum 1 LSB;
UI confirm returned in 6.568 ms and the cold PDF completed in 12,321.222 ms. Historical evidence:
`target/library-safety-gpu.log` and `target/library-safety/proof-quality.txt`.
These are an earlier-revision, single-fixture observation, not final-revision or universal speed
claims. The test now requests the actual print-cell bounds; it was not rerun after the new rule.

Full crate suites, final GPU acceptance and broader performance/raw-image evaluation are deliberately
left to the merged validation, as instructed. Implementation for the requested items is complete.

## Established sources followed

- Adobe official [import options](https://helpx.adobe.com/lightroom-classic/desktop/import-photos/photo-video-import-options.html):
  preview terminology and a second original copy at import.
- Adobe official [catalog backups](https://helpx.adobe.com/lightroom-classic/desktop/manage-catalogs-and-files/back-catalog.html):
  quit cadence, folder choice, integrity/optimization, and separate original-photo backup.
- Adobe official [print layouts and photo information](https://helpx.adobe.com/lightroom-classic/desktop/print-photos/print-module-layouts-templates.html):
  contact-sheet grid, metadata captions and optional watermark.
- Local ground truth: `docs/DESIGN-SYSTEM.md`, the task's holistic-review findings/roadmap,
  `lc-engine/src/cmd/previews.rs`, `import_move.rs`, `library.rs`, `lc-catalog/src/journal.rs` /
  `safe_file.rs`, and the existing Library headless harness.
- Existing PDF/text boundary: `pc-pdf/src/export.rs`, `pc-pdf/src/export/text.rs`, `pc-text`'s
  public layout/outlines API, and the pinned krilla 0.8.2 source/API in the local Cargo registry.
  The snapshot/log directory publication protocol and physical contact-sheet geometry were
  worked out for this app's existing format; neither introduced a competing storage or rendering stack.
