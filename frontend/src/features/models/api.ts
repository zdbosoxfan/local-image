import type { HardwareGuide, SetupState } from '../settings/types.ts';
import type { BrowserModel, DownloadJob, LoraFiles, LoraInventory, LoraItem, ModelDownloads } from './types.ts';
export function createModelApi(token: string, request: typeof fetch = fetch) {
  async function read<T>(path: string): Promise<T> {
    const response = await request(path, { headers: { 'x-local-remove-token': token } });
    const value: unknown = await response.json();
    if (!response.ok) throw Error(value && typeof value === 'object' && 'detail' in value && typeof value.detail === 'string' ? value.detail : `Could not read model details (${response.status}).`);
    return value as T;
  }
  return {
    catalog: (refresh = false) => read<{ models: BrowserModel[]; default_model?: string }>(`/api/local-remove/generation/models${refresh ? '?refresh=true' : ''}`),
    downloads: () => read<ModelDownloads>('/api/local-remove/generator/download'),
    setup: () => read<SetupState>('/api/local-remove/setup'), hardware: () => read<HardwareGuide>('/api/local-remove/hardware'),
    inventory: (model: string) => read<LoraInventory>(`/api/local-remove/loras?model=${encodeURIComponent(model)}`),
    search: (model: string, query: string) => read<{ results: LoraItem[] }>(`/api/local-remove/loras/search?model=${encodeURIComponent(model)}&query=${encodeURIComponent(query)}`),
    files: (model: string, repo: string, revision?: string) => read<LoraFiles>(`/api/local-remove/loras/files?model=${encodeURIComponent(model)}&repo_id=${encodeURIComponent(repo)}${revision ? `&revision=${encodeURIComponent(revision)}` : ''}`),
    loraDownload: () => read<DownloadJob>('/api/local-remove/loras/download'),
  };
}
export type ModelApi = ReturnType<typeof createModelApi>;
