// Real Chromium coverage of guided setup. All network and native actions are fixtures;
// this test never installs software, downloads weights, or changes the user's config.
const fs=require('node:fs');
const path=require('node:path');
const assert=require('node:assert/strict');
const {chromium}=require('playwright');
const root=path.resolve(__dirname,'..');
const output=path.resolve(process.argv[2]||path.join(root,'..','setup-review'));
const testUrl=new URL('/remove',process.env.LOCAL_REMOVE_TEST_URL||'http://127.0.0.1:51247').href;
fs.mkdirSync(output,{recursive:true});
const html=fs.readFileSync(path.join(root,'backend/local_remove.html'),'utf8')
  .replace('__EDITOR_STYLE__',fs.readFileSync(path.join(root,'backend/frontend/editor.css'),'utf8'))
  .replace('__EDITOR_SCRIPT__',fs.readFileSync(path.join(root,'backend/frontend/editor.js'),'utf8'))
  .replaceAll('__APP_ICON__','data:image/png;base64,'+fs.readFileSync(path.join(root,'backend/frontend/app-icon.png')).toString('base64')).replaceAll('__NONCE__','test-nonce').replaceAll('__TOKEN__','test-token');
const candidate={id:'existing-comfy-1',name:'ComfyUI',path:'C:\\Photo tools\\ComfyUI',kind:'source',startable:true};
const fixture=()=>({installation:null,installations:[candidate],managed_directory:'C:\\Photo tools',install_directory:'C:\\Photo tools\\LocalRemove-ComfyUI',portable:{version:'test-release',download_bytes:3300000000,minimum_free_bytes:12884901888},storage:{portable_folder:{directory:'C:\\Photo tools',free_bytes:200000000000},model_folder:{directory:'D:\\Photo Models',free_bytes:450000000000}},model_directory:'',models:[
  {name:'flux-2-klein-base-4b.safetensors',folder:'diffusion_models',label:'FLUX.2 Klein',exists:false,bytes:0,expected_bytes:7751105712},
  {name:'qwen_3_4b.safetensors',folder:'text_encoders',label:'Text encoder',exists:false,bytes:0,expected_bytes:8044982048},
  {name:'flux2-vae.safetensors',folder:'vae',label:'Image decoder',exists:false,bytes:0,expected_bytes:336211292},
  {name:'flux-2-klein-object-remove.safetensors',folder:'loras',label:'Object removal adapter',exists:false,bytes:0,expected_bytes:76038936}],
  service:{running:false,ready:false,reason:null,starting:false,busy:false,device:null,port:8188,can_start:false,can_eject:false},job:null});
const errors=[];
async function createPage(browser,native,options={}){
  const page=await browser.newPage({viewport:{width:1366,height:768}});let current=fixture();if(options.mode)current.setup_mode=options.mode;
  page.on('pageerror',error=>errors.push(error.message));page.on('console',entry=>{if(entry.type()==='error')errors.push(entry.text());});
  if(native)await page.addInitScript(initial=>{
    window.__setupState=initial;window.__nativeCalls=[];const handlers=[];
    window.chrome=window.chrome||{};window.chrome.webview={addEventListener:(name,fn)=>handlers.push(fn),postMessage:message=>{
      window.__nativeCalls.push(message);const result=message.action==='ready'?{native:true,projects:true,setup:true}:window.__setupState;
      setTimeout(()=>handlers.forEach(fn=>fn({data:{type:'local-remove-native',id:message.id,result}})),0);
    }};
  },current);
  await page.route('**/*',async route=>{
    const address=new URL(route.request().url());let body;
    if(address.pathname==='/remove')return route.fulfill({contentType:'text/html',body:html});
    if(address.pathname==='/api/local-remove/settings')body={model:'klein',models:[{id:'klein',label:'FLUX.2 Klein',available:current.models.every(x=>x.exists)},{id:'heal',label:'Quick Heal',available:true,methods:[{id:'texture',available:true},{id:'telea',available:true}]}]};
    else if(address.pathname==='/api/local-remove/status')body={ready:current.service.running,retouch_ready:true};
    else if(address.pathname==='/api/local-remove/qwen/status')body={connected:false,ready:false,reason:'Start ComfyUI',variants:[{id:'int8',available:false},{id:'bf16',available:false}]};
    else if(address.pathname==='/api/local-remove/qwen/download')body={running:false,phase:'idle',message:'Choose a model to download.',variants:[{id:'int8',installed:false,missing_bytes:15000000000},{id:'bf16',installed:false,missing_bytes:29000000000}]};
    else if(address.pathname==='/api/local-remove/backgrounds')body={libraries:[]};
    else if(address.pathname==='/api/local-remove/sessions')body=[];
    else if(address.pathname==='/api/local-remove/hardware')body={devices:[],profiles:[],note:'Planning recommendations, not hard minimums.'};
    else if(address.pathname==='/api/local-remove/generator/download')body={running:false,phase:'idle',model_directory:current.model_directory,models:[]};
    else if(address.pathname==='/api/local-remove/generation/models')body={default_model:'qwen',models:[{id:'qwen',label:'Qwen Image 2.1',available:false,description:'Editing and transparent assets',capabilities:{text_to_image:true,image_reference:true,max_references:10,transparent:true,negative_prompt:true},variants:[{id:'int8',label:'INT8',available:false,total_bytes:15000000000,missing_bytes:15000000000}],defaults:{variant:'int8',width:1024,height:1024,steps:25,guidance:1},limits:{max_dimension:4096,dimension_step:32,max_pixels:4194304},hardware:{vram_recommendation:'16 GB recommended'}}]};
    else if(address.pathname.startsWith('/api/local-remove/setup'))body=current;
    else return route.fulfill({status:404,body:'Unknown test route: '+address.pathname});
    return route.fulfill({contentType:'application/json',body:JSON.stringify(body)});
  });
  await page.addInitScript(settings=>{if(settings.hardwareSeen!==false)localStorage.setItem('local-image.hardware-guide.v1','1');if(settings.setupSeen)localStorage.setItem('local-image.first-ai-setup.v1','1');},options);
  await page.goto(testUrl);
  await page.waitForFunction(()=>document.querySelector('#status').textContent.includes('Quick Heal ready'));
  if(options.manualSettings!==false){await page.locator('#edit-menu-trigger').click();await page.locator('#settings').click();await page.waitForFunction(()=>!document.querySelector('#ai-refresh').disabled);}
  async function setState(state){current=state;if(native)await page.evaluate(value=>window.__setupState=value,state);await page.locator('#ai-refresh').click();await page.waitForFunction(()=>!document.querySelector('#ai-refresh').disabled);}
  return {page,setState};
}
async function main(){
  const browser=await chromium.launch({headless:true,channel:process.env.BROWSER_CHANNEL||'msedge'});
  try{
    const {page,setState}=await createPage(browser,true);
    assert.equal(await page.locator('#ai-native-note').isVisible(),false);
    assert.equal(await page.locator('#model').isVisible(),false,'No unnecessary model selector');
    assert.equal(await page.locator('#model option').count(),1);
    assert.equal(await page.locator('.menu:visible').count(),0,'Settings opens after dismissing Edit');
    assert.equal(await page.locator('#ai-model-files').isVisible(),false,'Model file details start collapsed');
    assert.equal(await page.locator('#ai-install').isVisible(),false,'Portable install stays inside its own disclosure');
    assert.equal(await page.locator('#ai-download').isVisible(),false,'Optional FLUX download is not presented as required setup');
    assert.equal(await page.locator('#ai-install-path').textContent(),'C:\\Photo tools\\LocalRemove-ComfyUI','Actual portable target is shown before installing');
    assert.match(await page.locator('#ai-install-space').textContent(),/Runtime download:.*Allow 12.0 GB.*available/,'Runtime download and drive space are separate from model storage');
    assert.match(await page.locator('#ai-model-space').textContent(),/available on this drive/);
    const modelSummary=page.locator('#ai-flux-options > summary');
    await modelSummary.focus();await page.keyboard.press('Enter');
    assert.equal(await page.locator('#ai-model-files').isVisible(),true,'Model file details open from the keyboard');
    assert.equal(await page.locator('#ai-model-files li').count(),4,'All four FLUX dependencies remain available in details');
    await page.keyboard.press('Space');assert.equal(await page.locator('#ai-model-files').isVisible(),false,'Model details collapse from the keyboard');
    await page.screenshot({path:path.join(output,'setup-overview.png'),animations:'disabled'});
    await page.locator('#ai-portable-options > summary').click();await modelSummary.click();
    for(const [width,height]of [[1440,900],[1280,720],[1024,768],[800,560]]){
      await page.setViewportSize({width,height});await page.locator('#settings-dialog').evaluate(el=>el.scrollTop=0);
      assert.ok(await page.locator('#settings-dialog').evaluate(el=>el.scrollWidth<=el.clientWidth+1),'Settings has no horizontal overflow');
      for(const selector of ['#ai-install','#ai-choose-models','#ai-download','#ai-eject','#ask-before-overwrite']){
        await page.locator(selector).scrollIntoViewIfNeeded();const control=await page.locator(selector).boundingBox(),dialog=await page.locator('#settings-dialog').boundingBox();
        assert.ok(control.y>=dialog.y&&control.y+control.height<=dialog.y+dialog.height,selector+' remains reachable');
      }
      await page.locator('#settings-dialog').evaluate(el=>el.scrollTop=0);
      await page.screenshot({path:path.join(output,`setup-${width}.png`),animations:'disabled'});
    }
    await page.locator('#ai-detect').click();await page.locator('#ai-installation-choice').selectOption(candidate.id);
    await page.locator('#ai-use-installation').click();
    for(const selector of ['#ai-choose-runtime','#ai-choose-install','#ai-install','#ai-choose-models'])await page.locator(selector).click();
    let state=fixture();state.installation=candidate;state.model_directory='D:\\My Photo Models';state.service.can_start=true;await setState(state);
    await page.locator('#ai-download').click();await page.locator('#ai-start').click();
    state.service.running=true;state.service.ready=true;state.service.can_eject=true;state.service.device='NVIDIA test GPU';state.models.forEach(file=>{file.exists=true;file.bytes=file.expected_bytes;});await setState(state);
    await page.locator('#ai-eject').click();
    const calls=await page.evaluate(()=>window.__nativeCalls.filter(call=>call.action.startsWith('setup')));
    for(const action of ['setupUseInstallation','setupChooseComfyDirectory','setupChooseInstallDirectory','setupChooseModelDirectory','setupInstall','setupDownloadModels','setupStart','setupEject'])assert.ok(calls.some(call=>call.action===action),action+' uses native bridge');
    for(const call of calls)assert.deepEqual(Object.keys(call).sort(),(call.action==='setupUseInstallation'?['id','action','installation_id']:['id','action']).sort(),'Page sends no filesystem path');
    assert.equal(calls.find(call=>call.action==='setupUseInstallation').installation_id,candidate.id);
    state.service.ready=false;state.service.reason='The running ComfyUI cannot see the configured FLUX model folder.';await setState(state);
    assert.match(await page.locator('#ai-runtime-detail').textContent(),/cannot see the configured FLUX model folder/);assert.match(await page.locator('#ai-runtime-detail').textContent(),/close ComfyUI.*Start AI backend/);assert.doesNotMatch(await page.locator('#ai-setup-state').textContent(),/AI Remove is ready/);assert.equal(await page.locator('#ai-start').isDisabled(),true,'An existing backend is not restarted automatically');
    await page.locator('#ai-runtime-detail').scrollIntoViewIfNeeded();await page.screenshot({path:path.join(output,'setup-running-needs-restart.png'),animations:'disabled'});
    state.service.ready=true;state.service.reason=null;state.models[0].exists=false;state.models[0].bytes=0;state.job={id:'test-job',action:'download-models',status:'running',phase:'download',message:'Downloading the FLUX diffusion model…',progress:37,downloaded_bytes:3700000000,total_bytes:10000000000};await setState(state);
    assert.equal(await page.locator('#ai-job-progress').getAttribute('value'),'37');assert.equal(await page.locator('#ai-install').isDisabled(),true);assert.equal(await page.locator('#ai-eject').isDisabled(),true);
    await page.locator('#ai-job').scrollIntoViewIfNeeded();await page.screenshot({path:path.join(output,'setup-progress.png'),animations:'disabled'});
    state.job={...state.job,status:'error',message:'Download paused.',error:'There is not enough free space in the model folder.'};await setState(state);
    assert.match(await page.locator('#ai-job-message').textContent(),/not enough free space/);await page.locator('#ai-job').scrollIntoViewIfNeeded();await page.screenshot({path:path.join(output,'setup-error.png'),animations:'disabled'});
    state.job=null;state.service.qwen_ready=true;state.service.flux_ready=false;await setState(state);
    assert.match(await page.locator('#ai-setup-state').textContent(),/Qwen Image 2.1 is ready/,'A Qwen-only installation is ready without downloading FLUX');
    state.service.qwen_ready=false;state.service.z_image_ready=true;await setState(state);assert.match(await page.locator('#ai-setup-state').textContent(),/Local AI is ready/,'A Z-Image-only installation is ready without FLUX or Qwen');assert.doesNotMatch(await page.locator('#ai-setup-state').textContent(),/Download FLUX|Choose a folder for the FLUX/);
    state.storage.model_folder={directory:'D:\\My Photo Models',error:'The selected drive is unavailable.'};await setState(state);assert.match(await page.locator('#ai-model-space').textContent(),/selected drive is unavailable/,'Storage errors are shown beside the selected folder');
    await page.keyboard.press('Escape');assert.equal(await page.locator('#settings-dialog').isVisible(),false);
    assert.equal(await page.evaluate(()=>document.activeElement!==document.body&&document.activeElement.getClientRects().length>0&&!document.activeElement.closest('.menu')),true,'Closing Settings restores focus to a visible workspace control');
    await page.locator('#edit-menu-trigger').click();await page.locator('#settings').click();await page.waitForFunction(()=>!document.querySelector('#ai-refresh').disabled);
    await page.locator('#ai-browse-models').click();await page.waitForFunction(()=>document.querySelectorAll('#model-browser-list button').length===1);assert.equal(await page.locator('#settings-dialog').isVisible(),false,'Browse models leaves Settings instead of stacking dialogs');assert.equal(await page.locator('#model-browser-dialog').isVisible(),true);assert.match(await page.locator('#model-browser-memory').textContent(),/16 GB/,'Setup leads directly to model and VRAM comparison');await page.locator('#model-browser-close').click();
    await page.locator('#workspace-retouch').click();await page.locator('#edit-menu-trigger').click();await page.locator('#settings').click();await page.waitForFunction(()=>!document.querySelector('#ai-refresh').disabled);await page.locator('#settings-close').focus();
    const focusState=()=>page.evaluate(()=>{
      const dialog=document.querySelector('#settings-dialog'),active=document.activeElement,closed=active.closest('details:not([open])');
      return {index:[...dialog.querySelectorAll('*')].indexOf(active),valid:!!active.closest('#settings-dialog')&&!active.disabled&&active.getClientRects().length>0&&(!closed||closed.querySelector(':scope > summary')===active),modelSummary:active.matches('.setup-components > summary')};
    });
    const first=await focusState(),visited=[];let cycled=false,sawModelSummary=false;
    for(let guard=0;guard<80;guard++){
      const current=await focusState();assert.equal(current.valid,true,'Keyboard focus stays on a visible enabled Settings control');
      visited.push(current.index);sawModelSummary||=current.modelSummary;
      await page.keyboard.press('Tab');const next=await focusState();
      if(next.index===first.index){cycled=true;break;}
      assert.ok(!visited.includes(next.index),'Focus visits each available control before wrapping');
    }
    assert.equal(cycled,true,'Tab completes a full modal focus cycle');assert.ok(visited.length>5,'The cycle reaches the settings controls');assert.equal(sawModelSummary,true,'Collapsed Model files remains keyboard reachable');
    await page.keyboard.press('Shift+Tab');assert.equal((await focusState()).index,visited.at(-1),'Shift+Tab wraps to the last available control');
    await page.locator('#settings-close').click();await page.locator('#workspace-cutout').click();
    await page.locator('.qwen-download-details > summary').click();
    await page.waitForFunction(()=>document.querySelector('#qwen-download').textContent.includes('GB'));
    await page.locator('#qwen-variant').selectOption('bf16');await page.locator('#qwen-download').click();
    const qwenRequest=await page.evaluate(()=>window.__nativeCalls.find(call=>call.action==='setupDownloadQwen'));
    assert.equal(qwenRequest.variant,'bf16','Download requests carry the explicitly selected model variant');
    assert.deepEqual(Object.keys(qwenRequest).sort(),['action','id','variant'],'Qwen download sends no page-supplied path or URL');
    await page.close();
    const browserOnly=await createPage(browser,false);assert.equal(await browserOnly.page.locator('#ai-native-note').isVisible(),true);for(const id of ['ai-install','ai-choose-install','ai-start','ai-download','ai-eject','ai-choose-models'])assert.equal(await browserOnly.page.locator('#'+id).isDisabled(),true);
    await browserOnly.page.screenshot({path:path.join(output,'setup-browser-only.png'),animations:'disabled'});await browserOnly.page.close();
    const portable=await createPage(browser,true,{mode:'portable',hardwareSeen:false,manualSettings:false});await portable.page.waitForFunction(()=>document.querySelector('#hardware-dialog').open);assert.equal(await portable.page.locator('#settings-dialog').isVisible(),false,'Hardware guidance appears before first AI setup');await portable.page.locator('#hardware-continue').click();await portable.page.waitForFunction(()=>document.querySelector('#settings-dialog').open&&!setupRefreshRunning);assert.equal(await portable.page.locator('#ai-portable-options').getAttribute('open'),'','Installer portable choice opens its controls');assert.equal(await portable.page.locator('#ai-installations').isVisible(),false);assert.equal(await portable.page.evaluate(()=>window.__nativeCalls.filter(call=>call.action!=='ready').length),0,'First setup never installs, starts or downloads automatically');await portable.page.screenshot({path:path.join(output,'setup-first-portable.png'),animations:'disabled'});await portable.page.locator('#settings-close').click();await portable.page.reload();await portable.page.waitForFunction(()=>setupState&&!setupRefreshRunning);await portable.page.waitForTimeout(150);assert.equal(await portable.page.locator('#settings-dialog').isVisible(),false,'The first setup does not recur after relaunch');await portable.page.close();
    const discovered=await createPage(browser,true,{mode:'discover',manualSettings:false});await discovered.page.waitForFunction(()=>document.querySelector('#settings-dialog').open&&!setupRefreshRunning);assert.equal(await discovered.page.locator('#ai-installations').isVisible(),true,'Installer discovery choice shows existing installations');assert.equal(await discovered.page.locator('#ai-portable-options').getAttribute('open'),null);await discovered.page.screenshot({path:path.join(output,'setup-first-discover.png'),animations:'disabled'});await discovered.page.close();
    for(const options of [{mode:'later'},{},{mode:'portable',setupSeen:true}]){const deferred=await createPage(browser,true,{...options,manualSettings:false});await deferred.page.waitForFunction(()=>setupState&&!setupRefreshRunning);await deferred.page.waitForTimeout(150);assert.equal(await deferred.page.locator('#settings-dialog').isVisible(),false,'Legacy, deferred and acknowledged setup do not interrupt the workspace');await deferred.page.close();}
    assert.deepEqual(errors,[],'No runtime or console errors');
    console.log('PASS: four setup sizes; flat existing/portable/model folder choices; eight native actions with no page paths; model comparison; disk hints and errors; hardware-first installer setup shown once; discovery/portable/later/legacy behavior; keyboard cycle; progress lockout; browser-only guidance.');
  }finally{await browser.close();}
}
main().catch(error=>{console.error(error);process.exitCode=1;});
