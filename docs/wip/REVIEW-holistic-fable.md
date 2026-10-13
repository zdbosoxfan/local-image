# Holistic product review of Local Image v2 (Fable, 2026-10-13)

Scope: the whole product as a working photographer would use it (headshots, architecture, events).
Branch `claude/v2-continue` = `origin/v2` at `c83768d9`. Read-only: no source edited, no cargo run,
no GUI started. Evidence is `file:line` in this worktree, screenshot paths, or docs. Match Look is
being revised on `wip/cx-matchlook` and is only mentioned at a high level. Bugs are secondary here:
the two final reviews (`~/.local/share/local-image-dev/reviews/opus-final-review.md`,
`astra-final-review.md`) already list them; this review checks which of those still matter for the
product and concentrates on quality, enhancements and optimisation.

Written incrementally. Status: **complete** (sections 1-4).

---

## 1. Executive summary (one page, for the owner)

**Where the product stands.** Local Image v2 is already more than the sum of its two forks. The
develop engine is reference-checked against darktable/RawTherapee code and the GPU matches the CPU
within half a level; the raw smart object with a live Library link is something no other
open-source editor has; keys now follow Lightroom Classic with Capture One / Photoshop / Affinity
profiles; the catalog journal is crash-safe; performance adapts to the machine; nothing leaves the
computer. The engineering is careful and well documented. What is left is mostly product work:
make the app *feel* like the apps you know, remove the few hard blockers for working photographers,
and turn the AI ingredients that are already in the tree into the two tools that save a headshot
photographer the most hours.

**The hard blocker.** Canon CR3 and compressed Fujifilm RAF / Olympus ORF files are not decoded
(`crates/lc-raw/src/lib.rs:20-21`); they open "preview only" from the embedded JPEG. For anyone
shooting a Canon R or a current Fujifilm, Develop is a JPEG editor. Nothing else in this review
matters as much to those users.

**The two biggest wins for your own work.** (1) Face-aware culling: eyes open, eyes sharp, best of
burst by face. The face detector already runs for People; this is weeks, not months, and it is what
Lightroom 15.4, Capture One 16.8 and Aftershoot all ship. (2) A portrait retouch assistant that
finds blemishes and offers brush → adjust → confirm, smooths with texture kept, and applies across a
session through Sync. That is Evoto's whole business and nobody open-source has it.

**Where it is weaker than Lightroom / Capture One today.** Import cannot build previews or write a
second copy; there is no catalog backup; no proof sheet or print; tethering is a folder watcher; the
Library's words and layout are Lightroom *cloud* ("My Photos", "Albums", a cloud icon) while its
keys are Classic; the Develop panel puts a film-negative tool above Light; the module switcher sits
in three different places; there are two Preferences windows; AI Remove needs a ComfyUI GPU, so the
Mac has none when away from the server.

**Under the hood.** Two of almost everything below the UI (raw, codecs, colour, i18n, prefs,
widgets) is the long-term cost driver: it is why Library export and Compositing Export As produce
different JPEGs for the same settings. The test suite is large and partly hardware-gated, with a
known-failure allow-list; it needs to be green and fast before a release. Three GPU users (Develop,
compositor, ComfyUI) share one card with no VRAM coordination, and a single GPU failure turns
Develop CPU-only until restart.

**Top 10 recommendations**

1. Decode CR3 and compressed RAF/ORF (port rawler's open decoders, golden-hash tests). (N1)
2. Face-aware Assisted Culling: eyes open / eye sharpness / best-of-burst per face. (N2)
3. Portrait retouch assistant with batch apply through Sync. (S1)
4. On-device AI Remove (LaMa via tract) so the Mac works offline. (N3)
5. Import: build previews and "second copy to" at ingest; catalog backup on quit + verify. (QW3, QW4)
6. Make the Library read like Lightroom Classic: Basic/WB first, Negative out of the way, module
   switcher in one place, Collections/Catalog/Loupe names, one Preferences window. (QW1, QW2, QW8, N4)
7. One codec/colour stack for both halves, then the rest of the shared core; delete `pc-raw`. (N5, S2)
8. A real-photo AI quality gate per engine (remove, cutout, enhance, generate), plus
   reproducible exports. (N12, QW6)
9. GPU/VRAM coordination with ComfyUI and recovery instead of "GPU off for the process"; battery
   awareness; the remaining serial pixel loops and CPU-AI estimates. (QW10, N10, N11, QW7)
10. Release hygiene: green workspace with no allow-listed failures and a fast CI lane, notarised
    Mac package and Linux Flatpak/AppImage from CI, check-for-updates, proof-sheet export, fix the
    stale docs. (N13, S7, QW5, N14, QW9)

---

## 2. Findings per area

Legend: **Impact** = what a working photographer feels. **Ref** = how Lightroom Classic (LRC),
Capture One (C1), Photoshop (PS), Affinity, Photo Mechanic (PM), Aftershoot or Evoto do it.

### 2.1 Workflow end to end (import → cull → develop → retouch/composite → export)

**Import.** Good: Add in place / Copy / Move / Copy as DNG, date folders, album, develop preset,
metadata preset, keywords, rename templates with a live example, duplicate detection, remembered
options (`crates/lc-ui-egui/src/import.rs:64` `ImportPreferences`), bounded parallel copy with
hash verification (`crates/lc-engine/src/import.rs:818`), and a watched folder "Auto Import" for
tethering-by-folder (`crates/lc-ui-egui/src/panels/settings.rs:370`).

| # | Finding | Evidence | Impact | Ref |
|---|---|---|---|---|
| W1 | **No "second copy" on ingest.** Import writes to one destination only. | `import.rs` body (`:845-1110`) has Add/Copy/Move, destination, DNG, organise, album, presets, keywords; nothing for a backup copy. | Event and headshot shooters back up the card to a second drive at ingest; here they must do it by hand before formatting. | LRC *Make a Second Copy To*; PM Ingest writes to a primary and secondary folder. |
| W2 | **No preview build at import.** Building 1:1 / smart previews is a separate command (`crates/lc-engine/src/cmd/previews.rs`), not an import option. | `import.rs` has no "Build previews" row (grep `preview` finds only the name preview). | First pass through 600 headshots renders each raw on demand; the first loupe view of each photo pays a decode (≈2.7-4.9 s for a 50 MP ARW per the Astra review, "Library, startup and RAW responsiveness"). | LRC Import: *Build Previews: Minimal / Embedded / Standard / 1:1*, *Build Smart Previews*. |
| W3 | **Tethering is a folder watcher.** Auto Import adds files that land in a folder; there is no tether bar (latest capture, shot count, "apply develop to next capture"). | `settings.rs:370-406` | Studio headshot sessions are shot tethered; the watcher works with the camera vendor's utility but the Library gives no session view. | C1 *Capture* tool tab, LRC *Tethered Capture* bar. |

**Cull.** Good: Lightroom Classic keys are now the default (E/D/C/N/R/Q/K/L, Shift+P/X/U advance:
`crates/lc-ui-egui/src/menus.rs:108-175`, `shortcuts.rs:13-21`, test `bugfix_develop_classic_keymap`
`shortcuts.rs:370`), Compare and Survey (`state.rs:16`), Auto Advance, Lights Out, a second window
(`panels/second.rs`), embedded-preview thumbnails, Assisted Culling with sharpness/clipping/burst
scores exposed to smart-album rules (`crates/lc-engine/src/cmd/cull.rs:1-15`), Smart Sort (CLIP
tagging, burst stacks, People with face clustering and per-person export folders:
`CODEX-REPORT-smart-sort-p1.md`, `-people-ui.md`), a delete dialog that always asks
(`settings.rs:160`). The 85,000-entry synthetic grid runs at ~0.2 ms/frame (Astra).

| # | Finding | Evidence | Impact | Ref |
|---|---|---|---|---|
| W4 | **Assisted Culling has no face-level scores (eyes open, eye sharpness, expression).** Scores are whole-frame Laplacian sharpness + clipping + burst grouping. The face detector (YuNet, with landmarks) and SFace embeddings already run for People, so eye points are available. | `cull.rs:1-5, 60-80`; `smart_sort/people.rs` (YuNet/SFace) | For a 400-frame headshot session the culling question is "eyes open and sharp on the eyes?", not "frame sharp?". This is the biggest time sink the app could remove. | LRC 15.4 *Faces panel in Assisted Culling Info: Eyes open and Eyes sharpness scores*; Aftershoot closed-eye + blur-with-subject-importance; C1 16.8 *Assisted Review* (closed eyes, missed focus, exposure). |
| W5 | **Bursts/duplicates only within the current Smart Sort run.** No catalog-wide "find duplicates" view. | `smart_sort/bursts.rs` (per run); import marks path/content duplicates only. | Event catalogs accumulate re-imports from several cards. | LRC 15.4 duplicate detection across folders, stacked for review. |
| W6 | **Culling speed depends on the preview size setting and on-demand rendering.** Default loupe edge 2560 (`state.rs:205`), "Click 1:1" renders the full raw; no "Embedded preview first, then replace" policy for raws beyond the grid thumb. | `state.rs:180-217`; `media.rs` loupe job | PM users cull at embedded-JPEG speed; here the first 1:1 of each frame waits for a decode. W2 fixes most of this. | PM embedded JPEG; LRC *Embedded & Sidecar* previews. |

**Develop.** Good: one engine everywhere (`docs/DEVELOP-DESIGN.md` §3.1, shipped), Capture One-style
stacked layers with masks (Brush, Linear, Radial, Colour range, Luminance range, Subject, Sky,
Background, Depth: `crates/lc-develop/src/settings.rs:1030-1060`), Tone Equalizer, Colour
Calibration, lens database, RCD/AMaZE demosaic, capture sharpening, AI denoise, AI Remove with a
patch store, Sync (⌘⇧S), presets, versions, before/after, soft proofing, HDR/panorama merge, film
negative, slider drafts capped at 1280 px with coalesced jobs (`CODEX-REPORT-develop-speed.md`).

| # | Finding | Evidence | Impact | Ref |
|---|---|---|---|---|
| W7 | **Panel order and names follow Lightroom *cloud*, not Classic.** The Edit panel reads Profile → **Negative** → Light → Color → Effects → Detail → Tone Equalizer → Optics; White Balance lives inside Color. "Negative" (film negative conversion, a niche tool) sits above Light on every photo. | `docs/images/v2/module-develop.png`; `target/smart-sort-p2b/screenshots/review-1920.png` (right panel); `crates/lc-ui-egui/src/panels/edit.rs:537` (`negative_section` placement) | A Classic/C1 user looks for *Basic* with WB and Tone first; a film-negative section at the top reads as clutter. | LRC: Basic (WB, Tone, Presence) → Tone Curve → HSL → Color Grading → Detail → Lens → Transform → Effects → Calibration. C1: Exposure, Color, Lens, Details tabs. |
| W8 | **Depth masks and AI masks are CPU-only; SAM 3 stays off (owner decision).** Subject/Sky/Depth run through tract on the compute pool (now off the UI thread: `wip/ui-offthread`), 2-6 s per mask on a desktop CPU (`BGREMOVE-BAKEOFF-REPORT.md` timings). | bake-off table; `li-seg` | Fine on the 9800X3D / M4 Max; slow on a laptop. Acceptable, but show the estimate in the tooltip (as the bg-remove tiers spec already requires). | LRC masks are GPU/NPU; C1 "4x faster masking". |

**Retouch / composite.** Good: Develop layer = raw smart object with live link to the Library
(`DEVELOP-DESIGN.md` §3.4, shipped), Camera Raw Filter parity (§3.5), AI Remove with brush → adjust
→ confirm (owner decision), content-aware heal, frequency separation (`pc-engine/src/freqsep_cmds.rs`),
Liquify, full PSD/PSB round trip, `-Edit.psd` stacked back into the Library.

| # | Finding | Evidence | Impact | Ref |
|---|---|---|---|---|
| W9 | **No portrait retouch assistant.** Skin work is manual (heal, freq-sep, Liquify). There is no face-aware blemish finder, skin smoothing with texture retention, eye/teeth brightening, or "apply this retouch to the whole session". The ingredients exist: face detection + landmarks (li-seg YuNet), a Skin Tone layer tool in Develop (`edit.rs:239`), freq-sep in the editor. | `pc-engine/src/retouch_cmds.rs`, `freqsep_cmds.rs`; `li-seg/src/faces.rs` | For a headshot photographer this is the single largest "hours per session" item after culling. | Evoto (batch skin, blemish, reshape), LRC *Portrait* AI masks (skin/eyes/teeth/hair per person), C1 Skin Tone. |
| W10 | **AI Remove needs a ComfyUI GPU.** On the owner's Mac without a remote server there is no generative removal at all (content-aware only). The design already names LaMa (Apache-2.0) as the offline engine "later" (`DEVELOP-DESIGN.md` §3.3). | `docs/MACOS.md` "Remote generation (recommended)"; `develop_ai.rs` | Architecture exteriors (cars, cones, cables) and events (exit signs) are exactly the AI Remove cases; on the road with the MacBook the tool is missing. | LRC Remove (cloud generative) + Distraction Removal; C1 has no generative remove. |

**Export / deliver.** Good: a Lightroom-style export dialog with presets, naming templates,
colour spaces with embedded ICC, bit depth, size/quality limits, output sharpening, text and
graphic watermark, metadata policy (incl. remove location, people names opt-in), DNG, "open in"
after export, *Export with Previous* (`menus.rs:244`), Save Over Original with a backup folder,
a bounded parallel render→encode→write pipeline (`BUILD-pipelines.md` §2), Smart Sort export to
per-person / per-tag folders.

| # | Finding | Evidence | Impact | Ref |
|---|---|---|---|---|
| W11 | **No proof/delivery output: contact sheet, PDF proof sheet, web gallery, print.** Only "slideshow" exists (`state.rs:447`). | grep for contact sheet / web gallery / print in `lc-*` → none | Headshot clients pick from a proof sheet; architecture clients get a PDF; neither can be made in-app. | LRC Print / Web / Book modules, C1 Print; Affinity prints. |
| W12 | **Exports are not byte-reproducible** (ICC header embeds the current time). | `UPSTREAM-FOLLOWUPS.md` "Exports aren't byte-reproducible" | Re-exporting an unchanged set changes every file; sync tools re-upload everything. | LRC exports are reproducible for identical settings. |
| W13 | **Two export paths with two JPEG encoders and two ICC embedders.** Library export uses `lc-codecs` (jpeg-encoder 0.7.1); Compositing *Export As* uses `pc-codecs` (jpeg-encoder 0.6.1, `image` 0.25). | `crates/lc-codecs/Cargo.toml:20-23`, `crates/pc-codecs/Cargo.toml:30-42`; Cargo.lock has both 0.6.1 and 0.7.1 | The same "Quality 85" gives different files, sizes and chroma subsampling depending on which module exported; metadata handling differs too. | One encoder per format in every reference app. |

### 2.2 Image quality

Strong foundation, mostly verified with reference vectors and GPU parity tests:
- Raw: 1,302 camera matrices, 55 bundled DCPs + a user DCP folder, chosen-white WB with DCP tables
  re-derived per white, three looks (Soft Film / Sigmoid / Camera-fit), RCD / AMaZE / dual
  demosaic, inpaint-opposed / segmentation highlight reconstruction, Richardson-Lucy capture
  sharpening, Rec.2020 working curves, hue-preserving gamut containment, TPDF dither shared CPU/GPU,
  GPU parity mean < 0.5 LSB (`CODEX-REPORT-colour-tone.md` §2-6, `-raw.md`, `-detail.md`).
- Noise / detail: RawNIND AI denoise (GPU, CPU fallback), new Detail renderer with per-layer radius.
- Display: monitor ICC detection (macOS, KWin/X11/colord on Linux), Display P3 surface on macOS,
  ordered dither on the canvas, 10-bit aware (`CODEX-REPORT-display.md`), soft proofing.
- AI: on-device models chosen by a measured bake-off (`BGREMOVE-BAKEOFF-REPORT.md`), two cutout tiers
  with time estimates (`SPEC-bg-remove-tiers.md`), upscalers evaluated on 500 outputs
  (`CODEX-REPORT-ai-enhance.md`).

| # | Finding | Evidence | Impact | Ref |
|---|---|---|---|---|
| Q1 | **Canon CR3, compressed Fujifilm RAF and compressed Olympus ORF are not decoded**; they open "preview only" from the embedded JPEG with an amber notice. | `crates/lc-raw/src/lib.rs:20-21` ("Variants we can't decode yet (… compressed ORF/RAF, CR3) return RawError::Unsupported"); `lc-engine/src/files.rs:110-124` (`preview_only`); `lc-ui-egui/src/widgets.rs:43` | Every Canon mirrorless since 2018 (R, R5, R6, R3, R1) and every current Fujifilm body with lossless compression on. For those cameras the whole Develop module is a JPEG editor. This is the one finding that can make the app unusable for a given photographer. | LRC/C1/darktable (rawspeed) all decode CR3 and compressed RAF/ORF. Pure-Rust options: port the open CRX decoder (rawler, MIT) and RAF/ORF compressed paths; `docs/PORTS.md` already records rawler-derived matrices. |
| Q2 | **A second, simpler raw developer survives in the editor.** `pc-raw` (5.9k lines, ProPhoto gamma 1.8, basic pipeline) is still what `pc-io::import` uses for raw bytes (`crates/pc-io/src/raw.rs:82 photocraft_raw::develop`). If any route (drag-drop onto Compositing, `LOCAL_IMAGE_NO_LIBRARY`, Place) reaches it, the same NEF looks different from Develop. | `crates/pc-raw/src/lib.rs:1-25`; `pc-io/src/raw.rs`; design says raw "through LightCraft's pipeline" (`V2-DESIGN.md` §4) | Two renderings of one raw in one app. Also 5.9k lines of duplicate decoder to maintain. | One raw engine per app everywhere. |
| Q3 | **Generative results have documented quality failures.** Astra: "Real image-generator routes complete technically, with substantial visual-quality failures documented separately"; Qwen cutout returned unusable alpha on the RTX 5090 (`UPSTREAM-FOLLOWUPS.md` "Qwen AI cutout"); refine recipes silently drop the selected LoRA (Astra F12). | Astra review; UPSTREAM-FOLLOWUPS | AI Remove / Generate are headline features; a bad patch on a client's wall is worse than no patch. Needs a fixed real-photo evaluation set (CC0 interiors, exteriors, portraits) and a pass/fail gate per engine, like the bake-offs already done for cutout and upscale. | Adobe ships a quality bar per Firefly release; Aftershoot/Evoto publish before/after sets. |
| Q4 | **CPU upscale is impractical and unguarded.** `Upscaler::tiled` runs tiles sequentially; bsrgan-4x needs ~1,700 256² inferences per input megapixel (`AUDIT-multithreading.md` F2); no time estimate or refusal before launch. | `crates/li-seg/src/enhance.rs:141-218` (no rayon: grep) | On the Mac a 24 MP "Rescue" upscale can run for hours with a progress bar. | LRC Super Resolution is GPU/NPU with a time estimate; Topaz refuses/warns. |
| Q5 | **Lens corrections for cameras without embedded data rely on the lensfun DB (CC-BY-SA) and the user picking a profile; lens-DB geometry resamples on the CPU** (`DEVELOP-DESIGN.md` §4.1 "Lens-database frames are not GPU-samplable"). | `lc-engine/src/lens_db.rs`; design §4.1 follow-ups | Architecture work is lens-correction heavy (TS-E, 14-24 mm); a CPU resample per slider move on a 50 MP file is felt. The WGSL port is listed as a follow-up. | LRC/C1 profiles are GPU-applied. |
| Q6 | **Masks lack Feather/Edge refinement controls at mask level** (brush feather exists; AI masks get a guided-filter refine with no user control). | `settings.rs` MaskShape; `masking.rs` | Sky/subject edges on hair and window frames need a feather/edge slider. | LRC 15.5 added Feather and Edge controls on masks; C1 *Refine Mask*. |

### 2.3 UX and UI consistency

Good: one theme recolours both halves (`library_host.rs:49`), five themes, design-system rule with
widget catalogue (`DESIGN-SYSTEM.md`), keyboard profiles (Lightroom / Capture One / Photoshop /
Affinity) with a conflict-aware editor (`CODEX-REPORT-keys.md`, `target/keys-ux/*.png`), plain-English
errors with a Details/History sink and disabled-with-reason everywhere (`CODEX-REPORT-reasons.md`),
dockable/tear-off panels in Compositing, monochrome Lucide icons, accessibility names and AccessKit
on, HiDPI tested at 1.0/1.5, starts in the Library by default (`library_host.rs:1371-1375`) with a
Startup › Open in setting.

| # | Finding | Evidence | Impact | Ref |
|---|---|---|---|---|
| U1 | **The module switcher moves.** Library/Develop: top-left of the title bar (`docs/images/v2/module-library.png`); Compositing: centre, after Help (`module-compositing.png`); a later Library build: top-right (`target/notices-ux/library-actions-1440.png`). | screenshots | The one global control is in three places depending on where you are. | LRC: module picker always top-right. |
| U2 | **Library speaks Lightroom *cloud*, keys speak Lightroom *Classic*.** "My Photos", "Albums", "Add 3 to My Photos", a search field in the title, an icon rail on the right, a cloud-style icon top-right in the Oct 12 docs screenshot (a folder icon in later builds) (`module-library.png`, `target/notices-ux/library-actions-1440.png`, `panels/left.rs:97-120`). | screenshots; `left.rs` | Classic/C1 users look for *Catalog / Folders / Collections* and a Metadata panel. | LRC: Navigator / Catalog / Folders / Collections left; Histogram / Quick Develop / Keywording / Metadata right. C1: Library tool tab. |
| U3 | **Two preference systems, two dialogs.** Library *Settings* (⌘, , `library-ui.json`) and editor *Preferences* (⌘K, `preferences.json`), with Local AI, Performance and Keyboard Shortcuts under the editor's. | `CODEX-REPORT-keys.md` B2; `panels/settings.rs`; `pc-ui-egui/src/prefs_ui.rs` | A user looking for "GPU" finds it under Library Settings › Rendering and again under Preferences › Performance. | One Preferences window with tabs (LRC Preferences + Catalog Settings is already two; do not add a third). |
| U4 | **Two widget sets that look alike but are not the same.** Documented as "duplicates across the crates" (`DESIGN-SYSTEM.md` §3): icon button, colour tile, dropdown, segmented/tab, separator, section header. Light theme: Library bodies darker than Compositing (Astra UX05). | `DESIGN-SYSTEM.md:158-164`; `target/develop-a-ux/expanded-light-400-1.png` vs `target/ux-polish/*light*` | Checkboxes, dropdowns and headers change shape when switching modules; the light theme is two greys. | One widget library per app. |
| U5 | **macOS native menu bar covers Compositing's Pixel menus only.** The native bar is built from `TOP_MENUS` (`pc-ui-egui/src/native_menu.rs:574`); the Vector persona's Object/Effect menus are not in it (Opus O-034, still open), and the Library/Develop menubar has no native builder in `mac_menu.rs`. | `native_menu.rs:574`; `apps/local-image/src/mac_menu.rs` (no Library references) | On the owner's Mac the menu bar is native in one module and in-window in the next; Vector users lose Object/Effect on Mac. | Every Mac app: one native menu bar, contents follow context. |
| U6 | **No first-run guidance in the Library.** Compositing has a Home screen with an AI-setup card; the Library opens an empty catalog with no "Import / Browse a folder / Open a sample" card and no catalog-location choice. | `V2-DESIGN.md` §6 flow 1 (editor only); `library_host.rs:1498` opens the default library silently | A new user sees an empty grid and a "My Photos" header. | LRC asks where to create the catalog; C1 offers Catalog vs Session. |
| U7 | **Terminology drift.** "Compositing" (module) vs "Editor"/"Photoshop" in docs and errors; "Albums" vs "Collections"; "Local" (folders browsed in place) vs "Folders"; "Detail view" vs "Loupe" (E key says Loupe, the menu says Loupe, the settings say "Detail"). | `menus.rs:110`, `settings.rs:597`, `left.rs` | Small, but it is the kind of thing that makes a Lightroom user feel the app is "almost" familiar. | LRC names. |
| U8 | **Stale user-facing docs.** `docs/whats-new.md` says Smart Sort is "not shown in the interface yet"; `README.md` says the internet is used for "checking for updates" but there is no update check (grep finds none); README lists Windows as a target the owner does not test. | `docs/whats-new.md:30-33`; `README.md:9-11` | The first thing a tester reads is wrong. | — |

### 2.4 Performance and resource use

Good, and recent: one compute profile detected at start (`apps/local-image/src/compute_policy.rs`),
one global rayon pool with tract on it (`li_seg::init_executor`), heavy-job permits sized by free
memory (`lc-sysmem/src/permits.rs`), Utility/Background thread classes, bounded parallel export /
preview build / import copy (`BUILD-pipelines.md`), AI selections and masks off the UI thread
(`wip/ui-offthread`), Develop drafts capped at 1280 px with per-slot coalescing, large-brush batch,
idle repaint fixed (`pc-ui-egui/src/lib.rs:1467-1475`), GPU develop with CPU stages inside the GPU
render rather than whole-render fallback, Preferences › Performance overrides, 85k-entry grid at
0.2 ms/frame. Measured: 24 MP JPEG open 189 ms, PSD 239 ms, PNG export 683 ms, PSD save 517 ms,
Develop exposure drag p95 18 ms (Astra "Current application measurements").

| # | Finding | Evidence | Impact | Ref |
|---|---|---|---|---|
| P1 | **Three GPU consumers, no VRAM coordination.** lc-gpu (Develop), pc-gpu (compositor) and ComfyUI (PyTorch, up to 24-32 GB for Qwen/SeedVR2) share one card. `BUILD-pipelines.md` §2: "a failed GPU render switches the GPU off for the process". | `BUILD-pipelines.md` GPU_GATE note; `README.md` GPU table | Start a Qwen job, drag an exposure slider → an OOM in wgpu turns Develop CPU-only until restart. On a 16 GB card this is the common case. | LRC has separate GPU switches per stage and recovers; darktable retries after "out of memory". |
| P2 | **Catalog opens synchronously on the UI thread.** `LibraryHost::library()` lazily calls `open_library_app` → `Session::open_library` → `Journal::open`, which parses the whole JSON snapshot and replays the log inside the frame that first shows the Library (`library_host.rs:205-215, 1498-1508`; `lc-catalog/src/journal.rs:309-345`). | code | Fine at 3,000 photos; at 50,000 photos with develop settings it is seconds of blank window with no progress. | LRC shows the window and a progress bar while the catalog opens. |
| P3 | **No battery / power-source awareness.** Smart Sort, face indexing, preview builds and model downloads run at the same thread count on battery; nothing in `lc-sysmem` reads the power source (grep `battery` → none). | `lc-sysmem/src`, `apps` | On the MacBook, indexing a shoot at 15 threads drains the battery and heats the laptop. | macOS QoS Background already helps; LRC pauses face detection on battery (preference "Pause on battery"). |
| P4 | **CPU AI tiling is sequential** (Q4) and **CPU AI denoise** runs one tile at a time with a serial gather/blend between GPU tiles. | `li-seg/src/enhance.rs:141-218`, `denoise.rs:824-905`; `AUDIT-multithreading.md` F2/F3/F10 | Mac denoise of a 24 MP raw: minutes. | C1 16.8 Enhanced Denoise runs in the background with a progress estimate. |
| P5 | **Serial pixel loops in the editor:** destructive adjustments (`pc-engine/src/pixels.rs` `adjusted`, no rayon), stack modes (`pc-algo/src/stack.rs`), restoration pre/post, li-ai enhance finishing resizes, gallery thumbnails decoded on the UI thread. | `AUDIT-multithreading.md` F4/F5/F7/F8/F14 (all still open: grep `rayon` in `pixels.rs`, `stack.rs` → none) | 1-4 s per Image › Adjustments apply on a 24 MP layer; hitches scrolling the Generate gallery. | PS applies adjustments in tiles on all cores. |
| P6 | **Vector persona misses its own frame budgets** (2,000-node warp p95 22.9 ms; 500-shape drag 34.6 ms; 5,000-path view 41 ms; 2,000-object live swatch edit 573 ms). | Astra "Vector responsiveness is not uniformly within a frame" | Not the owner's daily work; relevant for logo/overlay work in Vector. | Affinity Designer stays under a frame for these sizes. |
| P7 | **Full raw open is perceptible:** 50 MP ARW 4.9 s (defaults) / 2.7 s (edited) end to end on the RTX box. Decode of CR2/NEF/DNG lossless-JPEG is single-stream per strip (inherent), demosaic is parallel. | Astra; `AUDIT-multithreading.md` §C lc-raw row | First loupe of each frame in a 600-frame session; W2 (previews at import) hides it. | LRC ~1-2 s for the same file with Standard previews built. |

### 2.5 Architecture and maintainability risks

Size: about 800k lines of Rust in 56 crates (`pc-ui-egui` 162k lines / 360 files, `pc-engine`
114k / 307, `lc-engine` 46k, `lc-ui-egui` 44k), 890 locked packages, 11,827 `#[test]` functions,
192 `#[ignore]` (GPU/manual gates). Workspace test run: 886-929 s single-threaded on the RTX box
(Astra). `unsafe_code = deny` workspace-wide, `cargo xtask deps-pure` gate, crash guard, durable
catalog journal with torn-tail repair: all good.

| # | Risk | Evidence | Why it will hurt |
|---|---|---|---|
| A1 | **Two of everything below the UI.** Raw (`lc-raw` 19k vs `pc-raw` 5.9k), codecs (`lc-codecs`+`lc-tiff` vs `pc-codecs`), colour (`lc-color` vs `pc-color`+`pc-cms`), geometry, raster, metadata, i18n (`i18n::tr` vs `tl!`, two 12-language catalogs), preferences (two files, two dialogs), shortcut dispatch (two dispatchers under one profile layer), widgets/theme/icons (documented). | crate list; `DESIGN-SYSTEM.md` §3; `Cargo.lock` jpeg-encoder 0.6.1 + 0.7.1 | Every codec bug is fixed twice or shows up as "Library export looks different from Export As" (W13, Q2). Every string is translated twice. |
| A2 | **Upstream fork drift is one-way and accelerating.** PhotoCraft/LightCraft were forked ~Oct 7-8; fixes are ported by hand in `up-*` batches with a cross-owner backlog (`UPSTREAM-FOLLOWUPS.md` "Cross-owner changes still to apply"); the app has since restructured both halves (vector integration, Develop layers, module host). `cargo xtask upstream-check` tracks darktable ports only. | `UPSTREAM-FOLLOWUPS.md`; `docs/PORTS.md`; `docs/upstream/` | Each month the port gets harder; eventually a critical upstream fix (PSD, raw) will not apply. Decide a horizon. |
| A3 | **Test suite is big, slow and partly hardware-gated; failures are allow-listed.** 11 failing tests in the last full run, a documented pre-existing-failures list, a flaky engine test under load, a `land-known-failures.txt` mechanism, 192 ignored tests needing the RTX 5090 or Metal; the `xtask assets` gate failed on 1,356 uncovered assets at review time and trips on test screenshots written under `target/`. CI runs the whole workspace on ubuntu+windows with 2 threads. | `UPSTREAM-FOLLOWUPS.md` "Tests already failing"; `STATUS-codex-checkpoint-list.md` §5; `.github/workflows/ci.yml` | Regressions in GPU-only tests are invisible to CI; an allow-list of failures grows; a 15-min run with known reds stops being read. |
| A4 | **Compile-time shape.** `pc-ui-egui` is one 162k-line crate; release uses `lto = "thin"`, `codegen-units = 1` (`Cargo.toml:160-163`); worktree targets run 30-90 GB (`build-cache-cleanup` memory). | `Cargo.toml` | Every UI tweak recompiles the largest crate; parallel builder jobs fight for disk. Splitting `pc-ui-egui` (vector_packages, generate/model browser, dialogs) into 3-4 crates is mechanical. |
| A5 | **Documentation sprawl and drift.** 100+ `docs/wip/*` reports, several stale top-level docs (U8), no user manual; `docs/V2-DESIGN.md` still describes the 0.7-era layout in places. | `docs/wip` listing; `docs/whats-new.md` | New contributors (human or AI builder) read the wrong spec; the owner cannot tell what is true from the docs alone. |
| A6 | **Single-maintainer parsers of untrusted input.** `heic-rs 0.1.1` (pinned, fuzzed), own PSD/ABR/SVG/PDF preflights; the Opus review found allocation bombs and loops in PSD/DNG/SVG/PDF readers (O-011/012/017/018/019), several since hardened in `up-formats`/`fu-psd`. | `pc-heif/Cargo.toml`; Opus review | A client's PSD from a stock site should never take the app down; keep the fuzz corpus and byte budgets as a release gate. |

### 2.6 Feature gaps and best-in-class opportunities

Ranked for the owner's work (headshots in volume, architecture interiors/exteriors, events).

| # | Gap / opportunity | Why it matters | Reference | Notes |
|---|---|---|---|---|
| G1 | **CR3 / compressed RAF / ORF decoding** (Q1) | Without it, Canon and Fujifilm shooters cannot develop raws. | LRC, C1, rawler (MIT) | Highest-value engine work after Match Look. |
| G2 | **Face-aware culling** (W4): eyes-open, eye-sharpness, smile/expression, "best of burst by face". | Headshot sessions: 400 frames → 40 picks in minutes. | LRC 15.4 Faces panel; Aftershoot; C1 Assisted Review | Reuse YuNet landmarks; add a tiny eye-state classifier (ONNX, Apache/MIT) and eye-crop Laplacian. |
| G3 | **Portrait retouch assistant** (W9) with batch apply via Sync/presets. | Hours per session; nobody open-source does it. | Evoto; LRC Portrait masks | Face parsing model (licence check), freq-sep automation, heal-by-detection with confirm (same brush → adjust → confirm pattern as AI Remove). |
| G4 | **On-device AI Remove (LaMa)** (W10) | Mac and laptop users; offline. | LRC Remove; design §3.3 names it | tract ONNX, Apache-2.0; same patch store. |
| G5 | **Tether bar over Auto Import** (W3) | Studio headshots and product work. | C1 Capture, LRC Tethered Capture | Pure-Rust USB/PTP is a large project (`nusb` is pure Rust but PTP is from scratch); start with the folder-watch "session" UI. |
| G6 | **Previews at import + second copy** (W1, W2) | Event/headshot ingest safety and first-pass speed. | LRC Import panel; PM Ingest | Small engine work; mostly dialog. |
| G7 | **Proof sheet / PDF / web gallery export** (W11) | Client proofing and delivery. | LRC Print/Web; C1 Print | `krilla` (PDF) is already in the tree for vector export; a contact-sheet layout is a day. |
| G8 | **Architecture: flash/ambient ("flambient") blend assistant and window-pull luminosity masks** | The standard interior workflow is: ambient frame + flash frame → blend windows. Today it is manual layers. | Manual in PS; no reference app has it built in | Align (pc-engine `align_cmds.rs`) + luminosity mask + "darken" blend recipe as one command. Differentiator. |
| G9 | **Focus stacking** | Interiors at f/8 with foreground furniture; product. | Affinity, Helicon; LRC lacks it | Stack modes exist in `pc-algo/src/stack.rs` (serial, P5). |
| G10 | **Catalog backup on quit + verify** (see 2.7) | Data safety. | LRC backup on exit; 15.5 catalog repair | Journal is durable; the backup copy to a second drive is missing. |
| G11 | **Catalog-wide duplicate finder** (W5) | Event libraries. | LRC 15.4 | Reuse import's content keys and Smart Sort embeddings. |
| G12 | **Mask Feather/Edge controls** (Q6) | Hair and window edges. | LRC 15.5; C1 Refine Mask | Guided-filter radius/strength already exists internally. |
| G13 | **Check for updates** (U8) | Owner installs test builds; testers need to know a new one exists. | Every desktop app | Read GitHub releases JSON over rustls; link, do not auto-install. |

Best-in-class things the app already has and should market: raw smart object with live Library
link (no open-source editor has it), Capture One-style layers in a Lightroom-style module, People
→ per-person export folders, Smart Sort, the open model system with verified downloads, Model
Browser, MCP server + CLI for scripted workflows (`lc-mcp`, `local-image-cli`), a pure-Rust single
binary with no runtime, and no cloud at all.

### 2.7 Release readiness

| # | Area | State | Finding |
|---|---|---|---|
| R1 | Packaging | Linux: `tar.gz` + `./local-image --install` (menu entry, MIME); macOS: `.pkg`, ad-hoc signature, HEIF+SAM features; Windows: MSI/zip via xtask (CI tests on Windows but does not package it). `macos.yml` uploads the pkg; `ci.yml` builds no artifact. | No AppImage/Flatpak for Linux users outside Fedora; the Mac package is not notarised (Gatekeeper warning; `docs/MACOS.md` "Do not describe an ad-hoc package as notarized"); no Linux release artifact from CI. |
| R2 | Updates | None in-app (README claims otherwise). | Add "Check for updates" (G13). |
| R3 | Crash recovery | Compositing: autosave snapshots to a recovery dir, listed on launch (`pc-format/src/autosave.rs`); Library: fsync'd journal with torn-tail repair and batch rollback; preferences parse leniently with a backup notice. | Still open: NVIDIA SIGABRT on exit after Vulkan unload (`BUGFIX-BACKLOG.md`), ComfyUI orphaned after a crash (no watchdog), `Autosaver::discard` joins on the UI thread (Opus O-028). |
| R4 | Data safety | Save Over Original keeps `originals-backup/`; export never overwrites originals (`OriginalGuard`); XMP sidecars optional; Auto Sync no longer clobbers masks. | **No catalog backup** to another location, no "verify" (G10). The catalog is one `catalog.snap` + `catalog.log` in the library folder: a dead drive loses ratings, develop, people and keywords unless XMP is on. |
| R5 | Privacy | No telemetry (grep none); network only for model downloads, profile updates, LAN ComfyUI discovery on macOS; face data library-local and clearable; export can strip location and refuses Original copies with embedded places. | Write it down as a one-page privacy statement in docs and Help › About. |
| R6 | Licensing / attribution | GPL-3.0-or-later app; MIT/Apache forks; darktable/RawTherapee GPL ports with notices (`licenses/`); lensfun LGPL-3 + CC-BY-SA database; Lucide ISC; Krita AI Diffusion GPL port; models separate with NC flagged; attributions page + `xtask assets` gate. | Gate was red on 1,356 uncovered assets at review time and trips on test output under `target/`; must be green and `target/`-proof before a release. |
| R7 | Documentation | Developer docs are thorough; no user guide; Help opens the repo docs. | A 2-page "Coming from Lightroom Classic / from Photoshop" guide and the in-app shortcut sheet are enough for v2.0. |
| R8 | Validation discipline | STATUS doc lists the release gates (five GPU gates, 12 Rotate gates, HEIF build, archive/attribution checks, upgrade/rollback test, native workflow acceptance). | Good. Add: a real-photo AI quality set (Q3), reproducible exports (W12), and a green workspace with no allow-listed failures (A3). |

---

## 3. Prioritised roadmap

Value = to the owner's work. Effort: S = days, M = 1-3 weeks, L = 1-2 months, XL = more.

### Quick wins (days)

| Item | Value | Effort | Risk | Depends on |
|---|---|---|---|---|
| QW1 Move *Negative* below Detail/Optics and show it only when enabled or via "Add tool"; put White Balance first in Light (or rename Light → Basic with WB + Tone) (W7) | High | S | Low: UI order only; settings unchanged | — |
| QW2 One place for the module switcher (top-right, all modules) (U1) | Med | S | Low | — |
| QW3 Import dialog: "Build previews: Embedded / Standard / 1:1 / Smart" and "Make a second copy to…" (W1, W2) | High | S-M | Low: reuses `cmd/previews.rs` and `import_move::copy_new` | — |
| QW4 Catalog backup on quit (weekly, to a chosen folder, keep N) + "Verify library" (G10) | High | S | Low: copy snap+log atomically after a snapshot | — |
| QW5 Check for updates (GitHub releases JSON, rustls; link only) (G13) | Med | S | Low | — |
| QW6 Reproducible exports: fixed ICC creation date (W12) | Med | S | Low | — |
| QW7 CPU AI time estimate + refuse/warn above a threshold; tooltip estimates everywhere a model runs (Q4, P4) | Med | S | Low | — |
| QW8 Rename for Classic users: Albums → Collections, My Photos → Catalog, Detail → Loupe (U2, U7) | Med | S | Low: strings + icon; keep automation ids | — |
| QW9 Fix stale docs (whats-new, README update claim, targets) and add the two "coming from" pages (U8, R7) | Med | S | None | — |
| QW10 GPU failure recovery: retry Develop GPU after N seconds / when ComfyUI reports idle VRAM instead of disabling for the process (P1) | High | S-M | Med: needs a VRAM probe (wgpu adapter limits + ComfyUI `/system_stats`) | — |

### Next (weeks)

| Item | Value | Effort | Risk | Depends on |
|---|---|---|---|---|
| N1 **CR3 + compressed RAF/ORF decoders** (Q1/G1): port rawler's CRX and Fuji/Olympus compressed paths, with reference-vector tests like `lc-raw/tests/it/raw_speed.rs` | High | L | Med: decoder correctness; mitigated by golden hashes against rawler/darktable output | — |
| N2 **Face-aware culling** (G2): eyes-open/eye-sharpness per face, best-of-burst by face, filter bar + smart-album rules | High | M | Low-Med: model licence check; CPU cost (reuse People run) | People engine (exists) |
| N3 **On-device AI Remove with LaMa** (G4) | High | M | Low: tract ONNX, Apache-2.0; same patch store | — |
| N4 **One Preferences window** with tabs (U3) and one settings file; migrate `library-ui.json` keys leniently | Med | M | Med: two settings loaders | — |
| N5 **One codec/CMS stack** (A1 first slice): move Library export and Compositing Export As onto one JPEG/PNG/TIFF/WebP/AVIF encoder + one ICC embedder (W13) | High | M | Med: golden byte tests for both paths | — |
| N6 Mask Feather/Edge controls (G12) | Med | S-M | Low | — |
| N7 macOS: native menu bar for all modules and personas (U5) | Med | M | Med: Library menus through `mac_menu.rs` | — |
| N8 Library first-run card + catalog location choice (U6) | Med | S-M | Low | — |
| N9 Catalog open off the UI thread with progress (P2) | Med | S-M | Med: session construction ordering | — |
| N10 Battery awareness: lower thread count and pause indexing on battery (P3) | Med | S | Low: `/sys/class/power_supply` on Linux, `pmset -g batt` on macOS via the existing subprocess pattern | — |
| N11 Parallelise the remaining serial pixel loops (P5: `pixels.rs::adjusted`, `stack.rs`, restoration pre/post, enhance resizes, gallery thumbs off-thread) | Med | S-M | Low: per-pixel pure; byte-identical tests | — |
| N12 AI quality gate: a fixed CC0 real-photo set per engine (remove, cutout, enhance, generate) with pass/fail thresholds and before/after sheets (Q3) | High | M | Low | mock ComfyUI harness (exists) |
| N13 Test-suite health: zero allow-listed failures, a < 5-minute "fast" lane, nightly full + GPU lanes on the owner's boxes, `xtask assets` ignoring `target/` (A3, R6) | High | M | Low | — |
| N14 Proof sheet / PDF contact sheet export (G7, first half) | Med | S-M | Low: `krilla` | — |

### Strategic (months)

| Item | Value | Effort | Risk | Depends on |
|---|---|---|---|---|
| S1 **Portrait retouch assistant** (G3): face parsing, blemish detection with confirm, texture-preserving smoothing, eye/teeth, batch via Sync | High | L-XL | Med: model licences, quality bar | N2 (face pipeline), N12 (quality gate) |
| S2 **Shared core crates** (A1): `li-codecs`, `li-color` (CMS), one raw (delete `pc-raw`), one i18n, one metadata; both UIs on one widget/token crate | High (maintainability) | XL | Med-High: touches everything; do it slice by slice behind golden tests | N5 |
| S3 **Tether bar** (G5): folder-session UI first; pure-Rust PTP later | Med-High | L (UI) / XL (PTP) | High for PTP | — |
| S4 **Flambient / window-pull assistant and focus stacking** (G8, G9) | Med-High | M-L | Med | N11 (stack perf) |
| S5 **Upstream horizon**: port two more fix cycles with `upstream-check` extended to PhotoCraft/LightCraft paths, then declare the fork independent and stop tracking (A2) | Med | M | Low | — |
| S6 **Split `pc-ui-egui`** into 3-4 crates; measure incremental build time before/after (A4) | Med | M | Low | — |
| S7 **Signed/notarised macOS package, Flatpak/AppImage for Linux, CI release artifacts** (R1) | High (adoption) | M | Low | Developer ID |
| S8 **Catalog-wide duplicate finder** (G11) and session/people tooling for events | Med | M | Low | Smart Sort store |
| S9 **WGSL ports of the remaining CPU stages** (lens-DB sampling, non-linear calibration) (Q5) | Med | M | Med: parity tests exist | — |

---

## 4. What the app already does well (keep)

1. **One develop engine everywhere**, including Camera Raw Filter and the raw smart object with a
   live Library link: a real differentiator over every open-source editor and most paid ones.
2. **Measured, reference-checked image quality**: upstream C reference vectors, golden hashes,
   GPU parity within 0.5 LSB, bake-offs with CC0 sets for every on-device model.
3. **Performance policy that adapts**: one compute profile, memory-sized job permits, thread
   classes, bounded pipelines, drafts during slider drags, AI off the UI thread, idle at 0 fps.
4. **Data durability**: fsync'd journal with torn-tail repair, atomic writes everywhere, originals
   guard, Save Over Original backups, autosave recovery, lenient preference loading.
5. **Lightroom Classic keys by default with C1/PS/Affinity profiles**, a conflict-aware shortcut
   editor, plain-English errors with a Details sink, disabled-with-reason, accessible names.
6. **Privacy by construction**: no telemetry, local face data with a clear button, location
   stripping on export, verified model downloads from allow-listed hosts.
7. **Pure Rust, `unsafe` denied, no C toolchain**: one binary per platform, a `deps-pure` gate.
8. **Design-system discipline** written down and enforced in review (tokens, shared widgets,
   screenshots in both themes at two widths).
9. **Scriptability**: CLI, control channel, MCP server; headless tests drive the real UI.
10. **Honest engineering records**: every batch has a report with commands, counts and caveats,
    and the release gates are listed rather than assumed.

---

## 5. Sources consulted

- Code and docs in this worktree (file:line references above), the Opus and Astra final reviews in
  `~/.local/share/local-image-dev/reviews/`, `docs/wip/AUDIT-multithreading.md`,
  `REVIEW-threading-policy-fable.md`, `BUILD-pipelines.md`, the CODEX-REPORT-* batch reports, and
  the test screenshots under `target/*/` and `~/.local/share/local-image-dev/screenshots/`.
- Lightroom Classic release notes (15.3-15.5, 2026): https://helpx.adobe.com/ca/lightroom-classic/desktop/introduction-to-lightroom-classic/release-notes.html
  and https://helpx.adobe.com/ca/lightroom-classic/desktop/introduction-to-lightroom-classic/whats-new.html
- Capture One 16.8 (May 2026) and 16.8.5 (August 2026): https://www.captureone.com/explore-features/whats-new/16-8 ,
  https://www.captureone.com/explore-features/whats-new/16-8-5-upsell
- Aftershoot culling release notes: https://events.aftershoot.com/?p=43908 ; PetaPixel coverage:
  https://petapixel.com/2023/03/10/culling-software-aftershoot-rebrands-and-introduces-ai-editing-tools/
- Evoto feature set (vendor page and third-party roundups): https://www.evoto.ai/reviews/evoto-vs-imagen-ai ,
  https://imagen-ai.com/post/ai-image-retouching/
- Photo Mechanic culling model (embedded-preview speed, ingest with second copy):
  https://imagen-ai.com/post/photo-mechanic-vs-lightroom-culling/ ,
  https://havecamerawilltravel.com/photographer/extract-jpg-raw-2/ ,
  https://narrative.so/blog/top-5-photo-mechanic-free-and-paid-alternatives
- darktable 5.4 / 5.4.1 (capture sharpening, AgX, workspaces): https://www.darktable.org/2025/12/darktable-5.4.0-released/ ,
  https://www.darktable.org/2026/02/darktable-5.4.1-released/
