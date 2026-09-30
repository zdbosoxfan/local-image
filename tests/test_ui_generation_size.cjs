// Geometry is pure; UI uses real documents with only GPU/model inventory mocked.
const assert = require('node:assert/strict'), fs = require('node:fs'), path = require('node:path');
const {fitDimensions, dimensionBounds} = require('../backend/frontend/generation-size.js');
const large = {min_dimension:256,max_dimension:4096,dimension_step:32,max_pixels:4194304};
const small = {min_dimension:256,max_dimension:1536,dimension_step:64,max_pixels:1048576};
// These area-capped fixtures exercise externally supplied limits; they are not
// product defaults. Connected Comfy workflows currently expose no pixel cap.
const connected = {min_dimension:16,max_dimension:16384,dimension_step:16,max_pixels:null};
const fit = (width,height,ratio=1,axis='width',limits=large,locked=true) => fitDimensions({width,height,ratio,axis,locked},limits);
function valid(value,limits) {
  assert.ok(value.width>=limits.min_dimension && value.height>=limits.min_dimension);
  if (limits.max_dimension != null) assert.ok(value.width<=limits.max_dimension && value.height<=limits.max_dimension);
  assert.equal(value.width%limits.dimension_step,0);assert.equal(value.height%limits.dimension_step,0);
  if (limits.max_pixels != null) assert.ok(value.width*value.height<=limits.max_pixels);
}
assert.deepEqual(fit(4096,1024),{width:2048,height:2048});
assert.deepEqual(fit(2048,1024,3/2),{width:2016,height:1344});
assert.deepEqual(fit(1536,1600,3/2,'height'),{width:2400,height:1600});
assert.deepEqual(fit(4096,1024,16/9),{width:2560,height:1440});
assert.deepEqual(fit(4096,4096,16/9,'height',small),{width:1024,height:576});
assert.deepEqual(fit(4096,2048,1,'width',large,false),{width:2048,height:2048});
assert.deepEqual(fit(1024,4096,1,'height',large,false),{width:1024,height:4096});
assert.deepEqual(fit(0,NaN),{width:1024,height:1024});
assert.equal(dimensionBounds({width:1024,height:2048,locked:false},large).maxWidth,2048);
assert.equal(dimensionBounds({width:1024,height:576,ratio:16/9,locked:true},large).maxWidth,2560);
assert.equal(dimensionBounds({width:1024,height:576,ratio:16/9,locked:true},large).widthStep,512);
assert.deepEqual(fit(3840,2160,16/9,'width',connected),{width:3840,height:2160});
assert.deepEqual(fit(4096,2304,16/9,'width',connected),{width:4096,height:2304});
assert.deepEqual(fit(8192,8192,1,'width',connected),{width:8192,height:8192});
assert.deepEqual(fit(8000,4500,16/9,'width',{max_pixels:null}),{width:8000,height:4500});
assert.equal(dimensionBounds({width:8000,height:4500,ratio:16/9,locked:true},{max_pixels:null}).maxWidth,Infinity);
assert.deepEqual(fit(4096,2304,16/9,'width',{width:{min:16,max:8192,step:16},height:{min:32,max:4096,step:32},max_pixels:null}),{width:4096,height:2304});
const largeInputStart=performance.now();
assert.deepEqual(fit(1e9,1e9,16/9,'width',{}),{width:1000000000,height:562500000});
assert.deepEqual(fit(1e308,1e308,1,'width',{}),{width:1024,height:1024});
fit(1e9,16,1e9,'width',{});
assert.ok(performance.now()-largeInputStart<1000,'Huge uncapped input is bounded work, not a pixel-by-pixel search');
for (const limits of [small,large,{...large,dimension_step:16,max_pixels:2097152}]) {
  for (const ratio of [1,3/2,2/3,16/9,1000/733]) {
    for (const axis of ['width','height']) for (const size of [0,1,257,512,1024,1695,4096,Infinity,99999]) {
      const result=fit(size,size,ratio,axis,limits);valid(result,limits);
      if ([1,3/2,2/3,16/9].includes(ratio)) assert.ok(Math.abs(result.width/result.height-ratio)<1e-9);
    }
  }
}
console.log('PASS linked geometry: 270 mixed-limit cases plus uncapped 4K/8K, missing ceilings and per-axis connected-node bounds.');
if (process.argv.includes('--unit')) process.exit(0);
const {chromium}=require('playwright');
const base=process.env.LOCAL_REMOVE_TEST_URL||'http://127.0.0.1:51273';
const output=path.resolve(process.env.LOCAL_REMOVE_SIZE_OUTPUT||'qa-artifacts/ui-redesign/generation-size-test');fs.mkdirSync(output,{recursive:true});
const fixture=require('./helpers/image-fixture.cjs').imageFixture(1536,1024);
const models=['qwen','z-image-turbo','flux2-klein-9b'].map(id=>({id,label:id==='qwen'?'Qwen Image 2.1':id==='z-image-turbo'?'Z-Image Turbo':'FLUX.2 Klein 9B',available:true,variants:[{id:'test',label:'Test',available:true}],capabilities:{text_to_image:true,image_reference:id!=='z-image-turbo',image_to_image:id==='z-image-turbo',max_references:id==='qwen'?10:id==='z-image-turbo'?1:4,transparent:false,negative_prompt:false,denoise:id==='z-image-turbo'},defaults:{variant:'test',width:1024,height:1024,steps:id==='qwen'?40:id==='z-image-turbo'?8:4,guidance:1},limits:{...(id==='qwen'?connected:small),...(id==='qwen'?{reference_dimensions:{...connected,min_dimension:32,dimension_step:32,max_dimension:null}}:{}),min_steps:id==='flux2-klein-9b'?4:1,max_steps:id==='qwen'?100:id==='z-image-turbo'?20:4,min_guidance:1,max_guidance:1}}));
async function main(){
 const browser=await chromium.launch({headless:true,channel:process.env.BROWSER_CHANNEL||'msedge'}),context=await browser.newContext({viewport:{width:1440,height:1000}}),page=await context.newPage(),errors=[],requests=[];
 page.on('pageerror',error=>errors.push(error.message));
 await context.addInitScript(()=>{localStorage.setItem('local-image.hardware-guide.v1','1');localStorage.setItem('local-image.first-ai-setup.v1','1');});
 await context.route('**/api/local-remove/generation/models**',route=>route.fulfill({json:{connected:true,default_model:'qwen',models}}));
 await context.route('**/api/local-remove/generator/download',route=>route.fulfill({json:{running:false,models:[]}}));
 await context.route('**/api/local-remove/generation/upscale/models',route=>route.fulfill({json:{enabled:false}}));
 await context.route('**/api/local-remove/generation',async route=>{
   const payload=route.request().postDataJSON();requests.push(payload);
   const imported=await page.request.post(base+'/api/local-remove/import',{headers:{'x-local-remove-token':route.request().headers()['x-local-remove-token']},multipart:{file:{name:'Generated test.png',mimeType:'image/png',buffer:fixture}}});
   const session=await imported.json();session.generation=payload;
   await route.fulfill({json:{session,width:session.width,height:session.height,seed:17,model:payload.model}});
 });
 const ready=mode=>page.waitForFunction(mode=>LocalImageGenerationStudio.mode()===mode&&!LocalImageGenerationStudio.isModeBusy()&&!busy&&!generationLoading,mode);
 async function reveal(id){await page.locator('#'+id).evaluate(input=>{const panel=input.closest('[data-gen-panel]');if(panel?.hidden&&typeof selectGenerationTab==='function')selectGenerationTab(panel.dataset.genPanel);for(let parent=input.parentElement;parent;parent=parent.parentElement)if(parent.tagName==='DETAILS')parent.open=true;});}
 async function change(id,value){await reveal(id);await page.locator('#'+id).fill(String(value));await page.locator('#'+id).press('Tab');}
 async function size(prefix){return page.evaluate(prefix=>({width:Number(document.getElementById(prefix+'-width').value),height:Number(document.getElementById(prefix+'-height').value)}),prefix);}
 async function preset(id,value){await reveal(id);await page.locator('#'+id).selectOption(value);}
 try {
   await page.goto(base+'/remove');await page.waitForFunction(()=>settingsLoaded);
   await page.waitForFunction(()=>!!window.LocalImageGenerationSize);
   await page.evaluate(()=>{refineUpscaleStatus={enabled:false,limits:{min_dimension:2,dimension_step:2,max_dimension:null,max_pixels:null}};});
   assert.equal(await page.evaluate(()=>upscaleSizeValid({width:1000,height:667},{width:6000,height:4002})),true,'Upscale validation has no guessed 4096-pixel ceiling');
   assert.equal(await page.evaluate(()=>upscaleSizeValid({width:1000,height:667},{width:5001,height:3336})),false,'SeedVR2 rejects odd sizes that its postprocessor would crop');
   assert.deepEqual(await page.evaluate(()=>{document.getElementById('refine-upscale-preset').value='3840';return upscaleTarget({width:1000,height:667});}),{width:3840,height:2562},'Preset rounding follows the connected upscaler grid');
   await page.locator('#workspace-generate').click();await ready('create');
   await page.locator('#gen-prompt').fill('A bright mountain landscape');
   await change('gen-width',1536);assert.deepEqual(await size('gen'),{width:1536,height:1536});
   await preset('gen-aspect','3:2');await change('gen-width',2048);assert.deepEqual(await size('gen'),{width:2064,height:1376});
   await page.locator('#gen-width').press('ArrowUp');await page.locator('#gen-width').press('Tab');assert.deepEqual(await size('gen'),{width:2112,height:1408},'Arrow increments advance to the next valid linked size');
   await change('gen-height',1600);assert.deepEqual(await size('gen'),{width:2400,height:1600});
   await preset('gen-aspect','16:9');await change('gen-width',3840);assert.deepEqual(await size('gen'),{width:3840,height:2160});
   assert.equal(await page.locator('#gen-run').isEnabled(),true,'3840×2160 is valid without an invented area budget');
   await change('gen-width',4096);assert.deepEqual(await size('gen'),{width:4096,height:2304});
   assert.equal(await page.locator('#gen-run').isEnabled(),true,'4096×2304 is valid without an invented area budget');
   assert.equal(await page.locator('#gen-width').getAttribute('max'),'16384');
   await reveal('gen-model');await page.locator('#gen-model').selectOption('z-image-turbo');assert.deepEqual(await size('gen'),{width:1024,height:576});
   assert.equal(await page.locator('#gen-width').getAttribute('max'),'1024');
   await change('gen-steps',99);assert.equal(await page.locator('#gen-steps').inputValue(),'20');
   await reveal('gen-model');await page.locator('#gen-model').selectOption('qwen');await reveal('gen-ratio-lock');await page.locator('#gen-ratio-lock').click();
   await change('gen-height',1024);await change('gen-width',4096);assert.deepEqual(await size('gen'),{width:4096,height:1024});
   await change('gen-height',4096);assert.deepEqual(await size('gen'),{width:4096,height:4096});
   await change('gen-width',0);valid(await size('gen'),connected);
   await change('gen-steps',0);assert.equal(await page.locator('#gen-steps').inputValue(),'1');
   const createSize=await size('gen');
   await page.locator('#file').setInputFiles({name:'Source 3x2.png',mimeType:'image/png',buffer:fixture});await ready('edit');
   assert.equal(await page.locator('#gen-ratio-lock').getAttribute('aria-pressed'),'true','Edit has its own default-linked state');
   assert.deepEqual(await size('gen'),{width:1536,height:1024});
   assert.equal(await page.locator('#gen-width').getAttribute('max'),null,'Reference workflow does not inherit a ceiling from an unused latent node');
   await change('gen-width',1920);assert.deepEqual(await size('gen'),{width:1920,height:1280});
   await page.locator('#generation-create-tab').click();await ready('create');
   assert.equal(await page.locator('#gen-ratio-lock').getAttribute('aria-pressed'),'false');assert.deepEqual(await size('gen'),createSize);
   await page.locator('#generation-edit-tab').click();await ready('edit');assert.deepEqual(await size('gen'),{width:1920,height:1280});
   await reveal('gen-prompt');await page.locator('#gen-prompt').fill('Change the red object to blue');
   await page.evaluate(()=>{document.getElementById('gen-width').value='99999';document.getElementById('gen-steps').value='999';generateImage();});
   await page.waitForFunction(()=>!busy&&!!session?.generation);
   valid(requests.at(-1),models[0].limits.reference_dimensions);assert.equal(requests.at(-1).steps,100);assert.equal(requests.at(-1).width/requests.at(-1).height,1.5);
   await page.locator('#draft-refine-open').click();await page.waitForFunction(()=>refineInitialized);
   await page.evaluate(()=>LocalImageGenerationStudio.selectStage('draft'));await reveal('refine-draft-model');await page.locator('#refine-draft-model').selectOption('z-image-turbo');
   await change('refine-draft-width',1536);assert.deepEqual(await size('refine-draft'),{width:1024,height:1024});
   await page.locator('#refine-draft-ratio-lock').click();await change('refine-draft-width',512);assert.deepEqual(await size('refine-draft'),{width:512,height:1024});
   await page.evaluate(()=>LocalImageGenerationStudio.selectStage('final'));
   await reveal('refine-final-model');await page.locator('#refine-final-model').selectOption('qwen');await preset('refine-aspect','16:9');await change('refine-final-width',4096);
   assert.deepEqual(await size('refine-final'),{width:4096,height:2304});assert.deepEqual(await size('refine-draft'),{width:512,height:1024});
   assert.equal(await page.locator('#refine-final-ratio-lock').getAttribute('aria-pressed'),'true');
   await reveal('refine-final-model');await page.locator('#refine-final-model').selectOption('flux2-klein-9b');assert.deepEqual(await size('refine-final'),{width:1024,height:576});
   await change('refine-final-steps',80);assert.equal(await page.locator('#refine-final-steps').inputValue(),'4');
   await page.screenshot({path:path.join(output,'linked-output.png'),animations:'disabled'});
   assert.deepEqual(errors,[]);
   console.log('PASS linked-size UI: linked/unlinked edits, aspect presets, model changes, steps, separate Edit/Create/Draft/Final state, source aspect and submission guard. GPU calls mocked; real image import.');
 } catch(error){await page.screenshot({path:path.join(output,'failure.png')});console.error('Browser errors:',errors);throw error;}
 finally{await context.unrouteAll({behavior:'ignoreErrors'});await browser.close();}
}
main().catch(error=>{console.error(error);process.exitCode=1;});
