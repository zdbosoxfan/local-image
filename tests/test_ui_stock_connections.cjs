// Provider connection UX uses synthetic credentials and API responses only.
const {chromium}=require('playwright');
const assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path');
const base=process.env.LOCAL_REMOVE_TEST_URL||'http://127.0.0.1:51274';
const output=path.resolve('qa-artifacts/ui-redesign/stock-connections');fs.mkdirSync(output,{recursive:true});
(async()=>{
 const browser=await chromium.launch({headless:true,channel:'msedge'}),page=await browser.newPage({viewport:{width:1440,height:960}}),errors=[];
 page.on('pageerror',error=>errors.push(error.message));
 await page.addInitScript(()=>{localStorage.setItem('local-image.hardware-guide.v1','1');localStorage.setItem('local-image.first-ai-setup.v1','1');});
 const providers=[{id:'openverse',label:'Openverse',available:true},{id:'pexels',label:'Pexels',available:false,needs_key:true,connect_url:'https://www.pexels.com/api/'},{id:'unsplash',label:'Unsplash',available:false,needs_key:true,connect_url:'https://unsplash.com/developers'}];
 await page.route('**/stock/providers',route=>route.fulfill({json:{providers,default_provider:'openverse'}}));
 const writes=[];await page.route('**/stock/connection/*',route=>{const provider=providers.find(item=>route.request().url().endsWith('/'+item.id));const body=route.request().postDataJSON();writes.push(body);provider.available=!!body.key;return route.fulfill({json:{provider:provider.id,connected:provider.available}});});
 await page.route('**/stock/search?**',route=>{const provider=new URL(route.request().url()).searchParams.get('provider');return route.fulfill({json:{results:[{id:'a'.repeat(32),provider,title:'Mountain study',creator:'Test Photographer',creator_url:'https://example.com/photographer',source_url:'https://example.com/photo',license:provider+' license',license_url:'https://example.com/license',attribution:'Photo by Test Photographer',width:1024,height:768,thumbnail_url:'/fixture-stock.png'}],next_page:null}});});
 await page.route('**/fixture-stock.png',route=>route.fulfill({contentType:'image/png',body:fs.readFileSync('docs/images/examples/z-image-realism.png')}));
 try{
  await page.goto(base+'/remove');await page.waitForFunction(()=>settingsLoaded&&stockProviders.length===3&&!stockLoading);
  assert.deepEqual(await page.locator('#stock-provider option').allTextContents(),['Openverse','Pexels','Unsplash']);
  assert.equal(await page.locator('#studio-generated-open').count(),0);
  await page.locator('#stock-provider').selectOption('pexels');
  assert.equal(await page.locator('#stock-connection').isVisible(),true);assert.equal(await page.locator('#stock-search').isDisabled(),true);
  await page.locator('#stock-api-key').fill('synthetic-test-key');await page.locator('#stock-connect').click();
  await page.waitForFunction(()=>stockProviders.find(p=>p.id==='pexels').available);
  assert.equal(await page.locator('#stock-api-key').inputValue(),'');assert.equal(writes[0].key,'synthetic-test-key');
  await page.locator('#stock-query').fill('mountains');await page.locator('#stock-search').click();await page.waitForFunction(()=>!stockLoading&&stockResults.length===1);
  assert.equal(await page.locator('.stock-photo-credit').textContent(),'Test Photographer');
  await page.locator('.stock-result').click();assert.equal(await page.locator('#stock-import-image').isEnabled(),true);
  await page.screenshot({path:path.join(output,'pexels-dropdown.png')});
  await page.locator('#stock-provider').selectOption('unsplash');
  assert.equal(await page.locator('#stock-connection').isVisible(),true);assert.equal(await page.locator('.stock-result').count(),0);
  assert.equal(await page.locator('#stock-search').isDisabled(),true);
  await page.setViewportSize({width:1024,height:768});await page.screenshot({path:path.join(output,'unsplash-connect-1024.png')});
  assert.equal(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth+1),true);
  assert.deepEqual(errors,[]);console.log('PASS: provider dropdown, inline connection, secret clearing, photographer attribution, import selection, narrow layout');
 }finally{await browser.close();}
})().catch(error=>{console.error(error);process.exit(1);});
