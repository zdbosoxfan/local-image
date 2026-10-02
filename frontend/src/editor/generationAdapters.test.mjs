import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createGenerationAdapters } from './generationAdapters.ts';
import { createGenerationController } from '../features/generation/controller.ts';

const image = (id = 'source', revision = 1, extra = {}) => ({ id, revision, name: id, width: 1024, height: 768, layers: [], layer_stack: [], ...extra });
const model = id => ({ id, label: id, available: true, variants: [{ id: 'int8', available: true }], capabilities: { text_to_image: true, image_reference: true, image_to_image: true, max_references: 4, loras: true }, defaults: { variant: 'int8', width: 1024, height: 1024, steps: 4, guidance: 1 }, limits: { min_dimension: 256, max_dimension: 4096, dimension_step: 16, max_steps: 100, max_guidance: 10 } });
function deferred() { let resolve, reject; const promise = new Promise((a, b) => { resolve = a; reject = b; }); return { promise, resolve, reject }; }
const turn = () => new Promise(resolve => setTimeout(resolve, 0));
function setup(t, overrides = {}) {
  let document = image(), navigationEpoch = 1, operation = false, navigating = false, workspace = 'retouch', closeInProgress = false;
  let flags = {}, features, folder = null, modal = false;
  const listeners = new Set(), nativeListeners = new Set(), opened = [], retained = [], reports = [], tabs = [], reservations = [], inference = [], backgrounds = [];
  const publish = () => { for (const listener of [...listeners]) listener(); };
  const context = () => ({ document, documentId: document?.id ?? null, revision: document?.revision ?? 0, layerId: null, navigationEpoch, busy: operation || navigating || closeInProgress });
  const port = {
    getContext: context, getSnapshot: () => ({ ...context(), workspace, closeInProgress, ...flags }),
    subscribe(listener) { listeners.add(listener); return () => listeners.delete(listener); },
    async runDocumentChange(work) { if (context().busy) throw Error('An operation already owns the document.'); operation = true; reservations.push('reserved'); publish(); try { await overrides.flush?.(); return await work({ ...context(), busy: false }); } finally { operation = false; reservations.push('released'); publish(); } },
    async openSession(value, settings = {}) {
      if (settings.expectedNavigationEpoch !== undefined && navigationEpoch !== settings.expectedNavigationEpoch) return false;
      const epoch = ++navigationEpoch; navigating = true; publish();
      try { await overrides.preview?.(value); if (epoch !== navigationEpoch) return false; document = structuredClone(value); workspace = settings.workspace ?? workspace; opened.push(value.id); publish(); if (settings.notifyFeatures !== false) await features.afterDocumentOpened(document); return true; }
      finally { navigating = false; publish(); }
    },
    async acceptDocument(value, captured, destination) { if (captured.navigationEpoch !== navigationEpoch) return false; if (destination === 'background') { document = value; publish(); return true; } return port.openSession(value, { expectedNavigationEpoch: captured.navigationEpoch, workspace: destination === 'generated' ? 'generate' : undefined }); },
    retainDocument(value) { retained.push(structuredClone(value)); publish(); },
    async applyGeneratedBackground(...args) { backgrounds.push(args); return overrides.applyBackground ? overrides.applyBackground(...args) : true; },
    async activateMode(value, candidate) { if (candidate) return port.openSession(candidate, { workspace: value, notifyFeatures: false }); workspace = value; publish(); return true; },
    setGenerationView(value) { flags = value; publish(); }, report(text) { reports.push(text); }, rememberCurrentView() {}, isModalOpen: () => modal,
  };
  const native = { capabilities: () => ({ ready: true }), subscribe(listener) { nativeListeners.add(listener); return () => nativeListeners.delete(listener); }, chooseBackgroundFolder: async () => folder };
  const dialogs = [];
  const adapters = createGenerationAdapters({ document: port, native, openModels: value => dialogs.push(['models', value]), openLoras: value => dialogs.push(['loras', value]) });
  const api = { models: async () => ({ models: [model('qwen'), model('other')] }), upscaleModels: async () => ({ enabled: false, model: { id: 'upscale', available: false }, limits: {} }), progress: async () => ({ active: false, job_id: null, stage: 'idle', stage_label: '', elapsed_seconds: 0, progress: null }), generate: async payload => { inference.push(payload); return { session: image('generated'), seed: 0 }; }, installedLoras: async () => ({ installed: [] }), ...overrides.api };
  const generation = createGenerationController(adapters.generationHost, 'fixture-token', { api, pollMilliseconds: 10000 });
  const assets = { open: async tab => { tabs.push(tab); adapters.assetsHost.setDockVisible(true); } };
  adapters.bind(generation, assets); features = adapters.featureCommands;
  generation.acceptCatalog([model('qwen'), model('other')]);
  t.after(() => { adapters.dispose(); generation.dispose(); });
  return { adapters, generation, features, port, opened, retained, reports, tabs, reservations, inference, backgrounds, dialogs,
    current: context, setFolder(value) { folder = value; }, setModal(value) { modal = value; },
    replace(value, nextWorkspace = workspace) { document = value; workspace = nextWorkspace; navigationEpoch++; publish(); },
    revise(value) { document = { ...document, revision: value }; publish(); },
    close(ids) { closeInProgress = true; if (ids.includes(document?.id)) { document = null; navigationEpoch++; } publish(); features.noteClosed(ids); closeInProgress = false; publish(); },
  };
}

test('blank Create retains explicit reference source but has no implicit command target', async t => {
  const f = setup(t); await f.generation.setMode('create');
  assert.equal(f.adapters.generationHost.getContext().document.id, 'source');
  assert.equal(f.adapters.getSnapshot().visibleDocumentId, null);
  assert.equal(f.adapters.getSnapshot().creatingBlank, true);
  assert.equal(f.adapters.assetsHost.getContext().documentId, null);
  assert.equal(f.adapters.assetsHost.getContext().revision, 0);
  for (const kind of ['save', 'project', 'close', 'credits']) assert.equal((await f.features.prepareDocumentCommand(kind)).allowed, false);
  f.generation.useCurrent('create');
  assert.deepEqual(f.generation.referencesFor('create').map(value => value.id), ['source']);
  assert.deepEqual(f.opened, []); assert.equal(f.current().document.id, 'source');
});

test('Refine Save/Project/Close/Credits target selected refinement image, never hidden editor source', async t => {
  for (const kind of ['save', 'project', 'close', 'credits']) {
    const f = setup(t); f.generation.addDraft(image('draft')); await f.generation.setMode('refine');
    f.generation.setDraft('final', { prompt: 'Controlled result' }); await f.generation.run('final');
    assert.equal(f.adapters.generationHost.getContext().document.id, 'source');
    assert.equal(f.adapters.getSnapshot().visibleDocumentId, 'generated');
    assert.equal(f.adapters.assetsHost.getContext().documentId, null);
    assert.deepEqual(await f.features.prepareDocumentCommand(kind), { allowed: true, forceExport: true });
    assert.equal(f.current().document.id, 'generated'); assert.deepEqual(f.opened, ['generated']);
    assert.equal(f.generation.getSnapshot().mode, 'edit'); assert.equal(f.adapters.getSnapshot().refining, false);
  }
});

test('shared reservation spans accepted preview and releases failures without a retry', async t => {
  const gate = deferred(), f = setup(t, { preview: () => gate.promise }); await f.generation.setMode('create');
  f.generation.setDraft('create', { prompt: 'Controlled result' }); const run = f.generation.run('create'); await turn();
  assert.equal(f.current().busy, true); assert.equal(f.generation.getSnapshot().working, true);
  assert.deepEqual(f.reservations, ['reserved']); assert.equal(f.inference.length, 1);
  await assert.rejects(f.adapters.assetsHost.runDocumentChange(async () => {}), /already owns/);
  gate.resolve(); await run; assert.equal(f.current().busy, false); assert.deepEqual(f.reservations, ['reserved', 'released']);
  await assert.rejects(f.adapters.assetsHost.runDocumentChange(async () => { throw Error('Controlled failure'); }), /Controlled failure/);
  assert.equal(f.current().busy, false); assert.equal(f.inference.length, 1);
});

test('virtual mode changes and real navigation reject late document handoffs', async t => {
  const f = setup(t); await f.generation.setMode('create');
  const generationContext = f.adapters.generationHost.getContext(), assetsContext = f.adapters.assetsHost.getContext(), rawEpoch = f.current().navigationEpoch;
  await f.generation.setMode('refine'); assert.equal(f.current().navigationEpoch, rawEpoch);
  assert.equal(await f.adapters.generationHost.acceptResult(image('late'), generationContext, 'create'), false);
  assert.equal(await f.adapters.assetsHost.acceptDocument(image('late'), assetsContext, 'image'), false);
  const next = f.adapters.generationHost.getContext(); f.replace(image('navigated'));
  assert.equal(await f.adapters.generationHost.acceptResult(image('late'), next, 'edit'), false);
  assert.deepEqual(f.opened, []);
});

test('references follow live draft/model identity and reject a late prior model handoff', async t => {
  const f = setup(t); await f.generation.setMode('create'); const captured = f.adapters.assetsHost.getContext();
  f.generation.chooseModel('create', 'other');
  assert.equal(await f.adapters.assetsHost.addReference(image('late'), captured), false);
  assert.equal(await f.adapters.assetsHost.addReference(image('create-reference'), f.adapters.assetsHost.getContext()), true);
  await f.generation.setMode('refine');
  assert.equal(await f.adapters.assetsHost.addReference(image('draft-reference'), f.adapters.assetsHost.getContext()), true);
  assert.deepEqual(f.generation.referencesFor('create').map(value => value.id), ['create-reference']);
  assert.deepEqual(f.generation.referencesFor('draft').map(value => value.id), ['draft-reference']);
  assert.equal(f.generation.referencesFor('edit').length, 0);
});

test('newer accepted revision rejects a late source-based result or asset response', async t => {
  const f = setup(t); await f.generation.setMode('edit');
  const context = f.adapters.generationHost.getContext(), assetsContext = f.adapters.assetsHost.getContext(); f.revise(2);
  assert.equal(await f.adapters.generationHost.acceptResult(image('late'), context, 'edit'), false);
  assert.equal(await f.adapters.assetsHost.acceptDocument(image('source', 3), assetsContext, 'background'), false);
  assert.equal(f.current().revision, 2); assert.deepEqual(f.opened, []);
});

test('navigation during the shared flush submits no inference and leaves no uncertain operation', async t => {
  const held = deferred(), f = setup(t, { flush: () => held.promise }); await f.generation.setMode('create');
  f.generation.setDraft('create', { prompt: 'Controlled result' }); const run = f.generation.run('create');
  f.replace(image('new-document')); held.resolve(); await run;
  assert.equal(f.inference.length, 0); assert.equal(f.generation.getSnapshot().uncertain, false);
  assert.match(f.generation.getSnapshot().error, /No image operation was submitted/); assert.equal(f.current().busy, false);
});

test('asset generated copy and refinement draft handoffs use owned document lifecycle', async t => {
  const f = setup(t);
  assert.equal(await f.adapters.assetsHost.runDocumentChange(context => f.adapters.assetsHost.acceptDocument(image('copy'), context, 'generated')), true);
  assert.equal(f.generation.getSnapshot().modeDocuments.edit.id, 'copy');
  assert.equal(await f.adapters.assetsHost.runDocumentChange(context => f.adapters.assetsHost.useAsDraft(image('draft-copy'), context)), true);
  assert.equal(f.adapters.getSnapshot().refining, true); assert.equal(f.adapters.getSnapshot().visibleDocumentId, 'draft-copy');
  assert.equal(f.current().document.id, 'copy'); assert.equal(f.current().busy, false);
});

test('native background picker preserves cancellation and strips unneeded authority fields', async t => {
  const f = setup(t); assert.equal(await f.adapters.assetsHost.chooseBackgroundFolder(), null);
  f.setFolder({ id: 'library', name: 'Pictures', path: 'C:/private', entries: [{ id: 'entry', name: 'Photo', thumbnail: '/thumb', path: 'C:/private/file.png' }] });
  assert.deepEqual(await f.adapters.assetsHost.chooseBackgroundFolder(), { id: 'library', name: 'Pictures', entries: [{ id: 'entry', name: 'Photo', thumbnail: '/thumb' }] });
  f.setFolder({ id: 'broken', name: 'Broken', entries: [{ name: 'Missing ID' }] });
  await assert.rejects(f.adapters.assetsHost.chooseBackgroundFolder(), /invalid/);
});

test('saved generation metadata restores on different Edit document but preserves same-document UI changes', async t => {
  const f = setup(t); await f.generation.setMode('edit', image('source'));
  const saved = image('project', 1, { generation: { model: 'qwen', variant: 'int8', prompt: 'Project prompt', width: 1536, height: 1024, steps: 12, guidance: 1 } });
  await f.port.openSession(saved, { workspace: 'generate' });
  assert.equal(f.generation.getSnapshot().drafts.edit.prompt, 'Project prompt');
  f.generation.setDraft('edit', { prompt: 'Unsaved UI prompt' });
  await f.port.openSession({ ...saved, revision: 2 }, { workspace: 'generate' });
  assert.equal(f.generation.getSnapshot().drafts.edit.prompt, 'Unsaved UI prompt');
});

test('closing visible Edit clears its target without reopening a stored Create result', async t => {
  const f = setup(t); await f.generation.setMode('create'); f.generation.setDraft('create', { prompt: 'Controlled result' }); await f.generation.run('create');
  await f.generation.setMode('edit', image('edit-image')); const before = [...f.opened]; f.close(['edit-image']); await turn();
  assert.equal(f.current().document, null); assert.equal(f.adapters.getSnapshot().visibleDocumentId, null); assert.equal(f.adapters.getSnapshot().generationVisible, false);
  assert.equal(f.generation.getSnapshot().mode, 'edit'); assert.equal(f.generation.getSnapshot().modeDocuments.create.id, 'generated'); assert.deepEqual(f.opened, before);
  assert.ok(f.generation.errorsFor('edit').some(value => value.includes('Open an image')));
  assert.equal(f.generation.referencesFor('edit').some(value => value.id === 'edit-image'), false);
  f.generation.setDraft('edit', { prompt: 'Must not submit after close' }); await f.generation.run('edit'); assert.equal(f.inference.length, 1);
});

test('adapter publications are stable and dispose removes all subscriptions', async t => {
  const f = setup(t); let events = 0; const first = f.adapters.getSnapshot(); f.adapters.subscribe(() => { events++; });
  f.adapters.assetsHost.setDockVisible(false); assert.equal(f.adapters.getSnapshot(), first); assert.equal(events, 0);
  await f.features.showGenerated(); assert.equal(events, 1); assert.deepEqual(f.tabs, ['generated']);
  f.adapters.dispose(); f.replace(image('after-dispose')); assert.equal(events, 1);
});

test('generated background keeps named targets per mode and has no library or inference dependency', async t => {
  let generations = 0;
  const f = setup(t, { api: { generate: async () => { generations++; return { session: image('generated', 1, { generation: { model: 'qwen' } }), library_warning: 'Controlled library write failure' }; } } });
  f.replace(image('cutout-one', 3, { cutout: { enabled: true } }), 'cutout'); await f.generation.setMode('create');
  assert.deepEqual(f.adapters.generationHost.getContext().backgroundTarget, { id: 'cutout-one', name: 'cutout-one' });
  f.generation.setDraft('create', { prompt: 'Controlled result' }); await f.generation.run('create');
  assert.match(f.generation.getSnapshot().status, /library write failure/); assert.equal(await f.generation.applyGeneratedBackground(), true);
  assert.equal(f.backgrounds.length, 1); assert.deepEqual(f.backgrounds[0].slice(0, 2), ['generated', 'cutout-one']); assert.equal(generations, 1);
  f.replace(image('cutout-two', 5, { layer_stack: [{ id: 'mask', kind: 'cutout', discarded: false }] }), 'cutout'); await f.generation.setMode('edit');
  assert.equal(f.adapters.generationHost.getContext().backgroundTarget.id, 'cutout-two');
  await f.generation.setMode('create'); assert.equal(f.adapters.generationHost.getContext().backgroundTarget.id, 'cutout-one');
  f.close(['cutout-one']); assert.equal(f.adapters.generationHost.getContext().backgroundTarget, null); assert.equal(await f.generation.applyGeneratedBackground(), false); assert.equal(f.backgrounds.length, 1);
});

test('generated-background handoff rejects stale views and coalesces repeated presses without retry', async t => {
  const held = deferred(), f = setup(t, { applyBackground: () => held.promise });
  f.replace(image('target', 2, { cutout: { enabled: true } }), 'cutout'); await f.generation.setMode('refine');
  f.generation.addDraft(image('generated-draft', 1, { upscale: { model: 'fixture' } }));
  const captured = f.adapters.generationHost.getContext(); await f.generation.setMode('create');
  assert.equal(await f.adapters.generationHost.applyGeneratedBackground(image('late', 1, { generation: {} }), captured), false); assert.equal(f.backgrounds.length, 0);
  await f.generation.setMode('refine'); const first = f.generation.applyGeneratedBackground(); const second = f.generation.applyGeneratedBackground();
  assert.equal(await second, false); assert.equal(f.backgrounds.length, 1); assert.equal(f.backgrounds[0][0], 'generated-draft'); assert.equal(f.backgrounds[0][2].documentId, 'target');
  held.reject(Error('Controlled conflict')); assert.equal(await first, false); assert.match(f.generation.getSnapshot().error, /Controlled conflict/); assert.equal(f.generation.getSnapshot().working, false); assert.equal(f.backgrounds.length, 1);
});

test('Retouch and Cutout receive the selected full-size refinement or upscale and Generate resumes Refine', async t => {
  for (const workspace of ['retouch', 'cutout']) {
    const final = image('final-4k', 2, { width: 3840, height: 2160, upscale: { model: 'seedvr2' } });
    const f = setup(t, { api: { generate: async () => ({ session: final }) } });
    f.generation.addDraft(image('draft-500', 1, { width: 512, height: 288 }));
    await f.generation.setMode('refine');
    f.generation.setDraft('final', { prompt: 'Controlled final' }); await f.generation.run('final');
    const draftId = f.generation.getSnapshot().selectedDraftId, resultId = f.generation.getSnapshot().selectedResultId;
    assert.equal(await f.features.leaveGenerate(workspace), true);
    assert.equal(f.port.getSnapshot().workspace, workspace); assert.equal(f.current().document.id, 'final-4k');
    assert.equal(f.current().document.width, 3840); assert.equal(f.current().document.height, 2160);
    assert.deepEqual(f.opened, ['final-4k']);
    f.revise(3);
    assert.equal(f.generation.selectedResult().session.revision, 3, 'Refine retains the accepted Retouch revision');
    assert.equal(await f.features.enterGenerate(), true);
    assert.equal(f.adapters.getSnapshot().refining, true); assert.equal(f.generation.getSnapshot().mode, 'refine');
    assert.equal(f.generation.getSnapshot().selectedDraftId, draftId); assert.equal(f.generation.getSnapshot().selectedResultId, resultId);
    assert.equal(f.adapters.getSnapshot().visibleDocumentId, 'final-4k');
    assert.equal(f.inference.length, 0, 'Mode switches submit no inference');
  }
});

test('leaving Refine uses the selected draft when no result exists and keeps that selection on return', async t => {
  const f = setup(t); f.generation.addDraft(image('draft-selected')); await f.generation.setMode('refine');
  assert.equal(await f.features.leaveGenerate('retouch'), true);
  assert.equal(f.current().document.id, 'draft-selected');
  assert.equal(await f.features.enterGenerate(), true);
  assert.equal(f.adapters.getSnapshot().refining, true); assert.equal(f.generation.getSnapshot().selectedDraftId, 'draft-selected');
  assert.equal(f.generation.getSnapshot().selectedResultId, null);
});

test('failed or blocked Refine handoff keeps the current document and selected refinement visible', async t => {
  const f = setup(t, { preview: async () => { throw Error('Controlled preview failure'); } });
  f.generation.addDraft(image('selected-draft')); await f.generation.setMode('refine');
  f.setModal(true); assert.equal(await f.features.leaveGenerate('retouch'), false); f.setModal(false);
  assert.equal(await f.features.leaveGenerate('retouch'), false);
  assert.equal(f.current().document.id, 'source'); assert.equal(f.adapters.getSnapshot().refining, true);
  assert.equal(f.generation.getSnapshot().selectedDraftId, 'selected-draft'); assert.deepEqual(f.opened, []);
  assert.match(f.reports.at(-1), /Controlled preview failure/); assert.equal(f.adapters.getSnapshot().busy, false);
});

test('opening another image after leaving Refine uses Edit rather than resuming an unrelated refinement', async t => {
  const f = setup(t); f.generation.addDraft(image('selected-draft')); await f.generation.setMode('refine');
  await f.features.leaveGenerate('retouch');
  await f.port.openSession(image('new-source'), { workspace: 'retouch' });
  assert.equal(await f.features.enterGenerate(), true);
  assert.equal(f.generation.getSnapshot().mode, 'edit'); assert.equal(f.generation.getSnapshot().modeDocuments.edit.id, 'new-source');
  assert.equal(f.generation.getSnapshot().selectedDraftId, 'selected-draft', 'Prior refinement remains available by its Refine tab');
});

test('only displayed Refine selections become retained image buffers without canvas navigation', async t => {
  const f = setup(t); f.generation.addDraft(image('draft-one')); f.generation.addDraft(image('draft-two'));
  assert.deepEqual(f.retained, [], 'Hidden or unselected generation images are not tabs');
  await f.generation.setMode('refine');
  assert.deepEqual(f.retained.map(item => item.id), ['draft-two']); assert.deepEqual(f.opened, []);
  f.generation.selectDraft('draft-one');
  assert.deepEqual(f.retained.map(item => item.id), ['draft-two', 'draft-one']); assert.deepEqual(f.opened, []);
  f.features.resetWorkspace();
  assert.equal(f.generation.getSnapshot().mode, 'create'); assert.equal(f.generation.getSnapshot().selectedDraftId, 'draft-one');
  assert.equal(f.generation.getSnapshot().draftImages.length, 2, 'New keeps refinement history available');
});

test('explicit Retouch opening preserves persona while making saved generation settings available for later Edit', async t => {
  const f = setup(t), generated = image('generated-retouch', 1, {generation: {model: 'qwen', prompt: 'Saved prompt', width: 3840, height: 2160}});
  await f.port.openSession(generated, {workspace: 'retouch'});
  assert.equal(f.port.getSnapshot().workspace, 'retouch'); assert.equal(f.generation.getSnapshot().mode, 'create');
  assert.equal(f.generation.getSnapshot().drafts.edit.prompt, 'Saved prompt');
  assert.equal(await f.features.enterGenerate(), true); assert.equal(f.generation.getSnapshot().mode, 'edit');
  assert.equal(f.generation.getSnapshot().modeDocuments.edit.id, generated.id);
});
