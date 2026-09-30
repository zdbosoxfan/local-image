import type { BackendStatus, HardwareGuide, RemovalSettings, SetupState } from './types.ts';

export function createSettingsApi(token: string, request: typeof fetch = fetch) {
  async function read<T>(path: string): Promise<T> {
    const response = await request(path, { headers: { 'x-local-remove-token': token } });
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
    hardware: () => read<HardwareGuide>('/api/local-remove/hardware'),
  };
}
export type SettingsApi = ReturnType<typeof createSettingsApi>;
