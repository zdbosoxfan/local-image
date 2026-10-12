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

## FS009 bounded compatibility, stage one

`tests/import/compatibility.rs` adds original GPL-3.0-or-later compact fixtures
using this directory's independent classic-xref writer. They cover leading
headers at offsets 0, 1 and 1024; ASCIIHex/ASCII85/RunLength names and aliases;
ASCII85+Flate chains; RunLength repeat packets; forward and backward scalar
Length objects; exact editable-shape/compositor equivalence to unfiltered art.
Negative controls include malformed encodings, missing terminators, expansion
and intermediate-stage budgets, whitespace-driven upstream allocation, cancelled
imports, invalid length types/generations/references, and an xref deliberately
pointing at a convincing fake length object inside stream bytes. Private decoder
known-answer tests cover odd hex nibbles, ASCII85 zero groups/padding and literal
plus repeat RunLength packets at exact output budget boundaries.

This stage requires no external program or new dependency. It intentionally does
not enable LZW/CCITT/JPX/JBIG2, inline images, encryption, nonisolated transparency,
Type 4 functions, forward lengths in xref-stream-only/hybrid/incremental files,
or malformed filter repairs. Those require separate bounded semantic support.

## FS009 LZW and predictor admission, stage two

`compatibility.rs` adds an independent literal/code packer which crosses the
9/10/11/12-bit boundaries for both EarlyChange values, exercises clear resets
and table saturation, and renders the resulting PDFs against uncompressed art.
The private decoder known-answer vector is also used by pinned hayro-syntax
0.7.2 (`filter/lzw_flate.rs`), with expected bytes `-----A---B`; it exercises
actual dictionary references and the next-code special case rather than literals.

TIFF Predictor 2 and PNG predictors 10–15 are compared against unfiltered native
geometry and image pixels. Image cases include byte-aligned 1/2/4/8/16-bit
Grayscale TIFF prediction and 8-bit RGB PNG prediction. The same content-stream
predictor checks run through existing Flate and new LZW paths. A predictor-hidden
recursive Form must still fail graph admission. Malformed codes/EOD, bad
DecodeParms/DP shapes, overflow-prone dimensions, truncation, PNG row tags,
packing unsupported by the pinned decoder, output/dictionary/work budgets and
cancellation retain explicit negative controls. No new dependency or external
renderer is needed: interpreted output is compared with independently generated
unfiltered PDF fixtures and known samples.

LZW support here does not enable CCITT, other image codecs, encryption, inline
images or nonisolated transparency. Predictor layouts whose upstream BitChunks
handling drops partial pixel chunks or misaligns TIFF sub-byte rows are refused;
indirect DecodeParms and ambiguous aliases are also refused explicitly.

## FS009 CCITT strict admission, stage three

Original hand-coded T.4/T.6 fixtures in `tests/import/ccitt.rs` encode four 8px
rows: white, black, black, white. White8=`10011`; black8 consists of white0
`00110101` plus black8 `000101`. Group4 uses V0, Horizontal, V0/V0 and Pass;
Group3 fixtures cover 1D and tagged mixed 1D/2D. These fixtures do not call the
production decoder or an encoder to build their data. Uncompressed one-bit
image PDFs supply independent exact pixel references, including stencil and
BlackIs1 inversion. Options cover row byte alignment, optional EOL/EOB, missing
Rows using pinned image-height default, alias CCF and ASCIIHex filter chains.

Private decoder tests check those known pixels, callback output errors,
cancellation triggered after the first decoded row, shared bit-work exhaustion,
and repeated zero-length runs that consume work without output growth.
Malformed/truncated/early-EOB/overlong rows, dimensions, explicit Rows=0,
damaged-row repair, allocation budgets, decoder aliases and filter-order bypasses
must fail before canonical PDF publication. No partial upstream rows count as
success. The adapted implementation's source/hash/license is in `docs/PORTS.md`.

Admitted CCITT streams use direct one-bit DeviceGray or stencil image metadata,
matching positive Width/Columns and Height/Rows. Explicit Rows=0 remains refused
because pinned Group3 and Group4 interpret it differently; missing Rows uses
Height in pinned hayro-syntax 0.7.2. Indexed/indirect color spaces, overriding
inline aliases and damaged-row repair need separate normalization. This stage
does not enable inline images, encryption, nonisolated groups or new versions.
