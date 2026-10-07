import { createModelApi, type ModelApi } from './api.ts';
import type { SetupState } from '../settings/types.ts';
import type {
  BrowserModel,
  LoraDraftPort,
  LoraItem,
  LoraSelection,
  ModelBridge,
  ModelBrowserSelection,
  ModelsSnapshot,
} from './types.ts';

function immutable<T>(value: T): T {
  if (value && typeof value === 'object' && !Object.isFrozen(value)) {
    for (const item of Object.values(value)) immutable(item);
    Object.freeze(value);
  }
  return value;
}
const errorText = (error: unknown) => (error instanceof Error ? error.message : 'The model operation failed.');
export function createModelsController(options: {
  token: string;
  bridge: ModelBridge;
  api?: ModelApi;
  onCatalog?: (models: BrowserModel[]) => void;
  onSetup?: (setup: SetupState) => void;
  timers?: { set: (callback: () => void, ms: number) => unknown; clear: (id: unknown) => void };
}) {
  const api = options.api ?? createModelApi(options.token),
    bridge = options.bridge;
  const timers = options.timers ?? {
    set: (callback: () => void, ms: number) => setTimeout(callback, ms),
    clear: (id: unknown) => clearTimeout(id as ReturnType<typeof setTimeout>),
  };
  const listeners = new Set<() => void>();
  let epoch = 0,
    versions = { catalog: 0, inventory: 0, search: 0, files: 0, poll: 0 },
    timer: unknown = null,
    disposed = false;
  let modelSelection: ModelBrowserSelection = {},
    loraPort: LoraDraftPort | null = null;
  let folderConnection: SetupState['model_folder_connection'],
    waitingForStart = false;
  let state: ModelsSnapshot = immutable({
    view: null,
    loading: false,
    pendingNative: false,
    error: '',
    message: '',
    nativeSetup: bridge.capabilities().setup,
    models: [],
    selectedModelId: '',
    selectedVariant: '',
    downloads: null,
    setup: null,
    hardware: null,
    context: null,
    selectedLoras: [],
    loraTab: 'installed',
    inventory: null,
    query: '',
    searchResults: [],
    searching: false,
    filesLoading: false,
    files: null,
    selectedFilename: '',
    allowUnverified: false,
    info: null,
    loraJob: null,
    checkedAt: null,
  });
  function update(change: Partial<ModelsSnapshot>) {
    if (disposed) return;
    state = immutable({ ...state, ...structuredClone(change) });
    for (const listener of listeners) listener();
  }
  function refreshCapabilities() {
    const nativeSetup = bridge.capabilities().setup;
    if (nativeSetup !== state.nativeSetup) update({ nativeSetup });
  }
  function stopPolling() {
    if (timer !== null) timers.clear(timer);
    timer = null;
  }
  function scope(kind: keyof typeof versions) {
    return { epoch, kind, version: ++versions[kind], context: state.context };
  }
  function valid(value: ReturnType<typeof scope>) {
    return (
      !disposed &&
      state.view !== null &&
      value.epoch === epoch &&
      value.version === versions[value.kind] &&
      (!value.context ||
        (loraPort?.contextId === value.context.contextId && loraPort.read().modelId === value.context.modelId))
    );
  }
  function liveContext() {
    return (
      state.view === 'loras' &&
      !!loraPort &&
      !!state.context &&
      loraPort.contextId === state.context.contextId &&
      loraPort.read().modelId === state.context.modelId
    );
  }
  function locked() {
    return bridge.editorBusy() || state.pendingNative;
  }
  function selectModel(id: string, requestedVariant?: string) {
    const model = state.models.find(item => item.id === id);
    if (!model) return;
    const variant =
      model.variants?.find(item => item.id === requestedVariant)?.id ||
      model.defaults?.variant ||
      model.variants?.[0]?.id ||
      '';
    update({ selectedModelId: id, selectedVariant: variant });
  }
  async function refreshModels(force = false) {
    const token = scope('catalog');
    update({ loading: true, error: '', nativeSetup: bridge.capabilities().setup });
    const results = await Promise.allSettled([api.catalog(force), api.downloads(), api.setup(), api.hardware()]);
    if (!valid(token) || state.view !== 'models') return;
    const [catalog, downloads, setup, hardware] = results;
    const change: Partial<ModelsSnapshot> = { loading: false };
    if (catalog.status === 'fulfilled') {
      change.models = catalog.value.models.filter(model => !model.historical);
      options.onCatalog?.(catalog.value.models);
    }
    if (downloads.status === 'fulfilled') change.downloads = downloads.value;
    if (setup.status === 'fulfilled') {
      if (waitingForStart && setup.value.job?.status !== 'running' && setup.value.service?.running) {
        folderConnection = undefined;
        waitingForStart = false;
      }
      change.setup = { ...setup.value, ...(folderConnection ? { model_folder_connection: folderConnection } : {}) };
      options.onSetup?.(change.setup);
    }
    if (hardware.status === 'fulfilled') change.hardware = hardware.value;
    change.error = results
      .filter(result => result.status === 'rejected')
      .map(result => errorText((result as PromiseRejectedResult).reason))
      .join(' ');
    update(change);
    const chosen =
      state.models.find(model => model.id === state.selectedModelId) ||
      state.models.find(model => model.id === modelSelection.selectedModelId) ||
      state.models[0];
    if (chosen) selectModel(chosen.id, state.selectedVariant || modelSelection.selectedVariant);
    if (state.downloads?.running || state.setup?.service?.starting || state.setup?.job?.status === 'running')
      schedulePoll();
  }
  function selectedModel() {
    return state.models.find(model => model.id === state.selectedModelId);
  }
  function selectedVariant() {
    return selectedModel()?.variants?.find(variant => variant.id === state.selectedVariant);
  }
  function modelDownloadBlock() {
    const model = selectedModel(),
      variant = selectedVariant();
    const disk = state.downloads?.models
      ?.find(item => item.id === model?.id)
      ?.variants?.find(item => item.id === variant?.id);
    if (!bridge.capabilities().setup) return 'Open the desktop app to choose folders and download models.';
    if (locked() || state.loading || state.downloads?.running) return 'Wait for the current operation to finish.';
    if (!model || !variant) return 'Choose a model and precision.';
    if (model.downloadable === false || variant.downloadable === false)
      return variant.download_note || model.download_note || 'Publisher access is required for this model.';
    if (variant.available || variant.missing_bytes === 0 || disk?.installed || disk?.missing_bytes === 0)
      return 'Model files are already installed.';
    return '';
  }
  async function openModels(selection: ModelBrowserSelection = {}) {
    stopPolling();
    ++epoch;
    loraPort = null;
    modelSelection = selection;
    update({
      view: 'models',
      context: null,
      selectedModelId: selection.selectedModelId ?? '',
      selectedVariant: selection.selectedVariant ?? '',
      error: '',
      message: '',
      nativeSetup: bridge.capabilities().setup,
    });
    await refreshModels();
  }
  function close() {
    ++epoch;
    stopPolling();
    loraPort = null;
    update({ view: null, loading: false, searching: false, filesLoading: false });
    bridge.focusCanvas();
  }
  function schedulePoll() {
    stopPolling();
    if (state.view)
      timer = timers.set(() => {
        timer = null;
        void poll();
      }, 2000);
  }
  async function poll() {
    const token = scope('poll'),
      view = state.view;
    try {
      const job = view === 'models' ? await api.downloads() : await api.loraDownload();
      if (!valid(token) || state.view !== view) return;
      update(
        view === 'models'
          ? {
              downloads: job,
              error: job.phase === 'error' ? job.error || job.message || 'Download failed.' : '',
              message: job.message || '',
            }
          : {
              loraJob: job,
              error: job.phase === 'error' ? job.error || job.message || 'Adapter download failed.' : '',
              message: job.message || '',
            },
      );
      if (view === 'models' && (state.setup?.service?.starting || state.setup?.job?.status === 'running'))
        await refreshModels();
      else if (job.running) schedulePoll();
      else if (job.phase === 'complete') {
        if (view === 'models') await refreshModels(true);
        else await loadInventory();
      }
    } catch (error) {
      if (valid(token)) update({ error: errorText(error) });
    }
  }
  async function nativeOperation(
    operation: () => Promise<unknown | null>,
    success: (result: unknown) => Promise<void>,
  ) {
    if (locked() || !bridge.capabilities().setup) return;
    const operationEpoch = epoch;
    update({ pendingNative: true, error: '', message: '' });
    try {
      const result = await operation();
      if (result == null || disposed || operationEpoch !== epoch) return;
      await success(result);
    } catch (error) {
      if (operationEpoch === epoch) update({ error: errorText(error) });
    } finally {
      update({ pendingNative: false });
    }
  }
  function filteredCurated(query = state.query) {
    const needle = query.trim().toLowerCase();
    return (state.inventory?.curated ?? []).filter(
      item =>
        !needle ||
        [item.title, item.style, item.description, item.repo_id]
          .filter(Boolean)
          .join(' ')
          .toLowerCase()
          .includes(needle),
    );
  }
  async function openLoras(port: LoraDraftPort) {
    stopPolling();
    ++epoch;
    loraPort = port;
    const draft = port.read();
    update({
      view: 'loras',
      loading: true,
      context: {
        contextId: port.contextId,
        modelId: draft.modelId,
        modelLabel: draft.modelLabel || draft.modelId,
        referenceCount: draft.referenceCount ?? 0,
        supportsLoras: draft.supportsLoras !== false,
      },
      selectedLoras: draft.selected,
      inventory: null,
      files: null,
      selectedFilename: '',
      allowUnverified: false,
      info: null,
      loraTab: 'installed',
      query: '',
      searchResults: [],
      searching: false,
      filesLoading: false,
      loraJob: null,
      checkedAt: null,
      error: '',
      message: '',
      nativeSetup: bridge.capabilities().setup,
    });
    await loadInventory();
  }
  async function loadInventory() {
    if (!liveContext()) return;
    const token = scope('inventory');
    update({ loading: true });
    try {
      const inventory = await api.inventory(state.context!.modelId);
      if (!valid(token)) return;
      const selected = loraPort!.read().selected.map(item => {
        const installed = inventory.installed.find(entry => entry.id === item.id);
        return {
          ...item,
          missing: !installed || installed.supported === false,
          ...(installed
            ? {
                title: installed.title || installed.filename || item.title,
                usage: installed.usage,
                trigger_phrase: installed.trigger_phrase,
              }
            : {}),
        };
      });
      loraPort!.setSelected(structuredClone(selected));
      update({
        inventory,
        selectedLoras: selected,
        loraJob: inventory.job ?? null,
        loading: false,
        error: '',
        message: `Adapters for ${state.context!.modelLabel}. Select up to three.`,
      });
      if (inventory.job?.running) schedulePoll();
    } catch (error) {
      if (valid(token)) update({ loading: false, error: `${errorText(error)} Refresh to retry.` });
    }
  }
  async function search() {
    if (!liveContext() || state.loading) return;
    const token = scope('search'),
      query = state.query.trim(),
      recommended = filteredCurated(query);
    update({ searching: true, searchResults: recommended, error: '', message: 'Searching current adapter metadata…' });
    try {
      const value = await api.search(state.context!.modelId, query);
      if (!valid(token)) return;
      const known = new Set(recommended.map(item => item.repo_id));
      update({
        searching: false,
        searchResults: [...recommended, ...value.results.filter(item => !known.has(item.repo_id))],
        checkedAt: Date.now(),
        message: 'Review compatibility and license details before choosing a file.',
      });
    } catch (error) {
      if (valid(token))
        update({
          searching: false,
          checkedAt: null,
          error: `Online search failed: ${errorText(error)} Installed adapters and recommended examples remain available.`,
        });
    }
  }
  async function inspectFiles(item: LoraItem) {
    if (!liveContext() || !item.repo_id || item.supported === false) return;
    const token = scope('files');
    update({ filesLoading: true, files: null, selectedFilename: '', allowUnverified: false, info: null, error: '' });
    try {
      const files = await api.files(state.context!.modelId, item.repo_id, item.revision);
      if (!valid(token)) return;
      update({
        files,
        selectedFilename: files.files.some(file => file.filename === item.filename)
          ? item.filename!
          : files.files[0]?.filename || '',
        filesLoading: false,
        message: 'Select the exact adapter file to download. Downloading does not enable it.',
      });
    } catch (error) {
      if (valid(token)) update({ filesLoading: false, error: errorText(error) });
    }
  }
  function selectedFile() {
    return state.files?.files.find(file => file.filename === state.selectedFilename);
  }
  function loraDownloadBlock() {
    const file = selectedFile();
    if (!bridge.capabilities().setup)
      return 'Adapter downloads require the desktop app. Installed adapters remain usable here.';
    if (!liveContext() || !file || !state.files) return 'Select an adapter file for the current model.';
    if (state.context?.supportsLoras === false) return 'The selected workflow does not support adapters.';
    if (locked() || state.loraJob?.running) return 'An operation or adapter download is already running.';
    if (state.files.supported === false || file.supported === false)
      return file.warning || state.files.warning || 'This adapter is not supported by the selected workflow.';
    if (!/^[a-f0-9]{40}$/i.test(state.files.revision))
      return 'A verified repository revision is required before downloading.';
    if ((file.compatibility || state.files.compatibility || 'unverified') !== 'curated' && !state.allowUnverified)
      return 'Review compatibility and explicitly accept assigning this file to the selected model.';
    return '';
  }
  function setSelected(value: LoraSelection[]) {
    if (!liveContext()) return;
    loraPort!.setSelected(structuredClone(value));
    update({ selectedLoras: value });
  }
  function useLora(item: LoraItem) {
    if (!liveContext() || locked() || !item.id || item.supported === false || state.context?.supportsLoras === false)
      return;
    const selected = loraPort!.read().selected;
    if (selected.length >= 3 || selected.some(value => value.id === item.id)) return;
    setSelected([
      ...selected,
      {
        id: item.id,
        title: item.title || item.filename,
        strength: Number.isFinite(item.recommended_strength)
          ? Math.max(-2, Math.min(2, item.recommended_strength!))
          : 1,
        usage: item.usage,
        trigger_phrase: item.trigger_phrase,
      },
    ]);
  }
  return {
    getSnapshot: () => state,
    subscribe: (listener: () => void) => {
      listeners.add(listener);
      return () => {
        listeners.delete(listener);
      };
    },
    isOpen: () => state.view !== null,
    openModels,
    openLoras,
    close,
    refreshCapabilities,
    refreshModels,
    loadInventory,
    selectedModel,
    selectedVariant,
    modelDownloadBlock,
    selectedFile,
    loraDownloadBlock,
    selectModel: (id: string) => selectModel(id),
    selectVariant: (id: string) => {
      if (selectedModel()?.variants?.some(variant => variant.id === id)) update({ selectedVariant: id });
    },
    useModel: () => {
      if (locked() || state.loading || state.downloads?.running || !selectedModel() || !selectedVariant()) return;
      const result = modelSelection.onUse?.(state.selectedModelId, state.selectedVariant);
      close();
      return result;
    },
    downloadModel: async () => {
      if (modelDownloadBlock()) return;
      const model = state.selectedModelId,
        variant = state.selectedVariant;
      await nativeOperation(() => bridge.downloadModel(model, variant), poll);
    },
    chooseModelDirectory: () => {
      if (state.downloads?.running) return;
      return nativeOperation(
        () => bridge.chooseModelDirectory(),
        async result => {
          folderConnection = (result as SetupState).model_folder_connection;
          await refreshModels(true);
        },
      );
    },
    startBackend: () => {
      if (state.downloads?.running || !state.setup?.service?.can_start || state.setup?.service?.running) return;
      return nativeOperation(
        () => bridge.startBackend(),
        async () => {
          waitingForStart = true;
          await refreshModels(true);
        },
      );
    },
    selectLoraTab: (tab: 'installed' | 'browse') => {
      update({ loraTab: tab, info: null });
      if (tab === 'browse') void search();
    },
    setQuery: (query: string) => update({ query }),
    search,
    showRecommended: () => {
      update({ query: '' });
      return search();
    },
    inspectFiles,
    selectFile: (selectedFilename: string) => {
      if (state.files?.files.some(file => file.filename === selectedFilename))
        update({ selectedFilename, allowUnverified: false });
    },
    acknowledgeCompatibility: (allowUnverified: boolean) => update({ allowUnverified }),
    downloadLora: async () => {
      if (loraDownloadBlock()) return;
      const payload = {
        model: state.context!.modelId,
        repo_id: state.files!.repo_id,
        filename: state.selectedFilename,
        revision: state.files!.revision,
        allow_unverified:
          (selectedFile()?.compatibility || state.files?.compatibility || 'unverified') !== 'curated' &&
          state.allowUnverified,
      };
      await nativeOperation(() => bridge.downloadLora(payload), poll);
    },
    useLora,
    removeLora: (id: string) => {
      if (liveContext() && !locked()) setSelected(loraPort!.read().selected.filter(item => item.id !== id));
    },
    setStrength: (id: string, strength: number) => {
      if (liveContext() && !locked() && Number.isFinite(strength))
        setSelected(
          loraPort!
            .read()
            .selected.map(item => (item.id === id ? { ...item, strength: Math.max(-2, Math.min(2, strength)) } : item)),
        );
    },
    showInfo: (info: LoraItem | null) => update({ info }),
    addTrigger: (phrase: string) => {
      if (liveContext() && !locked() && phrase) {
        loraPort!.appendPrompt(phrase);
        update({ message: 'Trigger added to this draft. Review it before generating.' });
      }
    },
    applySampling: (settings: { steps?: number; guidance?: number }) => {
      if (liveContext() && !locked()) {
        loraPort!.applySampling(settings);
        update({ message: 'Recommended sampling applied to this draft. Choose the installed adapter separately.' });
      }
    },
    dispose: () => {
      disposed = true;
      ++epoch;
      stopPolling();
      listeners.clear();
      loraPort = null;
    },
  };
}
export type ModelsController = ReturnType<typeof createModelsController>;
