# Colour/tone reference vectors

Regenerate offline from the coordinator's read-only source archive:

```sh
python3 scripts/engine_colour_refvec.py /home/zdavidson/.local/share/local-image-dev/engine-sources
```

The script extracts upstream functions verbatim, supplies standalone types/main, compiles with
`gcc`/`g++ -O0 -ffp-contract=off`, and runs under `target/refvec/{sigmoid,basecurve,dcp,illuminants}`.
Cargo never invokes C/C++ or links these binaries. Only small CSV results and regeneration code
are tracked. darktable source: `733bd69f32cac7ff5e41025115942772add1f088`.

- `sigmoid.csv`: 39 evaluations of `commit_params` and `_generalized_loglogistic_sigmoid`
  (negative/zero, grey, shoulder, extreme values; three contrast/skew/black/white settings),
  plus 27 pixels from `_desaturate_negative_values`, `_pixel_channel_order`, and
  `_preserve_hue_and_energy` (ties, black, negative channels, primaries; hue 0/50/100%).
  Rust absolute tolerance: **2e-6** in linear RGB. These test the analytic port independently
  of the log-spaced runtime tone table and its intentional black-point extension.
- `basecurve.csv`: every imported camera/maker preset evaluated at 33 points with upstream
  `monotone_hermite_set` and `catmull_rom_val` from `src/common/curve_tools.c`.
  Absolute tolerance: **8e-7**. The extra asymptotic shoulder above 90% is our extension,
  tested separately; it deliberately replaces upstream extrapolation followed by clipping.

See `licenses/darktable-NOTICE.md` for authors, licence and differences. This is numeric parity,
not a claim of visual parity with proprietary raw processors.
