# Vector shapes fixture

`vector-shapes.pcraft` is a deterministic current-format (version 3) bundle containing a red
rectangle, a saved path, and a raster layer with a vector mask. Optional path fields
(`fill_rule`, `inverted`, component `op`, knot `smooth`) are omitted to exercise their serde
defaults. It keeps its small cached tile. It was re-saved on 2026-10-14 from the earlier
hand-made fixture with identical document content when v2 stopped reading older formats.
It is loaded with `include_bytes!` by `checked_in_shapes_fixture_with_missing_defaults_loads`
and the layout package tests; tests never rewrite it.
