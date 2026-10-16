# PhotoCraft upstream triage (75ff41a2..72b499a2)

Read-only triage, 2026-10-12. Upstream: storytold/photocraft. Ours: Local Image v2 @ 6024e4c7.

## Summary

- **Range:** 1,450 commits (2026-10-07 22:58 to 2026-10-12 23:20 EDT), 32 of them merges.
- **Import boundary correction:** the fork's `c18d18cf` import already contains upstream first-parent history up to
  **e5e3e397** ("Bump version to 0.5.0", 2026-10-08 10:01 EDT). The evidence is that the new test names from 62b0d0bd,
  11f6f00c, e694208a, 244a99c3 and b10b37c1 are all in our tree (muda is in our Cargo.lock too). Those 26 commits are
  already in. The range that is really new is **e5e3e397..HEAD = 1,424 commits**. Three later upstream commits were
  already ported in "Vector V0 sync": d438f842 (#1321), 5355221f (#1495) and d4884d33 (#1807). Our pc-affinity
  already has 003f9b4a. Our pc-svg has its own depth limits (equivalent to 0af072e8).
- **Category tally of the 1,424 (heuristic, by subject and paths):**
  1,120 feature/fix code, 64 automation/MCP/CLI, 64 docs/CI/release/scorecard, 51 i18n, 32 merges,
  29 Windows/macOS-only, 28 tests-only, 22 web/wasm, 14 printing/shared craft-ui.
- **Ratings:** MUST 17 · SHOULD ~45 listed (plus a long tail of small PS-parity fixes) · NICE: grouped below ·
  SKIP: everything else (see tally).
- **Divergence:** almost nothing after the boundary is in our tree. The fingerprint check (new test fn names, 200
  candidates) found 0 of N present for nearly all of them. Our team fixed a few things independently (marked "y" below).
  For most files a cherry-pick will conflict, so expect manual ports. The parser and crate-internal fixes in pc-psd,
  pc-codecs, pc-format, pc-compose and pc-raw are small and self-contained, which makes them the cheapest.
- **Release profile:** no `overflow-checks`, so pure integer-overflow fixes wrap silently in release. They matter when
  the wrapped value feeds a slice index or an allocation size. The MUST list counts only those.

## Pure-Rust gate (new deps since fork)

| Upstream | Adds | Verdict |
|---|---|---|
| 0256eaa4 screen eyedropper/loupe (#1232) | `xcap` -> pipewire-sys, libspa-sys, gbm-sys, drm-sys, xcb, clang-sys, bindgen | **SKIP** (C/-sys). Would need a pure-Rust portal/X11 capture rewrite. |
| ccee4894 system printing | `craft-print` (git dep storytold/craft-libs; deps: tempfile) | SKIP for Linux release (printing; plus 14 print/craft-ui follow-ups) |
| 99a39703 UI.9 shared UI | `craft-ui` git dep | SKIP (shared-UI migration, conflicts with our UI) |
| b10b37c1 macOS menu | `muda` | already in ours |
| 510636f4 DNG JPEG XL | jxl-oxide, zune-jpegxl, brotli-decompressor (pure Rust) | OK if wanted (NICE) |
| c7dbb37b Motion Blur FFT | rustfft (pure Rust) | OK |
| f41eb4f7 SVG to shape layers | fontdb, rustybuzz, gif (pure Rust) | N/A, we have our own pc-svg |
| 82c58fab Fontconfig fonts | none: shells out to `fc-list` | OK |

## MUST

| Upstream sha / subject | Our file(s) | Fixed in ours? | Difficulty | Why |
|---|---|---|---|---|
| c1cb455c Fix #1381: selecting type on Wayland no longer deletes it | pc-ui-egui/src/type_tool.rs `ime_update` | n (an empty preedit still replaces a non-empty selection) | easy (6 lines + test) | Linux/Wayland data loss: selecting text deletes it |
| 9e6bfdc7 refuse to save a document holding a NaN/inf float (#1101) | pc-format (no `finite.rs`) | n | moderate (serializer walker) | A NaN from a PSD makes .pcraft save/autosave "succeed" but the file never reopens |
| 6e9ba651 Format: admit the declared .pcraft canvas before loading (#2935) | pc-format/src/lib.rs `LoadOptions` | n | easy | Hostile .pcraft declares a 300000² / u32::MAX canvas; work and memory scale with area |
| 73a56d76 Format: overflow-safe blob decode, bounds-checked ZIP reads (#3486) | pc-format/src/store.rs:36,226; zip.rs:209 | n | easy | `max as u64 + 1` overflow and an unchecked `bytes[off..off+len]` slice in the .pcraft reader |
| 5049a1d0 Never crash on hostile numbers: LUT sizes, Quick Selection, link ids (#869) | pc-compose/src/adjust.rs:206 `(*size).pow(3)*3` | n | easy-moderate | A stored Color Lookup size wraps the guard, then indexes out of bounds (panic from a file) |
| 7c77143b PSD: don't preallocate knots from untrusted counts (#3635) | pc-psd/src/path.rs:158 | n | easy (1 line) | 100k tiny subpaths x 4096-knot reservation = GBs from a small PSD |
| 21f1012c Codecs: refuse implausible PNG/WebP headers before allocating (#3691) | pc-codecs/src/codecs/png.rs:46, webp.rs:27, options.rs | n | easy | A tiny file reserves up to max_alloc (8 GiB) with an infallible `vec!`; allocation failure aborts |
| 476f9838 Codecs: bound the TIFF IFD walk by an entry budget (#3683) | pc-codecs/src/codecs/tiff_ifd.rs | n | easy | Overlapping IFD tables give a quadratic walk on hostile TIFF |
| 7fd61074 Fix PNG text metadata decode budget bypass (#1460) | pc-codecs/src/codecs/png.rs:69 | n | easy | zTXt/iTXt/XMP inflate outside max_alloc (zip bomb in metadata) |
| 308ef632 raw: charge the lossless-JPEG intermediate against max_alloc (#2925) | pc-raw/src/sensor.rs:229 | n | easy | Raw ljpeg buffer sized from geometry and not budgeted |
| 2842973d Compose: no overflow/unbounded allocation on hostile coordinates (#4427) | pc-compose lib.rs, masks.rs:145, proxy.rs:26, pattern.rs, effects.rs, adjust.rs | n | easy-moderate | Hostile vector-mask/path coords and canvas sizes: wrap into bad rects, OOB slice in proxy downsample, giant render buffer |
| f82a55f0 Fix #1543: oversized Satin / Bevel sizes render instead of panicking | pc-compose/src/effects.rs (no `box_width` clamp) | n (MAX_REACH exists but is not used in the tent kernel) | easy | 1e308 Satin size from a PSD gives "capacity overflow" panic or minutes-long blur |
| af3c99d8 Clone Stamp/Healing refuse a far source instead of slicing OOB (#1111) | pc-engine/src/retouch_cmds.rs:208 `clone_sample` | n | easy | OOB slice panic in pc-paint/retouch.rs with a far source offset |
| 3f72158f fix(psd): write kept 8B64 blocks as 8BIM in a PSD (#3743) | pc-io/src/psd_export.rs:1367 (`tb.signature = *sig`) | n | easy | A PSB re-saved as PSD is refused by Photoshop 27.11 ("not compatible") |
| 61b6276c Security: harden document exports and automation inputs (#2269), web-export/asset part | pc-engine/src/web_cmds.rs:1003-1130 (`parse_asset_name`, `rel = format!(...)`, `join`) | n | moderate (big commit; take only path-safety parts) | Layer names drive Generate Image Assets output paths: `../` from a hostile PSD writes outside the folder, and assets fire on every save. Also slice URL schemes and HTML-escaping in Save for Web |
| 81f97450 fix(smart-object): keep pixels when rasterizing a smart object (#3873) | pc-engine/src/extra_cmds.rs:548-560 | n (renders `layer.surface()` = cache, empty when unpopulated) | easy-moderate | Rasterize Smart Object can produce an empty layer (content lost) |
| 9cdcfbb1 Fix startup panic when UI Font Size is not Small (#1467) | pc-ui-egui/src/theme.rs:364,406-431 (plugin identical to pre-fix) | likely n: **verify** | easy | If the saved UI Font Size is applied in frame 1, egui's default stack replaces Inter and `FontFamily::Name("medium")` panics on every launch. Repro: set Medium, restart |

## SHOULD

| Upstream sha / subject | Our file(s) | Fixed? | Difficulty | Why |
|---|---|---|---|---|
| b97a72ac File dialogs no longer freeze or crash the app (#673, #574) | apps/local-image/src/services.rs:456-482 (sync `rfd::FileDialog`) | n | hard (async dialog service across ~15 ui files) | Linux: the event loop is frozen while a dialog is open, so GNOME shows "not responding". macOS: SIGABRT. MUST before the macOS build |
| 9b4562f2 Open dialog: upper/mixed-case extensions on Linux (#1439) | services.rs `OPEN_EXTS` filters | n | easy | Portal/zenity globs are case-sensitive: IMG_0001.JPG and DSC.NEF are hidden in File > Open |
| be877c27 keep filter jobs polling on idle Linux (#3961) | pc-ui-egui/src/jobs_ui.rs:137, lib.rs update | n | easy | A job started after `tick` gets no repaint, so on idle Wayland the doc stays locked (undo blocked) until the mouse moves |
| 3952fcef save filters lead with the file's own extension (#1178) | services.rs:36 `save_filters` | n | easy | .jpeg/.gif/.bmp fall to the Photoshop filter first; some backends append .psd ("photo.gif.psd") |
| 0fb4bbf1 PSD: don't inflate an over-budget ZIP merged image at parse (#2926) | pc-psd/src/compression.rs:250 `validate_planes` | n | easy | CPU zip-bomb at parse time (not memory) |
| 64a02107 PSD: cap descriptor prealloc from untrusted counts (#3636) | pc-psd/src/descriptor.rs:352-397 | partial (check_count bounds n by input size) | easy | Allocation amplification about 4-10x the file size |
| 16341720 / 04396dd8 / fd351918 PSD pattern padding, composite budget, layer-tree cap | pc-psd patterns.rs, pixels.rs, tree.rs; pc-io tiff_layers.rs | n | easy each | Further hostile-PSD/TIFF bounds (our `charge()` budget covers part) |
| dc256b8b PSD layer with bounds but no channel data (#755) | pc-io/src/psd_import.rs:131 | partial (`charge()` budget already refuses huge) | easy | Skip allocation for channel-less layers |
| 59820eca / 9af9df82 PNM decode bounded by file (#2938, #1110) | pc-codecs/src/codecs/pnm.rs | n | easy | Header-driven reservation |
| 86a62993 / 9d9e512d / 0bb46df6 / 88f84aab raw: orientation range, ORF short IFD0, DNG DefaultCropSize origin, DNG clipped-below-white-level | pc-raw dng.rs, orf.rs, ... | n | easy-moderate | Raw open correctness and robustness (only if pc-raw is the open path; the Library uses lc-raw) |
| b2003a43 / 3e935907 / fd21e734 / 0050f29a .pcraft tile index i32, deflate prealloc, pattern sizes, hex '+' | pc-format convert.rs, zip.rs:213 | n | easy | Corrupt .pcraft gives wrong rects / large reservation (bounded by max) |
| 4f33c815 keep an unreadable PSD Anno block instead of deleting it on save (#944) | pc-io/src/annotations_map.rs:255 | n | easy | Silent loss of PS annotations on re-save |
| 6547ca0b Multichannel alpha stays alpha; save keeps resources and global blocks (#4031) | pc-io multichannel_map.rs, psd_export.rs | n | moderate | Multichannel PSD re-save drops guides/paths/EXIF/IPTC |
| f86517a0 descriptor ids warp/list/time as length-4 strings (#4039) | pc-psd/src/descriptor.rs:61 `Id::new` | n | easy | Photoshop ignores warps of new SOs/type and regenerated comp lists |
| e2aa1934 / c833294e / 0d058f4c / 6f1c4481 / 8fd4aa4c PSD writer fidelity (4-byte fill padding, raw empty channels, Photo Filter blocks, TySh bytes, unchanged Txt2) | pc-io psd_export.rs, pc-text psd.rs | n | easy-moderate each | PS/psd-tools interop of saved PSDs |
| 5290930e / 25c0ec78 / 2819f370 Preserve CMYK fills through PSD; keep CMYK ink values when flattening/merging | pc-compose, pc-io, pc-engine | n | moderate | Wrong output in CMYK docs |
| 3cc0d484 Flatten All Layer Effects keeps layer and vector masks (#3804) | pc-engine/src/file_cmds.rs:938-1004 | n | easy | Masks destroyed (visual unchanged, editability lost) |
| f586e713 vector-mask cache keyed by content, not address (#4426) | pc-compose/src/lib.rs:433 | n, latent (the temp-clone callers come from upstream advanced.rs, not in ours) | easy | Prevents another layer's mask being reused; harden before porting Advanced Blending |
| f1433f4a restore the Lab-mix flag after a tile (#1112) | pc-compose/src/lib.rs:110-127 | n | easy | Nested rayon gives random sRGB-mixed tiles in Lab docs |
| b0451df3 / 980cf05b / 57681126 effect/mask cache keys (TSL, group child effects, tile-address reuse) | pc-compose lib.rs (blend_if hashed, `advanced` not), masks.rs | partial | easy-moderate | Stale effect/mask renders until purge |
| 60018dea / 5b051861 / 4543ef35 / bacc886a 32-bit: Add/Divide and HSL modes not clipped; signed HDR kept in text raster and transforms | pc-color blend.rs, pc-compose, pc-gpu compose.wgsl | n | moderate (CPU+GPU parity) | Wrong output in 32-bit docs |
| 66ab5012 / 45b22e74 / 96ec16ed / 8c2e16fb Precise centre inner glow blank; GPU coincident stops; dithered gradient depth; GPU re-upload on default-pixel change | pc-compose effects.rs, pc-gpu | n | easy-moderate | Wrong render |
| 95dbb01a / 65e8d5f0 GPU: preflight uniform-buffer limits; price display textures first | pc-gpu/src/lib.rs, pc-ui-egui gpu_canvas.rs | n | moderate | wgpu validation panic/device loss on big docs or low-limit GPUs |
| 87486746 file.new 300000² no longer fills 360 GB (#705) | pc-raster/src/lib.rs `fill_rect` | n | easy | New doc / fill at max size OOMs; shared Arc tile |
| 28ed1c11 / 8157d1a8 / 9c554e2e fallible allocation in large pixel ops; Free Transform and Image Size refuse past budget | pc-algo transform/warp, pc-engine | n | moderate | OOM abort instead of an error on huge transforms/resamples |
| 3953d783 Selection distance transforms faster and correct past 4K (#2093) | pc-algo/src/selection.rs | n | moderate | Correctness (feather/expand on >4K docs) + 7-10x speed |
| 18bb712f Save/Close waits for TIFF Options | discard_ui.rs | **y** (ours waits on tiff_options/save_worker) | n/a | already fixed |
| f8efd089 keep the layered original when saving a flat copy | pc-ui-egui/src/lib.rs `write_document` | **y** (`preserves_document`) | n/a | already fixed |
| c6a525cc brush release takes live-stroke pixels | pc-engine lib.rs `live_commit` | **y** | n/a | already present |
| 85572c61 / c25630b5 background save | save_worker.rs | **y** (own save worker) | n/a | already present |
| fb36a3fd never overwrite preset/actions files that failed to load (#3346) | pc-engine/src/preset_store.rs:540 | partial (brush groups noted as skipped; actions.json load failure gives empty list, later rewritten) | easy | User's actions file can be wiped |
| 57be0f58 Shortcuts dialog rejects malformed overrides instead of wiping (#956) | pc-ui-egui/src/prefs_ui.rs:1367 | n | easy | Custom shortcuts wiped |
| 278e2941 / 2425431c / 0954b8c0 / e1a874d9 / 0c2c268c / fd74419a / 3ac39e1e / 64db46f5 / 3f7d8afc / c88dbd26 / 7fec6e94 engine param bounds (channel.merge closes an unrelated doc, delete the last layer, link-id overflow, timeline, crop i32, CA fill, Puppet Warp, scatter...) | pc-engine various | n | easy each | Reached via automation/control or edge UI; some are panics, channel.merge is data loss |
| 8208761a keep edits to a video frame when the playhead moves (#1288) | pc-engine video_cmds.rs, timeline_cmds.rs | n | moderate | Data loss in video layers (niche) |
| 40d7e6a1 / 37929405 / 06dcdc12 / 17167e27 / 6cfed3a7 / 3188866b / d8217b34 history/mask/transform correctness | pc-engine | n | easy-moderate | Wrong pixels in history brush/fill from locked states, feathered edges, smart-filter masks after resize |
| e8850ac5 / a32ba211 / 0423f74b / 8ebdd367 / c234865b / f435eb0b / a8931135 lock/hidden-layer respect | pc-engine, pc-ui-egui | n | easy-moderate | PS parity: edits ignore locks or hidden layers |
| 83d5c543 / c20b81b2 / 3ef63161 autosave: report write failures, sweep orphans, thread-start failure | pc-format/src/autosave.rs, services.rs | n | easy-moderate | Silent autosave failure means no recovery |
| 1e1d3dca separate mip render targets | gpu_canvas.rs | n | moderate | Workaround gated to DX12, so Linux-irrelevant: low |
| 82c58fab / 1a9a39ea Linux font discovery via fc-list; CJK fonts through Flatpak host mounts | pc-text fonts.rs, cjk.rs | n | easy | Fonts in fontconfig-only dirs are missing on Linux |
| b3993e89 / d29d81d5 Linux: say when no file dialog can open; no false missing-library warning | apps/local-image linux_libs.rs, services.rs | n | easy | Linux first-run UX |
| dd2ce3da / 43b4742e / 69c01c1b GPU startup fallbacks (CPU mode presents via Vulkan; failed CPU start doesn't pin; keep backend after windowing errors) | apps/local-image/src/gpu_startup.rs (heavily modified by us) | n | moderate (our file diverged) | Startup robustness on odd Linux GPU stacks |
| Perf, worth taking: 9182f8f7 undo/redo damage only; 286ff735 add/remove layers recomposite only their area; d7078f36 new empty layer; 7107c800/3e90c81e paste/duplicate; 05c4cd68 Navigator damage; 6d03075e marching ants one mesh; 0140ffdf menus with thousands of layers; 82148e77 filter previews off UI thread; 7aba2bd3 font scan off UI thread; 269d8fed responsive startup during recovery; 54ff2070 adjustments per tile in parallel; 45c5001d GPU tile convert parallel; 614d0689/9b3c6359/40300964 selection ops on 24 MP; 1e9f1ae4/3c469bfd/be43d58f/97448b27 | pc-engine, pc-compose, pc-ui-egui | n | moderate (engine lib.rs diverged) | Big responsiveness wins on large docs |
| UX parity (small, user-visible): 2cb3b70d crop crosshair; 4d410346/b1b44595 flyout close/tooltips; 6f41b324 drag past edge auto-scrolls; 4cf8a5fd wheel/pinch zoom over pasteboard; 9e378277 switching tool commits crop; d5bab136 commit Free Transform on outside click; 4db47f2b Esc deselects; 7e6fc93a middle-click closes tab; ee437352 Ctrl+Tab docs; 28edd593 Zoom double-click 100%; b81808f6 Hand double-click fit; e7576cd9 Ctrl+arrow nudge; 1db0dde9 Auto-Select skips locked; 426747d7 Auto-Select honours mask; 5c9d9116 Shift ranges in visual order; e80fe372 keep spaces on rename; 05954668 "copy 2" naming; 2b90697d Delete with nothing selected deletes layers (port **with** c287ecab/cd36d1ea, or the mask-delete bug comes with it) | pc-ui-egui | n | easy each, many conflicts | PS/LR users feel at home |

## NICE (later; grouped)

- **File formats:** Krita .kra import (4705fc59 composite; 0a32b359 layered), DDS BC1-3 (10ba166a), DPX read/write
  (f48a72cb), FITS (a35af4e2), OpenRaster .ora (77221725), Paint.NET .pdn (db0fb078), PCX (e54adc25), XPM2 + X11
  colours, EXR parts/AOV layers/Cryptomatte (ff1d933c, 291f30da, a0e9ee21, 8029b061), DNG JPEG XL / deflate / lossy
  JPEG (510636f4, 08ca4457, b7526789), Fuji RAF uncompressed (c0541baf), Nikon compressed NEF (3d095783), monochrome
  LinearRaw DNG, animated GIF frames to video layers (f14b1cc3), TIFF JPEG/CCITT compression, BMP indexed tables,
  TGA alpha, ICO/GIF/BMP in Save As, Save keeps JPEG quality / WebP mode (62346f7f), Photoshop Actions .atn
  load/save (3204231d), .kys shortcut import (a81eaa41), ABR folder hierarchy, GRD import in gradient map, Camera Raw
  .xmp settings (ccfc878a).
- **Type:** type on a path (15aac8d7; we have pc-text/on_path.rs from VectorCraft, so compare first), variable-font
  axis sliders, OpenType feature buttons, spelling suggestions, Photoshop type formatting keys, faux bold AA,
  ligature/alternates kept on edit, Indic/Khmer/Thai/Arabic fallbacks, Show Font Names in English, smart quotes,
  live font preview on hover.
- **Tools:** Content-Aware Move live preview + Transform On Drop, Remove tool, Perspective Crop, Frame tool, Artboard
  tool, Freeform/Curvature Pen, Add/Delete Anchor, Convert Point, Pen Auto Add/Delete, Type Mask tools, Color
  Replacement, Color Sampler, Single Row/Column Marquee, Rotate View, Pattern Stamp, Red Eye, Vanishing Point plane
  editor, Puppet Warp outline mesh, Object Finder hover, Select and Mask refine brush/view modes/output to new doc,
  Eraser Block mode, Count tool fixes, Clone Source panel options, painting symmetry axes.
- **Adjustments and filters (measured against PS):** Shadows/Highlights, Hue/Sat, Color Balance, B&W, Vibrance,
  Selective Color, Brightness/Contrast legacy, Levels/Curves eyedroppers, Curves pencil, Image Size resampling,
  Lab-aware Invert/Posterize/Threshold, Color to Alpha, generic filter preview window, angle dials, Filter Gallery
  previews at the applied scale, smart filter drag between SOs, Displace smart-filter import, background smart-filter
  re-render (d3ae5aad).
- **Layer styles:** PS contour presets + Contour Editor, glow ramps, chisel bevel facets, Satin, gradient interpolation
  methods (b79d0a0b), Advanced Blending: knockout / blend as group / TSL (b0680fd4; port f586e713 with it).
- **Workflow:** History snapshots, New Layer dialog, Duplicate Layer dialog, Merge Group, Combine Shapes, layer color
  labels, layer comps fixes, Copy/Paste multiple layers (5d8e5c13), Layer > Export As of selected layers, Save for Web
  resize preview + GIF palette editing, Export settings remembered, recent files, document tab overflow/float, dock
  tab reorder, preferences search, toolbar shortcut remap, spring-loaded tool keys, Photoshop's tool/layer/selection
  keys (8d49ecf5, 53d814f6), numeric fields with units/scrubby labels/expressions, dropdown type-ahead.
- **Linux/desktop chrome:** caption buttons follow desktop layout/icon theme (f5f0f73f), appearance sync with system
  plus GNOME light default (8c3b6f8e, 94b60422, bfa523f9, 7bea7dae: our app has no appearance.rs), themes (Soft
  Dark/Light, Breeze, Adwaita, Solarized), native brush cursors on X11/Wayland (da5abd10; check it stays pure Rust),
  menus avoid panels/taskbar (b507e04f, 62f4b9d4), display-server preference for XWayland file drops (a132e37e).

## Skipped tally (of 1,424 new commits)

| Reason | Count |
|---|---|
| i18n / translations / catalogs | 51 (plus i18n hunks inside many code commits, ignored) |
| Docs, scorecard, README, CI, release/version bumps, packaging, xtask/corpus | 64 |
| Merge commits | 32 |
| Windows/macOS-only fixes | 29 |
| Web/wasm/Docker | 22 |
| Printing (craft-print) and shared craft-ui migration | 14 |
| Tests-only | 28 |
| Automation/MCP/CLI/control-channel (only the security/data-loss ones above are rated) | 64 |
| C/-sys deps (screen eyedropper xcap) | 1 |
| Already in ours (pre-boundary 26 + d438f842, 5355221f, d4884d33, 003f9b4a, 0af072e8-equivalent, 18bb712f, f8efd089, c6a525cc, b1b3dff8) | ~35 |

The remaining ~1,000 code commits are PS-parity features and fixes, which the NICE groups and the UX-parity row only
sample. A second pass by area (e.g. "all Layers panel commits") is the practical way to mine them once the MUST/SHOULD
ports land.

## Method / caveats

- The "Fixed in ours?" column comes from grepping our tree for the fix's key line, function or test name, and reading the
  surrounding code for the MUST rows and most SHOULD rows. Bulk rows ("Perf", "UX parity", engine param bounds) were
  fingerprint-checked only (0/N upstream test names present), not read.
- 9cdcfbb1 needs a runtime repro (set UI Font Size = Medium, relaunch) because it depends on the order of first-frame
  init in our app.
- f586e713 is latent in ours: the trigger path (advanced.rs temporary clones) doesn't exist yet.
- Working notes from the run are in `triage-notes-backup.md` next to this file.
