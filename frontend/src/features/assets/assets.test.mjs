import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createAssetsApi, safeCreditUrl } from './api.ts';
import { createAssetsController } from './controller.ts';

const doc = (id = 'doc-a', revision = 3) => ({ id, revision, name: 'Photo', width: 32, height: 24, layer_stack: [] });
const stock = id => ({ id, provider: 'openverse', title: id, creator: 'Fixture photographer', source_url: 'https://example.org/photo', license: 'CC BY', license_url: 'https://example.org/license', attribution: 'Fixture credit', thumbnail_url: '/fixture.png', width: 32, height: 24 });
const generated = id => ({ id, name: id, model: 'fixture', variant: 'fixture', width: 32, height: 24, bytes: 100, thumbnail: '/api/local-remove/generation/library/' + id + '/thumbnail' });
const providers = { providers: [{ id: 'openverse', label: 'Openverse', available: true }, { id: 'pexels', label: 'Pexels', available: false, needs_key: true }], default_provider: 'openverse' };
function deferred() { let resolve, reject; const promise = new Promise((yes, no) => { resolve = yes; reject = no; }); return { promise, resolve, reject }; }
function setup(overrides = {}, hostChanges = {}) {
  let context = { documentId: 'doc-a', revision: 3, layerId: 'subject', navigationEpoch: 2, busy: false, nativeReady: true, canReference: true, referenceTarget: 'create-model-1', canUseDraft: true };
  let listener = () => {}; const accepted = [], references = [], drafts = [], visibility = [];
  const host = { getContext: () => ({ ...context }), subscribeContext: callback => { listener = callback; return () => {}; }, runDocumentChange: work => work({ ...context }), acceptDocument: async (...args) => { accepted.push(args); return true; }, addReference: async (...args) => { references.push(args); return true; }, useAsDraft: async (...args) => { drafts.push(args); return true; }, chooseBackgroundFolder: async () => null, setDockVisible: value => visibility.push(value), ...hostChanges };
  const api = { providers: async () => structuredClone(providers), search: async () => ({ results: [stock('stock-1')], page: 0, next_page: null }), folders: async () => ({ libraries: [] }), generated: async () => ({ items: [generated('copy-1'), generated('copy-2')], count: 2, bytes: 200 }), deleteCopies: async selection => ({ items: [generated('copy-2')], count: 1, bytes: 100, deleted: selection.ids ?? ['copy-1'], freed_bytes: 100 }), openGenerated: async () => doc('new-copy', 1), importStock: async (_id, target) => target ? doc(target.documentId, target.revision + 1) : doc('stock-working', 1), applyFolder: async target => doc(target.documentId, target.revision + 1), applyFile: async target => doc(target.documentId, target.revision + 1), connect: async () => ({ connected: true }), ...overrides };
  const controller = createAssetsController(host, 'private-token', api);
  return { controller, host, api, accepted, references, drafts, visibility, context: () => context, change: changes => { context = { ...context, ...changes }; listener(); } };
}

test('API keeps source keys and opaque stock IDs on backend routes with request token', async () => {
  const calls = [];
  const api = createAssetsApi('page-token', async (url, options) => { calls.push({ url, options }); return Response.json(url.endsWith('/import') ? { session: doc() } : { connected: true }); });
  await api.connect('pexels', 'private-provider-key');
  await api.importStock('opaque-id', { documentId: 'doc-a', revision: 2, layerId: 'layer-a' });
  assert.equal(calls[0].url, '/api/local-remove/stock/connection/pexels');
  assert.equal(calls[0].options.headers['x-local-remove-token'], 'page-token');
  assert.deepEqual(JSON.parse(calls[1].options.body), { id: 'opaque-id', target: 'background', session_id: 'doc-a', revision: 2, layer_id: 'layer-a' });
  assert.equal(calls.some(call => call.url.includes('private-provider-key')), false);
});

test('API rejects conflicts, malformed and stale document responses without retries', async () => {
  for (const result of [() => Response.json({ detail: 'Conflict' }, { status: 409 }), () => new Response('invalid'), () => Response.json({ session: doc('other', 9) }), () => Response.json({ session: doc('doc-a', 1) })]) {
    let count = 0; const api = createAssetsApi('token', async () => { count++; return result(); });
    await assert.rejects(api.importStock('opaque', { documentId: 'doc-a', revision: 3, layerId: null }));
    assert.equal(count, 1);
  }
});

test('API never substitutes image import with background mutation for reference destination', async () => {
  let body; const api = createAssetsApi('token', async (_url, options) => { body = JSON.parse(options.body); return Response.json({ session: doc('new-copy', 1) }); });
  await api.importStock('opaque'); assert.deepEqual(body, { id: 'opaque', target: 'image' });
  assert.equal(safeCreditUrl('javascript:alert(1)'), undefined); assert.equal(safeCreditUrl('https://example.org/photo'), 'https://example.org/photo');
});

test('controller reads providers only on open and searches only by deliberate command', async () => {
  let searches = 0; const fixture = setup({ search: async () => { searches++; return { results: [], page: 0, next_page: null }; } });
  await fixture.controller.open('stock'); assert.equal(searches, 0); fixture.controller.setQuery('forest'); assert.equal(searches, 0);
  await fixture.controller.search(); assert.equal(searches, 1); assert.deepEqual(fixture.visibility, [true]);
  fixture.controller.dispose();
});

test('stale stock searches cannot replace a newly chosen provider', async () => {
  const pending = deferred(); const fixture = setup({ search: () => pending.promise });
  await fixture.controller.open('stock'); fixture.controller.setQuery('old'); const result = fixture.controller.search(); fixture.controller.setProvider('pexels');
  pending.resolve({ results: [stock('stale')], page: 0, next_page: null }); await result;
  assert.equal(fixture.controller.getSnapshot().provider, 'pexels'); assert.equal(fixture.controller.getSnapshot().stock.results.length, 0); fixture.controller.dispose();
});

test('background revision is captured inside the shared host mutation queue', async () => {
  const gate = deferred(); let sent; const fixture = setup({ importStock: async (_id, target) => { sent = target; return doc(target.documentId, target.revision + 1); } });
  fixture.host.runDocumentChange = async work => { await gate.promise; return work(fixture.context()); };
  await fixture.controller.open('stock'); fixture.controller.setQuery('forest'); await fixture.controller.search(); const task = fixture.controller.importStock('background');
  fixture.change({ revision: 8 }); gate.resolve(); await task;
  assert.equal(sent.revision, 8); assert.equal(fixture.accepted[0][0].revision, 9); fixture.controller.dispose();
});

test('late stock import preserves navigation and never claims active-document success', async () => {
  const pending = deferred(), fixture = setup({ importStock: () => pending.promise });
  await fixture.controller.open('stock'); fixture.controller.setQuery('forest'); await fixture.controller.search(); const result = fixture.controller.importStock('background');
  await Promise.resolve(); fixture.change({ documentId: 'doc-b', navigationEpoch: 3 }); pending.resolve(doc('doc-a', 4)); await result;
  assert.equal(fixture.accepted.length, 0); assert.match(fixture.controller.getSnapshot().status, /left unchanged/); fixture.controller.dispose();
});

test('reference handoff uses capacity guard and retains backend document attribution', async () => {
  const credited = { ...doc('new-ref', 1), source_attribution: { attribution: 'Photographer credit' } };
  const fixture = setup({ importStock: async () => credited }); await fixture.controller.open('stock'); fixture.controller.setQuery('forest'); await fixture.controller.search();
  await fixture.controller.importStock('reference'); assert.equal(fixture.references[0][0].source_attribution.attribution, 'Photographer credit');
  fixture.change({ canReference: false }); await fixture.controller.importStock('reference'); assert.equal(fixture.references.length, 1); assert.match(fixture.controller.getSnapshot().error, /another reference/); fixture.controller.dispose();
});

test('native folder cancellation retains attached libraries and does not mutate document', async () => {
  const library = { id: 'folder-a', name: 'Backgrounds', entries: [{ id: 'entry-a', name: 'Backdrop', thumbnail: '/local.png' }] };
  const fixture = setup({ folders: async () => ({ libraries: [library] }) }); await fixture.controller.open('folders'); await fixture.controller.attachFolder();
  assert.equal(fixture.controller.getSnapshot().folderSelected, 'folder-a'); assert.equal(fixture.accepted.length, 0); assert.match(fixture.controller.getSnapshot().status, /cancelled/); fixture.controller.dispose();
});

test('browser folder keeps file handles private, sorts naturally and revokes replaced thumbnails', () => {
  const create = URL.createObjectURL, revoke = URL.revokeObjectURL, revoked = []; let issued = 0;
  URL.createObjectURL = () => 'blob:fixture-' + (++issued); URL.revokeObjectURL = value => revoked.push(value);
  try {
    const fixture = setup(); fixture.controller.attachFiles([new File(['pixels'], 'photo10.png'), new File(['pixels'], 'photo2.png'), new File(['text'], 'notes.txt')]);
    const folder = fixture.controller.getSnapshot().folders[0]; assert.deepEqual(folder.entries.map(item => item.name), ['photo2.png', 'photo10.png']); assert.equal('file' in folder.entries[0], false);
    fixture.controller.attachFiles([new File(['text'], 'notes.txt')]); assert.equal(fixture.controller.getSnapshot().folders[0], folder); assert.equal(revoked.length, 0);
    fixture.controller.attachFiles([new File(['pixels'], 'other.jpg')]); assert.equal(revoked.length, 2); fixture.controller.dispose(); assert.equal(revoked.length, 3);
  } finally { URL.createObjectURL = create; URL.revokeObjectURL = revoke; }
});

test('generated-copy deletion needs explicit confirmation and never closes editor documents', async () => {
  let deleted = 0; const fixture = setup({ deleteCopies: async selection => { deleted++; assert.deepEqual(selection, { ids: ['copy-1'] }); return { items: [generated('copy-2')], count: 1, bytes: 100, deleted: ['copy-1'], freed_bytes: 100 }; } });
  await fixture.controller.open('generated'); fixture.controller.setSelectionMode(true); fixture.controller.selectGenerated('copy-1'); await fixture.controller.deleteCopies(); assert.equal(deleted, 0);
  fixture.controller.confirmDelete(); await fixture.controller.deleteCopies(); assert.equal(deleted, 1); assert.equal(fixture.accepted.length, 0); assert.equal(fixture.controller.getSnapshot().generated.count, 1); fixture.controller.dispose();
});

test('generated images open independent copies and preserve the library entry', async () => {
  const fixture = setup(); await fixture.controller.open('generated'); fixture.controller.selectGenerated('copy-1'); await fixture.controller.openGenerated('image');
  assert.equal(fixture.accepted[0][0].id, 'new-copy'); assert.equal(fixture.controller.getSnapshot().generated.count, 2); assert.match(fixture.controller.getSnapshot().status, /library copy is retained/); fixture.controller.dispose();
});

test('generated draft handoff ignores changed generation target during copy creation', async () => {
  const pending = deferred(), fixture = setup({ openGenerated: () => pending.promise }); await fixture.controller.open('generated'); fixture.controller.selectGenerated('copy-1'); const task = fixture.controller.openGenerated('draft');
  fixture.change({ referenceTarget: 'another-draft-target' }); pending.resolve(doc('copy-new', 1)); await task; assert.equal(fixture.drafts.length, 0); fixture.controller.dispose();
});

test('search filtering clears a hidden focused generated item and selection drafts remain immutable', async () => {
  const fixture = setup(); await fixture.controller.open('generated'); fixture.controller.selectGenerated('copy-1'); const snapshot = fixture.controller.getSnapshot();
  assert.equal(Object.isFrozen(snapshot.generated.items), true); fixture.controller.setGeneratedQuery('copy-2'); assert.equal(fixture.controller.getSnapshot().generatedSelected, null); assert.equal(snapshot.generatedSelected, 'copy-1'); fixture.controller.dispose();
});

test('provider key never enters subscribed state and failed writes do not retry', async () => {
  let writes = 0; const fixture = setup({ connect: async () => { writes++; throw new Error('Storage unavailable'); } }); await fixture.controller.open('stock'); fixture.controller.setProvider('pexels'); await fixture.controller.connectProvider('secret-value');
  assert.equal(writes, 1); assert.equal(JSON.stringify(fixture.controller.getSnapshot()).includes('secret-value'), false); assert.equal(fixture.controller.getSnapshot().working, false); fixture.controller.dispose();
});
