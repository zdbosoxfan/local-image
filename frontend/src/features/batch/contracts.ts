export type BatchBackground = 'transparent' | 'white' | 'image';
export type BatchFormat = 'original' | 'png' | 'jpg' | 'tif' | 'webp';
export type BatchItemStatus =
  'pending' | 'preparing' | 'ready' | 'exporting' | 'exported' | 'failed' | 'conflict' | 'needs-cutout';
export interface BatchItem {
  id: string;
  name: string;
  session_id: string | null;
  revision: number | null;
  status: BatchItemStatus;
  error: string | null;
  preview: string | null;
  output_name: string | null;
  credits_name: string | null;
  export_bit_depth: number | null;
}
export interface BatchQueue {
  id: string;
  name: string;
  created: number;
  modified: number;
  phase: 'preparing' | 'review' | 'exporting' | 'paused' | 'complete';
  running: boolean;
  message: string;
  format: BatchFormat;
  mode: 'prepare' | 'folder' | 'zip';
  prepare_cutouts: boolean;
  qwen_variant: 'int8' | 'bf16';
  treatment_id: string | null;
  background_mode?: BatchBackground;
  background_name?: string | null;
  treatment_name: string | null;
  items: readonly BatchItem[];
  bytes: number;
  download: string | null;
  naming_template?: string;
}
export interface BatchTreatment {
  id: string;
  name: string;
  created: number;
  format: BatchFormat;
}
export interface BatchEntry {
  id: string;
  name: string;
  sessionId: string | null;
  cutoutReady?: boolean;
}
export interface BatchPendingSelection {
  sessionId: string;
  name: string;
  fingerprint: string;
}
export interface BatchEditorSnapshot {
  busy: boolean;
  document: { id: string; name: string; revision: number; canSaveTreatment: boolean } | null;
  collectionId: string | null;
  nativeCollection: boolean;
  entries: readonly BatchEntry[];
  pendingSelections: readonly BatchPendingSelection[];
  nativeExportAvailable: boolean;
}
/** Native folder paths come from the host's picker and remain owned by it. */
export interface BatchEditorAdapter {
  getSnapshot(): BatchEditorSnapshot;
  subscribe(listener: () => void): () => void;
  prepareForBatch(): Promise<void>;
  resolveSession(entryId: string): Promise<{ session_id: string; revision: number }>;
  returnToPendingSelection(sessionId: string): Promise<void>;
  chooseBatchExportFolder?(): Promise<{ directory: string } | null>;
  exportBatchFolder?(request: {
    job_id: string;
    item_ids: string[];
    naming_template?: string;
    use_selected_folder?: boolean;
  }): Promise<BatchQueue | null>;
}
export interface BatchDraft {
  treatmentId: string;
  format: BatchFormat;
  prepareCutouts: boolean;
  qwenVariant: 'int8' | 'bf16';
  backgroundMode: BatchBackground;
  backgroundImage: string | null;
  backgroundName: string;
}
export interface CreateBatchRequest {
  treatment_id: string | null;
  format: BatchFormat;
  prepare_cutouts: boolean;
  qwen_variant: 'int8' | 'bf16';
  background_mode?: BatchBackground;
  background_image?: string;
  background_name?: string;
  collection_id?: string;
  entry_ids?: string[];
  sessions?: { session_id: string; revision: number }[];
}
export interface QwenBatchStatus {
  connected: boolean;
  variants: readonly { id: string; available: boolean }[];
}
export interface BatchSnapshot {
  open: boolean;
  working: boolean;
  editor: BatchEditorSnapshot;
  treatments: readonly BatchTreatment[];
  queues: readonly BatchQueue[];
  active: BatchQueue | null;
  selectedIds: readonly string[];
  draft: BatchDraft;
  output: { mode: 'folder' | 'zip'; directory: string; namingTemplate: string };
  appliedOnly: boolean;
  pending: readonly BatchPendingSelection[];
  canPrepare: boolean;
  canExport: boolean;
  canSaveTreatment: boolean;
  status: string;
  error: boolean;
  cacheBytes: number;
  treatmentBytes: number;
  qwen: QwenBatchStatus;
  qwenAvailable: boolean;
  savingTreatment: boolean;
  treatmentName: string;
  historyId: string;
  inspection: { itemId: string; original: boolean; zoom: 'fit' | '100' | '200' } | null;
  confirmation: { kind: 'treatment' | 'queue'; id: string; name: string } | null;
}
