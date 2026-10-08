import type { BrowserModel } from '../models/types.ts';
import type { SetupState, RemovalSettings } from './types.ts';

export const REMOVAL_MODEL_ID = 'flux2-klein-remove';

export function removalModel(setup: SetupState | null | undefined, settings?: RemovalSettings): BrowserModel | null {
  if (!setup?.models?.length) return null;
  const files = setup.models.map(file => ({
    name: file.name,
    folder: file.folder || '',
    bytes: file.expected_bytes || file.bytes || 0,
    exists: file.exists,
  }));
  const ready = settings
    ? settings.models.some(model => model.id === 'klein' && model.available) === true
    : setup.service?.flux_ready === true;
  return {
    id: REMOVAL_MODEL_ID,
    label: 'FLUX.2 Klein Base 4B · AI Remove',
    available: ready,
    description: 'Retouch object removal with FLUX.2 Klein Base and its object removal adapter.',
    strengths: [
      'Local object removal in the Retouch workspace',
      'Uses the same model folder and shared companion files',
    ],
    limitations: ['This preset is used by AI Remove; choose the distilled Klein preset for image generation.'],
    hardware: { vram_recommendation: '16 GB recommended' },
    defaults: { variant: 'bf16' },
    variants: [
      {
        id: 'bf16',
        label: 'BF16',
        available: ready,
        total_bytes: files.reduce((total, file) => total + file.bytes, 0),
        missing_bytes: files.reduce((total, file) => total + (file.exists ? 0 : file.bytes), 0),
        files,
      },
    ],
  };
}
