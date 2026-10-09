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

**Built (October 2026):** layers are the existing masks, not a new `layers` list. `Mask` gained
`opacity` (0–100) and `tools: LayerTools`: a sparse partial `DevelopSettings` (`wb`, `light`, `curve`,
`color`, `mixer`, `point_colors`, `treatment`, `bw_mix`, `grading`, `effects`, `vignette`, `grain`,
`detail`; each `Option`, left out of the JSON when unset, as is an opacity of 100, so older
settings serialize, hash and render bit-identically, which golden hashes in
`lc-pipeline/src/tests_layers.rs` check). Layer values are relative to the photo's (white balance is
the layer's own white). `LocalAdjustments` (`adjust`) is kept alongside, not mapped: its sliders
are summed across masks in one log-domain term (mapping them to sequential mixes would change
existing renders), half of them have no global tool (hue shift, ± sharpness / noise, moiré,
defringe, colour overlay), and the GPU already renders them; the Masking panel shows them as
"Local Adjustments" above the full tool sections. `adjust.amount` × `opacity` scale the mask's
alpha, so presets' amount and the opacity fade everything the layer does.
Evaluation (`lc-pipeline/src/layers.rs`): each tool runs at its stage and every layer that sets it
blends `mix(in, tool_L(in), alpha)`: NR (lin image denoised again, cached), dehaze, WB + exposure
(with the local exposure), highlights/shadows/clarity/texture/sharpening (local tone stage),
contrast/whites/blacks (tone map with summed values), colour tools, vignette, curves, grain.
Settings with layer tools render on the CPU (`layers_need_cpu`; `lightcraft_gpu::render` returns
`None` with a reason); opacity alone stays on the GPU. Commands: `mask.setTools {id, tools?, values?}`
(merges like presets, clamps, `null` drops a section), `mask.setOpacity`, `mask.resetTools {section?}`,
`pointColor.pick/delete {mask}`; copy/paste, presets and Auto Sync carry layers with the Masking
group. UI: the Masking panel reuses the Edit panel's sections (`edit::tool_sections` with
`Target::Layer`), with a dot on sections a layer sets, Reset Section / Reset Layer and the
"Show only tools this layer uses" filter. Not yet: a WGSL port of the layer tools, a layer
White Balance picker, curve presets / targeted adjustment on layers.

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
  * **Built (October 2026):** the Remove panel has modes **AI** · Remove (content-aware) · Heal · Clone; AI removals are brushed, lassoed or take a develop layer's area (`spot.add {mode:"ai", points|polygon|mask, engine?, seed?}`). They are spots (`SpotMode::Ai`, with `polygon`, `mask` and `patch: AiPatch {key, source, rect, engine, seed, geometry}`, all left out of the JSON when empty, so older settings hash as before). A background job (`lightcraft_engine::enhance`) samples the uncropped, lens-corrected photo around the stroke at full size, shows it to the host's engine through an exactly reversible 8-bit view (white balance, a gain putting the 99th percentile below white, Rec.2020 → sRGB) and stores the decoded patch, linear RGBA, in a content-addressed store `<library>/Patches/remove/<key>.lip` (key = photo content + stroke + engine + seed + geometry; deflated √-encoded u16, no TIFF). The pipeline composites it first in `lin_cpu`, before white balance (`lc-pipeline/src/patches.rs`), so every develop change applies to it. The engine reaches Compositing's engines through an `AiHost` on the session (`apps/local-image/src/develop_ai.rs`; a session field rather than `Services`, so commands, the CLI and tests use it), and `lc-*` stays free of `li-ai`/`pc-*`. One undo step per result ("AI Remove", "Regenerate"); AI spots can't be moved or reshaped (only faded, regenerated, deleted); copy/paste, sync, presets and Auto Sync leave them behind (`presets::strip_photo_bound`), and a render never applies a patch made from another photo. A patch made under another geometry is shown as stale (Regenerate); a missing one renders as if absent.
* **AI Denoise (built):** Develop › Detail › AI Denoise runs darktable-ai's RawNIND UtNet2 linear model (GPL-3.0, 31 MB, downloaded as `rawdenoise-nind.dtmodel` from release-5.6.0 through the hash-checked Local AI model list and unpacked) with `tract` on the CPU (`li-seg/src/denoise.rs`: 256 px tiles — the export's static 512 px input re-declared —, 32 px overlap, feathered blend, one gain over the whole image). It runs as a background job on the full-size demosaiced source and stores the result in `<library>/Patches/denoise/` (full size + a 2560 px copy; key = photo content + model); `Enhance.ai` refers to it and the render uses it in place of the source, mixed by the Amount slider (`enhance.denoise`) at render time, before GPU/CPU (`enhance::for_render` in `RenderJob::run`). Without the stored result the photo renders normally and Develop offers to run Denoise again. Raw photos only; not in the Camera Raw Filter.

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

**Built (October 2026, #31):**
* **PSD:** a Develop layer is written as the smart object it is (source embedded, as for any
  smart object) with its develop as the existing Camera Raw Filter smart filter at the bottom of
  its stack (`filter.develop` → Photoshop's Camera Raw Filter, §3.5), carrying our private
  `localImage` record `{"command":"developLayer","settings","photo"}`; the importer takes that
  filter out of the stack and restores the layer's link (Photoshop's edits to the filter apply over
  our settings). Glue only: `smart_map::{develop_layer_filter, resolve_develop_layers}`.
* **Layers list:** Develop (Edit panel) and Library (Info) show a read-only Layers section for
  PSD/PSB, layered TIFF and `.pcraft` photos with more than one layer or a Develop layer:
  thumbnail, name, visibility, blend mode, opacity, Develop layers with the photo they follow
  (click: go to it), the note that Develop edits the merged image, and **Open in Compositing**
  (the host's editor via `Services.open_with`; an already open document comes forward; back in
  Develop the existing Composite → Develop prompt applies). The host reads the layers
  (`apps/local-image/src/doc_layers.rs`, `Services.doc_layers`), so `lc-*` crates never depend
  on `pc-*`; they are read on a worker thread once per file version.

### 3.5 Camera Raw Filter parity (Filter menu, Photoshop-style) — *shipped*

* **Filter › Camera Raw Filter…** (⇧⌘A) is `filter.develop`: the Library's engine
  (`lc-pipeline::render`, `SourceInfo::default()`) on the layer. `photocraft_io::develop_filter`
  converts the layer from the document's profile (matrix/TRC analytically — sRGB, Adobe RGB,
  ProPhoto, Display P3, linear; others through the CMS) to linear Rec.2020, develops at full size
  and converts back; **alpha is copied unchanged**, 8/16/32-bit and Grayscale work. Colours
  outside Rec.2020 (saturated ProPhoto) or above white keep the part outside it, so unchanged
  settings change nothing (identity is skipped outright; the full path is within 1/255).
* **Tools a filter can't use are off** (`develop_filter::sanitize`): crop, Geometry/Upright,
  orientation, lens profile, camera calibration, Enhance (AI Denoise, Raw Details, Super
  Resolution). The Negative conversion, masks, point colour, spots, red eye, manual optics,
  profiles and presets stay.
* **Plain layer → smart filter**: the menu asks "Convert to Smart Object to keep it editable?"
  (default Convert; Apply Destructively edits the pixels inside the selection). On a smart object
  it is a re-editable smart filter `SmartFilter{command:"filter.develop", params: DevelopSettings
  JSON}` (the selection becomes the filter mask; blend/opacity like other smart filters);
  `filter.develop {…, index}` replaces a filter's settings. Convert + filter is one history step.
  On a Develop layer it goes to Develop on its photo (every tool, raw data).
* **The editing UI is the real Develop module.** Compositing hands the host a request (the
  layer's pixels in linear Rec.2020, the settings, a name); the host opens them as an *ephemeral
  photo* (`lc-engine/src/ephemeral.rs`: a temporary 16-bit Rec.2020 TIFF in
  `<config>/cache/camera-raw`, in memory only — never journaled, snapshotted or given a sidecar,
  deleted when the session ends and at startup), shows Develop with the banner "Camera Raw
  Filter · ‹layer› — Cancel / OK" (↩ / Esc), hides the Library's chrome and Library-only
  shortcuts, and hides the Calibration panel, Crop and the lens-profile switch. OK hands the
  settings back and runs `filter.develop` (one history step); Cancel, or the module switch,
  changes nothing. The old `filter.cameraRaw` (the earlier, simpler engine) keeps working for
  existing documents; without the Library (`LOCAL_IMAGE_NO_LIBRARY`) its dialog edits a
  `filter.develop` instead (its controls mapped onto the settings, previews from the same engine).
* **PSD**: a `filter.develop` is written as Photoshop's Camera Raw Filter (the settings Camera Raw
  understands, patched into Photoshop's own descriptor when the filter came from Photoshop), plus,
  when the settings hold more (masks, profiles, B&W, point colour…), the full settings in a
  private `localImage` key of the filter item, which Photoshop ignores and our importer reads
  back (unless Photoshop changed the descriptor since: its values then apply over ours). Photoshop
  Camera Raw filters open as `filter.develop`; our own `filter.cameraRaw` ones stay what they were.
* **Composite → Develop**: switching from Compositing to Develop on a document with no Develop
  layer asks **Develop the composite (live)** (`develop.composite {mode:"live"}`: the visible
  layers grouped into a smart object, contents still editable, with the filter), **Merge visible to
  a new layer and develop** (`mode:"stamp"`), or **Just switch**; "Don't ask again" is kept in the
  preferences (`develop.compositeChoice`). Coming back needs no warning.
* Optionally later: cheap per-pixel develop adjustment layers (Light, Color & Vibrance) like
  Photoshop 2026, for live tweaks over a stack; a cache of the developed smart filter (each
  refresh renders the full pipeline on the CPU).

## 4. Toolset additions (from darktable / RawTherapee / ART / vkdt)

LightCraft already has: AHD/PPG/bilinear/X-Trans demosaic, clip-aware highlight reconstruction, guided-filter local tone, dehaze (dark channel), NR, spots/heal, Upright, HDR/pano merge, SAM 3 masks.

| Priority | Tool | Reference (licence) |
|---|---|---|
| P1 | AI masks Subject/Sky/Background (real models), mask refine | li-seg U²-Net/IS-Net; darktable refinement (GPL-3) |
| P1 | AI Remove in Develop + patch store (*built*, §3.3) | li-ai engines; LaMa ONNX later (Apache-2.0) |
| P1 | AI Denoise (*built*, §3.3) | darktable-ai RawNIND UtNet2 (GPL-3.0); OIDN (Apache-2.0) / vkdt `jddcnn` (BSD-2) as alternatives |
| P1 | Develop layers holding the full toolset | Capture One model, darktable blending |
| P2 | RCD + AMaZE + dual demosaic, capture sharpening (deconvolution, auto radius) (*built*, §4.1) | RawTherapee / ART (GPL-3) |
| P2 | Tone equalizer (EIGF) driving highlights/shadows/whites/blacks (*built*, guided filter, §4.1) | darktable (GPL-3), vkdt `llap` (BSD-2) |
| P2 | Highlight reconstruction: inpaint-opposed / segmentation (*built*, §4.1) | darktable (GPL-3) |
| P2 | Lens profiles (lensfun-format reader in Rust; database CC-BY-SA) (*built*, `lensfun` crate, §4.1) | darktable lens, ART lensexif |
| P2 | Colour calibration (CAT + colour checker), colour balance rgb, colour equalizer (*CAT + gamut compression built*, §4.1) | darktable (GPL-3) |
| P3 | Diffuse or sharpen, contrast & texture (5.8), haze removal upgrade (*diffuse or sharpen deferred*, §4.1) | darktable (GPL-3) |
| P3 | Negative conversion, film simulation, focus stacking (*negative built; film looks built as profiles*, §4.1) | darktable negadoctor, vkdt (BSD-2) |
| P3 | Depth masks (Depth Anything V2-Small, Apache-2.0; not Base/Large) (*built*, MiDaS small fallback, §4.1) | — |

All ports keep their copyright notices under `licenses/`. RapidRAW (AGPL-3.0) is a design reference only.

### 4.1 Toolset upgrades — *built* (2026-10-09)

Every tool is off by default, or at the behaviour photos had before. A new settings section is left
out of the JSON while it is at its defaults, so older settings and catalogues serialize, hash and
render bit-identically. Golden hashes guard this:

* `lc-pipeline` `tests_toolset`: 14 renders recorded before the tools were wired in;
* `lc-engine` `tests_toolset`: the raw loader at a binned, a bilinear and a full size.

Ports are listed in `docs/PORTS.md`, with notices in `licenses/darktable-NOTICE.md`,
`licenses/RawTherapee-NOTICE.md`, `licenses/lensfun-NOTICE.md` and `licenses/model-system-NOTICE.md`.

* **Raw processing** (`DevelopSettings.raw`, raw files only; Detail copy group):
  * Demosaic: Default (AHD) / AHD / **RCD** / **Dual (RCD + VNG4)** / **VNG4** /
    **AMaZE** / **Dual (AMaZE + VNG4)** / PPG / Bilinear. The saved `dualRcd`
    (RCD + bilinear) decoder and mask are unchanged and appear in the menu only when selected.
    New keys are `vng4`, `amaze`, `dualRcdVng`, `dualAmazeVng`.
    AMaZE and the new duals are Bayer-only; X-Trans keeps its existing decoder.
    Standalone VNG4 runs all 64 gradient terms. New duals use upstream's four-colour
    VNG-linear pass and two median colour-smoothing passes; their mask includes the
    exact upstream 9x9 disc Gaussian and exponential approximation.
  * Highlights: Reconstruct (today's) / **Inpaint Opposed** / **Segmentation** / Clip.
    `segmentation` runs on normalized CFA after opcode lists 1/2, before demosaic and
    opcode list 3. Bayer previews reduce each of the four CFA phases separately,
    preserving clipped maxima, before reconstruction with scaled morphology. X-Trans
    reconstructs its full CFA before binning. RGB/monochrome raws use opposed as fallback.
    The raw API implements all seven upstream recovery modes and noise; the Develop
    menu uses upstream defaults (combine 2, candidating 0.4, recovery off).
  * **Capture sharpening**: Richardson–Lucy with an automatic radius measured from the raw,
    corner boost and an ISO-based threshold.
  * Demosaic and highlights are applied when the file is decoded (`lc-engine` `files::RawOptions`).
    The decoded source is cached per option set (`raw_key`; 0 is the default, the old cache entry).
  * Capture sharpening is a *presource* stage on the decoded source (`lightcraft_pipeline::presource`,
    cached in `StageCache.pre`). It runs before the CPU and the GPU renders alike.
  * Binned previews scale the radius by `SourceInfo.sensor_scale`.
  * Quality/speed: AMaZE improves fine-detail PSNR on the supersampled zone plate and
    Siemens star; RCD has the highest PSNR on the colour-edge chart. On a Ryzen 7 9800X3D
    with eight workers, 24 MP RCD takes 57 ms, VNG4 353 ms, AMaZE 1288 ms; duals take
    634 / 1860 ms. Segmentation takes 1067 ms for 24 MP CFA alone. These are single
    synthetic release runs, not camera benchmarks. Preview-versus-full segmentation
    PSNR is 62.81 / 47.71 / 41.84 / 38.31 dB at bin factors 2 / 4 / 6 / 8;
    previews are approximations. See [CODEX-REPORT](wip/CODEX-REPORT.md) for all metrics.
  * Raw decoding and CFA reconstruction run on CPU before both CPU and GPU rendering;
    these options do not change the shader boundary. Existing default render goldens pass.
    The upstream reference suite also found and corrected an X-Trans capture-radius
    row offset; capture sharpening enabled on X-Trans can therefore improve/change.
* **Lens profiles** (`DevelopSettings.lens_db`, Optics; for cameras without embedded lens data).
  * The `lensfun` crate supplies the database, lookup and interpolation; it is used only in
    `lc-engine` (`lens_db`).
  * The pipeline evaluates the rescaled correction (`lensdb::LensCorrection`) as part of the
    geometry warp: distortion, TCA and vignetting, each with a strength.
  * Lens-database frames are not GPU-samplable (`Frame::gpu_samplable`), so the GPU renderer
    resamples their geometry on the CPU.
* **Tone equalizer** (`DevelopSettings.tone_eq`, Light group).
  * Nine zones from −8 to 0 EV, fitted by 8 Gaussians.
  * The mask is a guided filter of log2 of the RGB norm. It has size, refinement and
    exposure/contrast compensation controls, and a *Show Mask* overlay (`Overlay::ToneEqMask`).
  * It renders on the CPU (`tools_need_cpu`).
* **Colour calibration** (`DevelopSettings.color_cal`, Calibration group).
  * Adaptation from a standard, custom or White Balance illuminant to D65, with CAT16,
    Bradford (linear and non-linear) or XYZ.
  * Gamut compression in u′v′, and a clip of negatives.
  * Linear cases fold into the white-balance matrix and run on the GPU. The non-linear cases
    (non-linear Bradford, gamut > 0, clip) take the CPU path for the linear stage (`lin_needs_cpu`).
* **Film looks**: eight `lc.filmsim.*` profiles in the "Film Simulation" group. Each is a profile
  delta (calibration primaries, RGB-curve fades, grading wheels) on the existing profile mechanism.
  No vkdt code is used.
* **Depth masks**: a *Depth* mask component (`MaskShape::DepthRange` with a near→far band and
  smoothness).
  * The depth map comes from `li-seg` `Task::Depth`: Depth Anything V2 Small, falling back to
    MiDaS v2.1 small.
  * It is stored as distance (0 = near) in a `SegMask` and evaluated on the CPU.

**Deferred, with reasons:**

* **Diffuse or sharpen.** This is an iterative multi-scale anisotropic diffusion with tens of
  iterations of wavelet passes. It is far too slow on the CPU at interactive preview sizes without
  the GPU port, and the WGSL work is out of scope for this round.
* **EIGF mask in the tone equalizer.** The existing fast guided filter (on log luminance) is used.
  EIGF would be a second filter implementation for a similar mask.
* **Colour checker calibration, colour balance rgb, colour equalizer.** These were not in this
  round's approved list.

**Follow-ups:**

* Tone equalizer as a develop-layer tool (`LayerTools`).
* WGSL ports of the tone equalizer, the non-linear colour calibration and lens-database sampling,
  so they stop falling back to the CPU.
* `lensDb.*` commands for MCP and scripting (detect, search, pick).
* Depth masks in the Camera Raw Filter.
* `pc-io` develop layers decoding with the photo's raw options (`files::load_bytes_with`).
* The other `Frame::with_lens` sites in the UI should use the lens database: the neighbour
  prefetch in `panels/detail.rs` and `compare.rs`.

## 5. Plan

1. **Develop layer** — `SmartObject.develop`, raw/JPEG source decoded through `lc-pipeline` with the layer's settings; Library → Editor opens a Develop-layer document (linked), metadata carried; double-click opens Develop on the layer; saves to `.pcraft`/PSD; stacked back into the Library. *Retires the TIFF path for our own editor.*
2. **One engine for Camera Raw Filter** — `filter.develop` smart filter, edited in the Develop module through a host session; composite → develop (live smart object / merge visible). *(Shipped, §3.5.)*
3. **Develop layers** — `DevelopSettings.layers`, migration from masks, per-stage blending, layer UI. *(Built on the masks, §3.2.)*
4. **AI in develop** — unified segmentation service; AI Remove with the patch store; AI Denoise.
5. **Toolset upgrades** — the P2/P3 table.

Each step ships with tests against the mock ComfyUI and pipeline golden images, like the rest of V2.
