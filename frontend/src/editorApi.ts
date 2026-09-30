import type { EditorDocument, StackRequest } from './contracts.ts';

export class EditorApiError extends Error {
  readonly status: number;
  constructor(message: string, status: number) { super(message); this.name = 'EditorApiError'; this.status = status; }
}

/** A failed write invalidates already queued writes. A later deliberate command
 * can try again; nothing automatically retries a mutation or inference request. */
export function createEditorApi(token: string, request: typeof fetch = fetch) {
  const accepted = new Map<string, number>();
  const queues = new Map<string, Promise<unknown>>();
  const epochs = new Map<string, number>();
  function observe(document: EditorDocument | null) {
    if (document) accepted.set(document.id, Math.max(accepted.get(document.id) ?? -1, document.revision));
  }
  function mutate(input: StackRequest): Promise<EditorDocument> {
    const id = input.documentId;
    const epoch = epochs.get(id) ?? 0;
    // Copy UI drafts at dispatch, never use a subsequently edited input object.
    const body = structuredClone(input.body);
    const run = async () => {
      if ((epochs.get(id) ?? 0) !== epoch) throw new EditorApiError('A previous layer change failed. Review the document before trying again.', 409);
      if (!/^\/(?:stack(?:\/layers|\/layer\/[^/]+|\/undo|\/redo)?|merge)$/.test(input.tail) || !['POST', 'PATCH'].includes(input.method)) {
        throw new EditorApiError('Unsupported layer operation.', 400);
      }
      const revision = Math.max(accepted.get(id) ?? input.revision, input.revision);
      const response = await request(`/api/local-remove/session/${encodeURIComponent(id)}${input.tail}`, {
        method: input.method,
        headers: { 'Content-Type': 'application/json', 'x-local-remove-token': token },
        body: JSON.stringify({ ...body, revision }),
      });
      let data: unknown;
      try { data = await response.json(); } catch { throw new EditorApiError(`Invalid response (${response.status}).`, response.status); }
      if (!response.ok) {
        const detail = data && typeof data === 'object' && 'detail' in data ? String(data.detail) : `Request failed (${response.status}).`;
        throw new EditorApiError(detail, response.status);
      }
      const document = data as EditorDocument;
      if (!document || document.id !== id || !Number.isInteger(document.revision) || document.revision < revision || document.revision < (accepted.get(id) ?? -1)) {
        throw new EditorApiError('The document changed before this layer response arrived. The stale result was ignored.', 409);
      }
      observe(document);
      return document;
    };
    const pending = (queues.get(id) ?? Promise.resolve()).catch(() => undefined).then(run).catch(error => {
      if ((epochs.get(id) ?? 0) === epoch) epochs.set(id, epoch + 1);
      throw error;
    });
    queues.set(id, pending);
    // The caller receives the original rejection; cleanup cannot cause another.
    void pending.finally(() => { if (queues.get(id) === pending) queues.delete(id); }).catch(() => undefined);
    return pending;
  }
  return { observe, mutate };
}
