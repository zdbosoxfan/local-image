// UI navigation and state preservation. Generation is mocked; real import/export APIs are retained.
const {chromium}=require('playwright');
const assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path');
const base=process.env.LOCAL_REMOVE_TEST_URL||'http://127.0.0.1:51273';
const output=path.resolve(process.argv[2]||'qa-artifacts/ui-redesign/generation-studio-test');
fs.mkdirSync(output,{recursive:true});
const fixture=require('./helpers/image-fixture.cjs').imageFixture();
const models=['qwen','z-image-turbo','flux2-klein-9b','ernie-image'].map(id=>({id,label:{qwen:'Qwen Image 2.1','z-image-turbo':'Z-Image Turbo','flux2-klein-9b':'FLUX.2 Klein 9B','ernie-image':'ERNIE Image'}[id],available:true,variants:(id==='qwen'?['int8','bf16']:['bf16']).map(id=>({id,label:id.toUpperCase(),available:true})),capabilities:{text_to_image:true,image_reference:['qwen','flux2-klein-9b'].includes(id),image_to_image:id==='z-image-turbo',max_references:id==='qwen'?10:id==='z-image-turbo'?1:id==='ernie-image'?0:4,transparent:id==='qwen',negative_prompt:id==='qwen',denoise:id==='z-image-turbo',lora:id!=='ernie-image'},defaults:{variant:id==='qwen'?'int8':'bf16',width:1024,height:1024,steps:id==='qwen'?25:id==='z-image-turbo'?8:id==='ernie-image'?50:4,guidance:1},limits:{min_dimension:256,max_dimension:4096,dimension_step:32,max_pixels:4194304,min_steps:1,max_steps:100,min_guidance:1,max_guidance:id==='qwen'?10:1}}));
async function main(){
 const browser=await chromium.launch({headless:true,channel:process.env.BROWSER_CHANNEL||'msedge'});
 const context=await browser.newContext({viewport:{width:1440,height:1000}}),page=await context.newPage(),errors=[],requests=[];
 page.on('pageerror',error=>errors.push(error.message));
 await context.addInitScript(()=>{localStorage.setItem('local-image.hardware-guide.v1','1');localStorage.setItem('local-image.first-ai-setup.v1','1');});
 await context.route('**/api/local-remove/generation/models**',route=>route.fulfill({json:{connected:true,default_model:'qwen',models}}));
 await context.route('**/api/local-remove/generator/download',route=>route.fulfill({json:{running:false,models:[]}}));
 await context.route('**/api/local-remove/generation/upscale/models',route=>route.fulfill({json:{enabled:false}}));
 await context.route('**/api/local-remove/generation/library',route=>route.fulfill({json:{items:[],count:0,bytes:0}}));
 await context.route('**/api/local-remove/generation',async route=>{
   const payload=route.request().postDataJSON();requests.push(payload);
   const response=await page.request.post(base+'/api/local-remove/import',{headers:{'x-local-remove-token':route.request().headers()['x-local-remove-token']},multipart:{file:{name:'Studio fixture.png',mimeType:'image/png',buffer:fixture}}});
   assert.equal(response.status(),200);const session=await response.json();session.generation=payload;
   await route.fulfill({json:{session,width:session.width,height:session.height,seed:17,model:payload.model}});
 });
 try{
   await page.goto(base+'/remove');await page.waitForFunction(()=>settingsLoaded);
   await page.locator('#file').setInputFiles({name:'Existing editor image.png',mimeType:'image/png',buffer:fixture});
   await page.waitForFunction(()=>session&&!busy);
   const existingEditorId=await page.evaluate(()=>session.id);
   await page.locator('#workspace-generate').click();await page.waitForFunction(()=>!generationLoading);
   assert.equal(await page.locator('#gen-width').isVisible(),true,'Create exposes width beside prompt');
   assert.equal(await page.locator('#gen-height').isVisible(),true);
   assert.equal(await page.locator('#gen-steps').isVisible(),true);
   await page.locator('#generation-precision-options > summary').click();
   await page.locator('#gen-variant').selectOption('bf16');
   assert.equal(await page.locator('#gen-variant').inputValue(),'bf16','Installed models retain precision selection even when model setup is hidden');
   await page.locator('#generation-precision-options > summary').click();
   await page.locator('#gen-prompt').fill('Create prompt stays separate');
   await page.locator('#draft-refine-open').click();await page.waitForFunction(()=>refineInitialized);
   assert.equal(await page.locator('#refine-dialog').evaluate(node=>node.matches(':modal')),false,'Refinement must not trap the workspace in a modal');
   assert.equal(await page.evaluate(()=>modalOpen()),false);
   assert.equal(await page.locator('.canvas-area').isVisible(),false);
   assert.equal(await page.locator('#generation-create-tab').isVisible(),true);
   assert.equal(await page.locator('#refine-draft-width').isVisible(),true);
   assert.equal(await page.locator('#refine-final-width').isVisible(),true);
   assert.equal(await page.locator('#refine-final-steps').isVisible(),true);
   await page.locator('#refine-draft-model').selectOption('z-image-turbo');
   await page.locator('#generation-stage-draft-tab').click();
   await page.locator('#refine-draft-prompt').fill('Draft composition with a red sculpture');
   await page.locator('#refine-draft-width').fill('1024');
   await page.locator('#refine-draft-height').fill('1024');
   assert.match(await page.locator('#refine-draft-recommended').textContent(),/8 steps/);
   await page.locator('#refine-draft-run').click();await page.waitForFunction(()=>refineDrafts.length===1&&!refineJob);
   assert.equal(requests[0].model,'z-image-turbo');assert.equal(requests[0].steps,8);
   await page.locator('#refine-final-model').selectOption('qwen');
   await page.locator('#generation-stage-final-tab').click();
   await page.locator('#refine-copy-draft-prompt').click();
   assert.equal(await page.locator('#refine-final-prompt').inputValue(),'Draft composition with a red sculpture');
   await page.locator('#refine-final-prompt').fill('Keep the red sculpture, refine the lighting');
   await page.locator('#refine-final-prompt').press('End');
   await page.locator('#refine-final-prompt').pressSequentially(' additional text');
   await page.keyboard.press('Control+z');
   const undonePrompt=await page.locator('#refine-final-prompt').inputValue();
   assert.ok(undonePrompt.startsWith('Keep the red sculpture, refine the lighting')&&undonePrompt.length<'Keep the red sculpture, refine the lighting additional text'.length,'Native text undo is retained in the refinement prompt');
   await page.locator('#refine-final-prompt').fill('Keep the red sculpture, refine the lighting');
   await page.locator('#refine-final-width').fill('1536');
   await page.locator('#refine-final-height').fill('1024');
   await page.locator('#refine-final-steps').fill('30');
   await page.locator('#refine-final-transparent').check();
   await page.locator('#refine-final-run').click();await page.waitForFunction(()=>refineResults.length===1&&!refineJob);
   assert.equal(requests[1].width,1536);assert.equal(requests[1].height,1024);assert.equal(requests[1].steps,30);assert.equal(requests[1].transparent,true);
   assert.equal(requests[1].reference_session_ids.length,1,'Refinement receives the selected draft');
   assert.equal(await page.locator('#refine-draft-steps').inputValue(),'8','Stage values stay independent');
   assert.equal(await page.evaluate(()=>session.id),existingEditorId,'Draft and refine do not silently replace the editor document');
   await page.locator('#refine-library').click();assert.equal(await page.locator('#generated-library-dialog').isVisible(),true);await page.locator('#generated-library-close').click();
   assert.equal(await page.locator('#refine-dialog').isVisible(),true,'Closing the library returns to the inline workflow');
   await page.locator('#refine-compare-larger').click();assert.equal(await page.locator('.generation-refine-inspector').isVisible(),false);await page.locator('#refine-compare-larger').click();
   for(const [width,height] of [[1440,1000],[1024,768],[800,600]]){
     await page.setViewportSize({width,height});
     const layout=await page.evaluate(()=>({scroll:document.documentElement.scrollWidth,width:window.innerWidth,areas:[...document.querySelectorAll('.refine-image-area')].map(node=>({width:node.clientWidth,height:node.clientHeight})),modelVisible:!!document.getElementById('refine-final-model').getClientRects().length}));
     assert.ok(layout.scroll<=layout.width+1,'No horizontal document overflow at '+width);assert.ok(layout.areas.every(area=>area.width>=180&&area.height>=100),'Image panes remain usable at '+width);assert.equal(layout.modelVisible,true);
     await page.screenshot({path:path.join(output,'draft-refine-'+width+'.png'),animations:'disabled'});
   }
   await page.setViewportSize({width:1440,height:1000});
   await page.evaluate(()=>applyInterfaceDensity('large'));
   await page.locator('#refine-final-run').scrollIntoViewIfNeeded();
   const largeLayout=await page.locator('#refine-dialog').evaluate(node=>({width:node.clientWidth,scroll:node.scrollWidth,controlSize:parseFloat(getComputedStyle(document.getElementById('refine-final-steps')).fontSize)}));
   assert.ok(largeLayout.scroll<=largeLayout.width+1,'Large text does not overflow the refinement workspace horizontally');
   assert.ok(largeLayout.controlSize>=26,'Large interface preference applies to refinement controls');
   await page.screenshot({path:path.join(output,'draft-refine-large-text.png'),animations:'disabled'});
   await page.evaluate(()=>applyInterfaceDensity('comfortable'));
   await page.locator('#generation-create-tab').click();
   assert.equal(await page.locator('.canvas-area').isVisible(),true);assert.equal(await page.locator('#refine-dialog').isVisible(),false);
   assert.equal(await page.locator('#gen-prompt').inputValue(),'Create prompt stays separate');
   await page.locator('#generation-create-tab').focus();await page.keyboard.press('ArrowRight');await page.waitForFunction(()=>document.getElementById('refine-dialog').open);
   assert.equal(await page.locator('#refine-final-prompt').inputValue(),'Keep the red sculpture, refine the lighting');
   const refinedId=await page.evaluate(()=>refineSelectedResult().session.id);
   const editorBefore=await page.evaluate(()=>({id:session.id,revision:session.revision,zoom,selection:hasSelection}));
   await page.locator('#edit-menu-trigger').click();
   for(const id of ['undo','redo','merge','restore','close-image','clear','actual-size','fit','zoom-in','zoom-out','before']){
     assert.equal(await page.locator('#'+id).isDisabled(),true,id+' is unavailable while its editor image is hidden');
     await page.locator('#'+id).dispatchEvent('click');
   }
   assert.deepEqual(await page.evaluate(()=>({id:session.id,revision:session.revision,zoom,selection:hasSelection})),editorBefore,'Unsupported menu commands do not alter hidden editor state');
   await page.keyboard.press('Escape');
   async function restoreOriginalBehindRefinement(){
     if(await page.locator('#refine-dialog').isVisible())await page.locator('#generation-create-tab').click();
     await page.evaluate(async id=>{await openSession(openDocuments.get(id));setWorkspace('generate');},existingEditorId);
     await page.locator('#draft-refine-open').click();await page.waitForFunction(()=>document.getElementById('refine-dialog').open&&refineInitialized&&!document.getElementById('refine-close').disabled);
     assert.equal(await page.evaluate(()=>session.id),existingEditorId);
   }
   for(const command of ['save','save-project','save-project-as','return','save-unique','document-save']){
     await restoreOriginalBehindRefinement();
     const project=command.startsWith('save-project'),endpoint=project?'export-project':'save';
     const isHiddenProxy=['return','save-unique','document-save'].includes(command);
     const responsePromise=page.waitForResponse(response=>response.request().method()==='POST'&&response.url().includes('/session/'+refinedId+'/'+endpoint));
     const effects=Promise.all([responsePromise,isHiddenProxy?Promise.resolve(null):page.waitForEvent('download')]);
     if(isHiddenProxy)await page.locator('#'+command).dispatchEvent('click');
     else{await page.locator('#file-menu-trigger').click();await page.locator('#'+command).click();}
     const [savedResponse,savedDownload]=await effects;
     assert.equal(savedResponse.status(),200,command+' saves successfully');
     const savedRequest=savedResponse.request();
     if(!project){assert.equal(savedRequest.postDataJSON().return_to_source,false);assert.notEqual(savedRequest.postDataJSON().mode,'overwrite');}
     const target=path.join(output,'menu-'+command+(project?'.lremove':'.png'));
     if(savedDownload)await savedDownload.saveAs(target);
     else{const result=await savedResponse.json();const artifact=await page.request.get(new URL(result.download,base).href);assert.equal(artifact.status(),200);fs.writeFileSync(target,await artifact.body());}
     await page.waitForFunction(()=>!busy);
     assert.equal(await page.evaluate(()=>session.id),refinedId,command+' targets the selected refinement');
     assert.equal(fs.readFileSync(target).subarray(project?0:1,project?2:4).toString(),project?'PK':'PNG');
   }
   await restoreOriginalBehindRefinement();
   for(const command of ['open','open-project','open-folder']){
     if(!await page.locator('#refine-dialog').isVisible()){await page.locator('#draft-refine-open').click();await page.waitForFunction(()=>document.getElementById('refine-dialog').open&&!document.getElementById('refine-close').disabled);}
     const fileChooser=page.waitForEvent('filechooser');
     await page.locator('#file-menu-trigger').click();await page.locator('#'+command).click();
     const chooser=await fileChooser;
     if(command==='open-folder'){const emptyFolder=path.join(output,'empty-folder');fs.mkdirSync(emptyFolder,{recursive:true});await chooser.setFiles(emptyFolder);}
     else await chooser.setFiles([]);
     assert.equal(await page.locator('#refine-dialog').isVisible(),false,command+' returns to Create before opening the chooser');
     assert.equal(await page.evaluate(()=>session.id),existingEditorId,'Cancelling the chooser retains the editor document');
   }
   await page.locator('#draft-refine-open').click();await page.waitForFunction(()=>document.getElementById('refine-dialog').open&&!document.getElementById('refine-close').disabled);
   await page.keyboard.press('Control+w');
   assert.equal(await page.locator('#refine-dialog').isVisible(),false,'Ctrl+W returns to Create without closing the editor image');
   assert.equal(await page.evaluate(()=>session.id),existingEditorId);
   assert.equal(await page.locator('#close-image').isEnabled(),true,'Editor commands are restored after leaving refinement');
   await page.locator('#draft-refine-open').click();await page.waitForFunction(()=>document.getElementById('refine-dialog').open&&!document.getElementById('refine-close').disabled);
   const exportRequest=page.waitForRequest(request=>request.method()==='POST'&&request.url().includes('/session/'+refinedId+'/save'));
   const download=page.waitForEvent('download');
   await page.keyboard.press('Control+s');
   await exportRequest;
   await (await download).saveAs(path.join(output,'selected-refinement-export.png'));
   await page.waitForFunction(()=>!busy);
   assert.equal(await page.evaluate(()=>session.id),refinedId,'Save targets the visible refined image rather than the previously open editor image');
   assert.equal(fs.readFileSync(path.join(output,'selected-refinement-export.png')).subarray(1,4).toString(),'PNG');
   await page.locator('#draft-refine-open').click();await page.waitForFunction(()=>document.getElementById('refine-dialog').open);
   await page.locator('#workspace-retouch').click();assert.equal(await page.locator('#refine-dialog').isVisible(),false);assert.equal(await page.evaluate(()=>modalOpen()),false);
   assert.deepEqual(errors,[]);
   console.log('PASS: inline refinement, independent stage settings, reference handoff, library/compare, responsive and large text layouts, native text undo, safe File/menu/proxy/keyboard PNG and project exports, disabled hidden-editor commands, safe open/close and persona exit. Generation mocked; image imports and exports real.');
 }catch(error){await page.screenshot({path:path.join(output,'failure.png'),animations:'disabled'});throw error;}
 finally{await context.unrouteAll({behavior:'ignoreErrors'});await browser.close();}
}
main().catch(error=>{console.error(error);process.exitCode=1;});
