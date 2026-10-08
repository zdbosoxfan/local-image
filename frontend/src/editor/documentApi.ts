import type { DocumentMetadata, ImageCollection, NativeOpenResult, SaveResult } from './documentContracts.ts';

export class DocumentApiError extends Error {
  readonly status: number;
  constructor(message: string, status: number) {
    super(message);
    this.name = 'DocumentApiError';
    this.status = status;
  }
}
export function readDocument(value: unknown, expectedId?: string): DocumentMetadata {
  const data = value as DocumentMetadata;
  if (
    !data ||
    typeof data.id !== 'string' ||
    (expectedId && data.id !== expectedId) ||
    !Number.isInteger(data.revision) ||
    data.revision < 0 ||
    !Number.isFinite(data.width) ||
    !Number.isFinite(data.height) ||
    data.width <= 0 ||
    data.height <= 0 ||
    !Array.isArray(data.layers)
  )
    throw new DocumentApiError('The backend returned an invalid image document.', 502);
  return data;
}
export function readCollection(value: unknown): ImageCollection {
  const data = value as ImageCollection;
  if (
    !data ||
    typeof data.id !== 'string' ||
    typeof data.name !== 'string' ||
    !Array.isArray(data.entries) ||
    data.entries.some(entry => typeof entry.id !== 'string' || typeof entry.name !== 'string')
  )
    throw new DocumentApiError('The backend returned an invalid image collection.', 502);
  return data;
}
export function createDocumentApi(token: string, request: typeof fetch = fetch) {
  const prefix = '/api/local-remove',
    revisions = new Map<string, number>(),
    queues = new Map<string, Promise<unknown>>(),
    failureEpochs = new Map<string, number>();
  const id = encodeURIComponent;
  async function json<T>(path: string, options: RequestInit = {}): Promise<T> {
    const response = await request(prefix + path, {
      ...options,
      credentials: 'same-origin',
      headers: { 'x-local-remove-token': token, ...options.headers },
    });
    let data: unknown;
    try {
      data = await response.json();
    } catch {
      throw new DocumentApiError(`The backend response could not be read (${response.status}).`, response.status);
    }
    if (!response.ok) {
      const detail = data && typeof data === 'object' && 'detail' in data ? data.detail : null;
      throw new DocumentApiError(
        typeof detail === 'string' ? detail : `Request failed (${response.status}).`,
        response.status,
      );
    }
    return data as T;
  }
  const post = <T>(path: string, body: unknown, method = 'POST') =>
    json<T>(path, { method, headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(body) });
  function observe(document: DocumentMetadata) {
    revisions.set(document.id, Math.max(revisions.get(document.id) ?? -1, document.revision));
  }
  function cloneBody(body: Record<string, unknown> | FormData) {
    if (!(body instanceof FormData)) return structuredClone(body);
    const copy = new FormData();
    for (const [key, value] of body.entries()) copy.append(key, value);
    return copy;
  }
  function mutate<T = DocumentMetadata>(
    document: Pick<DocumentMetadata, 'id' | 'revision'>,
    tail: string,
    body: Record<string, unknown> | FormData = {},
    method: 'POST' | 'PATCH' = 'POST',
  ): Promise<T> {
    const sid = document.id,
      initialRevision = document.revision,
      epoch = failureEpochs.get(sid) ?? 0,
      captured = cloneBody(body);
    if (
      !/^\/(?:stack(?:\/layers|\/layer\/[^/]+|\/generated-layer|\/undo|\/redo)?|merge|layer\/[^/]+|remove|save|export-project|cutout(?:\/refine|\/background|\/white-background|\/generated-background|\/library-background|\/generate-background|\/undo|\/redo)?)$/.test(
        tail,
      )
    )
      return Promise.reject(new DocumentApiError('Unsupported document operation.', 400));
    const run = async () => {
      if ((failureEpochs.get(sid) ?? 0) !== epoch)
        throw new DocumentApiError(
          'A previous document change failed. Review the accepted image before trying another change.',
          409,
        );
      const revision = Math.max(revisions.get(sid) ?? initialRevision, initialRevision);
      let value: unknown;
      if (captured instanceof FormData) {
        captured.set('revision', String(revision));
        value = await json('/session/' + id(sid) + tail, { method, body: captured });
      } else value = await post('/session/' + id(sid) + tail, { ...captured, revision }, method);
      const candidate =
        value && typeof value === 'object' && 'session' in value ? (value as SaveResult).session : value;
      if (candidate) {
        const accepted = readDocument(candidate, sid);
        if (accepted.revision < revision || accepted.revision < (revisions.get(sid) ?? -1))
          throw new DocumentApiError(
            'The document changed before this response arrived. The stale result was ignored.',
            409,
          );
        observe(accepted);
      }
      return value as T;
    };
    const pending = (queues.get(sid) ?? Promise.resolve())
      .catch(() => undefined)
      .then(run)
      .catch(error => {
        if ((failureEpochs.get(sid) ?? 0) === epoch) failureEpochs.set(sid, epoch + 1);
        throw error;
      });
    queues.set(sid, pending);
    void pending
      .finally(() => {
        if (queues.get(sid) === pending) queues.delete(sid);
      })
      .catch(() => undefined);
    return pending;
  }
  return {
    observe,
    mutate,
    async flush(sid?: string) {
      await Promise.allSettled(sid ? [queues.get(sid)].filter(Boolean) : [...queues.values()]);
    },
    pending: (sid?: string) => (sid ? queues.has(sid) : queues.size > 0),
    async session(sid: string) {
      return readDocument(await json('/session/' + id(sid)), sid);
    },
    async sessions() {
      const values = await json<unknown[]>('/sessions');
      if (!Array.isArray(values)) throw new DocumentApiError('Could not read recent images.', 502);
      return values.map(value => readDocument(value));
    },
    async collection(cid: string) {
      return readCollection(await json('/collection/' + id(cid)));
    },
    async openEntry(cid: string, eid: string) {
      const value = await post<NativeOpenResult>('/collection/' + id(cid) + '/entry/' + id(eid) + '/open', {});
      if (value.session) readDocument(value.session);
      if (value.collection) readCollection(value.collection);
      return value;
    },
    async importImage(file: File) {
      const form = new FormData();
      form.append('file', file);
      return readDocument(await json('/import', { method: 'POST', body: form }));
    },
    async importProject(file: File) {
      const form = new FormData();
      form.append('file', file);
      const value = await json<NativeOpenResult>('/import-project', { method: 'POST', body: form });
      readDocument(value.session);
      return value;
    },
    async close(sessions: Array<{ id: string; revision: number }>) {
      const value = await post<{ closed: string[] }>('/close-sessions', { sessions, discard: true });
      if (
        !Array.isArray(value.closed) ||
        value.closed.length !== sessions.length ||
        sessions.some(item => !value.closed.includes(item.id))
      )
        throw new DocumentApiError('The backend did not confirm every requested document close.', 502);
      return value;
    },
    settings: () =>
      json<{
        model?: string;
        models: Array<{ id: string; label?: string; available?: boolean; reason?: string; [key: string]: unknown }>;
      }>('/settings'),
    status: () => json<{ ready?: boolean; retouch_ready?: boolean; device?: string }>('/status'),
    qwen: () => json<Record<string, unknown>>('/qwen/status'),
  };
}
export type DocumentApi = ReturnType<typeof createDocumentApi>;
