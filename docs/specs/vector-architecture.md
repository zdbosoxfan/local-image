# Vector architecture — 2026-10-10

This document is the implementation contract for `wip/vector`. It supplements
[the persona spec](vector-persona-spec.md) and [the quality plan](vector-quality-plan.md).
The owner's decisions at the end of the spec and the current task's testing rules take
precedence over their older recommendations. This job supplies shared data, native
persistence, undoable transaction plumbing and compiled engine registration slots. V2–V10,
Vectorizer T2 and Affinity are implementation packages, not features claimed complete here.

## 1. Verified baseline and corrections

Inspected baseline: `a7cf38c7`, branch `wip/codex-vector-arch`. Preserve the previous builders'
code; do not transplant an upstream app or renderer. The initial working tree had only the
untracked task brief, with no separate uncommitted architecture patch to recover.

| Spec §2 statement | Current code, verified 2026-10-10 | Consequence |
|---|---|---|
| Paths, Knot, ShapeLayer, masks and saved paths | `pc-doc/src/vector.rs` still has the same path/component model, `Knot.smooth`, six legacy live shape choices and single fill/stroke. No analytic booleans in this branch. | Preserve PSD ordered Combine/Subtract/Intersect/Exclude/Join semantics. Use V1's adapter before set operations. |
| No text on path, appearance, symbols or live groups | Still true of executable features. Foundations now store their additive data as described below. | Metadata storage is not renderer/tool implementation. |
| `pc-vector` about 2.7k lines, patterns unsupported | `CompiledShape`, `CompiledVectorMask`, sparse bounds-clipped/tile-band render, basic shape gradients and edit/trace primitives exist; pattern paint still returns no paint. No kurbo/lyon in its normal dependencies. | Keep its exact-area coverage; build feature projection around it. |
| Upstream #865/#1321/#747 need syncing | Direct Selection engine/UI/edit files match #865 Git blobs exactly; raster.rs matches #747. **#1321 is missing:** Fill stores midpoint/opacity/offset/dither/align, but Paint::from_fill ignores them using `..`; shape sampling is basic linear stops. Current source was imported in `c18d18cf`. | Retain #865/#747. Package 00 ports the demonstrated #1321 gap before appearance integration, including shape/fill CPU/GPU parity tests. The original whole-app fork hash remains unrecorded; per-file provenance is established below. |
| No trace engine | `pc-trace` T1 is present: checked RGBA input, Potrace/visioncortex fitting, colour clustering, palettes, adaptive threshold, stacked output, cancellation and budgets. `layer.vectorize`/`.preview` use background jobs. | T2 extends T1. LineArt/Sketch are explicitly rejected until centreline lands. T1's competitor/corpus gates remain unestablished; see its report. |
| No tool sets | `pc-engine/src/toolsets.rs`, prefs ToolbarCustomization, `pc-ui-egui/src/toolsets_ui.rs` and click tests exist. Five built-ins: All Tools, Photographer, Essentials, Retouching, AI. `Persona` currently contains Pixel only; tool names are strings, flyouts retained from canonical toolbar. | Extend this model, retain existing IDs/customizations. Do not create a second tool-set store. |
| 53 tools and only monochrome/glyph icons | `Tool::ALL` and engine ALL_TOOLS each list 53 tools (including AI); colour icon sheets, mono icons, themes, tooltips and icon tests already exist. | Persona tags are added to the existing registry; use the colour-icon system and explicit aliases for new tools. |
| Move mostly selection/translation | `move_input`, `move_mods`, `move_ui`, `move_fast`, transform handles, captured press/release, linked/selected-pixel moves, Alt previews, snapping and cached GPU plane runs exist. | New tools reuse capture/modifier contracts; preserve Move's warm no-upload previews and atomic release. |
| GPU only uploads the CPU document composite | `pc-gpu` builds a layer plan and composites supported documents directly into GPU tiles. `gpu_canvas` uses mipmapped Rgba8/Rgba16 textures, CMS/HDR/checkerboard/grid, paging and CPU fallback. Shape/mask coverage remains CPU-generated; `shape_split` separates fill and stroke. | Crisp zoom must feed scaled geometry into this actual compositor/canvas, not just upload a second flattened preview. |
| Workspace switch with persona | Owner chose tools + context bar only. | Dock, Layers, History, current document and selection survive persona switching. |
| Own PDF writer; import deferred | Owner approved full krilla/hayro export/import and embedded fonts. | Replace vector PDF path with the pinned libraries; keep raster print command until routing is integrated. |
| Default design units undecided | Owner chose pt/mm for new design documents, px for photographs. | Geometry remains f64 document pixels; UI units use DPI. Legacy documents default px. |
| Optional Affinity import | Owner approved later `.afdesign` and `.afphoto` import. | A separate bounded-reader package; no native Affinity exporter. |

Per-file upstream verification used the official PhotoCraft commit API and Git blob hashes:
[#865 / 75ff41a26beb445413f6ee9248eb40b42152bd77](https://github.com/storytold/photocraft/commit/75ff41a26beb445413f6ee9248eb40b42152bd77),
[#747 / 8302f57190c83439adb961ae23b9b110e819dea3](https://github.com/storytold/photocraft/commit/8302f57190c83439adb961ae23b9b110e819dea3),
and [#1321 / d438f842256435dba7aa41ca306656a216b98fbf](https://github.com/storytold/photocraft/commit/d438f842256435dba7aa41ca306656a216b98fbf).
The #865 vector lib is exactly our baseline, and is 78 commits behind #1321 in that
comparison. Diffing the latter's vector lib identifies ignored gradient fields and missing
sorting/layout/dither/midpoint/opacity behavior. Saved API/hash/diff evidence is under
`target/vector-arch/`; links above remain the durable primary sources.

Draft/Refinement (`ai_ui`, `generate_ui`, `li-ai/catalog`) and People
(`lc-ui-egui` Library/People, `li-seg`) are current parallel product features. They do not
change the vector model. Library vectorizer handoff must use the existing module/open bridge;
it must preserve People/Library state and reuse existing upscale services if selected.

## 2. Final library decisions and dependency policy

The foundation changes add **zero external registry packages**. Engine GPU tests promote
already-locked pc-gpu/wgpu/pollster to dev dependencies; no production dependency is added.
Generic “no new dependencies” rules apply to all packages except the owner's explicitly
approved V1 and PDF dependency bootstrap, performed by package 00. Jobs build offline.
Never let an ordinary feature job upgrade the lock or resolve a new default feature set.

| Concern | Final selection / versions | License and evidence |
|---|---|---|
| Curve operations | V1 `pc-pathops`, VectorCraft wrappers; linesweeper **=0.4.0**, kurbo **=0.13.1**, polycool **=0.4.0** | MIT OR Apache-2.0; official [linesweeper docs](https://docs.rs/linesweeper/0.4.0/linesweeper/), [kurbo docs](https://docs.rs/kurbo/0.13.1/kurbo/), cached manifests. Polycool 0.4.0 is **already** locked via kurbo; the old “two new packages” assertion is stale. Linesweeper is beta; quality-plan oracles/retry/ratchets remain mandatory. |
| Tessellation | No lyon addition | [lyon docs](https://docs.rs/lyon/latest/lyon/) describe flattening into triangles. It adds no needed capability to the exact-area CPU coverage path. |
| Raster/text | Existing pc-vector + pc-text (parley/harfrust/skrifa versions in our lock) | Existing attribution/license records. Do not take VC vello_cpu 0.2, skrifa 0.47 or harfrust 0.13. vello_cpu 0.1 already being transitive is not approval to upgrade. |
| SVG | usvg/resvg **0.45.1**, svgtypes **0.15.3**, roxmltree **0.20.0**; resvg test oracle only | MIT OR Apache-2.0; locked source/feature graph and [usvg 0.45.1](https://docs.rs/usvg/0.45.1/usvg/). Current lock has no usvg fontdb/text graph; placeholder XML extraction through pc-text is mandatory. No upgrade to VC's usvg 0.48. |
| PDF export | krilla **=0.8.2**, default-features=false, features=["raster-images"]; no `simple-text`, no `pdf` feature | MIT OR Apache-2.0; exact VC Cargo.lock pin and [krilla manifest](https://docs.rs/crate/krilla/0.8.2/source/Cargo.toml.orig). pc-text supplies positioned glyphs/font bytes; krilla/subsetter embeds them. The entire Image module is behind raster-images, including from_rgba8/custom, so this audited feature is required for full export/fallback. Avoid a second shaper. |
| PDF import | hayro-syntax **=0.8.0**, hayro-interpret **=0.8.0**, embed-fonts + embed-cmaps | Apache-2.0 OR MIT; VC PDF manifest and [Device API](https://docs.rs/hayro-interpret/0.8.0/hayro_interpret/trait.Device.html). Use interpreter callbacks to build editable layers, not hayro's standalone renderer/vello stack. |
| Affinity | Port VC `crates/affinity` at `d522c1d7…`; crc32fast, flate2 rust_backend, image, ruzstd **0.9.1** (RUSTSEC-2026-0344 block bounds) | MIT OR Apache-2.0. Read-only upstream container/stream/model/geometry/paint/text/raster/preview/access modules. Bounded best-effort import with warnings, not a claim of complete proprietary-format support. |
| Vectorizer | Existing pc-trace ports; kurbo fitting; T2 VC centreline/recognize algorithms | vtracer/visioncortex MIT OR Apache-2.0, Potrace GPL-2.0-or-later compatible with GPL-3.0-or-later. No vtracer/visioncortex/flo_curves dependency addition. |
| Paragraph composer | DesignCraft `crates/compose` at `66c7ce7e…`, optional V6c port onto pc-text | MIT OR Apache-2.0. Composer only, not its document tree or renderer. |

Selected-feature audit resolved offline under `target/refvec/vector-pdf-audit`, seeded
with the pinned VC lock. The audit and the crate/version/license/features/checksum allowlist
are in git history.
It contains 82 registry packages, **19 additional crate/version pairs** relative to this
branch's lock; only imagesize is added by enabling raster-images beyond the 18-package
vector/text/interpreter graph. Every additional package is Rust with no custom build script;
none is C/-sys. The four existing build scripts perform Rust cfg/version detection. This is
an isolated metadata audit, not production dependency registration or a PDF rendering test.
Package 00 retains existing pins when unifying the full workspace features, compares the
result against this allowlist, rejects new C/-sys or incompatible licenses, and regenerates
attributions. Offline fresh resolution hits yanked yoke-derive 0.8.3; **reuse the existing
locked entry**, which both current and VC locks already contain. Do not delete/rebuild locks
or silently upgrade yoke to work around it. Owner-approved bootstrap requires no new
permission question.

Important distinction: hayro-interpret 0.8.0 unconditionally requests hayro-syntax's
`unsafe` feature, which enables pure Rust SIMD codecs. Disabling interpreter defaults does
not remove it. The workspace `deny(unsafe_code)` applies to **our** crates, not upstream
registry dependencies (the existing image/GPU stack already uses unsafe internally).
No unsafe code may be added in our adapters. If a future policy instead forbids unsafe
in the entire dependency graph, this pinned interpreter needs an audited local patch;
do not silently claim a safe-only dependency graph. No C/-sys addition is needed by the
selected PDF libraries. The same audit applies to feature unification of flate2.

## 3. Shared document and persistence foundation (implemented)

`pc-doc::vector_design` owns data only, split into appearance, drawing, text, live and
structure modules. In-memory Layer/Document are not serde models; `pc-format::manifest`
is their persisted mirror and `convert.rs` copies the fields in both directions.

```rust
pub struct Layer { /* existing fields */ pub vector: VectorData }
pub struct Document {
    /* existing fields */ pub symbols: Vec<SymbolDef>, pub design_units: DesignUnits,
}
pub struct VectorData {
    pub corners: Vec<KnotCorner>,
    pub live_shape: Option<DesignShape>,
    pub appearance: Option<Appearance>,
    pub on_path: Option<PathText>,
    pub live: Option<LiveGroup>,
    pub constraints: Option<Constraints>,
    pub clip: Option<Path>,
}
pub struct SymbolDef { pub id: SymbolId, pub name: String, pub layers: Vec<Layer> }
pub struct SymbolInstance { pub symbol: SymbolId, pub transform: Affine, pub cache: Option<Surface> }
```

This is a deliberate organization refinement to the spec's illustrative §5 structs:
`Knot.corner` is indexed `Layer.vector.corners`, `ShapeLayer.appearance` is
`Layer.vector.appearance`, `TextShape::OnPath` is `Layer.vector.on_path`, `Group.live`
is `Layer.vector.live`, and `Layer.constraints` is `Layer.vector.constraints`.
It preserves Copy Knot/TextShape and the exhaustive legacy content/live-shape matches,
so geometry, paint, text and structure jobs do not all modify those core types. No change
to the PSD path byte representation or to legacy rendering follows merely from this data.
Do not later relocate these fields without a native migration.

| Data | Contract / owner |
|---|---|
| Corner/KnotCorner | Radius, Round/Inverted/Chamfer/Straight; indexed by base subpath/knot. Node insert/delete/reorder remaps indices in one edit. Affine bake scales or bakes corners according to the geometry package, not by moving indices. Package 06 evaluates them. |
| DesignShape | `[x,y,w,h]`, angle in radians, typed ShapeParameters for every §7 shape. Generated base Path is authoritative for legacy rendering/export. Package 02 implements all generators/parameter validation. Old LiveShape remains accepted. |
| Appearance | items bottom→top; None=legacy fill/stroke, Some(empty)=unpainted. FillItem paint/opacity/blend/visible; StrokeItem base/profile/brush/arrows/dash_align/scale_with_object/gradient_mode. Profile points use normalized arc length and absolute left/right pixel distances. Package 08 owns paint projection; 11 width; 17 brushes. |
| PathText | Owned text-space Path; normalized start/end arc lengths, flip and baseline/ascender/center/descender alignment. TextLayer still owns runs/text/transform. Package 12 evaluates pc-text glyphs and overflow. |
| LiveGroup | Blend spacing/orientation/spine, radial/grid/mirror repeats and warp/mesh/top-object envelope. Sources are the ordinary group's children in paint order, no duplicate object tree. Packages 18–20. |
| Constraints | Edge pins/fixed width/fixed height in parent space. Both pins + fixed width/height uses fixed-size alignment rule documented/tested by package 16; reject nonsensical or unsupported rotated combinations. |
| SymbolDef / SymbolInstance | Master layers include normal caches/blob payloads. SymbolId is a separate namespace, preserved when LayerIds are remapped. Package 15 validates duplicate IDs/cycles and adds instance content. |
| DesignUnits | Pixels(default)/Points/Millimeters/Inches; px=pt×dpi/72 and px=mm×dpi/25.4. Document pixels remain all geometry's storage units. New design preset chooses pt or mm; old/photo documents px. |

All new persisted optional/default values have serde defaults and skip-if-empty/default
attributes. Native symbol master tiles/blobs use the **same** Sink/Fetch, ID remapping,
size limits, incremental directory storage and GC as other layers. History pixel-budget
accounting walks symbol masters too, deduplicating shared COW tiles. Both master and root
layer depth are bounded by MAX_GROUP_DEPTH; manifests are decoded layer-by-layer for both.
The frozen `pc-format/tests/fixtures/vector-v1.pcraft` loads without any extension field,
and extended ZIP/directory documents round-trip. A symbol master is a template, not a new
content variant. FORMAT_VERSION remains **1** here. Package 15 adds `LayerContent::Symbol`
and `ContentM::Symbol`, sets FORMAT_VERSION=2 and adds the no-op v1→v2 migration **together**.
That package owns the simulated v1 TooNew test. It cannot serialize instances before then.

## 4. Geometry API: V1 contract and dependency direction

V1 is being finished in `wip/codex-vector-v1`. Its current public adapter was read directly;
do not create a duplicate pc-pathops stub/crate or copy unfinished algorithms into this branch.
The foundation command is path-data-only and has no link-time V1 dependency. Following
packages use these actual exports after V1 integrates:

```rust
validate(&Path) -> Result<(), PathOpsError>
finish_compound(&Path, Option<kurbo::Rect>) -> Result<Path, PathOpsError>
boolean(&Path, &Path, BoolOp) -> Result<Path, PathOpsError>
area(&Path) -> Result<f64, PathOpsError>
pathfinder(PathfinderOp, &[Shape]) -> Result<Vec<Shape>, PathOpsError>
regions(&[Shape]) -> Result<Vec<Region>, PathOpsError>
shape_builder(&[Shape], edges: bool) -> Result<BuilderArrangement, PathOpsError>
merge_regions(&[Shape], &[&Region]) -> Result<Path, PathOpsError>
offset_path(&Path, delta: f64, Join, miter: f64) -> Result<Path, PathOpsError>
outline_stroke(&Path, &ShapeStroke, tolerance: f64) -> Result<Path, PathOpsError>
simplify(&Path, tolerance: f64) -> Result<Path, PathOpsError>
smooth(&Path, amount: f64) -> Result<Path, PathOpsError>
reverse(&Path) -> Result<Path, PathOpsError>
join(&[Path], tolerance: f64) -> Result<Path, PathOpsError>
split_at(&Path, subpath: usize, segment: usize, t: f64) -> Result<Vec<Path>, PathOpsError>
contains(&Path, Point) -> Result<bool, PathOpsError>
nearest(&Path, Point) -> Result<Option<(usize, usize, f64, Point, f64)>, PathOpsError>
fit_path(&[Point], tolerance: f64) -> Result<Path, PathOpsError>
bounds(&Path) -> Result<Option<(f64, f64, f64, f64)>, PathOpsError>
```

Shape `{path,key:u64}`, Region `{path,sources:Vec<usize>}` refer back to input paint order;
BuilderArrangement has regions/lines/edges. BoolOp is Union/Difference/Intersect/Xor;
Divide is pathfinder/regions, not an invented binary BoolOp variant. Normalize ordered PSD
components before booleans. Output holes are Join in their owning component, not independent
Combine subpaths. Fill inversion requires a finite clip rect. Open fills close with a
straight edge; unused endpoint handles must not create a closing curve. Errors include
NonFinite/OpenPath/Degenerate/NeedsClip/InvalidOption; never hide failures by committing
an empty path. V1-Q owns coverage/EDT/property oracles and robustness retries.

Important dependency correction: the current V1 adapter depends on pc-vector for native
outline-stroke polygons. Therefore pc-vector **must not** depend back on pc-pathops.
Variable widths, corners, brushes and live expansion project to ordinary Paths in the
engine or pc-vecfx; the CPU renderer consumes the projection. Analytic curve bounds use
kurbo through the adapter, not a `Path::bounds` impl in pc-doc that would invert layering.
`pc-trace` stays independent of pc-pathops.

## 5. Commands, transactions and UI hooks

Implemented `pc-engine::vector_api`:
- `snapshot_shapes(doc, ids) -> Result<Vec<ShapeEdit>>`: detached clone, validates targets,
  duplicates and inherited all/pixel locks before render work.
- `ShapeEdit { layer, shape, vector }`: package-owned geometry/cache and additive metadata.
- `apply_shape_edits(session, label, edits)`: rejects nonfinite paths and absent caches,
  revalidates all targets, publishes through one Session::edit. Errors publish no document,
  revision, history or pixels. Package renderers prepare caches before committing.
- Journaled automation command `shape.applyPaths { edits:[{layer,path:serde Path}] }`:
  prepares existing pc-vector caches, clears stale live/corner metadata, and edits multiple
  shape paths in one step. Appearance edits require the appearance renderer. Selection stays
  with the existing session; Session::execute owns journal/coalescing.

`vector_packages.rs` registers 26 compiled, initially empty command slots. Each package
owns its file/subdirectory, calls ordinary Session commands for UI commits, and adds
journal=true specs there. Empty slots expose no fake successful command. Existing V1 and
legacy commands are not renamed or duplicated. Higher-level create/delete/reparent/symbol
edits use Session::edit directly with the same preflight discipline.

Tool actions follow VectorCraft Begin/Preview/Commit/Exec: Begin captures IDs, document ID
and revision plus pointer capture; Preview computes detached model/projection/overlay;
Commit first checks doc/revision still match and issues one engine command; Escape/focus
loss/persona switch cancels and drops transient caches. Modifiers are sampled each frame,
not latched on pointer-down. Never journal previews or re-render all document layers.

UI package 01 is the **single owner** of persona/Tool/toolbar/prefs registry wiring. Feature
packages implement new `pc-ui-egui/src/vector_packages/<feature>.rs` modules and test those
with a small egui_kittest host. They provide registration fragments (tool ID, label/icon,
context/paint/input entrypoints, State type and prefs fields) in their own integration notes.
Package 00 alone applies shared `lib.rs`, canvas/router, menu, i18n catalog, crate manifest
and layering registrations as those artifacts arrive. This serial merge lane is explicit:
it is not permission for two parallel jobs to edit a shared switch or a translations file.
Full app kittest/real-GPU acceptance runs after wiring, not on an unregistered widget.

Persona::Vector extends the existing Pixel toolset model. Persist persona, each persona's
last tool/set and custom sets with defaults; preserve current allTools/photographer IDs.
Vector Photographer is a paired family member, not an overwrite of the Pixel list. New
vector Pencil must have a distinct tool ID from today's raster Pencil. Registry generates
All Tools and carries family tags. Shared tools remain shared. Panels/history/selection stay
put. Tool switch mid-gesture cancels cleanly; pixel-on-vector prompts rasterization and
vector-on-pixel creates a new shape. Menus expose both families and resolve enablement
through engine preconditions.

## 6. Rendering and crisp view contract

Only pc-vector rasterizes filled Paths and only pc-text shapes text. A feature projection
expands live geometry, widths/brushes and symbol instances into ordinary Paths/layers plus
appearance metadata. Exports and display consume the same projection; do not invent a
separate PDF/SVG geometry evaluator. Projection results are transient and keyed by source
revision, appearance, relevant pattern/font/master generation and view transform.

Appearance rendering must accept a layer's VectorData and document pattern context in a
new package-owned API; render legacy shapes through render_shape unchanged. Reuse
pc-color blend math (where pc-compose ultimately gets it), never depend pc-vector→pc-compose.
Merge stack paint bottom→top; maintain legacy fill/stroke mirrors for PSD. Enhanced shapes
use merged shape coverage for clipping, with the decision integrated in shape_split and
pc-gpu plan. Pattern paint gains pattern ID/phase/scale/rotation context. Masks, effects and
blend are then applied by the normal compositor.

Proposed display interface owned by package 09:
`flatten_scaled(doc, visible_doc_rect, scale) -> Result<ViewTiles, ViewError>` plus an
analogous GPU render plan. A ViewRequest identifies doc revision, view key, logical zoom,
physical pixels-per-point, visible rect, scale bucket and display-CMS generation. Desired
scale=max(1, logical_zoom×pixels_per_point), so a 200% HiDPI display is not undersampled.
Scale bucket caches reuse nearby zooms; the exact transform is retained for sampling.
Never replace persisted document caches or history while building display tiles.

Project vector paths/text/masks into device-space tiles using a single affine; raster layers
sample in document space. Render the complete affected stack, including clipped raster
layers, groups, masks, effects and adjustment semantics. Required padding accounts for
stroke joins/arrows, mask feather and effect spread/blur; crop after rendering. All tiles
share the global origin so dither/gradients/pattern phase do not seam. Unsupported GPU plans
fall back to the **same scaled** CPU reference, not the 1x cache. Refresh only damaged tiles;
master edits invalidate instances; font/appearance changes invalidate their dependents.

Extend existing gpu_canvas ViewParams/texture tile keys through package 00. Preserve
mips, HDR/CMS/checkerboard/grid and Move planes. During captured gestures, use existing
transient preview planes where compatible, with scaled reprojection for vector paths;
release publishes once. Mask/effect expansion prevents seams. Export at 1x unless @2x/@3x
is requested, then use the same projection/scale API.

Acceptance: RTX 5090 Vulkan, premultiplied max diff≤1/255 at 2/4/8x, tiled=untiled, an 800%
1px hairline has an 8px sharp profile, mixed raster/vector clipping and effect padding,
HiDPI views and cancelled gestures. An offscreen **full app** pointer-drag benchmark
measures UI and GPU/readback median/p95/p99, cache uploads/rebuilds and first/release costs.
Do not infer interactive speed from a library microbenchmark. Target <16ms warm node-edit
frames on 6000×4000/500 shapes; boolean 1000 nodes<50ms; SVG 2MB<1s (release on real PC).
GPU failures/software adapters are failures of the gate, not successful skips.

## 7. Format layering and fidelity

`pc-svg`, `pc-pdf`, `pc-affinity` are lower-level adapters using the compiled
`pc-doc::vector_design::exchange::{ImportOptions, ImportLimits, ImportedScene, ExportScope,
ExportedVector, VectorWarning}` boundary. ImportedScene carries size/DPI/units, layers,
symbol masters and warnings; ExportedVector carries bytes and warnings. They do not depend on engine/UI. Engine
slots own file commands; UI owns dialogs/handoff only. Package 00 adds internal crate
registrations and selected already-locked/approved dependencies once, and attribution discovery.
Use `ImportLimits { bytes,pixels,objects,depth,segments,decompressed_bytes }`, cancellation,
no network/resource resolution, and externally supplied font/image resolution policies.
Malformed input is an error; unsupported editable features produce one aggregated warning.

SVG paths: [SVG 1.1 path syntax](https://www.w3.org/TR/SVG11/paths.html) controls close vs
manual line closure, arc-to-cubic conversion and fill-rule. [Painting](https://www.w3.org/TR/SVG11/painting.html)
controls dash units/caps/joins. Convert SVG absolute dash lengths into our width-relative
ShapeStroke convention; handle width=0 without division. usvg expands CSS/use/transforms,
clips/masks and images. Reinsert text extracted with placeholder IDs before normalization,
shape with pc-text, preserve editable text when resolvable and outlines otherwise. Design
units/viewBox/DPI stay explicit. Export selection/artboard/document, appearance as stacked
paths and gradients as defs; rasterize unsupported paint/effects/adjustments with warnings.
Use XML escaping, bounded embedded images, stable defs/decimals and no external URLs.

PDF: use [ISO 32000/PDF 1.7 reference](https://opensource.adobe.com/dc-acrobat-sdk-docs/pdfstandards/pdfreference1.7old.pdf)
for graphics state, path painting, clipping, shadings, soft masks, transparency groups,
text and embedded fonts. krilla handles object streams/resources/fonts, not a parallel
hand-written writer. Map top-left y-down pixels to PDF point/page coordinates via DPI and
one transform. Export pre-shaped pc-text glyph IDs/advances/fonts with Unicode mapping;
outline only where embedding is unavailable or chosen, with a warning. Vector-only output
contains no image XObjects. hayro Device callbacks preserve drawing order, clips, groups,
images and glyph outlines/text where reconstructable. Missing semantic information in a PDF
must not be invented; report outline/raster fallbacks. Respect CropBox/MediaBox, rotations,
page selection, encryption/error behavior and decompression/recursion limits. Import/export
jobs share a documented report/options boundary, not each other's private modules.

Affinity: bounded archive→object stream→model adapters (VC reader and PhotoCraft affinity
reader are references), preserve imported curves/text/paint/groups and available pixels.
Unsupported records are warnings and safe raster/preview fallback; unsupported/corrupt
archives cannot be accepted as empty documents. Tests use own small synthetic records,
plus licensed owner files read-only when supplied. No product-brand assets are copied.

PSD: `pc-io` remains the PSD adapter. All advanced packages list their editability losses
through a shared export warning collector owned by package 23. Native caches export pixels
plus the supported fill/stroke/vector blocks; no bogus editable live feature is advertised.
Spec prose disagrees between “first” and “topmost” paint mirror: final contract is the
**topmost visible supported fill and stroke**, matching §5 authoritative appearance, with
merged rendered pixels authoritative for fallback. Export warning names every lost feature
once. Existing raw vector blocks remain byte-identical until corresponding model edits.
PSD corpus gates run when corpus files are actually available; missing corpus is reported.

## 8. Independent implementation packages and merge contract

Engine/data/API packages can start
on these foundations immediately. Geometry consumers integrate on V1's actual exports;
UI wiring and manifest registration are one serial coordinator lane. Rendering consumers
use detached projection fixtures until their renderer dependency is integrated. Each task
lists owned files, exact acceptance and dependencies. Package 00 alone resolves central
file changes; other jobs leave a precise integration fragment and then complete their own
full app acceptance once integrated. No task claims “done” merely because an isolated stub
compiled. This job does not implement their later scope.

References: VectorCraft read-only source at `~/.local/share/local-image-dev/engine-sources/vectorcraft`
(pin above); `crates/{geom,pathops,tools,doc,brush,text,svg,pdf,affinity}`. DesignCraft composer
[at the spec pin](https://github.com/storytold/designcraft/tree/66c7ce7e96ec286a091878cc2541ee8369aa79d6/crates/compose)
is optional. [Affinity personas](https://affinity.help/designer2/en-US.lproj/pages/Introduction/about_Personas.html),
[Corner tool](https://affinity.help/designer2/en-US.lproj/pages/Tools/tools_corner.html) and
[Inkscape Shape Builder behavior](https://wiki.inkscape.org/wiki/Release_notes/1.3) guide
interaction, not a transplanted object tree. Current Move/GPU/trace reports are evidence
of existing code and prior measurements, not measurements made by this architecture job.
