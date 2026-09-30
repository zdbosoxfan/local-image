// First successful task and scalable controls on an isolated app profile.
const {chromium}=require('playwright'),assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path');
const base=process.env.LOCAL_REMOVE_TEST_URL||'http://127.0.0.1:51259';
const output=path.resolve(process.argv[2]||'qa-artifacts/v07/usability');fs.mkdirSync(output,{recursive:true});
(async()=>{
 const browser=await chromium.launch({headless:true,channel:process.env.BROWSER_CHANNEL||'msedge'});
 const context=await browser.newContext({viewport:{width:1280,height:800}}),page=await context.newPage(),errors=[];
 page.on('pageerror',error=>errors.push(error.message));
 await page.route('**/api/local-remove/hardware',route=>route.fulfill({json:{devices:[{name:'Test 12 GB GPU',vram_gb:12}],system_ram_gb:32,profiles:[{label:'Quick Heal',vram:'No GPU'},{label:'Fast generation',vram:'16 GB recommended'}],note:'Planning recommendations, not hard minimums.'}}));
 try{
  await page.goto(base+'/remove');await page.locator('#hardware-dialog').waitFor({state:'visible'});
  await page.waitForFunction(()=>document.querySelector('#hardware-start-note').textContent.includes('system-RAM offloading'));
  assert.equal(await page.locator('.hardware-starter button').count(),4);
  assert.equal(await page.locator('#hardware-recommendations').getAttribute('open'),null);
  await page.screenshot({path:path.join(output,'first-task.png')});
  await page.locator('#starter-generate').click();assert.equal(await page.locator('#hardware-dialog').isVisible(),false);
  assert.equal(await page.locator('#workspace-generate').getAttribute('aria-pressed'),'true');
  await page.locator('#edit-menu-trigger').click();await page.locator('#settings').click();
  await page.locator('#interface-density').selectOption('comfortable');
  assert.equal(await page.evaluate(()=>getComputedStyle(document.body).fontSize),'14px');
  await page.locator('#interface-density').selectOption('large');
  assert.equal(await page.evaluate(()=>getComputedStyle(document.body).fontSize),'24px');
  const setupFonts=await page.locator('#settings-dialog p,#settings-dialog summary').evaluateAll(elements=>elements.filter(el=>el.getClientRects().length).map(el=>parseFloat(getComputedStyle(el).fontSize)));
  assert(setupFonts.length&&setupFonts.every(size=>size>=22),'Explanations scale with controls at 200%');
  await page.screenshot({path:path.join(output,'large-text-settings.png')});
  await page.keyboard.press('Escape');await page.setViewportSize({width:800,height:700});
  await page.locator('#workspace-cutout').click();
  await page.locator('#qwen-variant').scrollIntoViewIfNeeded();
  const control=await page.locator('#qwen-variant').boundingBox();
  assert(control&&control.width>100&&control.height>30,'Scaled model choice remains reachable');
  await page.screenshot({path:path.join(output,'large-text-cutout-800.png')});
  assert.equal(await page.locator('#tool-hint').isVisible(),true);
  await page.reload();await page.waitForFunction(()=>settingsLoaded);
  assert.equal(await page.evaluate(()=>document.documentElement.dataset.uiDensity),'large');
  assert.equal(await page.locator('#hardware-dialog').isVisible(),false,'A selected starter task completes first guidance');
  assert.deepEqual(errors,[]);
  fs.writeFileSync(path.join(output,'results.json'),JSON.stringify({passed:true,starter_choices:4,small_gpu_explained:true,density_persists:true,large_text:'200%',screenshots:['first-task.png','large-text-settings.png','large-text-cutout-800.png']},null,2));
  console.log('PASS task-based guidance, small-GPU explanation, persisted interface sizes, 200% text and narrow tool hint.');
 }catch(error){await page.screenshot({path:path.join(output,'failure.png')});throw error;}finally{await browser.close();}
})().catch(error=>{console.error(error);process.exitCode=1;});
