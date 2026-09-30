// Exercise the shipped boundary against controlled documents. Composed-page
// browser coverage is still required for ownership, CSS, focus and pointer UI.
const assert=require('node:assert/strict');
const fs=require('node:fs');
const path=require('node:path');
const vm=require('node:vm');
const bridgeSource=fs.readFileSync(path.join(__dirname,'../backend/frontend/migration-bridge.js'),'utf8');
const layersSource=fs.readFileSync(path.join(__dirname,'../backend/frontend/layers-studio.js'),'utf8');
const shellSource=fs.readFileSync(path.join(__dirname,'../backend/frontend/studio-shell.js'),'utf8');

async function testBridge(){
  const calls=[];
  const element={focus:()=>calls.push('focus')};
  const message={textContent:'Ready',classList:{contains:()=>false}};
  const layers={selected:()=>({id:'original'}),select:id=>calls.push(['select',id]),patchLayer:(id,change)=>calls.push(['patch',id,change]),
    createRetouch(){},reorderSelected(){},restoreLayer(){},mergeLayers(){},addMask(){},cancelMove:()=>calls.push('cancelMove'),setStackTransport(){},showAssets(){}};
  const context=vm.createContext({window:{__LOCAL_IMAGE_REACT__:true,LocalImageLayers:layers},
    document:{body:{dataset:{}}},
    session:{id:'one',revision:1,layer_stack:[{id:'original',opacity:1}]},busy:false,closeInProgress:false,workspace:'retouch',showOriginal:false,tool:'brush',
    nativeReady:true,nativeProjects:true,nativeSetup:false,hasSelection:false,points:[],viewport:element,
    cloneDocument:data=>JSON.parse(JSON.stringify(data)),$:()=>message,layerChangesPending:()=>false,historyTarget:()=> 'selection',
    endGesture:()=>calls.push('endGesture'),fitImage:()=>calls.push('fit'),setPhotoZoom:value=>calls.push(['zoom',value]),resetTransientInput:()=>calls.push('reset'),
    openDocumentFiles:()=>calls.push('openFiles'),openDocumentFolder:()=>calls.push('openFolder'),openDocumentProject:()=>calls.push('openProject'),
    modalOpen:()=>false,saveEditableProject:(data,options)=>calls.push(['saveProject',data.id,options.saveAs]),
    undoEdit:()=>calls.push('undo'),redoEdit:()=>calls.push('redo'),selectTool(){},setWorkspace(){},paintPhoto(){},showSettings:()=>calls.push('settings')});
  vm.runInContext(bridgeSource,context);
  const bridge=context.window.LocalImageLegacyEditor;
  const first=bridge.getSnapshot();
  assert.equal(first,bridge.getSnapshot(),'Unchanged snapshots keep identity for useSyncExternalStore');
  assert.ok(Object.isFrozen(first.document.layer_stack[0]),'Accepted document data cannot be mutated by feature components');
  assert.notEqual(first.document,context.session,'Browser integration does not expose the live legacy session');
  let notifications=0;
  const unsubscribe=bridge.subscribe(()=>notifications++);
  bridge.publish();assert.equal(notifications,0);
  context.session={...context.session,revision:2};bridge.publish();
  assert.equal(notifications,1);assert.equal(bridge.getSnapshot().document.revision,2);
  assert.equal(first.document.revision,1,'Earlier accepted snapshots remain immutable');
  context.session.project_dirty=true;bridge.publish();assert.equal(notifications,2);
  assert.equal(bridge.getSnapshot().document.project_dirty,true,'In-place save/dirty metadata gets a new accepted copy');
  context.session.revision=3;bridge.publish();assert.equal(notifications,3);
  assert.equal(bridge.getSnapshot().document.revision,3,'In-place revision changes are observed');
  context.busy=true;bridge.publish();assert.equal(notifications,4);unsubscribe();
  context.busy=false;bridge.publish();assert.equal(notifications,4);
  bridge.commands.undo();bridge.commands.fit();bridge.commands.openFiles();bridge.commands.showSettings();
  bridge.commands.patchLayer('original',{opacity:0});bridge.native.saveProject(true);
  bridge.canvas.cancelGesture();
  assert.equal(bridge.canvas.element,element,'The original canvas viewport stays mounted');
  assert.deepEqual(calls,['undo','endGesture','fit','openFiles','settings',['patch','original',{opacity:0}],['saveProject','one',true],'reset','cancelMove']);
  bridge.commands.toggleInspector();assert.equal(bridge.getSnapshot().inspectorHidden,true);
  bridge.commands.toggleInspector();assert.equal(bridge.getSnapshot().inspectorHidden,false);
  const oldCallCount=calls.length;
  context.workspace='generate';bridge.commands.undo();bridge.commands.redo();
  assert.equal(bridge.getSnapshot().canUndo,false);assert.equal(bridge.getSnapshot().canRedo,false);
  context.workspace='retouch';context.window.LocalImageGenerationStudio={isRefining:()=>true};
  bridge.commands.undo();bridge.commands.redo();bridge.commands.toggleInspector();
  assert.equal(bridge.getSnapshot().inspectorHidden,false,'Refinement keeps its inspector layout');
  assert.equal(calls.length,oldCallCount,'Direct command calls cannot edit hidden document selection history');
  // Exercise the actual existing chrome synchronization entry point: mode and
  // result changes do not necessarily call editor.js controls().
  const shellStart=shellSource.indexOf('  function syncChrome(){'),shellEnd=shellSource.indexOf('  const priorControls=',shellStart);
  assert.ok(shellStart>=0&&shellEnd>shellStart);
  Object.assign(context,{reactShell:true,heading:{},shortcut:{},composition:{},compositionRows:{},
    publishEditorState:()=>bridge.publish(),closeMenus:()=>{},save:mode=>calls.push(['export',mode,context.session.id])});
  vm.runInContext(shellSource.slice(shellStart,shellEnd),context);
  context.workspace='generate';
  context.window.LocalImageGenerationStudio={isRefining:()=>false,isCreatingBlank:()=>true,hasVisibleDocument:()=>false,prepareSelectedForExport:async()=>false};
  let chromeNotifications=0;const stopChrome=bridge.subscribe(()=>chromeNotifications++);
  vm.runInContext('syncChrome()',context);
  assert.equal(chromeNotifications,1);assert.equal(bridge.getSnapshot().creatingBlank,true);
  assert.equal(bridge.getSnapshot().generationVisible,false,'A hidden retained document cannot enable Export in blank Create mode');
  const exportsBefore=calls.filter(call=>Array.isArray(call)&&call[0]==='export').length;
  await bridge.commands.exportImage();
  assert.equal(calls.filter(call=>Array.isArray(call)&&call[0]==='export').length,exportsBefore);
  context.window.LocalImageGenerationStudio={isRefining:()=>true,isCreatingBlank:()=>false,hasVisibleDocument:()=>true,
    prepareSelectedForExport:async()=>{context.session={id:'selected-result',revision:0};return true;}};
  vm.runInContext('syncChrome()',context);
  assert.equal(chromeNotifications,2);assert.equal(bridge.getSnapshot().generationVisible,true);assert.equal(bridge.getSnapshot().refining,true);
  vm.runInContext('syncChrome()',context);assert.equal(chromeNotifications,2,'Repeated chrome sync does not notify or recurse without state changes');
  await bridge.commands.exportImage();assert.deepEqual(calls.at(-1),['export','export','selected-result'],'Export prepares the selected result before saving');
  stopChrome();
  assert.deepEqual(Object.keys(bridge.native).sort(),['capabilities','openFiles','openFolder','openProject','saveProject'],'No generic privileged native action proxy');
  const legacy=vm.createContext({window:{__LOCAL_IMAGE_REACT__:false}});
  vm.runInContext(bridgeSource,legacy);assert.equal(legacy.window.LocalImageLegacyEditor,undefined,'Legacy rollback does not install the migration boundary');
}

async function testStaleMutation(){
  const start=layersSource.indexOf('  async function mutate('),end=layersSource.indexOf('  async function ensureStack(',start);
  assert.ok(start>=0&&end>start);
  let resolveResponse,transportCalls=0;
  const requested={id:'first',revision:4,layer_stack:[]};
  const context=vm.createContext({session:requested,busy:false,documentNavigationEpoch:1,selectedIds:new Map(),assetCache:new Map(),openDocuments:new Map(),renderKey:'',
    visibleDocument:()=>true,nodes:()=>[],setBusy:value=>{context.busy=value;},
    stackTransport:()=>{transportCalls++;return new Promise(resolve=>resolveResponse=resolve);},
    trackDocument:data=>context.openDocuments.set(data.id,data),refreshPreview:()=>{throw Error('Stale response must not repaint active canvas');},
    layerList(){},updateCollectionSession(){},renderCollection(){},message:()=>{}});
  vm.runInContext(layersSource.slice(start,end),context);
  const pending=vm.runInContext("mutate('/stack/layer/original',{opacity:0},'PATCH')",context);
  context.session={id:'second',revision:0};context.documentNavigationEpoch=2;context.busy=false;
  resolveResponse({...requested,revision:5});await pending;
  assert.equal(context.session.id,'second');assert.equal(context.openDocuments.get('first').revision,5);
  assert.equal(context.busy,false,'Earlier mutation does not overwrite newer navigation busy state');
  context.session=requested;context.documentNavigationEpoch=3;
  const old=vm.runInContext("mutate('/stack/layer/original',{opacity:0},'PATCH')",context);
  context.openDocuments.set('first',{...requested,revision:7});
  resolveResponse({...requested,revision:6});await old;
  assert.equal(context.openDocuments.get('first').revision,7,'Late lower revisions do not replace accepted documents');
  assert.equal(transportCalls,2,'Mutating requests are never retried');
}

(async()=>{await testBridge();await testStaleMutation();console.log('PASS: immutable stable snapshots, subscriptions, direct shared commands, retained canvas, constrained native facade, rollback, stale navigation/revision rejection and no mutation retry.');})().catch(error=>{console.error(error);process.exitCode=1;});
