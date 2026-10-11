# Package 47 — shared tools (2026-10-10)

Continued HEAD `327752c1` with the inherited implementations intact. Initial status contained
only the untracked task brief; there was no separate dirty package-47 implementation to reset.
`git log wip/vector` confirms packages 02–08, 12, 32–36, 41 and 46 merged. All changes are uncommitted.

The package-owned implementation is delivered. New persona/tool/menu/canvas registration remains
in the required package-00 handoff, [integration/47.md](vector-tasks/integration/47.md).
This report distinguishes shared implementation from integrated app acceptance. Hardware performance
and GPU acceptance are deferred by the final owner SPEED RULES; no timing or GPU pass is claimed.

## 1. Audit and established behavior

Before this change, toolsets contained 53 Pixel IDs and 59 Vector IDs, with 16 exact shared IDs.
The same legacy canvas already handled most common IDs, but its Pen and basic shape frontend
retained simpler implementations alongside the richer feature packages. Node's richer implementation
already extended the original `direct_select.rs`; a second Node implementation was unnecessary.
Package 47 adds 29 geometric IDs to Pixel's supported/customizable tool inventory, giving 45
shared IDs. Frozen ALL_TOOLS, built-in layouts, old tool IDs, and package-46 shortcuts stay intact.
The serial shell handoff appends the additional tools to the existing flyouts.

Official help confirms matching capabilities for the key common tools:

- [Photo Pen](https://affinity.help/photo2/en-US.lproj/pages/Tools/tools_pen.html) and
  [Designer Pen](https://affinity.help/designer2/en-US.lproj/pages/Tools/tools_pen.html):
  Pen, Smart, Polygon and Line drawing, optional rubber band and curve addition. Photo additionally
  exposes selection/mask outputs. Persona does not justify a reduced curve editor.
- [Photo Node](https://affinity.help/photo2/en-US.lproj/pages/Tools/tools_node.html) and
  [Designer Node](https://affinity.help/designer2/en-US.lproj/pages/Tools/tools_node.html):
  node conversions, curve operations and node transforms; selected live shapes show shape controls.
- [Photo Rectangle](https://affinity.help/photo2/en-US.lproj/pages/Tools/tools_rectangle.html) and
  [Designer Rectangle](https://affinity.help/designer2/en-US.lproj/pages/Tools/tools_rectangle.html):
  live corner settings/handles, fill/stroke and conversion to curves.
- [Photo Artistic Text](https://affinity.help/photo2/en-US.lproj/pages/Tools/tools_artText.html),
  [Designer Artistic Text](https://affinity.help/designer2/en-US.lproj/pages/Tools/tools_artText.html),
  [Photo Frame Text](https://affinity.help/photo2/en-US.lproj/pages/Tools/tools_frameText.html) and
  [Designer Frame Text](https://affinity.help/designer2/en-US.lproj/pages/Tools/tools_frameText.html):
  the same art/frame text families, typography and editing controls.

Implementation follows the merged feature interfaces, architecture §5, package-46 frozen keymaps,
and the locally sampled VectorCraft `crates/tools/src/{pen,shape}.rs` at
`d522c1d7be4035bd4f4a84cd6ebfca44f5155092`. Existing provenance/license notices are retained.
The mutation stamp and reusable preview container are original Local Image adaptations of the
existing detached gesture protocol, rather than an imported second tool framework.

Paths in the table are relative to `crates/pc-ui-egui/src` (UI) and `crates/pc-engine/src` (engine).
Feature hooks are compiled into the shared public UI module; availability in the production shell
still depends on the documented serial registration. No missing provider is treated as a successful
empty operation.

| Tool family / all covered IDs | Pixel code before | Vector code before | Shared code / differences and duplication |
|---|---|---|---|
| Move | `canvas.rs`, `move_input.rs`, `move_ui.rs`, engine `edit_cmds.rs` | Same existing canvas; package-34 specialized transforms | One existing Move path for pixels, shape geometry, groups and text; auto-selection, modifiers and detached previews are shared. No duplicate Move tool removed. |
| Pen | `vector_ui.rs::pen_down/move/up/commit` | `vector_packages/pen.rs`, engine `vector_packages/pen{,/gesture}.rs` | Removed legacy placement/handle/closure algorithm. Legacy entrypoints and both persona bindings now delegate to package 04. Context shows all four modes, rubber-band and curve-add options. Work/shape/vector-mask destination is captured; selection conversion retains `path.toSelection`. |
| Node / DirectSelection | `direct_select.rs`, engine `path_edit_cmds.rs` | The same extended `direct_select.rs`, engine `vector_packages/node.rs` | One Node pointer/context/overlay implementation, with legacy command aliases retained. Shared raw adapter calls the actual Node capture/frame functions. Multi-curve editing, live conversion prompts, modifiers and corner metadata stay in the feature implementation. |
| PathSelection | `vector_ui.rs::path_selection_finish`, `canvas.rs` | Same | One path move handler chooses work path, shape or targeted vector mask. No second implementation introduced. |
| Rectangle, EllipseShape, Triangle, Polygon, Line | `vector_ui.rs::shape_geometry/finish_shape` | `vector_packages/drawing.rs`, engine `vector_packages/drawing.rs`, `pc-vector/src/design_shapes.rs` | Removed duplicated rectangle/constraint/line calculations; both use `drawing::drag_design` and the same generator. Legacy commits now use `shape.design.create` and retain live parameters. Preview/commit paint is shared; custom foreground/background paint is an additive validated command payload. |
| RoundedRectangle, Star, DoubleStar, SquareStar, Trapezoid, Diamond, Donut, Pie, Segment, Arrow, Callout, Cog, Heart, Tear, Crescent, Cloud, Spiral, Arc, RectGrid, PolarGrid | Previously absent from Pixel inventory | Same drawing package | Supported in both personas; one pointer/context/generator. Tool IDs choose parameter defaults. All 25 registered shape IDs are interaction-tested in both personas. |
| CustomShape | `preset_panels.rs::finish_custom_shape`, `vector_ui.rs` | Same | Preset path fitting is a distinct output adapter, shared between personas. Bounding drag uses the common design geometry helper. No second preset geometry implementation added. |
| Type, VerticalType, ArtText, FrameText | `type_tool.rs`, engine `type_cmds.rs`, `type_extra_cmds.rs`, `pc-text` | Existing Type handler plus reserved art/frame IDs from package 12 | One established point/box type creator and inline editor; art/frame are defaults of that handler. Added IDs are supported in Pixel. The shared adapter applies art/frame creation defaults; the shell retains the same inline editor and text IME/key routing. Point and box type rendering share `pc-text`. |
| TextOnPath | Previously absent | `vector_packages/path_text.rs`, engine same, `pc-text/src/on_path.rs` | One compiled path-text input/context/projector for both personas, including typing, marker capture and one commit. Curve references remain document data. |
| Gradient / VectorFill / Transparency | `gradient_ui.rs`, engine `gradient_fill_cmds.rs` (live fill), `fill_cmds.rs` (classic pixels) | `vector_packages/appearance.rs`, engine same | Full shape Fill/Transparency handler is now callable in Pixel using its explicit IDs. Legacy Gradient's live/classic image operations remain a different output family; they are not silently substituted for shape appearance. Package 46 keeps Pixel G and Vector G defaults distinct. Both use existing document Fill and shared renderer paint data. |
| Eyedropper | `canvas.rs::sample_eyedropper/composite_color`, `color_picker_ui.rs`, engine `pixels.rs` | Same color sampler; package-38 attribute transfer in `vector_packages/eyedropper.rs`, engine `styles.rs` | Color sampling already shares one canvas path. Style transfer is an additional explicit tool mode/provider, not a second color sampler. Package-38 bridge accepts either persona when activated; its integration hook remains with 00. |
| Crop / VectorCrop | `crop_ui.rs`, engine `image_cmds.rs` | Same document Crop, plus `vector_packages/layout.rs`, engine same | Shared document crop frame/resizing; separate non-destructive object crop output uses the same layout handler in either persona. Added VectorCrop is supported in Pixel; its mask geometry is `vector.clip`. |
| Hand / Zoom | `canvas.rs`, `dialogs.rs::pan_delta`, `zoom_tool.rs`, `view_cmds.rs` | Same | Already one view-navigation implementation. Shared adapter leaves raw canvas pan/scrub/click handling in its existing host instead of swallowing navigation events. Actual canvas interactions are tested in both personas. |
| Pencil / VectorPencil | Raster `canvas.rs`, `paint_cmds.rs` / `photocraft_paint` | `vector_packages/pencil.rs`, engine same, shared fitting source | Architecture §5 explicitly requires distinct raster Pencil and VectorPencil identities. Geometric Pencil/Sculpt is now supported in Pixel with the exact Vector handler/fitter/options; historic pixel-perfect Pencil remains loadable. No reduced geometric Pencil exists. |
| Corner | Previously absent | `vector_packages/corners.rs`, engine same, shared corner geometry | One handler/projector in either persona; metadata and base node identities are preserved. Corner drag is interaction-tested in both. |
| Snapping / smart guides | `snap_ui.rs`, engine `snap.rs` | `vector_packages/snapping.rs`, engine `vector_snap.rs` | Shared geometric adapter captures one package-07 query cache per gesture and re-queries raw release with current modifiers, preserving construction rays. Shape and Pen use the same cache/query in either persona. Legacy raster/layer snap routing remains in the shell; 00 must use this richer provider at its shared boundary. |
| PointTransform / transform options | `transform_tool.rs`, engine `transform_cmds.rs` | `vector_packages/layout.rs` (point transform), `transform_tools.rs` (34) | PointTransform/VectorCrop share the compiled layout implementation in either persona. Existing raster Free Transform and 34's Rotate/Reflect/Scale/Shear/Reshape/FreeTransform/Each are contextual operations rather than new reduced persona copies. 34's same captured engine API remains available for both persona menu bindings. |
| VectorBrush | Raster Brush is a separate pixel output family | `vector_packages/brushes.rs`, engine same | Existing vector brush provider; no geometric clone in Pixel. Additional provider/tool registration remains with 00. |
| Smooth, PathEraser, Join, ShapeBuilder, Contour, Knife, Scissors, LineCut | Not Pixel baseline tools | `vector_packages/constructive.rs`, engine same | Feature 10's one geometry implementation; no duplicate Pixel tool exists. Provider registration retains those explicit identities. |
| Width | Not Pixel baseline | `width.rs`, engine `vector_packages/width.rs` | One width-profile implementation; no Pixel duplicate. |
| Blend, Repeat, Envelope | Not Pixel baseline | `blend.rs`, `repeat.rs`, `envelope.rs`, engine same | One captured feature implementation each; not duplicated in Pixel. |
| Artboard | Document/artboard commands via `artboard_ui.rs` | `vector_packages/layout.rs`, engine layout/artboard commands | Artboard creation/layout output adapter; serial tool registration remains with 00. |
| RectMarquee, EllipseMarquee, Lasso, PolygonLasso, MagneticLasso, ObjectSelection, QuickSelection, MagicWand | `canvas.rs`, `lasso_ui.rs`, `magnetic_lasso_ui.rs`, `smartselect_cmds.rs`, `selection_cmds.rs` | No same Pixel selection IDs; package 32's vector object/node/lasso/wand queries | Raster pixel selection and vector object/node selection are different target/result families. 32 reuses `DirectSelection` state for nodes and accepts persona-independent scope; no duplicate Node editor is added by 47. |
| Brush, MixerBrush, Eraser, BackgroundEraser, MagicEraser, PaintBucket, CloneStamp, HistoryBrush, SpotHealing, Healing, Patch, ContentAwareMove, RedEye, Blur, Sharpen, Smudge, Dodge, Burn, Sponge | `canvas.rs`, paint/stroke modules; corresponding engine paint/eraser/fill/retouch/redeye commands | No identical vector baseline IDs | Pixel-only output operations in the audited inventory; their names do not create reduced copies of geometric tools. Keep established raster stroke, mask targeting and rasterization prompt. |
| PerspectiveCrop, Slice, SliceSelect, Ruler, Note, Count, AiRemove, AiCutout | `perspective_crop_ui.rs`, `slice_ui.rs`, `analysis_ui.rs`, `ai_remove_ui.rs`, `ai_ui.rs`; corresponding engine commands | No identical vector IDs | Pixel-only baseline operations; no shared duplicate found. |

The table covers all 96 distinct baseline IDs. Package-46 supplemental slots also request
AddAnchor/DeleteAnchor/AnchorPoint/Curvature (33), Rotate/Reflect/Scale/Shear/Reshape/FreeTransform (34),
vector selection tools (32), clip/compound and object/path menu operations (35/36), and
VectorWarp/VectorTwirl/VectorPucker/VectorBloat/Scallop/Crystallize/Wrinkle/VectorPuppetWarp (41).
These are each implemented in their corresponding `vector_packages/<feature>.rs` UI/engine pair;
35/36 are commands, not extra pointer algorithms. 41's path deformations and raster Liquify/warp
have different target geometry and output semantics; no matching tool ID exists in both inventories.
The shared bindings must reuse those existing feature APIs rather than implement persona copies.

## 2. Unification and compatibility

- `pc-engine/src/shared_tools.rs` is the allocation-free family resolver. Case-insensitive IDs
  converge to one handler family before persona defaults. Every legacy/Vector registry ID resolves.
- `pc-ui-egui/src/shared_tools.rs` supplies one function binding per family. Native Pen, design
  shapes, Node, Pencil/Sculpt, Corner, Fill/Transparency, path text and point/crop layout use
  the inherited implementations directly. Existing shared canvas tools keep their established hosts.
- Removed the legacy Pen algorithm and old shape construction math from `vector_ui.rs`; the
  remaining functions adapt old automation/inspection structures and output paint. Old serialized
  Pen knots still load and finish through `path.penCommit`; new runtime capture is serde-skipped.
  New Pen options serde-default; native documents and settings keep their existing formats.
  Line and other stroke-only tools share foreground paint defaults with the legacy Line adapter;
  Pen Line uses a foreground stroke too. Filled-shape foreground/background options remain supported.
- Old engine command IDs stay registered as compatibility APIs. Keeping a command alias or the
  renderer of an old live-shape document is necessary for loading; it does not create a second
  pointer gesture implementation.
- ALL_TOOLS and the five Pixel built-in layouts, frozen command/tool shortcut fixtures, and package
  46's keymaps are unchanged. Additional supported IDs can be saved in Pixel custom sets and survive
  restart. New tool flyout/key/menu activation belongs in the serial handoff.

## 3. Optimization and limitations

Pen motion uses a small mutation generation instead of cloning the whole path to detect change.
The detached preview document is cloned once at gesture begin; subsequent changes replace only
its captured target or work path. New preview layers retain their insertion ID, avoiding repeated
allocation of document layer trees and ID churn. A regression test checks the layer vector's address,
preview ID, no journal writes, exact committed geometry and undo on a 12,000×12,000 document.
No image-sized buffer is allocated merely because that document has large dimensions.

Node overlay enumerates borrowed document/mask paths and anchor selections instead of cloning
every selected curve, selection and preview path each idle frame. Handle enumeration and node
transform bounds also borrow the selection. Shape overlays borrow already prepared paths and
use the host transform including flips, instead of regenerating geometry while idle.
Geometric shape previews expose a borrowed detached layer;
shared preview access borrows documents and returns each provider's own transient generation,
with a family tag to prevent cross-tool cache collisions. Typing/context previews invalidate
the key independently of pointer events, while idle previews retain their key. Runtime state belongs
to the host, not egui temp-data (whose `get_temp` would clone large gesture/preview state).
The shared raw-event adapter avoids cloning the full egui event vector and freezes the view map
for captured releases. It validates document/revision/active layer/selection and bounded coordinates. Physical
pixels-per-point comes from egui input and retained snap guides use the same transform.
Snap query failures cancel before preview/commit publication. The merged package-07 query now
rejects candidates outside an explicit construction ray and prefers real grid/node targets
on that ray over a zero-distance angle guide. Pen constrains through its existing engine helper
before querying, and manual Shift remains active when automatic angle guides are off.
This priority correction follows the local query/capture contract; it is an original bug fix,
not a second snapping solver. The common lifecycle hook
also cancels the existing Node capture across persona changes.

All edits retain the inherited Begin/Preview/Commit transactions: previews do not journal, releases
commit once, and Escape/focus/persona/tool/selection/revision changes abandon transients. Pen Enter
finishes once; path selection, type and document crop preserve their existing documented editing
sessions and commit behavior.

Remaining integration optimizations: inherited Node/Pencil/appearance/layout *changed-pointer*
preparation still creates detached document metadata snapshots. The large per-frame clone removals
above do not establish zero allocation across every provider. 00/09 must consume the borrowed
layer/document preview in the production GPU transient plane and measure real app Pen/shape/Node
drags on the RTX 5090 at integration. No llvmpipe result, synthetic renderer timing, skipped adapter,
release build or benchmark substitutes for that deferred measurement.

## 4. Validation

All Cargo commands use Rust 1.98.1, offline, CARGO_BUILD_JOBS=3, the inherited required
`RUSTC_WRAPPER=/home/zdavidson/.local/share/local-image-dev/bin/li-rustc-gate`, and one Cargo
invocation at a time. The new CPU tests use the shared adapters and the existing headless canvas. No existing
suite or GPU test is selected by the shared47 filter. Build and test logs are retained under
target/shared47-*.log (local, ignored artifacts).

| Gate | Result |
|---|---|
| Final workspace all-target check | Passed, 12.23s |
| Final strict changed-crate all-target clippy | Passed, 8.13s, engine and UI with `-D warnings` |
| New `shared47` engine tests | 4 distinct tests passed: 3 initial tests and the new constrained-snap regression |
| New `shared47` UI tests | 19 distinct tests passed; the initial 18-test filter and narrow corrective/new-test filters |
| Format and diff whitespace | Passed; rustfmt check with `reorder_modules=false` preserves the required append-only registration edits; `git diff --check` passed |
| New GPU tests | None; hardware execution explicitly prohibited by SPEED RULES |

Exact final build commands (offline, single invocation at a time):

```sh
cargo +1.98.1 check --offline --workspace --all-targets
CARGO_TARGET_DIR=target/clippy cargo +1.98.1 clippy --offline -p photocraft-engine -p photocraft-ui-egui --all-targets -- -D warnings
cargo +1.98.1 fmt --check -p photocraft-engine -p photocraft-ui-egui -- --config reorder_modules=false
```

The initial new-test commands were `cargo +1.98.1 test --offline -p photocraft-engine shared47`
and `cargo +1.98.1 test --offline -p photocraft-ui-egui --lib shared47`. The UI filter initially
passed 15 tests and failed 3. Corrective runs selected only the failing shape/cancel/snap names:
`shared47_all_live_shape_tools`, `shared47_cancel_switch_stale`, and `shared47_pen_snapping`.
A diagnostic shape retry established that modifier-only input before motion produced a zero-size
preview; the fix retains capture and abandons zero-size geometry without an edit. The cancellation
fixture was corrected to the registered `layer.duplicate` command. The Pen snap regression caught
the zero-distance angle guide suppressing a valid grid target; the shared query/constraint fix also
checks manual Shift with automatic angle guides off. All corrected filters passed. Two later new
filters, engine `shared47_constrained_snap` and UI `shared47_stroke_only_tools`, each passed once.
No successful full new-test filter was repeated, and no existing test was executed.

The workspace gate initially caught new test field/borrow errors and the inherited package-05
scratch GPU harness's references to temporary `vector_pencil_enabled`/`vector_pencil` app fields.
That harness was never a registered production test: its original report/handoff describes the
`target/` registration rehearsal. Its source is preserved; it is no longer imported by the newly
compiled production Pencil module. 00 must migrate its full-app acceptance to the shared runtime.
The second check caught one borrowing error in the Node overlay refactor; the corrected all-target
check passed. The final all-target gate above ran after the remaining UI/input fixes. Clippy initially
caught a collapsible conditional, and compiling the added tests caught a TextShape namespace error
and a stroke-width type mismatch; the final strict gate passed. These corrective retries selected
new tests or corrected compile/lint errors, rather than running additional broad suites.

Full-crate and whole-workspace test suites, GPU acceptance runs, release builds and benchmarks
are deliberately skipped under the overriding SPEED RULES. Existing test fixtures changed only
where a PenPath literal needs the new defaulted runtime field, and the closure regression now
releases its pointer before expecting a completed gesture.

All 23 distinct new tests passed. UI coverage includes all four Pen modes, all 25 live-shape
IDs, CustomShape, Node, Path Selection, Pencil, Corner, Fill/Transparency, point/frame/vertical
text and path text, document/object Crop, Move, Hand/Zoom, sampling, snapping and cancellation.
The function-identity guard checks all 45 shared IDs. Sculpt delegates to the same inherited
Pencil provider but is not separately exercised by the new interaction tests. Text typing changes its borrowed preview key;
idle input preserves it. Physical snapping tolerance and flipped views are exercised by real events.

No dependencies, manifests, Cargo.lock, unsafe code, commits, desktop windows, running app,
ComfyUI queue or owner image files were changed. No unsupported provider is counted as accepted.
