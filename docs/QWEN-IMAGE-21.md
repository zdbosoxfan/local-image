# Qwen Image 2.1 in Local Image

The adapter uses the native ComfyUI Qwen Image 2.1 nodes for transparent cutouts,
image edits, and empty background generation. No custom node pack is required.
The current ComfyUI service must expose `TextEncodeQwenImage21`; updating only the
frontend is insufficient. The application checks the live `/object_info` inventory
before submitting a job and reports missing nodes or model files.

## Model presets

Both presets use the same 7B image model. Compact is an INT8 quantization; Full
precision is BF16, not a separate larger-parameter model. The model selector does
not silently substitute another precision when the selected files are missing.

| ComfyUI folder | Compact INT8 | Full precision BF16 |
| --- | --- | --- |
| `models/diffusion_models` | `qwen_image_2.1_int8_convrot.safetensors` (7.26 GB) | `qwen_image_2.1_bf16.safetensors` (14.2 GB) |
| `models/text_encoders` | `qwen3vl_8b_int8_convrot.safetensors` (9.35 GB) | `qwen3vl_8b_bf16.safetensors` (17.5 GB) |
| `models/vae` | `qwen_image_2.1_vae_bf16.safetensors` | Same VAE |

These are download sizes, not VRAM requirements. ComfyUI can offload between VRAM
and system memory; actual memory and speed depend on image size, precision,
hardware, cache and reference count. Subfolders and `extra_model_paths.yaml` work
because loader filenames come from the running service. Prompt enhancer models
are optional and are not needed by this integration.

The app's **Download selected Qwen** action installs only the selected preset and
the shared VAE into the model folder configured in Settings. Downloads pin the
publisher revision and validate byte counts and SHA-256 hashes before publishing
each file. Existing matching files are verified and reused; conflicting files are
preserved and reported. Progress survives closing the Settings panel. Completed
files remain usable after an interrupted download; unfinished downloads can be
started again. The action does not restart ComfyUI. Refresh the Qwen connection
after completion so its live loader inventory determines availability.

Official files: [diffusion models](https://huggingface.co/Comfy-Org/Qwen-Image-2.1/tree/main/diffusion_models),
[text encoders](https://huggingface.co/Comfy-Org/Qwen-Image-2.1/tree/main/text_encoders),
[RGBA VAE](https://huggingface.co/Comfy-Org/Qwen-Image-2.1/tree/main/vae).

## API graph and generation behavior

The graph loads the UNet, Qwen image text encoder and RGBA VAE, then runs
`TextEncodeQwenImage21`, `KSampler`, `VAEDecode` and PNG `SaveImage`. The optional
`QwenImage21Cache` uses automatic placement and lossless default cache precision.
Defaults match the official templates: 25 steps, Euler, simple scheduler, CFG 1.

For edits, the encoder sees image references through flattened API inputs such as
`images.image_1`. It receives the VAE and its output slot 2 supplies the sampling
latent, preserving alignment with the first reference. `LoadImage` separates
transparency; `JoinImageWithAlpha` restores it before encoding. References are
rounded to multiples of 32, capped around 4 megapixels, and the result is resized
back to the original source dimensions. Generation uses `EmptyLatentImage` with
the requested canvas size. References are local temporary files uploaded through
the existing ComfyUI client, not absolute filesystem references in the server.

Native alpha is retained through decode, download and PNG export. Cutout results
that lack usable transparent and visible pixels are rejected. Qwen is a generative
editor and can redraw the subject; using its alpha on the existing source RGB
preserves original product pixels. The cutout editor uses this approach.

Live tests found one-level VAE quantization residue: large transparent areas
contained alpha 1 and opaque foreground pixels sometimes contained alpha 254.
After any output resizing, cutouts snap only alpha 0–1 to 0 and 254–255 to 255.
All soft alpha values from 2 through 253 remain unchanged, including fine hair and
partially transparent edges. Ordinary edit output receives no alpha cleanup.

For opaque Image Gen output, Qwen's RGBA result is composited over **white**.
Only alpha 254–255 is first normalized to 255. This prevents invisible RGB from
appearing as a colored background: live style tests found magenta hidden beneath
transparent pixels. The white matte is a fallback, not a generated scene, and the
model can still omit a requested background. Explicit transparent generation
retains its usable alpha instead. Entirely transparent results are rejected.

The dedicated empty-background operation has a stricter scene check: it rejects
a result if more than 1% of its pixels have alpha below 250, with a retry message.
Accepted plates use the same endpoint cleanup and white compositing. This permits
minor VAE residue without silently accepting a transparent subject as a complete
environment. Native RGBA is a documented model capability; the hidden RGB color is
not an official output convention. [Publisher RGBA documentation](https://github.com/QwenLM/Qwen-Image-2.1#transparent-image-generation-rgba)

Background requests put empty-scene requirements into the main instruction and
also supply exclusions in the negative prompt. **CFG 1 ignores negative
conditioning**, so negative text alone cannot enforce an empty scene. Higher
guidance is an experimental quality tradeoff; empty scenes still require visual
review. No prompt can guarantee the absence of objects in every generation.

Object removal supplies a second, black-and-white mask reference. After Qwen
edits the image, Local Image composites through that selection. Every pixel
outside a zero-valued selection remains exactly equal to the source. The mask
guides the edit, rather than claiming a specialized inpainting model.

Sources: [official ComfyUI guide](https://docs.comfy.org/tutorials/image/qwen/qwen-image-2-1),
[background removal template](https://github.com/Comfy-Org/workflow_templates/blob/main/templates/image_qwen_image_2_1_background_removal.json),
[native encoder implementation](https://github.com/Comfy-Org/ComfyUI/blob/master/comfy_extras/nodes_qwen.py),
[CFG behavior](https://github.com/Comfy-Org/ComfyUI/blob/master/comfy/samplers.py),
[alpha handling](https://github.com/Comfy-Org/ComfyUI/blob/master/comfy_extras/nodes_compositing.py).

## License

Qwen Image 2.1 uses the **Qwen Research License**, which limits the default grant
to noncommercial research and evaluation. Commercial use requires a separate
license from Qwen. This local integration does not bundle weights or grant rights
to use them. See the [official license](https://github.com/QwenLM/Qwen-Image-2.1/blob/main/LICENSE)
before using the model in a commercial workflow or distributing it with a product.

## Validation

`tests/test_qwen_image.py` checks model discovery, API graph wiring, transparency,
canvas alignment, empty-background instructions, and unchanged pixels outside a
removal mask. These tests mock inference; they do not establish model quality.
Live tests should separately inspect fine hair, transparent glass, hard product
edges, contact shadows, mask-only removals, and generated empty backgrounds.

Adapter entry points are `get_qwen_status()`, `run_qwen_image(...)` and
`run_qwen_removal(...)` in `backend/qwen_image.py`. All GPU operations use the
configured local ComfyUI port and the existing history-polling client.
