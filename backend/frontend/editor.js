'use strict';
const TOKEN='__TOKEN__';
const $=id=>document.getElementById(id);
const viewport=$('viewport'),stage=$('stage');
const mask=document.createElement('canvas'),mc=mask.getContext('2d',{willReadFrequently:true});
const baseCanvas=$('photo'),bc=baseCanvas.getContext('2d');
const overlay=$('selection'),oc=overlay.getContext('2d');
const draft=$('draft'),dc=draft.getContext('2d');
let session=null,tool='brush',subtract=false,busy=false,ready=false,showOriginal=false,previewImage=null,originalImage=null;
let points=[],undo=[],hasSelection=false,requestVersion=0;
let zoom=1,panX=0,panY=0,fitMode=true,spaceHeld=false,gesture=null,viewportWidth=0,viewportHeight=0;
let modelId='klein',models=[],settingsLoaded=false,settingsSaving=false,handActive=false,menuOpen=null,operation='heal',retouchReady=false;
let activeTask=null;
let collection=null,collectionIndex=-1,nativeReady=false,nativeBridge=null,nativeRequestNumber=0,dragDepth=0,brushPointer=null,saveInProgress=false,outputFormat='original';
let healMethod='texture',askBeforeOverwrite=true,overwritePrompt=null;
const OVERWRITE_PREFERENCE='local-remove-ask-before-overwrite';
let closePrompt=null,closeInProgress=false,nativeProjects=false;
const modalOpen=()=>$('settings-dialog').open||$('overwrite-dialog').open||$('close-dialog').open||!!overwritePrompt||!!closePrompt;
const viewStates=new Map(),nativePending=new Map(),openDocuments=new Map(),displayCache=new Map(),layerQueues=new Map();
const cloneDocument=data=>JSON.parse(JSON.stringify(data));
const layerChangesPending=()=>[...layerQueues.values()].some(queue=>queue.pending.length>0);
function trackDocument(data){if(data?.id)openDocuments.set(data.id,data);}

const BRUSH_STEPS=[1,2,3,4,5,6,7,8,9,10,12,15,20,25,30,35,40,45,50,60,70,80,90,100,125,150,175,200,250,300,400,500,600,800,1000,1200,1500,2000];
const MIN_PHOTO_ZOOM=.01,MAX_PHOTO_ZOOM=8;

async function api(path,options={}){
  const response=await fetch(path,{...options,headers:{'x-local-remove-token':TOKEN,...options.headers}});
  if(!response.ok){let data;try{data=await response.json();}catch{}throw Error(data?.detail||`Request failed (${response.status})`);}
  return response;
}
async function json(path,body,method='POST'){
  return(await api(path,{method,headers:{'Content-Type':'application/json'},body:JSON.stringify(body)})).json();
}
const url=tail=>'/api/local-remove/session/'+session.id+tail;
const currentModel=()=>models.find(model=>model.id===modelId);
const modelLabel=()=>currentModel()?.label||(modelId==='qwen'?'Qwen removal':'FLUX Klein');
const selectedHealMethod=()=>models.find(model=>model.id==='heal')?.methods?.find(method=>method.id===healMethod);
const operationReady=()=>operation==='heal'?retouchReady&&selectedHealMethod()?.available!==false:ready&&settingsLoaded&&currentModel()?.available!==false;
function message(text,error=false){$('message').textContent=text;$('message').title=text;$('message').classList.toggle('error',error);if(activeTask==='repair'&&$('progress-label'))$('progress-label').textContent=text;}
function settingsMessage(text,error=false){$('settings-message').textContent=text;$('settings-message').classList.toggle('error',error);}
function setBusy(value){busy=value;if(value)endGesture();controls();}
function controls(){
  const active=!!session&&!busy&&!layerChangesPending();
  for(const id of ['clear','undo','add','subtract','before','save','return','save-unique','save-project','save-project-as','close-image','merge','size','finish'])$(id).disabled=!active;
  for(const id of ['zoom','zoom-in','zoom-out','fit','actual-size','hand'])$(id).disabled=!session;
  for(const button of document.querySelectorAll('[data-tool]'))button.disabled=!active;
  $('open').disabled=busy||closeInProgress;
  $('open-project').disabled=busy||closeInProgress;
  $('document-close').hidden=!session;
  $('open-folder').disabled=busy||!nativeReady;
  $('open-folder').title=nativeReady?'Open a folder of images':'Open the Local Remove desktop app to browse a folder';
  $('settings').disabled=busy||settingsSaving;
  $('model-shortcut').disabled=busy||settingsSaving;
  $('heal-method').disabled=busy||settingsSaving||!session;
  $('mode-ai').disabled=busy||settingsSaving;$('mode-heal').disabled=busy||settingsSaving;
  $('heal-brush').disabled=!active;
  $('model').disabled=busy||settingsSaving||!settingsLoaded;
  $('remove').disabled=!active||!operationReady()||!hasSelection||showOriginal||settingsSaving;
  $('remove').classList.toggle('busy',busy);
  $('remove').textContent=busy?'Working…':operation==='heal'?'Heal selection':'Remove selection';
  $('edit-action-label').textContent=operation==='heal'?'Heal selected area':'Remove selected area';
  $('undo').disabled=!active||(!undo.length&&!points.length);
  $('return').hidden=!session?.can_return;
  $('save-unique').hidden=!session?.can_return;
  $('document-save').hidden=!session?.can_return;$('document-unique').hidden=!session?.can_return;
  $('document-export').hidden=!!session?.can_return;
  $('browser-note').hidden=nativeReady;
  $('folder-previous').disabled=busy||!collection||collectionIndex<=0;
  $('folder-next').disabled=busy||!collection||collectionIndex>=collection.entries.length-1;
  for(const button of document.querySelectorAll('[data-folder-entry]'))button.disabled=busy;
  for(const button of document.querySelectorAll('#recent button,#layers button,#layers input'))button.disabled=busy;
  $('restore').disabled=busy;
  for(const command of document.querySelectorAll('[data-command]'))command.disabled=$(command.dataset.command).disabled;
  updateDocumentState();updateBrushCursor();updateWorkflowChrome();
}

// One derived view of the editing workflow keeps guidance, readiness, and the
// action in agreement. It never changes the user's selection or chosen method.
function workflowState(){
  if(!session)return {title:'Start with a photo',description:'Open a photo, select a distraction, then apply a repair.',status:'No photo open'};
  if(busy)return {title:activeTask==='repair'?'Repairing your selection':'Updating your photo',description:activeTask==='repair'?'Your repair will appear as a new, editable layer.':'Please wait a moment.',status:'Working…'};
  if(showOriginal)return {title:'Viewing the original',description:'Choose Back to edits to continue. Your selection and layers are kept.',status:'Original view'};
  if(points.length)return {title:'Finish your selection',description:'Click the first point or press Enter to close the path. Escape cancels the path.',status:points.length+' path points'};
  if(!operationReady())return operation==='heal'
    ?{title:'Quick Heal unavailable',description:selectedHealMethod()?.available===false?'Texture repair is not installed. Choose Dust & scratches, or reinstall Local Remove to restore texture repair.':'The local healing service is unavailable. Check the connection status and try again.',status:'Service unavailable'}
    :{title:'Connect AI Remove',description:'Open AI settings to check your local connection and model. Quick Heal works without an AI model.',status:'AI connection needed'};
  if(hasSelection)return {title:operation==='heal'?'Ready to heal':'Ready to remove',description:'Use Add or Subtract to refine the highlighted area, then apply the repair.',status:'Selection ready'};
  if(session.layers?.some(layer=>!layer.discarded))return {title:'Review your repair',description:'Compare with the original, or hide a layer to check its effect. Select another area to keep editing.',status:'Select another area'};
  const selectionHint=handActive?'Drag to move around the photo. Choose a selection tool to mark a distraction.':tool==='pen'?'Click around the distraction; press Enter to close the path.':tool==='rectangle'||tool==='ellipse'?'Drag around the distraction, including its shadow. Use Add or Subtract to refine the area.':operation==='heal'?healHint():'Brush over the whole object, including its shadow. Your local model rebuilds the selected area.';
  return {title:handActive?'Move around your photo':'Select a distraction',description:selectionHint,status:'No selection yet'};
}
function updateWorkflowChrome(){
  const state=workflowState();
  const photoMeta=session?session.width+' × '+session.height+(session.bit_depth?' · '+session.bit_depth+'-bit':''):'';
  const labels={'workflow-title':state.title,'workflow-description':state.description,'workflow-status':state.status,'selection-status':hasSelection?'Selection ready':points.length?'Path in progress':'No selection','photo-meta':photoMeta};
  for(const [id,value] of Object.entries(labels))if($(id))$(id).textContent=value;
  if($('workflow-settings'))$('workflow-settings').hidden=operation!=='ai'||operationReady()||busy;
  if($('progress-strip'))$('progress-strip').hidden=activeTask!=='repair';
  viewport.setAttribute('aria-busy',String(busy));
  $('before').textContent=showOriginal?'Back to edits':'Original';
  $('before').title=showOriginal?'Return to edited photo (\\)':'Compare with the original (\\)';
  $('remove').title=!session?'Open a photo first':showOriginal?'Return to edited view first':!operationReady()?state.description:!hasSelection?'Select an area on the photo first':'Apply as a new editable layer';
}
function loadImage(src){return new Promise((resolve,reject)=>{const image=new Image();image.onload=()=>resolve(image);image.onerror=()=>reject(Error('Could not load the photo'));image.src=src;});}

function isDirty(data=session){return !!data&&(typeof data.dirty==='boolean'?data.dirty:data.revision>(data.saved_revision??0));}
function updateDocumentState(){
  const pending=hasSelection||points.length>0;
  const status=!session?'':layerChangesPending()?'Updating layers…':saveInProgress?'Saving…':isDirty()?'Modified':pending?'Selection pending':session.saved_name?(session.saved_name!==session.source_name?'Copy saved':'Saved'):'Original';
  $('document-state').textContent=status;$('document-state').classList.toggle('dirty',!!session&&(isDirty()||pending));
  $('save-project').title=session?.project_name?'Save editable layers · '+session.project_name:'Save the original and editable layers in a .lremove project';
  $('document-state').title=pending?'The current selection has not been applied. Use Remove or Heal before saving.':session?.saved_name?'Last saved: '+session.saved_name:status;
  const suffix=(session?.source_name||session?.name||'').match(/\.[a-z0-9]+$/i)?.[0]||'source';
  $('document-save').title='Overwrite the original '+suffix+' image (Ctrl+S). Keeps its original format.';
  $('return').title='Overwrite the source image in its original '+suffix+' format.';
  $('document-unique').title='Save a new '+(outputFormat==='original'?suffix:outputFormat.toUpperCase())+' copy beside the source (Ctrl+Shift+S).';
  $('output-format').disabled=busy;
  for(const option of $('output-format').options){
    const labels={original:'Original',png:'PNG',jpg:'JPEG',tif:'TIFF',webp:'WebP'};
    option.textContent=labels[option.value]+(session?.bit_depth===16&&option.value!=='original'?(option.value==='tif'?' (16-bit)':' (8-bit)'):'');
  }
  $('format-hint').textContent='Save a copy and Export use this format. Overwrite keeps '+suffix+'.'+(session?.bit_depth===16&&['png','jpg','webp'].includes(outputFormat)?' This copy will be 8-bit.':'');
}
$('output-format').onchange=()=>{outputFormat=$('output-format').value;try{localStorage.setItem('local-remove-copy-format',outputFormat);}catch{}updateDocumentState();};

// Keep selections and view state per image; compressed snapshots avoid retaining full raw pixel buffers.
function rememberCurrentView(){
  if(!session)return;
  trackDocument(session);endGesture();
  viewStates.set(session.id,{mask:hasSelection?mask.toDataURL('image/png'):null,undo:[...undo],points:points.map(point=>({...point})),
    width:mask.width,height:mask.height,tool,subtract,operation,brushSize:Number($('size').value),showOriginal,handActive,
    photoZoom:photoZoom(),panX,panY,fitMode,viewportWidth:viewport.clientWidth,viewportHeight:viewport.clientHeight,hasSelection});
}
function restoreView(state,selectionImage){
  if(!state)return;
  if(selectionImage){mc.globalCompositeOperation='source-over';mc.drawImage(selectionImage,0,0,mask.width,mask.height);}
  undo=[...state.undo];points=state.points.map(point=>({...point}));tool=state.tool;operation=state.operation;showOriginal=state.showOriginal;handActive=state.handActive;
  selectionMode(state.subtract);setBrushSize(state.brushSize);hasSelection=state.hasSelection;
  $('finish').hidden=!points.length;updateToolChrome();paintMask();penDraft();
  if(state.fitMode)fitImage();
  else{zoom=state.photoZoom*pixelRatio();panX=state.panX+(viewport.clientWidth-state.viewportWidth)/2;panY=state.panY+(viewport.clientHeight-state.viewportHeight)/2;fitMode=false;applyCamera();}
}
function collectionEntryState(entry){
  const state=entry.session_id===session?.id?{hasSelection,points}:viewStates.get(entry.session_id);
  if(entry.dirty)return 'Modified';
  if(state?.hasSelection||state?.points?.length)return 'Selection pending';
  if(entry.saved)return entry.saved_name&&entry.saved_name!==entry.name?'Copy saved':'Saved';
  return entry.edited?'Edited':'Original';
}
function renderCollection(scrollActive=false){
  $('folder-panel').hidden=!collection;
  if(!collection){$('filmstrip').replaceChildren();return;}
  $('folder-name').textContent=collection.name;$('folder-name').title=collection.local?'Browser upload copies; export downloads a new file.':collection.name;
  $('folder-count').textContent=(collectionIndex+1)+' / '+collection.entries.length;
  $('filmstrip').hidden=collection.entries.length<2;
  const strip=$('filmstrip');strip.replaceChildren();
  collection.entries.forEach((entry,index)=>{
    const button=document.createElement('button');button.dataset.folderEntry=entry.id;button.disabled=busy;
    button.setAttribute('aria-current',String(index===collectionIndex));button.title=entry.name+' — '+collectionEntryState(entry);
    if(!collection.local){const thumbnail=document.createElement('img');thumbnail.alt='';thumbnail.loading='lazy';thumbnail.decoding='async';
      thumbnail.src='/api/local-remove/collection/'+encodeURIComponent(collection.id)+'/entry/'+encodeURIComponent(entry.id)+'/thumbnail';thumbnail.onerror=()=>thumbnail.hidden=true;button.append(thumbnail);}
    const label=document.createElement('span');label.className='entry-label';const name=document.createElement('span');name.className='entry-name';name.textContent=entry.name;
    const state=document.createElement('span');state.className='entry-state';state.textContent=collectionEntryState(entry);label.append(name,state);button.append(label);
    button.onclick=()=>openCollectionEntry(index);strip.append(button);
    if(index===collectionIndex&&scrollActive)requestAnimationFrame(()=>button.scrollIntoView({block:'nearest',inline:'nearest'}));
  });
  controls();
}
function updateCollectionSession(data=session){
  if(!collection||!data)return;
  const entry=collection.entries.find(item=>item.id===data.entry_id||item.session_id===data.id);
  if(entry)Object.assign(entry,{session_id:data.id,edited:data.layers?.length>0,saved:data.saved_revision!==null&&data.saved_revision!==undefined,dirty:isDirty(data),saved_name:data.saved_name||null});
}
async function openCollectionEntry(index){
  if(busy||!collection||index<0||index>=collection.entries.length)return;
  if(index===collectionIndex&&session?.id===collection.entries[index].session_id)return;
  const activeCollection=collection,entry=activeCollection.entries[index];setBusy(true);message('Opening '+entry.name+'…');
  try{
    await flushLayerChanges();
    let result;
    if(activeCollection.local){
      let data;
      if(entry.session_id)data=await(await api('/api/local-remove/session/'+encodeURIComponent(entry.session_id))).json();
      else{const form=new FormData();form.append('file',entry.file);data=await(await api('/api/local-remove/import',{method:'POST',body:form})).json();entry.session_id=data.id;}
      result={session:data,collection:activeCollection,index};
    }else result=await json('/api/local-remove/collection/'+encodeURIComponent(activeCollection.id)+'/entry/'+encodeURIComponent(entry.id)+'/open',{});
    await openSession(result.session,{collection:result.collection||activeCollection,index:result.index??index});
  }catch(error){message('Could not open '+entry.name+': '+error.message,true);}finally{setBusy(false);}
}
async function openCollection(data,index=0){
  if(!data?.entries?.length)throw Error('This folder contains no supported images.');
  const previousCollection=collection,previousIndex=collectionIndex;
  collection=data;collectionIndex=-1;
  try{await openCollectionEntry(Math.max(0,Math.min(data.entries.length-1,index)));if(collectionIndex===-1){collection=previousCollection;collectionIndex=previousIndex;renderCollection();}}
  catch(error){collection=previousCollection;collectionIndex=previousIndex;renderCollection();throw error;}
}
function navigateCollection(direction){if(!busy&&collection)return openCollectionEntry(collectionIndex+direction);}
$('folder-previous').onclick=()=>navigateCollection(-1);$('folder-next').onclick=()=>navigateCollection(1);

function nativeRequest(action,files=null,details={}){
  if(!nativeBridge)return Promise.reject(Error('Open the Local Remove desktop app for this command.'));
  const id='local-remove-'+Date.now()+'-'+(++nativeRequestNumber);
  return new Promise((resolve,reject)=>{
    const timer=setTimeout(()=>{nativePending.delete(id);reject(Error('The desktop command timed out. Please try again.'));},action==='ready'?3000:600000);
    nativePending.set(id,{resolve,reject,timer});
    try{if(files){if(typeof nativeBridge.postMessageWithAdditionalObjects!=='function')throw Error('Use File → Open in this desktop window.');nativeBridge.postMessageWithAdditionalObjects({id,action,...details},files);}else nativeBridge.postMessage({id,action,...details});}
    catch(error){clearTimeout(timer);nativePending.delete(id);reject(error);}
  });
}
async function connectNative(){
  nativeBridge=window.chrome?.webview||null;if(!nativeBridge)return;
  nativeBridge.addEventListener('message',event=>{
    const data=event.data;
    if(data?.type==='local-remove-native'&&data.action==='requestClose'){handleNativeClose(data.id);return;}
    if(data?.type!=='local-remove-native'||!nativePending.has(data.id))return;
    const pending=nativePending.get(data.id);clearTimeout(pending.timer);nativePending.delete(data.id);
    if(data.error)pending.reject(Error(typeof data.error==='string'?data.error:'The desktop command failed.'));
    else pending.resolve(data.cancelled?null:data.result);
  });
  try{const result=await nativeRequest('ready');nativeReady=result?.native===true;nativeProjects=result?.projects===true;}catch{nativeReady=false;nativeProjects=false;}
  controls();
}
async function openNative(action,files=null){
  if(busy||!nativeReady)return;
  setBusy(true);
  try{
    await flushLayerChanges();
    const result=await nativeRequest(action,files);
    setBusy(false);
    if(result?.collection)await openCollection(result.collection,result.index??0);
    else if(result?.session)await openSession(result.session);
  }catch(error){message(error.message,true);}finally{setBusy(false);}
}
const supportedFile=file=>/\.(jpg|jpeg|png|tif|tiff|webp)$/i.test(file.name);
async function openBrowserFiles(files){
  if(busy)return;
  const allFiles=Array.from(files);
  if(allFiles.length===1&&/\.lremove$/i.test(allFiles[0].name)){await importProjectFile(allFiles[0]);return;}
  const images=allFiles.filter(supportedFile);if(!images.length){message('Choose JPEG, PNG, TIFF, or WebP images. Open folders with the desktop app.',true);return;}
  await openCollection({id:'uploads-'+Date.now(),name:images.length===1?'Browser upload copy':'Browser upload copies',local:true,
    entries:images.map((file,index)=>({id:String(index),name:file.name,file,session_id:null,edited:false,saved:false,dirty:false}))});
}
function filesDragged(event){return Array.from(event.dataTransfer?.types||[]).includes('Files');}
function resetDrop(){dragDepth=0;$('drop-notice').hidden=true;}
document.addEventListener('dragenter',event=>{if(!filesDragged(event))return;event.preventDefault();if(modalOpen()){resetDrop();return;}dragDepth++;$('drop-notice').textContent=busy?'Finish the current operation before opening images':nativeReady?'Drop images or a folder to open':'Drop images to open upload copies';$('drop-notice').hidden=false;});
document.addEventListener('dragover',event=>{if(!filesDragged(event))return;event.preventDefault();event.dataTransfer.dropEffect=busy||modalOpen()?'none':'copy';});
document.addEventListener('dragleave',event=>{if(filesDragged(event)&&--dragDepth<=0)resetDrop();});
document.addEventListener('drop',event=>{
  if(!filesDragged(event)&&!event.dataTransfer?.files?.length)return;event.preventDefault();resetDrop();
  if(modalOpen())return;
  if(busy){message('Finish the current operation before opening another image.');return;}
  const files=Array.from(event.dataTransfer.files||[]);if(nativeReady)openNative('drop',files);else openBrowserFiles(files);
});
window.addEventListener('blur',resetDrop);

// Camera scale uses preview pixels; the visible percentage uses full photo pixels.
function pixelRatio(){return session&&baseCanvas.width?session.width/baseCanvas.width:1;}
function photoZoom(){return zoom/pixelRatio();}
function setSizes(width,height){
  endGesture();
  for(const canvas of [baseCanvas,overlay,draft,mask]){canvas.width=width;canvas.height=height;}
  stage.style.width=width+'px';stage.style.height=height+'px';
  $('photo-image').style.width=width+'px';$('photo-image').style.height=height+'px';
  undo=[];points=[];hasSelection=false;$('finish').hidden=true;
  fitImage();
}
function clampCamera(){
  const width=baseCanvas.width*zoom,height=baseCanvas.height*zoom;
  const vw=viewport.clientWidth,vh=viewport.clientHeight;
  // A small image stays centered; a large image can be inspected to every edge.
  panX=width<=vw?(vw-width)/2:Math.min(24,Math.max(vw-width-24,panX));
  panY=height<=vh?(vh-height)/2:Math.min(24,Math.max(vh-height-24,panY));
}
function zoomLabel(){
  const percent=photoZoom()*100;
  return(percent<10?percent.toFixed(1):percent.toFixed(0))+'%';
}
function updateZoomControl(){
  const control=$('zoom'),value=photoZoom();
  let exact=[...control.options].find(option=>option.value!=='custom'&&Math.abs(Number(option.value)-value)<.00001);
  let custom=control.querySelector('option[value="custom"]');
  if(exact){if(custom)custom.remove();control.value=exact.value;}
  else{if(!custom){custom=document.createElement('option');custom.value='custom';control.prepend(custom);}custom.textContent=zoomLabel();control.value='custom';}
  control.title=zoomLabel()+' of full photo size';
  $('fit').setAttribute('aria-pressed',String(fitMode));
}
function updateCursor(){viewport.classList.toggle('hand',(spaceHeld||handActive)&&!!session);viewport.classList.toggle('panning',gesture?.kind==='pan');updateBrushCursor();}
function updateBrushCursor(){
  const visible=!!session&&!!brushPointer&&tool==='brush'&&!busy&&!spaceHeld&&!handActive&&!showOriginal&&!menuOpen&&!modalOpen()&&insidePhoto(brushPointer);
  $('brush-cursor').hidden=!visible;viewport.classList.toggle('brush-ready',visible);
  if(!visible)return;
  const p=localPoint(brushPointer),diameter=Number($('size').value)*photoZoom();
  $('brush-cursor').style.width=diameter+'px';$('brush-cursor').style.height=diameter+'px';
  $('brush-cursor').style.left=p.x+'px';$('brush-cursor').style.top=p.y+'px';
}
function setBrushSize(value){$('size').value=String(Math.max(1,Math.min(2000,Math.round(value))));$('size-label').textContent=$('size').value;updateBrushCursor();}
function stepBrushSize(direction){
  const value=Number($('size').value),next=direction>0?BRUSH_STEPS.find(size=>size>value):[...BRUSH_STEPS].reverse().find(size=>size<value);
  setBrushSize(next??(direction>0?2000:1));
}
function applyCamera(){
  if(!session)return;
  clampCamera();
  stage.style.transform=`translate(${panX}px,${panY}px) scale(${zoom})`;
  updateZoomControl();
  if(points.length)penDraft();
  else if(gesture?.kind==='draw'&&gesture.tool!=='brush')shapeDraft(gesture.start,gesture.last,gesture.tool);
  updateBrushCursor();
}
function fitImage(){
  if(!session||!baseCanvas.width)return;
  fitMode=true;
  zoom=Math.max(.001,Math.min((viewport.clientWidth-24)/baseCanvas.width,(viewport.clientHeight-24)/baseCanvas.height,pixelRatio()));
  panX=(viewport.clientWidth-baseCanvas.width*zoom)/2;panY=(viewport.clientHeight-baseCanvas.height*zoom)/2;
  viewportWidth=viewport.clientWidth;viewportHeight=viewport.clientHeight;
  applyCamera();
}
function localPoint(event){const rect=viewport.getBoundingClientRect();return{x:event.clientX-rect.left-viewport.clientLeft,y:event.clientY-rect.top-viewport.clientTop};}
function setPhotoZoom(value,anchor={x:viewport.clientWidth/2,y:viewport.clientHeight/2}){
  if(!session)return;
  // Finish a live stroke before the coordinate transform changes.
  if(gesture?.kind==='draw')endGesture();
  const imageX=(anchor.x-panX)/zoom,imageY=(anchor.y-panY)/zoom;
  zoom=Math.min(MAX_PHOTO_ZOOM,Math.max(MIN_PHOTO_ZOOM,value))*pixelRatio();
  panX=anchor.x-imageX*zoom;panY=anchor.y-imageY*zoom;fitMode=false;
  applyCamera();
}
new ResizeObserver(()=>{
  if(!session)return;
  if(fitMode){fitImage();return;}
  panX+=(viewport.clientWidth-viewportWidth)/2;panY+=(viewport.clientHeight-viewportHeight)/2;
  viewportWidth=viewport.clientWidth;viewportHeight=viewport.clientHeight;applyCamera();
}).observe(viewport);
viewport.addEventListener('wheel',event=>{
  if(!session||modalOpen())return;
  event.preventDefault();
  brushPointer={clientX:event.clientX,clientY:event.clientY};
  const delta=event.deltaY*(event.deltaMode===1?16:event.deltaMode===2?viewport.clientHeight:1);
  setPhotoZoom(photoZoom()*Math.exp(-Math.max(-150,Math.min(150,delta))*.0025),localPoint(event));
},{passive:false});
$('zoom').onchange=()=>{if($('zoom').value!=='custom')setPhotoZoom(Number($('zoom').value));viewport.focus({preventScroll:true});};
$('zoom-in').onclick=()=>setPhotoZoom(photoZoom()*1.25);
$('zoom-out').onclick=()=>setPhotoZoom(photoZoom()/1.25);
$('fit').onclick=()=>{endGesture();fitImage();};

function paintPhoto(){
  if(!originalImage)return;
  bc.clearRect(0,0,baseCanvas.width,baseCanvas.height);
  const photo=$('photo-image');if(photo.src!==originalImage.src)photo.src=originalImage.src;
  $('layer-stack').hidden=showOriginal;overlay.hidden=showOriginal;draft.hidden=showOriginal;
  $('before').setAttribute('aria-pressed',String(showOriginal));controls();
}
function paintMask(){
  oc.clearRect(0,0,overlay.width,overlay.height);oc.drawImage(mask,0,0);
  oc.globalCompositeOperation='source-in';oc.fillStyle='#ff6699';oc.fillRect(0,0,overlay.width,overlay.height);oc.globalCompositeOperation='source-over';
}
function refreshMask(){
  paintMask();
  const pixels=mc.getImageData(0,0,mask.width,mask.height).data;hasSelection=false;
  for(let i=3;i<pixels.length;i+=4){if(pixels[i]){hasSelection=true;break;}}
  controls();
}
function snapshot(){undo.push(mask.toDataURL('image/png'));if(undo.length>12)undo.shift();}
function clearSelection(){
  endGesture();mc.clearRect(0,0,mask.width,mask.height);dc.clearRect(0,0,draft.width,draft.height);
  points=[];hasSelection=false;$('finish').hidden=true;refreshMask();
}
function coord(event){
  const p=localPoint(event);
  return{x:Math.max(0,Math.min(mask.width,(p.x-panX)/zoom)),y:Math.max(0,Math.min(mask.height,(p.y-panY)/zoom))};
}
function insidePhoto(event){const p=localPoint(event);return p.x>=panX&&p.y>=panY&&p.x<=panX+mask.width*zoom&&p.y<=panY+mask.height*zoom;}
function mode(context){
  context.globalCompositeOperation=subtract?'destination-out':'source-over';context.fillStyle='white';context.strokeStyle='white';
  context.lineCap='round';context.lineJoin='round';context.lineWidth=Number($('size').value)/pixelRatio();
}
function stroke(a,b){
  mode(mc);mc.beginPath();mc.moveTo(a.x,a.y);mc.lineTo(b.x,b.y);mc.stroke();
  mc.beginPath();mc.arc(b.x,b.y,mc.lineWidth/2,0,Math.PI*2);mc.fill();
}
function shape(context,a,b,kind,fill){
  context.beginPath();
  if(kind==='rectangle')context.rect(Math.min(a.x,b.x),Math.min(a.y,b.y),Math.abs(b.x-a.x),Math.abs(b.y-a.y));
  else context.ellipse((a.x+b.x)/2,(a.y+b.y)/2,Math.abs(b.x-a.x)/2,Math.abs(b.y-a.y)/2,0,0,Math.PI*2);
  fill?context.fill():context.stroke();
}
function shapeDraft(a,b,kind){dc.clearRect(0,0,draft.width,draft.height);dc.strokeStyle='#ff87b3';dc.lineWidth=2/zoom;shape(dc,a,b,kind,false);}
function penDraft(hover=null){
  dc.clearRect(0,0,draft.width,draft.height);if(!points.length)return;
  dc.strokeStyle='#ff87b3';dc.fillStyle='#ff87b3';dc.lineWidth=2/zoom;
  dc.beginPath();dc.moveTo(points[0].x,points[0].y);
  for(const p of points.slice(1))dc.lineTo(p.x,p.y);
  if(hover)dc.lineTo(hover.x,hover.y);dc.stroke();
  for(const p of points){dc.beginPath();dc.arc(p.x,p.y,3/zoom,0,Math.PI*2);dc.fill();}
}
function finishPen(){
  if(busy||!session||showOriginal)return;
  if(points.length<3){message('Click at least three points for a pen selection.');return;}
  snapshot();mode(mc);mc.beginPath();mc.moveTo(points[0].x,points[0].y);
  for(const p of points.slice(1))mc.lineTo(p.x,p.y);
  mc.closePath();mc.fill();points=[];dc.clearRect(0,0,draft.width,draft.height);$('finish').hidden=true;refreshMask();
}
function endGesture(releaseCapture=true){
  if(!gesture)return;
  const ended=gesture;gesture=null;
  if(ended.kind==='draw'){
    if(ended.tool!=='brush'){mode(mc);shape(mc,ended.start,ended.last,ended.tool,true);}
    dc.clearRect(0,0,draft.width,draft.height);refreshMask();
  }
  if(releaseCapture&&viewport.hasPointerCapture(ended.pointerId))viewport.releasePointerCapture(ended.pointerId);
  if(points.length)penDraft();updateCursor();
}
function startPan(event){
  gesture={kind:'pan',pointerId:event.pointerId,clientX:event.clientX,clientY:event.clientY,startX:panX,startY:panY};
  viewport.setPointerCapture(event.pointerId);fitMode=false;updateZoomControl();
  if(points.length)penDraft();updateCursor();
}
viewport.addEventListener('pointerdown',event=>{
  if(!session||modalOpen()||gesture||![0,1].includes(event.button))return;
  event.preventDefault();viewport.focus({preventScroll:true});
  brushPointer={clientX:event.clientX,clientY:event.clientY};updateBrushCursor();
  if(spaceHeld||handActive||event.button===1){startPan(event);return;}
  if(busy||showOriginal||!insidePhoto(event))return;
  const p=coord(event);
  if(tool==='pen'){
    if(points.length>2&&Math.hypot(p.x-points[0].x,p.y-points[0].y)<10/zoom){finishPen();return;}
    points.push(p);$('finish').hidden=false;penDraft();controls();return;
  }
  snapshot();gesture={kind:'draw',tool,pointerId:event.pointerId,start:p,last:p,clientX:event.clientX,clientY:event.clientY};
  viewport.setPointerCapture(event.pointerId);
  if(tool==='brush'){stroke(p,p);refreshMask();}
});
viewport.addEventListener('pointermove',event=>{
  brushPointer={clientX:event.clientX,clientY:event.clientY};updateBrushCursor();
  if(!session)return;
  if(gesture){
    if(event.pointerId!==gesture.pointerId)return;
    if(gesture.kind==='pan'){
      panX=gesture.startX+event.clientX-gesture.clientX;panY=gesture.startY+event.clientY-gesture.clientY;applyCamera();return;
    }
    const p=coord(event);gesture.clientX=event.clientX;gesture.clientY=event.clientY;
    if(gesture.tool==='brush'){stroke(gesture.last,p);paintMask();}
    else shapeDraft(gesture.start,p,gesture.tool);
    gesture.last=p;return;
  }
  if(tool==='pen'&&points.length&&!busy&&!showOriginal&&!spaceHeld&&!handActive)penDraft(insidePhoto(event)?coord(event):null);
});
viewport.addEventListener('pointerup',event=>{
  if(!gesture||event.pointerId!==gesture.pointerId)return;
  if(gesture.kind==='draw'){
    const p=coord(event);if(gesture.tool==='brush')stroke(gesture.last,p);gesture.last=p;
  }
  endGesture();
});
viewport.addEventListener('pointercancel',event=>{brushPointer=null;if(gesture?.pointerId===event.pointerId)endGesture();updateBrushCursor();});
viewport.addEventListener('lostpointercapture',event=>{if(gesture?.pointerId===event.pointerId)endGesture();});
viewport.addEventListener('pointerleave',()=>{brushPointer=null;updateBrushCursor();if(!gesture&&points.length)penDraft();});
viewport.addEventListener('auxclick',event=>{if(event.button===1)event.preventDefault();});

function updateToolChrome(){
  for(const button of document.querySelectorAll('[data-tool]'))button.setAttribute('aria-pressed',String(button.dataset.tool===tool&&!handActive&&(tool!=='brush'||operation==='ai')));
  $('heal-brush').setAttribute('aria-pressed',String(tool==='brush'&&operation==='heal'&&!handActive));
  $('mode-ai').setAttribute('aria-pressed',String(operation==='ai'));$('mode-heal').setAttribute('aria-pressed',String(operation==='heal'));
  $('hand').setAttribute('aria-pressed',String(handActive));
  $('tool-name').textContent=handActive?'Hand':operation==='heal'&&tool==='brush'?'Heal brush':tool[0].toUpperCase()+tool.slice(1);
  $('brush-options').hidden=handActive||tool!=='brush';$('selection-modes').hidden=handActive;
  $('heal-method').hidden=operation!=='heal';$('model-shortcut').hidden=operation==='heal';
  updateCursor();updateWorkflowChrome();
}
function selectTool(value){
  endGesture();
  if(tool!==value){points=[];$('finish').hidden=true;dc.clearRect(0,0,draft.width,draft.height);}
  tool=value;handActive=false;updateToolChrome();
  $('tool-hint').textContent=tool==='pen'?'Click points; Enter closes the path.':tool==='brush'?(operation==='heal'?healHint():'Brush over the object and its shadow.'):'Drag to select. Selections add together.';
  controls();
}
function setOperation(value){
  if(busy||settingsSaving)return;
  endGesture();operation=value;try{localStorage.setItem('local-remove-operation',operation);}catch{}updateToolChrome();renderHealth();controls();
  $('tool-hint').textContent=operation==='heal'?healHint():'Select an object, then click Remove.';
  message(operation==='heal'?healHint()+' No model download.':'AI Remove · '+modelLabel());
}
function healHint(){return healMethod==='texture'?'Brush over a small object and its shadow. Uses nearby texture.':'Brush over dust or a thin scratch. For objects, choose Texture repair.';}
$('heal-method').onchange=()=>{healMethod=$('heal-method').value;try{localStorage.setItem('local-remove-heal-method',healMethod);}catch{}$('tool-hint').textContent=healHint();message(healHint());renderHealth();controls();};
function activateSelectionTool(value){if(value==='brush')setOperation('ai');selectTool(value);}
for(const button of document.querySelectorAll('[data-tool]'))button.onclick=()=>activateSelectionTool(button.dataset.tool);
$('mode-ai').onclick=()=>setOperation('ai');$('mode-heal').onclick=()=>setOperation('heal');
$('heal-brush').onclick=()=>{setOperation('heal');selectTool('brush');};
$('hand').onclick=()=>{endGesture();handActive=!handActive;if(points.length)penDraft();updateToolChrome();};
function selectionMode(value){subtract=value;$('subtract').setAttribute('aria-pressed',String(subtract));$('add').setAttribute('aria-pressed',String(!subtract));}
$('subtract').onclick=()=>selectionMode(true);$('add').onclick=()=>selectionMode(false);
$('size').oninput=()=>setBrushSize(Number($('size').value));
$('finish').onclick=finishPen;
$('clear').onclick=()=>{snapshot();clearSelection();};
$('undo').onclick=async()=>{
  endGesture();
  if(points.length){points.pop();penDraft();$('finish').hidden=!points.length;}
  else if(undo.length){
    const previous=undo.at(-1);setBusy(true);
    try{const image=await loadImage(previous);mc.clearRect(0,0,mask.width,mask.height);mc.globalCompositeOperation='source-over';mc.drawImage(image,0,0);undo.pop();refreshMask();}
    catch(error){message(error.message,true);}finally{setBusy(false);}
  }
  controls();
};
$('before').onclick=()=>{endGesture();showOriginal=!showOriginal;paintPhoto();};
function textEntry(target){return target instanceof Element&&!!target.closest('input,select,textarea,[contenteditable="true"],[role="textbox"],[role="menubar"],[role="menu"],[data-folder-entry]');}
document.addEventListener('keydown',event=>{
  if(modalOpen()){
    if((event.ctrlKey||event.metaKey)&&['s','o','w','z','+','=','-','0'].includes(event.key.toLowerCase()))event.preventDefault();
    return;
  }
  if((event.ctrlKey||event.metaKey)&&event.key.toLowerCase()==='w'&&session){event.preventDefault();if(!busy)closeCurrentImage();return;}
  if((event.ctrlKey||event.metaKey)&&event.altKey&&['s','o'].includes(event.key.toLowerCase())){event.preventDefault();if(!busy)$(event.key.toLowerCase()==='s'?'save-project':'open-project').click();return;}
  if((event.ctrlKey||event.metaKey)&&event.key.toLowerCase()==='s'&&session){
    event.preventDefault();if(!busy){closeMenus();$(session.can_return?(event.shiftKey?'save-unique':'return'):'save').click();}return;
  }
  const rangeBracket=event.target===$('size')&&['[',']'].includes(event.key);
  // Space/Enter activate focused buttons and links. Canvas shortcuts must not
  // consume those native actions or start a pan while keyboard users operate UI.
  if(!event.ctrlKey&&!event.metaKey&&!event.altKey&&[' ','Enter'].includes(event.key)&&event.target instanceof Element&&event.target.closest('button,a,[role="button"]'))return;
  const folderTarget=event.target instanceof Element&&!!event.target.closest('[data-folder-entry]');
  if(menuOpen||modalOpen()||(textEntry(event.target)&&!rangeBracket&&!folderTarget))return;
  if(folderTarget&&event.code==='Space')return;
  if((event.ctrlKey||event.metaKey)&&event.key.toLowerCase()==='o'){event.preventDefault();if(!busy)$(event.shiftKey?'open-folder':'open').click();return;}
  if(!session)return;
  if(event.code==='Space'&&!event.ctrlKey&&!event.metaKey&&!event.altKey){
    event.preventDefault();
    if(!spaceHeld){
      spaceHeld=true;
      // Switching mid-stroke retains the stroke and starts panning at this pointer position.
      if(gesture?.kind==='draw'){
        const previous=gesture;endGesture(false);
        startPan({pointerId:previous.pointerId,clientX:previous.clientX,clientY:previous.clientY});
      }
      if(points.length)penDraft();updateCursor();
    }
    return;
  }
  if(event.ctrlKey||event.metaKey){
    if(event.altKey&&event.shiftKey&&event.key.toLowerCase()==='e'){event.preventDefault();if(!busy)$('merge').click();return;}
    if(!busy&&event.key.toLowerCase()==='z'){event.preventDefault();$('undo').click();}
    if(event.key.toLowerCase()==='s'){event.preventDefault();if(!busy)$(session.can_return?(event.shiftKey?'save-unique':'return'):'save').click();}
    return;
  }
  if((event.altKey&&['ArrowLeft','ArrowRight'].includes(event.key))||(!event.altKey&&['PageUp','PageDown'].includes(event.key))){
    event.preventDefault();if(!busy&&!gesture&&collection)navigateCollection(['ArrowLeft','PageUp'].includes(event.key)?-1:1);return;
  }
  if(event.altKey||spaceHeld)return;
  if(event.key==='\\'){event.preventDefault();if(!busy)$('before').click();return;}
  if(['+','=','-','_','f','F','1'].includes(event.key)){
    event.preventDefault();
    if(event.key.toLowerCase()==='f'){endGesture();fitImage();}
    else if(event.key==='1')setPhotoZoom(1);
    else setPhotoZoom(photoZoom()*(['-','_'].includes(event.key)?.8:1.25));
    return;
  }
  if(event.key.toLowerCase()==='h'){event.preventDefault();$('hand').click();return;}
  if(busy||showOriginal)return;
  if(['[',']'].includes(event.key)&&tool==='brush'&&!handActive){event.preventDefault();stepBrushSize(event.key===']'?1:-1);return;}
  if(event.key==='Enter'&&tool==='pen'){event.preventDefault();finishPen();return;}
  if(event.key==='Escape'){points=[];penDraft();$('finish').hidden=true;controls();return;}
  if(event.key.toLowerCase()==='j'){event.preventDefault();$('heal-brush').click();return;}
  const chosen={b:'brush',p:'pen',r:'rectangle',e:'ellipse'}[event.key.toLowerCase()];if(chosen)activateSelectionTool(chosen);
});
document.addEventListener('keyup',event=>{
  if(event.code!=='Space')return;
  if(spaceHeld)event.preventDefault();spaceHeld=false;updateCursor();
  // An existing pan ends on pointer-up, never as a new paint stroke.
});
function resetTransientInput(){spaceHeld=false;brushPointer=null;endGesture();updateCursor();}
window.addEventListener('blur',resetTransientInput);
document.addEventListener('visibilitychange',()=>{if(document.hidden)resetTransientInput();});

// Application menus use actual commands and retain selections while open.
function closeMenus(restoreFocus=false){
  if(!menuOpen)return;
  const trigger=menuOpen;$(trigger.dataset.menu).hidden=true;trigger.setAttribute('aria-expanded','false');menuOpen=null;
  if(restoreFocus)trigger.focus();else viewport.focus({preventScroll:true});
}
function menuItems(){return menuOpen?[...$(menuOpen.dataset.menu).querySelectorAll('button:not(:disabled)')].filter(button=>!button.hidden):[];}
function openMenu(trigger,focusFirst=false){
  if(modalOpen())return;
  closeMenus();endGesture();controls();
  const menu=$(trigger.dataset.menu),rect=trigger.getBoundingClientRect();
  menu.hidden=false;menu.style.top=(rect.bottom+2)+'px';menu.style.left=Math.max(6,Math.min(rect.left,window.innerWidth-menu.offsetWidth-6))+'px';
  menuOpen=trigger;trigger.setAttribute('aria-expanded','true');
  updateBrushCursor();
  if(focusFirst)menuItems()[0]?.focus();
}
const menuTriggers=[...document.querySelectorAll('[data-menu]')];
for(const trigger of menuTriggers){
  trigger.onclick=()=>menuOpen===trigger?closeMenus():openMenu(trigger);
  trigger.addEventListener('pointerenter',()=>{if(menuOpen&&menuOpen!==trigger)openMenu(trigger);});
  trigger.addEventListener('keydown',event=>{
    if(['ArrowDown','ArrowUp'].includes(event.key)){event.preventDefault();event.stopPropagation();openMenu(trigger,true);if(event.key==='ArrowUp')menuItems().at(-1)?.focus();}
    if(['ArrowLeft','ArrowRight'].includes(event.key)){
      event.preventDefault();event.stopPropagation();const index=menuTriggers.indexOf(trigger),next=menuTriggers[(index+(event.key==='ArrowRight'?1:-1)+menuTriggers.length)%menuTriggers.length];
      if(menuOpen)openMenu(next);next.focus();
    }
  });
}
document.addEventListener('pointerdown',event=>{if(menuOpen&&!event.target.closest('.menu,[data-menu]'))closeMenus();});
document.addEventListener('click',event=>{if(event.target.closest('.menu button'))closeMenus();});
document.addEventListener('keydown',event=>{
  if(modalOpen()||!menuOpen)return;
  const items=menuItems(),index=items.indexOf(document.activeElement);
  if(event.key==='Escape'){event.preventDefault();closeMenus(true);return;}
  if(event.key==='Tab'){closeMenus();return;}
  if(['ArrowDown','ArrowUp','Home','End'].includes(event.key)){
    event.preventDefault();
    if(event.key==='Home')items[0]?.focus();else if(event.key==='End')items.at(-1)?.focus();
    else items[(index+(event.key==='ArrowDown'?1:-1)+items.length)%items.length]?.focus();
  }
  if(['ArrowLeft','ArrowRight'].includes(event.key)&&!menuTriggers.includes(document.activeElement)){
    event.preventDefault();const index=menuTriggers.indexOf(menuOpen);openMenu(menuTriggers[(index+(event.key==='ArrowRight'?1:-1)+menuTriggers.length)%menuTriggers.length],true);
  }
});
window.addEventListener('resize',()=>closeMenus());
window.addEventListener('blur',()=>closeMenus());
for(const button of document.querySelectorAll('[data-command]'))button.onclick=()=>$(button.dataset.command).click();
$('actual-size').onclick=()=>setPhotoZoom(1);
$('model-shortcut').onclick=()=>$('settings').click();

function describeModel(){
  const model=models.find(item=>item.id===$('model').value);
  $('model-description').textContent=model?(model.available===false?(model.reason||'This model is not available yet.'):(model.description||'Fills the selected area automatically.')):'Choose which model fills the selected area.';
}
function applySettings(data){
  if(!data||!Array.isArray(data.models)||!['klein','qwen'].includes(data.model))throw Error('Could not read model settings.');
  modelId=data.model;models=data.models;settingsLoaded=true;
  $('model').replaceChildren();
  for(const model of models.filter(model=>['klein','qwen'].includes(model.id))){
    const option=document.createElement('option');option.value=model.id;
    option.textContent=model.label+(model.available===false?' — unavailable':'');option.disabled=model.available===false;option.title=model.reason||'';
    $('model').append(option);
  }
  const unavailable=models.filter(model=>['klein','qwen'].includes(model.id)&&model.available===false);
  $('model-availability').textContent=unavailable.map(model=>model.label+': '+(model.reason||'Not available yet.')).join(' ');
  $('model-availability').hidden=!unavailable.length;
  $('model').value=modelId;describeModel();renderHealth();controls();
}
async function loadSettings(){applySettings(await(await api('/api/local-remove/settings')).json());}
$('settings').onclick=async()=>{
  if(busy||modalOpen())return;
  $('ask-before-overwrite').checked=askBeforeOverwrite;
  $('configure-ai').hidden=!nativeReady;
  closeMenus();resetTransientInput();settingsMessage('');$('settings-dialog').showModal();
  try{await loadSettings();}catch(error){settingsMessage(error.message,true);}
};
$('configure-ai').onclick=async()=>{
  if(busy||!nativeReady)return;
  try{await nativeRequest('configureAi');await loadSettings();settingsMessage('AI connection saved. Start ComfyUI to use AI Remove.');}
  catch(error){settingsMessage(error.message,true);}
};
$('settings-close').onclick=()=>$('settings-dialog').close();
$('settings-dialog').addEventListener('close',resetTransientInput);
$('model').onchange=async()=>{
  const chosen=$('model').value;if(chosen===modelId)return;
  settingsSaving=true;describeModel();controls();settingsMessage('Saving choice…');
  try{
    const data=await json('/api/local-remove/settings',{model:chosen},'PATCH');
    if(Array.isArray(data.models))applySettings(data);else await loadSettings();
    settingsMessage(modelLabel()+' will be used for the next removal.');
  }catch(error){$('model').value=modelId;describeModel();settingsMessage(error.message,true);}
  finally{settingsSaving=false;controls();}
};

// Original and transparent patches are immutable. Toggling only changes a DOM image's visibility.
async function displayAssets(data){
  let assets=displayCache.get(data.id);
  if(!assets){assets={base:null,layers:new Map()};displayCache.set(data.id,assets);}
  else{displayCache.delete(data.id);displayCache.set(data.id,assets);}
  assets.pixels=data.width*data.height+data.layers.reduce((total,layer)=>total+(layer.width||0)*(layer.height||0),0);
  const prefix='/api/local-remove/session/'+encodeURIComponent(data.id);
  if(!assets.base)assets.base=loadImage(prefix+'/base-display').catch(error=>{assets.base=null;throw error;});
  await Promise.all([assets.base,...data.layers.map(layer=>{
    if(!assets.layers.has(layer.id)){
      const pending=loadImage(prefix+'/layer/'+encodeURIComponent(layer.id)+'/display').then(image=>{
        image.className='layer-image';image.alt='';image.draggable=false;image.dataset.layerId=layer.id;return image;
      }).catch(error=>{assets.layers.delete(layer.id);throw error;});assets.layers.set(layer.id,pending);
    }
    return assets.layers.get(layer.id);
  })]);
  return assets;
}
function trimDisplayCache(){
  // Keep recent decoded documents responsive without retaining an entire folder in GPU/RAM.
  let pixels=[...displayCache.values()].reduce((total,assets)=>total+(assets.pixels||0),0);
  for(const [id,assets] of displayCache){
    if(displayCache.size<=3&&pixels<=160000000)break;
    if(id===session?.id)continue;
    displayCache.delete(id);pixels-=assets.pixels||0;
  }
}
async function refreshPreview(){
  const requested=session,version=++requestVersion;
  const assets=await displayAssets(requested);
  if(version!==requestVersion||session?.id!==requested.id)return;
  originalImage=await assets.base;previewImage=originalImage;
  const nodes=await Promise.all(requested.layers.map(layer=>assets.layers.get(layer.id)));
  const holder=$('layer-stack');
  // Reuse already decoded images. New generation and merge only fetch the new patch.
  const expected=new Set(nodes);for(const child of [...holder.children])if(!expected.has(child))child.remove();
  nodes.forEach((node,index)=>{if(holder.children[index]!==node)holder.append(node);});
  syncLayerDisplay();paintPhoto();trackDocument(session);trimDisplayCache();
}
function syncLayerDisplay(){
  if(!session)return;const ratio=pixelRatio(),verticalRatio=session.height/baseCanvas.height;
  for(const node of $('layer-stack').children){
    const layer=session.layers.find(item=>item.id===node.dataset.layerId);if(!layer)continue;
    node.hidden=!layer.visible||!!layer.discarded;
    node.style.left=(layer.x||0)/ratio+'px';node.style.top=(layer.y||0)/verticalRatio+'px';
    node.style.width=(layer.width||node.naturalWidth||node.width)/ratio+'px';
    node.style.height=(layer.height||node.naturalHeight||node.height)/verticalRatio+'px';
  }
}
function updateLayerRows(){
  for(const row of $('layers').children){
    const layer=session?.layers.find(item=>item.id===row.dataset.layerId);if(!layer)continue;
    row.classList.toggle('is-hidden',!layer.visible);row.hidden=!!layer.discarded;
    const button=row.children[0],label=(layer.visible?'Hide ':'Show ')+layer.name;
    button.setAttribute('aria-label',label);button.title=label;button.replaceChildren(svgIcon(layer.visible?'eye':'eye-off'));
  }
  const count=session.layers.filter(layer=>!layer.discarded).length;$('layer-count').textContent=count+(count===1?' edit':' edits');
  $('restore').hidden=!session.layers.some(layer=>layer.discarded);
}
function updateCollectionLabels(){
  updateCollectionSession();if(!collection)return;
  for(const button of $('filmstrip').children){const entry=collection.entries.find(item=>item.id===button.dataset.folderEntry);if(!entry)continue;
    button.title=entry.name+' — '+collectionEntryState(entry);
    const label=button.children[button.children.length-1];if(label?.children[1])label.children[1].textContent=collectionEntryState(entry);
  }
}
function layerList(){
  if(!session)return;
  const holder=$('layers');holder.replaceChildren();
  for(const layer of [...session.layers].reverse()){
    const item=document.createElement('div');item.className='layer'+(layer.visible?'':' is-hidden');item.dataset.layerId=layer.id;item.hidden=!!layer.discarded;
    const visibility=document.createElement('button');visibility.className='visibility';visibility.disabled=busy;
    visibility.setAttribute('aria-label',(layer.visible?'Hide ':'Show ')+layer.name);visibility.title=(layer.visible?'Hide ':'Show ')+layer.name;
    visibility.append(svgIcon(layer.visible?'eye':'eye-off'));visibility.onclick=()=>{const current=session.layers.find(item=>item.id===layer.id);changeLayer(layer.id,{visible:!current.visible});};
    const label=document.createElement('div');label.style.minWidth='0';
    const name=document.createElement('div');name.className='layer-name';name.textContent=layer.name;name.title=layer.name;
    const meta=document.createElement('div');meta.className='meta';meta.textContent=layer.model_label||'Removal';label.append(name,meta);
    const discard=document.createElement('button');discard.className='discard';discard.append(svgIcon('close'));
    discard.setAttribute('aria-label','Discard '+layer.name);discard.title='Discard '+layer.name;discard.disabled=busy;discard.onclick=()=>changeLayer(layer.id,{discarded:true});
    item.append(visibility,label,discard);holder.append(item);
  }
  const original=document.createElement('div');original.className='layer base';
  const originalLabel=document.createElement('div'),originalName=document.createElement('div'),originalMeta=document.createElement('div');
  originalName.className='layer-name';originalName.textContent='Original';originalMeta.className='meta';originalMeta.textContent='Protected';originalLabel.append(originalName,originalMeta);
  original.append(svgIcon('lock'),originalLabel);holder.append(original);
  const count=session.layers.filter(layer=>!layer.discarded).length;$('layer-count').textContent=count+(count===1?' edit':' edits');
  $('restore').hidden=!session.layers.some(layer=>layer.discarded);
}
function svgIcon(name){
  const svg=document.createElementNS('http://www.w3.org/2000/svg','svg'),use=document.createElementNS('http://www.w3.org/2000/svg','use');
  svg.setAttribute('class','icon');svg.setAttribute('aria-hidden','true');use.setAttribute('href','#i-'+name);svg.append(use);return svg;
}
function optimisticDocument(queue){
  const data=cloneDocument(queue.confirmed);
  for(const edit of queue.pending){const layer=data.layers.find(item=>item.id===edit.id);if(layer)Object.assign(layer,edit.change);}
  if(queue.pending.length){data.dirty=true;data.project_dirty=true;}
  return data;
}
function presentLayerQueue(queue){
  const data=optimisticDocument(queue);trackDocument(data);
  if(session?.id!==data.id)return;
  session=data;syncLayerDisplay();updateLayerRows();updateCollectionLabels();controls();
}
function changeLayer(id,change){
  if(!session||busy)return Promise.resolve();
  let queue=layerQueues.get(session.id);
  if(!queue){queue={confirmed:cloneDocument(session),pending:[],running:null};layerQueues.set(session.id,queue);}
  if(!queue.pending.length)queue.confirmed=cloneDocument(session);
  queue.pending.push({id,change});presentLayerQueue(queue);
  if(!queue.running)queue.running=persistLayerQueue(queue).finally(()=>{queue.running=null;controls();});
  return queue.running;
}
async function persistLayerQueue(queue){
  while(queue.pending.length){
    const edit=queue.pending[0];
    try{
      queue.confirmed=await json('/api/local-remove/session/'+encodeURIComponent(queue.confirmed.id)+'/layer/'+encodeURIComponent(edit.id),{...edit.change,revision:queue.confirmed.revision},'PATCH');
      queue.pending.shift();presentLayerQueue(queue);
      if(session?.id===queue.confirmed.id)message('Layer visibility updated. Save a project to keep editable layers.');
    }catch(error){
      queue.pending.shift();
      try{queue.confirmed=await(await api('/api/local-remove/session/'+encodeURIComponent(queue.confirmed.id))).json();}catch{}
      presentLayerQueue(queue);
      // A failed edit is rolled back; later requested states remain queued in order.
      if(session?.id===queue.confirmed.id)message('Layer update failed; that change was restored. '+error.message,true);
    }
  }
}
async function flushLayerChanges(){await Promise.all([...layerQueues.values()].map(queue=>queue.running).filter(Boolean));}
$('restore').onclick=()=>{const layer=[...session.layers].reverse().find(item=>item.discarded);if(layer)changeLayer(layer.id,{discarded:false});};
$('merge').onclick=async()=>{
  if(!session||busy||layerChangesPending())return;
  setBusy(true);message('Merging visible layers…');
  try{session=await json(url('/merge'),{revision:session.revision});await refreshPreview();layerList();updateCollectionSession();renderCollection();message('Merged visible result added as a new layer. Earlier layers remain editable.');}
  catch(error){message('Merge failed: '+error.message,true);}finally{setBusy(false);layerList();}
};
async function openSession(data,options={}){
  if(!data?.id)throw Error('The image session could not be opened.');
  rememberCurrentView();setBusy(true);
  try{
    await flushLayerChanges();
    const tracked=openDocuments.get(data.id);if(tracked&&tracked.revision>data.revision)data=tracked;
    const state=viewStates.get(data.id),prefix='/api/local-remove/session/'+encodeURIComponent(data.id);
    const assets=await displayAssets(data);
    const images=await Promise.all([assets.base,state?.mask?loadImage(state.mask):Promise.resolve(null)]);
    let nextCollection=options.collection;
    if(nextCollection===undefined&&data.collection_id){
      if(collection?.id===data.collection_id)nextCollection=collection;
      else{try{nextCollection=await(await api('/api/local-remove/collection/'+encodeURIComponent(data.collection_id))).json();}catch{nextCollection=null;}}
    }
    // Commit the navigation only after image and selection loading succeeded.
    session=data;collection=nextCollection||null;collectionIndex=collection?(options.index??collection.entries.findIndex(entry=>entry.id===data.entry_id||entry.session_id===data.id)):-1;
    previewImage=images[0];originalImage=images[0];showOriginal=false;brushPointer=null;trackDocument(session);
    $('empty').hidden=true;stage.hidden=false;
    const scale=Math.min(1,3000/Math.max(previewImage.width,previewImage.height));
    setSizes(Math.round(previewImage.width*scale),Math.round(previewImage.height*scale));restoreView(state,images[1]);await refreshPreview();layerList();updateCollectionSession();renderCollection(true);
    $('filename').textContent=session.name;$('filename').title=session.name+' · '+session.width+' × '+session.height+' · '+session.bit_depth+'-bit';
    $('save-note').textContent='File → Save editable project keeps the original and layers in a .lremove file. Image saves are flattened. Closing clears the working layers.';
    const navigation=collection&&!collection.local?'/remove?collection='+encodeURIComponent(collection.id)+'&index='+collectionIndex:'/remove?session='+encodeURIComponent(session.id);
    history.replaceState(null,'',navigation);
    message(state?'Selection and view restored.':session.project_name?'Project opened. Layers remain editable; use Export for a flattened image.':session.can_return?'Ready. Changes stay in this session until you save.':'Browser upload copy. Export downloads your finished image.');
    try{await recent();}catch{}
  }catch(error){message(error.message,true);throw error;}finally{setBusy(false);layerList();}
}
async function recent(){
  const sessions=await(await api('/api/local-remove/sessions')).json();$('recent').replaceChildren();
  for(const data of sessions.slice(0,6)){
    const button=document.createElement('button');button.className='recent';button.textContent=data.name;button.title=data.name;button.disabled=busy;button.setAttribute('role','menuitem');
    button.onclick=async()=>{if(busy)return;try{await openSession(await(await api('/api/local-remove/session/'+data.id)).json());}catch(error){message(error.message,true);}};
    $('recent').append(button);
  }
}
$('open').onclick=()=>nativeReady?openNative('openFiles'):$('file').click();
$('open-folder').onclick=()=>{if(nativeReady&&!busy)openNative('openFolder');};
$('file').onchange=async()=>{
  try{await openBrowserFiles($('file').files);}catch(error){message(error.message,true);}finally{$('file').value='';}
};
$('remove').onclick=async()=>{
  if(!session||busy||layerChangesPending()||!hasSelection||!operationReady()||settingsSaving)return;
  const healing=operation==='heal',requestModel=healing?'heal':modelId,label=modelLabel(),requestHealMethod=healMethod;
  const progress=healing?'Healing selected area…':'Removing with '+label+' on your GPU…';
  activeTask='repair';setBusy(true);layerList();const started=Date.now();message(progress);
  const timer=setInterval(()=>message(progress+' '+Math.round((Date.now()-started)/1000)+'s'),1000);
  try{
    const monochrome=document.createElement('canvas');monochrome.width=mask.width;monochrome.height=mask.height;
    const context=monochrome.getContext('2d');context.fillStyle='black';context.fillRect(0,0,mask.width,mask.height);context.drawImage(mask,0,0);
    session=await json(url('/remove'),{mask:monochrome.toDataURL('image/png').split(',')[1],revision:session.revision,model:requestModel,...(healing?{heal_method:requestHealMethod}:{})});
    clearSelection();undo=[];await refreshPreview();layerList();updateCollectionSession();renderCollection();
    message((healing?'Heal layer':'Removal')+' added in '+((Date.now()-started)/1000).toFixed(1)+'s. Hide or discard its layer to compare.');
  }catch(error){message(error.message,true);}finally{clearInterval(timer);activeTask=null;setBusy(false);layerList();try{await recent();}catch{}}
};
function loadOverwritePreference(){
  askBeforeOverwrite=true;
  try{askBeforeOverwrite=localStorage.getItem(OVERWRITE_PREFERENCE)!=='false';}catch{}
  $('ask-before-overwrite').checked=askBeforeOverwrite;
}
function setOverwritePreference(ask){
  askBeforeOverwrite=ask;$('ask-before-overwrite').checked=ask;
  try{localStorage.setItem(OVERWRITE_PREFERENCE,String(ask));}catch{}
}
$('ask-before-overwrite').onchange=()=>setOverwritePreference($('ask-before-overwrite').checked);
function resolveOverwrite(choice){
  const pending=overwritePrompt;if(!pending)return;
  if(choice==='overwrite'&&$('overwrite-dont-ask').checked)setOverwritePreference(false);
  overwritePrompt=null;
  $('overwrite-dialog').close();
  updateBrushCursor();viewport.focus({preventScroll:true});
  pending.resolve(choice);
}
function confirmOverwrite(){
  closeMenus();endGesture();spaceHeld=false;resetDrop();
  $('overwrite-filename').textContent=session.source_name||session.name||'Original image';
  $('overwrite-dont-ask').checked=false;
  const decision=new Promise(resolve=>{overwritePrompt={resolve};});
  $('overwrite-dialog').showModal();updateCursor();$('overwrite-cancel').focus();
  return decision;
}
$('overwrite-cancel').onclick=()=>resolveOverwrite(null);
$('overwrite-unique').onclick=()=>resolveOverwrite('unique');
$('overwrite-confirm').onclick=()=>resolveOverwrite('overwrite');
$('overwrite-dialog').addEventListener('cancel',event=>{event.preventDefault();resolveOverwrite(null);});
$('overwrite-dialog').addEventListener('close',()=>resolveOverwrite(null));
async function save(mode){
  if(!session||busy||layerChangesPending()||modalOpen())return;
  if(mode!=='export'&&!session.can_return){message('Use Export to save a flattened copy of this image.',true);return;}
  const requestedSession=session.id;
  if(mode==='overwrite'&&askBeforeOverwrite){
    mode=await confirmOverwrite();
    if(!mode||busy||session?.id!==requestedSession)return;
  }
  saveInProgress=true;setBusy(true);message(mode==='overwrite'?'Saving over the source image…':mode==='unique'?'Saving a unique copy beside the source…':'Preparing an exported copy…');
  try{
    const payload=mode==='export'?{return_to_source:false,revision:session.revision,format:outputFormat}:{mode,revision:session.revision,format:mode==='overwrite'?'original':outputFormat};
    const result=await json(url('/save'),payload);
    if(result.session){session=result.session;trackDocument(session);}
    if(result.collection){collection=result.collection;collectionIndex=result.index??collectionIndex;}
    if(result.download){const link=document.createElement('a');link.href=result.download;link.download=result.name;link.click();}
    updateCollectionSession();renderCollection();
    const depth=result.bit_depth?' · '+result.bit_depth+'-bit':'';
    message((mode==='overwrite'?'Saved: ':mode==='unique'?'Unique copy saved: ':'Export ready: ')+result.name+depth+((hasSelection||points.length)?'. Your pending selection has not been applied.':''));
  }catch(error){message('Save failed: '+error.message+'. Your edit and selection are still available.',true);}finally{saveInProgress=false;setBusy(false);}
}
$('save').onclick=()=>save('export');$('return').onclick=()=>save('overwrite');$('save-unique').onclick=()=>save('unique');
function hasWorkingLayers(data){return !!data?.layers?.length;}
function pendingSelection(id){
  if(session?.id===id)return hasSelection||points.length>0;
  const state=viewStates.get(id);return !!(state?.hasSelection||state?.points?.length);
}
function projectNeedsSave(data){return !data.project_saved||data.project_saved_revision!==data.revision||data.project_dirty===true;}
function downloadFile(result){const link=document.createElement('a');link.href=result.download;link.download=result.name;link.click();}
async function saveEditableProject(data=session,{forClose=false,saveAs=false}={}){
  if(!data)return false;
  await flushLayerChanges();data=openDocuments.get(data.id)||data;
  setBusy(true);message('Saving editable project…');
  try{
    const result=nativeProjects
      ?await nativeRequest('saveProject',null,{session_id:data.id,revision:data.revision,...(saveAs?{saveAs:true}:{})})
      :await json('/api/local-remove/session/'+encodeURIComponent(data.id)+'/export-project',{revision:data.revision});
    if(!result)return false;
    if(result.session){trackDocument(result.session);if(session?.id===result.session.id)session=result.session;}
    if(result.download)downloadFile(result);
    updateCollectionLabels();
    message(nativeProjects?'Editable project saved: '+result.name+(pendingSelection(data.id)?'. The pending selection is not included.':'')
      :'Project download prepared. Finish saving the .lremove file before closing.');
    // A browser download may be cancelled after this response. Keep the working document open.
    return nativeProjects&&result.saved===true;
  }catch(error){message('Project save failed. Your working layers are still available. '+error.message,true);return false;}
  finally{setBusy(false);}
}
async function importProjectFile(file){
  if(!file||busy)return;
  setBusy(true);message('Opening editable project…');
  try{const form=new FormData();form.append('file',file);const result=await(await api('/api/local-remove/import-project',{method:'POST',body:form})).json();await openSession(result.session);}
  catch(error){message('Could not open project: '+error.message,true);}finally{setBusy(false);}
}
$('save-project').onclick=()=>{if(!busy&&!modalOpen())saveEditableProject();};
$('save-project-as').onclick=()=>{if(!busy&&!modalOpen())saveEditableProject(session,{saveAs:true});};
$('open-project').onclick=()=>{if(!busy)(nativeProjects?openNative('openProject'):$('project-file').click());};
$('project-file').onchange=async()=>{try{await importProjectFile($('project-file').files[0]);}finally{$('project-file').value='';}};

function resolveClose(choice){
  const pending=closePrompt;if(!pending)return;closePrompt=null;
  $('close-dialog').close();updateBrushCursor();pending.resolve(choice);
}
function confirmClose(documents,all=false){
  closeMenus();endGesture();spaceHeld=false;resetDrop();
  const dirty=documents.filter(data=>hasWorkingLayers(data)&&projectNeedsSave(data));
  const pending=documents.some(data=>pendingSelection(data.id));
  $('close-title').textContent=all?'Close Local Remove?':'Close image?';
  $('close-warning').textContent=dirty.length
    ?'Closing will permanently clear the working layers. Save a project first to edit them again.'
    :documents.some(hasWorkingLayers)?'Closing clears the working layers. Saved .lremove projects remain available to reopen.':'Closing clears the pending selection.';
  $('close-summary').textContent=documents.filter(data=>hasWorkingLayers(data)||pendingSelection(data.id)).map(data=>{
    const count=data.layers.length;
    return data.name+' · '+count+(count===1?' layer':' layers')+(hasWorkingLayers(data)&&!projectNeedsSave(data)?' · saved in '+data.project_name:'');
  }).join('\n');
  $('close-selection-warning').hidden=!pending;
  $('close-discard').textContent=dirty.length?'Discard layers and close':'Close '+(all?'application':'image');
  $('close-save').hidden=!dirty.length;
  $('close-save').textContent=nativeProjects?(dirty.length>1?'Save projects and close…':'Save project and close…'):'Download project'+(dirty.length>1?'s':'')+'…';
  const result=new Promise(resolve=>{closePrompt={resolve};});
  $('close-dialog').showModal();updateCursor();$('close-cancel').focus();return result;
}
$('close-cancel').onclick=()=>resolveClose(null);
$('close-discard').onclick=()=>resolveClose('discard');
$('close-save').onclick=()=>resolveClose('save');
$('close-dialog').addEventListener('cancel',event=>{event.preventDefault();resolveClose(null);});
$('close-dialog').addEventListener('close',()=>resolveClose(null));
function forgetDocuments(ids){
  const removed=new Set(ids);
  for(const id of ids){openDocuments.delete(id);viewStates.delete(id);displayCache.delete(id);layerQueues.delete(id);}
  if(collection)for(const entry of collection.entries){if(removed.has(entry.session_id))Object.assign(entry,{session_id:null,edited:false,saved:false,dirty:false,saved_name:null});}
  if(session&&removed.has(session.id)){
    endGesture();requestVersion++;session=null;originalImage=null;previewImage=null;showOriginal=false;points=[];undo=[];hasSelection=false;brushPointer=null;
    $('layer-stack').replaceChildren();$('photo-image').removeAttribute('src');stage.hidden=true;$('empty').hidden=false;
    $('layers').replaceChildren();const help=document.createElement('p');help.className='muted';help.textContent='Open an image to begin.';$('layers').append(help);
    $('filename').textContent='No image open';$('filename').title='';$('layer-count').textContent='';$('restore').hidden=true;
    $('before').setAttribute('aria-pressed','false');history.replaceState(null,'','/remove');
    $('save-note').textContent='Open a .lremove project to continue with its original and editable layers.';
  }
  renderCollection();controls();
}
async function closeDocuments(documents,all=false){
  if(closeInProgress||busy||modalOpen())return false;
  closeInProgress=true;
  try{
    await flushLayerChanges();rememberCurrentView();
    documents=documents.map(data=>openDocuments.get(data.id)||data);
    if(documents.some(data=>hasWorkingLayers(data)||pendingSelection(data.id))){
      const choice=await confirmClose(documents,all);if(!choice)return false;
      if(choice==='save'){
        for(const data of documents.filter(data=>hasWorkingLayers(data)&&projectNeedsSave(data))){
          const saved=await saveEditableProject(data,{forClose:true});
          if(!saved){if(!nativeProjects)message('Finish saving the downloaded project, then close again and choose Discard layers and close. Your working layers have been kept.');return false;}
        }
      }
    }
    setBusy(true);message('Closing working image'+(documents.length===1?'':'s')+'…');
    // One validated batch prevents partial discard if another document changed or a close was cancelled.
    const current=documents.map(data=>openDocuments.get(data.id)||data);
    if(current.length)await json('/api/local-remove/close-sessions',{sessions:current.map(data=>({id:data.id,revision:data.revision})),discard:true});
    forgetDocuments(current.map(data=>data.id));try{await recent();}catch{}
    message('Image'+(current.length===1?'':'s')+' closed. Working layers cleared; saved files are unchanged.');return true;
  }catch(error){message('Could not close the image. Working layers have been kept. '+error.message,true);return false;}
  finally{closeInProgress=false;setBusy(false);}
}
function closeCurrentImage(){return session?closeDocuments([session]):Promise.resolve(true);}
$('close-image').onclick=closeCurrentImage;
async function handleNativeClose(id){
  let approved=false;
  try{approved=await closeDocuments([...openDocuments.values()],true);}catch(error){message(error.message,true);}
  nativeBridge?.postMessage({action:'closeReady',id,approved});
}
window.addEventListener('beforeunload',event=>{
  if([...openDocuments.values()].some(data=>hasWorkingLayers(data)||pendingSelection(data.id))){event.preventDefault();event.returnValue='';}
});

function renderHealth(){
  $('model-indicator').textContent=operation==='heal'?'Local retouch':modelLabel();$('model-shortcut').title=operation==='heal'?'Quick Heal information · Settings':'Removal model: '+modelLabel()+' · Settings';
  $('status').textContent=operation==='heal'?(operationReady()?'Quick Heal ready · On this PC':'Quick Heal unavailable'):(ready?(currentModel()?.available===false?modelLabel()+' unavailable':'AI Remove ready · On this PC'):'AI Remove · Connection needed');
  $('status').classList.toggle('ready',operationReady());
}
async function health(){
  try{const data=await(await api('/api/local-remove/status')).json();ready=data.ready;retouchReady=data.retouch_ready===true;$('status').title=data.device||'';renderHealth();}
  catch{ready=false;retouchReady=false;$('status').textContent='Local backend offline';$('status').classList.remove('ready');}
  if(!settingsLoaded){try{await loadSettings();}catch{if(operation!=='heal')$('status').textContent='Connecting to model settings…';}}
  controls();
}
async function init(){
  loadOverwritePreference();
  try{const stored=localStorage.getItem('local-remove-operation');if(['heal','ai'].includes(stored))operation=stored;}catch{}
  updateToolChrome();
  try{const stored=localStorage.getItem('local-remove-copy-format');if(['original','png','jpg','tif','webp'].includes(stored)){outputFormat=stored;$('output-format').value=stored;}}catch{}
  try{const stored=localStorage.getItem('local-remove-heal-method');if(['texture','telea'].includes(stored)){healMethod=stored;$('heal-method').value=stored;}}catch{}
  await connectNative();
  await health();
  try{await recent();}catch(error){message(error.message,true);}
  const query=new URLSearchParams(location.search),sid=query.get('session'),collectionId=query.get('collection');
  if(collectionId){try{const data=await(await api('/api/local-remove/collection/'+encodeURIComponent(collectionId))).json();await openCollection(data,Number(query.get('index'))||0);}catch(error){message(error.message,true);}}
  else if(sid){try{await openSession(await(await api('/api/local-remove/session/'+encodeURIComponent(sid))).json());}catch(error){message(error.message,true);}}
  controls();setInterval(health,15000);
}
init();
