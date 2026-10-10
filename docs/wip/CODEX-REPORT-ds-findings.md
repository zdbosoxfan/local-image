# DeepSeek findings verification and fixes

Worktree: `wip/codex-ds-findings`, continued from builder commit `053ac496`. All changes are uncommitted. Original review: 264 findings (69 high, 143 medium, 52 low). IDs below are one-based JSONL line numbers; file paths refer to the current branch, not the stale review line numbers.

Completed both review lists. All 149 confirmed findings have fixes and regression coverage; the separately requested safetensors metadata defect is also fixed. The nine explicitly excluded findings remain deferred for the coordinator. Validation results and limitations are recorded below.

## Original review counts

| Severity | REAL | NOT A BUG | ALREADY FIXED / OBSOLETE | DEFERRED | Total |
| --- | ---: | ---: | ---: | ---: | ---: |
| high | 14 | 50 | 4 | 1 | 69 |
| medium | 65 | 72 | 3 | 3 | 143 |
| low | 29 | 20 | 1 | 2 | 52 |
| Total | 108 | 142 | 8 | 6 | 264 |

## Second review counts

| Severity | REAL | NOT A BUG | ALREADY FIXED / OBSOLETE | DEFERRED | NOT IN BRANCH | Total |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| high | 3 | 11 | 2 | 0 | 0 | 16 |
| medium | 29 | 23 | 0 | 3 | 0 | 55 |
| low | 9 | 5 | 0 | 0 | 0 | 14 |
| Total | 41 | 39 | 2 | 3 | 0 | 85 |

Combined: 349 reviewed findings, 149 REAL, 181 NOT A BUG, 10 ALREADY FIXED / OBSOLETE, 9 DEFERRED. The second review referenced code already present in this branch; no finding required NOT IN BRANCH. Deferred IDs: 30, 31, 130, 134, 142, 143; 2-24, 2-25, 2-37. No pending review entries remain.

## Verification and reference basis

The review retained supported caller invariants, documented behavior and ported algorithms where the claim was speculative. Removed features were not reintroduced. The six original deferrals concern Library host/Develop-layer and model detection. The second review adds three composition group/shape/mask deferrals. These are the active rewrites excluded by the task.

Numerical reference work used `docs/PORTS.md`, `docs/upstream/`, checked-in reference vectors and read-only upstream checkouts under `~/.local/share/local-image-dev/engine-sources/`. In particular:

- darktable `733bd69f32cac7ff5e41025115942772add1f088`: local Laplacian/log floors, guided filter limiting behavior, VNG color conditions and segmentation boundary conventions. The broken quickselect partition was corrected against an independent sorted oracle rather than copied into the test.
- VTracer `928ed0a6f654408e28fb741b6133d4c456bd0160` and Potrace 1.16: private contour-domain invariants and diagonal connectivity policy.
- `docs/specs/smart-sort.md`: exact deterministic Chinese Whispers at cosine 0.5, sorted updates, 30 rounds and smallest-label ties; export-original directory policy. Dense clustering preserves this algorithm and bounds graph storage rather than approximating neighbors.
- egui 0.36.2 registry source: `Popup::menu`, retained temporary data, texture-handle ownership, image `RectShape` painting and public layer hit testing. Arc/Weak snapshot invalidation follows Rust `Arc::make_mut` ownership guarantees.
- [ComfyUI server](https://github.com/Comfy-Org/ComfyUI/blob/master/server.py) and [OpenAPI](https://github.com/Comfy-Org/ComfyUI/blob/master/openapi.yaml): atomic job-specific cancellation, avoiding global interrupt races.
- [freedesktop Trash 1.0](https://specifications.freedesktop.org/trash/1.0/) and [RFC 9636](https://www.rfc-editor.org/rfc/rfc9636.html): filename bytes, filesystem selection, local DeletionDate and TZif/POSIX footer offsets. The pure-Rust timezone reader adds no dependency.

Additional primary references used for the second review:

- [ComfyUI VAEEncode](https://github.com/Comfy-Org/ComfyUI/blob/master/nodes.py) and [Qwen Image 2.1 encoder documentation](https://github.com/Comfy-Org/embedded-docs/blob/main/comfyui_embedded_docs/docs/TextEncodeQwenImage21/en.md): an empty latent requires full denoise; partial resampling starts from encoded source pixels, while reference conditioning remains intact.
- [ag-psd descriptor enums](https://github.com/Agamnentzar/ag-psd/blob/master/src/descriptor.ts) and [psd-tools terminology](https://github.com/psd-tools/psd-tools/blob/main/src/psd_tools/terminology.py): slice layer origin and vertical enum identifiers are already compatible; the review confused enum values with descriptor keys.
- [Rust Unix Child implementation](https://doc.rust-lang.org/src/std/sys/process/unix/unix.rs.html): a completed Child retains its status and does not signal a recycled PID.
- [ModelSpec](https://github.com/Stability-AI/ModelSpec), the existing safetensors header parser and kohya metadata conventions: the separately requested metadata-classification fix retains tensor shapes and nested metadata and recognizes ss_base_model_version independently of the LoRA-only path. `ds2_metadata_flat_modelspecific_and_kohya_classification` and `ds2_metadata_safetensors_header_keeps_tensor_shapes_and_metadata` both failed before the fix (saved in target/ds/logs/ds2-metadata-before.log), then passed. The excluded arch/inventory rewrite was touched only for this explicitly authorized defect; the original excluded findings stay deferred.

Project-specific decisions, where no external recipe applies: preserve a single committed-settings thumbnail during live edits; invalidate imported-identity snapshots through existing catalog revisions; use a 256-entry headshot texture cache with at most four concurrent decodes; clear Move viewport resources at existing gesture cleanup points. Regression tests exercise the actual caller paths.

No GUI windows or owner documents were opened. Tests use owned temporary fixtures; generated-image libraries and presets are isolated under this worktree. No new dependencies, unsafe blocks, C/C++ or build-time `-sys` crates were introduced. Cargo work is offline with Rust 1.98.1 and three build jobs, one build at a time in this worktree.

## Validation

The full suites of the 19 changed crates ran once, offline with Rust 1.98.1, three build jobs and three test threads. The run produced **5,107 passes, 10 failures and 54 ignored tests**. All ten failures were addressed and rechecked in a **16-test filtered run with zero failures**; unrelated full suites were not repeated. The table below combines the full run with the successful rechecks of its failed cases, without double-counting repeated tests.

| Crate | Passing results after rechecks | Full-run failures fixed/rechecked | Ignored in full run |
| --- | ---: | ---: | ---: |
| `li-ai` | 360 | 1 | 0 |
| `li-attributions` | 5 | 0 | 0 |
| `li-seg` | 49 | 0 | 4 |
| `lightcraft-catalog` | 188 | 0 | 1 |
| `lightcraft-color` | 31 | 0 | 0 |
| `lightcraft-develop` | 96 | 0 | 0 |
| `lightcraft-engine` | 705 | 2 | 5 |
| `lightcraft-gpu` | 91 | 0 | 5 |
| `lightcraft-pipeline` | 408 | 3 | 3 |
| `lightcraft-raster` | 13 | 0 | 0 |
| `lightcraft-raw` | 173 | 0 | 4 |
| `lightcraft-ui-egui` | 250 | 2 | 3 |
| `local-image` | 81 | 0 | 1 |
| `pc-trace` | 19 | 0 | 3 |
| `photocraft-engine` | 1448 | 0 | 16 |
| `photocraft-gpu` | 88 | 0 | 0 |
| `photocraft-testkit` | 8 | 0 | 0 |
| `photocraft-ui-egui` | 1085 | 1 | 9 |
| `xtask` | 19 | 1 | 0 |
| Total | 5,117 | 10 | 54 |

Final clippy passed for all 19 crates with `CARGO_TARGET_DIR=target/clippy`, `--offline --all-targets -- -D warnings`. Final `cargo +1.98.1 fmt ... -- --check` and `git diff --check` passed. Cargo commands used `CARGO_BUILD_JOBS=3` and never overlapped in this worktree. The shared compiler gate was retained. Commands and outputs are saved in `target/ds/logs/final-{full,focused,clippy,fmt-check}.{log,json}`; per-crate totals are in the corresponding `.counts.json` files. The first full-command attempt rejected an unsupported Cargo flag before building or running tests; it is preserved separately as `final-full-arguments.*` and is not counted as a suite run.

The ten final fixes/rechecks comprise:

- UTC-only Trash assertions now run in a child with `TZ=UTC0`; the public formatter still uses local time. Its existing pre-epoch clamp is preserved. Local timezone/DST regressions also pass.
- The face-gate fixture now uses a valid min/max range and separately checks manual override and zero known faces. Contradictory gates retain their new rejection test.
- Saved session restrictions remain dormant when sessions are disabled, as the existing product contract requires. The speculative change from original finding 43 was reverted and that finding is NOT A BUG.
- Haze reference selection now uses an independent C `qsort` oracle in the out-of-build harness. The unchanged original upstream fixture reproduces the bad rank; `detail-haze-sorted.csv` changes only the selection helper and keeps the original tight tolerances. This difference is documented in the fixture README and darktable notice.
- The two historical-settings render goldens were re-recorded after the verified numerical corrections. All settings hashes remain unchanged, and historical JSON load/round-trip assertions remain active, as required by the task.
- The explicit Create a folder action now carries its event back from the owned People workspace without overriding an unchecked People folders switch on later frames. Both the existing person-folder UI test and the new Back/forward checkbox test pass.
- The CPU slider timing test runs in a child with `LIGHTCRAFT_GPU=0`, isolating it from other headless apps' process-wide GPU preferences. GPU timing is checked separately on the RTX 5090.
- The Refine validation label `Select a layer first.` follows the existing pending-AI-translation list and runtime English fallback while translation work is paused.
- Attribution parsing distinguishes a Markdown licence link target from its backticked display label, including root filenames and mixed local/remote references. The expanded asset regression was also run directly and passed.

The separately filtered `compare_highlights` example regression passed (1 test). The normally ignored `tests_people::people_ui_real_gpu_interactions` was run explicitly on the hardware GPU and passed (1 test), including a final rerun after the People handoff fix. Thus one of the 54 full-run ignored tests has independent passing hardware evidence; the remaining optional/benchmark ignored tests were left under their existing attributes.

### Real GPU and app-path evidence

Adapter: **NVIDIA GeForce RTX 5090**, Vulkan, NVIDIA driver **615.71.09**, 32,607 MiB reported by `nvidia-smi`. `LC_REQUIRE_GPU=1` and `LOCAL_IMAGE_REQUIRE_GPU=1` were set. Native tests select the app's `wgpu_setup`, restrict Vulkan discovery to the NVIDIA ICD and reject CPU adapters. No no-adapter GPU skips occurred. Device-loss tests intentionally exercise their CPU recovery paths.

Added GPU regressions passed:

- Library kernels: `ds_finding_native_haze_selection_matches_sorted_quantile`, `ds_finding_hidden_overlay_uses_primary_image`, `ds_finding_zero_skin_hue_range_is_finite_on_gpu`.
- Native Compositing UI: `ds_finding_encode_pool_releases_unused_tiles`, `ds_finding_real_app_navigator_job_finishes_during_drag`, and `ds2_move_cold_press_defers_cache_and_release_or_recovery_frees_viewport_resources`. These use the real app renderer and pointer events, including release and injected transient recovery cleanup.

Debug-build measurements are diagnostics from the real app paths, not release performance guarantees. The headless Library harness uses the desktop UI/control/photo-render paths and CPU rasterization for the UI itself; the native Move/Navigator harness includes actual GPU rendering and screenshot readback.

| Path | Focused hardware run | Full-suite / final hardware run |
| --- | --- | --- |
| 2,000-photo People view | 0.34 ms per UI frame; queue opening 20.76 ms; Yes plus live reranking 17.55 ms | Final explicit rerun: 0.20 ms per UI frame; queue opening 14.16 ms; Yes plus live reranking 15.52 ms |
| Native Move, 400×300 document, 1,200×800 harness | 20 rendered/readback frames: median 11.20 ms, max 22.58 ms | 20 frames: median 32.85 ms, max 39.53 ms |
| Native 5 MP Navigator while pointer remains held | 7 rendered/readback frames: median 25.20 ms, max 46.06 ms | 10 frames: median 19.42 ms, max 22.94 ms |

Logs: `ds-people-real-gpu.log`, `ds-navigator-hardware-discovery.log`, `ds2-ui-engines-fixed.log`, `final-people-gpu.log` and `final-full.log` under `target/ds/logs/`. Timing variation includes the shared machine's other work and debug/readback costs. These measurements apply to the fixtures above; the optional 48 MP benchmark remains ignored.

### Remaining validation limits

ComfyUI was checked again at 127.0.0.1:8188: both `/queue` and `/object_info/VAEEncode` refused connections. Graph construction, mock cancellation, request validation and Qwen latent-source regressions pass; **live model inference was not verified**. No service was started or owner process stopped. Availability evidence is in `comfy-availability-final.json`.

Some existing tests report success after conditionally skipping absent external fixtures: the RAW corpus (including NEF/CR2/ARW/RW2 camera samples), the Japanese watermark font without `CRAFT_FONTS_DIR`, and the vector competitor/external corpus references. Those branches are not claimed as exercised; the full log records each skip. Checked-in numeric reference vectors and synthetic regressions ran. No owner documents, new dependencies or unsafe code were introduced. All source changes remain uncommitted at `053ac496`.


## Findings

| ID | File | Severity | Verdict | Fix and regression, or reason |
| --- | --- | --- | --- | --- |
| 1 | `crates/lc-develop/src/presets.rs` | high | ALREADY FIXED / OBSOLETE | Enhance no longer has ai/denoise fields; copying cannot apply a source AI reference. |
| 2 | `crates/lc-develop/src/lib.rs` | medium | NOT A BUG | SkinTone has no enabled field; on/off is managed by disabled_sections, which reset_section leaves intact. |
| 3 | `crates/lc-develop/src/lib.rs` | medium | NOT A BUG | effects and curve are always serialized top-level fields; only their optional subkeys are skipped. |
| 4 | `crates/lc-codecs/src/jpeg_par.rs` | medium | NOT A BUG | Degenerate dimensions cannot encode any JPEG; documented empty-codestream sentinel avoids a panic. Export entry points validate nonempty images. Replacing it with another invalid sentinel would not fix a supported image. |
| 5 | `crates/lc-engine/src/cmd/prefs.rs` | medium | REAL | Validate every preference before side effects; ds_finding_invalid_preferences_have_no_partial_effects. |
| 6 | `crates/lc-develop/src/controls.rs` | high | ALREADY FIXED / OBSOLETE | enhance.denoise and its callers were deliberately removed with AI Denoise. |
| 7 | `crates/lc-engine/src/dcp_profiles.rs` | medium | NOT A BUG | One-time explicit profile-folder configuration scans names to select the lexicographically first 4096 deterministically; million-profile UI freeze is hypothetical, not an established hot path. |
| 8 | `crates/lc-engine/src/cmd/export.rs` | medium | REAL | Use only import report IDs and one canonical-path map; propagate stack.group errors. ds_finding_add_export_imports_only_new_ids_and_reports_stack_failure uses actual PNG imports, a skipped duplicate and an exhausted stack ID allocator. |
| 9 | `crates/lc-engine/src/cmd/export.rs` | medium | REAL | Use only import report IDs and one canonical-path map; propagate stack.group errors. ds_finding_add_export_imports_only_new_ids_and_reports_stack_failure uses actual PNG imports, a skipped duplicate and an exhausted stack ID allocator. |
| 10 | `crates/lc-engine/src/cmd/smart_sort.rs` | high | NOT A BUG | serde_json Value object IndexMut inserts absent keys. Existing opt_in_default_and_command_registry_privacy test exercises faces=true. |
| 11 | `crates/lc-engine/src/cmd/smart_sort.rs` | medium | NOT A BUG | Smart Sort spec uses multi-selection, otherwise the current view. Explicit ids accepts a single photo. |
| 12 | `crates/lc-engine/src/cmd/smart_sort.rs` | high | NOT A BUG | Smart Sort UI uses smart_sort_task::start/run_inputs on its worker; this synchronous engine command serves CLI/automation. |
| 13 | `crates/lc-engine/src/cmd/develop.rs` | medium | REAL | Reset keeps the freshly imported default look. ds_finding_reset_restores_import_preset_look. |
| 14 | `crates/lc-engine/src/camera_profiles.rs` | low | NOT A BUG | The camera key is OnceLock constant within a process; only the DCP key changes. XOR with a constant is injective, so two mutable keys cannot cancel here. |
| 15 | `crates/lc-engine/examples/compare_highlights.rs` | low | REAL | Identify the skin scene by name rather than filtered enumeration index; ds_finding_filtered_skin_scene_retains_rois (example test). |
| 16 | `crates/lc-engine/src/cmd/masks.rs` | low | REAL | Borrow ordinary settings; filter foreign patches once before dust source selection, and extend the spot list after the loop. ds_finding_dust_source_settings_filter_foreign_once. |
| 17 | `crates/lc-engine/src/camera_preview.rs` | medium | NOT A BUG | fit_matrix checks the unregularized Gram condition (limit 1e6) and rejects singular neutral data; ridge and held-out fitting validate the result. |
| 18 | `crates/lc-engine/src/cmd/save_over.rs` | medium | NOT A BUG | Full-file injected ByteReader is for virtual/wasm files and returns the caller-owned Vec by contract; native disk RAW signature probing reads only 64 KiB. No production native reader triggers the alleged full-file read. |
| 19 | `crates/lc-engine/src/cmd/save_over.rs` | medium | REAL | Reject mismatched Cb/Cr sampling; ds_finding_jpeg_requires_matching_chroma_components. |
| 20 | `crates/lc-engine/src/media.rs` | high | REAL | Keep camera tone and look curve independent in smart-preview fallback; ds_finding_smart_preview_keeps_distinct_look_curve. |
| 21 | `crates/lc-engine/src/media.rs` | medium | NOT A BUG | camera_profiles::cache_key already incorporates the DCP cache key; the thumbnail signature changes when DCP configuration changes. |
| 22 | `apps/local-image/src/app_log.rs` | low | REAL | Cap oversized UTF-8 lines before rotation; expanded app_log capped-log regression. |
| 23 | `crates/lc-catalog/src/rules.rs` | medium | NOT A BUG | Current problems/describe/UI use ops_for_field, which has person-specific anyOf/allOf operators. |
| 24 | `crates/lc-catalog/src/rules.rs` | low | NOT A BUG | Empty person selections deliberately fail closed, including explicit !ids.is_empty() in allOf. Smart-album rules do not use mathematical vacuous truth for incomplete filters. |
| 25 | `crates/lc-engine/src/preset_luminar.rs` | low | REAL | Skip nonfinite layer Amount; ds_finding_nonfinite_layer_amount_is_skipped. |
| 26 | `crates/lc-develop/src/settings.rs` | medium | REAL | Route skinTone controls to skin_tone layer tools; ds_finding_layer_skin_tone_controls_are_routed. |
| 27 | `crates/lc-develop/src/settings.rs` | high | ALREADY FIXED / OBSOLETE | Removed feature; old settings still deserialize via serde defaults, as required. Identical old rendering is explicitly not required. |
| 28 | `crates/lc-engine/src/cmd/smart_sort_people.rs` | medium | NOT A BUG | PeopleEngine::ready initializes data/cache only; model file verification happens in tagger(), so missing models do not make this status error. |
| 29 | `crates/lc-engine/src/smart_sort/presets.rs` | medium | REAL | Reject min_faces greater than max_faces; ds_finding_contradictory_face_gate_is_rejected. |
| 30 | `apps/local-image/src/library_host.rs` | medium | DEFERRED | Develop-layer/Library host area is being rewritten by another job per task; leave this file untouched. |
| 31 | `apps/local-image/src/library_host.rs` | low | DEFERRED | Library host Develop-layer open path is being rewritten by another job and is explicitly excluded by the task. |
| 32 | `crates/lc-engine/src/smart_sort/classify.rs` | low | REAL | Recognize the photo prefix at a word boundary; existing prompt regression now includes photographer. |
| 33 | `crates/lc-engine/src/import.rs` | medium | NOT A BUG | An unedited import can show its embedded preview while the true render arrives; edited default presets disable that shortcut. Product deliberately prioritizes initial display. |
| 34 | `crates/lc-engine/src/import.rs` | low | NOT A BUG | Import-picker candidates are quick preferred-look thumbnails before applying a default preset. The eventual imported photo render applies its default edits; candidate equivalence is not promised. |
| 35 | `crates/lc-engine/src/export.rs` | high | REAL | Use effective_depth for source-profile encoding; source_profile_encoding regression includes TIFF/PNG/JPEG/WebP implicit depth. |
| 36 | `crates/lc-engine/src/export.rs` | medium | NOT A BUG | run_batch honors to.exact for a single photo without per-item subfolder even with same_folder=true, matching planned_paths. |
| 37 | `crates/lc-gpu/src/toneeq_trace.rs` | high | NOT A BUG | Private scoped test snapshots are recorded only with nonempty, matching CPU/GPU planes and positive dimensions; no invalid-image caller. |
| 38 | `crates/lc-engine/src/smart_sort/sessions.rs` | high | REAL | Borrow one indexed photo/session map per planning and token-resolution pass; ds_finding_index_preserves_first_membership_and_missing_time. |
| 39 | `crates/lc-engine/src/smart_sort/examples.rs` | medium | NOT A BUG | Store validates model dimensions and classifier produces the same dimension; every production candidate/prototype comes from that store. |
| 40 | `crates/lc-gpu/src/ctx.rs` | medium | NOT A BUG | Exit path stops accepting jobs before quiesce. RUNNING counts queued initialization/render work before spawning, so the alleged concurrent-new-render caller is absent. |
| 41 | `crates/lc-gpu/src/ctx.rs` | low | ALREADY FIXED / OBSOLETE | Current quiesce computes remaining deadline before device.poll. |
| 42 | `crates/lc-gpu/src/capture/tests.rs` | high | NOT A BUG | render::rgb_words returns &[f32], not &[u32]. |
| 43 | `crates/lc-engine/src/smart_sort/plan.rs` | medium | NOT A BUG | Saved session restrictions intentionally remain dormant when sessions are disabled; p2b_sessions_commands_narrow_tag_custom_people_and_no_time_folders establishes this contract. Restored the original predicate and added ds_finding_session_restrictions_are_dormant_without_sessions. |
| 44 | `crates/lc-engine/src/smart_sort/plan.rs` | high | NOT A BUG | docs/specs/smart-sort.md requires refusing a root equal to an originals folder, not all descendants; current check follows that policy. |
| 45 | `crates/lc-engine/src/smart_sort/plan.rs` | medium | REAL | Resolve distinct original parents and share one OriginalGuard across the complete export; ds_finding_destination_guard_reuses_distinct_original_parents, plus existing forbidden-destination export tests. |
| 46 | `crates/lc-gpu/src/primary.rs` | high | NOT A BUG | GPU alpha comes from evaluate_with_tone, which returns only visible nonempty masks; compressed indices match its layout and scoped caches include content keys. |
| 47 | `crates/lc-gpu/src/primary.rs` | high | NOT A BUG | CPU primary::process deliberately mixes tools at their pipeline stages too; colour receives tone-mixed pixels once and does not reapply tone. |
| 48 | `apps/local-image/src/main.rs` | low | REAL | Remove inherited GPU failure environment before selecting a retry backend; ds_finding_gpu_retry_clears_inherited_cpu_failure_reason. |
| 49 | `crates/lc-engine/src/smart_sort/people.rs` | high | REAL | Retain manually named face identities independent of detector matches; ds_finding_manual_region_keeps_identity_without_detection. |
| 50 | `crates/lc-engine/src/smart_sort/people.rs` | medium | REAL | Compute all identity rankings and margins once, reused by bubbles and projection; ds_finding_people_shared_pass_preserves_margins_and_rejections. |
| 51 | `crates/lc-engine/src/smart_sort/people.rs` | low | REAL | Checked identity-ID allocation returns an error at exhaustion; ds_finding_identity_exhaustion_is_an_error_without_mutation. |
| 52 | `crates/lc-engine/src/smart_sort/faces_store.rs` | medium | NOT A BUG | Face matching uses actual cosine, which normalizes its operands. Loaded finite non-unit embeddings do not produce incorrect matching. |
| 53 | `crates/lc-engine/src/smart_sort/faces_store.rs` | medium | REAL | Recreate externally deleted append caches atomically from loaded plus pending records; deleted face/embedding cache regressions. |
| 54 | `crates/lc-engine/src/smart_sort/store.rs` | medium | REAL | Recreate externally deleted append caches atomically from loaded plus pending records; deleted face/embedding cache regressions. |
| 55 | `crates/lc-engine/src/smart_sort/store.rs` | low | NOT A BUG | Full-file copying is required by the atomic cache-write policy. Production analysis skips already cached content keys; duplicate rewrites from arbitrary repeated insert calls are not a demonstrated hot path. |
| 56 | `crates/lc-engine/src/smart_sort/tokens.rs` | medium | NOT A BUG | Token components are deliberately sanitized by trimming boundary dots/space for portable export names; file patterns provide stems, with the format extension appended by prepare_preset. |
| 57 | `crates/lc-engine/src/smart_sort/tokens.rs` | low | REAL | Validate ASCII calendar dates; keep per-base suffix counters for linear duplicate reservation. ds_finding_duplicate_folders_and_bad_date. |
| 58 | `crates/lc-engine/src/smart_sort/tokens.rs` | low | REAL | Validate ASCII calendar dates; keep per-base suffix counters for linear duplicate reservation. ds_finding_duplicate_folders_and_bad_date. |
| 59 | `crates/lc-engine/src/enhance/remove.rs` | medium | REAL | Return the actual dilated seamless-blend mask with healed pixels, preserving the solved apron in stored patches; expanded local_heal regression. |
| 60 | `crates/lc-pipeline/src/lib.rs` | high | NOT A BUG | Destructured fields are Copy; .. does not move frame or settings. |
| 61 | `crates/lc-pipeline/src/lib.rs` | high | NOT A BUG | Rust infers n as f32 from its use in f32 division; floating literals do not force f64. |
| 62 | `crates/lc-pipeline/src/finish.rs` | medium | NOT A BUG | Refine Saturation is deliberately a master RGB-curve control; luminance master already preserves chromaticity, and per-channel colour edits remain intentional. |
| 63 | `crates/lc-pipeline/src/local.rs` | high | NOT A BUG | FinishParams layers and Prepared masks/sharpening all use the same filtered-mask indices. |
| 64 | `crates/lc-engine/src/smart.rs` | low | REAL | Include newline and JPEG SOI bytes in the header-size budget; ds_finding_header_limit_includes_jpeg_start. |
| 65 | `crates/lc-pipeline/src/layers.rs` | medium | NOT A BUG | StageCache partitions by geo/lin keys including output size, source scale and sensor facts; layer NR cache cannot span those changes. |
| 66 | `crates/lc-pipeline/src/balance.rs` | high | NOT A BUG | f32::from_bits(0) is 0.0, not 1.0; the alleged black-to-white calculation is false. |
| 67 | `crates/lc-pipeline/src/balance.rs` | high | NOT A BUG | white and grey are reference normalization points, not adjustments. With neutral midtone/contrast exponents they cancel algebraically, so identity is correct. |
| 68 | `crates/lc-color/src/profile.rs` | medium | REAL | Evaluate Hermite coefficients in f64 to handle subnormal intervals; ds_finding_subnormal_tone_intervals_are_finite. |
| 69 | `crates/lc-color/src/profile.rs` | low | REAL | Build blended table metadata directly without cloning the unused source data; blend_and_tables_round_trip_through_prophoto checks blended values and metadata. |
| 70 | `crates/lc-pipeline/src/primary.rs` | high | NOT A BUG | evaluate_with_tone filters invisible/empty masks internally, so zip uses matching sequences. |
| 71 | `crates/lc-pipeline/src/primary.rs` | medium | REAL | Clamp Skin Tone hue range to 1e-3 radians on CPU and GPU to prevent a zero-width smoothstep. Real-GPU zero-range render agrees with CPU. ds_finding_zero_skin_hue_range_is_finite_on_gpu runs on the real RTX 5090. |
| 72 | `crates/lc-pipeline/src/metrics.rs` | high | REAL | Guard empty/different dimensions, out-of-range rows and edge positions in halo; expanded halo regression. |
| 73 | `crates/lc-pipeline/src/metrics.rs` | high | REAL | Guard empty/different dimensions, out-of-range rows and edge positions in halo; expanded halo regression. |
| 74 | `crates/lc-pipeline/src/metrics.rs` | medium | REAL | Guard empty/different dimensions, out-of-range rows and edge positions in halo; expanded halo regression. |
| 75 | `crates/lc-pipeline/src/metrics.rs` | medium | REAL | Compare only shared lightness samples; ds_finding_lightness_order_shared_samples. |
| 76 | `crates/lc-engine/src/files.rs` | high | REAL | Normalize ordinary raw samples on demand, retain full buffers for opcodes and non-Bayer paths; sensor-confidence comparison and existing binned/opcode tests. |
| 77 | `crates/lc-pipeline/src/toneeq.rs` | medium | NOT A BUG | local::prepare/render GPU applies mask_exposure and mask_contrast when constructing the guided mask. of only removes the global exposure already applied to that mask. |
| 78 | `crates/lc-pipeline/src/tone.rs` | high | NOT A BUG | 0.75 EV is the deliberate calibrated scene base exposure (also display/camera constructors and regression expectations). |
| 79 | `crates/lc-pipeline/src/tone.rs` | medium | NOT A BUG | preset index drives exact basecurves evaluation and is serialized. Dummy fixed-size knots are unused when preset is Some; original variable-length knots are kept in basecurves. |
| 80 | `crates/lc-pipeline/src/tone.rs` | low | NOT A BUG | PartialEq compares stored data, not rendering equivalence. Different serialized knots can conservatively invalidate a cache but produce no incorrect output. |
| 81 | `crates/lc-raw/examples/quality_support/mod.rs` | medium | NOT A BUG | Example-only helper is called with fixed border 4/16 on >=64px equal-sized synthetic frames; invalid border is unreachable. |
| 82 | `crates/lc-raw/examples/quality_support/mod.rs` | medium | NOT A BUG | Quality examples always compare same-sized outputs to their explicitly resized truth. |
| 83 | `crates/lc-pipeline/src/eigf.rs` | high | REAL | Use the finite constant-guide limiting coefficient at zero variance/feather; constant-guide guided-filter regression. |
| 84 | `crates/lc-pipeline/src/eigf.rs` | medium | REAL | Floor guides before logarithmic quantization; ds_finding_quantized_negative_guide_stays_finite. |
| 85 | `crates/lc-pipeline/src/colorequal.rs` | medium | NOT A BUG | Pinned darktable colorequal.c exponentiates the cosine-series kernel; reference fixtures check this form. |
| 86 | `crates/lc-raw/src/highlight/opposed.rs` | medium | NOT A BUG | Reference regeneration deliberately samples every third pixel. The fixture length is 9216 floats and matches this sampling. |
| 87 | `crates/lc-raw/src/demosaic/mod.rs` | medium | NOT A BUG | vng bool is named linear_only, not four_colour; false selects the full VNG pass. |
| 88 | `crates/lc-raster/src/blur.rs` | low | REAL | Saturate min-filter window arithmetic; ds_finding_min_filter_extreme_radius. |
| 89 | `crates/lc-raw/examples/quality.rs` | medium | NOT A BUG | psnr third argument is border width, not max pixel value; peak is fixed 1.0. |
| 90 | `crates/lc-raw/src/dcp.rs` | medium | NOT A BUG | Model strings often include make already; fallback supports that metadata. Inconsistent make/model is hypothetical and not a reproducible supported camera defect. |
| 91 | `crates/lc-pipeline/src/detail/haze.rs` | medium | REAL | Reproduced CPU and RTX 5090 Vulkan selector returning 3 instead of 4 for [1,4,3,2]. CPU uses standard selection; GPU excludes the pivot and uses a distinct middle sample. Sorted independent oracle covers small/large repeated/random inputs, dark and brightness phases; GPU regression passes. ds_finding_native_haze_selection_matches_sorted_quantile runs on the real RTX 5090. |
| 92 | `crates/lc-pipeline/src/tone2.rs` | medium | REAL | Apply the soft shoulder only for positive Whites; ds_finding_display_tone_preserves_white_without_positive_whites. |
| 93 | `crates/lc-gpu/src/render.rs` | medium | NOT A BUG | Haze preparation depends only on source pixels and scale, not slider values; planes cache key contains linear/primary key, which includes geometry/sensor scale. Both signs are prepared and strength is applied at finish. |
| 94 | `crates/lc-gpu/src/render.rs` | medium | NOT A BUG | Chroma is consumed by local mask adjustments. With no visible mask terms there is no such consumer; global color denoise runs earlier in denoise_native. A reserved zero plane cannot disable a visible mask: its nonempty list always appends chroma. |
| 95 | `crates/lc-gpu/src/render.rs` | medium | REAL | Hidden overlays now read prepared_lin after primary adjustments, matching visible-mask evaluation. Real-GPU hidden ColorRange overlay regression compares the final CPU/GPU image. ds_finding_hidden_overlay_uses_primary_image runs on the real RTX 5090. |
| 96 | `crates/lc-ui-egui/src/export_task.rs` | medium | NOT A BUG | Cancelled batches retain successfully and atomically completed files; AfterExport only processes rows with paths. Completing the requested after-action for those files does not import unfinished files. |
| 97 | `crates/lc-raw/src/demosaic/amaze.rs` | high | NOT A BUG | CDDIFF stores full-resolution differences before Nyquist; later half-resolution reuse aliases the scratch region intentionally. |
| 98 | `crates/lc-preview/src/pool.rs` | medium | NOT A BUG | Drop explicitly joins for demonstrated NVIDIA driver teardown safety (Xid 13). Shutdown is not interactive rendering; detaching as suggested would reintroduce the documented crash. |
| 99 | `crates/lc-raw/src/numerics.rs` | high | NOT A BUG | Private gaussian9 callers pass fixed positive sigma 2.0 and 1.2; zero sigma is unreachable. |
| 100 | `crates/lc-ui-egui/src/panels/dialogs.rs` | high | ALREADY FIXED / OBSOLETE | Current confirm path copies save_beside into the dialog before confirm_dialog. |
| 101 | `crates/lc-ui-egui/src/panels/dialogs.rs` | medium | NOT A BUG | run_save_over reads time with max(app.last_time) and stores toast in app state; default Context::input is valid. Confirmation is a live UI event already repainting. No picker/font operation uses this context. |
| 102 | `crates/lc-ui-egui/src/icons.rs` | low | REAL | Center a square SVG inside nonsquare tool bounds; ds_finding_colored_tool_icon_is_square_in_a_wide_click_target includes pointer press/release. |
| 103 | `crates/lc-ui-egui/src/control.rs` | medium | NOT A BUG | Tagger is a loaded model/cache, not user selection; preset/model choice lives in smart.prefs and dialog. Closing a dialog intentionally releases inference resources; subsequent use reloads it. |
| 104 | `crates/lc-ui-egui/src/panels/process.rs` | medium | NOT A BUG | Live slider updates are deliberate preview behavior. JobPool deduplicates loupe jobs per slot; develop merges coalesce history. Changing to release-only would remove live feedback without a reproduced slowdown. |
| 105 | `crates/lc-ui-egui/src/panels/settings.rs` | medium | REAL | Release unfocused temporary folder edits; suppress intermediate text commit on picker press and commit the chosen folder on release. ds_finding_camera_folder_focus_and_picker_do_not_keep_stale_text. |
| 106 | `crates/lc-ui-egui/src/panels/settings.rs` | medium | REAL | Release unfocused temporary folder edits; suppress intermediate text commit on picker press and commit the chosen folder on release. ds_finding_camera_folder_focus_and_picker_do_not_keep_stale_text. |
| 107 | `crates/lc-ui-egui/src/smart_sort_keys.rs` | high | NOT A BUG | UI ignores moves on empty rows and resolves focus/selection through get/filter_map; 0 is an empty-state sentinel. |
| 108 | `crates/lc-ui-egui/src/smart_sort_keys.rs` | medium | REAL | Reject extra physical modifiers while respecting platform command aliases; ds_finding_extra_physical_modifiers_do_not_trigger_review_chords and existing actual review keyboard tests. |
| 109 | `crates/lc-engine/src/smart_sort/mod.rs` | low | REAL | Check cancellation before model loading and todo construction; initial-cancellation analyze regression. |
| 110 | `crates/lc-ui-egui/src/menus.rs` | medium | REAL | Treat explicit dir as a direct export destination; ds_finding_explicit_preset_destination_exports_without_conflict_dialog exercises menu invocation. |
| 111 | `crates/lc-ui-egui/src/panels/tool_tips.rs` | low | REAL | Clear Rich hover/shown state in Simple mode; ds_finding_simple_tooltips_release_rich_hover_state uses actual hover. |
| 112 | `crates/lc-raw/src/binned.rs` | high | NOT A BUG | can_bin requires Cfa::valid(), which constrains colours to RGB, before bin_normalized is called. |
| 113 | `crates/lc-raw/src/binned.rs` | medium | REAL | Normalize ordinary raw samples on demand, retain full buffers for opcodes and non-Bayer paths; sensor-confidence comparison and existing binned/opcode tests. |
| 114 | `crates/lc-raw/src/binned.rs` | low | NOT A BUG | Hook contract is a pre-demosaic pixel operation at the existing CFA sampling phase. It is not a CFA-layout replacement API; production hook changes only samples. |
| 115 | `crates/lc-pipeline/src/llf.rs` | high | NOT A BUG | Pinned darktable locallaplacian.c uses 31-clz(min), i.e. floor(log2), exactly as this implementation. |
| 116 | `crates/lc-pipeline/src/llf.rs` | low | NOT A BUG | LLF pyramid callers add the required border and choose levels preserving a valid interior; a border-only 2-row expansion is not a supported production pyramid. |
| 117 | `crates/li-attributions/build.rs` | medium | NOT A BUG | Build input is the version-controlled attributions manifest, not user-controlled file/document metadata. All checked-in license references are repository-relative Unix paths; no attacker-controlled build-input path or supported invalid entry. |
| 118 | `crates/li-ai/src/ops.rs` | high | NOT A BUG | No step ceiling is an explicit product decision in ops.rs; positive large sampling is user-requested work and ComfyUI validates its range. |
| 119 | `crates/li-ai/src/ops.rs` | medium | NOT A BUG | Same explicit unlimited-user-step policy as #118; request supplies desired refine sampling count and ComfyUI validates ranges. No spec model maximum remains authoritative. |
| 120 | `crates/li-ai/src/download.rs` | high | NOT A BUG | Preset::id includes model AND variant; distinct variants are retained as others. |
| 121 | `crates/lc-ui-egui/src/softpaint.rs` | high | NOT A BUG | epaint 0.36 ImageData has only Color; the irrefutable pattern is correct. |
| 122 | `crates/lc-raw/src/highlight/segbased.rs` | high | REAL | Require a valid segmentation interior before border extension on narrow recovery planes; ds_finding_narrow_recovery_plane. |
| 123 | `crates/lc-raw/src/highlight/segbased.rs` | medium | NOT A BUG | Current comment explicitly retains pinned darktable truncated edge window; port fixtures require this behavior. No unsupported center averaging change. |
| 124 | `crates/li-ai/src/mock.rs` | low | NOT A BUG | ComfyUI queue tuples are [priority/number, prompt_id, graph, extra_data, outputs]; mock uses the correct prompt ID at index 1. Proposed fix would corrupt the protocol. |
| 125 | `crates/lc-ui-egui/src/panels/right.rs` | medium | NOT A BUG | text_button uses allocate_exact_size auto ID; string only names the control registry. Icon uses a distinct allocated widget ID. Same registry label is not an egui ID collision. |
| 126 | `crates/lc-ui-egui/src/panels/smart_sort.rs` | high | REAL | Take the dialog state; sort lightweight photo summaries instead of cloning analyzed score vectors. ds_finding_smart_sort_frames_keep_analyzed_rows and existing click/drag review tests. |
| 127 | `crates/lc-ui-egui/src/panels/smart_sort.rs` | medium | REAL | Take the dialog state; sort lightweight photo summaries instead of cloning analyzed score vectors. ds_finding_smart_sort_frames_keep_analyzed_rows and existing click/drag review tests. |
| 128 | `crates/lc-ui-egui/src/panels/smart_sort.rs` | medium | REAL | Validate the review folder after reclassification; ds_finding_preset_change_reclassifies_and_replaces_stale_review_folder clicks the preset and Review controls. |
| 129 | `crates/lc-ui-egui/src/panels/host_composite.rs` | medium | REAL | Intersect the button with the loupe and clip its painter; ds_finding_compositing_button_is_clipped_to_small_loupe clicks inside and outside. |
| 130 | `crates/li-ai/src/inventory.rs` | medium | DEFERRED | li-ai inventory/model detection is being rewritten by another job and is explicitly excluded. The separate metadata-classification request is handled below; this finding remains deferred. |
| 131 | `crates/lc-ui-egui/src/panels/enhance.rs` | medium | ALREADY FIXED / OBSOLETE | AI Denoise and JobKind::Denoise were removed. Every remaining Enhance job is removal/heal. |
| 132 | `crates/lc-ui-egui/src/panels/detail.rs` | medium | NOT A BUG | Fullscreen photo-only comment excludes tool overlays, not unsaved photo pixels from its compositing document. The host composite is the current visible photo rendition; changing it would hide unsaved work. |
| 133 | `crates/lc-ui-egui/src/panels/detail.rs` | low | REAL | Honor Never for the selected spot pin; ds_finding_never_spot_overlay_hides_the_selected_pin. |
| 134 | `crates/li-ai/src/arch.rs` | low | DEFERRED | li-ai architecture/model detection is being rewritten by another job and is explicitly excluded. Only the separately authorized metadata-classification defect was changed. |
| 135 | `crates/li-attributions/src/lib.rs` | low | REAL | Include full commit hashes in searchable entry fields; search_matches_every_word_in_any_field. |
| 136 | `crates/li-attributions/src/lib.rs` | low | REAL | Count the matching application row when deciding whether to show No matches; ds_finding_app_only_search_does_not_say_no_matches clicks and types into Search. |
| 137 | `crates/lc-raw/src/highlight/segmentation.rs` | high | NOT A BUG | This faithfully retains the pinned darktable initial-top test. Production border=9 prevents negative indexing; no output defect was demonstrated for supported inputs. |
| 138 | `crates/lc-raw/src/highlight/segmentation.rs` | high | NOT A BUG | Pinned upstream segmentation.c intentionally uses (height-border-1); retained explicitly and checked with reference fixtures. |
| 139 | `crates/lc-raw/src/highlight/segmentation.rs` | medium | NOT A BUG | Seg/borderfill are private implementation helpers; production padded planes are at least 16 tall (now 20 for recovery), border=9. height<=border is unreachable. |
| 140 | `crates/lc-raw/src/highlight/segmentation.rs` | high | NOT A BUG | Seg is private; production border is 9 and morphology uses at most 8 rings, so offsets remain inside the apron. |
| 141 | `crates/pc-algo/src/blur.rs` | medium | NOT A BUG | Document-edge repeat is intentional filter semantics (blur.rs tests/comments distinguish canvas edges from transparent smaller layers); changing to transparent would fade an opaque document edge. |
| 142 | `crates/li-ai/src/browser.rs` | high | DEFERRED | Model filename/detection area in li-ai browser.rs is being rewritten by another job, per task. |
| 143 | `crates/li-ai/src/browser.rs` | medium | DEFERRED | li-ai browser model detection/templates are being rewritten by another job and are explicitly excluded by the task. |
| 144 | `crates/li-seg/src/lib.rs` | high | REAL | Validate custom sky-class indices on load and prediction; ds_finding_custom_sky_class_is_validated_on_reload. |
| 145 | `crates/li-seg/src/lib.rs` | medium | NOT A BUG | Single-file installed_bytes reports existence/bytes; ONNX load validates usability. Only multi-file bundles have strict size/checksum completeness here. The single-file test explicitly establishes this contract; existence is not an integrity guarantee. |
| 146 | `crates/li-seg/src/lib.rs` | medium | REAL | Atomically publish the registry before disposing the replaced model; preserve/reuse identical reimports. ds_finding_failed_custom_publication_keeps_old_registration_and_file and custom-model replacement tests. |
| 147 | `crates/li-ai/src/trash.rs` | medium | REAL | Preserve Unix filename/path bytes; inspect the symlink device for mount selection. ds_finding_trash_preserves_non_utf8_names_and_symlinks uses an owned trash and read-only mount lookup. |
| 148 | `crates/li-ai/src/trash.rs` | medium | REAL | Preserve Unix filename/path bytes; inspect the symlink device for mount selection. ds_finding_trash_preserves_non_utf8_names_and_symlinks uses an owned trash and read-only mount lookup. |
| 149 | `crates/li-ai/src/trash.rs` | low | REAL | Use TZif/POSIX timezone offsets for local DeletionDate; ds_finding_deletion_date_uses_process_local_timezone tests UTC/New York/Tokyo in isolated subprocesses; TZif footer tests cover future DST. |
| 150 | `crates/lc-pipeline/src/ucs.rs` | medium | NOT A BUG | UCS inverse is defined on forward-transform UV values. User chroma edits are gamut-mapped before inversion; the claimed singular UV is outside that domain. Production callers use the fixed Rec.2020 gamut. |
| 151 | `crates/lc-pipeline/src/ucs.rs` | low | NOT A BUG | All production L_white arguments are y_to_l_star(1), strictly positive; there is no user-selected black white point. Zero is outside the UCS conversion domain. |
| 152 | `crates/li-ai/src/comfy.rs` | medium | REAL | Stop signals our JobControls and uses ComfyUI atomic job-specific /api/jobs/{id}/cancel; never sends a global interrupt after a stale queue check. Mock Stop test checks Cancelled and no global interrupts. |
| 153 | `crates/li-ai/src/comfy.rs` | medium | REAL | Stop signals our JobControls and uses ComfyUI atomic job-specific /api/jobs/{id}/cancel; never sends a global interrupt after a stale queue check. Mock Stop test checks Cancelled and no global interrupts. |
| 154 | `crates/li-ai/src/builders.rs` | medium | REAL | Refine initializes one empty latent and applies RepeatLatentBatch once; Create alone initializes the requested empty batch. Workflow graph regression covers batch multiplicity. |
| 155 | `crates/lc-ui-egui/src/panels/export_dialog.rs` | high | NOT A BUG | Export dialogs close after starting the export; each new dialog initializes existing=None, so Some(0) only permits this one answered export. |
| 156 | `crates/pc-engine/src/edit_menu_cmds.rs` | high | NOT A BUG | sampleAllLayers explicitly selects composite sampling with a new output layer; comment and engine output path document this behavior. |
| 157 | `crates/pc-engine/src/edit_menu_cmds.rs` | low | NOT A BUG | Pixel-only output settings are intentionally irrelevant for non-pixel targets; those dispatch to their own supported clipboard representation. |
| 158 | `crates/pc-engine/src/develop_layer_cmds.rs` | medium | NOT A BUG | sync_photo explicitly preserves edited compositing documents by adding a new Develop layer; open_photo requests current settings and follows that documented policy. In-place replacement would overwrite compositing work. |
| 159 | `crates/pc-engine/src/develop_layer_cmds.rs` | low | NOT A BUG | Selecting the newly added Develop layer is intentional document-edit behavior, not temporary document navigation. The saved active document is restored; undo restores layer selection through history. |
| 160 | `crates/li-seg/src/faces.rs` | high | NOT A BUG | Pinned YuNet model outputs flattened [1,count,channels]; shape expectation follows actual model and OpenCV output layout. |
| 161 | `crates/li-seg/src/faces.rs` | medium | NOT A BUG | warp_affine_112 validates the input and bilinear sampling explicitly returns black for coordinates outside the image; landmarks are alignment coordinates, not unchecked pixel indices. |
| 162 | `crates/pc-engine/src/ai_cmds.rs` | medium | REAL | Multiply mask indices as usize before indexing; ds_finding_mask_index_exceeds_u32_without_wrapping. |
| 163 | `crates/pc-gpu/src/fx.rs` | high | REAL | Saturate field reach/apron arithmetic; ds_finding_extreme_field_reach_matches_whole_region. |
| 164 | `crates/pc-algo/src/seamless.rs` | medium | ALREADY FIXED / OBSOLETE | Current builder code checks w*h*channels and isize byte capacity before allocation, with documented empty return on invalid dimensions. Finding describes removed code. |
| 165 | `crates/pc-trace/src/frontend/mod.rs` | medium | NOT A BUG | Default trait wrapper establishes boundary cancellation only; expensive ColorClusterFrontend overrides it with periodic checks. No AI segmentation implementation exists in this trait. |
| 166 | `crates/pc-trace/src/frontend/keying.rs` | medium | REAL | Build a single 24-bit RGB occupancy set and search unused colors; ds_finding_key_search_covers_more_than_initial_candidates. |
| 167 | `crates/pc-trace/src/frontend/keying.rs` | medium | REAL | Build a single 24-bit RGB occupancy set and search unused colors; ds_finding_key_search_covers_more_than_initial_candidates. |
| 168 | `crates/pc-engine/src/prefs.rs` | high | NOT A BUG | ToolbarCustomization::active returns an owned ToolSet, so there is no outstanding borrow. |
| 169 | `crates/pc-engine/src/prefs.rs` | high | REAL | Validate/batch legacy toolbar edits before migration, deduplicate bindable IDs, resolve only the active tool set, and apply legacy edits to it. Preference/toolbar regressions cover combined and repeated edits. |
| 170 | `crates/pc-engine/src/prefs.rs` | medium | REAL | Validate/batch legacy toolbar edits before migration, deduplicate bindable IDs, resolve only the active tool set, and apply legacy edits to it. Preference/toolbar regressions cover combined and repeated edits. |
| 171 | `crates/li-seg/src/tests.rs` | medium | NOT A BUG | Current single-file installed_bytes intentionally checks file existence, so Some(1234) is the correct result. Bundle integrity has separate tests. |
| 172 | `crates/pc-gpu/src/plan_cache.rs` | medium | REAL | Cache immutable Arc document identities with Weak, invalidate on make_mut and gamma changes; real canvas calls refresh_snapshot. ds_finding_snapshot_skips_structure_key_but_make_mut_invalidates covers mixed borrowed/snapshot APIs. |
| 173 | `crates/pc-algo/src/hdr.rs` | medium | NOT A BUG | Ward MTB minimizes XOR disagreements after applying exclusion masks. Dividing that cost by geometric overlap compensates for shifted window size; it is not a conditional mismatch probability. Changing to classified-pixel normalization changes the alignment objective without a reproduced alignment failure. |
| 174 | `crates/pc-trace/src/frontend/color_cluster.rs` | medium | NOT A BUG | Pinned VTracer converter intentionally uses diagonal connectivity when layer_difference==0; this frontend preserves that policy, not an independently selectable connectivity setting. |
| 175 | `crates/pc-engine/src/vectorize_cmds.rs` | high | NOT A BUG | Params includes preset and serializes it; the override map contains preset. |
| 176 | `crates/pc-engine/src/cutout_cmds.rs` | low | NOT A BUG | Background status is inferred from the name and transparency/position locks, not an independent flag. The duplicate is named Cutout and both locks are already cleared. |
| 177 | `crates/pc-engine/src/cutout_cmds.rs` | low | NOT A BUG | JobCtx::stage returns the closure result directly. apply_in_with returns Option<Surface>, where None means cancellation; there is no hidden Some(Err) variant. |
| 178 | `crates/pc-engine/src/cutout_cmds.rs` | low | NOT A BUG | Surface clones share Arc pixel tiles; src.clone does not copy all pixel buffers. read_region and mutation allocate only the output area. |
| 179 | `crates/li-seg/src/faces/cluster.rs` | high | REAL | Keep exact thresholded Chinese Whispers behavior with bounded dense-edge memory and cancellation; run large People queries on a snapshot worker and reject stale catalog/face/model results. ds_finding_dense_streaming_is_exact_and_cancellable and actual 5k-face People UI cancellation test. |
| 180 | `crates/pc-trace/src/frontend/binary.rs` | medium | NOT A BUG | Centered Bradley-Roth neighborhoods deliberately use inclusive endpoints (odd centered support), as the paper/pinned algorithm. Window is a nominal centered extent; no explicit exact even-side contract. |
| 181 | `crates/pc-trace/src/frontend/binary.rs` | low | NOT A BUG | Binary threshold uses the documented inclusive boundary; at 100 the zero-intensity pixels are exactly at the threshold and are valid foreground. |
| 182 | `crates/pc-trace/src/vc/color_clusters/container.rs` | high | NOT A BUG | get_pixel_at_index uses bounds checks; production trace images are limited by PIXEL_BUDGET well below i32::MAX. |
| 183 | `crates/pc-trace/src/vc/color_clusters/container.rs` | high | NOT A BUG | Private vc module callers supply in-image contour coordinates; no user mouse/metadata caller and trace validates image dimensions. |
| 184 | `crates/pc-trace/src/vc/color_clusters/runner.rs` | high | NOT A BUG | Builder::stage_2 checks infos.is_empty() and continues before invoking deepen. |
| 185 | `crates/pc-trace/src/vc/color_clusters/runner.rs` | medium | NOT A BUG | vc is private; ColorClusterFrontend uses fixed positive batch=25600 and generated opaque key distinct from transparent all-zero sentinel. Arbitrary RunnerConfig/user key is not exposed. |
| 186 | `crates/pc-trace/src/vc/color_clusters/runner.rs` | medium | NOT A BUG | vc is private; ColorClusterFrontend uses fixed positive batch=25600 and generated opaque key distinct from transparent all-zero sentinel. Arbitrary RunnerConfig/user key is not exposed. |
| 187 | `crates/pc-engine/src/brush_cmds.rs` | medium | NOT A BUG | tail_preview copies only touched coverage tiles but preserves each coverage map full bounds; tail.bounds() already includes the complete stroke body. No damage is lost. |
| 188 | `crates/li-seg/src/bpe.rs` | medium | REAL | Stop streaming BPE at the CLIP context limit, preserving exact prefix tokenization; ds_finding_bpe_preserves_prefix_without_processing_unused_suffix. |
| 189 | `crates/pc-trace/src/lib.rs` | medium | NOT A BUG | boundary::remove_collinear iterates 0..n (empty n performs no modulo), and polygon uses first() safely. Degenerate fallback loops cannot panic; generated contours are cardinal closed loops. |
| 190 | `crates/pc-trace/src/lib.rs` | high | NOT A BUG | Watershed cut clamps target to MST size; detail is a scalar exponent and does not drive allocations/iterations independently. |
| 191 | `crates/pc-trace/src/lib.rs` | medium | NOT A BUG | adaptive_bradley_roth explicitly treats 0 as auto (~shorter side/8), clamped to at least 1. No zero neighborhood or division occurs. |
| 192 | `crates/pc-trace/src/lib.rs` | low | NOT A BUG | LineArt/Sketch are reserved public preset names explicitly rejected until their dedicated frontends exist; presets available to users exclude them. |
| 193 | `crates/pc-trace/src/adapter.rs` | medium | NOT A BUG | Adapter is private and only consumes generated tracer/fitter contours, which always use MoveTo after ClosePath; it is not the SVG parser. No supported path input contains the claimed sequence. |
| 194 | `crates/pc-trace/src/adapter.rs` | low | NOT A BUG | Private adapter only consumes trace-generated pixel coordinates bounded by PIXEL_BUDGET, so finite-arithmetic overflow is unreachable. |
| 195 | `crates/pc-gpu/src/lib.rs` | high | NOT A BUG | Texture windows are intersected with effect region; a region fitting device_max cannot create a larger texture. Oversized axes are separately guarded. |
| 196 | `crates/pc-trace/src/topology.rs` | medium | NOT A BUG | Checker already sorts x minima and prunes nonoverlapping x/y intervals, and tracing runs on its worker. Worst-case all-overlapping segments is speculative; actual crossing paths exit at first hit. |
| 197 | `crates/pc-trace/src/topology.rs` | medium | NOT A BUG | Subpath::segments includes last-to-first cubic for closed paths, so flattening covers the closing edge already. |
| 198 | `crates/pc-ui-egui/src/background_ui.rs` | medium | NOT A BUG | Evidence itself specifies different parent IDs: non-inline settings use push_id(background-settings), inline settings are under options UI. Same local ComboBox salt under distinct parent IDs does not collide. |
| 199 | `crates/pc-ui-egui/src/ai_remove_ui.rs` | high | NOT A BUG | json! serializes a borrowed expression; it does not move p.mask. |
| 200 | `crates/pc-ui-egui/src/ai_remove_ui.rs` | high | NOT A BUG | Let chains are stable since Rust 1.88 in edition 2024; this workspace uses 1.98.1. |
| 201 | `crates/pc-ui-egui/src/ai_remove_ui.rs` | medium | REAL | Update bounds and dirty rectangles from changed paint; use partial texture uploads and submit immediate removal only when coverage changed. ds_finding_remove_bounds_and_partial_texture_follow_actual_strokes performs actual drags. |
| 202 | `crates/pc-ui-egui/src/ai_remove_ui.rs` | medium | REAL | Update bounds and dirty rectangles from changed paint; use partial texture uploads and submit immediate removal only when coverage changed. ds_finding_remove_bounds_and_partial_texture_follow_actual_strokes performs actual drags. |
| 203 | `crates/pc-ui-egui/src/ai_remove_ui.rs` | medium | REAL | Update bounds and dirty rectangles from changed paint; use partial texture uploads and submit immediate removal only when coverage changed. ds_finding_remove_bounds_and_partial_texture_follow_actual_strokes performs actual drags. |
| 204 | `crates/pc-gpu/src/plan.rs` | high | NOT A BUG | with_cmyk_grids uses validated fixed-input CLUTs; parsing requires grid dimensions to match inputs 3/4. |
| 205 | `crates/pc-gpu/src/plan.rs` | medium | NOT A BUG | BlendIf shader compares channel*255 to byte-valued thresholds, matching CPU BlendRange semantics. Other LUT types have unrelated units. |
| 206 | `crates/pc-trace/src/boundary.rs` | medium | REAL | Check cancellation periodically within each contour walk; ds_finding_cancel_during_single_long_contour. |
| 207 | `crates/pc-ui-egui/src/ai_remove_ui/tests.rs` | low | REAL | Use RAII for the global AI service override under the AI test mutex; existing mock removal test restores on every exit. |
| 208 | `crates/pc-testkit/src/vector/mod.rs` | medium | REAL | Track true adaptive cubic parameter intervals and refine intersection parameters on each subcurve; ds_finding_intersection_parameters_locate_original_cubics evaluates the returned original-curve points. |
| 209 | `crates/pc-trace/src/vc/mod.rs` | medium | NOT A BUG | vc is private; all production trace inputs are checked against PIXEL_BUDGET before constructing these images. Overflow-sized dimensions are unreachable. |
| 210 | `crates/pc-trace/src/vc/mod.rs` | medium | NOT A BUG | vc is private; all production trace inputs are checked against PIXEL_BUDGET before constructing these images. Overflow-sized dimensions are unreachable. |
| 211 | `crates/pc-trace/src/vc/mod.rs` | low | NOT A BUG | Private vc helpers receive validated image coordinates, clipped Bradley neighborhoods, and enumerated cluster indices; they do not consume UI selection rectangles or arbitrary public indices. |
| 212 | `crates/pc-trace/src/vc/mod.rs` | low | NOT A BUG | Private vc helpers receive validated image coordinates, clipped Bradley neighborhoods, and enumerated cluster indices; they do not consume UI selection rectangles or arbitrary public indices. |
| 213 | `crates/pc-trace/src/vc/mod.rs` | medium | NOT A BUG | Private vc helpers receive validated image coordinates, clipped Bradley neighborhoods, and enumerated cluster indices; they do not consume UI selection rectangles or arbitrary public indices. |
| 214 | `crates/pc-trace/src/vc/mod.rs` | low | NOT A BUG | Private vc helpers receive validated image coordinates, clipped Bradley neighborhoods, and enumerated cluster indices; they do not consume UI selection rectangles or arbitrary public indices. |
| 215 | `crates/pc-ui-egui/src/file_ui.rs` | low | REAL | Keep current-directory fallback for a saved relative path; ds_finding_relative_saved_path_keeps_current_directory. |
| 216 | `crates/pc-trace/src/ir.rs` | medium | NOT A BUG | Private stack_colors receives masks generated within the validated trace canvas by its frontends; negative/out-of-canvas mask origins are not supported production inputs. |
| 217 | `crates/li-seg/src/faces/geometry.rs` | low | REAL | Sample the centered row/column when the minor letterbox axis is one pixel; ds_finding_extreme_letterbox_samples_center. |
| 218 | `crates/pc-ui-egui/src/generate_ui/draft_refine/tests.rs` | low | REAL | Add a Drop guard for request capture; ds_finding_mock_request_guard_clears_during_unwind. |
| 219 | `crates/pc-ui-egui/src/generate_ui/draft_refine/tests.rs` | medium | REAL | Give each UI test thread a unique worktree-local library, remove it on thread teardown, and capture the library before worker launch; existing mock Draft/Refine test writes only there. |
| 220 | `crates/pc-ui-egui/src/generate_ui/draft_refine/tests.rs` | medium | REAL | Give each UI test thread a unique worktree-local library, remove it on thread teardown, and capture the library before worker launch; existing mock Draft/Refine test writes only there. |
| 221 | `crates/pc-ui-egui/src/generate_ui/draft_refine.rs` | high | NOT A BUG | egui load_texture allocates a new texture ID each time; name is only a debug label. |
| 222 | `crates/pc-ui-egui/src/generate_ui/draft_refine.rs` | medium | REAL | Share Arc library snapshots per frame; clone entries only on removal. ds_finding_library_snapshots_share_entries_until_removal covers 10,000 populated entries and copy-on-write. |
| 223 | `crates/pc-ui-egui/src/layer_props_ui.rs` | high | NOT A BUG | matches! wildcard patterns do not move fields or the scrutinee. |
| 224 | `crates/pc-ui-egui/src/liquify_ui.rs` | low | REAL | Return static shortcut IDs; ds_finding_liquify_shortcut_ids_are_stable_and_distinct and existing actual dialog/key tests. |
| 225 | `crates/pc-ui-egui/src/gpu_status.rs` | medium | NOT A BUG | egui 0.36 Popup::menu(&response) configures its click-triggered memory opening internally; separate memory.toggle_popup would double-toggle. |
| 226 | `crates/pc-ui-egui/src/model_browser.rs` | medium | REAL | Throttle Installed items and share the installed-name set between repaints; ds_finding_installed_names_are_cached_across_repaints verifies reuse and expiry. |
| 227 | `crates/pc-ui-egui/src/model_browser.rs` | low | REAL | Always navigate unknown-family LoRAs to Trending; ds_finding_unknown_family_loras_always_open_trending. |
| 228 | `crates/pc-ui-egui/src/ai_ui.rs` | high | NOT A BUG | selection returns () and arms the painting tool; readiness and errors are handled when remove executes. |
| 229 | `crates/pc-ui-egui/src/ai_ui.rs` | low | REAL | Request repaint after the unload worker publishes status; ds_finding_unload_completion_wakes_idle_ui clicks Eject and observes the worker callback. |
| 230 | `crates/pc-trace/src/potrace.rs` | high | NOT A BUG | Potrace longest requires the cardinal unit-step contours provided by boundary::contours; last and first share x or y (upstream explicitly guarantees i<n-1). |
| 231 | `crates/pc-trace/src/potrace.rs` | high | NOT A BUG | Same cardinal contour invariant as #230 prevents nc==n. |
| 232 | `crates/pc-ui-egui/src/tool_icon_sheet.rs` | medium | ALREADY FIXED / OBSOLETE | Current contact sheet was rebuilt at 20/40 px; raster boolean means disabled, not light/dark background. Both backgrounds correctly use enabled artwork. |
| 233 | `crates/pc-ui-egui/src/shortcut_dispatch.rs` | medium | NOT A BUG | Tool bindings have separate shortcuts.rs/modal handlers, independent of catalog defaults. ds_finding_tool_without_default_accepts_assigned_shortcut presses unassigned Alt+F13 and checks Blur. An earlier test used Alt+F9, the existing Actions shortcut, so its conflict correctly took precedence. |
| 234 | `crates/pc-ui-egui/src/generate_ui.rs` | high | NOT A BUG | Document::bounds always uses document size; render_layer fills the requested rectangle regardless of layer size. |
| 235 | `crates/pc-ui-egui/src/generate_ui.rs` | medium | REAL | Disable Selected layer Refine with no active layer and show an explicit status for programmatic start; ds_finding_selected_layer_refine_is_disabled_without_a_selected_layer. |
| 236 | `crates/pc-ui-egui/src/generate_ui.rs` | medium | REAL | Serialize optional custom refine sampler values with serde defaults; ds_finding_refine_preset_persists_custom_sampler_and_loads_old_defaults. |
| 237 | `crates/pc-ui-egui/src/canvas.rs` | medium | REAL | Keep polling an initial background Navigator placeholder during drags; ds_finding_real_app_navigator_job_finishes_during_drag renders actual hardware app frames while holding a marquee drag. |
| 238 | `crates/pc-ui-egui/src/context_bar.rs` | medium | NOT A BUG | vector_ui::active_shape returns the selected Shape layer regardless of current tool; the claimed Move-tool exclusion does not exist. |
| 239 | `crates/pc-trace/src/frontend/watershed.rs` | high | NOT A BUG | Hierarchy cut deliberately retains basins above minimum persistence, merging tied trivia as documented; max detail is not one region per pixel. |
| 240 | `crates/pc-ui-egui/src/icons.rs` | medium | REAL | Use faint monochrome tint in disabled tool UIs; ds_finding_disabled_monochrome_tool_has_faint_tint_and_ignores_click. |
| 241 | `crates/pc-gpu/src/health.rs` | medium | REAL | Serialize strike recording and frame decay in one mutex; ds_finding_error_and_decay_are_serialized exercises concurrent boundary events. |
| 242 | `crates/pc-ui-egui/src/color_icons.rs` | high | NOT A BUG | egui 0.36 get_temp/insert_temp persist between frames; temp only means not serialized. Colour-icon preference is also stored in app preferences. |
| 243 | `crates/pc-ui-egui/src/color_icons.rs` | medium | NOT A BUG | egui 0.36 get_temp/insert_temp persist between frames; temp only means not serialized. Colour-icon preference is also stored in app preferences. |
| 244 | `crates/pc-ui-egui/src/color_icons.rs` | medium | NOT A BUG | TextureHandle Drop releases its texture through egui TextureManager; explicit free while cloned handles exist would invalidate live images. |
| 245 | `crates/pc-ui-egui/src/type_tool.rs` | medium | REAL | Use actual layer size for empty text and preserve compatible font style across family changes; ds_finding_type_quick_bar_uses_layer_size_and_preserves_available_style clicks the real quick bar. |
| 246 | `crates/pc-ui-egui/src/type_tool.rs` | medium | REAL | Use actual layer size for empty text and preserve compatible font style across family changes; ds_finding_type_quick_bar_uses_layer_size_and_preserves_available_style clicks the real quick bar. |
| 247 | `crates/pc-ui-egui/src/dock.rs` | medium | REAL | Restore geometry, active tab, group order and height when a drop temporarily removes its target; dock metadata regressions. |
| 248 | `crates/pc-ui-egui/src/dock.rs` | medium | REAL | Restore geometry, active tab, group order and height when a drop temporarily removes its target; dock metadata regressions. |
| 249 | `crates/pc-trace/src/vc/color_clusters/builder.rs` | high | NOT A BUG | vc is a private implementation; production ColorClusterFrontend fixes batch_size=10000 and no public tracing parameter can set zero. |
| 250 | `crates/pc-ui-egui/src/tool_tips.rs` | high | NOT A BUG | LiquifyTool::ALL contains 11 brush tools, excluding the four whole-image commands; claimed test failure does not occur. |
| 251 | `crates/pc-ui-egui/src/vector_ui.rs` | medium | REAL | Borrow the active Shape through its Arc document in the options bar instead of deep-cloning its path; ds_finding_borrowed_shape_options_allow_corner_editing and existing shape color-picker UI regressions. |
| 252 | `xtask/src/main.rs` | medium | REAL | Force cargo tree --color never; ds_finding_dependency_audit_forces_plain_output and the actual workspace dependency audit. |
| 253 | `xtask/src/main.rs` | medium | NOT A BUG | Offline dependency-graph audit deliberately requires the workspace graph to resolve. Missing dependencies are an environment prerequisite failure, not flaky behavior; ignoring the purity audit would weaken the required all-Rust check. |
| 254 | `crates/lc-raw/src/demosaic/vng.rs` | high | NOT A BUG | Pinned darktable vng.c tests == with both adjacent colours, exactly as the Rust port. |
| 255 | `xtask/src/assets.rs` | medium | REAL | Directory patterns now pass through brace/glob matching for each path prefix. Regression covers wildcard and brace directory patterns. |
| 256 | `xtask/src/assets.rs` | low | REAL | Attribution parser now includes root filenames and root Markdown licence links, while treating backticked link labels as labels rather than paths. ds_finding_directory_globs_and_root_notices and licences_are_read_from_both_table_formats cover standalone files, link targets and mixed local/remote references. |
| 257 | `xtask/src/assets.rs` | low | REAL | Attribution parser now includes root filenames and root Markdown licence links, while treating backticked link labels as labels rather than paths. ds_finding_directory_globs_and_root_notices and licences_are_read_from_both_table_formats cover standalone files, link targets and mixed local/remote references. |
| 258 | `crates/pc-ui-egui/src/panels.rs` | medium | NOT A BUG | Custom tool-set order deliberately orders flyout slots globally; splitting canonical sections preserves the requested order. Height estimation counts the actual rendered dividers. |
| 259 | `crates/pc-engine/src/toolsets.rs` | medium | REAL | Validate/batch legacy toolbar edits before migration, deduplicate bindable IDs, resolve only the active tool set, and apply legacy edits to it. Preference/toolbar regressions cover combined and repeated edits. |
| 260 | `crates/pc-engine/src/toolsets.rs` | medium | REAL | Validate/batch legacy toolbar edits before migration, deduplicate bindable IDs, resolve only the active tool set, and apply legacy edits to it. Preference/toolbar regressions cover combined and repeated edits. |
| 261 | `crates/pc-ui-egui/src/dock_float.rs` | medium | REAL | Use actual layer z-order for overlapping targets; omit hidden/emptied panels immediately. Floating-panel pointer regressions cover raising and closing. |
| 262 | `crates/pc-ui-egui/src/dock_float.rs` | low | REAL | Use actual layer z-order for overlapping targets; omit hidden/emptied panels immediately. Floating-panel pointer regressions cover raising and closing. |
| 263 | `crates/pc-ui-egui/src/gpu_canvas.rs` | high | NOT A BUG | screen_x returns (origin-x1*scale, origin-x0*scale), which is ordered left<=right for positive scale even when flipped. |
| 264 | `crates/pc-ui-egui/src/gpu_canvas.rs` | medium | REAL | Truncate unused encode bindings after compositing and clear them on document removal; ds_finding_encode_pool_releases_unused_tiles on RTX 5090 Vulkan. |
| 2-1 | `crates/lc-catalog/src/rules.rs` | medium | REAL | Reject non-ID values in person anyOf/allOf rules. ds2_person_set_operators_validate_value_types covers scalars, mixed/invalid arrays and valid numeric/empty sets. |
| 2-2 | `crates/lc-engine/src/lib.rs` | medium | NOT A BUG | People identity mutations always go through commit_people, whose catalog Batch commit advances catalog.revision (including an empty ops batch). Undo/redo also advance the revision; the visible cache already invalidates. |
| 2-3 | `crates/lc-engine/src/export.rs` | high | NOT A BUG | Catalog::photo returns &Arc<Photo>; two dereferences are valid and the crate compiles. |
| 2-4 | `crates/lc-engine/src/export.rs` | medium | NOT A BUG | MetadataPolicy applies to rendered exports. The Original/DNG dialog explicitly preserves original metadata and XMP; its metadata restriction controls are absent. include_people_names follows that format policy. |
| 2-5 | `crates/lc-engine/src/export.rs` | low | REAL | Original/DNG sidecars borrow Photo unless People keyword augmentation requires ownership; ds2_original_export_borrows_photo_unless_people_keywords_are_requested checks identity and ownership. |
| 2-6 | `crates/lc-ui-egui/src/panels/grid.rs` | low | REAL | A missing thumbnail can request one committed-settings initial render (or embedded quick preview) during interaction; existing thumbnails remain stable and transient settings never enter the disk cache. ds2_missing_thumbnail_renders_committed_settings_once_during_interaction. |
| 2-7 | `crates/lc-ui-egui/src/icons.rs` | medium | NOT A BUG | egui_extras 0.36 install_image_loaders checks is_loader_installed for each loader before registration; repeated calls are idempotent. |
| 2-8 | `crates/lc-ui-egui/src/panels/smart_sort.rs` | high | NOT A BUG | prepare_preset/resolve_groups use the supplied planned folders and each folder.name/ids, not preset.folders or its indices. The effective preset changes only folders; token/session/file-pattern settings are identical. |
| 2-9 | `crates/lc-ui-egui/src/panels/smart_sort.rs` | medium | NOT A BUG | smart_sort_task::start delegates to people_task::start, which rejects an existing task before spawning a second worker. The confirm path returns that error. |
| 2-10 | `crates/lc-ui-egui/src/panels/smart_sort.rs` | medium | REAL | Initialize People-folder defaults at dialog opening and preserve explicit choices through Back/forward navigation. Carry an explicit Create a folder action back from the owned People workspace. Checkbox/navigation regression ds2_people_folder_switch_survives_back_navigation and existing people_ui_faces_opt_in_analysis_and_person_folder_export_picker exercise both paths. |
| 2-11 | `crates/lc-engine/src/smart_sort/people/tests.rs` | high | NOT A BUG | album.createSmart accepts rules through the Filter schema; ruleSet is the RuleSet field nested in Filter. Existing smart-album regression uses the documented command schema. |
| 2-12 | `crates/li-ai/src/workflows.rs` | high | REAL | Text-to-image Qwen forces full denoise; partial reference edits start from a VAEEncode of the first RGBA reference while retaining all reference conditioning. ds2_qwen_partial_denoise_uses_first_reference_latent validates links and inputs. |
| 2-13 | `crates/lc-ui-egui/src/smart_sort_task.rs` | high | REAL | Validate/prepare the embedding store before starting faces; cancel the newly started face task if Smart Sort worker spawn fails. ds2_smart_sort_setup_failure_does_not_start_face_worker reproduces a mismatched dimension through start and verifies that clicking disabled Analyse starts no workers. |
| 2-14 | `crates/lc-ui-egui/src/smart_sort_task.rs` | medium | NOT A BUG | Concurrent face analysis is deliberately excluded (UI Analyze is disabled). people_task::start returns a useful already-running error; no conflicting workers are created. |
| 2-15 | `crates/lc-ui-egui/src/smart_sort_task.rs` | medium | NOT A BUG | people_task::poll advances Smart Sort to step 1 when faces finish and app.smart_sort is None. Both completion orders are handled. |
| 2-16 | `crates/lc-gpu/src/ctx.rs` | medium | NOT A BUG | wgpu-core 30 Device::maintain compares the completed fence index with the requested wait index and returns WaitIdleError::Timeout/PollError::Timeout if incomplete. A zero-timeout Wait cannot report pending work as idle. |
| 2-17 | `crates/lc-ui-egui/src/widgets.rs` | high | NOT A BUG | egui temporary data means not serialized to disk; it persists across frames. The existing Shift-drag test exercises multi-frame motion. |
| 2-18 | `crates/li-ai/src/ops.rs` | high | REAL | Validate refine scale as finite and within 1..=4 before worker allocation. ds2_refinement_request_rejects_nonfinite_and_out_of_range_parameters. |
| 2-19 | `crates/li-ai/src/ops.rs` | medium | REAL | Validate request denoise as finite and within 0..=1 before dispatch. ds2_refinement_request_rejects_nonfinite_and_out_of_range_parameters. |
| 2-20 | `crates/li-ai/src/ops.rs` | medium | REAL | Validate refine strength as finite and within 0..=1 before dispatch. ds2_refinement_request_rejects_nonfinite_and_out_of_range_parameters. |
| 2-21 | `crates/pc-engine/src/float_cmds/tests.rs` | high | NOT A BUG | DocState.doc and before are Arc<Document>; dereferencing for Document equality is valid and the crate compiles. |
| 2-22 | `crates/lc-ui-egui/src/panels/rules_editor.rs` | medium | REAL | Include selected missing/ignored IDs in the person popup as removable fallback checkboxes. Actual popup click regression ds2_person_picker_can_remove_selected_missing_identity. |
| 2-23 | `crates/li-ai/src/family.rs` | high | NOT A BUG | Family inheritance intentionally copies compatible file-pattern overrides unless a child replaces them. SDXL has no overrides to leak to Pony; Flux Fill/Kontext explicitly override with empty lists. No actual conflicting child/profile is shown. |
| 2-24 | `crates/pc-compose/src/shape_split.rs` | medium | DEFERRED | Active photocraft-compose group/shape/mask caching rewrite; task explicitly excludes this area. Coordinator should verify after merging. |
| 2-25 | `crates/pc-compose/src/shape_split.rs` | medium | DEFERRED | Active photocraft-compose group/shape/mask caching rewrite; task explicitly excludes this area. Coordinator should verify after merging. |
| 2-26 | `crates/lc-ui-egui/src/panels/people_workspace.rs` | medium | NOT A BUG | The name field already contains the edited value before namePerson runs; suggestion clicks replace the cached value. peoplePasteNames changes suggestions, not person names. Undo/redo and merges clear names. |
| 2-27 | `crates/lc-ui-egui/src/panels/people_workspace.rs` | medium | REAL | Transfer face-options errors into the taken/restored finder state. Actual download click regression ds2_finder_retains_download_error_and_settings_owns_input. |
| 2-28 | `crates/lc-ui-egui/src/panels/people_workspace.rs` | low | NOT A BUG | i=48 duplicates the i=0 perimeter point at TAU; 48 triangles close the circle. An extra triangle would be degenerate. |
| 2-29 | `crates/li-ai/src/catalog.rs` | medium | NOT A BUG | All installed built-in TextEncoder reference profiles have edit=true; custom compatible native reference profiles may intentionally choose full denoise. No supported model demonstrates the alleged mismatch. |
| 2-30 | `crates/pc-ui-egui/src/canvas.rs` | high | NOT A BUG | move_input handles transform-control hit testing before calling tool_event; it establishes ui.transform before auto-select can run. Existing move_input_transform_controls_frame_and_scale_multiple_layers_together exercises the handle path. |
| 2-31 | `crates/pc-psd/src/slices.rs` | medium | NOT A BUG | The primary ag-psd descriptor implementation uses ESliceOrigin layer = layer. The proposed layerGenerated is not its descriptor enum. Keep the compatible existing value. |
| 2-32 | `crates/pc-psd/src/slices.rs` | medium | NOT A BUG | Primary psd-tools terminology defines Enum.Middle = Mddl and Enum.Bottom = Bttm; Key.Bottom = Btom is a different descriptor identifier. Keep the existing enum values. |
| 2-33 | `crates/pc-engine/src/transform_cmds.rs` | medium | REAL | Normalize requested layers before deriving the default source rectangle; transform the union of effective target bounds. ds2_transform_layers_uses_union_and_rejects_empty_requests checks numerical layer locations. |
| 2-34 | `crates/pc-engine/src/transform_cmds.rs` | low | REAL | A single effective layer in a layers array may take a folded transform and uses that named layer, not the active layer. ds2_single_layer_array_accepts_folded_quad_and_targets_named_layer. |
| 2-35 | `crates/pc-engine/src/transform_cmds.rs` | low | REAL | Reject explicitly empty/malformed layers arrays before editing. ds2_transform_layers_uses_union_and_rejects_empty_requests checks document preservation. |
| 2-36 | `crates/pc-io/src/raw.rs` | medium | NOT A BUG | LAST_DECODE is an intentional one-entry bounded decode cache used by slider renders, not an unbounded leak. Replacing it with Weak loses all hits because rendered output does not retain the source. |
| 2-37 | `crates/pc-compose/src/masks.rs` | medium | DEFERRED | Active photocraft-compose group/shape/mask caching rewrite; task explicitly excludes this area. Coordinator should verify after merging. |
| 2-38 | `crates/li-ai/src/builders.rs` | high | ALREADY FIXED / OBSOLETE | Fixed by original finding 154: non-Create empty latent batch is 1 before RepeatLatentBatch. ds_finding_flux_refine_batch_is_not_squared covers batch 2/4. |
| 2-39 | `crates/li-ai/src/builders.rs` | medium | NOT A BUG | ReferenceLatent carries conditioning at its own reference resolution; it need not match the target starting latent. FLUX.2 uses independent reference image IDs/positions. Current refine source is snapped to the requested output size. |
| 2-40 | `crates/pc-ui-egui/src/icons.rs` | high | NOT A BUG | edit-toolbar and quick-mask exist in TOOL_ICONS and mono_tool_icons; exists/image resolve them directly. Actual silhouette/icon coverage includes both. |
| 2-41 | `crates/pc-ui-egui/src/move_mods.rs` | medium | NOT A BUG | The release offset is rounded to whole pixels. layer.translate returns without editing or duplicating when dx=dy=0; transient copies are discarded at gesture finish. A subpixel Alt-click does not commit a duplicate. Existing one-pixel movement behavior is intentional. |
| 2-42 | `crates/pc-ui-egui/src/move_mods.rs` | low | NOT A BUG | PhotocraftApp::run sets ui.status and status_error on command failure before returning Err. Ignoring the returned result does not hide the error. |
| 2-43 | `crates/pc-ui-egui/src/shortcuts.rs` | medium | REAL | Consume captured Move Escape events independently of the final modifier state. Actual press/drag/key regression ds2_move_escape_uses_event_modifiers_and_dialog_open_cancels_capture uses Ctrl+Escape followed by modifier release. |
| 2-44 | `crates/pc-ui-egui/src/generate_ui/draft_refine.rs` | medium | NOT A BUG | set_model already sets denoise using mode.default_strength(model), including native reference families. The removed helper duplicated that policy. |
| 2-45 | `crates/pc-ui-egui/src/generate_ui/draft_refine.rs` | medium | NOT A BUG | set_model resets denoise for the current mode/model. The UI model-picker calls it. |
| 2-46 | `xtask/src/main.rs` | medium | REAL | The contact-sheet command selects only the two UI libraries; binary-only xtask is omitted. ds2_contact_sheet_command_only_selects_packages_with_libraries checks the actual command and target files. |
| 2-47 | `crates/pc-engine/src/float_cmds.rs` | medium | REAL | Reject movePixels while a piece floats, before the dispatcher can implicitly drop it. ds2_move_pixels_does_not_drop_an_existing_floating_piece checks Arc identity, offset and history for zero/nonzero deltas. |
| 2-48 | `crates/pc-ui-egui/src/generate_ui.rs` | low | REAL | Guard inverted backend step limits before clamp. ds2_refine_lora_preset_roundtrip_and_invalid_step_limits. |
| 2-49 | `crates/pc-ui-egui/src/generate_ui.rs` | medium | NOT A BUG | ModelId::intern accepts unknown keys; refine_default_denoise uses try_info and returns 0.35 for them. It does not panic. |
| 2-50 | `crates/pc-ui-egui/src/generate_ui.rs` | medium | REAL | Add serde-defaulted refine_loras, save/apply them and clear stale choices when older presets load. Actual Save Preset popup regression ds2_refine_lora_preset_roundtrip_and_invalid_step_limits. |
| 2-51 | `crates/pc-ui-egui/src/widgets.rs` | medium | REAL | Honor requested precision beyond two decimals. ds2_value_field_displays_requested_four_decimal_precision checks displayed text and edits the actual field. |
| 2-52 | `crates/pc-ui-egui/src/snap_ui.rs` | medium | REAL | Disable selection targets only for actual Move gestures; guide construction always includes selection targets, independent of tool. ds2_guides_snap_to_selection_with_move_tool_active. |
| 2-53 | `crates/pc-ui-egui/src/move_ui.rs` | medium | REAL | Resolve move_targets only when rebuilding the preview; unchanged frames reuse the cached list/document. Actual drag regression ds2_repeated_move_frames_reuse_target_list_and_preview_document. |
| 2-54 | `crates/pc-ui-egui/src/move_ui.rs` | high | NOT A BUG | select.movePixels int_param rounds fractional dx/dy to the same clamped integer offsets as drag_offset. Preview and final pixel positions agree. |
| 2-55 | `crates/lc-ui-egui/src/panels/edit.rs` | medium | NOT A BUG | The guard applies to a control that no longer owns the interaction; ending it would terminate another control/new edit. Commands already end or cancel old interactions; finish_slider_if_released closes lost releases. |
| 2-56 | `crates/lc-ui-egui/src/panels/second.rs` | medium | NOT A BUG | Renderer::request/request_needed deduplicate current and pending slot/key/priority; the offload queue also retains only one request per slot. |
| 2-57 | `crates/lc-ui-egui/src/panels/settings.rs` | medium | NOT A BUG | Both PeopleTask and SmartSortTask implement Drop to set their cancellation tokens. They hold prepared immutable input and send results through a dropped receiver, not live session references. |
| 2-58 | `crates/lc-ui-egui/src/panels/settings.rs` | medium | REAL | Display clear-face-data errors inside Settings confirmation. ds2_clear_face_data_failure_is_displayed_in_settings clicks the control against an owned directory that cannot be deleted as a cache file. |
| 2-59 | `crates/pc-ui-egui/src/titlebar.rs` | high | NOT A BUG | egui RawInput::default inserts ViewportId::ROOT; kittest retains it. The title-bar UI regression can safely update that entry. |
| 2-60 | `xtask/src/tool_art.rs` | medium | REAL | Create both monochrome and color output directories. ds2_redraw_creates_color_directory_and_reports_unknown_family runs in a fresh owned fixture. |
| 2-61 | `xtask/src/tool_art.rs` | low | REAL | Return a contextual recoverable error for unknown palette families instead of panicking. ds2_redraw_creates_color_directory_and_reports_unknown_family. |
| 2-62 | `crates/lc-ui-egui/src/lib.rs` | medium | REAL | Suspend the pending People overlay while another dialog owns input; allow the deliberate Smart Sort workspace overlay. ds2_finder_retains_download_error_and_settings_owns_input clicks Settings tabs with a finder pending. |
| 2-63 | `crates/lc-ui-egui/src/lib.rs` | low | NOT A BUG | develop.endInteraction is idempotent: Session::end_interaction returns Ok when no interaction exists. No spurious status error is emitted. |
| 2-64 | `crates/lc-ui-egui/src/panels/people.rs` | medium | REAL | Reuse Session.visible_shared Arc scope instead of copying every frame. ds2_imported_people_cache_shares_snapshots_and_empty_filter_remains_actionable; actual People app timing is also measured. |
| 2-65 | `crates/lc-ui-egui/src/panels/people.rs` | medium | REAL | Use the existing shared people snapshot directly when recognition is off; cache the filtered snapshot when on. ds2_imported_people_cache_shares_snapshots_and_empty_filter_remains_actionable. |
| 2-66 | `crates/lc-ui-egui/src/panels/people.rs` | low | REAL | Build an ASCII-folded known-name HashSet on imported-snapshot refresh, then use linear membership filtering; unchanged frames share the result. ds2_imported_people_cache_shares_snapshots_and_empty_filter_remains_actionable. |
| 2-67 | `crates/lc-ui-egui/src/panels/people.rs` | low | REAL | Render filter chips and actionable empty states even when no imported people remain. Actual filtered People UI regression ds2_imported_people_cache_shares_snapshots_and_empty_filter_remains_actionable. |
| 2-68 | `crates/lc-engine/src/media.rs` | low | NOT A BUG | A short String and uncontended mutex once per complete image render is not a demonstrated hot-path bottleneck. Formatting is negligible relative to actual develop work; no speculative optimization. |
| 2-69 | `crates/pc-ui-egui/src/transform_tool.rs` | medium | REAL | Prevent multi-layer Warp entry and retain session/preview if an invalid multi-layer Warp reaches commit. Actual Warp-toggle regression ds2_multilayer_warp_keeps_pending_box_and_preview. |
| 2-70 | `crates/pc-ui-egui/src/transform_tool.rs` | medium | REAL | Build multi-layer moving textures with the same linked-mask policy as single-layer previews, recursively excluding stationary masks. ds2_multilayer_preview_carries_only_linked_masks checks the actual preview pixels. |
| 2-71 | `crates/lc-ui-egui/src/people_task.rs` | medium | REAL | Partition missing inputs using a BTreeSet of prepared IDs instead of a nested search. ds2_missing_input_partition_includes_only_unprepared_ids covers duplicates and 50,000 IDs. |
| 2-72 | `crates/lc-ui-egui/src/people_task.rs` | low | REAL | Count missing-photo completion and headshot imports/skips; headshot Done reaches total. ds2_headshot_progress_counts_imported_and_skipped_images decodes owned fixtures through the worker. |
| 2-73 | `crates/lc-ui-egui/src/people_task.rs` | medium | REAL | Bound headshot textures to 256 entries with recency eviction and preserve pending decodes. ds2_headshot_cache_bounds_textures_and_concurrent_decodes checks texture allocations and eviction. |
| 2-74 | `crates/lc-ui-egui/src/people_task.rs` | medium | REAL | Report worker disconnection/Done errors, retain failure across polls and prevent successful-step advancement. ds2_disconnected_face_worker_reports_failure_and_does_not_advance. |
| 2-75 | `crates/lc-ui-egui/src/people_task.rs` | medium | REAL | Limit headshot decode requests to four concurrent workers, retrying pending visible requests on repaints. ds2_headshot_cache_bounds_textures_and_concurrent_decodes. |
| 2-76 | `crates/pc-ui-egui/src/move_input.rs` | medium | NOT A BUG | move_mods::abandon/finish discards transient modifier state and cached viewport resources; cleanup does not need app.drag or document mutation. Clearing the drag first loses no restoration state. |
| 2-77 | `crates/pc-ui-egui/src/move_input.rs` | medium | REAL | Cancel active Move capture when a dialog opens. Actual drag/shortcut regression ds2_move_escape_uses_event_modifiers_and_dialog_open_cancels_capture. |
| 2-78 | `crates/pc-ui-egui/src/move_input.rs` | medium | NOT A BUG | Capture-time zoom/flip and origin deliberately keep deltas continuous across panel/viewport relayout; converting the same pointer with a changed current origin would jump the layer. No permitted mid-gesture zoom/flip operation is demonstrated. |
| 2-79 | `crates/pc-ui-egui/src/move_fast.rs` | medium | NOT A BUG | canvas_display_for derives encode_srgb only from composite_profile(doc), not mutable display settings. Changes to the document profile replace the immutable Arc and invalidate FastMove; monitor/proof changes are applied through the separate display LUT/signature path. No stale eligibility flag exists. |
| 2-80 | `crates/pc-ui-egui/src/move_fast.rs` | medium | REAL | Never construct cold Move planes while a drag is active; prepare in idle frames and use the existing fallback during cold gestures. Native GPU app regression ds2_move_cold_press_defers_cache_and_release_or_recovery_frees_viewport_resources. |
| 2-81 | `crates/pc-ui-egui/src/gpu_canvas.rs` | high | ALREADY FIXED / OBSOLETE | display_color declares var rgb and let grid immediately before their uses. Native pipeline creation/regression tests verify the actual shader. |
| 2-82 | `crates/pc-ui-egui/src/gpu_canvas.rs` | medium | REAL | Transient recovery drains Move targets and associated per-plane/base view resources. Native GPU app regression ds2_move_cold_press_defers_cache_and_release_or_recovery_frees_viewport_resources. |
| 2-83 | `crates/pc-ui-egui/src/gpu_canvas.rs` | medium | REAL | Move finish/cancel frees viewport targets and view uniforms immediately. Native GPU app regression ds2_move_cold_press_defers_cache_and_release_or_recovery_frees_viewport_resources. |
| 2-84 | `crates/pc-ui-egui/src/gpu_canvas.rs` | low | NOT A BUG | The move texture is a screen-space composite reused across pans and callback rectangles. Its coordinates/scissor match the full screen; one bounded texture per active view is intentional, not a demonstrated bandwidth defect. |
| 2-85 | `tests/support/panic_hunt.rs` | high | NOT A BUG | Rust Child caches the reaped exit status; Unix Process::kill returns Ok without sending a signal when status.is_some(). Worker Drop cannot signal a reused PID after try_wait reaps it. |
