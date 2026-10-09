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
  strength changes on a cached view. Geometry is resampled on the CPU (`Frame::gpu_samplable`).
- **Tone equalizer**: `render` returns `None` with the reason; the Show Mask overlay
  (`Overlay::ToneEqMask`) falls back too, even with every zone at 0, and draws a grey mask that
  follows the image; with the section off the GPU renders again.
- **Colour calibration**: CAT16 / linear Bradford / XYZ × six illuminants with gamut 0 and clip off
  (GPU, folded into the WB matrix) on raw and rendered sources; non-linear Bradford, gamut > 0,
  clip and the defaults (CPU linear stage, then GPU); with WB, edits and a saturated scene;
  switching between the two paths on a cached view.
- **Capture sharpening**: several radius / threshold / iterations / corner-boost values, a binned
  preview, a rendered source; on one cached view each change (radius, threshold, iterations, corner
  boost, off, on) re-makes and re-uploads the sharpened source (cached == fresh == CPU), and going
  back to the first values gives the first image.
- **Depth masks**: three bands, combined with an inverted linear, opacity, inverted mask, rotation
  + crop, and the mask overlay.
- **Film looks**: all eight `lc.filmsim.*` profiles at 50 / 100 / 200 % on a raw and a rendered
  scene, with edits, and in Display P3 — GPU render vs CPU export.
- **AI Remove / AI Denoise**: a stub patch (gradient, soft alpha) at 100 % / 40 % opacity, with a
  heal spot, edits and geometry, added / removed on a cached view, a missing patch; a stub denoised
  source at 25 % / 100 % on a cached view.

Through the engine (`lightcraft-engine/src/tests_gpu.rs`, a view `RenderJob` with its stage cache):
raw options RCD / Dual / Opposed / Clip each decode again and render on the GPU, and switching back
to Default gives the original image bit for bit; a real lensfun-database lens at 0 / 100 / 200 %
with crop + rotation; AI Remove (mock engine) and AI Denoise (stand-in model) through the commands.

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
| lens profile (database) | 315 ms | 796 ms | GPU + CPU geometry |
| tone equalizer | — | 772 ms | CPU fallback |
| colour calibration, linear | 111 ms | 644 ms | GPU |
| colour calibration, gamut + clip | 320 ms | 703 ms | GPU + CPU linear stage |
| colour calibration, non-linear Bradford | 285 ms | 661 ms | GPU + CPU linear stage |
| capture sharpening | 1155 ms | 1657 ms | GPU + CPU capture presource |
| depth mask | 302 ms | 699 ms | GPU + CPU mask shape |
| film look | 109 ms | 723 ms | GPU |
| AI Remove patch | 282 ms | 653 ms | GPU + CPU linear stage |
| develop layer tools (curve on a mask) | — | 695 ms | CPU fallback |

On the real ARWs (18.7 MP output) capture sharpening took 1.4–3.5 s (radius measured from the raw).

### Candidates for native WGSL kernels (not built)

- **Capture sharpening** — the slowest stage by far (1–3.5 s per full render, every time a capture
  value changes); Richardson–Lucy is a few separable blurs and multiplies per iteration, which the
  existing blur kernels already cover. Highest value.
- **Tone equalizer** — a whole-render CPU fallback (~0.8 s at 24 MP) for a guided filter on log
  luminance and a per-pixel gain: the guided filter kernels exist (`guided_fast`), only the zone
  curve is new.
- **Develop layer tools** — a whole-render CPU fallback whenever a mask carries tools; the per-stage
  blends mirror kernels that exist for the global tools, but it is the largest port.
- **Lens-database sampling** — +200 ms at 24 MP for the CPU resample; the warp kernel would need
  the lensfun models (closed-form polynomials). Moderate value.
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
