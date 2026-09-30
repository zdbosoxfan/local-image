/* Visible generation views and accepted document handoff. React owns every
 * generation input/result; the existing canvas and document lifecycle persist. */
(()=>{
  'use strict';
  if(!window.__LOCAL_IMAGE_REACT__||window.LocalImageGenerationBridge)return;
  let feature=null,scheduler=null,mode='create',changing=false,internalNavigation=0,operation=null,commandPending=false,pendingDocument=null;
  let signature='';const listeners=new Set();
  const snapshot=()=>feature?.getSnapshot();
  const selected=()=>feature?.selectedResult()||feature?.selectedDraft()||null;
  function hidden(){return workspace==='generate'&&(mode==='refine'||mode==='create'&&!snapshot()?.modeDocuments.create);}
  function visibleDocument(){if(workspace!=='generate')return session;if(mode==='refine')return selected()?.session||null;if(mode==='create')return snapshot()?.modeDocuments.create||null;return session;}
  function context(){return{document:session?cloneDocument(session):null,navigationEpoch:documentNavigationEpoch,busy:busy||closeInProgress||changing||commandPending};}
  function renderView(){
    const active=workspace==='generate';document.body.dataset.generationView=active?mode:'editor';document.body.dataset.generationBlank=String(active&&mode==='create'&&!snapshot()?.modeDocuments.create);
    const mount=$('react-generation-root');if(mount)mount.hidden=!active;
    const blank=$('react-generation-empty');if(blank)blank.hidden=!(active&&mode==='create'&&!snapshot()?.modeDocuments.create);
  }
  function publish(){renderView();const next=JSON.stringify({id:session?.id,revision:session?.revision,epoch:documentNavigationEpoch,busy:busy||closeInProgress||changing||commandPending,mode,workspace});if(next!==signature){signature=next;for(const listener of [...listeners])listener();}publishEditorState();window.LocalImageAssetsBridge?.publish();}
  window.LocalImageLegacyEditor.subscribe(()=>{const next=JSON.stringify({id:session?.id,revision:session?.revision,epoch:documentNavigationEpoch,busy:busy||closeInProgress||changing||commandPending,mode,workspace});if(next!==signature){signature=next;for(const listener of [...listeners])listener();}});
  function matches(captured){return captured.navigationEpoch===documentNavigationEpoch&&captured.document?.id===session?.id;}
  async function runGeneration(work){
    if(!scheduler)throw Error('The shared document operation queue is unavailable.');
    const requested=context();
    return scheduler(requested.document?.id||'generation-new-document',async()=>{
      await flushLayerChanges();if(!matches(requested))throw Error('The document changed before generation started.');
      if(busy||closeInProgress||modalOpen())throw Error('Finish the current editor operation first.');
      const captured=context(),lease={epoch:documentNavigationEpoch,acceptedEpoch:null,acceptedId:null};operation=lease;activeTask='generate';setBusy(true);publish();
      try{return await work(captured);}
      finally{if(operation===lease){operation=null;activeTask=null;if(documentNavigationEpoch===lease.epoch||(documentNavigationEpoch===lease.acceptedEpoch&&session?.id===lease.acceptedId))setBusy(false);publish();}}
    });
  }
  async function navigate(data,captured){
    trackDocument(data);if(!matches(captured))return false;
    internalNavigation++;
    try{await openSession(data,{expectedNavigationEpoch:captured.navigationEpoch});}
    finally{internalNavigation--;}
    const accepted=session?.id===data.id&&session.revision>=data.revision;
    if(accepted&&operation){operation.acceptedEpoch=documentNavigationEpoch;operation.acceptedId=session.id;setBusy(true);}
    return accepted;
  }
  async function activateMode(value,data){
    if(changing||closeInProgress||(busy&&!operation&&!internalNavigation&&!window.LocalImageAssetsBridge?.ownsOperation()))return false;
    changing=true;const before=mode;rememberCurrentView();resetTransientInput();
    try{
      if(data&&(session?.id!==data.id||session.revision<data.revision)){const captured=context();if(!await navigate(data,captured))return false;}
      mode=value;documentNavigationEpoch++;setWorkspace('generate',{allowBusy:true});renderView();controls();publish();return true;
    }catch(error){mode=before;message(error.message,true);return false;}
    finally{changing=false;publish();}
  }
  async function acceptResult(data,captured,value){
    if(!matches(captured))return false;
    if(!await navigate(data,captured))return false;
    mode=value==='refine'?'edit':value;changing=true;
    try{setWorkspace('generate',{allowBusy:true});renderView();controls();publish();return true;}
    finally{changing=false;publish();}
  }
  async function enterWorkspace(){if(workspace==='generate'||changing||busy||!feature)return;return feature.setMode(session?'edit':'create',session||undefined);}
  async function afterDocumentOpened(data){
    if(internalNavigation||!feature)return;
    if(feature.getSnapshot().working){pendingDocument={data,epoch:documentNavigationEpoch};publish();return;}
    if(workspace==='generate'||data.generation||data.upscale){
      internalNavigation++;
      try{if(data.generation&&feature.getSnapshot().modeDocuments.edit?.id!==data.id)feature.restoreDocument('edit',data);await feature.setMode('edit',data);}
      finally{internalNavigation--;}
    }
    publish();
  }
  function applyCommandAvailability(){
    if(workspace!=='generate')return;
    const available=!!visibleDocument()&&!busy&&!changing&&!commandPending;
    for(const id of ['clear','undo','redo','cutout-undo','cutout-redo','add','subtract','before','merge','size','finish','remove','heal-brush','restore'])if($(id))$(id).disabled=true;
    for(const id of ['save','return','save-unique','save-project','save-project-as','close-image','image-credits'])if($(id))$(id).disabled=!available;
    if(hidden())for(const id of ['zoom','zoom-in','zoom-out','fit','actual-size','hand'])if($(id))$(id).disabled=true;
    if(hidden())for(const id of ['return','save-unique','document-save','document-unique'])if($(id))$(id).hidden=true;
    for(const command of document.querySelectorAll('[data-command]')){const source=$(command.dataset.command);if(source)command.disabled=source.disabled;}
  }
  async function prepareDocumentCommand(kind){
    if(workspace!=='generate'||!hidden())return{allowed:true,forceExport:false};
    if(busy||changing||commandPending||closeInProgress)return{allowed:false,forceExport:false};
    const item=mode==='refine'?selected():null;
    if(!item){message('Generate or select an image before '+(kind==='close'?'closing an image':'saving or exporting')+'. The other document is preserved.');return{allowed:false,forceExport:false};}
    commandPending=true;
    try{const accepted=await acceptResult(item.session,context(),'edit');if(accepted)await feature.setMode('edit',item.session);return{allowed:accepted,forceExport:accepted};}
    finally{commandPending=false;publish();}
  }
  function referencePort(){
    const state=snapshot();if(!state)return{key:'create',target:null,canReference:false};
    const key=mode==='refine'?'draft':mode,model=feature.modelFor(key),refs=feature.referencesFor(key),cap=model?.capabilities||{};
    return{key,target:key+':'+(model?.id||''),canReference:workspace==='generate'&&!!(cap.image_reference||cap.image_to_image||cap.references)&&refs.length<(cap.max_references||0)};
  }
  window.LocalImageGenerationBridge=Object.freeze({getContext:context,subscribeContext:listener=>{listeners.add(listener);return()=>listeners.delete(listener);},setScheduler:value=>{scheduler=value;},
    bindController:value=>{feature=value;feature.subscribe(()=>{if(pendingDocument&&!feature.getSnapshot().working){const pending=pendingDocument;pendingDocument=null;if(pending.epoch===documentNavigationEpoch&&pending.data.id===session?.id){if(pending.data.generation&&feature.getSnapshot().modeDocuments.edit?.id!==pending.data.id)feature.restoreDocument('edit',pending.data);void feature.setMode('edit',session);}}renderView();applyCommandAvailability();renderHealth();publishEditorState();window.LocalImageAssetsBridge?.publish();});renderView();},
    runGeneration,acceptResult,activateMode,enterWorkspace,afterDocumentOpened,applyCommandAvailability,prepareDocumentCommand,
    ownsWorkspaceChange:()=>changing||!!operation||internalNavigation>0,ownsOperation:()=>!!operation,isModeBusy:()=>changing||commandPending,
    isRefining:()=>workspace==='generate'&&mode==='refine',isCreatingBlank:()=>workspace==='generate'&&mode==='create'&&!snapshot()?.modeDocuments.create,
    hasHiddenEditor:hidden,hasVisibleDocument:()=>!!visibleDocument(),prepareSelectedForExport:async()=>!!(await prepareDocumentCommand('save')).allowed,
    referencePort,addReference:(data,captured)=>{const port=referencePort();return captured.referenceTarget===port.target&&port.canReference&&feature.addReference(port.key,data);},
    useAsDraft:async data=>{feature.addDraft(data);return await feature.setMode('refine');},
    noteClosed:ids=>{feature?.forgetDocuments(ids);renderView();},
    mode:()=>mode,activeDraftKey:()=>mode==='refine'?'draft':mode,publish});
})();
