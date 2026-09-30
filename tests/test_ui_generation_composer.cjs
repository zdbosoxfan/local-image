// Real browser layout + original engine handler tests. Generation responses use
// real imported fixtures so UI verification does not run a GPU job.
const {chromium}=require('playwright');
const assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path');
const base=process.env.LOCAL_REMOVE_TEST_URL||'http://127.0.0.1:51274';
const out=path.resolve(process.argv[2]||'qa-artifacts/ui-redesign/generation-composer-test');fs.mkdirSync(out,{recursive:true});
const example=path.resolve('docs/images/examples/klein-teal-lighthouse.png');
const fixture=fs.existsSync(example)?fs.readFileSync(example):require('./helpers/image-fixture.cjs').imageFixture();
const models=['qwen','z-image-turbo','flux2-klein-9b'].map(id=>({id,label:{qwen:'Qwen Image 2.1','z-image-turbo':'Z-Image Turbo','flux2-klein-9b':'FLUX.2 Klein 9B'}[id],short_benefit:id==='qwen'?'Edits & alpha':id==='z-image-turbo'?'Fast drafts':'Detailed edits',available:true,variants:(id==='qwen'?['int8','bf16']:['bf16']).map(id=>({id,label:id.toUpperCase(),available:true})),capabilities:{text_to_image:true,image_reference:id!=='z-image-turbo',image_to_image:id==='z-image-turbo',max_references:id==='qwen'?10:id==='z-image-turbo'?1:4,transparent:id==='qwen',negative_prompt:id==='qwen',denoise:id==='z-image-turbo',lora:true},defaults:{variant:id==='qwen'?'int8':'bf16',width:1024,height:1024,steps:id==='qwen'?40:id==='z-image-turbo'?8:4,guidance:1},limits:{min_dimension:256,max_dimension:4096,dimension_step:32,max_pixels:4194304,min_steps:1,max_steps:100,min_guidance:1,max_guidance:id==='qwen'?10:1}}));
for(const model of models)model.sampling_guidance={steps:{recommended:model.defaults.steps,source_label:model.id==='qwen'?'Qwen publisher default':'Workflow recommendation',source_url:model.id==='qwen'?'https://github.com/QwenLM/Qwen-Image-2.1':'https://github.com/Comfy-Org/workflow_templates',note:'Audited recommendation for this local model workflow.'}};
(async()=>{
 const browser=await chromium.launch({headless:true,channel:process.env.BROWSER_CHANNEL||'msedge'});
 const context=await browser.newContext({viewport:{width:1440,height:900}}),page=await context.newPage(),errors=[],requests=[];
 page.on('pageerror',e=>errors.push(e.stack||e.message));
 await context.addInitScript(()=>{localStorage.setItem('local-image.hardware-guide.v1','1');localStorage.setItem('local-image.first-ai-setup.v1','1');});
 if(process.env.LOCAL_IMAGE_COMPOSER_INJECT==='1')await context.route('**/remove',async route=>{
   const response=await route.fetch();let html=await response.text();
   if(!html.includes('Prompt-first generation')){
     html=html.replace('</style>',fs.readFileSync('backend/frontend/generation-composer.css','utf8')+'\n</style>');
     const code=['generation-size.js','generation-composer.js','generation-guidance.js'].filter(name=>fs.existsSync('backend/frontend/'+name)).map(name=>fs.readFileSync('backend/frontend/'+name,'utf8')).join('\n');
     html=html.replace(/<\/script>(?![\s\S]*<\/script>)/,'\n'+code+'\n</script>');
   }
   await route.fulfill({response,body:html});
 });
 await context.route('**/api/local-remove/generation/models**',route=>route.fulfill({json:{connected:true,default_model:'qwen',models}}));
 await context.route('**/api/local-remove/generator/download',route=>route.fulfill({json:{running:false,models:[]}}));
 await context.route('**/api/local-remove/generation/upscale/models',route=>route.fulfill({json:{enabled:false}}));
 await context.route('**/api/local-remove/generation/library',route=>route.fulfill({json:{items:[],count:0,bytes:0}}));
 await context.route('**/api/local-remove/generation',async route=>{
   const payload=route.request().postDataJSON();requests.push(payload);
   const response=await page.request.post(base+'/api/local-remove/import',{headers:{'x-local-remove-token':route.request().headers()['x-local-remove-token']},multipart:{file:{name:'Composer test.png',mimeType:'image/png',buffer:fixture}}});
   assert.equal(response.status(),200);const session=await response.json();session.generation=payload;
   await route.fulfill({json:{session,width:session.width,height:session.height,seed:17,model:payload.model}});
 });
 async function closed(host){assert.equal(await page.locator(host+' > .composer-section[open]').count(),0,'Inspector initially collapsed: '+host);}
 async function adjacency(prompt,action){const [p,a]=await Promise.all([page.locator(prompt).boundingBox(),page.locator(action).boundingBox()]);assert.ok(p&&a,'Prompt and action visible');assert.ok(a.y+a.height<=await page.evaluate(()=>innerHeight),'Action stays in viewport');assert.ok(Math.abs((p.y+p.height)-(a.y+a.height))<2,'Prompt and action align at their bottom');assert.ok(a.x-(p.x+p.width)>=0&&a.x-(p.x+p.width)<=17,'Action adjacent to prompt');}
 async function shot(name,width){await page.setViewportSize({width,height:width===1440?900:768});await page.screenshot({path:path.join(out,name+'-'+width+'.png'),animations:'disabled'});assert.ok(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth+1),'No horizontal overflow');}
 try{
   await page.goto(base+'/remove');await page.waitForFunction(()=>settingsLoaded);
   await page.waitForFunction(()=>!!window.LocalImageGenerationComposer);
   await page.locator('#workspace-generate').click();await page.waitForFunction(()=>!generationLoading&&!LocalImageGenerationStudio.isModeBusy());
   await closed('#generation-panel-scroll');
   assert.equal(await page.locator('#generation-panel>.studio-tabs').isVisible(),false);
   assert.equal(await page.locator('#gen-width').isVisible(),false);assert.equal(await page.locator('#gen-model').isVisible(),false);
   for(const width of [1440,1024]){await shot('create',width);await adjacency('#gen-prompt','#gen-run');}
   await page.locator('#composer-create-model>summary').click();await page.locator('#gen-variant').selectOption('bf16');
   await page.locator('#composer-create-output>summary').click();assert.equal(await page.locator('#composer-create-model').getAttribute('open'),null);
   await page.locator('#gen-width').fill('1024');await page.locator('#gen-height').fill('768');await page.locator('#gen-steps').fill('21');
   assert.match(await page.locator('#gen-sampling-note').innerText(),/40 steps/);
   await page.locator('#composer-create-output>summary').click();await page.locator('#gen-prompt').fill('A lighthouse on the coast at sunset');
   await page.locator('#gen-run').click();await page.waitForFunction(()=>!busy&&!!session?.generation);
   assert.equal(requests[0].steps,21);assert.equal(requests[0].variant,'bf16');
   await page.locator('#generation-edit-tab').click();await page.waitForFunction(()=>!LocalImageGenerationStudio.isModeBusy());
   assert.equal(await page.locator('#gen-run').innerText(),'Apply edit');await page.locator('#gen-prompt').fill('Make the light warmer');
   assert.equal(await page.locator('#gen-run').innerText(),'Apply edit','Typing preserves the selected edit action');
   for(const width of [1440,1024]){await shot('edit',width);await adjacency('#gen-prompt','#gen-run');}
   await page.locator('#gen-run').click();await page.waitForFunction(()=>!busy&&!!session?.generation);assert.ok(requests[1].reference_session_ids.length);
   await page.locator('#draft-refine-open').click();await page.waitForFunction(()=>refineInitialized&&!document.getElementById('refine-close').disabled);
   await closed('#generation-stage-draft-settings');await closed('#generation-stage-final-settings');
   assert.equal(await page.locator('#refine-close').isVisible(),false);
   await page.locator('#composer-draft-model>summary').click();await page.locator('#refine-draft-model').selectOption('z-image-turbo');
   await page.locator('#composer-draft-output>summary').click();assert.equal(await page.locator('#composer-draft-model').getAttribute('open'),null);
   await page.locator('#refine-draft-steps').fill('8');await page.locator('#composer-draft-output>summary').click();
   await page.locator('#refine-draft-prompt').fill('A lighthouse on the coast at sunset');await page.locator('#refine-draft-run').click();await page.waitForFunction(()=>refineDrafts.length===1&&!refineJob);
   await page.locator('#generation-stage-final-tab').click();await page.locator('#composer-final-model>summary').click();await page.locator('#refine-final-model').selectOption('qwen');
   await page.locator('#composer-final-output>summary').click();await page.locator('#refine-final-steps').fill('30');await page.locator('#composer-final-output>summary').click();
   await page.locator('#refine-final-prompt').fill('Preserve the composition and improve the natural lighting');await page.locator('#refine-final-run').click();await page.waitForFunction(()=>refineResults.length===1&&!refineJob);
   assert.equal(requests[2].model,'z-image-turbo');assert.equal(requests[3].steps,30);assert.equal(requests[3].reference_session_ids.length,1);
   await page.waitForFunction(()=>['refine-draft-image','refine-result-image'].every(id=>{const image=document.getElementById(id);return image.complete&&image.naturalWidth>0;}));
   for(const width of [1440,1024]){await shot('refine',width);await adjacency('#refine-draft-prompt','#refine-draft-run');await adjacency('#refine-final-prompt','#refine-final-run');}
   await page.locator('#generation-stage-draft-tab').click();await closed('#generation-stage-draft-settings');
   assert.equal(await page.locator('#refine-draft-model').inputValue(),'z-image-turbo');assert.equal(await page.locator('#refine-final-steps').inputValue(),'30');
   const resultId=await page.evaluate(()=>refineSelectedResult().session.id);
   const exported=page.waitForRequest(request=>request.method()==='POST'&&request.url().includes('/session/'+resultId+'/save'));
   const download=page.waitForEvent('download');await page.keyboard.press('Control+s');await exported;
   const saved=path.join(out,'selected-refinement-export.png');await(await download).saveAs(saved);
   assert.equal(fs.readFileSync(saved).subarray(1,4).toString(),'PNG','Export keeps targeting selected refinement');
   assert.deepEqual(errors,[]);fs.writeFileSync(path.join(out,'report.json'),JSON.stringify({passed:true,testOnlyInjection:process.env.LOCAL_IMAGE_COMPOSER_INJECT==='1',requests:requests.map(p=>({model:p.model,steps:p.steps,width:p.width,height:p.height})),screenshots:6,export:'selected-refinement-export.png'},null,2));
   console.log('Generation composer: collapsed defaults, single disclosure, prompt/action adjacency, stateful Create/Edit/Draft/Refine, 6 responsive screenshots passed.');
 }catch(error){await page.screenshot({path:path.join(out,'failure.png'),animations:'disabled'}).catch(()=>{});console.error('Page errors:',errors);throw error;}finally{await browser.close();}
})().catch(error=>{console.error(error);process.exitCode=1;});

