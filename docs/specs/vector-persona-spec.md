# Vector Persona: build spec (Codex task brief)

Status: research + spec, 2026-10-09. Scope: **the full professional vector tool set**
(Affinity Designer / Illustrator class). The owner set this scope on 2026-10-09: the app serves graphic designers and
compositing experts, not only photographers. Photographers get a curated **Photographer** tool-set
preset, which is a subset of the tools. The phases below are ordered by dependency and by value to designers. Nothing in
the scope has been cut; the later items are simply scheduled later.

Repository: `/home/zdavidson/Documents/Local Image - Testing`. Integration branch: `claude/sleepy-franklin-egimjb`
(see the branch strategy: `wip/*` → that branch → a PR into v2).

---

## 0. What we are building (one paragraph)

Inside the **Compositing** module there will be an Affinity-style **persona toggle, Pixel ⇄ Vector**. Switching it
changes the toolbar's tool family, the context bar and the default panel workspace. It keeps
the **same document, the same Layers panel and the same history**. Vector objects are ordinary layers:
shape layers, groups, type layers, plus new symbol instances. This is the Affinity Designer model, where every curve
is a layer, so retouchers and designers share one file. Every object stays editable and resolution-independent in
the model. It is rasterised on the CPU into the layer cache by `pc-vector`, and the GPU compositor
composites that cache, so CPU/GPU parity holds by construction. The geometry engine (booleans, offset,
outline stroke, shape builder, width profiles, brushes, blends, trace, SVG mapping) is **ported from
VectorCraft** (storytold/vectorcraft, MIT OR Apache-2.0, pure Rust). It is adapted to our document types, not
transplanted with VectorCraft's own document/renderer.

---

## 1. Hard rules for the implementer

1. **Pure Rust, no `unsafe`, no C or `-sys` crates.** The workspace denies `unsafe_code`.
   - **The only new crates allowed are `linesweeper` 0.4.0 and `polycool` 0.4.0.** Both are MIT OR Apache-2.0 and pure Rust. Together they are VectorCraft's robust curve-boolean engine, and they arrive in Phase V1.
   - Their other dependencies (`arrayvec` 0.7.8, `smallvec` 1.16.2, `rustc-hash` 2.1.3, `kurbo` 0.13.1) are already in our `Cargo.lock`.
   - The coordinator must run `cargo fetch` (with network) before the Codex job, because the sandbox has no network.
   - Any other new `[[package]]` in `Cargo.lock` is a review failure. Check with `git diff Cargo.lock`.
   - Making already-locked crates direct dependencies is fine and adds no packages: `kurbo` 0.13.1, `usvg` 0.45.1, `svgtypes` 0.15.3, `roxmltree` 0.20.0, `flate2`, `image`.
2. **Do not adopt VectorCraft's renderer** (`vello_cpu` 0.2 is not in our lock, and only 0.1 arrives via epaint), its text stack
   (skrifa 0.47 / harfrust 0.13; we have 0.44 / 0.12 via parley), krilla or hayro. **`pc-vector` stays the single
   vector rasteriser**: it is exact-area, PSD-calibrated and already tested against Photoshop corpora. **`pc-text` stays
   the single text engine.**
3. **Old documents must load unchanged.**
   - Every new persisted field is `#[serde(default, skip_serializing_if = …)]`.
   - New enum variants are additive.
   - When the first variant that older builds can't read is written (Phase V8, `ContentM::Symbol`),
     bump `pc-format` `FORMAT_VERSION` to 2 and add a no-op `STEPS[0]` in `crates/pc-format/src/migrate.rs`. Older
     builds then report `TooNew` instead of "corrupt".
   - Add a test that loads a v1 `.pcraft` fixture containing shapes, vector masks and paths.
4. **PSD round trips must not regress.** `crates/pc-io/tests/vector.rs` (including the corpus tests
   `corpus_shape_coverage_matches_photoshop` and `corpus_vector_blocks_survive_roundtrip`) must stay green. Features PSD can't
   express (multiple fills/strokes, width profiles, live blends, symbols, text on path until V6b) export
   as the layer's rendered pixels plus the first fill/stroke as `vscg`/`vstk`. The export also shows one warning
   listing what lost editability.
5. **GPU/CPU parity.**
   - Every new paint feature gets a `pc-gpu` parity test in the pattern of
     `crates/pc-gpu/tests/parity.rs` / `vector_mask.rs`: `photocraft_compose::flatten(d)` vs
     `photocraft_gpu::render_to_vec`, premultiplied, max diff ≤ 1/255, skipped without Rgba32Float.
   - Every new geometry feature also gets an analytic `pc-vector` test (areas, bounds) like `crates/pc-vector/src/tests.rs`.
6. **The engine first, then the UI.** Every tool gesture ends in an engine command (`crates/pc-engine/src/vector_cmds.rs`
   style, registered like the other `*_cmds` modules). Commands are journaled and undoable, and they can be driven by the
   automation/MCP channel. Tools follow VectorCraft's state-machine contract (`vectorcraft-tools` lib.rs: pointer
   events in, `Begin/Preview/Commit/Exec` actions out). Our existing equivalents are `vector_ui.rs`/`direct_select.rs`.
7. Use the toolchain `cargo +1.98.1 --offline`, `CARGO_BUILD_JOBS=3`, and keep `cargo +1.98.1 fmt --all` and
   `clippy -D warnings` clean. There is no `unwrap()`/`expect()` in shipped code of new crates; use VectorCraft's own lint set
   (`unwrap_used/expect_used/panic = "deny"`) in `pc-pathops`/`pc-svg`.
8. **Attribution for every port** (§8): a module doc naming the upstream file, a `docs/PORTS.md` row, and
   `licenses/vectorcraft-{LICENSE-MIT,LICENSE-APACHE,NOTICE}`. Before porting, run `cargo xtask` and check the attribution list
   (`assets/attributions*.json`, `crates/li-attributions`) the same way PhotoCraft/LightCraft are listed.
   VectorCraft's *brand* assets (docs/brand) are **not** open source and must not be copied. Its Lucide icons are
   ISC, and are only to be used if our icon set needs them.
9. i18n: every new UI string goes through the existing `.tsv` catalogs (`crates/pc-ui-egui/src/i18n/`).
10. No network in tests. SVG and PDF fixtures are small files written by the tests, or checked-in text fixtures under 50 KB.

---

## 2. Findings: what we already have (evidence)

All paths are relative to the repository root.

### 2.1 Model (`crates/pc-doc/src/vector.rs`, `lib.rs`, `text.rs`)
- `Knot { anchor, in_ctrl, out_ctrl, #[serde(default)] smooth }`, `Subpath { closed, knots, op: PathOp }`,
  `Path { subpaths, fill_rule, inverted }`. `PathOp` is `Combine | Subtract | Intersect | Exclude | Join` (PSD
  semantics: the first subpath is always Combine, and `Join` = a hole belonging to the previous component).
- `ShapeLayer { path, fill: Option<Fill>, stroke: Option<ShapeStroke>, live: Option<LiveShape>, cache, psd_raw }`.
- `ShapeStroke` (`#[serde(default)]`): width, paint (`Fill`), opacity, align (Inside/Center/Outside), cap, join,
  miter_limit, dashes, dash_offset.
- `LiveShape` (`#[serde(tag="kind", rename_all="camelCase")]`): `Rect{rect, radii}`, `Ellipse`, `Polygon{sides, star_ratio}`,
  `Line`.
- `Layer.vector_mask: Option<VectorMask{path, enabled, linked, density, feather}>`. The document has
  `paths: Vec<NamedPath>` (Paths panel), `work_path` and `clipping_path`.
- `LayerContent { Raster, Group, Adjustment, Fill, Text, Shape, Smart }` (`lib.rs:446`). Artboards are groups with
  `artboard: Option<Artboard>` (`comps.rs:155`).
- `TextLayer` has `shape: TextShape { Point, Box{..} }`, `warp: Option<TextWarp>`, runs and paragraphs. **There is no text on a path.**
- Persistence: `crates/pc-format/src/manifest.rs` has `ContentM` (`#[serde(tag="kind", rename_all="snake_case")]`), and
  `ContentM::Shape{fill, cache, psd_raw, path, stroke, live}` uses defaults on the newer fields. `FORMAT_VERSION = 1`, and
  `migrate.rs` has empty `STEPS`.

### 2.2 Geometry and raster (`crates/pc-vector`, about 2.7 k lines, internal deps only)
- An exact-area scanline coverage rasteriser (`raster.rs`, one winding counter per component) and
  flattening (`flatten.rs`).
- Strokes are polygons built from quads, join wedges and caps, plus dashes (`stroke.rs`).
- Shapes (`shapes.rs`): rect, rounded rect, ellipse, polygon/star, line.
- Node editing primitives (`edit.rs`): move anchors/handles, convert point, bend segment, marquee, hit test.
- Mask tracing (`trace.rs`): marching squares + Schneider fit.
- Paint: solid and gradients (linear/reflected/radial/diamond/angle). **Patterns are not rendered.**
- **Booleans exist only at coverage level** (the PathOps are folded while rasterising). **There are no geometric
  booleans, no offset/outline-path, no analytic curve bounds**, and no use of kurbo/lyon.

### 2.3 Rendering
- `ShapeLayer.cache` is filled by `photocraft_vector::render_shape(sh, fmt, doc.bounds())`
  (`crates/pc-engine/src/vector_cmds.rs:433`, `crates/pc-io/src/psd_import.rs:326`).
- pc-compose treats the cache as a surface.
- pc-gpu (`plan.rs:476`, `:660`) uploads CPU-made shape caches and CPU-made vector-mask coverage
  (`compose::masks::combined_mask`), with fill and stroke split by `compose::shape_split`.
- Parity tests that already cover vectors are in `crates/pc-gpu/tests/parity.rs`:
  - `layer_effects_on_shape_layers`
  - `stroke_effects_on_filled_and_stroked_shapes`
  - `type_layers_blend_with_text_gamma`
  - `stroked_shapes_with_clipped_layers`
- `crates/pc-gpu/tests/vector_mask.rs` has its own parity test.
- **Consequence:** the cache is in document pixels, so zooming past 100 % shows pixels. This is fine for photos, but
  not acceptable for designers (see Phase V5, "resolution-independent view").

### 2.4 UI and engine
- `enum Tool` (`crates/pc-ui-egui/src/state.rs:64`, 53 tools). Its vector members are `Pen, PathSelection, DirectSelection,
  Rectangle, EllipseShape, Triangle, Polygon, Line, CustomShape, Type, VerticalType`.
- The toolbar is `TOOL_SECTIONS` (`panels.rs:17`), with flyout slots.
- The vector UI is `vector_ui.rs` (pen, shape preview, options bar, stroke type/align, Paths panel) and `direct_select.rs`.
- Engine commands:
  - `shape.create/edit/info/rasterize`
  - `path.set/list/info/rename/delete/fill/stroke/toSelection/transform`
  - `path.moveAnchors/moveHandle/convertPoint/bendSegment`
  - `layer.vectorMask.*`
  - `type.createWorkPath`, `type.convertToShape`
  - `align_cmds.rs` (align/distribute) and `artboard_cmds.rs`
- **There is no path-operation UI, no tool sets** (only Photoshop-style *workspaces*: `workspace_ui.rs:101`
  `["Essentials","Photography","Painting","Pixel Art","Graphic and Web","Motion"]`), and **no persona**.
- The module switcher is `enum Module { Library, Develop, Compositing }` (`crates/pc-ui-egui/src/lib.rs:22`, ⌘⌥1/2/3, title bar).
  `docs/WORKSPACE-REFERENCES.md:7` already cites Affinity Personas.
- egui_kittest is a dev-dependency (`crates/pc-ui-egui/Cargo.toml:54`). The test pattern is
  `src/path_selection_tests.rs` / `direct_select_tests.rs`: `Session::new()`, `file.new`,
  `Harness::builder().with_size(vec2(1280,800)).build_eframe(..)`, and `h.state_mut().ui.tool = …`.

### 2.5 Formats
- **PSD** vector data is complete: `crates/pc-psd/src/path.rs` and `crates/pc-io/src/vector_map.rs` cover saved paths, the work path,
  the clipping path, `vmsk`/`vsms`, `vscg`, `vstk` and `vogk`, and they keep stored blocks byte-identical while unchanged.
- **SVG**: none. usvg/resvg 0.45.1 are only transitive (egui_extras icons).
- **PDF**: only the raster print writer (`crates/pc-engine/src/print_cmds.rs:133`, a hand-written one-page PDF).

### 2.6 Upstream PhotoCraft has newer vector work
`storytold/photocraft` `crates/vector` has the following commits, all dated after our fork:
- `75ff41a26b` (2026-10-08): "Direct Selection tool: reshape pen and shape paths after they're drawn" (#865)
- `d438f84225` (2026-10-08): "Shape gradients: honour midpoints, opacity stops, offset, dither and align" (#1321)
- `8302f57190` (2026-10-07): "Fix vector pixel bounds overflow" (#747)

PhotoCraft also gained a `crates/affinity` reader. The fork pin of our pc-* crates is not recorded in
`docs/PORTS.md`, so the coordinator must find it (§9) before Phase V0.

---

## 3. Findings: the ArtCraft siblings

Source of the GitHub org (`storytold`): the release JSON URLs in
`~/Documents/Codex/2026-10-06/https-getartcraft-com-apps-https-getartcraft/work/downloads/*-release.json`
(for example `api.github.com/repos/storytold/photocraft/...`). `gh repo list storytold` shows
the following repositories, all public and all Rust:
- vectorcraft, designcraft, photocraft, lightcraft, pdfcraft
- deckcraft, wordcraft, cadcraft, gridcraft, soundcraft, filmcraft, effectcraft
- craft-libs (empty so far), craft-fonts

### 3.1 VectorCraft: https://github.com/storytold/vectorcraft (Illustrator clone)
- Pin: `d522c1d7be4035bd4f4a84cd6ebfca44f5155092` (main, 2026-10-09, "Merge pull request #800").
- **Licence:** `NOTICE` and `Cargo.toml` say MIT OR Apache-2.0 (GitHub shows Apache-2.0), and both are compatible with GPL-3.0-or-later.
  The brand assets are excluded.
- Version 0.7.0, edition 2024, rust-version 1.95.
- The stack is **the same as ours**: egui/eframe **0.36** (we have 0.36.2), egui_kittest 0.36, kurbo **0.13** (we have
  0.13.1), `unsafe_code = "deny"`, a no-panic lint set, MCP/control channel, and an engine/tools/ui-egui layering identical
  to PhotoCraft.
- Differences: rendering is `vello_cpu` 0.2 (CPU only, multithreaded SIMD), text is skrifa 0.47 + harfrust 0.13, PDF
  is krilla 0.8 + hayro, SVG is usvg 0.48, and booleans are linesweeper 0.4.
- Self-assessment (README "Status"): about 69–75 % of Illustrator's features. Missing: 3D, the raster Effect Gallery,
  scripting and an interaction-fidelity pass.

**Crates and what we take** (size, licence MIT OR Apache-2.0 for all):

| Crate | What it is | Port? |
|---|---|---|
| `geom` (136 KB) | `PathData/SubPath/Anchor{p,in,out,kind}` (≈ our `Path/Subpath/Knot`), arcs, corners (live corner geometry), hit, projective, shape recognition, snap | **Yes**: into `pc-pathops` (adapted to our `Knot`) |
| `pathops` (212 KB) | curve-preserving booleans (linesweeper + refit), Pathfinder ops, Shape Builder/Live Paint regions, offset, outline stroke, simplify/smooth/join/average | **Yes**: the core of V1 |
| `doc` (752 KB) | Illustrator object tree (`Node/NodeKind`: Layer, Group(clip), Path(live), Compound, Text, Image, SymbolInstance, Blend, Envelope, Mesh, Repeat…), `Appearance{items: Fill/Stroke layers, effects}`, width profiles, blend/repeat/envelope specs | **The type designs only** (appearance, width profile, blend/repeat specs, symbols) re-expressed on our layers. Not the tree. |
| `brush` (88 KB) | calligraphic/scatter/art/pattern/bristle brush geometry | **Yes** (V9) |
| `trace` (236 KB) | Image Trace: quantise, contours, centreline, Bézier fit, logo mode | **Yes** (V9) |
| `effects` (352 KB) | live effects to geometry (warp, distort, roughen, zig-zag, offset, round corners) | Partly (V9: warp/envelope, round corners) |
| `svg` (596 KB) | usvg import (layers, clips, gradients, text, CSS) + hand-written export | **Yes** (V7), adapting the usvg 0.48 API to 0.45 |
| `tools` (1.1 MB) | tool state machines: pen, direct, shape, builder, corners, cut, draw2 (pencil/smooth/eraser), xform, symbolism, meshedit, text | **Logic yes**, rewritten into our tool/command structure tool by tool |
| `pdf` (920 KB) | krilla export + hayro import | **Algorithms only** (V10); the deps are not allowed (owner Q3) |
| `text` (532 KB) | its own shaping/layout incl. type on a path and threading | **Type-on-path layout only** (V6), on pc-text glyph outlines |
| `render` (624 KB) | vello_cpu renderer | No (rule 2) |
| `color` | swatches, gradients, harmony, recolor | Later, as reference |
| `affinity` (268 KB) | bounded `.af/.afdesign` reader (deps crc32fast, flate2, image, ruzstd: all in our lock) | Optional later (Q5) |
| `eps`, `cad`, `metafile`, `plugins`, `mcp`, `engine`, `ui-egui`, `format` | | No (reference for UI behaviour only) |

The ported subset is **about 13 k lines** of pathops/geom/brush/trace/svg plus the pen/direct/shape/builder/corner/cut/draw2 tool logic.

### 3.2 DesignCraft: https://github.com/storytold/designcraft
- Pin: `66c7ce7e96ec286a091878cc2541ee8369aa79d6`.
- It is an **InDesign** clone (spreads, parent pages, threaded stories, IDML, EPUB), **not** an Affinity Designer analogue.
  Licence MIT OR Apache-2.0, and the same stack (egui 0.36, kurbo 0.13, vello_cpu, krilla).
- Portable value: `crates/compose`, a Knuth–Plass paragraph composer with hyphenation, justification, optical margins and
  baseline grid. This is a later upgrade for **frame text** quality (V6c), and it is optional. Nothing else is in scope.

### 3.3 Other references
- **Graphite** (Apache-2.0, Rust; https://github.com/GraphiteEditor/Graphite at
  `377ba8f00c369a635d7ee2d1c637fa0799983436`) is a node-based design editor. Its vector tools are reference
  for the Pen/Path tool UX. Its node-graph architecture is not adopted.
- **kurbo 0.13** (already locked) is used for analytic bounds, nearest point, arc length and offset helpers, via `pc-pathops`.
- **lyon** (tessellation; not needed, because we rasterise on the CPU).
- **flo_curves** (Apache-2.0) and **i_overlay** (MIT/Apache, polygon-only) are the boolean alternatives considered. linesweeper is
  preferred because it is curve-preserving, robust, and the engine VectorCraft already wraps.
- **Inkscape** (GPL-2+, C++) is behaviour reference only: LPE, Offset/Inset, Shape Builder (1.3). No code is taken.
- **Affinity Designer 2** is the reference for the persona model (Designer / Pixel / Export) and the tools: Move, Node, Point Transform,
  Corner, Pen, Pencil, Vector Brush, Fill, Transparency, Contour, Knife, Shape Builder, Vector Crop, Artistic
  Text, Frame Text, Text on path, shape tools, Geometry ops (Add/Subtract/Intersect/Xor/Divide), Symbols,
  Constraints, Snapping, and the Appearance panel with multiple fills and strokes.
- **Illustrator essentials** not in Affinity: Width tool, Pathfinder (Trim/Merge/Crop/Outline/Minus Back), Offset
  Path, Image Trace, Blend, Repeat, Envelope.
- **Photopea and Krita** vector layers are a confirmation that "vector layers in a raster editor" is the expected model.
- **Designer vs photographer needs.**
  - Designers need everything above, plus precise snapping/units, artboards, symbols, SVG/PDF out and crisp zoom.
  - Photographers need logo/watermark import (SVG), shapes, art text and text on path, booleans, vector
    masks from pen paths, and social-graphic templates. That set becomes the "Photographer" tool set.

---

## 4. Recommendation

1. **The architecture is Affinity's: an object is a layer.** Extend `ShapeLayer` (appearance, live shapes, corners), `TextLayer`
   (art/frame/on-path), and `Group` (live blend/repeat specs). Add one new content kind, `Symbol`. Every vector feature then
   inherits our layer stack, masks, fx, blend modes, history, PSD path and GPU compositor.
2. **Port VectorCraft's geometry/tool logic, not its document, renderer or text stack.** This avoids two rasterisers, two
   font engines, and about 40 new crates (vello_cpu 0.2, fearless_simd, krilla, hayro, newer skrifa/harfrust/usvg).
3. **New crates in the workspace:**
   - `pc-pathops`: geom + pathops port, kurbo 0.13 + linesweeper.
   - `pc-svg`: SVG import/export, usvg 0.45 + a hand-written writer.
   - V9 adds `pc-vecfx`: brushes, blends, repeat, envelope and trace.
   - `pc-vector` stays the rasteriser and gains appearance rendering.
4. **Persona = Pixel | Vector inside Compositing.** The persona chooses the family of tool sets. Tool sets
   (All, Essentials, …, custom, Photographer) are named lists of tools tagged with a persona (§6).
5. **Resolution-independent view is a required phase (V5)**, not optional. Designers judge a vector app by crisp zoom.

---

## 5. Document-model changes (all additive; old documents load)

Model types live in `crates/pc-doc`. The persisted mirror lives in `crates/pc-format/src/manifest.rs` (`ContentM`, `DocM`), and the mapping is in
`convert.rs`.

```rust
// pc-doc/src/vector.rs
pub struct Knot { anchor, in_ctrl, out_ctrl,
    #[serde(default)] smooth: bool,
    /// V3 Corner tool: live corner on this anchor (None = sharp). VectorCraft geom/corners.rs.
    #[serde(default, skip_serializing_if = "Option::is_none")] corner: Option<Corner>,
}
pub struct Corner { radius: f64, kind: CornerKind /* Round | Inverted | Chamfer | Straight */ }

pub enum LiveShape { Rect{rect, radii,
        #[serde(default, skip_serializing_if = "all_round")] kinds: [CornerKind;4]},  // V3
    Ellipse{rect}, Polygon{rect, sides, star_ratio,
        #[serde(default)] curved: f64, #[serde(default)] corner: f64}, Line{..},
    // V3 additions (variants are additive; each carries `rect` plus a rotation `#[serde(default)] angle`):
    Star{rect, points, inner, #[serde(default)] corner: f64}, DoubleStar{..}, SquareStar{..},
    Triangle{rect, top: f64}, Trapezoid{rect, top_left, top_right}, Diamond{rect, top},
    Donut{rect, hole}, Pie{rect, start, end}, Segment{rect, start, end, inner},
    Arrow{from, to, head_w, head_l, shaft, ..}, Callout{rect, kind, tail: [f64;2], radius},
    Cog{rect, teeth, inner, hole, curve}, Heart{rect, ..}, Tear{rect, ..}, Crescent{rect, ..},
    Cloud{rect, bubbles}, Spiral{center, radius, decay, turns}, Arc{rect, start, end},
    RectGrid{rect, rows, cols}, PolarGrid{rect, rings, spokes},
}

pub struct ShapeLayer { path, fill, stroke, live, cache, psd_raw,
    /// V4: Appearance stack. None = legacy single fill/stroke (fields above). When Some, it is
    /// authoritative and `fill`/`stroke` mirror its topmost fill/stroke (for PSD + old readers).
    appearance: Option<Appearance>,
}
/// Paint order bottom→top, like VectorCraft doc/appearance.rs.
pub struct Appearance { items: Vec<AppearanceItem> }
pub enum AppearanceItem { Fill(FillItem), Stroke(StrokeItem) }
pub struct FillItem { paint: Fill, opacity: f32, blend: BlendMode, visible: bool }
pub struct StrokeItem { base: ShapeStroke /* existing */, blend: BlendMode, visible: bool,
    profile: Option<WidthProfile>,        // V5 Width tool / Affinity pressure
    brush: Option<BrushRef>,              // V9 vector brushes
    start_arrow: Option<Arrowhead>, end_arrow: Option<Arrowhead>, arrow_scale: (f64,f64),
    dash_align: bool, scale_with_object: bool, gradient_mode: StrokeGradientMode }
pub struct WidthProfile { points: Vec<(f64 /*t 0..1*/, f64 /*left*/, f64 /*right*/)> }

// pc-doc/src/text.rs
pub enum TextShape { Point /* = Artistic Text */, Box{..} /* = Frame Text */,
    /// V6: type on a path (path in text space), start/end offsets 0..1, side flip, baseline align.
    OnPath{ path: Path, start: f64, end: f64, flip: bool, align: PathTextAlign } }
// pc-doc/src/lib.rs
pub struct Group { …, #[serde(default)] live: Option<LiveGroup> }   // V9
pub enum LiveGroup { Blend(BlendSpec), Repeat(RepeatSpec), Envelope(EnvelopeSpec) }
pub enum LayerContent { …, Symbol(SymbolInstance) }                  // V8
pub struct SymbolInstance { symbol: SymbolId, transform: Affine, cache }
pub struct Document { …, #[serde(default)] symbols: Vec<SymbolDef> } // V8: SymbolDef = Vec<Layer> + name
pub struct Layer { …, #[serde(default)] constraints: Option<Constraints> } // V8 (Affinity)
```

Rules:
- `ContentM::Shape` gets `#[serde(default, skip_serializing_if="Option::is_none")] appearance`.
- `ContentM::Text` gets the `OnPath` shape. `TextShape` is already serialised by serde, and the new variant is additive.
- `ContentM::Symbol` is added together with the `FORMAT_VERSION` → 2 bump (rule 3). The fixture test `old_v1_documents_load`
  is in `crates/pc-format/tests/`.
- Any edit that a live shape cannot express converts it to a plain path (`live = None`). This is VectorCraft's rule, and pc-doc already
  follows it.
- **Booleans produce plain `Path`s** (subpaths with `op = Combine` + fill rule), so PSD export stays native.
  Non-destructive "compound" mode (Affinity Alt+boolean) = keep the subpath `PathOp`s as today. The coverage rasteriser already
  renders them, and **Layer › Geometry › Finish Compound** bakes them with `pc-pathops`.

---

## 6. Persona and tool-set UX

- **Persona switcher.**
  - Two segmented icons, **Pixel | Vector**, at the far left of the Compositing toolbar header (Affinity
    puts personas top-left). Tooltips are "Pixel persona" and "Vector persona".
  - Shortcut ⌘⇧1 / ⌘⇧2. The implementer checks
    `menu_catalog.rs` for conflicts and records the final keys in the menu catalog.
  - There is a View › Persona menu.
  - `ui.persona: Persona` (`#[serde(default)]`, persisted in UI prefs, global, not per document).
- **What switches:**
  - the toolbar (the active tool set of that persona)
  - the context bar
  - the last-used tool, remembered per persona
  - optionally the workspace (panel layout): Vector applies "Graphic and Web" only if the user has not customised the
    current one (Q2)
- **What stays the same:** the document, the selection, Layers, History, Navigator and the menus. Menus show both
  families. Layer › Geometry is enabled in both personas.
- **Cross-persona rules** (Affinity behaviour):
  - A vector tool used on a pixel layer creates a new shape layer above it.
  - A pixel tool on a shape/type/symbol layer shows the existing "Rasterize?" prompt (`shape.rasterize`).
  - Shared tools appear in both families: Move, Hand, Zoom, Eyedropper, Artboard, Crop and the Type tools.
- **Tool sets.** No code exists yet; the "tool sets" work in progress elsewhere must adopt this model.
  - `ToolSet { name, persona: Persona, tools: Vec<ToolSlot> }`, where a `ToolSlot` is a flyout group.
  - The toolbar shows the sets whose `persona == ui.persona`, with a set picker in the toolbar header.
  - Built-in sets:
    - Pixel: All Tools, Essentials, Retouching, AI.
    - Vector: All Tools, Essentials (Move, Node, Pen, Pencil, shapes, Art/Frame text, Fill, Transparency, Corner,
      Shape Builder, Eyedropper, Hand, Zoom), Illustration (adds Vector Brush, Width, Knife, Blend, Contour),
      Layout & Type.
    - **Photographer**, a cross-persona set: Pixel side = Essentials + Retouching subset; Vector side = Move, Node, Pen,
      Rectangle/Ellipse/Rounded/Star/Arrow/Callout, Art Text, Text on Path, Corner, Geometry ops in the context bar,
      SVG Place.
  - Custom sets: duplicate/edit/rename/delete, stored in UI prefs.
  - A single `Tool` registry (`state.rs` `Tool::ALL`) tags every tool with `personas: &[Persona]`. All Tools is
    generated from it, so a new tool can never be missing.

---

## 7. Vector tool list and behaviour (all phases)

V = phase. ⌥ = Alt/Option, ⇧ = Shift. The upstream column names the VectorCraft source to port from.

| Tool / feature | Behaviour | Upstream | V |
|---|---|---|---|
| **Move** (shared) | select/transform layers; bbox handles; ⇧ constrain; ⌥ duplicate; rotate near corners; transform origin | tools/select.rs, xform | V2 (exists, extended) |
| **Node** (Direct Selection) | edit nodes of all selected curves; click segment = select, drag = bend; marquee; ⇧ add; dbl-click segment = add node; Del = delete node (keep shape); context bar: Sharp/Smooth/Smart, Break, Join, Close, Reverse, Align nodes, Smart-convert; transform-selected-nodes bbox; live shapes ask "Convert to curves" | tools/direct.rs, pathops/edit.rs | V3 (extends `direct_select.rs`) |
| **Point Transform** | snap-to-node transforms (move/rotate/scale about a node) | Affinity behaviour | V8 |
| **Pen** | modes Pen / Smart / Polygon / Line; click = corner, drag = smooth; ⌥ break handle; rubber band; click start = close; continue an open path from its end; ⇧ 45°; Enter/Esc finish; Add-to-selected-curve vs new layer; fill/stroke from context bar | tools/pen.rs | V3 (extends `vector_ui.rs` pen) |
| **Pencil** | freehand → fitted Béziers (smoothness slider, `pathops::fit`); stabiliser (rope/window); ⌥ continue/close; Sculpt mode (redraw a stretch of a selected curve) | tools/draw2.rs | V3 |
| **Smooth / Path Eraser / Join** | Illustrator helpers on selected curves | draw2.rs, extra.rs | V5 |
| **Corner** | drag on nodes to set live corner radius (`Knot.corner`); kinds Round/Inverted/Chamfer/Straight; per-node or all | geom/corners.rs, tools/corners.rs | V3 |
| **Shape tools** | Rect, Rounded Rect, Ellipse, Triangle, Polygon, Star, Double Star, Square Star, Trapezoid, Diamond, Donut, Pie, Segment, Arrow, Callout (rect/ellipse), Cog, Heart, Tear, Crescent, Cloud, Spiral, Arc, Line, Rect/Polar Grid; drag from corner, ⌥ from centre, ⇧ square; on-canvas red live handles (Affinity) + context-bar params; "Convert to curves" | tools/shape.rs, geom/shapes.rs | V3 (existing 6 kept) |
| **Geometry ops** | Add, Subtract, Intersect, Xor, Divide (Affinity); Pathfinder Trim, Merge, Crop, Outline, Minus Back (Illustrator); ⌥ = non-destructive compound; result keeps the bottom object's appearance; multi-layer input, one undo step | pathops/boolean.rs, pathfinder.rs | V1 engine, V2 UI |
| **Expand Stroke / Outline Stroke** | stroke → filled path (joins, caps, dashes, align) | pathops/offset.rs | V1 |
| **Contour** (Offset) | drag on a curve = interactive offset (± distance, join type, miter); also Layer › Geometry › Offset Path dialog | pathops/offset.rs | V5 |
| **Knife / Scissors / Line Cut** | drag across curves to split into separate layers; click a segment to cut | tools/cut.rs, pathops/planar.rs `cut_out` | V5 |
| **Shape Builder** | hover highlights regions of overlapping selected shapes; drag = merge, ⌥ = remove; region gets the clicked object's appearance | tools/builder.rs, pathops/planar.rs | V5 |
| **Width** | drag on a stroke to add/move width points (`WidthProfile`); profile presets; Affinity "pressure" graph in Stroke panel | engine tests_widthtool.rs, doc/appearance.rs | V5 |
| **Fill tool** (Gradient for vectors) | on-canvas gradient handles on the selected layer's fill or stroke (linear/radial/elliptical/conical); stops, midpoints | ours: Gradient tool + upstream #1321 | V4 |
| **Transparency tool** | gradient of opacity applied as a vector-layer mask gradient | Affinity | V4 |
| **Appearance panel** | multiple fills/strokes, reorder, per-item opacity/blend/visibility, duplicate; Stroke panel: align, caps/joins, dashes + dash-align, arrowheads, scale-with-object, width profile, gradient-along/across stroke | doc/appearance.rs | V4 |
| **Snapping** | snap to nodes, curve, bbox edges/centres, guides, grid, pixel grid, artboard, other objects' geometry; Smart Guides (alignment, spacing, angle) with on-canvas magenta guides; snapping manager popover | geom/snap.rs, engine tests_smartguides.rs | V3 core, V8 smart guides |
| **Art Text** | point text scaling with the box (exists as TextShape::Point) | ours | V6 (polish) |
| **Frame Text** | text in a rectangle; later any shape as frame | ours (`TextShape::Box`) | V6 |
| **Text on Path** | click a curve with the Type tool → text flows along it; drag start/end markers; flip side; baseline align; convert to curves | text/layout.rs on-path part | V6 |
| **Text wrap, Knuth–Plass** | optional: DesignCraft compose | designcraft crates/compose | V6c (later) |
| **Align / Distribute / Transform panel** | exists (`align_cmds.rs`); add key object, spacing value, distribute spacing, Transform panel X/Y/W/H/R/S with anchor | ours | V8 |
| **Artboards** | exists (`artboard_cmds.rs`); add Artboard tool in Vector persona, export per artboard | ours | V8 |
| **Symbols** | Symbols panel; create from selection; instances update when the master changes; detach | doc SymbolInstance | V8 |
| **Constraints** | pin a layer's edges/size to its parent artboard/group for resizing (Affinity) | Affinity | V8 |
| **Blend** | live blend between two or more curves (steps/distance, along spine); expand | doc/blend.rs, live.rs | V9 |
| **Vector Brush** | stroke with brush: calligraphic, art (texture-stretch), scatter, pattern; pressure → width profile | brush crate | V9 |
| **Repeat / Envelope** | radial/grid/mirror repeat groups; envelope warp of a group | doc/live.rs, effects/warp.rs | V9 |
| **Image Trace** | raster layer → vector layers (modes Logo/B&W/Colour N, threshold, paths, corners, noise; centreline) with preview | trace crate | V9 |
| **Vector Crop** | crop a vector layer by a rectangle non-destructively (clip) | Affinity | V8 |
| **SVG import/export** | V7 (§10) | svg crate | V7 |
| **PDF export/import** | V10 (§10) | pdf crate (algorithms) | V10 |

---

## 8. Porting plan, pins and attribution

Upstream: `https://github.com/storytold/vectorcraft` at **`d522c1d7be4035bd4f4a84cd6ebfca44f5155092`**, licence
`MIT OR Apache-2.0`. The coordinator clones it outside the sandbox, or copies it into the job worktree under
`vendor-src/` (never committed), so Codex can read it offline. The shallow clone is at
`/tmp/claude-1000/.../scratchpad/upstream/vectorcraft`; re-clone it if that path is gone.

**Notices.**
- Add `licenses/vectorcraft-LICENSE-MIT`, `licenses/vectorcraft-LICENSE-APACHE` and `licenses/vectorcraft-NOTICE`,
  copied from the upstream root.
- Add `assets/ATTRIBUTION-vectorcraft.md`, modelled on `ATTRIBUTION-lightcraft.md`.
- Add an attributions JSON entry as for PhotoCraft.

**`docs/PORTS.md` rows.** Add one per upstream file, in the existing format. Rows to add, with the date being the date of the port:

| Our file | Upstream path | V |
|---|---|---|
| `crates/pc-pathops/src/geom/{path,bez,arc,corners,hit,snap,shapes,recognize}.rs` | `crates/geom/src/<same>.rs` | V1/V3 |
| `crates/pc-pathops/src/{boolean,pathfinder,planar,offset,edit,fit}.rs` | `crates/pathops/src/<same>.rs` | V1/V5 |
| `crates/pc-ui-egui/src/vector/{pen,node,shape,corner,pencil,cut,builder,width}.rs` + `crates/pc-engine/src/vector_cmds/*` | `crates/tools/src/{pen,direct,shape,corners,draw2,cut,builder}.rs` | V3/V5 |
| `crates/pc-doc/src/appearance.rs` | `crates/doc/src/appearance.rs` | V4 |
| `crates/pc-text/src/on_path.rs` | `crates/text/src/layout.rs` (on-path part) | V6 |
| `crates/pc-svg/src/{import,export,css}.rs` | `crates/svg/src/{import.rs,import/*,export.rs,css.rs}` | V7 |
| `crates/pc-vecfx/src/{blend,repeat,envelope}.rs` | `crates/doc/src/{blend,live}.rs`, `crates/effects/src/warp.rs` | V9 |
| `crates/pc-vecfx/src/brush/*` | `crates/brush/src/*` | V9 |
| `crates/pc-vecfx/src/trace/*` | `crates/trace/src/*` | V9 |
| `crates/pc-pdf/src/*` | `crates/pdf/src/{export,import*}.rs` (algorithms only) | V10 |

Every row has upstream project `[VectorCraft](https://github.com/storytold/vectorcraft)`, commit `d522c1d7be4035bd4f4a84cd6ebfca44f5155092` and licence `MIT OR Apache-2.0`.

**Adapting the types.** The type adapter is VectorCraft `Anchor{p, h_in, h_out, kind}` ↔ our `Knot{anchor, in_ctrl, out_ctrl, smooth}`.
The mapping is 1:1, with `kind: Smooth` ⇔ `smooth: true`. `PathData{subpaths}` ↔ our `Path` with every `op = Combine`, and
`FillRule` matches. Keep upstream tests: port each ported module's unit tests and the relevant `engine/tests_*.rs`
cases (`tests_pathops`, `tests_outlinestroke`, `tests_widthtool`, `tests_livecorners`, `tests_smartguides`,
`tests_draw2`, `tests_cut`, `tests_build`) as engine tests against our commands.

**Phase V0 (PhotoCraft sync).** Port the three upstream PhotoCraft vector commits in §2.6 if our fork predates them. Add
a PORTS row for each (licence `MIT OR Apache-2.0`).

---

## 9. Rendering requirements

1. **The CPU reference is `pc-vector`.**
   - V4 extends `CompiledShape` to render an `Appearance` stack bottom→top. Each item is rasterised with
     its coverage, paint, opacity and blend, using the same blend math as `pc-compose`'s blend-mode code. Reuse that code; do not copy it.
   - Strokes with a `WidthProfile`/brush/arrowheads are converted to fill geometry by `pc-pathops`
     (`outline_stroke` / variable-width outline) before rasterising, so stroke and fill share one rasteriser.
   - Pattern fills: implement the rendering here (it is currently `Paint::None`).
2. **The GPU path is unchanged.** It composites `ShapeLayer.cache` / symbol caches.
   - `shape_split` keeps working for single fill/stroke.
   - With an appearance stack and clipped layers, use the stack's merged cache and treat the shape as "fill only"
     for clipping. Document the decision in `shape_split.rs`.
   - Add parity tests to `crates/pc-gpu/tests/parity.rs`:
     - `appearance_stacks_on_shapes`: 2 fills + 2 strokes, blend modes, gradient stroke
     - `width_profile_strokes`
     - `symbol_instances`
     - `text_on_path_layers`
     - `live_blend_groups`
     - `layer_effects_on_appearance_shapes`
3. **Phase V5 is the resolution-independent view.** Today caches are in document pixels.
   - Add a *display render scale*: when the canvas zoom is > 100 % and the viewport shows vector/type/symbol layers, the canvas asks
     pc-compose / pc-gpu for the visible rect at scale `s = zoom`. Vector layers are re-rasterised at `s` into a
     per-view tile cache (`pc-vector` takes an `Affine`). Raster layers are sampled as today.
   - The CPU reference
     (`flatten_scaled(doc, rect, s)`) and the GPU path must match within 1/255. Add a parity test `zoomed_vectors_match_cpu`
     at s = 2, 4 and 8.
   - Export always uses document resolution unless the user exports at @2x/@3x, and then re-rasterises at that
     scale (same function).
4. **Performance budget**, measured in release builds on the coordinator machine:
   - Editing one node on a 6000×4000 document with 500 shape layers re-rasterises only that layer, at < 16 ms/frame.
   - A boolean on two 1000-node paths takes < 50 ms.
   - SVG import of a 2 MB file takes < 1 s.
   - Add `#[ignore]` perf tests like `pc-vector`'s `perf_6016`.
   - Shape caches must be bounds-limited (shape ∩ document), never full-document surfaces. Verify this with the
     current `render_shape` and fix it if needed.

---

## 10. File formats

- **SVG import** (V7): `pc-svg::import(bytes) -> Vec<Layer>` via `usvg` 0.45.1 (already locked).
  - Groups become groups, and paths become shape layers (fill and stroke, gradients, opacity, fill rule, dashes).
  - Clip paths become clipping groups, and masks become vector masks or a rasterised mask.
  - Text becomes type layers when the font resolves, and curves otherwise.
  - Images become raster layers.
  - Usage: File › Open (a new document sized to the viewBox, at a DPI chosen in a dialog), File › Place
    (as a group, or as an embedded smart object if the user ticks "Place as Smart Object"), and paste of SVG text.
  - Port the import mapping from VectorCraft `crates/svg/src/import*.rs`. API differences 0.48→0.45 are the
    implementer's job; keep a table of them in the module docs.
- **SVG export** (V7): a hand-written writer, ported from `crates/svg/src/export.rs`.
  - It exports the document, the selection or an artboard.
  - Shape layers become `<path>` with fill/stroke (multiple appearance items become stacked `<path>`s).
  - Gradients become `<linearGradient>/<radialGradient>`. Diamond/angle gradients and blend modes beyond CSS are rasterised.
  - Groups become `<g>`, with opacity, `mix-blend-mode` and a vector mask as `<clipPath>`.
  - Type becomes `<text>`, or outlines if the user chooses "Text as curves".
  - Pixel layers, adjustments and effects are emitted as embedded PNG `<image>`, with a warning.
  - Round-trip tests: export → import → render, IoU > 0.99 against the original render.
- **PSD** is unchanged (rule 4).
- **PDF** (V10):
  - Vector PDF export extends the existing hand-written writer (`print_cmds.rs`) with path operators
    (`m l c h re f f* S B W n`), gradients as Type 2/3 shadings, opacity/blend via ExtGState, and type as outlines
    (v1).
  - Embedded fonts and PDF import need new crates (krilla / hayro) and wait on Q3.
- **Affinity `.afdesign` import** is optional (Q5). `vectorcraft-affinity`'s dependencies are all already locked.

---

## 11. Phases (each sized as one Codex job; each ends green)

Every phase must leave all of the following green:
- `cargo +1.98.1 --offline test --workspace`
- `cargo +1.98.1 fmt --all --check`
- `cargo +1.98.1 clippy --workspace -- -D warnings`
- GPU parity tests on the coordinator's RTX 5090 (the coordinator runs them; they skip in Codex)
- `git diff Cargo.lock` shows no unapproved packages

UI tests use egui_kittest in the pattern of `crates/pc-ui-egui/src/direct_select_tests.rs`.

### V0: PhotoCraft vector sync (small)
- **Scope:** port upstream PhotoCraft `#865`, `#1321` and `#747` (see §2.6) if they are missing; add PORTS rows.
- **Acceptance:**
  - The upstream tests for those commits pass here.
  - The corpus vector tests stay green.

### V1: `pc-pathops` geometry engine (no UI)
- **Scope:**
  - Port `geom` (path, bez, arc, hit, shapes) and `pathops` (boolean, pathfinder, offset, edit, fit), with the adapter.
  - Add linesweeper 0.4.0 and polycool 0.4.0. The coordinator pre-fetches them.
  - Add engine commands, journaled and undoable, each taking `layers: [ids]` with options:
    - `path.boolean {op: add|subtract|intersect|xor|divide, keep_compound}`
    - `path.pathfinder {op: trim|merge|crop|outline|minusBack}`
    - `path.outlineStroke`, `path.offset {delta, join, miter}`
    - `path.simplify {tolerance}`, `path.smooth`, `path.reverse`, `path.join`, `path.splitAt`
    - `path.finishCompound`
  - Add analytic bounds (`Path::bounds()` via kurbo).
- **Acceptance:**
  - The upstream pathops unit tests pass when ported.
  - Area checks, compared with the coverage rasteriser on 20 random shape pairs (within 0.5 %):
    - two overlapping 100×100 squares: union area = 17 500 ± 0.1, intersect = 2 500
    - circle − circle
  - Divide yields N layers.
  - The output of `outline_stroke` rasterises to the same coverage as `stroke_polygons`
    (max diff ≤ 1/255 at 4× AA).
  - The fuzz test (proptest, already in upstream style) never panics on degenerate input.
  - The PSD round trip of a boolean result passes.

### V2: Persona + tool sets + vector toolbar + Geometry UI
- **Scope:**
  - `Persona`, the switcher, the per-persona last tool, the `ToolSet` model with built-ins and custom sets (§6), and `Tool` persona tags.
  - The Vector toolbar sections (existing vector tools for now).
  - The context-bar Geometry buttons, and the Layer › Geometry menu.
  - Cross-persona rules: a vector tool on a pixel layer creates a new layer; a pixel tool on a vector layer asks to rasterize.
- **Kittest acceptance:**
  - Clicking Vector shows only vector-family tools; the Pen is visible and the Brush is hidden.
  - ⌘⇧1/2 toggles the persona.
  - The persona and the active set survive restart (prefs round trip).
  - Two shape layers selected plus the Add button gives one layer with the union path, and one undo restores both.
  - Choosing the Photographer set shows exactly its list.
  - A custom set's create/rename/delete persists.
  - The Layers panel is unchanged across the switch (same row count and selection).

### V3: Core drawing and node editing
- **Scope:**
  - Node tool upgrades, Pen modes, Pencil (+ Sculpt), the Corner tool (`Knot.corner`), and the full shape list (new
    `LiveShape` variants with on-canvas live handles).
  - Core snapping: nodes, bbox, guides, grid and pixel grid.
- **Acceptance:**
  - Engine tests: each `LiveShape` → `live_path` area vs analytic (star, donut, pie, arrow, …).
  - Corner radius 10 on a square reduces its area by exactly (4 − π)·100.
  - Pencil on a sampled circle fits ≤ 8 nodes with max deviation ≤ tolerance.
- **Kittest acceptance:**
  - Pen click-click-drag-close produces a closed 3-node path with one smooth node.
  - Node tool marquee selects 2 of 4 nodes, and Del leaves 2.
  - Dragging a star's inner-radius handle changes `inner`.
  - Snap places a rectangle corner exactly on a guide.
- **Serde:** a v1 document without the new fields loads, and a v3-era document round-trips.

### V4: Appearance, paint and stroke
- **Scope:**
  - `Appearance` with multiple fills and strokes, and the Stroke panel completed (arrowheads, dash align, scale with object,
    gradient along/across stroke).
  - The Fill tool on vector layers, the Transparency tool, and pattern fill rendering.
  - PSD export rules (rule 4).
- **Acceptance:**
  - The parity tests listed in §9.2 (appearance, layer effects on appearance shapes).
  - CPU golden tests: 2 fills + 2 strokes give expected pixels at 6 sample points.
  - Arrowhead geometry tests.
  - PSD export of a multi-stroke shape reopens with identical pixels and shows the editability warning.
  - Kittest: Appearance panel add stroke / reorder / hide changes the canvas.

### V5: Resolution-independent view + constructive tools
- **Scope:**
  - The display render scale (§9.3).
  - Contour, Knife/Scissors/Line Cut, Shape Builder, Width tool + `WidthProfile`, Smooth/Path Eraser/Join.
- **Acceptance:**
  - `zoomed_vectors_match_cpu` (parity at s = 2, 4, 8).
  - A 1 px hairline at 800 % zoom renders as a sharp 8 px line (column-profile test).
  - Shape Builder on 2 overlapping circles has 3 regions; merging 2 of them gives a specific area.
  - The Knife across a rectangle gives 2 layers whose areas sum to the original.
  - A width profile of 0→10→0 gives the analytic lens area ± 1 %.

### V6: Type (Art, Frame, On Path)
- **Scope:**
  - `TextShape::OnPath` layout on pc-text glyph runs (port the on-path layout).
  - The Type tool click-on-curve creates on-path text; on-canvas start/end markers; flip.
  - Polish of Art/Frame text in the Vector persona.
  - Convert to curves for on-path text.
  - PSD export of on-path text as pixels + warning. V6b, optional, is a real Photoshop type-on-path `TySh`.
- **Acceptance:**
  - Glyph origins lie on the path within 0.5 px, and rotations equal the tangent angle ± 0.5°.
  - The parity test `text_on_path_layers`.
  - Kittest: clicking the Type tool on an ellipse and typing "ABC" creates an on-path layer.
- **Optional V6c:** DesignCraft Knuth–Plass composer for frame text.

### V7: SVG import/export
- **Scope:** §10 SVG, plus Place, Paste, Export dialog options (selection/artboard, text as curves, decimals, minify).
- **Acceptance:**
  - 15 checked-in SVG fixtures (shapes, transforms, gradients, clip, mask, text, `use`, CSS `<style>`, viewBox units)
    import with expected layer trees.
  - Export → import → render IoU > 0.99.
  - A malformed SVG returns an error and never panics.
  - Kittest: File › Place SVG adds a group.

### V8: Structure: Symbols, Constraints, Artboards, Align/Transform, Smart Guides, Point Transform, Vector Crop
- **Scope:**
  - `LayerContent::Symbol` + `Document.symbols` and the **FORMAT_VERSION 2** bump.
  - The Symbols panel.
  - Constraints applied on artboard/group resize.
  - The Artboard tool in the Vector persona.
  - Transform panel, Smart Guides, Point Transform and Vector Crop.
- **Acceptance:**
  - Editing a symbol master updates 3 instances (pixels).
  - Detach makes the instance independent.
  - A constraint "pin right, fixed width" keeps the right margin when the artboard grows.
  - v1 files load, and v2 files written here are refused by a simulated v1 reader with `TooNew`.
  - The parity test `symbol_instances`.

### V9: Live and advanced: Blend, Vector Brush, Repeat, Envelope, Image Trace (`pc-vecfx`)
- **Acceptance:**
  - A blend of 2 circles with 5 steps interpolates radius and colour linearly.
  - A calligraphic brush gives a width that varies with angle (analytic).
  - Image Trace (logo mode) of a synthetic 2-colour PNG gives ≤ N paths with IoU > 0.98.
  - The parity test `live_blend_groups`.

### V10: PDF
- **Scope:** vector PDF export (own writer, outlines for type). Import and embedded fonts depend on Q3.
- **Acceptance:**
  - Structural tests: path operators are present, and there is no raster for pure-vector documents.
  - The output re-renders via existing PDF tooling where available. Otherwise, structural assertions only.

Rough size: V1, V3, V4, V5 and V9 are large jobs (one Codex xhigh job each, possibly split into a/b). V0, V2, V6, V7, V8
and V10 are medium.

---

## 12. Risks

1. **Crisp zoom (V5) touches the compositor core.** Mitigation: a separate phase, parity tests at several scales,
   and vector layers only (raster unchanged).
2. **Many small shape layers.** A designer document can have thousands of objects. Per-layer caches must be
   bounds-limited and lazily (re)built; groups need merged caches. Budget tests are in §9.4.
3. **PSD fidelity vs new features.** Mitigation: rule 4 (pixels + first fill/stroke + warning), and the corpus tests
   stay green.
4. **Upstream drift.** VectorCraft moves daily (800 PRs in 10 days). Pin the commit, port by file, and update rows
   deliberately.
5. **usvg 0.45 vs 0.48 API gaps** in the import port. The fallback is direct `roxmltree` for missing pieces.
6. **linesweeper is young** (0.4/0.5). VectorCraft relies on it with exact-boolean claims. The fuzz tests in V1 are the guard,
   and the coverage rasteriser stays as the fallback for non-destructive compounds.
7. **UI surface area.** About 40 tools plus panels. The tool-set model keeps the Pixel persona unchanged for existing
   users, and every new tool lands behind the Vector persona.

---

## 13. Coordinator steps (need network or the real machine; not for Codex)

1. Find the PhotoCraft commit our pc-* crates were forked from (git history of the first pc-* import, or
   `licenses/photocraft-NOTICE` / `docs/upstream/photocraft`). Decide whether V0 is needed.
2. `cargo fetch` after adding `linesweeper = "0.4.0"` to the workspace, so the Codex sandbox has the
   crates offline. Then verify `git diff Cargo.lock` adds only `linesweeper` and `polycool`.
3. Provide the VectorCraft source at the pin inside the job worktree (`vendor-src/vectorcraft`, gitignored).
4. Run the GPU parity tests on the RTX 5090 after each phase.

---

## 14. Open questions for the owner (decisive)

1. **New crates:** may we add `linesweeper` + `polycool` (pure Rust, MIT/Apache, 2 packages) for exact curve
   booleans? The alternative is porting the sweep-line ourselves, which is weeks of work and riskier.
2. **Persona and workspace:** should switching to Vector also switch the panel workspace to "Graphic and Web" (Affinity does
   switch panels per persona)? Or should it only switch tools and the context bar?
3. **PDF:** accept krilla + hayro (about 30 extra pure-Rust crates) for full PDF export with embedded fonts and PDF import? Or
   keep our own writer, with type as outlines and no import, for now?
4. **Units:** designers expect pt/mm/in with a document DPI. Should the Vector persona default the rulers and context bar to
   points for new documents created from a design preset, and leave pixels for photos?
5. **Affinity file import** (`.afdesign/.af` via VectorCraft's reader, with no new crates): include it as a later phase, or skip it?

## Owner decisions (2026-10-09)
1. Booleans: **add `linesweeper` and `polycool`** (pure Rust, MIT/Apache), pinned to the versions VectorCraft uses.
2. Persona switch changes **tools + context bar only** (same panels/layers/history), Affinity-style.
3. PDF: **full — krilla + hayro** (embedded fonts, vector export and PDF import), pure Rust only.
4. New design/vector documents **default to pt/mm** (photo documents stay in px).
5. **Affinity `.afdesign`/`.afphoto` import** added as a later phase (no new crates).
6. Scope: the full designer-class vector tool set (owner: the app is for designers/compositors too); photographers get
   the curated "Photographer" tool set.
