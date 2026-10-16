# AI Enhance / Upscale

Compositing Image → AI Enhance, Generate → Upscale and Library/Develop export
Image Sizing use `li_ai::Ai::enhance` through their existing background job controls.

- **Faithful**: RealPLKSR 2×/4×, clean sources, preserves detail.
- **Rescue**: BSRGAN 2×/4×, noise, JPEG artefacts and mild blur.
- **Generative (SeedVR2)**: the existing enhancement graph, may change textures.

Old Compositing settings default to Generative. Old export settings have no AI method
and continue using ordinary resizing. All new fields have serde defaults. English
strings are listed in the existing Compositing i18n pending list and the Library's
`PENDING_ENHANCE_TRANSLATIONS`; translations are paused by the owner.

## Runtime

CPU inference uses the published ONNX exports, tract 0.23.8, NCHW sRGB floats in
0–1. Native fixed tiles are 512 input pixels for 2× and 256 for 4×. Each tile has
one-quarter-tile context on each side, which is discarded. BSRGAN 4× uses
112 pixels of context per side in its 256-pixel tile: the ordinary 64-pixel
context failed the measured seam comparison. Cores overlap by a
quarter of their width and blend with separable linear feather weights, normalised
per output pixel. Border samples extend the edge. Small images are padded, never
resized to the model's input size. Cancellation is checked between tract operations,
tiles, blending rows and before publication. The accumulator is bounded to 400 MP.

Auto uses a ready ComfyUI core upscale model when available, otherwise the verified
CPU model. GPU explicitly requires that preset and core nodes; CPU never contacts
ComfyUI; Generative requires the GPU backend. The GPU route is UpscaleModelLoader → LoadImage → ImageUpscaleWithModel →
SaveImage, using spandrel's main registry. No custom architecture package is needed.
ComfyUI controls its own tile/overlap sizes and OOM retry.

Settings › Local AI adds three function rows to the existing on-device rows:
**Upscale Faithful**, **Upscale Rescue**, and **Generative**. Each describes its
function, selected model, size/licence, download controls, Change…, Custom model…
and Use default. Defaults are exactly one checkpoint per function: RealPLKSR 2×,
BSRGAN 2×, and SeedVR2 7B FP16 with its VAE. The 25-image CC0 GB82 comparison
selected these restoration defaults; see the report for both requested scales.

The selected checkpoint is independent of the requested enlargement (up to 4×).
Run it at its native scale, then resize to the exact requested size with Lanczos3.
Change… keeps all four restoration checkpoints available in either restoration
function. Custom fixed-shape NCHW RGB ONNX models (2×/4×) are validated and run on a
small sample before installation, copied under a hash-based filename, and checked
against their size/SHA before use. Generative allows installed compatible SeedVR2
UNET/VAE filenames. Selections persist in `enhance-functions.json` in the models
directory. Missing/malformed old settings use the single defaults; Use default
removes the override. The Generate advanced model picker passes an explicit
checkpoint override without altering Settings. Restore
source alpha and soft-edge colour using the existing SeedVR2 alpha⁴ convention.
Generative's internal target is even, then resized to exact export dimensions.

Export renders at the developed/cropped native size in sRGB8 before restoration,
then converts to the requested output primaries and curve and honours 8/16/32-bit
containers. AI restoration has 8-bit source precision. Sharpening and watermarking
remain after enlargement. Cancellation prevents partial export publication.

## Vetted downloads

All downloads use Local Image's existing HTTPS host allow-list, exact size and
SHA-256 checks, `.part` cleanup and atomic publication. They are explicit user actions.
CPU model specifications are in `li-seg::MODELS`; GPU files are in the RealPLKSR and
BSRGAN `li-ai/families` profiles. GPU `.pth` is accepted only for an Upscaler family
with the core-upscale pipeline and Upscaler file role. The application never reads
pickle; ComfyUI uses its safe weights loader.

RealPLKSR is pinned to darktable's Hugging Face commit
`0bb636eeeed5fffd256571c3cc440d9fdf62467e`. BSRGAN ONNX is pinned to darktable's
`release-5.6.0/upscale-bsrgan.dtmodel` (124072128 bytes,
SHA-256 `e90621e12fea12098ad6abd701bf67f50cea6c633bad98a19a4ccebba94404a6`).
The package includes both scales; the reader decompresses only the selected member
in memory and never treats member names as filesystem paths. BSRGAN GPU files are
from the author's KAIR `v1.0` release. All file sizes and hashes are in the specs.

Licences and provenance: [notice](../../licenses/ai-enhance-NOTICE.md), MIT
RealPLKSR and Apache-2.0 BSRGAN. Attributions are generated through the existing xtask.

## Verification

Focused tests cover spatial tiling against whole-image inference, sizes, rejection
of malformed model outputs/archives, typed cancellation, verified download retries,
core GPU graph routing/alpha and clicking all three UI surfaces. Opt-in tests use
real models and assert the real RTX 5090 through ComfyUI.

`li-ai/examples/enhance_probe.rs` downloads into an isolated test model directory and
measures the interactive service (`download`, `cpu24mp`, `gpu24mp`, `gpu`). The 24 MP
input is 6000×4000 and 2× output 12000×8000. CPU runs have a 20-second cancellation
budget to establish the GPU decision threshold, not an extrapolated completion time.
`gpueval` / `cpueval` compare real CC0 GB82 references, bicubic reduction and a
controlled blur/noise/JPEG degradation, with Y PSNR/SSIM and existing CLIP cosine.
The benchmark module uses no additional dependencies. CPU timing crops and
cancellation deadlines are explicit environment options, and never silently
replace the full-image GPU quality set. Results and exact test commands are in the task report.

Sources: [darktable model cards](https://github.com/darktable-org/darktable-ai/tree/master/models),
[packaging format](https://github.com/darktable-org/darktable-ai/blob/master/darktable_ai/package.py),
[ComfyUI core upscale implementation](https://github.com/Comfy-Org/ComfyUI/blob/master/comfy_extras/nodes_upscale_model.py),
[tract plan/evaluation API](https://docs.rs/tract-core/0.23.8/tract_core/plan/index.html).
