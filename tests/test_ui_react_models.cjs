// Real composed React page/catalog + installed inventory. Provider search,
// repository metadata and previews below are controlled fixtures; no downloads.
const assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path'),crypto=require('node:crypto');
const {chromium}=require('playwright');
const root=path.resolve(__dirname,'..'),base=process.env.LOCAL_REMOVE_TEST_URL,profile=process.env.MIGRATION_REAL_PROFILE;
assert.ok(base&&profile);assert.equal(new URL(base).hostname,'127.0.0.1');
assert.ok(path.resolve(profile).startsWith(path.join(root,'qa-artifacts')+path.sep));
const out=path.join(root,'qa-artifacts/migration/full-models');
const hash=bytes=>crypto.createHash('sha256').update(bytes).digest('hex');
(async()=>{
 const runtime=await(await fetch(base+'/api/local-remove/runtime')).json();assert.equal(path.resolve(runtime.data_root),path.resolve(profile));fs.mkdirSync(out,{recursive:true});
 if(fs.existsSync(path.join(out,'results.json'))){const archive=path.join(out,'report-history'),stamp=Date.now();fs.mkdirSync(archive,{recursive:true});fs.copyFileSync(path.join(out,'results.json'),path.join(archive,`results-${stamp}.json`));if(fs.existsSync(path.join(out,'failure.png')))fs.copyFileSync(path.join(out,'failure.png'),path.join(archive,`failure-${stamp}.png`));}
 const actualCatalog=await(await fetch(base+'/api/local-remove/generation/models',{headers:{Origin:base}})).json();
 const browser=await chromium.launch({channel:'msedge',headless:true}),context=await browser.newContext({viewport:{width:1366,height:768}}),page=await context.newPage();
 const errors=[],requests=[],checks=[],providerFixtures=[],assets=[],shots=[];let passed=false,failure=null,fixtureInventory=false,fixtureCatalog=false,htmlHash=null;
 const style=model=>({id:'fixture-'+model,model,title:'Controlled ink wash',repo_id:'controlled/'+model,filename:'ink.safetensors',revision:'a'.repeat(40),compatibility:'curated',supported:true,preview_available:false,description:'Synthetic adapter metadata for UI verification',license:'Fixture only',usage:'reference-edit',recommended_strength:.75,trigger_phrase:'controlled ink wash',recommended_settings:{steps:6,guidance:1}});
 await context.route('**/*',async route=>{
  const request=route.request(),url=new URL(request.url());if(url.origin!==new URL(base).origin)return route.abort('blockedbyclient');
  if(url.pathname.startsWith('/api/')&&request.method()!=='GET')return route.abort('blockedbyclient');
  if(url.pathname==='/api/local-remove/generation/models'&&fixtureCatalog)return route.fulfill({json:{...actualCatalog,models:actualCatalog.models.map(model=>({...model,capabilities:{...model.capabilities,lora:true,loras:true}}))}});
  if(url.pathname==='/api/local-remove/loras/search'){providerFixtures.push('search');return route.fulfill({json:{results:[{...style(url.searchParams.get('model')),repo_id:'controlled/community',compatibility:'declared'}]}});}
  if(url.pathname==='/api/local-remove/loras/files'){providerFixtures.push('files');return route.fulfill({json:{repo_id:url.searchParams.get('repo_id'),revision:'a'.repeat(40),compatibility:'declared',files:[{filename:'community.safetensors',bytes:1024,compatibility:'declared',supported:true,description:'Controlled metadata; no remote request'}]}});}
  if(url.pathname.startsWith('/api/local-remove/loras/preview/')){providerFixtures.push('preview-blocked');return route.fulfill({status:404,body:'No fixture preview'});}
  if(url.pathname==='/api/local-remove/loras'&&fixtureInventory){const item=style(url.searchParams.get('model'));return route.fulfill({json:{installed:[item],curated:[item],job:{running:false,phase:'idle'}}});}
  return route.continue();
 });
 await context.addInitScript(()=>{localStorage.setItem('local-image.hardware-guide.v1','1');localStorage.setItem('local-image.first-ai-setup.v1','1');localStorage.setItem('local-image.first-task.v1','1');window.__modelsCsp=[];document.addEventListener('securitypolicyviolation',event=>window.__modelsCsp.push(event.violatedDirective));});
 page.on('pageerror',error=>errors.push(error.stack));page.on('request',request=>{if(request.url().includes('/api/'))requests.push({method:request.method(),path:new URL(request.url()).pathname});});
 page.on('response',response=>{if(response.url().includes('/frontend-assets/'))void response.body().then(body=>assets.push({path:new URL(response.url()).pathname,sha256:hash(body),status:response.status()}));});
 const shot=async name=>{const file=path.join(out,name);await page.screenshot({path:file,animations:'disabled'});shots.push(file);};
 try{
  const response=await page.goto(base+'/remove');htmlHash=hash(await response.body());await page.waitForFunction(()=>document.body.dataset.reactReady==='true'&&window.LocalImageEditor&&window.LocalImageReactFeatures?.models);
  assert.equal(await page.evaluate(()=>typeof window.LocalImageLegacyEditor),'undefined','Final React mode has no legacy editor bridge');
  assert.equal(await page.locator('#model-browser-dialog,#lora-dialog,#lora-file-detail,#lora-installed-list').count(),0,'Migrated legacy dialogs are absent');
  await page.getByRole('button',{name:'Settings',exact:true}).click();await page.getByRole('tab',{name:'Local AI',exact:true}).click();
  await page.waitForFunction(()=>!window.LocalImageReactFeatures.settings.getSnapshot().loading);await page.getByRole('button',{name:'Browse models…',exact:true}).click();
  const dialog=page.getByRole('dialog',{name:'Local image models',exact:true});await dialog.waitFor();await page.waitForFunction(()=>!window.LocalImageReactFeatures.models.getSnapshot().loading);
  assert.ok(await dialog.getByRole('option').count()>0);assert.equal(await dialog.getByRole('button',{name:'Models folder…',exact:true}).isDisabled(),true);
  let fileChooserCount=0;page.on('filechooser',()=>fileChooserCount++);await page.keyboard.press('Control+o');assert.equal(await dialog.isVisible(),true);assert.equal(fileChooserCount,0,'Open shortcut is suspended while the model dialog owns focus');
  checks.push('Settings Browse Models opens sole React dialog with actual backend catalog and browser capability gating');
  for(const [width,height,density]of [[800,560,'comfortable'],[1366,768,'comfortable'],[1366,768,'large']]){
   await page.setViewportSize({width,height});await page.evaluate(value=>window.LocalImageReactFeatures.settings.setDensity(value),density);await shot(`models-${width}-${height}-${density}.png`);
   const box=await dialog.boundingBox();assert.ok(box.x>=0&&box.y>=0&&box.x+box.width<=width+1&&box.y+box.height<=height+1,'Model dialog stays within viewport');
  }
  await page.evaluate(()=>window.LocalImageReactFeatures.settings.setDensity('comfortable'));await page.setViewportSize({width:1366,height:768});
  const selected=await page.evaluate(()=>window.LocalImageReactFeatures.models.getSnapshot().selectedModelId);await dialog.getByRole('button',{name:'Use this model',exact:true}).click();
  const generation=page.getByRole('region',{name:'Image generation',exact:true});
  assert.equal(await page.evaluate(()=>{const feature=window.LocalImageReactFeatures.generation;return feature.getSnapshot().drafts[feature.activeDraftKey()].modelId;}),selected);
  assert.equal(await generation.getByRole('combobox',{name:'Model',exact:true}).inputValue(),selected);
  const actualModel=actualCatalog.models.find(model=>model.id===selected);
  if(actualModel.capabilities.lora===false||actualModel.capabilities.loras===false)assert.equal(await generation.getByRole('button',{name:/Styles \/ LoRAs/}).isDisabled(),true,'Unavailable backend LoRA capability gates the React opener');
  const localInventory=await page.evaluate(async id=>await(await fetch('/api/local-remove/loras?model='+encodeURIComponent(id))).json(),selected);
  assert.ok(Array.isArray(localInventory.installed));assert.deepEqual(providerFixtures,[],'Actual installed inventory is local and does not request provider search or previews');
  checks.push('Actual backend adapter capability and installed inventory retained');
  fixtureCatalog=true;fixtureInventory=true;
  await generation.getByRole('button',{name:'Models',exact:true}).click();await dialog.waitFor();await page.waitForFunction(()=>!window.LocalImageReactFeatures.models.getSnapshot().loading);await dialog.getByRole('button',{name:'Use this model',exact:true}).click();
  await generation.getByRole('button',{name:/Styles \/ LoRAs/}).click();const lora=page.getByRole('dialog',{name:'LoRA library',exact:true});await lora.waitFor();await page.waitForFunction(()=>!window.LocalImageReactFeatures.models.getSnapshot().loading);
  assert.equal(await page.evaluate(()=>window.LocalImageReactFeatures.models.getSnapshot().context.modelId),selected);
  await lora.getByRole('button',{name:'Use',exact:true}).click();const styles=await page.evaluate(()=>{const feature=window.LocalImageReactFeatures.generation;return feature.getSnapshot().drafts[feature.activeDraftKey()].loras;});assert.equal(styles.length,1);assert.equal(styles[0].strength,.75);
  await lora.getByRole('button',{name:'Information about Controlled ink wash',exact:true}).click();await lora.getByRole('button',{name:'Add trigger',exact:true}).click();await lora.getByRole('button',{name:'Apply recommended sampling',exact:true}).click();
  const draft=await page.evaluate(()=>{const feature=window.LocalImageReactFeatures.generation;return feature.getSnapshot().drafts[feature.activeDraftKey()];});assert.match(draft.prompt,/controlled ink wash/);assert.equal(draft.steps,6);
  await lora.getByRole('tab',{name:'Browse',exact:true}).click();await page.waitForFunction(()=>!window.LocalImageReactFeatures.models.getSnapshot().searching);
  await lora.getByRole('button',{name:'Choose file…',exact:true}).last().click();await page.waitForFunction(()=>!window.LocalImageReactFeatures.models.getSnapshot().filesLoading);
  assert.equal(await lora.getByRole('combobox',{name:'Adapter file',exact:true}).inputValue(),'community.safetensors');assert.equal(await lora.getByRole('button',{name:'Download adapter',exact:true}).isDisabled(),true);
  assert.ok(await lora.getByRole('checkbox',{name:/Accept publisher-declared compatibility/}).count());assert.match(await lora.innerText(),/downloads require the desktop app/);
  await shot('lora-controlled.png');await page.keyboard.press('Escape');await lora.waitFor({state:'hidden'});assert.equal(await page.evaluate(()=>window.LocalImageReactFeatures.models.isOpen()),false);
  checks.push('Controlled adapter selection, trigger/sampling drafts, compatibility and download gating work without hidden controls');
  assert.deepEqual(requests.filter(request=>request.method!=='GET'),[]);assert.deepEqual(errors,[]);assert.deepEqual(await page.evaluate(()=>window.__modelsCsp),[]);passed=true;
 }catch(error){failure=error.stack;await shot('failure.png').catch(()=>{});throw error;}
 finally{fs.writeFileSync(path.join(out,'results.json'),JSON.stringify({passed,failure,runtime,checks,errors,requests,providerFixtures,assets,htmlHash,shots,evidence:'Production composed page; actual read-only catalog and installed inventory; controlled provider search/file metadata; no downloads or native dialogs'},null,2));await browser.close();}
 console.log(`PASS React Models/LoRA: ${checks.length} composed checks; ${out}`);
})().catch(error=>{console.error(error);process.exitCode=1;});
