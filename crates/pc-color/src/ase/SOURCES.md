# Package 39 ASE fixture

`fixture.ase` is original Local Image test data, GPL-3.0-or-later, authored 2026-10-10.
SHA-256: `e2afdb683560149b074d0d84c70b15ed52aeee8bdec9f37c443e13bb75498109`. No brand assets or proprietary palette files.

It was assembled independently of the Rust writer using Python 3.14.8's
standard `struct.pack` (`>HHI` header, `>HI` blocks, `>f` components and UTF-16BE names).
Parameters: ASE 1.0, one ungrouped global RGB colour (0.25,0.5,0.75),
“Café 🎨” group with CMYK spot (0.25,0.5,0.75,1), Gray process (0.75),
Lab process (L/100=0.5,a=20,b=-30); one group-end block. Parser/writer code is all Rust.

Layout/model/type oracle: [VectorCraft palette_io.rs](https://github.com/storytold/vectorcraft/blob/d522c1d7be4035bd4f4a84cd6ebfca44f5155092/crates/color/src/palette_io.rs),
MIT OR Apache-2.0, pinned local source HEAD verified. The reference itself cites
[Selapa format description](https://www.selapa.net/swatches/colors/fileformats.php) and
[nsfmc/swatch](https://github.com/nsfmc/swatch). Local Image's Gray stores luminance,
unlike VectorCraft's Gray ink, so ASE Gray is mapped without inversion here.

Recorded tests exercise every truncation offset, Unicode, colour kinds/models, byte-stable
writer re-emission, unknown-block/model warnings, invalid version/count/length, nonfinite
output and alpha-loss handling through the pc-doc exchange adapter. No external Adobe ASE
corpus was supplied; proprietary/external compatibility is not a passed corpus gate.
Toolchain: cargo/rustc 1.98.1, offline, existing serde; no dependency changes.
