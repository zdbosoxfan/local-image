import { createBatchApi, type BatchApi } from './api.ts';
import type {
  BatchSnapshot,
  BatchEditorAdapter,
  BatchDraft,
  BatchQueue,
  BatchPendingSelection,
  CreateBatchRequest,
} from './contracts.ts';

function immutable<T>(value: T): T {
  if (value && typeof value === 'object' && !Object.isFrozen(value)) {
    Object.freeze(value);
    for (const child of Object.values(value)) immutable(child);
  }
  return value;
}
const message = (error: unknown) => (error instanceof Error ? error.message : String(error));
const initialDraft = (): BatchDraft => ({
  treatmentId: '',
  format: 'png',
  prepareCutouts: true,
  qwenVariant: 'int8',
  backgroundMode: 'transparent',
  backgroundImage: null,
  backgroundName: '',
});

/** Owns queue view/drafts only. The backend owns the durable queue, immutable
 * reviewed settings, cancellation boundaries and publication journal. Closing
 * the dialog stops observation; it does not cancel or resubmit backend work. */
export function createBatchController(options: {
  token: string;
  editor: BatchEditorAdapter;
  api?: BatchApi;
  pollMs?: number;
}) {
  const editor = options.editor,
    api = options.api ?? createBatchApi(options.token);
  const listeners = new Set<() => void>();
  let epoch = 0,
    timer: ReturnType<typeof setTimeout> | undefined,
    disposed = false,
    acknowledgedFingerprint = '';
  let state: BatchSnapshot = {
    open: false,
    working: false,
    editor: structuredClone(editor.getSnapshot()),
    treatments: [],
    queues: [],
    active: null,
    selectedIds: [],
    draft: initialDraft(),
    appliedOnly: false,
    pending: [],
    canPrepare: false,
    canExport: false,
    canSaveTreatment: false,
    status: '',
    error: false,
    cacheBytes: 0,
    treatmentBytes: 0,
    qwen: { connected: false, variants: [] },
    qwenAvailable: false,
    savingTreatment: false,
    treatmentName: '',
    historyId: '',
    inspection: null,
    confirmation: null,
  };
  let snapshot: BatchSnapshot;
  function pending(): BatchPendingSelection[] {
    const selected = new Set(state.selectedIds),
      ids = new Set(
        (state.active
          ? state.active.items.map(item => ({ id: item.id, sessionId: item.session_id }))
          : state.editor.entries
        )
          .filter(item => selected.has(item.id))
          .map(item => item.sessionId),
      );
    if (state.savingTreatment && state.editor.document) ids.add(state.editor.document.id);
    return state.editor.pendingSelections.filter(item => ids.has(item.sessionId)).map(item => ({ ...item }));
  }
  const pendingKey = (values: readonly BatchPendingSelection[]) =>
    values
      .map(item => `${item.sessionId}:${item.fingerprint}`)
      .sort()
      .join('|');
  function publish() {
    const values = pending(),
      fingerprint = pendingKey(values);
    const appliedOnly = !!fingerprint && fingerprint === acknowledgedFingerprint;
    const allowed = !values.length || appliedOnly,
      locked = state.working || !!state.active?.running || state.editor.busy;
    const selected = new Set(state.selectedIds);
    state = {
      ...state,
      pending: values,
      appliedOnly,
      qwenAvailable: state.qwen.connected && state.qwen.variants.some(item => item.available),
      canPrepare: !locked && !state.active && state.selectedIds.length > 0 && allowed,
      canExport:
        !locked && !!state.active?.items.some(item => item.status === 'ready' && selected.has(item.id)) && allowed,
      canSaveTreatment: !locked && !state.active && !!state.editor.document?.canSaveTreatment && allowed,
    };
    snapshot = immutable(structuredClone(state));
    listeners.forEach(listener => listener());
  }
  const status = (text: string, error = false) => {
    state = { ...state, status: text, error };
    publish();
  };
  function clearPoll() {
    if (timer) clearTimeout(timer);
    timer = undefined;
  }
  function acceptQueue(queue: BatchQueue, selectAll = false) {
    if (state.active?.id === queue.id && state.active.modified > queue.modified) return false;
    state = {
      ...state,
      active: structuredClone(queue),
      historyId: queue.id,
      status: queue.message,
      error: false,
      ...(selectAll ? { selectedIds: queue.items.map(item => item.id), inspection: null } : {}),
    };
    publish();
    return true;
  }
  async function refreshLists(scope = epoch) {
    const [treatments, queues] = await Promise.all([api.treatments(), api.queues()]);
    if (scope !== epoch || disposed) return;
    const visibleQueues = queues.items.filter(
      queue => queue.prepare_cutouts && !queue.treatment_id && queue.format === 'png',
    );
    state = {
      ...state,
      treatments: treatments.items,
      queues: visibleQueues,
      cacheBytes: queues.bytes,
      treatmentBytes: treatments.bytes,
      historyId: visibleQueues.some(item => item.id === state.historyId) ? state.historyId : visibleQueues[0]?.id || '',
    };
    publish();
  }
  function schedule() {
    clearPoll();
    if (!state.open || !state.active?.running || disposed) return;
    const id = state.active.id,
      scope = epoch;
    timer = setTimeout(async () => {
      try {
        const queue = await api.queue(id);
        if (scope !== epoch || state.active?.id !== id || disposed) return;
        acceptQueue(queue);
        if (!queue.running) await refreshLists(scope);
      } catch (error) {
        if (scope === epoch) status(message(error), true);
      }
      if (scope === epoch) schedule();
    }, options.pollMs ?? 600);
  }
  async function run(action: () => Promise<void>) {
    if (state.working || disposed) return;
    const scope = epoch;
    state = { ...state, working: true };
    publish();
    try {
      await action();
    } catch (error) {
      if (scope === epoch) status(message(error), true);
    } finally {
      if (scope === epoch) {
        state = { ...state, working: false };
        publish();
        schedule();
      }
    }
  }
  function close() {
    ++epoch;
    clearPoll();
    state = { ...state, open: false, working: false, confirmation: null };
    publish();
  }
  async function open() {
    if (state.editor.busy || state.working || disposed) return;
    const scope = ++epoch;
    clearPoll();
    acknowledgedFingerprint = '';
    state = {
      ...state,
      open: true,
      active: null,
      working: true,
      editor: structuredClone(editor.getSnapshot()),
      inspection: null,
      savingTreatment: false,
      confirmation: null,
      selectedIds: editor.getSnapshot().entries.map(entry => entry.id),
      draft: initialDraft(),
      status: 'Select images, remove their backgrounds, review, then export PNGs.',
      error: false,
    };
    publish();
    try {
      await editor.prepareForBatch();
      if (scope !== epoch) return;
      state = { ...state, editor: structuredClone(editor.getSnapshot()) };
      publish();
      await refreshLists(scope);
      const qwen = await api.qwenStatus().catch(() => ({ connected: false, variants: [] }));
      if (scope !== epoch) return;
      state = { ...state, qwen };
      const running = state.queues.find(queue => queue.running);
      if (running) acceptQueue(running, true);
    } catch (error) {
      if (scope === epoch) status(message(error), true);
    } finally {
      if (scope === epoch) {
        state = { ...state, working: false };
        publish();
        schedule();
      }
    }
  }
  const unsubscribe = editor.subscribe(() => {
    state = { ...state, editor: structuredClone(editor.getSnapshot()) };
    publish();
  });
  publish();
  return {
    getSnapshot: () => snapshot,
    subscribe(listener: () => void) {
      listeners.add(listener);
      return () => {
        listeners.delete(listener);
      };
    },
    open,
    close,
    isOpen: () => state.open,
    dispose() {
      disposed = true;
      ++epoch;
      clearPoll();
      unsubscribe();
      listeners.clear();
    },
    api,
    setSelected(id: string, selected: boolean) {
      if (state.working || state.active?.running) return;
      const values = new Set(state.selectedIds);
      selected ? values.add(id) : values.delete(id);
      state = { ...state, selectedIds: [...values] };
      publish();
    },
    selectAll(selected: boolean) {
      if (state.working || state.active?.running) return;
      state = {
        ...state,
        selectedIds: selected ? (state.active?.items || state.editor.entries).map(item => item.id) : [],
      };
      publish();
    },
    setDraft(change: Partial<BatchDraft>) {
      if (state.working || state.active) return;
      const draft = { ...state.draft, ...change };
      draft.treatmentId = '';
      draft.format = 'png';
      draft.prepareCutouts = true;
      state = { ...state, draft };
      publish();
    },
    chooseBackground: (file: File) =>
      run(async () => {
        if (state.active) return;
        if (file.size > 16 * 1024 ** 2) throw new Error('Choose a background smaller than 16 MB.');
        const scope = epoch;
        const encoded = await new Promise<string>((resolve, reject) => {
          const reader = new FileReader();
          reader.onload = () => resolve(String(reader.result).split(',')[1]);
          reader.onerror = () => reject(new Error('The background file could not be read.'));
          reader.readAsDataURL(file);
        });
        if (scope !== epoch) return;
        state = {
          ...state,
          draft: { ...state.draft, backgroundMode: 'image', backgroundImage: encoded, backgroundName: file.name },
        };
        publish();
      }),
    acknowledgeAppliedOnly(value: boolean) {
      acknowledgedFingerprint = value ? pendingKey(pending()) : '';
      publish();
    },
    setTreatmentName(treatmentName: string) {
      state = { ...state, treatmentName };
      publish();
    },
    beginTreatment() {
      if (state.canSaveTreatment) {
        state = { ...state, savingTreatment: true };
        publish();
      }
    },
    cancelTreatment() {
      state = { ...state, savingTreatment: false };
      publish();
    },
    saveTreatment: () =>
      run(async () => {
        const current = state.editor.document;
        if (
          !current?.canSaveTreatment ||
          state.active ||
          (pending().length && pendingKey(pending()) !== acknowledgedFingerprint)
        )
          throw new Error('Acknowledge applied pixels only or apply the pending selection first.');
        const name = state.treatmentName.trim();
        if (!name) throw new Error('Name this treatment.');
        const scope = epoch,
          value = await api.saveTreatment({
            name,
            session_id: current.id,
            revision: current.revision,
            format: state.draft.format,
          });
        if (scope !== epoch) return;
        await refreshLists(scope);
        state = { ...state, draft: { ...state.draft, treatmentId: value.id }, savingTreatment: false };
        status(`Saved ${value.name}. Each product retains its own mask.`);
      }),
    prepare: () => {
      if (!state.canPrepare) return Promise.resolve();
      const scope = epoch,
        context = structuredClone(state.editor),
        draft = { ...state.draft },
        selected = new Set(state.selectedIds);
      return run(async () => {
        const entries = context.entries.filter(entry => selected.has(entry.id));
        if (!entries.length) throw new Error('Open and select the source photos for a new queue.');
        if (
          entries.some(entry => !entry.cutoutReady) &&
          (!state.qwen.connected || !state.qwen.variants.some(item => item.id === draft.qwenVariant && item.available))
        )
          throw new Error('Qwen is unavailable. Set it up in Settings, or select only images with existing cutouts.');
        if (draft.backgroundMode === 'image' && !draft.backgroundImage)
          throw new Error('Choose a background image first.');
        const body: CreateBatchRequest = {
          treatment_id: null,
          format: 'png',
          prepare_cutouts: true,
          qwen_variant: draft.qwenVariant,
          background_mode: draft.backgroundMode,
          ...(draft.backgroundMode === 'image'
            ? { background_image: draft.backgroundImage!, background_name: draft.backgroundName }
            : {}),
        };
        if (context.nativeCollection && context.collectionId) {
          body.collection_id = context.collectionId;
          body.entry_ids = entries.map(entry => entry.id);
        } else {
          body.sessions = [];
          for (const entry of entries) {
            if (scope !== epoch) return;
            body.sessions.push(await editor.resolveSession(entry.id));
          }
          if (scope !== epoch) return;
          state = { ...state, editor: structuredClone(editor.getSnapshot()) };
          publish();
        }
        if (scope !== epoch) return;
        if (pending().length && pendingKey(pending()) !== acknowledgedFingerprint)
          throw new Error('The pending selection changed. Review the applied-pixels choice again.');
        const queue = await api.create(body);
        if (scope !== epoch) return;
        acceptQueue(queue, true);
        schedule();
      });
    },
    exportReviewed: () => {
      if (!state.canExport || !state.active) return Promise.resolve();
      const queue = state.active,
        scope = epoch,
        selected = new Set(state.selectedIds),
        item_ids = queue.items.filter(item => item.status === 'ready' && selected.has(item.id)).map(item => item.id);
      return run(async () => {
        const value =
          state.editor.nativeExportAvailable && editor.exportBatchFolder
            ? await editor.exportBatchFolder({ job_id: queue.id, item_ids })
            : await api.exportZip(queue.id, item_ids);
        if (scope !== epoch) return;
        if (value) {
          if (value.id !== queue.id) throw new Error('The export response belongs to a different queue.');
          acceptQueue(value);
          schedule();
        } else {
          const current = await api.queue(queue.id);
          if (scope === epoch) {
            acceptQueue(current);
            status('Export cancelled. The reviewed queue remains available.');
          }
        }
      });
    },
    cancel: () =>
      run(async () => {
        const active = state.active,
          scope = epoch;
        if (!active?.running) return;
        const queue = await api.cancel(active.id);
        if (scope === epoch) {
          acceptQueue(queue);
          status('Cancelling after the current image. Completed outputs are retained.');
        }
      }),
    resume: () =>
      run(async () => {
        const active = state.active,
          scope = epoch;
        if (!active || active.running || active.phase !== 'paused') return;
        const queue = await api.resume(active.id);
        if (scope === epoch) acceptQueue(queue);
      }),
    setHistory(historyId: string) {
      state = { ...state, historyId };
      publish();
    },
    loadQueue: () => {
      if (!state.historyId || state.working || state.active?.running) return Promise.resolve();
      const id = state.historyId,
        scope = ++epoch;
      clearPoll();
      acknowledgedFingerprint = '';
      return run(async () => {
        const queue = await api.queue(id);
        if (scope === epoch) acceptQueue(queue, true);
      });
    },
    newQueue() {
      if (state.working || state.active?.running) return;
      ++epoch;
      clearPoll();
      acknowledgedFingerprint = '';
      const selected = new Set(state.selectedIds),
        items = state.active?.items.filter(item => selected.has(item.id)) || [];
      // Collection item IDs survive lazy import. A null session is not an
      // identity and must never match every unopened image in the folder.
      const entryIds = new Set(items.map(item => item.id)),
        sessions = new Set(items.flatMap(item => (item.session_id ? [item.session_id] : [])));
      let selectedIds = state.editor.entries
        .filter(item => entryIds.has(item.id) || (!!item.sessionId && sessions.has(item.sessionId)))
        .map(item => item.id);
      if (!selectedIds.length) selectedIds = state.editor.entries.map(item => item.id);
      const draft = { ...initialDraft(), qwenVariant: state.draft.qwenVariant };
      state = { ...state, active: null, selectedIds, draft, inspection: null, savingTreatment: false };
      status('Choose images and a background, then remove backgrounds and review.');
      void refreshLists().catch(error => status(message(error), true));
    },
    inspect(itemId: string) {
      if (!state.active?.items.some(item => item.id === itemId && item.preview)) return;
      state = { ...state, inspection: { itemId, original: false, zoom: 'fit' } };
      publish();
    },
    closeInspection() {
      state = { ...state, inspection: null };
      publish();
    },
    setInspection(change: Partial<{ original: boolean; zoom: 'fit' | '100' | '200' }>) {
      if (state.inspection) {
        state = { ...state, inspection: { ...state.inspection, ...change } };
        publish();
      }
    },
    returnToSelection: async () => {
      const target = pending()[0];
      if (!target) return;
      close();
      try {
        await editor.returnToPendingSelection(target.sessionId);
      } catch (error) {
        status(message(error), true);
      }
    },
    confirmTreatmentRemoval() {
      if (state.active || state.working || !state.draft.treatmentId) return;
      state = {
        ...state,
        confirmation: {
          kind: 'treatment',
          id: state.draft.treatmentId,
          name: state.treatments.find(item => item.id === state.draft.treatmentId)?.name || 'Saved treatment',
        },
      };
      publish();
    },
    confirmQueueClear() {
      if (state.working || state.active?.running || !state.historyId) return;
      state = {
        ...state,
        confirmation: {
          kind: 'queue',
          id: state.historyId,
          name: state.queues.find(item => item.id === state.historyId)?.name || 'Queue',
        },
      };
      publish();
    },
    dismissConfirmation() {
      state = { ...state, confirmation: null };
      publish();
    },
    confirmRemoval: () =>
      run(async () => {
        const target = state.confirmation,
          scope = epoch;
        if (!target) return;
        state = { ...state, confirmation: null };
        publish();
        if (target.kind === 'treatment') await api.deleteTreatment(target.id);
        else await api.clearQueue(target.id);
        if (scope !== epoch) return;
        if (target.kind === 'queue' && state.active?.id === target.id)
          state = { ...state, active: null, inspection: null, selectedIds: state.editor.entries.map(item => item.id) };
        if (target.kind === 'treatment' && state.draft.treatmentId === target.id)
          state = { ...state, draft: { ...state.draft, treatmentId: '', prepareCutouts: false } };
        await refreshLists(scope);
        status(
          target.kind === 'queue'
            ? 'Queue cache cleared. Original images and folder exports are kept.'
            : 'Treatment removed. Existing queue copies are kept.',
        );
      }),
  };
}
export type BatchController = ReturnType<typeof createBatchController>;
