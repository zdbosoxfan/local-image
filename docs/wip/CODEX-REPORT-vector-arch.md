# Vector architecture report — 2026-10-10

Completed the architecture/foundation/work-package scope on `wip/codex-vector-arch`, based
on `a7cf38c7`. Continued the existing Move/GPU/tool-set/trace work. At initial inspection the
only untracked item was the task brief; there was no separate pending architecture patch.
After the interruption, rechecked git status and retained logs under `target/vector-arch/`.
No commit was made. All writes stay in this worktree; reference projects were read-only.

## 1. Spec re-verification

[Architecture §1](../specs/vector-architecture.md) records the corrected baseline with exact
source paths. Current code already includes five tool sets and 53 canonical tools, colour and
mono icons, the captured Move/transform preview path, direct GPU layer compositing, and
Vectorizer T1. Persona is still Pixel-only; patterns/appearance/path-text/symbol/live features
remain implementation packages. LineArt/Sketch are rejected until T2 centreline support.

Official upstream commit/blob comparisons establish that our Direct Selection engine/UI/edit
files match [#865](https://github.com/storytold/photocraft/commit/75ff41a26beb445413f6ee9248eb40b42152bd77),
and raster bounds code matches [#747](https://github.com/storytold/photocraft/commit/8302f57190c83439adb961ae23b9b110e819dea3).
The shape-gradient change [#1321](https://github.com/storytold/photocraft/commit/d438f842256435dba7aa41ca306656a216b98fbf)
is **missing**: the model stores midpoint/opacity/offset/dither/align, but shape Paint ignores
them. Package 00 owns that demonstrated V0 port before appearance integration. This task
requested re-verification, so it records the gap rather than silently declaring V0 complete.
Per-file provenance is established; the original whole-app fork hash remains unrecorded.

## 2. Final library decisions

[Architecture §2](../specs/vector-architecture.md) fixes versions, licenses, feature policy and
dependency direction. Use the actual V1 pc-pathops adapter with linesweeper 0.4.0 / kurbo
0.13.1 / already-locked polycool 0.4.0. Retain exact-area pc-vector coverage and pc-text shaping;
no lyon, flo_curves, i_overlay, dssim or C/-sys addition. Retain locked usvg/resvg 0.45.1,
svgtypes 0.15.3 and roxmltree 0.20.0; extract/reinsert SVG text through pc-text because the
current usvg graph has no text/fontdb features. Affinity is a later bounded VectorCraft reader
port using already-locked Rust compression/image dependencies.

Full PDF uses krilla 0.8.2 with only raster-images, and hayro-interpret 0.7.0 / hayro-syntax
0.7.2 with embedded substitute fonts/cmaps and editable Device callbacks. Krilla gates even
raw/custom images behind raster-images, so full export/fallback needs it. No second shaper or
hayro standalone renderer. The [metadata/source audit](vector-tasks/pdf-dependency-audit.md)
and [checksum/feature/license allowlist](vector-tasks/pdf-dependencies.tsv) contain 82 registry
packages, **19 additional crate/version pairs** relative to our lock, all Rust and no new
custom build script/C/-sys. Seeded offline resolution succeeds; fresh resolution hits the
already-locked yanked yoke-derive 0.8.3 entry. Package 00 must preserve pins and check unified
workspace features against the snapshot when integrating the owner-approved graph.

Hayro requires dependency-internal pure Rust unsafe SIMD. Workspace unsafe lint still forbids
unsafe in our adapters; this is explicitly distinguished from a safe-only dependency graph.
No PDF registry dependency was added to production in this foundation. Cargo.lock adds only
three dev dependency edges (pc-gpu/wgpu/pollster already existed); **zero registry packages**.

## 3. Shared foundations landed

- `pc-doc::vector_design`: typed live shapes/corners, Appearance/FillItem/StrokeItem,
  WidthProfile/brush/arrows, PathText, Blend/Repeat/Envelope, symbols, constraints/design units,
  crop path and shared SVG/PDF/Affinity import/export/warning boundaries.
- `Layer.vector` and `Document.symbols/design_units` are additive. `pc-format` mirrors them
  with defaults and empty/default omission; old v1 native files load. Master layers use the
  normal content-addressed tiles/blobs, ID remapping, group-depth limit and directory GC.
- `pc-ops` history accounts for symbol master pixels and deduplicates shared COW tiles.
- `pc-engine::vector_api` snapshots detached multi-shape previews and validates/prepares
  atomic commits through existing Session::edit. Journaled `shape.applyPaths` publishes
  multiple prepared path caches in one revision/undo step; errors leave state untouched.
- 26 compiled package-owned engine command slots expose no placeholder successful commands.
  Package 00 alone wires shared module/router/manifests/i18n; feature jobs own separate files.
- Frozen native-v1 ZIP/JSON fixtures and original source record are checked in; the retained
  example documents generation. Tests read the frozen bytes rather than regenerating them.
- VectorCraft notices, per-file PORTS rows and generated app attribution entries are included.

The `Layer.vector` organization deliberately refines the spec's illustrative struct locations:
it preserves legacy Copy Knot/TextShape and exhaustive content/live-shape matches, while
letting independent packages own metadata modules. FORMAT_VERSION stays 1 until package 15
adds both symbol-instance content and version 2/migration. Merely storing metadata does not
implement advanced rendering. These integration choices and the scaled-view interface are
Local Image-specific designs; their geometry/paint/file semantics follow the cited references.

[Architecture §§3–8](../specs/vector-architecture.md) supplies concrete serde/persistence,
actual V1 signatures (including curve bounds), dependency-cycle correction, detached
Begin/Preview/Commit behavior, UI hook ownership, HiDPI/view tiles through the real GPU canvas,
mixed-stack clipping/effect padding/CMS, SVG/PDF/Affinity layering and PSD fallback warnings.
The PSD paint mirror ambiguity is resolved to the topmost visible supported fill/stroke.

## 4. Ready-to-run packages

[The index](vector-tasks/README.md) links **32 numbered task files (00–31)**. Required V2–V10,
Vectorizer T2 and Affinity scope is in 01–29; 00 is the serial integration lane, 30 optional
frame composition, 31 V1-quality follow-up. Every file lists owned paths, interfaces, scope,
independent start work, final dependencies, real RTX render gates, actual click/drag UI tests,
attribution and a report path. Each ends with the task's complete final Rules block verbatim.

[deepseek.tsv](vector-tasks/deepseek.tsv) has the exact `pkg, crate_dir, module, module_file`
tab-separated header and **14 unique small test/fixture jobs**, specified in
[deepseek.md](vector-tasks/deepseek.md). They wait for the owning API and do not own algorithms
or shared registration files. Final UI acceptance requires integrated app routes; isolated
widgets/stubs alone never satisfy a package. V1 remains the separate sibling branch's job.

## Validation

All cargo commands use `+1.98.1`, `--offline`, `CARGO_BUILD_JOBS=3`; one build at a time.
Formatting check passed. All-target clippy with `CARGO_TARGET_DIR=target/clippy` and
`-D warnings` passed for photocraft-doc/format/engine/ops. Full tests for exactly those four
changed crates ran once at the end; counts are recorded below.

| Changed crate | Unit | Coverage | Integration | Passed total | Ignored |
|---|---:|---:|---:|---:|---:|
| photocraft-doc | 50 | 102 | — | 152 | 0 |
| photocraft-engine | 850 | 582 | 47 | 1,479 | 16 |
| photocraft-format | 24 | 39 | 66 | 129 | 0 |
| photocraft-ops | 8 | — | — | 8 | 0 |
| **Total** | | | | **1,768** | **16** |

Zero failures. Doc-test targets contain zero tests and also passed. The 16 pre-existing
ignored engine tests are 12 timing/benchmark cases, two slow fuzz-process cases, one
user-supplied Camera Raw fixture and one manual Photoshop export check; none is the new
GPU gate. They were not counted as passes. The full-run GPU test passed again. Build output
retains the existing missing craft-fonts/CJK embedding warning; no external font tree was
modified or fetched.

New tests: **14** (2 doc, 5 native format, 5 engine transaction, 1 history, 1 GPU). Focused
runs passed before the final suite. Compatibility covers frozen old documents, omitted
extension fields, extended ZIP/directory round-trip, master tile GC/ID remapping/depth;
transaction coverage includes detached preview, cache publication, one journal/revision/undo,
redo, duplicate/missing/invalid targets, locks, finite geometry and metadata sharing history.
History tests cover nested masters and shared tile memory. Seven attribution checks passed
(`cargo test --offline -p xtask attributions`); generated data is current.

Added/executed `vector_foundation_transaction_cache_matches_real_gpu`: headless Vulkan,
**NVIDIA GeForce RTX 5090**, DiscreteGpu, NVIDIA driver **615.71.09**, Rgba32Float accumulator.
It compares premultiplied CPU/GPU pixels before edit, after command, undo and redo at
max difference ≤1/255. It fails for software/wrong adapters or missing GPU; no successful skip.
The focused run passed in 1.04s. Logs are in `target/vector-arch/{gpu-focused,clippy,final-tests,
attributions-test}.log`; API/provenance comparisons and PDF metadata are alongside them.

No UI code was changed in this foundation, so no invented UI interaction result or app
performance claim is made. Future task acceptance explicitly requires actual input and
real-app speed measurements. No app was killed/window opened and ComfyUI was not needed.
No PSD corpus files were present in `corpus/`; no corpus/competitor/performance gate is
claimed passed. Those checks remain explicit package/coordinator gates, including T1's
previously missing competitor references.

## Sources followed and scope remaining

Read-only VectorCraft source at `d522c1d7be4035bd4f4a84cd6ebfca44f5155092`, especially
`crates/{doc,geom,pathops,tools,svg,pdf,affinity}`; the actual sibling V1 public adapter;
current Local Image modules/tests/reports. The adapted appearance/live data designs are
recorded in [PORTS](../PORTS.md) with MIT/Apache notices. Optional DesignCraft composer is
pinned at `66c7ce7e96ec286a091878cc2541ee8369aa79d6`.

Primary semantics: [SVG 1.1 paths](https://www.w3.org/TR/SVG11/paths.html),
[painting](https://www.w3.org/TR/SVG11/painting.html),
[SVG 2 textPath](https://www.w3.org/TR/SVG2/text.html#TextPathElement),
[PDF 1.7 reference](https://opensource.adobe.com/dc-acrobat-sdk-docs/pdfstandards/pdfreference1.7old.pdf),
[krilla features](https://docs.rs/crate/krilla/0.8.2/source/Cargo.toml.orig),
[hayro Device](https://docs.rs/hayro-interpret/0.7.0/hayro_interpret/trait.Device.html),
[kurbo](https://docs.rs/kurbo/0.13.1/kurbo/), [linesweeper](https://docs.rs/linesweeper/0.4.0/linesweeper/),
[lyon](https://docs.rs/lyon/latest/lyon/),
[Affinity personas](https://affinity.help/designer2/en-US.lproj/pages/Introduction/about_Personas.html),
[Corner tool](https://affinity.help/designer2/en-US.lproj/pages/Tools/tools_corner.html), and
[Inkscape Shape Builder](https://wiki.inkscape.org/wiki/Release_notes/1.3).

No requested architecture/foundation/package artifact remains undone. Full feature tools,
renderers, SVG/PDF/Affinity adapters, T2 UI/algorithms, format v2 symbol instances and the
known V0 gradient gap are subsequent implementation scopes assigned explicitly to the task
files, not features represented as completed by this architecture job. Leave all changes
uncommitted for the coordinator.
