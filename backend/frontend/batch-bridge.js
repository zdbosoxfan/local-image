/* Explicit editor/collection ports for the React batch feature. Queue state and
 * UI drafts live in its typed controller; this adapter owns no DOM controls and
 * installs no listeners or native request machinery. */
(()=>{
  'use strict';
  if(!window.__LOCAL_IMAGE_REACT__)return;
  const pendingVersions=new Map();
  let nextPendingVersion=0;
  const entries=()=>collection?.entries||(session?[{id:session.id,name:session.name,session_id:session.id}]:[]);
  function fingerprint(id){
    const view=viewStates.get(id),mask=view?.mask||'',pointsKey=JSON.stringify(view?.points||[]);
    const previous=pendingVersions.get(id);
    if(!previous||previous.mask!==mask||previous.points!==pointsKey){
      pendingVersions.set(id,{mask,points:pointsKey,version:++nextPendingVersion});
    }
    return String(pendingVersions.get(id).version);
  }
  function getSnapshot(){
    const source=entries(),pending=[];
    for(const entry of source){
      if(entry.session_id&&pendingSelection(entry.session_id))pending.push({sessionId:entry.session_id,name:entry.name,fingerprint:fingerprint(entry.session_id)});
    }
    if(session&&pendingSelection(session.id)&&!pending.some(item=>item.sessionId===session.id))pending.push({sessionId:session.id,name:session.name,fingerprint:fingerprint(session.id)});
    for(const id of pendingVersions.keys())if(!openDocuments.has(id)&&session?.id!==id)pendingVersions.delete(id);
    return {busy:busy||closeInProgress,document:session?{id:session.id,name:session.name,revision:session.revision,canSaveTreatment:!!session.cutout?.enabled}:null,
      collectionId:collection?.id||null,nativeCollection:!!collection&&!collection.local,
      entries:source.map(entry=>({id:entry.id,name:entry.name,sessionId:entry.session_id||null})),
      pendingSelections:pending,nativeExportAvailable:nativeReady&&nativeBatch};
  }
  window.LocalImageBatchHost=Object.freeze({
    getSnapshot,
    subscribe:listener=>window.LocalImageLegacyEditor.subscribe(listener),
    prepareForBatch:async()=>{if(busy||closeInProgress)throw Error('Finish the current operation before opening Batch.');closeMenus();rememberCurrentView();await flushLayerChanges();publishEditorState();},
    resolveSession:async entryId=>{
      const sourceCollection=collection,entry=entries().find(item=>item.id===entryId);
      if(!entry)throw Error('The selected image is no longer in this collection.');
      let current;
      if(entry.session_id)current=await(await api('/api/local-remove/session/'+encodeURIComponent(entry.session_id))).json();
      else{
        if(!entry.file)throw Error('Open this photo before preparing its batch.');
        const form=new FormData();form.append('file',entry.file);
        current=await(await api('/api/local-remove/import',{method:'POST',body:form})).json();
        if(collection!==sourceCollection)throw Error('The image collection changed during preparation.');
        entry.session_id=current.id;trackDocument(current);renderCollection();publishEditorState();
      }
      return {session_id:current.id,revision:current.revision};
    },
    returnToPendingSelection:async id=>{
      if(id!==session?.id){const index=collection?.entries.findIndex(entry=>entry.session_id===id)??-1;
        if(index>=0)await openCollectionEntry(index);else{const current=openDocuments.get(id);if(!current)throw Error('The document is no longer open.');await openSession(current);}}
      message('Apply the pending selection, then reopen Batch. Your selection is kept.');viewport.focus({preventScroll:true});
    },
    exportBatchFolder:async request=>{
      if(!nativeReady||!nativeBatch)throw Error('This desktop host does not support batch folder export.');
      // Only backend queue/item IDs cross this boundary; the native owner picks
      // the path and cancellation remains null, with no dialog deadline.
      return nativeRequest('batchExportFolder',null,{job_id:request.job_id,item_ids:[...request.item_ids]});
    }
  });
})();
