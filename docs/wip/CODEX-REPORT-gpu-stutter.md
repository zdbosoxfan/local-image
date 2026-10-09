# GPU compositor and stutter report

Continued the existing `261bd7a` WIP rather than replacing it. No commits, new dependencies,
unsafe code, GUI windows, or edits to the owner's files outside the worktree. The pre-existing change to
`CODEX-TASK.md` was left alone. CPU compositor algorithms and document/settings serialization
are unchanged.

## Work completed

1. **Recoverable validation errors.** The WIP strike policy now survives asynchronous errors
   arriving during paint, after a refresh returned: the next health check invalidates canvas
   caches and schedules a CPU retry. Subsequent refreshes can composite on the GPU again.
   Device loss remains final; errors in three distinct frames disable the canvas. Multiple
   errors or health checks in one app frame do not add extra strikes. Fixed the clean-frame
   decay boundary (600 clean frames). Invalid cached GPU resources are discarded and messages
   retain the driver's reason. The performance inspector clears a prior fallback reason after
   successful GPU compositing.
2. **Flip Horizontal.** Finished and retained the WIP shader/view-transform implementation;
   flipped views use GPU canvas tiles, including culling, document shadow and grid. The image
   test now clicks View > Flip Horizontal and checks the mirrored result and active GPU path.
3. **Stutter reductions.**
   - Added an owned GPU pass-template cache. It derives an exact structure key from the inputs
     used by the planner (including content bounds, opacity, blend, clipping, masks, geometry,
     effects, adjustments, global light, profile, patterns, dimensions and depth). Ordinary
     pixel edits inside unchanged bounds rebind current surfaces without rebuilding the plan
     or `check_fx`; changed bounds or derived masks/shapes invalidate it, including feathered
     pixel masks edited inside unchanged bounds. Undo also invalidates
     correctly. Refusals are cached. No persisted revision field is required. Layer paths and
     the live layer-ID set are cached alongside the template, avoiding a per-refresh `walk()`
     allocation and rebuilding the ID HashSet. LUT records use shared Arcs.
   - Reused compositor uniform buffers/groups, texture groups and LUT textures. Each cell has
     a distinct uniform allocation within a submission, avoiding the queue-write overwrite
     hazard. Public `encode()` calls retain independent uniforms until their caller submits:
     an intervening managed render cannot overwrite them or grow an append-only pool.
     Resource caches are bounded/cleared on eviction, closure, limit changes or errors.
   - Pooled canvas encode uniforms/groups; cached mip groups when canvas tiles are created.
     Mips now append to the final compositor encoder, removing the extra GPU-path submission.
   - Released the renderer write lock before recording/submitting the compositor work. Local
     GPU handles keep the canvas resources alive; the lock is reacquired only to store results.
   - Ordinary composites do not wait for GPU completion. Blocking waits remain for explicit
     readbacks, `PHOTOCRAFT_GPU_SYNC` benchmarks, and throttling enormous multi-submit refreshes
     to bound staging/evicted memory. Removing those bounded backpressure waits would trade
     stutter for unbounded GPU memory and reset risk.
   - Added process-local, lazy surface revisions covering all mutation entry points. Clones
     preserve the revision until mutation; revisions do not affect pixel equality or files.
     Thumbnail fingerprints are now constant time and include default-pixel/format changes;
     layer and mask thumbnail size keys include document height as well as width. Per-pixel
     writes mark dirty without incrementing a global atomic for every sample.
   - Large Navigator thumbnails, including the first image, render on a worker. While dragging,
     the panel keeps the previous image; workers receive committed documents, so they do not
     pin the mutable live preview and trigger document clones. Stale worker results are dropped.
   - Retained the WIP incremental symmetry coverage merge: only dirty/tail tiles are merged,
     without cloning both entire renderers on each push. The mutable live document stays at the
     same allocation during ordinary pointer moves; a one-time initial COW snapshot and copies
     required by an explicitly retained preview still preserve snapshot semantics.
   - Mouse-up uses the live rendered target through the normal journal/history command only
     when document, revision, brush, selection/target params, points, seed, zoom and symmetry
     match. Otherwise it safely replays. Extended the WIP long-stroke equality test to 8/16/32
     bits, 13 brush/target/symmetry settings each, checking pixels, damage, history and undo/redo.
   - Live damage history is bounded to 128 rectangles. Older views catch up using cumulative
     damage bounds (including restored smoothing tails); recent views still get incremental
     damage. A 400-move drag test checks the bound,
     preview equality and commit reuse.
4. **Fallback features.** Fixed the common non-feature refusal “layers exceed the GPU memory
   budget; full refresh on the CPU”: full refreshes now use the compositor's existing paging,
   LRU eviction, focus ordering and bounded submits instead of synchronously compositing on
   the CPU. An over-budget, tiled-canvas GPU parity test covers this path.

## Remaining fallback reasons

These still intentionally use the CPU reference, and are visible as `Refresh.fallback` / the
performance inspector's `gpu_fallback`:

- Multichannel ink documents.
- Active Blend If ranges on a layer.
- Levels/Curves explicitly evaluated in native CMYK or Lab channels.
- Noise-bearing shadow/glow effects (the CPU's speckle maps must be matched exactly).
- Effects whose per-page apron exceeds the actual texture limit, or blur LUTs exceed 4096 taps.
- Patterns larger than the device's texture limit.
- Layer effects on adapters lacking float effect-map render targets; failed compositor pipeline
  creation; empty documents or missing canvas resources.
- Explicit `PHOTOCRAFT_CPU_COMPOSE`, preview texture keys other than the document ID, recoverable
  GPU errors, and permanent device loss/repeated errors.

Blend If, native channel adjustments and noise effects need matching shader implementations
and real-device parity validation; broadening them speculatively would violate correctness.
Normal RGB documents with the realistic photo/adjustments/masks/type-shadow/shape/group fixture
are covered by the GPU canvas test. Large document/layer dimensions alone are already supported.

## Validation and measurements

All builds used Rust 1.98.1, `--offline`, `CARGO_BUILD_JOBS=3`, and one Cargo build at
a time. No release rebuild or hardware timing was needed for the CPU measurements.

The required unfiltered test command was attempted. One unrelated engine test,
`ai_cmds::tests::ai_commands_against_the_mock_server`, failed at `ai_cmds.rs:702` because
this sandbox denies `MockComfy` binding a local socket: `PermissionDenied`, OS error 1,
“Operation not permitted.” That test was left unchanged. The full run with just that
test excluded passed with `RUST_TEST_THREADS=1`:

```sh
PATH="$HOME/.cargo/bin:$PATH" CARGO_BUILD_JOBS=3 RUST_TEST_THREADS=1 \
  cargo +1.98.1 test --offline -p photocraft-gpu -p photocraft-ui-egui \
  -p photocraft-engine -p photocraft-compose -p photocraft-raster -p photocraft-paint \
  -- --skip ai_cmds::tests::ai_commands_against_the_mock_server
```

Counts include unit, integration and doc tests; ignored benchmarks remain ignored here:

| Crate | Passed | Ignored | Filtered |
| --- | ---: | ---: | ---: |
| photocraft-compose | 134 | 0 | 0 |
| photocraft-engine | 844 | 15 | 1 |
| photocraft-gpu | 54 | 0 | 0 |
| photocraft-paint | 74 | 1 | 0 |
| photocraft-raster | 17 | 0 | 0 |
| photocraft-ui-egui | 924 | 6 | 0 |

The engine unit count is 806 plus 38 integration tests. GPU coverage comprises 21 unit
tests, 3 device-loss tests, 29 parity tests and 1 vector-mask test. UI coverage comprises
858 unit tests and 66 integration tests. The combined run passed 2,046 tests. The final
GPU-only run after the deferred-encoder safety change adds one parity test, giving 2,047
passed across the latest suites, with 22 ignored and one socket test excluded. The final
preview invalidation, cumulative damage and inherited paint refactor passed in the full run.
The GPU-only verification used `cargo +1.98.1 test --offline -p photocraft-gpu` with the
same environment above.

CPU measurements on AMD Ryzen 7 9800X3D, the workspace's optimized test profile, single
test thread, same deterministic data and process for before/after:

| CPU operation | Before (ms) | After (ms) | Samples |
| --- | ---: | ---: | ---: |
| Plan realistic 1800×1200 document, warm repeated refresh | 0.908302 | 0.013980 | 100; median |
| Fingerprint all raster/cache surfaces in that document | 0.000450 | 0.000020 | 1000; median |
| Symmetry pointer push, growing 240-point stroke | 1.422 | 0.083 | 240; median |
| Mouse-up, 400-point stroke, replay vs live-result reuse | 11.989 | 0.095 | 1 paired commit |

Symmetry maxima were 8.499 ms before and 0.213 ms after. Four-point live pushes took a
0.313937 ms median (100 samples), with the live document allocation unchanged after every
push. Both ignored CPU benchmarks were explicitly executed and passed exact surface/layer
equality checks (2 passed). These final measurements used the Cargo-built test binary after
builds and lint had finished. Timings vary with host load. The fingerprint saving
on this small fixture is under one microsecond; its benefit scales with tile count.
The mouse-up pair is an illustrative deterministic measurement, not a latency percentile.
Plan measurements exclude `check_fx`; production also avoids repeating that check.

Reproduce the CPU comparisons without a GPU:

```sh
PATH="$HOME/.cargo/bin:$PATH" CARGO_BUILD_JOBS=3 RUST_TEST_THREADS=1 \
  cargo +1.98.1 test --offline -p photocraft-ui-egui --test gpu_canvas_perf \
  bench_cpu_ -- --ignored --nocapture
```

The before paths reconstruct the original repeated planner, tile-pointer fingerprint,
clone/finish/full-coverage symmetry merge and normal stroke replay. The after paths call
the production cache, revision fingerprint, incremental merge and guarded live commit.
The benchmark reselects the paint layer after undo, since undo restores layer selection.

Per-crate formatting passes. Clippy passes for GPU, UI, engine, raster and paint with
`CARGO_TARGET_DIR=target/clippy`, `--all-targets -- -D warnings`. Two pre-existing engine
`chunks_exact(4)` uses (sky guide conversion and a seamless test fixture) required the
equivalent `as_chunks::<4>().0.iter()` form for Rust 1.98's new lint. The build script still
prints the existing notice about unavailable optional CJK embedded fonts.

```sh
PATH="$HOME/.cargo/bin:$PATH" cargo +1.98.1 fmt \
  -p photocraft-gpu -p photocraft-ui-egui -p photocraft-engine \
  -p photocraft-raster -p photocraft-paint --check
PATH="$HOME/.cargo/bin:$PATH" CARGO_BUILD_JOBS=3 CARGO_TARGET_DIR=target/clippy \
  cargo +1.98.1 clippy --offline -p photocraft-gpu -p photocraft-ui-egui \
  -p photocraft-engine -p photocraft-raster -p photocraft-paint --all-targets -- -D warnings
```

Remaining practical limits: brush rasterization still uses the CPU on the UI thread;
unsupported documents still need synchronous CPU compositing; enormous refreshes retain
bounded backpressure waits. Native large Navigator images are asynchronous; small images
and wasm keep their synchronous path. No serialized model or CPU compositor algorithm was
changed to obtain these improvements.

## Coordinator GPU verification

The sandbox has no hardware adapter (`/dev/dri` is absent), but contrary to the task's expected
skip-only environment it exposes Mesa llvmpipe Vulkan: LLVM 22.1.8, Mesa 26.2.3, CPU device.
The realistic canvas test printed that exact adapter/backend and passed its GPU-path and pixel
assertions. The compositor parity tests executed on that software adapter. Hardware performance and driver
validation still require RTX 5090/Vulkan (and AMD iGPU when practical). Use one test thread:
an initial default-thread run terminated with SIGSEGV in `adjust_preview_gpu`; serial runs passed.
This fits the existing tests' warning about concurrent wgpu instances on some drivers, although
the exact native crash cause was not traced. Tests that actually report no adapter still return
without their GPU assertions.
Run on hardware:

```sh
export PATH="$HOME/.cargo/bin:$PATH" CARGO_BUILD_JOBS=3 RUST_TEST_THREADS=1
cargo +1.98.1 test --offline -p photocraft-gpu -p photocraft-ui-egui -p photocraft-engine -p photocraft-compose -p photocraft-raster -p photocraft-paint
cargo +1.98.1 test --offline -p photocraft-gpu -- --nocapture
cargo +1.98.1 test --offline -p photocraft-ui-egui --test gpu_device_loss -- --nocapture
cargo +1.98.1 test --offline -p photocraft-ui-egui --test canvas_flip_gpu -- --nocapture
cargo +1.98.1 test --offline -p photocraft-ui-egui --test gpu_canvas_perf -- --nocapture
cargo +1.98.1 test --offline -p photocraft-ui-egui --test canvas_16f --test color_managed_canvas --test live_stroke_canvas --test adjust_preview_gpu --test drag_preview_canvas -- --nocapture
cargo +1.98.1 test --offline --release -p photocraft-ui-egui --test gpu_canvas_perf bench_gpu_canvas_24mp -- --ignored --nocapture
```

New/continued GPU coverage: single-validation CPU retry and recovery, three error frames,
injected and actual device loss, menu-click horizontal flip, realistic document GPU/CPU parity,
and over-budget full refresh with 256-pixel canvas tiles. New compositor parity tests are
`cached_plans_rebind_pixel_edits_and_survive_document_switches`,
`cached_feathered_mask_updates_match_cpu`, and
`deferred_encode_uniforms_survive_an_intervening_render`. The feathered-mask test reproduced
the stale alpha before invalidation was fixed (about 32/255 error), then passed with the fix.
Existing compositor parity cases that
particularly matter for the new caches are incremental edits, masks/clipping/groups, adjustments,
all effect kinds and incremental effect updates, shapes with clipping/vector strokes, vector
masks, artboards, patterns, formats/offsets/chunks, texture-limit paging, eviction and focus,
text gamma, first frame, and RGBA16F fallback. Existing UI canvas tests verify high precision,
colour management, live stroke release, and adjustment/move preview updates.

`bench_gpu_canvas_24mp` reports cold/warm full composites and damage refreshes on 6000x4000,
CPU-side enqueue time separately from GPU completion time, along with upload/plan, stroke,
thumbnail and Navigator measurements. Actual GPU speedup and absence of visual/validation faults
remain to be confirmed on hardware; no GPU timings are claimed from this sandbox.
