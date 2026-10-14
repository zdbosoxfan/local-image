# Frozen catalog contract references

Original Local Image test data, GPL-3.0-or-later, frozen 2026-10-11.

- `catalog-v2.snap`: a photo with rating, picked flag, purple label, Unicode/hierarchical
  keywords, nondefault exposure/vibrance/profile, an album with cover, a renamed label
  and a browsed folder. Historical version 2; tests construct expected records independently.
- `catalog-v1.snap` + `catalog-v1.log`: a format-v1 library (two JPEG photos, then rating 4),
  as builds before Local Image v2 wrote it.
- `catalog-v0.log`: a versionless log-only journal (the earliest builds) with the same photo
  and a rating update, CRC-protected records.

Provenance: `crates/pc-format/tests/contracts_support/freeze_references.py`. v2 does not read
or migrate older formats (owner decision, Oct 14 2026): the v1 and v0 files are kept only as
inputs that must be refused untouched (`CatalogError::Older`). Keep them unchanged.
