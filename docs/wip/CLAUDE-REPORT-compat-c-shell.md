# Claude builder report — compat-c-shell (old-version compatibility removal, job C)

Source: `docs/wip/INVENTORY-old-version-compat.md` section C, owner decisions of Oct 14 2026.
v2 now reads only `preferences.json`; nothing from earlier Local Image / LightCraft / PhotoCraft
versions is read, migrated, aliased or forwarded by the app shell, preferences, shortcuts, toolbar,
AI settings or model paths.

## Removed / changed, by inventory row

| row | what happened |
|---|---|
| Unified prefs migration (services.rs `read_unified_preferences`) | Deleted. Replaced by `read_preferences(path)`: plain `preferences.json` read (+ AI publish). No `library-ui.json`, `mode`, `dialogs."host.libraryUi"`, `dialogs."host.module"`, AI `config.json` or `<data>/models` input. |
| `recoveredOriginal` / `migrationError` | `recoveredOriginal` gone (services + library_host). **Damaged-settings guard kept** (owner decision): a `library` section that is not an object becomes `{"unreadable": "…"}` (`services::LIBRARY_UNREADABLE_KEY`); the host then sets `blocked` and shows the Library-problem screen, never opening/creating a different Library. The exact original bytes are preserved by the existing `preference-backups/preferences.corrupt-*` recovery on the next save (KEEP path), so no copy is parked in the section. A `library` object whose path cannot be decoded still blocks via `decode_library_ui`. |
| Legacy section backups (`read_legacy_section`, `uuidless_stamp`) | Deleted. |
| First-launch durable migration (`initialize_unified_preferences`, `migrations.rejectedFields`) | Deleted; `native()` installs the AI store on `prefs_file()` only. `preferences_services_with_legacy` → `preferences_services`; `ai_preferences_backend(path)`/`install_ai_preferences_store(path)` lost the legacy arg and read the `ai` section only. |
| Migration tests (j16) | Replaced by `j16_one_store_holds_library_and_ai_and_old_stores_are_not_read` and `j16_unreadable_library_section_blocks_catalog_substitution_and_keeps_exact_bytes`. |
| Old `library-ui.json` (`read_library_prefs`, `load_prefs`, `PrefsWriter.legacy_loaded`) | Deleted; no saved `library` section → fresh default. `decode_library_ui`/`validate_library_ui` kept (current section). |
| `host.libraryUi` key | Deleted (`LIBRARY_UI_KEY`). |
| Old `mode` file (`last_module`) | Deleted; default is Library. `module_preference` kept. |
| `library_ui_value` `migrationError` | Now `settings` or the `unreadable` marker. |
| Develop-layer "legacy Library" status text | Reworded to "belongs to another Library" (2 messages). |
| Legacy AI config path, `auto_start` alias, 0.7 doc comment | Deleted/reworded (li-ai settings.rs): `legacy_config_path`, `load_legacy` gone; `load()` falls back to defaults. |
| Port 51247 rejection | Deleted: not the current control port (`LOCAL_IMAGE_CONTROL_PORT`/`--control` have no default); port 0 still rejected. |
| `LOCAL_REMOVE_DATA_DIR` | Deleted (app_dirs.rs, li-ai settings.rs); only `LOCAL_IMAGE_DATA_DIR`. |
| `LIGHTCRAFT_GPU_BACKEND` | Renamed `LOCAL_IMAGE_GPU_BACKEND`, no fallback (li-gpu backend.rs, main.rs, gpu_startup.rs comment/test name, lc-gpu lib.rs messages/docs, docs/GPU-VALIDATION.md, scripts/stress-native-tests.py). `off` still disables compute. |
| `j12_legacy_init_marker…` | Left for job D (inventory says so). |
| Rendering-mode legacy flags | `Performance.rendering_mode` is a plain `RenderingMode` (default Automatic); `use_gpu`/`useGpu` and `effective_rendering_mode` deleted; dialog, GPU-recovery choice and startup no longer write/read `useGpu`. Mode-over-CPU-backend rule in `plan_with_mode` kept (`gpuBackend: cpu` is a current choice); comment/test reworded. |
| Legacy `shortcuts` map → profiles | **Writers switched first**: Keyboard Shortcuts dialog saves `shortcutProfiles` only (profiles missing from the dialog fall back to the saved profiles, never defaults); `edit.keyboardShortcuts` and `prefs.set/reset shortcuts.<id>` write the active profile through new `Preferences::set_shortcut` / `li_keys::ShortcutProfiles::{set_override, clear_overrides, overrides}` / `ShortcutWorkspace::editable` (a built-in becomes "Custom (<built-in>)", as before). `prefs.get shortcuts[.<id>]` is a read-only view of the active profiles. Workspaces save/restore `shortcutProfiles`. Then deleted: `Preferences.shortcuts`, `ShortcutProfiles.migrated`, `migrate`, "Custom (migrated)", `adopt_legacy_changes`, the `set`/`set_unmigrated` split, whole-map `prefs.set shortcuts`, the dialog's flat-map fallback branch, library_host's `migrate` call. |
| Old mixed-case chord repair | On-load normalisation loops deleted. Input normalisation kept; a chord this build cannot parse still survives an unchanged round trip (forward-compat for newer builds' key names, like the KEEP `AiSettings.rest`). |
| Toolbar `hidden`/`order` → "My Tools" | Fields, `ToolbarCustomization::migrate` and its 4 call sites deleted. `edit.toolbar {hidden, order}` (current command) unchanged. |
| Frozen Pixel tool list | `ALL_TOOLS` kept (still the canonical Pixel order); "legacy/migration oracle" wording removed. |
| Develop legacy language adoption | Deleted with `legacy_loaded`. |
| Interface language legacy commands | `app.language.*` are live Library commands (KEEP); test renamed. `LIGHTCRAFT_LANGUAGE` left to J29 (i18n.rs not touched). |
| `pc-engine` prefs re-export of li_keys | Deleted (`GLOBAL_SHORTCUTS`, `ShortcutProfile(s)`, `ShortcutWorkspace`, `normalize_shortcut`, `conflicts`); callers use `li_keys::` directly. |
| Old segmentation models cleanup | `li_seg::{LEGACY, legacy_installed, remove_legacy_with}` and the "Remove old models" button deleted; custom IS-Net/U²-Net min–max behaviour kept, "legacy" wording removed. |
| Old SAM downloader `.part` | `remove_file` deleted (lc-fetch). |
| Saved CPU enlargement backend | Force to GPU kept, comment reworded. |
| Docs | LIGHTROOM-CLASSIC-UX.md rewritten ("v2 reads only preferences.json"); dated notes in CODEX-REPORT-j15/j20. |

KEEP rows untouched: `preference-backups/preferences.corrupt-*` recovery, crash-recovery autosaves,
`legacyCompositing`, `useLegacyFreeTransform`, `maximizePsdCompatibility`, `AiSettings.rest`.

## Checks (minimal-testing rule)
- `cargo check --offline` on all touched packages `--all-targets`, plus one `--workspace --all-targets` check for dependents: clean.
- `cargo fmt` on touched packages; strict `cargo clippy --offline -p … --all-targets -- -D warnings` on li-keys, li-ai, li-seg, lightcraft-sysmem, lightcraft-fetch, li-gpu, lightcraft-gpu, photocraft-engine, photocraft-ui-egui, local-image: clean.
- Ran only the new/changed tests by name (li-keys 1, li-ai 2, li-gpu 2, li-seg 1, lc-fetch 1, photocraft-engine 35, photocraft-ui-egui 26, local-image 10): all pass. GPU-only tests touched by a mechanical `rendering_mode` edit (`keys_profiles_gpu_screenshots_*`, `lrc_module_clicks_remain_interactive_with_the_real_gpu_canvas`, two `library_workflow_*` theme tests) were not run.

## TEST-CHANGED
- li-keys `j20_profile_json_golden_and_migration` → `j20_profile_json_golden_and_editing`: golden built by editing ("Custom (Lightroom)/(Photoshop)"), no `migrated` key; deleted-profile/no-resurrect checks kept.
- pc-engine prefs.rs `keys_profiles_migrate_legacy_and_downgrade_read` → `keys_edits_land_in_the_active_profile_and_old_flat_maps_are_ignored` (edits land in profiles, survive restart, a flat `shortcuts` map is not read).
- pc-engine `j20_legacy_settings_roundtrip_and_overlapping_transfer` → `j20_settings_roundtrip_and_overlapping_transfer` (setup through commands; transfer assertions unchanged).
- pc-engine `keys_profiles_validation_paths_names_and_atomicity`, `keys_profiles_global_cannot_be_rebound_even_by_hold_keys`: dropped `migrate` call / flat-map inserts.
- pc-engine prefs/tests.rs: `legacy_named_shortcuts_load_with_readable_canonical_labels` deleted (old-file repair); `shortcut_reset_assignment_and_whole_map_validation_are_atomic` → `…_and_profile_validation_are_atomic` (whole-map set replaced by the profile overrides path; same atomicity assertions); `dsfix_legacy_shortcut_diff_keeps_profile_only_bindings` → `dsfix_shortcut_path_keeps_profile_only_bindings`; `ds_finding_legacy_toolbar_batch_keeps_hidden_and_order` → `ds_finding_toolbar_batch_keeps_hidden_and_order` and `ds_finding_toolbar_edits_after_migration_and_unique_bindings` → `ds_finding_repeated_toolbar_edits_and_unique_bindings` (via `edit.toolbar`); `keyboard_shortcuts_reassign_and_reset`, `keyed_preference_reset…` read the profile instead of the flat map; `choice_and_range_tables_cover_enum_fields` lost the null-renderingMode exception; `gpu_backend_round_trips_and_validates` no longer loads `useGpu`.
- pc-engine toolsets.rs: `old_hidden_and_order_become_my_tools` deleted; `toolsets_default_legacy_order_and_prefs_set_migration` → `toolsets_hidden_tools_and_order_land_in_my_tools`.
- pc-engine coverage: 3 `migrate_*` tests → `edit_toolbar_hidden_creates_my_tools_without_hidden_tools`, `edit_toolbar_order_and_hidden_preserves_order_ignores_unknown_and_removes_hidden`, `default_toolbar_stays_on_all_tools`; `performance_rendering_mode_resolution` (no Option/useGpu).
- pc-engine it/rendering_preferences.rs: `old_preferences_preserve_cpu_choices` → `rendering_mode_defaults_to_automatic`; `explicit_choices_override_legacy_flags_and_round_trip` → `explicit_choices_round_trip`; malformed test now also refuses `null` and resets to Automatic.
- pc-ui-egui: `rendering_mode_display_respects_explicit_mode_and_legacy_disable` → `…_explicit_mode`; `recovery_choices_save_next_launch_mode_without_restarting` asserts the mode, not `useGpu`; `transient_initial_read_failure_rebases_local_edits_and_next_save` and `shortcut_dialog_rejects_malformed_overrides` use profiles; `shortcut_dialog_preserves_unknown_legacy_entries…` → `…_unknown_entries…` (profiles loaded, dialog given the loaded profiles as it would read them on open; also asserts the custom profile stays active); keymap_tests drop `migrated`/`migrate`/"Custom (migrated)"; `sets_round_trip_through_the_saved_prefs_and_old_toolbars_migrate` → `…_and_hidden_tools_leave_the_toolbar` and `toolsets_custom_order…` set hidden/order with `edit.toolbar`; the "Remove old models" tests (`old_models_can_be_removed_from_settings`, one line of `settings_lists_one_row_per_function`) deleted; mechanical `p.shortcuts.insert/remove/clear` → `set_shortcut`/`clear_overrides` and flat-map reads → `shortcut_overrides(..)` in 12 test files (assertions unchanged).
- local-image: j16 migration tests replaced (above); `lrc_legacy_and_unified_library_preferences_are_lenient_and_preserve_paths` → `lrc_library_preferences_are_lenient_preserve_paths_and_unreadable_settings_block` (adds the guard check); `bugfix_save_o022_bad_library_setting…` deleted (tested the removed `library-ui.json` reader; lenient path recovery is covered by the new test); `prefs_are_read_leniently` no `useGpu` (adds "backend is not the policy"); `explicit_policy_overrides_legacy_cpu_backend` → `…_cpu_backend`; `j12_shell_legacy_override…` → `j12_shell_local_image_override…`; language test renamed.
- li-ai: `round_trip_keeps_unknown_keys_and_rejects_the_app_port` → `…_rejects_port_zero`; `j16_ai_aliases_normalize…` → `ai_auto_start_comfy_normalizes…` (alias assertion dropped with the alias).
- li-seg: `old_models_are_listed_never_used_and_can_be_removed` deleted; `one_official_model_per_function` lost the `LEGACY` loop.
- li-gpu: `j12_startup_environment_preserves_shell_priority_and_legacy_compute_off` → `…_and_compute_off`; `lightcraft_variable_wins_over_wgpu_backend` → `local_image_variable_wins_over_wgpu_backend`.
- lc-fetch: `j08_damaged_files_and_old_part_files_are_replaced` → `j08_damaged_files_are_replaced`.

## Outside edits (files not in job C's ownership list)
- pc-ui-egui (job B area), needed because `Preferences.shortcuts`/`use_gpu` and the prefs re-exports are gone:
  `workspace_ui.rs` (workspaces save/restore `shortcutProfiles`; test), `gpu_status.rs` (stop writing `useGpu`; test),
  `shortcut_dispatch.rs` (`li_keys::GLOBAL_SHORTCUTS`), and tests in `shortcuts.rs`, `tool_tips.rs`,
  `shortcut_capture_tests.rs`, `shared_library_shortcut_tests.rs`, `shortcut_dispatch/tests.rs`,
  `rotate_view/tests.rs`, `ai_remove_ui/tests.rs`, `panel_visibility_tests.rs`,
  `vector_integration/review_tests.rs`, `vector_packages/persona_tests.rs`. All one-line mechanical changes except workspace_ui.
- pc-engine `tests/coverage/ds_prefs_coverage.rs` (rendering-mode test, `li_keys::` paths).
- `crates/lc-gpu/src/lib.rs` (env name in messages/docs), `crates/lc-fetch/src/tests.rs`,
  `scripts/stress-native-tests.py`, `docs/GPU-VALIDATION.md` (env rename).
- New `docs/wip/i18n-pending-compat-c-shell.md` (1 new Library string; strings now unused).

## Left, and why
- `crates/lc-ui-egui/src/i18n.rs` (`LIGHTCRAFT_LANGUAGE`, Library string lists): J29 owns it.
- `li-keys/src/legacy_tests.rs`, `li-gpu` `j12_legacy_init_marker…`, gpu_startup `j12_shell_shared_sentinel_preserves_legacy_marker_format`: job D (API-forward/parity oracles).
- `ai.auto_start_comfy` stays hidden (`HIDDEN_UNTIL_IMPLEMENTED`), comment reworded; nothing consumes it yet.
- Other `LIGHTCRAFT_GPU_BACKEND` hits are historical docs/wip records, upstream copies, recorded JSON, and `crates/lc-engine/tests/references/tagging-eval/SOURCES.md:50` (job A's crate; its command now needs `LOCAL_IMAGE_GPU_BACKEND=off`).
- Stale 51247 port mentions in docs/INSTALLATION.md, docs/LINUX-INSTALLATION.md (0.7 pages, owner's call per inventory).
- workspace_ui.rs:274 "Workspaces saved before the dock layout existed" comment: job B's KEEP row (comment only).
