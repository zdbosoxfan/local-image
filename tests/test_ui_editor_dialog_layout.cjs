// Production dialog footer/exit presentation. The close decision is always
// Cancel; no native host, save, discard, provider or model action is invoked.
const assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path'),crypto=require('node:crypto');
const {chromium}=require('playwright'),{imageFixture}=require('./helpers/image-fixture.cjs');
const root=path.resolve(__dirname,'..'),base=process.env.LOCAL_REMOVE_TEST_URL,profile=process.env.MIGRATION_REAL_PROFILE;
assert.ok(base&&profile);assert.equal(new URL(base).hostname,'127.0.0.1');assert.ok(path.resolve(profile).startsWith(path.join(root,'qa-artifacts')+path.sep));
const output=path.join(root,'qa-artifacts/migration/editor-dialogs'),hash=value=>crypto.createHash('sha256').update(value).digest('hex');
const sourceFiles=['frontend/src/features/shell/shell.css','frontend/src/features/shell/EditorDialogs.tsx','frontend/src/features/settings/SettingsDialogs.tsx','frontend/src/features/models/ModelDialogs.tsx','backend/frontend_dist/.vite/manifest.json'];
const sources=()=>sourceFiles.map(name=>({path:name,sha256:hash(fs.readFileSync(path.join(root,name)))}));
(async()=>{
 const runtime=await(await fetch(base+'/api/local-remove/runtime')).json();assert.equal(path.resolve(runtime.data_root),path.resolve(profile));fs.mkdirSync(output,{recursive:true});
 if(fs.existsSync(path.join(output,'results.json'))){const archive=path.join(output,'report-history');fs.mkdirSync(archive,{recursive:true});fs.copyFileSync(path.join(output,'results.json'),path.join(archive,`results-${Date.now()}.json`));}
 const before=sources(),browser=await chromium.launch({channel:'msedge',headless:true}),context=await browser.newContext({viewport:{width:1366,height:768},reducedMotion:'no-preference'}),page=await context.newPage();
 const metrics=[],transitions=[],assets=[],errors=[],blocked=[],requests=[],reads=[];let passed=false,failure=null,sessionId=null;
 await context.route('**/*',route=>{const request=route.request(),url=new URL(request.url()),allowedWrite=request.method()==='POST'&&(/^\/api\/local-remove\/import$/.test(url.pathname)||/^\/api\/local-remove\/session\/[^/]+\/stack(?:\/layers)?$/.test(url.pathname));if(url.origin!==base||request.method()!=='GET'&&!allowedWrite){blocked.push({method:request.method(),path:url.pathname});return route.abort('blockedbyclient');}return route.continue();});
 await context.addInitScript(()=>{for(const key of ['local-image.hardware-guide.v1','local-image.first-ai-setup.v1','local-image.first-task.v1'])localStorage.setItem(key,'1');window.__dialogCsp=[];document.addEventListener('securitypolicyviolation',event=>window.__dialogCsp.push(event.violatedDirective));});
 page.on('pageerror',error=>errors.push(error.stack));page.on('request',request=>{const url=new URL(request.url());if(url.pathname.startsWith('/api/'))requests.push({method:request.method(),path:url.pathname});});page.on('response',response=>{const url=new URL(response.url());if(url.pathname.startsWith('/frontend-assets/'))reads.push(response.body().then(body=>assets.push({path:url.pathname,status:response.status(),sha256:hash(body)})));});
 const settle=()=>page.evaluate(async()=>{await Promise.all(document.getAnimations().filter(animation=>animation.effect?.getComputedTiming().iterations!==Infinity).map(animation=>animation.finished.catch(()=>{})));await new Promise(resolve=>requestAnimationFrame(()=>requestAnimationFrame(resolve)));});
 const idle=()=>page.waitForFunction(()=>!window.LocalImageEditor.getSnapshot().busy);
 async function exitCheck(name,selector,kind,sentinel,action){
  await settle();
  await page.evaluate(({selector,kind})=>{
   const surface=document.querySelector(selector),records=[];window.__dialogExit={done:false,records};
   const sample=()=>{if(!surface.isConnected)return;records.push({label:surface.getAttribute('aria-label'),text:surface.textContent,closed:kind==='editor'?!window.LocalImageEditor.getSnapshot().closeInProgress:!window.LocalImageReactFeatures[kind].isOpen()});};
   const observer=new MutationObserver(sample);observer.observe(surface,{attributes:true,childList:true,subtree:true,characterData:true});
   const frame=()=>{if(surface.isConnected){sample();requestAnimationFrame(frame);}else{observer.disconnect();window.__dialogExit.done=true;}};frame();
  },{selector,kind});
  await action();await page.waitForFunction(()=>window.__dialogExit.done);
  const trace=await page.evaluate(()=>window.__dialogExit.records);transitions.push({name,trace});
  assert.ok(trace.some(item=>item.closed),name+' preserves rendered exit while controller is already closed');
  assert.ok(trace.every(item=>item.label===name&&item.text.includes(sentinel)),name+' keeps its last open title/content throughout exit');
 }
 async function closeGeometry(density){
  await page.getByRole('button',{name:'Close image',exact:true}).click();const dialog=page.getByRole('alertdialog',{name:'Close image?',exact:true});await dialog.waitFor();await settle();
  const value=await dialog.evaluate(surface=>{const actions=surface.querySelector('.fui-DialogActions'),body=surface.querySelector('.fui-DialogBody'),rect=node=>{const box=node.getBoundingClientRect();return{left:box.left,right:box.right,top:box.top,bottom:box.bottom,width:box.width,height:box.height};};return{surface:rect(surface),body:rect(body),actions:rect(actions),buttons:[...actions.querySelectorAll('button')].map(button=>{const style=getComputedStyle(button),range=document.createRange();range.selectNodeContents(button);const text=range.getBoundingClientRect();return{text:button.textContent,box:rect(button),line:parseFloat(style.lineHeight),padding:parseFloat(style.paddingTop)+parseFloat(style.paddingBottom),range:{top:text.top,bottom:text.bottom,left:text.left,right:text.right}};})};});
  metrics.push({density,...value});assert.equal(value.buttons.length,3);assert.ok(Math.abs(value.actions.width-value.body.width)<1,'Footer spans all dialog grid columns');
  for(const button of value.buttons){assert.ok(button.box.left>=value.body.left-1&&button.box.right<=value.body.right+1);assert.ok(button.range.top>=button.box.top-1&&button.range.bottom<=button.box.bottom+1,'All button labels remain visible');if(density==='comfortable')assert.ok(button.box.height<=button.line+button.padding+3,'Comfortable footer labels stay on one line');}
  for(let a=0;a<value.buttons.length;a++)for(let b=a+1;b<value.buttons.length;b++){const x=value.buttons[a].box,y=value.buttons[b].box;assert.ok(x.right<=y.left||y.right<=x.left||x.bottom<=y.top||y.bottom<=x.top,'Wrapped action controls do not overlap');}
  await page.screenshot({path:path.join(output,`close-${density}.png`),animations:'disabled'});
  await exitCheck('Close image?','.li-editor-dialog[role="alertdialog"]','editor','Save an editable project to keep original images and layers.',()=>dialog.getByRole('button',{name:'Cancel',exact:true}).click());
  assert.equal(await page.evaluate(()=>window.LocalImageEditor.getSnapshot().document.id),sessionId);await idle();
 }
 try{
  await page.goto(base+'/remove');await page.waitForFunction(()=>document.body.dataset.reactReady==='true');
  const [chooser]=await Promise.all([page.waitForEvent('filechooser'),page.getByRole('button',{name:'Open image…',exact:true}).click()]);await chooser.setFiles({name:'Dialog layout fixture.png',mimeType:'image/png',buffer:imageFixture(320,240)});
  await page.waitForFunction(()=>!!window.LocalImageEditor.getSnapshot().document&&!window.LocalImageEditor.getSnapshot().busy);sessionId=await page.evaluate(()=>window.LocalImageEditor.getSnapshot().document.id);
  await page.getByRole('button',{name:'New retouch layer',exact:true}).click();await page.waitForFunction(()=>window.LocalImageEditor.getSnapshot().document.layer_stack.length===2&&!window.LocalImageEditor.getSnapshot().busy);
  await closeGeometry('comfortable');
  await page.getByRole('button',{name:'Settings',exact:true}).click();const settings=page.getByRole('dialog',{name:'Settings',exact:true});await settings.getByRole('combobox',{name:'Interface size',exact:true}).selectOption('large');await settings.getByRole('button',{name:'Done',exact:true}).click();await settings.waitFor({state:'detached'});
  await closeGeometry('large');
  await page.getByRole('button',{name:'Settings',exact:true}).click();await settings.getByRole('button',{name:'Hardware guide',exact:true}).click();const hardware=page.getByRole('dialog',{name:'Hardware guide',exact:true});await hardware.waitFor();await page.waitForFunction(()=>!window.LocalImageReactFeatures.settings.getSnapshot().loading);
  await exitCheck('Hardware guide','.li-settings-surface[role="dialog"]','settings','Quick Heal and compositing run on the CPU.',()=>hardware.getByRole('button',{name:'Close hardware guide',exact:true}).click());
  await page.getByRole('button',{name:'Settings',exact:true}).click();await settings.getByRole('tab',{name:'Local AI',exact:true}).click();await page.waitForFunction(()=>!window.LocalImageReactFeatures.settings.getSnapshot().loading);await settings.getByRole('button',{name:'Model details…',exact:true}).click();const models=page.getByRole('dialog',{name:'Local image models',exact:true});await models.waitFor();await page.waitForFunction(()=>!window.LocalImageReactFeatures.models.getSnapshot().loading);
  await exitCheck('Local image models','.li-models-surface[role="dialog"]','models','Supported local models',()=>models.getByRole('button',{name:'Done',exact:true}).click());
  // Controlled draft port exercises the real LoRA view without enabling a
  // backend model or querying a remote provider; local installed inventory only.
  await page.evaluate(()=>window.LocalImageReactFeatures.models.openLoras({contextId:'create',read:()=>({modelId:'qwen',modelLabel:'Controlled Qwen draft',selected:[],referenceCount:0,supportsLoras:true}),setSelected(){},appendPrompt(){},applySampling(){}}));
  const loras=page.getByRole('dialog',{name:'LoRA library',exact:true});await loras.waitFor();await page.waitForFunction(()=>!window.LocalImageReactFeatures.models.getSnapshot().loading);
  await exitCheck('LoRA library','.li-models-surface[role="dialog"]','models','Installed',()=>loras.getByRole('button',{name:'Done',exact:true}).click());
  await Promise.all(reads);assert.deepEqual(sources(),before);assert.deepEqual(errors,[]);assert.deepEqual(blocked,[]);assert.deepEqual(await page.evaluate(()=>window.__dialogCsp),[]);passed=true;
 }catch(error){failure=error.stack;await page.screenshot({path:path.join(output,'failure.png'),animations:'disabled'}).catch(()=>{});throw error;}
 finally{await Promise.allSettled(reads);fs.writeFileSync(path.join(output,'results.json'),JSON.stringify({passed,failure,runtime,sessionId,sources:before,assets,metrics,transitions,requests,errors,blocked,evidence:'Production rendered dialogs; actual imported QA image/empty retouch layer; all close decisions canceled; controlled LoRA draft port; no native/provider/model actions'},null,2));await browser.close();}
 console.log(`PASS editor dialogs: ${metrics.length} geometry states, ${transitions.length} rendered exits; ${output}`);
})().catch(error=>{console.error(error);process.exitCode=1;});
