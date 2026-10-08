import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createGenerationApi, GenerationApiError } from './api.ts';
import { createGenerationController } from './controller.ts';
import { generationPayload, validateDraft } from './validation.ts';
import { sizeMath } from './sizeMath.ts';
import { hardwareUsageSummary } from './hardwareUsage.ts';

const document = (id = 'source', revision = 1) => ({
  id,
  revision,
  name: id,
  width: 1024,
  height: 768,
  layer_stack: [],
});
const model = (id = 'qwen', changes = {}) => ({
  id,
  label: id,
  available: true,
  variants: [{ id: 'int8', label: 'INT8', available: true }],
  capabilities: {
    text_to_image: true,
    image_reference: true,
    references: true,
    image_to_image: true,
    max_references: 10,
    transparent: true,
    negative_prompt: true,
    loras: true,
  },
  defaults: { variant: 'int8', width: 1024, height: 1024, steps: 40, guidance: 1 },
  limits: {
    min_dimension: 256,
    max_dimension: 4096,
    dimension_step: 16,
    min_steps: 1,
    max_steps: 100,
    min_guidance: 1,
    max_guidance: 10,
    max_loras: 3,
  },
  ...changes,
});
const idle = {
  active: false,
  job_id: 'older-job',
  stage: 'completed',
  stage_label: 'Image ready',
  elapsed_seconds: 3,
  progress: null,
};
function deferred() {
  let resolve, reject;
  const promise = new Promise((a, b) => {
    resolve = a;
    reject = b;
  });
  return { promise, resolve, reject };
}
const turn = () => new Promise(resolve => setTimeout(resolve, 0));
const gpu = {
  devices: [
    {
      id: 'gpu-0',
      name: 'Fixture GPU',
      source: 'NVIDIA driver',
      utilization_percent: 54,
      vram_used_bytes: 9 * 1024 ** 3,
      vram_total_bytes: 16 * 1024 ** 3,
      is_backend_device: true,
      memory_scope: 'device',
    },
  ],
  comfy_connected: true,
  sampled_at: 1,
  refresh_after_ms: 2000,
};
function setup(overrides = {}, operationIds = ['owned-job', 'upscale-job']) {
  let context = { document: document(), navigationEpoch: 1, busy: false },
    notify = () => {};
  const posts = [],
    accepted = [],
    storageData = new Map();
  const storage = {
    getItem: key => storageData.get(key) ?? null,
    setItem: (key, value) => storageData.set(key, value),
  };
  const api = {
    models: async () => ({ models: [model()], busy: false }),
    upscaleModels: async () => ({
      enabled: true,
      model: { id: 'seedvr2', label: 'SeedVR2', available: true },
      limits: { min_dimension: 2, dimension_step: 2, max_dimension: 8192 },
    }),
    progress: async () => ({ ...idle }),
    hardwareUsage: async () => gpu,
    cancel: async () => ({ ...idle }),
    generate: async payload => {
      posts.push(payload);
      return {
        session: document('result'),
        seed: 0,
        width: payload.width,
        height: payload.height,
        library_warning: '',
      };
    },
    upscale: async () => ({ session: { ...document('upscaled'), width: 4096, height: 3072 } }),
    importReference: async file => document(file.name),
    installedLoras: async () => ({ installed: [] }),
    ...overrides,
  };
  const host = {
    getContext: () => structuredClone(context),
    subscribeContext: value => {
      notify = value;
      return () => {};
    },
    runGeneration: async work => work(structuredClone(context)),
    acceptResult: async (...args) => {
      accepted.push(args);
      return true;
    },
    activateMode: async () => true,
    openAssets() {},
    openModels() {},
    openLoras() {},
    sizeMath,
  };
  const controller = createGenerationController(host, 'page-token', {
    api,
    storage,
    pollMilliseconds: 10000,
    hardwarePollMilliseconds: 10000,
    createOperationId: () => operationIds.shift() ?? 'next-owned-job',
  });
  return {
    controller,
    api,
    host,
    posts,
    accepted,
    storageData,
    change: changes => {
      context = { ...context, ...changes };
      notify();
    },
  };
}
async function ready(fixture, key = 'create') {
  await fixture.controller.refreshModels();
  fixture.controller.setDraft(key, { prompt: 'A controlled fixture', seed: '0' });
}

test('generation API posts one authenticated payload and never retries inference failures', async () => {
  const calls = [];
  const api = createGenerationApi('page-token', async (url, options) => {
    calls.push({ url, options });
    return Response.json({ detail: 'Missing dependency' }, { status: 400 });
  });
  await assert.rejects(api.generate({ model: 'qwen', seed: 0 }), /Missing dependency/);
  assert.equal(calls.length, 1);
  assert.equal(calls[0].options.headers['x-local-remove-token'], 'page-token');
  assert.equal(calls[0].options.signal, undefined);
});

test('four mode drafts are independent, edit binds the source as first reference', async () => {
  const f = setup();
  await ready(f);
  f.controller.setDraft('create', { prompt: 'Create prompt', width: 1536, height: 1024 });
  await f.controller.setMode('edit');
  f.controller.setDraft('edit', { prompt: 'Edit prompt' });
  f.controller.addReference('edit', document('extra'));
  await f.controller.setMode('create');
  assert.equal(f.controller.getSnapshot().drafts.create.prompt, 'Create prompt');
  assert.equal(f.controller.getSnapshot().drafts.edit.prompt, 'Edit prompt');
  assert.deepEqual(
    f.controller.referencesFor('edit').map(item => item.id),
    ['source', 'extra'],
  );
  f.controller.removeReference('edit', 'source');
  assert.equal(f.controller.referencesFor('edit')[0].id, 'source');
  f.controller.dispose();
});

test('linked sizes use the existing tested geometry including reference-workflow limits', async () => {
  const catalog = model('qwen', {
    limits: {
      min_dimension: 256,
      max_dimension: 4096,
      dimension_step: 16,
      max_pixels: 4000000,
      reference_dimensions: {
        width: { min: 256, max: 2048, step: 32 },
        height: { min: 256, max: 1536, step: 64 },
        max_pixels: 2000000,
      },
    },
  });
  const f = setup({ models: async () => ({ models: [catalog] }) });
  await ready(f);
  f.controller.addReference('create', document());
  f.controller.setDraft('create', { width: 1891, height: 1091, locked: true, ratio: 16 / 9 });
  const draft = f.controller.getSnapshot().drafts.create;
  const expected = sizeMath.fitDimensions(
    { ...draft, axis: 'width' },
    { ...catalog.limits, ...catalog.limits.reference_dimensions },
  );
  f.controller.commitSize('create');
  assert.equal(f.controller.getSnapshot().drafts.create.width, expected.width);
  assert.equal(f.controller.getSnapshot().drafts.create.height, expected.height);
  f.controller.dispose();
});

test('chain control links the current dimensions and supports independent width and height when released', async () => {
  const f = setup();
  await ready(f);
  f.controller.setDraft('create', { width: 1536, height: 1024, aspect: 'custom' });
  f.controller.setDimensionsLinked('create', false);
  f.controller.setDraft('create', { width: 2048 });
  f.controller.commitSize('create', 'width');
  assert.equal(f.controller.getSnapshot().drafts.create.height, 1024);
  f.controller.setDimensionsLinked('create', true);
  assert.equal(f.controller.getSnapshot().drafts.create.ratio, 2);
  f.controller.setDraft('create', { height: 1536 });
  f.controller.commitSize('create', 'height');
  assert.equal(f.controller.getSnapshot().drafts.create.width, 3072);
  assert.equal(f.controller.getSnapshot().drafts.create.height, 1536);
  assert.equal(f.controller.getSnapshot().drafts.draft.locked, true);
  f.controller.dispose();
});

test('aspect menu choices relink dimensions and retain model and reference constraints', async () => {
  const f = setup();
  await ready(f);
  f.controller.setDimensionsLinked('create', false);
  f.controller.setAspect('create', '3:2');
  let draft = f.controller.getSnapshot().drafts.create;
  assert.equal(draft.locked, true);
  assert.equal(draft.aspect, '3:2');
  assert.equal(draft.ratio, 1.5);
  assert.equal(draft.width / draft.height, 1.5);
  f.controller.setAspect('create', 'custom');
  draft = f.controller.getSnapshot().drafts.create;
  assert.equal(draft.aspect, 'custom');
  assert.equal(draft.locked, true);
  assert.equal(f.controller.getSnapshot().drafts.final.aspect, '1:1');
  f.controller.dispose();
});

test('custom upscale inputs preserve the selected image proportions and reported dimension steps', async () => {
  const f = setup();
  await ready(f);
  const image = document('draft-image');
  f.controller.addDraft(image);
  f.controller.setUpscale({ preset: 'custom' });
  assert.equal(f.controller.getSnapshot().upscale.width / f.controller.getSnapshot().upscale.height, 4 / 3);
  f.controller.setUpscale({ width: 4096 });
  f.controller.commitUpscaleSize('width');
  assert.equal(f.controller.getSnapshot().upscale.width, 4096);
  assert.equal(f.controller.getSnapshot().upscale.height, 3072);
  assert.equal(f.controller.validUpscale(image), true);
  f.controller.setUpscale({ height: 2304 });
  f.controller.commitUpscaleSize('height');
  assert.equal(f.controller.getSnapshot().upscale.width, 3072);
  assert.equal(f.controller.getSnapshot().upscale.height, 2304);
  assert.equal(f.controller.upscaleSizeControls().bounds.widthStep, 8);
  assert.equal(f.controller.upscaleSizeControls().bounds.heightStep, 6);
  f.controller.setUpscale({ preset: '2048' });
  assert.deepEqual(f.controller.upscaleTarget(image), { width: 2048, height: 1536 });
  f.controller.dispose();
});

test('missing models, reference limits and missing LoRAs prevent network inference', async () => {
  const f = setup();
  await ready(f);
  f.controller.chooseModel('create', 'missing-model');
  await f.controller.run('create');
  assert.equal(f.posts.length, 0);
  assert.match(f.controller.getSnapshot().error, /installed model/);
  f.controller.chooseModel('create', 'qwen');
  f.controller.setDraft('create', { loras: [{ id: 'missing', strength: 1, missing: true }] });
  await f.controller.run('create');
  assert.equal(f.posts.length, 0);
  assert.match(f.controller.getSnapshot().error, /unavailable/);
  f.controller.dispose();
});

test('semantic references and starting-image variation follow capabilities, preserving seed zero', async () => {
  const f = setup();
  await ready(f);
  const draft = f.controller.getSnapshot().drafts.create;
  const z = model('z-image-turbo', {
    capabilities: {
      text_to_image: true,
      image_reference: false,
      image_to_image: true,
      max_references: 1,
      denoise: true,
    },
    limits: { min_steps: 1, max_steps: 50, min_guidance: 1, max_guidance: 1 },
  });
  const payload = generationPayload({ ...draft, denoise: 0.35 }, z, [{ id: 'one', name: 'One', thumbnail: '' }]);
  assert.equal(payload.seed, 0);
  assert.equal(payload.denoise, 0.35);
  assert.equal('negative_prompt' in payload, false);
  const errors = validateDraft('create', draft, z, [{ id: 'one' }, { id: 'two' }], sizeMath);
  assert.ok(errors.some(item => item.includes('at most 1')));
  f.controller.dispose();
});

test('duplicate Run presses cannot race while the original submission is pending', async () => {
  const held = deferred();
  let posts = 0;
  const f = setup({
    generate: () => {
      posts++;
      return held.promise;
    },
  });
  await ready(f);
  const first = f.controller.run('create'),
    second = f.controller.run('create');
  held.resolve({ session: document('done') });
  await Promise.all([first, second]);
  assert.equal(posts, 1);
  f.controller.dispose();
});

test('navigation before the shared generation queue executes submits no stale operation', async () => {
  const held = deferred(),
    f = setup();
  await ready(f);
  f.host.runGeneration = async work => {
    await held.promise;
    return work(f.host.getContext());
  };
  const task = f.controller.run('create');
  await new Promise(resolve => setTimeout(resolve, 0));
  f.change({ document: document('other'), navigationEpoch: 2 });
  held.resolve();
  await task;
  assert.equal(f.posts.length, 0);
  assert.match(f.controller.getSnapshot().error, /No image operation was submitted/);
  f.controller.dispose();
});

test('switching to a stored Create document does not overwrite the independent Edit source', async () => {
  const f = setup();
  await ready(f);
  await f.controller.run('create');
  await f.controller.setMode('edit', document('edit-source'));
  f.host.activateMode = async (_mode, shown) => {
    f.change({ document: shown, navigationEpoch: 2 });
    return true;
  };
  await f.controller.setMode('create');
  assert.equal(f.controller.getSnapshot().modeDocuments.edit.id, 'edit-source');
  assert.equal(f.controller.getSnapshot().modeDocuments.create.id, 'result');
  f.controller.dispose();
});

test('late result is retained per mode but does not replace a navigated document', async () => {
  const held = deferred(),
    f = setup({ generate: () => held.promise });
  await ready(f);
  const task = f.controller.run('create');
  await new Promise(resolve => setTimeout(resolve, 0));
  f.change({ document: document('other'), navigationEpoch: 2 });
  held.resolve({ session: document('completed'), seed: 0 });
  await task;
  assert.equal(f.accepted.length, 0);
  assert.equal(f.controller.getSnapshot().modeDocuments.create.id, 'completed');
  f.controller.dispose();
});

test('pause progress updates does not abort, cancel or resubmit the backend operation', async () => {
  const held = deferred();
  let calls = 0;
  const f = setup({
    generate: async () => {
      calls++;
      return held.promise;
    },
  });
  await ready(f);
  const task = f.controller.run('create');
  await new Promise(resolve => setTimeout(resolve, 0));
  f.controller.stopWatching();
  assert.match(f.controller.getSnapshot().status, /has not been cancelled/);
  assert.equal(f.controller.getSnapshot().working, true);
  held.resolve({ session: document('done') });
  await task;
  assert.equal(calls, 1);
  assert.equal(f.accepted.length, 1);
  f.controller.dispose();
});

test('transport abort leaves uncertain outcome until explicit backend-idle check', async () => {
  let calls = 0;
  const f = setup({
    generate: async () => {
      calls++;
      throw new DOMException('Connection aborted', 'AbortError');
    },
  });
  await ready(f);
  await f.controller.run('create');
  assert.equal(f.controller.getSnapshot().uncertain, true);
  await f.controller.run('create');
  assert.equal(calls, 1);
  await f.controller.checkOperation();
  assert.equal(f.controller.getSnapshot().uncertain, false);
  assert.equal(calls, 1);
  f.controller.dispose();
});

test('progress reports only the submitted operation UUID and actual sampler values', async () => {
  const held = deferred();
  const progress = {
    active: true,
    job_id: 'owned-job',
    model: 'qwen',
    stage: 'sampling',
    stage_label: 'Generating image',
    elapsed_seconds: 12,
    progress: { value: 7, max: 40, percent: 17.5 },
  };
  const f = setup({ progress: async () => progress, generate: () => held.promise });
  await ready(f);
  const task = f.controller.run('create');
  await new Promise(resolve => setTimeout(resolve, 0));
  assert.deepEqual(f.controller.getSnapshot().progress.progress, { value: 7, max: 40, percent: 17.5 });
  held.resolve({ session: document('done') });
  await task;
  f.controller.dispose();
});

test('refinement uses draft first then original references and routes result to its draft', async () => {
  const f = setup();
  await ready(f, 'final');
  await f.controller.setMode('refine');
  f.controller.addDraft(document('draft-image'), [{ id: 'reference-image', name: 'Reference', thumbnail: '' }]);
  await f.controller.run('final');
  assert.deepEqual(f.posts[0].reference_session_ids, ['draft-image', 'reference-image']);
  assert.equal(f.controller.getSnapshot().resultImages[0].draftId, 'draft-image');
  assert.equal(f.accepted.length, 1);
  f.controller.dispose();
});

test('explicit post-refinement upscale failure retains refinement and never resubmits', async () => {
  let upscales = 0;
  const f = setup({
    upscale: async () => {
      upscales++;
      throw new GenerationApiError('Unavailable upscaler', 400);
    },
  });
  await ready(f, 'final');
  f.controller.addDraft(document('draft-image'));
  f.controller.setUpscale({ enabled: true });
  await f.controller.run('final');
  assert.equal(f.posts.length, 1);
  assert.equal(upscales, 1);
  assert.equal(f.controller.getSnapshot().resultImages.length, 1);
  assert.match(f.controller.getSnapshot().error, /Refinement saved/);
  f.controller.dispose();
});

test('a chained upscale finishing after navigation stays in history without changing the new composition', async () => {
  const pending = deferred();
  const f = setup({ upscale: () => pending.promise });
  await ready(f, 'final');
  f.controller.addDraft(document('draft-image'));
  f.controller.setUpscale({ enabled: true });
  const run = f.controller.run('final');
  await turn();
  assert.equal(f.accepted.length, 1);
  f.change({ document: document('another-composition'), navigationEpoch: 2 });
  pending.resolve({ session: document('upscaled') });
  await run;
  assert.equal(f.accepted.length, 1);
  assert.ok(f.controller.getSnapshot().resultImages.some(item => item.session.id === 'upscaled'));
  f.controller.dispose();
});

test('v1 recipes round-trip stage settings and warn for missing model/adapters', async () => {
  const f = setup();
  await ready(f);
  f.controller.setDraft('draft', { prompt: 'Recipe prompt', seed: '0' });
  f.controller.setDraft('final', {
    modelId: 'old-model',
    prompt: 'Refine',
    loras: [{ id: 'abc', title: 'Missing style', strength: 0.5 }],
  });
  f.controller.saveRecipe('Test recipe');
  const saved = JSON.parse(f.storageData.get('local-image.refinement-recipes.v1'));
  assert.equal(saved[0].schema, 1);
  assert.equal(saved[0].stages.draft.seed, '0');
  f.controller.setDraft('draft', { prompt: 'Changed' });
  await f.controller.loadRecipe('Test recipe');
  assert.equal(f.controller.getSnapshot().drafts.draft.prompt, 'Recipe prompt');
  assert.ok(f.controller.getSnapshot().recipeWarnings.some(warning => warning.includes('old-model')));
  assert.equal(f.controller.getSnapshot().drafts.final.loras[0].missing, true);
  f.controller.dispose();
});

test('saved-project generation metadata preserves zero seed and flags missing references', async () => {
  const f = setup();
  await ready(f);
  f.controller.restoreDocument('create', {
    ...document(),
    generation: {
      model: 'old-model',
      variant: 'fp8',
      prompt: 'Original',
      width: 1536,
      height: 1024,
      seed: 0,
      reference_count: 2,
      loras: [{ id: 'style', strength: 1 }],
    },
  });
  const draft = f.controller.getSnapshot().drafts.create;
  assert.equal(draft.seed, '0');
  assert.equal(draft.missingReferenceCount, 2);
  assert.equal(draft.references.length, 0);
  assert.equal(draft.loras[0].missing, true);
  assert.equal(draft.modelId, 'old-model');
  f.controller.dispose();
});

test('LoRA ports read live model and refs rather than a captured draft', async () => {
  const f = setup();
  await ready(f);
  const port = f.controller.getLoraPort('create');
  f.controller.addReference('create', document());
  assert.equal(port.read().referenceCount, 1);
  port.appendPrompt('Style phrase');
  port.applySampling({ guidance: 3 });
  port.setSelected([{ id: 'lora', title: 'Style', strength: -1, usage: 'reference-edit' }]);
  assert.match(f.controller.getSnapshot().drafts.create.prompt, /Style phrase/);
  assert.equal(port.read().selected[0].usage, 'reference-edit');
  assert.equal(f.controller.getSnapshot().drafts.create.guidance, 3);
  f.controller.dispose();
});

test('a newer model-browser catalog supersedes an older background metadata read', async () => {
  const held = deferred(),
    f = setup({ models: () => held.promise });
  const pending = f.controller.refreshModels();
  f.controller.acceptCatalog([model('qwen', { available: false, reason: 'Newer catalog: dependency removed' })]);
  held.resolve({ models: [model()] });
  await pending;
  assert.equal(f.controller.getSnapshot().models[0].available, false);
  assert.match(f.controller.getSnapshot().models[0].reason, /Newer catalog/);
  assert.equal(f.controller.getSnapshot().loading, false);
  f.controller.dispose();
});

test('reference import never submits inference and rejects over-limit file batches', async () => {
  let imports = 0;
  const f = setup({
    importReference: async file => {
      imports++;
      return document(file.name);
    },
  });
  await ready(f);
  await f.controller.importReferences('create', [new File(['pixels'], 'one.png')]);
  assert.equal(imports, 1);
  assert.equal(f.posts.length, 0);
  assert.equal(f.controller.referencesFor('create')[0].name, 'one.png');
  await f.controller.importReferences(
    'create',
    Array.from({ length: 10 }, (_, i) => new File(['pixels'], `${i}.png`)),
  );
  assert.equal(imports, 1);
  f.controller.dispose();
});

test('Stop API submits only the owned job ID; hardware reads carry the token and abort signal', async () => {
  const calls = [];
  const api = createGenerationApi('gpu-token', async (url, options) => {
    calls.push({ url, options });
    return Response.json(url.endsWith('/cancel') ? idle : gpu);
  });
  await api.cancel('owned-job');
  const abort = new AbortController();
  await api.hardwareUsage(abort.signal);
  assert.equal(calls[0].url, '/api/local-remove/generation/cancel');
  assert.deepEqual(JSON.parse(calls[0].options.body), { job_id: 'owned-job' });
  assert.equal(calls[0].options.headers['x-local-remove-token'], 'gpu-token');
  assert.equal(calls[1].url, '/api/local-remove/hardware/usage');
  assert.equal(calls[1].options.method, 'GET');
  assert.equal(calls[1].options.signal, abort.signal);
});
const running = (id = 'owned-job', model = 'qwen') => ({
  ...idle,
  active: true,
  job_id: id,
  model,
  stage: 'sampling',
  stage_label: 'Generating image',
  can_cancel: true,
  progress: { value: 1, max: 4 },
});
test('Stop cancels one matching live job and acknowledged cancellation keeps the existing image', async () => {
  const held = deferred(),
    reply = deferred(),
    ids = [];
  const f = setup({
    progress: async () => running(),
    generate: () => held.promise,
    cancel: async id => {
      ids.push(id);
      return reply.promise;
    },
  });
  await ready(f);
  const task = f.controller.run('create');
  await turn();
  assert.equal(f.controller.canStop(), true);
  const stop = f.controller.stopGeneration();
  await f.controller.stopGeneration();
  assert.deepEqual(ids, ['owned-job']);
  assert.equal(f.controller.getSnapshot().stopping, true);
  reply.resolve({ ...running(), active: false, stage: 'cancelled', stage_label: 'Cancelled', can_cancel: false });
  await stop;
  held.reject(new GenerationApiError('Image operation cancelled.', 409));
  await task;
  assert.equal(f.accepted.length, 0);
  assert.equal(f.controller.getSnapshot().working, false);
  assert.equal(f.controller.getSnapshot().uncertain, false);
  assert.equal(f.controller.getSnapshot().error, null);
  assert.match(f.controller.getSnapshot().status, /Generation stopped/);
  f.controller.dispose();
});
test('Stop never targets an older job or guesses while the submitted UUID is unidentified', async () => {
  const held = deferred();
  let cancels = 0;
  const f = setup({
    progress: async () => running('older-job'),
    generate: () => held.promise,
    cancel: async () => {
      cancels++;
      return idle;
    },
  });
  await ready(f);
  const task = f.controller.run('create');
  await turn();
  assert.equal(f.controller.canStop(), false);
  await f.controller.stopGeneration();
  assert.equal(cancels, 0);
  held.resolve({ session: document('done') });
  await task;
  f.controller.dispose();
});
test('a foreign same-model job cannot enable Stop while our submission is pending', async () => {
  const held = deferred();
  let submittedId,
    cancels = 0;
  const f = setup({
    progress: async () => running('another-window-job'),
    generate: payload => {
      submittedId = payload.operation_id;
      return held.promise;
    },
    cancel: async () => {
      cancels++;
      return idle;
    },
  });
  await ready(f);
  const task = f.controller.run('create');
  await turn();
  assert.equal(submittedId, 'owned-job');
  assert.equal(f.controller.getSnapshot().progress, null);
  assert.equal(f.controller.canStop(), false);
  await f.controller.stopGeneration();
  assert.equal(cancels, 0);
  held.reject(new GenerationApiError('Another image operation started.', 409));
  await task;
  assert.equal(cancels, 0);
  f.controller.dispose();
});
test('generation and upscale API payloads preserve their distinct operation UUIDs', async () => {
  const calls = [];
  const api = createGenerationApi('token', async (url, options) => {
    calls.push({ url, body: JSON.parse(options.body) });
    return Response.json({ session: document('done') });
  });
  await api.generate({ model: 'qwen', operation_id: 'generate-id' });
  await api.upscale(document('draft'), { width: 4096, height: 3072 }, 'upscale-id');
  assert.equal(calls[0].body.operation_id, 'generate-id');
  assert.equal(calls[1].body.operation_id, 'upscale-id');
  assert.equal(calls[1].body.session_id, 'draft');
});
test('unsupported or failed Stop remains visible and keeps observing the original request', async () => {
  const held = deferred();
  let cancels = 0;
  const f = setup({
    progress: async () => running(),
    generate: () => held.promise,
    cancel: async () => {
      cancels++;
      throw new GenerationApiError('This backend cannot safely cancel the current job.', 503);
    },
  });
  await ready(f);
  const task = f.controller.run('create');
  await turn();
  await f.controller.stopGeneration();
  assert.equal(cancels, 1);
  assert.equal(f.controller.getSnapshot().working, true);
  assert.equal(f.controller.getSnapshot().stopping, false);
  assert.match(f.controller.getSnapshot().cancelError, /cannot safely cancel/);
  held.resolve({ session: document('completed') });
  await task;
  assert.equal(f.accepted.length, 1);
  assert.equal(cancels, 1);
  f.controller.dispose();
});
test('late Stop preserves a completed refinement and skips its following upscale', async () => {
  const held = deferred();
  let upscales = 0;
  const f = setup({
    progress: async () => running(),
    generate: () => held.promise,
    upscale: async () => {
      upscales++;
      return { session: document('upscaled') };
    },
    cancel: async () => ({ ...running(), stage: 'saving', can_cancel: false, cancellation_requested: false }),
  });
  await ready(f, 'final');
  f.controller.addDraft(document('draft'));
  f.controller.setUpscale({ enabled: true });
  const task = f.controller.run('final');
  await turn();
  await f.controller.stopGeneration();
  held.resolve({ session: document('refinement') });
  await task;
  assert.equal(f.controller.selectedResult().session.id, 'refinement');
  assert.equal(upscales, 0);
  assert.match(f.controller.getSnapshot().status, /upscale was skipped/);
  f.controller.dispose();
});
test('stopping the chained upscale retains the already completed refinement', async () => {
  const held = deferred();
  let reads = 0;
  const f = setup(
    {
      progress: async () => (++reads === 1 ? running('refine-job') : running('upscale-job', 'seedvr2')),
      generate: async () => ({ session: document('refinement') }),
      upscale: () => held.promise,
      cancel: async () => ({
        ...running('upscale-job', 'seedvr2'),
        active: false,
        stage: 'cancelled',
        can_cancel: false,
      }),
    },
    ['refine-job', 'upscale-job'],
  );
  await ready(f, 'final');
  f.controller.addDraft(document('draft'));
  f.controller.setUpscale({ enabled: true });
  const task = f.controller.run('final');
  await turn();
  assert.equal(f.controller.getSnapshot().runningKey, 'upscale');
  await f.controller.stopGeneration();
  held.reject(new GenerationApiError('Image operation cancelled.', 409));
  await task;
  assert.equal(f.controller.selectedResult().session.id, 'refinement');
  assert.equal(f.controller.getSnapshot().resultImages.length, 1);
  assert.equal(f.controller.getSnapshot().uncertain, false);
  assert.match(f.controller.getSnapshot().status, /Upscale stopped/);
  f.controller.dispose();
});
test('late refinement progress cannot replace or stop the following upscale UUID', async () => {
  const oldRead = deferred(),
    upscaled = deferred(),
    cancelledIds = [];
  let reads = 0,
    refineId,
    upscaleId;
  const f = setup(
    {
      progress: () => (++reads === 1 ? oldRead.promise : Promise.resolve(running('upscale-job', 'seedvr2'))),
      generate: async payload => {
        refineId = payload.operation_id;
        return { session: document('refinement') };
      },
      upscale: (_document, _size, id) => {
        upscaleId = id;
        return upscaled.promise;
      },
      cancel: async id => {
        cancelledIds.push(id);
        return { ...running('upscale-job', 'seedvr2'), cancelling: true, can_cancel: false };
      },
    },
    ['refine-job', 'upscale-job'],
  );
  await ready(f, 'final');
  f.controller.addDraft(document('draft'));
  f.controller.setUpscale({ enabled: true });
  const task = f.controller.run('final');
  await turn();
  assert.equal(refineId, 'refine-job');
  assert.equal(upscaleId, 'upscale-job');
  assert.equal(f.controller.getSnapshot().progress.job_id, 'upscale-job');
  oldRead.resolve(running('refine-job'));
  await turn();
  assert.equal(f.controller.getSnapshot().progress.job_id, 'upscale-job');
  await f.controller.stopGeneration();
  assert.deepEqual(cancelledIds, ['upscale-job']);
  upscaled.reject(new GenerationApiError('Image operation cancelled.', 409));
  await task;
  assert.equal(f.controller.selectedResult().session.id, 'refinement');
  f.controller.dispose();
});
test('standalone upscale submits and stops its own operation UUID', async () => {
  const held = deferred(),
    cancelledIds = [];
  let submittedId;
  const f = setup(
    {
      progress: async () => running('standalone-upscale', 'seedvr2'),
      upscale: (_document, _size, id) => {
        submittedId = id;
        return held.promise;
      },
      cancel: async id => {
        cancelledIds.push(id);
        return { ...running('standalone-upscale', 'seedvr2'), cancelling: true, can_cancel: false };
      },
    },
    ['standalone-upscale'],
  );
  await ready(f);
  f.controller.addDraft(document('draft'));
  const task = f.controller.run('upscale');
  await turn();
  assert.equal(submittedId, 'standalone-upscale');
  assert.equal(f.controller.canStop(), true);
  await f.controller.stopGeneration();
  assert.deepEqual(cancelledIds, ['standalone-upscale']);
  held.reject(new GenerationApiError('Image operation cancelled.', 409));
  await task;
  assert.equal(f.controller.selectedDraft().session.id, 'draft');
  f.controller.dispose();
});
test('hardware polling starts only when visible and ignores aborted hidden or disposed reads', async () => {
  const first = deferred(),
    second = deferred();
  const signals = [];
  const f = setup({
    hardwareUsage: signal => {
      signals.push(signal);
      return signals.length === 1 ? first.promise : second.promise;
    },
  });
  await ready(f);
  assert.equal(signals.length, 0);
  f.controller.setHardwareVisible(true);
  f.controller.setHardwareVisible(true);
  assert.equal(signals.length, 1);
  f.controller.setHardwareVisible(false);
  assert.equal(signals[0].aborted, true);
  first.resolve(gpu);
  await turn();
  assert.equal(f.controller.getSnapshot().hardware, null);
  f.controller.setHardwareVisible(true);
  assert.equal(signals.length, 2);
  f.controller.dispose();
  assert.equal(signals[1].aborted, true);
  second.resolve(gpu);
  await turn();
  assert.equal(f.controller.getSnapshot().hardware, null);
});
test('GPU monitor shows honest measured values and marks unsupported metrics unavailable', () => {
  assert.equal(hardwareUsageSummary(gpu, null).label, 'GPU 54% · VRAM 9.0 / 16.0 GB');
  const fallback = {
    ...gpu,
    devices: [{ ...gpu.devices[0], source: 'ComfyUI', utilization_percent: null, memory_scope: 'backend' }],
  };
  assert.equal(hardwareUsageSummary(fallback, null).label, 'GPU n/a · VRAM 9.0 / 16.0 GB');
  assert.match(hardwareUsageSummary(fallback, null).description, /AI backend/);
  assert.equal(hardwareUsageSummary({ ...gpu, devices: [] }, null).label, 'GPU unavailable');
  assert.equal(hardwareUsageSummary(gpu, 'Read failed').label, 'GPU unavailable');
});
test('eject requires a connected backend and never runs during generation or another eject', async () => {
  const held = deferred(),
    unload = deferred();
  let ejects = 0;
  const f = setup({ generate: () => held.promise });
  f.host.canEjectModels = () => true;
  f.host.ejectModels = async () => {
    ejects++;
    return unload.promise;
  };
  await ready(f);
  assert.equal(f.controller.canEjectModels(), false);
  f.controller.setHardwareVisible(true);
  await turn();
  assert.equal(f.controller.canEjectModels(), true);
  const task = f.controller.run('create');
  await turn();
  await f.controller.ejectModels();
  assert.equal(ejects, 0);
  held.resolve({ session: document('done') });
  await task;
  const eject = f.controller.ejectModels();
  await f.controller.ejectModels();
  assert.equal(ejects, 1);
  assert.equal(f.controller.getSnapshot().ejecting, true);
  unload.resolve({ job: { status: 'complete' } });
  await eject;
  assert.match(f.controller.getSnapshot().status, /unload requested/);
  f.controller.dispose();
});
