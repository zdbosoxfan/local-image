import type { DraftKey, GenerationDraft, GenerationModel, GenerationPayload, GenerationReference, SizeLimits, SizeMath } from './types.ts';

export function workflowLimits(model: GenerationModel | undefined, references = false): SizeLimits {
  const limits = model?.limits ?? {}; return references && limits.reference_dimensions ? { ...limits, ...limits.reference_dimensions } : limits;
}
export function supportsReferences(model?: GenerationModel) { const cap = model?.capabilities; return !!(cap?.image_reference || cap?.image_to_image || cap?.references) && (cap?.max_references ?? 0) > 0; }
export function validateDraft(key: DraftKey, draft: GenerationDraft, model: GenerationModel | undefined, references: GenerationReference[], math: SizeMath): string[] {
  const errors: string[] = [], variant = model?.variants.find(item => item.id === draft.variant), cap = model?.capabilities;
  if (!model || !model.available || !variant?.available) errors.push(variant?.reason || model?.reason || 'Select an installed model and precision preset.');
  if (key === 'final' && !cap?.image_reference) errors.push('Refinement requires a model with semantic image references.');
  if (key === 'edit' && !references.length) errors.push('Choose an image to edit.');
  if ((key === 'create' || key === 'draft') && !cap?.text_to_image) errors.push('This model does not support text-to-image generation.');
  if (!draft.prompt.trim() || draft.prompt.length > 4000) errors.push('Enter a prompt of 1–4,000 characters.');
  if (references.length && !supportsReferences(model)) errors.push('This model cannot use image inputs. Remove them or choose another model.');
  if (references.length > (cap?.max_references ?? 0)) errors.push(`This model accepts at most ${cap?.max_references ?? 0} image inputs.`);
  if (new Set(references.map(item => item.id)).size !== references.length) errors.push('Add each reference image only once.');
  const size = math.sizeLimits(workflowLimits(model, references.length > 0));
  if (![draft.width, draft.height].every(Number.isSafeInteger) || draft.width < size.width.min || draft.width > size.width.max || draft.height < size.height.min || draft.height > size.height.max || draft.width % size.width.step !== 0 || draft.height % size.height.step !== 0 || draft.width * draft.height > size.pixels) errors.push('Use dimensions supported by this model and reference workflow.');
  const limits = model?.limits ?? {};
  if (!Number.isInteger(draft.steps) || draft.steps < (limits.min_steps ?? 1) || draft.steps > (limits.max_steps ?? 100)) errors.push('Use the supported sampling step count.');
  if (!Number.isFinite(draft.guidance) || draft.guidance < (limits.min_guidance ?? 1) || draft.guidance > (limits.max_guidance ?? 1)) errors.push('Use the supported guidance value.');
  if (draft.seed.trim() && (!Number.isSafeInteger(Number(draft.seed)) || Number(draft.seed) < 0)) errors.push('Seed must be blank or a nonnegative safe integer.');
  if (draft.transparent && !cap?.transparent) errors.push('This model produces opaque output.');
  if (draft.negativePrompt.length > 2000) errors.push('Negative prompt must be at most 2,000 characters.');
  if (cap?.denoise && references.length && (!Number.isFinite(draft.denoise) || draft.denoise < .05 || draft.denoise > 1)) errors.push('Variation strength must be between 0.05 and 1.');
  if (draft.loras.length && (cap?.lora === false || cap?.loras === false)) errors.push('This model does not support style adapters.');
  if (draft.loras.length > (limits.max_loras ?? 3)) errors.push('Too many style adapters for this model.');
  if (draft.loras.some(item => item.missing)) errors.push('A saved adapter is unavailable. Install or remove it before running.');
  if (draft.loras.some(item => item.usage === 'reference-edit') && !references.length) errors.push('The selected adapter requires a reference image.');
  if (draft.loras.some(item => !Number.isFinite(item.strength) || item.strength < -2 || item.strength > 2)) errors.push('Adapter strength must be between -2 and 2.');
  return [...new Set(errors)];
}
export function generationPayload(draft: GenerationDraft, model: GenerationModel, references: GenerationReference[]): GenerationPayload {
  const cap = model.capabilities;
  return { model: model.id, variant: draft.variant, prompt: draft.prompt.trim(), width: draft.width, height: draft.height, steps: draft.steps, guidance: draft.guidance,
    transparent: draft.transparent, reference_session_ids: references.map(item => item.id), loras: draft.loras.map(item => ({ id: item.id, strength: item.strength })),
    ...(draft.seed.trim() ? { seed: Number(draft.seed) } : {}), ...(cap.negative_prompt && draft.guidance > 1 ? { negative_prompt: draft.negativePrompt.trim() } : {}), ...(cap.denoise && references.length ? { denoise: draft.denoise } : {}) };
}
