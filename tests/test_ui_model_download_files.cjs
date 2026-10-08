// Read-only UI acceptance against an isolated backend: no model/native mutations.
const assert = require('node:assert/strict'), path = require('node:path'), fs = require('node:fs');
const {chromium} = require('playwright');
const root=path.resolve(__dirname,'..'), base=process.env.LOCAL_REMOVE_TEST_URL, profile=process.env.MIGRATION_REAL_PROFILE;
assert.equal(new URL(base).hostname,'127.0.0.1');
assert.ok(path.resolve(profile).startsWith(path.join(root,'qa-artifacts')+path.sep));
(async()=>{
 const runtime=await(await fetch(base+'/api/local-remove/runtime')).json();assert.equal(path.resolve(runtime.data_root),path.resolve(profile));
 const browser=await chromium.launch({channel:process.env.PLAYWRIGHT_CHANNEL||(process.platform==='win32'?'msedge':undefined),headless:true});
 const context=await browser.newContext({viewport:{width:1366,height:768}}),page=await context.newPage(),errors=[],mutations=[];
 const output=path.join(root,'qa-artifacts/migration/model-download-files');fs.mkdirSync(output,{recursive:true});
 await context.route('**/*',route=>{const request=route.request();if(request.method()!=='GET'){mutations.push(request.url());return route.abort();}return route.continue();});
 await context.addInitScript(()=>{for(const key of ['local-image.hardware-guide.v1','local-image.first-ai-setup.v1','local-image.first-task.v1'])localStorage.setItem(key,'1');});
 page.on('pageerror',error=>errors.push(error.message));
 try {
  await page.goto(base+'/remove');await page.waitForFunction(()=>document.body.dataset.reactReady==='true');
  await page.getByRole('button',{name:/^Settings(?:, update available)?$/}).click();
  const settings=page.getByRole('dialog',{name:'Settings',exact:true});await settings.getByRole('tab',{name:'Local AI',exact:true}).click();
  const model=page.locator('#settings-download-model');await page.waitForFunction(()=>window.LocalImageReactFeatures.settings.getSnapshot().models.length>0&&!window.LocalImageReactFeatures.settings.getSnapshot().loading);
  const ids=await model.locator('option').evaluateAll(nodes=>nodes.map(node=>node.value));
  assert.ok(ids.includes('flux2-klein-remove'));assert.ok(ids.includes('flux2-klein-4b'));assert.ok(ids.includes('qwen'));
  assert.equal(await settings.getByRole('button',{name:'FLUX AI Remove',exact:true}).count(),0);
  for(const id of ['qwen','flux2-klein-4b','flux2-klein-remove']){
   await model.selectOption(id);const list=settings.getByRole('region',{name:'Included model download files'});
   await list.waitFor();assert.ok(await list.locator('li').count()>=3);assert.ok((await list.innerText()).includes('VAE'));assert.ok((await list.innerText()).includes('Text encoder'));
   if(id==='flux2-klein-remove')assert.ok((await list.innerText()).includes('Adapter'));
  }
  const [response]=await Promise.all([page.waitForResponse(response=>new URL(response.url()).pathname==='/api/local-remove/generation/models'&&new URL(response.url()).searchParams.get('refresh')==='true'),page.locator('#settings-scan-model-folder').click()]);assert.equal(response.status(),200);
  await page.waitForFunction(()=>!window.LocalImageReactFeatures.settings.getSnapshot().loading);
  for(const [width,height]of [[1366,768],[800,560]]){
   await page.setViewportSize({width,height});await settings.getByRole('tab',{name:'General',exact:true}).click();await settings.getByRole('combobox',{name:'Interface size'}).selectOption('large');await settings.getByRole('tab',{name:'Local AI',exact:true}).click();
   await page.locator('#settings-scan-model-folder').scrollIntoViewIfNeeded();
   assert.equal(await settings.evaluate(node=>node.scrollWidth<=node.clientWidth+1),true);assert.equal(await page.locator('#settings-scan-model-folder').isVisible(),true);
   await page.screenshot({path:path.join(output,`models-${width}-${height}-large.png`)});
  }
  await page.locator('#settings-download-model').selectOption('flux2-klein-remove');await settings.getByRole('button',{name:'Model details…'}).click();
  const models=page.getByRole('dialog',{name:'Local image models',exact:true});await models.waitFor();await models.getByRole('option',{name:/FLUX.2 Klein Base 4B/}).click();
  assert.ok((await models.getByRole('region',{name:'Included model download files'}).innerText()).includes('Adapter'));
  await page.locator('#models-scan-folder').click();await page.waitForFunction(()=>!window.LocalImageReactFeatures.models.getSnapshot().loading);
  assert.deepEqual(errors,[]);assert.deepEqual(mutations,[]);
  fs.writeFileSync(path.join(output,'results.json'),JSON.stringify({passed:true,ids,errors,mutations,runtime},null,2));
  console.log('PASS unified model selector, companion files, scan and Large interface layouts');
 }catch(error){await page.screenshot({path:path.join(output,'failure.png')});throw error;}finally{await browser.close();}
})().catch(error=>{console.error(error);process.exitCode=1;});
