import {test} from 'node:test';
import assert from 'node:assert/strict';
import {createBatchApi, type BatchApi} from '../src/features/batch/api.ts';
import {createBatchController} from '../src/features/batch/controller.ts';
import type {BatchEditorAdapter, BatchEditorSnapshot, BatchQueue, CreateBatchRequest} from '../src/features/batch/contracts.ts';

const clone = <T>(value: T): T => structuredClone(value);
function queue(id = 'queue-one', running = false): BatchQueue {
  return {id, name: 'Selected photos', created: 1, modified: 1, phase: running ? 'preparing' : 'review', running,
    message: running ? 'Preparing one image at a time.' : 'Review each preview before exporting.', format: 'png', mode: 'prepare',
    prepare_cutouts: true, qwen_variant: 'int8', treatment_id: null, treatment_name: null, bytes: 123, download: null,
    items: [{id: 'item-a', name: 'A.png', session_id: 'session-a', revision: 4, status: running ? 'pending' : 'ready', error: null, preview: running ? null : '/preview', output_name: null, credits_name: null, export_bit_depth: null}]};
}
function editorFixture() {
  let value: BatchEditorSnapshot = {busy: false, document: {id: 'session-a', name: 'A.png', revision: 4, canSaveTreatment: true}, collectionId: null, nativeCollection: false,
    entries: [{id: 'entry-a', name: 'A.png', sessionId: 'session-a'}], pendingSelections: [], nativeExportAvailable: false};
  const listeners = new Set<() => void>(), resolutions: string[] = [];
  const adapter: BatchEditorAdapter = {
    getSnapshot: () => clone(value), subscribe(listener) {listeners.add(listener); return () => {listeners.delete(listener);};},
    prepareForBatch: async () => {}, resolveSession: async id => {resolutions.push(id); return {session_id: 'session-a', revision: 4};},
    returnToPendingSelection: async () => {},
  };
  return {adapter, resolutions, update(change: Partial<BatchEditorSnapshot>) {value = {...value, ...change}; listeners.forEach(listener => listener());}};
}
function apiFixture(overrides: Partial<BatchApi> = {}): BatchApi {
  return {
    treatments: async () => ({items: [], bytes: 0}), queues: async () => ({items: [], bytes: 0}), queue: async id => queue(id),
    create: async () => queue(), exportZip: async id => ({...queue(id), running: true, phase: 'exporting', mode: 'zip'}),
    cancel: async id => ({...queue(id, true), modified: 2}), resume: async id => queue(id, true),
    saveTreatment: async body => ({id: 'treatment-one', name: body.name, created: 1, format: body.format}),
    deleteTreatment: async () => ({deleted: true}), clearQueue: async () => ({deleted: true}), qwenStatus: async () => ({connected: true, variants: [{id: 'int8', available: true}]}),
    previewUrl: (job, item, full = false, original = false) => `/api/local-remove/batch/jobs/${job}/items/${item}/preview${full ? '?full=true&original=' + original : ''}`,
    downloadUrl: id => '/api/local-remove/batch/jobs/' + id + '/download', ...overrides,
  };
}
function deferred<T>() {let resolve!: (value: T) => void; const promise = new Promise<T>(done => {resolve = done;}); return {promise, resolve};}

test('API attaches the page token and never retries a failed durable mutation', async () => {
  const seen: RequestInit[] = [];
  const api = createBatchApi('browser-token', (async (_url, init) => {seen.push(init!); return Response.json({detail: 'Revision changed'}, {status: 409});}) as typeof fetch);
  await assert.rejects(api.create({sessions: [{session_id: 'a', revision: 4}], format: 'png', treatment_id: null, prepare_cutouts: true, qwen_variant: 'int8'}), /Revision changed/);
  assert.equal(seen.length, 1); assert.equal((seen[0].headers as Record<string, string>)['x-local-remove-token'], 'browser-token');
  assert.equal(JSON.parse(seen[0].body as string).sessions[0].revision, 4);
});

test('pending pixels require explicit consent; reviewed settings and accepted snapshots stay immutable', async t => {
  const fixture = editorFixture(); fixture.update({pendingSelections: [{sessionId: 'session-a', name: 'A.png', fingerprint: 'selection-1'}]});
  let sent: CreateBatchRequest | undefined;
  const controller = createBatchController({token: 'x', editor: fixture.adapter, api: apiFixture({create: async body => {sent = clone(body); return queue();}})}); t.after(() => controller.dispose());
  await controller.open(); assert.equal(controller.getSnapshot().canPrepare, false);
  controller.setDraft({format: 'png'}); controller.acknowledgeAppliedOnly(true);
  const accepted = controller.getSnapshot(); assert.equal(accepted.canPrepare, true); assert.ok(Object.isFrozen(accepted.draft));
  await controller.prepare(); assert.equal(sent?.format, 'png'); assert.equal(controller.getSnapshot().appliedOnly, true);
  controller.setDraft({format: 'tif'}); assert.equal(controller.getSnapshot().active?.format, 'png');
  assert.equal(controller.getSnapshot().draft.format, 'png'); assert.equal(accepted.active, null);
  controller.newQueue(); assert.equal(controller.getSnapshot().canPrepare, false); controller.setDraft({format: 'tif'});
  controller.acknowledgeAppliedOnly(true); fixture.update({pendingSelections: [{sessionId: 'session-a', name: 'A.png', fingerprint: 'selection-2'}]});
  assert.equal(controller.getSnapshot().appliedOnly, false); assert.equal(controller.getSnapshot().canPrepare, false);
});

test('browser entries resolve sequentially but native collections keep their collection-entry identity', async t => {
  for (const nativeCollection of [false, true]) {
    const fixture = editorFixture(); fixture.update({nativeCollection, collectionId: nativeCollection ? 'folder-one' : 'browser-one', entries: [{id: 'entry-a', name: 'A', sessionId: 'session-a'}, {id: 'entry-b', name: 'B', sessionId: null}]});
    const order: string[] = [], bodies: CreateBatchRequest[] = [];
    fixture.adapter.resolveSession = async id => {order.push(id); return {session_id: id === 'entry-a' ? 'session-a' : 'session-b', revision: id === 'entry-a' ? 4 : 0};};
    const controller = createBatchController({token: 'x', editor: fixture.adapter, api: apiFixture({create: async body => {bodies.push(body); return queue();}})}); t.after(() => controller.dispose());
    await controller.open(); await controller.prepare();
    if (nativeCollection) {assert.deepEqual(order, []); assert.equal(bodies[0].collection_id, 'folder-one'); assert.deepEqual(bodies[0].entry_ids, ['entry-a', 'entry-b']); assert.equal(bodies[0].sessions, undefined);}
    else {assert.deepEqual(order, ['entry-a', 'entry-b']); assert.deepEqual(bodies[0].sessions, [{session_id: 'session-a', revision: 4}, {session_id: 'session-b', revision: 0}]); assert.equal(bodies[0].entry_ids, undefined);}
  }
});

test('late reads cannot replace a newly selected queue after closing and reopening', async t => {
  const fixture = editorFixture(), held = deferred<BatchQueue>();
  const api = apiFixture({queues: async () => ({items: [queue('first'), queue('second')], bytes: 1}), queue: async id => id === 'first' ? held.promise : queue(id)});
  const controller = createBatchController({token: 'x', editor: fixture.adapter, api}); t.after(() => controller.dispose());
  await controller.open(); controller.setHistory('first'); const previous = controller.loadQueue();
  controller.close(); await controller.open(); controller.setHistory('second'); await controller.loadQueue();
  held.resolve(queue('first')); await previous; assert.equal(controller.getSnapshot().active?.id, 'second'); assert.equal(controller.getSnapshot().working, false);
});

test('native picker cancellation refreshes reviewed queue without exporting or fabricating completion', async t => {
  const fixture = editorFixture(); fixture.update({nativeExportAvailable: true});
  const native: {job_id: string; item_ids: string[]}[] = [];
  fixture.adapter.exportBatchFolder = async input => {native.push(input); return null;};
  let reads = 0, zipExports = 0;
  const controller = createBatchController({token: 'x', editor: fixture.adapter, api: apiFixture({queues: async () => ({items: [queue()], bytes: 1}), queue: async id => {reads++; return queue(id);}, exportZip: async id => {zipExports++; return queue(id);}})}); t.after(() => controller.dispose());
  await controller.open(); await controller.loadQueue(); await controller.exportReviewed();
  assert.deepEqual(native, [{job_id: 'queue-one', item_ids: ['item-a']}]); assert.equal(reads, 2); assert.equal(zipExports, 0);
  assert.equal(controller.getSnapshot().active?.phase, 'review'); assert.equal(controller.getSnapshot().active?.download, null); assert.match(controller.getSnapshot().status, /cancelled/);
});

test('only checked ready items export; failed and unselected items never enter the request', async t => {
  const fixture = editorFixture(), original = queue();
  original.items = [...original.items, {...original.items[0], id: 'failed', status: 'failed', error: 'No mask'}, {...original.items[0], id: 'other', session_id: 'session-b'}];
  const requests: string[][] = [];
  const controller = createBatchController({token: 'x', editor: fixture.adapter, api: apiFixture({queues: async () => ({items: [original], bytes: 1}), queue: async () => original, exportZip: async (id, ids) => {requests.push(ids); return {...original, id};}})}); t.after(() => controller.dispose());
  await controller.open(); await controller.loadQueue(); controller.setSelected('other', false); await controller.exportReviewed();
  assert.deepEqual(requests, [['item-a']]);
});

test('closing stops observation but never cancels/resubmits; paused work resumes only on explicit command', async t => {
  const fixture = editorFixture(); let reads = 0, creates = 0, cancels = 0, resumes = 0;
  let saved = queue('queue-one', true);
  const api = apiFixture({create: async () => {creates++; return saved;}, queue: async () => {reads++; return saved;}, queues: async () => ({items: creates ? [saved] : [], bytes: 1}), cancel: async () => {cancels++; return saved;}, resume: async () => {resumes++; saved = {...queue('queue-one', true), modified: 4}; return saved;}});
  const controller = createBatchController({token: 'x', editor: fixture.adapter, api, pollMs: 5}); t.after(() => controller.dispose());
  await controller.open(); await controller.prepare(); controller.close(); await new Promise(resolve => setTimeout(resolve, 20));
  assert.equal(reads, 0); assert.equal(creates, 1); assert.equal(cancels, 0); assert.equal(resumes, 0);
  saved = {...saved, running: false, phase: 'paused', modified: 3}; await controller.open(); await controller.loadQueue();
  assert.equal(controller.getSnapshot().active?.phase, 'paused'); assert.equal(resumes, 0);
  await controller.resume(); assert.equal(resumes, 1); assert.equal(controller.getSnapshot().active?.phase, 'preparing'); controller.close();
});

test('preparation errors surface once and unavailable AI cannot silently submit inference', async t => {
  const fixture = editorFixture(); let creates = 0;
  const controller = createBatchController({token: 'x', editor: fixture.adapter, api: apiFixture({create: async () => {creates++; throw new Error('Source revision conflict');}})}); t.after(() => controller.dispose());
  await controller.open(); await controller.prepare(); assert.equal(creates, 1); assert.match(controller.getSnapshot().status, /Source revision conflict/); assert.equal(controller.getSnapshot().active, null);
  controller.close(); controller.dispose();
  const unavailable = createBatchController({token: 'x', editor: fixture.adapter, api: apiFixture({qwenStatus: async () => ({connected: false, variants: []}), create: async () => {creates++; return queue();}})}); t.after(() => unavailable.dispose());
  await unavailable.open(); await unavailable.prepare(); assert.equal(creates, 1); assert.match(unavailable.getSnapshot().status, /unavailable/);
});

test('background removal defaults to transparent PNGs without a saved treatment', async t => {
  const fixture = editorFixture(), bodies: CreateBatchRequest[] = [];
  const controller = createBatchController({token:'x', editor:fixture.adapter, api:apiFixture({create:async body => {bodies.push(body); return queue();}})});
  t.after(() => controller.dispose());
  await controller.open(); await controller.prepare();
  assert.equal(bodies[0].format, 'png'); assert.equal(bodies[0].prepare_cutouts, true); assert.equal(bodies[0].treatment_id, null);
});

test('background choice is submitted and reviewed queue settings cannot be changed', async t => {
  const fixture = editorFixture(), bodies: CreateBatchRequest[] = [];
  const controller = createBatchController({token:'x', editor:fixture.adapter, api:apiFixture({create:async body => {bodies.push(body); return {...queue(), background_mode:body.background_mode};}})});
  t.after(() => controller.dispose());
  await controller.open(); controller.setDraft({backgroundMode:'white'}); await controller.prepare();
  assert.equal(bodies[0].background_mode, 'white'); assert.equal(bodies[0].format, 'png');
  controller.setDraft({backgroundMode:'transparent'}); assert.equal(controller.getSnapshot().active?.background_mode, 'white');
});

test('existing cutouts can be batched without Qwen being available', async t => {
  const fixture = editorFixture(), bodies: CreateBatchRequest[] = [];
  fixture.update({entries:[{id:'entry-a',name:'A.png',sessionId:'session-a',cutoutReady:true}]});
  const controller = createBatchController({token:'x',editor:fixture.adapter,api:apiFixture({qwenStatus:async()=>({connected:false,variants:[]}),create:async body=>{bodies.push(body);return queue();}})});
  t.after(()=>controller.dispose()); await controller.open(); await controller.prepare(); assert.equal(bodies.length,1);
});
