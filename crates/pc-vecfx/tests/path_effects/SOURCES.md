# Path effects 45 fixture/oracle sources

All fixture paths are generated at test time by original Local Image Rust tests
(GPL-3.0-or-later); no proprietary artwork, image corpus or brand assets.

Toolchain: cargo/rustc 1.98.1; kurbo 0.13.1; pc-vector's existing exact-area rasterizer.
Upstream reference checkout was read-only and HEAD verified as
`d522c1d7be4035bd4f4a84cd6ebfca44f5155092`, VectorCraft, MIT OR Apache-2.0.
Read `effects/{distort,stylize,util,warp,live}.rs`, `doc/live.rs`,
`engine/cmd/effectcmd.rs` and its edit tests. No source is fetched during tests.
Notices: repository `licenses/vectorcraft-{LICENSE-MIT,LICENSE-APACHE,NOTICE}`.

Official behavior source (accessed 2026-10-10):
https://helpx.adobe.com/illustrator/using/appearance-attributes.html
and its linked https://helpx.adobe.com/illustrator/using/reshape-with-effects.html
(the latter endpoint returned an error; the pinned VC implementation is the formula source).
The Appearance source supports targeting paints, editing effects and order/visibility.

Parameters/oracles are explicit in `geometry.rs`: square at (8,8), 100×100, radii
1/10/30 against 10000−(4−pi)r² using cubic Green-integral area; offsets −4/0/8
against the actual V1 API; extra size (20,10), radius 10 against primitive bounds/areas;
Roughen seed123/124, size3, detail10 at 72/144 DPI; Zig Zag size5/ridges3 on a
100-pixel horizontal line against ±5 normal displacement; Pucker100 against midpoint
anchor contraction; Free Distort normalized four corners and Twist120 on an 80×40
ellipse checked with dense independent formula samples and V1 nearest-curve distance.
Order uses nonuniform Transform then Offset vs the opposite order. Transform copies3
are checked for paint order and independent duplicate translations. Identity fixtures
include real ellipse cubics, EvenOdd holes, Join and inverted filling.

No external goldens/corpus were supplied; they are missing evidence, not passed gates.
Nonlinear mapping uses numerical interpolation/subdivision with sample guards; no exact
interval-bound proof or competitor pixel identity is claimed.
