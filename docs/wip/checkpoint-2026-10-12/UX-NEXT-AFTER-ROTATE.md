# Next original UX slice after active Rotate/PDF work

Prepared read-only at candidate `3207abd2b23c6f1f4f93b2e06b25b2079d8a8810` in `/home/zdavidson/.codex/worktrees/v2-recovery/Local Image - Testing`. No builds, runtime, network, source edits or new tests. Recovery reports below are in the original workspace; the candidate's recovery-doc directory is absent. This is an implementation handoff, not a fresh broad review or acceptance claim.

**Recommended next coding batch: finish U030 AI Remove mask workflow, keeping U017 remembered dialogs as the next separate batch.** U030 is a small, coherent change around one existing workflow and has three directly confirmed unmet behaviors. It removes silent loss of painted work without touching active Rotate camera/input files or PDF/export code. Do not combine it with the broader Vector panel/docking work.

## Original authority and current exclusions

Original requirement source: `/home/zdavidson/.local/share/local-image-dev/reviews/opus-final-review.md`, U017–U030 at lines155–169, U031–U035 at175–179; `/home/zdavidson/.local/share/local-image-dev/reviews/astra-final-review.md`, UX06–UX10 at198–202. `specs/bugfix/ux.md` delegates those original behaviors; `UX-REMAINING-AUDIT.md` is an older inventory, not an up-to-date declaration that every listed source defect remains.

Current accepted milestones to preserve:

- `0857b9b7`: labelled personas, photo admission identity/default Move, Shift cycling and Ctrl+K convention. `EDITOR-WORKFLOW-REVIEW.md` records ownership/callback corrections. Do not redo U011/U034 or these U016 clauses.
- `e0f57cc0` and `bd5fac02`: full-size Library export/theme authority, Library navigation/typed controls/Original/Smart Sort/download-state and narrow footer. Original progress report records13 focused/176 affected Library passes and strict lint at the navigation checkpoint. Preserve those U026 corrections.
- `750e260e`: Tab panel ownership and Restore modal/first-click behavior, plus corrected raw-key/filmstrip lifecycle. `EDITOR-CONVENTIONS-REVIEW.md` and accepted progress checkpoint cover this scope. Do not reopen Tab/Restore as the next feature batch.
- `3207abd2`: shared Library title/menu layout, Preferences naming and genuinely editable shared shortcuts. `LIBRARY-SHARED-SHELL-REVIEW.md` records accepted source corrections; subsequent runtime corrections/fixture declarations are in its implementation report. This task treats the coordinator's accepted shared-shell slice as closed for implementation, with final native proof still separate.
- Actual Rotate View activation/input/overlays remains an **active owner**, beyond accepted foundation `a8a6ddcf`. PDF clipping/fallback/export remains another **active owner**. Neither belongs in this handoff.

## Batch A — U030 AI Remove, three behaviors in one owner

### Confirmed source and reuse

| Original behavior | Candidate source | Still missing |
| --- | --- | --- |
| Apply on release should be the fresh default | `crates/pc-ui-egui/src/ai_ui.rs:38–69`, `AiOptions::remove_immediately`; `ai_remove_ui.rs:121` already supports submission after a painted stroke | Default remains false. Explicit saved false must remain valid after changing the fresh default. |
| Switching tools must not silently discard a painted mask | `crates/pc-ui-egui/src/ai_remove_ui.rs:20–25`, `sync` | Tool mismatch immediately sets `app.ai_remove=None`, discarding mask and local undo. |
| Clear and Cancel must have different visible effects | `ai_remove_ui.rs:227–233`, `actions` | Both buttons share the same `app.ai_remove=None` branch. |

Existing work is useful, not a reason to start over: Pending already captures document ID/dimensions, stores mask/bounds and bounded local undo; `keys` implements local stroke Undo; `remove` restores the Pending on synchronous submission error; `detected` deliberately imports scratch masks without immediate submission. `ai_remove_ui/tests.rs` drives actual pointer strokes and has `TEST_REMOVE`/`TEST_STATUS` seams. Restore's accepted real-model path already routes its detected mask here. Keep its explicit confirmation behavior.

### Exact proposed ownership

One Editor/AI-UX owner, source-only until a coordinator lane is assigned:

1. `crates/pc-ui-egui/src/ai_remove_ui.rs` — mask lifetime, separate Clear/Cancel semantics, pending-target/input guards and any local confirmation state.
2. `crates/pc-ui-egui/src/ai_remove_ui/tests.rs` — extend existing actual-event fixtures; preserve local Undo/subtraction/submission/Restore controls.
3. `crates/pc-ui-egui/src/ai_ui.rs` — **narrow AiOptions default/serde test hunk only**; do not disturb download/inference implementation.
4. `crates/pc-ui-egui/src/i18n/pending_vector.rs` — append only any necessary visible labels, preserving accepted shared-shell/Hide Panels additions.

Use the existing tool/session/dialog facilities; no new AI job runner, model backend or persistent mask format. If the chosen tool-switch confirmation needs a central route change, first hand root the exact additional hunk instead of taking `lib.rs`, `shortcuts.rs`, `panels.rs` or Rotate-owned files implicitly.

### Required behavior and acceptance

1. Fresh/default UiState shows apply-on-release enabled; an explicitly saved false survives reload. A painted stroke submits once on release when ready, never on down/move or duplicate Enter. Existing explicit/manual mode remains usable.
2. Paint two strokes in manual mode, switch tools and return. Either retain the captured mask while inactive or present the original explicit keep/discard choice before destruction. **No silent loss.** If retained while another tool is active, it must not steal that tool's Undo/Enter/Escape or draw an active Remove overlay over unrelated editing.
3. Clear zeros mask/bounds/overlay while retaining a usable Remove workflow; Cancel exits/discards the pending operation. After Clear, painting again works without an unrelated document edit. Plain Cancel/negative confirmation preserves document pixels/revision/history.
4. Preserve local mask Undo and subtract strokes. Failure/unavailable engine keeps painted work and offers the existing setup/retry path; it cannot consume the mask or claim success. Pending work never migrates to another document/layer after switching documents, closing the target or changing dimensions. Test A/B identity explicitly, using the current captured target and existing lifecycle rather than a new per-document storage scheme.
5. A detected Restore scratch mask still waits for explicit approval even when the fresh immediate flag is true. The existing test at `ai_remove_ui/tests.rs` around265 explicitly exercises this; retain it. Accepted restoration preview/modal ownership and real-model lifecycle must not be weakened.
6. Actual visible controls at760×480 and1440×900 in light/dark themes: Clear, Cancel, Keep painting and Remove remain reachable; blocked engine status is intelligible. Use existing harness first; final private native/real-model case reuses UX-ACCEPTANCE-CHECKLIST N06 with coordinator GPU slot, not a second model matrix.

Fixture declaration needed: current `remove_confirmation_selection_clear_cancel_keep_painting_and_document_switch` expects Clear and Cancel both erase Pending; that is an old behavior assertion which must be explicitly updated for the requested distinction, while retaining pixel/history/no-unintended-submission assertions. Existing manual-mask fixtures must explicitly choose manual mode after the fresh default changes; add a separate fresh-default positive so test setup does not hide the requirement. Report these as TEST-CHANGED rather than silently weakening them.

## Batch B — U017 remembered filter/New Document choices

This is the next separate coherent batch, not an extra task to attach to U030.

Confirmed source: `filter_dialog.rs:169–205` rebuilds schema defaults each open; `state.rs:1325` hardcodes Untitled-1/1920×1080; `new_doc_ui.rs:16` uses fixed “Recent” examples; `menus.rs:243` opens those factory fields; `dialogs.rs:338–343` creates the document and `:352–357` confirms a generic filter without publishing remembered settings. Existing `pc-engine/src/prefs.rs:843–845` already supplies `Preferences.dialogs`, and existing New/template name helpers in `pc-engine/src/file_cmds.rs` should be reused.

Proposed owner files: pc-ui `filter_dialog.rs`, `new_doc_ui.rs`, `dialogs.rs`, `state.rs` (New defaults only), `menus.rs` (New opener only), existing corresponding tests; append-only pending strings if needed. Root owns any **narrow** `jobs_ui.rs` success-publication hook needed for asynchronous filters. No separate preference store or engine parameter-schema redesign is warranted.

Acceptance: successfully create custom dimensions/resolution/mode/depth, then reopen/restart and get those choices with a newly allocated Untitled name; Cancel does not consume a name or overwrite remembered defaults. “Recent” reflects actual successful creations, not fixed examples. Successfully apply a nondefault Gaussian Blur (plus one different schema shape), reopen/restart and recover its choices; preview/cancel/failure/cancelled job cannot replace the last successful values. Remember the **submitted snapshot** only on actual success, not a later edited dialog or unrelated active document. Do not persist live document/layer IDs, file paths from unrelated sessions, `__preview`/runtime fields or derived ruler/canvas values as generic durable defaults. New scalar range validation and existing smart-filter adapters must remain intact. Use existing prefs retry/load and filter preview/job contracts.

## Remaining original items: bounded queue, not duplicate implementation

| Original item | Current disposition and exact source owner path | Next bounded acceptance |
| --- | --- | --- |
| U018/U020/U021/U031 menu conventions | Still visible in PC `menus.rs:917` (Object/Effect appended after Help; no top-level width overflow), `menu_catalog.rs:26` (Geometry's first child before normal Layer entries), `vector_integration.rs:1444` (import/panel rows), `menu_catalog.rs:530–531/768–772` and `menus.rs::checked` (duplicate/equivalent rows and missing checks). **Library title-menu overflow is already fixed; this is the Editor bar.** | One later menu batch: logical Vector order/Help last, Geometry immediately below Combine Shapes, keyboard/pointer reachability of every top menu at760×480, one discoverable entry per function and correct aligned checks. Preserve existing long-menu scroll/press-drag and newer modal/Tab ownership. Check alias equivalence before removing a duplicate. |
| U019 Pixel flyouts | `panels.rs:64–69` appends shared additions to all built-in Pixel sets; `:96–103` emits extra vector slots | With the menu/toolbar owner, fold equivalent shape/type/crop into intended flyouts or restrict extras to All Tools as original allowed. Keep actual vector tools reachable, saved custom sets and distinct Pencil/VectorPencil identities. |
| U022 empty AI Integrations | `prefs_ui.rs` renders “These settings aren't available…” when integration field order is empty; `menu_catalog.rs` still advertises AI Integrations | Small routing correction alongside menu work: existing Local AI setup opens, or empty navigation disappears. Use existing `li.localAi`; no new setup screen. |
| U023 plain-language errors | `menus.rs` forwards raw invocation error to visible status; vector integration `report` also forwards strings. Old PSD projection failure is separately fixed/owned by format work. | Add mappings only for reproduced reachable remaining UI errors (locked/missing target, no photo file, failed action), preserving diagnostic detail separately. Do not reclassify a fixed old PSD error as new work or launch a repository-wide wording rewrite. |
| U028 Vector panels | `vector_integration.rs:26/1284–1364` keeps floating-window membership in runtime state; `dock.rs` lacks those feature tabs; no Vector workspace in current workspace catalog. Symbols is **already a dock tab**, but `vector_packages/symbols.rs:104–106` still requires a typed name and its master editing uses raw channel controls. | Separate larger owner after menus: required dockable feature panels/workspace plus persisted placement/visibility, auto symbol names and ordinary color control; preserve selection/undo/Tab-hidden maintenance. Original Release clipping/compound paths are **already registered** at vector_integration1410/1412 and engine clip_compound, with existing geometry/undo tests: verify visible eligibility instead of reimplementing. Unlock All/Show All remain absent in inspected vector menu registrations. |
| U029 disabled/context actions | `menus.rs:737/755` supplies reasons only for overwrite-specific items; `vector_integration.rs::menu_items` uses document-present enablement with tooltip None; `canvas_tool_menu.rs::SELECTION_MENU` explicitly omits Generative Fill/AI Remove; Vector Move still uses layer-pick path | Later context-action batch: no-selection/locked targets show a reason without invoking failing commands; appropriate Vector object menu; selection AI routes invoke existing operations with captured selection/target. Reuse current menu handlers. |
| U032 shape/boolean names | `pc-engine/src/vector_packages/drawing.rs:91` and pc-ui `vector_packages/drawing.rs:147` still create “Live Shape” | Small feature-owner follow-up: descriptive shape-kind names, boolean-operation names and unchanged undo/persistence. Boolean result naming needs a targeted recheck at its actual geometry provider before edits; only new-shape generic naming was confirmed here. |
| UX07 Liquify diagnostics | `liquify_ui.rs:682` still paints field px/node and dab milliseconds unconditionally | Small Liquify presentation change: ordinary footer; diagnostics opt-in. UX06 action wrapping already has implementation/history; only final native bounds proof remains unless a new failure appears. |
| UX09 Frame Composition empty state | `vector_packages/frame_compose.rs:207–211` silently returns for non-text/non-box text | Fold into later Vector-panel batch: explicit select-frame-text instruction for inapplicable selection, working controls for a valid box text. No new composition feature. |
| U024/U025/U033/U035 and U026/U027 | Recovered Library/default/navigation/shared-shell code and accepted reports exist | Preserve accepted implementation. Native full-flow proof still belongs to final acceptance; do not use that missing proof to start duplicate coding. |

## Coordinator handoff

Assign Batch A's narrow ownership after the current owners freeze; no worker is launched by this report. Validate one coherent affected batch, then move to Batch B/menu work. Keep model/native validation exclusive and the installed normal-profile6acff845 session untouched. Existing acceptance evidence remains valuable but does not mark every original clause complete. Track each original ID against behavior/evidence, not an umbrella “UX done” label. No additional generic accessibility initiative, redesign, new workflow framework or feature expansion is proposed.
