# Develop everywhere: one engine, develop layers, AI in the Library

Status: **in progress** (October 2026). Decisions taken with the owner are in §0. Answers: how Lightroom-style develop, Capture One-style layers,
AI masks and AI Remove work in the Library *and* the Editor, without TIFF round-trips, with settings
and metadata kept re-editable, and with a Camera Raw Filter that has the same tools.

## 0. Decisions

* **Modules, not "Edit in…":** the title bar switches **Library | Develop | Compositing** (⌘⌥1 / ⌘⌥2 / ⌘⌥3), as Lightroom's module picker does. Develop → Compositing carries the photo across as a Develop layer; Compositing → Develop goes to the active Develop layer's photo; double-clicking a Develop layer goes to Develop. *(Shipped: module switch, Develop layers, live follow on return from Develop.)*
* **Keymaps per module:** Library and Develop keep Lightroom Classic's keys, Compositing keeps Photoshop's; only the module switch is global.
* **Develop layers follow the Library** (live link): coming back from Develop re-renders every Develop layer of that photo, one undo step per document. The layer stores the settings too, so a document opened without the Library still renders.
* **darktable: port, don't track.** We do **not** vendor darktable or pull its updates automatically: it is C/GTK/OpenCL built around its own pipeline API, so its code can't be merged into a pure-Rust app mechanically, and depending on it would break "all Rust, standalone". Instead each algorithm is ported to Rust with its upstream path and commit recorded in `docs/PORTS.md`; `cargo xtask upstream-check` lists upstream commits that touched those files since the recorded commit, so improvements are reviewed and re-ported deliberately.
* **Film negatives:** darktable's negadoctor, ported (Develop › Negative).
* **Quick selections:** Subject, Background (U²-Net family) and Sky (PP-MobileSeg, Apache-2.0, 24 MB; TinySkyNet 0.2 MB as a preview) run on the CPU with no AI server, in Compositing (Select › Subject / Sky) and the Library's masks; the Qwen engines stay available for Remove Background and Subject (AI).

## 1. What we have today (audit)

| Area | Today | Problem |
|---|---|---|
| Develop engine | LightCraft `lc-pipeline::render(Rgb32f linear Rec.2020, SourceInfo, &DevelopSettings)`: pure, cached by stage (`render_cached`), GPU path in `lc-gpu`, a non-raw entry (`lc-engine/src/files.rs` `to_working`) | Good. RGB only (no alpha); crop/geometry change the frame |
| Editor's Camera Raw Filter | A **second, simpler engine** (`pc-algo/src/camera_raw.rs`) on sRGB pixels: basic, curves, HSL, grading, detail, effects | No masks, optics, geometry, point colour, profiles; results differ from the Library |
| Local adjustments | `DevelopSettings.masks`, each with `LocalAdjustments` (a **subset**: temp/tint, light, texture/clarity/dehaze, hue/sat, sharpness, noise, moiré, defringe, colour overlay) | No curve, HSL, grading, point colour per mask; not Capture One-style |
| AI masks in the Library | Subject / Sky / Background are **classical heuristics**; Object and Describe use SAM 3 (3.4 GB optional) | The editor's Select Subject (li-seg U²-Net/IS-Net, Qwen) is better; three segmentation stacks |
| Removal in the Library | Clone / Heal / Remove (classical), dust finder, red eye | No AI Remove (that is editor-only, via li-ai / ComfyUI) |
| Library → Editor | `photo.editExternal` renders a **16-bit TIFF** `-Edit.tif`, stacks it; `editOriginal` opens a JPEG directly but **without** its develop settings | TIFF copies; develop is baked in; raw opened in the editor gets default settings and **loses EXIF/XMP** |
| Editor → Library | The Library reads a PSD/TIFF's flattened composite only | Layers invisible; no develop on a composite |
| Smart objects | `SmartObject { source: Embedded/Linked, smart_filters: Vec<SmartFilter{command, params}>, cache }`; an embedded raw already decodes through LightCraft (default settings) | The building block we need — it just doesn't carry develop settings yet |
| Adjustment layers | Per-tile, per-pixel (CPU tiles and GPU kernels) | A full develop (clarity, NR, masks, spots need neighbourhoods) can't be a plain adjustment layer |

## 2. What the others do (research summary)

* **Capture One:** layers = settings + one mask (brush, magic brush, gradients, AI subject/background/people, luma range) + opacity; most tools work on a layer (not vignette, B&W, grain, crop/lens/geometry). Heal/clone layers. No generative removal. Hand-off to Photoshop renders a TIFF/PSD; layers never survive either way.
* **Lightroom 15:** AI masks Subject, Sky, Background, Objects, People (parts), Landscape; masks hold a subset of tools (now incl. a curve and point colour). Remove = generative (cloud) / content-aware / heal / clone, plus Distraction Removal (people, reflections, dust). Heavy AI results live in an `.acr` sidecar beside the XMP — **no TIFF**. *Open as Smart Object in Photoshop* embeds the raw + settings; double-click reopens Camera Raw.
* **Photoshop:** Camera Raw Filter = same UI minus raw-only tools (no crop, no Denoise / Super Resolution); used as a re-editable **smart filter**; "stamp visible → smart object → Camera Raw" is the common whole-stack workflow. Since 2026 some ACR controls (Light, Clarity & Dehaze, Color & Vibrance, Grain) exist as adjustment layers — testers find them weaker than raw processing.
* **Affinity Photo 2 / Pixelmator Pro / ON1 / Luminar:** a **RAW layer** in the layer stack that stays re-developable (Affinity: embedded or linked). Retouching *on* that layer is the weak spot (Pixelmator: destructive; Photoshop: rasterise trap) — retouch goes in layers above.
* **Open source:** darktable (GPL-3; any module maskable; best scene-referred algorithms; SAM 2.1 object masks refined with dense CRF + guided filter and vectorised), ART / RawTherapee (GPL-3; best demosaic and capture sharpening; ART's external/linked masks), vkdt (BSD-2; GPU local laplacian, OIDN denoise, wavelet highlights), RapidRAW (Rust + wgpu, Lightroom-like masks that each hold most sliders, AI subject/sky/depth, LaMa remove — **AGPL-3.0: reference only, don't copy**). PhotoFlow (every tool a layer) is the closest "layered raw editor" but abandoned. None of the open-source editors has a raw smart object: that would be ours.

## 3. Recommendation

### 3.1 One develop engine everywhere

`lc-pipeline` becomes the only develop engine: Library Develop, Develop layers in the Editor, and **Filter › Camera Raw Filter…** all render through it and share one Develop UI (the Library's panels, reused as a workspace/dialog in the Editor). Each input declares capabilities:

* **Scene-referred source** (raw, or a linear DNG): every tool.
* **Rendered pixels** (a JPEG, a layer, a composite): raw-only tools are hidden (demosaic, raw NR/AI Denoise on mosaic data, highlight reconstruction from clipped channels, camera profiles/calibration that need a camera matrix) — exactly Adobe's split.

`pc-algo::camera_raw` is retired; its Photoshop descriptor mapping (`pc-io/src/camera_raw_map.rs`) is kept so PSDs from Photoshop still open with their Camera Raw smart filters (mapped onto `DevelopSettings`).

### 3.2 Capture One-style develop layers (inside develop)

`DevelopSettings` gains `layers: Vec<DevelopLayer>` (replacing `masks` + `LocalAdjustments`, migrated automatically):

```text
DevelopLayer { name, enabled, opacity, mask: MaskExpr, settings: sparse DevelopSettings, kind: Adjust | Heal | Clone | Remove(AI) }
MaskExpr     = ordered [ (Add|Subtract|Intersect, Component, invert, opacity, feather, refine) ]
Component    = Brush | Linear | Radial | Polygon | LuminanceRange | ColorRange | DepthRange
             | AiSubject | AiSky | AiBackground | AiObject(points/box) | AiPrompt(text) | AiPeople(parts) | External(file) | Linked(layer)
```

* **Any tool on a layer** except image-level ones (crop, geometry/Upright, lens profile, camera profile/calibration, demosaic, raw NR). That is Capture One's list, minus its odd exclusions (we allow vignette, B&W mix and grain on layers).
* **Evaluation the darktable way, not once per layer:** each tool runs once at its canonical pipeline position; at that stage every layer that sets it mixes in `out = mix(in, tool_L(in), mask_L · opacity_L)`. Exposure still runs before the tone curve, so layers behave like Lightroom masks, not like stacked renders. Cost ≈ one render + per-layer stage evaluations only for the tools a layer touches.
* **Masks are stored in source coordinates** and pushed through geometry (as `SegMask` already is), so cropping or straightening later keeps them aligned. AI masks keep their logits/raster in the settings (works offline after the first run).
* **UI:** the Develop panel shows a layer strip (Background + layers, opacity, mask thumbnail, add/subtract/intersect chips), the same tool panels as global develop, and a "Show all tools / tools this layer uses" filter.

### 3.3 AI masks and AI Remove in the Library (and the same in the Editor)

* **One segmentation service** (`li-seg` grows into it), used by both modes:
  * Subject and Background: the editor's li-seg (U²-Net / IS-Net ONNX on `tract`, CPU, offline, small) → replaces the Library's heuristics; refined with the existing guided filter (`lc-pipeline/src/local.rs` `guided`).
  * Sky: a dedicated sky segmentation model (licence check pending) with the current classical sky detector as fallback.
  * Objects / Describe: SAM 3 (as now, optional download) or SAM 2.1 (Apache-2.0, smaller); darktable's refinement idea (CRF + guided filter, optional vectorisation to an editable path).
  * People/parts: later (no open model chosen yet).
  * Editor commands `select.subject`, `select.sky`, `ai.selectSubject` call the same service; the Library's mask components and the Editor's selections then agree pixel for pixel.
* **AI Remove in Develop** (Lightroom's Remove tool): modes **Remove (AI)** · Content-Aware · Heal · Clone, brushed or from a mask/selection.
  * The host passes the editor's AI service into the Library (a `Services.ai_remove` callback, like `open_with`), so `lc-*` doesn't depend on ComfyUI; engines are the ones on the AI Remove tool (FLUX.2 Klein / Qwen, or cloud with the user's key). Offline option: LaMa (Apache-2.0) via ONNX as a fast local engine, later.
  * **Generated pixels are stored, not baked:** a patch store beside the catalog (`<library>/Patches/<hash>.png`, 16-bit where possible; like Lightroom's `.acr`/`lrcat-data`) referenced from the layer (`Remove { patch, bbox, seed, engine, source_hash }`). Applied at the spot stage in scene-linear. If something upstream that moves pixels changes (crop/geometry, lens), the patch is marked stale and the layer offers **Regenerate**.

### 3.4 No more TIFF copies: the Develop layer (a raw smart object)

**Library → Editor** (Photo › Edit in Local Image, ⌘E-style; replaces the TIFF path for our own editor):

1. The editor opens a new document whose **Background is a Develop layer**: a `SmartObject` whose source is the original file (**Linked** to the library photo by path + photo id by default; **Embedded** on request or when saving portable PSDs) and whose new `develop: DevelopSettings` is applied *at source decode* — so a raw stays raw (full highlight recovery, white balance, lens correction, AI Denoise).
2. EXIF/XMP/IPTC come with it (today's raw import drops them — fixed as part of this).
3. Retouch, AI Remove, compositing all happen in **layers above** (the Develop layer is never rasterised; a pixel tool aimed at it offers "Work on a new layer above" — the Photoshop rasterise trap avoided).
4. **Double-click the Develop layer** → the Develop workspace opens on that layer (same panels, layers, AI masks). Change it and the layer re-renders; layers above stay put.
5. The saved document (`.pcraft`, or PSD/layered TIFF) is added to the Library **stacked with its original** as an *Edit*. Its thumbnail is the composite.

**Settings stay re-editable and metadata stays:** the Develop layer's settings are the source of truth for that document (stored in the layer, and mirrored as `lc:settings` XMP in the smart object). Editing the original photo in the Library later does not silently change an existing composite (like a virtual copy); the layer shows "The Library's edit of this photo is newer · Update / Keep". In PSD the smart object is written with its source + our XMP so Photoshop sees a raw smart object, and `crs:` values are written for the subset Camera Raw understands.

**Editor → Library with a layered document** (your "flatten or merge visible" idea, made non-destructive):

* Choosing **Develop** (or switching to Library Develop) on a layered document offers:
  * **Develop the composite (live)** — *default*: the visible layers are grouped into a smart object (contents stay editable layers) with a **Develop smart filter** on it. Change a layer inside and the develop re-applies; double-click the filter to re-develop. No pixels frozen, nothing flattened.
  * **Merge Visible to a new Develop layer** — a stamped copy (fast, frozen) when the stack is heavy.
  * **Flatten** — only on explicit request.
* Coming back from Library Develop to compositing needs no warning in the live case: the develop *is* a smart filter/Develop layer that stays editable in the Layers panel. Only the "merge visible" choice gets the note "Your develop settings apply to a merged copy; changes to the layers below won't show until you merge again."
* **Layer viewer in the Library:** for a layered document in Develop, a collapsible Layers list (visibility, which layer develop targets, open in Editor). Hidden for single-layer photos.

### 3.5 Camera Raw Filter parity (Filter menu, Photoshop-style)

* **Filter › Camera Raw Filter…** opens the same Develop workspace on the active layer: all non-raw-only tools, develop layers with AI masks, AI Remove, presets.
* On a smart object it is a **smart filter** `develop.lightcraft` (re-editable, maskable, opacity/blend like other smart filters); on a plain layer it asks "Convert to Smart Object to keep it editable?" (default yes) or applies destructively.
* On a Develop layer it edits the layer's own settings (scene-referred, every tool).
* Optionally later: cheap per-pixel develop adjustment layers (Light, Color & Vibrance) like Photoshop 2026, for live tweaks over a stack.

## 4. Toolset additions (from darktable / RawTherapee / ART / vkdt)

LightCraft already has: AHD/PPG/bilinear/X-Trans demosaic, clip-aware highlight reconstruction, guided-filter local tone, dehaze (dark channel), NR, spots/heal, Upright, HDR/pano merge, SAM 3 masks.

| Priority | Tool | Reference (licence) |
|---|---|---|
| P1 | AI masks Subject/Sky/Background (real models), mask refine | li-seg U²-Net/IS-Net; darktable refinement (GPL-3) |
| P1 | AI Remove in Develop + patch store | li-ai engines; LaMa ONNX later (Apache-2.0) |
| P1 | AI Denoise | OIDN (Apache-2.0) / vkdt `jddcnn` (BSD-2) |
| P1 | Develop layers holding the full toolset | Capture One model, darktable blending |
| P2 | RCD + AMaZE + dual demosaic, capture sharpening (deconvolution, auto radius) | RawTherapee / ART (GPL-3) |
| P2 | Tone equalizer (EIGF) driving highlights/shadows/whites/blacks | darktable (GPL-3), vkdt `llap` (BSD-2) |
| P2 | Highlight reconstruction: inpaint-opposed / segmentation | darktable (GPL-3) |
| P2 | Lens profiles (lensfun-format reader in Rust; database CC-BY-SA) | darktable lens, ART lensexif |
| P2 | Colour calibration (CAT + colour checker), colour balance rgb, colour equalizer | darktable (GPL-3) |
| P3 | Diffuse or sharpen, contrast & texture (5.8), haze removal upgrade | darktable (GPL-3) |
| P3 | Negative conversion, film simulation, focus stacking | darktable negadoctor, vkdt (BSD-2) |
| P3 | Depth masks (Depth Anything V2-Small, Apache-2.0; not Base/Large) | — |

All ports keep their copyright notices under `licenses/`. RapidRAW (AGPL-3.0) is a design reference only.

## 5. Plan

1. **Develop layer** — `SmartObject.develop`, raw/JPEG source decoded through `lc-pipeline` with the layer's settings; Library → Editor opens a Develop-layer document (linked), metadata carried; double-click opens Develop on the layer; saves to `.pcraft`/PSD; stacked back into the Library. *Retires the TIFF path for our own editor.*
2. **One engine for Camera Raw Filter** — `develop.lightcraft` smart filter, the Develop workspace in the Editor, capability flags; composite → develop (live smart object / merge visible / flatten).
3. **Develop layers** — `DevelopSettings.layers`, migration from masks, per-stage blending, layer UI.
4. **AI in develop** — unified segmentation service; AI Remove with the patch store; AI Denoise.
5. **Toolset upgrades** — the P2/P3 table.

Each step ships with tests against the mock ComfyUI and pipeline golden images, like the rest of V2.
