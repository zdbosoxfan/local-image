// A slow startup health response must not reopen a user-selected image without its collection.
const {chromium}=require('playwright'),assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path');
const base=process.env.LOCAL_REMOVE_TEST_URL||'http://127.0.0.1:51259',out=path.resolve(process.argv[2]||'qa-artifacts/v07/startup-navigation');fs.mkdirSync(out,{recursive:true});
(async()=>{const browser=await chromium.launch({headless:true,channel:process.env.BROWSER_CHANNEL||'msedge'}),page=await browser.newPage(),errors=[];let release;const gate=new Promise(resolve=>release=resolve);
 page.on('pageerror',error=>errors.push(error.message));await page.addInitScript(()=>{localStorage.setItem('local-image.hardware-guide.v1','1');localStorage.setItem('local-image.first-ai-setup.v1','1');});
 await page.route('**/api/local-remove/status',async route=>{await gate;await route.fulfill({json:{ready:false,retouch_ready:true,models:[]}});});
 try{await page.goto(base+'/remove');const buffer=require('./helpers/image-fixture.cjs').imageFixture(256,256);
  await page.locator('#file').setInputFiles([{name:'First product.png',mimeType:'image/png',buffer},{name:'Second product.png',mimeType:'image/png',buffer}]);
  await page.waitForFunction(()=>collection?.entries?.length===2&&session&&!busy);
  const before=await page.evaluate(()=>({id:session.id,collection:collection.id,count:collection.entries.length}));
  assert.equal(await page.evaluate(()=>initCompleted),false,'The test must navigate while initialization is still awaiting health');
  release();await page.waitForFunction(()=>initCompleted&&!busy);
  assert.deepEqual(await page.evaluate(()=>({id:session.id,collection:collection?.id,count:collection?.entries?.length})),before);
  assert.deepEqual(errors,[]);await page.screenshot({path:path.join(out,'preserved-collection.png')});
  fs.writeFileSync(path.join(out,'results.json'),JSON.stringify({passed:true,delayed_startup:true,user_navigation_preserved:before,errors},null,2));console.log('PASS slow initialization cannot clobber active user collection/navigation.');
 }finally{release();await browser.close();}})().catch(error=>{console.error(error);process.exitCode=1;});
