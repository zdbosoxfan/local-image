# Potrace 1.16

Ported from Potrace 1.16 © Peter Selinger.
Copyright (C) 2001-2019 Peter Selinger.
GPL-2.0-or-later; the complete upstream licence is in [potrace-COPYING](potrace-COPYING).
Local Image distributes the combined application under GPL-3.0-or-later.

`crates/pc-trace/src/potrace.rs` ports `src/trace.c`: calc_sums, calc_lon,
penalty3, bestpolygon, pointslope, adjust_vertices, smooth, opti_penalty and opticurve.
The bitmap decomposition is our region boundary extraction. Unused backend/debug
curve fields are omitted. Coordinates use document pixels, y down.

Source: [Potrace](https://potrace.sourceforge.net/), `potrace-1.16.tar.gz`.
SHA-256: `be8248a17dedd6ccbaab2fcc45835bb0502d062e40fbded3bc56028ce5eb7acc`,
matching the [upstream SourceForge archive metadata](https://sourceforge.net/projects/potrace/files/1.16/potrace-1.16.tar.gz/download).

There is no Potrace dependency, build script, C object or unsafe Rust in the application.
The optional numerical fixture recipe compiles an upstream oracle under `target/refvec/`,
which is outside the Cargo build. The fixture inputs are our original shapes.
