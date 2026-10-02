import {createCanvasController} from './editor/canvasController.ts';
import {BRUSH_STEPS} from './editor/canvasMath.ts';
import {createNativeBridge} from './editor/nativeBridge.ts';
import {createDocumentController,type DocumentController} from './editor/documentController.ts';
import {createFeatureAdapters} from './editor/featureAdapters.ts';
import {createGenerationAdapters} from './editor/generationAdapters.ts';
import {createBrowserPorts,installFileDrop} from './editor/browserPorts.ts';
import {installKeyboardController} from './editor/keyboardController.ts';
import {createShellController,type ShellController} from './features/shell/shellController.ts';
import {createEditorDialogs} from './features/shell/editorDialogs.ts';
import {createCommandExecutor} from './features/shell/executeCommand.ts';
import {createSettingsController,type SettingsController} from './features/settings/index.ts';
import {createModelsController,type ModelsController} from './features/models/index.ts';
import {createBatchController,type BatchController} from './features/batch/index.ts';
import {createAssetsController} from './features/assets/index.ts';
import {createGenerationController} from './features/generation/index.ts';
import type {GenerationModel} from './features/generation/types.ts';
import type {InterfaceDensity} from './features/settings/types.ts';
import type {FullAppProps} from './FullApp.tsx';

const required=<T extends HTMLElement>(id:string):T=>{const element=document.getElementById(id);if(!element)throw Error('The editor is missing its '+id+' mount.');return element as T;};
const loadImage=(url:string)=>new Promise<HTMLImageElement>((resolve,reject)=>{const image=new Image();image.onload=()=>{void image.decode().then(()=>resolve(image),()=>reject(Error('Could not decode the photo preview.')));};image.onerror=()=>reject(Error('Could not load the photo preview.'));image.src=url;});

export function createApplication(token:string) {
 let controller:DocumentController|undefined,shell:ShellController|undefined,settings:SettingsController|undefined,models:ModelsController|undefined,batch:BatchController|undefined;
 const browser=createBrowserPorts();
 const report=(error:unknown)=>controller?.report(error instanceof Error?error.message:String(error),true);
 const dialogs=createEditorDialogs({dontAskBeforeOverwrite:()=>controller?.commands.setOverwritePreference(false),focusCanvas:()=>canvas.focus(),copy:text=>navigator.clipboard.writeText(text),downloadCredits:()=>controller?.downloadCredits()});
 const modalOpen=()=>dialogs.isOpen()||!!settings?.isOpen()||!!models?.isOpen()||!!batch?.isOpen()||!!shell?.getSnapshot().backgroundGenerator;
 const native=createNativeBridge({onCloseRequest:()=>controller?.closeAll()??false,onError:report});
 const canvas=createCanvasController({viewport:required('viewport'),stage:required('stage'),photo:required<HTMLCanvasElement>('photo'),photoImage:required<HTMLImageElement>('photo-image'),overlay:required<HTMLCanvasElement>('selection'),draft:required<HTMLCanvasElement>('draft'),layerStack:required('layer-stack'),brushCursor:required('brush-cursor')},
  {getAcceptedDocument:()=>controller?.getAcceptedDocument()??null,isModalOpen:modalOpen,isMenuOpen:()=>shell?.isMenuOpen()??false,isEditorHidden:()=>!!controller&&(!!controller.getSnapshot().creatingBlank||!!controller.getSnapshot().refining),
   commitLayerTransform:value=>controller!.commitLayerTransform(value),commitLegacyCutoutTransform:value=>controller!.commitLegacyCutoutTransform(value),
   restoreInteraction:value=>controller?.restoreInteraction(value),report:(message,error)=>controller?.report(message,error),loadImage});
 controller=createDocumentController({token,canvas,native,dialogs,browser,storage:localStorage,loadImage,isModalOpen:modalOpen});
 const documents=controller;
 shell=createShellController({menuChanged:documents.setMenuOpen,focusCanvas:canvas.focus});
 const shellUi=shell;
 const applyDensity=(value:InterfaceDensity)=>{document.documentElement.dataset.uiDensity=value;requestAnimationFrame(()=>canvas.resize());};
 const featureAdapters=createFeatureAdapters({documents,native,storage:localStorage,applyDensity});
 applyDensity(featureAdapters.settingsBridge.preferences().density);
 const generationAdapters=createGenerationAdapters({document:documents,native,openModels:options=>{void models!.openModels(options);},openLoras:port=>{void models!.openLoras(port);}});
 const assets=createAssetsController(generationAdapters.assetsHost,token);
 const generation=createGenerationController(generationAdapters.generationHost,token);
 models=createModelsController({token,bridge:featureAdapters.modelBridge,onCatalog:value=>generation.acceptCatalog(value as unknown as GenerationModel[]),onSetup:value=>documents.acceptConfiguration({setup:value})});
 const modelController=models;
 settings=createSettingsController({token,bridge:featureAdapters.settingsBridge,browseModels:()=>generationAdapters.featureCommands.browseModels?.(),startTask:async workspace=>{documents.commands.setWorkspace(workspace);if(workspace!=='generate'&&!documents.getSnapshot().document){await documents.commands.openFiles();documents.commands.setWorkspace(workspace);}}});
 const settingsController=settings;
 batch=createBatchController({token,editor:featureAdapters.batchEditor});
 const batchController=batch;
 generationAdapters.bind(generation,assets);
 documents.setFeatures({...generationAdapters.featureCommands,showSettings:()=>settingsController.open('settings'),showHardware:()=>settingsController.open('hardware'),showShortcuts:()=>settingsController.open('shortcuts'),showBatch:()=>batchController.open(),edgeOptions:()=>shellUi.showCutoutProperties(),generateBackground:()=>shellUi.showBackgroundGenerator(),renameLayer:id=>shellUi.renameLayer(id)});
 const execute=createCommandExecutor(documents,shellUi.closeMenus);
 const mounts:FullAppProps['mounts']={tools:required('tools-root'),assets:required('assets-root'),document:required('document-root'),progress:required('progress-root'),empty:required('empty-root'),filmstrip:required('filmstrip-root'),inspector:required('inspector-root'),generation:required('react-generation-root'),status:required('status-root')};
 const updateView=()=>{
  const state=documents.getSnapshot(),view=generationAdapters.getSnapshot();
  document.body.dataset.persona=state.workspace;document.body.dataset.generationView=view.active?view.mode:'editor';
  document.body.dataset.generationBlank=String(view.creatingBlank);document.body.dataset.assetsOpen=String(view.assetsOpen);
  mounts.tools.hidden=state.workspace==='generate';mounts.inspector.hidden=state.workspace==='generate'||!!state.inspectorHidden;
  mounts.assets.hidden=!view.assetsOpen;mounts.generation.hidden=state.workspace!=='generate';
  mounts.empty.hidden=!!state.document&&!view.creatingBlank;
  document.title=state.document&&!view.creatingBlank&&!view.refining?state.document.name+' — Local Image':'Local Image';
 };
 const unsubscribes=[documents.subscribe(updateView),generationAdapters.subscribe(updateView),native.subscribe(()=>{settingsController.refreshCapabilities();modelController.refreshCapabilities();})];
 updateView();
 const removeKeyboard=installKeyboardController(document,{getState:()=>{const state=documents.getSnapshot(),view=generationAdapters.getSnapshot();return {modalOpen:modalOpen(),menuOpen:shellUi.isMenuOpen(),hasDocument:state.workspace==='generate'?view.generationVisible:!!state.document,hiddenEditor:view.refining||view.creatingBlank,canReturn:!!state.document?.can_return,busy:state.busy,showOriginal:state.showOriginal,workspace:state.workspace,tool:state.tool,handActive:state.tools.handActive};},execute,selectTool:documents.toolActions.tool,navigate:documents.navigateCollection,stepBrush:direction=>{const current=documents.getSnapshot().tools.brushSize;const next=direction>0?BRUSH_STEPS.find(size=>size>current):[...BRUSH_STEPS].reverse().find(size=>size<current);documents.toolActions.brushSize(next??(direction>0?2000:1));},setSpaceHeld:canvas.setSpaceHeld,cancelPen:()=>{if(canvas.getSnapshot().gesture==='move')canvas.cancelGesture();else canvas.cancelPen();},openMenu:shellUi.openMenu,toggleMenuFocus:()=>shellUi.toggleMenuFocus(!!document.activeElement?.closest('[role="menubar"]')),closeMenus:shellUi.closeMenus,resetTransientInput:canvas.resetTransientInput,report});
 const removeDrop=installFileDrop(document.body,required('drop-notice'),{allowed:()=>!documents.getSnapshot().busy&&!modalOpen(),nativeReady:()=>native.capabilities().ready,navigationIdentity:()=>{const context=documents.getContext();return context.navigationEpoch+':'+context.documentId;},drop:documents.drop,report});
 const beforeUnload=(event:BeforeUnloadEvent)=>{if(documents.hasUnsavedWork()){event.preventDefault();event.returnValue='';}};
 window.addEventListener('beforeunload',beforeUnload);
 return {
  props:{controller:documents,shell:shellUi,dialogs,assets,generation,batch:batchController,settings:settingsController,models:modelController,mounts,execute} satisfies FullAppProps,
  native,canvas,
  async initialize(){await documents.initialize(location.search);await generation.refreshModels();document.body.dataset.reactReady='true';await settingsController.maybeFirstRun();},
  dispose(){removeKeyboard();removeDrop();window.removeEventListener('beforeunload',beforeUnload);unsubscribes.forEach(unsubscribe=>unsubscribe());generationAdapters.dispose();generation.dispose();assets.dispose();batchController.dispose();settingsController.dispose();modelController.dispose();dialogs.dispose();documents.dispose();canvas.dispose();native.dispose();browser.dispose();},
 };
}
export type Application=ReturnType<typeof createApplication>;
