# Develop A completion report

Continued the merged C1–C3/A1 implementation in this branch. The owner's pre-existing `CODEX-TASK.md` edit is preserved. Changes are uncommitted. Only `lc-engine` and `lc-ui-egui` source/tests were changed; no dependencies, Cargo manifests/lockfile, pc-* crates, li-seg/li-ai, or `quickseg.rs` changes. No GUI windows or owner photo writes.

## Delivered

- **A2:** Sync/Copy/Paste retain the existing whole-`masks` transfer, including name, components, opacity, visibility and tools. The Masking checklist now explicitly explains replacement and AI recalculation. Engine and real-dialog tests cover whole-layer sync, cache stripping, Copy without Masking, and unchanged target layers under Auto Sync. Undo/redo and every UI frame resolve stale selection to Background. Deleting the selected layer also selects Background. History identifies the layer for tool controls, grouped curve/grading/opacity drags, WB, curve presets, mask drawing/refinement, visibility and inversion.
- **A3:** `panels/layer_bar.rs` at the top of Edit. Compact Background/+ with no layers; collapsible full list with layers. Eye, truncated name/full-name tooltip, double-click/Enter rename, percentage scrub, selected-layer opacity slider, kebab Invert/Duplicate/Delete/Reset/Rename, drag reorder, keyboard Up/Down selection and confirmed keyboard Delete. New Filled/Empty and all mask kinds accepted by the existing creation commands. Component Add/Subtract/Intersect menus plus direct **Intersect with Luminance Range**. Empty creation opens the component menu. Every supported Develop section targets the selected layer, with layer names in section headers and the canvas pill. Vignette, Grain and B&W display disabled headers, reasons and working Background links. Other unsupported image-level tools (Profile/RAW, Negative, Tone Equalizer, Optics and Calibration) also explain their global scope rather than editing Background through a selected layer.
- **A4:** Edit and Mask share canvas mask handles, pins, brush/object interaction and overlays. Brush Size/Feather/Flow and Add/Erase/Auto Mask settings are available in Edit while painting. Switching between those panels preserves the tool. New gradients draw from the press origin and group creation/drawing into one undo entry. Overlay has a persisted, default-safe **Only while drawing** option, limited to mask gestures. K paints into the selected Empty layer. Subtract/Intersect Brush creates the appropriate component and paints its coverage; Erase edits the brush's coverage. Photo changes clear transient layer rename/drawing/delete state.
- **A5:** `develop.wbPick {mask?}` and `curve.applyPreset {mask?}` target layer tools without changing global WB/curve. Layer UI has the WB picker and curve preset dropdown; Reset also targets the layer. Saving a curve preset accepts `mask?`, so that footer action captures the curve the user is editing. UI WB source loading/orientation/sampling runs on the existing named task worker and applies the captured photo/layer on completion, guarded against photo/library changes. Engine commands retain their synchronous caller contract.
- **A6:** Non-neutral imported `adjust` data appears expanded in **Lightroom local adjustments** in Edit, with its original sliders and no migration. Mask no longer contains these sliders or duplicate Develop tool sections. The panel/menu/tooltip name is **Mask**; it retains component controls, brush settings, edge refinement and overlay options.

## Validation

Pinned Rust/Cargo 1.98.1 through the supplied `claude-env.sh`, offline, four build jobs, one cargo lane and the same retained `target/` throughout.

- Strict all-target Clippy passed on both changed crates.
- Focused engine layer tests: **7 passed** (including two new A tests and the existing Auto Sync test).
- Integrated A/C walkthrough run on the RAW fixture: **10 passed, 0 failed, 1 benchmark ignored**. The benchmark was subsequently run explicitly on hardware and passed.
- Keyboard/Empty/brush/reorder/delete follow-up: **1 passed**.
- Complete changed-crate run performed once: **engine 883 passed / 0 failed / 6 ignored** (499 unit + 381 coverage + 1 Develop backend + 2 panic-hunt cases). **UI 325 passed / 2 failed / 5 ignored** initially. Both UI failures were old routing assertions: K expected Mask, and the WB tooltip test used Background section ids after selecting a layer. Their corrected targeted rechecks each passed (**2 passed / 0 failed**); the assertions now verify Edit routing, brush controls in Edit and the layer WB tooltip. This accounts for all **327 non-ignored UI cases**, with no unresolved failures. The complete suites were not rerun after those focused repairs, following the owner's testing budget.
- Final A acceptance recheck on the RAW: **6 passed / 0 failed / 1 benchmark ignored**; the caption/undo test then passed on the final name-only canvas-pill implementation (**1 passed**). The GPU benchmark was explicitly run again on that final Edit path (**1 passed**).
- Documentation tests passed for both crates (0 examples). Final all-target Clippy passed on both changed crates; formatting and scoped diff checks passed. The sole all-tree whitespace warning is the owner's pre-existing task-file trailing blank line, which remains untouched.

Final logs: `target/develop-a-full-tests.log`, `develop-a-classic-keys-final.log`, `develop-a-tooltips-final.log`, `develop-a-acceptance-final.log`, `develop-a-pill-final.log`, `develop-a-doc-tests.log`, `develop-a-clippy-final3.log`. **TEST-CHANGED:** the engine's two generic tool-history expectations now include the auto-named layer; old headless layer-tool locations/Reset actions follow Edit; K and WB tooltip expectations follow the newly required routing. Assertions were retained or extended, with no new ignores or weakened checks.

The six new normal headless A tests use actual widget clicks, pointer drags, text/key input and dialogs. They cover A, B, C, D1, E2, F, legacy imports, masks in Edit, keyboard list selection and the screenshot matrix. Existing C tests now use Edit for layer curves/Skin Tone and continue covering D2/E1. The Filled test also compares real rendered pixels for layer +1 EV and Background +1 EV. B tests grouped radial creation/drawing, invert/50% scrub and five-step undo/redo, plus independent rename and eye history entries. Selection stays UI state on redo and cannot reference a deleted layer. The old Masking layer-tools test was moved to Edit and uses the new Reset menu; its layer isolation and used-section assertions remain.

Failure logs are retained under `target/develop-a-*`. Early failures were compile errors, stale test widget identifiers, an import fixture that content-deduplicated its four identical files, and a missing fixture before generation completed. None was ignored or allowlisted. Test files now have distinct content and live only under this worktree's target directory.

## H1–H2: real GPU interactive measurements

New ignored test `tests_develop_a::develop_a_layer_drag_interactive_gpu_budget`, adapted from the existing C benchmark to use the real Edit layer bar, requires a RAW of at least 45 MP, the RTX 5090 and Vulkan. It asserts latest displayed textures, draft limit during live drag, retained GPU stages and a **16.7 ms UI-frame budget**. Six measured live moves per case; layer 4 is selected for the 4/8-layer cases. Preview completion is awaited on each move and on release. These are step latencies, not continuous-drag FPS.

Input: generated **8256 × 5504 Bayer DNG (45.44 MP)**, using the repository's DNG writer and synthetic RAW test conventions. This exercises actual RAW decoding and the app's render jobs but is not a photographic sensor benchmark. Generator/source/DNG: `target/develop-a-ux/reference/`.

Adapter: **NVIDIA GeForce RTX 5090 (Vulkan)**.

| Case | UI p95 | Change to displayed texture p95 | Retained stage-cache bytes | Sampled process VRAM peak |
|---|---:|---:|---:|---:|
| 1 Brush, exposure + clarity | 1.28 ms | 15.53 ms | 243,719,320 | 1,238 MiB |
| 4 Brush, exposure + clarity | 0.99 ms | 24.75 ms | 413,623,384 | 1,752 MiB |
| 8 Brush, exposure + clarity | 1.53 ms | 33.72 ms | 640,162,136 | 1,752 MiB |
| 8 Filled-only exposure layers | 1.17 ms | 7.82 ms | 621,691,640 | 1,752 MiB |

VRAM is sampled per test process from NVIDIA's XML process table at 100 ms intervals (14 samples); it excludes other desktop processes but includes retained allocations from earlier cases and the driver's allocator. It is a sampled peak, not exact allocation instrumentation. A 200 ms idle interval after each case is outside the timings and permits sampling the short Filled case. The first sampler queried compute-only entries and missed Vulkan; the corrected sampler reads graphics entries and matches the executable inside this worktree. Earlier runs/logs are preserved (a previous run sampled up to 2,006 MiB; retained driver allocations vary). Final logs: `target/develop-a-interactive-gpu.log`, `target/develop-a-vram.json`; sampler: `target/develop-a-ux/measure_gpu.py`.

Filled-only layers retain C's scalar representation. The alpha-layout implementation (`lc-gpu/src/render.rs::alpha_layout`, including the Full-only diagnostic path) skips their spatial planes; the existing C hardware/parity coverage remains unchanged. The equal retained VRAM numbers above are not used as proof of zero alpha-plane allocation. H3 full-resolution eight-layer/NR export is outside this task.

Reproduce:

```sh
source ~/.local/share/local-image-dev/claude-env.sh
LC_REQUIRE_GPU=1 LC_DEVELOP_A_RAW="$PWD/target/develop-a-ux/reference/45mp.dng" python3 target/develop-a-ux/measure_gpu.py
LC_DEVELOP_A_REFERENCE="$PWD/target/develop-a-ux/reference/45mp.dng" LC_DEVELOP_C_REFERENCE="$PWD/target/develop-a-ux/reference/45mp.dng" cargo test --offline -p lightcraft-ui-egui tests_develop_ -- --nocapture
```

## Visual review

Forty PNGs cover five states × two themes × two widths × two pixel scales. I inspected the dark/light contact sheets at both scales and individual narrow menu/global-only captures. Layer controls, menu actions, disabled reasons/links and tool labels are readable and stay within their panel. The short “Only used tools” label avoids narrow-panel overflow; its tooltip retains the full explanation. Plus-menu entries only expose the kinds the existing command accepts (People/Landscape currently have no creation command).

All images are under `target/develop-a-ux/`. The complete filename matrix is:

- `compact-{dark,light}-{250,400}-{1,1.5}.png`
- `expanded-{dark,light}-{250,400}-{1,1.5}.png`
- `global-only-{dark,light}-{250,400}-{1,1.5}.png`
- `kebab-{dark,light}-{250,400}-{1,1.5}.png`
- `plus-{dark,light}-{250,400}-{1,1.5}.png`
- `inspection-dark.png`, `inspection-light.png`, `inspection-dark-hidpi.png`, `inspection-light-hidpi.png` (contact sheets).

Individual screenshot files:

```text
compact-dark-250-1.5.png
compact-dark-250-1.png
compact-dark-400-1.5.png
compact-dark-400-1.png
compact-light-250-1.5.png
compact-light-250-1.png
compact-light-400-1.5.png
compact-light-400-1.png
expanded-dark-250-1.5.png
expanded-dark-250-1.png
expanded-dark-400-1.5.png
expanded-dark-400-1.png
expanded-light-250-1.5.png
expanded-light-250-1.png
expanded-light-400-1.5.png
expanded-light-400-1.png
global-only-dark-250-1.5.png
global-only-dark-250-1.png
global-only-dark-400-1.5.png
global-only-dark-400-1.png
global-only-light-250-1.5.png
global-only-light-250-1.png
global-only-light-400-1.5.png
global-only-light-400-1.png
kebab-dark-250-1.5.png
kebab-dark-250-1.png
kebab-dark-400-1.5.png
kebab-dark-400-1.png
kebab-light-250-1.5.png
kebab-light-250-1.png
kebab-light-400-1.5.png
kebab-light-400-1.png
plus-dark-250-1.5.png
plus-dark-250-1.png
plus-dark-400-1.5.png
plus-dark-400-1.png
plus-light-250-1.5.png
plus-light-250-1.png
plus-light-400-1.5.png
plus-light-400-1.png
```

## References and scope

Followed the accepted spec's Owner decisions/Review incorporated sections, review §2/§3/§5/§6, and threading policy §2. Reused the established `Target::Layer`, mask/component commands, `extract_groups`/`transfer_settings`, numeric-slider interaction ownership, Headless pointer/control protocol, C curve/Skin Tone widgets, source-cache/preview jobs and `tasks::spawn` completion pattern. The existing AI commands are called unchanged; their separately planned `quickseg.rs` threading work was not folded into this task.

Primary API references read locally: egui **0.36.2** source/docs in `~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/egui-0.36.2/src/{containers/popup.rs,widgets/drag_value.rs,widgets/checkbox.rs}`; NVIDIA `nvidia-smi` XML process reporting. The synthetic RAW generator follows `lc-raw/tests/it/dng.rs` and `lc-raw/src/dngwrite.rs`. The layer bar's composition of these existing widgets and its selection/drag bookkeeping are task-specific implementation choices.

Keyboard profiles B1–B3 and X1 release/design-document work remain separate streams. No task work remains. No commits were made.
