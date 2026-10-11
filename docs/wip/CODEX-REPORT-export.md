# Save Over Original and Export dialog

Continued the existing `554aa9a` WIP implementation. No commits were made. Changes are confined
to `lightcraft-engine`, `lightcraft-ui-egui`, and this report; the pre-existing edit to
`docs/wip/CODEX-TASK.md` was left alone. No dependencies or unsafe code were added, and no desktop
application was opened or stopped.

## A. Save Over Original

- `photo.saveOverOriginalPlan` describes the operation without writing anything.
  `photo.saveOverOriginal` requires `confirm: true`; invalid IDs cannot fall back to another photo.
- File → Save Over Original… is available in Library/Develop, with the unused Cmd/Ctrl+Alt+S
  shortcut. Its confirmation offers Cancel, Save Copy Beside, and Overwrite, describes the backup
  directory and irreversible reset, and saves Don't ask again in app preferences. Old preferences
  still ask by default, and Settings → General can enable confirmation again.
- JPEG saves at quality 95 with the source chroma sampling. PNG/TIFF keep 8/16-bit samples;
  float TIFF keeps 32-bit samples. WebP stays 8-bit and is encoded losslessly.
- The export pipeline renders at the full cropped size. Linear float output is converted back
  into the source RGB matrix/TRC profile, retaining its ICC bytes and per-channel curves, rather
  than silently converting a nonstandard profile to sRGB. Source metadata supplements catalog
  metadata, including camera/lens serials and GPS altitude; orientation and dimensions describe
  the new rendered pixels.
- The original is durably backed up and verified in the library's `originals-backup/` folder before
  a synced temp file is renamed over the source. Backups have unique names. The command checks
  for changes to the source during rendering. Normal exports still use OriginalGuard.
- Reload updates dimensions/hash/caches; develop settings, spots/AI patch references, masks,
  history, versions and incompatible undo/redo entries are reset. Existing manually saved XMP is
  refreshed so Read Metadata cannot apply the baked edits again.
- Raw/DNG files become Save JPEG Beside Original…: a unique JPEG is imported and stacked over
  the raw. Copy Beside also uses exclusive durable publication, so an intervening file cannot be
  replaced. Formats that cannot preserve the source in place, including AVIF, use the JPEG-copy route.
- Errors keep the confirmation open and show a useful message. Confirmation preferences change
  only after a successful save. Completion feedback names the backup and remains visible.

## B. Lightroom-style Export dialog

- Wide, resizable modal with built-in and user presets on the left; Add, Remove, and Update with
  Current Settings work. Loading presets clears unfinished graphic/subfolder controls.
  The backdrop captures input outside the dialog.
- Collapsible sections on the right follow Lightroom's order and show summaries while closed:
  Export Location, File Naming, File Settings, Image Sizing, Output Sharpening, Metadata,
  Watermarking, and Post-Processing.
- Location supports a specific folder/Choose, each original's folder, a subfolder, catalog import
  and stacking, and Ask/New name/Overwrite/Skip. Ask checks original-export sidecars too, cannot
  be bypassed by another Export click or the control harness, and is also honoured by Export
  with Previous and Export with Preset. Changes to the options invalidate a pending conflict question.
- Naming has templates, token editing/help, start number and the actual engine filename preview.
  File Settings expose format, quality, size limit, colour space, depth and compression. The
  disabled JPEG size-limit field no longer accidentally enables a 1 KB limit.
- Sizing includes Width & Height, Dimensions, Long/Short Edge, Megapixels, Percentage, Don't
  Enlarge and ppi. Sharpening includes Screen/Matte/Glossy and Low/Standard/High.
- Metadata includes All/All except camera/Copyright only/None and Remove Location. Watermarks
  support text/graphics, all nine anchors, inset, opacity and colour. Post-processing supports
  nothing, reveal in the file manager, and opening in another application.
- The bottom bar shows Export N Photo(s) and Cancel. Exports retain the background worker path.
  Old presets/last-export options still load; missing size in saved options retains full-size
  semantics. Internal source-profile/metadata state is excluded from serde and presets.
  Export dialog options and the optional source data are boxed to keep ordinary dialog/options
  values small; their serialized shape is unchanged.

## Validation

All builds used `PATH="$HOME/.cargo/bin:$PATH"`, toolchain `+1.98.1`, `--offline`, and
`CARGO_BUILD_JOBS=3`, with one Cargo build at a time.

- `RUST_TEST_THREADS=3 cargo +1.98.1 test --offline -p lightcraft-engine -p lightcraft-ui-egui`:
  engine **320 passed, 2 ignored**; UI **173 passed, 1 ignored**; no failures; both doc-test
  targets have 0 tests. The final combined run includes all changes, including File → Export
  with Preset conflict handling and boxed dialog/source state.
- Additional GPU test invocation with `--nocapture`: **1 passed by skipping**, with the explicit
  `skipped: no GPU adapter` message. It does not constitute validation on GPU hardware.
- `cargo +1.98.1 fmt -p lightcraft-engine -p lightcraft-ui-egui -- --check`: passed.
- `git diff --check -- crates docs/wip/CODEX-REPORT-export.md`: passed.
- `CARGO_TARGET_DIR=target/clippy cargo +1.98.1 clippy --offline -p lightcraft-engine
  -p lightcraft-ui-egui --all-targets -- -D warnings`: passed.

The initial full UI run with unrestricted test parallelism timed out in background export and
the existing demo-grid snapshot test. Reducing test parallelism to three produced complete
passing suites without weakening those tests or their timeouts. The existing missing craft-fonts
build warning remains; font assets are outside this task.

Added/extended coverage includes JPEG crop/exposure baking, backup and replacement fault
injection, source ICC/pixel conversion/metadata/depth, reset of AI references/history/versions/XMP,
raw unique copies/stacking, same-folder import/stack, old serde options, and headless clicks on
presets, collapse headers, watermark anchors, conflict decisions, post-processing and save
confirmation. A real pointer drag checks window resizing, and real key input checks the new
shortcut. Preset/new-name examples and disabling the file-size limit are checked too.

## GPU coverage and limits

- Added `tests_save_over::save_over_source_profile_gpu_matches_cpu`: on a GPU machine it performs
  the explicit save through both renderers, checks the retained ICC profile, and compares output
  pixels (mean < 0.5 LSB, max ≤ 3 LSB). It prints `skipped: no GPU adapter` here. No GPU kernels
  changed; the coordinator should run this test on the RTX 5090.
- In-place colour preservation supports RGB matrix/TRC ICC profiles and ordinary untagged or
  container-described RGB images. LUT/CMYK/gray or invalid ICC profiles are rejected with copy
  export guidance, because the existing export encoder has no reverse CMS transform for them.
  Metadata retention uses the existing EXIF/XMP schema; opaque proprietary MakerNotes are not
  copied verbatim. A library without on-disk storage explicitly reports that its backup is in the
  system temporary directory.
- Atomic publication covers each file. A manual XMP reset failure identifies the saved photo's
  backup. Catalog persistence uses the existing `NotSaved` recovery: the reset remains applied
  in memory and queued for a later write. There is no atomic transaction spanning the photo,
  sidecar and catalog.
