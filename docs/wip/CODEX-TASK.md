# Codex task: Save Over Original + Lightroom-style Export dialog (item 2)

Worktree of "Local Image" (Library/Develop UI: crates/lc-ui-egui; engine: crates/lc-engine). A previous builder was
stopped mid-work; its changes are in the last WIP commit ("Save Over Original and Lightroom-style Export dialog") —
continue from it (new files `crates/lc-engine/src/cmd/save_over.rs`, `tests_save_over.rs`,
`crates/lc-ui-egui/src/panels/export_dialog.rs`).

## A. "I want my 'save over original' function back for if I remove some junk from a photo and just want to save it
over the original quickly."

The old 0.7.x app had it (`git show dd46a73:backend/frontend/editor.js` ~158-159, 1177-1240: Overwrite kept the original
format, asked first with Cancel / Save Unique / Overwrite + "don't ask again"). lc-engine forbids writing over originals
(`originals.rs` OriginalGuard; test `export_never_overwrites_an_original` must keep passing for normal exports).
Implement `photo.saveOverOriginal` + Library/Develop File menu "Save Over Original…" (free shortcut) + confirm dialog
(Cancel / Save Copy Beside / Overwrite, "Don't ask again" in prefs): full-size render through the export path in the
original's format (JPEG ~95, PNG/TIFF/WebP at source bit depth) with the original's metadata and colour space; atomic
write (temp + rename) bypassing the guard only on this explicit path; keep a backup of the original in the library's
data folder (say so in the dialog); then reset the photo's develop settings, drop spots/AI patches referencing the old
pixels, and `photo.reload`. Raw/DNG originals: the action becomes "Save JPEG Beside Original" (Conflict::Unique),
imported and stacked with the raw (same import + `stack.group` as `edit_external` in cmd/convert.rs). Engine tests.

## B. "The export menu from Develop has bad UI; it should more closely match Adobe Lightroom or Capture One export
menus. Replace it with a real one; darktable's export module is a reference."

Rebuild in the **Lightroom Classic Export dialog** layout: wide resizable modal, preset list on the left (built-in +
User Presets; Add / Remove / Update), collapsible sections on the right in Lightroom's order — Export Location (Specific
folder / Same folder as original; Choose…; subfolder; Add to This Catalog [+ Add to Stack]; Existing Files:
Ask / New name / Overwrite / Skip), File Naming (template + live example filename + start number), File Settings
(format, quality, limit size, colour space, bit depth, compression), Image Sizing (W&H / Dimensions / Long / Short edge /
Megapixels / Percentage, Don't Enlarge, ppi), Output Sharpening (Screen / Matte / Glossy; Low / Standard / High),
Metadata (All / All except camera / Copyright only / None; Remove Location), Watermarking (all 9 anchors, inset, colour),
Post-Processing (Do nothing / Show in file manager / Open in other application…). Collapsed headers show a one-line
summary. Bottom bar "Export N photos" / Cancel. Engine additions only where cheap: same-folder destination, add to
catalog/stack, show in folder. Keep `ExportOptions` serde-compatible so saved presets / `last_export` load. Update
`headless.rs` export tests and add tests for presets, same-folder destination and the filename preview.

Tests: `cargo +1.98.1 test --offline -p lightcraft-engine -p lightcraft-ui-egui`.

Report file: `docs/wip/CODEX-REPORT-export.md`.

## Rules (all Codex jobs)

- "Local Image" is GPL-3.0-or-later and **all Rust**: workspace denies `unsafe`; no new dependencies, no C/C++ or
  `-sys` crates in the build (C only in out-of-build reference harnesses under `target/refvec/`).
- Offline builds (`--offline`). The PC has 60 GB RAM shared with other jobs: ONE cargo build at a time in your job,
  `CARGO_BUILD_JOBS=3`. Use `PATH=~/.cargo/bin:$PATH` and `cargo +1.98.1`. Format: `cargo +1.98.1 fmt -p <crate>`;
  lint: `CARGO_TARGET_DIR=target/clippy cargo +1.98.1 clippy --offline -p <crate> --all-targets -- -D warnings`.
- Run the tests of every crate you change (`cargo +1.98.1 test --offline -p <crate>`); all must pass. Your sandbox has
  **no GPU adapter**: GPU tests print "skipped: no GPU adapter" and pass without running — still write GPU code/tests
  and list them in your report; the coordinator runs them on the real RTX 5090.
- UI is egui 0.36; Compositing UI tests use egui_kittest, Library/Develop uses its own `headless.rs` harness. Add tests
  that actually click/drag the UI you change.
- A Local Image app may be running on the desktop: don't kill it or open GUI windows. Don't touch the owner's files
  outside this worktree (read-only access to reference data is fine).
- Old settings/documents must still load (serde defaults). The owner does not need old edits to render identically.
- **Do not commit.** Leave changes in the working tree. When done, write the report file named in your task: what's
  done per point, tests with counts, GPU tests added, anything left and why.

