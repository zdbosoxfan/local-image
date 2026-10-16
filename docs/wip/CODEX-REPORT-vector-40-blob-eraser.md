# Package 40 — Blob Brush and vector Eraser

Implementation is in the working tree, uncommitted. Production app wiring and integrated
GPU/release acceptance remain coordinator work under the ownership contract and final
SPEED RULES; this report does not claim those acceptance gates passed.

## Baseline and preservation

Started at `be9369e4` with only the untracked `CODEX-TASK.md`. There were no existing
package-40 files/report to recover. Retained the branch's integrated builders: V1 pathops,
05 Pencil sampling/fitting/stabilizers, 08 appearance rendering, 11 width projection,
17 brush pressure/projection, 06 live-corner evaluation, and Session transaction/history.
Read the task, architecture, persona spec/Owner decisions, quality plan, package index and
gap audit before implementing. Other package source files were read-only.

## Implementation by scope point

- **Blob Brush:** round/calligraphic swept tip with size/angle/roundness/diameter pressure;
  05/V1 fidelity fitting and optional rope stabilization; filled path uses stroke RGBA
  captured at press. Intersecting same-appearance fill-only shapes within one parent merge
  through fallible V1 union into one layer. Connected merges resolve independent of paint
  iteration order. Different colours, strokes, opacity/blend, masks, effects and clipping
  contexts do not merge. Keep Selected and Merge Only with Selection are implemented.
- **Vector Eraser:** selected shapes/group descendants, or all unlocked shapes when no
  layer is selected; closed area subtraction, connected output splitting with holes retained,
  live shape/corner conversion to curves, and Alt rectangle from first to current pointer.
  Open strokes are cut through V1 arrangement intersections/cut_out, retaining cubic
  subsegments; narrow crossings and completely contained curves are supported. Text and
  symbol instances are excluded with a UI explanation/tooltip.
- **Preview/commit:** pointer capture freezes document/revision/targets/colour/options.
  Preview is a detached document/region, non-journaled and cancellable. Escape, focus/pointer
  loss, mode/tool/persona deselection and stale revisions discard it. Release outside the
  canvas is captured. Commit revalidates the entire target set, prepares bounded caches,
  and uses one registered command/Session::edit. No-op UI strokes dispatch no edit/journal.
- **Bounds/robustness:** finite bounded inputs, 1024 samples/targets/outputs, aggregate
  131072 knots, 512 MiB total bounds-limited cache budget before cache allocation. Invalid
  geometry/paint/cache failures publish no document, revision, history or journal. Stationary
  pressure changes preserve earlier dabs alongside the last-value arc-length profile.
- **Persistence:** no document schema or file adapter change; defaulted Options/Tip/State
  accept older `{}` settings; transient preview/gesture/cache fields are skipped.
- **Integration:** compiled engine/UI/sweep modules and new command slot registered with
  minimal shared declarations. `integration/40.md` supplies exact app routes, tool IDs,
  Shift+B/Shift+E, icon requests, prefs and GPU registration fragments; `i18n/40.tsv` lists
  every context label/tooltip. No catalogues/router/prefs/manifests/lock or other packages
  were modified. No dependencies, C/-sys build code, unsafe code, or brand assets added.

## Validation

All Cargo check/lint/test invocations use Rust 1.98.1, `--offline`, `CARGO_BUILD_JOBS=3`, and the existing
`RUSTC_WRAPPER=/home/zdavidson/.local/share/local-image-dev/bin/li-rustc-gate`. One build at
a time. Formatting used `cargo +1.98.1 fmt -p photocraft-vecfx -p photocraft-engine -p
photocraft-ui-egui`. Unregistered app GPU source was also rustfmt-formatted.

| Gate | Result |
|---|---|
| `cargo check --offline --workspace --all-targets` | Passed; one initial attempt caught a test-only egui RawInput modifier API mismatch, corrected to ModifiersChanged, then successful check. Existing lc-engine warning about absent craft-fonts is unrelated. |
| `CARGO_TARGET_DIR=target/clippy cargo clippy --offline -p photocraft-vecfx -p photocraft-engine -p photocraft-ui-egui --all-targets -- -D warnings` | Passed. Final engine/UI-only strict lint recheck also passed after the endpoint/merge and numeric-input harness fixes. |
| `cargo test --offline -p photocraft-vecfx --lib sweep40` | 4 passed, 0 failed, 0 ignored, 7 filtered; includes 256 deterministic random cases. |
| `cargo test --offline -p photocraft-engine --lib blob40` | Initial: 2 passed, 5 failed, 1 ignored, 1133 filtered. After fixes, only the five failures rechecked: 5 passed, 0 failed, 0 ignored, 1136 filtered (`--skip blob40_compound --skip blob40_open_cubic --skip blob40_gpu`). All 7 new CPU engine tests now pass; the compiled GPU test remains ignored. |
| `cargo test --offline -p photocraft-ui-egui --lib blob40` | Initial: 3 passed, 1 failed, 0 ignored, 1099 filtered. After the numeric-input harness fix, only that test rechecked: 1 passed, 0 failed, 0 ignored, 1102 filtered. All 4 new CPU UI tests now pass. |
| Formatting / diff / dependencies | Final three-crate fmt --check and standalone app_gpu.rs rustfmt --check passed; git diff --check clean; no Cargo.toml/Cargo.lock changes. |

Logs are in `target/blob40-{check,check-fixed,clippy,clippy-recheck,sweep-tests,engine-tests,engine-recheck,ui-tests,ui-recheck}.log`.
In total, 15 unique new CPU tests passed. Only this package's new tests are executed;
whole-crate/workspace test suites are skipped
under the final SPEED RULES. No GPU acceptance, release build, benchmark, desktop window,
ComfyUI request or external corpus job was run.

Tests use independent analytic capsule/ellipse/rectangle areas, independently rasterized
source-minus-sweep coverage, exact line spans and original-cubic nearest-distance oracles,
document/pixel undo equality, current-stroke colour capture, selection/merge options,
staleness, whole-set locks, exclusions and old settings. Kittest performs real clicks,
text input, drags with synthetic pressure, Alt changes, Escape/focus/tool cancellation and
outside-canvas release. These are CPU tool-host tests, not a claim of production app routing.

The first engine run found five failures: the blob appearance gate compared the enabled
empty Effects container of Layer::new with the disabled Default container, and V1's kurbo
refit introduced sub-1e-7 endpoint roundoff that 05 correctly rejects. The gate now accepts
empty effects regardless of enabled state, excludes channel-restricted paint, and the
fitter adapter restores exact endpoints only within 1e-7 (material drift remains an error).
Only those five failing new tests were rechecked; the two passing tests were not repeated.
The first UI run passed three tests and caught a harness error in the numeric-input test:
egui_kittest::Node::type_text only queues text and does not focus a DragValue. The test now
clicks the SpinButton, selects its contents and types, following the existing 06/17 tests
and egui_kittest 0.36.2 source; only that failed test was rechecked.

## GPU tests supplied; gates intentionally not executed

1. Compiled ignored engine `blob40_gpu_rtx5090_vulkan_preview_release_eraser_and_undo_pixels`:
   RTX 5090 name + Vulkan + DiscreteGpu + Rgba32Float assertions; preview/release/undo
   premultiplied CPU/GPU max difference <=1/255. No software-adapter skip path.
2. `pc-ui-egui/src/vector_packages/blob_eraser/app_gpu.rs`, awaiting the production hooks
   and central registration by 00: full-app offscreen live pointer preview/release, area
   and Alt rectangle erasure, Escape/undo, actual float GpuCanvas texel parity.
3. Same source contains an ignored release real-app 32 pointer-frame median/p95/p99 and
   release measurement with asserted hardware identity. No measurements/speed claim here.

The engine GPU test is compiled but ignored; the two app gates are source supplied and
unregistered until Tool variants and PhotocraftApp::blob_eraser are wired. They are missing
integrated acceptance, not passed or adapter-skipped tests. Production menu/router/toolbar,
stroke proxy, prefs and i18n/icon wiring remain explicitly owned by 00/01/46. Detailed
fragments and commands are in [integration/40.md](vector-tasks/integration/40.md).

## Sources, attribution and corpus

Followed pinned VectorCraft engine `cmd/draw2.rs` and tool `draw2/gesture.rs` at
`d522c1d7be4035bd4f4a84cd6ebfca44f5155092` from the owner's local reference clone,
MIT OR Apache-2.0, copyright ArtCraft Team/contributors. Existing licence notices retained;
one PORTS row for each adapted file is supplied to 00 in integration/40.md for attribution
regeneration. The swept homothetic ellipse hull is original GPL-3.0-or-later code: 17's
support-normal brush outline does not model the whole variable-diameter swept area, so
this package implements that mathematical region locally and delegates all unions to V1.

Official behavior sources accessed 2026-10-10:
[Adobe Blob Brush](https://helpx.adobe.com/illustrator/desktop/paint-and-fill/learn-painting-basics/merge-paths-using-the-blob-brush-tool.html),
[Blob Brush options](https://helpx.adobe.com/illustrator/desktop/paint-and-fill/learn-painting-basics/blob-brush-options-and-best-practices.html),
[Eraser](https://helpx.adobe.com/illustrator/desktop/draw-shapes-and-paths/modify-paths/erase-paths-using-eraser-tool.html).

Fixture source/license/tool versions/parameters are recorded in owned engine/UI and
`pc-vecfx/tests/sweep/SOURCES.md`. All fixtures are synthetic original GPL-3.0-or-later,
generated at runtime. External/proprietary goldens/corpus were not supplied, not run and
are not included in successful gate counts. No proprietary fidelity claim is made.
