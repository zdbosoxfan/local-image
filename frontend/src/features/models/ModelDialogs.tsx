import { ModelFiles } from './ModelFiles.tsx';
import { REMOVAL_MODEL_ID } from '../settings/removalModel.ts';
import { useEffect, useId, useLayoutEffect, useRef, useState, useSyncExternalStore } from 'react';
import {
  Accordion,
  AccordionHeader,
  AccordionItem,
  AccordionPanel,
  Button,
  Checkbox,
  Dialog,
  DialogActions,
  DialogBody,
  DialogContent,
  DialogSurface,
  DialogTitle,
  Field,
  Input,
  Link,
  Menu,
  MenuItemRadio,
  MenuList,
  MenuPopover,
  MenuTrigger,
  ProgressBar,
  Spinner,
  Tab,
  TabList,
} from '@fluentui/react-components';
import { Icon } from '../shell/Icon.tsx';
import type { ModelsController } from './controller.ts';
import type { DownloadJob, LoraItem, LoraSelection, ModelsSnapshot } from './types.ts';
import { Hint } from '../shell/Hint.tsx';
import './models.css';
import { ChoiceSelect } from '../shell/ChoiceSelect.tsx';

const bytes = (value: number | undefined) => {
  const amount = Number(value) || 0;
  return amount >= 1073741824
    ? `${(amount / 1073741824).toFixed(1)} GB`
    : amount >= 1048576
      ? `${(amount / 1048576).toFixed(0)} MB`
      : amount >= 1024
        ? `${(amount / 1024).toFixed(0)} KB`
        : `${amount} B`;
};
const sourceUrl = (value: string | undefined) => (value?.startsWith('https://') ? value : undefined);
function previewUrl(value: string | undefined) {
  try {
    if (!value) return undefined;
    const parsed = new URL(value, location.href);
    return parsed.origin === location.origin && parsed.pathname.startsWith('/api/local-remove/')
      ? parsed.href
      : undefined;
  } catch {
    return undefined;
  }
}
const repositoryUrl = (repo: string) => `https://huggingface.co/${repo.split('/').map(encodeURIComponent).join('/')}`;
const loraIdentity = (item: LoraItem) =>
  item.id || `${item.model || ''}/${item.repo_id || ''}/${item.filename || ''}/${item.revision || ''}`;

export function ModelDialogs({ controller }: { controller: ModelsController }) {
  const snapshot = useSyncExternalStore(controller.subscribe, controller.getSnapshot);
  const lastOpen = useRef(snapshot),
    open = snapshot.view !== null;
  useLayoutEffect(() => {
    if (open) lastOpen.current = snapshot;
  }, [open, snapshot]);
  const state = open ? snapshot : lastOpen.current;
  const title = state.view === 'loras' ? 'LoRA library' : 'Local image models';
  return (
    <Dialog
      open={open}
      onOpenChange={(_, value) => {
        if (!value.open) controller.close();
      }}
    >
      <DialogSurface className="li-models-surface" data-react-owned="true" aria-label={title}>
        <DialogBody className="li-models-body">
          <DialogTitle
            action={
              <Hint content={`Close ${title.toLowerCase()}`}>
                <Button
                  appearance="subtle"
                  aria-label={`Close ${title.toLowerCase()}`}
                  onClick={() => controller.close()}
                  icon={<Icon name="close" />}
                />
              </Hint>
            }
          >
            {title}
          </DialogTitle>
          <DialogContent className="li-models-content">
            {state.view === 'models' && <ModelBrowser controller={controller} state={state} />}
            {state.view === 'loras' && <LoraBrowser controller={controller} state={state} />}
            {state.loading && (
              <Spinner size="tiny" label={state.view === 'models' ? 'Checking models' : 'Reading installed adapters'} />
            )}
            {state.pendingNative && (
              <p className="li-models-note" role="status">
                Waiting for the desktop command. Complete or cancel any open folder dialog.
              </p>
            )}
            {state.error && (
              <p className="li-models-error" role="alert">
                {state.error}
              </p>
            )}
            {state.message && (
              <p className="li-models-note" role="status">
                {state.message}
              </p>
            )}
          </DialogContent>
          <DialogActions>
            <Button appearance="primary" onClick={() => controller.close()}>
              Done
            </Button>
          </DialogActions>
        </DialogBody>
      </DialogSurface>
    </Dialog>
  );
}

function ModelBrowser({ controller, state }: { controller: ModelsController; state: ModelsSnapshot }) {
  const model = controller.selectedModel(),
    variant = controller.selectedVariant();
  const listRef = useRef<HTMLDivElement>(null);
  const disk = state.downloads?.models
    ?.find(item => item.id === model?.id)
    ?.variants?.find(item => item.id === variant?.id);
  const missing = disk?.missing_bytes ?? variant?.missing_bytes,
    total = variant?.total_bytes ?? disk?.total_bytes ?? model?.storage_bytes;
  const installed = missing === 0 || disk?.installed || (missing === undefined && variant?.available === true);
  const locked =
    state.loading ||
    state.pendingNative ||
    !!state.downloads?.running ||
    state.setup?.job?.status === 'running' ||
    !!state.setup?.service?.starting ||
    !!state.setup?.service?.busy;
  const hardwareId = model?.id === 'qwen' ? `qwen-${variant?.id}` : model?.id;
  const hardware = state.hardware?.profiles?.find(item => 'id' in item && item.id === hardwareId);
  const hardwareInfo = variant?.hardware || model?.hardware;
  const block = controller.modelDownloadBlock();
  return (
    <>
      <div className="li-models-toolbar">
        <p className="li-models-note">Supported local models</p>
        <Hint content="Refresh models" relationship="description">
          <Button
            size="small"
            appearance="subtle"
            className="li-lora-icon-button"
            aria-label="Refresh models"
            disabled={locked}
            icon={<Icon name="refresh" />}
            onClick={() => void controller.refreshModels(true)}
          />
        </Hint>
      </div>
      <div className="li-model-browser-grid">
        <div className="li-model-list" role="listbox" aria-label="Supported models" ref={listRef}>
          {state.models.map((item, index) => (
            <Button
              key={item.id}
              appearance={item.id === model?.id ? 'secondary' : 'subtle'}
              role="option"
              aria-selected={item.id === model?.id}
              tabIndex={item.id === model?.id ? 0 : -1}
              disabled={locked}
              onClick={() => controller.selectModel(item.id)}
              onKeyDown={event => {
                const next =
                  event.key === 'ArrowDown'
                    ? Math.min(index + 1, state.models.length - 1)
                    : event.key === 'ArrowUp'
                      ? Math.max(0, index - 1)
                      : event.key === 'Home'
                        ? 0
                        : event.key === 'End'
                          ? state.models.length - 1
                          : null;
                if (next === null) return;
                event.preventDefault();
                event.stopPropagation();
                controller.selectModel(state.models[next].id);
                listRef.current?.querySelectorAll<HTMLButtonElement>('[role=option]')[next]?.focus();
              }}
            >
              <span>
                {item.label}
                <small>{item.available ? 'Ready' : 'Setup needed'}</small>
              </span>
            </Button>
          ))}
          {!state.loading && !state.models.length && (
            <p className="li-models-note">Model details are unavailable. Refresh to retry.</p>
          )}
        </div>
        {model && (
          <section className="li-model-detail" aria-label="Selected model details">
            <h3>{model.label}</h3>
            <p>{model.description || model.benefit}</p>
            <p className="li-models-note">
              {[
                model.capabilities?.text_to_image ? 'Text to image' : null,
                model.capabilities?.image_reference
                  ? `Up to ${model.capabilities.max_references ?? 0} reference images`
                  : model.capabilities?.image_to_image
                    ? 'Starting-image variation'
                    : null,
                model.capabilities?.transparent ? 'Transparent images' : 'Opaque images',
              ]
                .filter(Boolean)
                .join(' · ')}
            </p>
            <Field label="Precision">
              <ChoiceSelect
                id="model-browser-precision"
                label="Model precision"
                value={state.selectedVariant}
                disabled={locked}
                choices={(model.variants || []).map(item => ({ value: item.id, label: item.label }))}
                onSelect={controller.selectVariant}
              />
            </Field>
            <dl className="li-model-facts">
              <div>
                <dt>Status</dt>
                <dd>
                  {variant?.available
                    ? 'Ready in ComfyUI'
                    : installed
                      ? `Files installed · ${variant?.reason || model.reason || 'Connect ComfyUI to use them'}`
                      : variant?.reason || model.reason || 'Download required'}
                </dd>
              </div>
              <div>
                <dt>Download</dt>
                <dd>
                  {total
                    ? `${bytes(total)} total${missing === 0 ? ' · already installed' : missing !== undefined ? ` · ${bytes(missing)} remaining` : ''}`
                    : 'Size unavailable'}
                </dd>
              </div>
              <div>
                <dt>GPU memory</dt>
                <dd>{hardwareInfo?.vram_recommendation || hardware?.vram || 'See Hardware guide'}</dd>
              </div>
              <div>
                <dt>Recommended steps</dt>
                <dd>{model.recommended?.steps || model.defaults?.steps || '—'}</dd>
              </div>
            </dl>
            <ModelFiles files={disk?.files || variant?.files || []} />
            <Accordion collapsible className="li-model-notes">
              <AccordionItem key={model.id} value="notes">
                <AccordionHeader>Model notes and limitations</AccordionHeader>
                <AccordionPanel>
                  {!!(model.strengths || model.notes)?.length && (
                    <ul>
                      {(model.strengths || model.notes || []).map(note => (
                        <li key={note}>{note}</li>
                      ))}
                    </ul>
                  )}
                  <p className="li-models-note">
                    {hardwareInfo?.basis ||
                      hardware?.detail ||
                      'Memory recommendations are planning estimates. CPU offloading uses system RAM and runs more slowly.'}
                  </p>
                  {!!model.limitations?.length && <p className="li-models-note">{model.limitations.join(' · ')}</p>}
                </AccordionPanel>
              </AccordionItem>
            </Accordion>
            {sourceUrl(model.license?.url) && (
              <Link href={model.license?.url} target="_blank" rel="noopener noreferrer">
                {model.license?.label || 'License details'}
              </Link>
            )}
            <div className="li-models-actions">
              {model.id !== REMOVAL_MODEL_ID && (
                <Button
                  size="small"
                  appearance="primary"
                  disabled={locked || !variant}
                  onClick={() => controller.useModel()}
                >
                  Use this model
                </Button>
              )}
              <Button size="small" disabled={!!block} title={block} onClick={() => void controller.downloadModel()}>
                {installed
                  ? 'Model files installed'
                  : model.downloadable === false || variant?.downloadable === false
                    ? 'Publisher access required'
                    : 'Download model'}
              </Button>
            </div>
            {block && block !== 'Model files are already installed.' && <p className="li-models-note">{block}</p>}
          </section>
        )}
      </div>
      <div className="li-model-folder">
        <div className="li-models-actions">
          <Button
            size="small"
            disabled={!state.nativeSetup || locked}
            onClick={() => void controller.chooseModelDirectory()}
          >
            Models folder…
          </Button>
          <Button
            id="models-scan-folder"
            size="small"
            disabled={locked || !(state.setup?.model_directory || state.downloads?.model_directory)}
            onClick={() => void controller.scanModels()}
          >
            Scan folder for models
          </Button>
          {state.setup?.service?.can_start && !state.setup.service.running && (
            <Button
              size="small"
              disabled={!state.nativeSetup || locked || !!state.setup.service.starting}
              onClick={() => void controller.startBackend()}
            >
              Start AI backend
            </Button>
          )}
        </div>
        <output>
          {state.setup?.model_directory ||
            state.downloads?.model_directory ||
            'Choose where local model files are stored'}
        </output>
      </div>
      {state.setup?.model_folder_connection?.status !== 'unchanged' &&
        state.setup?.model_folder_connection?.message && (
          <p className="li-models-note" role="status">
            {state.setup.model_folder_connection.message}
          </p>
        )}
      {state.setup?.job?.action === 'download-models' && (
        <JobProgress
          job={{
            ...state.setup.job,
            running: state.setup.job.status === 'running',
            phase: state.setup.job.status === 'error' ? 'error' : state.setup.job.phase,
            progress: state.setup.job.progress == null ? undefined : state.setup.job.progress / 100,
          }}
          label="AI Remove model download"
        />
      )}
      <JobProgress job={state.downloads} label="Model download" />
    </>
  );
}

function LoraBrowser({ controller, state }: { controller: ModelsController; state: ModelsSnapshot }) {
  const selected = state.selectedLoras,
    file = controller.selectedFile(),
    files = state.files;
  const filePanel = useRef<HTMLElement>(null);
  useEffect(() => {
    if (files) filePanel.current?.scrollIntoView({ block: 'nearest' });
  }, [files]);
  const compatibility = file?.compatibility || files?.compatibility || 'unverified';
  const block = controller.loraDownloadBlock();
  const availableItems = controller.visibleLoras(),
    hiddenCount = controller.hiddenLoraCount();
  const searchLocked = state.loading || state.searching || state.pendingNative;
  return (
    <>
      <div className="li-models-toolbar">
        <p className="li-models-note">
          {state.context?.modelLabel} ·{' '}
          {state.context?.contextId === 'draft'
            ? 'Draft stage'
            : state.context?.contextId === 'final'
              ? 'Refinement stage'
              : 'Image generation'}
        </p>
        <Hint content="Refresh installed" relationship="description">
          <Button
            size="small"
            appearance="subtle"
            className="li-lora-icon-button"
            aria-label="Refresh installed"
            disabled={searchLocked}
            icon={<Icon name="refresh" />}
            onClick={() => void controller.loadInventory()}
          />
        </Hint>
      </div>
      {state.context?.supportsLoras === false && (
        <p role="alert" className="li-models-error">
          This workflow does not support adapters. Remove saved adapters below or choose a compatible model.
        </p>
      )}
      {!!selected.length && (
        <section className="li-selected-adapters" aria-label="Selected adapters">
          {selected.map(item => (
            <div key={item.id} className="li-selected-adapter">
              <span>
                {item.missing ? 'Unavailable · ' : ''}
                {item.title || item.id}
                {item.usage === 'reference-edit' && !state.context?.referenceCount && (
                  <small>Requires a reference image before generating</small>
                )}
              </span>
              <StrengthInput
                item={item}
                disabled={state.pendingNative}
                onCommit={value => controller.setStrength(item.id, value)}
              />
              <Hint content={`Remove ${item.title || item.id}`}>
                <Button
                  size="small"
                  appearance="subtle"
                  aria-label={`Remove ${item.title || item.id}`}
                  disabled={state.pendingNative}
                  onClick={() => controller.removeLora(item.id)}
                  icon={<Icon name="close" />}
                />
              </Hint>
            </div>
          ))}
        </section>
      )}
      <TabList
        size="small"
        selectedValue={state.loraTab}
        onTabSelect={(_, data) => controller.selectLoraTab(data.value as 'installed' | 'browse')}
        aria-label="Adapter library sections"
      >
        <Tab value="installed">Installed</Tab>
        <Tab value="browse" disabled={state.loading}>
          Browse
        </Tab>
      </TabList>
      <div className="li-lora-content-controls">
        <Checkbox
          id="loras-adult-content"
          label="Show mature content (NSFW)"
          checked={state.showAdultContent}
          disabled={state.loading || state.savingContentPreference}
          onChange={(_, data) => void controller.setShowAdultContent(data.checked === true)}
        />
        <p className="li-models-note">
          Publisher labels may be incomplete.{!state.showAdultContent ? ' Unrated previews require a click.' : ''}
          {hiddenCount > 0 ? ` ${hiddenCount} mature ${hiddenCount === 1 ? 'adapter hidden' : 'adapters hidden'}.` : ''}
        </p>
      </div>
      {state.loraTab === 'browse' && (
        <form
          className="li-lora-search"
          onSubmit={event => {
            event.preventDefault();
            if (!searchLocked) void controller.search();
          }}
        >
          <Input
            type="search"
            aria-label="Search adapters"
            value={state.query}
            onChange={(_, value) => controller.setQuery(value.value)}
            placeholder="Style, subject or repository"
            contentAfter={
              <Hint content="Search adapters" relationship="description">
                <Button
                  type="submit"
                  size="small"
                  appearance="subtle"
                  className="li-lora-icon-button"
                  aria-label="Search adapters"
                  disabled={searchLocked}
                  icon={<Icon name="search" />}
                />
              </Hint>
            }
          />
          <Hint content="Refresh adapter search" relationship="description">
            <Button
              type="button"
              size="small"
              appearance="subtle"
              className="li-lora-icon-button"
              aria-label="Refresh adapter search"
              disabled={searchLocked}
              icon={<Icon name="refresh" />}
              onClick={() => void controller.search()}
            />
          </Hint>
        </form>
      )}
      {state.searching && <Spinner size="tiny" label="Searching adapter metadata" />}
      {state.loraTab === 'browse' && (
        <p className="li-models-note">
          {state.checkedAt
            ? `Checked ${new Date(state.checkedAt).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })}`
            : 'Recommended examples remain available when online search is unavailable.'}
        </p>
      )}
      <div className="li-lora-grid">
        {availableItems.map((item, index) => (
          <LoraTile
            key={item.id || `${item.repo_id}/${index}`}
            item={item}
            showAdultContent={state.showAdultContent}
            controller={controller}
            expanded={!!state.info && loraIdentity(state.info) === loraIdentity(item)}
            onInfo={() =>
              controller.showInfo(state.info && loraIdentity(state.info) === loraIdentity(item) ? null : item)
            }
            onUse={() =>
              state.loraTab === 'installed' ? controller.useLora(item) : void controller.inspectFiles(item)
            }
            disabled={
              state.pendingNative ||
              item.supported === false ||
              state.context?.supportsLoras === false ||
              (state.loraTab === 'installed' && (selected.length >= 3 || selected.some(value => value.id === item.id)))
            }
            action={
              state.loraTab === 'installed'
                ? selected.some(value => value.id === item.id)
                  ? 'Added'
                  : 'Use'
                : item.supported === false
                  ? 'Unsupported'
                  : 'Choose file…'
            }
          />
        ))}
      </div>
      {!state.loading && !availableItems.length && (
        <div className="li-models-note">
          <p>
            {state.loraTab === 'installed'
              ? 'No adapters installed for this model.'
              : state.query
                ? 'No matching adapters. Clear the query to show recommended examples.'
                : 'No recommended examples are available for this model yet.'}
          </p>
          {state.loraTab === 'browse' && state.query && (
            <Button size="small" onClick={() => void controller.showRecommended()}>
              Show recommended
            </Button>
          )}
        </div>
      )}
      {state.filesLoading && <Spinner size="tiny" label="Reading adapter files" />}
      {files && (
        <section ref={filePanel} className="li-lora-detail" aria-label="Adapter file selection">
          <div className="li-models-toolbar">
            <h3>{files.repo_id}</h3>
            <Link href={repositoryUrl(files.repo_id)} target="_blank" rel="noopener noreferrer">
              Publisher page
            </Link>
          </div>
          <Field label="Adapter file">
            <ChoiceSelect
              id="lora-adapter-file"
              label="Adapter file"
              value={state.selectedFilename}
              disabled={state.pendingNative}
              choices={files.files.map(item => ({
                value: item.filename || '',
                label: `${item.filename} · ${bytes(item.bytes)}`,
              }))}
              onSelect={controller.selectFile}
            />
          </Field>
          <p className="li-models-note">
            {file?.warning ||
              files.warning ||
              (compatibility === 'curated'
                ? 'This exact file is recommended for the selected model. Downloading does not automatically enable it.'
                : compatibility === 'declared'
                  ? 'The publisher declares compatibility. Review this file before assigning it to the model.'
                  : 'Compatibility is unverified. Review the publisher page before assigning this file.')}
          </p>
          {compatibility !== 'curated' && (
            <Checkbox
              label={`${compatibility === 'declared' ? 'Accept publisher-declared compatibility' : 'Accept unverified compatibility'} and assign to ${state.context?.modelLabel}`}
              checked={state.allowUnverified}
              onChange={(_, data) => controller.acknowledgeCompatibility(data.checked === true)}
            />
          )}
          {file && <LoraDetails item={file} controller={controller} />}
          {!file && (
            <p className="li-lora-full-description">{files.description || 'Publisher description unavailable.'}</p>
          )}
          <Button size="small" disabled={!!block} title={block} onClick={() => void controller.downloadLora()}>
            Download adapter
          </Button>
          {block && (
            <p role="status" className="li-models-note">
              {block}
            </p>
          )}
        </section>
      )}
      <JobProgress
        job={state.loraJob}
        label={
          state.loraJob?.model && state.loraJob.model !== state.context?.modelId
            ? 'Other model adapter download'
            : 'Adapter download'
        }
      />
    </>
  );
}

function StrengthInput({
  item,
  disabled,
  onCommit,
}: {
  item: LoraSelection;
  disabled: boolean;
  onCommit: (value: number) => void;
}) {
  const [draft, setDraft] = useState(String(item.strength)),
    input = useRef<HTMLInputElement>(null);
  useEffect(() => setDraft(String(item.strength)), [item.strength]);
  const commit = () => {
    const value = Number(draft);
    if (draft.trim() && Number.isFinite(value)) onCommit(Math.max(-2, Math.min(2, value)));
    else setDraft(String(item.strength));
  };
  return (
    <Input
      size="small"
      ref={input}
      type="number"
      min={-2}
      max={2}
      step={0.05}
      aria-label={`Strength for ${item.title || item.id}`}
      value={draft}
      disabled={disabled}
      onChange={(_, data) => setDraft(data.value)}
      onBlur={commit}
      onKeyDown={event => {
        event.stopPropagation();
        if (event.key === 'Enter') {
          event.preventDefault();
          input.current?.blur();
        }
        if (event.key === 'Escape') setDraft(String(item.strength));
      }}
    />
  );
}
function LoraTile({
  item,
  action,
  disabled,
  expanded,
  showAdultContent,
  controller,
  onUse,
  onInfo,
}: {
  item: LoraItem;
  action: string;
  disabled: boolean;
  expanded: boolean;
  showAdultContent: boolean;
  controller: ModelsController;
  onUse: () => void;
  onInfo: () => void;
}) {
  const [failed, setFailed] = useState(false),
    preview = previewUrl(item.preview_url),
    title = item.title || item.style || item.filename || item.repo_id || 'Adapter';
  const consentKey = `${loraIdentity(item)}|${preview || ''}|${showAdultContent}|${item.content_rating || 'unknown'}`,
    [revealedKey, setRevealedKey] = useState<string | null>(null);
  const revealed = revealedKey === consentKey,
    unknownPublisher =
      (!item.content_rating || item.content_rating === 'unknown') && item.example_source === 'publisher';
  const requiresConsent =
    (item.preview_requires_consent === true || unknownPublisher) && !showAdultContent && !revealed;
  const matureHidden = item.content_rating === 'adult' && !showAdultContent;
  const detailsId = useId(),
    availablePreview = !!preview && !failed && item.preview_available !== false,
    hasPreview = availablePreview && !requiresConsent && !matureHidden;
  let imageSource = preview;
  if (preview) {
    const url = new URL(preview);
    url.searchParams.delete('show_adult');
    url.searchParams.delete('show_unrated');
    if (showAdultContent) url.searchParams.set('show_adult', 'true');
    else if (revealed && (item.preview_requires_consent || unknownPublisher))
      url.searchParams.set('show_unrated', 'true');
    imageSource = url.href;
  }
  useEffect(() => setFailed(false), [preview]);
  useEffect(() => setRevealedKey(null), [consentKey]);
  return (
    <article className="li-lora-tile" aria-label={`${title} adapter`}>
      <div className="li-lora-preview-group">
        <div className="li-lora-preview">
          {hasPreview ? (
            <img src={imageSource} alt={`${title} example`} loading="lazy" onError={() => setFailed(true)} />
          ) : requiresConsent && availablePreview && !matureHidden ? (
            <div className="li-lora-preview-consent">
              <span>Unrated publisher preview</span>
              <Button size="small" onClick={() => setRevealedKey(consentKey)}>
                Show unrated preview
              </Button>
            </div>
          ) : (
            <span>{matureHidden ? 'Mature preview hidden' : failed ? 'Example unavailable' : 'No example yet'}</span>
          )}
        </div>
        <span className="li-models-note">
          {availablePreview
            ? item.example_source === 'local-test'
              ? 'Local test example'
              : 'Publisher example'
            : 'Preview unavailable'}
        </span>
      </div>
      <div className="li-lora-content">
        <h3 className="li-lora-title">{title}</h3>
        <p className="li-lora-metadata">
          <span>
            {item.compatibility === 'curated'
              ? 'Recommended for this model'
              : item.compatibility === 'declared'
                ? 'Publisher-declared compatibility'
                : 'Compatibility unverified'}
          </span>
          <span aria-hidden="true"> · </span>
          <span className={item.content_rating === 'adult' ? 'li-lora-adult' : undefined}>
            {item.content_rating === 'adult'
              ? 'Mature / NSFW'
              : item.content_rating === 'general'
                ? 'Publisher: general'
                : 'Not labeled'}
          </span>
        </p>
        <p className="li-lora-description">{item.description || 'Publisher description unavailable.'}</p>
        <div className="li-models-actions">
          <Button size="small" disabled={disabled} onClick={onUse}>
            {action}
          </Button>
          <Button
            size="small"
            appearance="subtle"
            aria-label={`Details for ${title}`}
            aria-expanded={expanded}
            aria-controls={detailsId}
            onClick={onInfo}
          >
            {expanded ? 'Hide details' : 'Details'}
          </Button>
        </div>
      </div>
      {expanded && (
        <section id={detailsId} className="li-lora-expanded-details" aria-label={`Details for ${title}`}>
          <LoraDetails item={item} controller={controller} />
          <p className="li-models-note">
            {hasPreview
              ? `${item.example_source === 'local-test' ? 'Local test example.' : 'Publisher example; not independently tested.'} ${item.example_caption || ''}`
              : 'No image example is available.'}
          </p>
          {item.content_rating_source && (
            <p className="li-models-note">Content label source: {item.content_rating_source}</p>
          )}
        </section>
      )}
    </article>
  );
}
function LoraDetails({ item, controller }: { item: LoraItem; controller: ModelsController }) {
  return (
    <>
      <p className="li-lora-full-description">{item.description || 'Publisher description unavailable.'}</p>
      <p className="li-models-note">
        {[
          item.experimental ? 'Experimental' : null,
          item.style,
          item.usage === 'reference-edit'
            ? 'Requires a reference image'
            : item.usage === 'text-to-image'
              ? 'Text to image'
              : item.usage === 'both'
                ? 'Text or reference images'
                : null,
          item.license,
          item.license_note,
        ]
          .filter(Boolean)
          .join(' · ')}
      </p>
      {item.warning && <p className="li-models-note">{item.warning}</p>}
      {item.trigger_phrase && <p className="li-models-note">Trigger: {item.trigger_phrase}</p>}
      <div className="li-models-actions">
        {item.trigger_phrase && (
          <Button size="small" onClick={() => controller.addTrigger(item.trigger_phrase!)}>
            Add trigger
          </Button>
        )}
        {item.recommended_settings && (
          <Button size="small" onClick={() => controller.applySampling(item.recommended_settings!)}>
            Apply recommended sampling
          </Button>
        )}
        {item.repo_id && (
          <Link href={repositoryUrl(item.repo_id)} target="_blank" rel="noopener noreferrer">
            Publisher and license details
          </Link>
        )}
      </div>
    </>
  );
}
function JobProgress({ job, label }: { job: DownloadJob | null | undefined; label: string }) {
  if (!job || job.phase === 'idle') return null;
  const value =
    job.progress != null && Number.isFinite(job.progress) ? Math.max(0, Math.min(1, job.progress)) : undefined;
  return (
    <section className="li-model-download" aria-label={label}>
      <strong>{label}</strong>
      {job.running && <ProgressBar value={value} aria-label={label} />}
      <p role={job.phase === 'error' ? 'alert' : 'status'}>{job.error || job.message || job.phase}</p>
      {job.running && value !== undefined && <span>{Math.round(value * 100)}%</span>}
      {!!job.downloaded_bytes && (
        <span>
          {bytes(job.downloaded_bytes)}
          {job.total_bytes ? ` of ${bytes(job.total_bytes)}` : ' downloaded'}
        </span>
      )}
    </section>
  );
}
