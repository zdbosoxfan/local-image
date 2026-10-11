# Package 12 layout fixtures

Synthetic strings and analytic line/circle inputs in `main.rs` are original
Local Image GPL-3.0-or-later fixtures. Geometry oracle: exact radius 100 circle,
arc length 200π; text at 72 DPI, Inter Regular 20pt. Rotation tolerance 0.5°,
anchor tolerance 0.5px. Straight pc-text raster oracle uses threshold 127/255
and IoU >0.97. This is not an Inkscape/resvg textPath golden.

Bundled Inter Regular: `assets/fonts/Inter-Regular.ttf`, SIL OFL 1.1, notice
`assets/fonts/OFL-Inter.txt`; SHA-256
`40d692fce188e4471e2b3cba937be967878f631ad3ebbbdcd587687c7ebe0c82`.

Required Japanese fixture: installed Noto Sans CJK VF TTC, SIL OFL 1.1,
`/usr/share/fonts/google-noto-sans-cjk-vf-fonts/NotoSansCJK-VF.ttc`, SHA-256
`d3d8256cdec8dbcb3552284bc6b20c734dd60c2ee9df83b5758e34807c4bac32`.
Override path with `LOCAL_IMAGE_PATH_TEXT_CJK_FONT` for another licensed Noto
CJK fixture. The test fails when it is missing; it never reports a passing skip.
It registers font bytes through the existing pc-text FontDb, selects the JP
family, and retains upright/rotated glyph IDs, metrics and vertical alternates.
Upstream font project: https://github.com/notofonts/noto-cjk (read-only system
fixture, not redistributed). No proprietary font is used for these gates.

Placement reference: VectorCraft `crates/text/src/layout.rs` at
`d522c1d7be4035bd4f4a84cd6ebfca44f5155092` (MIT OR Apache-2.0, copyright
2026 ArtCraft Team and the VectorCraft contributors); existing license notices
remain in `licenses/vectorcraft-*`. SVG semantics:
https://www.w3.org/TR/SVG2/text.html#TextPathLayoutRules (method=align,
advance midpoint tangent). Test toolchain Rust 1.98.1; parley 0.11.1,
harfrust 0.12.0, skrifa 0.44.0 from the unchanged workspace lock.
