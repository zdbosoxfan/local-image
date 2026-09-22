from pathlib import Path
p = Path(__file__).with_name('local_remove.html')
s = p.read_text(encoding='utf-8')
def replace(a,b):
    global s
    assert a in s,a[:160]
    s=s.replace(a,b)
replace('.stage .overlay{', '.stage .layer-stack{position:absolute;inset:0;pointer-events:none;overflow:hidden}\n.stage .layer-image{position:absolute;max-width:none;max-height:none;pointer-events:none;user-select:none}\n.document-close{padding:2px;min-height:19px;width:20px;height:19px;display:flex;align-items:center;justify-content:center;margin-left:3px}\n.document-close .icon{width:11px;height:11px}\n.close-summary{white-space:pre-line;max-height:180px;overflow-y:auto;line-height:1.65}\n.stage .overlay{')
replace('  <button id="open-folder"', '  <button id="open-project" role="menuitem"><span>Open editable project…</span><kbd>Ctrl+Alt+O</kbd></button>\n  <button id="open-folder"')
replace('  <button id="return"', '  <button id="save-project" role="menuitem" disabled><span>Save editable project…</span><kbd>Ctrl+Alt+S</kbd></button>\n  <button id="close-image" role="menuitem" disabled><span>Close image…</span><kbd>Ctrl+W</kbd></button>\n  <div class="menu-separator" role="separator"></div>\n  <button id="return"')
replace('<span class="filename" id="filename">No image open</span></div>', '<span class="filename" id="filename">No image open</span><button class="document-close" id="document-close" data-command="close-image" aria-label="Close image" title="Close image (Ctrl+W)" hidden><svg class="icon" aria-hidden="true"><use href="#i-close"/></svg></button></div>')
replace('<canvas id="photo"></canvas>', '<canvas id="photo"></canvas><div class="layer-stack" id="layer-stack"></div>')
replace('<script nonce="__NONCE__">', '''<input type="file" id="project-file" accept=".lremove" hidden>
<dialog id="close-dialog" aria-labelledby="close-title" aria-describedby="close-warning"><div class="dialog-header"><h2 id="close-title">Close image?</h2></div><p id="close-warning" class="overwrite-warning">Closing clears the working layers.</p><p id="close-summary" class="close-summary"></p><p class="muted">Save an editable .lremove project to keep the original and layers. PNG, JPEG, TIFF, and WebP saves are flattened images.</p><p id="close-selection-warning" class="muted" hidden>Pending brush and pen selections are not included in a project and will be cleared when you close.</p><div class="dialog-actions"><button id="close-cancel" autofocus>Cancel</button><button id="close-discard" class="overwrite-action">Discard layers and close</button><button id="close-save" class="primary">Save project and close…</button></div></dialog>
<script nonce="__NONCE__">''')
replace("const modalOpen=()=>$('settings-dialog').open||$('overwrite-dialog').open||!!overwritePrompt;", "let closePrompt=null,closeInProgress=false;\nconst modalOpen=()=>$('settings-dialog').open||$('overwrite-dialog').open||$('close-dialog').open||!!overwritePrompt||!!closePrompt;")
replace('const viewStates=new Map(),nativePending=new Map();', '''const viewStates=new Map(),nativePending=new Map(),openDocuments=new Map(),displayCache=new Map(),layerQueues=new Map();
const cloneDocument=data=>JSON.parse(JSON.stringify(data));
const layerChangesPending=()=>[...layerQueues.values()].some(queue=>queue.pending.length>0);
function trackDocument(data){if(data?.id)openDocuments.set(data.id,data);}
''')
replace("  const active=!!session&&!busy;", "  const active=!!session&&!busy&&!layerChangesPending();")
replace("'save-unique','merge','size','finish'", "'save-unique','save-project','close-image','merge','size','finish'")
replace("  $('open').disabled=busy;", "  $('open').disabled=busy||closeInProgress;\n  $('open-project').disabled=busy||closeInProgress;\n  $('document-close').hidden=!session;")
replace("  const status=!session?'':saveInProgress?", "  const status=!session?'':layerChangesPending()?'Updating layers…':saveInProgress?")
replace("  $('document-state').title=pending?", "  $('save-project').title=session?.project_name?'Save editable layers · '+session.project_name:'Save the original and editable layers in a .lremove project';\n  $('document-state').title=pending?")
replace("  if(!session)return;\n  endGesture();\n  viewStates.set", "  if(!session)return;\n  trackDocument(session);endGesture();\n  viewStates.set")
replace("function nativeRequest(action,files=null){", "function nativeRequest(action,files=null,details={}){")
replace("{id,action},files", "{id,action,...details},files")
replace("nativeBridge.postMessage({id,action});", "nativeBridge.postMessage({id,action,...details});")
replace("    const data=event.data;if(data?.type!=='local-remove-native'||!nativePending.has(data.id))return;", "    const data=event.data;\n    if(data?.type==='local-remove-native'&&data.event==='requestClose'){handleNativeClose(data.id);return;}\n    if(data?.type!=='local-remove-native'||!nativePending.has(data.id))return;")
replace("  const images=Array.from(files).filter(supportedFile);", "  const allFiles=Array.from(files);\n  if(allFiles.length===1&&/\\.lremove$/i.test(allFiles[0].name)){await importProjectFile(allFiles[0]);return;}\n  const images=allFiles.filter(supportedFile);")
start=s.index('function paintPhoto(){')
end=s.index('function paintMask(){',start)
s=s[:start]+'''function paintPhoto(){
  if(!originalImage)return;
  bc.clearRect(0,0,baseCanvas.width,baseCanvas.height);
  const photo=$('photo-image');if(photo.src!==originalImage.src)photo.src=originalImage.src;
  $('layer-stack').hidden=showOriginal;overlay.hidden=showOriginal;draft.hidden=showOriginal;
  $('before').setAttribute('aria-pressed',String(showOriginal));controls();
}
'''+s[end:]
replace("  if((event.ctrlKey||event.metaKey)&&event.key.toLowerCase()==='s'&&session){", "  if((event.ctrlKey||event.metaKey)&&event.key.toLowerCase()==='w'&&session){event.preventDefault();if(!busy)closeCurrentImage();return;}\n  if((event.ctrlKey||event.metaKey)&&event.altKey&&['s','o'].includes(event.key.toLowerCase())){event.preventDefault();if(!busy)$(event.key.toLowerCase()==='s'?'save-project':'open-project').click();return;}\n  if((event.ctrlKey||event.metaKey)&&event.key.toLowerCase()==='s'&&session){")
start=s.index('async function refreshPreview(){')
end=s.index('function layerList(){',start)
s=s[:start]+'''// Original and transparent patches are immutable. Toggling only changes a DOM image's visibility.
async function displayAssets(data){
  let assets=displayCache.get(data.id);
  if(!assets){assets={base:null,layers:new Map()};displayCache.set(data.id,assets);}
  const prefix='/api/local-remove/session/'+encodeURIComponent(data.id);
  if(!assets.base)assets.base=loadImage(prefix+'/base-display').catch(error=>{assets.base=null;throw error;});
  await Promise.all([assets.base,...data.layers.filter(layer=>!layer.discarded).map(layer=>{
    if(!assets.layers.has(layer.id)){
      const pending=loadImage(prefix+'/layer/'+encodeURIComponent(layer.id)+'/display').then(image=>{
        image.className='layer-image';image.alt='';image.draggable=false;image.dataset.layerId=layer.id;return image;
      }).catch(error=>{assets.layers.delete(layer.id);throw error;});assets.layers.set(layer.id,pending);
    }
    return assets.layers.get(layer.id);
  })]);
  return assets;
}
async function refreshPreview(){
  const requested=session,version=++requestVersion;
  const assets=await displayAssets(requested);
  if(version!==requestVersion||session?.id!==requested.id)return;
  originalImage=await assets.base;previewImage=originalImage;
  const nodes=await Promise.all(requested.layers.filter(layer=>!layer.discarded).map(layer=>assets.layers.get(layer.id)));
  const holder=$('layer-stack');
  // Reuse already decoded images. New generation and merge only fetch the new patch.
  const expected=new Set(nodes);for(const child of [...holder.children])if(!expected.has(child))child.remove();
  nodes.forEach((node,index)=>{if(holder.children[index]!==node)holder.append(node);});
  syncLayerDisplay();paintPhoto();trackDocument(session);
}
function syncLayerDisplay(){
  if(!session)return;const ratio=pixelRatio();
  for(const node of $('layer-stack').children){
    const layer=session.layers.find(item=>item.id===node.dataset.layerId);if(!layer)continue;
    node.hidden=!layer.visible||!!layer.discarded;
    node.style.left=(layer.x||0)/ratio+'px';node.style.top=(layer.y||0)/ratio+'px';
    node.style.width=(layer.width||node.naturalWidth||node.width)/ratio+'px';
    node.style.height=(layer.height||node.naturalHeight||node.height)/ratio+'px';
  }
}
function updateLayerRows(){
  for(const row of $('layers').children){
    const layer=session?.layers.find(item=>item.id===row.dataset.layerId);if(!layer)continue;
    row.classList.toggle('is-hidden',!layer.visible);row.hidden=!!layer.discarded;
    const button=row.children[0],label=(layer.visible?'Hide ':'Show ')+layer.name;
    button.setAttribute('aria-label',label);button.title=label;button.replaceChildren(svgIcon(layer.visible?'eye':'eye-off'));
  }
  $('layer-count').textContent=session.layers.filter(layer=>!layer.discarded).length+' edits';
  $('restore').hidden=!session.layers.some(layer=>layer.discarded);
}
function updateCollectionLabels(){
  updateCollectionSession();if(!collection)return;
  for(const button of $('filmstrip').children){const entry=collection.entries.find(item=>item.id===button.dataset.folderEntry);if(!entry)continue;
    button.title=entry.name+' — '+collectionEntryState(entry);
    const label=button.children[button.children.length-1];if(label?.children[1])label.children[1].textContent=collectionEntryState(entry);
  }
}
'''+s[end:]
replace("const item=document.createElement('div');item.className='layer'+(layer.visible?'':' is-hidden');", "const item=document.createElement('div');item.className='layer'+(layer.visible?'':' is-hidden');item.dataset.layerId=layer.id;")
replace("visibility.onclick=()=>changeLayer(layer.id,{visible:!layer.visible});", "visibility.onclick=()=>{const current=session.layers.find(item=>item.id===layer.id);changeLayer(layer.id,{visible:!current.visible});};")
start=s.index('async function changeLayer(id,change){')
end=s.index("$('restore').onclick",start)
s=s[:start]+'''function optimisticDocument(queue){
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
      queue.pending.shift();presentLayerQueue(queue);
      // A failed edit is rolled back; later requested states remain queued in order.
      if(session?.id===queue.confirmed.id)message('Layer update failed; that change was restored. '+error.message,true);
    }
  }
}
async function flushLayerChanges(){await Promise.all([...layerQueues.values()].map(queue=>queue.running).filter(Boolean));}
'''+s[end:]
replace("  if(!session||busy)return;\n  setBusy(true);message('Merging", "  if(!session||busy||layerChangesPending())return;\n  setBusy(true);message('Merging")
replace("    const images=await Promise.all([loadImage(prefix+'/preview?full=true&v='+data.revision),loadImage(prefix+'/preview?original=true&full=true'),state?.mask?loadImage(state.mask):Promise.resolve(null)]);", "    const assets=await displayAssets(data);\n    const images=await Promise.all([assets.base,state?.mask?loadImage(state.mask):Promise.resolve(null)]);")
replace("    previewImage=images[0];originalImage=images[1];showOriginal=false;brushPointer=null;", "    previewImage=images[0];originalImage=images[0];showOriginal=false;brushPointer=null;trackDocument(session);")
replace("restoreView(state,images[2]);paintPhoto();layerList();", "restoreView(state,images[1]);await refreshPreview();layerList();")
replace("    $('save-note').textContent=session.can_return?'Save Overwrite replaces the original file. Save Unique writes a separate copy beside it. Original and layers stay editable here.':'Browser upload copy. Export downloads a flattened image; original and layers stay editable here.';", "    $('save-note').textContent='File → Save editable project keeps the original and layers in a .lremove file. Image saves are flattened. Closing clears the working layers.';")
replace("  if(!session||busy||!hasSelection||!operationReady()||settingsSaving)return;", "  if(!session||busy||layerChangesPending()||!hasSelection||!operationReady()||settingsSaving)return;")
replace("  if(!session||busy||modalOpen())return;", "  if(!session||busy||layerChangesPending()||modalOpen())return;")
replace("    if(result.session)session=result.session;", "    if(result.session){session=result.session;trackDocument(session);}")
p.write_text(s,encoding='utf-8')
