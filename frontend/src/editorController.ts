import type { EditorDocument, EditorSnapshot, LegacyEditor } from './contracts.ts';
import { createEditorApi } from './editorApi.ts';

function freeze<T>(value: T): T {
  if (value && typeof value === 'object' && !Object.isFrozen(value)) {
    Object.freeze(value);
    for (const child of Object.values(value)) freeze(child);
  }
  return value;
}

/** Transitional boundary. Browser camera/masks remain in the persistent canvas
 * controller; only coarse, accepted metadata is published to React. Python owns
 * the document and bounded stack history. Components never serialize projects. */
export function createEditorController(legacy: LegacyEditor, token: string) {
  const api = createEditorApi(token);
  const documents = new Map<string, EditorDocument>();
  const listeners = new Set<() => void>();
  let snapshot: EditorSnapshot;
  let signature = '';
  function accept() {
    const raw = legacy.getSnapshot();
    let incoming = raw.document;
    if (incoming) {
      const prior = documents.get(incoming.id);
      if (prior && incoming.revision < prior.revision) incoming = prior;
      api.observe(incoming);
    }
    const accepted = { ...raw, document: incoming };
    const nextSignature = JSON.stringify(accepted);
    if (nextSignature === signature) return;
    signature = nextSignature;
    snapshot = freeze(structuredClone(accepted));
    if (snapshot.document) documents.set(snapshot.document.id, snapshot.document);
    listeners.forEach(listener => listener());
  }
  accept();
  legacy.setStackTransport(api.mutate);
  const unsubscribe = legacy.subscribe(accept);
  return {
    commands: legacy.commands,
    runDocumentOperation: api.runDocumentOperation,
    getSnapshot: () => snapshot,
    subscribe(listener: () => void) { listeners.add(listener); return () => { listeners.delete(listener); }; },
    dispose() { unsubscribe(); listeners.clear(); },
  };
}
export type EditorController = ReturnType<typeof createEditorController>;
