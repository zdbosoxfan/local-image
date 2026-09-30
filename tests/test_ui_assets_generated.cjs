// Assets Generated uses original library selection/cache actions in a nonmodal
// dock. API fixtures isolate this UI check from the user's real image cache.
const {chromium}=require('playwright');
const assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path');
const base=process.env.LOCAL_REMOVE_TEST_URL||'http://127.0.0.1:51274';
const out=path.resolve(process.argv[2]||'qa-artifacts/ui-redesign/assets-generated-test');fs.mkdirSync(out,{recursive:true});
let items=Array.from({length:6},(_,i)=>({id:'generated-'+i,name:'Lighthouse '+(i+1),thumbnail:'/generated-preview.png',width:1024,height:1024,model:'qwen',bytes:1024,generation:{prompt:'A lighthouse by the sea'}}));
const inventory=()=>({items,count:items.length,bytes:items.length*1024});
(async()=>{
 const browser=await chromium.launch({headless:true,channel:'msedge'}),page=await browser.newPage({viewport:{width:1440,height:900}}),errors=[],deletes=[];
 page.on('pageerror',e=>errors.push(e.stack||e.message));
 await page.addInitScript(()=>{localStorage.setItem('local-image.hardware-guide.v1','1');localStorage.setItem('local-image.first-ai-setup.v1','1');});
 await page.route('**/api/local-remove/stock/providers',route=>route.fulfill({json:{providers:[{id:'openverse',label:'Openverse',available:true}],default_provider:'openverse'}}));
 await page.route('**/api/local-remove/generation/library',route=>route.fulfill({json:inventory()}));
 await page.route('**/generated-preview.png',route=>route.fulfill({contentType:'image/png',body:fs.readFileSync('docs/images/examples/klein-teal-lighthouse.png')}));
 await page.route('**/api/local-remove/generation/library/delete',route=>{const payload=route.request().postDataJSON();deletes.push(payload);const deleted=items.filter(item=>payload.all||payload.ids.includes(item.id)).map(item=>item.id);items=items.filter(item=>!deleted.includes(item.id));return route.fulfill({json:{...inventory(),deleted,freed_bytes:deleted.length*1024}});});
 try{
  await page.goto(base+'/remove');await page.waitForFunction(()=>settingsLoaded&&!stockLoading);
  assert.equal(await page.locator('#stock-query').getAttribute('placeholder'),'Search photos');
  await page.locator('#studio-assets-generated').click();await page.waitForFunction(()=>!generatedLibraryBusy&&generatedLibrary.count===6);
  const library=page.locator('#generated-library-dialog');
  assert.equal(await library.evaluate(node=>node.parentElement.id),'studio-assets');
  assert.equal(await library.evaluate(node=>node.matches(':modal')),false);assert.equal(await page.evaluate(()=>modalOpen()),false);
  assert.equal(await page.locator('#studio-assets-generated').getAttribute('aria-pressed'),'true');
  assert.equal(await page.locator('#stock-dialog').isVisible(),false);assert.equal(await page.locator('#generated-library-grid .generated-library-item').count(),6);
  assert.match(await page.locator('#generated-library-usage').innerText(),/6 images.*6.*KB/i);
  await page.locator('.generated-library-item').first().click();assert.equal(await page.locator('#generated-library-edit').isEnabled(),true);
  const focused=await page.evaluate(()=>generatedLibraryFocused);
  await page.screenshot({path:path.join(out,'generated-docked.png'),animations:'disabled'});
  await page.locator('#stock-expand').click();assert.equal(await library.getAttribute('data-docked'),'false');assert.equal(await library.evaluate(node=>node.parentElement.tagName),'BODY');
  assert.equal(await page.evaluate(()=>modalOpen()),true);assert.equal(await page.locator('#stock-expand').getAttribute('aria-expanded'),'true');
  await page.screenshot({path:path.join(out,'generated-expanded.png'),animations:'disabled'});
  await page.keyboard.press('Escape');assert.equal(await library.getAttribute('data-docked'),'true');assert.equal(await library.evaluate(node=>node.parentElement.id),'studio-assets');
  assert.equal(await page.evaluate(()=>generatedLibraryFocused),focused,'Expanded browsing retains selection');
  await page.locator('#studio-assets-stock').click();assert.equal(await library.isVisible(),false);assert.equal(await page.locator('#stock-dialog').isVisible(),true);
  await page.locator('#studio-assets-generated').click();assert.equal(await library.isVisible(),true);assert.equal(await page.evaluate(()=>generatedLibraryFocused),focused,'Tab switch retains selection');
  await page.locator('#generated-library-search').fill('Lighthouse 2');assert.equal(await page.locator('.generated-library-item').count(),1);
  await page.locator('#generated-library-select-mode').click();await page.locator('.generated-library-item').click();await page.locator('#generated-library-delete').click();assert.equal(await page.locator('#generated-library-confirm').isVisible(),true);
  await page.locator('#generated-library-cancel-delete').click();assert.equal(deletes.length,0,'Deletion requires explicit confirmation');
  await page.locator('#generated-library-delete').click();await page.locator('#generated-library-confirm-delete').click();await page.waitForFunction(()=>!generatedLibraryBusy&&generatedLibrary.count===5);assert.deepEqual(deletes[0],{ids:['generated-1']});
  await page.locator('#generated-library-search').fill('');await page.locator('#generated-library-clear').click();await page.locator('#generated-library-confirm-delete').click();await page.waitForFunction(()=>!generatedLibraryBusy&&generatedLibrary.count===0);assert.deepEqual(deletes[1],{all:true});
  assert.match(await page.locator('#generated-library-usage').innerText(),/0 images/);assert.equal(await page.locator('#generated-library-empty').isVisible(),true);
  assert.deepEqual(errors,[]);fs.writeFileSync(path.join(out,'report.json'),JSON.stringify({passed:true,dockedNonmodal:true,expandedExplicit:true,selectionPreserved:true,cacheSelectionAndClearConfirmed:true},null,2));
  console.log('Assets Generated dock, explicit expansion, tab selection preservation, search, selective deletion, clear cache: passed.');
 }catch(error){await page.screenshot({path:path.join(out,'failure.png'),animations:'disabled'}).catch(()=>{});console.error(errors);throw error;}finally{await browser.close();}
})().catch(error=>{console.error(error);process.exitCode=1;});
