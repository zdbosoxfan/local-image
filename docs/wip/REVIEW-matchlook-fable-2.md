# REVIEW 2: Match Look (Fable)

Second independent review of branch `wip/cx-matchlook` at `1aae90be` (worktree `cx-matchlook`), 2026-10-13.
Read-only: no source was edited, no cargo run was needed to confirm the findings below (every finding is
backed by code citations; `file:line` refers to this commit). Inputs: `CODEX-REPORT-matchlook.md`,
`REVIEW-matchlook-fable.md` (my first review), `PLAN-matchlook-v2.md`, the three research reports,
`RESEARCH-style-reference-fable.md`, `DESIGN-SYSTEM.md`, the full diff `90d89ff3..1aae90be` (30 files), and
the neighbouring code the feature depends on (`media.rs`, `files.rs`, `smart.rs`, `local.rs`, `primary.rs`,
`finish.rs`, `controls.rs`, `people.rs`, `permits.rs`, `widgets.rs`, `dialogs.rs`, `tasks.rs`, `library.rs`).
Codex's harness outputs under `target/matchlook-review/` were read (summary.txt, metrics) but are not
reproducible from the repository (see N14).

Abbreviations: `pipeline` = `crates/lc-pipeline/src/match_look.rs`, `engine` = `crates/lc-engine/src/cmd/match_look.rs`,
`ui` = `crates/lc-ui-egui/src/match_look.rs`, `tests` = `crates/lc-ui-egui/src/tests_match_look.rs`.

## 1. Summary verdict

The feature is in much better shape than at the first review: all ten first-review bugs (B1–B10) are
genuinely fixed in code, not just claimed (section 2), the solver is now a bounded, regularised LM with a
warm start and a CDF seed, import is bounded before parsing, import/export has UI, and the workflow matches
Capture One's Analyze → groups → Impact → Apply model with sensible extras (locks, skin, contrast guard,
per-photo amount, compare wipe).

It is **not ready to merge yet** for the owner's main use (raw files) and for two correctness/UX problems:

1. **Raw is unproven.** Every quality number and the 500-photo timing were produced on JPEGs or synthetic
   512-px proxies with a stubbed loader (`tests:266`). The raw branch of the pipeline (camera profile, look
   curve, highlight clipping, lens, binned demosaic at Thumb level) is never exercised by any Match Look test;
   the raw-specific logic I traced (decode options, effective WB, relative-WB raws, embedded-preview raws,
   smart-preview fallback) is consistent on paper (section 3.D) but needs a real-raw end-to-end test, and the
   batch time on real raws will add a per-target raw decode that the benchmark never paid.
2. **Per-photo Strength drag floods undo/history.** Each drag frame commits a separate "Match Look amount"
   undo entry and history step (`ui:474-479` → `engine:607-611`), unlike every Develop slider (which coalesce
   through `begin_interaction`/`end_interaction`, `panels/edit.rs:24-36`).
3. **Monochrome references silently produce a wrong result** (muddy half-desaturated, possibly tinted),
   because nothing can set B&W treatment and saturation is bounded at ±55 (`pipeline:534`). Capture One's
   Color Adjust group explicitly covers "black and white".
4. The Match Look dialog bypasses the standard modal button row and forces a ~1000-px-tall window
   (`dialogs.rs:435-447, 564-570`), and `DESIGN-SYSTEM.md` was edited into a malformed table.

5. `close_library` now aborts before the journal snapshot if the optional Match Look amount file cannot be
   written (`library.rs:594`), a data-safety regression introduced by this branch.

Counts in the revision plan (section 5): **7 MUST (P1–P6, P6b), 9 SHOULD (P7–P15), 9 COULD (P16–P24).**

## 2. First-review items: fixed / not fixed / partially

| Item | Verdict | Evidence |
|---|---|---|
| B1 stale WB start for raw presets | **Fixed** | `pipeline:979-982` replaces temp/tint with `current_wb` for every non-Custom mode; `v2_tests::match_look_v2_raw_presets_effective_start_normalize_and_locks` (`pipeline:1566-1592`) compares preset vs explicit Custom start for Tungsten/Daylight/Auto/AsShot. `auto::current_wb` = `local::effective_wb` (`auto.rs:137`, `local.rs:117-124`), the same function rendering uses. |
| B2 whole batch discarded / "applied: null" | **Fixed** | `engine:423-453` skips only changed identities and returns `skipped`; `ui:200-213` uses `unwrap_or(0)` and reports the skipped count; names listed in the panel `ui:454-463`. Test `engine:739-746`. |
| B3 dangling `selected` id | **Fixed** | `reconcile_selection` (`engine:161-167`) on library open (`library.rs:137-139`), app start (`ui:75`), every panel frame (`ui:266`) and every saved-look action (`engine:487`); management row drawn only when the id resolves (`ui:406`). Test `engine:998-1001`. |
| B4 absolute layer WB | **Fixed** | `layer_settings` writes relative `adjust.temp/tint` (`pipeline:708-709`), `tools.wb` unset; mapping matches `primary.rs:389` (`exp(adjust.temp/100*0.6)`, `tint*0.4`). Test `pipeline:1594-1612` renders a later global WB change. |
| B5 ±2 EV / ±3200 K cap | **Fixed** | exposure bounds ±5 EV, temperature 2000–50000 K in log2 (`pipeline:528-530`); tanh-bounded LM steps (`pipeline:549-557, 1162-1166`); log-percentile warm start (`pipeline:1030-1043`). Harness: −3 EV targets now solve to +3.1/+3.5 EV (`target/matchlook-review/summary.txt`). |
| B6 Auto-tone side effect | **Fixed** | `auto.rs` is not in the diff; the >1.5 EV assertion is back (`auto.rs:149`). |
| B7 sliders on the rails | **Fixed (measured on JPEG only)** | ridge + L1-ish prior (`pipeline:1007-1013, 1154`), comfort boxes (`pipeline:526-541`), tanh reparameterisation. Codex's audit: Contrast ±100 in 0/12 (was 8/12). Not re-measured on raw. |
| B8 Apply needs Preview; option change kills preview | **Partially** | Apply no longer needs a preview (`dialogs.rs:566-568`, test `tests:383-385`). Any option change still clears the preview (`ui:541-543`) and there is no auto-preview; a user who tweaks Strength must click Preview again. Acceptable; listed as COULD. |
| B9 analysis snapshots in prefs.json | **Fixed** | separate `match-look-analysis.json` (`library.rs:109-120`), tolerant `read_match_look_prefs` (`library.rs:82-94`), legacy migration, test `engine:936-969`. Note: a malformed `options` sub-object now drops `references`/`selected` too (`unwrap_or_default` on the whole `State`, `library.rs:91`); harmless, listed as COULD. |
| B10 top-level Develop menu | **Fixed** | `menubar.rs:195-197` adds the three items under Photo next to Sync; test asserts no "Develop" menu (`tests:361-363`). |
| 5b import/export UI | **Fixed** | host pickers (`library_host.rs:1608-1620`), `file.importLook`/`file.exportLook` (`menus.rs:370-404`), panel "Look actions" menu (`ui:429-447`); test `tests:435-449`. |
| 5e per-file import bounds | **Fixed** | 2 MiB before parse (`engine:169-186`), name ≤128 chars/512 bytes, id ≤128, created ≤64, 1–10 refs, feature ranges (`engine:138-160`, `pipeline:101-118`); test `engine:971-1005`. |
| 7b admission reserves full-image bytes for a 512-px thumb | **Not fixed** | `engine:236` still reserves `width*height*32 + 16 MiB` of the full sensor size; harmless because `fits()` always admits when nothing is in flight (`permits.rs:159-161`). COULD. |
| 7c design-system docs | **Partially** | Widgets are documented, but the table is malformed (section 3.F, N11). |
| Progress window shared with culling | **Fixed** | `widgets::job_progress` (`widgets.rs:965-972`), used by `tasks.rs:103-105` and `ui:259-263`. |
| D labels / vocabulary | **Fixed** | "Add to Match Look References" (`grid.rs:887`), "Analyzed references" (`ui:296,299`), confirmed "Update from current references…" (`ui:417-426`, `dialogs.rs:458`), Normalize tooltip (`ui:516`), chips with units and hover explanation (`ui:372-379`), options remembered (`ui:544`, `state.options`). |
| D "Set as Match Look reference" vs "Add selected" | **Fixed** | both now say "Add…" (`grid.rs:887`, `ui:336`). |
| Quality thresholds from PLAN-v2 §7 (ΔE ≤ 3, L5/L95 improve, ≤ 80 s) | **Met on the JPEG harness** | ΔE 1.59, |ΔL5| 8.2→6.2, |ΔL95| 12.2→9.4 (CODEX-REPORT table). The harness file itself is not in the repo (N14). |

## 3. New findings

Severity: **H** = wrong result / data or undo damage / owner requirement, **M** = user-visible defect,
**L** = nit. Each has an N-number used by the plan in section 5.

### 3.A Correctness, undo, persistence

**N1 (H) Per-photo Strength slider commits one undo entry and one history step per drag frame.**
`ui:473-479` feeds every `SliderOut.value` (emitted on every dragged frame, `widgets.rs:220-232`) into
`develop.matchLookAmount`; `engine:606-611` commits each call (`s.commit("Match Look amount", op)`), and
`develop_op` pushes a history step each time (`lib.rs:694-701`). A 1-second drag = ~60 undo entries and ~60
History rows; Cmd+Z then walks back one pixel of slider at a time. Every Develop slider instead wraps a drag
in `begin_interaction`/`end_interaction` (`panels/edit.rs:24-36`, `lib.rs:703-725`), which commits one step.
The group reset buttons are single clicks and are fine.

**N2 (H) Monochrome references give a silently wrong result.** A B&W reference (treatment B&W or
saturation −100) measures chroma ≈ 0 (feature 7), zero support in the eight hue bands and ≈ 0 zone a/b.
The solver then drives `color.saturation`/`vibrance` toward their comfort bounds ±55/±65 (`pipeline:534-535`)
and the palette step toward −40 per band (`pipeline:1196`), which leaves 20–40 % chroma and lets the grading
wheels add hue to compensate. Nothing can set `treatment = Bw` (`settings.rs:20`; `LayerTools.treatment`
exists for layers, `settings.rs:1229`). Capture One's Color Adjust group explicitly includes "black and white"
(research, section 4). No test covers a monochrome reference or a monochrome target.

**N3 (M) Any video or non-library item in the selection rejects the whole batch.** `prepare` collects
`input()` results with `?` (`engine:342`), and `input` errors for `MediaKind::Video` / `!in_library()`
(`engine:232-234`). Selecting an album that contains one clip makes "Match Look…" fail with "Choose still
photos in the catalog" instead of skipping it. `Sync` tolerates mixed selections. Same for Analyze (`engine:264`),
where an error is correct (a video cannot be a reference) but the message should name the file.

**N4 (M) After Undo, the panel still shows "Applied to this photo" with a disabled slider and the wrong
reason.** The `applied` record survives undo (`engine:445-449` only inserts); `ui:470-472` then disables the
slider and `change_summary` says "Photo edited since Match Look; undo those edits…" (`ui:481`, `engine:589`)
when the user has in fact just undone it. Expected: when `develop_of(id) == record.before` the section
disappears (or says "Match Look was undone").

**N5 (L) Admission reservation uses full-sensor bytes for a 512-px thumb.** `engine:236` (first-review 7b).
Harmless today because `fits()` admits whenever `s.bytes == 0` (`permits.rs:160`), but it will serialise
Match Look behind any other heavy job on small machines for no reason. Use the Thumb edge:
`min(w,512)*min(h,512)*32 + 16 MiB`.

**N6 (L) `read_match_look_prefs` drops the reference tray and the selected look when only `options` is
malformed** (`library.rs:91`, `unwrap_or_default` on the whole `State`). Decode `references`, `options`,
`selected` field by field.

**N25 (H) `close_library` aborts before the journal snapshot if `match-look-applied.json` cannot be written.**
The branch inserted `self.save_match_look_applied()?;` at `library.rs:594`, ahead of `persist()` and the
closing `snapshot_with_unlogged` (`library.rs:595-608`). A failed write of this optional amount-history file
(disk full, permissions) now returns `Err` early and skips saving queued catalog ops — a data-safety
regression unrelated to Match Look's own data. Plan item P6b.

### 3.B Threading, cancellation, performance

**N7 (H) Batch time on real raws is unmeasured and the design is sequential.** `SolveWork::run` solves
targets one after another on one Utility thread (`engine:368-418`); each target first needs a Thumb-level
source, which for a raw that is not in the `thumbs` LRU (`media.rs:216, 359`) is a full file read plus a binned
bilinear demosaic (`files.rs:288-292, 369-387`). The 500-photo benchmark replaced the loader with a closure
returning one cached 512-px proxy (`tests:264-266`), so it measured solve time only (Codex: 83 s on the first
v2 run, later faster "with caching"; my first-review harness: 118–133 ms/solve at dev opt-level 1, Codex's
summary: 103–181 ms and 51–74 renders per target). Adding even 100–300 ms of raw decode per target puts 500
raws at 2–4 minutes on the owner's desktop and far more on a laptop. The render itself is rayon-parallel
inside, but at 60–128 px the per-call rayon overhead dominates, so a 16-thread machine is mostly idle. The
owner's requirement is "adapts to any hardware; 500 ≈ 80 s". The fix is to solve targets concurrently with a
worker count derived from `available_parallelism` and memory admission (plan item P5), which also parallelises
the decodes.

**N8 (M) Cancel while a job is blocked in `admit` is fine; cancel mid-batch is tested only after
completion.** `tests:163-183` clicks Cancel immediately; with demo images the job may already be finished,
and the test passes because the completion callback discards results when the cancel flag is set
(`ui:144-147`). That proves "cancel after finish commits nothing", not "cancel mid-batch stops the loop". Use a
slow stub loader (sleep per photo) to prove the loop exits early and the progress window closes.

**N9 (L) Preview and Apply share the `control` slot, so Apply is disabled while a preview is running
(`dialogs.rs:566`)** — correct — but opening the dialog does not cancel an in-flight preview from the previous
open (`menus.rs:357-359` only clears `preview`), so the first click on Preview reports "Match Look is already
running" (`ui:78-80`) until the stale preview finishes. Call `cancel_preview` in `dialog.matchLook`.

### 3.C Solver quality risks (no measured regression, but worth a test or a guard)

**N10 (M) "Protect skin" without faces anchors brick, wood and sand.** With no cached faces the fallback
skin window is any pixel with `a,b ∈ (0.008, 0.16)`, `c < 0.2`, `0.12 < L < 0.9` (`pipeline:170-171`), i.e.
every warm low-chroma surface. Skin features then get weight 3.0 (`pipeline:465-467`) and a 20° hue penalty
(`pipeline:1023`), with `protect_skin` defaulting to on (`pipeline:57`). For the owner's architecture work this
means a brick or timber facade is treated as skin and Color Tone is held back on exactly the surfaces the look
is about. Suggest: without cached faces, weight 1.0 and no hue penalty (window = "warm neutral" statistic only);
with faces, keep the current behaviour. Tooltip already says "uses cached People faces when available".

**N11 (L) MAD outlier rejection floor is one-size-fits-all.** `weighted_median` rejects beyond 3·MAD with
`MAD ≥ 0.005` (`pipeline:277-278`) for every statistic. For L quantiles (range 0–1) 0.005 is tiny: ten
"exterior" references where seven cluster within ±0.01 and three sit 0.08 brighter will discard the three in
Consistent mode (equal weights). For a/b (range ±0.1) the same floor is generous. Use per-feature floors
(L: 0.02, a/b/chroma: 0.005) and only reject with ≥ 4 samples.

**N12 (L) Probe statistics are read from 8-bit sRGB renders.** `measure_impl` decodes `Rgba8` through a
256-entry table (`pipeline:128-137`). Finite differences for Tint (step 3, `pipeline:588`) and the grading
wheels (step 4) move a/b by ~0.002–0.004, about one 8-bit quantum at mid-grey; the mean over thousands of
pixels dithers this out, so it is acceptable, but a `Rendered::deep` float path (`lib.rs:219, 245`) would make
the Jacobian cleaner at no extra render cost. COULD.

**N13 (M, raw) Proxy-vs-full-size discrepancy for clipped highlights is unmeasured.** `measurement_source`
box-downsamples the *linear* source to 60/128 px (`pipeline:342-350`); clipped raw pixels average with their
neighbours and fall below `raw_clip_level`, so highlight reconstruction and the look curve's shoulder behave
differently in the proxy than in the real 2560-px render the user sees. The L95 target may be met in the
proxy and missed on screen. The real-raw test (P1) must report proxy-vs-full L5/L50/L95 deltas.

**N14 (M) Codex's quality harness is not in the repository.** The report cites `target/matchlook-review/…`
and an instrumented `zz_matchlook_review-v2.rs` that "was removed from crate sources". Nobody can re-run the
ΔE / rail audit after the next solver change. Check it in as an `#[ignore]` integration test gated on an
environment variable pointing at the CC0 JPEG set (and, per P1, the raw set).

### 3.D The raw path end to end (owner's main use)

What I traced, and where it is consistent:

- **Decode options.** Analyze and Apply both build inputs through `input()` → `s.media.source_ref(&photo,
  SourceLevel::Thumb)` (`engine:248`), which keys the LRU and the loader by `RawOptions::of(&p.develop)`
  (`media.rs:435-449`), so a target with non-default Raw Processing (demosaic, highlight options) is solved on
  the same decode Develop renders. `src.info_or(i.info)` prefers the decoder's facts (`engine:284, 378`).
- **As-shot / camera WB.** The solver's start WB is `auto::current_wb` = `local::effective_wb`
  (`pipeline:974`, `auto.rs:137`, `local.rs:117-124`), the function the renderer uses. `WbMode::Auto` has no
  preset but `develop.wb` stores the resolved Kelvin when the mode is chosen (`develop.rs:308-321`), so the
  `unwrap_or((s.wb.temp, s.wb.tint))` fallback is right. Relative-WB raws (file-local look + fallback matrix,
  `files.rs:460-462`) and raws shown from their embedded preview (`media.rs:788-792`) both report
  `relative_wb: true` and are treated like JPEGs by the same function.
- **Kelvin/tint domains.** Raw temp is scene Kelvin; JPEG temp is a 6500-K-relative model (`settings.rs:276-278`,
  `panels/edit.rs:529-536`). The solver works in log2(temp) with a scale of 0.4 and bounds 2000–50000 K for
  both (`pipeline:492, 509, 528-529`), so the prior and the step size are domain-independent; `fade_controls`
  copies the whole solved `wb` (`pipeline:733`), never a difference, so there is no cross-domain arithmetic. Layer
  mode stores a ratio (`pipeline:708`) relative to the photo's own effective WB. A mixed raw/JPEG batch is
  therefore fine; raw reference → JPEG target and the reverse only exchange rendered statistics.
- **Offline originals.** `source_ref_default` returns `SourceRef::Smart` when the original is known offline and
  a valid smart preview exists (`media.rs:461-467`), and the file variant carries the smart preview as a
  fallback (`media.rs:189-193`). The smart preview stores the raw's `SourceInfo` (raw flag, as-shot WB, camera
  profile, look curve: `smart.rs:360-372`), so solved settings mean the same thing when the original comes back
  online. One caveat: `raw_key: 0` (`smart.rs:241`) — a photo with non-default raw options falls back to a
  default-options proxy; Develop has the same limitation.
- **Faces.** Stored rects are in the full EXIF-upright uncropped frame (`people.rs:663-686, 707-710`);
  `framed_faces` applies the user orientation, warp and crop forward (`pipeline:352-398`). Consistent.

What is missing: **no Match Look test loads a raw.** The "raw" unit tests use `SourceInfo { raw: true }` on a
synthetic gradient (`pipeline:1526, 1568`), which exercises `effective_wb` but not the camera profile, look
curve, clip level, binning or lens path. The report's ΔE numbers are JPEG-only. Raw-specific risks: N7
(decode time per target), N13 (clipped highlights in the proxy), and the untested interaction of the solved
`light.whites`/`highlights` with raw highlight reconstruction. Plan item P1 is the owner's required real-raw
end-to-end test; P5 covers the time.

### 3.E UX versus Lightroom Classic / Capture One

**N15 (M) The dialog is forced to ~1000 px tall and uses a non-standard footer.** `dialogs.rs:435`
computes `match_height = section_h × 18` (= 1026 px with `section_h` 57, `theme.rs:105`), capped at screen
height − 114, and applies it as `min_height`, `default_height` and `max_height` (`dialogs.rs:443-447`). Ten
checkboxes, a slider and an empty 400-px preview box therefore open as a near-full-screen modal. The footer
`return`s early with `text_button`/`text_action` (small bordered Develop-panel buttons, `dialogs.rs:564-570`)
instead of the `ui.button` Cancel/OK row every other Library modal uses (`dialogs.rs:582-588, 621-626`). Size
to content (let the preview area define the height; `max_height` ≈ 80 % of screen) and draw the standard
footer with `ui.add_enabled(..)` + hover reason, as `ConfirmDelete` does (`dialogs.rs:603-607`).

**N16 (M) Panel is crowded; management actions belong in an overflow menu.** With a saved look selected the
panel shows eight text buttons plus a "Look actions" dropdown (`ui:326-447`): Add selected, Analyze, Save Look…,
Match Look…, Rename…, Duplicate, Delete, Update from current references…, then a combo-styled "Look actions"
(Import/Export). Capture One's tool is: reference strip, Analyze, groups, Impact, Apply, with presets behind
the tool's "⋯" menu; Lightroom panels use "+"/"⋯" header buttons. Move Rename/Duplicate/Delete/Update/
Import/Export into one overflow menu opened from an icon button in the section header row (the `dropdown`
chevron labelled "Look actions" reads as a picker, not a menu). Keep Add selected, Analyze, Save Look…,
Match Look… visible.

**N17 (M) Saved-look picker virtualisation uses the wrong row height.** `ui:301` calls `show_rows(ui,
interact_size.y, …)` (22 px, `theme.rs:235`) but each row is a 24-px `text_button_translated` plus
`item_spacing.y` (`widgets.rs:663`). After scrolling a long list the visible range is computed from 22 px rows
while 24+ px are drawn, so rows drift/overlap. Pass the real row height (`24.0 + item_spacing.y`) or use a
non-virtualised `ScrollArea` with `show_rows` removed (1000 buttons is still cheap in egui).

**N18 (L) Spelling.** "Colour only" / "skin colour" (`ui:532, 527`) beside "Color Adjust" / "Color Tone"
(`ui:518-519`) in the same dialog. The rest of the Library UI uses "Color" (13 vs 3 occurrences).

**N19 (L) Export default file name is "Match Look.json"** (`menus.rs:395`) regardless of the look; default to
`<look name>.json` (sanitised).

**N20 (L) Photo ▸ "Analyze Match Look" is never disabled** (`ui_enabled` has no arm, `menus.rs:1488`); with an
empty tray it toasts "Choose 1–10 reference photos". Gate it on `!references.is_empty()` like the panel does.

**N21 (L) Reference thumbnails are 45 px** (`widgets.rs:713`, `slider_row_h`), too small to recognise which
exterior is which; use `thumb_bucket(2 × slider_row_h)` sized tiles (the tray already wraps).

**N22 (L) "Compare with reference" always shows the first reference** (`ui:552-561`); with ten references the
user cannot pick the one they care about. COULD: cycle with a click or show the highest-weight one.

### 3.F Design-system and documentation

**N23 (M) `docs/DESIGN-SYSTEM.md` table is malformed.** The Library (`lc`) widget table has three columns
(`| Widget | Purpose and usage | Line |`), but the nine new rows have five cells each
(`… | Match Look options; widgets.rs | 871 | 871 |`), and the pre-existing `checkbox` row was corrupted to
`| … | 705 | 871 | 871 |` (`DESIGN-SYSTEM.md:113` and `:154-162`). The rows also point at line numbers that
drift. Rewrite the rows in the table's format (`| name | purpose; signature; used by | line |`).

**N24 (L) `text_button_enabled` now elides every text button app-wide** (`widgets.rs:660-663`, `max_rows = 1`
clipped to `ui.max_rect().width() − 18`). Probably right, but it is an app-wide behaviour change smuggled in
with the feature; needs a one-line note in the report and a check of the Develop panel at 250 px (the ignored
screenshot test only covers the Match Look section).

### 3.G Tests: what they prove and what they do not

- `pipeline` unit tests are on 64×48 synthetic gradients; they prove mechanics (no-op, locks, layer, strength
  interpolation, determinism, cancel, curve plateau guard) but not quality on photographs — that is the
  out-of-tree harness (N14).
- `engine` tests cover transactions well (stale identities, generation, skipped rows, one undo, persistence,
  migration, import bounds, 1051 looks).
- `tests` (real clicks) cover the workflow; the two heavy ones (`screenshots`, `500_photo_app_path_benchmark`)
  are `#[ignore]` and the latter needs a physical GPU and stubs the loader.
- Missing: monochrome reference (N2), per-photo Strength drag = one undo (N1), video in selection (N3), undo
  then panel state (N4), mid-batch cancel with a slow loader (N8), any real raw (3.D), the harness (N14).

## 4. Research notes (sources)

Nothing from the owner's catalog or any photo left the machine; the fetches below were of public source
files and documentation only.

1. **darktable `src/iop/exposure.c`** (GPL-3.0-or-later; fetched from
   https://raw.githubusercontent.com/darktable-org/darktable/master/src/iop/exposure.c). Deflicker:
   `raw_ev = -log2(raw_max) + log2(raw_val)`, `correction = target_level - ev`; spot mode: `white = Y_in /
   Y_target`, `exposure = -log2(white)` = `log2(Y_target / Y_in)`. **Our adaptation is correct**: `pipeline:1033`
   uses `3·log2(goal_L50 / current_L50)`; Oklab L ≈ Y^(1/3) for neutral colours, so `3·log2(L ratio)` =
   `log2(Y ratio)`, exactly darktable's spot formula applied to the median (a percentile, as in deflicker).
   The seed is accepted only if the objective improves (`pipeline:1039`), which guards the cube-root
   approximation for strongly coloured scenes.
2. **RawTherapee `rtengine/histmatching.cc`** (Alberto Griggio 2018, GPL-3.0-or-later;
   https://raw.githubusercontent.com/Beep6581/RawTherapee/dev/rtengine/histmatching.cc). Builds 256-bin
   luminance CDFs, `findMatch` walks to the equal-CDF bin, bins outside the populated `[min_val, max_val]` of
   either histogram are marked `-1` and skipped; `mappingToCurve` picks ~8 points, prunes concavities in the
   upper S, falls back to linear with < 4 points, resamples on a 0.05 ×1.4 geometric grid; **no explicit
   monotonicity or slope constraint**. Our `cdf_curve` (`pipeline:318-338`) is a faithful reduction to five
   stored quantile knots: unpopulated/clipped tails (`x < 0.002`, `x > 0.998`) map to identity (RT's `-1`), and
   it *adds* a [0.5, 2.0] slope clamp outward from the median, which comes from Hu et al. "Exposure"
   (https://arxiv.org/abs/1709.09602, tone-curve slope clamp) — a sensible improvement over RT, since the
   editor's monotone repair (`finish.rs:47-50`) would otherwise flatten the curve into plateaus; `guard_curve`
   (`pipeline:655-699`) re-implements the exact region basis of `finish.rs:31-46` (same 0.22·cos window and
   4x(1−x) envelope) and is verified against the real LUT in `pipeline:1688-1706`. RT's concavity pruning is
   not needed with five knots. Attribution rows exist (`docs/PORTS.md:910-912`, `licenses/*-NOTICE.md`).
3. **darktable `src/iop/colormapping.c`**: only the deterministic a/b cluster centres with Shepard
   (inverse-square) weights are used (`pipeline:283-315`), and only as the neutral-chromaticity fallback when
   the neutral mask has no support (`pipeline:950-960`). That is a minimal, correct adaptation; no L-histogram
   pixel transfer or bilateral smoothing is ported (the sliders are fitted instead), which is the right call.
4. **Capture One Match Look** (official: https://support.captureone.com/hc/en-us/articles/22188770298269-Match-Look-Tool
   and https://support.captureone.com/hc/en-us/articles/22266038214173-Match-Look; blogs:
   https://blog.thomasfitzgeraldphotography.com/blog/2024/10/capture-one-165-match-look-a-remarkable-new-feature,
   https://alexonraw.com/?p=8580). Confirmed: Impact 40–125 % (ours 0–125, owner-accepted); four groups —
   Normalize (exposure, WB), Light & Contrast (contrast, brightness, highlights/shadows, whites/blacks,
   levels, luma curve), Color Tone (master and three-way colour balance, channel curves), **Color Adjust
   (saturation, Basic Color Editor, black and white)**; several references give a weighted average by
   per-target similarity; presets store references + groups + Impact; the result is ordinary editable tools.
   Known complaints (from `RESEARCH-matchlook-products.md`): dark-skin tint/contrast, excess contrast, yellow
   cast on evenly lit images, poor on snow/water/events, layered references analysed from the base layer.
   Our build addresses skin (with the N10 caveat), contrast guard, WB lock, similarity warning and renders
   references with their full stack. The one C1 behaviour we lack is B&W (N2).
5. **Lightroom Classic** has no reference-matching feature (only "Match Total Exposures" and Reference View);
   LR's Preset Amount slider (0–200 %) is the closest analogue to per-photo Strength, and LR coalesces a
   slider drag into one History step — the expectation behind N1.
6. **CC0 raw test data already recorded in this repo**: `docs/wip/CODEX-REPORT-ai-denoise.md:45` names
   raw.pixls.us file 4465 (Sony ILCE-1 ARW, CC0, SHA-256 `ddb72880…22a88`) and the RawNIND/NIND CC0 sets kept
   under `target/ai-denoise/eval`; raw.pixls.us publishes all samples under CC0. The real-raw test in P1 must
   use files like these through an environment variable (the existing `LOCAL_IMAGE_TEST_RAW` convention,
   `tests_gpu.rs:382-388`), never anything under `~/Desktop`.

## 5. Revision plan for Codex

Priority order within each tier. Each item: problem → evidence → exact change → test (fails before, passes
after) → risk. Keep everything inside the Match Look files unless stated; anything outside is flagged.

### MUST

**P1 — Real-raw end-to-end test: Analyze raw references → Apply to raw targets, measured (owner requirement).**
- Problem: no Match Look test loads a raw; all quality numbers are JPEG/synthetic (3.D, N13).
- Change: add `crates/lc-engine/src/tests_match_look_raw.rs` (`#[cfg(test)] mod` in `lib.rs`), one
  `#[ignore = "set LOCAL_IMAGE_TEST_RAW_DIR to a folder of CC0 raws"]` test plus a small always-on test (below).
  The ignored test:
  1. Reads every raw in `LOCAL_IMAGE_TEST_RAW_DIR` (≥ 4 files; document the CC0 sources + SHA-256 in
     `docs/wip/CODEX-REPORT-matchlook.md`, e.g. raw.pixls.us 4465 and three more pixls samples of different
     cameras; never `~/Desktop`). Build `Session::new().with_fs()` (`files.rs:604-612`) with a MemStore library,
     `library.import` the paths (mode `add`), assert each photo `develops_raw()`.
  2. Edits two photos as references: A = `develop.wb {mode:"tungsten"}` + exposure +0.4 + contrast 20 +
     grading shadows hue 220 sat 15; B = `develop.wb {mode:"custom", temp: as_shot*1.25}` + exposure −0.3 +
     saturation −15. `develop.matchLookReferences`, `develop.analyzeLook`.
  3. Targets: remaining raws, one with `crop.geometry.rect` = central 50 %, one with an existing Filled layer
     (`+10 contrast`), one with non-default raw options (whatever `RawOptions::of` reads, e.g. demosaic method).
     Run `develop.matchLook` with defaults, then again with `newLayer: true`, then once with `consistentBatch`.
  4. Assertions per target, measured on an independent **1024-px full-pipeline render** (`RenderRequest::fit
     (1024,1024)` from `s.source_now(id, SourceLevel::Preview)`), using `match_look::measure`/`distance`:
     `distance(after, goal) < 0.5 · distance(sync, goal)` where `sync` = the target rendered with reference A's
     settings copied; `|after.L50 − goal.L50| < 0.04`; `|ΔL5|, |ΔL95| < 0.08`; solved `wb.temp ∈ (2050, 48000)`,
     `|contrast| ≤ 60`, `|saturation| ≤ 55` (no rails); proxy-vs-full check: features measured at the solver's
     128-px proxy vs the 1024-px render differ by < 0.03 in L5/L50/L95 (N13) — print the deltas.
  5. Cross-domain: export reference A as JPEG (`export` command to a temp dir), import it, use it as the only
     reference, apply to a raw target: same thresholds. Mixed batch: one JPEG target in the raw batch, assert it
     got `wb.temp` in the JPEG (6500-relative) domain and the raws in Kelvin near `as_shot × ratio`.
  6. Timing: print `per_photo_ms` split into load/solve (`Matched.load_ms/solve_ms` already exist,
     `engine:329-331`) and assert `load_ms + solve_ms` per photo < 160 ms on the dev machine (record the number
     in the report; this is the input to P5).
  Always-on companion test (no files): a synthetic "raw-like" source with `SourceInfo { raw: true,
  raw_clip_level: Some(1.0), clip_confidence, camera_profile: None, look_curve: basecurves::camera(..) }` and
  20 % clipped highlights, asserting the proxy-vs-128-px feature deltas above and that `light.whites` moves
  negative when the goal L95 is lower.
- Fails before: the ignored test does not exist; the always-on test fails if proxy L95 differs > 0.03.
- Risk: medium (test only). If the thresholds fail on real raws, that is the finding the owner needs.

**P2 — One undo step per Strength drag (N1).**
- Evidence: `ui:473-479`, `engine:585-615`, convention `panels/edit.rs:24-36`.
- Change (engine): `amount()` must not commit while a drag is in progress. Add `p["interaction"]`:
  `"begin"` → `s.begin_interaction("Match Look amount")` and apply the develop change through the interaction
  (set develop without commit, as `develop.set` does during drags); `"end"` → `s.end_interaction()` (one
  commit, one history step). Without the flag (single click, reset buttons, headless) keep today's immediate
  commit. Update `record.last` on every call so the guard at `engine:588` keeps working mid-drag.
- Change (ui): use `out.drag_started` / `out.drag_stopped` from `widgets::slider` exactly like `edit.rs:24-36`
  (`app.ui.dragging_control` bookkeeping included), sending `{"strength":v,"interaction":"begin"|"end"}`.
- Test (engine): simulate begin / 5 values / end → `undo.len()` grows by 1 and the photo's history grows by 1;
  a plain `{"strength":40}` call still adds exactly 1. Test (ui, real clicks): `ui.dragWidget` on
  `slider:matchLook.photoStrength` with `steps: 6` → `undo.len() + 1`.
- Risk: low; follows the existing interaction API.

**P3 — Monochrome references and targets (N2).**
- Evidence: `pipeline:534-535, 1190-1196`; no treatment control; C1's Color Adjust includes B&W.
- Change (pipeline): in `solve_impl` after `blend`, compute `mono = goal.features[7] < 0.012 &&
  (17..25).all(|i| goal.support[i] < 0.05)`. If `mono && opts.color_adjust`: set `treatment = Treatment::Bw`
  on the edit target (global: `edits.treatment`; layer: `LayerTools.treatment = Some(Bw)` in `layer_settings`),
  drop `color.saturation`/`vibrance`/palette from `parameters()`/palette step, and (if `color_tone`) keep the
  grading wheels (B&W toning is legitimate). If `mono && !opts.color_adjust`: leave colour alone. Add
  `Solved.monochrome: bool`; `fade_controls`/`amount_settings` treat treatment as a non-faded switch (B&W at
  Strength 100 and 50 alike; `reset_group("color")` restores the previous treatment). If the **target** is
  already B&W (`base.treatment == Bw`), skip the colour groups entirely (their Jacobian columns are zero) and
  report it.
- Change (ui): status chip "Black & white look" when `mono`; the "What changed" row shows "Treatment Color →
  B&W".
- Test: reference = `source(1.0)` with `treatment = Bw`; target colour gradient; assert `settings.treatment ==
  Bw`, `settings.color == base.color` (no −55 saturation), after-error < 0.3 × before; layer variant asserts
  `masks.last().tools.treatment == Some(Bw)`; B&W target with colour reference asserts `settings.color ==
  base.color` and no NaN/odd grading. Fails before: saturation ends at −55 and treatment stays Color.
- Risk: medium (new switch in the fade/reset paths — keep it a bool, not a faded value).

**P4 — Dialog sizing and standard footer (N15); fix the design-system table (N23).**
- Evidence: `dialogs.rs:435-447, 564-570`; `DESIGN-SYSTEM.md:113, 154-162`.
- Change: remove `match_height` from `min_height`/`default_height`; keep only `max_height = (screen.height
  × 0.8)`; let `body` size itself (the `image_preview` placeholder already reserves `section_h × 4`). Delete
  the early-`return` footer; add `Dialog::MatchLook` to the normal path: `cancel = "Cancel"`, `ok = "Apply"`,
  and wrap the OK button in `ui.add_enabled(reason.is_none(), ..)` with `.on_disabled_hover_text(reason)` (the
  `ConfirmDelete` pattern at `dialogs.rs:603-607`); keep the automation ids `button:dialogCancel` /
  `button:dialogOk`. In `DESIGN-SYSTEM.md`, rewrite the nine rows as three-column rows and restore the
  `checkbox` row (`| checkbox | … | 705 |`), moving the `lc` variants into the Library table with their real
  signatures (`checkbox(ui, id, &mut on, label)` differs from `pc`'s).
- Test: existing click tests keep passing (ids unchanged); add an assertion in `tests` that the dialog rect
  height ≤ 0.8 × screen and < 700 px at 1200 px window height before a preview exists. A docs lint is not
  needed; reviewer checks the table renders.
- Risk: low.

**P5 — Adaptive parallel batch (N7), with the raw numbers from P1 as the gate.**
- Evidence: `engine:368-418` sequential; `tests:266` stubbed loader; owner: any hardware, 500 ≈ 80 s.
- Change (engine `SolveWork::run`): keep one `Op::Batch`; process inputs with `std::thread::scope` workers
  (not rayon: `heavy_jobs().admit` must stay off rayon, threading rule 5) pulling the next index from an
  `AtomicUsize`; worker count `n = clamp(available_parallelism / 2, 1, 8)` further limited by memory
  admission (each worker `admit`s its own permit; a worker that cannot get one exits). Each worker runs the
  unchanged per-photo load + `solve_with_faces` (deterministic per photo) and pushes `(index, result)` into a
  `Mutex<Vec<_>>`; after the scope, sort by index so `rows`/`applications`/`failed` order is unchanged.
  `completed` increments per photo; `stop()` is checked before each pull so cancel still ends the batch
  promptly. Preview (`preview == true`, one input) is unchanged. `set_current_thread_class(Utility)` in each
  worker.
- Test: engine test with a loader stub that sleeps 20 ms and records thread ids: 16 inputs finish in < 16×20
  ms on a machine with ≥ 4 CPUs (skip the timing assertion when `available_parallelism() < 2`), output order
  equals input order, `changed == 16`; determinism test (`pipeline:1400-1415`) still passes; the 500-photo
  ignored benchmark prints workers used. Re-run P1 timing and record load/solve per photo and the 500-equivalent.
- Risk: medium (new concurrency in one function). Keep the sequential path for `n == 1` to make the diff
  reviewable.

**P6 — Skip videos / non-library items instead of rejecting the batch (N3).**
- Evidence: `engine:230-234, 342`.
- Change: in `prepare`, `filter_map` the inputs, collecting rejects into `failed: Vec<Value>` (`[id, "video"]`),
  and return an error only if nothing remains ("No still photos in the selection"). `apply` already reports
  `failed`; the toast should say "N skipped (videos/offline)". Analyze keeps the error but names the file.
- Test: selection = 2 photos + 1 video (`MediaKind::Video`) → `changed == 2`, `failed.len() == 1`; no error.
  Fails before with "Choose still photos in the catalog".
- Risk: low.

### SHOULD

**P7 — Clear the applied record after Undo (N4).** In `panel_body` (`ui:464`) and `amount()` (`engine:588`):
if `develop_of(id) == record.before` → remove the record (or show "Match Look was undone") instead of the
"edited since" message. Test: apply, `edit.undo`, assert `applied` no longer contains the id and no
`slider:matchLook.photoStrength` widget is registered. Risk: low.

**P8 — Overflow menu for look management (N16).** Replace the "Look actions" `dropdown` with an `icon_button`
(`Icon::More` or the existing panel-menu icon) placed in the section header row (`section_header` has no
slot: draw it in the first `horizontal` row of `panel_body`, right-aligned); move Rename…, Duplicate, Delete,
Update from current references…, Import Look…, Export Look… into its `Popup::menu`. Keep Add selected /
Analyze / Save Look… / Match Look… visible. Test: existing ids keep working (`button:matchLookRename` etc. now
inside the popup — tests open the popup first, as `tests:441-447` does). Risk: low.

**P9 — Picker row height (N17).** `ui:301`: `show_rows(ui, 24.0 + ui.spacing().item_spacing.y, …)`, or drop
virtualisation. Test: with 60 saved looks, scroll the popup to the end and click the last row; assert the
selected id is the last look's. Risk: low.

**P10 — Skin protection without faces (N10).** `metric_weights`: skin weight 3.0 only when `faces` present
(pass a flag through `Options` or a `solve_impl` local), else 1.0; the hue penalty `pipeline:1014-1026` only
when `!faces.is_empty()`. Test: brick-coloured synthetic target, reference with a 60° warmer grading, no faces
→ grading hue reaches ≥ 40° of the goal; with a face rect over the same pixels → ≤ 20°. Risk: low.

**P11 — Check in the quality harness (N14).** Add `crates/lc-pipeline/tests/matchlook_quality.rs`,
`#[ignore]`, gated on `LOCAL_IMAGE_MATCHLOOK_JPEGS` (the six CC0 model-eval JPEGs) and
`LOCAL_IMAGE_TEST_RAW_DIR` (P1 raws); it writes `metrics.csv`/`summary.txt` under `target/` and asserts the
PLAN-v2 §7 gates (mean ΔE ≤ 3; Contrast ±100 in 0/12; no temp within 2 % of 2000/50000). Risk: none.

**P12 — Mid-batch cancel proof (N8).** Engine test with a 30 ms sleeping loader stub and 20 inputs: set
`cancel` after the 3rd `completed` tick (poll from the test thread), assert `rows.len() < 20` and that the
elapsed time is < 20×30 ms. Risk: none.

**P13 — Cancel a stale preview when the dialog reopens (N9).** `menus.rs:353-359`: call
`crate::match_look::cancel_preview(app)` before clearing `preview`. Test: start a preview with a slow stub,
close + reopen the dialog, click Preview → no "already running" error. Risk: low.

**P14 — Spelling (N18) and the two menu/label nits (N19, N20).** "Color only", "skin color"; export default
name = sanitised look name + `.json`; `ui_enabled("develop.analyzeLook") = !references.is_empty() &&
!busy`. Risk: none.

**P15 — Note the app-wide `text_button` eliding change in the report (N24)** and add one headless
screenshot of the Develop panel at 250 px to the ignored visual test, or revert the change to a Match Look-only
variant if any existing button now truncates. Risk: low.

### COULD

**P16** Debounced auto-preview on option change (first-review B8 remainder): when a preview exists for the
active photo and an option changes, re-run the preview job automatically after 300 ms idle; Apply never waits
for it. **P17** Per-feature MAD floors and a ≥ 4-sample rule in `weighted_median` (N11), with a test of the
7+3 cluster case. **P18** Float (`deep`) probe measurement (N12). **P19** Thumb-sized admission bytes
(N5). **P20** Field-wise `read_match_look_prefs` (N6). **P21** Larger reference tiles (N21) and pick which
reference the wipe compares (N22). **P22** `tr_format!` for the concatenated status strings (`ui:360-362,
375-377, 616, 625`) — translations are last per the owner, but the strings are being added now. **P23**
Hover-reveal remove buttons on the tray (LR/C1 convention) instead of a permanent × under every tile.
**P24** Dedupe on import when a look with identical statistics already exists (name + "(imported)" suffix
otherwise).

**P6b (MUST) — `close_library` must not abort on a Match Look amount-file write error (data safety).**
- Problem: this feature added `self.save_match_look_applied()?;` before `persist()` in `close_library`
  (`library.rs:594`, in the diff). If writing `match-look-applied.json` fails (disk full, read-only, blocked
  store), the `?` returns before the journal `persist()` and the closing snapshot (`library.rs:595-608`), so
  queued catalog ops can be lost over an optional amount-history file.
- Change: `if let Err(e) = self.save_match_look_applied() { log::warn!(..); lib.warnings.push(..) }` and
  continue; the amount history is best-effort (the report already says so).
- Test: `MemStore` wrapper whose `write_atomic("match-look-applied.json")` errors; apply a look, add one
  more edit, `close_library()` → `Ok`, and reopening shows the last edit. Fails before (close returns `Err`
  and the edit is not in the snapshot).
- Risk: none.

### Outside Match Look (flag only, no action in this job)

- `widgets::text_button_enabled` eliding change affects every Library text button (N24/P15).
- The smart-preview `raw_key: 0` limitation (`smart.rs:241`) applies to Develop as well; not a Match Look issue.
