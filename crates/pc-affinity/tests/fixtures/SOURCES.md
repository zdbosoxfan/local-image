Original Local Image regression data, GPL-3.0-or-later, generated 2026-10-10.
No Affinity application, proprietary program resources or brand assets were used.

- `shapes.afdesign`: v12 #FT4 archive, zlib doc.dat, transparent 48×48 page,
  rectangle (4,4)–(44,44), solid straight-alpha red; analytic ink area 1600.
- `warnings.afphoto`: same rectangle, attached unknown adjustment and unknown
  `ZZZZ` sibling named Future. The supported picture survives with aggregated warnings.
- `preview.afphoto`: unsupported v9 object stream, independently CRC-encoded
  8×6 PNG thumbnail of straight RGBA (20,80,200,255). Only Open recovers it.

The layout was independently assembled with Python 3 `struct` and `zlib` (level 1
for doc.dat, default for PNG IDAT), using the pinned reader's documented record
layout. Generation script/evidence is `target/affinity24/make_fixtures.py`; frozen
hashes in `hashes.json` make the checked-in bytes the durable independent oracle.
These are tests of accepted reader records, never an exporter or evidence that
Affinity can open synthetic data. Rust scene tests generate additional cubic holes,
shared references/cycles, text, ellipse and gradient cases at runtime.

Reader test/source adaptations retain VectorCraft's original tests at
`d522c1d7be4035bd4f4a84cd6ebfca44f5155092` (MIT OR Apache-2.0), © 2026 ArtCraft
Team and VectorCraft contributors. The `text` and `support/text_fixture` tests are
synthetic, with scalar-index / UTF-8-byte-index expectations; no external fonts
are bundled. The app's existing default font database performs substitutions.

Four real MIT documents are used read-only from the pinned VC corpus in
`target/affinity24/corpus`; they are not committed. `AFFINITY_CORPUS_DIR` is required
by the explicitly ignored external-corpus test. Its absence is never a pass.
See this package's integration/report records for URLs, hashes, licensing and
what remains missing. No test contacts the network.
