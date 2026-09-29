# Image generation models

Local Image uses the selected local ComfyUI service for ImageGen. The model controls follow the actual capabilities reported by that service. Quick Heal, existing FLUX removal and the Cutout workspace remain separate tools. Fresh installations use the Local Image AppData profile; existing Local Remove profiles and API paths remain compatible. See [installation and storage](INSTALLATION.md).

## Choose a model

| Preset | Local Image controls | Installed weights, decimal GB |
| --- | --- | ---: |
| Qwen Image 2.1 Compact INT8 | Text generation, up to ten ordered image references, optional transparent output, steps, guidance and negative prompt | 17.28 |
| Qwen Image 2.1 BF16 | The same capabilities at full precision | 32.44 |
| Z-Image Turbo BF16 | Text generation or a variation from one starting image; steps and variation strength | 20.69 |
| FLUX.2 Klein 4B BF16 | Fast text generation and up to four image references; distilled four-step preset | 16.13 |
| FLUX.2 Klein 9B FP8 | Larger distilled reference-editing model; four-step preset; publisher access required | 18.43 |
| ERNIE-Image Base BF16 | Text-heavy posters; text only, steps, guidance and negative prompt | 24.12 |

Qwen references are numbered in order: `<image1>`, `<image2>` and so on. Describe how each should influence the result. The first reference is fitted inside the chosen output canvas without losing its content. Qwen generation defaults to 25 steps and guidance 1. Negative conditioning has no effect at guidance 1; higher guidance is an experimental option. Transparent generation requests real alpha, checks the result, and reports an error if it receives an opaque image. Opaque generation composites unexpected transparency over white; dedicated empty-background generation rejects substantial transparency. See [Qwen setup and limitations](QWEN-IMAGE-21.md).

Z-Image Turbo uses an encoded starting image for variations. It does not interpret that image as a semantic instruction reference. The starting image is center-cropped to the output aspect ratio; transparent areas are placed over white. Lower variation strength retains more of the starting composition. Output is opaque. The Turbo model uses distilled sampling without classifier-free guidance, so the app does not expose a negative prompt or adjustable guidance for it. The publisher identifies Turbo as a 6B, eight-evaluation generation model. [Publisher model card](https://huggingface.co/Tongyi-MAI/Z-Image-Turbo)

Output dimensions are 256–4096 pixels per side in multiples of 32, with a 4,194,304-pixel total limit. Generation creates a new unsaved image session. Its prompt, model, precision, dimensions, seed and sampling settings are retained as generation metadata; saving a project preserves those values.

Both FLUX presets use distilled Klein models with four steps and fixed guidance 1. Klein 4B is the smaller option; 9B provides a larger model and text encoder. Both accept up to four references and return opaque output. The installed `flux-2-klein-base-4b.safetensors` and removal LoRA continue serving the existing repair workflow; the distilled generation model has its own file. FLUX Dev has been removed from the visible model picker, while historical project/API compatibility remains. [BFL model overview](https://docs.bfl.ai/flux_2/flux2_overview)

HiDream-O1 was evaluated and removed from the offered models after a workflow audit and final visual test still showed grid artifacts and soft detail. Existing project metadata remains readable. See [the evaluation record](HIDREAM-O1.md).

ERNIE-Image uses the Base BF16 model with 50 steps and guidance 4. The native preset accepts a written prompt, returns opaque RGB and does not support references or LoRAs in this release. It does not rewrite the prompt. In the actual poster test, it reproduced all five requested strings correctly in 70.719 seconds at 1024 x 1536 on the 32 GB RTX 5090, but missed the no-mockup layout instruction. Proofread each result; one successful example cannot guarantee perfect text. The 24.12 GB preset reuses the existing FLUX.2 VAE. See [the pinned ERNIE files and workflow](ERNIE-IMAGE.md).

## Install only the selected preset

Use **Browse models…** in Image Gen to compare the supported presets before downloading. It shows each model's strengths, limitations, precision, download size, sampling defaults and hardware guidance even when ComfyUI is offline. Choose the shared models folder in Settings or from the model browser. Download progress survives navigating between tools while the app remains open. A download blocks other model setup and image operations until it completes. Closing the app interrupts the current download; already verified files remain available.

Downloads use fixed publisher revisions, expected byte counts and SHA-256 checksums. Existing matching files are verified and reused. Existing mismatched files are preserved and reported; they are never replaced automatically. The size shown as missing accounts for reusable files. These are storage estimates, not GPU-memory requirements. The downloader does not restart ComfyUI; refresh model availability after completion.

Klein 9B's official BF16 and FP8 repositories currently require publisher access approval. The app keeps the FP8 preset in its catalog but cannot download it anonymously. Follow the publisher's access process and place the exact approved files in the selected model folders; retrying setup verifies matching existing files. The app does not collect publisher credentials or bypass repository access controls. The other visible model presets are public downloads. The validation PC completed approved installation of the pinned 9B preset and real text/reference generation; [the validation report](LOCAL-IMAGE-VALIDATION.md) records those results.

Developer CLI using the same verified downloader:

```powershell
python scripts/download_generation_models.py --models-dir "D:\AI Models" --model z-image-turbo --variant bf16
python scripts/download_generation_models.py --models-dir "D:\AI Models" --model qwen --variant int8
python scripts/download_generation_models.py --models-dir "D:\AI Models" --model flux2-klein-4b --variant bf16
python scripts/download_generation_models.py --models-dir "D:\AI Models" --model flux2-klein-9b --variant fp8
```

Z-Image Turbo's verified preset contains:

| ComfyUI folder | File | Bytes |
| --- | --- | ---: |
| diffusion_models | z_image_turbo_bf16.safetensors | 12,309,866,400 |
| text_encoders | qwen_3_4b.safetensors | 8,044,982,048 |
| vae | ae.safetensors | 335,304,388 |

The text encoder can be shared with the existing FLUX preset when its checksum matches. The pinned Z repository revision is `6fc90a3b1b653e935a0d175e260736de25b84df5`; exact hashes and URLs are in `backend/generation_download_catalog.py`. The Qwen catalog is in `backend/qwen_download_catalog.py`. Comfy-Org also publishes other Z quantizations; this app currently exposes the tested BF16 preset. [Official Comfy-Org files](https://huggingface.co/Comfy-Org/z_image_turbo/tree/6fc90a3b1b653e935a0d175e260736de25b84df5/split_files)

The FLUX generation presets add these components:

| Preset | Diffusion model | Encoder | VAE |
| --- | --- | --- | --- |
| Klein 4B BF16 | `flux-2-klein-4b.safetensors` — 7,751,105,712 bytes | Shared `qwen_3_4b.safetensors` | Shared `flux2-vae.safetensors` |
| Klein 9B FP8 | `flux-2-klein-9b-fp8.safetensors` — 9,433,061,528 bytes | `qwen_3_8b_fp8mixed.safetensors` — 8,664,848,742 bytes | Shared `flux2-vae.safetensors` |

Klein 4B is pinned to BFL revision `e7b7dc27f91deacad38e78976d1f2b499d76a294`. Klein 9B FP8 uses BFL revision `902d9d510b51533e07729f19211414a3648b77d2` and its encoder uses Comfy-Org revision `3f62d9d8ae1fec33c6e91453d5c712855b096b55`. The VAE catalog accepts two previously verified compatible exports without replacing either. NVFP4 and remote text encoding are not part of these presets. [BFL Klein 4B files](https://huggingface.co/black-forest-labs/FLUX.2-klein-4B/tree/e7b7dc27f91deacad38e78976d1f2b499d76a294), [BFL Klein 9B FP8](https://huggingface.co/black-forest-labs/FLUX.2-klein-9b-fp8), [9B encoder files](https://huggingface.co/Comfy-Org/flux2-klein-9B/tree/3f62d9d8ae1fec33c6e91453d5c712855b096b55)

## ComfyUI adapter details

The Z graph follows the official template: `UNETLoader`, `CLIPLoader` with `type=lumina2`, `VAELoader`, `CLIPTextEncode`, `ConditioningZeroOut`, `ModelSamplingAuraFlow` with shift 3, `KSampler`, `VAEDecode` and `SaveImage`. Text generation uses `EmptySD3LatentImage`; variations substitute `LoadImage` → `VAEEncode`. Defaults are 8 steps, CFG 1, `res_multistep`, `simple`, and full denoise for text generation. This is the ComfyUI convention; the publisher's Diffusers example uses different step/guidance arguments. [Official ComfyUI workflow](https://github.com/Comfy-Org/workflow_templates/blob/main/templates/image_z_image_turbo.json), [ComfyUI guide](https://docs.comfy.org/tutorials/image/z-image/z-image-turbo)

The app checks loader choices, required node names, encoder type and sampler availability before reporting a preset ready. ComfyUI must see the configured models folder. A running service alone is insufficient. Model discovery refreshes through `/object_info`; no custom Z nodes are installed.

FLUX uses `CLIPLoader` with `type=flux2`, `Flux2Scheduler`, Euler sampling, `EmptyFlux2LatentImage` and `SamplerCustomAdvanced`, with guidance 1. Image references are encoded by the VAE and attached through `ReferenceLatent`. The app retains the full FLUX.2 VAE already used for repair; a separate smaller decoder is optional upstream and not required here. [Official Klein 4B workflow](https://github.com/Comfy-Org/workflow_templates/blob/main/templates/image_flux2_klein_image_edit_4b_distilled.json), [official Klein 9B workflow](https://github.com/Comfy-Org/workflow_templates/blob/main/templates/image_flux2_klein_image_edit_9b_distilled.json)

Setup API: `GET /api/local-remove/generator/download` returns a shared job plus `models`, each containing variants, total/missing bytes and license links. The native-authorized POST accepts only `{model, variant}`. The old `/api/local-remove/qwen/download` endpoints remain compatible and use the same job and generation lock. Image generation capabilities are a separate `GET /api/local-remove/generation/models` response; generation uses `POST /api/local-remove/generation` and imported session IDs for references, never arbitrary filesystem paths.

## Licenses and hardware

Qwen Image 2.1 uses the Qwen Research License: noncommercial research/evaluation; commercial use requires a separate license. Z-Image Turbo is marked Apache 2.0 by its publisher and the Comfy-Org repack. Model and component license terms remain applicable when distributing weights. [Qwen license](https://github.com/QwenLM/Qwen-Image-2.1/blob/main/LICENSE), [Z model card](https://huggingface.co/Tongyi-MAI/Z-Image-Turbo), [Z repository license](https://github.com/Tongyi-MAI/Z-Image/blob/main/LICENSE)

FLUX.2 Klein 4B uses Apache 2.0. Klein 9B model use is governed by the FLUX Non-Commercial License; commercial model deployment requires separate licensing. Output use follows the license's conditions. [Klein 4B license](https://huggingface.co/black-forest-labs/FLUX.2-klein-4B/blob/main/LICENSE.md), [Klein 9B license](https://huggingface.co/black-forest-labs/FLUX.2-klein-9B/blob/main/LICENSE.md)

The publisher describes Z Turbo as fitting 16 GB consumer GPUs. That is not a guarantee for every resolution, ComfyUI version or workload. Local Image's hardware guide uses planning recommendations; only the actual validation hardware has measured timings. Reduce output dimensions or release other GPU workloads if a model runs out of memory. [Publisher hardware statement](https://huggingface.co/Tongyi-MAI/Z-Image-Turbo)

Z Turbo's published speed claim uses eight model evaluations. Its native example times generation after loading weights and recommends Flash Attention 3 plus compilation on Hopper GPUs (H100/H200/H800) after warm-up for sub-second execution. Local Image measures the whole ComfyUI request, which also includes any model loading/offloading, prompt encoding, image decoding and result transfer. A first run after switching models can therefore take longer than a repeated run. Fewer steps are available for experimentation; one-step quality is not the publisher's default. [Publisher timing example](https://github.com/Tongyi-MAI/Z-Image/blob/main/inference.py), [official eight-step ComfyUI preset](https://github.com/Comfy-Org/workflow_templates/blob/main/templates/image_z_image_turbo.json)

BFL describes Klein 4B as using approximately 13 GB VRAM. Klein 9B has a larger model and encoder; its FP8 disk size is not a memory minimum. Consult the app's planning guide and actual validation report, and expect offloading to system RAM to affect speed. All prompt encoding remains local. [Klein hardware guidance](https://docs.bfl.ai/flux_2/flux2_overview)

Optional 4K enlargement uses **SeedVR2 7B FP16 base**, a separate upscaler rather than an Image Gen family. Its 16.98 GB preset and 32 GB planning recommendation are described in [SeedVR2 setup and measured quality](SEEDVR2.md). The actual 3840 x 2160 portrait test took 13.812 seconds on a 32 GB RTX 5090; the result adds generative detail and requires review.

Optional adapters are selected through the live [LoRA library](LORA-LIBRARY.md). They do not download or enable automatically, and acceleration adapters can require explicit sampling changes.

Related guides: [Image Gen controls](IMAGE-GENERATION.md), [Cutout workspace](CUTOUT-WORKSPACE.md), [stock library](STOCK-LIBRARY.md), [Qwen validation](QWEN-VALIDATION.md), [workspace design references](WORKSPACE-REFERENCES.md).
