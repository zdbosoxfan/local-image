/* Stock stays in the Assets dock; expand it when browsing needs more room.
 * Existing editor search, provider, import, credit and selection contracts stay
 * in editor.js. This layer changes presentation without recreating those flows.
 */
(()=>{
  'use strict';
  const dialog=document.getElementById('stock-dialog');
  if(!dialog||dialog.dataset.studioStock)return;
  dialog.dataset.studioStock='true';
  dialog.classList.add('stock-studio');
  dialog.setAttribute('aria-modal','false');
  const byId=id=>document.getElementById(id);
  const header=dialog.querySelector('.dialog-header');
  const detail=dialog.querySelector('.stock-detail');
  const footer=dialog.querySelector('.stock-footer');
  const close=byId('stock-close');
  let expanded=false,informationOpen=false,providersRequested=false,refreshingControls=false;
  let stockWorkspace=workspace,stockHadDocument=!!session,stockReferenceSupport=false;
  const assets=document.createElement('aside');
  assets.id='studio-assets';
  assets.setAttribute('aria-label','Assets');
  assets.innerHTML='<header class="studio-assets-header"><h2>Assets</h2></header><nav class="studio-assets-nav" aria-label="Asset sources"><button id="studio-assets-stock" type="button" aria-pressed="true">Stock</button><button id="studio-assets-folders" type="button" title="Choose a local background folder">Folders</button><button id="studio-assets-generated" type="button" title="Open your generated image library">Generated</button></nav>';
  document.querySelector('.toolrail').after(assets);
  assets.append(dialog);
  dialog.dataset.docked='true';
  const assetsHeader=assets.querySelector('.studio-assets-header');
  const providerCaption=document.createElement('span');
  providerCaption.id='stock-provider-caption';
  providerCaption.className='stock-provider-caption';
  providerCaption.hidden=true;
  byId('stock-search-form').append(providerCaption);

  const expand=document.createElement('button');
  expand.id='stock-expand';
  expand.type='button';
  expand.className='stock-expand';
  expand.setAttribute('aria-expanded','false');
  expand.setAttribute('aria-controls','stock-dialog');
  header.insertBefore(expand,close);

  detail.id='stock-detail-panel';
  detail.tabIndex=-1;
  const informationClose=document.createElement('button');
  informationClose.id='stock-info-close';
  informationClose.type='button';
  informationClose.className='stock-info-close';
  informationClose.setAttribute('aria-label','Close image information');
  informationClose.innerHTML='<svg viewBox="0 0 20 20" aria-hidden="true"><path d="m5 5 10 10M15 5 5 15"/></svg>';
  detail.prepend(informationClose);
  const preview=byId('stock-preview');
  const previewUnavailable=document.createElement('p');
  previewUnavailable.className='stock-preview-unavailable';
  previewUnavailable.textContent='Preview unavailable. View the original source for this image.';
  previewUnavailable.hidden=true;
  preview.after(previewUnavailable);
  preview.addEventListener('error',()=>{preview.hidden=true;previewUnavailable.hidden=!selectedStock();});
  preview.addEventListener('load',()=>{preview.hidden=!selectedStock();previewUnavailable.hidden=true;});

  const selectedRow=document.createElement('div');
  selectedRow.className='stock-selection';
  const selectedLabel=document.createElement('span');
  selectedLabel.id='stock-selected-name';
  const information=document.createElement('button');
  information.id='stock-info';
  information.type='button';
  information.className='stock-info';
  information.setAttribute('aria-label','Image information and license');
  information.setAttribute('aria-controls',detail.id);
  information.setAttribute('aria-expanded','false');
  information.innerHTML='<svg viewBox="0 0 20 20" aria-hidden="true"><circle cx="10" cy="10" r="7"/><path d="M10 9v5m0-8v.1"/></svg>';
  selectedRow.append(selectedLabel,information);
  footer.prepend(selectedRow);

  function present(){
    dialog.dataset.docked=String(!expanded);
    if(expanded&&dialog.parentElement!==document.body)document.body.append(dialog);
    else if(!expanded&&dialog.parentElement!==assets)assets.append(dialog);
    (expanded?header:assetsHeader).append(expand,close);
    header.hidden=!expanded;
    providerCaption.hidden=expanded||stockProviders.length!==1;
    dialog.classList.toggle('stock-expanded',expanded);
    dialog.classList.toggle('stock-information-open',informationOpen&&!expanded);
    expand.setAttribute('aria-expanded',String(expanded));
    expand.setAttribute('aria-label',expanded?'Collapse stock browser':'Expand stock browser');
    expand.title=expanded?'Return to the Assets dock':'Give images more room';
    expand.innerHTML=expanded
      ?'<svg viewBox="0 0 20 20" aria-hidden="true"><path d="M7 3v4H3m10 10v-4h4M3 7l4-4m6 14 4-4"/></svg><span>Collapse</span>'
      :'<svg viewBox="0 0 20 20" aria-hidden="true"><path d="M11 3h6v6M9 17H3v-6M17 3l-5 5M3 17l5-5"/></svg><span>Expand</span>';
    detail.hidden=!expanded&&!informationOpen;
    information.hidden=expanded;
    informationClose.hidden=expanded;
    information.setAttribute('aria-expanded',String(informationOpen));
    close.setAttribute('aria-label',expanded?'Close expanded stock browser':'Hide Assets panel');
  }
  function sync(){
    const selected=typeof selectedStock==='function'?selectedStock():null;
    selectedLabel.textContent=selected?.title||'Select an image to import';
    selectedLabel.title=selected?.title||'';
    information.disabled=!selected;
    expand.disabled=close.disabled;
    byId('studio-assets-folders').disabled=busy||closeInProgress||!session;
    byId('studio-assets-folders').title=session?'Choose a local background folder':'Open an image to choose a background folder';
    byId('studio-assets-generated').disabled=busy||closeInProgress;
    byId('studio-assets-stock').disabled=busy||closeInProgress;
    if(!selected)previewUnavailable.hidden=true;
    providerCaption.textContent=stockProviders.length===1?stockProviders[0].label:'';
    providerCaption.hidden=expanded||stockProviders.length!==1;
    // The compact footer has a short selection label; full credits are one
    // deliberate action away instead of crowding every thumbnail.
    dialog.classList.toggle('stock-has-selection',!!selected);
    if(!selected&&informationOpen){informationOpen=false;present();}
  }
  const previousUpdate=updateStockControls;
  updateStockControls=function(){
    const cap=generationModel()?.capabilities||{};
    const supportsReference=!!(cap.image_reference||cap.image_to_image||cap.references);
    if(stockWorkspace!==workspace||stockHadDocument!==!!session||(workspace==='generate'&&stockReferenceSupport!==supportsReference)){
      stockWorkspace=workspace;stockHadDocument=!!session;
      stockOrigin=workspace==='cutout'&&session?'background':workspace==='generate'?'reference':'image';
    }
    stockReferenceSupport=supportsReference;
    if(stockOrigin==='reference'&&!supportsReference)stockOrigin='image';
    previousUpdate();sync();
  };
  function afterControls(previous,args,context){
    if(refreshingControls)return previous.apply(context,args);
    refreshingControls=true;
    try{const result=previous.apply(context,args);updateStockControls();return result;}
    finally{refreshingControls=false;}
  }
  const previousControls=controls;
  controls=function(...args){return afterControls(previousControls,args,this);};
  const previousGenerationControls=updateGenerationControls;
  updateGenerationControls=function(...args){return afterControls(previousGenerationControls,args,this);};
  const previousOpen=openStockLibrary;
  openStockLibrary=function(...args){
    if(busy||closeInProgress||stockLoading||stockImporting)return Promise.resolve();
    if(document.body.dataset.generationView==='refine'&&byId('refine-close')?.disabled)return Promise.resolve();
    providersRequested=true;
    return previousOpen(...args);
  };
  const previousRender=renderStockResults;
  renderStockResults=function(...args){
    previousRender(...args);
    for(const button of byId('stock-results').querySelectorAll('.stock-result')){
      const image=button.querySelector('img');
      image.addEventListener('error',()=>{
        if(button.querySelector('.stock-preview-unavailable'))return;
        image.hidden=true;button.classList.add('stock-preview-failed');
        const note=document.createElement('span');note.className='stock-preview-unavailable';note.textContent='Preview unavailable';button.append(note);
        const label=button.getAttribute('aria-label')||'Stock image';
        button.setAttribute('aria-label',label+' — Preview unavailable');button.title=label+' — Preview unavailable. Image information and source are still available.';
      });
    }
  };

  expand.addEventListener('click',()=>{
    if(expand.disabled)return;
    expanded=!expanded;informationOpen=false;present();expand.focus();
  });
  information.addEventListener('click',()=>{
    informationOpen=!informationOpen;present();
    if(informationOpen)informationClose.focus();
  });
  informationClose.addEventListener('click',()=>{
    informationOpen=false;present();information.focus();
  });

  // Keep a real, nonmodal dialog in the dock so legacy search/import IDs and
  // keyboard selection remain intact. Expansion relocates the same DOM.
  const show=dialog.show.bind(dialog);
  const nativeClose=dialog.close.bind(dialog);
  function openDock(){
    if(document.body.dataset.generationView==='refine'){
      window.LocalImageGenerationStudio?.showCreate();
      if(document.body.dataset.generationView==='refine')return;
    }
    expanded=false;informationOpen=false;present();
    assets.hidden=false;document.body.dataset.assetsOpen='true';
    byId('studio-stock-open')?.setAttribute('aria-expanded','true');
    if(!dialog.open)show();
    sync();
  }
  function closeDock(){
    if(close.disabled)return;
    expanded=false;informationOpen=false;present();assets.hidden=true;
    document.body.dataset.assetsOpen='false';
    byId('studio-stock-open')?.setAttribute('aria-expanded','false');
    if(dialog.open)nativeClose();
  }
  dialog.showModal=function(){
    openDock();
  };
  dialog.close=function(){
    // An imported asset leaves the useful dock available for the next choice.
    if(stockImporting){expanded=false;informationOpen=false;present();return;}
    if(expanded){expanded=false;informationOpen=false;present();expand.focus();return;}
    closeDock();
  };
  dialog.addEventListener('close',()=>{
    if(dialog.open)return;
    document.body.dataset.assetsOpen='false';assets.hidden=true;
    byId('studio-stock-open')?.setAttribute('aria-expanded','false');
  });
  document.addEventListener('keydown',event=>{
    // An import-options menu owns its first Escape; the expanded browser is
    // dismissed only after that menu has closed.
    if(event.target instanceof Element&&event.target.closest('[popover]:popover-open'))return;
    if(!dialog.open||event.key!=='Escape'||(!expanded&&!informationOpen))return;
    event.preventDefault();event.stopImmediatePropagation();
    if(close.disabled)return;
    if(informationOpen&&!expanded){informationOpen=false;present();information.focus();}
    else{expanded=false;informationOpen=false;present();expand.focus();}
  },true);
  // Asset keyboard navigation must not also activate canvas tool shortcuts.
  dialog.addEventListener('keydown',event=>{
    if(!event.ctrlKey&&!event.metaKey)event.stopPropagation();
  });
  function loadProviders(){
    if(!providersRequested&&!stockProviders.some(item=>item.available!==false)&&!stockLoading&&!busy&&!modalOpen()){
      providersRequested=true;
      openStockLibrary(stockOrigin,byId('studio-stock-open')||byId('stock-open'));
    }
  }
  function openForUser(){
    if(busy||closeInProgress||stockImporting)return;
    // Only a deliberate open retries a failed discovery. Focus events from
    // request completion must never recursively launch network attempts.
    if(!stockProviders.some(item=>item.available!==false))providersRequested=false;
    openDock();loadProviders();
  }
  assets.addEventListener('pointerenter',loadProviders);
  assets.addEventListener('focusin',loadProviders);
  byId('studio-assets-stock').addEventListener('click',()=>{
    if(busy)return;openForUser();byId('stock-query').focus();
  });
  byId('studio-assets-folders').addEventListener('click',()=>{
    if(busy||!session)return;
    setWorkspace('cutout');selectStudioTab('background');
    // Choosing a folder only adds it to the library, so it is useful before
    // the subject has a cutout. Reuse the existing native/browser chooser.
    byId('background-folder').onclick();
  });
  byId('studio-assets-generated').addEventListener('click',()=>{
    if(!busy)openGeneratedLibrary();
  });
  window.LocalImageStockStudio={open:openForUser,close(){stockOpener=null;closeDock();},setExpanded(value){
    if(close.disabled)return;openDock();expanded=!!value;present();
  }};
  openDock();requestAnimationFrame(loadProviders);
})();
