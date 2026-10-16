# rawler camera calibration data and RAW engine

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
are normalized for lookup. Missing illuminants stay absent (no fabricated A matrix). The calibration table remains the first choice for known bodies. File-provided calibration always wins.
Gaps use RawTherapee `camconst.json`; see `rawtherapee-NOTICE.md`. No RawSpeed cameras.xml
or CC-BY-SA camera data is used.


## Unmodified runtime dependency (2026-10-13)

`crates/lc-raw/Cargo.toml` pins the **unmodified crates.io `rawler = 0.8.0`** release, with
`default-features = false`. There is no vendored source, path dependency, fork or `[patch]`.
The only integration module is `crates/lc-raw/src/rawler_adapter.rs`. Measured format/compression routing selects it as the primary RAF/NRW/RW2 reader where it wins,
and as fallback for variants our native decoders report as unsupported, including Canon CR3 RAW/C-RAW and dual-pixel
files, compressed Fujifilm Bayer/X-Trans RAF, and compressed Olympus/OM ORF. The adapter
also uses rawler's public camera database for calibration missing from our first-choice table.

The library licence remains **LGPL-2.1-only**, elected GPL under LGPL 2.1 section 3 for
redistribution in this GPL-3.0-or-later application. The unmodified upstream LGPL text is
included as `rawler-LGPL-2.1.txt`. Authors include Daniel Vogelbacher and Pedro Côrte-Real;
individual codec authors and notices remain in the complete corresponding crate source.
The locked source is available from <https://crates.io/crates/rawler/0.8.0> and
<https://github.com/dnglab/dnglab/tree/v0.8.0/rawler>. Cargo.lock records the registry checksum.
See `docs/RAW-DECODERS.md` for maintenance and the dependency audit.
