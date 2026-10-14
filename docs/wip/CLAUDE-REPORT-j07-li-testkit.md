# J7 `li-testkit` report (Claude builder, 2026-10-14)

Status: **done**, one commit on `wip/cl-j07-li-testkit`.

Owner decision applied mid-job ("nothing old needs to be supported"): no thin forwards or legacy
aliases were kept. Every caller was moved to `li-testkit` and the old paths deleted.

## What merged

1. **New crate `crates/li-testkit`** (package `li-testkit`, pure Rust, no `unsafe`; its
   dependencies are workspace crates already in `Cargo.lock`, so there are no new external packages).
   - `li_testkit::softpaint` is the Library's deterministic CPU egui rasteriser, **moved** from
     `crates/lc-ui-egui/src/softpaint.rs`. The code is token-identical. The only diff is a
     whitespace reflow by the workspace rustfmt, because `lc-ui-egui` had its own 150-column
     rustfmt.toml. Its five unit tests moved with it.
   - The `kittest` feature adds `li_testkit::SoftpaintRenderer`, an `egui_kittest::TestRenderer`
     on softpaint. It gives the same frame as kittest's `WgpuTestRenderer`: content rect × the frame's
     pixels per point (rounded), premultiplied 8-bit gamma bytes. The clear colour can be transparent
     (the default), a fixed colour (`with_clear`), or read from the context per frame
     (`with_clear_from(|ctx| Tokens::get(ctx).canvas)`). `Harness::render()` and snapshots work on any
     machine with no GPU and no window. Paint callbacks (custom wgpu canvases) are not drawn, so those
     tests stay on the GPU.
   - The `gpu` feature adds `li_testkit::gpu`, the one strict real-GPU test prelude: `lock()`,
     `hardware_device(label)`, `hardware_device_with(label, desc)`, the `try_*` variants and
     `check_adapter(info, pin)`. The backend is Vulkan, or **Metal on macOS**. The adapter must be a
     hardware GPU, discrete or integrated, so Apple Silicon passes. Software, virtual and unknown
     adapters fail; they are never skipped. `LI_GPU_TEST_ADAPTER=<name>` pins one card (for example
     `RTX 5090`) when a gate must insist on it.
2. **Library (`lightcraft-ui-egui`)**: `pub mod softpaint` is deleted. `headless.rs`, `render.rs`
   and `lib.rs` now use `li_testkit::softpaint` directly.
3. **Editor (`photocraft-ui-egui`) kittest screenshots run on the CPU through one renderer:**
   - `design_system_shots::renderer()` was an RTX-5090-only Vulkan `WgpuTestRenderer`. It is now
     `li_testkit::SoftpaintRenderer`. Its users are the design-system screens, the display-output
     preferences shots and the LaMa model-row screens in `ai_ui.rs`. They now run on any machine
     and are deterministic.
   - **Nine duplicate hand-written `CpuRenderer` kittest renderers were removed.** They lived in
     `clipboard_ui.rs`, `notices.rs`, `rgb_histogram.rs`, `j18_widget_tests.rs`, `tool_tips.rs`,
     `restoration_ui/tests.rs`, `rotate_view/tests.rs`, `generate_ui/draft_refine/tests.rs` and
     `generate_ui/panel_controls_tests.rs`. Each one pulled in softpaint through a
     `#[path = "../../lc-ui-egui/src/softpaint.rs"]` hack in `tool_icon_tests.rs`. Each now calls
     `SoftpaintRenderer::with_clear*` with its own clear colour (canvas, chrome, gray 31 or black).
     The path hack is deleted, and `tool_icon_tests.rs` and `view_camera/tests.rs` use
     `li_testkit::softpaint`.
4. **GPU preludes now use one helper.** `pc-engine/src/gpu_test_support.rs` keeps only
   `compositor_device(label)` and `compositor_device_adapter_limits(label)`: the strict hardware
   device plus the existing `Compositor::preferred_acc_format == Rgba32Float` assertion. Its private
   `lock()` copy is deleted, and all callers use `li_testkit::gpu::lock()`. The audit counted six
   preludes, but there were **31** copies in `pc-engine`: 13 `vector_packages/*/gpu.rs`,
   13 `*/gpu_tests.rs`, `node/tests_gpu.rs`, `affinity/tests.rs`, `pdf_import/tests.rs`,
   `svg_import/tests.rs`, and the LaMa test in `ai_cmds.rs`. All 31 now make one call
   (about 20 lines down to 1 each, about 620 lines removed). The two gates that requested the
   adapter's full limits still do so through `compositor_device_adapter_limits`.

## Parity, quality and speed
- softpaint pixels are identical. The new golden test `testkit_softpaint_golden_unchanged_by_move`
  pins a fixed scene to FNV-1a `0x05cd78568cde9f88`. The scene covers fractional edges, vertex
  gradients, translucent overlap, linear and nearest texture sampling, a clip rect and 1.5× scale.
  The hash was taken from the byte-identical moved file. The test still passes after the rustfmt
  reflow.
- The kittest path equals softpaint: `testkit_kittest_renderer_matches_softpaint` checks that the
  renderer returns exactly the bytes `softpaint::paint` produces for the same egui frame.
  `testkit_kittest_harness_renders_deterministically` checks that a real `Harness` renders on the
  CPU twice with identical bytes.
- Speed: unchanged, because the code is the same. The editor screenshot tests no longer create a
  GPU device.
- Where the old copies differed, the most accurate behaviour was kept. The image size is now
  **rounded**, as in the wgpu renderer and the old j18 copy, instead of truncated as in the other
  eight copies. Only harnesses whose physical size is fractional change. `rotate_view` at 1200×850
  and 1.25× is 1063 rows instead of 1062, and the extra row is the last partial row the GPU also
  draws. No test asserts the image size, and pixels in the shared area are unchanged. The scale is
  the frame's own `output.pixels_per_point`, as all nine copies used.

## TEST-CHANGED
- All 31 `pc-engine` GPU preludes: the hard-coded `info.name.contains("5090")` / `"RTX 5090"`
  check (non-macOS) is replaced by the shared policy. Any hardware GPU on the required backend now
  passes, with the optional `LI_GPU_TEST_ADAPTER` pin. Setting `LI_GPU_TEST_ADAPTER="RTX 5090"`
  reproduces the old exact gate. The non-macOS `DiscreteGpu` requirement became "discrete or
  integrated hardware GPU", which is the rule the macOS branch already had. Virtual and unknown
  device types are now rejected on macOS too. The reasons are the owner rules "must accept Apple
  Metal too" and "never tune to the owner's machines". The backend and `Rgba32Float` assertions are
  unchanged.
- `ai_cmds.rs::lama_real_rtx5090_patch_undo_redo_parity` was Vulkan, discrete GPU and RTX 5090
  only. It now uses the shared policy, so Metal on macOS is accepted, and it gains the
  `Rgba32Float` accumulation assertion; it already built an `Rgba32Float` compositor.
- `colours/gpu.rs` and `envelope/gpu_tests.rs`: the parity assertion messages no longer interpolate
  the adapter info, because the helper prints it to stderr just before. The thresholds are unchanged.
- `design_system_shots::renderer()` no longer requires an RTX 5090. These tests are `#[ignore]`d
  screenshot generators with no pixel assertions, and their ignore attributes are left as they were.
- The nine converted CPU kittest renderers: the size is rounded instead of truncated, as described
  above.

## Tests and checks (minimum, per rules)
- `cargo check --offline -p photocraft-engine -p lightcraft-ui-egui -p photocraft-ui-egui -p li-testkit --all-features --tests`: OK.
- `cargo fmt` on those 4 packages: done.
- `cargo clippy --offline` on the same 4 packages `--all-features --all-targets -- -D warnings`: OK, no warnings.
- New tests only, `cargo test -p li-testkit --all-features testkit_`: **6 passed**. They are the
  golden, kittest = softpaint, harness determinism, HiDPI size, and the GPU gate accepting Apple
  Metal and any hardware GPU while rejecting software and wrong-backend adapters.
- No GPU gates, full suites or benchmarks were run.

## Outside edits
- `Cargo.toml` (workspace): the `li-testkit` dependency entry. `Cargo.lock` changes only for the
  new package and its dependents.
- `crates/lc-ui-egui/{Cargo.toml, src/lib.rs, src/headless.rs, src/render.rs}`: the dependency and
  callers moved.
- `crates/pc-ui-egui/Cargo.toml` (dev-dependency `li-testkit` with `kittest`), plus the 12 test
  files listed in item 3 (test code only).
- `crates/pc-engine/Cargo.toml` (dev-dependency `li-testkit` with `gpu`), `src/gpu_test_support.rs`,
  `src/ai_cmds.rs` (test fn only), and the prelude copies outside `*/gpu.rs` (test code only).

## Left / not done
- `pc-ui-egui/src/gpu_test_support.rs` is another copy of the plain GPU `lock()`, with 141 call
  sites across many actively edited UI files. Moving them to `li_testkit::gpu::lock()` is
  mechanical but has a large merge-conflict surface during this wave, so it is better done at
  integration. `lc-ui-egui`'s lock is different on purpose (re-entrant, with GPU shutdown) and stays.
- Real wgpu harnesses (eframe gates through `wgpu_setup`, canvas paint callbacks,
  `pc-*/tests/*/app_gpu.rs`) cannot move to softpaint.
- The ignored editor snapshot tests were not un-ignored in bulk; there are 192 and most draw the
  wgpu canvas. UI-only ones can now opt in with `.renderer(li_testkit::SoftpaintRenderer::new())`,
  and release hygiene can then drop their known-failures entries.
- `docs/wip/BUILD-design-system.md` still says the design-system shots use an RTX 5090. It is a doc
  outside this job's files.
