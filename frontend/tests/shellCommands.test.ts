import {test} from 'node:test';
import assert from 'node:assert/strict';
import {commandCatalog, type ShellView} from '../src/features/shell/commandCatalog.ts';

const base: ShellView = {document:null,selectedLayerId:null,busy:false,workspace:'retouch',showOriginal:false,canUndo:false,canRedo:false,status:'',generationVisible:false};
test('shell availability respects hidden generation documents and source-save ownership',()=>{
  const current={...base,document:{id:'a',name:'Photo',revision:0,width:10,height:10,can_return:true},generationVisible:true};
  assert.equal(commandCatalog(current).overwrite.visible,true);
  const hidden={...current,workspace:'generate' as const,creatingBlank:true,generationVisible:false};
  assert.equal(commandCatalog(hidden).overwrite.visible,false);
  assert.equal(commandCatalog(hidden).saveProject.enabled,false);
  assert.equal(commandCatalog(hidden).mergeLayers.enabled,false);
  assert.equal(commandCatalog(hidden).closeImage.enabled,false);
  for(const id of ['fit','actualSize','zoomIn','zoomOut','toggleOriginal'] as const)assert.equal(commandCatalog(hidden)[id].enabled,false);
  const visible={...current,workspace:'generate' as const};
  for(const id of ['fit','actualSize','zoomIn','zoomOut','toggleOriginal'] as const)assert.equal(commandCatalog(visible)[id].enabled,true);
  for(const id of ['fit','actualSize','zoomIn','zoomOut','toggleOriginal'] as const)assert.equal(commandCatalog({...visible,refining:true})[id].enabled,false);
});
test('original and locked layers keep backend operation restrictions',()=>{
  const layer={id:'original',name:'Original',kind:'original' as const,visible:true,locked:true,opacity:1,transform:{offset_x:0,offset_y:0,scale:1,rotation:0}};
  const current={...base,document:{id:'a',name:'Photo',revision:0,width:10,height:10,layer_stack:[layer]},selectedLayerId:'original',generationVisible:true};
  assert.equal(commandCatalog(current).deleteLayer.enabled,false);
  assert.equal(commandCatalog(current).layerUp.enabled,false);
  assert.equal(commandCatalog(current).toggleLayerLock.label,'Unlock layer');
  assert.equal(commandCatalog(current).renameLayer.enabled,true);
  assert.equal(commandCatalog({...current,busy:true}).renameLayer.enabled,false);
});

test('batch background removal is only offered in Cutout', () => {
  for (const workspace of ['retouch', 'generate', 'cutout'] as const) {
    const commands = commandCatalog({workspace, busy:false} as any);
    assert.equal(commands.showBatch.visible, workspace === 'cutout');
    assert.equal(commands.showBatch.enabled, workspace === 'cutout');
  }
});
