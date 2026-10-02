import type { ShellCommand } from '../features/shell/commandCatalog.ts';
import type { Tool } from '../features/shell/ToolControls.tsx';

export interface KeyboardState {
  modalOpen: boolean; menuOpen: boolean; hasDocument: boolean; hiddenEditor: boolean; canReturn: boolean;
  busy: boolean; showOriginal: boolean; workspace: string; tool: string; handActive: boolean;
}
export interface KeyInput {
  key: string; code: string; ctrlKey: boolean; metaKey: boolean; altKey: boolean; shiftKey: boolean;
  repeat?: boolean; defaultPrevented?: boolean;
}
export interface KeyTarget { textEntry: boolean; activates: boolean; folderEntry: boolean; brushSlider: boolean; layerEditing: boolean }
export type KeyAction = { kind: 'command'; command: ShellCommand }
  | { kind: 'tool'; tool: Tool } | { kind: 'navigate'; direction: -1 | 1 }
  | { kind: 'brush'; direction: -1 | 1 } | { kind: 'space'; held: boolean }
  | { kind: 'cancelPen' } | { kind: 'menu'; menu: string | null } | { kind: 'block' };
const menus: Record<string, string> = { f: 'File', e: 'Edit', l: 'Layer', s: 'Select', v: 'View', h: 'Help' };
const command = (value: ShellCommand): KeyAction => ({ kind: 'command', command: value });

/** One key routing policy shared by all visible editor regions. Component
 * keyboard behavior (menus, inputs and comparison views) gets first refusal. */
export function resolveEditorKey(event: KeyInput, state: KeyboardState, target: KeyTarget, spaceHeld = false): KeyAction | null {
  if (event.defaultPrevented) return null;
  const key = event.key.toLowerCase(), modifier = event.ctrlKey || event.metaKey;
  if (state.modalOpen) return modifier && ['n', 's', 'o', 'w', 'z', '+', '=', '-', '0'].includes(key) ? { kind: 'block' } : null;
  if (event.key === 'F10' && !event.shiftKey) return { kind: 'menu', menu: null };
  if (event.altKey && !modifier && menus[key]) return { kind: 'menu', menu: menus[key] };
  // File commands remain available while text fields are focused, matching the
  // existing app. Undo/redo and editing keys belong to the input when focused.
  if (modifier && key === 'n' && !event.altKey && !event.shiftKey) return command('newWorkspace');
  if (modifier && key === 'w' && state.hasDocument) return command('closeImage');
  if (modifier && event.altKey && ['s', 'o'].includes(key)) return command(key === 's' ? 'saveProject' : 'openProject');
  if (modifier && key === 's' && state.hasDocument) return command(state.hiddenEditor || !state.canReturn ? 'exportImage' : event.shiftKey ? 'saveUnique' : 'overwrite');
  if (modifier && key === 'o') return command(event.shiftKey ? 'openFolder' : 'openFiles');
  if (state.menuOpen && modifier) {
    if (key === 'z') return command(event.shiftKey ? 'redo' : 'undo');
    if (event.altKey && event.shiftKey && key === 'e') return command('mergeLayers');
  }
  if (!modifier && !event.altKey && [' ', 'Enter'].includes(event.key) && target.activates) return null;
  const bracket = target.brushSlider && ['[', ']'].includes(event.key);
  if (state.menuOpen || (target.textEntry && !bracket && !target.folderEntry)) return null;
  if (target.folderEntry && event.code === 'Space') return null;
  if (!state.hasDocument || state.hiddenEditor) return null;
  if (event.code === 'Space' && !modifier && !event.altKey) return { kind: 'space', held: true };
  if (modifier) {
    if (event.altKey && event.shiftKey && key === 'e') return command('mergeLayers');
    if (key === 'z') return command(event.shiftKey ? 'redo' : 'undo');
    return null;
  }
  if ((event.altKey && ['ArrowLeft', 'ArrowRight'].includes(event.key)) || (!event.altKey && ['PageUp', 'PageDown'].includes(event.key))) {
    return { kind: 'navigate', direction: ['ArrowLeft', 'PageUp'].includes(event.key) ? -1 : 1 };
  }
  if (event.altKey || spaceHeld) return null;
  if (event.key === '\\') return command('toggleOriginal');
  if (['+', '=', '-', '_', 'f', '1'].includes(key)) return command(key === 'f' ? 'fit' : key === '1' ? 'actualSize' : ['-', '_'].includes(key) ? 'zoomOut' : 'zoomIn');
  if (key === 'h') return { kind: 'tool', tool: 'hand' };
  if (state.busy || state.showOriginal || state.workspace === 'generate') return null;
  if (['[', ']'].includes(event.key) && state.tool === 'brush' && !state.handActive) return { kind: 'brush', direction: event.key === ']' ? 1 : -1 };
  if (event.key === 'Enter' && state.tool === 'pen') return command('finishPath');
  if (event.key === 'Escape') return { kind: 'cancelPen' };
  if (event.key === 'F2') return target.layerEditing ? command('renameLayer') : null;
  if (event.key === 'Delete') return target.layerEditing ? command('deleteLayer') : null;
  const tool = ({ j: 'heal', v: 'move', b: 'brush', p: 'pen', r: 'rectangle', e: 'ellipse' } as Record<string, Tool>)[key];
  return tool ? { kind: 'tool', tool } : null;
}

export interface KeyboardPorts {
  getState(): KeyboardState;
  execute(command: ShellCommand): unknown;
  selectTool(tool: Tool): void; navigate(direction: -1 | 1): unknown;
  stepBrush(direction: -1 | 1): void; setSpaceHeld(held: boolean): void; cancelPen(): void;
  openMenu(name: string): void; toggleMenuFocus(): void; closeMenus(): void;
  resetTransientInput(): void; report(error: unknown): void;
}
const owners = new WeakSet<Document>();
export function installKeyboardController(doc: Document, ports: KeyboardPorts) {
  if (owners.has(doc)) throw Error('This document already has an editor keyboard owner.');
  owners.add(doc);
  let spaceHeld = false, altPressed = false;
  function keydown(event: KeyboardEvent) {
    if (event.key === 'Alt') { altPressed = !event.ctrlKey && !event.metaKey && !event.defaultPrevented; return; }
    altPressed = false;
    const element = event.target instanceof Element ? event.target : null;
    const target = {
      textEntry: !!element?.closest('input,select,textarea,[contenteditable="true"],[role="textbox"],[role="menubar"],[role="menu"],[data-folder-entry]'),
      activates: !!element?.closest('button,a,[role="button"]'),
      folderEntry: !!element?.closest('[data-folder-entry]'),
      brushSlider: element?.getAttribute('aria-label') === 'Brush size',
      layerEditing: !!element?.closest('#viewport') || !!element?.matches('[role="option"][data-layer-id]'),
    };
    const action = resolveEditorKey(event, ports.getState(), target, spaceHeld);
    if (!action) return;
    event.preventDefault();
    try {
      let result: unknown;
      switch (action.kind) {
        case 'command': ports.closeMenus(); result = ports.execute(action.command); break;
        case 'tool': ports.selectTool(action.tool); break;
        case 'navigate': result = ports.navigate(action.direction); break;
        case 'brush': ports.stepBrush(action.direction); break;
        case 'space': if (!spaceHeld) { spaceHeld = true; ports.setSpaceHeld(true); } break;
        case 'cancelPen': ports.cancelPen(); break;
        case 'menu': if (action.menu) ports.openMenu(action.menu); else ports.toggleMenuFocus(); break;
        case 'block': break;
      }
      if (result && typeof (result as Promise<unknown>).then === 'function') void Promise.resolve(result).catch(ports.report);
    } catch (error) { ports.report(error); }
  }
  function keyup(event: KeyboardEvent) {
    if (event.code === 'Space') { if (spaceHeld) event.preventDefault(); spaceHeld = false; ports.setSpaceHeld(false); }
    if (event.key === 'Alt' && altPressed && !ports.getState().modalOpen) { event.preventDefault(); altPressed = false; ports.toggleMenuFocus(); }
  }
  function reset() { spaceHeld = false; altPressed = false; ports.setSpaceHeld(false); ports.resetTransientInput(); ports.closeMenus(); }
  const view = doc.defaultView;
  doc.addEventListener('keydown', keydown); doc.addEventListener('keyup', keyup);
  view?.addEventListener('blur', reset);
  const visibility = () => { if (doc.hidden) reset(); };
  doc.addEventListener('visibilitychange', visibility);
  return () => {
    doc.removeEventListener('keydown', keydown); doc.removeEventListener('keyup', keyup);
    view?.removeEventListener('blur', reset); doc.removeEventListener('visibilitychange', visibility);
    reset(); owners.delete(doc);
  };
}
