# Pre-J17 reference values

`previous_backends.json` was captured from the original files at branch base `88a073c5`, before
extracting any token/font implementation.

The temporary reference harness used the old editor token constructors/style applicator, the old
Library defaults/style applicator, the original host `library_palette`, and the original Latin font
stack constructors. It retained egui 0.36.2 defaults and ran the same shaping sample for both profiles.
The original source copies and capture script remain under ignored `target/j17-baseline/` and
`target/j17-goldens.py` for local inspection; they are not production dependencies.

The fixture records every original public token field, full egui style Debug fingerprints (FNV-1a),
font fallback ordering, font bytes/tweaks, and Latin/symbol shaping dimensions. The `j17_` parity test
projects the shared union into each old vocabulary before comparing it. A mismatch requires an
explicitly reviewed visual change; regenerating the fixture from the new backend is not a parity proof.

SVG parity uses the frozen old recolouring/stroke/disabled algorithms against all registered artwork
at toolbar and fractional-DPI physical sizes, rather than storing thousands of redundant raster files.
The named-registry test separately checks the old first/last alias lookup rules and actual Image bytes.
