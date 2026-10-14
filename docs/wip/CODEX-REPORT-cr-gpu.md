# Camera Raw / editor Develop bridge on the GPU

Continued the existing J4 bridge and preview/native worker implementation. Changes are
uncommitted. On entry, only `docs/wip/CODEX-TASK.md` was modified; that file was left alone.

## Implementation

- `pc-io::develop_filter::render_working_scaled` now delegates to the existing
  `lc-engine::media::develop` GPU/CPU dispatcher. `develop_rgba[_scaled]`,
  `develop_surface[_always]` and `camera_raw_surface` all reach it. This covers the
  dialog proxy, native refinement, Apply, smart filters and Lens Correction's Camera Raw stage.
- Added unquantized `F32Linear` output to the existing `lc-gpu` finish shader and readback,
  including adjustment-layer stacks and cached renders in all five output spaces. Both
  `lc-gpu` and the shared dispatcher previously refused this format, which the editor needs
  for document-profile conversion and preservation of floating-point/HDR/gamut residuals.
  Float output decodes the post-grain, unquantized sRGB-curve samples exactly as CPU
  `finish_deep_mode` does; it does not round-trip through an 8/16-bit intermediate.
- Float detail uses the existing high-precision deep-output NR calculations and cache keys.
  The existing 8-bit completion/alpha sentinel stays in the output buffer alongside float
  samples. Admission now accounts for all four words per pixel (preview sentinel + RGB float),
  checks overflow/device limits before dispatch, and preserves CPU fallback for failed work.
- Reused `lc-gpu`'s existing `pc-gpu::DeviceHealth`, error scopes, lost-device callbacks,
  retry/backoff, VRAM-pressure admission, pooled buffers and generation handling. No device,
  health object, backend policy or executor was added to the bridge. This is the small
  extension D13/J12 can incorporate when the existing GPU plumbing moves to `li-gpu`.
- Unsupported full-settings stages follow the Library's existing policy: LUT profiles,
  refined masks and soft proofing use CPU rendering; other existing hybrid stages remain
  hybrid. Disabled/missing GPU, insufficient limits and failed GPU submissions use the
  unchanged CPU reference pipeline automatically.
- Raw smart-object refresh already called `pc-io::raw::develop_with_settings` →
  `lc-engine::media::develop` with GPU enabled and 16-bit ProPhoto output. Retained that
  established route and added a hardware regression proving exact shared-GPU output and
  exact CPU fallback, rather than introducing another raw renderer.

Call-chain evidence: `pc-ui-egui/src/camera_raw_ui.rs` (proxy),
`camera_raw_detail_ui.rs` (native surface), `pc-engine/src/lens_cmds.rs`
(legacy command/Lens Correction stage), `develop_layer_cmds.rs` (raw refresh), and
`pc-io/src/raw.rs` (shared dispatcher).

## Quality and recovery

All hardware checks used **NVIDIA GeForce RTX 5090 (Vulkan)**. No software-adapter or
skipped-adapter result counts as validation. The reference remains the existing CPU
Develop pipeline; these numbers measure CPU/GPU equivalence, not an independent claim
about photographic restoration quality.

The bridge test exercises 19 settings cases at scales 1.0 and 0.25: exposure, tone
controls, WB, vibrance/saturation, parametric/channel curves, HSL mixer, grading,
positive/negative Dehaze, Texture and Clarity, luminance/color NR and detail controls,
sharpening, Grain, Vignette and a combined workload. Each case compares native float GPU
output against CPU output, converts both through the bridge's sRGB profile conversion,
and checks the existing display tolerance (**mean < 0.5 LSB; max ≤ 3 LSB**). It also
checks that the public RGBA bridge returns the GPU float samples and preserves alpha bits.

Across the 38 cases:

- Worst float absolute error: **0.00000876** (Grain).
- Worst float RMSE: **0.00000092** (Grain).
- Worst float mean absolute error: **0.00000061** (combined controls).
- Worst mean display error: **0.000176 LSB**; worst display error: **1 LSB**.

Additional hardware tests cover cached adjustment layers in sRGB, Display P3, Adobe RGB,
ProPhoto and Rec.2020: maximum float error **0.00000852**, mean ≤ **0.00000044**;
repeat cached output is bit-identical. A simulated binding limit that fits RGB but cannot
fit the four-word float output is refused before dispatch without faulting the device.

Bridge fault tests inject limits, dropped work and validation errors: the caller returns
**exact CPU reference pixels** for the failed operation, then GPU rendering resumes via
production automatic retry (no `reset_failures`). A refined mask exercises the dispatcher’s
unsupported-stage CPU policy. A separate new hardware test destroys the test process’s
compute device, observes the actual lost-device callback closing admission, waits for
automatic replacement, and verifies identical rebuilt cached float output and a new context
generation. Native surface/legacy Camera Raw rendering additionally
checks ProPhoto, out-of-range float inputs, identity preservation and exact alpha bits.
Raw smart-object refresh matches shared 16-bit GPU output exactly, and GPU-off refresh
matches CPU output exactly.

## Real-photo interactive latency

Measured the actual `PhotocraftApp` Camera Raw dialog in the existing offscreen
`egui_kittest` eframe harness, with real pointer presses, three successive Exposure drag
positions per workload and a verified Cancel click. The source is the authorized Sony A1
`Capture/_DSC3194.ARW`: **8640×5760 (49.77 MP)**, imported through the app's existing raw
service; the dialog's existing box proxy is **864×576**. The source and settings are the
same in both processes.

“Before” uses `LIGHTCRAFT_GPU=0`, exercising the retained CPU pipeline that the bridge
previously always used; “after” uses the shared GPU dispatcher. Both include the same
profile conversion, clipping/residual handling, histogram and egui texture preparation.
The test observes the dispatcher’s actual backend log and requires **CPU** in the baseline
and **RTX 5090 (Vulkan) GPU** afterwards; a silent CPU fallback fails the GPU measurement.
Raw opening/initial development, kernel warm-up, reference-file I/O and presentation are
outside the timed samples. The frame measurement includes the input frame and the following
render frame (the existing dialog marks dirty after laying out controls). It is an
application-update measurement, not a native display/FPS claim.

Numbers are medians of three actual drag renders per workload, from a single small-sample
run on the owner’s shared machine; load was uncontrolled.

| Workload during Exposure drags | Before CPU preview ms | After GPU preview ms | Speedup | CPU / GPU input + render frames ms |
|---|---:|---:|---:|---:|
| tone | 122.942 | 47.354 | 2.60× | 139.445 / 62.334 |
| dehaze | 107.236 | 48.203 | 2.22× | 123.991 / 64.520 |
| texture_clarity | 186.466 | 45.150 | 4.13× | 202.924 / 61.694 |
| NR | 84.190 | 46.331 | 1.82× | 101.161 / 62.481 |
| grain | 48.431 | 38.724 | 1.25× | 65.317 / 55.066 |

All 15 real-photo GPU drag previews also compare against the CPU run’s captured document-
profile float pixels at identical settings/positions: worst mean **0.00000057**, worst
RMSE **0.00000095**, worst absolute error **0.00013079** (0.0334 of an 8-bit level), exact
alpha bits. These checks include the small CPU/GPU difference in the initial raw import,
not only the filter finish. Native/proxy fidelity therefore remains within the existing
parity tolerance while the selected workloads improve **1.25×–4.13×** on this hardware.
No quality setting was reduced to obtain these timings. No hardware-specific processing
thresholds were introduced; existing device limits, hybrid stages, memory pressure,
recovery and CPU fallback adapt to other hardware.

The first JPEG candidate exceeded the existing flat decoder’s 33.5 MP guard; the raw test
photo uses the existing raw import route instead. Harness setup then required emulating a
4096-pixel native texture limit for the unrelated main-canvas proxy (the offscreen context
otherwise defaults to 2048), waiting the dialog’s next render frame, and distinguishing its
footer Cancel from the underlying colour-policy Cancel. No production limits or pixel/
revision assertions were weakened. Failure logs are retained in `target/cr-gpu-ui-{jpeg-limit,
headless-limit,revision,cancel-query}.log`; the final baseline/GPU runs both pass.

## Compatibility and scope

No settings/schema/serde fields, crop/filter sanitization rules, profiles, cancellation,
revision checks or native/proxy size limits changed. Existing catalogs, settings and PSD
Camera Raw descriptors still use their original loaders/defaults. Existing profile
conversion, gamut/HDR residual handling and alpha preservation remain in the bridge.
No production UI controls, layout, widgets, icons or text changed, so no new translation
strings or design-system exceptions were needed. The new UI test uses the existing
accessible Exposure slider and Cancel button. Match Look files were not touched.

No dependency, manifest, Cargo.lock, C/-sys or unsafe-code additions. The bridge uses its
existing `lightcraft-engine` dependency; tests use already-present workspace dependencies.
No owner GUI was opened or killed; test harnesses are offscreen. The owner image was read
only from the authorized test-shots directory. All derived reference pixels remain in
ignored `target/cr-gpu-reference/`; no photo/derived image was committed or uploaded.

## Validation

Followed the task's final **ABSOLUTE MINIMUM TESTING** override: only new tests,
filtered by name, plus check and strict clippy. No full crate/workspace suites,
GPU acceptance suite, benchmark runner or long quality sweep was run.

All commands used `PATH="$HOME/.cargo/bin:$PATH"`, `cargo +1.98.1`, offline builds,
the retained worktree `target/` and the launcher's unchanged `CARGO_BUILD_JOBS=4`.
Only one cargo build/test/check/clippy process ran in this job at a time.

| Final validation | Result |
|---|---|
| `check --offline` with `-p lightcraft-gpu -p lightcraft-engine -p photocraft-io -p photocraft-ui-egui` | Passed |
| `clippy --offline` on the same four crates, `--all-targets -- -D warnings` | Passed |
| `test --offline -p lightcraft-gpu --test cr_float cr_gpu_float_output_spaces_layers_and_limits -- --ignored --nocapture` | 1 passed |
| `test --offline -p lightcraft-gpu --lib cr_gpu_float_device_loss_recovers_cached_deep_output -- --ignored --nocapture` | 1 passed |
| `test --offline -p photocraft-io --lib cr_gpu_ -- --ignored --nocapture --test-threads=1` | 2 passed |
| `test --offline -p photocraft-ui-egui --lib cr_gpu_real_photo_preview_drag_latency -- --ignored --nocapture`, CPU baseline | 1 passed |
| Same UI test, real GPU + saved CPU reference comparison | 1 passed |
| `fmt` on the four changed crates, final `--check` | Passed |
| Implementation/report diff whitespace check | Passed |
| Manifest/Cargo.lock/unsafe/dependency audit | No additions/changes |

**5 distinct new tests**, **6 successful final test executions** (the UI measurement has
CPU/GPU modes), 0 failures in the final runs; all 5 tests have real-hardware GPU coverage.
They are opt-in/ignored in ordinary suites and fail rather than claim a hardware result
when a real GPU is unavailable. Existing tests were not modified or rerun. The initial
bridge test also passed before native surface coverage was added; its final expanded
version passed with the raw-refresh test. The report records the UI setup failures above;
those logs were preserved and their causes corrected without weakening assertions.

Final logs: `target/cr-gpu-check-final.log`, `cr-gpu-clippy.log`, `cr-gpu-float.log`,
`cr-gpu-device-loss.log`, `cr-gpu-io-final.log`, `cr-gpu-ui-before.log` and
`cr-gpu-ui-after.log`. The existing optional craft-fonts build-script warning appears in
build logs; strict Rust/clippy validation passed.

No implementation work remains in this task's scope. Integrated/full-suite validation is
left to the coordinator as the task explicitly requires. No commit, push, upload, owner
file mutation or GUI window was made; HEAD remains `b82273a1`.

## Sources followed

- Existing shared dispatch and CPU float contract: `lc-engine/src/media.rs`,
  `lc-pipeline/src/finish.rs::finish_deep_mode`, `lc-gpu/src/{lib,ctx,render,params}.rs`,
  `lc-gpu/src/wgsl/finish.wgsl`, and `pc-io/src/develop_filter.rs`.
- Existing parity/readback/cache conventions: `lc-gpu/tests/it/equivalence.rs`,
  `lc-gpu/tests/fallback.rs`, `lc-gpu/tests/support/mod.rs`, and
  `docs/upstream/lightcraft/gpu-pipeline.md`.
- Recovery/device sharing: `pc-gpu/src/health.rs` and D13/J12 in
  `docs/wip/AUDIT-duplicate-systems-fable.md`.
- [wgpu 30.0.1 Device documentation](https://docs.rs/wgpu/30.0.1/wgpu/struct.Device.html):
  retained the existing error scopes, uncaptured-error handler and device-loss callback
  rather than inventing another recovery mechanism.
- [Adobe Camera Raw GPU FAQ](https://helpx.adobe.com/camera-raw/desktop/troubleshoot/acr-gpu-faq.html):
  GPU processing accelerates interactive adjustments and rendering when the filter opens/closes;
  hardware capability and competing workloads affect acceleration. Local Image retains its
  requested CPU editing fallback even on hardware without GPU support.
- Existing J4 dialog/harness tests and `docs/DESIGN-SYSTEM.md`; processing changed under
  the existing shared controls, without changing UI design.

Float output layout/readback is the small project-specific extension; its numerical
contract comes directly from the existing CPU finish implementation.
