# J27 — export dialogs: one kernel list (Claude builder)

Status: complete. One commit on `wip/cl-j27-export-kernels`.

## Starting point (measured from the code, 2026-10-14)

- Library Export (`lc-ui-egui/src/panels/export_dialog.rs`): no resample choice. Photos are
  scaled inside the Develop render (`lc-pipeline` Frame sampling: Mitchell area pre-filter with
  the legacy f32 taps, then one bilinear sample; GPU twin in `lc-gpu`).
- Editor Export As (`pc-ui-egui/src/export_dialog.rs`): no resample choice either; the export
  always ran Image Size with plain `bicubic` (Catmull-Rom, f64 taps from `li-raster`).
- Neither dialog offered the other's kernels (audit D1, slice 4).

## Backend (done)

- `li-raster::resample::ResampleChoice`: the ONE list both dialogs show (8 entries, Photoshop
  order: Bicubic Automatic, Preserve Details, Bicubic Smoother, Bicubic Sharper, Bicubic,
  Lanczos, Bilinear, Nearest Neighbor), with key/label/hint and `resolve(enlarging)` (Photoshop's
  Automatic rule). Keys are the editor's existing `image.imageSize` `resample` names.
- `li-raster::resample::resize_interleaved`: dense adapter over the editor's own streamed engine
  (`resize_source`: f64 taps, premultiplied alpha, edges repeated, bounded reads). Reads u8/u16/f32
  in place, so a 16-bit render is never copied to float before resizing.
- `photocraft-algo::resample::preserve_details_sharpen`: Image Size's Preserve Details USM,
  factored out unchanged so Library export applies exactly the same step.
- `lightcraft-engine::export::ExportOptions::resample: Option<ResampleChoice>` (`None` = the
  Develop renderer scales, as before — default, so existing presets/exports are unchanged; not
  serialised when `None`). With a kernel and a real resize (no AI enlargement), the photo is
  developed at its full cropped size (8-bit exports develop at 16-bit so the resize never reads
  dithered 8-bit samples), resized with the shared engine, then dithered to 8 bits exactly like
  Develop's own 8-bit output (`finish::dither8`). 16-bit keeps 16-bit, 32-bit float stays linear.
  The memory estimate for the export permit counts the full-size render plus the output.

## Parity gate and measurements

- `j27_library_kernel_resize_matches_editor_image_size`: for all 8 choices, reduce (61×37→23×14)
  and enlarge (→150×91): Library export result vs the editor's Image Size
  (`resize_surface_in_canvas`), max abs difference ≤ 1e-6 (passes) — Library export with a kernel
  IS the editor's result, Preserve Details included.
- Zone plate 1024→256 (synthetic), RMS error; "detail" = where the pattern is still resolvable
  (lower = sharper, more faithful), "moire" = where it is beyond the new Nyquist limit (lower =
  less aliasing). Single run, this machine; timings are one sample, not a benchmark.

| Resampler | detail RMS | moire RMS | ms |
|---|---:|---:|---:|
| Develop renderer (current Library default) | 0.0615 | 0.0128 | 2.9 |
| Bicubic Automatic / Bicubic Sharper (reducing) | 0.0145 | 0.0180 | 3.9–6.1 |
| Preserve Details / Lanczos (reducing) | 0.0070 | 0.0162 | 8.0–9.2 |
| Bicubic | 0.0270 | 0.0203 | 5.5 |
| Bicubic Smoother | 0.0615 | 0.0133 | 5.7 |
| Bilinear | 0.0756 | 0.0164 | 3.8 |
| Nearest Neighbor | 0.0374 | 0.0611 | 2.8 |

  Reading: the renderer's scaling equals Bicubic Smoother (Mitchell) — the softest-but-cleanest
  look. Lanczos keeps ~9× more fine detail with ~27% more residual moire; Bicubic Sharper sits
  between. The renderer remains the fastest and least memory-hungry path for batch exports.
- Demo photo end-to-end (`j27_demo_export_with_kernel`, 320×213 PNG, warm): renderer 2698 ms,
  Lanczos 2947 ms (+9%); mean difference 1.8 levels (same develop, different resampler).
  Real raws pay more for the full-size develop; the renderer path stays the default.

## Dialogs (done)

- Library Export ▸ Image Sizing: new **Resample:** row (shared `widgets::choice` dropdown,
  `dropdown:exportResample`) under the size fields: `Develop renderer (fastest)` (default,
  unchanged behaviour) followed by the 8 shared kernels. A dim hint explains the choice; with a
  kernel a second hint says it develops at full size and matches the editor's Image Size. The
  collapsed section summary shows the kernel. Saved in presets / Export with Previous as
  `resample` (absent when the renderer is used, so existing presets are byte-identical).
- Editor Export As ▸ Image Size: new **Resample** dropdown (shared
  `widgets::dropdown_with_tooltips`, hints as tooltips) with the same 8 kernels, remembered with
  the other Export As settings (`file.exportAs.resample`). Default `Bicubic` = exactly what
  Export As always did; Quick Export and old prefs keep Bicubic. The 360 px preview and size
  estimate proxies stay bilinear (speed), as before.

## User-visible changes

- Both dialogs gain a Resample menu with the same 8 kernels. Defaults are unchanged in both
  products (Library: Develop renderer scaling; editor: Bicubic), so no existing export changes
  unless the user picks a kernel.
- OWNER DECISION SUGGESTED (not applied): by the zone-plate numbers above, Lanczos keeps ~9×
  more fine detail than the Library's current renderer scaling (slightly more moire, and a
  full-size develop per photo: slower, more memory). The audit suggested Lanczos as the
  Lightroom-style quality default for Library and Bicubic Automatic for the editor (Photoshop's
  Export As default). Either is a one-line default change (`ExportOptions::default().resample`,
  `DEFAULT_RESAMPLE` in `pc-ui-egui/src/export_dialog.rs`).

## Tests (new only, filtered by name `j27_`)

- `li-raster`: `j27_choice_keys_round_trip_and_resolve`, `j27_dense_resize_exact_size_and_flat_field` — 2 passed.
- `lightcraft-engine`: `j27_library_kernel_resize_matches_editor_image_size` (parity gate, ≤1e-6
  for all 8 kernels, reduce and enlarge), `j27_kernel_resize_depths`,
  `j27_resample_param_round_trips_and_defaults_off`, `j27_demo_export_with_kernel`,
  `j27_measure_zone_plate` (measurement) — 5 passed.
- `lightcraft-ui-egui`: `j27_resample_choice_round_trips_and_lists_the_shared_kernels` — 1 passed.
- `photocraft-ui-egui`: `j27_export_as_uses_the_chosen_shared_kernel` (each choice == Image Size
  with that key, at 40% and 250%; no choice == Bicubic) — 1 passed.
- `cargo check`, `cargo fmt` and strict clippy (`--all-targets -D warnings`) on li-raster,
  photocraft-algo, lightcraft-engine, lightcraft-ui-egui, photocraft-ui-egui: clean.
- No full suites, GPU runs or benchmarks (owner rule). TEST-CHANGED: none.

## Outside edits (minimal, needed for one backend)

- `crates/li-raster/src/resample.rs`: `ResampleChoice` (the shared list), `Sample` trait and
  `resize_interleaved` dense adapter over the existing `resize_source`; 2 tests.
- `crates/pc-algo/src/resample.rs`: re-export `ResampleChoice`; Preserve Details USM factored
  into `pub fn preserve_details_sharpen` (same parameters, same call; Image Size unchanged).
- `crates/lc-engine/src/export.rs`: `ExportOptions::resample`, full-size-develop + shared resize
  path (`resample_rendered`), memory estimate, param docs; 5 tests.
- `docs/wip/i18n-pending-j27-export-kernels.md`: new strings for the i18n pass.
- No Cargo.toml / Cargo.lock changes (all edges already existed).

## Left / follow-ups

- The Develop renderer's own scaling (Mitchell pre-filter + bilinear, CPU and GPU) is unchanged;
  folding it onto `li-raster` kernels belongs to a pipeline/GPU job (goldens + GPU parity).
- `pc-engine::image_cmds::parse_resample`/`auto_resample` could forward to
  `ResampleChoice::from_key`/`resolve`; left alone (not my file, behaviour identical).
- The kernel path develops at full size: very large photos use more memory/time; the export
  permit accounts for it. A tiled develop→resize stream would be a future optimisation.
