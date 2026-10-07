import { createAssetsApi } from './api.ts';
import type { AssetsApi } from './api.ts';
import type { AssetTab, AssetsHost, AssetsState, BackgroundLibrary, StockProviderId } from './types.ts';

const emptyGenerated = { items: [], count: 0, bytes: 0 };
function freeze<T>(value: T): T {
  if (value && typeof value === 'object' && !Object.isFrozen(value)) {
    for (const child of Object.values(value)) freeze(child);
    Object.freeze(value);
  }
  return value;
}
export function createAssetsController(host: AssetsHost, token: string, suppliedApi?: AssetsApi) {
  const api = suppliedApi ?? createAssetsApi(token),
    listeners = new Set<() => void>();
  let state: AssetsState = freeze({
    open: false,
    expanded: false,
    tab: 'stock',
    context: { ...host.getContext() },
    loading: false,
    working: false,
    status: '',
    error: null,
    providers: [],
    provider: 'openverse',
    query: '',
    stock: { results: [], page: 0, next_page: null },
    stockSelected: null,
    folders: [],
    folderSelected: '',
    folderEntrySelected: null,
    generated: emptyGenerated,
    generatedQuery: '',
    generatedSelected: null,
    selectionMode: false,
    selectedCopies: [],
    deleteSelection: null,
  });
  let readEpoch = 0,
    readAbort: AbortController | null = null,
    disposed = false;
  const browserFiles = new Map<string, File>();
  let objectUrls: string[] = [];
  function update(change: Partial<AssetsState>) {
    if (disposed) return;
    state = freeze({ ...state, ...change });
    listeners.forEach(listener => listener());
  }
  const detach = host.subscribeContext(() => update({ context: { ...host.getContext() } }));
  function cancelRead() {
    readEpoch++;
    readAbort?.abort();
    readAbort = null;
  }
  async function read(work: (signal: AbortSignal) => Promise<Partial<AssetsState>>) {
    cancelRead();
    const epoch = readEpoch;
    readAbort = new AbortController();
    update({ loading: true, error: null });
    try {
      const change = await work(readAbort.signal);
      if (epoch === readEpoch) update(change);
    } catch (error) {
      if (epoch === readEpoch && !(error instanceof DOMException && error.name === 'AbortError'))
        update({ error: error instanceof Error ? error.message : String(error) });
    } finally {
      if (epoch === readEpoch) update({ loading: false });
    }
  }
  async function write(work: () => Promise<void>) {
    if (state.working || host.getContext().busy) return;
    cancelRead();
    update({ working: true, loading: false, error: null, status: '' });
    try {
      await work();
    } catch (error) {
      update({ error: error instanceof Error ? error.message : String(error) });
    } finally {
      update({ working: false, context: { ...host.getContext() } });
    }
  }
  async function refresh() {
    if (state.working) return;
    const tab = state.tab;
    await read(async signal => {
      if (tab === 'stock') {
        const value = await api.providers(signal);
        return {
          providers: value.providers,
          provider: value.providers.some(item => item.id === state.provider) ? state.provider : value.default_provider,
        };
      }
      if (tab === 'folders') {
        const value = await api.folders(signal);
        const browser = state.folders.filter(item => item.browser);
        const folders = [...value.libraries, ...browser];
        return {
          folders,
          folderSelected: folders.some(item => item.id === state.folderSelected)
            ? state.folderSelected
            : (folders[0]?.id ?? ''),
          folderEntrySelected: null,
        };
      }
      const generated = await api.generated(signal),
        ids = new Set(generated.items.map(item => item.id));
      return {
        generated,
        generatedSelected: ids.has(state.generatedSelected ?? '') ? state.generatedSelected : null,
        selectedCopies: state.selectedCopies.filter(id => ids.has(id)),
        status: generated.warning ?? '',
      };
    });
  }
  async function open(tab: AssetTab = state.tab) {
    if (state.working) return;
    update({ open: true, tab, deleteSelection: null });
    host.setDockVisible(true);
    await refresh();
  }
  function close() {
    if (state.working) return;
    cancelRead();
    update({ open: false, expanded: false, loading: false });
    host.setDockVisible(false);
  }
  async function search(page = 0) {
    const provider = state.provider,
      query = state.query.trim();
    if (!query || !state.providers.some(item => item.id === provider && item.available) || state.working) return;
    await read(async signal => {
      const stock = await api.search(provider, query, page, signal);
      return {
        stock,
        stockSelected: stock.results[0]?.id ?? null,
        status: stock.warning ?? `${stock.results.length} images`,
      };
    });
  }
  function canHandoff(context: ReturnType<AssetsHost['getContext']>) {
    const now = host.getContext();
    return context.navigationEpoch === now.navigationEpoch && context.documentId === now.documentId;
  }
  async function importStock(destination: 'image' | 'background' | 'reference') {
    const id = state.stockSelected;
    if (!id) return;
    await write(() =>
      host.runDocumentChange(async context => {
        if (destination === 'reference' && !context.canReference)
          throw new Error('The selected model cannot accept another reference image.');
        const document = await api.importStock(id, destination === 'background' ? context : undefined);
        const accepted =
          canHandoff(context) &&
          (destination === 'reference'
            ? await host.addReference(document, context)
            : await host.acceptDocument(document, context, destination));
        update({
          status: accepted
            ? 'Stock image imported with its source credit.'
            : 'Import completed; the image you switched to was left unchanged.',
        });
      }),
    );
  }
  async function openGenerated(destination: 'image' | 'draft' | 'reference') {
    const id = state.selectionMode
      ? state.selectedCopies.length === 1
        ? state.selectedCopies[0]
        : null
      : state.generatedSelected;
    if (!id) return;
    await write(async () => {
      const original = { ...host.getContext() };
      if (destination === 'reference' && !original.canReference)
        throw new Error('The selected model cannot accept another reference image.');
      if (destination === 'draft' && !original.canUseDraft)
        throw new Error('Finish the active operation before choosing a refinement draft.');
      // Opening the library makes an independent backend copy, not a write to
      // the active document. Only its subsequent handoff needs the shared queue.
      const document = await api.openGenerated(id);
      const accepted =
        canHandoff(original) &&
        (await host.runDocumentChange(async context => {
          if (
            !canHandoff(original) ||
            ((destination === 'draft' || destination === 'reference') &&
              original.referenceTarget !== context.referenceTarget)
          )
            return false;
          return destination === 'draft'
            ? host.useAsDraft(document, context)
            : destination === 'reference'
              ? host.addReference(document, context)
              : host.acceptDocument(document, context, 'generated');
        }));
      update({
        status: accepted
          ? 'Opened a new working copy. The library copy is retained.'
          : 'A working copy was created; the image you switched to was left unchanged.',
      });
    });
  }
  async function attachFolder() {
    await write(async () => {
      const library = await host.chooseBackgroundFolder();
      if (!library) {
        update({ status: 'Folder selection cancelled.' });
        return;
      }
      update({
        folders: [...state.folders.filter(item => item.id !== library.id), library],
        folderSelected: library.id,
        folderEntrySelected: null,
        status: `${library.entries.length} background images`,
      });
    });
  }
  function attachFiles(files: File[]) {
    const supported = files.filter(file => /\.(?:jpe?g|png|tiff?|webp)$/i.test(file.name));
    if (!supported.length) {
      update({ error: 'This folder has no supported background images.' });
      return;
    }
    if (supported.length > 1000) {
      update({ error: 'Choose a folder with at most 1,000 supported images.' });
      return;
    }
    objectUrls.forEach(url => URL.revokeObjectURL(url));
    objectUrls = [];
    browserFiles.clear();
    supported.sort((a, b) => a.name.localeCompare(b.name, undefined, { numeric: true }));
    const library: BackgroundLibrary = {
      id: 'browser-files',
      name: supported[0]?.webkitRelativePath?.split('/')[0] || 'Selected folder',
      browser: true,
      entries: supported.map((file, index) => {
        const id = String(index),
          thumbnail = URL.createObjectURL(file);
        objectUrls.push(thumbnail);
        browserFiles.set(id, file);
        return { id, name: file.name, thumbnail };
      }),
    };
    update({
      folders: [...state.folders.filter(item => !item.browser), library],
      folderSelected: library.id,
      folderEntrySelected: null,
      error: null,
      status: `${supported.length} background images`,
    });
  }
  async function applyFolder() {
    const library = state.folders.find(item => item.id === state.folderSelected),
      entry = state.folderEntrySelected;
    if (!library || !entry) return;
    await write(() =>
      host.runDocumentChange(async context => {
        const file = library.browser ? browserFiles.get(entry) : undefined;
        if (library.browser && !file) throw new Error('Choose this folder again to access its files.');
        const document = file ? await api.applyFile(context, file) : await api.applyFolder(context, library.id, entry);
        const accepted = canHandoff(context) && (await host.acceptDocument(document, context, 'background'));
        update({
          status: accepted
            ? 'Background added to the document.'
            : 'Background added to its original document; your current image is unchanged.',
        });
      }),
    );
  }
  const visibleGenerated = () =>
    state.generated.items.filter(
      item =>
        !state.generatedQuery.trim() ||
        [item.name, item.model, item.generation?.prompt]
          .join(' ')
          .toLowerCase()
          .includes(state.generatedQuery.trim().toLowerCase()),
    );
  return {
    getSnapshot: () => state,
    subscribe(listener: () => void) {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
    open,
    close,
    refresh,
    search,
    importStock,
    openGenerated,
    attachFolder,
    attachFiles,
    applyFolder,
    setExpanded(value: boolean) {
      update({ expanded: value });
    },
    setQuery(query: string) {
      cancelRead();
      update({ query, loading: false });
    },
    setProvider(provider: StockProviderId) {
      cancelRead();
      update({
        provider,
        stock: { results: [], page: 0, next_page: null },
        stockSelected: null,
        loading: false,
        error: null,
      });
    },
    selectStock(id: string) {
      if (state.stock.results.some(item => item.id === id)) update({ stockSelected: id });
    },
    selectFolder(id: string) {
      update({ folderSelected: id, folderEntrySelected: null });
    },
    selectFolderEntry(id: string) {
      update({ folderEntrySelected: id });
    },
    async connectProvider(key: string, disconnect = false) {
      const provider = state.provider;
      if (provider === 'openverse') return;
      await write(async () => {
        const result = await api.connect(provider, disconnect ? '' : key.trim());
        const value = await api.providers();
        update({
          providers: value.providers,
          status: disconnect
            ? result.connected
              ? 'An environment key still supplies this connection.'
              : 'Disconnected.'
            : 'Connection saved for your account on this computer.',
        });
      });
    },
    setGeneratedQuery(generatedQuery: string) {
      const selected = state.generated.items.find(item => item.id === state.generatedSelected),
        query = generatedQuery.trim().toLowerCase();
      const matches =
        selected &&
        (!query ||
          [selected.name, selected.model, selected.generation?.prompt].join(' ').toLowerCase().includes(query));
      update({ generatedQuery, generatedSelected: matches ? state.generatedSelected : null, deleteSelection: null });
    },
    visibleGenerated,
    setSelectionMode(selectionMode: boolean) {
      update({ selectionMode, selectedCopies: [], deleteSelection: null });
    },
    selectGenerated(id: string) {
      if (!state.generated.items.some(item => item.id === id)) return;
      update(
        state.selectionMode
          ? {
              selectedCopies: state.selectedCopies.includes(id)
                ? state.selectedCopies.filter(item => item !== id)
                : [...state.selectedCopies, id],
              deleteSelection: null,
            }
          : { generatedSelected: id, deleteSelection: null },
      );
    },
    selectVisibleCopies(value: boolean) {
      const ids = new Set(visibleGenerated().map(item => item.id));
      update({
        selectedCopies: value
          ? [...new Set([...state.selectedCopies, ...ids])]
          : state.selectedCopies.filter(id => !ids.has(id)),
        deleteSelection: null,
      });
    },
    confirmDelete(all = false) {
      if (state.working || (all ? !state.generated.count : !state.selectedCopies.length)) return;
      update({ deleteSelection: all ? { all: true } : { ids: [...state.selectedCopies] } });
    },
    cancelDelete() {
      update({ deleteSelection: null });
    },
    async deleteCopies() {
      const selection = state.deleteSelection;
      if (!selection) return;
      await write(async () => {
        const generated = await api.deleteCopies(selection);
        const deleted = new Set(generated.deleted);
        update({
          generated,
          selectedCopies: state.selectedCopies.filter(id => !deleted.has(id)),
          generatedSelected: deleted.has(state.generatedSelected ?? '') ? null : state.generatedSelected,
          deleteSelection: null,
          status: `Deleted ${generated.deleted.length} library copies. Open documents and saved files are retained.`,
        });
      });
    },
    dispose() {
      disposed = true;
      cancelRead();
      detach();
      listeners.clear();
      objectUrls.forEach(url => URL.revokeObjectURL(url));
      browserFiles.clear();
    },
  };
}
export type AssetsController = ReturnType<typeof createAssetsController>;
