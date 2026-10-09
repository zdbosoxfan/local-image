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
