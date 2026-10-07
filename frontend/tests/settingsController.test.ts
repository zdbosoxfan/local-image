import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createSettingsController } from '../src/features/settings/settingsController.ts';
import { createSettingsApi } from '../src/features/settings/settingsApi.ts';
import type { SettingsApi } from '../src/features/settings/settingsApi.ts';
import type {
  AcceptedConfiguration,
  SettingsBridge,
  SetupState,
  UpdateStatus,
} from '../src/features/settings/types.ts';

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (error: Error) => void;
  const promise = new Promise<T>((yes, no) => {
    resolve = yes;
    reject = no;
  });
  return { promise, resolve, reject };
}
function fixture() {
  const accepted: AcceptedConfiguration[] = [],
    actions: string[] = [],
    pending: boolean[] = [];
  const storageValues = new Map<string, string>();
  const schedules: Array<{ callback: () => void; milliseconds: number }> = [];
  let data: SetupState = { service: { ready: false, running: false }, models: [], model_directory: '' };
  const bridge: SettingsBridge = {
    capabilities: () => ({ ready: true, setup: true }),
    editorBusy: () => false,
    preferences: () => ({ askBeforeOverwrite: true, density: 'comfortable' }),
    acceptConfiguration: value => {
      accepted.push(value);
    },
    setOperationPending: value => {
      pending.push(value);
    },
    setOverwritePreference: value => {
      actions.push(`overwrite:${value}`);
    },
    setDensity: value => {
      actions.push(`density:${value}`);
    },
    focusCanvas: () => {},
    chooseRuntime: async () => {
      actions.push('chooseRuntime');
      return null;
    },
    chooseInstallDirectory: async () => null,
    installRuntime: async () => {
      actions.push('installRuntime');
      return {};
    },
    chooseModelDirectory: async () => null,
    downloadRemovalModels: async () => {
      actions.push('downloadRemovalModels');
      return {};
    },
    startBackend: async () => ({}),
    ejectModels: async () => ({}),
    useInstallation: async id => {
      actions.push(`useInstallation:${id}`);
      return {};
    },
    configureConnection: async () => null,
    installUpdate: async () => {
      actions.push('installUpdate');
      return { ok: true, closing: true };
    },
  };
  let updateStatus: UpdateStatus = {
    current_version: '0.7.0',
    checked_at: null,
    check_error: '',
    available: false,
    release: null,
    download: { status: 'idle', received: 0, total: 0, error: '' },
    installer_ready: false,
    release_page: 'https://github.com/zdbosoxfan/local-image/releases',
  };
  const updateCalls: string[] = [];
  const api: SettingsApi = {
    settings: async () => ({ models: [{ id: 'klein', available: false }] }),
    update: async (refresh = false) => {
      updateCalls.push(refresh ? 'refresh' : 'read');
      return structuredClone(updateStatus);
    },
    checkUpdate: async () => {
      updateCalls.push('check');
      return structuredClone(updateStatus);
    },
    downloadUpdate: async () => {
      updateCalls.push('download');
      updateStatus = { ...updateStatus, download: { status: 'downloading', received: 10, total: 100, error: '' } };
      return structuredClone(updateStatus);
    },
    setup: async () => structuredClone(data),
    status: async () => ({ ready: false, retouch_ready: true }),
    qwen: async () => ({ ready: false }),
    hardware: async () => ({ devices: [], note: 'Planning guidance' }),
  };
  const controller = createSettingsController({
    token: 'per-page-token',
    bridge,
    api,
    timers: {
      set: (callback, milliseconds) => {
        const entry = { callback, milliseconds };
        schedules.push(entry);
        return entry;
      },
      clear: () => {},
    },
    storage: {
      getItem: key => storageValues.get(key) ?? null,
      setItem: (key, value) => {
        storageValues.set(key, value);
      },
    },
  });
  return {
    controller,
    bridge,
    api,
    accepted,
    actions,
    pending,
    schedules,
    storageValues,
    updateCalls,
    setSetup: (value: SetupState) => {
      data = value;
    },
    setUpdate: (value: Partial<UpdateStatus>) => {
      updateStatus = { ...updateStatus, ...value };
    },
  };
}

test('cancelled native picker has no timeout, success message, reload or duplicate dispatch', async () => {
  const f = fixture();
  await f.controller.open('settings');
  const dialog = deferred<unknown>();
  let calls = 0;
  f.bridge.chooseRuntime = () => {
    calls++;
    return dialog.promise;
  };
  const result = f.controller.run('chooseRuntime');
  assert.equal(f.controller.getSnapshot().pendingAction, 'chooseRuntime');
  assert.equal(f.schedules.length, 0, 'No machine timer wraps an owned dialog');
  await f.controller.run('chooseRuntime');
  assert.equal(calls, 1);
  dialog.resolve(null);
  await result;
  assert.equal(f.controller.getSnapshot().pendingAction, null);
  assert.equal(f.controller.getSnapshot().message, '');
  assert.equal(f.controller.getSnapshot().error, '');
  assert.equal(f.accepted.length, 1, 'Cancel does not pretend configuration changed');
  assert.deepEqual(f.pending, [true, false]);
});

test('native command failure is surfaced once without install or download retry', async () => {
  const f = fixture();
  await f.controller.open('settings');
  let calls = 0;
  f.bridge.installRuntime = async () => {
    calls++;
    throw Error('Chosen folder is not writable.');
  };
  await f.controller.run('installRuntime');
  assert.equal(calls, 1);
  assert.match(f.controller.getSnapshot().error, /not writable/);
  assert.equal(f.controller.getSnapshot().pendingAction, null);
  assert.equal(f.schedules.length, 0);
  await f.controller.refresh();
  assert.equal(calls, 1, 'Refresh is read-only');
});

test('out-of-order setup responses after closing and reopening are ignored', async () => {
  const f = fixture(),
    first = deferred<SetupState>(),
    second = deferred<SetupState>();
  let calls = 0;
  f.api.setup = () => (++calls === 1 ? first.promise : second.promise);
  const oldOpen = f.controller.open('settings');
  f.controller.close();
  const newOpen = f.controller.open('settings');
  second.resolve({ model_directory: 'new', service: { ready: true } });
  await newOpen;
  first.resolve({ model_directory: 'old', service: { ready: false } });
  await oldOpen;
  assert.equal(f.controller.getSnapshot().setup?.model_directory, 'new');
  assert.equal(f.accepted.length, 1);
  assert.equal(f.accepted[0].setup?.model_directory, 'new');
  assert.ok(Object.isFrozen(f.controller.getSnapshot().setup?.service));
});

test('real reported setup jobs poll with their exact progress; completion stops polling', async () => {
  const f = fixture();
  f.setSetup({ job: { status: 'running', progress: 17, message: 'Receiving verified files' }, service: {} });
  await f.controller.open('settings');
  assert.equal(f.schedules.length, 1);
  assert.equal(f.schedules[0].milliseconds, 1500);
  assert.equal(f.controller.getSnapshot().setup?.job?.progress, 17);
  await f.controller.run('downloadRemovalModels');
  assert.deepEqual(f.actions, []);
  f.setSetup({ job: { status: 'complete', progress: 100 }, service: { ready: true, running: true } });
  await f.controller.refresh();
  assert.equal(f.schedules.length, 1);
  assert.equal(f.controller.getSnapshot().setup?.job?.status, 'complete');
  assert.deepEqual(f.actions, [], 'Polling does not perform native setup actions');
});

test('status failure disables stale readiness and a later real read recovers it', async () => {
  const f = fixture();
  f.api.status = async () => {
    throw Error('Backend disconnected');
  };
  f.api.qwen = async () => {
    throw Error('Qwen status unavailable');
  };
  await f.controller.reloadConfiguration();
  assert.equal(f.controller.getSnapshot().view, null);
  assert.equal(f.accepted.at(-1)?.status?.ready, false);
  assert.equal(f.accepted.at(-1)?.qwen, null);
  assert.match(f.controller.getSnapshot().error, /Backend disconnected/);
  f.api.status = async () => ({ ready: true, retouch_ready: true });
  f.api.qwen = async () => ({ ready: true });
  await f.controller.refresh();
  assert.equal(f.controller.getSnapshot().error, '');
  assert.equal(f.accepted.at(-1)?.status?.ready, true);
});

test('backend inventory requests remain read-only and carry only the page token', async () => {
  const calls: Array<[string, RequestInit | undefined]> = [];
  const api = createSettingsApi('request-specific-token', (async (input, init) => {
    calls.push([String(input), init]);
    return new Response('{}', { status: 200, headers: { 'content-type': 'application/json' } });
  }) as typeof fetch);
  await Promise.all([api.settings(), api.setup(), api.setup(true), api.status(), api.qwen(), api.hardware()]);
  assert.equal(calls.length, 6);
  assert.ok(calls.every(([, init]) => !init?.method || init.method === 'GET'));
  assert.ok(
    calls.every(
      ([, init]) => (init?.headers as Record<string, string>)['x-local-remove-token'] === 'request-specific-token',
    ),
  );
  assert.ok(calls.every(([path]) => path.startsWith('/api/local-remove/')));
});

test('first-run hardware followed by setup uses existing preferences without automatic setup writes', async () => {
  const f = fixture();
  f.setSetup({ setup_mode: 'discover', service: { ready: false }, installations: [{ id: 'one', path: 'C:/ComfyUI' }] });
  await f.controller.reloadConfiguration();
  await f.controller.maybeFirstRun();
  assert.equal(f.controller.getSnapshot().view, 'hardware');
  assert.deepEqual(f.actions, []);
  f.controller.continueHardware();
  assert.equal(f.controller.getSnapshot().view, 'settings');
  assert.equal(f.controller.getSnapshot().requestedSection, 'ai');
  assert.equal(f.storageValues.get('local-image.hardware-guide.v1'), '1');
  assert.equal(f.storageValues.get('local-image.first-ai-setup.v1'), '1');
  assert.deepEqual(f.actions, []);
});

test('browser mode cannot dispatch native setup, and UI preferences remain browser-owned', async () => {
  const f = fixture();
  f.bridge.capabilities = () => ({ ready: false, setup: false });
  await f.controller.open('settings');
  await f.controller.run('installRuntime');
  await f.controller.run('configureConnection');
  assert.deepEqual(f.actions, []);
  f.controller.setDensity('large');
  f.controller.setAskBeforeOverwrite(false);
  assert.deepEqual(f.actions, ['density:large', 'overwrite:false']);
  assert.equal(f.controller.getSnapshot().preferences.density, 'large');
  assert.equal(f.controller.getSnapshot().preferences.askBeforeOverwrite, false);
});

test('update check, download polling and desktop install follow the verified backend state', async () => {
  const f = fixture();
  const release = {
    version: '0.8.0',
    tag: 'v0.8.0',
    name: 'Local Image 0.8.0',
    notes: 'Notes',
    html_url: 'https://github.com/zdbosoxfan/local-image/releases/tag/v0.8.0',
    published_at: '2026-10-10T00:00:00Z',
    prerelease: true,
    asset_name: 'Local-Image-Setup-0.8.0.exe',
    bytes: 100,
  };
  // The quiet startup check is a read: it refreshes at most once a day, and
  // an unreachable GitHub does not count as today's check.
  f.setUpdate({ check_error: 'Could not reach GitHub to check for updates.' });
  await f.controller.checkForUpdates(true);
  assert.deepEqual(f.updateCalls, ['refresh']);
  assert.equal(f.storageValues.get('local-image.update-check.v1'), undefined);
  f.setUpdate({ check_error: '' });
  await f.controller.checkForUpdates(true);
  assert.deepEqual(f.updateCalls, ['refresh', 'refresh']);
  assert.ok(Number(f.storageValues.get('local-image.update-check.v1')) > 0);
  await f.controller.checkForUpdates(true);
  assert.deepEqual(f.updateCalls, ['refresh', 'refresh', 'read']);
  f.setUpdate({ available: true, release, checked_at: 1 });
  await f.controller.checkForUpdates();
  assert.deepEqual(f.updateCalls, ['refresh', 'refresh', 'read', 'check']);
  assert.equal(f.controller.getSnapshot().update?.available, true);
  assert.equal(f.controller.getSnapshot().updateStep, null);
  // Nothing can be installed until the backend reports a verified installer.
  await f.controller.installUpdate();
  assert.ok(!f.actions.includes('installUpdate'));
  await f.controller.downloadUpdate();
  assert.equal(f.controller.getSnapshot().update?.download.status, 'downloading');
  const poll = f.schedules.at(-1);
  assert.equal(poll?.milliseconds, 1000);
  f.setUpdate({ download: { status: 'ready', received: 100, total: 100, error: '' }, installer_ready: true });
  poll!.callback();
  await new Promise(resolve => setTimeout(resolve, 0));
  assert.equal(f.controller.getSnapshot().update?.installer_ready, true);
  await f.controller.open('settings');
  await f.controller.installUpdate();
  assert.deepEqual(
    f.actions.filter(action => action === 'installUpdate'),
    ['installUpdate'],
  );
  // Settings closes before the host's unsaved-edits review, which the editor
  // would otherwise refuse while a dialog is open.
  assert.equal(f.controller.getSnapshot().view, null);
  assert.equal(f.controller.getSnapshot().updateStep, null);
  assert.deepEqual(f.pending.slice(-2), [true, false]);
  f.bridge.installUpdate = async () => {
    throw new Error('The downloaded installer is missing. Download the update again.');
  };
  await f.controller.open('settings');
  await f.controller.installUpdate();
  assert.equal(f.controller.getSnapshot().view, 'settings');
  assert.match(f.controller.getSnapshot().updateError, /missing/);
});

test('update check failures are shown for manual checks and stay silent at startup', async () => {
  const f = fixture();
  f.api.checkUpdate = async () => {
    throw new Error('GitHub is rate limiting update checks from this PC.');
  };
  f.api.update = async () => {
    throw new Error('offline');
  };
  await f.controller.checkForUpdates(true);
  assert.equal(f.controller.getSnapshot().updateError, '');
  await f.controller.checkForUpdates();
  assert.match(f.controller.getSnapshot().updateError, /rate limiting/);
  assert.equal(f.controller.getSnapshot().updateStep, null);
});
