import type { EditorDocument } from '../../contracts.ts';
import type {
  GenerationModel,
  GenerationPayload,
  GenerationResult,
  HardwareUsage,
  OperationProgress,
  UpscaleInventory,
} from './types.ts';

export class GenerationApiError extends Error {
  readonly status: number;
  constructor(message: string, status: number) {
    super(message);
    this.name = 'GenerationApiError';
    this.status = status;
  }
}
export function createGenerationApi(token: string, fetcher: typeof fetch = fetch) {
  async function request<T>(path: string, method = 'GET', body?: unknown, signal?: AbortSignal): Promise<T> {
    const form = body instanceof FormData;
    const response = await fetcher('/api/local-remove' + path, {
      method,
      signal,
      headers: {
        'x-local-remove-token': token,
        ...(body !== undefined && !form ? { 'Content-Type': 'application/json' } : {}),
      },
      body: body === undefined ? undefined : form ? body : JSON.stringify(body),
    });
    let result: unknown;
    try {
      result = await response.json();
    } catch {
      throw new GenerationApiError(`Invalid generation response (${response.status}).`, response.status);
    }
    if (!response.ok)
      throw new GenerationApiError(
        result && typeof result === 'object' && 'detail' in result
          ? String(result.detail)
          : `Generation request failed (${response.status}).`,
        response.status,
      );
    return result as T;
  }
  function validDocument(document: EditorDocument) {
    if (!document || typeof document.id !== 'string' || !Number.isInteger(document.revision))
      throw new GenerationApiError('The backend returned an invalid generated document.', 502);
    return document;
  }
  return {
    models: (refresh = false, signal?: AbortSignal) =>
      request<{ models: GenerationModel[]; busy?: boolean }>(
        '/generation/models' + (refresh ? '?refresh=true' : ''),
        'GET',
        undefined,
        signal,
      ),
    upscaleModels: (signal?: AbortSignal) =>
      request<UpscaleInventory>('/generation/upscale/models', 'GET', undefined, signal),
    progress: (signal?: AbortSignal) => request<OperationProgress>('/generation/progress', 'GET', undefined, signal),
    cancel: (jobId: string) => request<OperationProgress>('/generation/cancel', 'POST', { job_id: jobId }),
    hardwareUsage: (signal?: AbortSignal) => request<HardwareUsage>('/hardware/usage', 'GET', undefined, signal),
    async generate(payload: GenerationPayload) {
      const result = await request<GenerationResult>('/generation', 'POST', payload);
      validDocument(result.session);
      return result;
    },
    async upscale(document: EditorDocument, size: { width: number; height: number }, operationId?: string) {
      const result = await request<GenerationResult>('/generation/upscale', 'POST', {
        session_id: document.id,
        revision: document.revision,
        ...size,
        ...(operationId ? { operation_id: operationId } : {}),
      });
      validDocument(result.session);
      return result;
    },
    async importReference(file: File) {
      const data = new FormData();
      data.set('file', file);
      return validDocument(await request<EditorDocument>('/import', 'POST', data));
    },
    installedLoras: (model: string) =>
      request<{ installed: { id: string; title?: string; supported?: boolean; usage?: string }[] }>(
        '/loras?model=' + encodeURIComponent(model),
      ),
  };
}
export type GenerationApi = ReturnType<typeof createGenerationApi>;
