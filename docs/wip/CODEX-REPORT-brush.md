# Brush batch report

Implemented in the existing branch; changes are uncommitted. `docs/wip/CODEX-TASK.md` was already modified when this job began and was left intact. The existing pressure-curve implementation, continuous soft-brush renderer, ABR importer and preset store were retained and extended.

## 1. Wayland pressure and platform ordering

- Auto checks `BTN_TOOL_PEN` in `/proc/bus/input/devices`, `DISPLAY`, and the existing X11 library probe. On requests compatibility without requiring detection; Off keeps native Wayland. `LOCAL_IMAGE_NATIVE_WAYLAND=1` overrides compatibility. Preferences › Tools has **Pen tablet: use compatibility mode (XWayland)**, Auto / On / Off, a restart note and the fractional-scaling trade-off.
- Safe startup adaptation of PhotoCraft 652b9726: replace the process with the same executable/arguments and remove only `WAYLAND_DISPLAY` and `WAYLAND_SOCKET` from the replacement's environment. Winit then chooses X11. This avoids adding the upstream patch's direct winit dependency or using unsafe environment mutation. Failure to replace logs an error and retains the native session.
- Install the macOS monitor inside eframe app creation, after NSApplication exists, and retain it through the event loop. AppKit button-up keeps the contact sample; zero-pressure lift samples no longer overwrite the frame's contact readings.
- No tablet with BTN_TOOL_PEN was attached here. Input paths were tested with synthetic AppKit/XInput2/touch events and actual canvas UI gestures. Physical pen and macOS runtime verification remain hardware-dependent.

## 2. Upstream ports and regression coverage

| Upstream | Result and test |
|---|---|
| 652b9726 | Detection/modes, macOS ordering, button-up retention; `wayland::tests`, AppKit mapping tests and canvas lift regressions. |
| 994a0421 | Contact survives a cleared/zero feed; stationary taps use peak contact pressure. Actual press/release tests in `canvas_brush_batch_tests`. |
| cc4a13b5 | One commit for a tap/release; click fallback uses pen pressure. The history-count and pressure tests exercise actual UI clicks. |
| 917fbcb0 | Bounded sample queue, interpolation of pressure/tilt/short-arc rotation for each move, one drain per egui frame. Three moves with 0.4/0.6/0.8 in one UI frame retain those pressures. |
| 03d45a80 | Shift-click stores and interpolates the preceding endpoint's pressure; the stroke-constraint and real mouse Shift-click tests cover it. Press modifiers now come from the press event. |
| ab3e6ebe | Accumulated stroke depth controls Wet Edges for preview, commit and retouch coverage; `wet_edges_shape_the_stroke_not_each_dab` checks a uniform interior and darker stroke rim. |
| 55c1847f | Stroke Path dialog, all listed brush tools, pressure taper, single undo/journal step across subpaths. Engine taper/Blur/Pattern Stamp tests and an actual right-click dialog/tool/checkbox/OK UI test. This fork lacks standalone `paint.patternStamp`, so the path implementation uses the existing pattern compositor with brush coverage, in bounded tiles. |
| dbadd21a | Pen contacts disable touch press-and-hold through lift; the actual egui held-contact test verifies pens vs fingers. |
| 8d23c622 | Retained the existing `pen_pressure_curve`, cubic evaluator and editor. Added the missing direct-setting validation: 2..16 unit-square point pairs. Whole-section/old-file loads remain lenient and the existing evaluator sanitizes them. |

## 3. Brush packs

One **Import Brushes…** entry and the existing file/drop routing accept ABR, GBR, GIH, VBR, bundle, KPP, brush, brushset, MYB and afbrushes, including uppercase extensions. The existing `brush.presets.importAbr` command remains the compatible, permission-classified entry point; `format` selects non-ABR base64 input, and paths infer their extension. Decoding uses the existing cancellable background-job path. Imported groups use the file stem, and reimport replaces its group. A wholly skipped pack never erases an existing group.

- **GIMP:** GBR v1/v2 grayscale/RGBA, concatenated GIH cells, spacing and selection metadata, and VBR 1.0/1.5 circle/square/diamond/star geometry, hardness, aspect and angle. Image pipes become separately selectable tip variants; angular selection also maps to Direction. Generated hardness profiles and pipe selection are explicitly approximate. RGBA tips use alpha coverage with an approximate note; their full original colour artwork is retained as lossless PNG data because the paint engine's sampled tips are coverage masks.
- **Krita:** manifest-based bundles; KPP PNG text XML, including 5.0 embedded resources; paintbrush size, opacity, flow, spacing, angle, pressure/tilt/drawing-angle/rotation controls; auto, GBR, GIH variants, PNG and SVG tips, and GIMP PAT/PNG patterns. SVG uses the app's existing bounded offline importer/compositor through a callback, with no new IO dependency. Nonlinear curves and engine differences are noted as approximate. Smudge/colorsmudge/roundmarker/hairy/bristle use a pixel-brush approximation; unsupported engines, absent resources and unreadable presets are skipped with a reason. Older standalone KPPs requiring separate installed resources must be imported through their bundle; no external resource paths are followed.
- **Procreate:** brush folders and brushset ZIPs, bounded bplist00/NSKeyedArchiver scalars and UID resolution, size/opacity/spacing/jitter/pressure mapping. Shape and Grain PNG bytes are retained exactly, and grayscale mask samples preserve 16-bit precision. Source-image and 16-bit pixel regressions prove both. Dynamics and grain behavior are approximate.
- **MyPaint:** JSON v3 radius, hardness, opacity/flow, dab density, ellipse geometry, eraser and pressure inputs; flagged approximate. The optional preview image is not required for import; panel previews use the actual mapped brush.
- **Affinity:** detected and reports exactly **“Affinity brush files can't be read yet”**. No undocumented proprietary decoder was guessed.

Approximate presets show a small `approximate` note with mapping details in both list/grid tooltips. Synchronous and background imports share the summary `Imported N brushes, A approximate, S skipped (reasons)`; existing ABR warnings remain visible as notes. Metadata and exact source artwork survive preset-store reload. The group-file cap accommodates the bounded artwork plus metadata while the overall 2 GiB store cap remains.

Security limits: new pack file 128 MiB; entry 32 MiB; archive expansion / decoded artwork 256 MiB; 4,096 entries; 2,048 presets; image sides 8,192 and 16 Mi pixels. ZIP paths, duplicates, CRCs and decompression bounds are checked without extracting to disk. XML is limited to 2 MiB / 16,384 nodes / depth 64, with no DTD/entity expansion/external I/O. Plists are limited to 8 MiB / 32,768 objects / 131,072 references / UID depth 32; scalar materialization is also capped at 8 MiB. Object graphs retain references rather than recursively expanding, including when many keys reference one large string. Truncation, dimension, ZIP path, plist and XML tests run without panics.

Five small CC0 fixture assets (~90 KiB combined) plus source/licence records were copied from the approved corpus. Larger and restricted-license packs were only read for opt-in local testing. `assets/attributions-curated.json` was updated and `cargo xtask attributions` regenerated `assets/attributions.json`.

Read-only local corpus run (1,089 files, including extracted duplicates and AppleDouble metadata):

| Format | Files | Imported presets | Skipped presets | Rejected files |
|---|---:|---:|---:|---:|
| ABR | 7 | 757 | 0 | 2 |
| afbrushes | 1 | 0 | 0 | 1 |
| brush | 29 | 29 | 0 | 0 |
| bundle | 6 | 242 | 122 | 0 |
| GBR | 270 | 270 | 0 | 0 |
| GIH | 191 | 1,295 | 0 | 2 |
| KPP | 380 | 144 | 236 | 0 |
| MYB | 196 | 196 | 0 | 0 |
| VBR | 9 | 9 | 0 | 0 |

The two ABR rejections are AppleDouble `._` files. The two GIH rejections are copies of fairy-dust that declare four cells but contain only one (verified header/body), so they are truncated. The low-level corpus reader has no SVG callback; the app SVG path is separately tested. Many loose KPPs need their bundle's resources. Unsupported engines/missing resources are reported individually, not silently counted as imported. No real brushset was available; folder/manifest cases use generated archives containing licensed real brush data.

## 4. Real app-path performance

Offscreen eframe app, actual pointer input, production LiveStroke and GPU canvas; 6,000×4,000 document (24 MP), zoom 0.15, five warm frames then 35 measured frames per case. Asserted **NVIDIA GeForce RTX 5090 / Vulkan / discrete GPU**, NVIDIA driver 615.71.09, active GPU canvas and no CPU fallback. No desktop window was opened. These are optimized dev/test (`opt-level=1`) diagnostics, not release-build or desktop-compositor frame-rate claims.

The opt-in debug timing scope measures CPU primary/dual dab rasterization plus coverage accumulation on the actual calling thread. Per-dab values are amortized per emitted primary dab, including its dual work; they exclude end-cap/tail restoration, composition and UI work. UI-frame mean/p95 includes the complete app step. Render/readback is measured separately and includes screenshot readback.

| Brush | Raster ms / primary dab before → after | Raster ms / frame before → after | UI frame mean ms before → after | UI frame p95 ms before → after | GPU render/readback mean ms before → after |
|---|---:|---:|---:|---:|---:|
| soft-1000 | 22.86 → 22.79 | 7.84 → 7.81 | 79.66 → 79.37 | 106.32 → 106.11 | 6.47 → 6.40 |
| soft-2500 | 159.74 → 147.15 | 18.26 → 16.82 | 453.00 → 431.08 | 665.84 → 626.03 | 8.99 → 8.05 |
| textured-dual-1000 | 12.97 → 12.75 | 10.38 → 10.20 | 43.75 → 38.57 | 70.47 → 64.43 | 6.87 → 6.60 |

Fixed avoidable preview copies in `pc-paint`: immutable prepared settings and coverage tiles share storage until modified; dual masks are looked up once per coverage tile instead of per pixel. A snapshot-independence / dual seam pixel regression passes. No private thread pool, new dependency, fixed worker count or concurrency override was introduced.

The 2500 px soft brush still has a substantial latency problem. Further cost is in live-tail reconstruction, original-surface restoration and compositing (`pc-engine/src/brush_cmds.rs`), outside this task's paint performance ownership. The small before/after gains must not be read as a general responsiveness guarantee; host contention and a 35-frame sample limit precision.

## Validation

All changed-crate suites completed successfully. Counts below include unit and integration targets, exclude repeated filtered runs, and list intentional ignores separately. Documentation tests completed successfully (no doctests defined in these five library crates; local-image is a binary).

| Changed package | Passed | Failed | Intentionally ignored |
|---|---:|---:|---:|
| local-image | 142 | 0 | 2 |
| photocraft-engine | 2,096 | 0 | 33 |
| photocraft-io | 508 | 0 | 3 |
| photocraft-paint | 203 | 0 | 1 |
| photocraft-tablet | 15 | 0 | 0 |
| photocraft-ui-egui | 1,665 | 0 | 67 |
| **Total** | **4,629** | **0** | **106** |

- `cargo +1.98.1 fmt` on the six changed packages; changed code/assets/report pass `git diff --check`. The pre-existing task-file trailing blank line was left untouched.
- `cargo +1.98.1 clippy --offline -p ... --all-targets -- -D warnings` passed across all six changed packages. IO/UI were rechecked after their final changes; final logs are `clippy-complete.log`, `clippy-last-changes.log`, `clippy-ui-complete.log`.
- Full tests of only the changed packages, one cargo lane, launcher-provided `CARGO_BUILD_JOBS=4` and two test threads. Initial full run is `full-tests.log`. Only affected IO/UI work was repeated after new changes/failures: `full-io-ui-final.log` (passing complete UI unit suite / appearance / contracts / GPU targets), `full-io-complete.log` (complete final IO suite), and `ui-integrations-complete.log` (remaining complete UI targets, including all localization languages, layout16, path text, pen and snapping). `doc-tests.log` completes documentation checks.
- Filtered regressions cover imported groups/store restart, curve validation, SVG rendering, all path tool commands, actual pen taps/moves/Shift-click, actual import/drop, compatibility dropdown and Stroke Path dialog. Additional tests prove 16-bit shape/grain precision, byte-exact original PNG preservation and bounded expansion of shared plist scalars.
- **GPU test added:** `brush_batch_real_gpu_large_brush_performance`, intentionally opt-in, explicitly passed twice on the real RTX 5090/Vulkan, with no CPU fallback. Logs `perf-before.log` / `perf-after.log` and JSON `performance-before.json` / `performance.json` retain measurements. Regular hardware GPU tests also completed; no “no adapter”/llvmpipe result is used for a claim.
- **Corpus test added:** `brush_import_local_corpus`, explicitly passed (1,089 files). Log `corpus.log`. No larger/restricted assets were copied or changed.
- `cargo +1.98.1 run --offline -p xtask -- attributions` succeeded; only the new fixture attribution entry changed generated JSON.

Logs retain initial failures and fixes. The full run exposed the Shift-click event-modifier regression, fixed without weakening the test. Missing toggle/Stroke Path translations were supplied. Pre-existing shared-colour gaps were repaired: missing HSB translations and a Background test selector that also matched the shared colour button. It now selects the actual selected layer row while preserving document/selection/history assertions. Localization integration caught generic translated HSB labels colliding with Camera Raw; shared drawing-colour labels now remain distinct. Only affected suites/targets were repeated; no whole-workspace or deferred GPU milestone suite was run. An existing GPU test regenerated `docs/design/vector-tool-icons.png`; its diagnostic output was retained under `target/brush-work/` and the tracked document image restored. Existing app-test screenshots were moved from the incidental package-local `target/` to `target/brush-work/local-image-test-artifacts/`; the worktree's cargo cache was retained.


## Sources and integration seams

- PhotoCraft requested commits: [652b9726](https://github.com/storytold/photocraft/commit/652b9726), [994a0421](https://github.com/storytold/photocraft/commit/994a0421), [cc4a13b5](https://github.com/storytold/photocraft/commit/cc4a13b5), [917fbcb0](https://github.com/storytold/photocraft/commit/917fbcb0), [03d45a80](https://github.com/storytold/photocraft/commit/03d45a80), [ab3e6ebe](https://github.com/storytold/photocraft/commit/ab3e6ebe), [55c1847f](https://github.com/storytold/photocraft/commit/55c1847f), [dbadd21a](https://github.com/storytold/photocraft/commit/dbadd21a), [8d23c622](https://github.com/storytold/photocraft/commit/8d23c622). Read the supplied bare clone. Existing PhotoCraft MIT/Apache notices/attribution remain.
- GIMP primary formats: [GBR](https://developer.gimp.org/core/standards/gbr/), [GIH](https://developer.gimp.org/core/standards/gih/), [VBR](https://developer.gimp.org/core/standards/vbr/); sector-fold geometry checked against [gimpbrushgenerated.c](https://github.com/GNOME/gimp/blob/master/app/core/gimpbrushgenerated.c).
- Krita [preset documentation](https://docs.krita.org/en/reference_manual/resource_management/paintoppresets.html), [preset resource loading](https://github.com/KDE/krita/blob/master/libs/image/brushengine/kis_paintop_preset.cpp), [brush resource XML](https://github.com/KDE/krita/blob/master/libs/brush/kis_brush.cpp), and the licensed local real-resource bundles.
- MyPaint [brush JSON reader](https://github.com/mypaint/mypaint/blob/master/lib/brush.py) and [setting definitions](https://github.com/mypaint/libmypaint/blob/master/brushsettings.json).
- Apple's [public bplist layout reference](https://github.com/apple-oss-distributions/CF/blob/main/CFBinaryPList.c); the reader is independently written Rust, not copied Apple C code. Procreate scalar names were checked against the licensed Jay Versluis files and NekoPhoto synthetic fixtures. Dynamics mappings without a public exact engine specification are explicitly approximate.
- [Fixture SOURCES](../../crates/pc-io/tests/fixtures/brushes/SOURCES.md) and the owner's read-only `MANIFEST.md` record provenance. `docs/wip/REVIEW-threading-policy-fable.md` was read; no scheduling/pool code was touched.

Required small integration seams beyond the enumerated ownership paths: app `tablet.rs`/`linux_libs.rs`/file filters; UI `stylus.rs`, three-field endpoint storage/raw hook in `lib.rs`, completion summaries in `jobs_ui.rs`, path menu/dialog glue and translations; engine vector path command/tests for the explicitly requested 55c1847f port. The baseline persona selector repair is test-only. Keyboard shortcut parts, thread-policy crates/job pools and queue tooling were not edited. Cargo manifests and Cargo.lock are unchanged; no new dependencies, C/C++ build code or unsafe code were introduced. No owner app was killed, no GUI window was opened, no ComfyUI job was submitted, and no commit was made.
