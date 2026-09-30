// Optional interface density and task-based hardware guidance, without model downloads.
const UI_DENSITY_KEY='local-image.interface-density.v1';
function applyInterfaceDensity(value){
  const density=['compact','comfortable','large'].includes(value)?value:'comfortable';
  document.documentElement.dataset.uiDensity=density;
  try{localStorage.setItem(UI_DENSITY_KEY,density);}catch{}
  const select=$('interface-density');if(select)select.value=density;
  if(typeof resize==='function')requestAnimationFrame(()=>resize());
}
function setupInterfaceDensity(){
  if(window.__LOCAL_IMAGE_REACT__){let saved='comfortable';try{saved=localStorage.getItem(UI_DENSITY_KEY)||saved;}catch{}applyInterfaceDensity(saved);return;}
  const host=$('settings-dialog');if(!host)return;
  const row=document.createElement('div');row.className='interface-density';
  const label=document.createElement('label');label.htmlFor='interface-density';label.textContent='Interface size';
  const select=document.createElement('select');select.id='interface-density';
  for(const [value,text]of[['compact','Compact'],['comfortable','Comfortable'],['large','Large · 200% text']]){const option=document.createElement('option');option.value=value;option.textContent=text;select.append(option);}
  select.onchange=()=>applyInterfaceDensity(select.value);row.append(label,select);host.append(row);
  let saved='comfortable';try{saved=localStorage.getItem(UI_DENSITY_KEY)||saved;}catch{}applyInterfaceDensity(saved);
}
function starterTask(action){
  firstSetupPending=false;try{localStorage.setItem(FIRST_SETUP_SEEN,'1');localStorage.setItem(HARDWARE_GUIDE_SEEN,'1');}catch{}
  $('hardware-dialog').close();
  if(action==='setup'){$('settings').click();return;}
  $('workspace-'+(action==='generate'?'generate':action==='cutout'?'cutout':'retouch')).click();
  if(action==='retouch')$('mode-heal').click();
  if(action!=='generate'&&!session)$('empty-open').click();
}
function setupHardwareStarter(){
  if(window.__LOCAL_IMAGE_REACT__)return;
  const dialog=$('hardware-dialog'),table=dialog?.querySelector('.hardware-table');if(!dialog||!table)return;
  const note=document.createElement('p');note.id='hardware-start-note';note.className='hardware-start-note';
  note.textContent='Start with a photo repair now. Quick Heal and compositing work without a dedicated GPU. AI tools are optional and use a local engine.';
  const actions=document.createElement('div');actions.className='hardware-starter';
  for(const [action,label]of[['retouch','Repair a photo'],['cutout','Remove a background'],['generate','Create an image'],['setup','Set up AI']]){const button=document.createElement('button');button.id='starter-'+action;button.textContent=label;button.onclick=()=>starterTask(action);actions.append(button);}
  table.before(note,actions);
  const details=document.createElement('details');details.id='hardware-recommendations';details.className='hardware-recommendations';
  const summary=document.createElement('summary');summary.textContent='VRAM guidance by function and model';details.append(summary);table.before(details);details.append(table);
  const oldShow=showHardwareGuide;showHardwareGuide=async()=>{await oldShow();if(!dialog.open)return;try{const devices=($('hardware-device').textContent.match(/([\d.]+) GB VRAM/g)||[]).map(text=>parseFloat(text));const maximum=Math.max(0,...devices);note.textContent=maximum&&maximum<16?'CPU editing is ready. Your GPU is below our default-resolution planning estimates. Local AI may work through system-RAM offloading, with lower speed; start with a photo repair or inspect model choices before downloading.':'CPU editing is ready immediately. For quick AI experiments, compare Z-Image Turbo and Klein 4B in Browse models. Choose Qwen for background removal and transparent assets; memory guidance is below.';}catch{}};
  $('hardware-guide').onclick=showHardwareGuide;
}
setupInterfaceDensity();setupHardwareStarter();

let operationProgress=null,operationProgressPending=false;
window.localImageProgressText=fallback=>{
  const data=operationProgress;
  if(!busy||activeTask!=='generate'||!data?.active||data.connection_lost||data.updated_seconds_ago>8)return fallback;
  const elapsed=Math.max(0,Math.round(data.elapsed_seconds||0));
  if(data.progress&&Number.isFinite(data.progress.value)&&Number.isFinite(data.progress.max)&&data.progress.max>0)
    return (data.stage_label||'Sampling')+' · '+data.progress.value+'/'+data.progress.max+' steps · '+elapsed+'s';
  return (data.stage_label||'Working')+' · '+elapsed+'s'+(data.connection_lost?' · waiting for local engine':'');
};
setInterval(async()=>{
  if(!busy||activeTask!=='generate'){operationProgress=null;return;}
  if(operationProgressPending)return;operationProgressPending=true;
  try{operationProgress=await(await api('/api/local-remove/generation/progress')).json();}catch{operationProgress=null;}
  finally{operationProgressPending=false;}
},900);
