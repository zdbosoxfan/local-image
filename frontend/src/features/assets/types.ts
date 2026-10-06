import type { EditorDocument } from '../../contracts.ts';

export type AssetTab = 'stock' | 'folders' | 'generated';
export type StockProviderId = 'openverse' | 'pexels' | 'unsplash';
export type AssetDestination = 'image' | 'background' | 'reference' | 'draft';
export interface StockProvider {
  id: StockProviderId;
  label: string;
  available: boolean;
  needs_key?: boolean;
  connect_url?: string;
  description?: string;
}
export interface StockImage {
  id: string;
  provider: StockProviderId;
  title: string;
  creator: string;
  creator_url?: string;
  source_url: string;
  license: string;
  license_url: string;
  attribution: string;
  thumbnail_url: string;
  width: number;
  height: number;
}
export interface StockPage {
  results: StockImage[];
  page: number;
  next_page: number | null;
  warning?: string;
}
export interface FolderEntry {
  id: string;
  name: string;
  thumbnail: string;
}
export interface BackgroundLibrary {
  id: string;
  name: string;
  entries: FolderEntry[];
  browser?: boolean;
}
export interface GeneratedImage {
  id: string;
  name: string;
  model: string;
  variant: string;
  width: number;
  height: number;
  bytes: number;
  thumbnail: string;
  generation?: { prompt?: string } | null;
  reference_attributions?: unknown[];
}
export interface GeneratedLibrary {
  items: GeneratedImage[];
  count: number;
  bytes: number;
  warning?: string;
  storage_note?: string;
}
export interface DeleteResult extends GeneratedLibrary {
  deleted: string[];
  freed_bytes: number;
}
export type DeleteSelection = { all: true } | { ids: string[] };

/** Browser navigation and reference draft identities must be captured before a
 * request. The host accepts results only if these identities still match. */
export interface AssetsContext {
  documentId: string | null;
  revision: number;
  layerId: string | null;
  navigationEpoch: number;
  busy: boolean;
  nativeReady: boolean;
  canReference: boolean;
  referenceTarget: string | null;
  canUseDraft: boolean;
}
export interface AssetsHost {
  getContext(): AssetsContext;
  subscribeContext(listener: () => void): () => void;
  /** Shared editor queue captures the accepted revision and navigation here,
   * reserves busy state through preview acceptance, and releases it in finally. */
  runDocumentChange<T>(work: (context: AssetsContext) => Promise<T>): Promise<T>;
  acceptDocument(
    document: EditorDocument,
    context: AssetsContext,
    destination: 'image' | 'background' | 'generated',
  ): Promise<boolean>;
  addReference(document: EditorDocument, context: AssetsContext): Promise<boolean>;
  useAsDraft(document: EditorDocument, context: AssetsContext): Promise<boolean>;
  chooseBackgroundFolder(): Promise<BackgroundLibrary | null>;
  setDockVisible(visible: boolean): void;
}
export interface AssetsState {
  open: boolean;
  expanded: boolean;
  tab: AssetTab;
  context: AssetsContext;
  loading: boolean;
  working: boolean;
  status: string;
  error: string | null;
  providers: StockProvider[];
  provider: StockProviderId;
  query: string;
  stock: StockPage;
  stockSelected: string | null;
  folders: BackgroundLibrary[];
  folderSelected: string;
  folderEntrySelected: string | null;
  generated: GeneratedLibrary;
  generatedQuery: string;
  generatedSelected: string | null;
  selectionMode: boolean;
  selectedCopies: string[];
  deleteSelection: DeleteSelection | null;
}
