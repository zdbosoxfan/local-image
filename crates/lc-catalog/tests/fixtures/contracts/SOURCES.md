# Frozen catalog contract references

Original Local Image test data, GPL-3.0-or-later, frozen 2026-10-11.

- `catalog-v2.snap`: a photo with rating, picked flag, purple label, Unicode/hierarchical
  keywords, nondefault exposure/vibrance/profile, an album with cover, a renamed label
  and a browsed folder. Historical version 2; tests construct expected records independently.
- `catalog-v1.snap` + `catalog-v1.log`: frozen representation of the existing
  `src/tests_format_version.rs::legacy_parts` fixture (two JPEG photos, then rating 4).
- `catalog-v0.log`: frozen versionless journal case using that same legacy photo and
  a rating update, with CRC-protected records.

Provenance: `crates/pc-format/tests/contracts_support/freeze_references.py`. Legacy
fixtures omit additive fields so their defaults remain part of the migration contract.
Tests reopen again after migration to check upgraded snapshots, not just initial loading.
Keep these historical inputs unchanged when the format grows; add another version.
