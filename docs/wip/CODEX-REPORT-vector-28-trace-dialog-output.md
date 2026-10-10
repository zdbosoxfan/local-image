# Package 28 — trace dialog/output report

Continued the branch at `4a67efaf`, preserving T1's tracing algorithms, command IDs,
job framework, document representation and renderer. Changes are uncommitted.
The owned dialog/output implementation is supplied; **integrated package acceptance
is not claimed complete** while the named providers and shared app hooks are absent.

## Implementation

- Added the resizable egui modal with Split/Result/Outlines/Overlay views, draggable
  divider, independent preview wheel zoom/pan/Fit, palette strip, merge/remove/reset,
  document-swatches provider input, controls, stats and Trace/Cancel. Preview gestures
  leave the canvas transform, document, history and journal untouched.
- Logo, BlackWhite, FewColors, Silhouette, Photo and PixelArt presets adopt T1's actual
  defaults. Added serde-default `Preferences.last_preset` and a reopening/serialization
  test. Permanent preference storage is an exact field fragment for 00's existing
  UI-state save flow, not a new store or a claim of app integration.
- Background previews reuse T1 `Session::start` and `jobs::run_latest`: source/document
  keys cancel old requests, controls moving request 0.25 scale and release requests
  full scale. Workers render detached source/result images through the production
  rasterizer/compositor. Typed request mailboxes transfer paths/pixels without expanding
  pixel arrays into JSON or reparsing path JSON on the UI thread. Recent job history
  stores only small UI-preview summaries. Automation retains T1 detached trace queries.
- Opening captures document ID/revision, source ID, active layer and all selected IDs.
  Latest job/mailbox ownership and capture checks prevent late/canceled/stale results
  from being shown or committed. Previews do not lock edits or journal. Cancel drops
  the mailbox/textures and cancels the job; apply recomputes at full resolution.
- Extended only T1 output/preview command handling. Apply prepares transformed native
  paths, connectivity and bounds-limited caches on the worker, revalidates captured
  document/revision/target set, then inserts one group in one `Session::edit` and one
  journal entry. Group names/hex colour child names retain T1 behavior. Palette merges
  normalize repeated paints through the existing visible-region Pathfinder Merge,
  preserving occlusion, and removals remove that output paint's paths.
- Expand uses fallible native `regions` to split connected islands while retaining
  counters. BlackWhite Make Work Path stores a unique named path and `doc.work_path`
  together in one undo, without adding shape layers. Empty output errors without an
  empty group/history entry. Output fields have serde defaults and reject malformed
  options; finite/bounded scale, geometry, noise and palette limits are checked.
- Raster and rendered/cached smart pixels are supported, including selection alpha
  and document crop offsets. Exact `scale_xy` corrects odd-sized preview rounding.
  Nonpixel sources, empty/oversized selection and locked destination ancestors disable
  tracing with reasons. Locked background pixels remain readable.
- Added the owned integration/i18n handoff and original fixture source records. The
  final task registration override permits the two added UI module declaration lines;
  manifests, lock, catalogs, UI menus/router/preferences and algorithms are untouched.

## Verification

Final unique new-test results: **10 engine tests passed, 7 CPU UI tests passed**.
One supplied real-app GPU test is ignored/deferred, not counted as passing.
Existing tests were filtered out. No external corpus or performance gate was run.

Commands (with `PATH="$HOME/.cargo/bin:$PATH"` and the environment below):

- `cargo +1.98.1 check --offline --workspace --all-targets`: passed after corrections;
  log `target/trace28-workspace-check-complete.log`.
- `CARGO_TARGET_DIR=target/clippy cargo +1.98.1 clippy --offline
  -p photocraft-engine -p photocraft-ui-egui --all-targets -- -D warnings`: passed;
  log `target/trace28-clippy-final.log`.
- `cargo +1.98.1 test --offline -p photocraft-engine -p photocraft-ui-egui
  --lib trace28_`: initial engine run passed 8 and failed 2; UI was not executed
  after the engine failures. Log `target/trace28-tests.log`.
- The same package/filter command with eight passing engine names skipped:
  corrected engine fixtures **2 passed**; first UI run **2 passed, 5 failed,
  1 GPU ignored**. Log `target/trace28-correction-ui-tests.log`.
- Focused `trace28_ui_` reruns skipped passed tests. After the layout correction,
  **4 passed, 1 failed** (`target/trace28-ui-layout-correction.log`); the final
  preset correction plus revalidation of the two tests affected by the layout
  yielded **3 passed** (`target/trace28-ui-final-correction.log`). These results
  cover all seven unique CPU UI tests; they are not summed as extra tests.
- `cargo +1.98.1 fmt -p photocraft-engine -p photocraft-ui-egui -- --check`
  and `git diff --check`: passed.

The initially failing exact-coordinate/mask fixtures used tiny B&W fitted curves.
Their analytic expected coordinates/coverage were retained and their preset changed
only to exact PixelArt; the separate B&W work-path test passed. UI failures exposed
and fixed inherited horizontal column layouts (controls offscreen), the missing
accessible preset label, and a disabled colour slider clamping a binary preset from
one colour to two. Preset defaults are now retained. Strict clippy also required
`as_chunks` for the new fixed-size pixel assertion. Only corrective filtered reruns
were performed; no successful broad test suite was repeated. One engine-only
corrective compile was stopped before tests to avoid rebuilding a different feature
graph; subsequent filtered runs kept the same two-package feature graph.

All Cargo commands use Rust 1.98.1, `--offline`, `CARGO_BUILD_JOBS=3`, the unchanged
`RUSTC_WRAPPER=/home/zdavidson/.local/share/local-image-dev/bin/li-rustc-gate`, and one
Cargo build at a time. No whole-crate suites, whole-workspace tests, GPU runs, release
builds or benchmarks are run under the final owner speed override.

The initial workspace check found missing scroll-event phase fields in two new test
fixtures. They were corrected; a corrective workspace all-targets check passes. A
reviewed job-delivery correction added typed mailboxes and recent completion handling;
strict clippy identified one collapsible conditional, which was corrected. Final
verification includes these changes, rather than treating the earlier checks as final.

New engine tests exercise analytic rectangle RGB/alpha membership, ring/island area
and independent exact-area coverage, cache bounds, atomic undo/journal, work/named
paths, latest requests/cancel, stale revision/targets/document, cached smart/odd crop
coordinates, palette edits/fixed palette, invalid/nonpixel/locked/empty output and
pixel mailboxes without large JSON history. UI tests click presets, swatches, merge,
remove, mode/view controls, Trace and Cancel; drag sliders/divider/preview; type numeric
text; wheel zoom; hover disabled Trace; check stale edit, prefs and undo behavior.

Added one ignored offscreen real-app GPU gate:
`trace28_gpu_live_modal_zoom_drag_cancel_apply_undo_canvas_parity`. It calls the real
PhotocraftApp plus the modal in frame order and asserts RTX 5090, Vulkan, discrete
adapter, Rgba32Float accumulator and F16 canvas. It requires actual preview mode/zoom/
pan render changes and premultiplied canvas cancel/apply/undo max difference <=1/255.
A wrong/missing/software adapter fails. It opens no desktop window. Its minimal launch
wrapper is explicit test wiring pending 00's permanent menu/app fields.

## Remaining integration / gates

- **00:** install menu aliases, transient app state, persistent UI preference field,
  show/cancel hooks, translated catalog rows and a real document-palette provider.
  Exact fragments/signatures are in `vector-tasks/integration/28.md`. Existing Swatches
  is a fixed colour grid, not a document store. Empty provider input disables the
  option; injected document-library behavior is covered by the owned test host.
- **25–27 / 00 Params integration:** the baseline's centerline/cleanup/cutout slots are
  empty, and T1 Params lacks their extensions/stroke output. LineArt/Sketch/Centreline,
  Cutout, smoothing/snap lines/circles/symmetry and upscale are visibly disabled with
  tooltips. No unavailable mode or algorithm gate is claimed. Extend owned controls/
  shape output after the documented provider interfaces are actually merged.
- **29:** use the existing Library open bridge, then the same dialog open hook.
  Package 28 does not edit Library state or implement a second tracer.
- **Coordinator:** permanent full-app menu/prefs/handoff acceptance, the supplied real
  RTX offscreen gate and B.4/B.6 corpus/manual acceptance. GPU and release/performance
  execution are explicitly deferred by the owner speed rule. No 60fps claim or timing
  claim is made. Missing external/proprietary corpus is not counted as passing: T1's
  15 external SVG / five licensed scan inputs and competitor reference gates remain
  coordinator work as recorded in its report.

## Sources and attribution

Followed the read architecture/persona Owner decisions, quality-plan B.3/B.5 and T1
report. Established code/API sources: `pc-engine/src/jobs.rs` run_latest/cancellation/
recent results, `vector_cmds.rs` cache preparation, `vector_api.rs` transaction validation,
`pc-pathops` fallible regions/Pathfinder Merge, `pc-ui-egui/src/dialogs.rs` modal patterns,
and existing persona GPU gate identity/readback pattern. Official egui 0.36.2 sources
were read from the locked local Cargo cache (modal, Resize, slider, response, scroll events);
see [egui Modal](https://docs.rs/egui/0.36.2/egui/containers/struct.Modal.html) and
[Response](https://docs.rs/egui/0.36.2/egui/struct.Response.html). The attempted remote
0.36.0 docs were unavailable; implementation follows the actual locked 0.36.2 sources.

Read pinned VectorCraft `crates/ui-egui/src/panels/image_trace.rs` at
`d522c1d7be4035bd4f4a84cd6ebfca44f5155092` as a behavior reference, not an adapted
source file. The new dialog/mailbox/output glue and synthetic fixtures are original
Local Image GPL-3.0-or-later code, implemented on the established APIs. No additional
PORTS row/third-party notice/attribution fragment is required (zero new adapted upstream
files); T1/pathops notices and records remain intact. No dependency, unsafe, C/C++, -sys
or brand-asset addition. Fixture licences, dimensions, parameters and tool versions are
recorded in the owned `trace_output/SOURCES.md` and `trace_dialog/SOURCES.md`.
