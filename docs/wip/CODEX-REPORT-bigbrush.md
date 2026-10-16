# Large-brush responsiveness report

Continued the existing brush implementation at `90f444a9`: the continuous soft renderer, shared coverage tiles, live-commit reuse and real-GPU canvas harness were retained. The only initially modified file was `CODEX-TASK.md`; its user changes are preserved. No commit was made.

## 1. Measurement and diagnosis

Extended `brush_batch_real_gpu_large_brush_performance` to soft and textured-dual brushes at 300, 1000 and 2500 px, plus a 60 MP option (`BIGBRUSH_60MP=1`). It sends actual canvas press/move/release events into the offscreen eframe app, exercises production live strokes, verifies visible ink in the rendered GPU image, and asserts RTX 5090 / Vulkan / discrete adapter, active GPU canvas and no canvas CPU fallback. No desktop window is opened.

Debug-only, opt-in scopes now distinguish tail reconstruction, restore, coverage accumulation, stroke composition and GPU canvas refresh/submission. The harness separately times the complete UI step and GPU rendering/screenshot readback. Worker timings transfer to the receiving frame; they are CPU work done off the UI thread, not part of that frame's elapsed UI time. Tail and raster scopes include coverage work, so stage numbers are inclusive and must not be summed. `stage_ms_per_frame` is ordered **tail, restore, coverage, stroke composite, GPU refresh/submission**. GPU submission time is CPU elapsed time, not a GPU timestamp query. Screenshot readback is a harness operation; the production stroke worker performs none.

Instrumented baseline, 2500 px soft: tail 53.36 ms/frame, restore 128.15, coverage 17.16, composite 278.37, GPU refresh/submission 0.37; UI mean 480.47 ms, p95 651.08 ms. This confirms restoration and repeated blending dominate. Baseline load was 17.58 / 16.15 / 13.13, with other host jobs running; initial GPU utilization was 3%. The prior job's 431 / 626 ms numbers were not substituted for this run.

Measurements use optimized dev/test (`opt-level=1`), 1200×900 offscreen UI, five warm movement frames and 35 measured frames per case. The final harness initializes the same compute profile as application startup and paces input to at least 16 ms/frame, outside timing. Baseline frames already exceeded that cadence. The baseline harness used its existing default rayon startup; final runs use the application's configured pool. Host contention and pool initialization differ, so the table reports observed runs, not an isolated speedup factor or a shipping desktop FPS guarantee.

## 2. Established designs followed

- **Krita:** its [update scheduler](https://raw.githubusercontent.com/KDE/krita/master/libs/image/kis_update_scheduler.cpp) separates stroke jobs and projection updates; the [freehand helper](https://raw.githubusercontent.com/KDE/krita/master/libs/ui/tool/kis_tool_freehand_helper.cpp) drives asynchronous stroke/update work. This supports keeping interaction responsive while publishing completed dirty-region snapshots.
- **GIMP/GEGL:** [paint core](https://raw.githubusercontent.com/GNOME/gimp/master/app/paint/gimppaintcore.c) retains undo buffers and a separate float canvas mask. Constant painting combines coverage with the original buffer and updates the affected rectangle. This supports composing final coverage directly from the original pixels, rather than restoring then blending twice.
- **MyPaint:** [canvas documentation](https://www.mypaint.app/en/docs/backend/canvas/) describes sparse 64-pixel tiles; [libmypaint tiled surface](https://raw.githubusercontent.com/mypaint/libmypaint/master/mypaint-tiled-surface.c) processes queued tile work at `end_atomic` and reports dirty regions. This supports batching input and independently processing tiles.

These are design references, not copied C/C++ code. Implementation remains Rust and uses existing dependencies.

## 3. Implementation

- Live composition chooses the finished tail's coverage tile when present, otherwise base coverage, and blends each selected tile **once** from `pre`. Missing previous-tail coverage restores original pixels in that same pass. Both ordinary and mirrored strokes use this path. The old full tail-rectangle float restore and base-then-tail blend passes are gone.
- Large strokes have one named OS coordinator and bounded one-item input/output channels. Input that arrives while it is busy accumulates into one batch; every point is retained. The UI publishes completed document snapshots and never waits or joins the coordinator. The finished surface enters the existing command/undo path as one edit.
- CPU rows, coverage tiles and disjoint raster-storage tiles run on the existing global rayon pool. `pc-paint` accepts a host executor; it gains no dependency. Work bands use the frozen `lc-sysmem` compute budget, not a fixed worker count. Dab order and pixel arithmetic remain unchanged; tile results merge in deterministic order. Channel waits occur only on the OS coordinator. Pixel loops check cancellation at row/tile boundaries.
- Inline work cutoff is an area budget derived from compute threads, with a ceiling; lower-core profiles move smaller brushes to the worker. The four-core profile moves 300 px brushes off the UI as well.
- Release commands freeze brush, foreground/background, zoom, target and layer. Subsequent strokes retain their own settings, pose, input times and secondary-erase/resize state. Equivalent explicit parameters rebind the prepared stroke without rerasterization; settings changed during the gesture restart a worker. Cancelling or changing the document discards outstanding results. A prepared commit can still target its original raster after another layer is selected.
- Stationary queued airbrush contacts retain idle samples. The press timestamp now stays at zero rather than being overwritten by the first later stamp; redundant moves no longer change already-fed timestamps. Queued gestures also suppress duplicate egui drag-start/click fallback presses.

The GPU canvas continues compositing the document and uploading changed tiles. No GPU stroke kernel or extra GPU readback was introduced. Retaining the existing native-pixel renderer gives exact commit behavior across bit depths, masks, blend modes, dual/texture coverage and Wet Edges, while removing the dominant redundant work.

Ownership: brush engine/live code, paint, canvas, GPU-canvas instrumentation and the existing UI harness. `commands.rs` has one related integration change to the Brush command's enablement for a validated prepared commit; shortcut fields are unchanged. No keyboard shortcut, swatch, lc-*, li-seg or li-ai source was edited. Cargo manifests/lockfile are unchanged; no new dependencies or unsafe code were introduced.

## 4. Performance

Final runs use NVIDIA GeForce RTX 5090, Vulkan, driver 615.71.09. The normal process detected 16 logical / 8 physical cores and used seven compute threads. CPU affinity 0–3 detected four available logical CPUs and used three compute threads. All cases verified visible painted pixels in the actual GPU image.

| Brush | UI mean before → after (ms) | UI p95 before → after (ms) | Four-core UI p95 (ms) | GPU render/readback after mean (ms) |
|---|---:|---:|---:|---:|
| soft-300 | 18.51 → 9.74 | 23.49 → 12.41 | 5.08 | 5.99 |
| soft-1000 | 95.45 → 4.72 | 141.83 → 6.36 | 5.76 | 6.14 |
| soft-2500 | 480.47 → 5.16 | 651.08 → 9.21 | 8.99 | 6.54 |
| textured-dual-300 | 20.25 → 10.43 | 26.52 → 11.57 | 5.47 | 5.81 |
| textured-dual-1000 | 56.42 → 5.04 | 95.05 → 6.83 | 6.25 | 6.30 |
| textured-dual-2500 | 132.17 → 5.35 | 509.19 → 8.82 | 8.25 | 7.11 |

The final 1000 px soft UI p95 is below 16.7 ms, and the 2500 px soft UI p95 is below 33 ms. The four-core run also meets these UI targets. Coalescing reduces the number of expensive tail reconstructions without dropping input. These figures do not measure desktop presentation latency.

| Brush | Published worker batches / 35 frames | Mean worker batch (ms) | Maximum oldest-input publication latency (ms) | Release-to-commit completion (ms) |
|---|---:|---:|---:|---:|
| soft-300 | 0 | 0.00 | 0.00 | 10.96 |
| soft-1000 | 33 | 9.71 | 63.85 | 31.48 |
| soft-2500 | 9 | 53.39 | 164.27 | 90.47 |
| textured-dual-300 | 0 | 0.00 | 0.00 | 11.05 |
| textured-dual-1000 | 36 | 7.22 | 32.08 | 26.66 |
| textured-dual-2500 | 16 | 25.63 | 129.77 | 86.78 |

Latency starts when the oldest input in a batch was queued and ends when the UI accepts its snapshot. Release completion includes the release UI step and harness render/readback, then waiting for the final command; it is not a GPU-present timestamp. Inline brushes have no worker latency counter.

| Brush on 10000×6000 (60 MP), zoom 0.08 | UI mean (ms) | UI p95 (ms) | GPU render/readback mean (ms) |
|---|---:|---:|---:|
| soft-300 | 10.27 | 13.40 | 5.95 |
| soft-1000 | 4.69 | 7.15 | 6.64 |
| soft-2500 | 5.64 | 11.48 | 7.02 |
| textured-dual-300 | 11.74 | 12.93 | 5.89 |
| textured-dual-1000 | 5.81 | 8.82 | 7.34 |
| textured-dual-2500 | 5.50 | 10.53 | 6.85 |

Peak process RSS from cached-build `/usr/bin/time -v` runs (the harness retains all six documents):

- 24 MP, seven threads: 801.1 MiB.
  Host load before the corresponding performance run: 2.76 4.94 7.27 1/2533 3345668.
- 24 MP, four-core affinity: 791.1 MiB.
  Host load before the corresponding performance run: 4.31 5.05 7.20 1/2542 3348305.
- 60 MP, seven threads: 886.5 MiB.
  Host load before the corresponding performance run: 4.13 5.00 7.17 1/2544 3348351.

The first delivery command rebuilt test artifacts, so its command-wide RSS included compiler memory. The normal-profile memory number above comes from the cached `memory-delivery.log` rerun; the timing table retains the original delivery measurement.

These are six generated white documents per benchmark, matching the supplied canvas harness; no photo-decoding or real-PSD claim is made. CPU stroke buffers scale with touched pixels, not full document size. At 2500 px a float coverage/scratch footprint is roughly 25 MB each before tile slack, with additional buffers for dual/color/depth and touched layer tiles. Snapshots share immutable bytes and copy only modified tiles. An integrated GPU must additionally budget shared texture memory; the measured discrete-GPU RSS does not include VRAM and is not an 8 GB memory-cap simulation.

## 5. Tests and practical limits

Pixel oracles compare the new live pass with the pre-change restore/two-pass algorithm at 8/16/32 bits, including soft, hard, pressure, sampled/textured-dual, Wet Edges and erasing. Existing engine commit/replay/single-undo tests cover selection, masks, pencil/auto-erase, blend modes and symmetry. A new executor matrix compares all pixel bits at one and four threads, including sampled tips, color dynamics and mirrored coverage.

Actual UI regressions cover worker drag/commit/undo, Escape during drawing and pending release, stale-document rejection, a fast following pen tap with pressure/tilt/rotation, changes while drawing, release-time setting capture and replay, and queued stationary airbrush sampling. A ready-commit engine test covers changing the active layer to a nonpaintable group.

The measured UI-frame targets are distinct from full-resolution ink publication. The 2500 px renderer can still take longer than a 33 ms frame to publish a batch, particularly under contention or at three compute threads. The report retains batch time, oldest-input publication latency and release completion time; it does not claim 30 FPS ink updates. Further improvement there would require a faster preview representation or GPU coverage/composition, with additional cross-format equivalence work. Large imported bitmap tips and complicated multi-layer CPU-canvas fallbacks are not benchmarked here.

Four-core affinity is a real CPU restriction on four distinct physical cores (0–3; SMT siblings 8–11 excluded), deriving a three-thread pool. It still uses the RTX 5090 and the host's RAM; it is not an integrated-GPU or 8 GB hardware measurement. On an integrated GPU, the bounded snapshots and dirty uploads avoid full-frame restore buffers, but GPU textures share RAM and upload bandwidth can dominate. No integrated-GPU frame-rate claim is made. The CPU-only canvas can still spend time compositing its dirty display rectangle on the UI; that existing display fallback is separate from the accelerated stroke computation.

Read `REVIEW-threading-policy-fable.md` and the actual `lc-sysmem` profile/priority APIs. No private production pool, blocking rayon task or concurrency override was added. One cargo lane used the supplied four build jobs, pinned 1.98.1 environment and offline mode throughout. No owner application was killed, no GUI window was opened and no ComfyUI job was submitted. Logs and the retained cargo cache are under `target/bigbrush-work/`.

## 6. Completed validation and retained evidence

| Changed package | Passed | Failed | Intentionally ignored |
|---|---:|---:|---:|
| photocraft-paint | 205 | 0 | 1 |
| photocraft-engine | 2101 | 0 | 33 |
| photocraft-ui-egui | 1673 | 0 | 67 |
| **Total** | **3979** | **0** | **101** |

- Full suites of only changed packages; paint's final source passed in `full-tests.log`, final engine/UI production source in `full-engine-ui-delivery.log`. Engine/UI were repeated only after the new deferred-input compatibility changes. No workspace suite was run. Documentation tests passed.
- `clippy-delivery.log`: all three changed packages, all targets, `-D warnings`, passed. Final format and changed-code/report whitespace checks passed. The pre-existing task-file trailing blank line was left intact.
- New focused regressions: seven actual-canvas worker tests, two paint oracle/cancellation tests, executor pixel equality at one/four threads, and original-target prepared-commit enablement. Existing commit/replay matrix gained textured-dual coverage.
- Extended GPU harness explicitly passed for six cases at 24 MP, four-core affinity and 60 MP: `performance-delivery.log`, `performance-four-core-delivery.log`, `performance-60mp-delivery.log`, with corresponding JSON. It asserts the real GPU and checks ink, not merely a nonempty screenshot. No new GPU kernel was added. Existing UI GPU integration target also passed (89 tests, 16 deliberate ignores).
- `baseline.log` and `performance-before.json` retain the before measurement. Intermediate measurements, failed assertions and fixes remain in the log directory; none were counted as successful final results. Initial surface-structure comparison was replaced by equality of every pixel over both complete tile bounds because sparse empty-tile allocation is not pixel content. Airbrush failures exposed and fixed input/timestamp bugs; checks were not weakened.
- The GPU suite regenerated `docs/design/vector-tool-icons.png`; its diagnostic image was copied to the log directory and the tracked image restored. No unrelated tracked source/artifact changes are retained.

Remaining work is the full-resolution paint-publication latency described above, integrated-GPU/CPU-display-fallback measurements, and large-tip/complex-layer workloads. The requested UI-frame targets and specified correctness/undo/cancel tests are complete; the evidence does not support claiming immediate 2500 px ink under every workload.
