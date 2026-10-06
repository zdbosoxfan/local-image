/** Extraction of editor.js nativeRequest/connectNative/handleNativeClose.
 * The host still owns origin validation, pickers, paths and launcher secrets.
 * This closure exposes only the existing fixed protocol operations. */
export interface NativeCapabilities {
  readonly ready: boolean;
  readonly version: number;
  readonly projects: boolean;
  readonly closeRequests: boolean;
  readonly setup: boolean;
  readonly batch: boolean;
}
export interface NativeMessageEvent {
  data: unknown;
}
export interface NativeTransport {
  addEventListener(type: 'message', listener: (event: NativeMessageEvent) => void): void;
  removeEventListener?(type: 'message', listener: (event: NativeMessageEvent) => void): void;
  postMessage(message: unknown): void;
  postMessageWithAdditionalObjects?(message: unknown, objects: readonly File[]): void;
}
export type NativeResult = Record<string, unknown>;
export interface NativeProjectSave {
  session_id: string;
  revision: number;
  saveAs?: boolean;
}
export interface NativeBatchExport {
  job_id: string;
  item_ids: readonly string[];
}
export interface NativeLoraDownload {
  model: string;
  repo_id: string;
  filename: string;
  revision: string;
  allow_unverified?: boolean;
}
export interface NativeBridgeOptions {
  transport?: NativeTransport | null;
  onCloseRequest(id: string): boolean | Promise<boolean>;
  onError?(error: Error): void;
  clock?: { now(): number; setTimeout(callback: () => void, delay: number): unknown; clearTimeout(id: unknown): void };
}
type NativeAction =
  | 'ready'
  | 'openFiles'
  | 'openFolder'
  | 'openProject'
  | 'saveProject'
  | 'drop'
  | 'chooseBackgroundFolder'
  | 'batchExportFolder'
  | 'configureAi'
  | 'setupUseInstallation'
  | 'setupChooseComfyDirectory'
  | 'setupChooseModelDirectory'
  | 'setupChooseInstallDirectory'
  | 'setupInstall'
  | 'setupDownloadModels'
  | 'setupDownloadQwen'
  | 'setupDownloadGenerationModel'
  | 'loraDownload'
  | 'setupStart'
  | 'setupEject';
// These ten owned dialogs deliberately have no machine-operation deadline.
const DIALOG_ACTIONS = new Set<NativeAction>([
  'batchExportFolder',
  'openFiles',
  'openFolder',
  'openProject',
  'saveProject',
  'chooseBackgroundFolder',
  'configureAi',
  'setupChooseComfyDirectory',
  'setupChooseModelDirectory',
  'setupChooseInstallDirectory',
]);
const unavailable: NativeCapabilities = Object.freeze({
  ready: false,
  version: 0,
  projects: false,
  closeRequests: false,
  setup: false,
  batch: false,
});
const owners = new WeakMap<NativeTransport, symbol>();
export function createNativeBridge(options: NativeBridgeOptions) {
  const transport =
    options.transport !== undefined
      ? options.transport
      : typeof window === 'undefined'
        ? null
        : ((window as Window & { chrome?: { webview?: NativeTransport } }).chrome?.webview ?? null);
  const owner = Symbol('native-bridge');
  if (transport) {
    if (owners.has(transport)) throw Error('This native transport already has an owner.');
    owners.set(transport, owner);
  }
  const clock = options.clock ?? {
    now: () => Date.now(),
    setTimeout: (callback: () => void, delay: number) => setTimeout(callback, delay),
    clearTimeout: (id: unknown) => clearTimeout(id as ReturnType<typeof setTimeout>),
  };
  const pending = new Map<
    string,
    { resolve(value: NativeResult | null): void; reject(error: Error): void; timer: unknown | null }
  >();
  const closeRequests = new Set<string>(),
    closeReviews = new Set<string>(),
    subscribers = new Set<() => void>();
  let requestNumber = 0,
    listening = false,
    disposed = false,
    capabilities = unavailable,
    connection: Promise<NativeCapabilities> | null = null;
  function publish(next: NativeCapabilities) {
    if (disposed) return;
    if ((Object.keys(next) as Array<keyof NativeCapabilities>).every(key => next[key] === capabilities[key])) return;
    capabilities = Object.freeze(next);
    for (const listener of subscribers) listener();
  }
  function request(
    action: NativeAction,
    details: Record<string, unknown> = {},
    files?: readonly File[],
  ): Promise<NativeResult | null> {
    if (disposed) return Promise.reject(Error('The desktop bridge has been disposed.'));
    if (!transport) return Promise.reject(Error('Open the Local Image desktop app for this command.'));
    const id = `local-remove-${clock.now()}-${++requestNumber}`;
    return new Promise((resolve, reject) => {
      const timer = DIALOG_ACTIONS.has(action)
        ? null
        : clock.setTimeout(
            () => {
              pending.delete(id);
              reject(Error('The desktop command timed out. Please try again.'));
            },
            action === 'ready' ? 3000 : 600000,
          );
      pending.set(id, { resolve, reject, timer });
      try {
        const message = { id, action, ...details };
        if (files) {
          if (typeof transport.postMessageWithAdditionalObjects !== 'function')
            throw Error('Use File → Open in this desktop window.');
          transport.postMessageWithAdditionalObjects(message, files);
        } else transport.postMessage(message);
      } catch (error) {
        if (timer !== null) clock.clearTimeout(timer);
        pending.delete(id);
        reject(error instanceof Error ? error : Error('The desktop command failed.'));
      }
    });
  }
  function command(
    action: NativeAction,
    details: Record<string, unknown> = {},
    capability: 'ready' | 'projects' | 'setup' | 'batch' = 'ready',
    files?: readonly File[],
  ) {
    if (disposed) return Promise.reject(Error('The desktop bridge has been disposed.'));
    if (!transport || !capabilities.ready)
      return Promise.reject(Error('Open the Local Image desktop app for this command.'));
    if (!capabilities[capability])
      return Promise.reject(Error('The current desktop host does not support this command.'));
    return request(action, details, files);
  }
  async function reviewClose(id: string) {
    if (!transport || disposed || closeRequests.has(id)) return;
    closeRequests.add(id);
    closeReviews.add(id);
    let approved = false;
    try {
      approved = (await options.onCloseRequest(id)) === true;
    } catch (error) {
      options.onError?.(error instanceof Error ? error : Error('The document close review failed.'));
    } finally {
      closeReviews.delete(id);
    }
    // No timer, timeout approval, catch approval, or automatic retry. The host
    // receives the exact correlated decision, including explicit cancellation.
    if (!disposed) {
      try {
        transport.postMessage({ action: 'closeReady', id, approved });
      } catch (error) {
        options.onError?.(
          error instanceof Error ? error : Error('The close decision could not reach the desktop host.'),
        );
      }
    }
  }
  function onMessage(event: NativeMessageEvent) {
    if (disposed || !event.data || typeof event.data !== 'object') return;
    const data = event.data as {
      type?: unknown;
      action?: unknown;
      id?: unknown;
      error?: unknown;
      cancelled?: unknown;
      result?: NativeResult | null;
    };
    if (data.type !== 'local-remove-native' || typeof data.id !== 'string') return;
    if (data.action === 'requestClose') {
      if (data.id && data.id.length <= 128) void reviewClose(data.id);
      return;
    }
    const waiting = pending.get(data.id);
    if (!waiting) return;
    if (waiting.timer !== null) clock.clearTimeout(waiting.timer);
    pending.delete(data.id);
    if (data.error) waiting.reject(Error(typeof data.error === 'string' ? data.error : 'The desktop command failed.'));
    else waiting.resolve(data.cancelled ? null : (data.result ?? null));
  }
  function connect(): Promise<NativeCapabilities> {
    if (disposed) return Promise.reject(Error('The desktop bridge has been disposed.'));
    if (connection) return connection;
    if (!transport) return (connection = Promise.resolve(capabilities));
    if (!listening) {
      transport.addEventListener('message', onMessage);
      listening = true;
    }
    connection = request('ready')
      .then(result => {
        const ready = result?.native === true;
        publish({
          ready,
          version: typeof result?.version === 'number' && Number.isInteger(result.version) ? result.version : 0,
          projects: ready && result?.projects === true,
          closeRequests: ready && result?.closeRequests === true,
          setup: ready && result?.setup === true,
          batch: ready && result?.batch === true,
        });
        return capabilities;
      })
      .catch(() => {
        publish(unavailable);
        return capabilities;
      });
    return connection;
  }
  function dispose() {
    if (disposed) return;
    disposed = true;
    if (transport && listening) transport.removeEventListener?.('message', onMessage);
    listening = false;
    for (const value of pending.values()) {
      if (value.timer !== null) clock.clearTimeout(value.timer);
      value.reject(Error('The desktop bridge was disposed before the command completed.'));
    }
    pending.clear();
    subscribers.clear();
    closeReviews.clear();
    closeRequests.clear();
    if (transport && owners.get(transport) === owner) owners.delete(transport);
  }
  return Object.freeze({
    connect,
    getSnapshot: () => capabilities,
    capabilities: () => capabilities,
    subscribe: (listener: () => void) => {
      subscribers.add(listener);
      return () => {
        subscribers.delete(listener);
      };
    },
    getPendingCount: () => pending.size,
    getCloseReviewCount: () => closeReviews.size,
    dispose,
    openFiles: () => command('openFiles'),
    openFolder: () => command('openFolder'),
    openProject: () => command('openProject', {}, 'projects'),
    drop: (files: readonly File[]) => command('drop', {}, 'ready', [...files]),
    saveProject: (value: NativeProjectSave) =>
      command(
        'saveProject',
        { session_id: value.session_id, revision: value.revision, ...(value.saveAs ? { saveAs: true } : {}) },
        'projects',
      ),
    chooseBackgroundFolder: () => command('chooseBackgroundFolder'),
    batchExportFolder: (value: NativeBatchExport) =>
      command('batchExportFolder', { job_id: value.job_id, item_ids: [...value.item_ids] }, 'batch'),
    configureAi: () => command('configureAi'),
    setupUseInstallation: (installationId: string) =>
      command('setupUseInstallation', { installation_id: installationId }, 'setup'),
    setupChooseComfyDirectory: () => command('setupChooseComfyDirectory', {}, 'setup'),
    setupChooseModelDirectory: () => command('setupChooseModelDirectory', {}, 'setup'),
    setupChooseInstallDirectory: () => command('setupChooseInstallDirectory', {}, 'setup'),
    setupInstall: () => command('setupInstall', {}, 'setup'),
    setupDownloadModels: () => command('setupDownloadModels', {}, 'setup'),
    setupDownloadQwen: (variant: 'int8' | 'bf16') => command('setupDownloadQwen', { variant }, 'setup'),
    setupDownloadGenerationModel: (model: string, variant: string) =>
      command('setupDownloadGenerationModel', { model, variant }, 'setup'),
    loraDownload: (value: NativeLoraDownload) =>
      command(
        'loraDownload',
        {
          model: value.model,
          repo_id: value.repo_id,
          filename: value.filename,
          revision: value.revision,
          allow_unverified: value.allow_unverified === true,
        },
        'setup',
      ),
    setupStart: () => command('setupStart', {}, 'setup'),
    setupEject: () => command('setupEject', {}, 'setup'),
  });
}
export type NativeBridge = ReturnType<typeof createNativeBridge>;
