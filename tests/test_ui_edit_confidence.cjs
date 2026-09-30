// Real manual alpha edits and compositing. No generation/AI endpoints are used.
const {chromium}=require('playwright');
const assert=require('node:assert/strict');
const fs=require('node:fs');
const path=require('node:path');
const output=path.resolve(process.argv[2]||'qa-artifacts/focus-fixes/edit-confidence');
fs.mkdirSync(output,{recursive:true});
async function main(){
  const browser=await chromium.launch({headless:true,channel:process.env.BROWSER_CHANNEL||'msedge'});
  const page=await browser.newPage({viewport:{width:1440,height:960}}),errors=[],requests=[];
  await page.addInitScript(()=>{localStorage.setItem('local-image.hardware-guide.v1','1');localStorage.setItem('local-image.first-ai-setup.v1','1');localStorage.setItem('local-image.first-task.v1','1');});
  page.on('pageerror',error=>errors.push(error.message));page.on('request',request=>{if(request.method()!=='GET')requests.push(request.url());});
  const idle=()=>page.waitForFunction(()=>session&&!busy);
  const revision=()=>page.evaluate(()=>session.revision);
  const edited=before=>page.waitForFunction(value=>session.revision>value&&!busy,before);
  async function menu(command){await page.locator('#edit-menu-trigger').click();await page.locator('#'+command).click();}
  async function numeric(id,value){const before=await revision();await page.locator('#'+id).fill(String(value));await page.locator('#'+id).press('Enter');await edited(before);}
  async function rectangle(x1,y1,x2,y2){
    await page.locator('[data-tool="rectangle"]').click();const box=await page.locator('#photo').boundingBox();
    await page.mouse.move(box.x+box.width*x1,box.y+box.height*y1);await page.mouse.down();await page.mouse.move(box.x+box.width*x2,box.y+box.height*y2,{steps:8});await page.mouse.up();
  }
  async function pixel(bytes,x,y){return page.evaluate(async({bytes,x,y})=>{const image=new Image();image.src='data:image/png;base64,'+bytes;await image.decode();const canvas=document.createElement('canvas');canvas.width=image.width;canvas.height=image.height;const ctx=canvas.getContext('2d');ctx.drawImage(image,0,0);return [...ctx.getImageData(Math.round(image.width*x),Math.round(image.height*y),1,1).data];},{bytes,x,y});}
  async function exported(name){const pending=page.waitForEvent('download');await page.locator('#cutout-export').click();const download=await pending;const file=path.join(output,name);await download.saveAs(file);await idle();return fs.readFileSync(file).toString('base64');}
  try{
    await page.goto(new URL('/remove',process.env.LOCAL_REMOVE_TEST_URL||'http://127.0.0.1:51251').href);await page.waitForFunction(()=>settingsLoaded);
    const image=await page.evaluate(()=>{const canvas=document.createElement('canvas');canvas.width=600;canvas.height=480;const ctx=canvas.getContext('2d');ctx.fillStyle='#e7dcca';ctx.fillRect(0,0,600,480);ctx.fillStyle='#28695e';ctx.fillRect(120,48,360,384);return canvas.toDataURL('image/png').split(',')[1];});
    await page.locator('#file').setInputFiles({name:'Precision cutout.png',mimeType:'image/png',buffer:Buffer.from(image,'base64')});await idle();await page.locator('#workspace-cutout').click();
    await page.locator('[data-tool="pen"]').click();const box=await page.locator('#photo').boundingBox();
    for(const [x,y]of [[.2,.1],[.8,.1]])await page.mouse.click(box.x+box.width*x,box.y+box.height*y);
    await menu('undo');assert.equal(await page.evaluate(()=>points.length),1);
    await page.locator('#viewport').focus();await page.keyboard.press('Control+Shift+Z');assert.equal(await page.evaluate(()=>points.length),2,'Keyboard redo restores the same path point as Edit menu');
    await page.keyboard.press('Control+Z');await menu('redo');assert.equal(await page.evaluate(()=>points.length),2);
    await page.locator('#select-menu-trigger').click();await page.locator('#clear').click();
    await rectangle(.2,.1,.8,.9);await page.locator('#cutout-restore').click();let before=await revision();await page.locator('#cutout-apply-selection').click();await edited(before);
    assert.equal(await page.evaluate(()=>session.cutout.enabled),true);assert.equal(await page.locator('#cutout-apply-selection').isVisible(),false);
    for(const tab of ['subject','background','transform']){await page.locator('#studio-'+tab+'-tab').click();assert.equal(await page.locator('#cutout-undo').isVisible(),true,'History remains visible in '+tab);assert.equal(await page.locator('#cutout-redo').isVisible(),true);}
    await numeric('transform-scale-value',56.7);assert.equal(await page.locator('#transform-scale').inputValue(),'56.7');assert.ok(Math.abs(await page.evaluate(()=>session.cutout.transform.scale)-.567)<1e-9);
    await numeric('transform-rotation-value',12.3);assert.equal(await page.locator('#transform-rotation').inputValue(),'12.3');
    await page.locator('#studio-background-tab').click();before=await revision();await menu('undo');await edited(before);assert.equal(await page.evaluate(()=>session.cutout.transform.rotation),0);
    before=await revision();await page.locator('#edit-menu-trigger').click();await page.keyboard.press('Control+Shift+Z');await edited(before);assert.equal(await page.evaluate(()=>session.cutout.transform.rotation),12.3,'Open-menu shortcut follows the same redo context');
    await page.locator('#studio-transform-tab').click();await numeric('transform-x',-40);
    before=await revision();await page.locator('#transform-scale-value').fill('401');await page.locator('#transform-scale-value').press('Enter');assert.equal(await revision(),before,'Out of bounds values cannot commit');assert.equal(await page.locator('#transform-scale-value').inputValue(),'56.7');
    await page.locator('#transform-rotation-value').fill('');await page.locator('#transform-rotation-value').press('Enter');assert.equal(await revision(),before,'Empty numeric entry cannot become zero');
    await page.locator('#transform-rotation-value').fill('20');await page.locator('#transform-rotation-value').press('Escape');assert.equal(await revision(),before,'Escape cancels typed changes');assert.equal(await page.locator('#transform-rotation-value').inputValue(),'12.3');
    before=await revision();await page.locator('#transform-scale-value').press('ArrowUp');await page.locator('#transform-scale-value').press('Enter');await edited(before);assert.equal(await page.locator('#transform-scale-value').inputValue(),'56.8','Arrow uses the declared exact step');
    before=await revision();await page.locator('#transform-reset').click();await edited(before);
    await page.locator('#studio-subject-tab').click();await numeric('cutout-feather-value',1.5);assert.equal(await page.locator('#cutout-feather').inputValue(),'1.5');
    await page.locator('#studio-background-tab').click();await page.locator('#shadow-disclosure > summary').click();before=await revision();await page.locator('#shadow-enabled').check();await edited(before);
    for(const [id,value,field,expected]of [['shadow-opacity-value',42,'opacity',.42],['shadow-blur-value',12.5,'blur',12.5],['shadow-x-value',-321,'offset_x',-321],['shadow-y-value',27,'offset_y',27],['shadow-squeeze-value',63,'squeeze',.63]]){
      await numeric(id,value);assert.equal(await page.evaluate(key=>session.cutout.shadow[key],field),expected);assert.equal(await page.locator('#'+id.replace(/-value$/,'')).inputValue(),String(value));
    }
    before=await revision();await page.locator('#shadow-opacity').fill('36');await page.locator('#shadow-opacity').dispatchEvent('input');await page.locator('#shadow-opacity').dispatchEvent('change');await edited(before);assert.equal(await page.locator('#shadow-opacity-value').inputValue(),'36','Range updates exact numeric field');
    before=await revision();await page.locator('#shadow-enabled').uncheck();await edited(before);assert.equal(await page.locator('#shadow-blur-value').isEnabled(),false,'Inactive shadow fields cannot commit');
    await rectangle(.45,.45,.55,.55);await page.locator('#cutout-erase').click();
    before=await revision();await menu('undo');await page.waitForFunction(()=>!hasSelection&&!busy);assert.equal(await revision(),before,'Selection Undo leaves applied pixels unchanged');assert.equal(await page.evaluate(()=>hasSelection),false);
    await page.locator('#viewport').focus();await page.keyboard.press('Control+Shift+Z');await page.waitForFunction(()=>hasSelection&&!busy);assert.equal(await revision(),before,'Selection Redo leaves applied pixels unchanged');
    assert.match(await page.locator('#cutout-export-state').textContent(),/Selection not applied/);assert.match(await page.locator('#cutout-export').textContent(),/Export current image/);
    const saveCount=requests.filter(value=>value.endsWith('/save')).length;await page.locator('#file-menu-trigger').click();await page.locator('#save').click();
    assert.equal(requests.filter(value=>value.endsWith('/save')).length,saveCount,'Ordinary export routes to the pending state before writing');assert.equal(await page.locator('#cutout-export-state').evaluate(element=>element===document.activeElement),true);
    const unchanged=await exported('pending-current.png');assert.equal((await pixel(unchanged,.5,.5))[3],255,'Explicit current-image export excludes pending alpha edit');assert.equal(await page.evaluate(()=>hasSelection),true,'Export preserves pending selection');
    before=await revision();await page.locator('#cutout-apply-selection').click();await edited(before);assert.equal(await page.evaluate(()=>hasSelection),false);const applied=await exported('applied-cutout.png');assert.equal((await pixel(applied,.5,.5))[3],0,'Applied selection changes alpha in exported PNG');
    assert.match(await page.locator('#cutout-export-state').textContent(),/ready to export/);await page.screenshot({path:path.join(output,'cutout-edit-confidence.png')});
    assert.deepEqual(errors,[]);assert.equal(requests.some(value=>/generate|\/inpaint/.test(value)),false,'Manual acceptance does not call GPU workflows');
    fs.writeFileSync(path.join(output,'results.json'),JSON.stringify({ok:true,scenarios:['menu/keyboard path and alpha undo/redo','persistent history across 3 tabs','precise numeric and range synchronization','bounds/empty/Escape rejection','pending export guard and explicit applied PNG alpha','no GPU calls']},null,2));
    console.log('PASS: Context Undo/Redo, all Cutout tabs, exact scale/rotation/feather/shadow, numeric validation and keyboard steps, unapplied export guard/current image/apply selection, real PNG alpha.');
  }catch(error){await page.screenshot({path:path.join(output,'failure.png')});throw error;}finally{await browser.close();}
}
main().catch(error=>{console.error(error);process.exitCode=1;});
