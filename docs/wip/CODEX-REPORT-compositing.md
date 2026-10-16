# Compositing contextual UI, live shapes, and Pen bar

Continued the existing `0ceb78e` WIP commit rather than replacing it. Changes are left
uncommitted in this worktree; the owner's pre-existing task-file edit is preserved.

## 1. Edit shapes after placement

- Shape tools, Move, Path Selection and Direct Selection show the selected shape's
  Fill, Stroke, stroke width/type, W/H/X/Y, and live geometry. Pen-created free-form
  shapes get Fill/Stroke/width and path scaling/position controls too.
- Fill and Stroke are clickable colour pickers, with No Color. Stroke controls offer
  solid, dashed and dotted styles. Live rectangles have an overall radius and a
  Corners popup for separate TL/TR/BR/BL radii; polygons have sides and star ratio;
  lines have length, angle and weight.
- All selected-shape edits use `shape.edit`; continuous colour/numeric edits coalesce
  into one undo step. Existing live-shape data already supported per-corner radii,
  so no incompatible shape schema extension was needed.
- Creating or selecting shape/type layers reveals Properties. Studio's floating
  Properties card includes shape/type controls. Double-clicking a shape thumbnail
  opens its Fill picker, including when Properties or Appearance was collapsed.
  Automatic expansion waits for the double-click interval to finish so the Layers
  rows do not move between thumbnail clicks.

## 2. Context follows the selected item

- Shared layer-kind resolution supplies Shape, Text, Smart Object, Develop,
  Generated and Pixel contexts, regardless of the tool that created the layer.
  Selections take precedence, followed by selected closed paths.
- Saved paths and targeted vector masks retain path actions with Move. An explicitly
  selected path takes precedence; an unrelated stale work path does not displace a
  selected shape's controls. Unpainted shapes and missing raster previews retain
  useful contextual actions.
- Shape: Fill/Stroke/width/radius, Edit Path and Rasterize. Text: font/size/colour and
  Edit Text. Smart: Edit Contents. Develop: Open in Develop. Pixel: Select Subject,
  Remove Background and Transform. Generated: Regenerate/Variations/Select Subject.
- AI Fill, Generate Background and placed Generate/Library images keep generation
  metadata on the layer, preserved in `.pcraft`. Old manifests without the field
  load with `None`.
- Regenerating fills/backgrounds hides the old result only after success, in the
  same undo step as the new layer. Failure preserves the old result. General
  Generate/Library results restore their prompt, model/variant, dimensions, steps,
  guidance, denoise, LoRAs and mode; Regenerate starts a fresh job, and Variations
  opens the controls to adjust the prompt/batch.
- A held press on the bar or its popups keeps widgets alive. Numeric edits that
  change bounds keep the bar anchored during the drag. Canvas drags still hide it.
- New labels and tooltips are translated in all 12 non-English catalogs.

## 3. Pen and Selection actions work

- The closed-path bar has exactly Make Selection, Content-Aware Fill, AI Fill and
  Mask. Actions target the selected path, including saved paths.
- Content-Aware Fill converts paths to selections. On Smart/Develop, shape, text,
  locked or otherwise non-writable layers it samples the visible composite and
  writes a new pixel layer above the active layer. Selection also has a visible
  Content-Aware Fill button.
- AI Fill opens a prompt; leaving it empty requests a seamless continuation of the
  surroundings. Mask prefers a vector mask and falls back to a selection layer mask.
- Selection actions include Remove, Generative Fill, Content-Aware Fill, Invert,
  Mask, Deselect, and More (Feather, Select and Mask, fill options, Hide Bar).

## Verification

All Cargo builds ran offline with Rust `+1.98.1`,
`PATH=~/.cargo/bin:$PATH`, `CARGO_BUILD_JOBS=3`, and one Cargo build at a time.
Package names are `photocraft-*` (the source directories are named `pc-*`).

| Crate | Passed | Existing ignored | Sandbox-blocked test excluded |
| --- | ---: | ---: | ---: |
| `photocraft-doc` | 48 | 0 | 0 |
| `photocraft-format` | 85 | 0 | 0 |
| `photocraft-engine` | 844 (806 unit + 38 integration) | 15 | 1 |
| `photocraft-ui-egui` | 938 (877 unit + 61 integration) | 3 | 1 |
| **Total** | **1,915** | **18** | **2** |

The passing counts include existing tests that return successfully when their
required graphics/font capability is unavailable, detailed below. No tests were
newly marked ignored.

Final passing test commands, with the common PATH/jobs prefix above:

```sh
cargo +1.98.1 test --offline -p photocraft-doc
cargo +1.98.1 test --offline -p photocraft-format -p photocraft-engine -- --skip ai_cmds::tests::ai_commands_against_the_mock_server
cargo +1.98.1 test --offline -p photocraft-engine -- --skip ai_cmds::tests::ai_commands_against_the_mock_server
VK_DRIVER_FILES=/dev/null cargo +1.98.1 test --offline -p photocraft-ui-egui -- --skip context_bar::tests::path_and_selection_ai_and_generated_layer_buttons_work --nocapture
```

Formatting passed for all four crates, including the final `fmt -- --check`.
The following strict all-targets lint passed:

```sh
CARGO_TARGET_DIR=target/clippy cargo +1.98.1 clippy --offline -p photocraft-doc -p photocraft-format -p photocraft-engine -p photocraft-ui-egui --all-targets -- -D warnings
```

Rust 1.98.1's lint also required two equivalent `chunks_exact(4)` →
`as_chunks::<4>().0.iter()` changes in existing engine sky guidance and a seamless
editing test. The final engine suite passed after these changes. Whitespace checks
passed for implementation files and this report; the pre-existing task-file edit
(including its trailing blank line) was left alone.

The interaction tests press and release in separate frames, hold presses, and drag
numeric fields. They cover every Path/Selection action, saved-path selection,
composite CAF on a Smart Object, focused AI prompts and Back, shape Fill/No Color/
Solid Color, coalesced W/radius/stroke edits, Edit Path/Rasterize, Studio polygon
sides, collapsed Properties/Appearance thumbnail double-clicks, text editing, and
generated-layer settings. Mock-Comfy tests additionally exercise AI layer effects,
fresh seeds, replacement undo, and failure preservation. Native-format tests cover
generation metadata roundtrip and an old layer manifest without the field.

### Graphics coverage and remaining verification

GPU code/tests added: **0**; these changes affect document metadata, CPU commands,
and egui interactions. Existing GPU/canvas tests were still run.

Default GPU-driver discovery crashed `adjust_preview_gpu` with SIGSEGV in this
sandbox. Disabling Vulkan discovery only for the test process
(`VK_DRIVER_FILES=/dev/null`) allowed the full UI suite to pass through the available
OpenGL rendering path. Canvas 16-bit/32-bit uploads, drag previews, device-loss
fallbacks, live strokes, and the three Camera Raw image tests passed. This does not
verify the RTX 5090 Vulkan path.

Five existing GPU tests returned through their capability-skip paths because the
available adapter cannot render `Rgba32Float`:

- `adjust_preview_gpu::preview_layer_composites_on_the_gpu_like_the_command`
- `adjust_preview_gpu::proxy_preview_composites_on_the_gpu`
- `color_managed_canvas::display_p3_is_converted_on_gpu_and_cpu_canvases`
- `color_managed_canvas::srgb_documents_are_unchanged`
- `layout_perf::layout_document_composites_on_the_gpu_like_the_cpu`

A separate adapter-disabled run also confirmed the existing no-adapter skip paths.
The existing `cjk_fonts::tests::craft_fonts_render_japanese_without_system_fonts`
check returned early because the optional craft-fonts checkout is not embedded.

Unfiltered engine/UI test runs were attempted. These two tests cannot start their
mock ComfyUI server here: binding `127.0.0.1:0` fails with OS error 1,
`PermissionDenied: Operation not permitted`:

- `ai_cmds::tests::ai_commands_against_the_mock_server`
- `context_bar::tests::path_and_selection_ai_and_generated_layer_buttons_work`

They remain intact and are excluded only by the explicit command-line filters
above. The coordinator must run those tests with loopback sockets permitted and
rerun the five float32 GPU checks on the RTX 5090. Therefore the task's requirement
for a completely unfiltered passing suite remains environmentally blocked; no
functional implementation work remains identified.

No new dependencies, unsafe Rust, C/C++ source, or GPU code were introduced. No GUI
windows were opened, no running app was stopped, and no owner files outside this
worktree were changed.
