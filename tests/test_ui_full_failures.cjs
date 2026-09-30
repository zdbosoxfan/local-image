// Production React page + actual isolated backend writes/conflicts. The only
// controlled failure is one aborted preview GET after a real accepted PATCH.
const assert = require('node:assert/strict'), fs = require('node:fs'), path = require('node:path'), crypto = require('node:crypto');
const { chromium } = require('playwright');
const { imageFixture } = require('./helpers/image-fixture.cjs');
const root = path.resolve(__dirname, '..'), base = process.env.LOCAL_REMOVE_TEST_URL, profile = process.env.MIGRATION_REAL_PROFILE;
assert.ok(base && profile); assert.equal(new URL(base).hostname, '127.0.0.1');
assert.ok(path.resolve(profile).startsWith(path.join(root, 'qa-artifacts') + path.sep));
const output = path.join(root, 'qa-artifacts/migration/full-failures'), expectedBundle = process.env.EXPECTED_FRONTEND_BUNDLE || 'main-Cgd1OQZH.js';
const hash = bytes => crypto.createHash('sha256').update(bytes).digest('hex');
const sourceNames = ['frontend/src/editor/documentController.ts', 'frontend/src/editor/documentApi.ts', 'frontend/src/editor/canvasController.ts', 'frontend/src/features/shell/Layers.tsx', 'frontend/src/features/shell/executeCommand.ts', 'frontend/src/application.ts', 'backend/frontend_dist/.vite/manifest.json'];
const sources = () => sourceNames.map(name => ({ path: name, sha256: hash(fs.readFileSync(path.join(root, name))) }));

(async () => {
  const runtime = await (await fetch(base + '/api/local-remove/runtime')).json();
  assert.equal(path.resolve(runtime.data_root), path.resolve(profile));
  fs.mkdirSync(output, { recursive: true });
  if (fs.existsSync(path.join(output, 'results.json'))) {
    const archive = path.join(output, 'report-history'), stamp = Date.now(); fs.mkdirSync(archive, { recursive: true });
    fs.copyFileSync(path.join(output, 'results.json'), path.join(archive, `results-${stamp}.json`));
    if (fs.existsSync(path.join(output, 'failure.png'))) fs.copyFileSync(path.join(output, 'failure.png'), path.join(archive, `failure-${stamp}.png`));
  }
  const beforeSources = sources(), sourcePath = path.join(output, 'failure-source.png');
  const bytes = imageFixture(640, 480); fs.writeFileSync(sourcePath, bytes);
  const browser = await chromium.launch({ channel: 'msedge', headless: true });
  const context = await browser.newContext({ viewport: { width: 1366, height: 768 } });
  const requests = [], responses = [], assets = [], errors = [], aborts = [], blocked = [], checks = [], screenshots = [], responseReads = [];
  const requestRecords = new WeakMap(), pages = [];
  let phase = 'setup', armed = null, passed = false, failure = null, sessionId = null, previewRecovery = null, revisionConflict = null;
  await context.addInitScript(() => {
    for (const name of ['local-image.hardware-guide.v1', 'local-image.first-ai-setup.v1', 'local-image.first-task.v1']) localStorage.setItem(name, '1');
    window.__failureCsp = []; document.addEventListener('securitypolicyviolation', event => window.__failureCsp.push({ directive: event.violatedDirective, blocked: event.blockedURI }));
  });
  await context.route('**/*', route => {
    const request = route.request(), url = new URL(request.url());
    if (url.origin !== base) { blocked.push({ method: request.method(), path: url.pathname }); return route.abort('blockedbyclient'); }
    if (/\/(?:generation$|generator|setup|loras\/download|stock\/(?:search|import))/.test(url.pathname)) { blocked.push({ method: request.method(), path: url.pathname }); return route.abort('blockedbyclient'); }
    if (armed && request.method() === 'GET' && url.pathname === armed.path && url.searchParams.get('revision') === String(armed.revision)) {
      aborts.push({ page: request.frame().page() === pages[0] ? 'A' : 'B', path: url.pathname, revision: armed.revision, reason: 'Controlled single preview transport failure after accepted layer PATCH' });
      armed = null; return route.abort('failed');
    }
    return route.continue();
  });
  async function pageFor(name) {
    const page = await context.newPage(); pages.push(page);
    page.on('pageerror', error => errors.push({ page: name, message: error.stack }));
    page.on('request', request => {
      const url = new URL(request.url()); if (!url.pathname.startsWith('/api/')) return;
      const item = { page: name, phase, method: request.method(), path: url.pathname, revision: url.searchParams.get('revision') || undefined };
      if (request.method() === 'PATCH') { const body = request.postDataJSON(); item.payload = { revision: body.revision, opacity: body.opacity, locked: body.locked }; }
      requests.push(item); requestRecords.set(request, item);
    });
    page.on('response', response => {
      const url = new URL(response.url()), item = requestRecords.get(response.request());
      if (item) {
        const result = { ...item, status: response.status() }; responses.push(result);
        if (item.method === 'PATCH') responseReads.push(response.json().then(data => { result.acceptedRevision = data.revision; result.detail = data.detail; }).catch(() => {}));
      }
      if (url.pathname.startsWith('/frontend-assets/')) responseReads.push(response.body().then(body => assets.push({ page: name, path: url.pathname, sha256: hash(body), status: response.status() })));
    });
    return page;
  }
  const state = page => page.evaluate(() => window.LocalImageEditor.getSnapshot());
  const ready = page => page.waitForFunction(() => document.body.dataset.reactReady === 'true' && !window.LocalImageEditor.getSnapshot().busy);
  const idle = page => page.waitForFunction(() => !window.LocalImageEditor.getSnapshot().busy);
  const menu = async (page, group, label) => { await page.getByRole('menuitem', { name: group, exact: true }).click(); await page.getByRole('menuitem', { name: new RegExp('^' + label + '(?:\\s+Ctrl\\+.*)?$') }).click(); };
  const opacity = async (page, value) => { const input = page.getByRole('spinbutton', { name: 'Layer opacity percent', exact: true }); await input.fill(String(value)); await input.press('Enter'); };
  const screenshot = async (page, name) => { const file = path.join(output, name); await page.screenshot({ path: file, animations: 'disabled' }); screenshots.push(file); };
  const layer = document => document.layer_stack.find(item => item.id === 'original');
  const backendDocument = async () => { const response = await fetch(`${base}/api/local-remove/session/${sessionId}`); assert.equal(response.status, 200); return response.json(); };
  const writes = (page, label) => requests.filter(item => item.page === page && item.phase === label && item.method === 'PATCH');
  const pageA = await pageFor('A');
  try {
    const response = await pageA.goto(base + '/remove');
    const htmlHashA = hash(await response.body()); await ready(pageA);
    const [chooser] = await Promise.all([pageA.waitForEvent('filechooser'), menu(pageA, 'File', 'Open images…')]); await chooser.setFiles(sourcePath);
    await pageA.waitForFunction(() => !!window.LocalImageEditor.getSnapshot().document && !window.LocalImageEditor.getSnapshot().busy);
    sessionId = (await state(pageA)).document.id;
    await pageA.getByRole('button', { name: 'Unlock Original', exact: true }).click();
    await pageA.waitForFunction(() => !window.LocalImageEditor.getSnapshot().busy && !window.LocalImageEditor.getSnapshot().document.layer_stack.find(item => item.id === 'original').locked);
    const baseline = (await state(pageA)).document;
    phase = 'preview-failure'; armed = { path: `/api/local-remove/session/${sessionId}/preview`, revision: baseline.revision + 1 };
    await opacity(pageA, 60);
    await pageA.waitForFunction(() => !window.LocalImageEditor.getSnapshot().busy && window.LocalImageEditor.getSnapshot().statusError);
    const failed = await state(pageA), persisted = await backendDocument();
    assert.match(failed.status, /change was accepted.*preview could not be loaded/i); assert.equal(failed.document.revision, baseline.revision + 1);
    assert.equal(layer(failed.document).opacity, .6); assert.equal(layer(persisted).opacity, .6); assert.equal(persisted.revision, failed.document.revision);
    assert.equal(aborts.length, 1); assert.equal(writes('A', phase).length, 1);
    await screenshot(pageA, 'accepted-preview-error.png');
    phase = 'preview-recovery'; await menu(pageA, 'View', 'Refresh preview');
    await pageA.waitForFunction(() => !window.LocalImageEditor.getSnapshot().busy && window.LocalImageEditor.getSnapshot().status === 'View refreshed.' && !window.LocalImageEditor.getSnapshot().statusError);
    await pageA.waitForFunction(revision => { const image = document.querySelector('#photo-image'); return image.complete && image.naturalWidth > 0 && !image.hidden && new URL(image.src).searchParams.get('revision') === String(revision); }, failed.document.revision);
    const recovered = await state(pageA), afterRecovery = await backendDocument();
    assert.equal(recovered.document.revision, failed.document.revision); assert.equal(afterRecovery.revision, failed.document.revision);
    assert.equal(writes('A', 'preview-failure').length, 1); assert.equal(writes('A', phase).length, 0);
    assert.equal(await pageA.locator('.li-status-error').count(), 0); assert.equal(await pageA.locator('#photo-image').isVisible(), true);
    const image = await pageA.locator('#photo-image').evaluate(value => ({ width: value.naturalWidth, height: value.naturalHeight, source: new URL(value.src).pathname + new URL(value.src).search }));
    const rendered = await pageA.locator('#photo-image').screenshot(); const preview = await (await fetch(base + image.source)).arrayBuffer();
    previewRecovery = { sessionId, acceptedRevision: failed.document.revision, acceptedOpacity: .6, failureMessage: failed.status, recoveryMessage: recovered.status, image, previewSha256: hash(Buffer.from(preview)), renderedSha256: hash(rendered), htmlHashA };
    await screenshot(pageA, 'preview-recovered.png'); checks.push('Accepted real layer PATCH survives one controlled preview GET abort; View → Refresh preview reloads pixels and clears error without repeating the mutation');

    phase = 'second-page'; const pageB = await pageFor('B'), responseB = await pageB.goto(`${base}/remove?session=${encodeURIComponent(sessionId)}`);
    const htmlHashB = hash(await responseB.body()); await ready(pageB); const staleBefore = await state(pageB); assert.equal(staleBefore.document.revision, failed.document.revision);
    phase = 'conflict-accepted'; await opacity(pageA, 40); await pageA.waitForFunction(revision => !window.LocalImageEditor.getSnapshot().busy && window.LocalImageEditor.getSnapshot().document.revision === revision, failed.document.revision + 1);
    assert.equal((await state(pageB)).document.revision, staleBefore.document.revision);
    phase = 'conflict-stale'; const conflictResponse = pageB.waitForResponse(value => value.request().method() === 'PATCH' && value.status() === 409);
    await opacity(pageB, 75); const actualConflict = await conflictResponse, conflictBody = await actualConflict.json(); await idle(pageB);
    const staleAfter = await state(pageB), authoritative = await backendDocument();
    assert.equal(staleAfter.statusError, true); assert.match(staleAfter.status, /edit changed|revision|reload/i); assert.doesNotMatch(staleAfter.status, /Layer updated|View refreshed/);
    assert.equal(staleAfter.document.revision, staleBefore.document.revision); assert.equal(layer(staleAfter.document).opacity, .6);
    assert.equal(authoritative.revision, failed.document.revision + 1); assert.equal(layer(authoritative).opacity, .4);
    await pageB.waitForTimeout(500); assert.equal(writes('B', phase).length, 1); assert.equal((await backendDocument()).revision, authoritative.revision);
    revisionConflict = { sessionId, staleRevision: staleBefore.document.revision, acceptedRevision: authoritative.revision, acceptedOpacity: .4, rejectedOpacity: .75, status: actualConflict.status(), detail: conflictBody.detail, displayedError: staleAfter.status, stalePagePatchAttempts: writes('B', phase).length, htmlHashB };
    await screenshot(pageB, 'real-revision-conflict.png'); checks.push('Two real UI pages share one QA session; stale page receives actual HTTP 409, retains accepted metadata and makes exactly one rejected attempt');
    await Promise.all(responseReads);
    assert.deepEqual(responses.filter(item => item.page === 'A' && item.phase === 'preview-failure' && item.method === 'PATCH').map(item => [item.status, item.acceptedRevision]), [[200, previewRecovery.acceptedRevision]]);
    assert.ok(responses.some(item => item.phase === 'preview-recovery' && item.path === `/api/local-remove/session/${sessionId}/preview` && item.status === 200));
    assert.deepEqual(responses.filter(item => item.page === 'B' && item.phase === 'conflict-stale' && item.method === 'PATCH').map(item => item.status), [409]);
    assert.ok(assets.some(item => item.path.endsWith('/' + expectedBundle)), 'The expected frozen production bundle was actually loaded');
    assert.deepEqual(sources(), beforeSources, 'App sources and manifest remain frozen throughout acceptance');
    assert.deepEqual(errors, []); assert.deepEqual(blocked, []);
    for (const page of pages) assert.deepEqual(await page.evaluate(() => window.__failureCsp), []);
    passed = true;
  } catch (error) { failure = error.stack; await screenshot(pages.at(-1), 'failure.png').catch(() => {}); throw error; }
  finally {
    await Promise.allSettled(responseReads);
    fs.writeFileSync(path.join(output, 'results.json'), JSON.stringify({ passed, failure, runtime, expectedBundle, checks, source: { path: sourcePath, sha256: hash(bytes) }, sources: beforeSources, assets, requests, responses, aborts, blocked, errors, previewRecovery, revisionConflict, screenshots, evidence: 'Actual isolated backend and production UI; one controlled aborted image GET; actual accepted PATCH and real revision409; no provider/model/native operations' }, null, 2));
    await browser.close();
  }
  console.log(`PASS full UI failure recovery: ${checks.length} groups; ${output}`);
})().catch(error => { console.error(error); process.exitCode = 1; });
