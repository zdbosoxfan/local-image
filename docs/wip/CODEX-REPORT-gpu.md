# GPU sharing, recovery and efficiency — implementation report

Date: 2026-10-13. Branch `wip/cx-gpu`. Changes remain uncommitted. This continues the
existing shared compute profile, memory permits, priority helpers and off-thread jobs.
Implementation is complete; validation results and measurement limits are recorded below.

## QW10 / VRAM coordination, including the owner's D13 steering

- **One health model:** extended `pc-gpu::health::DeviceHealth`, retaining its three-strike /
  600-clean-frame decay policy. Recoverable operations pause for a bounded 4–60 second backoff;
  a new idle-VRAM observation can admit an earlier retry. A lost device stays unusable; its owner
  creates a replacement with independent callback state and retained consecutive-failure backoff.
- Develop now owns a `DeviceHealth` per compute context, including failed initialization. Its
  process-wide `BROKEN` flag/reason are removed. Transient captured errors use the same strike/
  retry model as the compositor. Device-lost callbacks, stage-cache generations and cached denoise
  weights cannot revive a lost device or bind its buffers on the replacement. Healthy retained
  leases still survive normal cache release. Late callbacks affect only their original health.
- The compositor consults the same compute-admission/backoff API, while a healthy device can still
  display CPU fallback pixels. Its cache budget shrinks when local AI reserves memory.
- The existing AI status worker probes `/system_stats` and `/queue`; no second poller and no HTTP
  on the UI thread. Complete observations expire after 10 seconds. A job reservation starts before
  prompt submission and spans the owned prompt wait; it wakes the host immediately. A disconnected
  server falls back to expiring telemetry and bounded GPU retry rather than an indefinite reservation.
- Local GPU consumers budget a fraction of currently free memory, retain a display/transient margin,
  and use host RAM headroom for unified memory. Network ComfyUI jobs do not reserve local VRAM.
  Develop releases view caches/recycled buffers and temporarily renders on CPU under pressure;
  compositor caches shrink even while its view is idle.
- **J12 handoff:** `DeviceHealth` and its retry methods are the future `li-gpu` health API. This job
  does not merge the window/compute devices, backend policies, crash sentinels or WGSL snippets;
  those are explicitly the subsequent J12 slices in the duplicate-systems audit.

## N10 / battery awareness

- Linux power-supply `type`, `status`, `present`, `online` probes and macOS `pmset -g batt` run on a
  power monitor, never during UI paint. Failed probes retain the last known state.
- Smart Sort, face indexing and background preview builds pause between jobs/bounded batches;
  current work can finish. Cancellation remains responsive while paused. Preview status reports
  `pausedOnBattery`.
- CPU Develop/pixel/resizing work uses half the normal budget on battery; tract inference uses a
  thread-local single-thread executor override. Foreground work stays available.
- Preferences > Performance > Processing has **Keep full speed on battery**. It applies live,
  persists with serde defaults, and old preferences default to battery saving.
- Rayon cannot resize its global pool. One lazy, shared battery pool is the necessary runtime
  power-source exception to the one-compute-pool policy; there are no per-job/model pools. AC work
  uses the existing pool, and the idle battery pool has no runnable work. Indexing pauses outside
  Rayon rather than blocking a Rayon worker.

## N11 / pixel loops and thumbnails

- Parallel destructive adjustment conversion/selection mixing, stack statistics, restoration
  preprocessing/postprocessing, finishing resizes and alpha restoration.
- Lanczos3 uses the existing image 0.25.10 vertical/horizontal tap order, f32 arithmetic, clamping,
  rounding and colour-space metadata. RGB and RGBA byte-equivalence tests include odd sizes,
  1-pixel-wide/tall inputs, alpha and one/four worker pools.
- Stack entropy previously summed a randomized HashMap iteration. Fixed bin order makes the
  reduction deterministic; per-pixel floating reductions otherwise retain their original order.
- Gallery decode/thumbnail creation moves to a bounded worker queue. Paint polls, then uploads only
  thumbnail-sized pixels. Pending requests are deduplicated, failed paths have a short negative
  cache, and texture retention follows a bounded RAM share. Gallery tiles have accessible names.
- Tests compare byte/float-bit results across worker counts and against serial references. The
  end-to-end probe runs the real editor adjustment command on a public-domain reference at tiny,
  24 MP and 60 MP sizes, with one/two/physical-budget/logical worker profiles.

## QW7 / CPU AI preflight

- A central tensor-sized estimator covers subject/background/sky/depth, CLIP and face models,
  colourisation and scratch detection. Successful inference calibrates subsequent estimates on
  this machine. Failed/cancelled calls do not make an estimate look faster.
- Shared `cpu_estimate` / `cpu_batch_estimate` widgets use theme tokens. Notices appear in Restore,
  Remove Background, selection tool controls/model settings and Smart Sort; mask/menu controls
  also give the relevant tooltip. Warnings start at 15 seconds; individual interactive inference
  above 120 seconds is refused before loading/running it. Long batches show a cancellable-work
  warning rather than treating the whole batch as one inference.
- Initial estimates are deliberately broad work estimates, **not benchmark promises**. Fixed-size
  models use their tensor size rather than full-photo megapixels. They learn from actual runs.
- Existing upscale/denoise CPU fallback routes were still present in this base. App/service routes
  now enforce the owner's GPU/NPU-only decision, including checking a ComfyUI accelerator for
  enlargement. Legacy CPU settings still deserialize; the UI offers GPU enlargement and reports
  unavailable acceleration rather than silently starting CPU inference. Denoise model cards and
  custom-model descriptions also say GPU/NPU required, with no CPU-fallback promise.
- New English strings are on both i18n pending lists; reusable notices are documented in the design
  system. No Match Look files, owner photos or desktop windows were opened or modified.

## Verification

- Offline toolchain `+1.98.1`, inherited `CARGO_BUILD_JOBS=4`, one Cargo invocation at a time,
  retained worktree target directory. Focused development tests only; final full suites cover
  every changed crate. Only failed suites/targets were rerun. Failure and interrupted logs are retained.
- Added real-GPU checks: lost-context replacement with cached view buffers; automatic operation
  recovery and local/remote pressure in the existing serialized fallback test; real headless
  Develop slider interaction under local VRAM reservation; live ComfyUI VRAM/queue plus one small
  smoke job. These use the actual hardware; no software-adapter result counts.
- GUI verification uses egui_kittest clicks (battery override, CPU preflight, gallery selection)
  and the Library headless harness slider drag. Shared notice screenshots cover dark/light,
  250/400 pt and 1.0/1.5 scale.
- `Cargo.lock`: no package added/removed; only existing workspace dependency edges. New edges:
  lc-gpu -> sysmem/pc-gpu, lc-ui -> li-seg, pc-gpu -> existing web-time. li-seg's existing sysmem
  edge moved from test-only to runtime. No new C, -sys, unsafe code or external runtime.

Full-suite results (default features, native Linux; pass counts include integration/doc tests):

| Crate | Passed | Ignored |
|---|---:|---:|
| `lightcraft-sysmem` | 30 | 0 |
| `photocraft-algo` | 714 | 4 |
| `photocraft-gpu` | 119 | 1 |
| `lightcraft-gpu` | 97 | 12 |
| `li-seg` | 83 | 8 |
| `li-ai` | 514 | 7 |
| `lightcraft-engine` | 897 | 6 |
| `photocraft-engine` | 2,117 | 33 |
| `lightcraft-ui-egui` | 338 | 9 |
| `photocraft-ui-egui` | 1,766 | 81 |
| `local-image` | 154 | 6 |
| **Total** | **6,829** | **167** |

The compositor UI total combines the successful unit/appearance/contracts targets with the
GPU target retry and remaining integration/doc targets. This avoids rerunning successful
unit tests after an integration failure. Counts/log paths are in
`target/gpu-final-validation/results.json`. Initial failures exposed legacy CPU-fallback and
immediate-recovery expectations, plus UI tests that assumed synchronous gallery decoding or
clicked below a scroll clip. Updated tests retain pixel, click, fallback and recovery assertions.

Late corrections to unified-memory admission, stack native/web dispatch, tract executor panic
restoration and retained/lost GPU context handling have focused checks in addition to the full
suites. The explicit hardware/visual checks below exercise opt-in tests omitted by default.

Final clippy passed for all 11 changed crates, `--offline --all-targets -- -D warnings`.
Formatting checks passed for every changed crate, and `git diff --check` is clean for this
job’s changes (the owner’s pre-existing task-file edits were excluded). The existing
build-script note about absent optional CJK craft fonts is unchanged.
`target/gpu-extra-validation/results.json` records ten successful focused/opt-in checks,
plus the probe build/run; failures from test setup are preserved separately. No software
adapter or skipped hardware result is counted as GPU validation.
The final denoise-description correction also passed the existing settings/custom-model click
test (including the GPU/NPU text assertion), compositor literal-translation coverage and Library
placeholder coverage. Logs: `target/gpu-denoise-copy-test.log` and
`target/gpu-copy-validation/`; clippy was repeated after this text-only correction.

### Actual GPU, service and UI checks

| Check | Result |
|---|---|
| Real `Device::destroy()`, cached view buffers, 4.1-second retry | 1 passed; RTX 5090 / Vulkan replacement; recovered image equals the original byte for byte; lost-device teardown does not poll it |
| Cached denoiser transient backoff | 1 passed; real GPU graph; retries its owning health after 4.1 seconds without requiring a Develop render |
| Retained denoiser device across normal global context release | 1 passed; healthy weights remain usable |
| Develop exposure drag with local AI reservation | 1 passed; CPU fallback clears GPU stages and releasing the reservation restores them |
| Live ComfyUI telemetry and smoke graph | 1 passed; queue was idle; one 64×64 EmptyImage → PreviewImage job; PNG returned and local reservation released |
| Shared battery/CPU-estimate widgets | 1 passed; eight dark/light, 250/400 pt, 1.0/1.5-scale screenshots visually reviewed; wrapped text and controls fit |

Develop used the normal renderer/slider path in the Library headless harness (1200×1800 pt,
400 pt sidebar), on **NVIDIA GeForce RTX 5090 (Vulkan)**. Under a simulated local AI reservation,
the exposure drag plus completed CPU fallback preview took **67.64 ms**. Thirty stationary UI
steps while that reservation remained held, after the preview completed, measured **p50 1.08 ms,
p95 1.52 ms**. Those frame numbers are not a concurrent neural-inference benchmark or a large-photo
latency promise. Pixel-cache assertions verified both pressure eviction and return to GPU stages.
Logs: `target/gpu-extra-validation/interactive.log`, `device-loss.log`, `denoise-backoff.log`,
`retained-device.log`. Default-suite `lc-gpu/tests/fallback.rs` additionally tests local versus
remote reservations and automatic captured-error recovery on the real Vulkan adapter.

Live ComfyUI reported total VRAM **33,731,248,128 bytes**, free **29,816,258,560 bytes**, an idle
queue, and an admitted app cache cap of **4 GiB** for a 4 GiB request. This check validates real
telemetry and prompt reservation lifetime. The server's UpscaleModelLoader inventory had no
installed upscale choices, so the small smoke graph is not a neural-model VRAM stress test.
`target/gpu-extra-validation/comfy-live.log` contains the observation.

Approved ONNX CPU restoration ran on the NASA fixture, including alpha preservation and a
synthetic scratch. Colourisation times were **3.26 / 2.85 / 2.67 seconds**; scratch detection
**1.01 / 1.04 / 1.37 seconds**, detecting 95 mask pixels in each sample. First runs include model
loading. These successful runs exercise the estimator calibration path; the initial estimates
remain broad. Outputs are in `target/restore-export/`; the log is
`target/gpu-extra-validation/restore-cpu.log`. The eight reviewed UI images are in
`target/gpu-resources-ui/`.

### Quality before speed: serial reference versus parallel resize

One measurement pass on an **AMD Ryzen 7 9800X3D (8 physical / 16 logical cores)**, AC power,
normal shared profile 7 threads, retained dev/test build (`opt-level=1`), with other owner jobs
running. The public-domain NASA image was expanded to each input size; finishing resize reduces
width/height by four. The old serial `image::imageops::resize` and new parallel path consume the
same pixels. The editor timing is the real `Session::execute("image.adjustments.invert", …)` path,
including document/history work, not an isolated pixel loop.

| Input | Threads | Old serial resize ms | New resize ms | Editor command ms |
|---|---:|---:|---:|---:|
| 257×193 | 1 | 3.46 | 2.29 | 4.20 |
| 257×193 | 2 | 3.46 | 1.01 | 2.49 |
| 257×193 | 7 | 3.46 | 0.53 | 1.92 |
| 257×193 | 16 | 3.46 | 0.52 | 3.00 |
| 6000×4000 (24 MP) | 1 | 1594.47 | 636.11 | 1270.13 |
| 6000×4000 (24 MP) | 2 | 1594.47 | 320.81 | 835.40 |
| 6000×4000 (24 MP) | 7 | 1594.47 | 95.37 | 656.03 |
| 6000×4000 (24 MP) | 16 | 1594.47 | 72.60 | 617.51 |
| 10000×6000 (60 MP) | 1 | 2316.69 | 1222.15 | 2655.88 |
| 10000×6000 (60 MP) | 2 | 2316.69 | 651.54 | 2052.81 |
| 10000×6000 (60 MP) | 7 | 2316.69 | 349.32 | 2112.44 |
| 10000×6000 (60 MP) | 16 | 2316.69 | 353.29 | 1780.19 |

**Quality metric: 0 differing resize bytes in every row.** Editor output is byte-identical
across all four thread counts at every size. Unit checks additionally cover RGBA transparency,
thin/odd sizes, U8/U16/F32 destructive selection mixing, all eleven stack statistics, and
restoration preprocessing/postprocessing with exact bytes or float bits. No quality reduction
was introduced to get the timing improvement. Stack entropy's former randomized sum order is
now deterministic, as documented above.

The probe's peak resident set was **4,799,700 KiB (4.58 GiB)**, wall time **21.07 seconds**, and
**0 swaps**. This includes both reference/new resize outputs and editor documents/history;
it is not the app's steady-state footprint or a measured before/after memory reduction. The
one-pass timings show hardware/thread-count sensitivity and are not release-build benchmark
claims. Source: `crates/pc-engine/examples/gpu_resources_probe.rs`; full raw measurements:
`target/gpu-extra-validation/probe-measure.log`.

### Scope and remaining handoff

All requested implementation points are handled. Shared startup/prefs wiring, model descriptions,
AI enhancement admission and a few export/reference tests were the minimal adjoining changes
needed for the task. The owner-provided task/audit/review documents were not rewritten. No
Match Look source is changed. Logs, screenshots, model fixtures and probe outputs remain under
this worktree's `target/`; nothing is committed. Application-suite screenshots generated
relative to its crate directory were retained at `target/gpu-app-test-artifacts/`.

Physical Apple/NPU validation and a neural ComfyUI pressure stress run remain unmeasured on
this setup; local/remote/unified policy is covered by deterministic tests and real discrete-GPU
recovery. The future shared-device/context/backend extraction is the owner's J12 work, not a
second health implementation or an unfinished slice added to this task.

## References and limits

- [ComfyUI core server](https://github.com/Comfy-Org/ComfyUI/blob/master/server.py): actual
  `/system_stats` device memory and `/queue` shapes; live `/object_info` for the small smoke graph.
- [wgpu Device callbacks](https://docs.rs/wgpu/30.0.1/wgpu/struct.Device.html): device loss and
  uncaptured-error ownership. Existing `pc-gpu::health` is the implementation baseline.
- [Rayon ThreadPool](https://docs.rs/rayon/latest/rayon/struct.ThreadPool.html) and
  [ThreadPoolBuilder](https://docs.rs/rayon/latest/rayon/struct.ThreadPoolBuilder.html): nested work
  uses the current pool, and the global pool cannot be resized. tract-linalg 0.23.8's checked-in
  registry source supplies its thread-local executor override.
- [Linux power-supply ABI](https://github.com/torvalds/linux/blob/master/Documentation/ABI/testing/sysfs-class-power)
  and existing repository macOS subprocess conventions.
- [Lightroom Classic face recognition](https://helpx.adobe.com/lightroom-classic/desktop/organize-photos-in-lightroom-classic/face-recognition.html)
  informs pausable background indexing; no unsupported Adobe CPU-thread claim is made.
- [image-rs 0.25.10 resampler](https://github.com/image-rs/image/blob/v0.25.10/src/imageops/sample.rs),
  MIT OR Apache-2.0: unchanged resize arithmetic and sample order.
- [NASA astronaut fixture](https://scikit-image.org/docs/stable/api/skimage.data.html#skimage.data.astronaut):
  public-domain image downloaded into `target/restore-export/`. Approved ONNX exports are copied
  from the local test-model cache into this worktree; source cache files remain untouched.
- VRAM snapshots are advisory, not an OS allocation guarantee; essential display/transient storage
  is outside cache budgets. Loopback tunnels are conservatively treated as local; network servers
  are independent. Physical macOS/Apple-memory validation is unavailable on this Linux machine;
  parsers and unified-memory budget cases are tested here. Shared-device recovery is J12's follow-up.
