import { useEffect, useLayoutEffect, useRef, useState, useSyncExternalStore } from 'react';
import { ModelFiles } from '../models/ModelFiles.tsx';
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
  Link,
  Menu,
  MenuItem,
  MenuItemRadio,
  MenuList,
  MenuPopover,
  MenuTrigger,
  ProgressBar,
  Spinner,
  Tab,
  TabList,
  Table,
  TableBody,
  TableCell,
  TableHeader,
  TableHeaderCell,
  TableRow,
} from '@fluentui/react-components';
import { Icon } from '../shell/Icon.tsx';
import type { SettingsController } from './settingsController.ts';
import type { InterfaceDensity, SettingsSnapshot, SetupState } from './types.ts';
import { Hint } from '../shell/Hint.tsx';
import { modelDownloadSelection } from './modelDownload.ts';
import './settings.css';
import { ChoiceSelect } from '../shell/ChoiceSelect.tsx';

export function setupBytes(value: number | undefined) {
  const amount = Number(value) || 0;
  if (amount >= 1073741824) return `${(amount / 1073741824).toFixed(1)} GB`;
  if (amount >= 1048576) return `${(amount / 1048576).toFixed(0)} MB`;
  if (amount >= 1024) return `${(amount / 1024).toFixed(0)} KB`;
  return `${amount} B`;
}
function storageText(value: { free_bytes?: number; error?: string } | undefined) {
  return value?.error
    ? `Folder unavailable: ${value.error}`
    : Number.isFinite(value?.free_bytes)
      ? `${setupBytes(value?.free_bytes)} available on this drive.`
      : '';
}
function setupSummary(setup: SetupState | null) {
  if (!setup) return 'Checking local AI setup…';
  if (setup.job?.status === 'error') return setup.job.error || setup.job.message || 'Setup could not finish.';
  if (setup.job?.status === 'running') return setup.job.message || 'Local AI setup is running.';
  if (setup.service?.starting) return 'Starting the AI backend…';
  if (setup.service?.ready) return 'Local AI is ready.';
  if (setup.service?.running) return setup.service.reason || 'ComfyUI is running, but no supported AI model is ready.';
  if (!setup.installation)
    return setup.portable?.available === false
      ? 'Choose an existing ComfyUI installation.'
      : 'Choose an existing ComfyUI installation or install a dedicated copy.';
  if (!setup.model_directory) return 'Choose a folder for model files.';
  return 'Start the local AI backend when you need a model.';
}

export function SettingsDialogs({ controller }: { controller: SettingsController }) {
  const snapshot = useSyncExternalStore(controller.subscribe, controller.getSnapshot);
  const lastOpen = useRef(snapshot),
    open = snapshot.view !== null;
  useLayoutEffect(() => {
    if (open) lastOpen.current = snapshot;
  }, [open, snapshot]);
  const state = open ? snapshot : lastOpen.current;
  const [tab, setTab] = useState('general');
  useEffect(() => {
    if (snapshot.view === 'settings') setTab(snapshot.requestedSection);
  }, [snapshot.view, snapshot.requestedSection]);
  const title =
    state.view === 'hardware' ? 'Hardware guide' : state.view === 'shortcuts' ? 'Keyboard shortcuts' : 'Settings';
  return (
    <Dialog
      open={open}
      onOpenChange={(_, data) => {
        if (!data.open) controller.close();
      }}
    >
      <DialogSurface className="li-settings-surface" data-react-owned="true" aria-label={title}>
        <DialogBody className="li-settings-body">
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
          <DialogContent className="li-settings-content">
            {state.view === 'settings' && (
              <>
                <TabList
                  size="small"
                  selectedValue={tab}
                  onTabSelect={(_, data) => setTab(String(data.value))}
                  aria-label="Settings sections"
                >
                  <Tab value="general">General</Tab>
                  <Tab value="ai">Local AI</Tab>
                </TabList>
                {tab === 'general' ? (
                  <section className="li-settings-section" aria-label="General settings">
                    <Checkbox
                      label="Ask before overwriting original images"
                      checked={state.preferences.askBeforeOverwrite}
                      onChange={(_, data) => controller.setAskBeforeOverwrite(data.checked === true)}
                    />
                    <Field className="li-settings-interface-size" label="Interface size">
                      <ChoiceSelect
                        id="settings-interface-size"
                        label="Interface size"
                        value={state.preferences.density}
                        choices={[
                          { value: 'compact', label: 'Compact' },
                          { value: 'comfortable', label: 'Comfortable' },
                          { value: 'large', label: 'Large · 200% text' },
                        ]}
                        onSelect={value => controller.setDensity(value as InterfaceDensity)}
                      />
                    </Field>
                    <div className="li-settings-actions li-settings-help">
                      <Button size="small" appearance="subtle" onClick={() => void controller.open('hardware')}>
                        Hardware guide
                      </Button>
                      <Button size="small" appearance="subtle" onClick={() => void controller.open('shortcuts')}>
                        Keyboard shortcuts
                      </Button>
                    </div>
                    <UpdatesSection state={state} controller={controller} />
                    <Accordion collapsible>
                      <AccordionItem value="quick-heal">
                        <AccordionHeader>About Quick Heal</AccordionHeader>
                        <AccordionPanel>
                          <p>
                            Texture repair copies nearby texture to cover small objects. Dust &amp; scratches smooths
                            tiny defects. Both run locally on the CPU without model downloads.
                          </p>
                          <p>
                            <Link
                              href="https://github.com/EmbarkStudios/texture-synthesis"
                              target="_blank"
                              rel="noopener noreferrer"
                            >
                              Texture synthesis
                            </Link>{' '}
                            ·{' '}
                            <Link
                              href="https://docs.opencv.org/4.x/df/d3d/tutorial_py_inpainting.html"
                              target="_blank"
                              rel="noopener noreferrer"
                            >
                              OpenCV inpainting
                            </Link>
                          </p>
                        </AccordionPanel>
                      </AccordionItem>
                    </Accordion>
                  </section>
                ) : (
                  <LocalAi controller={controller} state={state} />
                )}
              </>
            )}
            {state.view === 'hardware' && <Hardware controller={controller} state={state} />}
            {state.view === 'shortcuts' && <Shortcuts />}
            {state.loading && (
              <Spinner
                size="tiny"
                label={state.view === 'hardware' ? 'Reading hardware information' : 'Refreshing local setup'}
              />
            )}
            {state.pendingAction && (
              <p role="status" className="li-settings-note">
                {['chooseRuntime', 'chooseInstallDirectory', 'chooseModelDirectory', 'configureConnection'].includes(
                  state.pendingAction,
                )
                  ? 'Complete or cancel the desktop dialog to continue.'
                  : 'Waiting for the desktop command…'}
              </p>
            )}
            {state.error && (
              <p role="alert" className="li-settings-error">
                {state.error}
              </p>
            )}
            {state.message && (
              <p role="status" className="li-settings-note">
                {state.message}
              </p>
            )}
          </DialogContent>
          <DialogActions>
            {state.view === 'hardware' ? (
              <Button
                appearance="primary"
                disabled={state.savingHardwarePreference}
                onClick={() => controller.continueHardware()}
              >
                Continue
              </Button>
            ) : (
              <Button onClick={() => controller.close()}>Done</Button>
            )}
          </DialogActions>
        </DialogBody>
      </DialogSurface>
    </Dialog>
  );
}

function LocalAi({ controller, state }: { controller: SettingsController; state: SettingsSnapshot }) {
  const setup = state.setup,
    service = setup?.service,
    job = setup?.job;
  const selected = modelDownloadSelection(state),
    modelJob = state.modelDownloads;
  const locked = controller.locked() || state.loading;
  const nativeLocked = locked || !state.capabilities.setup;
  const progress =
    job?.progress != null && Number.isFinite(job.progress) ? Math.min(100, Math.max(0, job.progress)) / 100 : undefined;
  const installPath = setup?.install_directory || setup?.managed_directory || setup?.configured_ai_directory;
  const portableAvailable = setup?.portable?.available !== false;
  const modelProgress =
    modelJob?.progress != null && Number.isFinite(modelJob.progress)
      ? Math.min(1, Math.max(0, modelJob.progress))
      : undefined;
  const downloadBlock = controller.modelDownloadBlock();
  return (
    <section className="li-settings-section" aria-label="Local AI setup">
      <div className="li-settings-toolbar">
        <p role="status" className="li-settings-note">
          {setupSummary(setup)}
        </p>
        <Hint content="Refresh local AI setup">
          <Button
            size="small"
            appearance="subtle"
            icon={<Icon name="refresh" />}
            aria-label="Refresh local AI setup"
            disabled={state.loading || !!state.pendingAction}
            onClick={() => void controller.refresh()}
          />
        </Hint>
      </div>
      {!state.capabilities.setup && (
        <p className="li-settings-note">
          {state.capabilities.ready
            ? 'Update the desktop host to use guided AI setup.'
            : portableAvailable
              ? 'Use the Local Image desktop app to choose local folders, install ComfyUI or download model files.'
              : 'Use the Local Image desktop app to choose your ComfyUI folder or download model files.'}
        </p>
      )}
      <section className="li-settings-group" aria-labelledby="react-settings-runtime">
        <h3 id="react-settings-runtime">AI backend · ComfyUI</h3>
        <output className="li-settings-path" aria-label="ComfyUI installation">
          {setup?.installation?.path || 'No installation selected'}
        </output>
        <div className="li-settings-actions">
          <Button
            size="small"
            disabled={nativeLocked || !service?.can_start || !!service?.running}
            onClick={() => void controller.run('startBackend')}
          >
            {service?.starting ? 'Starting…' : service?.running ? 'Backend running' : 'Start AI backend'}
          </Button>
          <Button size="small" disabled={nativeLocked} onClick={() => void controller.run('chooseRuntime')}>
            Choose installation…
          </Button>
          <Button size="small" appearance="subtle" disabled={locked} onClick={() => void controller.refresh(true)}>
            Detect installations
          </Button>
        </div>
        {state.showInstallations && !!setup?.installations?.length && (
          <div className="li-settings-installation-choice">
            <Field label="Detected installation">
              <ChoiceSelect
                id="settings-detected-installation"
                label="Detected installation"
                value={state.selectedInstallation}
                disabled={locked}
                choices={setup.installations.map(item => ({
                  value: item.id || item.path,
                  label: `${item.name || 'ComfyUI'} · ${item.path}`,
                }))}
                onSelect={controller.selectInstallation}
              />
            </Field>
            <Button
              size="small"
              disabled={nativeLocked || !state.selectedInstallation}
              onClick={() => void controller.run('useInstallation')}
            >
              Use selected
            </Button>
          </div>
        )}
        {service?.running && !service.ready && (
          <p className="li-settings-note">
            {service.reason || 'No supported model is ready.'} If model folders changed, close ComfyUI, then start the
            backend again and refresh.
          </p>
        )}
        {service?.running && service.ready && (
          <p className="li-settings-note">Connected on this computer{service.port ? ` · port ${service.port}` : ''}.</p>
        )}
        {setup?.installation && !setup.installation.startable && !service?.running && (
          <p className="li-settings-note">
            {portableAvailable
              ? 'Start this installation from ComfyUI, or use a dedicated portable copy.'
              : 'Start this installation from ComfyUI, then refresh.'}
          </p>
        )}
        {portableAvailable ? (
          <Accordion collapsible>
            <AccordionItem value="portable">
              <AccordionHeader>Portable runtime</AccordionHeader>
              <AccordionPanel className="li-settings-disclosure">
                <output className="li-settings-path" aria-label="Portable runtime folder">
                  {installPath || 'Choose a writable folder for a dedicated runtime'}
                </output>
                <p className="li-settings-note">
                  {setup?.portable?.download_bytes
                    ? `Runtime download: ${setupBytes(setup.portable.download_bytes)}. `
                    : ''}
                  {setup?.portable?.minimum_free_bytes
                    ? `Allow ${setupBytes(setup.portable.minimum_free_bytes)} for installation. `
                    : ''}
                  {storageText(setup?.storage?.portable_folder)}
                </p>
                <div className="li-settings-actions">
                  <Button
                    size="small"
                    disabled={nativeLocked}
                    onClick={() => void controller.run('chooseInstallDirectory')}
                  >
                    Choose install folder…
                  </Button>
                  <Button size="small" disabled={nativeLocked} onClick={() => void controller.run('installRuntime')}>
                    Install portable ComfyUI
                  </Button>
                </div>
              </AccordionPanel>
            </AccordionItem>
          </Accordion>
        ) : (
          <p className="li-settings-note">Use an existing ComfyUI installation; choose its folder above.</p>
        )}
      </section>
      <section className="li-settings-group" aria-labelledby="react-settings-models">
        <div className="li-settings-toolbar">
          <h3 id="react-settings-models">Models</h3>
          <Button size="small" appearance="subtle" disabled={locked} onClick={() => controller.browseModels()}>
            Model details…
          </Button>
        </div>
        <div className="li-settings-folder-row">
          <div>
            <span className="li-settings-note">Model folder</span>
            <output className="li-settings-path" aria-label="Model folder">
              {setup?.model_directory || modelJob?.model_directory || 'No model folder selected'}
            </output>
          </div>
          <Button size="small" disabled={nativeLocked} onClick={() => void controller.run('chooseModelDirectory')}>
            Choose folder…
          </Button>
          <Button
            id="settings-scan-model-folder"
            size="small"
            disabled={locked || !(setup?.model_directory || modelJob?.model_directory)}
            onClick={() => void controller.scanModels()}
          >
            Scan folder for models
          </Button>
        </div>
        {storageText(setup?.storage?.model_folder) && (
          <p className="li-settings-note">{storageText(setup?.storage?.model_folder)}</p>
        )}
        {setup?.model_folder_connection?.status !== 'unchanged' && setup?.model_folder_connection?.message && (
          <p className="li-settings-note">{setup.model_folder_connection.message}</p>
        )}
        <div className="li-settings-model-download">
          <Field label="Model">
            <ChoiceSelect
              id="settings-download-model"
              label="Model to download"
              value={state.selectedModelId}
              disabled={locked}
              choices={state.models.map(model => ({
                value: model.id,
                label: model.id === 'qwen' ? `${model.label} · backgrounds & images` : model.label,
              }))}
              onSelect={controller.selectModel}
            />
          </Field>
          <Field label="Precision">
            {(selected.model?.variants?.length || 0) > 1 ? (
              <ChoiceSelect
                id="settings-download-variant"
                label="Download precision"
                value={state.selectedVariant}
                disabled={locked}
                choices={(selected.model?.variants || []).map(variant => ({ value: variant.id, label: variant.label }))}
                onSelect={controller.selectVariant}
              />
            ) : (
              <span className="li-settings-single-variant">{selected.variant?.label || '—'}</span>
            )}
          </Field>
        </div>
        <ModelFiles files={selected.files} />
        <div className="li-settings-download-action">
          <Button
            id="settings-download-model-button"
            size="small"
            appearance="primary"
            disabled={!!downloadBlock}
            title={downloadBlock || undefined}
            onClick={() => void controller.run('downloadModel')}
          >
            {selected.ready && (selected.filesPresent || selected.missing === undefined)
              ? 'Ready'
              : selected.filesPresent
                ? 'Files present'
                : !selected.downloadable
                  ? 'Publisher access required'
                  : 'Download model'}
          </Button>
          {selected.model && (
            <p id="settings-model-status" className="li-settings-note">
              {selected.ready && (selected.filesPresent || selected.missing === undefined)
                ? 'Ready in the AI backend.'
                : selected.filesPresent
                  ? 'Model files are present. Start the AI backend to use them.'
                  : selected.missing !== undefined
                    ? `${setupBytes(selected.missing)} to download${selected.total ? ` · ${setupBytes(selected.total)} total` : ''}.`
                    : selected.total
                      ? `${setupBytes(selected.total)} total download.`
                      : 'Download size is unavailable.'}
            </p>
          )}
        </div>
        {!selected.downloadable && selected.note && <p className="li-settings-note">{selected.note}</p>}
        {!selected.ready && !selected.filesPresent && downloadBlock === 'Choose a model folder first.' && (
          <p className="li-settings-note">{downloadBlock}</p>
        )}
        {modelJob && (modelJob.running || modelJob.phase === 'complete' || modelJob.phase === 'error') && (
          <section className="li-settings-job" aria-label="Model download">
            <strong>
              {modelJob.running
                ? 'Downloading model'
                : modelJob.phase === 'complete'
                  ? 'Model download complete'
                  : 'Model download needs attention'}
            </strong>
            {modelJob.running && <ProgressBar value={modelProgress} aria-label="Model download progress" />}
            <p role={modelJob.phase === 'error' ? 'alert' : 'status'}>{modelJob.error || modelJob.message || ''}</p>
            {modelProgress !== undefined && <span>{Math.round(modelProgress * 100)}%</span>}
            {!!modelJob.downloaded_bytes && (
              <span>
                {setupBytes(modelJob.downloaded_bytes)}
                {modelJob.total_bytes ? ` of ${setupBytes(modelJob.total_bytes)}` : ' downloaded'}
              </span>
            )}
          </section>
        )}
      </section>
      {job && (
        <section className="li-settings-job" aria-label="Setup progress">
          <strong>
            {job.status === 'complete'
              ? 'Setup complete'
              : job.status === 'error'
                ? 'Setup needs attention'
                : job.phase || 'Setting up local AI'}
          </strong>
          {job.status === 'running' && <ProgressBar value={progress} aria-label="Setup progress" />}
          <p role={job.status === 'error' ? 'alert' : 'status'}>{job.error || job.message || ''}</p>
          {progress !== undefined && <span>{Math.round(progress * 100)}%</span>}
          {!!job.downloaded_bytes && (
            <span>
              {setupBytes(job.downloaded_bytes)}
              {job.total_bytes ? ` of ${setupBytes(job.total_bytes)}` : ' downloaded'}
            </span>
          )}
        </section>
      )}
      <section className="li-settings-group">
        <div className="li-settings-toolbar">
          <div>
            <h3>GPU memory</h3>
            <p className="li-settings-note">{service?.device || 'Start the backend to check your device.'}</p>
          </div>
          <Hint
            content={
              nativeLocked
                ? 'Finish the current operation before unloading GPU models'
                : !service?.can_eject
                  ? 'Start the AI backend to unload its GPU models'
                  : 'Unload GPU models; keep files on disk'
            }
            relationship="description"
          >
            <Button
              size="small"
              appearance="subtle"
              className="li-settings-icon-button"
              aria-label="Unload GPU models"
              disabled={nativeLocked || !service?.can_eject}
              icon={<Icon name="eject" />}
              onClick={() => void controller.run('ejectModels')}
            />
          </Hint>
        </div>
      </section>
      <Accordion collapsible>
        <AccordionItem value="connection">
          <AccordionHeader>Advanced connection</AccordionHeader>
          <AccordionPanel>
            <p className="li-settings-note">
              Choose a different local ComfyUI port or model location in the desktop connection dialog.
            </p>
            <Button
              size="small"
              disabled={locked || !state.capabilities.ready}
              onClick={() => void controller.run('configureConnection')}
            >
              AI connection…
            </Button>
          </AccordionPanel>
        </AccordionItem>
      </Accordion>
    </section>
  );
}

function Hardware({ controller, state }: { controller: SettingsController; state: SettingsSnapshot }) {
  const guide = state.hardware;
  return (
    <section className="li-settings-section" aria-label="Hardware guide">
      <p>
        {guide?.devices?.length
          ? guide.devices
              .map(device => `${device.name}${device.vram_gb ? ` · ${device.vram_gb} GB VRAM` : ''}`)
              .join(' / ')
          : state.loading
            ? 'Checking graphics memory…'
            : 'GPU memory could not be detected'}
      </p>
      {!!guide?.system_ram_gb && <p className="li-settings-note">{guide.system_ram_gb} GB system RAM</p>}
      <p className="li-settings-note">Quick Heal and compositing run on the CPU. Local AI models are optional.</p>
      <div className="li-settings-preference">
        <Checkbox
          label="Don’t show again"
          checked={state.hideHardwareGuide}
          disabled={state.loading || state.savingHardwarePreference}
          onChange={(_, data) => void controller.setHideHardwareGuide(data.checked === true)}
        />
        <p className="li-settings-note">Reopen from Help → Hardware guide.</p>
      </div>
      <div className="li-settings-actions">
        <Button size="small" onClick={() => controller.startTask('setup')}>
          Set up AI
        </Button>
        <Menu>
          <MenuTrigger disableButtonEnhancement>
            <Button size="small" appearance="subtle">
              Start editing
              <Icon name="chevron-down" />
            </Button>
          </MenuTrigger>
          <MenuPopover data-react-owned="true">
            <MenuList>
              <MenuItem onClick={() => controller.startTask('retouch')}>Repair a photo</MenuItem>
              <MenuItem onClick={() => controller.startTask('cutout')}>Remove a background</MenuItem>
              <MenuItem onClick={() => controller.startTask('generate')}>Create an image</MenuItem>
            </MenuList>
          </MenuPopover>
        </Menu>
      </div>
      <Accordion collapsible multiple>
        <AccordionItem value="memory">
          <AccordionHeader>Model memory guide</AccordionHeader>
          <AccordionPanel className="li-settings-disclosure">
            <Table size="small" aria-label="Model memory planning">
              <TableHeader>
                <TableRow>
                  <TableHeaderCell>Tool or model</TableHeaderCell>
                  <TableHeaderCell>GPU memory</TableHeaderCell>
                </TableRow>
              </TableHeader>
              <TableBody>
                {guide?.profiles?.map(profile => (
                  <TableRow key={profile.label}>
                    <TableCell>{profile.label}</TableCell>
                    <TableCell>{profile.vram}</TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
            <p className="li-settings-note">
              {guide?.note ||
                'These are planning recommendations, not hard minimums. Editing tools remain available while hardware detection is unavailable.'}
            </p>
          </AccordionPanel>
        </AccordionItem>
        <AccordionItem value="sources">
          <AccordionHeader>Model notes and sources</AccordionHeader>
          <AccordionPanel className="li-settings-disclosure">
            {guide?.profiles?.map(profile => (
              <p key={profile.label} className="li-settings-note">
                <strong>{profile.label}</strong> — {profile.detail || profile.basis || ''}
                {profile.source_url?.startsWith('https://') && (
                  <>
                    {' '}
                    <Link href={profile.source_url} target="_blank" rel="noopener noreferrer">
                      Source
                    </Link>
                  </>
                )}
              </p>
            ))}
          </AccordionPanel>
        </AccordionItem>
      </Accordion>
    </section>
  );
}

function Shortcuts() {
  const shortcuts = [
    ['Quick Heal / brush', 'J / B'],
    ['Pen / rectangle / ellipse', 'P / R / E'],
    ['Move layer / hand', 'V / H'],
    ['Pan / fit / actual size', 'Space + drag / F / 1'],
    ['Brush size / finish pen path', '[ ] / Enter'],
    ['Original comparison', '\\'],
    ['Previous / next image', 'Alt + ← / →'],
    ['Open files / folder', 'Ctrl + O / Ctrl + Shift + O'],
    ['Save / save a copy', 'Ctrl + S / Ctrl + Shift + S'],
    ['Open / save editable project', 'Ctrl + Alt + O / Ctrl + Alt + S'],
    ['Undo / redo', 'Ctrl + Z / Ctrl + Shift + Z'],
  ];
  return (
    <Table size="small" aria-label="Keyboard shortcuts">
      <TableBody>
        {shortcuts.map(([label, keys]) => (
          <TableRow key={label}>
            <TableCell>{label}</TableCell>
            <TableCell>{keys}</TableCell>
          </TableRow>
        ))}
      </TableBody>
    </Table>
  );
}

function publishedDate(value: string) {
  const date = new Date(value);
  return Number.isNaN(date.getTime()) ? '' : date.toLocaleDateString(undefined, { dateStyle: 'medium' });
}
/** Check, download and install application updates from GitHub releases. The
 * backend verifies the installer checksum; only the desktop host runs it. */
function UpdatesSection({ state, controller }: { state: SettingsSnapshot; controller: SettingsController }) {
  const update = state.update,
    release = update?.release,
    download = update?.download,
    step = state.updateStep;
  const busy = !!step;
  const progress =
    download && download.status === 'downloading' && download.total ? download.received / download.total : undefined;
  const windows = update?.package === 'windows',
    packageName = windows ? 'installer' : update?.package === 'linux-tar' ? 'archive' : 'package';
  return (
    <section className="li-settings-group" aria-labelledby="react-settings-updates">
      <div className="li-settings-toolbar">
        <div>
          <h3 id="react-settings-updates">Updates</h3>
          <p className="li-settings-note">
            Local Image {update?.current_version ?? ''}
            {update?.available && release
              ? ` · Version ${release.version} is available`
              : update?.checked_at
                ? ' · You have the latest version'
                : ''}
          </p>
        </div>
        <Button size="small" disabled={busy} onClick={() => void controller.checkForUpdates()}>
          {step === 'check' ? 'Checking…' : 'Check for updates'}
        </Button>
      </div>
      {state.updateError && <p role="alert">{state.updateError}</p>}
      {!state.updateError && !!update?.check_error && <p role="alert">{update.check_error}</p>}
      {update?.available && release && download && (
        <>
          <p className="li-settings-note">
            {release.name}
            {release.published_at ? ` · ${publishedDate(release.published_at)}` : ''}
            {release.prerelease ? ' · Preview' : ''} ·{' '}
            <Link href={release.html_url} target="_blank" rel="noopener noreferrer">
              Release page
            </Link>
          </p>
          {!!release.notes.trim() && (
            <Accordion collapsible>
              <AccordionItem value="notes">
                <AccordionHeader size="small">What's new</AccordionHeader>
                <AccordionPanel>
                  <pre className="li-settings-release-notes">{release.notes}</pre>
                </AccordionPanel>
              </AccordionItem>
            </Accordion>
          )}
          {download.status === 'downloading' ? (
            <div className="li-settings-progress">
              <ProgressBar value={progress} aria-label="Update download progress" />
              <span>
                {setupBytes(download.received)}
                {download.total ? ` of ${setupBytes(download.total)}` : ''}
              </span>
            </div>
          ) : download.status === 'ready' && update.installer_ready ? (
            state.capabilities.setup ? (
              <div className="li-settings-actions">
                <Button
                  size="small"
                  appearance="primary"
                  disabled={busy}
                  onClick={() => void controller.installUpdate()}
                >
                  {step === 'install'
                    ? 'Closing to install…'
                    : windows
                      ? 'Install and restart'
                      : update.package === 'linux-tar'
                        ? 'Close and open archive'
                        : 'Close and open package'}
                </Button>
                <span className="li-settings-note">
                  {windows
                    ? 'Downloaded and verified. Unsaved edits are reviewed before closing.'
                    : update.package === 'linux-tar'
                      ? 'Downloaded and verified. After Local Image closes, extract the archive and run its install.sh.'
                      : 'Downloaded and verified. After Local Image closes, your package installer opens it.'}
                </span>
              </div>
            ) : (
              <p className="li-settings-note">
                The {packageName} is downloaded and verified. Install it from the Local Image desktop app, or download
                it from the release page.
              </p>
            )
          ) : (
            <div className="li-settings-actions">
              <Button size="small" disabled={busy} onClick={() => void controller.downloadUpdate()}>
                {step === 'download' ? 'Starting download…' : `Download update (${setupBytes(release.bytes)})`}
              </Button>
              {download.status === 'failed' && !!download.error && <span role="alert">{download.error}</span>}
            </div>
          )}
        </>
      )}
    </section>
  );
}
