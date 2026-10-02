import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createFeatureAdapters } from '../src/editor/featureAdapters.ts';
import type { DocumentController } from '../src/editor/documentController.ts';
import type { NativeBridge } from '../src/editor/nativeBridge.ts';
function fixture() {
  const values=new Map<string,string>([['local-image.interface-density.v1','large']]);const calls:unknown[]=[];
  let fingerprint='a:1',pending=true,epoch=0;
  const snapshot={busy:false,askBeforeOverwrite:true,document:{id:'a',name:'Photo A',revision:3,cutout:{enabled:true}},collection:{id:'collection',local:false,entries:[{id:'entry-a',name:'Photo A',session_id:'a'},{id:'entry-b',name:'Photo B',session_id:null}]}};
  const capabilities={ready:true,projects:true,setup:true,batch:true,closeRequests:true,version:2};
  const docs={hasCutout:(id:string)=>id==='a',getSnapshot:()=>snapshot,getContext:()=>({documentId:snapshot.document.id,navigationEpoch:epoch,busy:snapshot.busy}),subscribe:()=>()=>{},
    canvas:{resetTransientInput:()=>calls.push('reset'),focus:()=>calls.push('focus'),cancelGesture:()=>calls.push('cancelGesture')},
    commands:{setOverwritePreference:(value:boolean)=>{snapshot.askBeforeOverwrite=value;values.set('local-remove-ask-before-overwrite',String(value));}},
    acceptConfiguration:(value:unknown)=>calls.push(['config',value]),setSettingsPending:(value:boolean)=>calls.push(['settingsPending',value]),setMenuOpen:(value:boolean)=>calls.push(['menu',value]),
    pendingSelection:(id:string)=>id==='a'&&pending,pendingFingerprint:()=>fingerprint,rememberCurrentView:()=>calls.push('remember'),flush:async()=>{},
    resolveCollectionSession:async(id:string)=>({session_id:id==='entry-a'?'a':'b',revision:3}),returnToPendingSelection:async(id:string)=>{calls.push(['pending',id]);}};
  const native={capabilities:()=>capabilities,setupChooseComfyDirectory:async()=>{calls.push('chooseRuntime');return null;},setupChooseInstallDirectory:async()=>null,
    setupInstall:async()=>null,setupChooseModelDirectory:async()=>null,setupDownloadModels:async()=>null,setupStart:async()=>null,setupEject:async()=>null,
    setupUseInstallation:async(id:string)=>{calls.push(['useInstallation',id]);return null;},configureAi:async()=>null,
    setupDownloadGenerationModel:async(model:string,variant:string)=>{calls.push(['model',model,variant]);return null;},loraDownload:async(value:unknown)=>{calls.push(['lora',value]);return null;},
    batchExportFolder:async(value:unknown)=>{calls.push(['batch',value]);return null as Record<string,unknown>|null;}};
  const adapters=createFeatureAdapters({documents:docs as unknown as DocumentController,native:native as unknown as NativeBridge,
    storage:{getItem:key=>values.get(key)??null,setItem:(key,value)=>{values.set(key,value);}},applyDensity:value=>calls.push(['density',value])});
  return{adapters,docs,native,snapshot,capabilities,calls,values,setFingerprint:(value:string)=>{fingerprint=value;},setPending:(value:boolean)=>{pending=value;},navigate:()=>{epoch++;}};
}
test('typed settings adapter preserves live overwrite preference and existing density storage',async()=>{
  const f=fixture();assert.deepEqual(f.adapters.settingsBridge.preferences(),{askBeforeOverwrite:true,density:'large'});
  f.adapters.applyStoredDensity();f.adapters.settingsBridge.setDensity('compact');f.adapters.settingsBridge.setOverwritePreference(false);
  assert.equal(f.values.get('local-remove-ask-before-overwrite'),'false');assert.equal(f.snapshot.askBeforeOverwrite,false);assert.equal(f.values.get('local-image.interface-density.v1'),'compact');
  await f.adapters.settingsBridge.chooseRuntime();await f.adapters.settingsBridge.useInstallation('candidate');
  assert.deepEqual(f.calls,[['density','large'],['density','compact'],'chooseRuntime',['useInstallation','candidate']]);
});
test('model adapter uses only fixed native actions and preserves null cancellation',async()=>{
  const f=fixture();assert.equal(await f.adapters.modelBridge.chooseModelDirectory(),null);
  await f.adapters.modelBridge.downloadModel('qwen','int8');await f.adapters.modelBridge.downloadLora({model:'qwen',repo_id:'publisher/style',filename:'style.safetensors',revision:'a'.repeat(40),allow_unverified:false});
  assert.deepEqual(f.calls,[['model','qwen','int8'],['lora',{model:'qwen',repo_id:'publisher/style',filename:'style.safetensors',revision:'a'.repeat(40),allow_unverified:false}]]);
  assert.equal('getLegacyLoraPort'in f.adapters.modelBridge,false);
});
test('settings model download reuses the existing native generator action',async()=>{
  const f=fixture();assert.equal(await f.adapters.settingsBridge.downloadModel('qwen','bf16'),null);
  await f.adapters.settingsBridge.downloadModel('z-image-turbo','bf16');
  assert.deepEqual(f.calls,[['model','qwen','bf16'],['model','z-image-turbo','bf16']]);
});
test('batch pending fingerprint invalidates acknowledgement even when selection remains nonempty',()=>{
  const f=fixture(),first=f.adapters.batchEditor.getSnapshot();assert.equal(first,f.adapters.batchEditor.getSnapshot());assert.ok(Object.isFrozen(first.pendingSelections));
  assert.equal(first.nativeCollection,true);assert.equal(first.nativeExportAvailable,true);assert.equal(first.document?.canSaveTreatment,true);
  f.setFingerprint('a:2');const second=f.adapters.batchEditor.getSnapshot();assert.notEqual(first,second);assert.equal(second.pendingSelections[0].fingerprint,'a:2');
  f.setPending(false);assert.equal(f.adapters.batchEditor.getSnapshot().pendingSelections.length,0);
  f.capabilities.batch=false;assert.equal(f.adapters.batchEditor.getSnapshot().nativeExportAvailable,false);
});
test('batch preparation uses explicit canvas/domain commands and rejects navigation during flush',async()=>{
  const f=fixture();await f.adapters.batchEditor.prepareForBatch();assert.deepEqual(f.calls,[['menu',false],'cancelGesture','remember']);
  assert.deepEqual(await f.adapters.batchEditor.resolveSession('entry-b'),{session_id:'b',revision:3});
  f.docs.flush=async()=>{f.navigate();};await assert.rejects(f.adapters.batchEditor.prepareForBatch(),/image changed/);
  f.snapshot.busy=true;await assert.rejects(f.adapters.batchEditor.prepareForBatch(),/Finish the current/);
});
test('native batch export cancellation never becomes success; replies must match the prepared queue',async()=>{
  const f=fixture(),request={job_id:'queue',item_ids:['one']};assert.equal(await f.adapters.batchEditor.exportBatchFolder!(request),null);
  assert.deepEqual(f.calls,[['batch',request]]);
  f.native.batchExportFolder=async()=>({id:'another-queue',modified:1,items:[]});await assert.rejects(f.adapters.batchEditor.exportBatchFolder!(request),/unexpected batch queue/);
  f.native.batchExportFolder=async()=>({id:'queue',modified:1,items:[]});assert.equal((await f.adapters.batchEditor.exportBatchFolder!(request))?.id,'queue');
  f.capabilities.batch=false;await assert.rejects(f.adapters.batchEditor.exportBatchFolder!(request),/does not support/);
});
