# Research: raw decoders and what drives raw image quality

Date: 2026-10-13. Scope: Local Image v2 (all-Rust, GPL-3.0-or-later, never sold). Owner goal: maximum
image quality from raw files for headshots, architecture and events. Research and recommendations only;
no code was changed. Written incrementally; section 6 lists every source used.

What we already have (checked in `crates/lc-raw`, `crates/lc-pipeline`, `crates/li-seg`, `docs/PORTS.md`):

- Our own decoders: DNG (uncompressed, LJPEG, lossy, Deflate, float, JXL not yet), CR2, NEF/NRW (not "lossy after
  split", not HE/HE*), ARW (uncompressed, ARW2, lossless), uncompressed RAF incl. X-Trans, RW2/RWL (all
  compressions), PEF, uncompressed ORF. `embedded_preview` also reads CR3.
- rawler (dnglab 0.8.0) is being added as an unmodified dependency for CR3, compressed RAF, compressed ORF.
- Colour: rawler camera TOML matrices + RawTherapee `camconst.json` baked into `camera_matrices.rs`; DCP
  reader (`dcp.rs`, bundled CC0 profiles from RawTherapee's `dcpprofiles/`); DNG colour model incl.
  dual-illuminant interpolation, forward matrices, profile look tables/tone curve.
- DNG opcodes: parsed all; applied WarpRectilinear, FixVignetteRadial, FixBadPixels*, MapTable/Polynomial,
  GainMap, Delta/Scale per row/column. TrimBounds / WarpFisheye recorded, not applied.
- Sensor saturation detection (`saturation.rs`: true clip level found from the sample pile-up).
- Demosaic: bilinear, PPG, AHD, VNG4, RCD, AMaZE, dual RCD/AMaZE + VNG (darktable ports); X-Trans uses our
  own edge-weighted interpolation (no Markesteijn). Capture-sharpening radius estimate (darktable port) and
  deconvolution in `lc-pipeline`.
- Highlights: our own `reconstruct`, `clip_neutral`, darktable "inpaint opposed" and the full segmentation
  based v2 (`segbased.rs`). Not: darktable "guided laplacians", RawTherapee "colour propagation".
- Denoise: darktable denoise-profiled port (wavelets/NLM) in `lc-pipeline/src/detail/nr.rs`; AI denoise
  (RawNIND UtNet2 Bayer + linear Rec.2020, NIND UNet) through `tract` in `li-seg/src/denoise.rs`.
- Lens: lensfun 0.7.0 crate (LGPL-3.0) with ported correction models.
- Not found in the tree: darktable `noiseprofiles.json` style per-camera/ISO noise profiles, rawspeed
  `cameras.xml` style per-ISO black/white tables, pixel-shift merging, LMMSE/DCB demosaic, Markesteijn.

---

## 1. Plain-language summary for the owner

**Where we stand.** Our own Rust decoders already read the mainstream formats correctly, and for demosaic,
highlight repair and denoise we already carry the same algorithms darktable 5.6 uses (AMaZE, RCD, dual,
segmentation highlights, profiled denoise, the RawNIND neural denoiser, capture sharpening). rawler (pure Rust)
fills the CR3 / compressed RAF / compressed ORF gaps. There is no pure-Rust alternative with wider coverage, and
the C++ libraries (LibRaw, rawspeed, Adobe's SDK) are off limits for the build anyway, so the decoder choice is
settled: ours first, rawler second, measured per format.

**What actually makes pictures better now is data, not code.** Lightroom and Capture One do not win on
bit-unpacking; they win because they know, for every camera, exactly where each colour channel clips at each ISO
and aperture, where black sits at each ISO, what the lens embedded in the file says about distortion, and they
carry a measured colour profile with hue/saturation tables. All of that exists in open form (RawTherapee's
`camconst.json`, rawspeed's `cameras.xml`, darktable's noise and white-balance tables, dcamprof for making your
own profiles) and can be shipped as data inside a GPL app. Adobe's own profiles cannot be bundled, but the app can
read them from a user's Adobe DNG Converter install, which costs nothing.

**Top five, in order:**

1. Import the open per-camera tables (white/black per ISO and aperture, crops, PDAF rows, noise profiles, WB
   presets). Biggest fix for magenta highlights and colour-cast shadows; makes profiled denoise real.
2. Build the decoder comparison harness (ours vs rawler vs darktable/RawTherapee/LibRaw run as external tools)
   over the CC0 raw.pixls.us corpus and let the numbers choose the primary decoder per format.
3. Colour: read Adobe DCP/LCP from the user's install, add triple-illuminant support, and make the owner's own
   dual-illuminant profiles with dcamprof from a ColorChecker shot. This is the skin-tone lever.
4. Port Markesteijn 3-pass (+ dual) for X-Trans, and LMMSE/DCB with automatic method choice by ISO for Bayer.
5. Embedded lens corrections (Sony, Fuji, Panasonic first) so new lenses are corrected on day one.

**Honest limits.** Nikon HE/HE* raws use a licensed codec no open project can decode; the app should say so and
open the preview. Matching Lightroom *Denoise* / DxO DeepPRIME at ISO 12800+ needs a trained network of our own
(months, not weeks); the RawNIND model we ship is the best open starting point.

---

## 2. Decoder comparison table

Licence column: "GPL3 OK" means it can be linked into / shipped with a GPL-3.0-or-later program.
"Rust" means it can go in our build; C/C++ libraries are reference only under our pure-Rust rule.

| Library | Language | Licence (GPL3 OK?) | Coverage | New cameras | Correctness reputation | Speed | Activity 2025-26 |
|---|---|---|---|---|---|---|---|
| **LibRaw** 0.22.1 (Apr 2026) | C++ | LGPL-2.1 or CDDL-1.0 (yes, via LGPL) | Widest of all: 1000+ cameras; CR3 (incl. CRAW), Nikon HE/HE* (via "standard compression" only in 0.22 notes), Fuji compressed and lossy RAF, Sony lossless, Panasonic v8, Hasselblad, Phase One, GoPro (via GoPro SDK), DNG 1.7 incl. JXL (via Adobe DNG SDK), Samsung V3, OM 14-bit hi-res, Nikon NEFX pixel-shift merge, Sony SRAW/YCC | Fastest: dozens of bodies per release, often within weeks of launch; snapshot builds between releases | The de-facto reference ("what dcraw became"); used by RawTherapee ≥5.11, digiKam, ImageMagick, Affinity, many commercial apps. Known: colour matrices are dcraw-style single-matrix `adobe_coeff` tables; black levels occasionally wrong on first support; security CVEs fixed in 0.22.1 | Single-threaded decode; slower than rawspeed on LJPEG (CR2) by 1.5-3x per its own notes; fine elsewhere | Very active: 0.22.0 Dec 2025, 0.22.1 Apr 2026, public snapshot repo |
| **rawspeed** (darktable) | C++17 | LGPL-2.1 (yes) | ~900 cameras in `cameras.xml`; most mainstream formats incl. CR3, compressed RAF, compressed ORF, Sony lossless, Panasonic v8, Fuji 16-bit; NOT Nikon HE/HE*, Fuji lossy RAF, Apple ProRAW/JXL DNG, CinemaDNG lossless | Continuous (darktable 5.4: ~20 new bodies; 5.6: more). Needs a CC0 sample on raw.pixls.us before support | Very high: per-camera `cameras.xml` with crop, per-ISO black/white, colour matrix, sanity-checked against the raw.pixls.us corpus; OSS-Fuzz; the strictest decoder (will refuse unknown bodies rather than guess) | Fastest CPU decoder; multi-threaded strips/tiles, SIMD LJPEG | Very active (7,254 commits, 2026 copyright); tied to darktable releases |
| **RawTherapee decoder** (5.13, Jul 2026) | C++ | GPL-3.0-or-later (yes) | Since 5.11/5.12 a *patched LibRaw* is the primary reader, with the old patched dcraw as a fallback for a few legacy formats; adds `camconst.json` (per-ISO white levels, aperture scaling, crop, matrices) | Follows LibRaw; `camconst.json` entries added per body | High: `camconst.json` is the best open source of per-ISO white levels and raw-crop tweaks; RT has strong colour (DCP, dual-illuminant) and pixel-shift (Pentax) support | LibRaw speed | Active (5.13 Jul 2026) |
| **rawler / dnglab** 0.8.0 (Aug 2026) | Rust (pure, forbid-unsafe in parts) | LGPL-2.1 (yes; §3 relicense to GPL-3 allowed) | ~1,100 entries in SUPPORTED_CAMERAS.md: Canon CR2/CR3 incl. craw, sRaw, CRM; Nikon 12/14-bit (no HE/HE*); Sony; Fuji incl. compressed RAF; Olympus/OM incl. hi-res; Panasonic; Pentax; Leica; Hasselblad; Phase One; Samsung; Kodak; JXL-DNG (0.7.1); Apple QuickTake | Good: each release adds 10-20 bodies, typically months after launch (e.g. A7R VI, Nikon ZR, R50 V in 0.8.0) | Good but younger; TOML per-camera data (matrices, black/white, crop); many panic-to-error fixes in 0.8.0 (fuzzing by qarmin). Demosaic added in 0.8.0 is basic (bilinear, PPG, Markesteijn) and not our concern | Good; multithreaded in places (`multiversion` SIMD dispatch) | Active (0.7.0 Jun 2025, 0.7.1 Oct 2025, 0.7.2 Feb 2026, 0.8.0 Aug 2026) |
| **rawloader** 0.37.x (pedrocr) | Rust | LGPL-2.1 (yes) | ~200 Bayer cameras; no CR3, no X-Trans decode path for newer bodies | Stalled; no meaningful releases since ~2022 | Was the base for rawler; decent for what it covers | Good | Effectively unmaintained |
| **imagepipe** 0.5.x (pedrocr) | Rust | LGPL-2.1 | Pipeline on top of rawloader (demosaic, WB, curves) | Stalled (last update ~2022) | Simple; not quality-oriented | - | Unmaintained |
| **quickraw** 0.2.1-alpha (qdwang) | Rust | LGPL-2.1 | Small list; integer arithmetic for speed | Stalled since Jun 2023 | Author states results are less precise than float pipelines | Fast | Unmaintained |
| **zenraw** 0.2.0 (Imazen, Sep 2026) | Rust | AGPL-3.0-only or commercial (AGPL-3 is one-way compatible with GPL-3: a combined work must be AGPL-3; not acceptable for a GPL-3 app) | Wrapper: backends rawloader / rawler / darktable-cli; own Malvar-He-Cutler demosaic | Follows rawler | Thin layer; nothing we lack | - | New |
| **libopenraw** 0.4.0-alpha.x (Hubert Figuière) | Rust (rewrite of C++; C API) | LGPL-3.0-or-later (yes) | CR2/CR3/CRW, NEF/NRW, ARW/SR2, RAF, ORF, RW2, PEF, ERF, MRW, DNG; aims to match 0.3.x feature set | Steady small additions (e.g. GFX 100RF, 2025) | Thumbnail/preview focused historically; colour correction "unfinished" in alpha notes | - | Alpha since 2024; alpha.9 Mar 2025 |
| **Adobe DNG SDK** 1.7.1 (Jun 2026) | C++ | Adobe DNG SDK licence (BSD-like, read as GPL-compatible by digiKam/Debian; docs clauses non-free; licence text reportedly missing from 1.7.1 download) | DNG only (all versions incl. 1.7 JXL, semantic masks, gain maps) — the reference for DNG semantics | n/a | The reference implementation of the DNG colour model and opcodes; what Lightroom does | - | Updated with each DNG spec revision |
| **dcraw** (Dave Coffin, last 9.28, 2018) | C | GPL-2 (in practice) | Historical; no camera after 2018 | None | The origin of `adobe_coeff` matrices and most demosaic names; superseded | - | Dead; reference only |
| **Halide camera_pipe / hdr-plus** | C++ (Halide DSL) | MIT / varies | Not a decoder (uses dcraw/LibRaw); demo pipelines for demosaic and HDR+ burst merge | n/a | Educational; not production | Very fast on GPU/SIMD | Research code |
| **rsraw / libraw-rs** | Rust bindings to LibRaw | LGPL via LibRaw | LibRaw coverage | LibRaw | LibRaw | LibRaw | Bindings only; violate the pure-Rust rule |

Takeaways for us:

- For *coverage* nothing open beats LibRaw; for *per-camera correctness data* nothing open beats rawspeed's
  `cameras.xml` + RawTherapee's `camconst.json` + darktable's `noiseprofiles.json`/`wb_presets.json`.
  All three data files are LGPL/GPL and can be *consumed as data* (or transcribed into TOML) without any C++.
- rawler is the only maintained pure-Rust decoder with real breadth. Keeping it whole and unmodified (as
  planned) is right; the ours-vs-rawler comparison should be driven by rawspeed/LibRaw output as the
  third opinion, not by eye.
- The gaps every open decoder shares today: Nikon HE/HE* NEF (TicoRAW; LibRaw 0.22 notes say Z8/Z6III only
  "standard compression"), Fuji lossy RAF (LibRaw only), Apple ProRAW / JXL DNG (LibRaw via DNG SDK;
  rawler since 0.7.1; rawspeed no), CinemaDNG lossless.


---

## 3. Quality drivers and best sources

Bit-unpacking is table stakes: once the samples are right, the picture is decided by per-camera data and by the
few algorithms that touch every pixel (demosaic, highlights, denoise). Below, "we have" refers to the code checked
above; "best open source" is where to get the data, and under what licence.

| Driver | Why it matters for headshots / architecture / events | We have | Best open sources (licence) |
|---|---|---|---|
| **Colour matrices (camera → XYZ)** | Skin tones and brand colours. A wrong or single-illuminant matrix shifts hue by several ΔE; dual-illuminant (A + D65) interpolation matters under tungsten event lighting | rawler TOML matrices + RawTherapee `camconst.json` gaps (D65 mostly; "missing illuminants stay missing"); DNG `ColorMatrix1/2` from DNGs | (1) **rawspeed `cameras.xml`** `<ColorMatrices>` (CC BY-SA 3.0 on the data file; same Adobe-derived numbers as dcraw/LibRaw `adobe_coeff`). (2) **LibRaw `adobe_coeff` table** in `src/tables/colordata.cpp` (LGPL-2.1): the largest list, one D65 matrix per body. (3) **RawTherapee `camconst.json`** `dcraw_matrix` overrides for bodies where dcraw's is wrong (GPL-3). (4) **Adobe DNG Converter**: run the user's own installed copy on one file per body and read `ColorMatrix1/2`, `CalibrationIlluminant1/2`, `ForwardMatrix1/2` from the output — these are the *dual-illuminant* Adobe Standard numbers; we may read a user's converted DNG, but must not bundle the matrices from the DCPs themselves (Adobe profile licence, see DCP row) |
| **Camera profiles (DCP: HueSatMap, LookTable, tone curve)** | This is most of the "Lightroom look" and of Capture One's skin-tone reputation; a matrix alone leaves saturated reds/oranges and skin slightly off | DCP reader + evaluator (`lc-color/profile.rs`: dual-illuminant HueSatMap, LookTable, ToneCurve); bundled CC0 profiles from RawTherapee `dcpprofiles/` (small set) | (1) **Adobe DNG Converter / Camera Raw profiles** (`Adobe Standard`, `Camera Matching`): free download, but the Adobe profile licence only allows redistribution standalone or embedded in images, *not bundled with software*. Pattern used by RawTherapee and darktable users: *import from the user's own installation* (`C:\ProgramData\Adobe\CameraRaw\CameraProfiles`, macOS `/Library/Application Support/Adobe/CameraRaw/CameraProfiles`, Wine prefix on Linux). (2) **RawTherapee `rtdata/dcpprofiles`** (already used; CC0 / per-file rights). (3) **Make our own with dcamprof** (GPL-3, C) from a ColorChecker shot: produces dual-illuminant DCP with HueSatMap and LUT; the owner can shoot an SG/24 chart under tungsten + daylight for each of his bodies — this is what gives "better than Adobe" skin for *his* cameras. (4) **camspec SSF database** (CC BY-NC-SA 4.0: non-commercial, fine for a never-sold GPL app but check attribution) gives spectral sensitivities for many Canon/Nikon/Sony/Pentax bodies; dcamprof can build a profile from SSF + illuminant without a chart |
| **Triple-illuminant profiles (DNG 1.6: `ColorMatrix3`, `CalibrationIlluminant3`, `ProfileHueSatMapData3`)** | Newer Adobe profiles for recent bodies ship three illuminants; without support we silently fall back to two | Two illuminants only (`hue_sat_map: [Option; 2]`) | DNG 1.6/1.7 spec (Adobe, freely downloadable); nobody open implements it yet (RawTherapee: 2 max; AliceVision: >2 unsupported). Low effort, small but real gain on new bodies |
| **Black level** | Shadow colour casts (green/magenta) and noise floor in event shots; many bodies change black with ISO (Canon RP: ~512 low ISO, ~2048 high ISO) | From file tags/masked areas where present; `saturation.rs` finds the true clip | **rawspeed `cameras.xml` `<Sensor black= white= iso_list=>`** per-ISO tables (CC BY-SA 3.0) and `<BlackAreas>` (optical-black rectangles, let the decoder *measure* black per frame); **RawTherapee `camconst.json` `masked_areas`** |
| **White level / per-ISO and per-aperture clip** | Highlight reconstruction and the "magenta highlight" bug depend entirely on knowing where each channel really clips. Canon bodies scale raw values at apertures wider than f/4 (`aperture_scaling` in camconst) | `saturation.rs` detects pile-up per file (good, but only works when pixels actually clip); `white_max` style data from camconst gaps already baked for matrices only | **RawTherapee `camconst.json`** `ranges.white` per ISO (per channel where needed), `white_max`, `aperture_scaling` (GPL-3; quality grades A/B/C) — the single best open source; **rawspeed `cameras.xml`** per-ISO `white` |
| **Raw crop / masked borders** | Wrong crop = dark edge or 1-px colour fringe; matters for architecture when stitching | DNG `ActiveArea`/`DefaultCrop`; vendor tags where known | `cameras.xml` `<Crop>`, `camconst.json` `raw_crop` (with `frame` keyed variants for multi-aspect bodies) |
| **CFA / sensor quirks: PDAF rows, green imbalance, line noise** | Sony/Nikon PDAF striping/banding in pushed shadows (event work at ISO 6400+), Panasonic/Olympus green imbalance → maze artefacts after AMaZE | Not found | `camconst.json` `pdaf_pattern`/`pdaf_offset` + RawTherapee `rtengine/pdaflinesfilter.cc`, `green equilibration`, `line noise filter` (GPL-3). darktable has `rawprepare` flat-field/green-eq too |
| **Bad / hot pixel maps** | Long exposures (architecture interiors, night) and sensor-age hot pixels | DNG `FixBadPixelsConstant/List` opcodes applied | darktable `hotpixels` module (GPL-3) median-based detection; RawTherapee dark-frame + `.badpixels` text format; LibRaw `bad_pixels` file format (simple x y timestamp lines) — worth adopting the LibRaw/RT text format for user maps |
| **Dark frames / flat fields** | Night architecture; vignetting of fast portrait primes at f/1.2–1.4 | DNG `GainMap` opcode applied (phone DNGs, Adobe-converted DNGs) | RawTherapee `rtengine/rawimagesource.cc` dark-frame subtraction + flat-field (GPL-3); darktable flat-field via `rawprepare` |
| **White-balance presets** | "As shot" works, but named presets (daylight/tungsten/flash) per body need camera-measured multipliers; also "camera reference D65" for the DNG colour model | Reads as-shot multipliers from vendor tags | **darktable `data/wb_presets.json`** (from `wb_presets.c`, GPL-2+; ~900 bodies), ART copies the same |
| **Lens corrections (database)** | Architecture: distortion and TCA; headshots: vignetting of fast primes | lensfun 0.7.0 crate + ported models | **lensfun DB** (CC BY-SA 3.0 for data, LGPL-3 for lib): ~1,000 profiles, **stalled upstream** (0.3.4 is still the release; most new profiles arrive via the calibration web service / GitHub); Adobe **LCP** files (same redistribution limits as DCP; RawTherapee reads LCP from the user's Adobe install — GPL-3 reader in `rtengine/lcp.cc`) |
| **Lens corrections (embedded in raw)** | Modern mirrorless lenses (Sony E, Fuji X/GF, Canon RF, Nikon Z, Panasonic) ship distortion/vignetting/TCA tables *in the maker notes*; this is what Lightroom/C1 use for "built-in profile" and why they cover new lenses on day one | Not found (RW2 tag 0x0119 noted as unused in docs) | **darktable `src/iop/lens.cc` "embedded metadata"** path (GPL-3): Sony (SR2 private `0x9400`-area distortion/vignetting/CA polynomials) and Fujifilm (`RAF` tags `0x9650`-series), reverse-engineered; RawTherapee `rtengine/rtlensfun.cc` + metadata correction (Sony, Fuji, Olympus, Panasonic, DNG `WarpRectilinear`). Canon RF and Nikon Z data are known to exist (LibRaw/exiftool tag names) but no open decoder applies them yet; **DNG opcodes** are the open interchange (we already apply `WarpRectilinear`, `FixVignetteRadial`) |
| **Noise profiles (per camera/ISO a·x + b)** | Profiled denoise (wavelet/NLM) is only as good as the Poisson-Gaussian fit; also needed to choose demosaic (LMMSE vs AMaZE) and for the capture-sharpening blend | darktable denoise-profiled *algorithm* ported, but no `noiseprofiles.json` data found in tree | **darktable `data/noiseprofiles.json`** (GPL-3; ~1.8 MB, hundreds of bodies, per ISO `a[3]`, `b[3]`); generator `tools/noise/darktable-gen-noiseprofile` (bash + C) if the owner wants to profile his own bodies; RawTherapee instead *estimates* noise per image (`rtengine/FTblockDN.cc`) — a per-image estimator is a good fallback when no profile exists |
| **Pixel-shift / multi-shot** | Architecture: Pentax, Sony (ARQ via Imaging Edge), Panasonic/OM (in-camera merged, decoder-only), Nikon NEFX (LibRaw 0.22 merges) | Not found | **RawTherapee `rtengine/pixelshift.cc`** (GPL-3, Ingo Weyrich): Pentax 4-frame merge with motion detection (dcrawps-derived green-difference test, 4 pixels × 2 samples); LibRaw 0.22 NEFX merge (LGPL) and `SonyPixelShift2DNG` (closed). In-camera merged high-res ORF/RW2 are plain decodes (rawler covers OM "highres" modes) |
| **Dual-pixel (Canon DPRAW)** | Niche; CR2/CR3 dual-pixel files carry a second sub-image; LibRaw exposes it | Not found | LibRaw `dpraw` handling (LGPL) for reference; low priority |
| **Vendor opcodes / DNG 1.6-1.7 extras** | ProRAW/Pixel DNGs: `ProfileGainTableMap` (per-region exposure), semantic masks, JXL tiles | Opcodes applied; JXL DNG not decoded by lc-raw (rawler 0.7.1+ via `jxl-oxide` 0.12, pure Rust) | Adobe DNG spec 1.7.1; DNG SDK as reference; rawler for JXL tiles |
| **Capture sharpening / lens blur** | Real resolution gain on every file (diffraction at f/8–f/11 architecture, AA-filter softness) | Ported from darktable 5.4 (radius estimate + RL deconvolution) | RawTherapee `rtengine/capturesharpening.cc` (origin, GPL-3) incl. the 5.13 *pre-sharpening denoise* option |
| **Reference renders for validation** | Need a "known good" picture to measure against | Our corpus test (raw.pixls.us) checks *decodes*, not *values*; the camera-preview colour fit uses embedded JPEGs as a weak target | `darktable-cli`/`rawtherapee-cli` (GPL, run as external tools, not linked) to produce linear-TIFF references; Adobe DNG Converter output as a second opinion for colour tags; LibRaw `dcraw_emu -4 -T` for linear 16-bit references |

Notes on licences for *data*:

- rawspeed's `cameras.xml` is CC BY-SA 3.0; camconst.json, noiseprofiles.json and wb_presets.json are part of GPL
  projects. All are compatible with shipping inside a GPL-3 program with attribution (CC BY-SA 3.0 → GPL-3 is one-way
  compatible for *adaptations* per CC's compatibility list; keeping the file verbatim and crediting is simplest).
- Adobe DCP/LCP: do **not** bundle. Read from the user's own Adobe install, and offer a one-click "find Adobe
  profiles" button. This is what RawTherapee users do by hand today.
- The `adobe_coeff` matrices in dcraw/LibRaw/rawspeed/rawler are all the same Adobe numbers transcribed; rawler
  already elected GPL-3 under LGPL §3 in our tree.

---

## 4. Demosaic, highlight and denoise state of the art

### 4.1 Demosaic (Bayer)

| Method | Character | Where it is best | Known failure | Open implementation (licence) | We have |
|---|---|---|---|---|---|
| **RCD** (Luis Sanz Rodríguez) | Ratio-corrected directional green, diagonal R/B. Sharp, few maze artefacts, fast | darktable default since 4.x; general-purpose at base-mid ISO | Overshoot on noisy high-ISO files | darktable `rcd.c`, RT `rcd_demosaic.cc` (GPL-3) | Yes (darktable port) |
| **AMaZE** (Martinec, Weyrich) | Aliasing-minimising + zipper elimination with Nyquist refinement; "generally the best for low ISO" (RawPedia) | Base ISO detail, AA-filterless sensors; RawTherapee default | Colour overshoot at high ISO; diagonal "interpolation correction" artefacts on some scenes | RT `amaze_demosaic_RT.cc`, darktable `amaze.cc` (GPL-3) | Yes |
| **DCB** (Jacek Gozdz) | AMaZE-like detail with fewer false colours on AA-less sensors | AA-less bodies (most modern 40-60 MP) when AMaZE shows false colour | Slight loss vs AMaZE in detail; noise "chains" at high ISO | RT `dcb_demosaic.cc` (GPL-3) | No |
| **LMMSE** (Zhang & Wu; Llorens' implementation) | Linear MMSE on colour differences with refinement steps; smooth, no maze | **High ISO** (ISO 3200+), the RawPedia and darktable recommendation; pairs with denoise | Softer at base ISO | RT `lmmse_demosaic.cc`, darktable `lmmse.c` (GPL-3) | No |
| **IGV** (Luis Sanz Rodríguez) | Earlier RCD relative; excellent false-colour suppression, soft | High ISO alternative to LMMSE | Soft | RT `igv_demosaic.cc` (GPL-3) | No |
| **VNG4** | Four-colour gradient; very "random" noise, no chains | Noisy files, cameras with green imbalance; flat-area partner in dual demosaic | Washes fine detail | darktable `vng.c` (GPL-3) | Yes |
| **AHD / PPG / bilinear** | Historical / fast | Previews | Zipper (AHD), maze (PPG) | dcraw lineage | Yes |
| **Dual demosaic** (Weyrich) | Detail method (RCD/AMaZE) in textured areas, VNG4 (or bilinear) in flat areas, blended by a local-contrast mask | Best overall for noisy *and* detailed files; RT "AMaZE+VNG4", "RCD+VNG4", darktable "dual" | Mask threshold must be tuned per ISO | RT/darktable (GPL-3) | Yes (RCD/AMaZE + VNG) |
| **Learned (CNN) demosaic** | Adobe *Enhance/Raw Details* (2019): CNN trained on ~1 B patches; claimed up to 30 % resolution gain, fixes false colour/zipper; separate Bayer and X-Trans nets | Fine repetitive detail (fabric, brick, hair) | Needs GPU/NPU; occasional hallucinated texture | No open equivalent of Adobe's; open research nets: **JD3Net** (arXiv 2601.00703, Jan 2026, "state-of-the-art efficiency" fully-conv), **LJDD-Net** (Micromachines 2026, 83 % fewer params than plain conv), B2FNet (NTIRE 2025). darktable-ai `rawdenoise-nind` (UtNet2, GPL-3 weights) *is* a joint demosaic+denoise net and is what we already run | Yes: RawNIND UtNet2 via `tract` (joint demosaic + denoise; its demosaic is the net's own, not AMaZE) |

Practical ranking for "maximum quality" on Bayer, from the darktable/RawTherapee documentation and the pixls.us
expert threads (Weyrich, Sanz Rodríguez, Schwalm):

1. Base–ISO 800: **AMaZE** or **RCD** with *dual* VNG4 in flat areas, capture sharpening on. DCB where AMaZE shows
   false colour (AA-less sensors). Differences between AMaZE and RCD are tiny; RCD is 2-3× faster.
2. ISO 1600–6400: **dual RCD+VNG4** with a lower contrast threshold, or **LMMSE** with 2-4 refinement steps. Profiled
   denoise after.
3. ISO 6400+: **LMMSE** (or VNG4) + denoise, or a joint net (RawNIND UtNet2 / Lightroom Denoise class).

Learned joint demosaic-denoise is what Lightroom *Denoise* (2023, Gharbi) and DxO DeepPRIME (XD2s) do: one network
replaces demosaic + denoise, which is why they look "cleaner than possible" at ISO 12800. The open equivalent today is
darktable 5.6's neural restore with `rawdenoise-nind` — the model we already ship.

### 4.2 Demosaic (X-Trans)

- **Markesteijn 1-pass / 3-pass** (Frank Markesteijn; dcraw → darktable `xtrans.c`/RT `xtrans_demosaic.cc`, GPL-3)
  is the reference. 3-pass is "generally the best" (RawPedia), visibly sharper at low ISO, 2-3× slower; 1-pass is
  darktable's default. Known: slight highlight overshoot and a cross-hatch pattern on some noisy files.
- **Dual (Markesteijn + VNG/fast)**: RT "4-pass" = Markesteijn 3-pass in detail, fast X-Trans in flat areas,
  contrast-threshold blended; darktable dual works with Markesteijn 3-pass too.
- **Capture One** has the strongest reputation for X-Trans (no "worms"); Lightroom's old worm artefacts came from its
  sharpening on top of its own X-Trans demosaic, largely fixed by Enhance Details' X-Trans net.
- rawler 0.8.0 added a Markesteijn port (Rust, LGPL-2.1) — a second Rust reference to compare against.
- **We have**: our own edge-weighted interpolation only. This is the clearest demosaic gap: a Markesteijn 3-pass port
  (plus dual) is the standard and the owner's X-Trans output (if he shoots Fuji) is below RT/darktable today.

### 4.3 Highlight reconstruction

| Method | Idea | Strength | Weakness | Source | We have |
|---|---|---|---|---|---|
| Clip / `clip_neutral` | Clamp all channels to the lowest white-balanced clip | Never wrong colour | Loses everything above clip | – | Yes |
| **Inpaint opposed** (Schwalm, G'MIC team; darktable 4.2, RT 5.10) | Clipped channel = cube-root mean of the two others + global chrominance offset measured around the clipped region | Fast, "right for most images", darktable default; RT 5.10 calls it more reliable than colour propagation | Fails where clipped area borders a different colour | darktable `opposed.c`, RT `hilite_recon.cc` (GPL-3) | Yes |
| **Segmentation based** (Schwalm, darktable 4.4+) | Each clipped segment gets its own colour from valid ring pixels; candidate rejection; falls back to opposed; recovery modes for all-clipped areas | Large blown areas (skies, windows in architecture), interior lights | Slower, no OpenCL; can produce patterns/magenta if threshold too high | darktable `segbased.c` (GPL-3) | Yes (v2 full port) |
| **Guided laplacians** (A. Pierre, darktable 4.0) | Multi-scale guided-filter inpainting: copies detail from valid channels into clipped ones, propagates gradients | Specular highlights, spotlights, smooth gradients (sun, chrome, jewellery, catchlights in eyes) | Very slow, Bayer only, "favours smooth blur over detail" | darktable `laplacian.c` (GPL-3) | No |
| **Colour propagation** (RawTherapee) | Bleed surrounding colour into the clipped area, then reconstruct luminance from the unclipped channels | Small clipped regions | Can bleed wrong colours; image-dependent | RT `hilite_recon.cc` (GPL-3) | No |
| **Our `reconstruct`** | Local chromaticity diffusion (normalised convolution) | – | – | ours | Yes |
| LibRaw `highlight` modes 0-9 | Clip / unclip / blend / rebuild (dcraw's) | Baseline | Old | LibRaw (LGPL) | – |

Consensus from the darktable manual, RawPedia and the long pixls.us "darktable vs RawTherapee highlight
reconstruction" thread: there is no universal winner; *opposed* is the default, *segmentation* for big blown areas,
*guided laplacians* for small bright speculars. For headshots the relevant case is catchlights, specular skin and
backlit hair; for architecture it is windows and lamps; for events it is flash on white shirts and stage lights.
We cover 2 of the 3 darktable modes. The remaining gap is small (guided laplacians), the larger lever is **correct
per-channel white levels** (section 3), without which every method mis-detects clipping.

### 4.4 Denoise

| Approach | What it is | Quality tier | Source | We have |
|---|---|---|---|---|
| **Profiled wavelet/NLM** (darktable *denoise (profiled)*) | Poisson-Gaussian variance-stabilising transform from `noiseprofiles.json`, then NLM or wavelets in Y0U0V0 | Very good for a classical method; needs the profile | darktable `denoiseprofile.c` (GPL-3) | Algorithm yes; profiles no |
| **RT wavelet + "Capture sharpening pre-sharpen denoise"** | Per-image noise estimation, L*a*b* wavelet levels, median; 5.13 adds a denoise step before deconvolution | Good; no profile needed | RT `FTblockDN.cc`, `capturesharpening.cc` (GPL-3) | No |
| **RawNIND UtNet2** (Brummer & De Vleeschouwer 2025, darktable-ai) | U-Net trained on real paired raw noise across sensors; Bayer in → RGB out (joint) or linear Rec.2020 in/out | Beats classical methods on RawNIND test scenes; competitive with commercial "AI denoise" for mainstream bodies; trained on ~6 camera families so generalisation to unseen sensors is the open question | darktable-ai (GPL-3 weights, ONNX) | Yes via `tract` |
| **NIND UNet / NAFNet (SIDD)** | Display-RGB denoisers | NAFNet is the stronger architecture on SIDD; NIND trained on real DSLR noise | darktable-ai (GPL-3; NAFNet weights MIT upstream) | NIND yes; NAFNet no |
| **Lightroom Denoise (2023)** | Joint demosaic+denoise CNN; "usable at ISO 51200 on 20 MP"; non-destructive since June 2025 | Commercial tier; reviewers rate it ≈ DxO DeepPRIME for most photos, slightly behind XD2s on extreme fur/feathers; "plasticky" at high strength | closed | – |
| **DxO DeepPRIME 3 / XD2s** | Joint demosaic+denoise+CA; vendor claims ~3 stops | Commercial top tier in 2025-26 reviews | closed | – |
| **Topaz Photo AI RAW Denoise** | Several models; more control; faster than LR | Commercial | closed | – |
| **Capture One** | Classical NR + "AI" since 16.3; reviewers (Dec 2025) say LR AI denoise beats it by a wide margin | – | closed | – |

Bottom line: our stack (profiled wavelets + RawNIND net) is the open state of the art as of darktable 5.6. What is
missing is the *data* (noise profiles) and a per-image noise estimator for unprofiled bodies, plus, later, a stronger
raw net (JD3Net-class) trained on RawNIND + SIDD + the owner's own bodies.

### 4.5 How the big four compare, in short

- **Lightroom / ACR**: best colour *data* (Adobe Standard/Camera Matching DCPs, dual/triple illuminant), ML demosaic
  (Enhance) and ML denoise; classical demosaic is unremarkable; lens corrections from embedded data + LCP.
- **Capture One**: best reputation for X-Trans and skin tones (ICC camera profiles, per-camera "curves"); strong
  classical sharpening; denoise now behind Adobe.
- **darktable 5.6**: open state of the art in algorithms (RCD/AMaZE/LMMSE/dual, three highlight methods, profiled
  denoise, neural restore, capture sharpening); colour data = Adobe matrices + optional ICC; embedded lens data
  for Sony/Fuji only.
- **RawTherapee 5.13**: best open *camera data* (camconst.json), most demosaic choices (incl. DCB/IGV/LMMSE,
  4-pass X-Trans), pixel-shift, DCP/LCP readers, PDAF/line-noise filters; LibRaw-based decoding since 5.11.

---

## 5. Prioritized recommendations

Rules applied: pure Rust in the build (C/C++ only as reference or as an *external* validation tool run from a
script, never linked); libraries kept whole; quality first, speed second. "Gain" is image quality for the owner's
work (headshots, architecture, events). Effort: S < 1 week, M 1-3 weeks, L > 3 weeks of one builder.

| # | Recommendation | Expected quality gain | Effort | Risk | How to validate |
|---|---|---|---|---|---|
| 1 | **Import the open per-camera data files as data**: RawTherapee `camconst.json` (per-ISO/per-channel white levels, `white_max`, `aperture_scaling`, `raw_crop`, `masked_areas`, `pdaf_pattern`), rawspeed `cameras.xml` (per-ISO black/white, `BlackAreas`, `Crop`, matrices, aliases), darktable `noiseprofiles.json` and `wb_presets.json`. Generate TOML/Rust tables with a script like the existing `scripts/engine_colour_data.py`, record the upstream commit in `docs/PORTS.md`, and keep the files verbatim so updates are a re-run. Use measured black areas per frame when present, per-ISO tables otherwise, and `saturation.rs` as the last resort. | **High.** Fixes the two most common raw-quality bugs in the wild: magenta/cyan highlights (wrong white per channel/ISO/aperture) and green/magenta shadow casts (wrong black per ISO). Makes profiled denoise actually profiled. Adds named WB presets. | M | Low (data only; CC BY-SA 3.0 / GPL-3, attribution) | (a) For every raw.pixls.us sample: compare the table's white level with `saturation.rs`'s detected pile-up; disagreements > 2 % flag either a table error or a detector bug. (b) Render 20 deliberately overexposed CC0 samples (sky, white shirt, lamp) and count pixels whose hue after reconstruction is magenta/cyan (`hue ∈ [280°,340°]` with chroma > 0.1); must be ~0 after. (c) Black: mean of optical-black area vs table; shadows of a pushed +4 EV dark frame must be neutral (|a*|,|b*| < 1). |
| 2 | **Build the ours-vs-rawler-vs-reference decoder harness** before choosing primaries. For each CC0 file: decode with lc-raw and rawler (both Rust, in-process) and with `darktable-cli`/`rawtherapee-cli`/`dcraw_emu -4 -T -o 0` (external, GPL/LGPL tools run from a script, outputs cached as 16-bit linear TIFF + JSON of black/white/WB/crop/CFA). Compare raw sample arrays bit-exactly where the pipelines allow, else max-abs and histogram distance; compare metadata fields exactly. Fallback rule in the app: ours → rawler → embedded preview, chosen per (format, compression) from the harness, not by hand. | **High** for correctness confidence; catches silent errors (wrong CFA phase, off-by-one crop, wrong black on one body) that no unit test finds. | M | Low | Metrics: % of files bit-exact to rawspeed after black subtraction; max abs sample diff; metadata mismatch list. Target: 100 % bit-exact on uncompressed/LJPEG paths, documented justified differences elsewhere. Keep the result table in `docs/wip/` and re-run on every decoder change. |
| 3 | **Colour profiles: (a) import Adobe DCP/LCP from the user's own Adobe DNG Converter / Camera Raw install** ("find Adobe profiles" button; never bundle), (b) add **triple-illuminant** (`ColorMatrix3`, `ProfileHueSatMapData3`) to `lc-color/profile.rs`, (c) ship a `dcamprof`-based recipe (external GPL-3 C tool, offline) so the owner can make dual-illuminant DCPs for *his* bodies from a ColorChecker under tungsten and daylight, and bundle *those* (his own, CC0). | **High for skin and brand colours**; this is the single biggest visible difference between an open pipeline and Lightroom/C1. Triple-illuminant: small but real on 2023+ bodies. | S (import) + S (triple) + S (recipe/doc) | Low; licence risk only if we bundle Adobe files (we won't) | ΔE2000 on a ColorChecker SG shot per body, measured against the chart's reference Lab (ArgyllCMS `scanin` can read the patches from our TIFF export): target mean ΔE < 2.5, max < 6, with no patch worse than the Adobe Standard render of the same DNG. Compare skin patches specifically. |
| 4 | **Port Markesteijn 3-pass + dual (Markesteijn/VNG) for X-Trans** from darktable `xtrans.c` (GPL-3), using the same fixture method as the AMaZE/RCD ports (scalar reference vectors from the pinned darktable tree, 2e-6 tolerance). Keep our edge-weighted method as the fast/preview path. Compare against rawler 0.8.0's Rust Markesteijn as a second reference. | **High if the owner shoots Fujifilm**, otherwise moderate (X-Trans users of the app). Our current X-Trans output is below darktable/RT. | M | Low (well-trodden port) | Fixtures bit-match; then CPSNR vs darktable-cli output on 10 CC0 X-T5/X-H2/GFX files; visual check for worms/cross-hatch at 200 %. |
| 5 | **Add LMMSE (and DCB) demosaic and an automatic method choice** by ISO/noise estimate: AMaZE/RCD+VNG dual at low ISO, dual with lower threshold mid, LMMSE high. Port from darktable `lmmse.c` and RT `dcb_demosaic.cc` (GPL-3). | **Medium**: cleaner high-ISO event files (no AMaZE overshoot), fewer false colours on AA-less bodies with DCB. | S-M | Low | Re-mosaic benchmark: take 16-bit linear RGB (our own AMaZE output of base-ISO CC0 files, or Kodak/McMaster), add Poisson-Gaussian noise from a real `noiseprofiles.json` entry, re-mosaic, demosaic, measure CPSNR/SSIM against the clean RGB per method per ISO; pick the switch points from the curves. Note the sRGB-benchmark caveat (Kodak/McMaster are 8-bit sRGB re-mosaics); prefer the linear-raw-derived set. |
| 6 | **Embedded lens corrections** (Sony and Fujifilm first, as darktable `lens.cc` documents them; then Panasonic RW2 tag `0x0119` which our docs already note as unused; DNG opcodes already work). Keep lensfun as fallback and let the user pick. | **High for architecture** (distortion on day-one lenses, no lensfun wait) and fast-prime vignetting in headshots. | M | Medium (reverse-engineered formats; darktable's own path has had "latest algorithm" revisions) | Geometry: overlay our corrected render on the camera's embedded JPEG (already distortion-corrected) and measure edge displacement with a phase-correlation on 9 tiles: < 1 px. Vignetting: flat-field frame at f/1.4 must be within 3 % corner-to-centre after correction. |
| 7 | **Per-image noise estimation fallback + darktable profiles** (from #1): port RT's wavelet noise estimator (`FTblockDN.cc` median-of-finest-level) so unprofiled bodies still get a sane `a,b`; expose "profile: measured / estimated". | Medium (denoise works for every body from day one) | S | Low | On profiled bodies, estimated `a,b` within 20 % of the table; on RawNIND pairs, PSNR of profiled denoise with estimated vs tabled profile within 0.3 dB. |
| 8 | **PDAF lines filter, line-noise filter, green equilibration** from RawTherapee (`pdaflinesfilter.cc`, `rawimagesource.cc`), driven by `pdaf_pattern` data from #1. | Medium for Sony/Nikon event work at ISO 6400+ with pushed shadows; prevents AMaZE maze artefacts on green-imbalanced sensors. | S | Low | Row-periodicity metric: FFT of row means of a dark pushed frame; the PDAF-spacing peak must drop > 20 dB; no change on frames without PDAF rows. |
| 9 | **Guided laplacians highlight mode** (darktable `laplacian.c`, GPL-3) as a third option for speculars/catchlights. | Small-medium (speculars in headshots, chrome/glass in architecture). | M | Low | Synthetic: take a bracketed CC0 HDR merge, clip it at a lower white, reconstruct, measure PSNR in the clipped mask vs the true HDR for opposed / segmentation / laplacian; keep the best-per-case rule in the auto mode. |
| 10 | **Learned raw restoration, next step**: add darktable-ai `denoise-nafnet` (stronger RGB denoiser) now; later train a JD3Net-class joint demosaic+denoise net (small, fully convolutional, ONNX-friendly) on RawNIND + SIDD + the owner's own bodies, exported through `tract`. Treat Lightroom *Denoise* / DeepPRIME as the target. | High at ISO 6400+ (events); the only way to match commercial "AI denoise". | L | Medium (training infra, generalisation to unseen sensors) | RawNIND held-out scenes (incl. the unseen-sensor Canon M100 split) PSNR/SSIM vs UtNet2; SIDD validation; blind A/B by the owner on 10 of his own high-ISO frames. |
| 11 | **Pixel-shift merge** (RawTherapee `pixelshift.cc` motion-detecting Pentax merge; Nikon NEFX per LibRaw 0.22 as reference) | Medium only if the owner has such bodies; architecture benefits most. | M | Medium | Resolution: slanted-edge MTF50 of merged vs single frame (+30 % expected); motion mask visual check. |
| 12 | **User bad-pixel map + dark frame / flat field** (LibRaw/RT `.badpixels` text format; RT dark-frame subtraction) | Small; long exposures and ageing sensors. | S | Low | Hot-pixel count on a 30 s lens-cap frame before/after. |
| 13 | **Unsupported-format policy**: Nikon HE/HE* (TicoRAW; no open decoder, intoPIX SDK licence) and Fuji lossy RAF: open the embedded preview, show a clear notice, recommend lossless in camera. Don't spend builder time here. Apple ProRAW / JXL DNG: route to rawler (jxl-oxide, pure Rust). | Avoids wasted effort; clear UX | S | – | Corpus test marks these as `KNOWN_UNSUPPORTED` with the reason string shown in the UI. |

Not recommended:

- Linking LibRaw, rawspeed or the Adobe DNG SDK (C++; violates pure-Rust). Use them only as external reference
  renderers in the validation harness.
- Replacing lc-raw by rawler wholesale: rawler's per-camera data is thinner than camconst/cameras.xml, its demosaic
  is basic, and we already have the stricter correctness path for the mainstream formats. Keep both, pick per format
  from measurements (#2).
- zenraw (AGPL-3-only; wrapper over rawler), quickraw/rawloader/imagepipe (unmaintained), libopenraw-rs (alpha,
  thumbnail-oriented).
- Bundling Adobe DCP/LCP/DNG-Converter output.

### Test corpora and metrics (shared by the items above)

| Corpus | What | Licence | Use |
|---|---|---|---|
| **raw.pixls.us** (`rsync -rLv rsync://raw.pixls.us/data/`, ~22 GB+) | 1 sample per body/mode, ~800+ bodies | mostly CC0 (check per file) | Decoder bit-exactness (#2), white/black checks (#1), X-Trans/Bayer visual sets (#4, #5) — already wired via `cargo xtask corpus --download` |
| **RawNIND** (UCLouvain Dataverse doi:10.14428/DVN/DEQCIM) | Paired noisy/clean *raw* scenes, 6+ cameras, unseen-sensor split | open (check Dataverse terms) | Denoise/joint-net PSNR/SSIM (#7, #10) |
| **SIDD** (smartphone, raw + sRGB) and **DND** (50 DSLR pairs) | Real-noise benchmarks | research use | NAFNet/JDD validation (#10) |
| **Kodak (24) / McMaster (18) / MIT Moiré (1000 patches)** | Classic demosaic benchmarks (8-bit sRGB re-mosaic; moiré-prone set) | research use | Demosaic CPSNR, zipper/moiré sanity (#4, #5); caveat: not raw-domain |
| **MIT-Adobe FiveK** (5,000 DNGs + 5 expert retouches, ~47 GB) | Real DNGs with expert targets | research licence (check) | Default-render / look tuning, not decoding |
| **darktable-ai samples** (`samples/rawdenoise/bayer`) | High-ISO ARW/CR2 used for the neural restore demo | GPL-3 repo | Already used in our AI-denoise report |
| **Owner's own shots** | ColorChecker SG per body under tungsten/daylight/flash; lens-cap 30 s; flat frame at f/1.4; ETTR sky | private | Colour (#3), noise (#7), vignetting (#6), highlights (#1) |
| **Reference renders** | `darktable-cli`, `rawtherapee-cli`, LibRaw `dcraw_emu` run externally; Adobe DNG Converter for colour tags | GPL/LGPL/Adobe (external tools) | Third opinion in #2, colour tags in #3 |

Metrics: bit-exact sample match (decoders); CPSNR/SSIM on linear data (demosaic/denoise); ΔE2000 on chart patches
(colour); hue histogram of clipped regions (highlights); row-FFT peak (PDAF); edge displacement px and corner
falloff % (lens); MTF50 (pixel-shift, capture sharpening).

---

## 6. Sources

Decoders and releases

- LibRaw 0.22 release notes: https://www.libraw.org/node/2861 ; changelog mirror: https://third-party-mirror.googlesource.com/libraw/+/refs/heads/master/Changelog.txt ; licence text: https://third-party-mirror.googlesource.com/libraw/+/refs/heads/master/LICENSE ; licensing FAQ: https://www.libraw.org/node/2012 ; rawspeed-vs-LibRaw speed comments: https://www.libraw.org/node/2105 , https://libraw.org/node/2169 ; CRX decoder header (LGPL/CDDL): https://third-party-mirror.googlesource.com/libraw/+/refs/heads/master/src/decoders/crx.cpp
- rawspeed: https://github.com/darktable-org/rawspeed ; `cameras.xml` (CC BY-SA 3.0): https://raw.githubusercontent.com/darktable-org/rawspeed/develop/data/cameras.xml ; schema notes: https://rawstudio.org/blog/?p=816 ; per-ISO discussion: https://discuss.pixls.us/t/camera-black-white-level-iso-dependency/45386 , https://discuss.pixls.us/t/measure-black-and-white-levels/32082 ; JXL issue: https://github.com/darktable-org/rawspeed/issues/516
- darktable releases: 5.4.0 https://www.darktable.org/2025/12/darktable-5.4.0-released/ ; 5.4.1 https://www.darktable.org/2026/02/darktable-5.4.1-released/ ; 5.6.0 https://www.darktable.org/2026/06/darktable-5.6.0-released/ and https://github.com/darktable-org/darktable/releases/tag/release-5.6.0 ; 5.6.1 https://www.darktable.org/2026/08/darktable-5.6.1-released/ ; camera support table: https://www.darktable.org/resources/camera-support/
- RawTherapee 5.13: https://www.rawtherapee.com/downloads/5.13/ , https://discuss.pixls.us/t/rawtherapee-5-13-released/59653 ; LibRaw switch (5.11/5.12): https://en.wikipedia.org/wiki/RawTherapee , https://discuss.pixls.us/t/rawtherapee-5-11-released/45338 ; 5.10 notes (inpaint opposed): https://github.com/Beep6581/RawTherapee/blob/5.10/RELEASE_NOTES.txt ; `camconst.json`: https://raw.githubusercontent.com/Beep6581/RawTherapee/dev/rtengine/camconst.json ; RawPedia on adding cameras: https://rawpedia.rawtherapee.com/Adding_Support_for_New_Raw_Formats
- dnglab / rawler: releases https://github.com/dnglab/dnglab/releases ; supported cameras https://github.com/dnglab/dnglab/blob/main/SUPPORTED_CAMERAS.md ; `rawler/Cargo.toml` (0.8.0, LGPL-2.1, jxl-oxide 0.12.2) https://raw.githubusercontent.com/dnglab/dnglab/main/rawler/Cargo.toml ; crate docs https://docs.rs/rawler ; JXL DNG discussion https://discuss.pixls.us/t/compressed-dng-jpeg-xl-support/56423
- rawloader: https://github.com/pedrocr/rawloader , https://docs.rs/crate/rawloader/0.37.2 ; imagepipe: https://docs.rs/crate/imagepipe/0.5.1
- quickraw: https://lib.rs/crates/quickraw , https://crates-io.ganymede-static.progval.net/crates/quickraw/0.2.1-alpha.1
- zenraw (AGPL-3/commercial): https://docs.rs/zenraw , https://github.com/imazen/zenraw
- libopenraw: https://libopenraw.freedesktop.org/ , https://libopenraw.pages.freedesktop.org/libopenraw/ , dev log https://figuiere.net/hub/wlog/dev-log-march-2025 , CR3 notes https://libopenraw.pages.freedesktop.org/formats/cr3
- rsraw (LibRaw bindings): https://docs.rs/crate/rsraw/0.1.1
- Adobe DNG SDK / spec: https://helpx.adobe.com/camera-raw/desktop/dng-and-file-formats/digital-negative.html ; licence discussion https://community.adobe.com/questions-563/adobe-dng-sdk-terms-of-use-licensing-182173 ; digiKam's licence copy https://lxr.kde.org/source/graphics/digikam/core/libs/dngwriter/extra/COPYING-ADOBE-DNGSDK ; DNG 1.6/1.7 features https://loc.gov/preservation/digital/formats/fdd/fdd000628.shtml , https://cloudinary.com/blog/samsung-now-supports-dng-1-7-including-jpeg-xl ; ProfileGainTableMap https://libraw.org/node/2786
- jxl-oxide: https://github.com/tirr-c/jxl-oxide , https://docs.rs/crate/jxl-oxide/latest
- Halide / HDR+: https://github.com/AugustinJose1221/hdr-plus , https://ipol.im/pub/art/2021/336 , https://github.com/amonod/hdrplus-python
- Canon CR3 reverse engineering: https://github.com/neuroradiology/canon_cr3 (mirror of lclevy/canon_cr3), https://discuss.pixls.us/t/canon-cr3-raw-support-your-move/14192
- Nikon HE/HE* (TicoRAW): https://discuss.pixls.us/t/nikon-he-decoder/56955 , https://discuss.pixls.us/t/nikon-z-series-high-efficiency-raw-support/50428 , https://www.libraw.org/node/2766 , https://www.design-reuse.com/news/51061/intopix-ticoraw-nikon-z-9-mirrorless-camera.html

Quality drivers and data

- darktable `noiseprofiles.json`: https://raw.githubusercontent.com/darktable-org/darktable/master/data/noiseprofiles.json ; how profiles are made: https://pixls.us/articles/how-to-create-camera-noise-profiles-for-darktable/ , https://discuss.pixls.us/t/contribute-to-denoise-profiles/46123
- darktable white balance presets: https://github.com/darktable-org/darktable/wiki/White-balance-presets ; copyright (wb_presets.c GPL-2+): https://metadata.ftp-master.debian.org/changelogs/main/d/darktable/stable_copyright
- darktable lens correction (embedded metadata, Sony/Fuji): https://docs.darktable.org/usermanual/5.6/en/module-reference/processing-modules/lens-correction/ , https://discuss.pixls.us/t/embedded-metadata-lens-correction/49649 , https://discuss.pixls.us/t/manufacturer-lens-correction-module-for-darktable/21543
- lensfun status: https://discuss.pixls.us/t/lensfun-the-past-the-current-state-and-the-future/11470 , https://packages.fedoraproject.org/pkgs/lensfun/lensfun-devel/index.html
- Adobe DCP licensing: https://lists.freedesktop.org/archives/openicc/2005q2/000387.html , https://discuss.pixls.us/t/camicc-converting-adobes-dcp-profiles-to-icc-for-darktable/59588 , https://community.adobe.com/questions-563/copyright-cameraprofiles-dcp-files-180967 ; where Adobe installs them / RT usage: https://rawpedia.rawtherapee.com/How_to_get_LCP_and_DCP_profiles/de , https://rawpedia.rawtherapee.com/Color_Management
- dcamprof (GPL-3): https://github.com/Beep6581/dcamprof , docs mirror https://rawtherapee.com/mirror/dcamprof/dcamprof.html , tutorial https://ludd.ltu.se/~torger/photography/camera-profiling.html
- Camera spectral sensitivities: camspec https://discuss.pixls.us/t/camera-spectral-sensitivity-data-camspec/20354 ; Zhao et al. DB https://zenodo.org/records/4297288 ; Open Film Tools https://www.hdm-stuttgart.de/open-film-tools/en/camera_responsivities
- Triple-illuminant support status: https://discuss.pixls.us/t/does-rawtherapee-support-triple-illuminant-dcp/58898 , https://alicevision.readthedocs.io/en/stable/classaliceVision_1_1image_1_1DCPProfile.html
- PDAF / line noise / preprocessing (RawTherapee): https://rawpedia.pixls.us/preprocessing/ , https://discuss.pixls.us/t/how-do-i-fix-sony-a7iii-pdaf-striping-using-rawtherapee/10223 , https://blog.kasson.com/a7riii/pdaf-striping-sony-a7riii
- Pixel shift: https://pixls.us/articles/rawtherapee-and-pentax-pixel-shift/ , https://discuss.pixls.us/t/support-for-pentax-pixel-shift-files-3489/2560?page=4 , https://rawtherapee.com/downloads/5.1/ , https://www.slrlounge.com/sonypixelshift2dng-makes-sony-pixel-shift-more-practical-gets-the-files-into-your-favorite-processor/ , https://discuss.pixls.us/t/open-source-pixel-shift-merge/54586

Demosaic / highlights / denoise

- darktable demosaic manual (RCD default, LMMSE for high ISO, dual, Markesteijn): https://docs.darktable.org/usermanual/5.6/en/module-reference/processing-modules/demosaic/
- RawPedia demosaicing (AMaZE/DCB/LMMSE/IGV/VNG4, X-Trans 1/3/4-pass): https://rawpedia.pixls.us/demosaicing/
- pixls threads: AMaZE diagonal artefacts https://discuss.pixls.us/t/diagonal-interpolation-correction-artifacts-with-amaze-demosaicing/3338 ; high-ISO demosaic https://discuss.pixls.us/t/what-is-the-best-high-iso-bayer-demosaicer/24800 ; Markesteijn 3 artefacts https://discuss.pixls.us/t/x-trans-demosaic-markesteijn-3-artifacts/14427 ; choosing a demosaicer https://discuss.pixls.us/t/how-to-choose-a-demosaicing-algorithm/30556
- darktable highlight reconstruction manual: https://docs.darktable.org/usermanual/5.6/en/module-reference/processing-modules/highlight-reconstruction/ ; darktable vs RT thread https://discuss.pixls.us/t/highlight-reconstruction-darktable-vs-rawtherapee/31790 ; RT methods https://rawpedia.rawtherapee.com/Exposure
- darktable 5.6 AI: overview https://docs.darktable.org/usermanual/5.6/en/special-topics/ai/overview/ ; how it works https://docs.darktable.org/usermanual/5.6/en/special-topics/ai/how-ai-works/ ; neural restore https://docs.darktable.org/usermanual/5.6/en/module-reference/utility-modules/shared/neural-restore/ ; models repo https://github.com/darktable-org/darktable-ai ; announcement https://discuss.pixls.us/t/meet-the-new-ai-tools-in-darktable-5-6/58819
- RawNIND paper: https://arxiv.org/abs/2501.08924 ; NIND: https://openaccess.thecvf.com/content_CVPRW_2019/papers/NTIRE/Brummer_Natural_Image_Noise_Dataset_CVPRW_2019_paper.pdf ; nind-denoise https://github.com/trougnouf/nind-denoise
- Learned JDD: JD3Net https://arxiv.org/html/2601.00703v1 ; LJDD-Net https://www.ncbi.nlm.nih.gov/pmc/articles/PMC12844236/ ; NTIRE 2025 RAIM https://arxiv.org/pdf/2506.01394 ; Ehret et al. https://openaccess.thecvf.com/content_ICCV_2019/papers/Ehret_Joint_Demosaicking_and_Denoising_by_Fine-Tuning_of_Bursts_of_Raw_ICCV_2019_paper.pdf ; Bayer/Quad/Nona JDD https://arxiv.org/html/2504.07145v2 ; moiré-free demosaicking https://arxiv.org/pdf/2305.08585
- Adobe Enhance Details / Denoise: https://blog.adobe.com/en/publish/2019/02/12/enhance-details , https://blog.adobe.com/en/publish/2023/04/18/denoise-demystified
- DxO DeepPRIME: https://www.dxo.com/deepprime ; comparisons https://photofocus.com/photography/topaz-photo-ai-v2-vs-lightroom-noise-reduction-showdown/ , https://www.diyphotography.net/?p=312138 , https://discuss.pixls.us/t/denoise-comparison-darktable-on1-and-topaz/40563
- Capture One vs Lightroom vs darktable (anecdotal): https://www.digitalcameraworld.com/buying-guides/lightroom-vs-capture-one , https://discuss.pixls.us/t/captureone-vs-darktable/42831/2 , https://photographylife.com/switching-from-lightroom-to-capture-one-pro

Corpora

- raw.pixls.us: https://raw.pixls.us/ ; rsync/download notes https://discuss.pixls.us/t/raw-pixls-us-zip-download-issues/13197/8 ; licence caveat https://discuss.pixls.us/t/can-i-use-some-raw-files-from-raw-pixls-us-to-make-a-web-app-demo-video/44619
- SIDD / DND overview: https://arxiv.org/pdf/2308.00247 ; MIT-Adobe FiveK: https://huggingface.co/datasets/yangkun2197/MIT-Adobe-FiveK , https://paperswithcode.com/dataset/mit-adobe-fivek
- Kodak / McMaster / MIT moiré benchmark use: https://arxiv.org/pdf/2305.08585 , https://arxiv.org/pdf/2009.06205
