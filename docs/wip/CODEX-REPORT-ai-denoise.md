# AI Raw Denoise implementation and validation

Task: `CODEX-TASK.md`, branch `wip/codex-ai-denoise`. Continued the existing branch and recovered the CPU implementation from the parent of removal commit `a6f962c6`. No commits were made.

## Implementation

### CPU models and verified installation

`li-seg` restores tract inference and the previous ZIP extraction/gain-matching code. The current per-function model registry has two new slots: Raw Denoise (paired Bayer and linear models), and RGB Denoise. RawNIND uses static 512-pixel tiles; NIND uses its exported 768-pixel shape. The normal Settings downloader verifies the official release archive and each extracted ONNX member before installing them. Raw installation requires both members. Missing, partial, incorrectly sized, and incorrectly hashed installations are rejected.

All three ONNX files are from darktable-ai `release-5.6.0`; archive and member SHA-256 and sizes are pinned in `li-seg`. Approved test downloads used `li-ai::download::download_file` in `/tmp/local-image-denoise-vetting`, followed by the same bundle installer used by Settings in a temporary normal model directory. Real-model test copies live under `~/.cache/local-image-test-models/denoise/segmentation`.

### GPU execution

`lc-gpu` decodes the existing ONNX protobuf with tract and validates the supported FP32 graph. WGSL implements Conv, ConvTranspose, LeakyReLU/ReLU, Sigmoid, MaxPool, two-input channel Concat, and CRD/DCR DepthToSpace. Embedded weights and intermediate tensors stay resident across tiles. The GPU runner serializes use of its resident buffers. The engine caches the active model, uses the physical wgpu adapter, and retries a failed GPU image through tract; cancellation does not retry.

Both backends use the same reflected-context, overlapping, feathered tiler. Raw output gets one sign-preserving scalar gain over the image. Bayer gain is measured at matching RGGB photosites, so its twice-sampled green channel does not change exposure when converting to three-channel RGB; no independent channel gains alter WB. RGB output follows the exported NIND sigmoid contract.

### Develop, worker, and exports

Develop › Detail contains AI Denoise, Auto / Raw / RGB, Strength, Preview, Apply AI Denoise, model download, and Denoise to new DNG/TIFF. The Enhance worker handles model verification/loading, source decoding, inference, and result encoding/writing. The preview inference input is at most 1024 pixels on its longest side. Full Bayer inference replaces demosaic: normalized un-white-balanced R/G1/G2/B sensor samples become camera RGB, then DNG post-demosaic correction opcodes and the existing camera matrix/profile and as-shot WB are applied once, followed by crop/orientation. The correction-order test uses the existing raw developer as its pixel oracle. X-Trans/non-Bayer uses the white-balanced linear Rec.2020 path. NIND operates on sRGB-encoded RGB, converted back to the working space afterwards.

The non-destructive result is photo-bound in the existing AI store and enters at the decoded source before pipeline geometry, tone, detail, and masks. Strength mixes the cached result, without another inference. Completion is undoable and ignores results for a changed source. Copying settings/presets does not apply another photo's stored result. Defaults and forgiving deserialization allow pre-denoise settings and obsolete malformed AI references to load.

The new-file worker writes an atomic, non-overwriting 16-bit sRGB TIFF with ICC and capture metadata or floating-point linear Rec.2020 DNG. DNG has no CFA, neutral WB, explicit color/forward matrices, and normal orientation; the original camera WB is not attached to already-white-balanced working RGB. Its camera identity becomes “Local Image / Denoised linear Rec.2020” to prevent automatic assignment of the original camera's DCP on re-import. Capture metadata remains. The test reloads this DNG through the actual app decoder and checks its pixels. A preview result cannot be used for a normal full export: Apply runs the full source first. New-file denoising always runs the full source. English UI strings are recorded in the pending translation lists.

## Evaluation protocol and data permissions

This scope owns **Raw Denoise** and **RGB Denoise**. Their rows extend the existing on-device Settings rows from `3764aa80`; the other restoration functions belong to the other jobs. RawNIND is one default bundle with the Bayer architecture for Bayer sensors and its required linear architecture for other sensors/fast previews. RGB has one default, NIND UNet. Change… offers the installed RawNIND linear candidate and the existing custom ONNX flow; Use default returns to the single registered default. This does not restructure Settings.

The local paired evaluation follows the reference-pair approach in [RawNIND JDDC](https://github.com/trougnouf/rawnind_jddc) and [NIND](https://github.com/trougnouf/nind-denoise). It uses two predetermined 768×768 native-resolution crops in each of three scenes, omitting 64-pixel context borders. PSNR is `-10 log10(MSE)` over display-sRGB RGB samples with range 1. SSIM follows [Wang et al., 2004 / ssim_index.m](https://ece.uwaterloo.ca/~z70wang/research/ssim/): 11×11 Gaussian σ=1.5, K1=.01, K2=.03, valid windows, averaged across RGB channels at native resolution. This explicitly differs from the webpage's grayscale/downsampled viewing-distance variant. Supplemental CLIP cosine compares the existing normalized LAION ViT-B/32 embeddings, using the app's center-crop implementation; it measures semantic similarity, not LPIPS or fine-noise fidelity.

One reference exposure gain is determined from the clean/noisy pair and shared across every candidate. Integer alignment searches ±24 pixels in the input/reference green channel; Canon/Fuji offset (0,0), Sony (0,-1). Camera/DCP/profile conversion uses the same source implementation for Bayer and linear candidates. There is no candidate-specific exposure fitting, geometric alignment or test-time parameter tuning. These are local subset measurements, not a reproduction of the authors' published leaderboard: their preprocessed alignment/masks and training exposures differ. Only two Bayer scenes are used; broad superiority is not established.

The table below establishes RawNIND reservations for Canon/Sony and NIND's reservation for Fuji. Canon/Sony training overlap for NIND is unverified, so the RGB comparison is not represented as an entirely held-out NIND benchmark.

The [RawNIND Dataverse dataset](https://dataverse.uclouvain.be/dataset.xhtml?persistentId=doi:10.14428/DVN/DEQCIM), DOI `10.14428/DVN/DEQCIM`, publishes its archive under **CC BY-SA 4.0**; its `permissions.txt` also gives per-image permissions. Local copies were checked against the API's byte counts and MD5s; the filenames also contain original SHA-1s. No owner photos were used.

| Scene / reference and noisy ISO | Dataverse file IDs | Permission and attribution | Hold-out evidence |
|---|---|---|---|
| Canon EOS 7D, TEST_7D-2, 100 / 6400 | 30549 / 30043 | CC0, Peter Wemmert (`permissions.txt`) | RawNIND `config/test_reserve.yaml` |
| Sony A7C, TEST_TitusToys, 50 / 6400 | 31179 / 30136 | Dataset CC BY-SA 4.0, Benoit Brummer / Christophe De Vleeschouwer | RawNIND `config/test_reserve.yaml` |
| Fuji X-T1, stefantiek, 200 / 6400 | 30538 / 29660 | CC0, Benoit Brummer (`permissions.txt`) | NIND `test_reserve.yaml`; not a RawNIND hold-out claim |

Raw reference data, permissions, API metadata, logs and PNG results remain under `target/ai-denoise/eval` and `target/ai-denoise`; they are not application assets. The native 24 MP timing photo is the official darktable-ai `DSC04028.ARW` sample; `1D3_6400.CR2` is also used for the requested darktable comparison. Both are distributed in [darktable-ai's sample folder](https://github.com/darktable-org/darktable-ai/tree/f1a46e2d3feebc917a94f14705d128316fcf8114/samples/rawdenoise/bayer), a GPL-3.0 repository; no separate per-photo licence notice was found. They have no clean reference and are not used to choose the default. The native 50 MP Sony ILCE-1 sample is [raw.pixls.us file 4465](https://raw.pixls.us/getfile.php/4465/nice/Sony%20-%20ILCE-1%20-%2014bit%2014bit%20compressed%20(3:2).ARW), CC0, SHA-256 `ddb72880da333b46a4c13286f78473dacc11377ea874bc3656e64e30a8122a88`. They are decoded at their original dimensions, without resizing to simulate megapixels.

## Measured model choices

Quality numbers below are means of crop scores (not PSNR of pooled MSE). Speed is median per 768×768 crop call, including model switching/loading, with the physical NVIDIA GeForce RTX 5090 (Vulkan, driver 615.71.09) and Ryzen 7 9800X3D CPU. The desktop and other jobs remained running; these are observations on the shared machine, not exclusive-host claims. No ComfyUI jobs were submitted by this worktree.

### Raw Denoise

The first two scenes give four held-out Bayer crops:

| Candidate | PSNR dB ↑ | SSIM ↑ | CLIP cosine ↑ | RTX 5090 seconds ↓ | CPU seconds ↓ | Installed weights / licence |
|---|---:|---:|---:|---:|---:|---|
| **RawNIND Bayer (chosen Bayer path)** | **39.458** | 0.905338 | 0.949468 | **0.083** | **2.077** | 31.06 MB, GPL-3.0 |
| RawNIND linear after demosaic | 37.679 | 0.903826 | **0.957206** | 0.360 | 6.329 | 31.05 MB, GPL-3.0 |
| NIND RGB after demosaic (comparison) | 38.749 | **0.912848** | 0.929656 | 0.776 | 31.935 | 59.14 MB, GPL-3.0 |
| Existing wavelet NR, luminance 50 / colour 25 | 34.169 | 0.791785 | 0.923351 | See Develop timing below | 0.091 | No weights |
| No denoise | 26.274 | 0.506888 | 0.850967 | — | — | — |

RawNIND Bayer beats the linear candidate by 1.78 dB over these four crops and runs about four times faster in the measured crop calls. The Canon dark scene favours linear slightly; the Sony scene favours Bayer strongly. The Bayer default therefore follows the measured fidelity/speed trade-off, rather than assuming the joint model is better. The linear architecture remains part of the same required RawNIND bundle for X-Trans/non-Bayer and fast previews, and can be selected explicitly with Change…. Across the deployed Bayer/Fuji combination, mean PSNR is 40.771 dB and SSIM 0.929341. The default bundle is 62.11 MB installed, 57.70 MB download, GPL-3.0.

The gain investigation matters: matching a packed four-channel Bayer mean to an unweighted three-channel RGB mean shifted brightness. The sampling-domain scalar fix increased Sony PSNR from 32.92 / 31.74 dB to 46.40 / 45.89 dB without reference-based fitting. A coloured synthetic negative-scale test verifies preserved exposure/channel ratios. This is an intentional improvement over darktable's current RGB-mean formula, documented independently rather than attributed to upstream.

### RGB Denoise

Six rendered RGB crops from Canon, Sony and Fuji:

| Candidate | PSNR dB ↑ | SSIM ↑ | CLIP cosine ↑ | RTX 5090 seconds ↓ | CPU seconds ↓ | Installed weights / licence |
|---|---:|---:|---:|---:|---:|---|
| **NIND UNet (chosen default)** | **40.389** | **0.935997** | 0.937233 | 0.776 | 32.817 | 59.14 MB, GPL-3.0 |
| RawNIND linear (selectable candidate) | 39.585 | 0.928334 | **0.963253** | **0.348** | **6.329** | 31.05 MB, GPL-3.0 |
| Existing wavelet NR, luminance 50 / colour 25 | 36.235 | 0.842998 | 0.934615 | See Develop timing below | 0.090 | No weights |
| No denoise | 28.382 | 0.576120 | 0.854392 | — | — | — |

NIND is selected for higher PSNR and SSIM on the local reference pairs. RawNIND linear is faster, smaller and scores better on the supplemental semantic metric; it stays selectable. CLIP is not a fine-detail/noise metric and is not used to override the reference-pixel results. Visual review found NIND smoother, including softened Canon fine detail, and RawNIND linear more textured. Neither wins every crop. NIND's download is 55.01 MB; it is the only RGB default.

### Memory per candidate

CPU peak RSS is from an isolated process loading one model and running one native static tile, `/usr/bin/time -v`; it includes tract, weights, scratch and process overhead. GPU resident bytes count the actual allocated FP32 input/intermediate/weight/bias buffers, excluding driver/device overhead and source-image tiling buffers. These are different quantities, labelled separately rather than treating global desktop VRAM as ours.

| Candidate | CPU tile process peak RSS MiB | Resident GPU graph MiB | Native tile |
|---|---:|---:|---|
| RawNIND linear | 297.15 | 718.60 | 3×512×512 |
| RawNIND Bayer | 298.66 | 740.60 | 4×512×512 → 3×1024×1024 |
| NIND RGB | 1114.38 | 2880.14 | 3×768×768 |

Logs: `memory-{linear,bayer,rgb}-cpu.log` and `gpu-real-final.log`. The graph retains all intermediates; reducing that residency is an optional optimization, not required for correctness on the measured card.

## Native performance and real app interaction

| Native source | RawNIND Bayer RTX 5090 seconds | RawNIND Bayer CPU seconds | Wavelet complete Develop GPU / CPU seconds | RawNIND linear GPU seconds | 1024-pixel linear GPU preview seconds |
|---|---:|---:|---:|---:|---:|
| Sony sample, 6048×4024, 24.337 MP | 5.639 | 73.344 | 0.236 / 4.923 | 7.117 (6000×4000 crop) | 0.240 |
| Sony ILCE-1, 8672×5784, 50.159 MP | 6.457 | 136.933 | 0.955 / 8.810 | 12.281 (8640×5760 crop) | 0.229 |

Bayer timings include verification/model loading and the entire tiled inference/gain step, excluding source decode/file writing. They use 48 / 96 packed-space tiles. The 24 MP GPU call had a 3.3-second first-tile startup, while the later 50 MP run benefited from warmed driver shader caches. These single native runs are not a claim that doubling resolution only costs 0.8 seconds. Wavelet numbers time a real full-resolution Develop render including other normal stages, excluding source decoding; quality comparisons use its direct NR operation at luminance 50 / colour 25. The tract fallback remains much slower than the GPU.

Whole-image Bayer CPU/GPU float outputs are byte-identical at both sizes: SHA-256 `9f96bbbfba4dcaadafd403c6835ee28c8af7a49abb6eb7a33fa916b6cb8ea33f` (24 MP), `aa9d1203e26eaa852492a01355560896e42beae16628f626261e8585925a2f82` (50 MP). CPU Bayer processes peaked at 923 / 1599 MiB RSS. The GPU benchmark processes, also running wavelet and full linear inference, peaked at 1905 / 3165 MiB RSS and 1494 / 3830 MiB process-specific VRAM. CPU wavelet whole Develop processes peaked at 2056 / 4270 MiB RSS. `native-{24,50}-{gpu,cpu}.log/.json` and `wavelet-{24,50}-cpu.log/.json` preserve the commands and measurements; other jobs' global VRAM is excluded from our process figures.

The actual Library/Develop headless harness imports the official raw, clicks Preview, waits for the real worker, and drags Strength six times with GPU rendering. It uses an in-memory session with sidecar writing disabled and opens no window. The physical adapter was NVIDIA GeForce RTX 5090 (Vulkan). Preview click processing was **2.36 ms**, the job completed in **0.268 s**, drag processing was **1.94–3.20 ms**, and settled render completion was **53.06–62.94 ms**. This measures the app command/worker/render path, not a standalone inference approximation. Logs: `ui-real-final.log`. The ordinary UI test also clicks model selection, Preview, Apply, new-file export, and undo; Settings tests click Change…, choose a tested custom model, and Use default.

## Actual darktable comparison and visual review

The isolated out-of-build ctypes harness loads the **official darktable 5.6.2** `libdarktable.so` and its ONNX Runtime, invokes `dt_restore_raw_bayer` with original ADC-valued pixels and model config, and captures the actual returned CFA. AppImage SHA-256 is `4b0d1c737a2a18c7d8afb81aa3daaf25930bbe541dda98e6354dabd5c2e4dc36`. It uses a temporary config/cache, an in-memory library, `write_sidecar_files=never`, and CPU inference. No C/Python reference code or ONNX Runtime enters the application build. The returned CFA is rendered through Local Image's same original-source camera transform for comparison; this is **not** a claim to reproduce darktable's entire tone/color history stack.

| Official sample | Local Image Bayer GPU seconds | darktable Bayer CPU seconds | CFA agreement PSNR R / G / B dB |
|---|---:|---:|---|
| DSC04028.ARW | 5.639 (cold; above) | 5.789 (repeat; first 6.715) | 40.085 / 38.441 / 45.502 |
| 1D3_6400.CR2 | 1.699 | 3.748 | 40.881 / 37.569 / 47.280 |

These PSNRs measure agreement between programs, not quality against ground truth. Their preprocessing differs: our existing Sony decoder uses white=16320, whereas darktable reports 15360; compressed Sony sensor decoding differs in 18,272,339 / 24,337,152 samples, maximum 221 ADC units. Canon's original sensor samples are identical. Local Image uses a whole-image photosite-domain gain, reflected halo/feathering and direct camera RGB; darktable uses per-tile RGB-mean gain and remosaicing. Consequently an exact darktable output match is not claimed. Diagnostic logs confirm `wb_norm=[1,1,1]` with these pinned model attributes.

I inspected the paired reference/candidate PNGs and both official sample outputs. NIND reduces fine noise more aggressively and softens Canon texture; linear RawNIND retains more texture. Bayer closely follows the Sony reference fur and keeps Canon figurine contours with less noise than wavelet. Darktable's remosaiced Canon result has visible fine patterned detail on the yellow belt which is weaker in the direct RGB output. In the extremely underexposed Sony sample, Local Image retains more purple shadow colour than darktable; remosaicing our output preserves it, so it is not explained by direct RGB versus CFA rendering alone. The source decoder/normalization and tiler differences remain a comparison limitation. It is not evidence of a repeated as-shot WB: input WB is neutral, one camera transform is applied, and neutral exported DNG reloads are pixel-tested. No reference-based per-channel correction was added to conceal this difference. Broader extreme-shadow evaluation is an optional follow-up; the licensed paired-set measurements above determine the shipped defaults.

## Validation

Filtered development checks passed: 11 li-seg denoise tests; 7 engine denoise tests (all CFA phases, source binding/strength, preview/full exports, DNG pixel reload, Auto RGB encoding, post-demosaic corrections/crop/orientation); 2 GPU operator tests; actual Develop UI clicks; actual Settings custom/default clicks and seven-row layout; and the independent SSIM identity/known-constant test. The explicit ignored real-model GPU test ran successfully on the 5090: linear/Bayer tile max and RMS errors were zero; NIND max `1.2e-7`, RMS `2e-8`. Assertions require max below `2e-4 × max(peak,1)` and RMS below `2e-5 × max(peak,1)`, necessary for RawNIND's arbitrary raw scale. Whole-image gain-corrected outputs additionally matched exactly as above.

Final `clippy --all-targets -- -D warnings` passed for all seven changed code crates plus `li-attributions`, whose generated asset changed. Formatting and `git diff --check` passed; generated-attributions freshness passed (1 test). All cargo work used offline Rust 1.98.1 and three build jobs. No whole-workspace suite ran.

The full suites of these eight crates ran **once**, including integration tests and doctests. The first build was interrupted before any test execution to include the DNG correction-order fix. The actual full test run used `--no-fail-fast` and completed every requested crate. Counts from that run:

| Crate | Passed in full run | Initial failures | Ignored in full run | Follow-up |
|---|---:|---:|---:|---|
| li-attributions | 4 | 0 | 0 | Generated asset current |
| li-seg | 55 | 0 | 5 | Real-model evaluation separately above |
| lightcraft-develop | 96 | 0 | 0 | — |
| lightcraft-engine | 814 | 0 | 6 | Includes the DNG corrections/crop/orientation oracle |
| lightcraft-gpu | 92 | 0 | 6 | Explicit real-model ignored test also passed on 5090 |
| lightcraft-ui-egui | 229 | 2 | 4 | Both failed existing slider tests passed serially |
| local-image | 81 | 0 | 2 | — |
| photocraft-ui-egui | 1064 | 1 | 9 | All 16 affected Settings tests passed after fixture fix |
| **Total** | **2435** | **3** | **32** | **2438 distinct non-ignored tests verified passing after targeted follow-up** |

The existing CPU/GPU slider tests toggle the same process-global GPU enable flag. In the parallel full run, the GPU test observed it disabled and the CPU test observed GPU stages; these were backend-state assertions, not latency thresholds. Rerunning only `tests_slider_speed::develop_slider_` with `--test-threads=1` and `LC_REQUIRE_GPU=1` passed both, with the physical Vulkan 5090 and no GPU fallback. No assertions were weakened. The Settings failure came from the added rows placing the legacy-removal button below its fixed 1100-pixel test viewport; its fixture is now 1600 pixels high (the application already uses its existing scroll area). All 16 `ai_ui::tests` passed, including actual removal, new-model selection and default restoration. Full suites were not repeated. Logs: `full-tests-final.log`, `full-test-counts.json`, `slider-rerun-final.log`, `settings-rerun-final.log`, `clippy-final.log`.

The full command was `cargo +1.98.1 test --offline --no-fail-fast -p li-seg -p lightcraft-gpu -p lightcraft-develop -p lightcraft-engine -p lightcraft-ui-egui -p photocraft-ui-egui -p local-image -p li-attributions`. Lint used the same package list and `--all-targets -- -D warnings` under `target/clippy`. Existing missing-font, optional NVVM-probing and shared test-runner diagnostics appear in logs; the physical denoising validation above is independent of those probes and did not skip for a missing/software adapter.

No required implementation or validation remains. Optional follow-up work is a larger held-out RGB evaluation, repeated exclusive-host timing, lower GPU intermediate residency, faster tract fallback, and further extreme-shadow colour comparisons. The limited cohort, unverified NIND overlap and Sony darktable colour differences are disclosed above. Changes remain uncommitted; owner photos, owner app/configuration and shared compiler/test gates were not modified.

### Reproduction

All cargo invocations use Rust 1.98.1, offline mode and `CARGO_BUILD_JOBS=3`, with `~/.cargo/bin` on PATH. Lint uses `CARGO_TARGET_DIR=target/clippy`. Model selection/evaluation itself has no network or external runtime dependency once the verified bundles are installed.

- `cargo +1.98.1 build --offline -p lightcraft-engine --examples` builds the Rust measurement tools.
- `denoise_compare MODEL_DIR EVAL_DIR gpu|cpu test-canon|test-sony|fuji` reads the six named clean/noisy files listed above, saves comparison PNGs and prints per-crop PSNR/SSIM/timing. Run each backend and scene sequentially.
- `denoise_perceptual EXISTING_CLIP_MODEL_DIR EVAL_DIR` prints the supplemental CLIP scores, using the already-installed verified CLIP bundle.
- `denoise_bench RAW MODEL_DIR gpu|cpu 0 OUTPUT_DIR all|bayer-only|wavelet-only` runs the native-resolution measurement. `0` preserves the native sensor dimensions.
- `/usr/bin/time -v denoise_bench model MODEL_DIR linear|bayer|rgb cpu` measures isolated CPU model/tile process memory.
- `LOCAL_IMAGE_DENOISE_MODELS=MODEL_DIR cargo +1.98.1 test --offline -p lightcraft-gpu --test denoise ai_denoise_real_models_match_tract_on_5090 -- --ignored --nocapture` runs all three pinned graph comparisons.
- `LOCAL_IMAGE_DENOISE_RAW=RAW LOCAL_IMAGE_DENOISE_MODELS=MODEL_DIR cargo +1.98.1 test --offline -p lightcraft-ui-egui ai_denoise_real_app_preview_and_strength_latency -- --ignored --nocapture` runs the actual headless app interaction measurement.

Example executable names above refer to `target/debug/examples/`. Dataset API metadata/permissions and complete measurement commands/logs remain in `target/ai-denoise`; the isolated official darktable reference harness/output is in `target/refvec/ai-denoise`.

## Sources and decisions

- [darktable-ai RawNIND model, manifest and demo](https://github.com/darktable-org/darktable-ai/tree/f1a46e2d3feebc917a94f14705d128316fcf8114/models/rawdenoise-nind): static shapes, RGGB packing, no input WB for this Bayer export, linear Rec.2020, and scalar gain matching.
- [darktable-ai NIND model](https://github.com/darktable-org/darktable-ai/tree/f1a46e2d3feebc917a94f14705d128316fcf8114/models/denoise-nind): RGB encoding and exported graph.
- [ONNX ConvTranspose](https://onnx.ai/onnx/operators/onnx__ConvTranspose.html), [DepthToSpace](https://onnx.ai/onnx/operators/onnx__DepthToSpace.html): IOHW transposed weights and CRD/DCR channel ordering.
- [darktable raw Bayer implementation](https://github.com/darktable-org/darktable/blob/20891f6c6fa7be995e5bf9dff9d1ee5062051773/src/common/ai/restore_raw_bayer.c): CFA phase and gain/WB conventions. Local Image keeps camera RGB rather than remosaicing to CFA, then uses its established camera transform.
- [darktable neural restore documentation](https://docs.darktable.org/usermanual/development/lo/module-reference/utility-modules/shared/neural-restore/) and [WB discussion](https://discuss.pixls.us/t/introducing-neural-restore-module-raw-denoise-denoise-and-upscale/57349): new-file workflow and reported WB metadata problems.
- Existing `lc-pipeline` source/render architecture, `lc-engine::enhance` jobs/store, `files::load_bytes_with`, `camera_preview`, codec ICC writer, DNG writer, and `lc-catalog::safe_file::write_new` define the integration and persistence behavior.

No external dependencies were added. The GPU crate adds a dependency on the existing workspace `li-seg`. No unsafe Rust or C/C++ enters the build. Any reference binaries/harnesses are isolated under `target/refvec/ai-denoise`. Models/code are GPL-3.0; training data licence information is recorded in `licenses/darktable-ai-NOTICE.md`, `docs/PORTS.md`, and generated attributions.
