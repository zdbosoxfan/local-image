import type { HardwareGuide, SetupState } from '../settings/types.ts';
export interface ModelVariant { id: string; label: string; available?: boolean; reason?: string; total_bytes?: number; missing_bytes?: number; downloadable?: boolean; download_note?: string; hardware?: { vram_recommendation?: string; basis?: string } }
export interface BrowserModel {
  id: string; label: string; available?: boolean; historical?: boolean; reason?: string; description?: string; benefit?: string; short_benefit?: string;
  variants?: ModelVariant[]; defaults?: { variant?: string; steps?: number; guidance?: number }; recommended?: { steps?: number; guidance?: number };
  capabilities?: { text_to_image?: boolean; image_reference?: boolean; image_to_image?: boolean; max_references?: number; transparent?: boolean; lora?: boolean; loras?: boolean };
  strengths?: string[]; notes?: string[]; limitations?: string[]; license?: { url?: string; label?: string };
  storage_bytes?: number; downloadable?: boolean; download_note?: string; hardware?: { vram_recommendation?: string; basis?: string };
  [key: string]: unknown;
}
export interface DownloadJob { running?: boolean; phase?: string; model?: string; variant?: string; message?: string; error?: string; progress?: number | null; downloaded_bytes?: number; total_bytes?: number }
export interface ModelDownloads extends DownloadJob { model_directory?: string; models?: Array<{ id: string; variants?: Array<{ id: string; installed?: boolean; total_bytes?: number; missing_bytes?: number }> }> }
export interface LoraSelection { id: string; title?: string; strength: number; missing?: boolean; usage?: string; trigger_phrase?: string }
export type Compatibility = 'curated' | 'declared' | 'unverified';
export interface LoraItem {
  id?: string; model?: string; title?: string; repo_id?: string; filename?: string; revision?: string; bytes?: number;
  description?: string; style?: string; usage?: string; experimental?: boolean; license?: string; license_note?: string;
  compatibility?: Compatibility; supported?: boolean; warning?: string; trigger_phrase?: string;
  recommended_settings?: { steps?: number; guidance?: number }; recommended_strength?: number;
  preview_url?: string; preview_available?: boolean; example_source?: string; example_caption?: string;
}
export interface LoraInventory { installed: LoraItem[]; curated: LoraItem[]; job?: DownloadJob }
export interface LoraFiles { model?: string; repo_id: string; revision: string; files: LoraItem[]; compatibility?: Compatibility; supported?: boolean; warning?: string }
export interface LoraDraftPort {
  contextId: string;
  read(): { modelId: string; modelLabel?: string; selected: LoraSelection[]; referenceCount?: number; supportsLoras?: boolean };
  setSelected(loras: LoraSelection[]): void;
  appendPrompt(text: string): void;
  applySampling(settings: { steps?: number; guidance?: number }): void;
}
export interface LoraDownload { model: string; repo_id: string; filename: string; revision: string; allow_unverified: boolean }
export interface ModelBridge {
  capabilities(): { setup: boolean };
  editorBusy(): boolean;
  focusCanvas(): void;
  chooseModelDirectory(): Promise<SetupState | null>;
  startBackend(): Promise<unknown | null>;
  downloadModel(model: string, variant: string): Promise<unknown | null>;
  downloadLora(payload: LoraDownload): Promise<unknown | null>;
  legacyModelSelection?(): ModelBrowserSelection;
  getLegacyLoraPort?(context: 'generate' | 'draft' | 'final'): LoraDraftPort;
  acceptCatalog?(models: BrowserModel[]): void;
}
export interface ModelBrowserSelection { selectedModelId?: string; selectedVariant?: string; onUse?: (modelId: string, variant: string) => unknown }
export interface ModelsSnapshot {
  view: 'models' | 'loras' | null; loading: boolean; pendingNative: boolean; error: string; message: string;
  nativeSetup: boolean; models: BrowserModel[]; selectedModelId: string; selectedVariant: string;
  downloads: ModelDownloads | null; setup: SetupState | null; hardware: HardwareGuide | null;
  context: { contextId: string; modelId: string; modelLabel: string; referenceCount: number; supportsLoras: boolean } | null;
  selectedLoras: LoraSelection[]; loraTab: 'installed' | 'browse'; inventory: LoraInventory | null; query: string; searchResults: LoraItem[];
  searching: boolean; filesLoading: boolean; files: LoraFiles | null; selectedFilename: string; allowUnverified: boolean; info: LoraItem | null; loraJob: DownloadJob | null;
  checkedAt: number | null;
}
declare global { interface Window { LocalImageModelBridge?: ModelBridge } }
