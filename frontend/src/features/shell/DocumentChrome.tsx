import { useEffect, useRef, type CSSProperties } from 'react';
import {
  Badge,
  Button,
  Menu,
  MenuItem,
  MenuList,
  MenuPopover,
  MenuTrigger,
  Select,
  Slider,
  Tab,
  TabList,
  ToggleButton,
  Toolbar,
  ToolbarButton,
} from '@fluentui/react-components';
import { Icon } from './Icon.tsx';
import { Hint } from './Hint.tsx';
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
                <Hint
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
                      <Badge size="tiny" color="brand" className="li-open-document-dirty" aria-hidden="true" />
                    )}
                  </Tab>
                </Hint>
                <Hint content="Close image" relationship="description">
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
                </Hint>
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
            <Hint content="Open images" relationship="description">
              <Button
                size="small"
                appearance="subtle"
                className="li-open-document-close"
                aria-label="Open images menu"
                disabled={busy}
                icon={<Icon name="more" />}
              />
            </Hint>
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
  const documentState = state.dirty
    ? 'Modified'
    : state.selectionPending
      ? 'Selection pending'
      : state.projectDirty
        ? 'Project unsaved'
        : '';
  return (
    <header className="li-document-bar" aria-label="Active document">
      <div className="li-document-identity">
        <strong title={state.name}>{state.id ? state.name : 'No image open'}</strong>
        {state.id && (
          <>
            <span className="li-document-size">
              {state.width} × {state.height} · {state.bitDepth}-bit source
            </span>
            <Hint content="Close image">
              <Button
                size="small"
                appearance="subtle"
                aria-label="Close image"
                disabled={state.busy}
                onClick={() => actions.close()}
                icon={<Icon name="close" />}
              />
            </Hint>
          </>
        )}
      </div>
      {state.id && (
        <div className="li-document-actions">
          {documentState && (
            <Badge
              className="li-document-state"
              appearance="outline"
              color="warning"
              title={detail}
              aria-label={detail}
            >
              {documentState}
            </Badge>
          )}
          <ToggleButton
            size="small"
            appearance="subtle"
            checked={state.showOriginal}
            disabled={state.busy}
            icon={<Icon name="compare" />}
            onClick={actions.toggleOriginal}
          >
            {state.showOriginal ? 'Back to edits' : 'Original'}
          </ToggleButton>
          {/* Export lives once, in the editor command bar. Save and Save a copy
              are offered here only when the image can return to its source. */}
          {state.canReturn && (
            <>
              <Button size="small" appearance="subtle" disabled={state.busy} onClick={() => actions.overwrite()}>
                Save
              </Button>
              <Button size="small" appearance="subtle" disabled={state.busy} onClick={() => actions.saveUnique()}>
                Save a copy
              </Button>
            </>
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
        <Hint content="Zoom out">
          <ToolbarButton
            aria-label="Zoom out"
            disabled={!state.id}
            onClick={() => actions.zoomBy(0.8)}
            icon={<Icon name="zoom-out" />}
          />
        </Hint>
        <Select
          size="small"
          aria-label="Image zoom"
          title={label + ' of full photo size'}
          disabled={!state.id}
          value={value}
          onChange={(_, data) => {
            if (data.value !== 'custom') actions.zoom(Number(data.value));
          }}
        >
          {exact === undefined && <option value="custom">{label}</option>}
          {presets.map(zoom => (
            <option key={zoom} value={zoom}>
              {zoom * 100}%
            </option>
          ))}
        </Select>
        <Hint content="Zoom in">
          <ToolbarButton
            aria-label="Zoom in"
            disabled={!state.id}
            onClick={() => actions.zoomBy(1.25)}
            icon={<Icon name="zoom-in" />}
          />
        </Hint>
        <ToolbarButton disabled={!state.id} aria-pressed={state.fit} icon={<Icon name="fit" />} onClick={actions.fit}>
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
        <Button
          size="small"
          appearance="subtle"
          aria-expanded={!collapsed}
          icon={<Icon name={collapsed ? 'collapse' : 'expand'} />}
          onClick={() => onCollapsed(!collapsed)}
        >
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
        <Hint content="Previous image">
          <Button
            size="small"
            appearance="subtle"
            aria-label="Previous image"
            disabled={busy || index <= 0}
            onClick={() => onPrevious()}
            icon={<Icon name="previous" />}
          />
        </Hint>
        <Hint content="Next image">
          <Button
            size="small"
            appearance="subtle"
            aria-label="Next image"
            disabled={busy || index >= entries.length - 1}
            onClick={() => onNext()}
            icon={<Icon name="next" />}
          />
        </Hint>
      </header>
      {!collapsed && (
        <div className="li-filmstrip-items">
          {entries.map((entry, i) => (
            <Button
              appearance="subtle"
              type="button"
              key={entry.id}
              disabled={busy}
              className="li-filmstrip-item li-selectable"
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
