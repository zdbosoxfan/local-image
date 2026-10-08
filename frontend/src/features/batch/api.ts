import type { BatchQueue, BatchTreatment, BatchFormat, CreateBatchRequest, QwenBatchStatus } from './contracts.ts';

const prefix = '/api/local-remove/batch';
const encode = (id: string) => encodeURIComponent(id);
export class BatchApiError extends Error {
  readonly status: number;
  constructor(message: string, status: number) {
    super(message);
    this.name = 'BatchApiError';
    this.status = status;
  }
}
export function createBatchApi(token: string, transport: typeof fetch = fetch) {
  async function request<T>(tail: string, method = 'GET', body?: unknown): Promise<T> {
    const response = await transport(prefix + tail, {
      method,
      headers: { 'x-local-remove-token': token, ...(body === undefined ? {} : { 'Content-Type': 'application/json' }) },
      ...(body === undefined ? {} : { body: JSON.stringify(body) }),
    });
    let value: unknown;
    try {
      value = await response.json();
    } catch {
      throw new BatchApiError(`Batch response could not be read (${response.status}).`, response.status);
    }
    if (!response.ok) {
      const detail = value && typeof value === 'object' && 'detail' in value ? value.detail : null;
      throw new BatchApiError(
        typeof detail === 'string' ? detail : `Batch request failed (${response.status}).`,
        response.status,
      );
    }
    return value as T;
  }
  return {
    treatments: () => request<{ items: BatchTreatment[]; bytes: number }>('/treatments'),
    queues: () => request<{ items: BatchQueue[]; bytes: number }>('/jobs'),
    queue: (id: string) => request<BatchQueue>('/jobs/' + encode(id)),
    create: (body: CreateBatchRequest) => request<BatchQueue>('/jobs', 'POST', body),
    exportZip: (id: string, itemIds: string[], namingTemplate?: string) =>
      request<BatchQueue>('/jobs/' + encode(id) + '/export', 'POST', {
        item_ids: itemIds,
        ...(namingTemplate ? { naming_template: namingTemplate } : {}),
      }),
    cancel: (id: string) => request<BatchQueue>('/jobs/' + encode(id) + '/cancel', 'POST', {}),
    resume: (id: string) => request<BatchQueue>('/jobs/' + encode(id) + '/resume', 'POST', {}),
    saveTreatment: (body: { name: string; session_id: string; revision: number; format: BatchFormat }) =>
      request<BatchTreatment>('/treatments', 'POST', body),
    deleteTreatment: (id: string) => request<{ deleted: boolean }>('/treatments/' + encode(id), 'DELETE'),
    clearQueue: (id: string) => request<{ deleted: boolean }>('/jobs/' + encode(id), 'DELETE'),
    async qwenStatus(): Promise<QwenBatchStatus> {
      const response = await transport('/api/local-remove/qwen/status', { headers: { 'x-local-remove-token': token } });
      if (!response.ok) throw new BatchApiError('Could not read Qwen availability.', response.status);
      const value = (await response.json()) as QwenBatchStatus;
      return {
        connected: value.connected === true,
        variants: Array.isArray(value.variants)
          ? value.variants.map(item => ({ id: item.id, available: item.available === true }))
          : [],
      };
    },
    previewUrl: (queueId: string, itemId: string, full = false, original = false) =>
      `${prefix}/jobs/${encode(queueId)}/items/${encode(itemId)}/preview${full ? '?full=true&original=' + original : ''}`,
    downloadUrl: (queueId: string) => `${prefix}/jobs/${encode(queueId)}/download`,
  };
}
export type BatchApi = ReturnType<typeof createBatchApi>;
