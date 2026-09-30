// Composed-page migration acceptance with a local fixture transport only.
// No app service, provider, GPU job, installed profile or native host is started.
// The Python renderer and real browser scripts run; backend image/save semantics
// remain fixture behavior and must be checked separately by the Python suites.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const http = require('node:http');
const crypto = require('node:crypto');
const {execFileSync} = require('node:child_process');
const {chromium} = require('playwright');

const root = path.resolve(__dirname, '..');
const baseline = process.argv.includes('--baseline');
const rollback = process.argv.includes('--legacy');
const react = !baseline && !rollback;
const revision = 'e42aab6a49f222796dbbc9dc1ebf4211f46e9876';
const out = path.resolve(process.env.MIGRATION_OUTPUT || path.join(root, 'qa-artifacts', 'migration', baseline ? 'before' : rollback ? 'rollback' : 'after'));
const python = process.env.MIGRATION_PYTHON || 'C:/Users/Owner/.cache/codex-runtimes/codex-primary-runtime/dependencies/python/python.exe';
fs.mkdirSync(out, {recursive: true});

function sourceRoot() {
  if (!baseline) return path.join(root, 'backend');
  const target = path.join(out, 'source', 'backend');
  const files = execFileSync('git', ['ls-tree', '-r', '--name-only', revision, 'backend/frontend', 'backend/local_remove.html', 'backend/local_remove_frontend.py'], {cwd: root, encoding: 'utf8'}).trim().split(/\r?\n/);
  for (const file of files) {
    const destination = path.join(out, 'source', file);
    fs.mkdirSync(path.dirname(destination), {recursive: true});
    fs.writeFileSync(destination, execFileSync('git', ['show', revision + ':' + file], {cwd: root, maxBuffer: 10 * 1024 * 1024}));
  }
  return target;
}

const sources = sourceRoot();
const nonce = 'migration-test-nonce';
const token = 'migration-fixture-token';
const startedAt = new Date().toISOString();
const baseRevision = execFileSync('git', ['rev-parse', 'HEAD'], {cwd: root, encoding: 'utf8'}).trim();
const workingTreeStatus = execFileSync('git', ['status', '--porcelain=v1', '--untracked-files=normal'], {cwd: root, encoding: 'utf8'}).trim();
function hashFile(file, label) {
  const bytes = fs.readFileSync(file);
  return {path: label || path.relative(root, file).replaceAll('\\', '/'), bytes: bytes.length, sha256: crypto.createHash('sha256').update(bytes).digest('hex')};
}
function filesUnder(directory) {
  if (!fs.existsSync(directory)) return [];
  return fs.readdirSync(directory, {withFileTypes: true}).flatMap(entry => entry.isDirectory() ? filesUnder(path.join(directory, entry.name)) : [path.join(directory, entry.name)]);
}
const evidenceFiles = baseline
  ? filesUnder(sources).filter(file => !file.includes('__pycache__'))
  : [...filesUnder(path.join(root, 'backend/frontend')), ...filesUnder(path.join(root, 'backend/frontend_dist')), ...filesUnder(path.join(root, 'frontend/src')),
    'backend/local_remove_frontend.py', 'backend/local_remove.html', 'backend/local_remove.py', 'backend/frontend_tokens.py',
    'frontend/package.json', 'frontend/package-lock.json', 'frontend/vite.config.ts', 'frontend/tsconfig.json', 'tests/test_ui_migration.cjs'].map(file => path.isAbsolute(file) ? file : path.join(root, file)).filter(file => fs.existsSync(file));
const sourceInventory = [...new Set(evidenceFiles)].sort().map(file => hashFile(file, baseline ? 'backend/' + path.relative(sources, file).replaceAll('\\', '/') : undefined));
const sourceFingerprint = () => evidenceFiles.map(file => hashFile(file).sha256).join(':');
const beforeFingerprint = sourceFingerprint();
function render() {
  const call = baseline ? 'render_editor(sys.argv[2],sys.argv[3])' : 'render_editor(sys.argv[2],sys.argv[3],sys.argv[4])';
  return execFileSync(python, ['-c', 'import sys; sys.path.insert(0, sys.argv[1]); from local_remove_frontend import render_editor; print(' + call + ')', sources, nonce, token, react ? 'react' : 'legacy'], {cwd: root, encoding: 'utf8', maxBuffer: 10 * 1024 * 1024, env: {...process.env, PYTHONIOENCODING: 'utf-8'}});
}
const html = render();
const assets = react ? JSON.parse(execFileSync(python, ['-c', "import sys,json; sys.path.insert(0,sys.argv[1]); from local_remove_frontend import frontend_manifest,frontend_asset; print(json.dumps({name:[str(frontend_asset(name)[0]),frontend_asset(name)[1]] for name in frontend_manifest()[2]}))", sources], {cwd: root, encoding: 'utf8'})) : {};
function pageCsp(origin) {
  const source = react ? ' ' + origin + '/frontend-assets/' : '';
  return `default-src 'none'; base-uri 'none'; frame-ancestors 'none'; style-src 'nonce-${nonce}'${source}; script-src 'nonce-${nonce}'${source}; ${react ? 'font-src ' + origin + '/frontend-assets/; ' : ''}img-src 'self' blob: data: https://images.unsplash.com https://plus.unsplash.com; connect-src 'self'; form-action 'self'`;
}
const clone = value => JSON.parse(JSON.stringify(value));
const width = 4096, height = 2732;
function documentFixture() {
  const transform = {offset_x: 96, offset_y: -40, scale: .84, rotation: 12};
  return {id: 'migration-fixture', name: 'Studio still life — 4096 × 2732.png', width, height, bit_depth: 8,
    revision: 3, layers: [], dirty: true, project_dirty: true, saved: false, project_saved: false,
    edited: true, can_return: false, stack_can_undo: true, stack_can_redo: false,
    selected_layer_id: 'subject', attribution: [],
    layer_stack: [
      {id: 'original', kind: 'original', name: 'Original', visible: true, locked: true, opacity: 1, transform: {offset_x: 0, offset_y: 0, scale: 1, rotation: 0}, bounds: [0, 0, width, height]},
      {id: 'cleanup', kind: 'retouch', name: 'Surface cleanup', visible: true, locked: false, opacity: .65, patch_ids: [], transform: {offset_x: 0, offset_y: 0, scale: 1, rotation: 0}, bounds: [0, 0, width, height]},
      {id: 'subject', kind: 'cutout', name: 'Ceramic study', visible: true, locked: false, opacity: .92, transform, bounds: [1340, 720, 2700, 2180], cutout: {enabled: true, feather: 2, shadow: {enabled: true, opacity: .3, blur: 18, offset_x: 12, offset_y: 20, squeeze: 1}}}
    ]};
}

async function main() {
  let passed = false, failure = null;
  const browser = await chromium.launch({headless: true, channel: process.env.BROWSER_CHANNEL || 'msedge'});
  const context = await browser.newContext({viewport: {width: 1366, height: 768}});
  const page = await context.newPage();
  // Generated fixture pixels have no dependence on user photos or AI.
  const images = await page.evaluate(({width, height}) => {
    const make = () => {const c = document.createElement('canvas'); c.width = width; c.height = height; return c;};
    const base = make(), b = base.getContext('2d');
    const gradient = b.createLinearGradient(0, 0, width, height); gradient.addColorStop(0, '#526566'); gradient.addColorStop(1, '#a8a18d'); b.fillStyle = gradient; b.fillRect(0, 0, width, height);
    b.fillStyle = '#a18e76'; b.fillRect(0, 1980, width, height - 1980);
    for (let x = 0; x < width; x += 13) {b.fillStyle = 'rgba(245,227,195,.018)'; b.fillRect(x, 1980, 3, height - 1980);}
    const subject = make(), s = subject.getContext('2d');
    const clay = s.createLinearGradient(1380, 0, 2620, 0); clay.addColorStop(0, '#9b5436'); clay.addColorStop(.35, '#d8a47a'); clay.addColorStop(.7, '#bd7c57'); clay.addColorStop(1, '#7e402e');
    s.fillStyle = clay; s.beginPath(); s.moveTo(1660, 810); s.bezierCurveTo(1720, 1150, 1390, 1280, 1380, 1640); s.bezierCurveTo(1350, 2290, 2660, 2320, 2690, 1640); s.bezierCurveTo(2700, 1290, 2360, 1130, 2370, 810); s.closePath(); s.fill(); s.fillStyle = '#683e2b'; s.beginPath(); s.ellipse(2015, 810, 355, 90, 0, 0, 2 * Math.PI); s.fill();
    const composite = make(), c = composite.getContext('2d'); c.drawImage(base, 0, 0); c.save(); c.translate(width / 2 + 96, height / 2 - 40); c.rotate(12 * Math.PI / 180); c.scale(.84, .84); c.globalAlpha = .92; c.shadowColor = 'rgba(20,28,28,.3)'; c.shadowBlur = 18; c.shadowOffsetX = 12; c.shadowOffsetY = 20; c.drawImage(subject, -width / 2, -height / 2); c.restore();
    const empty = make();
    return Object.fromEntries(Object.entries({base, subject, composite, empty}).map(([key, canvas]) => [key, canvas.toDataURL('image/png').split(',')[1]]));
  }, {width, height});
  const png = Object.fromEntries(Object.entries(images).map(([key, value]) => [key, Buffer.from(value, 'base64')]));
  let document = documentFixture();
  const otherDocument = {...clone(document), id: 'migration-other', name: 'Other fixture.png', revision: 0};
  const documents = new Map([[document.id, document], [otherDocument.id, otherDocument]]);
  let failNext = null, holdNext = false, releaseHeld = null;
  const requests = [], unknown = [], pageErrors = [], cspViolations = [], screenshots = [], metrics = [], checks = [];
  const server = http.createServer(async (request, response) => {
    const parsed = new URL(request.url, 'http://127.0.0.1'), pathname = parsed.pathname;
    let body = ''; for await (const chunk of request) body += chunk;
    requests.push({method: request.method, path: pathname, body: body ? JSON.parse(body) : null});
    const json = (value, status = 200) => {response.writeHead(status, {'Content-Type': 'application/json', 'Cache-Control': 'no-store'}); response.end(JSON.stringify(value));};
    if (request.method !== 'GET' && pathname.startsWith('/api/') && request.headers['x-local-remove-token'] !== token) return json({detail: 'Missing fixture browser token'}, 403);
    if (pathname === '/remove') {response.writeHead(200, {'Content-Type': 'text/html; charset=utf-8', 'Content-Security-Policy': pageCsp('http://' + request.headers.host), 'Cache-Control': 'no-store'}); response.end(html); return;}
    if (pathname.startsWith('/frontend-assets/')) {
      const asset = assets[pathname.slice('/frontend-assets/'.length)];
      if (!asset) {response.writeHead(404); response.end(); return;}
      response.writeHead(200, {'Content-Type': asset[1], 'X-Content-Type-Options': 'nosniff', 'Cache-Control': 'private, max-age=31536000, immutable'}); response.end(fs.readFileSync(asset[0])); return;
    }
    if (pathname.endsWith('/settings')) return json({model: 'klein', models: [{id: 'klein', label: 'FLUX.2 Klein', available: false, reason: 'Controlled fixture: model not installed'}, {id: 'heal', label: 'Quick Heal', available: true}]});
    if (pathname.endsWith('/qwen/status')) return json({available: false, variants: [], reason: 'Controlled fixture: model not installed'});
    if (pathname.endsWith('/status')) return json({ready: false, retouch_ready: true, device: 'Controlled fixture; no model process'});
    if (pathname.endsWith('/sessions')) return json([]);
    if (pathname.endsWith('/generation/models')) return json({models: []});
    if (pathname.endsWith('/backgrounds')) return json({libraries: []});
    if (pathname.endsWith('/generation/library')) return json({items: []});
    if (pathname.endsWith('/stock/providers')) return json({providers: [], default_provider: null});
    if (pathname.endsWith('/setup')) return json({setup_mode: 'discover', service: {running: false, ready: false}, job: {status: 'idle'}});
    if (pathname.endsWith('/hardware')) return json({devices: [], profiles: [], note: 'Controlled fixture'});
    if (pathname.endsWith('/generator/download')) return json({running: false, phase: 'idle'});
    const requestDocument = documents.get(pathname.split('/')[4]);
    const prefix = '/api/local-remove/session/' + requestDocument?.id;
    if (pathname === prefix) return json(requestDocument);
    if (pathname === prefix + '/stack/layers' && request.method === 'POST') {
      const payload = JSON.parse(body); if (payload.revision !== requestDocument.revision) return json({detail: 'Fixture revision conflict'}, 409);
      if (payload.kind !== 'retouch') return json({detail: 'Only retouch creation is a fixture operation'}, 400);
      requestDocument.layer_stack.push({id: 'retouch-created', kind: 'retouch', name: payload.name, visible: true, locked: false, opacity: 1, patch_ids: [], transform: {offset_x: 0, offset_y: 0, scale: 1, rotation: 0}, bounds: [0, 0, width, height]});
      requestDocument.revision++; return json(requestDocument);
    }
    if (pathname.startsWith(prefix + '/stack/layer/') && request.method === 'PATCH') {
      if (failNext) {const failure = failNext; failNext = null; return json(failure.body || {detail: 'Controlled failure ' + failure.status}, failure.status);}
      if (holdNext) {holdNext = false; await new Promise(resolve => {releaseHeld = resolve;}); releaseHeld = null;}
      const payload = JSON.parse(body); if (payload.revision !== requestDocument.revision) return json({detail: 'Fixture revision conflict'}, 409);
      const id = pathname.split('/').at(-1), layer = requestDocument.layer_stack.find(value => value.id === id);
      if (!layer) return json({detail: 'Unknown fixture layer'}, 404);
      for (const [key, value] of Object.entries(payload)) {
        if (key === 'revision') continue;
        if (key === 'index') {
          requestDocument.layer_stack.splice(requestDocument.layer_stack.indexOf(layer), 1);
          requestDocument.layer_stack.splice(Math.max(0, Math.min(requestDocument.layer_stack.length, value)), 0, layer);
        } else layer[key] = key === 'transform' ? {...layer.transform, ...value} : value;
      }
      requestDocument.revision++; return json(requestDocument);
    }
    if (pathname.startsWith(prefix + '/') && (pathname.endsWith('/display') || pathname.endsWith('/base-display') || pathname.endsWith('/preview'))) {
      const data = pathname.includes('/subject/') ? png.subject : pathname.includes('/cleanup/') ? png.empty : pathname.endsWith('/preview') ? png.composite : png.base;
      response.writeHead(200, {'Content-Type': 'image/png'}); response.end(data); return;
    }
    if (pathname === '/favicon.ico') {response.writeHead(204); response.end(); return;}
    unknown.push({method: request.method, path: pathname}); json({detail: 'Unmocked fixture route'}, 404);
  });
  await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
  const origin = 'http://127.0.0.1:' + server.address().port;
  // This route guard prevents accidental live traffic even if an adapter changes.
  await context.route('**/*', route => new URL(route.request().url()).origin === origin ? route.continue() : route.abort('blockedbyclient'));
  await context.addInitScript(() => {
    for (const name of ['local-image.hardware-guide.v1', 'local-image.first-ai-setup.v1', 'local-image.first-task.v1']) localStorage.setItem(name, '1');
    localStorage.setItem('local-remove-operation', 'heal');
    window.__migrationCsp = [];
    document.addEventListener('securitypolicyviolation', event => window.__migrationCsp.push({directive: event.violatedDirective, blocked: event.blockedURI}));
  });
  page.on('pageerror', error => pageErrors.push(error.message));
  const ready = async () => {
    await page.waitForFunction(() => settingsLoaded && initCompleted, null, {timeout: 20000});
    if (react) {
      await page.waitForFunction(() => document.body.dataset.reactReady === 'true');
      await page.locator('#react-layers-root [role="listbox"]').waitFor({state: 'attached'});
    }
  };
  const shot = async name => {await page.mouse.move(4, 4); await page.screenshot({path: path.join(out, name), animations: 'disabled'}); screenshots.push({file: name, viewport: page.viewportSize()});};
  try {
    for (const [w, h] of [[800, 560], [1366, 768], [1920, 1080]]) {
      await page.setViewportSize({width: w, height: h});
      const start = performance.now(); await page.goto(origin + '/remove'); await ready();
      metrics.push({viewport: `${w}x${h}`, emptyReadyMs: performance.now() - start});
      await shot(`empty-${w}x${h}.png`);
      cspViolations.push(...await page.evaluate(() => window.__migrationCsp));
      const openStart = performance.now(); await page.goto(origin + '/remove?session=' + document.id); await ready();
      await page.waitForFunction(() => session?.id === 'migration-fixture' && !busy && window.LocalImageLayers);
      await page.locator('#move-subject').click();
      metrics.at(-1).fixtureReadyMs = performance.now() - openStart;
      metrics.at(-1).layout = await page.evaluate(() => ({bodyWidth: document.body.scrollWidth, viewportWidth: innerWidth, canvas: document.querySelector('#viewport').getBoundingClientRect().toJSON(), rootFont: getComputedStyle(document.documentElement).fontFamily, studioSize: getComputedStyle(document.documentElement).getPropertyValue('--studio-size')}));
      await shot(`transformed-${w}x${h}.png`);
      assert.ok(metrics.at(-1).layout.canvas.width > 100 && metrics.at(-1).layout.canvas.height > 100, 'Persistent canvas must remain visible at ' + w + 'x' + h);
      cspViolations.push(...await page.evaluate(() => window.__migrationCsp));
    }
    const sessionId = await page.evaluate(() => session.id);
    assert.equal(sessionId, document.id); checks.push('All retained adapters execute in the actual composed document');
    const canvas = await page.locator('#photo').elementHandle();
    const layerRow = id => page.locator(react ? '#react-layers-root [data-layer-id="' + id + '"]' : '[data-stack-id="' + id + '"] .layer-content');
    await layerRow('original').click();
    await layerRow('subject').click();
    assert.equal(await canvas.evaluate(node => node === document.querySelector('#photo')), true); checks.push('Selecting layers retains the mounted photo/canvas subtree');
    if (react) {
      for (const id of ['layers', 'layer-opacity', 'layer-add', 'layer-actions', 'stack-layer-menu', 'retouch-panel']) assert.equal(await page.locator('#' + id).count(), 0, id + ' is removed, not hidden beneath React');
      const styleEvidence = await page.evaluate(() => [...document.querySelectorAll('style')].map(style => ({nonce: style.nonce, bucket: style.getAttribute('data-make-styles-bucket')})));
      assert.ok(styleEvidence.length > 2, 'Fluent/Griffel inserted runtime style elements');
      assert.ok(styleEvidence.every(style => style.nonce === 'migration-test-nonce'), 'Every inline runtime stylesheet carries this page nonce');
      const theme = await page.evaluate(() => {
        const panel = getComputedStyle(document.querySelector('.li-layers')), button = getComputedStyle(document.querySelector('.li-row-button'));
        return {font: panel.fontFamily, color: panel.color, background: panel.backgroundColor, buttonPadding: button.padding, buttonHeight: button.height};
      });
      metrics.push({reactTheme: theme});
      assert.match(theme.font, /Segoe|system-ui/);
      assert.notEqual(theme.color, 'rgb(0, 0, 0)', 'Layers theme resolves outside the provider DOM ancestry');
      assert.notEqual(theme.background, 'rgba(0, 0, 0, 0)');
      const inspectorToggle = page.locator('#react-shell-root').getByRole('button', {name: 'Inspector', exact: true});
      const visibleCanvasWidth = (await page.locator('#viewport').boundingBox()).width;
      await inspectorToggle.focus(); await page.keyboard.press('Space');
      await page.waitForFunction(() => LocalImageLegacyEditor.getSnapshot().inspectorHidden);
      assert.ok((await page.locator('#viewport').boundingBox()).width > visibleCanvasWidth + 100);
      await page.keyboard.press('Space'); await page.waitForFunction(() => !LocalImageLegacyEditor.getSnapshot().inspectorHidden);
      assert.equal(await inspectorToggle.evaluate(node => node === document.activeElement), true);
      await page.locator('#react-shell-root').getByRole('button', {name: 'Assets', exact: true}).click();
      await page.locator('#studio-assets').waitFor({state: 'visible'});
      await page.locator('#stock-close').click();
      assert.equal(await canvas.evaluate(node => node === document.querySelector('#photo')), true);
      await page.getByRole('button', {name: 'Layer commands', exact: true}).click();
      await page.getByRole('menuitem', {name: 'Rename layer', exact: true}).waitFor({state: 'visible'});
      await page.keyboard.press('Escape');
      assert.equal(await page.getByRole('button', {name: 'Layer commands', exact: true}).evaluate(node => node === document.activeElement), true, 'Fluent menu dismiss returns focus');
      checks.push('Migrated Layers controls have one owner; production module/CSS and nonced Fluent runtime styles load');
      const frozen = await page.evaluate(() => {
        const first = LocalImageLegacyEditor.getSnapshot();
        window.__migrationAccepted = first;
        return Object.isFrozen(first) && Object.isFrozen(first.document) && Object.isFrozen(first.document.layer_stack) && Object.isFrozen(first.document.layer_stack[0]);
      });
      assert.equal(frozen, true);
      await layerRow('subject').focus(); await page.keyboard.press('ArrowDown');
      assert.equal(await page.evaluate(() => LocalImageLegacyEditor.getSnapshot().selectedLayerId), 'cleanup');
      assert.equal(await layerRow('cleanup').evaluate(node => node === document.activeElement), true);
      await page.keyboard.press('Home'); await page.keyboard.press('Space');
      assert.equal(await page.evaluate(() => spaceHeld), false, 'Space on a layer row does not arm canvas panning');
      await page.locator('[data-tool="rectangle"]').click();
      const renameStart = requests.length;
      await layerRow('subject').focus(); await page.keyboard.press('F2');
      await page.getByRole('textbox', {name: 'Layer name', exact: true}).fill('Temporary draft');
      await page.keyboard.press('v');
      assert.equal(await page.evaluate(() => tool), 'rectangle', 'Typing v into a layer input cannot activate Move');
      await page.keyboard.press('Escape');
      assert.equal(requests.slice(renameStart).filter(item => item.method === 'PATCH').length, 0, 'Cancelled rename is only a UI draft');
      await layerRow('subject').focus(); await page.keyboard.press('F2');
      await page.getByRole('textbox', {name: 'Layer name', exact: true}).fill('Ceramic study renamed');
      await page.keyboard.press('Enter'); await page.waitForFunction(() => !busy && LocalImageLayers.selected().name === 'Ceramic study renamed');
      await page.getByRole('spinbutton', {name: 'Layer opacity percent', exact: true}).fill('73');
      await page.getByRole('spinbutton', {name: 'Layer opacity percent', exact: true}).press('Enter');
      await page.waitForFunction(() => !busy && LocalImageLayers.selected().opacity === .73);
      assert.equal(await page.evaluate(() => window.__migrationAccepted.document.layer_stack.find(layer => layer.id === 'subject').opacity), .92, 'A previous immutable snapshot retains its accepted values');
      checks.push('Keyboard row navigation, focus, Space/input shortcut exclusions, rename cancellation, rename and Enter opacity commit');
      await page.getByRole('spinbutton', {name: 'Layer opacity percent', exact: true}).fill('0');
      await page.getByRole('spinbutton', {name: 'Layer opacity percent', exact: true}).press('Enter');
      await page.waitForFunction(() => !busy && LocalImageLayers.selected().opacity === 0);
      await page.getByRole('spinbutton', {name: 'Layer opacity percent', exact: true}).fill('73');
      await page.getByRole('spinbutton', {name: 'Layer opacity percent', exact: true}).press('Enter');
      await page.waitForFunction(() => !busy && LocalImageLayers.selected().opacity === .73);
      await page.getByRole('button', {name: 'Hide Ceramic study renamed', exact: true}).click();
      await page.waitForFunction(() => !busy && LocalImageLayers.selected().visible === false);
      await page.getByRole('button', {name: 'Show Ceramic study renamed', exact: true}).click();
      await page.waitForFunction(() => !busy && LocalImageLayers.selected().visible === true);
      await page.getByRole('button', {name: 'Lock Ceramic study renamed', exact: true}).click();
      await page.waitForFunction(() => !busy && LocalImageLayers.selected().locked);
      assert.equal(await page.getByRole('spinbutton', {name: 'Layer opacity percent', exact: true}).isDisabled(), true);
      await page.getByRole('button', {name: 'Unlock Ceramic study renamed', exact: true}).click();
      await page.waitForFunction(() => !busy && !LocalImageLayers.selected().locked);
      await page.getByRole('button', {name: 'New retouch layer', exact: true}).click();
      await page.waitForFunction(() => !busy && LocalImageLayers.selected().id === 'retouch-created');
      assert.equal(document.layer_stack.at(-1).id, 'retouch-created');
      for (const [command, expected] of [['Move layer down', 2], ['Move layer up', 3]]) {
        await page.getByRole('button', {name: 'Layer commands', exact: true}).click();
        await page.getByRole('menuitem', {name: command, exact: true}).click();
        await page.waitForFunction(index => !busy && session.layer_stack.findIndex(layer => layer.id === 'retouch-created') === index, expected);
        assert.equal(document.layer_stack.findIndex(layer => layer.id === 'retouch-created'), expected);
      }
      await layerRow('subject').click();
      assert.equal(document.layer_stack.find(layer => layer.id === 'subject').cutout.shadow.opacity, .3, 'Narrow layer metadata changes preserve the existing mask/shadow fields');
      checks.push('Zero opacity, visibility, lock, retouch creation and menu reorder reach token/revision-checked fixture API and accepted snapshots');
      failNext = {status: 409};
      const opacityFailureStart = requests.length;
      await page.getByRole('spinbutton', {name: 'Layer opacity percent', exact: true}).fill('55');
      await page.getByRole('spinbutton', {name: 'Layer opacity percent', exact: true}).press('Enter');
      await page.waitForFunction(() => !busy && LocalImageLegacyEditor.getSnapshot().status.includes('Controlled failure 409'));
      await page.waitForFunction(() => document.querySelector('[aria-label="Layer opacity percent"]').value === '73');
      assert.equal(await page.evaluate(() => LocalImageLayers.selected().opacity), .73);
      assert.equal(requests.slice(opacityFailureStart).filter(item => item.method === 'PATCH').length, 1);
      checks.push('A rejected opacity draft resets to the accepted value even when document revision is unchanged');
      for (const status of [409, 500]) {
        const beforeFailure = clone(document), start = requests.length;
        failNext = {status};
        await page.getByRole('button', {name: 'Hide Ceramic study renamed', exact: true}).click();
        await page.waitForFunction(code => !busy && LocalImageLegacyEditor.getSnapshot().status.includes('Controlled failure ' + code), status);
        assert.deepEqual(document, beforeFailure, 'Fixture persisted state remains unchanged after rejected mutation');
        assert.equal(await page.evaluate(() => LocalImageLayers.selected().visible), true);
        assert.equal(requests.slice(start).filter(item => item.method === 'PATCH').length, 1, 'Failed writes are never retried automatically');
      }
      checks.push('409/500 mutations preserve accepted layer state and dispatch once, without automatic retries');
      await page.locator('#move-subject').click();
    }
    const bounds = await page.locator('#photo').boundingBox();
    const before = document.revision, mutationStart = requests.length;
    const moveStart = performance.now();
    await page.mouse.move(bounds.x + bounds.width * .5, bounds.y + bounds.height * .55); await page.mouse.down();
    assert.equal(await page.evaluate(() => document.querySelector('#viewport').hasPointerCapture(1)), true, 'Persistent viewport owns pointer capture');
    await page.mouse.move(bounds.x + bounds.width * .55, bounds.y + bounds.height * .59, {steps: 20});
    assert.equal(document.revision, before, 'Pointer movement does not persist each intermediate transform');
    await page.mouse.up(); await page.waitForFunction(revision => session.revision > revision && !busy, before);
    const mutations = requests.slice(mutationStart).filter(item => item.method === 'PATCH');
    assert.equal(mutations.length, 1, 'One transform is committed at pointer release');
    metrics.push({gesture: '4096x2732 layer drag, 20 Playwright moves', wallMs: performance.now() - moveStart, commits: mutations.length, caveat: 'Includes driver dispatch and fixture HTTP; not production input latency'});
    checks.push('Real pointer gesture commits exactly once at pointer release');
    if (react) {
      const selectedBefore = clone(document), requestStart = requests.length;
      await page.locator('[data-tool="rectangle"]').click();
      await page.mouse.move(bounds.x + bounds.width * .3, bounds.y + bounds.height * .3); await page.mouse.down();
      await page.mouse.move(bounds.x + bounds.width * .4, bounds.y + bounds.height * .4, {steps: 5}); await page.mouse.up();
      await page.waitForFunction(() => hasSelection && !busy);
      assert.equal(await page.evaluate(() => historyTarget()), 'selection');
      await page.locator('#react-shell-root').getByRole('button', {name: 'Undo', exact: true}).click();
      await page.waitForFunction(() => !hasSelection);
      assert.equal(await page.evaluate(() => historyTarget(true)), 'selection');
      await page.locator('#react-shell-root').getByRole('button', {name: 'Redo', exact: true}).click();
      await page.waitForFunction(() => hasSelection);
      assert.deepEqual(document, selectedBefore);
      assert.equal(requests.slice(requestStart).filter(item => item.method !== 'GET').length, 0, 'Selection history takes priority over backend stack undo');
      checks.push('React Undo/Redo preserve browser selection priority without mutating backend stack state');
      await page.evaluate(() => clearSelection());
      holdNext = true;
      await page.getByRole('button', {name: 'Hide Ceramic study renamed', exact: true}).click();
      await page.waitForFunction(() => busy);
      for (let i = 0; i < 20 && !releaseHeld; i++) await new Promise(resolve => setTimeout(resolve, 10));
      assert.ok(releaseHeld, 'Fixture write is deliberately held');
      await page.evaluate(async value => {await openSession(value);}, clone(otherDocument));
      const expectedPreviousRevision = document.revision + 1;
      const release = releaseHeld; release();
      await page.waitForFunction(revision => !busy && session.id === 'migration-other' && openDocuments.get('migration-fixture')?.revision === revision, expectedPreviousRevision);
      assert.equal(await page.evaluate(() => LocalImageLegacyEditor.getSnapshot().document.id), 'migration-other');
      assert.equal(await page.evaluate(() => LocalImageLegacyEditor.getSnapshot().document.revision), 0);
      checks.push('A delayed write from the previous document cannot replace the active document after navigation');
      assert.equal(await canvas.evaluate(node => node === document.querySelector('#photo')), true);
      cspViolations.push(...await page.evaluate(() => window.__migrationCsp));
    }
    const cdp = await context.newCDPSession(page); await cdp.send('Performance.enable');
    const measured = await cdp.send('Performance.getMetrics');
    metrics.push({browserMetrics: Object.fromEntries(measured.metrics.filter(value => ['JSHeapUsedSize', 'JSHeapTotalSize', 'Nodes', 'Documents', 'LayoutCount', 'RecalcStyleCount'].includes(value.name)).map(value => [value.name, value.value])), caveat: 'Renderer JS heap excludes native image/GPU/process memory'});
    if (react) {
      document = documentFixture(); documents.set(document.id, document);
      await page.setViewportSize({width: 1366, height: 768});
      await page.goto(origin + '/remove?session=' + document.id); await ready();
      await page.waitForFunction(() => session?.id === 'migration-fixture' && !busy);
      const fontBefore = await page.locator('.li-layer-name').first().evaluate(node => parseFloat(getComputedStyle(node).fontSize));
      await page.evaluate(() => applyInterfaceDensity('large'));
      await page.waitForFunction(() => document.documentElement.dataset.uiDensity === 'large');
      await shot('large-text-200-1366x768.png');
      const density = await page.locator('.li-layer-name').first().evaluate((node, before) => ({before, after: parseFloat(getComputedStyle(node).fontSize), body: getComputedStyle(document.body).fontSize, canvas: document.querySelector('#viewport').getBoundingClientRect().toJSON(), bodyWidth: document.body.scrollWidth}), fontBefore);
      metrics.push({largeText: density, caveat: 'Existing app Large · 200% text preference in Edge, not Windows accessibility settings'});
      assert.ok(density.after >= density.before * 1.9, 'Large preference must scale migrated Layers text to 200%');
      assert.ok(density.canvas.height > 100 && density.canvas.width > 100, 'Large text retains a visible canvas');
      await page.evaluate(() => applyInterfaceDensity('comfortable'));
      const scaledContext = await browser.newContext({viewport: {width: 1366, height: 768}, deviceScaleFactor: 2});
      await scaledContext.route('**/*', route => new URL(route.request().url()).origin === origin ? route.continue() : route.abort('blockedbyclient'));
      await scaledContext.addInitScript(() => {
        for (const name of ['local-image.hardware-guide.v1', 'local-image.first-ai-setup.v1', 'local-image.first-task.v1']) localStorage.setItem(name, '1');
        localStorage.setItem('local-remove-operation', 'heal');
        window.__migrationCsp = []; document.addEventListener('securitypolicyviolation', event => window.__migrationCsp.push({directive: event.violatedDirective, blocked: event.blockedURI}));
      });
      const scaledPage = await scaledContext.newPage(); scaledPage.on('pageerror', error => pageErrors.push(error.message));
      await scaledPage.goto(origin + '/remove?session=' + document.id);
      await scaledPage.waitForFunction(() => settingsLoaded && initCompleted && document.body.dataset.reactReady === 'true' && session?.id === 'migration-fixture' && !busy);
      await scaledPage.locator('#react-layers-root [role="option"]').first().waitFor({state: 'visible'});
      await scaledPage.locator('#move-subject').click();
      await scaledPage.screenshot({path: path.join(out, 'device-scale-2-1366x768.png'), animations: 'disabled'});
      screenshots.push({file: 'device-scale-2-1366x768.png', viewport: {width: 1366, height: 768}, deviceScaleFactor: 2});
      metrics.push({deviceScaleFactor: await scaledPage.evaluate(() => devicePixelRatio), caveat: 'Chromium DPR2 emulation only; not packaged WebView2 or Windows OS DPI certification'});
      assert.equal(await scaledPage.evaluate(() => devicePixelRatio), 2);
      cspViolations.push(...await scaledPage.evaluate(() => window.__migrationCsp));
      await scaledContext.close();
      checks.push('Existing Large · 200% text preference scales React Layers; DPR2 capture retains the mounted browser editor');
      cspViolations.push(...await page.evaluate(() => window.__migrationCsp));
    }
    assert.deepEqual(pageErrors, [], 'No page script errors');
    assert.deepEqual(unknown, [], 'No unmocked API routes');
    assert.deepEqual(cspViolations, [], 'No CSP violations');
    assert.equal(sourceFingerprint(), beforeFingerprint, 'Captured source/build files must not change during this run');
    passed = true;
  } catch (error) {
    failure = {name: error.name, message: error.message};
    throw error;
  } finally {
    const report = {passed, failure, startedAt, completedAt: new Date().toISOString(), mode: baseline ? 'pinned legacy baseline' : rollback ? 'working source legacy rollback' : 'working source React',
      baseRevision, pinnedSourceRevision: baseline ? revision : null, sourceState: baseline ? 'Commit snapshot, independent of the dirty working tree' : workingTreeStatus ? 'Dirty working tree; baseRevision is not the tested source identity' : 'Clean working tree',
      workingTreeStatus, sourceInventory, sourceUnchangedDuringRun: sourceFingerprint() === beforeFingerprint, browser: browser.version(), node: process.version,
      evidence: 'Actual Edge browser plus Python-composed page; HTTP and preview images are deterministic fixtures. No Python API/image engine, native WebView2, save/export, GPU or packaged acceptance claim.', checks,
      screenshots: screenshots.map(item => ({...item, ...hashFile(path.join(out, item.file), item.file)})), metrics, pageErrors, cspViolations, unknown, requests};
    fs.writeFileSync(path.join(out, 'results.json'), JSON.stringify(report, null, 2));
    await browser.close(); await new Promise(resolve => server.close(resolve));
  }
  console.log(`PASS migration fixture browser (${baseline ? 'baseline' : rollback ? 'legacy rollback' : 'React'}): ${screenshots.length} screenshots, ${checks.length} checks; ${out}`);
}
main().catch(error => {console.error(error); process.exitCode = 1;});
