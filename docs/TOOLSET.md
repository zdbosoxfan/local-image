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
| Magic Wand | PhotoCraft + Compositor's *Sample Size* (3×3/5×5) and PaintFE's 8-way connectivity | 📋 |
| Quick Selection | PhotoCraft graph cut (stops at specular highlights; GEGL `paint-select` reference) | 📋 |
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
| Content-Aware Move | PhotoCraft (grey halo noted by the audit) | 📋 |
| Clone Stamp | PhotoCraft + GIMP's Registered/Fixed alignment modes | 📋 |
| Red Eye tool | GEGL `red-eye-removal` / Pinta rule | 📋 |
| Seamless paste/clone | GIMP Seamless Clone (mean-value coordinates; port from the paper, GEGL's weights have a bug) | 📋 |

### Painting

| Tool | Implementation | Status |
| --- | --- | --- |
| Brush engine | PhotoCraft (Photoshop dynamics) + **centripetal Catmull–Rom stroke path** (Compositor/Krita) so fast curves stay round | 🔧 |
| Soft round brushes | Compositor's continuous analytic coverage (Gauss–Legendre integral of dab density) | 📋 |
| Smudge | **Step ≤ max(1 px, 0.5 % of diameter)** (Compositor) to remove ribbing | 🔧 |
| Stabilizer, pressure curves | Krita stabilizer modes and sensor curves | 📋 |
| Blur tool | Own radius independent of brush size (Compositor) | 📋 |
| Behind / Clear modes | PaintFE | 📋 |

### Transforms and crop

| Tool | Implementation | Status |
| --- | --- | --- |
| Free Transform, Warp, Perspective Warp, Puppet Warp | PhotoCraft | ✅ |
| Liquify | PhotoCraft, **rim seam bug fixed** (strength now eases to 0 at the brush edge; was 0.5 at default density) | 🔧 |
| Distort with folded/concave corners | Compositor two-triangle fallback | 📋 |
| Cage transform | GIMP/Krita (Green coordinates) | 📋 |
| Crop rotate/straighten, Perspective Crop | PhotoCraft homography + PaintFE's UI | 📋 |

### Filters and adjustments

| Item | Implementation | Status |
| --- | --- | --- |
| Blurs, distortions, Filter Gallery, 16 adjustment layers, layer styles | PhotoCraft (audit: work well) | ✅ |
| Unsharp Mask / High Pass / Smart Sharpen | Use the fast Gaussian path (audit: 129 s vs 4 s on 24 MP) | 📋 |
| Reduce Noise | GEGL `denoise-dct`, then NL-means | 📋 |
| Frequency separation | GIMP Wavelet Decompose (detail bands as Linear Light layers) | 📋 |
| Median / Dust & Scratches | Pinta sliding-histogram (O(r)) | 📋 |
| Image Size Bicubic Smoother/Sharper | Real kernels (currently silently plain bicubic) | 📋 |
| Camera raw | **LightCraft pipeline** for raw files (DNG matrices/profiles, colour fitted to the embedded JPEG for ARW/NEF/RW2, highlight reconstruction) → 16-bit ProPhoto document | ✅ |

### AI generation and AI compositing

| Feature | Status |
| --- | --- |
| Generate panel (Create / Edit image / Fill selection; Qwen, Z-Image, Klein 4B/9B, ERNIE) | ✅ |
| Library tab (0.7-compatible generated library) | ✅ |
| AI Cutout tool: Remove Background (AI or Quick CPU), refine Erase/Restore, Add Background (colour, image, generated) | ✅ |
| Generative Fill, Generate Background, AI Enhance (SeedVR2) | ✅ |
| Batch Remove Backgrounds | ✅ |

## Cataloguing

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
