# BUILD threads-b: thread-policy leftovers, swatches test fixes, pending labels (Claude)

Branch `wip/threads-b`. Follows `docs/wip/BUILD-threads-a.md` (Needs 1-6) and `docs/wip/REVIEW-threading-policy-fable.md` section 2.

## 1. Priority classes at the spawn sites (via `lightcraft_sysmem::set_current_thread_class`, first line of the thread body)

| Thread | Class | File |
|---|---|---|
| `export` | Utility | `lc-ui-egui/src/export_task.rs` |
| `lc-import` | Utility | `lc-ui-egui/src/import.rs` (first line of the `work` closure) |
| `smart-sort` | Background | `lc-ui-egui/src/smart_sort_task.rs` |
| `model-download`, `seg-download` | Background | `pc-ui-egui/src/ai_ui.rs` |
| `model-install` | Background | `pc-ui-egui/src/model_browser.rs` |
| `batch-cutout` | Utility | `pc-ui-egui/src/generate_ui.rs` |
| `photo-restoration` | Utility | `pc-ui-egui/src/restoration_ui.rs` |
| `upscale-custom` | Utility | `pc-ui-egui/src/ai_ui/enhance_settings.rs` |

Not changed: the two `std::thread::spawn(job)` folder scans in `lc-ui-egui/src/import.rs` (short directory walks, not long jobs).

## 2. `gpu_canvas.rs` scoped-thread helper

The pixel-format conversion (`px.len() > 1 << 18`) now uses rayon `par_chunks` / `par_chunks_mut` on the shared pool, chunk count `rayon::current_num_threads().min(16)`; no OS thread per chunk. Output is per-pixel, so it does not depend on the thread count. (rayon is already a non-wasm dependency of pc-ui-egui, same cfg as the code.)

## 3. Cancel plumbing

- `select.sky`: the job calls the sky model with `ctx.cancellation_flag()` (then `ctx.check()?` as before). The sky model is now resolved on the thread that starts the job: new `seg::SkyModel` (+ `sky_probability_with`, and a thread-local test hook `mock_sky_model_on_thread` that reuses `MockModel`), the same pattern as `SubjectModel`. `sky_probability_with_cancel(…)` stays and delegates. This was needed to be able to test sky cancel at all (there is no installable sky model in tests). Note for the reviewer: this touches `seg.rs` (small, additive).
- `remove background`: the model is resolved in `run` (caller thread), the job calls `seg::subject_with(&model, …, ctx.cancellation_flag())`, and a cancel returns `EngineError::Cancelled` before the classical fallback, so a cancelled model run is not mistaken for "no subject" and does not start the heuristic.
- Tests: `select_extra_cmds::tests::cancelling_select_sky_stops_the_model_run_promptly`, `cutout_cmds::tests::cancelling_remove_background_stops_the_model_run_promptly` (gated mock model that never opens; the worker ends without the gate opening, selection / document unchanged). Both pass, with the existing smartselect cancel tests.

## 4. photocraft-ui-egui lib failures from the Swatches merge (c5849aa5)

- `every_tl_literal_is_translated`: the three Color-panel slider names ("Colour hue", "Colour saturation", "Colour brightness") are added to `i18n/pending_vector.rs`, the established English-until-translations-resume list that this test (and the menu test) already honour. I did not reuse "Hue" / "Saturation" / "Brightness": the Color panel and the Hue/Saturation adjustment panel can be on screen together and the distinct names keep accessible names unambiguous.
- `persona_clicks_and_shortcuts_keep_document_dock_selection_and_history`: the query `get_all_by_label("Background")` was ambiguous (Layers row plus the Swatches panel's Background colour chip, both buttons). The test now counts what the Layers panel actually drew for the Background layer, via `layer_row_ui::recorded` filtered by layer name. The chip keeps the Photoshop term "Background" ("Foreground" / "Background"; "Fill" / "Stroke" in the Vector persona). Assertions kept: exactly one Layers row before, same count after the persona switches.

## 5. Pending labels (translation pipeline entries, all in `pc-ui-egui/src/i18n/pending_vector.rs`)

- Processing (from threads-a): "Processing", "Processing threads", "Background work priority", "Memory for processing", "Detected: {logical} logical / {physical} physical cores{hybrid}, {ram} GB RAM".
- `jobs_ui::non_modal_text`: "Selecting subject…", "Selecting object…", "Selecting sky…", "Selecting…" (these are translated at display time with `tl!(name)`, so a catalog entry later is picked up with no code change).

## TEST-CHANGED

`persona_clicks_and_shortcuts_keep_document_dock_selection_and_history`: query scoped to Layers rows (reason in section 4); no assertion weakened.

## Final checks

- `cargo fmt` on photocraft-engine, photocraft-ui-egui, lightcraft-ui-egui.
- `cargo clippy --offline --all-targets -- -D warnings` on those three packages: clean.
- Lib suites, once: photocraft-engine 1441 passed, 27 ignored; photocraft-ui-egui 1499 passed, 49 ignored; lightcraft-ui-egui 327 passed, 5 ignored. 0 failures.

## Needs from other owners

None for this batch. Remaining from threads-a Needs 7 (later batches): JobPool sizing from the compute profile in `lc-preview/src/pool.rs` and `lc-ui-egui/src/render.rs`; import probe `lc-engine/src/import.rs:405`; `lc-catalog/src/local.rs:253`; `lc-ui-egui/src/menus.rs:1048` system-info line. The English-only labels will need real translations in the later i18n pass (the pending list entries mark where).
