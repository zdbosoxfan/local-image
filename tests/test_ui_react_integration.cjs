// Actual Python/FastAPI + production React page acceptance. No API responses are
// mocked. Requires an explicitly named isolated profile and a user-authorized
// running source or packaged backend. Does not start services, run model inference,
// download models or install.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const crypto = require('node:crypto');
const {execFileSync} = require('node:child_process');
const {chromium} = require('playwright');

const root = path.resolve(__dirname, '..');
const base = process.env.LOCAL_REMOVE_TEST_URL;
const expectedProfile = process.env.MIGRATION_REAL_PROFILE;
const backendKind = process.env.MIGRATION_BACKEND_KIND || 'source';
assert.ok(['source', 'packaged'].includes(backendKind), 'MIGRATION_BACKEND_KIND must be source or packaged');
assert.ok(base && expectedProfile, 'Set LOCAL_REMOVE_TEST_URL and MIGRATION_REAL_PROFILE to the authorized isolated backend');
const baseUrl = new URL(base);
assert.equal(baseUrl.protocol, 'http:');
assert.equal(baseUrl.hostname, '127.0.0.1');
const qaRoot = path.join(root, 'qa-artifacts', 'migration');
const profile = path.resolve(expectedProfile);
assert.ok(profile.toLowerCase().startsWith(path.join(root, 'qa-artifacts').toLowerCase() + path.sep), 'Profile must be inside this repository QA directory');
const out = path.resolve(process.env.MIGRATION_REAL_OUTPUT || path.join(qaRoot, 'real-backend'));
assert.ok(out.toLowerCase().startsWith(qaRoot.toLowerCase() + path.sep), 'Output must be inside migration QA directory');
fs.mkdirSync(out, {recursive: true});
const python = process.env.MIGRATION_PYTHON || path.join(root, '.venv', 'Scripts', 'python.exe');
const hash = bytes => crypto.createHash('sha256').update(bytes).digest('hex');
function fileHash(file) {const data = fs.readFileSync(file); return {path: path.relative(root, file).replaceAll('\\', '/'), bytes: data.length, sha256: hash(data)};}
function files(directory) {return fs.readdirSync(directory, {withFileTypes: true}).flatMap(entry => entry.isDirectory() ? files(path.join(directory, entry.name)) : [path.join(directory, entry.name)]);}
const sourceFiles = [...files(path.join(root, 'backend/frontend')), ...files(path.join(root, 'backend/frontend_dist')), ...files(path.join(root, 'frontend/src')),
  ...['backend/local_remove.py', 'backend/local_remove_frontend.py', 'backend/frontend_tokens.py', 'backend/local_remove.html', 'backend/layer_stack.py', 'backend/local_remove_project.py', 'frontend/package.json', 'frontend/package-lock.json', 'tests/test_ui_react_integration.cjs'].map(file => path.join(root, file))];
const sourceInventory = sourceFiles.map(fileHash);
const fingerprint = () => sourceFiles.map(file => fileHash(file).sha256).join(':');
const beforeFingerprint = fingerprint();
const baseRevision = execFileSync('git', ['rev-parse', 'HEAD'], {cwd: root, encoding: 'utf8'}).trim();
const workingTreeStatus = execFileSync('git', ['status', '--porcelain=v1'], {cwd: root, encoding: 'utf8'}).trim();

async function main() {
  const startedAt = new Date().toISOString();
  const runtimeResponse = await fetch(new URL('/api/local-remove/runtime', base));
  assert.equal(runtimeResponse.status, 200, 'Read actual backend identity before any editing');
  const runtime = await runtimeResponse.json();
  assert.equal(path.resolve(runtime.data_root).toLowerCase(), profile.toLowerCase(), 'Backend must use the explicitly authorized isolated profile');
  const browser = await chromium.launch({headless: true, channel: process.env.BROWSER_CHANNEL || 'msedge'});
  const context = await browser.newContext({viewport: {width: 1366, height: 768}, acceptDownloads: true});
  const page = await context.newPage();
  const checks = [], screenshots = [], outputs = [], errors = [], blocked = [], requests = [], responses = [], metrics = [], persistedSnapshots = [], cspViolations = [];
  const loadedAssetHashes = [], assetReads = [];
  let documentContentHash = null, contentSecurityPolicy = null;
  let passed = false, failure = null;
  page.on('pageerror', error => errors.push(error.message));
  page.on('response', response => {
    const url = new URL(response.url()); responses.push({method: response.request().method(), path: url.pathname, status: response.status()});
    if (url.pathname.startsWith('/frontend-assets/')) assetReads.push(response.body().then(bytes => loadedAssetHashes.push({path: url.pathname, status: response.status(), bytes: bytes.length, sha256: hash(bytes), contentType: response.headers()['content-type']})));
  });
  page.on('request', request => {
    const url = new URL(request.url()); let payload;
    if (request.headers()['content-type']?.includes('application/json')) {
      try {payload = JSON.parse(request.postData() || '{}'); if (payload.mask) payload.mask = `[${payload.mask.length} base64 characters]`;} catch {}
    }
    requests.push({method: request.method(), path: url.pathname, ...(payload ? {payload} : {})});
  });
  await context.route('**/*', route => {
    const request = route.request(), url = new URL(request.url());
    let forbidden = url.origin !== baseUrl.origin;
    if (request.method() !== 'GET') {
      forbidden ||= /^\/api\/local-remove\/(?:generation(?:\/|$)|generator|qwen\/download|setup|loras\/download|stock\/import)/.test(url.pathname);
      forbidden ||= /\/cutout$|\/cutout\/generate-background$/.test(url.pathname);
      if (/\/remove$/.test(url.pathname)) {
        try {forbidden ||= JSON.parse(request.postData() || '{}').model !== 'heal';} catch {forbidden = true;}
      }
    }
    if (forbidden) {blocked.push({method: request.method(), url: url.href}); return route.abort('blockedbyclient');}
    return route.continue(); // No fixture fulfill, route substitution or API mocks.
  });
  await context.addInitScript(() => {
    for (const name of ['local-image.hardware-guide.v1', 'local-image.first-ai-setup.v1', 'local-image.first-task.v1']) localStorage.setItem(name, '1');
    localStorage.setItem('local-remove-operation', 'heal');
    window.__migrationCsp = [];
    document.addEventListener('securitypolicyviolation', event => window.__migrationCsp.push({directive: event.violatedDirective, blocked: event.blockedURI}));
  });
  const idle = () => page.waitForFunction(() => session && !busy && settingsLoaded && document.body.dataset.reactReady === 'true');
  const row = id => page.locator('#react-layers-root [data-layer-id="' + id + '"]');
  const selected = () => page.evaluate(() => LocalImageLayers.selected());
  const snapshot = async name => {
    const data = await page.evaluate(async () => (await api('/api/local-remove/session/' + encodeURIComponent(session.id))).json());
    persistedSnapshots.push({name, document: data}); return data;
  };
  const record = name => {checks.push(name); console.log('PASS: ' + name);};
  const shot = async name => {await page.mouse.move(4, 4); const file = path.join(out, name); await page.screenshot({path: file, animations: 'disabled'}); screenshots.push(fileHash(file));};
  const rect = async (x1, y1, x2, y2) => {
    await page.locator('[data-tool="rectangle"]').click();
    const bounds = await page.locator('#photo').boundingBox();
    await page.mouse.move(bounds.x + bounds.width * x1, bounds.y + bounds.height * y1); await page.mouse.down();
    await page.mouse.move(bounds.x + bounds.width * x2, bounds.y + bounds.height * y2, {steps: 6}); await page.mouse.up();
    await page.waitForFunction(() => hasSelection && !busy);
  };
  try {
    const start = performance.now();
    const documentResponse = await page.goto(new URL('/remove', base).href);
    documentContentHash = hash(await documentResponse.body()); contentSecurityPolicy = documentResponse.headers()['content-security-policy'];
    await page.waitForFunction(() => settingsLoaded && initCompleted && document.body.dataset.reactReady === 'true');
    metrics.push({emptyReadyMs: performance.now() - start, context: `Actual Python ${backendKind} backend and installed headless Edge, not native WebView2 interaction`} );
    const original = Buffer.from(await page.evaluate(() => {
      const canvas = document.createElement('canvas'); canvas.width = 640; canvas.height = 480;
      const paint = canvas.getContext('2d'); paint.fillStyle = '#529180'; paint.fillRect(0, 0, 640, 480);
      paint.fillStyle = '#d44d42'; paint.fillRect(260, 150, 120, 180);
      paint.fillStyle = '#ffe18c'; paint.fillRect(40, 40, 14, 14); paint.fillRect(80, 80, 14, 14);
      return canvas.toDataURL('image/png').split(',')[1];
    }), 'base64');
    const sourcePath = path.join(out, 'synthetic-original.png'); fs.writeFileSync(sourcePath, original); outputs.push(fileHash(sourcePath));
    await page.locator('#file').setInputFiles({name: 'Actual backend fixture.png', mimeType: 'image/png', buffer: original}); await idle();
    const canvas = await page.locator('#photo').elementHandle();
    const opened = await snapshot('uploaded-original');
    assert.equal(opened.revision, 0); assert.equal(opened.dirty, false); assert.equal(opened.layer_stack.length, 1);
    record('Actual browser upload creates an unchanged original and revision-zero stack');
    await page.getByRole('button', {name: 'New retouch layer', exact: true}).click(); await idle();
    const retouchId = (await selected()).id;
    assert.equal((await selected()).kind, 'retouch');
    await page.locator('#heal-brush').click(); await page.locator('#heal-method').selectOption('telea');
    for (const [x1, y1, x2, y2, count] of [[.05, .065, .10, .135, 1], [.112, .15, .16, .21, 2]]) {
      await rect(x1, y1, x2, y2); await page.locator('#remove').click();
      await page.waitForFunction(count => !busy && LocalImageLayers.selected()?.patch_ids?.length === count, count, {timeout: 60000});
    }
    const repaired = await snapshot('two-real-cpu-repairs');
    assert.equal(repaired.layers.length, 2); assert.equal(repaired.layer_stack.find(layer => layer.id === retouchId).patch_ids.length, 2);
    record('Two actual CPU Quick Heals persist real repair patches in the selected retouch layer');
    await row(retouchId).focus(); await page.keyboard.press('F2');
    await page.getByRole('textbox', {name: 'Layer name', exact: true}).fill('Dust cleanup'); await page.keyboard.press('Enter');
    await page.waitForFunction(() => !busy && LocalImageLayers.selected()?.name === 'Dust cleanup');
    const opacity = page.getByRole('spinbutton', {name: 'Layer opacity percent', exact: true});
    await opacity.fill('0'); await opacity.press('Enter'); await page.waitForFunction(() => !busy && LocalImageLayers.selected()?.opacity === 0);
    await opacity.fill('55'); await opacity.press('Enter'); await page.waitForFunction(() => !busy && LocalImageLayers.selected()?.opacity === .55);
    await page.getByRole('button', {name: 'Hide Dust cleanup', exact: true}).click(); await page.waitForFunction(() => !busy && !LocalImageLayers.selected()?.visible);
    await page.getByRole('button', {name: 'Show Dust cleanup', exact: true}).click(); await page.waitForFunction(() => !busy && LocalImageLayers.selected()?.visible);
    await page.getByRole('button', {name: 'Lock Dust cleanup', exact: true}).click(); await page.waitForFunction(() => !busy && LocalImageLayers.selected()?.locked); assert.equal(await opacity.isDisabled(), true);
    await page.getByRole('button', {name: 'Unlock Dust cleanup', exact: true}).click(); await page.waitForFunction(() => !busy && !LocalImageLayers.selected()?.locked);
    const edited = await snapshot('edited-retouch-metadata');
    assert.equal(edited.layer_stack.find(layer => layer.id === retouchId).opacity, .55);
    assert.equal(edited.layer_stack.find(layer => layer.id === retouchId).name, 'Dust cleanup');
    record('React rename, zero/55% opacity, visibility and locking reach and persist in the real backend');
    // Generate a synthetic alpha selection, then use the real CPU mask endpoint.
    // This is explicit test input; no segmentation/generation inference executes.
    await page.evaluate(async () => {
      const mask = document.createElement('canvas'); mask.width = session.width; mask.height = session.height;
      const paint = mask.getContext('2d'); paint.fillStyle = '#000'; paint.fillRect(0, 0, mask.width, mask.height); paint.fillStyle = '#fff'; paint.fillRect(260, 150, 120, 180);
      const data = await json(url('/cutout/refine'), {revision: session.revision, operation: 'replace', mask: mask.toDataURL('image/png').split(',')[1]});
      await openSession(data); setWorkspace('cutout');
    }); await idle();
    const cutout = await selected(), cutoutId = cutout.id;
    assert.equal(cutout.kind, 'cutout');
    const originalId = await page.evaluate(() => LocalImageLayers.nodes().find(layer => layer.kind === 'original').id);
    await page.getByRole('button', {name: 'Hide Original', exact: true}).click(); await idle();
    await page.getByRole('button', {name: 'Hide Dust cleanup', exact: true}).click(); await idle();
    assert.equal((await selected()).id, cutoutId);
    await page.locator('#move-subject').click();
    const photo = await page.locator('#photo').boundingBox(), requestStart = requests.length;
    await page.mouse.move(photo.x + photo.width * .5, photo.y + photo.height * .5); await page.mouse.down();
    assert.equal(await page.evaluate(() => document.querySelector('#viewport').hasPointerCapture(1)), true);
    await page.mouse.move(photo.x + photo.width * .6, photo.y + photo.height * .55, {steps: 12}); await page.mouse.up();
    await page.waitForFunction(() => !busy && LocalImageLayers.selected()?.transform.offset_x > 40);
    assert.equal(requests.slice(requestStart).filter(request => request.method === 'PATCH').length, 1);
    await page.locator('#stack-transform-scale').fill('120'); await page.locator('#stack-transform-scale').press('Tab'); await page.waitForFunction(() => !busy && LocalImageLayers.selected()?.transform.scale === 1.2);
    await page.locator('#stack-transform-rotation').fill('12'); await page.locator('#stack-transform-rotation').press('Tab'); await page.waitForFunction(() => !busy && LocalImageLayers.selected()?.transform.rotation === 12);
    const transformed = await snapshot('actual-transformed-cutout');
    await shot('real-transformed-1366x768.png');
    record('Manual alpha creates a real editable cutout; persistent pointer gesture commits one backend transform');
    const beforeUndo = (await selected()).transform;
    const undo = page.locator('#react-shell-root').getByRole('button', {name: 'Undo', exact: true}), redo = page.locator('#react-shell-root').getByRole('button', {name: 'Redo', exact: true});
    await undo.click(); await page.waitForFunction(() => !busy && LocalImageLayers.selected()?.transform.rotation === 0);
    await redo.click(); await page.waitForFunction(() => !busy && LocalImageLayers.selected()?.transform.rotation === 12);
    assert.deepEqual((await selected()).transform, beforeUndo);
    await rect(.57, .4, .62, .5);
    const selectionRevision = await page.evaluate(() => session.revision);
    await undo.click(); await page.waitForFunction(() => !hasSelection && !busy);
    assert.equal(await page.evaluate(() => session.revision), selectionRevision);
    await redo.click(); await page.waitForFunction(() => hasSelection && !busy);
    assert.equal(await page.evaluate(() => session.revision), selectionRevision);
    await page.locator('#cutout-refine').click(); await page.waitForFunction(revision => !busy && !hasSelection && session.revision > revision, selectionRevision);
    record('Real stack Undo/Redo and browser-selection priority work; transformed mask refinement persists');
    const snapshotBeforeReorder = await snapshot('before-reorder');
    await page.getByRole('button', {name: 'Layer commands', exact: true}).click(); await page.getByRole('menuitem', {name: 'Move layer down', exact: true}).click(); await idle();
    const moved = await snapshot('layer-moved-down');
    assert.equal(moved.layer_stack.findIndex(layer => layer.id === cutoutId), snapshotBeforeReorder.layer_stack.findIndex(layer => layer.id === cutoutId) - 1);
    await page.getByRole('button', {name: 'Layer commands', exact: true}).click(); await page.getByRole('menuitem', {name: 'Move layer up', exact: true}).click(); await idle();
    const beforeProject = await snapshot('before-project-export');
    assert.equal(beforeProject.layer_stack.findIndex(layer => layer.id === cutoutId), snapshotBeforeReorder.layer_stack.findIndex(layer => layer.id === cutoutId));
    assert.equal(beforeProject.layer_stack.find(layer => layer.id === originalId).visible, false);
    assert.equal(await canvas.evaluate(node => node === document.querySelector('#photo')), true);
    record('Fluent layer reorder changes the actual persisted order while the canvas remains mounted');
    const projectWait = page.waitForEvent('download');
    await page.locator('#file-menu-trigger').click(); await page.locator('#save-project').click();
    const projectDownload = await projectWait, projectPath = path.join(out, 'real-layer-project.lremove');
    await projectDownload.saveAs(projectPath); assert.equal(await projectDownload.failure(), null); await idle(); outputs.push(fileHash(projectPath));
    assert.equal(await page.evaluate(() => session.project_saved), false, 'Browser download completion cannot grant native-save authority');
    const archive = JSON.parse(execFileSync(python, ['-c', "import sys,zipfile,json,hashlib; z=zipfile.ZipFile(sys.argv[1]); m=json.loads(z.read('manifest.json')); print(json.dumps({'manifest':m,'original_sha256':hashlib.sha256(z.read(m['original'])).hexdigest(),'entries':z.namelist()}))", projectPath], {encoding: 'utf8', maxBuffer: 4 * 1024 * 1024}));
    assert.equal(archive.manifest.version, 3); assert.equal(archive.original_sha256, hash(original));
    const oldId = await page.evaluate(() => session.id);
    await page.locator('#project-file').setInputFiles(projectPath); await page.waitForFunction(id => session?.id !== id && !busy, oldId); await idle();
    const reopened = await snapshot('actual-project-reopened');
    assert.deepEqual(reopened.layer_stack, beforeProject.layer_stack); assert.deepEqual(reopened.layers, beforeProject.layers);
    assert.equal(reopened.can_return, false); assert.equal(reopened.has_project_path, false);
    record('Downloaded v3 .lremove contains exact original bytes; real reopen retains all layers, repair links and masks');
    const exportWait = page.waitForEvent('download');
    await page.locator('#react-shell-root').getByRole('button', {name: 'Export', exact: true}).click();
    const imageDownload = await exportWait, exportPath = path.join(out, 'real-alpha-export.png');
    await imageDownload.saveAs(exportPath); assert.equal(await imageDownload.failure(), null); await idle(); outputs.push(fileHash(exportPath));
    const exported = fs.readFileSync(exportPath); assert.equal(exported.subarray(0, 8).toString('hex'), '89504e470d0a1a0a');
    assert.equal(exported.readUInt32BE(16), 640); assert.equal(exported.readUInt32BE(20), 480);
    const pixels = await page.evaluate(async bytes => {
      const image = new Image(); image.src = 'data:image/png;base64,' + bytes; await image.decode();
      const c = document.createElement('canvas'); c.width = image.width; c.height = image.height; const ctx = c.getContext('2d'); ctx.drawImage(image, 0, 0);
      const data = ctx.getImageData(0, 0, c.width, c.height).data; let opaque = 0, transparent = 0;
      for (let i = 3; i < data.length; i += 4) {if (data[i] === 0) transparent++; if (data[i] === 255) opaque++;}
      return {corner: Array.from(data.slice(0, 4)), opaque, transparent};
    }, exported.toString('base64'));
    assert.equal(pixels.corner[3], 0); assert.ok(pixels.opaque > 1000); assert.ok(pixels.transparent > 1000);
    metrics.push({actualExportPixels: pixels});
    record('Actual Python PNG export has correct size and both opaque subject pixels and transparent background');
    await page.locator('#move-subject').click();
    for (const [width, height] of [[800, 560], [1366, 768], [1920, 1080]]) {await page.setViewportSize({width, height}); await page.evaluate(() => LocalImageLegacyEditor.canvas.fit()); await shot(`real-reopened-${width}x${height}.png`);}
    await page.locator('#document-close').click(); await page.locator('#close-dialog').waitFor({state: 'visible'}); await page.locator('#close-cancel').click();
    assert.equal(await page.evaluate(() => session.id), reopened.id); assert.equal(await page.evaluate(() => LocalImageLayers.nodes().length), reopened.layer_stack.length);
    record('Cancel close preserves the reopened editable document after browser project download');
    assert.deepEqual(errors, []); assert.deepEqual(blocked, []);
    await Promise.all(assetReads);
    assert.ok(loadedAssetHashes.some(asset => asset.path.endsWith('.js') && asset.status === 200), 'Production JavaScript asset actually loaded');
    assert.ok(loadedAssetHashes.some(asset => asset.path.endsWith('.css') && asset.status === 200), 'Production stylesheet actually loaded');
    cspViolations.push(...await page.evaluate(() => window.__migrationCsp));
    assert.deepEqual(cspViolations, []);
    assert.equal(fingerprint(), beforeFingerprint, 'Application/build source did not change while acceptance was running');
    passed = true;
  } catch (error) {
    failure = {name: error.name, message: error.message};
    await shot('failure.png').catch(() => {});
    console.error('Page errors:', errors); throw error;
  } finally {
    const currentSessionId = await page.evaluate(() => session?.id || null).catch(() => null);
    const reviewUrl = currentSessionId ? new URL('/remove?session=' + encodeURIComponent(currentSessionId), base).href : null;
    fs.writeFileSync(path.join(out, 'results.json'), JSON.stringify({passed, failure, startedAt, completedAt: new Date().toISOString(), baseRevision, workingTreeStatus,
      sourceState: workingTreeStatus ? 'Dirty working tree; sourceInventory identifies the actual tested files' : 'Clean working tree', sourceInventory, sourceUnchangedDuringRun: fingerprint() === beforeFingerprint,
      runtime, backendKind, expectedProfile: profile, browser: browser.version(), node: process.version, base, currentSessionId, reviewUrl,
      sourceInventoryScope: 'Working-tree source and test harness identity; loadedAssetHashes record actual HTTP bundle bytes. For packaged runs, working-tree hashes do not certify the packaged Python source.',
      documentContentHash, contentSecurityPolicy, loadedAssetHashes,
      evidence: `Actual Python/FastAPI ${backendKind} backend with real image/project/export operations and production React bundle, through installed Edge. No API mocks, model inference, model downloads or live provider requests. Browser automation is not native WebView2 interaction or native dialog acceptance; project download is not native save confirmation.`,
      checks, screenshots, outputs, metrics, errors, blocked, cspViolations, requests, responses, persistedSnapshots}, null, 2));
    await browser.close();
  }
  console.log(`PASS real React/backend integration: ${checks.length} checks; ${out}`);
}
main().catch(error => {console.error(error); process.exitCode = 1;});
