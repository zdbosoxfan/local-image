# Frozen native contract reference

Original Local Image test data, GPL-3.0-or-later, frozen 2026-10-11. `document-v2.pcraft`
is a small STORE ZIP whose manifest is mirrored by `document-v2.json`. It extends the
existing `../vector-v1.json` fixture with appearance items, unequal stroke widths,
brush/arrow settings, live shape/corner parameters, path text, a symbol master and
instance, constraints, a clipping path and a live blend with a spine. All IDs are fixed.

Generation provenance: `../../contracts_support/freeze_references.py`. That script
hand-authors the manifest using the existing legacy fixture and freezes catalog
inputs from the existing migration cases. It does not run tests or invoke a GUI.
The expected native Document is constructed independently in
`../../contracts_support/mod.rs::golden_document` and compared in full after loading.

These are historical v2 bytes. Do not regenerate them when changing the format.
Keep the v1 fixture too, add a new versioned reference and migrate both old versions.
