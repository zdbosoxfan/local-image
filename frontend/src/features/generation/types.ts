import type { EditorDocument } from '../../contracts.ts';

export type GenerationMode = 'create' | 'edit' | 'refine';
export type DraftKey = 'create' | 'edit' | 'draft' | 'final';
export interface DimensionAxis {
  min?: number;
  max?: number | null;
  step?: number;
}
export interface SizeLimits {
  width?: DimensionAxis;
  height?: DimensionAxis;
  min_dimension?: number;
  max_dimension?: number | null;
  dimension_step?: number;
  max_pixels?: number | null;
  reference_dimensions?: SizeLimits;
  resolution_note?: string;
}
export interface GenerationModel {
  id: string;
  label: string;
  available: boolean;
  reason?: string;
  historical?: boolean;
  variants: { id: string; label?: string; available: boolean; reason?: string }[];
  capabilities: {
    text_to_image?: boolean;
    image_reference?: boolean;
    image_to_image?: boolean;
    references?: boolean;
    reference_mode?: string | null;
    max_references: number;
    transparent?: boolean;
    negative_prompt?: boolean;
    denoise?: boolean;
    loras?: boolean;
    lora?: boolean;
  };
  defaults: {
    variant: string;
    width?: number;
    height?: number;
    steps: number;
    guidance: number;
    denoise?: number | null;
  };
  limits: SizeLimits & {
    min_steps?: number;
    max_steps?: number;
    min_guidance?: number;
    max_guidance?: number;
    max_loras?: number;
  };
  notes?: string[];
}
export interface LoraSelection {
  id: string;
  title?: string;
  strength: number;
  missing?: boolean;
  usage?: string;
  trigger_phrase?: string;
}
export interface GenerationReference {
  id: string;
  name: string;
  thumbnail: string;
  revision?: number;
  attribution?: unknown;
}
export interface GenerationDraft {
  modelId: string;
  variant: string;
  prompt: string;
  negativePrompt: string;
  width: number;
  height: number;
  steps: number;
  guidance: number;
  seed: string;
  transparent: boolean;
  denoise: number;
  references: GenerationReference[];
  loras: LoraSelection[];
  locked: boolean;
  ratio: number;
  aspect: string;
  missingReferenceCount: number;
}
export interface GenerationPayload {
  model: string;
  variant: string;
  prompt: string;
  width: number;
  height: number;
  steps: number;
  guidance: number;
  transparent: boolean;
  reference_session_ids: string[];
  loras: { id: string; strength: number }[];
  seed?: number;
  negative_prompt?: string;
  denoise?: number;
  operation_id?: string;
}
export interface GenerationResult {
  session: EditorDocument;
  seed?: number;
  width?: number;
  height?: number;
  library_warning?: string;
}
export interface OperationProgress {
  active: boolean;
  job_id: string | null;
  model?: string;
  stage: string;
  stage_label: string;
  elapsed_seconds: number;
  progress: { value: number; max: number; percent?: number } | null;
  connection_lost?: boolean;
  error?: string;
  updated_seconds_ago?: number;
  can_cancel?: boolean;
  cancelling?: boolean;
  cancellation_requested?: boolean;
  cancel_error?: string;
}
export interface HardwareDevice {
  id: string;
  name: string;
  source: 'NVIDIA driver' | 'ComfyUI';
  utilization_percent: number | null;
  vram_used_bytes: number | null;
  vram_total_bytes: number | null;
  is_backend_device: boolean;
  memory_scope?: 'device' | 'backend' | 'shared';
}
export interface HardwareUsage {
  devices: HardwareDevice[];
  comfy_connected: boolean;
  sampled_at: number;
  refresh_after_ms: number;
}
export interface UpscaleInventory {
  enabled: boolean;
  model: { id: string; label: string; available: boolean; reason?: string };
  limits: SizeLimits;
  reason?: string;
}
export interface RefinementImage {
  session: EditorDocument;
  references: GenerationReference[];
  draftId?: string;
}
export interface GenerationBackgroundTarget {
  id: string;
  name: string;
}
export interface GenerationContext {
  document: EditorDocument | null;
  navigationEpoch: number;
  busy: boolean;
  backgroundTarget?: GenerationBackgroundTarget | null;
}
export interface LoraDraftPort {
  contextId: string;
  context: 'generate' | 'draft' | 'final';
  read(): {
    modelId: string;
    modelLabel?: string;
    selected: LoraSelection[];
    referenceCount: number;
    supportsLoras: boolean;
  };
  setSelected(loras: LoraSelection[]): void;
  appendPrompt(text: string): void;
  applySampling(settings: { steps?: number; guidance?: number }): void;
}
export interface SizeMath {
  fitDimensions(
    values: {
      width: number;
      height: number;
      ratio?: number;
      locked?: boolean;
      axis?: 'width' | 'height';
      fallbackWidth?: number;
      fallbackHeight?: number;
    },
    limits?: SizeLimits,
  ): { width: number; height: number };
  sizeLimits(limits?: SizeLimits): {
    width: { min: number; max: number; step: number };
    height: { min: number; max: number; step: number };
    pixels: number;
  };
  dimensionBounds(
    values: { width: number; height: number; ratio?: number; locked?: boolean },
    limits?: SizeLimits,
  ): {
    minWidth: number;
    maxWidth: number;
    minHeight: number;
    maxHeight: number;
    widthStep: number;
    heightStep: number;
  };
}
export interface GenerationHost {
  getContext(): GenerationContext;
  subscribeContext(listener: () => void): () => void;
  runGeneration<T>(work: (context: GenerationContext) => Promise<T>): Promise<T>;
  acceptResult(document: EditorDocument, context: GenerationContext, mode: GenerationMode): Promise<boolean>;
  applyGeneratedBackground(document: EditorDocument, context: GenerationContext): Promise<boolean>;
  activateMode(mode: GenerationMode, document: EditorDocument | null): Promise<boolean>;
  openAssets(destination: 'reference' | 'draft'): void;
  openModels(options: {
    selectedModelId: string;
    selectedVariant?: string;
    onUse(model: string, variant?: string): void;
  }): void;
  openLoras(port: LoraDraftPort): void;
  canEjectModels?(): boolean;
  ejectModels?(): Promise<unknown>;
  sizeMath: SizeMath;
}
export interface RefinementRecipe {
  schema: 1;
  name: string;
  stages: { draft: Record<string, unknown>; final: Record<string, unknown> };
  aspect?: string;
  negative?: string;
  denoise?: string;
  includeReferences?: boolean;
  upscale?: { enabled?: boolean; preset?: string; width?: string; height?: string };
}
export interface GenerationState {
  mode: GenerationMode;
  context: GenerationContext;
  models: GenerationModel[];
  loading: boolean;
  working: boolean;
  error: string | null;
  status: string;
  drafts: Record<DraftKey, GenerationDraft>;
  modeDocuments: { create: EditorDocument | null; edit: EditorDocument | null };
  draftImages: RefinementImage[];
  resultImages: RefinementImage[];
  selectedDraftId: string | null;
  selectedResultId: string | null;
  includeReferences: boolean;
  upscale: { enabled: boolean; preset: string; width: number; height: number };
  upscaleInventory: UpscaleInventory | null;
  progress: OperationProgress | null;
  watching: boolean;
  uncertain: boolean;
  progressError: string | null;
  runningKey: DraftKey | 'upscale' | null;
  stopping: boolean;
  cancelError: string | null;
  ejecting: boolean;
  hardware: HardwareUsage | null;
  hardwareLoading: boolean;
  hardwareError: string | null;
  recipes: RefinementRecipe[];
  recipeWarnings: string[];
}
