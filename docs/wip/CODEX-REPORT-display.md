# Display colour, dithering and 10-bit output

2026-10-13. Continued the branch's existing research/specification. No commits. HDR and the
Match Look files were left outside this change.

## Implementation

1. **Canvas dithering:** deterministic 8×8 Bayer ordered thresholds in the final GPU display
   pass, after colour/proof/checker/grid composition and before sRGB framebuffer encoding.
   The output uniform carries 255 or 1023 levels; exact 8-bit identity canvas textures bypass
   this stage. The CPU display path evaluates the transform as floats and dithers only at
   quantisation. High-depth identity CPU canvases also dither; 8-bit identity canvases preserve
   their previous bytes. Partial CPU refreshes use document coordinates so their pattern matches
   a full refresh. Exports are unaffected.
2. **Compositor-aware auto:** the native host supplies the actual window backend. macOS auto
   transforms to Display P3 and tags the surface P3; KWin managed native Wayland outputs use
   sRGB and compositor conversion. Detected panel profiles remain visible as information.
   X11/Windows and explicit selections use app-managed transforms. An unconfirmed Wayland
   output defaults to standard sRGB instead of applying an ICC that could cause double conversion.
   Mode is included in monitor-cache validation. An explicit built-in sRGB choice on macOS
   changes the surface tag to sRGB, making the reference comparison valid in both standard spaces.
   Explicit custom ICCs remain deliberate app-managed overrides on the standard tagged surface.
3. **Linux detection:** KWin's JSON first; live `kscreen-doctor -j` when configuration is absent,
   incomplete or ambiguous (KWin retains disconnected displays and reused connector names).
   Only an actual X11 window enables the root ICC atom fallback. A root atom is assigned to its RandR primary output, not copied
   onto unrelated monitors. Then colord via Linux-only
   zbus: match output names, honour device enablement/profiling, and request the selected default
   profile through `GetProfileForQualifiers(["*"])`. Do not assume the first `Profiles` entry is
   the default. ICC size/header checks reject truncated and oversized data.
   Geometry and monitor names are snapshotted through safe winit APIs. I/O runs on one
   background-priority worker with a bounded result channel; startup no longer waits two seconds
   on the UI thread. KScreen has a two-second deadline, and D-Bus method calls have two-second
   deadlines. Focus, display topology changes and opening Color Settings trigger the established
   throttled reread machinery. winit's current output name handles the main Wayland window even
   when its position is unavailable. Failed rereads retain the earlier valid reading.
   The asynchronous detection interface and `AppManaged` environment remain the Windows hook;
   a Windows OS-profile reader is intentionally for the later task. No new unsafe code.
4. **Library/Develop:** `ColorState::srgb_preview_display` supplies the editor's cached display
   transform. The renderer converts final sRGB preview pixels on its existing bounded workers,
   before making egui textures, at Utility priority on native workers. The existing Develop CPU/GPU float-to-byte TPDF dither
   is retained. Its previews remain 8-bit; high-depth 10-bit precision is supplied by the editor's
   float GPU canvas, not reconstructed from already quantised previews. This covers loupe, grid/filmstrip, embedded stand-ins, before,
   comparison and variant slots. Display changes discard stale monitor results and invalidate
   display textures while retaining processing stage caches. Develop's existing soft proof is
   already in the pipeline output; only the final monitor stage is added, so proofing runs once.
   `lc-gpu` does not present a separate surface: its processed pixels reach this same upload path.
5. **10-bit:** the two-file egui-wgpu 0.36.2 patch prefers `Rgb10a2Unorm` only when offered,
   propagates the standard colour-space tag through configure/resize/recreation, and turns off
   egui's fixed 8-bit noise after 10-bit negotiation. Unsupported preferences retain upstream's
   8-bit fallback. `LOCAL_IMAGE_OUTPUT_DEPTH=8` forces the fallback preference for validation.
   The canvas uses float-evaluated `Rgba16Float` display/proof LUTs on 10-bit targets, including
   gamut-warning alpha; high-depth source textures retain their existing half-float precision.
   Preferences › Performance's Graphics information shows actual client surface depth and the
   X11/XWayland tablet trade-off. The tablet dropdown tooltip states that XWayland forces 8-bit.
   Client depth is explicitly distinguished from panel/scanout depth, which the OS controls.

The vendor copy includes upstream dual licence texts, unchanged normalized manifests, an exact
patch and [rebase notes](../../vendor/egui-wgpu/LOCAL-IMAGE.md). Cargo.lock adds no packages: only
local egui-wgpu source selection and direct edges to already-locked pc-engine/zbus changed.
All additions to application code are Rust. Existing settings still load; runtime display state
is not persisted.

## Validation

Full default suites were run once, offline, only for changed crates:

| Crate | Passed | Failed | Intentionally ignored |
|---|---:|---:|---:|
| photocraft-engine | 2109 | 0 | 33 |
| photocraft-ui-egui | 1748 | 0 | 79 |
| lightcraft-ui-egui | 334 | 0 | 6 |
| local-image | 151 | 0 | 6 |
| egui-wgpu | 3 | 0 | 0 |

**Total: 4,345 passed, zero failed.** Three explicitly enabled validation tests also passed
(hardware app timing, display screenshots, connected Linux detection). The screenshot test was
then extended to 1.5× HiDPI and rerun alone. Ignored counts above are the default-suite slow/manual
benchmarks, not missing-adapter results. Clippy passed for every changed crate with `--all-targets
-- -D warnings`; later targeted lint runs covered the final native worker and detection changes.
Formatting and `git diff --check` pass for task-owned changes. The pre-existing task markdown's
trailing blank-line warning was preserved with the owner's specification.

Added checks cover mode decisions/cache changes; KWin/KScreen fixtures; X atom bytes; deterministic
ordered-dither statistics over the whole tile; byte identity; CPU high-depth identity and partial
refreshes; float LUT precision; Library texture equality with the editor transform and monitor
invalidation; Wayland output-name selection; read-only Preferences information with dropdown
clicks; standard-space tags; and negotiation/fallback in the vendored API.

The production canvas hardware readback ran on **NVIDIA GeForce RTX 5090, Vulkan, driver
615.71.09**, with a high-depth source and the real display LUT/final pass:

| Target | Distinct ramp codes | Mean (target 0.5) |
|---|---:|---:|
| Rgba8Unorm | 155 | 0.4999156 |
| Rgb10a2Unorm | 615 | 0.4999924 |

The test rejects software adapters. Vulkan's live native Wayland surface reports
`A2B10G10R10_UNORM_PACK32` with `SRGB_NONLINEAR`; XCB/Xlib surfaces report only BGRA8 unorm/sRGB.
The headless renderer has no native surface, so actual eframe swapchain acquisition/presentation
was not claimed as tested. Negotiation is separately exercised against capability lists, and
8/10-bit production rendering is tested offscreen on hardware. No GUI app window was opened,
and the owner's running app/settings/documents were not changed.

The real editor app path used a 1536×1024, 16-bit document with a P3 display transform:
30 steady frames, including GPU rendering and offscreen readback, measured **11.974 ms median /
13.454 ms p95**, with `gpu-full` and GPU canvas active. This is a debug/development offscreen
measurement, not a claim about desktop presentation or ProMotion deadlines.

The connected Linux test read both live outputs (DP-4 / DP-5), loaded valid ICCs and reported
compositor-managed mode for each. Eight display-info screenshots were rendered and visually
inspected: dark/light × 250/400 pt × 1.0/1.5 pixels-per-point, under `target/design-system/` with
`after-editor-display-output-` names. The 8-bit/tablet note wraps inside the narrow column and
remains readable in both themes. The real read-only information/dropdown interaction test passed.

The full suites also generated unrelated icon/workflow screenshots. Those were retained in
`target/display-output/full-suite-artifacts/`; the tracked icon contact sheet was restored to
its initial contents so it is not part of this task's change.

Logs and generated reference images are retained under `target/display-output/`. Earlier failed
validation logs are preserved there as well. No cache sweeping, toolchain changes or concurrency
changes were made; cargo ran offline with the supplied environment and four build jobs.

## Owner's manual checklist

Use `target/display-output/srgb-reference-patches.png` (8-bit, sRGB-tagged; rows of red/green/blue/
128-gray and #cc8040/black/white/64-gray) and `srgb-16bit-ramp.png` (sRGB-tagged 16-bit ramp).
Keep Proof Colors/Gamut Warning/32-bit preview adjustments off for the reference comparison.
Use 100% zoom, the same display brightness and position, and compare with a colour-managed OS
viewer. Capture each reference rendering once, without changing the document's profile.

### Mac / M4 Max

- [ ] Open the reference PNG in the editor; Monitor Profile = **auto**. Status must report
  Display P3/compositor, with the detected physical panel profile as information. Compare the
  saturated patches with the OS viewer.
- [ ] Render it once with explicit **Display P3**, then once with explicit **sRGB**. Auto/P3
  and explicitly tagged sRGB should have the same physical appearance; their raw framebuffer
  codes differ because the tagged standard spaces differ. This is the double-correction check.
- [ ] Render once with the panel ICC explicitly selected. Status must say manual; this is a
  deliberate override, and can differ because macOS still converts the tagged surface. Return
  to auto for normal use. Do not use this override as the colour-managed reference.
- [ ] Show the same PNG in Library/Develop, grid, filmstrip and loupe. Colours should match
  the editor and OS viewer. Toggle Develop soft proof, then return to the unproofed reference.
- [ ] Move between built-in/external displays; return focus after a system profile change.
  Status must follow the output. Disconnect/reconnect an output and check rereading.
- [ ] Preferences › Performance must report the negotiated client depth. Inspect the 16-bit
  ramp at 10-bit if supported, then restart with `LOCAL_IMAGE_OUTPUT_DEPTH=8` and compare the
  dithered fallback. Check GPU and CPU canvas modes.

### Linux / Plasma Wayland and X11

- [ ] On native Wayland, Monitor Profile = auto. On each managed KWin output, status must
  report sRGB/compositor and the correct detected panel profile. Move the main window between
  outputs, then hotplug/change resolution; status must update without a UI pause.
- [ ] Compare the reference once in auto and once with explicit sRGB: they should match.
  Explicitly select a panel ICC once: status must switch to manual and its different rendering
  demonstrates the potential double correction. Restore auto.
- [ ] Verify Library/Develop loupe, thumbnails and filmstrip agree with the editor. Toggle
  soft proof and check its gamut warning, then return to the reference.
- [ ] Native Wayland should show 10-bit when offered by the driver. Restart with
  `LOCAL_IMAGE_OUTPUT_DEPTH=8`; Graphics information must show 8-bit. Inspect the high-depth ramp
  for deterministic dithering and no broad flat bands.
- [ ] Enable the tablet compatibility mode through the existing tablet launch workflow, restart
  on XWayland and check 8-bit information plus the tooltip. Standard winit 0.30 **does not honour**
  the removed `WINIT_UNIX_BACKEND` variable; for a direct diagnostic launch use `env -u
  WAYLAND_DISPLAY -u WAYLAND_SOCKET ...` to select X11, or the depth override to force 8-bit without changing backend.
- [ ] On a true X11 session, auto must apply a valid per-output ICC from KWin, the ICC atom or
  colord. Test profile changes on two outputs and a missing/invalid ICC fallback. XWayland here
  has no ICC atom and colord has no devices, so those live fallback sources need a populated
  X11/colord environment; their parsers/API usage are covered separately.

## Sources followed

- The existing [research and ordered plan](RESEARCH-display-colour.md), plus its threading policy
  and the repository's design system; safe winit/eframe APIs in the locked registry source.
- [mpv 0.40 ordered dither construction](https://github.com/mpv-player/mpv/blob/v0.40.0/video/out/dither.c)
  for the standard Bayer arrangement; the Rust and WGSL integer expressions are implemented here.
- [wgpu 30 surface colour spaces](https://github.com/gfx-rs/wgpu/blob/v30.0.1/wgpu-types/src/surface.rs),
  and its locked Metal/Vulkan backend source for P3 tagging and the RGB10 surface format mapping.
- [egui-wgpu 0.36.2](https://github.com/emilk/egui/tree/0.36.2/crates/egui-wgpu): format selection,
  runtime surface config, gamma-framebuffer shader and fixed-amplitude dither. The patch uses
  those existing paths, with no replacement renderer or new graphics dependency.
- [colord Device D-Bus specification](https://github.com/hughsie/colord/blob/main/src/org.freedesktop.ColorManager.Device.xml):
  output-name metadata, Enabled and the wildcard default-profile query. The live KWin/KScreen
  structures were inspected read-only rather than guessed; test fixture names/paths are synthetic.

Mac hardware appearance, final panel bit depth and actual swapchain presentation remain manual
checks by design: this task forbids opening GUI windows, and this machine is Linux. Windows
profile detection and HDR are outside the approved scope.
