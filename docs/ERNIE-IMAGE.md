# ERNIE-Image Base for text-heavy graphics

ERNIE-Image Base is the additional poster model selected after local acceptance testing. The publisher describes an 8B text-to-image model aimed at structured layouts and dense text, with 24 GB consumer-GPU operation. Its published LongTextBench English result is 0.9679 without prompt enhancement and 0.9804 with enhancement. Those are aggregate benchmark results, not a guarantee of exact spelling on a particular poster. The comparison includes older Qwen releases, so it does not establish superiority over Qwen Image 2.1. [Publisher research and benchmark](https://github.com/baidu/ERNIE-Image)

The app preset uses **BF16 Base**, **50 steps**, guidance **4**, and the user's prompt directly. It has no image references or transparency and does not silently rewrite the requested wording. The separate prompt enhancer and Turbo model are not part of this preset. For a design with exact copy, describe the layout and quote each required line. Review the generated text before publishing. [Publisher parameters](https://huggingface.co/baidu/ERNIE-Image)

## Native ComfyUI workflow

The official workflow uses `UNETLoader`, `CLIPLoader` with **type `flux2`**, two `CLIPTextEncode` nodes, `EmptyFlux2LatentImage`, `KSampler`, `VAEDecode` and `SaveImage`. Positive conditioning contains the supplied prompt; the baseline negative prompt is empty. Sampling is **Euler / simple**, denoise **1**, with no extra model-sampling patch. The optional prompt-enhancement branch can be omitted entirely. The current ComfyUI template uses 20 steps; this app's quality baseline uses the publisher's recommended 50. [Official native workflow](https://github.com/Comfy-Org/workflow_templates/blob/main/templates/image_ernie_image.json)

The downloaded template and model metadata are recorded under `qa-artifacts/research/image_ernie_image.json` and `ernie-hub-metadata.json`. These local QA artifacts are not shipped with the app.

## Pinned weights

The public Comfy-Org repack is pinned to revision `82fe29a5cd056f8b1deebc50570f125bcd4f4bea`:

| Folder / file | Bytes | SHA-256 |
| --- | ---: | --- |
| `diffusion_models/ernie-image.safetensors` | 16,067,025,480 | `94a35abaa0899cccc34d2e37310abf74a0a714256526117bba782c7eb4eb91c7` |
| `text_encoders/ministral-3-3b.safetensors` | 7,717,637,511 | `49a750a128863854eac7d85e1a277a7b44bf6ec3646405b84686dfeeca3708ca` |
| `vae/flux2-vae.safetensors` | 336,213,556 | `d64f3a68e1cc4f9f4e29b6e0da38a0204fe9a49f2d4053f0ec1fa1ca02f9c4b5` |

Total storage is **24,120,876,547 bytes**, approximately 24.12 GB. The existing compatible FLUX.2 VAE is reused, reducing a new download to approximately 23.78 GB when that VAE is already installed. Download size is separate from runtime GPU memory. The downloader checks pinned hashes and preserves existing mismatched files for review. [Official Comfy-Org files](https://huggingface.co/Comfy-Org/ERNIE-Image/tree/82fe29a5cd056f8b1deebc50570f125bcd4f4bea)

```powershell
python scripts/download_ernie.py --models-dir "D:\AI Models"
```

ERNIE-Image is released under [Apache 2.0](https://github.com/baidu/ERNIE-Image/blob/main/LICENSE). The preset does not install custom nodes or additional Python code.

## Selection rationale and acceptance

Boogu Base is another credible text-heavy candidate: its publisher recommends 2K for dense layouts, and its BF16 path can use CPU offloading on 32 GB GPUs. However, the publisher also explicitly reports typos, missing characters and layout drift for long or small text. It is not a route to guaranteed perfect typography. We selected the smaller ERNIE native pipeline for the next actual poster comparison; Boogu was not downloaded or added. [Boogu model guidance and limitations](https://huggingface.co/Boogu/Boogu-Image-0.1-Base)

Qwen Image 2.1 remains the installed generation/editing alternative with native transparent assets. Its current release includes text-rendering examples, but those examples also cannot guarantee arbitrary exact text. [Qwen Image 2.1 release](https://qwen.ai/blog?id=qwen-image-2.1)

The actual ERNIE run completed in **70.719 seconds** at **1024 × 1536**, **50 steps**, guidance **4**, seed **87**, using the BF16 preset on a 32 GB RTX 5090. PNG and editable-project export completed. Independent visual inspection found all five requested strings correct, each once, with no extra text: **SHAPE & SOUND**, **Independent Design Festival**, **SATURDAY 12 OCTOBER**, **STUDIO 08 / NEW YORK**, and **CREATE. EXPLORE. CONNECT.** Lettering was clear, with no obvious malformed glyph or baseline defect.

The design still missed a constraint: it rendered a cream sheet on a gray surround with a drop shadow despite the explicit no-mockup, edge-to-edge request. Its circle was closer to magenta/red than vermilion. ERNIE is accepted as an optional poster/text model based on this one successful exact-copy test, not a guarantee for every font, language or layout. The same fixed prompt/seed with Qwen INT8 took 13.078 seconds, spelled four of the five strings correctly, and added unwanted pseudo-text; this is a single comparison rather than a model ranking.

The 24 GB hardware recommendation follows the publisher; the actual app run establishes success on the tested 32 GB card. Neither is a peak-memory measurement. Evidence: [ERNIE result and prompt](../qa-artifacts/v05/poster-quality/ernie-image/results.json), [poster preview](../qa-artifacts/v05/poster-quality/ernie-image/poster.png), [independent visual review](../qa-artifacts/v05/poster-quality/ernie-image/visual-review.json), and [Qwen comparison](../qa-artifacts/v05/poster-quality/qwen/visual-review.json).

Related: [Image Gen](IMAGE-GENERATION.md), [model setup](GEN-MODELS.md), [release validation](LOCAL-IMAGE-VALIDATION.md).
