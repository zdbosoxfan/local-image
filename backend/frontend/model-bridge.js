/* Fixed model/adapter operations share the existing correlated native bridge.
 * No URLs, output paths, generic filesystem methods or second listener. */
(()=>{
  'use strict';
  if(!window.__LOCAL_IMAGE_REACT__||window.LocalImageModelBridge)return;
  const request=(action,details={})=>nativeSetup?nativeRequest(action,null,details):Promise.reject(Error('Model setup requires the current Local Image desktop host.'));
  // These ports target the still-visible generation/refinement draft controls.
  // They are retired with that region; no duplicate or hidden controls exist.
  function getLegacyLoraPort(context='generate'){
    if(!['generate','draft','final'].includes(context))throw Error('Unknown adapter draft.');
    const prefix=context==='generate'?'gen-':'refine-'+context+'-';
    const selected=()=>context==='generate'?generationLoras:refineLoras[context];
    const refresh=()=>{if(context==='generate'){renderSelectedLoras();updateGenerationControls();}else{renderRefineLoras(context);updateRefineControls();}publishEditorState();};
    return{
      get contextId(){return context==='generate'?(window.LocalImageGenerationStudio?.mode?.()||'generate'):context;},
      read:()=>{const id=context==='generate'?generationModelId:$(prefix+'model').value,model=generationModels.find(item=>item.id===id);return{modelId:id,modelLabel:model?.label||id,selected:cloneDocument(selected()),referenceCount:context==='generate'?generationReferences.length:context==='final'?refinementReferences().length:refineReferences.length,supportsLoras:model?.capabilities?.lora!==false&&model?.capabilities?.loras!==false};},
      setSelected:value=>{if(context==='generate')generationLoras=cloneDocument(value);else refineLoras[context]=cloneDocument(value);refresh();},
      appendPrompt:phrase=>{const input=$(prefix+'prompt'),previous=input.value.trim();input.value=(previous?previous+'\n':'')+phrase;refresh();},
      applySampling:settings=>{for(const key of ['steps','guidance'])if(settings[key]!==undefined)$(prefix+key).value=String(settings[key]);refresh();},
    };
  }
  function acceptCatalog(value){
    const previous=generationModels.find(item=>item.id===generationModelId);generationModels=cloneDocument(value);
    if(previous?.historical&&!generationModels.some(item=>item.id===previous.id))generationModels.push(previous);
    if(!generationModels.some(item=>item.id===generationModelId)){
      if(session?.generation?.model===generationModelId)generationModels.push(historicalGenerationModel(session.generation));
      else generationModelId=generationModels[0]?.id||generationModelId;
    }
    renderGenerationModelOptions();syncGenerationModel();
    if(refineInitialized)for(const stage of ['draft','final']){fillRefineModels(stage);syncRefineModel(stage,{reset:false});}
    renderHealth();publishEditorState();
  }
  window.LocalImageModelBridge=Object.freeze({
    capabilities:()=>({setup:nativeSetup}),editorBusy:()=>busy||closeInProgress,
    focusCanvas:()=>{resetTransientInput();viewport.focus({preventScroll:true});},
    chooseModelDirectory:()=>request('setupChooseModelDirectory'),
    startBackend:()=>request('setupStart'),
    downloadModel:(model,variant)=>request('setupDownloadGenerationModel',{model,variant}),
    downloadLora:payload=>request('loraDownload',{model:payload.model,repo_id:payload.repo_id,filename:payload.filename,revision:payload.revision,allow_unverified:payload.allow_unverified===true}),
    getLegacyLoraPort,acceptCatalog,
    legacyModelSelection:()=>({selectedModelId:generationModelId,selectedVariant:$('gen-variant').value,onUse:chooseGenerationModel}),
  });
})();
