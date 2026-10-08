import { useRef, useSyncExternalStore } from 'react';
import {
  Badge,
  Button,
  Portal,
  ProgressBar,
  Toolbar,
  ToolbarButton,
  ToolbarDivider,
  ToolbarToggleButton,
} from '@fluentui/react-components';
import type { DocumentController } from './editor/documentController.ts';
import { BatchDialog, type BatchController } from './features/batch/index.ts';
import { SettingsDialogs, type SettingsController } from './features/settings/index.ts';
import { ModelDialogs, type ModelsController } from './features/models/index.ts';
import { AssetsDock, type AssetsController } from './features/assets/index.ts';
import { GenerationPanel, type GenerationController } from './features/generation/index.ts';
import { MenuBar } from './features/shell/MenuBar.tsx';
import { ToolOptions, ToolRail, WorkspaceTabs } from './features/shell/ToolControls.tsx';
import { DocumentBar, DocumentTabs, Filmstrip, StatusBar } from './features/shell/DocumentChrome.tsx';
import { Layers } from './features/shell/Layers.tsx';
import { Icon } from './features/shell/Icon.tsx';
import { Hint } from './features/shell/Hint.tsx';
import { CutoutProperties, BackgroundGenerator } from './features/shell/CutoutProperties.tsx';
import { EditorDialogs } from './features/shell/EditorDialogs.tsx';
import { useCompactToolbar } from './features/shell/useCompactToolbar.ts';
import type { EditorDialogsController } from './features/shell/editorDialogs.ts';
import type { ShellController } from './features/shell/shellController.ts';
import { commandCatalog, type ShellCommand } from './features/shell/commandCatalog.ts';

export interface FullAppProps {
  controller: DocumentController;
  shell: ShellController;
  dialogs: EditorDialogsController;
  assets: AssetsController;
  generation: GenerationController;
  batch: BatchController;
  settings: SettingsController;
  models: ModelsController;
  mounts: Record<
    'tools' | 'assets' | 'document' | 'progress' | 'empty' | 'filmstrip' | 'inspector' | 'generation' | 'status',
    HTMLElement
  >;
  execute(command: ShellCommand): unknown;
}
export function FullApp({
  controller,
  shell,
  dialogs,
  assets,
  generation,
  batch,
  settings,
  models,
  mounts,
  execute,
}: FullAppProps) {
  const state = useSyncExternalStore(controller.subscribe, controller.getSnapshot),
    ui = useSyncExternalStore(shell.subscribe, shell.getSnapshot),
    updateAvailable = !!useSyncExternalStore(settings.subscribe, settings.getSnapshot).update?.available;
  const activeDocumentId = state.creatingBlank ? null : (state.document?.id ?? null);
  const commands = commandCatalog(state);
  const batchLabel = commands.showBatch.label.replace(/…$/, '');
  const workspaceBar = useRef<HTMLDivElement>(null),
    commandBar = useRef<HTMLDivElement>(null);
  const compact = useCompactToolbar(workspaceBar, commandBar);
  // Labelled commands keep the same accessible name when shown icon-only.
  const label = (text: string) => ({ 'aria-label': text });
  const variant = state.health.qwen.variants.find(value => value.id === state.tools.qwenVariant),
    klein = state.health.models.find(value => value.id === 'klein');
  const needsQwen =
    (state.workspace === 'cutout' && !state.tools.maskReady) ||
    (state.workspace === 'retouch' && state.tools.operation === 'ai' && state.tools.aiProvider === 'qwen');
  const needsKlein =
    state.workspace === 'retouch' && state.tools.operation === 'ai' && state.tools.aiProvider === 'klein';
  const unavailableReason =
    needsQwen && (!state.health.qwen.ready || !variant?.available)
      ? variant?.reason || state.health.qwen.reason || 'Qwen Image 2.1 is not ready.'
      : needsKlein && (!state.health.ready || !klein?.available)
        ? klein?.reason || 'FLUX Klein is not ready.'
        : undefined;
  const entries = (state.collection?.entries ?? []).map((entry, index) => ({
    id: entry.id,
    name: entry.name,
    thumbnail: entry.thumbnail ?? null,
    dirty: !!entry.dirty,
    projectDirty: !!entry.project_dirty,
    selected: index === state.collectionIndex,
  }));
  return (
    <>
      <header className="li-app-header">
        <span className="li-brand">Local Image</span>
        <MenuBar
          state={state}
          execute={execute}
          openRecent={controller.commands.openRecent}
          opened={ui.menu}
          focusRequest={ui.menuFocus}
          setMenu={shell.setMenu}
        />
      </header>
      <DocumentTabs
        documents={state.openDocuments}
        activeId={activeDocumentId}
        busy={state.busy}
        onSelect={controller.commands.activateOpenDocument}
        onClose={controller.commands.closeOpenDocument}
        onNew={() => execute('newWorkspace')}
      />
      <div className="li-workspace-bar" ref={workspaceBar}>
        <WorkspaceTabs state={state.tools} actions={controller.toolActions} />
        <Toolbar
          ref={commandBar}
          className="li-commandbar"
          data-compact={compact}
          aria-label="Editor commands"
          size="medium"
          checkedValues={{ panels: state.inspectorHidden ? [] : ['inspector'] }}
        >
          <Hint content={state.undoLabel + ' (Ctrl+Z)'} relationship="description">
            <ToolbarButton
              aria-label="Undo"
              icon={<Icon name="undo" />}
              disabled={!commands.undo.enabled}
              onClick={() => execute('undo')}
            />
          </Hint>
          <Hint content={state.redoLabel + ' (Ctrl+Shift+Z)'} relationship="description">
            <ToolbarButton
              aria-label="Redo"
              icon={<Icon name="redo" />}
              disabled={!commands.redo.enabled}
              onClick={() => execute('redo')}
            />
          </Hint>
          <ToolbarDivider />
          <Hint content="Assets" enabled={compact}>
            <ToolbarButton
              {...label('Assets')}
              icon={<Icon name="assets" />}
              disabled={!commands.showAssets.enabled}
              onClick={() => execute('showAssets')}
            >
              {!compact && 'Assets'}
            </ToolbarButton>
          </Hint>
          {commands.showBatch.visible !== false && (
            <Hint content={batchLabel} enabled={compact}>
              <ToolbarButton
                id="batch-open"
                {...label(batchLabel)}
                icon={<Icon name="batch" />}
                disabled={!commands.showBatch.enabled}
                onClick={() => execute('showBatch')}
              >
                {!compact && batchLabel}
              </ToolbarButton>
            </Hint>
          )}
          <Hint content="Inspector" enabled={compact}>
            <ToolbarToggleButton
              name="panels"
              value="inspector"
              {...label('Inspector')}
              icon={<Icon name="inspector" />}
              aria-controls="inspector-root"
              disabled={state.busy}
              onClick={controller.commands.toggleInspector}
            >
              {!compact && 'Inspector'}
            </ToolbarToggleButton>
          </Hint>
          <ToolbarDivider />
          <Hint content="Export" enabled={compact}>
            <ToolbarButton
              {...label('Export')}
              icon={<Icon name="export" />}
              disabled={!commands.exportImage.enabled}
              onClick={() => execute('exportImage')}
            >
              {!compact && 'Export'}
            </ToolbarButton>
          </Hint>
          <Hint content={updateAvailable ? 'Settings · an update is available' : 'Settings'}>
            <ToolbarButton
              aria-label={updateAvailable ? 'Settings, update available' : 'Settings'}
              icon={
                <span className="li-badge-anchor">
                  <Icon name="settings" />
                  {updateAvailable && <Badge size="extra-small" color="brand" className="li-update-badge" />}
                </span>
              }
              disabled={!commands.showSettings.enabled}
              onClick={() => execute('showSettings')}
            />
          </Hint>
        </Toolbar>
      </div>
      <ToolOptions
        state={state.tools}
        actions={controller.toolActions}
        menuOpen={ui.popup === 'background'}
        setMenuOpen={open => shell.setPopup('background', open)}
        unavailableReason={unavailableReason}
        openSetup={() => void settings.open('settings', 'ai')}
      />
      <Portal mountNode={mounts.tools}>
        <ToolRail state={state.tools} actions={controller.toolActions} />
      </Portal>
      <Portal mountNode={mounts.assets}>
        <AssetsDock controller={assets} />
      </Portal>
      <Portal mountNode={mounts.document}>
        {!state.creatingBlank && <DocumentBar state={state.chrome} actions={controller.chromeActions} />}
      </Portal>
      <Portal mountNode={mounts.progress}>
        {state.operationLabel && (
          <div className="li-operation-progress" role="status">
            <span>{state.operationLabel}…</span>
            <ProgressBar aria-label={state.operationLabel} />
          </div>
        )}
      </Portal>
      <Portal mountNode={mounts.empty}>
        {(!state.document || state.creatingBlank) && (
          <div className="li-empty-canvas">
            {state.workspace === 'generate' ? (
              <span>Create a new image</span>
            ) : (
              <>
                <Icon name="image" />
                <Button disabled={state.busy} onClick={() => execute('openFiles')}>
                  Open image…
                </Button>
                <span>Drop images or an editable project here</span>
              </>
            )}
          </div>
        )}
      </Portal>
      <Portal mountNode={mounts.filmstrip}>
        <Filmstrip
          name={state.collection?.name ?? ''}
          entries={entries}
          busy={state.busy}
          collapsed={state.filmstripCollapsed}
          thumbnailSize={state.thumbnailSize}
          onThumbnailSize={controller.setThumbnailSize}
          onCollapsed={controller.setFilmstripCollapsed}
          onSelect={id => controller.openCollectionEntry(state.collection!.entries.findIndex(entry => entry.id === id))}
          onPrevious={() => controller.navigateCollection(-1)}
          onNext={() => controller.navigateCollection(1)}
        />
      </Portal>
      <Portal mountNode={mounts.inspector}>
        <Layers
          controller={controller}
          renameRequest={ui.rename}
          menuOpen={ui.popup === 'layers'}
          setMenuOpen={open => shell.setPopup('layers', open)}
        />
        <CutoutProperties controller={controller} shell={shell} />
        {state.workspace === 'generate' && <GenerationPanel controller={generation} />}
      </Portal>
      <Portal mountNode={mounts.status}>
        <StatusBar state={state.chrome} actions={controller.chromeActions} />
      </Portal>
      <BatchDialog controller={batch} />
      <SettingsDialogs controller={settings} />
      <ModelDialogs controller={models} />
      <EditorDialogs controller={dialogs} />
      <BackgroundGenerator controller={controller} shell={shell} />
    </>
  );
}
