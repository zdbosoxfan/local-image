import { useEffect, useRef, useState, useSyncExternalStore } from 'react';
import type { KeyboardEvent, ReactNode } from 'react';
import { Button, Checkbox, Field, Input, Select, Tab, TabList } from '@fluentui/react-components';
import { safeCreditUrl } from './api.ts';
import type { AssetsController } from './controller.ts';
import type { AssetTab, StockProviderId } from './types.ts';
import { Icon } from '../shell/Icon.tsx';
import './assets.css';

function CreditLink({ href, children }: { href?: string; children: ReactNode }) {
  const address = safeCreditUrl(href);
  return address ? (
    <a href={address} target="_blank" rel="noopener noreferrer">
      {children}
    </a>
  ) : (
    <span>{children}</span>
  );
}
function Thumbnail({ source, title }: { source: string; title: string }) {
  const [failed, setFailed] = useState(false);
  useEffect(() => setFailed(false), [source]);
  return failed ? (
    <span className="li-asset-thumbnail-failed">Preview unavailable</span>
  ) : (
    <img src={source} alt="" loading="lazy" decoding="async" onError={() => setFailed(true)} title={title} />
  );
}
function bytes(value: number) {
  return value < 1024 * 1024 ? `${Math.round(value / 1024)} KB` : `${(value / 1024 / 1024).toFixed(1)} MB`;
}
function moveThumbnailFocus(event: KeyboardEvent<HTMLDivElement>) {
  if (!['ArrowLeft', 'ArrowRight', 'ArrowUp', 'ArrowDown', 'Home', 'End'].includes(event.key)) return;
  const buttons = [...event.currentTarget.querySelectorAll<HTMLButtonElement>('button.li-asset-tile:not(:disabled)')];
  const index = buttons.indexOf(event.target as HTMLButtonElement);
  if (index < 0) return;
  const columns = Math.max(1, getComputedStyle(event.currentTarget).gridTemplateColumns.split(' ').length);
  const next =
    event.key === 'Home'
      ? 0
      : event.key === 'End'
        ? buttons.length - 1
        : index +
          (event.key === 'ArrowLeft'
            ? -1
            : event.key === 'ArrowRight'
              ? 1
              : event.key === 'ArrowUp'
                ? -columns
                : columns);
  event.preventDefault();
  event.stopPropagation();
  buttons[Math.max(0, Math.min(buttons.length - 1, next))]?.focus();
}

/** This component owns all three asset sources. It never relocates or invokes
 * a legacy control; mutations and document handoff go through its controller. */
export function AssetsDock({ controller }: { controller: AssetsController }) {
  const state = useSyncExternalStore(controller.subscribe, controller.getSnapshot);
  const locked = state.working || state.context.busy;
  const provider = state.providers.find(item => item.id === state.provider);
  const selected = state.stock.results.find(item => item.id === state.stockSelected);
  const folder = state.folders.find(item => item.id === state.folderSelected);
  const generated = controller.visibleGenerated();
  const canOpenGenerated = state.selectionMode ? state.selectedCopies.length === 1 : !!state.generatedSelected;
  const selectedVisible = generated.filter(item => state.selectedCopies.includes(item.id)).length;
  const [connectionOpen, setConnectionOpen] = useState(false),
    [key, setKey] = useState(''),
    [informationOpen, setInformationOpen] = useState(false);
  const folderInput = useRef<HTMLInputElement>(null),
    confirmButton = useRef<HTMLButtonElement>(null),
    generatedSelectButton = useRef<HTMLButtonElement>(null),
    confirming = useRef(false);
  useEffect(() => {
    setKey('');
    setConnectionOpen(false);
  }, [state.provider]);
  useEffect(() => {
    if (state.deleteSelection) {
      confirming.current = true;
      confirmButton.current?.focus();
    } else if (confirming.current) {
      confirming.current = false;
      generatedSelectButton.current?.focus();
    }
  }, [state.deleteSelection]);
  async function connect(disconnect = false) {
    const draft = key;
    setKey('');
    await controller.connectProvider(draft, disconnect);
  }
  return (
    <section
      className="li-assets"
      data-react-owned="true"
      data-expanded={state.expanded}
      aria-label="Assets"
      aria-busy={state.loading || state.working}
      hidden={!state.open}
      onKeyDown={event => {
        if (event.key === 'Escape' && state.expanded) {
          event.stopPropagation();
          controller.setExpanded(false);
        }
      }}
    >
      <header className="li-assets-header">
        <h2>Assets</h2>
        <Button
          size="small"
          appearance="subtle"
          disabled={locked}
          aria-expanded={state.expanded}
          onClick={() => controller.setExpanded(!state.expanded)}
        >
          {state.expanded ? 'Collapse' : 'Expand'}
        </Button>
        <Button
          size="small"
          appearance="subtle"
          aria-label="Close Assets"
          disabled={locked}
          onClick={controller.close}
          icon={<Icon name="close" />}
        />
      </header>
      <TabList
        size="small"
        selectedValue={state.tab}
        onTabSelect={(_, data) => {
          if (!locked) void controller.open(data.value as AssetTab);
        }}
        aria-label="Asset sources"
      >
        <Tab id="assets-stock-tab" aria-controls="assets-stock-panel" value="stock" disabled={locked}>
          Stock
        </Tab>
        <Tab id="assets-folders-tab" aria-controls="assets-folders-panel" value="folders" disabled={locked}>
          Folders
        </Tab>
        <Tab id="assets-generated-tab" aria-controls="assets-generated-panel" value="generated" disabled={locked}>
          Generated
        </Tab>
      </TabList>
      {state.tab === 'stock' && (
        <div id="assets-stock-panel" className="li-assets-source" role="tabpanel" aria-labelledby="assets-stock-tab">
          <form
            className="li-assets-search"
            onSubmit={event => {
              event.preventDefault();
              void controller.search();
            }}
          >
            <Input
              size="small"
              aria-label="Search stock photos"
              maxLength={120}
              value={state.query}
              disabled={locked}
              onChange={(_, data) => controller.setQuery(data.value)}
              placeholder="Search photos"
            />
            <Button
              size="small"
              type="submit"
              disabled={locked || state.loading || !state.query.trim() || !provider?.available}
            >
              Search
            </Button>
          </form>
          <div className="li-assets-source-select">
            <Select
              size="small"
              aria-label="Stock source"
              value={state.provider}
              disabled={locked}
              onChange={(_, data) => controller.setProvider(data.value as StockProviderId)}
            >
              {state.providers.map(item => (
                <option key={item.id} value={item.id}>
                  {item.label}
                </option>
              ))}
            </Select>
            {provider?.needs_key && (
              <Button
                size="small"
                appearance="subtle"
                aria-expanded={connectionOpen}
                onClick={() => setConnectionOpen(!connectionOpen)}
              >
                Connection
              </Button>
            )}
          </div>
          {provider?.needs_key && connectionOpen && (
            <div className="li-assets-connection">
              <Field label={`${provider.label} API key`}>
                <Input
                  type="password"
                  size="small"
                  autoComplete="off"
                  spellCheck={false}
                  value={key}
                  disabled={locked}
                  onChange={(_, data) => setKey(data.value)}
                  onKeyDown={event => {
                    if (event.key === 'Enter' && key.trim()) {
                      event.preventDefault();
                      void connect();
                    }
                  }}
                />
              </Field>
              <div className="li-assets-actions">
                <CreditLink href={provider.connect_url}>Get a key</CreditLink>
                <Button size="small" disabled={locked || !provider.available} onClick={() => void connect(true)}>
                  Disconnect
                </Button>
                <Button size="small" disabled={locked || !key.trim()} onClick={() => void connect()}>
                  Connect
                </Button>
              </div>
            </div>
          )}
          {provider?.needs_key && (
            <p className="li-assets-provider-credit">
              <CreditLink
                href={
                  provider.id === 'unsplash'
                    ? 'https://unsplash.com/?utm_source=local_image&utm_medium=referral'
                    : 'https://www.pexels.com/'
                }
              >
                Photos from {provider.label}
              </CreditLink>
            </p>
          )}
          <div className="li-assets-grid" aria-label="Stock search results" onKeyDown={moveThumbnailFocus}>
            {!state.stock.results.length && (
              <p className="li-assets-empty">
                {provider && !provider.available ? `Connect ${provider.label} to search.` : 'Search for a photo.'}
              </p>
            )}
            {state.stock.results.map(item => (
              <article className="li-asset-cell" key={item.id}>
                <Button
                  appearance="subtle"
                  className="li-asset-tile"
                  aria-label={item.title}
                  aria-pressed={state.stockSelected === item.id}
                  disabled={locked}
                  onClick={() => controller.selectStock(item.id)}
                >
                  <Thumbnail source={item.thumbnail_url} title={item.title} />
                  <span>{item.title}</span>
                </Button>
                <div className="li-asset-credit">
                  <CreditLink href={item.creator_url || item.source_url}>{item.creator}</CreditLink>
                </div>
              </article>
            ))}
          </div>
          <nav className="li-assets-pagination" aria-label="Stock result pages">
            <Button
              size="small"
              disabled={locked || state.loading || state.stock.page === 0}
              onClick={() => void controller.search(state.stock.page - 1)}
            >
              Previous
            </Button>
            <span>{state.stock.results.length ? `Page ${state.stock.page + 1}` : ''}</span>
            <Button
              size="small"
              disabled={locked || state.loading || state.stock.next_page == null}
              onClick={() => void controller.search(state.stock.next_page!)}
            >
              Next
            </Button>
          </nav>
          {selected && (
            <div className="li-assets-detail">
              <div className="li-assets-detail-title">
                <strong>{selected.title}</strong>
                <Button
                  size="small"
                  appearance="subtle"
                  aria-expanded={informationOpen}
                  onClick={() => setInformationOpen(!informationOpen)}
                >
                  Credits
                </Button>
              </div>
              {informationOpen && (
                <div className="li-assets-credits">
                  <p>
                    {selected.width} × {selected.height} px
                  </p>
                  <p>{selected.attribution}</p>
                  <CreditLink href={selected.source_url}>Original source</CreditLink>
                  <CreditLink href={selected.license_url}>{selected.license}</CreditLink>
                </div>
              )}
              <div className="li-assets-actions">
                <Button size="small" disabled={locked} onClick={() => void controller.importStock('image')}>
                  Open image
                </Button>
                <Button
                  size="small"
                  disabled={locked || !state.context.documentId}
                  onClick={() => void controller.importStock('background')}
                >
                  Background
                </Button>
                {state.context.canReference && (
                  <Button size="small" disabled={locked} onClick={() => void controller.importStock('reference')}>
                    Reference
                  </Button>
                )}
              </div>
            </div>
          )}
        </div>
      )}
      {state.tab === 'folders' && (
        <div
          id="assets-folders-panel"
          className="li-assets-source"
          role="tabpanel"
          aria-labelledby="assets-folders-tab"
        >
          <div className="li-assets-actions">
            <Button
              size="small"
              disabled={locked}
              onClick={() =>
                state.context.nativeReady ? void controller.attachFolder() : folderInput.current?.click()
              }
            >
              Attach folder…
            </Button>
            <Button size="small" disabled={locked || state.loading} onClick={() => void controller.refresh()}>
              Refresh
            </Button>
          </div>
          <input
            ref={element => {
              folderInput.current = element;
              element?.setAttribute('webkitdirectory', '');
            }}
            className="li-assets-file-picker"
            type="file"
            aria-label="Choose background folder"
            multiple
            accept=".jpg,.jpeg,.png,.tif,.tiff,.webp"
            onChange={event => {
              const files = [...(event.currentTarget.files ?? [])];
              if (files.length) controller.attachFiles(files);
              event.currentTarget.value = '';
            }}
          />
          {!!state.folders.length && (
            <Select
              size="small"
              aria-label="Background folder"
              value={state.folderSelected}
              disabled={locked}
              onChange={(_, data) => controller.selectFolder(data.value)}
            >
              {state.folders.map(item => (
                <option key={item.id} value={item.id}>
                  {item.name}
                </option>
              ))}
            </Select>
          )}
          <div className="li-assets-grid" aria-label="Folder backgrounds" onKeyDown={moveThumbnailFocus}>
            {!folder?.entries.length && <p className="li-assets-empty">Attach a folder to browse background images.</p>}
            {folder?.entries.map(item => (
              <Button
                key={item.id}
                appearance="subtle"
                className="li-asset-tile"
                aria-label={item.name}
                aria-pressed={state.folderEntrySelected === item.id}
                disabled={locked}
                onClick={() => controller.selectFolderEntry(item.id)}
              >
                <Thumbnail source={item.thumbnail} title={item.name} />
                <span>{item.name}</span>
              </Button>
            ))}
          </div>
          <div className="li-assets-actions">
            <Button
              size="small"
              disabled={locked || !state.context.documentId || !state.folderEntrySelected}
              onClick={() => void controller.applyFolder()}
            >
              Add background
            </Button>
          </div>
        </div>
      )}
      {state.tab === 'generated' && (
        <div
          id="assets-generated-panel"
          className="li-assets-source"
          role="tabpanel"
          aria-labelledby="assets-generated-tab"
        >
          <div className="li-assets-search">
            <Input
              size="small"
              aria-label="Filter generated images"
              value={state.generatedQuery}
              onChange={(_, data) => controller.setGeneratedQuery(data.value)}
              placeholder="Search images"
            />
            <Button size="small" disabled={locked || state.loading} onClick={() => void controller.refresh()}>
              Refresh
            </Button>
          </div>
          <div className="li-assets-actions">
            <Button
              ref={generatedSelectButton}
              size="small"
              aria-pressed={state.selectionMode}
              disabled={locked}
              onClick={() => controller.setSelectionMode(!state.selectionMode)}
            >
              {state.selectionMode ? 'Done' : 'Select'}
            </Button>
            {state.selectionMode && (
              <Checkbox
                label="Visible images"
                checked={
                  selectedVisible > 0 && selectedVisible < generated.length
                    ? 'mixed'
                    : generated.length > 0 && selectedVisible === generated.length
                }
                disabled={locked}
                onChange={(_, data) => controller.selectVisibleCopies(data.checked === true)}
              />
            )}
            <span className="li-assets-usage">
              {state.generated.count} images · {bytes(state.generated.bytes)}
            </span>
          </div>
          <div className="li-assets-grid" aria-label="Generated library images" onKeyDown={moveThumbnailFocus}>
            {!generated.length && (
              <p className="li-assets-empty">
                {state.generated.count ? 'No matching images.' : 'No generated images yet.'}
              </p>
            )}
            {generated.map(item => (
              <Button
                key={item.id}
                appearance="subtle"
                className="li-asset-tile"
                aria-label={item.name}
                aria-pressed={
                  state.selectionMode ? state.selectedCopies.includes(item.id) : state.generatedSelected === item.id
                }
                disabled={locked}
                onClick={() => controller.selectGenerated(item.id)}
              >
                <Thumbnail source={item.thumbnail} title={item.name} />
                <span>{item.name}</span>
                <small>
                  {item.width} × {item.height} · {item.model}
                </small>
              </Button>
            ))}
          </div>
          {state.deleteSelection ? (
            <div className="li-assets-confirm" role="group" aria-label="Confirm cached image deletion">
              <p>
                Delete {'all' in state.deleteSelection ? 'all' : state.deleteSelection.ids.length} cached library
                copies? Open documents, saved projects and model files are retained.
              </p>
              <div className="li-assets-actions">
                <Button size="small" disabled={locked} onClick={controller.cancelDelete}>
                  Cancel
                </Button>
                <Button
                  ref={confirmButton}
                  size="small"
                  disabled={locked}
                  onClick={() => void controller.deleteCopies()}
                >
                  Delete copies
                </Button>
              </div>
            </div>
          ) : (
            <div className="li-assets-actions">
              {state.selectionMode && (
                <Button
                  size="small"
                  disabled={locked || !state.selectedCopies.length}
                  onClick={() => controller.confirmDelete()}
                >
                  Delete selected
                </Button>
              )}
              <Button
                size="small"
                disabled={locked || !state.generated.count}
                onClick={() => controller.confirmDelete(true)}
              >
                Clear cache…
              </Button>
            </div>
          )}
          <div className="li-assets-actions">
            <Button
              size="small"
              disabled={locked || !canOpenGenerated}
              onClick={() => void controller.openGenerated('image')}
            >
              Open image
            </Button>
            <Button
              size="small"
              disabled={locked || !canOpenGenerated || !state.context.canUseDraft}
              onClick={() => void controller.openGenerated('draft')}
            >
              Use as draft
            </Button>
          </div>
        </div>
      )}
      <footer className="li-assets-status">
        {state.error ? (
          <p role="alert">{state.error}</p>
        ) : (
          <p role="status">{state.working ? 'Applying asset…' : state.loading ? 'Loading…' : state.status}</p>
        )}
      </footer>
    </section>
  );
}
