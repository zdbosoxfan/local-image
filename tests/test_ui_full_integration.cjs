// Full React interface acceptance through the actual source/packaged backend.
// Uses real user controls and file choosers; never fulfills/mocks API responses.
// Does not start services, install, infer with models, or download models.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const crypto = require('node:crypto');
const {execFileSync} = require('node:child_process');
const {chromium} = require('playwright');
const root = path.resolve(__dirname, '..');
const base = process.env.LOCAL_REMOVE_TEST_URL, profile = path.resolve(process.env.MIGRATION_REAL_PROFILE || '');
assert.ok(base && process.env.MIGRATION_REAL_PROFILE, 'Supply the authorized isolated backend URL/profile');
const origin = new URL(base); assert.equal(origin.hostname, '127.0.0.1'); assert.equal(origin.protocol, 'http:');
assert.ok(profile.toLowerCase().startsWith(path.join(root, 'qa-artifacts').toLowerCase() + path.sep));
const out = path.resolve(process.env.MIGRATION_REAL_OUTPUT || path.join(root, 'qa-artifacts', 'migration', 'full-react'));
assert.ok(out.toLowerCase().startsWith(path.join(root, 'qa-artifacts').toLowerCase() + path.sep)); fs.mkdirSync(out, {recursive: true});
const backendKind = process.env.MIGRATION_BACKEND_KIND || 'source'; assert.ok(['source', 'packaged'].includes(backendKind));
const python = process.env.MIGRATION_PYTHON || path.join(root, '.venv', 'Scripts', 'python.exe');
const hash = value => crypto.createHash('sha256').update(value).digest('hex');
const fileEvidence = file => ({path: path.relative(root, file).replaceAll('\\', '/'), bytes: fs.statSync(file).size, sha256: hash(fs.readFileSync(file))});
const files = directory => fs.existsSync(directory) ? fs.readdirSync(directory, {withFileTypes: true}).flatMap(entry => entry.isDirectory() ? files(path.join(directory, entry.name)) : [path.join(directory, entry.name)]) : [];
const sources = [...files(path.join(root, 'frontend/src')), ...files(path.join(root, 'backend/frontend_dist')),
  ...['backend/local_remove_frontend.py', 'backend/local_remove.py', 'backend/layer_stack.py', 'backend/local_remove_project.py', 'backend/frontend_tokens.py', 'backend/frontend/react.html', 'frontend/package-lock.json', 'tests/test_ui_full_integration.cjs'].map(name => path.join(root, name)).filter(file => fs.existsSync(file))];
const sourceInventory = sources.map(fileEvidence), fingerprint = () => hash(sources.map(file => hash(fs.readFileSync(file))).join(':')), beforeFingerprint = fingerprint();
const baseRevision = execFileSync('git', ['rev-parse', 'HEAD'], {cwd: root, encoding: 'utf8'}).trim();
const workingTreeStatus = execFileSync('git', ['status', '--porcelain=v1'], {cwd: root, encoding: 'utf8'}).trim();

async function main() {
  const runtimeResponse = await fetch(new URL('/api/local-remove/runtime', base)); assert.equal(runtimeResponse.status, 200);
  const runtime = await runtimeResponse.json(); assert.equal(path.resolve(runtime.data_root).toLowerCase(), profile.toLowerCase());
  const browser = await chromium.launch({headless: true, channel: process.env.BROWSER_CHANNEL || 'msedge'});
  const context = await browser.newContext({viewport: {width: 1366, height: 768}, acceptDownloads: true}), page = await context.newPage();
  const startedAt = new Date().toISOString(), checks = [], screenshots = [], outputs = [], requests = [], responses = [], errors = [], blocked = [], assetReads = [], loadedAssetHashes = [], snapshots = [], metrics = [];
  let passed = false, failure = null, pageHash = null, csp = null, cspViolations = [], reviewedSessionId = null;
  const state = () => page.evaluate(() => window.LocalImageEditor.getSnapshot());
  const ready = () => page.waitForFunction(() => document.body.dataset.reactReady === 'true' && !!window.LocalImageEditor, null, {timeout: 60000});
  const idle = () => page.waitForFunction(() => !!window.LocalImageEditor?.getSnapshot().document && !window.LocalImageEditor.getSnapshot().busy);
  const selected = async () => {const value = await state(); return value.document.layer_stack.find(layer => layer.id === value.selectedLayerId);};
  const row = id => page.locator('.li-layer-row[data-layer-id="' + id + '"]');
  const toolbar = () => page.getByRole('toolbar', {name: 'Editor commands', exact: true});
  const record = value => {checks.push(value); console.log('PASS: ' + value);};
  const snapshot = async name => {const document = await page.evaluate(async () => {const id = window.LocalImageEditor.getSnapshot().document.id; const response = await fetch('/api/local-remove/session/' + encodeURIComponent(id)); if (!response.ok) throw Error('Snapshot read failed ' + response.status); return response.json();}); snapshots.push({name, document}); return document;};
  const shot = async name => {await page.mouse.move(2, 2); const file = path.join(out, name); await page.screenshot({path: file, animations: 'disabled'}); screenshots.push(fileEvidence(file));};
  const menu = async (group, label) => {await page.getByRole('menuitem', {name: group, exact: true}).click(); await page.getByRole('menuitem', {name: new RegExp('^' + label + '(?:\\s+Ctrl\\+.*)?$')}).click();};
  const choose = async (action, files) => {const [chooser] = await Promise.all([page.waitForEvent('filechooser'), action()]); await chooser.setFiles(files); await idle();};
  const photoBounds = () => page.locator('#photo').evaluate(async node => {
    let previous = '', stable = 0, bounds;
    for (let i = 0; i < 12; i++) {await new Promise(requestAnimationFrame); bounds = node.getBoundingClientRect().toJSON(); const key = JSON.stringify([bounds.x, bounds.y, bounds.width, bounds.height]); stable = key === previous ? stable + 1 : 0; previous = key; if (stable >= 2) return bounds;}
    throw Error('Photo geometry did not settle after the tool/layout change.');
  });
  const rectangle = async (x1, y1, x2, y2) => {
    await page.getByRole('button', {name: 'Rectangle selection (R)', exact: true}).click(); const bounds = await photoBounds();
    await page.mouse.move(bounds.x + bounds.width * x1, bounds.y + bounds.height * y1); await page.mouse.down();
    await page.mouse.move(bounds.x + bounds.width * x2, bounds.y + bounds.height * y2, {steps: 6}); await page.mouse.up();
    await page.waitForFunction(() => window.LocalImageEditor.getSnapshot().canvas.hasSelection && !window.LocalImageEditor.getSnapshot().busy);
  };
  const erase = async (...box) => {await rectangle(...box); const revision = (await state()).document.revision; await page.getByRole('button', {name: 'Erase selection', exact: true}).click(); await page.waitForFunction(revision => {const state = window.LocalImageEditor.getSnapshot(); return !state.busy && !state.canvas.hasSelection && state.document.revision > revision;}, revision);};
  page.on('pageerror', error => errors.push(error.message));
  page.on('request', request => {const url = new URL(request.url()); let payload; if (request.headers()['content-type']?.includes('application/json')) {try {payload = JSON.parse(request.postData() || '{}'); if (payload.mask) payload.mask = '[' + payload.mask.length + ' base64 characters]';} catch {}} requests.push({path: url.pathname, method: request.method(), ...(payload ? {payload} : {})});});
  page.on('response', response => {const url = new URL(response.url()); responses.push({path: url.pathname, method: response.request().method(), status: response.status()}); if (url.pathname.startsWith('/frontend-assets/')) assetReads.push(response.body().then(value => loadedAssetHashes.push({path: url.pathname, bytes: value.length, sha256: hash(value), status: response.status(), contentType: response.headers()['content-type']})));});
  await context.route('**/*', route => {
    const request = route.request(), url = new URL(request.url()); let forbidden = url.origin !== origin.origin && !['data:', 'blob:'].includes(url.protocol);
    if (request.method() !== 'GET') {
      forbidden ||= /^\/api\/local-remove\/(?:generation(?:\/|$)|generator|qwen\/download|setup|loras\/download|stock\/import)/.test(url.pathname) || /\/cutout$|\/cutout\/generate-background$/.test(url.pathname);
      if (/\/remove$/.test(url.pathname)) {try {forbidden ||= JSON.parse(request.postData() || '{}').model !== 'heal';} catch {forbidden = true;}}
    }
    if (forbidden) {blocked.push({url: url.href, method: request.method()}); return route.abort('blockedbyclient');} return route.continue();
  });
  await context.addInitScript(() => {for (const key of ['local-image.hardware-guide.v1', 'local-image.first-ai-setup.v1', 'local-image.first-task.v1']) localStorage.setItem(key, '1'); localStorage.setItem('local-remove-operation', 'heal'); window.__fullReactCsp = []; document.addEventListener('securitypolicyviolation', event => window.__fullReactCsp.push({directive: event.violatedDirective, blocked: event.blockedURI}));});
  try {
    const begin = performance.now(), response = await page.goto(new URL('/remove', base).href); pageHash = hash(await response.body()); csp = response.headers()['content-security-policy']; await ready();
    metrics.push({emptyReadyMs: performance.now() - begin, context: 'Actual ' + backendKind + ' backend with installed headless Edge; not native WebView2 interaction'});
    assert.doesNotMatch(csp, /unsafe-inline|unsafe-eval/);
    assert.equal(await page.evaluate(() => typeof window.LocalImageLegacyEditor), 'undefined');
    assert.equal(await page.evaluate(() => typeof session), 'undefined', 'No shared legacy session global is loaded');
    for (const id of ['settings-dialog', 'hardware-dialog', 'shortcuts-dialog', 'model-browser-dialog', 'lora-dialog', 'stock-dialog', 'generated-library-dialog', 'retouch-panel', 'overwrite-dialog', 'close-dialog']) assert.equal(await page.locator('#' + id).count(), 0, 'Retired legacy DOM absent: ' + id);
    record('Full React composition loads without the legacy editor globals or duplicate retired dialogs');
    const original = Buffer.from(await page.evaluate(() => {const c = document.createElement('canvas'); c.width = 640; c.height = 480; const g = c.getContext('2d'); g.fillStyle = '#529180'; g.fillRect(0, 0, 640, 480); g.fillStyle = '#d44d42'; g.fillRect(260, 150, 120, 180); g.fillStyle = '#ffe18c'; g.fillRect(40, 40, 14, 14); g.fillRect(80, 80, 14, 14); return c.toDataURL('image/png').split(',')[1];}), 'base64');
    const sourcePath = path.join(out, 'full-original.png'); fs.writeFileSync(sourcePath, original); outputs.push(fileEvidence(sourcePath));
    await choose(() => page.getByRole('button', {name: 'Open image…', exact: true}).click(), {name: 'Full React fixture.png', mimeType: 'image/png', buffer: original});
    const opened = await snapshot('real-upload'); assert.equal(opened.revision, 0); assert.equal(opened.dirty, false); assert.equal(opened.layer_stack.length, 1);
    const canvas = await page.locator('#photo').elementHandle();
    await page.waitForFunction(() => document.getElementById('empty-root').hidden);
    const openedBounds = await photoBounds(), center = {x: openedBounds.x + openedBounds.width / 2, y: openedBounds.y + openedBounds.height / 2};
    const visible = await page.evaluate(point => {const image = document.getElementById('photo-image'); return {loaded: image.complete && image.naturalWidth > 0, hidden: image.hidden || document.getElementById('stage').hidden, hit: !!document.elementFromPoint(point.x, point.y)?.closest('#stage'), emptyHidden: document.getElementById('empty-root').hidden};}, center);
    assert.deepEqual(visible, {loaded: true, hidden: false, hit: true, emptyHidden: true});
    await shot('full-opened-1366x768.png');
    const screenshotPixel = JSON.parse(execFileSync(python, ['-c', "import sys,json; from PIL import Image; im=Image.open(sys.argv[1]).convert('RGB'); print(json.dumps(im.getpixel((int(sys.argv[2]),int(sys.argv[3])))))", path.join(out, 'full-opened-1366x768.png'), String(Math.floor(center.x)), String(Math.floor(center.y))], {encoding: 'utf8'}));
    assert.ok(screenshotPixel.every((value, index) => Math.abs(value - [212, 77, 66][index]) <= 2), 'Rendered screenshot center must show the actual red subject, not an opaque UI overlay');
    metrics.push({openedImageVisualProof: {point: center, expectedRgb: [212, 77, 66], screenshotRgb: screenshotPixel}});
    record('Owned file chooser uploads an unchanged original; visible canvas, hit testing and screenshot pixels show the actual image');
    await page.getByRole('button', {name: 'New retouch layer', exact: true}).click(); await idle(); const retouchId = (await selected()).id;
    await page.getByRole('button', {name: 'Quick Heal brush (J)', exact: true}).click(); await page.getByRole('combobox', {name: 'Quick Heal method', exact: true}).selectOption('telea');
    for (const [x1, y1, x2, y2, patches] of [[.05, .065, .10, .135, 1], [.112, .15, .16, .21, 2]]) {
      await rectangle(x1, y1, x2, y2); await page.getByRole('button', {name: 'Heal', exact: true}).click();
      await page.waitForFunction(patches => {const state = window.LocalImageEditor.getSnapshot(); return !state.busy && state.document.layer_stack.find(layer => layer.id === state.selectedLayerId)?.patch_ids?.length === patches;}, patches, {timeout: 60000});
    }
    const healed = await snapshot('two-cpu-heals'); assert.equal(healed.layers.length, 2); assert.equal(healed.layer_stack.find(layer => layer.id === retouchId).patch_ids.length, 2); record('Two real CPU repairs persist in the selected retouch layer through the new controller');
    await row(retouchId).focus(); await page.keyboard.press('F2'); await page.getByRole('textbox', {name: 'Layer name', exact: true}).fill('Dust cleanup'); await page.keyboard.press('Enter');
    await page.waitForFunction(() => {const state = window.LocalImageEditor.getSnapshot(); return !state.busy && state.document.layer_stack.some(layer => layer.name === 'Dust cleanup');});
    const opacity = page.getByRole('spinbutton', {name: 'Layer opacity percent', exact: true});
    for (const value of ['0', '55']) {await opacity.fill(value); await opacity.press('Enter'); await page.waitForFunction(value => {const state = window.LocalImageEditor.getSnapshot(); return !state.busy && state.document.layer_stack.find(layer => layer.id === state.selectedLayerId)?.opacity === Number(value) / 100;}, value);}
    await page.getByRole('button', {name: 'Hide Dust cleanup', exact: true}).click(); await idle(); await page.getByRole('button', {name: 'Show Dust cleanup', exact: true}).click(); await idle();
    await page.getByRole('button', {name: 'Lock Dust cleanup', exact: true}).click(); await idle(); assert.equal(await opacity.isDisabled(), true); await page.getByRole('button', {name: 'Unlock Dust cleanup', exact: true}).click(); await idle();
    assert.equal((await snapshot('metadata-edits')).layer_stack.find(layer => layer.id === retouchId).opacity, .55); record('React layer rename, zero opacity, visibility and locks are persisted by the actual backend');
    const originalId = opened.layer_stack[0].id; await row(originalId).click(); await page.getByRole('button', {name: 'Layer commands', exact: true}).click(); await page.getByRole('menuitem', {name: 'Add editable mask', exact: true}).click(); await idle();
    const cutoutId = (await selected()).id; assert.equal((await selected()).kind, 'cutout'); assert.equal((await state()).workspace, 'cutout');
    await page.getByRole('button', {name: 'Hide Original', exact: true}).click(); await idle(); await page.getByRole('button', {name: 'Hide Dust cleanup', exact: true}).click(); await idle();
    assert.equal((await state()).selectedLayerId, cutoutId, 'Toggling source visibility preserves selected mask');
    for (const box of [[0, 0, .40625, 1], [.59375, 0, 1, 1], [.40625, 0, .59375, .3125], [.40625, .6875, .59375, 1]]) await erase(...box);
    record('Add mask and four real UI erase operations create an editable alpha cutout without an inference request');
    await page.getByRole('button', {name: 'Move layer (V)', exact: true}).click(); const bounds = await photoBounds(), firstRequest = requests.length;
    await page.mouse.move(bounds.x + bounds.width * .5, bounds.y + bounds.height * .5); await page.mouse.down(); assert.equal(await page.evaluate(() => document.querySelector('#viewport').hasPointerCapture(1)), true);
    await page.mouse.move(bounds.x + bounds.width * .6, bounds.y + bounds.height * .55, {steps: 12}); await page.mouse.up();
    await page.waitForFunction(() => {const state = window.LocalImageEditor.getSnapshot(); return !state.busy && state.document.layer_stack.find(layer => layer.id === state.selectedLayerId)?.transform.offset_x > 40;});
    assert.equal(requests.slice(firstRequest).filter(value => value.method === 'PATCH').length, 1);
    for (const [label, value, field, accepted] of [['Layer scale', '120', 'scale', 1.2], ['Layer angle', '12', 'rotation', 12]]) {const control = page.getByRole('spinbutton', {name: label, exact: true}); await control.fill(value); await control.press('Enter'); await page.waitForFunction(({field, accepted}) => {const state = window.LocalImageEditor.getSnapshot(); return !state.busy && state.document.layer_stack.find(layer => layer.id === state.selectedLayerId)?.transform[field] === accepted;}, {field, accepted});}
    const transform = (await selected()).transform; await toolbar().getByRole('button', {name: 'Undo', exact: true}).click(); await idle(); assert.equal((await selected()).transform.rotation, 0);
    await toolbar().getByRole('button', {name: 'Redo', exact: true}).click(); await idle(); assert.deepEqual((await selected()).transform, transform); record('Persistent canvas pointer capture commits one transform; actual stack Undo/Redo restore exact parameters');
    await rectangle(.57, .4, .62, .5); const revision = (await state()).document.revision;
    await toolbar().getByRole('button', {name: 'Undo', exact: true}).click(); await page.waitForFunction(() => !window.LocalImageEditor.getSnapshot().canvas.hasSelection); assert.equal((await state()).document.revision, revision);
    await toolbar().getByRole('button', {name: 'Redo', exact: true}).click(); await page.waitForFunction(() => window.LocalImageEditor.getSnapshot().canvas.hasSelection); assert.equal((await state()).document.revision, revision);
    await page.getByRole('button', {name: 'Erase selection', exact: true}).click(); await idle(); assert.ok((await state()).document.revision > revision); record('Selection history takes priority over backend history, and transformed refinement commits to the real mask');
    const beforeOrder = await snapshot('before-reorder');
    for (const [name, expected] of [['Move layer down', 1], ['Move layer up', 2]]) {await page.getByRole('button', {name: 'Layer commands', exact: true}).click(); await page.getByRole('menuitem', {name, exact: true}).click(); await idle(); assert.equal((await snapshot(name)).layer_stack.findIndex(layer => layer.id === cutoutId), expected);}
    assert.equal(await canvas.evaluate(node => node === document.querySelector('#photo')), true); record('Layer reorder persists while the mounted canvas subtree remains the same object');
    const beforeProject = await snapshot('before-project');
    const [project] = await Promise.all([page.waitForEvent('download'), menu('File', 'Save project')]);
    const projectPath = path.join(out, 'full-editable.lremove'); await project.saveAs(projectPath); assert.equal(await project.failure(), null); await idle(); outputs.push(fileEvidence(projectPath));
    assert.equal((await state()).document.project_saved, false);
    const archive = JSON.parse(execFileSync(python, ['-c', "import sys,zipfile,json,hashlib; z=zipfile.ZipFile(sys.argv[1]); m=json.loads(z.read('manifest.json')); print(json.dumps({'version':m['version'],'original_sha256':hashlib.sha256(z.read(m['original'])).hexdigest()}))", projectPath], {encoding: 'utf8'}));
    assert.equal(archive.version, 3); assert.equal(archive.original_sha256, hash(original));
    const oldId = (await state()).document.id; await choose(() => menu('File', 'Open project…'), projectPath); await page.waitForFunction(id => window.LocalImageEditor.getSnapshot().document.id !== id, oldId);
    const reopened = await snapshot('reopened-project'); assert.deepEqual(reopened.layer_stack, beforeProject.layer_stack); assert.deepEqual(reopened.layers, beforeProject.layers); assert.equal(reopened.can_return, false); reviewedSessionId = reopened.id;
    record('UI project download/reopen preserves exact original bytes and every actual layer/repair relationship without granting native-save authority');
    const [exported] = await Promise.all([page.waitForEvent('download'), toolbar().getByRole('button', {name: 'Export', exact: true}).click()]);
    const imagePath = path.join(out, 'full-alpha.png'); await exported.saveAs(imagePath); assert.equal(await exported.failure(), null); await idle(); outputs.push(fileEvidence(imagePath)); const png = fs.readFileSync(imagePath);
    assert.equal(png.subarray(0, 8).toString('hex'), '89504e470d0a1a0a'); assert.equal(png.readUInt32BE(16), 640); assert.equal(png.readUInt32BE(20), 480);
    const pixels = await page.evaluate(async bytes => {const image = new Image(); image.src = 'data:image/png;base64,' + bytes; await image.decode(); const c = document.createElement('canvas'); c.width = image.width; c.height = image.height; const g = c.getContext('2d'); g.drawImage(image, 0, 0); const data = g.getImageData(0, 0, c.width, c.height).data; let opaque = 0, transparent = 0; for (let i = 3; i < data.length; i += 4) {if (!data[i]) transparent++; if (data[i] === 255) opaque++;} return {opaque, transparent, corner: [...data.slice(0, 4)]};}, png.toString('base64'));
    assert.ok(pixels.opaque > 1000); assert.ok(pixels.transparent > 1000); assert.equal(pixels.corner[3], 0); metrics.push({actualExportPixels: pixels}); record('Actual Python export contains correctly sized opaque subject pixels and transparent background');
    await page.getByRole('button', {name: 'Move layer (V)', exact: true}).click();
    for (const [width, height] of [[800, 560], [1366, 768], [1920, 1080]]) {await page.setViewportSize({width, height}); await page.getByRole('toolbar', {name: 'Canvas view', exact: true}).getByRole('button', {name: 'Fit', exact: true}).click(); await shot(`full-reopened-${width}x${height}.png`); const layout = await page.evaluate(() => ({width: innerWidth, body: document.body.scrollWidth, canvas: document.querySelector('#viewport').getBoundingClientRect().toJSON()})); assert.ok(layout.body <= width + 1); assert.ok(layout.canvas.width > 100 && layout.canvas.height > 100);}
    await page.getByRole('button', {name: 'Close image', exact: true}).click(); await page.getByRole('alertdialog', {name: 'Close image?', exact: true}).waitFor({state: 'visible'}); await page.getByRole('button', {name: 'Cancel', exact: true}).click(); assert.equal((await state()).document.id, reopened.id); record('Cancel in the React close dialog preserves the editable project after browser downloads');
    await choose(() => menu('File', 'Open images…'), [{name: 'Collection A.png', mimeType: 'image/png', buffer: original}, {name: 'Collection B.png', mimeType: 'image/png', buffer: original}]);
    await page.waitForFunction(() => window.LocalImageEditor.getSnapshot().collection?.entries.length === 2); const first = (await state()).document.id;
    await rectangle(.1, .1, .2, .2); await page.getByRole('combobox', {name: 'Image zoom', exact: true}).selectOption('2');
    await page.getByRole('button', {name: 'Next image', exact: true}).click(); await page.waitForFunction(id => !window.LocalImageEditor.getSnapshot().busy && window.LocalImageEditor.getSnapshot().document.id !== id, first);
    await page.getByRole('button', {name: 'Previous image', exact: true}).click(); await page.waitForFunction(id => !window.LocalImageEditor.getSnapshot().busy && window.LocalImageEditor.getSnapshot().document.id === id, first);
    assert.equal((await state()).canvas.hasSelection, true); assert.equal((await state()).canvas.photoZoom, 2); record('Real lazy folder navigation restores the per-image pending selection and camera through React filmstrip controls');
    await Promise.all(assetReads); assert.ok(loadedAssetHashes.some(item => item.path.endsWith('.js') && item.status === 200)); assert.ok(loadedAssetHashes.some(item => item.path.endsWith('.css') && item.status === 200));
    cspViolations = await page.evaluate(() => window.__fullReactCsp); assert.deepEqual(errors, []); assert.deepEqual(blocked, []); assert.deepEqual(cspViolations, []);
    assert.equal(fingerprint(), beforeFingerprint, 'Tested source and build stayed fixed throughout the run'); passed = true;
  } catch (error) {failure = {name: error.name, message: error.message}; await shot('failure.png').catch(() => {}); console.error('Page errors:', errors); throw error;}
  finally {
    const finalState = await state().catch(() => null), currentSessionId = finalState?.document?.id ?? null;
    fs.writeFileSync(path.join(out, 'results.json'), JSON.stringify({passed, failure, startedAt, completedAt: new Date().toISOString(), baseRevision, workingTreeStatus,
      sourceInventory, sourceUnchangedDuringRun: fingerprint() === beforeFingerprint, runtime, backendKind, browser: browser.version(), node: process.version, base,
      sourceInventoryScope: 'Working-tree source/test identity; actual HTTP asset hashes establish served frontend bytes, not packaged Python source.', pageHash, contentSecurityPolicy: csp, loadedAssetHashes,
      currentSessionId, reviewedSessionId, reviewUrl: reviewedSessionId ? new URL('/remove?session=' + encodeURIComponent(reviewedSessionId), base).href : null,
      evidence: 'Full React UI and actual Python/FastAPI ' + backendKind + ' backend. UI/filechooser actions; read-only backend snapshots; no API mocks, model inference, model downloads or live providers. Installed Edge is not native WebView2 dialog acceptance; browser project downloads do not confirm native save.',
      checks, screenshots, outputs, metrics, requests, responses, errors, blocked, cspViolations, snapshots}, null, 2)); await browser.close();
  }
  console.log(`PASS full React integration: ${checks.length} groups; ${out}`);
}
main().catch(error => {console.error(error); process.exitCode = 1;});
