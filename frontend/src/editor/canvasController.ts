import {
  BRUSH_STEPS,
  clampCamera,
  fitCamera,
  identityTransform,
  layerGeometry,
  movedTransform,
  pixelRatio,
  previewCoordinate,
  rotatedTransform,
  scaledTransform,
  zoomAt,
} from './canvasMath.ts';
import type { Camera, LayerGeometry, LayerTransform, Point } from './canvasMath.ts';
import type {
  CanvasDisplay,
  CanvasDocument,
  CanvasElements,
  CanvasInteraction,
  CanvasLayer,
  CanvasPorts,
  CanvasSnapshot,
  CanvasTool,
  CanvasViewState,
  SelectionRedo,
} from './canvasContracts.ts';

type ClientPoint = { clientX: number; clientY: number };
type DrawGesture = {
  kind: 'draw';
  tool: Exclude<CanvasTool, 'pen' | 'move'>;
  pointerId: number;
  start: Point;
  last: Point;
  clientX: number;
  clientY: number;
};
type PanGesture = { kind: 'pan'; pointerId: number; clientX: number; clientY: number; startX: number; startY: number };
type CutoutGesture = {
  kind: 'transform';
  pointerId: number;
  clientX: number;
  clientY: number;
  sid: string;
  revision: number;
  original: LayerTransform;
  next: LayerTransform;
};
type MoveAsset = { node: CanvasLayer; image: HTMLImageElement };
type LayerGesture = {
  sid: string;
  revision: number;
  id: string;
  pointerId: number;
  start: Point;
  original: LayerTransform;
  next: LayerTransform;
  geometry: LayerGeometry;
  corner: number;
  kind: 'move' | 'scale' | 'rotate';
  assets: MoveAsset[] | null;
};
const owners = new WeakMap<HTMLElement, symbol>();
const defaults: CanvasInteraction = {
  tool: 'brush',
  workspace: 'retouch',
  operation: 'heal',
  cutoutOperation: 'erase',
  subtract: false,
  brushSize: 28,
  handActive: false,
  showOriginal: false,
  busy: false,
  selectedLayerId: null,
};

/** Owns only the persistent canvas subtree. No application control IDs, React
 * state, backend mutation retries, native listeners, or function overrides.
 * Central application shortcuts call the exposed methods instead of installing
 * another keyboard handler. See canvasExtraction.ts for source provenance. */
export function createCanvasController(elements: CanvasElements, ports: CanvasPorts) {
  const { viewport, stage, photo: baseCanvas, photoImage, overlay, draft, layerStack, brushCursor } = elements;
  if (owners.has(viewport)) throw Error('This viewport already has a canvas controller.');
  const owner = Symbol('canvas-controller');
  owners.set(viewport, owner);
  const ownerDocument = viewport.ownerDocument,
    ownerWindow = ownerDocument.defaultView!;
  const mask = ownerDocument.createElement('canvas');
  const mc = mask.getContext('2d', { willReadFrequently: true })!,
    bc = baseCanvas.getContext('2d')!,
    oc = overlay.getContext('2d')!,
    dc = draft.getContext('2d')!;
  if (!mc || !bc || !oc || !dc) throw Error('Canvas 2D is unavailable.');
  const views = new Map<string, CanvasViewState>(),
    assetCache = new Map<string, MoveAsset[]>(),
    listeners = new Set<() => void>();
  let document: CanvasDocument | null = null,
    display: CanvasDisplay | null = null,
    interaction = { ...defaults };
  let camera: Camera = { zoom: 1, panX: 0, panY: 0, fitMode: true },
    viewportWidth = 0,
    viewportHeight = 0;
  let points: Point[] = [],
    undo: string[] = [],
    redo: SelectionRedo[] = [],
    hasSelection = false,
    historyBusy = false,
    spaceHeld = false,
    selectionVersion = 0;
  let gesture: DrawGesture | PanGesture | CutoutGesture | null = null,
    moving: LayerGesture | null = null,
    brushPointer: ClientPoint | null = null;
  let legacyTransformAssets: {
    sid: string;
    revision: number;
    foreground: HTMLImageElement;
    background: HTMLImageElement;
  } | null = null;
  let navigationEpoch = 0,
    disposed = false,
    currentSnapshot: CanvasSnapshot | null = null;
  const image =
    ports.loadImage ??
    ((url: string) =>
      new Promise<HTMLImageElement>((resolve, reject) => {
        const value = ownerDocument.createElement('img');
        value.onload = () => resolve(value);
        value.onerror = () => reject(Error('Could not load the photo'));
        value.src = url;
      }));
  const ratio = () => (document ? pixelRatio(document.width, baseCanvas.width) : 1);
  const photoZoom = () => camera.zoom / ratio();
  const selected = () =>
    document?.layer_stack?.find(layer => layer.id === interaction.selectedLayerId && !layer.discarded) ?? null;
  const transform = (layer: CanvasLayer) => ({ ...identityTransform(), ...layer.transform });
  const sameDocument = (sid: string, revision: number) => {
    const accepted = ports.getAcceptedDocument();
    return (
      !disposed &&
      document?.id === sid &&
      document.revision === revision &&
      accepted?.id === sid &&
      accepted.revision === revision
    );
  };
  function getSnapshot(): CanvasSnapshot {
    const next: CanvasSnapshot = {
      documentId: document?.id ?? null,
      revision: document?.revision ?? null,
      hasSelection,
      pointCount: points.length,
      selectionVersion,
      canUndoSelection: !!(points.length || undo.length),
      canRedoSelection: redo.length > 0,
      historyBusy,
      gesture: moving ? 'move' : gesture?.kind === 'transform' ? 'move' : (gesture?.kind ?? null),
      photoZoom: photoZoom(),
      fitMode: camera.fitMode,
      panX: camera.panX,
      panY: camera.panY,
      previewWidth: baseCanvas.width,
      previewHeight: baseCanvas.height,
    };
    if (
      !currentSnapshot ||
      (Object.keys(next) as Array<keyof CanvasSnapshot>).some(key => next[key] !== currentSnapshot![key])
    )
      currentSnapshot = Object.freeze(next);
    return currentSnapshot;
  }
  function publish() {
    if (disposed) return;
    const old = currentSnapshot,
      next = getSnapshot();
    if (next !== old) {
      ports.onSnapshot?.(next);
      for (const listener of listeners) listener();
    }
  }
  function localPoint(event: ClientPoint): Point {
    const rect = viewport.getBoundingClientRect();
    return { x: event.clientX - rect.left - viewport.clientLeft, y: event.clientY - rect.top - viewport.clientTop };
  }
  const coord = (event: ClientPoint) => previewCoordinate(localPoint(event), camera, mask.width, mask.height);
  function insidePhoto(event: ClientPoint) {
    const point = localPoint(event);
    return (
      point.x >= camera.panX &&
      point.y >= camera.panY &&
      point.x <= camera.panX + mask.width * camera.zoom &&
      point.y <= camera.panY + mask.height * camera.zoom
    );
  }
  function updateBrushCursor() {
    const visible =
      interaction.workspace !== 'generate' &&
      !!document &&
      !!brushPointer &&
      interaction.tool === 'brush' &&
      !interaction.busy &&
      !historyBusy &&
      !spaceHeld &&
      !interaction.handActive &&
      !interaction.showOriginal &&
      !ports.isMenuOpen?.() &&
      !ports.isModalOpen() &&
      insidePhoto(brushPointer);
    brushCursor.hidden = !visible;
    viewport.classList.toggle('brush-ready', visible);
    if (!visible || !brushPointer) return;
    const point = localPoint(brushPointer),
      diameter = interaction.brushSize * photoZoom();
    brushCursor.style.width = `${diameter}px`;
    brushCursor.style.height = `${diameter}px`;
    brushCursor.style.left = `${point.x}px`;
    brushCursor.style.top = `${point.y}px`;
  }
  function updateCursor() {
    viewport.classList.toggle('hand', (spaceHeld || interaction.handActive) && !!document);
    viewport.classList.toggle('panning', gesture?.kind === 'pan');
    viewport.classList.toggle(
      'moving-subject',
      interaction.workspace === 'cutout' && interaction.tool === 'move' && !spaceHeld && !interaction.handActive,
    );
    updateBrushCursor();
  }
  function shape(context: CanvasRenderingContext2D, a: Point, b: Point, kind: string, fill: boolean) {
    context.beginPath();
    if (kind === 'rectangle')
      context.rect(Math.min(a.x, b.x), Math.min(a.y, b.y), Math.abs(b.x - a.x), Math.abs(b.y - a.y));
    else
      context.ellipse(
        (a.x + b.x) / 2,
        (a.y + b.y) / 2,
        Math.abs(b.x - a.x) / 2,
        Math.abs(b.y - a.y) / 2,
        0,
        0,
        Math.PI * 2,
      );
    if (fill) context.fill();
    else context.stroke();
  }
  function shapeDraft(a: Point, b: Point, kind: string) {
    dc.clearRect(0, 0, draft.width, draft.height);
    dc.strokeStyle = '#ff87b3';
    dc.lineWidth = 2 / camera.zoom;
    shape(dc, a, b, kind, false);
  }
  function penDraft(hover: Point | null = null) {
    dc.clearRect(0, 0, draft.width, draft.height);
    if (!points.length) return;
    dc.strokeStyle = '#ff87b3';
    dc.fillStyle = '#ff87b3';
    dc.lineWidth = 2 / camera.zoom;
    dc.beginPath();
    dc.moveTo(points[0].x, points[0].y);
    for (const point of points.slice(1)) dc.lineTo(point.x, point.y);
    if (hover) dc.lineTo(hover.x, hover.y);
    dc.stroke();
    for (const point of points) {
      dc.beginPath();
      dc.arc(point.x, point.y, 3 / camera.zoom, 0, Math.PI * 2);
      dc.fill();
    }
  }
  function applyCamera() {
    if (!document) return;
    camera = clampCamera(camera, baseCanvas.width, baseCanvas.height, viewport.clientWidth, viewport.clientHeight);
    stage.style.transform = `translate(${camera.panX}px,${camera.panY}px) scale(${camera.zoom})`;
    if (points.length) penDraft();
    else if (gesture?.kind === 'draw' && gesture.tool !== 'brush')
      shapeDraft(gesture.start, gesture.last, gesture.tool);
    updateBrushCursor();
    drawHandles();
  }
  function fit() {
    if (!document || !baseCanvas.width) return;
    camera = fitCamera(baseCanvas.width, baseCanvas.height, viewport.clientWidth, viewport.clientHeight, ratio());
    viewportWidth = viewport.clientWidth;
    viewportHeight = viewport.clientHeight;
    applyCamera();
    publish();
  }
  function setPhotoZoom(value: number, anchor = { x: viewport.clientWidth / 2, y: viewport.clientHeight / 2 }) {
    if (!document) return;
    if (gesture?.kind === 'draw') endGesture();
    camera = zoomAt(camera, value, ratio(), anchor);
    applyCamera();
    publish();
  }
  function resize() {
    if (!document) return;
    if (camera.fitMode) {
      fit();
      return;
    }
    camera.panX += (viewport.clientWidth - viewportWidth) / 2;
    camera.panY += (viewport.clientHeight - viewportHeight) / 2;
    viewportWidth = viewport.clientWidth;
    viewportHeight = viewport.clientHeight;
    applyCamera();
    publish();
  }
  function paintMask() {
    oc.clearRect(0, 0, overlay.width, overlay.height);
    oc.drawImage(mask, 0, 0);
    oc.globalCompositeOperation = 'source-in';
    oc.fillStyle =
      interaction.workspace === 'cutout' && interaction.cutoutOperation === 'restore' ? '#6bdeb7' : '#ff6699';
    oc.fillRect(0, 0, overlay.width, overlay.height);
    oc.globalCompositeOperation = 'source-over';
  }
  function refreshMask() {
    paintMask();
    const pixels = mc.getImageData(0, 0, mask.width, mask.height).data;
    hasSelection = false;
    for (let i = 3; i < pixels.length; i += 4)
      if (pixels[i]) {
        hasSelection = true;
        break;
      }
    selectionVersion++;
    publish();
  }
  function snapshotSelection() {
    redo = [];
    undo.push(mask.toDataURL('image/png'));
    if (undo.length > 12) undo.shift();
  }
  function drawingMode(context: CanvasRenderingContext2D) {
    context.globalCompositeOperation = interaction.subtract ? 'destination-out' : 'source-over';
    context.fillStyle = 'white';
    context.strokeStyle = 'white';
    context.lineCap = 'round';
    context.lineJoin = 'round';
    context.lineWidth = interaction.brushSize / ratio();
  }
  function stroke(a: Point, b: Point) {
    drawingMode(mc);
    mc.beginPath();
    mc.moveTo(a.x, a.y);
    mc.lineTo(b.x, b.y);
    mc.stroke();
    mc.beginPath();
    mc.arc(b.x, b.y, mc.lineWidth / 2, 0, Math.PI * 2);
    mc.fill();
  }
  function finishPen() {
    if (interaction.busy || historyBusy || !document || interaction.showOriginal) return;
    if (points.length < 3) {
      ports.report?.('Click at least three points for a pen selection.');
      return;
    }
    snapshotSelection();
    drawingMode(mc);
    mc.beginPath();
    mc.moveTo(points[0].x, points[0].y);
    for (const point of points.slice(1)) mc.lineTo(point.x, point.y);
    mc.closePath();
    mc.fill();
    points = [];
    dc.clearRect(0, 0, draft.width, draft.height);
    refreshMask();
  }
  function release(pointerId: number) {
    if (viewport.hasPointerCapture(pointerId)) viewport.releasePointerCapture(pointerId);
  }
  function endGesture(releaseCapture = true) {
    if (!gesture) return;
    const ended = gesture;
    gesture = null;
    if (ended.kind === 'draw') {
      if (ended.tool !== 'brush') {
        drawingMode(mc);
        shape(mc, ended.start, ended.last, ended.tool, true);
      }
      dc.clearRect(0, 0, draft.width, draft.height);
      refreshMask();
    }
    if (ended.kind === 'transform') {
      dc.clearRect(0, 0, draft.width, draft.height);
      paintPhoto();
    }
    if (releaseCapture) release(ended.pointerId);
    if (points.length) penDraft();
    updateCursor();
    publish();
  }
  function cancelMove() {
    if (!moving) return;
    const previous = moving;
    moving = null;
    release(previous.pointerId);
    paintPhoto();
    publish();
  }
  function clearSelection(options: { recordHistory?: boolean } = {}) {
    if (options.recordHistory) snapshotSelection();
    endGesture();
    mc.clearRect(0, 0, mask.width, mask.height);
    dc.clearRect(0, 0, draft.width, draft.height);
    points = [];
    hasSelection = false;
    refreshMask();
  }
  function paintPhoto() {
    if (!document || !display) return;
    bc.clearRect(0, 0, baseCanvas.width, baseCanvas.height);
    const composited = !interaction.showOriginal && (!!document.layer_stack || !!document.cutout?.enabled),
      shown = composited && display.composite ? display.composite : display.original;
    photoImage.hidden = false;
    if (photoImage.src !== shown.src) photoImage.src = shown.src;
    stage.classList.toggle('cutout-preview', composited);
    layerStack.hidden = !!document.layer_stack || interaction.showOriginal || composited;
    overlay.hidden = interaction.showOriginal || interaction.workspace === 'generate';
    draft.hidden = interaction.showOriginal || interaction.workspace === 'generate';
    drawHandles();
    updateCursor();
  }
  function drawHandles() {
    const layer = selected();
    if (
      !document?.layer_stack ||
      interaction.tool !== 'move' ||
      interaction.workspace === 'generate' ||
      interaction.showOriginal ||
      interaction.handActive ||
      !layer
    )
      return;
    dc.clearRect(0, 0, draft.width, draft.height);
    if (!layer.visible || layer.discarded) return;
    const geometry = layerGeometry(
        document.width,
        document.height,
        layer.bounds,
        moving?.next || transform(layer),
        ratio(),
        camera.zoom,
      ),
      r = ratio(),
      size = 7 / camera.zoom;
    draft.hidden = false;
    dc.save();
    dc.lineWidth = 1 / camera.zoom;
    dc.strokeStyle = layer.locked ? '#a9b4c2' : '#65bdff';
    dc.fillStyle = '#172734';
    dc.beginPath();
    geometry.corners.forEach((point, index) =>
      index ? dc.lineTo(point.x / r, point.y / r) : dc.moveTo(point.x / r, point.y / r),
    );
    dc.closePath();
    dc.stroke();
    if (!layer.locked) {
      dc.beginPath();
      dc.moveTo(geometry.top.x / r, geometry.top.y / r);
      dc.lineTo(geometry.rotate.x / r, geometry.rotate.y / r);
      dc.stroke();
      for (const point of [...geometry.corners, geometry.rotate]) {
        dc.fillRect(point.x / r - size / 2, point.y / r - size / 2, size, size);
        dc.strokeRect(point.x / r - size / 2, point.y / r - size / 2, size, size);
      }
    }
    dc.restore();
  }
  async function loadMoveAssets(requested: CanvasDocument) {
    const key = `${requested.id}:${requested.revision}`,
      cached = assetCache.get(key);
    if (cached) return cached;
    const value = await Promise.all(
      (requested.layer_stack || [])
        .filter(node => node.visible && !node.discarded)
        .map(async node => ({
          node,
          image: await image(
            `/api/local-remove/session/${encodeURIComponent(requested.id)}/stack/layer/${encodeURIComponent(node.id)}/display?r=${requested.revision}`,
          ),
        })),
    );
    if (sameDocument(requested.id, requested.revision)) {
      assetCache.clear();
      assetCache.set(key, value);
    }
    return value;
  }
  function paintMoving() {
    if (!document || !moving?.assets) return;
    photoImage.hidden = true;
    layerStack.hidden = true;
    overlay.hidden = true;
    bc.clearRect(0, 0, baseCanvas.width, baseCanvas.height);
    const r = ratio(),
      cx = (document.width - 1) / 2 / r,
      cy = (document.height - 1) / 2 / r;
    for (const asset of moving.assets) {
      const next = asset.node.id === moving.id ? moving.next : transform(asset.node);
      bc.save();
      bc.globalAlpha = asset.node.opacity ?? 1;
      bc.translate(cx + next.offset_x / r, cy + next.offset_y / r);
      bc.rotate((next.rotation * Math.PI) / 180);
      bc.scale(next.scale, next.scale);
      bc.drawImage(asset.image, -cx, -cy, baseCanvas.width, baseCanvas.height);
      bc.restore();
    }
    drawHandles();
  }
  async function prepareLegacyTransform() {
    if (!document?.cutout?.enabled) return;
    const { id, revision } = document;
    if (legacyTransformAssets?.sid === id && legacyTransformAssets.revision === revision) return;
    const prefix = `/api/local-remove/session/${encodeURIComponent(id)}/cutout/`,
      [foreground, background] = await Promise.all([
        image(`${prefix}foreground?full=true&revision=${revision}`),
        image(`${prefix}background-preview?full=true&revision=${revision}`),
      ]);
    if (sameDocument(id, revision)) legacyTransformAssets = { sid: id, revision, foreground, background };
  }
  function paintLegacyTransform(next: LayerTransform) {
    if (!document || !legacyTransformAssets) return;
    const width = baseCanvas.width,
      height = baseCanvas.height,
      r = ratio();
    photoImage.hidden = true;
    overlay.hidden = true;
    bc.clearRect(0, 0, width, height);
    bc.drawImage(legacyTransformAssets.background, 0, 0, width, height);
    bc.save();
    bc.translate((document.width - 1) / 2 / r + next.offset_x / r, (document.height - 1) / 2 / r + next.offset_y / r);
    bc.rotate((next.rotation * Math.PI) / 180);
    bc.scale(next.scale, next.scale);
    bc.drawImage(
      legacyTransformAssets.foreground,
      -(document.width - 1) / 2 / r,
      -(document.height - 1) / 2 / r,
      width,
      height,
    );
    bc.restore();
    dc.clearRect(0, 0, draft.width, draft.height);
    const bounds = document.cutout_bounds;
    if (bounds) {
      dc.save();
      dc.translate((document.width - 1) / 2 / r + next.offset_x / r, (document.height - 1) / 2 / r + next.offset_y / r);
      dc.rotate((next.rotation * Math.PI) / 180);
      dc.scale(next.scale, next.scale);
      dc.strokeStyle = '#b9d8f5';
      dc.lineWidth = 1 / camera.zoom / next.scale;
      dc.strokeRect(
        (bounds[0] - (document.width - 1) / 2) / r,
        (bounds[1] - (document.height - 1) / 2) / r,
        (bounds[2] - bounds[0]) / r,
        (bounds[3] - bounds[1]) / r,
      );
      dc.restore();
    }
  }
  function documentPoint(event: ClientPoint): Point {
    const point = coord(event),
      r = ratio();
    return { x: point.x * r, y: point.y * r };
  }
  function startPan(event: ClientPoint & { pointerId: number }) {
    gesture = {
      kind: 'pan',
      pointerId: event.pointerId,
      clientX: event.clientX,
      clientY: event.clientY,
      startX: camera.panX,
      startY: camera.panY,
    };
    viewport.setPointerCapture(event.pointerId);
    camera.fitMode = false;
    if (points.length) penDraft();
    updateCursor();
    publish();
  }
  async function pointerDown(event: PointerEvent) {
    if (disposed || !document || ports.isModalOpen() || ports.isEditorHidden?.()) return;
    // The old stack capture listener precedes the base gesture listener. Keep
    // that exact priority, including no-mask cutout and move hit-test behavior.
    if (
      document.layer_stack &&
      interaction.workspace !== 'generate' &&
      !spaceHeld &&
      !interaction.handActive &&
      event.button === 0
    ) {
      if (interaction.workspace === 'cutout' && selected()?.kind !== 'cutout' && interaction.tool !== 'move') {
        event.stopImmediatePropagation();
        return;
      }
      if (interaction.tool === 'move') {
        event.preventDefault();
        event.stopImmediatePropagation();
        const node = selected();
        if (
          !node ||
          interaction.busy ||
          historyBusy ||
          interaction.showOriginal ||
          !node.visible ||
          node.discarded ||
          moving ||
          gesture
        )
          return;
        if (node.locked) {
          ports.report?.('Unlock this layer in Layers to move it.');
          return;
        }
        const start = documentPoint(event),
          original = transform(node),
          geometry = layerGeometry(document.width, document.height, node.bounds, original, ratio(), camera.zoom),
          tolerance = (12 * ratio()) / camera.zoom;
        const corner = geometry.corners.findIndex(
            point => Math.hypot(point.x - start.x, point.y - start.y) < tolerance,
          ),
          kind =
            corner >= 0
              ? 'scale'
              : Math.hypot(geometry.rotate.x - start.x, geometry.rotate.y - start.y) < tolerance
                ? 'rotate'
                : 'move';
        const drag: LayerGesture = {
          sid: document.id,
          revision: document.revision,
          id: node.id,
          pointerId: event.pointerId,
          start,
          original,
          next: { ...original },
          geometry,
          corner,
          kind,
          assets: null,
        };
        moving = drag;
        viewport.setPointerCapture(event.pointerId);
        viewport.focus({ preventScroll: true });
        publish();
        try {
          const assets = await loadMoveAssets(document);
          if (moving === drag && sameDocument(drag.sid, drag.revision)) {
            drag.assets = assets;
            paintMoving();
          }
        } catch (error) {
          if (moving === drag) {
            cancelMove();
            ports.report?.(error instanceof Error ? error.message : 'Could not load layer preview', true);
          }
        }
        return;
      }
    }
    if (gesture || moving || ![0, 1].includes(event.button)) return;
    event.preventDefault();
    viewport.focus({ preventScroll: true });
    brushPointer = { clientX: event.clientX, clientY: event.clientY };
    updateBrushCursor();
    if (spaceHeld || interaction.handActive || event.button === 1) {
      startPan(event);
      return;
    }
    if (
      interaction.busy ||
      historyBusy ||
      interaction.showOriginal ||
      interaction.workspace === 'generate' ||
      !insidePhoto(event)
    )
      return;
    if (interaction.workspace === 'cutout' && interaction.tool === 'move') {
      if (!document.cutout?.enabled) return;
      if (legacyTransformAssets?.sid !== document.id || legacyTransformAssets?.revision !== document.revision) {
        void prepareLegacyTransform().catch(error =>
          ports.report?.(error instanceof Error ? error.message : 'Could not load subject preview', true),
        );
        ports.report?.('Preparing subject preview. Drag again in a moment.');
        return;
      }
      const original = { ...identityTransform(), ...document.cutout.transform };
      gesture = {
        kind: 'transform',
        sid: document.id,
        revision: document.revision,
        pointerId: event.pointerId,
        clientX: event.clientX,
        clientY: event.clientY,
        original,
        next: { ...original },
      };
      viewport.setPointerCapture(event.pointerId);
      paintLegacyTransform(original);
      updateCursor();
      publish();
      return;
    }
    const point = coord(event);
    if (interaction.tool === 'pen') {
      if (points.length > 2 && Math.hypot(point.x - points[0].x, point.y - points[0].y) < 10 / camera.zoom) {
        finishPen();
        return;
      }
      redo = [];
      points.push(point);
      selectionVersion++;
      penDraft();
      publish();
      return;
    }
    if (interaction.tool === 'move') return;
    snapshotSelection();
    gesture = {
      kind: 'draw',
      tool: interaction.tool,
      pointerId: event.pointerId,
      start: point,
      last: point,
      clientX: event.clientX,
      clientY: event.clientY,
    };
    viewport.setPointerCapture(event.pointerId);
    if (interaction.tool === 'brush') {
      stroke(point, point);
      refreshMask();
    }
    publish();
  }
  function pointerMove(event: PointerEvent) {
    if (moving) {
      if (event.pointerId !== moving.pointerId || !document) return;
      event.preventDefault();
      event.stopImmediatePropagation();
      const point = documentPoint(event),
        drag = moving;
      drag.next =
        drag.kind === 'move'
          ? movedTransform(drag.original, drag.start, point)
          : drag.kind === 'scale'
            ? scaledTransform(drag.original, drag.geometry, drag.corner, point, document.width, document.height)
            : rotatedTransform(drag.original, drag.geometry, drag.start, point, document.width, document.height);
      paintMoving();
      return;
    }
    brushPointer = { clientX: event.clientX, clientY: event.clientY };
    updateBrushCursor();
    if (!document) return;
    if (gesture) {
      if (event.pointerId !== gesture.pointerId) return;
      if (gesture.kind === 'pan') {
        camera.panX = gesture.startX + event.clientX - gesture.clientX;
        camera.panY = gesture.startY + event.clientY - gesture.clientY;
        applyCamera();
        return;
      }
      if (gesture.kind === 'transform') {
        gesture.next = {
          ...gesture.original,
          offset_x: Math.round(gesture.original.offset_x + ((event.clientX - gesture.clientX) / camera.zoom) * ratio()),
          offset_y: Math.round(gesture.original.offset_y + ((event.clientY - gesture.clientY) / camera.zoom) * ratio()),
        };
        paintLegacyTransform(gesture.next);
        return;
      }
      const point = coord(event);
      gesture.clientX = event.clientX;
      gesture.clientY = event.clientY;
      if (gesture.tool === 'brush') {
        stroke(gesture.last, point);
        paintMask();
      } else shapeDraft(gesture.start, point, gesture.tool);
      gesture.last = point;
      return;
    }
    if (
      interaction.tool === 'pen' &&
      points.length &&
      !interaction.busy &&
      !historyBusy &&
      !interaction.showOriginal &&
      !spaceHeld &&
      !interaction.handActive
    )
      penDraft(insidePhoto(event) ? coord(event) : null);
  }
  async function finishMove(event: PointerEvent, cancel = false) {
    if (!moving || event.pointerId !== moving.pointerId) return;
    event.stopImmediatePropagation();
    const drag = moving;
    moving = null;
    release(event.pointerId);
    if (cancel || !sameDocument(drag.sid, drag.revision)) {
      paintPhoto();
      publish();
      return;
    }
    if (JSON.stringify(drag.next) !== JSON.stringify(drag.original)) {
      try {
        await ports.commitLayerTransform({
          documentId: drag.sid,
          revision: drag.revision,
          layerId: drag.id,
          transform: { ...drag.next },
        });
      } catch (error) {
        ports.report?.(error instanceof Error ? error.message : 'Could not transform layer', true);
      }
    }
    paintPhoto();
    publish();
  }
  function pointerUp(event: PointerEvent) {
    if (moving) {
      void finishMove(event);
      return;
    }
    if (!gesture || event.pointerId !== gesture.pointerId) return;
    if (gesture.kind === 'transform') {
      const previous = gesture,
        next = { ...previous.next },
        changed = next.offset_x !== previous.original.offset_x || next.offset_y !== previous.original.offset_y;
      endGesture();
      if (changed && sameDocument(previous.sid, previous.revision))
        void ports
          .commitLegacyCutoutTransform?.({ documentId: previous.sid, revision: previous.revision, transform: next })
          .catch(error => ports.report?.(error instanceof Error ? error.message : 'Could not move subject', true));
      return;
    }
    if (gesture.kind === 'draw') {
      const point = coord(event);
      if (gesture.tool === 'brush') stroke(gesture.last, point);
      gesture.last = point;
    }
    endGesture();
  }
  function pointerCancel(event: PointerEvent) {
    brushPointer = null;
    if (moving?.pointerId === event.pointerId) {
      void finishMove(event, true);
      return;
    }
    if (gesture?.pointerId === event.pointerId) endGesture();
    updateBrushCursor();
  }
  function lostCapture(event: PointerEvent) {
    if (moving?.pointerId === event.pointerId) {
      moving = null;
      paintPhoto();
      publish();
      return;
    }
    if (gesture?.pointerId === event.pointerId) endGesture();
  }
  function pointerLeave() {
    brushPointer = null;
    updateBrushCursor();
    if (!gesture && !moving && points.length) penDraft();
  }
  function wheel(event: WheelEvent) {
    if (!document || ports.isModalOpen() || ports.isEditorHidden?.()) return;
    event.preventDefault();
    brushPointer = { clientX: event.clientX, clientY: event.clientY };
    const delta = event.deltaY * (event.deltaMode === 1 ? 16 : event.deltaMode === 2 ? viewport.clientHeight : 1);
    setPhotoZoom(photoZoom() * Math.exp(-Math.max(-150, Math.min(150, delta)) * 0.0025), localPoint(event));
  }
  function setSpaceHeld(value: boolean) {
    if (spaceHeld === value) return;
    spaceHeld = value;
    if (value && gesture?.kind === 'draw') {
      const previous = gesture;
      endGesture(false);
      startPan({ pointerId: previous.pointerId, clientX: previous.clientX, clientY: previous.clientY });
    }
    if (points.length) penDraft();
    updateCursor();
  }
  function resetTransientInput() {
    spaceHeld = false;
    brushPointer = null;
    cancelMove();
    endGesture();
    updateCursor();
  }
  function rememberCurrentView() {
    if (!document) return;
    cancelMove();
    endGesture();
    views.set(document.id, {
      mask: hasSelection ? mask.toDataURL('image/png') : null,
      undo: [...undo],
      redo: structuredClone(redo),
      points: points.map(point => ({ ...point })),
      width: mask.width,
      height: mask.height,
      interaction: { ...interaction },
      photoZoom: photoZoom(),
      camera: { ...camera },
      viewportWidth: viewport.clientWidth,
      viewportHeight: viewport.clientHeight,
      hasSelection,
      selectionVersion,
    });
  }
  async function presentDocument(next: CanvasDisplay | null) {
    const epoch = ++navigationEpoch;
    historyBusy = false;
    if (!next || next.document.id !== document?.id) rememberCurrentView();
    if (!next) {
      document = null;
      display = null;
      legacyTransformAssets = null;
      points = [];
      undo = [];
      redo = [];
      hasSelection = false;
      stage.hidden = true;
      photoImage.removeAttribute('src');
      layerStack.replaceChildren();
      publish();
      return false;
    }
    const saved = next.document.id !== document?.id ? views.get(next.document.id) : null;
    let selection: HTMLImageElement | null = null;
    if (saved?.mask) selection = await image(saved.mask);
    const accepted = ports.getAcceptedDocument();
    if (
      disposed ||
      epoch !== navigationEpoch ||
      accepted?.id !== next.document.id ||
      accepted.revision !== next.document.revision
    )
      return false;
    const changedDocument = document?.id !== next.document.id,
      changedRevision = changedDocument || document?.revision !== next.document.revision;
    if (changedRevision) {
      cancelMove();
      legacyTransformAssets = null;
      assetCache.clear();
    }
    document = structuredClone(next.document);
    display = next;
    interaction = { ...interaction, ...next.interaction };
    stage.hidden = false;
    if (changedDocument) {
      endGesture();
      const sourceWidth = next.original.naturalWidth || next.original.width,
        sourceHeight = next.original.naturalHeight || next.original.height,
        scale = Math.min(1, 3000 / Math.max(sourceWidth, sourceHeight));
      const width = Math.round(sourceWidth * scale),
        height = Math.round(sourceHeight * scale);
      for (const canvas of [baseCanvas, overlay, draft, mask]) {
        canvas.width = width;
        canvas.height = height;
      }
      stage.style.width = `${width}px`;
      stage.style.height = `${height}px`;
      photoImage.style.width = `${width}px`;
      photoImage.style.height = `${height}px`;
      undo = [];
      redo = [];
      points = [];
      hasSelection = false;
      selectionVersion = 0;
      brushPointer = null;
      interaction.showOriginal = false;
      fit();
      if (saved) {
        if (selection) {
          mc.globalCompositeOperation = 'source-over';
          mc.drawImage(selection, 0, 0, mask.width, mask.height);
        }
        undo = [...saved.undo];
        redo = structuredClone(saved.redo);
        points = saved.points.map(point => ({ ...point }));
        interaction = { ...saved.interaction, busy: interaction.busy, selectedLayerId: interaction.selectedLayerId };
        hasSelection = saved.hasSelection;
        selectionVersion = saved.selectionVersion;
        if (saved.camera.fitMode) fit();
        else {
          camera = {
            ...saved.camera,
            zoom: saved.photoZoom * ratio(),
            panX: saved.camera.panX + (viewport.clientWidth - saved.viewportWidth) / 2,
            panY: saved.camera.panY + (viewport.clientHeight - saved.viewportHeight) / 2,
            fitMode: false,
          };
          applyCamera();
        }
        ports.restoreInteraction?.({ ...interaction });
      }
    }
    const patchImages = next.legacyLayers || [],
      expected = new Set(patchImages.map(value => value.image));
    for (const child of Array.from(layerStack.children)) if (!expected.has(child as HTMLImageElement)) child.remove();
    patchImages.forEach(({ layer, image: patch }, index) => {
      if (layerStack.children[index] !== patch) layerStack.append(patch);
      patch.hidden = !layer.visible || !!layer.discarded;
      patch.style.left = `${(layer.x || 0) / ratio()}px`;
      patch.style.top = `${(layer.y || 0) / (document!.height / baseCanvas.height)}px`;
      patch.style.width = `${(layer.width || patch.naturalWidth || patch.width) / ratio()}px`;
      patch.style.height = `${(layer.height || patch.naturalHeight || patch.height) / (document!.height / baseCanvas.height)}px`;
    });
    paintMask();
    if (points.length) penDraft();
    paintPhoto();
    publish();
    return true;
  }
  async function undoSelection(redoRequested = false) {
    if (!document || interaction.busy || historyBusy) return false;
    endGesture();
    if (!redoRequested && points.length) {
      redo.push({ kind: 'point', point: points.pop()! });
      selectionVersion++;
      penDraft();
      publish();
      return true;
    }
    const next = redoRequested ? redo.at(-1) : null;
    if (redoRequested && next?.kind === 'point') {
      points.push({ ...next.point });
      redo.pop();
      selectionVersion++;
      penDraft();
      publish();
      return true;
    }
    const encoded = redoRequested ? (next?.kind === 'mask' ? next.mask : null) : undo.at(-1);
    if (!encoded) return false;
    const current = mask.toDataURL('image/png'),
      sid = document.id,
      revision = document.revision,
      epoch = navigationEpoch;
    historyBusy = true;
    publish();
    try {
      const restored = await image(encoded);
      if (epoch !== navigationEpoch || !sameDocument(sid, revision)) return false;
      mc.clearRect(0, 0, mask.width, mask.height);
      mc.globalCompositeOperation = 'source-over';
      mc.drawImage(restored, 0, 0);
      if (redoRequested) {
        undo.push(current);
        redo.pop();
      } else {
        undo.pop();
        redo.push({ kind: 'mask', mask: current });
      }
      refreshMask();
      return true;
    } catch (error) {
      if (epoch === navigationEpoch)
        ports.report?.(error instanceof Error ? error.message : 'Could not restore selection', true);
      return false;
    } finally {
      if (epoch === navigationEpoch) {
        historyBusy = false;
        publish();
      }
    }
  }
  function setInteractionState(change: Partial<CanvasInteraction>) {
    const next = { ...interaction, ...change };
    next.brushSize = Math.max(1, Math.min(2000, Math.round(next.brushSize)));
    if ((Object.keys(next) as Array<keyof CanvasInteraction>).every(key => next[key] === interaction[key])) return;
    if (next.tool !== interaction.tool) {
      endGesture();
      cancelMove();
      if (points.length) selectionVersion++;
      points = [];
      dc.clearRect(0, 0, draft.width, draft.height);
    }
    if (next.busy && !interaction.busy) endGesture();
    interaction = next;
    paintMask();
    paintPhoto();
    updateCursor();
    publish();
  }
  function selectionPayload() {
    const monochrome = ownerDocument.createElement('canvas');
    monochrome.width = mask.width;
    monochrome.height = mask.height;
    const context = monochrome.getContext('2d')!;
    context.fillStyle = 'black';
    context.fillRect(0, 0, mask.width, mask.height);
    context.drawImage(mask, 0, 0);
    return monochrome.toDataURL('image/png').split(',')[1];
  }
  function commitSelection(expectedDocumentId?: string) {
    if (expectedDocumentId && document?.id !== expectedDocumentId) return false;
    clearSelection();
    undo = [];
    redo = [];
    publish();
    return true;
  }
  function fullMaskPayload() {
    if (!document) throw Error('Open an image before adding a mask.');
    const full = ownerDocument.createElement('canvas');
    full.width = document.width;
    full.height = document.height;
    const context = full.getContext('2d')!;
    context.fillStyle = 'white';
    context.fillRect(0, 0, full.width, full.height);
    return full.toDataURL('image/png').split(',')[1];
  }
  const bindings: Array<() => void> = [];
  function bind<K extends keyof HTMLElementEventMap>(
    name: K,
    handler: (event: HTMLElementEventMap[K]) => void,
    options?: AddEventListenerOptions,
  ) {
    viewport.addEventListener(name, handler as EventListener, options);
    bindings.push(() => viewport.removeEventListener(name, handler as EventListener, options));
  }
  bind('pointerdown', event => {
    void pointerDown(event);
  });
  bind('pointermove', pointerMove);
  bind('pointerup', pointerUp);
  bind('pointercancel', pointerCancel);
  bind('lostpointercapture', lostCapture);
  bind('pointerleave', pointerLeave);
  bind('wheel', wheel, { passive: false });
  bind('auxclick', event => {
    if (event.button === 1) event.preventDefault();
  });
  const observer = new ResizeObserver(resize);
  observer.observe(viewport);
  const visibility = () => {
    if (ownerDocument.hidden) resetTransientInput();
  };
  ownerWindow.addEventListener('blur', resetTransientInput);
  ownerDocument.addEventListener('visibilitychange', visibility);
  getSnapshot();
  return {
    elements,
    getSnapshot,
    subscribe: (listener: () => void) => {
      listeners.add(listener);
      return () => {
        listeners.delete(listener);
      };
    },
    presentDocument,
    setInteractionState,
    fit: () => {
      endGesture();
      fit();
    },
    setPhotoZoom,
    actualSize: () => setPhotoZoom(1),
    zoomIn: () => setPhotoZoom(photoZoom() * 1.25),
    zoomOut: () => setPhotoZoom(photoZoom() / 1.25),
    resize,
    // Choose selection-vs-backend history before awaiting. A false result on
    // decode failure/stale navigation must never trigger a backend undo retry.
    clearSelection,
    commitSelection,
    fullMaskPayload,
    finishPen,
    selectionPayload,
    undoSelection: () => undoSelection(false),
    redoSelection: () => undoSelection(true),
    cancelPen: () => {
      if (points.length) selectionVersion++;
      points = [];
      penDraft();
      publish();
    },
    setSpaceHeld,
    resetTransientInput,
    cancelGesture: () => {
      cancelMove();
      endGesture();
    },
    endGesture,
    setBrushSize: (value: number) => setInteractionState({ brushSize: value }),
    stepBrushSize: (direction: number) => {
      const value = interaction.brushSize,
        next =
          direction > 0
            ? BRUSH_STEPS.find(size => size > value)
            : [...BRUSH_STEPS].reverse().find(size => size < value);
      setInteractionState({ brushSize: next ?? (direction > 0 ? 2000 : 1) });
    },
    rememberCurrentView,
    pendingSelection: (id: string) =>
      id === document?.id
        ? hasSelection || points.length > 0
        : !!views.get(id)?.hasSelection || !!views.get(id)?.points.length,
    pendingFingerprint: (id: string) =>
      id === document?.id
        ? `${id}:${selectionVersion}:${hasSelection ? 1 : 0}:${points.length}`
        : views.has(id)
          ? `${id}:${views.get(id)!.selectionVersion}:${views.get(id)!.hasSelection ? 1 : 0}:${views.get(id)!.points.length}`
          : `${id}:absent`,
    forgetDocument: (id: string) => {
      if (document?.id === id) void presentDocument(null);
      views.delete(id);
    },
    focus: () => viewport.focus({ preventScroll: true }),
    dispose: () => {
      disposed = true;
      ++navigationEpoch;
      cancelMove();
      endGesture();
      observer.disconnect();
      for (const unbind of bindings) unbind();
      ownerWindow.removeEventListener('blur', resetTransientInput);
      ownerDocument.removeEventListener('visibilitychange', visibility);
      listeners.clear();
      assetCache.clear();
      views.clear();
      if (owners.get(viewport) === owner) owners.delete(viewport);
    },
  };
}
export type CanvasController = ReturnType<typeof createCanvasController>;
