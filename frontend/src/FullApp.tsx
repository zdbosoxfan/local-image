import {useSyncExternalStore} from 'react';
import {Button,Portal,ProgressBar,Toolbar,ToolbarButton} from '@fluentui/react-components';
import type {DocumentController} from './editor/documentController.ts';
import {BatchDialog,type BatchController} from './features/batch/index.ts';
import {SettingsDialogs,type SettingsController} from './features/settings/index.ts';
import {ModelDialogs,type ModelsController} from './features/models/index.ts';
import {AssetsDock,type AssetsController} from './features/assets/index.ts';
import {GenerationPanel,type GenerationController} from './features/generation/index.ts';
import {MenuBar} from './features/shell/MenuBar.tsx';
import {ToolOptions,ToolRail,WorkspaceTabs} from './features/shell/ToolControls.tsx';
import {DocumentBar,Filmstrip,StatusBar} from './features/shell/DocumentChrome.tsx';
import {Layers} from './features/shell/Layers.tsx';
import {Icon} from './features/shell/Icon.tsx';
import {CutoutProperties,BackgroundGenerator} from './features/shell/CutoutProperties.tsx';
import {EditorDialogs} from './features/shell/EditorDialogs.tsx';
import type {EditorDialogsController} from './features/shell/editorDialogs.ts';
import type {ShellController} from './features/shell/shellController.ts';
import {commandCatalog,type ShellCommand} from './features/shell/commandCatalog.ts';

export interface FullAppProps {
 controller:DocumentController;shell:ShellController;dialogs:EditorDialogsController;
 assets:AssetsController;generation:GenerationController;batch:BatchController;settings:SettingsController;models:ModelsController;
 mounts:Record<'tools'|'assets'|'document'|'progress'|'empty'|'filmstrip'|'inspector'|'generation'|'status',HTMLElement>;
 execute(command:ShellCommand):unknown;
}
export function FullApp({controller,shell,dialogs,assets,generation,batch,settings,models,mounts,execute}:FullAppProps) {
 const state=useSyncExternalStore(controller.subscribe,controller.getSnapshot),ui=useSyncExternalStore(shell.subscribe,shell.getSnapshot);
 const commands=commandCatalog(state);
 const variant=state.health.qwen.variants.find(value=>value.id===state.tools.qwenVariant),klein=state.health.models.find(value=>value.id==='klein');
 const needsQwen=state.workspace==='cutout'&&!state.tools.maskReady||state.workspace==='retouch'&&state.tools.operation==='ai'&&state.tools.aiProvider==='qwen';
 const needsKlein=state.workspace==='retouch'&&state.tools.operation==='ai'&&state.tools.aiProvider==='klein';
 const unavailableReason=needsQwen&&(!state.health.qwen.ready||!variant?.available)?variant?.reason||state.health.qwen.reason||'Qwen Image 2.1 is not ready.':needsKlein&&(!state.health.ready||!klein?.available)?klein?.reason||'FLUX Klein is not ready.':undefined;
 const entries=(state.collection?.entries??[]).map((entry,index)=>({id:entry.id,name:entry.name,thumbnail:entry.thumbnail??null,dirty:!!entry.dirty,projectDirty:!!entry.project_dirty,selected:index===state.collectionIndex}));
 return <>
  <header className="li-app-header"><span className="li-brand">Local Image</span><MenuBar state={state} execute={execute} setOutputFormat={value=>controller.commands.setOutputFormat(value)} openRecent={controller.commands.openRecent} opened={ui.menu} focusRequest={ui.menuFocus} setMenu={shell.setMenu}/></header>
  <div className="li-workspace-bar"><WorkspaceTabs state={state.tools} actions={controller.toolActions}/><Toolbar className="li-commandbar" aria-label="Editor commands" size="small">
   <ToolbarButton aria-label="Undo" title={state.undoLabel+' (Ctrl+Z)'} icon={<Icon name="undo"/>} disabled={!commands.undo.enabled} onClick={()=>execute('undo')}/>
   <ToolbarButton aria-label="Redo" title={state.redoLabel+' (Ctrl+Shift+Z)'} icon={<Icon name="redo"/>} disabled={!commands.redo.enabled} onClick={()=>execute('redo')}/>
   <span className="li-divider"/><ToolbarButton disabled={!commands.fit.enabled} onClick={()=>execute('fit')}>Fit</ToolbarButton>
   <ToolbarButton icon={<Icon name="image"/>} disabled={!commands.showAssets.enabled} onClick={()=>execute('showAssets')}>Assets</ToolbarButton>
   <ToolbarButton id="batch-open" disabled={!commands.showBatch.enabled} onClick={()=>execute('showBatch')}>Batch</ToolbarButton>
   <ToolbarButton aria-pressed={!state.inspectorHidden} aria-controls="inspector-root" disabled={state.refining||state.workspace==='generate'} onClick={controller.commands.toggleInspector}>Inspector</ToolbarButton>
   <ToolbarButton disabled={!commands.exportImage.enabled} onClick={()=>execute('exportImage')}>Export</ToolbarButton>
   <ToolbarButton aria-label="Settings" title="Settings" icon={<Icon name="settings"/>} disabled={!commands.showSettings.enabled} onClick={()=>execute('showSettings')}/>
  </Toolbar></div>
  <ToolOptions state={state.tools} actions={controller.toolActions} menuOpen={ui.popup==='background'} setMenuOpen={open=>shell.setPopup('background',open)} unavailableReason={unavailableReason} openSetup={()=>void settings.open('settings','ai')}/>
  <Portal mountNode={mounts.tools}><ToolRail state={state.tools} actions={controller.toolActions}/></Portal>
  <Portal mountNode={mounts.assets}><AssetsDock controller={assets}/></Portal>
  <Portal mountNode={mounts.document}>{!state.creatingBlank&&<DocumentBar state={state.chrome} actions={controller.chromeActions}/>}</Portal>
  <Portal mountNode={mounts.progress}>{state.operationLabel&&<div className="li-operation-progress" role="status"><span>{state.operationLabel}…</span><ProgressBar aria-label={state.operationLabel}/></div>}</Portal>
  <Portal mountNode={mounts.empty}>{(!state.document||state.creatingBlank)&&<div className="li-empty-canvas">{state.workspace==='generate'?<span>Create a new image</span>:<><Icon name="image"/><Button disabled={state.busy} onClick={()=>execute('openFiles')}>Open image…</Button><span>Drop images or an editable project here</span></>}</div>}</Portal>
  <Portal mountNode={mounts.filmstrip}><Filmstrip name={state.collection?.name??''} entries={entries} busy={state.busy} collapsed={state.filmstripCollapsed} thumbnailSize={state.thumbnailSize} onThumbnailSize={controller.setThumbnailSize} onCollapsed={controller.setFilmstripCollapsed} onSelect={id=>controller.openCollectionEntry(state.collection!.entries.findIndex(entry=>entry.id===id))} onPrevious={()=>controller.navigateCollection(-1)} onNext={()=>controller.navigateCollection(1)}/></Portal>
  <Portal mountNode={mounts.inspector}><Layers controller={controller} renameRequest={ui.rename} menuOpen={ui.popup==='layers'} setMenuOpen={open=>shell.setPopup('layers',open)}/><CutoutProperties controller={controller} shell={shell}/></Portal>
  <Portal mountNode={mounts.generation}><GenerationPanel controller={generation}/></Portal>
  <Portal mountNode={mounts.status}><StatusBar state={state.chrome} actions={controller.chromeActions}/></Portal>
  <BatchDialog controller={batch}/><SettingsDialogs controller={settings}/><ModelDialogs controller={models}/><EditorDialogs controller={dialogs}/><BackgroundGenerator controller={controller} shell={shell}/>
 </>;
}
