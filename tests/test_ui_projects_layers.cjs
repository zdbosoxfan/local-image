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
const document=new Emitter();
document.body=new Element('body');
document.getElementById=id=>elements.get(id);
document.createElement=tag=>new Element(tag);
document.createElementNS=(namespace,tag)=>new Element(tag);
document.createTextNode=text=>({textContent:text});
document.querySelectorAll=selector=>selector==='[data-tool]'?buttons:selector==='[data-command]'?[...elements.values()].filter(element=>element.dataset.command):[];
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

const tick=()=>new Promise(resolve=>setImmediate(resolve));
async function main(){
  const host=new Element(),anchor=new Element(),sibling=new Element();
  anchor.before(sibling);assert.equal(sibling.parent,undefined,'before() on a detached node leaves siblings detached');
  host.append(anchor,sibling);anchor.before(sibling);
  assert.deepEqual(host.children,[sibling,anchor],'before() moves an existing sibling rather than duplicating it');
  assert.equal(sibling.parent,host);
  const assets=[];
  context.mockLoadImage=async src=>{assets.push(src);return Object.assign(new Element('img'),{src,width:4000,height:2667,naturalWidth:4000,naturalHeight:2667});};
  run(`loadImage=mockLoadImage;globalThis.calls=[];globalThis.server={id:'one',name:'Drone.jpg',width:4000,height:2667,bit_depth:8,revision:2,layers:[
    {id:'first',name:'Remove 1',x:100,y:200,width:800,height:600,visible:true,discarded:false},
    {id:'second',name:'Heal 1',x:180,y:250,width:100,height:80,visible:false,discarded:true}],dirty:false,saved_revision:2};
    globalThis.releasePatch=null;globalThis.failNext=false;
    json=async(path,body,method)=>{
      calls.push({path,body,method});
      if(method==='PATCH'){
        await new Promise(resolve=>releasePatch=resolve);
        if(failNext){failNext=false;throw Error('Offline');}
        if(body.revision!==server.revision)throw Error('Stale revision');
        const layer=server.layers.find(item=>path.endsWith('/'+item.id));for(const key of ['visible','discarded'])if(key in body)layer[key]=body[key];
        server.revision++;return cloneDocument(server);
      }
      if(path.endsWith('/close-sessions'))return {closed:body.sessions.map(item=>item.id)};
      if(path.endsWith('/export-project'))return {download:'/project-download',name:'Drone.lremove',session:cloneDocument(server)};
      throw Error('Unexpected route '+path);
    };
    api=async path=>({json:async()=>path.endsWith('/sessions')?[]:cloneDocument(server)});`);
  await run('openSession(cloneDocument(server))');
  assert.equal(assets.length,3,'Initial open loads the original and each layer once, including restorable layers');
  assert.ok(assets.every(src=>!src.includes('/preview')),'No flattened JPEG display requests remain');
  const layerNodes=elements.get('layer-stack').children;
  assert.equal(layerNodes.length,2);assert.equal(layerNodes[0].hidden,false);assert.equal(layerNodes[1].hidden,true);
  nearly(parseFloat(layerNodes[0].style.left),75);nearly(parseFloat(layerNodes[0].style.width),600);
  const originalLayerRow=elements.get('layers').children[0];
  run('changeLayer("first",{visible:false})');
  assert.equal(layerNodes[0].hidden,true,'The layer hides before the request finishes');
  assert.equal(elements.get('merge').disabled,true,'A merge cannot use unpersisted visibility');
  assert.equal(run('busy'),false,'Visibility persistence does not make the whole editor busy');
  run('changeLayer("first",{visible:true});changeLayer("first",{visible:false})');
  assert.equal(layerNodes[0].hidden,true);assert.equal(run('calls.length'),1,'Mutations persist serially');
  run('releasePatch()');await tick();assert.equal(run('calls.length'),2);assert.equal(layerNodes[0].hidden,true,'The first response cannot overwrite the latest requested visibility');
  run('releasePatch()');await tick();assert.equal(run('calls.length'),3);assert.equal(layerNodes[0].hidden,true);
  run('releasePatch()');await run('flushLayerChanges()');
  assert.equal(run('session.revision'),5);assert.equal(run('session.layers[0].visible'),false);
  assert.equal(assets.length,3,'Repeated visibility toggles fetch zero image assets');
  assert.equal(elements.get('layers').children[0],originalLayerRow,'Visibility changes retain focused layer rows');
  assert.equal(elements.get('layer-stack').children[0],layerNodes[0],'Visibility changes retain decoded DOM images');
  run('failNext=true;changeLayer("first",{visible:true})');assert.equal(layerNodes[0].hidden,false);
  run('releasePatch()');await run('flushLayerChanges()');assert.equal(layerNodes[0].hidden,true,'Failed persistence restores the server state');
  assert.match(elements.get('message').textContent,/failed/);
  run('failNext=true;changeLayer("first",{visible:true});changeLayer("first",{visible:true});releasePatch()');await tick();
  assert.equal(layerNodes[0].hidden,false,'A failed earlier request does not erase a later intent');
  run('releasePatch()');await run('flushLayerChanges()');assert.equal(run('session.layers[0].visible'),true);
  run('changeLayer("second",{discarded:false,visible:true})');assert.equal(layerNodes[1].hidden,false,'Restore uses its already decoded image');
  run('releasePatch()');await run('flushLayerChanges()');assert.equal(assets.length,3);
  assert.equal(elements.get('layers').children[0].hidden,false,'Restored layer row becomes visible without rebuilding the list');
  run('showOriginal=true;paintPhoto()');assert.equal(elements.get('layer-stack').hidden,true);
  run('showOriginal=false;paintPhoto()');assert.equal(elements.get('layer-stack').hidden,false);assert.equal(assets.length,3);
  await run('refreshPreview()');assert.equal(assets.length,3,'Refresh after metadata changes reuses assets');
  run('session.layers.push({id:"new",name:"Merged",x:0,y:0,width:4000,height:2667,visible:true});session.revision++;');
  await run('refreshPreview()');assert.equal(assets.length,4,'A merge or generation loads only the newly added layer');

  // Explicit close always protects editable layers, including after a flattened image save.
  run('server=cloneDocument(session);session.dirty=false;session.saved_revision=session.revision;trackDocument(session);');
  const callsBeforeClose=run('calls.length');let closing=run('closeCurrentImage()');await tick();
  assert.equal(elements.get('close-dialog').open,true);assert.match(elements.get('close-warning').textContent,/permanently clear/);
  assert.equal(run('calls.length'),callsBeforeClose);elements.get('close-cancel').click();assert.equal(await closing,false);
  assert.equal(run('session.id'),'one');assert.equal(run('session.layers.length'),3);
  run('points=[{x:30,y:40}];hasSelection=true;');closing=run('closeCurrentImage()');await tick();
  assert.equal(elements.get('close-selection-warning').hidden,false,'The dialog explicitly warns that project files omit pending selections');
  elements.get('close-dialog').fire('cancel');assert.equal(await closing,false);assert.equal(run('points.length'),1);

  // Download completion is not observable in a browser, so saving a download cannot discard automatically.
  closing=run('closeCurrentImage()');await tick();elements.get('close-save').click();assert.equal(await closing,false);
  assert.equal(run('session.id'),'one');assert.equal(run('openDocuments.size'),1);assert.match(elements.get('message').textContent,/Finish saving/);
  assert.equal(run('calls.filter(call=>call.path.endsWith("/close-sessions")).length'),0);

  // Native save paths are private to the host. Save all documents before one atomic close call.
  const nativeRequests=[];let cancelSecond=true;
  context.mockNative=async(action,files,details)=>{
    nativeRequests.push({action,...details});
    if(details.session_id==='two'&&cancelSecond)return null;
    return run(`(()=>{const data=cloneDocument(openDocuments.get(${JSON.stringify(details.session_id)}));data.project_saved=true;data.project_dirty=false;data.project_saved_revision=data.revision;data.project_name=data.name+'.lremove';return{saved:true,name:data.project_name,session:data};})()`);
  };
  run('nativeProjects=true;nativeRequest=mockNative;trackDocument({...cloneDocument(session),id:"two",name:"Second.jpg",revision:1});');
  closing=run('closeDocuments([...openDocuments.values()],true)');await tick();elements.get('close-save').click();assert.equal(await closing,false);
  assert.equal(run('openDocuments.size'),2,'Cancellation during the second project picker retains every working document');
  assert.equal(run('calls.filter(call=>call.path.endsWith("/close-sessions")).length'),0);
  assert.equal(run('openDocuments.get("one").project_saved'),true,'The first project remains safely saved when a later picker is cancelled');
  assert.ok(nativeRequests.every(request=>!('path' in request)),'No page-provided filesystem path reaches the save bridge');
  cancelSecond=false;closing=run('closeDocuments([...openDocuments.values()],true)');await tick();elements.get('close-save').click();assert.equal(await closing,true);
  assert.equal(run('session'),null);assert.equal(run('openDocuments.size'),0);assert.equal(run('displayCache.size'),0);assert.equal(run('viewStates.size'),0);
  assert.equal(run('calls.filter(call=>call.path.endsWith("/close-sessions")).length'),1);
  assert.deepEqual(JSON.parse(run('JSON.stringify(calls.at(-1).body.sessions.map(item=>item.id))')),['one','two']);
  assert.equal(elements.get('document-close').hidden,true);

  // Save As reaches the host explicitly; ordinary saves use its existing private project path.
  run('session=cloneDocument(server);trackDocument(session);');
  await run('saveEditableProject(session,{saveAs:true})');assert.equal(nativeRequests.at(-1).saveAs,true);
  await run('saveEditableProject()');assert.equal(nativeRequests.at(-1).saveAs,undefined);

  const unload=window.fire('beforeunload');assert.equal(unload.prevented,true,'Browser closing still prompts when working layers are open');
  const hostMessages=[];context.mockCloseBridge={postMessage:data=>hostMessages.push(data)};run('nativeBridge=mockCloseBridge;');
  closing=run('handleNativeClose("cancel-native")');await tick();elements.get('close-cancel').click();await closing;
  assert.equal(hostMessages.at(-1).approved,false);assert.equal(hostMessages.at(-1).id,'cancel-native');
  closing=run('handleNativeClose("close-native")');await tick();elements.get('close-discard').click();await closing;
  assert.equal(hostMessages.at(-1).approved,true);assert.equal(run('openDocuments.size'),0);
  console.log('PASS: immutable base/patch caching; native-resolution patch coordinates; immediate visibility; rapid serialized intents; stale-response protection; failure rollback and replay; no toggle asset reloads or row replacement; restored layers; compare original; new-layer-only loading; close after flattened save warning; Cancel/Escape preserves selection/layers; browser download keeps documents; native multi-document save/cancel/batch discard; private native paths; Save As; application close handshake.');
}
main().catch(error=>{console.error(error);process.exitCode=1;});
