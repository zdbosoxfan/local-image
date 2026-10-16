# U017 remembered dialogs — source handoff

Current: READY/FROZEN v4, seven owned files plus separately refreshed opener/string hunks.
Main SHA256 `9ff7264e060ffce1d46a22baa95a6da098e305ca118a2237782db6bb6147715c`.
The v1/v2 sections below retain history; use primary refreshed artifacts and v3 notes.

READY/FROZEN in `/home/zdavidson/.codex/worktrees/remembered-dialogs/Local Image - Testing`,
base `37e0d4787f6e56f1167fe86970e27f5fd4211c00`. No Cargo/tests/builds, runtime, GPU,
network/model requests or commits. Exact-owned-file rustfmt and diff whitespace
checks completed. Queue source remained frozen after its separately reviewed v2 fix.

## Patch integration

- Six owned files: `/tmp/remembered-dialogs-handoff.patch`,33418bytes,
  SHA256 `8c9b7f8425121708d63b4f4d2b3abe82e5abe5f61d9c38e97461f25365accbb5`.
- Separate candidate-based home New/control New opener hunks:
  `/tmp/remembered-dialogs-openers.patch`,1738bytes,
  SHA256 `416d4833771ab61e7e80f2a1aa9b07e5729beb296afeeaf709518695cfba928c`.
  Those files were not edited in this checkout. Preserve the Editor's canvas code;
  merge only these helper-call hunks and the now-unused UiState import cleanup. Explicit automation fields still override defaults.
- Separate append-only pending-label hunk: `/tmp/remembered-dialogs-strings.patch`,422bytes,
  SHA256 `56b285b9ec9ec0b7901e5756a856a1aecb32d243ae642e8bed3398fa842a0f20`.
  Do not replace the Editor-owned pending_vector file.
- `/tmp/remembered-dialogs-handoff.json` gives all exact hashes and integration modes;
  durable artifact copies beside this report. State/default and menu New/import hunks
  are narrow: do not replace unrelated shared source wholesale.

Owned paths: pc-ui-egui/src/{new_doc_ui.rs,filter_dialog.rs,dialogs.rs,jobs_ui.rs,
menus.rs,state.rs}. No engine metadata/preferences/shortcut/Rotate regions changed.

## Behavior

New Document uses existing Preferences.dialogs `file.new` for last successful choices
and an eight-entry deduplicated Recent list. Width/height/resolution/mode/depth are
recorded from the actual created document; validated background/design-unit choices
also survive. Invalid saved fields fall back independently. Names, paths and runtime
UI fields never become durable choices. Recent shows only actual successful creations,
with clickable dimension/resolution/model/depth descriptions; its fresh state is empty.

Menu/home/control openers share one helper. Existing untitled_name helper chooses a
name not already used by an open document or another New dialog; opening/cancelling
consumes no counter. Confirmation rechecks an auto-generated name against documents
created meanwhile. Explicit custom names remain intentional. Names use the existing
available-slot convention (closed slots may be reused), not a new persisted serial.
Cancel, Escape and creation failure leave preferences/history unchanged.

Filter defaults are schema-validated scalars under `filter.dialog/<command>` (stable
registered plug-in identity scopes plugin.run choices). Ranges use current f32 control
precision; enum strings must be a schema choice; booleans must be bool; integers stay
inside the existing numeric field bounds. Text/path, document/layer references, grids,
structured JSON, swatches, IDs and private fields are excluded. Non-filter contextual
commands, including ruler-derived arbitrary rotation, keep current behavior.

Confirmation preserves the existing smart-filter command adapter. A synchronous
success publishes its submitted snapshot; a pending response stores it with exact
JobId and actual command in existing JobsUi. The terminal event publishes only after
engine-applied Done. Failed/cancelled events drop it. Preview/open/cancel and later
dialog edits or another active document cannot publish/replace that snapshot. No
async runner redesign; existing prefs persistence/load/retry remains authoritative.

## Authored tests (not run)

Filter `remembered_` selects seven new focused regressions:

- New success/reopen/serialized-preference restart, dimensions/resolution/gray16,
  distinct names, cancel and failed creation unchanged, deduplicated Recent.
- Real Create/Close/Recent buttons plus Escape; earlier successful preset recall;
  visible row bounds; cancellation never replaces last successful choice/name.
- Malformed saved values, forbidden runtime/name keys, eight-entry Recent bound.
- Gaussian Blur + Add Noise scalar shapes, restart, real preview, cancel and no-doc
  failure leaving last successful defaults.
- Actual asynchronous Gaussian job, later document/dialog change, matching applied
  completion, cancellation, and injected failing worker through real jobs_ui polling.
- Schema scalar limits and exclusion of paths/IDs/structured/context-derived state.
- Actual filter checkbox + Cancel/OK events proving publication only on success.

Existing assertions/tests retained. Root should run focused `remembered_`, then
relevant existing New Document/filter/preview/jobs and preference persistence gates.
No fresh pass or rendering claim is made here. Plugin persistence uses existing
registered plugin schema and synchronous/job pathways; custom non-scalar inputs are
intentionally not remembered under this bounded scalar-only requirement.

## V2 — Custom Filter grid correction

Independent review correctly identified Custom's kernel:int[25] as reusable settings,
not arbitrary runtime JSON. The durable-value helper now accepts schema-declared
square integer grids only: positive declared length, at most256cells, exact flat
array length, each cell an integer in[-999,999]. Non-square/oversized schema shapes,
short/long/nested arrays, null/fractional/typed-float/out-of-range/oversized integer
cells are rejected without cloning. Arbitrary Json/path/reference exclusions remain.
This supersedes the scalar-only/grid-excluded wording above for legitimate Grid fields.

Two new tests (nine focused total, not run) cover actual Custom doubled-centre output,
reopen and serialized-pref restart, identical replay pixels, Cancel/no-document
failure preservation, plus malformed stored matrix read-repair and cancellation
without a preference write. Existing scalar/exclusion assertions retained. The
reviewer reported no other generic/New blocker; dedicated Filter Gallery's existing
journal-only restart behavior remains a separately reported scope question for root.

READY/FROZEN v2; original v1 full/openers/strings/manifest preserved. Current main
patch38746bytes SHA256 `18d7183ea723ccb2496e0b6c8db4b24bd73f5e3c036661e2d38c96a851c8ffa9`.
Minimal one-file correction `/tmp/remembered-dialogs-v1-corrections.patch`,7900bytes,
SHA256 `915c6e994743f53a5a3afa841975b6f65822596b728636a969537b510125a129`.
Openers/strings hashes unchanged. Exact before/after JSON and refreshed main manifest
copied beside report. Rustfmt/diff check only; no tests/Cargo/runtime executed.

## V3 — dedicated Filter Gallery restart and publication

Root granted gallery_ui.rs settings/publication and its existing tests. Gallery now
loads its typed recipe from Preferences.dialogs, with known GalleryFilter enums,
ordered effects and visibility. Parameters are limited to the declared schema and
bounds, plus a finite3/4-component[0,1] explicit glowColor. At most64 effects are
remembered. Current foreground/background swatches, layer IDs, preview surfaces,
zoom/selection and paths are excluded. Unknown fields are stripped; malformed known
values/recipes fall back to the normal Colored Pencil initial stack without writing
preferences. Over-budget/invalid runtime recipes keep existing application behavior
but cannot replace the last validated remembered recipe.

The old journal-only reopening fallback is removed. Session command records are not
durable preferences and cannot reintroduce an unproven/cancelled/stale recipe. The
existing successful UI commit now shares generic filter submit and its exact-job Done
publication. No new runner or Gallery UI/backend design. All-hidden no-op, preview,
Cancel, rejected invocation, cancelled job and stale failed apply do not publish.

Three Gallery tests bring focused total to12 (not run): typed stack order/visibility
and settings survive preference serialization into a journal-empty session; current
swatches stay current; preview/cancel/synchronous refusal unchanged; actual async
submission uses captured recipe despite later edits; actual cancellation and engine
stale-target Failed preserve defaults; journal contamination cannot restore old work;
bounded known schemas/color/visibility and forbidden runtime fields are checked.
Existing Gallery preview/engine/history assertions retained.

Current `/tmp/remembered-dialogs-handoff.patch`:52212bytes,
SHA256 `0115520592d7cadaa5e33ff01551ec8fc6e7f285817b455a320b03b25e519d87`.
Narrow v2-to-v3 two-file delta `/tmp/remembered-dialogs-v2-corrections.patch`:14486bytes,
SHA256 `0b8388fa727d63ed8d653a3b5eea2614307c43c0e164f1abaae1ffb2eb5330ff`.
Full7 hashes and exact delta before/after hashes are refreshed in matching JSON;
openers/strings patches unchanged, v1/v2 preserved. No tests/builds/runtime performed;
owned rustfmt/diff check completed. Root can run `remembered_` then affected existing
new_doc_ui/filter_dialog/gallery_ui/jobs/prefs tests at its single validation boundary.

## V4 — preserve supported long stacks and hex Glow Color

R2: removed the persistence-only64-effect cap; no application/UI limit was added.
Recipes preserve every supported effect in order. Validation/storage work is linear
in the already-parsed recipe, and each effect is restricted to its fixed declared
schema and typed bounded parameters. Existing Preferences.dialogs storage remains
its authority; the helper does not invent a lower quota and silently drop success.
This supersedes the v3 count-cap/over-budget limitation above.

R3: validated ASCII #rrggbb (including mixed hex case) is accepted alongside finite
3/4-component[0,1] arrays. The literal is retained, with the same byte/255 channel
interpretation as engine gallery_cmds::colour. The actual Gallery color-picker paint
also now reads hex correctly and writes an array only on deliberate editing; an
ordinary frame no longer replaces a restored hex value with the default color.
Malformed spelling, unknown filters, invalid visibility/parameters and runtime fields
remain excluded; no opaque JSON persistence was enabled.

TEST-CHANGED: removed only the obsolete65-effect rejection from the malformed-recipe
list. Added a real65-effect successful application/reopen/restart/reapply control and
valid lower/mixed-case hex successful recall/restart/ordinary-paint controls checked
against the engine's colour interpretation. Other malformed/no-publication assertions
remain.14 focused remembered_ regressions now authored, not run.

READY/FROZEN v4: main58953bytes,
SHA256 `9ff7264e060ffce1d46a22baa95a6da098e305ca118a2237782db6bb6147715c`.
Single-file v3 delta `/tmp/remembered-dialogs-v3-corrections.patch`:9481bytes,
SHA256 `2c38778b71acbff54b2fd32b8618b311c79610aa1f64aea22c3e5852054f2e9c`.
Matching full7/delta manifests refreshed; v3 preserved. Opener/string hunks have
identical intended edits but were regenerated against the newer candidate context:
openers SHA256 `ac9c46977e1ccff1f85349f6e8eb843641066d9c3d328fe70294afa9c64e6ba8`,
strings SHA256 `fd743509e2635809d821c5e5282429b8a1dda4c51c2f0e0e1cde921f43ebe019`.
Those shared candidate files were not edited; merge only the narrow patches. No
Cargo/tests/runtime/builds executed; exact-file rustfmt/diff check completed.
