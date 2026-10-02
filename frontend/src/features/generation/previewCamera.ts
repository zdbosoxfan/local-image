import type { EditorDocument } from '../../contracts.ts';

export type PreviewCamera = { scale: number | 'fit'; x: number; y: number };
export type PreviewDimensions = { width: number; height: number };
const clamp = (value: number, min: number, max: number) => Math.max(min, Math.min(max, value));
export const fitPreview = (): PreviewCamera => ({ scale: 'fit', x: .5, y: .5 });

export function previewLayout(camera: PreviewCamera, image: PreviewDimensions, viewport: PreviewDimensions) {
  const fitted = Math.min(viewport.width / image.width, viewport.height / image.height);
  const scale = camera.scale === 'fit' ? (fitted > 0 && Number.isFinite(fitted) ? fitted : 1) : camera.scale;
  const width = image.width * scale, height = image.height * scale;
  return { scale, width, height, left: viewport.width / 2 - camera.x * width, top: viewport.height / 2 - camera.y * height };
}

/** Keep the image pixel under the pointer fixed while changing magnification. */
export function zoomPreview(camera: PreviewCamera, image: PreviewDimensions, viewport: PreviewDimensions, factor: number, anchor = { x: viewport.width / 2, y: viewport.height / 2 }): PreviewCamera {
  const before = previewLayout(camera, image, viewport);
  if (!(image.width > 0 && image.height > 0 && factor > 0 && Number.isFinite(factor))) return camera;
  const scale = clamp(before.scale * factor, .02, 8);
  return { scale,
    x: clamp(((anchor.x - before.left) / before.scale - (anchor.x - viewport.width / 2) / scale) / image.width, 0, 1),
    y: clamp(((anchor.y - before.top) / before.scale - (anchor.y - viewport.height / 2) / scale) / image.height, 0, 1) };
}

export function panPreview(camera: PreviewCamera, image: PreviewDimensions, viewport: PreviewDimensions, dx: number, dy: number): PreviewCamera {
  const { scale } = previewLayout(camera, image, viewport);
  if (!(image.width > 0 && image.height > 0)) return camera;
  return { ...camera, x: clamp(camera.x - dx / (image.width * scale), 0, 1), y: clamp(camera.y - dy / (image.height * scale), 0, 1) };
}

export function wheelZoomFactor(delta: number, mode: number, viewportHeight: number) {
  const pixels = delta * (mode === 1 ? 16 : mode === 2 ? viewportHeight : 1);
  return Math.exp(-clamp(pixels, -480, 480) * .002);
}

export function refinementPreview(step: 'draft' | 'final', draft?: EditorDocument, result?: EditorDocument) {
  return step === 'draft'
    ? { document: draft, label: 'Draft', notice: '' }
    : { document: result ?? draft, label: 'Refined result', notice: !result && draft ? 'Showing the draft until a refined result is ready.' : '' };
}
