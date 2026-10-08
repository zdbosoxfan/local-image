import { useEffect } from 'react';
import {
  Button,
  Menu,
  MenuDivider,
  MenuItem,
  MenuItemCheckbox,
  MenuList,
  MenuPopover,
  MenuTrigger,
} from '@fluentui/react-components';
import { commandCatalog, type ShellCommand, type ShellView } from './commandCatalog.ts';

export interface MenuBarProps {
  state: ShellView;
  execute(command: ShellCommand): unknown;
  openRecent(id: string): unknown;
  opened: string | null;
  focusRequest: { name: string; sequence: number } | null;
  setMenu(name: string | null): void;
}
export function MenuBar({ state, execute, openRecent, opened, focusRequest, setMenu }: MenuBarProps) {
  const commands = commandCatalog(state);
  useEffect(() => {
    if (focusRequest) document.getElementById(`${focusRequest.name.toLowerCase()}-menu-trigger`)?.focus();
  }, [focusRequest?.sequence]);
  const command = (id: ShellCommand) => {
    const item = commands[id];
    if (item.visible === false) return null;
    return (
      <MenuItem key={id} disabled={!item.enabled} secondaryContent={item.shortcut} onClick={() => execute(id)}>
        {item.label}
      </MenuItem>
    );
  };
  const groups: Array<{ name: string; entries: React.ReactNode }> = [
    {
      name: 'File',
      entries: (
        <>
          {(['newWorkspace', 'openFiles', 'openProject', 'openFolder'] as const).map(command)}
          <MenuDivider />
          {(['saveProject', 'saveProjectAs'] as const).map(command)}
          <MenuDivider />
          {(['overwrite', 'saveUnique', 'exportImage'] as const).map(command)}
          <MenuDivider />
          {(['credits', 'closeImage'] as const).map(command)}
          <MenuDivider />
          <Menu>
            <MenuTrigger disableButtonEnhancement>
              <MenuItem>Recent sessions</MenuItem>
            </MenuTrigger>
            <MenuPopover data-react-owned="true">
              <MenuList>
                {state.recentSessions?.length ? (
                  state.recentSessions.map(session => (
                    <MenuItem key={session.id} disabled={state.busy} onClick={() => openRecent(session.id)}>
                      {session.name}
                    </MenuItem>
                  ))
                ) : (
                  <MenuItem disabled>No recent sessions</MenuItem>
                )}
              </MenuList>
            </MenuPopover>
          </Menu>
        </>
      ),
    },
    {
      name: 'Edit',
      entries: (
        <>
          {(['undo', 'redo'] as const).map(command)}
          <MenuDivider />
          {command('showSettings')}
        </>
      ),
    },
    {
      name: 'Layer',
      entries: (
        <>
          {(
            [
              'createRetouch',
              'addMask',
              'renameLayer',
              'toggleLayerLock',
              'layerUp',
              'layerDown',
              'deleteLayer',
            ] as const
          ).map(command)}
          <MenuDivider />
          {(['mergeLayers', 'restoreLayer'] as const).map(command)}
        </>
      ),
    },
    { name: 'Select', entries: <>{(['applySelection', 'clearSelection', 'finishPath'] as const).map(command)}</> },
    {
      name: 'View',
      entries: (
        <>
          {(['fit', 'actualSize', 'zoomIn', 'zoomOut'] as const).map(command)}
          <MenuDivider />
          <MenuItemCheckbox
            name="original"
            value="original"
            disabled={!commands.toggleOriginal.enabled}
            onClick={() => execute('toggleOriginal')}
          >
            Show original
          </MenuItemCheckbox>
          {command('refreshView')}
          <MenuDivider />
          {(['toggleInspector', 'showAssets', 'showGenerated'] as const).map(command)}
        </>
      ),
    },
    ...(commands.showBatch.visible === false ? [] : [{ name: 'Tools', entries: <>{command('showBatch')}</> }]),
    { name: 'Help', entries: <>{(['showHardware', 'showShortcuts'] as const).map(command)}</> },
  ];
  return (
    <nav className="li-menu-bar" role="menubar" aria-label="Application menu">
      {groups.map((group, index) => (
        <Menu
          key={group.name}
          open={opened === group.name}
          onOpenChange={(_, data) => setMenu(data.open ? group.name : null)}
        >
          <MenuTrigger disableButtonEnhancement>
            <Button
              id={`${group.name.toLowerCase()}-menu-trigger`}
              role="menuitem"
              size="small"
              appearance="subtle"
              tabIndex={index === 0 ? 0 : -1}
              onKeyDown={event => {
                if (event.key !== 'ArrowLeft' && event.key !== 'ArrowRight') return;
                event.preventDefault();
                const next = groups[(index + (event.key === 'ArrowRight' ? 1 : groups.length - 1)) % groups.length];
                document.getElementById(`${next.name.toLowerCase()}-menu-trigger`)?.focus();
                if (opened) setMenu(next.name);
              }}
            >
              {group.name}
            </Button>
          </MenuTrigger>
          <MenuPopover data-react-owned="true">
            <MenuList
              checkedValues={{ original: state.showOriginal ? ['original'] : [] }}
              onKeyDown={event => {
                if (event.defaultPrevented || !['ArrowLeft', 'ArrowRight'].includes(event.key)) return;
                const target = event.target as HTMLElement;
                if (target.closest('[role="menu"]') !== event.currentTarget) return;
                if (event.key === 'ArrowRight' && target.closest('[aria-haspopup="menu"]')) return;
                event.preventDefault();
                event.stopPropagation();
                const next = groups[(index + (event.key === 'ArrowRight' ? 1 : groups.length - 1)) % groups.length];
                setMenu(next.name);
              }}
            >
              {group.entries}
            </MenuList>
          </MenuPopover>
        </Menu>
      ))}
    </nav>
  );
}
