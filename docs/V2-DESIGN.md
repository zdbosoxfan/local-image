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
* **Filmstrip** (from LightCraft): appears under the canvas when a folder or several files are
  open; 120 pt cells, names, an "edited" badge, arrow keys move, click opens, the active cell
  follows. Window › Filmstrip toggles it.
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
  context menu). Engine: **Qwen Compact / Qwen Full** (AI, best edges) or **Quick (CPU)**
  (PhotoCraft's built-in subject finder; works without a GPU).
* The result is a **layer mask** on the active layer: the original pixels stay; the mask hides the
  background. Optional hint field ("the red car") is passed to the model.
* Refine: drag on the canvas with the Cutout tool to paint the mask — **Erase** (hide) or
  **Restore** (reveal), toggled with X or the options-bar switch; size with `[` `]`.
* **Add background ▾** on the options bar: Solid colour (fill layer), Image… (place below),
  Generate… (Qwen empty background plate from a description). The new layer goes *below* the
  cutout layer.
* Shadow: Layer › Layer Style › Drop Shadow (PhotoCraft's live effect) — no separate shadow model.

### Generate panel (dock group "Generate | Library")
* **Mode** chips: *Create* (new image), *Edit* (instruction edit of the open image), *Fill*
  (generative fill of the selection — Qwen edit composited through the selection).
* Prompt box (Enter to generate, ⇧Enter for a new line, character count), model picker showing
  "ready / download needed", precision, aspect-ratio chips (1:1, 4:3, 3:2, 16:9, 9:16, Custom) plus
  width × height fields snapped to the model's grid, Transparent (Qwen), count (1–4).
* *Image inputs* (references): **+ Current image**, **+ File…**, **+ Library**; reorder; Z-Image
  shows *Variation strength* instead.
* *Advanced* (collapsed): steps, guidance (hidden when the model fixes it), seed (🎲 random / 🔒
  keep), negative prompt (only for models that use it), styles (LoRAs, ≤3, strength −2…2).
* Results stream into a thumbnail grid in the panel (pending tiles show stage and progress). Each
  result: **Open** (new document, default on double-click), **Place as layer**, **Use as
  reference**, **Recreate** (same settings, new seed), **Copy seed**. Every result is also saved
  to the library.

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
| Edit › Preferences › Local AI… | ComfyUI connection, installation, start/stop, model folder, models |
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
| Filmstrip only with ≥2 images | Open Folder always shows it; Window › Filmstrip toggles it |

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
7. **Work through a folder** → File › Open Folder… → filmstrip → arrow keys / click → AI Remove on a
   few → File › Export… with a preset → Export with Previous on the next one.
8. **Raw photo** → open a .NEF → developed 16-bit document → retouch → export to JPEG sRGB.
9. **Batch backgrounds** → File › Automate › Remove Backgrounds… → pick files → White background →
   output folder → run, watch progress, open the folder.
10. **Round trip PSD** → save with AI layers → reopen → layers and masks intact.

## 7. Architecture

```
apps/local-image          the desktop binary (window, services, raw import, branding)
crates/li-ai              ComfyUI client, model catalog, workflows, downloads, library, setup, mock server
crates/li-raw             LightCraft pipeline → PhotoCraft document (raw + colour)
vendor/photocraft/…       PhotoCraft (git subtree) — engine, compositor, PSD, CMS, egui UI
vendor/lightcraft/…       LightCraft (git subtree) — raw, develop, pipeline, codecs, export encoders
```

* AI features enter PhotoCraft through **one engine module (`ai_cmds.rs`)** and **a few UI
  modules (`ai_ui.rs`, `generate_ui.rs`, `filmstrip_ui.rs`, `lc_export_ui.rs`)** added to the vendored
  crates, plus small, marked hook edits (`// local-image:`) in existing files (tool enum, toolbar
  groups, dock groups, menu catalog, preferences, branding). New files never conflict on upstream
  merges; the marked hooks are listed in `docs/UPSTREAM.md`.
* Jobs: AI commands snapshot the document (`Arc` copy-on-write), run `li-ai` on a worker thread,
  and apply the result as one history step on the UI thread (PhotoCraft's `jobs::run`).
* Upstream: `scripts/sync-upstream.sh` runs `git subtree pull --squash` for both projects, then
  builds and tests; a weekly GitHub Action opens a pull request when either upstream has moved.

## 8. Verification

* `cargo test` for `li-ai` (graphs, catalog, imaging, downloads, library, setup, and every AI
  operation end-to-end against the mock ComfyUI server).
* The vendored test suites (PhotoCraft engine/UI, LightCraft raw/pipeline) still pass.
* Integration tests drive the real app headlessly through PhotoCraft's control channel against the
  mock ComfyUI: each flow in §6 is a test.
* Screenshots of every flow are captured offscreen with the snapshot tool and reviewed at 1440×900
  and 1024×700.
