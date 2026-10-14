# On-device AI Remove — LaMa (N3 / G4)

Status: complete and verified. Changes are uncommitted.

## Scope and result

Read the executive summary, W10, Q3, G4 and N3 roadmap row in
`docs/wip/REVIEW-holistic-fable.md` (the review arrived in the worktree during this
run). Continued the existing brush/adjust/confirm UI, model manager and patch jobs.
No previous LaMa implementation or LaMa report was present at the start. Preserved
the existing modified task file and unrelated concurrently supplied review/research files.

- Added unmodified FP32 big-LaMa ONNX inference through the existing tract runtime.
  Both the original Samsung Research LaMa license and Carve's pinned model card
  declare Apache-2.0. Verified the downloaded export's 208,044,816-byte size and
  SHA-256 `1faef5301d78db7dda502fe59966957ec4b79dd64e16f03ed96913c7a4eb68d6`.
- Registered AI Remove through the existing `li-ai::catalog::Catalog`, which now
  borrows the established `li-seg::MODELS` descriptors. The manager and inference
  service resolve that catalog entry; the runner takes the descriptor. There is
  no copied registry, additional model-spec type or model folder.
- Registered AI Remove as a function on Preferences → AI → On-device Tools. It uses
  the existing `download_seg_files` path, verified downloads, progress, cancellation and model removal. UI
  readiness reads metadata; workers verify integrity and cache compilation by file
  stamp. Removal releases the cached plan.
- Added Automatic / On-device · LaMa alongside existing Klein / Qwen options.
  Automatic chooses LaMa when ComfyUI is disconnected; explicit generative choices
  remain available and persist. Compositing's shared selection action checks the
  same readiness as the brush tool. Missing models preserve painted work and link
  directly to On-device Tools.
- Reused Develop's AI host, background jobs, reversible AiView encoding and existing
  patch store. Compositing still publishes a repair layer. Soft coverage goes
  through the existing compositor once. All existing opacity/delete/copy/undo
  mechanics remain applicable. LaMa is deterministic: Regenerate does not produce
  a random alternative from a different seed.
- Large-photo inference separates distant marks at empty bands, uses native context
  crops when possible, and resizes connected large holes together. Aspect is
  preserved; reflect padding and conservative mask pooling avoid distortion and
  disappearing thin strokes. The 32-region cap retains every hole. Inference is
  serialized per cached model and cancellation is checked between tract nodes and
  compositing stages. Thread count uses the existing shared Rayon budget.
- Added no dependencies, C/C++/`-sys` crates, unsafe code, settings schema or catalog
  schema. Cargo.lock is unchanged. Old saved engine keys and missing AI fields load.
  No Match Look file, desktop image, app window, running app or owner file was touched.
  Added strings to the editor's pending translation list; model/host labels are
  shared by Develop. No translation work or SAM 3 changes.

## Duplicate-systems steering (D15 / D14 / D18)

Read the audit's D14, D15, D18 and J13 sections after the owner's steering update.
The catalog projection shares the exact descriptor addresses (contract-tested),
so pins and license metadata are not duplicated. Download installation remains
`download_seg_files` → `li-ai::download`. Cancellation/progress use `JobControl`
callbacks directly and remain compatible with a borrowed `pc-raster::Interrupt`
view; no new job-control primitive was introduced. Develop continues through
`AiHost` → `RemoveResult` → the existing `Kind::Remove` patch store. No Heal or
`spots.rs` change was made; J13 can reuse that path later.

## Measured quality

All originals are real photographs whose **file licenses** explicitly say CC0-1.0
(not merely Wikimedia's structured-data license). Original downloads, source-page
snapshots, URLs and SHA-256 fingerprints are in `target/lama/photos/manifest.json`.

1. Interior: [Ray S (CA), Interior of ground floor](https://commons.wikimedia.org/wiki/File:Interior_of_ground_floor.jpg),
   2736×3648, photographed at the Integratron.
2. Exterior: [History Trust of South Australia, Exterior of a house GN15008](https://commons.wikimedia.org/wiki/File:Exterior_of_a_house(GN15008).jpg),
   3264×2448, a digitized architectural photograph.
3. Portrait: [Official portrait, 1st Lieutenant Anna Mae Yu Lamentillo](https://commons.wikimedia.org/wiki/File:Official_Portrait_-_1st_Lieutenant_Anna_Mae_Yu_Lamentillo.jpg).

`li-seg/examples/lama_eval.rs` compares two ways to prepare the same unmodified
model. Photos are limited to a 1600-pixel long edge; a deterministic central
rectangular holdout is blacked out, then reconstructed against the known clean
original. This measures **reconstruction**, not accuracy of removing a real object
whose hidden background is unknown. Metrics are masked-only PSNR / MAE in 0–255
RGB. Every unmasked pixel was byte-identical in every result.

| Photo | Context crop PSNR / MAE | Whole-photo square PSNR / MAE | Crop CPU seconds |
| --- | --- | --- | --- |
| Interior | 23.076 dB / 13.199 | 23.717 dB / 12.139 | 5.718 |
| Exterior | 14.033 dB / 39.305 | 13.064 dB / 43.812 | 4.925 |
| Portrait | 21.406 dB / 16.328 | 16.905 dB / 28.484 | 3.528 |

Selected context crops by measured quality: +0.969 dB exterior and +4.501 dB
portrait; the whole-photo alternative wins the interior by 0.641 dB. Arithmetic
mean masked PSNR: 19.505 vs 17.895 dB (+1.610 dB). Mean MAE: 22.944 vs 28.145
(18.5% lower). Do not interpret three images as a comprehensive quality guarantee.

Inspected `target/lama/eval/{interior,exterior,portrait}-sheet.jpg`, each showing
original / masked input / chosen repair / alternative. LaMa reconstructs texture
and broad structure but does not reliably recover the exact hidden architectural
line or cloth detail. Hard rectangular boundaries can show tonal seams. The
portrait crop preserves finer cloth texture than the whole-image reduction; the
exterior remains plausible foliage but differs substantially from the hidden
original. Qwen remains available for semantic fills. Large connected removals are
resized rather than split across tiles, deliberately favoring coherent structure;
fine detail in such repairs is limited by the fixed 512 export.

## Speed and application verification

Measured on the real Ryzen 7 9800X3D CPU, with the default shared Rayon executor
and other jobs sharing the machine. No GPU speed is claimed for LaMa. Initial
compatibility probe: 4.563 s load, 7.660 s inference. The photo bake-off loaded in
5.476 s and used 3.528–5.718 s per context crop. These include production tract
execution and image preparation/compositing, not an ONNX Runtime speed estimate.

Expected Apple Silicon Mac speed is **an unmeasured planning estimate**: roughly
5–25 s per 512-pixel context crop on an M-series CPU, plus first-use compilation.
Older Intel Macs may take substantially longer. A Mac was unavailable here, so
there is no measured Mac performance claim. No comparable LaMa/tract Mac benchmark
was found; [tract's supported CPU targets](https://github.com/sonos/tract) include
x86 and ARM. This is a budgeting range, not a platform performance guarantee.
Download size, inference resolution,
thread scheduling and concurrency are independent of this PC's core count/GPU.

Numerical reference: ONNX Runtime 1.31.0 (CPU) versus tract on the same 512×512
photo tensor and mask: 786,432 values, maximum absolute error **0.00048828125 on
the 0–255 scale**, mean absolute error 0.000001428; **zero rounded pixel differences**.
Reference packages/scripts live only in `target/refvec/`; none enter the build.

Actual `Session::execute("ai.remove")` on original-resolution CC0 photos (including
mask preparation, color management, inference and repair layer), cold / warm:
interior 14.366 / 7.711 s; exterior 11.895 / 6.811 s; portrait 9.134 / 4.068 s.
The latter two use the refined masks and current catalog wiring; first-mask runs
and their sheets/logs are retained separately.
Undo and redo passed for every command, and repeat outputs were deterministic.
Peak process RSS for the two-pass harness: 1.87 / 1.62 / 1.01 GiB respectively.
Object-removal sheets are under `target/lama/objects/` (person, tree, sleeve flag).
These have no known ground-truth hidden background; they supplement the measured
holdout bake-off. After inspecting the first results, enlarged the tree mask to
include a remaining tip and refined the flag mask to follow its slanted edge and
preserve the adjacent badge. Final repairs remove those targets; the person repair
continues the wall/floor plausibly, the tree repair reconstructs sky/foliage, and
the flag repair retains surrounding cloth but introduces softer texture. All three
final outputs have **zero changed pixels outside their masks** and repeat exactly.
Preservation checks use the app's decoded baseline: the initial Pillow comparison
included decoder differences, which disappeared when using native-decoded PNGs.

Real offscreen app (GPU canvas enabled, NVIDIA RTX 5090, Vulkan, NVIDIA driver
615.71.09): brush → subtract/adjust → confirm kept the source untouched until
completion, then added one repair layer. Submission **45 ms**, total cold job
**11.638 s**. During inference, actual GPU render + readback frames averaged
**21.857 ms**, p95 **36.770 ms**, maximum **66.132 ms**. Readback is included; these
are not native-present-only frame times. Undo/redo and no GPU fallback were asserted.
Inspected `target/lama/ui/real-confirm.png` and `real-repaired.png`.

Strict real RTX 5090 compositor parity test: original, LaMa repair and undo/redo
renders all passed; maximum premultiplied RGBA error **2.9802322e-8**, comfortably
below half an 8-bit level. The actual CPU inference + layer command took 13.134 s
in that cold 320×240 fixture. No llvmpipe or successful no-adapter skips are used.

Model-manager screenshots: **8 images**, dark Pro / Studio Light, 250 / 400 pt,
1.0 / 1.5 pixels per point. Rendered on the strict RTX 5090 helper and inspected all
variants via `target/lama/ui/model-sheet-{1,1.5}.png`; labels wrap and controls fit.

## Verification

Final verification uses `PATH=~/.cargo/bin:$PATH`, `cargo +1.98.1`, `--offline`,
the retained worktree target and launcher `CARGO_BUILD_JOBS=4` throughout.

- Format: all five source crates.
- Final clippy: all six affected/embedded-data crates, `--all-targets -- -D warnings`,
  passed. The strengthened GPU integration test was subsequently linted again.
- Final full tests: only these crates, one combined invocation after the audit
  steering; **4,642 passed, 0 failed, 136 normally ignored** (logs in
  `target/lama/final-tests.log`, exit code 0). Counts include library, integration
  and doc test targets.

| Crate | Passed | Failed | Normally ignored |
| --- | ---: | ---: | ---: |
| `li-seg` | 87 | 0 | 8 |
| `li-ai` | 513 | 0 | 6 |
| `photocraft-engine` | 2116 | 0 | 34 |
| `photocraft-ui-egui` | 1766 | 0 | 82 |
| `local-image` | 155 | 0 | 6 |
| `li-attributions` | 5 | 0 | 0 |

- New filtered model/control tests: 6 `li-seg` + 2 `li-ai`; 3 new editor interaction /
  compatibility tests; app host dimensions/engine registration test. All passed.
- New real GPU gates explicitly run: **3 passed** — repair-layer parity/undo/redo,
  actual app brush/adjust/confirm responsiveness, and model-manager screenshots.
  Logs: `gpu-patch.log`, `gpu-real-app.log`, `gpu-ui.log`.
- Existing Preferences test now clicks Manage AI Remove and checks its model label.
- Attribution generator succeeded; regenerated JSON adds only the LaMa notice.
  License text is embedded and `li-attributions` tests passed.
- `Cargo.lock` and Match Look files are unchanged. Own-file `git diff --check` passed;
  the pre-existing task file's trailing blank line was left untouched.

Preserved the first filtered UI failure logs: dropdown queries initially used a
label instead of a value, and the first readiness hint exceeded a toolbar row;
corrected the queries and reused the existing compact warning layout. The first
full UI run had one stale download-count assertion (8 before LaMa, 9 after); added
explicit LaMa title/model assertions and corrected the count. The final suite is
fully green. The application build interrupted at the steering update was retained
as an interruption, not classified as a test failure. No full workspace suites,
cache sweeps, installed queue changes or concurrency overrides were used. App-test
screenshots were relocated intact from its package-local `target/` into
`target/lama/app-test-artifacts/` to keep generated files out of the source diff.

Nothing required remains unfinished. Mac throughput is estimated rather than
measured because no Mac was available; semantic/high-detail quality limitations
are documented above. Broader registry/type/job consolidation and Heal migration
are intentionally the later J15/J9/J13 jobs, not part of this feature.

## Sources and implementation choices

- [Original LaMa Apache-2.0 license](https://github.com/advimman/lama/blob/main/LICENSE).
- [Pinned Carve model card / FP32 contract](https://huggingface.co/Carve/LaMa-ONNX/blob/c3c0c9e468934d62e79c329e35d82dd09ff8c444/README.md).
  Followed its recommended FP32 export; output is 0–255, not normalized RGB.
- Local tract 0.23.8 sources: `tract-core/src/plan.rs` node evaluator and existing
  `li-seg/src/enhance.rs` cancellation pattern; reused the app's executor/cache and
  model download conventions. No external inference runtime ships.
- [Adobe Lightroom Classic Remove help](https://helpx.adobe.com/ie/lightroom-classic/desktop/process-and-develop-photos/remove-tool.html):
  brush, refine with Add/Subtract, then Remove. The owner already chose this flow.
- `docs/DESIGN-SYSTEM.md`, current shared dropdown/buttons/model rows and theme
  tokens. No new control design. Extended the existing offscreen screenshot helper
  by visibility only to reuse its strict RTX 5090 renderer.
- The fixed-export context/splitting/pooling policy is an original adaptation;
  selected it through the measured bake-off rather than claiming an upstream
  high-resolution algorithm. Model graph and weights remain unmodified.

Extra-area changes are minimal: Develop's app host adapter, Compositing's shared
context-action readiness, attribution data/license notices, and test-helper
visibility. No Library/Develop UI source or other job's engine feature was changed.
