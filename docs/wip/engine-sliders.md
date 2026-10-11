# WIP: V2026 engine for the primary Develop sliders (paused 2026-10-09)

The work is on branch `worktree-agent-a5fc7556e46ab4857`. Before starting, I reset that branch onto
`claude/sleepy-franklin-egimjb` @ fbc02a9, because the worktree had been created from `main`.

The owner paused the work partway through. Nothing is wired into the render yet, so Legacy output is
unchanged.

In the paths below, `<scratch>` means
`/tmp/claude-1000/-home-zdavidson-Documents-Local-Image---Testing/d1bb3608-dca2-4e30-892b-54147f9b368f/scratchpad`.

## Done

- **`lc-develop/src/settings.rs`:** adds `ProcessVersion { Legacy, V2026 }`.
  - The new field `DevelopSettings.process` sits right after `profile` and is left out of the JSON
    while it is Legacy.
  - Also adds `is_legacy_process` and `DevelopSettings::v2026()`.
  - This is exactly the definition agreed with the ProcessVersion agent. It builds.
- **`lc-pipeline/src/ucs.rs`** (new, not yet in `lib.rs`):
  - darktable UCS 22, Filmlight Yrg and CIE 2006 LMS conversions.
  - The Rec.2020 gamut LUT (`dt_UCS_22_build_gamut_LUT`), `lookup_gamut`, `soft_clip` and
    `gamut_map_HSB`.
  - An inverse UV → xy conversion, for Skin Tone.
  - It has unit tests but has **not been compiled yet**.
- **`lc-pipeline/src/llf.rs`** (new, not yet in `lib.rs`): a port of darktable
  `src/common/locallaplacian.c`, **not compiled yet**.
  - Ported faithfully: `local_laplacian_internal` in its regular mode — replication padding,
    `gauss_reduce` / `gauss_expand` / `ll_expand_gaussian`, the boundary fills, `curve_scalar` with
    `dt_fast_expf`, and the double-precision stencil terms.
  - Documented extensions:
    - the number of γ samples is a parameter;
    - each γ's Laplacians are accumulated as it goes (same sums, less memory);
    - per-level weights;
    - our `Remap::Tone`, a tone curve applied edge-aware, with the residual remapped too.
- **darktable checkout** at the pinned commit 733bd69f: `<scratch>/dt`.

## Not done: reference vectors

No fixtures exist yet. The helper was stopped before it wrote any files.

Planned layout:
- C harnesses (built with `gcc -O0 -ffp-contract=off`) in `<scratch>/refvec/`.
- Fixtures, `manifest.json` and a README in `crates/lc-pipeline/tests/fixtures/darktable/`.
- Kinds: gaussian IIR, interpolate_bilinear, luminance_mask, EIGF, fast_surface_blur,
  local_laplacian, colorbalancergb (DTUCS and JzAzBz), colorequal, UCS.

What the helper found while reading upstream; apply these when the work resumes:

**Build and math details the port must match**
- `dt_box_mean` is C++ (`src/common/box_filters.cc`).
  - It starts with `#pragma GCC optimize("fast-math")`. Strip the pragma for IEEE reference
    vectors, and document that.
- Release darktable builds with `-ffast-math`. That makes `dt_fast_hypotf` = `sqrtf(x*x+y*y)`, so
  the Rust port should use that form, not `hypot`.
- colorbalancergb's midtones power uses `dt_vector_powf`. That is a polynomial log2/exp2
  approximation, not `powf`, and has to be ported as-is.

**How toneequal calls the mask functions**
- `exposure_boost = exp2f(p)` and `contrast_boost = exp2f(p)`.
- `feathering = 1 / p->feathering`.
- `CONTRAST_FULCRUM = exp2f(-4)`, passed only in the GUIDED and EIGF modes.
- Quantize clamps are `exp2f(-14)` and `4`.
- EIGF downscales by `clamp(sigma, 1, 4)`; the guided filter downscales by a fixed 4.

**Harness setup**
- Rec.2020 work profile: in the harness, `#define` `XYZ_D50_to_D65_CAT16` and
  `XYZ_D65_to_D50_CAT16` to identity after including `chromatic_adaptation.h`. Then set
  `matrix_in = M_rec2020_D65` and `matrix_out` to its inverse. The upstream function text stays
  unchanged.
  - colorequal has not been checked for other uses of these two matrices.

**Next steps**
1. Add `ucs` and `llf` to `lib.rs` and build.
2. Generate the fixtures.
3. Write the fixture-driven tests, at about 1e-5 relative tolerance.
4. Port faithfully:
   - `dt_gaussian_blur` (Deriche IIR, `src/common/gaussian.c`);
   - EIGF (`src/common/eigf.h`) and `luminance_mask.h`;
   - colorbalancergb, both formulas;
   - colorequal (including `_periodic_RBF_interpolate` and `pseudo_solve`).

## Left: the agreed design (see the coordinator's messages)

1. **Highlights / Shadows: a candidate framework behind an internal switch** (`HsMethod`; changing
   the default is a one-line edit).
   - **A, "LI Tone":** LLF tone fields on a proxy of about 1280–1600 px.
     - The luminance estimator is blended towards max RGB in the highlights.
     - γ spacing is σ/1.5.
     - Each slider gets one gain field G. G is linear in the slider, so slider drags are per-pixel
       only. The fields are keyed on exposure.
     - The fields are guided-upsampled to the output size and applied as RGB·2^G.
     - Negative highlight gain is tapered where pixels look clipped. The current plan is a
       heuristic (near-neutral pixels at the image's peak). Real raw clip information would need a
       `SourceInfo` field from the raw-colour owner.
   - **B:** the faithful toneequal mask (EIGF or guided filter of *linear* luminance) driving zone
     curves. The unit profiles are fitted with `toneeq::Curve`.
   - **C:** a multi-scale variant.
   - Local Whites / Blacks become extra fields or curves.
2. **Clarity, Texture, Structure.**
   - Clarity uses the LLF detail term (darktable `curve_scalar` with shadows = highlights = 1, which
     makes it linear in Clarity).
     - Level weights are centred around ~1.5 % of the long edge, and the effect is
       midtone-weighted.
     - Modes: Natural / Punch / Neutral, stored in `effects.clarity_mode`, with a segmented control
       in the UI.
   - Texture and a new Structure slider: guided band-pass on noise-biased log luminance.
     - The bands are about 4–16 px (Texture) and 2–4 px (Structure) at 6000 px, scaled by
       px_per_long.
3. **Colour.**
   - **Faithful colorbalancergb**, applied before the tone map: vibrance, chroma, perceptual
     saturation, the 4-way wheels, and the white and grey fulcrums. Mapping from the LR sliders:
     - Saturation < 0 → chroma; Saturation > 0 → chroma + saturation;
     - global wheel → an unmasked gain;
     - Blending → the mask weights;
     - Balance → the mask grey fulcrum.
   - **Faithful colorequal** for the 8 HSL bands.
     - Its nodes sit at the LR band hues.
     - The multiplication by saturation (S) is moved out of the smoothing step to the per-pixel
       stage, so the colour planes don't depend on exposure.
   - **B&W:** grey from UCS brightness (which accounts for the Helmholtz–Kohlrausch effect), with
     the hue-band mix applied on top.
   - **Point Color** stays in OkLCh, after the tone map.
   - **New Skin Tone tool** (our own design): a UCS window around the skin colour, uniformity
     applied to low-pass chroma, high-pass detail kept.
     - Its colour-pick command needs the owner of the lc-engine `cmd` module.
4. **Masks.**
   - Local Temp / Tint become a CAT16 von Kries shift (a log-LMS basis), keeping neutral luminance.
   - Colour-range selection happens in OkLab, on the exposed value after the photo's tone map.
5. **Layers:** each V2026 tool is blended at its own stage, on the CPU, as layers are rendered
   today.
6. **GPU:** WGSL for everything above.
   - Kernels: pyramid kernels, the IIR gaussian, and the per-pixel colour in `finish.wgsl`.
   - Buffer layout: the fields are packed planar into the `clar` binding; the colour and skin
     planes are appended to `masks`.
   - Tests: equivalence and toolset tests, plus 24 MP rows in the bench.
7. **Evaluation harness** (not started): an ignored test or an example binary.
   - Renders each candidate at Highlights / Shadows −100 … +100.
   - Inputs: synthetic HDR scenes, plus the owner's ARWs in `<scratch>/raw/*.ARW` (use locally
     only, never commit).
   - Output: JPEG crops and one HTML page in `<scratch>/compare-highlights/`, not in the repo.
   - Metrics on the page:
     - halo overshoot, width and energy;
     - Lightness Order Error (LOE);
     - hue shift Δh and ΔE2000;
     - preview-vs-export difference.
8. **Docs and goldens:** `DEVELOP-DESIGN.md` §4.2, `PORTS.md`, `licenses/darktable-NOTICE.md`, and
   V2026 golden hashes.
