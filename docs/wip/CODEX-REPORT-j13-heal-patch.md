# J13 / D18 — one patch backend for Develop healing

Continued the existing content-aware Heal/AI Remove work already in this branch. Changes are uncommitted.

## Delivered

- `lightcraft_pipeline::spots::heal_patch` is the shared scene-linear adapter to the existing editor's multi-scale PatchMatch completion and Poisson seamless clone. The LOCAL background worker is a thin JobCtl/cancellation/progress adapter to it. The production frequency-separation Heal implementation is deleted; its old implementation exists only as a test oracle.
- Automatic catalog `spot.add` Heal and Remove now use the existing LOCAL background job, patch store, source identity/geometry validation, selection, regeneration, opacity, and single-result undo path. They do not require an AI host. The command description documents `{job}` instead of `{index}` for these modes, including `wait` and `seed`.
- Clone keeps its original feathered copy renderer and automatic-source APIs. Explicit-source Heal/Remove, older saved spots, dust spots, and Camera Raw Filter descriptors use a bounded compatibility adapter to the same Poisson backend. Manual sources supply their selected texture; source-less old/Camera Raw edits use PatchMatch. This keeps standalone rendering and self-contained Camera Raw descriptors working without a process-global injected engine callback.
- The Develop canvas sends the semantic Heal/Remove mode to the command instead of implementing its own catalog-vs-Camera-Raw decision. Existing widgets/labels/monochrome icons are reused. No new UI strings or pending translations are needed. Errors from all three ordinary tools are now surfaced through the existing error toast.
- Feathered LOCAL patches now retain fractional brush coverage instead of converting every nonzero edge into an opaque patch. Hard masks retain the established solved-border pixels exactly. Legacy feather/opacity are preserved, and zero-opacity spots bypass the solve.
- Existing LaMa engine registration/selection and AI Remove's brush → adjust → confirm flow remain in place; the new UI gesture test also exercises draft cancellation without submitting it.

## Compatibility and user-visible changes

Saved Spot/DevelopSettings/AiPatch serialization is unchanged; new automatic heals reuse the existing `Ai` + `engine: local` representation. Older Heal/Remove settings still load, retain their editable source pins, and now render with Poisson healing. Clone is bit-identical on automatic/manual sources, brushed strokes and edge cases.

New automatic Remove strokes now behave like the already-existing content-aware Heal: asynchronous generated pixels, Regenerate/opacity/delete, no movable source pin. They use the existing LOCAL undo label, `Heal`. Explicit-source command callers retain the inline, movable source behavior. Camera Raw Filter remains inline because its descriptors cannot depend on an external patch store; it now uses the shared completion backend rather than frequency separation. Geometry changes require regeneration for stored patches, as for existing LOCAL heals.

The existing pipeline ordering remains: stored patches before WB/negative processing, inline source spots after those stages. The LOCAL worker sees earlier stored patches; this task does not change mixed Clone/inline-spot ordering. The existing Poisson/fallback membrane routines do not have mid-solve cancellation; cancellation checks before/after still discard cancelled results, with PatchMatch cooperatively cancellable throughout.

## Quality and speed evidence (small samples only)

- LOCAL automatic completion: **0 differing floats** against the previous LOCAL implementation on the deterministic 48×40 fixture (pixels and alpha), with an additional exact command/patch/render parity test between ordinary Heal and AI mode's LOCAL engine.
- Clone: **0 differing float bits** against the premerge renderer on a 640×420 textured fixture with automatic/manual sources, a brushed path, and edge spots. Automatic source ranking is also identical.
- Source healing: deterministic 96×80 RGB gradient + periodic texture, with a damaged 16×16 area and a specified matching source. Linear RGB RMSE falls from **0.00239806** (frequency separation) to **0.00172731** (shared Poisson), a **27.97% reduction**. Legacy Remove and Heal produce identical repaired pixels; pixels outside the stroke are unchanged.
- Soft-edge alpha: fractional coverage is preserved exactly (including 0.25 and 0.75); zero-opacity rendering is bit-identical, and 50% opacity is the expected interpolation.
- One 96×80 source-heal CPU sample: old frequency-separation **test oracle 0.213 ms**, new shared Poisson adapter **0.647 ms**. The new source solve costs more in this small sample and was selected for its measured quality improvement. This is not a premerge optimized-renderer benchmark or a general speedup claim.
- Actual headless Develop app path, 64×48 synthetic image, real **NVIDIA GeForce RTX 5090 (Vulkan)** renderer:

  | Tool | Brush gesture returned | Patch arrived | Selected patch opacity drag |
  | --- | ---: | ---: | ---: |
  | Heal | 11.83 ms | 19.68 ms | 16.19 ms |
  | Remove | 12.03 ms | 22.45 ms | 11.47 ms |

  These are single small-fixture observations, including control-event processing, not statistical performance claims or a large-photo projection. Both opacity drags leave the patch key/pixels unchanged. No separate GPU acceptance test was added/run; this new UI interaction test uses the app's real GPU rendering path.

New automatic repairs are solved once and stored; subsequent opacity changes composite the same patch. Legacy/Camera Raw solves are limited to the stroke plus three brush diameters of context, with the existing 48–600 px bounds. Existing stage caching remains responsible for reuse. No machine-specific tuning, concurrency changes, or cache sweepers were introduced.

## Validation

Only new tests, filtered by `j13_`, are run, following the task's ABSOLUTE MINIMUM TESTING override. No full crate/workspace suites, GPU acceptance suites, ignored benchmarks, or long sweeps are run. All commands use `cargo +1.98.1`, offline, the same worktree target directory and the inherited `CARGO_BUILD_JOBS=4`; only one cargo process runs in this job.

- `cargo +1.98.1 fmt -p lightcraft-pipeline -p lightcraft-engine -p lightcraft-ui-egui`: completed.
- `cargo +1.98.1 check --offline -p lightcraft-pipeline -p lightcraft-engine -p lightcraft-ui-egui`: passed, including the final revision.
- `cargo +1.98.1 test --offline -p lightcraft-pipeline j13_ -- --nocapture`: **6 passed**, 0 failed. Final run includes the feather fix; an earlier five-test development run also passed before that change.
- `cargo +1.98.1 test --offline -p lightcraft-engine j13_ -- --nocapture`: **5 passed**, 0 failed.
- `cargo +1.98.1 test --offline -p lightcraft-ui-egui j13_ -- --nocapture`: **1 passed**, 0 failed, physical RTX 5090 Vulkan path.
- `cargo +1.98.1 clippy --offline -p lightcraft-pipeline -p lightcraft-engine -p lightcraft-ui-egui --all-targets -- -D warnings`: **passed**.
- **12 new/focused tests passed**. The existing `craft-fonts` build-script notice appears; it predates this work.
- Diff whitespace check passes for all J13 changes (the untouched task file has its pre-existing trailing blank line). Cargo.lock package/version entries were compared against HEAD and are identical.

Logs and premerge reference copies stay under `target/j13/`. Data is generated synthetic test content; no Desktop/client photos were read or uploaded. No GUI windows were opened and no running app/service was stopped. No ComfyUI job was submitted.

## Unavoidable edits outside the assigned files

- `crates/lc-pipeline/Cargo.toml` and `Cargo.lock`: two dependency **edges** to already-present, already-locked workspace crates `photocraft-algo` and `photocraft-raster`. **Zero new packages/versions, external dependencies, C/-sys crates or unsafe code.** Static reuse is necessary for standalone/Camera Raw pipeline rendering; adding another implementation or a global callback with a missing-engine fallback would defeat the merge.
- `crates/lc-engine/src/cmd/masks.rs`: route automatic semantic Heal/Remove commands to the existing LOCAL worker and document the response. An overlay-only change would leave automation/other callers on a different backend.
- `crates/lc-engine/src/tests_enhance.rs`: two existing tests need explicitly sourced inline spots to continue testing their original regeneration/index and copy-exclusion behavior.
- `crates/lc-ui-egui/src/tests_ai.rs`: the existing headless interaction helpers exercise actual tool clicks, canvas drags, selected-patch opacity dragging, and AI draft cancellation.
- This report. The pre-existing owner changes to `docs/wip/CODEX-TASK.md` were left intact.

TEST-CHANGED J13: `tests_spots::spots_select_move_edit_refresh_and_delete` uses Clone for the automatic source-pin fixture, since automatic Heal/Remove now create stored local patches. All source-pin assertions remain.

TEST-CHANGED J13: `tests_enhance::{bugfix_develop_regenerate_follows_patch_after_earlier_spot_deleted,copies_and_other_photos_never_get_ai_results}` use explicit-source Heal fixtures to preserve the original inline-spot intent; their assertions remain.

TEST-CHANGED J13: the pipeline mixed frequency-separation/Clone bit-exact test becomes `j13_clone_matches_reference_bit_exactly`. Healing is covered by exact LOCAL parity and measured improvement against the old frequency-separation test oracle. The obsolete ignored frequency-separation benchmark is removed, not run.

## Sources followed

- Repository `docs/wip/AUDIT-duplicate-systems-fable.md` §1, D18 and J13; `docs/DESIGN-SYSTEM.md`; prior content-aware Heal work (`ca2129a4` / `7bf461a7`) and merged on-device LaMa work (`35c587cd`).
- Existing pure-Rust implementations and their documented algorithms: `crates/pc-algo/src/inpaint.rs` (Wexler completion / Barnes PatchMatch), `crates/pc-algo/src/poisson.rs` (gradient-domain clone, multigrid/SOR), and the original LOCAL adapter in `crates/lc-engine/src/enhance/remove.rs`. No new healing algorithm was invented.
- [Adobe Lightroom Classic Healing tool help](https://helpx.adobe.com/au/lightroom-classic/desktop/help/healing-tool.html): source-area editing and feather/opacity semantics inform the preservation of explicit-source/Clone controls. The owner's existing content-aware Heal behavior takes precedence for new automatic local patches.

J13 implementation and the required targeted validation are complete. Broader merged quality/GPU/full suites remain deferred exactly as the owner’s minimum-testing override requires.
