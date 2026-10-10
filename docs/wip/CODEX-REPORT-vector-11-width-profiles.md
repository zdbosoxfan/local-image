# Width profiles — package 11

Date: 2026-10-10. Base: `fa015212`, continuing the existing integration branch foundations.
No commits. Initial working tree contained only the untracked task brief. The owned engine
slot was empty; the UI width module and pc-vecfx crate were absent. Existing WidthProfile /
StrokeItem persistence, V1 geometry, package 08 renderer and vector_api were reused.

Status: package-owned implementation supplied; **integrated app acceptance remains pending
00/01 routing and 08's combined provider**. The numeric UI test's deadlock was corrected after
its one permitted execution; runtime verification of that correction remains for integration.
The latest task SPEED RULES explicitly prohibit
GPU runs, release builds, benchmarks and whole-crate suites in this job. These are not
reported as passing. Exact registration, provider, prefs, router, translation, attribution
and full-app test fragments are in `vector-tasks/integration/11.md`.

## Implemented

- New internal `photocraft-vecfx` pure projection crate; existing kurbo 0.13.1 / V1 dependencies.
  Width profiles use the existing absolute left/right document-pixel distances, normalized
  subpath arc length, linear interpolation and endpoint extension. Empty retains native width.
  Seven pinned VectorCraft presets and named saved profiles. Ordered/finite/bounded inputs;
  4096-point limit, coordinate/node/outline/dash work limits. No shared data type changed.
- Variable-width normal offsets, corner-aware miter/round/bevel joins, inner overshoot detours,
  adaptive round/square/butt caps, closed rings, ordinary dash spans at original profile
  positions, inside/outside clipping. Overlap cleanup uses the **existing V1 kernel**; renderer
  consumes only ordinary filled Paths. No renderer code or replacement boolean engine.
  Closed seams preserve incoming t=1 and outgoing t=0 distances when endpoint widths differ.
- Constant symmetric profiles use the exact existing native stroke flattening/polygons,
  then topology cleanup. The coverage oracle passes all 3 caps × 3 joins on line/V/rectangle.
  The original stroke weight remains the basis of PSD-relative dash lengths and offsets.
- Exact kurbo arc-length / inverse-length Metric for canvas point placement and nearest hits;
  left is up for a +x stroke in y-down coordinates. Unequal segment lengths and a nonlinear
  cubic parameterization have independent position oracles.
- Compiled engine commands `shape.width.info`, `.preview`, `.set`. Begin captures document
  ID/revision and target IDs. Preview is detached and non-journaled. Commit revalidates the
  entire captured target set, prepares bounded package 08 caches (512 MiB padded-tile budget),
  and publishes one vector_api Session edit / journal entry. Editable source paths are retained.
- Compiled Width UI module: click/add, centre drag/move, side drag, Alt asymmetric editing,
  modifier-only updates, pointer capture/release outside the canvas, selected point deletion,
  numeric t/left/right controls, pressure-graph add/drag, seven presets, saved profile
  add/apply/remove. Clicking to add preserves inherited widths. Escape/focus loss/tool changes,
  selection/document/revision changes cancel detached state; numeric buffers reset on cancel.
  Options have serde defaults; saved values are validated before use. New strings use i18n.
- Minimal registration under the task's explicit REGISTRATION override: new crate manifest/lib,
  workspace dependency, engine internal dependency, public UI width module. Lock adds only
  the internal crate entry and engine edge; **zero new registry packages**. No tool enum,
  app router/menu, catalog, aggregate GPU registration, shared appearance or renderer edit.

## Acceptance evidence

| Gate | Status / independent evidence |
|---|---|
| 0→10→0 lens area within 0.5% | Passed: 100px line, 10px each side, independent triangle area 1000px². |
| Constant profile native coverage | Passed: 27 cap/join/path combinations, max alpha diff ≤1/255 against native rasterizer. |
| Arc-length t and correct sides | Passed: unequal 10+90px segments, cubic x(u)=100u³, asymmetric 8/2px side containment, reversed direction. |
| Tight bends/corners without crossings | Passed: wave profiles, tight zigzag / closed rectangle, all joins; independent proper segment-crossing checker on normalized rings. |
| Closed seam with unequal endpoint widths | Passed: independent incoming/outgoing containment at .99/.01 of a 400px perimeter, all three joins. |
| Original-distance dashes / alignment | Passed: taper dash area 600px², gap containment, closed inside-stroke containment. |
| Atomic preview/release/native roundtrip/undo | Engine test result recorded below. |
| Actual kittest add/move/delete/graph/saved/undo/cancellation | Five UI tests passed. |
| Numeric typing/Escape/stale buffer | Test failed on a nested egui context lock; corrected afterward, not rerun under the one-execution rule. |
| `width_profile_strokes` GPU | One compiled strict ignored engine GPU test supplied; execution prohibited by speed override. |
| Full-app GPU preview/release parity | One real-app source supplied for 00 registration after Width router integration; not compiled or executed here. |
| Interactive speed | Not measured; no release/performance claims. |

## Verification commands and counts

All cargo builds were sequential with `CARGO_BUILD_JOBS=3`, `--offline`, toolchain 1.98.1,
and the inherited `RUSTC_WRAPPER=/home/zdavidson/.local/share/local-image-dev/bin/li-rustc-gate`.
No desktop window was opened and no running owner app/service was modified.

- `cargo +1.98.1 fmt -p photocraft-vecfx -p photocraft-engine -p photocraft-ui-egui`:
  passed; orphan full-app source formatted with `rustfmt +1.98.1 --edition 2024`.
- `CARGO_TARGET_DIR=target/clippy cargo +1.98.1 check --offline --workspace --all-targets`:
  passed, **one workspace invocation** (log `target/width-workspace-check.log`).
- `CARGO_TARGET_DIR=target/clippy cargo +1.98.1 clippy --offline -p photocraft-vecfx
  -p photocraft-engine -p photocraft-ui-egui --all-targets -- -D warnings`:
  first pass found egui Rect's missing Default, fixed; subsequent pass passed. Final pass
  after click/profile validation, closed-seam and numeric context-lock adjustments **passed**.
  Log `target/width-final-clippy.log`; all touched crates and all targets included.
- `cargo +1.98.1 test --offline -p photocraft-vecfx --lib width_package`:
  **6 passed, 0 failed, 0 ignored, 0 filtered**, executed once. Log `target/width-vecfx-tests.log`.
  Final review then added the closed-seam regression and ran only that new test with
  `cargo +1.98.1 test --offline -p photocraft-vecfx --lib width_package_closed_profile_seam`:
  **1 passed, 0 failed, 0 ignored, 6 filtered**, executed once. Log `target/width-seam-test.log`.
  No previously executed geometry test was rerun.
- `cargo +1.98.1 test --offline -p photocraft-engine --lib width_package`:
  **4 passed, 0 failed, 0 ignored, 1053 filtered**, executed once. Log `target/width-engine-tests.log`.
- `cargo +1.98.1 test --offline -p photocraft-ui-egui --lib width_package`:
  **5 passed, 1 failed, 0 ignored, 1040 filtered**, executed once. Log `target/width-ui-tests.log`.
  Numeric test failed with epaint's 10-second lock panic. The locked egui 0.36.2
  `Context::input` docs (`egui/src/context.rs`) explicitly forbid re-entering the context
  from its callback. `panel` now reads keyboard focus before entering `ui.input`, removing
  the nested read. The test was not rerun, as instructed; its numeric/Enter/Escape/stale-buffer
  assertions remain runtime-unverified. The five other interaction tests completed successfully.
- Final format and `git diff --check`: passed. Overall executed new-test counts:
  **16 passed, 1 failed, 0 ignored** (each test executed once); the failed numeric test was
  corrected afterward and remains unrerun. Final strict clippy includes that correction.

Skipped by **latest speed override**, not passed: whole-crate/workspace test suites, real GPU
execution, release builds, pointer-frame benchmarks. Ignored gates supplied: 1 compiled engine
GPU test; 1 full-app source awaiting registration (separate from ignored-test counts in the
executed filters). Missing corpus: no proprietary files or external Inkscape goldens supplied
for width; none counted as a successful gate. Existing font build-script warning notes absent
craft-fonts for CJK; no width test uses text/fonts as an oracle.

## Remaining integration / limits

00/01 own product tool/router/prefs/catalog wiring and the serialized GPU binary. 08/09/format
projection consumers must compose the Width provider and re-use it on load, geometry edits,
scaled views and export. The actual app Width path is therefore not advertised as integrated
or accepted in this branch. Once routed, the supplied strict app gate checks held preview,
release, cancellation and one undo through actual app GPU readback with max premultiplied
byte diff ≤1/255, RTX 5090/Vulkan/discrete/Rgba32Float/f16 identity, and no software skip.

Width+brush/arrow/fitted-dash combinations need the named 08/17 combined projection provider;
live-corner metadata needs 06/08's geometry-before-appearance composition. The isolated
provider explicitly rejects these cases, preserving decorations/metadata instead of silently
rendering a partial result. Step/discontinuous profiles (duplicate t) are intentionally outside
the **shared strictly ordered WidthProfile contract**. Moving onto another point replaces it.
Saved profiles store absolute document-pixel distances; they do not rescale by a later base
stroke-weight change. Each subpath runs its own normalized profile.

## Sources / attribution

Pinned local checkout verified at `d522c1d7be4035bd4f4a84cd6ebfca44f5155092`:
VectorCraft `crates/doc/src/appearance.rs`, `crates/effects/src/stroke/width.rs`,
`crates/tools/src/distort/width.rs`, `crates/engine/src/tests_widthtool.rs`.
Retained source copyright / MIT OR Apache-2.0 notices; existing VectorCraft licenses were
already present. No brand assets copied. PORTS rows and the attribution regeneration handoff
are in `vector-tasks/integration/11.md`; core PORTS/catalog regeneration belongs to 00.

Official [kurbo 0.13.1 arc-length docs](https://docs.rs/kurbo/0.13.1/kurbo/trait.ParamCurveArclen.html)
and [Inkscape PowerStroke behavior](https://wiki.inkscape.org/wiki/Release_notes/0.91#PowerStroke)
were read alongside the required architecture/persona Owner decisions/quality plan.
Fixture licenses, tool versions and parameters are recorded in
`../../crates/pc-vecfx/src/width/SOURCES.md`. All test inputs are original GPL-3.0-or-later
synthetic geometry; no external corpus/golden pass is invented. Local cache/transaction
budgets, native constant-profile routing and UI host glue are original adaptations to the
established Local Image APIs, not a transplanted object tree or renderer.
