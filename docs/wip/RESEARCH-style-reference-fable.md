# Research: "Style reference" / Match Look for Local Image

Status: in progress (written incrementally). Author: Claude Fable 5.1, 2026-10-13.
Owner question: "Capture One has a style reference feature: it analyses the edits on a photo and then makes other photos look like that. Can we recreate it?"

Sections are appended as research completes.


## v1 implementation design (Codex continuation, 2026-10-13)

The inherited file above contained only its introduction when this task began; no code map,
phased design or numbered acceptance list had been appended. This section records the explicit
owner requirements implemented by the continuation. Learned v2 is excluded.

Sources read: Capture One's official [Match Look Tool](https://support.captureone.com/hc/en-us/articles/22188770298269-Match-Look-Tool)
for the four groups and Impact/Analyze workflow; LightCraft commit
`8b9a0200843d9ef4d47e3d72f6ca7f53531e4bdc` for calibrated Auto and the held-out evaluation
methodology; this repository's `DESIGN-SYSTEM.md`, threading review, Sync `Op::Batch`,
`cmd/cull.rs` preparation/worker/application pattern, `tasks.rs`, library `PrefsFile`, atomic
catalog `Store` and stacked Develop report. The matching algorithm below is original,
deterministic CPU work, not a reverse-engineered claim about Capture One internals.

- Analyze renders every edited reference through the complete pipeline at 64 px, with its source also bounded to 64 px and sensor-scale compensation (including
  crop and layers). It records OkLab lightness quantiles, neutral chromaticity, global chroma,
  shadow/midtone/highlight chromaticity, a conservative skin window, and conditional chroma
  in eight established hue bands. Empty/unsupported samples have zero weight.
- The reference's unedited source contributes a ten-component scene descriptor: hue occupancy
  and tone spreads rather than absolute brightness. Per-target weights are normalized `exp(-8*d²)` with a
  floor. Consistent batch uses equal weights. Blending respects each feature's support.
- The target's own source/settings are rendered repeatedly through `render_cached`. Two
  stable-order coordinate sweeps use a finite-difference response and a bounded secant step,
  accepting only decreases in the supported perceptual objective. Normalize solves Exposure
  and WB; Light solves Contrast/Highlights/Shadows/Whites/Blacks; Color Adjust solves
  Saturation/Vibrance; a final bounded, rendered palette correction solves eight mixer saturation controls jointly from supported band-chroma errors. Color Tone solves three grading
  wheels in Cartesian coordinates. Strength interpolates editable controls, including circular
  grading via Cartesian vectors. Geometry, retouching, detail and other tools are retained.
- Layer mode inserts a Filled Develop layer and evaluates the entire resulting stack for every
  candidate. Background and lower layers are preserved. Exact self matches, zero Strength,
  disabled groups and unsuccessful improvements are no-ops.
- Decode/analysis/solve run in Utility-priority workers with cancellable `heavy_jobs` memory
  admission. The decoder reservation is based on source dimensions. A batch commits only on
  completion, in one `Op::Batch`; cancellation commits nothing. Library generation and
  captured photo identity/settings reject stale worker results.
- Current references/analysis/selected look persist in library `prefs.json` with defaults.
  Reference edits or removals invalidate current analysis. Saved looks are global, versioned
  `Presets/match-looks.json`, using the desktop's existing `app_dirs` implementation and atomic
  `Store` writes. They contain name, UUID, creation date, count, statistics and scene descriptors;
  no photo IDs, settings, images or paths. The engine supports import/export JSON, preserving
  measurements and assigning fresh imported IDs. Invalid/future files are not overwritten.
- Develop's collapsible Match Look section has a reusable wrapped thumbnail tray, Analyze,
  status/summary chips, saved-look picker and management. Its modal requires a current preview
  before Apply. New controls use shared widgets and tokens; all commands are contributed to
  the existing profile catalog, with unassigned defaults to avoid profile conflicts.

Code map (symbols are stable when line numbers drift): `lc-pipeline/src/match_look.rs`
(`measure`, `analyze`, `weights`, `blend`, `solve`); `lc-engine/src/cmd/match_look.rs`
(`State`, `SavedLooks`, `prepare_analysis`, `apply_analysis`, `prepare`, `apply`, `specs`);
`lc-engine/src/library.rs` (`PrefsFile`); `lc-ui-egui/src/match_look.rs` (`panel`, `body`,
`analyze`, `start`, `progress`); `lc-ui-egui/src/widgets.rs` (`reference_tray`,
`summary_chips`, `checkbox`, `text_action`); `tests_match_look.rs` (real-click walkthroughs).

### Fourteen acceptance areas

1. Exact self-match, no settings/history/undo change.
2. Different illumination: Match Look differs from Sync and lowers rendered error.
3. One to ten references, capped without partially replacing the tray.
4. Target-specific similarity weights and one consistent blend across a batch.
5. Four groups isolate editable controls; absent support cannot drive a color correction.
6. Strength zero/half/full and editable results.
7. Conservative skin statistics improve a cast portrait without model loading.
8. Filled layer builds on the finished stack and keeps Background/lower layers.
9. One undo restores the complete batch.
10. Identical sliders/statistics across one/four worker threads.
11. Cancellation, stale photo edits and library switching commit nothing obsolete.
12. No ONNX loading/download; old preferences and settings remain readable.
13. Real clicks: add/remove references, Analyze/wait, stale state, groups, Strength, preview,
    Apply, undo, Save Look, pick saved/current, reload and apply saved look.
14. Saved-look persistence/rename/delete/duplicate/update/import/export, dark/light 250/400 px
    screenshot inspection, app-path 500-photo timings and simulated low-end profile.

Known algorithmic limit: distribution matching is a starting-point look, not semantic object
correspondence or recovery of clipped data. Skin selection is conservative chromatic evidence,
not a required face detector. References with substantially different scene content may require
manual finishing. This v1 does not infer arbitrary curves, local masks, grain or spatial style.
