# Audit: duplicate systems in Local Image v2 (Fable, 2026-10-13)

Scope: every place where the same job is done twice (or more) in different ways across the `lc-*`
(LightCraft: Library/Develop), `pc-*` (PhotoCraft/VectorCraft: editor) and `li-*` crates plus
`apps/local-image`. Read-only audit on worktree `.claude/worktrees/integrate`, branch
`claude/v2-continue`, HEAD `c9fd969c`. No code edited, no cargo run, no GUI. Evidence is
`file:line` in this worktree (line numbers at time of writing). Line counts are `wc -l` of the
module(s) named.

Already fixed today and therefore NOT re-listed as open duplicates (only referenced where a residue
remains): one encoder set + one ICC/CMS path for export (`lc-codecs::encode` adapts to
`pc-codecs`, `lc-codecs::icc` delegates to `photocraft-cms`), `pc-raw` deleted, raw decode routes
`lc-raw` first then `rawler`, and the two Preferences windows (being merged by a running job).

Written incrementally. Status: **complete** (sections 1-5).

Owner's rules applied throughout: pure Rust and no C/-sys deps; libraries kept whole where
possible; design-system tokens and shared widgets only; owner decisions not re-opened (AI Remove =
brush → adjust → confirm; monochrome icons; vignette/grain/B&W whole-image only; SAM 3 off).
Jobs editing code right now (Match Look, Lightroom-Classic UX incl. one Preferences window,
release hygiene, GPU coordination, on-device LaMa, library safety, raw quality) are noted as
"RUNNING: <job>" wherever a finding overlaps their files.

---

## 1. Plain-language summary for the owner

**What I found.** Thirty places where the app does the same job two or more ways (D1–D30 in
§3), about 20 of them worth merging, the rest deliberate and listed in §5. After today's fixes
(one encoder set, one CMS, one raw developer) the biggest remaining doubles are below the
pixels, not in the UI: two resizers, eleven blurs, two decoder stacks, two metadata libraries
(one of them string-splicing XMP), six "atomic write" helpers with different safety, two GPU
devices on one card with two failure policies, three AI model registries, and a second Camera
Raw engine that still previews what another engine then commits.

**What combining them buys — quality first:**

1. **The Camera Raw Filter preview becomes what you get** (D17). Today the dialog previews
   with `pc-algo::camera_raw` (its own dehaze, noise reduction, grain) and commits with the
   Library's pipeline. On Dehaze, Texture/Clarity, NR and Grain the preview is visibly wrong.
2. **One resizer, one blur, one histogram** (D1, D2, D4). Library export and Compositing
   Export As give different sharpness at the same "long edge 2048"; AI cut-outs get dark fringes
   because the AI round trips resize straight alpha with the `image` crate; a 0.5 px mask
   feather in Develop silently does nothing (a bug the editor already fixed in its own copy,
   #706); the "luminosity" histogram is a different quantity in Develop (Rec. 709) and in
   Levels/Curves (R+G+B)/3.
3. **The Library opens HEIC, the editor opens JXL — after D8 both open both**, with the
   editor's fuzzed limits and the Library's colour interpretation on one decoder set.
4. **Metadata that round-trips** (D9). The editor's File Info reads and writes XMP by text
   search; the Library has a real EXIF/XMP/IPTC library. Fields edited in one half can be lost in
   the other; the "Camera" readout for Lens Correction reads fewer tags than the Metadata panel.
5. **No more "GPU off until restart"** and one VRAM budget (D13): one wgpu device shared by
   the window, the compositor, Develop and AI denoise, with the editor's strike/decay health
   model (retry per operation) instead of Develop's permanent switch-off. Canvas dither then
   also matches between modules.
6. **One Heal** (D18): Develop's Heal spot (frequency-separation approximation) vs AI Remove's
   local engine and the editor's Healing Brush (PatchMatch + Poisson). Put Develop's Heal on
   the patch engine it already has.
7. **Safer files** (D10): AI settings and the model inventory are written without fsync; Library
   writes have no Windows antivirus rename retry; one `li-fs` gives every write the best of both.
8. **One place for models** (D15), **one automation server** (D29: the Library's MCP tools are
   currently unreachable — `lc-mcp` has no binary and no dependents), **one job/progress model**
   (D14), **one soft proof** that can take a printer ICC in Develop too (D6).

**Speed and memory.** The shared resampler is double-precision, premultiplied and
memory-bounded (64 MB reads) on every path; AI round trips stop going through 8-bit buffers
(D3); the dormant SAM 3 port (candle + Metal) drops out of the default build (D16: faster
builds, smaller binary); duplicate embedded fonts (≈1.5 MB, D24) and the hand-written HTTP
client (D15) go; one GPU device means no double texture uploads for Develop layers.

**How to do it without breaking anything.** §4 lays out 29 Codex-sized jobs in two waves with
disjoint file ownership. Wave 0 (ten jobs, five at a time, nothing in the way): `li-raster`,
`li-color`, metadata on `lc-meta`, one Camera Raw engine, one automation layer, one decoder set,
a CPU test renderer for both UIs, SAM defeature, the shared `JobCtl`, and a merge bake-off.
Wave 1 waits for the jobs running now (GPU coordination → `li-gpu`; LaMa → Heal/matting/
registry; library safety → `li-fs`/blake3; LRC-UX → preferences store, UI kit, keys, menus,
notices). Translations (`li-i18n`) go last, as you asked.

**Top 8 by value:** D17 Camera Raw engine · D1/D2 resample+blur · D8 decoders (HEIC in the
Library) · D13 one GPU device · D9 metadata · D18 Heal · D10 atomic writes · D24 UI kit
(tokens/icons/fonts first).

---

## 2. Duplicate inventory table

Effort S = days, M = 1–3 weeks, L = 1–2 months, XL = more. "Winner" = which design the merged
backend is built from. Job ids refer to §4.

| # | Area | Copies | Evidence (first lines) | Size (lines) | Quality / speed difference | User-visible effect today | Winner / merge | Job | Effort | Risk |
|---|---|---|---|---|---|---|---|---|---|---|
| D1 | Resampling | `lc-raster::resample`, `pc-algo::resample`, `image::imageops` (≈30 sites in `li-ai`/`li-seg`), ≈10 ad hoc, GPU twin | `lc-raster/src/resample.rs:59`, `pc-algo/src/resample.rs:152`, `li-ai/src/ops.rs:943` | 212 / 504 / ≈400 | pc: f64 taps, premultiplied, 7 kernels, bounded memory; lc: f32, 4 kernels, no premultiply; image: 8-bit straight alpha, serial | Different sharpness Library export vs Export As; alpha fringes on AI cut-outs; third look for thumbnails | `li-raster` from pc core + lc generic `Pixel` | J1, J27 | M | low–med |
| D2 | Gaussian/box blur | 11 CPU copies, 2 WGSL | `lc-raster/src/blur.rs:7`, `pc-algo/src/blur.rs:72`, `fxutil.rs:165` | ≈900 | lc boxes-only, **no small-σ guard** (no-op ≤ 0.58 px); pc exact ≤ 4 px + guard (#706) | 0.5 px Develop feather does nothing; different blur look per module | `li-raster::blur` (pc policy + guard) | J1 | M | low |
| D3 | Dense pixel types | `lc-raster::Image`, `pc-algo::Image`, `pc-codecs::Image`, `Rgba8Image`, `Buffer`, `image::RgbaImage`, `ColorImage` | `pc-algo/src/image.rs:19`, `li-ai/src/ops.rs:943` | — | AI results pass through 8-bit before compositing into 16-bit/float docs | Precision loss on AI round trips | one `Frame<T>`; Surface/Image split stays | J28 | M | low |
| D4 | Histogram | `lc-raster::Histogram`, `pc-algo::RgbHistogram`, `pc-ui-egui::tone`, `edit_cmds::histograms`, 2 painters | `lc-raster/src/histogram.rs:19`, `pc-ui-egui/src/tone.rs:70` | ≈400 | luma = 709 vs (R+G+B)/3; exact vs thumbnail sampling | Different luminosity curves and clipping rules per module | `RgbHistogram` + luma + one widget | J1, J18 | S–M | low |
| D5 | Colour math | sRGB ×4, HSV ×5, HSL ×2, OkLab ×4, Bradford ×2, Lab ×5, luma ×12, named spaces ×3, Trc ×2 | `lc-color/src/perceptual.rs`, `pc-color/src/convert.rs`, `pc-cms/src/math.rs`, `lc-codecs/src/space.rs:168` | ≈1,200 | HSV copies differ on greys; luma weights differ; three enums for five spaces | Dialog mappings; inconsistent numbers | `li-color` (lc-color + pc convert + cms math) | J2 | M | low |
| D6 | Soft proofing | `lc-pipeline::ProofParams` (5 spaces, matrix) vs `pc-engine::color_cmds` (any ICC, BPC, gamut) | `lc-pipeline/src/output.rs:169`, `pc-engine/src/color_cmds.rs:350` | ≈150 | matrix vs CMS transform | Develop cannot proof a printer ICC | `pc-cms` Transform | J22 | S–M | low |
| D7 | Curves / LUTs | Fritsch–Carlson ×2; `.cube` parser ×2; trilinear vs tetrahedral | `lc-pipeline/src/lut.rs:21`, `pc-cms/src/lutfile.rs:93`, `pc-compose/src/adjust.rs:372` | ≈350 | pc: cube/3dl/look + tetrahedral | Same `.cube` renders differently; Develop refuses .3dl/.look | `pc-cms::lutfile` + tetrahedral in `li-color::lut` | J2 | S | low |
| D8 | Image decoding | 2 decoder stacks, 3 TIFF IFD readers, 2 PSD readers, 3 JPEG marker scanners, 2 sniffers, ≈9 extension lists | `lc-codecs/src/lib.rs:154`, `pc-codecs/src/lib.rs:45`, `lc-tiff/src/lib.rs`, `pc-codecs/src/codecs/tiff_ifd.rs` | ≈3,000 lc / ≈9,500 pc | pc: more formats (HEIC, EXR…), fuzzed limits; lc: JXL, DCT-scaled JPEG, SourceSpace; lc-tiff: only complete TIFF lib incl. writer | **Library cannot import HEIC**; editor cannot open JXL; PSD hardening differs | `pc-codecs` decoders + lc colour layer; `lc-tiff` for all IFD work; delete `lc-codecs::psd`, `::exif` | J6 | L | med |
| D9 | Metadata | `lc-meta` vs `pc-algo::exif`, `pc-codecs::orientation`, `pc-io::flat` byte patches, `pc-engine::file_cmds` string-search XMP | `lc-meta/src/lib.rs:1`, `pc-engine/src/file_cmds.rs:553`, `pc-io/src/flat.rs:371` | 4,573 vs ≈800 | lc: schema-aware XMP + EXIF write; pc: splicing | File Info does not round-trip Library↔editor; two EXIF upright rewrites | `lc-meta` (+`lc-tiff`) for both | J3 | M–L | med |
| D10 | Atomic writes | `lc-catalog::safe_file`, `pc-format::atomic`, `li-ai::settings::write_atomic`, ≈20 ad hoc | `lc-catalog/src/safe_file.rs:155`, `pc-format/src/atomic.rs:139`, `li-ai/src/settings.rs:162` | ≈900 | pc: Windows retry, symlink, mode, test seam; lc: durable/nosync policy, write_new; li-ai: **no fsync** | AI settings can be lost on power cut; no AV retry for Library writes | `li-fs` = union | J11 | M | low |
| D11 | Thumbnail caches | `lc-preview` vs `filmstrip_ui` worker (+own orient), gallery/doc thumbs | `lc-preview/src/lib.rs:30`, `pc-ui-egui/src/filmstrip_ui.rs:91` | 1,192 vs ≈150 | lc: disk cache, dedup, priorities | Filmstrip re-decodes each session, shows undeveloped thumbs | `lc-preview` service via host | J23 | S–M | low |
| D12 | Content hashing | SipHash-128 (catalog), blake3 (editor), sha256 (downloads) | `lc-preview/src/hash.rs:59` | — | SipHash not collision-resistant for duplicates | Linked-source hash ≠ catalog key | blake3 for identity | J24 | S | med (persisted) |
| D13 | GPU device / health | own device in `lc-gpu` vs eframe device; 2 backend policies; 2 crash sentinels; `BROKEN` vs `DeviceHealth`; 2 dither/sRGB WGSL | `lc-gpu/src/ctx.rs:531`, `lc-gpu/src/backend.rs:129`, `pc-gpu/src/health.rs:1`, `apps/.../gpu_startup.rs:1` | ≈2,000 | pc health retries per op; lc turns off for the process | OOM → Develop CPU-only until restart; two GPU settings; dither differs per module | `li-gpu` one device + pc health | J12 (after GPU job) | L | med–high |
| D14 | Jobs / progress / cancel | `Interrupt`, `pc-engine::Jobs`, `lc JobCtl`, `JobPool`, `Tasks`, `cull::Control`, `li-ai JobControl`, `li-seg`/`lc-fetch` flags; 3 progress UIs | `pc-raster/src/interrupt.rs:12`, `pc-engine/src/jobs.rs:218`, `li-ai/src/comfy.rs:78`, `lc-ui-egui/src/tasks.rs:21` | ≈1,500 | pc runner: snapshot + apply as undo step; Library tasks: no cancel | Cancel/progress behave differently per module | `JobCtl` + pc runner model | J9, J25 | M–L | med |
| D15 | Model download/registry | `li-ai::download` vs `lc-fetch` own HTTP client; 5 FileSpec types; 3 model dirs; 3 config dirs | `li-ai/src/download.rs:1`, `lc-fetch/src/http.rs:1`, `li-seg/src/lib.rs:42` | ≈2,500 | li-ai: allow-list + mandatory hash + cancel; lc-fetch: mirrors/resume, SAM only | Three places to manage models | `li-ai` registry + downloader; retire `lc-fetch` | J8, J15 | S–M | low |
| D16 | Segmentation stacks | `li-seg` (tract) shared; `lc-segment` SAM 3 (candle, **off** but in default features); `pc-algo::segment` classical; CLIP tokenizer ×2; edge refine ×2 | `lc-segment/src/lib.rs:1`, `apps/local-image/Cargo.toml:12`, `pc-algo/src/cf_matting.rs` | 1,923 + ≈1,200 | closed-form matting (editor) > guided filter (Develop) | Subject edges differ on hair; no Feather/Edge in Develop; dormant SAM in build | defeature SAM; `li-matte` | J8, J14 | S + M–L | low / med |
| D17 | Camera Raw engine | `pc-algo::camera_raw::develop` (preview, Lens Correction) vs `lightcraft_pipeline::render` (commit) | `pc-ui-egui/src/camera_raw_ui.rs:4,146`, `pc-engine/src/lens_cmds.rs:170`, `menu_catalog.rs:554` | 1,087 | different dehaze/NR/grain/working encoding | **Preview ≠ result** in the Camera Raw dialog | one engine; `CameraRaw` = PSD descriptor only | J4 | M | med |
| D18 | Heal / inpaint | `lc-pipeline::spots` freq-sep Heal vs `pc-algo::inpaint` PatchMatch + Poisson (already used by Develop AI Remove LOCAL) | `lc-pipeline/src/spots.rs:1`, `lc-engine/src/enhance/remove.rs:9`, `pc-algo/src/inpaint.rs:1` | 585 vs 904+328 | PatchMatch + Poisson higher quality | Two Heal results inside Develop | patch engine for Heal; spots = Clone | J13 (after LaMa) | M | med |
| D19 | NR / sharpen / dehaze | Develop stages vs Photoshop filters (+ D17's copies) | `lc-pipeline/src/detail.rs`, `pc-algo/src/denoise.rs` | — | different tools | none beyond D17 | keep (§5) | — | — | — |
| D20 | HDR / panorama | `lc-merge` vs `pc-algo::{hdr,panorama,features}` | `lc-merge/src/lib.rs:1`, `pc-algo/src/panorama.rs:1` | 3,433 vs ≈3,700 | lc: scene-linear raw merge, DNG out; pc: Debevec, layer alignment semantics; detectors unmeasured | Same brackets align/blend differently per module | `li-merge` after a bake-off | J10, J19 | L–XL | med |
| D21 | Lens / perspective / red-eye | lensfun Optics vs manual Lens Correction (keep); Upright vs Perspective Warp (keep); red-eye ×2 | `lc-pipeline/src/redeye.rs:1`, `pc-engine/src/redeye_cmds.rs:1` | ≈600 | lc red-eye: pet eye, GPU | editor has no lens DB; two red-eye results | bridge lens profile; lc red-eye | J26 | S–M | low |
| D22 | Preferences / dirs | `preferences.json` vs `library-ui.json` vs `AiSettings`; config_dir ×3; models dir ×3 | `pc-engine/src/prefs.rs:1732`, `apps/.../library_host.rs:1395`, `li-ai/src/settings.rs:36` | — | pc store: lenient + backups + recovery | Two dialogs (being merged), GPU/language in both | one `PrefsStore` with sections | J16 (LRC-UX) | M | med |
| D23 | i18n | JSON+formats (lc) vs TSV+plurals+contexts (pc); `tr` vs `tl!`; 12 langs × 2 | `lc-ui-egui/src/i18n.rs:1`, `pc-ui-egui/src/i18n/mod.rs:1` | ≈19k + ≈47k catalog lines | pc model richer; lc has compile-checked formats | Shared strings translated twice; no plurals in Library | `li-i18n` = pc engine + `tr_format!` | J29 (last) | L | low |
| D24 | UI kit | 6+ widget pairs, 2 `Tokens`, hand mapping, 2 icon registries/rasterisers, fonts embedded twice | `DESIGN-SYSTEM.md` §3; `apps/.../library_host.rs:49`; `lc-ui-egui/src/icons.rs:251`; `pc-text/src/fonts.rs:22` | 853+1,149 widgets; 269+798 theme | same look, two codebases; ≈1.5 MB duplicate fonts | Controls change shape between modules; icon stroke differs | `li-ui-kit` | J17, J18 | M + L | low / med (visual) |
| D25 | Shortcuts | 2 dispatchers, 3 chord parsers, `#[path]` cross-crate include | `lc-ui-egui/src/shortcuts.rs:36`, `pc-ui-egui/src/shortcuts.rs:10`, `native_menu.rs:279`, `pc-ui-egui/src/keymaps.rs:7` | 1,814 | parse/mapping differences | profile parsed differently per module | `li-keys` | J20 | M | med |
| D26 | Notices / toasts | `pc notices` (history) vs `lc toast` (1.4 s) | `pc-ui-egui/src/notices.rs:1`, `lc-ui-egui/src/lib.rs:514` | ≈250 | history vs none | Library warnings vanish | pc model in `li-ui-kit` | J18 | S–M | low |
| D27 | Commands / menus | identical `CommandSpec` ×2, `UI_COMMANDS` ×2, menu builders ×2, native bar editor-only | `lc-engine/src/cmd/mod.rs:44`, `pc-engine/src/commands.rs:15`, `pc-ui-egui/src/native_menu.rs:1` | ≈6,800 | — | Mac menu bar native in one module only | `li-commands` types + menu model | J21 | M | med |
| D28 | Test harness | `softpaint` CPU rasteriser (lc) vs kittest+wgpu (pc, GPU-gated); 6 RTX preludes | `lc-ui-egui/src/softpaint.rs:1`, `pc-ui-egui/src/design_system_shots.rs` | 470 + 2,419 | CPU deterministic vs GPU-ignored | editor visual gates partly ignored | `li-testkit` | J7 | M | low |
| D29 | Automation | `lc-mcp` (hand JSON-RPC, **no binary, no dependents**) vs `pc-automation` (rmcp, security) ; no Library CLI | `lc-mcp/Cargo.toml`, `pc-automation/src/lib.rs:1`, `apps/local-image-cli/src/main.rs` | 1,441 vs 3,093 | pc: security, budgets, bridge | Library tools unreachable by agents | `pc-automation` hosts Library tools | J5 | M | low |
| D30 | Brush dab generation | `pc-paint::dynamics` vs `lc-pipeline::masks::brush_dabs` | `pc-paint/src/lib.rs:1`, `lc-pipeline/src/masks.rs:387` | 835 vs ≈130 | pc: pressure/smoothing/spacing dynamics | Develop brush feels different on a tablet | later: lc uses `pc-paint::dynamics` | — | S–M | low |

---

## 3. Detailed findings per duplicate

Numbering: **D1…** Each finding gives the copies with evidence, what differs, who wins, the
user-visible effect today, and the migration slices (effort S = days, M = 1–3 weeks, L = 1–2
months, XL = more). Crate names in the plan use the `li-*` prefix for new shared crates; where
a shared crate already exists in one half (e.g. `lc-meta`, `pc-cms`) the plan keeps it whole and
points the other half at it, rather than creating a third copy.

### 3.1 Pixel foundations

#### D1 Resampling (resize / fit / thumbnails): four CPU engines and one GPU engine

| Copy | What it does | Evidence | Size | Callers |
|---|---|---|---|---|
| A `lc-raster::resample` | Separable; filters Box / Bilinear / Mitchell / Lanczos3; `f32` weights; generic `Pixel` (f32, [f32;3], [f32;4]); `par_rows` parallel; **no alpha premultiply**, clamp edges | `crates/lc-raster/src/resample.rs:5-170` | 212 | Library export (`lc-engine/src/export.rs:472` Mitchell), thumbnails (`lc-codecs/src/convert.rs:155` Box ≥ 2× else Mitchell), `lc-pipeline` (auto, layers, local, primary, geometry), `lc-merge`, `lc-engine/enhance/{remove,denoise}`, `camera_preview`, `lc-scenes`, `lc-raw/binned` |
| B `pc-algo::resample` | Separable over the tiled `Surface`; Nearest / Bilinear / Bicubic (Keys a = −0.5) / Lanczos / PreserveDetails (Lanczos + USM) / BicubicSmoother (Mitchell) / BicubicSharper (Keys a = −0.75); **f64 taps**, **premultiplied alpha**, canvas-edge clamp (`resize_surface_in_canvas`), streamed 64 MB read budget, 64-row bands in rayon, `prune()` | `crates/pc-algo/src/resample.rs:12-306` | 504 | Image Size (`pc-engine/src/image_cmds.rs` ×12), Export As / Save for Web (`file_cmds.rs`, `web`), smart objects, float conversions, variables, `pc-algo/transform.rs`, Wide Angle |
| C `image::imageops::resize` | The `image` crate's resampler (Lanczos3 / Triangle / CatmullRom), `u8` pixels, no premultiply, serial | `crates/li-ai/src/ops.rs:318,393,414,586,633,659,682,701,724,747,768,825,943,947`; `imaging.rs:188,392,402`; `cloud.rs:454,461,672`; `refine.rs:48,289,297`; `inpaint.rs:145`; `restoration/byo.rs:492`; `li-seg/src/clip.rs:110`, `restoration.rs:182` (own bilinear), `lib.rs:729` (own premultiplied resize) | ≈30 call sites | Every AI round trip (Generate, AI Remove context, Enhance, Restore, Cutout, Smart Sort CLIP crops) |
| D ad hoc | `pc-algo/seam.rs:351 resize_bilinear`, `cf_matting.rs:162-411` (4 fns), `inpaint.rs:83 downsample`, `pc-compose/proxy.rs:12` (point sampling), `pc-engine/vector_packages/trace_output.rs:305`, `pc-automation/server.rs:327 downscale_png`, `lc-merge/{frame,hdr}.rs shrink`, `lc-raw/highlight.rs:63` | — | ≈400 total | internal |
| E `lc-gpu::render::resize` | WGSL twin of A for the Develop GPU path | `crates/lc-gpu/src/render.rs:362` | — | Develop GPU render |

**Differences.** B is the better resampler: double-precision taps, premultiplied alpha (A smears
colour from transparent pixels when it ever meets RGBA; today A only sees opaque RGB so the bug is
latent, but the AI round trips in C resize **straight** RGBA with `image` and do get fringes on
cut-outs), seven kernels including the two Photoshop enlargement/reduction cubics, bounded memory,
and edge clamping. A is simpler and generic over the pipeline's `Image<T>`; it has only four
filters and no bicubic. C is the weakest (8-bit, straight alpha, serial) and sits on the hot path of
every AI tool.

**User-visible today.** The same photo exported at "long edge 2048" from the Library (Mitchell) and
from Compositing › Export As (Bicubic, or whatever Image Size last used) has different
sharpness; neither dialog offers the other's kernel list. AI cut-outs (`li-ai/ops.rs:747` resizes
the alpha with Lanczos3 separately from the colour) can show dark fringes that the editor's own
Image Size would not. The 2× box pre-shrink for thumbnails (`convert.rs:155`) is a third look for
the same photo in the grid.

**Winner / merge.** New crate **`li-resample`** (no workspace deps except rayon): B's kernel set
and f64 tap builder, premultiply-when-alpha, edge modes (clamp / transparent), generic over
interleaved `f32` slices with `n` channels, rayon bands with a read budget. Adapters: `Image<T>`
in `lc-raster` (keeps the `Filter` names as aliases), `Surface` in `pc-algo` (keeps the streaming
tile reader), `image::RgbaImage` helper in `li-ai`/`li-seg` (replacing every `imageops::resize`).
Default kernels stay per product (Lightroom-style exports: Lanczos3 for reduction is the quality
choice; Photoshop Image Size: Bicubic Automatic), but both export dialogs list the same kernels.
`lc-gpu` keeps E but gains a parity test against `li-resample`.

**Slices.** (1) Create `li-resample` from B's core + A's `Pixel` generic; byte-golden tests that
A's four filters and B's seven produce identical output through the adapters (S–M, low risk).
(2) Replace C in `li-ai`/`li-seg` (premultiplied) with before/after fringe tests on a CC0 cut-out
(S, low). (3) Delete the ad hoc copies in D where they are plain resizes (S). (4) Export dialogs:
one kernel list (S, UX; wait for the LRC-UX job on `lc-ui-egui/src/panels/export_dialog.rs`).

#### D2 Gaussian / box blur: eleven CPU implementations, two GPU

| Copy | Approach | Evidence | Notes |
|---|---|---|---|
| `lc-raster::blur::gaussian` | 3 box passes (Kovesi radii) | `crates/lc-raster/src/blur.rs:7-105` | **no small-σ guard**: `box_radii(σ ≤ ≈0.58)` = `[0,0,0]` → the call is a no-op. Callers that hit it: `lc-pipeline/src/masks.rs:365` (`sigma.max(0.5)`: a 0.5 px feather does nothing) |
| `lc-raster::blur::gaussian_fine` | exact taps | `blur.rs:106-170` | used by Detail USM |
| `pc-algo::blur::gaussian` | exact kernel ≤ 4 px, 3 boxes above; premultiplied | `crates/pc-algo/src/blur.rs:11-110` | the Photoshop filter |
| `pc-algo::fxutil::gauss_blur_n` | 3 boxes **with the σ < 0.577 guard (#706)** | `fxutil.rs:157-200` | the same bug as lc's, fixed only here |
| `pc-algo::pyramid::box_blur1`, `content_aware::box_blur`, `matting::gaussian_blur`, `selection_blur::{feather,box_blur}`, `pc-compose::masks::gaussian` (mask feather), `lc-raw::numerics::gaussian9` (darktable 9-tap), `lc-pipeline::eigf::gaussian` (darktable), `lc-engine/smart_sort/bursts.rs:416 box_blur` (Rgba8) | various | listed above in §3 search | each a private copy |
| GPU | `lc-gpu/src/render.rs:324,1556` (`gaussian`, `gaussian_fine`), `pc-ui-egui/src/gpu_canvas.rs:2564` (WGSL `box_blur`) | | |

**Differences.** Quality: `pc-algo::blur` (exact for small radii, boxes for large) is the right
policy; `lc-raster::blur::gaussian` is boxes-only and silently no-ops below σ ≈ 0.58, which is a
real (if small) Develop defect in mask feathering at low preview sizes. The darktable ports
(`gaussian9`, `eigf`) are faithful kernels with their own edge rules and must stay as ports.

**User-visible today.** A 0.5 px mask feather in Develop does nothing; the editor's identical
operation blurs. Two different "gaussian" looks between Develop masks and editor masks at the
same radius (3-box vs exact).

**Winner / merge.** `li-raster::blur` (inside the same new foundation crate as D1 or in
`li-resample` renamed `li-raster-ops`): exact-kernel-below-4-px + 3-box-above with the #706
guard, premultiply option, generic over channel count, plus `min_filter`/`max_filter` and the
Kovesi radii helper exported for the GPU twins. The darktable ports keep their own kernels
(provenance) but their callers no longer need `lc-raster::blur`. Both WGSL twins get a parity
test against the one CPU reference. Effort M; slices: (1) crate + goldens against both current
outputs (document the intentional change: small σ now blurs), (2) `lc-raster` adapter + delete
`blur.rs` body, (3) `pc-algo` adapters (`blur`, `fxutil`, `matting`, `selection_blur`,
`pc-compose::masks`), (4) GPU parity tests. Overlap: none of the running jobs own these files.

#### D3 Pixel buffer types: seven dense/tiled image types and their bridges

`lc-raster::Image<T>` (dense, `Rgb32f`/`Rgba8`/`Plane`; `lib.rs:48-250`), `pc-raster::Surface`
(tiled COW, any `PixelFormat`; `lib.rs:51-662`), `pc-algo::image::Image` (dense `f32` region of
a Surface; `image.rs:19-170`), `pc-codecs::Image` (bytes + `SampleType`; `image.rs:114-440`),
`pc-raster::Rgba8Image` (`lib.rs:757`), `pc-compose::Buffer`, `image::RgbaImage` (every
`li-ai`/`li-seg` boundary), `egui::ColorImage` (12 upload sites in `pc-ui-egui`). Bridges:
`pc-io/src/develop_filter.rs:239 surface_to_working`, `pc-io/src/raw.rs` (Rgb32f → Surface),
`pc-compose/src/lib.rs:1034 surface_to_buffer`, `pc-engine/src/ai_cmds.rs:217 surface_from`,
`apps/local-image/src/doc_layers.rs`.

**Verdict: mostly intentional** (tiled COW documents vs dense pipeline frames are the right
split; see §5). The duplication worth removing is the *dense float region* family:
`lc-raster::Image<[f32;N]>`, `pc-algo::image::Image`, the `Vec<[f32;3]>` in `li-seg`, and the
`u8` `RgbaImage` detours in `li-ai`. **Merge:** one `li-raster::Frame<T>` (dense, row-major,
`Pixel` trait, premultiply helpers, `Edge` modes) that `pc-algo::image::Image` becomes a thin
wrapper of and `lc-raster::Image` is renamed to; `li-ai`/`li-seg` take `Frame<[f32;4]>` and
encode PNG only at the ComfyUI wire. Effort M, mechanical; slices per crate. User-visible gain:
no 8-bit truncation on the AI round trips (today every AI Remove/Enhance result passes through
8-bit `RgbaImage` before it is composited back into a 16-bit or float document:
`li-ai/src/ops.rs:943-947`).

#### D4 Histogram: five computations, two luminance definitions

| Copy | Input | Luma | Sampling | Evidence |
|---|---|---|---|---|
| `lc-raster::Histogram::of_srgb8` | sRGB8 preview | BT.709-ish (54,183,19)/256 | ≤ 1 MP stride | `crates/lc-raster/src/histogram.rs:19-36`; used by `lc-engine/src/media.rs:594,748`, `export.rs:2541` |
| `pc-algo::histogram::RgbHistogram` | straight float RGBA | none (RGB only) + exact shadow/highlight endpoint counts, under/overflow, transparent/invalid | all pixels | `crates/pc-algo/src/histogram.rs:8-60`; Camera Raw dialog |
| `pc-ui-egui::tone::compute_histograms` | 384 px composite thumbnail | **(R+G+B)/3** | thumbnail | `crates/pc-ui-egui/src/tone.rs:70-121`; Levels/Curves |
| `pc-engine/src/edit_cmds.rs:282 histograms` | u64 per channel | — | — | Auto Levels/Contrast |
| `pc-algo/src/quantize.rs:167`, `noise.rs:108 median_histogram` | palette / median | — | — | internal, keep |

Two painters: `pc-ui-egui/src/widgets.rs:531 rgb_histogram` (tokens `histogram_bg/level`) and
`lc-ui-egui/src/panels/edit.rs:869` (own painter).

**User-visible today.** The "luminosity" curve is a different quantity in Develop (BT.709
weights) and in the editor's Levels/Curves (channel mean); the editor's Levels histogram is
built from a 384 px thumbnail while Camera Raw's is exact; highlight clipping warnings use
different rules (Develop: bin 255; Camera Raw: exact ≥ 1.0 before rounding).

**Winner / merge.** `RgbHistogram` as the single data type (richer: exact clipping, overflow,
transparency), extended with a `luma` channel using the Rec. 709 weights of the working space
(`lc-pipeline/src/output.rs:133 luma()` already defines them per output space) and the stride
sampler from lc for big previews; one `li-ui-kit::histogram` widget on the shared scope tokens.
Effort S–M. Slices: (1) type + luma + goldens (S), (2) Library panel on the widget (S; wait for
LRC-UX job which owns `panels/edit.rs`), (3) Levels/Curves on the type (S).

#### D5 Colour math: the same formulas in three to twelve places

`srgb_to_linear`/`linear_to_srgb` ×4 (`lc-color/src/transfer.rs:6,11`, `pc-color/src/convert.rs:162,168`,
`pc-trace/src/colorfit/oklab.rs:22`, WGSL `pc-ui-egui/src/gpu_canvas.rs:2542,2548`);
`rgb↔hsv` ×5 (`lc-color/src/perceptual.rs:84,92`, `lc-color/src/profile.rs:178` (private copy
inside the same crate), `pc-algo/src/camera_raw.rs:419,434`, `pc-paint/src/dynamics.rs:422,438`,
`pc-ui-egui/src/color_picker_ui.rs:19,35`); `rgb↔hsl` ×2 (`lc-color perceptual.rs:109,118`,
`pc-compose/src/adjust.rs:657,676`); OkLab ×4 (`lc-color perceptual.rs:50-65`,
`pc-testkit/src/vector/mod.rs:113`, `pc-vecfx/src/blend/paint.rs:14`, `pc-trace colorfit/oklab.rs`);
Bradford ×2 (`lc-color/src/lib.rs:155`, `pc-cms/src/math.rs:61`); Lab ×5 (`pc-cms/src/math.rs:95-124`,
`pc-color convert.rs:204,215`, `pc-algo/src/segment/slic.rs:12`, `quantize.rs:264`,
`pc-engine/src/preset_import_cmds.rs:263`); `luma` ×12 with at least three weight sets
(709 in `lc-pipeline/output.rs:133`; `pc-algo/retouch.rs:24`, `tone.rs:16`, `hdr.rs:27`,
`stylize.rs:14`, `segment/sky.rs:45`, `relight.rs:253`, `camera_raw.rs:201`, `li-seg/restoration.rs:170`,
`lc-engine/camera_preview.rs:137`, `lc-pipeline/cull.rs:11`, `lc-ui-egui/theme.rs:244`).
Named spaces ×3: `lc-codecs::space::NamedSpace` (`space.rs:168`), `lc-pipeline::OutputSpace`
(`output.rs:16`), `pc-cms::Builtin` (`builtin.rs:18`). Transfer curves ×2: `lc-codecs::space::Trc`
(`space.rs:9`) vs `pc-cms::Curve` (adapter `lc-codecs/src/icc.rs:1-60` converts between them).

**Differences.** Numerically the sRGB and Lab copies agree to float rounding (checked by reading:
same constants); the HSV copies differ in hue wrap and grey handling (`color_picker_ui` keeps hue
for greys, `camera_raw` does not); the `luma` copies are not the same quantity (709 vs 601 vs
mean), which is fine where it is a heuristic weight but wrong where it feeds a user-facing
number (D4). Three enums for five spaces means every export/proof dialog maps between them.

**Winner / merge.** **`li-color`** = `lc-color` (RgbSpace, CCT, transfer, OkLab, splines) +
`pc-color::convert`'s Lab/gray/CMYK-naive + `pc-cms::math`; `pc-cms` then depends on
`li-color` (it currently depends on no workspace crate by rule; the rule can move down one level
since `li-color` has no deps but `serde`). One `NamedSpace` with `to_builtin()`/`rgb_space()`,
one `Trc`/`Curve`. Every private copy becomes an import. Effort M, mechanical, byte-identical
unit tests per function. Overlap: Match Look job (`wip/cx-matchlook`) likely adds colour
statistics code — it should import, not copy; sequence D5 before or alongside and tell that job
the crate name.

#### D6 Display colour management and soft proofing

Monitor detection and the canvas transform are **already shared**: `apps/local-image/src/monitor_profile.rs`
(macOS/Linux detection) feeds `pc-engine::display_color::CanvasDisplay`, and the Library's
renderer uses the same object (`crates/lc-ui-egui/src/render.rs:101-108,228-237`). Keep (§5).

What is still double: **soft proofing**. Develop proofs only to the five `OutputSpace`s with a
matrix path (`crates/lc-pipeline/src/output.rs:169-215 ProofParams`; UI
`lc-ui-egui/src/panels/edit.rs:957`), while View › Proof Setup in the editor proofs through any
ICC profile with intent + BPC + gamut warning (`pc-engine/src/color_cmds.rs:350-355`,
`pc-cms::GamutCheck`). **User-visible:** a printer or lab ICC cannot be soft-proofed in Develop,
and the two proofs of "Adobe RGB" use different maths (matrix vs CMS transform with BPC).
**Winner:** `pc-cms` transform; `ProofParams` becomes a cached `Transform` on the 8-bit
output (as `CanvasDisplay` already does). Effort S–M; owner of `panels/edit.rs` is the LRC-UX
job, so the UI slice waits.

#### D7 Curves and 3D LUTs

Curve interpolation: `lc-color::spline::MonotoneCurve` (Fritsch–Carlson, `spline.rs:8-115`),
`pc-algo::camera_raw::curve_lut` (Fritsch–Carlson again, `camera_raw.rs:334-412`),
`lc-pipeline::basecurves::hermite` (`basecurves.rs:44`, darktable port, keep),
`pc-compose::adjust::curve_lut` (natural cubic, Photoshop parity, `adjust.rs:584-600`, keep).
`.cube` parsers ×2: `lc-pipeline::lut::Lut3d::parse_cube` (cube only, trilinear sample,
`lut.rs:11-62`) vs `pc-cms::lutfile` (cube + 3dl + look, writer, built-ins, `lutfile.rs:80-243`)
sampled by `pc-compose::adjust::lut3d_sample` (trilinear **or tetrahedral**, `adjust.rs:371-400`).

**User-visible today.** The same `.cube` applied as a Develop LUT profile (trilinear, on
display-encoded output, blended by amount) and as a Color Lookup layer (tetrahedral available,
dither) renders differently; Develop refuses `.3dl`/`.look` files that the editor accepts.
**Winner:** `pc-cms::lutfile` parser + the tetrahedral sampler moved to `li-color::lut`;
`lc-pipeline::lut` keeps only its profile registry. The second Fritsch–Carlson goes away with
D16 (Camera Raw engine). Effort S.

### 3.2 Files, codecs and metadata

#### D8 Image decoding and format detection: two decoder stacks, three TIFF readers, two PSD readers, three JPEG marker scanners

| Job | Library (`lc-*`) | Editor (`pc-*`) |
|---|---|---|
| Sniff / detect | `lc-codecs::sniff` (`sniff.rs:82-160`, 279 lines; knows raws, HEIF, AVIF, JXL) | `pc-codecs::format::detect` (`format.rs:223-320`) + `from_extension` |
| JPEG | `zune-jpeg` full + `jpeg-decoder` DCT-scaled/CMYK, own marker walk (`lc-codecs/src/jpeg.rs`, 262) | `zune-jpeg`, own marker scanner (`pc-codecs/src/codecs/jpeg.rs`, 378) |
| JPEG markers (third copy) | `lc-meta::jpeg_segments` (`container.rs:69`) | — |
| PNG | `png` crate (`png_codec.rs`, 116) + `lc-meta::png_chunks` | `png` crate (`codecs/png.rs`, 631) |
| TIFF pixels | `tiff` crate (`tiff_codec.rs`, 304) | `tiff` crate + own IFD walker `tiff_ifd.rs` (448) + pages (`codecs/tiff.rs`, 1200) |
| TIFF/EXIF directory | `lc-tiff` (1,641: classic + BigTIFF, SubIFDs, makernotes, **writer**) and a second minimal walker `lc-codecs/src/exif.rs` (166) | `pc-codecs/src/codecs/tiff_ifd.rs` (448) and ad hoc walkers in `pc-io/src/flat.rs:371-430` (`drop_exif_thumbnail`, `update_exif_dimensions`), `pc-codecs/src/orientation.rs:169-200`, `pc-algo/src/exif.rs` (240) |
| PSD | own merged-composite reader `lc-codecs/src/psd.rs` (299) | `pc-psd` (11,247: full layered, round-trip, PSB) |
| WebP | `image-webp` (`webp.rs`, 29) + `lc-meta::webp_chunks` | `image-webp` + own VP8 lossy encoder (`codecs/vp8/`, ≈2,300) |
| JXL | `jxl-oxide` (`jxl.rs`) | — (not supported) |
| HEIF/HEIC | **detected, not decoded** (`sniff.rs:69 can_decode → false` for Heif) | `pc-heif` (heic-rs) behind feature `heif` |
| EXR / PNM / TGA / QOI / HDR | — | `exr`, `pnm.rs`, `via_image.rs` |
| Output type | linear `Rgb32f` + `SourceSpace` (ICC applied) + orientation reported | bytes + `SampleType` + raw ICC/EXIF/XMP + `Limits` + `DecodeWarning` |
| Limits | `thumbnail_max_pixels`, `TooLarge` | `Limits` struct, fuzzed (`pc-codecs/fuzz`) |

Encoders are already one set (today's work). Decoders are not.

**Differences.** The editor decodes more formats (HEIC, EXR, PNM/TGA/QOI/HDR, TIFF pages) with
explicit limits and warnings; the Library decodes JXL, has the DCT-scaled JPEG fast path and
embedded-preview thumbnails (`decode_thumbnail`, `lib.rs:238-250`), and turns everything into
linear working space with a tracked `SourceSpace`. `lc-tiff` is the only complete TIFF directory
library (BigTIFF, SubIFDs, makernotes, writer) and the only one that can *write* back metadata
(`lc-meta::preserve_tiff_metadata`); `pc-io/flat.rs` patches EXIF bytes by hand instead.
`pc-psd` makes `lc-codecs/src/psd.rs` redundant (composite = `pc-psd::ImageData::decode`).

**User-visible today.** The Library cannot import iPhone/Mac HEIC photos at all, while the editor
opens them (File › Open); the editor cannot open JXL that the Library shows. A PSD with a merged
composite opens in both, but malformed PSDs are hardened in `pc-psd` (fuzzed, limits) and only
partly in `lc-codecs/psd.rs`. "Open in Compositing" from the Library re-decodes the file with the
other stack, so the same JPEG can differ by rounding and by which ICC path was applied.

**Winner / merge.** `pc-codecs` is the single decoder set; `lc-codecs::decode` becomes an
adapter: `pc-codecs::decode_with(limits)` → `space::SourceSpace::from(icc, container tags)` →
`to_working`. Keep from lc and move into `pc-codecs`: the DCT-scaled JPEG path (`jpeg-decoder`),
embedded-preview extraction, the JXL decoder (feature), and the raw sniff tags (so one `Format`
enum knows raws). `lc-tiff` becomes the TIFF directory library for both (`pc-codecs::tiff_ifd`,
`pc-io/flat.rs` byte patching and `pc-codecs::orientation::upright_exif` re-implemented on
`lc-tiff::writer` / `lc-meta::preserve_tiff_metadata`). Delete `lc-codecs/src/psd.rs`
(use `pc-psd`), `lc-codecs/src/exif.rs` (use `lc-tiff`), and the fourth photo-extension list
(`lc-mcp/src/headless.rs:14`, `lc-engine/src/import.rs`, `pc-ui-egui/src/dialog_filter.rs`,
`filmstrip_ui.rs`, `overwrite_original.rs`, `generate_ui.rs`, `pc-engine/src/file_cmds.rs`,
`apps/local-image/src/{library_host,services}.rs` all carry their own `"nef"` lists) in favour
of `Format::extensions()`.

**Slices** (golden: decode hashes of `crates/lc-codecs/tests/it/corpus.rs` and `pc-codecs`
goldens must not change): (1) one `Format` + sniff (S); (2) `lc-codecs::decode` over
`pc-codecs` for JPEG/PNG/TIFF/WebP/GIF/BMP with SourceSpace adapter; HEIC appears in the Library
for free (M, medium risk: colour interpretation parity); (3) move DCT-scaled JPEG + thumbnails
+ JXL into `pc-codecs` (S–M); (4) `lc-tiff` under `pc-codecs::tiff_ifd` and `pc-io::flat`
(M, medium risk: PSD/TIFF export byte goldens in `pc-io` tests); (5) delete `lc-codecs::psd`
(S); (6) one extension list (S). Overlap: raw-quality job owns `lc-raw`, not `lc-codecs`; the
library-safety job may touch `lc-engine/src/import.rs` (slice 6 waits for it).

#### D9 Metadata (EXIF / XMP / IPTC) reading and writing

| Copy | Does | Evidence | Size |
|---|---|---|---|
| `lc-meta` | full EXIF read **and write**, XMP parse/write via `quick-xml` with namespaces and merge rules (`xmp_merge.rs`), IPTC-IIM, GPX, CR3 container, TIFF metadata preservation, embedded-block locator for JPEG/PNG/WebP | `crates/lc-meta/src/lib.rs:1-30`, `exif.rs:13-260`, `xmp.rs:457-577`, `xmp_merge.rs`, `tiff_replace.rs:9` | 4,573 |
| `lc-codecs::exif` | minimal TIFF walk for orientation/thumbnail/colour hint | `crates/lc-codecs/src/exif.rs:1-140` | 166 |
| `pc-algo::exif` | minimal EXIF reader for camera/lens/exposure + a **writer** (`build`) | `crates/pc-algo/src/exif.rs:8-134` | 240 |
| `pc-codecs::orientation` | `exif_orientation`, `upright_exif` (rewrites tag 274 in place), `upright_xmp` (string edit) | `crates/pc-codecs/src/orientation.rs:169-220` | 371 |
| `pc-io::flat` | `drop_exif_thumbnail`, `update_exif_dimensions` by byte patching | `crates/pc-io/src/flat.rs:371-430` | ≈70 |
| `pc-engine::file_cmds` | File › File Info: XMP read/write by **string search** (`find_element`, `find_attr`) | `crates/pc-engine/src/file_cmds.rs:553-640` | ≈120 |
| `pc-engine::web_cmds::web_xmp` | copyright-only XMP for Save for Web built on the above | `web_cmds.rs:336-356` | 20 |
| `pc-psd::metadata`, `resources.rs:203 xmp_str` | PSD resource 1060 XMP as string | | |

**Differences.** `lc-meta` is a real metadata library (schema-aware XMP with arrays, lang-alts,
namespaces, merge-with-sidecar rules; EXIF write with maker-note preservation). The editor's
copies are byte patchers and string scanners: `read_file_info` finds `<dc:creator>` by text
search and `write_file_info` splices text, which breaks on prefixed/attribute forms, CDATA or
`rdf:Alt` ordering the Library writes.

**User-visible today.** Edit File Info in Compositing, save, import into the Library: fields
can be lost or duplicated (Library parses properly, editor wrote by splice; and vice versa).
The editor's "Camera" readout (Lens Correction, Wide Angle) comes from `pc-algo::exif`, which
reads fewer tags than the Library's Metadata panel for the same file. Orientation uprighting on
export rewrites EXIF in two different ways (`pc-codecs::upright_exif` vs `lc-meta::write_exif`),
so "Keep orientation" policies differ (today's shared-core report already had to add a
`keep_orientation` policy at the encoder boundary to reconcile them).

**Winner / merge.** `lc-meta` (+ `lc-tiff`) is the one metadata library; rename to `li-meta`
later or keep. `pc-algo::exif::read` → `lc_meta::read_exif` projected to `CameraInfo`;
`pc-algo::exif::build` → `lc_meta::write_exif`; `pc-engine::file_cmds` File Info →
`lc_meta::parse_xmp`/`write_xmp` (+ `xmp_merge` rules); `pc-codecs::orientation::upright_{exif,xmp}`
and `pc-io::flat` patches → `lc-meta` + `lc-tiff` writer. Dependency direction is fine (pure
Rust; `lc-meta` depends only on `lc-tiff`, `lc-geom`, `quick-xml`). Effort M–L. Slices: (1)
`pc-algo::exif` adapter (S); (2) File Info on `lc-meta` with round-trip tests
Library↔editor (M); (3) orientation + dimension patching on `lc-tiff` (M, PSD/TIFF goldens).
Overlap: none of the running jobs.

#### D10 Atomic / durable file writes: six implementations with different guarantees

| Copy | fsync | dir fsync | Windows rename retry | symlink-aware | mode kept | never-replace mode | test seam | Evidence |
|---|---|---|---|---|---|---|---|---|
| `lc-catalog::safe_file` | policy: `write_atomic` (yes) / `write_atomic_nosync` (no) | yes | no | no | no | `write_new`, `write_new_unique` (hard link) + `same_file` | no | `crates/lc-catalog/src/safe_file.rs:1-200` |
| `pc-format::atomic` | always | best effort | **yes** (AV scanners) | **yes** | **yes** | `StagedWrite` | `AtomicFs` injection, `Interrupt` | `crates/pc-format/src/atomic.rs:1-200` |
| `li-ai::settings::write_atomic` | **no** | no | no | no | no | — | — | `crates/li-ai/src/settings.rs:162-172` |
| `lc-preview::disk` | no (by design: cache) | — | — | — | — | — | — | `disk.rs:99-101` |
| `lc-engine/src/{smart,rename,import_move,camera_profiles,cmd/browse,smart_sort/faces_store}.rs`, `apps/.../{library_host,desktop_install,app_log}.rs`, `pc-ui-egui/src/generate_ui.rs`, `pc-automation/src/workspace.rs`, `li-seg/src/{denoise,restoration,lib}.rs`, `li-ai/src/{trash,library,enhance/selection}.rs` | own `.tmp` + `rename` | varies | no | no | no | — | — | grep counts in §3 search |
| wrappers | `lc-engine/src/sidecar.rs:326`, `apps/.../services.rs:94`, `pc-format/src/store.rs:739`, `lc-catalog::store` trait | delegate | | | | | | |

**User-visible today.** AI settings and the model inventory (`li-ai`) are written without
fsync: after a power cut the file can be empty on some file systems (the Library and editor
documents are protected). On Windows, Library writes have no antivirus rename retry (the editor's
do). A catalog export that must never overwrite an original is protected by `write_new`
only in the Library; the editor relies on `OriginalGuard` logic instead.

**Winner / merge.** One **`li-fs`** (or promote `pc-format::atomic` and give it
`lc-catalog::safe_file`'s policy + `write_new`): `Policy::{Durable, Atomic}`, Windows retry,
symlink target resolution, mode copy, `write_new`/`write_new_unique`, `same_file`,
`StagedWrite`, `Interrupt`, injectable fs. All ≈20 private `.tmp`+`rename` sites go through it.
Effort M (mechanical; each caller a one-line change; tests exist in both crates). Overlap: the
**library-safety job** (`wip/cx-library-safety`) is editing `lc-catalog`/`lc-engine` write paths
right now: do the merge after it lands, and tell it to build on `safe_file` only.

#### D11 Thumbnail / preview caches

`lc-preview` (`PreviewCache` memory LRU + `DiskCache` JPEG files keyed by content/settings hash +
`JobPool` with per-slot de-duplication; `crates/lc-preview/src/lib.rs:1-40`) serves the Library
grid, loupe, filmstrip, CLI and MCP. The editor has its own: `pc-ui-egui/src/filmstrip_ui.rs:55-125`
(a worker thread, `lightcraft_codecs::decode_thumbnail_with`, a private `orient()` over
`image::RgbaImage`, no disk cache), `pc-compose::thumbnail` (document composites for Levels,
recents), `li-ai::browser::Cache` (catalogue bodies by SHA-256), `pc-ui-egui/src/brush_preview.rs
SigCache`, `canvas.rs CanvasCache`. **User-visible:** the editor's filmstrip re-decodes every
thumbnail each session and shows the embedded/DCT thumbnail even for photos the Library has
developed (the grid shows the developed render) — the same folder looks different in the two
strips. **Winner:** `lc-preview` (`li-preview`) is the one thumbnail service; the filmstrip asks
the Library's `MediaCache` through the host. Effort S–M; owner: LRC-UX job may touch the Library
filmstrip, not the editor's `filmstrip_ui.rs`.

#### D12 Content hashing

`lc-preview::hash` (SipHash-128 `Hash128`, `hash.rs:59`; catalog duplicate detection, cache
keys, `lc-engine/src/media.rs:770 content_key`), `blake3` (editor: `pc-engine`, `pc-format`,
`pc-io` linked sources and pcraft dedupe), `sha2` (downloads, `li-ai::browser::Cache` keys),
`crc32fast` (journal records). **Effect:** a photo's identity is computed two different ways in
one app (a linked smart object's source hash and the catalog's content key never match), and
SipHash is not collision-resistant for a duplicate finder (G11 in the holistic review).
**Winner:** blake3 for content identity everywhere (already in tree, fast, cryptographic); keep
sha256 only for published download hashes, CRC-32 for the journal. Effort S; catalog keys are
persisted, so slice as "new key alongside old, migrate on next scan". Wait for the
library-safety job.

### 3.3 GPU, jobs and threading

#### D13 Two wgpu devices, two backend-choice policies, two crash sentinels, two health models

| | Develop (`lc-gpu`) | Editor (`pc-gpu` + shell) |
|---|---|---|
| Device | **its own** `Instance` + `Adapter` + `Device` (`crates/lc-gpu/src/ctx.rs:531-575`): no display handle, `HighPerformance`, software adapters refused, limits = adapter limits (optionally capped by `LIGHTCRAFT_GPU_LIMITS`) | the window's eframe `RenderState` device (`pc-ui-egui/src/gpu_canvas.rs:114-137`, `apps/.../main.rs:431`) |
| Backend choice | `lc-gpu::backend` (`backend.rs:1-125`): `LIGHTCRAFT_GPU_BACKEND` / `WGPU_BACKEND`, DX12-only on Windows | `apps/local-image/src/gpu_startup.rs:1-40`: Vulkan → DX12 → CPU chain, `performance.gpuBackend` preference, `--safe-gpu`, `WGPU_BACKEND` |
| Crash sentinel | init marker file around device creation (`backend.rs:129-160`, issue #136) | `gpu-starting.json` + OS file lock + `gpu-recovery.json` (`gpu_startup.rs`, #4) |
| Health | `BROKEN` static: first uncaptured error or device loss turns the GPU off **for the whole process** (`lib.rs:50-60`, `ctx.rs:562-571`) | `pc-gpu::health::DeviceHealth` (`health.rs:1-50`): strike/decay counting, per-operation CPU redo, device loss final |
| Shader helpers | `lc-gpu/src/wgsl/*.wgsl` (sRGB, TPDF dither `finish.rs:130 dither8`, gaussian) | `pc-gpu/src/compose.wgsl:867 dither_noise`, inline WGSL in `gpu_canvas.rs:2542-2570` (sRGB, box blur) |
| Test preludes | `lc-gpu` tests | six copies of "request an RTX adapter" in `pc-engine/src/vector_packages/{appearance,expand,colours,layout,live_paint,width}/gpu.rs` and `affinity/tests.rs` (ignored tests) |

**Differences.** Two devices on one card means two command queues, two memory pools, no shared
VRAM budget, and texture data that cannot be shared (the Develop render is read back to the CPU
and re-uploaded by the editor canvas for a Develop layer). `pc-gpu`'s health model is clearly
better (a transient OOM is retried on the CPU for that operation; `lc-gpu` goes CPU-only until
restart, which is P1/QW10 in the holistic review). `lc-gpu`'s refusal of software adapters and
its storage-buffer-count check are the right compute checks; `gpu_startup`'s chain is the right
window policy. The two crash sentinels guard the same event (a driver crash while creating a
device) with two files.

**User-visible today.** An OOM during an AI job or a big composite while a Develop slider is
dragged turns Develop CPU-only for the session; the editor keeps working on the GPU. Two
different env variables and two settings decide "which GPU": a user who sets
Preferences › Performance › GPU backend changes only the window/compositor; Develop still
picks on its own. Dither noise differs between the Develop canvas (TPDF) and the editor canvas
(gradient hash), visible as different grain in smooth gradients when switching modules.

**Winner / merge.** **`li-gpu`** device service: one `Instance` (backends from `gpu_startup`'s
chain, merged with `lc-gpu::backend` env parsing), one adapter, one `Device`/`Queue` created by
the shell and handed to eframe (window), `pc-gpu` (compositor), `lc-gpu` (compute) and the AI
denoise kernel; required limits = union of both stacks' needs (`lc-gpu` needs ≥ 10 storage
buffers per stage and large storage bindings); one `DeviceHealth` (the `pc-gpu` model) that
both consult; one crash sentinel; a shared WGSL snippet file (`li-gpu/wgsl/common.wgsl`: sRGB
transfer, TPDF dither, gaussian) string-included by both shader sets with CPU twins in
`li-color`. Headless/CLI/tests get the same service without a window. Effort L.
**RUNNING: GPU-coordination job** (`wip/cx-gpu`) is in exactly these files
(`lc-gpu/src/lib.rs`, `pc-gpu/src/health.rs`, `apps/.../gpu_startup.rs`, ComfyUI VRAM probe):
D13 must wait for it and should be specified as its follow-up, not in parallel. Slices after
it: (1) `li-gpu` crate with device + health (M); (2) `lc-gpu` takes an external device
(M; parity tests `lc-gpu/tests` unchanged); (3) shared WGSL snippets + dither parity (S);
(4) delete `lc-gpu::backend` sentinel (S); (5) one test prelude (S).

#### D14 Jobs, progress and cancellation: eight primitives, two runners, three progress UIs

| Primitive | Shape | Evidence |
|---|---|---|
| `pc-raster::Interrupt` | borrowed `&dyn Fn() -> bool` + `&dyn Fn(f32)`; `NONE` | `crates/pc-raster/src/interrupt.rs:12-70` |
| `pc-engine::jobs::{Jobs, JobCtx, JobId}` | worker thread on a document snapshot; result applied on the UI thread as one undo step; disables conflicting commands | `crates/pc-engine/src/jobs.rs:1-30,218` |
| `lc-engine::enhance::{Job, JobCtl}` | `AtomicU32` progress + message + cancel | `crates/lc-engine/src/enhance/mod.rs:78-95,179-200` |
| `lc-preview::JobPool` | slot de-dup + priorities (thumbnails/loupe) | `crates/lc-preview/src/pool.rs:1-25` |
| `lc-ui-egui::tasks::Tasks` | `Finish` closures applied on the UI thread | `crates/lc-ui-egui/src/tasks.rs:1-30` |
| `lc-engine::cmd::cull::Control`, `lc-engine/segment/worker.rs Job/Worker`, `camera_profiles::Pool` | per-feature | `cull.rs:24`, `segment/worker.rs:45,108`, `camera_profiles.rs:191` |
| `li-ai::comfy::JobControl` | `Arc<AtomicBool>` + `Mutex<Progress{stage, step, message}>` | `crates/li-ai/src/comfy.rs:78-130` |
| `li-seg::Cancelled` / `AtomicBool` args; `lc-fetch::Progress` + cancel flag | per-crate | `li-seg/src/lib.rs:488`, `denoise.rs:34`; `lc-fetch/src/lib.rs:96` |
| Progress UI | `pc-ui-egui/src/jobs_ui.rs` (563: status-bar readout + Cancel, modal after delay, background opens) vs `lc-ui-egui/src/tasks.rs:101 progress`, `import.rs:812 progress`, `panels/enhance.rs:26 progress`, `export_task.rs`, `smart_sort_task.rs`, `people_task.rs` (each its own bar) | |

Shared already (good): `lc-sysmem` thread classes, permits and the one rayon pool.

**User-visible today.** Cancel means different things: editor jobs cancel per tile and
roll back cleanly; Library tasks either cannot be cancelled (`Tasks` has no cancel) or cancel
through a per-feature flag; AI jobs (`JobControl`) show stages; export shows a bar with no
stage. The status bar in Compositing and the task strip in the Library are two different
progress designs for the same person.

**Winner / merge.** `li-jobs` (could live in `lc-sysmem`, which both halves already depend on):
one `JobCtl { cancel, progress: f32, message, stage }` that (a) hands out a `pc-raster::Interrupt`
view for algorithms (keep `Interrupt` as the borrowed API: it is the right shape for hot loops),
(b) replaces `JobControl`, `lc-engine JobCtl`, `cull::Control`, `lc-fetch` flags; one runner
with the `pc-engine::jobs` model (snapshot → worker → apply on UI thread), hosted by both
`Session`s through an apply hook; `lc-preview::JobPool` stays as the render-slot scheduler
(different job: de-dup by slot). One progress/cancel widget in `li-ui-kit` fed by `JobCtl`.
Effort M–L. Slices: (1) `JobCtl` + adapters (S–M), (2) `li-ai`/`li-seg`/`lc-fetch` on it (S),
(3) Library tasks on the runner with cancel (M; LRC-UX job owns the Library's task UI files —
wait), (4) one widget (S).

### 3.4 AI, models and the heavy algorithms

#### D15 Model download, registry and model folders: three downloaders, five spec types, three model roots

| Copy | Evidence | Notes |
|---|---|---|
| `li-ai::download` (ureq 3 + rustls-rustcrypto, host allow-list checked on every redirect, cancel-aware transport, `.part` + size + SHA-256, hard-link into place) | `crates/li-ai/src/download.rs:1-45`, `download/transport.rs` | the mature one; used by Model Browser, Generate, Enhance, li-seg models via `pc-ui-egui/src/ai_ui.rs:371 download_seg_files` |
| `lc-fetch` (**own HTTP/1.1 client** over rustls: `http.rs` 553 lines; mirrors, resume, stall timeouts) | `crates/lc-fetch/src/lib.rs:1-40`, `http.rs:1-30` | used **only** by `lc-segment::fetch` (SAM 3 weights) — SAM 3 is off by owner decision |
| `lc-engine/src/segment/download.rs` | | SAM 3 download job over `lc-fetch` |
| Spec types: `li-seg::ModelSpec` (`lib.rs:42`), `li-ai::catalog::{Preset, FileSpec}` (`catalog.rs:169`), `li-ai::family::FileSpec` (`family.rs:329`), `lc-fetch::FileSpec` (`lib.rs:36`), `li-ai::restoration::byo::Model`, `li-ai::cloud::MODELS` | | five shapes for "a file, its size, its hash, its URL" |
| Model roots: `li-ai::settings::AiSettings::model_dir()` (`settings.rs:93`), `pc-engine::seg::models_dir()` (`seg.rs:16`), `lc-engine::segment::model_dir()` (`segment/mod.rs:229`); config roots ×3: `apps/.../app_dirs.rs:59`, `apps/.../services.rs:73`, `lc-engine/src/camera_profiles.rs:65` | | |

**User-visible today.** Models are managed in three places (Model Browser; Preferences › Local AI;
Develop's AI-mask model download), with three "where are my models" answers; the allow-list
and verification policy differ (lc-fetch: mirrors file + optional hash; li-ai: allow-list +
mandatory hash). The LaMa job will add another model: without a single registry it becomes
the fourth place.

**Winner / merge.** `li-ai::download` + `li-ai::inventory`/`catalog` are the registry and the
downloader. Fold `li-seg::MODELS` entries into the catalog (they already flow through
`download_seg_files`), one `FileSpec`, one models root in the merged preferences (D22).
Delete `lc-fetch`'s HTTP client and `lc-engine/src/segment/download.rs` (SAM 3 off); if SAM 3
ever returns it downloads through `li-ai::download` (port `lc-fetch`'s resume + mirrors as a
feature of that one client, which has neither today). Effort S–M. **RUNNING: on-device LaMa
job** (`wip/cx-lama`) adds a model + runner in `li-seg`/`lc-engine/enhance`: ask it to register
through `li-ai` catalog + `download_seg_files` and not to add a model dir.

#### D16 Segmentation and AI runners: three stacks, one dormant with a 1.9k-line candle port

| Stack | Runtime | Models | Users | Evidence |
|---|---|---|---|---|
| `li-seg` | tract (CPU) | RMBG-1.4, InSPyReNet, PP-MobileSeg sky, Depth Anything V2, YuNet/SFace faces, CLIP, RawNIND denoise, DDColor/old-photo restoration, upscalers | **both halves** (`lc-engine` ×10 files, `pc-ui-egui` ×7, `lc-gpu` ×2, `li-ai` ×4) | `crates/li-seg/src/lib.rs:1-60` |
| `lc-segment` | candle (+Metal on macOS) | SAM 3 (ViT 1008², text + click prompts) | `lc-engine` behind feature `sam`; **the app enables it** (`apps/local-image/Cargo.toml:12`) although SAM 3 is **off** by owner decision | `crates/lc-segment/src/lib.rs:1-40`, `nn.rs`, `tokenizer.rs` |
| `pc-algo::segment` | classical (GrabCut, GMM, max-flow, SLIC, saliency, focus) | Quick Selection, Object Selection click mode fallback, Select Subject fallback, Focus Area | editor only | `crates/pc-algo/src/segment/mod.rs:1-30` |
| CLIP BPE tokenizer | `lc-segment/src/tokenizer.rs` (177) and `li-seg/src/bpe.rs` (227, "generalised here to context 77") | | the same port twice | `PORTS.md` row for `bpe.rs` names `lc-segment/src/tokenizer.rs` as its source |
| Edge refinement | Develop AI masks: guided-filter refine (`lc-pipeline/src/masks.rs:457-467`, no user control: Q6) | editor: closed-form matting `pc-algo/src/cf_matting.rs` (481), `matting.rs` (746), Refine Edge | two algorithms for "refine this mask's edge" |

**Differences.** `li-seg` is the shared, measured (bake-off) stack: good. `lc-segment` costs
compile time and binary size for every build (candle-core/nn, Metal feature) for a feature that
is switched off; its tokenizer duplicates `li-seg::bpe`. Edge refinement: closed-form matting
(editor) is the higher-quality hair/edge solution; Develop has only a guided filter.

**User-visible today.** Select Subject in Compositing (li-seg RMBG/InSPyReNet + closed-form
refine) and the Subject mask in Develop (same model, guided refine) give different edges on
hair; Develop masks have no Feather/Edge controls (Q6/G12). The dormant SAM path still ships
menu items / download UI that the owner turned off.

**Winner / merge.** Keep `li-seg` as the one on-device runner (add LaMa there — the running job
does). Make `lc-segment` **not default**: drop `sam` from the app's default features (keep the
crate in tree), delete `lc-segment/src/tokenizer.rs` in favour of `li-seg::bpe`. Share edge
refinement: move `cf_matting` + guided refine into `li-matte` (a module of `li-seg` or a small
crate) with the Feather/Edge parameters, used by `lc-pipeline::masks` (through the `SegMask`
logits, resolution-independent) and the editor's Refine Edge. Effort: defeature SAM S; tokenizer
S; `li-matte` M–L (CPU cost on big previews must be bounded: refine at ≤ 1024 px like the
models, as `quickseg.rs:15 INPUT_EDGE` does). Overlap: LaMa job (`li-seg`, `lc-engine/enhance`);
do the `li-matte` slice after it.

#### D17 A second Camera Raw develop engine survives in the editor (`pc-algo::camera_raw`, 1,087 lines)

Filter › Camera Raw Filter… is wired to `filter.develop` (`pc-ui-egui/src/menu_catalog.rs:554`),
which runs the Library's `lightcraft_pipeline::render` on the layer (`pc-io/src/develop_filter.rs:1-25`;
`pc-engine/src/develop_filter_cmds.rs`). Good: one develop engine. But `photocraft_algo::camera_raw::develop`
(`crates/pc-algo/src/camera_raw.rs:530`, own WB/exposure, dark-channel dehaze, guided-filter
tone, Fritsch–Carlson curves, HSL mixer, colour grading, USM, vignette, grain) is still live in
two places: the Camera Raw **dialog preview** (`pc-ui-egui/src/camera_raw_ui.rs:4,146`:
"The preview runs `photocraft_algo::camera_raw::develop` on a CPU proxy"), which
`develop_filter_ui.rs:229` opens via `camera_raw_ui::open_develop` after mapping
`DevelopSettings → CameraRaw` (`develop_filter.rs:353 to_camera_raw`), and
`pc-engine/src/lens_cmds.rs:160-176 camera_raw_surface` (Lens Correction's Camera Raw stage).
The `CameraRaw` struct itself is also the PSD interchange model (`pc-io/src/camera_raw_map.rs`,
`smart_map.rs`), which must stay.

**User-visible today.** The Camera Raw dialog previews with one engine (pc-algo: sRGB-curve
working encoding, its own dehaze and NR) and commits with another (lc pipeline: linear
Rec.2020, darktable haze/NR): the preview is not what you get — on Dehaze, Clarity/Texture,
Noise Reduction and Grain the difference is plainly visible, and settings outside the
`CameraRaw` subset (masks, point colour, Negative) cannot be previewed at all.

**Winner / merge.** Preview with the same engine as the commit: the dialog proxy runs
`lightcraft_pipeline::render` at ≤ 900 px (it already renders Develop drafts at 1280 px in
≈18 ms p95 on the GPU), through the existing `develop_filter` bridge; `camera_raw::develop`,
`curve_lut`, `band_weights` and the dehaze/tone/detail stages are deleted (≈800 lines), and
`CameraRaw` shrinks to the PSD descriptor model (`validate_curve`, `Wheel`, fields). Lens
Correction's Camera Raw stage calls the same bridge. Effort M; golden: PSD round-trip tests in
`pc-io` unchanged; a new test asserts dialog preview == committed pixels on a fixture within the
proxy's resampling tolerance. Overlap: none running (Match Look is Library-side).

#### D18 Healing and content-aware fill: two Heal algorithms

Develop spots (`crates/lc-pipeline/src/spots.rs:1-15`: Clone = feathered copy; Heal = `src +
blur(target − src)` frequency-separation approximation; Remove = Heal with ring-searched
source) vs the editor's Spot Healing / Content-Aware Fill (`pc-algo/src/inpaint.rs:1-15`:
multi-scale PatchMatch completion + `poisson` seamless blend; `content_aware.rs`), plus
`pc-paint::retouch` dab machinery and `pc-algo::retouch` dodge/burn kernels. The Library's
content-aware **Heal through AI Remove** already uses the editor's PatchMatch + Poisson
(`lc-engine/src/enhance/remove.rs:9-12,315-316`) and stores the result as a patch.

**User-visible today.** Two different "Heal" results in Develop itself: a spot drawn with the
Heal tool (freq-sep, re-rendered every time) vs a stroke in AI Remove with the local engine
(PatchMatch + Poisson, patch store). The editor's Healing Brush matches the second, not the
first.

**Winner / merge.** PatchMatch + Poisson everywhere; Develop's Heal spot becomes the LOCAL
patch engine (already built), `spots.rs` keeps Clone only (trivial and correctly re-rendered
per settings), Remove-with-ring-search becomes "pick the source automatically, then
PatchMatch". Effort M; golden: existing `lc-engine/src/tests_spots.rs` fixtures re-baselined
with a documented quality note. **RUNNING: LaMa job** edits `lc-engine/src/enhance/*` — wait.

#### D19 Noise reduction, sharpening, dehaze (mostly intentional, one residue)

Develop: darktable-faithful wavelet NR, capture sharpening (RL), log-luminance USM, haze
removal (`lc-pipeline/src/detail.rs`, `detail/{nr,haze}.rs`), AI denoise (`li-seg::denoise`,
GPU twin in `lc-gpu/src/denoise.rs`). Editor: Photoshop-parity Reduce Noise (guided filter,
`pc-algo/src/denoise.rs`), Unsharp Mask / Smart Sharpen / High Pass (`sharpen.rs`). These are
different tools with different semantics (Photoshop filter parity vs raw development) and the
editor already reaches the Develop versions through Camera Raw Filter; **keep** (§5). The only
residue is D17 (pc-algo's own dehaze/NR/USM inside `camera_raw.rs`), removed there.

#### D20 HDR merge and panorama: two complete stacks (≈3.4k vs ≈3.7k lines)

`lc-merge` (`crates/lc-merge/src/lib.rs:1-20`): MOPS features with ANMS (`features.rs`), RANSAC,
rotation bundle adjustment, spherical/cylindrical/perspective projection, gain compensation,
multi-band blending in log space, HDR from scene-linear raw frames with noise-optimal weights
and deghosting, output as linear DNG (`hdr.rs:1-15`, `pano/mod.rs:1-15`). `pc-algo`
(`hdr.rs:1-15`, `panorama.rs:1-20`, `features.rs:1-12`): Ward MTB alignment, Debevec response
recovery, hat-weighted merge, ghost mask, tone-mapping methods; Harris + steered BRIEF,
Brown–Lowe verification, Shum–Szeliski focal, Levenberg–Marquardt bundle with radial
distortion; Photoshop layouts (Perspective/Cylindrical/Spherical/Collage/Reposition) and
Auto-Align Layers semantics. Callers: `lc-engine/src/merge.rs` (Library › Photo Merge) and
`pc-engine/src/{align_cmds,photo_cmds}.rs` (Photomerge, Merge to HDR Pro, Auto-Align/Blend).

**Differences.** For raw brackets `lc-merge` is superior (no response-curve estimation needed;
scene-linear weighting; DNG output that keeps the raw workflow). For JPEG/8-bit sources and
for *layer* alignment the editor's is the one with the right semantics (Debevec; per-layer
transforms returned, not a flat image). Feature detectors differ (MOPS vs Harris+BRIEF); which
matches better on real interiors is not measured anywhere in the tree.

**User-visible today.** The same five brackets merged in the Library and in Compositing align
differently and blend differently; "flambient" interior work (G8) would need the editor's
layer alignment but the Library's radiometric merge.

**Winner / merge.** `li-merge` core: one feature detector + matcher + RANSAC + bundle adjuster
(decide by a bake-off on a fixed CC0 interior/exterior set: match inlier counts and residuals),
one multi-band blender, one HDR merger with two input models (linear raw frames: lc weights;
non-linear 8-bit: Debevec), returning both a flat result and per-input transforms so
Auto-Align Layers and Photo Merge are thin callers. Effort L–XL; value medium (architecture
work benefits; not the owner's daily headshot path). Slice 0 is the bake-off (S). No running
job overlaps.

#### D21 Lens correction, perspective and red-eye (two of three intentional)

- **Lens:** Develop has the lensfun database + embedded profiles + manual terms + CA + vignetting
  in one inverse warp (`lc-pipeline/src/optics.rs:1-12`, `lensdb.rs`, `lc-engine/src/lens_db.rs`);
  the editor has Photoshop's manual Lens Correction (`pc-algo/src/lens.rs`, Brown–Conrady) with
  no database and camera info from `pc-algo::exif` (D9). `develop_filter::sanitize` switches
  lens profiles off for layers (`develop_filter.rs:15-18`) because a layer's geometry is unknown.
  **Keep both models**, but let the bridge keep the lens profile on when the layer is an unscaled
  full-frame photo (document opened from a camera file with intact EXIF) — then the editor gets
  the database for free. Effort S–M after D9. **RUNNING: lens-correction/raw-quality jobs**
  (`wip/cx-lenscorr`) own `lc-pipeline/src/optics.rs`/`lensdb.rs` — wait.
- **Perspective:** Develop Upright (automatic, `lc-pipeline/src/upright.rs`) vs editor
  Perspective Warp / Vanishing Point (manual planes): different tools; keep (§5). Exposing
  Upright's auto-level/vertical through the Camera Raw Filter bridge is a feature, not a merge.
- **Red-eye:** `lc-pipeline/src/redeye.rs:1-15` (ellipse → red-dominant centroid, pet eye, GPU
  kernel) vs `pc-engine/src/redeye_cmds.rs:1-8` (flood region from a click, GEGL style). Two
  algorithms, one job. Winner: lc's (pet-eye, resolution independence, scene-linear); the
  editor's `paint.redEye` calls it through the `develop_filter` pixel bridge. Effort S. Low
  priority.

### 3.5 Application layer: preferences, i18n, UI kit, keys, menus, automation

#### D22 Preferences, settings files and directories (RUNNING: LRC-UX job merges the two windows)

| Store | File | Load/recovery | Write | Evidence |
|---|---|---|---|---|
| `pc-engine::prefs::{Preferences, PrefsStore}` | `<config>/preferences.json` | lenient parse, `preference-backups/`, `preferences.recovery.json` record, UI notice | `pc-format::atomic_write` via `apps/.../services.rs:94` | `crates/pc-engine/src/prefs.rs:957,1732`; `apps/local-image/src/services.rs:78-120` |
| `lc-ui-egui::state::{AppSettings, UiState}` | `<config>/library-ui.json` | own corrupt-file rename `library-ui.json.corrupt-<stamp>` | `lc-catalog::safe_file::write_atomic` on a timer | `crates/lc-ui-egui/src/state.rs:166,246`; `apps/.../library_host.rs:1395-1470` |
| `li-ai::settings::AiSettings` | its own JSON under `data_root()` | — | own non-fsync `write_atomic` (D10) | `crates/li-ai/src/settings.rs:36,93,162` |
| `lc-ui-egui::import::ImportPreferences` | inside library-ui | | | `import.rs:64` |
| Shortcut profiles | `pc-engine::prefs::ShortcutProfiles` (editor file), read by the Library through the host | | | `prefs.rs:853-859`; `library_host.rs` (×4) |
| Config dirs ×3 | `apps/.../app_dirs.rs:59`, `apps/.../services.rs:73`, `lc-engine/src/camera_profiles.rs:65` | | | |
| Model dirs ×3 | see D15 | | | |

**User-visible today** (U3 in the holistic review): two dialogs, two files, GPU/performance in
both, language in both (`interface.language` in the editor prefs; `app.ui.language` in the
Library's UiState, synced one way at `library_host.rs:1527`). **Winner:** the LRC-UX job's
single Preferences window must also mean a **single store**: `PrefsStore` (lenient load, backups,
recovery notice — the better of the two) with namespaced sections (`library.*`, `ai.*`), one
`config_dir()`, one models root; `library-ui.json` migrated leniently on first start (keep
reading it for one release). `AiSettings` moves in (and gets fsync for free). Effort M; owned by
the running job — this audit only asks that its design cover all three stores, not two.

#### D23 i18n: two catalog formats, two lookup APIs, 12 languages translated twice

| | Library (`lc-ui-egui::i18n`) | Editor (`pc-ui-egui::i18n`) |
|---|---|---|
| Catalog | JSON, ≈1,590 lines × 12 + `*-formats.json` (143 × 12) compiled by `build.rs` into a `tr_format!` macro (compile-checked placeholders) | TSV, ≈3,900 lines × 12; `@plural` entries; contexts; command-id keys |
| API | `i18n::tr(&str)` (778 uses), `tr_format!`, `builtin_label` | `tl!` (2,976 uses), `tr`, `tr_ctx`, `tr_id`, `trn`, `fmt` |
| State | thread-local `Cell<Locale>` | `OnceLock` catalogs; `Lang` in egui data; `system_lang()` detection (`system.rs`) |
| Plurals | none (formats only) | per-language rules (`plural_russian`, Czech few) |
| Evidence | `crates/lc-ui-egui/src/i18n.rs:1-200`, `build.rs:1-20` | `crates/pc-ui-egui/src/i18n/mod.rs:1-60,260-300` |

**Differences.** The editor's model is richer (contexts, ids, plurals, system detection, a
documented "adding a language" path with tests for completeness); the Library's `tr_format!`
is the one feature the editor lacks (compile-time placeholder checking), and its
`builtin_label` rule (never translate user names) is good policy.

**User-visible today.** Strings both halves show ("Export", "Preferences", "Cancel", file-type
names, error prefixes) are translated twice, by two translators, and can differ within one
window (the Library host draws editor widgets inside Library panels). Adding a language is two
jobs. Plural forms are wrong in Library messages for Russian/Czech ("3 photos" style through
format strings).

**Winner / merge.** **`li-i18n`** crate = the editor's catalog engine (TSV, contexts, ids,
plurals, system detection) + a `tr_format!`-style compile-checked macro ported from the
Library's `build.rs`; one TSV per language holding both halves' strings (a script converts the
JSON catalogs; conflicts resolved by context keys). `tl!` and `i18n::tr` both forward to it.
Effort L for the mechanical merge of ≈4,500 keys × 12 languages, S for the engine. Priority:
**last** (owner: translations are lowest priority; a parked i18n review branch exists). Parity:
a test that every old key resolves to the same string through the new engine. Overlap: every UI
job adds strings; sequence after LRC-UX lands and before the next translation pass.

#### D24 UI kit: widgets, tokens, icons, fonts

- **Widgets.** Six documented pairs (`DESIGN-SYSTEM.md` §3: icon button, colour tile, dropdown,
  segmented/pill tab, separator, section heading) plus undocumented pairs: slider
  (`lc widgets.rs:147,162` vs `pc widgets.rs:251,312`), text/primary buttons, dialogs (`dialog_buttons`
  only in pc), disabled-reason tooltips (`pc widgets.rs:1124 disabled_reason_tooltip`; the
  Library has the reason strings but its own tooltip code per panel), automation-id registry
  (`lc widgets.rs:17 register/take_registry` vs the editor's accessible-name route). Sizes:
  `lc-ui-egui/src/widgets.rs` 853, `pc-ui-egui/src/widgets.rs` 1,149.
- **Tokens.** Two `Tokens` structs with different vocabularies (lc 40 fields incl. metrics;
  pc 36 fields incl. radii/`kind`/`bevel`); `apps/.../library_host.rs:49-100 library_palette`
  maps one onto the other by hand (every new token must be added in three places, as
  `DESIGN-SYSTEM.md` §2 "Add a token" admits).
- **Icons.** Two registries (`lc icons.rs:171 svg()` match arm per `Icon` variant;
  `pc icon_data.rs` 130 entries + `tool_icon_data.rs` 98), two rasterisation paths
  (`lc icons.rs:251-290`: `egui_extras::image::load_svg_bytes_with_size` with a string replace of
  `currentColor`/stroke width into a per-(icon, px) texture cache; `pc icons.rs:48 paint` via the
  egui image loader with tint). Same Lucide files, two caches, two tint methods (lc recolours
  the SVG text; pc tints the texture).
- **Fonts.** Inter/JetBrains Mono embedded twice: `lc-ui-egui/src/theme.rs:144-148`
  `include_bytes!("../../../assets/fonts/…")` and `pc-text/src/fonts.rs:23-26` from the **same
  files** — `pc-text` even documents that "a second `include_bytes!` of the same file elsewhere
  put a second 1.5 MB copy into the wasm" (`fonts.rs:22`). ≈1.5 MB of duplicate font data in
  the native binary too, plus two `install_fonts` routines.

**User-visible today** (U4): checkboxes, dropdowns and headers change shape between modules;
light theme bodies differ (Astra UX05); the same icon is a hair thinner in the Library
(stroke 1.75 after the string replace) than in the editor (stroke 2).

**Winner / merge.** **`li-ui-kit`** (egui-only crate below both UI crates): one `Tokens`
(the union; the Library's metrics fields become a `Metrics` sub-struct), `ThemeKind` and
`library_palette` logic inside it; the widget catalogue merged pair by pair (keep each pair's
*look* through tokens so neither module changes appearance — the owner likes it as is); one
icon registry + one rasteriser (pc's loader path, with lc's per-size texture cache); fonts
from `pc-text::fonts` only. Effort L overall but each widget pair is an S slice with its own
screenshot test in both themes at both widths (`DESIGN-SYSTEM.md` §7). Overlap: LRC-UX job
(Library panels), menus job (merged). Do tokens + icons + fonts first (S each, invisible),
widgets after LRC-UX lands.

#### D25 Keyboard shortcuts: two dispatchers, three chord parsers, one file included across crates

`lc-ui-egui/src/shortcuts.rs` (485: parse `Cmd+Shift+X` at `:36`, aliases, dispatch) +
`shortcuts_catalog.rs` (208) + `keymaps.rs` (32: Capture One deltas); `pc-ui-egui/src/shortcuts.rs`
(667: own parser at `:10`) + `shortcut_dispatch.rs` (355) + `keymaps.rs` (67: Photoshop/Affinity
deltas, and `#[path = "../../lc-ui-egui/src/keymaps.rs"] pub mod library;` — a source file
compiled into two crates) + `native_menu.rs:269-290 Chord::parse` (third parser) +
`pc-engine::prefs::ShortcutProfiles`. The conflict-aware editor lives in the editor prefs UI;
the Library reads the chosen profile through the host.

**User-visible today.** One profile picker, two dispatch rules (e.g. which keys text fields
keep, how ⌥/⌃ are mapped on Linux), and chord strings parsed three ways (a profile edited in the
editor can parse differently in the Library for rarer keys).

**Winner / merge.** `li-keys`: one `Chord` type + parser + platform mapping, the profile delta
tables (`CAPTURE_ONE`, `AFFINITY`, Photoshop) and conflict detection; both dispatchers consume
it; the `#[path]` include goes away. Effort M. Overlap: `wip/cx-keys` merged; LRC-UX may
touch `lc-ui-egui/src/menus.rs` — sequence after.

#### D26 Errors, notices and toasts

Editor: `pc-ui-egui/src/notices.rs` (non-blocking row, 256-entry history window, control-channel
readable, `status_only` errors; `notices.rs:1-40,168`). Library: 1.4 s toast with no history
(`lc-ui-egui/src/lib.rs:514-522 toast/toast_for`) plus per-task status JSON. **User-visible:**
an import warning in the Library vanishes after 1.4 s and cannot be reviewed; the same class of
warning in Compositing stays in Notices › History. **Winner:** the editor's model, moved to
`li-ui-kit::notices`; Library toasts become `status_only` notices. Effort S–M; Library UI files
are LRC-UX territory — wait.

#### D27 Commands, menus and the native menu bar

Two identical `CommandSpec`/`Enabled`/`Run` shapes (`lc-engine/src/cmd/mod.rs:44-70` and
`pc-engine/src/commands.rs:15-35`), two `UI_COMMANDS` tables (`lc menus.rs:104`, `pc menus.rs:35`),
two in-window menu builders (`lc menus.rs` 1,732 + `menubar.rs` 1,182; `pc menus.rs` 1,726 +
`menu_catalog.rs` 803), and a native macOS bar for the editor only (`pc native_menu.rs` 982 +
`apps/.../mac_menu.rs` 359; U5). **Winner:** one `li-commands` crate for the spec/info types and
the menu model (`MenuItem`, `MenuRole`, `Chord`), so `native_menu::mac_layout` can lay out the
Library's menus too and `command_menu_item` draws both. Effort M (types S; Library menus onto the
model M). Overlap: `wip/cx-menus` merged; LRC-UX owns `lc menus.rs` — wait.

#### D28 Test harnesses: a CPU UI rasteriser the editor's tests cannot use

Library: `lc-ui-egui/src/headless.rs` (2,419) + `softpaint.rs` (470: deterministic CPU
rasteriser of egui output) drive the real app offscreen with no GPU (55 tests in `headless.rs`
alone; `lc-mcp`/CLI reuse it). Editor: `egui_kittest` in 199 files, screenshots through wgpu
(`pc-ui-egui/src/design_system_shots.rs` is `#[ignore]`d: needs a GPU) and the `snapshot`
example (GPU). Six copies of an RTX test prelude in `pc-engine/src/vector_packages/*/gpu.rs`.
**Effect:** the editor's visual gates are GPU-gated and partly ignored (A3); the Library's are
not. **Winner:** `softpaint` as a kittest renderer (`li-testkit`), so both crates' screenshot
tests run on CI without a GPU; one GPU test prelude in `pc-testkit`. Effort M; value: un-gates a
large share of the 192 ignored tests. Overlap: release-hygiene job (`wip/cx-release`) owns CI
lanes and the known-failures list — coordinate, do not collide (its files are `.github/`,
`xtask`, `land-known-failures.txt`; this slice is in `lc-ui-egui`/`pc-testkit`).

#### D29 Automation: two MCP servers, one of them unreachable; no Library CLI

`pc-automation` (rmcp SDK, bridge to the control channel, headless `Session`, security:
token, connection limits, workspace authorisation, budgets; `crates/pc-automation/src/lib.rs:1-40`)
is what `apps/local-image/src/control_server.rs` and `apps/local-image-cli` use. `lc-mcp`
(hand-written JSON-RPC over stdio, 1,441 lines, `crates/lc-mcp/src/lib.rs:1-25`) has **no binary
target and no dependents** (`Cargo.toml` has no `[[bin]]`; no crate or app references
`lightcraft_mcp`), so the Library's MCP tools (`tools.rs`, 607 lines: develop, export, rate,
render…) are not reachable by any agent today; `local-image-cli` is `photocraft_cli` only, so
there is no Library CLI either (the `lightcraft-cli snapshot` mentioned in `headless.rs:6` does
not exist in this tree). **Winner:** `pc-automation`'s server hosts the Library tools
(re-register `lc-mcp/src/tools.rs` as rmcp tools over the same control channel and the Library
`Session`), one security model, one CLI with Library subcommands (import, develop, export,
snapshot). Delete `lc-mcp`'s JSON-RPC server and base64 helper. Effort M; no running overlap.

#### D30 Brush engines (note, not a merge today)

`pc-paint` (7,298: dynamics, tips, dual brush, smoothing, pressure) vs Develop's mask brush
(`lc-pipeline/src/masks.rs:387-520 brush_dabs/rasterize_brush`, `lc-develop/src/settings.rs:1008
BrushStroke`). Different jobs (paint pixels vs rasterise a mask), but the dab generation
(points → dabs with spacing, pressure, smoothing) is the same sub-problem; the Develop brush
feels different from the editor's under a tablet. A later slice can make `lc-pipeline` consume
`pc-paint::dynamics` for dab placement (S–M) once `li-raster` exists; not in this plan's critical
path.

---

## 4. Sequenced consolidation roadmap (Codex-sized jobs, disjoint file ownership)

Rules used: each job owns a disjoint set of files (listed), ships behind golden/parity tests,
and keeps public APIs of the old crates as re-exports so other jobs do not break mid-flight.
"Wait for" names the running job whose files overlap. The owner's builder rule is five jobs at
once; the waves below are sized for that. Effort: S = days, M = 1–3 weeks, L = 1–2 months.

### Wave 0 — can start now, no overlap with the running jobs

| Job | Does (findings) | Owns (files) | Effort | Risk | Parity gate |
|---|---|---|---|---|---|
| **J1 `li-raster`** | D1 resampling core (+ `pc-algo` kernels), D2 blur with #706 guard, D4 `RgbHistogram` + luma | new `crates/li-raster`; `crates/lc-raster/src/{resample,blur,histogram}.rs` (become adapters); `crates/pc-algo/src/{resample,blur,fxutil,histogram,image,pyramid,matting,selection_blur,content_aware,seam,cf_matting}.rs`; `crates/pc-compose/src/masks.rs` (gaussian only); `crates/li-ai/src/{ops,imaging,cloud,refine,inpaint,restoration/byo}.rs` resize calls; `crates/lc-engine/src/smart_sort/bursts.rs` blur | M | low–med | byte goldens of every current filter through the adapters; documented change: small-σ blur now blurs; fringe test on a CC0 cut-out |
| **J2 `li-color`** | D5 colour math + `NamedSpace`/`Trc` unification, D7 LUT parser + tetrahedral sampler | new `crates/li-color` (absorbs `crates/lc-color`); `crates/pc-color/src/convert.rs`; `crates/pc-cms/src/{math,lutfile,lut3d}.rs` (+ `pc-cms` gains a dep on `li-color`); `crates/lc-codecs/src/space.rs`; `crates/lc-pipeline/src/{lut,output}.rs` (enum aliases only); private copies: `pc-algo/src/{segment/slic,quantize,retouch,tone,hdr,stylize,relight,segment/sky}.rs` luma/lab, `pc-paint/src/dynamics.rs` hsv, `pc-compose/src/adjust.rs` hsl + lut sampler, `pc-ui-egui/src/color_picker_ui.rs` hsv, `pc-trace`, `pc-vecfx`, `pc-testkit` oklab, `lc-color/src/profile.rs` hsv | M | low | byte-identical unit tests per function; Develop reference-vector tests unchanged; Match Look told to import from `li-color` |
| **J3 metadata on `lc-meta`** | D9 | `crates/pc-algo/src/exif.rs`; `crates/pc-engine/src/file_cmds.rs` (`read_file_info`/`write_file_info` only), `web_cmds.rs` (`web_xmp`); `crates/pc-codecs/src/orientation.rs`; `crates/pc-io/src/flat.rs` (`drop_exif_thumbnail`, `update_exif_dimensions`); additions to `crates/lc-meta`, `crates/lc-tiff` | M | med | `pc-io` PSD/TIFF export byte goldens unchanged; new Library↔editor File Info round-trip test |
| **J4 one Camera Raw engine** | D17 | `crates/pc-algo/src/camera_raw.rs` (shrinks to the descriptor model); `crates/pc-ui-egui/src/camera_raw_{ui,preview_ui,scope_ui,detail_ui}.rs`; `crates/pc-engine/src/lens_cmds.rs` (`camera_raw_surface`); `crates/pc-io/src/develop_filter.rs` (bridge additions) | M | med | PSD Camera Raw round-trip tests unchanged; new "preview == commit" test |
| **J5 one automation layer** | D29 | `crates/lc-mcp` (delete server, keep `tools.rs` logic), `crates/pc-automation/src/{server,headless,rpc}.rs`, `apps/local-image-cli/src` | M | low | existing `pc-automation` tests + `lc-mcp/src/tests.rs` re-targeted |
| **J6 one decoder set** | D8 (+ D8's extension lists) | `crates/lc-codecs/src/{lib,sniff,jpeg,png_codec,tiff_codec,psd,webp,other,jxl,convert,exif}.rs`; `crates/pc-codecs/src/{format,image}.rs`, `codecs/{jpeg,tiff_ifd,via_image}.rs`; `crates/lc-tiff` (additions); extension lists in `lc-mcp/src/headless.rs`, `pc-ui-egui/src/{dialog_filter,filmstrip_ui,overwrite_original,generate_ui}.rs`, `pc-engine/src/file_cmds.rs` (extension fn only) | L | med | `lc-codecs/tests/it/corpus.rs` decode hashes; `pc-codecs` goldens; HEIC now opens in the Library (new test) |
| **J7 `li-testkit`** | D28 | `crates/lc-ui-egui/src/softpaint.rs` (moves), new `crates/li-testkit`, `crates/pc-testkit`, the six `pc-engine/src/vector_packages/*/gpu.rs` preludes | M | low | kittest screenshot tests pass on CPU; pixel-identical to today's `softpaint` output |
| **J8 SAM defeature + `lc-fetch` retire** | D15/D16 small slices | `apps/local-image/Cargo.toml` (features), `crates/lc-segment/src/tokenizer.rs` (delete → `li-seg::bpe`), `crates/lc-fetch` (delete `http.rs`, thin API over `li-ai::download` or delete), `crates/lc-engine/src/segment/download.rs` | S | low | workspace builds without candle in default features; `lc-segment` still builds behind the feature |
| **J9 `JobCtl` (part a)** | D14 primitives | `crates/lc-sysmem/src/jobs.rs` (new), `crates/pc-raster/src/interrupt.rs` (adapter), `crates/li-ai/src/comfy.rs` (`JobControl` → alias), `crates/li-seg/src/lib.rs` cancel args (**only if the LaMa job has not touched them; else wait**), `crates/lc-fetch` progress, `crates/lc-engine/src/cmd/cull.rs` Control | S–M | low | behaviour tests in `li-ai/src/download/tests_cancel.rs` unchanged |
| **J10 merge bake-off** | D20 slice 0 | new `crates/lc-merge/examples/bakeoff.rs`, `crates/pc-algo/examples/bench_photo.rs` | S | none | measured match quality on a fixed CC0 set → decides J19 |

Start order within the wave (five at a time): J1, J2, J4, J3, J6; then J5, J7, J8, J9, J10.
J1 and J2 are independent; J3 and J6 own different files in `pc-codecs`/`pc-io` (listed) and
may run together; J4 depends on nothing but should land before J2 touches `camera_raw.rs`'s
`curve_lut` (J2 leaves that file alone).

### Wave 1 — after a named running job lands

| Job | Does | Wait for | Owns | Effort | Risk |
|---|---|---|---|---|---|
| **J11 `li-fs`** | D10 atomic/durable writes | library-safety (`wip/cx-library-safety`) | `crates/lc-catalog/src/safe_file.rs`, `crates/pc-format/src/atomic.rs` (+ `store.rs:739`), `crates/li-ai/src/settings.rs` (write), every ad hoc `.tmp`+`rename` site listed in D10 | M | low |
| **J12 `li-gpu`** | D13 one device, one health, shared WGSL | GPU-coordination (`wip/cx-gpu`) | `crates/lc-gpu/src/{ctx,backend,lib}.rs` (device plumbing), `crates/pc-gpu/src/health.rs`, `apps/local-image/src/gpu_startup.rs`, `crates/pc-ui-egui/src/gpu_canvas.rs` (device handoff + inline WGSL), new `crates/li-gpu` | L | med–high |
| **J13 Heal on the patch engine** | D18 | on-device LaMa (`wip/cx-lama`) | `crates/lc-pipeline/src/spots.rs`, `crates/lc-engine/src/enhance/remove.rs`, `crates/lc-engine/src/tests_spots.rs`, Develop spot UI in `lc-ui-egui/src/panels/detail.rs` (after LRC-UX too) | M | med |
| **J14 `li-matte`** | D16 edge refinement + Feather/Edge controls | LaMa; J1 | `crates/pc-algo/src/{cf_matting,matting}.rs` (move), `crates/lc-pipeline/src/masks.rs` refine, `crates/li-seg` (new module) | M–L | med |
| **J15 model registry** | D15 | LaMa | `crates/li-ai/src/{catalog,inventory,settings}.rs`, `crates/li-seg/src/lib.rs` MODELS wiring, `crates/pc-engine/src/seg.rs`, `crates/lc-engine/src/segment/mod.rs` (model_dir), `crates/pc-ui-egui/src/ai_ui.rs` download fns | S–M | low |
| **J16 one preferences store** | D22 | **is** the LRC-UX job's scope (ask it to cover `library-ui.json` + `AiSettings` + one `config_dir`) | `apps/local-image/src/{services,app_dirs,library_host}.rs`, `crates/pc-engine/src/prefs.rs`, `crates/lc-ui-egui/src/state.rs`, `crates/li-ai/src/settings.rs`, `crates/lc-engine/src/camera_profiles.rs` (config_dir) | M | med |
| **J17 `li-ui-kit` part 1: tokens, icons, fonts** | D24 | LRC-UX (touches `library_host.rs`) | new `crates/li-ui-kit`; `crates/lc-ui-egui/src/{theme,icons}.rs`; `crates/pc-ui-egui/src/{theme,icons,icon_data,color_icons}.rs`; `apps/local-image/src/library_host.rs` (`library_palette`) | M | low |
| **J18 `li-ui-kit` part 2: widgets + notices + histogram widget** | D24, D26, D4 widget | J17; LRC-UX | `crates/lc-ui-egui/src/widgets.rs`, `crates/pc-ui-egui/src/{widgets,notices}.rs`, `crates/lc-ui-egui/src/lib.rs` toast, panel call sites | L (as S slices per widget pair) | med (visual) — both themes × both widths screenshots |
| **J19 `li-merge`** | D20 | J10 result | `crates/lc-merge`, `crates/pc-algo/src/{hdr,panorama,features}.rs`, `crates/lc-engine/src/merge.rs`, `crates/pc-engine/src/{align_cmds,photo_cmds}.rs` | L–XL | med |
| **J20 `li-keys`** | D25 | LRC-UX | `crates/lc-ui-egui/src/{shortcuts,shortcuts_catalog,keymaps}.rs`, `crates/pc-ui-egui/src/{shortcuts,shortcut_dispatch,keymaps}.rs`, `native_menu.rs` (Chord), `crates/pc-engine/src/prefs.rs` (ShortcutProfiles types) | M | med |
| **J21 `li-commands` + native Library menus** | D27 | LRC-UX; J20 | `crates/lc-engine/src/cmd/mod.rs` types, `crates/pc-engine/src/commands.rs` types, `crates/lc-ui-egui/src/{menus,menubar}.rs`, `crates/pc-ui-egui/src/{menus,menu_catalog,native_menu}.rs`, `apps/local-image/src/mac_menu.rs` | M | med |
| **J22 soft proof via CMS** | D6 | LRC-UX; J2 | `crates/lc-pipeline/src/output.rs` (ProofParams), `crates/lc-engine/src/view.rs`, `crates/lc-ui-egui/src/panels/edit.rs` (soft-proof rows) | S–M | low |
| **J23 filmstrip on `lc-preview`** | D11 | LRC-UX | `crates/pc-ui-egui/src/filmstrip_ui.rs`, host service in `apps/local-image/src/library_host.rs` | S–M | low |
| **J24 blake3 content identity** | D12 | library-safety; J11 | `crates/lc-preview/src/hash.rs`, `crates/lc-engine/src/media.rs` (content_key), `crates/lc-engine/src/import.rs` duplicate keys | S | med (persisted keys: migrate on next scan) |
| **J25 jobs runner + Library tasks (part b)** | D14 | J9; LRC-UX; LaMa | `crates/pc-engine/src/jobs.rs`, `crates/lc-engine/src/enhance/mod.rs` (Job), `crates/lc-ui-egui/src/{tasks,export_task,smart_sort_task,people_task}.rs`, `crates/pc-ui-egui/src/jobs_ui.rs` | M–L | med |
| **J26 lens profile through the bridge + red-eye** | D21 | lens-correction / raw-quality (`wip/cx-lenscorr`); J3 | `crates/pc-io/src/develop_filter.rs` (sanitize rule), `crates/pc-engine/src/{lens_cmds,redeye_cmds}.rs` | S–M | low |
| **J27 export dialogs: one kernel list** | D1 slice 4 | LRC-UX; J1 | `crates/lc-ui-egui/src/panels/export_dialog.rs`, `crates/pc-ui-egui/src/export_dialog.rs` | S | low |
| **J28 dense `Frame<T>`** | D3 | J1 | `crates/li-raster` (type), `crates/pc-algo/src/image.rs`, `li-ai`/`li-seg` boundaries | M | low |
| **J29 `li-i18n`** | D23 | everything UI-side above (last, per owner priority) | new `crates/li-i18n`; `crates/lc-ui-egui/src/i18n.rs`, `locales/`, `build.rs`; `crates/pc-ui-egui/src/i18n/` | L | low (mechanical) |

Parallelism in wave 1: {J11, J24} share `lc-engine` write paths → serial; {J12} alone in GPU
files; {J13, J14, J15} all wait for LaMa and then are disjoint (`spots`/`enhance` vs
`matting`/`masks` vs `catalog`/`seg.rs`); {J17 → J18 → J21} serial on the UI kit; {J20, J22,
J23, J27} disjoint and can run beside J18; J19 and J29 are independent long jobs.

### What the running jobs should be told now

- **GPU coordination:** its VRAM/retry design becomes the `DeviceHealth` of `li-gpu` (J12);
  please keep it in `pc-gpu::health` shape, not a third model.
- **On-device LaMa:** register the model through `li-ai` catalog + `download_seg_files`
  (J15), run through `JobCtl`-compatible cancel/progress (J9), and store results as patches
  (J13 will put Heal on the same path).
- **LRC-UX / one Preferences window:** one *store* too (J16): `preferences.json` with
  `library.*` and `ai.*` sections, one `config_dir`, one models root.
- **Library safety:** build only on `lc-catalog::safe_file`; J11 will make it the app-wide
  writer.
- **Match Look:** import colour maths from `lc-color` (soon `li-color`, J2); do not add
  Lab/OkLab/luma copies.
- **Raw quality / lens correction:** `lc-pipeline/src/optics.rs`/`lensdb.rs` are yours; J26
  comes after.
- **Release hygiene:** J7 (`li-testkit`) will un-gate GPU-only UI snapshot tests; keep the
  known-failures list format so J7 can remove entries.

---

## 5. Duplication that is intentional and should stay

| Pair | Why it stays |
|---|---|
| `pc-raster::Surface` (tiled, copy-on-write, any depth) vs `lc-raster::Image<T>` (dense frames) | Documents need COW tiles for undo/autosave/GPU residency; the develop pipeline needs dense rows for SIMD-friendly stages. One bridge (`pc-io::develop_filter`, `doc_layers.rs`) is enough; D3 only unifies the *dense* family. |
| `lc-geom` (normalised `f64` image coordinates, `Orientation`, crop model) vs `pc-geom` (integer document pixels, tiles) | Different coordinate systems by design (resolution-independent settings vs pixel canvases). A shared `Affine`/`Point` could come later; low value. |
| `lc-catalog::Journal` (op log + snapshots) vs `pc-ops::History` (document snapshots) vs `pc-format::autosave` | Catalog durability vs in-memory document undo vs crash recovery of unsaved documents: three jobs. The Develop layer's two histories are documented behaviour of the live link. |
| Photoshop-parity filters (`pc-algo` Reduce Noise, Unsharp Mask/Smart Sharpen/High Pass, manual Lens Correction, Perspective Warp, Vanishing Point, natural-cubic Curves) vs Develop stages (darktable NR, capture sharpening, lensfun Optics, Upright, monotone curves) | Different tools with different semantics the user expects; the editor already reaches the Develop versions through Camera Raw Filter (`filter.develop`). Only the *second copies inside* `camera_raw.rs` go (D17). |
| darktable / RawTherapee / vkdt ports with their own kernels (`lc-raw::numerics::gaussian9`, `lc-pipeline::eigf`, `basecurves::hermite`, `detail/{nr,haze}`) | Provenance-pinned ports (`docs/PORTS.md`); replacing their inner kernels with a generic blur would break reference-vector parity. They may *call* `li-raster` for plain pre-blurs only where the fixtures stay green. |
| CPU reference + WGSL twin (`lc-pipeline` ↔ `lc-gpu`, `pc-compose` ↔ `pc-gpu`, `li-seg::denoise` ↔ `lc-gpu::denoise`) | The twin is the design (CPU is the reference, GPU parity-tested). D13 shares the device and the common snippets, not the kernels. |
| `pc-geom::{warp,cage}` (geometry) vs `pc-algo::{warp,cage}` (resampling through that geometry) | A deliberate geometry/raster split, not a duplicate. |
| `flate2` (editor) vs `miniz_oxide` (Library) | `flate2` is built with `rust_backend`, which *is* `miniz_oxide`; one implementation, two APIs. Harmless. |
| One ComfyUI client (`li-ai::comfy`) behind `lc-engine::enhance::AiHost` (`apps/.../develop_ai.rs`) | Already the right shape: the Library knows no ComfyUI. Keep. |
| Display colour (`pc-engine::display_color::CanvasDisplay` used by both canvases, `apps/.../monitor_profile.rs`) | Already shared today. Keep; D6 only moves soft proofing onto it. |
| `lc-preview::JobPool` (slot de-dup, priorities for thumbnails/loupe) vs the command job runner (D14) | Different scheduling problem (render slots that supersede each other vs one-shot commands). Keep both; share `JobCtl`. |
| `pc-algo::segment` classical selection (GrabCut, Quick Selection, Focus Area) vs `li-seg` models | The classical path is the no-model fallback and the interactive brush model; models are optional downloads. Keep; share edge refinement (D16). |
| `pc-codecs` own VP8 lossy encoder, `lc-codecs` 10-bit AVIF encoder, `jxl-oxide` | Single implementations each; after D8 they simply live in one crate. |
| Grain/noise hashes (`lc-pipeline::finish::grain`, `lc-scenes::noise`, `pc-algo::noise::hash01`, `pc-color::dither_noise`) | The *looks* are tool-specific and deterministic by hash constants; changing them changes rendered output. Keep; only the canvas dither (D13) should match across modules. |
| Two command *sets* (Library commands vs editor commands) | The commands differ; only the `CommandSpec` types and the menu model merge (D27). |
| `lc-ui-egui::i18n::builtin_label` policy (never translate user-named presets) | Policy, not duplication; `li-i18n` keeps it. |

---

