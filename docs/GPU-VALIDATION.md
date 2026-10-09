# GPU render path on real hardware (Develop toolset, October 2026)

The develop GPU renderer (`lightcraft-gpu`) and the compositing GPU renderer (`photocraft-gpu`)
were written in a container without a GPU adapter, where every GPU test skipped itself. This
records their first run on a real GPU, the tests added for the §4.1 toolset
([DEVELOP-DESIGN.md](DEVELOP-DESIGN.md)), the bugs found and what still needs a human eye.

## Machine

- NVIDIA GeForce RTX 5090, **Vulkan**, driver NVIDIA 615.71.09 (wgpu picks it as the
  high-performance adapter). Also present: AMD Radeon 610M (RADV, Mesa 26.2.3) and llvmpipe, not
  used by the renderer.
- AMD Ryzen 7 9800X3D (16 threads), Fedora 44, Rust 1.98.1.

The GPU tests print the adapter (`toolset.rs`); no test skipped.

## Tests

| Suite | Result |
| --- | --- |
| `lightcraft-gpu` unit tests | 10 passed, 1 ignored (`bench_kernels`) |
| `lightcraft-gpu` `equivalence.rs` (incl. layer opacity, masks, overlays, output spaces) | 10 passed, worst max 1 LSB (2 LSB on the point-colour overlay) |
| `lightcraft-gpu` `fallback.rs` (incl. `layers_need_cpu`, new absurd-size case) | 3 passed |
| `lightcraft-gpu` `memory.rs` | 1 passed |
| `lightcraft-gpu` `toolset.rs` (new) | 12 passed, 1 ignored (`bench_toolset_24mp`); 187 comparisons, all max ≤ 1 LSB |
| `photocraft-gpu` | 45 passed (after the Normal-blend fix below) |
| `lightcraft-engine` (incl. new `tests_gpu.rs`, golden hashes) | all passed |
| `lightcraft-pipeline` (golden hashes `tests_toolset`, `tests_layers`) | all passed, CPU output unchanged |

New GPU ↔ CPU equivalence coverage (`lightcraft-gpu/tests/toolset.rs`, bounds as `equivalence.rs`):

- **Lens profiles** (lens database): poly3 / poly5 / ptlens distortion, linear and poly3 TCA,
  vignetting, off-centre axis; each part and all together at 0 %, 100 %, 200 %; with crop,
  straighten, flips, orientations, perspective + constrain crop, manual CA, edits and a mask;
  strength changes on a cached view. The original RTX run resampled geometry on the CPU;
  the native GPU warp now evaluates these models (RTX rerun described below).
- **Tone equalizer**: `render` returns `None` with the reason; the Show Mask overlay
  (`Overlay::ToneEqMask`) falls back too, even with every zone at 0, and draws a grey mask that
  follows the image; with the section off the GPU renders again.
- **Colour calibration**: CAT16 / linear Bradford / XYZ × six illuminants with gamut 0 and clip off
  (GPU, folded into the WB matrix) on raw and rendered sources; non-linear Bradford, gamut > 0,
  clip and the defaults (CPU linear stage, then GPU); with WB, edits and a saturated scene;
  switching between the two paths on a cached view.
- **Capture sharpening**: several radius / threshold / iterations / corner-boost values, a binned
  preview, a rendered source; on one cached view each change (radius, threshold, iterations, corner
  boost, off, on) invalidates the sharpened source and downstream stages (cached == fresh within
  the CPU bounds), and going back to the first values gives the first image. The original RTX
  run used CPU capture; the new native GPU implementation is awaiting the rerun described below.
- **Depth masks**: three bands, combined with an inverted linear, opacity, inverted mask, rotation
  + crop, and the mask overlay.
- **Film looks**: all eight `lc.filmsim.*` profiles at 50 / 100 / 200 % on a raw and a rendered
  scene, with edits, and in Display P3 — GPU render vs CPU export.
- **AI Remove**: a stub patch (gradient, soft alpha) at 100 % / 40 % opacity, with a
  heal spot, edits and geometry, added / removed on a cached view, a missing patch.

Through the engine (`lightcraft-engine/src/tests_gpu.rs`, a view `RenderJob` with its stage cache):
raw options RCD / Dual / Opposed / Clip each decode again and render on the GPU, and switching back
to Default gives the original image bit for bit; a real lensfun-database lens at 0 / 100 / 200 %
with crop + rotation; AI Remove (mock engine) through the commands.

Real raw files (three Sony ILCE-1 ARW, 3744 × 5616, Tamron 35-150 f/2-2.8 found in the lens
database), rendered at 3333 × 5000 through the engine's job path: default, typical edit, lens
profile 100 % and 200 % + crop/rotate, tone equalizer, colour calibration linear and mixed, capture
sharpening, RCD + Opposed, Dual + Clip, two film looks — 36 of 36 within max 1 LSB of the CPU.

## Bugs found

1. **`photocraft-gpu` Normal blend rounded differently from the CPU** (`parity::adjustment_layers`
   failed: Hue/Saturation on an 8-bit document, 230 vs 229). `compose.wgsl` always used the general
   blend formula; the CPU (`psblend::composite`) has a Normal fast path. Equal in exact arithmetic,
   1 ulp apart in f32, which flipped the adjustment's 8-bit rounding at a pixel sitting on the
   boundary. Also the 8-bit quantize step used plain GPU division (255/255 → 0.99999994 on NVIDIA)
   where the CPU's is correctly rounded. Fix: the Normal fast path term for term, and one fma
   residual step in the quantize. CPU unchanged, tolerance unchanged.
2. **`lightcraft-gpu` size guard could overflow.** `render` checked `gpu.fits(w * h * 3)` with
   unchecked arithmetic; a request whose size overflows (e.g. `RenderRequest::fit(usize::MAX, …)`,
   since `fit` scales up) wrapped to a small number, passed, and the per-pixel stage then
   dispatched bands of rows indefinitely (the GPU busy, the render never returning). The app never
   asks for such sizes, but the guard is now checked and the request is refused as over the limit
   (`fallback::absurd_render_sizes_are_refused`, which hangs without the fix).
3. **Engine test isolation.** `tests_export::export_falls_back_to_the_cpu_when_gpu_work_is_lost`
   injects a fault that stops the GPU for the whole process; tests that need the GPU path now share
   a lock with it (`tests_gpu::gpu_state`).

## CPU fallbacks and timings at 24 MP

`cargo test --release -p lightcraft-gpu --test toolset -- --ignored --nocapture`, 6000 × 4000
full-size render of a synthetic scene with a typical edit on top, warm device:

| Tool | GPU path | CPU only | Renderer |
| --- | ---: | ---: | --- |
| typical edit | 109 ms | 673 ms | GPU |
| lens profile (database, historical CPU resample) | 315 ms | 796 ms | previous hybrid implementation |
| tone equalizer | — | 772 ms | CPU fallback |
| colour calibration, linear | 111 ms | 644 ms | GPU |
| colour calibration, gamut + clip | 320 ms | 703 ms | GPU + CPU linear stage |
| colour calibration, non-linear Bradford | 285 ms | 661 ms | GPU + CPU linear stage |
| capture sharpening (historical CPU presource) | 1155 ms | 1657 ms | previous hybrid implementation |
| depth mask | 302 ms | 699 ms | GPU + CPU mask shape |
| film look | 109 ms | 723 ms | GPU |
| AI Remove patch | 282 ms | 653 ms | GPU + CPU linear stage |
| develop layer tools (curve on a mask) | — | 695 ms | CPU fallback |

Lens profiles now run in the GPU geometry kernel; the historical 315 ms row does
not measure the native implementation. The before/after benchmark below and the
current full-render lens rows await RTX measurements.

On the real ARWs (18.7 MP output) the previous CPU capture stage took 1.4–3.5 s
(radius measured from the raw). Capture now runs on the device; these historical timings do
not measure the native GPU implementation.

### Candidates for native WGSL kernels (not built)

- **Tone equalizer** — a whole-render CPU fallback (~0.8 s at 24 MP) for a guided filter on log
  luminance and a per-pixel gain: the guided filter kernels exist (`guided_fast`), only the zone
  curve is new.
- **Develop layer tools** — a whole-render CPU fallback whenever a mask carries tools; the per-stage
  blends mirror kernels that exist for the global tools, but it is the largest port.
- Non-linear colour calibration and depth-mask evaluation cost ~0.2 s each; low priority.

## Needs a human eye

Not automatable here (no display automation was used on the photos):

- In Develop on a real raw, toggle each tool above on and off and drag its sliders: watch for
  flicker, a frame of the previous state after toggling (stale view), or a brief colour jump when a
  tool switches the render between GPU and CPU (tone equalizer, layer tools).
- Compare the Develop view with **File › Export** for film looks, colour calibration and a lens
  profile (the tests show ≤ 1 LSB, so any visible difference is a display/colour-management issue).
- A develop layer whose mask uses several tools (curve + colour + vignette), with opacity changes:
  smoothness of the CPU fallback while dragging.
- Raw options: switching Demosaic / Highlights shows a re-decode; check the progress feedback and
  that the view never shows the old decode after the switch.
- Tone equalizer *Show Mask* overlay in the UI.

## Detail tools (2026-10-09; RTX validation pending)

`lc-gpu/tests/toolset.rs` adds four tests for pixel-scale log USM/halo clamping,
Alt Masking preview, Y0U0V0 wavelet NR's six sliders, RGB-guided dark-channel dehaze
with both signs, and cached slider/sensor-scale changes. Odd dimensions, HDR values,
strong colour edges, full size and two preview sizes are covered. Each comparison
requires mean absolute error <0.5 LSB and maximum <=3 LSB. The no-adapter sandbox
skips device execution; all WGSL modules are parsed and validated by Naga regardless.

The ignored `bench_toolset_24mp` includes independent sharpening, wavelet NR and
Dehaze rows. `LC_DETAIL_BENCH_ONLY=1` selects them; CPU rows run without an adapter.
GPU NR executes VST, EAW, reduction, soft-threshold synthesis and recombination.
Only image noise estimation and reduced band statistics run on the host. GPU haze
uses host ambient-light selection, then native morphology, cropped Kahan boxes,
covariance solves and reconstruction. If its 9-channel covariance buffer exceeds
the storage-buffer limit, only haze preparation runs on the CPU inside the GPU render.

## Native capture sharpening (2026-10-09; RTX validation pending)

Capture is no longer a CPU stage inside GPU renders. `lc-gpu/src/capture.rs` and
`wgsl/capture.wgsl` run luminance, the black/clipped exclusion and 21-pixel variance
mask, corner-boost kernel indices, Richardson–Lucy and the final RGB gain on the
uploaded source before geometry. The 256 quarter Gaussian kernels (25 KiB) are
generated once on the host in exactly the reference's f32 order. Their disc
truncation makes them non-separable; both 5x5 and 9x9 convolution use the actual
Gaussian taps, including at small sigma. A shared-memory tile includes the 4px halo.

The mask's sigma-2 blur reuses `blur.wgsl` box kernels with whole-row horizontal
sums and 32-row vertical bands to match the CPU's addition order. RL accumulates
taps in the CPU's row-major order. Radial distance uses WGSL sqrt in place of Rust
hypot; device exp/log/division and multiply-add contraction can differ by ulps.
A residual correction on division protects the sigma-index truncation boundary.
The CPU reference and its output goldens are unchanged.

The per-view device cache keeps the original upload and the last sharpened source,
keyed by source identity and all resolved capture parameters. Sampled, linear and
spatial stages also include that capture key. Other tools and output-size changes
reuse capture; radius/threshold/iterations/corner boost and sensor metadata changes
rebuild it. Device memory accounting and cache clearing include both source buffers.
Source-dependent pupil/automatic Upright analyses read back the GPU-sharpened
source. Lens-database geometry consumes it directly on the device. If capture's full
source exceeds the device buffer limit, the renderer returns `None` with a limit
reason for the ordinary CPU fallback; it does not run hidden CPU capture inside a
successful GPU render.

Adapter-independent tests compare the f32 WGSL transcription's mask and output
with the CPU at 1/2/8/50 iterations and its estimate with the independent upstream
RL fixture (4096 samples). Device tests inspect luminance, mask, kernel indices,
ratio and estimate after each iteration, exercise cache reuse/invalidation and
source-limit refusal. `toolset.rs` requires mean |delta| <0.5 LSB and max <=3 LSB
for parameter sweeps, binned previews, odd/tiny dimensions, HDR/black/negative
border pixels, cached edits and lens geometry. No adapter is available in the
sandbox, so device numerical comparisons and timings still need the RTX 5090.

`bench_capture_sharpening_24mp` measures a fresh capture render, a cached exposure
edit and a radius change that preserves the upload, plus CPU ms at 6000x4000.
`bench_toolset_24mp` retains its capture row and now reports it as GPU. Exact RTX
commands and sandbox test counts are in
[`CODEX-REPORT-gpu-capture.md`](wip/CODEX-REPORT-gpu-capture.md).

## Native lens-database geometry (2026-10-09; RTX validation pending)

Lens profiles are no longer a CPU pixel stage inside GPU renders. The existing
`geom.wgsl::sample_warp` evaluates lensfun poly3/poly5/ptlens distortion, linear
and poly3 TCA, and PA vignetting. Parameters come from the same `Warp::lensdb`
map as the CPU, including its already reoriented optical centre and pixel-centre
normalisation. The order is inverse perspective, manual distortion, embedded
warp if present, database distortion and TCA, then manual CA. The database
replaces the embedded profile under the existing `set_lensdb` rules. Gain is
evaluated at the final green source position and multiplies manual/embedded
vignetting; PA retains the reference's nonpositive/near-zero denominator guard
and strength exponent.

Positions and gains use f32 on the GPU. Distortion coefficients and linear TCA
scales incorporate their strengths in f64 on the host before packing. WGSL
sqrt/pow/division and multiply-add contraction can differ from CPU f64 math.
The host still computes the exact reference f64 coverage bit for each output
pixel, using outward-rounded block intervals and per-pixel decisions near edges;
the shader never uses rounded f32 positions to classify blank canvas. Red and
blue sample independently, and gain uses green, like `Frame::sample`.

Zero strengths and numerically identity models use the ordinary copy/affine
sample plan with no coverage mask or lens dispatch. If unrelated tools require
a warp, inactive database models bypass their coordinate/gain calculations.
Profile and strength edits invalidate sampled stages through the existing
geometry key while retaining the uploaded/capture-sharpened source. The source
buffer size limit can still require host prefiltering/resampling, as for other
geometry; this is independent of whether a lens profile is present.

Adapter-independent tests check packed f32 models against the f64 reference at
24 MP, all eight optical-centre orientations, identity/zero-strength branches,
the PA denominator guard, and every coverage bit in edge-heavy frames. Device
tests verify the rounding boundary and source upload reuse. `toolset.rs` adds 96
border comparisons with strong barrel/pincushion poly3/poly5/ptlens, an
off-centre axis, TCA at 200%, all eight orientations, native size and Mitchell
prefiltered previews with crop/rotation/flips/perspective/manual corrections;
it also checks embedded-profile replacement and PA's guard. Every rendered
comparison requires mean absolute error <0.5 LSB and max <=3 LSB.

`render::lensdb_tests::bench_lensdb_geometry_24mp` directly measures the old
CPU resample + upload against the new source upload + GPU warp, plus the GPU
warp with an existing source upload, at 0/100/200%. It synchronizes device work
and reports the minimum of three timed repeats after warm-up. The ignored
`toolset::bench_toolset_24mp` retains the full lens render row (now GPU) and adds
a zero-strength row. No GPU adapter is available in this sandbox; device parity
and before/after timings remain for the RTX 5090. Exact commands and sandbox
test counts are in [`CODEX-REPORT-gpu-lensdb.md`](wip/CODEX-REPORT-gpu-lensdb.md).

### Lens database on the GPU — RTX 5090 validation (2026-10-09)

NVIDIA GeForce RTX 5090, Vulkan, driver 615.71.09. `lightcraft-gpu` + `lightcraft-pipeline`: 242 passed, 0 failed,
no device test skipped; every lens comparison within mean < 0.5 LSB, max ≤ 1 LSB.

| 24 MP geometry | Before: CPU resample + upload | After: upload + GPU warp | After: reused upload + warp |
| --- | ---: | ---: | ---: |
| lens 0% | 96.3 ms | 36.7 ms | 0.0 ms |
| lens 100% | 274.2 ms | 48.2 ms | 5.2 ms |
| lens 200% | 260.7 ms | 51.0 ms | 5.6 ms |

Full-render toolset bench (24 MP): lens profile (db) 276 ms on the GPU path vs 2894 ms CPU (was 315 ms hybrid).
