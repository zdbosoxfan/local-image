import type { EditorDocument } from '../../contracts.ts';
import type {
  AssetsContext,
  BackgroundLibrary,
  DeleteResult,
  DeleteSelection,
  GeneratedLibrary,
  StockPage,
  StockProvider,
  StockProviderId,
} from './types.ts';

export class AssetsApiError extends Error {
  readonly status: number;
  constructor(message: string, status: number) {
    super(message);
    this.name = 'AssetsApiError';
    this.status = status;
  }
}
export function safeCreditUrl(value: string | undefined): string | undefined {
  try {
    const url = new URL(value ?? '');
    return ['https:', 'http:'].includes(url.protocol) ? url.href : undefined;
  } catch {
    return undefined;
  }
}
export function createAssetsApi(token: string, fetcher: typeof fetch = fetch) {
  async function request<T>(path: string, method = 'GET', body?: unknown, signal?: AbortSignal): Promise<T> {
    const form = body instanceof FormData;
    const response = await fetcher('/api/local-remove' + path, {
      method,
      signal,
      headers: {
        'x-local-remove-token': token,
        ...(!form && body !== undefined ? { 'Content-Type': 'application/json' } : {}),
      },
      body: body === undefined ? undefined : form ? body : JSON.stringify(body),
    });
    let result: unknown;
    try {
      result = await response.json();
    } catch {
      throw new AssetsApiError(`Invalid asset response (${response.status}).`, response.status);
    }
    if (!response.ok)
      throw new AssetsApiError(
        result && typeof result === 'object' && 'detail' in result
          ? String(result.detail)
          : `Asset request failed (${response.status}).`,
        response.status,
      );
    return result as T;
  }
  function document(result: EditorDocument, expected?: AssetsContext): EditorDocument {
    if (
      !result ||
      typeof result.id !== 'string' ||
      !Number.isInteger(result.revision) ||
      (expected && (result.id !== expected.documentId || result.revision < expected.revision))
    )
      throw new AssetsApiError('The document response is stale or belongs to another image.', 409);
    return result;
  }
  function target(context: AssetsContext) {
    if (!context.documentId) throw new AssetsApiError('Open an image before adding a background.', 400);
    return '/session/' + encodeURIComponent(context.documentId);
  }
  return {
    providers: (signal?: AbortSignal) =>
      request<{ providers: StockProvider[]; default_provider: StockProviderId }>(
        '/stock/providers',
        'GET',
        undefined,
        signal,
      ),
    search: (provider: StockProviderId, query: string, page: number, signal?: AbortSignal) =>
      request<StockPage>(
        '/stock/search?' + new URLSearchParams({ provider, query, page: String(page) }),
        'GET',
        undefined,
        signal,
      ),
    connect: (provider: 'pexels' | 'unsplash', key: string) =>
      request<{ connected: boolean }>('/stock/connection/' + provider, 'PUT', { key }),
    folders: (signal?: AbortSignal) =>
      request<{ libraries: BackgroundLibrary[] }>('/backgrounds', 'GET', undefined, signal),
    generated: (signal?: AbortSignal) => request<GeneratedLibrary>('/generation/library', 'GET', undefined, signal),
    deleteCopies: (selection: DeleteSelection) =>
      request<DeleteResult>('/generation/library/delete', 'POST', selection),
    async openGenerated(id: string) {
      const value = await request<{ session: EditorDocument }>(
        '/generation/library/' + encodeURIComponent(id) + '/open',
        'POST',
        {},
      );
      return document(value.session);
    },
    async importStock(id: string, background?: AssetsContext) {
      if (background) target(background);
      const body = background
        ? {
            id,
            target: 'background',
            session_id: background.documentId,
            revision: background.revision,
            layer_id: background.layerId,
          }
        : { id, target: 'image' };
      const value = await request<{ session: EditorDocument }>('/stock/import', 'POST', body);
      return document(value.session, background);
    },
    async applyFolder(context: AssetsContext, library: string, entry: string) {
      return document(
        await request<EditorDocument>(target(context) + '/cutout/library-background', 'POST', {
          revision: context.revision,
          library_id: library,
          entry_id: entry,
          layer_id: context.layerId,
        }),
        context,
      );
    },
    async applyFile(context: AssetsContext, file: File) {
      const body = new FormData();
      body.set('revision', String(context.revision));
      body.set('file', file);
      if (context.layerId) body.set('layer_id', context.layerId);
      return document(await request<EditorDocument>(target(context) + '/cutout/background', 'POST', body), context);
    },
  };
}
export type AssetsApi = ReturnType<typeof createAssetsApi>;
