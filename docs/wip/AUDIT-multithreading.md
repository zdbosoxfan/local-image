# AUDIT: multithreading and parallelism (static, read-only)

Worktree: .claude/worktrees/integrate (claude/v2-continue). Written incrementally.

## Method and scope

Static read only (no builds, tests or benchmarks). Source: `.claude/worktrees/integrate`. Paths below are relative to that worktree. Overlap with `docs/wip/checkpoint-2026-10-12/PERFORMANCE-EXECUTION-MATRIX.md` ("MATRIX") is marked **[OVERLAP]**. The MATRIX is almost entirely about GPU/vector interactive latency, Navigator, Save As worker and the Rescue CPU lane; it contains no CPU-threading plan, so nearly everything here is new. Priorities: P0 = user-visible slow path, P1, P2.

## A. Thread-pool picture (read this first)

Mechanisms in use today (none is coordinated with another):

| Pool / mechanism | Size | Where |
|---|---|---|
| rayon GLOBAL pool (never configured: no `build_global` anywhere) | logical CPUs (SMT included) | pc-algo, pc-compose, pc-vector, pc-cms, pc-engine, lc-raw (17 files), lc-merge, lc-engine, li-seg denoise |
| tract executor (`tract-linalg` `multithread-mm` off, so `Executor::SingleThread`) | 1 | all li-seg ONNX models |
| candle-core CPU (SAM 3, `lc-segment`) uses rayon internally (`gemm` + rayon conv2d/im2col) | rayon global | `crates/lc-segment` |
| `lightcraft_preview::JobPool` (`lc-job-N`) | cores-1 clamp 2..8 (Renderer clamps 1..6) | `crates/lc-preview/src/pool.rs:100`, `crates/lc-ui-egui/src/render.rs:162` |
| scoped OS threads, `available_parallelism()` each | up to 16/32 per call | `pc-compose/src/effects.rs:840`, `pc-codecs/src/codecs/png.rs:365`, `pc-io/src/pixels.rs:187`, `pc-format/src/store.rs:489`, `pc-ui-egui/src/gpu_canvas.rs:1187`, `lc-ui-egui/src/softpaint.rs:204` |
| private rayon pools | cores/2 | `lc-engine/src/smart_sort/mod.rs:228,285`, `smart_sort/people.rs:748`, `lc-ui-egui/src/people_task.rs:105` |
| ad-hoc `std::thread::spawn` job threads | 1 each | export, import, smart-sort, previews build, batch-cutout, restoration, enhance, merge ... |
| filmstrip workers | 1..3 | `pc-ui-egui/src/filmstrip_ui.rs:64` |
| import probe | min(cores,8) | `lc-engine/src/import.rs:405` |

Observations:
1. No process-wide policy. A rayon-heavy job (RAW demosaic, filter tiles) plus the JobPool (cores-1 workers, each running a RAW render that itself uses the global rayon pool) plus an export thread all fan out at once. This is not catastrophic (rayon work-steals, OS time-slices), but it makes UI-thread frame time and the GPU feeder thread unpredictable under load. There is no reserved core for the UI thread.
2. If the tract fix uses `Executor::multithread(n)` it builds a SECOND rayon pool ("tract-default"). `li-seg::denoise::run_tiled` already does a rayon `par_iter` over tiles, so each global-pool worker would call into a private tract pool: N x N runnable threads. **Use `tract_linalg::multithread::Executor::RayonGlobal`** (exists in 0.23.8, `crates/.../tract-linalg-0.23.8/src/multithread.rs:31`) so tract shares the one global rayon pool and nested parallelism is work-stolen, not oversubscribed. `set_default_executor` is process-global (a `static Mutex`), so one call at startup reaches every tract model in li-seg (also faces, CLIP, enhance, restoration). `multithread-mm` pulls in `rayon`, which is already in Cargo.lock (only a dependency edge is added to tract-linalg, no new package).
3. Recommended single policy (P0, tiny): in `apps/local-image/src/main.rs` before any worker starts, call `rayon::ThreadPoolBuilder::new().num_threads(compute).thread_name(|i| format!("li-compute-{i}")).build_global()` with `compute = max(2, physical_cores - 1)` (physical cores: `available_parallelism()/2` when SMT is on is a heuristic; `lc-sysmem` or `/sys/devices/system/cpu/cpu0/topology/thread_siblings_list` can give it without new deps), then `tract_linalg::multithread::set_default_executor(Executor::RayonGlobal)`. Keep JobPool at its existing cores-1 cap but make "heavy" jobs (RAW render, export, AI) take a shared `compute_permit` (a counting semaphore of size ~2) so that at most two heavy jobs fan out into rayon at once. Run egui on its own thread (already so) and never `rayon::scope`/`par_iter` from the UI thread (see section E). Replace the six ad-hoc `std::thread::scope(available_parallelism())` helpers with rayon (or with `rayon::current_num_threads()`) unless they deliberately avoid rayon; `effects.rs:rows_par` documents a real reason (rayon workers waiting inside a tile steal other tiles), so leave that one but cap it at `current_num_threads()`.

## B. tract / ONNX / candle (task 1)

All tract use is inside `crates/li-seg` (`crates/li-seg/Cargo.toml:12` `tract-onnx = "0.23"`). `lc-segment` (SAM 3) uses candle, not tract. `li-ai` has no tract code; it calls li-seg (CPU upscale) or ComfyUI (GPU). `restoration_graph.rs` is a GPU-graph decoder (no inference on the CPU).

Will the global executor fix reach every model? **Yes**, if it is `tract_linalg::multithread::set_default_executor` (process-wide `static`, `tract-linalg-0.23.8/src/multithread.rs:60`). All sites below build a plain `into_runnable()` plan with no per-plan `executor`, so they consult the default. A direct `tract-linalg = { version = "=0.23.8", features = ["multithread-mm"] }` dependency is required because `tract-onnx`/`tract-core` 0.23.8 have no feature pass-through (checked their Cargo.toml). The tract kernels are partitioned over output panels/rows (not over the K dimension), so results should stay bit-identical to the single-thread run; keep one `tiny_onnx_model_runs_through_tract` style equality test. Small models are gated by tract's own thresholds (`THREADING_PANEL_THRESHOLD`=64, `THREADING_ELEMENT_THRESHOLD`=32768), so YuNet/SFace/CLIP-text gain little; that is fine.

| # | Site | Model / shape | Threading today | Verdict |
|---|---|---|---|---|
| B1 | `li-seg/src/lib.rs:520-525` (Segmenter: IS-Net subject 1024^2, sky, depth) | conv nets | 1 thread | gets the fix; see section E for where it is CALLED (UI thread) |
| B2 | `li-seg/src/enhance.rs:60-86` Upscaler (bsrgan 2x/4x RRDB, realplksr, custom) | 256^2 tiles | 1 thread; tiles strictly sequential (`tiled()` `enhance.rs:174-176`) | fix helps ~2.5-3x; tile level parallelism is the extra lever, see F2 |
| B3 | `li-seg/src/denoise.rs:83-110` (RawNIND bayer/linear 512, NIND RGB 768) | U-Net | CPU fallback = `run_tiled_channels` (`denoise.rs:824`) fully sequential. `run_tiled` (`denoise.rs:193`, row-parallel `par_iter`) is NOT called by the app (only GPU `lc-gpu/src/denoise.rs:149` and CPU `denoise_channels` use `run_tiled_channels`) | F3 |
| B4 | `li-seg/src/faces.rs:235-259` YuNet 1024 + SFace 112 | tiny | per-photo parallel already (rayon pool of cores/2 in `smart_sort/people.rs:748`, `lc-ui-egui/src/people_task.rs:105`) | little gain from intra-op; leave |
| B5 | `li-seg/src/clip.rs:40-54` CLIP visual/text | ViT 224^2, up to 3 crops sequential (`clip.rs:61-73`) | per-photo parallel (cores/2 pool, `smart_sort/mod.rs:228`); crops sequential inside | intra-op helps a little when fewer photos than workers; leave |
| B6 | `li-seg/src/restoration.rs:135-147` DD-Color 512^2, scratch detector up to 2048 | single `plan.run` | 1 thread | gets the fix; surrounding pixel code is serial, F7 |
| B7 | `lc-segment` SAM 3 (candle, `crates/lc-segment`) | ViT-L 1008^2, F32, CPU on Linux (`best_device()` is Metal on macOS else CPU, `lc-segment/src/lib.rs:90-96`) | candle's CPU backend uses rayon (conv2d, im2col, `gemm` `Parallelism::Rayon`) so it IS multithreaded on the rayon global pool; pre/post-processing serial (`mask.rs:32,78`) | not a tract issue. P2: `mask::alpha` (`lc-segment/src/mask.rs:78`) does a full-resolution sigmoid per pixel serially, `preprocess` (`mask.rs:32`) two serial resample passes. Both are small next to the encoder |
| B8 | Develop AI Denoise (`lc-engine/src/enhance/denoise.rs:107-138`) | GPU first (`lc-gpu`), CPU fallback `Denoiser::denoise_channels` | GPU path: `run_tiled_channels` (`lc-gpu/src/denoise.rs:149`) runs the CPU gather and blend serially between GPU tiles | F3 and F10 |

**Interaction with rayon.** Use `Executor::RayonGlobal`, not `Executor::multithread(n)`. `RayonGlobal` dispatches with `into_par_iter` in whichever rayon pool the calling thread belongs to, so (a) the smart-sort private pool (cores/2) keeps tract work inside that pool and (b) a tile level `par_iter` over tiles nests with the intra-op work by work-stealing instead of multiplying thread counts. `Executor::multithread(n)` would create a second pool named `tract-default` and give n*n runnable threads under any tile level `par_iter`.

### F1 (P0) Make the tract executor global and shared
- Where: `crates/li-seg/Cargo.toml`, one init in `crates/li-seg/src/lib.rs` (or the app start-up, see policy in section A).
- Evidence: the 907.34 s CPU seam test (`docs/wip/checkpoint-2026-10-12/RELEASE-ACCEPTANCE-MATRIX.md:55`) is single-thread; owner measured 2.5-3x on conv nets.
- Risk: determinism (low, see above), memory (none), oversubscription (use RayonGlobal).
- Status: a builder is already on it. **[no overlap with MATRIX]**

### F2 (P0 for any CPU upscale) Upscaler tiles: sequential, extreme redundancy
- `crates/li-seg/src/enhance.rs:141-218` `tiled()`: double loop over tiles, each calls `predict` (one `plan.spawn()?.run_plan_with_eval`, `enhance.rs:72`), then a scalar feathered blend `enhance.rs:195-206` and a scalar flush/convert `enhance.rs:209-218`. All on the calling thread.
- Redundancy: for `bsrgan-4x` the tile is 256 with halo 112 (`enhance.rs:52-54`), so core = 32 px, overlap = core/4 = 8, step = 24 px (`enhance.rs:157-158`). That is about 1,700 full 256^2 RRDB inferences PER INPUT MEGAPIXEL (each roughly 0.4-0.5 TFLOP, estimated from the architecture). The 907 s figure covers only a handful of tiles on the 44x16 seam fixture. Even with a 3x tract speedup a 1 MP input needs on the order of hours on CPU; no thread fix makes CPU Rescue practical for photos. (MATRIX marks Rescue "bounded expensive inference ... no reason to reduce halo" **[OVERLAP]**; this audit agrees about the halo and adds that the fixed ONNX shape caps core at 32 px.)
- Proposed change: (a) run tiles through a bounded rayon window: `tiles.chunks(W).par_iter()` where `W = clamp(compute_threads / 3, 1, 4)` and a per-tile memory estimate (RRDB 256^2: roughly 0.3-0.6 GB working set; read `lc-sysmem` for headroom) with the blend and progress callback done in tile order on the calling thread; (b) keep `check()` per node (it already is, `enhance.rs:73`); (c) surface a time estimate and refuse (or warn) before launch when tiles x per-tile cost exceeds a threshold on the CPU backend. Pure Rust, rayon already a dep of li-seg.
- Risk: output must be byte-identical: blend order stays tile-major as today (collect `values` per window, blend in original order); GPU/CPU parity unaffected (CPU only path).
- Priority: P0 only as a guard/estimate (user-visible hang), P1 for tile parallelism.

### F3 (P0 on machines without a usable GPU, P1 otherwise) CPU AI Denoise is one tile at a time
- `crates/li-seg/src/denoise.rs:824-905` `run_tiled_channels`: sequential tile loop with gather (`:861-868`), model call, scalar weighted blend (`:874-893`), final divide (`:896-903`). Closure is `impl Fn` without `Sync`, and the caller `lc-engine/src/enhance/denoise.rs:138`.
- Tile counts (computed from the tiler): Bayer/linear tile 512 with halo 64 and overlap 128 (`denoise.rs:461-463, 841-842`). A 24 MP Bayer RAW is packed to 3000x2000 plus halo = about 8 x 6 = 48 tiles; RGB tile 768 gives about 30 tiles. Per-tile cost not measured in any report; a 512^2 U-Net on one tract thread is plausibly tens of seconds (estimate), so minutes to tens of minutes for one photo.
- Proposed: make `model: impl Fn(&[f32]) -> Result<Vec<f32>> + Sync`, process tiles with a bounded rayon window (2-3 tiles for a 512 U-Net; each holds the 64-256 MB feature maps plus skip connections), accumulate in tile order. On the GPU path (`lc-gpu/src/denoise.rs:149`) the GPU already serialises tiles; pipeline CPU gather of tile k+1 and blend of tile k-1 on rayon while the GPU runs tile k (F10).
- Risk: float accumulation order must stay tile-major so the output is bit-identical; memory bounded by window size; GPU buffers are resident and serialised by design (`CODEX-REPORT-ai-denoise.md`).

### F7 (P1) Restoration pre/post pixel code is serial
- `crates/li-seg/src/restoration.rs:195-215` `colour_input` builds three `Vec<f32>` by iterating the source image per channel, then `bilinear` (`:181`); `colour_output` (`:203`) does `linear()`/`srgb()` (powf) and `lab_f` (cbrt) per pixel: about 7 transcendental calls per pixel, on the full-size image, serially. At 24 MP that is on the order of 3-5 s (estimate). `scratch_input` (`:236`) and `scratch_output` (`:247`) are also serial full-resolution loops.
- Proposed: `rayon::par_chunks_mut` over output rows for `colour_output` / `scratch_output`; compute the 512^2 greyscale input via a row-parallel pass. Results are per-pixel pure so bit-identical.
- Risk: none (pure functions). Priority P1.

### F8 (P1) li-ai enhance finishing step: two serial Lanczos3 resizes
- `crates/li-ai/src/enhance.rs:231-232`: `imageops::resize(&enlarged, ..)` and `imageops::resize(input, w, h, Lanczos3)` from the `image` crate, both single-threaded, then a serial `from_fn` blend (`:233`). The second resize (`source`) exists only to blend premultiplied alpha at `a = (alpha/255)^4`; for an opaque input (almost all photos) `a == 1` everywhere and `source` is never used. The first resize is a no-op copy when dimensions already match, but the target can differ from `scale * input` (`native_scale` accepts a range), then it is a full serial Lanczos3 at output size (up to 400 MP, `enhance.rs:150`).
- Proposed: skip the `source` resize when the input has no alpha < 255 (scan once); use `lightcraft_raster::resample` (rayon, Lanczos3, already in the workspace) or a row-band parallel wrapper for the real resize; row-parallelise the blend with `par_chunks_mut`.
- Evidence: Lanczos3 serial at roughly 30-60 ns per output pixel per pass => 100 MP output about 6-12 s serial (estimate). Risk: Lanczos3 weights must match `image`'s to stay byte-identical; if not identical use it only for the new fast path and add a golden test. P1.

## C. rayon coverage vs heavy loops (task 2)

Status by area (read the code, not just grep; many crates call helper wrappers: `par_rows`/`par_map` in `pc-algo/src/photo_util.rs:128-152`, `lc-raster::par_rows` `lc-raster/src/lib.rs:18`, `Image::map*`):

| Area | State |
|---|---|
| pc-algo filters (tile engine `pc-algo/src/lib.rs:1012-1050`, box blur strips `lib.rs:1088-1100`, liquify, warp, resample, transform, noise, PatchMatch bands `inpaint.rs:277,367,424`, Poisson `poisson.rs:195`, panorama, HDR, quantize, seam) | Already parallel. No finding. |
| pc-algo `stack.rs:77` `combine` | Serial, see F5 |
| pc-algo `denoise.rs`, `sharpen.rs`, `relight.rs`, `retouch.rs` | go through the parallel tile engine (kernel per tile), no finding |
| pc-compose compositor (`lib.rs:140-190`) | Parallel by band/tile. `effects.rs:rows_par` (`:838`) deliberately uses scoped OS threads (up to 16 per call) because the maps can be built from inside a rayon tile; oversubscription note in section A. |
| pc-compose `adjust.rs` (`map_rgb` `:458`) | Serial per buffer but called per tile from the parallel compositor (fine). BUT the destructive Image > Adjustments path calls it on the WHOLE layer: F4 |
| pc-paint (brush rasterize/accumulate `render.rs:164,308`) | No rayon dependency at all. Stamps are order-dependent, dabs small; a GPU damage path exists. P2: `Brush::rasterize` per big dab could be row-banded. **[OVERLAP: MATRIX "Compositing filters, blends, adjustments, brushes"]** |
| pc-raster | only a Surface container; resampling lives in pc-algo (parallel) |
| pc-codecs: PNG encode banded on scoped threads (`png.rs:365-380`), TIFF tiles rayon (`tiff.rs:740`), JPEG encode `jpeg_encoder` single thread (`jpeg.rs:218-275`), JPEG decode zune single thread, WebP lossy own VP8 encoder serial (`vp8/mod.rs`) | JPEG/WebP-lossy single-image encode serial: 24 MP JPEG encode about 0.4-0.8 s (estimate). Fix is batch-level overlap (F9), not intra-image. P2 |
| pc-io / pc-format | `pc-io/src/pixels.rs:174-187` conversions parallel, PSD channel compress parallel (`psd_export.rs:285,1096`), store compress parallel (`pc-format/src/store.rs:489`). Fine. |
| lc-raw demosaic and recovery | Parallel already (`CODEX-REPORT-raw-speed.md`) **[OVERLAP]**. Decoders: ARW/RW2/ORF/RAF/TIFF tiles parallel; CR2 (`vendor/cr2.rs:124-126`), NEF (`vendor/nef.rs`, `nefc.rs`), SR2 (`sr2.rs`) and DNG lossless-JPEG per strip are single stream Huffman decodes (inherent: no restart markers) => about 0.3-0.6 s per 24 MP serial (estimate). P2, leave. |
| lc-pipeline CPU path | Mostly via `Image::map*` (parallel). Serial pockets in per-pixel loops: `primary.rs:405,412` (mix3 over full image), `:439` (exposure/shadows application with `exp2`), `:499-505` (clarity: `rgb_to_hsb` + `exp2` per pixel), `:555-575` (skin/hue selection), `tone2.rs`, `finish.rs`, `masks.rs` (13 loops), `detail/nr.rs`, `detail/haze.rs`. The Develop CPU path is the GPU-fallback and the path used when `lightcraft_pipeline::lut::is_lut_profile` forces CPU (`lc-engine/src/media.rs:655`). P2 unless the owner often runs without a working GPU (then P1). **[OVERLAP: CODEX-REPORT-develop-speed.md covers slider path, not these loops]** |
| lc-segment (candle) | see B7 |
| thumbnail/preview generation | Library thumbnails run on `JobPool` (cores-1, max 6, `lc-ui-egui/src/render.rs:158`): parallel. "Build previews" and Smart Previews are serial: F6 |
| Library import | Probing parallel (min(cores, 8), `lc-engine/src/import.rs:405`). Copy-verify serial: F9b |
| Smart Sort / People | Parallel per photo, cores/2 private pools (`smart_sort/mod.rs:228,285`, `people.rs:748`, `people_task.rs:105`). Batches of 32 with a barrier (`mod.rs:229-264`): tail idle, P2. Tagging and faces each use a cores/2 pool and each decodes the same photo; a combined run uses all cores and decodes twice (P2). |

### F4 (P1) Destructive adjustments run on the whole layer serially
- `crates/pc-engine/src/pixels.rs:46-71` `adjusted()`: `s.read_region(r)` of the full content bounds, `chunks_exact(n).map(to_rgba)` collect, `buf.clone()` (`orig`), `adjust::apply_depth` (serial `map_rgb`, `adjust.rs:458`), then a serial per-pixel mix with `sel.sample_channel(...)` and `from_rgba_into`. Same wrapper in `crates/pc-engine/src/adjust_cmds.rs:147-168` `process_surface` (used by Shadows/Highlights, HDR toning, Match Color, and every `rgba_edit`). Also `adjust_as_filter` (`adjust_cmds.rs:211-218`) for smart-filter adjustments.
- Cost: for 24 MP: 384 MB `Vec<[f32;4]>` plus a clone (another 384 MB) and three serial passes; with `powf` per channel for Exposure, hue/saturation and curves it is roughly 1-4 s per apply (estimate: 24M px x 40-150 ns). Whether this runs on the UI thread depends on the command (adjustments are not `jobs::run` commands in `adjust_cmds.rs`, filters in `filters.rs:232` are); treat as UI-blocking until verified.
- Proposed: `rayon::par_chunks_mut` over rows for the three passes (convert, apply per row or chunk, mix+encode). Needs `apply_depth` to be row-chunkable: all `map_rgb`-based variants are; ones that need neighbours do not exist in this function. Keep `orig` by recomputing from `raw` instead of cloning (saves 384 MB).
- Risk: determinism OK (per-pixel pure); selection sampling is read-only. P1.
- **[no overlap]** (MATRIX has no CPU adjustment plan).

### F5 (P1) Layer stack statistics are serial and allocation heavy
- `crates/pc-algo/src/stack.rs:77-102` `combine()`: per pixel pushes samples into three `Vec`s and calls `stat()`; `Stat::Entropy` allocates a `HashMap` per channel per pixel (`stack.rs:60-66`). Used by `pc-engine/src/smart_cmds.rs:287-331` (Smart Object stack modes).
- Cost: 24 MP x 3 channels x one `HashMap` build is likely 10-30 s (estimate); median stack similar order of magnitude with N layers.
- Proposed: `par_chunks_mut(out)` with thread-local scratch `Vec`s; replace the entropy `HashMap` with a 256-entry `[u16;256]` histogram. Output identical.
- Risk: none. Priority P1 (niche feature, huge win, tiny change).

## D. Serialised job queues (task 3)

### F6 (P0) "Build Previews" and Smart Previews run one photo at a time
- `crates/lc-engine/src/cmd/previews.rs:65-77` `run_all`: `for photo_jobs in jobs { ... photo_jobs.into_iter().all(|j| j.run().rendered.is_ok()) }` on one `lc-build-previews` thread (spawned `previews.rs:226`). Each photo has two jobs (largest grid thumb + loupe render at `edge`, `jobs_for` `:56-62`), each a full decode + develop + encode. Smart previews: `smart_run` (`previews.rs:132-205`) is a `for` loop doing `load_source` + `encode_source` + `write_atomic` (fsync) per photo, one thread.
- Cost: per 24 MP RAW roughly 1-3 s (estimate: decode with rayon bursts, then serial encode/write, plus GPU or CPU develop). Rayon inside the decode keeps cores busy only part of the time; encode/IO/fsync phases leave cores idle. 1000 photos = tens of minutes serial.
- Proposed: bounded worker group of `W = clamp(compute_threads/3, 2, 4)` photos in flight (shared atomic cursor, same pattern as `import.rs:405-431`), results counted with the existing atomics (`state.done/failed`), cancellation unchanged. Memory gate: each in-flight 24 MP source is about 288 MB f32 RGB plus outputs, so cap by `lc-sysmem` headroom. For Smart Previews the file write is the long pole: 2-3 workers.
- GPU: loupe renders use `lightcraft_gpu::render` when enabled (`media.rs:653`); the GPU device is serialised internally, so parallel photos overlap CPU decode/encode with GPU work; do not exceed the GPU memory pool.
- Risk: determinism (per-photo independent, unchanged); thread oversubscription with the Library `JobPool` that is rendering thumbnails for the visible grid at the same time: take a shared "heavy job" permit (section A) so the build yields to visible-grid renders (give the JobPool higher priority, e.g. the build uses at most `JobPool.threads - 1`).
- Priority P0 (core Library workflow, simple change). **[no overlap]**

### F9 (P0) Export batch is strictly sequential render -> encode -> write
- `crates/lc-engine/src/export.rs:1476-1558` `run_batch_with_ctl`: `for item in items { item.run_with_ctl(ctl) (render + JPEG/PNG/TIFF encode, export.rs:1259-1286) ; naming/conflict; write }`; the worker is one `export` thread (`lc-ui-egui/src/export_task.rs:79`).
- Cost: per 24 MP photo: RAW decode+develop (GPU or CPU) then single-threaded JPEG encode (about 0.4-0.8 s, estimate) then write. The GPU/cores idle during encode and the encoder idles during render. 200 photos at ~2 s = ~7 min serial; a 2-deep pipeline should approach max(render, encode) rather than the sum.
- Proposed: keep naming, conflict resolution, `guard.check` and `write` strictly serial and in order (they mutate `taken` and the output list); run `item.run_with_ctl` for items i+1..i+D concurrently on a small pool (`D = clamp(compute_threads/4, 2, 3)`; `PreparedExport` is already `Send` because it is moved to the export thread) and consume results in order via a bounded channel. AI upscale exports (`export.rs:1267`) must stay D=1 (CPU tract or Comfy GPU). Memory: D x (source f32 + RGBA output) so cap by `lc-sysmem`.
- Risk: output bytes per photo unchanged, file order unchanged; progress callback still monotonic. Cancel semantics: stop submitting new items, drain in-flight.
- Priority P0. **[no overlap]** (MATRIX "Export resampling" is about GPU resize, not batch overlap).

### F9b (P1) Import copy+verify is serial after a parallel probe
- `crates/lc-engine/src/import.rs:798-905`: after `probe_all` (parallel, min(cores,8)), the per-file loop runs sidecar read, XMP parse, `copy_into` -> `import_move::copy_new` (copy, fsync, read-back hash verify, `import_move.rs:178-205`), all serial, plus optional DNG conversion (`:880-896`, a full RAW decode and DNG write per file). 
- Cost: IO bound; on NVMe/SATA SSD two copies in flight overlap fsync latency (typically 1.5-2x); on a card reader or HDD parallel copies are slower, so keep the default at 1 for unknown media and use 2 for same-device SSD, or only parallelise the CPU-heavy DNG conversion (rayon, it is pure CPU: about 1-2 s per 24 MP serial, estimate).
- Proposed: assign names serially (the `rename::expand` counter `self.seq` and `folders` are computed in order), copy/convert in a bounded group of 2, collect `PreparedItem` in order.
- Risk: `copy_new` already uses `AlreadyExists` retry so concurrent same-name races are safe; ordering of `PreparedItem` must be preserved for undo/ID allocation. Priority P1; the DNG conversion part is P0 for users who tick "Copy as DNG".

### F10 (P2) GPU denoise: CPU gather/blend between tiles
- `crates/lc-gpu/src/denoise.rs:149` runs `run_tiled_channels` (shared with the CPU path): per tile it builds the input on the CPU, dispatches, reads back, blends serially (`li-seg/src/denoise.rs:874-893`, about 20-40 ns per output pixel, i.e. roughly 0.5-1.5 s of serial work for 24 MP, estimate). Pipeline with a rayon task: gather tile k+1 and blend tile k-1 while tile k runs on the GPU; or parallelise the blend by rows. Risk: float accumulation order; keep tile-major order. P2.

### F11 (P2) Photocraft Remove Backgrounds (batch) is intentionally serial
- `crates/pc-ui-egui/src/generate_ui.rs:2810-2860` `start_batch`: GPU work is the Comfy queue (one at a time is right). The CPU side of each file (decode `file_rgba_srgb`, per-pixel composite `RgbaImage::from_fn` at `:2831`, PNG `save` at `:2847`) is serial and could be overlapped with the next file's Comfy job via one prefetch/post thread. Gain maybe 20-30 %. P2.

### Other queues checked, no change needed
- Library thumbnails (`JobPool`, `lc-preview/src/pool.rs:96-135`): parallel, newest-wins per slot.
- Smart Sort tagging and People faces: per-photo parallel (but see policy about two pools).
- SAM 3 (`sam3` worker, `lc-engine/src/segment/worker.rs:159`): single worker with idle unload; correct for a 3.4 GB F32 model.
- Autosave/Save As workers: **[OVERLAP: MATRIX "Snapshot Save As worker" implemented]**.
- Navigator: **[OVERLAP: implemented, MATRIX]**.
- AI watcher: **[OVERLAP: implemented, MATRIX]**.

## E. Work on the UI thread that should be off it (task 4)

### F12 (P0) Adding a Subject / Background / Sky / Depth mask runs the model on the UI thread
- Path: `lc-ui-egui/src/panels/masking.rs:127-129` (click) -> `app.run("mask.add", {"kind": k})` -> `Session::execute` inline -> `crates/lc-engine/src/cmd/masks.rs:178-181` `s.quick_segment(id, &mut shape)` -> `crates/lc-engine/src/quickseg.rs:96-124`: `li_seg::shared(&dir)` (first use: `into_optimized()` of a 170 MB IS-Net plus a SHA-256 over the whole weight file, `li-seg/src/lib.rs:1169-1187`), then `self.preview_job(...).run()` (a 1024 px develop render, inline, `quickseg.rs:103-107`), then `model.predict` (single-thread tract on 1024^2). Also `mask.addComponent` (`masks.rs:220`).
- Same inline path for every photo when settings that contain masks are Copied/Pasted/Synced: `crates/lc-engine/src/cmd/develop.rs:640,669,694,808` and `lc-engine/src/lib.rs:637` -> `transfer_settings` -> `refresh_transferred_masks` (`quickseg.rs:21-33,57-68`) => one render + one model run PER TARGET PHOTO on the calling thread. Sync 50 photos with a Subject mask = 50 inferences with the window frozen.
- Cost: IS-Net 1024^2 on one tract thread is plausibly 5-20 s (estimate; no measurement in repo), first call adds model load/verify (several seconds). After F1 about 2-7 s: still a visible freeze.
- Proposed: run it as an engine background task (pattern already in the repo: `segment_text_later` `masks.rs:111-125` returns `{pending}` and `queue_later`); i.e. create the mask immediately with the classical estimate (the code already falls back to that when no model: `quickseg.rs:7-8`) and fill `seg` on a worker, applying the result on the UI thread when done (key by photo+mask id; ignore if settings changed). For Sync/Paste, batch the targets on one worker with one model load. Also move the model load/verify to first-use-on-worker.
- Risk: undo semantics (one history entry for the mask, one for the fill, or merge); stale results when the photo changes (key already exists: `segment_key`). P0.
- **[no overlap]**

### F13 (P0) Photocraft Select Subject / Sky / Object (click) / Quick Selection prior run inline
- `crates/pc-engine/src/smartselect_cmds.rs:164-175` `select_subject`, `:148-162` `object_selection`, `:88-111` `quick_selection` (calls `seg::quick_prior`), `crates/pc-engine/src/select_extra_cmds.rs:49-70` `select_sky`; model call in `crates/pc-engine/src/seg.rs:47-52,132-137`. None uses `jobs::run`/`edit_job` (contrast `cutout_cmds.rs:182` and `ai_cmds.rs:452-657` which do), and `jobs_ui::run` (`pc-ui-egui/src/jobs_ui.rs:56-90`) only backgrounds commands that opt in. So the whole document is composited (`seg.rs:33-42` `photocraft_compose::render`), converted to 8-bit (serial `map`+`flatten`), the model runs single-threaded, then `matting::refine_mask` runs, all on the UI thread, per click for Object Selection click mode.
- Cost: composite of 24 MP (parallel) + IS-Net 1024^2 (plausibly 5-20 s single-thread) + refine; each Object Selection click repeats it (`probability` has no cache except `quick_prior`'s `QUICK_PRIOR`).
- Proposed: wrap in `crate::jobs::edit_job` like `cutout_cmds.rs:182`; cache the probability map per (document revision, layer, sample-all) like `QUICK_PRIOR` (`seg.rs:98-120`) so repeated Object Selection clicks reuse it. P0.
- Risk: selection mode (replace/add) semantic with the job lock is already handled by `edit_job`. **[no overlap]**

### F14 (P1) Gallery thumbnails decode full PNGs on the UI thread
- `crates/pc-ui-egui/src/generate_ui.rs:670-678` `thumb_texture`: `image::open(path)` (full resolution AI result, often 1-4 MP) + `to_rgba8` + texture upload on first request per entry, in the frame that draws the gallery. Scrolling a long history causes a hitch per new entry (about 20-100 ms each for 2-4 MP PNG, estimate).
- Proposed: decode and downscale to the display size on a worker (the file already has `filmstrip_ui` workers as a pattern, `filmstrip_ui.rs:61-90`), return a texture via `ctx.request_repaint`. P1.

### F15 (P2) Small synchronous reads in click handlers
- `generate_ui.rs:2246,2443,2633,1654` `std::fs::read` of a full image/workflow before the (already backgrounded) decode; trivial on SSD, a stall on a slow share. `lc-ui-egui/src/smart_sort_task.rs:57` `prepare_inputs` builds `RenderJob`s for every unprocessed photo on the UI thread (cheap per photo but O(library) per click). `lc-ui-egui/src/panels/left.rs:362` `read_dir` is on a thread already (`lc-fs-list`). P2.

### Checked and fine
- Document open/decoding: backgrounded (`pc-engine/src/jobs.rs:483`, `pc-ui-egui/src/lib.rs:1059`).
- Smart Sort / People model loading: on the worker (`lc-ui-egui/src/smart_sort_task.rs:81-85`).
- Restoration UI: worker (`pc-ui-egui/src/restoration_ui.rs:193`).
- AI denoise/enhance in Develop: Enhance worker (`lc-engine/src/enhance/mod.rs:276`).
- Export: worker.

Note on F2 tile counts: `bsrgan-2x` and `realplksr-2x` use 512 tiles (halo 128, core 256, step 192, about 27 tile inferences per MP) and `realplksr-4x` uses 256 (`li-seg/src/lib.rs:354,369,384,399`); only `bsrgan-4x` has the extreme 112 px halo (about 1,700 per MP). So tile parallelism helps all, but only the 2x models are plausible on CPU.

## F. Oversubscription analysis (global picture)

Concurrent consumers that can all be active at once during a typical session: (1) UI thread (egui), (2) JobPool workers (up to 6) each rendering and calling rayon-parallel demosaic/filters, (3) one export / preview-build / import / smart-sort thread, each launching rayon work, (4) tract (after F1) on the same rayon pool, (5) private cores/2 rayon pools in smart sort and people, (6) scoped-thread helpers (`effects.rs:840` up to 16, `png.rs:365` up to 32, `store.rs:489` up to 32, `pixels.rs:187` up to 32, `softpaint.rs:204`, `gpu_canvas.rs:1187`), (7) wgpu submission/driver threads, (8) candle (SAM 3) on rayon.
Failure modes: (a) a heavy rayon region started from JobPool threads and an export thread together makes the UI thread and the GPU feeder fight for cores (frame hitches, GPU idle); (b) the scoped-thread helpers spawn N OS threads per call and are often invoked from inside rayon tiles (PNG banding, compositor effect maps), giving N x N runnable threads and cache thrash; (c) two private cores/2 pools plus the global pool exceed the core count when tagging and faces run together; (d) a second tract pool (if `Executor::multithread(n)` is chosen).
None of these is a correctness bug; they cost latency and jitter.

**Recommended single policy (concrete):**
1. One compute budget `C = max(2, physical_cores - 1)` (leaves a core for the UI thread and the GPU submit thread). Physical cores: `/sys/devices/system/cpu/cpu*/topology/thread_siblings_list` on Linux (no new dep), `available_parallelism()/2` heuristic elsewhere, overridable by `LOCAL_IMAGE_THREADS`.
2. `rayon::ThreadPoolBuilder::new().num_threads(C).thread_name(|i| format!("li-compute-{i}")).build_global()` once at app start (before any rayon use; `apps/local-image/src/main.rs`), and `tract_linalg::multithread::set_default_executor(Executor::RayonGlobal)`.
3. The five private pools and the six scoped-thread helpers use `rayon::current_num_threads()` (or `pool.install` on the global pool) instead of their own `available_parallelism()`; keep `effects.rs:rows_par` on scoped threads (documented reason) but clamp it to `current_num_threads()`.
4. Job-level concurrency (photos in flight for previews/export/import) = small numbers from section D (2-4) with a shared counting permit (`heavy_jobs = 2`): a build, an export and an AI job do not all fan out simultaneously; visible-grid renders (JobPool) are never blocked by the permit.
5. Never `par_iter`, `rayon::scope` or `Session` model inference on the UI thread (F12, F13).
6. Lower the priority of the batch threads on Linux/macOS is not available without `libc` (already in lock via other crates; `unsafe_code = deny` workspace-wide blocks calling `setpriority` unless isolated in the one allowed platform helper), so prefer the permit approach.

## G. Overlap summary with PERFORMANCE-EXECUTION-MATRIX.md

| Matrix item | This audit |
|---|---|
| Scoped AI watcher, Snapshot Save As worker, Navigator worker, O040 compositor reuse, FC001 cache | implemented; no finding, do not redo |
| Fixed-tile CPU Rescue (`li-seg/src/enhance.rs`) | F2 is a different angle (tile parallelism, redundancy, UI guard); respects "do not reduce halo" |
| Develop panels / GPU stages; compositing/brushes GPU; thumbnails/previews (Navigator vs Develop thumbnails); export resampling GPU | not touched except to note that the preview-build and export batch loops are serial around those GPU stages (F6, F9) |
| RAW decode/denoise (`denoise_bench`) | F3, F10 are the CPU-fallback and CPU-side-of-GPU parts; the GPU kernel itself is out of scope |
| Live swatches, warp/selection/view/mesh/text-path vector frames | not related |
Everything else (F1-F15, policy) is new relative to the Matrix. Measurements: none run (read-only); every cost is an estimate from algorithm and image size unless a report is cited.

## H. Ranked top 10 (user-visible gain vs effort) and builder batches

| Rank | Change | Priority | Gain | Effort |
|---|---|---|---|---|
| 1 | F1 + policy: tract `RayonGlobal` default executor, `multithread-mm`, one sized global rayon pool | P0 | 2.5-3x on every ONNX model (segment/sky/depth, upscale, denoise, restoration, CLIP) | S (in progress) |
| 2 | F12: Develop AI masks (add, addComponent, Copy/Paste/Sync) off the UI thread | P0 | removes multi-second freezes (50x for Sync) | M |
| 3 | F13: Photocraft Select Subject/Sky/Object click/Quick prior as jobs + probability cache | P0 | removes UI freezes, repeat clicks instant | S-M |
| 4 | F9: Export batch 2-3 deep pipeline (render/encode overlap, ordered writes) | P0 | ~1.5-2.5x batch export | M |
| 5 | F6: Build Previews + Smart Previews, 2-4 photos in flight | P0 | ~2-3x preview builds | S |
| 6 | F2/F3: bounded tile windows in `Upscaler::tiled` and `run_tiled_channels`, plus CPU time estimate/guard | P0 guard, P1 speed | near-linear on many-core CPUs after F1; avoids hour-long CPU runs | M |
| 7 | F4: parallelise `pixels.rs::adjusted` and `adjust_cmds::process_surface` wrappers | P1 | 4-8x on destructive adjustments of large layers, -384 MB | S |
| 8 | F8 + F7: li-ai enhance finishing (skip opaque `source` resize, parallel resize/blend) and restoration pre/post rows | P1 | seconds per upscale/colourise at 24 MP | S |
| 9 | F5: `stack::combine` parallel + histogram entropy | P1 | 10-30 s -> ~2 s for entropy/median stacks | S |
| 10 | F9b: import copy/DNG convert group of 2, F14 gallery thumbnails off-thread | P1 | import + gallery smoothness | S-M |
Honourable mentions (P2): F10 GPU denoise gather/blend pipeline, F11 batch cutout prefetch, F15, pc-paint dab row banding, `lc-segment::mask` loops, lc-pipeline primary/finish serial pockets (only if the CPU Develop path matters), smart-sort batch barrier, shared decode between tagging and faces.

### Builder batches (file ownership; no file appears in two batches)
- **Batch A "Runtime policy and tract"** (one builder, already running): `crates/li-seg/Cargo.toml`, `crates/li-seg/src/lib.rs` (init only), `apps/local-image/src/main.rs` (global pool + executor + `LOCAL_IMAGE_THREADS`), new `apps/local-image/src/compute_policy.rs`. Then (second pass) replace `available_parallelism()` pools in `crates/lc-engine/src/smart_sort/mod.rs`, `smart_sort/people.rs`, `crates/lc-ui-egui/src/people_task.rs`, and the clamp in `crates/pc-compose/src/effects.rs:840`.
- **Batch B "AI off the UI thread"**: `crates/lc-engine/src/quickseg.rs`, `crates/lc-engine/src/cmd/masks.rs`, `crates/lc-engine/src/cmd/develop.rs` (transfer sites only), `crates/lc-engine/src/lib.rs:630-640`, `crates/lc-ui-egui/src/panels/masking.rs`; and separately in the same batch for Photocraft: `crates/pc-engine/src/seg.rs`, `smartselect_cmds.rs`, `select_extra_cmds.rs`. Needs tests: mask appears pending then filled, stale result ignored, one undo step.
- **Batch C "Batch pipelines"**: `crates/lc-engine/src/export.rs` (`run_batch_with_ctl` only), `crates/lc-ui-egui/src/export_task.rs`, `crates/lc-engine/src/cmd/previews.rs`, `crates/lc-engine/src/import.rs`, `crates/lc-engine/src/import_move.rs`. Tests: output bytes and order identical to serial (compare against `D=1`), cancel mid-batch, conflict policies unchanged.
- **Batch D "CPU AI tiling"**: `crates/li-seg/src/enhance.rs`, `crates/li-seg/src/denoise.rs` (`run_tiled_channels` only; keep `lc-gpu/src/denoise.rs` call-compatible), `crates/li-seg/src/restoration.rs`, `crates/li-ai/src/enhance.rs`. Needs F1 merged first. Test: byte-identical output vs the sequential version on the existing tiny-model fixtures; do not run the 15-minute seam test (MATRIX); the 44x16 seam fixture is enough.
- **Batch E "Pixel-loop pockets"**: `crates/pc-engine/src/pixels.rs`, `crates/pc-engine/src/adjust_cmds.rs` (wrapper only), `crates/pc-algo/src/stack.rs`, `crates/pc-ui-egui/src/generate_ui.rs` (`thumb_texture` and nothing else; batch-cutout code at 2810 left alone), optionally `crates/lc-segment/src/mask.rs`.
Batches B, C, D, E are independent and can run in parallel after A merges; D depends on A.

### Verification notes for builders
- Determinism: assert bit-identical outputs between 1 and N threads (existing pattern: `lc-raw/tests/it/raw_speed.rs` builds explicit 1 vs 8 thread pools).
- No new dependencies are needed: `rayon` is in every touched crate except `lc-segment`; `tract-linalg` and `rayon` are already in `Cargo.lock` (F1 changes the dependency edge list of `tract-linalg` in `Cargo.lock`, not the package set; check on merge per the pure-Rust rule).
- Do not run these workloads concurrently with compiles; per-tile model timings above are estimates and should be measured once (one tile, one cold + three warm) before choosing window sizes.
