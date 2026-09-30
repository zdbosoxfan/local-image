// Focused tests for the actual page script, without opening or changing user sessions.
const fs=require('node:fs');
const vm=require('node:vm');
const assert=require('node:assert/strict');
const path=require('node:path');
const html=fs.readFileSync(path.join(__dirname,'..','backend','local_remove.html'),'utf8');
const source=fs.readFileSync(path.join(__dirname,'..','backend','frontend','editor.js'),'utf8').replace(/\ninit\(\);\s*$/,'');

class Emitter {
  constructor(){this.listeners={};}
  addEventListener(type,listener){(this.listeners[type]??=[]).push(listener);}
  fire(type,event={}){event.type=type;event.preventDefault=()=>event.prevented=true;for(const listener of this.listeners[type]??[])listener(event);return event;}
}
class Element extends Emitter {
  constructor(tag='div',id=''){
    super();this.tagName=tag.toUpperCase();this.id=id;this.children=[];this.style={};this.attributes={};this.dataset={};this.value='';this.open=false;
    this.clientWidth=1000;this.clientHeight=700;this.clientLeft=1;this.clientTop=1;this.classList={toggle(){},remove(){}};
    this.capture=new Set();this.paints=0;this.width=3000;this.height=2000;
  }
  get options(){return this.children;}
  get parentElement(){return this.parent instanceof Element?this.parent:null;}
  get width(){return this._width;}
  set width(value){this._width=value;if(this.tagName==='CANVAS')this.paints=0;}
  get height(){return this._height;}
  set height(value){this._height=value;if(this.tagName==='CANVAS')this.paints=0;}
  append(...children){for(const child of children){child.parent=this;this.children.push(child);}}
  prepend(child){child.parent=this;this.children.unshift(child);}
  before(...nodes){
    if(!this.parent)return;
    const parent=this.parent;
    for(let node of nodes){
      if(node===this)continue;
      if(typeof node==='string')node={textContent:node};
      if(node.parent)node.parent.children.splice(node.parent.children.indexOf(node),1);
      node.parent=parent;parent.children.splice(parent.children.indexOf(this),0,node);
    }
  }
  remove(){if(this.parent)this.parent.children.splice(this.parent.children.indexOf(this),1);}
  replaceChildren(...children){this.children=[];this.append(...children);}
  setAttribute(key,value){this.attributes[key]=value;}
  removeAttribute(key){delete this.attributes[key];if(key==='src')delete this.src;}
  querySelector(selector){return selector==='option[value="custom"]'?this.children.find(child=>child.value==='custom'):null;}
  closest(selector){if(selector.includes('[data-folder-entry]')&&this.dataset.folderEntry)return this;return /input|select|textarea/.test(selector)&&['INPUT','SELECT','TEXTAREA'].includes(this.tagName)?this:null;}
  getBoundingClientRect(){return{left:10,top:10,width:this.clientWidth+2,height:this.clientHeight+2};}
  setPointerCapture(id){this.capture.add(id);}
  hasPointerCapture(id){return this.capture.has(id);}
  releasePointerCapture(id){this.capture.delete(id);}
  showModal(){this.open=true;}
  close(){this.open=false;this.fire('close');}
  click(){return this.onclick?.();}
  focus(){document.activeElement=this;}
  scrollIntoView(){}
  toDataURL(){return 'data:image/png;base64,'+Buffer.from(JSON.stringify({paint:this.paints,width:this.width,height:this.height})).toString('base64');}
  getContext(){
    if(this.context)return this.context;
    this.context={
      clearRect:()=>{this.paints=0;},drawImage:image=>{this.paints=Number.isFinite(image?.paint)?image.paint:Number.isFinite(image?.paints)?image.paints:this.paints+1;},fillRect:()=>this.paints++,fill:()=>this.paints++,stroke:()=>this.paints++,
      beginPath(){},moveTo(){},lineTo(){},arc(){},rect(){},ellipse(){},closePath(){},putImageData:()=>this.paints++,
      getImageData:()=>({data:new Uint8ClampedArray([0,0,0,this.paints?255:0])})
    };
    return this.context;
  }
}
const elements=new Map([...html.matchAll(/id="([^"]+)"/g)].map(match=>[match[1],new Element('div',match[1])]));
for(const side of ['draft','result']){
  const area=new Element();area.className='refine-image-area';
  area.append(elements.get('refine-'+side+'-image'),elements.get('refine-'+side+'-empty'));
}
for(const match of html.matchAll(/<button\b([^>]*\bid="([^"]+)"[^>]*)>/g)){
  const command=match[1].match(/data-command="([^"]+)"/);
  if(command)elements.get(match[2]).dataset.command=command[1];
}
for(const id of ['photo','selection','draft'])elements.get(id).tagName='CANVAS';
for(const id of ['model','zoom','heal-method'])elements.get(id).tagName='SELECT';
for(const value of [.1,.25,.5,1,2,4]){const option=new Element('option');option.value=String(value);elements.get('zoom').append(option);}
for(const value of ['original','png','jpg','tif','webp']){const option=new Element('option');option.value=value;elements.get('output-format').append(option);}
elements.get('size').value='50';
const buttons=['brush','pen','rectangle','ellipse'].map(tool=>{const button=new Element('button');button.dataset={tool};return button;});
const repairControls=new Element('div'),penContext=new Element('button');penContext.dataset.context='pen';
const document=new Emitter();
document.body=new Element('body');
document.getElementById=id=>elements.get(id);
document.createElement=tag=>new Element(tag);
document.createElementNS=(namespace,tag)=>new Element(tag);
document.createTextNode=text=>({textContent:text});
document.querySelector=selector=>selector==='.context-actions'?repairControls:null;
document.querySelectorAll=selector=>selector==='[data-tool]'?buttons:selector==='[data-command]'?[...elements.values()].filter(element=>element.dataset.command):selector==='[data-context]'?[penContext]:[];
const window=new Emitter();
const storage=new Map();
const context=vm.createContext({document,window,Element,ResizeObserver:class{constructor(callback){this.callback=callback;}observe(){}},
  localStorage:{getItem:key=>storage.get(key)??null,setItem:(key,value)=>storage.set(key,String(value))},
  console,URLSearchParams,Uint8ClampedArray,fetch:()=>{throw Error('Tests must not make network calls');},setInterval:()=>1,clearInterval(){},
  setTimeout,clearTimeout,requestAnimationFrame:callback=>callback(),FormData:class{constructor(){this.fields={};}append(key,value){this.fields[key]=value;}},
  history:{replaceState(){}},location:{search:''},Image:class{}});
vm.runInContext(source,context);
const run=code=>vm.runInContext(code,context);
const viewport=elements.get('viewport');
const key=(code,key,target=elements.get('photo'))=>document.fire('keydown',{code,key,target});
const pointer=(type,x,y,button=0)=>viewport.fire(type,{clientX:x+11,clientY:y+11,pointerId:1,button});
const state=()=>JSON.parse(run('JSON.stringify({zoom:photoZoom(),panX,panY,points,tool,spaceHeld,gesture:gesture?.kind??null,paint:mask.paints})'));
const nearly=(a,b)=>assert.ok(Math.abs(a-b)<1e-8,`${a} differs from ${b}`);

async function main(){
  const host=new Element(),anchor=new Element(),sibling=new Element();
  anchor.before(sibling);assert.equal(sibling.parent,undefined,'before() on a detached node leaves siblings detached');
  host.append(anchor,sibling);anchor.before(sibling);
  assert.deepEqual(host.children,[sibling,anchor],'before() moves an existing sibling rather than duplicating it');
  assert.equal(sibling.parent,host);
  assert.equal(run('operation'),'heal','First use defaults to local healing without an AI connection');
  run(`session={id:'test',width:6000,height:4000,name:'Test',revision:0,layers:[]}; setSizes(3000,2000);`);
  assert.ok(state().zoom<.2,'Fit uses actual-photo pixels');
  run('setPhotoZoom(1)');nearly(run('zoom'),2);assert.equal(elements.get('zoom').value,'1');
  const pointBefore=run('({x:(400-panX)/zoom,y:(250-panY)/zoom})');
  run('setPhotoZoom(1.6,{x:400,y:250})');
  const pointAfter=run('({x:(400-panX)/zoom,y:(250-panY)/zoom})');
  nearly(pointBefore.x,pointAfter.x);nearly(pointBefore.y,pointAfter.y);
  run('setPhotoZoom(900)');nearly(state().zoom,8);
  run('setPhotoZoom(.0001)');nearly(state().zoom,.01);
  run('setPhotoZoom(1);selectTool("pen");points=[{x:900,y:650},{x:1000,y:700}];penDraft()');
  const beforePan=state();
  const space=key('Space',' ');assert.equal(space.prevented,true);
  pointer('pointerdown',450,300);pointer('pointermove',540,360);
  assert.equal(state().gesture,'pan');assert.notEqual(state().panX,beforePan.panX);
  document.fire('keyup',{code:'Space'});pointer('pointermove',555,365);
  assert.equal(state().gesture,'pan','Releasing Space must not start a paint stroke');
  pointer('pointerup',555,365);
  assert.equal(state().gesture,null);assert.equal(state().tool,'pen');
  assert.deepEqual(state().points,beforePan.points,'Unfinished pen points survive panning');
  assert.equal(state().paint,beforePan.paint,'Panning does not change the selection mask');
  run('setPhotoZoom(1.25);fitImage()');assert.deepEqual(state().points,beforePan.points,'Pen points survive zoom and fit');

  // Switching while a brush is held commits its existing stroke once; panning never draws a connecting line.
  run('setPhotoZoom(1);selectTool("brush")');
  pointer('pointerdown',400,250);pointer('pointermove',430,270);
  const brushPaint=state().paint;key('Space',' ');pointer('pointermove',520,340);
  document.fire('keyup',{code:'Space'});pointer('pointermove',560,370);pointer('pointerup',560,370);
  assert.equal(state().paint,brushPaint,'No extra brush segments are drawn while or after temporary panning');
  assert.equal(state().tool,'brush');assert.equal(state().gesture,null);
  pointer('pointermove',570,380);assert.equal(state().paint,brushPaint,'Pointer movement after pan release remains idle');

  run('selectTool("pen");points=[{x:100,y:120},{x:300,y:320}];');
  const savedPen=state().points;key('Space',' ');pointer('pointerdown',400,300);window.fire('blur');
  assert.equal(state().spaceHeld,false);assert.equal(state().gesture,null);assert.deepEqual(state().points,savedPen);
  key('Space',' ');pointer('pointerdown',400,300);pointer('pointercancel',410,310);
  assert.deepEqual(state().points,savedPen);document.fire('keyup',{code:'Space'});
  const inputSpace=key('Space',' ',new Element('input'));assert.equal(inputSpace.prevented,undefined);assert.equal(state().spaceHeld,false);
  elements.get('settings-dialog').open=true;
  key('Space',' ');assert.equal(state().spaceHeld,false,'Dialog keyboard input does not reach the photo');
  elements.get('settings-dialog').open=false;
  elements.get('hand').onclick();assert.equal(run('handActive'),true);
  assert.equal(repairControls.hidden,true,'Hand mode hides repair settings and Apply');
  assert.equal(penContext.hidden,true,'Hand mode immediately hides pen-only commands');
  pointer('pointerdown',400,300);pointer('pointermove',460,340);pointer('pointerup',460,340);
  assert.deepEqual(state().points,savedPen,'Persistent Hand tool also retains the pen path');
  run('selectTool("pen")');assert.equal(run('handActive'),false);assert.deepEqual(state().points,savedPen);
  assert.equal(repairControls.hidden,false,'Returning to a selection tool restores repair controls');
  assert.equal(penContext.hidden,false,'The retained pen path restores its contextual command');
  elements.get('zoom').value='1';elements.get('zoom').onchange();assert.equal(document.activeElement,viewport,'Selecting a zoom level returns keyboard focus to the canvas');

  // Exercise the actual settings handler and removal payload with in-memory API responses.
  run(`applySettings({model:'klein',models:[{id:'klein',label:'FLUX Klein',available:true},{id:'qwen',label:'Qwen removal',available:true}]});
       globalThis.requests=[];json=async(path,body,method)=>{requests.push({path,body,method});return path.endsWith('/settings')?{model:body.model,models}:session;};
       refreshPreview=async()=>{};recent=async()=>{};`);
  assert.equal(run('modelId'),'klein');assert.deepEqual(elements.get('model').options.map(option=>option.value),['klein'],'Only FLUX is offered for AI removal');
  run('setOperation("ai");ready=true;hasSelection=true;');await elements.get('remove').onclick();
  assert.equal(run('requests[0].body.model'),'klein','Removal uses FLUX');
  run('setBusy(true)');assert.equal(elements.get('settings').disabled,true);assert.equal(elements.get('model').disabled,true);
  run('setBusy(false)');
  run(`applySettings({model:'qwen',models:[{id:'klein',label:'FLUX Klein',available:true},{id:'qwen',label:'Qwen removal',available:true},{id:'heal',label:'Quick Heal',available:true}]});
       ready=false;retouchReady=true;hasSelection=true;selectTool('pen');points=[{x:25,y:50},{x:75,y:100}];`);
  elements.get('mode-heal').onclick();
  assert.deepEqual(state().points,[{x:25,y:50},{x:75,y:100}],'Changing editing operation preserves the unfinished pen path');
  assert.equal(elements.get('remove').disabled,false,'Local healing works while the GPU is offline');
  assert.equal(elements.get('remove').textContent,'Heal');
  assert.equal(elements.get('model').options.some(option=>option.value==='heal'),false,'Quick Heal is separate from the saved AI model selector');
  assert.equal(run('modelId'),'klein');assert.equal(run('requests.length'),1,'Switching operation does not change persisted AI settings');
  run('handActive=true;updateToolChrome()');key('KeyJ','j');
  assert.equal(run('operation'),'heal');assert.equal(run('handActive'),false);assert.equal(state().tool,'brush');
  assert.equal(elements.get('heal-brush').attributes['aria-pressed'],'true');
  key('Space',' ');pointer('pointerdown',400,300);pointer('pointermove',450,330);pointer('pointerup',450,330);document.fire('keyup',{code:'Space'});
  assert.equal(run('operation'),'heal','Temporary panning preserves Quick Heal mode');
  run('hasSelection=true');await elements.get('remove').onclick();
  assert.equal(run('requests[1].body.model'),'heal');assert.equal(run('modelId'),'klein','Healing keeps FLUX as the AI model');
  assert.equal(run('requests[1].body.heal_method'),'texture','Quick Heal defaults to texture repair');
  assert.equal(elements.get('heal-method').hidden,false);assert.equal(elements.get('model-shortcut').hidden,true);
  run('hasSelection=true');elements.get('mode-ai').onclick();
  assert.equal(elements.get('remove').disabled,true,'AI removal still requires the GPU');
  run('ready=true;controls()');assert.equal(elements.get('remove').disabled,false);
  await elements.get('remove').onclick();assert.equal(run('requests[2].body.model'),'klein','Returning to AI uses FLUX');
  assert.equal(run('requests[2].body.heal_method'),undefined,'AI removal does not request a healing method');
  assert.equal(elements.get('heal-method').hidden,true);assert.equal(elements.get('model-shortcut').hidden,false);
  run('hasSelection=true;retouchReady=false');elements.get('mode-heal').onclick();assert.equal(elements.get('remove').disabled,true,'Unavailable local retouch is disabled');
  run('handActive=true;updateToolChrome()');key('KeyB','b');assert.equal(run('operation'),'ai');assert.equal(run('handActive'),false);

  // The brush ring and actual mask diameter use original-image pixels, independently of preview resolution.
  run('session.width=6144;setPhotoZoom(1);setBrushSize(100);selectTool("brush")');pointer('pointermove',400,300);
  assert.equal(elements.get('brush-cursor').hidden,false);nearly(parseFloat(elements.get('brush-cursor').style.width),100);
  run('mode(mc)');nearly(run('mc.lineWidth*pixelRatio()'),100);nearly(run('mc.lineWidth*zoom'),parseFloat(elements.get('brush-cursor').style.width));
  const brushBefore=state().paint;run('setPhotoZoom(.5)');nearly(parseFloat(elements.get('brush-cursor').style.width),50);
  key('BracketRight',']');assert.equal(elements.get('size').value,'125');nearly(parseFloat(elements.get('brush-cursor').style.width),62.5);assert.equal(state().paint,brushBefore);
  key('BracketLeft','[',elements.get('size'));assert.equal(elements.get('size').value,'100','Bracket sizing works while the range slider has focus');
  key('BracketRight',']',new Element('input'));assert.equal(elements.get('size').value,'100','Text input does not change brush size');
  elements.get('settings-dialog').open=true;key('BracketRight',']');assert.equal(elements.get('size').value,'100');elements.get('settings-dialog').open=false;
  key('Space',' ');assert.equal(elements.get('brush-cursor').hidden,true);document.fire('keyup',{code:'Space'});assert.equal(elements.get('brush-cursor').hidden,false);
  run('setBusy(true)');assert.equal(elements.get('brush-cursor').hidden,true);run('setBusy(false)');
  run('selectTool("pen")');assert.equal(elements.get('brush-cursor').hidden,true);run('selectTool("brush")');
  viewport.fire('pointerleave');assert.equal(elements.get('brush-cursor').hidden,true);

  // Exercise actual navigation using separate mask snapshots and unfinished paths on two images.
  const failedImages=new Set();
  context.mockLoadImage=async src=>{
    if([...failedImages].some(part=>src.includes(part)))throw Error('Image decoding failed');
    if(src.startsWith('data:'))return {...JSON.parse(Buffer.from(src.split(',')[1],'base64').toString()),src};
    return Object.assign(new Element('img'),{width:6000,height:4000,naturalWidth:6000,naturalHeight:4000,src});
  };
  run(`loadImage=mockLoadImage;session=null;collection=null;viewStates.clear();
       globalThis.photoA={id:'a',name:'a.tif',source_name:'a.tif',width:6000,height:4000,bit_depth:16,revision:1,layers:[],can_return:true,dirty:true,saved_revision:null,collection_id:'folder',entry_id:'ea'};
       globalThis.photoB={id:'b',name:'b.png',source_name:'b.png',width:6000,height:4000,bit_depth:8,revision:0,layers:[],can_return:true,dirty:false,saved_revision:null,collection_id:'folder',entry_id:'eb'};
       globalThis.folder={id:'folder',name:'Test folder',entries:[{id:'ea',name:'a.tif',session_id:'a',dirty:true},{id:'eb',name:'b.png',session_id:'b',dirty:false}]};
       globalThis.navRequests=[];globalThis.saveFailure=false;
       json=async(path,body,method)=>{
         navRequests.push({path,body,method});
         if(path.endsWith('/open')){const index=path.includes('/ea/')?0:1;return {session:index?photoB:photoA,collection:folder,index};}
         if(path.endsWith('/merge'))return {...session,revision:session.revision+1,dirty:true,layers:[...session.layers,{id:'merged',name:'Merged 1',kind:'snapshot',model_label:'Merged visible',visible:true,discarded:false}]};
         if(path.endsWith('/save')){
           if(saveFailure)throw Error('Source image changed');
           const name=body.mode==='unique'?'a-removed.png':body.mode==='overwrite'?session.source_name:'export.webp';
           return {name,bit_depth:body.format==='original'?session.bit_depth:8,session:{...session,dirty:false,saved_revision:session.revision,saved_name:name}};
         }
         throw Error('Unexpected test endpoint '+path);
       };`);
  await run('openSession(photoA,{collection:folder,index:0})');
  assert.equal(elements.get('folder-panel').hidden,false,'Multiple images show folder navigation');
  run('collection={...folder,entries:[folder.entries[0]]};renderCollection()');
  assert.equal(elements.get('folder-panel').hidden,true,'A single photo does not show a redundant folder row');
  assert.equal(run('session.id'),'a','Hiding a single-photo folder row preserves the open document');
  run('collection=folder;renderCollection()');
  assert.equal(elements.get('folder-panel').hidden,false,'Folder navigation returns for multiple images');
  run(`mask.paints=7;hasSelection=true;undo=[mask.toDataURL('image/png')];points=[{x:120,y:160},{x:280,y:240}];tool='pen';setPhotoZoom(.8);panX=-420;panY=-310;applyCamera();`);
  const aState=state();
  await run('openCollectionEntry(1)');assert.equal(run('session.id'),'b');assert.equal(run('hasSelection'),false);assert.deepEqual(state().points,[]);
  run('mask.paints=3;hasSelection=true;tool="brush";setPhotoZoom(1.5)');
  await run('openCollectionEntry(0)');assert.equal(run('session.id'),'a');assert.equal(state().paint,7);assert.equal(run('undo.length'),1);
  assert.deepEqual(state().points,aState.points);nearly(state().zoom,aState.zoom);nearly(state().panX,aState.panX);nearly(state().panY,aState.panY);
  assert.equal(run('navRequests.some(request=>request.path.endsWith("/save"))'),false,'Changing images never writes a source file');
  run('displayCache.delete("b")');failedImages.add('/b/base-display');await run('openCollectionEntry(1)');failedImages.clear();
  assert.equal(run('session.id'),'a');assert.equal(run('collectionIndex'),0);assert.equal(state().paint,7);assert.deepEqual(state().points,aState.points);
  assert.match(elements.get('message').textContent,/Could not open/,'A failed navigation leaves the current edit intact');
  run('setBusy(true)');assert.equal(elements.get('folder-next').disabled,true);assert.equal(elements.get('save-unique').disabled,true);
  const lockedRequests=run('navRequests.length');await run('openCollectionEntry(1)');assert.equal(run('navRequests.length'),lockedRequests);run('setBusy(false)');

  // Merge and all save modes preserve selection/view; overwrite always retains the source format.
  const revisionBeforeMerge=run('session.revision');await elements.get('merge').onclick();
  assert.equal(run('navRequests.at(-1).body.revision'),revisionBeforeMerge);assert.equal(run('session.layers.at(-1).model_label'),'Merged visible');
  assert.equal(state().paint,7);assert.deepEqual(state().points,aState.points);nearly(state().zoom,aState.zoom);
  elements.get('output-format').value='png';elements.get('output-format').onchange();
  assert.match(elements.get('output-format').options.find(option=>option.value==='png').textContent,/8-bit/);
  await run('save("unique")');assert.equal(run('navRequests.at(-1).body.mode'),'unique');assert.equal(run('navRequests.at(-1).body.format'),'png');assert.equal(state().paint,7);
  // All overwrite entry points pause before any write; cancel and dialog keys retain the working selection.
  assert.match(html,/>Overwrite original…<\/span>/);assert.match(html,/>Save a copy<\/span>/);
  run('loadOverwritePreference()');assert.equal(run('askBeforeOverwrite'),true);
  const beforeConfirm=run('navRequests.length'),selectionBeforeConfirm=state();
  const pendingCancel=elements.get('document-save').onclick();
  assert.equal(elements.get('overwrite-dialog').open,true);assert.equal(run('busy'),false,'A pending question is not an editing operation');
  assert.equal(run('navRequests.length'),beforeConfirm,'The document Overwrite button cannot write before confirmation');
  assert.equal(elements.get('overwrite-filename').textContent,run('session.source_name'));
  assert.equal(document.activeElement,elements.get('overwrite-cancel'),'Cancel receives the initial focus');
  await run('save("overwrite")');await run('save("unique")');assert.equal(run('navRequests.length'),beforeConfirm,'Repeated actions cannot bypass the dialog');
  const modalZoom=state().zoom,modalSize=elements.get('size').value;
  for(const [code,keyName] of [['BracketRight',']'],['KeyF','f'],['Enter','Enter'],['KeyJ','j'],['KeyB','b'],['PageDown','PageDown'],['Space',' '],['Escape','Escape']])key(code,keyName);
  const modalSave=document.fire('keydown',{target:viewport,code:'KeyS',key:'s',ctrlKey:true});assert.equal(modalSave.prevented,true,'Ctrl+S also suppresses the browser Save Page command while a dialog is open');
  document.fire('keydown',{target:viewport,code:'KeyO',key:'o',ctrlKey:true});
  viewport.fire('wheel',{clientX:400,clientY:300,deltaY:100});pointer('pointerdown',400,300);pointer('pointermove',450,320);pointer('pointerup',450,320);
  document.fire('drop',{dataTransfer:{types:['Files'],files:[{name:'unopened.png'}]}});
  assert.equal(run('navRequests.length'),beforeConfirm,'Keyboard, canvas, and dropped files cannot edit or navigate behind the modal');
  assert.equal(state().zoom,modalZoom);assert.equal(elements.get('size').value,modalSize);assert.equal(state().paint,selectionBeforeConfirm.paint);assert.deepEqual(state().points,selectionBeforeConfirm.points);assert.equal(run('spaceHeld'),false);
  elements.get('overwrite-dont-ask').checked=true;elements.get('overwrite-cancel').onclick();await pendingCancel;
  assert.equal(elements.get('overwrite-dialog').open,false);assert.equal(run('askBeforeOverwrite'),true);assert.equal(storage.has('local-remove-ask-before-overwrite'),false,'Cancel must not persist a checked opt-out');
  assert.equal(state().paint,selectionBeforeConfirm.paint);assert.deepEqual(state().points,selectionBeforeConfirm.points);

  const pendingEscape=elements.get('return').onclick();key('Escape','Escape');elements.get('overwrite-dialog').fire('cancel');await pendingEscape;
  assert.equal(run('navRequests.length'),beforeConfirm,'Escape cancels without a write');assert.deepEqual(state().points,selectionBeforeConfirm.points,'Escape dismisses the question without clearing a pen path');

  const pendingUnique=run('save("overwrite")');elements.get('overwrite-dont-ask').checked=true;elements.get('overwrite-unique').onclick();await pendingUnique;
  assert.equal(run('navRequests.at(-1).body.mode'),'unique');assert.equal(run('navRequests.at(-1).body.format'),'png','Save Unique in the warning uses the selected copy format');
  assert.equal(run('askBeforeOverwrite'),true);assert.equal(storage.has('local-remove-ask-before-overwrite'),false,'Choosing a copy must not opt out of future overwrite questions');
  assert.equal(state().paint,selectionBeforeConfirm.paint);assert.deepEqual(state().points,selectionBeforeConfirm.points);

  const beforeShortcutConfirm=run('navRequests.length');
  document.fire('keydown',{target:viewport,code:'KeyS',key:'s',ctrlKey:true});
  assert.equal(elements.get('overwrite-dialog').open,true);assert.equal(run('navRequests.length'),beforeShortcutConfirm,'Ctrl+S uses the same confirmation gate');
  elements.get('overwrite-confirm').onclick();await new Promise(resolve=>setImmediate(resolve));
  assert.equal(run('navRequests.length'),beforeShortcutConfirm+1);assert.equal(run('navRequests.at(-1).body.format'),'original');assert.equal(run('navRequests.at(-1).body.revision'),run('session.revision'));
  assert.equal(run('askBeforeOverwrite'),true,'Confirming without the checkbox continues asking');

  const pendingOptOut=run('save("overwrite")');elements.get('overwrite-dont-ask').checked=true;elements.get('overwrite-confirm').onclick();await pendingOptOut;
  assert.equal(storage.get('local-remove-ask-before-overwrite'),'false');assert.equal(elements.get('ask-before-overwrite').checked,false);
  run('askBeforeOverwrite=true;loadOverwritePreference()');assert.equal(run('askBeforeOverwrite'),false,'The opt-out survives reloading saved preferences');
  const beforeOptOut=run('navRequests.length');await run('save("overwrite")');assert.equal(run('navRequests.length'),beforeOptOut+1);assert.equal(elements.get('overwrite-dialog').open,false,'An explicit opt-out allows direct overwrite');
  elements.get('ask-before-overwrite').checked=true;elements.get('ask-before-overwrite').onchange();run('loadOverwritePreference()');assert.equal(run('askBeforeOverwrite'),true);
  const pendingReenabled=run('save("overwrite")');assert.equal(elements.get('overwrite-dialog').open,true,'Settings can re-enable overwrite questions');elements.get('overwrite-cancel').onclick();await pendingReenabled;
  run('saveFailure=true');await run('save("unique")');run('saveFailure=false');
  assert.equal(state().paint,7);assert.deepEqual(state().points,aState.points);assert.equal(run('busy'),false);assert.match(elements.get('message').textContent,/still available/);
  run('session={...session,id:"upload",can_return:false};collection=null');
  const beforeInvalidSave=run('navRequests.length');await run('save("overwrite")');assert.equal(run('navRequests.length'),beforeInvalidSave);
  elements.get('output-format').value='webp';elements.get('output-format').onchange();await run('save("export")');
  assert.equal(run('navRequests.at(-1).body.return_to_source'),false);assert.equal(run('navRequests.at(-1).body.format'),'webp');

  // Native messages correlate responses and forward File objects, never a page-supplied filesystem path.
  const hostListeners=[],hostRequests=[];let hostResponse={collection:{id:'chosen',entries:[]},index:0};
  window.chrome={webview:{
    addEventListener:(type,listener)=>hostListeners.push(listener),
    postMessage:request=>{hostRequests.push({request});for(const listener of hostListeners)listener({data:{type:'local-remove-native',id:request.id,result:request.action==='ready'?{native:true,version:1}:hostResponse}});},
    postMessageWithAdditionalObjects:(request,files)=>{hostRequests.push({request,files});for(const listener of hostListeners)listener({data:{type:'local-remove-native',id:request.id,result:hostResponse}});}
  }};
  await run('connectNative()');assert.equal(run('nativeReady'),true);
  context.droppedFiles=[{name:'photo.tif'}];await run('nativeRequest("drop",droppedFiles)');
  assert.equal(hostRequests.at(-1).files,context.droppedFiles);assert.deepEqual(Object.keys(hostRequests.at(-1).request).sort(),['action','id']);
  hostResponse=null;assert.equal(await run('nativeRequest("openFolder")'),null,'Picker cancellation is handled without navigation');
  run('globalThis.dropCalls=[];openNative=async(action,files)=>dropCalls.push({kind:"native",action,names:files?.map(file=>file.name)});openBrowserFiles=async(files)=>dropCalls.push({kind:"browser",names:files.map(file=>file.name)});');
  document.fire('drop',{dataTransfer:{types:['Files'],files:context.droppedFiles}});assert.equal(run('dropCalls.at(-1).kind'),'native');
  run('nativeReady=false');document.fire('drop',{dataTransfer:{types:['Files'],files:context.droppedFiles}});assert.equal(run('dropCalls.at(-1).kind'),'browser');
  const drops=run('dropCalls.length');run('setBusy(true)');document.fire('drop',{dataTransfer:{types:['Files'],files:context.droppedFiles}});assert.equal(run('dropCalls.length'),drops);run('setBusy(false)');

  run('globalThis.shortcuts=[];save=async(mode)=>shortcuts.push(mode);navigateCollection=direction=>shortcuts.push(direction);session.can_return=true;collection=folder;');
  document.fire('keydown',{target:viewport,code:'KeyS',key:'s',ctrlKey:true});
  document.fire('keydown',{target:viewport,code:'KeyS',key:'s',ctrlKey:true,shiftKey:true});
  document.fire('keydown',{target:viewport,code:'ArrowRight',key:'ArrowRight',altKey:true});
  document.fire('keydown',{target:viewport,code:'PageUp',key:'PageUp'});
  assert.deepEqual(JSON.parse(run('JSON.stringify(shortcuts)')),['overwrite','unique',1,-1]);
  run('retouchReady=true;setOperation("heal");points=[{x:20,y:30},{x:60,y:70}];hasSelection=true;');
  const methodSelection=state().points;
  elements.get('heal-method').value='telea';elements.get('heal-method').onchange();
  assert.equal(run('healMethod'),'telea');assert.deepEqual(state().points,methodSelection,'Changing heal method preserves selection');
  const maskBeforeMethodKey=state().paint;key('BracketRight',']',elements.get('heal-method'));assert.equal(state().paint,maskBeforeMethodKey,'Method dropdown keys do not paint');
  run('setBusy(true)');assert.equal(elements.get('heal-method').disabled,true);run('setBusy(false)');
  run('points=[];showOriginal=false;hasSelection=true;controls()');
  assert.equal(run('workflowState().status'),'Selection ready');
  assert.equal(storage.get('local-remove-operation'),'heal','Explicit method choice is remembered');
  run('setOperation("ai");ready=false;controls()');
  assert.equal(run('workflowState().status'),'AI connection needed');
  assert.equal(elements.get('remove').disabled,true);
  assert.equal(run('hasSelection'),true,'Unavailable AI preserves the selection');
  run('setOperation("heal");showOriginal=true;controls()');
  assert.equal(run('workflowState().status'),'Original view');
  assert.equal(elements.get('before').textContent,'Back to edits');
  assert.equal(elements.get('remove').disabled,true,'Original comparison cannot apply edits');
  key('Backslash','\\');assert.equal(run('showOriginal'),false);
  run('activeTask="repair";setBusy(true)');
  assert.equal(run('workflowState().title'),'Repairing your selection');
  assert.equal(elements.get('viewport').attributes['aria-busy'],'true');
  run('activeTask=null;setBusy(false)');
  run('showOriginal=false;points=[{x:10,y:10},{x:20,y:20}];controls();selectTool("rectangle")');
  assert.notEqual(run('workflowState().title'),'Finish your selection','Changing tools clears obsolete path guidance immediately');
  run('models=models.filter(model=>model.id!=="heal");models.push({id:"heal",methods:[{id:"texture",available:false},{id:"telea",available:true}]});healMethod="texture";hasSelection=true;controls()');
  assert.equal(elements.get('remove').disabled,true,'A missing texture helper is unavailable before submission');
  assert.match(run('workflowState().description'),/Dust & scratches/,'Unavailable texture offers the installed alternative');
  run('healMethod="telea";controls()');
  assert.equal(elements.get('remove').disabled,false,'The installed dust repair method remains available');
  console.log('PASS: camera/pan/pen and AI/Heal regressions; native-pixel brush ring and bracket keys; two-image selection/undo/view preservation; navigation failure and busy guards; merge preservation; unique/overwrite/export formats and save errors; overwrite confirmation across buttons/keyboard, Cancel/Escape, modal isolation, copy routing, opt-out persistence and Settings reset; native File-object bridge; browser drop fallback; folder/save shortcuts.');
}
main().catch(error=>{console.error(error);process.exitCode=1;});
