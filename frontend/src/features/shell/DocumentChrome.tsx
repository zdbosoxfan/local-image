import { useEffect, useRef, type CSSProperties } from 'react';
import {
  Button,
  Menu,
  MenuItem,
  MenuList,
  MenuPopover,
  MenuTrigger,
  Slider,
  Tab,
  TabList,
  Toolbar,
  ToolbarButton,
  Tooltip,
} from '@fluentui/react-components';
import { Icon } from './Icon.tsx';
import { ChoiceMenu } from './ChoiceMenu.tsx';
import './shell.css';

export interface DocumentChromeState {
  id: string | null;
  name: string;
  width: number;
  height: number;
  bitDepth: number;
  busy: boolean;
  dirty: boolean;
  projectDirty: boolean;
  selectionPending: boolean;
  canReturn: boolean;
  showOriginal: boolean;
  zoom: number;
  fit: boolean;
  status: string;
  error: boolean;
  toolHint: string;
}
export interface DocumentChromeActions {
  close(): unknown;
  toggleOriginal(): void;
  overwrite(): unknown;
  saveUnique(): unknown;
  export(): unknown;
  zoom(value: number): void;
  zoomBy(factor: number): void;
  fit(): void;
}
export interface OpenDocumentTab {
  id: string;
  name: string;
  dirty?: boolean;
  project_dirty?: boolean;
}
export function DocumentTabs({
  documents,
  activeId,
  busy,
  onSelect,
  onClose,
  onNew,
}: {
  documents: readonly OpenDocumentTab[];
  activeId: string | null;
  busy: boolean;
  onSelect(id: string): unknown;
  onClose(id: string): unknown;
  onNew(): unknown;
}) {
  const list = useRef<HTMLDivElement>(null),
    newButton = useRef<HTMLButtonElement>(null);
  useEffect(() => {
    list.current
      ?.querySelector<HTMLElement>('[aria-selected="true"]')
      ?.scrollIntoView({ block: 'nearest', inline: 'nearest' });
  }, [activeId]);
  async function close(id: string) {
    const hadFocus = !!list.current?.contains(document.activeElement),
      closed = await onClose(id);
    if (closed && hadFocus)
      requestAnimationFrame(() => {
        const tab =
          list.current?.querySelector<HTMLElement>('[aria-selected="true"]') ||
          list.current?.querySelector<HTMLElement>('[role="tab"]');
        if (tab) tab.focus();
        else newButton.current?.focus();
      });
  }
  return (
    <section className="li-open-documents" aria-label="Open image workspaces">
      {documents.length ? (
        <div className="li-open-documents-scroll">
          <TabList
            ref={list}
            size="small"
            aria-label="Open images"
            selectedValue={activeId}
            onTabSelect={(_, data) => {
              if (!busy) void onSelect(String(data.value));
            }}
          >
            {documents.map(item => (
              <div key={item.id} className="li-open-document" data-selected={item.id === activeId}>
                <Tooltip
                  content={`${item.name}${item.dirty || item.project_dirty ? ' · Unsaved changes' : ''}`}
                  relationship="description"
                >
                  <Tab
                    value={item.id}
                    disabled={busy}
                    aria-controls="editor-layout"
                    aria-label={`${item.name}${item.dirty || item.project_dirty ? ', unsaved changes' : ''}`}
                    onKeyDown={event => {
                      if (event.key === 'Delete' && !busy) {
                        event.preventDefault();
                        event.stopPropagation();
                        void close(item.id);
                      }
                    }}
                  >
                    <span className="li-open-document-name">{item.name}</span>
                    {(item.dirty || item.project_dirty) && (
                      <span className="li-open-document-dirty" aria-hidden="true">
                        •
                      </span>
                    )}
                  </Tab>
                </Tooltip>
                <Tooltip content="Close image" relationship="description">
                  <Button
                    size="small"
                    appearance="subtle"
                    className="li-open-document-close"
                    aria-label={`Close ${item.name}`}
                    disabled={busy}
                    tabIndex={-1}
                    data-tabster='{"focusable":{"excludeFromMover":true}}'
                    icon={<Icon name="close" />}
                    onClick={() => void close(item.id)}
                  />
                </Tooltip>
              </div>
            ))}
          </TabList>
        </div>
      ) : (
        <span className="li-empty-workspace">Empty workspace</span>
      )}
      {documents.length > 1 && (
        <Menu>
          <MenuTrigger disableButtonEnhancement>
            <Tooltip content="Open images" relationship="description">
              <Button
                size="small"
                appearance="subtle"
                className="li-open-document-close"
                aria-label="Open images menu"
                disabled={busy}
                icon={<Icon name="more" />}
              />
            </Tooltip>
          </MenuTrigger>
          <MenuPopover className="li-open-documents-menu" data-react-owned="true">
            <MenuList aria-label="Open images">
              {documents.map(item => (
                <MenuItem
                  key={item.id}
                  disabled={busy}
                  aria-current={item.id === activeId ? 'true' : undefined}
                  onClick={() => void onSelect(item.id)}
                >
                  {item.name}
                  {item.dirty || item.project_dirty ? ' · Unsaved' : ''}
                </MenuItem>
              ))}
            </MenuList>
          </MenuPopover>
        </Menu>
      )}
      <Tooltip content="New workspace (Ctrl+N)" relationship="description">
        <Button
          ref={newButton}
          size="small"
          appearance="subtle"
          className="li-new-workspace"
          aria-label="New workspace"
          disabled={busy}
          icon={<Icon name="add" />}
          onClick={() => void onNew()}
        >
          <span>New workspace</span>
        </Button>
      </Tooltip>
    </section>
  );
}
export function DocumentBar({ state, actions }: { state: DocumentChromeState; actions: DocumentChromeActions }) {
  const detail = [
    state.dirty ? 'Image modified' : 'Image unchanged',
    state.projectDirty ? 'Editable project needs saving' : 'Project up to date',
    state.selectionPending ? 'Unapplied selection' : '',
  ]
    .filter(Boolean)
    .join(' · ');
  return (
    <header className="li-document-bar" aria-label="Active document">
      <div className="li-document-identity">
        <strong title={state.name}>{state.id ? state.name : 'No image open'}</strong>
        {state.id && (
          <>
            <span className="li-document-size">
              {state.width} × {state.height} · {state.bitDepth}-bit source
            </span>
            <Tooltip content="Close image" relationship="description">
              <Button
                size="small"
                appearance="subtle"
                aria-label="Close image"
                disabled={state.busy}
                icon={<Icon name="close" />}
                onClick={() => actions.close()}
              />
            </Tooltip>
          </>
        )}
      </div>
      {state.id && (
        <div className="li-document-actions">
          <span className="li-document-state" title={detail} aria-label={detail}>
            {state.dirty
              ? 'Modified'
              : state.selectionPending
                ? 'Selection pending'
                : state.projectDirty
                  ? 'Project unsaved'
                  : ''}
          </span>
          <Button
            size="small"
            appearance="subtle"
            aria-pressed={state.showOriginal}
            disabled={state.busy}
            onClick={actions.toggleOriginal}
          >
            {state.showOriginal ? 'Back to edits' : 'Original'}
          </Button>
          {state.canReturn ? (
            <>
              <Button size="small" appearance="subtle" disabled={state.busy} onClick={() => actions.overwrite()}>
                Save
              </Button>
              <Button size="small" appearance="subtle" disabled={state.busy} onClick={() => actions.saveUnique()}>
                Save a copy
              </Button>
            </>
          ) : (
            <Button size="small" appearance="subtle" disabled={state.busy} onClick={() => actions.export()}>
              Export
            </Button>
          )}
        </div>
      )}
    </header>
  );
}
export function StatusBar({ state, actions }: { state: DocumentChromeState; actions: DocumentChromeActions }) {
  const label = (state.zoom * 100 < 10 ? (state.zoom * 100).toFixed(1) : (state.zoom * 100).toFixed(0)) + '%',
    presets = [0.1, 0.25, 0.5, 1, 2, 4];
  const exact = presets.find(value => Math.abs(value - state.zoom) < 0.00001),
    value = exact === undefined ? 'custom' : String(exact);
  return (
    <footer className="li-status-bar">
      <span className="li-tool-hint" title={state.toolHint}>
        {state.toolHint}
      </span>
      <span className={state.error ? 'li-status-error' : 'li-status-message'} role="status" title={state.status}>
        {state.status}
      </span>
      <Toolbar size="small" className="li-zoom-controls" aria-label="Canvas view">
        <ToolbarButton
          aria-label="Zoom out"
          disabled={!state.id}
          onClick={() => actions.zoomBy(0.8)}
          icon={<Icon name="subtract" />}
        />
        <ChoiceMenu
          label="Image zoom"
          disabled={!state.id}
          value={value}
          choices={[
            ...(exact === undefined ? [{ value: 'custom', label }] : []),
            ...presets.map(zoom => ({ value: String(zoom), label: `${zoom * 100}%` })),
          ]}
          onSelect={next => {
            if (next !== 'custom') actions.zoom(Number(next));
          }}
        />
        <ToolbarButton
          aria-label="Zoom in"
          disabled={!state.id}
          onClick={() => actions.zoomBy(1.25)}
          icon={<Icon name="add" />}
        />
        <ToolbarButton disabled={!state.id} aria-pressed={state.fit} onClick={actions.fit}>
          Fit
        </ToolbarButton>
      </Toolbar>
    </footer>
  );
}
export interface FilmstripEntry {
  id: string;
  name: string;
  thumbnail: string | null;
  dirty: boolean;
  projectDirty: boolean;
  selected: boolean;
}
export function Filmstrip({
  name,
  entries,
  busy,
  collapsed,
  thumbnailSize = 72,
  onThumbnailSize,
  onCollapsed,
  onSelect,
  onPrevious,
  onNext,
}: {
  name: string;
  entries: readonly FilmstripEntry[];
  busy: boolean;
  collapsed: boolean;
  thumbnailSize?: number;
  onThumbnailSize?(value: number): void;
  onCollapsed(value: boolean): void;
  onSelect(id: string): unknown;
  onPrevious(): unknown;
  onNext(): unknown;
}) {
  if (entries.length < 2) return null;
  const index = entries.findIndex(entry => entry.selected);
  return (
    <section
      className="li-filmstrip"
      aria-label="Folder images"
      style={{ '--li-thumbnail-size': `${thumbnailSize}px` } as CSSProperties}
    >
      <header>
        <Button size="small" appearance="subtle" aria-expanded={!collapsed} onClick={() => onCollapsed(!collapsed)}>
          {collapsed ? 'Show images' : 'Hide images'}
        </Button>
        <span title={name}>{name}</span>
        {onThumbnailSize && (
          <Slider
            size="small"
            aria-label="Thumbnail size"
            min={48}
            max={160}
            value={thumbnailSize}
            onChange={(_, data) => onThumbnailSize(data.value)}
          />
        )}
        <span>
          {index + 1} / {entries.length}
        </span>
        <Button
          size="small"
          appearance="subtle"
          aria-label="Previous image"
          disabled={busy || index <= 0}
          onClick={() => onPrevious()}
        >
          ←
        </Button>
        <Button
          size="small"
          appearance="subtle"
          aria-label="Next image"
          disabled={busy || index >= entries.length - 1}
          onClick={() => onNext()}
        >
          →
        </Button>
      </header>
      {!collapsed && (
        <div className="li-filmstrip-items">
          {entries.map((entry, i) => (
            <Button
              appearance="subtle"
              type="button"
              key={entry.id}
              disabled={busy}
              className="li-filmstrip-item"
              aria-label={entry.name}
              aria-current={entry.selected ? 'true' : undefined}
              tabIndex={entry.selected ? 0 : -1}
              onClick={() => onSelect(entry.id)}
              onKeyDown={event => {
                if (!['ArrowLeft', 'ArrowRight', 'Home', 'End'].includes(event.key)) return;
                event.preventDefault();
                event.stopPropagation();
                const next =
                  event.key === 'Home'
                    ? 0
                    : event.key === 'End'
                      ? entries.length - 1
                      : Math.max(0, Math.min(entries.length - 1, i + (event.key === 'ArrowRight' ? 1 : -1)));
                if (!busy) {
                  void onSelect(entries[next].id);
                  (event.currentTarget.parentElement?.children[next] as HTMLElement)?.focus();
                }
              }}
            >
              {entry.thumbnail && (
                <img
                  src={entry.thumbnail}
                  alt=""
                  loading="lazy"
                  decoding="async"
                  onError={event => {
                    event.currentTarget.style.visibility = 'hidden';
                  }}
                  onLoad={event => {
                    event.currentTarget.style.visibility = '';
                  }}
                />
              )}
              <span title={entry.name}>{entry.name}</span>
              {(entry.dirty || entry.projectDirty) && <small>{entry.dirty ? 'Modified' : 'Project unsaved'}</small>}
            </Button>
          ))}
        </div>
      )}
    </section>
  );
}
