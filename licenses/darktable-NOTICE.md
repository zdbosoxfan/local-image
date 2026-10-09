# darktable: ported code and notices

Local Image is GPL-3.0-or-later (see `LICENSE`). Parts of its develop module are Rust ports of
algorithms from **darktable** — <https://github.com/darktable-org/darktable> — which is licensed
under the GNU General Public License, version 3 or (at your option) any later version. No darktable
C code is compiled or shipped: the math was re-implemented in Rust, and each ported file names its
upstream source in its module documentation. The project-wide list of ports is
[`docs/PORTS.md`](../docs/PORTS.md).

## Ported files

| Our file | Upstream file | Upstream commit | Copyright | Licence |
|---|---|---|---|---|
| `crates/lc-pipeline/src/negative.rs` | [`src/iop/negadoctor.c`](https://github.com/darktable-org/darktable/blob/733bd69f32cac7ff5e41025115942772add1f088/src/iop/negadoctor.c) | `733bd69f32cac7ff5e41025115942772add1f088` | Copyright (C) 2020-2026 darktable developers | GPL-3.0-or-later |
| `crates/lc-raw/src/demosaic/vng.rs` | [`src/iop/demosaicing/vng.c`](https://github.com/darktable-org/darktable/blob/733bd69f32cac7ff5e41025115942772add1f088/src/iop/demosaicing/vng.c) | `733bd69f32cac7ff5e41025115942772add1f088` | Copyright (C) 2010-2026 darktable developers; dcraw VNG by Dave Coffin | GPL-3.0-or-later |
| `crates/lc-raw/src/demosaic/vng.rs` | [`src/iop/demosaicing/basics.c`](https://github.com/darktable-org/darktable/blob/733bd69f32cac7ff5e41025115942772add1f088/src/iop/demosaicing/basics.c) | `733bd69f32cac7ff5e41025115942772add1f088` | Copyright (C) 2010-2026 darktable developers (median colour smoothing) | GPL-3.0-or-later |
| `crates/lc-raw/src/demosaic/amaze.rs` | [`src/iop/demosaicing/amaze.cc`](https://github.com/darktable-org/darktable/blob/733bd69f32cac7ff5e41025115942772add1f088/src/iop/demosaicing/amaze.cc) | `733bd69f32cac7ff5e41025115942772add1f088` | Copyright (C) 2011-2024 darktable developers; 2008-2010 Emil Martinec; Ingo Weyrich (optimization); ideas of Luis Sanz Rodrigues and Paul Lee | GPL-3.0-or-later |
| `crates/lc-raw/src/highlight/segbased.rs` | [`src/iop/hlreconstruct/segbased.c`](https://github.com/darktable-org/darktable/blob/733bd69f32cac7ff5e41025115942772add1f088/src/iop/hlreconstruct/segbased.c) | `733bd69f32cac7ff5e41025115942772add1f088` | Copyright (C) 2022-2026 darktable developers; Hanno Schwalm | GPL-3.0-or-later |
| `crates/lc-raw/src/highlight/segmentation.rs` | [`src/iop/hlreconstruct/segmentation.c`](https://github.com/darktable-org/darktable/blob/733bd69f32cac7ff5e41025115942772add1f088/src/iop/hlreconstruct/segmentation.c) | `733bd69f32cac7ff5e41025115942772add1f088` | Copyright (C) 2022-2026 darktable developers; Hanno Schwalm | GPL-3.0-or-later |
| `crates/lc-raw/src/highlight/segbased.rs` | [`src/common/distance_transform.c`](https://github.com/darktable-org/darktable/blob/733bd69f32cac7ff5e41025115942772add1f088/src/common/distance_transform.c) | `733bd69f32cac7ff5e41025115942772add1f088` | Copyright (C) 2022-2026 darktable developers; original Copyright (C) 2006 Pedro Felzenszwalb, GPL-2.0-or-later; Pedro F. Felzenszwalb and Daniel P. Huttenlocher | GPL-3.0-or-later |
| `crates/lc-raw/src/highlight/segbased.rs` | [`src/common/box_filters.cc`](https://github.com/darktable-org/darktable/blob/733bd69f32cac7ff5e41025115942772add1f088/src/common/box_filters.cc) | `733bd69f32cac7ff5e41025115942772add1f088` | Copyright (C) 2009-2026 darktable developers | GPL-3.0-or-later |
| `crates/lc-raw/src/highlight/segbased.rs` | [`src/develop/noise_generator.h`](https://github.com/darktable-org/darktable/blob/733bd69f32cac7ff5e41025115942772add1f088/src/develop/noise_generator.h) | `733bd69f32cac7ff5e41025115942772add1f088` | Copyright (C) 2020-2023 darktable developers | GPL-3.0-or-later |
| `crates/lc-raw/src/numerics.rs` | [`src/common/gaussian.c`](https://github.com/darktable-org/darktable/blob/733bd69f32cac7ff5e41025115942772add1f088/src/common/gaussian.c) | `733bd69f32cac7ff5e41025115942772add1f088` | Copyright (C) 2012-2026 darktable developers | GPL-3.0-or-later |
| `crates/lc-raw/src/numerics.rs` | [`src/common/math.h`](https://github.com/darktable-org/darktable/blob/733bd69f32cac7ff5e41025115942772add1f088/src/common/math.h) | `733bd69f32cac7ff5e41025115942772add1f088` | Copyright (C) 2018-2025 darktable developers | GPL-3.0-or-later |
| `crates/lc-raw/src/demosaic/rcd.rs` | [`src/iop/demosaicing/rcd.c`](https://github.com/darktable-org/darktable/blob/733bd69f32cac7ff5e41025115942772add1f088/src/iop/demosaicing/rcd.c) | `733bd69f32cac7ff5e41025115942772add1f088` | Copyright (C) 2010-2026 darktable developers; RCD by Luis Sanz Rodríguez; tiling by Ingo Weyrich (RawTherapee); Hanno Schwalm | GPL-3.0-or-later |
| `crates/lc-raw/src/demosaic/dual.rs` | [`src/iop/demosaicing/dual.c`](https://github.com/darktable-org/darktable/blob/733bd69f32cac7ff5e41025115942772add1f088/src/iop/demosaicing/dual.c), [`src/develop/masks/detail.c`](https://github.com/darktable-org/darktable/blob/733bd69f32cac7ff5e41025115942772add1f088/src/develop/masks/detail.c) | `733bd69f32cac7ff5e41025115942772add1f088` | Copyright (C) 2010-2025 / 2013-2025 darktable developers; dual demosaic by Ingo Weyrich (RawTherapee), adapted by Hanno Schwalm | GPL-3.0-or-later |
| `crates/lc-raw/src/highlight/opposed.rs` | [`src/iop/hlreconstruct/opposed.c`](https://github.com/darktable-org/darktable/blob/733bd69f32cac7ff5e41025115942772add1f088/src/iop/hlreconstruct/opposed.c) | `733bd69f32cac7ff5e41025115942772add1f088` | Copyright (C) 2022-2026 darktable developers | GPL-3.0-or-later |
| `crates/lc-raw/src/capture.rs`, `crates/lc-pipeline/src/capture.rs` | [`src/iop/demosaicing/capture.c`](https://github.com/darktable-org/darktable/blob/733bd69f32cac7ff5e41025115942772add1f088/src/iop/demosaicing/capture.c) | `733bd69f32cac7ff5e41025115942772add1f088` | Copyright (C) 2025-2026 darktable developers; algorithm by Ingo Weyrich (RawTherapee) | GPL-3.0-or-later |
| `crates/lc-pipeline/src/toneeq.rs` | [`src/iop/toneequal.c`](https://github.com/darktable-org/darktable/blob/733bd69f32cac7ff5e41025115942772add1f088/src/iop/toneequal.c), [`src/iop/gaussian_elimination.h`](https://github.com/darktable-org/darktable/blob/733bd69f32cac7ff5e41025115942772add1f088/src/iop/gaussian_elimination.h), [`src/common/luminance_mask.h`](https://github.com/darktable-org/darktable/blob/733bd69f32cac7ff5e41025115942772add1f088/src/common/luminance_mask.h) | `733bd69f32cac7ff5e41025115942772add1f088` | Copyright (C) 2018-2026 / 2017-2023 / 2019-2026 darktable developers (Aurélien Pierre) | GPL-3.0-or-later |
| `crates/lc-pipeline/src/colorcal.rs` | [`src/iop/channelmixerrgb.c`](https://github.com/darktable-org/darktable/blob/733bd69f32cac7ff5e41025115942772add1f088/src/iop/channelmixerrgb.c), [`src/common/chromatic_adaptation.h`](https://github.com/darktable-org/darktable/blob/733bd69f32cac7ff5e41025115942772add1f088/src/common/chromatic_adaptation.h) | `733bd69f32cac7ff5e41025115942772add1f088` | Copyright (C) 2010-2026 / 2020-2025 darktable developers (Aurélien Pierre) | GPL-3.0-or-later |

### `negadoctor.c` → film negative conversion

What was ported: the per-pixel inversion and print model (`commit_params`, `_process_pixel`: Cineon
densitometry with D-min as the fulcrum, density rescaling by D-max, log-space white balance and
offset, paper black / exposure / grade, the highlight soft clip), the parameter set with its units,
ranges and defaults (`dt_iop_negadoctor_params_t`, `init`), and the automatic colour-picker
estimates (`apply_auto_Dmin`, `apply_auto_Dmax`, `apply_auto_offset`, `apply_auto_black`,
`apply_auto_exposure`). The settings live in `crates/lc-develop/src/settings.rs` (`Negative`), the
picker commands in `crates/lc-engine/src/cmd/negative.rs` and the panel in
`crates/lc-ui-egui/src/panels/edit.rs`.

Differences from upstream: a "slide" film stock passes the scan through unchanged; for B&W film the
film-base picker stores one grey (the area's mean of the three channels); the automatic print
exposure includes the highlights white balance in the density offset exactly as the conversion
applies it (upstream's picker leaves it out; both agree at the neutral balance); image-wide
estimates ignore 0.1 % tails per channel.

Upstream header (`src/iop/negadoctor.c`):

```
    This file is part of darktable,
    Copyright (C) 2020-2026 darktable developers.

    darktable is free software: you can redistribute it and/or modify
    it under the terms of the GNU General Public License as published by
    the Free Software Foundation, either version 3 of the License, or
    (at your option) any later version.

    darktable is distributed in the hope that it will be useful,
    but WITHOUT ANY WARRANTY; without even the implied warranty of
    MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
    GNU General Public License for more details.

    You should have received a copy of the GNU General Public License
    along with darktable.  If not, see <http://www.gnu.org/licenses/>.
```

The module's documentation credits its references: Kodak's sensitometry workbook, the Cineon
format paper (digital-intermediate.co.uk) and an OpenEXR mailing-list post on highlight
compression (lists.gnu.org/archive/html/openexr-devel/2005-03/msg00009.html).

### Develop toolset upgrades (2026-10-09)

All of these are off by default (or at the behaviour photos had before), so earlier edits render
as before by default; the X-Trans capture-radius row offset was deliberately corrected. Each file's module documentation lists the upstream functions it follows and
how it differs from them. Several of the algorithms started in **RawTherapee** (GPL-3.0-or-later,
<https://github.com/RawTherapee/RawTherapee>) and **ART** (GPL-3.0-or-later). The ports here were
made from darktable's versions. The authors named in the upstream headers are credited above and
in the module documentation.

* **`rcd.c` → RCD demosaic** (`Demosaic::Rcd`). Ratio Corrected Demosaicing v2.3 by Luis Sanz
  Rodríguez. The tiling is Ingo Weyrich's from RawTherapee. Differences: the mosaic is read
  mirror-reflected, so the borders are interpolated like the interior; the input is not rescaled;
  the diagonal high-pass filters are read at the site itself. Upstream's half-width packing reads
  the neighbouring column on some rows, while the original RCD code reads the site.
* **`vng.c`, `basics.c` → VNG4 and median colour smoothing.** The full four-colour gradient
  interpolation (all 64 terms) is available separately. The new RCD/AMaZE dual options use
  upstream's VNG-linear + two median passes with its exact 9x9 disc Gaussian detail mask.
  The `dualRcd` bilinear decode and historical separable-Gaussian mask remain unchanged for
  saved settings. Masks use unbalanced camera RGB, equivalent to upstream undoing WB.
* **`amaze.cc` → AMaZE and dual-AMaZE.** Emil Martinec's full algorithm, optimized by Ingo
  Weyrich, with ideas from Luis Sanz Rodrigues and Paul Lee. Shared scratch-plane lifetimes,
  border reflection, tile overlaps, Nyquist refinement and diagonal interpolation are retained
  in safe Rust. Tiny crops get parity-preserving mirror extension. Finite negatives/HDR are
  preserved like darktable; RawTherapee clamps negative outputs. See its separate notice.
* **`segbased.c`, `segmentation.c` → segmentation-based CFA highlights.** The opposed CFA
  fallback, morphological combination, scanline segments, per-segment candidates, Euclidean
  distance transform, ring-gradient recovery, box/Gaussian filtering and Poisson noise are
  ported. All seven recovery modes are available in the raw API. WB is temporarily applied and
  undone; host GUI diagnostic masks/chroma caches are omitted. Bayer preview reductions keep
  the four CFA phases and clipped maxima before the CFA hook. X-Trans runs the full CFA hook.
  Segmentation preview/full discrepancies and reference tolerances are recorded in
  [CODEX-REPORT](../docs/wip/CODEX-REPORT.md).
* **`opposed.c` → inpaint-opposed highlights** (`HighlightMode::Opposed`). This is the linear
  (demosaiced) variant, by Hanno Schwalm with @garagecoder and @Iain (G'MIC). Differences: it runs
  on camera RGB before white balance; the clipped test reads each channel; the superpixel grid
  rounds up.
* **`capture.c` → capture sharpening.** The radius estimation is in `lc-raw/src/capture.rs`. The
  Richardson–Lucy deconvolution, the per-pixel kernel table with corner boost, and the blend mask
  are in `lc-pipeline/src/capture.rs`. The algorithm is Ingo Weyrich's from RawTherapee.
  Differences: the deconvolution runs on the scene-linear Rec.2020 source with Rec.2020 luminance
  weights. The X-Trans radius scan now uses the upstream post-search row offset.
* **`toneequal.c`, `gaussian_elimination.h`, `luminance_mask.h` → tone equalizer**, by Aurélien
  Pierre. Ported: the Gaussian interpolation matrix, the least-squares solve, the correction
  table and the per-pixel correction. Differences: the mask is a guided filter of the log2
  Euclidean-norm luminance; upstream uses a guided filter or EIGF of the linear luminance.
* **`channelmixerrgb.c`, `chromatic_adaptation.h` → colour calibration** (adaptation and gamut
  compression only, not the channel mixer), by Aurélien Pierre. It supports the CAT16, Bradford
  (linear and non-linear) and XYZ adaptations. Differences: the target white is D65, which is
  this pipeline's Rec.2020 working space (upstream: D50), and adaptation is always complete.

The upstream headers of these files are the same GPL-3.0-or-later notice as `negadoctor.c` above,
with the copyright years listed in the table.

Independent extracted upstream fixtures and regeneration recipes accompany every listed raw
quality port under the raw/pipeline `tests/fixtures/README.md` files. No C/C++ is part of the build.
The distance transform retains Pedro Felzenszwalb's original GPL-2.0-or-later authorship and
algorithm attribution (compatible with this GPL-3.0-or-later combined work).
