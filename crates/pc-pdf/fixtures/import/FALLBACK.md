Original Local Image contributors fixtures, GPL-3.0-or-later.

`fallback-quadrants.jp2`: a lossless 16×16 RGB image, top-left red,
top-right green, bottom-left blue, bottom-right yellow. Generated with Pillow
and its out-of-build OpenJPEG encoder (`irreversible=False`).

`fallback-checker.jb2`: a 16×16 bi-level image, top-left and bottom-right black,
other quadrants white. Original embedded JBIG2 page-info and lossless generic
region segments (ITU T.88 §7.2, §7.4.6, §7.4.8), with a Pillow/libtiff Group-4
MMR payload. The bi-level values are interpreted with JBIG2's black/white
semantics, rather than the TIFF photometric tag. No file header or end-of-file segment, per PDF embedded organization.

These are reference fixtures, not build dependencies. Analytic quadrant samples
are the independent fidelity oracle. The fixture writer in `source.rs` embeds
these streams in a conventional PDF page. No copied third-party images.
