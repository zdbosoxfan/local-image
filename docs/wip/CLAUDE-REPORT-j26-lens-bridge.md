# J26 lens profile through the bridge + red-eye (Claude builder)

Status: **done**. Audit item D21 (docs/wip/AUDIT-duplicate-systems-fable.md §3 D21, §4 J26).
Built on the lens-correction unification (CODEX-REPORT-lenscorr.md) and the GPU Camera Raw
bridge (CODEX-REPORT-cr-gpu.md), both already in this branch's base. No helpers were used.

## What merged

### Lens: the editor uses Develop's lens model (no second database)

- `photocraft_io::develop_filter::LensProfile` is the editor's only way into lens profiles. It
  detects camera + lens from the document's EXIF with `lightcraft_engine::lens_db::detect`,
  resolves the correction with `lens_db::correction` (the exact query Develop's lensfun fallback
  makes for a photo) and evaluates it with `lightcraft_pipeline::lensdb::LensMap` (Develop's
  `to_source` / `gain`). Stored as JSON (`"lensProfile": {lens, camera?, focal, aperture?,
  orientation?}`) so smart filters re-render without EXIF.
- Sanitize rule (D21): a profile only fits the camera's whole frame. `from_exif` refuses an
  image whose shape differs from the recorded pixel size (cropped); resized images are fine (the
  model is resolution independent); a portrait shot shown upright keeps its EXIF orientation for
  an off-centre optical axis. `sanitize` keeps the Camera Raw Filter's lens profile off
  (documented why, pointing to `LensProfile`).
- Filter › Lens Correction / File › Automate › Lens Correction: `"profile": "auto"` now means
  the lens database (generic profile only as the fallback for an unknown lens or a cropped
  image). `develop_filter::lens_correction_surface` divides the profile's vignetting out in
  **linear light** (document profile's TRC undone, as Develop does; RGB/Gray), then composes the
  profile's distortion + lateral CA with Photoshop's manual terms (pc-algo Brown–Conrady, CA,
  vignette, keystone, angle, scale) in **one resample** with the existing edge modes and
  premultiplied alpha. `lens_auto_scale` covers the frame through both stages. The command
  stores `"profile": "database"` + `"lensProfile"` and reports `lensDatabase` and the lens name.
- Embedded vendor/DNG data and imported LCP need raw sensor data, which an editor layer no
  longer has; the precedence embedded → lensfun → LCP is therefore lensfun-only here.

### Red eye: one algorithm (Develop's), pet-eye capable

- `paint.redEye` now runs `lightcraft_pipeline::redeye` (Develop's detector + renderer) through
  `develop_filter::red_eye_surface`, in linear light through the document profile. The click
  circle / drag ellipse is detected, refined once on an eye-sized ellipse around the found pupil
  (as if drawn there), and a click must land on a pupil: new shared
  `lightcraft_pipeline::redeye::confidence` (mean detection weight inside the found disc,
  threshold 0.35). Only changed pixels are rewritten; alpha keeps exact bits.
- New params `"pet"` and `"catchlight"`; the editor's Red Eye options gain **Pet Eye** and
  **Add Catchlight** (shared `widgets::checkbox`, strings on the pending list). History step is
  "Pet Eye" in pet mode.
- The editor's flood-fill detector (`pupil_mask`, `redness`, `correct`) is deleted; it had no
  other callers.

## Quality / speed (before → after)

Lens, Canon EF 24-70mm f/2.8L @ 24 mm f/4 (bundled lensfun), 360×240 smooth linear test image,
display-encoded difference to **Develop's own CPU render** with the same profile, interior:

| | mean | max |
|---|---:|---:|
| Before: editor "auto" = generic 24 mm profile | 2.705 /255 | 27.86 /255 |
| After: editor "auto" = lens database | **0.002 /255** | **0.013 /255** |

The profile's map is bit-identical to Develop's (`to_source`, `gain` asserted equal at
corners/centre for all planes) and the resolved correction equals Develop's query result.
Editor render 9–14 ms for 360×240 on CPU (one-shot, not a benchmark). Auto Scale: border pixel
centres now must sample ≥ ½ px inside the frame, so auto-scaled corners are fully opaque (before:
corner alpha 0.358 at 70 mm; this also applies to the generic/manual path).

Red eye (synthetic 8-bit sRGB scenes, Darken 50, Pupil Size 50; display-encoded; "outside" =
everything > pupil radius + 2 px):

| Scene | Old pupil red / level / outside mean, max | New pupil red / level / outside mean, max |
|---|---|---|
| Classic 60×60 (old unit test) | 0.000 / 0.063 / 0, 0 | 0.000 / 0.146 / 0, 0 |
| Eye with iris + catchlight | 0.000 / 0.081 / 0.0013, **0.243** | 0.000 / 0.149 / 0.0000, 0.012 |
| Large search radius (200 px) | 0.000 / 0.095 / 0.0040, **0.243** | 0.000 / 0.154 / 0.0000, 0.078 |
| Flushed skin (stress) | 0.000 / 0.081 / 0.0016, **0.600** (skin leak) | 0.000 / 0.149 / 0.0000, 0.016 |
| Pet eye (green glow) | not found | level 0.545 → 0.165 |
| Click on plain skin | nothing | nothing |

Speed at a 200 px click radius: old 11.1 ms, new 12.5 ms (single test thread; region conversion
parallelized, per-pixel work limited to the eye's box).

**User-visible changes:** Lens Correction "auto" applies the real lens profile when the lens is
in the database (stored as "database"; response `lensDatabase`); Auto Scale ≤ ~1 px larger so no
half-transparent corner pixel. Red Eye no longer darkens the iris/catchlight edge or leaks into
reddish skin; at the same Darken Amount the pupil comes out a little lighter (Develop's scale:
≈0.15 vs ≈0.06–0.10 encoded at 50 %; raise Darken for darker); Pet Eye + Add Catchlight added.

## Tests (only new ones run, per owner rule)

- `cargo check`, `cargo fmt` and `cargo clippy --offline --all-targets -- -D warnings` on
  lightcraft-pipeline, photocraft-algo, photocraft-io, photocraft-engine, photocraft-ui-egui:
  clean.
- photocraft-io `j26_` (3 passed): `j26_lens_profile_is_develops_lensfun_query`,
  `j26_lens_correction_matches_develops_render`,
  `j26_red_eye_through_develop_fixes_pupils_and_leaves_the_rest` (in
  `crates/pc-io/src/develop_filter_j26_tests.rs`).
- photocraft-engine `j26_` (2 passed): `j26_lens_correction_command_stores_the_database_profile`
  (auto → database, stored params re-render bit-identically as a smart filter, unknown lens →
  generic, opaque auto-scaled corners), `j26_pet_eye_command_darkens_a_glow_in_one_step`.
- The old-algorithm red-eye numbers came from a temporary measurement test (the deleted code
  copied verbatim), removed before commit.

TEST-CHANGED: `redeye_cmds::tests` only gains `use photocraft_geom::Rect;` (the production
module no longer imports it); its assertions are unchanged. Not run (owner rule); expected to pass
(pupil R ≈ 0.15 < 0.2, skin untouched, skin click finds nothing — same as the new classic scene).

## Outside edits

- `crates/lc-pipeline/src/redeye.rs`: additive `confidence()`; `detect` split into private
  `grid_samples` / `pupil_weights` helpers (same arithmetic; Develop's results unchanged).
- `crates/pc-algo/src/lens.rs`: additive `auto_scale_with` (Auto Scale through a profile stage);
  `auto_scale` checks border pixel centres (opaque corners).
- `crates/pc-ui-egui/src/{state,panels,canvas}.rs`: Pet Eye / Add Catchlight options and params;
  `i18n/mod.rs`: the three new strings on `PENDING_LRC_UX_TRANSLATIONS`.
- New file `crates/pc-io/src/develop_filter_j26_tests.rs` (tests).

## Left, and why

- Camera Raw Filter (`filter.develop`) Optics lens profile on whole-frame layers: still off. It
  needs `develop_filter_cmds.rs` + Camera Raw UI wiring (other jobs' files) and an alpha warp in
  `develop_rgba` — note: manual Optics distortion/CA in the Camera Raw Filter already moves RGB
  while alpha stays put (pre-existing; visible only on layers with transparency).
- The Lens Correction dialog has no lens picker or detected-lens readout (the generic schema
  dialog shows `none|auto|generic`); Develop's picker could be reused there later.
- Existing suites/GPU acceptance not run (owner rule); the editor's lens path is CPU (one
  resample), Develop's GPU path is unchanged.
