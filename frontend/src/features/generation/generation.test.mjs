import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createGenerationApi, GenerationApiError } from './api.ts';
import { createGenerationController } from './controller.ts';
import { generationPayload, validateDraft } from './validation.ts';
import { sizeMath } from './sizeMath.ts';

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
function setup(overrides = {}) {
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
  const controller = createGenerationController(host, 'page-token', { api, storage, pollMilliseconds: 10000 });
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

test('duplicate Run presses cannot race while the progress preflight is pending', async () => {
  const held = deferred();
  let preflight = true;
  const f = setup({ progress: () => (preflight ? held.promise : Promise.resolve(idle)) });
  await ready(f);
  const first = f.controller.run('create'),
    second = f.controller.run('create');
  preflight = false;
  held.resolve(idle);
  await Promise.all([first, second]);
  assert.equal(f.posts.length, 1);
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

test('progress reports only a new matching backend operation and actual sampler values', async () => {
  const held = deferred();
  let polls = 0;
  const progress = {
    active: true,
    job_id: 'new-job',
    model: 'qwen',
    stage: 'sampling',
    stage_label: 'Generating image',
    elapsed_seconds: 12,
    progress: { value: 7, max: 40, percent: 17.5 },
  };
  const f = setup({ progress: async () => (++polls === 1 ? idle : progress), generate: () => held.promise });
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
  assert.equal(f.accepted.length, 0);
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
