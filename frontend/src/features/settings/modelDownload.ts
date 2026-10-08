import type { SettingsSnapshot } from './types.ts';
import type { BrowserModel, ModelDownloads } from '../models/types.ts';

export function modelFolderSummary(downloads: ModelDownloads | null, models: readonly BrowserModel[]) {
  if (!downloads?.models) return 'Scan this folder to check for downloaded models.';
  const found = downloads.models.flatMap(model => {
    const details = models.find(item => item.id === model.id);
    return (model.variants || [])
      .filter(variant => variant.installed || variant.missing_bytes === 0)
      .map(variant => `${details?.label || model.id} (${variant.id.toUpperCase()})`);
  });
  for (const model of models.filter(model => !downloads.models?.some(item => item.id === model.id))) {
    for (const variant of model.variants || []) {
      if (variant.files?.length && variant.missing_bytes === 0)
        found.push(`${model.label} (${variant.id.toUpperCase()})`);
    }
  }
  return found.length
    ? `Model files found: ${found.join(', ')}.`
    : 'No complete supported model presets found. Check the diffusion_models, text_encoders and vae subfolders inside this folder.';
}

/** Files in the local folder and availability in ComfyUI are separate facts. */
export function modelDownloadSelection(
  state: Pick<
    SettingsSnapshot,
    'models' | 'modelCatalogAvailable' | 'modelDownloads' | 'selectedModelId' | 'selectedVariant'
  >,
) {
  const model = state.models.find(item => item.id === state.selectedModelId);
  const variant = model?.variants?.find(item => item.id === state.selectedVariant);
  const disk = state.modelDownloads?.models
    ?.find(item => item.id === model?.id)
    ?.variants?.find(item => item.id === variant?.id);
  const missing = disk?.missing_bytes ?? variant?.missing_bytes;
  const total = disk?.total_bytes ?? variant?.total_bytes ?? model?.storage_bytes;
  const filesPresent = disk?.installed === true || missing === 0;
  const ready = state.modelCatalogAvailable && variant?.available === true;
  const downloadable = model?.downloadable !== false && variant?.downloadable !== false;
  const note = variant?.download_note || model?.download_note || '';
  const files = disk?.files ?? variant?.files ?? [];
  return { model, variant, ready, filesPresent, missing, total, downloadable, note, files };
}
