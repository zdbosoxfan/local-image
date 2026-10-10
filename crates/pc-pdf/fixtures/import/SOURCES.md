# Package 22 PDF fixture sources

`source.rs` is original Local Image contributor code, GPL-3.0-or-later, written
2026-10-10. It writes conventional PDF 1.7 xrefs independently of hayro/krilla.
No downloaded artwork, proprietary corpus, brand assets, filesystem font scan,
network requests, or external resource resolution are used by the tests.

Fixtures are generated in memory: 64×64 point RGB rectangles, a two-page
64×64 / 80×40 document (second CropBox [10 5 70 35], Rotate 90, bundled Helvetica),
luminosity/alpha Form masks, red→blue axial shading, and an uncompressed 2×2 RGB
image with four distinctive corners. Additional test content supplies cubic
lens area 1382.4, even-odd holes, native clip paths, 8pt butt strokes, nonuniform
stroke outlines, radial shading, isolated transparency, a repeated tiling cell,
a Type 4 triangle mesh, and original Type 3 glyph artwork. Pages use 72 DPI unless the test explicitly sets 144.

Hostile fixtures include oversized dimensions/xref Size, zlib expansion (1 MiB
zero stream, 1024-byte budget), cyclic/inherited Forms, tiny dashes, >64 saves,
unsupported inline images/knockout/nonisolated groups, exponential shared page
trees, sampled-function/packed-mesh/predictor allocation, binary-resource/content reuse, and 256 proptest random byte arrays of 0..4096 bytes, both raw and wrapped as valid PDF content streams. Generated hostile data is not an external corpus.

Independent oracles: PDF rectangle/clip coordinates, elementary circle-distance
radial interpolation, triangle barycentrics, PDF luminosity weights, four-corner
image orientation, and Green's theorem cubic area. hayro's colour conversion
quantizes channels to 8 bits; channel and premultiplied CPU/GPU tolerances are
1/255. Native path edits rebuild caches using the existing pc-vector rasterizer.

The public package 21 API generates additional round-trip PDFs using krilla
0.8.2, compression on/off, `ExportScope::Document`, and `ExportOptions::default()`
(or only `compress` changed). Embedded Inter text is `PDF`, 14pt, at (4,24), from
the workspace's existing pc-text font assets and licenses. The exporter is not
copied into the import implementation or modified by package 22.

Development tested a read-only sibling package 21 snapshot in the disposable
registration worktree, not an invented test exporter. Initial development snapshot SHA-256 values:

- export.rs: b842df15bcacf841d89ecbb45d2522d0c8f642f325da91be1ed947491181eb48
- export/paint.rs: ea2d3caeabcc239f562e93243ade0f84120f7dd7fb628a83dfa91c4e4c7ffded
- export/text.rs: 27ff658b5da5089e23da2667f23d25d881461aab6af45c6877b041212fd780c8
- export/validate.rs: 5dea23cf97abc45db6af5f172eec9eeae97d1050379310920f359dc8dad2f52a

Toolchain: Rust/cargo 1.98.1, hayro-syntax 0.7.2, hayro-interpret 0.7.0,
kurbo 0.13.1, image/flate2/proptest at workspace-lock versions. No new dependency
or online build. Integration registration and exact commands are in
`docs/wip/vector-tasks/integration/22.md`; gate outcomes are in the package report.

Missing external/proprietary PDF suites are recorded as missing, never passed.

The final lint/full-suite pass refreshes that read-only snapshot to package 21
current sources (which fix an upstream snapshot lint), with no exporter edits:

- export.rs: d3cef883f25d62f7451a29f0ddeae2865ebaa525d2ac973b6343d49931cda587
- export/paint.rs: 564fd28fe68619059777ad7aaa5c3a643394a44bce5442da634a2d1e12ffdf30
- export/text.rs: a57b916fef6b0be94e5967e32193c28bd59da46a7f381a66882bbd72c172013d
- export/validate.rs: d4219adbac62067ebaf6b17a7082b8d21a45942b4b798aad1f17907e93e3dae5
