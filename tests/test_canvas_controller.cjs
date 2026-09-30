// Real Edge Canvas2D and pointer capture in a controlled nonced page. No app
// service, native bridge, provider, inference or authoritative export runs here.
const assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path'),http=require('node:http'),crypto=require('node:crypto');
const {stripTypeScriptTypes}=require('node:module');
const {chromium}=require('playwright');
const root=path.resolve(__dirname,'..'),output=path.join(root,'qa-artifacts/migration/canvas-extraction');fs.mkdirSync(output,{recursive:true});
const nonce='canvas-extraction-test';
const modules=new Set(['canvasController.ts','canvasMath.ts','canvasContracts.ts']);
const html=`<!doctype html><meta charset="utf-8"><title>Persistent canvas extraction fixture</title>
<style nonce="${nonce}">*{box-sizing:border-box}html,body{margin:0;background:#181b20;color:white;font:14px system-ui}#viewport{position:relative;width:800px;height:560px;overflow:hidden;border:1px solid #455;touch-action:none}#stage{position:absolute;transform-origin:0 0}#stage>canvas,#stage>img,#layer-stack{position:absolute;left:0;top:0;pointer-events:none}#photo{z-index:0}#photo-image{z-index:1}#layer-stack{z-index:2}#selection{z-index:3}#draft{z-index:4}#brush-cursor{position:absolute;pointer-events:none;transform:translate(-50%,-50%);border:1px solid white;border-radius:50%;z-index:5}[hidden]{display:none!important}</style>
<div id="viewport" tabindex="0"><div id="stage"><canvas id="photo"></canvas><img id="photo-image" alt=""><div id="layer-stack"></div><canvas id="selection"></canvas><canvas id="draft"></canvas></div><div id="brush-cursor" hidden></div></div>
<script nonce="${nonce}">window.csp=[];document.addEventListener('securitypolicyviolation',event=>csp.push(event.violatedDirective));</script>
<script type="module" nonce="${nonce}">
import{createCanvasController}from'/editor/canvasController.ts';
const element=id=>document.getElementById(id),elements={viewport:element('viewport'),stage:element('stage'),photo:element('photo'),photoImage:element('photo-image'),overlay:element('selection'),draft:element('draft'),layerStack:element('layer-stack'),brushCursor:element('brush-cursor')};
const decode=url=>new Promise((resolve,reject)=>{const image=new Image();image.onload=()=>resolve(image);image.onerror=reject;image.src=url;});
const canvas=document.createElement('canvas');canvas.width=640;canvas.height=480;const g=canvas.getContext('2d');g.fillStyle='#b64a38';g.fillRect(0,0,640,480);const original=await decode(canvas.toDataURL());
const source=id=>({id,revision:0,width:640,height:480,layer_stack:[{id:'original',kind:'original',visible:true,locked:false,opacity:1,transform:{offset_x:0,offset_y:0,scale:1,rotation:0},bounds:[90,80,240,220]}]});
window.docs={a:source('a'),b:source('b'),large:{...source('large'),width:6400,height:4800}};window.accepted=docs.a;window.commits=[];window.notifications=[];window.restored=[];window.reports=[];window.deferAssets=false;window.pendingAssets=[];window.deferMasks=false;window.pendingMasks=[];
window.editorHidden=false;
const ports={getAcceptedDocument:()=>accepted,isModalOpen:()=>false,isEditorHidden:()=>editorHidden,report:(...args)=>reports.push(args),onSnapshot:snapshot=>notifications.push(snapshot),restoreInteraction:state=>restored.push(state),
loadImage:url=>url.startsWith('data:')?(deferMasks?new Promise(resolve=>pendingMasks.push(async()=>resolve(await decode(url)))):decode(url)):deferAssets?new Promise(resolve=>pendingAssets.push(()=>resolve(original))):Promise.resolve(original),
commitLayerTransform:async value=>{commits.push(value);accepted={...accepted,revision:accepted.revision+1,layer_stack:accepted.layer_stack.map(layer=>layer.id===value.layerId?{...layer,transform:value.transform}:layer)};docs[accepted.id]=accepted;await controller.presentDocument({document:accepted,original,composite:original});}};
window.controller=createCanvasController(elements,ports);window.initialViewport=elements.viewport;window.initialPhoto=elements.photo;
window.scene=async id=>{accepted=docs[id];await controller.presentDocument({document:accepted,original,composite:original});};
window.maskPixel=async(x,y)=>{const png=await decode('data:image/png;base64,'+controller.selectionPayload());const c=document.createElement('canvas');c.width=png.width;c.height=png.height;const context=c.getContext('2d');context.drawImage(png,0,0);return Array.from(context.getImageData(x,y,1,1).data);};
window.point=(x,y)=>{const state=controller.getSnapshot(),rect=elements.viewport.getBoundingClientRect();return{x:rect.left+elements.viewport.clientLeft+state.panX+x*(state.photoZoom*accepted.width/state.previewWidth),y:rect.top+elements.viewport.clientTop+state.panY+y*(state.photoZoom*accepted.width/state.previewWidth)};};
elements.viewport.addEventListener('pointerdown',event=>window.lastPointer=event.pointerId);
await scene('a');window.ready=true;
</script>`;
const server=http.createServer((request,response)=>{
 const pathname=new URL(request.url,'http://127.0.0.1').pathname;
 response.setHeader('Content-Security-Policy',`default-src 'none'; script-src 'self' 'nonce-${nonce}'; style-src 'nonce-${nonce}'; img-src 'self' data:; connect-src 'self'; base-uri 'none'`);
 if(pathname==='/'){response.setHeader('Content-Type','text/html');return response.end(html);}
 if(pathname.startsWith('/editor/')&&modules.has(pathname.slice(8))){response.setHeader('Content-Type','text/javascript');return response.end(stripTypeScriptTypes(fs.readFileSync(path.join(root,'frontend/src/editor',pathname.slice(8)),'utf8'),{mode:'strip'}));}
 response.statusCode=404;response.end();
});
(async()=>{
 await new Promise(resolve=>server.listen(0,'127.0.0.1',resolve));const base=`http://127.0.0.1:${server.address().port}`;
 const browser=await chromium.launch({headless:true,channel:'msedge'}),page=await browser.newPage({viewport:{width:900,height:650}}),errors=[],checks=[];let passed=false,failure=null;
 page.on('pageerror',error=>errors.push(error.stack));
 const record=name=>{checks.push(name);console.log('PASS: '+name);};
 async function drag(x1,y1,x2,y2,end='up'){
  const a=await page.evaluate(([x,y])=>point(x,y),[x1,y1]),b=await page.evaluate(([x,y])=>point(x,y),[x2,y2]);
  await page.mouse.move(a.x,a.y);await page.mouse.down();await page.mouse.move(b.x,b.y,{steps:5});
  if(end==='cancel')await page.evaluate(()=>document.getElementById('viewport').dispatchEvent(new PointerEvent('pointercancel',{pointerId:lastPointer,bubbles:true})));
  else if(end==='lost')await page.evaluate(()=>document.getElementById('viewport').dispatchEvent(new PointerEvent('lostpointercapture',{pointerId:lastPointer,bubbles:true})));
  await page.mouse.up();
 }
 try{
  await page.goto(base);await page.waitForFunction(()=>window.ready);
  await page.evaluate(()=>controller.setInteractionState({tool:'rectangle'}));await drag(40,50,110,120,'cancel');
  assert.equal(await page.evaluate(()=>controller.getSnapshot().hasSelection),true);assert.deepEqual(await page.evaluate(()=>maskPixel(70,80)),[255,255,255,255]);assert.deepEqual(await page.evaluate(()=>maskPixel(10,10)),[0,0,0,255]);
  record('Rectangle pointercancel preserves the completed selection, matching the legacy boundary');
  await page.evaluate(()=>controller.clearSelection());await page.evaluate(()=>controller.setInteractionState({tool:'brush',brushSize:20}));await drag(180,100,220,130);
  assert.deepEqual(await page.evaluate(()=>maskPixel(200,115)),[255,255,255,255]);
  const firstFingerprint=await page.evaluate(()=>controller.pendingFingerprint('a'));
  await page.evaluate(()=>controller.setInteractionState({subtract:true}));await drag(200,115,200,115);assert.deepEqual(await page.evaluate(()=>maskPixel(200,115)),[0,0,0,255]);
  assert.equal(await page.evaluate(()=>controller.getSnapshot().hasSelection),true);assert.notEqual(await page.evaluate(()=>controller.pendingFingerprint('a')),firstFingerprint,'Changed nonempty mask invalidates pending-edit acknowledgement without exposing pixels');
  record('Brush selection and subtract retain native-pixel width and actual mask pixels');
  await page.evaluate(()=>{controller.clearSelection();controller.setInteractionState({tool:'pen',subtract:false});});
  for(const coordinates of[[50,50],[130,50],[130,130]]){const p=await page.evaluate(([x,y])=>point(x,y),coordinates);await page.mouse.click(p.x,p.y);}
  assert.equal(await page.evaluate(()=>controller.getSnapshot().pointCount),3);
  await page.evaluate(()=>controller.undoSelection());assert.equal(await page.evaluate(()=>controller.getSnapshot().pointCount),2);
  await page.evaluate(()=>controller.redoSelection());assert.equal(await page.evaluate(()=>controller.getSnapshot().pointCount),3);
  await page.evaluate(()=>controller.finishPen());assert.deepEqual(await page.evaluate(()=>maskPixel(110,70)),[255,255,255,255]);
  assert.equal(await page.evaluate(()=>controller.getSnapshot().pointCount),0);assert.equal(await page.evaluate(()=>commits.length),0);
  record('Pen-point undo/redo has selection priority and closing the path creates a real mask without backend history');
  await page.evaluate(()=>{controller.setPhotoZoom(2);controller.setInteractionState({operation:'heal',brushSize:125,tool:'ellipse'});});
  const remembered=await page.evaluate(()=>controller.getSnapshot());await page.evaluate(()=>scene('b'));assert.equal(await page.evaluate(()=>controller.getSnapshot().hasSelection),false);
  assert.equal(await page.evaluate(()=>controller.pendingSelection('a')),true);await page.evaluate(()=>scene('a'));
  const restored=await page.evaluate(()=>controller.getSnapshot());assert.equal(restored.photoZoom,remembered.photoZoom);assert.equal(restored.panX,remembered.panX);assert.equal(restored.panY,remembered.panY);assert.equal(restored.hasSelection,true);
  assert.equal(await page.evaluate(()=>restored.at(-1).brushSize),125);assert.equal(await page.evaluate(()=>initialViewport===controller.elements.viewport&&initialPhoto===controller.elements.photo),true);
  record('Document navigation restores compressed selection/history, camera and tool settings without remounting canvas nodes');
  await page.evaluate(()=>scene('b'));await page.evaluate(()=>{deferMasks=true;window.lateNavigation=scene('a');});await page.waitForFunction(()=>pendingMasks.length===1);
  await page.evaluate(()=>scene('b'));await page.evaluate(async()=>{for(const resolve of pendingMasks.splice(0))await resolve();await lateNavigation;deferMasks=false;});
  assert.equal(await page.evaluate(()=>controller.getSnapshot().documentId),'b');assert.equal(await page.evaluate(()=>controller.getSnapshot().hasSelection),false);
  await page.evaluate(()=>scene('a'));record('Late per-document mask decoding cannot overwrite newer navigation');
  await page.evaluate(()=>{controller.clearSelection();controller.fit();controller.setInteractionState({tool:'move',selectedLayerId:'original',handActive:false,workspace:'retouch'});notifications=[];});
  await drag(160,150,185,168,'cancel');assert.equal(await page.evaluate(()=>commits.length),0);
  await drag(160,150,190,175,'lost');assert.equal(await page.evaluate(()=>commits.length),0);
  await drag(160,150,190,170);await page.waitForFunction(()=>commits.length===1);
  const transformed=await page.evaluate(()=>commits[0]);assert.equal(transformed.transform.offset_x,30);assert.equal(transformed.transform.offset_y,20);assert.equal(transformed.revision,0);
  assert.ok(await page.evaluate(()=>notifications.length)<12,'No snapshot publication for each move event');
  record('Layer pointercancel/lost capture cancel; pointerup commits exactly once at the captured revision');
  await page.evaluate(()=>{deferAssets=true;controller.setInteractionState({selectedLayerId:'original'});});
  const p=await page.evaluate(()=>point(180,160));await page.mouse.move(p.x,p.y);await page.mouse.down();await page.waitForFunction(()=>pendingAssets.length===1);
  await page.evaluate(()=>{document.getElementById('viewport').dispatchEvent(new PointerEvent('pointercancel',{pointerId:lastPointer,bubbles:true}));for(const resolve of pendingAssets.splice(0))resolve();});await page.mouse.up();
  assert.equal(await page.evaluate(()=>document.getElementById('photo-image').hidden),false);assert.equal(await page.evaluate(()=>commits.length),1);
  record('Late layer-preview decoding after cancellation cannot repaint or commit a retired gesture');
  await page.evaluate(()=>{deferAssets=false;});
  const q=await page.evaluate(()=>point(180,160));await page.mouse.move(q.x,q.y);await page.mouse.down();await page.mouse.move(q.x+20,q.y+10);
  await page.evaluate(()=>{accepted={...accepted,revision:accepted.revision+1};docs.a=accepted;});await page.mouse.up();assert.equal(await page.evaluate(()=>commits.length),1);
  await page.evaluate(()=>scene('a'));record('Changed accepted revision rejects a stale transform commit');
  await page.evaluate(()=>scene('large'));await page.evaluate(()=>controller.setInteractionState({tool:'brush',brushSize:125}));const largePoint=await page.evaluate(()=>point(100,100));await page.mouse.move(largePoint.x,largePoint.y);
  const cursor=await page.evaluate(()=>({diameter:parseFloat(document.getElementById('brush-cursor').style.width),expected:125*controller.getSnapshot().photoZoom,width:controller.getSnapshot().previewWidth}));
  assert.equal(cursor.width,640);assert.ok(Math.abs(cursor.diameter-cursor.expected)<.0001,'CSS serialization retains native-size ring within subpixel precision');record('Large native image coordinates keep brush width tied to native photo zoom, not preview pixels');
  await page.evaluate(()=>scene('a'));
  await page.evaluate(()=>{controller.setPhotoZoom(2);controller.setInteractionState({busy:true,handActive:true});const context=document.getElementById('selection').getContext('2d');window.originalDrawImage=context.drawImage;window.redundantPaints=0;context.drawImage=function(...args){redundantPaints++;return originalDrawImage.apply(this,args);};});
  const panStart=await page.evaluate(()=>point(320,240));await page.mouse.move(panStart.x,panStart.y);await page.mouse.down();const panNotifications=await page.evaluate(()=>notifications.length);await page.mouse.move(panStart.x+20,panStart.y+10,{steps:8});
  assert.equal(await page.evaluate(()=>notifications.length),panNotifications,'Pan pointermove paints without publishing React state');
  const panBefore=await page.evaluate(()=>controller.getSnapshot());await page.evaluate(()=>controller.setInteractionState({busy:true,handActive:true}));
  assert.equal(await page.evaluate(()=>controller.getSnapshot().gesture),'pan');assert.equal(await page.evaluate(()=>redundantPaints),0,'Unchanged store publication must not repaint the full mask');
  await page.mouse.move(panStart.x+30,panStart.y+20,{steps:8});assert.equal(await page.evaluate(()=>notifications.length),panNotifications);await page.mouse.up();assert.notEqual(await page.evaluate(()=>controller.getSnapshot().panX),panBefore.panX);
  assert.equal(await page.evaluate(()=>notifications.length),panNotifications+1,'Pointerup publishes exactly one settled pan snapshot');
  await page.evaluate(()=>{document.getElementById('selection').getContext('2d').drawImage=originalDrawImage;controller.setInteractionState({busy:false,handActive:false});controller.fit();});
  record('Unchanged interaction publication neither repaints the mask nor interrupts an allowed busy-state camera pan');
  await page.evaluate(()=>{controller.setPhotoZoom(2);controller.setInteractionState({workspace:'generate'});editorHidden=true;});
  const hiddenBefore=await page.evaluate(()=>controller.getSnapshot());await page.mouse.move(400,280);await page.mouse.wheel(0,150);await page.waitForTimeout(50);
  for(const mode of ['middle','space','hand']){
   await page.evaluate(value=>{controller.setSpaceHeld(value==='space');controller.setInteractionState({handActive:value==='hand'});},mode);
   await page.mouse.move(400,280);await page.mouse.down({button:mode==='middle'?'middle':'left'});await page.mouse.move(425,300);assert.equal(await page.evaluate(()=>controller.getSnapshot().gesture),null);await page.mouse.up({button:mode==='middle'?'middle':'left'});
  }
  assert.deepEqual(await page.evaluate(()=>controller.getSnapshot()),hiddenBefore,'Hidden editor pointer/wheel input preserves the retained camera and selection');
  await page.evaluate(()=>{editorHidden=false;controller.setSpaceHeld(false);controller.setInteractionState({handActive:false});});
  await page.mouse.move(400,280);await page.mouse.wheel(0,150);await page.waitForFunction(value=>controller.getSnapshot().photoZoom!==value,hiddenBefore.photoZoom);
  const visibleBefore=await page.evaluate(()=>controller.getSnapshot());await page.mouse.down({button:'middle'});await page.mouse.move(425,300);await page.mouse.up({button:'middle'});
  assert.notEqual(await page.evaluate(()=>controller.getSnapshot().panX),visibleBefore.panX,'A visible generated image still supports camera panning');
  await page.evaluate(()=>{controller.setInteractionState({workspace:'retouch'});controller.fit();});
  record('Hidden generation editor rejects wheel and middle/Space/Hand pan while a visible generated image retains camera controls');
  await page.evaluate(()=>{controller.setInteractionState({tool:'brush',subtract:false,brushSize:12});controller.clearSelection();for(let index=0;index<15;index++)controller.clearSelection({recordHistory:true});});
  const undoCount=await page.evaluate(async()=>{let count=0;while(await controller.undoSelection())count++;return count;});assert.equal(undoCount,12);
  record('Selection mask history remains bounded to twelve states');
  await page.evaluate(()=>controller.setInteractionState({tool:'rectangle'}));await drag(30,35,80,85);
  assert.equal(await page.evaluate(()=>controller.commitSelection('other-document')),false);assert.equal(await page.evaluate(()=>controller.getSnapshot().hasSelection),true);
  assert.equal(await page.evaluate(()=>controller.commitSelection('a')),true);const committed=await page.evaluate(()=>controller.getSnapshot());assert.equal(committed.hasSelection,false);assert.equal(committed.canUndoSelection,false);assert.equal(committed.canRedoSelection,false);
  record('Accepted repair/cutout selection commit clears mask and both local history stacks only for its document');
  await page.evaluate(async()=>{docs.full={...docs.a,id:'full',width:1280,height:960};await scene('full');});
  const fullMask=await page.evaluate(async()=>{const image=new Image();image.src='data:image/png;base64,'+controller.fullMaskPayload();await image.decode();const canvas=document.createElement('canvas');canvas.width=image.width;canvas.height=image.height;const context=canvas.getContext('2d');context.drawImage(image,0,0);return{width:image.width,height:image.height,last:Array.from(context.getImageData(image.width-1,image.height-1,1,1).data),snapshotKeys:Object.keys(controller.getSnapshot())};});
  assert.equal(fullMask.width,1280);assert.equal(fullMask.height,960);assert.deepEqual(fullMask.last,[255,255,255,255]);assert.equal(fullMask.snapshotKeys.includes('mask'),false);
  await page.evaluate(()=>{controller.forgetDocument('full');});assert.equal(await page.evaluate(()=>controller.pendingSelection('full')),false);await page.evaluate(()=>scene('a'));
  record('Full-mask payload uses native document dimensions while pixel buffers stay outside public snapshots');
  await page.screenshot({path:path.join(output,'persistent-canvas.png')});
  const before=await page.evaluate(()=>{controller.dispose();return controller.getSnapshot();});await drag(40,40,60,60);assert.deepEqual(await page.evaluate(()=>controller.getSnapshot()),before);
  assert.deepEqual(await page.evaluate(()=>csp),[]);assert.deepEqual(errors,[]);record('Disposal removes gesture ownership; nonced controlled page reports no CSP or browser errors');passed=true;
 }catch(error){failure=error.stack;await page.screenshot({path:path.join(output,'failure.png')}).catch(()=>{});throw error;}
 finally{
  const sources=['frontend/src/editor/canvasController.ts','frontend/src/editor/canvasMath.ts','frontend/src/editor/canvasContracts.ts','frontend/src/editor/canvasExtraction.ts'].map(name=>({path:name,sha256:crypto.createHash('sha256').update(fs.readFileSync(path.join(root,name))).digest('hex')}));
  fs.writeFileSync(path.join(output,'results.json'),JSON.stringify({passed,failure,checks,errors,sources,scope:'Real Edge Canvas2D/pointer fixture; no backend/native/GPU/export certification'},null,2));await browser.close();await new Promise(resolve=>server.close(resolve));
 }
})().catch(error=>{console.error(error);process.exitCode=1;server.close();});
