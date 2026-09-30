// Actual comfortable brush-toolbar geometry; all backend traffic is read-only.
const assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path'),crypto=require('node:crypto');
const {chromium}=require('playwright');
const root=path.resolve(__dirname,'..'),base=process.env.LOCAL_REMOVE_TEST_URL,profile=process.env.MIGRATION_REAL_PROFILE;
assert.ok(base&&profile);assert.equal(new URL(base).hostname,'127.0.0.1');assert.ok(path.resolve(profile).startsWith(path.join(root,'qa-artifacts')+path.sep));
const output=path.join(root,'qa-artifacts/migration/brush-layout'),hash=value=>crypto.createHash('sha256').update(value).digest('hex');
const sourceFile=path.join(root,'frontend/src/features/shell/ToolControls.tsx');
(async()=>{
 const runtime=await(await fetch(base+'/api/local-remove/runtime')).json();assert.equal(path.resolve(runtime.data_root),path.resolve(profile));fs.mkdirSync(output,{recursive:true});
 const sourceHash=hash(fs.readFileSync(sourceFile)),browser=await chromium.launch({channel:'msedge',headless:true}),page=await browser.newPage({viewport:{width:1366,height:768}});
 const metrics=[],assets=[],errors=[],blocked=[],reads=[];let passed=false,failure=null;
 await page.route('**/*',route=>{const request=route.request(),url=new URL(request.url());if(url.origin!==base||request.method()!=='GET'){blocked.push({method:request.method(),path:url.pathname});return route.abort('blockedbyclient');}return route.continue();});
 await page.addInitScript(()=>{for(const key of ['local-image.hardware-guide.v1','local-image.first-ai-setup.v1','local-image.first-task.v1'])localStorage.setItem(key,'1');localStorage.setItem('local-image.interface-density.v1','comfortable');window.__brushCsp=[];document.addEventListener('securitypolicyviolation',event=>window.__brushCsp.push(event.violatedDirective));});
 page.on('pageerror',error=>errors.push(error.stack));page.on('response',response=>{const url=new URL(response.url());if(url.pathname.startsWith('/frontend-assets/'))reads.push(response.body().then(body=>assets.push({path:url.pathname,sha256:hash(body),status:response.status()})));});
 try{
  await page.goto(base+'/remove');await page.waitForFunction(()=>document.body.dataset.reactReady==='true');
  assert.equal(await page.getByRole('spinbutton',{name:'Brush diameter',exact:true}).count(),1,'Unwrapped input retains its accessible name');
  for(const width of [775,1366,2194]){
   await page.setViewportSize({width,height:900});await page.evaluate(()=>new Promise(resolve=>requestAnimationFrame(()=>requestAnimationFrame(resolve))));
   const value=await page.evaluate(()=>{const group=document.querySelector('.li-brush-size'),slider=group.querySelector('.fui-Slider'),input=group.querySelector('.fui-Input');const rect=node=>{const box=node.getBoundingClientRect();return {left:box.left,right:box.right,top:box.top,bottom:box.bottom,width:box.width,height:box.height,centerY:(box.top+box.bottom)/2};};return {width:innerWidth,density:document.documentElement.dataset.uiDensity,group:rect(group),slider:rect(slider),input:rect(input),directInput:input.parentElement===group,fieldCount:group.querySelectorAll(':scope>.fui-Field').length};});
   metrics.push(value);assert.equal(value.density,'comfortable');assert.equal(value.directInput,true);assert.equal(value.fieldCount,1,'Only the labelled Size slider retains a Field wrapper');
   assert.ok(Math.abs(value.slider.centerY-value.input.centerY)<=1,`Brush diameter and slider share one row at${width}px`);
   assert.ok(value.group.height<=Math.max(value.slider.height,value.input.height)+1,`Brush group does not reserve a second row at${width}px`);
   await page.screenshot({path:path.join(output,`brush-${width}.png`),animations:'disabled'});
  }
  await Promise.all(reads);assert.equal(hash(fs.readFileSync(sourceFile)),sourceHash);assert.deepEqual(errors,[]);assert.deepEqual(blocked,[]);assert.deepEqual(await page.evaluate(()=>window.__brushCsp),[]);passed=true;
 }catch(error){failure=error.stack;await page.screenshot({path:path.join(output,'failure.png'),animations:'disabled'}).catch(()=>{});throw error;}
 finally{await Promise.allSettled(reads);fs.writeFileSync(path.join(output,'results.json'),JSON.stringify({passed,failure,runtime,sourceHash,assets,metrics,errors,blocked,evidence:'Actual comfortable production brush options at775/1366/2194px; no backend writes or native actions'},null,2));await browser.close();}
 console.log(`PASS brush toolbar: ${metrics.length} widths; ${output}`);
})().catch(error=>{console.error(error);process.exitCode=1;});
