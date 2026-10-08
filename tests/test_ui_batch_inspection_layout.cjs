// Real batch preparation from an editable cutout mask; no provider/model jobs or exports.
const assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path'),crypto=require('node:crypto');
const {chromium}=require('playwright'),{imageFixture}=require('./helpers/image-fixture.cjs');
const root=path.resolve(__dirname,'..'),base=process.env.LOCAL_REMOVE_TEST_URL,profile=process.env.MIGRATION_REAL_PROFILE;
assert.ok(base&&profile);assert.equal(new URL(base).hostname,'127.0.0.1');assert.ok(path.resolve(profile).startsWith(path.join(root,'qa-artifacts')+path.sep));
const fixtureName='Batch flow fixture '+ 'long-file-name-'.repeat(12)+'.png';
const output=path.join(root,'qa-artifacts/migration/batch-inspection-layout'),hash=value=>crypto.createHash('sha256').update(value).digest('hex');
(async()=>{
 const runtime=await(await fetch(base+'/api/local-remove/runtime')).json();assert.equal(path.resolve(runtime.data_root),path.resolve(profile));fs.mkdirSync(output,{recursive:true});
 const sourceFile=path.join(root,'frontend/src/features/batch/batch.css'),sourceHash=hash(fs.readFileSync(sourceFile)),browser=await chromium.launch({channel:process.env.PLAYWRIGHT_CHANNEL||(process.platform==='win32'?'msedge':undefined),headless:true}),context=await browser.newContext({viewport:{width:1366,height:768},deviceScaleFactor:1.75}),page=await context.newPage();
 const metrics=[],assets=[],errors=[],blocked=[],requests=[],reads=[];let passed=false,failure=null,queueId=null;
 await context.route('**/*',route=>{const request=route.request(),url=new URL(request.url());let allowed=request.method()==='GET';if(request.method()==='POST'){allowed=/^\/api\/local-remove\/import$/.test(url.pathname)||/^\/api\/local-remove\/session\/[^/]+\/(?:stack|cutout\/refine)$/.test(url.pathname);if(url.pathname==='/api/local-remove/batch/jobs'){const body=request.postDataJSON();allowed=body.prepare_cutouts===true&&!body.treatment_id;}}if(url.origin!==base||!allowed){blocked.push({method:request.method(),path:url.pathname});return route.abort('blockedbyclient');}return route.continue();});
 await context.addInitScript(()=>{for(const key of ['local-image.hardware-guide.v1','local-image.first-ai-setup.v1','local-image.first-task.v1'])localStorage.setItem(key,'1');window.__inspectionCsp=[];document.addEventListener('securitypolicyviolation',event=>window.__inspectionCsp.push(event.violatedDirective));});
 page.on('pageerror',error=>errors.push(error.stack));page.on('request',request=>{const url=new URL(request.url());if(url.pathname.startsWith('/api/'))requests.push({method:request.method(),path:url.pathname});});page.on('response',response=>{const url=new URL(response.url());if(url.pathname.startsWith('/frontend-assets/'))reads.push(response.body().then(body=>assets.push({path:url.pathname,status:response.status(),sha256:hash(body)})));});
 const settle=()=>page.evaluate(()=>new Promise(resolve=>requestAnimationFrame(()=>requestAnimationFrame(resolve))));
 try{
  await page.goto(base+'/remove');await page.waitForFunction(()=>document.body.dataset.reactReady==='true');const [chooser]=await Promise.all([page.waitForEvent('filechooser'),page.getByRole('button',{name:'Open image…',exact:true}).click()]);await chooser.setFiles({name:fixtureName,mimeType:'image/png',buffer:imageFixture(640,480)});await page.waitForFunction(()=>!!window.LocalImageEditor.getSnapshot().document&&!window.LocalImageEditor.getSnapshot().busy);
  await page.getByRole('button',{name:'Layer commands',exact:true}).click();await page.getByRole('menuitem',{name:'Add editable mask',exact:true}).click();await page.waitForFunction(()=>window.LocalImageEditor.getSnapshot().document.layer_stack.some(layer=>layer.kind==='cutout')&&!window.LocalImageEditor.getSnapshot().busy);
  await page.getByRole('button',{name:/^Settings(?:, update available)?$/,exact:true}).click();const settings=page.getByRole('dialog',{name:/^Settings(?:, update available)?$/,exact:true});await settings.getByRole('combobox',{name:'Interface size',exact:true}).selectOption('large');await settings.getByRole('button',{name:'Done',exact:true}).click();await settings.waitFor({state:'detached'});
  await page.locator('#workspace-cutout').click();await page.waitForFunction(()=>!window.LocalImageEditor.getSnapshot().busy);await page.getByRole('button',{name:'Remove backgrounds',exact:true}).click();await page.waitForFunction(()=>window.LocalImageReactFeatures.batch.getSnapshot().canPrepare);const [created]=await Promise.all([page.waitForResponse(response=>new URL(response.url()).pathname==='/api/local-remove/batch/jobs'&&response.request().method()==='POST'),page.locator('#batch-create').click()]);assert.equal(created.status(),200);const queue=await created.json();assert.equal(queue.prepare_cutouts,true);queueId=queue.id;
  await page.waitForFunction(()=>window.LocalImageReactFeatures.batch.getSnapshot().canExport&&!window.LocalImageReactFeatures.batch.getSnapshot().active.running);await page.getByRole('button',{name:'Inspect '+fixtureName+' at full size',exact:true}).click();await page.locator('#batch-inspect-image').evaluate(image=>image.decode());
  for(const [width,height]of [[800,560],[1366,768],[2195,1164]])for(const zoom of ['fit','200']){
   await page.setViewportSize({width,height});await page.locator('#batch-inspect-zoom').selectOption(zoom);await settle();
   const geometry=await page.locator('#batch-dialog').evaluate(dialog=>{
    const rect=selector=>{const element=dialog.querySelector(selector),box=element.getBoundingClientRect();return {x:box.x,y:box.y,width:box.width,height:box.height,bottom:box.bottom};};
    return {font:parseFloat(getComputedStyle(dialog).fontSize),surface:dialog.getBoundingClientRect().toJSON(),viewport:rect('#batch-inspect-scroll'),toolbar:rect('.li-batch-viewer-toolbar'),image:rect('#batch-inspect-image'),close:rect('#batch-inspect-back'),overflow:dialog.scrollWidth>dialog.clientWidth+1};
   });
   assert.equal(geometry.font,28);
   assert.equal(geometry.overflow,false);
   assert.ok(geometry.surface.x>=0&&geometry.surface.right<=width+1);
   assert.ok(geometry.surface.y>=0&&geometry.surface.bottom<=height+1);
   assert.ok(geometry.viewport.y>=geometry.toolbar.bottom,'Preview starts after controls');
   assert.ok(geometry.viewport.height>=120,'Preview retains usable space at Large interface size');
   assert.ok(geometry.viewport.bottom<=geometry.surface.bottom,'Image stays inside dialog');
   assert.ok(geometry.close.bottom<=height,'Close button is reachable');
   if(zoom==='200'){
    assert.equal(geometry.image.width,1280);
    const box=geometry.viewport, x=box.x+box.width/2,y=box.y+box.height/2;
    await page.mouse.move(x,y);await page.mouse.down();await page.mouse.move(x+60,y+35,{steps:4});await page.mouse.up();
    const after=await page.locator('#batch-inspect-image').boundingBox();
    if(1280>box.width)assert.ok(Math.abs(after.x-geometry.image.x)>20,'Pointer drag pans a zoomed image');
    await page.mouse.wheel(0,-160);await settle();
    assert.ok(await page.locator('#batch-inspect-image').evaluate(image=>parseFloat(image.style.width)>1280),'Wheel zoom changes image scale');
    await page.locator('#batch-inspect-scroll').focus();await page.keyboard.press('f');await settle();
    assert.equal(await page.locator('#batch-inspect-zoom').inputValue(),'fit');
   }
   metrics.push({width,height,zoom,...geometry});await page.screenshot({path:path.join(output,`inspection-${width}-${height}-${zoom}.png`),animations:'disabled'});

  }
  await page.keyboard.press('Escape');
  assert.equal(await page.locator('#batch-inspect-image').count(),0);
  assert.equal(await page.getByRole('dialog').count(),1);
  const thumbnail=page.getByRole('button',{name:'Inspect '+fixtureName+' at full size',exact:true});
  await thumbnail.waitFor();await page.waitForFunction(name=>document.activeElement?.getAttribute('aria-label')==='Inspect '+name+' at full size',fixtureName);assert.equal(await thumbnail.evaluate(button=>button===document.activeElement),true,'Closing restores focus to thumbnail');
  await thumbnail.click();await page.locator('#batch-inspect-back').click();assert.equal(await page.locator('#batch-inspect-image').count(),0);
  await Promise.all(reads);assert.equal(hash(fs.readFileSync(sourceFile)),sourceHash);assert.deepEqual(errors,[]);assert.deepEqual(blocked,[]);assert.deepEqual(await page.evaluate(()=>window.__inspectionCsp),[]);passed=true;
 }catch(error){failure=error.stack;await page.screenshot({path:path.join(output,'failure.png'),animations:'disabled'}).catch(()=>{});throw error;}
 finally{await Promise.allSettled(reads);fs.writeFileSync(path.join(output,'results.json'),JSON.stringify({passed,failure,runtime,queueId,sourceHash,metrics,assets,requests,errors,blocked,evidence:'Actual editable-mask backend batch with real full-size preview; Large28px at three viewport sizes and fit/200% image zoom; no model/native actions'},null,2));await browser.close();}
 console.log(`PASS Batch inspection flow: ${metrics.length} layouts; ${output}`);
})().catch(error=>{console.error(error);process.exitCode=1;});
