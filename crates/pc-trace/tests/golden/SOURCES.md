# Potrace numerical stages

`potrace-stages.json` uses original rectangle, disk and staircase masks (CC0-1.0).
It records optimal-polygon indices and optimized curve controls from the actual
Potrace 1.16 C `src/trace.c`, GPL-2.0-or-later © Peter Selinger, with alphamax 1.0
and opttolerance 0.2. Input point coordinates are included.

Pin: potrace-1.16.tar.gz, SHA-256
`be8248a17dedd6ccbaab2fcc45835bb0502d062e40fbded3bc56028ce5eb7acc`.

Regenerate with `python scripts/generate-trace-stage-fixtures.py`. That script
writes/compiles the optional C oracle only under `target/refvec/trace-stages/`;
it is never part of Cargo, never a C dependency, and does not run a tracer CLI.
The Rust test checks polygon indices and curve controls within 1e-8. These
numeric stage fixtures are independent of the unavailable competitor SVGs.

The helper reconstructs the original unit-edge contours from three masks:
rectangle `[2,18) × [3,11)`, a radius-11 disk centred at `(16,16)` sampled at
pixel centres, and the staircase `x,y ∈ [2,22), x+y ≤ 23`. Each contour starts
at its lexicographically smallest vertex. It calls `calc_sums`, `calc_lon`,
`bestpolygon`, `adjust_vertices`, `smooth` and `opticurve` from unmodified
upstream `trace.c`, compiled with `-O0 -ffp-contract=off` and `curve.c`.
Unused corner controls are normalized to `[0,0]`.

The default uses the owner's cached archive at
`~/.local/share/local-image-dev/engine-sources/potrace/potrace-1.16.tar.gz`,
or downloads the [upstream archive](https://potrace.sourceforge.net/download/1.16/potrace-1.16.tar.gz)
into `target/refvec/trace-stages/`. `--archive /path/to/potrace-1.16.tar.gz`
works offline. Every archive is checked against the SHA-256 above before use.
`python3 scripts/generate-trace-stage-fixtures.py --check` rebuilds the oracle
and compares all JSON values without modifying the fixture.
