# Backlog

Open items carried over from the retired `docs/wip` working notes (bug-fix backlog of 2026-10-09 and the
2026-10-13 duplicate-systems audit). Items that were completed since then are not listed.

## Known issues
- NVIDIA exit crash: SIGABRT after a long Library test run because the driver's shader compiler is still active during Vulkan unload; `LIGHTCRAFT_GPU=0` avoids it.
- NVIDIA banded-render compiler hang: intermittent and not reproduced in five RTX 5090 runs (three with disk caching requested off); the compute-off fallback `LIGHTCRAFT_GPU=0` avoids the path, the driver cause is unknown.
- ComfyUI is stopped on a normal quit but is still left running if the app crashes or is killed (needs a watchdog); idle model unloading is not implemented.
- PSD v7 nondefault slice alignments: the writer follows the Adobe SDK terminology and keeps legacy read aliases, but no Photoshop-saved nondefault-alignment fixture is available to confirm it.
- Translations are complete in all 12 languages for both apps but have had no native-speaker spot checks; Camera Raw contextual labels elsewhere (AI panel `tr_ctx` and shared widgets) may double-translate.
- Vectorizer competitor gates (vtracer, potrace and Inkscape references): results still pending.

## Consolidation leftovers
- Develop brush dab generation (`lc-pipeline::masks::brush_dabs`) is still a separate, simpler copy of `pc-paint::dynamics`; moving it over would give Develop the pressure, smoothing and spacing dynamics so tablet feel matches the editor.

## Develop colour follow-ups
- Camera look, Automatic profile: an embedded DNG `ProfileToneCurve` (DJI DNGs carry one) still loses to the generic or maker curve when no JPEG fit is accepted. The decoder (`lc-engine/src/files.rs`) should tell an embedded DNG profile from an auto-matched DCP and put the embedded curve before `basecurves::camera()` in `look_curve`; `files.rs` `dng_profile_look_is_applied` should then assert that `tone_map(...).apply(0.18)` follows the embedded curve.
- Auto white balance from import-default presets, `develop.merge` partials and CLI partials that only say `wb.mode: auto` keeps the stored (as-shot) numbers under the Auto label. Paste, Sync, Auto Sync and presets measure it (`lc-engine/src/auto_wb.rs`); these paths should queue the photo the same way, or the first decode should resolve a pending Auto.
