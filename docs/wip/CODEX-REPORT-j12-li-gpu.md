# J12 — one GPU device, health service and WGSL helpers

Continued the GPU coordination and Camera Raw GPU work already present at `c330f913`.
On entry only `docs/wip/CODEX-TASK.md` was dirty; its supplied changes remain untouched.
All implementation changes are uncommitted.

## Consolidation

- Added the pure-Rust workspace crate `li-gpu`. The shell reserves device ownership before
  initialization and publishes eframe's instance, adapter, device and queue before Library
  workers start. Develop, AI denoise and the compositor use those same handles. Headless
  callers use the same service; a window failure never starts a second private compute device.
- The shell requests the hardware adapter's limits, including Develop's ten storage bindings.
  GL retains egui's downlevel limits and large texture/buffer support; unsuitable compute or
  software adapters keep the CPU reference path. `LIGHTCRAFT_GPU_LIMITS` remains a compute cap.
- `pc-gpu::health` and `lc-gpu::backend` are thin reexports. One health object owns callbacks,
  error counts, strikes, decay, retry timers and idle-VRAM observations. A stale successful
  operation cannot erase another consumer's failure backoff. Every failure schedules backoff,
  while errors in the same frame still count as one strike.
- Recoverable strike faults resume on the same device after backoff. The canvas retains its
  presentation path and displays freshly uploaded CPU results while compute is paused. Only
  genuine device loss removes the canvas. Replacement identities include the instance and a
  service ID; old callbacks and pooled buffers cannot contaminate the replacement.
- Failed initialization retries instead of permanently caching `None`, and a newly published
  shell device bypasses the predecessor's initialization failure. Pool eviction uses guarded
  polling. Shutdown drains the window queue even when Develop kernels were never initialized,
  then releases the service before native driver exit handlers.
- CPU/safe launch policy also blocks compute when hardware is still needed to draw the window.
  Both compute stacks honor it. Existing preferences, backend recovery chains, marker JSON and
  legacy compute-off APIs remain supported. `WGPU_BACKEND` keeps shell priority; valid legacy
  `LIGHTCRAFT_GPU_BACKEND` values configure that same instance rather than another device.
- One locked `StartupSentinel` implements startup marker I/O. The shell wraps it with its
  existing marker model; legacy standalone marker APIs adapt to it. Live initializers' files
  remain untouched, reads are bounded to 64 KiB, and damaged UTF-8 remains crash evidence.
- `li-gpu/wgsl/common.wgsl` supplies scalar/vector sRGB transfer, neutral TPDF dither, explicit
  output-code rounding and the existing analytic Gaussian shadow coverage. Develop, compositor
  and canvas include it. Processing kernels, grain, Dissolve and gradient-fill noise retain
  their own established CPU references. CPU sRGB comes from `li-color`; Develop's existing CPU
  `finish::dither8` remains the byte-golden reference.
- The compositor's public `SHADER: &'static str` API remains intact: a small Rust build script
  assembles its shared source under Cargo's target directory. Browser handles use a thread-local
  registry rather than requiring wgpu's optional fragile Send/Sync behavior.

## Measured quality

All hardware samples used **NVIDIA GeForce RTX 5090 / Vulkan**, without software fallback or
successful adapter skips. Inputs were synthetic ramps and the repository's demo scene.

| Sample | Before / reference | Shared result |
|---|---|---|
| Develop device handoff, 192×128 | Direct kernel context on the same device | **0 changed preview bytes** |
| Develop exposure sample vs CPU | Existing parity gate: mean <0.5, max ≤3 LSB | Mean **0.000027**, max **1 LSB** |
| WGSL dither, 4,096 real GPU samples | Existing CPU `finish::dither8` / position hash | **Exact byte and TPDF equality** |
| sRGB scalar/vector, including negative/threshold/HDR values | CPU transfer reference | Max error **0.000007629** |
| Analytic Gaussian coverage | Existing shadow arithmetic | Max error **0.000000119** |
| 10-bit pre-rounding code | CPU TPDF expression | Max error **0.000122070 code** |
| Half-float encode, each output format | Original vector helper shader | **4,080 nonwhite texels bit-exact**; white **0.9995117 → 1.0** |
| Final 8-bit output | Old Bayer/blend native nearest-code error: mean **0.034667969**, max **1 code** | **Exact CPU output codes**, neutral RGB and endpoints |
| Final 10-bit output | Old Bayer/blend native nearest-code error: mean **0.033691406**, max **1 code** | **Exact CPU output codes**, neutral RGB and endpoints |
| 128×128 noise spatial sample, lag-8 correlation | Bayer **0.08331299** | TPDF **0.00110304**: **98.68% less periodic correlation** |

TPDF has more noise power: measured variance **0.16689232**, versus Bayer's **0.08331299**;
its mean is **−0.00105999**. The quality choice preserves Develop's existing neutral TPDF
behavior and removes the repeating tile; it does not claim lower noise variance.

The real final-pass test uncovered two precision details. Half-float target conversion can
choose either neighboring value, so an arbitrary half-ULP oracle was replaced with exact
old/new encode parity and a one-storage-code CPU bound. The old blended final pass also
stored code 3 for a fragment code 3.5234375. The final shader is always opaque, so its tile
and Move final passes now store directly; shadows and transparent intermediate planes still
blend. Shared dither explicitly chooses the nearest code and preserves exact endpoints.
These intentional changes improve white fidelity and final quantization; the new output-code
oracle remains exact, with no tolerance added.

## Interactive path and timing

The small samples execute the production Develop API with its stage cache and the production
canvas encode / `CanvasCallback::prepare` / paint paths, on hardware without opening a window.
They are not whole-app or export benchmarks, and they do not establish a general speedup.

- Develop 192×128: first cached render **0.441 ms**; changed Exposure **0.237 ms**.
- Final canvas 256×16: three warm frames per mode, CPU submission plus GPU wait, excluding
  copy/readback. The before sample changes the final dither/blend slice to the old Bayer pass.

| Output | Before median frame + GPU wait | After median frame + GPU wait | Before median copy/readback | After median copy/readback |
|---|---:|---:|---:|---:|
| 8-bit | 0.052939 ms | 0.033880 ms | 0.034199 ms | 0.022370 ms |
| 10-bit | 0.047240 ms | 0.044790 ms | 0.027089 ms | 0.021950 ms |

These tiny samples are noisy and do not establish overall speed on other images or hardware.
Device ownership goes from **two independent runtime devices/queues to one shared pair**;
the test checks actual device and queue identity. No total-VRAM byte reduction is claimed.
Existing buffer/texture caches and the GPU coordination job's pressure governor remain.
`Rendered` CPU results and document/color-conversion readbacks retain their public contracts;
this task does not claim a new zero-copy document conversion path.

## Validation

- Offline `cargo check` passed for `li-gpu`, `lightcraft-gpu`, `photocraft-gpu`,
  `photocraft-ui-egui`, and `local-image`.
- Focused new tests: **17 passed** (no ignored/skipped cases): `li-gpu j12_` **7**;
  `li-gpu shared_wgsl_` **3**; `lightcraft-gpu --lib j12_` **2**;
  `photocraft-gpu --lib j12_` **1**; `photocraft-ui-egui --lib j12_` **2**;
  `local-image j12_` **2**.
- The canvas recovery test opens the circuit with three failed frames, runs the real app GPU
  check and CPU refresh, verifies **4,096 exact CPU white callback pixels** while compute is
  paused, then retries on the same device. Device loss still blocks presentation.
- Strict `cargo clippy --all-targets -- -D warnings` passed for all five affected crates.
- Package-scoped `cargo fmt -- --check` passed for all five affected crates; the changed diff
  is whitespace-clean. The supplied task file retains its pre-existing trailing blank line.
- Logs: `target/j12-{check,clippy,service-tests,wgsl-tests,develop-tests,compositor-tests,canvas-tests,shell-tests}.log`.
  The compile-time font warning about missing Chinese/Japanese embedded fonts is pre-existing.

All cargo operations use `+1.98.1`, `--offline`, the same retained target directory and the
launcher's `CARGO_BUILD_JOBS=4`. No full crate/workspace suites, GPU acceptance commands,
benchmarks or long quality sweeps were run, following the task's overriding testing budget.

TEST-CHANGED: Existing canvas shader-validation code now assembles the shared prelude before
Naga validation. Its validation flags and acceptance conditions are unchanged.
TEST-CHANGED: The new J12 final-pass test's legacy-only native conversion oracle now accepts
only Vulkan's permitted adjacent integer and records its error against nearest. The new
TPDF integer oracle, neutral RGB and black/white endpoint checks remain exact. The new encode
oracle verifies old/new nonwhite parity and the deliberate exact-white improvement.

Failure evidence is retained under `target/j12-service-first-failure.txt` and the three
`target/j12-canvas-tests-*-failure.log` files. The first service failure exposed missing
same-frame backoff; implementation was corrected. Canvas failures isolated half storage
rounding and native final conversion, rather than weakening the new quantization gate.

## Scope and compatibility

Unavoidable edits outside the task's original file list:

- Workspace/Cargo manifests and `Cargo.lock`: wire the shared crate. The **only added lockfile
  package is local `li-gpu`**; no external or C/`-sys` package was added or upgraded.
- `apps/local-image/src/main.rs`: publish the shell handles, accept the legacy backend override,
  and release the service after shutdown.
- `lc-gpu/src/wgsl/{common,finish}.wgsl`: remove duplicate transfers and adapt output dither.
- `pc-gpu/src/{lib.rs,compose.wgsl}` and `pc-gpu/build.rs`: include shared helpers while retaining
  the constant shader API, honor shared compute admission, and guard stale successes.
- `docs/wip/i18n-pending-j12-li-gpu.md`: defer new diagnostic translations as instructed.

No control/layout changes or new UI widgets were made; the canvas tests exercise actual GPU
painting and CPU fallback presentation. No Match Look files, catalogs, settings or owner
photos were touched. No GUI windows, ComfyUI jobs, commits, pushes or uploads were created.

Genuine device loss remains terminal for that device. The service accepts a replacement and
isolates its predecessor, but current eframe window recovery uses the existing restart flow;
it cannot revive a destroyed device. Windows, macOS and browser builds were not run on this
Linux host (only the native Rust target is installed). Broad acceptance remains the merge
coordinator's work under the owner's explicit testing limit.

## Established references

- Installed/vendored **wgpu 30.0.1** and **egui-wgpu 0.36.2** API sources: cloneable RenderState
  handles, requested adapter limits, single lost/error callbacks, and instance-local device IDs.
  [wgpu Device API](https://docs.rs/wgpu/30.0.1/wgpu/struct.Device.html),
  [egui-wgpu RenderState](https://docs.rs/egui-wgpu/0.36.2/egui_wgpu/struct.RenderState.html).
- Existing Develop CPU `finish::{DITHER_HASH,dither8}`, `li-color` transfer math, and original
  canvas shadow arithmetic provide the compatibility references. The sampled
  `~/.local/share/local-image-dev/engine-sources/dt/src/iop/dither.c` also uses one TPDF sample
  across RGB. [darktable dither](https://github.com/darktable-org/darktable/blob/master/src/iop/dither.c).
- Exact normalized transfer endpoints follow [W3C sRGB / IEC 61966-2-1](https://www.w3.org/Graphics/Color/srgb).
- The measured half-float/native conversion distinction follows the primary
  [Vulkan conversion rules](https://docs.vulkan.org/spec/latest/chapters/fundamentals.html#fundamentals-fixedfpconv)
  and [blending precision rules](https://docs.vulkan.org/spec/latest/chapters/framebuffer.html#framebuffer-blending).
- Marker locking follows [Rust File::try_lock](https://doc.rust-lang.org/std/fs/struct.File.html#method.try_lock).
  No Lightroom/Photoshop/Affinity/Capture One UI behavior needed a new interpretation here.
