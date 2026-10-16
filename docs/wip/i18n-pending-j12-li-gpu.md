# Pending English strings — J12 shared GPU

Translations remain deferred. No control labels or widgets were added.

| Source | English string | Context |
|---|---|---|
| `crates/li-gpu/src/device.rs` | `waiting for the window's shared GPU device` | Existing GPU fallback/status diagnostics while the shell has reserved ownership but not published its handles. |
| `crates/li-gpu/src/device.rs` | `{adapter} does not support compute shaders` | Compatible window adapter cannot run Develop compute; CPU fallback remains available. |
| `crates/lc-gpu/src/ctx.rs` | `shared device faulted` | Fallback diagnostic when the device is unusable without a driver detail. |
| `crates/li-gpu/src/device.rs` | `device creation failed ({error})` | Shared standalone/CLI initialization failure. |
| `apps/local-image/src/main.rs` | `LIGHTCRAFT_GPU_BACKEND={backend} overrides the backend` | Existing environment-override diagnostic now identifies the legacy variable accurately when it supplies the window and compute choice. |
| `crates/li-gpu/src/device.rs`, `crates/lc-gpu/src/lib.rs`, `crates/pc-gpu/src/lib.rs` | `GPU compute disabled by the shell's CPU rendering policy` | CPU/safe launch blocks compute even if a hardware adapter is needed to draw the window. |
| `crates/pc-ui-egui/src/gpu_canvas.rs` | `GPU acceleration paused for recovery; using the CPU compositor` | Transient compute backoff; CPU pixels remain displayed on the live window device. |
