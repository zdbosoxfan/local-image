# egui-wgpu 0.36.2 display patch

Source: crates.io `egui-wgpu` 0.36.2, checksum
`5cef1f6d1bba40b09a4d0d62cb822b81030922cfbf65c5373398c3a3caafc5bd`.
Upstream: <https://github.com/emilk/egui/tree/0.36.2/crates/egui-wgpu>.
The upstream MIT / Apache-2.0 notices accompany this copy. No new unsafe code or dependencies.

Only `src/lib.rs` and `src/winit.rs` differ; `local-image.patch` is the exact diff.
The implementation adds about 20 lines, plus API comments and a negotiation test:

- `WgpuConfiguration::preferred_formats` and `framebuffer_format`: choose the first requested
  format in actual surface capabilities. Empty/unsupported preferences retain upstream's selection.
- `SurfaceConfig::color_space`: propagate wgpu 30's standard-space surface tag at every configure,
  including resize and recreation. The default remains `Auto`.
- Disable `RendererOptions::dithering` on `Rgb10a2Unorm`, after negotiation (the upstream shader's
  noise has a fixed 8-bit amplitude). Keep the caller's setting on fallback surfaces.

Local Image requests `Rgb10a2Unorm`, unless `LOCAL_IMAGE_OUTPUT_DEPTH=8`, and requests Display P3
on macOS. `NativeOptions::dithering` stays enabled so negotiation can choose its correct value.
No HDR/fp16 surfaces are requested. The patch does not infer panel/scanout depth from client format.

Rebase: replace the copy with the next locked upstream crate, apply the two-file patch, review API
changes, and run the negotiation test plus the app's display readback tests. Do not format the entire
vendored crate using the workspace's different rustfmt style. The normalized Cargo manifest and
`Cargo.toml.orig` are unchanged. An upstream contribution has not been sent (external messaging
was not requested).
