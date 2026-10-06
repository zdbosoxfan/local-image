import type { DocumentController } from '../../editor/documentController.ts';
import { commandCatalog, type ShellCommand } from './commandCatalog.ts';

/** Menus, toolbars and application shortcuts share this exact availability
 * check and command route. Direct feature controls use the same controller. */
export function createCommandExecutor(controller: DocumentController, closeMenus: () => void) {
  return (command: ShellCommand): unknown => {
    const state = controller.getSnapshot(),
      item = commandCatalog(state)[command];
    if (!item.enabled || item.visible === false) return;
    closeMenus();
    const commands = controller.commands,
      selected = state.document?.layer_stack?.find(layer => layer.id === state.selectedLayerId);
    switch (command) {
      case 'toggleLayerLock':
        return selected && commands.patchLayer(selected.id, { locked: !selected.locked });
      case 'deleteLayer':
        return selected && commands.patchLayer(selected.id, { discarded: true });
      case 'layerUp':
        return commands.reorderLayer(1);
      case 'layerDown':
        return commands.reorderLayer(-1);
      case 'fit':
        return commands.fit();
      case 'actualSize':
        return commands.actualSize();
      case 'zoomIn':
        return commands.zoomIn();
      case 'zoomOut':
        return commands.zoomOut();
      case 'refreshView':
        return controller.refreshPreview();
      default:
        return commands[command]();
    }
  };
}
