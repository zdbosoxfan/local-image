import type { HardwareGuide, SetupState } from '../settings/types.ts';
import type { BrowserModel, DownloadJob, LoraFiles, LoraInventory, LoraItem, ModelDownloads } from './types.ts';
export function createModelApi(token: string, request: typeof fetch = fetch) {
  async function read<T>(path: string, init: RequestInit = {}): Promise<T> {
    const response = await request(path, { ...init, headers: { ...init.headers, 'x-local-remove-token': token } });
    const value: unknown = await response.json();
    if (!response.ok) throw Error(value && typeof value === 'object' && 'detail' in value && typeof value.detail === 'string' ? value.detail : `Could not read model details (${response.status}).`);
    return value as T;
  }
  return {
    catalog: (refresh = false) => read<{ models: BrowserModel[]; default_model?: string }>(`/api/local-remove/generation/models${refresh ? '?refresh=true' : ''}`),
    downloads: () => read<ModelDownloads>('/api/local-remove/generator/download'),
    setup: () => read<SetupState>('/api/local-remove/setup'), hardware: () => read<HardwareGuide>('/api/local-remove/hardware'),
    contentPreference: () => read<{ show_adult_content: boolean }>('/api/local-remove/loras/preferences'),
    saveContentPreference: (showAdult: boolean) => read<{ show_adult_content: boolean }>('/api/local-remove/loras/preferences', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ show_adult_content: showAdult }) }),
    inventory: (model: string, showAdult = false) => read<LoraInventory>(`/api/local-remove/loras?model=${encodeURIComponent(model)}&show_adult=${showAdult}`),
    search: (model: string, query: string, showAdult = false) => read<{ results: LoraItem[]; hidden_count?: number; content_labels?: Record<string, Pick<LoraItem, 'content_rating' | 'content_rating_source'>> }>(`/api/local-remove/loras/search?model=${encodeURIComponent(model)}&query=${encodeURIComponent(query)}&show_adult=${showAdult}`),
    files: (model: string, repo: string, revision?: string, showAdult = false) => read<LoraFiles>(`/api/local-remove/loras/files?model=${encodeURIComponent(model)}&repo_id=${encodeURIComponent(repo)}${revision ? `&revision=${encodeURIComponent(revision)}` : ''}&show_adult=${showAdult}`),
    loraDownload: () => read<DownloadJob>('/api/local-remove/loras/download'),
  };
}
export type ModelApi = ReturnType<typeof createModelApi>;
