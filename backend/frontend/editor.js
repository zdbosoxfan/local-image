'use strict';
const TOKEN='__TOKEN__';
const $=id=>document.getElementById(id);
const viewport=$('viewport'),stage=$('stage');
const mask=document.createElement('canvas'),mc=mask.getContext('2d',{willReadFrequently:true});
const baseCanvas=$('photo'),bc=baseCanvas.getContext('2d');
const overlay=$('selection'),oc=overlay.getContext('2d');
const draft=$('draft'),dc=draft.getContext('2d');
let session=null,tool='brush',subtract=false,busy=false,ready=false,showOriginal=false,previewImage=null,originalImage=null;
let points=[],undo=[],selectionRedo=[],hasSelection=false,requestVersion=0,documentNavigationEpoch=0,initCompleted=false;
let zoom=1,panX=0,panY=0,fitMode=true,spaceHeld=false,gesture=null,viewportWidth=0,viewportHeight=0;
let modelId='klein',models=[],settingsLoaded=false,settingsSaving=false,handActive=false,menuOpen=null,operation='heal',retouchReady=false;
let activeTask=null;
let setupState=null,setupRequestBusy=false,setupTimer=null,setupRefreshRunning=false,setupCandidatesOpen=false,nativeSetup=false,firstSetupPending=false;
let collection=null,collectionIndex=-1,nativeReady=false,nativeBridge=null,nativeRequestNumber=0,dragDepth=0,brushPointer=null,saveInProgress=false,outputFormat='original';
let healMethod='texture',askBeforeOverwrite=true,overwritePrompt=null;
let workspace='retouch',cutoutOperation='erase',qwenVariant='int8',aiProvider='klein',qwenStatus=null,compositeImage=null;
let backgroundLibraries=[],browserBackgrounds=[],cutoutRequestRunning=false;
let qwenDownload=null,qwenDownloadTimer=null;
let transformAssets=null;
let cutoutStudioTab="subject",generationStudioTab="prompt",filmstripCollapsed=false;
let generationModels=[],generationModelId="qwen",generationReferences=[],generationTargetSession=null,generationResultId=null,generationLoading=false;
let generationLoras=[],loraInventory=null,loraLibraryTab='installed',loraFiles=null,loraLibraryBusy=false,loraDownloadTimer=null;
let loraDialogEpoch=0,loraFilesModelId=null,loraVersions={inventory:0,search:0,files:0,poll:0};
let generationSamplingModel=null;
let generationLoadPromise=null,generationRefreshQueued=false;
let generationMissingReferences=0,generationDownloadJob=null,generationDownloadTimer=null;
let modelBrowserId='qwen',modelBrowserDownloads=null,modelBrowserHardware=null,modelBrowserRequest=false,modelFolderConnection=null,modelFolderStartPending=false;
let stockProviders=[],stockResults=[],stockSelectedId=null,stockPage=0,stockNextPage=null,stockLoading=false,stockImporting=false,stockOrigin='image',stockOpener=null;
let refineInitialized=false,refineDrafts=[],refineResults=[],refineDraftId=null,refineResultId=null,refineReferences=[],refineJob=null;
let refineLoras={draft:[],final:[]},loraContext='generate',refineRecipeWarnings=[],refineRecipeLoading=false,refineCompare={scale:1,x:.5,y:.5,drag:null};
let refineUpscaleStatus=null,refineUpscaleSetupBusy=false;
let generatedLibrary={items:[],count:0,bytes:0},generatedLibrarySelection=new Set(),generatedLibraryBusy=false,generatedLibraryOrigin='editor',generatedLibraryDelete=null;
let generatedLibraryFocused=null,generatedLibrarySelectMode=false;
const OVERWRITE_PREFERENCE='local-remove-ask-before-overwrite';
let closePrompt=null,closeInProgress=false,nativeProjects=false,nativeBatch=false;
const modalOpen=()=>window.LocalImageReactFeatures?.isModalOpen()||$('settings-dialog')?.open||$('overwrite-dialog').open||$('close-dialog').open||$('hardware-dialog')?.open||$('shortcuts-dialog')?.open||$('lora-dialog')?.open||$('model-browser-dialog')?.open||($('stock-dialog')?.open&&$('stock-dialog').dataset.docked!=='true')||$('credits-dialog').open||($('refine-dialog')?.open&&$('refine-dialog').dataset.inlineStudio!=='true')||($('generated-library-dialog')?.open&&$('generated-library-dialog').dataset.docked!=='true')||!!document.querySelector?.('dialog:modal')||window.LocalImageBatch?.isOpen()||!!overwritePrompt||!!closePrompt;
const viewStates=new Map(),nativePending=new Map(),openDocuments=new Map(),displayCache=new Map(),layerQueues=new Map();
const cloneDocument=data=>JSON.parse(JSON.stringify(data));
const layerChangesPending=()=>[...layerQueues.values()].some(queue=>queue.pending.length>0);
function publishEditorState(){window.LocalImageLegacyEditor?.publish();}
function trackDocument(data){
  if(data?.id&&(!openDocuments.has(data.id)||openDocuments.get(data.id).revision<=data.revision))openDocuments.set(data.id,data);
  publishEditorState();
}

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
const modelLabel=()=>currentModel()?.label||'FLUX.2 Klein';
const selectedHealMethod=()=>models.find(model=>model.id==='heal')?.methods?.find(method=>method.id===healMethod);
const operationReady=()=>operation==='heal'?retouchReady&&selectedHealMethod()?.available!==false:aiProvider==='qwen'?qwenReady():ready&&settingsLoaded&&currentModel()?.available!==false;
function message(text,error=false){$('message').textContent=text;$('message').title=text;$('message').classList.toggle('error',error);if(activeTask==='repair'&&$('progress-label'))$('progress-label').textContent=text;publishEditorState();}
function settingsMessage(text,error=false){const node=$('settings-message');if(!node){message(text,error);return;}node.textContent=text;node.classList.toggle('error',error);}
function setBusy(value){busy=value;if(value)endGesture();controls();}
function controls(){
  const active=!!session&&!busy&&!layerChangesPending();
  for(const id of ['clear','undo','add','subtract','before','save','return','save-unique','save-project','save-project-as','close-image','merge','size','finish'])$(id).disabled=!active;
  for(const id of ['zoom','zoom-in','zoom-out','fit','actual-size','hand'])$(id).disabled=!session;
  for(const button of document.querySelectorAll('[data-tool]'))button.disabled=!active;
  $('open').disabled=busy||closeInProgress;
  $('open-project').disabled=busy||closeInProgress;
  $('document-close').hidden=!session;
  $('open-folder').disabled=busy;
  for(const id of ['stock-open','background-stock','gen-stock'])if($(id))$(id).disabled=busy||closeInProgress;
  $('image-credits').hidden=!documentCredits().length;$('image-credits').disabled=busy;
  $('open-folder').title='Open a folder of images';
  $('settings').disabled=busy||settingsSaving;
  $('model-shortcut').disabled=busy||settingsSaving;
  $('heal-method').disabled=busy||settingsSaving||!session;
  $('mode-ai').disabled=busy||settingsSaving;$('mode-heal').disabled=busy||settingsSaving;
  $('heal-brush').disabled=!active;
  if($('model'))$('model').disabled=busy||settingsSaving||!settingsLoaded;
  $('remove').disabled=!active||!operationReady()||!hasSelection||showOriginal||settingsSaving;
  $('remove').classList.toggle('busy',busy);
  $('remove').textContent=busy?'Working…':operation==='heal'?'Heal':'Remove';
  $('edit-action-label').textContent=operation==='heal'?'Heal selected area':'Remove selected area';
  updateHistoryControls(active);
  $('clear').disabled=!active||(!hasSelection&&!points.length);
  $('finish').disabled=!active||points.length<3;
  $('merge').disabled=!active||!!session?.cutout?.enabled||!session.layers?.some(layer=>layer.visible&&!layer.discarded);
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
  updateCutoutControls();
  for(const command of document.querySelectorAll('[data-command]')){
    const source=$(command.dataset.command);if(!source)continue;command.disabled=source.disabled;
    if(command.dataset.command==='remove')command.hidden=workspace!=='retouch';
    if(command.dataset.command==='cutout-refine')command.hidden=workspace!=='cutout';
    if(command.dataset.command==='before')command.setAttribute('aria-checked',String(showOriginal));
  }
  for(const command of document.querySelectorAll('[data-needs-photo]'))if(!session)command.disabled=true;
  updateContextualControls();
  updateDocumentState();updateBrushCursor();updateWorkflowChrome();
  if($('refine-dialog')?.open)updateRefineControls();
  window.LocalImageGenerationBridge?.applyCommandAvailability();
  publishEditorState();
}
function updateContextualControls(){
  const context={photo:!!session,selection:!!session&&(hasSelection||points.length>0),
    layers:!!session?.layers?.some(layer=>!layer.discarded),pen:!!session&&tool==='pen'&&points.length>0&&!handActive,
    brush:!!session&&tool==='brush'&&!handActive};
  for(const element of document.querySelectorAll('[data-context]'))element.hidden=!context[element.dataset.context];
  const repairControls=document.querySelector?.('.context-actions');
  if(repairControls)repairControls.hidden=workspace==='generate'||handActive||workspace==='cutout'&&tool==='move';
}

// One derived view of the editing workflow keeps guidance, readiness, and the
// action in agreement. It never changes the user's selection or chosen method.
function workflowState(){
  if(workspace==='generate')return{title:busy?'Generating your image':'Create an image',description:'Describe a scene or add reference images in the Studio.',status:busy?'Generating…':'Image Gen'};
  if(workspace==='cutout')return cutoutWorkflowState();
  if(!session)return {title:'Start with a photo',description:'Open a photo, select a distraction, then apply a repair.',status:'No photo open'};
  if(busy)return {title:activeTask==='repair'?'Repairing your selection':'Updating your photo',description:activeTask==='repair'?'Your repair will appear as a new, editable layer.':'Please wait a moment.',status:'Working…'};
  if(showOriginal)return {title:'Viewing the original',description:'Choose Back to edits to continue. Your selection and layers are kept.',status:'Original view'};
  if(points.length)return {title:'Finish your selection',description:'Click the first point or press Enter to close the path. Escape cancels the path.',status:points.length+' path points'};
  if(!operationReady())return operation==='heal'
    ?{title:'Quick Heal unavailable',description:selectedHealMethod()?.available===false?'Texture repair is not installed. Choose Dust & scratches, or reinstall Local Image to restore texture repair.':'The local healing service is unavailable. Check the connection status and try again.',status:'Service unavailable'}
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
  if($('workflow-settings'))$('workflow-settings').hidden=workspace==='cutout'||operation!=='ai'||operationReady()||busy;
  if($('progress-strip'))$('progress-strip').hidden=!activeTask;
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
  $('document-state').title=pending?'The current selection has not been applied. '+(workspace==='cutout'?'Apply Erase or Restore before saving.':'Use Remove or Heal before saving.'):session?.saved_name?'Last saved: '+session.saved_name:status;
  const suffix=(session?.source_name||session?.name||'').match(/\.[a-z0-9]+$/i)?.[0]||'source';
  $('document-save').title='Overwrite the original '+suffix+' image (Ctrl+S). Keeps its original format.';
  $('return').title='Overwrite the source image in its original '+suffix+' format.';
  $('document-unique').title='Save a new '+(outputFormat==='original'?suffix:outputFormat.toUpperCase())+' copy beside the source (Ctrl+Shift+S).';
  $('output-format').disabled=busy;
  for(const option of $('output-format').options){
    const labels={original:'Original',png:'PNG',jpg:'JPEG',tif:'TIFF',webp:'WebP'};
    option.textContent=labels[option.value]+(session?.bit_depth===16&&option.value!=='original'?(option.value==='tif'?' (16-bit)':' (8-bit)'):'');
  }
  for(const item of document.querySelectorAll('[data-output-format]')){
    item.setAttribute('aria-checked',String(item.dataset.outputFormat===outputFormat));item.disabled=busy;
    item.textContent=[...$('output-format').options].find(option=>option.value===item.dataset.outputFormat)?.textContent||item.textContent;
  }
  $('format-hint').textContent='Save a copy and Export use this format. Overwrite keeps '+suffix+'.'+(session?.bit_depth===16&&['png','jpg','webp'].includes(outputFormat)?' This copy will be 8-bit.':'');
}
$('output-format').onchange=()=>{outputFormat=$('output-format').value;try{localStorage.setItem('local-remove-copy-format',outputFormat);}catch{}updateDocumentState();};
for(const item of document.querySelectorAll('[data-output-format]'))item.onclick=()=>{
  $('output-format').value=item.dataset.outputFormat;$('output-format').onchange();closeMenus();
};

// Keep selections and view state per image; compressed snapshots avoid retaining full raw pixel buffers.
function rememberCurrentView(){
  if(!session)return;
  trackDocument(session);endGesture();
  viewStates.set(session.id,{mask:hasSelection?mask.toDataURL('image/png'):null,undo:[...undo],selectionRedo:[...selectionRedo],points:points.map(point=>({...point})),
    width:mask.width,height:mask.height,tool,subtract,operation,workspace,cutoutOperation,brushSize:Number($('size').value),showOriginal,handActive,
    photoZoom:photoZoom(),panX,panY,fitMode,viewportWidth:viewport.clientWidth,viewportHeight:viewport.clientHeight,hasSelection});
}
function restoreView(state,selectionImage){
  if(!state)return;
  if(selectionImage){mc.globalCompositeOperation='source-over';mc.drawImage(selectionImage,0,0,mask.width,mask.height);}
  undo=[...state.undo];selectionRedo=[...(state.selectionRedo||[])];points=state.points.map(point=>({...point}));tool=state.tool;operation=state.operation;showOriginal=state.showOriginal;handActive=state.handActive;
  workspace=state.workspace||'retouch';cutoutOperation=state.cutoutOperation||'erase';
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
  $('folder-panel').hidden=!collection||collection.entries.length<=1;
  if(!collection){$('filmstrip').replaceChildren();return;}
  $('folder-name').textContent=collection.name;$('folder-name').title=collection.local?'Browser upload copies; export downloads a new file.':collection.name;
  $('folder-count').textContent=(collectionIndex+1)+' / '+collection.entries.length;
  $('filmstrip').hidden=collection.entries.length<2||filmstripCollapsed;
  const strip=$('filmstrip');strip.replaceChildren();
  collection.entries.forEach((entry,index)=>{
    const button=document.createElement('button');button.dataset.folderEntry=entry.id;button.disabled=busy;
    button.setAttribute('aria-current',String(index===collectionIndex));button.title=entry.name+' — '+collectionEntryState(entry);
    if(!collection.local||entry.thumbnail){const thumbnail=document.createElement('img');thumbnail.alt='';thumbnail.loading='lazy';thumbnail.decoding='async';
      thumbnail.src=entry.thumbnail||'/api/local-remove/collection/'+encodeURIComponent(collection.id)+'/entry/'+encodeURIComponent(entry.id)+'/thumbnail';thumbnail.onerror=()=>thumbnail.hidden=true;button.append(thumbnail);}
    const label=document.createElement('span');label.className='entry-label';const name=document.createElement('span');name.className='entry-name';name.textContent=entry.name;
    const state=document.createElement('span');state.className='entry-state';state.textContent=collectionEntryState(entry)==='Original'?'':collectionEntryState(entry);label.append(name,state);button.append(label);
    button.onclick=()=>openCollectionEntry(index);strip.append(button);
    if(index===collectionIndex&&scrollActive)requestAnimationFrame(()=>button.scrollIntoView({block:'nearest',inline:'nearest'}));
  });
  controls();
}
function updateCollectionSession(data=session){
  if(!collection||!data)return;
  const entry=collection.entries.find(item=>item.id===data.entry_id||item.session_id===data.id);
  if(entry)Object.assign(entry,{session_id:data.id,edited:data.layers?.length>0||!!data.cutout?.enabled,saved:data.saved_revision!==null&&data.saved_revision!==undefined,dirty:isDirty(data),saved_name:data.saved_name||null});
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

// These native actions open owned Windows dialogs and reply after OK or Cancel.
// A deadline must not expire while the user is still choosing a file or folder.
const NATIVE_DIALOG_ACTIONS=new Set(['batchExportFolder','openFiles','openFolder','openProject','saveProject','chooseBackgroundFolder','configureAi','setupChooseComfyDirectory','setupChooseModelDirectory','setupChooseInstallDirectory']);
function nativeRequest(action,files=null,details={}){
  if(!nativeBridge)return Promise.reject(Error('Open the Local Image desktop app for this command.'));
  const id='local-remove-'+Date.now()+'-'+(++nativeRequestNumber);
  return new Promise((resolve,reject)=>{
    const timer=NATIVE_DIALOG_ACTIONS.has(action)?null:setTimeout(()=>{nativePending.delete(id);reject(Error('The desktop command timed out. Please try again.'));},action==='ready'?3000:600000);
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
  try{const result=await nativeRequest('ready');nativeReady=result?.native===true;nativeProjects=result?.projects===true;nativeSetup=result?.setup===true;nativeBatch=result?.batch===true;}catch{nativeReady=false;nativeProjects=false;nativeSetup=false;nativeBatch=false;}
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
  await openCollection({id:'uploads-'+Date.now(),name:images[0].webkitRelativePath?.split('/')[0]||(images.length===1?'Browser upload copy':'Browser upload copies'),local:true,
    entries:images.map((file,index)=>({id:String(index),name:file.name,file,thumbnail:typeof URL!=='undefined'&&URL.createObjectURL?URL.createObjectURL(file):null,session_id:null,edited:false,saved:false,dirty:false}))});
}
function filesDragged(event){return Array.from(event.dataTransfer?.types||[]).includes('Files');}
async function filesFromDroppedEntries(entries){
  const files=[];
  async function visit(entry){
    if(entry.isFile){const file=await new Promise((resolve,reject)=>entry.file(resolve,reject));if(supportedFile(file))files.push(file);return;}
    if(!entry.isDirectory)return;
    const reader=entry.createReader();
    // Chromium returns directory entries in batches; read until exhaustion.
    for(;;){const batch=await new Promise((resolve,reject)=>reader.readEntries(resolve,reject));if(!batch.length)break;for(const child of batch)await visit(child);}
  }
  for(const entry of entries)await visit(entry);return files;
}
function resetDrop(){dragDepth=0;$('drop-notice').hidden=true;}
document.addEventListener('dragenter',event=>{if(!filesDragged(event))return;event.preventDefault();if(modalOpen()){resetDrop();return;}dragDepth++;$('drop-notice').textContent=busy?'Finish the current operation before opening images':nativeReady?'Drop images or a folder to open':'Drop images to open upload copies';$('drop-notice').hidden=false;});
document.addEventListener('dragover',event=>{if(!filesDragged(event))return;event.preventDefault();event.dataTransfer.dropEffect=busy||modalOpen()?'none':'copy';});
document.addEventListener('dragleave',event=>{if(filesDragged(event)&&--dragDepth<=0)resetDrop();});
document.addEventListener('drop',event=>{
  if(!filesDragged(event)&&!event.dataTransfer?.files?.length)return;event.preventDefault();resetDrop();
  if(modalOpen())return;
  if(busy){message('Finish the current operation before opening another image.');return;}
  const files=Array.from(event.dataTransfer.files||[]);if(nativeReady)openNative('drop',files);else{
    const entries=Array.from(event.dataTransfer.items||[]).map(item=>item.webkitGetAsEntry?.()).filter(Boolean);
    if(entries.some(entry=>entry.isDirectory))filesFromDroppedEntries(entries).then(openBrowserFiles).catch(error=>message('Could not read this folder: '+error.message,true));
    else openBrowserFiles(files);
  }
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
  undo=[];selectionRedo=[];points=[];hasSelection=false;$('finish').hidden=true;
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
function updateCursor(){viewport.classList.toggle('hand',(spaceHeld||handActive)&&!!session);viewport.classList.toggle('panning',gesture?.kind==='pan');viewport.classList.toggle('moving-subject',workspace==='cutout'&&tool==='move'&&!spaceHeld&&!handActive);updateBrushCursor();}
function updateBrushCursor(){
  const visible=workspace!=='generate'&&!!session&&!!brushPointer&&tool==='brush'&&!busy&&!spaceHeld&&!handActive&&!showOriginal&&!menuOpen&&!modalOpen()&&insidePhoto(brushPointer);
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
  if(!session||modalOpen()||window.LocalImageGenerationBridge?.hasHiddenEditor())return;
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
  const cutout=!!session?.cutout?.enabled&&!showOriginal,display=cutout&&compositeImage?compositeImage:originalImage;
  const photo=$('photo-image');photo.hidden=false;if(photo.src!==display.src)photo.src=display.src;
  stage.classList.toggle('cutout-preview',cutout);
  $('layer-stack').hidden=showOriginal||cutout;overlay.hidden=showOriginal||workspace==='generate';draft.hidden=showOriginal||workspace==='generate';
  $('before').setAttribute('aria-pressed',String(showOriginal));controls();
}
function paintMask(){
  oc.clearRect(0,0,overlay.width,overlay.height);oc.drawImage(mask,0,0);
  oc.globalCompositeOperation='source-in';oc.fillStyle=workspace==='cutout'&&cutoutOperation==='restore'?'#6bdeb7':'#ff6699';oc.fillRect(0,0,overlay.width,overlay.height);oc.globalCompositeOperation='source-over';
}
function refreshMask(){
  paintMask();
  const pixels=mc.getImageData(0,0,mask.width,mask.height).data;hasSelection=false;
  for(let i=3;i<pixels.length;i+=4){if(pixels[i]){hasSelection=true;break;}}
  controls();
}
function snapshot(){selectionRedo=[];undo.push(mask.toDataURL('image/png'));if(undo.length>12)undo.shift();}
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
  if(ended.kind==='transform'){dc.clearRect(0,0,draft.width,draft.height);paintPhoto();syncCutoutFields();}
  if(releaseCapture&&viewport.hasPointerCapture(ended.pointerId))viewport.releasePointerCapture(ended.pointerId);
  if(points.length)penDraft();updateCursor();
}
function startPan(event){
  gesture={kind:'pan',pointerId:event.pointerId,clientX:event.clientX,clientY:event.clientY,startX:panX,startY:panY};
  viewport.setPointerCapture(event.pointerId);fitMode=false;updateZoomControl();
  if(points.length)penDraft();updateCursor();
}
viewport.addEventListener('pointerdown',event=>{
  if(!session||window.LocalImageGenerationBridge?.hasHiddenEditor()||modalOpen()||gesture||![0,1].includes(event.button))return;
  event.preventDefault();viewport.focus({preventScroll:true});
  brushPointer={clientX:event.clientX,clientY:event.clientY};updateBrushCursor();
  if(spaceHeld||handActive||event.button===1){startPan(event);return;}
  if(busy||showOriginal||workspace==='generate'||!insidePhoto(event))return;
  if(workspace==='cutout'&&tool==='move'){
    if(!session.cutout?.enabled)return;
    if(transformAssets?.sessionId!==session.id||transformAssets?.revision!==session.revision){prepareTransformAssets().catch(error=>message(error.message,true));message('Preparing subject preview. Drag again in a moment.');return;}
    const transform=subjectTransform();gesture={kind:'transform',pointerId:event.pointerId,clientX:event.clientX,clientY:event.clientY,original:transform,transform:{...transform}};
    viewport.setPointerCapture(event.pointerId);paintTransformPreview(transform);updateCursor();return;
  }
  const p=coord(event);
  if(tool==='pen'){
    if(points.length>2&&Math.hypot(p.x-points[0].x,p.y-points[0].y)<10/zoom){finishPen();return;}
    selectionRedo=[];points.push(p);$('finish').hidden=false;penDraft();controls();return;
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
    if(gesture.kind==='transform'){
      gesture.transform={...gesture.original,offset_x:Math.round(gesture.original.offset_x+(event.clientX-gesture.clientX)/zoom*pixelRatio()),offset_y:Math.round(gesture.original.offset_y+(event.clientY-gesture.clientY)/zoom*pixelRatio())};
      paintTransformPreview(gesture.transform);return;
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
  if(gesture.kind==='transform'){
    const transform={...gesture.transform},changed=transform.offset_x!==gesture.original.offset_x||transform.offset_y!==gesture.original.offset_y;
    endGesture();if(changed)cutoutEdit('/cutout',{transform},'PATCH','Moving subject…');else syncCutoutFields();return;
  }
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
  for(const button of document.querySelectorAll('[data-tool]')){button.setAttribute('aria-pressed',String(button.dataset.tool===tool&&!handActive&&(workspace==='cutout'||tool!=='brush'||operation==='ai')));if(button.dataset.tool==='brush'){const label=workspace==='cutout'?'Cutout refinement brush (B)':'AI Remove brush (B)';button.title=label;button.setAttribute('aria-label',label);}}
  $('heal-brush').setAttribute('aria-pressed',String(tool==='brush'&&operation==='heal'&&!handActive));
  $('mode-ai').setAttribute('aria-pressed',String(operation==='ai'));$('mode-heal').setAttribute('aria-pressed',String(operation==='heal'));
  $('hand').setAttribute('aria-pressed',String(handActive));
  $('tool-name').textContent=handActive?'Hand':tool==='move'?'Move subject':workspace==='retouch'&&operation==='heal'&&tool==='brush'?'Heal brush':tool[0].toUpperCase()+tool.slice(1);
  $('brush-options').hidden=handActive||tool!=='brush';$('selection-modes').hidden=handActive||tool==='move';
  $('heal-method').hidden=workspace==='cutout'||operation!=='heal';$('model-shortcut').hidden=workspace==='cutout'||operation==='heal'||aiProvider==='qwen';
  $('ai-provider').hidden=workspace==='cutout'||operation!=='ai';$('retouch-qwen-variant').hidden=workspace==='cutout'||operation!=='ai'||aiProvider!=='qwen';
  updateCursor();updateWorkflowChrome();
}
function selectTool(value){
  endGesture();
  if(tool!==value){points=[];$('finish').hidden=true;dc.clearRect(0,0,draft.width,draft.height);}
  tool=value;handActive=false;updateToolChrome();
  $('tool-hint').textContent=tool==='pen'?'Click to add points · Enter to close':tool==='brush'?(operation==='heal'?healHint():'Brush to select · Subtract to erase'):'Drag to select · Subtract to erase';
  if(workspace==='cutout')$('tool-hint').textContent=tool==='pen'?'Click to add points · Enter to close':'Select an edge · Erase or Restore to refine';
  if(tool==='move'){$('tool-hint').textContent='Drag the subject · Scale and rotate in the Transform panel';selectStudioTab('transform');prepareTransformAssets().catch(error=>message(error.message,true));}
  controls();
}
function setOperation(value){
  if(busy||settingsSaving)return;
  endGesture();operation=value;try{localStorage.setItem('local-remove-operation',operation);}catch{}updateToolChrome();renderHealth();controls();
  $('tool-hint').textContent=operation==='heal'?healHint():'Select an object · Remove to apply';
  message(operation==='heal'?(healMethod==='texture'?'Texture repair':'Dust & scratches'):'AI Remove · '+modelLabel());
}
function healHint(){return healMethod==='texture'?'Brush to select · Repairs from nearby texture':'Brush over dust or narrow scratches';}
$('heal-method').onchange=()=>{healMethod=$('heal-method').value;try{localStorage.setItem('local-remove-heal-method',healMethod);}catch{}$('tool-hint').textContent=healHint();message(healHint());renderHealth();controls();};
function activateSelectionTool(value){if(value==='brush'&&workspace==='retouch')setOperation('ai');selectTool(value);}
for(const button of document.querySelectorAll('[data-tool]'))button.onclick=()=>activateSelectionTool(button.dataset.tool);
$('mode-ai').onclick=()=>setOperation('ai');$('mode-heal').onclick=()=>setOperation('heal');
$('heal-brush').onclick=()=>{if(workspace!=='retouch')setWorkspace('retouch');setOperation('heal');selectTool('brush');};
$('hand').onclick=()=>{endGesture();handActive=!handActive;if(points.length)penDraft();updateToolChrome();controls();};
function selectionMode(value){subtract=value;$('subtract').setAttribute('aria-pressed',String(subtract));$('add').setAttribute('aria-pressed',String(!subtract));}
$('subtract').onclick=()=>selectionMode(true);$('add').onclick=()=>selectionMode(false);
$('size').oninput=()=>setBrushSize(Number($('size').value));
$('finish').onclick=finishPen;
$('clear').onclick=()=>{snapshot();clearSelection();};
function historyTarget(redo=false){
  if(redo&&selectionRedo.length)return 'selection';
  if(!redo&&(points.length||undo.length))return 'selection';
  if(workspace==='cutout'&&!showOriginal&&session?.[redo?'cutout_can_redo':'cutout_can_undo'])return 'cutout';
  return null;
}
function updateHistoryControls(active=!!session&&!busy&&!layerChangesPending()){
  for(const redo of [false,true]){
    const target=historyTarget(redo),id=redo?'redo':'undo',label=(redo?'Redo':'Undo')+(target==='cutout'?' cutout edit':target==='selection'?(points.length||selectionRedo.at(-1)?.kind==='point'?' path point':' selection'):'');
    $(id).disabled=!active||!target;$('edit-'+id+'-label').textContent=label;
    $('cutout-'+id).disabled=!active||!target;$('cutout-'+id).textContent=label;$('cutout-'+id).title=label+' ('+(redo?'Ctrl+Shift+Z':'Ctrl+Z')+')';
  }
}
async function undoEdit(){
  if(window.LocalImageGenerationBridge?.hasHiddenEditor())return;
  if(!session||busy||layerChangesPending())return;
  endGesture();
  if(points.length){selectionRedo.push({kind:'point',point:points.pop()});penDraft();$('finish').hidden=!points.length;}
  else if(undo.length){
    const previous=undo.at(-1),current=mask.toDataURL('image/png');setBusy(true);
    try{const image=await loadImage(previous);mc.clearRect(0,0,mask.width,mask.height);mc.globalCompositeOperation='source-over';mc.drawImage(image,0,0);undo.pop();selectionRedo.push({kind:'mask',mask:current});refreshMask();}
    catch(error){message(error.message,true);}finally{setBusy(false);}
  }else if(historyTarget()==='cutout')await cutoutEdit('/cutout/undo',{},'POST','Undoing cutout edit…');
  controls();
}
async function redoEdit(){
  if(window.LocalImageGenerationBridge?.hasHiddenEditor())return;
  if(!session||busy||layerChangesPending())return;
  endGesture();const next=selectionRedo.at(-1);
  if(next?.kind==='point'){points.push(next.point);selectionRedo.pop();penDraft();$('finish').hidden=false;}
  else if(next?.kind==='mask'){
    const previous=mask.toDataURL('image/png');setBusy(true);
    try{const image=await loadImage(next.mask);mc.clearRect(0,0,mask.width,mask.height);mc.globalCompositeOperation='source-over';mc.drawImage(image,0,0);undo.push(previous);selectionRedo.pop();refreshMask();}
    catch(error){message(error.message,true);}finally{setBusy(false);}
  }else if(historyTarget(true)==='cutout')await cutoutEdit('/cutout/redo',{},'POST','Redoing cutout edit…');
  controls();
}
$('undo').onclick=undoEdit;$('redo').onclick=redoEdit;
$('before').onclick=()=>{endGesture();showOriginal=!showOriginal;paintPhoto();};
function textEntry(target){return target instanceof Element&&!!target.closest('input,select,textarea,[contenteditable="true"],[role="textbox"],[role="menubar"],[role="menu"],[data-folder-entry]');}
document.addEventListener('keydown',event=>{
  if(modalOpen()){
    if((event.ctrlKey||event.metaKey)&&['s','o','w','z','+','=','-','0'].includes(event.key.toLowerCase()))event.preventDefault();
    return;
  }
  if((event.ctrlKey||event.metaKey)&&event.key.toLowerCase()==='w'&&(session||window.LocalImageGenerationBridge?.hasVisibleDocument())){event.preventDefault();if(!busy)closeCurrentImage();return;}
  if((event.ctrlKey||event.metaKey)&&event.altKey&&['s','o'].includes(event.key.toLowerCase())){event.preventDefault();if(!busy)$(event.key.toLowerCase()==='s'?'save-project':'open-project').click();return;}
  if((event.ctrlKey||event.metaKey)&&event.key.toLowerCase()==='s'&&(session||window.LocalImageGenerationBridge?.hasVisibleDocument())){
    if(window.LocalImageGenerationBridge?.hasHiddenEditor()){event.preventDefault();if(!busy)save('export');return;}
    event.preventDefault();if(!busy){closeMenus();$(session.can_return?(event.shiftKey?'save-unique':'return'):'save').click();}return;
  }
  if((event.ctrlKey||event.metaKey)&&event.key.toLowerCase()==='o'){event.preventDefault();if(!busy){closeMenus();$(event.shiftKey?'open-folder':'open').click();}return;}
  if(menuOpen&&(event.ctrlKey||event.metaKey)){
    const command=event.key.toLowerCase()==='z'?(event.shiftKey?'redo':'undo'):event.altKey&&event.shiftKey&&event.key.toLowerCase()==='e'?'merge':null;
    if(command){event.preventDefault();closeMenus();$(command).click();return;}
  }
  const rangeBracket=event.target===$('size')&&['[',']'].includes(event.key);
  // Space/Enter activate focused buttons and links. Canvas shortcuts must not
  // consume those native actions or start a pan while keyboard users operate UI.
  if(!event.ctrlKey&&!event.metaKey&&!event.altKey&&[' ','Enter'].includes(event.key)&&event.target instanceof Element&&event.target.closest('button,a,[role="button"]'))return;
  const folderTarget=event.target instanceof Element&&!!event.target.closest('[data-folder-entry]');
  if(menuOpen||modalOpen()||(textEntry(event.target)&&!rangeBracket&&!folderTarget))return;
  if(folderTarget&&event.code==='Space')return;
  if(!session||window.LocalImageGenerationBridge?.hasHiddenEditor())return;
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
    if(!busy&&event.key.toLowerCase()==='z'){event.preventDefault();$(event.shiftKey?'redo':'undo').click();}
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
  if(busy||showOriginal||workspace==='generate')return;
  if(['[',']'].includes(event.key)&&tool==='brush'&&!handActive){event.preventDefault();stepBrushSize(event.key===']'?1:-1);return;}
  if(event.key==='Enter'&&tool==='pen'){event.preventDefault();finishPen();return;}
  if(event.key==='Escape'){points=[];penDraft();$('finish').hidden=true;controls();return;}
  if(event.key.toLowerCase()==='j'){event.preventDefault();$('heal-brush').click();return;}
  if(event.key.toLowerCase()==='v'&&workspace==='cutout'){event.preventDefault();if(session.cutout?.enabled)selectTool('move');return;}
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
let menuReturnFocus=null,menuTypeahead='',menuTypeaheadTime=0,menuAltPressed=false,submenuOpen=null;
function closeSubmenu(restoreFocus=false){
  if(!submenuOpen)return;
  const summary=submenuOpen.querySelector('summary');submenuOpen.open=false;submenuOpen=null;
  summary.setAttribute('aria-expanded','false');menuTypeahead='';
  if(restoreFocus)summary.focus();
}
function closeMenus(restoreFocus=false,keepFocus=false){
  if(!menuOpen)return;
  closeSubmenu();
  const trigger=menuOpen;$(trigger.dataset.menu).hidden=true;trigger.setAttribute('aria-expanded','false');menuOpen=null;
  menuTypeahead='';
  if(restoreFocus)trigger.focus();else if(!keepFocus&&!modalOpen())viewport.focus({preventScroll:true});
  updateBrushCursor();
}
function menuItems(scope=null){
  if(!menuOpen)return [];
  const focusedSubmenu=document.activeElement?.closest('.submenu');
  scope=scope||(submenuOpen&&focusedSubmenu===submenuOpen.querySelector('.submenu')?focusedSubmenu:$(menuOpen.dataset.menu));
  return [...scope.querySelectorAll('button:not(:disabled),summary')].filter(element=>{
    if(element.closest('[hidden]'))return false;
    return scope.classList.contains('submenu')?element.closest('.submenu')===scope:!element.closest('.submenu');
  });
}
function openSubmenu(foldout,focusFirst=false){
  if(!menuOpen)return;
  const submenu=foldout.querySelector('.submenu'),summary=foldout.querySelector('summary');
  if(!submenu)return;
  if(submenuOpen!==foldout)closeSubmenu();
  submenuOpen=foldout;foldout.open=true;summary.setAttribute('aria-expanded','true');menuTypeahead='';
  submenu.style.maxWidth=Math.max(120,window.innerWidth-12)+'px';
  submenu.style.maxHeight=Math.max(120,window.innerHeight-12)+'px';
  const parent=$(menuOpen.dataset.menu).getBoundingClientRect(),anchor=summary.getBoundingClientRect();
  const width=submenu.offsetWidth,height=submenu.offsetHeight;
  const left=parent.right+width>window.innerWidth-6?parent.left-width+1:parent.right-1;
  submenu.style.left=Math.max(6,Math.min(left,window.innerWidth-width-6))+'px';
  submenu.style.top=Math.max(6,Math.min(anchor.top-4,window.innerHeight-height-6))+'px';
  if(focusFirst)menuItems(submenu)[0]?.focus();
}
function focusMenuItem(item){
  if(!item)return;
  if(submenuOpen&&!item.closest('.submenu'))closeSubmenu();
  item.focus();
}
const menuTriggers=[...document.querySelectorAll('[data-menu]')];
const menubarTriggers=menuTriggers.filter(trigger=>trigger.closest('[role="menubar"]'));
function focusMenuTrigger(trigger){
  if(menubarTriggers.includes(trigger))for(const item of menubarTriggers)item.tabIndex=item===trigger?0:-1;
  trigger.focus({preventScroll:true});
}
function adjacentMenuTrigger(trigger,direction){
  const current=menubarTriggers.find(item=>item.dataset.menu===trigger.dataset.menu);
  return menubarTriggers[(menubarTriggers.indexOf(current)+direction+menubarTriggers.length)%menubarTriggers.length];
}
function openMenu(trigger,focusFirst=false){
  if(modalOpen()||trigger.disabled)return;
  if(!menuOpen&&!document.activeElement?.closest('.menu,[data-menu]'))menuReturnFocus=document.activeElement;
  closeMenus(false,true);endGesture();controls();
  const menu=$(trigger.dataset.menu),rect=trigger.getBoundingClientRect();
  for(const foldout of menu.querySelectorAll('details.menu-foldout'))foldout.open=false;
  menu.style.maxHeight=Math.max(120,window.innerHeight-rect.bottom-8)+'px';
  menu.hidden=false;menu.style.top=(rect.bottom+2)+'px';menu.style.left=Math.max(6,Math.min(rect.left,window.innerWidth-menu.offsetWidth-6))+'px';
  menuOpen=trigger;trigger.setAttribute('aria-expanded','true');
  updateBrushCursor();
  if(focusFirst&&menuItems().length)menuItems()[0].focus();else focusMenuTrigger(trigger);
}
for(const [index,trigger] of menubarTriggers.entries())trigger.tabIndex=index===0?0:-1;
for(const trigger of menuTriggers){
  trigger.onclick=()=>menuOpen===trigger?closeMenus(true):openMenu(trigger);
  trigger.addEventListener('focus',()=>{if(menubarTriggers.includes(trigger))for(const item of menubarTriggers)item.tabIndex=item===trigger?0:-1;});
  trigger.addEventListener('pointerenter',()=>{if(menuOpen&&menuOpen!==trigger&&menubarTriggers.includes(trigger))openMenu(trigger);});
  trigger.addEventListener('keydown',event=>{
    if(['ArrowDown','ArrowUp'].includes(event.key)){event.preventDefault();event.stopPropagation();openMenu(trigger,true);if(event.key==='ArrowUp')menuItems().at(-1)?.focus();}
    if(['ArrowLeft','ArrowRight'].includes(event.key)){
      event.preventDefault();event.stopPropagation();const next=adjacentMenuTrigger(trigger,event.key==='ArrowRight'?1:-1);
      if(next){if(menuOpen)openMenu(next);focusMenuTrigger(next);}
    }
  });
}
for(const foldout of document.querySelectorAll('details.menu-foldout')){
  const summary=foldout.querySelector('summary');summary.tabIndex=-1;
  summary.addEventListener('click',event=>{event.preventDefault();openSubmenu(foldout,true);});
  summary.addEventListener('pointerenter',()=>openSubmenu(foldout));
  summary.addEventListener('keydown',event=>{
    if(['ArrowRight','Enter',' '].includes(event.key)){event.preventDefault();event.stopPropagation();openSubmenu(foldout,true);}
  });
}
for(const item of document.querySelectorAll('.menu button,.menu select'))item.tabIndex=-1;
document.addEventListener('pointerdown',event=>{if(menuOpen&&!event.target.closest('.menu,[data-menu]'))closeMenus(false,true);});
document.addEventListener('pointerover',event=>{
  const item=event.target.closest('.menu button,.menu summary');
  if(submenuOpen&&item&&!item.closest('.submenu')&&item.closest('details.menu-foldout')!==submenuOpen)closeSubmenu();
});
// Close before dispatch so opening a dialog never loses focus back to the canvas.
document.addEventListener('click',event=>{if(menuOpen&&event.target.closest('.menu button:not(:disabled)'))closeMenus();},true);
document.addEventListener('keydown',event=>{
  if(modalOpen())return;
  if(event.key==='Alt'){menuAltPressed=!event.ctrlKey&&!event.metaKey;return;}
  menuAltPressed=false;
  if(event.key==='F10'&&!event.shiftKey){
    event.preventDefault();
    if(menuOpen||menubarTriggers.includes(document.activeElement)){closeMenus(false,true);(menuReturnFocus||viewport).focus();}
    else if(menubarTriggers.length){menuReturnFocus=document.activeElement;focusMenuTrigger(menubarTriggers[0]);}
    return;
  }
  if(event.altKey&&!event.ctrlKey&&!event.metaKey&&event.key.length===1){
    const trigger=menubarTriggers.find(item=>item.textContent.trim().toLowerCase().startsWith(event.key.toLowerCase()));
    if(trigger){event.preventDefault();openMenu(trigger,true);return;}
  }
  if(!menuOpen){
    if(event.key==='Escape'&&menubarTriggers.includes(document.activeElement)){event.preventDefault();(menuReturnFocus||viewport).focus();}
    return;
  }
  const items=menuItems(),index=items.indexOf(document.activeElement);
  if(event.key==='Escape'){event.preventDefault();if(submenuOpen)closeSubmenu(true);else closeMenus(true);return;}
  if(event.key==='Tab'){closeMenus(true);return;}
  if(event.key==='ArrowLeft'&&submenuOpen){event.preventDefault();closeSubmenu(true);return;}
  if(['ArrowDown','ArrowUp','Home','End'].includes(event.key)){
    event.preventDefault();
    if(event.key==='Home')focusMenuItem(items[0]);else if(event.key==='End')focusMenuItem(items.at(-1));
    else focusMenuItem(items[(index+(event.key==='ArrowDown'?1:-1)+items.length)%items.length]);
    return;
  }
  if(['ArrowLeft','ArrowRight'].includes(event.key)&&!menuTriggers.includes(document.activeElement)){
    event.preventDefault();const next=adjacentMenuTrigger(menuOpen,event.key==='ArrowRight'?1:-1);if(next)openMenu(next,true);return;
  }
  if(event.key.length===1&&!event.ctrlKey&&!event.metaKey&&!event.altKey&&event.key!==' '){
    event.preventDefault();const now=Date.now();menuTypeahead=now-menuTypeaheadTime<700?menuTypeahead+event.key.toLowerCase():event.key.toLowerCase();menuTypeaheadTime=now;
    const match=prefix=>items.slice(index+1).concat(items.slice(0,index+1)).find(item=>item.textContent.trim().toLowerCase().startsWith(prefix));
    const next=match(menuTypeahead)||match(event.key.toLowerCase());focusMenuItem(next);
  }
});
document.addEventListener('keyup',event=>{
  if(event.key!=='Alt'||!menuAltPressed||modalOpen())return;
  event.preventDefault();menuAltPressed=false;
  if(menuOpen||menubarTriggers.includes(document.activeElement)){closeMenus(false,true);(menuReturnFocus||viewport).focus();}
  else if(menubarTriggers.length){menuReturnFocus=document.activeElement;focusMenuTrigger(menubarTriggers[0]);}
});
window.addEventListener('resize',()=>closeMenus(false,true));
window.addEventListener('blur',()=>{menuAltPressed=false;closeMenus(false,true);});
for(const button of document.querySelectorAll('[data-command]'))button.onclick=()=>$(button.dataset.command).click();
$('actual-size').onclick=()=>setPhotoZoom(1);
$('model-shortcut').onclick=()=>$('settings').click();

function describeModel(){
  $('model-description').textContent='Choose where model files are stored, then compare models and download the ones that fit your GPU. Existing verified files are reused.';
}
function applySettings(data){
  if(!data||!Array.isArray(data.models)||!data.models.some(item=>item.id==='klein'))throw Error('Could not read FLUX settings.');
  modelId='klein';models=data.models.filter(item=>['klein','heal'].includes(item.id));settingsLoaded=true;
  $('model').replaceChildren();const option=document.createElement('option');option.value='klein';option.textContent=modelLabel();$('model').append(option);$('model').value='klein';
  $('model-availability').hidden=true;describeModel();renderHealth();controls();
}
async function loadSettings(){if(window.LocalImageReactFeatures?.settings)return window.LocalImageReactFeatures.settings.reloadConfiguration();applySettings(await(await api('/api/local-remove/settings')).json());}
const setupJobActive=()=>setupState?.job?.status==='running';
function setupBytes(value){
  const amount=Number(value)||0;if(amount>=1073741824)return(amount/1073741824).toFixed(1)+' GB';
  if(amount>=1048576)return(amount/1048576).toFixed(0)+' MB';
  if(amount>=1024)return(amount/1024).toFixed(0)+' KB';return amount+' B';
}
function setupStorageText(storage){
  if(storage?.error)return 'Folder unavailable: '+storage.error;
  if(Number.isFinite(storage?.free_bytes))return setupBytes(storage.free_bytes)+' available on this drive.';
  return '';
}
function scheduleSetupRefresh(){
  clearTimeout(setupTimer);setupTimer=null;
  if(window.LocalImageReactFeatures?.settings)return;
  if($('settings-dialog').open&&(setupJobActive()||setupState?.service?.starting))setupTimer=setTimeout(()=>loadSetup().catch(error=>{settingsMessage(error.message,true);scheduleSetupRefresh();}),1500);
}
function renderSetup(){
  if(window.LocalImageReactFeatures?.settings)return;
  const state=setupState||{},service=state.service||{},job=state.job,files=Array.isArray(state.models)?state.models:[];
  const activeJob=job?.status==='running',locked=busy||settingsSaving||setupRequestBusy||activeJob||!!service.starting||!!service.busy;
  const completeFiles=files.filter(file=>file.exists).length,allFiles=files.length>0&&completeFiles===files.length;
  const aiReady=service.ready===true,needsAttention=!!service.running&&!aiReady;
  $('ai-native-note').hidden=nativeSetup;
  $('ai-native-note').textContent=nativeReady?'Update the Local Image desktop app to use guided AI setup.':'Open the Local Image desktop app to choose folders, install ComfyUI, or download model files.';
  $('configure-ai').hidden=!nativeReady;
  $('ai-runtime-status').textContent=service.running?(aiReady?'Running':'Running · setup needed'):service.starting?'Starting':state.installation?'Ready to start':'Not selected';
  $('ai-runtime-status').classList.toggle('ready',!!service.running&&aiReady);$('ai-runtime-status').classList.toggle('attention',needsAttention);
  $('ai-runtime-path').textContent=state.installation?.path||'No installation selected';$('ai-runtime-path').title=state.installation?.path||'';
  const installPath=state.install_directory||state.managed_directory||state.configured_ai_directory||'';
  $('ai-install-path').textContent=installPath||'Choose a writable folder for the portable runtime';$('ai-install-path').title=installPath;
  $('ai-install-path-note').textContent=state.install_directory?'Model files can be stored separately in the folder below.':'A dedicated runtime subfolder is created inside this folder. Model files can be stored separately.';
  const portableStorage=[state.portable?.download_bytes?'Runtime download: '+setupBytes(state.portable.download_bytes)+'.':'',state.portable?.minimum_free_bytes?'Allow '+setupBytes(state.portable.minimum_free_bytes)+' for installation.':'',setupStorageText(state.storage?.portable_folder)].filter(Boolean).join(' ');
  $('ai-install-space').textContent=portableStorage;$('ai-install-space').hidden=!portableStorage;
  const runtimeIssue=service.reason||'ComfyUI is connected, but no supported AI model is ready.';
  const restartGuidance=/close ComfyUI/i.test(runtimeIssue)?'':' If you changed model folders or added files, close ComfyUI, then choose Start AI backend in Local Image to reconnect. Refresh afterward.';
  $('ai-runtime-detail').textContent=needsAttention?runtimeIssue+restartGuidance:service.running?'Connected to ComfyUI on this PC, port '+service.port+'.':service.starting?'ComfyUI is starting. You can keep this window open to follow its progress.':state.installation?state.installation.startable?'This installation is ready. Start the backend when you want to use local AI.':'This installation was found. Start it from ComfyUI, or install a portable copy below.':'Use an existing installation or install a portable copy for Local Image.';
  $('ai-model-path').textContent=state.model_directory||'No model folder selected';$('ai-model-path').title=state.model_directory||'';
  const modelStorage=setupStorageText(state.storage?.model_folder);$('ai-model-space').textContent=modelStorage;$('ai-model-space').hidden=!modelStorage;
  $('ai-models-status').textContent=aiReady?'Models connected':state.model_directory?'Folder selected':'Choose a folder';$('ai-models-status').classList.toggle('ready',aiReady);
  const fileRows=files.map(file=>{const li=document.createElement('li');li.classList.toggle('available',!!file.exists);const label=document.createElement('span');label.className='file-label';label.textContent=file.label||file.name;label.title=(file.folder?file.folder+'/':'')+file.name;const status=document.createElement('span');status.className='file-state';const bytes=file.exists?file.bytes:file.expected_bytes;status.textContent=(file.exists?'Ready':'Required')+(bytes?' · '+setupBytes(bytes):'');li.append(label,status);return li;});$('ai-model-files').replaceChildren(...fileRows);
  let summary=!setupState?'Checking your setup…':job?.status==='error'?job.error||job.message||'Setup could not finish. Check the details below.':activeJob?job.message||'Setting up local AI…':service.qwen_ready&&!service.flux_ready?'Qwen Image 2.1 is ready for cutouts, backgrounds and AI Remove. Other models are optional.':service.running&&aiReady?'Local AI is ready. Choose an installed model in the toolset you want to use.':service.starting?'Starting the AI backend…':needsAttention?'ComfyUI is running. Check the connection details or choose a supported model in Image Gen → Browse models.':!state.installation?'Choose how you want to run ComfyUI.':!state.model_directory?'Choose a folder for your AI models. Browse models in Image Gen to compare downloads.':allFiles?'Model files are installed. Start the backend to use local AI.':'Choose a supported model in Image Gen → Browse models, or install the optional AI Remove model below.';
  const folderConnection=state.model_folder_connection||modelFolderConnection;if(folderConnection&&folderConnection.status!=='unchanged')summary+=' '+folderConnection.message;
  $('ai-setup-state').textContent=summary;$('ai-setup-state').classList.toggle('error',job?.status==='error');$('ai-setup-state').classList.toggle('needs-attention',needsAttention);
  const candidates=Array.isArray(state.installations)?state.installations:[],select=$('ai-installation-choice'),previous=select.value;
  select.replaceChildren(...candidates.map(candidate=>{const option=document.createElement('option');option.value=candidate.id;option.textContent=(candidate.name||'ComfyUI')+' · '+candidate.path;return option;}));
  if(candidates.some(candidate=>candidate.id===previous))select.value=previous;else if(state.installation?.id)select.value=state.installation.id;
  $('ai-installations').hidden=!setupCandidatesOpen||!candidates.length;
  for(const id of ['ai-choose-runtime','ai-choose-install','ai-install','ai-start','ai-choose-models','ai-download','ai-use-installation','ai-eject'])$(id).disabled=!nativeSetup||locked;
  $('ai-browse-models').disabled=locked||generationLoading;
  $('ai-detect').disabled=setupRefreshRunning||locked;$('ai-refresh').disabled=setupRefreshRunning||setupRequestBusy;
  $('ai-start').disabled=!nativeSetup||locked||!service.can_start||!!service.running;
  $('ai-start').textContent=service.starting?'Starting…':service.running?'Backend running':'Start AI backend';
  $('ai-download').disabled=!nativeSetup||locked||!state.model_directory||allFiles;
  $('ai-download').textContent=allFiles?'FLUX models ready':'Download FLUX models';
  $('ai-use-installation').disabled=!nativeSetup||locked||!select.value;
  $('ai-installation-choice').disabled=locked;
  $('configure-ai').disabled=locked;
  $('ai-eject').disabled=!nativeSetup||locked||!service.can_eject;
  $('ai-device').textContent=service.device||(!service.running?'Start the backend to check your device.':'Release loaded models when you need GPU memory for another app.');
  $('ai-eject').title='Unload models from the GPU. Model files stay on disk; the next removal loads them again.';
  $('ai-job').hidden=!job;$('ai-job').classList.toggle('error',job?.status==='error');
  if(job){
    const names={install:'Installing ComfyUI',download:'Downloading FLUX models','download-models':'Downloading FLUX models',start:'Starting ComfyUI',eject:'Releasing GPU memory'};
    $('ai-job-title').textContent=job.status==='complete'?'Setup complete':job.status==='error'?'Setup needs attention':names[job.action]||'Setting up local AI';
    const progress=Number(job.progress);if(job.progress!=null&&Number.isFinite(progress)){$('ai-job-progress').value=Math.max(0,Math.min(100,progress));$('ai-job-percent').textContent=Math.round(progress)+'%';}else{$('ai-job-progress').removeAttribute('value');$('ai-job-percent').textContent='';}
    $('ai-job-message').textContent=job.error||job.message||job.phase||'';
    $('ai-job-bytes').textContent=job.total_bytes?setupBytes(job.downloaded_bytes)+' of '+setupBytes(job.total_bytes):job.downloaded_bytes?setupBytes(job.downloaded_bytes)+' downloaded':'';
  }
}
async function loadSetup(refresh=false){
  if(window.LocalImageReactFeatures?.settings)return window.LocalImageReactFeatures.settings.refresh(refresh);
  if(setupRefreshRunning)return;
  setupRefreshRunning=true;renderSetup();
  const wasActive=setupJobActive();
  try{setupState=await(await api('/api/local-remove/setup'+(refresh?'/detect':''))).json();if(modelFolderStartPending&&setupState.job?.status!=='running'){if(setupState.job?.status==='complete'&&setupState.service?.running){modelFolderConnection=null;delete setupState.model_folder_connection;}modelFolderStartPending=false;}if(wasActive&&!setupJobActive()){await loadSettings();await health();}}
  finally{setupRefreshRunning=false;renderSetup();scheduleSetupRefresh();}
}
async function runSetupAction(action,details={},success=''){
  if(!nativeSetup||setupRequestBusy||setupJobActive()||busy)return;
  setupRequestBusy=true;settingsSaving=true;settingsMessage('');renderSetup();controls();
  try{
    const result=await nativeRequest(action,null,details);if(!result)return;
    if(result.service)setupState=result;if(result.model_folder_connection)modelFolderConnection=result.model_folder_connection;if(action==='setupStart')modelFolderStartPending=true;
    if(success)settingsMessage(success);
    await loadSettings();await health();await loadSetup();
  }catch(error){settingsMessage(error.message,true);}
  finally{setupRequestBusy=false;settingsSaving=false;renderSetup();controls();scheduleSetupRefresh();}
}
async function showSettings(){
  if(busy||modalOpen())return;
  if(window.LocalImageReactFeatures?.settings){closeMenus();resetTransientInput();return window.LocalImageReactFeatures.settings.open('settings');}
  if($('ask-before-overwrite'))$('ask-before-overwrite').checked=askBeforeOverwrite;
  closeMenus();resetTransientInput();settingsMessage('');$('settings-dialog').showModal();renderSetup();
  const checks=await Promise.allSettled([loadSettings(),loadSetup()]);
  for(const check of checks)if(check.status==='rejected')settingsMessage(check.reason?.message||'Could not read AI setup.',true);
}
$('settings').onclick=showSettings;
$('ai-refresh').onclick=()=>loadSetup(true).catch(error=>settingsMessage(error.message,true));
$('ai-detect').onclick=async()=>{setupCandidatesOpen=true;settingsMessage('Looking for ComfyUI on this PC…');try{await loadSetup(true);settingsMessage(setupState?.installations?.length?'Choose the installation you want to use.':'No installation found in the usual locations. Choose a folder or install a dedicated copy.');}catch(error){settingsMessage(error.message,true);}};
$('ai-use-installation').onclick=()=>runSetupAction('setupUseInstallation',{installation_id:$('ai-installation-choice').value},'ComfyUI installation selected.');
$('ai-choose-runtime').onclick=()=>runSetupAction('setupChooseComfyDirectory',{},'ComfyUI folder selected.');
$('ai-choose-install').onclick=()=>runSetupAction('setupChooseInstallDirectory',{},'Portable installation folder selected.');
$('ai-install').onclick=()=>runSetupAction('setupInstall');
$('ai-choose-models').onclick=()=>runSetupAction('setupChooseModelDirectory',{},'Model folder selected.');
$('ai-browse-models').onclick=async()=>{if($('ai-browse-models').disabled)return;$('settings-dialog').close();await openModelBrowser();setWorkspace('generate');};
$('ai-download').onclick=()=>runSetupAction('setupDownloadModels');
$('ai-start').onclick=()=>runSetupAction('setupStart');
$('ai-eject').onclick=()=>runSetupAction('setupEject',{},'GPU unload requested. Files remain on disk.');
$('configure-ai').onclick=async()=>{
  if(busy||!nativeReady||setupJobActive()||setupRequestBusy)return;
  try{const configured=await nativeRequest('configureAi');if(!configured)return;await loadSettings();await health();await loadSetup();settingsMessage('AI connection saved.');}
  catch(error){settingsMessage(error.message,true);}
};
$('settings-close').onclick=()=>$('settings-dialog').close();
$('settings-dialog').addEventListener('keydown',event=>{
  if(event.key!=='Tab')return;
  const dialog=$('settings-dialog'),items=[...dialog.querySelectorAll('button,input,select,summary,a[href],[tabindex]')].filter(item=>!item.disabled&&item.tabIndex>=0&&item.getClientRects().length&&(!item.closest('details:not([open])')||item.tagName==='SUMMARY'));
  if(!items.length)return;
  const first=items[0],last=items[items.length-1];
  if(event.shiftKey&&document.activeElement===first){event.preventDefault();last.focus();}
  else if(!event.shiftKey&&document.activeElement===last){event.preventDefault();first.focus();}
});
$('settings-dialog').addEventListener('close',()=>{clearTimeout(setupTimer);setupTimer=null;resetTransientInput();});
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
  compositeImage=requested.cutout?.enabled?await loadImage('/api/local-remove/session/'+encodeURIComponent(requested.id)+'/preview?full=true&revision='+requested.revision):null;
  if(version!==requestVersion||session?.id!==requested.id)return;
  const nodes=await Promise.all(requested.layers.map(layer=>assets.layers.get(layer.id)));
  const holder=$('layer-stack');
  // Reuse already decoded images. New generation and merge only fetch the new patch.
  const expected=new Set(nodes);for(const child of [...holder.children])if(!expected.has(child))child.remove();
  nodes.forEach((node,index)=>{if(holder.children[index]!==node)holder.append(node);});
  syncLayerDisplay();paintPhoto();trackDocument(session);trimDisplayCache();syncCutoutFields();
  if(workspace==='cutout'&&tool==='move'&&session.cutout?.enabled)await prepareTransformAssets();
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
  if(window.__LOCAL_IMAGE_REACT__){publishEditorState();return;}
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
  if(window.__LOCAL_IMAGE_REACT__){publishEditorState();return;}
  if(!session)return;
  const holder=$('layers');holder.replaceChildren();
  for(const layer of [...session.layers].reverse()){
    const item=document.createElement('div');item.className='layer'+(layer.visible?'':' is-hidden');item.dataset.layerId=layer.id;item.hidden=!!layer.discarded;
    const visibility=document.createElement('button');visibility.className='visibility';visibility.disabled=busy;
    visibility.setAttribute('aria-label',(layer.visible?'Hide ':'Show ')+layer.name);visibility.title=(layer.visible?'Hide ':'Show ')+layer.name;
    visibility.append(svgIcon(layer.visible?'eye':'eye-off'));visibility.onclick=()=>{const current=session.layers.find(item=>item.id===layer.id);changeLayer(layer.id,{visible:!current.visible});};
    const label=document.createElement('div');label.className='layer-content';label.style.minWidth='0';
    const thumbnail=document.createElement('img');thumbnail.className='layer-thumbnail';thumbnail.alt='';thumbnail.loading='lazy';thumbnail.src='/api/local-remove/session/'+encodeURIComponent(session.id)+'/layer/'+encodeURIComponent(layer.id)+'/display';label.append(thumbnail);
    const name=document.createElement('div');name.className='layer-name';name.textContent=layer.name;name.title=layer.name;
    const meta=document.createElement('div');meta.className='meta';meta.textContent=layer.model_label||'Removal';label.append(name,meta);
    const discard=document.createElement('button');discard.className='discard';discard.append(svgIcon('close'));
    discard.setAttribute('aria-label','Discard '+layer.name);discard.title='Discard '+layer.name;discard.disabled=busy;discard.onclick=()=>changeLayer(layer.id,{discarded:true});
    item.append(visibility,label,discard);holder.append(item);
  }
  const original=document.createElement('div');original.className='layer base';
  const originalLabel=document.createElement('div'),originalName=document.createElement('div'),originalMeta=document.createElement('div');
  originalLabel.className='layer-content';const originalThumbnail=document.createElement('img');originalThumbnail.className='layer-thumbnail';originalThumbnail.alt='';originalThumbnail.src='/api/local-remove/session/'+encodeURIComponent(session.id)+'/base-display';originalLabel.append(originalThumbnail);
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
      if(session?.id===queue.confirmed.id&&session.cutout?.enabled)await refreshPreview();
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
  if(options.expectedNavigationEpoch!==undefined&&options.expectedNavigationEpoch!==documentNavigationEpoch)return;
  const navigationEpoch=++documentNavigationEpoch;
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
    if(navigationEpoch!==documentNavigationEpoch)return;
    session=data;collection=nextCollection||null;collectionIndex=collection?(options.index??collection.entries.findIndex(entry=>entry.id===data.entry_id||entry.session_id===data.id)):-1;
    previewImage=images[0];originalImage=images[0];compositeImage=null;showOriginal=false;brushPointer=null;trackDocument(session);
    if(!state&&data.cutout?.enabled)workspace='cutout';
    else if(!state&&(data.generation||data.upscale))workspace='generate';
    const restoreGeneration=!!data.generation&&!state&&activeTask!=='generate';
    if(restoreGeneration)generationModelId=data.generation.model||generationModelId;
    $('empty').hidden=true;stage.hidden=false;
    const scale=Math.min(1,3000/Math.max(previewImage.width,previewImage.height));
    setSizes(Math.round(previewImage.width*scale),Math.round(previewImage.height*scale));restoreView(state,images[1]);await refreshPreview();layerList();updateCollectionSession();renderCollection(true);updateToolChrome();
    if((workspace==='generate'||restoreGeneration)&&!generationModels.length){try{await loadGenerationModels();}catch{}}
    if(restoreGeneration&&!window.__LOCAL_IMAGE_REACT__)await restoreGenerationSettings(data.generation);
    renderHealth();
    if(workspace==='cutout')$('tool-hint').textContent=tool==='move'?'Drag the subject · Scale and rotate in the Transform panel':'Select an edge · Erase or Restore to refine';
    if(workspace==='generate')$('tool-hint').textContent='Describe an image in the Prompt studio';
    $('filename').textContent=session.name;$('filename').title=session.name+' · '+session.width+' × '+session.height+' · '+session.bit_depth+'-bit';
    $('save-note').textContent='File → Save editable project keeps the original and layers in a .lremove file. Image saves are flattened. Closing clears the working layers.';
    const navigation=collection&&!collection.local?'/remove?collection='+encodeURIComponent(collection.id)+'&index='+collectionIndex:'/remove?session='+encodeURIComponent(session.id);
    history.replaceState(null,'',navigation);
    message(state?'Selection and view restored.':session.project_name?'Project opened. Layers remain editable; use Export for a flattened image.':session.can_return?'Ready. Changes stay in this session until you save.':'Browser upload copy. Export downloads your finished image.');
    try{await recent();}catch{}
  }catch(error){if(navigationEpoch===documentNavigationEpoch)message(error.message,true);throw error;}finally{if(navigationEpoch===documentNavigationEpoch){setBusy(false);layerList();}}
  if(session?.id===data.id)await window.LocalImageGenerationBridge?.afterDocumentOpened(data);
}
async function recent(){
  const sessions=await(await api('/api/local-remove/sessions')).json();$('recent').replaceChildren();
  for(const data of sessions.slice(0,6)){
    const button=document.createElement('button');button.className='recent';button.textContent=data.name;button.title=data.name;button.disabled=busy;button.tabIndex=-1;button.setAttribute('role','menuitem');
    button.onclick=async()=>{if(busy)return;try{await openSession(await(await api('/api/local-remove/session/'+data.id)).json());}catch(error){message(error.message,true);}};
    $('recent').append(button);
  }
}
function openDocumentFiles(){if(!busy&&!closeInProgress)return nativeReady?openNative('openFiles'):$('file').click();}
function openDocumentFolder(){if(!busy&&!closeInProgress)return nativeReady?openNative('openFolder'):$('folder-file').click();}
$('open').onclick=openDocumentFiles;
$('open-folder').onclick=openDocumentFolder;
$('folder-file').onchange=async()=>{try{await openBrowserFiles($('folder-file').files);}catch(error){message(error.message,true);}finally{$('folder-file').value='';}};
$('file').onchange=async()=>{
  try{await openBrowserFiles($('file').files);}catch(error){message(error.message,true);}finally{$('file').value='';}
};
$('remove').onclick=async()=>{
  if(!session||busy||layerChangesPending()||!hasSelection||!operationReady()||settingsSaving)return;
  if(workspace==='cutout')return;
  const targetDocumentId=session.id;
  if(await window.LocalImageLayers?.prepareRetouch?.()===false)return;
  if(!session||session.id!==targetDocumentId||busy||!hasSelection)return;
  const healing=operation==='heal',requestModel=healing?'heal':aiProvider==='qwen'?'qwen':modelId,label=aiProvider==='qwen'?'Qwen Image 2.1':modelLabel(),requestHealMethod=healMethod;
  const progress=healing?'Healing selected area…':'Removing with '+label+' on your GPU…';
  activeTask='repair';setBusy(true);layerList();const started=Date.now();message(progress);
  const timer=setInterval(()=>message(progress+' '+Math.round((Date.now()-started)/1000)+'s'),1000);
  try{
    const monochrome=document.createElement('canvas');monochrome.width=mask.width;monochrome.height=mask.height;
    const context=monochrome.getContext('2d');context.fillStyle='black';context.fillRect(0,0,mask.width,mask.height);context.drawImage(mask,0,0);
    session=await json(url('/remove'),{mask:monochrome.toDataURL('image/png').split(',')[1],revision:session.revision,model:requestModel,target_layer_id:window.LocalImageLayers?.retouchTargetId?.()||null,...(healing?{heal_method:requestHealMethod}:requestModel==='qwen'?{variant:qwenVariant}:{})});
    clearSelection();undo=[];selectionRedo=[];await refreshPreview();layerList();updateCollectionSession();renderCollection();
    message((healing?'Heal layer':'Removal')+' added in '+((Date.now()-started)/1000).toFixed(1)+'s. Hide or discard its layer to compare.');
  }catch(error){message(error.message,true);}finally{clearInterval(timer);activeTask=null;setBusy(false);layerList();try{await recent();}catch{}}
};
function loadOverwritePreference(){
  askBeforeOverwrite=true;
  try{askBeforeOverwrite=localStorage.getItem(OVERWRITE_PREFERENCE)!=='false';}catch{}
  if($('ask-before-overwrite'))$('ask-before-overwrite').checked=askBeforeOverwrite;
}
function setOverwritePreference(ask){
  askBeforeOverwrite=ask;if($('ask-before-overwrite'))$('ask-before-overwrite').checked=ask;
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
async function save(mode,allowPending=false){
  if(window.LocalImageGenerationBridge){const target=await window.LocalImageGenerationBridge.prepareDocumentCommand('save');if(!target.allowed)return;if(target.forceExport)mode='export';}
  if(!session||busy||layerChangesPending()||modalOpen())return;
  if(workspace==='cutout'&&(hasSelection||points.length)&&!allowPending){
    message(points.length?'Finish or clear the path before exporting.':'Selection not applied. Apply or clear it before exporting.');
    (window.LocalImageLayers&&session?.layer_stack?(points.length?$('finish'):$('cutout-refine')):$('cutout-export-state')).focus();return;
  }
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
function hasWorkingLayers(data){return !!data?.layers?.length||!!data?.cutout||!!data?.generation||!!data?.upscale||!!data?.source_attribution||!!data?.layer_stack?.some(layer=>layer.kind!=='original'||layer.visible===false||layer.discarded||(layer.opacity??1)!==1||(layer.transform?.offset_x||0)!==0||(layer.transform?.offset_y||0)!==0||(layer.transform?.scale??1)!==1||(layer.transform?.rotation||0)!==0);}
function pendingSelection(id){
  if(session?.id===id)return hasSelection||points.length>0;
  const state=viewStates.get(id);return !!(state?.hasSelection||state?.points?.length);
}
function projectNeedsSave(data){return !data.project_saved||data.project_saved_revision!==data.revision||data.project_dirty===true;}
function downloadFile(result){const link=document.createElement('a');link.href=result.download;link.download=result.name;link.click();}
async function saveEditableProject(data=session,{forClose=false,saveAs=false}={}){
  if(!forClose&&window.LocalImageGenerationBridge){const target=await window.LocalImageGenerationBridge.prepareDocumentCommand('project');if(!target.allowed)return false;data=session;}
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
function openDocumentProject(){if(!busy&&!closeInProgress)return nativeProjects?openNative('openProject'):$('project-file').click();}
$('open-project').onclick=openDocumentProject;
$('project-file').onchange=async()=>{try{await importProjectFile($('project-file').files[0]);}finally{$('project-file').value='';}};

function resolveClose(choice){
  const pending=closePrompt;if(!pending)return;closePrompt=null;
  $('close-dialog').close();updateBrushCursor();pending.resolve(choice);
}
function confirmClose(documents,all=false){
  closeMenus();endGesture();spaceHeld=false;resetDrop();
  const dirty=documents.filter(data=>hasWorkingLayers(data)&&projectNeedsSave(data));
  const pending=documents.some(data=>pendingSelection(data.id));
  $('close-title').textContent=all?'Close Local Image?':'Close image?';
  $('close-warning').textContent=dirty.length
    ?'Closing will permanently clear the working layers. Save a project first to edit them again.'
    :documents.some(hasWorkingLayers)?'Closing clears the working layers. Saved .lremove projects remain available to reopen.':'Closing clears the pending selection.';
  $('close-summary').textContent=documents.filter(data=>hasWorkingLayers(data)||pendingSelection(data.id)).map(data=>{
    const count=data.layers.length;
    return data.name+' · '+(data.upscale?'Upscaled image':data.generation?'Generated image':data.source_attribution?'Imported stock image':data.cutout?'Editable cutout':count+(count===1?' layer':' layers'))+(hasWorkingLayers(data)&&!projectNeedsSave(data)?' · saved in '+data.project_name:'');
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
    endGesture();requestVersion++;session=null;originalImage=null;previewImage=null;showOriginal=false;points=[];undo=[];selectionRedo=[];hasSelection=false;brushPointer=null;
    $('layer-stack').replaceChildren();$('photo-image').removeAttribute('src');stage.hidden=true;$('empty').hidden=false;
    if(!window.__LOCAL_IMAGE_REACT__){$('layers').replaceChildren();const help=document.createElement('p');help.className='muted';help.textContent='Open an image to begin.';$('layers').append(help);$('layer-count').textContent='';}
    $('filename').textContent='No image open';$('filename').title='';$('restore').hidden=true;
    $('before').setAttribute('aria-pressed','false');history.replaceState(null,'','/remove');
    $('save-note').textContent='Open a .lremove project to continue with its original and editable layers.';
  }
  window.LocalImageGenerationBridge?.noteClosed(ids);
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
async function closeCurrentImage(){if(window.LocalImageGenerationBridge){const target=await window.LocalImageGenerationBridge.prepareDocumentCommand('close');if(!target.allowed)return false;}return session?closeDocuments([session]):true;}
$('close-image').onclick=closeCurrentImage;
async function handleNativeClose(id){
  let approved=false;
  try{approved=await closeDocuments([...openDocuments.values()],true);}catch(error){message(error.message,true);}
  nativeBridge?.postMessage({action:'closeReady',id,approved});
}
window.addEventListener('beforeunload',event=>{
  if([...openDocuments.values()].some(data=>hasWorkingLayers(data)||pendingSelection(data.id))){event.preventDefault();event.returnValue='';}
});

// Cutout edits are committed with the document revision, so a navigation or stale
// response cannot attach a mask or background to a different photograph.
function qwenReady(){
  if(!qwenStatus)return false;
  const variant=qwenStatus.variants?.[qwenVariant]||qwenStatus.variants?.find?.(item=>item.id===qwenVariant);
  return qwenStatus.ready===true&&variant?.available===true;
}
function cutoutWorkflowState(){
  if(!session)return{title:'Start with a photo',description:'Open a photo, then remove its background.',status:'No photo open'};
  if(busy)return{title:'Updating your cutout',description:'Your local image operation is running.',status:'Working…'};
  if(showOriginal)return{title:'Viewing the original',description:'Choose Back to edits to continue refining your cutout.',status:'Original view'};
  if(!session.cutout?.enabled)return{title:'Remove the background',description:'Choose a Qwen model and remove the background to isolate your subject.',status:'Ready to cut out'};
  if(points.length)return{title:'Finish your edge selection',description:'Click the first point or press Enter to close the path.',status:points.length+' path points'};
  if(hasSelection)return{title:'Refine your cutout',description:(cutoutOperation==='restore'?'Restore original pixels':'Erase pixels')+' in the highlighted area.',status:'Selection ready'};
  return{title:'Refine or compose',description:'Brush or draw a pen selection to refine edges. Choose a background and adjust the shadow.',status:'Cutout ready'};
}
function setWorkspace(value,{allowBusy=false}={}){
  if(value==='generate'&&window.LocalImageGenerationBridge&&!window.LocalImageGenerationBridge.ownsWorkspaceChange())return window.LocalImageGenerationBridge.enterWorkspace();
  if((busy&&!(allowBusy&&(window.LocalImageAssetsBridge?.ownsOperation()||window.LocalImageGenerationBridge?.ownsWorkspaceChange())))||!['retouch','cutout','generate'].includes(value))return;
  if(value==='generate'&&session?.cutout?.enabled&&!session.generation)generationTargetSession=session.id;
  endGesture();workspace=value;updateToolChrome();controls();paintMask();renderHealth();
  if(value==='retouch'&&tool==='move')selectTool('brush');
  if(value==='cutout')$('cutout-panel-scroll').scrollTop=0;
  $('tool-hint').textContent=value==='cutout'?'Select an edge · Erase or Restore to refine':operation==='heal'?healHint():'Select an object · Remove to apply';
  message(value==='cutout'?'Cutout tools · Refine the subject, replace its background, and add a shadow.':'Retouch tools · Heal blemishes or remove distractions.');
  if(value==='cutout')loadBackgroundLibraries().catch(error=>message(error.message,true));
  if(value==='cutout'&&nativeSetup&&!qwenDownload)refreshQwenDownload();
  if(value==='generate'){loadGenerationModels().catch(error=>message(error.message,true));$('tool-hint').textContent='Describe an image in the Prompt studio';message('Image Gen · Create from a prompt or guide the result with images.');}
  if(session)paintPhoto();
}
function updateCutoutControls(){
  document.body.dataset.persona=workspace;
  const cutout=workspace==='cutout',retouch=workspace==='retouch',generating=workspace==='generate',active=!!session&&!busy&&!layerChangesPending(),editable=active&&!!session.cutout?.enabled&&!showOriginal;
  for(const mode of ['retouch','cutout','generate']){$('workspace-'+mode).setAttribute('aria-pressed',String(workspace===mode));$('workspace-'+mode).disabled=busy;}
  $('persona-description').textContent={retouch:'Photo retouching',cutout:'Subject and background',generate:'Local image generation'}[workspace];
  $('retouch-modes').hidden=!retouch;$('cutout-modes').hidden=!cutout;if($('retouch-panel'))$('retouch-panel').hidden=!retouch;$('cutout-panel').hidden=!cutout;if($('generation-panel'))$('generation-panel').hidden=!generating;
  $('selection-context').hidden=generating;$('generation-context').hidden=!generating;
  $('heal-brush').hidden=!retouch;$('remove').hidden=!retouch;$('cutout-refine').hidden=!cutout;
  $('empty-open').hidden=generating;$('empty-help').textContent=generating?'Describe an image in the Prompt studio to get started.':'Drop an image or folder here';
  for(const button of document.querySelectorAll('[data-tool]')){button.hidden=generating;button.disabled=!active||generating;}
  $('move-subject').hidden=!cutout;$('move-subject').disabled=!editable;
  $('cutout-restore').disabled=!active||showOriginal;$('cutout-erase').disabled=!active||showOriginal;
  $('cutout-restore').textContent=session?.cutout?.enabled?'Restore':'Keep';
  updateHistoryControls(active);
  $('cutout-restore').setAttribute('aria-pressed',String(cutoutOperation==='restore'));$('cutout-erase').setAttribute('aria-pressed',String(cutoutOperation==='erase'));
  $('cutout-refine').textContent=cutoutOperation==='restore'?(session?.cutout?.enabled?'Restore selection':'Keep selection'):'Erase selection';$('cutout-refine').disabled=!active||showOriginal||!hasSelection;
  $('cutout-edit-label').textContent=$('cutout-refine').textContent;
  $('cutout-remove').disabled=!active||!qwenReady()||showOriginal;$('cutout-remove').textContent=session?.cutout?.enabled?'Recalculate cutout':'Remove background';
  $('cutout-reset').disabled=!active||!session?.cutout||showOriginal;$('cutout-reset').textContent=session?.cutout?.enabled?'Restore full image':'Show saved cutout';$('cutout-export').disabled=!active;
  const pending=hasSelection||points.length>0;
  $('cutout-export-state').textContent=!session?'Open an image to export.':pending?(points.length?'Path not finished. Close the path, then apply it; export uses current pixels.':'Selection not applied. Apply it below, or export the current image.'):'Current image ready to export. PNG keeps transparent pixels.';
  $('cutout-export-state').classList.toggle('pending',pending);
  $('cutout-apply-selection').hidden=!pending;$('cutout-apply-selection').disabled=$('cutout-refine').disabled||points.length>0;
  $('cutout-apply-selection').title=$('cutout-refine').textContent;$('cutout-export').textContent=pending?'Export current image…':'Export PNG…';
  $('cutout-export').title=pending?'Exports applied pixels only. The pending selection is not included.':'Export the current composition as PNG.';
  $('qwen-variant').disabled=busy;$('retouch-qwen-variant').disabled=busy;$('ai-provider').disabled=busy;
  for(const id of ['cutout-feather','cutout-feather-value','background-mode','background-color','background-open','background-folder','background-library','shadow-enabled']){const control=$(id);if(control)control.disabled=!editable;}
  for(const id of ['transform-x','transform-y','transform-scale','transform-scale-value','transform-rotation','transform-rotation-value','transform-reset','transform-move'])$(id).disabled=!editable;
  $('transform-move').setAttribute('aria-pressed',String(cutout&&tool==='move'&&!handActive));
  for(const id of ['shadow-opacity','shadow-blur','shadow-x','shadow-y','shadow-squeeze']){ $(id).disabled=!editable||!$('shadow-enabled').checked;$(id+'-value').disabled=$(id).disabled; }
  $('background-prompt').disabled=!editable;$('background-generate').disabled=!editable||!qwenReady()||!$('background-prompt').value.trim();
  $('background-color').hidden=$('background-mode').value!=='color';
  for(const button of $('background-grid')?.children||[])button.disabled=!editable;
  $('cutout-state').textContent=session?.cutout?.enabled?'Editable cutout':'No cutout';
  const variant=qwenStatus?.variants?.[qwenVariant]||qwenStatus?.variants?.find?.(item=>item.id===qwenVariant);
  $('qwen-state').textContent=qwenReady()?'ComfyUI ready · '+(qwenVariant==='int8'?'Compact INT8':'Full BF16'):variant?.reason||qwenStatus?.reason||'Qwen models or ComfyUI workflow unavailable. Open Settings to check your backend.';
  $('qwen-state').classList.toggle('error',!!qwenStatus&&!qwenReady());
  $('qwen-download').disabled=!nativeSetup||busy||!!qwenDownload?.running;
  const downloadVariant=qwenDownload?.variants?.find(item=>item.id===qwenVariant);
  $('qwen-download').textContent=downloadVariant?.installed?'Verify selected model':downloadVariant?'Download '+qwenVariant.toUpperCase()+' · '+setupBytes(downloadVariant.missing_bytes):'Download selected model';
  $('qwen-download-progress').hidden=!qwenDownload?.running;$('qwen-download-progress').value=qwenDownload?.progress||0;
  $('qwen-download-message').textContent=qwenDownload?.message||(!nativeSetup?'Open the desktop app to download model files.':'Downloads the selected model and its required text encoder and VAE.');
  updateStudioTabs();updateGenerationControls();
}
function syncCutoutFields(){
  const data=session?.cutout||{},background=data.background||{},shadow=data.shadow||{};
  setRangeValue('cutout-feather',data.feather||0);
  $('background-mode').value=background.mode||'transparent';$('background-color').value=background.color||'#e8e5df';
  $('background-name').textContent=background.mode==='image'?(background.name||'Image background'):background.mode==='color'?'Solid color background':'PNG export preserves transparency.';
  $('shadow-enabled').checked=!!shadow.enabled;
  const values={'shadow-opacity':(shadow.opacity??.3)*100,'shadow-blur':shadow.blur??18,'shadow-x':shadow.offset_x??12,'shadow-y':shadow.offset_y??20,'shadow-squeeze':(shadow.squeeze??1)*100};
  for(const [id,value]of Object.entries(values))setRangeValue(id,value);
  const transform=subjectTransform();$('transform-x').value=String(transform.offset_x);$('transform-y').value=String(transform.offset_y);
  setRangeValue('transform-scale',transform.scale*100);setRangeValue('transform-rotation',transform.rotation);
  updateCutoutControls();
}
function selectionPayload(){
  const monochrome=document.createElement('canvas');monochrome.width=mask.width;monochrome.height=mask.height;
  const context=monochrome.getContext('2d');context.fillStyle='black';context.fillRect(0,0,mask.width,mask.height);context.drawImage(mask,0,0);
  return monochrome.toDataURL('image/png').split(',')[1];
}
async function cutoutEdit(path,body,method='POST',label='Updating cutout…',clear=false){
  if(window.LocalImageGenerationBridge?.hasHiddenEditor())return;
  if(!session||busy||cutoutRequestRunning||layerChangesPending())return;
  const requested=session.id,revision=session.revision;cutoutRequestRunning=true;activeTask='cutout';setBusy(true);message(label);$('progress-label').textContent=label;
  const started=Date.now(),timer=setInterval(()=>{$('progress-label').textContent=label+' '+Math.round((Date.now()-started)/1000)+'s';},1000);
  try{
    const data=body instanceof FormData?await(await api('/api/local-remove/session/'+encodeURIComponent(requested)+path,{method,body})).json():await json('/api/local-remove/session/'+encodeURIComponent(requested)+path,{...body,revision},method);
    trackDocument(data);if(session?.id!==requested)return;session=data;
    if(clear){clearSelection();undo=[];selectionRedo=[];}
    await refreshPreview();layerList();updateCollectionSession();renderCollection();message('Cutout updated. Save a project to keep it editable, or export PNG.');
  }catch(error){message(error.message,true);syncCutoutFields();}
  finally{clearInterval(timer);cutoutRequestRunning=false;activeTask=null;setBusy(false);}
}
async function uploadBackground(file){
  if(!file||!session||busy)return;
  const form=new FormData();form.append('file',file);form.append('revision',String(session.revision));
  await cutoutEdit('/cutout/background',form,'POST','Loading background…');
}
async function loadBackgroundLibraries(){
  if(window.__LOCAL_IMAGE_REACT__)return;
  const data=await(await api('/api/local-remove/backgrounds')).json();backgroundLibraries=data.libraries||[];renderBackgroundLibraries();
}
function renderBackgroundLibraries(){
  if(window.__LOCAL_IMAGE_REACT__)return;
  const choice=$('background-library'),previous=choice.value;choice.replaceChildren();
  for(const library of backgroundLibraries){const option=document.createElement('option');option.value=library.id;option.textContent=library.name;choice.append(option);}
  if(browserBackgrounds.length){const option=document.createElement('option');option.value='browser';option.textContent='Selected background folder';choice.append(option);}
  const ids=backgroundLibraries.map(item=>item.id);if(browserBackgrounds.length)ids.push('browser');choice.value=ids.includes(previous)?previous:ids[0]||'';choice.hidden=!ids.length;
  renderBackgroundGrid();
}
function renderBackgroundGrid(){
  if(window.__LOCAL_IMAGE_REACT__)return;
  const grid=$('background-grid'),library=backgroundLibraries.find(item=>item.id===$('background-library').value);
  const entries=$('background-library').value==='browser'?browserBackgrounds:library?.entries||[];grid.replaceChildren();grid.hidden=!entries.length;
  for(const entry of entries){const button=document.createElement('button'),image=document.createElement('img');button.title=entry.name;button.setAttribute('aria-label','Use '+entry.name);image.alt=entry.name;image.loading='lazy';image.src=entry.thumbnail;button.append(image);
    button.onclick=()=>entry.file?uploadBackground(entry.file):cutoutEdit('/cutout/library-background',{library_id:library.id,entry_id:entry.id},'POST','Applying background…');grid.append(button);}
  updateCutoutControls();
}
function shadowPayload(){return{enabled:$('shadow-enabled').checked,opacity:Number($('shadow-opacity').value)/100,blur:Number($('shadow-blur').value),offset_x:Number($('shadow-x').value),offset_y:Number($('shadow-y').value),squeeze:Number($('shadow-squeeze').value)/100,color:'#000000'};}
function subjectTransform(){return{offset_x:0,offset_y:0,scale:1,rotation:0,...session?.cutout?.transform};}
async function prepareTransformAssets(){
  if(!session?.cutout?.enabled)return;
  const id=session.id,revision=session.revision;if(transformAssets?.sessionId===id&&transformAssets?.revision===revision)return;
  const prefix='/api/local-remove/session/'+encodeURIComponent(id)+'/cutout/';
  const [foreground,background]=await Promise.all([loadImage(prefix+'foreground?full=true&revision='+revision),loadImage(prefix+'background-preview?full=true&revision='+revision)]);
  if(session?.id===id&&session.revision===revision)transformAssets={sessionId:id,revision,foreground,background};
}
function paintTransformPreview(transform){
  if(!transformAssets)return;
  const width=baseCanvas.width,height=baseCanvas.height,ratio=pixelRatio();
  $('photo-image').hidden=true;overlay.hidden=true;bc.clearRect(0,0,width,height);bc.drawImage(transformAssets.background,0,0,width,height);
  bc.save();bc.translate((session.width-1)/2/ratio+transform.offset_x/ratio,(session.height-1)/2/ratio+transform.offset_y/ratio);bc.rotate(transform.rotation*Math.PI/180);bc.scale(transform.scale,transform.scale);bc.drawImage(transformAssets.foreground,-(session.width-1)/2/ratio,-(session.height-1)/2/ratio,width,height);bc.restore();
  $('transform-x').value=String(transform.offset_x);$('transform-y').value=String(transform.offset_y);
  dc.clearRect(0,0,draft.width,draft.height);
  const bounds=session.cutout_bounds;if(bounds){dc.save();dc.translate((session.width-1)/2/ratio+transform.offset_x/ratio,(session.height-1)/2/ratio+transform.offset_y/ratio);dc.rotate(transform.rotation*Math.PI/180);dc.scale(transform.scale,transform.scale);dc.strokeStyle='#b9d8f5';dc.lineWidth=1/zoom/transform.scale;dc.strokeRect((bounds[0]-(session.width-1)/2)/ratio,(bounds[1]-(session.height-1)/2)/ratio,(bounds[2]-bounds[0])/ratio,(bounds[3]-bounds[1])/ratio);dc.restore();}
}
function setQwenVariant(value){qwenVariant=value;$('qwen-variant').value=value;$('retouch-qwen-variant').value=value;try{localStorage.setItem('local-remove-qwen-variant',value);}catch{}controls();renderHealth();}
$('workspace-retouch').onclick=()=>setWorkspace('retouch');$('workspace-cutout').onclick=()=>setWorkspace('cutout');
$('qwen-variant').onchange=()=>setQwenVariant($('qwen-variant').value);$('retouch-qwen-variant').onchange=()=>setQwenVariant($('retouch-qwen-variant').value);
$('ai-provider').onchange=()=>{aiProvider=$('ai-provider').value;updateToolChrome();renderHealth();controls();};
$('cutout-restore').onclick=()=>{cutoutOperation='restore';paintMask();controls();};$('cutout-erase').onclick=()=>{cutoutOperation='erase';paintMask();controls();};
$('cutout-remove').onclick=()=>cutoutEdit('/cutout',{variant:qwenVariant},'POST','Removing background with Qwen…',true);
$('cutout-reset').onclick=()=>cutoutEdit('/cutout',{enabled:!session?.cutout?.enabled},'PATCH','Updating cutout visibility…',true);
$('cutout-refine').onclick=()=>{if(hasSelection&&!showOriginal)cutoutEdit('/cutout/refine',{mask:selectionPayload(),operation:cutoutOperation==='restore'&&!session?.cutout?.enabled?'replace':cutoutOperation},'POST','Refining subject edges…',true);};
$('cutout-undo').onclick=undoEdit;$('cutout-redo').onclick=redoEdit;
function setRangeValue(id,value){const text=String(Math.round(value*1000000)/1000000);$(id).value=text;$(id+'-value').value=text;}
function bindRangeNumber(id,commit){
  const range=$(id),number=$(id+'-value');
  range.oninput=()=>{number.value=range.value;};range.onchange=()=>commit(Number(range.value));
  number.oninput=()=>{if(number.value!==''&&number.checkValidity())range.value=number.value;};
  number.onchange=()=>{
    if(number.value===''||!Number.isFinite(Number(number.value))||!number.checkValidity()){
      message('Enter '+number.min+' to '+number.max+' in steps of '+number.step+'. The value was not applied.',true);syncCutoutFields();return;
    }
    range.value=number.value;commit(Number(number.value));
  };
  number.onkeydown=event=>{if(event.key==='Enter'){event.preventDefault();number.blur();}else if(event.key==='Escape'){event.preventDefault();syncCutoutFields();number.blur();}};
}
bindRangeNumber('cutout-feather',value=>cutoutEdit('/cutout',{feather:value},'PATCH','Softening cutout edges…'));
$('background-mode').onchange=()=>{if($('background-mode').value==='image'&&!session?.cutout?.background?.asset){$('background-mode').value=session?.cutout?.background?.mode||'transparent';$('background-file').click();return;}cutoutEdit('/cutout',{background:{mode:$('background-mode').value,color:$('background-color').value}},'PATCH','Changing background…');};
$('background-color').onchange=()=>cutoutEdit('/cutout',{background:{mode:'color',color:$('background-color').value}},'PATCH','Changing background color…');
$('background-open').onclick=()=>$('background-file').click();
$('background-file').onchange=async()=>{try{await uploadBackground($('background-file').files[0]);}finally{$('background-file').value='';}};
$('background-folder').onclick=async()=>{if(!nativeReady){$('background-folder-file').click();return;}try{const result=await nativeRequest('chooseBackgroundFolder');if(result)await loadBackgroundLibraries();}catch(error){message(error.message,true);}};
$('background-folder-file').onchange=()=>{for(const item of browserBackgrounds)URL.revokeObjectURL(item.thumbnail);browserBackgrounds=Array.from($('background-folder-file').files||[]).filter(supportedFile).map((file,index)=>({id:String(index),file,name:file.name,thumbnail:URL.createObjectURL(file)}));renderBackgroundLibraries();$('background-library').value='browser';renderBackgroundGrid();$('background-folder-file').value='';};
$('background-library').onchange=renderBackgroundGrid;
$('background-prompt').oninput=updateCutoutControls;
$('background-generate').onclick=()=>{const prompt=$('background-prompt').value.trim();if(prompt)cutoutEdit('/cutout/generate-background',{prompt,variant:qwenVariant},'POST','Generating an empty background with Qwen…');};
for(const id of ['shadow-opacity','shadow-blur','shadow-x','shadow-y','shadow-squeeze'])bindRangeNumber(id,()=>cutoutEdit('/cutout',{shadow:shadowPayload()},'PATCH','Updating shadow…'));
$('shadow-enabled').onchange=()=>cutoutEdit('/cutout',{shadow:shadowPayload()},'PATCH','Updating shadow…');
$('transform-move').onclick=()=>{if(session?.cutout?.enabled)selectTool('move');};
$('transform-reset').onclick=()=>cutoutEdit('/cutout',{transform:{offset_x:0,offset_y:0,scale:1,rotation:0}},'PATCH','Resetting subject position…');
for(const [id,field,factor]of [['transform-x','offset_x',1],['transform-y','offset_y',1],['transform-scale','scale',.01],['transform-rotation','rotation',1]]){
  const commit=value=>cutoutEdit('/cutout',{transform:{[field]:value*factor}},'PATCH','Transforming subject…');
  if($(id+'-value'))bindRangeNumber(id,commit);
  else $(id).onchange=()=>{if($(id).value!==''&&$(id).checkValidity()&&Number.isFinite(Number($(id).value)))commit(Number($(id).value));else{message('Enter a valid subject position from -100000 to 100000 pixels.',true);syncCutoutFields();}};
}
$('cutout-apply-selection').onclick=()=>{if(!$('cutout-refine').disabled&&!points.length)$('cutout-refine').click();};
$('cutout-export').onclick=()=>{outputFormat='png';$('output-format').value='png';updateDocumentState();save('export',true);};
async function refreshQwenDownload(){
  clearTimeout(qwenDownloadTimer);qwenDownloadTimer=null;
  try{qwenDownload=await(await api('/api/local-remove/qwen/download')).json();updateCutoutControls();
    if(qwenDownload.running)qwenDownloadTimer=setTimeout(refreshQwenDownload,2000);
    else if(qwenDownload.phase==='complete'){qwenStatus=await(await api('/api/local-remove/qwen/status')).json();controls();renderHealth();}
  }catch(error){$('qwen-download-message').textContent=error.message;}
}
$('qwen-download').onclick=async()=>{if(!nativeSetup||busy||qwenDownload?.running)return;try{await nativeRequest('setupDownloadQwen',null,{variant:qwenVariant});await refreshQwenDownload();}catch(error){message(error.message,true);}};

function updateStudioTabs(){
  for(const button of document.querySelectorAll('[data-studio-tab]')){const selected=button.dataset.studioTab===cutoutStudioTab;button.setAttribute('aria-selected',String(selected));button.tabIndex=selected?0:-1;}
  for(const panel of document.querySelectorAll('[data-studio-panel]'))panel.hidden=panel.dataset.studioPanel!==cutoutStudioTab;
  $('studio-heading').textContent={subject:'Subject',background:'Background',transform:'Transform subject'}[cutoutStudioTab];
  $('cutout-panel-scroll').setAttribute('aria-labelledby','studio-'+cutoutStudioTab+'-tab');
  for(const button of document.querySelectorAll('[data-gen-tab]')){const selected=button.dataset.genTab===generationStudioTab;button.setAttribute('aria-selected',String(selected));button.tabIndex=selected?0:-1;}
  for(const panel of document.querySelectorAll('[data-gen-panel]'))panel.hidden=panel.dataset.genPanel!==generationStudioTab;
  $('generation-panel-scroll')?.setAttribute('aria-labelledby','gen-'+generationStudioTab+'-tab');
}
function selectStudioTab(value){cutoutStudioTab=value;updateStudioTabs();$('cutout-panel-scroll').scrollTop=0;}
function selectGenerationTab(value){if(window.__LOCAL_IMAGE_REACT__)return;generationStudioTab=value;updateStudioTabs();$('generation-panel-scroll').scrollTop=0;}
function wireStudioTabs(selector,key,select){
  const tabs=[...document.querySelectorAll(selector)];
  for(const button of tabs){
    button.onclick=()=>select(button.dataset[key]);
    button.addEventListener('keydown',event=>{const available=tabs.filter(tab=>!tab.hidden&&!tab.disabled),index=available.indexOf(button);let next;if(event.key==='ArrowRight')next=(index+1)%available.length;else if(event.key==='ArrowLeft')next=(index+available.length-1)%available.length;else if(event.key==='Home')next=0;else if(event.key==='End')next=available.length-1;else return;event.preventDefault();available[next]?.focus();if(available[next])select(available[next].dataset[key]);});
  }
}
wireStudioTabs('[data-studio-tab]','studioTab',selectStudioTab);wireStudioTabs('[data-gen-tab]','genTab',selectGenerationTab);
$('filmstrip-toggle').onclick=()=>{filmstripCollapsed=!filmstripCollapsed;$('filmstrip').hidden=filmstripCollapsed||!collection||collection.entries.length<2;$('filmstrip-toggle').textContent=filmstripCollapsed?'▴':'▾';$('filmstrip-toggle').setAttribute('aria-expanded',String(!filmstripCollapsed));$('filmstrip-toggle').setAttribute('aria-label',filmstripCollapsed?'Expand filmstrip':'Collapse filmstrip');};
$('filmstrip-size').oninput=()=>{$('filmstrip').style.setProperty('--thumbnail-size',$('filmstrip-size').value+'px');};

const generationModel=()=>window.LocalImageReactFeatures?.generation?.modelFor(window.LocalImageReactFeatures.generation.activeDraftKey())||generationModels.find(item=>item.id===generationModelId);
const generationVariant=()=>{const feature=window.LocalImageReactFeatures?.generation;return generationModel()?.variants?.find(item=>item.id===(feature?feature.getSnapshot().drafts[feature.activeDraftKey()].variant:$('gen-variant')?.value));};
function historicalGenerationModel(saved){return{id:saved.model,label:(saved.model==='flux2-dev'?'FLUX.2 Dev':saved.model)+' · saved project',historical:true,available:false,reason:'This historical model is outside the current catalog. Choose a supported model to generate again.',variants:[{id:saved.variant||'default',label:(saved.variant||'Saved precision').toUpperCase(),available:false}],capabilities:{text_to_image:false,max_references:0},defaults:{variant:saved.variant||'default',steps:saved.steps||20,guidance:saved.guidance||1},limits:{min_steps:1,max_steps:100,min_guidance:1,max_guidance:10}};}
function renderGenerationModelOptions(){
  if(window.__LOCAL_IMAGE_REACT__)return;
  $('gen-model').replaceChildren();for(const model of generationModels){const option=document.createElement('option');option.value=model.id;const benefit=model.short_benefit||{'qwen':'Edits & alpha','z-image-turbo':'Fast images','flux2-klein-4b':'Fast edits','flux2-klein-9b':'Detailed edits','hidream-o1':'Text & layouts','flux2-dev':'Detailed edits'}[model.id]||model.benefit||model.description;option.textContent=model.label+(!model.historical&&benefit?' · '+benefit:'');option.title=model.label+' — '+(model.benefit||model.description||benefit||'');$('gen-model').append(option);}$('gen-model').value=generationModelId;
}
function loadGenerationModels(force=false){
  if(window.LocalImageReactFeatures?.generation){const feature=window.LocalImageReactFeatures.generation;return feature.refreshModels(force).then(()=>{generationModels=feature.getSnapshot().models;});}
  if(generationLoading){if(force)generationRefreshQueued=true;return generationLoadPromise;}
  generationLoading=true;updateGenerationControls();if($('refine-dialog').open)updateRefineControls();
  generationLoadPromise=(async()=>{let failure;
    try{const data=await(await api('/api/local-remove/generation/models'+(force?'?refresh=true':''))).json();generationModels=data.models||[];
      if(!generationModels.some(item=>item.id===generationModelId)){if(session?.generation?.model===generationModelId)generationModels.push(historicalGenerationModel(session.generation));else generationModelId=data.default_model||generationModels[0]?.id||'qwen';}
      renderGenerationModelOptions();syncGenerationModel();
    }catch(error){failure=error;}finally{generationLoading=false;updateGenerationControls();renderHealth();if($('refine-dialog').open)updateRefineControls();}
    if(generationRefreshQueued){generationRefreshQueued=false;return await loadGenerationModels(true);}if(failure)throw failure;
  })();return generationLoadPromise;
}
function generationWorkflowLimits(model,hasReferences=false){const limits=model?.limits||{};return hasReferences&&limits.reference_dimensions?{...limits,...limits.reference_dimensions}:limits;}
function generationCanvasBounds(model,hasReferences=false){
  const limits=generationWorkflowLimits(model,hasReferences);
  if(window.LocalImageGenerationSize)return window.LocalImageGenerationSize.sizeLimits(limits);
  const axis=name=>({min:limits[name]?.min||limits.min_dimension||1,max:limits[name]?.max||limits.max_dimension||Infinity,step:limits[name]?.step||limits.dimension_step||1});
  return{width:axis('width'),height:axis('height'),pixels:limits.max_pixels||Infinity};
}
function generationCanvasValid(width,height,model,hasReferences=false){
  const limits=generationCanvasBounds(model,hasReferences);
  return Number.isSafeInteger(width)&&Number.isSafeInteger(height)&&width>=limits.width.min&&height>=limits.height.min&&width<=limits.width.max&&height<=limits.height.max&&width%limits.width.step===0&&height%limits.height.step===0&&width*height<=limits.pixels;
}
function configureGenerationDimension(input,axis,model,hasReferences=false){
  const limits=generationCanvasBounds(model,hasReferences)[axis];input.min=limits.min;input.step=limits.step;if(Number.isFinite(limits.max))input.max=limits.max;else input.removeAttribute('max');
}
function syncGenerationModel(){
  if(window.__LOCAL_IMAGE_REACT__)return;
  const model=generationModel(),previous=$('gen-variant').value;$('gen-variant').replaceChildren();
  for(const variant of model?.variants||[]){const option=document.createElement('option');option.value=variant.id;option.textContent=variant.label;$('gen-variant').append(option);}
  $('gen-variant').value=model?.variants?.some(item=>item.id===previous)?previous:model?.defaults?.variant||model?.variants?.[0]?.id||'int8';
  if(generationSamplingModel!==model?.id){$('gen-guidance').value=String(model?.defaults?.guidance||1);$('gen-steps').value=String(model?.defaults?.steps||25);const dimensions=generationPresetDimensions($('gen-aspect').value);if(dimensions){$('gen-width').value=String(dimensions[0]);$('gen-height').value=String(dimensions[1]);}generationSamplingModel=model?.id;}
  const limits=model?.limits||{};for(const axis of ['width','height'])configureGenerationDimension($('gen-'+axis),axis,model,generationReferences.length>0);
  $('gen-steps').min=limits.min_steps||1;$('gen-steps').max=limits.max_steps||100;$('gen-guidance').min=limits.min_guidance||1;$('gen-guidance').max=limits.max_guidance||1;
  $('gen-size-note').textContent=limits.resolution_note||'';
  renderGenerationReferences();updateGenerationControls();
}
function updateGenerationControls(){
  if(window.__LOCAL_IMAGE_REACT__){window.LocalImageGenerationBridge?.publish();return;}
  const model=generationModel(),cap=model?.capabilities||{},variant=generationVariant(),available=!!model?.available&&variant?.available!==false;
  const refs=!!(cap.image_reference||cap.image_to_image||cap.references),limit=cap.max_references||0,tooMany=refs&&generationReferences.length>limit;
  $('gen-model').disabled=busy||generationLoading;$('gen-variant').disabled=busy;$('gen-variant').hidden=(model?.variants?.length||0)<=1;$('gen-variant-label').hidden=$('gen-variant').hidden;
  $('gen-model-state').textContent=generationLoading?'Checking installed models…':available?(model?.label||'Model')+' ready on this PC':variant?.reason||model?.reason||'Select an installed local model.';
  $('gen-model-state').classList.toggle('error',!generationLoading&&!!model&&!available);
  $('gen-setup-options').hidden=available||!!model?.historical;
  $('gen-reference-tab').disabled=!refs;$('gen-reference-tab').hidden=!refs;if(!refs&&generationStudioTab==='reference')selectGenerationTab('prompt');$('gen-reference-title').textContent=cap.image_reference?'Reference images':'Starting image';
  $('gen-reference-note').textContent=generationMissingReferences>generationReferences.length?'This result used '+generationMissingReferences+' reference image'+(generationMissingReferences===1?'':'s')+'. Reattach the original references to reproduce that setup ('+generationReferences.length+' attached).':!refs?'This model uses text prompts only.':cap.image_reference?'Use up to '+limit+' images to guide subjects, style and composition.':'Use one starting image. Strength controls how much the model changes it; semantic reference guidance is unavailable.';
  $('gen-use-current').disabled=busy||!session||!refs||generationReferences.length>=limit;$('gen-add-reference').disabled=busy||!refs||generationReferences.length>=limit;
  $('gen-denoise-options').hidden=!cap.denoise||!generationReferences.length;
  $('gen-transparent-row').hidden=!cap.transparent;$('gen-transparent-note').hidden=!cap.transparent;$('gen-transparent').disabled=busy||!cap.transparent;
  $('gen-negative-options').hidden=!cap.negative_prompt;$('gen-negative').disabled=busy||!cap.negative_prompt||Number($('gen-guidance').value)<=1;
  $('gen-negative-note').textContent=Number($('gen-guidance').value)<=1?'Set guidance above 1 in Output → Sampling to use a negative prompt.':'Negative conditioning is active with guidance above 1.';
  $('gen-guidance-row').hidden=(model?.limits?.max_guidance||1)<=1;
  $('gen-sampling-note').textContent='Recommended for this model: '+(model?.recommended?.steps||model?.defaults?.steps||25)+' steps.';
  $('gen-sampling-options').hidden=(model?.limits?.max_guidance||1)<=1;
  for(const id of ['gen-prompt','gen-guidance','gen-steps','gen-denoise','gen-aspect','gen-width','gen-height','gen-seed','gen-random-seed'])$(id).disabled=busy;
  const width=Number($('gen-width').value),height=Number($('gen-height').value),limits=model?.limits||{},step=generationCanvasBounds(model,generationReferences.length>0).width.step;
  const sizeValid=generationCanvasValid(width,height,model,generationReferences.length>0);
  const steps=Number($('gen-steps').value),guidance=Number($('gen-guidance').value),samplingValid=Number.isInteger(steps)&&steps>=(limits.min_steps||1)&&steps<=(limits.max_steps||100)&&guidance>=(limits.min_guidance||1)&&guidance<=(limits.max_guidance||1);
  const missingLoras=generationLoras.some(item=>item.missing),referenceLoraMissingImage=generationLoras.some(item=>item.usage==='reference-edit')&&(!refs||!generationReferences.length);
  $('gen-lora-note').textContent=referenceLoraMissingImage?'This adapter requires an image. Add one in References before generating.':'Optional adapters for a particular style or subject. Up to three per image.';$('gen-lora-note').classList.toggle('error',referenceLoraMissingImage);
  $('gen-run').disabled=busy||generationLoading||!available||!$('gen-prompt').value.trim()||tooMany||!sizeValid||!samplingValid||missingLoras||referenceLoraMissingImage;
  $('gen-run').textContent=activeTask==='generate'?'Generating…':'Generate image';
  $('gen-run').title=missingLoras?'Remove unavailable adapters or install their exact files':referenceLoraMissingImage?'This adapter requires a reference image. Add one in References.':tooMany?'Remove extra reference images for this model':!sizeValid?'Choose dimensions within the model limits, in multiples of '+step:!samplingValid?'Choose sampling values within the selected model limits':!available?'Install or connect the selected model':'Generate as a new editable document';
  $('generation-summary').textContent=(model?.label||'New image')+' · '+width+' × '+height+(refs&&generationReferences.length?' · '+generationReferences.length+' reference'+(generationReferences.length===1?'':'s'):'');
  const result=!!(session?.generation||session?.upscale);for(const id of ['generated-retouch','generated-cutout'])$(id).disabled=busy||!result;
  $('generated-background').disabled=busy||!result||!generationTargetSession||generationTargetSession===session?.id;
  const downloadable=variant?.downloadable!==false&&model?.downloadable!==false;$('gen-download').disabled=busy||!nativeSetup||!!generationDownloadJob?.running||!downloadable;$('gen-download').textContent=downloadable?'Download selected model':'Publisher access required';$('gen-download-note').textContent=(!downloadable&&(variant?.download_note||model?.download_note))||generationDownloadJob?.message||(nativeSetup?'Install the selected model in ComfyUI.':'Model downloads are available in the desktop app.');$('model-browser').disabled=busy||generationLoading;
  $('gen-lora-options').hidden=cap.lora===false||cap.loras===false;$('lora-library').disabled=busy||!model;$('gen-lora-count').textContent=String(generationLoras.length);
  for(const id of ['draft-refine-open','generated-library-open','generated-library-menu'])$(id).disabled=busy||generationLoading;
  for(const input of document.querySelectorAll('#gen-selected-loras button,#gen-selected-loras input'))input.disabled=busy;
  window.LocalImageAssetsBridge?.publish();
}
function renderGenerationReferences(){
  if(window.__LOCAL_IMAGE_REACT__)return;
  const holder=$('gen-references');holder.replaceChildren();
  for(const [index,reference]of generationReferences.entries()){const row=document.createElement('div');row.className='generation-reference';const image=document.createElement('img');image.alt='';image.src=reference.thumbnail;const label=document.createElement('span');label.textContent=reference.name;label.title=reference.name;const remove=document.createElement('button');remove.setAttribute('aria-label','Remove reference '+reference.name);remove.title='Remove reference';remove.textContent='×';remove.disabled=busy;remove.onclick=()=>{generationReferences.splice(index,1);renderGenerationReferences();updateGenerationControls();};row.append(image,label,remove);holder.append(row);}
}
function addGenerationReference(data){if(generationReferences.some(item=>item.id===data.id))return;generationReferences.push({id:data.id,name:data.name,thumbnail:'/api/local-remove/session/'+encodeURIComponent(data.id)+'/preview?revision='+data.revision});renderGenerationReferences();updateGenerationControls();}
async function restoreGenerationSettings(saved){
  if(window.LocalImageReactFeatures?.generation&&session){window.LocalImageReactFeatures.generation.restoreDocument('edit',{...session,generation:saved});return;}
  if(saved.model&&!generationModels.some(model=>model.id===saved.model)){generationModels.push(historicalGenerationModel(saved));generationModelId=saved.model;renderGenerationModelOptions();}
  if(generationModels.some(model=>model.id===saved.model)){generationModelId=saved.model;$('gen-model').value=saved.model;syncGenerationModel();}
  if(generationModel()?.variants?.some(variant=>variant.id===saved.variant))$('gen-variant').value=saved.variant;
  const defaults={prompt:'',negative_prompt:'',width:1024,height:1024,steps:25,guidance:1,...generationModel()?.defaults,seed:''};
  for(const [id,key]of [['gen-prompt','prompt'],['gen-negative','negative_prompt'],['gen-width','width'],['gen-height','height'],['gen-steps','steps'],['gen-guidance','guidance'],['gen-seed','seed']])$(id).value=String(saved[key]??defaults[key]);
  $('gen-transparent').checked=!!saved.transparent;$('gen-aspect').value='custom';if(saved.denoise!==undefined&&saved.denoise!==null){$('gen-denoise').value=String(Math.round(saved.denoise*100));$('gen-denoise-value').textContent=$('gen-denoise').value+'%';}
  generationReferences=[];generationMissingReferences=Number(saved.reference_count)||0;renderGenerationReferences();
  generationLoras=[];loraInventory=null;
  if(saved.loras?.length){try{loraInventory=await(await api('/api/local-remove/loras?model='+encodeURIComponent(generationModelId))).json();}catch{}
    generationLoras=saved.loras.map(item=>{const installed=loraInventory?.installed?.find(entry=>entry.id===item.id);return{id:item.id,strength:item.strength??1,title:installed?.title||'Unavailable adapter '+item.id,usage:installed?.usage,trigger_phrase:installed?.trigger_phrase,missing:!installed};});
  }
  renderSelectedLoras();$('gen-result-note').textContent=generationLoras.some(item=>item.missing)?'This project uses adapters unavailable in this library. Remove or reinstall them to generate again.':generationMissingReferences?'Saved settings restored. Reattach '+generationMissingReferences+' reference image'+(generationMissingReferences===1?'':'s')+' in References to recreate the setup.':'Saved generation settings restored.';updateGenerationControls();
}
async function generateImage(){
  if(window.LocalImageReactFeatures?.generation)return window.LocalImageReactFeatures.generation.run(window.LocalImageReactFeatures.generation.activeDraftKey());
  if($('gen-run').disabled)return;const model=generationModel(),cap=model.capabilities||{},seed=$('gen-seed').value.trim();
  const payload={model:model.id,variant:$('gen-variant').value,prompt:$('gen-prompt').value.trim(),width:Number($('gen-width').value),height:Number($('gen-height').value),transparent:!!cap.transparent&&$('gen-transparent').checked,reference_session_ids:(cap.image_reference||cap.image_to_image||cap.references)?generationReferences.map(item=>item.id):[],loras:(cap.lora===false||cap.loras===false)?[]:generationLoras.map(item=>({id:item.id,strength:item.strength})),...(seed?{seed:Number(seed)}:{})};
  payload.steps=Number($('gen-steps').value);
  if((model.limits?.max_guidance||1)>1)payload.guidance=Number($('gen-guidance').value)||1;
  if(cap.negative_prompt&&(payload.guidance||1)>1)payload.negative_prompt=$('gen-negative').value.trim();
  if(cap.denoise&&generationReferences.length)payload.denoise=Number($('gen-denoise').value)/100;
  activeTask='generate';setBusy(true);message('Generating with '+model.label+'…');$('progress-label').textContent='Generating with '+model.label+'…';
  const started=Date.now(),timer=setInterval(()=>{const fallback='Generating with '+model.label+' · '+Math.round((Date.now()-started)/1000)+'s';$('progress-label').textContent=window.localImageProgressText?.(fallback)||fallback;},1000);
  try{const result=await json('/api/local-remove/generation',payload);generationResultId=result.session.id;await openSession(result.session);workspace='generate';updateToolChrome();controls();renderHealth();$('gen-result-note').textContent='Created '+result.width+' × '+result.height+' · Seed '+result.seed+(result.library_warning?' · '+result.library_warning:'');message(result.library_warning?'Image created. '+result.library_warning:'Image created. Retouch, cut out, or export your result.',!!result.library_warning);}
  catch(error){message(error.message,true);$('gen-result-note').textContent=error.message;}finally{clearInterval(timer);activeTask=null;setBusy(false);}
}
$('workspace-generate').onclick=()=>setWorkspace('generate');
function chooseGenerationModel(id,variant){
  if(busy||!generationModels.some(item=>item.id===id))return;
  generationModelId=id;$('gen-model').value=id;generationMissingReferences=0;generationLoras=[];loraInventory=null;
  renderSelectedLoras();syncGenerationModel();
  if(generationModel()?.variants?.some(item=>item.id===variant))$('gen-variant').value=variant;
  updateGenerationControls();renderHealth();
}
$('gen-model').onchange=()=>chooseGenerationModel($('gen-model').value);$('gen-variant').onchange=()=>{updateGenerationControls();renderHealth();};
$('gen-prompt').oninput=updateGenerationControls;$('gen-guidance').oninput=updateGenerationControls;$('gen-steps').oninput=updateGenerationControls;
$('gen-use-current').onclick=()=>{if(session)addGenerationReference(session);};$('gen-add-reference').onclick=()=>$('generation-reference-file').click();
$('generation-reference-file').onchange=async()=>{if(busy)return;setBusy(true);try{const max=generationModel()?.capabilities?.max_references||0;for(const file of Array.from($('generation-reference-file').files||[]).slice(0,Math.max(0,max-generationReferences.length))){const form=new FormData();form.append('file',file);addGenerationReference(await(await api('/api/local-remove/import',{method:'POST',body:form})).json());}}catch(error){message(error.message,true);}finally{$('generation-reference-file').value='';setBusy(false);}};
$('gen-denoise').oninput=()=>{$('gen-denoise-value').textContent=$('gen-denoise').value+'%';};
function generationPresetDimensions(shape){const presets={'1:1':[1024,1024],'3:2':[1536,1024],'2:3':[1024,1536],'16:9':[1536,864]},base=presets[shape],model=generationModel();if(!base)return null;const area=(model?.defaults?.width||1024)*(model?.defaults?.height||1024),limits=generationWorkflowLimits(model,generationReferences.length>0),ratio=Number(shape.split(':')[0])/Number(shape.split(':')[1]);const width=area<=1048576?base[0]:Math.sqrt(area*ratio),height=area<=1048576?base[1]:width/ratio;if(window.LocalImageGenerationSize){const fitted=window.LocalImageGenerationSize.fitDimensions({width,height,ratio,locked:true},limits);return[fitted.width,fitted.height];}return[Math.round(width),Math.round(height)];}
$('gen-aspect').onchange=()=>{const dimensions=generationPresetDimensions($('gen-aspect').value);if(dimensions){$('gen-width').value=String(dimensions[0]);$('gen-height').value=String(dimensions[1]);}updateGenerationControls();};
for(const id of ['gen-width','gen-height'])$(id).oninput=()=>{$('gen-aspect').value='custom';updateGenerationControls();};
$('gen-random-seed').onclick=()=>{$('gen-seed').value='';};$('gen-run').onclick=generateImage;
$('generated-retouch').onclick=()=>setWorkspace('retouch');$('generated-cutout').onclick=()=>setWorkspace('cutout');
$('generated-background').onclick=async()=>{if(!generationTargetSession||!(session?.generation||session?.upscale)||busy)return;const generated=session.id;try{const target=await(await api('/api/local-remove/session/'+encodeURIComponent(generationTargetSession))).json();await openSession(target);setWorkspace('cutout');selectStudioTab('background');await cutoutEdit('/cutout/generated-background',{generated_session_id:generated},'POST','Applying generated background…');}catch(error){message(error.message,true);}};
$('gen-download').onclick=async()=>{if(!nativeSetup||busy)return;try{await nativeRequest('setupDownloadGenerationModel',null,{model:generationModelId,variant:$('gen-variant').value});await refreshGenerationDownload();}catch(error){message(error.message,true);}};
async function refreshGenerationDownload(){if(window.__LOCAL_IMAGE_REACT__)return;clearTimeout(generationDownloadTimer);generationDownloadTimer=null;try{const job=await(await api('/api/local-remove/generator/download')).json();generationDownloadJob=job;$('gen-download-progress').hidden=!job.running;$('gen-download-progress').value=job.progress||0;updateGenerationControls();if($('model-browser-dialog')?.open)renderModelBrowser();if($('refine-dialog').open)updateRefineUpscaleControls();if(job.running)generationDownloadTimer=setTimeout(refreshGenerationDownload,2000);else if(job.phase==='complete'){await loadGenerationModels(true);if(job.model==='seedvr2'){refineUpscaleStatus=await(await api('/api/local-remove/generation/upscale/models')).json();updateRefineControls();}}}catch(error){$('gen-download-note').textContent=error.message;}}

const browserModel=()=>generationModels.find(model=>model.id===modelBrowserId);
function modelBrowserStatus(text,error=false){$('model-browser-status').textContent=text;$('model-browser-status').classList.toggle('error',error);}
function selectBrowserModel(id){modelBrowserId=id;$('model-browser-variant').replaceChildren();const model=browserModel();for(const variant of model?.variants||[]){const option=document.createElement('option');option.value=variant.id;option.textContent=variant.label;$('model-browser-variant').append(option);}$('model-browser-variant').value=model.id===generationModelId?$('gen-variant').value:model.defaults?.variant||model.variants?.[0]?.id;renderModelBrowser();}
function renderModelBrowser(){
  const model=browserModel();if(!model)return;const variant=model.variants?.find(item=>item.id===$('model-browser-variant').value),download=modelBrowserDownloads?.models?.find(item=>item.id===model.id)?.variants?.find(item=>item.id===variant?.id),cap=model.capabilities||{};
  for(const button of $('model-browser-list').querySelectorAll('button'))button.setAttribute('aria-current',String(button.dataset.model===model.id));
  $('model-browser-name').textContent=model.label;$('model-browser-description').textContent=model.description||model.benefit||'';
  $('model-browser-capabilities').textContent=['Text to image',cap.image_reference?'Up to '+cap.max_references+' reference images':cap.image_to_image?'Starting-image variation':null,cap.transparent?'Transparent PNG':'Opaque images'].filter(Boolean).join(' · ');
  $('model-browser-strengths').replaceChildren();for(const strength of model.strengths||model.notes||[]){const li=document.createElement('li');li.textContent=strength;$('model-browser-strengths').append(li);}
  const total=variant?.total_bytes??download?.total_bytes??model.storage_bytes,missing=variant?.missing_bytes??download?.missing_bytes,installed=missing===0||download?.installed===true||variant?.available===true;
  $('model-browser-state').textContent=variant?.available?'Ready in ComfyUI':installed?'Files installed · '+(variant?.reason||model.reason||'Connect ComfyUI to use them'):variant?.reason||model.reason||'Download required';
  $('model-browser-size').textContent=total?setupBytes(total)+' total'+(missing===0?' · already installed':missing!==undefined?' · '+setupBytes(missing)+' remaining':''):'Checking download size…';
  const hardwareId=model.id==='qwen'?'qwen-'+variant?.id:model.id,hardware=modelBrowserHardware?.profiles?.find(item=>item.id===hardwareId),hardwareInfo=variant?.hardware||model.hardware;
  $('model-browser-memory').textContent=hardwareInfo?.vram_recommendation||hardware?.vram||'See Help → Hardware guide';$('model-browser-memory-note').textContent=hardwareInfo?.basis||hardware?.detail||'Memory recommendations are planning estimates. CPU offloading can lower VRAM use and runs more slowly.';
  $('model-browser-steps').textContent=String(model.recommended?.steps||model.defaults?.steps||'—');$('model-browser-limitations').textContent=(model.limitations||[]).join(' · ');
  const license=model.license||{};$('model-browser-license').hidden=!license.url;$('model-browser-license').href=license.url||'#';$('model-browser-license').textContent=license.label?license.label+' · License details ↗':'License details ↗';
  const locked=modelBrowserRequest||!!generationDownloadJob?.running||busy,downloadable=variant?.downloadable!==false&&model.downloadable!==false;$('model-browser-download').disabled=!nativeSetup||locked||installed||!downloadable;$('model-browser-download').textContent=installed?'Model files installed':downloadable?'Download model':'Publisher access required';$('model-browser-use').disabled=locked;$('model-browser-folder').disabled=!nativeSetup||locked;$('model-browser-variant').disabled=locked;$('model-browser-refresh').disabled=locked;
  $('model-browser-path').textContent=setupState?.model_directory||modelBrowserDownloads?.model_directory||'Choose where local model files are stored';$('model-browser-path').title=$('model-browser-path').textContent;
  $('model-browser-progress').hidden=!generationDownloadJob?.running;$('model-browser-progress').value=generationDownloadJob?.progress||0;
  const folderConnection=setupState?.model_folder_connection||modelFolderConnection,downloadMessage=generationDownloadJob?.phase!=='idle'?generationDownloadJob?.message:null,accessNote=!installed&&(variant?.download_note||model.download_note);modelBrowserStatus(folderConnection&&folderConnection.status!=='unchanged'?folderConnection.message+(accessNote?' '+accessNote:''):accessNote||downloadMessage||(!nativeSetup?'Open the desktop app to choose folders and download models.':'Existing verified files are reused. Downloading does not change your current model.'),generationDownloadJob?.phase==='error');
}
async function openModelBrowser(){
  if(window.LocalImageReactFeatures?.generation){if(busy||modalOpen())return;closeMenus();resetTransientInput();return window.LocalImageReactFeatures.generation.openModels(window.LocalImageReactFeatures.generation.activeDraftKey());}
  if(window.LocalImageReactFeatures?.models){if(busy||modalOpen())return;closeMenus();resetTransientInput();return window.LocalImageReactFeatures.models.openModels(window.LocalImageModelBridge.legacyModelSelection());}
  if(busy||modalOpen())return;closeMenus();$('model-browser-dialog').showModal();modelBrowserId=generationModel()?.historical?generationModels.find(model=>!model.historical)?.id:generationModelId;modelBrowserStatus('Loading model details…');
  const checks=await Promise.allSettled([loadGenerationModels(),api('/api/local-remove/generator/download').then(response=>response.json()),api('/api/local-remove/hardware').then(response=>response.json()),api('/api/local-remove/setup').then(response=>response.json())]);
  if(checks[1].status==='fulfilled'){modelBrowserDownloads=checks[1].value;generationDownloadJob=checks[1].value;}if(checks[2].status==='fulfilled')modelBrowserHardware=checks[2].value;if(checks[3].status==='fulfilled')setupState=checks[3].value;
  const list=$('model-browser-list');list.replaceChildren();for(const model of generationModels.filter(item=>!item.historical)){const button=document.createElement('button');button.dataset.model=model.id;const label=document.createElement('span');label.textContent=model.label;const note=document.createElement('small');note.textContent=model.available?'Ready':'Setup needed';button.append(label,note);button.onclick=()=>selectBrowserModel(model.id);button.addEventListener('keydown',event=>{const buttons=[...list.querySelectorAll('button')],index=buttons.indexOf(button);let next;if(event.key==='ArrowDown')next=(index+1)%buttons.length;else if(event.key==='ArrowUp')next=(index+buttons.length-1)%buttons.length;else if(event.key==='Home')next=0;else if(event.key==='End')next=buttons.length-1;else return;event.preventDefault();buttons[next].focus();selectBrowserModel(buttons[next].dataset.model);});list.append(button);}
  if(!browserModel())modelBrowserId=generationModels[0]?.id;if(browserModel())selectBrowserModel(modelBrowserId);else modelBrowserStatus('Model details are unavailable. Close and reopen to retry.',true);
}
$('model-browser').onclick=openModelBrowser;$('model-browser-close').onclick=()=>$('model-browser-dialog').close();$('model-browser-variant').onchange=renderModelBrowser;
const modelRefreshButton=document.createElement('button');modelRefreshButton.id='model-browser-refresh';modelRefreshButton.className='secondary';modelRefreshButton.textContent='Refresh';modelRefreshButton.title='Recheck model files and the ComfyUI connection';$('model-browser-folder').before(modelRefreshButton);modelRefreshButton.onclick=async()=>{if(modelBrowserRequest||busy)return;modelBrowserRequest=true;renderModelBrowser();modelBrowserStatus('Rechecking models and ComfyUI…');let failure;try{await loadGenerationModels(true);if(browserModel())selectBrowserModel(modelBrowserId);}catch(error){failure=error.message;}finally{modelBrowserRequest=false;renderModelBrowser();modelBrowserStatus(failure?'Could not refresh model status: '+failure:'Model status refreshed from ComfyUI.',!!failure);}};
$('model-browser-use').onclick=()=>{const precision=$('model-browser-variant').value;$('gen-model').value=modelBrowserId;$('gen-model').onchange();$('gen-variant').value=precision;updateGenerationControls();renderHealth();$('model-browser-dialog').close();};
$('model-browser-download').onclick=async()=>{if($('model-browser-download').disabled)return;modelBrowserRequest=true;renderModelBrowser();let failure;try{await nativeRequest('setupDownloadGenerationModel',null,{model:modelBrowserId,variant:$('model-browser-variant').value});await refreshGenerationDownload();}catch(error){failure=error.message;}finally{modelBrowserRequest=false;renderModelBrowser();if(failure)modelBrowserStatus(failure,true);}};
$('model-browser-folder').onclick=async()=>{if($('model-browser-folder').disabled)return;modelBrowserRequest=true;renderModelBrowser();let failure;try{const result=await nativeRequest('setupChooseModelDirectory');if(result){setupState=result;if(result.model_folder_connection)modelFolderConnection=result.model_folder_connection;await loadGenerationModels(true);}}catch(error){failure=error.message;}finally{modelBrowserRequest=false;renderModelBrowser();if(failure)modelBrowserStatus(failure,true);}};
$('model-browser-dialog').addEventListener('close',()=>{$('model-browser').focus();updateGenerationControls();});

function stockStatus(text,error=false){if(window.__LOCAL_IMAGE_REACT__)return;$('stock-status').textContent=text;$('stock-status').classList.toggle('error',error);}
function safeSourceUrl(value){try{const url=new URL(value);return['http:','https:'].includes(url.protocol)?url.href:null;}catch{return null;}}
function setCreditLink(element,address,label){const href=safeSourceUrl(address);element.hidden=!href;if(href)element.href=href;else element.removeAttribute('href');element.textContent=label;}
const selectedStock=()=>stockResults.find(item=>item.id===stockSelectedId);
function updateStockControls(){
  if(window.__LOCAL_IMAGE_REACT__){window.LocalImageAssetsBridge?.publish();return;}
  const locked=busy||stockLoading||stockImporting,selected=!!selectedStock(),cap=generationModel()?.capabilities||{},canReference=workspace==='generate'&&(cap.image_reference||cap.image_to_image||cap.references)&&generationReferences.length<(cap.max_references||0);
  $('stock-search').disabled=locked||!$('stock-query').value.trim()||!stockProviders.some(item=>item.id===$('stock-provider').value&&item.available!==false);$('stock-query').disabled=locked;$('stock-provider').disabled=locked;
  $('stock-close').disabled=stockImporting;$('stock-previous').disabled=locked||stockPage===0;$('stock-next').disabled=locked||stockNextPage===null||stockNextPage===undefined;
  $('stock-import-image').disabled=locked||!selected;$('stock-import-background').disabled=locked||!selected||!session;$('stock-import-background').title=session?'Place this image behind the current subject':'Open an image before choosing its background';
  $('stock-import-reference').hidden=workspace!=='generate';$('stock-import-reference').disabled=locked||!selected||!canReference;$('stock-import-reference').title=canReference?'Add as an image input to the selected model':'The selected model already has its maximum image inputs';
  for(const [id,origin]of [['stock-import-image','image'],['stock-import-background','background'],['stock-import-reference','reference']]){$(id).classList.toggle('primary',stockOrigin===origin);$(id).classList.toggle('secondary',stockOrigin!==origin);}
  $('stock-results').setAttribute('aria-busy',String(stockLoading));for(const button of $('stock-results').querySelectorAll('button'))button.disabled=locked;
}
function selectStock(id){
  if(window.__LOCAL_IMAGE_REACT__)return window.LocalImageReactFeatures?.assets?.selectStock(id);
  stockSelectedId=id;const item=selectedStock();for(const button of $('stock-results').querySelectorAll('button')){const selected=button.dataset.stockId===id;button.setAttribute('aria-pressed',String(selected));button.tabIndex=selected?0:-1;}
  $('stock-preview').hidden=!item;if(item){$('stock-preview').src=item.thumbnail_url;$('stock-preview').alt=item.title||'Selected stock image';}
  $('stock-image-title').textContent=item?.title||'Select an image';$('stock-image-size').textContent=item?.width&&item?.height?item.width+' × '+item.height+' px':'';$('stock-creator').textContent=item?.creator?'By '+item.creator:'';
  setCreditLink($('stock-license'),item?.license_url,item?.license||'License not provided');$('stock-license').hidden=!item;setCreditLink($('stock-source'),item?.source_url,'View original source ↗');$('stock-attribution').textContent=item?.attribution||'';$('stock-license-note').hidden=!item;updateStockControls();
}
function renderStockResults(){
  if(window.__LOCAL_IMAGE_REACT__)return;
  const holder=$('stock-results');holder.replaceChildren();for(const item of stockResults){const button=document.createElement('button');button.className='stock-result';button.dataset.stockId=item.id;button.setAttribute('aria-label',item.title||'Stock image');button.title=item.title||'Stock image';const image=document.createElement('img');image.src=item.thumbnail_url;image.alt='';image.loading='lazy';image.decoding='async';const label=document.createElement('span');label.textContent=item.title||'Untitled image';button.append(image,label);button.onclick=()=>selectStock(item.id);button.addEventListener('keydown',event=>{const buttons=[...holder.querySelectorAll('button')],index=buttons.indexOf(button),columns=Math.max(1,getComputedStyle(holder).gridTemplateColumns.split(' ').length);let next;if(event.key==='ArrowRight')next=Math.min(index+1,buttons.length-1);else if(event.key==='ArrowLeft')next=Math.max(0,index-1);else if(event.key==='ArrowDown')next=Math.min(index+columns,buttons.length-1);else if(event.key==='ArrowUp')next=Math.max(0,index-columns);else if(event.key==='Home')next=0;else if(event.key==='End')next=buttons.length-1;else return;event.preventDefault();selectStock(buttons[next].dataset.stockId);buttons[next].focus();});holder.append(button);}
  $('stock-empty').hidden=!!stockResults.length;$('stock-page').textContent=stockResults.length?'Page '+(stockPage+1):'';selectStock(stockResults[0]?.id||null);
}
async function searchStock(page=0){
  if(window.__LOCAL_IMAGE_REACT__)return window.LocalImageReactFeatures?.assets?.search(page);
  if(stockLoading||stockImporting||!$('stock-query').value.trim())return;stockLoading=true;updateStockControls();stockStatus('Searching '+($('stock-provider').selectedOptions[0]?.textContent||'stock images')+'…');
  try{const data=await(await api('/api/local-remove/stock/search?provider='+encodeURIComponent($('stock-provider').value)+'&query='+encodeURIComponent($('stock-query').value.trim())+'&page='+page)).json();stockResults=data.results||[];stockPage=page;stockNextPage=data.next_page;renderStockResults();if(!stockResults.length)$('stock-empty').textContent='No images found. Try a different search or provider.';stockStatus((stockResults.length?stockResults.length+' images · checked just now':'No results')+(data.warning?' · '+data.warning:''));}
  catch(error){stockStatus('Search failed: '+error.message+' Try Search again or choose another provider.',true);}finally{stockLoading=false;updateStockControls();}
}
async function openStockLibrary(origin='image',opener=$('stock-open')){
  if(window.__LOCAL_IMAGE_REACT__){if(busy||modalOpen())return;closeMenus();resetTransientInput();return window.LocalImageReactFeatures?.assets?.open('stock');}
  if(busy||modalOpen())return;stockOrigin=origin;stockOpener=opener;closeMenus();resetTransientInput();$('stock-dialog').showModal();stockLoading=true;updateStockControls();
  try{const data=await(await api('/api/local-remove/stock/providers')).json();stockProviders=data.providers||[];const previous=$('stock-provider').value;$('stock-provider').replaceChildren();for(const provider of stockProviders){const option=document.createElement('option');option.value=provider.id;option.textContent=provider.label;option.disabled=false;option.title=provider.reason||'';$('stock-provider').append(option);}$('stock-provider').value=stockProviders.some(item=>item.id===previous)?previous:data.default_provider||stockProviders.find(item=>item.available!==false)?.id;$('stock-provider').hidden=stockProviders.length<=1;$('stock-title').textContent=stockProviders.length===1?'Stock images · '+stockProviders[0].label:'Stock library';stockStatus(stockProviders.some(item=>item.available!==false)?'Online search · Review source and license before importing.':'Stock providers are unavailable. Close and reopen the library to retry.',!stockProviders.some(item=>item.available!==false));}
  catch(error){stockStatus('Could not load stock providers: '+error.message,true);}finally{stockLoading=false;updateStockControls();$('stock-query').focus();}
}
async function importStock(target){
  if(window.__LOCAL_IMAGE_REACT__)return window.LocalImageReactFeatures?.assets?.importStock(target);
  const item=selectedStock();if(!item||busy||stockLoading||stockImporting)return;stockImporting=true;setBusy(true);updateStockControls();stockStatus('Importing '+item.title+'…');
  try{await flushLayerChanges();const result=await json('/api/local-remove/stock/import',{id:item.id,target:target==='background'?'background':'image',...(target==='background'?{session_id:session.id,revision:session.revision,layer_id:window.LocalImageLayers?.selected?.()?.id||null}:{})});$('stock-dialog').close();
    if(target==='reference'){addGenerationReference(result.session);selectGenerationTab('reference');message('Stock image added as a reference. Source credit stays with the image.');}
    else{await openSession(result.session);setWorkspace(target==='background'?'cutout':'retouch');if(target==='background')selectStudioTab('background');message(target==='background'?'Stock background applied. Its credit is available in File → Image credits.':'Stock image opened. Its credit is available in File → Image credits.');}
  }catch(error){stockStatus('Import failed: '+error.message,true);}finally{stockImporting=false;setBusy(false);updateStockControls();}
}
function documentCredits(){
  const credits=[];
  if(session?.source_attribution)credits.push({label:'Source image',...session.source_attribution});
  if(session?.cutout?.background?.attribution)credits.push({label:'Background',...session.cutout.background.attribution});
  for(const [index,entry]of (session?.reference_attributions||[]).entries())credits.push({label:'Reference '+(index+1),...entry});
  for(const [index,entry]of (session?.cutout?.background?.reference_attributions||[]).entries())credits.push({label:'Background reference '+(index+1),...entry});
  for(const layer of session?.layer_stack||[]){
    if(layer.discarded||!layer.visible)continue;
    if(layer.attribution)credits.push({label:'Layer · '+layer.name,...layer.attribution});
    for(const [index,entry]of (layer.reference_attributions||[]).entries())credits.push({label:layer.name+' reference '+(index+1),...entry});
  }
  const seen=new Set();return credits.filter(credit=>{const key=JSON.stringify([credit.provider,credit.asset_id,credit.source_url]);if(seen.has(key))return false;seen.add(key);return true;});
}
function creditHandoffText(){return documentCredits().map(credit=>[credit.label+(credit.title?' · '+credit.title:''),credit.attribution||[credit.title,credit.creator,credit.license].filter(Boolean).join(' · '),safeSourceUrl(credit.source_url),safeSourceUrl(credit.license_url)].filter(Boolean).join('\n')).join('\n\n');}
$('credits-copy').onclick=async()=>{try{await navigator.clipboard.writeText(creditHandoffText());$('credits-status').textContent='Credits copied. Paste them with the shared image.';}catch{$('credits-status').textContent='Clipboard unavailable. Save a credits text file instead.';}};
$('credits-download').onclick=()=>{if(!session)return;const link=document.createElement('a');link.href='/api/local-remove/session/'+encodeURIComponent(session.id)+'/download-credits';link.download=(session.name||'image').replace(/\.[^.]+$/,'')+'-credits.txt';document.body.append(link);link.click();link.remove();$('credits-status').textContent='Credits text file prepared for sharing alongside the image.';};
async function showImageCredits(){if(window.LocalImageGenerationBridge){const target=await window.LocalImageGenerationBridge.prepareDocumentCommand('credits');if(!target.allowed)return;}if(busy||modalOpen())return;closeMenus();$('credits-list').replaceChildren();for(const credit of documentCredits()){const section=document.createElement('section');section.className='credits-item';const heading=document.createElement('h3');heading.textContent=credit.label+(credit.title?' · '+credit.title:'');const text=document.createElement('p');text.textContent=credit.attribution||[credit.title,credit.creator,credit.license].filter(Boolean).join(' · ');const source=document.createElement('a'),license=document.createElement('a');for(const link of [source,license]){link.target='_blank';link.rel='noopener noreferrer';}setCreditLink(source,credit.source_url,'Original source ↗');setCreditLink(license,credit.license_url,credit.license||'License details ↗');section.append(heading,text,source,license);$('credits-list').append(section);}$('credits-dialog').showModal();}
$('stock-open').onclick=()=>openStockLibrary('image',$('stock-open'));$('background-stock').onclick=()=>openStockLibrary('background',$('background-stock'));$('gen-stock').onclick=()=>openStockLibrary('reference',$('gen-stock'));
$('stock-close').onclick=()=>$('stock-dialog').close();$('stock-dialog').addEventListener('cancel',event=>{if(stockImporting)event.preventDefault();});$('stock-dialog').addEventListener('close',()=>{stockOpener?.focus();});$('stock-search-form').onsubmit=event=>{event.preventDefault();searchStock(0);};$('stock-query').oninput=updateStockControls;$('stock-provider').onchange=()=>{stockResults=[];stockSelectedId=null;stockPage=0;stockNextPage=null;renderStockResults();if($('stock-query').value.trim()&&stockProviders.some(item=>item.id===$('stock-provider').value&&item.available!==false))searchStock(0);updateStockControls();};$('stock-previous').onclick=()=>searchStock(Math.max(0,stockPage-1));$('stock-next').onclick=()=>searchStock(stockNextPage);
$('stock-import-image').onclick=()=>importStock('image');$('stock-import-background').onclick=()=>importStock('background');$('stock-import-reference').onclick=()=>importStock('reference');$('image-credits').onclick=showImageCredits;$('credits-close').onclick=()=>$('credits-dialog').close();$('credits-dialog').addEventListener('close',()=>viewport.focus({preventScroll:true}));

const contextLoras=()=>loraContext==='generate'?generationLoras:refineLoras[loraContext];
const contextModelId=()=>loraContext==='generate'?generationModelId:$('refine-'+loraContext+'-model').value;
const contextModel=()=>generationModels.find(model=>model.id===contextModelId());
function loraScope(kind){return{epoch:loraDialogEpoch,context:loraContext,model:contextModelId(),kind,version:++loraVersions[kind]};}
function validLoraScope(scope){return $('lora-dialog')?.open&&scope.epoch===loraDialogEpoch&&scope.context===loraContext&&scope.model===contextModelId()&&scope.version===loraVersions[scope.kind];}
function updateLoraContext(){if(loraContext==='generate')updateGenerationControls();else updateRefineControls();}
function renderLoraSelection(holder,items,update){
  holder.replaceChildren();
  for(const [index,lora]of items.entries()){const row=document.createElement('div');row.className='selected-lora';const name=document.createElement('span');name.textContent=(lora.missing?'Unavailable · ':'')+(lora.title||lora.id);name.title=name.textContent;const strength=document.createElement('input');strength.type='number';strength.min='-2';strength.max='2';strength.step='.05';strength.value=String(lora.strength);strength.setAttribute('aria-label','Strength for '+name.textContent);strength.onchange=()=>{const value=Number(strength.value);lora.strength=Number.isFinite(value)?Math.max(-2,Math.min(2,value)):0;strength.value=String(lora.strength);update();};const remove=document.createElement('button');remove.textContent='×';remove.title='Remove adapter';remove.setAttribute('aria-label','Remove '+name.textContent);remove.onclick=()=>{items.splice(index,1);renderLoraSelection(holder,items,update);update();};row.append(name,strength,remove);holder.append(row);}
}
function renderSelectedLoras(){if(window.__LOCAL_IMAGE_REACT__)return;renderLoraSelection($('gen-selected-loras'),generationLoras,()=>{renderSelectedLoras();updateGenerationControls();});$('gen-lora-count').textContent=String(generationLoras.length);}
function renderContextLoras(){if(loraContext==='generate')renderSelectedLoras();else renderRefineLoras(loraContext);}
function loraStatus(text,error=false){$('lora-library-status').textContent=text;$('lora-library-status').classList.toggle('error',error);}
function addLoraTrigger(phrase){if(!phrase)return;const input=$(loraContext==='generate'?'gen-prompt':'refine-'+loraContext+'-prompt'),prompt=input.value.trim();input.value=(prompt?prompt+'\n':'')+phrase;updateLoraContext();loraStatus('Trigger added to this stage’s editable prompt. Review it before generating.');}
function applyLoraSettings(settings){if(!settings)return;const prefix=loraContext==='generate'?'gen-':'refine-'+loraContext+'-';if(settings.steps!==undefined)$(prefix+'steps').value=String(settings.steps);if(settings.guidance!==undefined)$(prefix+'guidance').value=String(settings.guidance);updateLoraContext();loraStatus('Recommended sampling values applied to this stage. Select the adapter from Installed before generating.');}
function loraUsageText(item){return[item.experimental?'Experimental':null,item.style,item.usage==='reference-edit'?'Requires a reference image':item.usage==='text-to-image'?'Text to image':item.usage==='both'?'Text or reference images':null,item.license,item.license_note].filter(Boolean).join(' · ');}
function appendLoraDetails(holder,item,{actions=false}={}){
  for(const text of [item.description,loraUsageText(item),item.trigger_phrase?'Trigger: '+item.trigger_phrase:null]){if(!text)continue;const note=document.createElement('small');note.textContent=text;note.className='lora-description';holder.append(note);}
  if(actions&&(item.trigger_phrase||item.recommended_settings)){const bar=document.createElement('div');bar.className='lora-inline-actions';if(item.trigger_phrase){const button=document.createElement('button');button.textContent='Add trigger';button.onclick=()=>addLoraTrigger(item.trigger_phrase);bar.append(button);}if(item.recommended_settings){const button=document.createElement('button');button.textContent='Apply settings';button.onclick=()=>applyLoraSettings(item.recommended_settings);bar.append(button);}holder.append(bar);}
}
function safeLoraPreview(value){try{const url=new URL(value,location.href);return url.origin===location.origin&&url.pathname.startsWith('/api/local-remove/')?url.href:null;}catch{return null;}}
function showLoraInfo(item,opener){
  let panel=$('lora-info-panel');if(!panel){panel=document.createElement('section');panel.id='lora-info-panel';panel.className='lora-info-panel';panel.setAttribute('aria-label','Adapter information');$('lora-dialog').insertBefore(panel,$('lora-dialog').querySelector('.lora-library-footer'));}panel.hidden=false;panel.replaceChildren();const heading=document.createElement('div');heading.className='section-title';const name=document.createElement('h3');name.textContent=item.title||item.filename||item.repo_id;const close=document.createElement('button');close.textContent='×';close.setAttribute('aria-label','Close adapter information');close.onclick=()=>{panel.hidden=true;const label=opener?.getAttribute('aria-label'),replacement=Array.from($('lora-dialog').querySelectorAll('.lora-info-button')).find(button=>button.getAttribute('aria-label')===label);(opener?.isConnected?opener:replacement||$('lora-close')).focus({preventScroll:true});};panel.onkeydown=event=>{if(event.key==='Escape'){event.preventDefault();event.stopPropagation();close.click();}};heading.append(name,close);panel.append(heading);appendLoraDetails(panel,item,{actions:true});const compatibility=document.createElement('p');compatibility.className='cutout-note';compatibility.textContent=(item.warning||(item.compatibility==='curated'?'Recommended for this exact model':item.compatibility==='declared'?'Publisher declares compatibility':'Compatibility unverified'))+(item.bytes?' · '+setupBytes(item.bytes):'');panel.append(compatibility);if(item.repo_id){const link=document.createElement('a');link.textContent='Publisher and license details ↗';link.href='https://huggingface.co/'+item.repo_id.split('/').map(encodeURIComponent).join('/');link.target='_blank';link.rel='noopener noreferrer';panel.append(link);}close.focus();panel.scrollIntoView({block:'nearest'});
  const source=document.createElement('p');source.className='cutout-note';source.textContent=item.preview_available?(item.example_source==='local-test'?'Local test example. ':'Publisher example; not independently tested. ')+(item.example_caption||''):'No image example is available for this adapter.';panel.append(source);
}
function loraExampleTile(item,action,onUse){
  const tile=document.createElement('article');tile.className='lora-example-tile';const preview=document.createElement('div');preview.className='lora-example-preview';const url=safeLoraPreview(item.preview_url);if(url&&item.preview_available!==false){const image=document.createElement('img');image.src=url;image.alt=(item.title||item.style||'Adapter')+' example';image.loading='lazy';image.onerror=()=>{image.remove();preview.textContent='Example unavailable';};preview.append(image);}else preview.textContent='No example yet';const caption=document.createElement('small');caption.className='lora-example-source';caption.textContent=url?(item.example_source==='local-test'?'Local test':'Publisher example'):'Preview unavailable';const title=document.createElement('strong');title.textContent=item.title||item.style||item.repo_id||item.filename;title.title=title.textContent;const actions=document.createElement('div');actions.className='lora-example-actions';const use=document.createElement('button');use.className='secondary lora-use-action';use.textContent=action;use.onclick=onUse;const info=document.createElement('button');info.className='lora-info-button';info.textContent='i';info.setAttribute('aria-label','Information about '+title.textContent);info.title='Adapter details, compatibility and license';info.onclick=()=>showLoraInfo(item,info);actions.append(use,info);tile.append(preview,caption,title,actions);return tile;
}
function renderInstalledLoras(){
  const list=$('lora-installed-list');list.replaceChildren();const installed=loraInventory?.installed||[];
  if(!installed.length){const empty=document.createElement('p');empty.className='cutout-note';empty.textContent='No adapters installed for this model. Browse online to find more.';list.append(empty);}
  list.classList.add('lora-gallery');for(const item of installed){const selected=contextLoras().some(lora=>lora.id===item.id),tile=loraExampleTile(item,selected?'Added':'Use',()=>{if(contextLoras().length>=3)return;contextLoras().push({id:item.id,title:item.title||item.filename,strength:Number.isFinite(item.recommended_strength)?Math.max(-2,Math.min(2,item.recommended_strength)):1,usage:item.usage,trigger_phrase:item.trigger_phrase});renderContextLoras();renderInstalledLoras();updateLoraContext();});tile.querySelector('.lora-use-action').disabled=selected||contextLoras().length>=3||item.supported===false;list.append(tile);}
}
async function loadLoraInventory(){
  const scope=loraScope('inventory');try{const inventory=await(await api('/api/local-remove/loras?model='+encodeURIComponent(scope.model))).json();if(!validLoraScope(scope))return;loraInventory=inventory;for(const lora of contextLoras()){const installed=loraInventory.installed?.find(item=>item.id===lora.id);lora.missing=!installed||installed.supported===false;if(installed){lora.title=installed.title||installed.filename;lora.usage=installed.usage;lora.trigger_phrase=installed.trigger_phrase;}}renderContextLoras();updateLoraContext();renderInstalledLoras();const job=loraInventory.job;if(job?.running){loraStatus(job.message||'Downloading adapter…');pollLoraDownload();}else loraStatus('Adapters are matched to '+(contextModel()?.label||contextModelId())+' · '+(loraContext==='generate'?'Image Gen':loraContext==='draft'?'Draft stage':'Refinement stage')+'. Select up to three.');}
  catch(error){if(validLoraScope(scope))loraStatus(error.message+' Use Browse → Refresh to retry.',true);}finally{if(validLoraScope(scope))$('lora-browse-tab').disabled=false;}
}
async function openLoraLibrary(context='generate'){
  if(window.LocalImageReactFeatures?.generation){if(busy||modalOpen())return;closeMenus();return window.LocalImageReactFeatures.generation.openLoras(context==='generate'?window.LocalImageReactFeatures.generation.activeDraftKey():context);}
  if(window.LocalImageReactFeatures?.models){if(busy||window.LocalImageReactFeatures.models.isOpen()||(modalOpen()&&!(context!=='generate'&&$('refine-dialog')?.open)))return;closeMenus();resetTransientInput();return window.LocalImageReactFeatures.models.openLoras(window.LocalImageModelBridge.getLegacyLoraPort(context));}
  if(busy||$('lora-dialog').open||(modalOpen()&&!(context!=='generate'&&$('refine-dialog').open)))return;loraDialogEpoch++;loraContext=context;loraLibraryBusy=false;loraInventory=null;loraFiles=null;loraFilesModelId=null;clearTimeout(loraDownloadTimer);closeMenus();$('lora-dialog').showModal();$('lora-model-label').textContent=(context==='generate'?'Image Gen':context==='draft'?'Draft stage':'Refinement stage')+' · '+(contextModel()?.label||contextModelId());$('lora-file-detail').hidden=true;if($('lora-info-panel'))$('lora-info-panel').hidden=true;$('lora-search').disabled=false;$('lora-refresh').disabled=false;$('lora-browse-tab').disabled=true;$('lora-installed-list').replaceChildren();$('lora-search-results').replaceChildren();loraStatus('Loading styles for '+(contextModel()?.label||contextModelId())+'…');selectLoraTab('installed');await loadLoraInventory();
}
function selectLoraTab(tab){
  loraLibraryTab=tab;if($('lora-info-panel'))$('lora-info-panel').hidden=true;
  for(const value of ['installed','browse']){const active=tab===value;$('lora-'+value+'-tab').setAttribute('aria-selected',String(active));$('lora-'+value+'-tab').tabIndex=active?0:-1;$('lora-'+value+'-panel').hidden=!active;}
  if(tab==='browse')searchLoras();
}
function renderLoraBrowse(results,query=''){
  const list=$('lora-search-results');list.replaceChildren();list.classList.add('lora-gallery');
  for(const category of ['curated','declared','unverified']){const items=results.filter(item=>(['curated','declared'].includes(item.compatibility)?item.compatibility:'unverified')===category);if(!items.length)continue;const heading=document.createElement('h3');heading.textContent=category==='curated'?'Recommended for this model':category==='declared'?'Publisher declares compatibility':'Community · compatibility unverified';list.append(heading);for(const item of items){const tile=loraExampleTile(item,item.supported===false?'Unsupported':'Download…',()=>loadLoraFiles(item.repo_id,item.filename,item.revision));tile.querySelector('.lora-use-action').disabled=item.supported===false;list.append(tile);}}
  if(!results.length){const empty=document.createElement('p');empty.className='cutout-note';empty.textContent=query?'No matching local styles. Clear the search to show recommended examples.':'No recommended examples are available for this model yet.';list.append(empty);if(query){const clear=document.createElement('button');clear.className='secondary';clear.textContent='Show recommended';clear.onclick=()=>{$('lora-query').value='';++loraVersions.search;loraLibraryBusy=false;searchLoras();};list.append(clear);}}
}
async function searchLoras(){
  if(loraLibraryBusy)return;loraLibraryBusy=true;$('lora-search').disabled=true;$('lora-refresh').disabled=true;loraStatus('Looking for current adapters on Hugging Face…');
  const scope=loraScope('search'),query=$('lora-query').value.trim(),needle=query.toLowerCase(),recommended=(loraInventory?.curated||[]).filter(item=>!needle||[item.title,item.style,item.description,item.repo_id].filter(Boolean).join(' ').toLowerCase().includes(needle));renderLoraBrowse(recommended,query);
  try{const data=await(await api('/api/local-remove/loras/search?model='+encodeURIComponent(scope.model)+'&query='+encodeURIComponent(query))).json();if(!validLoraScope(scope))return;const seen=new Set(recommended.map(item=>item.repo_id)),results=[...recommended,...(data.results||[]).filter(item=>!seen.has(item.repo_id))];renderLoraBrowse(results,query);
    $('lora-last-checked').textContent='Checked '+new Date().toLocaleTimeString([],{hour:'2-digit',minute:'2-digit'});loraStatus(results.length?'Choose a repository to inspect its files and compatibility.':'No matching adapters found. Try a different search.');
  }catch(error){if(validLoraScope(scope)){$('lora-last-checked').textContent='Offline · Local examples';loraStatus('Online search failed: '+error.message+' Recommended examples and installed adapters remain available. Use Refresh to retry.',true);}}
  finally{if(validLoraScope(scope)){loraLibraryBusy=false;$('lora-search').disabled=false;$('lora-refresh').disabled=false;}}
}
async function loadLoraFiles(repo,preferredFilename,revision){
  const scope=loraScope('files');loraStatus('Reading adapter files…');loraFiles=null;loraFilesModelId=null;$('lora-file-detail').hidden=true;$('lora-download').disabled=true;if($('lora-info-panel'))$('lora-info-panel').hidden=true;
  try{const files=await(await api('/api/local-remove/loras/files?model='+encodeURIComponent(scope.model)+'&repo_id='+encodeURIComponent(repo)+(revision?'&revision='+encodeURIComponent(revision):''))).json();if(!validLoraScope(scope))return;loraFiles=files;loraFilesModelId=scope.model;$('lora-file-detail').hidden=false;$('lora-repo-title').textContent=repo;$('lora-repo-link').href='https://huggingface.co/'+repo.split('/').map(encodeURIComponent).join('/');$('lora-file').replaceChildren();
    for(const file of loraFiles.files||[]){const option=document.createElement('option');option.value=file.filename;option.textContent=file.filename+' · '+setupBytes(file.bytes);$('lora-file').append(option);}
    if(preferredFilename&&loraFiles.files?.some(file=>file.filename===preferredFilename))$('lora-file').value=preferredFilename;
    $('lora-unverified').checked=false;$('lora-unverified-row').hidden=loraFiles.compatibility==='curated';$('lora-unverified-label').textContent=(loraFiles.compatibility==='declared'?'Accept publisher-declared compatibility and assign to ':'Assign this unverified adapter to ')+(contextModel()?.label||contextModelId());$('lora-file-detail').scrollIntoView({block:'nearest'});$('lora-file').focus();
    $('lora-compatibility-note').textContent=loraFiles.warning||(loraFiles.compatibility==='curated'?'Recommended for this model. Downloading does not automatically enable the adapter.':loraFiles.compatibility==='declared'?'The publisher names this base model. This adapter has not been reviewed; check the model page before using it.':'Community metadata does not confirm compatibility. Check the model page before assigning this adapter.');updateLoraDownloadButton();loraStatus('Select the adapter file to download.');
  }catch(error){if(validLoraScope(scope))loraStatus(error.message,true);}
}
function selectedLoraFile(){return loraFiles?.files?.find(file=>file.filename===$('lora-file').value);}
function selectedLoraCompatibility(){return selectedLoraFile()?.compatibility||loraFiles?.compatibility||'unverified';}
function updateLoraDownloadButton(){
  const file=selectedLoraFile(),compatibility=selectedLoraCompatibility();
  $('lora-unverified-row').hidden=compatibility==='curated';
  $('lora-unverified-label').textContent=(compatibility==='declared'?'Accept publisher-declared compatibility and assign to ':'Assign this unverified adapter to ')+(contextModel()?.label||contextModelId());
  $('lora-compatibility-note').textContent=file?.warning||loraFiles?.warning||(compatibility==='curated'?'This exact adapter file is recommended for this model. Downloading does not automatically enable it.':compatibility==='declared'?'The publisher names this base model. This file has not been reviewed; check the model page before using it.':'Community metadata does not confirm compatibility. Check the model page before assigning this file.');
  $('lora-apply-settings').hidden=!file?.recommended_settings;
  $('lora-file-description').textContent=file?.description||'';$('lora-file-usage').textContent=file?loraUsageText(file):'';$('lora-file-trigger').textContent=file?.trigger_phrase?'Trigger: '+file.trigger_phrase:'';$('lora-add-trigger').hidden=!file?.trigger_phrase;
  const blocked=!nativeSetup?'LoRA downloads need the desktop app. This browser preview can browse and use installed adapters. Open the desktop preview to download.'
    :loraFilesModelId!==contextModelId()||!file?'Select an adapter file for the current model.'
    :loraFiles?.supported===false||file.supported===false?(file.warning||loraFiles.warning||'This adapter is not supported by the selected workflow.')
    :loraInventory?.job?.running?'An adapter download is already running.'
    :compatibility!=='curated'&&!$('lora-unverified').checked?'Review the compatibility information and check the assignment box above before downloading.':'';
  $('lora-download').disabled=!!blocked;$('lora-download').title=blocked||'Download into your configured models folder';
  let note=$('lora-download-availability');if(!note){note=document.createElement('p');note.id='lora-download-availability';note.className='cutout-note';note.setAttribute('role','status');$('lora-download').parentElement.after(note);}
  note.textContent=blocked;note.hidden=!blocked;
}
async function downloadLora(){
  if($('lora-download').disabled||loraFilesModelId!==contextModelId())return;const scope=loraScope('files'),payload={model:scope.model,repo_id:loraFiles.repo_id,filename:$('lora-file').value,revision:loraFiles.revision,allow_unverified:selectedLoraCompatibility()!=='curated'&&$('lora-unverified').checked};$('lora-download').disabled=true;if(loraInventory)loraInventory.job={running:true,model:scope.model};
  try{await nativeRequest('loraDownload',null,payload);if(validLoraScope(scope))await pollLoraDownload();}catch(error){if(validLoraScope(scope)){if(loraInventory)loraInventory.job={running:false,model:scope.model};updateLoraDownloadButton();loraStatus(error.message,true);}}
}
async function pollLoraDownload(){
  clearTimeout(loraDownloadTimer);loraDownloadTimer=null;if(!$('lora-dialog')?.open)return;const scope=loraScope('poll');
  try{const job=await(await api('/api/local-remove/loras/download')).json();if(!validLoraScope(scope))return;if(loraInventory)loraInventory.job=job;$('lora-download-progress').hidden=!job.running;$('lora-download-progress').value=job.progress||0;loraStatus((job.model&&job.model!==scope.model?'Other model download · ':'')+(job.error||job.message||'Adapter download ready'),job.phase==='error');updateLoraDownloadButton();if(job.running)loraDownloadTimer=setTimeout(pollLoraDownload,2000);else if(job.phase==='complete')await loadLoraInventory();}catch(error){if(validLoraScope(scope))loraStatus(error.message,true);}
}
$('lora-library').onclick=()=>openLoraLibrary();$('lora-close').onclick=()=>$('lora-dialog').close();
$('lora-installed-tab').onclick=()=>selectLoraTab('installed');$('lora-browse-tab').onclick=()=>selectLoraTab('browse');
for(const tab of ['installed','browse'])$('lora-'+tab+'-tab').addEventListener('keydown',event=>{if(!['ArrowLeft','ArrowRight','Home','End'].includes(event.key))return;event.preventDefault();const next=event.key==='Home'?'installed':event.key==='End'?'browse':tab==='installed'?'browse':'installed';selectLoraTab(next);$('lora-'+next+'-tab').focus();});
$('lora-search').onclick=searchLoras;$('lora-refresh').onclick=searchLoras;$('lora-query').addEventListener('keydown',event=>{if(event.key==='Enter'){event.preventDefault();searchLoras();}});
$('lora-file').onchange=()=>{$('lora-unverified').checked=false;updateLoraDownloadButton();};$('lora-unverified').onchange=updateLoraDownloadButton;$('lora-download').onclick=downloadLora;
$('lora-apply-settings').onclick=()=>applyLoraSettings(selectedLoraFile()?.recommended_settings);$('lora-add-trigger').onclick=()=>addLoraTrigger(selectedLoraFile()?.trigger_phrase);
$('lora-dialog').addEventListener('close',()=>{++loraDialogEpoch;loraLibraryBusy=false;loraFilesModelId=null;clearTimeout(loraDownloadTimer);loraDownloadTimer=null;$(loraContext==='generate'?'lora-library':'refine-'+loraContext+'-lora-library').focus();updateLoraContext();});

function renderHealth(){
  if(workspace==='generate'){const ready=!!generationModel()?.available&&generationVariant()?.available!==false;$('status').textContent=ready?(generationModel()?.label||'Image Gen')+' ready · On this PC':'Image Gen · Choose an available model';$('status').classList.toggle('ready',ready);return;}
  if(workspace==='cutout'||operation==='ai'&&aiProvider==='qwen'){$('status').textContent=qwenReady()?'Qwen Image 2.1 ready · On this PC':'Qwen Image 2.1 · Setup needed';$('status').classList.toggle('ready',qwenReady());return;}
  $('model-indicator').textContent=operation==='heal'?'Local retouch':modelLabel();$('model-shortcut').title=operation==='heal'?'Quick Heal information · Settings':'Removal model: '+modelLabel()+' · Settings';
  $('status').textContent=operation==='heal'?(operationReady()?'Quick Heal ready · On this PC':'Quick Heal unavailable'):(ready?(currentModel()?.available===false?modelLabel()+' unavailable':'AI Remove ready · On this PC'):'AI Remove · Connection needed');
  $('status').classList.toggle('ready',operationReady());
}
async function health(){
  if(window.LocalImageReactFeatures?.settings){await window.LocalImageReactFeatures.settings.reloadConfiguration();renderHealth();controls();return;}
  try{const data=await(await api('/api/local-remove/status')).json();ready=data.ready;retouchReady=data.retouch_ready===true;$('status').title=data.device||'';renderHealth();}
  catch{ready=false;retouchReady=false;$('status').textContent='Local backend offline';$('status').classList.remove('ready');}
  if(!settingsLoaded){try{await loadSettings();}catch{if(operation!=='heal')$('status').textContent='Connecting to model settings…';}}
  try{qwenStatus=await(await api('/api/local-remove/qwen/status')).json();}catch{qwenStatus=null;}
  renderHealth();
  controls();
}
const HARDWARE_GUIDE_SEEN='local-image.hardware-guide.v1';
const FIRST_SETUP_SEEN='local-image.first-ai-setup.v1';
function openFirstSetup(){
  if(window.LocalImageReactFeatures?.settings){window.LocalImageReactFeatures.settings.maybeFirstRun();return;}
  if(!firstSetupPending||busy||modalOpen()||!nativeSetup)return;
  firstSetupPending=false;try{localStorage.setItem(FIRST_SETUP_SEEN,'1');}catch{}
  setupCandidatesOpen=setupState?.setup_mode==='discover';
  $('ai-portable-options').open=setupState?.setup_mode==='portable';
  $('settings').click();
}
async function showHardwareGuide(){
  if(window.LocalImageReactFeatures?.settings){if(busy||modalOpen())return;closeMenus();resetTransientInput();return window.LocalImageReactFeatures.settings.open('hardware');}
  if(busy||modalOpen())return;closeMenus();resetTransientInput();$('hardware-dialog').showModal();
  try{const data=await(await api('/api/local-remove/hardware')).json();
    $('hardware-device').textContent=data.devices?.length?data.devices.map(device=>device.name+(device.vram_gb?' · '+device.vram_gb+' GB VRAM':'')).join(' / '):'GPU memory could not be detected';
    $('hardware-ram').textContent=data.system_ram_gb?data.system_ram_gb+' GB system RAM':'';
    $('hardware-profiles').replaceChildren();$('hardware-sources').replaceChildren();
    for(const profile of data.profiles||[]){const row=document.createElement('tr'),name=document.createElement('td'),memory=document.createElement('td');name.textContent=profile.label;memory.textContent=profile.vram;row.append(name,memory);$('hardware-profiles').append(row);const note=document.createElement('p');note.className='cutout-note';note.textContent=profile.label+' — '+(profile.detail||profile.basis||'');if(profile.source_url){const link=document.createElement('a');link.href=profile.source_url;link.target='_blank';link.rel='noopener noreferrer';link.textContent=' Source ↗';note.append(link);}$('hardware-sources').append(note);}
    $('hardware-note').textContent=data.note;
  }catch(error){$('hardware-device').textContent='GPU memory could not be detected';$('hardware-note').textContent='You can use the editing tools while hardware detection is unavailable. Reopen this guide from Help after connecting the backend.';}
}
$('hardware-guide').onclick=showHardwareGuide;$('hardware-close').onclick=()=>$('hardware-dialog').close();
$('hardware-continue').onclick=()=>{try{localStorage.setItem(HARDWARE_GUIDE_SEEN,'1');}catch{}$('hardware-dialog').close();};
function showKeyboardGuide(){closeMenus();if(window.LocalImageReactFeatures?.settings)return window.LocalImageReactFeatures.settings.open('shortcuts');$('shortcuts-dialog').showModal();}
$('keyboard-guide').onclick=showKeyboardGuide;$('shortcuts-close').onclick=()=>$('shortcuts-dialog').close();
for(const id of ['hardware-dialog','shortcuts-dialog'])$(id).addEventListener('close',()=>{viewport.focus({preventScroll:true});controls();if(id==='hardware-dialog')openFirstSetup();});
async function init(){
  if(window.__LOCAL_IMAGE_REACT__&&!window.LocalImageReactFeatures?.started){window.LocalImageInitializeEditor=init;return;}
  const initialQuery=new URLSearchParams(location.search),initialNavigationEpoch=documentNavigationEpoch;
  loadOverwritePreference();
  try{const stored=localStorage.getItem('local-remove-operation');if(['heal','ai'].includes(stored))operation=stored;}catch{}
  try{const stored=localStorage.getItem('local-remove-qwen-variant');if(['int8','bf16'].includes(stored))qwenVariant=stored;}catch{}
  $('qwen-variant').value=qwenVariant;$('retouch-qwen-variant').value=qwenVariant;
  updateToolChrome();
  try{const stored=localStorage.getItem('local-remove-copy-format');if(['original','png','jpg','tif','webp'].includes(stored)){outputFormat=stored;$('output-format').value=stored;}}catch{}
  try{const stored=localStorage.getItem('local-remove-heal-method');if(['texture','telea'].includes(stored)){healMethod=stored;$('heal-method').value=stored;}}catch{}
  await connectNative();
  await health();
  if(nativeSetup){try{await loadSetup();const mode=setupState?.setup_mode;firstSetupPending=['discover','portable'].includes(mode)&&setupState?.service?.ready!==true&&localStorage.getItem(FIRST_SETUP_SEEN)!=='1';}catch{}}
  try{await recent();}catch(error){message(error.message,true);}
  const sid=initialQuery.get('session'),collectionId=initialQuery.get('collection');
  if(documentNavigationEpoch===initialNavigationEpoch&&!session&&!collection){
    if(collectionId){try{const data=await(await api('/api/local-remove/collection/'+encodeURIComponent(collectionId))).json();if(documentNavigationEpoch===initialNavigationEpoch&&!session&&!collection)await openCollection(data,Number(initialQuery.get('index'))||0);}catch(error){message(error.message,true);}}
    else if(sid){try{const data=await(await api('/api/local-remove/session/'+encodeURIComponent(sid))).json();await openSession(data,{expectedNavigationEpoch:initialNavigationEpoch});}catch(error){message(error.message,true);}}
  }
  controls();setInterval(health,15000);
  if(window.LocalImageReactFeatures?.settings)await window.LocalImageReactFeatures.settings.maybeFirstRun();
  else{try{if(localStorage.getItem(HARDWARE_GUIDE_SEEN)!=='1')await showHardwareGuide();}catch{}openFirstSetup();}
  initCompleted=true;
}
// The two-stage workspace owns its settings so experimentation never overwrites
// the single-image generator's draft or the document behind this window.
const refineModel=stage=>generationModels.find(model=>model.id===$('refine-'+stage+'-model').value);
const refineSelectedDraft=()=>refineDrafts.find(item=>item.session.id===refineDraftId);
const refineSelectedResult=()=>refineResults.find(item=>item.session.id===refineResultId);
const sessionPreview=data=>'/api/local-remove/session/'+encodeURIComponent(data.id)+'/preview?revision='+data.revision;
function renderRefineLoras(stage){if(window.__LOCAL_IMAGE_REACT__)return;renderLoraSelection($('refine-'+stage+'-selected-loras'),refineLoras[stage],()=>{renderRefineLoras(stage);updateRefineControls();});$('refine-'+stage+'-lora-count').textContent=String(refineLoras[stage].length);}
// Keep corresponding image locations together while preserving real pixel scale.
function applyRefineComparison(){
  if(window.__LOCAL_IMAGE_REACT__)return;
  for(const side of ['draft','result']){const image=$('refine-'+side+'-image'),area=image.parentElement;if(image.hidden||!image.naturalWidth)continue;const scale=refineCompare.scale==='fit'?Math.min(area.clientWidth/image.naturalWidth,area.clientHeight/image.naturalHeight):refineCompare.scale;image.style.width=image.naturalWidth*scale+'px';image.style.height=image.naturalHeight*scale+'px';image.style.left=(area.clientWidth/2-refineCompare.x*image.naturalWidth*scale)+'px';image.style.top=(area.clientHeight/2-refineCompare.y*image.naturalHeight*scale)+'px';}
  $('refine-compare-zoom').textContent=refineCompare.scale==='fit'?'Fit':Math.round(refineCompare.scale*100)+'%';
}
function zoomRefineComparison(factor){
  if(refineCompare.scale==='fit'){const image=$('refine-draft-image');refineCompare.scale=image.naturalWidth?Math.min(image.parentElement.clientWidth/image.naturalWidth,image.parentElement.clientHeight/image.naturalHeight):1;}
  refineCompare.scale=Math.max(.02,Math.min(8,refineCompare.scale*factor));applyRefineComparison();
}
if(!window.__LOCAL_IMAGE_REACT__)for(const side of ['draft','result']){const image=$('refine-'+side+'-image'),area=image.parentElement;image.draggable=false;image.onload=applyRefineComparison;area.tabIndex=0;area.setAttribute('aria-label',(side==='draft'?'Draft':'Refined result')+' comparison. Drag to pan, use plus or minus to zoom.');area.addEventListener('pointerdown',event=>{if(event.button!==0||image.hidden)return;const scale=refineCompare.scale==='fit'?Math.min(area.clientWidth/image.naturalWidth,area.clientHeight/image.naturalHeight):refineCompare.scale;refineCompare.drag={id:event.pointerId,startX:event.clientX,startY:event.clientY,x:refineCompare.x,y:refineCompare.y,width:image.naturalWidth*scale,height:image.naturalHeight*scale};area.setPointerCapture(event.pointerId);event.preventDefault();});area.addEventListener('pointermove',event=>{const drag=refineCompare.drag;if(!drag||drag.id!==event.pointerId)return;refineCompare.x=Math.max(0,Math.min(1,drag.x-(event.clientX-drag.startX)/drag.width));refineCompare.y=Math.max(0,Math.min(1,drag.y-(event.clientY-drag.startY)/drag.height));applyRefineComparison();});const stop=()=>{refineCompare.drag=null;};area.addEventListener('pointerup',stop);area.addEventListener('pointercancel',stop);area.addEventListener('wheel',event=>{if(image.hidden)return;event.preventDefault();zoomRefineComparison(event.deltaY<0?1.2:1/1.2);},{passive:false});area.addEventListener('keydown',event=>{if(['+','=','-','1','f','F','ArrowLeft','ArrowRight','ArrowUp','ArrowDown'].includes(event.key)){event.preventDefault();if(event.key==='1')refineCompare.scale=1;else if(event.key.toLowerCase()==='f'){refineCompare.scale='fit';refineCompare.x=refineCompare.y=.5;}else if(['+','=','-'].includes(event.key))zoomRefineComparison(event.key==='-'?1/1.25:1.25);else{const key=event.key;refineCompare.x=Math.max(0,Math.min(1,refineCompare.x+(key==='ArrowLeft'?-.05:key==='ArrowRight'?.05:0)));refineCompare.y=Math.max(0,Math.min(1,refineCompare.y+(key==='ArrowUp'?-.05:key==='ArrowDown'?.05:0)));}applyRefineComparison();}});new ResizeObserver(applyRefineComparison).observe(area);}
refineCompare.scale='fit';
$('refine-compare-fit').onclick=()=>{refineCompare.scale='fit';refineCompare.x=refineCompare.y=.5;applyRefineComparison();};$('refine-compare-actual').onclick=()=>{refineCompare.scale=1;applyRefineComparison();};$('refine-compare-plus').onclick=()=>zoomRefineComparison(1.25);$('refine-compare-minus').onclick=()=>zoomRefineComparison(1/1.25);
$('refine-compare-larger').onclick=()=>{const value=$('refine-dialog').dataset.compareOnly!=='true';$('refine-dialog').dataset.compareOnly=String(value);$('refine-compare-larger').setAttribute('aria-pressed',String(value));$('refine-compare-larger').textContent=value?'Back to settings':'Compare larger';requestAnimationFrame(applyRefineComparison);};
const REFINE_RECIPES_KEY='local-image.refinement-recipes.v1';
function readRefineRecipes(){try{const recipes=JSON.parse(localStorage.getItem(REFINE_RECIPES_KEY)||'[]');return Array.isArray(recipes)?recipes.filter(item=>item&&item.schema===1&&typeof item.name==='string'&&item.stages?.draft&&item.stages?.final).slice(0,40):[];}catch{return[];}}
function renderRefineRecipes(){const select=$('refine-recipe'),previous=select.value;select.replaceChildren(new Option('Choose a recipe',''));for(const recipe of readRefineRecipes())select.append(new Option(recipe.name,recipe.name));select.value=previous;}
function markRefineRecipeEdited(){if(refineRecipeLoading||!$('refine-recipe').value)return;$('refine-recipe-note').textContent='Current settings differ from the selected recipe. Save the recipe to keep these changes.';$('refine-recipe-note').classList.remove('error');}
function captureRefineRecipe(name){const stages={};for(const stage of ['draft','final']){const settings={};for(const key of ['model','variant','prompt','width','height','steps','guidance','seed'])settings[key]=$('refine-'+stage+'-'+key).value;settings.transparent=$('refine-'+stage+'-transparent').checked;settings.loras=refineLoras[stage].map(item=>({id:item.id,title:item.title,strength:item.strength}));stages[stage]=settings;}return{schema:1,name,stages,aspect:$('refine-aspect').value,negative:$('refine-negative').value,denoise:$('refine-denoise').value,includeReferences:$('refine-include-references').checked,upscale:{enabled:$('refine-upscale').checked,preset:$('refine-upscale-preset').value,width:$('refine-upscale-width').value,height:$('refine-upscale-height').value}};}
function saveRefineRecipe(){const name=$('refine-recipe-name').value.trim();if(!name){$('refine-recipe-note').textContent='Enter a name for this recipe.';$('refine-recipe-name').focus();return;}const recipes=readRefineRecipes(),index=recipes.findIndex(item=>item.name===name);if(index<0&&recipes.length>=40){$('refine-recipe-note').textContent='You have 40 recipes. Delete a recipe before saving another.';return;}const recipe=captureRefineRecipe(name);if(index>=0)recipes[index]=recipe;else recipes.push(recipe);try{localStorage.setItem(REFINE_RECIPES_KEY,JSON.stringify(recipes));renderRefineRecipes();$('refine-recipe').value=name;$('refine-recipe-note').textContent=(index>=0?'Updated ':'Saved ')+name+'. Both stages, styles and output settings are included.';}catch{$('refine-recipe-note').textContent='Recipe storage is unavailable. Your current settings remain open.';}}
async function loadRefineRecipe(){
  const recipe=readRefineRecipes().find(item=>item.name===$('refine-recipe').value);if(!recipe)return;if(refineRecipeLoading||refineJob||busy){$('refine-recipe-note').textContent='Wait for the current image operation, then load this recipe.';return;}refineRecipeWarnings=[];refineRecipeLoading=true;updateRefineControls();try{
  for(const stage of ['draft','final']){const settings=recipe.stages[stage],select=$('refine-'+stage+'-model');if(!Array.from(select.options).some(option=>option.value===settings.model))select.append(new Option('Unavailable · '+settings.model,settings.model));select.value=settings.model;syncRefineModel(stage);const model=refineModel(stage);if(!model?.available)refineRecipeWarnings.push((stage==='draft'?'Draft':'Refinement')+' model unavailable: '+settings.model);const variant=$('refine-'+stage+'-variant');if(!Array.from(variant.options).some(option=>option.value===settings.variant))variant.append(new Option('Unavailable · '+settings.variant,settings.variant));for(const key of ['variant','prompt','width','height','steps','guidance','seed'])$('refine-'+stage+'-'+key).value=String(settings[key]??'');if(!model?.variants?.find(item=>item.id===settings.variant)?.available)refineRecipeWarnings.push('Precision unavailable: '+settings.model+' / '+settings.variant);$('refine-'+stage+'-transparent').checked=!!settings.transparent&&!!model?.capabilities?.transparent;if(settings.transparent&&!model?.capabilities?.transparent)refineRecipeWarnings.push(settings.model+' does not support transparent output.');refineLoras[stage]=Array.isArray(settings.loras)?settings.loras.slice(0,3).map(item=>({...item,strength:Number(item.strength),missing:true})):[];
    if(model&&(model.capabilities?.lora===false||model.capabilities?.loras===false)&&refineLoras[stage].length){refineRecipeWarnings.push(settings.model+' does not support style adapters; those saved styles were cleared.');refineLoras[stage]=[];}
    if(refineLoras[stage].length){try{const inventory=await(await api('/api/local-remove/loras?model='+encodeURIComponent(settings.model))).json();for(const item of refineLoras[stage]){const installed=inventory.installed?.find(value=>value.id===item.id&&value.supported!==false);item.missing=!installed||!Number.isFinite(item.strength)||item.strength<-2||item.strength>2;if(installed){item.title=installed.title;item.usage=installed.usage;}if(item.missing)refineRecipeWarnings.push('Adapter unavailable or invalid: '+(item.title||item.id));}}catch{refineRecipeWarnings.push('Could not verify '+stage+' adapters.');}}renderRefineLoras(stage);
  }
  $('refine-aspect').value=recipe.aspect||'custom';$('refine-negative').value=recipe.negative||'';$('refine-denoise').value=recipe.denoise||'.65';$('refine-include-references').checked=!!recipe.includeReferences;$('refine-upscale').checked=!!recipe.upscale?.enabled&&!!refineUpscaleStatus?.model?.available;if(recipe.upscale?.enabled&&!refineUpscaleStatus?.model?.available)refineRecipeWarnings.push('SeedVR2 is unavailable; install it before enabling the saved upscale.');$('refine-upscale-preset').value=recipe.upscale?.preset||'3840';for(const key of ['width','height'])$('refine-upscale-'+key).value=recipe.upscale?.[key]||'';$('refine-recipe-name').value=recipe.name;updateRefineControls();$('refine-recipe-note').textContent=refineRecipeWarnings.length?'Recipe loaded with warnings: '+refineRecipeWarnings.join(' · '):'Loaded '+recipe.name+'. Choose the draft or reference images for this run.';$('refine-recipe-note').classList.toggle('error',!!refineRecipeWarnings.length);
  }catch(error){$('refine-recipe-note').textContent='Recipe could not be fully loaded: '+error.message;$('refine-recipe-note').classList.add('error');}finally{refineRecipeLoading=false;updateRefineControls();}
}
$('refine-recipe-save').onclick=saveRefineRecipe;$('refine-recipe-load').onclick=loadRefineRecipe;$('refine-recipe-delete').onclick=()=>{const name=$('refine-recipe').value;if(!name)return;try{localStorage.setItem(REFINE_RECIPES_KEY,JSON.stringify(readRefineRecipes().filter(item=>item.name!==name)));renderRefineRecipes();$('refine-recipe-note').textContent='Deleted recipe '+name+'. Images remain in the library.';}catch{$('refine-recipe-note').textContent='Recipe storage is unavailable.';}};
for(const stage of ['draft','final']){$('refine-'+stage+'-lora-library').onclick=()=>openLoraLibrary(stage);$('refine-'+stage+'-transparent').onchange=()=>{markRefineRecipeEdited();updateRefineControls();};}
function refineStatus(text,error=false){$('refine-status').textContent=text;$('refine-status').classList.toggle('error',error);}
function modelBenefit(model){return model.short_benefit||{'qwen':'Edits & alpha','z-image-turbo':'Fast images','flux2-klein-4b':'Fast edits','flux2-klein-9b':'Detailed edits','hidream-o1':'Text & layouts'}[model.id]||model.benefit||'';}
function refineModels(stage){return generationModels.filter(model=>!model.historical&&(stage==='draft'?model.capabilities?.text_to_image:model.capabilities?.image_reference&&model.capabilities?.max_references>=1));}
function fillRefineModels(stage){
  if(window.__LOCAL_IMAGE_REACT__)return;
  const select=$('refine-'+stage+'-model'),previous=select.value,candidates=refineModels(stage);select.replaceChildren();
  for(const model of candidates){const option=document.createElement('option');option.value=model.id;option.textContent=model.label+' · '+modelBenefit(model);select.append(option);}
  const preferred=stage==='draft'?['flux2-klein-4b','z-image-turbo','qwen']:['flux2-klein-9b','qwen','hidream-o1'];
  if(previous&&!candidates.some(model=>model.id===previous)){const unavailable=document.createElement('option');unavailable.value=previous;unavailable.textContent='Unavailable · '+previous;select.append(unavailable);}
  select.value=previous||preferred.find(id=>candidates.some(model=>model.id===id&&model.available))||candidates.find(model=>model.available)?.id||candidates[0]?.id||'';
}
function refinePreset(){
  const model=refineModel('final'),shape=$('refine-aspect').value;if(shape==='custom'||!model)return;
  const [aspectWidth,aspectHeight]=shape.split(':').map(Number),limits=generationWorkflowLimits(model,true);
  const area=(model.defaults?.width||1024)*(model.defaults?.height||1024);
  const presets={'1:1':[1024,1024],'3:2':[1536,1024],'2:3':[1024,1536],'16:9':[1536,864]};
  const unit=Math.sqrt(area/(aspectWidth*aspectHeight));
  const [width,height]=area<=1048576?presets[shape]:[aspectWidth*unit,aspectHeight*unit];
  const fitted=window.LocalImageGenerationSize?window.LocalImageGenerationSize.fitDimensions({width,height,ratio:aspectWidth/aspectHeight,locked:true},limits):{width:Math.round(width),height:Math.round(height)};
  $('refine-final-width').value=String(fitted.width);$('refine-final-height').value=String(fitted.height);
}
function syncRefineModel(stage,{reset=true}={}){
  if(window.__LOCAL_IMAGE_REACT__)return;
  const model=refineModel(stage),variant=$('refine-'+stage+'-variant'),previous=variant.value;variant.replaceChildren();
  for(const item of model?.variants||[]){const option=document.createElement('option');option.value=item.id;option.textContent=item.label;variant.append(option);}
  if(!reset&&previous&&!model?.variants?.some(item=>item.id===previous)){const missing=document.createElement('option');missing.value=previous;missing.textContent='Unavailable · '+previous;variant.append(missing);}
  variant.value=!reset&&previous?previous:model?.defaults?.variant||model?.variants?.[0]?.id||'';
  variant.hidden=(model?.variants?.length||0)<=1;
  const limits=model?.limits||{};
  for(const dimension of ['width','height'])configureGenerationDimension($('refine-'+stage+'-'+dimension),dimension,model,stage==='final'||refineReferences.length>0);
  for(const key of ['steps','guidance']){const input=$('refine-'+stage+'-'+key);input.min=limits['min_'+key]||1;input.max=limits['max_'+key]||1;if(reset)input.value=String(model?.defaults?.[key]||1);}
  if(reset){if(stage==='final')refinePreset();else{for(const dimension of ['width','height'])$('refine-draft-'+dimension).value=String(model?.defaults?.[dimension]||1024);}}
  $('refine-'+stage+'-guidance-row').hidden=(limits.max_guidance||1)<=1;
  const cap=model?.capabilities||{};$('refine-'+stage+'-transparent-row').hidden=!cap.transparent;if(!cap.transparent)$('refine-'+stage+'-transparent').checked=false;
  $('refine-'+stage+'-lora-options').hidden=cap.lora===false||cap.loras===false;if(cap.lora===false||cap.loras===false)refineLoras[stage]=[];renderRefineLoras(stage);
  $('refine-'+stage+'-recommended').textContent='Recommended for this model: '+(model?.recommended?.steps||model?.defaults?.steps||1)+' steps'+(stage==='final'?' · '+(model?.defaults?.width||1024)+' × '+(model?.defaults?.height||1024)+' canvas.':'.');
  renderRefineReferences();updateRefineControls();
}
function refineInputsValid(stage){
  const model=refineModel(stage),limits=model?.limits||{},width=Number($('refine-'+stage+'-width').value),height=Number($('refine-'+stage+'-height').value),steps=Number($('refine-'+stage+'-steps').value),guidance=Number($('refine-'+stage+'-guidance').value),seed=$('refine-'+stage+'-seed').value.trim();
  return generationCanvasValid(width,height,model,stage==='final'||refineReferences.length>0)&&Number.isInteger(steps)&&steps>=(limits.min_steps||1)&&steps<=(limits.max_steps||100)&&guidance>=(limits.min_guidance||1)&&guidance<=(limits.max_guidance||1)&&(!seed||(Number.isSafeInteger(Number(seed))&&Number(seed)>=0));
}
function refinementReferences(){
  const selected=refineSelectedDraft();if(!selected)return[];
  return [selected.session.id,...($('refine-include-references').checked?(selected.references||[]).filter(id=>id!==selected.session.id):[])];
}
function upscaleTarget(source){
  const preset=$('refine-upscale-preset').value;if(preset==='custom')return{width:Number($('refine-upscale-width').value),height:Number($('refine-upscale-height').value)};
  const scale=Number(preset)/Math.max(source.width,source.height),bounds=generationCanvasBounds({limits:refineUpscaleStatus?.limits||{}});return{width:Math.round(source.width*scale/bounds.width.step)*bounds.width.step,height:Math.round(source.height*scale/bounds.height.step)*bounds.height.step};
}
function upscaleSizeValid(source,target){
  if(!source)return false;const limits=refineUpscaleStatus?.limits||{},width=target.width,height=target.height;
  return generationCanvasValid(width,height,{limits})&&width>=source.width&&height>=source.height&&(width>source.width||height>source.height)&&Math.min(Math.abs(height-width*source.height/source.width),Math.abs(width-height*source.width/source.height))<=2;
}
function updateRefineUpscaleControls(){
  if(window.__LOCAL_IMAGE_REACT__)return;
  const enabled=!!refineUpscaleStatus?.enabled,available=enabled&&!!refineUpscaleStatus?.model?.available,locked=!!refineJob||busy||refineRecipeLoading;
  $('refine-upscale-options').hidden=!enabled;if(!enabled)$('refine-upscale').checked=false;
  for(const dimension of ['width','height'])configureGenerationDimension($('refine-upscale-'+dimension),dimension,{limits:refineUpscaleStatus?.limits||{}});
  $('refine-size-label').textContent=$('refine-upscale').checked?'Refinement canvas':'Final resolution';
  const canvas={width:Number($('refine-final-width').value),height:Number($('refine-final-height').value)},target=upscaleTarget(canvas);
  if($('refine-upscale-preset').value!=='custom'){for(const key of ['width','height'])$('refine-upscale-'+key).value=String(target[key]||'');}
  const selected=refineSelectedResult()||refineSelectedDraft(),direct=selected?upscaleTarget(selected.session):null;
  $('refine-upscale-only').disabled=locked||!available||!selected||!upscaleSizeValid(selected?.session,direct||{});
  $('refine-upscale-only').textContent='Upscale '+(refineSelectedResult()?'refined image':'selected draft')+' only'+(direct?' · '+direct.width+' × '+direct.height:'');
  $('refine-upscale').disabled=locked||!available;
  $('refine-upscale-note').textContent=!available?refineUpscaleStatus?.model?.reason||'Connect or install SeedVR2 to upscale.':'SeedVR2 7B · Photo detail · 4K. Preserves the aspect ratio; 16:9 becomes 3840 × 2160. Fine details may change; review faces and lettering. Both versions stay in the library.';
  $('refine-upscale-setup').hidden=available;
  const job=generationDownloadJob,seedJob=job?.model==='seedvr2',variant=job?.models?.find(model=>model.id==='seedvr2')?.variants?.find(item=>item.id==='fp16');
  $('refine-upscale-download').disabled=locked||refineUpscaleSetupBusy||!!job?.running||!nativeSetup;
  $('refine-upscale-download').textContent='Download SeedVR2 · '+setupBytes(variant?.missing_bytes||variant?.total_bytes||16981908774);
  $('refine-upscale-download').title=nativeSetup?'Install SeedVR2 in the configured models folder':'Model downloads are available in the desktop app.';
  $('refine-upscale-refresh').disabled=locked||refineUpscaleSetupBusy;$('refine-upscale-progress').hidden=!(seedJob&&job.running);$('refine-upscale-progress').value=job?.progress||0;
  if(seedJob&&(job.running||job.phase==='error'))$('refine-upscale-note').textContent=job.message||job.error;
  if($('refine-upscale').checked&&(!available||!upscaleSizeValid(canvas,target))){$('refine-final-run').disabled=true;$('refine-final-run').title='Upscaled output must enlarge the refinement canvas and preserve its aspect ratio.';}
}
function updateRefineControls(){
  if(window.__LOCAL_IMAGE_REACT__)return;
  const locked=!!refineJob||busy||refineRecipeLoading||generationLoading,draft=refineSelectedDraft(),result=refineSelectedResult();
  const originalRefs=draft?.references?.length||0;if(!originalRefs)$('refine-include-references').checked=false;
  for(const stage of ['draft','final']){
    const model=refineModel(stage),variant=model?.variants?.find(item=>item.id===$('refine-'+stage+'-variant').value),cap=model?.capabilities||{},available=!!model?.available&&!!variant&&variant.available!==false&&(stage==='draft'?!!cap.text_to_image:!!cap.image_reference&&(cap.max_references||0)>=1);
    const refs=stage==='draft'?(cap.image_reference||cap.image_to_image?refineReferences.map(item=>item.id):[]):refinementReferences(),limit=cap.max_references||0,tooMany=refs.length>limit;
    const prompt=$('refine-'+stage+'-prompt').value.trim(),valid=refineInputsValid(stage);
    const missingLora=refineLoras[stage].some(item=>item.missing),referenceLora=refineLoras[stage].some(item=>item.usage==='reference-edit')&&!refs.length;
    $('refine-'+stage+'-run').disabled=locked||!available||!prompt||!valid||tooMany||missingLora||referenceLora||(stage==='final'&&!draft);
    $('refine-'+stage+'-run').textContent=refineJob===stage?(stage==='draft'?'Generating…':'Refining…'):(stage==='draft'?'Generate draft':'Refine selected draft');
    $('refine-'+stage+'-run').title=tooMany?'Remove references: this model accepts '+limit:!valid?'Use supported dimensions, step count, guidance and a nonnegative seed':!available?'Connect or install this model':stage==='final'?'Use the selected draft as the first image reference':'Create a new draft';
    const note=$('refine-'+stage+'-model-note');note.textContent=missingLora?'A recipe adapter is unavailable. Install it in Styles / LoRAs or remove it.':referenceLora?'The selected adapter needs a reference image.':tooMany?'Too many references ('+refs.length+'/'+limit+').':available?(stage==='final'?'Reference editing · Up to '+limit+' images':'Ready · '+(cap.image_reference?'Semantic image references':cap.image_to_image?'Optional starting-image variation':'Text to image')):variant?.reason||model?.reason||'Select an installed model.';note.classList.toggle('error',tooMany||!available||missingLora||referenceLora);
    $('refine-'+stage+'-output-note').textContent=($('refine-'+stage+'-transparent').checked&&cap.transparent?'Transparent PNG':'Opaque image')+' · '+(refineLoras[stage].length?refineLoras[stage].map(item=>item.title||item.id).join(', '):'No style adapters');$('refine-'+stage+'-lora-library').disabled=locked||!model;
  }
  for(const input of $('refine-dialog').querySelectorAll('input,select,textarea'))input.disabled=locked;
  $('refine-include-references').disabled=locked||!originalRefs;$('refine-original-references-label').textContent=originalRefs?'Include the draft’s '+originalRefs+' original reference image'+(originalRefs===1?'':'s'):draft?.session?.generation?.reference_count?'Original reference files are not included with this library image.':'This draft has no original reference images to include.';
  $('refine-negative-row').hidden=!refineModel('final')?.capabilities?.negative_prompt;
  $('refine-negative').disabled=locked||Number($('refine-final-guidance').value)<=1;
  const draftCap=refineModel('draft')?.capabilities||{},canReference=!!(draftCap.image_reference||draftCap.image_to_image),referenceFull=refineReferences.length>=(draftCap.max_references||0);
  $('refine-add-reference').disabled=locked||!canReference||referenceFull;$('refine-current-reference').disabled=locked||!session||!canReference||referenceFull;
  $('refine-denoise-row').hidden=!draftCap.denoise||!refineReferences.length;
  $('refine-use-current').disabled=locked||!session;$('refine-library').disabled=locked;
  $('refine-open-draft').disabled=locked||!draft;$('refine-open-result').disabled=locked||!result;$('refine-close').disabled=locked;
  for(const id of ['refine-recipe-save','refine-recipe-load','refine-recipe-delete'])$(id).disabled=locked;
  $('refine-refresh-models').disabled=locked;
  for(const input of $('refine-dialog').querySelectorAll('.selected-lora button'))input.disabled=locked;
  for(const id of ['refine-compare-fit','refine-compare-actual','refine-compare-plus','refine-compare-minus'])$(id).disabled=!draft&&!result;
  for(const button of $('refine-dialog').querySelectorAll('.refine-strip button,.refine-references button'))button.disabled=locked;
  updateRefineUpscaleControls();
  window.LocalImageAssetsBridge?.publish();
}
function renderRefineReferences(){
  if(window.__LOCAL_IMAGE_REACT__)return;
  $('refine-references').replaceChildren();for(const [index,item]of refineReferences.entries()){
    const row=document.createElement('div');row.className='generation-reference';const image=document.createElement('img');image.src=item.thumbnail;image.alt='';const title=document.createElement('span');title.textContent=item.name;const remove=document.createElement('button');remove.textContent='×';remove.setAttribute('aria-label','Remove draft reference '+item.name);remove.onclick=()=>{refineReferences.splice(index,1);renderRefineReferences();updateRefineControls();};row.append(image,title,remove);$('refine-references').append(row);
  }
  const cap=refineModel('draft')?.capabilities||{};$('refine-reference-note').textContent=cap.image_reference?'Up to '+cap.max_references+' semantic references.':cap.image_to_image?'One starting image creates a variation; it is not semantic reference editing.':'This model creates images from text only.';
}
function renderRefineImages(){
  if(window.__LOCAL_IMAGE_REACT__)return;
  const selected=refineSelectedDraft(),result=refineSelectedResult();
  for(const [stage,item]of [['draft',selected],['result',result]]){
    $('refine-'+stage+'-image').hidden=!item;$('refine-'+stage+'-empty').hidden=!!item;if(item)$('refine-'+stage+'-image').src=sessionPreview(item.session)+'&full=true';
    const holder=$('refine-'+stage+'-strip');holder.replaceChildren();
    for(const entry of stage==='draft'?refineDrafts:refineResults.filter(value=>value.draftId===refineDraftId)){
      const button=document.createElement('button'),image=document.createElement('img');image.src=sessionPreview(entry.session);image.alt='';button.append(image);button.title=entry.session.name+' · '+entry.session.width+' × '+entry.session.height;button.setAttribute('aria-label',button.title);button.setAttribute('aria-pressed',String(entry===item));button.onclick=()=>{if(stage==='draft'){refineDraftId=entry.session.id;refineResultId=refineResults.filter(value=>value.draftId===refineDraftId).at(-1)?.session.id||null;}else refineResultId=entry.session.id;renderRefineImages();};holder.append(button);
    }
  }
  $('refine-result-size').textContent=result?result.session.width+' × '+result.session.height:'';applyRefineComparison();updateRefineControls();
}
function addRefineDraft(data,references=[]){
  if(window.LocalImageReactFeatures?.generation)return window.LocalImageReactFeatures.generation.addDraft(data,references);
  if(!refineDrafts.some(item=>item.session.id===data.id))refineDrafts.push({session:data,references:[...references]});
  refineDraftId=data.id;refineResultId=refineResults.filter(item=>item.draftId===data.id).at(-1)?.session.id||null;renderRefineImages();
}
async function openRefineWorkspace({allowBusy=false}={}){
  if(window.LocalImageReactFeatures?.generation)return window.LocalImageReactFeatures.generation.setMode('refine');
  const owned=allowBusy&&window.LocalImageAssetsBridge?.ownsOperation();
  if((busy&&!owned)||modalOpen())return;
  if(owned&&workspace!=='generate')setWorkspace('generate',{allowBusy:true});
  closeMenus();resetTransientInput();$('refine-dialog').showModal();
  try{
    if(!generationModels.length)await loadGenerationModels();
    try{refineUpscaleStatus=await(await api('/api/local-remove/generation/upscale/models')).json();}catch{refineUpscaleStatus=null;}
    for(const stage of ['draft','final'])fillRefineModels(stage);
    if(!refineInitialized){$('refine-draft-prompt').value=$('gen-prompt').value;refineReferences=generationReferences.map(item=>({...item}));refineInitialized=true;for(const stage of ['draft','final'])syncRefineModel(stage);}
    else for(const stage of ['draft','final'])syncRefineModel(stage,{reset:false});
    renderRefineRecipes();renderRefineImages();
  }catch(error){refineStatus(error.message,true);}
}
async function runRefineStage(stage){
  if(window.LocalImageReactFeatures?.generation)return window.LocalImageReactFeatures.generation.run(stage);
  if($('refine-'+stage+'-run').disabled)return;
  const model=refineModel(stage),cap=model.capabilities||{},seed=$('refine-'+stage+'-seed').value.trim(),refs=stage==='draft'?(cap.image_reference||cap.image_to_image?refineReferences.map(item=>item.id):[]):refinementReferences(),selectedDraftId=refineDraftId;
  const payload={model:model.id,variant:$('refine-'+stage+'-variant').value,prompt:$('refine-'+stage+'-prompt').value.trim(),width:Number($('refine-'+stage+'-width').value),height:Number($('refine-'+stage+'-height').value),steps:Number($('refine-'+stage+'-steps').value),guidance:Number($('refine-'+stage+'-guidance').value),transparent:!!cap.transparent&&$('refine-'+stage+'-transparent').checked,reference_session_ids:refs,loras:(cap.lora===false||cap.loras===false)?[]:refineLoras[stage].map(item=>({id:item.id,strength:item.strength})),...(seed?{seed:Number(seed)}:{})};
  if(stage==='final'&&cap.negative_prompt&&payload.guidance>1)payload.negative_prompt=$('refine-negative').value.trim();
  if(stage==='draft'&&cap.denoise&&refs.length)payload.denoise=Number($('refine-denoise').value);
  refineJob=stage;activeTask='generate';setBusy(true);updateRefineControls();const started=Date.now();
  const tick=()=>{const fallback=(stage==='draft'?'Generating draft':'Refining selected draft')+' with '+model.label+' · '+Math.round((Date.now()-started)/1000)+'s';refineStatus(window.localImageProgressText?.(fallback)||fallback);};tick();const timer=setInterval(tick,1000);
  try{
    const result=await json('/api/local-remove/generation',payload);clearInterval(timer);
    if(stage==='draft')addRefineDraft(result.session,refs);else{refineResults.push({session:result.session,draftId:selectedDraftId});refineResultId=result.session.id;renderRefineImages();}
    refineStatus((stage==='draft'?'Draft':'Refined image')+' created · '+result.width+' × '+result.height+' · Seed '+result.seed+(result.library_warning?' · '+result.library_warning:' · Saved to image library.'),!!result.library_warning);
    if(stage==='final'&&$('refine-upscale').checked){try{await performRefineUpscale(result.session,selectedDraftId,upscaleTarget(result.session));}catch(error){refineStatus('Refinement saved. Upscale failed: '+error.message,true);}}
  }catch(error){refineStatus(error.message,true);}finally{clearInterval(timer);refineJob=null;activeTask=null;setBusy(false);updateRefineControls();}
}
async function performRefineUpscale(source,draftId,target){
  refineJob='upscale';updateRefineControls();const started=Date.now(),tick=()=>{const fallback='Upscaling with SeedVR2 7B · '+target.width+' × '+target.height+' · '+Math.round((Date.now()-started)/1000)+'s';refineStatus(window.localImageProgressText?.(fallback)||fallback);};tick();const timer=setInterval(tick,1000);
  try{const result=await json('/api/local-remove/generation/upscale',{session_id:source.id,revision:source.revision,...target});refineResults.push({session:result.session,draftId});refineResultId=result.session.id;renderRefineImages();refineStatus('Upscaled image created · '+result.session.width+' × '+result.session.height+(result.library_warning?' · '+result.library_warning:' · Saved to image library.'),!!result.library_warning);}
  finally{clearInterval(timer);}
}
async function upscaleRefineSelection(){
  if(window.LocalImageReactFeatures?.generation)return window.LocalImageReactFeatures.generation.run('upscale');
  if($('refine-upscale-only').disabled)return;const selected=refineSelectedResult()||refineSelectedDraft();refineJob='upscale';activeTask='generate';setBusy(true);updateRefineControls();
  try{await performRefineUpscale(selected.session,refineDraftId,upscaleTarget(selected.session));}catch(error){refineStatus(error.message,true);}finally{refineJob=null;activeTask=null;setBusy(false);updateRefineControls();}
}
async function openRefineDocument(item){if(!item||busy)return;$('refine-dialog').close();await openSession(item.session);setWorkspace('generate');}
$('draft-refine-open').onclick=openRefineWorkspace;
$('refine-refresh-models').onclick=async()=>{if(busy||refineJob||refineRecipeLoading)return;try{await loadGenerationModels(true);for(const stage of ['draft','final']){fillRefineModels(stage);syncRefineModel(stage,{reset:false});}refineStatus('Models and ComfyUI status refreshed. Your stage settings are retained.');}catch(error){refineStatus(error.message,true);}};
$('refine-close').onclick=()=>{if(!refineJob&&!busy&&!refineRecipeLoading)$('refine-dialog').close();};
$('refine-dialog').addEventListener('cancel',event=>{if(refineJob||busy||refineRecipeLoading)event.preventDefault();});
$('refine-dialog').addEventListener('close',()=>{$('draft-refine-open').focus();});
for(const stage of ['draft','final']){
  $('refine-'+stage+'-model').onchange=()=>{refineLoras[stage]=[];syncRefineModel(stage);markRefineRecipeEdited();};$('refine-'+stage+'-variant').onchange=()=>{markRefineRecipeEdited();updateRefineControls();};
  for(const key of ['prompt','width','height','steps','guidance','seed'])$('refine-'+stage+'-'+key).oninput=()=>{if(stage==='final'&&['width','height'].includes(key))$('refine-aspect').value='custom';markRefineRecipeEdited();updateRefineControls();};
  $('refine-'+stage+'-run').onclick=()=>runRefineStage(stage);
}
$('refine-aspect').onchange=()=>{refinePreset();updateRefineControls();};$('refine-include-references').onchange=updateRefineControls;
$('refine-upscale').onchange=updateRefineControls;$('refine-upscale-preset').onchange=updateRefineControls;$('refine-upscale-only').onclick=upscaleRefineSelection;
$('refine-upscale-download').onclick=async()=>{if($('refine-upscale-download').disabled)return;refineUpscaleSetupBusy=true;updateRefineControls();try{await nativeRequest('setupDownloadGenerationModel',null,{model:'seedvr2',variant:'fp16'});await refreshGenerationDownload();}catch(error){refineStatus(error.message,true);}finally{refineUpscaleSetupBusy=false;updateRefineControls();}};
$('refine-upscale-refresh').onclick=async()=>{refineUpscaleSetupBusy=true;updateRefineControls();try{refineUpscaleStatus=await(await api('/api/local-remove/generation/upscale/models')).json();}catch(error){refineStatus(error.message,true);}finally{refineUpscaleSetupBusy=false;updateRefineControls();}};
for(const key of ['width','height'])$('refine-upscale-'+key).oninput=()=>{$('refine-upscale-preset').value='custom';updateRefineControls();};
$('refine-use-current').onclick=()=>{if(session)addRefineDraft(cloneDocument(session));};
$('refine-current-reference').onclick=()=>{if(session&&!refineReferences.some(item=>item.id===session.id)){refineReferences.push({id:session.id,name:session.name,thumbnail:sessionPreview(session)});renderRefineReferences();updateRefineControls();}};
$('refine-add-reference').onclick=()=>$('refine-reference-file').click();
$('refine-reference-file').onchange=async()=>{
  if(busy)return;setBusy(true);updateRefineControls();
  try{const limit=refineModel('draft')?.capabilities?.max_references||0;for(const file of Array.from($('refine-reference-file').files||[]).slice(0,Math.max(0,limit-refineReferences.length))){const form=new FormData();form.append('file',file);const data=await(await api('/api/local-remove/import',{method:'POST',body:form})).json();refineReferences.push({id:data.id,name:data.name,thumbnail:sessionPreview(data)});}renderRefineReferences();}
  catch(error){refineStatus(error.message,true);}finally{$('refine-reference-file').value='';setBusy(false);updateRefineControls();}
};
$('refine-open-draft').onclick=()=>openRefineDocument(refineSelectedDraft());$('refine-open-result').onclick=()=>openRefineDocument(refineSelectedResult());

function libraryStatus(text,error=false){if(window.__LOCAL_IMAGE_REACT__)return;$('generated-library-status').textContent=text;$('generated-library-status').classList.toggle('error',error);}
function visibleLibraryItems(){const query=$('generated-library-search').value.trim().toLowerCase();return generatedLibrary.items.filter(item=>!query||[item.name,item.model,item.generation?.prompt].join(' ').toLowerCase().includes(query));}
function updateLibraryControls(){
  if(window.__LOCAL_IMAGE_REACT__)return;
  const count=generatedLibrarySelection.size,locked=generatedLibraryBusy||!!refineJob,visible=visibleLibraryItems(),selected=visible.filter(item=>generatedLibrarySelection.has(item.id)).length;
  $('generated-library-select-all').checked=!!visible.length&&selected===visible.length;$('generated-library-select-all').indeterminate=selected>0&&selected<visible.length;
  for(const id of ['generated-library-close','generated-library-refresh','generated-library-search','generated-library-select-all','generated-library-select-mode'])$(id).disabled=locked;
  for(const input of $('generated-library-grid').querySelectorAll('input'))input.disabled=locked;
  const canOpen=generatedLibrarySelectMode?count===1:visible.some(item=>item.id===generatedLibraryFocused);
  $('generated-library-edit').disabled=locked||!canOpen;$('generated-library-as-draft').disabled=locked||!canOpen;
  $('generated-library-delete').disabled=locked||!generatedLibrarySelectMode||!count;$('generated-library-delete').hidden=!generatedLibrarySelectMode;$('generated-library-clear').disabled=locked||!generatedLibrary.count;
  $('generated-library-select-all-row').hidden=!generatedLibrarySelectMode;$('generated-library-select-mode').setAttribute('aria-pressed',String(generatedLibrarySelectMode));$('generated-library-select-mode').textContent=generatedLibrarySelectMode?'Done':'Select';
  $('generated-library-confirm-delete').disabled=locked;$('generated-library-cancel-delete').disabled=locked;
  $('generated-library-delete').textContent=count?'Delete selected ('+count+')':'Delete selected';
  $('generated-library-usage').textContent=generatedLibrary.count+' image'+(generatedLibrary.count===1?'':'s')+' · '+setupBytes(generatedLibrary.bytes||0)+' cache'+(count?' · '+count+' selected':'');
}
function renderGeneratedLibrary(){
  if(window.__LOCAL_IMAGE_REACT__)return;
  const items=visibleLibraryItems();$('generated-library-grid').replaceChildren();$('generated-library-empty').hidden=!!items.length;
  $('generated-library-empty').firstChild.textContent=generatedLibrary.count?'No matching images.':'No generated images yet.';
  for(const item of items){
    const label=document.createElement('div');label.className='generated-library-item';label.tabIndex=0;label.setAttribute('role','button');label.setAttribute('aria-label',item.name);label.setAttribute('aria-pressed',String(generatedLibrarySelectMode?generatedLibrarySelection.has(item.id):generatedLibraryFocused===item.id));label.dataset.focused=String(!generatedLibrarySelectMode&&generatedLibraryFocused===item.id);const focus=()=>{if(generatedLibraryBusy)return;if(generatedLibrarySelectMode){if(generatedLibrarySelection.has(item.id))generatedLibrarySelection.delete(item.id);else generatedLibrarySelection.add(item.id);}else generatedLibraryFocused=item.id;cancelLibraryDelete();renderGeneratedLibrary();$('generated-library-grid').querySelector('[data-library-id="'+item.id+'"]')?.focus({preventScroll:true});};label.dataset.libraryId=item.id;label.onclick=event=>{if(event.target!==check)focus();};label.onkeydown=event=>{if(event.target!==label)return;if(event.key==='Enter'||event.key===' '){event.preventDefault();focus();}};const check=document.createElement('input');check.type='checkbox';check.hidden=!generatedLibrarySelectMode;check.checked=generatedLibrarySelection.has(item.id);check.setAttribute('aria-label','Select '+item.name);check.onclick=event=>event.stopPropagation();check.onchange=()=>{if(check.checked)generatedLibrarySelection.add(item.id);else generatedLibrarySelection.delete(item.id);cancelLibraryDelete();renderGeneratedLibrary();};
    const image=document.createElement('img');image.src=item.thumbnail;image.alt='';image.loading='lazy';const title=document.createElement('strong');title.textContent=item.name;title.title=item.generation?.prompt||item.name;const info=document.createElement('small');info.textContent=item.width+' × '+item.height+' · '+(generationModels.find(model=>model.id===item.model)?.label||item.model);label.append(check,image,title,info);$('generated-library-grid').append(label);
  }
  updateLibraryControls();
}
async function refreshGeneratedLibrary(){
  if(window.__LOCAL_IMAGE_REACT__){const assets=window.LocalImageReactFeatures?.assets;if(assets?.getSnapshot().open&&assets.getSnapshot().tab==='generated')return assets.refresh();return;}
  if(generatedLibraryBusy)return;generatedLibraryBusy=true;updateLibraryControls();libraryStatus('Loading library…');
  try{generatedLibrary=await(await api('/api/local-remove/generation/library')).json();const ids=new Set(generatedLibrary.items.map(item=>item.id));generatedLibrarySelection=new Set([...generatedLibrarySelection].filter(id=>ids.has(id)));if(!ids.has(generatedLibraryFocused))generatedLibraryFocused=null;renderGeneratedLibrary();libraryStatus(generatedLibrary.warning||'Click an image to open or use as a draft. Choose Select to delete several cached copies.',!!generatedLibrary.warning);}
  catch(error){libraryStatus(error.message+' Use Refresh to retry.',true);}finally{generatedLibraryBusy=false;updateLibraryControls();}
}
async function openGeneratedLibrary(origin='editor'){
  if(window.__LOCAL_IMAGE_REACT__){if(busy||modalOpen())return;closeMenus();return window.LocalImageReactFeatures?.assets?.open('generated');}
  if(busy||generatedLibraryBusy||(modalOpen()&&!$('refine-dialog').open))return;generatedLibraryOrigin=origin;generatedLibrarySelectMode=false;generatedLibrarySelection.clear();closeMenus();cancelLibraryDelete();$('generated-library-dialog').showModal();await refreshGeneratedLibrary();
}
function cancelLibraryDelete(){generatedLibraryDelete=null;$('generated-library-confirm').hidden=true;}
function confirmLibraryDelete(all){
  if(generatedLibraryBusy)return;const count=all?generatedLibrary.count:generatedLibrarySelection.size;if(!count)return;
  generatedLibraryDelete=all?{all:true}:{ids:[...generatedLibrarySelection]};$('generated-library-confirm-text').textContent='Delete '+(all?'all ':'')+count+' cached library '+(count===1?'image?':'images?');$('generated-library-confirm').hidden=false;$('generated-library-confirm-delete').focus();
}
async function deleteLibraryCopies(){
  if(!generatedLibraryDelete||generatedLibraryBusy)return;generatedLibraryBusy=true;updateLibraryControls();
  try{const result=await json('/api/local-remove/generation/library/delete',generatedLibraryDelete);generatedLibrary=result;generatedLibrarySelection=new Set([...generatedLibrarySelection].filter(id=>!result.deleted.includes(id)));if(result.deleted.includes(generatedLibraryFocused))generatedLibraryFocused=null;cancelLibraryDelete();renderGeneratedLibrary();libraryStatus('Deleted '+result.deleted.length+' library copies · Freed '+setupBytes(result.freed_bytes||0)+'.');}
  catch(error){libraryStatus(error.message,true);}finally{generatedLibraryBusy=false;updateLibraryControls();}
}
async function openLibrarySelection(asDraft){
  if(window.__LOCAL_IMAGE_REACT__)return window.LocalImageReactFeatures?.assets?.openGenerated(asDraft?'draft':'image');
  if(generatedLibraryBusy)return;const id=generatedLibrarySelectMode?(generatedLibrarySelection.size===1?[...generatedLibrarySelection][0]:null):generatedLibraryFocused;if(!id)return;generatedLibraryBusy=true;updateLibraryControls();libraryStatus('Opening image…');
  try{
    const result=await json('/api/local-remove/generation/library/'+encodeURIComponent(id)+'/open',{});$('generated-library-dialog').close();
    if(asDraft){if(!$('refine-dialog').open)await openRefineWorkspace();addRefineDraft(result.session);refineStatus('Library image selected as the draft. Adjust the refinement prompt and settings.');}
    else{if($('refine-dialog').open)$('refine-dialog').close();await openSession(result.session);setWorkspace('generate');}
  }catch(error){libraryStatus(error.message,true);}finally{generatedLibraryBusy=false;updateLibraryControls();}
}
$('generated-library-open').onclick=()=>openGeneratedLibrary();$('generated-library-menu').onclick=()=>openGeneratedLibrary();$('refine-library').onclick=()=>openGeneratedLibrary('refine');
$('generated-library-close').onclick=()=>{if(!generatedLibraryBusy)$('generated-library-dialog').close();};$('generated-library-dialog').addEventListener('cancel',event=>{if(generatedLibraryBusy)event.preventDefault();});
$('generated-library-dialog').addEventListener('close',()=>{if($('refine-dialog').open)$('refine-library').focus();else $('generated-library-open').focus();});
$('generated-library-refresh').onclick=refreshGeneratedLibrary;$('generated-library-search').oninput=()=>{if(!visibleLibraryItems().some(item=>item.id===generatedLibraryFocused))generatedLibraryFocused=null;cancelLibraryDelete();renderGeneratedLibrary();};
$('generated-library-select-mode').onclick=()=>{generatedLibrarySelectMode=!generatedLibrarySelectMode;generatedLibrarySelection.clear();cancelLibraryDelete();renderGeneratedLibrary();libraryStatus(generatedLibrarySelectMode?'Select cached copies to delete. Open documents and saved files are retained.':'Click an image to focus it.');};
$('generated-library-select-all').onchange=()=>{for(const item of visibleLibraryItems()){if($('generated-library-select-all').checked)generatedLibrarySelection.add(item.id);else generatedLibrarySelection.delete(item.id);}cancelLibraryDelete();renderGeneratedLibrary();};
$('generated-library-delete').onclick=()=>confirmLibraryDelete(false);$('generated-library-clear').onclick=()=>confirmLibraryDelete(true);$('generated-library-cancel-delete').onclick=cancelLibraryDelete;$('generated-library-confirm-delete').onclick=deleteLibraryCopies;
$('generated-library-edit').onclick=()=>openLibrarySelection(false);$('generated-library-as-draft').onclick=()=>openLibrarySelection(true);
init();
