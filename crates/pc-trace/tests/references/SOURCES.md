# Competitor references (coordinator)

The inherited branch contains 54 matched Logo SVGs from each of vtracer-alpha4 and
vtracer-065: three synthetic cases, three sizes (256/512/1024), six variants.
The alpha4 directory also contains one additional unmatched SVG. Integration
re-scores the explicitly scoped 18-case Cutout gate (three cases, 256px, six
variants) against these raw SVGs; this does not establish the complete B.4
population or the other presets. The original generation parameter record and
`scores.json` were not supplied, so their provenance is not reconstructed.
No new competitor reference SVGs were generated during integration.
Normal tests explicitly print which competitor/preset gates are unavailable.
A complete tool suite is required to compare its medians; partial files do not
silently change the image population. Other robustness properties still run.

Expected layout: `<vtracer-alpha4|vtracer-065|potrace-116>/<preset>/<case-id>.svg`.
Presets: logo, black_white, few_colors, silhouette, photo, pixel_art. Potrace uses
the B&W subset only. Case IDs are `logo-00-256-clean`, etc. The reader re-renders
committed SVGs using resvg and re-scores them using exactly the same metrics as
our output. `scores.json` records generation commands' tool versions, parameters,
case lists and scores; raw stored scores are provenance, never a scoring shortcut.

Run outside the sandbox, with vtracer 1.0.0-alpha.4, potrace 1.16 and Inkscape
1.4.4 on PATH (no GUI window is opened):

```
PATH=~/.cargo/bin:$PATH CARGO_BUILD_JOBS=3 TRACE_REF_FAST=1 cargo +1.98.1 test --offline -p pc-trace --test it regenerate_competitor_references -- --ignored --nocapture
```

Omit TRACE_REF_FAST for the full matrix. Set VTRACER_065 to a separate vtracer
0.6.5 executable; otherwise the harness tries `vtracer-0.6.5` and clearly skips it
if unavailable. Inputs and raw tool results go under `target/refvec/trace/`.
The harness invokes vtracer with corresponding clustering, max-colours, speckle
and fitting mode; Potrace with the same fixed threshold and alphamax/opttolerance;
then Inkscape exports plain SVG. vtracer 0.6.5 lacks max-colours/watershed and is
run in its corresponding legacy colour/binary and pixel/spline mode.

Commit the SVGs and scores.json with this source record updated to the exact
versions printed. Then run:

```
PATH=~/.cargo/bin:$PATH CARGO_BUILD_JOBS=3 cargo +1.98.1 test --offline --release -p pc-trace --test it full_quality_benchmark -- --ignored --nocapture
PATH=~/.cargo/bin:$PATH CARGO_BUILD_JOBS=3 cargo +1.98.1 test --offline --release -p pc-trace --test it release_performance_gates -- --ignored --nocapture
```

Gates: median fidelity within 0.005 of the best competitor; per-image wins at
least 80% for Logo/B&W, 70% otherwise; median nodes within 1.2× the closest-fidelity
competitor; Logo 1024² below 1.5s and Photo 2048² below 8s in release on the
coordinator machine. Update ../corpus/SOURCES.md with the measured fitter decision.
