import type { DocumentBrowserPort } from './documentContracts.ts';

/** Owned browser pickers are created for the request and removed on its
 * explicit change/cancel event. There is deliberately no elapsed-time limit. */
export function createBrowserPorts(doc: Document = document): DocumentBrowserPort & { dispose(): void } {
  let pending: { input: HTMLInputElement; cancel(): void } | null = null;
  return {
    chooseFiles(kind) {
      if (pending) return Promise.reject(Error('Finish the current file picker first.'));
      return new Promise((resolve, reject) => {
        const input = doc.createElement('input');
        input.type = 'file';
        input.hidden = true;
        input.accept = kind === 'project' ? '.lremove' : '.jpg,.jpeg,.png,.tif,.tiff,.webp';
        input.multiple = kind === 'images' || kind === 'folder';
        if (kind === 'folder') {
          input.setAttribute('webkitdirectory', '');
          input.setAttribute('directory', '');
        }
        let settled = false;
        const finish = (files: readonly File[] | null) => {
          if (settled) return;
          settled = true;
          input.remove();
          pending = null;
          resolve(files?.length ? files : null);
        };
        input.addEventListener('change', () => finish(Array.from(input.files ?? [])), { once: true });
        input.addEventListener('cancel', () => finish(null), { once: true });
        pending = { input, cancel: () => finish(null) };
        doc.body.append(input);
        try {
          input.click();
        } catch (error) {
          settled = true;
          input.remove();
          pending = null;
          reject(error);
        }
      });
    },
    download(url, name) {
      const target = new URL(url, doc.location.href);
      if (target.origin !== doc.location.origin || !target.pathname.startsWith('/api/local-remove/'))
        throw Error('Unexpected download destination.');
      const link = doc.createElement('a');
      link.href = target.href;
      link.download = name;
      link.hidden = true;
      doc.body.append(link);
      link.click();
      link.remove();
    },
    replaceUrl: url => doc.defaultView?.history.replaceState({}, '', url),
    createObjectURL: file => URL.createObjectURL(file),
    revokeObjectURL: url => URL.revokeObjectURL(url),
    dispose() {
      pending?.cancel();
    },
  };
}

/** Chromium directory readers return batches; a single read silently drops
 * files in larger folders. Keep traversal private until every batch is read. */
export async function collectDroppedFiles(entries: readonly FileSystemEntry[]): Promise<File[]> {
  const files: File[] = [];
  async function visit(entry: FileSystemEntry): Promise<void> {
    if (entry.isFile) {
      const file = await new Promise<File>((resolve, reject) => (entry as FileSystemFileEntry).file(resolve, reject));
      if (/\.(jpg|jpeg|png|tif|tiff|webp)$/i.test(file.name)) files.push(file);
      return;
    }
    if (!entry.isDirectory) return;
    const reader = (entry as FileSystemDirectoryEntry).createReader();
    for (;;) {
      const batch = await new Promise<FileSystemEntry[]>((resolve, reject) => reader.readEntries(resolve, reject));
      if (!batch.length) break;
      for (const child of batch) await visit(child);
    }
  }
  for (const entry of entries) await visit(entry);
  return files;
}

export function installFileDrop(
  element: HTMLElement,
  notice: HTMLElement,
  ports: {
    allowed(): boolean;
    nativeReady(): boolean;
    navigationIdentity(): string;
    drop(files: readonly File[]): unknown;
    report(error: unknown): void;
  },
) {
  let depth = 0;
  const containsFiles = (event: DragEvent) => Array.from(event.dataTransfer?.types ?? []).includes('Files');
  const enter = (event: DragEvent) => {
    if (!containsFiles(event)) return;
    event.preventDefault();
    if (ports.allowed()) {
      depth++;
      notice.hidden = false;
    }
  };
  const over = (event: DragEvent) => {
    if (!containsFiles(event)) return;
    event.preventDefault();
    if (event.dataTransfer) event.dataTransfer.dropEffect = ports.allowed() ? 'copy' : 'none';
  };
  const leave = () => {
    depth = Math.max(0, depth - 1);
    if (!depth) notice.hidden = true;
  };
  const drop = (event: DragEvent) => {
    if (!containsFiles(event) && !event.dataTransfer?.files.length) return;
    event.preventDefault();
    depth = 0;
    notice.hidden = true;
    if (!ports.allowed()) return;
    const files = Array.from(event.dataTransfer?.files ?? []),
      identity = ports.navigationIdentity();
    const entries = ports.nativeReady()
      ? []
      : Array.from(event.dataTransfer?.items ?? [])
          .map(item => item.webkitGetAsEntry?.())
          .filter((entry): entry is FileSystemEntry => !!entry);
    if (entries.some(entry => entry.isDirectory)) {
      void collectDroppedFiles(entries)
        .then(found => {
          if (!ports.allowed() || ports.navigationIdentity() !== identity)
            throw Error('The active image changed while reading the dropped folder. Drop it again to open it.');
          return ports.drop(found);
        })
        .catch(ports.report);
    } else if (files.length) {
      try {
        void Promise.resolve(ports.drop(files)).catch(ports.report);
      } catch (error) {
        ports.report(error);
      }
    }
  };
  element.addEventListener('dragenter', enter);
  element.addEventListener('dragover', over);
  element.addEventListener('dragleave', leave);
  element.addEventListener('drop', drop);
  return () => {
    element.removeEventListener('dragenter', enter);
    element.removeEventListener('dragover', over);
    element.removeEventListener('dragleave', leave);
    element.removeEventListener('drop', drop);
    notice.hidden = true;
  };
}
