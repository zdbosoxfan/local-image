/* Fixed settings operations over the existing native bridge. The one native
 * listener, request correlation, picker cancellation and deadlines stay owned
 * by editor.js; this module grants no generic action or filesystem access. */
(()=>{
  'use strict';
  if(!window.__LOCAL_IMAGE_REACT__||window.LocalImageSettingsBridge)return;
  const preference=(key,fallback)=>{try{return localStorage.getItem(key)??fallback;}catch{return fallback;}};
  const request=(action,details={})=>{
    if(action==='configureAi'?!nativeReady:!nativeSetup)return Promise.reject(Error('This setup command requires the current Local Image desktop host.'));
    return nativeRequest(action,null,details);
  };
  window.LocalImageSettingsBridge=Object.freeze({
    capabilities:()=>({ready:nativeReady,setup:nativeSetup}),
    editorBusy:()=>busy||closeInProgress,
    preferences:()=>({askBeforeOverwrite,density:['compact','comfortable','large'].includes(preference('local-image.interface-density.v1','comfortable'))?preference('local-image.interface-density.v1','comfortable'):'comfortable'}),
    acceptConfiguration:value=>{
      if(value.settings){models=value.settings.models.filter(item=>['klein','heal'].includes(item.id));modelId='klein';settingsLoaded=true;}
      if(value.status){ready=value.status.ready===true;retouchReady=value.status.retouch_ready===true;}
      if('qwen' in value)qwenStatus=value.qwen;
      if(value.setup)setupState=value.setup;
      controls();publishEditorState();
    },
    setOperationPending:value=>{settingsSaving=value;setupRequestBusy=value;controls();publishEditorState();},
    setOverwritePreference:value=>{askBeforeOverwrite=!!value;try{localStorage.setItem(OVERWRITE_PREFERENCE,String(!!value));}catch{}publishEditorState();},
    setDensity:value=>{const density=['compact','comfortable','large'].includes(value)?value:'comfortable';document.documentElement.dataset.uiDensity=density;try{localStorage.setItem('local-image.interface-density.v1',density);}catch{}requestAnimationFrame(()=>{if(typeof resize==='function')resize();});},
    focusCanvas:()=>{resetTransientInput();viewport.focus({preventScroll:true});},
    chooseRuntime:()=>request('setupChooseComfyDirectory'),
    chooseInstallDirectory:()=>request('setupChooseInstallDirectory'),
    installRuntime:()=>request('setupInstall'),
    chooseModelDirectory:()=>request('setupChooseModelDirectory'),
    downloadRemovalModels:()=>request('setupDownloadModels'),
    downloadModel:(model,variant)=>request('setupDownloadGenerationModel',{model,variant}),
    startBackend:()=>request('setupStart'),
    ejectModels:()=>request('setupEject'),
    useInstallation:id=>request('setupUseInstallation',{installation_id:id}),
    configureConnection:()=>request('configureAi'),
  });
})();
