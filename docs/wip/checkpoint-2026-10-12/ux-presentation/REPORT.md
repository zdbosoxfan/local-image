# UX07 / UX09 / U-032 presentation details — source-only handoff

Frozen at base `528b6fd51ebc5dd6b82f63d1838720f58e8f29b1` in `/home/zdavidson/.codex/worktrees/ux-presentation-details/Local Image - Testing`. Seven approved files, including the separately granted narrow boolean-provider hunk. No source change in the integration candidate, tests/builds/runtime/GPU/network/commits or new agents. Pinned Rust 1.98.1 rustfmt (owned files, skip_children), whitespace and reverse-apply checks passed.

## Original requirements

- `/home/zdavidson/.local/share/local-image-dev/reviews/astra-final-review.md:199` (UX07): normal Liquify editing should show user-oriented feedback; field px/node and dab timing belong in explicit diagnostics.
- Same source `:201` (UX09): inapplicable Frame Composition should explain how to make it usable, explicitly “Select frame text”, rather than an empty window.
- `/home/zdavidson/.local/share/local-image-dev/reviews/opus-final-review.md:176` (U-032): generated layers should be named by shape kind and boolean operation instead of every result inheriting “Live Shape”. The implementation brief `UX-NEXT-AFTER-ROTATE.md` was reread; unrelated U030/U017/Rotate work is excluded.

## Changes

**Liquify:** normal feedback is `Strokes: N`. Field spacing and last-dab milliseconds are inside an explicitly expanded **Performance diagnostics** section, initially collapsed. The existing bounded right-side scroll area, footer, stroke engine/options, masks, rendering and history remain unchanged. Disclosure state is ordinary egui view state; no new global preference system.

**Frame Composition:** no document, no active selection, a non-text layer or point text now displays **Select frame text** and returns a usable empty state. Existing valid frame-text controls and stale-draft synchronization remain unchanged; no geometry/composition behavior is replaced.

**Live-shape names:** a shared engine helper supplies shape-kind defaults (Rectangle, Rounded Rectangle, Ellipse, Star, etc.). Detached UI preview and pointer commit use the same helper; the UI translates existing kind labels and passes its generated name explicitly. `shape.design.create` now accepts optional `name`; a nonblank explicit name is retained verbatim. Missing/null/blank optional values use the kind default; non-string values fail before mutation. Editing an existing shape preserves its name. No retroactive document renaming or schema change.

**Boolean/Pathfinder names:** actual provider is `pc-engine/src/pathops_cmds.rs::run`, whose result loop previously cloned the source name. The narrow metadata hunk applies existing Vector Geometry labels (Add/Subtract/Intersect/Xor/Divide; Trim/Merge/Crop/Outline/Minus Back) to result layers. Optional nonblank `name` overrides the generated result label verbatim; absent/null/empty form defaults cannot erase it. Every piece of a multi-result operation follows this policy. Source snapshot/undo, layer IDs, result ordering, paint/caches and geometry kernel are unchanged. Unrelated offset/smooth/reverse/outline-stroke/etc. operations retain their previous names and behavior.

## Authored regressions — not executed

1. Liquify feedback in a narrow panel: normal stroke count present, diagnostic rows absent initially, real disclosure click reveals bounded metric rows, document unchanged.
2. Frame empty state for no-document/no-selection/shape/point-text: visible hint fits a narrow viewport, no inapplicable width controls or document/journal mutation.
3. Shape defaults and explicit custom name: Rectangle/Ellipse kinds, verbatim custom name through edits, Undo/Redo and native save/load; invalid name type leaves source unchanged.
4. Actual shape pointer preview/release: preview and committed Ellipse names agree; one Undo/Redo restores exact documents.
5. Boolean Divide multi-results: operation default and empty optional value yield Divide, custom name stays explicit on all outputs, Undo/Redo preserves exact source/result documents.

No old assertions, geometry thresholds or tests were removed. Existing focused geometry, frame drag/apply, Liquify stroke/mask and native persistence tests should remain in the affected gate. The narrow authored checks are not a claim of full native viewport/theme acceptance.

## Artifacts and integration

- `implementation.patch`: **780ba4fa592371de9add1a13b48c8ab082c377f044fab5d1d69a8099eb51ec09**, 21,132 bytes.
- `strings-append-only.patch`: **999b6c8e52a493be8734633964e3eeb185496a2b264ad8e1d789a0bd27e9c3c3**, 638 bytes; nine additions also listed in `strings.json`.
- `sha256.json`: exact base/full source hashes; `files.txt` and `source/` snapshots alongside.

`pending_vector.rs` was not edited. Root must merge only string additions, preserving Editor and PDF/shared-shell changes. No file overlaps the Rotate v1 source set. The boolean-provider grant is confined to its name helper/assignment/optional parameter declarations and focused test; no algorithm change.

Suggested coordinator commands under the established pinned/offline/frozen lane (not run here):

```text
cargo clippy --offline --frozen -p photocraft-engine -p photocraft-ui-egui --all-targets -- -D warnings
cargo test --offline --frozen -p photocraft-engine -p photocraft-ui-egui --lib -- liquify_ui::tests frame_compose::tests drawing::tests pathops_cmds::validation_tests
```

Use the established feature set to avoid unnecessary cache churn, then inspect actual Liquify/frame/layer-panel behavior at supported compact/scaled viewports. Root owns integration/runtime evidence and final original-app completion. Source frozen; coding permit released.
