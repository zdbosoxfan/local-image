export type OverwriteChoice = 'overwrite' | 'unique' | null;
export type CloseChoice = 'save' | 'discard' | null;
export interface ClosePlan {
  all: boolean;
  dirtyCount: number;
  names: readonly string[];
  pendingSelection: boolean;
  nativeProjects: boolean;
  warning: string;
}
export interface Credit {
  label: string;
  title?: string;
  creator?: string;
  attribution?: string;
  source_url?: string;
  license?: string;
  license_url?: string;
}
export type EditorDialog =
  | { kind: 'none' }
  | { kind: 'overwrite'; id: number; filename: string; dontAsk: boolean }
  | { kind: 'close'; id: number; plan: ClosePlan }
  | { kind: 'credits'; credits: readonly Credit[]; text: string; status: string; error: boolean };

function immutable<T>(value: T): T {
  if (value && typeof value === 'object' && !Object.isFrozen(value)) {
    Object.freeze(value);
    for (const child of Object.values(value)) immutable(child);
  }
  return value;
}
/** Resolves only a UI decision. Saving, native picker completion and document
 * destruction remain explicit editor operations after this decision returns. */
export function createEditorDialogs(ports: {
  dontAskBeforeOverwrite(): void;
  focusCanvas(): void;
  copy(text: string): Promise<void>;
  downloadCredits(): unknown;
}) {
  let state: EditorDialog = immutable({ kind: 'none' }),
    sequence = 0;
  let pending: { id: number; kind: 'close' | 'overwrite'; resolve(value: CloseChoice | OverwriteChoice): void } | null =
    null;
  const listeners = new Set<() => void>();
  const publish = (next: EditorDialog) => {
    state = immutable(structuredClone(next));
    for (const listener of listeners) listener();
  };
  function ask<T extends CloseChoice | OverwriteChoice>(
    next: EditorDialog & { id: number; kind: 'close' | 'overwrite' },
  ): Promise<T> {
    if (pending || state.kind !== 'none')
      return Promise.reject(Error('Finish the current dialog before starting another.'));
    return new Promise<T>(resolve => {
      pending = { id: next.id, kind: next.kind, resolve: value => resolve(value as T) };
      publish(next);
    });
  }
  function respond(id: number, choice: CloseChoice | OverwriteChoice) {
    const current = pending;
    if (!current || current.id !== id) return;
    if (current.kind === 'overwrite' && choice !== null && choice !== 'overwrite' && choice !== 'unique') return;
    if (current.kind === 'close' && choice !== null && choice !== 'save' && choice !== 'discard') return;
    const dontAsk = state.kind === 'overwrite' && state.dontAsk && choice === 'overwrite';
    pending = null;
    publish({ kind: 'none' });
    if (dontAsk) ports.dontAskBeforeOverwrite();
    ports.focusCanvas();
    current.resolve(choice);
  }
  function close() {
    if (pending) respond(pending.id, null);
    else {
      publish({ kind: 'none' });
      ports.focusCanvas();
    }
  }
  return {
    getSnapshot: () => state,
    subscribe(listener: () => void) {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
    isOpen: () => state.kind !== 'none',
    confirmOverwrite: (filename: string) =>
      ask<OverwriteChoice>({ kind: 'overwrite', id: ++sequence, filename, dontAsk: false }),
    confirmClose: (plan: ClosePlan) => ask<CloseChoice>({ kind: 'close', id: ++sequence, plan }),
    respond,
    close,
    setDontAsk(dontAsk: boolean) {
      if (state.kind === 'overwrite') publish({ ...state, dontAsk });
    },
    showCredits(credits: readonly Credit[], text: string) {
      if (state.kind === 'none') publish({ kind: 'credits', credits, text, status: '', error: false });
    },
    async copyCredits() {
      if (state.kind !== 'credits') return;
      const current = state;
      try {
        await ports.copy(current.text);
        if (state === current) publish({ ...current, status: 'Credits copied.', error: false });
      } catch {
        if (state === current)
          publish({ ...current, status: 'Clipboard unavailable. Use Save credits to keep the text.', error: true });
      }
    },
    downloadCredits() {
      if (state.kind === 'credits') return ports.downloadCredits();
    },
    dispose() {
      close();
      listeners.clear();
    },
  };
}
export type EditorDialogsController = ReturnType<typeof createEditorDialogs>;
