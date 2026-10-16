# Frozen native contract reference

Original Local Image test data, GPL-3.0-or-later, first frozen 2026-10-11 at format version 2
and re-frozen 2026-10-14 at format version 3 (v2 reads only the current format).
`document-v3.pcraft` is a small STORE ZIP whose manifest is mirrored by `document-v3.json`. It
extends the existing `../vector-basic.json` fixture with appearance items, unequal stroke widths,
brush/arrow settings, live shape/corner parameters, path text, a symbol master and
instance, constraints, a clipping path and a live blend with a spine. All IDs are fixed.

Generation provenance: `../../contracts_support/freeze_references.py`. That script
hand-authors the manifest from the basic fixture's mirror. It does not run tests,
application code or a GUI. The expected native Document is constructed independently in
`../../contracts_support/mod.rs::golden_document` and compared in full after loading.

When `FORMAT_VERSION` changes, re-freeze this reference at the new version with the script
and keep `golden_document` unchanged unless the document model itself changed.
