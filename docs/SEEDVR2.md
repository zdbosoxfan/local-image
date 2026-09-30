# SeedVR2 base 7B upscaling

The selected upscaler is **SeedVR2 7B FP16 base**, using native ComfyUI nodes. It restores an existing image in one diffusion step; it does not need a text encoder, prompt enhancer or custom-node package. The base checkpoint was selected to avoid the additional sharpening of the separate Sharp preset. [ComfyUI guide](https://docs.comfy.org/tutorials/utility/seedvr2), [official ComfyUI weights](https://huggingface.co/Comfy-Org/SeedVR2)

SeedVR2 synthesizes detail. The publisher reports that lightly degraded inputs can become overly detailed or sharpened. Enlarging an image therefore requires visual review, especially for lettering, faces and fine repeated patterns. The model's success at producing a large PNG alone does not establish faithful restoration. [Publisher limitations](https://github.com/ByteDance-Seed/SeedVR#-notice)

## Pinned download

Both files use immutable Comfy-Org revision `df48879708206a403d2a61acd55578c2e80fd233`:

| Folder / filename | Bytes | SHA-256 |
| --- | ---: | --- |
| `diffusion_models/seedvr2_7b_fp16.safetensors` | 16,480,583,960 | `2742ca6fee63bc5cc1773f426dd4b07b78cad27f51c9ea5cd42b035e6b592252` |
| `vae/seedvr2_ema_vae_fp16.safetensors` | 501,324,814 | `20678548f420d98d26f11442d3528f8b8c94e57ee046ef93dbb7633da8612ca1` |

The complete preset requires **16,981,908,774 bytes**, approximately 16.98 GB of disk space. This is separate from runtime GPU memory. The alternative VAE filename `ema_vae_fp16.safetensors` identifies the same published model. The app catalog uses the unambiguous SeedVR2-prefixed name.

Developer download through the app's size- and checksum-verified pipeline:

```powershell
python scripts/download_seedvr2.py --models-dir "D:\AI Models"
```

Existing files are verified and preserved. The downloader does not install custom nodes or alter ComfyUI. SeedVR2 is licensed under [Apache 2.0](https://github.com/ByteDance-Seed/SeedVR/blob/main/LICENSE).

## Workflow basis

The [native 7B image template](https://github.com/Comfy-Org/workflow_templates/blob/main/templates/utility_seedvr2_7b_int8_upscale_image.json) is the graph reference, with its INT8 model replaced by the selected FP16 base file. Images are resized with Lanczos, preprocessed, encoded through the SeedVR2 VAE, sampled once with Euler/simple at guidance 1, decoded, and postprocessed against the resized source. The template uses tiled VAE encode/decode with 512-pixel tiles and 128-pixel overlap. Its default color correction is `none`.

The app uses `lab` color correction and offers SeedVR2 as an optional enhancement, separate from the Image Gen model picker. See [Draft & Refine](IMAGE-GENERATION.md#draft--refine-and-optional-upscaling) and [the generation library](GENERATION-LIBRARY.md) for the controls.

## Measured quality and limits

The FP16 base model enlarged a generated portrait from **2048 × 1152 to 3840 × 2160 in 13.812 seconds** on the validation PC's 32 GB RTX 5090. The source file remained unchanged. Matched 100% crops show a substantial apparent-detail improvement over Lanczos in eyelashes, fingers, linen weave and ceramic edges, with the overall face, pose and object layout retained.

The improvement comes with synthesized detail: pores, wrinkles, textile threads and glaze mottling are not guaranteed to represent the source accurately. Dark lettering gains a thin outline/halo, malformed lettering in the source stays malformed, and tiny stray marks can become more character-like. Review important faces and text at full resolution before exporting. This is accepted as **optional photo enhancement**, not exact restoration.

A transparent origami fox enlarged from **1024 × 1024 to 2048 × 2048 in 5.593 seconds**. Its alpha exactly matches a Lanczos resampling of the source alpha. On a black background, the original light fringe remains; paper detail changes and some fine folds become smoother. Upscaling preserves the supplied matte and does not repair its edges. The app blends source RGB back at translucent boundaries to avoid amplifying edge contamination.

These are individual runs on one generated portrait and one generated transparent asset. They do not establish quality for all photographs or subjects. The **32 GB hardware recommendation** is a planning allowance backed by this actual 4K run, not a measured peak or minimum. There was no peak-memory measurement. Evidence: [portrait timing](../qa-artifacts/v05/seedvr2-quality/portrait-4k.json), [transparent timing](../qa-artifacts/v05/seedvr2-quality/transparent-fox.json), [independent observations and crop coordinates](../qa-artifacts/v05/seedvr2-quality/visual-review.json), and [the release validation report](LOCAL-IMAGE-VALIDATION.md).

## Native processing details

Preprocessing discards alpha and pads only the bottom/right edges to multiples of 16. Postprocessing crops the generated RGB back to the resized reference, restores its alpha, then crops odd dimensions to even values. Local Image therefore requires even target dimensions, with a shorter edge of at least 2 pixels, and preserves the resized source alpha explicitly. These are constraints of the native nodes; the app adds no 4096-pixel or 16-megapixel ceiling. The nodes report no maximum output size, so available memory determines what can run. This retains cutout geometry; it does not generate a more accurate matte. Single images need no temporal padding. [Native processing implementation](https://github.com/Comfy-Org/ComfyUI/blob/master/comfy_extras/nodes_seedvr.py)

The node's own color-correction default is `lab`, intended to retain source colors while preserving generated detail; the template overrides it to `none`. For the fidelity evaluation, `lab` is the recommended first comparison. `wavelet` transfers low-frequency color and `adain` matches global channel statistics. These methods cannot guarantee exact source colors or prevent invented details.

The template's VAE tiling reduces encoding/decoding memory. It does not spatially tile the diffusion model itself, so a successful small output does not establish that a 4K image fits the same GPU. Transparent edges, repeated textures, small lettering and facial features need direct review at full resolution.
