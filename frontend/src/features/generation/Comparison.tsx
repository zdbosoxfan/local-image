import { useEffect, useLayoutEffect, useRef } from 'react';
import { Button } from '@fluentui/react-components';
import { Icon } from '../shell/Icon.tsx';
import type { EditorDocument } from '../../contracts.ts';
import {
  fitPreview,
  panPreview,
  previewLayout,
  wheelZoomFactor,
  zoomPreview,
  type PreviewCamera,
} from './previewCamera.ts';
import { Hint } from '../shell/Hint.tsx';

/** A single active-stage preview. Camera changes bypass React reconciliation. */
export function Comparison({ document, label, notice }: { document?: EditorDocument; label: string; notice?: string }) {
  const area = useRef<HTMLDivElement>(null),
    image = useRef<HTMLImageElement>(null),
    zoomLabel = useRef<HTMLSpanElement>(null);
  const camera = useRef<PreviewCamera>(fitPreview()),
    cameras = useRef(new Map<string, PreviewCamera>()),
    identity = useRef('');
  const drag = useRef<{ pointerId: number; x: number; y: number; camera: PreviewCamera } | null>(null);
  const key = `${label}:${document?.id ?? ''}`;
  function sizes() {
    if (!area.current || !image.current?.naturalWidth) return null;
    return {
      image: { width: image.current.naturalWidth, height: image.current.naturalHeight },
      viewport: { width: area.current.clientWidth, height: area.current.clientHeight },
    };
  }
  function render() {
    const size = sizes();
    if (size && image.current) {
      const view = previewLayout(camera.current, size.image, size.viewport);
      Object.assign(image.current.style, {
        width: `${view.width}px`,
        height: `${view.height}px`,
        left: `${view.left}px`,
        top: `${view.top}px`,
        visibility: 'visible',
      });
    }
    if (zoomLabel.current)
      zoomLabel.current.textContent =
        camera.current.scale === 'fit' ? 'Fit' : `${Math.round(camera.current.scale * 100)}%`;
  }
  function zoom(factor: number, anchor?: { x: number; y: number }) {
    const size = sizes();
    if (!size) return;
    endDrag();
    camera.current = zoomPreview(camera.current, size.image, size.viewport, factor, anchor);
    render();
  }
  function endDrag(pointerId = drag.current?.pointerId) {
    if (pointerId !== undefined && area.current?.hasPointerCapture(pointerId))
      area.current.releasePointerCapture(pointerId);
    drag.current = null;
    if (area.current) area.current.dataset.panning = 'false';
  }
  useLayoutEffect(() => {
    if (identity.current) cameras.current.set(identity.current, camera.current);
    if (cameras.current.size > 20) cameras.current.delete(cameras.current.keys().next().value!);
    identity.current = key;
    camera.current = cameras.current.get(key) ?? fitPreview();
    endDrag();
    render();
  }, [key]);
  useEffect(() => {
    const viewport = area.current;
    if (!viewport) return;
    const observer = new ResizeObserver(render);
    observer.observe(viewport);
    const wheel = (event: WheelEvent) => {
      if (!sizes()) return;
      event.preventDefault();
      event.stopPropagation();
      const box = viewport.getBoundingClientRect();
      zoom(wheelZoomFactor(event.deltaY, event.deltaMode, viewport.clientHeight), {
        x: event.clientX - box.left,
        y: event.clientY - box.top,
      });
    };
    viewport.addEventListener('wheel', wheel, { passive: false });
    render();
    return () => {
      observer.disconnect();
      viewport.removeEventListener('wheel', wheel);
    };
  }, []);
  const empty = label === 'Draft' ? 'Choose or generate a draft.' : 'Choose a draft, then refine it.';
  return (
    <section className="li-generation-comparison" aria-label={`${label} viewer`}>
      <div className="li-generation-comparison-tools">
        <span className="li-generation-preview-title" title={document?.name}>
          {label}
          {document ? ` · ${document.width} × ${document.height}` : ''}
        </span>
        <Hint content="Fit image (F)" relationship="description">
          <Button
            size="small"
            appearance="subtle"
            disabled={!document}
            onClick={() => {
              camera.current = fitPreview();
              render();
            }}
          >
            Fit
          </Button>
        </Hint>
        <Hint content="Actual size (1)" relationship="description">
          <Button
            size="small"
            appearance="subtle"
            disabled={!document}
            onClick={() => {
              camera.current.scale = 1;
              render();
            }}
          >
            100%
          </Button>
        </Hint>
        <Hint content="Zoom out (−)" relationship="description">
          <Button
            size="small"
            appearance="subtle"
            className="li-generation-icon-button"
            aria-label="Zoom preview out"
            disabled={!document}
            icon={<Icon name="subtract" />}
            onClick={() => zoom(1 / 1.25)}
          />
        </Hint>
        <span ref={zoomLabel} className="li-generation-preview-zoom">
          Fit
        </span>
        <Hint content="Zoom in (+)" relationship="description">
          <Button
            size="small"
            appearance="subtle"
            className="li-generation-icon-button"
            aria-label="Zoom preview in"
            disabled={!document}
            icon={<Icon name="add" />}
            onClick={() => zoom(1.25)}
          />
        </Hint>
      </div>
      {notice && (
        <p className="li-generation-preview-notice" role="status">
          {notice}
        </p>
      )}
      <div
        ref={area}
        className="li-generation-comparison-image"
        tabIndex={0}
        aria-label={`${label}: mouse wheel to zoom; drag or arrow keys to pan; F to fit; 1 for actual size`}
        onPointerDown={event => {
          if (event.button !== 0 || !sizes()) return;
          event.currentTarget.focus({ preventScroll: true });
          drag.current = {
            pointerId: event.pointerId,
            x: event.clientX,
            y: event.clientY,
            camera: { ...camera.current },
          };
          event.currentTarget.setPointerCapture(event.pointerId);
          event.currentTarget.dataset.panning = 'true';
          event.preventDefault();
        }}
        onPointerMove={event => {
          const held = drag.current,
            size = sizes();
          if (!held || held.pointerId !== event.pointerId || !size) return;
          camera.current = panPreview(
            held.camera,
            size.image,
            size.viewport,
            event.clientX - held.x,
            event.clientY - held.y,
          );
          render();
        }}
        onPointerUp={event => endDrag(event.pointerId)}
        onPointerCancel={event => endDrag(event.pointerId)}
        onLostPointerCapture={() => endDrag()}
        onKeyDown={event => {
          if (!['+', '=', '-', '1', 'f', 'F', 'ArrowLeft', 'ArrowRight', 'ArrowUp', 'ArrowDown'].includes(event.key))
            return;
          event.preventDefault();
          event.stopPropagation();
          if (event.key === '1') camera.current.scale = 1;
          else if (event.key.toLowerCase() === 'f') camera.current = fitPreview();
          else if (['+', '=', '-'].includes(event.key)) zoom(event.key === '-' ? 1 / 1.25 : 1.25);
          else {
            const size = sizes();
            if (size)
              camera.current = panPreview(
                camera.current,
                size.image,
                size.viewport,
                event.key === 'ArrowLeft' ? 40 : event.key === 'ArrowRight' ? -40 : 0,
                event.key === 'ArrowUp' ? 40 : event.key === 'ArrowDown' ? -40 : 0,
              );
          }
          render();
        }}
      >
        {document ? (
          <img
            key={`${document.id}:${document.revision}`}
            ref={image}
            style={{ visibility: 'hidden' }}
            alt={`${label}: ${document.name}`}
            src={`/api/local-remove/session/${encodeURIComponent(document.id)}/preview?revision=${document.revision}&full=true`}
            draggable={false}
            onLoad={render}
          />
        ) : (
          <p>{empty}</p>
        )}
      </div>
    </section>
  );
}
