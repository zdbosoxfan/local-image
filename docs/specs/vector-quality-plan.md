# Vector Quality Plan + Vectorizer: addendum to `vector-persona-spec.md`

Status: 2026-10-09. The coordinator merges this into the main spec. Phase V1 (geometry engine) is already
running from the main spec. Everything in Part A that touches V1 is written so it can be applied as a **follow-up
job "V1-Q"** without redoing V1. Part B (the vectorizer) can start **now**, in parallel with V1.

Method: for every operation we choose the best available implementation, or a combination, with a
reason and the known failure modes. We then prove correctness three ways:

1. **Independent oracles** inside our own code. Example: our coverage rasteriser already computes booleans per pixel, so it
   can check the geometric booleans.
2. **Golden outputs from upstream tools.** The coordinator generates these once, outside the sandbox, and commits them.
3. **Property tests** (`proptest` 1.11.0 is already in `Cargo.lock`), plus a ratcheted real-world corpus.

---

## Part A: Quality plan

### A.1 Shared test infrastructure (new; build it in V1-Q and reuse it in every phase)

**`crates/pc-testkit/src/vector/`.** pc-testkit already exists; keep it a dev-only crate. It holds:

**Geometry metrics.**
- `area(path, rule)`: exact, via Green's theorem on cubics (kurbo `ParamCurveArea`).
- `hausdorff(a, b, samples)`: symmetric, uses kurbo `nearest`.
- `self_intersections(path) -> Vec<(seg, seg, t, u)>`: a brute-force O(n²) cubic–cubic intersection on
  flattened segments. It is deliberately simple, so it does not share code with linesweeper.
- `is_finite(path)` and `node_count(path)`.

**Raster metrics.**
- `coverage(path, size)`: via `pc-vector`.
- `max_abs_diff` and `iou(mask_a, mask_b, threshold)`.
- `ssim_ms(a, b)`: an **own** multiscale SSIM, written from Wang et al. 2003. **Do not** use `dssim-core`, which is AGPL-3.0.
- `delta_e_ok(a, b)`: Oklab ΔE. Reuse the existing colour maths in `crates/pc-cms/src/math.rs`, or the Oklab code if
  present.
- `fidelity(a, b)`: a port of **vtracer-bench**'s blind composite (MIT OR Apache-2.0,
  `visioncortex/vtracer` `crates/vtracer-bench/src/lib.rs` @ `928ed0a6f654408e28fb741b6133d4c456bd0160`).
  - It is `(psnr¹·ssim²·patch¹)^(1/4)`.
  - The "missing patch" detector clusters large coherent errors that SSIM averages away.
  - Use our SSIM in place of dssim.

**Distance-transform oracle.** An exact Euclidean distance transform (Felzenszwalb–Huttenlocher, written from the
paper) of a rasterised shape at 8× supersampling. Its threshold at `d` is the ground truth for a round-join
offset or a round-cap stroke.

**SVG reference renderer for tests: `resvg` 0.45.1.**
- It is already in `Cargo.lock` (through egui_extras). Adding it as a **dev-dependency** adds no packages; verify
  with `git diff Cargo.lock`.
- It renders SVG fixtures and committed upstream golden SVGs to pixels, independently of our renderer.

**Golden files** go under `crates/<crate>/tests/golden/<case>.{svg,json}`, each with a `SOURCES.md` stating the tool,
version and command that produced it, plus the licence of the input. Goldens are compared by **geometry metrics, not by
node equality**:
- area difference
- Hausdorff distance
- IoU of 8× supersampled coverage

**The ratchet file.** `tests/ratchet/<suite>.json` lists the corpus cases that currently fail, with the reason. CI fails if a
case **not** listed fails, and also if a listed case now passes and is still listed. The list only ever shrinks.

**Property-test budgets.**
- 256 cases per property in normal runs.
- The coordinator's nightly runs `PROPTEST_CASES=10000`.
- Failures are persisted to `proptest-regressions/` and committed.

**No fuzzing crates.** cargo-fuzz needs libFuzzer, which is C. Use proptest with targeted generators:
- random cubic paths, including degenerate ones (zero-length segments, collinear handles, cusps)
- coincident and tangent shapes
- huge (1e6) and tiny (1e-4) coordinates
- NaN and inf injected at the API boundary, where it must return `Err` and never panic

**Coordinator golden generation**, outside the sandbox, run once per upstream pin:
- Inkscape 1.4.4: `dnf install inkscape`, which needs the owner's OK.
  - Used for booleans, offset (Path › Offset), outline stroke (Stroke to Path), simplify and text-on-path:
    `inkscape --actions="select-all;path-union;export-plain-svg;export-do" in.svg`, and so on.
- potrace 1.16 (`dnf install potrace`) and vtracer 0.6.5 + 1.0.0-alpha.4, built from source (`cargo install`
  into the scratchpad) for the vectorizer goldens.
- The resulting SVGs are committed. Codex never runs these tools.

### A.2 Implementation choice per operation

Licences: VectorCraft (VC) is MIT OR Apache-2.0; Graphite (GR) Apache-2.0; kurbo and linesweeper MIT OR Apache-2.0;
Inkscape GPL-2+ and lib2geom LGPL-2.1 OR MPL-1.1, all compatible with our GPL-3.0-or-later. Clipper2 is BSL-1.0 (the
`clipper2` crate wraps C++ and is **forbidden**; `clipper2-rust` 1.2.0 is pure Rust).

| Operation | Pick (port / combine) | Why | Known failure modes of the candidates | Proof (beyond unit tests) |
|---|---|---|---|---|
| **Booleans / Pathfinder** | **linesweeper 0.4 + VC `pathops/boolean.rs`, `pathfinder.rs`** (sweep, then refit pieces to few cubics). Fallback on `Err`: retry with snap-rounding at `precision×10`, then on a flattened polygon (linesweeper handles lines robustly) followed by `kurbo::fit_to_bezpath_opt` refit. | Both VC **and Graphite** (`Cargo.toml`: `linesweeper = "0.4"`, kurbo 0.13) converged on it, and it works directly on cubics. | linesweeper: young (0.4→0.5 in Oct 2026) and precision-sensitive; the refit can round sharp corners and leave sliver faces. Livarot / lib2geom (Inkscape): known "coincident edge" artefacts and occasional dropped holes. flo_curves: slow, and loses tangent cases. Clipper2: perfectly robust, but polygons only (loses curves). | (1) **Coverage oracle**: rasterise A op B with `pc-vector`'s per-component PathOps (an independent implementation) vs the rasterised geometric result: max diff ≤ 2/255 at 4× AA. (2) Area identities: \|A∪B\| + \|A∩B\| = \|A\| + \|B\|; (A−B) ∪ (A∩B) ≈ A; xor = ∪ − ∩; A∪A = A; commutativity; all within 1e-6·area. (3) No self-intersections and no NaN in outputs. (4) Inkscape goldens on the **tricky corpus** (A.3), area diff < 0.1 % and Hausdorff < 0.05 px. (5) Node budget ≤ 1.5× Inkscape's count. |
| **Offset Path / Contour** | **kurbo 0.13 `offset::CubicOffset` / `kurbo::stroke`** (Levien's parallel-curve fitting) **+ linesweeper union cleanup** (= VC `offset.rs`). Joins: round / miter (limit) / bevel. Cross-check UX with GR `offset_bezpath.rs`. | The offset stays curves with few segments, and the union removes the loops that cusps create. | kurbo offset: loops at cusps and high curvature (removed by the union); inner offsets of thin parts must vanish, not invert (nonzero winding + union does this). Miter at near-180° explodes (limit it). Clipper2 offset is robust but polygonal. Inkscape Path › Offset (livarot) wobbles on large distances. | (1) **Distance-transform oracle**: a round-join offset at d equals the EDT threshold at d (IoU > 0.999, max boundary error < 0.05 px). (2) Monotonicity: area(offset(d₁)) < area(offset(d₂)) for d₁ < d₂. (3) Every sampled point on the result lies at distance d ± tol from the source (round joins). (4) offset(−d)∘offset(d) ≈ source for convex shapes. (5) Inkscape goldens (miter/bevel). |
| **Outline Stroke / Expand Stroke** | **`kurbo::stroke` + linesweeper union** (VC). Dashes via `kurbo::dash`; arrowheads from VC. | Same engine as offset; vello uses it. | Overlap loops in tight curves (union removes them). Square-cap direction on zero-length subpaths. Our `pc-vector/stroke.rs` polygon stroker stays the renderer, and may differ by sub-pixel AA. | (1) Rasterised outline vs `pc-vector` stroke coverage: max diff ≤ 2/255. (2) Round cap/join vs the EDT oracle. (3) Expanded and original look identical in parity rendering. |
| **Simplify / curve fitting** | **`kurbo::fit_to_bezpath_opt`** (optimal subdivision under a tolerance) for the fitting, with **VC `edit.rs`/`fit.rs` corner detection** deciding where to split first. | kurbo's optimiser gives the fewest cubics for a given error. VC detects corners. | Plain Schneider LSQ (VC, our `trace.rs`) over-segments and wobbles near corners. Potrace-style fitting works only for pixel boundaries. | Hausdorff(result, source) ≤ tolerance (property). Node count is non-increasing in tolerance. Corners (angle < threshold) are preserved as corner knots. |
| **Width tool / variable width** | **VC width profiles** (`doc/appearance.rs WidthProfile`, `engine tests_widthtool.rs`). Outline = sampled normal offsets fitted by kurbo, then union. Behaviour reference: Inkscape Power Stroke. | Matches Illustrator; already tested upstream. | Self-overlap on the inside of tight bends (union fixes it). Discontinuities where a profile point lies on a corner. | Analytic lens area for a 0→w→0 profile on a line (± 0.5 %). The constant profile equals the plain stroke (coverage oracle). Width at t equals the profile value (sampled). |
| **Knife / Scissors / Line Cut** | **VC `planar.rs::cut_out` + `tools/cut.rs`** (linesweeper arrangement). | It splits closed shapes into closed pieces correctly. | Cuts exactly through nodes or along an edge (degenerate). Open-path knife on curves near tangency. | Area conservation: the sum of pieces = the original (1e-6). Every piece is closed and has no self-intersection. A cut through a node gives the right piece count. |
| **Shape Builder** | **VC `planar.rs::shape_builder` + `tools/builder.rs`**. Behaviour reference: Inkscape 1.3 Shape Builder. | It builds the faces of the full planar arrangement. | Slivers from near-coincident edges (cleaned by a minimum-area threshold). Huge face counts (VC caps segments at `SHAPE_BUILDER_MAX_SEGMENTS`). | The face areas sum to the union area. Merging all faces equals the union (coverage oracle). The region count matches combinatorics on canonical cases (2 circles → 3, 3 Venn → 7). |
| **Corner tool / live corners** | **VC `geom/corners.rs`** (Illustrator live corners: round / inverted / chamfer), with radius clamping to the adjacent segment length. | It handles curved adjacent segments. | Radius larger than the half segment (clamp it). Corners between two curves (VC fits an arc approximation). | Square with radius r: area = s² − (4−π)r² exactly (± 1e-6). Chamfer: s² − 2r². Monotone in r. No self-intersections up to the maximum radius. |
| **Pen / Node editing** | **VC `tools/pen.rs`, `direct.rs`** state machines on our existing `vector_ui.rs` / `direct_select.rs`. Graphite `pen_tool.rs` / `path_tool.rs` is a UX reference only (Apache-2.0). | VC is Illustrator-faithful and replayable as commands. | Handle-symmetry drift on smooth nodes. Closing onto the first node with handles. Undo granularity. | Scripted replays of event sequences (ported from VC `engine/tests_*`) give the expected paths. Property: any random edit sequence keeps the path finite and valid, and undo restores a byte-identical document. Kittest gestures. |
| **Text on path** | **VC `text/layout.rs` on-path placement** on `pc-text` glyph runs, using SVG `textPath` semantics (startOffset, side, method=align) for SVG compatibility. | One model works for import, export and editing. | Glyph overlap in concave bends. Text longer than the path (show an overflow marker; don't clip silently). Closed-path start seam. | Glyph origins lie on the path (≤ 0.5 px) and rotations equal the tangent (± 0.5°). Advance sums equal the arc length used. Inkscape textPath goldens rendered with resvg vs ours: IoU > 0.97 (fonts: a committed OFL test font). |
| **Gradients / fills** | **Ours** (PhotoCraft, PSD-calibrated) + upstream PhotoCraft #1321 (midpoints, opacity stops). SVG mapping per usvg semantics. | It is already Photoshop-matched; the corpus tests exist. | spreadMethod reflect/repeat, gradientTransform with skew, focal radial gradients (fr). | resvg vs our render of gradient SVG fixtures: mean ΔE_ok < 0.5, max < 2 off-edge. GPU parity ≤ 1/255. |
| **Blend** | **VC `doc/blend.rs`/`live.rs`** (steps, distance, spine, orientation), with node correspondence by resampling to equal counts. | Illustrator semantics, with an upstream test suite (`tests_blend*`). | Different winding or start point gives twisted blends (normalise orientation and start point). Compound paths with different subpath counts. | Two circles, 5 steps: the radius and the Oklab colour interpolate linearly (exact). A blend with 0 steps equals the keys. The spine length matches. |
| **Vector brush** | **VC `brush` crate** (calligraphic, art stretch, scatter, pattern). Behaviour reference: Inkscape Pattern Along Path. Affinity texture brushes are later (they are raster textures). | It is complete, with tests. | Art-brush folding on tight curves, pattern corner tiles, scatter seeding (must be deterministic). | Calligraphic width = |sin(Δangle)|·w (analytic). A pattern along a straight line has exact tile spacing. Seeded scatter is reproducible. |
| **Repeat / Envelope** | **VC `doc/live.rs` (RepeatSpec) + `effects/warp.rs`** (envelope mesh / warp). | Live and non-destructive. | Envelope fidelity vs node explosion; warping text needs outlines. | An identity envelope gives the identity output (Hausdorff < 1e-6). A radial repeat of n has n-fold rotational symmetry (exact). |
| **Symbols / Constraints** | Own (Affinity semantics). SVG `<symbol>/<use>` mapping on import/export. | Simple data model. | Nested symbols (cycle detection required); constraints with rotation. | Editing the master updates N instances (pixel-exact). Cycle insertion is rejected. Constraint arithmetic table tests. |
| **SVG import** | **usvg 0.45.1** (normalises CSS, `<use>`, units, transforms, clip/mask) + **VC `svg/import*.rs` mapping**, including its **text placeholder trick** (`import/text.rs`). Our lock's usvg has **no text feature/fontdb**, so text must be read from XML exactly as VC does. Direct `roxmltree` 0.20 is the fallback for what usvg drops. | usvg is the de facto Rust SVG normaliser. | usvg drops `<use>` identity (fine) and drops text without fontdb (handled). Filters are unsupported (rasterise or ignore, with a warning). `<foreignObject>`. CSS edge cases. | (1) **resvg test-suite subset** (A.3): import → our render vs the resvg reference PNG, per-test IoU/ΔE thresholds, ratcheted. (2) Real-world corpus: no panics, warnings recorded. (3) Malformed/hostile SVG (billion-laughs-like nesting, huge viewBox, deep recursion) returns `Err` within 1 s. |
| **SVG export** | **VC `svg/export.rs`**, our own writer. | Clean, minimal output. | Blend modes and diamond/angle gradients have no SVG equivalent (rasterise, with a warning). Precision vs size. | Export → resvg render vs our render: IoU > 0.995, ΔE mean < 0.5. Export → import → export is stable (the second export is byte-identical). Output validates as XML (roxmltree parse). |
| **PDF export** | Own writer extending `print_cmds.rs`, ported **operator logic** from VC `pdf/export.rs`. krilla only if the owner approves (main spec Q3). | No new crates. | Text as outlines only in v1. Transparency groups and soft masks. Blend-mode coverage. | Structural assertions (operators present, no images for pure vectors). Coordinator visual check with `pdftoppm`/`mutool` vs our render (manual gate). |
| **PDF import** | Deferred (needs hayro; owner Q3). | | | |
| **Crisp zoom** | Own (main spec §9.3): `pc-vector` with an `Affine` per view tile; vello is a reference only. | It is the single rasteriser, so parity is easy. | Tile seams, cache thrash while zooming, text hinting differences. | `zoomed_vectors_match_cpu` at s = 2, 4, 8. A seam test: render a rect across 4 tiles and compare with an untiled render (exact). Hairline sharpness profile. Perf: < 16 ms per tile repaint. |

### A.3 Corpora (all committed, licences recorded, total ≤ 15 MB)

1. **Tricky geometry corpus** (`crates/pc-pathops/tests/tricky/`, own SVGs, about 60 cases):
   - Geometry cases:
     - coincident edges, touching circles, a circle tangent to a line
     - figure-8 self-intersection, a star with 50 overlapping spikes
     - nested holes, holes touching the outer edge, identical paths, a path with zero-area spikes
     - cusps and loops in single cubics
   - Scale and precision cases:
     - near-degenerate slivers (1e-4)
     - coordinates near 1e6
     - a shape fully containing another
     - open paths given to booleans (they must be closed first)
   - Goldens from Inkscape for union, difference, intersection, exclusion, division, offset ±4, ±20 (round/miter/bevel), stroke to
     path (caps and joins), and simplify.
2. **resvg test suite subset.**
   - Source: `linebender/resvg` `crates/resvg/tests/` (MIT OR Apache-2.0), SVG + reference PNG pairs.
   - Take about 400 cases covering structure, shapes, painting (fill, stroke, markers, gradients, patterns), masking, text
     (basic) and `<use>`.
   - Pin the commit and keep the case list in `SOURCES.md`.
3. **Real-world SVG stress corpus** (about 40 files):
   - Openclipart (CC0) and Wikimedia Commons files marked public domain (PD-shape / PD-textlogo).
   - Each file's licence and URL is in `SOURCES.md`.
   - Includes icons (Lucide is ISC: allowed, include a few), maps, logos and illustrations from Inkscape/Illustrator
     exports.
4. **v1 document fixtures** (`crates/pc-format/tests/fixtures/`): shapes, vector masks and paths written by the
   current build, for the backward-compatibility gates.

### A.4 Per-phase definition of done (DoD)

These gates apply to every phase:
- All tests green.
- `fmt` and `clippy -D warnings` clean.
- `git diff Cargo.lock` shows only approved packages.
- No `unwrap`/`expect`/`panic` in shipped code.
- Property tests pass at 256 cases in Codex; the coordinator runs them at 10 000 nightly.
- The ratchet files have not grown.
- GPU parity has been run on the RTX 5090.
- The manual QA list below is signed off in the merge note.

| Phase | Automated gate (in addition to the main spec's acceptance) | Manual QA (coordinator, in the app) |
|---|---|---|
| **V1 / V1-Q** (geometry) | The coverage oracle on 200 random pairs × 5 ops. Area identities. EDT oracle for offset/stroke. The tricky corpus vs Inkscape: ≥ 95 % within thresholds, the rest ratcheted with reasons. Zero NaN or panic across 10 k random inputs (nightly). Perf: a 1000-node boolean < 50 ms. | Via engine commands from the automation console: union, subtract and divide two overlapping text-outline paths; offset a star by ±10 and inspect the joins; expand a dashed stroke. Reopen a PSD containing the results in the app and confirm it renders the same. |
| **V2** (persona) | Kittest tests per the main spec. A UI-state serde round trip. The Pixel persona's tool list is unchanged vs the current `TOOL_SECTIONS` (snapshot). | Switch personas 20× in the middle of a Pen gesture: no stuck tool. Every vector tool and Geometry button has a tooltip and works with Undo/Redo. |
| **V3** (draw/edit) | VC-derived replay tests. Corner area formulas. Each `LiveShape` area vs analytic. Pencil fitting Hausdorff ≤ tol. Snapping exactness. Random edit sequences keep the path valid; undo is byte-identical. | Draw a logo with the Pen (curves, corners, close, continue). Node tool: marquee, convert, break/join. Corner tool on a rounded star. Pencil with stabiliser on a tablet. Snapping to a guide and to another shape's node. |
| **V4** (appearance) | GPU parity for the appearance tests. resvg-vs-ours on gradient fixtures. PSD multi-stroke export: identical pixels + warning. | Build a 2-fill / 3-stroke badge. Gradient-along-stroke. Transparency tool fade. Save → reopen `.pcraft`. Export PSD → reopen. |
| **V5** (crisp zoom + constructive) | Zoomed parity at s = 2, 4, 8. The tile seam test. Shape Builder face-sum = union. Knife area conservation. Width lens area. EDT oracle for Contour. Perf budgets. | Zoom to 3200 % on curves and type: sharp, no seams, no lag when panning. Shape Builder merge/remove on a Venn. Knife through text outlines. Width tool on a calligraphic line. |
| **V6** (type) | On-path glyph tests. Inkscape textPath goldens vs ours (IoU > 0.97). Parity `text_on_path_layers`. | Text on a circle; flip; drag start/end; edit the text; convert to curves; long text overflow indicator. Japanese text on a path (vertical-capable font). |
| **V7** (SVG) | resvg suite pass rate ≥ 90 % (ratchet the rest). Round-trip stability. Hostile-input timeouts. The real-world corpus loads with no panic. | Place 10 logos from the real-world corpus; edit the imported shapes; export the selection as SVG and open it in a browser and in Inkscape. |
| **V8** (structure) | Symbol instance pixels. Cycle rejection. Constraint table. The format v2 bump with the `TooNew` test. | Make a symbol, place 5, edit the master. Resize an artboard with constraints. Align/distribute with a key object. |
| **V9** (live/advanced) | Blend, brush and repeat analytic tests. Envelope identity. Determinism (same seed → identical output). | Blend two shapes along a spine. Calligraphic and art brushes on Pencil strokes. Radial repeat mandala. Envelope warp on text. |
| **V10** (PDF) | Structural PDF tests. Vector-only documents contain no raster images. | Open the exported PDF in a viewer and in Inkscape; check colours, gradients and text outlines; print-preview size. |

### A.5 Changes this plan makes to the main spec

1. Add **V1-Q**, a follow-up to the running V1 job. It adds the shared test infrastructure (A.1), the coverage and EDT oracles, the
   property tests, the tricky corpus with Inkscape goldens, the ratchet files, the kurbo `fit_to_bezpath_opt` refit fallback, and
   snap-rounding retries for booleans.
2. Use **kurbo's own offset/stroke/fit** (kurbo 0.13.1, already locked) as the curve engine under the VC wrappers. This is
   explicit now; VC already does it for strokes.
3. **SVG import:** usvg has no text support in our lock, so the VC text-placeholder path (`svg/src/import/text.rs`) is
   mandatory in V7.
4. **resvg 0.45.1 becomes a test-only reference renderer** (a dev-dependency, no new packages).
5. **Never use `dssim-core`** (AGPL-3.0) and never use the `clipper2` crate (C++ -sys). `clipper2-rust` (BSL-1.0, pure Rust) is not
   needed; use it only if the polygon fallback proves insufficient. That would need owner approval, as a new crate.
6. Corpora and goldens are generated by the coordinator. **Inkscape 1.4.4 and potrace 1.16 must be installed on the coordinator machine**
   (owner approval for `dnf install`).

---

## Part B: Vectorizer ("Vectorize…" / Image Trace)

### B.1 Candidates evaluated

| Candidate | Licence / form | Strengths | Weaknesses |
|---|---|---|---|
| **vtracer 1.0.0-alpha.4** (visioncortex/vtracer @ `928ed0a6f654408e28fb741b6133d4c456bd0160`, Oct 2026) + **visioncortex 0.9.3** (@ `0062088c89645aac76c00e066deb7e8f53980dd7`) | MIT OR Apache-2.0, pure Rust | Best open-source colour tracer. Stacked layering (no holes, compact). **Seam-free "cutout" mosaic with shared boundaries.** Watershed edge-aware clustering. Oklab palette snap and `--max-colors`. Bradley–Roth adaptive threshold for scans. Optimal-ish `--simplify`. Pixel/polygon/spline modes. Has its own blind benchmark (vtracer-bench). | Splines use a 4-point subdivision + fit, which is weaker than potrace on small, clean B&W logos (rounder corners, less straight lines). Depends on `flo_curves` (two versions) + `bit-vec` 0.6, which are not in our lock. Alpha-quality API. |
| **potrace 1.16** (Peter Selinger) | **GPL-2.0-or-later** (C), so a port to Rust is allowed into GPL-3.0-or-later with the notice | The gold standard for **binary** images. Optimal polygon (penalty-minimising), `alphamax` corner/smooth decision, curve optimisation (`opttolerance`), turn policy, `turdsize`. Crisp straight lines and corners on logos and type. | Binary only. Colour needs multi-scan (Inkscape stacks potrace per colour or brightness level, which gives gaps/overlaps). Slow optimal-polygon search on huge images (fine for logos). |
| **VectorCraft Image Trace** (`crates/trace` @ `d522c1d7…`) | MIT OR Apache-2.0, pure Rust, clean-room "from the potrace paper" | Illustrator-style presets and parameters (palette modes, noise, abutting/overlapping, **centreline** tracing for strokes, logo mode, mosaic), already shaped like our UI needs. `geom/recognize.rs` recognises shapes (lines, circles). | Its polygon stage is Douglas–Peucker and its curves are LSQ fit: lower fidelity per node than potrace or vtracer. Young. |
| Inkscape Trace Bitmap | GPL-2+ (C++), potrace + multi-scan + Kopf–Lischinski pixel art (libdepixelize) | Brightness, colour and autotrace modes. Depixelize for pixel art. | Multi-scan stacking artefacts. Algorithm reference only. |
| autotrace | GPL-2+ (C) | Centreline tracing | Dated, noisy output. Reference only. |
| Adobe Image Trace / Affinity / Vectorizer.AI | Proprietary | UX and presets reference (logo, B&W, 3/6/16 colours, high fidelity photo, line art, sketched art, silhouettes, shades of grey). Vectorizer.AI-quality hallmarks: shared edges, geometric snapping, symmetry. | Behaviour only. |

### B.2 Recommendation: build `pc-trace` (a new crate) by combining the three

**Port, don't depend.** Porting adds **no new crates.** Do not depend on vtracer or visioncortex directly: they would bring `flo_curves` ×2, `bit-vec` 0.6, `ouroboros`,
`roots` and `itertools` into the lock. The pipeline below is ported instead, and **kurbo 0.13.1** replaces flo_curves for fitting.

1. **Front end / regions: vtracer 1.0 + visioncortex.**
   - Colour clustering (hierarchical, Oklab), watershed region forming (edge-aware; good on JPEG logos and photos), Bradley–Roth adaptive threshold, palette snap /
     max-colours, speckle filter.
   - Port from `crates/vtracer/src/{frontend/*, colorfit/*, pipeline.rs, mosaic/*, simplify.rs}` and the needed
     `visioncortex/src/{color_clusters/*, clusters.rs, path/*, disjoint_sets.rs, color.rs}`.
2. **Layering.**
   - **Stacked** (vtracer default; compact, gap-free): this is the default.
   - **Cutout / Abutting** (vtracer seam-free mosaic, shared boundaries) for designers who need no overlaps.
3. **Boundary → curves, per region or per shared boundary chain.** There are two fitters, and the benchmark (B.4) picks the default per preset:
   - **potrace port** (decomposition is replaced by the region boundaries; optimal polygon → alphamax corners → curve optimisation).
     It is used for B&W, Logo and Silhouette, where crisp straight lines and corners matter.
   - **vtracer spline/simplify** for Photo and high-colour presets (it is faster and faithful at high resolution).
   - Both emit `kurbo::BezPath` and are refined by `kurbo::fit_to_bezpath_opt` under the Detail tolerance.
4. **Centreline mode (Line Art / Sketch):** VectorCraft `trace/centerline.rs`. Its output is stroked open paths, with width from a
   distance transform.
5. **Our improvements (logo cleanup, all optional toggles):**
   - Pre-filter: an edge-preserving denoise to remove JPEG ringing. Reuse the guided filter already ported in
     `crates/lc-pipeline/src/detail/haze.rs` (darktable), or a small bilateral filter.
   - **Auto-upscale** small inputs (< 512 px on the long side) ×2–4 with an edge-aware resample before tracing, then scale the
     paths back. This gives much smoother curves on small logos.
   - **Anti-alias-aware palette:** estimate the palette from low-gradient pixels only, then assign edge pixels by nearest Oklab
     colour, so fringe colours don't become extra layers.
   - **Geometric snapping:**
     - Snap near-horizontal and near-vertical segments to the axes (angle tolerance).
     - Merge collinear segments.
     - Fit circles, ellipses and arcs where the residual < tol (VectorCraft `geom/recognize.rs`).
     - Snap right angles.
     - **Symmetry:** detect a mirror axis (IoU of the region mask vs its reflection > 0.98), then symmetrise the paths.
   - Gap filling and hole policy: small holes (< speckle) are filled. Contrasting text counters are always kept.
   - **Ignore colour / background removal:** a picked colour, or the existing alpha, becomes transparent, so a white-background
     logo traces without the white box.
6. **Output:**
   - A group "Vectorized – <layer name>" with **one shape layer per colour** (a compound `Path`, nonzero, back-to-front by
     stacking order), named by hex colour.
   - "Expand to shape layers" splits into one layer per connected shape.
   - A "Make Work Path" option (Paths panel) for B&W, which gives photographers a vector mask of a logo.
   - The output uses our `pc_doc::Path` (`Knot{anchor, in_ctrl, out_ctrl, smooth}`), the model V1 adapts to, via a
     `bezpath_to_path` adapter local to `pc-trace`. **`pc-trace` must not depend on `pc-pathops`**, so it can run in parallel with V1.
     After V1 merges, an optional cleanup (union per colour, `path.simplify`) can be wired in.

**Presets.** Parameters map to the sliders.

| Preset | Description |
|---|---|
| **Logo** | auto colours ≤ 8, stacked, potrace fitter, snapping on, symmetry on, speckle 4 px, auto-upscale on |
| **Black & White** | fixed or adaptive threshold, potrace (alphamax 1.0, opttolerance 0.2, turdsize 2) |
| **Few colours** | 3 / 6 / 16 colours, stacked, potrace fitter |
| **Silhouette** | alpha or threshold, a single fill |
| **Line Art** | centreline, strokes, adaptive threshold |
| **Sketch** | centreline + strong smoothing + adaptive threshold |
| **Photo (high fidelity)** | 32–64 colours, watershed, vtracer spline, cutout off |
| **Pixel Art** | polygon mode, no smoothing, exact pixel corners (vtracer `pixel`/`polygon`) |

### B.3 UX

**Entry points.**
- Compositing: **Layer › Vectorize…**, on a pixel layer, a smart object, or the active selection, which limits the
  area. In the Vector persona it is also listed as **Object › Image Trace…** and opens the same dialog.
- Library: right-click on a photo → **Vectorize in Compositing…** opens the image in Compositing with the dialog open.
- There is no separate Develop-side tracer. A direct "Export as SVG (vectorized)" from Library is a later item.

**Dialog** (modal, resizable, remembers its last preset):
- **Left: preview.** A split before/after slider, with view modes Result / Outlines (paths + nodes) / Overlay.
  It zooms and pans independently of the canvas.
- **Right: controls.**
  - Preset dropdown.
  - Mode: Fill / Centreline.
  - Layering: Stacked / Cutout.
  - Colours (Auto, 2–64), with a palette strip; click a swatch to merge or remove it; "Use document swatches".
  - Detail (fitting tolerance).
  - Corners (alphamax / corner threshold).
  - Noise (speckle px).
  - Smoothing (pre-filter).
  - Snap shapes (Lines, Circles, Symmetry checkboxes).
  - Ignore colour (picker) / Use alpha.
  - Upscale (Auto/1×/2×/4×).
- **Stats line:** colours, paths, nodes, time.
- **Buttons:** Trace (apply), Expand to shape layers (checkbox), Make Work Path (B&W only), Cancel.

**Behaviour.**
- The preview runs on a background job (`crates/pc-engine/src/jobs.rs` pattern), at a reduced scale while sliders move
  and full scale on release. It is cancellable; the latest request wins.
- Apply is a single undo step, using the engine command `layer.vectorize {params}`.

### B.4 Quality benchmark (committed, deterministic, offline)

**Corpus** (`crates/pc-trace/tests/corpus/`, ≤ 5 MB, each item's licence in `SOURCES.md`). It comes from two sources.

**Synthetic logos with ground truth.**
- About 30 logos are generated **at test time** by code from our own shapes (circles, rounded rects, stars, arrows, a
  wordmark from a committed OFL font's outlines), so the ground-truth vectors are known.
- They are rendered with `pc-vector` at 256, 512 and 1024 px.
- Each has degraded variants:
  - clean antialiased
  - JPEG q = 60 (the `image` crate's encoder)
  - Gaussian σ = 1
  - noise σ = 4/255
  - 0.5× downscale → upscale
  - "scan": 0.7° rotation + paper texture + uneven lighting

**Real clip art.**
- About 15 CC0 / public-domain logos and clip art (Openclipart CC0, Wikimedia PD-textlogo/PD-shape), stored as
  **SVG**. The test rasterises them with resvg (a dev-dependency), so ground truth exists for these too.
- Plus about 5 pixel-art and line-art scans that the owner or coordinator provides with a licence (no ground truth;
  fidelity vs the input only).

**Metrics per image** (`pc-testkit` A.1):
- Re-rasterised **fidelity** (vtracer-bench composite) and **MS-SSIM**, measured vs the **clean** ground-truth render. For a degraded input,
  the tracer should *recover* the clean logo.
- Mean/95th-percentile **ΔE_ok**.
- Per-colour IoU and boundary Hausdorff vs the ground-truth vectors.
- **Node count**, with its ratio to the ground-truth node count.
- Time (release build).

**Reference competitors.** The coordinator runs these once and commits the SVGs + `scores.json`:
- vtracer 1.0.0-alpha.4 CLI, with matching presets
- vtracer 0.6.5
- potrace 1.16 on the B&W subset (after the same threshold)
- the test renders the committed reference SVGs with resvg and scores them with the same code

**Gates.**
- Per preset, our median fidelity ≥ the best competitor's median − 0.005.
- Ours ≥ the best competitor on ≥ 70 % of the images, or ≥ 80 % for Logo/B&W.
- Median node count ≤ 1.2× the competitor with the closest fidelity.
- Logo preset at 1024² < 1.5 s, and Photo at 2048² < 8 s (release build, coordinator machine).
- The parameter-to-default choice (potrace vs spline fitter per preset) is **decided by this benchmark**, and the
  decision is recorded in `SOURCES.md`.

**Robustness properties.**
- Every output path is finite, closed (for fills) and free of self-intersections. Use A.1's checker after V1-Q lands; until
  then use a local copy.
- Fully transparent and single-colour inputs give empty or one-shape output, never a panic.
- 1×1 and 16000×16000 inputs are handled (refused above a pixel budget with a message).
- Output is deterministic: identical bytes across runs and thread counts.

### B.5 Jobs (can start now, in parallel with V1)

**T1: `pc-trace` engine + benchmark (large; Codex xhigh).**
- **Scope:**
  - Ports:
    - vtracer 1.0 front end (colour clustering, watershed, adaptive threshold, palette/max-colours, speckle)
    - stacked layering
    - the **potrace port** (optimal polygon, alphamax, opticurve; files carry the GPL-2.0-or-later notice "Ported from Potrace 1.16 © Peter Selinger")
    - the vtracer spline fitter
  - kurbo refit, the `bezpath_to_path` adapter, and the presets.
  - Engine command `layer.vectorize` (returns a group of shape layers; the preview variant returns paths at a scale).
  - The `pc-testkit` metrics needed (fidelity, MS-SSIM, ΔE_ok, IoU, Hausdorff), the synthetic corpus generator, the
    benchmark test (`#[ignore]` full run plus a fast 6-image subset in normal tests) and the property tests.
- **New crates: none.** kurbo 0.13.1, `image`, resvg 0.45.1 (dev) and proptest are all in the lock.
  Attribution (PORTS rows):
  - visioncortex/vtracer (MIT OR Apache-2.0)
  - visioncortex/visioncortex (MIT OR Apache-2.0)
  - potrace 1.16 (GPL-2.0-or-later; upstream is a tarball, not git: record `potrace-1.16.tar.gz` + its sha256 in the commit column)
  - licence notices in `licenses/` (`vtracer-LICENSE-MIT/APACHE`, `visioncortex-LICENSE-*`, `potrace-NOTICE.md`)
- **Done when:** the B.4 gates pass on the fast subset in Codex, and the coordinator runs the full benchmark with the
  competitor references.

**T2: Dialog + cleanup + centreline (medium-large).**
- **Scope:**
  - The B.3 dialog with background preview and kittest tests:
    - opening the dialog on a pixel layer enables Trace
    - changing a preset updates the sliders
    - Apply creates the group with N colour layers, and undo removes it
    - Make Work Path adds a path to the Paths panel
    - the dialog is disabled on non-pixel layers with a tooltip
  - Cutout (seam-free mosaic) layering, the centreline mode (VectorCraft `trace/centerline.rs`), and the logo cleanup
    (axis/collinear/circle snapping via VectorCraft `geom/recognize.rs`, symmetry), the anti-alias-aware palette, auto-upscale, the
    pre-filter, and the Library hand-off.
- **Done when:** the full B.4 gates pass (coordinator run), with the cleanup measured to not reduce fidelity by more than 0.002 while
  cutting nodes by at least 15 % on the Logo set.

**Coordinator before T1:**
- Provide the upstream sources offline in the worktree (`vendor-src/`: vtracer @ `928ed0a6…`,
  visioncortex @ `0062088c…`, potrace-1.16 tarball, vectorcraft @ `d522c1d7…`).
- Generate the competitor reference SVGs (needs `dnf install potrace`, which needs the owner's OK, and `cargo install vtracer-cli` into the scratchpad).
- Add the CC0/PD clip-art SVGs with `SOURCES.md`.

### B.6 Manual QA checklist (vectorizer)

- Trace 5 real logos (a JPEG from the web, a phone photo of a printed logo, a small 200 px PNG, a pixel-art sprite, a
  pencil sketch) with Logo / B&W / Sketch / Pixel Art. Inspect them at 800 % in Outlines view:
  - straight lines are straight
  - circles are round
  - no hairline gaps, no stray specks
  - text counters are kept
- Ignore white: the logo sits on transparency. Edit one colour layer's fill afterwards.
- Expand to shape layers, then move one letter.
- Make Work Path → vector mask on a photo layer.
- Moving the sliders keeps the preview responsive (UI stays at 60 fps), and Cancel leaves the document untouched.
- Export the result as SVG (after V7) or PSD (shape layers), then reopen it.
