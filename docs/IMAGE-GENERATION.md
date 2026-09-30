# Image Gen in Local Image

Choose **Generate** in the top toolbar. An open image starts in **Edit image**; **Create new** and **Draft & Refine** are adjacent toolbar commands. The prompt and its Generate or Apply edit action sit together beneath the image. Model, Output, References and Style are collapsed inspector sections; opening one closes the others. Generated results are ordinary documents that can move into Retouch or Cutout.

## Pick a model and describe the result

The model menu includes a short description of each model's strengths. Choose **Browse models…** to compare all supported models before downloading: strengths, limitations, precision, full and remaining download size, recommended steps, GPU memory guidance and a license link. These catalogue descriptions are available even when ComfyUI is offline. **Use this model** selects it; **Download model** installs its selected preset. You can also expand **Model setup** inside Model to download the current preset.

Choose **Models folder…** in the model browser to select the shared download folder. Files already present there and models visible to the running ComfyUI are separate statuses. If you change the folder while ComfyUI is running, close ComfyUI, then choose **Start AI backend** from Local Image so it launches with the new folder configuration; refresh the connection afterward. Downloading to a folder does not change another running ComfyUI process's model paths. The app does not terminate that process automatically.

| Model | Useful for | Reference input | Default sampling |
| --- | --- | --- | --- |
| Qwen Image 2.1 | Image edits, new images and transparent assets | Up to 10 ordered images | 40 steps, guidance 1 |
| Z-Image Turbo | Fast image generation and variations | One optional starting image | 8 steps, fixed guidance 1 |
| FLUX.2 Klein 4B | Fast reference editing and new images | Up to 4 ordered images | 4 steps, fixed guidance 1 |
| FLUX.2 Klein 9B | Larger reference-editing model | Up to 4 ordered images | 4 steps, fixed guidance 1 |
| ERNIE-Image Base | Text-heavy posters and graphic layouts | No image input | 50 steps, guidance 4 |

Qwen offers **Compact INT8** and **Full BF16** presets. These are precision choices for the same image model. Klein 4B uses distilled BF16 weights, separate from the existing base-model repair workflow. Klein 9B uses an FP8 preset. Only Qwen offers transparent generation.

Klein 9B requires publisher access approval. It was installed with user-approved access and passed real generation and reference-edit tests on the validation PC. Other users must follow the publisher's access process and configure the approved local file; the app does not bypass that requirement. Qwen, Z and Klein 4B have public download presets.

Write the desired subject, composition, lighting and style in **Prompt**. Leave References empty for text-to-image generation. Model downloads do not start image generation automatically.

ERNIE uses the BF16 Base preset and your written prompt directly, with no hidden prompt enhancer. It produces opaque images and currently accepts no references, starting image or LoRAs. Quote each required text line and describe its position, size and style. Its negative prompt is effective above guidance 1; the default is 4. No model guarantees exact spelling or layout, so proofread the result. See [the ERNIE setup and evaluation record](ERNIE-IMAGE.md).

The local ERNIE poster test reproduced all five requested strings correctly with no added text. It still rendered a paper mockup despite the request for edge-to-edge output. This single test supports the poster option; it does not guarantee perfect copy or layout for other designs.

## Guide generation with images

In **References**, choose **Use current image** or **Add images…**. The listed order is the reference order. Remove a reference with its × button. Adding a reference copies it into an image document; generation takes a snapshot of the current visible result, including repair layers and enabled cutout composition. It does not change the source document or the imported file.

**Stock library…** searches Openverse inside Local Image. Review the creator, license and source before choosing **Add reference**. You can also open a stock image as an editable document or use it directly as a cutout background. Imported source credits, background credits and credits from generation references are retained in `.lremove` projects; view them through **File → Image credits…**. Flattened image exports do not automatically place a visible credit on the image, so follow the source license when sharing them.

For Qwen, refer to images explicitly as `<image1>`, `<image2>` and so on. For example: “Use the chair from `<image1>` and the room from `<image2>`. Keep the chair's shape and place it beside the window.” The first reference is fitted inside the requested output canvas without cropping, with transparent padding if needed. The app limits Qwen to ten references to keep local workloads bounded.

FLUX references guide content and composition while the chosen output dimensions define the new canvas. The app accepts four references, reduces each to approximately one megapixel, and places transparent reference pixels over white. Both FLUX models return opaque images.


Z-Image Turbo's input is a **starting image** for a variation. It initializes the model's image latent instead of providing semantic reference instructions. The starting image is center-cropped to the output aspect ratio, with transparency placed over white. **Strength** controls how much it changes: lower values retain more of the starting composition; higher values permit more change. Its output is opaque.

## Set the canvas, transparency and seed

Open **Output** to choose an aspect ratio or enter dimensions. The chain control links width and height. Local Image uses the connected ComfyUI workflow's actual dimension constraints; it does not impose a 4 MP area cap or 4096-pixel ceiling. Current latent nodes report a 16384-pixel side limit, while reference encoders may report no maximum. Qwen generation uses a 16-pixel grid, and its reference-editing workflow uses 32; other active generation workflows use 16. Available memory determines whether a large job can finish. Resolution defaults are starting points, not model maxima.

For Qwen, select **Transparent background** to request a real RGBA image. Local Image checks for a usable alpha channel and reports a failure if the model returns an opaque result. Generated edges can still need refinement in Cutout. Export transparent output as PNG; JPEG cannot preserve alpha.

With transparency off, any transparent Qwen pixels are composited over white. This prevents hidden RGB from becoming a colored backdrop, but a white fallback does not mean the model generated the requested scene. The dedicated **Generate empty background** operation rejects substantial transparency and asks you to retry instead of treating a transparent subject as a finished background.

Leave **Seed** empty for a fresh random seed, or enter a number, including zero, to repeat sampling with the same inputs and settings. The result reports the actual seed. Reproducibility can also depend on the model files, ComfyUI version and hardware.

**Steps** appears inside Output with a short recommendation and an info button linking to its source. Qwen uses the publisher's 40-step default, Z-Image Turbo uses the ComfyUI 8-step preset, distilled Klein uses 4, and ERNIE Base uses 50. Z's publisher Diffusers example specifies 9 inference steps but explicitly describes 8 model forwards. Qwen negative conditioning is inactive at guidance 1. The fixed-guidance distilled workflows retain their supported settings.

## Add an optional LoRA

Expand **LoRAs** on the Prompt tab and choose **Open LoRA library…**. **Installed** shows image examples for adapters assigned to the selected model, with a short name, a **Use** action and a small **i** information button. The information view explains usage, triggers, recommended settings, compatibility and the source/license. Local test examples and publisher examples are labeled separately; an attractive example does not verify an adapter's compatibility. After choosing **Use**, set its strength beside the selected adapter; × removes it. Up to three adapters may be selected. Strength can range from −2 to 2, and zero has no effect. Start with the publisher's recommended value.

**Browse** searches current Hugging Face results and separates recommendations, publisher-declared compatibility and unverified community adapters. Choose **Download…** to review the available files, model page and compatibility before downloading a selected file. Downloading an adapter does not select it automatically. Return to Installed and choose Use. Changing the base model clears the selected adapters. When a publisher has no usable example, the gallery says so; it does not substitute an invented style sample.

Adapters are restricted to their assigned base model. An adapter for the original Qwen Image, another FLUX family, or a different Klein size is not automatically compatible. Unverified community files require an explicit model assignment in the library. See [LoRA library details](LORA-LIBRARY.md).

Some acceleration adapters need particular sampling values. **Apply recommended settings**, when offered, changes those values explicitly; it does not enable the adapter. Ordinary LoRAs leave the chosen sampling values alone. The app applies adapters to the diffusion model only, leaving the text encoder and VAE unchanged.

## Continue editing or use the result as a background

**Generate image** creates a new unsaved document. **Retouch result** opens the normal retouch tools; **Cut out result** opens the Cutout studio. The original reference documents remain available.

To generate a replacement background, start from a document with an enabled cutout, switch to Image Gen, describe an empty scene and generate. **Use as background** then places the generated image behind that original subject and returns to Cutout. Subject position, scale, rotation and shadow remain editable there. For the dedicated Qwen empty-scene prompt, use **Generate empty background** directly in Cutout's Background tab.

Save a `.lremove` project to preserve image data, cutout composition, repair layers and generation settings. Reopening a generated project restores its model, precision, prompt, seed, dimensions and sampling controls. Projects from retired models, including HiDream and FLUX Dev, retain their saved identity and show that it is unavailable; the app does not silently select a replacement. Reference images and LoRA weights are not bundled with the generated result's project. The app shows the saved reference count and asks you to reattach those images; unavailable saved adapters must be removed or reinstalled before generating again. Switching personas preserves your current prompt draft. Export creates a flattened image for other software.

## Draft & Refine and optional upscaling

Choose **Draft & Refine…** in the Image Gen context toolbar. The two-pane dialog keeps **Draft** on the left and **Refined image** on the right for comparison. Select a draft from the current document or from **Library…**, or generate one using the separate **Draft model** and **Draft prompt** controls. Draft size, sampling and references are available in the draft settings disclosure.

Choose a **Refinement model** and edit the visible **Refinement prompt** to say what should remain and what should improve. **Final resolution**, **Width**, **Height** and **Steps** are editable, with a model-specific recommendation. For example, a fast Z or Klein draft can be used as a reference for Qwen or Klein 9B. Refinement creates a separate result; it does not overwrite the selected draft.

Each stage has independent **Styles / LoRAs…** and output settings. **Transparent PNG** appears for Qwen; other presets explain that they produce opaque output. A draft's transparency or styles do not silently carry into the refinement stage: choose that stage's supported settings explicitly. ERNIE has no image-reference or LoRA workflow and is available as a text-only draft option, not as a reference-editing refinement model.

This second pass is semantic image editing. Even an instruction to preserve the scene can alter small shapes, text, materials or facial details. FLUX and Qwen reference editing do not provide Z's variation-strength control. The tested Klein 9B portrait retained the person and studio layout broadly, but changed small details and did not correct malformed lettering. Compare the result before keeping it.

**Compare larger** gives the two images more space. **Fit**, **100%** and the zoom buttons set their viewing scale; drag either image to pan both. At 100%, one output pixel occupies one display pixel in the comparison, and differently sized images retain their different sizes. These views load the full-resolution PNGs, not thumbnail enlargements. While focused on either image, use **1** for 100%, **F** for Fit, **+ / −** for zoom and arrow keys to move the comparison position. **Back to settings** restores the stage controls.

Expand **Saved recipes** to keep a named combination of both models, precision choices, prompts, dimensions, steps, guidance, seeds, styles, transparency and optional upscale settings. **Save recipe** updates a recipe with the same name; **Load** restores its settings and **Delete** removes only that recipe. Up to 40 recipes are stored in the current browser profile. Draft images and references are chosen separately and are not embedded in a recipe. Loading checks model, precision and adapter availability and reports missing dependencies instead of substituting another model. Remove or reinstall unavailable adapters before running; an unavailable saved upscale remains off until its model is ready. Editing loaded settings does not change the saved recipe until you save it again.

**Finish with SeedVR2 upscale** is optional and starts off. When enabled, the generation size becomes **Refinement canvas**, followed by a separate **Upscaled output** size: 3840- or 4096-pixel long edge, or custom dimensions. This final stage enlarges the refined image using SeedVR2 7B FP16 base. The **Upscale selected draft only** and **Upscale refined image only** buttons run just that stage and show the computed output size.

SeedVR2's tested 3840 x 2160 portrait was visibly sharper than a Lanczos enlargement, especially in fabric, eyelashes and ceramic edges. It also synthesized fine skin and surface texture and added a slight outline to dark lettering. Treat the result as optional enhancement, not recovered ground truth. Alpha is resampled and preserved exactly; existing cutout fringes still need Cutout refinement. Our quality review covers one portrait and one transparent asset, not every photograph or subject. See [the measured SeedVR2 limits](SEEDVR2.md).

The generated-image library keeps results available independently of the current editor document. Open **Library…** or **File > Generated image library…**, filter or refresh the results, then choose **Use as draft** or **Open in editor**. Deleting selected items or clearing the library requires the dialog's explicit confirmation and removes cached library entries; it does not delete source files, models or saved projects. See [the library guide](GENERATION-LIBRARY.md).

During generation, the status can show ComfyUI's current stage, elapsed time and sampling steps. If the engine does not report fresh progress, the app keeps its elapsed-time message; it does not invent a completion percentage or estimated finish time.

Related: [Cutout workspace](CUTOUT-WORKSPACE.md), [model installation and native workflows](GEN-MODELS.md), [Qwen Image 2.1 details](QWEN-IMAGE-21.md), [stock library](STOCK-LIBRARY.md).


Sampling sources (checked 2026-09-30): [Qwen publisher defaults](https://github.com/QwenLM/Qwen-Image-2.1#default-parameters), [Z-Image ComfyUI template](https://github.com/Comfy-Org/workflow_templates/blob/main/templates/image_z_image_turbo.json), [BFL Klein](https://github.com/black-forest-labs/flux2#the-klein-family), [ERNIE Base](https://huggingface.co/baidu/ERNIE-Image#recommended-parameters).
