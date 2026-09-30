// Redesign acceptance against an explicitly isolated source preview.
// Stock responses are fixtures; image import, CPU repair, layers and PNG export
// use the real backend. No generation, model download or installer work occurs.
const {chromium}=require('playwright');
const assert=require('node:assert/strict');
const fs=require('node:fs');
const path=require('node:path');
const crypto=require('node:crypto');
const base=process.env.LOCAL_REMOVE_TEST_URL||'http://127.0.0.1:51273';
const output=path.resolve(process.argv[2]||'qa-artifacts/ui-redesign/shell-checks');
fs.mkdirSync(output,{recursive:true});

async function main(){
  const runtime=await(await fetch(base+'/api/local-remove/runtime')).json();
  assert.match(runtime.data_root.replaceAll('\\','/'),/\/qa-artifacts\/ui-redesign\/profile$/i,'This suite may only use its isolated redesign profile');
  const browser=await chromium.launch({headless:true,channel:process.env.BROWSER_CHANNEL||'msedge'});
  const page=await browser.newPage({viewport:{width:1920,height:1080},deviceScaleFactor:1});
  const errors=[],requests=[],checks=[],screenshots=[];
  page.on('pageerror',error=>errors.push(error.message));
  page.on('request',request=>{if(request.method()!=='GET')requests.push({url:request.url(),body:request.postData()});});
  await page.addInitScript(()=>{
    localStorage.setItem('local-image.hardware-guide.v1','1');
    localStorage.setItem('local-image.first-ai-setup.v1','1');
    localStorage.setItem('local-image.first-task.v1','1');
  });
  const idle=()=>page.waitForFunction(()=>session&&!busy&&settingsLoaded);
  const record=name=>{checks.push(name);console.log('PASS: '+name);};
  const shot=async name=>{
    await page.mouse.move(4,4);await page.screenshot({path:path.join(output,name),animations:'disabled'});
    screenshots.push({file:name,viewport:page.viewportSize()});
  };
  async function rectangle(x1,y1,x2,y2){
    await page.locator('[data-tool="rectangle"]').click();
    const bounds=await page.locator('#photo').boundingBox();
    await page.mouse.move(bounds.x+bounds.width*x1,bounds.y+bounds.height*y1);
    await page.mouse.down();await page.mouse.move(bounds.x+bounds.width*x2,bounds.y+bounds.height*y2,{steps:8});await page.mouse.up();
    await page.waitForFunction(()=>hasSelection&&!busy);
  }
  async function clearSelection(){
    await page.locator('#select-menu-trigger').click();await page.locator('#clear').click();
    await page.waitForFunction(()=>!hasSelection&&!points.length&&!busy);
  }
  try{
    await page.goto(base+'/remove');await page.waitForFunction(()=>settingsLoaded&&initCompleted);
    const ids=await page.locator('[id]').evaluateAll(nodes=>nodes.map(node=>node.id));
    assert.equal(new Set(ids).size,ids.length,'All control IDs are unique');
    const typography=await page.evaluate(()=>({size:getComputedStyle(document.body).fontSize,font:getComputedStyle(document.body).fontFamily}));
    assert.equal(typography.size,'14px');assert.match(typography.font,/Segoe|Inter|sans-serif/i);
    for(const id of ['studio-stock-open','studio-generated-open','studio-export','studio-undo','studio-redo','studio-inspector-toggle']){
      assert.equal(await page.locator('#'+id).count(),1,id+' is available once');
      assert.ok(await page.locator('#'+id).getAttribute('aria-label')||await page.locator('#'+id).textContent(),id+' has an accessible name');
    }
    record('Readable default typography and unique, named studio controls');

    const png=await page.evaluate(()=>{
      const canvas=document.createElement('canvas');canvas.width=960;canvas.height=640;
      const context=canvas.getContext('2d'),gradient=context.createLinearGradient(0,0,960,640);
      gradient.addColorStop(0,'#839ca2');gradient.addColorStop(1,'#345e67');context.fillStyle=gradient;context.fillRect(0,0,960,640);
      context.fillStyle='#a24b35';context.beginPath();context.arc(480,320,12,0,Math.PI*2);context.fill();
      return canvas.toDataURL('image/png').split(',')[1];
    });
    const fixture=Buffer.from(png,'base64');
    await page.locator('#file').setInputFiles({name:'Studio acceptance.png',mimeType:'image/png',buffer:fixture});await idle();
    assert.equal(await page.locator('#folder-panel').isVisible(),false,'One image has no filmstrip');
    assert.equal(await page.locator('#studio-export').isEnabled(),true);
    record('Single-image opening gives the canvas the filmstrip space');

    const inspector=page.locator('#studio-inspector-toggle');
    assert.equal(await inspector.getAttribute('aria-expanded'),'true');
    const openWidth=(await page.locator('#viewport').boundingBox()).width;
    await inspector.focus();await page.keyboard.press('Space');
    await page.waitForFunction(()=>document.querySelector('#studio-inspector-toggle').getAttribute('aria-expanded')==='false');
    await page.waitForFunction(width=>document.querySelector('#viewport').getBoundingClientRect().width>width+100,openWidth);
    assert.equal(await inspector.evaluate(node=>node===document.activeElement),true,'Keyboard focus remains on the panel toggle');
    await page.keyboard.press('Enter');
    await page.waitForFunction(()=>document.querySelector('#studio-inspector-toggle').getAttribute('aria-expanded')==='true');
    record('Inspector collapse and restore work from the keyboard and resize the canvas');

    for(const name of ['file','edit','layer','select','view','help']){
      const trigger=page.locator('#'+name+'-menu-trigger');await trigger.focus();await page.keyboard.press('ArrowDown');
      assert.equal(await page.locator('#'+name+'-menu').isVisible(),true,name+' opens by keyboard');
      assert.equal(await page.locator('.menu:visible').count(),1,'Only one command menu opens');
      await page.keyboard.press('Escape');assert.equal(await page.locator('.menu:visible').count(),0);
      assert.equal(await trigger.evaluate(node=>node===document.activeElement),true,'Escape restores menu trigger focus');
    }
    record('Command menus support keyboard entry, dismissal and focus return');

    // Deterministic public-stock stand-in; no provider/network variability.
    await page.route('**/api/local-remove/stock/**',async route=>{
      const url=new URL(route.request().url());
      if(url.pathname.endsWith('/providers'))return route.fulfill({json:{providers:[{id:'openverse',label:'Openverse',available:true}],default_provider:'openverse'}});
      if(url.pathname.includes('/thumbnail/'))return route.fulfill({contentType:'image/png',body:fixture});
      if(url.pathname.endsWith('/search'))return route.fulfill({json:{results:Array.from({length:9},(_,index)=>({id:'studio-fixture-'+index,provider:'openverse',asset_id:'studio-fixture-'+index,title:'Synthetic landscape '+(index+1),creator:'Local Image QA',width:960,height:640,license:'CC0',license_url:'https://creativecommons.org/publicdomain/zero/1.0/',source_url:'https://example.com/qa',thumbnail_url:'/api/local-remove/stock/thumbnail/studio-fixture-'+index})),next_page:null,checked_at:new Date().toISOString()}});
      return route.abort('blockedbyclient');
    });
    await page.locator('#studio-stock-open').click();await page.locator('#stock-dialog').waitFor({state:'visible'});
    await page.locator('#stock-query').fill('landscape');await page.locator('#stock-search').click();
    await page.waitForFunction(()=>stockResults.length===9&&!stockLoading);
    await page.locator('.stock-result').nth(2).click();
    const stockChoice=await page.evaluate(()=>stockSelectedId);
    assert.equal(await page.locator('#studio-assets').isVisible(),true,'Assets opens in the left dock');
    assert.equal(await page.locator('#stock-dialog').getAttribute('data-docked'),'true');
    assert.equal(await page.locator('#stock-dialog').evaluate(node=>node.matches(':modal')),false,'Docked stock is nonmodal');
    const compact=await page.locator('#stock-dialog').boundingBox();
    const dockCanvas=await page.locator('#viewport').boundingBox();
    assert.ok(compact.width<page.viewportSize().width*.4&&compact.x+compact.width<=dockCanvas.x+2,'Stock opens beside the canvas in the locked left dock');
    // Pick a visible point on the photograph which is outside the stock bounds.
    await page.locator('[data-tool="rectangle"]').click();
    const photo=await page.locator('#photo').boundingBox();
    const free=await page.evaluate(({photo})=>{
      for(const [fx,fy]of [[.2,.55],[.5,.7],[.75,.75],[.2,.8]]){
        const x=photo.x+photo.width*fx,y=photo.y+photo.height*fy;
        if(document.elementFromPoint(x,y)?.closest('#viewport')&&document.elementFromPoint(x+22,y+22)?.closest('#viewport'))return{x,y};
      }
      return null;
    },{photo});
    assert.ok(free,'The stock dock leaves an exposed working area');
    await page.mouse.move(free.x,free.y);await page.mouse.down();await page.mouse.move(free.x+22,free.y+22,{steps:5});await page.mouse.up();
    await page.waitForFunction(()=>hasSelection&&!busy);
    record('Left Assets dock is nonmodal and the adjacent canvas remains editable');
    // Expand/collapse IDs are shared with the stock implementation.
    await page.locator('#stock-expand').click();
    await page.waitForFunction(width=>document.querySelector('#stock-dialog').getBoundingClientRect().width>width+150,compact.width);
    await shot('stock-expanded-1920.png');
    await page.locator('#stock-expand').click();
    await page.waitForFunction(width=>Math.abs(document.querySelector('#stock-dialog').getBoundingClientRect().width-width)<4,compact.width);
    assert.equal(await page.locator('#stock-dialog').getAttribute('data-docked'),'true');
    assert.equal(await page.locator('#stock-query').inputValue(),'landscape');
    assert.equal(await page.evaluate(()=>stockSelectedId),stockChoice,'Expand/collapse preserves selected image');
    assert.equal(await page.locator('.stock-result').count(),9,'Expand/collapse preserves results');
    await page.locator('#stock-info').click();
    await page.locator('#stock-detail-panel').waitFor({state:'visible'});
    assert.match(await page.locator('#stock-detail-panel').textContent(),/CC0/,'Information reveals the selected license');
    await page.locator('#stock-info-close').click();
    await shot('stock-docked-1920.png');
    await page.locator('#stock-close').click();await clearSelection();
    record('Stock expands for browsing and returns to its left Assets dock');

    await page.locator('#mode-heal').click();await page.locator('#heal-method').selectOption('telea');
    await rectangle(.47,.45,.53,.55);
    await page.locator('#studio-undo').click();await page.waitForFunction(()=>!hasSelection&&!busy);
    await page.locator('#studio-redo').click();await page.waitForFunction(()=>hasSelection&&!busy);
    const revision=await page.evaluate(()=>session.revision);
    await page.locator('#remove').click();
    await page.waitForFunction(before=>session.revision>before&&session.layers.length===1&&!busy,revision);
    assert.equal(await page.locator('#layers [data-layer-id]').count(),1);
    record('Studio history restores selections and real CPU repair creates an editable layer');

    const downloadWait=page.waitForEvent('download');await page.locator('#studio-export').click();
    const download=await downloadWait;const exportPath=path.join(output,'studio-repaired.png');await download.saveAs(exportPath);await idle();
    const exported=fs.readFileSync(exportPath);
    assert.equal(exported.subarray(0,8).toString('hex'),'89504e470d0a1a0a','Toolbar export downloads a genuine PNG');
    assert.equal(exported.readUInt32BE(16),960);assert.equal(exported.readUInt32BE(20),640);
    const exportedPixel=await page.evaluate(async bytes=>{
      const image=new Image();image.src='data:image/png;base64,'+bytes;await image.decode();
      const canvas=document.createElement('canvas');canvas.width=image.width;canvas.height=image.height;const context=canvas.getContext('2d');context.drawImage(image,0,0);
      return [...context.getImageData(480,320,1,1).data];
    },exported.toString('base64'));
    assert.ok(exportedPixel[1]>exportedPixel[0],'The exported center contains the repaired background pixels');
    record('Studio Export dispatches a real repair export with correct dimensions and repaired pixels');

    await page.locator('#file').setInputFiles([
      {name:'Studio collection A.png',mimeType:'image/png',buffer:fixture},
      {name:'Studio collection B.png',mimeType:'image/png',buffer:fixture}
    ]);await idle();
    await page.locator('#folder-panel').waitFor({state:'visible'});
    assert.equal(await page.locator('#filmstrip [data-folder-entry]').count(),2);
    await page.locator('#folder-next').click();await page.waitForFunction(()=>collectionIndex===1&&!busy);
    assert.match(await page.locator('#filename').textContent(),/collection B/);
    record('Opening multiple images creates a working bottom filmstrip');

    for(const [width,height]of [[1920,1080],[1440,900],[1024,768]]){
      await page.setViewportSize({width,height});await page.locator('#fit').click();
      const layout=await page.evaluate(()=>({
        width:innerWidth,body:document.body.scrollWidth,
        canvas:document.querySelector('#viewport').getBoundingClientRect().toJSON(),
        strip:document.querySelector('#folder-panel').getBoundingClientRect().toJSON(),
        exportButton:document.querySelector('#studio-export').getBoundingClientRect().toJSON()
      }));
      assert.ok(layout.body<=width+1,'No horizontal page overflow at '+width);
      assert.ok(layout.strip.top>=layout.canvas.bottom-2,'Filmstrip follows the canvas at '+width);
      assert.ok(layout.strip.bottom<=height,'Filmstrip remains within the window at '+width);
      assert.ok(layout.canvas.width>=width*.45&&layout.canvas.height>=height*.4,'Canvas retains useful working space at '+width);
      assert.ok(layout.exportButton.left>=0&&layout.exportButton.right<=width&&layout.exportButton.bottom<=height,'Export remains reachable at '+width);
      await shot('studio-'+width+'x'+height+'.png');
    }
    record('Canvas, export and bottom filmstrip fit 1920, 1440 and 1024 desktop widths');

    await page.setViewportSize({width:1440,height:900});
    await page.locator('#edit-menu-trigger').click();await page.locator('#settings').click();
    await page.locator('#interface-density').selectOption('large');
    assert.ok(parseFloat(await page.evaluate(()=>getComputedStyle(document.body).fontSize))>14,'Large text remains available');
    await page.locator('#interface-density').selectOption('comfortable');await page.keyboard.press('Escape');
    record('Larger interface text remains available through Settings');

    assert.deepEqual(errors,[],'No browser runtime errors');
    assert.equal(requests.some(request=>/\/(generation|inpaint|qwen\/generate|cutout\/generate|upscale)(?:\?|$)/.test(new URL(request.url).pathname)),false,'No GPU generation was requested');
    const removeRequests=requests.filter(request=>/\/session\/[^/]+\/remove$/.test(new URL(request.url).pathname));
    assert.equal(removeRequests.length,1);assert.equal(JSON.parse(removeRequests[0].body).model,'heal');
    fs.writeFileSync(path.join(output,'results.json'),JSON.stringify({ok:true,scope:'Isolated source browser preview; stock fixtures, real CPU repair and PNG export, no GPU jobs.',checks,screenshots,export:{file:path.basename(exportPath),width:960,height:640,sha256:crypto.createHash('sha256').update(exported).digest('hex')},limitations:['Native folder pickers and native application bridge are unavailable in browser preview.']},null,2));
  }catch(error){
    await shot('failure.png');fs.writeFileSync(path.join(output,'failure.json'),JSON.stringify({ok:false,error:error.stack,checks,errors},null,2));throw error;
  }finally{
    await page.unrouteAll({behavior:'ignoreErrors'});await browser.close();
  }
}
main().catch(error=>{console.error(error);process.exitCode=1;});
