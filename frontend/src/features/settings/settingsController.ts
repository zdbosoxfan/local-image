import { createSettingsApi, type SettingsApi } from './settingsApi.ts';
import type { AcceptedConfiguration, InterfaceDensity, SettingsBridge, SettingsSnapshot, SettingsView, SetupAction } from './types.ts';
import { modelDownloadSelection } from './modelDownload.ts';

function immutable<T>(value: T): T {
  if (value && typeof value === 'object' && !Object.isFrozen(value)) { for (const child of Object.values(value)) immutable(child); Object.freeze(value); }
  return value;
}
export function createSettingsController(options: {
  token: string; bridge: SettingsBridge; api?: SettingsApi;
  browseModels?: () => unknown; startTask?: (workspace: 'retouch' | 'cutout' | 'generate') => unknown;
  storage?: Pick<Storage, 'getItem' | 'setItem'>;
  timers?: { set: (callback: () => void, milliseconds: number) => unknown; clear: (id: unknown) => void };
}) {
  const { bridge } = options;
  const api = options.api ?? createSettingsApi(options.token);
  const timers = options.timers ?? { set: (callback: () => void, milliseconds: number) => setTimeout(callback, milliseconds), clear: (id: unknown) => clearTimeout(id as ReturnType<typeof setTimeout>) };
  const listeners = new Set<() => void>();
  let epoch = 0, timer: unknown = null, disposed = false, firstSetupPending = false;
  let snapshot: SettingsSnapshot = immutable({ view: null, requestedSection: 'general', loading: false, pendingAction: null, setup: null, hardware: null,
    models: [], modelCatalogAvailable: false, modelDownloads: null, selectedModelId: '', selectedVariant: '',
    hideHardwareGuide: false, savingHardwarePreference: false,
    capabilities: bridge.capabilities(), preferences: bridge.preferences(), error: '', message: '', showInstallations: false, selectedInstallation: '' });
  function update(change: Partial<SettingsSnapshot>) {
    if (disposed) return;
    snapshot = immutable({ ...snapshot, ...structuredClone(change) });
    for (const listener of listeners) listener();
  }
  function stopPolling() { if (timer !== null) timers.clear(timer); timer = null; }
  function schedule() {
    stopPolling();
    if (!disposed && snapshot.view === 'settings' && (snapshot.modelDownloads?.running || snapshot.setup?.job?.status === 'running' || snapshot.setup?.service?.starting)) {
      timer = timers.set(() => { timer = null; void refresh(); }, 1500);
    }
  }
  function refreshCapabilities() {
    const capabilities = bridge.capabilities();
    if (capabilities.ready !== snapshot.capabilities.ready || capabilities.setup !== snapshot.capabilities.setup) update({ capabilities });
  }
  async function refresh(detect = false) {
    const requestEpoch = ++epoch;
    refreshCapabilities();
    update({ loading: true, error: '', ...(detect ? { showInstallations: true, message: 'Looking for ComfyUI on this PC…' } : {}) });
    const results = await Promise.allSettled([api.settings(), api.setup(detect), api.status(), api.qwen(), api.modelCatalog(), api.modelDownloads()]);
    if (disposed || requestEpoch !== epoch) return;
    const [settings, setup, status, qwen, initialCatalog, downloads] = results;
    let catalog = initialCatalog;
    // Completion can change the backend's model availability. Refresh that
    // catalog once, while regular progress polls use the existing cached read.
    if (downloads.status === 'fulfilled' && !downloads.value.running && downloads.value.phase === 'complete' && snapshot.modelDownloads?.running) {
      try {catalog = {status:'fulfilled', value:await api.modelCatalog(true)};}
      catch (reason) {catalog = {status:'rejected', reason};}
      if (disposed || requestEpoch !== epoch) return;
    }
    const accepted: AcceptedConfiguration = {};
    const validSettings = settings.status === 'fulfilled' && Array.isArray(settings.value.models) && settings.value.models.some(model => model.id === 'klein');
    if (settings.status === 'fulfilled' && validSettings) accepted.settings = settings.value;
    if (setup.status === 'fulfilled') accepted.setup = setup.value;
    accepted.status = status.status === 'fulfilled' ? status.value : { ready: false, retouch_ready: false };
    accepted.qwen = qwen.status === 'fulfilled' ? qwen.value : null;
    bridge.acceptConfiguration(accepted);
    const errors = results.filter(result => result.status === 'rejected').map(result => (result as PromiseRejectedResult).reason?.message || 'The local backend did not respond.');
    if (settings.status === 'fulfilled' && !validSettings) errors.push('Could not read FLUX settings. Refresh the local backend state.');
    const nextSetup = accepted.setup ?? snapshot.setup;
    const modelCatalogAvailable = catalog.status === 'fulfilled' && Array.isArray(catalog.value.models);
    const models = catalog.status === 'fulfilled' && modelCatalogAvailable ? catalog.value.models.filter(model => !model.historical && !!model.variants?.length) : snapshot.models;
    const model = models.find(item => item.id === snapshot.selectedModelId) || models.find(item => item.id === 'qwen') || models[0];
    const selectedVariant = model?.variants?.find(item => model.id === snapshot.selectedModelId && item.id === snapshot.selectedVariant)?.id || model?.variants?.find(item => item.id === model.defaults?.variant)?.id || model?.variants?.[0]?.id || '';
    if (catalog !== initialCatalog && catalog.status === 'rejected') errors.push(catalog.reason instanceof Error ? catalog.reason.message : 'Could not refresh model availability.');
    if (catalog.status === 'fulfilled' && !modelCatalogAvailable) errors.push('Could not read model details. Refresh the local backend state.');
    const candidates = nextSetup?.installations ?? [];
    const selectedInstallation = candidates.some(item => item.id === snapshot.selectedInstallation) ? snapshot.selectedInstallation : nextSetup?.installation?.id ?? candidates[0]?.id ?? '';
    update({ loading: false, setup: nextSetup, selectedInstallation, models, modelCatalogAvailable, selectedModelId:model?.id || '', selectedVariant,
      modelDownloads: downloads.status === 'fulfilled' ? downloads.value : snapshot.modelDownloads, error: [...new Set(errors)].join(' '),
      ...(detect ? { message: candidates.length ? 'Choose an installation.' : nextSetup?.portable?.available === false ? 'No installation found. Choose your existing ComfyUI folder.' : 'No installation found. Choose a folder or install a dedicated copy.' } : {}) });
    schedule();
  }
  async function open(view: Exclude<SettingsView, null>, requestedSection: 'general' | 'ai' = 'general') {
    stopPolling(); ++epoch;
    refreshCapabilities();
    update({ view, requestedSection, error: '', message: '', preferences: bridge.preferences(), loading: false });
    if (view === 'settings') await refresh();
    else if (view === 'hardware') {
      const requestEpoch = ++epoch;
      update({ loading: true });
      const [hardware, preference] = await Promise.allSettled([api.hardware(), api.hardwarePreference()]);
      if (disposed || requestEpoch !== epoch) return;
      update({loading: false, hideHardwareGuide: preference.status === 'fulfilled' && typeof preference.value.dont_show_again === 'boolean' ? preference.value.dont_show_again : readPreference('local-image.hardware-guide.v1') === '1',
        ...(hardware.status === 'fulfilled' ? {hardware: hardware.value} : {error: hardware.reason instanceof Error ? hardware.reason.message : 'Hardware detection is unavailable.'})});
    }
  }
  function readPreference(key: string) { try { return (options.storage ?? localStorage).getItem(key); } catch { return null; } }
  function writePreference(key: string, value: string) { try { (options.storage ?? localStorage).setItem(key, value); } catch { /* Preference storage may be unavailable. */ } }
  function offerFirstSetup() {
    if (!firstSetupPending || snapshot.view || bridge.editorBusy() || !bridge.capabilities().setup) return;
    firstSetupPending = false; writePreference('local-image.first-ai-setup.v1', '1');
    update({ showInstallations: snapshot.setup?.setup_mode === 'discover' });
    void open('settings', 'ai');
  }
  function close() { const wasHardware = snapshot.view === 'hardware'; ++epoch; stopPolling(); update({ view: null, loading: false }); bridge.focusCanvas(); if (wasHardware) offerFirstSetup(); }
  function locked() { return bridge.editorBusy() || !!snapshot.pendingAction || !!snapshot.modelDownloads?.running || snapshot.setup?.job?.status === 'running' || !!snapshot.setup?.service?.starting || !!snapshot.setup?.service?.busy; }
  function modelDownloadBlock() {
    const selected = modelDownloadSelection(snapshot);
    if (!bridge.capabilities().setup) return 'Model downloads require the Local Image desktop app.';
    if (locked() || snapshot.loading) return 'Wait for the current operation to finish.';
    if (!snapshot.modelCatalogAvailable) return 'Refresh model details before downloading.';
    if (!selected.model || !selected.variant) return 'Choose a model and precision.';
    if (selected.ready) return 'This model is ready in the AI backend.';
    if (selected.filesPresent) return 'Model files are present. Start the AI backend to use them.';
    if (!selected.downloadable) return selected.note || 'Publisher access is required for this model.';
    if (!snapshot.setup?.model_directory && !snapshot.modelDownloads?.model_directory) return 'Choose a model folder first.';
    return '';
  }
  async function run(action: SetupAction) {
    refreshCapabilities();
    const supported = action === 'configureConnection' ? snapshot.capabilities.ready : snapshot.capabilities.setup;
    if (!supported || locked() || (action === 'installRuntime' && snapshot.setup?.portable?.available === false) || (action === 'useInstallation' && !snapshot.selectedInstallation) || (action === 'downloadModel' && modelDownloadBlock())) return;
    const selected = snapshot.selectedInstallation, model = snapshot.selectedModelId, variant = snapshot.selectedVariant;
    update({ pendingAction: action, error: '', message: '' });
    bridge.setOperationPending(true);
    try {
      // The native bridge owns picker cancellation and deadlines. No timeout,
      // fetch abort or automatic retry is introduced around a dialog request.
      const result = action === 'useInstallation' ? await bridge.useInstallation(selected) : action === 'downloadModel' ? await bridge.downloadModel(model, variant) : await bridge[action]();
      if (result === null || result === undefined) return;
      if (disposed) return;
      await refresh();
      if (!snapshot.error) update({ message: action === 'ejectModels' ? 'GPU unload requested. Model files remain on disk.' : action === 'downloadModel' ? snapshot.modelDownloads?.message || 'Model download started.' : 'Setup updated.' });
    } catch (error) { update({ error: error instanceof Error ? error.message : 'The desktop setup command failed.' }); }
    finally { bridge.setOperationPending(false); update({ pendingAction: null }); schedule(); }
  }
  function setDensity(density: InterfaceDensity) { bridge.setDensity(density); update({ preferences: { ...snapshot.preferences, density } }); }
  function setAskBeforeOverwrite(value: boolean) { bridge.setOverwritePreference(value); update({ preferences: { ...snapshot.preferences, askBeforeOverwrite: value } }); }
  async function setHideHardwareGuide(value: boolean) {
    if (snapshot.savingHardwarePreference || disposed) return;
    const previous = snapshot.hideHardwareGuide;
    update({hideHardwareGuide: value, savingHardwarePreference: true, error: ''});
    try {
      const saved = await api.saveHardwarePreference(value);
      if (saved.dont_show_again !== value) throw Error('The hardware guide preference was not saved.');
      writePreference('local-image.hardware-guide.v1', value ? '1' : '0');
    } catch (error) { update({hideHardwareGuide: previous, error: error instanceof Error ? error.message : 'Could not save the hardware guide preference.'}); }
    finally { update({savingHardwarePreference: false}); }
  }
  async function maybeFirstRun() {
    if (snapshot.view || bridge.editorBusy()) return;
    refreshCapabilities();
    if (!snapshot.setup && snapshot.capabilities.setup) await refresh();
    if (snapshot.view || bridge.editorBusy() || disposed) return;
    const requestEpoch = epoch;
    let hidden = readPreference('local-image.hardware-guide.v1') === '1';
    try { const preference = await api.hardwarePreference(); if (typeof preference.dont_show_again === 'boolean') hidden = preference.dont_show_again; } catch { /* Retain legacy browser behavior while the backend is unavailable. */ }
    if (snapshot.view || bridge.editorBusy() || disposed || requestEpoch !== epoch) return;
    update({hideHardwareGuide: hidden});
    firstSetupPending = snapshot.capabilities.setup && ['discover', 'portable'].includes(snapshot.setup?.setup_mode ?? '') && snapshot.setup?.service?.ready !== true && readPreference('local-image.first-ai-setup.v1') !== '1';
    if (!hidden) await open('hardware');
    else offerFirstSetup();
  }
  return {
    getSnapshot: () => snapshot, subscribe: (listener: () => void) => { listeners.add(listener); return () => { listeners.delete(listener); }; },
    open, close, refresh, reloadConfiguration: refresh, maybeFirstRun, refreshCapabilities, run, locked, modelDownloadBlock, isOpen: () => snapshot.view !== null,
    selectModel: (id: string) => {
      if (locked() || snapshot.loading) return;
      const model = snapshot.models.find(item => item.id === id); if (!model) return;
      update({selectedModelId:id, selectedVariant:model.variants?.find(item => item.id === model.defaults?.variant)?.id || model.variants?.[0]?.id || ''});
    },
    selectVariant: (id: string) => {if (!locked() && !snapshot.loading && modelDownloadSelection(snapshot).model?.variants?.some(variant => variant.id === id)) update({selectedVariant:id});},
    setDensity, setAskBeforeOverwrite, setHideHardwareGuide, selectInstallation: (selectedInstallation: string) => update({ selectedInstallation }),
    browseModels: () => { close(); return options.browseModels?.(); },
    continueHardware: close,
    startTask: (workspace: 'retouch' | 'cutout' | 'generate' | 'setup') => { firstSetupPending = false; writePreference('local-image.first-ai-setup.v1', '1'); if (workspace === 'setup') return open('settings', 'ai'); close(); return options.startTask?.(workspace); },
    dispose: () => { disposed = true; ++epoch; stopPolling(); listeners.clear(); },
  };
}
export type SettingsController = ReturnType<typeof createSettingsController>;
