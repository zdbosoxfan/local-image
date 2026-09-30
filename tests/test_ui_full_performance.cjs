// Observations of the real UI/backend, not a performance budget or a mock.
const assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path'),crypto=require('node:crypto');
const {chromium}=require('playwright');
const {imageFixture}=require('./helpers/image-fixture.cjs');
const root=path.resolve(__dirname,'..'),base=process.env.LOCAL_REMOVE_TEST_URL,profile=process.env.MIGRATION_REAL_PROFILE;
assert.ok(base&&profile,'Specify the isolated backend URL and exact QA profile');
assert.equal(new URL(base).hostname,'127.0.0.1');
assert.ok(path.resolve(profile).toLowerCase().startsWith(path.join(root,'qa-artifacts').toLowerCase()+path.sep));
const out=path.join(root,'qa-artifacts/migration/full-performance');fs.mkdirSync(out,{recursive:true});
const hash=value=>crypto.createHash('sha256').update(value).digest('hex');
async function main(){
 const runtime=await(await fetch(base+'/api/local-remove/runtime')).json();assert.equal(path.resolve(runtime.data_root).toLowerCase(),path.resolve(profile).toLowerCase());
 const browser=await chromium.launch({headless:true,channel:'msedge'}),context=await browser.newContext({viewport:{width:1366,height:768}}),page=await context.newPage();
 const errors=[],blocked=[],requests=[],assets=[];let result={passed:false};
 page.on('pageerror',error=>errors.push(error.message));
 await context.route('**/*',route=>{const request=route.request(),url=new URL(request.url());if(url.origin!==new URL(base).origin||request.method()!=='GET'&&!/^\/api\/local-remove\/(import|session\/[^/]+\/stack)$/.test(url.pathname)){blocked.push(url.pathname);return route.abort();}requests.push({path:url.pathname,method:request.method()});return route.continue();});
 page.on('response',response=>{if(response.url().includes('/frontend-assets/'))assets.push(response.body().then(bytes=>({url:response.url(),sha256:hash(bytes),bytes:bytes.length})));});
 await context.addInitScript(()=>{localStorage.setItem('local-image.hardware-guide.v1','1');localStorage.setItem('local-image.first-ai-setup.v1','1');window.__perfCsp=[];document.addEventListener('securitypolicyviolation',event=>window.__perfCsp.push({directive:event.violatedDirective,blocked:event.blockedURI}));});
 const cdp=await context.newCDPSession(page);await cdp.send('Performance.enable');
 const memory=async()=>{const {metrics}=await cdp.send('Performance.getMetrics');return Object.fromEntries(metrics.filter(item=>['JSHeapUsedSize','JSHeapTotalSize','Nodes','Documents','LayoutCount','RecalcStyleCount'].includes(item.name)).map(item=>[item.name,item.value]));};
 const settled=()=>page.evaluate(()=>new Promise(resolve=>requestAnimationFrame(()=>requestAnimationFrame(resolve))));
 try{
  let start=performance.now();await page.goto(base+'/remove');await page.waitForFunction(()=>document.body.dataset.reactReady==='true');const startupMs=performance.now()-start,empty=await memory();
  const pixels=imageFixture(4096,2732);fs.writeFileSync(path.join(out,'large-synthetic.png'),pixels);
  const [chooser]=await Promise.all([page.waitForEvent('filechooser'),page.getByRole('button',{name:'Open image…',exact:true}).click()]);
  start=performance.now();await chooser.setFiles({name:'Large synthetic 4096x2732.png',mimeType:'image/png',buffer:pixels});await page.waitForFunction(()=>{const s=window.LocalImageEditor.getSnapshot();return s.document?.width===4096&&!s.busy;});await settled();const importReadyMs=performance.now()-start,loaded=await memory();
  assert.equal(await page.locator('#empty-root').isHidden(),true);assert.ok(await page.locator('#photo-image').isVisible());
  await page.locator('#viewport').focus();await page.keyboard.press('1');await settled();
  const box=await page.locator('#viewport').boundingBox();const before=await page.evaluate(()=>window.LocalImageEditor.canvas.getSnapshot());
  await page.evaluate(()=>{window.__panMetrics={frames:[],running:true,publications:0};window.__panUnsubscribe=window.LocalImageEditor.subscribe(()=>window.__panMetrics.publications++);const frame=time=>{window.__panMetrics.frames.push(time);if(window.__panMetrics.running)requestAnimationFrame(frame);};requestAnimationFrame(frame);});
  await page.keyboard.down('Space');await page.mouse.move(box.x+box.width*.55,box.y+box.height*.55);await page.mouse.down();
  start=performance.now();await page.mouse.move(box.x+box.width*.35,box.y+box.height*.4,{steps:40});await page.mouse.up();await page.keyboard.up('Space');const panDriverMs=performance.now()-start;
  await settled();const after=await page.evaluate(()=>{window.__panMetrics.running=false;window.__panUnsubscribe();return {canvas:window.LocalImageEditor.canvas.getSnapshot(),metrics:window.__panMetrics};});
  assert.equal(after.canvas.photoZoom,1);assert.notEqual(after.canvas.panX,before.panX);assert.equal(after.canvas.gesture,null);
  const frameIntervals=after.metrics.frames.slice(1).map((value,index)=>value-after.metrics.frames[index]);
  const sorted=frameIntervals.slice().sort((a,b)=>a-b),p95=sorted.length?sorted[Math.floor((sorted.length-1)*.95)]:null;
  const afterPan=await memory();await page.screenshot({path:path.join(out,'large-image-1366x768.png')});
  const violations=await page.evaluate(()=>window.__perfCsp);assert.deepEqual(errors,[]);assert.deepEqual(blocked,[]);assert.deepEqual(violations,[]);
  result={passed:true,runtime,browser:browser.version(),node:process.version,viewport:{width:1366,height:768},image:{width:4096,height:2732,bytes:pixels.length,sha256:hash(pixels)},startupMs,importReadyMs,panDriverMs,frameIntervals,p95FrameIntervalMs:p95,documentPublicationsDuringPan:after.metrics.publications,cameraBefore:before,cameraAfter:after.canvas,memory:{empty,loaded,afterPan},assets:await Promise.all(assets),requests,errors,cspViolations:violations,
   limits:'Single real-backend run in installed headless Edge. Timing includes automation and normal system load. JS heap excludes decoded image, GPU and total-process memory. No performance budget is established; historical mocked-fixture timings are not a comparable workload.'};
 }catch(error){result={...result,failure:error.message,errors,blocked};throw error;}
 finally{const file=path.join(out,'results.json');if(fs.existsSync(file)){fs.mkdirSync(path.join(out,'history'),{recursive:true});fs.copyFileSync(file,path.join(out,'history',Date.now()+'.json'));}fs.writeFileSync(file,JSON.stringify(result,null,2));await browser.close();}
 console.log(JSON.stringify({passed:result.passed,startupMs:result.startupMs,importReadyMs:result.importReadyMs,panDriverMs:result.panDriverMs,p95FrameIntervalMs:result.p95FrameIntervalMs,documentPublicationsDuringPan:result.documentPublicationsDuringPan,output:out},null,2));
}
main().catch(error=>{console.error(error);process.exitCode=1;});
