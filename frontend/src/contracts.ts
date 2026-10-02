export type Workspace = 'retouch' | 'cutout' | 'generate';
export interface Transform { offset_x: number; offset_y: number; scale: number; rotation: number }
export interface Layer {
  id: string;
  name: string;
  kind: 'original' | 'retouch' | 'image' | 'cutout';
  visible: boolean;
  locked: boolean;
  discarded?: boolean;
  opacity: number;
  transform: Transform;
  patch_ids?: string[];
  display_key?: string;
  // Mask, shadow, provenance and repair relationships pass through untouched.
  [key: string]: unknown;
}
export interface EditorDocument {
  id: string;
  name: string;
  revision: number;
  width: number;
  height: number;
  layer_stack?: Layer[];
  dirty?: boolean;
  project_dirty?: boolean;
  [key: string]: unknown;
}
export interface LayerPatch {
  name?: string; visible?: boolean; locked?: boolean; discarded?: boolean;
  opacity?: number; index?: number; transform?: Partial<Transform>;
}
export interface StackRequest {
  documentId: string; revision: number; tail: string;
  body: Record<string, unknown>; method: string;
}
export interface EditorSnapshot {
  document: EditorDocument | null;
  selectedLayerId: string | null;
  busy: boolean;
  workspace: Workspace;
  showOriginal: boolean;
  canUndo: boolean;
  canRedo: boolean;
  status: string;
  tool?: string;
  nativeReady?: boolean;
  nativeProjects?: boolean;
  selectionActive?: boolean;
  statusError?: boolean;
  refining?: boolean;
  creatingBlank?: boolean;
  generationVisible?: boolean;
  inspectorHidden?: boolean;
}
export interface EditorCommands {
  selectLayer(id: string): void;
  createRetouch(): unknown;
  patchLayer(id: string, change: LayerPatch): unknown;
  reorderLayer(direction: number): unknown;
  restoreLayer(): unknown;
  mergeLayers(): unknown;
  addMask(): unknown;
  undo(): unknown;
  redo(): unknown;
  fit(): void;
  actualSize(): void;
  setWorkspace(workspace: Workspace): void;
  selectTool(tool: string): void;
  toggleOriginal(): void;
  openFiles(): unknown;
  openFolder(): unknown;
  openProject(): unknown;
  newWorkspace?(): unknown;
  activateOpenDocument?(id: string): unknown;
  closeOpenDocument?(id: string): unknown;
  saveProject(): unknown;
  saveProjectAs(): unknown;
  exportImage(): unknown;
  showSettings(): void;
  browseModels(): unknown;
  startTask(workspace: Workspace): unknown;
  toggleInspector(): void;
  showAssets?(): void;
}
export interface LegacyEditor {
  getSnapshot(): EditorSnapshot;
  subscribe(listener: () => void): () => void;
  commands: EditorCommands;
  canvas: { element: HTMLElement; fit(): void; actualSize(): void; focus(): void; cancelGesture(): void };
  native: { capabilities(): { ready: boolean; projects: boolean; setup: boolean }; openFiles(): unknown; openFolder(): unknown; openProject(): unknown; saveProject(saveAs?: boolean): unknown };
  setStackTransport(transport: (request: StackRequest) => Promise<EditorDocument>): void;
}
declare global {
  interface Window {
    __LOCAL_IMAGE_REACT__?: boolean;
    __LOCAL_IMAGE_BOOTSTRAP__?: { nonce: string; token: string };
    LocalImageLegacyEditor?: LegacyEditor;
  }
}
