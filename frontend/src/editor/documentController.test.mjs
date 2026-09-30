import {test} from 'node:test';
import assert from 'node:assert/strict';
import {createDocumentController} from './documentController.ts';
import {createDocumentApi} from './documentApi.ts';

const copy = value => structuredClone(value);
const transform = () => ({offset_x: 0, offset_y: 0, scale: 1, rotation: 0});
const original = () => ({id: 'original', kind: 'original', name: 'Original', visible: true, locked: true, discarded: false, opacity: 1, transform: transform()});
function document(id = 'a', revision = 0, extra = {}) {
  return {id, name: id + '.png', width: 640, height: 480, bit_depth: 8, revision, layers: [], layer_stack: [original()], dirty: revision > 0,
    project_dirty: true, project_saved: false, project_saved_revision: null, saved_revision: null, can_return: false, stack_can_undo: false, stack_can_redo: false, ...extra};
}
function cutoutDocument(id = 'a', revision = 1) {
  const cutout = {enabled: true, alpha: 'alpha.png', feather: 0, background: {mode: 'transparent', color: '#fff'}, shadow: {enabled: false, opacity: .3}};
  return document(id, revision, {cutout, layer_stack: [original(), {id: 'cutout', kind: 'cutout', name: 'Cutout', visible: true, locked: false, discarded: false, opacity: 1, transform: transform(), cutout}], selected_layer_id: 'cutout', stack_can_undo: true});
}
const deferred = () => {let resolve, reject; const promise = new Promise((yes, no) => {resolve = yes; reject = no;}); return {promise, resolve, reject};};
const turn = () => new Promise(resolve => setTimeout(resolve, 0));

function canvasFixture() {
  const listeners = new Set(), views = new Map();
  let state = {documentId: null, revision: null, hasSelection: false, pointCount: 0, selectionVersion: 0, canUndoSelection: false, canRedoSelection: false, historyBusy: false,
    gesture: null, photoZoom: 1, fitMode: true, panX: 0, panY: 0, previewWidth: 640, previewHeight: 480};
  let interaction = {}, active = null;
  const calls = [], host = {accepted: () => null, restore: () => {}, present: null, undo: null};
  const emit = () => listeners.forEach(listener => listener());
  const remember = () => {if (active) views.set(active, {state: copy(state), interaction: copy(interaction)}); calls.push(['remember', active]);};
  const canvas = {
    getSnapshot: () => state, subscribe: listener => {listeners.add(listener); return () => listeners.delete(listener);},
    async presentDocument(display) {
      calls.push(['present', display?.document.id ?? null]);
      if (host.present) {const okay = await host.present(display); if (!okay) return false;}
      if (!display) {active = null; state = {...state, documentId: null, revision: null, hasSelection: false, pointCount: 0, canUndoSelection: false, canRedoSelection: false}; emit(); return false;}
      assert.equal(host.accepted()?.id, display.document.id, 'Canvas receives provisional accepted identity without reading React controls');
      assert.equal(host.accepted()?.revision, display.document.revision);
      if (display.document.id !== active) {
        remember(); const saved = views.get(display.document.id);
        state = saved ? {...saved.state} : {...state, hasSelection: false, pointCount: 0, canUndoSelection: false, canRedoSelection: false, photoZoom: 1};
        if (saved) host.restore({...saved.interaction, busy: display.interaction.busy, selectedLayerId: display.interaction.selectedLayerId});
      }
      active = display.document.id; state = {...state, documentId: active, revision: display.document.revision}; interaction = {...interaction, ...display.interaction}; emit(); return true;
    },
    setInteractionState: value => {interaction = {...interaction, ...value};}, rememberCurrentView: remember,
    pendingSelection: id => id === active ? state.hasSelection || state.pointCount > 0 : !!views.get(id)?.state.hasSelection || !!views.get(id)?.state.pointCount,
    pendingFingerprint: id => id + ':' + (id === active ? state.selectionVersion : views.get(id)?.state.selectionVersion ?? 'absent'),
    forgetDocument: id => {calls.push(['forget', id]); views.delete(id); if (active === id) void canvas.presentDocument(null);},
    clearSelection: options => {calls.push(['clear', options]); state = {...state, hasSelection: false, pointCount: 0, canUndoSelection: !!options?.recordHistory || state.canUndoSelection}; emit();},
    commitSelection: id => {calls.push(['commitSelection', id]); if (id && active !== id) return false; state = {...state, hasSelection: false, pointCount: 0, canUndoSelection: false, canRedoSelection: false}; emit(); return true;},
    fullMaskPayload: () => 'FULL_WHITE_MASK', selectionPayload: () => 'PRIVATE_MASK_' + active,
    finishPen: () => {state = {...state, pointCount: 0, hasSelection: true}; emit();}, cancelPen: () => {state = {...state, pointCount: 0}; emit();},
    async undoSelection() {calls.push(['undoSelection']); if (host.undo) return host.undo(); state = {...state, hasSelection: false, canUndoSelection: false, canRedoSelection: true}; emit(); return true;},
    async redoSelection() {calls.push(['redoSelection']); state = {...state, hasSelection: true, canUndoSelection: true, canRedoSelection: false}; emit(); return true;},
    cancelGesture: () => {calls.push(['cancelGesture']);}, resetTransientInput: () => {calls.push(['reset']);},
    fit: () => {state = {...state, photoZoom: 1, fitMode: true}; emit();}, actualSize: () => {state = {...state, photoZoom: 1, fitMode: false}; emit();},
    setPhotoZoom: zoom => {state = {...state, photoZoom: zoom, fitMode: false}; emit();}, zoomIn: () => canvas.setPhotoZoom(state.photoZoom * 1.25), zoomOut: () => canvas.setPhotoZoom(state.photoZoom / 1.25),
    setBrushSize: value => {interaction.brushSize = value;}, stepBrushSize: () => {}, focus: () => {calls.push(['focus']);},
  };
  return {canvas, host, calls, set(change) {state = {...state, ...change}; emit();}, get interaction() {return interaction;}};
}

function fixture(initial = [document()], overrides = {}) {
  const docs = new Map(initial.map(value => [value.id, copy(value)])), records = [], downloads = [], choices = [], preference = new Map(), c = canvasFixture();
  let imported = 0, dialogOpen = false;
  const controls = {request: null, image: null, closeChoice: 'discard', overwriteChoice: 'overwrite', nativeOpen: null, nativeSave: null, caps: {ready: false, version: 0, projects: false, closeRequests: false, setup: false, batch: false}};
  const native = {capabilities: () => controls.caps, subscribe: () => () => {}, connect: async () => controls.caps,
    openFiles: async () => {records.push({native: 'openFiles'}); return controls.nativeOpen;}, openFolder: async () => controls.nativeOpen, openProject: async () => controls.nativeOpen,
    drop: async values => {records.push({native: 'drop', files: values}); return controls.nativeOpen;}, saveProject: async body => {records.push({native: 'saveProject', body}); return typeof controls.nativeSave === 'function' ? controls.nativeSave(body) : controls.nativeSave;}};
  const dialogs = {isOpen: () => dialogOpen,
    async confirmOverwrite(name) {choices.push({kind: 'overwrite', name}); dialogOpen = true; try {return typeof controls.overwriteChoice === 'function' ? await controls.overwriteChoice() : controls.overwriteChoice;} finally {dialogOpen = false;}},
    async confirmClose(plan) {choices.push({kind: 'close', plan}); dialogOpen = true; try {return typeof controls.closeChoice === 'function' ? await controls.closeChoice(plan) : controls.closeChoice;} finally {dialogOpen = false;}},
    showCredits: (credits, text) => choices.push({kind: 'credits', credits, text})};
  const browser = {chooseFiles: async kind => {choices.push({kind: 'files', target: kind}); return [];}, download: (url, name) => {downloads.push({url, name});}, replaceUrl: url => records.push({url}), createObjectURL: file => 'blob:' + file.name, revokeObjectURL: url => records.push({revoked: url})};
  const request = async (url, init = {}) => {
    const pathname = new URL(url, 'http://fixture').pathname, method = init.method || 'GET';
    const body = init.body instanceof FormData ? Object.fromEntries(init.body.entries()) : init.body ? JSON.parse(init.body) : {};
    records.push({path: pathname, method, body: copy(body), headers: init.headers});
    if (controls.request) {const intercepted = await controls.request({url, pathname, method, body, docs}); if (intercepted) return intercepted;}
    if (pathname.endsWith('/sessions')) return Response.json([...docs.values()]);
    if (pathname.endsWith('/settings')) return Response.json({models: [{id: 'klein', available: true}, {id: 'heal', available: true}]});
    if (pathname.endsWith('/qwen/status')) return Response.json({ready: true, connected: true, variants: [{id: 'int8', available: true}, {id: 'bf16', available: false}]});
    if (pathname.endsWith('/status')) return Response.json({ready: true, retouch_ready: true});
    if (pathname.endsWith('/import')) {const data = document('upload-' + (++imported), 0, {name: body.file?.name || 'Upload.png'}); docs.set(data.id, data); return Response.json(data);}
    if (pathname.endsWith('/import-project')) {const data = document('project-' + (++imported), 1, {project_name: body.file.name, can_return: false}); docs.set(data.id, data); return Response.json({session: data});}
    if (pathname.endsWith('/close-sessions')) {
      if (body.sessions.some(value => docs.get(value.id)?.revision !== value.revision)) return Response.json({detail: 'Revision conflict on close'}, {status: 409});
      body.sessions.forEach(value => docs.delete(value.id)); return Response.json({closed: body.sessions.map(value => value.id)});
    }
    const match = pathname.match(/\/session\/([^/]+)(.*)$/); if (!match) return Response.json({detail: 'Unknown fixture route'}, {status: 404});
    const sid = decodeURIComponent(match[1]), tail = match[2], data = docs.get(sid);
    if (!data) return Response.json({detail: 'Missing fixture document'}, {status: 404});
    if (method === 'GET' && !tail) return Response.json(data);
    if (Number(body.revision) !== data.revision) return Response.json({detail: 'Revision conflict'}, {status: 409});
    if (tail === '/stack') {data.layer_stack ||= [original()]; return Response.json(data);}
    if (tail === '/save') {data.saved_revision = data.revision; data.saved_name = 'saved-' + sid + '.' + (body.format === 'original' ? 'png' : body.format); data.dirty = false; return Response.json({saved: true, name: data.saved_name, session: data, download: body.mode ? null : '/api/local-remove/session/' + sid + '/download?ext=' + (body.format === 'original' ? 'png' : body.format)});}
    if (tail === '/export-project') return Response.json({saved: true, name: sid + '.lremove', session: data, download: '/api/local-remove/session/' + sid + '/download-project'});
    if (tail === '/stack/layers') {const layer = {...original(), id: 'retouch-' + (data.revision + 1), kind: 'retouch', name: body.name, locked: false, patch_ids: []}; data.layer_stack.push(layer); data.selected_layer_id = layer.id;}
    else if (tail.startsWith('/stack/layer/')) {const layer = data.layer_stack.find(value => value.id === decodeURIComponent(tail.split('/').at(-1))); for (const [key, value] of Object.entries(body)) if (key !== 'revision') layer[key] = key === 'transform' ? {...layer.transform, ...value} : value; data.selected_layer_id = layer.id;}
    else if (tail === '/remove') {const layer = data.layer_stack.find(value => value.id === body.target_layer_id), patch = {id: 'patch-' + (data.revision + 1), visible: true, discarded: false}; data.layers.push(patch); layer.patch_ids.push(patch.id); data.selected_layer_id = layer.id;}
    else if (tail === '/cutout/refine' && data.layer_stack.find(layer => layer.id === body.layer_id)?.kind !== 'cutout') {const layer = {...original(), id: 'mask-' + (data.revision + 1), kind: 'cutout', name: 'Mask', locked: false, cutout: {enabled: true, feather: 0}}; data.layer_stack.push(layer); data.selected_layer_id = layer.id;}
    else if (tail === '/cutout/generated-background') {const layer = {...original(), id: 'background-' + (data.revision + 1), kind: 'image', name: 'Generated background', locked: false}; data.layer_stack.splice(1, 0, layer); data.selected_layer_id = layer.id;}
    data.revision++; data.dirty = true; data.stack_can_undo = true; data.stack_can_redo = tail === '/stack/undo';
    return Response.json(data);
  };
  const api = createDocumentApi('page-token', request);
  const controller = createDocumentController({token: 'page-token', api, canvas: c.canvas, native, dialogs, browser, storage: {getItem: key => preference.get(key) ?? null, setItem: (key, value) => preference.set(key, value)},
    loadImage: async url => {if (controls.image) await controls.image(url); return {src: url, width: 640, height: 480, naturalWidth: 640, naturalHeight: 480};}, ...overrides});
  c.host.accepted = controller.getAcceptedDocument; c.host.restore = controller.restoreInteraction;
  return {controller, api, docs, records, downloads, choices, controls, c, browser, preference};
}
const mutations = fixture => fixture.records.filter(value => value.method && value.method !== 'GET');

test('ported asset operation waits for the layer API queue and captures its accepted revision inside the reservation', async t => {
  const f = fixture(), held = deferred(), order = []; t.after(() => f.controller.dispose());
  await f.controller.openSession(f.docs.get('a')); const captured = f.controller.getContext();
  f.controls.request = async request => {if (request.pathname.endsWith('/stack/layer/original')) {order.push('layer'); await held.promise;} return null;};
  const layer = f.api.mutate(captured.document, '/stack/layer/original', {opacity: 0}, 'PATCH');
  const acceptedLayer = layer.then(value => f.controller.acceptDocument(value, captured, 'background'));
  const asset = f.controller.runDocumentChange(async context => {
    order.push('asset'); assert.equal(context.documentId, 'a'); assert.equal(context.revision, 1);
    assert.equal(context.document.layer_stack[0].opacity, 0); assert.equal(f.controller.getSnapshot().busy, true);
    return 'accepted';
  });
  await turn(); assert.deepEqual(order, ['layer']); assert.equal(f.controller.getSnapshot().busy, true);
  held.resolve(); assert.equal(await asset, 'accepted'); assert.equal(await acceptedLayer, true);
  assert.deepEqual(order, ['layer', 'asset']); assert.equal(f.controller.getSnapshot().busy, false);
});

test('opening adopts immutable metadata, migrates old stacks once and keeps pixels outside public state', async t => {
  const data = document('old'); delete data.layer_stack;
  const f = fixture([data]); t.after(() => f.controller.dispose());
  assert.equal(await f.controller.openSession(data), true);
  const state = f.controller.getSnapshot(); assert.equal(state.document.id, 'old'); assert.ok(Object.isFrozen(state.document.layer_stack));
  assert.deepEqual(mutations(f).map(value => value.path), ['/api/local-remove/session/old/stack']);
  assert.doesNotMatch(JSON.stringify(state), /PRIVATE_MASK|data:image/); assert.equal('mask' in state.canvas, false); assert.equal('undo' in state.canvas, false);
  assert.equal(state.document.revision, 0); assert.equal(state.chrome.dirty, false);
});

test('late navigation assets or failure cannot overwrite a newer active image', async t => {
  const a = document('a'), b = document('b'), f = fixture([a, b]), held = deferred(); t.after(() => f.controller.dispose());
  f.controls.image = url => url.includes('/a/') ? held.promise : Promise.resolve();
  const first = f.controller.openSession(a); await turn(); assert.equal(await f.controller.openSession(b), true);
  held.reject(new Error('Old image failed')); assert.equal(await first, false);
  assert.equal(f.controller.getSnapshot().document.id, 'b'); assert.equal(f.controller.getSnapshot().statusError, false);
});

test('per-image canvas view restores without copying mask buffers into the document store', async t => {
  const f = fixture([document('a'), document('b')]); t.after(() => f.controller.dispose());
  await f.controller.openSession(f.docs.get('a')); f.c.set({hasSelection: true, pointCount: 2, canUndoSelection: true, photoZoom: 2});
  await f.controller.openSession(f.docs.get('b')); assert.equal(f.controller.getSnapshot().selectionActive, false);
  await f.controller.openSession(f.docs.get('a')); assert.equal(f.controller.getSnapshot().selectionActive, true); assert.equal(f.controller.getSnapshot().canvas.pointCount, 2); assert.equal(f.controller.getSnapshot().chrome.zoom, 2);
});

test('explicit image handoff workspace takes precedence over cached canvas mode before feature notification', async t => {
  const seen = [], f = fixture([document('a'), document('b')]); t.after(() => f.controller.dispose());
  f.controller.setFeatures({afterDocumentOpened: () => {seen.push(f.controller.getSnapshot().workspace);}});
  await f.controller.openSession(f.docs.get('a'), {workspace: 'generate'}); f.c.set({hasSelection: true, photoZoom: 2});
  await f.controller.openSession(f.docs.get('b'));
  await f.controller.openSession(f.docs.get('a'), {workspace: 'retouch'});
  const state = f.controller.getSnapshot(); assert.equal(state.workspace, 'retouch'); assert.equal(f.c.interaction.workspace, 'retouch'); assert.equal(seen.at(-1), 'retouch');
  assert.equal(state.canvas.hasSelection, true); assert.equal(state.canvas.photoZoom, 2);
});

test('settled pan and viewport resize coordinates reach the immutable document snapshot without redundant publications', async t => {
  const f = fixture(); t.after(() => f.controller.dispose()); await f.controller.openSession(f.docs.get('a'));
  let publications = 0; const stop = f.controller.subscribe(() => publications++); t.after(stop);
  f.c.set({panX: 24, panY: -30}); assert.equal(publications, 1);
  const settled = f.controller.getSnapshot(); assert.equal(settled.canvas.panX, 24); assert.equal(settled.canvas.panY, -30); assert.ok(Object.isFrozen(settled.canvas));
  f.c.set({panX: 24, panY: -30}); assert.equal(publications, 1); assert.equal(f.controller.getSnapshot(), settled);
  f.c.set({panX: 50.5, panY: 6.5}); assert.equal(publications, 2);
  assert.equal(f.controller.getSnapshot().canvas.panX, 50.5); assert.equal(f.controller.getSnapshot().canvas.panY, 6.5);
});

test('rapid layer mutations serialize at accepted revisions and do not select a toggled unrelated layer', async t => {
  const f = fixture([cutoutDocument()]); t.after(() => f.controller.dispose()); await f.controller.openSession(f.docs.get('a'));
  const first = f.controller.commands.patchLayer('original', {visible: false});
  const second = f.controller.commands.patchLayer('original', {locked: false}); await Promise.all([first, second]);
  const edits = mutations(f).filter(value => value.method === 'PATCH'); assert.deepEqual(edits.map(value => value.body.revision), [1, 2]);
  assert.equal(f.controller.getSnapshot().document.revision, 3); assert.equal(f.controller.getSnapshot().selectedLayerId, 'cutout');
});

test('a revision conflict preserves accepted state, invalidates queued writes and never retries', async t => {
  const f = fixture([cutoutDocument()]); t.after(() => f.controller.dispose()); await f.controller.openSession(f.docs.get('a'));
  f.controls.request = ({method}) => method === 'PATCH' ? Response.json({detail: 'External revision conflict'}, {status: 409}) : null;
  await Promise.all([f.controller.commands.patchLayer('cutout', {opacity: .2}), f.controller.commands.patchLayer('cutout', {opacity: .7})]);
  assert.equal(mutations(f).filter(value => value.method === 'PATCH').length, 1); assert.equal(f.controller.getSnapshot().document.layer_stack[1].opacity, 1); assert.equal(f.controller.getSnapshot().document.revision, 1);
  assert.equal(f.controller.getSnapshot().statusError, true);
});

test('selection undo is chosen before await and a decode failure cannot fall through to backend history', async t => {
  const f = fixture([cutoutDocument()]); t.after(() => f.controller.dispose()); await f.controller.openSession(f.docs.get('a'));
  f.c.set({hasSelection: true, canUndoSelection: true}); f.c.host.undo = async () => false;
  await f.controller.commands.undo(); assert.equal(mutations(f).length, 0);
  f.c.set({hasSelection: false, canUndoSelection: false}); await f.controller.commands.undo(); assert.equal(mutations(f).at(-1).path, '/api/local-remove/session/a/stack/undo');
});

test('CPU repair creates an editable target, preserves selected mask until result, then clears selection history', async t => {
  const f = fixture(); t.after(() => f.controller.dispose()); await f.controller.openSession(f.docs.get('a')); await f.controller.refreshHealth();
  f.c.set({hasSelection: true, canUndoSelection: true, canRedoSelection: true}); await f.controller.commands.applySelection();
  const requests = mutations(f); assert.deepEqual(requests.map(value => value.path), ['/api/local-remove/session/a/stack/layers', '/api/local-remove/session/a/remove']);
  assert.equal(requests[1].body.model, 'heal'); assert.equal(requests[1].body.mask, 'PRIVATE_MASK_a'); assert.equal(requests[1].body.revision, 1);
  assert.equal(f.controller.getSnapshot().document.layers.length, 1); assert.equal(f.controller.getSnapshot().canvas.canUndoSelection, false); assert.equal(f.controller.getSnapshot().canUndo, true);
});

test('failed repair retains the pending selection and does not resubmit inference', async t => {
  const f = fixture(); t.after(() => f.controller.dispose()); await f.controller.openSession(f.docs.get('a')); await f.controller.refreshHealth();
  f.c.set({hasSelection: true, canUndoSelection: true}); f.controls.request = ({pathname}) => pathname.endsWith('/remove') ? Response.json({detail: 'Repair failed'}, {status: 500}) : null;
  await f.controller.commands.applySelection(); assert.equal(f.controller.getSnapshot().selectionActive, true);
  assert.equal(mutations(f).filter(value => value.path.endsWith('/remove')).length, 1); assert.equal(f.c.calls.filter(value => value[0] === 'commitSelection').length, 0);
});

test('Add mask reads the protected Original and uses the existing replace endpoint', async t => {
  const f = fixture(); t.after(() => f.controller.dispose()); await f.controller.openSession(f.docs.get('a'));
  await f.controller.commands.addMask(); const request = mutations(f).at(-1);
  assert.equal(request.path, '/api/local-remove/session/a/cutout/refine'); assert.equal(request.body.operation, 'replace'); assert.equal(request.body.layer_id, 'original'); assert.equal(request.body.mask, 'FULL_WHITE_MASK');
  assert.equal(f.controller.getSnapshot().document.layer_stack[0].locked, true); assert.equal(f.controller.getSnapshot().tools.maskReady, true);
});

test('overwrite, unique copy and download export retain different payloads and confirmation meaning', async t => {
  const f = fixture([document('a', 1, {can_return: true, source_name: 'original.tif', bit_depth: 16})]); t.after(() => f.controller.dispose()); await f.controller.openSession(f.docs.get('a'));
  f.controller.commands.setOutputFormat('png'); let result = await f.controller.commands.overwrite();
  assert.equal(mutations(f).at(-1).body.format, 'original'); assert.equal(mutations(f).at(-1).body.mode, 'overwrite'); assert.equal(result.confirmed, true);
  result = await f.controller.commands.saveUnique(); assert.equal(mutations(f).at(-1).body.format, 'png'); assert.equal(mutations(f).at(-1).body.mode, 'unique'); assert.equal(result.confirmed, true);
  result = await f.controller.commands.exportImage(); assert.equal(mutations(f).at(-1).body.return_to_source, false); assert.equal(result.confirmed, false); assert.equal(result.kind, 'download-started'); assert.equal(f.downloads.length, 1);
});

test('cancelled or stale overwrite confirmation cannot write another image', async t => {
  const f = fixture([document('a', 1, {can_return: true}), document('b')]), answer = deferred(); t.after(() => f.controller.dispose()); await f.controller.openSession(f.docs.get('a'));
  f.controls.overwriteChoice = () => answer.promise; const saving = f.controller.commands.overwrite(); await turn(); await f.controller.openSession(f.docs.get('b')); answer.resolve('overwrite');
  assert.equal(await saving, null); assert.equal(mutations(f).length, 0); assert.equal(f.controller.getSnapshot().document.id, 'b');
});

test('browser project download is never native save success and cannot approve close', async t => {
  const f = fixture([cutoutDocument()]); t.after(() => f.controller.dispose()); await f.controller.openSession(f.docs.get('a'));
  assert.equal(await f.controller.commands.saveProject(), false); assert.equal(f.downloads.length, 1);
  f.controls.closeChoice = 'save'; assert.equal(await f.controller.closeAll(), false);
  assert.equal(mutations(f).filter(value => value.path.endsWith('/close-sessions')).length, 0); assert.equal(f.controller.getSnapshot().openDocuments.length, 1);
});

test('native Save As waits for actual saved result and picker cancellation leaves documents intact', async t => {
  const f = fixture([cutoutDocument()]); t.after(() => f.controller.dispose()); f.controls.caps = {...f.controls.caps, ready: true, projects: true}; await f.controller.openSession(f.docs.get('a'));
  assert.equal(await f.controller.commands.saveProjectAs(), false); assert.deepEqual(f.records.find(value => value.native === 'saveProject').body, {session_id: 'a', revision: 1, saveAs: true});
  f.controls.nativeSave = {saved: true, name: 'a.lremove', session: {...f.docs.get('a'), project_saved: true, project_saved_revision: 1, project_dirty: false}};
  assert.equal(await f.controller.commands.saveProject(), true); assert.equal(f.controller.getSnapshot().document.project_saved, true);
});

test('multi-document close is atomic after all native project saves; one cancellation prevents every discard', async t => {
  const f = fixture([cutoutDocument('a'), cutoutDocument('b')]); t.after(() => f.controller.dispose()); f.controls.caps = {...f.controls.caps, ready: true, projects: true};
  await f.controller.openSession(f.docs.get('a')); await f.controller.openSession(f.docs.get('b')); f.controls.closeChoice = 'save';
  f.controls.nativeSave = body => body.session_id === 'a' ? {saved: true, session: {...f.docs.get('a'), project_saved: true, project_saved_revision: 1, project_dirty: false}} : null;
  assert.equal(await f.controller.closeAll(), false); assert.equal(mutations(f).filter(value => value.path.endsWith('/close-sessions')).length, 0); assert.equal(f.controller.getSnapshot().openDocuments.length, 2);
  f.controls.closeChoice = 'discard'; assert.equal(await f.controller.closeAll(), true);
  const closes = mutations(f).filter(value => value.path.endsWith('/close-sessions')); assert.equal(closes.length, 1); assert.deepEqual(closes[0].body.sessions.map(value => value.id).sort(), ['a', 'b']); assert.equal(f.controller.getSnapshot().document, null);
});

test('backend close conflict retains every document and pending selection', async t => {
  const f = fixture([cutoutDocument('a'), cutoutDocument('b')]); t.after(() => f.controller.dispose()); await f.controller.openSession(f.docs.get('a')); f.c.set({hasSelection: true}); await f.controller.openSession(f.docs.get('b'));
  f.docs.get('a').revision++; assert.equal(await f.controller.closeAll(), false); assert.equal(f.controller.getSnapshot().openDocuments.length, 2); assert.equal(f.controller.pendingSelection('a'), true); assert.equal(f.docs.size, 2);
});

test('browser collection imports lazily, reuses session identity and clears only working state on close', async t => {
  const f = fixture([]); t.after(() => f.controller.dispose()); const a = new File(['a'], 'A.png'), b = new File(['b'], 'B.png');
  assert.equal(await f.controller.openBrowserFiles([a, b]), true); assert.equal(mutations(f).filter(value => value.path.endsWith('/import')).length, 1);
  const first = f.controller.getSnapshot().document.id; f.c.set({hasSelection: true, pointCount: 1}); await f.controller.openCollectionEntry(1);
  assert.equal(mutations(f).filter(value => value.path.endsWith('/import')).length, 2); await f.controller.openCollectionEntry(0); assert.equal(f.controller.getSnapshot().document.id, first); assert.equal(f.controller.pendingSelection(first), true);
  assert.equal(await f.controller.closeDocuments([first]), true); assert.equal(f.controller.getSnapshot().collection.entries[0].session_id, null); assert.equal(f.controller.getSnapshot().collection.entries[1].session_id, 'upload-2');
});

test('native Explorer drop sends actual File objects through the fixed bridge, never paths', async t => {
  const f = fixture(); t.after(() => f.controller.dispose()); f.controls.caps = {...f.controls.caps, ready: true}; f.controls.nativeOpen = {session: f.docs.get('a')}; const file = new File(['pixels'], 'Dropped.png');
  assert.equal(await f.controller.drop([file]), true); const call = f.records.find(value => value.native === 'drop'); assert.equal(call.files[0], file); assert.equal(mutations(f).length, 0);
});

test('hidden Generate source cannot be saved or closed without an explicit feature preflight', async t => {
  const f = fixture([cutoutDocument()]); t.after(() => f.controller.dispose()); await f.controller.openSession(f.docs.get('a')); await f.controller.activateMode('generate', null); f.controller.setGenerationView({creatingBlank: true, visible: false});
  assert.equal(await f.controller.commands.exportImage(), null); assert.equal(await f.controller.commands.closeImage(), false); assert.equal(mutations(f).length, 0); assert.equal(f.controller.getSnapshot().document.id, 'a');
});

test('asset/generation transaction captures navigation and ignores late result activation', async t => {
  const f = fixture([document('a'), document('b'), document('result')]), held = deferred(); t.after(() => f.controller.dispose()); await f.controller.openSession(f.docs.get('a'));
  const job = f.controller.runDocumentChange(async context => {await held.promise; return f.controller.acceptDocument(f.docs.get('result'), context, 'generated');}); await turn(); await f.controller.openSession(f.docs.get('b')); held.resolve();
  assert.equal(await job, false); assert.equal(f.controller.getSnapshot().document.id, 'b'); assert.equal(f.controller.getSnapshot().busy, false);
});

test('panning-only canvas frames do not publish React document snapshots', async t => {
  const f = fixture(); t.after(() => f.controller.dispose()); await f.controller.openSession(f.docs.get('a')); let events = 0; const stop = f.controller.subscribe(() => events++);
  for (let i = 0; i < 30; i++) f.c.set({panX: i, panY: i * 2, gesture: 'pan'});
  assert.equal(events, 0); f.c.set({gesture: null}); assert.equal(events, 1); assert.equal(f.controller.getSnapshot().canvas.panX, 29); assert.equal(f.controller.getSnapshot().canvas.panY, 58);
  f.c.set({photoZoom: 2}); assert.equal(events, 2); stop();
});

test('shared operation reservation is acquired before an awaited flush', async t => {
  const f = fixture(), held = deferred(); t.after(() => f.controller.dispose()); await f.controller.openSession(f.docs.get('a'));
  f.api.flush = () => held.promise; let calls = 0;
  const first = f.controller.runDocumentChange(async context => {calls++; assert.equal(context.busy, false); assert.equal(f.controller.getSnapshot().busy, true);});
  await assert.rejects(f.controller.runDocumentChange(async () => {calls++;}), /Finish the current operation/);
  held.resolve(); await first; assert.equal(calls, 1); assert.equal(f.controller.getSnapshot().busy, false);
});

test('intentional Cutout dialog commands work while unrelated commands remain guarded', async t => {
  let modal = false; const f = fixture([cutoutDocument()], {isModalOpen: () => modal}); t.after(() => f.controller.dispose());
  await f.controller.openSession(f.docs.get('a')); await f.controller.refreshHealth(); modal = true;
  assert.equal(await f.controller.commands.patchLayer('cutout', {opacity: .3}), null);
  await f.controller.patchCutout({feather: 2}); assert.equal(mutations(f).at(-1).path, '/api/local-remove/session/a/cutout');
  await f.controller.generateBackground('A plain studio wall'); assert.equal(mutations(f).at(-1).path, '/api/local-remove/session/a/cutout/generate-background');
  assert.equal(mutations(f).at(-1).body.prompt, 'A plain studio wall'); assert.equal(await f.controller.closeAll(), false);
});

test('background removal reads locked Original without unlocking or overwriting it', async t => {
  const f = fixture(); t.after(() => f.controller.dispose()); await f.controller.openSession(f.docs.get('a')); await f.controller.refreshHealth(); f.controller.commands.setWorkspace('cutout');
  assert.equal(f.controller.getSnapshot().tools.canRemoveBackground, true); await f.controller.toolActions.removeBackground();
  assert.equal(mutations(f).at(-1).path, '/api/local-remove/session/a/cutout'); assert.equal(mutations(f).at(-1).body.layer_id, 'original'); assert.equal(f.controller.getSnapshot().document.layer_stack[0].locked, true);
});

test('accepted writes survive preview failure and refreshing the view never repeats the write', async t => {
  const f = fixture([cutoutDocument()]); t.after(() => f.controller.dispose()); await f.controller.openSession(f.docs.get('a'));
  f.controls.image = url => {if (url.includes('revision=2')) throw Error('Preview offline');};
  await f.controller.commands.patchLayer('cutout', {opacity: .25});
  assert.equal(f.controller.getSnapshot().document.revision, 2); assert.equal(f.controller.getSnapshot().document.layer_stack[1].opacity, .25); assert.match(f.controller.getSnapshot().status, /change was accepted/);
  f.controls.image = null; assert.equal(await f.controller.refreshPreview(), true); assert.equal(f.controller.getSnapshot().canvas.revision, 2); assert.equal(mutations(f).filter(value => value.method === 'PATCH').length, 1);
  assert.equal(f.controller.getSnapshot().status, 'View refreshed.'); assert.equal(f.controller.getSnapshot().statusError, false);
});

test('background handoff selects only its explicitly returned new layer', async t => {
  const f = fixture([cutoutDocument()]); t.after(() => f.controller.dispose()); await f.controller.openSession(f.docs.get('a')); const context = f.controller.getContext();
  const result = copy(f.docs.get('a')); result.revision++; result.layer_stack.splice(1, 0, {...original(), id: 'background', kind: 'image', locked: false}); result.selected_layer_id = 'background';
  assert.equal(await f.controller.acceptDocument(result, context, 'background'), true); assert.equal(f.controller.getSnapshot().selectedLayerId, 'background');
});

test('native collection thumbnails use lazy backend URLs keyed by accepted session and revision', async t => {
  const f = fixture(); t.after(() => f.controller.dispose());
  const collection = {id: 'folder one', name: 'Chosen photos', entries: [{id: 'entry one', name: 'A.png', session_id: 'a'}, {id: 'entry two', name: 'B.png', session_id: null}]};
  f.controls.request = ({pathname}) => pathname.includes('/collection/') && pathname.endsWith('/open') ? Response.json({session: f.docs.get('a'), collection, index: 0}) : null;
  await f.controller.openCollection(collection);
  let entries = f.controller.getSnapshot().collection.entries;
  assert.equal(entries[0].thumbnail, '/api/local-remove/collection/folder%20one/entry/entry%20one/thumbnail?session=a&revision=0');
  assert.equal(entries[1].thumbnail, '/api/local-remove/collection/folder%20one/entry/entry%20two/thumbnail');
  await f.controller.commands.patchLayer('original', {visible: false}); entries = f.controller.getSnapshot().collection.entries;
  assert.match(entries[0].thumbnail, /session=a&revision=1$/);
  assert.equal(f.records.filter(value => value.path?.includes('entry%20two') && value.path.endsWith('/open')).length, 0, 'Unopened native images remain lazy');
});

test('a locked or hidden retouch target cannot apply, while protected Original remains a readable source', async t => {
  const retouch = {...original(), id: 'repair', kind: 'retouch', name: 'Repair', patch_ids: []};
  const f = fixture([document('a', 1, {layer_stack: [original(), retouch]})]); t.after(() => f.controller.dispose()); await f.controller.openSession(f.docs.get('a')); await f.controller.refreshHealth(); f.c.set({hasSelection: true});
  assert.equal(f.controller.getSnapshot().tools.canApply, false); await f.controller.commands.applySelection(); assert.equal(mutations(f).length, 0);
  await f.controller.commands.patchLayer('repair', {locked: false, visible: false}); assert.equal(f.controller.getSnapshot().tools.canApply, false);
  f.controller.commands.selectLayer('original'); f.c.set({hasSelection: true}); assert.equal(f.controller.getSnapshot().tools.canApply, true);
});

test('an imported unstacked document accepts same-revision scaffold metadata before canvas presentation', async t => {
  const imported = document('imported'); delete imported.layer_stack;
  const f = fixture([document('a'), imported]); t.after(() => f.controller.dispose()); await f.controller.openSession(f.docs.get('a'));
  const context = f.controller.getContext(); assert.equal(await f.controller.acceptDocument(imported, context, 'image'), true);
  const state = f.controller.getSnapshot(); assert.equal(state.document.id, 'imported'); assert.equal(state.document.revision, 0); assert.equal(state.document.layer_stack.length, 1); assert.equal(state.selectedLayerId, 'original');
  assert.equal(mutations(f).filter(value => value.path === '/api/local-remove/session/imported/stack').length, 1);
});

test('hand panning remains available during busy/Original comparison but not a hidden generation view', async t => {
  const f = fixture(), held = deferred(); t.after(() => f.controller.dispose()); await f.controller.openSession(f.docs.get('a'));
  f.controller.commands.toggleOriginal(); assert.equal(f.controller.getSnapshot().showOriginal, true); f.controller.commands.selectTool('hand'); assert.equal(f.controller.getSnapshot().tools.handActive, true);
  const operation = f.controller.runDocumentChange(async () => held.promise); await turn(); f.controller.commands.selectTool('hand'); assert.equal(f.controller.getSnapshot().tools.handActive, false); assert.equal(f.controller.getSnapshot().busy, true);
  held.resolve(); await operation; await f.controller.activateMode('generate', null); f.controller.setGenerationView({creatingBlank: true, visible: false}); f.controller.commands.selectTool('hand'); assert.equal(f.controller.getSnapshot().tools.handActive, false);
});

test('generated background reads a fresh target revision and accepts its ordinary background layer', async t => {
  const f = fixture([cutoutDocument('target'), document('generated', 0, {generation: {model: 'fixture'}})]); t.after(() => f.controller.dispose());
  await f.controller.openSession(f.docs.get('target')); await f.controller.openSession(f.docs.get('generated')); f.controller.setGenerationView({visible: true});
  const captured = f.controller.getContext(); f.docs.get('target').revision = 5;
  assert.equal(await f.controller.applyGeneratedBackground('generated', 'target', captured), true);
  const request = mutations(f).find(value => value.path.endsWith('/cutout/generated-background'));
  assert.equal(request.body.generated_session_id, 'generated'); assert.equal(request.body.revision, 5); assert.equal(request.body.layer_id, 'cutout');
  assert.equal(f.controller.getSnapshot().document.id, 'target'); assert.equal(f.controller.getSnapshot().workspace, 'cutout'); assert.equal(f.controller.getSnapshot().selectedLayerId, 'background-6');
});

test('stale or same-document generated-background requests never submit a mutation', async t => {
  const f = fixture([cutoutDocument('target'), document('generated'), document('other')]); t.after(() => f.controller.dispose()); await f.controller.openSession(f.docs.get('generated')); const captured = f.controller.getContext();
  assert.equal(await f.controller.applyGeneratedBackground('generated', 'generated', captured), false); await f.controller.openSession(f.docs.get('other'));
  assert.equal(await f.controller.applyGeneratedBackground('generated', 'target', captured), false); assert.equal(mutations(f).length, 0);
});

test('an explicitly selected Refine result can become a background while the editor retains the target document', async t => {
  const f = fixture([cutoutDocument('target'), document('refine-result', 0, {generation: {model: 'fixture'}})]); t.after(() => f.controller.dispose());
  await f.controller.openSession(f.docs.get('target')); await f.controller.activateMode('generate', null); f.controller.setGenerationView({refining: true, creatingBlank: false, visible: true});
  const captured = f.controller.getContext(); assert.equal(captured.documentId, 'target');
  assert.equal(await f.controller.applyGeneratedBackground('refine-result', 'target', captured), true);
  assert.equal(f.controller.getSnapshot().document.id, 'target'); assert.equal(f.controller.getSnapshot().workspace, 'cutout'); assert.equal(f.controller.getSnapshot().selectedLayerId, 'background-2');
});

test('Original comparison remains available for a visible generated image but never changes a hidden editor view', async t => {
  const f = fixture([document('generated', 0, {generation: {model: 'fixture'}})]); t.after(() => f.controller.dispose()); await f.controller.openSession(f.docs.get('generated')); f.controller.setGenerationView({visible: true});
  f.controller.commands.toggleOriginal(); assert.equal(f.controller.getSnapshot().showOriginal, true); f.controller.commands.toggleOriginal(); assert.equal(f.controller.getSnapshot().showOriginal, false);
  f.controller.setGenerationView({refining: true, visible: true}); f.controller.commands.toggleOriginal(); assert.equal(f.controller.getSnapshot().showOriginal, false);
  f.controller.setGenerationView({refining: false, creatingBlank: true, visible: false}); f.controller.commands.toggleOriginal(); assert.equal(f.controller.getSnapshot().showOriginal, false);
});
