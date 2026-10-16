# Performance execution matrix — preparation only

Prepared 2026-10-12 from the original `workers/v2-performance.md`, `STAGE4-PERFORMANCE-TASK.md`, `PERFORMANCE-CURRENT-TRIAGE.md`, `../EXECUTION.md`, installed `specs/bugfix/speed.md` and `specs/swarm/swarm-gpu.md`, Astra's actual review/prototypes, and read-only inspection of the recovery candidate. Candidate HEAD observed: `6acff8451aa59457a64a7390af7c029204bb7bde`. The candidate has concurrent integration edits; this is a source-inspection snapshot, not final-candidate acceptance.

No Cargo, compilation, benchmark, inference, GPU workload, source edit, dependency download, installation, commit or agent delegation was performed for this preparation. Only this file was written in the original checkout. Execution is frozen until the coordinator assigns the exclusive performance interval and, where needed, a coding permit/file ownership.

## Evidence that must survive

Candidate `docs/wip/v2-recovery-validation/release-vector-pointer-baseline.{json,log,sources.json}` records **ddd5411e01c409d80dac2b086fc6e6b37a6b2bc6**, RTX 5090/Vulkan/discrete, GPU canvas with `fallback: None`, 6000×4000 RGB8/500 shapes, 64 warm samples. First click plus GPU/readback **91.678 ms**; reported warm median **8.068 ms**, p95 **8.662 ms**, p99 **10.537 ms**; release **8.304 ms**, 64 uploads/64 rebuilds. The original `<16 ms` median assertion passed unchanged. The recorded source-manifest digest is `8ef7c724d33a0f649dbbc8e24f9e4a55c85028b02926946379bf6450327ad77a`, lock digest `373da4aa462c3294fc243412d508f53454b0eb20bac2d1237fc318d17a46fefc`, log digest `9ed98a222a0f9d9523e5c7b7e280d118518ddfc30327b1894910af92b9422eeb`.

This is a combined pointer/app/GPU/readback observation, not standalone UI latency or a speedup over another commit. First-click timing follows fixture setup and earlier frames; do not rename it process/device cold start. FirstMove, pan, zoom, fresh device, fresh document and warm pointer frames need distinct labels. Preserve the legacy statistics as reported; for new raw samples state the quantile convention. Some existing tests use fixed sample indices rather than a common nearest-rank implementation.

Original review source remains at `/home/zdavidson/Documents/Local Image - Testing/.claude/worktrees/codex-final-review-astra/docs/wip/final-review/`: `perf.rs`, `interactive_required.rs`, `models.rs`, `interactive-results.md` and driver scripts. Their `target/final-review` raw JSON/image artifacts were absent from the inspected original/review/candidate locations. The candidate does not expose a `final_review_probes` test target. Do not run the old drivers: their APIs, workflow, broad job lists and private test modules are historical. Adapt only a needed fixture into the current production path after a permit.

The original Save As review describes a three-layer fixture; retained `perf.rs` explicitly pushes one patterned raster into `Document::new`. Verify actual final layer count before calling a new fixture an identical three-layer comparison. The current interactive example separately constructs its layered scenario. Historical numbers are context, never a current A/B baseline.

## Proposal-by-proposal disposition

| Proposal / original evidence | Current disposition and production path | Required next measurement / equivalence | Priority |
|---|---|---|---|
| Scoped AI watcher; historical 100.396→5.273 ms bridge median, 16 interleaved pairs | **Implemented.** `pc-engine/src/ai_cmds.rs:76` uses scoped thread, drop completion signal and `recv_timeout(100 ms)`; do not replace it again. | Reuse the 5 ms callback and 16 alternating pairs only as bridge-overhead protocol: current `bridged` versus direct callback, identical four RGBA bytes. Name direct-callback control honestly, not “old application.” Record normal/error/unwind/cancel and join latency; existing `final_review_bridge_stops_on_unwind_and_preserves_results` is the regression. A current timing harness is needed; retained prototype is not wired into candidate. No model inference needed. | P2, cheap bounded confirmation |
| Snapshot Save As worker; old dispatch 735.247→0.02475 ms but total 735.247→787.931 ms | **Implemented for ordinary native Save As.** `save_worker.rs` captures Arc/doc/revision, encodes/stages in a worker and checks publication authority; scripts/assets still run during UI completion. Automation remains scoped synchronous. | Actual `PhotocraftApp::save_as`/native writer path, 6000×4000 RGB8 and verified three layers; one cold observation then six interleaved pairs for comparable exploratory median only. For a tail claim collect 30 paired completions, not six-sample p95. Separate snapshot capture/dispatch, idle UI frame cost, worker encode/stage, final publication, scripts/assets and total. Compare PSD bytes and reopened document/pixels; slow/failing writer, edit/switch/close, cancel-ready race and history/save revision assertions. Old memory-sink prototype is insufficient for native publication timing. | P1 UI completion audit; no encoder speed claim |
| 512 px Navigator, historical 501.3 ms one observation | **Worker already implemented.** `canvas.rs:658–779`: >4 Mi pixels including first image, one worker per document, Weak snapshot + preview/display keys; stale result rejected. Raw `thumbnail_buffer` remains CPU composition. | Current `navigator_texture` request-to-placeholder/UI-return versus worker generation versus accepted texture publication; 24 MP fixture, initial/warm/edit/display-change/close/drag. One cold plus 30 completion samples per relevant state if claiming p95; do not time only `thumbnail_buffer` and call it UI blocking. Existing `ds_finding_real_app_navigator_job_finishes_during_drag` verifies production interaction. | P1 verify current native behavior; GPU only if generation matters |
| Live swatches, acceptance 43; old 2,000-object median 572.933 ms / release 418.092 ms | **Still credible hotspot, not yet measured on candidate.** `pc-engine/src/vector_packages/colours.rs:327`: validates/resolves palette, deduplicates `(symbol,layer)`, then compiles appearances once for aggregate budget and again for render; synchronous commit. Dependent instances already refreshed. | Run current 2,000-object fixture, n=24 pointer frames + separate Apply. Profile resolution, compilation/budget check, cache rasterization, detached document projection, app frame, upload and readback. If justified, preserve compiled work only within correct ownership/budget limits. Preview/commit max ≤1 8-bit level, exact cancel/undo, one journal, affected objects only. | **P0 first optimization candidate after baseline** |
| Warp 27, scaled node 32, selection 52, view mode 53 | Common tree projection/preparation still appears on live UI paths; async exports did not make editing async. Scaled node 32 now has retained passing current-ish release evidence. Other old failed frame gates need current runs before a fix. | Existing production pointer tests below; separate first click, firstMove, warm UI/route, GPU submission/wait and readback. Warp/selection already print UI versus GPU/readback route phases. Keep all existing median/p95 assertions, exact geometry, detached preview and history/cancel checks. Selection's old ~0.03 ms route does not establish the cause of its full-frame delay. | **P0 phase breakdown; P1 measured fix** |
| Live Paint 36, mesh 37, Blob 56, text path 64, shared Pen/shape 65 | Retained production fixtures remain available. Historical passes did not prove a 16 ms user experience; Blob release was an outlier. | Replay original sample counts below, press/preview/commit independently; CPU reference pixels and geometry, preview=release, cancel/undo/source edits. Mesh fixture is only 120×120 and cannot stand in for 24 MP. Shared Pen uses TWO app steps plus readback and must retain its idle baseline. | P1, after swatch/projection |
| Measurement overlay 62 | Historical passing 7.885/12.768 ms; no new defect established. | Existing n=35 control fixture; prove overlay does not cause layer render. Use as control if common projection changes invalidate it. | P2 control, no speculative rewrite |
| Fixed-tile CPU Rescue | **Still bounded expensive inference.** `li-seg/src/enhance.rs` uses 256 tile, 112 halo for `bsrgan-4x`, 32 core; node evaluation calls cancellation check. No reason to reduce halo. | Begin with retained 32×24→128×96 production-service recipe (one cold + three warm observations; report samples/range, no p95). Break out archive/model preparation, padding/tensor packing, each inference node/block, crop/blend and cancel-to-stop. Historical 192×128/40-tile run is explicitly prohibited as the initial probe. Real tiled/whole test offers 44×16 seam fixture for this model. Keep ≤1-level historical halo-equivalence target for a proposed halo change; existing test currently asserts RMSE<1 and max≤6 and must not be weakened. | P2 bounded CPU/model lane; no generic enhance_probe CPU run |
| O040 compositor recreation on zoom/pan | **Implemented.** `gpu_canvas.rs` retains per-view compositor while replacing targets, records construction failures separately from projection refusal. Existing counter/parity tests available. | Small fixtures first: zoom buckets, viewport aprons, real pan/zoom events, firstMove and 1× CPU refusal. Then 24 MP/500 shapes. Phase counters: compositor creations, target allocations, projection rebuilds, uploads; first vs warm. New timing spans required for the 91.678 ms first-click attribution. | **P0 first diagnostic; do not redo cache architecture** |
| FC001 ownership / bounded pattern cache / cleanup | **Implemented in prior accepted work.** Existing replacement, close, budget and colour-profile regressions are present. | Measure peak/resident/released bytes and warm latency on bounded working set; include document close/reopen and profile/pattern replacement. Do not introduce another cache or redo ownership. | P2 final memory acceptance |

## GPU candidate audit (original swarm scope)

| Area | Current source evidence / disposition | Bounded execution and decision gate |
|---|---|---|
| Develop panels, masks, local adjustments | Many production stages already GPU; `lc-gpu/tests/it/{equivalence,toolset}.rs` cover tone, WB, local masks, depth, wavelet NR, sharpening, lens, curves, colour and finish bands. Backend test logs actual adapter and CPU reasons. | Production slider suite with synthetic input first; test full settings/changed control through actual UI, then approved RAW when located. Separate preview availability and settled full quality. `--test it` is current target; old `--test toolset` comments are stale. Identify unsupported or over-limit reasons from production logs before choosing a conversion. |
| RAW decode and denoise | Decode/linear preparation are separate from GPU Develop. Existing `denoise_bench` uses actual prepare/pack/infer and can explicitly select CPU/GPU. Model cache exists. | No arbitrary 100/400 MP rescaling. First `model … bayer gpu` single tile, then one approved small RAW or bounded 1 MP packed workload if provenance restored. Record model load separately from warm infer, bytes and cancellation. No claim that a GPU denoise kernel fixes CPU RAW open. |
| Compositing filters, blends, adjustments, brushes | `GpuCanvas` benchmark asserts `gpu-full`/damage routes after successful adapter creation; compositor parity/effect/mask suites already exist. Some helpers return early if GPU unavailable. | 6000×4000 procedural fixture: cold projection, warm adjustment, 96×96 damage, 400-point stroke preview/commit, explicit CPU reference. Verify adapter and nonempty sample output; “test passed” alone is insufficient. Time CPU flatten in PNG/JPEG export separately. |
| Vector rasterization/live effects | Existing GPU scaled projection and geometry paths coexist with CPU prepared caches/tree traversal. WGSL availability does not identify the slow phase. | Use table below and existing CPU/GPU geometry/appearance tests. Only convert a phase shown to dominate after dispatch/upload/readback separation; retain refusal fallback reason and exact state/geometry semantics. |
| Thumbnails/previews | Navigator already asynchronous. Develop thumbnails use cached render/embedded RAW previews; distinct subsystem from Navigator. | Navigator production fixture above. `thumb_request_bench` deliberately uses a blocked offloader for UI allocation/dispatch tests, so it cannot prove rendered thumbnail throughput. Do not conflate the two. |
| Export resampling | `lc-gpu/src/render.rs` already has separable GPU resize and production Mitchell resize. `lc-engine/src/export.rs:472` CPU resize is **watermark logo coverage**, not proof that all image export resize is CPU. | Profile actual export with resize and transparent logo separately, use same filter/taps/alpha/colour space; include upload+readback+encode and bounded source/output. A local WGSL twin is not justification for changing watermark or small-image CPU paths. Current generic export benchmark is baseline/phase reconnaissance only until fallback logging is strict. |

## Admission and reproducibility

All commands below are a future recipe, **not permission to start now**. Coordinator must first freeze the candidate, resolve file ownership, grant one exclusive release/build interval, then one exclusive runtime interval. Keep installed app/ComfyUI alive; if their jobs are active, defer measurement rather than terminating them. No compiler, inference or other acceptance run may overlap. Do not clear caches to manufacture a cold run. Record desktop GPU baseline (previous run: 7%, 2785 MiB) and process/GPU identity rather than assuming an idle desktop is zero load.

Use the existing recovery validation runner if available, retaining its source manifest/profile isolation; do not replace wrappers mid-validation. Equivalent explicit shell setup for an approved future interval:

```bash
cd '/home/zdavidson/.codex/worktrees/v2-recovery/Local Image - Testing'
export RUSTUP_TOOLCHAIN=1.98.1
export RUSTC=/home/zdavidson/.rustup/toolchains/1.98.1-x86_64-unknown-linux-gnu/bin/rustc
export RUSTDOC=/home/zdavidson/.rustup/toolchains/1.98.1-x86_64-unknown-linux-gnu/bin/rustdoc
export RUSTC_WRAPPER=/home/zdavidson/.local/share/local-image-dev/bin/li-rustc-gate
export CARGO_BUILD_JOBS=4 RUST_TEST_THREADS=2
export CARGO_TARGET_DIR="$PWD/target"
export CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_RUNNER=/home/zdavidson/.local/share/local-image-dev/bin/li-test-runner
export CRAFT_FONTS_DIR=/home/zdavidson/.local/share/local-image-dev/engine-sources/craft-fonts
export CRAFT_FONTS_REQUIRED=1
perf_profile="$PWD/target/recovery-profiles/stage4-performance"
mkdir -p "$perf_profile"/{library,local-image,models,config,data,cache,state,tmp}
export LIGHTCRAFT_LIBRARY="$perf_profile/library" LOCAL_IMAGE_DATA_DIR="$perf_profile/local-image"
export LOCAL_IMAGE_MODELS_DIR="$perf_profile/models" LOCAL_IMAGE_COMFY=127.0.0.1:9
export XDG_CONFIG_HOME="$perf_profile/config" XDG_DATA_HOME="$perf_profile/data"
export XDG_CACHE_HOME="$perf_profile/cache" XDG_STATE_HOME="$perf_profile/state" TMPDIR="$perf_profile/tmp"
export LIGHTCRAFT_GPU=1 LIGHTCRAFT_GPU_BACKEND=vulkan WGPU_BACKEND=vulkan WGPU_POWER_PREF=high
export LC_REQUIRE_GPU=1 LOCAL_IMAGE_REQUIRE_GPU=1
perf_cargo=/home/zdavidson/.rustup/toolchains/1.98.1-x86_64-unknown-linux-gnu/bin/cargo

# BUILD interval only: prepare just the selected first tranche, once.
"$perf_cargo" test --offline --frozen --release -p photocraft-ui-egui --lib --test gpu --no-run

# Runtime helpers. Include-ignored covers both ordinary and opt-in gates without changing tests.
# A unique filter must report exactly the expected test count; zero tests is an invalid run.
pc_lib() { "$perf_cargo" test --offline --frozen --release -p photocraft-ui-egui --lib "$1" -- --include-ignored --nocapture --test-threads=1; }
pc_gpu() { "$perf_cargo" test --offline --frozen --release -p photocraft-ui-egui --test gpu "$1" -- --include-ignored --nocapture --test-threads=1; }
```

Do not run a Cargo runtime command if it unexpectedly rebuilds: finish the build interval, wait for compiler exit, then start the recorded runtime anew. For exact comparison with ddd5411e, retain its original command (`-p photocraft-ui-egui -p local-image --features local-image/heif --lib --bins -- … --ignored --nocapture`) and manifest; the narrower helper can unify features differently, so do not silently present its result as an identical build. Record all effective env/flags, HEAD, dirty-file source hashes, Cargo.lock, compiler/driver, adapter, viewport/scale and fixture seed.

Use process peak RSS (`/usr/bin/time -v` around the actual runtime or existing in-process `RssSampler`) plus explicit compositor/texture/model resident-byte counters. Sample `nvidia-smi --query-gpu=name,driver_version,utilization.gpu,memory.used --format=csv -lms 200` to a log for the bounded interval, stopping only that sampler afterward. Device-wide memory includes the live desktop; report baseline and peak with this limitation. This cannot replace application GPU byte counters or resolve a sub-200 ms transient.

For A/B: same final fixture/build options, one separately recorded cold observation per arm, alternating AB/BA order, one source change/hotspot, retained per-sample phases. Original counts below reproduce original acceptance; repeat blocks only to answer a defined question. For a new cross-run p95 claim use at least 30 paired operations or multiple fixed gesture blocks (retain per-block distribution and ordering); never derive p95 from three cold starts. Gesture frames are correlated, not independent app launches. Do not silently discard outliers.

## Runnable current vector sequence and exact filters

Run these sequentially, not a broad `--ignored` suite. All inputs are procedural repository fixtures, not owner photos. Do not relax failures. A failed combined-frame gate is evidence to profile, not permission to remove readback or change its threshold.

| Order / original case | Future command | Built-in sample/input contract |
|---|---|---|
| 1, O040 correctness | `pc_lib scaled_compositor_reuses_pipelines_across_zoom_buckets` then `pc_lib scaled_compositor_reuses_pipelines_across_viewport_aprons` | Counter assertions across target recreation; no timing claim. |
| 2, real pan/zoom and parity | `pc_lib scaled_full_app_actual_pan_zoom_pointer_events` then `pc_lib scaled_full_app_rtx5090_parity_2_4_8x_hidpi_flip_and_warm_cache` | Small mixed fixture, zoom 2/4/8 and fractional 1.7, HiDPI/flip; CPU/GPU premultiplied max ≤1/255+epsilon. |
| 3, original 32 | `pc_lib scaled_release_6000x4000_500_shapes_node_pointer_frames_rtx5090` | 6000×4000/500 shapes, n=64, first press + release, uploads/rebuilds, median<16. Requires extra permitted spans for first=91.678 ms attribution. |
| 4, original 43 | `pc_lib colour39_full_app_2000_objects_release_pointer_frames` | 2,000 objects, n=24, separate Apply; snapshot unchanged during preview, one journal. Pair with `pc_lib colour39_full_app_rtx5090_live_swatch_preview_cancel_release_one_journal`. |
| 5, original 27 | `pc_lib distort41_gpu_release_app_pointer_frames_2000_nodes_at_least_60hz` | 2,000 nodes, n=90 **per tool**, separate press/release, UI/GPU-readback/route samples, p95≤1000/60. It iterates all tools; budget accordingly. |
| 6, original 52 | `pc_lib selection_full_app_release_500_pointer_frames_under_16ms_rtx5090` | 500 selected shapes, five baseline frames and n=30 measured pointer frames, route/UI/readback phases. |
| 7, original 53 | `pc_lib view37_full_app_gpu_5000_paths_warm_pointer_frames_under_16ms_release` | 5,000 paths, four warm-up frames, n=48 stationary Hand-pointer frames; this is not moving-pan timing. |
| 8, original 36 | `pc_lib live_paint42_full_app_release_click_to_paint_latency` | n=20 press/release/painted-frame cycles, CPU parity and undo after each. |
| 9, original 56 | `pc_gpu blob40_full_app_release_pointer_frame_measurement` | n=32 actual pointer frames plus release, final GPU/CPU comparison. |
| 10, original 37 | `pc_lib mesh43_gpu_app_20_by_20_pointer_frame_budget` | 20×20 mesh in 120×120 document, n=60, separate release; ordinary in release, so use include-ignored. |
| 11, original 64 | `pc_gpu path_text_app_gpu_release_pointer_frame_measurement` | n=80, actual path text, initial/first event distinct from warm. |
| 12, original 65 | `pc_gpu shared_pen_shape_large_document_release_pointer_frames_rtx5090` | 6000×4000/500 shapes, n=64 idle and n=64 held **each Pen/Rectangle**, two app steps+readback; RSS/upload counters. |
| 13, control 62 if common paths changed | `pc_gpu measure44_full_app_release_warm_pointer_frames_under_16ms` | n=35, unchanged scene/overlay; no-layer-render invariant. |

The existing Move benchmark `pc_gpu move_canvas_bench_8000x6000_real_app` is **48 MP and not admitted in the initial tranche**. Start with `pc_gpu ds2_move_cold_press_defers_cache_and_release_or_recovery_frees_viewport_resources` and small stack/flip/HiDPI tests; a permitted 24 MP timing adaptation should use the approved fixture and production Move route. Keep first press/cache construction and first actual motion separate.

## Other executable gates and measurement prerequisites

```bash
# Existing correctness gates (build their targets first in the assigned build interval).
"$perf_cargo" test --offline --frozen --release -p photocraft-engine --lib final_review_bridge_stops_on_unwind_and_preserves_results -- --nocapture --test-threads=1
pc_lib background_save_
pc_gpu ds_finding_real_app_navigator_job_finishes_during_drag
pc_gpu realistic_document_composites_on_the_gpu
pc_gpu bench_gpu_canvas_24mp

# Actual Develop UI: procedural photo by default, 120 pointer moves over ~2 seconds.
LC_SLIDER_BENCH_SIZE=1920x1080 LC_SLIDER_BENCH_PPP=1 \
  "$perf_cargo" test --offline --frozen --release -p lightcraft-ui-egui --lib develop_slider_drag_latency -- --ignored --nocapture --test-threads=1
"$perf_cargo" test --offline --frozen --release -p lightcraft-engine --test develop_backend -- --nocapture --test-threads=1

# Scoped GPU/CPU equivalence and fault/refusal fallback; targets are current, not stale comments.
"$perf_cargo" test --offline --frozen --release -p lightcraft-gpu --test it equivalence::gpu_matches_cpu -- --exact --nocapture --test-threads=1
"$perf_cargo" test --offline --frozen --release -p lightcraft-gpu --test fallback -- --nocapture --test-threads=1
"$perf_cargo" test --offline --frozen --release -p photocraft-gpu --test scaled_view -- --nocapture --test-threads=1
"$perf_cargo" test --offline --frozen --release -p photocraft-gpu --test gpu parity::pattern_ -- --nocapture --test-threads=1

# Single real denoiser tile; GPU required and inspect printed adapter/resident bytes.
"$perf_cargo" run --offline --frozen --release -p lightcraft-engine --example denoise_bench -- \
  model /home/zdavidson/.cache/local-image-test-models/denoise bayer gpu

# Small real Rescue seam equivalence, explicit CPU model; no Comfy server needed.
LOCAL_IMAGE_TEST_MODELS=/home/zdavidson/.cache/local-image-test-models ENHANCE_CANDIDATE=bsrgan-4x \
  ENHANCE_OUTPUT="$perf_profile/rescue-equivalence" \
  "$perf_cargo" test --offline --frozen --release -p li-seg --lib enhance_real_tiled_vs_whole -- --ignored --nocapture --test-threads=1
```

Hardware qualification is not implied by environment names. `bench_gpu_canvas_24mp` can return early without an adapter; several compositor parity helpers also skip. Require nonempty measured rows and captured **RTX 5090 / Vulkan / discrete** evidence, `gpu-full`/`gpu-damage` as appropriate and no unexpected fallback. For real CUDA service work require server `/system_stats` naming the RTX 5090 and a read-only `/queue` check with zero running/pending before submission. CPU-fallback tests deliberately exercise CPU and must report the reason; do not reject their intended fault path as a normal GPU run. The single denoise model example prints resident bytes but not a complete adapter record; capture `lightcraft-gpu` adapter evidence or add an explicit permitted assertion before hardware acceptance.

`bench_gpu_canvas_24mp` has one cold full refresh, nine warm full adjustments, 40 GPU brush damage samples, five explicit CPU damage samples, three live-preview observations, 20 thumbnail fingerprint samples and one raw Navigator generation. Report those exact counts; it does not provide robust Navigator p95 or async UI timing. Do not run all CPU symmetry/stutter benchmarks without a measured reason.

`interactive_bench --size 6000x4000 --reps 30 --json "$perf_profile/interactive.json"` is an existing Session/open/brush/filter/adjustment/move/undo/native-save/PSD-save/PNG/JPEG production-path reconnaissance example (`cargo run … -p photocraft-ui-egui --example interactive_bench -- …`). **Not strict GPU acceptance:** its `render()` catches compositor error and silently CPU-renders; `LC_REQUIRE_GPU=1` does not fix that. `--only` still executes other rows once for setup. Before using it for GPU claims, permitted instrumentation must assert adapter + render result and emit fallback reason, or adapt the saved strict `interactive_required.rs` using current APIs. One cold + three warm samples can replicate original exploratory protocol; use 30 interleaved pairs for a new tail claim. Saving through `Session` does not measure asynchronous native Save As UI dispatch.

The original real RAW name is `Sony-ILCE-1-CC0-4465.ARW` under review `target/final-review/fixtures`; it was not found in the inspected retained directories. Candidate `photo.ARW` files under overwrite-failure test profiles are negative-test artifacts and **not approved replacements**. Restore/locate the licensed fixture and verify hash/provenance before `LC_SLIDER_BENCH_FILE=…` or `denoise_bench RAW_PATH MODEL_DIR gpu 1 OUTPUT bayer-only`. That latter 1 MP setting measures rescaled packed input, not native RAW throughput; label it so. The API accepts much larger rescaling but this plan does not authorize it. CPU comparison uses `LIGHTCRAFT_GPU=0` with both require-GPU variables unset, a separate isolated run, identical settings and pixels. Use 1× first, then 2×/clarity/masks only if those paths are the subject of the measured change.

Available approved installed model artifacts observed: `/home/zdavidson/.cache/local-image-test-models/segmentation/upscale-bsrgan.dtmodel` and `/home/zdavidson/.cache/local-image-test-models/denoise/segmentation/{denoise-nind,rawdenoise-nind-bayer,rawdenoise-nind-linear}.onnx`. Verify selected spec/hash before execution; presence is not a substitute for model compatibility. No downloads.

The 32×24 Rescue recipe exists in saved `models.rs::final_review_rescue_cpu_single_tile`, using repository `docs/images/examples/removal-before.png`, Lanczos3 resize, `Ai::new("127.0.0.1:1")`, `Backend::Cpu`, `Choice::Curated { id: "bsrgan-4x" }`, output 128×96. It is **not a current runnable candidate test target**. Wire a focused production-service phase probe only after a coding permit; do not substitute `enhance_probe -- cpu`, which runs the previously excessive multi-tile matrix. The existing seam test above is runnable and only uses 44×16 for this candidate, but covers several fixed model executions and must still have its own CPU/inference-exclusive budget.

## Completion requirements / handoff

1. Retain every original proposal row, including implemented/unsupported/not-justified outcomes. No silently dropped slow feature.
2. First permitted edit should add narrowly scoped measurement boundaries where missing (swatch preparation/compile/render, first node/Move, native save completion, Navigator publication). Do not optimize blindly from the old 573/501 ms observations.
3. Record raw samples, source hashes, exact effective command/env, cold/first/warm distinctions, median/tail/max, peak RSS and attributable GPU residency; render/equivalence artifacts stay under isolated validation output.
4. After each justified coherent change, run the focused failing-before/passing-after regression and unchanged appropriate equivalence/fallback tests. At a stable batch boundary run affected lint/suites once, not a workspace loop. Final supported workspace validation belongs to coordinator's final gate.
5. Publish measured results to candidate `docs/wip/V2-PERFORMANCE-PROGRESS.md` and `docs/wip/CODEX-REPORT-v2-performance.md` only in the later authorized workstream. This matrix does not claim performance completion.

Freeze state: preparation complete; no active work/build/GPU process started by this worker. Timing-harness gaps and missing RAW provenance are explicit next-step prerequisites, not permission to weaken tests or substitute owner photos.

## Inspected source fingerprints

SHA-256 at preparation close; concurrent integration may invalidate these. Paths below are relative to the candidate root stated above.

| File | SHA-256 |
|---|---|
| `Cargo.lock` | `373da4aa462c3294fc243412d508f53454b0eb20bac2d1237fc318d17a46fefc` |
| `crates/pc-engine/src/ai_cmds.rs` | `8e399cf710fda98074be7543d009a2685c99067f5bb43d3a1125d365883c3ebd` |
| `crates/pc-ui-egui/src/save_worker.rs` | `d3ca5a6a89c78915f86c3fce4d2c8f9abbf55fa76ffb7f17477e5961fa5df73f` |
| `crates/pc-ui-egui/src/canvas.rs` | `872e63c401e82f33ef7d79593028b33a0b898be8e4ce38ac937941bf3eb2dc73` |
| `crates/pc-ui-egui/src/gpu_canvas.rs` | `1db0aafda083c2709630af41c731297cdab49d8bcd36c7e2eb070427ca696c8d` |
| `crates/pc-ui-egui/src/gpu_canvas_view_tests.rs` | `4310416bf1c7e99a898ab8660f3e09cd4b484a35cb68a2d50160fbe041fed215` |
| `crates/pc-engine/src/vector_packages/colours.rs` | `0433639e15d20254f2872dbd89e312cc8fdb4da65b5e910cefa928244224dbb4` |
| `crates/li-seg/src/enhance.rs` | `97010900d1ceb659cf5115e324dbe77e053ce7e88877af8fd736ac6fc85fd9de` |
| `crates/lc-ui-egui/src/tests_slider_speed.rs` | `c40b84562d58530626245da12fd07a78eb7b6f8079830257c2af62cd00251d08` |
| `crates/pc-ui-egui/examples/interactive_bench.rs` | `e1041feded70ebb5383fedf9cad388356cde5701e28a2efa5efbd1f2e2f6f7d8` |
