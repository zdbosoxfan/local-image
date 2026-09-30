# Generation host integration contract

`types.ts` defines the exact `GenerationHost` interface. `controller.ts` owns Create/Edit/Draft/Final inputs, ordered references, recipes, accepted generated documents and refinement selection. `sizeMath.ts` now exports `sizeMath` plus `sizeLimits`, `fitDimensions` and `dimensionBounds`; pass this object directly. It has no dependency on the old DOM adapter.

## Context and operation ownership

- `getContext()` returns `{document, navigationEpoch, busy}` for the retained browser editor document. It supplies identity/revision and the explicit “Use current” reference/draft action; it is **not** authority to save, close or edit a hidden image. The typed adapter separately exposes `refining`, `creatingBlank`, `generationVisible` and `visibleDocumentId`. Every implicit document command uses those flags and `prepareDocumentCommand()` before reaching the retained session. Assets context has no document ID while the retained canvas is hidden.
- `documentController.ts` owns accepted metadata, navigation identity, busy reservations, close review and collection membership; `canvasController.ts` owns view/selection state. Session pixels, masks, pointer positions and history arrays never enter generation component state.
- `subscribeContext(listener)` subscribes to the editor/controller publication path and returns cleanup. It must not install a second global keyboard, pointer or native-message listener.
- `runGeneration(work)` delegates to `documentController.runDocumentChange`: reserve shared document ownership, await the stack-write queue, capture current accepted context inside the reservation, await `work(context)` including accepted preview navigation, then release only that reservation in `finally`. It does not retry work or create its own inference request. A rejection before the work callback starts is a known no-submission failure; it is not reported as an unknown running job.

The feature checks the invocation context again inside this reservation before submitting inference. It also checks the context before accepting late results. A transport failure can leave backend work running; the feature marks that outcome uncertain and requires an explicit backend-status check before another request. `dispose()` and stopping progress updates never abort or claim to cancel inference.

## Activation and document acceptance

`activateMode(mode, storedDocument)` is a direct editor operation, not a legacy tab click. It returns `true` only after the mode is accepted. Reject while another operation or close review owns the editor. Before changing the visible document, end/reset transient gestures and preserve the current document's view/selection through the core `rememberCurrentView()`/navigation path.

- **Edit:** activate the supplied accepted document. The generation controller binds it as reference one and preserves additional ordered references and the independent Edit draft.
- **Create with a stored result:** activate that document while retaining independent Edit state.
- **Blank Create:** show the blank generation view, keep the existing canvas mounted and its prior document recoverable, and disable image-edit/history/save commands that would target the hidden document.
- **Refine:** show the React refinement/comparison region. Keep the editor canvas mounted. Command routing uses the selected refinement result or draft when appropriate; it never silently saves, overwrites or exports the unrelated hidden document.

Advance the relevant navigation/view identity when the command target changes, including blank/refinement transitions. Apply application-owned view classes/state directly. Do not call `LocalImageGenerationStudio.activate`, manipulate legacy generation inputs, invoke legacy `showModal()` overrides, or keep hidden copies of `generation-panel`/`refine-dialog`.

`acceptResult(document, context, mode)` validates view identity, document identity and revision, then uses the core `openSession()` operation with its current raw navigation epoch. The adapter's view epoch also advances for blank/refinement transitions that retain the same underlying canvas document. `openSession()` saves/restores view and selection, respects newer accepted revisions, restores a real folder collection when one belongs to the document, and loads preview assets before committing navigation. Return `false` if the context changed or navigation did not accept the result.

Project restoration calls `controller.restoreDocument('edit', document)` through the explicit `afterDocumentOpened` hook. Its accepted Edit identity is tracked separately from the feature's context subscription, which can bind the new document before the hook runs. Reaccepting the same document preserves in-progress UI prompt edits. Close clears closed session references and leaves an empty Edit view; it never opens an unrelated stored Create result automatically.

The host must reserve per-document state correctly when `openSession()` updates `busy` internally. Saved project metadata and backend revisions remain authoritative; this adapter must not serialize a replacement project format.

## Other feature ports

- `openAssets('reference')` selects the active Create/Edit draft, or the Draft stage in Refine. Capture that draft/model identity so a late asset cannot attach to a different target. `addReference(key, document)` enforces capacity and preserves source attribution. `openAssets('draft')` hands an independent library working copy to `addDraft(document)`.
- `openModels({selectedModelId, selectedVariant, onUse})` connects to the Models controller directly. Its callback selects a model on the captured draft key.
- `openLoras(port)` passes `getLoraPort(key)`. The port's `read()` resolves live model, references and selected adapters; its setters modify only that draft. This is structurally compatible with `features/models/types.ts` and preserves `usage`/trigger metadata.
- Export, project save, source overwrite, unique copy, close review and native pickers remain shared editor/native commands. While blank Create or Refine hides the editor, their availability and target must come from the explicit visible-view state. Do not reintroduce the old click-interception or hidden-control proxy behavior.
- `backgroundTarget` in the context preserves the named cutout target per generation mode. Entering from a non-generated cutout captures its identity; closing that target clears it. The compact `Use as background` result action calls `applyGeneratedBackground(document, context)`, which checks view/source/target identity and delegates to the document controller's reservation. That controller reads the target's fresh revision and posts the existing `/cutout/generated-background` command, then accepts the target as Cutout. It uses the generated working session directly and still works when its optional library copy could not be saved; it never submits inference or retries a failed mutation.

`frontend/src/editor/generationAdapters.ts` implements the final typed ports without DOM or legacy globals. Create the adapter, pass `generationHost` and `assetsHost` into their feature controllers, then call `bind(generation, assets)` once. Merge `featureCommands` into `documentController.setFeatures()` with the other feature commands, since `setFeatures()` replaces the full command object. The application owns final body flags/layout and subscribes to `getSnapshot()`/`subscribe()`.

`generationAdapters.test.mjs` uses the actual Generation controller with controlled document/native ports to test visible-command routing, shared reservations, stale handoffs, reference/model identity, project restoration, closed-session cleanup and stable publications. It does not establish backend inference or native-dialog execution. Earlier composed-page evidence is in `tests/test_ui_react_generation.cjs`; final-shell browser evidence is recorded separately by the application integration tests.
