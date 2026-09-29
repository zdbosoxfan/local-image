// Explicit GPU acceptance through the real studio UI and installed LoRA registry.
const {chromium}=require('playwright');
const assert=require('node:assert/strict');
const fs=require('node:fs'),path=require('node:path');
const base=process.env.LOCAL_REMOVE_TEST_URL||'http://127.0.0.1:51249';
const output=path.resolve(process.argv[2]||'qa-artifacts/v05/live-studio');
fs.mkdirSync(output,{recursive:true});
(async()=>{
  const browser=await chromium.launch({headless:true,channel:'msedge'});
  const page=await browser.newPage({viewport:{width:1440,height:900}}),errors=[];
  page.on('pageerror',error=>errors.push(error.message));
  try{
    await page.goto(base+'/remove');
    await page.locator('#hardware-dialog').waitFor({state:'visible'});
    await page.locator('#hardware-continue').click();
    await page.locator('#workspace-generate').click();
    await page.waitForFunction(()=>generationModels.some(model=>model.id==='z-image-turbo')&&!generationLoading);
    await page.locator('#gen-model').selectOption('z-image-turbo');
    await page.locator('#gen-prompt').fill("A child's crayon drawing of a yellow submarine with pink fish, blue ocean, playful simple shapes on white paper.");
    await page.locator('#gen-lora-options > summary').click();
    await page.locator('#lora-library').click();
    await page.locator('#lora-installed-list button').filter({hasText:'Use'}).first().click();
    await page.locator('#lora-close').click();
    assert.equal(await page.locator('#gen-lora-count').textContent(),'1');
    await page.locator('#gen-output-tab').click();
    await page.locator('#gen-seed').fill('20260929');
    const started=Date.now();
    const response=page.waitForResponse(response=>response.url()===base+'/api/local-remove/generation'&&response.request().method()==='POST',{timeout:600000});
    await page.locator('#gen-run').click();
    const result=await response,body=await result.json();
    assert.equal(result.status(),200,JSON.stringify(body));
    await page.waitForFunction(()=>!busy&&session?.generation?.model==='z-image-turbo',{},{timeout:30000});
    assert.equal(body.session.generation.loras[0].id,'ad436c78fd03da0e7b4dff88');
    assert.equal(body.session.generation.loras[0].strength,1);
    await page.locator('#gen-prompt-tab').click();
    await page.screenshot({path:path.join(output,'z-image-lora-studio.png')});
    const saved=await page.evaluate(async()=>{
      const saved=await(await api('/api/local-remove/session/'+session.id+'/save',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({revision:session.revision,format:'png'})})).json();
      await api('/api/local-remove/session/'+session.id+'/export-project',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({revision:session.revision})});
      return {download:saved.download,id:session.id};
    });
    for(const [name,url]of [['z-image-lora.png',saved.download],['z-image-lora.lremove','/api/local-remove/session/'+saved.id+'/download-project']]){
      const downloaded=await page.request.get(base+url);assert.ok(downloaded.ok());fs.writeFileSync(path.join(output,name),await downloaded.body());
    }
    assert.deepEqual(errors,[]);
    const report={model:body.session.generation.model,seconds:(Date.now()-started)/1000,generation:body.session.generation,session_id:saved.id,browser_errors:errors,real_ui:true,real_gpu:true};
    fs.writeFileSync(path.join(output,'results.json'),JSON.stringify(report,null,2));console.log('PASS '+JSON.stringify(report));
  }catch(error){await page.screenshot({path:path.join(output,'failure.png')});throw error;}
  finally{await browser.close();}
})().catch(error=>{console.error(error);process.exitCode=1;});
