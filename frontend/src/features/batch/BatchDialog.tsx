import { useEffect, useState, useSyncExternalStore } from 'react';
import { Button, Checkbox, Dialog, DialogActions, DialogBody, DialogContent, DialogSurface, DialogTitle, Field, Input, Link, MessageBar, MessageBarBody, ProgressBar, Select, Table, TableBody, TableCell, TableHeader, TableHeaderCell, TableRow } from '@fluentui/react-components';
import type { BatchController } from './controller.ts';
import type { BatchFormat, BatchItemStatus } from './contracts.ts';
import './batch.css';

const formats: {value: BatchFormat; label: string}[] = [
  {value: 'original', label: 'Original precision & format'}, {value: 'png', label: 'PNG · 8-bit with alpha'},
  {value: 'jpg', label: 'JPEG · 8-bit opaque'}, {value: 'tif', label: 'TIFF · preserve 16-bit'}, {value: 'webp', label: 'WebP · lossless 8-bit'},
];
const itemStatus: Record<BatchItemStatus, string> = {pending: 'Waiting', preparing: 'Preparing…', ready: 'Ready for review', exporting: 'Exporting…', exported: 'Exported', failed: 'Needs attention', conflict: 'Edits changed', 'needs-cutout': 'Cutout needed'};
const bytes = (value: number) => value < 1024 ** 2 ? `${Math.round(value / 1024)} KB` : value < 1024 ** 3 ? `${(value / 1024 ** 2).toFixed(1)} MB` : `${(value / 1024 ** 3).toFixed(2)} GB`;

/** A React-owned dialog. All commands call the typed controller; none dispatch
 * synthetic clicks or retain an invisible copy of the former batch controls. */
export function BatchDialog({controller}: {controller: BatchController}) {
  const state = useSyncExternalStore(controller.subscribe, controller.getSnapshot);
  const active = state.active, locked = state.working || !!active?.running;
  const draft = active ? {treatmentId: active.treatment_id || '', format: active.format, prepareCutouts: active.prepare_cutouts, qwenVariant: active.qwen_variant} : state.draft;
  const selection = new Set(state.selectedIds);
  const rows = active ? active.items.map(item => ({id: item.id, name: item.name, sessionId: item.session_id, status: itemStatus[item.status], error: item.error, outputName: item.output_name, preview: !!item.preview}))
    : state.editor.entries.map(entry => ({...entry, status: 'Current applied edits', error: null, outputName: null, preview: false}));
  const selectedReady = active?.items.filter(item => item.status === 'ready' && selection.has(item.id)).length || 0;
  const completed = active?.items.filter(item => active.mode === 'prepare' ? !['pending', 'preparing'].includes(item.status) : ['exported', 'failed', 'conflict', 'needs-cutout'].includes(item.status)).length || 0;
  const inspected = active?.items.find(item => item.id === state.inspection?.itemId);
  const [naturalWidth, setNaturalWidth] = useState(0);
  useEffect(() => setNaturalWidth(0), [inspected?.id]);
  const inspectionSource = active && inspected && state.inspection ? controller.api.previewUrl(active.id, inspected.id, true, state.inspection.original) : undefined;
  const pendingNames = state.pending.map(item => item.name).join(', ');
  return <>
    <Dialog open={state.open} onOpenChange={(_, data) => {if (!data.open) controller.close();}}>
      <DialogSurface id="batch-dialog" className="li-batch-surface" data-react-owned="true" aria-label="Batch treatment and export">
        <DialogBody className="li-batch-body">
          <DialogTitle action={<Button appearance="subtle" aria-label="Close batch window" id="batch-close" onClick={controller.close}>×</Button>}>Batch treatment & export</DialogTitle>
          <DialogContent className="li-batch-content">
            <div className="li-batch-settings">
              <Field label="Treatment">
                <Select id="batch-treatment" value={draft.treatmentId} disabled={locked || !!active} onChange={(_, data) => controller.setDraft({treatmentId: data.value})}>
                  <option value="">Current edits only</option>
                  {state.treatments.map(item => <option key={item.id} value={item.id}>{item.name}</option>)}
                  {active?.treatment_id && !state.treatments.some(item => item.id === active.treatment_id) && <option value={active.treatment_id}>{active.treatment_name || 'Saved treatment'} · queue copy</option>}
                </Select>
              </Field>
              <Field label="Export format"><Select id="batch-format" value={draft.format} disabled={locked || !!active} onChange={(_, data) => controller.setDraft({format: data.value as BatchFormat})}>{formats.map(format => <option key={format.value} value={format.value}>{format.label}</option>)}</Select></Field>
              <Button id="batch-save-treatment" disabled={!state.canSaveTreatment} onClick={controller.beginTreatment}>Save current treatment…</Button>
              <Button id="batch-delete-treatment" disabled={locked || !!active || !draft.treatmentId} onClick={controller.confirmTreatmentRemoval}>Remove treatment</Button>
            </div>
            {active && <p id="batch-queue-settings" className="li-batch-note">Reviewed queue: {active.treatment_name || 'Current edits only'} · {formats.find(item => item.value === active.format)?.label}. Settings are fixed for these previews and exports.</p>}
            {state.savingTreatment && <div className="li-batch-name" id="batch-name-row">
              <Field label="Treatment name"><Input id="batch-treatment-name" autoFocus value={state.treatmentName} maxLength={80} onChange={(_, data) => controller.setTreatmentName(data.value)} onKeyDown={event => {event.stopPropagation(); if (event.key === 'Enter') {event.preventDefault(); void controller.saveTreatment();}}} /></Field>
              <Button appearance="primary" id="batch-name-save" disabled={!state.canSaveTreatment || !state.treatmentName.trim()} onClick={() => void controller.saveTreatment()}>Save treatment</Button>
              <Button id="batch-name-cancel" onClick={controller.cancelTreatment}>Cancel</Button>
            </div>}
            {state.pending.length > 0 && <MessageBar intent="warning" id="batch-pending-warning"><MessageBarBody>
              <div id="batch-pending-description" title={pendingNames}>{state.pending.length} selected {state.pending.length === 1 ? 'photo has' : 'photos have'} an unapplied selection or unfinished path: {pendingNames}. Batch uses applied pixels only.</div>
              <div className="li-batch-warning-actions"><Checkbox id="batch-applied-only" checked={state.appliedOnly} label="Use applied pixels only; keep pending selections" onChange={(_, data) => controller.acknowledgeAppliedOnly(data.checked === true)} /><Button id="batch-return-apply" onClick={() => void controller.returnToSelection()}>Return to apply selection</Button></div>
            </MessageBarBody></MessageBar>}
            <div className="li-batch-selection">
              <Checkbox id="batch-all" checked={rows.length > 0 && rows.every(item => selection.has(item.id)) ? true : state.selectedIds.length ? 'mixed' : false} disabled={locked || rows.length === 0} label="Select all" onChange={(_, data) => controller.selectAll(data.checked === true)} />
              <span id="batch-selection-count">{state.selectedIds.length} selected · up to 100 per queue</span>
              {draft.treatmentId && <><Checkbox id="batch-prepare-cutouts" checked={draft.prepareCutouts} disabled={locked || !!active || !state.qwenAvailable} label={'Prepare missing cutouts with Qwen' + (state.qwenAvailable ? '' : ' · AI setup needed')} onChange={(_, data) => controller.setDraft({prepareCutouts: data.checked === true})} />
                {draft.prepareCutouts && <Select id="batch-qwen-variant" aria-label="Qwen cutout variant" value={draft.qwenVariant} disabled={locked || !!active} onChange={(_, data) => controller.setDraft({qwenVariant: data.value as 'int8' | 'bf16'})}>{(['int8', 'bf16'] as const).map(variant => <option key={variant} value={variant} disabled={!state.qwen.variants.some(item => item.id === variant && item.available)}>Qwen {variant.toUpperCase()}</option>)}</Select>}</>}
            </div>
            {inspected && state.inspection ? <section id="batch-inspector" className="li-batch-inspection" aria-label="Full-size batch review">
              <div className="li-batch-inspect-toolbar"><Button id="batch-inspect-back" onClick={controller.closeInspection}>Back to images</Button><strong id="batch-inspect-name">{inspected.name}</strong>
                <Button id="batch-inspect-original" aria-pressed={state.inspection.original} onClick={() => controller.setInspection({original: !state.inspection!.original})}>{state.inspection.original ? 'Treatment' : 'Original'}</Button>
                <Field label="Zoom" orientation="horizontal"><Select id="batch-inspect-zoom" value={state.inspection.zoom} onChange={(_, data) => controller.setInspection({zoom: data.value as 'fit' | '100' | '200'})}><option value="fit">Fit</option><option value="100">100%</option><option value="200">200%</option></Select></Field></div>
              <div id="batch-inspect-scroll" className="li-batch-inspect-scroll"><img id="batch-inspect-image" className={state.inspection.zoom === 'fit' ? 'li-batch-fit' : undefined} style={state.inspection.zoom !== 'fit' && naturalWidth ? {width: naturalWidth * Number(state.inspection.zoom) / 100} : undefined} src={inspectionSource} alt={`Full-size ${state.inspection.original ? 'original' : 'treatment'} preview of ${inspected.name}`} onLoad={event => setNaturalWidth(event.currentTarget.naturalWidth)} /></div>
            </section> : <div className="li-batch-table-scroll" id="batch-table-scroll">
              <Table size="small" aria-label="Batch images" className="li-batch-table"><TableHeader><TableRow><TableHeaderCell>Select</TableHeaderCell><TableHeaderCell>Photo</TableHeaderCell><TableHeaderCell>Preview</TableHeaderCell><TableHeaderCell>Status</TableHeaderCell></TableRow></TableHeader>
                <TableBody id="batch-rows">{rows.map(entry => <TableRow key={entry.id}>
                  <TableCell><Checkbox checked={selection.has(entry.id)} disabled={locked} aria-label={'Select ' + entry.name} onChange={(_, data) => controller.setSelected(entry.id, data.checked === true)} /></TableCell>
                  <TableCell>{entry.name}</TableCell>
                  <TableCell>{entry.preview && active ? <Button appearance="subtle" className="li-batch-preview" aria-label={`Inspect ${entry.name} at full size`} onClick={() => controller.inspect(entry.id)}><img loading="lazy" alt={'Batch preview of ' + entry.name} src={controller.api.previewUrl(active.id, entry.id)} /></Button> : '—'}</TableCell>
                  <TableCell className={entry.error ? 'li-batch-error' : undefined}>{entry.status}{entry.error ? ' · ' + entry.error : ''}{entry.outputName ? ' · ' + entry.outputName : ''}{state.editor.pendingSelections.some(item => item.sessionId === entry.sessionId) ? ' · Selection pending' : ''}</TableCell>
                </TableRow>)}</TableBody></Table>
              {!rows.length && <p id="batch-empty" className="li-batch-note">Open a folder or images to prepare a batch.</p>}
            </div>}
            <div className="li-batch-status"><p id="batch-status" role={state.error ? 'alert' : 'status'} className={state.error ? 'li-batch-error' : undefined}>{state.status}</p>
              {active?.running && <ProgressBar id="batch-progress" value={active.items.length ? completed / active.items.length : 0} aria-label={`${completed} of ${active.items.length} images processed`} />}
            </div>
            <details className="li-batch-history"><summary>Previous queues & cache</summary><div className="li-batch-history-controls">
              <Select id="batch-history-select" aria-label="Previous batch queue" value={state.historyId} disabled={locked} onChange={(_, data) => controller.setHistory(data.value)}>{!state.queues.length && <option value="">No previous queues</option>}{state.queues.map(queue => <option key={queue.id} value={queue.id}>{queue.name} · {new Date(queue.created * 1000).toLocaleString()} · {queue.phase}</option>)}</Select>
              <Button id="batch-load-queue" disabled={locked || !state.historyId} onClick={() => void controller.loadQueue()}>Review queue</Button>
              <Button id="batch-clear-queue" disabled={locked || !state.historyId} onClick={controller.confirmQueueClear}>Clear queue cache</Button>
              <span id="batch-cache-size" className="li-batch-note">Cache {bytes(state.cacheBytes)} · treatments {bytes(state.treatmentBytes)}</span>
            </div></details>
          </DialogContent>
          <DialogActions className="li-batch-actions">
            <span className="li-batch-note">Exports use new filenames. Originals remain intact.</span>
            {active?.running && <Button id="batch-cancel" disabled={state.working} onClick={() => void controller.cancel()}>Cancel batch</Button>}
            {active?.phase === 'paused' && !active.running && <Button id="batch-resume" disabled={state.working} onClick={() => void controller.resume()}>Resume</Button>}
            {active?.download && !active.running && <Link id="batch-download" href={controller.api.downloadUrl(active.id)} download="Local Image batch.zip">Download ZIP</Link>}
            {active && <Button id="batch-new-queue" disabled={locked} onClick={controller.newQueue}>New queue…</Button>}
            <Button id="batch-export" disabled={!state.canExport} onClick={() => void controller.exportReviewed()}>{state.editor.nativeExportAvailable ? 'Export reviewed to folder…' : 'Export reviewed as ZIP'}{selectedReady ? ` (${selectedReady})` : ''}</Button>
            {!active && <Button id="batch-create" appearance="primary" disabled={!state.canPrepare} onClick={() => void controller.prepare()}>Prepare selected</Button>}
          </DialogActions>
        </DialogBody>
      </DialogSurface>
    </Dialog>
    <Dialog open={!!state.confirmation} modalType="alert" onOpenChange={(_, data) => {if (!data.open) controller.dismissConfirmation();}}>
      <DialogSurface data-react-owned="true"><DialogBody><DialogTitle>{state.confirmation?.kind === 'queue' ? 'Clear queue cache?' : 'Remove treatment?'}</DialogTitle>
        <DialogContent>{state.confirmation?.name}. {state.confirmation?.kind === 'queue' ? 'This removes cached previews and export copies. Original files, editor documents, saved projects and exports in your chosen folder remain.' : 'Existing queues and image documents keep their copies.'}</DialogContent>
        <DialogActions><Button id="batch-confirm-no" onClick={controller.dismissConfirmation}>Cancel</Button><Button id="batch-confirm-yes" appearance="primary" onClick={() => void controller.confirmRemoval()}>{state.confirmation?.kind === 'queue' ? 'Clear cache' : 'Remove'}</Button></DialogActions>
      </DialogBody></DialogSurface>
    </Dialog>
  </>;
}
