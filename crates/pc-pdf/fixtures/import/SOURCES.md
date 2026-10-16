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
or online build.

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

## FS009 exact inline-image admission and canonical marker normalization

`tests/import/inline.rs` uses original GPL-3.0-or-later fixtures, comparing inline
images with separate XObjects carrying the same independently encoded samples.
Cases cover exact packed rows; ASCIIHex/85, RunLength, Flate, LZW, CCITT and JPEG
codec endpoints; predictors and filter chains; aliases, CRLF, stencils and pixel
whitespace; fake EI/BI within binary data; JPEG EOI bytes inside APP data; later
strings/names/comments in content arrays; subsequent real images; and recursive
Form references after the actual boundary. Existing helper encoders remain
unchanged apart from module visibility. Negative controls retain truncation,
explicit Length, duplicate aliases, allocation/object/filter limits and cancel.

Private tests in `preflight/inline.rs` append real PDF operators after each codec
and assert the exact consumed position. Whole-stream ASCII decoder entry points
still reject that tail. The pinned hayro typed parser independently compares
literal/name values before and after escaping (including nested parentheses,
backslashes, existing octal/name escapes, non-ASCII and NUL). Canonical stream
rewrite tests preserve opaque dictionary spans while updating transport fields.

Implementation is original and based on observed APIs/semantics, not copied
upstream code. The compatibility target is hayro-syntax 0.7.2, registry-verified
commit `34834627c0b4afa9c83c9b64d4d978b127030c77`: `content/mod.rs` heuristically
searches EI and ignores inline Length; `page.rs` concatenates content arrays.
Exact boundaries and ASCIIHex wrapping prevent those heuristics from seeing
binary marker candidates. Equivalent octal/# escapes and comment removal cover
all relevant content-array members while retaining decoded values. Unsupported
resource-resolved colorspaces, indirect inline metadata, ambiguous syntax and
tokens split across streams remain explicit refusals. No public schema, new
dependency or relaxed decoder guard is introduced.

The Group3 LF boundary regression uses exact bytes `[0x4d, 0x9a, 0x80]`:
white1728 makeup `010011011` plus white0 `00110101`, padded by seven zeros.
Those padding bits plus LF can mimic an EOL to a decoder reading the PDF tail.
Prefix mode now returns the completed-row bit position before optional EOL/EOB
lookahead. The inline boundary resolver tries that exact position first, then
at most six Group3 or two Group4 EOL markers; it never searches arbitrary EI.
The selected slice still passes ordinary isolated CCITT validation. The fixture
requires identical output to the three-byte image XObject with a following blue
vector marker, proving that the PDF separator and later drawing are retained.

Optional-Type role follow-up: pinned hayro-interpret 0.7.0 dispatches Type3 fonts
from Subtype and tiling patterns from stream objects, without requiring Type.
The role classifier therefore accepts Subtype Type3 CharProcs and declared
PatternType1 streams without those optional Type names. New canonical and pixel
fixtures omit Type, include fake EI/BI image samples in a glyph/pattern, and
compare them with matching XObject images. Actual BI in any still-unclassified
stream is refused before publication; literal/comment text containing BI remains
opaque and byte-exact. Existing XML/CMap preservation controls are retained.

## Empty-user-password Standard security admission

`encryption/README.md` and `encryption/generate.py` record the synthetic R2–4
RC4/AES128 fixture provenance. Positive inputs are compared with the existing
independent plaintext artwork builder. Private tests inspect exact metadata,
strings, nonzero reference generations, a large scalar and deferred object-stream
members. Original page cycles, invalid xref locations and bombs that only expand
after decryption must fail normal admission after a safe bootstrap; invalid AES
blocks/padding and malformed handlers fail before plaintext publication. Runtime
tests need no OpenSSL, external network, new library or password UI.
