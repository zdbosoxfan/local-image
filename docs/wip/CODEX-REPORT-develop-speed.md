# Develop slider responsiveness

Continued the previous builder's work in this branch. No commits, dependency changes,
unsafe Rust, shader-math changes, or golden updates. No desktop app was launched or
stopped, and no owner photo/settings/library files were changed.

## 1. Drag path and fixes

The actual path is `widgets::slider` → `edit::apply_slider_out` → engine
`develop.beginInteraction` / `develop.set` / `develop.endInteraction` →
`detail::show` → `Session::loupe_job` → `Renderer::request` → `JobPool` worker →
`RenderJob::run` → `media::develop` → GPU compute or CPU pipeline →
`Renderer::poll` → egui texture delta. The desktop host consumes that delta to upload
the displayed texture. Live settings change in memory, with one history/undo entry
at release. Rendering and GPU readback run on workers, not in the UI layout.

Fixed the following:

- Kept the builder's per-slot running-job tracking. One job runs per slot and the
  newest queued job replaces intermediate requests. Other slots still use idle
  workers. Kept the running guard through result delivery so completions for a slot
  remain ordered; panic unwinding releases the slot too.
- The old draft request was 60% of the displayed image, capped only by the settled
  preview preference. At 100%/200% zoom or on HiDPI displays this could request an
  enormous image and choose `SourceLevel::Full`. Drafts now have an independent
  **1280-pixel long-edge cap**, use preview sources, and retain `Quality::Draft`.
  Release requests `Quality::Full` at the normal display/zoom/preview-preference size.
  Full-size exports remain unchanged.
- Deferred thumbnail requests for the photo being edited until release. Previously
  the filmstrip rendered transient settings on the CPU and cached them while the
  loupe was also rendering.
- The second window shares the main loupe's live preview during a Develop drag,
  then renders at its own settled size. If the main window shows the grid, the
  second window renders its own bounded draft. It now keeps its stage cache and
  coalesces newer requests instead of waiting for an obsolete request to finish
  before even requesting the newest settings.
- Shift-drag now accumulates unrounded movement. Previously each tiny pointer delta
  was rounded independently, making fine adjustments stick. The release event's
  final position is applied within the same undo step. Disabled sliders' labels
  cannot reset values.
- A hidden slider or lost focus cannot leave an interaction stuck in draft mode:
  release commits the interaction even when the widget disappeared.
- Selection, Undo or Cancel commands stop a held slider gesture when they end its
  session interaction. The old gesture cannot subsequently write to another photo.
- Late advancing previews must match the requested photo and source content;
  another photo's render or a pre-reload source can no longer replace it. A late
  render with a repeated key cannot clear a newer request's pending state.
- An advancing superseded preview also supplies its decoded source to the validated
  source cache. A cold, uninterrupted drag no longer keeps decoding until release.
- Applying the GPU preference now flips the flag directly and warms the device on a
  worker. Previously it called the synchronous `app.gpu` diagnostic query on the UI
  thread, which initialized the device before the intended background warmup.

Native GPU stages already retain uploaded sources and amount-independent fields.
The renderer still reads back one completed RGBA8 output and gives egui its pixels;
that architecture was traced, not replaced. The fixed draft cap bounds the copy and
upload size, and stale jobs are coalesced before execution.

## 2. Backend visibility

`media::develop` logs **`Develop render backend: GPU: <adapter> (<backend>)`**
after a successful GPU render, or **`Develop render backend: CPU: <reason>`**.
Reasons include the GPU preference/environment, device/adapter failures, render
fallback, LUT profiles, mask refinement, soft proofing and high-bit-depth output.
Logs are emitted when the backend/reason changes rather than for every step.

The native host already defaults its Library/Develop GPU preference to enabled.
`apps/local-image/src/app_log.rs` already includes the `lightcraft` target prefix
in `INFO_TARGETS`; that existing correct filter was retained. It therefore includes
these engine messages and the GPU device messages in the normal application log.
The new engine integration test captures the actual info messages and checks the
adapter, CPU soft-proof reason and disabled-GPU reason.

## 3. Headless CPU/GPU timing and regressions

`tests_slider_speed.rs` extends the builder's continuous-drag benchmark and adds
automatic tests that drive real egui pointer presses, moves and releases through
Library/Develop's `Headless`, the session, worker queue, renderer and texture manager.
They use a deterministic procedural photo with nominal dimensions 6000×4000 and
exercise Exposure, Highlights, Clarity and Saturation, reporting every step's UI,
render-job and change-to-texture time, then the release-to-full time. They assert
pointer tracking, changed pixels, current request keys, one undo step, bounded
preview size, settled full-quality keys, and actual GPU stage storage (or none for
the CPU run). Additional UI assertions cover fine movement, a disappearing widget,
200% zoom at 2× HiDPI, thumbnail deferral, second-window sharing and changing photos
while a slider is held.

The GPU variant normally skips without an adapter for CI; `LC_REQUIRE_GPU=1` makes
that a failure. Existing numerical GPU tests previously forced software Vulkan;
the test device now prefers hardware, and the same strict flag prevents software
fallback/skipping in those numerical tests. This is test setup only.

GPU coverage added: `develop_slider_gpu_step_timings` (automatic UI-to-GPU timing
and output assertions), `develop_slider_drag_latency` (the builder's ignored
continuous benchmark, extended and run explicitly), and the engine integration
test `develop_backend_logs_actual_adapter_and_cpu_reasons`. The nine existing
`primary::tests::native*` numerical tests also ran on the RTX, with their original
CPU-equivalence bounds unchanged.

Final automatic timings on **NVIDIA GeForce RTX 5090 (Vulkan)**, optimized test
profile, six steps per control, 861×574 drafts and 1435×956 settled output:

| Control | CPU step mean (ms) | RTX step mean (ms) | RTX step p95 (ms) | RTX release-to-full (ms) |
| --- | ---: | ---: | ---: | ---: |
| Exposure | 58.02 | 15.00 | 27.04 | 16.73 |
| Highlights | 1136.46 | 16.51 | 46.59 | 40.74 |
| Clarity | 653.86 | 16.86 | 42.39 | 60.14 |
| Saturation | 122.53 | 14.64 | 22.43 | 11.14 |

These change-to-texture times include cold field preparation and headless polling;
individual warm GPU jobs are generally about 3–8 ms. They are observations on a
shared host, not a fixed performance threshold or a before/after comparison. CPU
spatial primary processing remains expensive (the CPU clarity release took 3.70 s),
but it runs on workers, uses preview output size, and cannot accumulate an unbounded
queue. These tests measure texture
readiness including GPU readback and egui texture staging, not desktop presentation
or the window renderer's final GPU upload. No software-adapter timing is claimed.

The continuous 120-move landscape benchmark also ran on the RTX with a 3840×2160
viewport at 2× scale and Highlights −40, Shadows +30, Clarity 15, Texture 10,
Dehaze 10, luminance NR 30, colour NR 25 and sharpening 40. It applied 113 distinct
exposure values over 124 UI frames in 2.13 s. Draft textures stayed at **1280×853**;
the settled output was **2400×1599**. It displayed 47 updates (22.1/s), with mean
UI frame 2.5 ms / p95 6.3 ms / maximum 13.4 ms. Render jobs averaged 40.8 ms
(p95 55.7 ms); change-to-texture averaged 62.5 ms (p95 80.7 ms). Release-to-full
was **127 ms**, including an 83.6 ms final job. This heavier combined-edit case
rebuilds dependent fields when exposure changes, unlike warm amount-only drags.
The earlier portrait demo benchmark passed too (58 updates / 27.1 per second).

### Reproduction

All builds: `PATH="$HOME/.cargo/bin:$PATH" CARGO_BUILD_JOBS=3`, Rust 1.98.1,
`--offline`, one Cargo build at a time. Hardware validation also uses
`LIGHTCRAFT_GPU_BACKEND=vulkan LC_REQUIRE_GPU=1 RUST_TEST_THREADS=1`.

```sh
cargo +1.98.1 test --offline -p lightcraft-ui-egui tests_slider_speed -- --nocapture
cargo +1.98.1 test --offline -p lightcraft-engine --test develop_backend -- --nocapture
# Continuous 120-step drag; omit FILE for the procedural landscape photo.
LC_SLIDER_BENCH_FILE=/path/to/photo.arw LC_SLIDER_BENCH_PPP=2 \
  cargo +1.98.1 test --offline -p lightcraft-ui-egui \
  develop_slider_drag_latency -- --ignored --nocapture
# Optional LC_SLIDER_BENCH_CONTROL, LC_SLIDER_BENCH_SETTINGS and LC_SLIDER_BENCH_SIZE.
# LIGHTCRAFT_GPU=0 selects CPU for the continuous benchmark (omit LC_REQUIRE_GPU).
```

## Final validation

Ran the full suites of the four changed crates once, unfiltered, with the hardware
environment above. Assertion counts (unit + integration tests):

| Crate | Passed | Failed assertions | Ignored |
| --- | ---: | ---: | ---: |
| `lightcraft-engine` | 679 | 0 | 3 |
| `lightcraft-gpu` | 86 | 0 | 5 |
| `lightcraft-preview` | 31 | 0 | 0 |
| `lightcraft-ui-egui` | 215 | 0 | 2 |
| Total | **1011** | **0** | **10** |

Doc tests passed (zero doctests in these crates). Existing golden, equivalence,
fallback, memory and interaction assertions passed without changing goldens or
tolerances. All adapter reports identify **NVIDIA GeForce RTX 5090 (Vulkan)**;
there are zero missing-GPU skips and no llvmpipe results. The ignored cases are
performance/diagnostic runs and a people-model test needing external weights.
The slider benchmark is ignored in the normal suite but was run explicitly twice
as described above. A pre-existing Japanese-watermark test reported missing optional
Craft fonts; no fonts or models were downloaded.

The full command was:

```sh
PATH="$HOME/.cargo/bin:$PATH" CARGO_BUILD_JOBS=3 \
  LIGHTCRAFT_GPU_BACKEND=vulkan LC_REQUIRE_GPU=1 RUST_TEST_THREADS=1 \
  cargo +1.98.1 test --offline --no-fail-fast \
  -p lightcraft-engine -p lightcraft-gpu -p lightcraft-preview \
  -p lightcraft-ui-egui -- --nocapture
```

**Process-exit incident:** the initial combined command returned 101 because the
engine unit-test process segfaulted after printing `417 passed; 0 failed; 3 ignored`.
The core trace places the fault in `libnvidia-gpucomp` on a shader-compiler thread
while the main thread was in the driver's libc exit handlers. It shows no test
assertion failure. The other test targets and all doctests exited successfully.
Reran only `-p lightcraft-engine --lib` with the same hardware environment:
**417 passed, 3 ignored, exit 0**, in 143.60 s. Every changed-crate test target has
therefore been observed exiting successfully. No Rust source change was made to
hide the incident, and no driver, system runner or owner application was changed.
The driver exit fault did not recur on this rerun; it is not claimed fixed by the
slider changes. Evidence is in `target/develop-speed-tests-final.log`,
`target/develop-speed-engine-exit-recheck.log` and
`target/develop-speed-engine-exit-core-info.log`.

During development, focused runs passed: five automatic slider/timing regressions,
15 renderer identity/cache tests, six slot-pool tests, the one new backend-log test,
and nine native primary GPU numerical tests on the RTX. The benchmark logs are
`target/develop-speed-continuous-landscape-gpu.log` and
`target/develop-speed-continuous-gpu.log`.

Clippy passed for all four changed crates and all targets, with warnings denied:

```sh
PATH="$HOME/.cargo/bin:$PATH" CARGO_BUILD_JOBS=3 CARGO_TARGET_DIR=target/clippy \
  cargo +1.98.1 clippy --offline -p lightcraft-engine -p lightcraft-gpu \
  -p lightcraft-preview -p lightcraft-ui-egui --all-targets -- -D warnings
```

Log: `target/develop-speed-clippy-final.log`. `cargo +1.98.1 fmt` with `--check`
passed for those four crates, and `git diff --check` passed. The remaining
performance/presentation limits are described in section 3. Changes are left
uncommitted in this worktree.
