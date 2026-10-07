import type { BackendStatus, HardwareGuide, HardwarePreference, RemovalSettings, SetupState } from './types.ts';
import type { BrowserModel, ModelDownloads } from '../models/types.ts';

export function createSettingsApi(token: string, request: typeof fetch = fetch) {
  async function read<T>(path: string, init: RequestInit = {}): Promise<T> {
    const response = await request(path, { ...init, headers: { ...init.headers, 'x-local-remove-token': token } });
    const data: unknown = await response.json();
    if (!response.ok) {
      const detail = data && typeof data === 'object' && 'detail' in data ? data.detail : null;
      throw new Error(typeof detail === 'string' ? detail : `Could not read setup (${response.status}).`);
    }
    return data as T;
  }
  return {
    settings: () => read<RemovalSettings>('/api/local-remove/settings'),
    setup: (detect = false) => read<SetupState>(`/api/local-remove/setup${detect ? '/detect' : ''}`),
    status: () => read<BackendStatus>('/api/local-remove/status'),
    qwen: () => read<Record<string, unknown>>('/api/local-remove/qwen/status'),
    modelCatalog: (refresh = false) =>
      read<{ models: BrowserModel[] }>(`/api/local-remove/generation/models${refresh ? '?refresh=true' : ''}`),
    modelDownloads: () => read<ModelDownloads>('/api/local-remove/generator/download'),
    hardware: () => read<HardwareGuide>('/api/local-remove/hardware'),
    hardwarePreference: () => read<HardwarePreference>('/api/local-remove/hardware/preference'),
    saveHardwarePreference: (dontShowAgain: boolean) =>
      read<HardwarePreference>('/api/local-remove/hardware/preference', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ dont_show_again: dontShowAgain }),
      }),
  };
}
export type SettingsApi = ReturnType<typeof createSettingsApi>;
