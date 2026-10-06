import { test } from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';

test('model native facade fixes action names and drops unapproved download fields', async () => {
  const source = fs.readFileSync(new URL('../../backend/frontend/model-bridge.js', import.meta.url), 'utf8');
  const calls: Array<{ action: string; details: unknown }> = [];
  const context = vm.createContext({
    window: { __LOCAL_IMAGE_REACT__: true },
    nativeSetup: true,
    busy: false,
    closeInProgress: false,
    viewport: { focus() {} },
    resetTransientInput() {},
    nativeRequest: async (action: string, files: unknown, details: unknown) => {
      assert.equal(files, null);
      calls.push({ action, details });
      return null;
    },
  });
  vm.runInContext(source, context);
  const bridge = context.window.LocalImageModelBridge;
  await bridge.chooseModelDirectory();
  await bridge.startBackend();
  await bridge.downloadModel('qwen', 'int8');
  await bridge.downloadLora({
    model: 'qwen',
    repo_id: 'publisher/style',
    filename: 'style.safetensors',
    revision: 'a'.repeat(40),
    allow_unverified: true,
    path: 'C:/unapproved',
    url: 'https://unapproved.example/weights',
  });
  assert.deepEqual(
    calls.map(call => call.action),
    ['setupChooseModelDirectory', 'setupStart', 'setupDownloadGenerationModel', 'loraDownload'],
  );
  assert.deepEqual(JSON.parse(JSON.stringify(calls.at(-1)?.details)), {
    model: 'qwen',
    repo_id: 'publisher/style',
    filename: 'style.safetensors',
    revision: 'a'.repeat(40),
    allow_unverified: true,
  });
  assert.equal('request' in bridge, false);
  assert.equal('postMessage' in bridge, false);
  context.nativeSetup = false;
  await assert.rejects(bridge.chooseModelDirectory(), /desktop host/);
  assert.equal(calls.length, 4);
});
