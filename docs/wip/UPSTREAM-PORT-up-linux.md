# Upstream port: up-linux (Claude)

Job: Linux desktop/UI fixes from PhotoCraft (after e5e3e397) and LightCraft (after b8df5f4).
Branch: wip/up-linux. Each fix was ported by hand onto our code (no cherry-picks).

## Items

### MUST

1. **LightCraft e2d35de (+58f7e74 test) / PhotoCraft 9b4562f2: case-insensitive file-dialog filters on Linux** - PORTED
   - New shared helper `crates/pc-ui-egui/src/dialog_filter.rs` (from upstream `apps/lightcraft/src/dialog_filter.rs`):
     `dialog_extensions_for` / `FileDialogExt::add_filter_nocase`. On Linux/BSD each extension gives `nef`, `NEF`, `[nN][eE][fF]`
     (lower case first, so a save dialog that appends the first pattern still appends `.nef`). Windows/macOS unchanged.
   - Used by File > Open (`services.rs` `open_dialog()`), File > Save/Export (`pick_save`), Library > Add Photos
     (`library_host.rs`, new `ADD_PHOTOS_EXTS`, a superset of `lightcraft_engine::import::EXTENSIONS` plus `heif`), every
     other Library dialog (presets, tracklog, curves, denoise) and the editor's own dialogs (`ai_ui.rs` ONNX, `generate_ui.rs` workflow/images).
   - 58f7e74 is an lc-engine import test (folder scan already folds case); not in my ownership, and our `import.rs`
     already lower-cases (`EXTENSIONS.contains(&e.to_lowercase())`), so nothing to port there.
   - Tests: `dialog_filter::tests::dialog_filter_*` (5, incl. a guard that ai_ui/generate_ui never call rfd's `add_filter`
     or an unparented `FileDialog::new()`), `services::tests::open_filters_cover_uppercase_and_mixed_case_extensions`,
     `services::tests::no_app_dialog_bypasses_the_nocase_filter_helper`,
     `library_host::tests::add_photos_filter_covers_import_extensions_in_any_case`. Result: pass.
2. **PhotoCraft c1cb455c: selecting type on Wayland no longer deletes it** - PORTED
   - `crates/pc-ui-egui/src/type_tool.rs` `ime_update`: an empty preedit/commit while nothing is composed changes nothing.
   - Test: `type_tool::tests::empty_ime_events_never_delete_the_selection` (direct `ime_update` calls, cheaper than the
     upstream harness test; same assertions). Existing `ime_preedit_is_replaced_by_each_update_and_commit_finalises` still passes.
3. **PhotoCraft 9cdcfbb1: startup panic when UI Font Size is not Small** - PORTED (bug confirmed in ours)
   - Ported upstream's test first: on our code it FAILED ("the installed stack reaches the second pass": `FontFamily::Name("medium")`
     unbound after frame 1, i.e. the panic path). After the fix it passes.
   - `crates/pc-ui-egui/src/theme.rs`: plugin re-armed via `with_plugin` on re-install, `defer_after_install`, `ui_fonts_active`.
   - Test: `theme::tests::saved_ui_font_size_survives_first_frame_install`.

### SHOULD

4. **PhotoCraft 3952fcef: save filters lead with the file's own extension** - PORTED (services.rs part)
   - `apps/local-image/src/services.rs` `save_filters` now returns owned filters and inserts an `EXT` filter first for an
     unlisted extension (.jpeg, .gif, .bmp, .html, .csv, .pcpresets ...). Our svg/pdf special cases kept.
   - The upstream `lib.rs` Save As suggestion change was NOT ported: ours already suggests differently
     (`saves_in_place` / TIFF / Library `-Edit.psd`), and it would change our recent Save As behaviour.
   - Test: `services::tests::save_filters_lead_with_the_files_own_extension` (new). TEST-CHANGED: see below.
5. **PhotoCraft be877c27: keep job polling on idle Linux** - PORTED
   - `crates/pc-ui-egui/src/jobs_ui.rs` new `keep_polling` (used by `tick`), called at the end of `PhotocraftApp::ui` in `lib.rs`.
   - Test: `jobs_ui::tests::a_job_started_while_drawing_requests_the_follow_up_poll`.
6. **PhotoCraft 57be0f58: Shortcuts dialog rejects malformed overrides** - PORTED
   - Our shared shortcut editor (`prefs_ui.rs` `shortcuts_body` / `confirm`) still had the pre-fix parse-or-empty code in both places.
   - Test: `prefs_ui::tests::shortcut_dialog_rejects_malformed_overrides` (upstream test, incl. the drawn-dialog part).
7. **PhotoCraft 3ef63161 / 83d5c543 / c20b81b2: autosave failures and orphans** - PORTED (except store.rs part, see Needs)
   - `crates/pc-format/src/autosave.rs`: spawn failure recorded via `note` (3ef63161); per-revision write outcomes
     (`request_checked`, `take_completed`, `RecoveryStore::autosave_checked/take_completed`) (83d5c543);
     `sweep_orphans` on `RecoveryStore::new` (bundles without sidecar, other processes' `.pcsave.tmp`, 24 h age) (c20b81b2).
   - `crates/pc-ui-egui/src/lib.rs`: `AutosaveResultsFn`, `Services::autosave_results`.
   - `crates/pc-ui-egui/src/prefs_ui.rs`: queued autosaves are "pending" until the write succeeds; a failure sets the
     status bar error and posts ONE "Autosave failed" error notice (not one per document/interval); the same revision is retried.
   - `apps/local-image/src/services.rs`: wires `autosave_checked` + `autosave_results`.
   - Tests: `autosave::tests::autosave_spawn_failure_is_reported_not_silent`,
     `autosave::tests::autosave_background_outcomes_report_failure_then_recovery`,
     `autosave::tests::autosave_sweep_removes_orphaned_bundles_and_foreign_temp_files_only`,
     `services::tests::failed_background_autosave_is_reported_and_retried_without_another_edit`.
8. **LightCraft 9c2bad1: file dialogs parented to the main window** - PORTED (rfd 0.17.2 has `set_parent`, default features xdg-portal + wayland)
   - `dialog_filter::set_dialog_parent` / `file_dialog()` (one parented template, `FileDialog` is Clone + Send); `main.rs`
     sets it from the `CreationContext` on Linux/FreeBSD only. Every dialog in services.rs, library_host.rs, ai_ui.rs,
     generate_ui.rs now starts from `file_dialog()`. Guard tests above check no unparented `FileDialog::new()` remains.
   - Not unit-testable headlessly beyond the guard; needs a manual check on KDE Wayland (fullscreen + Add Photos).
9. **PhotoCraft d29d81d5 / b3993e89: Linux libs + "no file dialog can open"** - PORTED (adapted)
   - d29d81d5: `apps/local-image/src/linux_libs.rs` `decide`: an unreadable linker cache starts silently (no false warning).
   - b3993e89 part 2: `ldconfig -p` run with `LC_ALL=C`, listing accepted when it starts with a count.
   - b3993e89 part 1: new `apps/local-image/src/file_dialog_probe.rs`. Upstream's portal check uses zbus (not a dependency
     of ours, can't add), so ours reads installed backends' `.portal` files under `XDG_DATA_DIRS` instead (no folder = can't
     tell = no message). Our dialogs are synchronous, so instead of upstream's `FileDialogAnswer::Unavailable` the app wraps
     each pick in `explain_empty`; the reason goes through `dialog_filter::report_unavailable`, and the host shows it
     (Library/Develop: error toast; Compositing: error notice).
   - Tests: `linux_libs::tests::unreadable_cache_never_warns_or_fails`, `linux_libs::tests::translated_ldconfig_listing_is_valid`,
     `file_dialog_probe::tests::file_dialog_probe_*` (3). TEST-CHANGED: see below.
10. **PhotoCraft 82c58fab / 1a9a39ea: Linux font discovery** - PARTIAL
   - 1a9a39ea PORTED: `crates/pc-text/src/cjk.rs` `with_flatpak_host_fonts` (CJK + last-resort fonts also tried under
     `/run/host/fonts`). Tests: `cjk::flatpak_tests::*` (2).
   - 82c58fab PORTED for the font database: `crates/pc-text/src/fonts.rs` `fontconfig_font_paths` (`fc-list`, no new deps)
     plus canonical-path de-duplication. Test: `fonts::fontconfig_tests::parses_unique_absolute_font_paths_from_fontconfig`.
   - 82c58fab SKIPPED for the CJK UI fallback (async Fontconfig CJK scan, `FontFile.family` -> `String`, `cjk_fonts.rs`
     plugin changes, ~200 lines): larger cross-file change; our CJK path list + the Flatpak mirror cover Fedora's Noto CJK.

### Not ported (as instructed)

- b97a72ac / d3ceaf0 async file dialogs: scope only (see final summary).
- 0ed5fcd forcing XWayland: product trade-off for the owner.
- Screen eyedropper (C deps), craft-ui / craft-print.

## Needs from other owners

- `crates/pc-format/src/store.rs` (c20b81b2, second half): in `PcraftWriter::save_dir`, after `write_atomic(&dir.join(MANIFEST), ...)`,
  make the unreferenced-object cleanup best-effort:
  replace `std::fs::remove_file(dir.join(&path))?; stats.objects_removed += 1;` with
  `if std::fs::remove_file(dir.join(&path)).is_ok() { stats.objects_removed += 1; }` (keep the `verified_directory` cache removal),
  plus upstream's test `failed_cleanup_after_commit_does_not_fail_the_save`. Without it a vanished/locked tile still turns a
  committed autosave into a reported failure (now visible to the user via item 7).

## TEST-CHANGED

- `services::tests::save_filters_lead_with_every_export_format`: `save_filters` now returns owned `(String, Vec<String>)`, so
  the comparisons were rewritten for owned values; same assertions, no weakening.
- `linux_libs::tests::library_dirs_count_and_unread_cache_only_warns` -> renamed `..._is_inconclusive`; the
  `msg.contains("Starting anyway")` assertion was removed because d29d81d5 deliberately removes that warning (the app now
  starts silently); the rest of the test is unchanged and `unreadable_cache_never_warns_or_fails` covers the new behaviour.
  `missing_message` lost its `conclusive` argument in two other tests (signature only).
- `autosave::tests::bugfix_develop_discard_returns_before_busy_writer_finishes`: struct literal gained the new `completed` field.

## i18n

- Two new user-visible strings (`tl!`): "Autosave failed" and "Crash recovery can't keep a copy of your unsaved changes.
  Save your work to keep it." `i18n::tests::every_tl_literal_is_translated` requires them, so one entry each was appended
  to the 12 complete catalogs (`crates/pc-ui-egui/src/i18n/*.tsv`, machine-quality; native review welcome).
  The file-dialog-unavailable message (app crate) stays plain English, like the other app-side messages.

## Results

- `cargo fmt` on photocraft-ui-egui, photocraft-format, photocraft-text, local-image: done.
- `cargo clippy --offline -p photocraft-format -p photocraft-text -p photocraft-ui-egui -p local-image --all-targets -- -D warnings`: clean.
- `photocraft-format --lib`: 45 passed, 0 failed.
- `photocraft-text --lib`: 96 passed, 0 failed, 3 ignored.
- `local-image --bins`: 142 passed, 0 failed, 2 ignored.
- `photocraft-ui-egui --lib`: 1460 passed, 14 failed, 48 ignored on the full run. After the i18n entries above,
  `i18n::` tests 39/39 pass. The other 13 failures are NOT caused by this job:
  - 11 fail identically on the untouched baseline pc-ui-egui (verified by temporarily restoring HEAD's crate and rerunning):
    brush_panel options_bar_edits_are_one_set_brush_per_gesture / options_bar_reaches_brush_settings_and_the_preset_library,
    rotate_view_extra_window_controls_and_text_focus_are_local, constructive_ui_contour_numeric_type_enter_and_escape,
    control ui_set_rejects_unknown_fields_before_changing_anything, toolbar46_frozen_pixel_menu_keys_and_all_chords_parse,
    toolbar46_registered_icon_alpha_and_styles_at_16_20_40, remembered_gallery_hex_glow_color_survives_success_restart_and_paint,
    draft_refine_shared_aspect_presets_lock_and_model_grid, remembered_new_recent_uses_real_create_close_and_recall_buttons,
    tool_tips every_tool_has_a_complete_entry (3 of these are already listed in docs/wip/RESUME-LINUX-V2.md).
  - `context_bar::tests::path_and_selection_ai_and_generated_layer_buttons_work`: flaky under the full parallel run (mock
    ComfyUI server timing); passes alone twice with these changes.
  - `selection_full_app_gpu_live_lasso_..._rtx5090`: hardware GPU test (egui_kittest wgpu), not part of this budget.
