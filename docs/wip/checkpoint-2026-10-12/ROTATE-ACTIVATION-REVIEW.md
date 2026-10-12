# Rotate View activation — independent review

Status: **FINAL SOURCE REVIEW — v2 READY for integration and execution.** The three v1 findings below are resolved in the exact frozen v2 correction, as recorded at the end. This is static source review, not execution evidence: no Cargo, builds, tests, GPU, renderer, native-app interaction, network, or source edits were performed. Runtime acceptance remains coordinator work.

Reviewed checkout: `/home/zdavidson/.codex/worktrees/editor-rotate-activation/Local Image - Testing`, HEAD `3207abd2b23c6f1f4f93b2e06b25b2079d8a8810` plus frozen v1. Patch `/tmp/editor-rotate-activation-v1/activation.patch` SHA-256 `76ce97c0dd5847cd5763fb561f122f16dbe38784b45eb4f6835b7e0209693384`. All 65 manifest entries matched both current source and immutable `files/` snapshots when inspected. Author may subsequently change the checkout for corrections; v1 snapshots remain the review reference.

## Actionable findings

### R1 — rotated scrollbars and raw vector input disagree (P1)

`crates/pc-ui-egui/src/vector_integration.rs:466–472`, `canvas_scrollbar_at`, still derives scrollbar presence from unrotated document width/height and document center. `scrollbars.rs:138` now draws bars from the rotated screen basis. A 100×1000 document at 90 degrees in a 500×500 viewport has an actual horizontal scrollbar and no vertical scrollbar; the raw-input guard predicts the opposite. Vector tool input at the actual bottom bar can reach editing handlers before the bar is handled, and the phantom right strip blocks valid canvas input.

Minimal repair: share the same camera-aware bar geometry between rendering and raw-pointer exclusion. Regression: actual pointer press/drag on the rotated-only scrollbar changes camera position without changing document/revision/history or beginning vector capture; the phantom strip remains available for intended editing. Include reflection.

### R2 — on-canvas brush resize still assumes upright input (P2)

`crates/pc-ui-egui/src/brush_resize.rs:108–114` computes screen-right diameter changes from document `dx × flip`, and vertical hardness changes from document `dy × zoom`. The activated canvas now inverse-rotates pointer coordinates before `tool_event`. At 90 degrees, horizontal Alt+right-drag changes hardness instead of diameter, while vertical drag changes diameter.

Minimal repair: capture the view basis for the resize gesture and recover screen-oriented deltas; diameter uses horizontal screen displacement divided by captured zoom, hardness uses vertical screen displacement. Keep the document-space anchor and existing coalescing. Regression: real 90-degree horizontal/vertical resize gestures, reflected control, unchanged document/history and correctly independent size/hardness changes.

### R3 — saved AI Remove shortcut display resolves the Blur group (P2)

`state.rs:505` changes `AiRemove.key()` to `\0`. `shortcuts.rs:83–85`, `tool_shortcut_id`, groups every matching key, including `\0`, so AI Remove now resolves the first unbound tool, Blur. Explicit saved `tools.select.AiRemove = R` still dispatches through the override pass, but tooltip/toolbar lookup can omit R or display Blur's override. Existing new test checks actual dispatch, not this lookup.

Minimal repair: do not treat the absence of a default key as a shared shortcut group (or narrowly preserve AI Remove identity). Regression: `tool_shortcut(AiRemove)` reports its explicit saved R and remains independent of a Blur assignment, alongside actual R dispatch and Vector object Rotate controls.

## Reviewed coverage and limits

The reviewed camera adapters consistently use the shared inverse/forward basis for pan and pointer-anchored zoom; fit uses rotated extents; scrollbar rendering/clamping uses screen axes. Crop, Free Transform handles, artboard frames/pasteboard, selection outlines, guides/grid, slice overlays, AI mask texture and stroke trail now transform document geometry rather than drawing screen-axis boxes. Selection/repeat/measure/symbol and move captures carry their basis. No additional concrete math/overlay defect was found in this bounded pass.

`rotate_view.rs` confines angle/capture writes to View state and uses document/viewport ownership, actual-press capture, explicit angle/reset and early owned Escape. The separate `ux_acceptance` reviewer is independently covering commands, focus/modal lifecycle, extra windows and Match Rotation/All; this report does not substitute for that review. Authored CPU/GPU tests remain unexecuted by this reviewer. Accepted CPU/GPU presentation foundation was not re-audited in full. No touch, restart-persistence, or unrelated architecture requirement was added.

## Inspected source hashes

| File (relative to checkout) | SHA-256 |
|---|---|
| `crates/pc-ui-egui/src/canvas.rs` | `0400497705305bfd159c87a0a33683a73e8b912dcdee611ffe45b4040d267d0e` |
| `crates/pc-ui-egui/src/rotate_view.rs` | `c1aeb43635c36926bed9b523e65de7a27697c8fe639334226bcd9076335efefd` |
| `crates/pc-ui-egui/src/rulers.rs` | `9b6ff6b7001ac21f77e018168341bdd0078a0b60a8d77f86a38934568e9711e0` |
| `crates/pc-ui-egui/src/scrollbars.rs` | `43300abf0bcb779042afe8359c797fa574fb784c5d7c79d5a469fc8742df6cf8` |
| `crates/pc-ui-egui/src/state.rs` | `205302953ca6e8b7670235c3541f8697e9dddaf9015464497f127879a561c365` |
| `crates/pc-ui-egui/src/view_cmds.rs` | `09435948bcd8e39cd5ad011cb674838b6c0db873dc429d8ec500e7e025781c4e` |
| `crates/pc-ui-egui/src/vector_integration.rs` (unchanged dependency) | `f8cb35ba88079c6c0c8b540ed8074fde4d9d22adfa5e730bf0ca578eecfb81c6` |
| `crates/pc-ui-egui/src/brush_resize.rs` (unchanged dependency) | `f87928117e928cdabb88a2f534dbac3c2f85b4072b52dc48e0be602653e7fc2c` |
| `crates/pc-ui-egui/src/shortcuts.rs` (unchanged dependency) | `88c7eee19310e8e749c2838990e268706774b634e6a2bd29526c1919a3adfce2` |

Complete 65-file identities are in the verified v1 manifest. This report is the only file written by this reviewer.

## Final v2 correction re-review

Reviewed `/tmp/editor-rotate-activation-v2/REPORT.md`, `corrections-from-v1.patch`, changed production adapters and authored regressions. Base remains `3207abd2b23c6f1f4f93b2e06b25b2079d8a8810`. Full patch SHA-256: `62c4485cbc6b99066398108472ef48e1a8535c7d643fadb804faad9db19fcd5e`. Correction SHA-256: `8f167711d76e0072a5fcb159d5f51751623046b35de095de25daa8980b2c391c`. All **69** manifest hashes matched both author checkout and immutable `files/` snapshots, with zero mismatches.

- **R1 resolved:** `scrollbars::axis_spans` supplies both actual bar rendering and `visible_bar_rects`; `vector_integration::canvas_scrollbar_at` constructs its camera-only View from the current `ViewXform` plus actual document size. It therefore cannot reuse an upright/stale cached View for exclusion. New actual-pointer regression checks rotated-only bottom-bar ownership, camera movement without document/revision/history change, and successful drawing in the formerly phantom right strip followed by Undo.
- **R2 resolved:** resize capture stores forward screen basis and zoom at Down; updates map the document delta back into screen axes before applying diameter/hardness formulas. Canvas publishes its actual frame View before raw tool routing, so first-frame Fit/clamp state agrees with the pointer inverse. New pointer regressions cover 90-degree reflected/unreflected primary Ctrl+Alt and secondary Alt drags, independent horizontal diameter/vertical hardness, one coalesced tool-state update, and unchanged document/history/no paint stroke.
- **R3 resolved:** unbound tools retain their own shortcut IDs instead of sharing the first `\0` tool. The existing saved-R actual dispatch test now also asserts the AI Remove ID and displayed effective shortcut.

Also inspected the narrow dormant-mask key guard and Shift free-angle test correction. The separate UX reviewer reported SOURCE READY for lifecycle/command/mask controls. No additional concrete source blocker was found in this correction pass. This verdict does not claim compilation, authored test success, physical GPU acceptance, or complete U016 runtime closure.

| Corrected source | SHA-256 |
|---|---|
| `crates/pc-ui-egui/src/brush_resize.rs` | `897923b0a15e57fa8d229b694491d747cad1c23f09b16fc899a97f77392ef24d` |
| `crates/pc-ui-egui/src/canvas.rs` | `4ce9342d2289f2fe9e223e35effb3d6343dc6d952d7c24ab32f7cd7e0582c76b` |
| `crates/pc-ui-egui/src/scrollbars.rs` | `88070b79f595be68eafcd5c61b812b224781336b154c74231bbd602e1b1e165d` |
| `crates/pc-ui-egui/src/shortcuts.rs` | `68aeba18a1cf98459b484861014560da89a94f4c3c0b31d97914df08f84f509f` |
| `crates/pc-ui-egui/src/vector_integration.rs` | `839899576b58e56709e0109e1e731961f0d818f416cfeb0077fd5feb3015feb7` |

Complete final identities are in the verified 69-file v2 manifest. V1 findings above are retained as review history, not outstanding defects.
