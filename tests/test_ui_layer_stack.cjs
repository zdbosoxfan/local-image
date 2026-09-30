// Uses an isolated backend. Repairs, stack composition, mask edits, project
// persistence and PNG exports are real; no GPU/model/stock network work occurs.
const {chromium}=require('playwright');
const assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path');
const base=process.env.LOCAL_REMOVE_TEST_URL||'http://127.0.0.1:51274';
const out=path.resolve(process.argv[2]||'qa-artifacts/ui-redesign/layer-checks');
fs.mkdirSync(out,{recursive:true});
(async()=>{
 const runtime=await(await fetch(base+'/api/local-remove/runtime')).json();
 assert.match(runtime.data_root.replaceAll('\\','/'),/\/qa-artifacts\/ui-redesign\/layer-test-profile$/i,'Layer suite may only use its isolated test profile');
 const browser=await chromium.launch({headless:true,channel:'msedge'}),page=await browser.newPage({viewport:{width:1600,height:1000},acceptDownloads:true});
 const errors=[],checks=[];page.on('pageerror',error=>errors.push(error.message));
 const record=name=>{checks.push(name);console.log('PASS: '+name);};
 const idle=()=>page.waitForFunction(()=>session&&!busy&&settingsLoaded);
 async function rect(x1,y1,x2,y2){await page.locator('[data-tool=rectangle]').click();const b=await page.locator('#photo').boundingBox();await page.mouse.move(b.x+b.width*x1,b.y+b.height*y1);await page.mouse.down();await page.mouse.move(b.x+b.width*x2,b.y+b.height*y2,{steps:5});await page.mouse.up();await page.waitForFunction(()=>hasSelection&&!busy);}
 try{
  await page.addInitScript(()=>{for(const name of ['local-image.hardware-guide.v1','local-image.first-ai-setup.v1','local-image.first-task.v1'])localStorage.setItem(name,'1');});
  await page.goto(base+'/remove');await page.waitForFunction(()=>settingsLoaded&&initCompleted&&window.LocalImageLayers);
  const png=await page.evaluate(()=>{const c=document.createElement('canvas');c.width=640;c.height=480;const g=c.getContext('2d');g.fillStyle='#529180';g.fillRect(0,0,640,480);g.fillStyle='#d44d42';g.fillRect(260,150,120,180);g.fillStyle='#ffe18c';g.fillRect(40,40,14,14);g.fillRect(80,80,14,14);return c.toDataURL('image/png').split(',')[1];});
  await page.locator('#file').setInputFiles({name:'Layer workflow.png',mimeType:'image/png',buffer:Buffer.from(png,'base64')});await idle();
  assert.equal(await page.evaluate(()=>isDirty()),false,'Opening an untouched image does not mark it modified');
  assert.equal(await page.locator('[data-stack-id]').count(),1);assert.equal(await page.locator('[data-stack-id][aria-selected=true] .layer-name').textContent(),'Original');
  await page.locator('#workspace-cutout').click();
  for(const selector of ['.toolrail [data-tool=brush]','.toolrail [data-tool=pen]','#cutout-modes','#cutout-panel'])assert.equal(await page.locator(selector).isVisible(),false,selector+' is not exposed before a mask exists');
  assert.equal(await page.locator('#cutout-remove').isVisible(),true);assert.equal(await page.locator('#retouch-panel').isVisible(),true);
  await page.screenshot({path:path.join(out,'01-cutout-before-mask.png')});record('Cutout starts with a source layer and one Remove background action; no premature mask tools');
  await page.locator('#workspace-retouch').click();await page.locator('#layer-add').click();await idle();
  const target=await page.evaluate(()=>LocalImageLayers.selected().id);assert.equal(await page.evaluate(()=>LocalImageLayers.selected().kind),'retouch');
  await page.locator('#heal-brush').click();await rect(.055,.07,.105,.135);await page.locator('#remove').click();await page.waitForFunction(()=>!busy&&LocalImageLayers.selected()?.patch_ids?.length===1);
  await rect(.12,.155,.16,.205);await page.locator('#remove').click();await page.waitForFunction(()=>!busy&&LocalImageLayers.selected()?.patch_ids?.length===2);
  assert.equal(await page.locator('[data-stack-id]').count(),2);assert.equal(await page.evaluate(()=>LocalImageLayers.selected().id),target);record('Two real CPU Quick Heals accumulate in the selected retouch layer');
  await page.locator('[data-stack-id="'+target+'"] .layer-name').dblclick();await page.locator('.stack-layer-rename').fill('Dust cleanup');await page.locator('.stack-layer-rename').press('Enter');await idle();
  await page.waitForFunction(()=>LocalImageLayers.selected()?.name==='Dust cleanup');
  await page.locator('#layer-opacity').fill('55');await page.locator('#layer-opacity').press('Tab');await idle();await page.waitForFunction(()=>LocalImageLayers.selected()?.opacity===.55);
  await page.locator('[data-stack-id="'+target+'"] .visibility').click();await idle();assert.equal(await page.evaluate(()=>LocalImageLayers.selected().visible),false);await page.locator('[data-stack-id="'+target+'"] .visibility').click();await idle();
  record('Layer selection, rename, opacity and visibility persist through backend mutations');
  // A manually-created alpha mask tests the same editable cutout stack without
  // spending GPU inference time. Its content is a rectangular synthetic object.
  await page.evaluate(async()=>{const c=document.createElement('canvas');c.width=session.width;c.height=session.height;const g=c.getContext('2d');g.fillStyle='black';g.fillRect(0,0,c.width,c.height);g.fillStyle='white';g.fillRect(260,150,120,180);const data=await json(url('/cutout/refine'),{revision:session.revision,mask:c.toDataURL('image/png').split(',')[1],operation:'replace'});await openSession(data);setWorkspace('cutout');});await idle();
  const cutout=await page.evaluate(()=>LocalImageLayers.nodes().find(n=>n.kind==='cutout').id);await page.locator('[data-stack-id="'+cutout+'"] .layer-content').click();
  assert.equal(await page.locator('.toolrail [data-tool=brush]').isVisible(),true);assert.equal(await page.locator('#cutout-panel').isVisible(),false);
  const original=await page.evaluate(()=>LocalImageLayers.nodes().find(n=>n.kind==='original').id);await page.locator('[data-stack-id="'+original+'"] .visibility').click();await idle();
  assert.equal(await page.evaluate(()=>LocalImageLayers.selected().id),cutout,'Toggling Original visibility keeps the cutout selected');assert.equal(await page.locator('#move-subject').isEnabled(),true);assert.equal(await page.locator('.toolrail [data-tool=brush]').isVisible(),true);
  await page.locator('[data-stack-id="'+original+'"] .stack-lock').focus();await page.keyboard.press('Space');await idle();assert.equal(await page.evaluate(()=>LocalImageLayers.selected().id),cutout,'Keyboard lock toggle on another row preserves selection');assert.equal(await page.evaluate(()=>LocalImageLayers.nodes().find(n=>n.kind==='original').locked),false);await page.locator('[data-stack-id="'+original+'"] .stack-lock').click();await idle();
  await page.locator('[data-stack-id="'+target+'"] .visibility').click();await idle();await page.locator('[data-stack-id="'+cutout+'"] .layer-content').click();
  await page.locator('#move-subject').click();let b=await page.locator('#photo').boundingBox();await page.mouse.move(b.x+b.width*.5,b.y+b.height*.5);await page.mouse.down();await page.mouse.move(b.x+b.width*.62,b.y+b.height*.55,{steps:8});await page.mouse.up();await idle();
  await page.waitForFunction(()=>LocalImageLayers.selected()?.transform.offset_x>50);
  const t=await page.evaluate(()=>LocalImageLayers.selected().transform);assert.ok(t.offset_y>10);record('Cutout is independent of Original; Move tool drags the selected layer on the actual stack');
  await page.locator('#stack-transform-scale').fill('120');await page.locator('#stack-transform-scale').press('Tab');await idle();await page.waitForFunction(()=>LocalImageLayers.selected().transform.scale===1.2);
  await page.locator('#stack-transform-rotation').fill('12');await page.locator('#stack-transform-rotation').press('Tab');await idle();await page.waitForFunction(()=>LocalImageLayers.selected().transform.rotation===12);
  await page.screenshot({path:path.join(out,'02-layers-cutout-move.png')});
  await page.locator('[data-tool=rectangle]').click();await rect(.58,.46,.63,.54);await page.locator('#cutout-refine').click();await idle();await page.waitForFunction(()=>!hasSelection);
  record('Transformed cutout mask refinement commits to the selected mask layer');
  await page.locator('#stack-background').click();assert.equal(await page.locator('#stack-background-menu').isVisible(),true);
  for(const name of ['import','assets','generate','folder'])assert.equal(await page.locator('#stack-background-'+name).isVisible(),true);
  await page.keyboard.press('Escape');await page.locator('#studio-assets-folders').click();assert.equal(await page.locator('#stack-background-folders').isVisible(),true);assert.equal(await page.locator('#stock-dialog').isVisible(),false);
  await page.locator('#studio-assets-stock').click();assert.equal(await page.locator('#stack-background-folders').isVisible(),false);record('Background commands are one menu; folders reuse the Assets dock');
  const downloadWait=page.waitForEvent('download');await page.locator('#studio-export').click();const download=await downloadWait;await download.saveAs(path.join(out,'layer-stack-real-export.png'));
  const exported=fs.readFileSync(path.join(out,'layer-stack-real-export.png'));assert.equal(exported.subarray(0,8).toString('hex'),'89504e470d0a1a0a');
  const pixel=await page.evaluate(async()=>{const image=await loadImage(url('/preview?full=true&r='+session.revision)),c=document.createElement('canvas');c.width=session.width;c.height=session.height;const g=c.getContext('2d');g.drawImage(image,0,0);return [...g.getImageData(0,0,1,1).data];});assert.equal(pixel[3],0,'Hiding Original preserves transparent export background');record('Real PNG export uses stack visibility and alpha');
  await page.locator('#move-subject').click();
  const handle=await page.evaluate(()=>{const n=LocalImageLayers.selected(),t=n.transform,b=n.bounds,a=t.rotation*Math.PI/180,cx=(session.width-1)/2,cy=(session.height-1)/2,x=(b[2]-cx)*t.scale,y=(b[3]-cy)*t.scale,point={x:cx+t.offset_x+Math.cos(a)*x-Math.sin(a)*y,y:cy+t.offset_y+Math.sin(a)*x+Math.cos(a)*y},rect=$('photo').getBoundingClientRect();return{x:rect.left+point.x/session.width*rect.width,y:rect.top+point.y/session.height*rect.height,scale:t.scale};});
  await page.mouse.move(handle.x,handle.y);await page.mouse.down();await page.mouse.move(handle.x+24,handle.y+24,{steps:8});await page.mouse.up();await idle();assert.ok(await page.evaluate(scale=>LocalImageLayers.selected().transform.scale>scale,handle.scale));record('Corner handles resize the selected layer on canvas');
  const afterScale=await page.evaluate(()=>LocalImageLayers.selected().transform.scale);await page.locator('#studio-undo').click();await idle();assert.ok(await page.evaluate(scale=>LocalImageLayers.selected().transform.scale<scale,afterScale));await page.locator('#studio-redo').click();await idle();assert.equal(await page.evaluate(()=>LocalImageLayers.selected().transform.scale),afterScale);record('Toolbar undo and redo restore layer transforms');
  const blue=await page.evaluate(()=>{const c=document.createElement('canvas');c.width=640;c.height=480;const g=c.getContext('2d');g.fillStyle='#385caa';g.fillRect(0,0,c.width,c.height);return c.toDataURL('image/png').split(',')[1];});
  await page.locator('#background-file').setInputFiles({name:'Blue background.png',mimeType:'image/png',buffer:Buffer.from(blue,'base64')});await idle();await page.waitForFunction(()=>LocalImageLayers.selected()?.kind==='image');
  const bg=await page.evaluate(()=>LocalImageLayers.selected().id);assert.ok(await page.evaluate(({bg,cutout})=>LocalImageLayers.nodes().findIndex(n=>n.id===bg)<LocalImageLayers.nodes().findIndex(n=>n.id===cutout),{bg,cutout}));record('Imported background becomes a normal image layer below the selected cutout');
  await page.locator('#layer-actions').click();await page.locator('#stack-mask').click();await idle();await page.waitForFunction(()=>LocalImageLayers.selected()?.kind==='cutout');
  const secondCutout=await page.evaluate(()=>LocalImageLayers.selected().id);assert.notEqual(secondCutout,cutout);
  await page.locator('#stack-mask-options>summary').click();await page.locator('#cutout-feather-value').fill('9');await page.locator('#cutout-feather-value').press('Tab');await idle();
  await page.locator('#shadow-disclosure>summary').click();await page.locator('#shadow-enabled').check();await idle();await page.locator('#shadow-opacity-value').fill('63');await page.locator('#shadow-opacity-value').press('Tab');await idle();
  await page.locator('[data-stack-id="'+cutout+'"] .layer-content').click();assert.equal(await page.locator('#cutout-feather-value').inputValue(),'0');assert.equal(await page.locator('#shadow-enabled').isChecked(),false);
  await page.locator('[data-stack-id="'+secondCutout+'"] .layer-content').click();assert.equal(await page.locator('#cutout-feather-value').inputValue(),'9');assert.equal(await page.locator('#shadow-enabled').isChecked(),true);assert.equal(await page.locator('#shadow-opacity-value').inputValue(),'63');
  await page.locator('#stack-mask-options>summary').click();record('Add mask works without a GPU; selecting masks restores each layer’s feather and shadow settings');
  await page.locator('[data-stack-id="'+secondCutout+'"] .stack-lock').click();await idle();assert.equal(await page.locator('#cutout-refine').isEnabled(),false);assert.equal(await page.locator('#layer-opacity').isEnabled(),false);await page.locator('[data-stack-id="'+secondCutout+'"] .stack-lock').click();await idle();
  await page.locator('#layer-actions').click();await page.locator('#stack-down').click();await idle();const newIndex=await page.evaluate(id=>LocalImageLayers.nodes().findIndex(n=>n.id===id),secondCutout);assert.ok(newIndex<4);record('Layer locks and stack ordering are editable');
  const projectWait=page.waitForEvent('download');await page.locator('#file-menu-trigger').click();await page.locator('#save-project').click();const project=await projectWait;const projectPath=path.join(out,'layer-stack-editable.lremove');await project.saveAs(projectPath);await idle();
  const snapshot=await page.evaluate(()=>LocalImageLayers.nodes().map(({id,kind,name,visible,locked,opacity,transform,patch_ids,cutout})=>({id,kind,name,visible,locked,opacity,transform,patch_ids,cutout})));
  const savedSession=await page.evaluate(()=>session.id);await page.locator('#project-file').setInputFiles(projectPath);await page.waitForFunction(id=>session?.id!==id&&!busy,savedSession);await idle();
  const reopened=await page.evaluate(()=>LocalImageLayers.nodes().map(({id,kind,name,visible,locked,opacity,transform,patch_ids,cutout})=>({id,kind,name,visible,locked,opacity,transform,patch_ids,cutout})));assert.deepEqual(reopened,snapshot);record('Editable project download and reopen preserve the entire layer stack');
  await page.locator('#layer-menu-trigger').click();await page.locator('#merge').click();await idle();await page.waitForFunction(()=>LocalImageLayers.selected()?.name==='Merged visible');assert.equal(await page.evaluate(()=>LocalImageLayers.nodes().filter(n=>n.visible&&!n.discarded).length),1);record('Merge visible adds an independent image layer and keeps earlier layers recoverable');
  await page.locator('#workspace-generate').click();await page.waitForFunction(()=>workspace==='generate');assert.equal(await page.locator('#retouch-panel').isVisible(),false);assert.equal(await page.locator('#stack-background').isVisible(),false);assert.equal(await page.locator('#layer-add').isEnabled(),false);record('Generate mode cannot mutate hidden editing layers');
  assert.deepEqual(errors,[]);fs.writeFileSync(path.join(out,'results.json'),JSON.stringify({checks,errors},null,2));console.log(JSON.stringify({checks:checks.length,output:out}));
 }catch(error){await page.screenshot({path:path.join(out,'failure.png')});console.error('PAGE ERRORS',errors);throw error;}finally{await browser.close();}
})().catch(error=>{console.error(error);process.exitCode=1;});
