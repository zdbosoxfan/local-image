# Bring your own restoration models

Local Image can detect selected models that **you installed in your own ComfyUI**. It does not ship, bundle, download or install these weights or their community nodes. They are not bundled-component attributions. The default Restoration section uses the separately vetted, pinned permissive models: FBCNN, SCUNet, NAFNet and the relicensed Apache-2.0 DDColor weights. Runtime architecture support is checked separately from licensing.

Open Compositing → Filter → AI → Restore… → Bring your own model → Detect installed models. The app reads ComfyUI's `/object_info` choices and the configured model folders, including `upscale_models`, `facerestore_models` and `stablesr`. Names are identification hints, not proof that a renamed file is the author's checkpoint. Only recognised models are listed. SUPIR and HYPIR are excluded even when installed; names containing either identifier are never offered or submitted.

Every detected BYO model shows its licence name/link and this note:

> Non-commercial licence: the authors restrict commercial use of the model; check the licence before using results in paid work

Before Preview, acknowledge the terms once for that model and ComfyUI server. The acknowledgement is saved in Local Image's state directory (`restoration-byo-acknowledgements.json`). It is specific to the model filename, folder, licence policy and server; another model or server needs its own acknowledgement. An acknowledgement records that you read the notice; it does not grant commercial rights. Existing settings and documents need no migration.

## Licence review (owner review, 2026-10-10)

| User-installed model | Licence / source | Review summary |
|---|---|---|
| LED | [CC BY-NC 4.0, author's repository](https://github.com/Srameo/LED#-license) | Restricts licensed code/weights to non-commercial use. The wording does not expressly cover outputs, but paid use of the model is commercial. LED is a RAW denoiser; an RGB composite needs a compatible image node/conversion supplied by the user. |
| Zero-DCE / Zero-DCE++ | [Zero-DCE](https://github.com/Li-Chongyi/Zero-DCE#license), [Zero-DCE++](https://github.com/Li-Chongyi/Zero-DCE_extension#license), CC BY-NC 4.0 | Same distinction: restricted code/weights, outputs not expressly mentioned, paid model use is commercial. Generic `Epoch99.pth` files need a named folder or node to identify the model. |
| StableSR / CodeFormer | [StableSR S-Lab 1.0](https://github.com/IceClear/StableSR/blob/main/LICENSE.txt), [CodeFormer S-Lab 1.0](https://github.com/sczhou/CodeFormer/blob/master/LICENSE) | Restricts commercial use of software; outputs are not mentioned. |
| PromptIR | [Academic Public License](https://github.com/va1shn9v/PromptIR/blob/main/LICENSE.md) | Permits non-commercial research/personal settings; commercial software use needs the authors' agreement. Outputs are not mentioned. |
| GFPGAN v1.4 | [Apache-2.0 plus third-party licences](https://github.com/TencentARC/GFPGAN/blob/master/LICENSE) | The umbrella Apache licence does not erase the NVIDIA StyleGAN2 component's restriction to research/evaluation. Other notices, including DFDNet, are also present. |
| RestoreFormer++ | [Apache-2.0 code](https://github.com/wzhouxiff/RestoreFormerPlusPlus/blob/main/LICENSE), [author's training-data description](https://github.com/wzhouxiff/RestoreFormerPlusPlus#preparations-of-dataset-and-models), [FFHQ terms](https://github.com/NVlabs/ffhq-dataset#licenses) | Trained on FFHQ. Its dataset and some source images have non-commercial terms. This is a background risk, not an assertion that the Apache code or all outputs are non-commercial. |

This is not legal advice. The owner's review distinguishes restrictions on using the model/software from an explicit licence over output images. Absence of an output clause does not authorise commercial model use or resolve training-data and other rights.

## Clauses behind the review

[CC BY-NC 4.0 §1(i)](https://creativecommons.org/licenses/by-nc/4.0/legalcode.en) defines NonCommercial as “not primarily intended for or directed towards commercial advantage or monetary compensation.” Its grant is limited to licensed material and adaptations under the rights the licensor can grant. LED's author says “for non-commercial use only”; the Zero-DCE repositories also expressly limit the code to academic/non-commercial use. Treat paid operation as commercial under the owner's review; do not infer an express output licence from those statements.

The [S-Lab licences](https://github.com/IceClear/StableSR/blob/main/LICENSE.txt) permit “Redistribution and use for non-commercial purpose in source and binary forms”. [CodeFormer uses the same clause](https://github.com/sczhou/CodeFormer/blob/master/LICENSE). [PromptIR](https://github.com/va1shn9v/PromptIR/blob/main/LICENSE.md) says “PromptIR is free for use in noncommercial settings” and asks commercial users to contact its authors. These clauses concern software use; they do not mention output images.

[GFPGAN's NVIDIA component §3.3](https://github.com/TencentARC/GFPGAN/blob/master/LICENSE) defines its non-commercial use as “for research or evaluation purposes only.” [FFHQ's dataset notice](https://github.com/NVlabs/ffhq-dataset#licenses) permits “use, redistribute, and adapt it for non-commercial purposes”; the individual image licences differ. Whether those data restrictions attach to trained weights/results is not settled by RestoreFormer++'s code licence.

[SUPIR](https://github.com/Fanghua-Yu/SUPIR/blob/master/LICENSE) and [HYPIR](https://github.com/XPixelGroup/HYPIR/blob/main/LICENSE) have a SupPixel Commercial Use clause expressly including “Using the software to process images that are intended for sale.” The owner excludes both entirely. They have no detection cards, acknowledgement bypass or download path in this feature.

## Installed-node compatibility

The core `UpscaleModelLoader` / `ImageUpscaleWithModel` path can use architectures recognised by the user's installed spandrel environment, including extra arches that **the user installed**. Local Image does not install that package. An unsupported descriptor is refused by ComfyUI. The permissive default section retains the separate main-spandrel-only rule. RGB image-node compatibility is distinct from finding a file; a RAW-only LED checkpoint cannot be treated as an RGB model.

Only weight-filename choices count, not version/task menus. Unknown community nodes also need a matching file found in the configured model folders, including named model directories such as `PromptIR` and `LED`; hardcoded download catalogues are not offered as installed models. Required download/install flags defaulting to true and remote URL defaults are refused. Simple community image nodes and loader/image-processor pairs are connected from the types, choices and defaults actually advertised in `/object_info`. An unknown required connection is refused. Folder-only models stay visible with an unavailable reason until ComfyUI advertises a compatible loader. This is not an installer for arbitrary plugins or a licence audit of renamed files.

The [facerestore adapter](https://github.com/comfyorg/comfyui-facerestore/blob/master/__init__.py) uses installed `FaceRestoreModelLoader` and `FaceRestoreCFWithModel` / `FaceRestoreWithModel` nodes. For that adapter, `detection_Resnet50_Final.pth` and `parsing_parsenet.pth` must already be in the face-detection folder; otherwise the app refuses to trigger its known first-run auxiliary downloads. That legacy loader does not support RestoreFormer++; a compatible community image node is needed for that model.

The [StableSR adapter](https://github.com/gameltb/Comfyui-StableSR/blob/main/nodes.py) uses installed `ApplyStableSRUpscaler` and core checkpoint/VAE/conditioning/sampler nodes. Select your installed compatible SD 2.1 diffusion checkpoint. Sampling and conditioning follow the plugin's [non-turbo example](https://github.com/gameltb/Comfyui-StableSR/blob/main/examples/stablesr_w_color_fix.json): 20 Euler steps, normal schedule, CFG 8, denoise 1 and its quality/negative conditioning. Local Image uses seed 0 and original-resolution input, fitting output back to the document. These are not benchmark results, and the separate turbo workflow is not selected automatically. No checkpoint is downloaded. Community plugins remain code installed and managed by the user.

Preview uses the existing ComfyUI queue/cancellation path. Apply adds a new layer in one undo step. Output is fitted back to the document size and its original alpha is restored. The original root layers are hidden in the same edit to avoid doubling transparency.
