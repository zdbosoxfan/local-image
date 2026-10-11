# Test and build-gate health report

Continued HEAD `c8ca0b63f8be402084d4e85b1c589f4f47f3e94e` (the landed hotfix and known-failure work), rather than replacing it. The only initial uncommitted change was the owner's `docs/wip/CODEX-TASK.md`; that edit is preserved. Changes remain uncommitted.

| Item | Verdict after current-code re-check | Fix | Test |
|---|---|---|---|
| F02: Mixer options | Already fixed by the landed hotfix | Retained | `brush_panel::tests::mixer_brush_options_update_the_persisted_mixer_settings` passes |
| F02: brush gesture options | Already fixed by the landed hotfix | Retained | `brush_panel::tests::options_bar_edits_are_one_set_brush_per_gesture` passes |
| F02: brush preset/settings reachability | Already fixed by the landed hotfix | Retained | `brush_panel::tests::options_bar_reaches_brush_settings_and_the_preset_library` passes |
| F02: Move shape options | Already fixed by the landed hotfix | Retained | `context_bar::tests::move_tool_options_bar_edits_the_selected_shape` passes |
| Merge Layers undo | Already fixed in the known-failures landing; complete pre-edit selection is restored | Retained | `layer_multi_cmds::tests::merge_selected_into_top_most` passes |
| Layer reveal after undo | Already fixed in the known-failures landing; reveal follows the restored target | Retained | `layer_reveal::tests::a_new_active_layer_opens_its_groups_and_scrolls_into_view` passes |
| Borrowed legacy rectangle corners | Already fixed in the known-failures landing; the fixture now uses the legacy representation it exercises, with additional native-design coverage | Retained | `vector_ui::tests::ds_finding_borrowed_shape_options_allow_corner_editing` passes |
| Blend icon alpha/padding | Already fixed in the known-failures landing; generator and generated artwork agree | Retained | `fill_stroke_proxy::tests::toolbar46_registered_icon_alpha_and_styles_at_16_20_40` passes |
| F02: Layout Move | Already fixed by the landed hotfix | Retained | `layout_app_acceptance::layout16_app_existing_move_keeps_clip_and_path_translation_together` passes |
| F02: Layout Crop | Already fixed by the landed hotfix | Retained | `layout_app_acceptance::layout16_app_rtx_pointer_crop_live_release_cancel_undo_zoom_parity` passes |
| F02: Layout Resize | Already fixed by the landed hotfix | Retained | `layout_app_acceptance::layout16_app_rtx_pointer_resize_constraints_and_point_transform` passes |
| Strict Clippy | Already fixed: the icon generator validates `Art::tool`, `label`, and `recognition` | Retained; no lint suppression | Baseline and final strict workspace all-targets Clippy pass |
| Asset attribution coverage | Real: the new repository-wide audit regression reproduced exactly 1,356 missing assets | Scoped attribution rows from retained SOURCES/licence/hash records: 450 geometry SVGs, 44 PDF reference PNGs, 860 SVG fixtures/references, two text-path SVGs | `assets::tests::repository_assets_have_attribution_coverage` fails before the rows; asset audit passes afterwards |
| Handoff 33 full-app GPU registration | Real: GPU integration binary discovered zero `anchor33_gpu_` tests; both tests existed only in the ignored unit module | Register `anchor_tools_app` in GPU `main.rs`, supply the documented aliases (including `Document`), move both tests, use the central GPU lock, enable them | Original two RTX/Vulkan tests pass explicitly; final default discovery/execution recorded below |
| Handoff 43 native persistence | Real: integrated native schema/commands/rendering exist, but retained acceptance was still ignored | Enable the existing native save/load/preview/undo/redo gate | Original native gate passes explicitly; final default execution recorded below |
| Handoff 43 engine GPU | Real: installed scaled-view provider satisfies the old dependency | Enable the existing 1/2/4/8x GPU parity gate, retaining its engine GPU lock | Original RTX/Vulkan parity gate passes explicitly; final default execution recorded below |
| Handoff 43 full-app GPU | Real: test already drives actual `app.mesh_state`, production routing/display and GPU readback | Enable the functional pointer/preview/cancel/release gate, retaining its UI GPU lock | Original full-app gate passes explicitly; final default execution recorded below |
| Handoff 43 mesh timing | Real: the unconditional ignore also suppressed valid release executions | Replace unconditional ignore with `cfg_attr(debug_assertions, ignore = "performance gate requires a release build")` | Normal release command runs one test, passes on RTX 5090/Vulkan/discrete, no `--ignored` |
| Additional concurrent People timing failure | Did not reproduce in isolation with the original two-second assertion and actual cancel click | Use the review's one-test-thread execution method; source unchanged | Isolated test passes in 4.26 s including its untimed 5,000-photo fixture |
| Additional Draft/Refine mock failure | Reproduces with the user's real AI profile; passes unchanged with an isolated profile | Validation isolates AI settings/model roots as the review requires. The mock refine checker resolves its Klein 9B fixture name to the real configured 9,433,061,528-byte model before hashing/discovery; metadata-only inspection confirmed the file exists | Original binary: failure in 10.52 s on default profile; pass in 2.59 s on isolated profile |

All eleven historical failing assertions pass on the current head. The previous decisions and regressions are retained; see `docs/wip/CODEX-REPORT-known-failures.md` for their landed rationale. This task changes no test expectations or production UX and adds no user-visible strings, so no i18n pending entries or reference-app UX claims are needed.

TEST-CHANGED: The two `anchor33_gpu_` tests moved from the UI unit module to the central GPU integration binary and became regular tests. Their hardware, preview/release/cancel, byte-parity, undo and journal assertions remain intact; the local duplicate registration and private lock were removed as handoff 33 requests. No test was removed.

TEST-CHANGED: The native mesh and two functional mesh GPU ignores were removed because their integration prerequisites now exist. The timing gate is ignored only in debug builds; its release assertion and all measurement assertions remain intact and its normal release execution passed. No tolerance, timeout, expected pixels or history expectation was relaxed.

The asset-table edit is necessary shared attribution metadata; `xtask/src/assets.rs` adds a regression that runs the actual audit over Git's complete tracked/unignored asset inventory. The checker and its path-matching rules are unchanged. Patterns cover named fixture families and preserved upstream notices, including original CC0 inputs/geometry outputs, resvg MIT/Apache files, Lucide ISC/Feather MIT files and individually recorded public-domain artwork. Attribution JSON regeneration succeeds and leaves `assets/attributions.json` unchanged. No manifest, lockfile, dependency, model or product implementation changed.

Rust and Cargo are 1.98.1. Every Cargo compilation inherited `RUSTC_WRAPPER=/home/zdavidson/.local/share/local-image-dev/bin/li-rustc-gate`; `CARGO_BUILD_JOBS` is unset. Cargo build/check/test/Clippy commands use `--offline`. No GUI windows were opened; the owner app and ComfyUI were not stopped. Functional GPU checks asserted RTX 5090, Vulkan, DiscreteGpu and float formats; normal final workspace execution covers their integrated registrations.

The concurrent/default-profile workspace diagnostic finished with **10,680 passed, 2 failed, 157 ignored over 206 target summaries**, exit 101. Its two additional failures and the unchanged isolated re-checks above are explicitly retained. Its compilation overlapped the attribution work: the late xtask target includes the new audit regression and corrected metadata. The eleven historical application assertions were unchanged, and GPU registration/ignore changes followed this diagnostic; it is not a pristine-HEAD asset baseline. The explicit failing audit-regression run below establishes the attribution defect before its fix. This diagnostic was followed by the review's serialized, isolated-profile validation. The external `li-test-runner` occasionally emits an arithmetic warning while a process exits; those diagnostics are preserved separately from successful executable results. Cargo also reports the existing `rawinfo` example filename collision and missing optional embedded CJK fonts. No application/process-exit crash was observed in the baseline.

Final workspace result: **10,687 passed, zero failed, 152 ignored over 206 target summaries; exit 0**, using isolated preferences/AI state and one test thread. All five enabled functional gates execute normally; the sixth, release-only timing gate, passes in its separate normal release run. The diagnostic and final runs each account for 10,839 tests: two environmental failures become passes and five ignored functional gates become passes; no test disappears.

Complete raw command output is retained locally under `target/bugfix-gates/` (ignored build evidence). The ledger below preserves exact commands and captured output; long compilation/path inventories are explicitly excerpted. Cargo commands inherited the wrapper environment documented above. Paths are relative to this worktree unless noted.

**Toolchain** — exit 0.

```bash
rustc --version
cargo --version
```

```text
rustc 1.98.1 (48a229cea 2026-09-01) (Fedora 1.98.1-1.fc44)
cargo 1.98.1 (797e8a9bc 2026-08-05) (Fedora 1.98.1-1.fc44)
```

**Concurrent/default-profile workspace diagnostic** — exit 101. Historical failure re-check and failed-target output excerpt; aggregate counts are above.

```bash
cargo test --workspace --offline --no-fail-fast > /tmp/bugfix-gates-workspace-baseline.log 2>&1
```

```text
test tests_people::ds_finding_large_people_query_keeps_ui_cancellable ... FAILED
test result: FAILED. 257 passed; 1 failed; 4 ignored; 0 measured; 0 filtered out; finished in 524.83s
error: test failed, to rerun pass `-p lightcraft-ui-egui --lib`
test layer_multi_cmds::tests::merge_selected_into_top_most ... ok
test brush_panel::tests::mixer_brush_options_update_the_persisted_mixer_settings ... ok
test brush_panel::tests::options_bar_edits_are_one_set_brush_per_gesture ... ok
test brush_panel::tests::options_bar_reaches_brush_settings_and_the_preset_library ... ok
test context_bar::tests::move_tool_options_bar_edits_the_selected_shape ... ok
test fill_stroke_proxy::tests::toolbar46_registered_icon_alpha_and_styles_at_16_20_40 ... ok
test layer_reveal::tests::a_new_active_layer_opens_its_groups_and_scrolls_into_view ... ok
test generate_ui::draft_refine::tests::draft_refine_mock_comfy_uses_source_and_library_results_can_be_used_or_opened ... FAILED
test vector_ui::tests::ds_finding_borrowed_shape_options_allow_corner_editing ... ok
test result: FAILED. 1285 passed; 1 failed; 40 ignored; 0 measured; 0 filtered out; finished in 115.32s
error: test failed, to rerun pass `-p photocraft-ui-egui --lib`
test layout_app_acceptance::layout16_app_existing_move_keeps_clip_and_path_translation_together ... ok
test layout_app_acceptance::layout16_app_rtx_pointer_crop_live_release_cancel_undo_zoom_parity ... ok
test layout_app_acceptance::layout16_app_rtx_pointer_resize_constraints_and_point_transform has been running for over 60 seconds
test layout_app_acceptance::layout16_app_rtx_pointer_resize_constraints_and_point_transform ... ok
error: 2 targets failed:
    `-p lightcraft-ui-egui --lib`
    `-p photocraft-ui-egui --lib`
```

**Attribution regression before metadata fix** — exit 101.

```bash
cargo test --offline --target-dir target/clippy -p xtask repository_assets_have_attribution_coverage -- --nocapture > /tmp/bugfix-gates-assets-regression-before.log 2>&1
```

```text
running 1 test
every tracked asset must have provenance and licence coverage: 1356 assets have no attribution row:
    assets::tests::repository_assets_have_attribution_coverage
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 20 filtered out; finished in 2.93s
error: test failed, to rerun pass `-p xtask --bin xtask`
[The complete log lists all 1,356 missing paths.]
```

**Asset-row correction during validation** — exit 1. The first metadata draft used a direct-file glob for nested geometry goldens; it was corrected to `goldens/*/*.svg`. The matcher stayed unchanged.

```bash
cargo run --offline --target-dir target/clippy -p xtask -- assets > /tmp/bugfix-gates-assets-final.log 2>&1
```

```text
Compiling xtask v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-gates/xtask)
    Finished `dev` profile [optimized + debuginfo] target(s) in 17.24s
     Running `/home/zdavidson/.local/share/local-image-dev/bin/li-test-runner target/clippy/debug/xtask assets`
Error: 345 assets have no attribution row:
crates/pc-pathops/tests/tricky/goldens/circle-tangent-line-large-origin/difference.svg
crates/pc-pathops/tests/tricky/goldens/circle-tangent-line-large-origin/division.svg
crates/pc-pathops/tests/tricky/goldens/circle-tangent-line-large-origin/intersect.svg
[Remaining 345 missing paths omitted here; complete log retained.]
```

**Asset audit after corrected metadata** — exit 0.

```bash
cargo run --offline --target-dir target/clippy -p xtask -- assets > /tmp/bugfix-gates-assets-final2.log 2>&1
```

```text
Finished `dev` profile [optimized + debuginfo] target(s) in 0.19s
     Running `/home/zdavidson/.local/share/local-image-dev/bin/li-test-runner target/clippy/debug/xtask assets`
assets: 2041 assets attributed; referenced licence files exist
```

**Attribution catalogue regeneration** — exit 0. Generated file equals HEAD; no catalogue change retained.

```bash
cargo run --offline --target-dir target/clippy -p xtask -- attributions > /tmp/bugfix-gates-attributions.log 2>&1
```

```text
Finished `dev` profile [optimized + debuginfo] target(s) in 0.32s
     Running `/home/zdavidson/.local/share/local-image-dev/bin/li-test-runner target/clippy/debug/xtask attributions`
  235  Algorithms & code ports
    4  Data
   35  AI models
   12  Assets
    4  Upstream projects Local Image grew from
  758  Rust crates
wrote assets/attributions.json (758 crates)
```

**Pure dependency gate** — exit 0.

```bash
cargo run --offline --target-dir target/clippy -p xtask -- deps-pure > /tmp/bugfix-gates-deps-pure.log 2>&1
```

```text
Finished `dev` profile [optimized + debuginfo] target(s) in 0.21s
     Running `/home/zdavidson/.local/share/local-image-dev/bin/li-test-runner target/clippy/debug/xtask deps-pure`
deps-pure: no C-building crates in the dependency graph
```

**Initial strict workspace all-targets Clippy** — exit 0. Compilation output excerpt; historical unused-field blocker was already fixed.

```bash
cargo clippy --workspace --all-targets --offline --target-dir target/clippy --keep-going -- -D warnings > /tmp/bugfix-gates-clippy-baseline.log 2>&1
```

```text
Checking eframe v0.36.2
    Checking egui_kittest v0.36.2
    Checking lightcraft-sysmem v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-gates/crates/lc-sysmem)
    Finished `dev` profile [optimized + debuginfo] target(s) in 3m 00s
```

**Final strict workspace all-targets Clippy** — exit 0.

```bash
cargo clippy --workspace --all-targets --offline --target-dir target/clippy --keep-going -- -D warnings > /tmp/bugfix-gates-clippy-final.log 2>&1
```

```text
warning: lightcraft-engine@2.0.0-alpha.1: no craft-fonts embedded: Chinese and Japanese text will render as empty boxes. Clone https://github.com/storytold/craft-fonts next to this repo and rebuild with CRAFT_FONTS_DIR=../craft-fonts
   Compiling li-attributions v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-gates/crates/li-attributions)
    Checking photocraft-engine v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-gates/crates/pc-engine)
    Checking photocraft-ui-egui v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-gates/crates/pc-ui-egui)
    Checking lightcraft-ui-egui v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-gates/crates/lc-ui-egui)
    Checking local-image v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-gates/apps/local-image)
    Finished `dev` profile [optimized + debuginfo] target(s) in 1m 00s
```

**Workspace check** — exit 0. Final compilation output excerpt.

```bash
cargo check --workspace --offline --target-dir target/clippy > /tmp/bugfix-gates-check.log 2>&1
```

```text
Checking is-wsl v0.4.0
    Checking photocraft-testkit v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-gates/crates/pc-testkit)
    Checking candle-nn v0.9.2
    Checking schemars v1.2.2
    Checking egui-winit v0.36.2
    Checking open v5.4.4
    Checking eframe v0.36.2
    Checking lightcraft-segment v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-gates/crates/lc-segment)
    Checking xtask v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-gates/xtask)
    Checking photocraft-automation v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-gates/crates/pc-automation)
    Checking local-image-cli v2.0.0-alpha.1 (/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-bugfix-gates/apps/local-image-cli)
    Finished `dev` profile [optimized + debuginfo] target(s) in 14m 24s
```

**Original GPU integration discovery** — exit 0.

```bash
target/debug/deps/gpu-0aa56f63f3f81cf9 --list anchor33_gpu_ > /tmp/bugfix-gates-anchor-discovery-before.log
```

```text
0 tests, 0 benchmarks
```

**Original ignored engine mesh gates (saved baseline executable)** — exit 0.

```bash
cp target/debug/deps/photocraft_engine-5657aa4b0e9797f3 /tmp/bugfix-gates-baseline-engine-tests
# Run from crates/pc-engine:
WGPU_BACKEND=vulkan WGPU_POWER_PREF=high WGPU_ADAPTER_NAME='NVIDIA GeForce RTX 5090' __GL_SHADER_DISK_CACHE=0 /home/zdavidson/.local/share/local-image-dev/bin/li-test-runner /tmp/bugfix-gates-baseline-engine-tests mesh43_native_save_load_undo_redo_preview_and_release mesh43_mesh_paint_shapes_rtx5090_at_1x_and_09_scales_preview_release --ignored --test-threads=1 --nocapture > /tmp/bugfix-gates-engine-mesh-before-executed.log 2>&1
```

```text
running 2 tests
test vector_packages::mesh::tests::gpu_tests::mesh43_mesh_paint_shapes_rtx5090_at_1x_and_09_scales_preview_release ... mesh adapter: AdapterInfo { name: "NVIDIA GeForce RTX 5090", vendor: 4318, device: 11141, device_type: DiscreteGpu, device_pci_bus_id: "0000:01:00.0", driver: "NVIDIA", driver_info: "615.71.09", backend: Vulkan, subgroup_min_size: 32, subgroup_max_size: 32, transient_saves_memory: Some(false), limit_bucket: None }
ok
test vector_packages::mesh::tests::mesh43_native_save_load_undo_redo_preview_and_release ... ok

test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 1317 filtered out; finished in 3.83s
```

**Original ignored full-app gates (saved baseline executable)** — exit 0.

```bash
cp target/debug/deps/photocraft_ui_egui-285d2089cce66030 /tmp/bugfix-gates-baseline-ui-tests
# Run from crates/pc-ui-egui:
WGPU_BACKEND=vulkan WGPU_POWER_PREF=high WGPU_ADAPTER_NAME='NVIDIA GeForce RTX 5090' __GL_SHADER_DISK_CACHE=0 /home/zdavidson/.local/share/local-image-dev/bin/li-test-runner /tmp/bugfix-gates-baseline-ui-tests anchor33_gpu_ mesh43_gpu_app_u_point_drag_cancel_and_release --ignored --test-threads=1 --nocapture > /tmp/bugfix-gates-ui-gates-before-executed.log 2>&1
```

```text
running 3 tests
test anchor_tools::app_gpu::anchor33_gpu_full_app_add_delete_convert_preview_release_and_cancel ... anchor33 app GPU: AdapterInfo { name: "NVIDIA GeForce RTX 5090", vendor: 4318, device: 11141, device_type: DiscreteGpu, device_pci_bus_id: "0000:01:00.0", driver: "NVIDIA", driver_info: "615.71.09", backend: Vulkan, subgroup_min_size: 32, subgroup_max_size: 32, transient_saves_memory: Some(false), limit_bucket: None }
anchor33 app GPU: AdapterInfo { name: "NVIDIA GeForce RTX 5090", vendor: 4318, device: 11141, device_type: DiscreteGpu, device_pci_bus_id: "0000:01:00.0", driver: "NVIDIA", driver_info: "615.71.09", backend: Vulkan, subgroup_min_size: 32, subgroup_max_size: 32, transient_saves_memory: Some(false), limit_bucket: None }
anchor33 app GPU: AdapterInfo { name: "NVIDIA GeForce RTX 5090", vendor: 4318, device: 11141, device_type: DiscreteGpu, device_pci_bus_id: "0000:01:00.0", driver: "NVIDIA", driver_info: "615.71.09", backend: Vulkan, subgroup_min_size: 32, subgroup_max_size: 32, transient_saves_memory: Some(false), limit_bucket: None }
ok
test anchor_tools::app_gpu::anchor33_gpu_full_app_curvature_preview_finish_one_journal_and_undo ... anchor33 app GPU: AdapterInfo { name: "NVIDIA GeForce RTX 5090", vendor: 4318, device: 11141, device_type: DiscreteGpu, device_pci_bus_id: "0000:01:00.0", driver: "NVIDIA", driver_info: "615.71.09", backend: Vulkan, subgroup_min_size: 32, subgroup_max_size: 32, transient_saves_memory: Some(false), limit_bucket: None }
ok
test mesh::tests::gpu_tests::mesh43_gpu_app_u_point_drag_cancel_and_release ... ok

test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 1323 filtered out; finished in 11.09s
```

**Unchanged People responsiveness re-check** — exit 0.

```bash
WGPU_BACKEND=vulkan WGPU_POWER_PREF=high WGPU_ADAPTER_NAME='NVIDIA GeForce RTX 5090' __GL_SHADER_DISK_CACHE=0 /home/zdavidson/.local/share/local-image-dev/bin/li-test-runner target/debug/deps/lightcraft_ui_egui-e88ee823a18419f4 ds_finding_large_people_query_keeps_ui_cancellable --test-threads=1 --nocapture > /tmp/bugfix-gates-people-isolated.log 2>&1
```

```text
running 1 test
test tests_people::ds_finding_large_people_query_keeps_ui_cancellable ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 261 filtered out; finished in 4.26s

/home/zdavidson/.local/share/local-image-dev/bin/li-test-runner: line 13: total +  : arithmetic syntax error: operand expected (error token is "+  ")
/home/zdavidson/.local/share/local-image-dev/bin/li-test-runner: line 18: [: : integer expected
```

**Unchanged mock refinement with default profile** — exit 101.

```bash
# Run from crates/pc-ui-egui:
WGPU_BACKEND=vulkan WGPU_POWER_PREF=high WGPU_ADAPTER_NAME='NVIDIA GeForce RTX 5090' __GL_SHADER_DISK_CACHE=0 /home/zdavidson/.local/share/local-image-dev/bin/li-test-runner /tmp/bugfix-gates-baseline-ui-tests draft_refine_mock_comfy_uses_source_and_library_results_can_be_used_or_opened --test-threads=1 --nocapture > /tmp/bugfix-gates-draft-refine-isolated.log 2>&1
```

```text
running 1 test
test generate_ui::draft_refine::tests::draft_refine_mock_comfy_uses_source_and_library_results_can_be_used_or_opened ...
thread 'generate_ui::draft_refine::tests::draft_refine_mock_comfy_uses_source_and_library_results_can_be_used_or_opened' (3496850) panicked at crates/pc-ui-egui/src/generate_ui/draft_refine/tests.rs:399:5:

note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
FAILED

failures:

failures:
    generate_ui::draft_refine::tests::draft_refine_mock_comfy_uses_source_and_library_results_can_be_used_or_opened

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 1325 filtered out; finished in 10.52s
```

**Unchanged mock refinement with isolated AI profile** — exit 0.

```bash
# Run from crates/pc-ui-egui:
LOCAL_IMAGE_DATA_DIR=/tmp/bugfix-gates-isolated-profile LOCAL_IMAGE_MODELS_DIR=/tmp/bugfix-gates-isolated-profile/models WGPU_BACKEND=vulkan WGPU_POWER_PREF=high WGPU_ADAPTER_NAME='NVIDIA GeForce RTX 5090' __GL_SHADER_DISK_CACHE=0 /home/zdavidson/.local/share/local-image-dev/bin/li-test-runner /tmp/bugfix-gates-baseline-ui-tests draft_refine_mock_comfy_uses_source_and_library_results_can_be_used_or_opened --test-threads=1 --nocapture > /tmp/bugfix-gates-draft-refine-isolated-profile.log 2>&1
```

```text
running 1 test
test generate_ui::draft_refine::tests::draft_refine_mock_comfy_uses_source_and_library_results_can_be_used_or_opened ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 1325 filtered out; finished in 2.59s
```

**Release mesh gate compilation** — exit 0. Compilation output excerpt; no test execution in this step.

```bash
cargo test --release --offline --target-dir target/mesh-release -p photocraft-ui-egui --lib mesh43_gpu_app_20_by_20_pointer_frame_budget --no-run > /tmp/bugfix-gates-mesh-release-build.log 2>&1
```

```text
Compiling egui_kittest v0.36.2
    Finished `release` profile [optimized + debuginfo] target(s) in 29m 26s
  Executable unittests src/lib.rs (target/mesh-release/release/deps/photocraft_ui_egui-305602ff86546e9a)
```

**Release mesh gate, normal execution** — exit 0.

```bash
WGPU_BACKEND=vulkan WGPU_POWER_PREF=high WGPU_ADAPTER_NAME='NVIDIA GeForce RTX 5090' __GL_SHADER_DISK_CACHE=0 cargo test --release --offline --target-dir target/mesh-release -p photocraft-ui-egui --lib mesh43_gpu_app_20_by_20_pointer_frame_budget -- --test-threads=1 --nocapture > /tmp/bugfix-gates-mesh-release-executed.log 2>&1
```

```text
warning: lightcraft-engine@2.0.0-alpha.1: no craft-fonts embedded: Chinese and Japanese text will render as empty boxes. Clone https://github.com/storytold/craft-fonts next to this repo and rebuild with CRAFT_FONTS_DIR=../craft-fonts
    Finished `release` profile [optimized + debuginfo] target(s) in 1.53s
     Running unittests src/lib.rs (target/mesh-release/release/deps/photocraft_ui_egui-305602ff86546e9a)

running 1 test
test mesh::tests::gpu_tests::mesh43_gpu_app_20_by_20_pointer_frame_budget ... 20x20 mesh, 120x120 document, 1280x800 full-app offscreen UI+GPU readback: median=46.072503ms p95=78.592337ms p99=89.716749ms release=69.470175ms hardware=GpuInfo { adapter: "NVIDIA GeForce RTX 5090", backend: "vulkan", device_type: "discreteGpu", driver: "NVIDIA 615.71.09", preference: "", selected: "", fallback: None, canvas: "gpu", lost: None, recovery: None }
ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 1322 filtered out; finished in 6.19s
```

Three initial Cargo diagnostic builds (engine `mesh43_ --include-ignored`, UI `anchor33_gpu_ --ignored`, and UI `mesh43_gpu_app_u_point_drag_cancel_and_release --ignored`) were interrupted before test execution, exit 130, to avoid overlap with the workspace's GPU processes. Their logs are retained as aborted build diagnostics; the saved-baseline-binary executions above are the actual results. An incorrectly formed `cargo xtask --offline --target-dir target/clippy assets` alias invocation was also interrupted while awaiting the build lock, exit 130, and replaced with the explicit offline `cargo run -p xtask -- assets` commands above. These are not counted as successful tests or gates.

**Final GPU integration discovery** — exit 0.

```bash
target/debug/deps/gpu-0aa56f63f3f81cf9 --list anchor33_gpu_ > /tmp/bugfix-gates-anchor-discovery-final.log
```

```text
anchor_tools_app::anchor33_gpu_full_app_add_delete_convert_preview_release_and_cancel: test
anchor_tools_app::anchor33_gpu_full_app_curvature_preview_finish_one_journal_and_undo: test

2 tests, 0 benchmarks
```

**Final isolated, serialized workspace tests** — exit 0. Selected historical regressions, enabled gates, environment-sensitive rechecks and affected-target summaries are excerpted below; the complete workspace log includes all 206 summaries.

```bash
LOCAL_IMAGE_DATA_DIR=/tmp/bugfix-gates-final-profile/local-image LOCAL_IMAGE_MODELS_DIR=/tmp/bugfix-gates-final-profile/models XDG_CONFIG_HOME=/tmp/bugfix-gates-final-profile/config XDG_DATA_HOME=/tmp/bugfix-gates-final-profile/data XDG_CACHE_HOME=/tmp/bugfix-gates-final-profile/cache XDG_STATE_HOME=/tmp/bugfix-gates-final-profile/state WGPU_BACKEND=vulkan WGPU_POWER_PREF=high WGPU_ADAPTER_NAME='NVIDIA GeForce RTX 5090' __GL_SHADER_DISK_CACHE=0 cargo test --workspace --offline --no-fail-fast -- --test-threads=1 > /tmp/bugfix-gates-workspace-final.log 2>&1
```

```text
test tests_people::ds_finding_large_people_query_keeps_ui_cancellable ... ok
test layer_multi_cmds::tests::merge_selected_into_top_most ... ok
test vector_packages::mesh::tests::gpu_tests::mesh43_mesh_paint_shapes_rtx5090_at_1x_and_09_scales_preview_release ... ok
test vector_packages::mesh::tests::mesh43_native_save_load_undo_redo_preview_and_release ... ok
test brush_panel::tests::mixer_brush_options_update_the_persisted_mixer_settings ... ok
test brush_panel::tests::options_bar_edits_are_one_set_brush_per_gesture ... ok
test brush_panel::tests::options_bar_reaches_brush_settings_and_the_preset_library ... ok
test context_bar::tests::move_tool_options_bar_edits_the_selected_shape ... ok
test fill_stroke_proxy::tests::toolbar46_registered_icon_alpha_and_styles_at_16_20_40 ... ok
test generate_ui::draft_refine::tests::draft_refine_mock_comfy_uses_source_and_library_results_can_be_used_or_opened ... ok
test layer_reveal::tests::a_new_active_layer_opens_its_groups_and_scrolls_into_view ... ok
test mesh::tests::gpu_tests::mesh43_gpu_app_20_by_20_pointer_frame_budget ... ignored, performance gate requires a release build
test mesh::tests::gpu_tests::mesh43_gpu_app_u_point_drag_cancel_and_release ... ok
test vector_ui::tests::ds_finding_borrowed_shape_options_allow_corner_editing ... ok
test anchor_tools_app::anchor33_gpu_full_app_add_delete_convert_preview_release_and_cancel ... ok
test anchor_tools_app::anchor33_gpu_full_app_curvature_preview_finish_one_journal_and_undo ... ok
test layout_app_acceptance::layout16_app_existing_move_keeps_clip_and_path_translation_together ... ok
test layout_app_acceptance::layout16_app_rtx_pointer_crop_live_release_cancel_undo_zoom_parity ... ok
test layout_app_acceptance::layout16_app_rtx_pointer_resize_constraints_and_point_transform ... ok
test assets::tests::repository_assets_have_attribution_coverage ... ok

     Running unittests src/lib.rs (target/debug/deps/lightcraft_ui_egui-e88ee823a18419f4)
test result: ok. 258 passed; 0 failed; 4 ignored; 0 measured; 0 filtered out; finished in 482.25s
     Running unittests src/lib.rs (target/debug/deps/photocraft_engine-5657aa4b0e9797f3)
test result: ok. 1289 passed; 0 failed; 30 ignored; 0 measured; 0 filtered out; finished in 151.07s
     Running unittests src/lib.rs (target/debug/deps/photocraft_ui_egui-285d2089cce66030)
test result: ok. 1287 passed; 0 failed; 37 ignored; 0 measured; 0 filtered out; finished in 263.96s
     Running tests/gpu/main.rs (target/debug/deps/gpu-0aa56f63f3f81cf9)
test result: ok. 88 passed; 0 failed; 16 ignored; 0 measured; 0 filtered out; finished in 252.51s
     Running unittests src/main.rs (target/debug/deps/xtask-6a5b7fbed43fedde)
test result: ok. 21 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.68s
```

After the final run, the generated tracked ellipse PNG was restored from HEAD and only the two test-created review-empty-board PDF/PNG scratch files were removed. The owner's task edit is preserved. No generated fixture changes are retained.

**Asset audit after scratch cleanup** — exit 0.

```bash
cargo run --offline --target-dir target/clippy -p xtask -- assets > /tmp/bugfix-gates-assets-clean-final.log 2>&1
```

```text
    Finished `dev` profile [optimized + debuginfo] target(s) in 0.48s
     Running `/home/zdavidson/.local/share/local-image-dev/bin/li-test-runner target/clippy/debug/xtask assets`
assets: 2041 assets attributed; referenced licence files exist
```

**Final formatting and patch checks** — both exit 0, with no output.

```bash
cargo fmt --all -- --check > /tmp/bugfix-gates-fmt-final.log 2>&1
git diff --check > /tmp/bugfix-gates-diff-check-final.log 2>&1
```

Final inspection confirms the only retained changes are the attribution metadata/regression, anchor registration/lock consolidation, mesh gate activation, this report and the owner's pre-existing task edit. Cargo manifests, Cargo.lock and the regenerated attribution catalogue are unchanged. The final source was checked with workspace all-targets Clippy and workspace check before the complete workspace test run; no Rust source changed afterwards.
