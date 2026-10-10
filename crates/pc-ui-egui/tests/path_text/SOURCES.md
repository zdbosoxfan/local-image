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
