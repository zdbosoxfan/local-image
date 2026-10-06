import { useEffect, useLayoutEffect, useRef, useState, useSyncExternalStore } from 'react';
import {
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
  ProgressBar,
  Select,
  Spinner,
  Tab,
  TabList,
} from '@fluentui/react-components';
import type { ModelsController } from './controller.ts';
import type { DownloadJob, LoraItem, LoraSelection, ModelsSnapshot } from './types.ts';
import { Icon } from '../shell/Icon.tsx';
import { Hint } from '../shell/Hint.tsx';
import './models.css';

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
  const missing = variant?.missing_bytes ?? disk?.missing_bytes,
    total = variant?.total_bytes ?? disk?.total_bytes ?? model?.storage_bytes;
  const installed = variant?.available === true || missing === 0 || disk?.installed;
  const locked = state.loading || state.pendingNative || !!state.downloads?.running;
  const hardwareId = model?.id === 'qwen' ? `qwen-${variant?.id}` : model?.id;
  const hardware = state.hardware?.profiles?.find(item => 'id' in item && item.id === hardwareId);
  const hardwareInfo = variant?.hardware || model?.hardware;
  const block = controller.modelDownloadBlock();
  return (
    <>
      <div className="li-models-toolbar">
        <p className="li-models-note">Supported local models · capabilities and availability come from the backend.</p>
        <Button size="small" disabled={locked} onClick={() => void controller.refreshModels(true)}>
          Refresh
        </Button>
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
            <ul>
              {(model.strengths || model.notes || []).map(note => (
                <li key={note}>{note}</li>
              ))}
            </ul>
            <Field label="Precision">
              <Select
                aria-label="Model precision"
                value={state.selectedVariant}
                disabled={locked}
                onChange={event => controller.selectVariant(event.target.value)}
              >
                {model.variants?.map(item => (
                  <option key={item.id} value={item.id}>
                    {item.label}
                  </option>
                ))}
              </Select>
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
            <p className="li-models-note">
              {hardwareInfo?.basis ||
                hardware?.detail ||
                'Memory recommendations are planning estimates. CPU offloading uses system RAM and runs more slowly.'}
            </p>
            {!!model.limitations?.length && <p className="li-models-note">{model.limitations.join(' · ')}</p>}
            {sourceUrl(model.license?.url) && (
              <Link href={model.license?.url} target="_blank" rel="noopener noreferrer">
                {model.license?.label || 'License details'}
              </Link>
            )}
            <div className="li-models-actions">
              <Button
                size="small"
                appearance="primary"
                disabled={locked || !variant}
                onClick={() => controller.useModel()}
              >
                Use this model
              </Button>
              <Button size="small" disabled={!!block} title={block} onClick={() => void controller.downloadModel()}>
                {installed
                  ? 'Model files installed'
                  : model.downloadable === false || variant?.downloadable === false
                    ? 'Publisher access required'
                    : 'Download model'}
              </Button>
            </div>
            {block && <p className="li-models-note">{block}</p>}
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
      <JobProgress job={state.downloads} label="Model download" />
    </>
  );
}

function LoraBrowser({ controller, state }: { controller: ModelsController; state: ModelsSnapshot }) {
  const selected = state.selectedLoras,
    file = controller.selectedFile(),
    files = state.files;
  const compatibility = file?.compatibility || files?.compatibility || 'unverified';
  const block = controller.loraDownloadBlock();
  const availableItems = state.loraTab === 'installed' ? state.inventory?.installed || [] : state.searchResults;
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
        <Button
          size="small"
          disabled={state.loading || state.pendingNative}
          onClick={() => void controller.loadInventory()}
        >
          Refresh installed
        </Button>
      </div>
      {state.context?.supportsLoras === false && (
        <p role="alert" className="li-models-error">
          This workflow does not support adapters. Existing saved adapter settings are preserved.
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
      {state.loraTab === 'browse' && (
        <form
          className="li-lora-search"
          onSubmit={event => {
            event.preventDefault();
            void controller.search();
          }}
        >
          <Input
            type="search"
            aria-label="Search adapters"
            value={state.query}
            onChange={(_, value) => controller.setQuery(value.value)}
            placeholder="Style, subject or repository"
          />
          <Button type="submit" size="small" disabled={state.loading}>
            Search
          </Button>
          <Button size="small" disabled={state.loading} onClick={() => void controller.search()}>
            Refresh
          </Button>
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
            onInfo={() => controller.showInfo(item)}
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
      {state.info && (
        <section className="li-lora-detail" aria-label="Adapter information">
          <div className="li-models-toolbar">
            <h3>{state.info.title || state.info.filename || state.info.repo_id}</h3>
            <Hint content="Close adapter information">
              <Button
                size="small"
                appearance="subtle"
                aria-label="Close adapter information"
                onClick={() => controller.showInfo(null)}
                icon={<Icon name="close" />}
              />
            </Hint>
          </div>
          <LoraDetails item={state.info} controller={controller} />
          <p className="li-models-note">
            {state.info.preview_available
              ? `${state.info.example_source === 'local-test' ? 'Local test example.' : 'Publisher example; not independently tested.'} ${state.info.example_caption || ''}`
              : 'No image example is available.'}
          </p>
        </section>
      )}
      {state.filesLoading && <Spinner size="tiny" label="Reading adapter files" />}
      {files && (
        <section className="li-lora-detail" aria-label="Adapter file selection">
          <div className="li-models-toolbar">
            <h3>{files.repo_id}</h3>
            <Link href={repositoryUrl(files.repo_id)} target="_blank" rel="noopener noreferrer">
              Publisher page
            </Link>
          </div>
          <Field label="Adapter file">
            <Select
              aria-label="Adapter file"
              value={state.selectedFilename}
              disabled={state.pendingNative}
              onChange={event => controller.selectFile(event.target.value)}
            >
              {files.files.map(item => (
                <option key={item.filename} value={item.filename}>
                  {item.filename} · {bytes(item.bytes)}
                </option>
              ))}
            </Select>
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
  onUse,
  onInfo,
}: {
  item: LoraItem;
  action: string;
  disabled: boolean;
  onUse: () => void;
  onInfo: () => void;
}) {
  const [failed, setFailed] = useState(false),
    preview = previewUrl(item.preview_url),
    title = item.title || item.style || item.filename || item.repo_id || 'Adapter';
  return (
    <article className="li-lora-tile">
      <div className="li-lora-preview">
        {preview && !failed && item.preview_available !== false ? (
          <img src={preview} alt={`${title} example`} loading="lazy" onError={() => setFailed(true)} />
        ) : (
          <span>{failed ? 'Example unavailable' : 'No example yet'}</span>
        )}
      </div>
      <span className="li-models-note">
        {preview ? (item.example_source === 'local-test' ? 'Local test' : 'Publisher example') : 'Preview unavailable'}
      </span>
      <strong>{title}</strong>
      <span className="li-models-note">
        {item.compatibility === 'curated'
          ? 'Recommended for this model'
          : item.compatibility === 'declared'
            ? 'Publisher-declared compatibility'
            : 'Compatibility unverified'}
      </span>
      <div className="li-models-actions">
        <Button size="small" disabled={disabled} onClick={onUse}>
          {action}
        </Button>
        <Button size="small" appearance="subtle" aria-label={`Information about ${title}`} onClick={onInfo}>
          Info
        </Button>
      </div>
    </article>
  );
}
function LoraDetails({ item, controller }: { item: LoraItem; controller: ModelsController }) {
  return (
    <>
      <p className="li-models-note">{item.description}</p>
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
