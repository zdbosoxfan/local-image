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
exactly as before. Each file's module documentation lists the upstream functions it follows and
how it differs from them. Several of the algorithms started in **RawTherapee** (GPL-3.0-or-later,
<https://github.com/RawTherapee/RawTherapee>) and **ART** (GPL-3.0-or-later). The ports here were
made from darktable's versions. The authors named in the upstream headers are credited above and
in the module documentation.

* **`rcd.c` → RCD demosaic** (`Demosaic::Rcd`). Ratio Corrected Demosaicing v2.3 by Luis Sanz
  Rodríguez. The tiling is Ingo Weyrich's from RawTherapee. Differences: the mosaic is read
  mirror-reflected, so the borders are interpolated like the interior; the input is not rescaled;
  the diagonal high-pass filters are read at the site itself. Upstream's half-width packing reads
  the neighbouring column on some rows, while the original RCD code reads the site.
* **`dual.c`, `detail.c` → dual demosaic** (`Demosaic::DualRcd`). RCD is used on detail and a
  smooth method in flat areas, blended by the detail mask. The dual method is by Ingo Weyrich
  (RawTherapee) and was adapted by Hanno Schwalm. Differences: the flat method is bilinear (not
  VNG4), and the mask is computed without white balance.
* **`opposed.c` → inpaint-opposed highlights** (`HighlightMode::Opposed`). This is the linear
  (demosaiced) variant, by Hanno Schwalm with @garagecoder and @Iain (G'MIC). Differences: it runs
  on camera RGB before white balance; the clipped test reads each channel; the superpixel grid
  rounds up.
* **`capture.c` → capture sharpening.** The radius estimation is in `lc-raw/src/capture.rs`. The
  Richardson–Lucy deconvolution, the per-pixel kernel table with corner boost, and the blend mask
  are in `lc-pipeline/src/capture.rs`. The algorithm is Ingo Weyrich's from RawTherapee.
  Differences: the deconvolution runs on the scene-linear Rec.2020 source with Rec.2020 luminance
  weights.
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

### Colour and tone (2026-10-09)

At **733bd69f32cac7ff5e41025115942772add1f088**:

| Our file | Upstream file | Copyright / authors | Licence |
|---|---|---|---|
| `crates/lc-pipeline/src/tone2.rs`, GPU `finish.wgsl` | `src/iop/sigmoid.c` | Copyright (C) 2020-2026 darktable developers; sigmoid by Jakob Andrén | GPL-3.0-or-later |
| `crates/lc-pipeline/src/base_curve_data.rs`, `basecurves.rs` | `src/iop/basecurve.c` | Copyright (C) 2010-2026 darktable developers and preset contributors | GPL-3.0-or-later |
| `crates/lc-pipeline/src/basecurves.rs` | `src/common/curve_tools.c` | Copyright (C) 2011-2022 darktable developers; based in part on UFraw `nikon_curve.c`, Shawn Freeman and Udi Fuchs (2004-2008) | GPL-3.0-or-later |

Sigmoid: `commit_params`, `_generalized_loglogistic_sigmoid`, negative desaturation, channel
ordering and `_preserve_hue_and_energy` are faithful scalar ports. With upstream default
primaries (work profile, zero attenuation/rotation/purity), the extra primary transforms are
identity. Those optional darktable controls are not exposed. The runtime curve is log-sampled
for matching CPU/GPU interpolation; exposure is +0.7 EV, chosen hue preservation defaults to
75% (owner decision), and the absolute black endpoint is extended continuously to zero.

Base curves: all 31 active monotone camera/maker presets and the upstream monotone Hermite
polynomial are retained. Above 90% display luminance a C1 asymptotic shoulder replaces the
upstream unbounded exponential extrapolation and clipping, retaining highlight headroom.
The Camera JPEG fit uses the luminance norm and scales all channels together, then applies its
fitted chroma curve and hue-preserving gamut containment. The hue slider blends the
luminance-ratio colour with a per-channel curve at the same luminance (a documented extension;
at 100% the luminance-ratio port is exact). Soft Film is an independently fitted
parametric curve; **no Adobe DNG SDK / ACR3 table is copied**. Standard, Extra Shadow, High
Contrast and Linear variants are our own scene-exposure shapers (Linear is identity in scene light
below 80%, with a continuous output shoulder). C fixtures test the extracted upstream scalar functions;
see `crates/lc-pipeline/tests/fixtures/README.md`.
