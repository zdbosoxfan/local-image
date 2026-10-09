# Glossy Duotone tool icons

Implemented 2026-10-09. Changes remain uncommitted in the existing branch.

## Artwork and coverage

- Added 93 original hand-authored SVGs in `assets/icons/color/`, using a 24 × 24 viewBox, family gradients, neutral handles, dark outlines, and restrained highlights. The reference boards supplied style direction; their raster artwork is not embedded or traced.
- Covered all 53 Compositing tools, including every grouped tool; all 15 Liquify variants (11 strip tools and four global actions); Edit Toolbar and Quick Mask; and Library/Develop's tool strip, retouch modes, mask types, crop helpers, and point-colour picker.
- Tools that formerly shared monochrome glyphs now have distinct colour artwork, including Blur/Smooth, Smudge/Forward Warp, Healing/Spot Healing, vertical/horizontal Type, and individual selection, shape, and mask tools.
- `docs/design/tool-icons.png` shows every icon at actual 24 and 48 pixel sizes on `#1f2023` and `#f2f2f4`. It was rendered with resvg by the Rust contact-sheet test and visually inspected. Regenerate with `PATH="$HOME/.cargo/bin:$PATH" CARGO_BUILD_JOBS=3 CARGO_NET_OFFLINE=true cargo +1.98.1 xtask tool-icons`.

## Rendering and preferences

- A separate embedded colour table and shared Rust renderer preserve SVG colours without tinting. Textures are cached by name, logical size, display scale, and disabled state within each app's egui context. Disabled colour tools are desaturated and dimmed to 45% alpha.
- Existing selected/hover button backgrounds remain visible. Generic UI icons retain the existing monochrome renderer. Toolbar changes are localized to tool drawing helpers and names.
- Develop retouch-mode icons have namespaced widget IDs, with click tests checking that the separate Remove strip button still toggles its existing spot-overlay state.
- Both apps expose Tool icons: Colour (default) / Monochrome in Interface preferences. Serde defaults keep older settings loadable; saving, restoring, resetting, and switching back are tested.
- Added the new labels to all 12 Compositing translation catalogs and all four Develop catalogs. Existing translation-completeness checks pass.
- `pc-ui-egui::icons::tool_icon(ui, name, size)` returns an image for toolbar and rich-tooltip slots. Develop exposes the same helper, returning `None` for its original vector-glyph fallback when monochrome is selected.
- Reused the existing workspace `egui_extras` SVG feature for Develop's usvg/resvg rendering. Cargo.lock adds only that existing package to Develop's dependency list; no new package, version, C/C++ implementation, or `-sys` crate was introduced.

## Attribution and tooling

- Registered the SVG directory and contact sheet as original Local Image contributor work, GPL-3.0-or-later, added 2026-10-09, with the requested OpenAI image-generation style-direction credit.
- Updated curated attributions and regenerated `assets/attributions.json` offline; its Assets section includes the new family and contact sheet.
- The inherited branch documented `cargo xtask assets` but did not implement that command. Added a Rust audit of both attribution tables, tracked/untracked nonignored assets, licence-file references, and inherited relocated paths. Added missing notices for existing development screenshots/example images and ComfyUI test thumbnails so the complete inventory passes.

## Validation

Final Compositing/xtask verification passed with one test thread:

```sh
PATH="$HOME/.cargo/bin:$PATH" CARGO_BUILD_JOBS=3 RUST_TEST_THREADS=1 \
  cargo +1.98.1 test --offline -p photocraft-ui-egui -p xtask --no-fail-fast
```

Compositing: 917 unit tests and 68 integration tests passed; seven existing ignored tests. xtask: 12 passed. Contact-sheet test: one passed, rendering all 93 icons. A final combined run of all four changed crates is in progress.

Clippy passed for all four changed crates and all targets with warnings denied:

```sh
PATH="$HOME/.cargo/bin:$PATH" CARGO_BUILD_JOBS=3 CARGO_TARGET_DIR=target/clippy \
  cargo +1.98.1 clippy --offline -p photocraft-ui-egui -p lightcraft-ui-egui \
  -p photocraft-engine -p xtask --all-targets -- -D warnings
```

Formatting ran for all four changed crates. `git diff --check` passes. No unrelated CPU-painter formatting changes were retained.

The final combined test build was interrupted when the repository's entire `target` directory was removed externally. Source changes and the generated contact sheet remained intact. The rerun uses the separate ignored `build/codex-color-icons-20261009` directory, with its log in `/tmp/codex-color-icons-all-final-tests.log`.

The offline attribution generator succeeded (690 Rust crates, nine Assets entries). The asset audit succeeded: 333 assets attributed and all referenced licence files present.

New checks cover every embedded SVG at 20/24/48 pixels, nonempty renders, disabled grayscale/alpha, cache identity across size/scale/state, exhaustive tool coverage, unique tools, unused SVGs, real toolbar preference clicks, CPU-rendered saturation changes, persistence, disabled clicks, and Develop retouch/mask selection clicks. UI checks are windowless and require no GPU adapter.

GPU tests added: none. This change uses CPU SVG rasterization and existing egui texture uploads; it changes no GPU algorithm or shader.

The initial parallel run exposed two missing translated labels, which were added, an existing AI status-note race, and SIGSEGV in three existing GPU integration binaries. The AI test passed alone; all three GPU binaries passed with one test thread. The final serial Compositing suite passes in full. Contrary to the task's expected no-adapter skip, these existing GPU tests did run successfully with an adapter available to wgpu; RTX 5090 hardware was not verified.

The sandbox denies local HTTP socket binding. The existing engine mock-server test now follows the UI's existing convention of explicitly skipping only `PermissionDenied` (including the mock's wrapped IO error); other setup errors still fail. The mock-server flows remain unexercised here and run normally when local sockets are permitted. No production AI code changed.
