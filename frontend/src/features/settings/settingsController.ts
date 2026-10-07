import { createSettingsApi, type SettingsApi } from './settingsApi.ts';
import type {
  AcceptedConfiguration,
  InterfaceDensity,
  SettingsBridge,
  SettingsSnapshot,
  SettingsView,
  SetupAction,
  UpdateStatus,
} from './types.ts';

const UPDATE_CHECK_KEY = 'local-image.update-check.v1';
const UPDATE_CHECK_INTERVAL = 24 * 60 * 60 * 1000;

function immutable<T>(value: T): T {
  if (value && typeof value === 'object' && !Object.isFrozen(value)) {
    for (const child of Object.values(value)) immutable(child);
    Object.freeze(value);
  }
  return value;
}
export function createSettingsController(options: {
  token: string;
  bridge: SettingsBridge;
  api?: SettingsApi;
  browseModels?: () => unknown;
  startTask?: (workspace: 'retouch' | 'cutout' | 'generate') => unknown;
  storage?: Pick<Storage, 'getItem' | 'setItem'>;
  timers?: { set: (callback: () => void, milliseconds: number) => unknown; clear: (id: unknown) => void };
}) {
  const { bridge } = options;
  const api = options.api ?? createSettingsApi(options.token);
  const timers = options.timers ?? {
    set: (callback: () => void, milliseconds: number) => setTimeout(callback, milliseconds),
    clear: (id: unknown) => clearTimeout(id as ReturnType<typeof setTimeout>),
  };
  const listeners = new Set<() => void>();
  let epoch = 0,
    timer: unknown = null,
    disposed = false,
    firstSetupPending = false;
  let snapshot: SettingsSnapshot = immutable({
    view: null,
    requestedSection: 'general',
    loading: false,
    pendingAction: null,
    setup: null,
    hardware: null,
    capabilities: bridge.capabilities(),
    preferences: bridge.preferences(),
    error: '',
    message: '',
    showInstallations: false,
    selectedInstallation: '',
    update: null,
    updateStep: null,
    updateError: '',
  });
  let updateTimer: unknown = null;
  function update(change: Partial<SettingsSnapshot>) {
    if (disposed) return;
    snapshot = immutable({ ...snapshot, ...structuredClone(change) });
    for (const listener of listeners) listener();
  }
  function stopPolling() {
    if (timer !== null) timers.clear(timer);
    timer = null;
  }
  function schedule() {
    stopPolling();
    if (
      !disposed &&
      snapshot.view === 'settings' &&
      (snapshot.setup?.job?.status === 'running' || snapshot.setup?.service?.starting)
    ) {
      timer = timers.set(() => {
        timer = null;
        void refresh();
      }, 1500);
    }
  }
  function refreshCapabilities() {
    const capabilities = bridge.capabilities();
    if (capabilities.ready !== snapshot.capabilities.ready || capabilities.setup !== snapshot.capabilities.setup)
      update({ capabilities });
  }
  async function refresh(detect = false) {
    const requestEpoch = ++epoch;
    refreshCapabilities();
    update({
      loading: true,
      error: '',
      ...(detect ? { showInstallations: true, message: 'Looking for ComfyUI on this PC…' } : {}),
    });
    const results = await Promise.allSettled([api.settings(), api.setup(detect), api.status(), api.qwen()]);
    if (disposed || requestEpoch !== epoch) return;
    const [settings, setup, status, qwen] = results;
    const accepted: AcceptedConfiguration = {};
    const validSettings =
      settings.status === 'fulfilled' &&
      Array.isArray(settings.value.models) &&
      settings.value.models.some(model => model.id === 'klein');
    if (settings.status === 'fulfilled' && validSettings) accepted.settings = settings.value;
    if (setup.status === 'fulfilled') accepted.setup = setup.value;
    accepted.status = status.status === 'fulfilled' ? status.value : { ready: false, retouch_ready: false };
    accepted.qwen = qwen.status === 'fulfilled' ? qwen.value : null;
    bridge.acceptConfiguration(accepted);
    const errors = results
      .filter(result => result.status === 'rejected')
      .map(result => (result as PromiseRejectedResult).reason?.message || 'The local backend did not respond.');
    if (settings.status === 'fulfilled' && !validSettings)
      errors.push('Could not read FLUX settings. Refresh the local backend state.');
    const nextSetup = accepted.setup ?? snapshot.setup;
    const candidates = nextSetup?.installations ?? [];
    const selectedInstallation = candidates.some(item => item.id === snapshot.selectedInstallation)
      ? snapshot.selectedInstallation
      : (nextSetup?.installation?.id ?? candidates[0]?.id ?? '');
    update({
      loading: false,
      setup: nextSetup,
      selectedInstallation,
      error: [...new Set(errors)].join(' '),
      ...(detect
        ? {
            message: candidates.length
              ? 'Choose an installation.'
              : 'No installation found. Choose a folder or install a dedicated copy.',
          }
        : {}),
    });
    schedule();
  }
  async function open(view: Exclude<SettingsView, null>, requestedSection: 'general' | 'ai' = 'general') {
    stopPolling();
    ++epoch;
    refreshCapabilities();
    update({ view, requestedSection, error: '', message: '', preferences: bridge.preferences(), loading: false });
    if (view === 'settings') await refresh();
    else if (view === 'hardware') {
      const requestEpoch = ++epoch;
      update({ loading: true });
      try {
        const hardware = await api.hardware();
        if (!disposed && requestEpoch === epoch) update({ hardware, loading: false });
      } catch (error) {
        if (!disposed && requestEpoch === epoch)
          update({
            loading: false,
            error: error instanceof Error ? error.message : 'Hardware detection is unavailable.',
          });
      }
    }
  }
  function readPreference(key: string) {
    try {
      return (options.storage ?? localStorage).getItem(key);
    } catch {
      return null;
    }
  }
  function writePreference(key: string, value: string) {
    try {
      (options.storage ?? localStorage).setItem(key, value);
    } catch {
      /* Preference storage may be unavailable. */
    }
  }
  function offerFirstSetup() {
    if (!firstSetupPending || snapshot.view || bridge.editorBusy() || !bridge.capabilities().setup) return;
    firstSetupPending = false;
    writePreference('local-image.first-ai-setup.v1', '1');
    update({ showInstallations: snapshot.setup?.setup_mode === 'discover' });
    void open('settings', 'ai');
  }
  function close() {
    const wasHardware = snapshot.view === 'hardware';
    ++epoch;
    stopPolling();
    update({ view: null, loading: false });
    bridge.focusCanvas();
    if (wasHardware) offerFirstSetup();
  }
  function locked() {
    return (
      bridge.editorBusy() ||
      !!snapshot.pendingAction ||
      snapshot.setup?.job?.status === 'running' ||
      !!snapshot.setup?.service?.starting ||
      !!snapshot.setup?.service?.busy
    );
  }
  async function run(action: SetupAction) {
    refreshCapabilities();
    const supported = action === 'configureConnection' ? snapshot.capabilities.ready : snapshot.capabilities.setup;
    if (!supported || locked() || (action === 'useInstallation' && !snapshot.selectedInstallation)) return;
    const selected = snapshot.selectedInstallation;
    update({ pendingAction: action, error: '', message: '' });
    bridge.setOperationPending(true);
    try {
      // The native bridge owns picker cancellation and deadlines. No timeout,
      // fetch abort or automatic retry is introduced around a dialog request.
      const result = action === 'useInstallation' ? await bridge.useInstallation(selected) : await bridge[action]();
      if (result === null || result === undefined) return;
      if (disposed) return;
      await refresh();
      if (!snapshot.error)
        update({
          message: action === 'ejectModels' ? 'GPU unload requested. Model files remain on disk.' : 'Setup updated.',
        });
    } catch (error) {
      update({ error: error instanceof Error ? error.message : 'The desktop setup command failed.' });
    } finally {
      bridge.setOperationPending(false);
      update({ pendingAction: null });
      schedule();
    }
  }
  function setDensity(density: InterfaceDensity) {
    bridge.setDensity(density);
    update({ preferences: { ...snapshot.preferences, density } });
  }
  function setAskBeforeOverwrite(value: boolean) {
    bridge.setOverwritePreference(value);
    update({ preferences: { ...snapshot.preferences, askBeforeOverwrite: value } });
  }
  function stopUpdatePolling() {
    if (updateTimer !== null) timers.clear(updateTimer);
    updateTimer = null;
  }
  function acceptUpdate(status: UpdateStatus) {
    status.release ??= null;
    status.download ??= { status: 'idle', received: 0, total: 0, error: '' };
    status.check_error ??= '';
    status.installer_ready = !!status.installer_ready;
    status.available = !!status.available;
    status.release_page ||= 'https://github.com/zdbosoxfan/local-image/releases';
    if (disposed) return;
    stopUpdatePolling();
    update({ update: status });
    // Keep following a download the backend is still writing; a failed poll
    // retries rather than leaving the progress frozen.
    if (status.download.status === 'downloading') pollUpdate(1000);
  }
  function pollUpdate(delay: number) {
    updateTimer = timers.set(() => {
      updateTimer = null;
      void api.update().then(acceptUpdate, () => {
        if (!disposed && snapshot.update?.download.status === 'downloading') pollUpdate(3000);
      });
    }, delay);
  }
  /** Quiet startup check: a read that refreshes at most once a day and never
   * downloads anything. The button performs an explicit check. */
  async function checkForUpdates(quiet = false) {
    if (disposed || snapshot.updateStep) return;
    if (quiet) {
      const last = Number(readPreference(UPDATE_CHECK_KEY) ?? 0);
      const refresh = !Number.isFinite(last) || Date.now() - last >= UPDATE_CHECK_INTERVAL;
      try {
        const status = await api.update(refresh);
        if (disposed) return;
        // Only a check that reached GitHub counts towards the daily interval.
        if (refresh && !status.check_error) writePreference(UPDATE_CHECK_KEY, String(Date.now()));
        acceptUpdate(status);
      } catch {
        /* Startup checks stay silent; the Settings button reports problems. */
      }
      return;
    }
    update({ updateStep: 'check', updateError: '' });
    try {
      const status = await api.checkUpdate();
      if (disposed) return;
      if (!status.check_error) writePreference(UPDATE_CHECK_KEY, String(Date.now()));
      acceptUpdate(status);
    } catch (error) {
      if (!disposed) update({ updateError: error instanceof Error ? error.message : 'The update check failed.' });
    } finally {
      if (!disposed) update({ updateStep: null });
    }
  }
  async function downloadUpdate() {
    if (disposed || snapshot.updateStep || !snapshot.update?.available) return;
    update({ updateStep: 'download', updateError: '' });
    try {
      acceptUpdate(await api.downloadUpdate());
    } catch (error) {
      if (!disposed) update({ updateError: error instanceof Error ? error.message : 'The update download failed.' });
    } finally {
      if (!disposed) update({ updateStep: null });
    }
  }
  async function installUpdate() {
    refreshCapabilities();
    if (disposed || snapshot.updateStep || !snapshot.update?.installer_ready || !snapshot.capabilities.setup) return;
    update({ updateStep: 'install', updateError: '' });
    // The host closes the window through the unsaved-edits review, which the
    // editor refuses while a dialog is open, so Settings must close first.
    close();
    bridge.setOperationPending(true);
    try {
      await bridge.installUpdate();
    } catch (error) {
      if (!disposed) {
        // Reopen Settings so the problem is visible where the button lives.
        await open('settings');
        update({ updateError: error instanceof Error ? error.message : 'The update could not be started.' });
      }
    } finally {
      bridge.setOperationPending(false);
      if (!disposed) update({ updateStep: null });
    }
  }
  function rememberGuide() {
    writePreference('local-image.hardware-guide.v1', '1');
  }
  async function maybeFirstRun() {
    if (snapshot.view || bridge.editorBusy()) return;
    refreshCapabilities();
    if (!snapshot.setup && snapshot.capabilities.setup) await refresh();
    if (snapshot.view || bridge.editorBusy() || disposed) return;
    firstSetupPending =
      snapshot.capabilities.setup &&
      ['discover', 'portable'].includes(snapshot.setup?.setup_mode ?? '') &&
      snapshot.setup?.service?.ready !== true &&
      readPreference('local-image.first-ai-setup.v1') !== '1';
    if (readPreference('local-image.hardware-guide.v1') !== '1') await open('hardware');
    else offerFirstSetup();
  }
  return {
    getSnapshot: () => snapshot,
    subscribe: (listener: () => void) => {
      listeners.add(listener);
      return () => {
        listeners.delete(listener);
      };
    },
    open,
    close,
    refresh,
    reloadConfiguration: refresh,
    maybeFirstRun,
    refreshCapabilities,
    run,
    locked,
    isOpen: () => snapshot.view !== null,
    setDensity,
    setAskBeforeOverwrite,
    selectInstallation: (selectedInstallation: string) => update({ selectedInstallation }),
    checkForUpdates,
    downloadUpdate,
    installUpdate,
    browseModels: () => {
      close();
      return options.browseModels?.();
    },
    continueHardware: () => {
      rememberGuide();
      close();
    },
    startTask: (workspace: 'retouch' | 'cutout' | 'generate' | 'setup') => {
      firstSetupPending = false;
      rememberGuide();
      writePreference('local-image.first-ai-setup.v1', '1');
      if (workspace === 'setup') return open('settings', 'ai');
      close();
      return options.startTask?.(workspace);
    },
    dispose: () => {
      disposed = true;
      ++epoch;
      stopPolling();
      stopUpdatePolling();
      listeners.clear();
    },
  };
}
export type SettingsController = ReturnType<typeof createSettingsController>;
