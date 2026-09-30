/* Narrow document/native ports for the React Assets owner. There are no asset
 * controls, fetch wrappers, native listeners or global keyboard handlers here. */
(()=>{
  'use strict';
  if(!window.__LOCAL_IMAGE_REACT__||window.LocalImageAssetsBridge)return;
  let scheduler=null,lease=null,signature='';
  const listeners=new Set();
  function referenceState(){
    if(window.LocalImageGenerationBridge){const port=window.LocalImageGenerationBridge.referencePort();return{model:null,references:[],target:port.target,canReference:port.canReference};}
    const refining=!!window.LocalImageGenerationStudio?.isRefining();
    const model=refining?refineModel('draft'):generationModel();
    const references=refining?refineReferences:generationReferences;
    const mode=refining?'draft':window.LocalImageGenerationStudio?.mode?.()||'create';
    return{model,references,target:workspace==='generate'?mode+':'+(model?.id||''):null};
  }
  function context(){
    const generation=window.LocalImageGenerationBridge||window.LocalImageGenerationStudio,refs=referenceState(),cap=refs.model?.capabilities||{};
    const visible=!!session&&!generation?.isCreatingBlank?.()&&!generation?.isRefining?.();
    return{documentId:visible?session.id:null,revision:visible?session.revision:0,layerId:visible?window.LocalImageLayers?.selected()?.id||null:null,
      navigationEpoch:documentNavigationEpoch,busy:busy||closeInProgress||!!refineJob||!!generation?.isModeBusy?.(),nativeReady,
      canReference:refs.canReference??(workspace==='generate'&&!!(cap.image_reference||cap.image_to_image||cap.references)&&refs.references.length<(cap.max_references||0)),
      referenceTarget:refs.target,canUseDraft:!refineJob&&!closeInProgress};
  }
  function publish(){const next=JSON.stringify(context());if(next===signature)return;signature=next;for(const listener of [...listeners])listener();}
  window.LocalImageLegacyEditor.subscribe(publish);
  function matches(captured){const current=context();return captured.navigationEpoch===current.navigationEpoch&&captured.documentId===current.documentId;}
  async function runDocumentChange(work){
    if(!scheduler)throw Error('The shared document operation queue is unavailable.');
    const requested=context(),key=requested.documentId||'assets-new-document';
    return scheduler(key,async()=>{
      await flushLayerChanges();
      if(!matches(requested))throw Error('The document changed before the asset operation started. Try the command on the current image.');
      if(busy||closeInProgress||modalOpen())throw Error('Finish the current editor operation first.');
      const captured=context(),owned={epoch:documentNavigationEpoch,acceptedEpoch:null,acceptedId:null};lease=owned;setBusy(true);
      try{return await work(captured);}
      finally{if(lease===owned){lease=null;if(documentNavigationEpoch===owned.epoch||(documentNavigationEpoch===owned.acceptedEpoch&&session?.id===owned.acceptedId))setBusy(false);publish();}}
    });
  }
  async function acceptDocument(data,captured,destination){
    if(!data?.id||!Number.isInteger(data.revision))throw Error('The asset response did not contain a valid document.');
    if(destination==='background'&&(data.id!==captured.documentId||data.revision<captured.revision))throw Error('The background result belongs to a different document or revision.');
    trackDocument(data);
    if(!matches(captured)||(session?.id===data.id&&session.revision>data.revision))return false;
    if($('refine-dialog')?.open)$('refine-dialog').close();
    await openSession(data,{expectedNavigationEpoch:captured.navigationEpoch});
    if(session?.id!==data.id||session.revision<data.revision)return false;
    if(lease){lease.acceptedEpoch=documentNavigationEpoch;lease.acceptedId=session.id;setBusy(true);}
    setWorkspace(destination==='background'?'cutout':destination==='generated'?'generate':'retouch',{allowBusy:true});
    publish();return session?.id===data.id;
  }
  async function addReference(data,captured){
    const current=context();if(!matches(captured)||!current.canReference||current.referenceTarget!==captured.referenceTarget)return false;
    if(window.LocalImageGenerationBridge)return window.LocalImageGenerationBridge.addReference(data,captured);
    const refs=referenceState();
    if(refs.target?.startsWith('draft:')){if(!refineReferences.some(item=>item.id===data.id))refineReferences.push({id:data.id,name:data.name,thumbnail:sessionPreview(data)});renderRefineReferences();updateRefineControls();}
    else{addGenerationReference(data);selectGenerationTab('reference');}
    publish();return true;
  }
  async function useAsDraft(data,captured){
    if(!matches(captured)||refineJob)return false;
    if(window.LocalImageGenerationBridge){const accepted=await window.LocalImageGenerationBridge.useAsDraft(data);if(accepted&&lease){lease.acceptedEpoch=documentNavigationEpoch;lease.acceptedId=session?.id;}publish();return accepted;}
    if(!$('refine-dialog')?.open)await openRefineWorkspace({allowBusy:true});
    if(!$('refine-dialog')?.open||documentNavigationEpoch!==captured.navigationEpoch)return false;
    addRefineDraft(data);refineStatus('Library working copy selected as the draft.');publish();return true;
  }
  window.LocalImageAssetsBridge=Object.freeze({getContext:context,subscribeContext:listener=>{listeners.add(listener);return()=>listeners.delete(listener);},
    setScheduler:value=>{scheduler=value;},runDocumentChange,acceptDocument,addReference,useAsDraft,ownsOperation:()=>!!lease,
    chooseBackgroundFolder:async()=>{if(!nativeReady)throw Error('Use the browser folder picker in Assets.');return await nativeRequest('chooseBackgroundFolder');},
    // The existing viewport ResizeObserver preserves fit/pan when docks change.
    setDockVisible:visible=>{document.body.dataset.assetsOpen=String(visible);const mount=$('studio-assets');if(mount)mount.hidden=!visible;if(!visible)viewport.focus({preventScroll:true});},publish});
  publish();
})();
