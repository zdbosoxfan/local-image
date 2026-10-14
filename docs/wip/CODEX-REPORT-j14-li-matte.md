# J14 — one shared matting backend

Implemented on the existing branch, preserving the inherited J1 raster kernels and LaMa integration. Changes are uncommitted. The pre-existing modification to `docs/wip/CODEX-TASK.md` was left untouched.

## What merged

- New lightweight `li-matte` holds the editor's guided RGB/gray filters, matting parameter model, edge-width estimation, Smooth/Feather/Contrast/Shift Edge processing, closed-form alpha solver, foreground estimation and Euclidean distance transform. `li-seg::matte` reexports it. This follows the audit's explicitly permitted small-crate alternative: numerical callers do not need tract/ONNX or model downloads.
- `photocraft_algo::{matting,cf_matting}` keep their existing public signatures and editor Region/Surface/tile adapters. RGB guides borrow the original pixel buffer; no input-image clone. `selection::edt` is a thin reexport. Duplicate numerical bodies were deleted, rather than retained as alternate implementations.
- Develop's Gaussian-weighted log-EV guided filter now uses the shared grayscale implementation with its original Gaussian box policy, nonnegative variance and output clamp. Auto Mask retains its max(original, refined) coverage behavior. Box-weighted editor filtering retains its different averaging contract within the same backend.
- Develop Object/Prompt Edge uses the shared logit gain and normalized feather adapter, preserving f64-to-f32 conversion order, the crop scale, Gaussian policy and half-pixel minimum. Existing settings load unchanged; no schema additions.
- Explicit Refine Edges on a single stored AI subject/sky/background/object/prompt mask uses shared closed-form matting. Depth, gradients, brush strokes, heuristic masks and mixed component masks retain guided refinement. The existing refinement amount blend, component inversion, mask inversion, opacity and adjustment amount keep their ordering.
- The GPU mask planner routes the same AI case through `evaluate_one_tone`, uploading its finalized alpha and bypassing the old guided shader and second finalization. Ordinary guided GPU kernels remain intentional CPU/GPU twins.

The audit's claim that Develop lacks Feather/Edge controls is outdated in this branch: Refine Edges, Object/Prompt Edge, and shape/brush feather controls already exist. Their widgets and strings were preserved; no UI, icons, theme tokens, or translation additions were needed.

## Quality and bounded work

The iterative subject solve is bounded to a 1024-pixel long edge, with 256-pixel output tiles and margin `band + 32`. Full-resolution model/detail coverage remains the base; only a bilinearly upsampled correction is applied. Coverage outside the solve-grid unknown band plus one interpolation cell stays unchanged. Seedless masks, constant guides (including spatially constant chromatic RGB), invalid guides and extremely thin solve grids preserve the input.

Each solve tile affine-normalizes its scene-referred RGB without per-pixel clipping/tone mapping. A remote HDR highlight does not change a tile's regularization. Local trimaps lacking both foreground and background seeds preserve original coverage. Broad optical transitions use larger Laplacian windows; a normalized covariance ridge of 1e-5 avoids falling below f32 covariance precision as window area increases. Narrow boundaries retain a 3x3 window. This policy was chosen from the explicit small fixtures below; it is not a claim of universal best quality on arbitrary photographs.

Small generated reference scenes run on the owner's actual CPU through the Develop `evaluate_one` path, using the ordinary optimized-dev test profile. Lower alpha MAE is better. Both before and after include stored SegMask decoding/sampling; before then uses the original Gaussian/log-EV refinement. These are single small samples, not a benchmark suite or a GPU/UI latency claim.

| Sample | Old alpha MAE | Shared alpha MAE | MAE reduction | Before ms | After ms |
| --- | ---: | ---: | ---: | ---: | ---: |
| Equal-luminance color boundary, 96x64 | 0.05822247 | 0.00164443 | 97.2% | 0.597 | 4.164 |
| Same reference at preview size, 384x256 | 0.05903204 | 0.00087439 | 98.5% | 2.998 | 127.532 |
| 50-pixel optical transition crossing a tile boundary, 768x16 | 0.01627604 (hard input) | 0.00011382 | 99.3% | not timed | not timed |

The wide-edge tile-step difference is 0.02377224 (a legitimate smooth-transition step, below the 0.08 seam gate); adding a remote 1e6-valued HDR highlight changes **zero output bits**. The odd 2051x17 case verifies bounded work and finite output with a negative/HDR affine transform.

The quality gain costs CPU time: this shared closed-form route is slower than the old grayscale guided filter. It runs only on explicitly refined AI masks, and bounds iterative image dimensions independently of hardware. Existing editor and Gaussian-guided paths retain their algorithms and bit-exact golden results; no speedup is claimed for moving them. GPU download/upload overhead is not included in these CPU timings. Borrowed RGB adapters allocate **zero bytes** for an input-image copy; model inference remains outside the numerical crate.

## Compatibility and parity gates

Independent old outputs were captured before migration, with original sources and logs retained under `target/j14-before/`. Five editor golden checksums pin every f32 bit of grayscale guidance, RGB guidance, combined Smart Radius/Smooth/Feather/Contrast/Shift Edge, closed-form alpha and estimated foreground. A separate checksum pins Develop's Gaussian log-EV guided output. These protect against a shared-vs-adapter comparison merely agreeing with itself.

New tests also cover zero-copy adapter identity, tiled solver adapter equality, old camelCase/default parameter JSON, old Develop mask JSON, default refinement identity, cropped detail patches, fractional Edge arithmetic, depth dispatch exclusion, inversion ordering, odd/thin input beyond the solve bound, HDR affine invariance, seedless/invalid/constant guides, exact EDT against brute force, a broad edge crossing a tile boundary, remote HDR independence and the GPU planner's host dispatch contract. Existing numerical tests were moved with their implementation; no existing assertion was weakened or removed.

`TEST-CHANGED: none` — existing closed-form tests and the private max-square test moved into `li-matte`; their fixtures and assertions are unchanged. New tests use the `j14_` filter.

## Verification completed

All commands used `PATH=~/.cargo/bin:$PATH`, `cargo +1.98.1`, `--offline`, the retained worktree `target/`, and the launcher's unchanged `CARGO_BUILD_JOBS=4`. One cargo process ran at a time; the review agent did not build or edit.

- Format: all five changed crates passed.
- `cargo +1.98.1 check --offline -p li-matte -p photocraft-algo -p lightcraft-pipeline -p li-seg -p lightcraft-gpu`: passed on final source.
- `cargo +1.98.1 clippy --offline -p li-matte -p photocraft-algo -p lightcraft-pipeline -p li-seg -p lightcraft-gpu --all-targets -- -D warnings`: passed.
- `cargo +1.98.1 test --offline --lib -p li-matte -p photocraft-algo -p lightcraft-pipeline -p li-seg -p lightcraft-gpu j14_ -- --nocapture`: **11 passed, 0 failed**. Counts: shared core 4, editor 2, Develop pipeline 3, segmentation facade 1, GPU-host dispatch 1. Other tests were filtered out.
- `git diff --check` over changed source, lockfile and report: passed. The task file's pre-existing modification was excluded and retained.
- Read-only review checked the numerical migration, legacy policies, overlap/seed safety and GPU finalization ordering. The constant-chromatic-guide edge case found in review was fixed and included in the final focused pass.

Failure evidence is retained under `target/j14-before/`: the first wide-edge fixture measured MAE 0.023245526; widening its kernel without a precision-safe covariance ridge exposed a hard tile seam. The final policy reduces MAE to 0.00011382 and passes the unchanged quality/seam gates. The first strict clippy pass identified a Cow slice `into_iter` warning; using `iter` fixes it without changing numerical output. No existing test was weakened to accommodate either failure.

No full crate/workspace suites, GPU acceptance tests, ComfyUI jobs, benchmark suite or long quality sweeps were run, following the newest absolute-minimum-testing rule. GPU tests added: **one host-only dispatch contract**, **zero device tests**. Real-device transfer/output acceptance remains with the coordinator's merged validation; no GPU speed/parity claim is made here. There are no remaining J14 implementation items. Before-source snapshots and final check/test/clippy logs are under `target/j14-before/`.

## Unavoidable outside edits

- `crates/li-matte/`: lightweight crate alternative expressly allowed by audit D16. Only already locked pure-Rust `li-raster`, `li-color`, `serde` and native `rayon` are used.
- Three consumer manifests and `Cargo.lock`: path wiring. The lock adds one local package and three dependency edges; zero new registry packages, version changes, C/C++ libraries or `-sys` dependencies.
- `crates/pc-algo/src/selection/distance.rs`: reexport the existing EDT from the shared backend instead of duplicating it. Its original selection tests remain in place.
- `crates/lc-gpu/src/render.rs`: necessary to keep the real GPU preview path on the same AI matte as CPU rendering/export. Adds one pure host dispatch test, without launching a GPU.

No edits to Match Look, model registry/download logic, LaMa, settings schemas or UI files. No client/test-shot data was read, uploaded or added. No GUI window was opened and no running application/service was interrupted.

## Sources followed

- `docs/wip/AUDIT-duplicate-systems-fable.md` §1, D16 and J14; the existing branch's matting, EDT, masks and GPU dispatch implementations; the inherited J1 report and raster Gaussian policies.
- [He, Sun and Tang's guided-filter project and paper](https://people.csail.mit.edu/kaiming/eccv10/index.html): local linear guidance and RGB covariance. Develop's Gaussian/log-EV variant is an existing application policy, not claimed as exact paper reference parity.
- [Levin, Lischinski and Weiss, closed-form matting](https://people.csail.mit.edu/alevin/papers/Matting-Levin-Lischinski-Weiss-PAMI.pdf): trimap constraints and matting Laplacian, retained from the existing implementation.
- [He et al., large-kernel matting](https://people.csail.mit.edu/kaiming/cvpr10/index.htm): radius-independent matrix products and faster propagation with wider kernels. The bounded correction, tile normalization, overlap and ridge policy are Local Image adaptations verified on the small fixtures.
- [Germer et al., foreground estimation](https://arxiv.org/abs/2006.14970): inherited coarse-to-fine foreground estimator retained unchanged.
- [Adobe's selection/mask help](https://helpx.adobe.com/photoshop/ipad/select-and-mask/select-mask-on-ipad.html) and [official Refine Edge guide](https://www.adobe.com/products/photoshop/refine-edge.html): feather softens selection boundaries; shift/contrast and decontamination keep their established roles. Existing application units/order are retained; this report does not claim Adobe pixel parity.
