# V1 vector compatibility fixture

`vector-v1.pcraft` is a deterministic format-version-1 bundle containing a red
rectangle, a saved path, and a raster layer with a vector mask. Optional path
fields (`fill_rule`, `inverted`, component `op`, knot `smooth`) are omitted to
exercise their legacy serde defaults. The bundle uses only the existing v1
content variants and keeps its small cached tile. It is loaded with `include_bytes!`
by `checked_in_v1_fixture_with_missing_defaults_loads`; tests never rewrite it.
