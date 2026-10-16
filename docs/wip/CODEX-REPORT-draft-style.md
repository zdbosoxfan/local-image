# Draft / Refinement design-system task

Continued the existing branch work. Changes are uncommitted. Changed crates: `photocraft-ui-egui` and `li-ai`.

## Implemented

- **Window and layout:** shared themed frame for native viewport and embedded modal, padded title/preview toolbars, app pill tabs, Properties collapsible sections, padded themed settings cards, fixed action bar, compact History picker. Previews retain zoom/pan, Fit, 100%, comparison, and large side-by-side layout. The zoom readout is a percentage rather than a second “Fit” label. Backgrounds, borders, comparison handle, typography and controls use theme tokens/app widgets, including Pro density and Classic bevels.
- **One Generate path:** both surfaces call the same model picker, precision picker, LoRA picker, sampling rows, readiness warning/actions, size component, and `local_request`. Removed the window's duplicate model/sampling/size/request logic.
- **Size and aspect feedback:** the branch's Generate size row was still the old dropdown. Updated that shared component once, and use it from both the panel and the window: app aspect preset chips, the app's link/unlink icon and linked dimension editing, selected-model grid snapping, and grid hint. Existing open-image dimensions are preserved until the user chooses a preset or edits them. The saved lock has a serde default; old settings load.
- **Per-model refinement defaults (coordinator correction):** added `catalog::refine_default_denoise(model)` with the requested rule: 1.0 for `ReferenceLatent`, 0.35 otherwise. Fresh Refinement settings, refinement model changes, Generate Mode::Refine and its model changes, and “Refine with” defaults/model changes all use it. Missing preset strengths and invalid request strengths use the selected refine model too; explicit saved strengths remain editable within ComfyUI limits. This supersedes the earlier fixed-0.35 implementation. The reference-latent refinement graph in this worktree also needed the source added to both conditionings and an empty sampling latent, so Klein's 1.0 still uses the input image as a reference. Also removed Qwen's hard-coded refinement denoise of 1.0: both direct refinement and its second pass carry the requested strength to KSampler. Qwen's normal Create/Edit defaults remain explicit.
- **Shared refine model picker (coordinator correction):** added `catalog::RefineMethod` / `refine_method(model)`. The shared picker groups ReferenceLatent and TextEncoder edit models under “Edit models — refine by reference”, and img2img models under “Image models — refine by denoise”. It offers only image generators/edit models; upscalers, tool-only models, video/template-only families and custom workflows are excluded. The selected method has the requested one-line hint under the picker in both the Generate panel and Refinement tab. Qwen uses the reference group while retaining its specified 0.35 default.
- **Sampling limits:** Steps/Guidance/Denoise fields and requests use connected ComfyUI limits only, with model recommendations shown as hints. Disconnected sampling has no arbitrary ceiling or denoise floor; steps must be at least one. Integer fields preserve large exact step values. Removed hidden engine denoise clamps/floors that contradicted these controls.
- **Refine LoRAs:** Refinement and Generate's “Refine with” use the same compatible LoRA list and strength editor. `RefineStep.loras` has `#[serde(default)]`; LoRA names/strengths and known family compatibility are validated through the same engine path for both passes. The refine graph carries the selected styles into checkpoint `LoraLoader` and Qwen `LoraLoaderModelOnly` nodes. Old serialized refine steps without LoRAs load empty.
- **Strings:** reused existing labels; the grid hint, two refine group headings and two refine method hints use `tl!` and are in the existing pending AI translation list.

## New regression coverage

- Retained the seven existing Draft/Refinement behavior tests and their intent.
- Added a real-GPU kittest matrix for all five themes, both tabs, and both 1280×800 and 1920×1080 sizes. Checks app pill-tab roles/selection, primary-button styling/density, app numeric fields and visible bounds, and large preview bounds.
- Click/drag tests cover compatible refine LoRA selection/strength/request propagation, section collapse/expand and large unclamped sampling values, fresh/model-change per-model denoise defaults, grouped refine choices/exclusions and method hints, and preset/link/grid dimension editing.
- A new Generate-panel click test switches Refine mode/models and second-pass models in both directions, checks the two groups and method hints, model-specific defaults and preset/request fallback handling. The grouping regression also excludes an explicit video/template-only fixture.
- The existing real Generate-panel test clicks its shared size controls, selects second-pass model and precision and a compatible LoRA, scrubs refine steps, and checks the built request and old saved settings.
- Catalog/graph regressions check reference versus img2img classification/defaults, Klein reference conditioning and its empty sampling latent, and editable denoise.
- Engine tests check second-pass LoRA loader names and strengths, wrong-family and invalid-strength rejection, old-data loading, Qwen's loader/partial denoise, and unclamped graph denoise values.

## Screenshot inspection

Regenerated and visually inspected all 20 theme/tab/resolution PNGs plus the grouped refine model picker (21 PNGs) under `target/draft-style/`. The coordinator correction is visible: Klein 9B shows denoise 1 and the reference hint. The picker screenshot is `pro-1280x800-refine-model-picker.png`, with both requested headers and no tool/upscaler choices. Contact sheets: `inspection-1280x800.jpg` and `inspection-1920x1080.jpg`. Also inspected the individual Pro refinement and grouped-picker renders; the original review inspected Classic separately too. Sections and fields are padded, actions remain visible, History is compact, and no preview/control overlap appeared. Long settings/LoRA lists scroll while the primary actions remain fixed.

The final matrix uses the real **NVIDIA GeForce RTX 5090**, **Vulkan**, driver **615.71.09**, and rejects CPU adapters. `target/draft-style/gpu-adapter.txt` records the adapter. No llvmpipe or missing-adapter result is counted.

## Validation

All original task and coordinator regressions pass. Ran offline with Rust 1.98.1, `CARGO_BUILD_JOBS=3`, one cargo operation at a time, and serial tests. Both changed crates were formatted. Clippy was rerun for both with `CARGO_TARGET_DIR=target/clippy`, `--all-targets -- -D warnings`; both passed.

After the coordinator corrections, ran each changed crate's full suite once:

| Test scope | Passed | Ignored | Result |
| --- | ---: | ---: | --- |
| `li-ai` full unit suite | 79 | 0 | Pass |
| `li-ai` full coverage target | 261 | 0 | Pass |
| UI full unit suite | 969 | 4 | Pass |
| UI GPU target | 54 | 3 | Pass on RTX 5090 / Vulkan |
| UI integration target | 14 | 0 | Pass |
| Doc tests, both crates | 0 | 0 | Pass |

All **1,377 active tests** passed in these final full runs. The seven ignored tests are existing opt-in benchmarks, not adapter skips. All seven original Draft/Refinement tests still pass, including source selection, zoom/pan/comparison, mock-engine/library, and Use/Open behavior.

Focused coordinator checks passed before the final suites: 4 engine unit tests and 3 matching coverage tests (`refine` filter), and 17 UI unit tests (`refine` filter). Added catalog/default and reference-latent graph regressions, a grouped-picker eligibility regression, and a real Generate-panel click test. Extended the fresh/model-change Refinement test and GPU matrix to check per-model defaults, grouping/hints, and Klein's visible 1.0 value.

The GPU matrix remains `draft_refine_gpu_design_system_all_themes_and_resolutions`: 20 theme/tab/resolution screenshots plus one grouped-picker screenshot. GPU selection: `VK_DRIVER_FILES=/usr/share/vulkan/icd.d/nvidia_icd.x86_64.json`, `WGPU_BACKEND=vulkan`. Both screenshot and GPU integration logs record the RTX 5090/Vulkan adapter; no llvmpipe, CPU-adapter or missing-adapter result is counted. Existing live-stroke timing checks also ran on that GPU (case medians 1.26–9.78 ms; largest reported frame 11.11 ms). These are existing canvas checks, not a new Draft/Refinement benchmark.

All 21 final screenshot hashes match the inspected artifacts after the full UI run. Logs and the hash manifest are under `target/draft-style/validation/`: `review-engine-focused.log`, `review-ui-focused.log`, `review-clippy-engine.log`, `review-clippy-ui.log`, `review-engine-full.log`, `review-ui-full.log`, and `inspected-screenshots.json`. Earlier task-validation logs are retained there too.

## Boundaries and remaining work

- No task work remains. Changes and this report are left in the working tree; no commit was made.
- No new dependencies or unsafe code. Builds/tests were offline. No installations or external model downloads were made; download-related tests use their local mock server and isolated test files.
- Final test data/config/model/cache/temp directories are isolated under `target/draft-style/`. The running Local Image app and owner's files were not modified, and no GUI windows were opened. Native viewport styling uses the same frame/content as the headless-tested embedded modal; native OS window decorations were not manually inspected.
- The shared ComfyUI queue was checked and empty. Read-only `/object_info` confirmed actual sampler limits (steps 1–10000, guidance 0–100, denoise 0–1) and both LoRA loaders. No live inference job was submitted; engine graph/transport assertions use local MockComfy.
- The existing build warning about missing optional CJK font assets remains; translations are paused and the new hints/headings are in the pending list. No font assets were downloaded.
