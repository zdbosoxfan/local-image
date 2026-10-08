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
