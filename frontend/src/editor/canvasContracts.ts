import type { Camera, LayerTransform, Point } from './canvasMath.ts';
export type CanvasTool = 'brush' | 'pen' | 'rectangle' | 'ellipse' | 'move';
export interface CanvasLayer { id: string; kind: string; visible: boolean; discarded?: boolean; locked?: boolean; opacity?: number; transform?: Partial<LayerTransform>; bounds?: readonly number[]; display_key?: string }
export interface LegacyPatch { id: string; visible: boolean; discarded?: boolean; x?: number; y?: number; width?: number; height?: number }
export interface CanvasDocument { id: string; revision: number; width: number; height: number; layer_stack?: readonly CanvasLayer[]; layers?: readonly LegacyPatch[]; cutout?: { enabled?: boolean; transform?: Partial<LayerTransform> }; cutout_bounds?: readonly number[] }
export interface CanvasInteraction { tool: CanvasTool; workspace: 'retouch' | 'cutout' | 'generate'; operation: string; cutoutOperation: 'erase' | 'restore'; subtract: boolean; brushSize: number; handActive: boolean; showOriginal: boolean; busy: boolean; selectedLayerId: string | null }
export interface CanvasElements { viewport: HTMLElement; stage: HTMLElement; photo: HTMLCanvasElement; photoImage: HTMLImageElement; overlay: HTMLCanvasElement; draft: HTMLCanvasElement; layerStack: HTMLElement; brushCursor: HTMLElement }
export interface CanvasDisplay { document: CanvasDocument; original: HTMLImageElement; composite?: HTMLImageElement | null; legacyLayers?: ReadonlyArray<{ layer: LegacyPatch; image: HTMLImageElement }>; interaction?: Partial<CanvasInteraction> }
export interface TransformCommit { documentId: string; revision: number; layerId?: string; transform: LayerTransform }
export interface CanvasPorts {
  getAcceptedDocument(): Pick<CanvasDocument, 'id' | 'revision'> | null;
  isModalOpen(): boolean;
  isMenuOpen?(): boolean;
  /** Blank generation and refinement keep this document mounted but inactive. */
  isEditorHidden?(): boolean;
  commitLayerTransform(value: TransformCommit & { layerId: string }): Promise<unknown>;
  commitLegacyCutoutTransform?(value: TransformCommit): Promise<unknown>;
  loadImage?(url: string): Promise<HTMLImageElement>;
  restoreInteraction?(value: CanvasInteraction): void;
  report?(message: string, error?: boolean): void;
  onSnapshot?(value: CanvasSnapshot): void;
}
export interface CanvasSnapshot { documentId: string | null; revision: number | null; hasSelection: boolean; pointCount: number; selectionVersion: number; canUndoSelection: boolean; canRedoSelection: boolean; historyBusy: boolean; gesture: 'draw' | 'pan' | 'move' | null; photoZoom: number; fitMode: boolean; panX: number; panY: number; previewWidth: number; previewHeight: number }
export type SelectionRedo = { kind: 'point'; point: Point } | { kind: 'mask'; mask: string };
/** Internal browser-only state; never part of a React snapshot or project file. */
export interface CanvasViewState { mask: string | null; undo: string[]; redo: SelectionRedo[]; points: Point[]; width: number; height: number; interaction: CanvasInteraction; photoZoom: number; camera: Camera; viewportWidth: number; viewportHeight: number; hasSelection: boolean; selectionVersion: number }
