import type { DocumentController } from './documentController.ts';
import type { NativeBridge } from './nativeBridge.ts';
import { deepFreeze } from './documentLogic.ts';
import type { SettingsBridge, InterfaceDensity, SetupState } from '../features/settings/types.ts';
import type { ModelBridge } from '../features/models/types.ts';
import type { BatchEditorAdapter, BatchEditorSnapshot, BatchQueue } from '../features/batch/contracts.ts';

const DENSITY_KEY = 'local-image.interface-density.v1';
const density = (value: string | null | undefined): InterfaceDensity =>
  value === 'compact' || value === 'large' ? value : 'comfortable';
/** Domain ports for the fully owned React app. The feature controllers never
 * inspect legacy controls or receive a generic native request/filename proxy. */
export function createFeatureAdapters(options: {
  documents: DocumentController;
  native: NativeBridge;
  storage?: Pick<Storage, 'getItem' | 'setItem'>;
  applyDensity(value: InterfaceDensity): void;
}) {
  const { documents, native } = options;
  const readDensity = () => {
    try {
      return density(options.storage?.getItem(DENSITY_KEY));
    } catch {
      return 'comfortable' as const;
    }
  };
  const focusCanvas = () => {
    documents.canvas.resetTransientInput();
    documents.canvas.focus();
  };
  const settingsBridge: SettingsBridge = {
    capabilities: () => ({ ready: native.capabilities().ready, setup: native.capabilities().setup }),
    editorBusy: () => documents.getSnapshot().busy,
    preferences: () => ({ askBeforeOverwrite: documents.getSnapshot().askBeforeOverwrite, density: readDensity() }),
    acceptConfiguration: value => documents.acceptConfiguration(value),
    setOperationPending: value => documents.setSettingsPending(value),
    // The document controller owns the existing
    // local-remove-ask-before-overwrite=true|false preference and live value.
    setOverwritePreference: value => documents.commands.setOverwritePreference(value),
    setDensity: value => {
      const next = density(value);
      try {
        options.storage?.setItem(DENSITY_KEY, next);
      } catch {
        /* Session styling still applies when persistence is unavailable. */
      }
      options.applyDensity(next);
    },
    focusCanvas,
    chooseRuntime: () => native.setupChooseComfyDirectory(),
    chooseInstallDirectory: () => native.setupChooseInstallDirectory(),
    installRuntime: () => native.setupInstall(),
    chooseModelDirectory: () => native.setupChooseModelDirectory(),
    downloadRemovalModels: () => native.setupDownloadModels(),
    downloadModel: (model, variant) => native.setupDownloadGenerationModel(model, variant),
    startBackend: () => native.setupStart(),
    ejectModels: () => native.setupEject(),
    useInstallation: id => native.setupUseInstallation(id),
    configureConnection: () => native.configureAi(),
    installUpdate: () => native.updateInstall(),
  };
  const modelBridge: ModelBridge = {
    capabilities: () => ({ setup: native.capabilities().setup }),
    editorBusy: () => documents.getSnapshot().busy,
    focusCanvas,
    chooseModelDirectory: async () => (await native.setupChooseModelDirectory()) as SetupState | null,
    startBackend: () => native.setupStart(),
    downloadModel: (model, variant) => native.setupDownloadGenerationModel(model, variant),
    downloadLora: value =>
      native.loraDownload({
        model: value.model,
        repo_id: value.repo_id,
        filename: value.filename,
        revision: value.revision,
        allow_unverified: value.allow_unverified,
      }),
  };
  let batchSnapshot: BatchEditorSnapshot | null = null,
    batchSignature = '';
  function getBatchSnapshot(): BatchEditorSnapshot {
    const snapshot = documents.getSnapshot(),
      doc = snapshot.document,
      collection = snapshot.collection;
    const source = collection?.entries ?? (doc ? [{ id: doc.id, name: doc.name, session_id: doc.id }] : []);
    const pendingSelections: BatchEditorSnapshot['pendingSelections'][number][] = [];
    for (const entry of source)
      if (entry.session_id && documents.pendingSelection(entry.session_id))
        pendingSelections.push({
          sessionId: entry.session_id,
          name: entry.name,
          fingerprint: documents.pendingFingerprint(entry.session_id),
        });
    if (doc && documents.pendingSelection(doc.id) && !pendingSelections.some(item => item.sessionId === doc.id))
      pendingSelections.push({ sessionId: doc.id, name: doc.name, fingerprint: documents.pendingFingerprint(doc.id) });
    const next: BatchEditorSnapshot = {
      busy: snapshot.busy,
      document: doc
        ? { id: doc.id, name: doc.name, revision: doc.revision, canSaveTreatment: !!doc.cutout?.enabled }
        : null,
      collectionId: collection?.id ?? null,
      nativeCollection: !!collection && !collection.local,
      entries: source.map(entry => ({
        id: entry.id,
        name: entry.name,
        sessionId: entry.session_id,
        cutoutReady: !!entry.session_id && documents.hasCutout(entry.session_id),
      })),
      pendingSelections,
      nativeExportAvailable: native.capabilities().ready && native.capabilities().batch,
    };
    const signature = JSON.stringify(next);
    if (!batchSnapshot || signature !== batchSignature) {
      batchSignature = signature;
      batchSnapshot = deepFreeze(next);
    }
    return batchSnapshot;
  }
  const batchEditor: BatchEditorAdapter = {
    getSnapshot: getBatchSnapshot,
    subscribe: listener => documents.subscribe(listener),
    prepareForBatch: async () => {
      const before = documents.getContext();
      if (before.busy) throw Error('Finish the current operation before opening Batch.');
      documents.setMenuOpen(false);
      documents.canvas.cancelGesture();
      documents.rememberCurrentView();
      await documents.flush();
      const after = documents.getContext();
      if (after.navigationEpoch !== before.navigationEpoch || after.documentId !== before.documentId)
        throw Error('The current image changed while opening Batch. Open Batch again.');
    },
    resolveSession: async id => {
      const collectionId = documents.getSnapshot().collection?.id ?? null;
      const value = await documents.resolveCollectionSession(id);
      if ((documents.getSnapshot().collection?.id ?? null) !== collectionId)
        throw Error('The image collection changed during batch preparation.');
      return value;
    },
    returnToPendingSelection: id => documents.returnToPendingSelection(id),
    exportBatchFolder: async value => {
      if (!native.capabilities().ready || !native.capabilities().batch)
        throw Error('This desktop host does not support batch folder export.');
      const result = await native.batchExportFolder({ job_id: value.job_id, item_ids: [...value.item_ids] });
      if (result === null) return null;
      if (result.id !== value.job_id || !Array.isArray(result.items) || typeof result.modified !== 'number')
        throw Error(
          'The desktop returned an unexpected batch queue. Refresh the reviewed queue before exporting again.',
        );
      return structuredClone(result) as unknown as BatchQueue;
    },
  };
  return { settingsBridge, modelBridge, batchEditor, applyStoredDensity: () => options.applyDensity(readDensity()) };
}
export type FeatureAdapters = ReturnType<typeof createFeatureAdapters>;
