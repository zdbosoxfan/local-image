# HiDream-O1 Full FP8 evaluation — not offered

HiDream-O1 was evaluated and **rejected for this release** because regular grid artifacts and soft detail persisted after a workflow audit and final sampler comparison. It is absent from the model browser, setup download choices and hardware recommendations. Historical projects retain their model metadata; the downloaded checkpoint remains on disk. The following is an evaluation record, not an installation recommendation. [Publisher project](https://github.com/HiDream-ai/HiDream-O1-Image), [ComfyUI native guide](https://docs.comfy.org/tutorials/image/hidream/hidream-o1)

The model is trained around four megapixels. Start with **2048 × 2048** when evaluating quality. ComfyUI's native node explicitly warns that much smaller outputs reduce quality; a small preview is not a representative quality benchmark. Other trained shapes include 2560 × 1440 and 2496 × 1664, with portrait equivalents. The publisher lists 50 inference steps for Full. The current ComfyUI template uses 40, guidance 5 and `dpmpp_2m_sde_gpu` with the normal schedule. These are slower, detailed-generation settings rather than a distilled fast preset. [Native node resolution guidance](https://github.com/Comfy-Org/ComfyUI/blob/master/comfy_extras/nodes_hidream_o1.py), [official Full workflow](https://github.com/Comfy-Org/workflow_templates/blob/main/templates/image_hidream_o1.json)

The evaluated checkpoint is a single file in the shared `checkpoints` folder:

| Field | Value |
| --- | --- |
| File | `hidream_o1_image_fp8_scaled.safetensors` |
| Size | 8,067,535,296 bytes / 8.07 GB |
| Publisher repack | `Comfy-Org/HiDream-O1-Image` |
| Pinned revision | `377ec7124bc46a15736c68cd9e4ad6d7da7a7614` |
| SHA-256 | `05ad98bc4a94557697f31b839f6dbf6dba293a353d9e3c52eef7818b5802d206` |

The exact checkpoint is linked by ComfyUI's model guide. Its integrated components are exposed through `CheckpointLoaderSimple`; the graph still uses ComfyUI's `VAEDecode` interface. No separate VAE or text-encoder weight download is required for direct-prompt generation. FP8 uses higher precision for some sensitive tensors; its filename does not mean every tensor uses eight bits. [Published checkpoint](https://huggingface.co/Comfy-Org/HiDream-O1-Image/blob/377ec7124bc46a15736c68cd9e4ad6d7da7a7614/checkpoints/hidream_o1_image_fp8_scaled.safetensors)

The evaluation download verified the pinned size and SHA-256 and installed no custom nodes. App download requests for this family are now rejected. The published Full model and the Comfy-Org repack use the MIT license. [Publisher license](https://github.com/HiDream-ai/HiDream-O1-Image/blob/main/LICENSE)

The app completed real text generation and a one-reference edit on a **32 GB RTX 5090** at **2048 x 2048, 50 steps and guidance 5**. The jobs took **44.17 seconds** and **100.84 seconds**, respectively; PNG export and editable-project round trips passed. Driver GPU-usage samples during the runs were **21,879, 22,455 and 22,705 MiB**. These observations are not a measured peak or a minimum VRAM requirement. [Measured results and visual limitations](LOCAL-IMAGE-VALIDATION.md#real-image-gen-inference)

Checkpoint size is not a VRAM requirement: activations, image dimensions, references, other loaded components and offloading affect memory. The publisher documentation reviewed here does not establish a minimum consumer-GPU memory requirement for this FP8 ComfyUI preset. The successful local run validates this machine and these settings; app hardware figures remain planning estimates for other configurations.

Related: [Image Gen controls](IMAGE-GENERATION.md), [model selection and setup](GEN-MODELS.md), [stock library](STOCK-LIBRARY.md).

## Quality audit after visual rejection

The initial images passed execution, dimensions and project checks but show visible regular grid texture and soft detail at full resolution. The chair edit also colors wooden arms that the prompt intended to preserve. These findings prevent treating successful inference as acceptance of photographic quality.

The adapter was compared with the official Full template and publisher code. Its 2048-square canvas, noise scale 8, guidance 5, normal schedule, `dpmpp_2m_sde_gpu` sampler, and seam-smoothing values (`0.8` to `1.0`, `single_shift`, `ramp_2_4`, `median`, strength `1`) match the native template. The template uses 40 steps and a BF16 checkpoint; the app uses the publisher's recommended 50 steps and ComfyUI's documented Full FP8 checkpoint. No external prompt enhancer is necessary for the direct-prompt path. Passing the scheduler the seam-patched model does not change its sampling configuration. [Official template](https://github.com/Comfy-Org/workflow_templates/blob/main/templates/image_hidream_o1.json), [publisher inference defaults](https://github.com/HiDream-ai/HiDream-O1-Image/blob/main/inference.py)

ComfyUI implementer Kijai reports reproducing the grid artifacts with the publisher's own implementation, and describes seam smoothing as mitigation that can introduce blur at higher pass counts. This supports a model-level limitation rather than an identified adapter wiring error. The evidence does not isolate FP8 as the cause or establish that BF16 fixes it. [Maintainer's reproduction and smoothing explanation](https://huggingface.co/Comfy-Org/HiDream-O1-Image/discussions/2)

The final diagnostic followed the maintainer's reported results with `res_multistep` and a `beta` schedule, holding seed 0, prompt, 2048-square canvas, 50 steps, guidance 5 and seam settings fixed. It completed in **48.828 seconds**. The title and footer remained correct, but regular grid/block texture persisted in the wall and floor and detail remained soft. The result failed the requested high-fidelity quality bar. HiDream was therefore removed from the offered catalog; no BF16 download or further sampler tuning was used to prolong acceptance. [Maintainer's sampler observations](https://huggingface.co/Comfy-Org/HiDream-O1-Image/discussions/5), [diagnostic settings](../qa-artifacts/v05/hidream-audit/results.json), [final diagnostic image](../qa-artifacts/v05/hidream-audit/res-multistep-beta.png)
