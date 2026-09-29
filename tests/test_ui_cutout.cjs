// Real-browser cutout acceptance. AI execution is covered by the live Qwen runner;
// this suite exercises real mask, compositing, background, shadow and export APIs.
const {chromium}=require('playwright');
const assert=require('node:assert/strict');
const fs=require('node:fs');
const path=require('node:path');
const output=path.resolve(process.argv[2]||'qa-artifacts/cutout-ui');
fs.mkdirSync(output,{recursive:true});

async function main(){
  const browser=await chromium.launch({headless:true,channel:process.env.BROWSER_CHANNEL||'msedge'});
  const page=await browser.newPage({viewport:{width:1440,height:960}}),errors=[];
  await page.addInitScript(()=>{localStorage.setItem('local-image.hardware-guide.v1','1');localStorage.setItem('local-image.first-ai-setup.v1','1');});
  page.on('pageerror',error=>errors.push(error.message));
  // Stable capability fixtures: the real user's installed model inventory is
  // irrelevant to whether the UI gates each selected variant correctly.
  await page.route('**/api/local-remove/qwen/status',route=>route.fulfill({json:{connected:true,ready:true,variants:[{id:'int8',available:true},{id:'bf16',available:false,reason:'Full BF16 model not installed'}]}}));
  const idle=()=>page.waitForFunction(()=>!!session&&!busy);
  const revision=()=>page.evaluate(()=>session.revision);
  const studio=name=>page.locator('#studio-'+name+'-tab').click();
  const waitEdit=async before=>{await page.waitForFunction(value=>!!session&&session.revision>value&&!busy,before);};
  async function rectangle(x1,y1,x2,y2){
    await page.locator('[data-tool="rectangle"]').click();const box=await page.locator('#photo').boundingBox();
    await page.mouse.move(box.x+box.width*x1,box.y+box.height*y1);await page.mouse.down();await page.mouse.move(box.x+box.width*x2,box.y+box.height*y2,{steps:8});await page.mouse.up();
  }
  async function alphaAt(x,y){return page.evaluate(async({x,y})=>{const image=new Image();image.src=url('/preview?full=true&revision='+session.revision);await image.decode();const canvas=document.createElement('canvas');canvas.width=image.width;canvas.height=image.height;const context=canvas.getContext('2d');context.drawImage(image,0,0);return [...context.getImageData(Math.round(image.width*x),Math.round(image.height*y),1,1).data];},{x,y});}
  try{
    await page.goto(new URL('/remove',process.env.LOCAL_REMOVE_TEST_URL||'http://127.0.0.1:51248').href);
    await page.waitForFunction(()=>settingsLoaded&&qwenStatus);
    const image=await page.evaluate(()=>{const canvas=document.createElement('canvas');canvas.width=800;canvas.height=600;const ctx=canvas.getContext('2d');ctx.fillStyle='#d4c7b3';ctx.fillRect(0,0,800,600);ctx.fillStyle='#3c7169';ctx.fillRect(180,90,440,420);ctx.fillStyle='#f2d7a0';ctx.fillRect(330,200,140,190);return canvas.toDataURL('image/png').split(',')[1];});
    const fixture={name:'Cutout subject.png',mimeType:'image/png',buffer:Buffer.from(image,'base64')};
    await page.locator('#file').setInputFiles(fixture);await idle();
    assert.equal(await page.locator('#folder-panel').isVisible(),false,'Single image has no filmstrip');
    await page.locator('#workspace-cutout').click();
    assert.equal(await page.locator('#cutout-panel').isVisible(),true);assert.equal(await page.locator('#retouch-panel').isVisible(),false);
    assert.equal(await page.locator('#cutout-remove').isEnabled(),true);
    await page.locator('#qwen-variant').selectOption('bf16');assert.equal(await page.locator('#cutout-remove').isEnabled(),false,'Unavailable selected variant cannot run');
    await page.locator('#qwen-variant').selectOption('int8');
    // Pen selection creates a cutout manually, with no model required.
    await page.locator('[data-tool="pen"]').click();const box=await page.locator('#photo').boundingBox();
    for(const [x,y]of [[.2,.1],[.8,.1],[.8,.9],[.2,.9]])await page.mouse.click(box.x+box.width*x,box.y+box.height*y);
    await page.locator('#finish').click();await page.locator('#cutout-restore').click();
    assert.equal(await page.locator('#cutout-refine').textContent(),'Keep selection');let before=await revision();
    await page.locator('#cutout-refine').click();await waitEdit(before);
    assert.equal((await alphaAt(.05,.05))[3],0);assert.equal((await alphaAt(.5,.5))[3],255);
    assert.equal(await page.locator('#stage').evaluate(element=>element.classList.contains('cutout-preview')),true);
    await studio('subject');before=await revision();await page.locator('#cutout-undo').click();await waitEdit(before);
    assert.equal(await page.evaluate(()=>!!session.cutout?.enabled),false,'Undo first cutout restores the whole original');
    before=await revision();await page.locator('#cutout-redo').click();await waitEdit(before);assert.equal((await alphaAt(.05,.05))[3],0,'Redo restores alpha');
    // Move uses an isolated foreground preview; the background stays in place.
    await page.locator('#viewport').focus();await page.keyboard.press('v');
    await page.waitForFunction(()=>transformAssets?.sessionId===session.id&&transformAssets?.revision===session.revision);
    let movingPhoto=await page.locator('#photo').boundingBox();before=await revision();
    await page.mouse.move(movingPhoto.x+movingPhoto.width*.5,movingPhoto.y+movingPhoto.height*.5);await page.mouse.down();await page.mouse.move(movingPhoto.x+movingPhoto.width*.6,movingPhoto.y+movingPhoto.height*.58,{steps:8});
    assert.equal(await page.locator('#photo-image').isVisible(),false,'Dragging previews isolated foreground and background');
    await page.mouse.up();await waitEdit(before);
    assert.equal(await page.evaluate(()=>session.cutout.transform.offset_x),80);assert.equal(await page.evaluate(()=>session.cutout.transform.offset_y),48);
    assert.equal((await alphaAt(.22,.5))[3],0,'Moving changes canvas alpha at the old subject location');
    before=await revision();await page.locator('#transform-scale').fill('60');await page.locator('#transform-scale').dispatchEvent('change');await waitEdit(before);
    before=await revision();await page.locator('#transform-rotation').fill('25');await page.locator('#transform-rotation').dispatchEvent('change');await waitEdit(before);
    assert.deepEqual(await page.evaluate(()=>session.cutout.transform),{offset_x:80,offset_y:48,scale:.6,rotation:25});
    await page.locator('#cutout-panel-scroll').evaluate(element=>element.scrollTop=0);await page.screenshot({path:path.join(output,'00-transformed-subject.png')});
    const projectDownload=page.waitForEvent('download');await page.locator('#file-menu-trigger').click();await page.locator('#save-project').click();const project=await projectDownload,projectPath=path.join(output,'transformed.lremove');await project.saveAs(projectPath);await idle();
    const previousId=await page.evaluate(()=>session.id);await page.locator('#project-file').setInputFiles(projectPath);await page.waitForFunction(id=>session?.id!==id&&!busy,previousId);
    assert.deepEqual(await page.evaluate(()=>session.cutout.transform),{offset_x:80,offset_y:48,scale:.6,rotation:25},'Project reload preserves transform');
    await studio('transform');before=await revision();await page.locator('#transform-reset').click();await waitEdit(before);assert.equal(await page.evaluate(()=>session.cutout.transform.scale),1);
    await studio('subject');before=await revision();await page.locator('#cutout-undo').click();await waitEdit(before);assert.equal(await page.evaluate(()=>session.cutout.transform.rotation),25,'Transform reset can be undone');
    before=await revision();await page.locator('#cutout-redo').click();await waitEdit(before);assert.equal(await page.evaluate(()=>session.cutout.transform.offset_x),0);
    // Brush erase and restore commit opposing edits to the same alpha pixels.
    await page.locator('[data-tool="brush"]').click();await page.locator('#cutout-erase').click();
    let photo=await page.locator('#photo').boundingBox();await page.mouse.click(photo.x+photo.width*.5,photo.y+photo.height*.5);
    before=await revision();await page.locator('#cutout-refine').click();await waitEdit(before);assert.equal((await alphaAt(.5,.5))[3],0);
    photo=await page.locator('#photo').boundingBox();await page.mouse.click(photo.x+photo.width*.5,photo.y+photo.height*.5);await page.locator('#cutout-restore').click();
    before=await revision();await page.locator('#cutout-refine').click();await waitEdit(before);assert.equal((await alphaAt(.5,.5))[3],255);
    await studio('background');before=await revision();await page.locator('#background-mode').selectOption('color');await waitEdit(before);assert.equal((await alphaAt(.05,.05))[3],255,'Color background flattens composition opacity');
    await studio('background');await page.locator('#shadow-disclosure > summary').click();before=await revision();await page.locator('#shadow-enabled').check();await waitEdit(before);assert.equal(await page.evaluate(()=>session.cutout.shadow.enabled),true);
    await studio('subject');before=await revision();await page.locator('#cutout-feather').fill('2');await page.locator('#cutout-feather').dispatchEvent('change');await waitEdit(before);assert.equal(await page.evaluate(()=>session.cutout.feather),2);
    before=await revision();await page.locator('#background-file').setInputFiles({...fixture,name:'Warm studio.png'});await waitEdit(before);assert.match(await page.locator('#background-name').textContent(),/Warm studio/);
    await studio('background');before=await revision();await page.locator('#background-mode').selectOption('transparent');await waitEdit(before);
    await studio('background');before=await revision();await page.locator('#background-mode').selectOption('image');await waitEdit(before);assert.match(await page.locator('#background-name').textContent(),/Warm studio/,'Switching modes reuses the saved image background');
    const backgroundFolder=path.join(output,'background-folder');fs.mkdirSync(backgroundFolder,{recursive:true});fs.writeFileSync(path.join(backgroundFolder,'Folder backdrop.png'),fixture.buffer);
    await page.locator('#background-folder-file').setInputFiles(backgroundFolder);assert.equal(await page.locator('#background-grid button').count(),1);
    before=await revision();await page.locator('#background-grid button').click();await waitEdit(before);assert.match(await page.locator('#background-name').textContent(),/Folder backdrop/,'Browser folder thumbnails apply the chosen image');
    await page.locator('#workspace-retouch').click();assert.equal(await page.locator('#retouch-panel').isVisible(),true);
    assert.equal(await page.locator('#stage').evaluate(element=>element.classList.contains('cutout-preview')),true,'Switching toolsets retains composition');
    await page.locator('#mode-ai').click();await page.locator('#ai-provider').selectOption('qwen');await rectangle(.3,.3,.4,.4);assert.equal(await page.locator('#remove').isEnabled(),true);
    await page.locator('#retouch-qwen-variant').selectOption('bf16');assert.equal(await page.locator('#remove').isEnabled(),false,'Qwen repair respects model availability');
    await page.locator('#retouch-qwen-variant').selectOption('int8');await page.locator('#workspace-cutout').click();
    await studio('background');before=await revision();await page.locator('#background-mode').selectOption('transparent');await waitEdit(before);
    const downloadPromise=page.waitForEvent('download');await page.locator('#cutout-export').click();const download=await downloadPromise;await download.saveAs(path.join(output,'cutout-export.png'));await idle();assert.match(download.suggestedFilename(),/\.png$/i);
    await page.screenshot({path:path.join(output,'01-cutout-workspace.png')});
    // Selection prompt is dispatched without accidental foreground instructions;
    // the backend is responsible for adding empty-scene positive/negative text.
    let generateRequest=null;
    await page.route('**/cutout/generate-background',async route=>{generateRequest=route.request().postDataJSON();const data=await page.evaluate(()=>session);await route.fulfill({json:data});});
    await studio('background');await page.locator('[data-studio-panel=background]').filter({has:page.locator('#background-generate')}).locator('summary').click();await page.locator('#background-prompt').fill('Empty beige studio, soft window light');await page.locator('#background-generate').click();
    await page.waitForFunction(()=>!busy);assert.equal(generateRequest.variant,'int8');assert.match(generateRequest.prompt,/Empty beige studio/);
    await page.locator('#file').setInputFiles([fixture,{...fixture,name:'Second subject.png'}]);await idle();
    assert.equal(await page.locator('#filmstrip button').count(),2);await page.locator('#filmstrip-toggle').click();assert.equal(await page.locator('#filmstrip').isVisible(),false);await page.locator('#filmstrip-toggle').click();assert.equal(await page.locator('#folder-panel').isVisible(),true);
    const viewport=await page.locator('#viewport').boundingBox(),filmstrip=await page.locator('#folder-panel').boundingBox();assert.ok(filmstrip.y>=viewport.y+viewport.height-1,'Filmstrip is below the canvas');
    await page.screenshot({path:path.join(output,'02-bottom-filmstrip.png')});
    assert.deepEqual(errors,[],'No editor JavaScript errors');
    console.log('PASS: Retouch/Cutout switching, model gating, pen cutout and undo/redo, subject drag/scale/rotation/reset and project persistence, brush erase/restore alpha, color/image/folder backgrounds, feather, shadow, PNG export, background generation request, single/multiple bottom filmstrip.');
  }catch(error){await page.screenshot({path:path.join(output,'failure.png')});throw error;}finally{await page.unrouteAll({behavior:'ignoreErrors'});await browser.close();}
}
main().catch(error=>{console.error(error);process.exitCode=1;});
