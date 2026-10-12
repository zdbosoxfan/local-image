# Rotate View activation v1 — frozen source-only handoff

Base3207abd2b23c6f1f4f93b2e06b25b2079d8a8810, safely fast-forwarded from accepted foundationa8a6ddcf with zero incoming-path overlap and all53existing WIP hashes preserved. Earlier conventions/foundation checkouts and snapshots remain unchanged.

## Integration boundary

Use either the full `activation.patch`, OR `integration-main.patch` plus the six files in `shared-hunks/`, never both. Shared hunk separation protects simultaneous U017/PDF/U030 work: lib.rs is EXACTLY one early Escape call; state.rs changes Tool/View only; menus.rs changes only View commands; canvas.rs does not touch the home New opener or dropped/Place routes; pending_vector adds only seven Rotate strings; ai_remove_ui changes only navigation mask retention/quad painting and a test accessor. No Services, PDF import, New defaults, file_open, native schema, LC or source-upload changes. Full file copies are archival: do not overwrite other owners’ integrated files with them.

The gpu-scheduling-only.patch is an audit delta against the pre-scheduling activation test source, already included in the full/main patches. Do not apply it a second time. manifest.json records exact source and patch hashes plus strip-equality proof.

## Implemented behavior

- Real `Tool::RotateView` in both personas’ navigation groups, with visible View-menu entry, typed degree control and Reset View. Fresh Pixel R now selects Rotate View; AI Remove has no default letter while every explicit saved binding remains authoritative. Vector default R still selects object Rotate, and custom Rotate View bindings remain available. Registry-driven shortcut editing required no changes to shortcuts.rs, shortcut_dispatch.rs or prefs_ui.rs, so the accepted library::namespace exclusion and shared modal behavior remain intact.
- Each View has a default-zero clockwise radian angle and a serde-skipped gesture capture tied to immutable DocId plus viewport identity. Start is the actual press, not a later drag-threshold event. Release keeps angle; Escape restores that drag’s initial angle and cannot restart while the same button stays held. Tool/focus/modal/document changes clear stale capture without editing another view. Extra document windows have their own controls/capture. The one pre-job Escape hook prevents global AI/job handlers stealing an owned Rotate Escape; ordinary Escape remains available when no gesture owns it.
- Shared camera angle now drives actual CPU/GPU main, proxy, filter/adjustment and live-move presentation. No document transform, rotated replacement image, journal entry or export transform is created. Pan, pointer-anchored wheel/pinch/scrubby zoom, Fit and screen-axis scrollbars inverse-map the same basis. Fit preserves angle; Reset resets only angle. Match Rotation/All copies and reports actual angles.
- True transformed quads/segments now cover crop grids/handles/dimming, marquee/type boxes, Move transform controls/hits, artboard gaps/frames, slice/snap boxes, image borders, pixel grids, mask/trail textures and selection coverage. Rulers label the dominant document axis along each screen ruler and guides use actual rotated lines. Resize cursors follow camera axes. Point-only type/vector-shape/perspective-crop/layout consumers inherit the central mapping instead of redundant rewrites.
- Existing independent captured mappings in Move, selection/repeat, symbols, measure, corners and constructive tools now use the same camera inverse. Their document operations, revisions, target validation, preview/Undo and raw sample retention are retained. The unused standalone snapping Mapping was not changed.
- CPU rotated fallback includes a viewport-bounded analytic shadow mesh; upright shadow behavior remains unchanged. Rotated checker phase is screen-aligned like GPU; the established upright phase remains unchanged. Selecting Rotate View retains an existing AI Remove mask, and its Escape does not Clear/Cancel the mask. General U030 apply/Clear/Cancel behavior is not changed.

No touchscreen rotation or new restart-camera storage was added. Existing UI View serde gains a backward-compatible default-zero field; no document/native persistence schema is touched.

## Authored acceptance (unrun by worker)

Eight ordinary `canvas::rotate_view::tests` functions exercise actual menu/keyboard/pointer frames and CPU rendering: drag/typed angle/reset/held-Escape, exact Doc/revision/history/dirty and PNG-byte invariance; saved-R/disabled overrides and Vector object/view distinction; stale captures through document switching/reordering, independent extra-window controls and truthful Match Rotation; anchor zoom/pan/Fit/scrollbar and first-frame Restore-modal ownership; brush edit/Undo, direct node drag/Undo, crop handles and Free Transform entry; compact1.25x Pro/Studio controls and AI-mask retention through Rotate Escape; real text-field ownership; CPU shadow interior/clear-background/near-edge controls.

One new strict physical `gpu_canvas::view_tests::rotate_view_full_app_rtx5090_pointer_angle_pixels_and_reset` exercises the real app’s R tool, a37-degree pointer drag, scaled-vector pixels, actual Reset button, and unchanged document/history. The existing scaled-image oracle now inverse-rotates coordinates; its original one-byte tolerance and count assertions remain unchanged, including all zero-angle gates.

Coordinator runs generate PNGs under target/rotate-view-activation/: CPU rotated image, crop handles, compact mask/theme controls, CPU shadow, and actual full-app GPU image. The worker has not generated or inspected them.

## Scheduling and TEST-CHANGED declarations

TEST-CHANGED: approved explicit View/ViewXform constructor fixtures now initialize rotation (actual-view helpers copy v.rotation; intentionally upright fixtures use0), plus serde-skipped captureNone where View literals require it. Existing assertions were not weakened. Selection/Repeat/Measure standalone fixture mappings explicitly remain upright.

TEST-CHANGED: the existing full-app GPU pixel oracle incorporates view angle so it can validate the new actual-app rotated case; all existing sample coverage, numerical/hardware and document assertions remain.

TEST-CHANGED: eleven proven existing ordinary RTX5090/Vulkan-only UI tests and the new strict full-app rotation test gain descriptive ignore scheduling attributes. This is not a waiver or a hardware pass. `PHYSICAL-GATES.md` contains all twelve exact required local invocations. The scheduling-only delta is attribute-only; removing those12attributes reproduces the pre-scheduling file byte-for-byte. No module-wide ignore, skipped case, adapter-success fallback, numerical tolerance change or model execution was introduced. CPU/math tests remain ordinary. The existing release benchmark ignore is unchanged.

## Worker checks / limits

Standalone Rust1.98.1 rustfmt completed on all changed Rust files except the pending-list include expression, and git diff --check passed. Static constructor inventory found no explicit ViewXform literal missing rotation. The exact single-line lib hook and GPU scheduling body-equality proof were checked. No Cargo, builds, tests, GPU activity, inference, native-app control or commits were performed. Compilation, independent review, affected existing suites, mandatory physical gates and rendered/native acceptance remain coordinator work; this source handoff is not runtime acceptance or completion of the overall Linuxv2 goal.

During mechanical migration an approval reviewer rejected HEAD-based regeneration for potential unrelated-WIP loss. The rejected action did not run. All current files were preserved in /tmp/rotate-activation-before-literal-repair, and only three exact malformed constructor strings were repaired with byte-identical backup guards; no reset/stash/delete was used.
