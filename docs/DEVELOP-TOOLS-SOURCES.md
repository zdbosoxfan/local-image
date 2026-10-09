# Develop tools: assessment and sources

What we assessed for the Library and Develop modules, what we built, and where each tool came from.
The full design is in [`DEVELOP-DESIGN.md`](DEVELOP-DESIGN.md). [`PORTS.md`](PORTS.md) lists every
ported file with the upstream commit it came from. Notices are in [`licenses/`](../licenses).

Status as of 2026-10-09.

## 1. The assessment

### What Local Image had before this round

| Area | What was there | The problem |
|---|---|---|
| Develop engine | LightCraft's pipeline (`lc-pipeline`): linear Rec.2020, cached by stage, with a GPU path | Good base. RGB only, no alpha |
| Camera Raw Filter in Compositing | A second, simpler engine on sRGB pixels | No masks, optics, geometry or profiles; results differed from the Library |
| Local adjustments | Masks with a subset of the tools | No curve, HSL, grading or point colour per mask; not Capture One-style |
| AI masks in the Library | Subject, Sky and Background were classical heuristics | Weaker than Compositing's Select Subject |
| Removal in the Library | Clone, Heal and classical Remove, dust finder, red eye | No AI Remove |
| Library → Compositing | Rendered a 16-bit TIFF copy | TIFF copies, develop baked in, raw opened without its settings or EXIF/XMP |

### What other editors do (research summary)

* **Capture One:** layers are settings plus one mask plus opacity, and most tools work on a layer. No generative removal. Layers never survive a hand-off to Photoshop.
* **Lightroom:** AI masks (Subject, Sky, Background, Objects, People). Remove is generative, content-aware, heal or clone. Heavy AI results live in a sidecar, not in TIFF copies. "Open as Smart Object" embeds the raw plus its settings.
* **Photoshop:** the Camera Raw Filter is the same UI minus the raw-only tools, used as a re-editable smart filter.
* **Affinity Photo, Pixelmator Pro, ON1, Luminar:** a re-developable RAW layer in the layer stack.
* **darktable** (GPL-3): any module can be masked, and it has the best scene-referred algorithms.
* **RawTherapee and ART** (GPL-3): the best demosaic and capture sharpening.
* **vkdt** (BSD-2): GPU local laplacian, OIDN denoise, wavelet highlights.
* **RapidRAW** (AGPL-3.0): Lightroom-like masks. Used as a design reference only; none of its code was copied.
* **None of the open-source editors has a raw smart object.** That is new in Local Image.

### Decisions

* **One develop engine everywhere.** Library Develop, Develop layers in Compositing, and the Camera Raw Filter all render through `lc-pipeline`.
* **darktable: port, don't track.** We don't vendor darktable or pull its updates automatically. It is C/GTK/OpenCL, so it can't be merged into a pure-Rust app. Instead:
  * each algorithm is ported to Rust, and its upstream path and commit are recorded in `PORTS.md`;
  * `cargo xtask upstream-check` lists upstream changes since the ported commit, so updates are reviewed and re-ported on purpose.
* **Reuse before writing new code.** We use existing libraries and the app's existing paths where they exist: the `lensfun` crate, PSD smart objects and the Camera Raw Filter mapping, Compositing's AI engines, and the model downloader.
* **Licences:** only GPL-3.0-or-later or more permissive code is ported. The whole app is GPL-3.0-or-later.

## 2. Each tool and where it came from

**How** says how each tool came in:
* **Port:** an algorithm re-implemented in Rust from the named source file.
* **Library:** a Rust crate we depend on.
* **Model:** a downloaded network weight file. Our code runs it.
* **Concept:** the behaviour is modelled on another product; the code is our own.
* **Existing:** already in LightCraft or PhotoCraft, the MIT OR Apache-2.0 projects Local Image grew from.

### Raw processing

| Tool | Status | How | Source | Licence | Our file |
|---|---|---|---|---|---|
| AHD, PPG, bilinear, X-Trans demosaic | Existing | Existing | LightCraft | MIT OR Apache-2.0 | `crates/lc-raw/src/demosaic/` |
| RCD demosaic | Built | Port | darktable `src/iop/demosaicing/rcd.c` (credits RawTherapee / Luis Sanz Rodríguez) | GPL-3.0-or-later | `crates/lc-raw/src/demosaic/rcd.rs` |
| Dual demosaic (RCD + bilinear) | Built | Port | darktable `src/iop/demosaicing/dual.c`, `src/develop/masks/detail.c` | GPL-3.0-or-later | `crates/lc-raw/src/demosaic/dual.rs` |
| Highlights: Reconstruct | Existing | Existing | LightCraft | MIT OR Apache-2.0 | `crates/lc-raw/src/highlight.rs` |
| Highlights: Inpaint Opposed | Built | Port | darktable `src/iop/hlreconstruct/opposed.c` | GPL-3.0-or-later | `crates/lc-raw/src/highlight/opposed.rs` |
| Capture sharpening (auto radius) | Built | Port | darktable `src/iop/demosaicing/capture.c`, radius estimation (originally RawTherapee) | GPL-3.0-or-later | `crates/lc-raw/src/capture.rs` |
| Capture sharpening (deconvolution) | Built | Port | darktable `src/iop/demosaicing/capture.c`, Richardson–Lucy deconvolution and blend mask | GPL-3.0-or-later | `crates/lc-pipeline/src/capture.rs` |

### Optics and geometry

| Tool | Status | How | Source | Licence | Our file |
|---|---|---|---|---|---|
| Embedded DNG lens corrections, Upright, manual optics | Existing | Existing | LightCraft | MIT OR Apache-2.0 | `crates/lc-pipeline/src/optics.rs`, `geometry.rs` |
| Lens profiles: database, lookup, interpolation | Built | Library | [`lensfun`](https://crates.io/crates/lensfun) crate 0.7.0 (pure Rust), with the LensFun database | Code LGPL-3.0-or-later; database CC BY-SA 3.0 | `crates/lc-engine/src/lens_db.rs` |
| Lens profiles: distortion, TCA, vignetting models | Built | Port | LensFun `libs/lensfun/mod-coord.cpp`, `mod-subpix.cpp`, `mod-color.cpp`, `modifier.cpp` | LGPL-3.0-or-later | `crates/lc-pipeline/src/lensdb.rs` |

### Tone and colour

| Tool | Status | How | Source | Licence | Our file |
|---|---|---|---|---|---|
| Light, colour, curves, HSL, grading, effects, local tone, dehaze, NR | Existing | Existing | LightCraft | MIT OR Apache-2.0 | `crates/lc-pipeline/` |
| Tone equalizer (9 zones, guided-filter mask) | Built | Port | darktable `src/iop/toneequal.c`, `src/iop/gaussian_elimination.h`, `src/common/luminance_mask.h` | GPL-3.0-or-later | `crates/lc-pipeline/src/toneeq.rs` |
| Colour calibration (CAT16 / Bradford / XYZ, gamut compression) | Built | Port | darktable `src/iop/channelmixerrgb.c`, `src/common/chromatic_adaptation.h` | GPL-3.0-or-later | `crates/lc-pipeline/src/colorcal.rs` |
| Film negative conversion | Built | Port | darktable `src/iop/negadoctor.c` | GPL-3.0-or-later | `crates/lc-pipeline/src/negative.rs` |
| Film Simulation looks (8 profiles) | Built | Our own | Profile deltas on the existing profile system. Generic names only; no vkdt code | GPL-3.0-or-later (ours) | `crates/lc-pipeline/src/profiles.rs` |

### Masks and selections

| Tool | Status | How | Source | Licence | Our file |
|---|---|---|---|---|---|
| Brush, linear, radial, colour and luminance range masks | Existing | Existing | LightCraft | MIT OR Apache-2.0 | `crates/lc-pipeline/src/masks.rs` |
| Object and Describe masks (SAM 3, optional) | Existing | Model | LightCraft's SAM 3 integration | per model | `crates/lc-engine/src/segment/` |
| Quick Subject and Background | Built | Model | IS-Net (Qin et al. 2022), file from the rembg release; or a custom ONNX model the user picks (Settings › Local AI) | Apache-2.0 | `crates/li-seg`, `crates/lc-engine/src/quickseg.rs` |
| Quick Sky | Built | Model | PP-MobileSeg (PaddleSeg, trained on ADE20K) | Apache-2.0 | `crates/li-seg` |
| Depth masks | Built | Model | Depth Anything V2 Small (Yang et al. 2024; ONNX export by fabio-sim) | Apache-2.0 | `crates/li-seg`, `crates/lc-engine/src/quickseg.rs` |
| Capture One-style develop layers (any tool per mask, opacity) | Built | Concept | Capture One's layers. Blending inspired by darktable's "any module is maskable" | ours | `crates/lc-pipeline/src/layers.rs`, `crates/lc-develop` |

### Removal and AI

| Tool | Status | How | Source | Licence | Our file |
|---|---|---|---|---|---|
| Clone, Heal, Remove (classical), dust finder, red eye | Existing | Existing | LightCraft | MIT OR Apache-2.0 | `crates/lc-pipeline/src/spots.rs` |
| AI Remove in Develop | Built | Our own + Concept | Reuses Compositing's AI Remove engines (Qwen via ComfyUI, or Standard CPU). The patch store follows Lightroom's idea of keeping AI results beside the catalog instead of in TIFF copies | ours | `crates/lc-engine/src/enhance/`, `crates/lc-pipeline/src/patches.rs` |

### Develop ↔ Compositing

| Tool | Status | How | Source | Licence | Our file |
|---|---|---|---|---|---|
| Develop layer (raw smart object in Compositing) | Built | Concept | Lightroom's "Open as Smart Object", Affinity's RAW layer | ours | `crates/pc-engine/src/develop_layer_cmds.rs` |
| Camera Raw Filter on the shared engine | Built | Concept + Existing | Photoshop's Camera Raw Filter as a smart filter. The Photoshop descriptor mapping is PhotoCraft's existing `camera_raw_map.rs` | ours; MIT OR Apache-2.0 | `crates/pc-io/src/develop_filter.rs`, `crates/pc-engine/src/develop_filter_cmds.rs` |
| Develop layers in PSD | Built | Existing | Existing PSD smart-object and Camera Raw Filter export, plus a private `localImage` record | MIT OR Apache-2.0 + ours | `crates/pc-io/src/smart_map.rs` |
| Layers panel in Library and Develop | Built | Our own | Reads files with the existing PSD and `.pcraft` importers | ours | `crates/lc-ui-egui/src/panels/doc_layers.rs`, `apps/local-image/src/doc_layers.rs` |

## 3. Not built yet, and why

| Tool | Would come from | Why not this round |
|---|---|---|
| AMaZE demosaic | RawTherapee / ART (GPL-3) | About 1,500 lines of tightly coupled C. Its gain over RCD is small and only on the finest periodic detail. Not measured |
| Segmentation-based highlights | darktable `segbased` (GPL-3) | Works on the raw mosaic before demosaic, so binned previews wouldn't match the full-size result. Inpaint Opposed covers the common case |
| Diffuse or sharpen | darktable (GPL-3) | Tens of wavelet iterations: too slow on the CPU at preview sizes without a GPU port |
| VNG4 inside dual demosaic | darktable / RawTherapee (GPL-3) | Small visual difference in flat areas; bilinear is used |
| EIGF mask for the tone equalizer | darktable (GPL-3) | The existing guided filter gives a similar mask |
| Colour checker calibration, colour balance rgb, colour equalizer | darktable (GPL-3) | Not on this round's list |
| LaMa inpainting as a local AI Remove engine | LaMa (Apache-2.0) | Later; AI Remove uses Compositing's engines for now |
| OIDN or vkdt `jddcnn` denoise | OIDN (Apache-2.0), vkdt (BSD-2) | Alternatives to RawNIND; not needed now |

## 4. Still to check

* **GPU path** (on the owner's machine): lens profiles with crop and rotate, the tone equalizer's switch to the CPU, the linear colour calibration modes against the CPU, capture-sharpening refresh, depth masks, film looks, and switching demosaic and highlight modes back to Default.
* **AI Remove in Develop** has run only against a mock engine in tests.
* **Follow-ups:**
  * the tone equalizer as a develop-layer tool;
  * GPU ports of the tone equalizer, non-linear colour calibration and lens-database sampling;
  * cleanup of unused stored AI results;
  * AI removals following a rotate;
  * depth masks in the Camera Raw Filter.
