# Vector and Pixel/Vector review fixes

Continued the supplied branch at `c8ca0b63`; the landed `e698208d` hotfix and the four known-test corrections are retained. The pre-existing `docs/wip/CODEX-TASK.md` edits are preserved. No commits, native GUI windows, owner-app shutdowns, or ComfyUI actions were made. All UI verification uses headless egui harnesses.

All assigned fixes are complete. The full target sweep is not wholly green: one unchanged Draft/Refinement mock-service test also fails on a forced rebuild of unmodified HEAD. The assigned regressions, workspace check and strict Clippy pass; details and full failure output are below.

## Re-check and fixes

Regression test names below are suffixes of `bugfix_vector_*` unless otherwise stated. Baselines were run with only the new assertions added, before applying production fixes.

| Item | Verdict against current branch | Fix | Regression / validation |
|---|---|---|---|
| O-006 | Real: the legacy options provider still claimed Direct Selection and primary shape tools; conversion was drawn only by the skipped context provider. | Return ownership to the shared context provider; draw the captured conversion modal from the normal frame hook. | `shared_options_own_nodes_and_live_shapes`; existing direct-selection conversion and shared-tool tests. |
| O-014 | Real: flat export, Flatten, Merge Visible, web export and print used native source caches instead of the shared display projection. | Project at those output boundaries, including Save a Copy and flattened Place/stack input. Keep native/layered save behavior. | `flat_export_matches_projected_paints`, `flatten_and_merge_use_canvas_projection`; existing file, web, print, merge and vector projection tests. |
| O-015 | Real: raw canvas presses reached vector tools beneath floating UI and scrollbars. | Check the top egui layer and scrollbar rectangles before routing a press; retain captures for release outside the canvas. | `floating_panel_click_does_not_start_pen`; existing shared capture/release and scrollbar tests. |
| O-016 | Real: temporary Hand/Zoom keys were checked after shared vector input. | Check held/latched temporary tools before vector input; retain the existing Space reposition policy for an active shape drag. | `space_pan_does_not_start_pen`; existing held-tool tests. |
| O-027 | Real: vector display policies and overlays retained closed documents. | Prune policies, overlays and projection/preview Arc caches by open DocId; overlay keys also use stable DocId. | `closed_document_releases_vector_view_cache` checks weak-reference release. |
| O-030 | Real: rasterization left path effects in VectorData and failed subsequent projections. | Bake the shared isolated projection into pixels, then clear VectorData; retain outer opacity, masks and layer effects for single application. | `rasterize_path_effect_bakes_pixels_and_clears_effect`, `legacy_paint_edits_update_appearance_and_rasterize_clears_metadata`, including Undo. |
| O-031 | Real: maintenance used edit eligibility, so locking removed swatch links. | Prune by structural paint availability rather than edit locks. | `lock_keeps_global_swatch_links` checks lock/unlock. |
| O-032 | Real: Pixel Fill/Stroke edits changed legacy fields while the projection read appearance. | Synchronize the top appearance paint item for legacy paint edits/pastes, preserve advanced stroke fields, and render through the common shape provider. | `legacy_paint_edits_update_appearance_and_rasterize_clears_metadata`; existing shape edits/paste tests. |
| O-033 | Real: displayed Pixel bindings diverged from the Vector routing; Join lacked its adapter. | Derive menu labels and tooltips from effective command/tool bindings; route Join and vector paint/paste commands through their captured adapters. | `printed_command_and_tool_bindings_match_dispatch`, `preferences_preset_apply_cancel_and_custom_keys`; existing menu/shortcut tests. |
| O-035 | Real: Place SVG sent an empty engine request without opening a picker. | Use the normal Place Embedded picker/import route. | `place_svg_uses_picker` drives the actual bar button with an injected picker. |
| O-036 | Real: Path dialogs were drawn in the keyboard hook, which skipped focused text inputs. | Draw captured Path dialogs every app frame, independently of keyboard focus. | `path_dialog_drawn_with_text_focus`; existing Path numeric dialog tests. |
| O-037 | Real: generic Clear consumed Delete ahead of Width/Puppet. | Handle selected width points and puppet pins in the pre-dispatch hook with canvas keyboard focus. | `width_delete_runs_before_generic_clear`, `puppet_delete_runs_before_generic_clear`. |
| O-038 | Real: global focus loss reset multistep work in both personas. | Suspend interrupted pointer drags; preserve Pen anchors, polygon/magnetic lasso, perspective quad, recolor and numeric drafts. Tool/persona/stale-target transitions still reset captures. | `pen_keeps_draft_on_focus_loss_and_finishes_escape`, `pixel_polygon_survives_focus_loss`, `focus_loss_and_foreign_dialog_preserve_recolor_and_ase`; existing gesture lifecycle tests. |
| O-039 | Real: engine Outline Stroke silently discarded unsupported metadata, and Offset lacked a production menu route. | Refuse destructive outline until Expand Appearance for live/advanced objects; register Offset Path in the production menu and adapter. | `outline_stroke_refuses_unexpanded_appearance`; existing outline, offset and Path tests. |
| F04 | Real in viewport, native and on-path rendering: global color grouping painted red/blue/red as red/blue. | Group only adjacent paints; preserve that order in raster, viewport, on-path and text-to-curve conversion providers. | `overlapping_native_text_preserves_run_paint_order` uses shaped HHH, legal negative tracking, opaque/translucent runs, viewport/native/on-path output. |
| F05 | Real: newly drawn design rectangles did not have legacy live-rectangle metadata. | Borrowed Move options read/write design rectangle radii through `shape.design.set`, retaining the legacy provider. | `design_rectangle_move_bar_exposes_corners` draws/edits a rectangle in each persona, checks one journal entry and full Undo. |
| F08 | Real: endpoint-to-curve ties reserved only the source and made opposite duplicate bridges. | Recognize coincident dangling targets and reserve both endpoint identities. | `gap_endpoint_curve_ties_reserve_both_ends`: reversed source order, one bridge, closed-region construction and interior face hit. No visible-fill-loss claim is made. |
| F11 / FB-1 | Real: transform lifetime was gated by the canvas tool and overwrote menu-captured kind. | Initialize captured target/document/kind; enable lifecycle while a dialog/pending conversion exists; avoid overwriting its kind. | `menu_transforms_survive_production_frames` covers six kinds from Move/Pen/Rotate; `actual_object_transform_menu_click`; `menu_transform_numeric_ok_copy_and_undo`. |
| FB-2 | Real: Non-destructive Envelope had no executable host handoff. | Hide that action as explicitly permitted by the review; expose the working Convert to Curves action. Typed envelope-provider APIs remain for future integration. | `conversion_prompt_offers_only_executable_action` checks absence, executes conversion, checks one journal entry. |
| FB-3 | Real: global reset on any Escape/dialog/focus loss erased independent drafts and ASE receiver. | Remove the global reset for transient UI ownership, gate Escape by keyboard focus, preserve package drafts and ASE receiver across tool/modal transitions. | `escape_in_recolor_text_field_preserves_plan`; `focus_loss_and_foreign_dialog_preserve_recolor_and_ase` verifies eventual palette import after focus returns; existing swatch/transform/mesh/width/envelope tests. |
| FB-4 | Real in Pixel persona as well. | Apply the same suspension policy in persona/shared tools. | Pixel cases of Pen/polygon regressions and existing shared-tool tests. |
| FB-5 | Per-persona policy is authorized; lost overrides and mismatched labels were real. | Apply explicit user assignments after persona defaults; use separate Affinity command defaults, and one table for display and dispatch. | `printed_command_and_tool_bindings_match_dispatch`, `preferences_preset_apply_cancel_and_custom_keys`, existing keyboard table tests. |
| U-010 | Real: preset affected only tool letters and was hidden in the Vector menu. | Affinity command defaults include Duplicate/Deselect/Character/Group/Ungroup; move picker into Preferences → Tools with normal draft/Apply/Cancel. | `preferences_preset_apply_cancel_and_custom_keys`; menu and tool binding regression. |
| U-012 | Real: unfinished Pen/path-text was discarded and AI Remove Undo reached the document. | Escape/Enter finish Pen and path text; Backspace/Cmd-Z remove one new Pen anchor; focus/pointer leave preserve drafts. Add bounded local mask-stroke Undo before document Undo. | `pen_keeps_draft_on_focus_loss_and_finishes_escape`, `path_text_survives_pointer_leaving_and_escape_keeps_text`, `remove_undo_keeps_document_and_removes_last_stroke`. |

The additional Medium/Low vector-file items are O-027, O-031 and O-039; all are included above. No review item outside the assigned area was expanded. Small changes to engine file/web/print/flatten/merge boundaries, UI Preferences and AI Remove are required for the explicitly assigned O-014/U-010/U-012 behavior and are identified in the table.

## Reference behavior and strings

Adobe documents Escape as stopping/keeping a Pen path and Undo Pen as removing the last point: [Illustrator Pen tutorial](https://www.adobe.com/learn/illustrator/web/use-pen-tool), [Pen tool help](https://helpx.adobe.com/uk/illustrator/using/tool-techniques/pen-tool.html). Affinity's command and tool defaults, and its settings-based customization, are drawn from [Designer keyboard shortcuts](https://affinity.help/designer2/English.lproj/pages/Workspace/shortcuts.html) and the [official Windows shortcut sheet](https://resources.serif.com/spotlight/learning/shortcuts/Affinity-Designer-2-Shortcuts-Windows.pdf). The guarded outline behavior uses the existing Expand Appearance route rather than silently dropping style; compare [Affinity Expand Stroke](https://affinity.help/designer2/English.lproj/pages/CurvesShapes/expandStroke.html).

New transform-conversion/corner labels and updated Pen instructions are registered in `crates/pc-ui-egui/src/i18n/pending_vector.rs`; existing translated labels are reused for Preferences and the outline guard.

## Validation

Toolchain: `rustc 1.98.1 (48a229cea 2026-09-01) (Fedora 1.98.1-1.fc44)`. `CARGO_BUILD_JOBS` is unset. Every Cargo invocation uses the shared gate and `--offline`.

The full stdout/stderr captures are compressed losslessly under [bugfix-vector-validation](bugfix-vector-validation/). Commands below ran from the repository root. Baseline failures were obtained before production edits; incomplete fixture attempts are retained separately.

```sh
PATH=/home/zdavidson/.local/share/local-image-dev/bin:$PATH RUSTC_WRAPPER=li-rustc-gate cargo test --offline -p photocraft-engine -p photocraft-ui-egui -p photocraft-vecfx --lib bugfix_vector > target/bugfix-vector/baseline-3.log 2>&1
PATH=/home/zdavidson/.local/share/local-image-dev/bin:$PATH RUSTC_WRAPPER=li-rustc-gate cargo test --offline -p photocraft-text --lib bugfix_vector > target/bugfix-vector/baseline-text.log 2>&1
PATH=/home/zdavidson/.local/share/local-image-dev/bin:$PATH RUSTC_WRAPPER=li-rustc-gate cargo test --offline -p photocraft-ui-egui -p photocraft-vecfx --lib bugfix_vector --no-fail-fast > target/bugfix-vector/baseline-ui-final.log 2>&1
PATH=/home/zdavidson/.local/share/local-image-dev/bin:$PATH RUSTC_WRAPPER=li-rustc-gate cargo test --offline -p photocraft-ui-egui --lib bugfix_vector --no-fail-fast > target/bugfix-vector/baseline-ui-complete.log 2>&1
PATH=/home/zdavidson/.local/share/local-image-dev/bin:$PATH RUSTC_WRAPPER=li-rustc-gate cargo test --offline -p photocraft-ui-egui --lib delete_runs_before_generic_clear --no-fail-fast > target/bugfix-vector/baseline-delete.log 2>&1
```

Captured baseline output (all exit 101):

```text
engine: test result: FAILED. 0 passed; 6 failed; 0 ignored; 0 measured; 1319 filtered out
text: test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 85 filtered out
vecfx: test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 47 filtered out
UI complete: test result: FAILED. 0 passed; 21 failed; 0 ignored; 0 measured; 1326 filtered out
Delete corrected fixture: test result: FAILED. 0 passed; 2 failed; 0 ignored; 0 measured; 1345 filtered out
```

Full captures: [engine](bugfix-vector-validation/baseline-3.log.gz), [text](bugfix-vector-validation/baseline-text.log.gz), [UI/vecfx](bugfix-vector-validation/baseline-ui-final.log.gz), [UI complete](bugfix-vector-validation/baseline-ui-complete.log.gz), [Delete](bugfix-vector-validation/baseline-delete.log.gz). Earlier fixture attempts are [baseline](bugfix-vector-validation/baseline.log.gz), [baseline-2](bugfix-vector-validation/baseline-2.log.gz) and [baseline UI](bugfix-vector-validation/baseline-ui.log.gz): corrected mismatched egui APIs, missing texture-delta cleanup and actual `RawInput.focused` before final baselines. Final headless fixture corrections select the SpinButton rather than the identically labelled Slider and install the application's named fonts. Assertions were retained.

The first production regression run passed engine 6/6, text 1/1, vecfx 1/1 and UI 17/21. Its remaining failures identified the Recolor focus timing and shared Escape/path-text routing, plus the two headless fixtures above. Build attempts also identified fallible helper callers and the text helper visibility required by the standalone view harness. Full captures: [first build](bugfix-vector-validation/fixed-regressions.log.gz), [second build](bugfix-vector-validation/fixed-regressions-2.log.gz), [first regression results](bugfix-vector-validation/fixed-regressions-3.log.gz), [first workspace check](bugfix-vector-validation/workspace-check.log.gz). These are intermediate failures, not final passing gates.

The final production code passes all 29 new regressions (engine 6, text 1, UI 21, vecfx 1), the workspace/all-target check and strict Clippy on all four touched crates. The existing-target sweep was run once; its failing cases were then investigated and rechecked after the necessary fixes/declared expectation corrections. The final code has no assigned vector regression failure.

```sh
PATH=/home/zdavidson/.local/share/local-image-dev/bin:$PATH RUSTC_WRAPPER=li-rustc-gate cargo test --offline -p photocraft-engine -p photocraft-ui-egui -p photocraft-text -p photocraft-vecfx --lib bugfix_vector --no-fail-fast > target/bugfix-vector/fixed-regressions-4.log 2>&1
PATH=/home/zdavidson/.local/share/local-image-dev/bin:$PATH RUSTC_WRAPPER=li-rustc-gate cargo check --offline --workspace --all-targets > target/bugfix-vector/workspace-check-final.log 2>&1
PATH=/home/zdavidson/.local/share/local-image-dev/bin:$PATH RUSTC_WRAPPER=li-rustc-gate cargo clippy --offline -p photocraft-engine -p photocraft-ui-egui -p photocraft-text -p photocraft-vecfx --all-targets -- -D warnings > target/bugfix-vector/clippy-final.log 2>&1
```

Captured final output (all exit 0):

```text
engine: test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 1319 filtered out
text: test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 85 filtered out
UI: test result: ok. 21 passed; 0 failed; 0 ignored; 0 measured; 1326 filtered out
vecfx: test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 47 filtered out
workspace check: Finished `dev` profile [optimized + debuginfo] target(s) in 4m 05s
strict Clippy: Finished `dev` profile [optimized + debuginfo] target(s) in 4m 05s
```

Full outputs: [29 regressions](bugfix-vector-validation/fixed-regressions-4.log.gz), [workspace check](bugfix-vector-validation/workspace-check-final.log.gz), [strict Clippy](bugfix-vector-validation/clippy-final.log.gz). The existing lightcraft-engine build-script warning about absent optional craft-fonts is retained in the captures; Clippy emitted no lint failure. Earlier successful gates are retained as [workspace check 2](bugfix-vector-validation/workspace-check-2.log.gz) and [Clippy 3](bugfix-vector-validation/clippy-3.log.gz). Earlier lint failures identified `as_chunks` and test-only panic syntax; both were corrected, with [first](bugfix-vector-validation/clippy.log.gz) and [second](bugfix-vector-validation/clippy-2.log.gz) captures preserved.

The corrected Preferences baseline temporarily restored only `prefs_ui.rs` from HEAD, ran the following command, and restored the exact fixed bytes in a Python `finally` block. With named fonts installed in the fixture, it failed on the genuinely missing `Vector Keyboard Preset` picker (exit 101), rather than the earlier font setup issue. Full output: [Preferences baseline](bugfix-vector-validation/baseline-preferences.log.gz).

```sh
PATH=/home/zdavidson/.local/share/local-image-dev/bin:$PATH RUSTC_WRAPPER=li-rustc-gate cargo test --offline -p photocraft-ui-egui --lib bugfix_vector_preferences_preset_apply_cancel_and_custom_keys --no-fail-fast
```

### Existing targets and failure rechecks

```sh
PATH=/home/zdavidson/.local/share/local-image-dev/bin:$PATH RUSTC_WRAPPER=li-rustc-gate RUST_TEST_THREADS=3 WGPU_BACKEND=vulkan WGPU_POWER_PREF=high WGPU_ADAPTER_NAME='NVIDIA GeForce RTX 5090' __GL_SHADER_DISK_CACHE=0 cargo test --offline -p photocraft-engine -p photocraft-ui-egui -p photocraft-text -p photocraft-vecfx --all-targets --no-fail-fast > target/bugfix-vector/existing-tests.log 2>&1
```

This sweep exited 101: 40 test summaries total **3,633 passed, 14 failed, 98 ignored**. It included the new tests. Thirteen failures now pass their targeted rechecks; the remaining mock-service failure is discussed below. The unchanged successful cases were not repeatedly rerun. Full output: [all targets](bugfix-vector-validation/existing-tests.log.gz). The test watchdog occasionally printed an arithmetic error while reading a child process's `/proc` data during process exit; the captures retain those messages and actual child exit statuses.

| Failure in sweep | Resolution and final output |
|---|---|
| `transform34_conversion_styled_and_path_text_preserve_paints_decorations_atomic_copy` | Corrected paint-order expectation, retaining geometry/Undo assertions; [1 passed](bugfix-vector-validation/recheck-text-conversion.log.gz). |
| `outline_stroke_converts_paint_and_join_connects_open_ends` | Keep the legacy native renderer when VectorData is empty; use the shared provider for vector metadata. The complete `vector_v1` target [passes 12, ignores 1](bugfix-vector-validation/recheck-vector-v1.log.gz). |
| `space_on_the_canvas_moves_the_frame_not_the_view` | Temporary Hand guard also recognizes the existing crop drag; [1 passed](bugfix-vector-validation/recheck-crop.log.gz). |
| `space_repositions_a_marquee_being_drawn_then_sizing_resumes` | Guard retains the active pixel marquee reposition policy; [1 passed](bugfix-vector-validation/recheck-marquee.log.gz). |
| `tool_tips_show_remapped_keys_and_hidden_tools_still_switch_by_key` | Explicit custom tool assignments select their named tool before default group cycling; [1 passed](bugfix-vector-validation/recheck-tool-key.log.gz). |
| `bugfix_vector_escape_in_recolor_text_field_preserves_plan` | Capture pre-Escape keyboard focus in egui's input hook, before egui clears it; passes in the final 29-test run. The earlier isolated failure is [retained](bugfix-vector-validation/recolor-focus-rerun.log.gz). |
| `width_package_ui_escape_focus_selection_tool_switch_and_release_outside` | Recognize explicit `WindowFocused(false)` as well as RawInput focus; suspend/cancel the in-flight drag, preserve package values; [1 passed](bugfix-vector-validation/recheck-width.log.gz). |
| `clip35_full_app_gpu_draw_inside_live_pointer_preview_release_cancel_and_undo` | Restore Release Clip/Release Compound commands and their captured adapters in the unified binding table; [1 passed on RTX 5090/Vulkan](bugfix-vector-validation/recheck-gpu-clip.log.gz). |
| `path_text_app_gpu_actual_curve_typing_zoomed_preview_release_undo_cancel` | Declared Escape-finish expectation correction; [1 passed on RTX 5090/Vulkan](bugfix-vector-validation/recheck-gpu-path.log.gz). |
| `pen_app_gpu_live_curved_closed_release_undo_cancel_and_modes` | Declared Escape-finish expectation correction; [1 passed on RTX 5090/Vulkan](bugfix-vector-validation/recheck-gpu-pen.log.gz). |
| `path_text_ui_escape_focus_tool_change_and_staleness_cancel` | Declared Escape/focus expectation correction; complete path-text target [5 passed, 1 ignored](bugfix-vector-validation/recheck-pen-path-text.log.gz). |
| `pen_ui_escape_focus_persona_document_revision_and_mode_switch_cancel` | Declared Escape/focus expectation correction; complete Pen target [8 passed](bugfix-vector-validation/recheck-pen-path-text.log.gz). |
| `gpu::pen_ui_gpu_real_pointer_preview_release_and_cancel` | Declared Escape-finish and exact Undo GPU-frame checks; passes in the complete Pen target above. |
| `draft_refine_mock_comfy_uses_source_and_library_results_can_be_used_or_opened` | Still fails in an isolated run after its ten-second wait; [full output](bugfix-vector-validation/recheck-mock-service.log.gz). Original-code comparison follows below. |

Exact Cargo recheck commands (exit 0):

```sh
PATH=/home/zdavidson/.local/share/local-image-dev/bin:$PATH RUSTC_WRAPPER=li-rustc-gate cargo test --offline -p photocraft-engine --test vector_v1 > target/bugfix-vector/recheck-vector-v1.log 2>&1
PATH=/home/zdavidson/.local/share/local-image-dev/bin:$PATH RUSTC_WRAPPER=li-rustc-gate RUST_TEST_THREADS=3 WGPU_BACKEND=vulkan WGPU_POWER_PREF=high WGPU_ADAPTER_NAME='NVIDIA GeForce RTX 5090' __GL_SHADER_DISK_CACHE=0 cargo test --offline -p photocraft-ui-egui --test path_text --test pen --no-fail-fast > target/bugfix-vector/recheck-pen-path-text.log 2>&1
```

The remaining rechecks used the gated, rebuilt test executables through the same memory-watchdog runner. These are the exact commands; each selected case passes except the explicitly identified mock-service test:

```sh
RUST_TEST_THREADS=3 /home/zdavidson/.local/share/local-image-dev/bin/li-test-runner target/debug/deps/photocraft_ui_egui-3460679ec9f9b1dc crop_ui::tests::space_on_the_canvas_moves_the_frame_not_the_view --exact --nocapture > target/bugfix-vector/recheck-crop.log 2>&1
RUST_TEST_THREADS=3 /home/zdavidson/.local/share/local-image-dev/bin/li-test-runner target/debug/deps/photocraft_ui_egui-3460679ec9f9b1dc hold_keys::tests::space_repositions_a_marquee_being_drawn_then_sizing_resumes --exact --nocapture > target/bugfix-vector/recheck-marquee.log 2>&1
RUST_TEST_THREADS=3 /home/zdavidson/.local/share/local-image-dev/bin/li-test-runner target/debug/deps/photocraft_ui_egui-3460679ec9f9b1dc tool_tips::tests::tool_tips_show_remapped_keys_and_hidden_tools_still_switch_by_key --exact --nocapture > target/bugfix-vector/recheck-tool-key.log 2>&1
RUST_TEST_THREADS=3 /home/zdavidson/.local/share/local-image-dev/bin/li-test-runner target/debug/deps/photocraft_ui_egui-3460679ec9f9b1dc width::tests::width_package_ui_escape_focus_selection_tool_switch_and_release_outside --exact --nocapture > target/bugfix-vector/recheck-width.log 2>&1
/home/zdavidson/.local/share/local-image-dev/bin/li-test-runner target/debug/deps/photocraft_engine-0962a62fc0794aad vector_packages::transform_tools::tests::transform34_conversion_styled_and_path_text_preserve_paints_decorations_atomic_copy --exact --nocapture > target/bugfix-vector/recheck-text-conversion.log 2>&1
RUST_TEST_THREADS=3 WGPU_BACKEND=vulkan WGPU_POWER_PREF=high WGPU_ADAPTER_NAME='NVIDIA GeForce RTX 5090' __GL_SHADER_DISK_CACHE=0 /home/zdavidson/.local/share/local-image-dev/bin/li-test-runner target/debug/deps/gpu-bf41e076053c48c1 clip_compound::clip35_full_app_gpu_draw_inside_live_pointer_preview_release_cancel_and_undo --exact --nocapture > target/bugfix-vector/recheck-gpu-clip.log 2>&1
RUST_TEST_THREADS=3 WGPU_BACKEND=vulkan WGPU_POWER_PREF=high WGPU_ADAPTER_NAME='NVIDIA GeForce RTX 5090' __GL_SHADER_DISK_CACHE=0 /home/zdavidson/.local/share/local-image-dev/bin/li-test-runner target/debug/deps/gpu-bf41e076053c48c1 path_text_app_acceptance::path_text_app_gpu_actual_curve_typing_zoomed_preview_release_undo_cancel --exact --nocapture > target/bugfix-vector/recheck-gpu-path.log 2>&1
RUST_TEST_THREADS=3 WGPU_BACKEND=vulkan WGPU_POWER_PREF=high WGPU_ADAPTER_NAME='NVIDIA GeForce RTX 5090' __GL_SHADER_DISK_CACHE=0 /home/zdavidson/.local/share/local-image-dev/bin/li-test-runner target/debug/deps/gpu-bf41e076053c48c1 pen_app_acceptance::pen_app_gpu_live_curved_closed_release_undo_cancel_and_modes --exact --nocapture > target/bugfix-vector/recheck-gpu-pen.log 2>&1
RUST_TEST_THREADS=3 WGPU_BACKEND=vulkan WGPU_POWER_PREF=high WGPU_ADAPTER_NAME='NVIDIA GeForce RTX 5090' __GL_SHADER_DISK_CACHE=0 /home/zdavidson/.local/share/local-image-dev/bin/li-test-runner target/debug/deps/photocraft_ui_egui-3460679ec9f9b1dc generate_ui::draft_refine::tests::draft_refine_mock_comfy_uses_source_and_library_results_can_be_used_or_opened --exact --nocapture > target/bugfix-vector/recheck-mock-service.log 2>&1
```

A preliminary GPU retry omitted the test's module prefix with `--exact`, so it selected zero tests and is **not** counted as validation. It rebuilt the current GPU executable; the three qualified commands above then ran real tests. Its [capture](bugfix-vector-validation/recheck-gpu.log.gz) is retained.


### Remaining pre-existing mock-service failure

To distinguish this failure from the vector changes, the complete `c8ca0b63` HEAD tree was extracted with `git archive HEAD` and `tar -x -C /tmp/bugfix-vector-head-baseline`. All 59 changed Rust paths in that tree were verified byte-for-byte against `git show HEAD:<path>`; the mock test itself is byte-identical in the current and original trees. The current working tree was never reverted for this comparison.

An initial original-UI run reproduced the failure ([capture](bugfix-vector-validation/baseline-mock-service.log.gz)). A second run refreshed all `.rs` timestamps in the four touched crates in the extracted tree, forcing compilation of the original engine, text, UI and vecfx dependencies as well. This avoids relying on timestamp freshness in the reused target directory. The exact final command ran with working directory `/tmp/bugfix-vector-head-baseline`:

```sh
PATH=/home/zdavidson/.local/share/local-image-dev/bin:$PATH RUSTC_WRAPPER=li-rustc-gate CARGO_TARGET_DIR='/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-vector/target' RUST_TEST_THREADS=3 WGPU_BACKEND=vulkan WGPU_POWER_PREF=high WGPU_ADAPTER_NAME='NVIDIA GeForce RTX 5090' __GL_SHADER_DISK_CACHE=0 cargo test --offline -p photocraft-ui-egui --lib generate_ui::draft_refine::tests::draft_refine_mock_comfy_uses_source_and_library_results_can_be_used_or_opened -- --exact --nocapture > '/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-vector/target/bugfix-vector/baseline-mock-service-forced.log' 2>&1
```

Captured output (exit 101), matching the fixed-code isolated failure:

```text
Finished `test` profile [optimized + debuginfo] target(s) in 2m 28s
thread 'generate_ui::draft_refine::tests::draft_refine_mock_comfy_uses_source_and_library_results_can_be_used_or_opened' panicked at crates/pc-ui-egui/src/generate_ui/draft_refine/tests.rs:399:5:
[test's status message is empty]
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 1325 filtered out; finished in 10.47s
```

Full output: [forced original-code comparison](bugfix-vector-validation/baseline-mock-service-forced.log.gz). The refinement result is still absent after the test's ten-second wait. Its cause remains outside this vector task; neither the test nor Draft/Refinement production files were edited or weakened. The current-code isolated run has the same assertion and empty status, with `1346 filtered out` and a 10.51-second test duration. This is a verified pre-existing failure in this environment, not a claim that the full target sweep passes.

After the comparison, fixed-source timestamps were refreshed without changing their bytes, and the normal UI-only feature variant was rebuilt so the shared target directory contains current-code artifacts again:

```sh
PATH=/home/zdavidson/.local/share/local-image-dev/bin:$PATH RUSTC_WRAPPER=li-rustc-gate cargo test --offline -p photocraft-ui-egui --lib bugfix_vector --no-fail-fast > target/bugfix-vector/restored-current-tests.log 2>&1
```

Captured output (exit 0):

```text
test result: ok. 21 passed; 0 failed; 0 ignored; 0 measured; 1326 filtered out; finished in 2.31s
```

Full output: [restored current UI regressions](bugfix-vector-validation/restored-current-tests.log.gz). All source contents are unchanged from the successful final workspace/Clippy gates and the combined 29-regression run.

The flattened helper is fallible so projection failures reach the caller. Its other callers in photo stacking, variable-image replacement, video export and Vanishing Point required small error-propagation edits (four engine files). Their existing tests are covered by the engine suite. Rasterization regression coverage also checks opacity, fill opacity and clipping.

TEST-CHANGED: The newly added `bugfix_vector_rasterize_path_effect_bakes_pixels_and_clears_effect` compares composed floating-point pixels within `1e-6` after adding opacity/fill-opacity/clipping coverage. Flattening the path-effect group to a raster introduces premultiply/unpremultiply roundoff: four channels differed by about `6e-8` (one f32 ULP), not a paint/opacity error. Dimensions, metadata clearing and full-document Undo remain exact. No pre-existing tests were removed or weakened.

TEST-CHANGED: `transform34_conversion_styled_and_path_text_preserve_paints_decorations_atomic_copy` now expects three ordered children (black glyph, white glyph, black underline), rather than two globally color-grouped children. The old expectation encoded F04's incorrect paint reordering. The test explicitly checks black/white/black order, retains the aggregate black contour and underline assertions, and keeps exact copy/one-journal/Undo checks.

TEST-CHANGED: `pen_ui_escape_focus_persona_document_revision_and_mode_switch_cancel` and `pen_app_gpu_live_curved_closed_release_undo_cancel_and_modes` now assert that Escape finishes a two-anchor open path, and Undo restores the full original document. The isolated focus-loss case asserts retained anchors, detached preview and no document/journal edit. Persona/tool/mode/stale-document cancellation and existing GPU pixel comparisons remain. The previous discard expectations contradicted assigned U-012/O-038.

TEST-CHANGED: `path_text_ui_escape_focus_tool_change_and_staleness_cancel` and `path_text_app_gpu_actual_curve_typing_zoomed_preview_release_undo_cancel` now check the exact retained text after Escape and exact document restoration by Undo; focus loss retains the isolated draft without a document edit. Tool/stale-target cancellation and GPU pixel comparisons remain. The previous discard expectations contradicted assigned U-012/O-038.

TEST-CHANGED: `gpu::pen_ui_gpu_real_pointer_preview_release_and_cancel` now checks Escape's committed pixels against the held curve preview, one journal entry, and full document/GPU-frame restoration by Undo. PointerGone leaves the committed path intact. Its second Enter/preview pixel comparison remains. The prior empty-frame expectation contradicted U-012's finish behavior.

## Rework

Coordinator request, 2026-10-11: li-land rejected the attempted integration on `claude/sleepy-franklin-egimjb` at `9247cc397d929a21e9abba721ad963a4410a7d2b`, with `width::tests::width_package_ui_numeric_typing_escape_and_stale_cancel` failing. The original job result is committed as `d2f69d1b`; this rework continues that implementation.

Merged the coordinator branch into the working tree with `--no-commit`. The two conflicts are resolved: `pending_vector.rs` retains both the Vector English keys and the incoming save/Library keys; `CODEX-TASK.md` retains the active Vector assignment and quotes the already-landed upstream build-gate handoff. The incoming AI, save, and build-gate implementations are retained. There are no unmerged index entries. The prepared merge and Width fix remain uncommitted for li-land.

The first merge attempt could not proceed because Git has no configured committer identity. A command-local identity was supplied for the noncommitting merge; no global or repository Git configuration was changed:

```sh
git merge --no-commit claude/sleepy-franklin-egimjb
git -c user.name=Codex -c user.email=codex@localhost merge --no-commit claude/sleepy-franklin-egimjb
git add crates/pc-ui-egui/src/i18n/pending_vector.rs docs/wip/CODEX-TASK.md
```

**Real cause:** the earlier Escape-focus protection correctly remembers a text field's focus before egui clears it. Width's panel therefore skips its canvas-wide cancellation, but its numeric blur handler still sees no current keyboard focus and commits the pending width. The unchanged test reproduces an unwanted committed `left: 35.0` where the original document has `left: 12.0`, at `width/tests.rs:184`; its journal assertion also requires no edit.

The failure is in the original Vector focus-handling change, rather than an incompatible incoming implementation. The full existing-target sweep preceded the final Escape focus-capture hook; its numeric Width case was not rerun after that hook. Rework closes that verification gap by running the complete Width module again.

**Fix:** detect Escape when one of the Width panel's own numeric `DragValue` responses loses focus and cancel that pending gesture before the automatic blur commit. This uses the response's widget ownership rather than cancelling independent fields or package drafts. Enter still commits; revision changes still discard stale edits. The production change is six added lines and one changed line in `vector_packages/width.rs`. No tests, assertions, or expectations were changed, and no new user-visible strings were added.

The queue's actual compiled test executable reproduces the failure before this fix:

```sh
RUST_TEST_THREADS=3 /home/zdavidson/.local/share/local-image-dev/bin/li-test-runner /home/zdavidson/.local/share/local-image-dev/land-target/debug/deps/photocraft_ui_egui-a93212bec34d6250 width::tests::width_package_ui_numeric_typing_escape_and_stale_cancel --exact --nocapture > target/bugfix-vector/rework/width-land-before.log 2>&1
```

Captured output (exit 101):

```text
panicked at crates/pc-ui-egui/src/vector_packages/width/tests.rs:184:9:
assertion `left == right` failed
[left document's middle WidthPoint: t: 0.5, left: 35.0, right: 20.0]
[right/original document's middle WidthPoint: t: 0.5, left: 12.0, right: 20.0]
test width::tests::width_package_ui_numeric_typing_escape_and_stale_cancel ... FAILED
```

Full captures: [queue rejection](bugfix-vector-validation/rework/land-failure.log.gz), [before-fix reproduction](bugfix-vector-validation/rework/width-land-before.log.gz). The bracketed lines above summarize the large document dumps; the capture preserves them in full.

An initial standalone Cargo baseline began rebuilding the cold worktree cache. Once the queue executable provided the actual failure, only that task-owned baseline Cargo process and its compiler descendants were terminated (exit 143), and subsequent builds used the warm queue target. That incomplete build is not counted as validation:

```sh
PATH=/home/zdavidson/.local/share/local-image-dev/bin:$PATH RUSTC_WRAPPER=li-rustc-gate cargo test --offline -p photocraft-ui-egui --lib width::tests::width_package_ui_numeric_typing_escape_and_stale_cancel -- --exact --nocapture > target/bugfix-vector/rework/width-before.log 2>&1
```

Full partial output: [abandoned cold-cache build](bugfix-vector-validation/rework/width-before.log.gz). No owner Local Image or ComfyUI process was stopped.

Required rework commands use Rust 1.98.1, the shared rustc gate, `--offline`, and an unset `CARGO_BUILD_JOBS`. These run from the repository root:

```sh
PATH=/home/zdavidson/.local/share/local-image-dev/bin:$PATH RUSTC_WRAPPER=li-rustc-gate CARGO_TARGET_DIR=/home/zdavidson/.local/share/local-image-dev/land-target /home/zdavidson/.cargo/bin/cargo +1.98.1 test --offline -p photocraft-ui-egui --lib width::tests:: -- --nocapture > target/bugfix-vector/rework/width-after.log 2>&1
PATH=/home/zdavidson/.local/share/local-image-dev/bin:$PATH RUSTC_WRAPPER=li-rustc-gate CARGO_TARGET_DIR=/home/zdavidson/.local/share/local-image-dev/land-target /home/zdavidson/.cargo/bin/cargo +1.98.1 test --offline -p photocraft-engine -p photocraft-ui-egui -p photocraft-text -p photocraft-vecfx --lib bugfix_vector --no-fail-fast > target/bugfix-vector/rework/own-tests.log 2>&1
PATH=/home/zdavidson/.local/share/local-image-dev/bin:$PATH RUSTC_WRAPPER=li-rustc-gate CARGO_TARGET_DIR=/home/zdavidson/.local/share/local-image-dev/land-target /home/zdavidson/.cargo/bin/cargo +1.98.1 check --offline --workspace --all-targets > target/bugfix-vector/rework/workspace-check.log 2>&1
PATH=/home/zdavidson/.local/share/local-image-dev/bin:$PATH RUSTC_WRAPPER=li-rustc-gate CARGO_TARGET_DIR=/home/zdavidson/.local/share/local-image-dev/land-target /home/zdavidson/.cargo/bin/cargo +1.98.1 clippy --offline -p photocraft-engine -p photocraft-ui-egui -p photocraft-text -p photocraft-vecfx --all-targets -- -D warnings > target/bugfix-vector/rework/clippy.log 2>&1
```

The first strict Clippy attempt exited 101 with incompatible metadata in dependencies: Cargo used the official Rust 1.98.1 toolchain, while `cargo-clippy` was found in `/usr/bin` and used the Fedora build of the compiler. This is a toolchain selection failure, not a source lint failure. The diagnostic identifies artifacts compiled by `rustc 1.98.1 (48a229cea 2026-09-01)` and a reader expecting the Fedora-suffixed compiler version. The retry puts the rustup proxies first in PATH; `cargo-clippy +1.98.1 --version` reports `clippy 0.1.98 (48a229ceae 2026-09-01)`:

```sh
PATH=/home/zdavidson/.cargo/bin:/home/zdavidson/.local/share/local-image-dev/bin:$PATH RUSTC_WRAPPER=li-rustc-gate CARGO_TARGET_DIR=/home/zdavidson/.local/share/local-image-dev/land-target /home/zdavidson/.cargo/bin/cargo +1.98.1 clippy --offline -p photocraft-engine -p photocraft-ui-egui -p photocraft-text -p photocraft-vecfx --all-targets -- -D warnings > target/bugfix-vector/rework/clippy-aligned.log 2>&1
```


The corrected driver then detected metadata written by the first attempt (`bitflags` was compiled by the Fedora driver). The packages compiled/checked during those two attempts were selectively cleaned; Cargo cleaned their build artifacts only, retaining the shared target's other packages. The clean exited 0 and reported `Removed 19876 files, 14.5GiB total`; its warnings say version qualifiers are ignored for cleaning. Source files were not changed. Exact repair and final retry commands:

```sh
PATH=/home/zdavidson/.cargo/bin:/home/zdavidson/.local/share/local-image-dev/bin:$PATH RUSTC_WRAPPER=li-rustc-gate CARGO_TARGET_DIR=/home/zdavidson/.local/share/local-image-dev/land-target /home/zdavidson/.cargo/bin/cargo +1.98.1 clean --offline -p bitflags@2.13.2 -p rustix@1.1.5 -p serde_json@1.0.151 -p half@2.7.1 -p stable_deref_trait@1.2.1 -p photocraft-cms@2.0.0-alpha.1 -p hashbrown@0.16.1 -p photocraft-geom@2.0.0-alpha.1 -p memmap2@0.9.11 -p font-types@0.12.5 -p indexmap@2.14.2 -p lightcraft-geom@2.0.0-alpha.1 -p photocraft-text@2.0.0-alpha.1 -p photocraft-psd@2.0.0-alpha.1 -p lightcraft-tiff@2.0.0-alpha.1 -p ttf-parser@0.25.1 -p yoke@0.8.3 -p photocraft-color@2.0.0-alpha.1 -p png@0.18.1 -p spirv@0.4.0+sdk-1.4.341.0 -p lightcraft-color@2.0.0-alpha.1 -p as-slice@0.2.1 -p lightcraft-meta@2.0.0-alpha.1 -p raw-cpuid@11.6.0 -p owned_ttf_parser@0.25.1 > target/bugfix-vector/rework/artifact-clean.log 2>&1
PATH=/home/zdavidson/.cargo/bin:/home/zdavidson/.local/share/local-image-dev/bin:$PATH RUSTC_WRAPPER=li-rustc-gate CARGO_TARGET_DIR=/home/zdavidson/.local/share/local-image-dev/land-target /home/zdavidson/.cargo/bin/cargo +1.98.1 clippy --offline -p photocraft-engine -p photocraft-ui-egui -p photocraft-text -p photocraft-vecfx --all-targets -- -D warnings > target/bugfix-vector/rework/clippy-cleaned.log 2>&1
```


Final rework results (all four required commands exited 0):

```text
Width module: test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 1354 filtered out; finished in 0.06s
Own engine regressions: test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 1323 filtered out; finished in 0.01s
Own text regression: test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 85 filtered out; finished in 0.03s
Own UI regressions: test result: ok. 21 passed; 0 failed; 0 ignored; 0 measured; 1340 filtered out; finished in 30.56s
Own vecfx regression: test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 47 filtered out; finished in 0.01s
Workspace/all-target check: Finished `dev` profile [optimized + debuginfo] target(s) in 58m 55s
Final strict Clippy: Finished `dev` profile [optimized + debuginfo] target(s) in 2m 15s
```

The combined regression build reported `58m 00s`, including shared-target and linking waits. The absent optional craft-fonts build-script warning remains in the full captures. The final Clippy run has no source lint failure. Width's existing exact document and journal assertions pass unchanged; the complete Width module also covers numeric Enter, stale revisions, graph/canvas drags, focus loss, presets, deletion and Undo. All 29 original Vector regressions pass on the merged tree.

Full stdout/stderr: [Width module](bugfix-vector-validation/rework/width-after.log.gz), [29 regressions](bugfix-vector-validation/rework/own-tests.log.gz), [workspace/all-target check](bugfix-vector-validation/rework/workspace-check.log.gz), [final strict Clippy](bugfix-vector-validation/rework/clippy-cleaned.log.gz). Intermediate toolchain diagnostics are retained as [first Clippy](bugfix-vector-validation/rework/clippy.log.gz), [aligned driver before repair](bugfix-vector-validation/rework/clippy-aligned.log.gz), and [selective artifact cleanup](bugfix-vector-validation/rework/artifact-clean.log.gz).

Final review: `git diff claude/sleepy-franklin-egimjb --check` passes; `git diff --name-only --diff-filter=U` is empty. No existing test was removed or modified in rework, so no new expectation-change declaration is required. No commit was created; `MERGE_HEAD` records `9247cc397d929a21e9abba721ad963a4410a7d2b` for the prepared merge that li-land will commit with this fix. Earlier validation above remains the record for the original `c8ca0b63` baseline; this section records the current rework gates.
