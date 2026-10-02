import type { EditorSnapshot } from '../../contracts.ts';

export type ShellCommand = 'newWorkspace' | 'openFiles' | 'openFolder' | 'openProject' | 'saveProject' | 'saveProjectAs' | 'closeImage'
  | 'overwrite' | 'saveUnique' | 'exportImage' | 'credits' | 'showAssets' | 'showGenerated' | 'showBatch'
  | 'undo' | 'redo' | 'applySelection' | 'clearSelection' | 'finishPath' | 'showSettings'
  | 'createRetouch' | 'addMask' | 'renameLayer' | 'toggleLayerLock' | 'layerUp' | 'layerDown' | 'deleteLayer'
  | 'restoreLayer' | 'mergeLayers' | 'fit' | 'actualSize' | 'zoomIn' | 'zoomOut' | 'toggleOriginal' | 'refreshView'
  | 'showHardware' | 'showShortcuts';
export interface CommandItem { id: ShellCommand; label: string; shortcut?: string; enabled: boolean; visible?: boolean; checked?: boolean }
export interface ShellView extends EditorSnapshot {
  outputFormat?: string; penPointCount?: number; canApplySelection?: boolean;
  selectionActionLabel?: string; undoLabel?: string; redoLabel?: string; creditsAvailable?: boolean;
  recentSessions?: readonly { id: string; name: string }[];
}

/** Availability is document/view state, never read back from a hidden button. */
export function commandCatalog(state: ShellView): Record<ShellCommand, CommandItem> {
  const idle = !state.busy, visible = !!state.generationVisible, editing = !!state.document && state.workspace !== 'generate';
  const cameraAvailable = !!state.document && !state.refining && !state.creatingBlank && (state.workspace !== 'generate' || visible);
  const selected = state.document?.layer_stack?.find(layer => layer.id === state.selectedLayerId && !layer.discarded);
  const layers = state.document?.layer_stack ?? [], active = idle && editing && !state.showOriginal;
  const editable = active && !!selected?.visible && !selected.locked;
  const item = (id: ShellCommand, label: string, enabled: boolean, shortcut?: string): CommandItem => ({ id, label, enabled, shortcut });
  return {
    newWorkspace: item('newWorkspace', 'New workspace', idle, 'Ctrl+N'),
    openFiles: item('openFiles', 'Open images…', idle, 'Ctrl+O'), openFolder: item('openFolder', 'Open folder…', idle, 'Ctrl+Shift+O'),
    openProject: item('openProject', 'Open project…', idle, 'Ctrl+Alt+O'),
    saveProject: item('saveProject', 'Save project', idle && visible, 'Ctrl+Alt+S'), saveProjectAs: item('saveProjectAs', 'Save project as…', idle && visible),
    closeImage: item('closeImage', 'Close image…', idle && (state.workspace === 'generate' ? visible : !!state.document), 'Ctrl+W'),
    overwrite: { ...item('overwrite', 'Overwrite original…', idle && visible, 'Ctrl+S'), visible: !!state.document?.can_return && !state.refining && !state.creatingBlank },
    saveUnique: { ...item('saveUnique', 'Save a copy', idle && visible, 'Ctrl+Shift+S'), visible: !!state.document?.can_return && !state.refining && !state.creatingBlank },
    exportImage: item('exportImage', 'Export a copy…', idle && visible), credits: { ...item('credits', 'Image credits…', idle), visible: !!state.creditsAvailable },
    showAssets: item('showAssets', 'Assets…', idle), showGenerated: item('showGenerated', 'Generated library…', idle), showBatch: {...item('showBatch', 'Remove backgrounds…', idle && state.workspace === 'cutout'), visible: state.workspace === 'cutout'},
    undo: item('undo', state.undoLabel || 'Undo', idle && !!state.canUndo, 'Ctrl+Z'), redo: item('redo', state.redoLabel || 'Redo', idle && !!state.canRedo, 'Ctrl+Shift+Z'),
    applySelection: item('applySelection', state.selectionActionLabel || 'Apply selection', !!state.canApplySelection),
    clearSelection: item('clearSelection', 'Clear selection', active && !!state.selectionActive), finishPath: { ...item('finishPath', 'Close path', active && (state.penPointCount ?? 0) >= 3, 'Enter'), visible: state.tool === 'pen' },
    showSettings: item('showSettings', 'Settings…', idle), createRetouch: item('createRetouch', 'New retouch layer', active),
    addMask: item('addMask', 'Add editable mask', active && !!selected?.visible && selected.kind !== 'cutout'), renameLayer: item('renameLayer', 'Rename layer', active && !!selected, 'F2'),
    toggleLayerLock: item('toggleLayerLock', selected?.locked ? 'Unlock layer' : 'Lock layer', active && !!selected),
    layerUp: item('layerUp', 'Move layer up', editable && layers.indexOf(selected!) < layers.length - 1), layerDown: item('layerDown', 'Move layer down', editable && layers.indexOf(selected!) > 0),
    deleteLayer: item('deleteLayer', 'Delete layer', active && !!selected && !selected.locked && selected.kind !== 'original', 'Delete'),
    restoreLayer: item('restoreLayer', 'Restore discarded layer', active && layers.some(layer => layer.discarded)), mergeLayers: item('mergeLayers', 'Merge visible to new layer', active && layers.filter(layer => layer.visible && !layer.discarded).length > 1, 'Ctrl+Alt+Shift+E'),
    fit: item('fit', 'Fit image', cameraAvailable, 'F'), actualSize: item('actualSize', 'Actual size (100%)', cameraAvailable, '1'),
    zoomIn: item('zoomIn', 'Zoom in', cameraAvailable, '+'), zoomOut: item('zoomOut', 'Zoom out', cameraAvailable, '−'),
    toggleOriginal: { ...item('toggleOriginal', 'Show original', idle && cameraAvailable, '\\'), checked: state.showOriginal },
    refreshView: item('refreshView', 'Refresh preview', idle && cameraAvailable),
    showHardware: item('showHardware', 'Hardware guide…', true), showShortcuts: item('showShortcuts', 'Keyboard shortcuts…', true),
  };
}
