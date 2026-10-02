import { useEffect, useRef, useState, useSyncExternalStore } from 'react';
import { Button, Checkbox, Dialog, DialogActions, DialogBody, DialogContent, DialogSurface, DialogTitle, Field, Link, Menu, MenuItemRadio, MenuList, MenuPopover, MenuTrigger, MessageBar, MessageBarBody, ProgressBar, Table, TableBody, TableCell, TableHeader, TableHeaderCell, TableRow } from '@fluentui/react-components';
import type { BatchController } from './controller.ts';
import type { BatchBackground, BatchItemStatus } from './contracts.ts';
import './batch.css';

const itemStatus: Record<BatchItemStatus, string> = {pending: 'Waiting', preparing: 'Preparing…', ready: 'Ready for review', exporting: 'Exporting…', exported: 'Exported', failed: 'Needs attention', conflict: 'Edits changed', 'needs-cutout': 'Cutout needed'};
const bytes = (value: number) => value < 1024 ** 2 ? `${Math.round(value / 1024)} KB` : value < 1024 ** 3 ? `${(value / 1024 ** 2).toFixed(1)} MB` : `${(value / 1024 ** 3).toFixed(2)} GB`;

function ChoiceMenu({id, label, value, choices, disabled, onSelect}: {
  id: string; label: string; value: string; choices: {value: string; label: string; disabled?: boolean}[];
  disabled?: boolean; onSelect(value: string): void;
}) {
  const selectedLabel = choices.find(choice => choice.value === value)?.label || 'No previous batches';
  return <Menu><MenuTrigger disableButtonEnhancement>
    <Button id={id} className="li-batch-choice" aria-label={`${label}: ${selectedLabel}`} disabled={disabled || !choices.length || choices.every(choice => choice.disabled)}>
      <span>{selectedLabel}</span><span aria-hidden="true">▾</span>
    </Button>
  </MenuTrigger><MenuPopover data-react-owned="true"><MenuList aria-label={label} checkedValues={{[id]: [value]}}>
    {choices.map(choice => <MenuItemRadio key={choice.value} name={id} value={choice.value} disabled={choice.disabled} onClick={() => onSelect(choice.value)}>{choice.label}</MenuItemRadio>)}
  </MenuList></MenuPopover></Menu>;
}

/** A React-owned dialog. Commands call the typed controller without retaining
 * an invisible copy of the former editor controls. */
export function BatchDialog({controller}: {controller: BatchController}) {
  const state = useSyncExternalStore(controller.subscribe, controller.getSnapshot);
  const active = state.active, locked = state.working || !!active?.running;
  const draft = active ? {treatmentId: active.treatment_id || '', format: active.format, prepareCutouts: active.prepare_cutouts, qwenVariant: active.qwen_variant, backgroundMode: active.background_mode ?? 'transparent', backgroundName: active.background_name ?? ''} : state.draft;
  const selection = new Set(state.selectedIds);
  const rows = active ? active.items.map(item => ({id: item.id, name: item.name, sessionId: item.session_id, status: itemStatus[item.status], error: item.error, outputName: item.output_name, preview: !!item.preview}))
    : state.editor.entries.map(entry => ({...entry, status: 'Ready to remove background', error: null, outputName: null, preview: false}));
  const selectedReady = active?.items.filter(item => item.status === 'ready' && selection.has(item.id)).length || 0;
  const completed = active?.items.filter(item => active.mode === 'prepare' ? !['pending', 'preparing'].includes(item.status) : ['exported', 'failed', 'conflict', 'needs-cutout'].includes(item.status)).length || 0;
  const inspected = active?.items.find(item => item.id === state.inspection?.itemId);
  const [naturalWidth, setNaturalWidth] = useState(0);
  const backgroundInput = useRef<HTMLInputElement>(null);
  useEffect(() => setNaturalWidth(0), [inspected?.id]);
  const inspectionSource = active && inspected && state.inspection ? controller.api.previewUrl(active.id, inspected.id, true, state.inspection.original) : undefined;
  const pendingNames = state.pending.map(item => item.name).join(', ');
  return <>
    <Dialog open={state.open} onOpenChange={(_, data) => {if (!data.open) controller.close();}}>
      <DialogSurface id="batch-dialog" className="li-batch-surface" data-react-owned="true" aria-label="Remove backgrounds and export PNGs">
        <DialogBody className="li-batch-body">
          <DialogTitle action={<Button appearance="subtle" aria-label="Close batch window" id="batch-close" onClick={controller.close}>×</Button>}>Remove backgrounds</DialogTitle>
          <DialogContent className="li-batch-content">
            <p className="li-batch-note">1. Select images. 2. Remove backgrounds and review. 3. Export PNGs.</p>
            <p className="li-batch-note">Existing cutouts are reused. Images without a cutout use Qwen. Each image keeps its own subject.</p>
            <Field label="Output background"><ChoiceMenu id="batch-background" label="Output background" value={draft.backgroundMode} disabled={locked || !!active} choices={[{value:'transparent',label:'Transparent PNG'},{value:'white',label:'White background'},{value:'image',label:'Background image…'}]} onSelect={value => controller.setDraft({backgroundMode: value as BatchBackground})}/></Field>
            {draft.backgroundMode === 'image' && <Field label="Background file">
              <Button id="batch-choose-background" disabled={locked || !!active} onClick={() => backgroundInput.current?.click()}>Choose background image…</Button>
              <input ref={backgroundInput} id="batch-background-file" type="file" hidden accept="image/*" disabled={locked || !!active} onChange={event => {const file = event.currentTarget.files?.[0]; event.currentTarget.value=''; if (file) void controller.chooseBackground(file);}}/>
              <span>{draft.backgroundName || 'Choose an image to use behind every subject.'}</span><span className="li-batch-note">The background fills each canvas and may be cropped to fit.</span>
            </Field>}
            {active && <p className="li-batch-note">Background settings are fixed for these previews and exports. Choose more images to start another batch.</p>}
            {state.pending.length > 0 && <MessageBar intent="warning" id="batch-pending-warning"><MessageBarBody>
              <div id="batch-pending-description" title={pendingNames}>{state.pending.length} selected {state.pending.length === 1 ? 'photo has' : 'photos have'} an unapplied selection or unfinished path: {pendingNames}. Batch uses applied pixels only.</div>
              <div className="li-batch-warning-actions"><Checkbox id="batch-applied-only" checked={state.appliedOnly} label="Use applied pixels only; keep pending selections" onChange={(_, data) => controller.acknowledgeAppliedOnly(data.checked === true)} /><Button id="batch-return-apply" onClick={() => void controller.returnToSelection()}>Return to apply selection</Button></div>
            </MessageBarBody></MessageBar>}
            <div className="li-batch-selection">
              <Checkbox id="batch-all" checked={rows.length > 0 && rows.every(item => selection.has(item.id)) ? true : state.selectedIds.length ? 'mixed' : false} disabled={locked || rows.length === 0} label="Select all" onChange={(_, data) => controller.selectAll(data.checked === true)} />
              <span id="batch-selection-count">{state.selectedIds.length} selected · up to 100 images</span>
              <ChoiceMenu id="batch-qwen-variant" label="Background removal model" value={draft.qwenVariant} disabled={locked || !!active} choices={(['int8','bf16'] as const).map(variant => ({value:variant,label:`Qwen ${variant.toUpperCase()}`,disabled:!state.qwen.variants.some(item => item.id === variant && item.available)}))} onSelect={value => controller.setDraft({qwenVariant: value as 'int8' | 'bf16'})}/>
              {!state.qwenAvailable && <span className="li-batch-note">Missing cutouts need Qwen setup in Settings. Existing cutouts can be exported without it.</span>}
            </div>
            {inspected && state.inspection ? <section id="batch-inspector" className="li-batch-inspection" aria-label="Full-size batch review">
              <div className="li-batch-inspect-toolbar"><Button id="batch-inspect-back" onClick={controller.closeInspection}>Back to images</Button><strong id="batch-inspect-name">{inspected.name}</strong>
                <Button id="batch-inspect-original" aria-pressed={state.inspection.original} onClick={() => controller.setInspection({original: !state.inspection!.original})}>{state.inspection.original ? 'Cutout' : 'Original'}</Button>
                <Field label="Zoom" orientation="horizontal"><ChoiceMenu id="batch-inspect-zoom" label="Batch review zoom" value={state.inspection.zoom} choices={[{value:'fit',label:'Fit'},{value:'100',label:'100%'},{value:'200',label:'200%'}]} onSelect={value => controller.setInspection({zoom: value as 'fit' | '100' | '200'})}/></Field></div>
              <div id="batch-inspect-scroll" className="li-batch-inspect-scroll"><img id="batch-inspect-image" className={state.inspection.zoom === 'fit' ? 'li-batch-fit' : undefined} style={state.inspection.zoom !== 'fit' && naturalWidth ? {width: naturalWidth * Number(state.inspection.zoom) / 100} : undefined} src={inspectionSource} alt={`Full-size ${state.inspection.original ? 'original' : 'cutout'} preview of ${inspected.name}`} onLoad={event => setNaturalWidth(event.currentTarget.naturalWidth)} /></div>
            </section> : <div className="li-batch-table-scroll" id="batch-table-scroll">
              <Table size="small" aria-label="Batch images" className="li-batch-table"><TableHeader><TableRow><TableHeaderCell>Select</TableHeaderCell><TableHeaderCell>Photo</TableHeaderCell><TableHeaderCell>Preview</TableHeaderCell><TableHeaderCell>Status</TableHeaderCell></TableRow></TableHeader>
                <TableBody id="batch-rows">{rows.map(entry => <TableRow key={entry.id}>
                  <TableCell><Checkbox checked={selection.has(entry.id)} disabled={locked} aria-label={'Select ' + entry.name} onChange={(_, data) => controller.setSelected(entry.id, data.checked === true)} /></TableCell>
                  <TableCell>{entry.name}</TableCell>
                  <TableCell>{entry.preview && active ? <Button appearance="subtle" className="li-batch-preview" aria-label={`Inspect ${entry.name} at full size`} onClick={() => controller.inspect(entry.id)}><img loading="lazy" alt={'Batch preview of ' + entry.name} src={controller.api.previewUrl(active.id, entry.id)} /></Button> : '—'}</TableCell>
                  <TableCell className={entry.error ? 'li-batch-error' : undefined}>{entry.status}{entry.error ? ' · ' + entry.error : ''}{entry.outputName ? ' · ' + entry.outputName : ''}{state.editor.pendingSelections.some(item => item.sessionId === entry.sessionId) ? ' · Selection pending' : ''}</TableCell>
                </TableRow>)}</TableBody></Table>
              {!rows.length && <p id="batch-empty" className="li-batch-note">Open images or a folder in Cutout to remove their backgrounds.</p>}
            </div>}
            <div className="li-batch-status"><p id="batch-status" role={state.error ? 'alert' : 'status'} className={state.error ? 'li-batch-error' : undefined}>{state.status}</p>
              {active?.running && <ProgressBar id="batch-progress" value={active.items.length ? completed / active.items.length : 0} aria-label={`${completed} of ${active.items.length} images processed`} />}
            </div>
            <details className="li-batch-history"><summary>Previous batches</summary><div className="li-batch-history-controls">
              <ChoiceMenu id="batch-history-select" label="Previous batch" value={state.historyId} disabled={locked} choices={state.queues.map(queue => ({value:queue.id,label:`${queue.name} · ${new Date(queue.created * 1000).toLocaleString()} · ${queue.phase}`}))} onSelect={controller.setHistory}/>
              <Button id="batch-load-queue" disabled={locked || !state.historyId} onClick={() => void controller.loadQueue()}>Review batch</Button>
              <Button id="batch-clear-queue" disabled={locked || !state.historyId} onClick={controller.confirmQueueClear}>Clear batch</Button>
              <span id="batch-cache-size" className="li-batch-note">Cache {bytes(state.cacheBytes)}</span>
            </div></details>
          </DialogContent>
          <DialogActions className="li-batch-actions">
            <span className="li-batch-note">Exports use new filenames. Originals remain intact.</span>
            {active?.running && <Button id="batch-cancel" disabled={state.working} onClick={() => void controller.cancel()}>Cancel batch</Button>}
            {active?.phase === 'paused' && !active.running && <Button id="batch-resume" disabled={state.working} onClick={() => void controller.resume()}>Resume</Button>}
            {active?.download && !active.running && <Link id="batch-download" href={controller.api.downloadUrl(active.id)} download="Local Image batch.zip">Download ZIP</Link>}
            {active && <Button id="batch-new-queue" disabled={locked} onClick={controller.newQueue}>Choose more images…</Button>}
            <Button id="batch-export" disabled={!state.canExport} onClick={() => void controller.exportReviewed()}>{state.editor.nativeExportAvailable ? 'Export reviewed to folder…' : 'Export reviewed as ZIP'}{selectedReady ? ` (${selectedReady})` : ''}</Button>
            {!active && <Button id="batch-create" appearance="primary" disabled={!state.canPrepare} onClick={() => void controller.prepare()}>Remove backgrounds</Button>}
          </DialogActions>
        </DialogBody>
      </DialogSurface>
    </Dialog>
    <Dialog open={!!state.confirmation} modalType="alert" onOpenChange={(_, data) => {if (!data.open) controller.dismissConfirmation();}}>
      <DialogSurface data-react-owned="true"><DialogBody><DialogTitle>{state.confirmation?.kind === 'queue' ? 'Clear batch?' : 'Remove treatment?'}</DialogTitle>
        <DialogContent>{state.confirmation?.name}. {state.confirmation?.kind === 'queue' ? 'This removes cached previews and export copies. Original files, editor documents, saved projects and exports in your chosen folder remain.' : 'Existing queues and image documents keep their copies.'}</DialogContent>
        <DialogActions><Button id="batch-confirm-no" onClick={controller.dismissConfirmation}>Cancel</Button><Button id="batch-confirm-yes" appearance="primary" onClick={() => void controller.confirmRemoval()}>{state.confirmation?.kind === 'queue' ? 'Clear cache' : 'Remove'}</Button></DialogActions>
      </DialogBody></DialogSurface>
    </Dialog>
  </>;
}
