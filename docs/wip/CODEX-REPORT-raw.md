# Raw-processing quality report — 2026-10-09

Continued `c80785f` (`WIP: Codex partial work (interrupted by PC reset, unverified)`).
Changes are left uncommitted for coordinator review. Work stayed offline, used rustup Cargo
1.98.1 with `~/.cargo/bin` first in PATH, and ran only one Cargo build at a time.
No client photographs were read, copied, committed or uploaded; no GUI was opened or stopped.
No forbidden production files or dependencies were changed.

## Implemented behavior

* Full darktable VNG4: four separate CFA colours, all 64 gradient terms, threshold/directional
  interpolation, delayed green merge; full and linear modes plus two median colour-smoothing
  passes are independently checked. Standalone `vng4` uses the full mode.
* Full AMaZE and dual-AMaZE, Bayer only. The interrupted version incorrectly separated scratch
  planes whose boundary values survive upstream storage reuse: BGGR/GRBG errors reached
  0.024102286 / 0.021692961. A safe indexed shared buffer now retains the exact plane lifetimes,
  tile overlaps, Nyquist refinement, diagonal interpolation and mirror borders. All four Bayer
  phases match darktable exactly in the reference samples. Crops smaller than 33 pixels are
  extended by parity-preserving reflection before the kernel and cropped afterwards.
* `dualRcdVng` and `dualAmazeVng` use darktable's four-colour VNG-linear + two median passes,
  exact Scharr/detail-mask exponential approximation and disc-truncated 9x9 Gaussian. The
  existing `dualRcd` RCD + bilinear decode and historical mask retain their arithmetic and
  are hidden from `Demosaic::ALL`; the UI adds that option back only when it is selected.
  Threshold controls and cache keys cover all three duals. A source comparison confirms the
  legacy dual/mask body matches the WIP commit byte-for-byte.
* Segmentation-based CFA highlight reconstruction: opposed CFA fallback, morphological
  closing including upstream radius-8 footprint, scanline flood fill and border labels,
  segment candidates, all-clipped Euclidean distance transform, gradient-ring propagation,
  Gaussian/box smoothing, attenuation, all seven recovery modes and deterministic Poisson
  noise. Upstream's aliased recovery scratch border also matters and is preserved. The raw
  API exposes the full options; the Develop menu uses upstream defaults: combine 2,
  candidating 0.4, recovery off, strength/noise 0. GUI diagnostic masks/chroma caches and late
  D65 host correction are omitted. Temporary WB is undone; unclipped source bits are preserved.
* Full decode hook runs after normalization/opcode lists 1/2 and before demosaic/opcode list 3.
  Bayer previews average each of the four CFA phases separately at twice the RGB output size,
  retain a clipped phase's maximum, reconstruct that reduced CFA with scaled morphology, then
  bin to RGB. X-Trans reconstructs its full CFA before binning. RGB/monochrome input falls back
  to opposed. The hook clip argument is an effective normalized threshold (.99), consistent
  with the existing host; it is not darktable UI's multiplier before its .987 module magic.
  Reference shims use magic 1 to pass the same effective threshold to the kernel. Crops, phase shifts, patterned black levels, clipping, hook mutation, opcode
  ordering and full-size fallback for list 3 have focused tests.
* New serialized options/menu entries: `vng4`, `amaze`, `dualRcdVng`, `dualAmazeVng`,
  `segmentation`. Default options/key zero and existing default render hashes are unchanged.
* Existing X-Trans capture-radius scan corrected from `sy + 2` to `sy + 3`: upstream's search
  increments its row after finding the solitary green. Test scan radii change from
  0.629398227 to 0.629392326 (before the public X-Trans +0.2 adjustment). Bayer/monochrome
  scans are unchanged. This is a deliberate fidelity fix for capture-enabled X-Trans;
  existing goldens have capture off and needed no rerecording.

## Numerical fidelity

Reference sources: darktable `733bd69f32cac7ff5e41025115942772add1f088`, RawTherapee
`5f486d3678b34c74ba0c63571c17babe20935019`. Standalone GCC/G++ harnesses extract the upstream
numeric functions and live only in ignored `target/refvec/`. Cargo compiles pure Rust.
Fixtures are deterministic little-endian float arrays, not images. All 82 fixtures were
regenerated from the documented recipes and retained identical SHA-256 hashes.

Complete extraction recipes, host shims, regeneration commands and assertions:
[raw README](../../crates/lc-raw/tests/fixtures/README.md),
[pipeline README](../../crates/lc-pipeline/tests/fixtures/README.md).
Small host shims disable GUI/OpenMP, supply numeric types and deterministic allocation, and
adapt C's finish-goto to an equivalent enclosing C++ conditional. The AMaZE kernels retain
upstream shared allocations. RCD excludes a 16-pixel border so its PPG border stub is unused.

Absolute errors are in normalized camera samples unless the row states otherwise.

| Port/reference | Samples / coverage | Maximum error | RMS error | Assertion tolerance |
|---|---|---:|---:|---:|
| VNG4, darktable full / linear / linear + medians | 4 Bayer phases × 3 modes × 64² RGB, including borders | 0 | 0 | 2e-7 |
| AMaZE, darktable | 4 phases × 49,455 floats from 255×251, edges and tile seams | 0 | 0 | 2e-6 |
| AMaZE, RT scalar, after its negative clamp | RGGB/BGGR | 1.19209e-7 | 5.25614e-9 | 2e-6 |
| AMaZE, RT scalar, after its negative clamp | GRBG/GBRG | 1.00210e-6 | 8.77544e-9 | 2e-6 |
| New upstream dual mask | 64² | 0 | 0 | 2e-7 |
| Segmentation, neutral WB | 4 Bayer + X-Trans × 7 modes × 3,840 CFA samples from 192×180 | 0 | 0 | 2e-6 |
| Segmentation, WB [1.7, .85, 2.3] | 5 patterns × Off/AdaptiveFlat, same crop | 5.96046e-8 | ≤1.4e-8 | 2e-6 |
| RCD, site-corrected upstream packing | 4 phases × 3,072 interior RGB floats | 0 | 0 | 2e-6 |
| Opposed, channel-corrected upstream mask | 96² RGB, 9,216 sparse floats | 1.19209e-7 | 2.6e-8 | 2e-6 |
| Capture-radius scans | Bayer 4 phases, mono, X-Trans × textured/clipped/flat 128² | 0 | 0 | 2e-7; matching flat infinities |
| Capture RL | 64², 8 iterations, σ .30/.70/1.50, zero blend locations and borders | 1.132e-6 | 1.650e-7 | 3e-6 |
| Negadoctor pixel kernel | 256 RGB transmissions; non-neutral WB/offset, negatives/zero/HDR | 1.78814e-7 | 1.9e-8 | 1e-6 |
| CAT adaptation to D65 | Bradford linear/nonlinear, CAT16, XYZ; 256 triples each, including negative blue | 2.38419e-7 | 3.5e-8 | 5e-7 |
| Toneeq least squares, same float-built A promoted to double | 3 sigmas, 9×8 matrices and eight coefficients | approximately 1e-12 | — | 2e-8 |
| Toneeq correction, same coefficients | 3×513 EV samples | 0 | 0 | 2e-6 |

Measured intentional differences, retained rather than relabeled rounding error:

* RCD's existing site-local high-pass parity differs from darktable's neighbouring-column
  packed lookup. Against **unmodified** darktable: max/RMS 0.070446610 / 0.014404683 for
  RGGB/BGGR; 0.068227410 / 0.004709468 for GRBG/GBRG. The site-corrected C reference differs
  only in that index and matches Rust exactly. Production RCD was not changed in this task.
  Rust's reflected border is also an existing difference; the RCD fixture measures the interior.
* The legacy dual mask differs from the upstream mask by max 0.249582440, RMS 0.019573418
  on the 64² fixture, due to its historical separable blur, exponential and borders. It is
  deliberately retained for saved `dualRcd`; the new options use the exact upstream mask.
* Opposed's existing per-channel mask fixes upstream's use of `input[idx]` for every channel.
  On a clipped blue/HDR green disc with red unclipped: difference from unmodified upstream
  max 0.164680600, RMS 0.036349160. The C reference corrected only to `input[idx+c]` matches
  within 2e-6. Expanded edge-mask bounds, rounded-up superpixel grid and preserving unclipped
  samples are existing differences; this crop avoids the grid/border differences.
* AMaZE preserves finite negative/HDR outputs like darktable. RT clamps negatives: raw
  unclamped-vs-RT max/RMS 0.173294693 / 0.001099004 for RGGB/BGGR and
  0.181304038 / 0.001062812 for GRBG/GBRG. After the clamp the table's tight bounds apply.
  This avoids discarding scene-linear information in our decode.
* Capture RL accumulates equivalent symmetric kernel groups in a different order. The small
  measured rounding error is bounded above; Rec.2020 luminance and the later capture stage
  remain existing host differences.
* Colour calibration targets D65 rather than upstream's pipeline D50. The independent fixture
  extracts upstream's D65 functions and uses identity host matrices to isolate adaptation;
  existing tests cover Rec.2020 conversions. This is an adaptation fixture, not a complete
  channel-mixer or gamut-compression port claim.
* The complete toneeq curve uses a double-built interpolation matrix and a 256/EV interpolated
  LUT. Against the float-built upstream solve/correction: max 2.38419e-7 at σ .7,
  9.53674e-7 at √2, 4.4584e-5 at 2 (bounded by 6e-5). Solve/correction match when supplied
  the same matrix/coefficients. The guided log-luminance mask remains an existing host
  difference from darktable's linear-luminance guided/EIGF mask; it was outside this raw task.

## Synthetic quality

`crates/lc-raw/examples/quality.rs` is the reproducible measurement tool. Charts are 256²,
analytic zone plate, 48-cycle Siemens star and slanted colour bars, with 4×4 pixel-area
supersampling before point CFA sampling. RGGB, normalized peak 1, 16-pixel metric exclusion.
False colour is RMS error in the two colour differences R−G and B−G (zero on grey charts).
Each cell below is **PSNR dB / chroma RMSE**. These are controlled stress charts, not a
claim of universal camera/image ranking.

| Method | Zone plate | Siemens star | Colour edges | 24 MP ms |
|---|---:|---:|---:|---:|
| Bilinear | 13.8911 / .215497 | 23.6265 / .066990 | 28.6083 / .050571 | 25.421 |
| PPG | 16.5280 / .182582 | 28.2849 / .042316 | 27.6153 / .050594 | 57.415 |
| AHD | 22.3092 / .081453 | 32.6014 / .025700 | 26.3288 / .061230 | 209.002 |
| RCD | 19.5045 / .129267 | 31.3937 / .031068 | 33.2632 / .028115 | 56.869 |
| Legacy RCD + bilinear | 19.3872 / .129187 | 31.3989 / .031003 | 30.8530 / .038196 | 218.414 |
| Full VNG4 | 15.1557 / .150876 | 26.0844 / .042105 | 30.1527 / .040839 | 353.205 |
| RCD + VNG-linear + medians | 19.5054 / .122666 | 31.4408 / .030364 | 31.0951 / .039464 | 633.690 |
| AMaZE | 25.7647 / .065523 | 34.5023 / .024223 | 28.9284 / .045983 | 1287.933 |
| AMaZE + VNG-linear + medians | 25.5186 / .064298 | 34.5917 / .023431 | 29.1956 / .047727 | 1860.140 |

Measured choice of dual's low branch: same detail mask/high method/threshold .2, replace
VNG-linear by **full darktable VNG4**, retain two median passes. Results for full + medians:

| High method | Zone plate | Siemens star | Colour edges |
|---|---:|---:|---:|
| RCD | 19.7045 / .123354 | 31.4986 / .030423 | 32.0037 / .032900 |
| AMaZE | 25.7721 / .065063 | 34.6897 / .023436 | 29.7415 / .042386 |

Full VNG raises PSNR and improves the colour-edge result, but slightly raises monochrome
false colour. The shipped dual retains darktable's faithful linear + smoothing choice and
its lower monochrome false colour; full VNG4 remains available standalone. This measurement
is **darktable full VNG**, not an independent RT VNG result. RT's dual source calls full VNG4;
the supplied `rtengine/demosaic_algos.cc` has no VNG4 implementation (nor do the other supplied
RT files). That exact RT comparison could not be completed offline.

## Highlight previews and timings

768×512 warm gradient (RGB ratios 1/.7/.5) with a vertical sine modulation, brightness .2–2,
clipped at sensor white 1, reconstruction clip .99, defaults, RCD full decode. Segmentation
full-versus-unclipped truth: 16.954458 dB / .124099108 chroma RMSE. Fully clipped data and a
long brightness ramp are intentionally difficult; the method cannot infer all lost signal.
Previews are compared to a box-resampled **full reconstructed** result, excluding four output
pixels for PSNR/chroma; max error includes the borders.

| Bin factor | Output | Preview/full PSNR dB | Chroma RMSE | Max channel difference | 24 MP normalization + CFA hook + bin ms |
|---|---|---:|---:|---:|---:|
| 2 | 384×256 | 62.811417 | .000892535 | .069107056 | 1156.625 |
| 4 | 192×128 | 47.713425 | .004375980 | .072544098 | 297.514 |
| 6 | 128×85 | 41.841734 | .008284384 | .091331244 | 139.813 |
| 8 | 96×64 | 38.307483 | .012538010 | .101134658 | 86.032 |

24 MP full-CFA segmentation alone: **1066.815 ms**, excluding normalization/demosaic.
Bayer previews reconstruct reduced CFA to reduce latency; segment topology and candidate
selection change with sampling, so this is a measured approximation. X-Trans's full CFA hook
preserves reconstruction sampling at a higher preview cost.

Timing environment: AMD Ryzen 7 9800X3D, eight Rayon workers, offline Rust 1.98.1 release
(thin LTO), 6000×4000 synthetic sensor, one run per method. Demosaic times exclude raw parsing,
normalization, matrix/WB, resizing, capture sharpening and rendering. Segmentation uses a
clipped warm-ramp sensor; the other methods use a deterministic textured gradient.
AMaZE is currently scalar across tiles, so its CPU cost reflects fidelity rather than a
parallel tile optimization. Absolute times are machine/run dependent.

Reproduce (run Cargo commands sequentially):

```sh
PATH="$HOME/.cargo/bin:$PATH" CARGO_INCREMENTAL=0 RAYON_NUM_THREADS=8 \
  cargo +1.98.1 run --offline --release -p lightcraft-raw --example quality -- --timings
PATH="$HOME/.cargo/bin:$PATH" CARGO_INCREMENTAL=0 \
  cargo +1.98.1 test --offline -p lightcraft-raw --lib full_vng_vs_linear_dual_quality -- --ignored --nocapture
```

## Validation and GPU review

Final required tests passed: **774 passed, 0 failed, 8 ignored** across the five requested
crates and their integration tests (28 develop, 302 engine, 148 pipeline, 101 raw unit,
29 raw integration, 166 UI). Existing engine/pipeline default goldens remain unchanged.
The ignored raw chart measurement was additionally run successfully. Formatting with
`cargo +1.98.1 fmt -p ... --check` and all-target Clippy with `-D warnings` passed.
Build logs remain in ignored `target/refvec/{required-tests,clippy}.log`.

```sh
PATH="$HOME/.cargo/bin:$PATH" CARGO_INCREMENTAL=0 cargo +1.98.1 test --offline \
  -p lightcraft-raw -p lightcraft-engine -p lightcraft-pipeline -p lightcraft-develop -p lightcraft-ui-egui
PATH="$HOME/.cargo/bin:$PATH" cargo +1.98.1 fmt \
  -p lightcraft-raw -p lightcraft-engine -p lightcraft-pipeline -p lightcraft-develop -p lightcraft-ui-egui --check
PATH="$HOME/.cargo/bin:$PATH" CARGO_INCREMENTAL=0 CARGO_TARGET_DIR=target/clippy \
  cargo +1.98.1 clippy --offline -p lightcraft-raw -p lightcraft-engine -p lightcraft-pipeline \
  -p lightcraft-develop -p lightcraft-ui-egui --all-targets -- -D warnings
```
All commands use `--offline` for builds and run serially. Incremental compilation was disabled
because artifacts left by the reset initially caused undefined LLVM symbols at link time;
non-incremental builds succeeded. The existing build-script notice about unavailable embedded
craft-fonts remains; it is unrelated to this task.

GPU test added: `lc-engine::tests_gpu::new_cfa_methods_and_segmentation_render_on_the_gpu`:
VNG4, RCD/VNG dual, AMaZE, AMaZE/VNG dual × default/segmentation highlights × full/binned
preview = 16 cases, same mean <0.5 LSB / max ≤3 LSB bounds as existing engine GPU tests.
It uses the existing GPU-state test lock and explicitly skips without an adapter. The sandbox
run passed with `skipped: no GPU adapter`, reporting the llvmpipe software adapter was skipped;
none of the 16 numeric GPU cases ran. Its log is `target/refvec/gpu-review.log`. Coordinator
should run this test on the RTX 5090 with `--nocapture` and check that it did not skip.

No new WGSL was added: raw normalization, demosaic and CFA reconstruction run on CPU before
both renderers split. There is no GPU CFA stage to update; `lc-gpu` is explicitly forbidden by
the task scope and remains untouched. A GPU implementation of these kernels would require a
separate scope/change to the decoding boundary. The added test checks the actual existing GPU
render path with the new decoded sources rather than unused shaders. GPU numerical execution
could not be verified in this sandbox.

Remaining reference limitation: independent **RawTherapee full VNG4** comparison requires its
missing source. All supplied darktable kernels and RT AMaZE were checked. Real-camera visual
review was not performed; only deterministic synthetic/reference inputs were used.
