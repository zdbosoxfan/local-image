# Upstream port: up-library (LightCraft Library/catalog/metadata fixes)

Builder: Claude. Branch: wip/up-library. Upstream: lightcraft.git (fork point b8df5f4).

## Items

### 1. 87cd873 XMP merge: refuse repeated attributes in linear time — PORTED
- Files: crates/lc-meta/src/xmp_merge.rs (`scan()` uses `attributes().with_checks(false)` + per-element HashSet refusing repeats;
  the geometry-rewrite property check also uses `with_checks(false)` since `scan()` already validated every element).
- Tests (upstream's): `xmp_merge::tests::many_attributes_scan_in_linear_time`, `xmp_merge::tests::refuses_a_repeated_attribute` — pass.

### 2. 0a474cc permanent delete of several photos as ONE consistent op list — PORTED (delete part)
- Files: crates/lc-catalog/src/lib.rs (new `Catalog::delete_photos_permanently_ops(&[PhotoId])`, dedups ids, keeps id order),
  crates/lc-engine/src/cmd/library.rs (`photo.deletePermanently` uses it), crates/lc-ui-egui/src/menus.rs (Remove from Library /
  Empty Recently Deleted confirm path now commits one op instead of N per-photo commits + merge_undo; it was already correct but
  wrote N journal entries).
- The import "restored photos join album/selection" half of 0a474cc depends on upstream's onDeleted import modes (see item 9).
- Tests: lc-catalog `tests::deleting_several_photos_of_one_album_and_stack_permanently_leaves_nothing_dangling` (two photos in
  one album + one stack; a 3-photo stack losing two; undo restores exact snapshot); lc-engine
  `tests_organize::deleting_several_photos_of_one_album_and_stack_permanently_leaves_nothing_dangling` (through the command,
  undo/redo, journal replay) — pass.

### 3. 790a341 (walker part) bounded folder walk for import — PORTED
- Files: new crates/lc-engine/src/walk.rs (upstream `walk::files_in` + `Limits` {32 levels, 1,000,000 entries}, iterative,
  file type from the directory listing, links to folders followed once via canonical path), crates/lc-engine/src/lib.rs
  (`pub mod walk;`), crates/lc-engine/src/import.rs (`expand()` → `expand_within()` using it; logs when truncated). `expand()`
  also backs `cmd/browse.rs` folder browsing, so that is bounded too.
- Not ported (optional parts): launch-arg `launch_path` / drive-root refusal (apps/local-image, not owned), and moving preset/LUT
  folder walks / MCP `expand_paths` onto the walker.
- Tests: upstream `walk::tests::{files_come_in_path_order_without_hidden_ones_or_the_skipped_folder, deep_and_wide_trees_stop_at_the_limits, link_loops_end}`,
  `tests_import::expand_stops_at_the_walk_limits`, new `tests_import::expand_ends_on_symlink_loops` (two links up the tree) — pass;
  existing `recursive_import_with_duplicates`, `browsing_a_folder_lists_its_photos_without_adding_them` still pass.

### 4. cdca2b9 + cd8dfa7 keyword rename/merge by hierarchy levels, not bytes — PORTED
- Files: crates/lc-catalog/src/keywords.rs (`reparent` now level-based and `pub`), crates/lc-engine/src/cmd/keywords.rs
  (`keyword.rename` / `keyword.merge` filter-follow uses `reparent` instead of byte slicing).
- Tests: lc-catalog `keywords::tests::renaming_keeps_the_levels_below_whatever_the_case` (ẞ/ß, İ, merge); lc-engine
  `tests_libops::keyword_filter_follows_a_rename_whatever_the_case` (ẞẞA/ßßa, İzmir) — pass.

### 5. e1267a7 modal dialogs block background input — PORTED (keyboard part)
- Checked against our host-modal mechanism: `host_modal` covers Camera Raw host dialogs only; app dialogs (Export, Rename,
  Copy Settings, …) only blocked shortcuts for SmartSort. Pointer input was already blocked by our click-absorbing
  `dialog-dim` backdrop, so upstream's `prepare()` / modal-layer part was not needed and not ported.
- Files: crates/lc-ui-egui/src/shortcuts.rs (`handle` returns while any `app.ui.dialog` is open — supersedes the SmartSort-only
  check), crates/lc-ui-egui/src/shortcuts_catalog.rs (`consume_grid` arrow keys also blocked under a dialog).
  Side effect fixed: Esc in a dialog in Detail view also fired Back to Grid.
- Test: new crates/lc-ui-egui/src/tests_dialogs.rs `modal_dialogs_block_background_shortcuts_and_keep_escape` (Copy Settings +
  Rename dialogs; 5 / ArrowRight / P blocked; Esc closes only the dialog; shortcuts work after) — pass; verified it fails
  without the fix.

### 6. 2c85203 dropped photos + XMP sidecars — PORTED
- Files: crates/lc-ui-egui/src/lib.rs (new `split_presets`; same-folder `.xmp` whose stem matches a dropped photo's stem or
  full name is a sidecar and skipped; lookup via HashSet instead of upstream's Vec scan).
- Test (upstream's): `drop_tests::photo_sidecars_are_not_presets` — pass.

### 7. 7ba1470 + 11e36bf + 4266b0a + a08d0f6 capture-time shift / "in the last N" overflow — PORTED
- Files: crates/lc-catalog/src/dates.rs (`shift_iso` uses `checked_add`, `None` outside years 0000–9999),
  crates/lc-catalog/src/rules.rs (`last_secs`: zero/negative/NaN → None, capped at 10,000 years; an "ever" span before year 0
  makes inLast true / notInLast false instead of comparing against ""), crates/lc-engine/src/cmd/manage.rs
  (`photo.setCaptureTime`: refuses |shift| > 10,000 years or NaN; refuses the whole batch when a readable date would leave
  0000–9999; impossible anchor calendar dates refused before shifting others).
- Tests: lc-catalog `dates::tests::calendar` (extended with hostile shifts), `rules::tests::in_the_last_survives_hostile_counts`;
  lc-engine `tests_libops::capture_time_shift_is_bounded`, `tests_libops::capture_time_invalid_calendar_anchor_changes_nothing`
  (adapted: compares undo/redo/log lengths) — pass. Existing `capture_time_set_shift_undo_and_replay` still passes.

### 8. db7e378 Remove Location also for Original/DNG exports — PORTED
- Files: crates/lc-engine/src/export.rs (`photo_for_sidecar` also strips GPS/sublocation/city/state/country — and so the
  `lc:location` — when `remove_location`, so the sidecar and the DNG packet carry no location; `Work::File` gets
  `remove_location` and refuses copying an original that itself embeds GPS). Equivalent to upstream's
  `sidecar_packet_with`, without changing sidecar.rs's API.
  crates/lc-ui-egui/src/panels/export_dialog.rs (UX: the Remove Location Info checkbox is now shown for Original/DNG too and
  the collapsed-section summary mentions it, so a setting carried over from a JPEG export is never a hidden cause of the
  refusal).
- Test: lc-engine `tests_export::remove_location_holds_for_original_and_dng_exports` (sidecar, DNG packet, GPS-embedding JPEG
  refused) — pass; existing `original_export_copies_the_file_and_writes_its_edits_beside_it`,
  `export::ds2_original_export_borrows_photo_unless_people_keywords_are_requested` still pass.

### 9. 3285e02 + 2928880 re-import of a photo in Recently Deleted restores it — PORTED (restore/skip; `fresh` mode not ported)
- Engine: crates/lc-engine/src/import.rs (`OnDeleted {Restore (default), Skip}`, `ImportOptions::on_deleted`,
  `ImportReport::restored`, `Duplicate::existing_deleted`; `commit_prepared` restores a trashed duplicate with
  `Op::SetDeleted{false}` in the same undoable batch, relinking it when matched by content and its own file is gone; never
  for `local` browsing; commit label "Restore N Photos" when only restores), crates/lc-engine/src/cmd/library.rs
  (`onDeleted: restore|skip` param + docs; `import_batch_done` puts restored photos in the named album; `library.import`
  selects them and labels the undo step; Auto Import scan passes `onDeleted: skip`), crates/lc-engine/src/cmd/browse.rs
  (Skip), crates/lc-engine/src/cmd/missing.rs (`relink_op` now `pub(crate)`).
- UI: crates/lc-ui-egui/src/import.rs (review: trashed duplicates marked via `set_trashed`, checked by default, badge
  "Will be restored", summary counts them apart; UI Auto Import tasks pass `onDeleted: skip`; finish toast "Restored N
  photo(s) from Recently Deleted"; the restored photo is selected).
- Catalog identity model: no catalog-level change; restore is an ordinary SetDeleted/Relink op in the import's batch, so
  Library undo isolation and the import's library-generation checks apply unchanged.
- Not ported: `onDeleted: fresh` (purge trashed record + import afresh) and its review choice combo; new UI strings use plain
  `format!` (English) instead of `tr_format!`, which would require entries in every locales/*-formats.json.
- Tests: lc-engine `tests_import::{importing_a_deleted_photo_again_brings_it_back, importing_a_deleted_photo_from_its_new_place_restores_and_relinks_it, browsing_and_skip_never_restore_a_deleted_photo}`;
  lc-ui-egui `import::trashed_review_tests::{trashed_duplicates_are_checked_to_be_restored, auto_import_tasks_skip_deleted_photos}`,
  `headless::tests::readding_a_deleted_photo_shows_it_in_recently_deleted` (TEST-CHANGED, see below) — pass.
- lc-engine/tests/coverage/ds_import_coverage.rs: struct literals given the two new fields (compile fix only).

### 10. 5a21fc9 import re-probes a file gone/changed since the review — PORTED (ours did not have it)
- Our `ImportJob::prepare_files` reused every cached review probe unconditionally (Copy/Move verified bytes against the probe
  hash, but Add mode catalogued a vanished file). Files: crates/lc-engine/src/import.rs (cached probe used only while the
  file exists with the same size; native only).
- Test (upstream's): `tests_import::a_file_gone_since_the_import_review_is_not_imported` — pass.

### 11. 4b57d5d + 8dd3afc lc-fetch token scoping / validation — PORTED (4b57d5d fully; 8dd3afc URL part only)
- Files: crates/lc-fetch/src/lib.rs (`Options::token_env` (default None) replaces the hard-coded `LIGHTCRAFT_SAM3_TOKEN`;
  `bearer()` decides: non-empty, https, first host only; attempts clamped to 1..=16), crates/lc-fetch/src/http.rs (URLs over
  16 KiB refused; parse errors no longer echo the URL, whose query may hold a token), crates/lc-engine/src/segment/mod.rs
  (SAM 3 download names `LIGHTCRAFT_SAM3_TOKEN`, so behaviour is unchanged for users; upstream did this via a new
  `lightcraft_segment::fetch::options()`, but lc-segment is not owned here, so the engine sets the field).
- Not ported from 8dd3afc: `validate_files`, `download_url`, `FileSpec<'a>` lifetimes — they serve upstream's user-editable
  model list, which we don't have (our file specs are static SAM 3 constants).
- Tests: `tests::a_token_is_sent_only_over_https_to_the_first_host`, `http::tests::rejects_oversized_and_control_character_urls` — pass.

## Final results

- `cargo fmt` on lightcraft-meta, -catalog, -engine, -ui-egui, -fetch: clean.
- `cargo clippy --offline -p lightcraft-meta -p lightcraft-catalog -p lightcraft-engine -p lightcraft-ui-egui -p lightcraft-fetch --all-targets -- -D warnings`: clean.
- `cargo test --offline --lib`: lightcraft-meta 62 passed; lightcraft-catalog 81 passed (1 ignored); lightcraft-fetch 15 passed;
  lightcraft-engine 495 passed (4 ignored); lightcraft-ui-egui 316 passed, 1 failed (4 ignored) — the failure is
  `headless::tests::full_screen_preview_and_info_overlay_by_keyboard` (Right in full screen lands on photo 21 instead of 14),
  which fails identically on the unmodified base (verified by reverting the whole diff and rerunning): PRE-EXISTING, not caused
  by this port.
- Integration tests touched: lightcraft-engine `--test coverage` ds_import (20 passed); lightcraft-catalog `--test coverage`
  (111 passed, covers the changed `shift_iso`).
- `cargo check -p lightcraft-engine --features sam` fails on the base with a pre-existing duplicate `apply_outcome`
  (enhance/session.rs:78 vs segment/mod.rs:593), so the one-line SAM 3 `token_env` call in segment/mod.rs could not be
  compile-checked under that feature (rustfmt parses it; the field exists on the re-exported `lightcraft_fetch::Options`).

## TEST-CHANGED
- crates/lc-ui-egui/src/headless.rs `readding_a_deleted_photo_shows_it_in_recently_deleted`: it encoded the old behaviour
  (re-adding a trashed photo's file is skipped and the trash view opened). Upstream 3285e02 changes that on purpose: the
  re-add now restores the photo (asserted: not deleted, no second record, selected in All Photos, toast "Restored 1 photo from
  Recently Deleted"); the test then undoes and opens Recently Deleted itself before its unchanged menu/filmstrip checks.
- crates/lc-engine/tests/coverage/ds_import_coverage.rs: struct literals gained `restored` / `existing_deleted` (no assertion
  changed).

## Needs from other owners
- None required. Optional: apps/local-image could adopt the drive-root/home-folder refusal for launch arguments
  (`launch_path` from upstream 790a341) — not ported, apps/local-image not owned.
- Note for the engine/SAM owner: the pre-existing `--features sam` compile error above.

## Upstream shas ported (for docs/PORTS.md)
| Upstream sha | Our files |
|---|---|
| 87cd873 | crates/lc-meta/src/xmp_merge.rs |
| 0a474cc (delete part) | crates/lc-catalog/src/lib.rs, crates/lc-catalog/src/tests.rs, crates/lc-engine/src/cmd/library.rs, crates/lc-engine/src/tests_organize.rs, crates/lc-ui-egui/src/menus.rs |
| 790a341 (walker part) | crates/lc-engine/src/walk.rs (new), crates/lc-engine/src/import.rs, crates/lc-engine/src/lib.rs, crates/lc-engine/src/tests_import.rs |
| cdca2b9, cd8dfa7 | crates/lc-catalog/src/keywords.rs, crates/lc-engine/src/cmd/keywords.rs, crates/lc-engine/src/tests_libops.rs |
| e1267a7 (keyboard part) | crates/lc-ui-egui/src/shortcuts.rs, crates/lc-ui-egui/src/shortcuts_catalog.rs, crates/lc-ui-egui/src/tests_dialogs.rs (new), crates/lc-ui-egui/src/lib.rs |
| 2c85203 | crates/lc-ui-egui/src/lib.rs |
| 7ba1470, 11e36bf, 4266b0a, a08d0f6 | crates/lc-catalog/src/dates.rs, crates/lc-catalog/src/rules.rs, crates/lc-engine/src/cmd/manage.rs, crates/lc-engine/src/tests_libops.rs |
| db7e378 | crates/lc-engine/src/export.rs, crates/lc-engine/src/tests_export.rs, crates/lc-ui-egui/src/panels/export_dialog.rs |
| 2928880, 3285e02 (restore/skip, no `fresh`) | crates/lc-engine/src/import.rs, crates/lc-engine/src/cmd/library.rs, crates/lc-engine/src/cmd/browse.rs, crates/lc-engine/src/cmd/missing.rs, crates/lc-engine/src/tests_import.rs, crates/lc-ui-egui/src/import.rs, crates/lc-ui-egui/src/headless.rs |
| 5a21fc9 | crates/lc-engine/src/import.rs, crates/lc-engine/src/tests_import.rs |
| 4b57d5d, 8dd3afc (URL part) | crates/lc-fetch/src/lib.rs, crates/lc-fetch/src/http.rs, crates/lc-fetch/src/tests.rs, crates/lc-engine/src/segment/mod.rs |
