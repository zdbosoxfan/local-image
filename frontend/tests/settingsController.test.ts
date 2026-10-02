import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createSettingsController } from '../src/features/settings/settingsController.ts';
import { createSettingsApi } from '../src/features/settings/settingsApi.ts';
import type { SettingsApi } from '../src/features/settings/settingsApi.ts';
import type { AcceptedConfiguration, SettingsBridge, SetupState } from '../src/features/settings/types.ts';

function deferred<T>() { let resolve!: (value: T) => void; let reject!: (error: Error) => void; const promise = new Promise<T>((yes, no) => { resolve = yes; reject = no; }); return { promise, resolve, reject }; }
function fixture() {
  const accepted: AcceptedConfiguration[] = [], actions: string[] = [], pending: boolean[] = [];
  const storageValues = new Map<string, string>();
  const schedules: Array<{ callback: () => void; milliseconds: number }> = [];
  let data: SetupState = { service: { ready: false, running: false }, models: [], model_directory: '' };
  let hideHardware: boolean | null = null;
  const bridge: SettingsBridge = {
    capabilities: () => ({ ready: true, setup: true }), editorBusy: () => false,
    preferences: () => ({ askBeforeOverwrite: true, density: 'comfortable' }),
    acceptConfiguration: value => { accepted.push(value); }, setOperationPending: value => { pending.push(value); },
    setOverwritePreference: value => { actions.push(`overwrite:${value}`); }, setDensity: value => { actions.push(`density:${value}`); }, focusCanvas: () => {},
    chooseRuntime: async () => { actions.push('chooseRuntime'); return null; }, chooseInstallDirectory: async () => null,
    installRuntime: async () => { actions.push('installRuntime'); return {}; }, chooseModelDirectory: async () => null,
    downloadRemovalModels: async () => { actions.push('downloadRemovalModels'); return {}; }, startBackend: async () => ({}), ejectModels: async () => ({}),
    useInstallation: async id => { actions.push(`useInstallation:${id}`); return {}; }, configureConnection: async () => null,
  };
  const api: SettingsApi = { settings: async () => ({ models: [{ id: 'klein', available: false }] }), setup: async () => structuredClone(data),
    status: async () => ({ ready: false, retouch_ready: true }), qwen: async () => ({ ready: false }), hardware: async () => ({ devices: [], note: 'Planning guidance' }),
    hardwarePreference: async () => ({dont_show_again: hideHardware}), saveHardwarePreference: async value => {hideHardware = value; return {dont_show_again: value};} };
  const controller = createSettingsController({ token: 'per-page-token', bridge, api,
    timers: { set: (callback, milliseconds) => { const entry = { callback, milliseconds }; schedules.push(entry); return entry; }, clear: () => {} },
    storage: { getItem: key => storageValues.get(key) ?? null, setItem: (key, value) => { storageValues.set(key, value); } },
  });
  return { controller, bridge, api, accepted, actions, pending, schedules, storageValues, setSetup: (value: SetupState) => { data = value; } };
}

test('cancelled native picker has no timeout, success message, reload or duplicate dispatch', async () => {
  const f = fixture(); await f.controller.open('settings');
  const dialog = deferred<unknown>(); let calls = 0;
  f.bridge.chooseRuntime = () => { calls++; return dialog.promise; };
  const result = f.controller.run('chooseRuntime');
  assert.equal(f.controller.getSnapshot().pendingAction, 'chooseRuntime');
  assert.equal(f.schedules.length, 0, 'No machine timer wraps an owned dialog');
  await f.controller.run('chooseRuntime'); assert.equal(calls, 1);
  dialog.resolve(null); await result;
  assert.equal(f.controller.getSnapshot().pendingAction, null);
  assert.equal(f.controller.getSnapshot().message, ''); assert.equal(f.controller.getSnapshot().error, '');
  assert.equal(f.accepted.length, 1, 'Cancel does not pretend configuration changed');
  assert.deepEqual(f.pending, [true, false]);
});

test('native command failure is surfaced once without install or download retry', async () => {
  const f = fixture(); await f.controller.open('settings'); let calls = 0;
  f.bridge.installRuntime = async () => { calls++; throw Error('Chosen folder is not writable.'); };
  await f.controller.run('installRuntime');
  assert.equal(calls, 1); assert.match(f.controller.getSnapshot().error, /not writable/);
  assert.equal(f.controller.getSnapshot().pendingAction, null); assert.equal(f.schedules.length, 0);
  await f.controller.refresh(); assert.equal(calls, 1, 'Refresh is read-only');
});

test('out-of-order setup responses after closing and reopening are ignored', async () => {
  const f = fixture(), first = deferred<SetupState>(), second = deferred<SetupState>(); let calls = 0;
  f.api.setup = () => ++calls === 1 ? first.promise : second.promise;
  const oldOpen = f.controller.open('settings'); f.controller.close();
  const newOpen = f.controller.open('settings');
  second.resolve({ model_directory: 'new', service: { ready: true } }); await newOpen;
  first.resolve({ model_directory: 'old', service: { ready: false } }); await oldOpen;
  assert.equal(f.controller.getSnapshot().setup?.model_directory, 'new');
  assert.equal(f.accepted.length, 1); assert.equal(f.accepted[0].setup?.model_directory, 'new');
  assert.ok(Object.isFrozen(f.controller.getSnapshot().setup?.service));
});

test('real reported setup jobs poll with their exact progress; completion stops polling', async () => {
  const f = fixture(); f.setSetup({ job: { status: 'running', progress: 17, message: 'Receiving verified files' }, service: {} });
  await f.controller.open('settings');
  assert.equal(f.schedules.length, 1); assert.equal(f.schedules[0].milliseconds, 1500);
  assert.equal(f.controller.getSnapshot().setup?.job?.progress, 17);
  await f.controller.run('downloadRemovalModels'); assert.deepEqual(f.actions, []);
  f.setSetup({ job: { status: 'complete', progress: 100 }, service: { ready: true, running: true } });
  await f.controller.refresh(); assert.equal(f.schedules.length, 1);
  assert.equal(f.controller.getSnapshot().setup?.job?.status, 'complete');
  assert.deepEqual(f.actions, [], 'Polling does not perform native setup actions');
});

test('status failure disables stale readiness and a later real read recovers it', async () => {
  const f = fixture(); f.api.status = async () => { throw Error('Backend disconnected'); }; f.api.qwen = async () => { throw Error('Qwen status unavailable'); };
  await f.controller.reloadConfiguration();
  assert.equal(f.controller.getSnapshot().view, null);
  assert.equal(f.accepted.at(-1)?.status?.ready, false); assert.equal(f.accepted.at(-1)?.qwen, null);
  assert.match(f.controller.getSnapshot().error, /Backend disconnected/);
  f.api.status = async () => ({ ready: true, retouch_ready: true }); f.api.qwen = async () => ({ ready: true });
  await f.controller.refresh(); assert.equal(f.controller.getSnapshot().error, ''); assert.equal(f.accepted.at(-1)?.status?.ready, true);
});

test('backend inventory requests remain read-only and carry only the page token', async () => {
  const calls: Array<[string, RequestInit | undefined]> = [];
  const api = createSettingsApi('request-specific-token', (async (input, init) => { calls.push([String(input), init]); return new Response('{}', { status: 200, headers: { 'content-type': 'application/json' } }); }) as typeof fetch);
  await Promise.all([api.settings(), api.setup(), api.setup(true), api.status(), api.qwen(), api.hardware(), api.hardwarePreference()]);
  assert.equal(calls.length, 7);
  assert.ok(calls.every(([, init]) => !init?.method || init.method === 'GET'));
  assert.ok(calls.every(([, init]) => (init?.headers as Record<string, string>)['x-local-remove-token'] === 'request-specific-token'));
  assert.ok(calls.every(([path]) => path.startsWith('/api/local-remove/')));
});

test('first-run hardware followed by setup uses existing preferences without automatic setup writes', async () => {
  const f = fixture(); f.setSetup({ setup_mode: 'discover', service: { ready: false }, installations: [{ id: 'one', path: 'C:/ComfyUI' }] });
  await f.controller.reloadConfiguration(); await f.controller.maybeFirstRun();
  assert.equal(f.controller.getSnapshot().view, 'hardware'); assert.deepEqual(f.actions, []);
  f.controller.continueHardware();
  assert.equal(f.controller.getSnapshot().view, 'settings'); assert.equal(f.controller.getSnapshot().requestedSection, 'ai');
  assert.equal(f.storageValues.get('local-image.hardware-guide.v1'), undefined); assert.equal(f.storageValues.get('local-image.first-ai-setup.v1'), '1');
  assert.deepEqual(f.actions, []);
});

test('hardware dismissal survives fresh browser storage and Help can still reopen the guide', async () => {
  const f = fixture(); await f.controller.maybeFirstRun();
  assert.equal(f.controller.getSnapshot().view, 'hardware');
  await f.controller.setHideHardwareGuide(true); f.controller.continueHardware(); f.controller.dispose();
  f.storageValues.clear();
  const restarted = createSettingsController({token:'new-page', bridge:f.bridge, api:f.api, storage:{getItem:()=>null,setItem:()=>{}}});
  await restarted.maybeFirstRun(); assert.equal(restarted.getSnapshot().view,null);
  await restarted.open('hardware'); assert.equal(restarted.getSnapshot().hideHardwareGuide,true);
  await restarted.setHideHardwareGuide(false); restarted.close(); await restarted.maybeFirstRun();
  assert.equal(restarted.getSnapshot().view,'hardware'); restarted.dispose();
});

test('closing without opting out does not suppress hardware guidance; failed saves are visible', async () => {
  const f=fixture(); await f.controller.maybeFirstRun(); f.controller.close();
  await f.controller.maybeFirstRun(); assert.equal(f.controller.getSnapshot().view,'hardware');
  let writes=0; f.api.saveHardwarePreference=async()=>{writes++;throw Error('Could not save preference.');};
  await f.controller.setHideHardwareGuide(true);
  assert.equal(writes,1); assert.equal(f.controller.getSnapshot().hideHardwareGuide,false);
  assert.equal(f.controller.getSnapshot().savingHardwarePreference,false); assert.match(f.controller.getSnapshot().error,/Could not save/);
  assert.equal(f.storageValues.get('local-image.hardware-guide.v1'),undefined); f.controller.dispose();
});

test('explicit persistent preference overrides a legacy browser acknowledgement', async () => {
  const f=fixture(); f.storageValues.set('local-image.hardware-guide.v1','1');
  await f.controller.maybeFirstRun(); assert.equal(f.controller.getSnapshot().view,null);
  f.api.hardwarePreference=async()=>({dont_show_again:false});
  await f.controller.maybeFirstRun(); assert.equal(f.controller.getSnapshot().view,'hardware'); f.controller.dispose();
});

test('hardware preference writes use the token and a boolean-only POST without retries', async () => {
  const calls:RequestInit[]=[];
  const api=createSettingsApi('page-token',(async(_url,init)=>{calls.push(init!);return Response.json({dont_show_again:true});}) as typeof fetch);
  assert.deepEqual(await api.saveHardwarePreference(true),{dont_show_again:true}); assert.equal(calls.length,1);
  assert.equal(calls[0].method,'POST'); assert.equal((calls[0].headers as Record<string,string>)['x-local-remove-token'],'page-token');
  assert.deepEqual(JSON.parse(calls[0].body as string),{dont_show_again:true});
});

test('browser mode cannot dispatch native setup, and UI preferences remain browser-owned', async () => {
  const f = fixture(); f.bridge.capabilities = () => ({ ready: false, setup: false }); await f.controller.open('settings');
  await f.controller.run('installRuntime'); await f.controller.run('configureConnection'); assert.deepEqual(f.actions, []);
  f.controller.setDensity('large'); f.controller.setAskBeforeOverwrite(false);
  assert.deepEqual(f.actions, ['density:large', 'overwrite:false']);
  assert.equal(f.controller.getSnapshot().preferences.density, 'large'); assert.equal(f.controller.getSnapshot().preferences.askBeforeOverwrite, false);
});
