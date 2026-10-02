import test from 'node:test';
import assert from 'node:assert/strict';
import {createCanvasController} from './canvasController.ts';

const deferred=()=>{let resolve,reject;const promise=new Promise((yes,no)=>{resolve=yes;reject=no;});return{promise,resolve,reject};};
const tick=()=>new Promise(resolve=>setImmediate(resolve));
const transform=()=>({offset_x:0,offset_y:0,scale:1,rotation:0});
class Element {
  constructor(){this.style={};this.classList={toggle(){}};this.children=[];this.events=new Map();this.hidden=false;this.id='';this.className='photo-image';this.alt='';this.clientWidth=800;this.clientHeight=560;this.clientLeft=0;this.clientTop=0;this.src='';this.naturalWidth=640;this.naturalHeight=480;}
  addEventListener(name,fn){this.events.set(name,fn);}
  removeEventListener(name){this.events.delete(name);}
  fire(name,x=180,y=170){this.events.get(name)?.({clientX:x,clientY:y,pointerId:1,button:0,preventDefault(){},stopImmediatePropagation(){}});}
  getBoundingClientRect(){return{left:0,top:0};}
  setPointerCapture(){this.captured=true;}
  hasPointerCapture(){return!!this.captured;}
  releasePointerCapture(){this.captured=false;this.fire('lostpointercapture');}
  focus(){}
  replaceWith(value){this.replacedBy=value;}
  replaceChildren(){this.children=[];}
  removeAttribute(key){this[key]='';}
  append(value){this.children.push(value);}
}
class Canvas extends Element {
  constructor(){super();this.width=640;this.height=480;this.calls=[];this.context=new Proxy({getImageData:()=>({data:new Uint8ClampedArray(4)}),drawImage:(...args)=>this.calls.push(['draw',...args]),translate:(...args)=>this.calls.push(['translate',...args]),scale:(...args)=>this.calls.push(['scale',...args]),rotate:(...args)=>this.calls.push(['rotate',...args])},{get:(object,key)=>object[key]??(()=>{})});}
  getContext(){return this.context;}
  toDataURL(){return'data:image/png;base64,fixture';}
}
function setup(t,{stable=true,deferSprite=false}={}){
  const oldObserver=globalThis.ResizeObserver;globalThis.ResizeObserver=class{observe(){}disconnect(){}};
  t.after(()=>{globalThis.ResizeObserver=oldObserver;});
  const viewport=new Element(),win=new Element(),doc=new Element();doc.defaultView=win;doc.createElement=name=>name==='canvas'?new Canvas():new Element();viewport.ownerDocument=doc;
  const elements={viewport,stage:new Element(),photo:new Canvas(),photoImage:new Element(),overlay:new Canvas(),draft:new Canvas(),layerStack:new Element(),brushCursor:new Element()};
  let accepted={id:'a',revision:0,width:640,height:480,layer_stack:[{id:'cutout',kind:'cutout',visible:true,locked:false,opacity:1,transform:transform(),bounds:[90,80,240,220],...(stable?{display_key:'pixels-a'}:{})}]};
  const original=new Element();original.src='original';const composite=new Element();composite.src='composite-0';const sprite=new Element();sprite.src='sprite';
  const pending=deferred(),spritePending=deferred(),commits=[],loads=[],reports=[];
  const ports={getAcceptedDocument:()=>accepted,isModalOpen:()=>false,report:(...args)=>reports.push(args),loadImage:async url=>{loads.push(url);return deferSprite?spritePending.promise:sprite;},commitLayerTransform:value=>{commits.push(value);controller.cancelGesture();controller.setInteractionState({busy:true});return pending.promise;}};
  const controller=createCanvasController(elements,ports);t.after(()=>controller.dispose());
  const present=async(value=accepted)=>{accepted=value;const next=new Element();next.src='composite-'+value.revision;await controller.presentDocument({document:value,original,composite:next});return next;};
  const ready=async()=>{await controller.presentDocument({document:accepted,original,composite});controller.setInteractionState({workspace:'cutout',tool:'move',selectedLayerId:'cutout'});controller.setPhotoZoom(1);};
  const point=(x,y)=>{const state=controller.getSnapshot();return[x+state.panX,y+state.panY];};
  const move=async()=>{viewport.fire('pointerdown',...point(160,150));await tick();viewport.fire('pointermove',...point(190,170));viewport.fire('pointerup',...point(190,170));await tick();};
  return{elements,controller,commits,loads,reports,pending,spritePending,sprite,ready,move,present,original,composite,ports,get accepted(){return accepted;},set accepted(value){accepted=value;}};
}
test('released Cutout preview and handles remain at the dragged position while the write is pending',async t=>{
  const f=setup(t);await f.ready();const before=f.controller.getSnapshot();await f.move();
  assert.equal(f.commits.length,1);assert.equal(f.commits[0].transform.offset_x,30);assert.equal(f.commits[0].transform.offset_y,20);
  assert.equal(f.elements.photoImage.hidden,true,'Do not reveal the pre-transform composite during busy publication');
  assert.deepEqual(f.elements.photo.calls.filter(call=>call[0]==='translate').at(-1),['translate',349.5,259.5]);
  assert.equal(f.controller.getSnapshot().gesture,null,'Pointer ownership ends while the last pixels remain');
  assert.equal(f.controller.getSnapshot().panX,before.panX);assert.equal(f.controller.getSnapshot().photoZoom,before.photoZoom);
  const next={...f.accepted,revision:1,layer_stack:f.accepted.layer_stack.map(layer=>({...layer,transform:f.commits[0].transform}))};
  const decoded=await f.present(next);f.pending.resolve(next);await tick();f.controller.setInteractionState({busy:false});
  assert.equal(f.elements.photoImage,decoded,'Present the already-loaded image itself');assert.equal(decoded.hidden,false);
  assert.equal(f.controller.getSnapshot().revision,1);assert.equal(f.controller.getSnapshot().photoZoom,before.photoZoom);
});
test('a rejected transform restores the accepted frame without committing twice',async t=>{
  const f=setup(t);await f.ready();await f.move();f.pending.reject(Error('Write failed'));await tick();f.controller.setInteractionState({busy:false});
  assert.equal(f.elements.photoImage.hidden,false);assert.equal(f.elements.photoImage.src,'composite-0');assert.equal(f.commits.length,1);
});
test('an accepted transform with a failed preview keeps its final pixels until refresh',async t=>{
  const f=setup(t);await f.ready();await f.move();f.accepted={...f.accepted,revision:1};f.pending.resolve(null);await tick();f.controller.setInteractionState({busy:false});
  assert.equal(f.elements.photoImage.hidden,true);assert.deepEqual(f.elements.photo.calls.filter(call=>call[0]==='translate').at(-1),['translate',349.5,259.5]);
  await f.present();assert.equal(f.elements.photoImage.hidden,false);assert.equal(f.controller.getSnapshot().revision,1);
});
test('late transform completion cannot repaint a different document',async t=>{
  const f=setup(t);await f.ready();await f.move();const image=await f.present({...f.accepted,id:'b',revision:0});f.pending.resolve(null);await tick();
  assert.equal(f.controller.getSnapshot().documentId,'b');assert.equal(f.elements.photoImage,image);assert.equal(image.hidden,false);
});
test('source sprites survive transform revisions and refresh only when source pixels change',async t=>{
  const f=setup(t);await f.ready();await f.move();const next={...f.accepted,revision:1,layer_stack:f.accepted.layer_stack.map(layer=>({...layer,transform:f.commits[0].transform}))};await f.present(next);f.pending.resolve(next);await tick();f.controller.setInteractionState({busy:false});
  f.elements.viewport.fire('pointerdown',...(()=>{const s=f.controller.getSnapshot();return[190+s.panX,170+s.panY];})());await tick();f.elements.viewport.fire('pointercancel');
  assert.equal(f.loads.length,1,'Transform metadata must not decode unchanged pixels again');assert.match(f.loads[0],/r=pixels-a$/);
  await f.present({...f.accepted,revision:2,layer_stack:f.accepted.layer_stack.map(layer=>({...layer,display_key:'pixels-b'}))});
  f.elements.viewport.fire('pointerdown',...(()=>{const s=f.controller.getSnapshot();return[190+s.panX,170+s.panY];})());await tick();f.elements.viewport.fire('pointercancel');
  assert.equal(f.loads.length,2);assert.match(f.loads[1],/r=pixels-b$/);
});
test('an empty workspace preserves decoded sources for reopening the retained photo',async t=>{
  const f=setup(t);await f.ready();const image=f.elements.photoImage;
  await f.controller.presentDocument(null);
  assert.equal(image.src,'composite-0');assert.equal(f.original.src,'original');assert.equal(f.controller.getSnapshot().documentId,null);
  await f.controller.presentDocument({document:f.accepted,original:f.original,composite:image});
  assert.equal(f.elements.photoImage,image);assert.equal(image.hidden,false);assert.equal(f.controller.getSnapshot().previewWidth,640);
});
test('delayed released sprites respect Original view after a failed composite refresh',async t=>{
  const f=setup(t,{deferSprite:true});await f.ready();await f.move();f.accepted={...f.accepted,revision:1};f.pending.resolve(null);await tick();f.controller.setInteractionState({busy:false,showOriginal:true});
  f.spritePending.resolve(f.sprite);await tick();
  assert.equal(f.elements.photoImage,f.original);assert.equal(f.original.hidden,false);
});
test('sprite loading errors after pointer release are reported',async t=>{
  const f=setup(t,{deferSprite:true});await f.ready();await f.move();f.spritePending.reject(Error('Sprite decode failed'));await tick();
  assert.ok(f.reports.some(([message,error])=>message==='Sprite decode failed'&&error));f.pending.resolve(null);await tick();
});
test('hidden Refine navigation keeps the source persona and manual camera saved before hiding',async t=>{
  const f=setup(t);await f.ready();f.controller.setPhotoZoom(1.5);
  const before=f.controller.getSnapshot(),restored=[];f.ports.restoreInteraction=value=>restored.push(value);
  f.controller.rememberCurrentView();f.ports.isEditorHidden=()=>true;
  f.controller.setInteractionState({workspace:'generate'});f.elements.viewport.clientWidth=0;f.elements.viewport.clientHeight=0;
  f.controller.resize();assert.equal(f.controller.getSnapshot().panX,before.panX);assert.equal(f.controller.getSnapshot().panY,before.panY);
  f.controller.rememberCurrentView();await f.controller.presentDocument(null);
  f.ports.isEditorHidden=()=>false;f.elements.viewport.clientWidth=800;f.elements.viewport.clientHeight=560;
  await f.controller.presentDocument({document:f.accepted,original:f.original,composite:f.composite});
  assert.equal(restored.at(-1).workspace,'cutout');assert.equal(restored.at(-1).tool,'move');
  assert.equal(f.controller.getSnapshot().photoZoom,before.photoZoom);assert.equal(f.controller.getSnapshot().panX,before.panX);assert.equal(f.controller.getSnapshot().panY,before.panY);
});
