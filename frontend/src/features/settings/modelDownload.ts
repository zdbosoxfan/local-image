import type { SettingsSnapshot } from './types.ts';

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
  return { model, variant, ready, filesPresent, missing, total, downloadable, note };
}
