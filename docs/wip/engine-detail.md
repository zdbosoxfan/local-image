# WIP: process 2026 detail tools (sharpening, noise reduction, dehaze)

Status when paused (2026-10-09). Branch `worktree-agent-afd6402aed9ef067b`, based on
`claude/sleepy-franklin-egimjb` @ `fbc02a9`. One WIP commit; **lc-pipeline builds**, lc-gpu does
**not** yet (callers not updated, see below), new tests not yet registered or run.

## Done

- `lc-develop/src/settings.rs`: `ProcessVersion { Legacy, V2026 }`, `DevelopSettings::process`
  (after `profile`), `is_legacy_process`, `DevelopSettings::v2026()` — the exact shared
  definition agreed with the colour/tone agent.
- `lc-raster/src/blur.rs`: `gauss_taps`, `gaussian_fine` (true Gaussian kernel for σ < 2, three-tap
  variance-exact kernel under 0.7 px; box approximation above), `min_filter` (clamped window); unit
  tests for both.
- `lc-pipeline/src/detail.rs` (new): V2026 tools.
  - Sharpening (own design, as the coordinator's research asked): USM on log luminance at the
    photo's pixel scale (`out_per_orig` = output px per original px incl. `sensor_scale`), Detail
    mixes in a deconvolution-like term (2·h − blur(h), two Van Cittert steps) and relaxes halo
    control (overshoot past the 3×3 min/max damped to 15 % at Detail 0), Masking = smoothstep of
    the smoothed gradient magnitude (edge contrast in EV). `sharp_planes`, `SharpK`,
    `sharpen_at`, `sharp_mask_plane`.
  - Noise reduction: √Y + opponent √r−√g / √b−√g (variance-stabilising), à-trous B3 wavelet
    shrinkage (non-negative garrote) with per-level/channel noise measured by MAD on 8 tiles,
    Detail / Contrast / Smoothness level weighting, luminance-edge guard for chroma.
    **This is an interim own design; superseded by the decision below (faithful darktable port).**
  - Dehaze: own DCP design (1024-px grid, min-filter patch, coloured airlight from the top 0.1 %,
    cross guided-filter refinement, floor 0.1, highlight protection, negative = add haze).
    **To be replaced by a faithful port of darktable `hazeremoval.c` (see below).**
- Wiring (CPU): `local.rs` (`plane_sigmas(s, ppl, out_per_orig, q)` with `sharp`/`haze`,
  `Planes.sharp/haze`, `prepare` builds them, `denoise(.., sensor_scale)` dispatches V2026),
  `layers.rs` (5-tuple `planes_needed`, `LocalTone.sharpen_2026/sharpen_detail`, layer NR in the
  photo's process), `lib.rs` (`pub mod detail`, `Prepared.sharp/haze`, lin_key + `nr_contrast` +
  process, `StageCache::bytes`, `Overlay::SharpenMask` alpha), `finish.rs` (`FinishParams`
  V2026 fields + `with_planes`, V2026 dehaze/sharpening incl. local Sharpness and layers),
  `visualize.rs` (`Overlay::SharpenMask`, kind 5). Legacy paths untouched (planes and formulas
  are only switched on `s.v2026()`).
- `lc-pipeline/src/tests_detail.rs` written (slanted-edge MTF50/overshoot, radius frequency
  response, Detail, Masking + Alt overlay, preview vs full size, NR Contrast texture retention +
  shadow noise, colour-edge chroma leak, dehaze halo ring, cache equality, legacy ignores new
  sliders) — **not yet added to `lib.rs` (`#[cfg(test)] mod tests_detail;`) nor run**; thresholds
  will need tuning, and the NR tests assumed the earlier guided design.

## In progress / next steps

1. **Decision (coordinator, owner directive "prioritize quality")**: NR and dehaze become
   *faithful ports* of darktable at `733bd69f32cac7ff5e41025115942772add1f088`:
   - `src/iop/denoiseprofile.c` wavelet mode, Y0U0V0, new VST: `precondition_Y0U0V0`,
     `backtransform_Y0U0V0`, `set_up_conversion_matrices`, `variance_stabilizing_xform`
     (BayesShrink thresholds from `sum_y2`), `process_wavelets` (max_scale from `in_scale`),
     `src/common/eaw.c` `eaw_dn_decompose` (edge-avoiding 5×5 à-trous, `dn_weight` with
     `fast_mexp2f` from `src/common/math.h`) + `eaw_synthesize`/`accumulate` (soft threshold).
     Noise profile: no camera DB → estimate a (and b) from the image (or generic poissonian
     a=1e-4 × ISO scaling); `shadows`/`bias` via `infer_*_from_profile`; wb = (1,1,1).
     Slider mapping to design: Amount → `strength`, Detail → fine-band `force`, Contrast →
     mid/coarse Y0 band force, Colour Amount/Detail/Smoothness → U0V0 band forces.
   - `src/iop/hazeremoval.c` (`_dark_channel`, `_transition_map`, box max/min closing,
     `_ambient_light` with the quick-select quantiles, `t_min = exp(−distance·distance_max)`,
     adaptive windows `w1 = 2+ceil(4·wscale)`, `w2 = 3+ceil(6·wscale)`), with
     `src/common/guided_filter.c` (colour guided filter, Cramer 3×3, eps = 0.025) and
     `src/common/box_filters.cc` (box mean normalised by in-image count; box min/max).
     Mapping: strength = Dehaze/100, distance = |Dehaze|/100 (document).
   - Sharpening stays the own design above (coordinator's spec), optionally checked against
     darktable `src/iop/sharpen.c`'s USM core.
   Upstream sources were fetched (reference only) to
   `/tmp/claude-1000/.../scratchpad/dt-src/` (session scratchpad; re-fetch with
   `scratchpad/fetch_dt.sh <dir> <paths>` from raw.githubusercontent at the pinned commit).
2. Reference vectors: standalone C harnesses (gcc) in
   `scratchpad/refvec/<name>` → fixtures under `crates/lc-pipeline/tests/fixtures/` + README on
   regeneration; Rust ports assert within a stated tolerance.
3. Fix `lc-gpu` callers: `local::plane_sigmas` (new `out_per_orig` arg), `PlaneSigmas`
   (`sharp`, `haze`), `local::nr_params` (legacy-only again), `FinishParams::with_planes`.
4. GPU twins: `blur.wgsl` (`conv_h/v` true Gaussian, `min_h/v`, box mean/max, à-trous EAW),
   `map.wgsl` (VST/back-transform, synthesize, haze kernels, sharpen mask), `finish.wgsl`
   (V2026 sharpening reading `log_l` neighbours and `[texture | B1 | B2]` concatenated in the
   `tex` binding at `F_SHARP_OFF`; V2026 dehaze from the `dark` binding + `F_AIR_RGB`),
   `params.rs` fields (`V2026`, `SHARP_A/D/HC/T/EK/OFF`, `HAS_SHARP`, `AIR_RGB`), render.rs
   planes/denoise/overlay. Then equivalence cases in `tests/toolset.rs` (mean < 0.5, max ≤ 3
   LSB) and 24 MP bench rows.
5. UI: sliders already shown (`panels/edit.rs`); Alt-drag Masking preview needs a small hook in
   `panels/edit.rs` + `panels/detail.rs::view_overlay` → `Overlay::SharpenMask` (V2026 only).
6. `crs.rs`: mapping is already 1:1 (LR units = ours); add range clamps + a V2026 test.
7. docs/PORTS.md rows + `licenses/darktable-NOTICE.md` for the ports; GPU-VALIDATION timings.
8. fmt/clippy (rustup 1.98.1) on changed crates; full test runs.

## Decisions

- Legacy renders bit-identical: every new path is gated on `s.v2026()`; legacy planes/keys
  unchanged except the lin_key hash input (cache key only, not pixels).
- No UI/commands for switching process versions (colour/tone agent owns that).
- No dependency on AI Denoise / `enhance.denoise` (being removed by the coordinator).
- Texture/Clarity untouched (another agent).
