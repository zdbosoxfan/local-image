# AI Enhance / Upscale completion report

Branch work continued from the previous builder; no commit was made. Scope is Upscale Faithful,
Upscale Rescue and Generative. Other denoise/cleanup functions belong to the coordinated jobs.

## Delivered

- Four vetted restoration checkpoints: darktable RealPLKSR 2×/4× ONNX and BSRGAN 2×/4×
  ONNX inside its official ZIP package. Pure Rust tract inference, fixed tiles, context discard,
  overlapping core blending, exact output sizes and cancellation between operations/tiles.
- Measured GPU route through ComfyUI core `UpscaleModelLoader` / `ImageUpscaleWithModel`;
  spandrel's main registry supports both architectures. No extra-architecture package, WGSL port,
  new third-party dependency, unsafe code or application Python/C runtime was added.
  Cargo.lock changes only to add the existing workspace li-seg dependency to li-ai.
- One shared `Ai::enhance` service for the Compositing Enhance dialog, Generate Upscale and
  Library/Develop export. Export restores developed native-size sRGB8 pixels before requested
  colour-space/depth conversion, sharpening and watermarking. Typed cancellation prevents partial
  publication; CPU explicitly avoids ComfyUI, Generative uses its existing GPU graph.
- Exactly one default checkpoint per function: **RealPLKSR 2×**, **BSRGAN 2×**, **SeedVR2 7B FP16**
  (with its required VAE). Requested 2×/4× enlargement does not silently change the checkpoint;
  native inference is followed by exact-size Lanczos3. All four restoration alternatives remain
  selectable in either restoration row. Generate's advanced picker passes its explicit checkpoint.
- Three additive Settings › Local AI function rows follow commit 3764aa80: function/purpose,
  selected model, size/licence, Change…, Custom model…, Use default and existing download controls.
  The existing on-device function rows retain their layout. Selections persist per function.
  Custom ONNX is shape-checked, sample-tested, copied to a hash-based filename and verified on use;
  custom installed SeedVR2-compatible UNET/VAE filenames are supported. Old settings/documents
  load with serde defaults. English additions are in both existing/pending i18n lists; no translations changed.
- Existing vetted downloader, byte/SHA pins, atomic .part publication and retry/cancel tests are used.
  Full MIT/Apache notices and generated assets/attributions.json register both CPU and GPU models.

## Evaluation protocol and sources

All **25 GB82 lossless PNGs**, 576×576 sRGB, are used: two requested enlargement factors,
two input conditions and five model checkpoints = **500 actual GPU outputs**. Bicubic adds
100 reference-resize outputs. Dataset source and all image URL/size/SHA records are retained in
[the evaluation manifest](CODEX-REPORT-ai-enhance-eval-manifest.json); all 600 per-image measurements
are in [the results CSV](CODEX-REPORT-ai-enhance-results.csv). No owner photos were used.

[GB82](https://github.com/gianni-rosato/gb82-image-set) declares all images CC0-1.0. The selected
revision is `502f9f94cb73d1ad5c89ce06fe6b100d0a27df8f`; it includes portraits, landscapes,
closeups, low-light and rendered content. This is a controlled restoration evaluation on an openly
licensed photographic set, not a claim to reproduce a named SR leaderboard. [DIV2K's official
page](https://data.vision.ee.ethz.ch/cvl/DIV2K/) restricts its data to academic research; DIV2K,
SIDD, Kodak and the owner's photo library were not downloaded or used.

The clean input uses image-rs CatmullRom bicubic reduction. The damaged input additionally uses
Gaussian blur sigma 0.6, deterministic Box–Muller noise sigma 10/255 and JPEG quality 50.
[BSRGAN's published scoring implementation](https://github.com/cszn/BSRGAN/blob/main/utils/utils_image.py)
guides PSNR/SSIM: BT.601 video-range Y, scale-pixel border crop, valid 11×11 Gaussian SSIM,
sigma 1.5, K1=0.01, K2=0.03. Its [paper](https://arxiv.org/abs/2103.14006) motivates testing mixed
degradation. Our degradation is a reproducible controlled subset, not its full random training
pipeline; image-rs bicubic is not asserted identical to MATLAB. Scores are per-image means.

Perceptual evidence is cosine similarity of the existing, pinned li-seg CLIP B/32 LAION normalized
512-dimensional center-crop embeddings. It is a semantic similarity proxy, not LPIPS or a guarantee
of visual fidelity. The metric implementation has identity and known-error tests. Seed is 1 throughout.
All candidates return exactly the 576×576 reference size through the production service.

RTX measurements used the real CUDA RTX 5090, ComfyUI 0.38.0, Torch 2.14.1+cu130, and the same
upload/graph/save/download/alpha service as the app. The isolated server on port 8191 used worktree
input/output/temp/user/cache directories and read-only existing SeedVR2 model search paths. The
shared owner server on 8188 was not interrupted. Each queue was checked empty; model jobs ran
sequentially. Model timer includes loading and service overhead, excludes metric calculation and
CLIP embedding. Means include cold samples; GPU memory is process-specific nvidia-smi sampling
at 200 ms and RSS at 20 ms. Measurements ran on a shared workstation, not an isolated performance lab.
CPU is AMD Ryzen 7 9800X3D; CPU inference is the actual tract backend, with no GPU fallback.

## Upscale Faithful comparison: clean inputs

| Candidate | 2× PSNR / SSIM / CLIP | 4× PSNR / SSIM / CLIP | RTX mean seconds 2× / 4× |
|---|---:|---:|---:|
| RealPLKSR 2× | 39.00 / 0.9625 / 0.9976 | 33.38 / 0.8666 / 0.9426 | 0.475 / 0.476 |
| RealPLKSR 4× | 36.86 / 0.9566 / 0.9921 | 32.63 / 0.8707 / 0.9437 | 0.645 / 0.479 |
| BSRGAN 2× | 34.04 / 0.9140 / 0.9638 | 30.76 / 0.8112 / 0.9051 | 0.623 / 0.511 |
| BSRGAN 4× | 33.47 / 0.9132 / 0.9619 | 29.91 / 0.7998 / 0.9181 | 0.712 / 0.538 |
| Bicubic reference | 36.71 / 0.9353 / 0.9715 | 31.55 / 0.8230 / 0.9108 | 0.017 / 0.015 |

**Default: RealPLKSR 2×.** Highest reference PSNR at both requested factors and best 2× SSIM/CLIP.
Native 4× has slightly higher 4× SSIM (0.8707 vs 0.8666) and CLIP (0.9437 vs 0.9426), but lower
PSNR, weaker 2× fidelity and no consistent GPU speed advantage. The default prioritizes reference
fidelity across both sizes. Native 4× remains under Change… and is faster on the CPU timing crop.
BSRGAN's restoration smooths clean texture and scores lower on these clean references.

## Upscale Rescue comparison: blur + noise + JPEG inputs

| Candidate | 2× PSNR / SSIM / CLIP | 4× PSNR / SSIM / CLIP | RTX mean seconds 2× / 4× |
|---|---:|---:|---:|
| RealPLKSR 2× | 30.06 / 0.7705 / 0.8112 | 27.59 / 0.7023 / 0.7403 | 0.476 / 0.438 |
| RealPLKSR 4× | 29.56 / 0.7482 / 0.8172 | 27.19 / 0.6835 / 0.7200 | 0.729 / 0.443 |
| BSRGAN 2× | 31.73 / 0.8325 / 0.9270 | 28.79 / 0.7393 / 0.8526 | 0.652 / 0.484 |
| BSRGAN 4× | 30.90 / 0.8232 / 0.9153 | 27.96 / 0.7187 / 0.8480 | 0.705 / 0.567 |
| Bicubic reference | 30.53 / 0.7947 / 0.8181 | 27.84 / 0.7186 / 0.7455 | 0.026 / 0.023 |

**Default: BSRGAN 2×.** Beats BSRGAN 4× in reference PSNR/SSIM/CLIP at both requested factors.
At 2× it improves PSNR by 1.20 dB and SSIM by 0.0378 over bicubic; RealPLKSR retains the controlled
damage. The 2× checkpoint is also quicker on average on the GPU. BSRGAN can soften genuine grain,
so clean sources should use Faithful. Both native 4× checkpoints remain selectable.

## Generative comparison

The inherited catalogue has one vetted eligible generative checkpoint, SeedVR2 7B FP16. The
four restoration checkpoints above were measured as reference-fidelity comparators; they remain in
their restoration rows, rather than being mislabeled generative candidates. No unvetted generative
model was downloaded. The Generative row allows installed compatible custom SeedVR2 models.

| Candidate | 2× PSNR / SSIM / CLIP | 4× PSNR / SSIM / CLIP | RTX mean seconds 2× / 4× |
|---|---:|---:|---:|
| SeedVR2 7B FP16 | 28.78 / 0.7944 / 0.9551 | 28.48 / 0.7685 / 0.9589 | 1.349 / 1.250 |

| Candidate | 2× PSNR / SSIM / CLIP | 4× PSNR / SSIM / CLIP | RTX mean seconds 2× / 4× |
|---|---:|---:|---:|
| SeedVR2 7B FP16 | 27.96 / 0.7539 / 0.9048 | 25.84 / 0.6511 / 0.8669 | 0.833 / 0.886 |

**Default: existing SeedVR2 7B FP16.** Retained for deliberately generated detail, not selected as a
reference-fidelity winner. Its damaged-image CLIP can be high while pixel agreement is substantially
lower than Rescue. This is a measured comparison to the other scoped models, not evidence that
7B is best among all generative architectures or precisions. CPU execution is not offered for this
function and is rejected before contacting ComfyUI; CPU speed is consequently unavailable.

## Visual inspection

I inspected reference/model contact sheets at both 2× and 4×, including baby/portrait, city, sand
and low-light street-sign crops, clean and damaged. RealPLKSR keeps clean edges and texture closer
to the reference. BSRGAN removes JPEG blocks/noise but smooths skin, sand and authentic grain.
SeedVR2 adds skin pores, replaces sand/pebble texture, invents some facade details and alters sign
lettering. These observations support retaining a separate Generative function and the two measured
restoration defaults. Inspection PNGs and all model outputs remain under
`target/refvec/enhance-eval/` (including `inspect-2x-clean.png`,
`inspect-2x-blur_noise_jpeg.png`, `inspect-4x-blur_noise_jpeg.png`).

## Speed, memory and download size

The CPU comparison uses the same CC0 baby-image center crop, 256×256 reference, 128×128 input,
requested 2× output, clean condition, cold model setup. It is a timing/memory probe, not a substitute
for the full 25-image GPU quality set. CPU RSS includes the preloaded CLIP bundle; GPU RSS is the
Comfy server process, so these memory domains should not be treated as bare weight allocations.

| Checkpoint | CPU seconds | CPU peak RSS MiB | RTX 2× clean mean / median seconds | RTX small-set server RSS MiB / GPU MiB | CPU / GPU download MB | Licence |
|---|---:|---:|---:|---:|---:|---|
| RealPLKSR 2× | 78.814 | 1461.5 | 0.475 / 0.349 | 1940.4 / 928 | 29.628 / 29.595 | MIT |
| RealPLKSR 4× | 18.573 | 1094.3 | 0.645 / 0.619 | 1960.9 / 928 | 29.711 / 29.678 | MIT |
| BSRGAN 2× | 199.327 | 2434.8 | 0.623 / 0.536 | 1967.9 / 1152 | 124.072 / 66.894 | Apache-2.0 |
| BSRGAN 4× | >20; canceled at 20.042 | 1180.3 at cancellation | 0.712 / 0.533 | 1951.3 / 2080 | 124.072 / 67.047 | Apache-2.0 |
| SeedVR2 7B FP16 | unavailable in this backend | — | 1.349 / 0.664 | 17934.8 / 17484 | — / 16981.909 incl. VAE | Apache-2.0 |

BSRGAN's official CPU package contains both native scales; it is one shared 124.072 MB download,
not two copies. The selected default uses only its 2× member. The 4× CPU time is a measured lower
bound after the necessary wider context correction, not its old faster, visibly discrepant tiler.
No completion time was extrapolated from canceled runs. GPU quality paths do not use the Rust tiler.
The initial RealPLKSR 2× quality run had an invalid memory-monitor PID: its 50 quality/timing rows
remain valid, memory was remeasured on two images with the correct PID (CSV zero means missing).
The first 4× RealPLKSR 2× RSS sample also captured unloading the previously loaded SeedVR2;
small-set memory above uses the uncontaminated 2× runs/repeat, not that transition peak.

### 24 MP and actual Compositing command

CPU Faithful 6000×4000 → 12000×8000 hit the 20-second deadline, returned typed Cancelled in
21.035 s with 0/651 tiles completed; process peak RSS 992.7 MiB, no major page faults or swaps.
It establishes that the existing CPU path misses the task's interactive threshold, without inventing
a full-job duration. The GPU core-node route was therefore implemented and measured.

| Actual ai.enhance command, 24 MP → 96 MP | Faithful | Rescue |
|---|---:|---:|
| Uniform controlled input, seconds | 26.277 | 40.461 |
| CC0 baby photo enlarged to 6000×4000, seconds | 35.071 | 45.246 |
| CC0 photo run client peak RSS MiB | 2364.3 | 2348.7 |

These timings include document composition, model service, transfer/decode, alpha restoration and
new document application. Source expansion happens before the timer. The enlarged CC0 photo is a
performance fixture, not a new quality reference. Model computation is content independent, but
PNG transfer/compression costs vary with content. A 96 MP output remains a tens-of-seconds job on
this implementation; no claim of <20 seconds is made. Small actual 192×128 → 384×256 commands
measured Faithful 5.804 s at fresh-server startup / 1.209 s later, Rescue 0.401 s. UI jobs use existing
background controls and remain cancelable. No desktop window was opened.

## Tiling and source conventions

The implementation follows [darktable's published model/export conventions](https://github.com/darktable-org/darktable-ai)
and [ComfyUI's core tiled-upscale implementation](https://github.com/Comfy-Org/ComfyUI/blob/master/comfy_extras/nodes_upscale_model.py).
CPU tiles are 512 (native 2×) and 256 (native 4×), with edge-extension padding, context discarded
before feathering and normalized accumulation. Original one-quarter context passes RealPLKSR 2×,
RealPLKSR 4× and BSRGAN 2× tiled/whole comparisons: RMSE 0.0456 / 0.0955 / 0.0307 levels, max 1.
BSRGAN 4× initially had max 26 levels despite low global RMSE. A GPU reference-harness sweep found
112-pixel context much better than 64/80/96; the corrected actual ONNX test now has **RMSE 0.0576,
max 1**, passing the unchanged RMSE<1/max≤6 criterion. This model-specific context choice is our
measured adaptation, not a claim copied from upstream. A spatial convolution fixture also checks
context and seams; nearest-neighbour alone would not expose these errors.

RealPLKSR is pinned to darktable HF revision `0bb636eeeed5fffd256571c3cc440d9fdf62467e`.
BSRGAN ONNX uses official darktable release-5.6.0; GPU weights use author's KAIR v1.0. All eight
CPU/GPU download specifications completed the application downloader's exact byte/SHA checks.
BSRGAN's CPU package is read in memory, stored/DEFLATE members bounded and named explicitly;
member names are never extraction paths. Existing SeedVR2 UNET/VAE were read-only, size and SHA
matched the inherited Comfy-Org pins. Their full hashes, other model pins and license provenance
are in the catalogue/specs and [notice](../../licenses/ai-enhance-NOTICE.md).

## Validation

Final `clippy --offline --all-targets -- -D warnings` passes for all eight changed crates
(`CARGO_TARGET_DIR=target/clippy`, Rust 1.98.1, CARGO_BUILD_JOBS=3). Formatting and
`git diff --check` pass. Attributions regeneration passes its full xtask consistency test.

The full targets of only the changed crates were run once. Three failures were repaired and
verified with targeted reruns, rather than repeating full suites: the old li-ai safetensors-only
catalogue assertion now permits only the vetted core-upscaler PTH/official-host case; li-seg's
six-model assertion now counts its original groups separately from the four upscale candidates;
and the new export UI test now clicks the real resize controls and checks the existing serialized
`resize.value` field. All other full-suite targets passed. **4,156 distinct ordinary tests are now
validated**, with 43 ordinary-run ignored tests; counts below combine full-run successes with those
three targeted repairs, without double-counting focused reruns.

| Changed crate | Validated passed | Ordinary ignored | Coverage |
|---|---:|---:|---|
| li-ai | 432 | 2 | 113 unit + 319 integration; catalogue fix targeted |
| li-seg | 48 | 5 | Unit/model tests; original-group count fix targeted |
| lightcraft-engine | 809 | 6 | 425 unit + 381 coverage + 1 backend + 2 panic-hunt |
| lightcraft-ui-egui | 231 | 3 | Full UI target + repaired actual-click export test |
| local-image | 81 | 2 | App adapter/settings tests; macOS-only target empty on Linux |
| photocraft-engine | 1473 | 16 | 845 unit + 582 coverage + 46 integration |
| photocraft-ui-egui | 1067 | 9 | 994 unit + 59 GPU + 14 integration |
| xtask | 15 | 0 | Includes regenerated-attributions consistency |

Additional opt-in checks passed: actual pinned ONNX BSRGAN 4× tiled/whole test (1), actual
RTX 5090 core-model test (1, all four 2×/4× checkpoints at odd 67×49 dimensions, 3.32 s total),
and the evaluation metric example test (1). The GPU test is newly added and explicitly rejects
non-CUDA/non-RTX hardware; CUDA output-size/nonempty checks completed for every candidate.
The other three ONNX tiled/whole comparisons passed before correcting the BSRGAN 4× discrepancy,
as detailed above. Model downloads were exercised through the real pinned application flow.

New UI tests click the Compositing dialog's method/scale/backend controls, Generate's three
methods/scale, every new Settings function row's alternate/custom/default controls and Library's
actual resize/method dropdowns. Focused final tests also revalidated the custom-ONNX execution,
alpha/output sizing, cancellation, download retry/cleanup, colour conversion and native export
rendering. The export regression now uses a small aspect-preserving native fixture and finishes
in 0.50 s, replacing unnecessary full-photo encodes; the corrected Library click test takes 0.34 s.
Doc targets pass (zero tests). Vulkan inventory identifies the discrete RTX 5090, NVIDIA driver
615.71.09; no software-adapter performance claim is used.

Full history logs: `target/enhance-full-final.log` (li-ai unit),
`target/enhance-ai-full-integration.log`, `target/enhance-full-remaining.log` (li-seg),
`target/enhance-full-last-six.log`; targeted fixes are `target/enhance-catalog-fix-test.log`,
`target/enhance-seg-count-fix-test.log`, `target/enhance-lc-ui-click-final.log`.
Final focused/lint/doc logs are `target/enhance-final-focused.log`,
`target/enhance-clippy-final.log`, `target/enhance-model-doc-tests.log`.
The focused log retains the superseded resize-field assertion failure; its final repaired pass
is the click-final log. The isolated test ComfyUI process was stopped after an empty-queue check.
No owner GUI application was opened, changed or stopped.

## Reproduction and artifacts

Use Rust 1.98.1, offline, PATH including ~/.cargo/bin and CARGO_BUILD_JOBS=3. One cargo build runs
at a time. `li-ai/examples/enhance_probe.rs` actions: `download`, `gpueval`, `cpueval`, `cpu24mp`.
Set `LOCAL_IMAGE_TEST_MODELS` to the isolated cache; GPU also requires `LOCAL_IMAGE_COMFY`.
For evaluation set `ENHANCE_EVAL` to the downloaded manifest PNG directory,
`ENHANCE_CANDIDATE` to one model id (or all), `ENHANCE_FACTOR` to 2 or 4,
`ENHANCE_COMFY_PID` to the actual isolated server PID, and `ENHANCE_OUTPUT` to a worktree directory.
CPU probe options: `ENHANCE_CPU_CROP=256 ENHANCE_CLEAN_ONLY=1 ENHANCE_LIMIT=1`;
`ENHANCE_TIMEOUT=20` records canceled timings. `baseline` measures the bicubic comparator.

`photocraft-engine/examples/enhance_real.rs` executes the actual command, checks the real RTX 5090
and empty queue and validates document dimensions. `24mp` chooses the 24 MP size,
`ENHANCE_METHOD=faithful|rescue` chooses function, and `ENHANCE_INPUT` optionally names a CC0
performance fixture. Set `LOCAL_IMAGE_SEG_MODELS` to the isolated model cache.

Development used filtered tests/checks only. Final lint and full-target history logs are listed above; the full suites were not repeated
after targeted assertion repairs. Real-model seam,
24 MP CPU and actual-app GPU logs use `target/enhance-real-seams-final.log`,
`target/enhance-cpu-24mp-final.log`, `target/enhance-app-gpu-photo-24mp.log`.
Build/test data and reference harnesses stay under target/refvec; optional verified named model
copies live in ~/.cache/local-image-test-models. Requested implementation is complete; known limits
are slow CPU inference, 8-bit restoration input precision, Generative GPU-only support, and the
controlled dataset's limited coverage. No owner photo or GUI application was changed or stopped.
