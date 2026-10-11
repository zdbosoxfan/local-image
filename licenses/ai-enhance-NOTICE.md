# Local Image AI Enhance / Upscale

RealPLKSR (Dongheon Lee et al.) is used for Faithful 2× and 4× enlargement.
Code and checkpoints: https://github.com/dslisleedh/PLKSR (MIT).
The ONNX exports and corresponding GPU checkpoints are published by darktable:
https://huggingface.co/darktable-org/upscale-realplksr-onnx .
Full upstream licence: `licenses/realplksr-MIT.txt`.

BSRGAN (Kai Zhang et al.) is used for Rescue 2× and 4× restoration/enlargement.
Code: https://github.com/cszn/BSRGAN (Apache-2.0).
Original GPU checkpoints: https://github.com/cszn/KAIR/releases/tag/v1.0 .
ONNX exports: darktable-org/darktable-ai, release-5.6.0, upscale-bsrgan.dtmodel.
Full upstream licence: `licenses/bsrgan-Apache-2.0.txt`.

Darktable's model conversion and packaging project is GPL-3.0:
https://github.com/darktable-org/darktable-ai . Local Image reads its published ONNX
exports and ZIP model packages. The Rust tiler follows the overlap/context conventions;
it incorporates no Python or C runtime in the application build.

The optional GPU route uses ComfyUI's core UpscaleModelLoader / ImageUpscaleWithModel
nodes (GPL-3.0, https://github.com/Comfy-Org/ComfyUI) and spandrel's main registry
(MIT, https://github.com/chaiNNer-org/spandrel). No extra architectures are required.

Model training provenance is documented in darktable's model cards. RealPLKSR uses
DF2K and realistic synthetic degradation; BSRGAN uses DIV2K, Flickr2K, WED and OST.
DIV2K's current official dataset page limits use to academic research; the other named
research datasets do not all carry explicit open-source licences. These datasets were
not used for Local Image's evaluation. The owner approved these named models on
2026-10-10. Checkpoint licences
are MIT / Apache-2.0; the weights are optional downloads and are not bundled.

SeedVR2 remains the existing Apache-2.0 Generative choice, with its original attributions.
