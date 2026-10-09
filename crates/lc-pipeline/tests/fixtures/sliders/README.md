# Primary slider reference vectors

Generated from **unmodified upstream C function bodies**, darktable
`733bd69f32cac7ff5e41025115942772add1f088`. The extraction script supplies allocator,
OpenMP and host-profile adapters; it does not use the Rust implementation for expected values.
The workspace remains all Rust. Generated C/executables live only in ignored
`target/refvec/sliders/`. Fixture SHA-256 values are in `manifest.json`.

Regenerate offline with gcc:

```
python3 scripts/engine_sliders_refvec.py /path/to/pinned/darktable
```

The default source path is the owner's read-only engine reference directory. Compilation:
`gcc -O0 -ffp-contract=off … -lm`; SIMD/OpenMP dispatch is suppressed in the host adapters.
The LLF allocator has spare unused SIMD lanes, as upstream reads a fourth lane which does
not contribute to its three-lane stencil. Rust bounds that lane and tests every output.

| Fixture | C source / coverage | Rust tolerance |
|---|---|---|
| `eigf.csv` | `eigf.h`, `gaussian.c`, `fast_guided_filter.h`; 37×29, three iterations, σ1.7/6.3, self/quantized guidance and linear/geometric blend | absolute <3e-6, all 4,292 samples |
| `llf.csv` | `locallaplacian.c`; complete 50×37 padded pyramid, identity, shadows/highlights, clarity | absolute <3e-6, all 5,550 samples |
| `ucs.csv` | `colorspaces_inline_conversions.h`; JCH↔HSB↔xyY, 41 samples from −12 to +8 EV | absolute/relative 4e-5 |
| `colorequal-filter.csv` | `colorequal.c`; complete UV prefilter and saturation/brightness guided correction with real Gaussian covariance filters | absolute <5e-6, 2,146 two-channel pixels |
| `colorequal-rbf.csv` | `colorequal.c`, `choleski.h`; full periodic RBF LUT, signed and positive/clipped paths | absolute <8e-6, 1,024 samples |
| `balance.csv` | complete unchanged `colorbalancergb.c` pixel body, D65 Rec.2020 host matrices, deterministic varying 512-bin gamut LUT, both UCS and Jz formulas, all masked chroma/saturation/brilliance parameters, hue rotation and grading | absolute/relative 4e-4, 82 RGB samples |
| `jz.csv` | current stable `dt_XYZ_2_JzAzBz` and inverse, −12 to +8 EV | forward 6e-6; inverse absolute/relative 3e-4 |

The RBF host replaces only `_get_hue_node` with evenly spaced positions; Rust's LR
extension supports arbitrary UCS hue nodes. Rust solves the original matrix in f64
with partial pivoting instead of squaring its condition number in normal equations.
Jz forward PQ uses f64 direct evaluation; the upstream near-neutral difference-based
implementation and HDR inverse are checked here. Slider-to-parameter mapping, log-EV
LLF remapping, colour/B&W defaults and image-relative radii are product calibrations,
not claims that darktable presets/sliders render identically.

vkdt's GLSL construction is covered by step ordering, fine-detail, ratio/hue, clipping,
proxy, layer and cache tests, rather than executing GLSL as a C reference. Its BSD-2
notice is in `licenses/vkdt-NOTICE.md`. Skin Tone is an original LI algorithm.

Independent CIEDE2000 formula checks use four published pairs from
[Sharma, Wu & Dalal (2005)](https://hajim.rochester.edu/ece/sites/gsharma/ciede2000/ciede2000noteCRNA.pdf).
