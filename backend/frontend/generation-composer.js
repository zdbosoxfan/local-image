/* Prompt-first generation. Existing controls are moved, never copied: the
 * engine, document state and export handlers keep owning their original IDs.
 * References: Photoshop's contextual prompt bar; Affinity's context tools;
 * Spectrum's single-open disclosure pattern.
 */
(() => {
  'use strict';
  const $ = id => document.getElementById(id);
  const panel = $('generation-panel'), refine = $('refine-dialog');
  if (!panel || !refine || $('generation-composer')) return;
  const make = (tag, className, text) => {
    const node = document.createElement(tag);
    if (className) node.className = className;
    if (text !== undefined) node.textContent = text;
    return node;
  };
  const move = (target, ...nodes) => {
    for (const node of nodes) {
      if (!node) continue;
      if (typeof node === 'string') { if ($(node)) target.append($(node)); }
      else if (node instanceof Node) target.append(node);
      else move(target, ...Array.from(node));
    }
  };
  const labels = id => [...document.querySelectorAll('label[for="' + id + '"]')];
  const group = (host, name, id) => {
    const details = make('details', 'composer-section'); details.id = id;
    details.setAttribute('name',host.closest('#generation-panel')?'composer-create':'composer-refine');
    const summary = make('summary', '', name);
    const content = make('div', 'composer-section-content');
    details.append(summary, content); host.append(details);
    details.addEventListener('toggle', () => {
      if (!details.open) return;
      const scope=host.closest('.composer-inspector')||host;
      for (const other of scope.querySelectorAll('.composer-section')) if (other !== details) other.open = false;
    });
    return { details, content, summary };
  };
  const flatten = node => {
    if (!node) return;
    // Retain detail IDs relied upon by capability and busy-state logic, but
    // avoid nested disclosure controls inside the four inspector sections.
    node.classList.add('composer-flat-group'); node.open = true;
  };
  function quietNote(note) {
    if (!note || note.classList.contains('composer-guidance')) return;
    note.classList.add('composer-guidance');
    const info = make('button', 'composer-info', 'i'); info.type = 'button';
    const sync = () => {
      const text = note.textContent.trim();
      info.title = text; info.setAttribute('aria-label', text || 'More information');
      info.hidden = !text || note.hidden || note.classList.contains('error');
      const error=note.classList.contains('error');
      if(note.classList.contains('composer-guidance-error')!==error)note.classList.toggle('composer-guidance-error',error);
    };
    note.before(info);
    // Native title is available on hover; clicking exposes an accessible,
    // transient explanation rather than permanent instructional paragraphs.
    const tip = make('div', 'composer-info-popover'); tip.setAttribute('popover', 'auto');
    info.after(tip);
    info.addEventListener('click', event => {
      event.preventDefault();
      tip.textContent = note.textContent;
      if (tip.matches(':popover-open')) tip.hidePopover();
      else { tip.showPopover(); const r=info.getBoundingClientRect();tip.style.left=Math.max(8,Math.min(innerWidth-292,r.right-280))+'px';tip.style.top=Math.min(innerHeight-tip.offsetHeight-8,r.bottom+8)+'px'; }
    });
    new MutationObserver(sync).observe(note, { childList:true,subtree:true,characterData:true,attributes:true,attributeFilter:['hidden','class'] });
    sync();
  }
  function promptDock(promptId, actionId, id) {
    const dock = make('div', 'generation-composer'); dock.id=id;
    const label = labels(promptId)[0];
    if (label) { label.classList.add('composer-visually-hidden'); dock.append(label); }
    const input = $(promptId);input.rows=2;
    dock.append(input,$(actionId));
    return dock;
  }

  panel.classList.add('composer-inspector');
  panel.querySelector('.studio-tabs').hidden=true;
  const scroll=$('generation-panel-scroll');scroll.removeAttribute('role');scroll.removeAttribute('aria-labelledby');
  const oldSections=[...scroll.querySelectorAll('[data-gen-panel]')];
  oldSections.forEach(section=>{section.removeAttribute('data-gen-panel');section.hidden=false;});
  const model=group(scroll,'Model','composer-create-model');
  const output=group(scroll,'Output','composer-create-output');
  const references=group(scroll,'References','composer-create-references');
  const style=group(scroll,'Style','composer-create-style');
  move(model.content, panel.querySelector('.generation-model-label'), 'gen-model', 'gen-model-state', 'generation-precision-options', 'gen-setup-options');
  flatten($('generation-precision-options'));flatten($('gen-setup-options'));
  const source=$('generation-edit-source');
  if(source)model.content.append(source);
  const dock=promptDock('gen-prompt','gen-run','generation-composer');
  document.querySelector('.canvas-area').append(dock);
  $('gen-run').classList.remove('full-width');
  move(dock,'gen-result-note');
  move(output.content,oldSections[2].children,document.querySelector('.generation-create-essentials'));
  // Size and steps are first, presets second, advanced sampling last.
  const essentials=output.content.querySelector('.generation-create-essentials');
  output.content.prepend(essentials);
  flatten($('gen-sampling-options'));
  move(references.content,oldSections[1].children);
  $('gen-reference-title').classList.add('composer-visually-hidden');
  $('gen-add-reference').textContent='Add reference';$('gen-stock').textContent='Choose from Assets';
  move(style.content,'gen-lora-options','gen-negative-options');
  flatten($('gen-lora-options'));flatten($('gen-negative-options'));
  $('lora-library').textContent='Browse LoRAs';
  // A flattened negative section still needs its own visible field label.
  const negativeLabel=make('label','field-label spaced','Negative prompt');negativeLabel.htmlFor='gen-negative';
  $('gen-negative').before(negativeLabel);
  oldSections.forEach(section=>section.remove());
  panel.querySelector('.studio-footer')?.remove();
  const hint=$('generation-blank-canvas');hint.replaceChildren(make('p','','Describe an image below.'));
  for(const note of panel.querySelectorAll('.cutout-note'))if(!['gen-sampling-note','gen-size-note'].includes(note.id))quietNote(note);
  // Source identity is already present above the image. Only a blocking source
  // problem should be repeated in the generation inspector.
  source?.classList.add('composer-source-note');

  const createRows={model,output,references,style};
  const syncCreate=()=>{
    references.details.hidden=$('gen-reference-tab').hidden;
    style.details.hidden=$('gen-lora-options').hidden&&$('gen-negative-options').hidden;
    const error=$('gen-model-state').classList.contains('error');
    model.summary.classList.toggle('composer-has-error',error);
    model.summary.title=error?$('gen-model-state').textContent:'';
    const run=$('gen-run');
    if(run.textContent==='Generate image')run.textContent='Generate';
  };
  const priorGenerationControls=updateGenerationControls;
  updateGenerationControls=function(...args){const result=priorGenerationControls.apply(this,args);syncCreate();return result;};
  // The original HTML controller assigned these callbacks before the mode
  // adapter existed. Resolve the current handler so typing cannot revert an
  // Edit button to the old generic label or bypass mode validation.
  for(const id of ['gen-prompt','gen-guidance','gen-steps'])$(id).oninput=()=>updateGenerationControls();
  // Programmatic navigation from restored recipes and stock references still
  // selects a useful disclosure without reviving the removed tab interface.
  const priorGenerationTab=selectGenerationTab;
  selectGenerationTab=function(value){
    const result=priorGenerationTab.call(this,value);
    if(value==='reference')references.details.open=true;
    else if(value==='output')output.details.open=true;
    return result;
  };
  syncCreate();
  const syncActionLabel=()=>{
    const editing=window.LocalImageGenerationStudio?.mode()==='edit';
    const text=busy&&activeTask==='generate'?(editing?'Editing…':'Generating…'):(editing?'Apply edit':'Generate');
    if($('gen-run').textContent!==text)$('gen-run').textContent=text;
  };
  new MutationObserver(syncActionLabel).observe($('gen-run'),{childList:true});
  syncActionLabel();

  const inspector=refine.querySelector('.generation-refine-inspector');
  inspector.classList.add('composer-inspector');
  inspector.querySelector('.generation-inspector-title').textContent='Settings';
  inspector.querySelector('.generation-local-note').hidden=true;
  $('refine-close').hidden=true;
  for(const [stage, actionId, emptyId] of [['draft','refine-draft-run','refine-draft-empty'],['final','refine-final-run','refine-result-empty']]){
    const pane=refine.querySelector('.refine-pane[data-stage="'+stage+'"]');
    const settings=$('generation-stage-'+stage+'-settings');
    const core=pane.querySelector('.generation-stage-core');
    const stageDock=promptDock('refine-'+stage+'-prompt',actionId,'composer-'+stage+'-prompt');
    pane.append(stageDock);
    const openButton=$('refine-open-'+(stage==='draft'?'draft':'result'));
    openButton.textContent='Open in editor';
    pane.querySelector('.refine-pane-heading').append(openButton);
    pane.querySelector('.refine-pane-actions')?.remove();
    $(actionId).textContent=stage==='draft'?'Generate':'Refine';
    $(emptyId).replaceChildren(document.createTextNode(stage==='draft'?'Describe a draft below.':'Choose a draft to refine.'));
    const old=[...settings.children];
    const sModel=group(settings,'Model','composer-'+stage+'-model');
    const sOutput=group(settings,'Output','composer-'+stage+'-output');
    const sRefs=group(settings,'References','composer-'+stage+'-references');
    const sStyle=group(settings,'Style','composer-'+stage+'-style');
    move(sModel.content,core.querySelector('.generation-stage-model'));
    const precision=settings.querySelector('.generation-model-details');flatten(precision);move(sModel.content,precision);
    move(sOutput.content,core.querySelector('.generation-core-fields'),'refine-'+stage+'-size-note','refine-'+stage+'-recommended');
    core.remove();
    const stageStyles=settings.querySelector('.generation-stage-style');
    move(sOutput.content,'refine-'+stage+'-transparent-row');
    move(sStyle.content,'refine-'+stage+'-lora-options','refine-'+stage+'-output-note');
    stageStyles?.remove();
    if(stage==='draft'){
      const opts=$('refine-draft-options');
      move(sOutput.content,opts.querySelector('.refine-fields'));
      move(sRefs.content,'refine-references','refine-reference-note',opts.querySelector('.background-actions'),'refine-denoise-row');
      // Keep the original ID for callers that set options.open.
      flatten(opts);move(sOutput.content,opts);
    }else{
      move(sOutput.content,settings.querySelector('.refine-output-heading'));
      const advanced=old.find(node=>node.tagName==='DETAILS'&&!node.classList.contains('generation-model-details')&&!node.classList.contains('generation-upscale-disclosure'));
      move(sOutput.content,advanced?.querySelector('.refine-fields'));
      move(sStyle.content,'refine-negative-row');
      move(sRefs.content,$('refine-include-references').closest('label'));
      if(advanced){for(const note of advanced.querySelectorAll('.cutout-note'))move(sRefs.content,note);advanced.remove();}
      const upscale=settings.querySelector('.generation-upscale-disclosure');
      if(upscale){const row=group(settings,'Upscale','composer-final-upscale');move(row.content,'refine-upscale-options');upscale.remove();
        const sync=()=>{row.details.hidden=$('refine-upscale-options').hidden;};new MutationObserver(sync).observe($('refine-upscale-options'),{attributes:true,attributeFilter:['hidden']});sync();}
      move(sStyle.content,settings.querySelector('.refine-change-note'));
      const reuse=$('refine-copy-draft-prompt');reuse.textContent='Use draft prompt';reuse.title='Copy the draft prompt into refinement';sRefs.content.append(reuse);
    }
    for(const note of settings.querySelectorAll('.cutout-note,.refine-change-note'))if(!note.id.endsWith('-recommended')&&!note.classList.contains('generation-size-note'))quietNote(note);
    // Any vacated layout wrapper has no visual role; meaningful controls above
    // keep all handlers, values and native focus/undo behavior.
    old.forEach(node=>{if(node.parentElement===settings&&!node.matches('.composer-section')&&!node.querySelector('input,select,textarea,button'))node.remove();});
  }
  const recipes=refine.querySelector('.refine-recipe-panel');recipes.classList.add('composer-section');recipes.open=false;
  recipes.setAttribute('name','composer-refine');
  recipes.addEventListener('toggle',()=>{if(recipes.open)for(const row of inspector.querySelectorAll('.composer-section'))if(row!==recipes)row.open=false;});
  quietNote($('refine-recipe-note'));
  const refresh=$('refine-refresh-models');refresh.textContent='Refresh models';
  const syncRefineLabels=()=>{
    if($('refine-draft-run').textContent==='Generate draft')$('refine-draft-run').textContent='Generate';
    if($('refine-final-run').textContent==='Refine selected draft')$('refine-final-run').textContent='Refine';
  };
  const priorRefineControls=updateRefineControls;
  updateRefineControls=function(...args){const result=priorRefineControls.apply(this,args);syncRefineLabels();return result;};
  syncRefineLabels();
  const closeSections=()=>{for(const row of document.querySelectorAll('.composer-section'))row.open=false;};
  // Changing stage/mode should never reopen every setting panel from a prior
  // layout. Values stay intact; only disclosure state changes.
  for(const id of ['generation-stage-draft-tab','generation-stage-final-tab'])$(id).addEventListener('click',closeSections);
  new MutationObserver(closeSections).observe(document.body,{attributes:true,attributeFilter:['data-generation-view']});
  closeSections();
  window.LocalImageGenerationComposer={closeSections,sections:createRows};
})();
