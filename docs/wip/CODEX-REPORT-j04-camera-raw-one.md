# J4 / D17 — one Camera Raw engine

Implemented on the existing branch; changes are uncommitted. The existing Develop bridge,
PSD mapping and native-preview worker were continued rather than replaced.

## What merged

- Legacy `filter.cameraRaw`, `filter.develop` fallback-dialog previews, native refinement and
  commits now use `photocraft_io::develop_filter` → `lightcraft_pipeline::render`.
  `CameraRaw` remains the same serde-defaulted PSD descriptor, with validation and identity
  checks. Its module shrank from **1,087 to 292 lines** (795 lines / 73.1% removed, including
  obsolete renderer tests). Duplicate tone/dehaze/NR/grain, HSL processing and spline code are gone.
- The UI curve graph samples the pipeline's own curve tables, including parametric regions.
  `HSL_BANDS` remains descriptor/UI metadata, rather than another processing implementation.
- The bridge carries proxy scale through the pipeline's existing `SourceInfo.sensor_scale`;
  invalid dimensions and scales return errors. Identity previews preserve input exactly.
- Native detail uses full effective settings to decide identity, so B&W/other tools outside
  the descriptor do not disappear at native zoom. Rendering errors reach the existing preview
  error field instead of becoming successful original-image results. Existing serialization,
  cancellation, revision rejection, viewport uploads and image-size bounds remain in place.
- Document-profile RGB is used consistently for legacy commands, smart-filter dispatch and
  previews. Display copies convert to sRGB for egui textures; processing, samplers and scopes
  retain document RGB. Alpha remains untouched by processing.
- Dialog edits apply only changed descriptor controls. Untouched custom WB outside descriptor
  ranges, f64 curves, individual curve splits, grading/mixer values and unsupported full settings
  are preserved. Full Develop sessions open with their engine already attached and render once
  instead of constructing a legacy preview then rendering again.
- Existing `lens_cmds::camera_raw_surface` and `apply_to_surface` public APIs remain thin sRGB
  adapters. Profile-aware document paths use fallible adapters. All repository callers of
  `pc-algo::camera_raw::{develop,curve_lut,band_weights}` were migrated/removed before deleting
  those processing APIs. A pc-algo→pc-io re-export would introduce a dependency cycle; the task's
  descriptor-only model and existing engine adapters provide the retained compatibility boundary.

## Quality and speed evidence

Quality here measures **preview fidelity to the existing commit renderer**, not an independent
claim about photographic restoration quality. The task's selected shared engine already uses the
established darktable detail ports. No broad image sweep or GPU benchmark was run, following the
latest minimum-testing override.

A generated 96×64 F32 test pattern (no external/client photos) was rendered through the real
`PhotocraftApp` Camera Raw dialog in an offscreen eframe harness. Four settings were compared with
the Develop bridge before removing the duplicate engine and after consolidation. The interactive
render timer includes processing, histogram and egui texture preparation. Native-size pixels
have no resampling ambiguity.

| Setting | Before RGB RMSE | Before max error | After RGB RMSE / max error | Before render ms | After render ms |
|---|---:|---:|---:|---:|---:|
| Dehaze 40 | 0.12489718 | 0.95432508 | 0 / 0 | 0.868 | 9.169 |
| Texture 35 + Clarity 30 | 0.00821517 | 0.02821970 | 0 / 0 | 0.463 | 11.874 |
| Luminance NR 45 + Color NR 35 | 0.03424731 | 0.12019208 | 0 / 0 | 0.553 | 5.085 |
| Grain 45, seed 42 | 0.02690863 | 0.15072697 | 0 / 0 | 0.132 | 3.246 |

A separate **902×24 → 451×12** smooth exposure fixture compares the actual dialog proxy against
the box-downsampled committed surface: maximum RGB error **0.00000080**, below the test's **0.0001**
tolerance (well below one 8-bit display level).

The shared CPU renderer costs more than the former approximate preview on this tiny fixture;
maximum fidelity takes priority. These are single small-sample CPU measurements of the app path
on a machine shared with other jobs. Load was uncontrolled; the numbers do not establish
percentiles, GPU speed or statistically comparable hardware performance. CPU/Rayon scheduling,
existing ≤900-pixel proxy, serialized native worker and viewport-bounded uploads adapt without
machine-specific settings. A full Develop dialog's opening render count improves **2 → 1**,
asserted by the UI test. No peak-RSS claim was made; existing memory bounds were retained.

## Validation

Only new tests were run, plus cargo check and strict clippy, per the task's final testing override.
All commands use `PATH="$HOME/.cargo/bin:$PATH"`, `cargo +1.98.1`, `--offline`, the same retained
worktree `target/`, and the launcher's unchanged `CARGO_BUILD_JOBS=4`. One cargo process at a time.

| Validation | Result |
|---|---|
| `cargo +1.98.1 check --offline` on all four changed crates | **Passed** |
| strict `clippy --offline --all-targets -- -D warnings` on all four changed crates | **Passed** |
| `test --offline -p photocraft-algo j04_` | **1 passed**, 0 failed; integration target had 0 matching tests |
| `test --offline -p photocraft-io --lib j04_` | **3 passed**, 0 failed |
| `test --offline -p photocraft-engine --lib j04_` | **1 passed**, 0 failed; 18 depth/mode/profile combinations |
| `test --offline -p photocraft-ui-egui --lib j04_ -- --nocapture` | **4 passed**, 0 failed |
| Existing PSD Camera Raw round-trip tests | Source unchanged; not rerun under the new-tests-only rule |
| GPU tests added/run | **0 / 0**; no GPU acceptance/performance claims |
| Dependency/unsafe audit | No manifest or Cargo.lock changes; no dependencies or unsafe added |
| `cargo +1.98.1 fmt` on the four changed crates, final `--check` | **Passed** |
| Diff whitespace check | Changed implementation/report files clean; pre-existing task-file EOF warning left alone |

New tests cover descriptor loading/defaults, precise full-settings edits, proxy scale against the
pipeline, malformed old curves rendering, profile/depth-aware smart filters with alpha preservation,
full-settings native identity and display conversion, native preview/commit pixels and actual UI
slider drags/Before toggles/OK clicks. The full-settings UI test commits a Display P3 document and
checks WB/curve precision and unsupported settings after a real Exposure drag.

The final check/lint commands selected `photocraft-algo`, `photocraft-io`, `photocraft-engine`
and `photocraft-ui-egui` using four `-p` arguments. Clippy used `--all-targets -- -D warnings`.
The final isolated timing probe uses only the new `j04_preview_equals_commit_interactive` test,
matching the baseline's single-test execution and avoiding overlap with the other new UI tests.

Logs are under `target/j04-*.log`. The baseline probe ran one new measurement test before the
renderer was removed. All **9 distinct new tests passed**. The final isolated fidelity/timing test also passed after lint fixes.
Initial validation caught a test dragging the caption instead of the
accessible slider and a missed private helper argument after removing the double opening render;
those were corrected without weakening assertions. Initial strict clippy also caught a
redundant clone of Copy WB and a constant-size chunk iterator in a new test; both were corrected. Failure logs remain in target. The final
focused tests check real slider input and committed pixels.

## Compatibility, UI and user-visible changes

- `CameraRaw` fields, serde defaults, legacy `pixelScale`/`seed`, PSD descriptor templates and
  full-settings PSD private records were not changed. Existing catalogs/settings/documents load.
  Existing saved malformed curves stay lenient when replayed; new edits remain validated.
  Legacy `pixelScale` stays serialized; preview scale is supplied separately and commits use
  native scale rather than trusting saved preview metadata. Native refinement retains its
  existing 64-megapixel limit and native worker requirement; oversized/web previews keep
  the bounded proxy.
- Previously mismatched legacy Camera Raw processing now has the Library Develop look, including
  dehaze, tone/curves, HSL/grading, Texture/Clarity, detail, vignette and grain. Old edits may
  therefore render differently, as permitted by the task. Downsampled previews remain approximate; native refinement shows committed pixels.
- Untouched full settings and profiles no longer get silently changed by the fallback dialog.
  The curve graph now includes the actual parametric curve. Wide-gamut/linear-profile display
  copies are correctly converted for egui textures.
- No controls, layout, icons or styling were added. The existing shared sliders, checkboxes and
  dialog buttons remain; `camera_raw_preview_ui.rs` and `camera_raw_scope_ui.rs` needed no changes.
  The one new validation message is in `docs/wip/i18n-pending-j04-camera-raw-one.md`.

## Unavoidable edits outside the owned implementation files

| File | Why |
|---|---|
| `pc-engine/src/filters.rs` | Pass the already-available document profile to legacy Camera Raw smart-filter dispatch. |
| `pc-engine/src/smart_cmds.rs` | Supply that profile when legacy RAW smart filters render, including previews below an edited filter. |
| `pc-algo/examples/bench_photo.rs` | Remove benchmark cases calling the deleted duplicate engine; remaining photo cases are unchanged. No benchmark run. |
| `pc-algo/src/histogram.rs` | Replace a test's duplicate-engine call with its known half-stop transfer calculation. |
| `pc-algo/src/photo_util.rs` | Mark two formerly renderer-only helpers test-only; preserve their existing unit-test coverage without dead-code warnings. |
| `pc-algo/tests/coverage/ds_camera_raw_coverage.rs` | Remove callers/tests of the retired engine, retain descriptor serialization/validation tests. |
| Report and pending-string document | Required evidence and deferred translation record. |

`docs/wip/CODEX-TASK.md` was already modified on entry and was not edited by this job. No Match Look
files, manifests, lockfile, owner's Desktop files or running GUI applications were touched.

TEST-CHANGED: `pc-algo/src/camera_raw.rs` removes tests specific to the deleted renderer; preserves
existing descriptor validation/loading checks and moves legacy-curve rendering evidence to a new
bridge test. Their old renderer assertions were not repurposed as claims about the new algorithm.

TEST-CHANGED: `pc-algo/tests/coverage/ds_camera_raw_coverage.rs` removes duplicate-renderer/spline/HSL
processing tests and keeps the four descriptor/default/validation/serialization tests. Default
identity test no longer calls a removed processing API.

TEST-CHANGED: `pc-algo/src/histogram.rs` retains quantization-comb assertions using a known exposure
transfer, independent of either develop engine. `photo_util.rs` helper tests are unchanged.

TEST-CHANGED: `pc-ui-egui/src/camera_raw_ui.rs` updates one existing private `open_pixels` test call
with `None` for the newly attached engine argument; its behavior/assertions are unchanged.

## Sources followed

- [Adobe: Sharpening and noise reduction in Camera Raw](https://helpx.adobe.com/camera-raw/desktop/using/sharpening-noise-reduction-camera-raw.html), checked during this task: assess detail at least at 100%; retained the existing native-resolution refinement.
- Existing pipeline scale and rendering contracts: `lc-pipeline/src/lib.rs`, `detail.rs`,
  `primary.rs`, `local.rs`, `finish.rs`; existing profile bridge `pc-io/src/develop_filter.rs`.
- Existing darktable haze and NR ports: `lc-pipeline/src/detail/{haze,nr}.rs`, upstream commit
  `733bd69f32cac7ff5e41025115942772add1f088`, provenance and GPL notices in their headers,
  `licenses/darktable-NOTICE.md`, and `docs/PORTS.md`.
- Existing Fritsch–Carlson spline: `lc-color/src/spline.rs`; the UI delegates through the
  pipeline's `finish::curve_luts`, using the same tables as rendering.
- Existing `egui_kittest` app harness/control patterns and `docs/DESIGN-SYSTEM.md`.

No work remains in the implementation scope. Integrated/full/GPU suites are deferred to the
coordinator by the task's explicit minimum-testing rule, not presented as completed here.
