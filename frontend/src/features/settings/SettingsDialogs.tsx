import { useEffect, useLayoutEffect, useRef, useState, useSyncExternalStore } from 'react';
import {
  Accordion, AccordionHeader, AccordionItem, AccordionPanel, Button, Checkbox,
  Dialog, DialogActions, DialogBody, DialogContent, DialogSurface, DialogTitle,
  Field, Link, ProgressBar, Select, Spinner, Tab, TabList,
  Table, TableBody, TableCell, TableHeader, TableHeaderCell, TableRow,
} from '@fluentui/react-components';
import type { SettingsController } from './settingsController.ts';
import type { InterfaceDensity, SettingsSnapshot, SetupState } from './types.ts';
import './settings.css';

export function setupBytes(value: number | undefined) {
  const amount = Number(value) || 0;
  if (amount >= 1073741824) return `${(amount / 1073741824).toFixed(1)} GB`;
  if (amount >= 1048576) return `${(amount / 1048576).toFixed(0)} MB`;
  if (amount >= 1024) return `${(amount / 1024).toFixed(0)} KB`;
  return `${amount} B`;
}
function storageText(value: { free_bytes?: number; error?: string } | undefined) {
  return value?.error ? `Folder unavailable: ${value.error}` : Number.isFinite(value?.free_bytes) ? `${setupBytes(value?.free_bytes)} available on this drive.` : '';
}
function setupSummary(setup: SetupState | null) {
  if (!setup) return 'Checking local AI setup…';
  if (setup.job?.status === 'error') return setup.job.error || setup.job.message || 'Setup could not finish.';
  if (setup.job?.status === 'running') return setup.job.message || 'Local AI setup is running.';
  if (setup.service?.starting) return 'Starting the AI backend…';
  if (setup.service?.ready) return 'Local AI is ready.';
  if (setup.service?.running) return setup.service.reason || 'ComfyUI is running, but no supported AI model is ready.';
  if (!setup.installation) return 'Choose an existing ComfyUI installation or install a dedicated copy.';
  if (!setup.model_directory) return 'Choose a folder for model files.';
  return 'Start the local AI backend when you need a model.';
}

export function SettingsDialogs({ controller }: { controller: SettingsController }) {
  const snapshot = useSyncExternalStore(controller.subscribe, controller.getSnapshot);
  const lastOpen = useRef(snapshot), open = snapshot.view !== null;
  useLayoutEffect(() => { if (open) lastOpen.current = snapshot; }, [open, snapshot]);
  const state = open ? snapshot : lastOpen.current;
  const [tab, setTab] = useState('general');
  useEffect(() => { if (snapshot.view === 'settings') setTab(snapshot.requestedSection); }, [snapshot.view, snapshot.requestedSection]);
  const title = state.view === 'hardware' ? 'Hardware guide' : state.view === 'shortcuts' ? 'Keyboard shortcuts' : 'Settings';
  return <Dialog open={open} onOpenChange={(_, data) => { if (!data.open) controller.close(); }}>
    <DialogSurface className="li-settings-surface" data-react-owned="true" aria-label={title}>
      <DialogBody className="li-settings-body">
        <DialogTitle action={<Button appearance="subtle" aria-label={`Close ${title.toLowerCase()}`} onClick={() => controller.close()}>×</Button>}>{title}</DialogTitle>
        <DialogContent className="li-settings-content">
          {state.view === 'settings' && <>
            <TabList size="small" selectedValue={tab} onTabSelect={(_, data) => setTab(String(data.value))} aria-label="Settings sections">
              <Tab value="general">General</Tab><Tab value="ai">Local AI</Tab>
            </TabList>
            {tab === 'general' ? <section className="li-settings-section" aria-label="General settings">
              <Checkbox label="Ask before overwriting original images" checked={state.preferences.askBeforeOverwrite} onChange={(_, data) => controller.setAskBeforeOverwrite(data.checked === true)} />
              <Field label="Interface size"><Select aria-label="Interface size" value={state.preferences.density} onChange={event => controller.setDensity(event.target.value as InterfaceDensity)}>
                <option value="compact">Compact</option><option value="comfortable">Comfortable</option><option value="large">Large · 200% text</option>
              </Select></Field>
              <div className="li-settings-actions"><Button size="small" onClick={() => void controller.open('hardware')}>Hardware guide</Button><Button size="small" onClick={() => void controller.open('shortcuts')}>Keyboard shortcuts</Button></div>
              <Accordion collapsible><AccordionItem value="quick-heal"><AccordionHeader>About Quick Heal</AccordionHeader><AccordionPanel>
                <p>Texture repair copies nearby texture to cover small objects. Dust &amp; scratches smooths tiny defects. Both run locally on the CPU without model downloads.</p>
                <p><Link href="https://github.com/EmbarkStudios/texture-synthesis" target="_blank" rel="noopener noreferrer">Texture synthesis</Link> · <Link href="https://docs.opencv.org/4.x/df/d3d/tutorial_py_inpainting.html" target="_blank" rel="noopener noreferrer">OpenCV inpainting</Link></p>
              </AccordionPanel></AccordionItem></Accordion>
            </section> : <LocalAi controller={controller} state={state} />}
          </>}
          {state.view === 'hardware' && <Hardware controller={controller} state={state} />}
          {state.view === 'shortcuts' && <Shortcuts />}
          {state.loading && <Spinner size="tiny" label={state.view === 'hardware' ? 'Reading hardware information' : 'Refreshing local setup'} />}
          {state.pendingAction && <p role="status" className="li-settings-note">{['chooseRuntime', 'chooseInstallDirectory', 'chooseModelDirectory', 'configureConnection'].includes(state.pendingAction) ? 'Complete or cancel the desktop dialog to continue.' : 'Waiting for the desktop command…'}</p>}
          {state.error && <p role="alert" className="li-settings-error">{state.error}</p>}
          {state.message && <p role="status" className="li-settings-note">{state.message}</p>}
        </DialogContent>
        <DialogActions>
          {state.view === 'hardware' ? <Button appearance="primary" onClick={() => controller.continueHardware()}>Continue</Button> : <Button appearance="primary" onClick={() => controller.close()}>Done</Button>}
        </DialogActions>
      </DialogBody>
    </DialogSurface>
  </Dialog>;
}

function LocalAi({ controller, state }: { controller: SettingsController; state: SettingsSnapshot }) {
  const setup = state.setup, service = setup?.service, files = setup?.models ?? [], job = setup?.job;
  const locked = controller.locked() || state.loading;
  const nativeLocked = locked || !state.capabilities.setup;
  const allFiles = files.length > 0 && files.every(file => file.exists);
  const progress = job?.progress != null && Number.isFinite(job.progress) ? Math.min(100, Math.max(0, job.progress)) / 100 : undefined;
  const installPath = setup?.install_directory || setup?.managed_directory || setup?.configured_ai_directory;
  return <section className="li-settings-section" aria-label="Local AI setup">
    <div className="li-settings-toolbar"><p role="status" className="li-settings-note">{setupSummary(setup)}</p><Button size="small" disabled={state.loading || !!state.pendingAction} onClick={() => void controller.refresh()}>Refresh</Button></div>
    {!state.capabilities.setup && <p className="li-settings-note">{state.capabilities.ready ? 'Update the desktop host to use guided AI setup.' : 'Use the Local Image desktop app to choose local folders, install ComfyUI or download model files.'}</p>}
    <section className="li-settings-group" aria-labelledby="react-settings-runtime">
      <h3 id="react-settings-runtime">ComfyUI</h3>
      <output className="li-settings-path">{setup?.installation?.path || 'No installation selected'}</output>
      <div className="li-settings-actions"><Button size="small" disabled={locked} onClick={() => void controller.refresh(true)}>Detect installations</Button><Button size="small" disabled={nativeLocked} onClick={() => void controller.run('chooseRuntime')}>Choose installation…</Button><Button size="small" disabled={nativeLocked || !service?.can_start || !!service?.running} onClick={() => void controller.run('startBackend')}>{service?.starting ? 'Starting…' : service?.running ? 'Backend running' : 'Start AI backend'}</Button></div>
      {state.showInstallations && !!setup?.installations?.length && <div className="li-settings-installation-choice"><Field label="Detected installation"><Select aria-label="Detected installation" value={state.selectedInstallation} disabled={locked} onChange={event => controller.selectInstallation(event.target.value)}>{setup.installations.map(item => <option key={item.id || item.path} value={item.id}>{item.name || 'ComfyUI'} · {item.path}</option>)}</Select></Field><Button size="small" disabled={nativeLocked || !state.selectedInstallation} onClick={() => void controller.run('useInstallation')}>Use selected</Button></div>}
      {service?.running && !service.ready && <p className="li-settings-note">{service.reason || 'No supported model is ready.'} If model folders changed, close ComfyUI, then start the backend again and refresh.</p>}
      {service?.running && service.ready && <p className="li-settings-note">Connected to ComfyUI on this PC{service.port ? `, port ${service.port}` : ''}.</p>}
      {setup?.installation && !setup.installation.startable && !service?.running && <p className="li-settings-note">Start this installation from ComfyUI, or use a dedicated portable copy.</p>}
      <Accordion collapsible><AccordionItem value="portable"><AccordionHeader>Portable runtime</AccordionHeader><AccordionPanel className="li-settings-disclosure">
        <output className="li-settings-path">{installPath || 'Choose a writable folder for a dedicated runtime'}</output>
        <p className="li-settings-note">{setup?.portable?.download_bytes ? `Runtime download: ${setupBytes(setup.portable.download_bytes)}. ` : ''}{setup?.portable?.minimum_free_bytes ? `Allow ${setupBytes(setup.portable.minimum_free_bytes)} for installation. ` : ''}{storageText(setup?.storage?.portable_folder)}</p>
        <div className="li-settings-actions"><Button size="small" disabled={nativeLocked} onClick={() => void controller.run('chooseInstallDirectory')}>Choose install folder…</Button><Button size="small" disabled={nativeLocked} onClick={() => void controller.run('installRuntime')}>Install portable ComfyUI</Button></div>
      </AccordionPanel></AccordionItem></Accordion>
    </section>
    <section className="li-settings-group" aria-labelledby="react-settings-models">
      <h3 id="react-settings-models">Model files</h3><output className="li-settings-path">{setup?.model_directory || 'No model folder selected'}</output>
      {storageText(setup?.storage?.model_folder) && <p className="li-settings-note">{storageText(setup?.storage?.model_folder)}</p>}
      {setup?.model_folder_connection?.status !== 'unchanged' && setup?.model_folder_connection?.message && <p className="li-settings-note">{setup.model_folder_connection.message}</p>}
      <div className="li-settings-actions"><Button size="small" disabled={nativeLocked} onClick={() => void controller.run('chooseModelDirectory')}>Choose model folder…</Button><Button size="small" disabled={locked} onClick={() => controller.browseModels()}>Browse models…</Button></div>
      <Accordion collapsible><AccordionItem value="removal-model"><AccordionHeader>FLUX AI Remove · optional</AccordionHeader><AccordionPanel className="li-settings-disclosure">
        <p className="li-settings-note">This set supports FLUX object removal. Compare generation and cutout models in Browse models.</p>
        <ul className="li-settings-file-list" aria-label="Required FLUX model files">{files.map(file => <li key={`${file.folder}/${file.name}`}><span title={`${file.folder ? `${file.folder}/` : ''}${file.name}`}>{file.label || file.name}</span><span>{file.exists ? 'Ready' : 'Required'}{(file.exists ? file.bytes : file.expected_bytes) ? ` · ${setupBytes(file.exists ? file.bytes : file.expected_bytes)}` : ''}</span></li>)}</ul>
        <Button size="small" disabled={nativeLocked || !setup?.model_directory || allFiles} onClick={() => void controller.run('downloadRemovalModels')}>{allFiles ? 'FLUX models ready' : 'Download FLUX models'}</Button>
      </AccordionPanel></AccordionItem></Accordion>
    </section>
    {job && <section className="li-settings-job" aria-label="Setup progress"><strong>{job.status === 'complete' ? 'Setup complete' : job.status === 'error' ? 'Setup needs attention' : job.phase || 'Setting up local AI'}</strong>{job.status === 'running' && <ProgressBar value={progress} aria-label="Setup progress" />}<p role={job.status === 'error' ? 'alert' : 'status'}>{job.error || job.message || ''}</p>{progress !== undefined && <span>{Math.round(progress * 100)}%</span>}{!!job.downloaded_bytes && <span>{setupBytes(job.downloaded_bytes)}{job.total_bytes ? ` of ${setupBytes(job.total_bytes)}` : ' downloaded'}</span>}</section>}
    <section className="li-settings-group"><div className="li-settings-toolbar"><div><h3>GPU memory</h3><p className="li-settings-note">{service?.device || 'Start the backend to check your device.'}</p></div><Button size="small" disabled={nativeLocked || !service?.can_eject} title="Unload GPU models; keep files on disk" onClick={() => void controller.run('ejectModels')}>Unload models</Button></div></section>
    <Accordion collapsible><AccordionItem value="connection"><AccordionHeader>Advanced connection</AccordionHeader><AccordionPanel><p className="li-settings-note">Choose a different local ComfyUI port or model location in the desktop connection dialog.</p><Button size="small" disabled={locked || !state.capabilities.ready} onClick={() => void controller.run('configureConnection')}>AI connection…</Button></AccordionPanel></AccordionItem></Accordion>
  </section>;
}

function Hardware({ controller, state }: { controller: SettingsController; state: SettingsSnapshot }) {
  const guide = state.hardware;
  return <section className="li-settings-section" aria-label="Hardware guide">
    <p>{guide?.devices?.length ? guide.devices.map(device => `${device.name}${device.vram_gb ? ` · ${device.vram_gb} GB VRAM` : ''}`).join(' / ') : state.loading ? 'Checking graphics memory…' : 'GPU memory could not be detected'}</p>
    {!!guide?.system_ram_gb && <p className="li-settings-note">{guide.system_ram_gb} GB system RAM</p>}
    <p className="li-settings-note">Quick Heal and compositing run on the CPU. Local AI models are optional.</p>
    <div className="li-settings-actions"><Button size="small" onClick={() => controller.startTask('retouch')}>Repair a photo</Button><Button size="small" onClick={() => controller.startTask('cutout')}>Remove a background</Button><Button size="small" onClick={() => controller.startTask('generate')}>Create an image</Button><Button size="small" onClick={() => controller.startTask('setup')}>Set up AI</Button></div>
    <Table size="small" aria-label="Model memory planning"><TableHeader><TableRow><TableHeaderCell>Tool or model</TableHeaderCell><TableHeaderCell>GPU memory</TableHeaderCell></TableRow></TableHeader><TableBody>{guide?.profiles?.map(profile => <TableRow key={profile.label}><TableCell>{profile.label}</TableCell><TableCell>{profile.vram}</TableCell></TableRow>)}</TableBody></Table>
    <p className="li-settings-note">{guide?.note || 'These are planning recommendations, not hard minimums. Editing tools remain available while hardware detection is unavailable.'}</p>
    <Accordion collapsible><AccordionItem value="sources"><AccordionHeader>Model notes and sources</AccordionHeader><AccordionPanel>{guide?.profiles?.map(profile => <p key={profile.label} className="li-settings-note"><strong>{profile.label}</strong> — {profile.detail || profile.basis || ''}{profile.source_url?.startsWith('https://') && <> <Link href={profile.source_url} target="_blank" rel="noopener noreferrer">Source</Link></>}</p>)}</AccordionPanel></AccordionItem></Accordion>
  </section>;
}

function Shortcuts() {
  const shortcuts = [['Quick Heal / brush', 'J / B'], ['Pen / rectangle / ellipse', 'P / R / E'], ['Move layer / hand', 'V / H'], ['Pan / fit / actual size', 'Space + drag / F / 1'], ['Brush size / finish pen path', '[ ] / Enter'], ['Original comparison', '\\'], ['Previous / next image', 'Alt + ← / →'], ['Open files / folder', 'Ctrl + O / Ctrl + Shift + O'], ['Save / save a copy', 'Ctrl + S / Ctrl + Shift + S'], ['Open / save editable project', 'Ctrl + Alt + O / Ctrl + Alt + S'], ['Undo / redo', 'Ctrl + Z / Ctrl + Shift + Z']];
  return <Table size="small" aria-label="Keyboard shortcuts"><TableBody>{shortcuts.map(([label, keys]) => <TableRow key={label}><TableCell>{label}</TableCell><TableCell>{keys}</TableCell></TableRow>)}</TableBody></Table>;
}
