# U017 remembered dialogs — independent source review

## V4 final re-review (current)

**READY for coordinator integration and focused execution. R1, R2 and R3 are resolved statically.** No builds, tests, runtime actions or production source edits were performed by the reviewer; this report is the only written file. Runtime acceptance remains outstanding.

Verified exact frozen hashes:

- Main seven-file patch: `9ff7264e060ffce1d46a22baa95a6da098e305ca118a2237782db6bb6147715c`.
- V3→V4 Gallery correction: `2c38778b71acbff54b2fd32b8618b311c79610aa1f64aea22c3e5852054f2e9c`.
- Refreshed New home/control openers: `ac9c46977e1ccff1f85349f6e8eb843641066d9c3d328fe70294afa9c64e6ba8`.
- Refreshed pending-label append: `fd743509e2635809d821c5e5282429b8a1dda4c51c2f0e0e1cde921f43ebe019`.
- All seven source hashes match `/tmp/remembered-dialogs-handoff.json`; all three separate-hunk candidate base hashes also match current Rotate-aware candidate files.

R1's exact bounded Custom integer-grid validation remains unchanged from the cleared V3 review. R2 is corrected without introducing a new UI or application cap: Gallery validation/storage visits the already parsed effect list linearly, reconstructing only the fixed declared schema per known effect. All supported effects are retained. The new65-effect test exercises normal control add/change, actual commit, full-order reopen, preference serialization/restart and reapplication without truncation.

R3 now accepts exact ASCII `#rrggbb` (including mixed case) and bounded3/4channel numeric arrays. Hex parsing uses the same byte-to-f32 division as the engine and preserves the accepted literal in stored values; malformed spelling is refused. The ordinary Glow Color painter reads via that parser and writes a replacement array only when the color control actually changes. Reopen/paint therefore cannot silently substitute the default color or overwrite an accepted alpha when untouched. The two literal controls compare interpretation against the engine, successfully commit/restart, run two ordinary UI frames, then reapply and assert unchanged durable recipe. Existing malformed-value and lifecycle controls remain; only the obsolete65-effect rejection was removed. Fourteen remembered_ tests are authored, not run.

Refreshed shared-file patches contain only the intended changes: home New uses the common remembered-fields helper; control New does likewise before explicit field overrides and removes its now-unused UiState import; pending strings append the one Recent-empty label. Their shifted line context reflects current Rotate code, but no Rotate behavior/content is overwritten by these hunks. Integrate shared state/menu/dialog/job hunks around current candidate work rather than replacing older whole files blindly.

Final owned source SHA256:

| Path | SHA256 |
| --- | --- |
| crates/pc-ui-egui/src/dialogs.rs | 2697b3f155f341eb84a9b2a427c1acde428fa04f7607e1303d0c1adab9f0f8e9 |
| crates/pc-ui-egui/src/filter_dialog.rs | f2b35bd689094a9ed00fa75598426227a5dd1ffb46c813115c6145a37e1f3acd |
| crates/pc-ui-egui/src/gallery_ui.rs | 0007a6c0ec93a6950fc4ee8194a0757a382b391ded8ca0f2fee3b5fe32c69eb9 |
| crates/pc-ui-egui/src/jobs_ui.rs | 94b9f5a80f04568d78599f3b77fc1646b26a2a01c4d35615d9c173b18e1587ea |
| crates/pc-ui-egui/src/menus.rs | 06327be2ad83c7cbbedae5bdbdc6204c8b0528683b22353de7c2ccf12f98e15a |
| crates/pc-ui-egui/src/new_doc_ui.rs | ea2e638bfbf8ce9b435f36ab653eba0d43a08291670f86cc7a4c64b0c0518220 |
| crates/pc-ui-egui/src/state.rs | 4c8bebbb4899556df942539d60498619b014fceb209048f447e1b051446e303a |

The original source-scoped exclusions remain intentional: no layer/document IDs, captured surfaces, current swatches, source paths from unrelated sessions, private runtime keys or ruler-derived state are made durable. No further concrete blocker found in this frozen delta. Suggested next gate is affected strict lint plus all14 remembered_ controls, followed by existing New/filter/Gallery/preview/jobs/preference tests under coordinator ownership.

---

## Archived V3 review

### V3 findings before correction

**BLOCKED on R2/R3 below. R1 Custom kernel is resolved.** Frozen main patch `0115520592d7cadaa5e33ff01551ec8fc6e7f285817b455a320b03b25e519d87` and all seven source hashes were verified against the refreshed handoff manifest. Separate opener/string patch hashes are unchanged and verified. No source edits, builds, tests or runtime actions were performed; this report is the only written file.

The Custom grid now accepts only a nonzero square schema size at most256 and an exact-length flat integer array with every cell in−999…999. Malformed shape/types fall back without mutating preferences. Its new controls apply a doubled-centre kernel, verify pixel gain and unchanged alpha, reopen/restart/replay, cancel, no-document failure and malformed values. This resolves the original definite grid finding without admitting opaque JSON.

Gallery now stores a typed recipe (known filter keys, declared validated values, visibility and explicit bounded glow color), excludes captured source/runtime/swatches, uses preferences rather than journal, and publishes via the same exact-job applied-Done path. The authored success, restart, preview, cancel, no-op, injected refusal, cancellation and real stale-target tests cover the intended lifecycle statically. Two admission mismatches remain:

### R2 — persistence-only 64-effect cap silently loses a successful supported recipe

`gallery_ui.rs:25–32` rejects recipes above64 only while constructing/restoring remembered data. `GalleryDialog::add` at line154 and control `add` at line286 have no such admission cap; engine `gallery_cmds.rs:64–79` iterates all supplied effects without a64 limit. `gallery_ui.rs:265–267` turns failed recipe validation into `None` and still successfully executes the entire filter stack. A user can add65 legitimate effects, apply successfully, then reopen an older/default stack. The old journal-based opener would retain that65-effect stack, making this a concrete recall regression. Bound memory deliberately, but coordinate the bound with an explicit supported admission behavior rather than silently treating successful application as unrememberable. Add a boundary control proving the selected policy around64/65 without a large rendering benchmark.

### R3 — valid bounded hexadecimal Glow Color drops the entire recipe

The actual engine `gallery_cmds.rs:16–27` accepts both `[r,g,b(,a)]` and `#rrggbb`. The Gallery control route accepts parameters and immediate commit (`gallery_ui.rs:270–310`), so a `neonGlow` recipe with `glowColor:"#336699"` is a valid successful command. New `remembered_effects` at lines51–59 accepts only arrays and returns `None` for the string, which then prevents publication of the entire recipe. Normalize the supported exact seven-character hexadecimal spelling into bounded canonical channels, with strict malformed-string refusal; this does not require arbitrary text/JSON persistence. A direct control-commit/reopen test should preserve both that explicit color and the other recipe settings.

V3 source SHA256 changes from the v1 table below:

- `crates/pc-ui-egui/src/filter_dialog.rs`: `f2b35bd689094a9ed00fa75598426227a5dd1ffb46c813115c6145a37e1f3acd`.
- New `crates/pc-ui-egui/src/gallery_ui.rs`: `51d7db3ac9d23599ead897ff7d7c9def7b8eeb586398e7d2815ba4b58e29a238`.
- Other five owned source hashes unchanged from the table below.

The prior Gallery restart scope observation is now addressed for admitted recipes; R2/R3 concern the new typed admission edges. No further blocker found in inspected current catalog schemas. Root and author received these findings before re-freeze.

---

## Archived v1 review

Status: **BLOCKED on R1 for the frozen v1 handoff**. The author has acknowledged R1 and is preparing a separately frozen correction. No source changes, builds, tests, runtime, renderer or network operations were performed by this reviewer.

Reviewed checkout: `/home/zdavidson/.codex/worktrees/remembered-dialogs/Local Image - Testing`, base `37e0d4787f6e56f1167fe86970e27f5fd4211c00`.

Verified frozen artifacts byte-for-byte against `/tmp/remembered-dialogs-handoff.json`:

- Main patch: `8c9b7f8425121708d63b4f4d2b3abe82e5abe5f61d9c38e97461f25365accbb5`.
- Separate home/control opener patch: `416d4833771ab61e7e80f2a1aa9b07e5729beb296afeeaf709518695cfba928c`.
- Separate pending-label patch: `56b285b9ec9ec0b7901e5756a856a1aecb32d243ae642e8bed3398fa842a0f20`.
- All six owned source hashes match the manifest. Those exact source hashes are reproduced below.

## R1 — Custom Filter loses its actual kernel (required correction)

`crates/pc-ui-egui/src/filter_dialog.rs:206` excludes every `Kind::Grid` from persistence. The real registered Custom Filter (`crates/pc-engine/src/filters_ext.rs:670`) has `kernel:int[25]`; its visible editor at `filter_dialog.rs:464–479` is a 5×5 integer grid bounded to −999…999. On open, lines292–293 restore the identity kernel. Consequently, after successfully applying a custom sharpen/edge kernel, reopening preserves scale/offset but replaces the actual filter kernel with identity. Restart has the same loss.

These 25 coefficients are stable, source-independent user settings, not a document reference or arbitrary opaque JSON. Original U-017 in `~/.local/share/local-image-dev/reviews/opus-final-review.md:155` asks for a filter dialog's last values and does not restrict this to scalar values. The handoff's final claim of a “bounded scalar-only requirement” is not the original requirement.

A bounded schema-sized integer-array validator is sufficient; it need not admit arbitrary Json, IDs, object graphs or unbounded grids. Validate shape/length and cell range on restore and snapshot, preserve submitted-success-only publication, and add actual Custom Filter success/reopen/serialized-preference restart plus malformed/cancel/failure controls. The author reports this correction is authorized and in progress. V1 remains frozen and superseded only after new hashes/re-review.

## Scope observation — dedicated Filter Gallery restart persistence

`filter_dialog.rs:151–153` excludes Filter Gallery from the generic schema dialog. Its dedicated opener at `gallery_ui.rs:178–196` reconstructs the last stack from `session.journal`; `gallery_ui.rs:229–235` commits through `app.run` without storing preferences. It already recalls the successful stack within a session, but this patch does not provide its `prefs.dialogs` restart persistence. This is an existing dedicated-dialog coverage gap, not a newly broken route. It should remain explicit if claiming all-filter U017/restart closure. A future bounded typed effect-list (registered filter keys plus validated parameters/visibility) can retain settings without persisting captured layer IDs, source surfaces, textures, zoom or selected-document state. Root decides that separate ownership/scope; R1 alone blocks this v1 source handoff.

## Successful paths and boundaries checked

- New Document (`new_doc_ui.rs:46–103`) shares validated remembered fields, allocates an available Untitled name across open documents/dialogs, and rechecks auto-generated names on confirmation. Successful `file.new` result identifies the created document; size/resolution/model/depth/design units are taken from that accepted document. Failure exits before preferences/history change. Last/Recent contain no names/runtime keys, and Recent deduplicates and is bounded to eight entries. Malformed fields independently fall back. No invented monotonic-across-restart naming requirement is imposed: existing available-slot naming is appropriate.
- New menu path uses the common helper (`menus.rs:243–246`); the separate narrow canvas home and control opener hunks do the same, with explicit control fields overriding defaults. These separate hunks must be integrated alongside the main patch without replacing unrelated Editor-owned files. Dialog confirmation dispatches through `new_doc_ui::confirm`; cancellation/Escape do not call it.
- Generic filter open/preview/cancel never publish. Snapshot is taken before `app.run`, validated per current schema and keyed by filter command/stable registered plugin identity. The smart-filter adapter remains in `dialogs.rs:346–350`; preferences use the original filter identity while pending completion is matched against the actual submitted adapter command.
- Pending values are owned by exact JobId and command (`filter_dialog.rs:249–271`, `jobs_ui.rs:228`). `jobs_ui::tick` polls `Session::poll_jobs` before dispatch. Engine `jobs.rs:758–810` produces Done only after cancellation checks, target/snapshot validation and successful apply; a closed/stale document or apply failure produces Failed. Merely finishing worker computation is not sufficient. A later dialog/document cannot replace the stored snapshot. Cancelled/Failed terminal events remove it without publication.
- Scalar validation rejects wrong types/out-of-range numeric values and unknown enum values. Excluding live document/layer selectors, current foreground/background swatches, internal `__` fields and ruler-derived context is correct. Displace's `mapPath` is source-specific external data, distinct from the source-independent Custom kernel; this review does not require blindly persisting paths from unrelated sessions or opaque JSON. Any future source-reference recall needs its own intentional scope/validation policy.
- Existing preference load/edit/persistence mechanisms are reused. No separate store or async engine redesign was introduced. Proposed tests are authored, not executed; runtime/persistence gates remain coordinator-owned.

## Source hashes

| Path under checkout | SHA256 |
| --- | --- |
| crates/pc-ui-egui/src/dialogs.rs | 2697b3f155f341eb84a9b2a427c1acde428fa04f7607e1303d0c1adab9f0f8e9 |
| crates/pc-ui-egui/src/filter_dialog.rs | 70c7362c79e885a39d3dab147a5b2fb61880abdde4878eb03727c098ada1d967 |
| crates/pc-ui-egui/src/jobs_ui.rs | 94b9f5a80f04568d78599f3b77fc1646b26a2a01c4d35615d9c173b18e1587ea |
| crates/pc-ui-egui/src/menus.rs | 06327be2ad83c7cbbedae5bdbdc6204c8b0528683b22353de7c2ccf12f98e15a |
| crates/pc-ui-egui/src/new_doc_ui.rs | ea2e638bfbf8ce9b435f36ab653eba0d43a08291670f86cc7a4c64b0c0518220 |
| crates/pc-ui-egui/src/state.rs | 4c8bebbb4899556df942539d60498619b014fceb209048f447e1b051446e303a |
