# Codex task: Develop layers stop freezing the window (Library/Develop → Compositing)

Written by the opus-scan bug/speed pass (branch `wip/opus-scan`, 2026-10-09). Rules as always: all
Rust, no new dependencies, no `unsafe`, no GUI windows (kittest / headless only), small commits,
`cargo +1.98.1 … --offline`.

## Problem

Switching from the Library or Develop to Compositing on a photo, and coming back from Develop with
new settings, runs the whole full-size develop **on the UI thread**: the window freezes for
seconds. This is the largest interactive stall found in the scan and matches the owner's "laggy"
complaint.

## Evidence (RTX 5090 / Ryzen 9800X3D, 21 MP Sony ARW, `bench_develop_open`)

`cargo run --release -p photocraft-engine --example bench_develop_open -- <file.ARW> [--typical]`
(the example was added by opus-scan; machine was shared with other builds, so numbers are ±20 %).

| Case | No edits | Typical edits (`--typical`: exposure, highlights/shadows, clarity, texture, dehaze, NR, sharpening) |
|---|---|---|
| `develop.openPhoto` (Library → Compositing) | 1.8–1.9 s | **9.2–10.2 s** |
| `develop.syncPhoto` with new settings (Develop → Compositing) | 0.8–0.9 s (was 2.3–2.5 s before opus-scan commit 6f580fca) | **≈ 9 s** |

Stage breakdown (no edits, before 6f580fca): raw decode 1.3 s, CPU develop render 0.7 s,
image → document 0.2 s, flatten 0.23 s, surface conversion 0.26 s. opus-scan already removed the
repeated decode (single-entry decode cache in `pc-io/src/raw.rs`) and the flatten round trip for
re-develops (`plain_layer_surface` in `pc-engine/src/smart_cmds.rs`). What remains is the develop
render itself, on the CPU, on the UI thread.

Why CPU: `photocraft_io::raw::develop_with_settings` (`crates/pc-io/src/raw.rs`) asks for
`OutputDepth::U16` in `OutputSpace::ProPhoto` and calls `lightcraft_pipeline::render`. The GPU
renderer (`lightcraft_gpu::render`, used by the Library via `lightcraft_engine::media::develop`)
only produces 8-bit output (`crates/lc-gpu/src/lib.rs` ~line 357:
`req.depth != OutputDepth::U8 → None`). `render_bench` on the same file: CPU full-size export render
10.8 s wall vs GPU 0.32 s.

## Where in the code

- Host switch: `apps/local-image/src/library_host.rs` `Host::switch` → `Module::Compositing` arm
  → `sync_develop_layers()` and `photocraft_ui_egui::develop_layer::open_photo`.
- UI wrappers: `crates/pc-ui-egui/src/develop_layer.rs` (`open_photo`, `sync`).
- Engine commands: `crates/pc-engine/src/develop_layer_cmds.rs` (`develop.openPhoto`,
  `develop.syncPhoto`, `layer.develop.set`), `crates/pc-engine/src/smart_cmds.rs`
  (`developed_image`, `refresh_layer`).
- Background-job infrastructure to reuse: `crates/pc-engine/src/jobs.rs` (`Session::start_job`,
  `start_open`, `OpenSource`) and `crates/pc-ui-egui/src/jobs_ui.rs` (`start_open`, `finish_open`,
  open tabs with progress). Large file opens already go through it (`file_open.rs::open_path`).

## What to do

1. **Off the UI thread.** Make `develop.openPhoto` a background job like `file.open`: the worker
   reads and develops the file (`develop_with_settings`), the UI thread only builds the Develop-layer
   document from the result (the smart object wrapping in `open_photo`) and opens it. While it runs,
   Compositing shows the opening tab with progress (the existing open-tab UI). Keep the
   "already open → bring forward" path synchronous (it is cheap).
2. Same for re-develops (`develop.syncPhoto` / `layer.develop.set` from the host's
   `sync_develop_layers`): render on a worker; apply the new pixels as one undo step when done; a
   newer settings change cancels/supersedes an older pending one (keep the newest, like the Library
   renderer's request ids). Until it lands the layer keeps its old pixels.
3. **GPU for 16-bit output** (bigger; can be a second commit series): teach `lightcraft-gpu` to
   produce the deep (`OutputDepth::U16`) ProPhoto output the CPU pipeline produces, within the same
   1–3 LSB tolerance documented in `docs/gpu-pipeline.md`, then route `develop_with_settings`
   through `lightcraft_engine::media::develop(&src, &info, &settings, &req, None, true)` (it falls
   back to the CPU itself). This also speeds 16-bit TIFF/PNG/AVIF export (`lc-engine/src/export.rs`
   picks `OutputDepth::U16` for those).
4. Also check: `develop_with_settings` ignores the photo's Raw Processing options (it calls
   `load_bytes` with `RawOptions::default()`), so a photo with non-default raw processing renders
   differently as a Develop layer than in Develop. Pass the photo's options through the settings
   JSON (or the command params) and use `load_bytes_with`; the opus-scan decode cache key must then
   include the options.

## Tests

- Engine: a `develop.openPhoto` job on a `photocraft_raw::testgen` DNG completes and yields the same
  document (pixels, Develop link) as the synchronous command did; a superseded re-develop never
  replaces a newer one.
- UI (kittest, `PhotocraftApp`): the switch returns within one frame (no develop on the UI
  thread — assert via a test hook/counter, not wall-clock), the opening tab shows, and the
  document appears after the job finishes.
- Host (`apps/local-image/src/library_host.rs` tests, e.g. `library_with_photo` + `Host`):
  Library → Compositing → Develop → Compositing round trip still ends with the right layers and
  one undo step per re-develop.
- GPU (if step 3): 16-bit GPU vs CPU within tolerance on the corpus raw, like `tests_gpu.rs`.
- Re-run `bench_develop_open` (and `--typical`) and record before/after in the report.
