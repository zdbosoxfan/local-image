// Actual production UI typography/layout checks. All backend requests are GET;
// no models, provider queries, native dialogs or backend mutations are used.
const assert = require('node:assert/strict'), fs = require('node:fs'), path = require('node:path'), crypto = require('node:crypto');
const { chromium } = require('playwright');
const root = path.resolve(__dirname, '..'), base = process.env.LOCAL_REMOVE_TEST_URL, profile = process.env.MIGRATION_REAL_PROFILE;
assert.ok(base && profile); assert.equal(new URL(base).hostname, '127.0.0.1');
assert.ok(path.resolve(profile).startsWith(path.join(root, 'qa-artifacts') + path.sep));
const output = path.join(root, 'qa-artifacts/migration/large-controls'), hash = bytes => crypto.createHash('sha256').update(bytes).digest('hex');
const files = ['frontend/src/styles.css', 'frontend/src/features/batch/batch.css', 'frontend/src/features/settings/settings.css', 'frontend/src/features/models/models.css', 'backend/frontend_dist/.vite/manifest.json'];
const sourceHashes = () => files.map(name => ({ path: name, sha256: hash(fs.readFileSync(path.join(root, name))) }));
(async () => {
  const runtime = await (await fetch(base + '/api/local-remove/runtime')).json(); assert.equal(path.resolve(runtime.data_root), path.resolve(profile));
  fs.mkdirSync(output, { recursive: true });
  if (fs.existsSync(path.join(output, 'results.json'))) { const archive = path.join(output, 'report-history'); fs.mkdirSync(archive, { recursive: true }); fs.copyFileSync(path.join(output, 'results.json'), path.join(archive, `results-${Date.now()}.json`)); }
  const sources = sourceHashes(), browser = await chromium.launch({ channel: 'msedge', headless: true }), context = await browser.newContext({ viewport: { width: 1366, height: 768 } }), page = await context.newPage();
  const metrics = [], assets = [], errors = [], blocked = [], requests = [], screenshots = [], reads = []; let passed = false, failure = null, htmlHash = null;
  await context.route('**/*', route => { const request = route.request(), url = new URL(request.url()); if (url.origin !== base || request.method() !== 'GET' || /\/(stock|loras)\/(search|files|preview)/.test(url.pathname)) { blocked.push({ method: request.method(), path: url.pathname }); return route.abort('blockedbyclient'); } return route.continue(); });
  await context.addInitScript(() => { for (const key of ['local-image.hardware-guide.v1', 'local-image.first-ai-setup.v1', 'local-image.first-task.v1']) localStorage.setItem(key, '1'); window.__densityCsp = []; document.addEventListener('securitypolicyviolation', event => window.__densityCsp.push(event.violatedDirective)); });
  page.on('pageerror', error => errors.push(error.stack)); page.on('request', request => { const url = new URL(request.url()); if (url.pathname.startsWith('/api/')) requests.push({ method: request.method(), path: url.pathname }); });
  page.on('response', response => { const url = new URL(response.url()); if (url.pathname.startsWith('/frontend-assets/')) reads.push(response.body().then(value => assets.push({ path: url.pathname, status: response.status(), sha256: hash(value) }))); });
  const settle = () => page.evaluate(async () => { await Promise.all(document.getAnimations().filter(value => value.effect?.getComputedTiming().iterations !== Infinity).map(value => value.finished.catch(() => {}))); await new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))); });
  async function inspect(name, locator) {
    await settle();
    const result = await locator.evaluate(root => {
      const number = value => parseFloat(value) || 0;
      const controls = [...root.querySelectorAll('.fui-Select__select,.fui-Input__input,.fui-Button')].filter(element => element.getClientRects().length && !element.closest('[hidden]')).map(element => {
        const style = getComputedStyle(element), rect = element.getBoundingClientRect(), font = number(style.fontSize), line = number(style.lineHeight) || font * 1.2;
        const padding = number(style.paddingTop) + number(style.paddingBottom), ranges = [];
        if (element instanceof HTMLButtonElement) {
          const walker = document.createTreeWalker(element, NodeFilter.SHOW_TEXT); let node;
          while ((node = walker.nextNode())) if (node.textContent.trim()) { const range = document.createRange(); range.selectNodeContents(node); for (const bounds of range.getClientRects()) ranges.push({ top: bounds.top, bottom: bounds.bottom }); }
        }
        const label = element.getAttribute('aria-label') || (element.id && [...document.querySelectorAll('label')].find(label => label.htmlFor === element.id)?.textContent) || element.textContent.trim().slice(0, 80);
        return { tag: element.tagName, label, font, line, clientHeight: element.clientHeight, padding, contentHeight: element.clientHeight - padding, rect: { top: rect.top, bottom: rect.bottom, width: rect.width, height: rect.height }, ranges };
      });
      const headers = [...root.querySelectorAll('.li-batch-table th')].map(cell => { const bounds = cell.getBoundingClientRect(), range = document.createRange(); range.selectNodeContents(cell); const text = range.getBoundingClientRect(); return { text: cell.textContent, cell: { left: bounds.left, right: bounds.right, top: bounds.top, bottom: bounds.bottom }, content: { left: text.left, right: text.right, top: text.top, bottom: text.bottom } }; });
      const rect = root.getBoundingClientRect();
      const surface = root.matches('.fui-DialogSurface') ? { width: rect.width, left: rect.left, top: rect.top, right: rect.right, bottom: rect.bottom, viewportWidth: innerWidth, viewportHeight: innerHeight, clientWidth: root.clientWidth, scrollWidth: root.scrollWidth } : null;
      return { density: document.documentElement.dataset.uiDensity, controls, headers, surface };
    });
    metrics.push({ name, ...result }); assert.equal(result.density, 'large'); assert.ok(result.controls.length > 0, name + ' has actual controls');
    for (const control of result.controls) {
      assert.ok(control.font >= 24, `${name}: ${control.label} retains Large text`);
      assert.ok(control.contentHeight + 1 >= control.line, `${name}: ${control.tag} ${control.label} content ${control.contentHeight}px must fit line ${control.line}px`);
      for (const range of control.ranges) assert.ok(range.top >= control.rect.top - 1 && range.bottom <= control.rect.bottom + 1, `${name}: button text stays within its own control`);
    }
    for (const header of result.headers) assert.ok(header.content.left >= header.cell.left - 1 && header.content.right <= header.cell.right + 1 && header.content.top >= header.cell.top - 1 && header.content.bottom <= header.cell.bottom + 1, `${name}: ${header.text} text stays inside its own header cell`);
    if (result.surface) {
      const surface = result.surface;
      assert.ok(surface.left >= 0 && surface.top >= 0 && surface.right <= surface.viewportWidth + 1 && surface.bottom <= surface.viewportHeight + 1, `${name}: dialog remains inside the viewport`);
      assert.ok(surface.scrollWidth <= surface.clientWidth + 1, `${name}: responsive dialog has no horizontal content overflow`);
      if (surface.viewportWidth === 1366 && name.startsWith('settings')) assert.ok(Math.abs(surface.width - 900) < 1, 'Settings uses its intended Large width');
      if (surface.viewportWidth === 1366 && name === 'models-large') assert.ok(Math.abs(surface.width - 1100) < 1, 'Models uses its intended Large width');
    }
    const file = path.join(output, name + '.png'); await page.screenshot({ path: file, animations: 'disabled' }); screenshots.push(file);
  }
  try {
    const response = await page.goto(base + '/remove'); htmlHash = hash(await response.body()); await page.waitForFunction(() => document.body.dataset.reactReady === 'true');
    await page.getByRole('button', { name: 'Settings', exact: true }).click(); const settings = page.getByRole('dialog', { name: 'Settings', exact: true }); await settings.waitFor();
    await settings.getByRole('combobox', { name: 'Interface size', exact: true }).selectOption('large'); await inspect('settings-large', settings);
    await settings.getByRole('tab', { name: 'Local AI', exact: true }).click(); await page.waitForFunction(() => !window.LocalImageReactFeatures.settings.getSnapshot().loading); await inspect('settings-ai-large', settings);
    await settings.getByRole('button', { name: 'Model details…', exact: true }).click(); const models = page.getByRole('dialog', { name: 'Local image models', exact: true }); await models.waitFor(); await page.waitForFunction(() => !window.LocalImageReactFeatures.models.getSnapshot().loading); await inspect('models-large', models);
    await models.getByRole('button', { name: 'Use this model', exact: true }).click(); const generation = page.getByRole('region', { name: 'Image generation', exact: true }); await inspect('generation-large', generation);
    await page.locator('#workspace-cutout').click(); await page.waitForFunction(() => !window.LocalImageEditor.getSnapshot().busy); await page.getByRole('button', { name: 'Remove backgrounds', exact: true }).click(); const batch = page.getByRole('dialog', { name: 'Remove backgrounds and export PNGs', exact: true }); await batch.waitFor(); await page.waitForFunction(() => !window.LocalImageReactFeatures.batch.getSnapshot().working);
    await inspect('batch-1366-large', batch); await page.setViewportSize({ width: 800, height: 560 }); await inspect('batch-800-large', batch);
    await Promise.all(reads); assert.deepEqual(sourceHashes(), sources); assert.deepEqual(errors, []); assert.deepEqual(blocked, []); assert.deepEqual(await page.evaluate(() => window.__densityCsp), []); passed = true;
  } catch (error) { failure = error.stack; await page.screenshot({ path: path.join(output, 'failure.png'), animations: 'disabled' }).catch(() => {}); throw error; }
  finally { await Promise.allSettled(reads); fs.writeFileSync(path.join(output, 'results.json'), JSON.stringify({ passed, failure, runtime, htmlHash, sources, assets, metrics, requests, errors, blocked, screenshots, evidence: 'Actual production React controls with Large font tokens; computed content/line heights and header text bounds; read-only backend requests' }, null, 2)); await browser.close(); }
  console.log(`PASS Large controls: ${metrics.length} surfaces; ${output}`);
})().catch(error => { console.error(error); process.exitCode = 1; });
