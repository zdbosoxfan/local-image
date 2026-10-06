import type { CSSProperties } from 'react';
import { Button, Select, Slider, Toolbar, ToolbarButton } from '@fluentui/react-components';
import { Icon } from './Icon.tsx';
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
            <Button
              size="small"
              appearance="subtle"
              aria-label="Close image"
              disabled={state.busy}
              onClick={() => actions.close()}
              icon={<Icon name="close" />}
            />
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
        <ToolbarButton
          aria-label="Zoom out"
          disabled={!state.id}
          onClick={() => actions.zoomBy(0.8)}
          icon={<Icon name="zoom-out" />}
        />
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
        <ToolbarButton
          aria-label="Zoom in"
          disabled={!state.id}
          onClick={() => actions.zoomBy(1.25)}
          icon={<Icon name="zoom-in" />}
        />
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
          icon={<Icon name="previous" />}
        />
        <Button
          size="small"
          appearance="subtle"
          aria-label="Next image"
          disabled={busy || index >= entries.length - 1}
          onClick={() => onNext()}
          icon={<Icon name="next" />}
        />
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
