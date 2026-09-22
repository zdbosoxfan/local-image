function hasWorkingLayers(data){return !!data?.layers?.length;}
function pendingSelection(id){
  if(session?.id===id)return hasSelection||points.length>0;
  const state=viewStates.get(id);return !!(state?.hasSelection||state?.points?.length);
}
function projectNeedsSave(data){return !data.project_saved||data.project_saved_revision!==data.revision||data.project_dirty===true;}
function downloadFile(result){const link=document.createElement('a');link.href=result.download;link.download=result.name;link.click();}
async function saveEditableProject(data=session,{forClose=false}={}){
  if(!data)return false;
  await flushLayerChanges();data=openDocuments.get(data.id)||data;
  setBusy(true);message('Saving editable project…');
  try{
    const result=nativeProjects
      ?await nativeRequest('saveProject',null,{session_id:data.id,revision:data.revision})
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
