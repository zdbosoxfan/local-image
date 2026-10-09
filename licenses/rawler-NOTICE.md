# rawler camera calibration data

`crates/lc-raw/src/camera_matrices.rs` contains three-channel colour matrices and camera aliases
from **rawler**, in dnglab **v0.8.0**, `rawler/data/cameras/**/*.toml`.
Upstream: <https://github.com/dnglab/dnglab>.
Authors: Daniel Vogelbacher, Pedro Côrte-Real and the camera-data contributors listed in
upstream `AUTHORS` and individual TOMLs. Upstream licence: **LGPL-2.1-only** (the rawler
manifest specifies `LGPL-2.1`).

We elect the ordinary GNU GPL under **LGPL 2.1 section 3** for these copied data, and distribute
them as part of this GPL-3.0-or-later project. The corresponding source data can be reproduced
from the v0.8.0 archive using `scripts/engine_colour_data.py`; the exact input tree and aggregate
SHA-256 are in `crates/lc-raw/data/colour-sources.json`. The supplied offline archive has no Git
commit metadata; its immutable Git tree is `ae01bcb2d0f8a74f9dfbb9f7b5c7e315c8e668b7`.

Differences: only three-channel 3×3 A/D65 matrices are imported; exact EXIF and clean aliases
are normalized for lookup. Missing illuminants stay absent (no fabricated A matrix). No rawler
runtime dependency or raw-format decoder is used. File-provided calibration always wins.
Gaps use RawTherapee `camconst.json`; see `rawtherapee-NOTICE.md`. No RawSpeed cameras.xml
or CC-BY-SA camera data is used.
