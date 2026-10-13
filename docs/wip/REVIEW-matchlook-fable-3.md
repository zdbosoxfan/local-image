# REVIEW 3: Match Look (Fable)

Third independent review of branch `wip/cx-matchlook` at `96fba0c7` (worktree `cx-matchlook`), 2026-10-13.
Read-only: no source was edited; the existing `#[ignore]` real-raw tests were re-run for numbers where noted.
Inputs: `REVIEW-matchlook-fable-2.md` (plan P1–P24 + P6b), the revision-2 diff `4df08550..96fba0c7`
(26 files), Codex's logs under `target/matchlook-revision2/`, the CC0 raw set + manifest under
`target/matchlook-raws/`, and the neighbouring raw/media/pipeline code.

Abbreviations as in review 2: `pipeline` = `crates/lc-pipeline/src/match_look.rs`,
`engine` = `crates/lc-engine/src/cmd/match_look.rs`, `ui` = `crates/lc-ui-egui/src/match_look.rs`,
`tests` = `crates/lc-ui-egui/src/tests_match_look.rs`, `rawtest` = `crates/lc-engine/src/tests_match_look_raw.rs`,
`quality` = `crates/lc-pipeline/tests/matchlook_quality.rs`. Line numbers refer to `96fba0c7`.

## 1. Verdict

Revision 2 did what the plan asked: all 25 review-2 items are in code (23 done, 2 partial: the always-on raw proxy
test proves little, and the eliding note/screenshots were not produced). Pipeline (27) and engine (16) Match Look
unit tests pass at `96fba0c7`; the parallel batch is correct (deterministic, index-ordered, one undo, admission per
photo, cancel within one photo) and meets the owner's time (500 real raws in 73 s on this machine, 2 ms UI frames,
1.9 GiB). **It cannot be merged yet** for three reasons:

1. **The UI test target does not compile** (`widgets.rs:762, :766`), so none of the 10 click tests run, and the last
   UI run that did run had one failure (`tests_match_look.rs:400`). R0.
2. **The two raw failures are real and both must be fixed as the owner asked.** Nikon is a proxy-fidelity defect: the
   solver measures every statistic on a 128-px box-filtered proxy, which erases thin dark structure (the bench's
   ironwork) and raises L5 by 0.03 relative to what the user sees; the reference statistics carry the same bias
   (Sony foliage: L5 +0.09, L95 −0.09 at 40 px vs 320 px). Pentax is an unreachable goal: a flat carpet (L5–L95 span
   0.14) cannot take a 0.42–0.50 reference span inside the designed slope cone, so the solver correctly stops 60 %
   short and the test gate is wrong for that scene — but the product has no way to say so. R1 and R2 fix both
   without loosening a threshold (R3 fixes the test's own cache-dependent proxy metric and its machine-specific
   160 ms gate).
3. **Result quality is "statistically matched, photographically flat".** The contact sheets show every target pulled
   to the references' absolute median and mid-grey; high-key frames go dark, low-key frames go flat, and dissimilar
   scenes get 20 000 K white balances. These are exactly Capture One's known complaints, and the per-photo
   adaptivity that Lightroom/Imagen/Aftershoot sell. R4 (scene-adaptive brightness), R8 (WB shift bound) and R6/R11
   (black/white points, local contrast, per-band hue) are what would make this best-in-class; R5 (keep decoded
   sources) removes 65 % of the re-apply cost.

CR3 via the embedded preview is acceptable for Match Look (consistent with Develop) provided the batch summary says
so (R10); the decoder is an engine item.

Plan: **6 MUST (R0–R5), 7 SHOULD (R6–R12), 7 COULD (R13–R19)**; details in section 6.


## 2. Review-2 items status (P1–P24, P6b)

Verified in code at `96fba0c7`, not from Codex's notes. Unit suites I ran: `lightcraft-pipeline match_look` 27/27 pass,
`lightcraft-engine match_look` 16/16 pass (3 ignored: 2 GPU, 1 real-raw). **The `lightcraft-ui-egui` test target does
not compile** (N1 below), so no UI click test can be run at HEAD; the UI entries below are verified by reading.

| Item | Verdict | Evidence |
|---|---|---|
| P1 real-raw end-to-end test | **Done; gates fail on Pentax + Nikon (section 3)** | `rawtest:54-259`: 6 CC0 raws (manifest with SHA-256), refs A/B as specified, crop / existing layer / non-default demosaic targets, 4 modes, 1024-px full render metrics, JPEG-reference cross-domain (`:199-233`), mixed raw+JPEG batch (`:234-248`), timing. The always-on companion (`pipeline:1476-1510`) exists but compares a 128-px render of the 512-px source with a 128-px render of the 128-px proxy — both outputs are 128 px, so it does not prove proxy-vs-large fidelity (**partial**, N8). |
| P2 one undo per Strength drag | **Done** | engine `amount` honours `interaction: begin/end` (`engine:757-765`); UI uses `panels::edit::apply_slider_out` (`ui:470-472` → `edit.rs:18-40`, `develop.beginInteraction`/`endInteraction`); tests `engine:1195-1216` (6 values → +1 undo, +1 history), `tests:528-532` (`ui.dragWidget` steps 6 → +1). |
| P3 monochrome references/targets | **Done** | `is_monochrome` (`pipeline:898-900`), solver drops Color Adjust for mono/B&W targets (`pipeline:1000-1005`), `edits.treatment = Bw` (`:1021-1023`), layer `tools.treatment` (`:717`), `fade_controls` treats treatment as a switch (`:745-752`), `reset_group("color")` restores it (`:872, :889`); UI chips + "Treatment Color → B&W" row (`ui:406-408, 465-467, 511-513, 782-784`); tests `pipeline:1358-1406`, GPU test (`engine:1423`, passed in `gpu.log`). Gap: toned/split-toned B&W (N5). |
| P4 dialog sizing + standard footer; design-system table | **Done** | `dialogs.rs:127-151` (`auto_sized`, `max_height = 0.8·screen`, `min_height 0`), early-return footer deleted, standard row with `ui.add_enabled` + `on_disabled_hover_text` and "Apply" label (`dialogs.rs:616, 633-638`); `DESIGN-SYSTEM.md:113, 155-163` three-column rows, `checkbox` row restored. Test `tests:490-497` (height < 700 at 1200). |
| P5 adaptive parallel batch | **Done** | `engine:379-461`: scoped std threads (not rayon), `workers = min(len, profile().threads, headroom/bytes)`, per-photo `admit`, index-sorted merge, cancel checked per pull; solve inside `in_background_pool` (`:481-483`); `preview` stays sequential. Tests `engine:1231-1319` (thread ids, order, elapsed < load+solve, cancel after 3). App benchmark: 500 real raws 73.4 s, 7 workers, 1.9 GiB RSS, 2 ms frame p95 (`raw-app-500.log`). Serial vs parallel results identical in my two runs. |
| P6 skip videos / non-library | **Done** | `engine:355-369` (`filter_map`, `failed`, "No still photos" only when empty); toast `ui:231-234`; test `engine:1238-1245`. |
| P6b close_library survives amount-file error | **Done** | `library.rs:599-604` (warn + `settings_warnings`); test `engine:1321-1358` with a failing store. |
| P7 clear applied record after Undo | **Done** | `reconcile_applied` (`engine:712-726`) from `edit.undo` (`edit.rs:7-11`), the panel (`ui:459`) and `amount` (`engine:735`); tests `engine:1217-1220`, `tests:533-536`. Redo does not restore the record (N12, L). |
| P8 overflow menu | **Done** | `icon_button(… Icon::More …)` beside the picker (`ui:343-344`), `look_menu` (`ui:790-837`) with Rename/Duplicate/Delete/Update/Import/Export; test `tests:506-511`. Style nit N11. |
| P9 picker row height | **Done** | `TEXT_BUTTON_H` (`widgets.rs:421`) used by `show_rows` (`ui:329`); test with 60 looks scrolling to the last (`tests:514-521`). |
| P10 skin without faces | **Done (different mechanism, acceptable)** | confidence-scaled skin observation when no faces (`pipeline:1012-1019`), skin weight ×3 and hue penalty only with cached faces (`:1006-1010, :1036-1047`); test `pipeline:1441-1474` (no faces ≥ 40° of a 60° shift, faces ≤ 20°). |
| P11 quality harness in repo | **Done** | `quality:1-191`, `#[ignore]`, `LOCAL_IMAGE_MATCHLOOK_JPEGS`, writes `target/matchlook-quality/{metrics.csv,summary.txt}`, asserts mean ΔE ≤ 3 / rails 0 / no temp rails. Measured: 24 samples, ΔE 2.46 (`jpeg-quality.log`). |
| P12 mid-batch cancel proof | **Done** | `engine:1290-1318` (30 ms loader, cancel after 3 completed, rows < 20, elapsed bound). |
| P13 cancel stale preview on reopen | **Done** | `menus.rs:971`; plus `Arc::ptr_eq` guards so a stale callback cannot clear a new job (`ui:102-104, 153-155`); test `tests:566-581`. |
| P14 spelling, export name, Analyze gating | **Done** | "Color only" (`ui:542`), tooltip (`ui:530`); `export_name` (`engine:727-733`, `menus.rs:1276`); `ui_enabled("develop.analyzeLook")` (`menus.rs:1489`); tests `engine:1222`, `tests:488`. |
| P15 note the app-wide text-button eliding; Develop panel screenshot | **Partial** | the ignored screenshot test now saves `develop-{tag}` at 250/400 px (tests diff), but Codex wrote no report note and the screenshots were not regenerated (no `target/matchlook-ux/develop-*.png`). |
| P16 debounced auto-preview | **Done** | `ui:552-569` (300 ms after an option change, only when a preview existed/was running), reset-group path too (`ui:624-626`); test `tests:546-565`. |
| P17 per-feature MAD floors, ≥ 4 samples | **Done** | `weighted_median_floor` (`pipeline:285-312`), floors 0.03 for L quantiles/skin L and 0.005 otherwise (`:474`), `v.len() < 4` early return (`:306`); test `pipeline:1408-1430`. |
| P18 float probe measurement | **Done** | `OutputDepth::F32Linear` requests, `measure_rendered`/`measure_deep_impl` (`pipeline:139-178, 437-441, 973-985`); test `:1431-1438`. |
| P19 Thumb-sized admission bytes | **Done** | `engine:237-246` (`min(512)` thumb working set + the raw decode's sensor×6 + file size, released by `finish_heavy` after decode, `:471-476`); test `engine:1246`. |
| P20 field-wise prefs decode | **Done** | `library.rs:175-180`. |
| P21 larger tiles, choose the compared reference | **Done** | `reference_tile_edge = thumb_bucket(2·slider_row_h)` (`widgets.rs:913-915`), HiDPI thumb bucket requests (`ui:347-351`), "Next reference" cycling (`ui:587-600`, `reference_index`). |
| P22 `tr_format!` strings | **Done** | ui diff; strings registered in `i18n.rs:335-358` and the 13 `*-formats.json` files. |
| P23 hover-reveal remove on tray | **Done** | `hover_icon_button` (`widgets.rs:500-503, 530-560`), tray overlay (`:941-947`), keyboard focus still reveals; the widget test for it is what breaks the UI test build (N1). |
| P24 import dedupe | **Done** | identical statistics skipped, same name → "(imported)" suffix (`engine:684-693`); test `engine:1223-1230`. |

Not in the plan but delivered: the raw-aware admission bytes, `reconcile_applied` on every panel frame, "Black & white target / Color groups skipped" chips, `Apply` enabled while a preview runs (Apply cancels it, `ui:132-134`, `dialogs.rs:633`).


## 3. Pentax + Nikon root causes (and CR3)

### 3.1 What I ran

`LOCAL_IMAGE_TEST_RAW_DIR=<worktree>/target/matchlook-raws LOCAL_IMAGE_MATCHLOOK_SERIAL=1 cargo test --offline
-p lightcraft-engine match_look_real_raw_end_to_end -- --ignored --nocapture` at `96fba0c7` (log in my scratchpad,
46 s; Codex's logs backed up before the run because the test overwrites `raw-serial-gates.txt`). The six sensor raws
decode (`RAW_FIXTURE … sensor_decode=true relative_wb=false clip=true look_curve=true` for all six); CR3 is
`preview_only` ("Cr3 files are not decoded yet") and is excluded by the test's `develops_raw()` filter
(`rawtest:39-46`). Test roles (sorted by file name, `rawtest:21`): ids[0] = Canon 7D CR2 = reference A (Tungsten
preset, +0.4 EV, contrast 20, shadow grade 220°/15), ids[1] = Sony ILCE-1 ARW = reference B (as-shot ×1.25 K,
−0.3 EV, saturation −15); targets: id 4 Panasonic GF6 RW2 (cropped 50 %), id 5 Nikon D3200 NEF (existing Filled
layer, contrast +10), id 6 Fujifilm X-E2S RAF (Bilinear demosaic), id 7 Pentax K-7 DNG (plain).

Numbers at HEAD (my run; Codex's logs are from an earlier state of the test — they lack the `RAW_FIXTURE` lines and
show `loadMs` > 0 on every batch, so they are not byte-comparable; the `error`, `sync`, `L5/L50/L95` columns agree
to six digits, the `proxy` column does not — see 3.4):

| target | mode | error | sync | L5 | L50 | L95 | proxy ΔL5 | ΔL50 | ΔL95 | temp |
|---|---|---|---|---|---|---|---|---|---|---|
| Nikon (5) | global | 0.0049 | 0.217 | 0.026 | 0.001 | 0.003 | **0.0298** | 0.0002 | 0.0022 | 6150 |
| Nikon (5) | global, consistent | 0.0058 | 0.187 | 0.028 | 0.001 | 0.008 | **0.0299** | 0.0002 | 0.0025 | 6553 |
| Nikon (5) | layer | 0.0046 | 0.217 | 0.028 | 0.000 | 0.001 | **0.0302** | 0.0002 | 0.0036 | 5844 |
| Nikon (5) | layer, consistent | 0.0094 | 0.187 | 0.028 | 0.003 | 0.003 | **0.0303** | 0.0005 | 0.0030 | 5844 |
| Pentax (7) | global | **0.176** | 0.301 | **0.129** | 0.013 | **0.147** | 0.0080 | 0.0001 | 0.0061 | 2988 |
| Pentax (7) | global, consistent | **0.212** | 0.210 | **0.108** | 0.017 | **0.189** | 0.0078 | 0.0001 | 0.0082 | 2672 |
| Pentax (7) | layer | **0.162** | 0.301 | **0.118** | 0.016 | **0.146** | 0.0072 | 0.0008 | 0.0061 | 2988 |
| Pentax (7) | layer, consistent | **0.192** | 0.210 | **0.098** | 0.017 | **0.180** | 0.0073 | 0.0001 | 0.0106 | 2988 |
| Panasonic (4) | global | 0.0050 | 0.327 | 0.022 | 0.005 | 0.015 | 0.0210 | 0.0050 | 0.0173 | 3276 |
| Fuji (6) | global | 0.0124 | 0.334 | 0.013 | 0.001 | 0.001 | 0.0105 | 0.0016 | 0.0002 | 22577 |

(Gates, `rawtest:176-180`: `error < 0.5·sync`, `|ΔL50| < 0.04`, `|ΔL5|,|ΔL95| < 0.08`, every proxy delta `< 0.03`,
temp in (2050, 48000).) The only failures are Nikon's proxy ΔL5 (0.0298–0.0303, on the 0.03 line) and Pentax's
`error`/`L5`/`L95`. Both are reproducible and deterministic across my run and Codex's.

### 3.2 Pentax K-7 DNG: unreachable tonal goal, not a decode or solver bug

The Pentax frame is a flat close-up of a beige carpet (`target/matchlook-revision2/raw-native-contact-sheet.png`,
bottom-right tile). I measured the native tile's Oklab-L quantiles (scratch Python over the sheet PNG, sRGB →
linear → Oklab): **L5 = 0.424, L25 = 0.484, L50 = 0.511, L75 = 0.534, L95 = 0.562** — a total L5–L95 span of
0.14. The two references (a tungsten-lit desk at +0.4 EV / contrast 20 and a sunlit foliage-and-water frame) have
spans of ≈ 0.42 (Canon tile: 0.223–0.644) and ≈ 0.50 (Sony: 0.383–0.882); the weighted-median blend of the goal is
in that range. Reaching it needs a ≥ 3× expansion of the carpet's tonal range.

The solver cannot and should not do that: `cdf_curve` clamps the equal-CDF knots to a slope of [0.5, 2.0] outward
from the median (`pipeline:328-345`), `guard_curve` enforces the same cone on the editable curve
(`pipeline:682-701`), Contrast is bounded at ±60 (`pipeline:571`), the curve regions at ±55 (`pipeline:574`), and
the Huber loss (`pipeline:1021-1024`, scale = 2·spread = 0.12 for L quantiles) turns the huge L5/L95 residuals into
linear terms, so the prior (`pipeline:1030-1035`) keeps the sliders modest (Contrast 9, no curve rails). The result
is exactly the designed "bounded, no-rails" behaviour; `L50` is met (0.013–0.017) and WB/exposure move as they
should. `sync` is barely better (0.21–0.30) because copying reference A's Contrast 20 cannot stretch the carpet
either. Decode is fine: `sensor_decode=true`, clip plane and look curve present, as-shot 2988 K is plausible for the
tungsten-lit carpet, and the proxy deltas (0.007–0.011) are the smallest of the set.

Verdict: **unreasonable gate for this scene, plus a genuine product gap** — the solver silently reports a large
`after_error` for a target whose tonal range cannot reach the references, and the UI has no way to say so. The MUST
fix is a *reachable goal* (R2): project the five tonal quantiles of the goal through the same slope cone the curve
is allowed (the existing `cdf_curve` already computes this map), optimise and report against that, expose
"tonal range reached N %" in the batch summary/confidence, and make the test gate compare `after` with the
reachable goal. Codex should keep the carpet in the set — it is a good "flat scene" fixture — and add a second gate
that `after` moved at least 50 % of the *reachable* way from `before`.

### 3.3 Nikon D3200 NEF: proxy resolution bias from thin dark structure (a real proxy-fidelity defect)

The Nikon frame is a cast-iron garden bench against sunlit grass (contact sheet, bottom-left). Its darkest 5 % of
pixels are the bench's thin iron members. The solver measures every statistic on a 128-px box-downsampled
proxy (`measurement_source(src, info, 128)`, `pipeline:344-353`; `fine_src` in `solve_impl`, `pipeline:971`; and
`analyze`, `pipeline:434-443`), i.e. a 38× reduction of the 4928-px sensor. Thin dark members average with the
bright background and disappear, so the 5th percentile rises. Measured on the native tile (same scratch script):

| box factor (tile px) | L5 | L25 | L50 | L75 | L95 |
|---|---|---|---|---|---|
| 1 (320) | 0.292 | 0.405 | 0.498 | 0.654 | 0.788 |
| 2 (160) | 0.307 | 0.410 | 0.500 | 0.652 | 0.786 |
| 4 (80) | 0.331 | 0.415 | 0.503 | 0.648 | 0.781 |
| 8 (40) | 0.357 | 0.426 | 0.509 | 0.642 | 0.776 |

L5 climbs ≈ 0.03 per halving of resolution while L50 barely moves — exactly the signature in the test
(ΔL5 = 0.030, ΔL50 = 0.0002, ΔL95 = 0.002–0.004 between the 128-px proxy and the 1024-px render from the 2560-px
Preview source). The same effect is visible on the Panasonic kettle (ΔL5 0.021, ΔL95 0.017: specular highlights and
the dark grate) and, worse, on the Sony *reference* (native tile: L5 0.383 → 0.476 and L95 0.882 → 0.797 between
320 and 40 px — fine foliage). The reference statistics are measured at 128 px too (`analyze`), so the goal itself
carries the bias. Because the bias depends on each image's fine structure, it does not cancel between reference
and target.

So the Nikon failure is **a real proxy-fidelity defect, not a solver bug and not a decode bug**: the solver matches
the 128-px proxy within ≈ 0.005, and the user judges a 2000-px render where the dark ironwork exists. Secondary
factor, smaller: the Thumb source is an 8×-binned decode whose binner takes a block's *maximum* when any sample is
clipped (`crates/lc-raw/src/binned.rs:12-13, 101, 184`), while the 2560-px Preview the user looks at is binned 2×
(`files.rs:235-246`: `bin_factor` for 512 vs 2560 on a 4928-px sensor gives 8 and 2). An 8×8 block is clipped if 1 of
64 samples is; the clipped area is therefore inflated in the proxy, which moves L95 and the highlight-reconstruction
behaviour for scenes with small speculars (the kettle). I could not isolate this second factor without new code; R3
includes the measurement.

The fix (R1, MUST): measure the fine statistics at the Thumb's native resolution. Concretely: `analyze` renders at
512 px (one render per reference — negligible), and `solve_impl` keeps the 60-px coarse probes for the Jacobian but
runs the *fine* evaluations at 256 px with a one-time per-photo calibration of the five L quantiles (and chroma)
against a 512-px render of the start settings (`δ_i = f_i(512) − f_i(256)` added to every fine measurement). On
this test set that removes the ΔL5 (the 320→160 step above is 0.015 and 512→1024 is smaller still), at a cost of
one 512-px render plus 4× the cost of ≈ 12 fine evaluations per photo (≈ +40–60 ms per photo on this machine;
measured budget in R1's test). The test's `proxy` column then also stops being a cache-state artefact (3.4).

### 3.4 The test's proxy metric is itself cache-dependent (bug in the test)

`rawtest:158-159` measures the proxy as `analyze(&thumb, &s.source_info(id), d)` with `thumb = source_now(id, Thumb)`
but `info = s.source_info(id)` → `source_facts` (`media.rs:368-382`) which returns the *first cached level's*
`SourceInfo` (Thumb if cached, else Preview, else Full). `sensor_scale` and the `clip_confidence` plane differ by
level (`files.rs:429-434`: Thumb ≈ 9.6×, Preview ≈ 1.9×). Whether the Thumb is still in the LRU when the line runs
depends on `enforce_budget` after the two `source_now(Preview)` calls just above (`rawtest:156-157`), so the
`proxy` numbers differ between Codex's runs (0.0396) and mine (0.0298) for identical solved settings and identical
`error`. Same hazard for `full()` (`rawtest:50-53`): the 1024-px render from the Preview source is given whichever
info `source_facts` finds first. The solver itself is not affected (it uses `src.info_or(i.info)` from the decoded
source, `engine:481`). R3 fixes the test to take the info from the decoded source it measures
(`SourceRef::load_source()` → `DecodedSource::info_or`).

### 3.5 CR3 via embedded preview

`lightcraft-raw` has no CR3 decoder (`RAW_UNSUPPORTED rawpixls-2663.CR3 Some("Cr3 files are not decoded yet")`,
`Photo::preview_only`). The app shows and *edits* such a file from its embedded JPEG everywhere (Develop included:
`media.rs:1002`, `files.rs:341`), with `relative_wb: true`. Match Look is consistent with that: as a reference the
embedded preview is a legitimate "finished look" (it is what the camera rendered, 8-bit, with the picture style
baked in); as a target the solved sliders are applied to the same JPEG-domain pipeline the user sees in Develop, so
what the user previews is what they get. It is **acceptable for Match Look now**, with two conditions: (1) the batch
summary must say how many targets/references were matched on an embedded preview (today it is silent; R10), and (2)
it is not a substitute for the owner's raw requirement — a CR3 decoder is an engine item outside this feature (flagged
in section 6, "outside Match Look"). It must not be used to make the raw test look better: keep `develops_raw()` as
the filter and keep the `RAW_UNSUPPORTED` line.

## 4. New findings (revision-2 diff, with the owner's quality emphasis)

Severity: **H** = wrong result / data or undo damage / owner requirement / cannot ship, **M** = user-visible defect,
**L** = nit. Q = result quality, X = performance, B = bug, U = UX/design-system, T = tests.

### 4.A What the results look like (photographer's judgement)

I looked at Codex's contact sheets (`target/matchlook-review/sheets/{warm,cool,airy}-{similarity,consistent}.png`:
columns = reference, before, Match Look, Sync) and at the raw contact sheet. The JPEG set is six CC0 scenes
(portrait, street, branches, product glass, pet, backpack); the raw set is desk/foliage/kettle/bench/harbour/carpet.
The numbers are good (ΔE 2.5, no rails) and the mechanics are right (editable sliders, locks, layers, skin, B&W).
What a photographer would say:

1. **Every result drifts to a mid-grey, flat rendering.** "Cool" (a contrasty, desaturated, cool pair) turns the
   high-key water bottle into a dark grey product shot and lifts the low-key cat into a flat mid-grey; "airy" (bright,
   low-contrast, warm) *darkens* the bottle. The cause is structural: the objective matches the *absolute* L50 with
   the largest weight of all (5, `pipeline:508`) and L5/L95 with 4, so the solver's first job is to move every
   scene's median to the references' median, whatever the scene's key. Capture One's Normalize does the same on
   purpose, but C1 users' top complaints (yellow cast on evenly lit frames, bad on snow/water/events, excess
   contrast; `RESEARCH-matchlook-products.md`, and the C1 forum thread on headshots, section 5) are the same
   failure. Lightroom's and Imagen/Aftershoot's selling point is exactly "adjust exposure and WB *per photo* to the
   photo, not to the reference" (section 5). **Q1 (H)**.
2. **Looks come out desaturated and tint-shifted rather than "graded".** The palette fit is eight per-band
   saturation sliders only (`PALETTE_CONTROLS`, `pipeline:21-30`); there is no per-band hue or luminance, so a
   teal-orange or warm-skin/cool-shadow reference is approximated with global saturation, vibrance and three
   grading wheels. The warm look's branches/backpack come out brownish-uniform, the cool look's backpack simply
   duller. **Q2 (M)**.
3. **White balance swings to extreme Kelvin on dissimilar scenes.** Fuji harbour 5234 K → 22577 K, JPEG "glass"
   24088 K, "product" 24029 K (`raw-serial-fable.log`, `v2-final/summary.txt`). These are inside the bounds
   (2000–50000) so the rail audit passes, but a ×4.3 Kelvin ratio is a strong orange cast that no photographer
   would apply as "white balance"; the warmth should come from Color Tone, not from the illuminant estimate.
   Layer mode already limits the WB *shift* to e^±0.6 (`pipeline:567`); global mode has no shift limit and the
   temperature prior (0.00004, `:552`) is negligible against the neutral a/b weight (3 ×3 for features 25/26,
   `:509-516`). **Q3 (M)**.
4. **Tonal "shape" is too global.** Five global L quantiles cannot distinguish a crushed-black contrasty look from a
   lifted-black matte look below L5, nor say anything about local contrast; the cat/backpack results look soft.
   There is no L1/L99 (black/white point) and no local-contrast statistic, although the pipeline has Clarity/Texture
   style controls the solver never touches (`parameters()`, `pipeline:598-636`). **Q4 (M)**.
5. **The solver cannot tell the user how well it did.** `after_error`/`similarity` exist, but the panel shows only
   "Scene similarity N %" and the weights; a target like the Pentax carpet silently ends 60 % short of the goal.
   Capture One shows nothing either — this is a chance to be better. **Q5 (M)**, folded into R2.
6. Raw-specific: the proxy bias (section 3.3) means the *goal* is biased by the references' fine structure and the
   result by the target's; on the owner's architecture work (fine window mullions, railings, foliage) this is the
   common case, not the exception. **Q6 (H)** = R1.

### 4.B Bugs and defects

**N1 (H, B/T) The `lightcraft-ui-egui` test target does not compile at HEAD.** `widgets.rs:762` and `:766` call
`view.paint(&[])` but `HeadlessView::paint` takes `&HashMap<TextureId, CpuTexture>` (`headless.rs:105`); error
E0308 twice, reproduced by my `cargo test -p lightcraft-ui-egui tests_match_look` and already visible in Codex's
own `target/matchlook-revision2/widget-reveal.log`. Consequences: no Match Look click test (and no other UI test)
runs at HEAD; Codex's last green UI log predates this test. In addition the last UI log that did run
(`ui-tests-final.log`) shows `match_look_ui_context_reference_remove_and_profile_catalog` **FAILED at
`tests_match_look.rs:400`** (7 passed, 1 failed, 2 ignored) — unresolved when Codex was stopped. Fix: `&HashMap::new()`
(or `&Default::default()`), then run the whole UI module and fix whatever `:400` is (at HEAD `:400` is the keymap
assertion `keymaps::default(profile, command, …).is_none()`; the line may have moved).

**N2 (H, X) Decoded Thumb sources from a batch are thrown away, so every Analyze → Preview → Apply → re-Apply
decodes the same raws again.** `SolveWork::run_sequential` loads via `i.source.load_source()` (`engine:467`) and
never inserts the result into `s.media`; `AnalyzeWork::run` likewise (`engine:289`). `input()` only gets a
`SourceRef::Loaded` when the Thumb LRU already has the photo (`media.rs:435-449`). Measured: load 150–710 ms per raw
cold, 0 ms warm (`raw-parallel-fable.log`: first batch 990 ms wall for 4 raws, later batches 335–357 ms). On the
500-raw benchmark decode is ~65 % of the wall time (`loadMs` 145.6 s of worker time vs `solveMs`), so the owner's
"tweak Strength/options and apply again" costs the full 73 s every time instead of ~25 s. Fix: R5.

**N3 (M, Q/T) The real-raw test's proxy metric depends on cache state** (section 3.4): `rawtest:158-159` pairs the
Thumb image with `s.source_info(id)`, whose `sensor_scale`/`clip_confidence` come from whichever level
`source_facts` finds first. Same for `full()` (`rawtest:50-53`). Fix: R3.

**N4 (M, T) The per-photo timing gate is wrong for the owner's requirement.** `rawtest:139-141` fails any photo
with `load + solve ≥ 160 ms` in a *serial* run including a cold raw decode (150–710 ms). The requirement is 500
photos ≈ 80–90 s wall on adaptive hardware, which the app benchmark meets (73.4 s). The gate should be on
`equivalent500 / workers` (or the app benchmark), with per-photo numbers recorded, not asserted. Fix: R3.

**N5 (M, Q) Toned / split-toned black-and-white references are not detected as monochrome.** `is_monochrome`
needs mean chroma < 0.012 and all eight hue bands with support < 0.05 (`pipeline:898-900`). A sepia or selenium
B&W reference has chroma 0.02–0.05 concentrated in one or two bands, so it is treated as colour: Saturation and
Vibrance drive to −55/−65 and the wheels tint — exactly the muddy result P3 was meant to remove. Fix: R7.

**N6 (M, Q) Global-mode WB has no shift bound** (Q3 above). Fix: R8.

**N7 (M, U) Reference tiles in the tray scroll inside a 2-row box** (`widgets.rs:925`, `max_height = 2·edge`), so
with 10 references at 250 px the user sees 6 and must scroll a 100-px strip inside a panel that itself scrolls;
nested scroll areas fight the wheel. Capture One wraps the strip. Let the grid wrap fully (≤ 10 tiles = 4 rows at
250 px) and drop the inner ScrollArea. (L→M because 10 references is the owner's stated case.)

**N8 (M, T) `match_look_revision2_raw_clipped_proxy_and_whites` does not test what its name says** (`pipeline:1497-1509`):
both sides are 128-px outputs, so the assertion `|small − full| < 0.03` is near-tautological; it would not have
caught the Nikon case. Replace with a 512-px synthetic containing 1-px dark lines (thin structure) and compare the
128-px proxy with a 512-px render — fails before R1, passes after.

**N9 (L, X) The parallel batch solves on the background pool but decodes on the global pool at normal priority.**
Workers call `set_current_thread_class(Utility)` (`engine:396, 464`) which covers the std thread, but the demosaic's
`par_iter` runs on rayon's global pool threads (threading policy rule 7). Not a bug today (the heavy-job permit caps
decodes at 4), but the policy's "Utility class" promise is only half kept. Note for the threading job; no action here.

**N10 (L, B) Redo after Undo does not restore the per-photo Strength record** (`edit.rs:12-15` has no reconcile and
the record was removed on undo). Keep removed records in a small side map keyed by `(id, last)` and re-insert on
`edit.redo` when `develop_of(id) == last`. COULD.

**N11 (L, U) `look_menu` items are `text_action` bordered buttons inside a popup** (`ui:795-836`), whereas every other
Library popup uses plain menu entries (`ui.button(..)` with `.frame`-less look: `presets.rs:43-50`, `chips.rs:81-86`,
`right.rs:75-90`). The menu reads as a stack of small buttons. Use the same entries as `presets.rs` (and keep the
automation ids via `register`). COULD.

**N12 (L, U) "Next reference" is a text button that is disabled unless "Compare with reference" is on**
(`ui:591-599`) — two controls for one idea. Make the compare toggle a segmented control `Before | Reference 1/2/…`
or let clicking the reference tile in the tray choose the compared reference. COULD.

**N13 (L, T) The parallel test asserts order, not equality with the serial result** (`engine:1270-1281`). Add
`assert_eq!(parallel.rows, serial.rows)` on the same inputs (my two runs show they are identical; the test should
pin it). COULD, part of R4's test.

**N14 (L, B) `SolveWork::run` clones `looks` (up to 10 × 30 features + clusters) per photo** (`engine:409`): harmless
(~µs), but an `Arc<[Look]>` in `SolveWork` is the obvious shape. COULD.

### 4.C Optimisation observations (measured)

- Per photo at HEAD: ~70 renders per solve (`v2-final/summary.txt`: 72–74), 100–190 ms solve, 150–710 ms raw
  decode (Thumb, 8× binned), peak RSS 1.9 GiB for 500 raws with 7 workers.
- Jacobian cost dominates: 20 parameters × up to 3 passes of 60-px probes = up to 60 of the ~70 renders. Passes 1–2
  re-probe every parameter even when the step was small; a Broyden rank-1 update (or re-probing only parameters
  whose step exceeded 0.2·scale) saves ~30 % without changing results beyond 1e-3 (R9).
- The probes are independent renders; running them with `rayon::par_iter` inside the background pool (each with its
  own `StageCache`) would cut *preview latency* (one photo) by ~4× on 8 cores. Batches are already parallel across
  photos, so this matters for the dialog's Preview and the per-photo Strength drag, not for the 500-photo time (R9).
- Decode: the Thumb decode reads and unpacks the whole raw (ljpeg/NEF lossy/ARW) before binning; most of the
  150–700 ms is unpack. Nothing cheap to do inside Match Look except caching (N2/R5) and keeping the permit cap.
  (A tile-skipping unpacker is an lc-raw item, outside scope.)
- Memory: `bytes` per worker = sensor×6 + file size + thumb×32 + 16 MiB (`engine:237-246`) ≈ 300–450 MB for 40–60 MP
  files, so a 16 GB laptop with 8 GB headroom gets ~16 workers capped by `threads`; fine. Peak RSS 1.9–2.1 GiB on
  500 photos is consistent with 7 workers × ~250 MB.
- UI: 2 ms frame p95 during the batch (`raw-app-500.log`) — good; the dialog's auto-preview repaint every 300 ms is fine.


## 5. Research notes with sources

No photo, catalog data or file left the machine; only public pages were fetched (read 2026-10-13).

1. **Capture One Match Look / Normalize.** Official: https://support.captureone.com/hc/en-us/articles/22188770298269-Match-Look-Tool
   (four groups; Normalize = Exposure + White Balance; Impact does not touch Normalize) and the Normalize tool
   (https://alexonraw.com/normalize-tool-in-capture-one/ : "can change the overall white balance quite dramatically if
   the difference … is large"). Reviews: Thomas Fitzgerald (2024, https://blog.thomasfitzgeraldphotography.com/blog/2024/10/capture-one-165-match-look-a-remarkable-new-feature):
   "doesn't match the source 100 %", "helps if the subjects are broadly similar", recommends applying as a layer.
   Forum, corporate headshots (https://support.captureone.com/hc/de/community/posts/23309278723997-Match-Look-and-Black-skin-tones-corporate-headshot-setups):
   "heavy yellow color tone on single images for no reason", poor on Black skin (tint, contrast "increased to create deep
   shadows and overly bright highlights", blue-tinted hair highlights). *Applies:* our Q1/Q3 are the same two failure
   modes (absolute brightness/WB normalization on dissimilar frames); R4 and R8 address them where C1 does not.
2. **Photoshop Match Color** (https://www.johnpaulcaponigro.com/blog/17772/photoshops-match-color-may-change-way-see/ ,
   https://www.photoshopessentials.com/?p=905): global statistics of hue/saturation/luminosity, sliders Luminance
   (0–200, default 100), Color Intensity (1–200), Fade, and a Neutralize checkbox that removes the target's dominant
   cast with or without a source; destructive. *Applies:* the Luminance/Color Intensity/Fade triad is the model for a
   small "how much brightness / how much colour" control that is independent of Impact (R4 keeps it to one toggle;
   R10 exposes the measured numbers).
3. **Lightroom Classic (2025)**: no reference-matching tool; Adaptive Color / Adaptive B&W profiles analyse each photo
   and adjust "as if Exposure, Shadows, Highlights, Color Mixer, Curves … had been changed" with an Amount slider
   (https://blog.adobe.com/en/publish/2025/02/12/innovation-empower-creativity-explore-latest-in-lightroom);
   Reference View is manual (https://jkost.com/blog/2024/08/reference-view-in-lightroom-classic.html); a "Match Look"
   feature request is open (https://community.adobe.com/t5/lightroom-classic-ideas/match-look-feature/idi-p/14943471).
   *Applies:* our editable-slider output is the differentiator LR users ask for; per-photo adaptivity (R4) is what
   Adobe sells.
4. **Imagen AI / Aftershoot personal profiles** (https://support.imagen-ai.com/hc/en-us/articles/4446646190097 ,
   https://support.aftershoot.com/en/articles/9189832-how-to-edit-your-images-with-aftershoot-pre-built-ai-styles):
   trained on ≥ 2 000 / 2 500 of the user's own edits (XMP for raws), one profile per genre and per raw/JPEG, and
   explicitly "adjusting basics like exposure and white balance individually per photo rather than just stamping the
   same values". *Applies:* a statistics-based tool cannot learn a photographer's taste, but it can copy the one
   behaviour users value most — exposure/WB *relative to each scene* (R4/R8) — and it can keep raw and JPEG
   references in their own domains (already true: section 3.D of review 2).
5. **Neural Preset for Color Style Transfer** (Ke et al., CVPR 2023, https://arxiv.org/abs/2303.13511): splits
   transfer into *colour normalization* then *stylization*, with the style stored as a reusable preset and applied by a
   deterministic per-image colour mapping; artefact-free at 8 K. *Applies:* our architecture already has the same
   two stages (Normalize group, then style groups); the paper's lesson that normalization must be image-adaptive
   supports R4. Not a candidate to port (needs a trained network; pure-Rust inference would be a separate project).
6. **Semantic / region-aware colour transfer**: Reinhard et al. 2001 (mean/variance in lαβ) and Pitié et al. 2005 (iterative
   full-distribution matching) are global; Cusano et al. 2012 "Content-Based Colour Transfer"
   (https://diglib.eg.org/handle/10.1111/v32i1pp190-203) transfers between regions with the same class (vegetation,
   snow, water, sky…); Luan et al. "Deep Photo Style Transfer" (https://arxiv.org/abs/1703.07511) shows global methods
   "spill" sky style into buildings; He et al. 2019 "Progressive Color Transfer with Dense Semantic Correspondences"
   (https://arxiv.org/abs/1710.00756). *Applies:* our zone (shadow/mid/highlight) a/b features are a cheap proxy for
   regions; the app already has Sky/Subject masks (`cmd/masks.rs`, quick segmenter) that a COULD item (R17) can reuse
   as sky/foliage/skin weights without any new model.
7. **Nikon D3200 NEF**: lossy "type 2" 12-bit compression (encoding table 769, first missing value 114,
   https://www.photonstophotos.net/NikonInfo/NEF_Compression.htm); LibRaw notes non-zero black levels only for
   D3300/D5300 among 12-bit NEFs (https://www.rawdigger.com/node/226). Our decode reports `clip=true`, as-shot
   5844 K, and the proxy deltas are explained by resolution alone (section 3.3), so no black-level or
   linearisation issue is implicated. The pink stripe on the right edge of the Nikon tile (contact sheet) looks like
   an un-cropped masked-border column — an lc-raw crop question, outside Match Look (flagged in section 6).
8. **Pentax K-7 DNG**: nothing camera-specific is involved (section 3.2); the frame is simply a flat texture.
9. **Percentile statistics on downscaled images**: box downsampling is a low-pass filter; order statistics of the
   filtered image converge to the mean as the kernel grows (thin structure contributes less than its pixel share once
   it is narrower than the kernel). This is textbook signal processing rather than a paper result; the measured
   effect on this set (section 3.3 table) is the evidence that matters. RawTherapee's histogram matching works on the
   embedded JPEG *at its full size* and darktable's deflicker reads full-resolution raw histograms (review 2, sources
   1–2), i.e. neither uses a 128-px proxy for quantiles.


## 6. Revision plan for Codex

Numbered R0…; each with problem, evidence, exact change, the test that fails before / passes after, risk. Tiers
are by value to the owner (raw quality first). Keep everything inside the Match Look files unless stated.

Counts: **6 MUST (R0–R5), 7 SHOULD (R6–R12), 7 COULD (R13–R19).**

### MUST

**R0 — Make the UI test target compile and the UI Match Look suite green (N1).**
- Evidence: `widgets.rs:762, :766` (`view.paint(&[])` vs `headless.rs:105`), `widget-reveal.log`, `ui-tests-final.log`
  (`…reference_remove_and_profile_catalog` FAILED at `tests_match_look.rs:400`).
- Change: `view.paint(&std::collections::HashMap::new())` at both sites; run
  `cargo test -p lightcraft-ui-egui tests_match_look widgets::tests` and fix the `:400` failure (if the keymap
  assertion: the catalog entry for the three commands must have no default binding in the Lightroom/Capture One
  profiles; if the remove-icon click: `ui.hoverWidget` on `reference:{id}` must precede the click, which it does at
  `:389` — then the overlay `hover_icon_button` is not registered while hidden, and `register` must run before the
  `reveal` early-out, which it does at `widgets.rs:531`; verify).
- Test: the module compiles and all non-ignored tests pass; add that result to the Codex report.
- Risk: none.

**R1 — Measure statistics at Thumb resolution (Nikon root cause, section 3.3; Q6).**
- Evidence: `pipeline:344-353, 434-443, 971-985`; `rawtest` Nikon ΔL5 0.030; the 320→40 px table in 3.3.
- Change (pipeline):
  1. `analyze_with_faces`: `measurement_source(src, info, 512)` and `RenderRequest::fit(512, 512)` (one render per
     reference; the `signature` render stays at 128).
  2. `solve_impl`: keep `SOURCE_EDGE = 60` for the Jacobian; make the fine proxy 256 (`measurement_source(src, info,
     256)`, `fine_req = fit(256,256)`); add a one-time calibration render of the *start* settings at 512 (through
     `measurement_source(.., 512)`) and compute `offset[i] = f512[i] − f256[i]` for `i ∈ {0..=4, 7, 27..=29}` (zero
     elsewhere); apply `offset` inside `evaluate(.., fine = true)` to the returned `Look` before it is used (and to
     `initial`). The coarse 60-px probes are only used as differences (`jac`), so they need no offset.
  3. `Solved.after_error`/`before_error` are then computed on offset-corrected 256-px measurements.
  4. `PROXY_EDGE`/constants: expose `FINE_EDGE = 256`, `CALIBRATION_EDGE = 512` as `pub const` for the tests.
- Test: (a) replace `match_look_revision2_raw_clipped_proxy_and_whites`'s proxy assertion (N8) with a 512×384
  synthetic that has 1-px-wide dark lines every 16 px on a mid-grey field plus the clipped band: assert
  `|analyze(src)[0] − measure(render(src, fit(512)))[0]| < 0.01` (fails before: the 128-px analyze loses the lines,
  ΔL5 ≈ 0.04), and that `solve` on it reaches the goal L5 within 0.015 measured at 512. (b) `rawtest`: every proxy
  delta < 0.03 for all six raws in all four modes (Nikon fails before). (c) record the new per-photo `solveMs`
  (budget: ≤ +60 ms on this machine; the 500-raw app benchmark must stay ≤ 90 s).
- Risk: medium — saved looks analysed at 128 px are now slightly inconsistent with new ones; bump `LookFile`
  `version` and re-analyse on "Update from current references…" (import of old files keeps working, the statistics
  just carry the old bias). Determinism unaffected.

**R2 — Reachable tonal goal + fit reporting (Pentax root cause, section 3.2; Q5).**
- Evidence: `pipeline:328-345` (`cdf_curve` slope cone), `:571-574` bounds, `:1021-1024` Huber; Pentax error 0.16–0.22
  vs sync 0.21–0.30, L5/L95 misses 0.10–0.19 with proxy deltas < 0.011.
- Change (pipeline): in `solve_impl` after `goal` is blended and `initial` measured, compute
  `reachable = goal.clone(); reachable.features[0..5] = cdf_curve(&initial, &goal)` (the slope-cone projection that
  already exists) and use `reachable` for the tonal residuals in `objective`, for `before_error`/`after_error` and
  for `cdf_curve` itself (identity). Add `Solved.reach: f64` = 1 − (Σ_i∈0..5 |reachable_i − goal_i|·w_i) /
  (Σ_i |initial_i − goal_i|·w_i) clamped to [0,1] (1 = the references' tonal range is fully reachable), and
  `Solved.fit: f64` = 1 − after_error/before_error. Store both in `Applied` (engine) and return them in the batch
  result rows (`perPhoto` gains `reach`, `fit`).
- Change (ui): the "Applied to this photo" section and the preview show a chip row
  `Fit 92 % · Tonal range 100 %`; below 60 % reach show the caution line "References have a wider tonal range than this
  photo can reach — Light & Contrast was limited" (tr). The batch toast gains "N photos matched weakly" when
  `fit < 0.5 || reach < 0.6`.
- Test: (a) pipeline: flat synthetic target (L in 0.45–0.55) vs a wide reference: assert `reach < 0.6`,
  `after_error < 0.5·before_error` *against reachable*, and no curve/contrast at its bound; (b) `rawtest`: compute
  `goal_reach` the same way and gate `error_reachable < 0.5·sync_reachable` and `|L5|,|L95| < 0.08` against the
  reachable goal; Pentax passes, the other five unchanged; (c) ui click test: chip text present after Apply.
- Risk: low–medium (objective change). The Huber/prior stay; results on reachable scenes are identical because
  `cdf_curve(initial, goal) == goal` when the cone is not active.

**R3 — Fix the real-raw test so it measures what it claims (N3, N4, N13).**
- Change (`rawtest`): (1) `full()` and the proxy line must use the info of the source they render:
  `let src = s.media.source_ref(&photo, level).load_source()?; render(&src.image, &src.info_or(s.source_info(id)), …)`;
  (2) replace the per-photo `≥ 160 ms` failure with a recorded metric and one gate
  `elapsed_ms / rows × 500 / workers ≤ 90 000` for the *cold* (first) batch, plus a note that the app benchmark is the
  authoritative wall-clock gate; (3) run every batch twice, serial and parallel, and `assert_eq!(serial.rows, parallel.rows)`;
  (4) print `RAW_FIXTURE` with the Thumb's bin factor and `sensor_scale` for both levels; (5) keep CR3 excluded and
  print `RAW_UNSUPPORTED`.
- Test: the test itself; it must pass after R1+R2 on the seven-file set with no threshold loosened except the timing
  gate above.
- Risk: none.

**R4 — Scene-adaptive brightness normalization (Q1; the single biggest quality lift).**
- Evidence: sheets (`cool`/`airy` bottle and cat), `pipeline:508` (L50 weight 5), Normalize always at full strength
  (`fade_controls`, `:757`), C1 forum complaints, Imagen/Aftershoot per-photo exposure (section 5).
- Change (pipeline, no new UI): in `solve_impl`, after `similarity` is known, define
  `s = if opts.consistent_batch { 1.0 } else { similarity.sqrt().clamp(0.35, 1.0) }` and replace the five absolute
  quantile targets by `goal'_q = initial_q + (goal_q − initial_q)·s` **for the median only**, while the four *shape*
  terms `goal_q − goal_L50` (q ∈ {5, 25, 75, 95}) are matched at full weight around the adapted median, i.e.
  `goal'_q = goal'_L50 + (goal_q − goal_L50)`. (Consistent batch = "same session", keep today's behaviour exactly.)
  `keep_exposure` still removes exposure from `parameters()` as now. Document it in the Normalize tooltip:
  "Exposure follows the references fully for similar scenes and partially for different ones; Consistent batch
  always matches fully."
- Test: (a) pipeline: high-key synthetic target (L50 0.8) vs a low-key reference (L50 0.3) built from a different
  gradient so `similarity < 0.3`: assert solved L50 ∈ (0.55, 0.75) (moved ≤ 50 % of the gap) and the L5/L95 spread
  matches the reference's spread within 0.03; same pair with `consistent_batch` → L50 within 0.04 of 0.3 (today's
  behaviour); self-similar pair (`similarity > 0.9`) → full match. (b) JPEG harness: add per-look `median_shift`
  column; the `cool`→glass row must keep L50 ≥ 0.55. (c) `rawtest`: unchanged gates (the raw pairs are "similar"
  enough: similarity ≥ 0.5 for all four; print it).
- Risk: medium (behaviour change the owner must see in the contact sheets — regenerate `sheets/` and attach them
  to the report). Saved looks unaffected (the adaptation is at solve time).

**R5 — Keep decoded Thumb sources from a batch (N2).**
- Evidence: `engine:289, 467` (no `insert_source`), `media.rs:435-449`, load 150–710 ms cold vs 0 warm.
- Change (engine): `Matched`/`Analyzed` gain `decoded: Vec<(PhotoId, DecodedSource)>` collected by the workers
  (only when the input was not already `SourceRef::Loaded`); in `apply`/`apply_analysis` (UI thread, after the
  generation check) call `s.media.insert_source(id, SourceLevel::Thumb, src)` for each — the Thumb LRU is
  byte-budgeted (`THUMB_SOURCE_BYTES` 384 MiB, `media.rs:75`) so this is bounded; for the preview job insert only
  the active photo. Thumb sources are ~3 MB each (512² × 12 B), so 500 photos fit in ~1.5 GB — insert in batch order
  and let the LRU keep the tail; this is still a win for Preview → Apply and for re-applying with new options.
- Test: engine test with a counting loader: `prepare` + `run` + `apply` twice on 8 inputs → the loader is called
  8 times, not 16; `rawtest` second batch `load_ms ≈ 0` without the test's own `source_now` calls (remove them from
  the test's first batch to prove it).
- Risk: low (memory bounded by the existing LRU budget; `enforce_budget` runs on insert).

### SHOULD

**R6 — Tonal shape features: black/white points and local contrast (Q4).** Add features L1 and L99 (indices 30, 31)
and a local-contrast statistic (mean |L − box5(L)| at the fine proxy, index 32), `FEATURES = 33`
(`feature_array` already accepts shorter arrays; keep support 0 for old looks). Weights: L1/L99 2.0 (only when
support > 0 on both sides), local contrast 2.0 under `light_contrast`. Add `effects.clarity` to `parameters()` with
bounds ±40 and prior 0.0005 (`controls.rs:219`). Test: matte (lifted blacks, L1 ≈ 0.08) vs crushed reference →
`light.blacks` moves negative and L1 within 0.03; clarity moves positive for a reference with 2× local contrast and
stays 0 for equal local contrast. Risk: medium (new axis, +3 renders per pass; keep the ±40 bound).

**R7 — Toned / split-toned B&W references (N5).** `is_monochrome`: `features[7] < 0.012 || (features[7] < 0.045 &&
hue bands with support > 0.05 ≤ 2 && dominant band share ≥ 0.8)`; when the toned branch triggers, set `treatment =
Bw` and let Color Tone fit the wheels (already allowed), seeding `grading.midtones` with the dominant band's hue at
sat = 100·features[7]/0.1. Test: sepia reference (`treatment Bw` + `grading.global` 35°/20) → solved `treatment ==
Bw`, `color == base.color`, midtone/global hue within 25° of 35°, after_error < 0.3·before. Risk: low.

**R8 — Bound the global white-balance shift (Q3).** `Parameter::Temperature.bounds` in global mode:
`(wb.0·e^-1.0).max(2000)..(wb.0·e^1.0).min(50000)` (×2.7 either way instead of unbounded); tint ±40 around the
effective tint; scale the neutral a/b and global a/b weights (5, 6, 25, 26) by `0.5 + 0.5·s` from R4 so a dissimilar
scene gets its warmth from Color Tone. Test: Fuji harbour row `temp ≤ 14 200` and the JPEG harness `dCCT` column
stays below 2.7× for every row; synthetic daylight target + tungsten reference → temp ratio ≤ e, grading shadows/
midtones carry the warmth (sat > 8). Risk: low–medium (results on strongly mismatched pairs get less orange; that is
the point — regenerate the sheets).

**R9 — Fewer and parallel renders (4.C).** (1) Passes 1–2: re-probe only parameters whose last step was
> 0.2·scale, reuse the previous column otherwise (Broyden-free, simpler); (2) in `solve_impl` run the probe loop
with `rayon::prelude::par_iter` over `params` inside `in_background_pool`, one `StageCache` per probe, collecting
`(j, column)`; keep the sequential path when `rayon::current_num_threads() == 1`. Test: `match_look_deterministic_at_one_and_four_threads`
must still pass bit-for-bit (the probe order no longer matters because columns are indexed); assert renders ≤ 50
per solve on the JPEG harness and preview time on one photo ≤ 0.5× (print). Risk: medium (concurrency inside the
solver; only the probes are parallel, the line search stays sequential).

**R10 — Explain the result (Q5, section 3.5).** After R2: batch result rows carry `fit`, `reach`, `similarity`,
`wbRatio`, `previewOnly: bool` (embedded-preview source); the toast adds "N on embedded previews" and "N weak";
"Applied to this photo" shows the four chips; the batch summary is written to `last_result` for the panel's "Skipped
photos" block style. Test: ui click test asserting the chips exist and the toast text for a stubbed weak result.
Risk: none.

**R11 — Per-band hue and luminance in the palette fit (Q2).** Add 8 per-band mean-hue-offset features (circular
mean of hue relative to the band centre, weighted by chroma; indices 33–40) and fit `mixer.{band}.hue` (±40) and
`mixer.{band}.lum` (±30) in the existing palette step (`pipeline:1283-1330`) as two more joint directions with one
probe render each. Test: teal-orange reference (orange band hue +15, blue band hue −15 via the mixer) on a neutral
colourful target → solved mixer hues within 8 of the reference's; after_error < 0.4·before. Risk: medium (+2
renders; palette interplay with saturation — keep the existing acceptance rule "only if the objective improves").

**R12 — Reference tray wraps fully (N7).** Remove the inner `ScrollArea` (`widgets.rs:925`); the grid wraps to ≤ 4
rows at 250 px. Test: screenshot test at 250 px with 10 references shows all 10 `reference:{id}` rects on screen.
Risk: none.

### COULD

**R13** Redo restores the per-photo Strength record (N10). **R14** `look_menu` uses the plain menu-entry style of
`presets.rs:43-50` (N11). **R15** One control for the compared image: segmented `Before | Ref 1 | Ref 2 …` or click a
tray tile (N12). **R16** `Arc<[Look]>` in `SolveWork` and `assert_eq!(serial.rows, parallel.rows)` in the engine
parallel test (N13/N14). **R17** Region-aware weighting without a new model: reuse the app's quick Sky/Subject masks
(and cached faces) to compute sky / subject / rest statistics for reference and target and match them regionally
when both sides have support (the C1 "snow/water/sky" complaint; Cusano et al. in section 5) — effort high, value
high for landscapes/architecture; prototype behind an option. **R18** "Weak matches" smart collection / grid filter
after a batch (`fit < 0.5 || reach < 0.6`) so the owner can review them first. **R19** Per-reference weight pin in the
tray (★ = "weight this reference 2×"), stored in `State.reference_weights`, applied in `weights()` before
normalisation.

### Outside Match Look (flag only)

- **CR3 decoder** (`lightcraft-raw`: "Cr3 files are not decoded yet"); Match Look on CR3 uses the embedded preview like
  the rest of the app (section 3.5).
- **Nikon D3200 right-edge pink column** on the native render (contact sheet): looks like an un-cropped masked
  border; an lc-raw crop/active-area check.
- **Rayon global pool threads keep normal priority during batch decodes** (threading policy rule 7, N9).
- The owner's "any hardware" rule is met by `profile().threads` and the memory headroom in `SolveWork::run`; nothing
  in Match Look is tuned to this PC (the 160 ms test gate was — R3 removes it).

