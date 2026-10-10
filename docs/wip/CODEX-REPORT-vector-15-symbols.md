# Package 15 — Symbols

Status: **implemented and verified in the integrated baseline copy; final joint acceptance
pending shared activation/catalogs and named renderer/export providers**. Do not mark
this package complete before those coordinated gates finish. Changes
are uncommitted on `wip/vec-15-symbols`, based on `22cf5a12`.

## Continuation and ownership

Continued the existing foundation: SymbolId/SymbolDef/SymbolInstance, Document.symbols,
native master storage, history snapshots and the compiled engine registration slot.
The prior engine symbol slot was empty; no builder implementation was discarded.
Read the task, architecture, full persona/owner decisions, quality plan and package README.

Only task-owned source and handoff/source/i18n/report files changed in the actual
checkout. No shared core files, crate manifests, lockfile, catalog, or aggregate test
registration were edited there. [The exact package-00 patch](vector-tasks/integration/15.md)
activates the enum/format/UI/compositor consumers atomically. It currently covers 30
shared files and applies cleanly (`git apply --check`). A complete disposable copy
inside this worktree, `target/symbol-15/worktree`, carries that patch for real integrated
crate/app verification. No files outside this worktree were written; no desktop
window, running owner app or ComfyUI job was touched.

## Implementation

- Master create/rename/update/delete/list, place, multi-instance transform and detach.
  `symbol.editMaster` edits captured ordinary master objects; `symbol.update` replaces
  from captured selected layers. Pure public master/placement previews publish nothing.
  Every edit validates document ID/revision, complete IDs/locks and the dependency graph,
  prepares detached caches, and publishes one Session::edit / ordinary command journal
  entry. Errors preserve document/revision/history/journal. Parent/child property edits
  are supported without allowing overlapping destructive target sets.
- Validation covers both ID namespaces, missing references, all nested/unused masters,
  cycles, group/reference depth, expansion fanout, finite bounded affine/path/paint/text
  inputs, singular matrices and bounded cache allocation. Limits: 100,000 objects,
  document coordinates/affine entries within ±1,000,000, normal layer depth limit64,
  and 16,000,000 pixels per prepared cache. Large shape bounds are checked before pixel
  allocation; text ink bounds are checked before rasterization.
- Public data-only expansion returns ordinary layers plus composed affine transforms,
  retaining enclosing instance masks/effects/blending and fresh template LayerIds.
  It is shared through pc-doc, with no lower crate dependency on the engine or UI.
  `expand_scene` consumes the existing exchange ImportedScene. Transient per-master
  generations propagate through dependencies and undo; source revision and preview
  epochs invalidate actual display caches. Geometry comes from masters rather than
  stale template caches. Legacy transformed canvas fills use a bounded raster fallback;
  detach of those fills yields ordinary raster layers.
- Symbols is an initially hidden ordinary dock tab with Create/Place/Edit/Apply/Rename/
  Delete/Detach, master selection, name typing, visibility, solid paint and text controls.
  Native placement captures press coordinates/view transform and target IDs, previews
  through the real display_doc path, and commits on release. Escape, focus, tool/persona,
  document/revision or captured selection changes cancel. Opening/applying a master without edits preserves HDR paint metadata and creates no
  history/journal entry. State has serde defaults and transient captures/previews are not persisted. Existing default dock layouts stay intact.
- FORMAT_VERSION2, Symbol persisted content and no-op v1 migration activate together.
  Cached instances use existing Sink/Fetch/hash/depth/ZIP/directory infrastructure.
  Raw duplicate IDs/cycles/missing references are rejected before tile allocation and
  LayerId remapping; SymbolIds are preserved. Native loader/writer validate graph budgets.
- Shared patch covers all content matches reported by compiler checks, GPU structure
  cache keys, proxy/composition, layer kinds/filter/properties, generic move/transform/
  rasterize, image/depth/color conversions and PSD pixel fallback. Cross-document copy
  transfers the transitive master closure with fresh master/template IDs in the same edit.
  It validates the resulting graph again after the destination offset, rejecting an
  out-of-range placement before either document or its journal changes.

## Acceptance and tests

Test source includes 13 engine CPU tests plus one strict engine RTX parity test,
three panel/settings kittest tests, one full-app RTX test, one explicit release-only
pointer measurement, and five native/migration symbol tests. The focused automatic gates currently pass: 23 tests; the one release-only
measurement is run separately. Final suite/lint/timing results are recorded below.

Independent oracles include separately created ordinary type layers, separately constructed ordinary rectangles for three linked
instances, 256 bounded placements, scale/quarter-turn and transformed canvas fill. Other
checks cover detach independence, transitive edits/delete/undo, whole-batch atomic errors,
locked and stale targets, cache budget rejection, duplicate IDs, nested cycles/depth/fanout,
master generations and colliding cross-document namespaces. The frozen committed v1 fixture
is consumed unchanged. Native tests compare exact ZIP/directory cache data and LayerId remap;
a simulated v1 reader rejects v2 as TooNew before content deserialization.

The panel and full app tests really click, drag and type. The app test opens the registered
Symbols tab, uses native canvas input, checks detached GPU previews and single release,
then master edit, logical zoom1/2/4/8, detach/undo and focus cancellation. It reads the real
app callback document texture; it does not label parser tests as interaction tests.

| Validation | Result / counts |
|---|---|
| Formatting | Owned engine/format/UI cargo fmt run; data-only helpers formatted with +1.98.1 rustfmt; whitespace check clean |
| Integrated checks | All seven affected crates and test targets compiled with the activation patch |
| Focused engine `symbol` tests | 14 passed, 0 failed, 0 ignored (13 CPU + strict RTX GPU) |
| Final changed copy-offset test | 1 passed, 0 failed; same existing cross-document test extended with prepublication rejection checks |
| Focused native `symbols` tests | 5 passed, 0 failed, 0 ignored; graph cause precedes deliberately missing tile payloads |
| Focused panel/full-app `symbols` tests | 3 panel/settings + 1 full-app RTX passed; 1 release-only measurement ignored in debug |
| Final full changed-crate tests, once | 3,662 passed, 5 failed, 36 ignored; four stale version assertions fixed and individually rerun successfully; one catalog coverage failure remains with 00 |
| Clippy all-targets -D warnings | Passed on all seven crates; final UI recheck after HDR coverage and engine recheck after copy-offset validation also passed |
| Release real-app pointer frames | 1 passed, 0 failed, 0 ignored; p50 2.865 ms, p95 3.933 ms, max 4.781 ms |
| Missing external/proprietary corpus | Affinity symbol files / external SVG symbol corpus absent; zero passed gates claimed |

The full run was performed once; only the four failed version tests and focused changed
symbol behavior/interaction checks were rerun. The fixtures were not regenerated. Four targeted
version reruns passed, leaving 3,666 distinct successful tests and one unresolved shared
translation-coverage gate. The original full-run counts by crate follow; resolved failures
remain visible here rather than being rewritten as a green full run.

| Crate | Passed | Failed in full run | Ignored | Follow-up |
|---|---:|---:|---:|---|
| pc-compose | 198 | 0 | 0 | — |
| pc-doc | 152 | 0 | 0 | — |
| pc-engine | 1,561 | 3 | 19 | Three version expectations corrected; all three exact tests passed |
| pc-format | 133 | 1 | 0 | Human-readable version expectation corrected; exact test passed |
| pc-gpu | 91 | 0 | 0 | — |
| pc-io | 422 | 0 | 2 | — |
| pc-ui-egui | 1,105 | 1 | 15 | Translation inventory/catalog merge belongs to 00 |

The translation gate reports Japanese first: 27 missing literals already occur in tracked
HEAD22cf5a12, and one new literal (`Filter for symbols`) is in the owned 21-key TSV.
Baseline comparison is recorded in `target/symbol-15/baseline-i18n.txt`; no gate exclusions,
catalog edits or English-as-translation entries were added. The shared activation patch
includes the four corrected version expectations and explicitly requests catalog completion.
36 default ignored tests include the new release-only measurement, which passed in its
separate release execution; the remaining 35 are existing baseline ignores.
Corpus features/proprietary Affinity and external SVG fixtures
were not supplied and are not counted as passed gates. The existing build warning about
missing embedded Chinese/Japanese fonts remains; the independent Latin text oracle passed.

Commands used offline Cargo/Rust1.98.1, CARGO_BUILD_JOBS=3, the shared compiler gate,
and one Cargo build at a time. Full logs, per-binary counts (`full-counts.json`), four exact
version rerun logs, GPU identity and timing output are under `target/symbol-15/`.
Clippy used the required `target/clippy` directory. No whole-workspace suite was run.
The final copy-offset regression and engine all-targets lint output are in
`copy-offset-final.log` and `clippy-engine-final.log`; both exited successfully.

GPU gates require RTX5090 / Vulkan / discrete / Rgba32Float, with premultiplied maximum
pixel difference ≤1/255, and contain no adapter skip. Both passing symbol GPU tests selected NVIDIA GeForce RTX5090, Vulkan, DiscreteGpu,
vendor4318/device11141, PCI0000:01:00.0, NVIDIA driver615.71.09, and asserted the required
Rgba32Float accumulator. Adapter absence/fallback fails the tests; neither test skips.
The separate release execution selected the same strict adapter and passed. It measured
real app pointer frames including GPU callbacks/readback: 20 warmup +128 samples,
128×96 badge scene, one master and two instances (one preview), zoom2. Results were
p50 **2.865 ms**, p95 **3.933 ms**, maximum **4.781 ms**. These figures describe this
fixture only; they do not establish large-artboard performance. The exact output is
`target/symbol-15/release-pointer-frames.log`.

The release command was:

```sh
PATH="$HOME/.cargo/bin:$PATH" CARGO_BUILD_JOBS=3 CARGO_TARGET_DIR="$PWD/target" \
  cargo +1.98.1 test --offline \
  --manifest-path target/symbol-15/worktree/Cargo.toml -p photocraft-ui-egui \
  --release --test gpu symbols_release_real_app_pointer_frames \
  -- --ignored --nocapture --test-threads=1
```

## Coordinated work still required

- Package00 applies the atomic shared patch, completes catalog coverage, and merges
  i18n and GPU aggregate registration in
  `wip/vector`. The actual package checkout intentionally does not self-edit those files.
- Package09 consumes expansion/master generations for crisp scaled geometry, including
  actual view/zoom/output-scale cache keys, regeneration of absent native caches, and
  shape/text tile-seam acceptance. The legacy
  app test verifies cache invalidation, not unmerged crisp rendering.
- Packages08/06/12/18–20 supply advanced ordinary appearance/corner/path-text/live projection.
  The baseline cache renderer rejects unmerged advanced content before publication.
- Packages13/14/21 consume the public exchange expansion for SVG/PDF mapping and round trips.
  Package23 integrates the symbol editability warning for the PSD cached pixel fallback.
- Final joint renderer/export acceptance cannot be claimed before those providers merge.

[Fixture and source/license record](vector-tasks/integration/15-sources.md) includes official
Affinity Symbols and W3C SVG semantics, the pinned VectorCraft behaviour reference, and
existing egui/app harness sources. This implementation and synthetic fixtures are original
GPL-3.0-or-later; no upstream file was copied/adapted and no new PORTS/attribution row or
brand asset is claimed. Existing notices are retained. No dependency or lock changes.
