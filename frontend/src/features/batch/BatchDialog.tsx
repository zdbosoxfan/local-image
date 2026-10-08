import { BatchViewer } from './BatchViewer.tsx';
import { useEffect, useRef, useState, useSyncExternalStore } from 'react';
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
  Menu,
  MenuItemRadio,
  MenuList,
  MenuPopover,
  MenuTrigger,
  MessageBar,
  MessageBarBody,
  ProgressBar,
  Table,
  TableBody,
  TableCell,
  TableHeader,
  TableHeaderCell,
  TableRow,
} from '@fluentui/react-components';
import { Icon } from '../shell/Icon.tsx';
import type { BatchController } from './controller.ts';
import type { BatchBackground, BatchFormat, BatchItemStatus } from './contracts.ts';
import { DEFAULT_NAMING_TEMPLATE, filenamePreview, namingError } from './naming.ts';
import { batchPage } from './pagination.ts';
import { Hint } from '../shell/Hint.tsx';
import './batch.css';
import { ChoiceSelect } from '../shell/ChoiceSelect.tsx';

const itemStatus: Record<BatchItemStatus, string> = {
  pending: 'Waiting',
  preparing: 'Preparing…',
  ready: 'Ready for review',
  exporting: 'Exporting…',
  exported: 'Exported',
  failed: 'Needs attention',
  conflict: 'Edits changed',
  'needs-cutout': 'Cutout needed',
};
const bytes = (value: number) =>
  value < 1024 ** 2
    ? `${Math.round(value / 1024)} KB`
    : value < 1024 ** 3
      ? `${(value / 1024 ** 2).toFixed(1)} MB`
      : `${(value / 1024 ** 3).toFixed(2)} GB`;

/** A React-owned dialog. Commands call the typed controller without retaining
 * an invisible copy of the former editor controls. */
export function BatchDialog({ controller }: { controller: BatchController }) {
  const state = useSyncExternalStore(controller.subscribe, controller.getSnapshot);
  const active = state.active,
    locked = state.working || !!active?.running;
  const draft = active
    ? {
        treatmentId: active.treatment_id || '',
        format: active.format,
        prepareCutouts: active.prepare_cutouts,
        qwenVariant: active.qwen_variant,
        backgroundMode: active.background_mode ?? 'transparent',
        backgroundName: active.background_name ?? '',
      }
    : state.draft;
  const selection = new Set(state.selectedIds);
  const [requestedPage, setPage] = useState(0);
  const page = batchPage((active?.items || state.editor.entries).length, requestedPage);
  const rows = active
    ? active.items.slice(page.start, page.end).map(item => ({
        id: item.id,
        name: item.name,
        sessionId: item.session_id,
        status: itemStatus[item.status],
        error: item.error,
        outputName: item.output_name,
        preview: !!item.preview,
      }))
    : state.editor.entries.slice(page.start, page.end).map(entry => ({
        ...entry,
        status: entry.cutoutReady ? 'Cutout available' : 'Not processed',
        error: null,
        outputName: null,
        preview: false,
      }));
  const allSelected = page.total > 0 && (active?.items || state.editor.entries).every(item => selection.has(item.id));
  const selectedReady = active?.items.filter(item => item.status === 'ready' && selection.has(item.id)).length || 0;
  const completed =
    active?.items.filter(item =>
      active.mode === 'prepare'
        ? !['pending', 'preparing'].includes(item.status)
        : ['exported', 'failed', 'conflict', 'needs-cutout'].includes(item.status),
    ).length || 0;
  const inspected = active?.items.find(item => item.id === state.inspection?.itemId);
  const inspectionTrigger = useRef<HTMLButtonElement | HTMLAnchorElement | null>(null);
  const inspectionTriggerId = useRef<string | null>(null);
  const backgroundInput = useRef<HTMLInputElement>(null);
  useEffect(() => setPage(0), [state.open, active?.id]);
  useEffect(() => {
    if (state.inspection) return;
    // Fluent restores focus while replacing the dialog content. Restore the
    // thumbnail after that pass so keyboard review continues at the same image.
    const frame = requestAnimationFrame(() => inspectionTrigger.current?.focus({ preventScroll: true }));
    return () => cancelAnimationFrame(frame);
  }, [!!state.inspection]);
  const pendingNames = state.pending.map(item => item.name).join(', ');
  const namingScheme =
    state.output.namingTemplate === DEFAULT_NAMING_TEMPLATE
      ? 'suffix'
      : state.output.namingTemplate === '{name}'
        ? 'original'
        : state.output.namingTemplate === 'Image-{index}'
          ? 'numbered'
          : 'custom';
  const [customNaming, setCustomNaming] = useState(false);
  const patternError = namingError(state.output.namingTemplate);
  const exampleName = (active?.items || state.editor.entries).find(item => selection.has(item.id))?.name || 'Photo.jpg';
  return (
    <>
      <Dialog
        open={state.open}
        onOpenChange={(_, data) => {
          if (!data.open) {
            if (state.inspection) controller.closeInspection();
            else controller.close();
          }
        }}
      >
        <DialogSurface
          id="batch-dialog"
          className={inspected ? 'li-batch-surface li-batch-viewer-surface' : 'li-batch-surface'}
          data-react-owned="true"
          aria-label={inspected ? 'Batch image viewer' : 'Remove backgrounds and export PNGs'}
        >
          {inspected && state.inspection ? (
            <BatchViewer controller={controller} state={state} item={inspected} />
          ) : (
            <DialogBody className="li-batch-body">
              <DialogTitle
                action={
                  <Hint content="Close batch window">
                    <Button
                      appearance="subtle"
                      aria-label="Close batch window"
                      id="batch-close"
                      onClick={controller.close}
                      icon={<Icon name="close" />}
                    />
                  </Hint>
                }
              >
                Remove backgrounds
              </DialogTitle>
              <DialogContent className="li-batch-content">
                <p className="li-batch-note">Existing cutouts are reused.</p>
                <div className="li-batch-settings" aria-label="Batch output settings">
                  <Field label="Output background">
                    <ChoiceSelect
                      id="batch-background"
                      label="Output background"
                      value={draft.backgroundMode}
                      disabled={locked || !!active}
                      choices={[
                        { value: 'transparent', label: 'Transparent PNG' },
                        { value: 'white', label: 'White background' },
                        { value: 'image', label: 'Background image…' },
                      ]}
                      onSelect={value => controller.setDraft({ backgroundMode: value as BatchBackground })}
                    />
                  </Field>
                  <Field label="Removal model">
                    <ChoiceSelect
                      id="batch-qwen-variant"
                      label="Background removal model"
                      value={draft.qwenVariant}
                      disabled={locked || !!active}
                      choices={(['int8', 'bf16'] as const).map(variant => ({
                        value: variant,
                        label: variant === 'int8' ? 'Qwen · Standard' : 'Qwen · High precision',
                        disabled: !state.qwen.variants.some(item => item.id === variant && item.available),
                      }))}
                      onSelect={value => controller.setDraft({ qwenVariant: value as 'int8' | 'bf16' })}
                    />
                    {!active && !state.qwenAvailable && (
                      <span className="li-batch-note">Set up Qwen in Settings for photos without a cutout.</span>
                    )}
                  </Field>
                </div>
                <div className="li-batch-settings" aria-label="Batch destination and filenames">
                  <Field label="Output destination">
                    <ChoiceSelect
                      id="batch-output-mode"
                      label="Output destination"
                      value={state.output.mode}
                      disabled={locked}
                      choices={[
                        { value: 'folder', label: 'Output folder', disabled: !state.editor.nativeExportAvailable },
                        { value: 'zip', label: 'ZIP download' },
                      ]}
                      onSelect={mode => controller.setOutput({ mode: mode as 'folder' | 'zip' })}
                    />
                    {state.output.mode === 'folder' && (
                      <div className="li-batch-folder">
                        <Button
                          id="batch-choose-output-folder"
                          disabled={locked}
                          onClick={() => void controller.chooseOutputFolder()}
                        >
                          Choose folder…
                        </Button>
                        <output id="batch-output-folder" aria-label="Batch output folder">
                          {state.output.directory || 'Choose now or when exporting'}
                        </output>
                      </div>
                    )}
                  </Field>
                  <Field label="File naming scheme">
                    <ChoiceSelect
                      id="batch-naming-scheme"
                      label="File naming scheme"
                      value={customNaming ? 'custom' : namingScheme}
                      disabled={locked}
                      choices={[
                        { value: 'suffix', label: 'Original name + local-image' },
                        { value: 'original', label: 'Original name' },
                        { value: 'numbered', label: 'Sequential numbers' },
                        { value: 'custom', label: 'Custom pattern' },
                      ]}
                      onSelect={scheme => {
                        setCustomNaming(scheme === 'custom');
                        if (scheme !== 'custom')
                          controller.setOutput({
                            namingTemplate:
                              scheme === 'original'
                                ? '{name}'
                                : scheme === 'numbered'
                                  ? 'Image-{index}'
                                  : DEFAULT_NAMING_TEMPLATE,
                          });
                      }}
                    />
                  </Field>
                  {(customNaming || namingScheme === 'custom') && (
                    <Field
                      label="Filename pattern"
                      validationState={patternError ? 'error' : 'none'}
                      validationMessage={
                        patternError ||
                        'Use {name} for the source name and {index} for a three-digit number. PNG extension is added.'
                      }
                    >
                      <Input
                        id="batch-naming-template"
                        value={state.output.namingTemplate}
                        maxLength={160}
                        disabled={locked}
                        onChange={(_, data) => controller.setOutput({ namingTemplate: data.value })}
                      />
                    </Field>
                  )}
                  <p className="li-batch-note" id="batch-filename-preview" aria-live="polite">
                    {patternError ||
                      `Example: ${filenamePreview(state.output.namingTemplate, exampleName)} · Existing files get a unique name.`}
                  </p>
                </div>
                {draft.backgroundMode === 'image' && (
                  <Field label="Background file">
                    <div className="li-batch-background-file">
                      {!active && (
                        <Button
                          id="batch-choose-background"
                          disabled={locked}
                          onClick={() => backgroundInput.current?.click()}
                        >
                          Choose image…
                        </Button>
                      )}
                      <span title={draft.backgroundName}>{draft.backgroundName || 'No image chosen'}</span>
                    </div>
                    <input
                      ref={backgroundInput}
                      id="batch-background-file"
                      type="file"
                      hidden
                      accept="image/*"
                      disabled={locked || !!active}
                      onChange={event => {
                        const file = event.currentTarget.files?.[0];
                        event.currentTarget.value = '';
                        if (file) void controller.chooseBackground(file);
                      }}
                    />
                    <span className="li-batch-note">Cropped to fill each image.</span>
                  </Field>
                )}
                {state.pending.length > 0 && (
                  <MessageBar intent="warning" id="batch-pending-warning">
                    <MessageBarBody>
                      <div id="batch-pending-description" title={pendingNames}>
                        {state.pending.length} selected {state.pending.length === 1 ? 'photo has' : 'photos have'}{' '}
                        pending selections. Apply them first, or use the images as they are.
                      </div>
                      <div className="li-batch-warning-actions">
                        <Checkbox
                          id="batch-applied-only"
                          checked={state.appliedOnly}
                          label="Use images as they are; keep pending selections"
                          onChange={(_, data) => controller.acknowledgeAppliedOnly(data.checked === true)}
                        />
                        <Button id="batch-return-apply" onClick={() => void controller.returnToSelection()}>
                          Return to editor
                        </Button>
                      </div>
                    </MessageBarBody>
                  </MessageBar>
                )}
                <div className="li-batch-selection">
                  <Checkbox
                    id="batch-all"
                    checked={allSelected ? true : state.selectedIds.length ? 'mixed' : false}
                    disabled={locked || page.total === 0}
                    label="Select all images"
                    onChange={(_, data) => controller.selectAll(data.checked === true)}
                  />
                  <span id="batch-selection-count">
                    {state.selectedIds.length} of {page.total} selected
                  </span>
                </div>
                <>
                  {page.pages > 1 && (
                    <div className="li-batch-pagination" aria-label="Batch image pages">
                      <span id="batch-page-count" aria-live="polite">
                        Page {page.page + 1} of {page.pages} · Images {page.from}–{page.to} of {page.total}
                      </span>
                      {page.pages > 1 && (
                        <>
                          <Button
                            id="batch-page-previous"
                            disabled={page.page === 0}
                            onClick={() => setPage(page.page - 1)}
                          >
                            Previous page
                          </Button>
                          <Button
                            id="batch-page-next"
                            disabled={page.page === page.pages - 1}
                            onClick={() => setPage(page.page + 1)}
                          >
                            Next page
                          </Button>
                        </>
                      )}
                    </div>
                  )}
                  <div className="li-batch-table-scroll" id="batch-table-scroll">
                    <Table size="small" aria-label="Batch images" className="li-batch-table">
                      <TableHeader>
                        <TableRow>
                          <TableHeaderCell aria-label="Selection" />
                          <TableHeaderCell>Image</TableHeaderCell>
                          <TableHeaderCell>Preview</TableHeaderCell>
                          <TableHeaderCell>Status</TableHeaderCell>
                        </TableRow>
                      </TableHeader>
                      <TableBody id="batch-rows">
                        {rows.map(entry => (
                          <TableRow key={entry.id}>
                            <TableCell>
                              <Checkbox
                                checked={selection.has(entry.id)}
                                disabled={locked}
                                aria-label={'Select ' + entry.name}
                                onChange={(_, data) => controller.setSelected(entry.id, data.checked === true)}
                              />
                            </TableCell>
                            <TableCell>{entry.name}</TableCell>
                            <TableCell>
                              {entry.preview && active ? (
                                <Button
                                  appearance="subtle"
                                  className="li-batch-preview"
                                  ref={button => {
                                    if (entry.id === inspectionTriggerId.current) inspectionTrigger.current = button;
                                  }}
                                  aria-label={`Inspect ${entry.name} at full size`}
                                  onClick={event => {
                                    inspectionTriggerId.current = entry.id;
                                    inspectionTrigger.current = event.currentTarget;
                                    controller.inspect(entry.id);
                                  }}
                                >
                                  <img
                                    loading="lazy"
                                    alt={'Batch preview of ' + entry.name}
                                    src={controller.api.previewUrl(active.id, entry.id)}
                                  />
                                </Button>
                              ) : (
                                '—'
                              )}
                            </TableCell>
                            <TableCell className={entry.error ? 'li-batch-error' : undefined}>
                              {entry.status}
                              {entry.error ? ' · ' + entry.error : ''}
                              {entry.outputName ? ' · ' + entry.outputName : ''}
                              {state.editor.pendingSelections.some(item => item.sessionId === entry.sessionId)
                                ? ' · Selection pending'
                                : ''}
                            </TableCell>
                          </TableRow>
                        ))}
                      </TableBody>
                    </Table>
                    {!rows.length && (
                      <p id="batch-empty" className="li-batch-note">
                        Open images or a folder in Cutout to remove their backgrounds.
                      </p>
                    )}
                  </div>
                </>

                <div className="li-batch-status">
                  <p
                    id="batch-status"
                    role={state.error ? 'alert' : 'status'}
                    className={state.error ? 'li-batch-error' : undefined}
                  >
                    {state.status}
                  </p>
                  {active?.running && (
                    <ProgressBar
                      id="batch-progress"
                      value={active.items.length ? completed / active.items.length : 0}
                      aria-label={`${completed} of ${active.items.length} images processed`}
                    />
                  )}
                </div>
                <Accordion collapsible className="li-batch-history">
                  <AccordionItem value="history">
                    <AccordionHeader>Previous batches</AccordionHeader>
                    <AccordionPanel className="li-batch-history-controls">
                      <ChoiceSelect
                        id="batch-history-select"
                        label="Previous batch"
                        value={state.historyId}
                        disabled={locked}
                        choices={state.queues.map(queue => ({
                          value: queue.id,
                          label: `${queue.name} · ${new Date(queue.created * 1000).toLocaleString()} · ${queue.phase}`,
                        }))}
                        onSelect={controller.setHistory}
                      />
                      <Button
                        id="batch-load-queue"
                        disabled={locked || !state.historyId}
                        onClick={() => void controller.loadQueue()}
                      >
                        Review batch
                      </Button>
                      <Button
                        id="batch-clear-queue"
                        disabled={locked || !state.historyId}
                        onClick={controller.confirmQueueClear}
                      >
                        Clear batch
                      </Button>
                      <span id="batch-cache-size" className="li-batch-note">
                        Cache {bytes(state.cacheBytes)}
                      </span>
                    </AccordionPanel>
                  </AccordionItem>
                </Accordion>
              </DialogContent>
              <DialogActions className="li-batch-actions">
                <span className="li-batch-note">Original files are kept.</span>
                {active?.running && (
                  <Button id="batch-cancel" disabled={state.working} onClick={() => void controller.cancel()}>
                    Cancel batch
                  </Button>
                )}
                {active?.phase === 'paused' && !active.running && (
                  <Button id="batch-resume" disabled={state.working} onClick={() => void controller.resume()}>
                    Resume
                  </Button>
                )}
                {active && (
                  <Button id="batch-new-queue" disabled={locked} onClick={controller.newQueue}>
                    New batch
                  </Button>
                )}
                {active && (!active.download || selectedReady > 0) && (
                  <Button
                    id="batch-export"
                    appearance="primary"
                    disabled={!state.canExport}
                    onClick={() => void controller.exportReviewed()}
                  >
                    {state.output.mode === 'folder'
                      ? state.output.directory
                        ? 'Export to folder'
                        : 'Export to folder…'
                      : 'Export ZIP'}
                    {selectedReady ? ` (${selectedReady})` : ''}
                  </Button>
                )}
                {active?.download && !active.running && (
                  <Button
                    as="a"
                    appearance={selectedReady ? 'secondary' : 'primary'}
                    id="batch-download"
                    href={controller.api.downloadUrl(active.id)}
                    download="Local Image batch.zip"
                  >
                    Download ZIP
                  </Button>
                )}
                {!active && (
                  <Button
                    id="batch-create"
                    appearance="primary"
                    disabled={!state.canPrepare}
                    onClick={() => void controller.prepare()}
                  >
                    Remove backgrounds
                  </Button>
                )}
              </DialogActions>
            </DialogBody>
          )}
        </DialogSurface>
      </Dialog>
      <Dialog
        open={!!state.confirmation}
        modalType="alert"
        onOpenChange={(_, data) => {
          if (!data.open) controller.dismissConfirmation();
        }}
      >
        <DialogSurface data-react-owned="true">
          <DialogBody>
            <DialogTitle>{state.confirmation?.kind === 'queue' ? 'Clear batch?' : 'Remove treatment?'}</DialogTitle>
            <DialogContent>
              {state.confirmation?.name}.{' '}
              {state.confirmation?.kind === 'queue'
                ? 'This removes cached previews and export copies. Original files, editor documents, saved projects and exports in your chosen folder remain.'
                : 'Existing queues and image documents keep their copies.'}
            </DialogContent>
            <DialogActions>
              <Button id="batch-confirm-no" onClick={controller.dismissConfirmation}>
                Cancel
              </Button>
              <Button id="batch-confirm-yes" appearance="primary" onClick={() => void controller.confirmRemoval()}>
                {state.confirmation?.kind === 'queue' ? 'Clear cache' : 'Remove'}
              </Button>
            </DialogActions>
          </DialogBody>
        </DialogSurface>
      </Dialog>
    </>
  );
}
