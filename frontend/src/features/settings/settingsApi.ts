import type {
  BackendStatus,
  HardwareGuide,
  HardwarePreference,
  RemovalSettings,
  SetupState,
  UpdateStatus,
} from './types.ts';
import type { BrowserModel, ModelDownloads } from '../models/types.ts';

export function createSettingsApi(token: string, request: typeof fetch = fetch) {
  async function call<T>(path: string, method: 'GET' | 'POST', body?: unknown): Promise<T> {
    const response = await request(path, {
      method,
      headers: {
        'x-local-remove-token': token,
        ...(body === undefined ? {} : { 'Content-Type': 'application/json' }),
      },
      ...(body === undefined ? {} : { body: JSON.stringify(body) }),
    });
    const data: unknown = await response.json();
    if (!response.ok) {
      const detail = data && typeof data === 'object' && 'detail' in data ? data.detail : null;
      throw new Error(typeof detail === 'string' ? detail : `Could not read setup (${response.status}).`);
    }
    return data as T;
  }
  const read = <T>(path: string) => call<T>(path, 'GET');
  const post = <T>(path: string) => call<T>(path, 'POST');
  return {
    settings: () => read<RemovalSettings>('/api/local-remove/settings'),
    update: (refresh = false) => read<UpdateStatus>(`/api/local-remove/update${refresh ? '?refresh=true' : ''}`),
    checkUpdate: () => post<UpdateStatus>('/api/local-remove/update/check'),
    downloadUpdate: () => post<UpdateStatus>('/api/local-remove/update/download'),
    setup: (detect = false) => read<SetupState>(`/api/local-remove/setup${detect ? '/detect' : ''}`),
    status: () => read<BackendStatus>('/api/local-remove/status'),
    qwen: () => read<Record<string, unknown>>('/api/local-remove/qwen/status'),
    modelCatalog: (refresh = false) =>
      read<{ models: BrowserModel[] }>(`/api/local-remove/generation/models${refresh ? '?refresh=true' : ''}`),
    modelDownloads: () => read<ModelDownloads>('/api/local-remove/generator/download'),
    hardware: () => read<HardwareGuide>('/api/local-remove/hardware'),
    hardwarePreference: () => read<HardwarePreference>('/api/local-remove/hardware/preference'),
    saveHardwarePreference: (dontShowAgain: boolean) =>
      call<HardwarePreference>('/api/local-remove/hardware/preference', 'POST', { dont_show_again: dontShowAgain }),
  };
}
export type SettingsApi = ReturnType<typeof createSettingsApi>;
