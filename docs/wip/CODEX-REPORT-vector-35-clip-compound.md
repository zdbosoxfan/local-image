# Vector package 35 — clipping masks and compound paths

Date: 2026-10-10. Branch: `wip/vec-35-clip-compound`. Changes are uncommitted.

Package-owned engine/UI code, tests, translations and integration fragments are supplied.
Production router/menu/prefs integration remains with 00/01/46 under the task's ownership
contract. This report does **not** mark integrated UI/GPU acceptance complete. The final
owner speed override forbids GPU acceptance runs, full suites, release builds and benchmarks.

## Implementation

- Make Clipping Mask consumes the top selected sibling shape/text into an ordinary
  `LayerContent::Group` with its authoritative `vector_mask`. Remaining children keep
  geometry, IDs, appearance and paint order. The clipping path loses its paint.
- Release restores ordinary contents and a fresh, unpainted shape from the current mask.
  Later edits through existing `layer.vectorMask.edit` are reflected in the released path.
  Text clips use pc-text outlines and release as outlines. Additional group appearance,
  artboard/live/pixel-mask semantics are rejected before release to avoid silently losing them.
- Edit Mask selects the group and returns its ID for the existing Direct Selection
  `PathRef::Layer` target. Edit Contents selects children. These commands do not journal,
  change document bytes or increment document revision.
- Make Compound Path uses the bottom object's complete appearance/compositing envelope.
  Simple contours preserve knots/handles; 06 projects corners, and V1 normalizes ordered
  PSD boolean operations/even-odd source geometry. The native format requires a first
  `Combine` contour and subsequent `Join` contours for one shared fill rule: separate
  Combine components union independently and cannot produce the specified even-odd ring.
- Release Compound Path emits one shape per subpath with the same appearance. Its template
  excludes the original compound geometry, avoiding a complete path clone for every contour.
  Fill Rule changes shape paths or native group vector masks atomically.
- Move Inside supports ordinary groups or shape/text clipping parents, keeps the selected
  parent's paint as a bottom child, preserves IDs/order and rejects ancestor cycles.
  Move Outside hoists selected siblings above their immediate parent in paint order.
- Draw Normal/Behind/Inside are serde-defaulted preferences; Normal is the old-settings
  default. Inside creates a native clip group around the selected object, retains its paint,
  appends new contents and selects the group for subsequent insertions. An explicit insertion
  target from package 37 is captured at pointer-down.
- Package UI supplies the draw-mode buttons, Shift+D cycle, dashed-corner indicator,
  Object/Layers menu fragments and actual Layers-row drag hooks. Vector Ctrl/Cmd+7/8 and
  release chords use exact key-event modifiers and the app's established keyboard-focus
  policy. Pixel dispatch remains coordinator-owned and unchanged.

All gestures carry document ID/revision/target IDs. Preview uses a detached document and
creates no history/journal entry. Commit checks freshness and validates the entire set,
prepares aggregate bounds-limited caches, then publishes through one `Session::edit` and
one journaled engine command. Queries are non-journaled. Selection uses the existing store.
Duplicate/ancestor targets, effective locks, nonfinite/out-of-range coordinates, invalid
appearance, stale gestures, ID collisions, cycles and excessive nesting reject atomically.
Limits are 256 selected/released objects, 65,536 path nodes, coordinates within 1,000,000
pixels, 100 nested groups, and 512 MiB aggregate prepared shape caches.

Existing Photoshop `layer.createClippingMask`, layer mask/vector-mask commands, document
content variants, native schema, manifests and lockfile are unchanged. No dependencies,
unsafe Rust, non-Rust implementation, desktop windows or commits were added. Existing branch
work was preserved and reused: V1, 06 corners, 08 appearance, 11/17 stroke projection, 12
text layout/outlines, and the already integrated SVG/PDF/PSD/native-format public hooks.

Minimal shared registration adds the engine module/spec list and UI `pub mod` only.
Package 12's private curve-to-path adapter is compiled from its existing single source;
the narrowly justified Clippy duplicate-module expectation avoids editing another package
or copying a second shaper. The handoff asks 00 to expose that adapter and remove the bridge.

## Validation

All build/lint/test commands used `--offline`, Rust 1.98.1 and `CARGO_BUILD_JOBS=3`, retained
`RUSTC_WRAPPER=/home/zdavidson/.local/share/local-image-dev/bin/li-rustc-gate`, and ran
one cargo build at a time. Format ran for both changed crates; the supplied unregistered
app-GPU source was formatted separately. No passing test set was replayed.

| Gate | Result |
|---|---|
| Engine `cargo +1.98.1 test --offline -p photocraft-engine --lib clip35_` | 8 passed, 1 failed, 1 ignored, 1,099 filtered out; 0.91s test execution |
| UI `cargo +1.98.1 test --offline -p photocraft-ui-egui --lib clip35_` | 5 passed, 0 failed/ignored, 1,075 filtered out; 0.03s test execution |
| Strict all-targets Clippy, both changed crates | passed, exit 0; final current-code run 12.59s |
| `cargo +1.98.1 check --offline --workspace --all-targets` | passed, exit 0; final current-code run 53.66s |

The engine failure was the ring-center assertion's exact float equality: expected zero,
observed alpha `1.1175871e-8`. The V1 ring/disc area checks had passed. The assertion is
corrected to require alpha ≤1e-6, far stricter than the task's 1/255 pixel tolerance.
It was **not rerun**, honoring the owner's once-only test budget; assertions after that
point, including this case's compound release/undo checks, remain unexecuted in that run.
This corrected case is not counted as passing. The initial log is retained at
`target/clip35/engine-tests.log`.

Totals: 14 new active tests executed once, 13 passed and one initial assertion failure
corrected without a rerun; one compiled hardware test ignored; one full-app GPU source
unregistered/uncompiled and counted as a missing gate. UI log:
`target/clip35/ui-tests.log`. Final Clippy log: `target/clip35/clippy-final.log`.
Clippy command: `CARGO_TARGET_DIR=target/clippy cargo +1.98.1 clippy --offline
-p photocraft-engine -p photocraft-ui-egui --all-targets -- -D warnings`.
Workspace log: `target/clip35/workspace-check-final.log`. The initial check caught an egui
API mismatch, repaired before these final gates; initial Clippy caught the private adapter's
duplicate source load, addressed by the specific bridge expectation described above.
`git diff --check` and package-file whitespace inspection passed. Only the two minimal
shared registration files and the owned package/doc files changed; no manifest/lock edits.

The new tests use the unique `clip35_` filter. Engine tests cover an independent three-shape
composite multiplied by separately rasterized star coverage (premultiplied max difference
≤1/255), detached previews, release geometry, exact undo, edited native masks, text counters,
circle analytic/V1 area and fill-rule pixels, bottom appearance, release, draw modes,
repeated Draw Inside, nesting/order/cycles/locks/staleness/depth, and atomic errors.

Format fixtures use public SVG/PDF/PSD hooks with `vector_design::exchange::ExportScope`.
They assert native SVG clipPath and PDF clipping operators without image fallback, PSD
group vector-mask geometry within native 8.24 coordinate quantization, pixel parity after
PSD import, and native `.pcraft` mask/pixel roundtrip. They are synthetic fixtures, not a
claim that proprietary application corpus gates passed.

Five egui_kittest tests send real key/click/pointer events through the package host: clipping
and compound keyboard chords, fill-rule menu, persona scope, Shift+D/widget/default settings,
Layers drag-in with detached preview/release/cancel and Move Outside, rectangle drawing
inside an ellipse with stale cancellation, and Edit Mask/Contents menu targeting. These
exercise package hooks; they do not substitute for the production app integration gate.

## GPU tests and skipped gates

- Compiled ignored engine test:
  `clip35_gpu_raster_child_vector_parent_preview_commit_release_and_undo`. It asserts
  RTX 5090, Vulkan, discrete GPU and Rgba32Float; compares CPU straight colors multiplied
  by alpha to the GPU's already-premultiplied float readback with max difference ≤1/255.
  Mixed raster/vector clipping, detached preview, commit, release and undo are covered.
  It is deliberately ignored and was not executed under the speed override.
- Production app test source:
  `crates/pc-ui-egui/src/vector_packages/clip_compound/app_gpu.rs`. It drives the real
  Rectangle tool with pointer frames, requires the app's live detached preview route,
  checks the actual app GPU float document texture, clip release, undo and Escape. It
  requires RTX 5090/Vulkan/discrete, Rgba32Float and float canvas, with no CPU fallback.
  **Unregistered/uncompiled pending 00's real state/tool/display hooks**; it is supplied
  against the exact `PhotocraftApp.clip_compound_state` interface documented in the handoff.
  It is a missing integrated gate, not a passing/ignored test count.
- Whole-crate/workspace test suites, GPU runs, release builds and pointer-frame performance
  benchmarks were skipped as explicitly required by the final owner speed rules. No hardware
  timing or GPU acceptance result is claimed. Missing external Adobe/Affinity corpus and
  proprietary visual goldens are also not counted as passing gates.

## Integration lane

Exact APIs, command JSON, menu/shortcut placement, 37 insertion targeting, preference
default, Layers-row hooks, Direct Selection routing, display-preview cache requirements,
central GPU registration and attribution row are in
[`vector-tasks/integration/35.md`](vector-tasks/integration/35.md).
The 19 English catalog entries are in
[`vector-tasks/i18n/35.tsv`](vector-tasks/i18n/35.tsv).

00/01/46 must attach the fragments to production Object/Layer menus and Vector shortcuts,
add the transient app state/persisted settings, route tool paths through Begin/Preview/Commit,
display detached previews with a transient cache generation, route Edit Mask to existing
Direct Selection, attach the existing Layers rows and register the supplied full-app GPU
source after wiring. These shared files were deliberately left to their owners. Integrated
acceptance remains open until that wiring and the coordinator's deferred GPU gate pass.

## Sources and attribution

Followed the task's architecture/persona Owner decisions/quality plan, integration contract
and gap audit. Read pinned VectorCraft `cmd/object.rs`, `doc/clipnest.rs` and clipping/layerclip
tests from project pin `d522c1d7be4035bd4f4a84cd6ebfca44f5155092`; adapted object command
semantics, with object.rs last-change pin `7e82dbc05b972d8b32b68e7b31444a87ed8e6d48`.
Existing MIT/Apache licences and NOTICE remain. The handoff supplies one PORTS row for
the adapted engine file so 00 can update the aggregate attribution without competing edits.

Official references: [Adobe clipping masks](https://helpx.adobe.com/illustrator/desktop/manage-objects/edit-objects/about-clipping-masks.html),
[compound paths](https://helpx.adobe.com/illustrator/desktop/manage-objects/reshape-transform-objects/create-compound-paths.html),
[drawing modes](https://helpx.adobe.com/illustrator/desktop/draw-shapes-and-paths/learn-drawing-basics/drawing-modes-overview.html),
[Affinity Layer clipping](https://affinity.help/designer2/en-US.lproj/pages/Layers/layerClip.html)
and [Fill modes](https://affinity.help/designer2/en-US.lproj/pages/CurvesShapes/fillMode.html).
The existing native group-mask model and renderer/exporter component rules determine the
Local Image adapters; no alternative object tree or boolean approximation was invented.

Fixture generation parameters, font licence, tool versions, format settings, oracle
construction and missing corpus are recorded in
[`SOURCES.md`](../../crates/pc-engine/src/vector_packages/clip_compound/SOURCES.md).
UI/tests/fixtures are original GPL-3.0-or-later; no brand assets.
