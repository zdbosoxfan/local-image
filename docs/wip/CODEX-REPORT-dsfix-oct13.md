# DeepSeek findings — October 13, 2026

All 47 findings were re-checked against this branch and are real. Changes are uncommitted. The pre-existing task document and confirmed-findings list were retained. No Match Look or vendor source was edited, no dependencies were added, and no GUI app was opened or killed.

## Findings

| ID | Re-check | Fix | Regression / validation |
| --- | --- | --- | --- |
| F019 | REAL | Refuse Original copies with embedded GPS or nonblank place names; parse IPTC-IIM place datasets (suggested extraction-only fix was insufficient). | `dsfix_original_export_refuses_textual_xmp_and_iptc_locations; dsfix_original_export_rejects_embedded_places_without_gps; dsfix_iptc_iim_location_fields_are_extracted_for_export_privacy` |
| F055 | REAL | Clear pending gradient/origin on click, tool change, Escape and Done; reject pending edits outside linear/radial. | `dsfix_pending_gradient_click_and_tool_switch_do_not_convert_shape (headless canvas click/drag)` |
| F100 | REAL | Apply only changed/removed legacy bindings to the active profile. | `dsfix_legacy_shortcut_diff_keeps_profile_only_bindings` |
| F155 | REAL | Cache models directory by config path/default directory; explicit settings loads and successful saves refresh it. Set subject quality on change only. | `dsfix_model_dir_cache_avoids_frame_reads_and_refreshes_after_save; dsfix_subject_quality_refresh_is_change_driven` |
| F187 | REAL | One profile_defaults helper resolves edited editor/library profiles for both display and OK conflict checks. Saved chords are rechecked when the parent profile changes. | `dsfix_shortcuts_ok_uses_selected_capture_one_defaults (profile dropdown clicks)` |
| F223 | REAL | Keep layouts restored by sync_persona and retain custom layouts when selecting Vector again; explicit reset remains separate. | `vectorws_workspace_and_personas_restore_custom_layouts_after_restart (extended)` |
| F001 | REAL | Unix executable detection requires regular file and executable permission bits. | `file_dialog_probe_finds_programs_and_portal_backends (extended)` |
| F005 | REAL | Continue past failures in each colord device proxy, Metadata, Enabled, profile proxy or Filename read. | `Existing monitor_profile::linux tests; no isolated D-Bus hot-unplug regression (requires a synthetic bus/service)` |
| F017 | REAL | Count imports plus restores in both the command merger and commit_prepared label; a single undo step is not relabeled by merge_undo. | `dsfix_mixed_import_restore_undo_counts_all_photos (failed with Add 1 Photo before commit-label fix)` |
| F020 | REAL | Timestamp probes around file reads; require matching size and known mtime for both scan and import cache reuse. Copy/move also retain and check the reviewed content hash, rejecting changed files. | `dsfix_cached_scan_reprobes_same_size_file_after_edit; copy_import_refuses_a_file_changed_since_the_review` |
| F038 | REAL | Append Background masks after stacked masks in scene-stage finishing, matching the CPU path. | `dsfix_scene_finish_retains_background_range_overlay_after_stack; dsfix_gpu_stack_preserves_background_range_overlay (hardware)` |
| F053 | REAL | Edit panel selection clears incompatible Remove/Crop tools while retaining Edit-hosted tools. | `dsfix_edit_switch_and_enter_finish_mask_tools` |
| F054 | REAL | Enter finishes Edit-hosted brush/linear/radial interactions and clears pending gradients. | `dsfix_edit_switch_and_enter_finish_mask_tools (actual Enter input)` |
| F062 | REAL | Mixed-mask thumbnail uses the first image-dependent shape rather than the first component. | `dsfix_mixed_mask_badge_uses_image_dependent_component` |
| F071 | REAL | Propagate every warm prediction error before recording its timing. | `No new test: development-only example; checked by changed-crate all-target clippy` |
| F075 | REAL | Check checked raster byte count and available input before reserving samples. | `dsfix_binary_truncation_precedes_allocation` |
| F081 | REAL | Strip UTF-8 BOM before parsing profiles.xml. | `dsfix_profile_xml_accepts_bom_and_names_reject_invalid_xml_scalars` |
| F083 | REAL | Reject XML 1.0-invalid U+FFFE/U+FFFF names. | `dsfix_profile_xml_accepts_bom_and_names_reject_invalid_xml_scalars` |
| F092 | REAL | Parallel bounded chunks use CMS float batch conversion, then quantize with original global dither phase and alpha; retain exact float transform precision. | `dsfix_parallel_display_keeps_dither_phase_alpha_and_fractional_values; existing display_cpu_identity_high_depth_dithers_but_eight_bit_stays_exact; dsfix_cpu_canvas_interactive_display_timing (real ensure_texture path)` |
| F096 | REAL | Compute center sums in f64; saturate transformed far edges. | `dsfix_rotated_crop_has_large_positive_center_without_overflow` |
| F098 | REAL | Allocate unused positive link IDs, skipping stored groups and newly allocated IDs, instead of max+1. | `move_copy_linked_layers_is_atomic_and_has_one_journal_and_undo_step (u64::MAX preview added)` |
| F102 | REAL | Preflight a supplied non-ABR group as a nonblank string before importing. | `dsfix_non_abr_import_rejects_invalid_group_before_import` |
| F106 | REAL | Suspend unused-tip deletion while any unreadable group remains; shared hashes may still belong to that group. | `dsfix_skipped_group_keeps_tip_shared_with_deleted_group` |
| F112 | REAL | Skip pattern writes with zero covered pixels and prune transparent tiles after painting. | `dsfix_pattern_stroke_with_empty_selection_allocates_no_tiles` |
| F123 | REAL | Keep Linux RTX/Vulkan diagnostic and report hardware Metal on macOS. | `No new test: test-harness diagnostic only; compiled with all-target clippy` |
| F125 | REAL | Keep Linux RTX/Vulkan diagnostic and report hardware Metal on macOS. | `No new test: test-harness diagnostic only; compiled with all-target clippy` |
| F135 | REAL | Convert finite radians in f64 before wrapping degrees into 0..360. | `dsfix_krita_large_finite_radian_angle_stays_finite` |
| F139 | REAL | Preserve scatter/randomise angle jitter when adding Direction control. | `dsfix_procreate_direction_preserves_scatter_and_randomise (real binary archives)` |
| F146 | REAL | Use the existing 512 MiB import decode budget to skip ZIP validation inflation; preserve larger encoder limits. | `zip_over_budget_is_not_decompressed_at_parse (extended between the two budgets)` |
| F152 | REAL | Treat isolated PassThrough children (including Auto below full fill) as atomic Normal content when checking an outer group. | `dsfix_isolated_child_does_not_force_outer_passthrough_raster` |
| F156 | REAL | Dropdown updates subject quality only when the engine changes; shared directory cache prevents frame-time config reads. | `Existing background_ui_single_and_batch_click_all_four_shared_engines; shared cache/change regressions under F155` |
| F165 | REAL | Acquire the existing process GPU lock in display_output_preferences_shots. | `Existing ignored hardware screenshot test; no separate test-harness regression` |
| F166 | REAL | Close custom groups by hiding tabs while retaining placement; Window check/reopen respects hidden tabs. | `dsfix_close_custom_group_preserves_placement_and_reopens (menu click)` |
| F168 | REAL | Translate visible strip insertion positions to full tab-list positions before removing dragged tabs (floating and docked). | `dsfix_floating_hidden_tab_fallback_and_visible_drop_order (actual drag)` |
| F169 | REAL | Persist the first-visible fallback as the floating active tab. | `dsfix_floating_hidden_tab_fallback_and_visible_drop_order` |
| F172 | REAL | Validate portable filenames including Windows punctuation, trailing dots/spaces and reserved device names. | `bg_batch_template_preview_and_conflicts (extended)` |
| F196 | REAL | Accumulate all dropped palette errors/warnings; later successes retain earlier errors. | `dsfix_multiple_palette_drops_keep_earlier_error (real egui dropped files)` |
| F198 | REAL | Allow precondition checks for Transform Again and Outline Stroke without taking over their command routing; Outline requires selected stroked shapes. | `dsfix_outline_and_transform_again_menu_reasons` |
| F201 | REAL | Keep Linux RTX/Vulkan diagnostic and report hardware Metal on macOS. | `No new test: test-harness diagnostic only; compiled with all-target clippy` |
| F202 | REAL | Keep Linux RTX/Vulkan diagnostic and report hardware Metal on macOS. | `No new test: test-harness diagnostic only; compiled with all-target clippy` |
| F204 | REAL | Keep Linux RTX/Vulkan diagnostic and report hardware Metal on macOS. | `No new test: test-harness diagnostic only; compiled with all-target clippy` |
| F205 | REAL | Keep Linux RTX/Vulkan diagnostic and report hardware Metal on macOS. | `No new test: test-harness diagnostic only; compiled with all-target clippy` |
| F209 | REAL | Keep Linux RTX/Vulkan diagnostic and report hardware Metal on macOS. | `No new test: test-harness diagnostic only; compiled with all-target clippy` |
| F210 | REAL | Keep Linux RTX/Vulkan diagnostic and report hardware Metal on macOS. | `No new test: test-harness diagnostic only; compiled with all-target clippy` |
| F215 | REAL | Keep Linux RTX/Vulkan diagnostic and report hardware Metal on macOS. | `No new test: test-harness diagnostic only; compiled with all-target clippy` |
| F216 | REAL | Keep Linux RTX/Vulkan diagnostic and report hardware Metal on macOS. | `No new test: test-harness diagnostic only; compiled with all-target clippy` |
| F222 | REAL | Both tile and label are clickable with distinct accessible names; union response anchors the shared picker. Document shared control. | `dsfix_color_tile_click_opens_picker (click on tile)` |

## Sources followed

- Existing architecture and code: `docs/DESIGN-SYSTEM.md`, `docs/PORTS.md`, `docs/upstream/`, `crates/pc-cms/src/transform.rs` (`convert_f32`, precision options), `crates/pc-compose/src/lib.rs` (isolated/pass-through/fill semantics), existing docking and shortcut helpers/tests. No upstream code was copied. Routine cache, diff and error-isolation fixes follow these existing APIs; no external algorithm is needed.
- [IPTC-IIM 4.2 specification](https://www.iptc.org/std/IIM/4.2/specification/IIMV4.2.pdf): datasets 2:90/92/95/100/101.
- [Procreate Brush Studio settings](https://help.procreate.com/procreate/handbook/5.4/brushes/brush-studio-settings): Scatter/Randomized and directional Rotation are independent. Existing licensed import fixtures and plist reader supplied the format implementation.
- [Adobe Lightroom Classic masking](https://helpx.adobe.com/dk/lightroom-classic/desktop/process-and-develop-photos/masking.html) and the app's existing Done/Back behavior: finish local tool interactions without losing mask edits.
- [W3C compositing](https://www.w3.org/TR/compositing-1/) and [SVG rendering model](https://www.w3.org/TR/SVG/render.html): an isolated group does not blend its children with the outer backdrop.
- [Windows filename rules](https://learn.microsoft.com/en-us/windows/desktop/fileio/naming-a-file): prohibited punctuation, device names (including superscript digits) and trailing dots/spaces.
- [XML 1.0 valid character range](https://www.w3.org/TR/xml/#charsets) and the existing KPL reader's BOM handling; [Netpbm PPM raster specification](https://netpbm.sourceforge.net/doc/ppm.html) and the existing checked sample-count implementation. [Lightroom Classic keyboard shortcuts](https://helpx.adobe.com/lightroom-classic/desktop/introduction-to-lightroom-classic/keyboard-shortcuts.html) supplies the local-tool lifecycle reference.

## TEST-CHANGED lines

The following tests were strengthened, with no assertion weakened:

- TEST-CHANGED F001: `file_dialog_probe_finds_programs_and_portal_backends`: a regular Unix file is unavailable until executable bits are set.
- TEST-CHANGED F098: `move_copy_linked_layers_is_atomic_and_has_one_journal_and_undo_step`: Alt-drag preview includes stored `u64::MAX` link groups.
- TEST-CHANGED F146: `zip_over_budget_is_not_decompressed_at_parse`: includes the import budget and larger encoder budget.
- TEST-CHANGED F172: `bg_batch_template_preview_and_conflicts`: includes Windows-invalid characters, device names and trailing dots/spaces.
- TEST-CHANGED F223: `vectorws_workspace_and_personas_restore_custom_layouts_after_restart`: selects/reselects through Window > Workspace > Vector.
- TEST-CHANGED (existing owner decision): `strip_tool_tips_icon_pixels_follow_colour_preference_and_keep_text` assumed coloured icons by default despite `AppSettings::default` and this task requiring monochrome. It now asserts the monochrome default, explicitly enables its colour phase, and retains both colour and monochrome pixel assertions and the real settings-button click, using index 0 (the current Monochrome choice). No production icon/default behavior changed. The editor counterpart `tool_tips_icon_pixels_follow_colour_preference_and_keep_text` was corrected identically: it asserts the monochrome default and explicitly selects colour before its unchanged pixel checks.

## Commands and outputs

`PATH=~/.cargo/bin:$PATH`, toolchain `+1.98.1`, offline builds, inherited `CARGO_BUILD_JOBS=4`, and this worktree's `target/` were used throughout. Failed development logs are retained under `target/dsfix-*.log`; compiler/test integration errors were corrected without weakening tests. The changed-crate suites, final clippy output, hardware GPU run and real CPU-canvas timing are recorded below.

Focused regressions: `target/dsfix-regressions2.log` completed all 11 selected library targets; two UI harness setup errors (missing medium font and using a dock-only harness for a floating panel) were corrected and `target/dsfix-ui-regressions.log` passed all 7 selected UI tests. The full changed-crate suites below also cover strengthened existing tests that do not use the `dsfix` name.

Hardware and UI QA completed:

```text
cargo +1.98.1 test --offline --lib -p lightcraft-ui-egui dsfix_gpu_stack_preserves_background_range_overlay -- --ignored --nocapture
dsfix Background overlay adapter: NVIDIA GeForce RTX 5090 (Vulkan)
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 345 filtered out; finished in 0.37s

cargo +1.98.1 test --offline --lib -p photocraft-ui-egui dsfix_cpu_canvas_interactive_display_timing -- --ignored --nocapture
dsfix CPU canvas ensure_texture 2048x2048 f32 -> P3 median 82.315 ms; all [81.209152, 81.918604, 82.31504100000001, 83.148061, 83.634958]
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 1672 filtered out; finished in 0.51s

cargo +1.98.1 test --offline --lib -p photocraft-ui-egui display_output_preferences_shots -- --ignored --nocapture
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 1672 filtered out; finished in 0.25s
```

GPU tests added: one strict hardware overlay regression. The GPU run completed on RTX 5090/Vulkan with no adapter skip. The CPU timing measures the real app `canvas::ensure_texture` route (compositing, float display conversion and texture update), with one cold run excluded and five measured updates; it is a current responsiveness measurement, not a claimed before/after speedup. Eight hardware-rendered display-preferences images were generated under `target/design-system/`; the narrow dark/light images were inspected and show no clipping or overlapping text. No ComfyUI generation job was needed or queued.

Final lint command and output:

```text
cargo +1.98.1 clippy --offline -p li-ai -p li-seg -p lightcraft-engine -p lightcraft-meta -p lightcraft-pipeline -p lightcraft-ui-egui -p local-image -p photocraft-codecs -p photocraft-color -p photocraft-engine -p photocraft-io -p photocraft-psd -p photocraft-svg -p photocraft-ui-egui --all-targets -- -D warnings
warning: lightcraft-engine@2.0.0-alpha.1: no craft-fonts embedded: Chinese and Japanese text will render as empty boxes. Clone https://github.com/storytold/craft-fonts next to this repo and rebuild with CRAFT_FONTS_DIR=../craft-fonts
    Checking photocraft-engine v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/cx-dsfix/crates/pc-engine)
    Checking photocraft-ui-egui v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/cx-dsfix/crates/pc-ui-egui)
    Checking lightcraft-ui-egui v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/cx-dsfix/crates/lc-ui-egui)
    Checking photocraft-automation v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/cx-dsfix/crates/pc-automation)
    Checking local-image v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/cx-dsfix/apps/local-image)
    Finished `dev` profile [optimized + debuginfo] target(s) in 30.19s
```

## Full changed-crate run and recovery

The full suites of all 14 changed crates ran once. The initial combined command exited 101 with five failing tests in four targets. These exposed three causes: reviewed-copy hashes needed to survive probe refresh; the two old tooltip tests assumed coloured defaults; and the new tile and label shared an accessible name, making existing symbol-palette queries ambiguous. All were corrected; the original symbol and copy-safety tests were left unchanged, and all five failures subsequently passed. The tile now has the distinct localized accessible name `{label} colour swatch`. A further profile regression verifies that an unchanged saved chord is checked against new defaults when the parent profile changes.

To honor the testing budget, the full combined run was not repeated. All unaffected full-run tests passed, and focused rechecks covered the repaired imports, both tooltip tests, profile-default logic, tile activation, and the existing CPU/GPU symbol flows. No known failing test remains.

| Crate | Initial full-run passed | Failed (all repaired below) | Ignored |
| --- | ---: | ---: | ---: |
| li-ai | 504 | 0 | 5 |
| li-seg | 81 | 0 | 8 |
| lightcraft-engine | 895 | 1 | 6 |
| lightcraft-meta | 96 | 0 | 0 |
| lightcraft-pipeline | 421 | 0 | 3 |
| lightcraft-ui-egui | 337 | 1 | 8 |
| local-image | 153 | 0 | 6 |
| photocraft-codecs | 585 | 0 | 3 |
| photocraft-color | 31 | 0 | 0 |
| photocraft-engine | 2116 | 0 | 33 |
| photocraft-io | 510 | 0 | 3 |
| photocraft-psd | 405 | 0 | 0 |
| photocraft-svg | 82 | 0 | 0 |
| photocraft-ui-egui | 1753 | 3 | 80 |

Exact full-run command and result output (every test binary and doctest summary; complete per-test output is retained in `target/dsfix-final-test.log`):

```text
cargo +1.98.1 test --offline -p li-ai -p li-seg -p lightcraft-engine -p lightcraft-meta -p lightcraft-pipeline -p lightcraft-ui-egui -p local-image -p photocraft-codecs -p photocraft-color -p photocraft-engine -p photocraft-io -p photocraft-psd -p photocraft-svg -p photocraft-ui-egui --no-fail-fast
    Finished `test` profile [optimized + debuginfo] target(s) in 6m 23s
     Running unittests src/lib.rs (target/debug/deps/li_ai-41b3b046cc7ceff2)
test result: ok. 184 passed; 0 failed; 3 ignored; 0 measured; 0 filtered out; finished in 62.78s
     Running tests/coverage/main.rs (target/debug/deps/coverage-7215dae4863971be)
test result: ok. 319 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 0.64s
     Running tests/real_generation.rs (target/debug/deps/real_generation-1be488c2db61be59)
test result: ok. 0 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 0.00s
     Running tests/remote.rs (target/debug/deps/remote-6964c494b48da4e1)
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.26s
     Running unittests src/lib.rs (target/debug/deps/li_seg-995930d58e18f555)
test result: ok. 80 passed; 0 failed; 8 ignored; 0 measured; 0 filtered out; finished in 0.55s
     Running tests/threading.rs (target/debug/deps/threading-d3377e4df23b256f)
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.02s
     Running unittests src/lib.rs (target/debug/deps/lightcraft_engine-1eb54fb7e444a2ab)
test result: FAILED. 511 passed; 1 failed; 4 ignored; 0 measured; 0 filtered out; finished in 180.23s
     Running tests/coverage/main.rs (target/debug/deps/coverage-4c152a48567ea4b7)
test result: ok. 381 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 2.69s
     Running tests/develop_backend.rs (target/debug/deps/develop_backend-150567d4db28e0ed)
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 2.37s
     Running tests/panic_hunt.rs (target/debug/deps/panic_hunt-991ce665177c9240)
test result: ok. 2 passed; 0 failed; 2 ignored; 0 measured; 0 filtered out; finished in 1.03s
     Running unittests src/lib.rs (target/debug/deps/lightcraft_meta-3e0f49e74b712fbd)
test result: ok. 63 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.11s
     Running tests/coverage/main.rs (target/debug/deps/coverage-00385910f6b389dc)
test result: ok. 29 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
     Running tests/robust.rs (target/debug/deps/robust-d16460faeea548ec)
test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.08s
     Running unittests src/lib.rs (target/debug/deps/lightcraft_pipeline-001425a00f0b0dde)
test result: ok. 220 passed; 0 failed; 3 ignored; 0 measured; 0 filtered out; finished in 3.88s
     Running tests/coverage/main.rs (target/debug/deps/coverage-84fb0dff5f88ec04)
test result: ok. 201 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.04s
     Running unittests src/lib.rs (target/debug/deps/lightcraft_ui_egui-82ee36dabf084767)
test result: FAILED. 337 passed; 1 failed; 8 ignored; 0 measured; 0 filtered out; finished in 388.37s
     Running unittests src/main.rs (target/debug/deps/local_image-878d1c2946569db9)
test result: ok. 153 passed; 0 failed; 6 ignored; 0 measured; 0 filtered out; finished in 9.85s
     Running tests/mac_menu_appkit.rs (target/debug/deps/mac_menu_appkit-df3b0a9ebd5cb12b)
     Running unittests src/lib.rs (target/debug/deps/photocraft_codecs-5f45ab9f1385100b)
test result: ok. 41 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.31s
     Running tests/coverage/main.rs (target/debug/deps/coverage-7a5ab75905a28276)
test result: ok. 19 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
     Running tests/it/main.rs (target/debug/deps/it-8cf01d6370eee56e)
test result: ok. 524 passed; 0 failed; 3 ignored; 0 measured; 0 filtered out; finished in 3.67s
     Running tests/pnm_peak_memory.rs (target/debug/deps/pnm_peak_memory-b4806dc70bfe7ef7)
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.04s
     Running unittests src/lib.rs (target/debug/deps/photocraft_color-c2ed80d78802d2b2)
test result: ok. 31 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.10s
     Running unittests src/lib.rs (target/debug/deps/photocraft_engine-bd64e653bdd1b116)
test result: ok. 1465 passed; 0 failed; 27 ignored; 0 measured; 0 filtered out; finished in 22.74s
     Running tests/contracts_registrations.rs (target/debug/deps/contracts_registrations-54caf643142a0e8d)
test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
     Running tests/contracts_storage.rs (target/debug/deps/contracts_storage-992b295c557a5b55)
test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
     Running tests/coverage/main.rs (target/debug/deps/coverage-e99ef579d1991440)
test result: ok. 582 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.53s
     Running tests/it/main.rs (target/debug/deps/it-4c940ef917077d7c)
test result: ok. 47 passed; 0 failed; 5 ignored; 0 measured; 0 filtered out; finished in 1.99s
     Running tests/vector_v1.rs (target/debug/deps/vector_v1-a01c6b1b4d59bdfa)
test result: ok. 12 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 0.04s
     Running unittests src/lib.rs (target/debug/deps/photocraft_io-32965b1d4be23561)
test result: ok. 142 passed; 0 failed; 2 ignored; 0 measured; 0 filtered out; finished in 2.45s
     Running tests/contracts_psd.rs (target/debug/deps/contracts_psd-7f033970c72aab88)
test result: ok. 14 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.55s
     Running tests/coverage/main.rs (target/debug/deps/coverage-75e0887e1b27bf7b)
test result: ok. 120 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
     Running tests/it/main.rs (target/debug/deps/it-42d29f68b2275269)
test result: ok. 222 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 3.96s
     Running tests/overwrite_original.rs (target/debug/deps/overwrite_original-23aaa2205faa3c36)
test result: ok. 12 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
     Running unittests src/lib.rs (target/debug/deps/photocraft_psd-d61d6602a4ad44e9)
test result: ok. 169 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.19s
     Running tests/coverage/main.rs (target/debug/deps/coverage-41e7687d76ab761d)
test result: ok. 97 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
     Running tests/it/main.rs (target/debug/deps/it-0777ed173079486a)
test result: ok. 138 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.16s
     Running unittests src/lib.rs (target/debug/deps/photocraft_svg-db8fa09ac3cdcea1)
test result: ok. 17 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
     Running tests/export/main.rs (target/debug/deps/export-77c1e8ed28565fe5)
test result: ok. 43 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 11.33s
     Running tests/import/main.rs (target/debug/deps/import-efc50d2b15f964ae)
test result: ok. 22 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 3.49s
     Running unittests src/lib.rs (target/debug/deps/photocraft_ui_egui-b3cbfc39ef03b484)
test result: FAILED. 1608 passed; 2 failed; 63 ignored; 0 measured; 0 filtered out; finished in 133.02s
     Running tests/appearance/main.rs (target/debug/deps/appearance-1882aa47c0c67db0)
test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.29s
     Running tests/contracts_tools.rs (target/debug/deps/contracts_tools-02e56ba20ea5c7db)
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
     Running tests/gpu/main.rs (target/debug/deps/gpu-6940f3f481e708f2)
test result: FAILED. 88 passed; 1 failed; 16 ignored; 0 measured; 0 filtered out; finished in 73.94s
     Running tests/it/main.rs (target/debug/deps/it-60bd2c6d2a06caed)
test result: ok. 14 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 19.99s
     Running tests/layout16.rs (target/debug/deps/layout16-9bfcc0d1daf0bbba)
test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.07s
     Running tests/path_text/main.rs (target/debug/deps/path_text-bff47f64ee34a674)
test result: ok. 5 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 0.05s
     Running tests/pen/main.rs (target/debug/deps/pen-91f729768f5277b6)
test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.28s
     Running tests/snapping/main.rs (target/debug/deps/snapping-b3838632578b515b)
test result: ok. 10 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 2.66s
   Doc-tests li_ai
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
   Doc-tests li_seg
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
   Doc-tests lightcraft_engine
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
   Doc-tests lightcraft_meta
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
   Doc-tests lightcraft_pipeline
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
   Doc-tests lightcraft_ui_egui
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
   Doc-tests photocraft_codecs
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
   Doc-tests photocraft_color
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
   Doc-tests photocraft_engine
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
   Doc-tests photocraft_io
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
   Doc-tests photocraft_psd
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
   Doc-tests photocraft_svg
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
   Doc-tests photocraft_ui_egui
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
error: 4 targets failed:
Exit: 101 (five failures, subsequently repaired)
```

Exact repair/recheck commands and outputs:

```text
cargo +1.98.1 test --offline -p lightcraft-engine -p photocraft-ui-egui --lib tests_import -- --nocapture
     Running unittests src/lib.rs (target/debug/deps/lightcraft_engine-a9ee97b72229db97)
test tests_import::dsfix_cached_scan_reprobes_same_size_file_after_edit ... ok
test tests_import::dsfix_mixed_import_restore_undo_counts_all_photos ... ok
test tests_import_move::copy_import_refuses_a_file_changed_since_the_review ... ok
test result: ok. 45 passed; 0 failed; 1 ignored; 0 measured; 470 filtered out; finished in 18.24s
     Running unittests src/lib.rs (target/debug/deps/photocraft_ui_egui-ba8dd41c9e954487)
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 1673 filtered out; finished in 0.00s
Exit: 0
```

```text
cargo +1.98.1 test --offline -p lightcraft-ui-egui -p photocraft-ui-egui --lib strip_tool_tips_icon_pixels_follow_colour_preference_and_keep_text -- --nocapture
     Running unittests src/lib.rs (target/debug/deps/lightcraft_ui_egui-71d0df8a6e3c0dcf)
test headless::tests::strip_tool_tips_icon_pixels_follow_colour_preference_and_keep_text ... ok
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 345 filtered out; finished in 1.67s
     Running unittests src/lib.rs (target/debug/deps/photocraft_ui_egui-ba8dd41c9e954487)
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 1673 filtered out; finished in 0.00s
Exit: 0
```

```text
cargo +1.98.1 test --offline -p photocraft-ui-egui --lib tool_tips_icon_pixels_follow_colour_preference_and_keep_text -- --nocapture
     Running unittests src/lib.rs (target/debug/deps/photocraft_ui_egui-ba8dd41c9e954487)
test tool_tips::tests::tool_tips_icon_pixels_follow_colour_preference_and_keep_text ... ok
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 1672 filtered out; finished in 0.47s
Exit: 0
```

```text
cargo +1.98.1 test --offline -p photocraft-ui-egui --lib dsfix_shortcuts_ok_uses_selected_capture_one_defaults -- --nocapture
     Running unittests src/lib.rs (target/debug/deps/photocraft_ui_egui-ba8dd41c9e954487)
test prefs_ui::keymap_tests::dsfix_shortcuts_ok_uses_selected_capture_one_defaults ... ok
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 1672 filtered out; finished in 0.56s
Exit: 0
```

```text
cargo +1.98.1 test --offline -p photocraft-ui-egui --lib dsfix_color_tile_click_opens_picker -- --nocapture
     Running unittests src/lib.rs (target/debug/deps/photocraft_ui_egui-ba8dd41c9e954487)
test widgets::dsfix_color_tile_click_opens_picker ... ok
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 1672 filtered out; finished in 0.01s
Exit: 0
```

```text
cargo +1.98.1 test --offline -p photocraft-ui-egui --lib symbols_kittest_create_place_master_edit_detach_and_undo -- --nocapture
     Running unittests src/lib.rs (target/debug/deps/photocraft_ui_egui-ba8dd41c9e954487)
test symbols::tests::symbols_kittest_create_place_master_edit_detach_and_undo ... ok
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 1672 filtered out; finished in 0.02s
Exit: 0
```

```text
cargo +1.98.1 test --offline -p photocraft-ui-egui --test gpu symbols_full_app_rtx_preview_release_master_edit_zoom_detach_undo_and_cancel -- --nocapture
     Running tests/gpu/main.rs (target/debug/deps/gpu-9c1dec9a48f81597)
symbols full-app adapter: AdapterInfo { name: "NVIDIA GeForce RTX 5090", vendor: 4318, device: 11141, device_type: DiscreteGpu, device_pci_bus_id: "0000:01:00.0", driver: "NVIDIA", driver_info: "615.71.09", backend: Vulkan, subgroup_min_size: 32, subgroup_max_size: 32, transient_saves_memory: Some(false), limit_bucket: None }
test symbol_instances::symbols_full_app_rtx_preview_release_master_edit_zoom_detach_undo_and_cancel ... ok
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 104 filtered out; finished in 1.51s
Exit: 0
```

```text
cargo +1.98.1 clippy --offline -p li-ai -p li-seg -p lightcraft-engine -p lightcraft-meta -p lightcraft-pipeline -p lightcraft-ui-egui -p local-image -p photocraft-codecs -p photocraft-color -p photocraft-engine -p photocraft-io -p photocraft-psd -p photocraft-svg -p photocraft-ui-egui --all-targets -- -D warnings
warning: lightcraft-engine@2.0.0-alpha.1: no craft-fonts embedded: Chinese and Japanese text will render as empty boxes. Clone https://github.com/storytold/craft-fonts next to this repo and rebuild with CRAFT_FONTS_DIR=../craft-fonts
    Checking lightcraft-ui-egui v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/cx-dsfix/crates/lc-ui-egui)
    Checking local-image v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/cx-dsfix/apps/local-image)
    Finished `dev` profile [optimized + debuginfo] target(s) in 2.65s

Exit: 0
```

## Final state

All 47 findings are fixed; none was dismissed after re-check. New English UI/accessibility/error strings are on the existing pending lists. The intentional AI Remove flow, monochrome defaults, whole-image-only tools, macOS P3 behavior and adaptive thread policy were retained. `Cargo.toml`/`Cargo.lock`, Match Look sources and vendor sources are unchanged. No commits were made.

One additional strict hardware GPU test was added (F038); the existing symbol full-app GPU case also passed its repaired recheck on RTX 5090/Vulkan. The rare colord hot-unplug/property-error case has no isolated synthetic-bus regression; its per-device failure guards were checked directly and the host crate's existing tests passed. macOS diagnostic branches were compiled as ordinary conditional expressions on Linux; no macOS hardware run was needed for these message-only fixes.

Artifacts and full logs remain in this worktree's `target/`, including `dsfix-*.log`, eight display-preferences images under `target/design-system/`, and 110 host-workflow screenshots from the required host suite under `target/dsfix-host-qa/library-workflow-ux/`. No build cache was removed or refreshed.
