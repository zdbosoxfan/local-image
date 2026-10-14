# REVIEW: Match Look (Fable)

Independent review of the uncommitted Match Look feature in worktree `cx-matchlook`, 2026-10-13.
Line numbers refer to the uncommitted working tree. No source was edited; the evaluation harness
and its outputs live under `target/matchlook-review/` (see C).

Files read: `crates/lc-pipeline/src/match_look.rs`, `crates/lc-engine/src/cmd/match_look.rs`,
`crates/lc-ui-egui/src/match_look.rs`, the diffs to `auto.rs`, `menubar.rs`, `menus.rs`,
`panels/{dialogs,edit,grid}.rs`, `widgets.rs`, `state.rs`, `lib.rs`, `library.rs`, plus
`media.rs` (source levels), `primary.rs` (layer WB), `local.rs` (`effective_wb`),
`settings.rs` (WB modes), `controls.rs` (clamping), `tasks.rs` (spawn), `tests_match_look.rs`.

## A. Owner requirements

| # | Requirement | Verdict | Evidence |
|---|---|---|---|
| 1a | Up to 10 references | MET | `pipeline/match_look.rs:13` `MAX_REFERENCES=10`; `engine/cmd/match_look.rs:358` rejects >10; UI `ui/match_look.rs:264` disables "Add selected" at 10 |
| 1b | Analyze step: background, progress, Cancel, "Analyzing…", summary, Re-analyze | MET | worker `ui/match_look.rs:69-103` via `tasks::spawn`; progress+Cancel window `:196-212`; "Analyzing…" `:290-291`; "Analyzed: N references" + chips `:296-315`; "Re-analyze" label `:283` |
| 1c | Stale references show "Analyze again" and disable Apply | MET | `engine:134-146` `current_ready` compares settings snapshot + content key; UI `:298-299` "References changed — Analyze again"; "Match Look…" disabled `:327-334`; menu item disabled `menus.rs:1451` |
| 2 | Matches the result, not copied sliders; per-target editable sliders | MET (quality caveat C) | `solve()` `pipeline:307-488` never reads reference settings; `Look` holds only statistics (`:57-61`); output is `DevelopSettings` committed through `develop_op` (`engine:333`). Caveat: solved values often sit at slider rails (B5/B7) |
| 3 | Similarity weighting + Consistent batch | MET | `weights()` `pipeline:160-177` (scene signature on unedited render, `exp(-8 d)`); `consistent_batch` -> equal weights; dialog checkbox `ui:412` |
| 4a | Four C1 groups as toggles, Strength, preview | MET | `Options` `pipeline:30-38`; checkboxes `ui:392-395`; Strength slider `:396-410`; preview `:417-423` (active photo, 400 px) |
| 4b | One undo step | MET | `Op::Batch` `engine:337`; test `engine:521-539` |
| 4c | Apply as new layer, stacking on finished result below | MET (B4 caveat) | `layer_settings` `pipeline:279-296` appends a Full mask; solve runs through the full stacked render; test `:575-590` |
| 5a | Saved looks: save/rename/delete/duplicate/update | MET | `engine:371-438`; UI row `ui:341-364` |
| 5b | Import/export JSON | PARTLY | engine actions `export`/`import` only (`engine:382-407`); no menu item, no file picker; builder's report admits this |
| 5c | Global, independent of catalog/photo IDs | MET | `Presets/match-looks.json` under `app_dirs::config_dir()` (`ui:45-57`); records contain no IDs (`engine:44-52`, test `:579-580`). Note the **selected** id is stored per catalog in `prefs.json` (B3) |
| 5d | Survives removal of references | MET | test `engine:563-583` |
| 5e | No cap on number of looks; per-file safety bounds on import | PARTLY | no count cap (`validate_file` `engine:111-129` only bounds refs/look at 10). But there are **no** per-file bounds either: unlimited look count, name length, `created` length, no byte limit; `import` takes an already-parsed JSON Value, so a 500 MB file would be parsed in full before validation |
| 6 | Sync vs Match Look explained in the UI | MET | Sync button tooltip `edit.rs:495`; Sync dialog line `dialogs.rs:485`; Match Look tooltip `ui:335` and dialog line `:384-390` |
| 7a | Pure Rust, CPU, no ONNX, Mac | MET | no `Cargo.lock` change; test `engine:643-652`; no cfg(target_os) code; `web_time::Instant` |
| 7b | Threading policy | MET | dedicated std thread (`tasks.rs:61`), `ThreadClass::Utility` (`engine:205,280`), `heavy_jobs().admit` outside rayon (`:209,297`), cancel checked before every render, determinism test `pipeline:609-624`. Nit: admission reserves `w*h*32+16 MiB` of the *full* image although only the 512 px thumb is decoded (`engine:175`) |
| 7c | Design system: shared widgets, no one-off UI | PARTLY | five new widgets in `widgets.rs` (checkbox, text_action, reference_tray, summary_chips, image_preview, text_button_translated) with tokens, but `docs/DESIGN-SYSTEM.md` was **not** updated (grep finds none of them). Progress window re-implements the cull pattern (`ui:204-210`) rather than sharing it |
| 8 | Performance (500 photos 78 s accepted) | MET | builder's benchmark on 512 px synthetic proxies; my harness: 118-133 ms/solve on 768 px JPEGs, 62-66 renders each (dev profile, opt-level 1) |

## B. Bugs and broken behaviour

### B1. Normalize starts from a stale WB value for raw photos with a WB preset (Auto/Daylight/Tungsten…)
- Severity: medium (raw files only).
- `pipeline/match_look.rs:345-350` only replaces `temp/tint` with the effective WB when the mode is
  `AsShot`. For raw sources, presets resolve via `WbMode::preset()` (`local.rs:117-124`), but
  `s.wb.temp` still holds the stored value (default 6500, `settings.rs:287`). The first `wb.temp`
  probe (`:366`, mode flipped to Custom at `:241`) renders at `stale+400`, not `effective+400`, so
  the finite difference includes the preset->stale jump; and the Strength fade (`:462-465`) blends
  between the stale temp and the solved temp.
- Repro (raw): set a target to the Tungsten preset, Match Look with Normalize on, Strength 50;
  the result's temp lands near (6500 + solved)/2 rather than near 3200K.
- Fix: `if d.wb.mode != WbMode::Custom { d.wb.temp = wb.0; d.wb.tint = wb.1; }`.
- (My harness probe used a JPEG source, where presets are identity, so it could not show it.)

### B2. "Match Look applied: null" and a whole batch discarded after any edit during the run
- Severity: medium for the 500-photo case.
- `engine/cmd/match_look.rs:324-328` discards the entire batch if *any* target's settings changed
  while the worker ran (the dialog closes and the app is fully interactive during the 78 s run).
  `ui/match_look.rs:155-158` then toasts `"Match Look applied: " + value["changed"]`, i.e. `null`.
- Repro: Apply to 200 photos, nudge one slider on one selected photo during the progress window.
- Fix: skip only changed rows (`filter(|(i,_)| unchanged(s,i))`), return `skipped`, toast it.

### B3. Dangling `selected` saved-look id (per-catalog pref pointing at global data)
- Severity: low-medium.
- `match_look.selected` lives in catalog `prefs.json` (`library.rs:169,386,604`), the look in the
  global file. Delete the look from another catalog, or have the global file fail to open
  (`saved_looks.error`), and `selected_looks()` returns `None` (`engine:148-151`) while `selected`
  stays `Some`: the Rename/Duplicate/Delete/Update row is drawn (`ui:341-364`) for a look that does
  not exist, the picker says "Current analysis", the status may say "Analyzed: N references"
  (`:296-297`) yet "Match Look…" is disabled with "Analyze references or choose a saved look first".
- Fix: clear `selected` on library open / when it does not resolve; draw the management row only
  when the look resolves.

### B4. New-layer mode always writes an absolute WB into the layer
- Severity: medium (surprising later edits).
- `pipeline:286` `wb: opts.normalize.then_some(edits.wb)` with `edits.wb = {Custom, effective}`
  (`:343`). Layer WB is absolute (`primary.rs:388-390`: `wb_change(global effective -> layer temp)`),
  so after a layer apply any later move of the *global* Temp slider is cancelled by the layer.
- Repro: Apply as new layer (Normalize on); then move global Temp: nothing changes.
- Fix: only emit `tools.wb` when the solved WB differs from the start, or encode the change as the
  relative `mask.adjust.temp/tint`.

### B5. Solve range is hard-capped: ±2 EV exposure, ±3200 K temp, ±120 contrast per Apply
- Severity: medium (quality; verified in C).
- `:257` step 0.25 EV, `:387` `dx` clamped to ±4h = ±1 EV, two sweeps (`:357`). Temp step 400 ->
  ±1600 K per sweep -> 6500+3200 = **9700 K**, which is exactly where 5 of 12 harness solves ended.
  The two −3 EV targets ended at exposure **+2.00** with L50 still 6–9 points too dark.
- Fix: first-sweep step sized from the measured L-median / neutral a-b gap (log-domain for
  exposure), or allow extra sweeps while the last sweep still improved by >1e-3.

### B6. Auto tone recipe replaced as a side effect (scope)
- Severity: low. `auto.rs` now returns a fixed recipe (Contrast 68, Highlights −66, Shadows +52,
  Vibrance 30) and `dark_image_gets_positive_exposure` was loosened from >1.5 to >0.5 EV. Match Look
  uses only `current_wb`. The owner should decide the Auto-button change separately.

### B7. Solved sliders land on the rails (no regularisation)
- Severity: medium-high for the owner's core promise ("editable sliders a user can continue from").
- Harness (C): Contrast ended at **±100** in 8 of 12 solves, Saturation at −77/−49/−48/−46,
  Tint at −47/−39, Temp at the 9700 cap. The statistical match is good, but a Lightroom/Capture One
  user opening the Basic panel afterwards sees absurd values. Cause: pure statistical objective with
  25 features, coordinate descent with `dx` up to ±60 contrast per step and no cost on parameter
  magnitude, so contrast/exposure/saturation trade against each other freely.
- Fix: add a small quadratic penalty on each parameter's normalised change (λ·Σ(Δp/range)²) to the
  objective, cap per-group ranges (e.g. |contrast| ≤ 60, |saturation| ≤ 40 unless Strength demands),
  and prefer Whites/Blacks before Contrast for end-point errors.

### B8. Dialog cannot Apply without a manual Preview; Preview is invalidated by any option change
- Severity: low-medium (UX). `dialogs.rs:565-567` disables Apply until `preview_ready`; `ui:414-416`
  clears the preview on every checkbox/Strength change, so each tweak needs another Preview click.
  Capture One previews live and applies directly. Fix: auto-preview (debounced) on change, and allow
  Apply without a preview.

### B9. Hardening: analysis snapshots embedded in catalog `prefs.json`
- Severity: low. `State.analysis` stores up to 10 full `DevelopSettings` snapshots inside
  `prefs.json`. `settings.read::<PrefsFile>().unwrap_or_default()` (`library.rs:385`) means any
  deserialisation failure of that sub-tree silently drops *all* prefs (XMP, Smart Sort, last export).
  A non-finite f64 serialises as `null` and fails to deserialise. Fix: deserialise `match_look` through
  `serde_json::Value`/`deserialize_with` so a bad snapshot only drops the analysis.

### B10. Menus
- Low. A new top-level **Develop** menu (`menubar.rs:56,62`) holds only the three Match Look items,
  which are *also* in Edit (`:107-108`) and Photo (`:198-199`). Shortcuts are unassigned in both
  profiles (fine). Suggest: drop the new top-level menu, keep Photo (next to Sync Settings) and the
  context menu.

Checked and OK: NaN/validity gates (`pipeline:315-323`, `Look::valid`), cancel before every render,
cancelled worker commits nothing (`engine:541-561`), import assigns fresh UUIDs, version check on
import/open, corrupt global file preserved, undo grouping, thread-count determinism, layer id
allocation, `controls::set` clamping, Thumb source is a real decode (`media.rs:435-470`), library
generation guard in `progress()` (`ui:196-202`), `Runtime::drop` cancels.

## C. Matching quality (measured)

Harness: `target/matchlook-review/zz_matchlook_review.rs` (was run as
`crates/lc-pipeline/tests/zz_matchlook_review.rs`, then moved out of the source tree; copy it back
to rerun with `cargo test --offline -p lightcraft-pipeline --test zz_matchlook_review -- --ignored --nocapture`).
Outputs: `target/matchlook-review/metrics.csv`, `summary.txt`, `sheets/{warm,cool,airy}-{similarity,consistent}.png`
(columns: reference | target before | after Match Look | after Sync).

Setup: the 6 CC0 model-eval JPEGs (768 px, sRGB -> linear Rec.2020), `SourceInfo::default()`.
Three looks with distinct Develop settings: **warm** (7600 K, +0.25 EV, contrast 18, warm grading),
**cool** (5000 K, −0.45 EV, contrast 28, blacks −18, sat −18, blue grading), **airy** (6900 K,
+0.8 EV, contrast −18, highlights −35, shadows +35). References = portrait + street. Targets =
branches, glass, pet, product, each perturbed in the *source* to simulate a different capture:
(−1.2 EV, warm cast), (+0.7 EV, cool cast), (−3.0 EV), (−0.5 EV, strong cool cast). Renders at 512 px.
Metric: distribution ΔE = sqrt(ΔL50² + Δa² + Δb²) in Oklab×100 between the target render and the
mean of the two reference renders; plus L 5/50/95 percentiles, mean a/b, mean chroma, CCT of the
image mean, and the feature's own `distance()`.

| phase (12 target×look pairs) | mean ΔE | mean abs ΔL5 | ΔL50 | ΔL95 | Δab | Δchroma | ΔCCT (K) | feature dist |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| before | 17.2 | 15.3 | 16.5 | 20.2 | 3.61 | 1.28 | 2764 | 0.228 |
| after **Sync** (copied sliders) | 17.0 | 15.2 | 16.6 | 18.2 | 3.21 | 1.53 | 4488 | 0.202 |
| after **Match Look** | **3.1** | 8.2 | **2.8** | 12.2 | **0.82** | 0.50 | **556** | 0.038 |

Within ±2 EV (9 pairs): ΔE 15.5 -> **2.3** (Sync 15.8); |ΔL50| 14.6 -> 2.0; |ΔCCT| 3172 -> 501 K.
Per look: cool 16.6 -> 0.9, warm 17.4 -> 3.4, airy 17.6 -> 4.9. Consistent batch: 3.2 (same as
similarity with only two references). The −3 EV targets: 22.7 -> 6.5 and 29.4 -> 8.7 with exposure
pinned at +2.00 (B5). One regression: airy/branches 3.0 -> 3.5 (contrast driven to +100, B7).
Speed: 118–133 ms per target, 62–66 proxy renders.

Visual check of the sheets: the Match Look column moves every target clearly toward the reference
tone and temperature (warm glass: blue cast -> warm neutral; cool set: uniformly blue/dense; airy
pet: lifted), while the Sync column keeps the wrong exposure and cast. Weak spots: the −3 EV pet
stays dark; highlight/shadow end-points (ΔL5 8.2, ΔL95 12.2) are matched much worse than the
median, which is where the extreme Contrast values come from; grading is barely transferred
(the "cool" blue shadows come mostly from WB/saturation, not from the grading wheels).

## D. UX (Lightroom / Capture One user view)

- Natural: panel under Sync in Develop, context-menu "Set as Match Look reference", C1 group names,
  Strength, "Apply as new layer", the Sync vs Match Look sentence in both dialogs.
- Confusing: "Set as Match Look reference" (menu) **adds** to the tray (`engine:348-357`), while
  "Add selected" (panel) does the same thing: two labels, one action, and "Set" reads as replace.
  Summary chips "Tone 48 / Warmth +34 / Saturation 80" are unitless internal numbers. "Current
  analysis" as a picker entry is engineering vocabulary ("Analyzed references" would do). "Update
  look" silently replaces a saved look with the *current* tray's analysis (`engine:231-237`); call it
  "Update from current references…" and confirm. "Normalize" needs a tooltip for Lightroom users.
  Apply requires a manual Preview (B8). Dialog options reset to defaults each time it opens
  (`menus.rs:969`). The new top-level Develop menu with three entries (B10).

## Prioritised fix list

Blocking before merge:
1. B7 regularise the solve so sliders stay plausible (and B5 range cap, same code).
2. B1 WB-preset start value (raw files).
3. B2 per-row stale skipping + correct toast.
4. B4 relative (or conditional) layer WB.
5. 5e per-file import bounds (count, name length, byte size) and 7c document the new widgets in
   `docs/DESIGN-SYSTEM.md`.

Follow-up:
6. B3 dangling selected id; B9 prefs hardening; B8 live preview / Apply without preview;
   B10 menu placement; label wording in D; import/export UI (5b); decide the Auto-tone port (B6)
   separately; share the progress window with the cull one.

## Verdict on matching quality (plain English)

It works, and it is clearly not Sync: on 12 deliberately mismatched targets the distance to the
reference look (median lightness + colour, Oklab×100) fell from 17.2 to 3.1, while copying sliders
left it at 17.0; the colour temperature error fell from ~2800 K to ~560 K and the median
lightness error from 16.5 to 2.8 points. Inside ±2 EV it reaches ΔE 2.3. It cannot yet bridge
more than about 2 EV or 3200 K in one go, highlight/shadow end-points are matched only roughly,
and the slider values it produces are often extreme (Contrast ±100, Saturation −50..−77), which
undermines the "editable starting point" promise until the solver is regularised.
