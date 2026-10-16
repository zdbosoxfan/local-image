# REVIEW: Threading policy for Local Image (Fable)

Date: 2026-10-13. Reviewer: Claude Fable 5.1. Read-only review of `docs/wip/AUDIT-multithreading.md` section F plus web research. Written incrementally; sections are appended as they are finished.

## 0. Spot-checks of the audit (what I verified in the code)

All paths relative to `.claude/worktrees/integrate`.

| Audit claim | Verified? | Evidence |
|---|---|---|
| rayon global pool never configured | Yes | `grep build_global` over the whole tree: no hits |
| tract executor still single-thread in this worktree | Yes | no `set_default_executor` / `multithread-mm` / `RayonGlobal` in `crates` or `apps` (the Batch A builder's work is not merged here yet) |
| `Executor::RayonGlobal` exists and `set_default_executor` is a process-wide `static Mutex` | Yes | `~/.cargo/registry/src/.../tract-linalg-0.23.8/src/multithread.rs:31` (RayonGlobal), `:52` (static), `:65` (`set_default_executor`); also `multithread_tract_scope` at `:69` gives a thread-local override, useful for tests |
| JobPool = cores-1 clamped 2..8; Renderer caps at 6 | Yes | `crates/lc-preview/src/pool.rs:96-100`, `crates/lc-ui-egui/src/render.rs:158,167` |
| `effects.rs:rows_par` uses scoped OS threads up to 16 with a documented reason | Yes | `crates/pc-compose/src/effects.rs:831-849` |
| PNG IDAT banding spawns up to 32 scoped threads | Yes | `crates/pc-codecs/src/codecs/png.rs:365` |
| three private rayon pools (smart sort, people) | Yes | `crates/lc-engine/src/smart_sort/mod.rs:228,285`, `smart_sort/people.rs:748`, `crates/lc-ui-egui/src/people_task.rs:105` |
| Subject/Sky/Depth mask inference runs inline on the command path (UI thread) | Yes | `crates/lc-engine/src/cmd/masks.rs:178-180` calls `s.quick_segment`, which renders (`quickseg.rs:106-107`) and predicts (`:111-117`) inline |
| export batch is a strictly serial `for` loop | Yes | `crates/lc-engine/src/export.rs:1459` (`run_batch_with_ctl`), loop at `:1476-1500` |
| `li-seg::denoise::run_tiled` is row-parallel (`Sync` model) | Yes | `crates/li-seg/src/denoise.rs:193-201` |
| `unsafe_code = "deny"` workspace-wide blocks a raw `setpriority` call | Yes | `Cargo.toml:138`; no priority/nice/QoS code exists anywhere (`grep setpriority|qos_class`: none) |
| No new dep needed for physical core count | Partly | `num_cpus` is already in `Cargo.lock:4798` (transitive); `num_cpus::get_physical()` would avoid parsing sysfs by hand, but a direct dependency edge would need the pure-Rust check (num_cpus is Rust over `libc`, no C build) |
| `lc-sysmem` can give memory headroom | Not as described | `crates/lc-sysmem/src/lib.rs` only exposes `release_free_memory()` / `release()`; there is no "available memory" query today. A memory gate needs a small new helper (read `/proc/meminfo` `MemAvailable` on Linux; `sysctl`/`host_statistics` on macOS needs a crate or a platform helper) |

Two things I did not find and the audit should not assume: (1) any existing thread-priority or QoS code, (2) any memory-headroom query. Both appear in the policy below as new, small, well-scoped helpers.

## 1. What the sources say (research notes)

Each entry: what the source establishes, then how it applies here. Dates are the day I read them (2026-10-13).

### rayon (the data-parallel pool we use everywhere)
- **Global pool is created on first use with one thread per LOGICAL CPU** ("the number of logical CPUs", `RAYON_NUM_THREADS` overrides). `build_global` is optional, must run before any rayon use, and a second call returns an error. Source: https://docs.rs/rayon/latest/rayon/struct.ThreadPoolBuilder.html ; FAQ: "on hyperthreaded systems this counts logical cores, not physical ones" https://github.com/rayon-rs/rayon/blob/main/FAQ.md
- **`ThreadPool::install` runs the closure inside that pool; nested `join`/`scope`/`par_iter` stay in whichever pool the current thread belongs to.** Calling `install` from a worker of another pool makes the caller "try to keep busy while the op completes" (it blocks and work-steals). Source: https://docs.rs/rayon/latest/rayon/struct.ThreadPool.html
- **Blocking inside rayon tasks can deadlock the pool.** Worked example: tasks that `send` on a full bounded channel occupy every worker while the consumer also needs a worker: "a classic deadlock"; the author's rule: trouble comes when "multiple pieces of code with control flow dependencies are sharing the same pool". Source: https://dev.to/dimfeld/avoiding-deadlock-from-rayon-thread-pool-exhaustion-59l4
- **Two runtimes each sized to the core count = 2x oversubscription**; the fix was an explicit budget per runtime. Source (production case, Tokio + rayon): https://posthog.com/blog/untangling-rayon-and-tokio
- `ThreadPoolBuilder::spawn_handler` lets us spawn pool workers ourselves (so a dedicated low-priority pool can set nice/QoS on its workers at start). Same ThreadPoolBuilder page.

Applies here: our global pool is unconfigured (16 workers on the 9800X3D, 16 on an M4 Max counting E-cores), there are three extra private rayon pools plus six scoped-thread helpers, and tract will soon join the same pool. Sizing the global pool once and routing everything through it is the single biggest structural fix. The deadlock rule matters for F9 (export pipeline) and F6 (preview build): the bounded channel between stages must never be waited on from inside a rayon worker.

### tract (ONNX inference)
- tract-linalg 0.23.8 `Executor` has `SingleThread` (default), `MultiThread(Arc<ThreadPool>)` (its own pool, named `tract-default`) and `RayonGlobal` ("Use rayon's GLOBAL thread pool"). `set_default_executor` is a process-wide static; `multithread_tract_scope` is a thread-local override. Matmul parallelism is gated by panel and element thresholds so tiny models stay inline. Source: `~/.cargo/registry/src/.../tract-linalg-0.23.8/src/multithread.rs:14-80` (the hosted doc page https://docs.rs/tract-linalg/latest/tract_linalg/multithread/index.html lists the items but not the prose; the `doc/multithreading.md` the audit implied does not exist on GitHub, 404).

Applies here: the audit's choice of `RayonGlobal` is right; `Executor::multithread(n)` would stack a second N-thread pool under any `par_iter` over tiles.

### egui / eframe (the UI thread)
- `Context` is cheap to clone, `Send + Sync`; `request_repaint()` "If called from outside the UI thread, the UI thread will wake up and run" (eframe wires the callback). `request_repaint_after` takes the smallest requested duration. The docs warn that extra passes cause "frame judder". Source: https://docs.rs/egui/latest/egui/struct.Context.html
- egui is immediate mode: every frame re-runs the whole UI closure, so anything slow inside `update()` stretches the frame directly. (The code already follows the pattern: 196 `request_repaint*` call sites in `lc-ui-egui` + `pc-ui-egui`, and worker threads like `export`, `lc-import`, `smart-sort` hand results back through channels.)

### Comparable applications
- **darktable**: "resource level" presets (`resource_small / default / large`) express memory for the pipeline, tiling buffer, thumbnail cache and OpenCL memory as fractions of what the machine has; tiling when memory is short is "sometimes up to 10x slower"; OpenCL has a "micro nap" (default 250 us) "so the GPU can handle screen updates", an "advantage hint" that sends work to the CPU when the GPU is estimated slower, and an "OpenCL scheduling profile" (`default`: GPU does the centre view, CPU the preview; `very fast GPU`: both on the GPU). Sources: https://docs.darktable.org/usermanual/stable/en/special-topics/mem-performance/ and https://docs.darktable.org/usermanual/stable/en/preferences-settings/processing/
- **RawTherapee**: "Maximum number of threads for Noise Reduction": memory grows by about 128 MB per thread, default 0 = auto (Tmax = 2 x physical cores), lower it if you run out of RAM. Source (rawpedia, older wording): https://rawpedia.rawtherapee.com/Preferences (page moved; wording confirmed via search results)
- **GIMP**: System Resources has "Number of threads to use" (GEGL) and "Tile cache size" (RAM for image data; suggestion about half of RAM). Source: http://docs.gimp.org/3.2/en/gimp-prefs-system-resources.html
- **Krita**: Performance settings expose "Memory limit" ("maximum space Krita will reserve on your RAM"), "CPU limit" (cores allowed), "Frame rendering clones limit" (capped by cores; "try lowering this value" on heavy files), "Fps limit" so the canvas updates less often while painting. Source: https://docs.krita.org/en/reference_manual/preferences/performance_settings.html
- **Blender (Cycles)**: Threads = Auto-detect or Fixed; Fixed exists so "if you want to use your computer while rendering, you can set the property to a thread count lower" than the logical count; a user-reported change in 2.91 limits auto-detect to physical cores. Source: https://lists.blender.org/pipermail/bf-docboard-svn/2020-September/000861.html and https://developer.blender.org/T84267 (the latter is a user claim)
- **Photoshop**: "Let Photoshop Use" defaults to 70 % of RAM; Adobe says not to go above 85 % because that leaves "no memory for other essential system applications"; tune in 5 % steps using the Efficiency indicator; cache levels and tile size are the other knobs. Source: https://helpx.adobe.com/photoshop/using/memory-performance.html (via search; the kb page returned 403)
- **Lightroom Classic**: GPU use is Auto/Custom/Off with separate "Image processing" and "Export" boxes; 8 GB dedicated VRAM recommended for export on GPU; only one GPU is used. Source: https://helpx.adobe.com/lightroom/kb/lightroom-gpu-faq.html . I found no Adobe statement on CPU core counts or parallel exports (community posts only), so the "Lightroom leaves headroom" idea in the task is not something Adobe documents; what Adobe documents is the memory headroom rule (Photoshop) and the GPU on/off split (Lightroom).

The pattern across all six: (1) memory budget is the first-class setting, expressed as a fraction of the machine; (2) thread count is auto by default with a user override whose stated purpose is "I want to keep using my computer"; (3) GPU use is a tri-state with a safety margin for display.

### Apple silicon: P-cores, E-cores, QoS
- Apple's QoS classes: User-interactive (UI, animations), User-initiated (click, results "nearly instantaneous"), Utility ("may take some time ... typically have a progress bar", seconds to minutes), Background (indexing, sync, minutes to hours). "System resources may be reallocated away from the lower priority work". Source: https://developer.apple.com/library/archive/documentation/Performance/Conceptual/EnergyGuide-iOS/PrioritizeWorkWithQoS.html
- On Apple silicon the QoS class influences placement: background work is "more likely to run ... on lower performance cores" and there is no thread-to-core affinity API. Sources: https://developer.apple.com/documentation/apple-silicon/tuning-your-code-s-performance-for-apple-silicon (body not retrievable here; wording from search excerpt) and https://developer.apple.com/forums/thread/674456
- The API is `pthread_set_qos_class_self_np(class, relative_priority)`; calling `pthread_setschedparam` on a thread opts it out of QoS permanently. Source: https://opensource.apple.com/source/libpthread/libpthread-330.201.1/private/qos_private.h.auto.html

Applies here: on the M4 Max, export/preview-build/AI workers should be Utility (they have progress bars) and indexing-style work (smart sort, face scan, thumbnails for off-screen pages) Background; the UI thread stays User-interactive. We cannot pin threads to P-cores; we tell the OS the class and it chooses. `available_parallelism()` on an M4 Max counts P+E cores together (there is no SMT), so "physical / 2" is wrong there.

### Linux scheduling
- Nice is per-thread (-20..19); each nice step changes the CFS weight by 1.25x; `SCHED_BATCH` marks a thread CPU-bound ("small scheduling penalty with respect to wakeup behavior") without changing nice; `SCHED_IDLE` runs only when nothing else wants the CPU. Source: https://man7.org/linux/man-pages/man7/sched.7.html

Applies here: nice +5..+10 on batch workers makes the UI thread win every contested time slice (1.25^5 = 3x weight, 1.25^10 = 9x) while still using all idle cores. This is the standard way a Linux app "leaves headroom" without idling a core.

### Rust standard library
- `std::thread::available_parallelism()` "Returns an estimate of the default amount of parallelism"; it honours cgroup quotas and affinity masks on Linux, counts one Windows processor group only, and "does not account for differences in (co)processor capabilities" (so P and E cores count the same). It "should not be called from hot code". Source: https://doc.rust-lang.org/std/thread/fn.available_parallelism.html
- There is no std API for thread priority or QoS. The `thread-priority` crate (Rust over `libc`/`windows`, no C) covers nice/priority on Linux, macOS, Windows but does not expose macOS QoS classes. Source: https://docs.rs/thread-priority/latest/thread_priority/ . QoS needs one `libc::pthread_set_qos_class_self_np` call (unsafe FFI, same shape as the one call `lc-sysmem` already isolates, `crates/lc-sysmem/src/lib.rs:9`).

### Memory bandwidth and SMT
- SMT siblings share execution units, caches and memory bandwidth; the second thread only helps when the first stalls on memory and bandwidth is not already saturated. For streaming image kernels the gain is small or negative (indirect evidence; no rayon-specific benchmark found). Sources: https://inventivehq.com/blog/cpu-thread-scaling-llm-inference and the Photoshop engineers' observation that 8+ processors gave "far less encouraging" scaling (ACM Queue interview, via search).
- Nested pools multiply threads (N x N) and cause "cache thrash"; the fix is one pool or a flattened index space. Source: the TBB nested-arena thread at https://lists.r-forge.r-project.org/pipermail/rcpp-devel/2026-May/011014.html (same mechanism).

### GPU sharing between UI and compute
- GPU queues are time-sliced per context; a long dispatch can only be preempted at a coarse boundary, so splitting long work into smaller dispatches gives the scheduler more yield points; compositors see late presents and drop frames when the GPU is overwhelmed. Sources: https://docs.nvidia.com/drive/drive-os-5.2.6.0L/drive-os/DRIVE_OS_Linux_SDK_NGC_Development_Guide/Graphics/graphics_gpu_scheduling.html , https://learn.microsoft.com/en-us/windows/win32/api/dxgi1_2/ne-dxgi1_2-dxgi_compute_preemption_granularity , https://gitlab.gnome.org/GNOME/mutter/-/merge_requests/1383
- darktable's "micro nap" is the practical precedent: a tiny sleep between OpenCL kernels so the display gets its turn.

Applies here: `lc-gpu` opens its own `wgpu::Instance`/`Device` (`crates/lc-gpu/src/ctx.rs:530-552`), separate from eframe's. That is good (no shared command-encoder lock with the UI) but both land on the same physical GPU, so a 2 s denoise dispatch still delays the UI's present. Tile-sized dispatches (already the case for denoise) are the right shape; a whole-image upscale in one dispatch is not.

## 2. Rules of the road (one page)

These rules are general. They must hold on a 2-core laptop with 8 GB and an integrated GPU, on a 32-core workstation, on hybrid P/E chips (Apple silicon, Intel 12th gen and later), with SMT on or off, and on Linux, macOS and Windows. The owner's Ryzen 9800X3D / RTX 5090 and MacBook Pro M4 Max are only examples. Numbers below are floors, ceilings and fractions, never fixed counts.

### The compute profile (detected once at start-up, then frozen)
Everything else derives from this. Put it in one small crate (`lc-sys` or extend `lc-sysmem`, which is already "the only crate allowed `unsafe`", `crates/lc-sysmem/src/lib.rs:9`).

| Field | How it is detected | If detection fails |
|---|---|---|
| `logical` | `std::thread::available_parallelism()` (honours cgroup quotas and affinity masks) | 4 |
| `physical` | Linux: unique `core_id` per `/sys/devices/system/cpu/cpu*/topology/`; macOS: `sysctl hw.physicalcpu`; Windows: `GetLogicalProcessorInformationEx` (or `num_cpus::get_physical`, already in `Cargo.lock`) | `= logical` (never halve blindly: Apple silicon and most ARM have no SMT, and SMT may be switched off) |
| `perf_cores`, `eff_cores` | macOS: `hw.perflevel0.physicalcpu` / `hw.perflevel1.physicalcpu`; Linux Intel: `/sys/devices/cpu_core/cpus`, `/sys/devices/cpu_atom/cpus`; Windows: `EfficiencyClass` | not hybrid |
| `ram_total`, `ram_available()` | Linux `/proc/meminfo` (`MemTotal`, `MemAvailable`); macOS `sysctl hw.memsize` + `host_statistics64`; Windows `GlobalMemoryStatusEx` | total = 8 GB assumed, available = total / 2 |
| `gpu` | wgpu adapter info + limits; `DeviceType::IntegratedGpu` means VRAM is shared with RAM | no GPU: CPU fallback with the time-estimate guard (rule 8) |

### The rules

1. **One global compute pool, sized to physical cores minus a little headroom.** `C = clamp(min(physical, logical) - reserve, 2, logical)` with `reserve = 1` when `min(physical, logical) >= 4`, else `0`. On hybrid chips count P and E cores together (E-cores add real throughput; the OS places threads by priority, rule 7). `rayon::ThreadPoolBuilder::new().num_threads(C).thread_name(..).build_global()` runs first thing in `main`, before anything touches rayon; honour `RAYON_NUM_THREADS` / `LOCAL_IMAGE_THREADS` if set. *Why:* rayon's default is one thread per logical CPU and cannot be changed after first use; image kernels are memory-bandwidth bound, so SMT siblings add little and 2x pools add cache thrash. *Sources:* rayon ThreadPoolBuilder docs; rayon FAQ; PostHog case; Blender's move to physical cores.

2. **Every library that can parallelise uses that one pool.** tract: `set_default_executor(Executor::RayonGlobal)` (never `Executor::multithread(n)`); candle already uses rayon; the three private `ThreadPoolBuilder` pools in smart sort / people become `install` on the global pool or are sized from `rayon::current_num_threads()`; the scoped-thread helpers (`png.rs:365`, `pixels.rs:187`, `store.rs:489`, `softpaint.rs:204`, `gpu_canvas.rs:1187`) become rayon `par_chunks`; `effects.rs:rows_par` keeps its OS threads (documented reason) but caps at `current_num_threads()`. *Why:* nested pools multiply (N x N runnable threads). *Sources:* tract-linalg `multithread.rs`; TBB nested-arena thread; audit section F observation (b).

3. **Pick the mechanism by the shape of the work.** rayon = CPU-bound, splittable, short tasks (a tile, a row band, one photo in a batch) that never wait. Dedicated OS thread (named, one per long-lived job) = anything that blocks or lives long: export, import, preview build, model worker, file watcher, HTTP to ComfyUI, GPU readback waits. No async runtime: the app has none, `pollster` is enough for wgpu, and adding tokio would add a second thread budget to balance (PostHog case). *Source:* rayon ThreadPool docs (`install` blocks the caller); dimfeld deadlock write-up ("dedicate plain OS threads to each long-lived pipeline stage").

4. **Never block the UI thread. Budgets:** at 60 Hz a frame is 16.7 ms and at 120 Hz (ProMotion MacBooks) 8.3 ms; the app's own logic inside `update()` gets at most about 4 ms, leaving the rest for layout, text and paint. Anything that is not bounded by the number of widgets on screen (decode, render, inference, a `read_dir`, a sync of settings to N photos, a full-layer adjustment) goes to a worker and comes back through a channel plus `ctx.request_repaint()`. Work expected over 100 ms shows a busy state; over 1 s shows progress and a Cancel. *Sources:* egui Context docs (`request_repaint` from other threads); Nielsen's 0.1 s / 1 s / 10 s limits (https://www.nngroup.com/articles/response-times-3-important-limits/, well known, not re-fetched); audit F12/F13 are the current violations.

5. **No waiting inside a rayon task.** A rayon closure must not lock a contended mutex for long, `send` on a bounded channel, `recv`, join a thread, poll the GPU, read a file, or call `install` on another pool. Pools exhaust and deadlock when tasks wait for something that itself needs a pool thread. If a stage must wait, it runs on its own OS thread (rule 3) and only its inner loop uses rayon. *Source:* dimfeld deadlock write-up; rayon `install` docs.

6. **Limit concurrent heavy jobs by memory first, cores second.** A heavy job (RAW render, export item, preview build item, AI tile window, import DNG conversion) takes a permit before it starts. Permits are computed, not fixed: `H = min( clamp(C / 4, 1, 4), floor(budget / per_job_bytes) )` where `budget = clamp(0.7 * ram_total - rss_now, 0, ram_available() - 1 GB)` is re-read on each admission and `per_job_bytes` is estimated from the image (24 MP f32 RGB = 288 MB source + output + working set; tile windows use their tile size). At least one job always runs. Integrated GPUs share this budget; discrete GPU jobs are additionally limited by VRAM. Visible-grid thumbnail renders (JobPool) never queue behind batch permits. *Why:* every comparable app makes memory the first-class control (darktable resource levels, Photoshop 70 % / 85 % ceiling, Krita memory limit, RawTherapee "128 MB per thread"), and swapping costs more than any thread count gains. *Sources:* listed in section 1.

7. **Background work runs at lower priority, so the UI wins contested time slices instead of idling a core.** Classes: Interactive (UI thread, visible-grid renders, slider renders) = normal; Utility (export, preview build, import, AI with a progress bar) = Linux nice +5, macOS QoS Utility, Windows `THREAD_PRIORITY_BELOW_NORMAL`; Background (smart sort, face indexing, off-screen thumbnails, model download) = Linux nice +10 and `SCHED_BATCH`, macOS QoS Background, Windows `THREAD_MODE_BACKGROUND_BEGIN`. Set on the job's own thread; for rayon fan-out, use a second pool built with `spawn_handler` whose workers set the class at start (size C, so under load the kernel gives the interactive pool 3x-9x the weight; when idle, background work still gets every core). All of it in the one platform helper crate behind a safe API; nothing else gets `unsafe`. On Apple silicon this is also what moves work to E-cores; there is no affinity API and we should not want one. Until the helper exists, the permit from rule 6 is the stand-in. *Sources:* sched(7) (nice per thread, 1.25x per step, SCHED_BATCH); Apple QoS guide; Apple forum on affinity; thread-priority crate.

8. **GPU work goes to the GPU when it is faster and leaves the display its turn.** Use the GPU for per-pixel pipelines and conv nets when a discrete or capable integrated adapter exists and VRAM allows; keep dispatches tile-sized (hundreds of ms at most, never one whole-image dispatch) so the driver can time-slice the UI's present between them; readback and gather/blend happen on a worker, never the UI thread. `lc-gpu` keeps its own `wgpu::Device` (as today, `ctx.rs:530`), which avoids lock-sharing with eframe. On CPU-only machines any AI feature estimates its time from tile count x measured per-tile cost and warns or refuses above a threshold. *Sources:* NVIDIA GPU scheduling (timeslices, preemption), DXGI preemption granularity, mutter frame-clock issue, darktable micro-nap and advantage hint, Lightroom GPU FAQ (separate Image processing / Export switches).

9. **Every job is cancellable and reports progress without the UI asking.** A shared `AtomicBool` cancel flag checked at tile or item granularity (never more than about 100 ms of work between checks); progress via atomics or a channel plus `request_repaint`; cancel means stop admitting new items and let in-flight ones finish or abort at the next check; the UI thread never joins a worker. *Sources:* egui Context docs; existing patterns (`export.rs` `ctl.cancelled()`, `smart_sort` cancel flag).

10. **Batch pipelines overlap decode, process and encode on separate OS threads joined by bounded channels.** Stage threads (not rayon workers) pass items through channels of depth 1-2; each stage may use rayon inside; items carry a sequence number so writes stay in input order; depth and in-flight count come from rule 6, not a constant. *Sources:* dimfeld write-up (dedicated stage threads, blocking sends never from a pool); audit F6/F9.

11. **Results do not depend on the thread count.** Per-pixel maps are safe by construction. Anything that accumulates (blends across tiles, sums, histograms, running statistics) fixes its reduction order (tile-major, row-major) or accumulates in integers; tests build explicit 1-thread and N-thread pools and assert byte-identical output (pattern: `crates/lc-raw/tests/it/raw_speed.rs`). *Source:* audit section B (tract partitions over output panels, not K), existing test pattern.

12. **Measure on real files before and after, on more than one machine class.** For each change: wall time of the whole job (not just the kernel), peak RSS, UI frame time during the job (egui's frame history), and GPU-busy time; inputs are real RAW/PSD files at 24 MP and 60 MP plus one tiny fixture; run at thread counts 1, physical, logical and on at least a low-core and a high-core profile from the test matrix (section 6). No change ships on a kernel micro-benchmark alone. *Sources:* Photoshop's Efficiency indicator approach (tune in small steps while watching one number); Blender's reverted "cores + 1" change ("benchmarks ... showed that there is no gain").

## 3. Where the audit's section F is right, wrong or incomplete

### Right (keep)
- One sized global rayon pool created in `main` before any rayon use, and `Executor::RayonGlobal` for tract. Both verified against rayon docs and `tract-linalg` source. This is the biggest single fix.
- Floor of 2 workers. A 1-thread pool gives no overlap at all, and even a 2-core laptop benefits from overlapping decode with blend.
- Routing the private pools and scoped-thread helpers through the global pool, and keeping `effects.rs:rows_par` on OS threads with a cap (its reason, "a waiting worker steals other tiles", is exactly rule 5).
- Small in-flight counts (2-4) for batch pipelines, with visible-grid renders exempt from the permit.
- "Never `par_iter` or run inference on the UI thread" (F12, F13 are real and verified at `masks.rs:178-180`, `quickseg.rs:106-117`).

### Wrong or risky (fix)
1. **`physical = available_parallelism() / 2` "when SMT is on" is not a safe heuristic.** It assumes you know SMT is on. Apple silicon has no SMT (an M4 Max reports 16 and has 16 cores: 12 P + 4 E, so halving gives 8 and throws away half the machine); most ARM chips are the same; SMT can be disabled in firmware; cgroup-limited containers report a quota, not a topology. Detect physical cores per platform (section 2 table) and when detection fails use `physical = logical`, never half.
2. **`max(2, physical - 1)` reserves a core but nothing keeps the UI on it.** Neither Linux nor macOS pins threads for us; the "spare" core is used by whichever runnable thread the scheduler picks. The reserve only reduces average contention. The reliable lever is priority (rule 7). Keep the reserve on machines with 4+ cores (it costs little), drop it below 4 (on a 2-core machine `max(2, 1)` already gives 2, i.e. no reserve, so the policy must say priority carries the load there).
3. **Section F point 6 is wrong that lowering priority is "not available".** `libc` is already in `Cargo.lock`, `lc-sysmem` is the sanctioned place for one-call FFI (`lc-sysmem/src/lib.rs:9`, `pc-tablet/src/macos.rs` is the other), and `thread-priority` is a pure-Rust-over-`libc` crate if the owner prefers a dependency. Nice/QoS/Windows background mode are the standard way desktop apps leave headroom; the permit is the fallback, not the answer.
4. **`heavy_jobs = 2` and `W = clamp(compute_threads/3, 1, 4)` are hard-coded.** The owner's instruction is explicit: derive from hardware. Both become functions of `C` and the memory budget (rule 6). On an 8 GB laptop the memory term will usually give 1; on a 128 GB workstation the core term gives 4.
5. **Memory gating cites `lc-sysmem` as if it could report headroom; it cannot** (only `release_free_memory()`). The helper needs an `available_memory()` query on all three OSes, and the policy should say what the fallback is (total / 2).
6. **Windows is absent.** Eight files already carry `target_os = "windows"` paths, so the policy must name the Windows equivalents (`GetLogicalProcessorInformationEx`, `GlobalMemoryStatusEx`, `THREAD_PRIORITY_BELOW_NORMAL`, `THREAD_MODE_BACKGROUND_BEGIN`), and note `available_parallelism()` counts one processor group only on big Windows boxes.
7. **E-cores.** The audit never mentions hybrid CPUs. Count them in `C` (Apple's own guidance is to tell the OS the QoS and let it place work; there is no affinity API), and rely on QoS so interactive threads land on P-cores while Background-class work drifts to E-cores. On Intel hybrid under Linux, `physical` = P-cores + E-cores (not 2x anything); SMT exists only on the P-cores, so the sysfs `core_id` count is the right source.

### Physical or logical?
Default to **physical** when detectable (image kernels are bandwidth-bound; SMT siblings share the memory path; Blender moved to physical for the same reason), capped by `logical` (cgroup quota). Make "use SMT threads" an advanced opt-in and tell the user to measure; RawTherapee's doubling advice is the counter-example and is specific to its noise-reduction tiles. On a machine without SMT the two are equal and the question disappears.

### "Leave headroom" as Photoshop / Lightroom do it
What Adobe documents is **memory** headroom (70 % default, 85 % hard advice, "leaves no memory for other essential system applications") and a GPU on/off split; nothing official about CPU cores for Lightroom exports. Krita, GIMP and Blender expose a thread cap whose documented purpose is "I want to keep using my computer", i.e. a user override, not the default. So: automatic = all physical cores minus one with low priority for batch work; the user override is the cap. That matches rule 1 + rule 7 and section 5.

### Missing from section F (add)
- Priority / QoS classes per job type (rule 7) and the single platform helper that owns them.
- A memory budget and per-job estimate (rule 6).
- GPU placement and VRAM budget; integrated-GPU memory counting against RAM (rule 8).
- Determinism tests at 1 vs N threads for every new reduction (rule 11).
- The "test matrix" of machine classes (section 6) and the measurement protocol (rule 12).
- The bounded-channel deadlock rule (rule 5): F9's design ("consume results in order via a bounded channel") is only safe if the producers are OS threads or the channel is never full when a pool thread sends. Say so in the F9 spec.
- JobPool interaction: JobPool workers (cores-1, 2..8) are coordinators that block inside `install` on the global pool; that is fine, but after rule 1 their count should derive from `C` (e.g. `clamp(C / 2, 2, 8)`), not from `available_parallelism()` separately.

## 4. Check before merging (for any change that adds threads or heavy work)

- [ ] No new `ThreadPoolBuilder`, `std::thread::scope(available_parallelism())`, or `Executor::multithread`. New parallel loops use the global pool (`par_iter`, `par_chunks_mut`) or `rayon::current_num_threads()`.
- [ ] Nothing heavier than widget bookkeeping runs inside `update()` / a command executed inline on the UI thread. Grep the diff for decode, render, `predict`, `plan.run`, `read_dir`, `fs::read`, full-layer `map` on the UI path.
- [ ] Inside any rayon closure: no `send` on a bounded channel, no `recv`, no `join`, no `install` on another pool, no GPU poll, no file I/O, no long-held mutex.
- [ ] Long-lived or blocking work is a named `std::thread::Builder` thread with a priority class (Utility or Background) from the platform helper.
- [ ] Heavy jobs take a permit; in-flight counts and window sizes derive from `C` and the memory budget, no literal 2/3/4.
- [ ] Cancel flag checked at tile/item granularity; progress reported via atomics + `request_repaint`; the UI never joins or waits on the worker.
- [ ] Output is byte-identical at 1 thread and N threads (test with explicit pools); any reduction states its order.
- [ ] GPU dispatches are tile-sized; readback and blend are off the UI thread; VRAM/shared-memory estimate is checked before launch; CPU fallback has a time estimate guard.
- [ ] Memory: peak RSS measured on a 24 MP and a 60 MP file; no unbounded queues; no full-image clones where a recompute is cheap.
- [ ] Measured: whole-job wall time, UI frame time during the job, peak RSS, on at least one low-core (2-4 core / 8 GB) and one high-core profile; numbers in the PR.
- [ ] Pure Rust: `Cargo.lock` diff shows no new `-sys` or C-built crate; any new FFI is one call inside the platform helper crate with a `SAFETY` comment.
- [ ] Behaviour when detection fails is stated (fallbacks from the section 2 table).

## 5. Settings: what to expose, what stays automatic

Expose (all optional overrides on top of the automatic value, each showing the current automatic number next to it, in a "Performance" preferences page like Krita / Photoshop / Lightroom):

| Setting | Default | Why expose |
|---|---|---|
| Processing threads: Automatic (N) / Custom | Automatic = `C` | The one override every comparable app has (GIMP, Krita, Blender, RawTherapee); its documented purpose is "keep my computer usable while it works" |
| Use SMT / hyper-threads (advanced) | off | Lets a user on a machine where SMT helps measure it; hidden under Advanced |
| Background work priority: Low / Normal | Low | Laptop users on battery or a dedicated edit box may want batch work at full speed when they are not using the UI; Normal = skip nice/QoS |
| Memory for processing: Automatic (x GB) / Custom % | Automatic = 70 % of RAM | Photoshop 70 %/85 %, Krita memory limit, darktable resource level; tells low-RAM users why jobs run one at a time |
| Use GPU: Auto / Off; Use GPU for export | Auto, on | Lightroom's split; Off is the escape hatch for bad drivers |
| Concurrent exports / preview builds: Auto (H) / 1 | Auto | A "1" option is the safe choice on HDD/card-reader targets and for users who report corruption; derived value otherwise |
| Reset to automatic | - | One click back to the detected profile |

Keep automatic (never shown): rayon pool construction and naming, tract executor, JobPool size, tile window sizes, per-job memory estimates, QoS/nice numbers per class, SCHED_BATCH, GPU dispatch tiling, channel depths, the detection fallbacks. Log the whole compute profile once at start-up (`logical`, `physical`, P/E, RAM, GPU, derived `C`, `H`, budget) so support questions can be answered from the log.

Environment overrides for testing only: `LOCAL_IMAGE_THREADS`, `RAYON_NUM_THREADS`, `LIGHTCRAFT_GPU=0` (exists), plus a `LOCAL_IMAGE_PROFILE=laptop-2c|m1-8gb|...` that forces a test-matrix profile so CI and reviewers can exercise the low-end paths on a big machine.

## 6. Test matrix (machine classes the rules must behave on)

| Class | Example | logical / physical | RAM | GPU | What must hold |
|---|---|---|---|---|---|
| Small laptop | Intel i3 / Ryzen 3, Windows | 4 / 2 | 8 GB | integrated (shared) | `C = 2`, `H = 1`, batch at low priority; export still completes; no swap storm at 24 MP |
| Mid laptop, no usable GPU | 4c/8t Linux, driver missing | 8 / 4 | 16 GB | none | `C = 3`; AI features show a time estimate and can be refused; UI stays 60 Hz during export |
| Owner's Linux box | Ryzen 9800X3D + RTX 5090 | 16 / 8 | 64 GB | discrete 32 GB | `C = 7`; `H = 2-4` by memory; GPU does develop/denoise; UI frame time flat during a 200-photo export |
| Apple silicon entry | M1 MacBook Air | 8 / 8 (4P + 4E), no SMT | 8 GB | Metal, shared | `physical` must read 8 not 4; `H = 1` by memory; Background-class work lands on E-cores; fan stays quiet during indexing |
| Owner's Mac | M4 Max | 16 / 16 (12P + 4E) | 64 GB | Metal, shared | `C = 15`; ProMotion 8.3 ms frames hold during builds; VRAM budget counts against RAM |
| Intel hybrid desktop | i9-14900K, Windows | 32 / 24 (8P + 16E) | 64 GB | discrete | `physical` = 24 via `core_id`/EfficiencyClass, not 16; processor-group count correct |
| Workstation | Threadripper 32c/64t, Linux | 64 / 32 | 128 GB | discrete | `C = 31`; memory term does not limit; measure that 31 is not slower than 16 on bandwidth-bound kernels (if it is, cap by measurement, not guess) |
| SMT off | 8c/8t, firmware SMT disabled | 8 / 8 | 32 GB | any | halving heuristic would have given 4; detection gives 8 |
| Container / VM quota | 2-CPU cgroup on a 64-core host | 2 / 64 | 4 GB | none | `C = min(physical, logical) = 2`; memory fallback `total / 2` if `/proc/meminfo` is namespaced oddly |
| Detection failure | unknown OS or sandbox | Err | Err | unknown | fallbacks: logical 4, physical = logical, RAM 8 GB / available 4 GB, `C = 3`, `H = 1`; app still starts and logs the fallback |

For each class the acceptance checks are the four numbers from rule 12 (job wall time, peak RSS, UI frame time during the job, GPU busy) on the same two real files, at the automatic settings and with the thread override at 1.

## 7. Suggested order of work

1. Platform helper crate (`lc-sys`): compute profile detection, `available_memory()`, priority classes (Linux nice + SCHED_BATCH, macOS QoS, Windows thread priority), one `unsafe` block per OS call, safe API, logged at start-up.
2. Batch A as planned (global pool from the profile, tract `RayonGlobal`, retire private pools and scoped helpers) but with `C`, `H` and the budget computed from step 1.
3. Batch B (AI off the UI thread) and C (pipelines) using the permit, the priority classes and the bounded-channel rule.
4. Settings page and `LOCAL_IMAGE_PROFILE` test override; run the matrix once per release.
