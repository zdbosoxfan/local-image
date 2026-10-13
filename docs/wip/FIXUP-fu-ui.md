# Fix-ups: editor and Library UI (fu-ui, Claude)

Worktree `.claude/worktrees/fu-ui`, branch `wip/fu-ui` (base claude/v2-continue a2d123f7).
Owned: `crates/pc-ui-egui/**` (minus AI/model-picker/AI Remove files), `crates/lc-ui-egui/**`.

## Failing tests: diagnosis and fix

| Test | Diagnosis | Fix |
|---|---|---|
| brush_panel::options_bar_edits_are_one_set_brush_per_gesture | STALE TEST. 3b4604a1 turned the "Pressure for Size" switch into Photoshop's icon toggle "Always use pressure for size". | Click the icon toggle by its accessible name; every assertion kept (one `tools.setBrush` per gesture, `{"pressureSize": !was}`, journal replay). |
| brush_panel::options_bar_reaches_brush_settings_and_the_preset_library | STALE FIXTURE. The harness left `last_canvas_rect` at the origin, so the Brush Settings window opened over the options bar and swallowed the second toggle click. In the app the canvas sits below the bar. | Harness sets a canvas rect below the bar. |
| constructive::constructive_ui_contour_numeric_type_enter_and_escape | CODE BUG. With the Offset field focused, Escape is a text-field key, so `frame()` doesn't cancel. egui then drops focus and the `lost_focus` branch COMMITTED the typed offset. | `context_bar`: lost focus with Escape pressed now calls `cancel()` (discard buffer + preview), as the doc comment already promised. |
| canvas::rotate_view::rotate_view_extra_window_controls_and_text_focus_are_local (RESUME 1) | STALE FIXTURE. The doc has only a Background layer, whose Opacity/Fill spin buttons are disabled (Photoshop), so the click focused nothing. | Add a normal layer first; the no-shortcut-steal assertions are unchanged. |
| control::ui_set_rejects_unknown_fields_before_changing_anything | STALE TEST. The default tool became Move (0857b9b7), so `"tool": "move"` could no longer show a partial apply. | Start the test from Brush; assertion unchanged. |
| fill_stroke_proxy::toolbar46_frozen_pixel_menu_keys_and_all_chords_parse | STALE FIXTURE. 0857b9b7 moved Cmd+K from Edit › Search to Preferences › Settings (Photoshop). | `pixel-menu-keys.json`: drop `edit.search Cmd+K`, add `edit.preferences.general Cmd+K` in catalog order. |
| fill_stroke_proxy::toolbar46_registered_icon_alpha_and_styles_at_16_20_40 | CODE GAP. Engine `VECTOR_TOOLS` gained RotateView, but the vector icon table had no entry. | `fill_stroke_proxy/icons.rs`: RotateView → `vector-rotate` (the icon the engine registry uses) + "Rotate View" label. |
| gallery_ui::remembered_gallery_hex_glow_color_survives_success_restart_and_paint (RESUME 2) | STALE FIXTURE. The fixture sent `glowSize: 3` (integer). The f64 slider writes 3.0 back on recommit, and serde_json treats `3` and `3.0` as unequal. The values mean the same and the hex colour is kept. | Fixture sends `3.0`. All colour/recall assertions unchanged. |
| generate_ui::draft_refine::draft_refine_shared_aspect_presets_lock_and_model_grid | STALE TEST. The Properties panel behind the window also has a "Link width and height" button. | Click the link button nearest the Width field. |
| new_doc_ui::remembered_new_recent_uses_real_create_close_and_recall_buttons (RESUME 3) | STALE TEST. The window title bar also has a "Close". | Click the Close nearest the dialog's Create button; cancel/default/name assertions unchanged. |
| tool_tips::every_tool_has_a_complete_entry | CODE GAP. RotateView had a one-line placeholder tip. | Real blurb + two how-to lines (drag/Escape; View angle field / Reset View), via `tl!` + pending i18n list. |
| context_bar::path_and_selection_ai_and_generated_layer_buttons_work (flaky) | SHARED STATE. `li_ai::set_service_override` is process-wide. `generation_metadata_saved_layers_regenerate_from_context_bar` (same file) set and then cleared it WITHOUT `AI_TEST_LOCK`, so in a parallel run it could repoint or clear the flaky test's mock server mid-run. | That test now holds `AI_TEST_LOCK` (declared first, dropped last). One more unlocked user is outside my ownership (see Needs). |
| GPU psd_advanced_fallback::vector_fallback_app_real_gpu_native_reopen_preview_release_warning_click | STALE TEST (file lives under pc-io). f0558183 changed save results: the notice lines are now [path, warnings…, "Saved x"], details show under "Details / History", and every save shows a result card. | Verified patch, NOT applied (ownership). See Needs. |
| GPU psd_advanced_fallback::vector_fallback_native_width_brush_blend_repeat_envelope_full_app_gpu_shared_projection | Same as above (same helper). | Same patch. |
| GPU pdf_export_app_gpu::pdf_export_app_real_gpu_preview_release_and_menu_dialog | STALE TEST (file lives under pc-pdf). 3b4604a1 removed the visible File › Export › PDF… item (Export As owns the format chooser). Export As is enabled only with the raster exporter service, and notice lines moved to Details / History (f0558183). | Verified patch, NOT applied (ownership). See Needs. |
| lightcraft headless::full_screen_preview_and_info_overlay_by_keyboard | CODE BUG. In the Grid view the shortcut layer leaves arrows to the grid's own row-aware navigation. Full screen hides the grid, so Right did nothing (stayed on photo 21 instead of 14). | `lc-ui-egui/src/shortcuts.rs`: keep `library.next/previous` when `ui.fullscreen` (Lightroom: arrows step photos in full screen). |

### Keyboard rule (Photoshop): R = Rotate View, AI Remove in the J group
- `state.rs`: `Tool::AiRemove` key `'\0'` → `'J'`, so it joins Spot Healing / Healing / Patch / Content-Aware Move / Red Eye. Shift+J (or J with Shift-switch off) cycles to it, and its tooltip shows J.
- `shortcuts.rs::tool_shortcut`: a group member's own saved binding (e.g. an older saved `tools.select.AiRemove = R`) is what `handle()` already honours first, so it is also the key shown. This keeps "saved Remove bindings still win".
- New test `rotate_view_owns_r_and_ai_remove_cycles_in_the_j_healing_group`.

## Small items
1. **Move tool cursor:** `canvas.rs` now uses `CursorIcon::Default` (arrow) for the Move tool while hovering and during a Move drag. Several Linux cursor themes draw `CursorIcon::Move` as a large hand. Hand / Space-pan still show Grab/Grabbing. Test: `move_input::tests::move_input_native_move_and_temporary_hand_cursors` (updated, see TEST-CHANGED).
2. **puppet_ui.rs:** our pc-algo already had both algo halves (64db46f5, 3f7d8afc). UI ports:
   - PhotoCraft 64db46f5: `remesh_or_restore`. A density/expansion remesh with no triangles is refused, the previous settings and mesh come back, the control channel returns the error and the options bar snaps back. Test `an_expansion_that_empties_the_mesh_is_refused` (upstream's).
   - 3f7d8afc (UI side, ours): `move_pin` refuses a pin position whose deformed mesh box is non-finite or exceeds `MAX_WARP_PIXELS`, which the engine would refuse at Commit. The pin and live preview keep the last good state. Test `a_pin_dragged_past_the_render_budget_keeps_the_last_good_preview`.
3. **Save for Web:** no UI change. The dialog never sends `trustedSliceHtml`, and the engine strips it from the remembered settings. New test `file_ui::tests::save_for_web_dialog_escapes_slice_html_cells_and_never_remembers_trust`: a scripted trusted export first, then the real dialog. Its page has the cell escaped, and the dialog never starts with the flag.

## Needs from other owners
1. **pc-engine (R/J key table), exact fixture changes.** I did not run these two engine tests:
   - `crates/pc-engine/src/vector_toolbar_layout/pixel-tool-keys.json`: insert `["tools.select.RotateView", "R"]` right after `["tools.select.Hand", "H"]`, and change `["tools.select.AiRemove", "R"]` to `["tools.select.AiRemove", null]` (matches `prefs::TOOL_SHORTCUTS`; AI Remove's default J comes from the healing group). Fixes `vector_toolbar_layout::tests::toolbar46_frozen_pixel_tool_keys_and_query_no_journal`.
   - `crates/pc-engine/tests/fixtures/persona/pixel-toolbar.json`: insert `"RotateView"` after `"Hand"` in both `allTools` and `photographer` (matches `ALL_TOOLS`/`PHOTOGRAPHER`). Fixes `toolsets::tests::persona_pixel_snapshot_and_vector_photographer_contract`.
   - Optional wording: the `TOOL_SHORTCUTS` label "Healing Tools" could become "Healing and Remove Tools".
2. **GPU tests in pc-io / pc-pdf test dirs** (compiled only into `photocraft-ui-egui --test gpu` via `#[path]`): apply `docs/wip/FIXUP-fu-ui-other-owners.patch` (`git apply` checked clean). With it, all three listed GPU tests PASSED individually on the RTX 5090. It changes:
   - `crates/pc-io/tests/vector_fallback/app_gpu.rs`. First save: assert `warning_count == 1` and lines `[path, warning, "Saved fixture.psd"]`; the per-feature loss checks are kept. Then open Details / History, find the warning label, click "Dismiss notice", then "Close window". Second save: assert no notice carries a warning or "editability lost" (it used to assert `notices.is_empty()`; every save now shows a result card).
   - `crates/pc-pdf/tests/export/app_gpu.rs`. File › Export › "Export As…" → format combo PNG → PDF instead of the removed "PDF…" item. Stub `services.export`, since Export As is enabled only with the raster exporter and PDF itself uses pick_save/write. Open Details / History before finding the "embedded" warning label.
3. **`crates/pc-ui-egui/src/ai_remove_ui/tests.rs`** (AI Remove owner): `restoration_detected_mask_fill_once_then_optional_colourise_preview` sets the process-wide `li_ai::set_service_override` without `crate::ai_ui::AI_TEST_LOCK`. Add `let _lock = crate::ai_ui::AI_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());` as its first line. It is the other source of the context-bar flake.
4. **`docs/design/vector-tool-icons.png`**: `toolbar46_contact_sheet_original_artwork` regenerates it, now with the RotateView row. I reverted the regenerated file to stay in my lane; regenerate/commit at integration.
5. **docs/PORTS.md rows:** PhotoCraft 64db46f5 → `crates/pc-ui-egui/src/puppet_ui.rs` (UI half); 3f7d8afc → `crates/pc-ui-egui/src/puppet_ui.rs` (live-preview bound, our UI counterpart of the engine refusal).

## TEST-CHANGED
- `brush_panel_tests.rs` `options_bar_edits_are_one_set_brush_per_gesture`: clicks "Always use pressure for size" (icon toggle since 3b4604a1) instead of the old switch label. All gesture/journal/replay assertions kept.
- `brush_panel_tests.rs` `app_harness_with_language`: sets `last_canvas_rect` below the options bar (as in the app), so the Brush Settings window doesn't cover the toggle.
- `rotate_view/tests.rs` `rotate_view_extra_window_controls_and_text_focus_are_local`: adds a normal layer so the Layers spin button is enabled. Focus/no-steal assertions kept.
- `rotate_view/tests.rs` `rotate_view_default_r_saved_overrides_and_vector_object_rotate_are_distinct`: `tool_shortcut_id(AiRemove)` is now `tools.select.SpotHealing` (J group, product rule). Still asserts that the saved AI Remove key R selects AI Remove and is the key shown. Adds: with no saved key, AI Remove shows J.
- `control.rs` `ui_set_rejects_unknown_fields_before_changing_anything`: starts from Brush (default tool is Move since 0857b9b7).
- `fill_stroke_proxy/pixel-menu-keys.json`: Cmd+K now Preferences › Settings (0857b9b7).
- `gallery_ui.rs` remembered glow test: fixture `glowSize` 3 → 3.0 (same value, f64 form the slider writes).
- `generate_ui/draft_refine/tests.rs`: link button scoped to the one beside Width.
- `new_doc_ui.rs` remembered test: Close scoped to the dialog's button.
- `move_input/tests.rs` `move_input_native_move_and_temporary_hand_cursors`: Move tool expects `CursorIcon::Default` (arrow) instead of `Move` (hand on Linux), per the Photoshop UX rule. Space-pan Grab/Grabbing assertions kept.
- `context_bar_tests.rs` `generation_metadata_saved_layers_regenerate_from_context_bar`: takes `AI_TEST_LOCK` (isolation only; no assertion change).

## Final results
- `cargo fmt -p photocraft-ui-egui -p lightcraft-ui-egui`: clean.
- `cargo clippy --offline -p photocraft-ui-egui -p lightcraft-ui-egui --all-targets -- -D warnings`: clean.
- `cargo test --offline -p photocraft-ui-egui --lib`: **1478 passed, 0 failed, 48 ignored**.
- `cargo test --offline -p lightcraft-ui-egui --lib`: **317 passed, 0 failed, 4 ignored**.
- GPU (run individually, `--exact … --test-threads=1`, RTX 5090): the 3 listed tests PASS with the patch from Needs §2 applied temporarily. They still FAIL on this branch until that patch is applied.
