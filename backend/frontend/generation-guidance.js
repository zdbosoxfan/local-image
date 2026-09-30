/* Audited sampling recommendations, with publisher/preset sources on demand. */
(() => {
  const $=id=>document.getElementById(id);
  const rows=[{note:'gen-sampling-note',model:()=>generationModel()},
    ...['draft','final'].map(stage=>({note:'refine-'+stage+'-recommended',model:()=>refineModel(stage)}))];
  for(const row of rows){
    row.element=$(row.note);if(!row.element)continue;
    row.info=document.createElement('button');row.info.className='generation-guidance-info';row.info.type='button';row.info.textContent='i';
    row.info.setAttribute('aria-label','Recommended steps and source');
    row.popup=document.createElement('div');row.popup.className='composer-info-popover';row.popup.setAttribute('popover','auto');
    row.element.after(row.info,row.popup);
    row.info.onclick=()=>{
      const data=row.model()?.sampling_guidance;if(!data)return;
      row.popup.replaceChildren();
      const heading=document.createElement('strong');heading.textContent=data.steps.source_label;
      const note=document.createElement('p');note.textContent=data.steps.note;
      const source=document.createElement('a');source.textContent='View source';source.href=data.steps.source_url;source.target='_blank';source.rel='noopener noreferrer';
      row.popup.append(heading,note,source);
      if(row.popup.matches(':popover-open'))row.popup.hidePopover();else{row.popup.showPopover();const rect=row.info.getBoundingClientRect();row.popup.style.left=Math.max(8,Math.min(innerWidth-310,rect.right-290))+'px';row.popup.style.top=Math.max(8,Math.min(innerHeight-row.popup.offsetHeight-8,rect.bottom+6))+'px';}
    };
  }
  function sync(){for(const row of rows){if(!row.element)continue;const data=row.model()?.sampling_guidance;
    const text=data?'Recommended: '+data.steps.recommended+' steps':'No published recommendation available';
    if(row.element.textContent!==text)row.element.textContent=text;
    row.info.hidden=!data;row.info.title=data?.steps.source_label||'';
  }}
  const generation=updateGenerationControls;updateGenerationControls=function(...args){const result=generation.apply(this,args);sync();return result;};
  const refine=updateRefineControls;updateRefineControls=function(...args){const result=refine.apply(this,args);sync();return result;};
  sync();
})();
