import { test } from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';

test('settings bridge exposes fixed allowlisted native operations without listeners, paths or deadlines', async () => {
  const source = fs.readFileSync(new URL('../../backend/frontend/settings-bridge.js', import.meta.url), 'utf8');
  const calls: Array<{ action: string; files: unknown; details: unknown }> = [];
  const preferences = new Map<string, string>();
  const context = vm.createContext({ window: { __LOCAL_IMAGE_REACT__: true }, nativeReady: true, nativeSetup: true, busy: false, closeInProgress: false,
    askBeforeOverwrite: true, OVERWRITE_PREFERENCE: 'local-remove-ask-before-overwrite', settingsSaving: false, setupRequestBusy: false,
    models: [], modelId: '', settingsLoaded: false, ready: false, retouchReady: false, qwenStatus: null, setupState: null,
    document: { documentElement: { dataset: {} } }, viewport: { focus() {} }, resetTransientInput() {}, controls() {}, publishEditorState() {},
    requestAnimationFrame: (callback: () => void) => callback(), resize() {},
    localStorage: { getItem: (key: string) => preferences.get(key) ?? null, setItem: (key: string, value: string) => { preferences.set(key, value); } },
    nativeRequest: async (action: string, files: unknown, details: unknown) => { calls.push({ action, files, details }); return null; },
  });
  vm.runInContext(source, context);
  const bridge = context.window.LocalImageSettingsBridge;
  for (const method of ['chooseRuntime', 'chooseInstallDirectory', 'installRuntime', 'chooseModelDirectory', 'downloadRemovalModels', 'startBackend', 'ejectModels', 'configureConnection']) await bridge[method]();
  await bridge.useInstallation('candidate-1');
  await bridge.downloadModel('qwen','int8');
  assert.deepEqual(calls.map(call => call.action), ['setupChooseComfyDirectory', 'setupChooseInstallDirectory', 'setupInstall', 'setupChooseModelDirectory', 'setupDownloadModels', 'setupStart', 'setupEject', 'configureAi', 'setupUseInstallation', 'setupDownloadGenerationModel']);
  assert.ok(calls.every(call => call.files === null));
  assert.equal(JSON.stringify(calls.at(-2)?.details), JSON.stringify({ installation_id: 'candidate-1' }));
  assert.equal(JSON.stringify(calls.at(-1)?.details), JSON.stringify({model:'qwen',variant:'int8'}));
  assert.equal('nativeRequest' in bridge, false); assert.equal('request' in bridge, false);
  bridge.setOverwritePreference(false); assert.equal(preferences.get('local-remove-ask-before-overwrite'), 'false'); assert.equal(context.askBeforeOverwrite, false);
  bridge.setDensity('large'); assert.equal(context.document.documentElement.dataset.uiDensity, 'large');
  bridge.acceptConfiguration({ settings: { models: [{ id: 'klein', available: true }, { id: 'heal' }] }, status: { ready: true, retouch_ready: true }, qwen: null, setup: { service: { ready: true } } });
  assert.equal(context.settingsLoaded, true); assert.equal(context.ready, true); assert.equal(context.qwenStatus, null);
  context.nativeSetup = false;
  await assert.rejects(bridge.installRuntime(), /desktop host/);
  await assert.rejects(bridge.downloadModel('qwen','int8'), /desktop host/);
  assert.equal(calls.length, 10, 'Unsupported setup capability does not send a native request');
});
