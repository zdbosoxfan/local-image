# Camera data spot checks

`camera-matrices.csv` contains exact A and D65 arrays read with Python `tomllib` from six
maker TOMLs in rawler (dnglab v0.8.0). `tests_camera_matrices` compares all entries exactly,
checks key ordering and verifies that file-provided matrices take precedence. Regenerate the
table with `python3 scripts/engine_colour_data.py ENGINE_SOURCES`. Regenerate the independent
spot fixture with:

```sh
python3 scripts/engine_colour_refvec.py /home/zdavidson/.local/share/local-image-dev/engine-sources
```

The script reads `canon/1000d.toml`, `nikon/1aw1.toml`, `sony/a1.toml`, `fuji/e550.toml`,
`panasonic/cm1.toml` and `olympus/c5050z.toml` directly. It never samples the generated Rust table.
Columns are make, model, illuminant code (17=A, 21=D65), then the nine row-major coefficients;
CSV quoting preserves Olympus's comma. There is no tolerance: these data must match exactly.

The immutable archive-tree identity and aggregate input SHA-256 are recorded in
`data/colour-sources.json`. DCP file rights and SHA-256 values are in `data/dcp/manifest.json`.
See `licenses/rawler-NOTICE.md` and `licenses/rawtherapee-NOTICE.md`.
