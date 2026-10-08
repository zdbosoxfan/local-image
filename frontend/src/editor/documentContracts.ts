import type { EditorDocument, EditorSnapshot, Layer, Transform, Workspace } from '../contracts.ts';
import type { ToolActions, ToolSnapshot } from '../features/shell/ToolControls.tsx';
import type { DocumentChromeActions, DocumentChromeState } from '../features/shell/DocumentChrome.tsx';
import type { ClosePlan, CloseChoice, OverwriteChoice, Credit } from '../features/shell/editorDialogs.ts';
import type { CanvasController } from './canvasController.ts';
import type { CanvasInteraction, CanvasSnapshot } from './canvasContracts.ts';
import type { NativeBridge } from './nativeBridge.ts';

export type OutputFormat = 'original' | 'png' | 'jpg' | 'tif' | 'webp';
export type SaveMode = 'overwrite' | 'unique' | 'export';
export interface CutoutState {
  enabled?: boolean;
  alpha?: string;
  feather?: number;
  transform?: Partial<Transform>;
  shadow?: {
    enabled?: boolean;
    color?: string;
    opacity?: number;
    blur?: number;
    offset_x?: number;
    offset_y?: number;
    squeeze?: number;
  };
  background?: {
    mode?: 'transparent' | 'color' | 'image';
    color?: string;
    name?: string;
    attribution?: Attribution;
    reference_attributions?: Attribution[];
  };
}
export interface Attribution {
  provider?: string;
  asset_id?: string;
  title?: string;
  creator?: string;
  source_url?: string;
  license_url?: string;
  license?: string;
  attribution?: string;
  [key: string]: unknown;
}
export interface DocumentLayer extends Layer {
  cutout?: CutoutState;
  bounds?: number[];
  attribution?: Attribution;
  reference_attributions?: Attribution[];
}
export interface DocumentMetadata extends EditorDocument {
  layers: Array<{
    id: string;
    name?: string;
    visible: boolean;
    discarded?: boolean;
    x?: number;
    y?: number;
    width?: number;
    height?: number;
    [key: string]: unknown;
  }>;
  layer_stack?: DocumentLayer[];
  bit_depth?: number;
  selected_layer_id?: string;
  can_return?: boolean;
  source_name?: string | null;
  saved_revision?: number | null;
  saved_name?: string | null;
  project_name?: string | null;
  project_saved?: boolean;
  project_saved_revision?: number | null;
  has_project_path?: boolean;
  saved?: boolean;
  edited?: boolean;
  stack_can_undo?: boolean;
  stack_can_redo?: boolean;
  cutout_can_undo?: boolean;
  cutout_can_redo?: boolean;
  cutout?: CutoutState;
  generation?: Record<string, unknown>;
  upscale?: Record<string, unknown>;
  source_attribution?: Attribution;
  reference_attributions?: Attribution[];
  collection_id?: string;
  entry_id?: string;
}
export interface CollectionEntry {
  id: string;
  name: string;
  session_id: string | null;
  thumbnail?: string | null;
  dirty?: boolean;
  project_dirty?: boolean;
  edited?: boolean;
  saved?: boolean;
  saved_name?: string | null;
}
export interface ImageCollection {
  id: string;
  name: string;
  local?: boolean;
  entries: CollectionEntry[];
}
export interface NativeOpenResult {
  session?: DocumentMetadata;
  collection?: ImageCollection;
  index?: number;
}
export interface SaveResult {
  saved?: boolean;
  name?: string;
  download?: string | null;
  mode?: string;
  bit_depth?: number;
  session?: DocumentMetadata;
  collection?: ImageCollection | null;
  index?: number | null;
}
export interface SaveOutcome {
  mode: SaveMode;
  documentId: string;
  name: string;
  confirmed: boolean;
  kind: 'written' | 'download-started';
}
export interface DocumentContext {
  document: DocumentMetadata | null;
  documentId: string | null;
  revision: number;
  layerId: string | null;
  navigationEpoch: number;
  busy: boolean;
}
export interface DocumentHealth {
  settingsLoaded: boolean;
  ready: boolean;
  retouchReady: boolean;
  device: string;
  models: Array<{ id: string; label?: string; available?: boolean; reason?: string; [key: string]: unknown }>;
  healMethods: Array<{ id: string; label: string; available?: boolean }>;
  qwen: {
    ready: boolean;
    connected: boolean;
    variants: Array<{ id: string; label: string; available: boolean; reason?: string }>;
    reason?: string;
  };
}
export interface DocumentSnapshot extends EditorSnapshot {
  document: DocumentMetadata | null;
  selectedLayerId: string | null;
  navigationEpoch: number;
  tool: CanvasInteraction['tool'];
  tools: ToolSnapshot;
  chrome: DocumentChromeState;
  collection: ImageCollection | null;
  collectionIndex: number;
  filmstripCollapsed: boolean;
  thumbnailSize: number;
  recentSessions: readonly DocumentMetadata[];
  openDocuments: readonly DocumentMetadata[];
  outputFormat: OutputFormat;
  askBeforeOverwrite: boolean;
  closeInProgress: boolean;
  operationLabel: string | null;
  health: DocumentHealth;
  penPointCount: number;
  canApplySelection: boolean;
  selectionActionLabel: string;
  undoLabel: string;
  redoLabel: string;
  creditsAvailable: boolean;
  canvas: CanvasSnapshot;
}
export type DocumentCanvasPort = Pick<
  CanvasController,
  | 'getSnapshot'
  | 'subscribe'
  | 'presentDocument'
  | 'setInteractionState'
  | 'rememberCurrentView'
  | 'pendingSelection'
  | 'forgetDocument'
  | 'clearSelection'
  | 'commitSelection'
  | 'fullMaskPayload'
  | 'selectionPayload'
  | 'finishPen'
  | 'cancelPen'
  | 'undoSelection'
  | 'redoSelection'
  | 'cancelGesture'
  | 'resetTransientInput'
  | 'fit'
  | 'actualSize'
  | 'setPhotoZoom'
  | 'zoomIn'
  | 'zoomOut'
  | 'setBrushSize'
  | 'stepBrushSize'
  | 'focus'
  | 'pendingFingerprint'
>;
export type DocumentNativePort = Pick<
  NativeBridge,
  'capabilities' | 'subscribe' | 'connect' | 'openFiles' | 'openFolder' | 'openProject' | 'drop' | 'saveProject'
> &
  Partial<Pick<NativeBridge, 'acceptDrop'>>;
export interface DocumentDialogsPort {
  isOpen(): boolean;
  confirmOverwrite(filename: string): Promise<OverwriteChoice>;
  confirmClose(plan: ClosePlan): Promise<CloseChoice>;
  showCredits(credits: readonly Credit[], text: string): void;
}
export interface DocumentBrowserPort {
  chooseFiles(kind: 'images' | 'folder' | 'project' | 'background'): Promise<readonly File[] | null>;
  /** Starts a browser/WebView download. Its return value is never confirmation
   * that the user completed an operating-system Save dialog. */
  download(url: string, name: string): void | Promise<void>;
  replaceUrl?(url: string): void;
  createObjectURL?(file: File): string;
  revokeObjectURL?(url: string): void;
}
export interface DocumentFeatureCommands {
  prepareDocumentCommand?(
    kind: 'save' | 'project' | 'close' | 'credits',
  ): Promise<{ allowed: boolean; forceExport?: boolean }>;
  afterDocumentOpened?(document: DocumentMetadata): void | Promise<void>;
  noteClosed?(ids: readonly string[]): void;
  enterGenerate?(): void | Promise<unknown>;
  leaveGenerate?(workspace: 'retouch' | 'cutout'): void | Promise<unknown>;
  resetWorkspace?(): void;
  visibleDocumentId?(): string | null;
  showAssets?(destination?: 'image' | 'background' | 'reference' | 'draft'): unknown;
  showGenerated?(): unknown;
  showSettings?(): unknown;
  showHardware?(): unknown;
  showShortcuts?(): unknown;
  showBatch?(): unknown;
  browseModels?(): unknown;
  edgeOptions?(): unknown;
  generateBackground?(): unknown;
  renameLayer?(id: string): unknown;
}
export interface DocumentControllerPorts {
  canvas: DocumentCanvasPort;
  native?: DocumentNativePort;
  dialogs: DocumentDialogsPort;
  browser: DocumentBrowserPort;
  storage?: Pick<Storage, 'getItem' | 'setItem'>;
  loadImage(url: string): Promise<HTMLImageElement>;
  isModalOpen?(): boolean;
  features?: DocumentFeatureCommands;
}
export interface DocumentControllerViews {
  toolActions: ToolActions;
  chromeActions: DocumentChromeActions;
}
export interface SavedInteraction extends CanvasInteraction {
  workspace: Workspace;
}
