# Refine workflows against Comfy-Org

Checked 2026-10-09. Sources are the downloaded UI-format workflows in
`target/refine-templates/`, including `definitions.subgraphs`. The comparison includes all
16 image families; SeedVR2 is an upscaler and has no `Task::Refine` recipe.

For families without a dedicated local img2img template, the comparison uses their official
text-to-image loader/conditioning/sampler recipe plus ComfyUI's
[documented img2img conversion](https://comfyanonymous.github.io/ComfyUI_examples/img2img/):
load the image, VAE-encode it, and sample with partial denoise. Those rows are **derived
img2img**, not claims that Comfy-Org ships a family-specific refine template. API-only
Stability/BFL templates are not substitutes for local model graphs. Pony and Illustrious
inherit SDXL; no separate Comfy-Org refine template was found in this snapshot.

`Refine` uses denoise 1 for native reference/edit conditioning, otherwise 0.35.
At 0.4 it always starts from an encoded source. `Edit` is compared only for families that
advertise native editing. An Edit request on an ordinary model falls back to Refine in
`ops.rs`. In the table, “graph verified” means topology and profile tests pass; runtime
verification is reported separately below.

## Family comparison

| Family | Official template / reference | Our graph after fixes | Differences and verdict |
| --- | --- | --- | --- |
| SD1.5 | [Official img2img example](https://comfyanonymous.github.io/ComfyUI_examples/img2img/) | Checkpoint loader → CLIP positive/negative; source → VAEEncode → KSampler at partial denoise → VAEDecode. | No dedicated SD1.5 template in this snapshot. Standard img2img path retained; optional IP-Adapter references are an extension. **Graph verified, derived img2img.** |
| SDXL | [image_sdxl_simple.json](https://github.com/Comfy-Org/workflow_templates/blob/main/templates/image_sdxl_simple.json) + img2img conversion | Checkpoint CLIP/VAE; encoded source replaces the empty latent; KSampler respects denoise. | Profile recommends 30 steps/CFG 6/dpmpp_2m_sde instead of template 25/7/dpmpp_2m; both use karras. Recommendations remain user-overridable. **Graph verified; both installed RealVisXL checkpoints pass.** |
| Pony | SDXL template above | SDXL img2img, with CLIP skip −2 and score prompt conventions. | Inherited architecture; no semantic reference required for preserving the source. **Graph verified, derived img2img.** |
| Illustrious | SDXL template above | SDXL img2img, with CLIP skip −2 and anime prompt conventions. | Euler ancestral/normal is the profile recommendation. **Graph verified, derived img2img.** |
| SD3.5 | [sd3.5_simple_example.json](https://github.com/Comfy-Org/workflow_templates/blob/main/templates/sd3.5_simple_example.json) + img2img conversion | Checkpoint + external TripleCLIPLoader; ModelSamplingSD3 shift 3; source VAEEncode; partial KSampler. | External encoders and explicit shift support split checkpoint installations; sampler recommendation dpmpp_2m versus template euler, both sgm_uniform. **Graph verified, derived img2img.** |
| FLUX.1 Dev / Schnell | [flux_dev_full_text_to_image.json](https://github.com/Comfy-Org/workflow_templates/blob/main/templates/flux_dev_full_text_to_image.json), [flux_schnell_full_text_to_image.json](https://github.com/Comfy-Org/workflow_templates/blob/main/templates/flux_schnell_full_text_to_image.json) + img2img conversion | UNET + DualCLIP + VAE; source VAEEncode; euler/simple KSampler at CFG 1. Dev has FluxGuidance 3.5. | **Fixed Schnell file defaults**: 4 steps and no guidance embedding instead of inherited Dev settings. Redux is optional and separate from the starting source. **Graph verified, derived img2img.** |
| FLUX.1 Kontext | [flux_kontext_dev_basic.json](https://github.com/Comfy-Org/workflow_templates/blob/main/templates/flux_kontext_dev_basic.json) | Source → FluxKontextImageScale → VAEEncode; latent attached to conditioning and used as the sampler's starting latent for both Refine and Edit; FluxGuidance 2.5, euler/simple, CFG 1. | **Fixed Refine**: full denoise previously used an empty canvas, partial denoise used an unscaled second encoding. Both now reuse the scaled source encoding. Our extra negative reference is inert at the default CFG 1; multiple references are chained instead of the template's stitched example. **Graph verified.** |
| FLUX.1 Fill | [flux_fill_inpaint_example.json](https://github.com/Comfy-Org/workflow_templates/blob/main/templates/flux_fill_inpaint_example.json) | Source + generated white SolidMask → InpaintModelConditioning; its positive, negative and latent outputs feed KSampler; FluxGuidance 30. | **Fixed Refine** to supply the Fill image/mask conditioning explicitly. Whole-image refinement uses an all-white mask and partial denoise; the template regenerates a painted region at denoise 1. DifferentialDiffusion remains on actual masked inpainting. **Graph verified, whole-canvas adaptation.** |
| FLUX.2 Dev | [image_flux2.json](https://github.com/Comfy-Org/workflow_templates/blob/main/templates/image_flux2.json) | Source scaled to ~1 MP and VAE-encoded as ReferenceLatent; FluxGuidance 4; Flux2Scheduler + SamplerCustomAdvanced. Full denoise uses EmptyFlux2LatentImage. | CFGGuider at CFG 1 is equivalent to BasicGuider. At partial denoise we add source VAEEncode + SplitSigmasDenoise output 1. Optional Turbo LoRA switches are omitted. **Graph verified; existing reference path retained.** |
| FLUX.2 Klein | [4B distilled](https://github.com/Comfy-Org/workflow_templates/blob/main/templates/image_flux2_klein_image_edit_4b_distilled.json), [9B distilled](https://github.com/Comfy-Org/workflow_templates/blob/main/templates/image_flux2_klein_image_edit_9b_distilled.json), [4B base](https://github.com/Comfy-Org/workflow_templates/blob/main/templates/image_flux2_klein_image_edit_4b_base.json), [9B base](https://github.com/Comfy-Org/workflow_templates/blob/main/templates/image_flux2_klein_image_edit_9b_base.json) | Both conditionings get the ~1 MP reference latent. Full denoise starts empty; partial denoise starts from the source with split sigmas. Distilled: 4 steps/CFG 1; installed base: 20 steps/CFG 5. | Previous builder's distilled reference fix is preserved, including `resolution_steps`. **Fixed base defaults and negative encoding**: CFG above 1 needs blank text conditioning, not zeroed tokens. Lanczos replaces nearest-exact in some templates. **Graph verified; all three installed Klein weights pass.** |
| Qwen Image / 2512 | [image_qwen_image.json](https://github.com/Comfy-Org/workflow_templates/blob/main/templates/image_qwen_image.json), [image_qwen_Image_2512.json](https://github.com/Comfy-Org/workflow_templates/blob/main/templates/image_qwen_Image_2512.json) + img2img conversion | CLIPLoader qwen_image + VAE; ModelSamplingAuraFlow shift 3.1; source VAEEncode; euler/simple partial KSampler. | Lightning LoRAs and their switches are optional template branches, not required for base refinement. Steps/CFG differ by recommendations. **Graph verified, derived img2img.** |
| Qwen Edit original / 2509 / 2511 | [original](https://github.com/Comfy-Org/workflow_templates/blob/main/templates/image_qwen_image_edit.json), [2509](https://github.com/Comfy-Org/workflow_templates/blob/main/templates/image_qwen_image_edit_2509.json), [2511](https://github.com/Comfy-Org/workflow_templates/blob/main/templates/image_qwen_image_edit_2511.json) | Original uses TextEncodeQwenImageEdit and a shared 1.5 MP image/starting latent. 2509/2511 use TextEncodeQwenImageEditPlus for both conditionings and FluxKontextImageScale for the starting latent. All use CFGNorm after the sampling shift. | **Fixed original encoder selection, source scaling and CFGNorm**. The 2511 weights select their timestep-zero reference method internally; explicit template reference-method nodes are redundant for those weights. Full refine/edit use denoise 1; partial refine uses the source latent at requested denoise. **Graph verified.** |
| Qwen Image 2.1 | [image_qwen_image_2_1_image_edit.json](https://github.com/Comfy-Org/workflow_templates/blob/main/templates/image_qwen_image_2_1_image_edit.json) | TextEncodeQwenImage21 receives source as reference 1, outputs positive/negative and an empty latent sized to reference 1. Full Refine/Edit uses that third output. Partial Refine also VAE-encodes reference 1. | **Fixed ignored denoise** and the empty starting latent at partial denoise. The source and starting encoding share a 32-pixel rounded canvas and alpha join. Default changed from 0.35 to 1, matching full edit; optional prompt enhancer/cache branches are omitted. **Graph verified; BF16 and INT8 pass.** |
| Z-Image Turbo / base | [image_z_image_turbo.json](https://github.com/Comfy-Org/workflow_templates/blob/main/templates/image_z_image_turbo.json), [image_z_image.json](https://github.com/Comfy-Org/workflow_templates/blob/main/templates/image_z_image.json) + img2img conversion | lumina2 CLIP + ae VAE; AuraFlow shift 3; source VAEEncode; res_multistep/simple. Turbo: 8 steps/CFG 1/zero negative. Base: 25 steps/CFG 4/text negative. | **Fixed base file defaults**, which previously inherited Turbo's CFG 1 and zero negative. Source is an initial latent, not a semantic reference. **Graph verified; installed Turbo passes.** |
| ERNIE-Image | [image_ernie_image.json](https://github.com/Comfy-Org/workflow_templates/blob/main/templates/image_ernie_image.json) + img2img conversion | flux2 CLIP with Ministral + flux2 VAE; CLIP positive/negative; source VAEEncode; euler/simple, CFG 4. | Optional prompt enhancement omitted; profile recommends 50 steps versus this template's 20. ERNIE uses KSampler, not the FLUX.2 custom scheduler despite sharing components. **Graph verified; installed weights pass.** |
| HiDream I1 Full / Dev / Fast | [Full](https://github.com/Comfy-Org/workflow_templates/blob/main/templates/hidream_i1_full.json), [Dev](https://github.com/Comfy-Org/workflow_templates/blob/main/templates/hidream_i1_dev.json), [Fast](https://github.com/Comfy-Org/workflow_templates/blob/main/templates/hidream_i1_fast.json) + img2img conversion | QuadrupleCLIPLoader + ae VAE; source VAEEncode; ModelSamplingSD3. Full: 50 steps/CFG 5/shift 3/uni_pc/simple. Dev: 28/1/shift 6/lcm/normal. Fast: 16/1/shift 3/lcm/normal. | **Fixed Dev/Fast file defaults**, which previously inherited Full's CFG and sampler (and Dev's wrong shift). I1 profiles do not implement the separate E1/O1 architectures. **Graph verified, derived img2img.** |

## Conditioning details and compatibility

The [Qwen node implementation](https://github.com/Comfy-Org/ComfyUI/blob/master/comfy_extras/nodes_qwen.py)
confirms that Qwen 2.1's third encoder output contains zeros, not the encoded source. Lowering
denoise on that output cannot preserve an image reliably. Partial refinement now uses a
separate VAEEncode while retaining the official reference conditioning. Its source canvas
rounding matches the encoder's ties-to-even rule. Full editing continues to use the empty
canvas, as in the official template.

Qwen Edit Plus encodes reference images internally; another ReferenceLatent node would
duplicate them. The [Qwen transformer](https://github.com/Comfy-Org/ComfyUI/blob/master/comfy/ldm/qwen_image/model.py)
stores the `index_timestep_zero` marker in 2511 weights. Original Qwen Edit uses a different
single-image encoder, selected by known original weight filenames. Renamed unknown variants
need an explicit user profile; architecture names alone cannot distinguish every variant.

New optional `file_overrides` profile rules select known weight variants by basename patterns.
They are applied when installed models enter the catalog; curated model overrides still
take precedence. Existing schema-1 profiles load without this field through serde defaults.
Rules are data-only and do not add dependencies. `zero_negative` now applies only at CFG ≤ 1;
CFG above 1 uses actual text conditioning, including an empty negative prompt.

Klein and FLUX.2 partial denoise retain the previous builder's encoded-source adaptation.
The split scheduler uses output 1, and only exactly 1.0 takes the empty-latent path. Optional
LoRAs, prompt enhancers, comparison nodes and UI switches do not belong to the minimal refine
graph. Graph tests pin connections as well as node classes.

## Reproduce the inspection

```sh
PATH=~/.cargo/bin:$PATH CARGO_BUILD_JOBS=3 cargo +1.98.1 run --offline -p li-ai \
  --example refine_compare -- target/refine-templates > target/refine-comparison.md
```

The Rust summarizer accepts a directory or one UI JSON file. It reads tuple and object links,
shows root and subgraph scopes, labels subgraph input boundaries, retains widgets/modes,
prints snapshot SHA-256 values, and compares classes/inputs with our generated Refine and
native Edit graphs, including file variants. Scope counts include optional branches; widget
arrays are evidence, not guessed API inputs. Follow linked switch inputs to determine the
active branch. The raw comparison is in `target/refine-comparison.md` (regenerable, untracked).

## Live preservation check

```sh
PATH=~/.cargo/bin:$PATH CARGO_BUILD_JOBS=3 cargo +1.98.1 run --offline -p li-ai \
  --example refine_check -- --host 127.0.0.1:8188
```

The checker finds every recognized installed image-model main file, including alternate
precisions, checkpoints and tool base weights. It resolves installed encoders/VAEs and
known sampling variants, uploads a deterministic 512×288 landscape, and runs Refine with
seed 12345 at the catalog's refine default and at 0.4. It prints RGB mean absolute difference
after independently resizing source/output to 64×36 with Triangle filtering. **PASS is
strictly below 35** on the 0–255 scale. Artifacts are saved under `target/refine-check/`.

It checks both running and pending queues before each job, waits up to 120 seconds for an idle
queue, submits one job at a time, never interrupts globally, and fails on execution errors,
missing components/nodes, failed preservation, or no executed models. Unsupported/non-image
weights are explicitly listed as SKIP. `--model filename-substring` supports focused reruns.
The checker uses the production builder directly at a small canvas; the app additionally
rescales work images to the family's working resolution and resizes results back.

### RTX 5090 results

ComfyUI 0.38.0, PyTorch 2.14.1+cu130, NVIDIA RTX 5090, driver 615.71.09. No GPU jobs skipped.
Nine image-model files, 18 final measurements, all PASS. The initial base-Klein failure
reproduced the zero-negative bug; its focused rerun used the corrected graph and the base
template's 20 steps/CFG 5. Other measurements are from the serial full inventory run.

| Installed weights | Default denoise | MAD at default | MAD at 0.4 | Seconds default / 0.4 |
| --- | ---: | ---: | ---: | ---: |
| RealVisXL_V5.0_fp16 | 0.35 | 3.580 | 4.958 | 17.69 / 3.72 |
| XL_RealVisXL_V5.0_Lightning | 0.35 | 3.910 | 6.179 | 14.29 / 3.86 |
| ernie-image | 0.35 | 3.116 | 3.445 | 28.61 / 6.05 |
| flux-2-klein-4b | 1.0 | 14.832 | 4.316 | 17.71 / 0.80 |
| flux-2-klein-9b-fp8 | 1.0 | 11.936 | 3.773 | 16.17 / 0.94 |
| flux-2-klein-base-4b | 1.0 | 5.672 | 3.926 | 22.98 / 4.00 |
| qwen_image_2.1_bf16 | 1.0 | 9.595 | 0.843 | 31.41 / 3.81 |
| qwen_image_2.1_int8_convrot | 1.0 | 8.831 | 0.812 | 15.02 / 4.51 |
| z_image_turbo_bf16 | 0.35 | 2.975 | 3.620 | 18.98 / 0.62 |

Timings include loading, upload and polling; warm/cold differences are expected and they are
not a model speed ranking. SeedVR2 and Yue were excluded as non-image services. Models absent
from this ComfyUI were checked structurally, not synthesized: SD1.5, SD3.5, Pony, Illustrious,
FLUX.1/Fill/Kontext, FLUX.2 Dev, Qwen Image/Edit, Z-Image base and HiDream I1 variants. No model
weights were downloaded. These preservation measurements exercise one image/prompt/seed;
they do not prove quality for arbitrary instructions.
