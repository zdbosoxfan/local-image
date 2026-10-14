# Claude builder report: compat-b-editor (old-version compatibility removal, job B)

Branch `wip/cl-compat-b-editor`. Source: `docs/wip/INVENTORY-old-version-compat.md` (Job B rows,
owner decisions of Oct 14 2026). Status: complete.

## Removed / changed, by inventory row

### Frozen old-document fixtures (care) — done first
- `pc-format/tests/fixtures/vector-v1.pcraft` + `.json` → `vector-basic.pcraft` + `.json`, current
  format 3. Produced by the existing generator `pc-format/examples/vector_arch_fixture.rs` (paths
  updated); its output is byte-identical to re-saving the old fixture, and the loaded Document was
  checked equal to the old fixture's (only `format_version` differs in the mirror).
- `pc-format/tests/fixtures/contracts/document-v2.*` → `document-v3.*`, re-frozen with the same
  hand-authoring script (`contracts_support/freeze_references.py`, now writing v3 from
  `vector-basic.json`); `golden_document` unchanged except its name ("Contract reference v3").
  The script's catalog half was left alone (job A owns those fixtures); running it reproduced the
  lc-catalog files byte-identically, so nothing there changed.
- `pc-engine/tests/fixtures/vector-v1.pcraft` → `vector-shapes.pcraft`, re-saved in format 3 with
  identical document content; the optional path fields (`fill_rule`, `inverted`, `op`, `smooth`)
  are still omitted so the test keeps covering their serde defaults.
- All 18 `include_bytes!` users swapped; SOURCES.md / README files updated (pc-format fixtures,
  contracts, pc-engine fixtures, pc-engine appearance/corners/styles, pc-ui-egui/tests/appearance,
  pc-vecfx/tests/repeat, pc-vector/tests/design_shapes, pc-vector/src/appearance).

### Native document migration chain (yes)
- Deleted `pc-format/src/migrate.rs` (`STEPS`, `v1_to_v2`, `v2_to_v3`, `migrate_with`, its tests).
- `store::read_manifest` now calls `manifest::check_format_version`: only `FORMAT_VERSION` (3)
  loads; 1–2 → new `FormatError::TooOld` ("bundle uses an older format that is not supported
  (format N; this build reads only format 3)"); newer → `TooNew` (kept); 0/missing/out of range →
  `Corrupt`. Crash-recovery autosaves use the same reader, so older autosaves are refused too.

### Version-guard comments (yes)
- manifest.rs module doc, `FORMAT_VERSION` doc, lib.rs `read_manifest` doc, pc-doc structure.rs.

### Live-group flat legacy wire (yes)
- pc-doc `LiveGroup` deserializes only the tagged `LiveGroupWire` (no untagged flat
  repeat/envelope specs).
- pc-engine `vector_packages/repeat/native.rs` (kept as the codec, per inventory): dropped the
  "old flattened WIP repeat/envelope kinds" recovery; blend/live-paint without `spec` still read
  because that is pc-doc's current tagged form.

### Develop link without `library` namespace (care)
- Kept the safe "unbound link never matches a named Library" rule.
- Removed the `develop_layer_overwrite.rs` backfill of `link.source` from the linked path.
- Reworded "legacy" in pc-doc lib.rs, develop_layer_cmds.rs, pc-ui-egui develop_layer.rs message.
  library_host.rs wording is job C's.

### PSD filter-effects layout from earlier PhotoCraft (yes → kept the reader, reworded)
- Not removed, deliberately. The Adobe layout documented at the top of `pc-psd/src/filter_effects.rs`
  ("u64 length, then until that length is used up: items") lets one section hold several items;
  the reader loop is that grammar, not a PhotoCraft-only fallback. Making it stricter could reject
  (and, through `tagged.rs` validation, fail to open) a spec-conforming PSD from another app.
  Only the "earlier PhotoCraft" wording went; the test is renamed in comment only.

### PSD `phfl` 18-byte block (care)
- Removed the `psd_export.rs` "repair a legacy 18-byte Photo Filter on re-save" branch and the
  test half that fed it an 18-byte block; removed the adjust_map test assertion that 18-byte
  blocks still import. The Photo Filter parser itself never had an 18-byte-specific path (it reads
  the 17 payload bytes, any trailing padding is ignored), so it is unchanged. The writer still
  writes Photoshop's 20 bytes, and both tests still check that.

### Camera Raw Filter (Legacy) (care → KEEP)
- Left as is: PSD Camera Raw smart filters that cannot map to `filter.develop` still land on it,
  and Photoshop's ACR curves can have more points, so `validate_changes` stays.

### Brush serde aliases (yes)
- `pc-paint/src/brush.rs`: removed `alias = "pressure_size"` / `"pressure_opacity"`.

### Brush "legacy scalar params" (care → KEEP) and gradient `colors` (care → KEEP)
- Both are current automation/UI shapes, not old files: `paint.stroke {size, hardness, …}` is
  used by current tests and tools; `paint.gradient {colors}` is sent by current UI code
  (canvas.rs, channel_view.rs, transform_tool.rs). Only the "legacy" wording was changed
  (brush_cmds.rs, paint_cmds.rs, presets/gradients.rs).

### Migrate Presets (owner: drop)
- Deleted `pc-engine/src/migrate_cmds.rs` (command, spec, its tests), `mod migrate_cmds`,
  `commands.rs` spec registration, the Edit › Presets menu row (menu_catalog.rs), the
  pc-automation denylist entry, and the now-unused "Migrate Presets" row in the 12 editor
  translation catalogs (removal only, no new strings).

### Editor command-ID aliases (owner: drop)
- Removed old IDs completely (no handler, no menu row, not invokable):
  `type.panels.character|paragraph|glyphs|characterStyles|paragraphStyles` (also their hidden
  Photoshop-catalog rows under Type › Panels; Window › Character etc. are the one location),
  `help.website` (duplicate of Help › Local Image on GitHub), `window.newWindowForDocument`
  (the control server's `ui.window.open` now runs `window.arrange.newWindowForDocument`, the
  same new-window code the menu uses), and the old export IDs `ui.svg.export`, `ui.pdf.export`,
  `file.export.pdf` (handlers in vector_integration.rs and menus.rs, catalog row "File › Export ›
  PDF…").
- Callers moved to the new design: `file.export.exportAs` now accepts `{"format": "pdf"|"svg"|…}`
  and opens Export As on that format (the unified-export test and the pc-pdf app test use it).
- **Left, blocked by file ownership:** `li.aiModels` and `edit.preferences.integrations` stay as
  the only two `MENU_ALIASES` rows (hidden duplicates of Local AI setup). Their handlers are in
  job C's files (`pc-ui-egui/src/ai_ui.rs` list + match arm, `pc-engine/src/prefs.rs`
  `section!("edit.preferences.integrations")`, `pc-ui-egui/src/prefs_ui.rs:732`). Follow-up after
  C merges: delete those handlers, then `MENU_ALIASES`, the two `UI_COMMANDS` rows in menus.rs,
  the alias part of the shortcut-label lookup in `menu_items`, and the tests in
  menus/conventions_tests.rs (`… "edit.preferences.integrations" | "li.aiModels"` block and
  `menus_ux_preferences_hide_empty_integrations_and_preserve_legacy_route`).

### KEEP rows
- Artboard moveContents/size, workspace JSON without dock layout, generate `refine_defaults`,
  vector ruler units, `measure` scale, BrightnessContrast `useLegacy`, and the whole KEEP list
  (PSD/ABR/ICC/Affinity/`normalize_legacy_groups`/`legacy_appearance`/Save for Web (Legacy)/
  autosave without sidecar/`li-color` matrix) are untouched. `channels` overlay: test renamed to
  `channels_without_options_load_with_default_options`, comment reworded.

## Found but not in the inventory (not changed)
- Remove Background engine value `"quick"` → `"fast"` (`pc-ui-egui/src/background_ui.rs`
  `deserialize_engine`, `cpu()`, the fix-up in `settings()`; also `ai_ui.rs:1320`). It is an old
  saved-value alias, but the serde attribute sits on `AiOptions` in `ai_ui.rs` (job C). Same
  follow-up as above; test `background_ui_quick_migration_and_defaults` would go with it.
- `docs/specs/vector-architecture.md:139` still names `vector-v1.pcraft` in its historical design
  text; left as history (not job B's file).
- `pc-pathops/proptest-regressions/vector-v1.txt` and `pc-engine/tests/vector_v1.rs` are named
  after the VectorCraft "V1" milestone, not the file format; unchanged (except the fixture test).
- `crates/pc-pdf/fixtures/export/bootstrap.py` (a past job's one-time patch script) still mentions
  `file.export.pdf`; historical, nothing runs it.

## Checks run
- `cargo fmt` on the 9 touched packages.
- `cargo clippy --offline --all-targets -- -D warnings` on photocraft-format, -doc, -psd, -io,
  -paint, -engine, -ui-egui, -automation, -pdf: clean (0 warnings).
- Filtered tests only (all pass):
  - photocraft-format (13): `format_version_accepts_only_the_current_version`,
    `older_native_formats_are_refused_with_a_clear_message`, `frozen_v3_native_reference_loads_exact_document`,
    `frozen_basic_native_reference_loads_without_data_loss`, `symbols_frozen_basic_fixture_loads_and_v3_writer_is_guarded`,
    `channels_without_options_load_with_default_options`, the 7 `vector_foundations` tests.
  - photocraft-doc / -paint / -psd (5): `live_group_tags_roundtrip_and_refuse_untagged_flat_specs`,
    `presets_render_and_round_trip` (+1 name match), `reads_several_items_in_one_section`, `reads_one_section_per_item`.
  - photocraft-io (2): `photo_filter_golden_blocks`, `photo_filter_psd_is_four_byte_aligned_and_round_trips`.
  - photocraft-engine (32): every regenerated-fixture user (appearance, colours, corners, drawing,
    envelope, layout, mesh, path_effects, repeat, width), both repeat codec tests,
    `overwrite_preservation_keeps_sources_history_snapshots_and_library_baseline`,
    `checked_in_shapes_fixture_with_missing_defaults_loads` (plus name-matched neighbours).
  - photocraft-ui-egui (9): `unified_export_format_param_shares_one_modal_and_one_visible_menu_entry`,
    `menus_ux_type_panels_have_one_location_and_toggle`, `menus_ux_order_geometry_and_verified_duplicates`,
    `menus_ux_active_editor_profile_labels_and_swatches_tab_are_retained`, `link_commands_open_their_urls`,
    `menu_toggles_panels`, `window_panels_select_dock_tabs`, `window_character_opens_its_own_group_and_keeps_properties`.
- Not run: styles `native_acceptance`/styles tests (same fixture, already shown to load an identical
  Document), full suites (owner rule).

## Tests changed (TEST-CHANGED)
- pc-format `migrate.rs` tests deleted with the file (`chained_steps_run_in_order`, `partial_chain_from_middle`,
  `too_new_and_invalid_versions`, `symbols_v1_migration_preserves_every_document_field`,
  `symbols_simulated_v1_reader_refuses_v2…`, `independent_group_semantics_are_rejected_by_simulated_v2_reader`,
  `current_version_is_noop`). The TooNew / 0 / missing / current checks moved to the new
  `format_version_accepts_only_the_current_version`, which also checks that 1–2 are refused.
- contracts_native.rs: `frozen_v2_native_reference_loads_exact_document` → `frozen_v3_…` (asserts the
  fixture is the current version instead of the frozen literal 2); `frozen_v1_native_reference_migrates_without_data_loss`
  → `frozen_basic_native_reference_loads_without_data_loss` (same assertions); new
  `older_native_formats_are_refused_with_a_clear_message`.
- Renamed only (same assertions, fixture swapped): convert.rs `symbols_frozen_basic_fixture_loads_and_v3_writer_is_guarded`,
  vector_foundations `frozen_basic_documents_load`, `vector_foundations_empty_fields_are_absent_in_output`,
  pc-engine `checked_in_shapes_fixture_with_missing_defaults_loads`, manifest `channels_without_options_load_with_default_options`.
  The 12 pc-engine vector-package tests keep their names; they now load the current-format plain fixture.
- pc-doc live.rs: `live_group_tags_roundtrip_and_read_legacy_flat_specs` → `…_and_refuse_untagged_flat_specs`;
  the flat inner spec must now fail to parse (was: parse to the same group).
- pc-engine repeat/native.rs: `repeat19_native_codec_recovers_legacy_flattened_tags_and_rejects_bad_parameters`
  → `repeat19_native_codec_reads_pc_doc_tags_and_rejects_flattened_or_bad_parameters`; added a
  complete flattened `grid` record to the must-fail list.
- pc-engine develop_layer_jobs_tests.rs `overwrite_preservation_…`: removed the setup line that
  cleared `link.source` ("supported legacy path-backed link"); all assertions unchanged.
- pc-io adjust_map.rs `photo_filter_golden_blocks`: removed the assertion that an 18-byte block still
  imports. psd_export.rs `photo_filter_psd_is_four_byte_aligned_and_round_trips`: removed the
  18-byte import-and-repair half.
- pc-paint brush_tests.rs `presets_render_and_round_trip`: partial JSON uses `pressureSize` instead
  of the removed `pressure_size` alias.
- pc-engine `migrate_cmds.rs` tests deleted with the command (owner decision).
- pc-ui-egui: `unified_export_legacy_aliases_share_one_modal_and_one_visible_menu_entry` →
  `unified_export_format_param_shares_one_modal_and_one_visible_menu_entry` (opens SVG/PDF via
  `file.export.exportAs {format}`); `menus_ux_verified_type_panel_aliases_have_one_location_and_identical_effects`
  → `menus_ux_type_panels_have_one_location_and_toggle` (asserts old IDs are not live, canonical
  rows keep one location, shortcut and toggle); `menus_ux_order_geometry_and_verified_duplicates`
  now asserts the removed IDs are gone instead of live; `menus_ux_active_editor_profile_labels…`
  sets its override on `window.arrange.newWindowForDocument`; `link_commands_open_their_urls` drops
  `help.website`; `menu_toggles_panels`, `window_panels_select_dock_tabs`,
  `window_character_opens_its_own_group_and_keeps_properties` use `window.panel.*` (the last one no
  longer checks the "always show" Type › Panels variant, which is gone).
- pc-pdf tests/export/app_gpu.rs: opens PDF export via `file.export.exportAs {"format":"pdf"}`
  (this file is not part of any test target, so it is not compiled).

## Outside edits
- `crates/pc-pdf/tests/export/app_gpu.rs` (one line; pc-pdf is not listed for any job).
- SOURCES.md in pc-vecfx/pc-vector/pc-ui-egui tests (listed for job B in the inventory).
- Nothing in apps/, lc-*, li-*, pc-engine/src/prefs*.rs, toolsets.rs or the job-C pc-ui-egui files.
