/* The document is a stack of independently selectable image, cutout and retouch
 * layers. A retouch layer may own many repair patches. All mutations go through
 * the revision-checked backend stack; the canvas never invents saved pixels. */
(()=>{
  'use strict';
  if(window.LocalImageLayers)return;
  const byId=id=>document.getElementById(id),selectedIds=new Map(),assetCache=new Map();
  let moving=null,renderKey='',syncing=false,assetTab='stock';
  const nodes=()=>Array.isArray(session?.layer_stack)?session.layer_stack:session?.layer_stack?.layers||[];
  const stacked=()=>!!session?.layer_stack;
  // Generate may retain a hidden source document while displaying an unrelated
  // draft/result. Layer commands belong only to the two editing workspaces.
  const visibleDocument=()=>workspace!=='generate';
  const selected=()=>nodes().find(node=>node.id===selectedIds.get(session?.id)&&!node.discarded)||[...nodes()].reverse().find(node=>!node.discarded)||null;
  const transform=node=>({offset_x:0,offset_y:0,scale:1,rotation:0,...node?.transform});
  const endpoint=tail=>'/api/local-remove/session/'+encodeURIComponent(session.id)+tail;
  const iconButton=(id,label,icon,handler)=>{const button=document.createElement('button');button.type='button';button.id=id;button.setAttribute('aria-label',label);button.title=label;button.append(svgIcon(icon));button.onclick=handler;return button;};
  const textButton=(id,label,handler)=>{const button=document.createElement('button');button.type='button';button.id=id;button.textContent=label;button.onclick=handler;return button;};
  const panel=byId('retouch-panel'),holder=byId('layers');
  document.body.dataset.layerStudio='true';
  holder.setAttribute('role','listbox');holder.setAttribute('aria-label','Document layers');
  holder.setAttribute('aria-multiselectable','false');
  const heading=panel.querySelector('.panel-heading');
  const add=iconButton('layer-add','New retouch layer','plus',()=>createRetouch());
  add.textContent='+';add.className='panel-menu-trigger';heading.insertBefore(add,byId('layers-options'));
  const footer=document.createElement('div');footer.className='stack-properties';
  footer.innerHTML='<label for="layer-opacity">Opacity</label><input id="layer-opacity" type="number" min="0" max="100" step="1" value="100" aria-label="Layer opacity percent"><span>%</span>';
  const more=iconButton('layer-actions','Selected layer actions','settings',()=>openPopup(layerMenu,more));more.textContent='\u2026';footer.append(more);panel.append(footer);
  const layerMenu=document.createElement('div');layerMenu.id='stack-layer-menu';layerMenu.className='stack-popup';layerMenu.setAttribute('popover','auto');layerMenu.setAttribute('role','menu');document.body.append(layerMenu);
  const actions={};
  for(const [id,label,handler]of[
    ['mask','Add editable mask',()=>addMask()],['rename','Rename layer',()=>renameSelected()],['lock','Lock layer',()=>patchSelected({locked:!selected()?.locked})],
    ['up','Move layer up',()=>reorderSelected(1)],['down','Move layer down',()=>reorderSelected(-1)],
    ['reset','Reset transform',()=>patchSelected({transform:{offset_x:0,offset_y:0,scale:1,rotation:0}})],
    ['delete','Delete layer',()=>patchSelected({discarded:true})]
  ]){const button=textButton('stack-'+id,label,()=>{closePopup(layerMenu);handler();});button.setAttribute('role','menuitem');layerMenu.append(button);actions[id]=button;}
  // The app Layer menu and the dock menu dispatch to the same commands.
  const menuNew=document.createElement('button');menuNew.setAttribute('role','menuitem');menuNew.dataset.command='layer-add';menuNew.textContent='New retouch layer';menuNew.onclick=()=>{closeMenus();add.click();};
  const mainLayerMenu=byId('layer-menu');mainLayerMenu.prepend(menuNew);
  for(const [id,label]of [['mask','Add editable mask'],['rename','Rename layer'],['lock','Lock / unlock layer'],['up','Move layer up'],['down','Move layer down'],['delete','Delete layer']]){
    const item=document.createElement('button');item.setAttribute('role','menuitem');item.dataset.command='stack-'+id;item.textContent=label;item.onclick=()=>{closeMenus();actions[id].click();};mainLayerMenu.append(item);
  }
  const toolbar=document.createElement('div');toolbar.id='stack-cutout-actions';toolbar.className='stack-context-actions';
  const remove=byId('cutout-remove');remove.classList.remove('full-width');toolbar.append(remove);
  const options=iconButton('stack-cutout-settings','Cutout model options','settings',()=>openPopup(cutoutSettings,options));toolbar.append(options);
  const background=textButton('stack-background','Add background',()=>openPopup(backgroundMenu,background));background.setAttribute('aria-haspopup','menu');toolbar.append(background);
  document.querySelector('.optionsbar').prepend(toolbar);
  const cutoutSettings=document.createElement('div');cutoutSettings.id='stack-cutout-settings-panel';cutoutSettings.className='stack-popup stack-settings-popup';cutoutSettings.setAttribute('popover','auto');
  cutoutSettings.append(byId('qwen-variant'),byId('qwen-state'),byId('qwen-download').closest('details'));document.body.append(cutoutSettings);
  const backgroundMenu=document.createElement('div');backgroundMenu.id='stack-background-menu';backgroundMenu.className='stack-popup';backgroundMenu.setAttribute('popover','auto');backgroundMenu.setAttribute('role','menu');document.body.append(backgroundMenu);
  for(const [id,label,handler]of[
    ['import','Import image\u2026',()=>byId('background-file').click()],
    ['assets','Choose from Assets',()=>{stockOrigin='background';showAssets('stock');byId('stock-query').focus();}],
    ['generate','Generate background\u2026',()=>{generationDialog.showModal();byId('background-prompt').focus();}],
    ['folder','Attach backgrounds folder\u2026',()=>{showAssets('folders');byId('background-folder').onclick();}]
  ]){const button=textButton('stack-background-'+id,label,()=>{closePopup(backgroundMenu);handler();});button.setAttribute('role','menuitem');backgroundMenu.append(button);}
  const generationDialog=document.createElement('dialog');generationDialog.id='stack-background-dialog';generationDialog.className='stack-background-dialog';generationDialog.setAttribute('aria-labelledby','stack-background-title');
  generationDialog.innerHTML='<div class="dialog-header"><h2 id="stack-background-title">Generate background</h2><button id="stack-background-close" aria-label="Close background generator">\u00d7</button></div><label for="background-prompt">Describe the empty scene</label>';
  generationDialog.append(byId('background-prompt'));const negativeNote=document.createElement('p');negativeNote.className='cutout-note';negativeNote.textContent='Creates a separate layer. Empty-scene instructions exclude people, products and text.';generationDialog.append(negativeNote,byId('background-generate'));document.body.append(generationDialog);
  byId('stack-background-close').onclick=()=>generationDialog.close();
  generationDialog.addEventListener('keydown',event=>event.stopPropagation());
  const refinement=document.createElement('details');refinement.id='stack-mask-options';refinement.className='stack-context-details';
  const summary=document.createElement('summary');summary.textContent='Edge options';refinement.append(summary);
  const refineContent=document.createElement('div');refineContent.className='stack-mask-popover';refineContent.append(byId('cutout-feather').closest('label'),byId('shadow-disclosure'));refinement.append(refineContent);toolbar.append(refinement);
  const moveControls=document.createElement('div');moveControls.id='stack-move-controls';moveControls.className='stack-context-actions';
  for(const [id,label,field,min,max,step]of[['x','X','offset_x',-100000,100000,1],['y','Y','offset_y',-100000,100000,1],['scale','Scale','scale',5,400,.1],['rotation','Angle','rotation',-180,180,.1]]){
    const labelNode=document.createElement('label');labelNode.textContent=label;const input=document.createElement('input');input.type='number';input.id='stack-transform-'+id;input.min=String(min);input.max=String(max);input.step=String(step);input.setAttribute('aria-label','Layer '+label);labelNode.append(input);if(id==='scale')labelNode.append('%');if(id==='rotation')labelNode.append('\u00b0');moveControls.append(labelNode);
    input.onchange=()=>{if(input.value!==''&&input.checkValidity())patchSelected({transform:{[field]:Number(input.value)*(id==='scale'?.01:1)}});else{message('Enter a valid '+label.toLowerCase()+' value.',true);sync();}};
  }
  document.querySelector('.optionsbar').append(moveControls);
  // Local folder images use the same compact Assets area as online stock.
  const folders=document.createElement('section');folders.id='stack-background-folders';folders.className='stack-background-folders';folders.hidden=true;
  const attach=textButton('stack-attach-folder','Attach folder\u2026',()=>byId('background-folder').onclick());folders.append(attach,byId('background-library'),byId('background-grid'));
  const folderEmpty=document.createElement('p');folderEmpty.id='stack-folder-empty';folderEmpty.className='cutout-note';folderEmpty.textContent='Attach a folder to browse your own backgrounds here.';folders.append(folderEmpty);byId('studio-assets').append(folders);

  function openPopup(popup,anchor){
    if(anchor.disabled)return;closeMenus();if(popup.matches(':popover-open')){popup.hidePopover();return;}popup.showPopover();
    const rect=anchor.getBoundingClientRect();popup.style.left=Math.max(8,Math.min(innerWidth-popup.offsetWidth-8,rect.left))+'px';popup.style.top=Math.max(8,Math.min(innerHeight-popup.offsetHeight-8,rect.bottom+6))+'px';
    popup.querySelector('button:not(:disabled),select:not(:disabled),input:not(:disabled)')?.focus();
  }
  function closePopup(popup){if(popup.matches(':popover-open'))popup.hidePopover();}
  for(const popup of [layerMenu,backgroundMenu,cutoutSettings])popup.addEventListener('keydown',event=>{
    event.stopPropagation();if(event.key==='Escape'){event.preventDefault();closePopup(popup);return;}
    const buttons=[...popup.querySelectorAll('button:not(:disabled)')],index=buttons.indexOf(document.activeElement);if(['ArrowDown','ArrowUp','Home','End'].includes(event.key)&&index>=0){event.preventDefault();buttons[event.key==='Home'?0:event.key==='End'?buttons.length-1:(index+(event.key==='ArrowDown'?1:buttons.length-1))%buttons.length]?.focus();}
  });
  function setAssetTab(tab){
    assetTab=tab;byId('studio-assets').dataset.assetTab=tab;folders.hidden=tab!=='folders';
    byId('stock-dialog').hidden=tab!=='stock';byId('stock-expand').hidden=tab!=='stock';
    byId('studio-assets-stock').setAttribute('aria-pressed',String(tab==='stock'));byId('studio-assets-folders').setAttribute('aria-pressed',String(tab==='folders'));
  }
  function showAssets(tab){window.LocalImageStockStudio?.open();setAssetTab(tab);}
  const previousStockOpen=openStockLibrary;
  openStockLibrary=function(...args){setAssetTab('stock');return previousStockOpen.apply(this,args);};
  // Capture avoids the former Folders tab automatically opening the chooser.
  byId('studio-assets-folders').addEventListener('click',event=>{event.stopImmediatePropagation();if(!busy)showAssets('folders');},true);
  byId('studio-assets-stock').addEventListener('click',()=>showAssets('stock'),true);
  byId('studio-stock-open').addEventListener('click',()=>{if(assetTab==='folders')showAssets('folders');});

  function choose(id,{clear=true,focus=false}={}){
    const node=nodes().find(item=>item.id===id&&!item.discarded);if(!node||busy||!visibleDocument())return;
    if(clear&&selected()?.id!==id&&(hasSelection||points.length))clearSelection();
    selectedIds.set(session.id,id);renderKey='';layerList();syncCutoutFields();paintPhoto();
    if(focus)holder.querySelector('[data-stack-id="'+CSS.escape(id)+'"]')?.focus();
  }
  async function mutate(tail,body={},method='POST',{selectNew=false,label='Layer updated'}={}){
    if(!session||busy||!visibleDocument())return null;
    const sid=session.id,oldIds=new Set(nodes().map(node=>node.id));setBusy(true);
    try{
      const data=await json(endpoint(tail),{...body,revision:session.revision},method);trackDocument(data);
      if(session?.id!==sid)return data;session=data;
      if(selectNew){const created=[...(Array.isArray(data.layer_stack)?data.layer_stack:data.layer_stack?.layers||[])].reverse().find(node=>!oldIds.has(node.id));if(created)selectedIds.set(sid,created.id);}
      assetCache.clear();await refreshPreview();renderKey='';layerList();updateCollectionSession();renderCollection();message(label);return data;
    }catch(error){message(error.message,true);return null;}finally{setBusy(false);}
  }
  async function ensureStack(){
    if(!session||stacked())return !!session;
    const data=await mutate('/stack',{},'POST',{label:'Layer stack ready'});return !!data;
  }
  async function createRetouch(){
    if(!await ensureStack())return null;
    return mutate('/stack/layers',{kind:'retouch',name:'Retouch'},'POST',{selectNew:true,label:'Retouch layer selected. New repairs will collect here.'});
  }
  async function prepareRetouch(){
    if(!session||busy||!visibleDocument()||workspace!=='retouch')return false;
    if(!await ensureStack())return false;
    const layer=selected();
    if(layer?.kind!=='retouch'){if(!await createRetouch())return false;}
    const target=selected();if(!target||target.locked||!target.visible||target.discarded){message('Select an unlocked, visible retouch layer before repairing.',true);return false;}
    return true;
  }
  async function addMask(){
    if(!selected()||busy||!visibleDocument())return;
    const full=document.createElement('canvas');full.width=session.width;full.height=session.height;const context=full.getContext('2d');context.fillStyle='white';context.fillRect(0,0,full.width,full.height);
    await cutoutEdit('/cutout/refine',{operation:'replace',mask:full.toDataURL('image/png').split(',')[1]},'POST','Adding editable mask\u2026',true);
    if(selected()?.kind==='cutout'){setWorkspace('cutout');selectTool('brush');message('Editable mask added above the source. Erase or restore pixels on this layer.');}
  }
  const patchSelected=change=>selected()?mutate('/stack/layer/'+encodeURIComponent(selected().id),change,'PATCH'):Promise.resolve(null);
  function reorderSelected(direction){const layer=selected();if(layer)patchSelected({index:Math.max(0,Math.min(nodes().length-1,nodes().indexOf(layer)+direction))});}
  function renameSelected(){
    const layer=selected(),row=holder.querySelector('[data-stack-id="'+CSS.escape(layer?.id||'')+'"]'),name=row?.querySelector('.layer-name');if(!name)return;
    const input=document.createElement('input');input.className='stack-layer-rename';input.value=layer.name;input.maxLength=120;input.setAttribute('aria-label','Layer name');name.replaceWith(input);input.focus();input.select();
    let committed=false;const finish=async save=>{if(committed)return;committed=true;const next=input.value.trim();if(save&&next&&next!==layer.name)await patchSelected({name:next});renderKey='';layerList();};
    input.addEventListener('click',event=>event.stopPropagation());input.onblur=()=>finish(true);input.onkeydown=event=>{event.stopPropagation();if(event.key==='Enter'){event.preventDefault();finish(true);}if(event.key==='Escape'){event.preventDefault();finish(false);}};
  }
  byId('layer-opacity').onchange=()=>{const input=byId('layer-opacity');if(input.value!==''&&input.checkValidity())patchSelected({opacity:Number(input.value)/100});else sync();};

  const previousLayerList=layerList;
  layerList=function(...args){
    if(!stacked())return previousLayerList.apply(this,args);
    const current=selected();if(current)selectedIds.set(session.id,current.id);
    const signature=session.id+':'+session.revision+':'+current?.id;
    if(renderKey===signature){sync();return;}renderKey=signature;holder.replaceChildren();
    for(const node of [...nodes()].reverse().filter(node=>!node.discarded)){
      const row=document.createElement('div');row.className='layer stack-layer'+(!node.visible?' is-hidden':'');row.dataset.stackId=node.id;row.dataset.layerId=node.id;row.setAttribute('role','option');row.setAttribute('aria-selected',String(node.id===current?.id));row.tabIndex=node.id===current?.id?0:-1;
      const visibility=iconButton('',(node.visible?'Hide ':'Show ')+node.name,node.visible?'eye':'eye-off',event=>{event.stopPropagation();if(!busy)mutate('/stack/layer/'+encodeURIComponent(node.id),{visible:!node.visible},'PATCH');});visibility.removeAttribute('id');visibility.className='visibility';
      const content=document.createElement('div');content.className='layer-content';const thumb=document.createElement('img');thumb.className='layer-thumbnail';thumb.alt='';thumb.loading='lazy';thumb.src=endpoint('/stack/layer/'+encodeURIComponent(node.id)+'/display?r='+session.revision);const name=document.createElement('div');name.className='layer-name';name.textContent=node.name;name.title='Double-click to rename';
      const meta=document.createElement('div');meta.className='meta';meta.textContent=node.kind==='retouch'?(node.patch_ids?.length||0)+' repairs':node.kind==='cutout'?'Editable mask':node.kind==='original'?'Original image':'Image';content.append(thumb,name,meta);
      const lock=iconButton('',node.locked?'Unlock '+node.name:'Lock '+node.name,'lock',event=>{event.stopPropagation();if(!busy)mutate('/stack/layer/'+encodeURIComponent(node.id),{locked:!node.locked},'PATCH');});lock.removeAttribute('id');lock.className='stack-lock';lock.setAttribute('aria-pressed',String(!!node.locked));
      row.append(visibility,content,lock);row.onclick=()=>choose(node.id);name.ondblclick=()=>{choose(node.id);renameSelected();};
      row.addEventListener('keydown',event=>{if(event.target!==row)return;const rows=[...holder.children],index=rows.indexOf(row);let next;if(event.key==='ArrowUp')next=Math.max(0,index-1);else if(event.key==='ArrowDown')next=Math.min(rows.length-1,index+1);else if(event.key==='Home')next=0;else if(event.key==='End')next=rows.length-1;else if(event.key==='F2'){event.preventDefault();renameSelected();return;}else if(event.key==='Delete'){event.preventDefault();patchSelected({discarded:true});return;}else if(event.key==='Enter'||event.key===' '){event.preventDefault();choose(node.id);return;}else return;event.preventDefault();event.stopPropagation();choose(rows[next].dataset.stackId,{focus:true});});
      holder.append(row);
    }
    byId('layer-count').textContent=nodes().filter(node=>!node.discarded).length.toString();byId('restore').hidden=!nodes().some(node=>node.discarded);sync();
  };
  const previousRestore=byId('restore').onclick;
  byId('restore').onclick=()=>{if(!stacked())return previousRestore();const node=[...nodes()].reverse().find(item=>item.discarded);if(node)mutate('/stack/layer/'+encodeURIComponent(node.id),{discarded:false},'PATCH');};
  const previousMerge=byId('merge').onclick;
  byId('merge').onclick=()=>stacked()?mutate('/merge',{},'POST',{selectNew:true,label:'Visible layers merged into a new image layer. Earlier layers remain in the project.'}):previousMerge();

    const previousOpen=openSession;
  openSession=async function(data,...args){
    if(data?.id&&!data.layer_stack)data={...data,...await json('/api/local-remove/session/'+encodeURIComponent(data.id)+'/stack',{revision:data.revision})};
    if(data?.id&&data.selected_layer_id)selectedIds.set(data.id,data.selected_layer_id);
    return previousOpen.call(this,data,...args);
  };
  const previousRefresh=refreshPreview;
  refreshPreview=async function(...args){
    if(!stacked())return previousRefresh.apply(this,args);
    const requested=session,version=++requestVersion,assets=await displayAssets(requested);
    const [original,composite]=await Promise.all([assets.base,loadImage(endpoint('/preview?full=true&revision='+requested.revision))]);
    if(version!==requestVersion||session?.id!==requested.id)return;
    originalImage=original;previewImage=original;compositeImage=composite;byId('layer-stack').replaceChildren();
    paintPhoto();trackDocument(session);trimDisplayCache();syncCutoutFields();layerList();
  };
  const previousPaint=paintPhoto;
  paintPhoto=function(...args){
    if(!stacked())return previousPaint.apply(this,args);
    if(!originalImage)return;bc.clearRect(0,0,baseCanvas.width,baseCanvas.height);
    const photo=byId('photo-image'),display=showOriginal?originalImage:compositeImage||originalImage;photo.hidden=false;if(photo.src!==display.src)photo.src=display.src;
    byId('layer-stack').hidden=true;stage.classList.toggle('cutout-preview',!showOriginal);overlay.hidden=showOriginal||workspace==='generate';draft.hidden=showOriginal||workspace==='generate';
    byId('before').setAttribute('aria-pressed',String(showOriginal));controls();drawHandles();
  };
  const previousSyncCutout=syncCutoutFields;
  syncCutoutFields=function(...args){
    const value=previousSyncCutout.apply(this,args),node=selected();
    if(stacked()&&node?.kind==='cutout'){
      const data=node.cutout||{},shadow=data.shadow||{};setRangeValue('cutout-feather',data.feather||0);byId('shadow-enabled').checked=!!shadow.enabled;
      for(const [id,value]of Object.entries({'shadow-opacity':(shadow.opacity??.3)*100,'shadow-blur':shadow.blur??18,'shadow-x':shadow.offset_x??12,'shadow-y':shadow.offset_y??20,'shadow-squeeze':(shadow.squeeze??1)*100}))setRangeValue(id,value);
    }sync();return value;
  };
  const previousCutout=cutoutEdit;
  cutoutEdit=async function(path,body,method,...args){
    if(!visibleDocument())return;
    if(!await ensureStack())return;
    const node=selected(),oldIds=new Set(nodes().map(item=>item.id));
    if(body instanceof FormData){body.set('revision',String(session.revision));if(node)body.set('layer_id',node.id);}
    else body={...body,...(node?{layer_id:node.id}:{})};
    await previousCutout.call(this,path,body,method,...args);
    const created=[...nodes()].reverse().find(item=>!oldIds.has(item.id));if(created)selectedIds.set(session.id,created.id);
    assetCache.clear();renderKey='';layerList();syncCutoutFields();paintPhoto();
    if(created?.kind==='cutout'&&path==='/cutout')message('Cutout added as a new layer. Hide its source layer to see transparency.');
    if(path.includes('generate-background')&&!byId('message').classList.contains('error'))generationDialog.close();
  };
  // Selected mask only. The backend maps canvas-space brush pixels back through
  // this layer's transform so moved/scaled cutouts remain directly editable.
  byId('cutout-refine').onclick=()=>{if(hasSelection&&!showOriginal&&selected()?.kind==='cutout')cutoutEdit('/cutout/refine',{mask:selectionPayload(),operation:cutoutOperation},'POST','Refining layer mask\u2026',true);};
  byId('cutout-remove').onclick=()=>cutoutEdit('/cutout',{variant:qwenVariant},'POST','Removing background\u2026',true);

  const oldHistory=historyTarget;
  historyTarget=function(redo=false){if(stacked()&&!showOriginal&&!(redo?selectionRedo.length:points.length||undo.length)&&session[redo?'stack_can_redo':'stack_can_undo'])return 'stack';return oldHistory(redo);};
  const oldUndo=undoEdit,oldRedo=redoEdit;
  undoEdit=async function(...args){if(historyTarget()==='stack')return mutate('/stack/undo',{},'POST',{label:'Layer change undone'});return oldUndo.apply(this,args);};
  redoEdit=async function(...args){if(historyTarget(true)==='stack')return mutate('/stack/redo',{},'POST',{label:'Layer change redone'});return oldRedo.apply(this,args);};
  byId('undo').onclick=undoEdit;byId('redo').onclick=redoEdit;byId('cutout-undo').onclick=undoEdit;byId('cutout-redo').onclick=redoEdit;

  const previousSelect=selectTool;
  selectTool=function(value,...args){
    if(workspace==='cutout'&&selected()?.kind!=='cutout'&&['brush','pen','rectangle','ellipse'].includes(value))return;
    if(value==='move'&&stacked()){endGesture();tool='move';handActive=false;points=[];byId('finish').hidden=true;updateToolChrome();byId('tool-hint').textContent='Drag the layer to move \u00b7 Drag a corner to resize \u00b7 Drag the top handle to rotate';controls();drawHandles();return;}
    return previousSelect.call(this,value,...args);
  };
  byId('move-subject').onclick=()=>selectTool('move');byId('move-subject').setAttribute('aria-label','Move layer (V)');byId('move-subject').title='Move layer (V)';
  const previousControls=controls;
  controls=function(...args){const result=previousControls.apply(this,args);sync();return result;};
  const previousCamera=applyCamera;
  applyCamera=function(...args){const value=previousCamera.apply(this,args);drawHandles();return value;};
  const previousWorkspace=setWorkspace;
  setWorkspace=function(...args){const value=previousWorkspace.apply(this,args);sync();drawHandles();return value;};
  const previousCutoutWorkflow=cutoutWorkflowState;
  cutoutWorkflowState=function(){
    if(!stacked())return previousCutoutWorkflow();
    if(!session)return{title:'Open an image',description:'Open an image to start a cutout.',status:'No image'};
    if(selected()?.kind!=='cutout')return{title:'Remove the background',description:'Remove the background to create a new cutout layer, or add a mask from the Layer menu.',status:'Source layer selected'};
    return{title:'Edit the selected cutout',description:tool==='move'?'Drag the layer or use its corner handles.':'Brush an edge, then Erase or Restore. Hide the source layer to view transparency.',status:'Editable mask selected'};
  };
  function sync(){
    if(syncing)return;syncing=true;
    try{
      const layer=selected(),cutout=workspace==='cutout',generate=workspace==='generate',active=!!session&&!busy&&!showOriginal&&visibleDocument(),maskReady=cutout&&layer?.kind==='cutout'&&!layer.discarded,editable=active&&!layer?.locked&&!!layer?.visible;
      document.body.dataset.maskReady=String(!!maskReady);document.body.dataset.layerMove=String(!generate&&tool==='move'&&!handActive);
      byId('cutout-panel').hidden=true;panel.hidden=generate;toolbar.hidden=!cutout;moveControls.hidden=generate||tool!=='move'||handActive;
      byId('move-subject').hidden=generate;byId('move-subject').disabled=!active||!layer;
      if(cutout){
        for(const button of document.querySelectorAll('.toolrail [data-tool]:not([data-tool=move])')){button.hidden=!maskReady;button.disabled=!editable;}
        byId('cutout-modes').hidden=!maskReady||tool==='move'||handActive;
        byId('selection-context').hidden=!maskReady||tool==='move'||handActive;
        byId('repair-context').hidden=!maskReady||tool==='move'||handActive;
        byId('cutout-refine').disabled=!editable||!hasSelection;byId('cutout-erase').disabled=!editable;byId('cutout-restore').disabled=!editable;byId('cutout-restore').textContent='Restore';
        remove.textContent='Remove background';remove.hidden=!!maskReady;remove.disabled=!active||!qwenReady()||!layer?.visible;
        options.hidden=!!maskReady;options.disabled=busy;refinement.hidden=!maskReady;refinement.open=refinement.open&&maskReady;
        byId('shadow-disclosure').hidden=false;
        if(!maskReady)byId('tool-hint').textContent='Remove background creates a new layer \u00b7 Layer menu adds a manual mask';
        byId('cutout-feather').disabled=!editable;byId('cutout-feather-value').disabled=!editable;byId('shadow-enabled').disabled=!editable;
        for(const id of ['shadow-opacity','shadow-blur','shadow-x','shadow-y','shadow-squeeze']){byId(id).disabled=!editable||!byId('shadow-enabled').checked;byId(id+'-value').disabled=byId(id).disabled;}
      }
      if(tool==='move'&&!generate){byId('selection-context').hidden=true;byId('repair-context').hidden=true;byId('tool-name').textContent='Move layer';}
      background.disabled=!active;byId('background-generate').disabled=!active||!qwenReady()||!byId('background-prompt').value.trim();byId('background-prompt').disabled=!active;byId('background-library').disabled=!active;attach.disabled=!active;
      for(const button of byId('background-grid').querySelectorAll('button'))button.disabled=!active;
      folderEmpty.hidden=!!byId('background-grid').children.length;
      add.disabled=!active;more.disabled=!active||!layer;byId('layer-opacity').disabled=!editable;
      if(document.activeElement!==byId('layer-opacity'))byId('layer-opacity').value=String(Math.round((layer?.opacity??1)*100));
      for(const input of moveControls.querySelectorAll('input'))input.disabled=!editable;
      if(layer){const t=moving?.next||transform(layer);for(const [id,value]of Object.entries({x:t.offset_x,y:t.offset_y,scale:t.scale*100,rotation:t.rotation})){const input=byId('stack-transform-'+id);if(document.activeElement!==input)input.value=String(Math.round(value*100)/100);}}
      for(const button of holder.querySelectorAll('button'))button.disabled=busy;
      actions.lock.textContent=layer?.locked?'Unlock layer':'Lock layer';actions.lock.disabled=!active;actions.delete.disabled=!active||!!layer?.locked||layer?.kind==='original';actions.rename.disabled=!active;actions.reset.disabled=!editable;actions.mask.disabled=!active||!layer?.visible||layer?.kind==='cutout';
      actions.up.disabled=!editable||nodes().indexOf(layer)===nodes().length-1;actions.down.disabled=!editable||nodes().indexOf(layer)===0;
      if(stacked()){byId('merge').disabled=!active||nodes().filter(node=>node.visible&&!node.discarded).length<2;byId('merge').title='Add the visible composition as a new image layer';}
      for(const button of document.querySelectorAll('[data-command=merge]'))button.disabled=byId('merge').disabled;
      for(const button of mainLayerMenu.querySelectorAll('[data-command]')){const source=byId(button.dataset.command);if(source)button.disabled=source.disabled;}
      if(stacked()&&workspace==='retouch'&&layer?.kind==='retouch'&&(layer.locked||!layer.visible))byId('remove').disabled=true;
      if(stacked()&&historyTarget()==='stack')byId('edit-undo-label').textContent='Undo layer change';
      if(stacked()&&historyTarget(true)==='stack')byId('edit-redo-label').textContent='Redo layer change';
    }finally{syncing=false;}
  }

  function pointOnLayer(point,node,next=transform(node)){
    const cx=(session.width-1)/2,cy=(session.height-1)/2,a=next.rotation*Math.PI/180,c=Math.cos(a),s=Math.sin(a),x=(point.x-cx)*next.scale,y=(point.y-cy)*next.scale;
    return{x:cx+next.offset_x+c*x-s*y,y:cy+next.offset_y+s*x+c*y};
  }
  function geometry(node,next){
    const b=node?.bounds||[0,0,session.width,session.height],corners=[{x:b[0],y:b[1]},{x:b[2],y:b[1]},{x:b[2],y:b[3]},{x:b[0],y:b[3]}].map(p=>pointOnLayer(p,node,next));
    const center={x:(corners[0].x+corners[2].x)/2,y:(corners[0].y+corners[2].y)/2},top={x:(corners[0].x+corners[1].x)/2,y:(corners[0].y+corners[1].y)/2},length=Math.hypot(top.x-center.x,top.y-center.y)||1,distance=28*pixelRatio()/zoom;
    return{corners,center,top,rotate:{x:top.x+(top.x-center.x)/length*distance,y:top.y+(top.y-center.y)/length*distance},bounds:b};
  }
  function drawHandles(){
    if(!stacked()||tool!=='move'||workspace==='generate'||showOriginal||handActive||!selected())return;
    const node=selected();dc.clearRect(0,0,draft.width,draft.height);if(!node.visible||node.discarded)return;
    const g=geometry(node,moving?.next||transform(node)),ratio=pixelRatio(),size=7/zoom;draft.hidden=false;dc.save();dc.lineWidth=1/zoom;dc.strokeStyle=node.locked?'#a9b4c2':'#65bdff';dc.fillStyle='#172734';dc.beginPath();g.corners.forEach((p,index)=>index?dc.lineTo(p.x/ratio,p.y/ratio):dc.moveTo(p.x/ratio,p.y/ratio));dc.closePath();dc.stroke();
    if(!node.locked){dc.beginPath();dc.moveTo(g.top.x/ratio,g.top.y/ratio);dc.lineTo(g.rotate.x/ratio,g.rotate.y/ratio);dc.stroke();for(const p of [...g.corners,g.rotate]){dc.fillRect(p.x/ratio-size/2,p.y/ratio-size/2,size,size);dc.strokeRect(p.x/ratio-size/2,p.y/ratio-size/2,size,size);}}dc.restore();
  }
  async function loadMoveAssets(){
    const key=session.id+':'+session.revision;if(assetCache.has(key))return assetCache.get(key);
    const prefix=endpoint('/stack/layer/'),value=await Promise.all(nodes().filter(node=>node.visible&&!node.discarded).map(async node=>({node,image:await loadImage(prefix+encodeURIComponent(node.id)+'/display?r='+session.revision)})));
    assetCache.clear();assetCache.set(key,value);return value;
  }
  function paintMoving(){
    if(!moving?.assets)return;byId('photo-image').hidden=true;byId('layer-stack').hidden=true;overlay.hidden=true;bc.clearRect(0,0,baseCanvas.width,baseCanvas.height);
    const ratio=pixelRatio(),cx=(session.width-1)/2/ratio,cy=(session.height-1)/2/ratio;
    for(const {node,image}of moving.assets){const t=node.id===moving.id?moving.next:transform(node);bc.save();bc.globalAlpha=node.opacity??1;bc.translate(cx+t.offset_x/ratio,cy+t.offset_y/ratio);bc.rotate(t.rotation*Math.PI/180);bc.scale(t.scale,t.scale);bc.drawImage(image,-cx,-cy,baseCanvas.width,baseCanvas.height);bc.restore();}drawHandles();sync();
  }
  const documentPoint=event=>{const p=coord(event),r=pixelRatio();return{x:p.x*r,y:p.y*r};};
  viewport.addEventListener('pointerdown',async event=>{
    if(!stacked()||workspace==='generate'||spaceHeld||handActive||event.button!==0||modalOpen())return;
    if(workspace==='cutout'&&selected()?.kind!=='cutout'&&tool!=='move'){event.stopImmediatePropagation();return;}
    if(tool!=='move')return;event.preventDefault();event.stopImmediatePropagation();
    const node=selected();if(!node||busy||showOriginal||!node.visible||node.discarded)return;
    if(node.locked){message('Unlock this layer in Layers to move it.');return;}
    const point=documentPoint(event),g=geometry(node,transform(node)),tolerance=12*pixelRatio()/zoom;
    let kind='move',corner=g.corners.findIndex(p=>Math.hypot(p.x-point.x,p.y-point.y)<tolerance);if(corner>=0)kind='scale';else if(Math.hypot(g.rotate.x-point.x,g.rotate.y-point.y)<tolerance)kind='rotate';
    const drag={sid:session.id,revision:session.revision,id:node.id,pointerId:event.pointerId,start:point,original:transform(node),next:transform(node),geometry:g,corner,kind,assets:null};moving=drag;viewport.setPointerCapture(event.pointerId);viewport.focus({preventScroll:true});
    try{const assets=await loadMoveAssets();if(moving===drag){drag.assets=assets;paintMoving();}}catch(error){if(moving===drag){moving=null;paintPhoto();message(error.message,true);}}
  },true);
  viewport.addEventListener('pointermove',event=>{
    if(!moving||moving.pointerId!==event.pointerId)return;event.preventDefault();event.stopImmediatePropagation();const p=documentPoint(event),d=moving,o=d.original;
    if(d.kind==='move')d.next={...o,offset_x:o.offset_x+p.x-d.start.x,offset_y:o.offset_y+p.y-d.start.y};
    else if(d.kind==='scale'){
      const opposite=(d.corner+2)%4,anchor=d.geometry.corners[opposite],corner=d.geometry.corners[d.corner],factor=Math.hypot(p.x-anchor.x,p.y-anchor.y)/Math.max(1,Math.hypot(corner.x-anchor.x,corner.y-anchor.y)),scale=Math.max(.05,Math.min(4,o.scale*factor));
      const b=d.geometry.bounds,source=[{x:b[0],y:b[1]},{x:b[2],y:b[1]},{x:b[2],y:b[3]},{x:b[0],y:b[3]}][opposite],a=o.rotation*Math.PI/180,cx=(session.width-1)/2,cy=(session.height-1)/2,x=(source.x-cx)*scale,y=(source.y-cy)*scale;
      d.next={...o,scale,offset_x:anchor.x-cx-Math.cos(a)*x+Math.sin(a)*y,offset_y:anchor.y-cy-Math.sin(a)*x-Math.cos(a)*y};
    }else{
      const c=d.geometry.center,delta=(Math.atan2(p.y-c.y,p.x-c.x)-Math.atan2(d.start.y-c.y,d.start.x-c.x))*180/Math.PI,rotation=((o.rotation+delta+540)%360)-180,a=rotation*Math.PI/180,b=d.geometry.bounds,cx=(session.width-1)/2,cy=(session.height-1)/2,x=((b[0]+b[2])/2-cx)*o.scale,y=((b[1]+b[3])/2-cy)*o.scale;
      d.next={...o,rotation,offset_x:c.x-cx-Math.cos(a)*x+Math.sin(a)*y,offset_y:c.y-cy-Math.sin(a)*x-Math.cos(a)*y};
    }paintMoving();
  },true);
  const finishMove=async(event,cancel=false)=>{
    if(!moving||moving.pointerId!==event.pointerId)return;event.stopImmediatePropagation();const drag=moving;moving=null;if(viewport.hasPointerCapture(event.pointerId))viewport.releasePointerCapture(event.pointerId);
    if(cancel||session?.id!==drag.sid||session.revision!==drag.revision){paintPhoto();return;}
    if(JSON.stringify(drag.next)!==JSON.stringify(drag.original))await mutate('/stack/layer/'+encodeURIComponent(drag.id),{transform:drag.next},'PATCH',{label:'Layer transformed'});else paintPhoto();
  };
  viewport.addEventListener('pointerup',event=>finishMove(event),true);viewport.addEventListener('pointercancel',event=>finishMove(event,true),true);
  viewport.addEventListener('lostpointercapture',event=>{if(moving?.pointerId===event.pointerId){moving=null;paintPhoto();}},true);
  document.addEventListener('keydown',event=>{
    if(textEntry(event.target)||modalOpen()||workspace==='generate'||!stacked())return;
    if(event.key.toLowerCase()==='v'&&!event.ctrlKey&&!event.metaKey&&!event.altKey){event.preventDefault();event.stopImmediatePropagation();selectTool('move');}
    if(event.key==='Escape'&&moving){event.preventDefault();event.stopImmediatePropagation();const drag=moving;moving=null;if(viewport.hasPointerCapture(drag.pointerId))viewport.releasePointerCapture(drag.pointerId);paintPhoto();}
  },true);
  const oldBackgroundRender=renderBackgroundGrid;
  renderBackgroundGrid=function(...args){const value=oldBackgroundRender.apply(this,args);sync();return value;};
  window.LocalImageLayers={nodes,selected,select:choose,prepareRetouch,retouchTargetId:()=>selected()?.kind==='retouch'?selected().id:null,refreshUI:()=>{renderKey='';layerList();sync();},createRetouch,patchSelected,ensureStack,showAssets,addMask};
  sync();
})();
