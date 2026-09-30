/* Temporary, explicit boundary for the first React slice. This file neither
 * remounts the canvas nor adds native, pointer, or keyboard event listeners.
 * Python session responses remain the persisted document authority. */
(()=>{
  'use strict';
  if(!window.__LOCAL_IMAGE_REACT__||window.LocalImageLegacyEditor)return;
  const listeners=new Set();
  let current=null,sourceDocument,sourceVersion='',acceptedDocument=null;
  function freeze(value){
    if(value&&typeof value==='object'&&!Object.isFrozen(value)){for(const child of Object.values(value))freeze(child);Object.freeze(value);}
    return value;
  }
  // Legacy save/dirty metadata may update without replacing the document.
  // Compare its scalar fields; document pixels and pointer positions never
  // enter this snapshot, and unchanged gestures do not clone the document.
  function documentVersion(data){return data?JSON.stringify(Object.entries(data).filter(([,value])=>value===null||typeof value!=='object')):'';}
  function historyAvailable(){
    const generation=window.LocalImageGenerationStudio;
    return !!session&&!busy&&!closeInProgress&&!layerChangesPending()&&workspace!=='generate'&&!generation?.isRefining()&&!generation?.isCreatingBlank?.()&&!modalOpen();
  }
  function getSnapshot(){
    const version=documentVersion(session);
    if(sourceDocument!==session||sourceVersion!==version){sourceDocument=session;sourceVersion=version;acceptedDocument=session?freeze(cloneDocument(session)):null;}
    const generation=window.LocalImageGenerationStudio;
    const next={document:acceptedDocument,selectedLayerId:window.LocalImageLayers?.selected()?.id||null,
      busy:busy||closeInProgress,workspace,showOriginal,tool,
      canUndo:historyAvailable()&&!!historyTarget(),
      canRedo:historyAvailable()&&!!historyTarget(true),
      status:$('message').textContent,statusError:$('message').classList.contains('error'),
      nativeReady,nativeProjects,selectionActive:hasSelection||points.length>0,inspectorHidden:document.body.dataset.inspectorHidden==='true',
      refining:!!generation?.isRefining(),creatingBlank:!!generation?.isCreatingBlank?.(),
      generationVisible:generation?.hasVisibleDocument?generation.hasVisibleDocument():!!session};
    if(!current||Object.keys(next).some(key=>next[key]!==current[key]))current=Object.freeze(next);
    return current;
  }
  function publish(){
    const previous=current,next=getSnapshot();
    if(next!==previous)for(const listener of [...listeners])listener();
  }
  async function exportImage(){
    if(busy)return;
    closeMenus();
    try{
      const generation=window.LocalImageGenerationStudio;
      if(generation?.prepareSelectedForExport){if(!await generation.prepareSelectedForExport())return;}
      else if(generation?.isRefining()){if(!await generation.openSelectedInEditor())return;}
      if(workspace==='cutout'&&!hasSelection&&!points.length){outputFormat='png';$('output-format').value='png';updateDocumentState();return save('export',true);}
      return save('export');
    }catch(error){message(error.message,true);}
  }
  const saveProject=(saveAs=false)=>{if(!busy&&!modalOpen())return saveEditableProject(session,{saveAs});};
  const canvas=Object.freeze({element:viewport,fit:()=>{endGesture();fitImage();},actualSize:()=>setPhotoZoom(1),
    focus:()=>viewport.focus({preventScroll:true}),cancelGesture:()=>{resetTransientInput();window.LocalImageLayers.cancelMove();}});
  const native=Object.freeze({capabilities:()=>Object.freeze({ready:nativeReady,projects:nativeProjects,setup:nativeSetup}),
    openFiles:openDocumentFiles,openFolder:openDocumentFolder,openProject:openDocumentProject,saveProject});
  const commands=Object.freeze({
    selectLayer:id=>window.LocalImageLayers.select(id),createRetouch:()=>window.LocalImageLayers.createRetouch(),
    patchLayer:(id,change)=>window.LocalImageLayers.patchLayer(id,change),reorderLayer:direction=>window.LocalImageLayers.reorderSelected(direction),
    restoreLayer:()=>window.LocalImageLayers.restoreLayer(),mergeLayers:()=>window.LocalImageLayers.mergeLayers(),addMask:()=>window.LocalImageLayers.addMask(),
    undo:()=>{if(historyAvailable()&&historyTarget())return undoEdit();},redo:()=>{if(historyAvailable()&&historyTarget(true))return redoEdit();},fit:canvas.fit,actualSize:canvas.actualSize,selectTool:value=>selectTool(value),
    setWorkspace:value=>setWorkspace(value),toggleOriginal:()=>{if(!session||busy)return;endGesture();showOriginal=!showOriginal;paintPhoto();},
    openFiles:openDocumentFiles,openFolder:openDocumentFolder,openProject:openDocumentProject,
    saveProject:()=>saveProject(),saveProjectAs:()=>saveProject(true),exportImage,showSettings,
    showAssets:()=>window.LocalImageLayers.showAssets('stock'),
    toggleInspector:()=>{if(window.LocalImageGenerationStudio?.isRefining())return;document.body.dataset.inspectorHidden=String(document.body.dataset.inspectorHidden!=='true');window.LocalImageStudio?.sync();publish();}
  });
  window.LocalImageLegacyEditor=Object.freeze({getSnapshot,subscribe:listener=>{listeners.add(listener);return()=>listeners.delete(listener);},
    commands,canvas,native,publish,setStackTransport:transport=>window.LocalImageLayers.setStackTransport(transport)});
  getSnapshot();
})();
