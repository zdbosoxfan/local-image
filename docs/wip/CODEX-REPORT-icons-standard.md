# Tool icons: standard editing metaphors

Continued the existing colour-icon implementation on `wip/codex-icons-standard`, without resetting or committing it. All work stays in this worktree.

## Deliverables

1. Redrew all **93 existing tool icons** and added one dedicated Targeted Adjustment icon, giving **94 colour + 94 monochrome SVGs**. The original IDs remain intact. Following the owner's 2026-10-10 review, the industry-standard contours now use the chosen **Glossy Duotone** colour treatment: filled faces, the original nine family gradients and neutral-metal gradient, `#1b1c20` outlines, and restrained `#F7F9FC` highlights capped at 28% opacity. Inherently hollow glyphs retain their openings through substantial gradient-filled bands. Artwork is hand-authored in `xtask/src/tool_art.rs`, on the same 24 px grid with the original optical inset. Both styles share geometry, holes and padding; monochrome replaces visible materials with `currentColor` and receives the existing theme tint. Transparent cutouts preserve details such as finger gaps, the nib slit and ruler ticks.
2. `docs/design/tool-icons.png` shows current colour and monochrome artwork at **20 and 40 px on dark and light backgrounds**. `docs/design/tool-icons-before-after.png` shows **old colour / new colour / new monochrome**, at both sizes on both backgrounds, with friendly tool names and asset IDs. `docs/design/tool-icons-before.json` preserves the original SVGs, including the old glyph used by Targeted Adjustment. The renderer also writes unscaled 640 px review panels under ignored `target/icon-review/`.
3. The recognition inventory below covers every asset and all existing tool families. Existing UI variants use these family glyphs. Freeform Pen, separate Add/Delete Anchor/Convert Point tools, Pattern Stamp, and separate Zoom Out are not members of the current 53-tool enum/93-asset family; no new tools, layout or behavior were introduced.
4. Coverage now includes matching monochrome files and all Develop strip IDs. Rendered alpha comparisons check both styles at 16/20/40 px; existing preferences, selection clicks, disabled-state, caching and silhouette tests remain. A new opt-in GPU test exercises all 94 texture uploads, actual tool-button clicks and switching styles at 1×/2× on a discrete Vulkan adapter. Validation results are recorded below.
5. No toolbar geometry, tool behavior, preference schema, dependency or shader changes. Generic controls and the colour-picker cursor keep their original glyphs and hotspot; tool controls explicitly select the new family. Original SVGs and sheets are attributed to Local Image contributors under **GPL-3.0-or-later** in the asset inventory and curated/generated in-app notices. No vendor paths, tracing, copied PNGs or SVGs are shipped.

## Mapping corrections

- Removed monochrome aliases that replaced tool silhouettes with a clock, cloud, generic pointer, droplet or unrelated selection glyph. Compositing and Develop now share the new monochrome assets, including rich tooltips and Liquify/global variants.
- Burn keeps the legacy `flame` ID but now depicts a cupped hand. Mixer Brush keeps `palette` but now depicts a brush with two drops. Note keeps `message-square` but now depicts a folded sticky note. Patch (`lasso-select`), Polygonal Lasso (`pentagon`), Gradient (`blend`) and Content Aware Move (`arrow-left-right`) likewise keep IDs with corrected metaphors.
- Develop Targeted Adjustment formerly shared `circle-dot` with Compositing Count. It now maps to `targeted-adjust` (target with vertical adjustment arrows); `circle-dot` is a numbered marker for Count. Point Color Picker now maps to the dropper-and-target `color-range` instead of the plain white-balance dropper. The before sheet preserves these controls' former glyphs.
- Dodge now has a **solid circular paddle head on a slender stick**, with no lens opening or magnifier badge. Content-Aware Move now uses **diagonal crossed arrows over a dashed selection box**, clearly separate from Move's orthogonal arrow cross and pointer.

## Sources followed for the glossy revision

The direct colour-style source is the owner's original Local Image artwork, preserved in [`tool-icons-before.json`](../design/tool-icons-before.json) from the branch's original SVGs. All nine family stop pairs, the metal stop pair, dark-outline colour and highlight colour/opacity were verified against that snapshot. Original industry-standard paths were retained; vendor artwork was not copied or traced.

Official references followed: Adobe explicitly identifies [Dodge as a lollipop](https://www.adobe.com/products/photoshop/remove-shadow.html); the [Content-Aware Move guide](https://helpx.adobe.com/ca/photoshop/using/tool-techniques/content-aware-move-tool.html) documents the selection/move convention. The owner's requested crossed-arrow/dashed-box combination is drawn independently here. Material construction follows the W3C [SVG painting specification](https://www.w3.org/TR/SVG2/painting.html), including `paint-order="stroke fill"` so thin filled objects keep their coloured faces; [gradient coordinate systems](https://www.w3.org/TR/SVG2/pservers.html), using object bounding boxes for faces and `userSpaceOnUse` for horizontal/vertical bands; and [SVG luminance masking](https://www.w3.org/TR/SVG11/masking.html#Masking) for transparent detail cuts shared by both styles.

Rendering continues through the existing egui_extras 0.36.2 SVG loader and resvg. The installed upstream `egui_extras-0.36.2/src/loaders/svg_loader.rs` and `src/image.rs` were read directly from the Cargo registry; they define the resvg SVG path and size handling used by this work. The [upstream resvg project](https://github.com/linebender/resvg) documents static SVG rendering. No replacement renderer or GPU path was invented. `docs/PORTS.md`, the sampled engine-source inventory and the trace-reference provenance were checked for applicable existing work; their image-processing algorithms are unrelated to this icon-material revision.

The original work here is the per-tool path authoring, semantic division into filled faces/hollow bands, highlight placement and optical adjustments after 20 px review. There is no existing source for these independently drawn Local Image icons; shared conventions and the owner-approved materials supply the design constraints.

## Recognition references and limits

The table records **shared metaphors**, not pixel matches. Affinity and Photoshop conventions differ for a few tools; the task's requested metaphor decides those cases (for example Photoshop's cupped Burn hand). Variants marked “extension” use the established family metaphor with a Local Image badge; there is no claim that another app has the exact same composite glyph.

Primary references reviewed: [Affinity marquee and freehand selection](https://affinity.help/photo2/English.lproj/pages/Tools/tools_marquee.html), [Selection Brush](https://affinity.help/photo2/English.lproj/pages/Tools/tools_selectionBrush.html), [Paint Brush](https://affinity.help/photo2/English.lproj/pages/Tools/tools_paintBrush.html), [Healing Brush](https://affinity.help/photo2/English.lproj/pages/Tools/tools_healingBrush.html), [Pen](https://affinity.help/photo2/English.lproj/pages/Tools/tools_pen.html), [Liquify tools](https://affinity.help/photo2/English.lproj/pages/Tools/tools_liquify.html), [Photoshop retouch tools](https://helpx.adobe.com/nz/photoshop/desktop/repair-retouch/remove-objects-fill-space/retouch-tools-overview.html), [Photoshop Liquify](https://helpx.adobe.com/ca/photoshop/desktop/effects-filters/artistic-stylize-filters/use-liquify-to-distort-an-image.html), [Photoshop tool shortcuts/families](https://helpx.adobe.com/content/dam/help/en/photoshop/using/default-keyboard-shortcuts/photoshop-keyboard-shortcuts.pdf), [GIMP paint tools](https://docs.gimp.org/3.0/en/gimp-tools-paint.html), [Lightroom masking](https://helpx.adobe.com/lightroom-classic/desktop/process-and-develop-photos/masking.html), and [Lightroom Develop tools](https://helpx.adobe.com/lightroom-classic/desktop/process-and-develop-photos/develop-module-tools.html). These references were read for tool meaning and conventional silhouettes only. Capture One shares brush and gradient mask concepts; the Lightroom mask reference is the basis for this set.

Abbreviations: **AP** = Affinity Photo; **AD** = Affinity Designer; **PS** = Photoshop; **LR** = Lightroom/Lightroom Classic; **G** = GIMP. All rows describe the independently drawn Local Image artwork.

| Tool (preserved asset ID) | Metaphor | Apps using the convention |
|---|---|---|
| History (`activity`) | Clock and back arrow | LR, PS history |
| Content Aware Move (`arrow-left-right`) | Diagonal crossed arrows over a dashed selection box | PS |
| Background Mask (`background-mask`) | Landscape frame behind person | LR |
| Gradient (`blend`) | Square with continuous linear fade | PS, AP, G |
| Bloat (`bloat`) | Four outward arrows | PS, AP Punch |
| Blur (`blur`) | Water drop | PS, AP, G |
| Brush (`brush`) | Round brush with bristle tip | AP, PS, G, LR masks |
| Ellipse Shape (`circle`) | Filled ellipse | AD, AP, PS |
| Ellipse Marquee (`circle-dashed`) | Dashed ellipse | AP, PS, G |
| Count (`circle-dot`) | Numbered marker | PS Count; marker extension |
| Colour Range (`color-range`) | Dropper with target | LR, PS sampling |
| Crop (`crop`) | Two interlocking L corners | AP, PS, LR, G |
| Custom Shape (`custom-shape`) | Filled star | AD, PS custom shape |
| Depth Range (`depth-range`) | Layered depth planes | LR depth concept; layered extension |
| Develop Edit (`develop-edit`) | Three adjustment sliders | LR, Capture One adjustments |
| Direct Selection (`direct-select`) | Hollow pointer arrow | PS, AD Node selection |
| Dodge (`dodge`) | Solid round paddle on a slender stick | PS |
| Edit Toolbar (`edit-toolbar`) | Ellipsis with pencil | PS toolbar customization; extension |
| Eraser (`eraser`) | Diagonal eraser block | AP, PS, G |
| Background Eraser (`eraser-background`) | Eraser plus transparency grid | PS |
| Magic Eraser (`eraser-magic`) | Eraser plus sparkle | PS |
| Burn (`flame`) | Cupped hand | PS |
| Forward Warp (`forward-warp`) | Pushing finger plus arrow | PS, AP Liquify |
| Freeze Mask (`freeze`) | Closed padlock with mask stripe | AP freeze protection; extension |
| Freeze All (`freeze-all`) | Freeze padlock plus whole-image box | AP/PS freeze family; extension |
| Freeze Lasso (`freeze-lasso`) | Lasso plus padlock | AP selection/freeze families; extension |
| Guided Upright (`guided-upright`) | Perspective guides and straight verticals | LR guided perspective |
| Hand (`hand`) | Open palm | AP, PS, LR, G |
| Healing (`healing`) | Adhesive bandage | PS, AP, G healing |
| History Brush (`history-brush`) | Brush plus back arrow | PS |
| Info (`info`) | Circled lowercase i | LR, general information convention |
| Invert Freeze (`invert-freeze`) | Half-filled mask circle | AP/PS mask inversion family |
| Keywords (`keywords`) | Label tag with hole | LR, photo catalog tags |
| Landscape Mask (`landscape-mask`) | Mountains and sun | LR |
| Lasso (`lasso`) | Curved rope loop and tail | PS, AP freehand, G |
| Magnetic Lasso (`lasso-magnetic`) | Rope loop plus horseshoe magnet | PS, AP magnetic selection |
| Patch (`lasso-select`) | Dashed bandage patch with move arrow | PS, AP Patch |
| Linear Mask (`linear-mask`) | Gradient rectangle with centre guide | LR, Capture One |
| Luminance Range (`luminance-range`) | Sun with rays | LR luminance concept; sun extension |
| Masking (`mask`) | Dashed circular boundary and interior | LR |
| Note (`message-square`) | Folded sticky note with writing | PS Notes |
| More (`more`) | Horizontal ellipsis | LR, PS |
| Path Selection (`mouse-pointer-2`) | Solid pointer arrow | PS, AD selection |
| Move (`move`) | Four-way arrow cross and pointer | PS, AP |
| Object Mask (`object-mask`) | Dashed box around solid object | LR |
| Object Selection (`object-selection`) | Dashed rectangle and pointer | PS |
| Paint Bucket (`paint-bucket`) | Tilted bucket and pouring drop | PS, AP Flood Fill, G |
| Mixer Brush (`palette`) | Brush with two drops | PS mixer; two-material extension |
| Pen (`pen-tool`) | Fountain nib with slit | AD, AP, PS |
| Pencil (`pencil`) | Sharpened diagonal pencil | PS, G, AD |
| Polygonal Lasso (`pentagon`) | Straight-segment loop and tail | PS, AP polygonal selection |
| People Mask (`people-mask`) | Two person busts | LR |
| Perspective Crop (`perspective-crop`) | Crop corners with skewed grid | PS |
| Eyedropper (`pipette`) | Glass tube, bulb, tip down-left | AP Colour Picker, PS, G, LR |
| Presets (`presets`) | Overlapping circles | LR presets |
| Prompt Mask (`prompt-mask`) | Text inside dashed selection | PS/AD text and selection families; extension |
| Pucker (`pucker`) | Four inward arrows | PS, AP Pinch |
| Push Left (`push-left`) | Side arrow crossing warped lines | PS, AP Liquify |
| Quick Mask (`quick-mask`) | Rectangle with half-filled mask circle | PS |
| Quick Selection (`quick-selection`) | Brush with dashed tip circle | PS, AP Selection Brush |
| Radial Mask (`radial-mask`) | Ellipse with dashed feather boundary | LR, Capture One |
| Reconstruct (`reconstruct`) | Brush with back arrow | PS, AP reconstruct |
| Reconstruct All (`reconstruct-all`) | Reconstruct brush plus whole-image box | PS/AP reconstruct family; extension |
| Rectangle Shape (`rectangle-horizontal`) | Filled rectangle | AD, AP, PS |
| Red Eye (`red-eye`) | Eye with target cross | PS, LR |
| Spot Removal (`remove`) | Repair bandage with minus badge | LR removal/healing family; extension |
| Ruler (`ruler`) | Diagonal ruler with ticks | PS, G Measure |
| AI Cutout (`scissors`) | Open scissors | G Intelligent Scissors; PS cutout concept |
| Polygon Shape (`shape-polygon`) | Filled pentagon | AD, AP, PS |
| Triangle Shape (`shape-triangle`) | Filled triangle | AD, AP, PS |
| Sharpen (`sharpen`) | Triangular pyramid | PS, AP |
| Sky Mask (`sky-mask`) | Cloud above horizon | LR |
| Line Shape (`slash`) | Diagonal line | AD, AP, PS |
| Slice (`slice-knife`) | Craft knife | PS, AP Export Slice |
| Slice Select (`slice-select`) | Same knife with pointer | PS slice family |
| Smooth (`smooth`) | Two flowing wave lines | AP/PS Liquify smoothing family |
| Smudge (`smudge`) | Bent finger and smudge stroke | PS, AP, G |
| AI Remove (`sparkles`) | Brush with sparkle | PS/LR Remove; AI extension |
| Sponge (`sponge`) | Sponge block with four pores | PS, AP |
| Spot Healing (`spot-healing`) | Bandage with dashed spot circle | PS, AP Blemish Removal |
| Rectangle Marquee (`square-dashed`) | Dashed rectangle | AP, PS, G |
| Clone Stamp (`stamp`) | Rubber stamp | PS, AP Clone, G |
| Straighten (`straighten`) | Tilted spirit level over baseline | LR, PS leveling |
| Subject Mask (`subject-mask`) | Bust surrounded by dashed outline | LR, PS Select Subject |
| Targeted Adjustment (`targeted-adjust`, new) | Bullseye plus up/down arrows | LR |
| Thaw Mask (`thaw`) | Open padlock with mask stripe | AP freeze/thaw family; extension |
| Thaw All (`thaw-all`) | Thaw padlock plus whole-image box | AP/PS mask family; extension |
| Twirl CCW (`twirl-ccw`) | Counterclockwise spiral and arrowhead | AP, PS |
| Twirl CW (`twirl-cw`) | Clockwise spiral and arrowhead | AP, PS |
| Type (`type`) | Filled serif T | AD, AP, PS, G |
| Versions (`versions`) | Stacked image rectangles | LR |
| Vertical Type (`vertical-type`) | Same serif T and downward arrow | PS |
| Magic Wand (`wand-sparkles`) | Diagonal wand with sparkle at tip | PS, AP Flood Select, G Fuzzy Select |
| Zoom (`zoom-in`) | Magnifier with plus | AP, PS, LR, G |

Develop aliases: `heal` uses Healing; `clone` uses Clone Stamp; `ai` uses AI Remove; `masking` uses Masking; `redeye` and `petEye` use Red Eye; `wbPicker` and `negDmin` use Eyedropper; `pointColorPicker` uses Colour Range; `target` and `tat-*` use Targeted Adjustment. Mask kinds map exhaustively to the named mask glyphs above. All 53 Compositing tools, 15 Liquify variants/actions, Edit Toolbar, Quick Mask, Develop panels and 13 mask variants are covered.

## Visual review

Inspected all 94 rows of both sheets using the unscaled `target/icon-review/` panels, keeping the 20/40 px raster sizes intact, on both backgrounds and in both styles. The final colour set restores the owner's Glossy Duotone material treatment across the new contours. Dodge's filled head and Content-Aware Move's diagonal X/dashed box are distinct at 20 px. Review of the filled treatment prompted shared transparent finger gaps, nib slit, ruler ticks, tag hole, note fold/text and object-face divisions; a hollow Red Eye glyph and perspective grid; a dashed hollow Patch; and actual transparency checkerboard cells. These keep the monochrome metaphors readable when visible paint is reduced to a single colour. Both sheets were regenerated after the refinements.

The base metaphors are readable at 20 px. Patch is the busiest conventional composite at this size, and I am less certain of its unlabelled recognition than of the simpler base tools. Remaining custom recognition uncertainty concerns **variant meaning**: the whole-image box on Freeze All/Thaw All/Reconstruct All, text-in-selection for Prompt Mask, and layered planes for Depth Range extend related conventions. These benefit from the existing tooltips. No empirical user recognition study was performed.

The first sheet renderer check rejected a blue-gray monochrome tint because low-alpha antialiasing amplified RGB rounding differences. Review tiles now use neutral gray tints; runtime icons still use the existing themes. The revised renderer check passed.

## Validation

All cargo commands use Rust **1.98.1**, `--offline`, `CARGO_BUILD_JOBS=3`, and one build at a time. Final validation for the 2026-10-10 glossy revision passed. Full-suite coverage is complete for **only the three changed crates**, once for this revision; interrupted runs were resumed without repeating completed cases, as detailed below. No tests were removed or weakened. A new raster test requires an opaque Dodge paddle head at 16/20/40 px in both styles; silhouette regression pairs also distinguish Dodge/Zoom, Content-Aware Move/Move, and hollow/solid path selection.

| Check | Passed | Failed | Ignored |
|---|---:|---:|---:|
| `lightcraft-ui-egui` library | 209 | 0 | 1 |
| `photocraft-ui-egui` library | 964 | 0 | 5 |
| `photocraft-ui-egui` GPU integration | 54 | 0 | 3 |
| `photocraft-ui-egui` CPU integration | 14 | 0 | 0 |
| `xtask` | 15 | 0 | 0 |
| **Full-suite total** | **1,256** | **0** | **9** |
| Explicit final hardware icon test | 1 | 0 | 0 |
| Explicit contact-sheet renderer | 1 | 0 | 0 |

Both UI crates' doc tests passed (zero cases). Of the nine ignored tests in the ordinary full run, the contact-sheet writer and the new hardware test passed when explicitly invoked; the other seven are existing optional benchmarks. The final development filter `tool_icon` passed **16 tests** (11 Compositing, 5 Library/Develop); these overlap the full-suite counts.

Initial full-suite command:

```sh
PATH=~/.cargo/bin:$PATH CARGO_BUILD_JOBS=3 CARGO_NET_OFFLINE=true \
RUST_TEST_THREADS=1 WGPU_BACKEND=vulkan \
VK_DRIVER_FILES=/usr/share/vulkan/icd.d/nvidia_icd.x86_64.json \
WGPU_ADAPTER_NAME='RTX 5090' \
cargo +1.98.1 test --offline -p photocraft-ui-egui -p lightcraft-ui-egui -p xtask -- --show-output
```

The initial full run received **SIGTERM (exit 143)** after 179 Library/Develop tests passed and one benchmark was ignored. Its 30 uncompleted cases then passed. That resume command received another SIGTERM after six GPU integration cases passed. Neither interruption reported a failing assertion or a memory-guard diagnostic; the source of the signals was not reported. The remaining cases completed in bounded groups with libtest's documented `--exact` name filters, using the same Cargo-produced binaries and the existing `li-test-runner` memory guard. `cargo test --no-run --message-format=json` supplied the executable paths without rebuilding. No completed full-suite cases were rerun.

A small worktree-local recovery script compared completed names against each binary's `--list` inventory. The resulting `target/icon-glossy-full-coverage.json` accounts for all **1,265 cases exactly once** in the full-suite results: **1,256 passed / 0 failed / 9 ignored**. The script and grouping are original test-run bookkeeping for these interruptions; product test behavior was unchanged. Doc tests and clippy were then run through Cargo normally.

**Real GPU verification:** the added `gpu_tool_icons_upload_and_switch_styles_on_hardware` test renders all 94 tool icons, clicks a tool button, switches colour/monochrome, and checks the rendered pixels at 1× and 2×. It requires a discrete Vulkan adapter and fails instead of skipping if hardware is unavailable. The final run printed **NVIDIA GeForce RTX 5090 / DiscreteGpu / Vulkan / NVIDIA 615.71.09** at both scales and passed. No software-adapter or no-adapter result is counted as validation.

```sh
PATH=~/.cargo/bin:$PATH CARGO_BUILD_JOBS=3 CARGO_NET_OFFLINE=true \
RUST_TEST_THREADS=1 LOCAL_IMAGE_TEST_ADAPTER='RTX 5090' WGPU_BACKEND=vulkan \
VK_DRIVER_FILES=/usr/share/vulkan/icd.d/nvidia_icd.x86_64.json \
cargo +1.98.1 test --offline --lib \
  -p photocraft-ui-egui -p lightcraft-ui-egui -p xtask \
  gpu_tool_icons_upload_and_switch_styles_on_hardware -- --ignored --nocapture
```

`cargo +1.98.1 xtask tool-icons` regenerated both sheets successfully. The `xtask` artwork freshness test verifies all generated SVGs against their Rust geometry; a separate inventory check confirmed that all 93 original SVGs are preserved byte-for-byte in the baseline and that the 94 recognition rows cover both current styles exactly. `xtask assets` passed: **538 assets attributed**, with all referenced licence files present. The inventory also now covers the pre-existing CC0 trace-reference SVGs; no trace implementation changed. Curated and generated in-app attribution entries were updated together.

Formatting passed for all three crates. Cross-crate module imports now leave formatting to the file's owning crate, respecting Compositing's 160-column and Develop's 150-column configurations. `softpaint.rs` has formatting-only changes from its own formatter. These final formatting adjustments do not change the tested behavior.

Clippy passed for **all targets of all three changed crates**, using the required separate target directory and `-D warnings`:

```sh
PATH=~/.cargo/bin:$PATH CARGO_BUILD_JOBS=3 CARGO_NET_OFFLINE=true \
CARGO_TARGET_DIR=target/clippy cargo +1.98.1 clippy --offline \
  -p photocraft-ui-egui -p lightcraft-ui-egui -p xtask --all-targets -- -D warnings
```

`git diff --check` passed. `Cargo.toml` and `Cargo.lock` are unchanged.

Development checks caught and fixed the weak 16 px wand shaft and contact-sheet monochrome tint rounding in the earlier pass. During the glossy revision, the existing crop saturation assertion caught a too-neutral colour arm; restoring colour on both crop corners resolved it without weakening the test. Two earlier development test commands were interrupted before completion; their partial runs are not counted as passing validation. No failing assertions remain in the completed runs. The build still prints the existing missing optional CJK-font warning; this task did not install fonts or dependencies.

Logs remain under ignored `target/`: `icon-glossy-targeted-final.log`, the partial `icon-glossy-full-tests.log` and `icon-glossy-full-resume.log`, completed `icon-glossy-resume-lightcraft_ui_egui.log` and `icon-glossy-group-*.log`, `icon-glossy-full-coverage.json`, `icon-glossy-gpu-final.log`, `icon-glossy-sheet-final.log`, `icon-glossy-assets-final.log`, `icon-glossy-attributions.log`, `icon-glossy-doctests.log`, `icon-glossy-format.log`, and `icon-glossy-clippy.log`. Earlier-pass logs remain separately under their original names. GPU algorithms, shaders, and ComfyUI workflows are unchanged; ComfyUI was not used and no performance claims are made. The running desktop app and other worktrees were left alone. No commits were created. All requested implementation and artwork deliverables are complete; the custom-metaphor recognition limits above are documented for review.
