import type { EditorDocument } from '../contracts.ts';
import type { DocumentController } from './documentController.ts';
import type { DocumentContext, DocumentFeatureCommands, DocumentMetadata } from './documentContracts.ts';
import { readDocument } from './documentApi.ts';
import type { NativeBridge } from './nativeBridge.ts';
import type { AssetsController } from '../features/assets/controller.ts';
import type { AssetsContext, AssetsHost, BackgroundLibrary } from '../features/assets/types.ts';
import type { GenerationController } from '../features/generation/controller.ts';
import { GenerationApiError } from '../features/generation/api.ts';
import type {
  DraftKey,
  GenerationBackgroundTarget,
  GenerationContext,
  GenerationHost,
  GenerationMode,
} from '../features/generation/types.ts';
import { sizeMath } from '../features/generation/sizeMath.ts';
import type { SetupState } from '../features/settings/types.ts';
import { waitForSetupAction } from '../features/settings/setupAction.ts';

export type GenerationDocumentPort = Pick<
  DocumentController,
  | 'getContext'
  | 'getSnapshot'
  | 'subscribe'
  | 'runDocumentChange'
  | 'acceptDocument'
  | 'retainDocument'
  | 'applyGeneratedBackground'
  | 'applyGeneratedLayer'
  | 'selectGeneratedLayer'
  | 'openSession'
  | 'activateMode'
  | 'setGenerationView'
  | 'report'
  | 'rememberCurrentView'
  | 'isModalOpen'
>;
export interface GenerationAdaptersOptions {
  document: GenerationDocumentPort;
  native: Pick<NativeBridge, 'capabilities' | 'subscribe' | 'chooseBackgroundFolder'> &
    Partial<Pick<NativeBridge, 'setupEject'>>;
  openModels: GenerationHost['openModels'];
  openLoras: GenerationHost['openLoras'];
  setupStatus?: () => Promise<SetupState>;
}
export interface GenerationAdapterSnapshot {
  mode: GenerationMode;
  active: boolean;
  refining: boolean;
  creatingBlank: boolean;
  generationVisible: boolean;
  assetsOpen: boolean;
  busy: boolean;
  visibleDocumentId: string | null;
}

/** Typed feature orchestration only. No DOM, hidden controls, privileged paths,
 * transport listeners, inference retries or replacement document authority. */
export function createGenerationAdapters(options: GenerationAdaptersOptions) {
  const doc = options.document,
    native = options.native;
  let generation: GenerationController | null = null,
    assets: AssetsController | null = null;
  let mode: GenerationMode = 'create',
    assetsOpen = false,
    changing = false,
    preparing = false,
    ownedDepth = 0,
    internalNavigation = 0;
  let disposed = false,
    publishing = false,
    again = false,
    viewEpoch = 0,
    rawEpoch = doc.getContext().navigationEpoch;
  let generationSignature = '',
    assetsSignature = '',
    viewSignature = '',
    flagsSignature = '';
  let pendingDocument: { document: DocumentMetadata; epoch: number } | null = null;
  let editDocumentId: string | null = null;
  let returnToRefine: { documentId: string | null } | null = null;
  let refinementLayerSelection = '';
  const backgroundTargets: Record<GenerationMode, GenerationBackgroundTarget | null> = {
    create: null,
    edit: null,
    refine: null,
  };
  let unsubscribeGeneration: (() => void) | null = null;
  const subscribers = new Set<() => void>(),
    generationSubscribers = new Set<() => void>(),
    assetSubscribers = new Set<() => void>();
  let view: GenerationAdapterSnapshot = Object.freeze({
    mode,
    active: false,
    refining: false,
    creatingBlank: false,
    generationVisible: false,
    assetsOpen,
    busy: false,
    visibleDocumentId: null,
  });
  function raw(): DocumentContext {
    const value = doc.getContext();
    if (value.navigationEpoch !== rawEpoch) {
      rawEpoch = value.navigationEpoch;
      viewEpoch++;
    }
    return value;
  }
  const active = () => doc.getSnapshot().workspace === 'generate';
  function selectedDocument(): EditorDocument | null {
    return raw().document;
  }
  const hidden = () => active() && !selectedDocument();
  function generationContext(): GenerationContext {
    const value = raw();
    return {
      document: value.document,
      navigationEpoch: viewEpoch,
      busy: value.busy || changing || preparing,
      backgroundTarget: backgroundTargets[mode],
    };
  }
  function referencePort() {
    const key: DraftKey = mode === 'refine' ? 'draft' : mode;
    const model = generation?.modelFor(key),
      cap = model?.capabilities;
    const supported = !!(cap?.image_reference || cap?.image_to_image || cap?.references);
    return {
      key,
      target: active() && model ? `${key}:${model.id}` : null,
      canReference: active() && supported && (generation?.referencesFor(key).length ?? 0) < (cap?.max_references ?? 0),
    };
  }
  function assetsContext(): AssetsContext {
    const value = raw(),
      reference = referencePort(),
      visible = !hidden();
    return {
      documentId: visible ? value.documentId : null,
      revision: visible ? value.revision : 0,
      layerId: visible ? value.layerId : null,
      navigationEpoch: viewEpoch,
      busy: value.busy || changing || preparing || !!generation?.getSnapshot().working,
      nativeReady: native.capabilities().ready,
      canReference: reference.canReference,
      referenceTarget: reference.target,
      canUseDraft: !!generation && !generation.getSnapshot().working && !doc.getSnapshot().closeInProgress,
    };
  }
  function publish() {
    if (disposed) return;
    if (publishing) {
      again = true;
      return;
    }
    publishing = true;
    try {
      do {
        again = false;
        const context = raw(),
          isActive = active(),
          state = generation?.getSnapshot();
        const shown = selectedDocument();
        const selectedRefinement = state?.refinementStep === 'draft' ? state.selectedDraftId : state?.selectedResultId;
        if (
          isActive &&
          mode === 'refine' &&
          selectedRefinement &&
          selectedRefinement !== refinementLayerSelection &&
          !context.busy
        ) {
          refinementLayerSelection = selectedRefinement;
          doc.selectGeneratedLayer(selectedRefinement);
        }
        const flags = {
          refining: isActive && mode === 'refine',
          creatingBlank: isActive && !shown,
          visible: !!selectedDocument(),
        };
        const nextFlags = JSON.stringify(flags);
        if (nextFlags !== flagsSignature) {
          flagsSignature = nextFlags;
          doc.setGenerationView(flags);
        }
        const nextGeneration = JSON.stringify([
          context.documentId,
          context.revision,
          viewEpoch,
          context.busy,
          changing,
          preparing,
          backgroundTargets[mode],
        ]);
        if (nextGeneration !== generationSignature) {
          generationSignature = nextGeneration;
          for (const listener of [...generationSubscribers]) listener();
        }
        const assetContext = assetsContext(),
          nextAssets = JSON.stringify(assetContext);
        if (nextAssets !== assetsSignature) {
          assetsSignature = nextAssets;
          for (const listener of [...assetSubscribers]) listener();
        }
        const nextView: GenerationAdapterSnapshot = {
          mode,
          active: isActive,
          refining: flags.refining,
          creatingBlank: flags.creatingBlank,
          generationVisible: flags.visible,
          assetsOpen,
          busy: context.busy || changing || preparing || !!state?.working,
          visibleDocumentId: selectedDocument()?.id ?? null,
        };
        const nextViewSignature = JSON.stringify(nextView);
        if (nextViewSignature !== viewSignature) {
          viewSignature = nextViewSignature;
          view = Object.freeze(nextView);
          for (const listener of [...subscribers]) listener();
        }
      } while (again && !disposed);
    } finally {
      publishing = false;
    }
  }
  function generationMatches(captured: GenerationContext) {
    const current = generationContext();
    return (
      current.navigationEpoch === captured.navigationEpoch &&
      current.document?.id === captured.document?.id &&
      current.document?.revision === captured.document?.revision
    );
  }
  function assetsMatch(captured: AssetsContext) {
    const current = assetsContext();
    return (
      current.navigationEpoch === captured.navigationEpoch &&
      current.documentId === captured.documentId &&
      current.revision === captured.revision
    );
  }
  async function runOwned<T>(work: (context: DocumentContext) => Promise<T>): Promise<T> {
    const requested = raw(),
      requestedView = viewEpoch;
    return doc.runDocumentChange(async context => {
      raw();
      if (requestedView !== viewEpoch || requested.documentId !== context.documentId)
        throw Error('The active view changed before the operation started.');
      ownedDepth++;
      try {
        return await work(context);
      } finally {
        ownedDepth--;
        publish();
      }
    });
  }
  async function showDocument(
    document: EditorDocument,
    expectedRawEpoch: number,
    workspace: 'generate' | 'retouch' | 'cutout',
  ) {
    const value = readDocument(document);
    internalNavigation++;
    try {
      return await doc.openSession(value, {
        expectedNavigationEpoch: expectedRawEpoch,
        workspace,
        notifyFeatures: false,
      });
    } finally {
      internalNavigation--;
      publish();
    }
  }
  async function activateMode(next: GenerationMode, document: EditorDocument | null) {
    const context = raw();
    if (changing || doc.isModalOpen() || (context.busy && !ownedDepth && !internalNavigation && !preparing))
      return false;
    const entering = !active(),
      source = context.document;
    const target =
      source &&
      !source.generation &&
      !source.upscale &&
      (source.cutout?.enabled || source.layer_stack?.some(layer => layer.kind === 'cutout' && !layer.discarded))
        ? { id: source.id, name: source.name }
        : null;
    const previous = mode;
    changing = true;
    doc.rememberCurrentView();
    publish();
    try {
      if (
        document &&
        !context.document &&
        (context.documentId !== document.id || context.revision < document.revision)
      ) {
        if (!(await showDocument(document, context.navigationEpoch, 'generate'))) return false;
      } else if (!(await doc.activateMode('generate', null))) return false;
      if (entering && !source?.generation && !source?.upscale) {
        backgroundTargets[next] = target;
        if (target) for (const key of ['create', 'edit', 'refine'] as const) backgroundTargets[key] ??= target;
      }
      mode = next;
      if (next === 'edit') editDocumentId = document?.id ?? raw().documentId;
      viewEpoch++;
      publish();
      return true;
    } catch (error) {
      mode = previous;
      doc.report(error instanceof Error ? error.message : String(error), true);
      return false;
    } finally {
      changing = false;
      publish();
    }
  }
  async function acceptResult(document: EditorDocument, captured: GenerationContext, destination: GenerationMode) {
    if (!generationMatches(captured)) return false;
    const context = raw();
    if (context.document) {
      const accepted = await doc.applyGeneratedLayer(document.id, context);
      if (accepted) {
        mode = destination;
        if (mode === 'edit') editDocumentId = context.documentId;
        publish();
      }
      return accepted;
    }
    const accepted = await showDocument(document, raw().navigationEpoch, 'generate');
    if (accepted) {
      mode = destination;
      if (mode === 'edit') editDocumentId = document.id;
      publish();
    }
    return accepted;
  }
  async function enterGenerate() {
    if (!generation || raw().busy || changing) return false;
    if (active()) return true;
    if (returnToRefine?.documentId === raw().documentId && generation.getSnapshot().mode === 'refine') {
      const resumed = await generation.setMode('refine');
      if (resumed) returnToRefine = null;
      return resumed;
    }
    returnToRefine = null;
    const document = raw().document;
    return generation.setMode(document ? 'edit' : 'create', document ?? undefined);
  }
  async function leaveGenerate(workspace: 'retouch' | 'cutout') {
    if (
      !generation ||
      !active() ||
      raw().busy ||
      changing ||
      preparing ||
      generation.getSnapshot().working ||
      doc.isModalOpen()
    )
      return false;
    const context = raw(),
      selected = selectedDocument(),
      previousMode = mode;
    changing = true;
    doc.rememberCurrentView();
    publish();
    try {
      const accepted =
        selected && (context.documentId !== selected.id || context.revision < selected.revision)
          ? await showDocument(selected, context.navigationEpoch, workspace)
          : await doc.activateMode(workspace, null);
      if (accepted && previousMode === 'refine') returnToRefine = { documentId: raw().documentId };
      return accepted;
    } catch (error) {
      doc.report(error instanceof Error ? error.message : String(error), true);
      return false;
    } finally {
      changing = false;
      publish();
    }
  }
  async function afterDocumentOpened(document: DocumentMetadata) {
    if (internalNavigation || !generation) return;
    returnToRefine = null;
    const context = raw();
    if (generation.getSnapshot().working) {
      pendingDocument = { document, epoch: context.navigationEpoch };
      return;
    }
    const savedGeneration =
      document.generation ?? document.layer_stack?.find(layer => layer.id === context.layerId)?.generation;
    if (savedGeneration && editDocumentId !== document.id)
      generation.restoreDocument('edit', { ...document, generation: savedGeneration });
    if (active()) {
      internalNavigation++;
      // Context subscriptions can already bind the new image before this hook.
      // Track the explicit accepted Edit identity independently of that binding.
      try {
        await generation.setMode('edit', document);
      } finally {
        internalNavigation--;
      }
    }
    editDocumentId = document.id;
    publish();
  }
  async function prepareDocumentCommand(kind: 'save' | 'project' | 'close' | 'credits') {
    if (!hidden()) return { allowed: true, forceExport: false };
    if (!generation || raw().busy || changing || preparing || generation.getSnapshot().working)
      return { allowed: false, forceExport: false };
    const selected = mode === 'refine' ? selectedDocument() : null;
    if (!selected) {
      doc.report(
        `Generate or select an image before ${kind === 'close' ? 'closing an image' : 'saving or exporting'}. The retained editor document is preserved.`,
      );
      return { allowed: false, forceExport: false };
    }
    preparing = true;
    publish();
    try {
      const accepted = await acceptResult(selected, generationContext(), 'edit');
      if (accepted) await generation.setMode('edit', selected);
      return { allowed: accepted, forceExport: accepted };
    } finally {
      preparing = false;
      publish();
    }
  }
  function showAssets(destination: 'image' | 'background' | 'reference' | 'draft' = 'image') {
    return assets?.open(destination === 'draft' ? 'generated' : destination === 'background' ? 'folders' : 'stock');
  }
  const generationHost: GenerationHost = {
    getContext: generationContext,
    subscribeContext: listener => {
      generationSubscribers.add(listener);
      return () => {
        generationSubscribers.delete(listener);
      };
    },
    async runGeneration(work) {
      let started = false;
      try {
        return await runOwned(context => {
          started = true;
          return work({ document: context.document, navigationEpoch: viewEpoch, busy: context.busy });
        });
      } catch (error) {
        if (!started)
          throw new GenerationApiError(
            `${error instanceof Error ? error.message : String(error)} No image operation was submitted.`,
            409,
          );
        throw error;
      }
    },
    acceptResult,
    activateMode,
    openAssets: destination => {
      void showAssets(destination);
    },
    openModels: options.openModels,
    openLoras: options.openLoras,
    sizeMath,
    canEjectModels: () => !!native.capabilities().ready && !!native.capabilities().setup && !!native.setupEject,
    ejectModels: () =>
      runOwned(async () => {
        if (!native.capabilities().ready || !native.capabilities().setup || !native.setupEject)
          throw Error('Open the Local Image desktop app to unload GPU models.');
        const initial = await native.setupEject();
        if (!initial) throw Error('The GPU unload request was not accepted.');
        if (!options.setupStatus && (initial as SetupState).job?.status === 'running')
          throw Error('GPU unload is pending. Check Settings for its outcome.');
        return waitForSetupAction(initial as SetupState, options.setupStatus ?? (async () => initial as SetupState), {
          active: () => !disposed,
        });
      }),
    async applyGeneratedBackground(document, captured) {
      const target = backgroundTargets[mode];
      if (
        !target ||
        target.id !== captured.backgroundTarget?.id ||
        target.id === document.id ||
        !generationMatches(captured) ||
        !(document.generation || document.upscale)
      )
        return false;
      const current = raw();
      internalNavigation++;
      try {
        return await doc.applyGeneratedBackground(document.id, target.id, {
          documentId: current.documentId,
          navigationEpoch: current.navigationEpoch,
          revision: current.revision,
        });
      } finally {
        internalNavigation--;
        publish();
      }
    },
  };
  const assetsHost: AssetsHost = {
    getContext: assetsContext,
    subscribeContext: listener => {
      assetSubscribers.add(listener);
      return () => {
        assetSubscribers.delete(listener);
      };
    },
    runDocumentChange: work => runOwned(() => work(assetsContext())),
    async acceptDocument(document, captured, destination) {
      if (!assetsMatch(captured)) return false;
      const value = readDocument(document),
        current = raw();
      if (destination === 'background' && (value.id !== captured.documentId || value.revision < captured.revision))
        return false;
      internalNavigation++;
      let accepted: boolean;
      try {
        accepted = await doc.acceptDocument(
          value,
          { navigationEpoch: current.navigationEpoch, documentId: captured.documentId, revision: captured.revision },
          destination,
        );
      } finally {
        internalNavigation--;
      }
      if (!accepted) return false;
      if (destination === 'background') await doc.activateMode('cutout', null);
      else if (destination === 'generated' && generation) {
        if (value.generation) generation.restoreDocument('edit', value);
        await generation.setMode('edit', value);
      } else await doc.activateMode('retouch', null);
      publish();
      return true;
    },
    async addReference(document, captured) {
      const port = referencePort();
      return (
        !!generation &&
        assetsMatch(captured) &&
        captured.referenceTarget === port.target &&
        port.canReference &&
        generation.addReference(port.key, document)
      );
    },
    async useAsDraft(document, captured) {
      if (!generation || !assetsMatch(captured)) return false;
      generation.addDraft(document);
      return generation.setMode('refine');
    },
    async chooseBackgroundFolder() {
      const value = await native.chooseBackgroundFolder();
      if (!value) return null;
      if (
        typeof value.id !== 'string' ||
        typeof value.name !== 'string' ||
        !Array.isArray(value.entries) ||
        value.entries.some(
          entry =>
            !entry ||
            typeof entry.id !== 'string' ||
            typeof entry.name !== 'string' ||
            typeof entry.thumbnail !== 'string',
        )
      )
        throw Error('The native background folder response was invalid.');
      return {
        id: value.id,
        name: value.name,
        entries: value.entries.map(entry => ({ id: entry.id, name: entry.name, thumbnail: entry.thumbnail })),
      } as BackgroundLibrary;
    },
    setDockVisible(value) {
      assetsOpen = value;
      publish();
    },
  };
  const featureCommands: Pick<
    DocumentFeatureCommands,
    | 'prepareDocumentCommand'
    | 'afterDocumentOpened'
    | 'noteClosed'
    | 'enterGenerate'
    | 'leaveGenerate'
    | 'resetWorkspace'
    | 'visibleDocumentId'
    | 'showAssets'
    | 'showGenerated'
    | 'browseModels'
  > = {
    prepareDocumentCommand,
    afterDocumentOpened,
    visibleDocumentId: () => selectedDocument()?.id ?? null,
    resetWorkspace() {
      returnToRefine = null;
      pendingDocument = null;
      editDocumentId = null;
      for (const key of ['create', 'edit', 'refine'] as const) backgroundTargets[key] = null;
      mode = 'create';
      viewEpoch++;
      generation?.resetWorkspace();
      publish();
    },
    noteClosed(ids) {
      refinementLayerSelection = '';
      if (returnToRefine?.documentId && ids.includes(returnToRefine.documentId)) returnToRefine = null;
      if (editDocumentId && ids.includes(editDocumentId)) editDocumentId = null;
      for (const key of ['create', 'edit', 'refine'] as const)
        if (backgroundTargets[key] && ids.includes(backgroundTargets[key]!.id)) backgroundTargets[key] = null;
      generation?.forgetDocuments([...ids]);
      publish();
    },
    enterGenerate,
    leaveGenerate,
    showAssets,
    showGenerated: () => assets?.open('generated'),
    async browseModels() {
      if (!generation) return;
      await enterGenerate();
      generation.openModels(generation.activeDraftKey());
    },
  };
  const unsubscribeDocument = doc.subscribe(publish),
    unsubscribeNative = native.subscribe(publish);
  publish();
  return {
    generationHost,
    assetsHost,
    featureCommands,
    getSnapshot: () => view,
    subscribe(listener: () => void) {
      subscribers.add(listener);
      return () => {
        subscribers.delete(listener);
      };
    },
    bind(generationController: GenerationController, assetsController: AssetsController) {
      if (generation || assets) throw Error('Generation adapters already have feature owners.');
      generation = generationController;
      assets = assetsController;
      unsubscribeGeneration = generation.subscribe(() => {
        if (pendingDocument && !generation!.getSnapshot().working) {
          const pending = pendingDocument;
          pendingDocument = null;
          if (raw().navigationEpoch === pending.epoch && raw().documentId === pending.document.id)
            void afterDocumentOpened(pending.document);
        }
        publish();
      });
      publish();
    },
    dispose() {
      disposed = true;
      unsubscribeDocument();
      unsubscribeNative();
      unsubscribeGeneration?.();
      subscribers.clear();
      generationSubscribers.clear();
      assetSubscribers.clear();
    },
  };
}
export type GenerationAdapters = ReturnType<typeof createGenerationAdapters>;
