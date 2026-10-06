import type { LayerPatch, Transform, Workspace } from '../contracts.ts';
import type { CanvasDisplay, CanvasInteraction, CanvasSnapshot, TransformCommit } from './canvasContracts.ts';
import type { Tool, ToolActions } from '../features/shell/ToolControls.tsx';
import type { DocumentChromeActions } from '../features/shell/DocumentChrome.tsx';
import type { AcceptedConfiguration } from '../features/settings/types.ts';
import { createDocumentApi, readCollection, readDocument, type DocumentApi } from './documentApi.ts';
import {
  collectionForDisplay,
  creditText,
  deepFreeze,
  documentCredits,
  forgetCollectionDocuments,
  hasWorkingLayers,
  imageDirty,
  projectNeedsSave,
  safeDownload,
  supportedImage,
  withCollectionDocument,
} from './documentLogic.ts';
import type {
  CollectionEntry,
  DocumentContext,
  DocumentControllerPorts,
  DocumentFeatureCommands,
  DocumentHealth,
  DocumentLayer,
  DocumentMetadata,
  DocumentSnapshot,
  ImageCollection,
  NativeOpenResult,
  OutputFormat,
  SaveMode,
  SaveOutcome,
  SaveResult,
} from './documentContracts.ts';

const identity = (): Transform => ({ offset_x: 0, offset_y: 0, scale: 1, rotation: 0 });
const formats = new Set<OutputFormat>(['original', 'png', 'jpg', 'tif', 'webp']);
const failureText = (error: unknown) => (error instanceof Error ? error.message : String(error));
const emptyHealth = (): DocumentHealth => ({
  settingsLoaded: false,
  ready: false,
  retouchReady: false,
  device: '',
  models: [],
  healMethods: [
    { id: 'texture', label: 'Texture repair' },
    { id: 'telea', label: 'Dust & scratches' },
  ],
  qwen: {
    ready: false,
    connected: false,
    variants: [
      { id: 'int8', label: 'Compact INT8', available: false },
      { id: 'bf16', label: 'Full BF16', available: false },
    ],
  },
});

/** Backend-authoritative document metadata, collection/navigation and explicit
 * editing commands. Canvas pixels/history live only in canvasController; native
 * filesystem authority remains only in the fixed native bridge. No DOM control
 * reads, synthetic control clicks, duplicate keyboard/native listeners, or
 * frontend project serialization occur here. */
export function createDocumentController(options: DocumentControllerPorts & { token: string; api?: DocumentApi }) {
  const canvas = options.canvas,
    api = options.api ?? createDocumentApi(options.token),
    native = options.native,
    browser = options.browser,
    dialogs = options.dialogs;
  const listeners = new Set<() => void>(),
    accepted = new Map<string, DocumentMetadata>(),
    acceptedOrder = new Map<string, number>(),
    opened = new Set<string>(),
    selected = new Map<string, string>();
  const operations = new Map<number, string>(),
    files = new Map<string, File>(),
    objectUrls = new Set<string>();
  const bases = new Map<string, Promise<HTMLImageElement>>(),
    displays = new Map<string, Promise<CanvasDisplay>>();
  let sequence = 0,
    navigationEpoch = 0,
    activeId: string | null = null,
    collection: ImageCollection | null = null,
    collectionIndex = -1;
  let navigationBusy: number | null = null,
    pendingDisplay: { epoch: number; document: DocumentMetadata } | null = null;
  let closeInProgress = false,
    disposed = false,
    publishing = false,
    suppressCanvas = false,
    health = emptyHealth(),
    healthVersion = 0;
  let status = '',
    statusError = false,
    recent: DocumentMetadata[] = [],
    inspectorHidden = false,
    filmstripCollapsed = false,
    thumbnailSize = 72;
  let settingsPending = false,
    menuOpen = false,
    externalOperation = false;
  let features: DocumentFeatureCommands = options.features ?? {},
    generationView = { refining: false, creatingBlank: false, visible: false };
  const readPreference = (key: string, fallback: string) => {
    try {
      return options.storage?.getItem(key) ?? fallback;
    } catch {
      return fallback;
    }
  };
  const store = (key: string, value: string) => {
    try {
      options.storage?.setItem(key, value);
    } catch {
      /* Storage preference does not decide document success. */
    }
  };
  let outputFormat = readPreference('local-remove-copy-format', 'original') as OutputFormat;
  if (!formats.has(outputFormat)) outputFormat = 'original';
  let askBeforeOverwrite = readPreference('local-remove-ask-before-overwrite', 'true') !== 'false';
  let healMethod = readPreference('local-remove-heal-method', 'texture');
  if (!['texture', 'telea'].includes(healMethod)) healMethod = 'texture';
  let aiProvider: 'klein' | 'qwen' = 'klein';
  let qwenVariant = readPreference('local-remove-qwen-variant', 'int8');
  if (!['int8', 'bf16'].includes(qwenVariant)) qwenVariant = 'int8';
  let interaction: CanvasInteraction = {
    tool: 'brush',
    workspace: 'retouch',
    operation: readPreference('local-remove-operation', 'heal') === 'ai' ? 'ai' : 'heal',
    cutoutOperation: 'erase',
    subtract: false,
    brushSize: 28,
    handActive: false,
    showOriginal: false,
    busy: false,
    selectedLayerId: null,
  };
  let snapshot: DocumentSnapshot;
  const current = () => (activeId ? (accepted.get(activeId) ?? null) : null);
  const busy = () =>
    navigationBusy === navigationEpoch ||
    operations.size > 0 ||
    closeInProgress ||
    settingsPending ||
    canvas.getSnapshot().historyBusy;
  const modal = () => dialogs.isOpen() || !!options.isModalOpen?.();
  function selectedLayer(data = current()): DocumentLayer | null {
    return (
      data?.layer_stack?.find(
        layer => layer.id === (selected.get(data.id) ?? data.selected_layer_id) && !layer.discarded,
      ) ??
      [...(data?.layer_stack ?? [])].reverse().find(layer => !layer.discarded) ??
      null
    );
  }
  const editing = () =>
    !!current() && interaction.workspace !== 'generate' && !generationView.refining && !generationView.creatingBlank;
  const editable = () => editing() && !busy() && !interaction.showOriginal;
  const writableLayer = () => {
    const layer = selectedLayer();
    return !!layer?.visible && !layer.discarded && !layer.locked;
  };
  function historyTarget(redo = false): 'selection' | 'stack' | 'cutout' | null {
    if (!editing() || busy() || interaction.showOriginal || modal()) return null;
    const view = canvas.getSnapshot();
    if (redo ? view.canRedoSelection : view.canUndoSelection) return 'selection';
    const doc = current();
    if (doc?.layer_stack && (redo ? doc.stack_can_redo : doc.stack_can_undo)) return 'stack';
    if (interaction.workspace === 'cutout' && (redo ? doc?.cutout_can_redo : doc?.cutout_can_undo)) return 'cutout';
    return null;
  }
  const qwenReady = () =>
    health.qwen.ready && health.qwen.variants.some(value => value.id === qwenVariant && value.available);
  const operationReady = () =>
    interaction.operation === 'heal'
      ? health.retouchReady && health.healMethods.find(method => method.id === healMethod)?.available !== false
      : aiProvider === 'qwen'
        ? qwenReady()
        : health.settingsLoaded &&
          health.ready &&
          health.models.find(model => model.id === 'klein')?.available !== false;
  function publish() {
    if (disposed || publishing) return;
    publishing = true;
    try {
      const doc = current(),
        layer = selectedLayer(doc),
        view = canvas.getSnapshot(),
        locked = busy();
      const nativeCaps = native?.capabilities();
      const maskReady = layer?.kind === 'cutout',
        canEdit = !!doc && !locked && !interaction.showOriginal && interaction.workspace !== 'generate';
      const canApply =
        canEdit &&
        view.hasSelection &&
        !view.pointCount &&
        (interaction.workspace === 'cutout'
          ? maskReady && writableLayer()
          : operationReady() && (layer?.kind !== 'retouch' || writableLayer()));
      const applyLabel =
        interaction.workspace === 'cutout'
          ? interaction.cutoutOperation === 'restore'
            ? 'Restore selection'
            : 'Erase selection'
          : interaction.operation === 'heal'
            ? 'Heal'
            : 'Remove';
      const credits = documentCredits(doc),
        undoTarget = historyTarget(),
        redoTarget = historyTarget(true);
      const toolHint = !doc
        ? 'Open an image to begin'
        : interaction.handActive
          ? 'Drag to pan the image'
          : interaction.tool === 'move'
            ? 'Drag the layer; use its handles to scale or rotate'
            : interaction.tool === 'pen'
              ? 'Place points; Enter closes the path'
              : interaction.workspace === 'cutout'
                ? 'Select pixels to erase or restore in the selected mask'
                : interaction.operation === 'heal'
                  ? 'Select a blemish, then apply Quick Heal'
                  : 'Select an object, then apply removal';
      const tools = {
        workspace: interaction.workspace,
        tool: interaction.tool,
        handActive: interaction.handActive,
        operation: interaction.operation as 'heal' | 'ai',
        busy: locked,
        hasDocument: !!doc,
        showOriginal: interaction.showOriginal,
        canEdit,
        brushSize: interaction.brushSize,
        subtract: interaction.subtract,
        canFinish: canEdit && view.pointCount >= 3,
        selectionActive: view.hasSelection || view.pointCount > 0,
        canApply,
        applyLabel,
        healMethod,
        healMethods: health.healMethods,
        aiProvider,
        qwenVariant,
        qwenVariants: health.qwen.variants,
        maskReady,
        canRemoveBackground: canEdit && qwenReady() && !!layer && layer.visible,
        cutoutOperation: interaction.cutoutOperation,
        transform: layer ? { ...identity(), ...layer.transform } : null,
        canTransform: canEdit && writableLayer(),
      };
      const chrome = {
        id: doc?.id ?? null,
        name: doc?.name ?? '',
        width: doc?.width ?? 0,
        height: doc?.height ?? 0,
        bitDepth: doc?.bit_depth ?? 8,
        busy: locked,
        dirty: doc ? imageDirty(doc) : false,
        projectDirty: doc ? projectNeedsSave(doc) : false,
        selectionPending: tools.selectionActive,
        canReturn: !!doc?.can_return,
        showOriginal: interaction.showOriginal,
        zoom: view.photoZoom,
        fit: view.fitMode,
        status,
        error: statusError,
        toolHint,
      };
      snapshot = deepFreeze({
        document: doc,
        selectedLayerId: layer?.id ?? null,
        navigationEpoch,
        busy: locked,
        workspace: interaction.workspace,
        showOriginal: interaction.showOriginal,
        tool: interaction.tool,
        canUndo: !!undoTarget,
        canRedo: !!redoTarget,
        status,
        statusError,
        nativeReady: !!nativeCaps?.ready,
        nativeProjects: !!nativeCaps?.projects,
        selectionActive: tools.selectionActive,
        refining: generationView.refining,
        creatingBlank: generationView.creatingBlank,
        generationVisible: interaction.workspace === 'generate' ? generationView.visible : !!doc,
        inspectorHidden,
        tools,
        chrome,
        collection: collectionForDisplay(collection, accepted),
        collectionIndex,
        filmstripCollapsed,
        thumbnailSize,
        recentSessions: recent,
        openDocuments: [...opened].map(id => accepted.get(id)!).filter(Boolean),
        outputFormat,
        askBeforeOverwrite,
        closeInProgress,
        operationLabel:
          [...operations.values()].at(-1) ?? (navigationBusy === navigationEpoch ? 'Opening image' : null),
        health: structuredClone(health),
        penPointCount: view.pointCount,
        canApplySelection: canApply,
        selectionActionLabel: applyLabel,
        undoLabel: undoTarget === 'selection' ? 'Undo selection' : undoTarget ? 'Undo layer change' : 'Undo',
        redoLabel: redoTarget === 'selection' ? 'Redo selection' : redoTarget ? 'Redo layer change' : 'Redo',
        creditsAvailable: credits.length > 0,
        canvas: { ...view },
      });
      canvas.setInteractionState({ ...interaction, busy: locked, selectedLayerId: layer?.id ?? null });
    } finally {
      publishing = false;
    }
    for (const listener of listeners) listener();
  }
  function report(text: string, error = false) {
    status = text;
    statusError = error;
    publish();
  }
  function startOperation(label: string) {
    const id = ++sequence;
    operations.set(id, label);
    canvas.cancelGesture();
    publish();
    return () => {
      operations.delete(id);
      publish();
    };
  }
  function accept(data: DocumentMetadata, order = ++sequence, track = true) {
    const doc = readDocument(data),
      prior = accepted.get(doc.id);
    if (
      prior &&
      (doc.revision < prior.revision || (doc.revision === prior.revision && order < (acceptedOrder.get(doc.id) ?? -1)))
    )
      return prior;
    const value = deepFreeze(structuredClone(doc));
    accepted.set(doc.id, value);
    acceptedOrder.set(doc.id, order);
    api.observe(value);
    if (track) opened.add(doc.id);
    collection = withCollectionDocument(collection, value);
    const layer = selectedLayer(value);
    if (layer) selected.set(value.id, layer.id);
    return value;
  }
  function getAcceptedDocument() {
    return pendingDisplay?.epoch === navigationEpoch ? pendingDisplay.document : current();
  }
  function trimDisplayCache() {
    while (displays.size > 8) displays.delete(displays.keys().next().value!);
    for (const key of bases.keys()) {
      if (bases.size <= 4) break;
      if (key !== activeId) bases.delete(key);
    }
  }
  async function displayFor(data: DocumentMetadata): Promise<CanvasDisplay> {
    const key = data.id + ':' + data.revision;
    let pending = displays.get(key);
    if (!pending) {
      let base = bases.get(data.id);
      if (!base) {
        base = options.loadImage('/api/local-remove/session/' + encodeURIComponent(data.id) + '/base-display');
        bases.set(data.id, base);
        void base.catch(() => bases.delete(data.id));
      }
      pending = Promise.all([
        base,
        options.loadImage(
          '/api/local-remove/session/' + encodeURIComponent(data.id) + '/preview?full=true&revision=' + data.revision,
        ),
      ]).then(([original, composite]) => ({ document: data, original, composite }));
      displays.set(key, pending);
      void pending.catch(() => displays.delete(key));
    }
    return pending;
  }
  async function present(
    data: DocumentMetadata,
    epoch: number,
    nextCollection = collection,
    index = collectionIndex,
    newNavigation = false,
  ) {
    let candidate = data;
    const newer = accepted.get(candidate.id);
    if (newer && newer.revision >= candidate.revision) candidate = newer;
    const display = await displayFor(candidate);
    if (disposed || epoch !== navigationEpoch) return false;
    const latest = accepted.get(candidate.id);
    if (latest && latest.revision > candidate.revision)
      return present(latest, epoch, nextCollection, index, newNavigation);
    pendingDisplay = { epoch, document: candidate };
    suppressCanvas = true;
    try {
      const layer = selectedLayer(candidate);
      const okay = await canvas.presentDocument({
        ...display,
        interaction: { ...interaction, busy: busy(), selectedLayerId: layer?.id ?? null },
      });
      if (!okay || disposed || epoch !== navigationEpoch) return false;
      activeId = candidate.id;
      accept(candidate);
      collection = withCollectionDocument(nextCollection, candidate);
      collectionIndex = nextCollection ? index : -1;
      if (newNavigation)
        browser.replaceUrl?.(
          collection && !collection.local
            ? '/remove?collection=' + encodeURIComponent(collection.id) + '&index=' + collectionIndex
            : '/remove?session=' + encodeURIComponent(candidate.id),
        );
      trimDisplayCache();
      return true;
    } finally {
      if (pendingDisplay?.epoch === epoch) pendingDisplay = null;
      suppressCanvas = false;
      publish();
    }
  }
  async function refreshDocument(data: DocumentMetadata, epoch: number) {
    const value = accept(data);
    if (activeId === value.id && epoch === navigationEpoch) await present(value, epoch);
    publish();
    return value;
  }
  async function openSession(
    value: DocumentMetadata,
    settings: {
      collection?: ImageCollection | null;
      index?: number;
      expectedNavigationEpoch?: number;
      workspace?: Workspace;
      notifyFeatures?: boolean;
    } = {},
  ) {
    if (
      disposed ||
      (settings.expectedNavigationEpoch !== undefined && settings.expectedNavigationEpoch !== navigationEpoch)
    )
      return false;
    const epoch = ++navigationEpoch;
    navigationBusy = epoch;
    canvas.rememberCurrentView();
    canvas.cancelGesture();
    report('Opening ' + value.name + '…');
    const oldInteraction = { ...interaction };
    try {
      let data = readDocument(value);
      await api.flush(data.id);
      const tracked = accepted.get(data.id);
      if (tracked && tracked.revision >= data.revision) data = tracked;
      api.observe(data);
      if (!data.layer_stack) {
        data = await api.mutate(data, '/stack');
        // Stack scaffolding intentionally keeps the revision unchanged. Its
        // newly accepted metadata must supersede an equally-versioned import
        // already observed by an Assets/Generation handoff before presentation.
        data = accept(data, ++sequence, false);
      }
      if (epoch !== navigationEpoch) return false;
      let nextCollection = settings.collection;
      if (nextCollection === undefined && data.collection_id)
        nextCollection =
          collection?.id === data.collection_id
            ? collection
            : await api.collection(data.collection_id).catch(() => null);
      nextCollection ??= null;
      const nextIndex = nextCollection
        ? (settings.index ??
          nextCollection.entries.findIndex(entry => entry.id === data.entry_id || entry.session_id === data.id))
        : -1;
      if (settings.workspace) interaction.workspace = settings.workspace;
      else if (!opened.has(data.id))
        interaction.workspace =
          data.generation || data.upscale ? 'generate' : data.cutout?.enabled ? 'cutout' : 'retouch';
      if (!opened.has(data.id)) interaction.showOriginal = false;
      const shown = await present(data, epoch, nextCollection, nextIndex, true);
      if (!shown) {
        if (epoch === navigationEpoch) interaction = oldInteraction;
        return false;
      }
      // Restoring a cached canvas view restores its tool/workspace preferences.
      // An explicit handoff destination takes precedence over that saved mode.
      if (epoch === navigationEpoch && settings.workspace && interaction.workspace !== settings.workspace)
        setInteraction({ workspace: settings.workspace }, true);
      if (epoch === navigationEpoch)
        report(
          data.project_name
            ? 'Project opened. Layers remain editable.'
            : data.can_return
              ? 'Ready. Changes remain in this session until saved.'
              : 'Browser upload copy. Export downloads a finished image.',
        );
      void refreshRecent();
      if (settings.notifyFeatures !== false && epoch === navigationEpoch)
        await features.afterDocumentOpened?.(current()!);
      return true;
    } catch (error) {
      if (epoch === navigationEpoch) {
        interaction = oldInteraction;
        report('Could not open image: ' + failureText(error), true);
      }
      return false;
    } finally {
      if (navigationBusy === epoch) navigationBusy = null;
      publish();
    }
  }
  async function refreshRecent() {
    const order = ++sequence;
    try {
      const values = await api.sessions();
      if (!disposed && order >= (recentOrder || 0)) {
        recentOrder = order;
        recent = values.slice(0, 6).map(value => deepFreeze(structuredClone(value)));
        publish();
      }
    } catch {
      /* Recovery documents already open remain usable. */
    }
  }
  let recentOrder = 0;
  const fileKey = (cid: string, eid: string) => cid + '\0' + eid;
  async function resolveEntry(source: ImageCollection, entry: CollectionEntry) {
    if (entry.session_id) return api.session(entry.session_id);
    const file = files.get(fileKey(source.id, entry.id));
    if (!file) throw Error('Open this photo again before continuing.');
    const data = await api.importImage(file);
    entry.session_id = data.id;
    if (collection?.id === source.id)
      collection = {
        ...collection,
        entries: collection.entries.map(item => (item.id === entry.id ? { ...item, session_id: data.id } : item)),
      };
    return data;
  }
  async function openCollectionEntry(index: number, source = collection) {
    if (
      !source ||
      index < 0 ||
      index >= source.entries.length ||
      closeInProgress ||
      externalOperation ||
      operations.size > 0
    )
      return false;
    const entry = source.entries[index];
    if (source.id === collection?.id && collectionIndex === index && current()?.id === entry.session_id) return true;
    const requestEpoch = ++navigationEpoch;
    navigationBusy = requestEpoch;
    canvas.rememberCurrentView();
    report('Opening ' + entry.name + '…');
    try {
      const result = source.local
        ? { session: await resolveEntry(source, entry), collection: source, index }
        : await api.openEntry(source.id, entry.id);
      if (requestEpoch !== navigationEpoch || !result.session) return false;
      navigationBusy = null;
      return openSession(result.session, {
        collection: result.collection ?? source,
        index: result.index ?? index,
        expectedNavigationEpoch: requestEpoch,
      });
    } catch (error) {
      if (requestEpoch === navigationEpoch) report('Could not open ' + entry.name + ': ' + failureText(error), true);
      return false;
    } finally {
      if (navigationBusy === requestEpoch) navigationBusy = null;
      publish();
    }
  }
  async function openCollection(value: ImageCollection, index = 0) {
    const source = structuredClone(readCollection(value));
    if (!source.entries.length) {
      report('This folder contains no supported images.', true);
      return false;
    }
    return openCollectionEntry(Math.max(0, Math.min(source.entries.length - 1, index)), source);
  }
  async function openBrowserFiles(values: readonly File[]) {
    if (busy() || modal()) return false;
    if (values.length === 1 && /\.lremove$/i.test(values[0].name)) return importProject(values[0]);
    const images = values.filter(supportedImage);
    if (!images.length) {
      report('Choose JPEG, PNG, TIFF, WebP or an editable .lremove project.', true);
      return false;
    }
    const cid = 'uploads-' + Date.now() + '-' + ++sequence,
      source: ImageCollection = {
        id: cid,
        name:
          images[0].webkitRelativePath?.split('/')[0] ||
          (images.length === 1 ? 'Browser upload copy' : 'Browser upload copies'),
        local: true,
        entries: [],
      };
    images.forEach((file, index) => {
      const id = String(index),
        thumbnail = browser.createObjectURL?.(file) ?? null;
      files.set(fileKey(cid, id), file);
      if (thumbnail) objectUrls.add(thumbnail);
      source.entries.push({ id, name: file.name, session_id: null, thumbnail, dirty: false, project_dirty: false });
    });
    return openCollection(source);
  }
  async function importProject(file: File) {
    if (busy() || modal()) return false;
    const finish = startOperation('Opening editable project');
    let result: NativeOpenResult | null = null;
    try {
      result = await api.importProject(file);
    } catch (error) {
      report('Could not open project: ' + failureText(error), true);
    } finally {
      finish();
    }
    return result?.session ? openSession(result.session) : false;
  }
  async function applyNativeResult(value: unknown) {
    if (!value) return false;
    const result = value as NativeOpenResult;
    if (result.collection) return openCollection(readCollection(result.collection), result.index ?? 0);
    if (result.session) return openSession(readDocument(result.session));
    throw Error('The desktop did not return an image or project.');
  }
  async function openNative(kind: 'openFiles' | 'openFolder' | 'openProject' | 'drop', dropped?: readonly File[]) {
    if (!native || busy() || modal()) return false;
    const finish = startOperation('Choosing files');
    let result: unknown;
    try {
      await api.flush();
      result = kind === 'drop' ? await native.drop([...(dropped ?? [])]) : await native[kind]();
    } catch (error) {
      report(failureText(error), true);
      return false;
    } finally {
      finish();
    }
    try {
      return await applyNativeResult(result);
    } catch (error) {
      report(failureText(error), true);
      return false;
    }
  }
  async function chooseFiles(kind: 'images' | 'folder' | 'project') {
    if (busy() || modal()) return false;
    const caps = native?.capabilities();
    if (kind === 'project' ? caps?.projects : caps?.ready)
      return openNative(kind === 'images' ? 'openFiles' : kind === 'folder' ? 'openFolder' : 'openProject');
    try {
      const chosen = await browser.chooseFiles(kind);
      return chosen?.length ? openBrowserFiles(chosen) : false;
    } catch (error) {
      report(failureText(error), true);
      return false;
    }
  }
  function mutationAllowed(allowModal = false) {
    return (
      !!current() &&
      interaction.workspace !== 'generate' &&
      !closeInProgress &&
      navigationBusy !== navigationEpoch &&
      !externalOperation &&
      !settingsPending &&
      (allowModal || !modal())
    );
  }
  async function mutateDocument(
    data: DocumentMetadata,
    tail: string,
    body: Record<string, unknown> | FormData,
    method: 'POST' | 'PATCH',
    settings: { clearSelection?: boolean; selectNew?: boolean; label?: string; epoch?: number } = {},
  ) {
    const epoch = settings.epoch ?? navigationEpoch,
      oldIds = new Set(data.layer_stack?.map(layer => layer.id));
    const next = await api.mutate<DocumentMetadata>(data, tail, body, method);
    const value = accept(next);
    if (settings.selectNew) {
      const created = [...(value.layer_stack ?? [])].reverse().find(layer => !oldIds.has(layer.id));
      if (created) selected.set(value.id, created.id);
    }
    if (activeId === value.id && epoch === navigationEpoch) {
      if (settings.clearSelection) canvas.commitSelection(value.id);
      try {
        await present(value, epoch);
      } catch (error) {
        throw Error(
          'The change was accepted, but the preview could not be loaded. Refresh the view. ' + failureText(error),
        );
      }
      if (settings.label) report(settings.label);
    }
    return value;
  }
  async function edit(
    tail: string,
    body: Record<string, unknown> | FormData = {},
    method: 'POST' | 'PATCH' = 'POST',
    settings: { clearSelection?: boolean; selectNew?: boolean; label?: string; allowModal?: boolean } = {},
  ) {
    if (!mutationAllowed(settings.allowModal)) return null;
    const doc = current()!,
      epoch = navigationEpoch,
      finish = startOperation(
        tail === '/cutout/generate-background'
          ? 'Generating background'
          : tail === '/cutout' && method === 'POST'
            ? 'Removing background'
            : 'Updating image',
      );
    try {
      return await mutateDocument(doc, tail, body, method, { ...settings, epoch });
    } catch (error) {
      if (activeId === doc.id && epoch === navigationEpoch) report(failureText(error), true);
      return null;
    } finally {
      finish();
    }
  }
  function chooseLayer(id: string) {
    if (!editable() || modal()) return;
    const doc = current()!,
      layer = doc.layer_stack?.find(value => value.id === id && !value.discarded);
    if (!layer) return;
    if (selectedLayer()?.id !== id && canvas.pendingSelection(doc.id)) canvas.clearSelection();
    selected.set(doc.id, id);
    publish();
  }
  function setInteraction(change: Partial<CanvasInteraction>, force = false) {
    if (!force && (busy() || modal())) return;
    canvas.cancelGesture();
    interaction = { ...interaction, ...change };
    canvas.setInteractionState(interaction);
    publish();
  }
  function setWorkspace(value: Workspace, settings: { internal?: boolean } = {}) {
    if (!['retouch', 'cutout', 'generate'].includes(value) || (busy() && !settings.internal)) return;
    if (value === 'generate' && !settings.internal && features.enterGenerate) {
      void features.enterGenerate();
      return;
    }
    setInteraction(
      { workspace: value, ...(value === 'retouch' && interaction.tool === 'move' ? { tool: 'brush' as const } : {}) },
      !!settings.internal,
    );
  }
  function selectTool(value: Tool) {
    if (value === 'hand') {
      if (
        !current() ||
        modal() ||
        (interaction.workspace === 'generate' &&
          (!generationView.visible || generationView.creatingBlank || generationView.refining))
      )
        return;
      // Panning does not edit pixels. Like the original hand tool it remains
      // available during a backend operation and while comparing Original.
      setInteraction({ handActive: !interaction.handActive }, true);
      return;
    }
    if (!editable() || modal()) return;
    if (interaction.workspace === 'cutout' && selectedLayer()?.kind !== 'cutout' && value !== 'move') return;
    if (value === 'heal') {
      setInteraction({ workspace: 'retouch', operation: 'heal', tool: 'brush', handActive: false });
      store('local-remove-operation', 'heal');
      return;
    }
    const operation = value === 'brush' && interaction.workspace === 'retouch' ? 'ai' : interaction.operation;
    const changedOperation = operation !== interaction.operation;
    setInteraction({ tool: value, handActive: false, operation });
    if (changedOperation) store('local-remove-operation', operation);
  }
  async function applySelection() {
    if (!snapshot.tools.canApply || modal()) return null;
    const doc = current()!,
      epoch = navigationEpoch,
      payload = canvas.selectionPayload(),
      chosen = selectedLayer(),
      work = { ...interaction },
      variant = qwenVariant,
      provider = aiProvider,
      method = healMethod;
    const finish = startOperation(
      work.workspace === 'cutout'
        ? 'Refining selected mask'
        : work.operation === 'heal'
          ? 'Healing selected area'
          : 'Removing selected area',
    );
    try {
      let next: DocumentMetadata;
      if (work.workspace === 'cutout')
        next = await mutateDocument(
          doc,
          '/cutout/refine',
          { mask: payload, operation: work.cutoutOperation, layer_id: chosen?.id },
          'POST',
          { clearSelection: true, epoch, label: 'Mask updated.' },
        );
      else {
        let target = chosen,
          source = doc;
        if (target?.kind !== 'retouch') {
          source = await mutateDocument(source, '/stack/layers', { kind: 'retouch', name: 'Retouch' }, 'POST', {
            selectNew: true,
            epoch,
          });
          target = selectedLayer(source);
        }
        if (!target || target.locked || !target.visible)
          throw Error('Select a visible, unlocked retouch layer before repairing.');
        if (epoch !== navigationEpoch || activeId !== source.id) return null;
        next = await mutateDocument(
          source,
          '/remove',
          {
            mask: payload,
            model: work.operation === 'heal' ? 'heal' : provider,
            target_layer_id: target.id,
            ...(work.operation === 'heal' ? { heal_method: method } : provider === 'qwen' ? { variant } : {}),
          },
          'POST',
          {
            clearSelection: true,
            epoch,
            label:
              work.operation === 'heal' ? 'Heal added to the selected layer.' : 'Removal added to the selected layer.',
          },
        );
      }
      void refreshRecent();
      return next;
    } catch (error) {
      if (epoch === navigationEpoch && activeId === doc.id) report(failureText(error), true);
      return null;
    } finally {
      finish();
    }
  }
  async function undo(redo = false) {
    const target = historyTarget(redo);
    if (!target) return;
    if (target === 'selection') {
      await (redo ? canvas.redoSelection() : canvas.undoSelection());
      publish();
      return;
    }
    await edit('/' + target + '/' + (redo ? 'redo' : 'undo'), {}, 'POST', {
      label: redo ? 'Layer change redone.' : 'Layer change undone.',
    });
  }
  async function beforeDocumentCommand(kind: 'save' | 'project' | 'close' | 'credits') {
    if (features.prepareDocumentCommand) return features.prepareDocumentCommand(kind);
    if (
      interaction.workspace === 'generate' &&
      (!generationView.visible || generationView.refining || generationView.creatingBlank)
    ) {
      report('Choose a generated image before using this document command.');
      return { allowed: false, forceExport: false };
    }
    return { allowed: true, forceExport: false };
  }
  async function download(result: SaveResult, sid: string) {
    if (result.download) await browser.download(safeDownload(result.download, sid), result.name || 'Image');
  }
  async function save(mode: SaveMode, allowPending = false): Promise<SaveOutcome | null> {
    const prepared = await beforeDocumentCommand('save');
    if (!prepared.allowed || busy() || modal() || !current()) return null;
    if (prepared.forceExport) mode = 'export';
    const doc = current()!,
      epoch = navigationEpoch;
    if (interaction.workspace === 'cutout' && canvas.pendingSelection(doc.id) && !allowPending) {
      report('Apply or clear the pending mask/path before exporting.');
      return null;
    }
    if (mode !== 'export' && !doc.can_return) {
      report('Use Export to save a flattened copy of this uploaded image.', true);
      return null;
    }
    if (mode === 'overwrite' && askBeforeOverwrite) {
      canvas.resetTransientInput();
      const choice = await dialogs.confirmOverwrite(doc.source_name || doc.name);
      if (!choice || epoch !== navigationEpoch || activeId !== doc.id || busy()) return null;
      mode = choice;
    }
    const chosenFormat = mode === 'overwrite' ? 'original' : outputFormat,
      finish = startOperation(mode === 'export' ? 'Preparing export' : 'Saving image');
    try {
      const body =
        mode === 'export' ? { return_to_source: false, format: chosenFormat } : { mode, format: chosenFormat };
      const result = await api.mutate<SaveResult>(accepted.get(doc.id) ?? doc, '/save', body);
      if (result.session) accept(result.session);
      if (activeId === doc.id && epoch === navigationEpoch && result.collection) {
        collection = withCollectionDocument(readCollection(result.collection), current()!);
        collectionIndex = result.index ?? collectionIndex;
      }
      await download(result, doc.id);
      if (activeId === doc.id && epoch === navigationEpoch)
        report(
          (mode === 'export' ? 'Export ready: ' : mode === 'unique' ? 'Unique copy saved: ' : 'Saved: ') +
            (result.name || doc.name) +
            (canvas.pendingSelection(doc.id) ? '. Pending selection remains unapplied.' : ''),
        );
      return {
        mode,
        documentId: doc.id,
        name: result.name || doc.name,
        confirmed: mode !== 'export' && result.saved === true,
        kind: mode === 'export' ? 'download-started' : 'written',
      };
    } catch (error) {
      report('Save failed. Your edit and selection remain available. ' + failureText(error), true);
      return null;
    } finally {
      finish();
    }
  }
  async function saveProjectDocument(data: DocumentMetadata, saveAs = false): Promise<boolean> {
    await api.flush(data.id);
    const doc = accepted.get(data.id) ?? data,
      finish = startOperation('Saving editable project');
    try {
      const nativeProjects = !!native?.capabilities().projects;
      const result = (
        nativeProjects
          ? await native!.saveProject({
              session_id: doc.id,
              revision: doc.revision,
              ...(saveAs ? { saveAs: true } : {}),
            })
          : await api.mutate<SaveResult>(doc, '/export-project')
      ) as SaveResult | null;
      if (!result) {
        report('Project save cancelled. Working layers remain available.');
        return false;
      }
      if (result.session) accept(readDocument(result.session, doc.id));
      await download(result, doc.id);
      report(
        nativeProjects && result.saved === true
          ? 'Editable project saved: ' + (result.name || doc.name)
          : 'Project download prepared. Finish saving the file before closing.',
      );
      return nativeProjects && result.saved === true;
    } catch (error) {
      report('Project save failed. Working layers remain available. ' + failureText(error), true);
      return false;
    } finally {
      finish();
    }
  }
  async function saveProject(saveAs = false) {
    const target = await beforeDocumentCommand('project');
    if (!target.allowed || busy() || modal() || !current()) return false;
    return saveProjectDocument(current()!, saveAs);
  }
  async function closeDocuments(ids: readonly string[], all = false) {
    if (busy() || closeInProgress || modal()) return false;
    closeInProgress = true;
    canvas.rememberCurrentView();
    canvas.resetTransientInput();
    publish();
    try {
      await api.flush();
      const documents = [...new Set(ids)].map(id => accepted.get(id)).filter((doc): doc is DocumentMetadata => !!doc);
      const dirty = documents.filter(doc => hasWorkingLayers(doc) && projectNeedsSave(doc)),
        pending = documents.some(doc => canvas.pendingSelection(doc.id));
      if (documents.some(hasWorkingLayers) || pending) {
        const choice = await dialogs.confirmClose({
          all,
          dirtyCount: dirty.length,
          names: documents.map(doc => doc.name),
          pendingSelection: pending,
          nativeProjects: !!native?.capabilities().projects,
          warning: dirty.length
            ? 'Closing clears unsaved working layers. Save an editable project to continue later.'
            : 'Closing clears working layers and pending selections; saved files remain unchanged.',
        });
        if (!choice) return false;
        if (choice === 'save')
          for (const doc of dirty)
            if (!(await saveProjectDocument(doc))) {
              if (!native?.capabilities().projects)
                report(
                  'Finish saving the project download, then close again and choose Discard. Working layers were kept.',
                );
              return false;
            }
      }
      const latest = documents.map(doc => accepted.get(doc.id) ?? doc);
      if (latest.length) await api.close(latest.map(doc => ({ id: doc.id, revision: doc.revision })));
      const removed = new Set(latest.map(doc => doc.id));
      collection = forgetCollectionDocuments(collection, removed);
      for (const id of removed) {
        accepted.delete(id);
        acceptedOrder.delete(id);
        opened.delete(id);
        selected.delete(id);
        bases.delete(id);
        canvas.forgetDocument(id);
        for (const key of displays.keys()) if (key.startsWith(id + ':')) displays.delete(key);
      }
      if (activeId && removed.has(activeId)) {
        activeId = null;
        ++navigationEpoch;
        interaction.showOriginal = false;
        void canvas.presentDocument(null);
        browser.replaceUrl?.('/remove');
      }
      features.noteClosed?.([...removed]);
      await refreshRecent();
      report('Working images closed. Saved files remain unchanged.');
      return true;
    } catch (error) {
      report('Could not close the images. Working layers were kept. ' + failureText(error), true);
      return false;
    } finally {
      closeInProgress = false;
      publish();
    }
  }
  async function closeCurrent() {
    const target = await beforeDocumentCommand('close');
    return target.allowed ? (current() ? closeDocuments([current()!.id]) : true) : false;
  }
  function setOutputFormat(value: string) {
    if (!formats.has(value as OutputFormat) || busy()) return;
    outputFormat = value as OutputFormat;
    store('local-remove-copy-format', outputFormat);
    publish();
  }
  function setOverwritePreference(value: boolean) {
    askBeforeOverwrite = value;
    store('local-remove-ask-before-overwrite', String(value));
    publish();
  }
  function normalizeQwen(value: Record<string, unknown> | null | undefined) {
    if (!value) return emptyHealth().qwen;
    const supplied = value.variants;
    const variants = Array.isArray(supplied)
      ? supplied
      : supplied && typeof supplied === 'object'
        ? Object.entries(supplied).map(([id, item]) => ({ id, ...(item as object) }))
        : [];
    return {
      ready: value.ready === true,
      connected: value.connected === true,
      reason: typeof value.reason === 'string' ? value.reason : undefined,
      variants: ['int8', 'bf16'].map(id => {
        const item = variants.find(item => item.id === id);
        return {
          id,
          label: id === 'int8' ? 'Compact INT8' : 'Full BF16',
          available: item?.available === true,
          reason: item?.reason,
        };
      }),
    };
  }
  function acceptConfiguration(value: AcceptedConfiguration) {
    if (value.settings?.models) {
      const models = value.settings.models.filter(item => ['klein', 'heal'].includes(item.id));
      health = { ...health, models, settingsLoaded: models.some(item => item.id === 'klein') };
      const healer = models.find(item => item.id === 'heal');
      const methods = healer?.methods;
      if (Array.isArray(methods))
        health.healMethods = methods
          .filter(item => item && typeof item === 'object' && typeof item.id === 'string')
          .map(item => ({ id: item.id, label: item.label || item.id, available: item.available !== false }));
    }
    if (value.status)
      health = {
        ...health,
        ready: value.status.ready === true,
        retouchReady: value.status.retouch_ready === true,
        device: typeof value.status.device === 'string' ? value.status.device : '',
      };
    if ('qwen' in value) health = { ...health, qwen: normalizeQwen(value.qwen) };
    publish();
  }
  async function refreshHealth() {
    const version = ++healthVersion,
      results = await Promise.allSettled([api.settings(), api.status(), api.qwen()]);
    if (disposed || version !== healthVersion) return;
    const [settings, statusResult, qwen] = results;
    acceptConfiguration({
      ...(settings.status === 'fulfilled' ? { settings: settings.value } : {}),
      status: statusResult.status === 'fulfilled' ? statusResult.value : { ready: false, retouch_ready: false },
      qwen: qwen.status === 'fulfilled' ? qwen.value : null,
    });
  }
  function getContext(): DocumentContext {
    const doc = current();
    return {
      document: doc,
      documentId: doc?.id ?? null,
      revision: doc?.revision ?? 0,
      layerId: selectedLayer()?.id ?? null,
      navigationEpoch,
      busy: busy(),
    };
  }
  async function runDocumentChange<T>(work: (context: DocumentContext) => Promise<T>) {
    if (busy() || closeInProgress) throw Error('Finish the current operation before changing this document.');
    const epoch = navigationEpoch;
    externalOperation = true;
    const finish = startOperation('Updating document');
    try {
      await api.flush();
      if (epoch !== navigationEpoch) throw Error('The document changed before the operation could start.');
      return await work({ ...getContext(), busy: false });
    } finally {
      externalOperation = false;
      finish();
    }
  }
  async function acceptDocument(
    value: DocumentMetadata,
    context: Pick<DocumentContext, 'navigationEpoch' | 'documentId' | 'revision'>,
    destination: 'image' | 'background' | 'generated' = 'image',
  ) {
    const doc = readDocument(value);
    accept(doc);
    if (
      context.navigationEpoch !== navigationEpoch ||
      (destination === 'background' && (activeId !== context.documentId || doc.id !== context.documentId))
    )
      return false;
    if (destination === 'background') {
      if (
        doc.selected_layer_id &&
        doc.layer_stack?.some(layer => layer.id === doc.selected_layer_id && !layer.discarded)
      )
        selected.set(doc.id, doc.selected_layer_id);
      await refreshDocument(doc, navigationEpoch);
      return true;
    }
    return openSession(doc, {
      expectedNavigationEpoch: context.navigationEpoch,
      workspace: destination === 'generated' ? 'generate' : undefined,
    });
  }
  async function activateMode(workspace: Workspace, document: DocumentMetadata | null) {
    if (document) return openSession(document, { workspace, notifyFeatures: false });
    setWorkspace(workspace, { internal: true });
    return true;
  }
  async function applyGeneratedBackground(
    generatedId: string,
    targetId: string,
    captured: Pick<DocumentContext, 'documentId' | 'navigationEpoch' | 'revision'>,
  ) {
    const matches = () =>
      navigationEpoch === captured.navigationEpoch &&
      activeId === captured.documentId &&
      (current()?.revision ?? 0) === captured.revision;
    if (generatedId === targetId || !matches()) {
      report('The generated image or background target changed. Choose the image again.', true);
      return false;
    }
    try {
      return await runDocumentChange(async () => {
        if (!matches()) return false;
        const target = await api.session(targetId);
        if (!matches()) return false;
        api.observe(target);
        const layer = target.layer_stack?.find(
          value => value.id === selected.get(targetId) && value.kind === 'cutout' && !value.discarded,
        );
        const result = await api.mutate<DocumentMetadata>(target, '/cutout/generated-background', {
          generated_session_id: generatedId,
          ...(layer ? { layer_id: layer.id } : {}),
        });
        const stillCurrent = matches();
        accept(result);
        if (!stillCurrent) return false;
        if (
          result.selected_layer_id &&
          result.layer_stack?.some(value => value.id === result.selected_layer_id && !value.discarded)
        )
          selected.set(result.id, result.selected_layer_id);
        return openSession(result, { expectedNavigationEpoch: captured.navigationEpoch, workspace: 'cutout' });
      });
    } catch (error) {
      report('Could not apply the generated background. ' + failureText(error), true);
      return false;
    }
  }
  async function showCredits() {
    const result = await beforeDocumentCommand('credits');
    if (!result.allowed || busy() || modal()) return;
    const credits = documentCredits(current());
    dialogs.showCredits(credits, creditText(credits));
  }
  function downloadCredits() {
    const doc = current();
    if (doc)
      return browser.download(
        '/api/local-remove/session/' + encodeURIComponent(doc.id) + '/download-credits',
        doc.name.replace(/\.[^.]+$/, '') + '-credits.txt',
      );
  }
  const commands = {
    openFiles: () => chooseFiles('images'),
    openFolder: () => chooseFiles('folder'),
    openProject: () => chooseFiles('project'),
    openRecent: async (id: string) => {
      if (!busy() && !modal()) {
        try {
          await openSession(await api.session(id));
        } catch (error) {
          report(failureText(error), true);
        }
      }
    },
    saveProject: () => saveProject(),
    saveProjectAs: () => saveProject(true),
    overwrite: () => save('overwrite'),
    saveUnique: () => save('unique'),
    exportImage: (allowPending = false) => save('export', allowPending),
    closeImage: closeCurrent,
    selectLayer: chooseLayer,
    createRetouch: () =>
      edit('/stack/layers', { kind: 'retouch', name: 'Retouch' }, 'POST', {
        selectNew: true,
        label: 'Retouch layer selected.',
      }),
    patchLayer: (id: string, patch: LayerPatch) =>
      current()?.layer_stack?.some(layer => layer.id === id)
        ? edit('/stack/layer/' + encodeURIComponent(id), patch as Record<string, unknown>, 'PATCH', {
            label: 'Layer updated.',
          })
        : Promise.resolve(null),
    reorderLayer: (direction: number) => {
      const doc = current(),
        layer = selectedLayer();
      return doc && layer && writableLayer()
        ? edit(
            '/stack/layer/' + encodeURIComponent(layer.id),
            {
              index: Math.max(
                0,
                Math.min((doc.layer_stack?.length ?? 1) - 1, doc.layer_stack!.indexOf(layer) + direction),
              ),
            },
            'PATCH',
            { label: 'Layer order updated.' },
          )
        : Promise.resolve(null);
    },
    restoreLayer: () => {
      const layer = [...(current()?.layer_stack ?? [])].reverse().find(value => value.discarded);
      return layer
        ? edit('/stack/layer/' + encodeURIComponent(layer.id), { discarded: false }, 'PATCH', {
            label: 'Layer restored.',
          })
        : Promise.resolve(null);
    },
    mergeLayers: () =>
      edit('/merge', {}, 'POST', {
        selectNew: true,
        label: 'Merged visible layers; earlier layers remain recoverable.',
      }),
    addMask: async () => {
      const layer = selectedLayer();
      if (!editable() || !layer || !layer.visible || layer.kind === 'cutout') return null;
      const result = await edit(
        '/cutout/refine',
        { layer_id: layer.id, operation: 'replace', mask: canvas.fullMaskPayload() },
        'POST',
        { selectNew: true, clearSelection: true, label: 'Editable mask added.' },
      );
      if (result) {
        setWorkspace('cutout');
        setInteraction({ tool: 'brush' });
      }
      return result;
    },
    renameLayer: () => {
      const layer = selectedLayer();
      if (editable() && layer) return features.renameLayer?.(layer.id);
    },
    undo: () => undo(),
    redo: () => undo(true),
    applySelection,
    clearSelection: () => {
      if (editable()) {
        canvas.clearSelection({ recordHistory: true });
        publish();
      }
    },
    finishPath: () => {
      if (editable()) {
        canvas.finishPen();
        publish();
      }
    },
    selectTool,
    setWorkspace,
    setOutputFormat,
    setOverwritePreference,
    fit: () => canvas.fit(),
    actualSize: () => canvas.actualSize(),
    zoomIn: () => canvas.zoomIn(),
    zoomOut: () => canvas.zoomOut(),
    toggleOriginal: () => {
      const visible =
        !!current() &&
        (interaction.workspace !== 'generate' ||
          (generationView.visible && !generationView.refining && !generationView.creatingBlank));
      if (visible && !busy() && !modal()) setInteraction({ showOriginal: !interaction.showOriginal });
    },
    toggleInspector: () => {
      if (!generationView.refining) {
        inspectorHidden = !inspectorHidden;
        publish();
      }
    },
    showAssets: () => features.showAssets?.('image'),
    showGenerated: () => features.showGenerated?.(),
    showBatch: () => features.showBatch?.(),
    showSettings: () => features.showSettings?.(),
    showHardware: () => features.showHardware?.(),
    showShortcuts: () => features.showShortcuts?.(),
    browseModels: () => features.browseModels?.(),
    credits: showCredits,
  };
  const toolActions: ToolActions = {
    workspace: setWorkspace,
    tool: selectTool,
    brushSize: value => {
      if (editable()) {
        interaction.brushSize = Math.max(1, Math.min(2000, Math.round(value)));
        canvas.setBrushSize(interaction.brushSize);
        publish();
      }
    },
    selectionMode: subtract => setInteraction({ subtract }),
    finishPath: commands.finishPath,
    clearSelection: commands.clearSelection,
    applySelection,
    healMethod: value => {
      if (!busy() && health.healMethods.some(method => method.id === value)) {
        healMethod = value;
        store('local-remove-heal-method', value);
        publish();
      }
    },
    aiProvider: value => {
      if (!busy()) {
        aiProvider = value;
        publish();
      }
    },
    qwenVariant: value => {
      if (!busy() && ['int8', 'bf16'].includes(value)) {
        qwenVariant = value;
        store('local-remove-qwen-variant', value);
        publish();
      }
    },
    cutoutOperation: value => setInteraction({ cutoutOperation: value }),
    removeBackground: () =>
      snapshot.tools.canRemoveBackground
        ? edit('/cutout', { variant: qwenVariant, layer_id: selectedLayer()?.id }, 'POST', {
            selectNew: true,
            clearSelection: true,
            label: 'Background removal completed.',
          })
        : Promise.resolve(null),
    importBackground: async () => {
      if (!editable()) return;
      const chosen = await browser.chooseFiles('background');
      if (!chosen?.[0] || !editable()) return;
      const body = new FormData();
      body.append('file', chosen[0]);
      if (selectedLayer()) body.append('layer_id', selectedLayer()!.id);
      return edit('/cutout/background', body, 'POST', { selectNew: true, label: 'Background image added.' });
    },
    browseBackgrounds: () => features.showAssets?.('background'),
    generateBackground: () => features.generateBackground?.(),
    edgeOptions: () => {
      features.edgeOptions?.();
    },
    transform: value => {
      const layer = selectedLayer();
      return snapshot.tools.canTransform && layer
        ? commands.patchLayer(layer.id, { transform: value })
        : Promise.resolve(null);
    },
  };
  const chromeActions: DocumentChromeActions = {
    close: closeCurrent,
    toggleOriginal: commands.toggleOriginal,
    overwrite: commands.overwrite,
    saveUnique: commands.saveUnique,
    export: commands.exportImage,
    zoom: value => canvas.setPhotoZoom(value),
    zoomBy: factor => canvas.setPhotoZoom(canvas.getSnapshot().photoZoom * factor),
    fit: commands.fit,
  };
  // Canvas pointermove paints privately. Also ignore any intermediate pan-only
  // notifications while keeping completed gestures and resizes truthful.
  let settledPan = [canvas.getSnapshot().panX, canvas.getSnapshot().panY];
  const cameraKey = (view: CanvasSnapshot) => {
    if (view.gesture !== 'pan') settledPan = [view.panX, view.panY];
    return JSON.stringify([
      view.documentId,
      view.revision,
      view.hasSelection,
      view.pointCount,
      view.selectionVersion,
      view.canUndoSelection,
      view.canRedoSelection,
      view.historyBusy,
      view.photoZoom,
      view.fitMode,
      ...settledPan,
      view.previewWidth,
      view.previewHeight,
    ]);
  };
  let lastCanvas = cameraKey(canvas.getSnapshot());
  const unsubscribeCanvas = canvas.subscribe(() => {
    const key = cameraKey(canvas.getSnapshot());
    if (key === lastCanvas) return;
    lastCanvas = key;
    if (!suppressCanvas) publish();
  });
  const unsubscribeNative = native?.subscribe(publish);
  async function openInitial(search: string) {
    const query = new URLSearchParams(search),
      epoch = navigationEpoch;
    try {
      if (query.get('collection')) {
        const value = await api.collection(query.get('collection')!);
        if (epoch === navigationEpoch) await openCollection(value, Number(query.get('index')) || 0);
      } else if (query.get('session')) {
        const value = await api.session(query.get('session')!);
        if (epoch === navigationEpoch) await openSession(value, { expectedNavigationEpoch: epoch });
      }
    } catch (error) {
      report(failureText(error), true);
    }
  }
  publish();
  return {
    getSnapshot: () => snapshot,
    subscribe(listener: () => void) {
      listeners.add(listener);
      return () => {
        listeners.delete(listener);
      };
    },
    commands,
    toolActions,
    chromeActions,
    api,
    canvas,
    getAcceptedDocument,
    getContext,
    runDocumentChange,
    acceptDocument,
    activateMode,
    applyGeneratedBackground,
    openSession,
    openCollection,
    openCollectionEntry,
    openBrowserFiles,
    importProject,
    applyNativeResult,
    drop: (dropped: readonly File[]) =>
      native?.capabilities().ready ? openNative('drop', dropped) : openBrowserFiles(dropped),
    openInitial,
    async initialize(search = '') {
      await native?.connect();
      await refreshHealth();
      await refreshRecent();
      await openInitial(search);
    },
    refreshHealth,
    refreshRecent,
    acceptConfiguration,
    report,
    downloadCredits,
    async refreshPreview() {
      if (busy() || !current()) return false;
      const doc = current()!,
        epoch = navigationEpoch,
        finish = startOperation('Refreshing view');
      try {
        const shown = await present(doc, epoch);
        if (shown) report('View refreshed.');
        return shown;
      } catch (error) {
        report('Could not refresh the view. ' + failureText(error), true);
        return false;
      } finally {
        finish();
      }
    },
    setFeatures(value: DocumentFeatureCommands) {
      features = value;
    },
    setGenerationView(value: Partial<typeof generationView>) {
      generationView = { ...generationView, ...value };
      publish();
    },
    restoreInteraction(value: CanvasInteraction) {
      interaction = { ...value };
      if (!suppressCanvas) publish();
    },
    setSettingsPending(value: boolean) {
      settingsPending = value;
      publish();
    },
    setMenuOpen(value: boolean) {
      menuOpen = value;
      if (value) canvas.resetTransientInput();
      publish();
    },
    isMenuOpen: () => menuOpen,
    isModalOpen: modal,
    setFilmstripCollapsed(value: boolean) {
      filmstripCollapsed = value;
      publish();
    },
    setThumbnailSize(value: number) {
      thumbnailSize = Math.max(48, Math.min(160, value));
      publish();
    },
    navigateCollection: (direction: number) => openCollectionEntry(collectionIndex + direction),
    flush: (id?: string) => api.flush(id),
    pendingSelection: (id: string) => canvas.pendingSelection(id),
    pendingFingerprint: (id: string) => canvas.pendingFingerprint(id),
    rememberCurrentView: () => canvas.rememberCurrentView(),
    async resolveCollectionSession(entryId: string) {
      if (!collection) {
        const doc = current();
        if (doc?.id !== entryId) throw Error('The selected image is no longer open.');
        return { session_id: doc.id, revision: doc.revision };
      }
      const source = collection,
        entry = source.entries.find(item => item.id === entryId);
      if (!entry) throw Error('The selected photo is no longer in this collection.');
      const doc = source.local
        ? await resolveEntry(source, entry)
        : entry.session_id
          ? await api.session(entry.session_id)
          : (await api.openEntry(source.id, entry.id)).session!;
      accept(doc);
      publish();
      return { session_id: doc.id, revision: doc.revision };
    },
    async returnToPendingSelection(id: string) {
      if (activeId !== id) {
        const index = collection?.entries.findIndex(entry => entry.session_id === id) ?? -1;
        if (index >= 0) await openCollectionEntry(index);
        else {
          const doc = accepted.get(id);
          if (!doc) throw Error('The image is no longer open.');
          await openSession(doc);
        }
      }
      canvas.focus();
      report('Apply the pending selection, then reopen Batch.');
    },
    closeDocuments,
    closeAll: () => closeDocuments([...opened], true),
    hasUnsavedWork: () =>
      [...opened].some(id => {
        const doc = accepted.get(id);
        return !!doc && (hasWorkingLayers(doc) || canvas.pendingSelection(id));
      }),
    commitLayerTransform(value: TransformCommit & { layerId: string }) {
      if (activeId !== value.documentId || current()?.revision !== value.revision) return Promise.resolve(null);
      return commands.patchLayer(value.layerId, { transform: value.transform });
    },
    commitLegacyCutoutTransform(value: TransformCommit) {
      if (activeId !== value.documentId || current()?.revision !== value.revision) return Promise.resolve(null);
      return edit('/cutout', { transform: value.transform }, 'PATCH');
    },
    patchCutout(value: {
      feather?: number;
      shadow?: Record<string, unknown>;
      enabled?: boolean;
      background?: { mode?: 'transparent' | 'color'; color?: string };
    }) {
      const layer = selectedLayer();
      return layer?.kind === 'cutout' || (!current()?.layer_stack && current()?.cutout)
        ? edit('/cutout', { ...value, ...(layer ? { layer_id: layer.id } : {}) }, 'PATCH', {
            allowModal: true,
            label: 'Cutout properties updated.',
          })
        : Promise.resolve(null);
    },
    generateBackground(prompt: string) {
      return prompt.trim() && qwenReady()
        ? edit('/cutout/generate-background', { prompt, variant: qwenVariant, layer_id: selectedLayer()?.id }, 'POST', {
            allowModal: true,
            selectNew: true,
            label: 'Generated background added.',
          })
        : Promise.resolve(null);
    },
    dispose() {
      disposed = true;
      ++navigationEpoch;
      ++healthVersion;
      unsubscribeCanvas();
      unsubscribeNative?.();
      for (const url of objectUrls) browser.revokeObjectURL?.(url);
      listeners.clear();
      displays.clear();
      bases.clear();
      files.clear();
    },
  };
}
export type DocumentController = ReturnType<typeof createDocumentController>;
