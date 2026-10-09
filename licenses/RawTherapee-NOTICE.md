# RawTherapee algorithm attribution

Local Image is GPL-3.0-or-later (see `LICENSE`). AMaZE, RCD, dual demosaicing and capture
sharpening originated in [RawTherapee](https://github.com/RawTherapee/RawTherapee),
GPL-3.0-or-later. Production Rust ports use the darktable forks listed in
[PORTS](../docs/PORTS.md) and [darktable notice](darktable-NOTICE.md).

AMaZE (Aliasing Minimization and Zipper Elimination): copyright (c) 2008-2010
Emil Martinec; optimized for speed by Ingo Weyrich; incorporating ideas of Luis Sanz Rodrigues
and Paul Lee. The upstream source credits code dated May 27, 2010 and Ingo Weyrich's
January 25, 2016 modification. This is free software under GNU GPL version 3 or any later
version, supplied without warranty; see the repository `LICENSE` for the licence text.

The port `crates/lc-raw/src/demosaic/amaze.rs` follows darktable's complete scalar kernel.
Independent out-of-build scalar fixtures also extract RawTherapee
`rtengine/amaze_demosaic_RT.cc` at `5f486d3678b34c74ba0c63571c17babe20935019`.
`rtengine/rt_math.h` / `sleef.h` supply interpolation and scalar exponent helper semantics.
Fixtures account for RawTherapee's output scaling and negative-output clamp.

RCD: Luis Sanz Rodríguez; tiling by Ingo Weyrich. Dual demosaicing and capture sharpening:
Ingo Weyrich. `rtengine/dual_demosaic_RT.cc` at the same revision was read to compare its
full-VNG4 low branch with darktable's VNG-linear + two median passes. RawTherapee's actual
VNG4 kernel is absent from the provided offline reference files, so an independent RT VNG4
numerical comparison remains pending; see [CODEX-REPORT](../docs/wip/CODEX-REPORT.md).

No RawTherapee C/C++ is compiled or shipped as a dependency. Upstream functions run only in
ignored `target/refvec` harnesses to produce deterministic numerical fixtures. Complete
regeneration steps are in [raw fixture README](../crates/lc-raw/tests/fixtures/README.md).
