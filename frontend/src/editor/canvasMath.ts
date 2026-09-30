/** Arithmetic extracted from editor.js camera/coord helpers and
 * layers-studio.js pointOnLayer/geometry/pointermove. Keep document-pixel and
 * preview-pixel spaces separate; this is the existing Canvas2D gesture model. */
export interface Point { x: number; y: number }
export interface LayerTransform { offset_x: number; offset_y: number; scale: number; rotation: number }
export interface Camera { zoom: number; panX: number; panY: number; fitMode: boolean }
export interface LayerGeometry { corners: Point[]; center: Point; top: Point; rotate: Point; bounds: readonly number[] }
export const MIN_PHOTO_ZOOM = .01, MAX_PHOTO_ZOOM = 8;
export const BRUSH_STEPS = [1,2,3,4,5,6,7,8,9,10,12,15,20,25,30,35,40,45,50,60,70,80,90,100,125,150,175,200,250,300,400,500,600,800,1000,1200,1500,2000] as const;
export const identityTransform = (): LayerTransform => ({ offset_x: 0, offset_y: 0, scale: 1, rotation: 0 });
export function pixelRatio(documentWidth: number, previewWidth: number) { return previewWidth ? documentWidth / previewWidth : 1; }
export function clampCamera(camera: Camera, previewWidth: number, previewHeight: number, viewportWidth: number, viewportHeight: number): Camera {
  const width = previewWidth * camera.zoom, height = previewHeight * camera.zoom;
  return { ...camera, panX: width <= viewportWidth ? (viewportWidth - width) / 2 : Math.min(24, Math.max(viewportWidth - width - 24, camera.panX)),
    panY: height <= viewportHeight ? (viewportHeight - height) / 2 : Math.min(24, Math.max(viewportHeight - height - 24, camera.panY)) };
}
export function fitCamera(previewWidth: number, previewHeight: number, viewportWidth: number, viewportHeight: number, ratio: number): Camera {
  const zoom = Math.max(.001, Math.min((viewportWidth - 24) / previewWidth, (viewportHeight - 24) / previewHeight, ratio));
  return { zoom, panX: (viewportWidth - previewWidth * zoom) / 2, panY: (viewportHeight - previewHeight * zoom) / 2, fitMode: true };
}
export function zoomAt(camera: Camera, photoZoom: number, ratio: number, anchor: Point): Camera {
  const imageX = (anchor.x - camera.panX) / camera.zoom, imageY = (anchor.y - camera.panY) / camera.zoom;
  const zoom = Math.min(MAX_PHOTO_ZOOM, Math.max(MIN_PHOTO_ZOOM, photoZoom)) * ratio;
  return { zoom, panX: anchor.x - imageX * zoom, panY: anchor.y - imageY * zoom, fitMode: false };
}
export function previewCoordinate(local: Point, camera: Camera, width: number, height: number): Point {
  return { x: Math.max(0, Math.min(width, (local.x - camera.panX) / camera.zoom)), y: Math.max(0, Math.min(height, (local.y - camera.panY) / camera.zoom)) };
}
export function pointOnLayer(point: Point, width: number, height: number, next: LayerTransform): Point {
  const cx = (width - 1) / 2, cy = (height - 1) / 2, a = next.rotation * Math.PI / 180, c = Math.cos(a), s = Math.sin(a), x = (point.x - cx) * next.scale, y = (point.y - cy) * next.scale;
  return { x: cx + next.offset_x + c * x - s * y, y: cy + next.offset_y + s * x + c * y };
}
export function layerGeometry(width: number, height: number, bounds: readonly number[] | undefined, next: LayerTransform, ratio: number, zoom: number): LayerGeometry {
  const b = bounds || [0, 0, width, height], corners = [{ x: b[0], y: b[1] }, { x: b[2], y: b[1] }, { x: b[2], y: b[3] }, { x: b[0], y: b[3] }].map(point => pointOnLayer(point, width, height, next));
  const center = { x: (corners[0].x + corners[2].x) / 2, y: (corners[0].y + corners[2].y) / 2 }, top = { x: (corners[0].x + corners[1].x) / 2, y: (corners[0].y + corners[1].y) / 2 }, length = Math.hypot(top.x - center.x, top.y - center.y) || 1, distance = 28 * ratio / zoom;
  return { corners, center, top, rotate: { x: top.x + (top.x - center.x) / length * distance, y: top.y + (top.y - center.y) / length * distance }, bounds: b };
}
export function movedTransform(original: LayerTransform, start: Point, point: Point): LayerTransform {
  return { ...original, offset_x: original.offset_x + point.x - start.x, offset_y: original.offset_y + point.y - start.y };
}
export function scaledTransform(original: LayerTransform, geometry: LayerGeometry, cornerIndex: number, point: Point, width: number, height: number): LayerTransform {
  const opposite = (cornerIndex + 2) % 4, anchor = geometry.corners[opposite], corner = geometry.corners[cornerIndex], factor = Math.hypot(point.x - anchor.x, point.y - anchor.y) / Math.max(1, Math.hypot(corner.x - anchor.x, corner.y - anchor.y)), scale = Math.max(.05, Math.min(4, original.scale * factor));
  const b = geometry.bounds, source = [{ x: b[0], y: b[1] }, { x: b[2], y: b[1] }, { x: b[2], y: b[3] }, { x: b[0], y: b[3] }][opposite], a = original.rotation * Math.PI / 180, cx = (width - 1) / 2, cy = (height - 1) / 2, x = (source.x - cx) * scale, y = (source.y - cy) * scale;
  return { ...original, scale, offset_x: anchor.x - cx - Math.cos(a) * x + Math.sin(a) * y, offset_y: anchor.y - cy - Math.sin(a) * x - Math.cos(a) * y };
}
export function rotatedTransform(original: LayerTransform, geometry: LayerGeometry, start: Point, point: Point, width: number, height: number): LayerTransform {
  const c = geometry.center, delta = (Math.atan2(point.y - c.y, point.x - c.x) - Math.atan2(start.y - c.y, start.x - c.x)) * 180 / Math.PI, rotation = ((original.rotation + delta + 540) % 360) - 180, a = rotation * Math.PI / 180, b = geometry.bounds, cx = (width - 1) / 2, cy = (height - 1) / 2, x = ((b[0] + b[2]) / 2 - cx) * original.scale, y = ((b[1] + b[3]) / 2 - cy) * original.scale;
  return { ...original, rotation, offset_x: c.x - cx - Math.cos(a) * x + Math.sin(a) * y, offset_y: c.y - cy - Math.sin(a) * x - Math.cos(a) * y };
}
