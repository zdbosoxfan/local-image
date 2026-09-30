// Presentation commands delegate to the same document operations as the menus.
(() => {
  const icons={undo:'M9 5 3 10l6 5M4 10h10a6 6 0 0 1 0 12',redo:'m15 5 6 5-6 5M20 10H10a6 6 0 0 0 0 12',stock:'M3 4h18v16H3zM3 16l5-5 4 4 4-6 5 7',library:'M3 5h7l2 3h9v12H3z',export:'M12 15V3m-4 4 4-4 4 4M5 12v8h14v-8',inspector:'M3 4h18v16H3zM15 4v16'};
  const svg=key=>{const icon=document.createElementNS('http://www.w3.org/2000/svg','svg'),path=document.createElementNS(icon.namespaceURI,'path');icon.setAttribute('viewBox','0 0 24 24');icon.setAttribute('class','icon');icon.setAttribute('aria-hidden','true');path.setAttribute('d',icons[key]);icon.append(path);return icon;};
  const actions=document.createElement('div');actions.className='studio-header-actions';
  const make=(id,label,icon,handler,iconOnly=false)=>{const button=document.createElement('button');button.id=id;button.type='button';button.setAttribute('aria-label',label);button.title=label;button.append(svg(icon));if(iconOnly)button.className='history-action';else{const text=document.createElement('span');text.className='header-action-label';text.textContent=label;button.append(text);}button.onclick=handler;actions.append(button);return button;};
  const invoke=id=>{closeMenus();$(id).click();};
  make('studio-undo','Undo','undo',()=>invoke('undo'),true);
  make('studio-redo','Redo','redo',()=>invoke('redo'),true);
  const divider=document.createElement('span');divider.className='header-divider';divider.setAttribute('aria-hidden','true');actions.append(divider);
  make('studio-stock-open','Assets','stock',()=>window.LocalImageStockStudio?.open());
  make('studio-generated-open','Library','library',()=>invoke('generated-library-open'));
  const inspector=make('studio-inspector-toggle','Inspector','inspector',()=>{document.body.dataset.inspectorHidden=String(document.body.dataset.inspectorHidden!=='true');syncChrome();});
  inspector.setAttribute('aria-controls','studio-inspector');inspector.setAttribute('aria-expanded','true');
  const exportButton=make('studio-export','Export','export',async()=>{
    try{if(window.LocalImageGenerationStudio?.isRefining()){if(!await window.LocalImageGenerationStudio.openSelectedInEditor())return;}
      invoke(workspace==='cutout'?'cutout-export':'save');
    }catch(error){message(error.message,true);}
  });exportButton.className='secondary';
  const settings=document.querySelector('.persona-settings');if(settings)actions.append(settings);
  document.querySelector('.persona-toolbar').append(actions);
  // Repair methods are selected with the two tool-rail buttons. Preserve the
  // command hooks for shortcuts/startup without displaying a second selector.
  $('output-format').parentElement.append($('retouch-modes'));
  document.querySelector('.right').id='studio-inspector';
  $('workspace-generate').querySelector('span').textContent='Generate';
  $('workspace-generate').title='Generate · Create images with local models';
  const heading=document.createElement('h1');heading.id='studio-empty-title';heading.textContent='Start with an image';$('empty-open').before(heading);
  const shortcut=document.createElement('span');shortcut.className='empty-shortcut';shortcut.textContent='Ctrl + O to open a photo';$('empty').append(shortcut);
  const composition=document.createElement('section');composition.className='composition-layers';composition.hidden=true;composition.setAttribute('aria-label','Composition');
  const title=document.createElement('h3');title.textContent='Composition';composition.append(title);
  const compositionRows={};
  for(const [key,label,icon,tab]of[['subject','Subject','image','transform'],['shadow','Shadow','eye','background'],['background','Background','image','background']]){
    const row=document.createElement('button');row.className='composition-layer';row.append(svgIcon(icon));const text=document.createElement('span');text.textContent=label;const detail=document.createElement('small');row.append(text,detail);row.onclick=()=>{selectStudioTab(tab);if(key==='shadow'){$('shadow-disclosure').open=true;$('shadow-disclosure').scrollIntoView({block:'nearest'});}};compositionRows[key]={row,detail};composition.append(row);
  }
  $('cutout-panel').append(composition);
  function syncChrome(){
    const refining=!!window.LocalImageGenerationStudio?.isRefining();
    for(const command of ['undo','redo']){const button=$('studio-'+command),source=$(command);button.disabled=source.disabled||refining;button.title=refining?'Return to the editor to change edit history':source.textContent.trim();button.setAttribute('aria-label',button.title);}
    exportButton.disabled=busy||(refining?!window.LocalImageGenerationStudio.hasSelectedImage():!session);
    inspector.setAttribute('aria-expanded',String(document.body.dataset.inspectorHidden!=='true'));
    inspector.setAttribute('aria-pressed',String(document.body.dataset.inspectorHidden!=='true'));
    inspector.disabled=refining;exportButton.title=refining?'Export the selected refinement result':workspace==='cutout'?'Export composition as PNG':'Export a copy';
    $('document-state').hidden=$('document-state').textContent.trim()==='Original';
    heading.textContent=workspace==='generate'?'Create something new':'Start with an image';shortcut.hidden=workspace==='generate';
    const enabled=workspace==='cutout'&&!!session?.cutout?.enabled;composition.hidden=!enabled;
    if(enabled){for(const {row}of Object.values(compositionRows))row.disabled=busy;
      compositionRows.subject.detail.textContent='Cutout';
      compositionRows.shadow.detail.textContent=session.cutout.shadow?.enabled?'On':'Off';
      compositionRows.background.detail.textContent=({transparent:'Transparent',color:'Solid color',image:'Image'})[session.cutout.background?.mode]||'Transparent';
    }
  }
  const priorControls=controls;controls=function(...args){const result=priorControls.apply(this,args);syncChrome();return result;};
  new MutationObserver(syncChrome).observe(document.body,{attributes:true,attributeFilter:['data-persona','data-generation-studio']});
  window.LocalImageStudio={sync:syncChrome};syncChrome();
})();
