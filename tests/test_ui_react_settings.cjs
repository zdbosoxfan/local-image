// Actual metadata reads and browser preferences, no native setup mutation,
// provider request, inference or model download.
const assert=require('node:assert/strict'),path=require('node:path'),fs=require('node:fs'),crypto=require('node:crypto');
const {chromium}=require('playwright');
const root=path.resolve(__dirname,'..'),base=process.env.LOCAL_REMOVE_TEST_URL,profile=process.env.MIGRATION_REAL_PROFILE;
assert.ok(base&&profile);assert.equal(new URL(base).hostname,'127.0.0.1');
assert.ok(path.resolve(profile).startsWith(path.join(root,'qa-artifacts')+path.sep));
const out=path.join(root,'qa-artifacts/migration/full-settings');fs.mkdirSync(out,{recursive:true});
if(fs.existsSync(path.join(out,'results.json'))){const archive=path.join(out,'report-history');fs.mkdirSync(archive,{recursive:true});fs.copyFileSync(path.join(out,'results.json'),path.join(archive,`results-${Date.now()}.json`));}
(async()=>{
 const runtime=await(await fetch(base+'/api/local-remove/runtime')).json();assert.equal(path.resolve(runtime.data_root),path.resolve(profile));
 const browser=await chromium.launch({channel:'msedge',headless:true}),context=await browser.newContext({viewport:{width:1366,height:768}}),page=await context.newPage();
 const errors=[],requests=[],checks=[],servedAssets=[],assetReads=[];let passed=false,failure=null,htmlHash=null,csp=null,cspViolations=[];
 const hash=bytes=>crypto.createHash('sha256').update(bytes).digest('hex');
 await context.route('**/*',route=>new URL(route.request().url()).origin===new URL(base).origin?route.continue():route.abort('blockedbyclient'));
 await context.addInitScript(()=>{localStorage.setItem('local-image.hardware-guide.v1','1');localStorage.setItem('local-image.first-ai-setup.v1','1');localStorage.setItem('local-image.first-task.v1','1');window.__settingsCsp=[];document.addEventListener('securitypolicyviolation',event=>window.__settingsCsp.push(event.violatedDirective));});
 page.on('pageerror',error=>errors.push(error.stack));page.on('request',request=>{if(request.url().includes('/api/'))requests.push({method:request.method(),path:new URL(request.url()).pathname});});
 page.on('response',response=>{if(response.url().includes('/frontend-assets/'))assetReads.push(response.body().then(body=>servedAssets.push({path:new URL(response.url()).pathname,sha256:hash(body),status:response.status()})));});
 try{
  const response=await page.goto(base+'/remove');htmlHash=hash(await response.body());csp=response.headers()['content-security-policy'];await page.waitForFunction(()=>document.body.dataset.reactReady==='true'&&window.LocalImageEditor);
  for(const id of ['settings-dialog','hardware-dialog','shortcuts-dialog','model','ask-before-overwrite','interface-density'])assert.equal(await page.locator('#'+id).count(),0,`Legacy ${id} removed`);
  await page.getByRole('button',{name:'Settings',exact:true}).click();await page.getByRole('dialog',{name:'Settings',exact:true}).waitFor();
  await page.getByRole('checkbox',{name:'Ask before overwriting original images'}).uncheck();assert.equal(await page.evaluate(()=>window.LocalImageEditor.getSnapshot().askBeforeOverwrite),false);
  await page.getByRole('combobox',{name:'Interface size'}).selectOption('large');assert.equal(await page.evaluate(()=>document.documentElement.dataset.uiDensity),'large');
  assert.equal(await page.locator('.li-layer-name').count(),0);await page.getByRole('button',{name:'Close settings',exact:true}).click();
  await page.getByRole('button',{name:'Settings',exact:true}).click();assert.equal(await page.getByRole('checkbox',{name:'Ask before overwriting original images'}).isChecked(),false);
  assert.equal(await page.getByRole('combobox',{name:'Interface size'}).inputValue(),'large');checks.push('React-owned preferences persist and apply without duplicate legacy controls');
  await page.getByRole('combobox',{name:'Interface size'}).selectOption('comfortable');await page.getByRole('tab',{name:'Local AI',exact:true}).click();
  await page.waitForFunction(()=>!window.LocalImageReactFeatures.settings.getSnapshot().loading);assert.equal(await page.getByRole('button',{name:'Choose folder…',exact:true}).isDisabled(),true);
  await page.keyboard.press('Control+o');assert.equal(await page.getByRole('dialog',{name:'Settings',exact:true}).isVisible(),true);
  assert.equal(await page.evaluate(()=>window.LocalImageReactFeatures.settings.isOpen()),true);checks.push('Actual setup/status data, browser-native capability gating and modal shortcut exclusion');
  for(const [w,h,density]of [[800,560,'comfortable'],[1366,768,'comfortable'],[1366,768,'large'],[1920,1080,'comfortable']]){
   await page.setViewportSize({width:w,height:h});await page.evaluate(value=>window.LocalImageReactFeatures.settings.setDensity(value),density);
   await page.screenshot({path:path.join(out,`settings-${w}-${h}-${density}.png`)});const box=await page.getByRole('dialog',{name:'Settings',exact:true}).boundingBox();assert.ok(box.x>=0&&box.x+box.width<=w+1);assert.ok(box.y>=0&&box.y+box.height<=h+1);
  }
  await page.getByRole('button',{name:'Close settings',exact:true}).click();await page.getByRole('menuitem',{name:'Help',exact:true}).click();await page.getByRole('menuitem',{name:'Hardware guide…',exact:true}).click();
  await page.getByRole('dialog',{name:'Hardware guide',exact:true}).waitFor();await page.waitForFunction(()=>!window.LocalImageReactFeatures.settings.getSnapshot().loading);
  await page.getByRole('button',{name:'Model memory guide',exact:true}).click();assert.ok((await page.getByRole('table',{name:'Model memory planning'}).locator('tbody tr').count())>0);await page.getByRole('button',{name:'Close hardware guide',exact:true}).click();
  await page.getByRole('menuitem',{name:'Help',exact:true}).click();await page.getByRole('menuitem',{name:'Keyboard shortcuts…',exact:true}).click();await page.getByRole('dialog',{name:'Keyboard shortcuts',exact:true}).waitFor();assert.ok(await page.getByRole('table',{name:'Keyboard shortcuts'}).locator('tr').count()>8);await page.keyboard.press('Escape');
  checks.push('Existing Help routes open React hardware and shortcut dialogs with real backend guidance');
  assert.deepEqual(requests.filter(item=>item.method!=='GET'),[],'Opening settings/help never performs setup writes');assert.deepEqual(errors,[]);cspViolations=await page.evaluate(()=>window.__settingsCsp);assert.deepEqual(cspViolations,[]);passed=true;
 }catch(error){failure=error.stack;await page.screenshot({path:path.join(out,'failure.png')}).catch(()=>{});throw error;}
 finally{await Promise.all(assetReads);fs.writeFileSync(path.join(out,'results.json'),JSON.stringify({passed,failure,runtime,checks,errors,requests,htmlHash,csp,cspViolations,servedAssets},null,2));await browser.close();}
 console.log(`PASS React Settings: ${checks.length} actual browser checks; ${out}`);
})().catch(error=>{console.error(error);process.exitCode=1;});
