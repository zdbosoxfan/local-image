# Package 04 — Pen

Status: owned implementation supplied; focused Pen gates pass. Full UI crate failures
are recorded below. **Full-app acceptance
is pending the package 00/01 hooks and is not claimed complete.** The task expressly
reserves legacy `vector_ui.rs`, canvas/router, shared UI state/registries, preferences,
catalogues and aggregate GPU registration to those packages. Their hooks are absent
on this base; exact fragments and a full-app test source are supplied in
[integration/04.md](vector-tasks/integration/04.md).

## Implementation

- Pen click/drag corners and symmetric smooth handles; Alt sampled during drag freezes
  the incoming handle and breaks smoothness, including Alt held from the start.
- Smart auto handles on clicked nodes, preserving explicit drags and existing endpoints;
  Polygon sharp straight edges; Line supports click-drag and two-click placement,
  self-terminating on release with two sharp endpoints. Shift constrains to 45 degrees.
- Closing near the first node preserves its handles unless dragged; a closing drag
  shapes the arriving curve and Alt preserves its outgoing handle. No duplicate closing
  knot is inserted. Rubber band and node/handle overlays remain transient.
- Pen/Smart/Polygon continue either end of an open subpath without reversing or changing its existing
  knots/handles. Add a subpath to exactly one selected curve. Existing paint is preserved.
  Corner indices survive unchanged knots and prepending, including equal duplicate knots;
  changed parametric/live/raw PSD geometry is invalidated. Undo restores full metadata.
- Enter finishes; Escape, focus loss, pointer loss, tool/persona/modal deactivation,
  document/revision changes and context mode changes cancel. Captured pointer releases
  outside the canvas are honored. The adapter supplies both isolated-host event input
  and direct hooks for the existing app capture route.
- `path.penCommit` is compiled in the existing engine registration slot. Begin captures
  document/revision/target/insertion IDs and paint. Previews are detached Documents with
  the same bounds-limited pc-vector shape caches as commits. Commit validates freshness,
  the whole target set, inherited locks, node/coordinate/paint budgets and supported
  rendering before **one** Session edit, history step and journal entry. Work-path and
  vector-mask targets use the same guarded command. Intermediate node releases never
  publish or journal. Appearance/pattern rendering dependencies return explicit errors.
- Native Path/Knot/paint schema is retained. Options have serde defaults and old `{}`
  settings load; no persisted document field, dependency, manifest or lockfile changes.
  UI strings and per-file attribution rows are supplied in the owned integration lane.

## Validation

Commands use Rust 1.98.1, offline mode, `CARGO_BUILD_JOBS=3`, one active Cargo build.
Focused changed-code gates passed:

- `cargo +1.98.1 check --offline -p photocraft-engine`: passed.
- `cargo +1.98.1 test --offline -p photocraft-engine --lib vector_packages::pen:: -- --nocapture`:
  **13 passed, 0 failed, 0 ignored**, including Line drag and duplicate-knot metadata
  regressions, 256 deterministic replay seeds and the strict GPU gate.
- `cargo +1.98.1 test --offline -p photocraft-ui-egui --test pen pen_ui -- --nocapture`:
  **8 passed, 0 failed, 0 ignored**, with real egui pointer/key/widget events,
  held Alt/Shift, capture outside the canvas, cancellation, selected-curve addition
  (including raster rejection), old options and the strict isolated GPU gate.

- `CARGO_TARGET_DIR=target/clippy cargo +1.98.1 clippy --offline -p photocraft-engine -p photocraft-ui-egui --all-targets -- -D warnings`:
  **passed** (including the isolated test's imported production UI module).
- `cargo +1.98.1 fmt -p photocraft-engine -p photocraft-ui-egui -- --check`,
  standalone `rustfmt --check` of the unregistered app gate, and `git diff --check`:
  **passed**.

Full suites were run once, only for the changed crates:
`cargo +1.98.1 test --offline -p photocraft-engine -p photocraft-ui-egui`.
After Cargo stopped at the UI library failures, the previously unrun UI binaries were
completed once with `--test gpu --test it --test pen --no-fail-fast`.

| Crate / binary | Passed | Failed | Ignored |
|---|---:|---:|---:|
| Engine library | 864 | 0 | 11 |
| Engine coverage | 582 | 0 | 0 |
| Engine integration | 47 | 0 | 5 |
| UI library | 989 | 2 | 5 |
| UI GPU | 59 | 0 | 4 |
| UI integration | 14 | 0 | 0 |
| UI Pen | 8 | 0 | 0 |
| Total (excluding focused reruns) | 2563 | 2 | 25 |

The full UI crate gate is **not green**:

- `i18n::tests::every_tl_literal_is_translated`: the scanner includes the new
  unregistered UI source and reports Japanese missing `Add to selected curve`,
  `Polygon`, `Rubber band`, `Smart`. The owned `i18n/04.tsv` supplies the exact
  English keys; catalogue merging/translation belongs to 00. No shared catalogue
  or audit exception was changed to hide this dependency.
- Existing `generate_ui::draft_refine::tests::draft_refine_mock_comfy_uses_source_and_library_results_can_be_used_or_opened`
  failed at its expected refinement-entry assertion. A single focused isolated
  replay **passed (1/1)**; the full-run failure did not reproduce. Its implementation
  and fixtures were not changed. The failure remains recorded rather than declaring
  the full suite passed.

Final review found and corrected corner remapping when prepending and then dragging
the closing endpoint. The existing continuation test now checks that the untouched
original node keeps its corner index and Undo restores all metadata. The final
focused engine replay **passed 13/13**, including the strict GPU gate again, and the
lint refresh **passed** for both crates/all targets with `-D warnings`. The previously
unrun doctest phase **passed** for both crates (0 tests each); the full suites were not
repeated under the task's once-only test budget. The table records the full run before this final corner fix;
the strengthened focused test verifies that last change.

Ignored gates are separate from passes: engine has 11 latency/benchmark tests,
two panic-hunt entry/full-fuzz tests, one user-supplied Camera Raw PSD test, one manual
Photoshop output and one smart-object benchmark. UI has 7 benchmarks, one contact-sheet
review output and one explicit hardware icon test. **No Pen tests are ignored; no Pen
corpus is missing.** The external Camera Raw PSD/manual review/benchmark gates were not
established by this run. The inherited build-script warning about missing optional
craft-fonts CJK fonts was emitted; Pen fixtures do not require those fonts.

Logs are in `target/pen-test-engine-focused*.log`, `target/pen-test-ui-focused.log`,
`target/pen-clippy.log`, `target/pen-tests-full.log`, `target/pen-tests-ui-remaining.log`,
`target/pen-clippy-final.log`, `target/pen-tests-doc.log`, and
`target/pen-test-ui-existing-failure.log`.

Strict engine GPU gate **passed** on NVIDIA GeForce RTX 5090, driver 615.71.09,
Vulkan, DiscreteGpu, Rgba32Float accumulation. Complete premultiplied open-curved
and closed-path CPU/GPU max difference was **0** (gate <=1/255); preview and committed
GPU cache pixels were identical, with Undo/Redo checked. This is the engine gate,
not a claim of full-app pointer acceptance.

Added GPU tests:

- Engine `pen/gpu.rs`: mandatory RTX/Vulkan/float checks, full curved/open/closed
  preview parity, atomic commit, Undo/Redo. No adapter skip is accepted.
- UI `tests/pen/gpu.rs`: mandatory real-adapter egui rendering, real pointer preview,
  cancellation and release in the isolated host.
- UI `tests/pen/app_acceptance.rs`: actual offscreen PhotocraftApp pointer events,
  real canvas GPU callbacks, curved/closed preview, release, Undo/cancel and Line mode
  at 1x/HiDPI, with native cache equality and actual canvas pixel gate. **Unregistered
  and unrun until 00 installs the supplied state/router/preview hooks.**

Fixture sources/parameters/licenses are in engine `pen/SOURCES.md` and UI
`tests/pen/SOURCES.md`. Pen fixtures are original synthetic Rust data; no proprietary
or external corpus is needed. Unrelated ignored/missing corpus gates are counted
separately above. No performance claim is made; release real-app
pointer-frame measurement and manual logo drawing await the named app integration.

## Sources and continuity

Continued foundation/V0 commit `2e8c671f`; the initial working-tree changes consisted
only of the untracked task brief. Existing legacy Pen, vector renderer, Direct Selection, shared transactions,
GPU and tests were inspected and retained. The compiled pen slot was empty; no previous
pen package implementation was removed or replaced.

Adapted [VectorCraft tools/pen.rs at d522c1d7](https://github.com/storytold/vectorcraft/blob/d522c1d7be4035bd4f4a84cd6ebfca44f5155092/crates/tools/src/pen.rs),
verified against the owner's pinned read-only checkout. Copyright/license notices
remain in each adapted file; existing `licenses/vectorcraft-*` are unchanged. One
PORTS row per adapted file is supplied for 00's attribution regeneration; no brand
assets were used. Existing PhotoCraft capture/render and vector_api patterns were used.

Behavior follows the official [Affinity Pen modes](https://affinity.help/designer2/en-US.lproj/pages/Tools/tools_pen.html),
[Line click-drag/curve drawing](https://affinity.help/designer2/en-US.lproj/pages/CurvesShapes/draw_linesAndShapes.html)
and [egui 0.36.2 Event API](https://docs.rs/egui/0.36.2/egui/enum.Event.html).
The public sources do not specify Smart's numerical fitter; the local standard uniform
Catmull–Rom-to-cubic tangent policy is documented/tested, with no claim of numerical
Affinity equivalence. Package 07's implementation lane was read read-only and its actual
Cache/Angle query signatures are recorded for 00, without adding a guessed snap stub.

No commit was made. Completion of the owned code does not waive integrated acceptance.
