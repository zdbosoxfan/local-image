# Package 21 PDF export sources and fixtures

All test geometry, paints, page lists, masks and strings are original synthetic
Local Image fixtures under GPL-3.0-or-later, generated offline by Rust tests.
No proprietary corpus, downloaded artwork, brand assets, or external PDF files
are used. Missing proprietary/cross-package fixtures are not successful gates.

References inspected before implementation:

- VectorCraft `crates/pdf/src/export.rs`, pinned at
  `d522c1d7be4035bd4f4a84cd6ebfca44f5155092` (verified with local git),
  MIT OR Apache-2.0, copyright 2026 ArtCraft Team and contributors. Adapted
  tree/graphics-state, gradient and positioned-font writer organization; the
  VectorCraft object model, shaper, renderer and handwritten postprocessors
  were not adopted. Existing `licenses/vectorcraft-*` notices remain intact.
- krilla **0.8.2**, cached source and official
  [Surface API](https://docs.rs/krilla/0.8.2/krilla/surface/struct.Surface.html),
  `examples/parley.rs`, `examples/stream_builder.rs`, `graphics/paint.rs`,
  `graphics/mask.rs`, `graphics/image.rs`, `surface.rs` and `text/font.rs`.
  Default features disabled; only the package-00 approved `raster-images`.
- [PDF 1.7 reference](https://opensource.adobe.com/dc-acrobat-sdk-docs/pdfstandards/pdfreference1.7old.pdf),
  graphics state, clipping, shadings, transparency groups, masks and text.
  Downloaded read-only reference SHA-256:
  `4aa598e7e2cc88867565fae793db38c140a10ca2b5551dccdb45e2124282ce29`.
  Inspected sections 7.5.4–7.5.5 and 5.9.2 through Poppler text extraction;
  reference PDF/text stay under `target/refvec/pdf21`.
- [OpenType OS/2 fsType](https://learn.microsoft.com/en-us/typography/opentype/spec/os2#fstype):
  refuse restricted, bitmap-only and no-subsetting fonts before handing font
  bytes to krilla/subsetter. Font-table reading is bounded safe Rust.
- Existing Local Image `pc-svg/src/export{.rs,/validate.rs,/paint.rs}`:
  complete target validation, detached scoping, compound normalization,
  gradient frame/ramp and bounded cache preparation. These Local Image
  preflight helpers are reused/adapted; no second geometry evaluator is added.
- Existing `pc-text::{layout,render,fonts}` public APIs: positioned glyph IDs,
  font bytes, normalized instance coordinates, outlines and decorations.
  Public layout currently lacks source ranges on PlacedGlyph. One-to-one
  cluster correspondence is checked exactly; ambiguous substitutions/marks,
  vertical or variable-instance text uses authoritative pc-text outlines and
  reports lost PDF text semantics. Full embedded mapping needs 00/pc-text's
  source-range extension; no independent shaper or heuristic Unicode is used.
  Export is bounded to 1024 source runs/paragraph runs, 100,000 placed glyphs
  and 1,000,000 outline segments. Position correspondence uses a cancellable
  indexed exact lookup rather than a quadratic scan.

Bundled font fixture: Inter Regular, existing `assets/fonts/Inter-Regular.ttf`,
SIL Open Font License 1.1; original notice/license in `assets/fonts`. Font fixture
uses “ABC Café”, 22 pt at 96 DPI, positioned at (16,64). Ligature/mark/multiline
fixtures use Inter, 16 pt. No font file is newly copied into this package.

Structural oracle: **hayro-syntax 0.7.2**, resolved objects and decoded streams,
including compressed form/content streams. Assertions check parsed path
instructions, image subtypes, PDF page dimensions/order, shading and SMask
resources, embedded subset FontFile2 and ToUnicode. Font extraction is
independently checked with Poppler `pdftotext`, not writer string equality.
The hard-stop regression checks PDF 1.7 §3.9.3's strictly increasing Type 3
function bounds independently through resolved arrays, plus Poppler rendering.
Equal native color/alpha stops are separated by one representable float step
in krilla's stop offsets rather than emitting zero-width PDF intervals.

Visual oracle: installed **Poppler 26.01.0** (`pdftoppm -singlefile -png -r 96
fixture.pdf prefix`), white page background. Native CPU samples are straight
RGBA composited over white. Required mean absolute RGB error < **0.008** in
normalized channels; path-artwork silhouette disagreements must be within
**one pixel** of the CPU silhouette boundary. Poppler's conservative boundary
coverage and font hinting differ from the native exact-area renderer, so raw
visible IoU is diagnostic rather than an all-pixel parity claim. Raster fallback
also asserts decoded embedded RGB and alpha are byte-exact to the native
compositor. Image resampling in Poppler is measured separately. Ordinary rectangle
orientation is checked at independent inside/outside sample positions.

Fixtures: 160×128 RGB/U8 at 96 DPI; rect (16,20,120,88), matching ellipse,
ordered subtract hole (40,40)-(112,88), strokes width 4 with round cap/join,
width-relative dashes [3,2] and offset .5, center/inside/outside alignment;
linear/reflected/radial ramps with unsorted stops, reverse, midpoint .3,
independent alpha [.5,1], angle31°, scale .8, offset(.1,-.05).
Hard-stop variants use two color stops at .5 and two opacity stops at .25,
with reverse enabled for all three vector gradient styles.
Isolated group opacity .5; vector clip/mask (24,24,96,72); mask density .4/.7, feather3;
empty reveal/inverted masks; a pixel mask with .25 in its left section.
Appearance fixture has two fills/two strokes. Pattern is an original 2×2
red/blue checker, tested through the public appearance renderer, with an
explicit error when its resource is missing. No network in tests.

Generated reference PDFs/PNGs/text and CPU PNGs are scratch artifacts under
`target/refvec/pdf21` relative to the test process's directory (the disposable
registered crate by default). They are not inputs or committed golden assets.

RTX test: offscreen real PhotocraftApp, Vulkan/discrete/RTX 5090/Rgba32Float
assertions; actual pointer press/drag/release and canvas texture/frame readback,
max premultiplied channel difference ≤1/255. It clicks File→Export→PDF…,
options and Export, uses recorded file services, and checks an actual warning
notice. `pdf-import-integrated` adds the public package-22 import/roundtrip
comparison at the same threshold; absence of 22 is a missing gate, not a skip
or structural-only success. No interactive performance claim is made.
The original app fixture is a 256×256 white RGB/U8 canvas at 96 DPI with a
128×128 blue rectangle at (40,40), moved by a real pointer drag by (20,10).


## Native layer blend retention (FS008)

Synthetic GPL-3.0-or-later fixtures in `tests/export/blends.rs` retain twenty
small vector rectangles, their vector backdrop and a text layer beneath an
overlapping translucent blended shape. The 14 admitted non-Normal standard
modes are independently rendered with Poppler at native document DPI,
using the existing unchanged pixel/coverage thresholds. Structural assertions
require retained paths/type and native blend resources/styles, with no images
for vector-only cases. Additional fixtures distinguish isolated and pass-through
groups (including group fill opacity), and masked raster sources with Multiply
and Screen whose blend must apply exactly once against the original backdrop.

Negative controls retain warned whole-composition rasterization for Photoshop
Soft Light, Linear Dodge, Blend If, an adjustment, clipped stacking and a blended
layer with effects. Soft Light is deliberately excluded from native mappings:
the compositor uses Photoshop's square-root variant whereas CSS/PDF specifies
a different low-backdrop branch. Unsupported effects/artboard blend interactions
remain conservative; this change does not claim portable adjustment semantics.


FS008 sibling clipping source-atop fixtures (Local Image contributors, GPL-3.0-or-later):
`tests/export/clipping.rs` authors fifty independent vector objects plus native type,
solid vector clipping bases with fractional alpha/opacity/fill and one/two/three
blended siblings, even-odd/inverted geometry, stroke-over-clip order, cropped
pixel/feathered masks, raster alpha bases, visibility/orphans/empty bases, isolated
nesting, dependency guards and cancellation/output limits. Existing independent
renderer thresholds are unchanged. CPU interior alpha/color also has closed-form
source-atop assertions; SVG's independent RGBA output checks retained alpha.
Mask/base images are structurally cropped and distinguishable from whole-artwork
flattening. Existing clipped-text-base negative controls remain unsupported: this
stage admits constant solid-vector and raster base decomposition, not text-gamma
source reconstruction or general variable-alpha gradient decomposition.

Reviewer controls additionally retain the stroke of a compound-cancelled fill,
require initial cancellation to fail, and route tagged CMYK sources through the
existing document-profile-aware raster path. The CMYK control derives a valid
nondefault profile from the repository CC0 coated-CMYK profile by scaling only
A2B1 mft2 CLUT L* samples; it asserts active profile, changed source color, retained
alpha and the unchanged independent renderer thresholds. Native direct source
decomposition in this stage is limited to RGB documents and RGB raster sources.

CMYK cache-context follow-up: `tagged_cmyk_vector_and_text_cache_rebuild_matches_independent_cms_inks` spans three 256-pixel bands with RGB/CMYK solid and gradient paint plus RGB text. A synthesized nondefault profile changes both A2B and B2A behavior. Expected ink caches are populated from explicit canonical CMS Transform evaluations, never render_shape; text uses white glyph coverage only. Actual exporter cache regeneration must produce identical bytes regardless of whether those reference caches are present, and independent renderer/alpha assertions retain existing thresholds. pc-vector profile tests also force two Rayon workers and test captured None, restoration and unwind.

Uniform-color gamma-off text clipping uses public positioned glyph outlines (including counters and decorations), a single coverage boundary, existing masks, and native child blends. Gamma-off tests self-spawn their exact test in a deadline/reaped child process, preserving the parent global gamma and inherited private profiles/fonts. Default-gamma text remains guarded at the root; a supported closed isolated parent may rasterize only its bounded source, with outside50paths/native type preserved. Closed-parent tests include actual internal backdrop, opacity/fill/blend, masks, exact embedded RGB/alpha source samples, independent renderer comparisons and negative external-dependency/effect cases. Original numeric renderer thresholds remain.
