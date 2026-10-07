import type { Attribution, CollectionEntry, DocumentMetadata, ImageCollection } from './documentContracts.ts';
import type { Credit } from '../features/shell/editorDialogs.ts';

export function deepFreeze<T>(value: T): T {
  if (value && typeof value === 'object' && !Object.isFrozen(value)) {
    Object.freeze(value);
    for (const item of Object.values(value)) deepFreeze(item);
  }
  return value;
}
export function hasWorkingLayers(data: DocumentMetadata) {
  return (
    !!data.layers.length ||
    !!data.cutout ||
    !!data.generation ||
    !!data.upscale ||
    !!data.source_attribution ||
    !!data.layer_stack?.some(
      layer =>
        layer.kind !== 'original' ||
        !layer.visible ||
        layer.discarded ||
        layer.opacity !== 1 ||
        layer.transform.offset_x !== 0 ||
        layer.transform.offset_y !== 0 ||
        layer.transform.scale !== 1 ||
        layer.transform.rotation !== 0,
    )
  );
}
export const projectNeedsSave = (data: DocumentMetadata) =>
  !data.project_saved || data.project_saved_revision !== data.revision || data.project_dirty === true;
export const imageDirty = (data: DocumentMetadata) =>
  typeof data.dirty === 'boolean' ? data.dirty : data.revision !== (data.saved_revision ?? 0);
export function withCollectionDocument(
  collection: ImageCollection | null,
  data: DocumentMetadata,
): ImageCollection | null {
  if (!collection) return null;
  return {
    ...collection,
    entries: collection.entries.map(entry =>
      entry.id === data.entry_id || entry.session_id === data.id
        ? {
            ...entry,
            session_id: data.id,
            dirty: imageDirty(data),
            project_dirty: projectNeedsSave(data),
            edited: data.revision > 0,
            saved: data.saved_revision != null,
            saved_name: data.saved_name ?? null,
          }
        : entry,
    ),
  };
}
export function forgetCollectionDocuments(
  collection: ImageCollection | null,
  ids: Set<string>,
): ImageCollection | null {
  return collection
    ? {
        ...collection,
        entries: collection.entries.map((entry): CollectionEntry =>
          entry.session_id && ids.has(entry.session_id)
            ? {
                ...entry,
                session_id: null,
                dirty: false,
                project_dirty: false,
                edited: false,
                saved: false,
                saved_name: null,
              }
            : entry,
        ),
      }
    : null;
}
export function collectionForDisplay(
  collection: ImageCollection | null,
  accepted: ReadonlyMap<string, DocumentMetadata>,
): ImageCollection | null {
  if (!collection) return null;
  return {
    ...collection,
    entries: collection.entries.map(entry => {
      if (collection.local) return { ...entry };
      const current = entry.session_id ? accepted.get(entry.session_id) : null;
      const revision = current ? '&revision=' + current.revision : '';
      const key = entry.session_id ? '?session=' + encodeURIComponent(entry.session_id) + revision : '';
      return {
        ...entry,
        thumbnail:
          '/api/local-remove/collection/' +
          encodeURIComponent(collection.id) +
          '/entry/' +
          encodeURIComponent(entry.id) +
          '/thumbnail' +
          key,
      };
    }),
  };
}
export function documentCredits(data: DocumentMetadata | null): Credit[] {
  const credits: Array<Credit & Attribution> = [];
  if (data?.source_attribution) credits.push({ label: 'Source image', ...data.source_attribution });
  if (data?.cutout?.background?.attribution)
    credits.push({ label: 'Background', ...data.cutout.background.attribution });
  data?.reference_attributions?.forEach((value, index) =>
    credits.push({ label: 'Reference ' + (index + 1), ...value }),
  );
  data?.cutout?.background?.reference_attributions?.forEach((value, index) =>
    credits.push({ label: 'Background reference ' + (index + 1), ...value }),
  );
  for (const layer of data?.layer_stack ?? [])
    if (layer.visible && !layer.discarded) {
      if (layer.attribution) credits.push({ label: 'Layer · ' + layer.name, ...layer.attribution });
      layer.reference_attributions?.forEach((value, index) =>
        credits.push({ label: layer.name + ' reference ' + (index + 1), ...value }),
      );
    }
  const seen = new Set<string>();
  return credits.filter(credit => {
    const key = JSON.stringify([credit.provider, credit.asset_id, credit.source_url]);
    if (seen.has(key)) return false;
    seen.add(key);
    return true;
  });
}
const safeLink = (value?: string) => {
  try {
    const url = new URL(value || '');
    return ['http:', 'https:'].includes(url.protocol) && !url.username && !url.password ? value : '';
  } catch {
    return '';
  }
};
export const creditText = (credits: readonly Credit[]) =>
  credits
    .map(credit =>
      [
        credit.label + (credit.title ? ' · ' + credit.title : ''),
        credit.attribution || [credit.title, credit.creator, credit.license].filter(Boolean).join(' · '),
        safeLink(credit.source_url),
        safeLink(credit.license_url),
      ]
        .filter(Boolean)
        .join('\n'),
    )
    .join('\n\n');
export function safeDownload(value: string, sid: string): string {
  const prefix = '/api/local-remove/session/' + encodeURIComponent(sid) + '/';
  if (
    !value.startsWith(prefix) ||
    !/^download(?:-project|-credits)?(?:\?ext=(?:png|jpg|jpeg|tif|tiff|webp))?$/.test(value.slice(prefix.length))
  )
    throw Error('The backend returned an unsupported document download.');
  return value;
}
export const supportedImage = (file: Pick<File, 'name'>) => /\.(jpe?g|png|tiff?|webp)$/i.test(file.name);
