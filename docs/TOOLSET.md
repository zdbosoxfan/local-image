# Local Image 2 — the raster tool set

How Local Image's pixel tools were chosen: PhotoCraft's toolset was audited hands-on (every tool
run on real photos and the output inspected), then compared tool by tool with GIMP 3, Krita 5,
Photopea, Compositor/OmaPhoto, PaintFE, Pinta and Graphite. For each tool we keep the best
implementation, or port the better one into Rust. Vector, shape and type tools are out of scope for
now (they stay as PhotoCraft ships them).

Licensing: Local Image is GPL-3.0-or-later, so code from GIMP/GEGL (GPL/LGPL-3+), Krita (GPL-2+),
darktable (GPL-3+), digiKam (GPL-2+) and the permissive projects (PhotoCraft and LightCraft
MIT/Apache-2.0, Compositor/OmaPhoto/PaintFE/Pinta MIT, Graphite Apache-2.0) may be ported, with
notices kept in `licenses/`. Photopea is proprietary: behaviour only.

**Status key:** ✅ shipped in this branch · 🔧 improved in this branch · 📋 planned (next) ·
— not planned.

## Headline findings

1. PhotoCraft already uses the right published algorithms almost everywhere (PatchMatch content-aware
   fill, Poisson healing, graph-cut quick selection, guided-filter refine edge, puppet/perspective
   warp, seam carving, a Photoshop-grade brush engine and layer model). **The gaps are output
   quality, a few bugs, interactive polish and learned (ML) segmentation**, not missing tools.
2. Every reference editor that does Select Subject / Remove Background well uses a learned model
   (Photoshop, Photopea and Compositor server- or OS-side; OmaPhoto and PaintFE run U²-Net-class
   models locally). PhotoCraft's classical saliency + GrabCut heuristic cut holes in subjects.
3. Hair-grade edges need real alpha matting (GIMP's Foreground Select engines). Nothing in the
   PhotoCraft family has it.

## The condensed tool set

### Selection

| Tool | Implementation in Local Image | Status |
| --- | --- | --- |
| Marquee, Lasso, Polygonal, Magnetic Lasso | PhotoCraft (audit: work well) | ✅ |
| Magic Wand | PhotoCraft + Photoshop/Compositor **Sample Size** (point to 101×101 average) | 🔧 |
| Quick Selection | PhotoCraft graph cut + **highlight-aware edges**: specular highlights (dichromatic model: body colour + white) enclosed by the painted colours are filled from their surroundings before the cut, so it no longer stops at them, while a real hole showing the background stays out; **Subject Assist** adds the installed U²-Net/IS-Net map as a weak prior | ✅ |
| Object Selection | PhotoCraft rectangle mode (excellent) + **click mode** from the local segmentation model (OmaPhoto's flood of the 320² probability map) | 🔧 |
| **Select › Subject (AI)** | Qwen Image 2.1 matte via ComfyUI → **closed-form matting** of the edge band | ✅ |
| Select › Subject | **Local U²-Net / IS-Net** on the CPU (`li-seg`, tract, pure Rust) when installed, PhotoCraft heuristic otherwise | 🔧 |
| Select and Mask | PhotoCraft guided-filter refine + **Refine: Matting** (closed-form, He et al. 2010 matrix-free CG; Germer 2020 foreground colours) | 🔧 |
| Color Range, Feather/Expand/Contract/Smooth/Border | PhotoCraft (audit: work well) | ✅ |

### Retouching

| Tool | Implementation | Status |
| --- | --- | --- |
| **AI Remove brush** (R) | FLUX.2 Klein + object-removal LoRA or Qwen via ComfyUI; red-outline input, Poisson blend; repair on its own layer | ✅ |
| Content-Aware Fill | PhotoCraft PatchMatch/EM, **fixed**: full EM up to 400 px holes (was 96), re-synthesis from the upsampled match field each level, sharp final vote (low-cost quantile + spatial falloff), **Poisson seam correction**. Audit showed blurry fills with hard seams before | 🔧 |
| Spot Healing | Same engine; **sampling context widened** to 3× brush (was brush + 8 px, which pasted unrelated texture) | 🔧 |
| Healing Brush, Patch | PhotoCraft (audit: seamless) | ✅ |
| Content-Aware Move | PhotoCraft; the audit's grey halo doesn't reproduce: regression tests cover opaque, feathered and transparent layers | ✅ |
| Clone Stamp | PhotoCraft: Aligned, Sample (current / current & below / all layers), Clone Source panel with five sources, offset, scale and rotation — Photoshop parity (GIMP's Registered/Fixed modes not needed) | ✅ |
| **Red Eye tool** (J group) | Red-pupil region grown from the click (GEGL / Pinta redness rule), holes filled, fitted and feathered; Pupil Size and Darken Amount | ✅ |
| Seamless paste/clone | **Edit › Paste Special › Paste Seamless** and Clone Stamp **Seamless**: mean-value-coordinate cloning from Farbman et al. 2009 (adaptive boundary sampling, holes and separate parts; not GEGL's weights) | ✅ |

### Painting

| Tool | Implementation | Status |
| --- | --- | --- |
| Brush engine | PhotoCraft (Photoshop dynamics) + **centripetal Catmull–Rom stroke path** (Compositor/Krita) so fast curves stay round | 🔧 |
| Soft round brushes | **Continuous coverage** (Compositor's idea): the dab density per unit length at the brush's spacing is integrated along the stroke (Gauss–Legendre, or a cumulative line-integral table for constant segments), so the mean density matches stamping at that spacing with no beading; hard, textured and dynamic brushes still stamp | ✅ |
| Smudge | **Step ≤ max(1 px, 0.5 % of diameter)** (Compositor) to remove ribbing | 🔧 |
| Airbrush build-up | Was a dead button and the UI never sent point times, so Airbrush presets never built up: strokes now carry timestamps and a held airbrush keeps depositing | ✅ |
| Smoothing options | Was a dead button: Pulled String, Stroke Catch-up, Catch-up on Stroke End, Adjust for Zoom (the engine had them) | ✅ |
| Symmetry | Was a dead button: Vertical, Horizontal, Diagonal and along the work path; Dual Axis, Radial and Mandala need multi-copy compositing | 🔧 |
| Stabilizer | Photoshop Smoothing (amount, Pulled String, catch-up, zoom-adjusted): covers Krita's stabilizer use | ✅ |
| Pen pressure curve | Preferences › Tools › **Pen pressure curve**: Soft / Linear / Firm presets or an edited monotone curve (Krita's global tablet curve), applied to every pen sample | ✅ |
| Blur tool | Photoshop behaviour (brush size and strength) kept; Compositor's separate radius not adopted | ✅ |
| Behind / Clear modes | Photoshop/PaintFE: in the Brush and Pencil Mode lists after Dissolve; Clear follows the Eraser's rules | ✅ |

### Transforms and crop

| Tool | Implementation | Status |
| --- | --- | --- |
| Free Transform, Warp, Perspective Warp, Puppet Warp | PhotoCraft | ✅ |
| Liquify | PhotoCraft, **rim seam bug fixed** (strength now eases to 0 at the brush edge; was 0.5 at default density) | 🔧 |
| Distort with folded/concave corners | Compositor's **two-triangle fallback**: a concave or self-crossing quad draws as two affine triangles (preview and result) instead of wrapping through the horizon; smart objects keep it as a fitted warp | ✅ |
| Cage transform | **Edit › Transform › Cage**: draw a cage, ↩ to deform, drag its points, ↩ to commit; Green coordinates (Lipman et al. 2008) or mean value coordinates; pixel layers, linked masks, smart objects (as a smart filter) | ✅ |
| Crop straighten | Options-bar angle, tilted frame preview with thirds; rotate + crop in one undo step | 🔧 |
| Crop overlays | Was a dead button: Rule of Thirds, Grid, Diagonal, Triangle, Golden Ratio, None | ✅ |
| Perspective Crop | PhotoCraft homography + PaintFE's UI: drag a frame, drag its corners, ↩ straightens | ✅ |

### Filters and adjustments

| Item | Implementation | Status |
| --- | --- | --- |
| Blurs, distortions, Filter Gallery, 16 adjustment layers, layer styles | PhotoCraft (audit: work well) | ✅ |
| Unsharp Mask / High Pass / Smart Sharpen | **Fast Gaussian path** (running-sum boxes; was a direct O(r) kernel, 129 s vs 4 s on 24 MP) | 🔧 |
| Reduce Noise | Luminance by **sliding 8×8 DCT shrinkage** (Yu & Sapiro 2011, GEGL `denoise-dct`) with sparsity-weighted aggregation; guided-filter chroma kept. NL-means next | 🔧 |
| Frequency separation | GIMP Wavelet Decompose idea: Filter › Other › Frequency Separation makes Low + High (Linear Light) layers | ✅ |
| Median / Dust & Scratches | **Sliding histogram** per row with a running median pointer (Huang, as Pinta), O(r) per pixel instead of an O(r²) gather and sort; exact path kept for r ≤ 2 and HDR values | ✅ |
| Image Size Bicubic Smoother/Sharper/Automatic | Mitchell–Netravali (Smoother), Keys a = −0.75 (Sharper), Automatic picks by direction (they used to fall through to plain bicubic) | 🔧 |
| Camera raw | **LightCraft pipeline** for raw files (DNG matrices/profiles, colour fitted to the embedded JPEG for ARW/NEF/RW2, highlight reconstruction) → 16-bit ProPhoto document | ✅ |

### AI generation and AI compositing

| Feature | Source | Status |
| --- | --- | --- |
| Generate panel: Create, Edit, Fill, Refine, Upscale with any installed model; presets; LoRAs; Draft → Refine | Krita AI Diffusion (ported), Photoshop names | ✅ |
| Family profiles as data (SD 1.5, SDXL, Pony, Illustrious, SD 3.5, FLUX.1/Kontext/Fill, FLUX.2, Klein, Qwen Image/Edit/2.1, Z-Image, ERNIE, HiDream) with live updates | Krita's per-architecture rules, ComfyUI templates | ✅ |
| Installed-model detection from `/object_info` and safetensors/GGUF headers | Krita's and ComfyUI's model detection, safetensors spec | ✅ |
| Inpaint geometry (grow, feather, context crop, blur pre-fill, green-fill instruction) | Krita AI Diffusion (ported) | ✅ |
| Custom workflows (`li:` markers; editor or API format, subgraphs flattened) and official templates with basic controls | Krita's custom workflows, ComfyUI templates | ✅ |
| Model Browser (Hugging Face, Civitai, templates, ComfyUI-Manager list; verified installs) | SwarmUI and InvokeAI model managers (ideas) | ✅ |
| Library tab (0.7-compatible generated library) | Local Image 0.7 | ✅ |
| AI Cutout tool: Remove Background (AI or Quick CPU), refine Erase/Restore, Add Background (colour, image, generated) | Local Image 0.7, PhotoCraft | ✅ |
| Generative Fill, Generate Background, AI Enhance (SeedVR2) | Local Image 0.7 | ✅ |
| Batch Remove Backgrounds | Local Image 0.7 | ✅ |
| Cloud generation with your own API key (OpenAI GPT Image, Google Gemini image, Black Forest Labs FLUX pro, Stability AI) | Provider APIs; fills composite through the feathered selection | ✅ |
| Contextual Task Bar under a new selection or closed pen path: **Remove (AI)**, Generative Fill, Invert, Mask, Deselect | Photoshop | ✅ |
| Remove Background: the editor's own (CPU) and Qwen AI side by side (Cutout tool, Properties › Quick Actions) | PhotoCraft + Local Image | ✅ |

## Cataloguing

**Shipped in this branch:** the Library mode. One window, two modes switched from the title bar
(**Library | Editor**). The Library is LightCraft's UI and catalog (`~/Pictures/Local Image Library`,
or `LOCAL_IMAGE_LIBRARY`); it keeps importing, exporting and saving while the editor is shown.
**Edit in Local Image** renders a 16-bit TIFF stacked with the original and opens it in the editor
in-process; switching back reloads the edit. The app reopens in the mode it was closed in.

LightCraft already implements nearly all of Lightroom's Library module (import, grid/loupe/compare/
survey, ratings/flags/labels, hierarchical keywords, metadata, filter bar, albums, smart albums,
stacks, virtual copies, sync, batch rename, export presets, XMP sidecars, Edit-in round trip). Local
Image hosts it as the **Library** mode next to the PhotoCraft **Edit** mode, with a shared filmstrip
in the editor. Planned improvements (from darktable/digiKam): incremental queries off the UI thread
(50k-photo responsiveness), a catalogued Folders panel with Synchronize, a similarity/duplicate
index, darktable-style collections browser, sidecar change detection.

## Sources

The full research notes (feature matrices, file and line references into each project, algorithm
details) are summarised from: a hands-on audit of PhotoCraft; Compositor and OmaPhoto (Swift/C++,
MIT); PaintFE (Rust, MIT), Pinta (C#, MIT), Graphite (Rust, Apache-2.0); GIMP/GEGL and Krita
source and manuals; Photopea's published behaviour; LightCraft, darktable and digiKam for
cataloguing. Key papers: Wexler et al. 2007 (space-time completion), Barnes et al. 2009
(PatchMatch), Pérez et al. 2003 (Poisson editing), Levin et al. 2008 (closed-form matting), He et
al. 2010 (fast matting), Germer et al. 2020 (foreground estimation), Qin et al. 2020/2022
(U²-Net, IS-Net).

## Model system provenance

What the open model system takes from each source (notices in `licenses/model-system-NOTICE.md`):

| Source | Licence | How it is used | Where |
| --- | --- | --- | --- |
| ComfyUI | GPL-3.0 | The engine, over HTTP: `/object_info` (nodes, file lists, input order), `/prompt`, `/history`, `/view`, `/upload/image`, `/templates` | `comfy.rs`, `ops.rs` |
| Krita AI Diffusion | GPL-3.0 | Ported: workflow construction per architecture, inpaint geometry and green fill, Pony/Illustrious prompt rules, style presets, editor-to-API conversion through `/object_info` | `builders.rs`, `inpaint.rs`, `custom.rs`, `presets.rs`, `families/*.json` |
| ComfyUI workflow templates | MIT | Per-model graphs and their required files (`properties.models`); unknown families run through them with basic controls; ten are test fixtures | `browser.rs`, `custom.rs`, `tests/fixtures/templates` |
| Hugging Face Hub API | service terms | Search (`/api/models`), file trees with LFS SHA-256 and sizes, `resolve` downloads; optional token | `browser.rs`, `download.rs` |
| Civitai API | service terms | Search (`/api/v1/models`), published SHA-256, size checked against the download's headers, downloads through its R2/B2 delivery; optional key | `browser.rs`, `download.rs` |
| ComfyUI-Manager `model-list.json` | GPL-3.0 (data read at run time) | A fallback community catalogue; installs only when Hugging Face publishes the hash | `browser.rs` |
| safetensors / GGUF headers | Apache-2.0 / MIT formats | Tensor names, shapes and ModelSpec metadata identify the family of installed files | `arch.rs` |
| SwarmUI, InvokeAI | MIT, Apache-2.0 | Ideas only: model manager layout, architectures as data | `model_browser.rs` |
| This repository | GPL-3.0 | Family profile updates, fetched by *Update Model Profiles* | `family.rs` |

## Dead controls found and fixed

PhotoCraft looked complete but several controls drew without doing anything. An audit for buttons
whose clicks were ignored found eight, all now working: the airbrush, smoothing options and
symmetry buttons (brush options bar), the crop overlay button, the tool-preset button (opens Tool
Presets), and the History panel's Delete Current State, New Snapshot and New Document from Current
State (snapshots are listed above the states; click restores, right-click deletes).

