# Linux v2 checkpoint — paused to conserve credits

User request, 2026-10-12: save all current progress and remaining-work notes, build the current app, push the checkpoint to `v2`, then pause. This is a **development checkpoint, not a finished release**. No remaining requirement is waived.

## Current source and evidence

The commit containing this note includes the current integrated Rotate View and U017 remembered New/filter/Gallery work. Strict affected all-target Clippy passed. The fresh focused run passed **30 tests and failed 3**; it is not a green release gate. The failures are:

1. `canvas::rotate_view::tests::rotate_view_extra_window_controls_and_text_focus_are_local`: expected text focus after clicking a layer spin button, got None. Diagnose the actual enabled/editable field and pointer interaction; preserve the no-shortcut-steal assertion.
2. `gallery_ui::tests::remembered_gallery_hex_glow_color_survives_success_restart_and_paint`: JSON numeric representation `3.0` versus `3` after ordinary UI painting/recommit. Hex colour remains preserved. Check semantic equivalence and correct the fixture or actual behavior without weakening colour/recall assertions.
3. `new_doc_ui::remembered_tests::remembered_new_recent_uses_real_create_close_and_recall_buttons`: the global `Close` query matches more than one button. Scope the real click to the New Document dialog; keep cancellation/default/name assertions.

The source-only Rotate reviews passed after fixing rotated scrollbar hit regions, screen-axis brush resizing, unbound shortcut labels and dormant AI Remove mask key ownership. Runtime coverage is incomplete: the new full-app GPU case and the other eleven required physical view tests still need explicit execution, as do actual extra-window gestures/typed/reset/Match All. See `checkpoint-2026-10-12/rotate-activation-v2/PHYSICAL-GATES.md`.

Accepted earlier milestones remain in Git history: `9095d440` native clipping/text/CMYK/feathered-mask export (32 focused +48 other exporter checks,373 core tests); `37e0d478` CI hardware/numerical separation (actual RTX5090 and forced llvmpipe haze passed); `528b6fd5` portable queue source (59 Python tests passed). The portable tools have NOT replaced the installed runtime; legacy automatic dispatch remains held.

## Saved work NOT integrated into the app

All artifacts below are in `docs/wip/checkpoint-2026-10-12/` and deliberately remain patches, so unfinished work does not silently change the built app.

- **PDF image fallback:** `pdf-fallback-v2/` full or split patch, then v3 and v4 correction patches; append strings separately. Source reviewers accepted the cumulative result. Actual renderer/provider, cancellation/process lifecycle, JPEG2000/JBIG2 fidelity, UI Open/Place/drop and installed dependency checks remain unrun. Preserve linked-to-embedded consent, stale-target checks and no-unconfined-retry policy. Never apply both a full and its equivalent split patch.
- **AI Remove U030:** `ai-remove/` incremental patch and separate strings. Fresh apply-on-release, saved-false preservation, retained masks, distinct Clear/Cancel and async failure handling are authored but not independently reviewed or run. The JobId terminal tests are synthetic contract proof, not real inference proof. Patch base includes current Rotate/U017 source; preserve both.
- **UX07/UX09/U032:** `ux-presentation/` patch and strings for opt-in Liquify diagnostics, Frame Composition empty state and descriptive shape/boolean names. Five tests authored, unrun; review first.
- **Editor menus/toolbars:** `interrupted-menu-worker/work-in-progress.patch` is the exact interrupted worker delta against its recorded seeded source. It is incomplete/unreviewed/uncompiled. Resume original U018/19/20/21/22/23/31 without dropping working command IDs or the distinct selection/matting routes. Do not treat this as a ready handoff.
- Remembered-dialog and Rotate source patches/reviews are also archived for provenance. Their changes are already integrated; do not apply them twice.

## Resume order

1. Read this note, `checkpoint-2026-10-12/EXECUTION.md`, current release matrix and saved reviews. Inspect actual Git/process state; do not resurrect old workers or the deleted recurring monitor. Work from `codex/v2-recovery` / remote `v2`, preserving the original checkout.
2. Resolve the three focused failures above, then rerun only those cases. Run affected UI suites and the explicitly required real GPU checks at the integration milestone; inspect rendered output and native extra-window behavior.
3. Independently review/integrate the saved PDF, AI Remove and presentation patches in dependency order, with narrow shared-file merges. Validate focused behavior before affected suites. Finish interrupted menu work, U028 Vector docking/workspace and U029 context/disabled-state behavior. Preserve every original remaining item in the saved inventory and UX audit; older audit rows may already be superseded by accepted commits.
4. Measure the remaining original performance proposals using the saved matrix. Preserve quality and CPU fallback. Existing real AI/model/RAW receipts can be reused only when relevant source is unchanged; do not rerun the 15-minute tiled-upscale quality test just for UI edits. The release matrix documents scoped reuse and required workflow rechecks.
5. Final exact-candidate formatting, strict lint, supported workspace tests, required RTX/AI/native import/edit/cancel/save/reopen/export/restart workflows; HEIF-enabled Linux package, upgrade/rollback and dependency acceptance. Fetch/reconcile `v2`, normal non-force push, recoverable final install and installed version/hash proof. Linux first; macOS/Windows release, large Library scaling and full translation review remain explicitly deferred.

## Build and coordination notes

Use Rust1.98.1, pinned craft-fonts, retained caches and one coordinator Cargo command at a time. Three source owners maximum with explicit ownership; four Cargo jobs, ten global compiler slots and two ordinary test threads. Heavy builds/GPU/inference intervals are exclusive. The user has already authorized the final v2 push and local installation; no repeat approval needed.

A temporary test-build staging operation restored old file timestamps and exposed stale Cargo artifacts in one feature graph. The bytes were intact. All15 restored inputs were timestamp-refreshed, and the fresh focused run compiled successfully before its three test failures. Future restoration must refresh mtimes (`os.utime(path,None)` after copying), or use properly isolated checkpoint builds. Never full-clean the cache as the first response. Recheck the affected core/export gate with fresh dependencies before final acceptance.

Before this checkpoint the installed test app was `3207abd2b`, with rollback backup `~/.local/share/local-image-dev/install-backups/20261012T185907Z`; its normal-profile app was opened without a control endpoint. Do not terminate a user instance or touch personal documents/settings/models during validation. Final checkpoint build information is recorded separately after compilation. Binaries, model caches, credentials and runtime profiles are not committed to Git.

Paused after saving/building/pushing at the user's explicit request. Resume only when asked.
