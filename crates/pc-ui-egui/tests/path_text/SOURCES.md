# Package 12 interaction/render fixtures

`main.rs` uses egui_kittest 0.36.2, actual PointerButton/PointerMoved/Text/Key
events and the real Session command registry. It is an isolated host, not the
full app. `app_acceptance.rs` uses the actual PhotocraftApp canvas/router,
wgpu paint callbacks, zoom 1/2/4/8, HiDPI, detached preview, release and cancel.
It strictly asserts NVIDIA RTX 5090 / Vulkan / discrete / Rgba32Float.
Fixtures and tests here are original Local Image code, GPL-3.0-or-later.

`goldens/abc-circle-input.svg` is a synthetic SVG2 textPath fixture, Inter Regular
20px at 72 DPI, closed cubic ellipse radius 100, startOffset=25%, method=align,
baseline alignment, no tracking/warp. Font hash/license are in
`crates/pc-text/tests/on_path/SOURCES.md`. The input is original GPL-3.0-or-later.
The coordinator must use that bundled OFL font when generating
`abc-circle-outlines.svg`, record Inkscape version/command and output SHA-256,
and put it in `LOCAL_IMAGE_PATH_TEXT_GOLDENS`. No external golden was available
in this branch at task start. The package's corpus gate remains ignored/missing
until that file is supplied, and fails if explicitly run without it.

The existing egui_extras SVG loader exposes the locked resvg 0.45.1 oracle
without a manifest/feature/lock change. It renders the independent converted
golden's paths, not pc-text's exported outlines. Current usvg lacks its optional
text/fontdb graph; feeding it raw textPath would produce empty pixels and is
explicitly rejected. Binary/image corpus data is never fabricated.

Full-app registration is an exact, disposable-tree validation of the shared
hooks provided to 00 in `docs/wip/vector-tasks/integration/12.md`. Production
shared files are not edited by this package. Release pointer-frame measurements
must be taken with the ignored real-app benchmark, never inferred from these
isolated tests or CPU/GPU image parity.

## Integration reference generation

Generated with `/usr/bin/inkscape` 1.4.4 (dcaf3e7d9e, 2026-05-05), batch only,
`--export-type=svg --export-area-page --export-plain-svg --export-text-to-path
--actions=select-by-element:text;object-to-path;export-do` and
`--export-filename=goldens/abc-circle-outlines.svg`. `DISPLAY` and
`DBUS_SESSION_BUS_ADDRESS` were empty. `INKSCAPE_PROFILE_DIR` pointed to an
isolated default profile. `FONTCONFIG_FILE` selected only the repository's
`assets/fonts` directory; `fc-match Inter` returned `Inter-Regular.ttf` / Inter Regular.
The original bundled OFL font and input above were used. The output contains
paths and no text. Output SHA-256: `d5cb6261d7f32672b9a08dcdf671bf66211d94e17603218ca999bfd5bb04af9f`.
Set `LOCAL_IMAGE_PATH_TEXT_GOLDENS` to this `goldens` directory for acceptance.

The integration IoU comparison renders both the independent Inkscape paths and
the native placement at 8x resolution, retaining the package's IoU > 0.97 gate.
The locked resvg/tiny-skia oracle uses 4x4 coverage sampling at each output
pixel; the native renderer integrates coverage. At 256px the few glyph-edge
samples dominate the thresholded mask (observed IoU 0.949748). At 2048px the
same outlines and unchanged font/advance/placement produce IoU 0.991751.
No reference geometry or tolerance was changed. This compares geometry with
less dependence on the two rasterizers' antialias quantization.
