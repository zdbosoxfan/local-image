# Local Image

A local Windows image workspace with **Retouch**, **Cutout** and **Image Gen** personas. Version **0.6.0** adds deployment for other Windows PCs, with selectable installation and AI storage folders. The flat, canvas-first interface uses conventional menus, contextual tools, tabbed Studio panels and a compact bottom filmstrip. One image keeps the filmstrip hidden; multiple images or an imported folder show it.

- **Retouch:** CPU Quick Heal and local AI object removal. Brush, pen and geometric selections; editable repair layers.
- **Cutout:** Qwen background removal, brush/pen refinement, subject movement, scaling and rotation, editable shadows and imported or generated backgrounds.
- **Image Gen:** text-to-image and model-specific image inputs, reference imports, repeatable seeds, dimensions and optional LoRAs. Generated results can continue into Retouch or Cutout, or become another document's background. Draft & Refine compares stages side by side, with a persistent image library and optional SeedVR2 enlargement.

| Model | Best suited to | Image inputs | Transparent generation |
| --- | --- | --- | --- |
| Qwen Image 2.1 | Edits and transparent assets | Up to 10 semantic references | Yes, native RGBA |
| Z-Image Turbo | Fast generation | One starting image for latent variations | No |
| FLUX.2 Klein 4B | Fast reference editing | Up to 4 semantic references | No |
| FLUX.2 Klein 9B | Larger reference-editing model | Up to 4 semantic references | No |
| ERNIE-Image Base | Text-heavy posters and graphic layouts | Text only | No |

Qwen offers Compact INT8 and Full BF16 presets of the same model. Klein 4B uses BF16; Klein 9B uses an FP8 preset. Model availability is checked against the running ComfyUI service. Klein 9B's publisher repository currently requires access approval; it remains listed in the model browser but cannot be downloaded anonymously. The UI shows only applicable inputs: for example, Z-Image Turbo exposes variation strength while Qwen exposes transparent output. LoRAs are optional and belong to a specific base model.

ERNIE reproduced all five requested strings in the local poster test, but still missed a no-mockup layout instruction. No model guarantees perfect text or layout. Qwen's opaque generation composites unexpected transparency over white; dedicated background generation rejects substantial transparency instead of accepting an incomplete scene.

## Install and start

Use the Windows installer built under `dist/local-image-v06/installer/Local-Image-Setup-0.6.0.exe`. Choose installation for all users in **Program Files**, or for the current user in **AppData**, and change the application folder if needed. Optional folder choices reserve a location for model downloads and the dedicated portable ComfyUI runtime. Settings, browser data, recovery and caches belong to each Windows user's AppData. See [the installation guide](docs/INSTALLATION.md) for storage, upgrades and setup on a new PC.

The installer includes the app, Python backend and Windows WebView2 host; ComfyUI and model weights are separate optional downloads. In **Edit > Settings**, detect or browse for an existing ComfyUI installation, or install a dedicated runtime. In **Image Gen > Browse models…**, compare strengths, precision, download sizes and GPU memory recommendations, then choose or download a model. **Models folder…** selects storage that can also be shared with an existing ComfyUI. Downloads verify publisher revisions, byte sizes and SHA-256 checksums.

Image Gen's **Output** tab exposes resolution and steps together, with a suggested step count for the chosen model. Optional guidance is under **Advanced sampling**. See the [Image Gen user guide](docs/IMAGE-GENERATION.md) for references, adapters, the two-pane Draft & Refine workspace, and optional SeedVR2 upscaling.

SeedVR2 is a separate optional photo enhancer. Its real 3840 x 2160 test improved apparent detail over Lanczos, but synthesized fine textures and slightly outlined lettering. Review results at full size; transparent output retains the resized source alpha rather than repairing its edges.

The first launch displays a hardware guide with detected GPU memory and planning recommendations for each function. Open **Help > Hardware guide** to see it again. CPU editing and Quick Heal do not require a dedicated GPU. Model file sizes describe disk storage, not VRAM. ComfyUI can offload to system RAM, with a speed cost; large canvases and many references increase memory use.

Fresh profiles use `%LOCALAPPDATA%\Local Image`. An existing `%LOCALAPPDATA%\Local Remove` profile is retained so upgrades preserve settings, recovery sessions, model paths and editable projects. The `.lremove` project format and internal loopback endpoint `http://127.0.0.1:51247/remove` remain compatible. Normal editing runs without administrator rights.

## Workspaces and guides

- [Cutout editing and compositing](docs/CUTOUT-WORKSPACE.md)
- [Installation, AI setup and storage on another PC](docs/INSTALLATION.md)
- [Image generation models and input behavior](docs/GEN-MODELS.md)
- [Generated-image library](docs/GENERATION-LIBRARY.md)
- [Ten reviewed styles and the live LoRA browser](docs/LORA-LIBRARY.md)
- [SeedVR2 upscaling and measured quality limits](docs/SEEDVR2.md)
- [ERNIE poster preset and exact-text evaluation](docs/ERNIE-IMAGE.md)
- [Stock image search, import and attribution](docs/STOCK-LIBRARY.md)
- [Interface references and design decisions](docs/WORKSPACE-REFERENCES.md)
- [Qwen integration details](docs/QWEN-IMAGE-21.md)
- [Build and test instructions](docs/DEVELOPMENT.md)
- [Earlier 0.4.0 cutout validation](docs/QWEN-VALIDATION.md)
- [0.6.0 installation and deployment validation](docs/INSTALLATION-VALIDATION.md)
- [0.5.0 model-quality findings and GPU validation](docs/LOCAL-IMAGE-VALIDATION.md)

Transparent PNG, WebP and RGBA TIFF exports preserve alpha. TIFF editing retains native 16-bit source precision. Projects retain original pixels, repair layers, cutout masks, backgrounds, shadows, transforms and generation parameters. The generated image is portable; model weights and LoRA files are separate dependencies for regenerating it.

The LoRA browser queries Hugging Face for newly published adapters when searched or refreshed. Ten exact-family starting points passed local GPU loading and visual review: three Qwen, two Z, two Klein 4B and three Klein 9B styles. Curated recommendations and unverified community results are labeled separately. Exact model compatibility and licensing still need checking for community adapters. The browser only downloads selected Safetensors files and does not install Python code or custom nodes.

**File > Stock library…** searches Openverse inside the app. Preview images and review their source and license, then open an image, use it as a cutout background, or add it as a generation reference. Imported credits stay with editable projects and are available through **File > Image credits…**. Pexels and Unsplash remain clearly labeled external website links followed by local import. Compositor is a visual/workflow reference for this release; no Compositor code is integrated.

## Model licenses

Qwen Image 2.1 uses the [Qwen Research License](https://github.com/QwenLM/Qwen-Image-2.1/blob/main/LICENSE). FLUX.2 Klein 9B uses a [noncommercial model license](https://huggingface.co/black-forest-labs/FLUX.2-klein-9B/blob/main/LICENSE.md). Z-Image Turbo, FLUX.2 Klein 4B, ERNIE-Image and SeedVR2 are Apache 2.0. Separate LoRAs and stock images have their own licenses. The installer does not bundle model weights or grant additional model rights.
