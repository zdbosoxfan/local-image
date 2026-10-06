import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createModelsController } from '../src/features/models/controller.ts';
import { createModelApi, type ModelApi } from '../src/features/models/api.ts';
import type {
  BrowserModel,
  LoraDraftPort,
  LoraFiles,
  LoraInventory,
  LoraItem,
  LoraSelection,
  ModelBridge,
} from '../src/features/models/types.ts';
const revision = 'a'.repeat(40);
const item = (model = 'qwen', id = 'one'): LoraItem => ({
  id,
  model,
  title: `${model} style ${id}`,
  repo_id: `publisher/${model}`,
  filename: `${id}.safetensors`,
  revision,
  compatibility: 'curated',
  supported: true,
  recommended_strength: 0.7,
});
const files = (model = 'qwen'): LoraFiles => ({
  repo_id: `publisher/${model}`,
  revision,
  files: [
    { ...item(model), filename: 'one.safetensors' },
    { ...item(model, 'other'), filename: 'other.safetensors', compatibility: 'unverified' },
  ],
});
function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (error: Error) => void;
  const promise = new Promise<T>((a, b) => {
    resolve = a;
    reject = b;
  });
  return { promise, resolve, reject };
}
function fixture() {
  const actions: unknown[] = [],
    selectedUpdates: LoraSelection[][] = [],
    modelChoices: unknown[] = [];
  const draft = { modelId: 'qwen', selected: [] as LoraSelection[], referenceCount: 0 };
  const port: LoraDraftPort = {
    contextId: 'create',
    read: () => draft,
    setSelected: value => {
      draft.selected = value;
      selectedUpdates.push(value);
    },
    appendPrompt: phrase => {
      actions.push(['prompt', phrase]);
    },
    applySampling: value => {
      actions.push(['sampling', value]);
    },
  };
  const models: BrowserModel[] = ['qwen', 'flux2-klein-4b'].map(id => ({
    id,
    label: id,
    available: false,
    variants: [{ id: 'int8', label: 'Compact', missing_bytes: 500, available: false }],
    defaults: { variant: 'int8' },
    capabilities: { image_reference: true, max_references: 4 },
  }));
  const bridge: ModelBridge = {
    capabilities: () => ({ setup: true }),
    editorBusy: () => false,
    focusCanvas() {},
    chooseModelDirectory: async () => null,
    startBackend: async () => ({}),
    downloadModel: async (model, variant) => {
      actions.push(['modelDownload', model, variant]);
      return {};
    },
    downloadLora: async payload => {
      actions.push(['loraDownload', payload]);
      return {};
    },
  };
  const api: ModelApi = {
    catalog: async () => ({ models }),
    downloads: async () => ({ running: false, phase: 'idle' }),
    setup: async () => ({ model_directory: 'C:/Models', service: { can_start: true } }),
    hardware: async () => ({}),
    inventory: async model => ({
      installed: [item(model), item(model, 'two'), item(model, 'three'), item(model, 'four')],
      curated: [item(model)],
      job: { running: false },
    }),
    search: async model => ({
      results: [{ ...item(model, 'community'), repo_id: `community/${model}`, compatibility: 'declared' }],
    }),
    files: async model => files(model),
    loraDownload: async () => ({ running: false, phase: 'idle' }),
  };
  const schedules: Array<() => void> = [];
  const controller = createModelsController({
    token: 'request-token',
    bridge,
    api,
    timers: {
      set: callback => {
        schedules.push(callback);
        return callback;
      },
      clear() {},
    },
  });
  return { controller, bridge, api, draft, port, actions, selectedUpdates, modelChoices, schedules };
}

test('delayed adapter inventory cannot replace a reopened library for a different model', async () => {
  const f = fixture(),
    old = deferred<LoraInventory>();
  f.api.inventory = model =>
    model === 'qwen' ? old.promise : Promise.resolve({ installed: [item(model)], curated: [] });
  const first = f.controller.openLoras(f.port);
  f.controller.close();
  f.draft.modelId = 'flux2-klein-4b';
  await f.controller.openLoras(f.port);
  old.resolve({ installed: [item('qwen')], curated: [] });
  await first;
  assert.equal(f.controller.getSnapshot().context?.modelId, 'flux2-klein-4b');
  assert.equal(f.controller.getSnapshot().inventory?.installed[0].model, 'flux2-klein-4b');
  assert.equal(f.selectedUpdates.length, 1);
  assert.ok(Object.isFrozen(f.controller.getSnapshot().inventory?.installed));
});

test('late search and pinned-file replies stay bound to dialog, draft and model', async () => {
  const f = fixture();
  await f.controller.openLoras(f.port);
  const oldSearch = deferred<{ results: LoraItem[] }>(),
    oldFiles = deferred<LoraFiles>();
  f.api.search = model => (model === 'qwen' ? oldSearch.promise : Promise.resolve({ results: [item(model)] }));
  f.api.files = model => (model === 'qwen' ? oldFiles.promise : Promise.resolve(files(model)));
  const search = f.controller.search(),
    inspect = f.controller.inspectFiles(item());
  f.controller.close();
  f.draft.modelId = 'flux2-klein-4b';
  await f.controller.openLoras(f.port);
  await f.controller.search();
  await f.controller.inspectFiles(item('flux2-klein-4b'));
  oldSearch.resolve({ results: [item('qwen')] });
  oldFiles.resolve(files('qwen'));
  await Promise.all([search, inspect]);
  assert.equal(f.controller.getSnapshot().files?.repo_id, 'publisher/flux2-klein-4b');
  assert.ok(f.controller.getSnapshot().searchResults.every(value => value.model === 'flux2-klein-4b'));
  assert.equal(f.controller.getSnapshot().searching, false);
  assert.equal(f.controller.getSnapshot().filesLoading, false);
});

test('unverified assignment consent is per exact file and download payload is current and pinned', async () => {
  const f = fixture();
  await f.controller.openLoras(f.port);
  await f.controller.inspectFiles(item());
  f.controller.selectFile('other.safetensors');
  assert.match(f.controller.loraDownloadBlock(), /explicitly accept/);
  await f.controller.downloadLora();
  assert.deepEqual(f.actions, []);
  f.controller.acknowledgeCompatibility(true);
  await f.controller.downloadLora();
  assert.deepEqual(f.actions, [
    [
      'loraDownload',
      { model: 'qwen', repo_id: 'publisher/qwen', filename: 'other.safetensors', revision, allow_unverified: true },
    ],
  ]);
  assert.deepEqual(f.draft.selected, [], 'Downloading never enables an adapter or changes its draft');
  f.controller.selectFile('one.safetensors');
  assert.equal(f.controller.getSnapshot().allowUnverified, false);
  f.draft.modelId = 'flux2-klein-4b';
  await f.controller.downloadLora();
  assert.equal(f.actions.length, 1, 'Changed draft model cannot use stale file metadata');
});

test('installed selection preserves missing adapters, allows at most three, and separates prompt/sampling edits', async () => {
  const f = fixture();
  f.draft.selected = [{ id: 'missing', title: 'Saved adapter', strength: 0.25 }];
  await f.controller.openLoras(f.port);
  assert.equal(f.draft.selected[0].missing, true);
  assert.equal(f.draft.selected[0].strength, 0.25);
  f.controller.useLora(item());
  f.controller.useLora(item('qwen', 'two'));
  f.controller.useLora(item('qwen', 'three'));
  f.controller.useLora(item());
  assert.equal(f.draft.selected.length, 3);
  assert.equal(f.draft.selected[1].strength, 0.7);
  assert.deepEqual(f.actions, []);
  f.controller.setStrength('one', -9);
  assert.equal(f.draft.selected[1].strength, -2);
  f.controller.removeLora('missing');
  assert.equal(f.draft.selected.length, 2);
  f.controller.addTrigger('watercolor');
  f.controller.applySampling({ steps: 6, guidance: 1 });
  assert.deepEqual(f.actions, [
    ['prompt', 'watercolor'],
    ['sampling', { steps: 6, guidance: 1 }],
  ]);
});

test('offline live search retains matching recommended examples and never downloads', async () => {
  const f = fixture();
  await f.controller.openLoras(f.port);
  f.api.search = async () => {
    throw Error('Offline');
  };
  f.controller.setQuery('style');
  await f.controller.search();
  assert.equal(f.controller.getSnapshot().searchResults.length, 1);
  assert.match(f.controller.getSnapshot().error, /Installed adapters and recommended examples remain available/);
  f.controller.setQuery('no-such-style');
  await f.controller.search();
  assert.equal(f.controller.getSnapshot().searchResults.length, 0);
  await f.controller.showRecommended();
  assert.equal(f.controller.getSnapshot().query, '');
  assert.equal(f.controller.getSnapshot().searchResults.length, 1);
  assert.deepEqual(f.actions, []);
});

test('browser can use installed adapters and select unavailable models without native download', async () => {
  const f = fixture();
  f.bridge.capabilities = () => ({ setup: false });
  await f.controller.openLoras(f.port);
  f.controller.useLora(item());
  assert.equal(f.draft.selected.length, 1);
  await f.controller.inspectFiles(item());
  assert.match(f.controller.loraDownloadBlock(), /desktop app/);
  await f.controller.downloadLora();
  await f.controller.openModels({
    selectedModelId: 'qwen',
    onUse: (id, variant) => {
      f.modelChoices.push([id, variant]);
    },
  });
  f.controller.useModel();
  assert.deepEqual(f.modelChoices, [['qwen', 'int8']]);
  assert.deepEqual(f.actions, []);
});

test('model download failures never auto-retry or change model selection', async () => {
  const f = fixture();
  let attempts = 0;
  f.bridge.downloadModel = async () => {
    attempts++;
    throw Error('Missing configured model folder');
  };
  await f.controller.openModels({
    selectedModelId: 'qwen',
    onUse: () => {
      f.modelChoices.push('used');
    },
  });
  await f.controller.downloadModel();
  assert.equal(attempts, 1);
  assert.match(f.controller.getSnapshot().error, /Missing configured/);
  await f.controller.refreshModels(true);
  assert.equal(attempts, 1);
  assert.deepEqual(f.modelChoices, []);
  assert.equal(f.controller.getSnapshot().pendingNative, false);
});

test('folder picker cancellation has no deadline and reconnect guidance survives setup refresh', async () => {
  const f = fixture();
  await f.controller.openModels();
  const picker = deferred<null>();
  f.bridge.chooseModelDirectory = () => picker.promise;
  const cancelled = f.controller.chooseModelDirectory();
  assert.equal(f.controller.getSnapshot().pendingNative, true);
  assert.equal(f.schedules.length, 0);
  picker.resolve(null);
  await cancelled;
  assert.equal(f.controller.getSnapshot().message, '');
  f.bridge.chooseModelDirectory = async () => ({
    model_directory: 'D:/Models',
    model_folder_connection: {
      status: 'restart-required',
      message: 'Close ComfyUI, then start the backend to connect this folder.',
    },
  });
  f.api.setup = async () => ({ model_directory: 'D:/Models', service: { running: false, can_start: true } });
  await f.controller.chooseModelDirectory();
  assert.match(f.controller.getSnapshot().setup?.model_folder_connection?.message || '', /Close ComfyUI/);
});

test('model and adapter read APIs encode queries and send no privileged credential', async () => {
  const calls: Array<[string, RequestInit | undefined]> = [];
  const api = createModelApi('page-token', (async (url, init) => {
    calls.push([String(url), init]);
    return new Response('{}', { headers: { 'content-type': 'application/json' } });
  }) as typeof fetch);
  await Promise.all([
    api.catalog(true),
    api.downloads(),
    api.setup(),
    api.hardware(),
    api.inventory('qwen'),
    api.search('qwen', 'water & light'),
    api.files('qwen', 'publisher/style', revision),
    api.loraDownload(),
  ]);
  assert.equal(calls.length, 8);
  assert.ok(calls.every(([, value]) => !value?.method || value.method === 'GET'));
  assert.ok(calls.some(([url]) => url.includes('water%20%26%20light')));
  assert.ok(calls.every(([, value]) => !('x-local-launcher' in (value?.headers || {}))));
});
