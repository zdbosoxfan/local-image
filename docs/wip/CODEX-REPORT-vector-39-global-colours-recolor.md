# Package 39 — document colours and Recolor Artwork

Status: owned implementation delivered; **integrated acceptance remains pending package 00**.
This is not a claim that the feature is already exposed in the running app. The task forbids
editing shared data/mirrors/router/catalog/command aggregation outside the serial integration
lane. Those interfaces are supplied concretely in
[the integration handoff](vector-tasks/integration/39.md). The final owner speed rules also
forbid GPU acceptance runs, release builds, benchmarks and full suites in this job.

## Work preserved and changes

Continued the existing vector foundations at `cc11baffe7f112b611fd72f49cc1cfdf0957c04b`.
The initial working tree contained only the untracked CODEX-TASK.md; none of package 39's
owned colour/recolour/data/ASE modules existed. Existing builders' geometry, renderer,
appearance, symbol, persona and transaction code was retained. No commits, dependency,
Cargo.lock, format-version, catalog or application-router edits. Eight new module declarations
in four shared files are the task's explicit REGISTRATION exception.

- **Document palette/data:** stable swatch IDs, solid/gradient paints, swatch groups, global
  and spot flags, optional per-paint/stop references, gradient-swatch stop links and native
  remapping helper. Serde defaults load `{}` as an empty book; complete data roundtrip tests.
  `SwatchBook` is an adapter snapshot of Document.swatches/paint references, with an explicit
  injected Access read/write contract. Production never hides colour state in XMP. The test
  provider uses metadata only to simulate document-owned history while shared storage is absent.
- **Swatch transactions:** add/edit, group/ungroup/delete group, add used colours, merge into
  another swatch, assign solids/gradients to fill/stroke/appearance paints and solid colours
  to gradient stops, import merge/replace, delete/global-off with conversion to local colours.
  Existing links resolve before deletion/replacement; gradient-palette stop links follow edits.
  Book, entire target set, inherited locks, IDs/revision, finite/bounded inputs and aggregate
  cache budget are validated. Prepared caches use the existing Appearance/corner evaluators;
  symbol dependents refresh through 15. Publishing is one Session::edit; journaled specs are
  supplied for 00's aggregate dispatch.
- **Tint:** percent 0..100, alpha retained. RGB/Gray mix in encoded/model space toward white;
  CMYK ink components scale; Lab mixes with L*=100,a*=b*=0. A 100% use returns the original
  colour exactly. Example: RGB (0.2,0.4,0.6), 50% -> (0.6,0.7,0.8). This follows the pinned
  VectorCraft tint semantics, not a gamma-linear RGB mixture.
- **ASE:** bounded all-Rust reader/writer for ASE 1.x, big-endian fields, UTF-16BE names,
  RGB/CMYK/Lab/Gray, global/spot/process flags, groups, explicit unsupported-block/model
  warnings, malformed/truncated/nonfinite errors. Reader accepts process blocks without a
  type tag, matching the reference's compatibility fallback. The pc-doc adapter uses the
  exchange warning/limits/output boundary, renames collisions deterministically and warns
  when ASE drops gradients or opacity. Independent recorded fixture and byte-stability tests.
- **Recolor Artwork:** extract authoritative fills/strokes/appearance items/gradient stops
  and referenced global colours, deduplicate exact model/components/alpha/reference identity,
  traverse groups and selected symbol masters, aggregate unsupported paint warnings. Seeded,
  weighted farthest-first k-means in the existing Oklab helper keeps global tint units together
  during reduction. Row assignment/drag, exact/preserve-tints/preserve-lightness methods,
  seeded Fisher-Yates order and saturation/brightness randomizers, optional saved swatch group.
  Preserve Lightness retains **CIE L* D50, 0..100** by reducing target chroma when sRGB clipping
  would alter L; independent luminance oracle asserts delta<=1. Source alpha is retained.
  Ordinary selection recolour detaches global references, preserving unselected uses. Selected
  symbol instances intentionally edit their shared master. Apply re-extracts/validates the
  captured selection and publishes one undo step.
- **UI:** existing Application tab preserved via a separate Document palette hook. Swatch
  double-click editor, real global/spot controls, tint input, gradient stop edits, groups,
  add-used, merge/delete/assign, ASE events and Recolor entry. Dialog supplies real typed
  drag/drop, hex assignment (invalid/partial hex disables Apply and clears preview), reduction,
  methods/randomizers, detached Preview, Cancel/Escape/staleness handling and save-group.
  Widgets return captured commands to the host; 00 must dispatch through Session::execute.
  Localization fragment contains all package UI literals. No desktop windows were opened.
- **Export fallback:** plain resolved paint/legacy mirrors are prepared before publication.
  Exchange warnings for lost global links and process fallback for spot colours are supplied
  to 14/21/23. PDF Separation is not falsely advertised: 21 may opt in only after actually
  emitting Separation spaces; otherwise the spot warning is mandatory.

## Acceptance evidence and pending gates

CPU tests use five linked objects: solid fill with a 50% tint, stroke, gradient stop,
appearance fill, and a stroke inside a symbol master with a real instance. Independent
interior RGB pixel expectations, resolved colour values, detached preview, cache/pixel equality
on delete, stale/late-locked rejection and one history step/undo/redo are asserted. Twelve
colours reduce to three expected primary-hue families deterministically. UI tests really
click, double-click, type and drag; they are widget/controller tests with a test provider,
not a claim of already wired app acceptance.

Four deferred tests are supplied and compiled:

1. Engine RTX 5090 Vulkan/discrete/Rgba32Float before/preview/release/undo parity,
   premultiplied max diff<=1/255; no software adapter/skip fallback.
2. Native old-v1 fixture, ZIP roundtrip and remapped root/master colour links with **real** 00
   Access, intentionally failing until the additive native fields/mirrors are integrated.
3. Offscreen **full PhotocraftApp** swatch double-click/live preview/cancel/release and one
   journal entry; strict real GPU setup, actual canvas readback and preview/release parity.
4. Release-only **full PhotocraftApp**, 2000 objects, actual DragValue pointer frames with
   UI+GPU+readback median/p95/p99/release timing and hardware identity.

The final speed rule prohibits executing these gates here. No GPU/performance result,
hardware verification or interactive timing is claimed. No proprietary/external ASE corpus
was supplied; the original fixture is a parser/writer gate, not an Adobe interoperability gate.

Remaining integration: Document storage and per-use reference maintenance through delete,
copy/paste, stop/item reordering and native ID remap; native manifest/convert mirrors;
once-only Access installation and command aggregation; Vector Document Palette tab/menu,
file dialogs and transient canvas preview keys/routing; translation catalog/PORTS attribution
regeneration; export warning collectors. Exact fragments and call signatures are in 39.md.
The native/full-app tests intentionally expose these missing dependencies rather than
passing against the test provider. Current symbol rendering rejects advanced appearance,
corners/on-path/live/clip masters; 15/08's shared projection remains required for those
combinations. Ordinary master fills/strokes/stops work; root corner cache geometry uses 06.
Brush/profile strokes require the injected StrokeProjection; unsupported shape clip/live/text
projection fails atomically. These limitations are named integration dependencies, not silently
rendered approximations.

## Validation

All builds use Rust/cargo 1.98.1, `--offline`, `CARGO_BUILD_JOBS=3`, and the inherited
`RUSTC_WRAPPER=/home/zdavidson/.local/share/local-image-dev/bin/li-rustc-gate`.
Only one cargo process runs at a time. No whole-crate/workspace test suite, GPU run, release
build, benchmark or desktop app launch was performed.

The single workspace/all-targets check attempt found new-code tuple/type-inference errors;
those were corrected. It was not repeated under the owner's “once” speed rule. Thus the
workspace check is **not reported as a passed gate**; final touched-crate all-targets clippy
and focused test compilation validate the corrected modules. Log: `target/colour39-check.log`.

Initial strict clippy found new-code UI style/test borrow issues; corrected and the focused
engine/UI follow-up passed. The final strict run passed all four touched crates/all targets
with `-D warnings` (19.15 s), after the input-validation fixes. Logs: `target/colour39-clippy.log`,
`target/colour39-clippy-fixes.log`, `target/colour39-clippy-final.log`.

The one new-test sweep used:
`cargo +1.98.1 test --offline -p photocraft-color -p photocraft-doc -p photocraft-engine -p photocraft-ui-egui --lib colour39_`.
It executed 17 new tests: color 3 passed, doc 2 passed, engine 8 passed/2 ignored, UI 3
passed/1 failed/2 ignored. Existing tests were filtered (22/50/1123/1094 respectively).
The UI failure was a double-click simulated with kittest's default 250 ms frame step,
exceeding the double-click interval. The harness now uses 60 Hz, also in the deferred full-app
gesture tests. Log: `target/colour39-tests.log`. Only that failed test and the strengthened
invalid-hex typing test are rerun; no passed sweep or unrelated suite is repeated. The first
targeted retry caught a missing NodeT import in the new disabled-Apply assertion, corrected
before test execution (`target/colour39-ui-doubleclick.log`). The strengthened typing test
then passed; the swatch test still failed on reopening (`target/colour39-ui-final.log`). Two
further one-test diagnostic runs (`target/colour39-ui-reopen.log`,
`target/colour39-ui-gesture-debug.log`) identified egui's 600 ms triple-click history: the
prior Cancel click made the second reopening click count as three. The helper now expires
earlier click sequences, following the existing context_bar_tests gesture pattern. Temporary
diagnostics are removed. Final isolated swatch retry **passed** (1 passed, 0 failed,
1099 filtered), `target/colour39-ui-verified.log`. All **17 distinct new nonignored tests**
now have passing results; the **4 deferred tests** remain ignored. Follow-up strict UI
all-targets lint **passed** with `-D warnings` (30.62 s), verifying the final test-helper
changes (`target/colour39-clippy-ui-final.log`).

Existing build-script warning: no embedded craft-fonts for Chinese/Japanese. No new fonts
or external corpus were introduced, and those rendering languages are not a passed gate.

## Sources and attribution

Read the architecture, persona spec including Owner decisions, quality plan, task index and
gap audit; reused existing vector_api/CompiledAppearance/live-corner/symbol cache APIs.
Verified local VectorCraft source HEAD = `d522c1d7be4035bd4f4a84cd6ebfca44f5155092`.
Adapted these pinned MIT OR Apache-2.0 sources, retaining copyright/license notices:
[swatch data](https://github.com/storytold/vectorcraft/blob/d522c1d7be4035bd4f4a84cd6ebfca44f5155092/crates/color/src/swatch.rs),
[doc swatch walks](https://github.com/storytold/vectorcraft/blob/d522c1d7be4035bd4f4a84cd6ebfca44f5155092/crates/doc/src/swatches.rs),
[swatch commands](https://github.com/storytold/vectorcraft/blob/d522c1d7be4035bd4f4a84cd6ebfca44f5155092/crates/engine/src/cmd/swatch.rs),
[recolour algorithms](https://github.com/storytold/vectorcraft/blob/d522c1d7be4035bd4f4a84cd6ebfca44f5155092/crates/color/src/recolor.rs),
[recolour commands](https://github.com/storytold/vectorcraft/blob/d522c1d7be4035bd4f4a84cd6ebfca44f5155092/crates/engine/src/cmd/recolor.rs),
[ASE reference](https://github.com/storytold/vectorcraft/blob/d522c1d7be4035bd4f4a84cd6ebfca44f5155092/crates/color/src/palette_io.rs).

Behavior references checked online:
[Affinity global colours](https://affinity.help/designer2/en-US.lproj/pages/Clr/globalClr.html),
[Illustrator swatches](https://helpx.adobe.com/illustrator/desktop/manage-colors/use-swatches/about-swatches.html),
[Illustrator Recolor Artwork](https://helpx.adobe.com/illustrator/desktop/manage-colors/modify-colors/recolor-artwork.html).
Oklab conversion is reused from pc-vecfx and verified against
[Ottosson's published matrices](https://bottosson.github.io/posts/oklab/).
The explicit Document Access bridge, stable IDs, strict reader failures, CIE L* gamut reduction
and egui widgets are Local Image adaptations to the existing contracts, rather than a
transplanted VectorCraft tree. PORTS rows and attribution regeneration are handed to 00 in
39.md; existing VectorCraft notice/license files remain intact. Fixture license/hash/tool
versions and generation parameters: `crates/pc-color/src/ase/SOURCES.md`. No brand assets.
