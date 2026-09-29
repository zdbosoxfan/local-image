# Image Gen in Local Image

Choose **Image Gen** in the top persona toolbar to create a new image. The right studio has **Prompt**, **References** and **Output** tabs. Generated images open as normal documents: use **Retouch result**, **Cut out result**, or export them through the File menu.

## Pick a model and describe the result

The model menu includes a short description of each model's strengths. Choose **Browse models…** to compare all supported models before downloading: strengths, limitations, precision, full and remaining download size, recommended steps, GPU memory guidance and a license link. These catalogue descriptions are available even when ComfyUI is offline. **Use this model** selects it; **Download model** installs its selected preset. You can also expand **Model setup** in the Prompt tab to download the current preset.

Choose **Models folder…** in the model browser to select the shared download folder. Files already present there and models visible to the running ComfyUI are separate statuses. If you change the folder while ComfyUI is running, close ComfyUI, then choose **Start AI backend** from Local Image so it launches with the new folder configuration; refresh the connection afterward. Downloading to a folder does not change another running ComfyUI process's model paths. The app does not terminate that process automatically.

| Model | Useful for | Reference input | Default sampling |
| --- | --- | --- | --- |
| Qwen Image 2.1 | Image edits, new images and transparent assets | Up to 10 ordered images | 25 steps, guidance 1 |
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

Open **Output** and choose a canvas shape or enter custom dimensions. Each side must be 256–4096 pixels in multiples of 32, and total area must not exceed 4,194,304 pixels. Large images and multiple references need more GPU memory. Smaller canvases help test models quickly.

For Qwen, select **Transparent background** to request a real RGBA image. Local Image checks for a usable alpha channel and reports a failure if the model returns an opaque result. Generated edges can still need refinement in Cutout. Export transparent output as PNG; JPEG cannot preserve alpha.

With transparency off, any transparent Qwen pixels are composited over white. This prevents hidden RGB from becoming a colored backdrop, but a white fallback does not mean the model generated the requested scene. The dedicated **Generate empty background** operation rejects substantial transparency and asks you to retry instead of treating a transparent subject as a finished background.

Leave **Seed** empty for a fresh random seed, or enter a number, including zero, to repeat sampling with the same inputs and settings. The result reports the actual seed. Reproducibility can also depend on the model files, ComfyUI version and hardware.

**Steps** is shown directly on the Output tab, with the recommended default for the selected model. Width and height remain editable on that tab. Expand **Advanced sampling** to change supported guidance. Qwen's negative prompt has no effect at guidance 1; higher guidance is experimental. Z and the two FLUX presets do not use negative prompts in these workflows. Klein and Z have fixed guidance 1.

## Add an optional LoRA

Expand **LoRAs** on the Prompt tab and choose **Open LoRA library…**. **Installed** lists adapters assigned to the selected model. Choose **Use**, then set the strength beside the selected adapter; × removes it. Up to three adapters may be selected. Strength can range from −2 to 2, and zero has no effect. Start with the publisher's recommended value.

**Browse** searches current Hugging Face results. Open a repository to review its files, model page and compatibility. Downloading an adapter does not select it automatically. Return to Installed and choose Use. Changing the base model clears the selected adapters.

Adapters are restricted to their assigned base model. An adapter for the original Qwen Image, another FLUX family, or a different Klein size is not automatically compatible. Unverified community files require an explicit model assignment in the library. See [LoRA library details](LORA-LIBRARY.md).

Some acceleration adapters need particular sampling values. **Apply recommended settings**, when offered, changes those values explicitly; it does not enable the adapter. Ordinary LoRAs leave the chosen sampling values alone. The app applies adapters to the diffusion model only, leaving the text encoder and VAE unchanged.

## Continue editing or use the result as a background

**Generate image** creates a new unsaved document. **Retouch result** opens the normal retouch tools; **Cut out result** opens the Cutout studio. The original reference documents remain available.

To generate a replacement background, start from a document with an enabled cutout, switch to Image Gen, describe an empty scene and generate. **Use as background** then places the generated image behind that original subject and returns to Cutout. Subject position, scale, rotation and shadow remain editable there. For the dedicated Qwen empty-scene prompt, use **Generate empty background** directly in Cutout's Background tab.

Save a `.lremove` project to preserve image data, cutout composition, repair layers and generation settings. Reopening a generated project restores its model, precision, prompt, seed, dimensions and sampling controls. Projects from retired models, including HiDream and FLUX Dev, retain their saved identity and show that it is unavailable; the app does not silently select a replacement. Reference images and LoRA weights are not bundled with the generated result's project. The app shows the saved reference count and asks you to reattach those images; unavailable saved adapters must be removed or reinstalled before generating again. Switching personas preserves your current prompt draft. Export creates a flattened image for other software.

## Draft & Refine and optional upscaling

Choose **Draft & Refine…** in the Image Gen context toolbar. The two-pane dialog keeps **Draft** on the left and **Refined image** on the right for comparison. Select a draft from the current document or from **Library…**, or generate one using the separate **Draft model** and **Draft prompt** controls. Draft size, sampling and references are available in the draft settings disclosure.

Choose a **Refinement model** and edit the visible **Refinement prompt** to say what should remain and what should improve. **Final resolution**, **Width**, **Height** and **Steps** are editable, with a model-specific recommendation. For example, a fast Z or Klein draft can be used as a reference for Qwen or Klein 9B. Refinement creates a separate result; it does not overwrite the selected draft.

This second pass is semantic image editing. Even an instruction to preserve the scene can alter small shapes, text, materials or facial details. FLUX and Qwen reference editing do not provide Z's variation-strength control. The tested Klein 9B portrait retained the person and studio layout broadly, but changed small details and did not correct malformed lettering. Compare the result before keeping it.

**Finish with SeedVR2 upscale** is optional and starts off. When enabled, the generation size becomes **Refinement canvas**, followed by a separate **Upscaled output** size: 3840- or 4096-pixel long edge, or custom dimensions. This final stage enlarges the refined image using SeedVR2 7B FP16 base. The **Upscale selected draft only** and **Upscale refined image only** buttons run just that stage and show the computed output size.

SeedVR2's tested 3840 x 2160 portrait was visibly sharper than a Lanczos enlargement, especially in fabric, eyelashes and ceramic edges. It also synthesized fine skin and surface texture and added a slight outline to dark lettering. Treat the result as optional enhancement, not recovered ground truth. Alpha is resampled and preserved exactly; existing cutout fringes still need Cutout refinement. Our quality review covers one portrait and one transparent asset, not every photograph or subject. See [the measured SeedVR2 limits](SEEDVR2.md).

The generated-image library keeps results available independently of the current editor document. Open **Library…** or **File > Generated image library…**, filter or refresh the results, then choose **Use as draft** or **Open in editor**. Deleting selected items or clearing the library requires the dialog's explicit confirmation and removes cached library entries; it does not delete source files, models or saved projects. See [the library guide](GENERATION-LIBRARY.md).

Related: [Cutout workspace](CUTOUT-WORKSPACE.md), [model installation and native workflows](GEN-MODELS.md), [Qwen Image 2.1 details](QWEN-IMAGE-21.md), [stock library](STOCK-LIBRARY.md).
