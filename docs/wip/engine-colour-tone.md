# WIP: Process 2026 colour and tone engine (paused 2026-10-09)

Branch `worktree-agent-a8d90e3dc5155d0a9`, reset onto `claude/sleepy-franklin-egimjb` (`fbc02a9`).
**The WIP commit is not yet verified:** `lightcraft-develop` and the `tone2` unit tests build. The
engine, GPU and UI crates have not been compiled since their edits. No fmt or clippy has been run.

## Done (uncommitted work up to the WIP commit)

- **lc-develop**
  - `ProcessVersion { Legacy, V2026 }` uses the coordinator's exact definition.
  - The `process` field (after `profile`), `is_legacy_process` and `DevelopSettings::v2026()` are
    in `settings.rs`.
  - New fields: `Look { Adobe, Sigmoid, Camera }`, `LookOptions { base: ToneBase, hue_preservation: 75 }`
    with `ToneBase { Standard, ExtraShadow, HighContrast, Linear }` (Capture One-style variants),
    and `ToneCurve.mode: CurveMode { Luminance, Rgb }`. Each is left out of the JSON at its default.
  - `to_json_full` includes them. `is_unedited` ignores `process`.
  - Copy groups: `process` is in Calibration; `look` and `look_options` are in Profile.
  - Test: `process_version_and_look_default_to_legacy_and_stay_out_of_old_json` passes.
- **lc-catalog `Photo`**
  - New fields `process` and `look`, left out of the JSON when Legacy or default.
  - `camera_defaults()` uses them, so old photos keep Legacy defaults and "edited" state.
- **lc-engine**
  - `apply_import_defaults` and `candidate_thumb_job` set `ProcessVersion::CURRENT` and the
    preferred look.
  - `ImportDefaults.look` is the "Default look for new photos" preference (`library.preferences`
    `import.look`).
  - New commands:
    - `develop.setProcess {process, ids?}`: undoable Batch, returns `{changed}`.
    - `develop.look {look, ids?}`.
  - `develop.reset` keeps the photo's process version and uses the photo's look.
- **lc-ui-egui**
  - New `panels/process.rs`:
    - Look combo under Profile (raw photos on V2026).
    - Process combo plus "Update to Process 2026" at the top of Calibration.
    - Settings › Import "Default look for new photos".
  - These are wired into `panels/edit.rs`, `panels/settings.rs` and `panels/mod.rs`.
- **lc-pipeline**
  - New `tone2.rs`:
    - darktable sigmoid port (`commit_params` and `_generalized_loglogistic_sigmoid`), at
      upstream defaults with +0.7 EV.
    - Our own Adobe-like curve: grey 0.18 → 0.29, sensor white 1.0 → ~0.90, smooth toe and
      shoulder.
    - `BaseCurve::Camera(CameraTone)`.
    - `scene_ev` maps Contrast to log slope, Whites to the range above grey and Blacks to the
      range below, before the curve, for raw sources.
    - `display_tone` handles rendered sources (identity at neutral).
    - Per-pixel `tone_px` is the darktable per-channel method plus `_preserve_hue_and_energy`
      (hue preservation from settings), then the camera chroma scale.
    - darktable's hyperbolic `compress` is kept (ratio mode, currently unused).
  - `ToneMap` gained `v2: Option<V2Tone>` (`with_v2`, `v2()`).
  - `finish::tone_map` and `finish::tone_px` dispatch to tone2 when `s.v2026()`. The Legacy path
    is untouched.
  - New `SourceInfo` fields: `look_curve` (camera JPEG or maker curve) and `profile_curve` (DNG
    `ProfileToneCurve`). Both are `None` until Process 2026 decodes fill them.
  - `before_settings` keeps `process` and `look`.
  - tone2 tests: 7 of 8 pass. `adobe_like_has_five_stops_of_headroom_and_no_knee` fails at line
    ~516. That is the slope-smoothness tolerance or the +5 EV step after the recalibration to
    `grey_out 0.29 / slope 1.3 / shoulder 1.6`. Next step: check the numbers and loosen the
    tolerance or retune.
- **lc-gpu**
  - `params.rs`: new `TONE_V2` and `TONE_HUE` fields.
  - `finish.wgsl`: `tone_v2()` mirrors `tone2::tone_px`, with a branch in `main`.
  - Not compiled or tested yet.

## Not started / left

1. **Look data in decode (lc-engine `files.rs`)**
   - Add `process` to `RawOptions` (`RawOptions::of` must set it even when the Raw section is
     off), so `key()` separates V2026 decodes and the Legacy key stays 0.
   - Fill `look_curve` and `profile_curve` in V2026 decodes:
     - Extend `camera_preview` fitting to every format with a preview (tone and chroma only when
       the format has a matrix).
     - Use RawTherapee `histmatching.cc` as a robustness reference.
     - Fall back to maker base curves: port darktable `basecurve.c` presets and the monotone
       Hermite curve, as a table in a new `lc-engine/src/base_curves.rs`.
   - Smart previews have no V2026 curves. They fall back to profile or Adobe.
2. **Camera matrices** (coordinator decision)
   - Generate a Rust table from rawler `dnglab v0.8.0/rawler/data/cameras/**/*.toml`
     (LGPL-2.1 → GPL-3).
   - Fill gaps from RawTherapee `camconst.json` (GPL-3). No RawSpeed `cameras.xml`.
   - In V2026 decodes, fill `raw.color.color_matrix` and `illuminant` (A + D65) when the file has
     none, then the existing DNG interpolation applies.
   - Also: `Photo::relative_wb` should become false for matched cameras; honour
     `BaselineExposureOffset` / `DefaultBlackRender`.
   - Add a test that spot-checks the table against the TOMLs.
3. **DCP loader**
   - Bundle only RawTherapee DCPs that are public domain or CC0. Skip "Maciej Dworak" and
     "Dr Slony".
   - Apply the base HueSatMap. Add a "Camera profiles folder" setting for user DCPs. Never bundle
     Adobe profiles.
4. **WB per white (V2026)**
   - `SourceInfo.camera_color`: a Copy DNG colour model plus the as-shot undo matrix.
   - `local::wb_matrix_for` V2026 path: `M_t·W_t·W_s⁻¹·M_s⁻¹`, normalised to neutral luminance as
     Legacy is. Use a copy of the DNG math in `lc-color` so `lc-raw` stays bit-identical.
   - Add the process to `lin_key`.
5. **Curves V2026**
   - Before the gamut stage, in display-linear Rec.2020.
   - Luminance mode: base LUT on the sRGB-encoded luminance with a ratio, then desaturate past
     white.
   - RGB mode: per channel, with refine saturation.
   - Then the R/G/B curves.
   - GPU: 4 tables `[base, r, g, b]`, plus `F_CURVES_V2` and `F_CURVE_LUM`. UI: a Luminance/RGB
     toggle.
6. **Output V2026**
   - Soft gamut compression for raw sources: ACES-like per-channel distance (thr 0.8, limit 1.3,
     p 1.2), then the existing hard map. Rendered sources keep the hard map.
   - TPDF dither for 8-bit, position-hashed:
     - Integer hash with exact f32 noise.
     - Amplitude faded to 0 at 0 and 255.
     - `finish_with`'s `store` gets (x, y).
     - New `DITHER_HASH` constants exported to WGSL like `GRAIN_HASH`.
7. **GPU tests and goldens**
   - GPU vs CPU cases in `lc-gpu/tests/equivalence.rs` and `toolset.rs`, for every look, base,
     hue, curve mode, gamut and dither.
   - V2026 golden hashes once behaviour is final.
8. **Owner rule: reference vectors**
   - Every port (`sigmoid.c`, basecurve with Hermite and norms, `histmatching`, DCP application,
     dual-illuminant interpolation) needs C harness reference vectors.
   - Build them in `scratchpad/refvec/<name>`, never in the build. Fixtures go in the crate's
     `tests/fixtures/` with a README.
9. **Docs and notices**
   - `docs/PORTS.md` rows: sigmoid.c (`733bd69f32cac7ff5e41025115942772add1f088`), basecurve.c,
     rawler, camconst, RawTherapee DCPs, histmatching.
   - New `licenses/rawler-NOTICE.md`, and updates to `licenses/darktable-NOTICE.md`.

## Sources (read-only scratch)

All under `/tmp/claude-1000/-home-zdavidson-Documents-Local-Image---Testing/d1bb3608-dca2-4e30-892b-54147f9b368f/scratchpad/`:

- `dt/`: darktable at `733bd69f…` (`src/iop/sigmoid.c`, `src/iop/basecurve.c`)
- `dnglab/`: rawler camera TOMLs
- `dcp/`: RawTherapee DCPs and `dump.txt`
- `pv/`: my edit scripts and `curves.py`, the curve calibration table

## Scope agreements

- Other agents own `colorops.rs`, the highlights / shadows / clarity / texture code in `local.rs`
  and `finish.rs`, `masks.rs`, and lc-raw `demosaic/` and `highlight/`.
- The tone stage must hand display-linear Rec.2020 in 0..1 to the colour ops.
- The highlights agent applies a scene gain before the tone curve and needs the curve not to clip
  above 1.0.
