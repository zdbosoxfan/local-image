# Local Image 2 — product and interface design

Status: design for the `v2` branch (October 2026). This document is the plan the V2 code follows:
what the app is, how it is laid out, every user flow and how it was tested, and how the code is put
together so that fixes from PhotoCraft and LightCraft flow in.

## 1. What Local Image 2 is

**One native image app that combines a full compositing editor with local AI.**

| From | What it brings |
| --- | --- |
| **PhotoCraft** (Photoshop-style editor, pure Rust) | Layers, masks, adjustment layers, layer styles (drop shadow for cutouts), type, vectors, brushes, selections, Free Transform, healing tools, 27 blend modes, ICC colour management, 8/16/32-bit, real PSD/PSB, history, command palette, the Pro design system |
| **LightCraft** (Lightroom-style library, pure Rust) | Camera-raw decoding and its scene-referred colour pipeline (camera matrices, DNG profiles, highlight recovery), embedded-preview thumbnails, the filmstrip and folder behaviour, the export presets and options |
| **Local Image 0.7** | Local AI through ComfyUI: AI Remove (FLUX.2 Klein + object-removal LoRA, or Qwen), background removal with a real alpha matte (Qwen Image 2.1), generation and instruction edits (Qwen, Z-Image Turbo, FLUX.2 Klein 4B/9B, ERNIE), SeedVR2 enhancement, styles (LoRAs), the generated library, verified model downloads |

0.7 was three separate "workspaces" (Retouch, Cutout, Generate) with their own tool rails, a
browser UI in a WebView and a Python server. V2 is a single editor in which the AI features are
**tools, commands and panels in the places creative-app users already look for them**.

### Design principles

1. **Familiar first.** Photoshop's menu order, tool groups and shortcuts (J for healing, W for
   object selection, ⌘J, ⇧⌘I, ⌘T…), Lightroom's filmstrip and export. If you know those apps you can
   use this one.
2. **AI is a tool, not a mode.** No mode switching: the AI Remove brush sits in the healing group,
   AI Cutout sits with the selection tools, Generate is a dock panel next to Layers.
3. **Non-destructive by default.** AI repairs land on their own layer; cutouts become layer masks;
   generated images are new documents or new layers. Every AI change is one undo step.
4. **Never block the canvas.** AI jobs run in the background with progress in the status bar and
   Esc/✕ to cancel; you keep editing other documents while ComfyUI works.
5. **Explain, don't fail.** When ComfyUI isn't running or a model is missing, the tool says so in
   one sentence with the button that fixes it (Start AI engine, Download model).

## 2. Layout

```
┌──────────────────────────────────────────────────────────────────────────────────────┐
│ ◆ File Edit Image Layer Type Select Filter View Window Help   photo.jpg •   ☼ ⌕ [Essentials ▾]│ title bar
├──────────────────────────────────────────────────────────────────────────────────────┤
│ ⌂ │ [tool] │ tool options …                                    contextual action ▸   │ options bar
├───┬──────────────────────────────────────────────────────────┬───┬──────────────────┤
│ ↖ │ photo.jpg @ 50% (Layer 1, RGB/8) × │ portrait.CR2 × │      │ ⚙ │ Properties │ Adj. │
│ ⬚ │                                                          │ ◐ │──────────────────│
│ ◌ │                                                          │ ✦ │ Generate │ Library│  ← new group
│ ✂ │                    canvas                                │ ≡ │  prompt, model,  │
│ ⌖ │                                                          │   │  size, results   │
│ ✚ │                                                          │   │──────────────────│
│ ✎ │                                                          │   │ Layers │Channels│
│ … │                                                          │   │                  │
│ ■ ├──────────────────────────────────────────────────────────┤   │                  │
│ □ │ ▤ filmstrip (folder or multi-file open)  ◀ img img [img] img img ▶             │   │
├───┴──────────────────────────────────────────────────────────┴───┴──────────────────┤
│ 50 % │ RGB · 8 bit │ 4000×3000 │ 3 layers │ status message      ● AI ready · 9/32 GB │ status bar
└──────────────────────────────────────────────────────────────────────────────────────┘
```

* **Theme:** PhotoCraft's Pro tokens (charcoal panels, Spectrum blue `#378ef0` accent, Inter +
  JetBrains Mono, Lucide icons). The other PhotoCraft themes (Medium Gray, Studio, Light, Classic)
  stay available from the sun button.
* **Workspaces** (top-right switcher, Window › Workspace) rearrange panels and pick a starting tool:
  - *Essentials* — Properties, Layers, Generate collapsed.
  - *Retouch* — AI Remove brush selected, Layers + History.
  - *Cutout* — AI Cutout tool, Properties (mask), Layers.
  - *Generate* — Generate panel expanded with the Library tab, Layers.
* **Filmstrip** (from LightCraft): appears under the canvas for File › Open Folder…, and whenever a
  photo is opened from a folder that holds others (Preferences: off in the strip's options).
  120 pt cells, names, the active cell follows; click, Alt+←/→ or Page Up/Down move. Built for
  working through a folder:
  - Stepping on closes the image you leave when it has no unsaved edits (no pile of tabs); one
    with unsaved edits keeps its tab and an accent dot in the strip.
  - **Save & Next** (⌘⌥→ / Ctrl+Alt+→) saves and opens the next image. By default the edit goes to
    a copy in an `Edited` folder beside the originals (raw files as TIFF); the strip then shows the
    edited copy with a ✓ badge and reopens it when you go back. The options menu switches to
    overwriting the original or Save As.
  - **Review in Library** shows the folder in the Library's grid (read in place, not imported) to
    compare, rate and flag the results; **Develop** and **Compositing** in the title bar take the
    chosen photo on from there (Compositing opens it as a Develop layer, no copy).
  Window › Filmstrip toggles it.
* **Status bar** gains an **AI status pill**: grey "AI off", amber "Starting…", green "AI ready ·
  GPU · used/total"; click it to open Preferences › Local AI. Running AI jobs show their stage
  ("Sampling 12/28") with a cancel button, exactly like PhotoCraft's other background jobs.

## 3. The AI tools

### AI Remove brush (J, healing group)
* Paint over a distraction; on release it is removed. The stroke preview is the usual healing-brush
  trail.
* Options bar: brush chip (size, hardness), **Engine** (FLUX.2 Klein · Qwen Compact · Qwen Full),
  **Output** (New layer / Current layer), *Sample All Layers* (on).
* The repair is computed from the visible composite around the stroke (context crop, red-outline
  input, Poisson blend — the 0.7 pipeline) and arrives as a layer "AI Remove 1" above the active
  layer, masked to the stroke, so it can be hidden, lowered in opacity, masked or merged.
* No engine? The options bar shows "AI engine not running — Start" in place of the hint.

### AI Cutout tool (W, object-selection group)
* **Remove Background** button on the options bar (also Layer › Remove Background, and the Layers
  context menu). Engine: **Qwen AI · Compact / Full** (a real alpha matte, best edges) or
  **Standard (CPU)** (the editor's own Remove Background: Select Subject plus edge refinement;
  works without a GPU). Properties › Quick Actions offers both, as *Remove Background* and
  *Remove Background (AI)*.
* The result is a **layer mask** on the active layer: the original pixels stay; the mask hides the
  background. Optional hint field ("the red car") is passed to the model.
* Refine: drag on the canvas with the Cutout tool to paint the mask — **Erase** (hide) or
  **Restore** (reveal), toggled with X or the options-bar switch; size with `[` `]`.
* **Add background ▾** on the options bar: Solid colour (fill layer), Image… (place below),
  Generate… (Qwen empty background plate from a description). The new layer goes *below* the
  cutout layer.
* Shadow: Layer › Layer Style › Drop Shadow (PhotoCraft's live effect) — no separate shadow model.

### Generate panel (dock group "Generate | Library")
Generation follows Krita AI Diffusion (any local model, the canvas as input, results as layers)
with Photoshop's names (Generative Fill) and dock conventions. Opening it (Window › Generate,
File › New from Prompt…) folds Properties away so the prompt has room.
* **Mode** chips: *Create* (new image), *Edit* (instruction edit of the open image: native edit
  models, or inpaint at a strength), *Fill* (the selection, Generative Fill), *Refine* (img2img at a
  strength), *Upscale* (enlarge and add detail tile by tile).
* **Preset** (Krita's styles): prompt template, negative, steps, guidance, sampler, LoRAs and an
  optional Draft → Refine model; built-ins plus the user's own (bookmark button saves the current
  settings).
* **Model picker**: every model grouped by family, with a readiness dot (ready / installed but
  incomplete / not installed), the family name and capability tags (Create, Edit, refs, Fill,
  LoRA…); imported custom workflows below; **Browse Models…** and **Import Workflow…** at the
  bottom. Fields follow the model's capabilities: negative prompt only where the family uses one,
  steps and guidance only where they aren't fixed, references only up to what the model takes.
* Prompt (Enter generates, ⇧Enter for a new line), negative prompt where used, then one **Size**
  row (aspect menu, width × height snapped to the model's grid), Transparent where supported.
* *Strength* (Edit-by-inpaint, Fill with non-instruction models, Refine, Upscale), *Enlarge*
  (Upscale), *References* (current image or a file; native multi-image, else IP-Adapter/Redux).
* *Styles (LoRA)*: installed LoRAs that fit the family (≤ the family's maximum, strength −2…2);
  **Find LoRAs…** opens the Model Browser on that family's LoRAs.
* *Refine with*: Draft → Refine — a fast model drafts, a second model resamples it at a strength,
  optionally enlarged first.
* *Advanced* (collapsed): steps, guidance, variation strength, sampler and scheduler, seed (keep or
  new each time).
* The **Generate** button sits directly under the inputs with the image count (1–4) beside it, so
  the primary action never scrolls away; its label follows the mode (Apply Edit, Fill Selection,
  Upscale 2×). When something is missing the reason shows above it with the button that fixes it.
* Results stream into the thumbnail grid right below (pending tiles show stage and progress). Each
  result: **Open**, **Place as layer**, **Use as reference**, **Recreate**, **Copy seed**; every
  result is also saved to the library.

### Model Browser (Window › Model Browser…)
Laid out like SwarmUI's and InvokeAI's model managers, with Civitai/Hugging Face-style cards, in
the PhotoCraft Pro theme (one dark window, no extra chrome).
* Left column: **Models | LoRAs**, then *New*, *Trending*, *Installed*, *Works on my GPU* (the
  detected GPU memory), then every family grouped (Stable Diffusion, FLUX, Qwen, Z-Image, ERNIE,
  HiDream).
* Top row: search, source filters (Hugging Face, Civitai, Templates, Community), refresh.
* Cards: preview, source and licence-gate badges, title, author and downloads, family and
  capability tags, download size, rough GPU memory (amber when above the detected GPU), licence,
  **Install** (or Get Template), the publisher's page.
* **Install** first shows the licence (with a link, non-commercial and gated warnings) and the
  exact files, their folders and sizes, and asks to accept the licence; then downloads each file
  verified by size and SHA-256 into ComfyUI's folders, with progress and Stop, and refreshes ComfyUI.
  The model appears in Generate's picker straight away.
* A template for a family Local Image doesn't know yet installs its files and becomes a custom
  workflow with a **Basic controls** badge (prompt, negative, seed, size, images).
* *Update Model Profiles* fetches newer family profiles from this repository.
* Nothing goes online until the window is open; every catalogue is cached so it works offline.

### The open model system
* **Families as data** (`crates/li-ai/families/*.json`): SD 1.5, SDXL, Pony, Illustrious/NoobAI,
  SD 3.5, FLUX.1 (+ Kontext, Fill), FLUX.2, FLUX.2 Klein, Qwen Image, Qwen Image Edit, Qwen Image
  2.1, Z-Image, ERNIE-Image, HiDream, SeedVR2. A profile states the loaders, text encoders, sampling
  defaults, capabilities, LoRA rules, prompt conventions, component files and the Civitai/Hugging
  Face names that mean it; derived families inherit (`base`). 0.7's curated models live in the
  profiles unchanged (same files and hashes).
* **Detection** (`arch.rs`): installed files are classified from `/object_info` plus their
  safetensors/GGUF headers (tensor names and shapes, ModelSpec metadata), then the file name for
  what weights can't tell apart (Pony vs Illustrious, Kontext vs Krea).
* **Workflow builders** (`builders.rs`): one generic builder for every family and task (create,
  refine, inpaint, edit, references, LoRAs), Draft → Refine and tiled upscale-refine (`ops.rs`).
* **Custom workflows** (`custom.rs`): any ComfyUI workflow, editor or API format (subgraphs
  flattened), becomes a model when its nodes are titled `li:prompt`, `li:negative`, `li:image`,
  `li:image2`…, `li:mask`, `li:seed`, `li:steps`, `li:cfg`, `li:denoise`, `li:width`, `li:height`,
  `li:output`.
* **Guardrails**: downloads only from allow-listed hosts (Hugging Face and its CDNs, Civitai and its
  R2/B2 delivery buckets, GitHub), only `.safetensors` and `.gguf`, never without a published size
  and SHA-256, tokens sent only to their own site, licence shown before downloading.

### Library tab
* The generated-image library (same storage as 0.7, so old generations appear): searchable grid
  with model and size, newest first. Open, place, use as reference, recreate, delete.

### Other AI commands
| Command | Where |
| --- | --- |
| Select › Subject (AI) | Qwen matte → selection |
| Edit › AI Remove | removes the current selection (same as the brush) |
| Edit › Generative Fill… | opens Generate in Fill mode for the selection |
| Image › AI Enhance (SeedVR2)… | 2× / 4K long edge / custom, keeps alpha, new document |
| File › New from Prompt… | focuses the Generate panel in Create mode |
| File › Automate › Remove Backgrounds… | batch: files or folder → transparent / white / colour → PNGs |
| Edit › Preferences › Local AI… | ComfyUI connection, installation, start/stop, model folder, models, Hugging Face and Civitai tokens |
| Window › Model Browser… | find, compare and install models and LoRAs (also from Generate's model picker) |
| Help › AI Models & GPU… | model guide, downloads with progress, detected GPU memory |

## 4. Files, colour and export

* **Open:** everything PhotoCraft opens (PSD/PSB, TIFF with layers, PNG, JPEG, WebP, GIF, BMP, TGA,
  EXR, HDR, AVIF, QOI, HEIC with the feature) **plus camera raw through LightCraft's pipeline**
  (DNG, CR2, NEF/NRW, ARW, RAF, RW2/RWL, PEF, ORF; formats it can't decode open from the embedded
  preview). Raw files are developed with LightCraft's camera defaults into a **16-bit document**
  in the chosen working space (ProPhoto by default) with its ICC profile, so editing keeps the
  full tonal range.
* **Open Folder… (⇧⌘O)** opens a folder into the filmstrip (natural sort, supported files only);
  thumbnails come from embedded previews first, then a real decode on worker threads.
* **Save:** PSD keeps every AI layer and mask; File › Save As offers PSD, TIFF, PNG, JPEG, WebP and
  PhotoCraft's `.pcraft`.
* **Export (⌥⇧⌘W)** uses LightCraft's export model: format (JPEG, PNG, TIFF, WebP, AVIF), bit depth,
  colour space (sRGB, Display P3, Adobe RGB, ProPhoto, Rec.2020) with an embedded ICC profile,
  quality or file-size limit, resize (long edge / short edge / width / height / megapixels /
  percent, don't enlarge), output sharpening, metadata policy, file-name template and conflict
  handling, plus presets and **Export with Previous**. PhotoCraft's Quick Export as PNG stays.

## 5. How 0.7's snags were resolved

| 0.7 snag | V2 |
| --- | --- |
| Retouch operation was a hidden mode (Brush = AI, Heal tool = CPU, other tools inherited the last) | Separate, named tools: Spot Healing / Healing / Patch / Content-Aware Move (CPU) and AI Remove (AI), each with its own options |
| No brush hardness, opacity, invert/save selection | PhotoCraft's brush engine and full Select menu |
| Cutout dead end before a mask existed | Remove Background always available; Layer › Layer Mask › Reveal All starts a manual mask |
| Shadow hidden in an accordion; backgrounds scattered in five places | Drop Shadow is a layer style; one **Add background ▾** menu on the Cutout tool |
| Reset transform only in a "…" menu | Free Transform (⌘T) and Edit › Transform |
| Same command in several places, Ctrl+S silently exporting | Photoshop's File menu: Save, Save As, Export As, Export, Quick Export |
| Incomplete shortcuts list | Edit › Keyboard Shortcuts lists every command; command palette (⌘K) |
| Generate number fields wrote on every keystroke | Shared value fields that commit on Enter/blur and accept arithmetic |
| "Refine" mode with inner Draft/Refine tabs | Create / Edit / Fill modes; refine = "Use as reference" on a result |
| Settings overloaded with hardware guide and shortcuts | Preferences › Local AI; Help › AI Models & GPU |
| Assets dock doing three jobs | Generate · Library tabs; stock search is File › Place from Openverse… |
| Global busy lock | Background jobs per document; other documents stay editable |
| Filmstrip only with ≥2 images | Open Folder always shows it; opening a photo shows its folder when it holds ≥2 images; Window › Filmstrip toggles it |

## 6. User flows (each covered by an automated test, see §8)

1. **First launch** → Home: New, Open, Open Folder, a prompt box ("Describe an image to create"),
   recent files and an *AI setup* card when the engine isn't ready → Preferences › Local AI: Detect
   finds ComfyUI, *Start* launches it, *Models* lists what is installed and downloads the rest with
   verified checksums.
2. **Remove a distraction** → open photo → J (AI Remove) → paint → release → status bar shows
   progress → "AI Remove 1" layer appears → toggle its eye to compare → ⌘S.
3. **Cut out a product and place it on a new background** → W (AI Cutout) → Remove Background →
   mask appears → refine with Erase/Restore → Add background ▸ Generate… "marble countertop, soft
   window light" → background layer below → Layer Style › Drop Shadow → Export as PNG.
4. **Generate from nothing** → Home prompt or Generate panel → model, aspect, Generate → result
   tiles → Open → it's a document; retouch it like any photo.
5. **Edit with an instruction** → open image → Generate › Edit "make it golden hour" → result as a
   new layer above (or new document).
6. **Generative fill** → lasso an area → Edit › Generative Fill… → prompt → result layer masked to
   the selection.
7. **Work through a folder** → open any photo in it (or File › Open Folder…) → filmstrip → AI Remove
   (or the Contextual Task Bar's Remove after a pen path) → **Save & Next** → … → **Review in
   Library** to compare the `Edited` copies in the grid.
8. **Raw photo** → open a .NEF → developed 16-bit document → retouch → export to JPEG sRGB.
9. **Batch backgrounds** → File › Automate › Remove Backgrounds… → pick files → White background →
   output folder → run, watch progress, open the folder.
10. **Round trip PSD** → save with AI layers → reopen → layers and masks intact.
11. **Get a new model** → Generate › model picker › Browse Models… → Trending or a family → card →
    Install → licence and files → accept → progress → Installed → it is in the picker, ready.
12. **Camera Raw Filter on a layer** → Filter › Camera Raw Filter… (⇧⌘A) → "Convert to Smart
    Object to keep it editable?" (Convert) → the Develop module opens on the layer with a
    "Camera Raw Filter · ‹layer› — Cancel / OK" banner (every non-raw tool, masks, presets) → OK
    (↩) → one history step, a re-editable `filter.develop` smart filter; double-click it to edit
    again. Compositing → Develop on a layered document asks: develop the composite (live), a
    merged copy, or just switch (see DEVELOP-DESIGN §3.5).
13. **Bring your own workflow** → Generate › model picker › Import Workflow… → a ComfyUI workflow
    with `li:` titles (or an official template) → it is a model with the fields it marks.

## 7. Architecture

```
apps/local-image          the desktop binary (window, services, raw import, branding)
apps/local-image-cli      command-line automation
crates/pc-*               PhotoCraft (engine, compositor, PSD, CMS, paint, egui UI), now in-tree
crates/lc-*               LightCraft (raw, develop, pipeline, catalog, codecs, export), now in-tree
crates/li-ai              ComfyUI client, family profiles, detection, workflow builders, custom
                          workflows, presets, model browser data, downloads, library, setup, mock
crates/li-seg             the local segmentation model (Quick selection and backgrounds on CPU)
```

* `li-ai` modules: `family` (profiles and updates), `catalog` (models and presets from the
  profiles plus what is installed), `arch` (header detection), `inventory` (installed models and
  LoRAs), `builders` and `workflows` (graphs), `inpaint` (Krita's mask geometry), `custom`
  (workflow import), `presets`, `browser` (catalogues, cards, install plans), `download`
  (verified downloads), `ops` (every AI operation), `mock` and `mock_hub` (the test server).
* AI features enter the editor through one engine module (`ai_cmds.rs`) and UI modules
  (`ai_ui.rs`, `generate_ui.rs`, `model_browser.rs`, `filmstrip_ui.rs`, `lc_export_ui.rs`), plus
  small marked hooks (`// local-image:`) in the tool enum, toolbar groups, dock groups, menu catalog,
  preferences and branding.
* Jobs: AI commands snapshot the document (`Arc` copy-on-write), run `li-ai` on a worker thread,
  and apply the result as one history step on the UI thread.

## 8. Verification

* `cargo test` for `li-ai` (graphs for every family and task, detection, profiles and updates,
  catalog, imaging, downloads, library, setup, every AI operation end-to-end against the mock
  ComfyUI, all ten official template fixtures converting with basic controls, and the Model
  Browser's whole install flow against the mock hub: Hugging Face tree hashes, Civitai CDN
  redirect, unverified and gated refusals, the offline cache).
* `cargo run -p li-ai --example mock_comfy -- 8199 <model folder>` serves ComfyUI and every model
  source for demos and screenshots (see the example's header for the environment variables).
* The vendored test suites (PhotoCraft engine/UI, LightCraft raw/pipeline) still pass.
* Integration tests drive the real app headlessly through PhotoCraft's control channel against the
  mock ComfyUI: each flow in §6 is a test.
* Screenshots of every flow are captured offscreen with the snapshot tool and reviewed at 1440×900
  and 1024×700.
