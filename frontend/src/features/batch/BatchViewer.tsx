import { useEffect, useLayoutEffect, useRef, useState } from 'react';
import {
  Button,
  DialogBody,
  DialogContent,
  DialogTitle,
  Field,
  MessageBar,
  MessageBarBody,
  ToggleButton,
} from '@fluentui/react-components';
import { Icon } from '../shell/Icon.tsx';
import { Hint } from '../shell/Hint.tsx';
import { ChoiceSelect } from '../shell/ChoiceSelect.tsx';
import {
  fitPreview,
  panPreview,
  previewLayout,
  wheelZoomFactor,
  zoomPreview,
  type PreviewCamera,
} from '../generation/previewCamera.ts';
import type { BatchController } from './controller.ts';
import type { BatchSnapshot, BatchItem } from './contracts.ts';

export function BatchViewer({
  controller,
  state,
  item,
}: {
  controller: BatchController;
  state: BatchSnapshot;
  item: BatchItem;
}) {
  const area = useRef<HTMLDivElement>(null),
    image = useRef<HTMLImageElement>(null),
    close = useRef<HTMLButtonElement>(null);
  const camera = useRef<PreviewCamera>(fitPreview());
  const drag = useRef<{ id: number; x: number; y: number; camera: PreviewCamera } | null>(null);
  const [zoomValue, setZoomValue] = useState('fit'),
    [error, setError] = useState('');
  const inspection = state.inspection!;
  const items = state.active!.items.filter(item => item.preview);
  const index = items.findIndex(candidate => candidate.id === item.id);
  function sizes() {
    if (!area.current || !image.current?.naturalWidth) return null;
    return {
      image: { width: image.current.naturalWidth, height: image.current.naturalHeight },
      viewport: { width: area.current.clientWidth, height: area.current.clientHeight },
    };
  }
  function render() {
    const size = sizes();
    if (!size || !image.current) return;
    const view = previewLayout(camera.current, size.image, size.viewport);
    Object.assign(image.current.style, {
      width: `${view.width}px`,
      height: `${view.height}px`,
      left: `${view.left}px`,
      top: `${view.top}px`,
      visibility: 'visible',
    });
    setZoomValue(camera.current.scale === 'fit' ? 'fit' : String(Math.round(camera.current.scale * 100)));
  }
  function endDrag() {
    if (drag.current && area.current?.hasPointerCapture(drag.current.id))
      area.current.releasePointerCapture(drag.current.id);
    drag.current = null;
    if (area.current) area.current.dataset.panning = 'false';
  }
  function zoom(factor: number, anchor?: { x: number; y: number }) {
    const size = sizes();
    if (!size) return;
    endDrag();
    camera.current = zoomPreview(camera.current, size.image, size.viewport, factor, anchor);
    render();
  }
  function setZoom(value: 'fit' | '100' | '200') {
    endDrag();
    camera.current = value === 'fit' ? fitPreview() : { ...camera.current, scale: Number(value) / 100 };
    controller.setInspection({ zoom: value });
    render();
  }
  useLayoutEffect(() => {
    camera.current = fitPreview();
    endDrag();
    setError('');
    render();
  }, [item.id]);
  useEffect(() => {
    close.current?.focus();
  }, []);
  useEffect(() => {
    const viewport = area.current;
    if (!viewport) return;
    const observer = new ResizeObserver(render);
    observer.observe(viewport);
    const wheel = (event: WheelEvent) => {
      event.preventDefault();
      event.stopPropagation();
      const box = viewport.getBoundingClientRect();
      zoom(wheelZoomFactor(event.deltaY, event.deltaMode, viewport.clientHeight), {
        x: event.clientX - box.left,
        y: event.clientY - box.top,
      });
    };
    viewport.addEventListener('wheel', wheel, { passive: false });
    return () => {
      observer.disconnect();
      viewport.removeEventListener('wheel', wheel);
      endDrag();
    };
  }, []);
  return (
    <DialogBody className="li-batch-viewer-body">
      <DialogTitle
        action={
          <Hint content="Close image viewer (Esc)" relationship="description">
            <Button
              ref={close}
              id="batch-inspect-back"
              appearance="subtle"
              aria-label="Close image viewer"
              icon={<Icon name="close" />}
              onClick={controller.closeInspection}
            />
          </Hint>
        }
      >
        <Hint content={item.name} relationship="description">
          <span id="batch-inspect-name">{item.name}</span>
        </Hint>
      </DialogTitle>
      <div className="li-batch-viewer-toolbar">
        <Hint content="Previous image">
          <Button
            aria-label="Previous batch image"
            disabled={index <= 0}
            icon={<Icon name="previous" />}
            onClick={() => controller.inspect(items[index - 1].id)}
          />
        </Hint>
        <span>
          {index + 1} of {items.length}
        </span>
        <Hint content="Next image">
          <Button
            aria-label="Next batch image"
            disabled={index >= items.length - 1}
            icon={<Icon name="next" />}
            onClick={() => controller.inspect(items[index + 1].id)}
          />
        </Hint>
        <ToggleButton
          id="batch-inspect-original"
          checked={inspection.original}
          onClick={() => controller.setInspection({ original: !inspection.original })}
        >
          Original
        </ToggleButton>
        <Field label="Zoom" orientation="horizontal">
          <ChoiceSelect
            id="batch-inspect-zoom"
            label="Batch review zoom"
            value={zoomValue}
            choices={[
              { value: 'fit', label: 'Fit' },
              { value: '100', label: '100%' },
              { value: '200', label: '200%' },
              ...(!['fit', '100', '200'].includes(zoomValue) ? [{ value: zoomValue, label: `${zoomValue}%` }] : []),
            ]}
            onSelect={value => setZoom(value as 'fit' | '100' | '200')}
          />
        </Field>
        <Hint content="Zoom out">
          <Button aria-label="Zoom batch image out" icon={<Icon name="zoom-out" />} onClick={() => zoom(1 / 1.25)} />
        </Hint>
        <Hint content="Zoom in">
          <Button aria-label="Zoom batch image in" icon={<Icon name="zoom-in" />} onClick={() => zoom(1.25)} />
        </Hint>
      </div>
      <DialogContent className="li-batch-viewer-content">
        {error && (
          <MessageBar intent="error">
            <MessageBarBody>{error}</MessageBarBody>
          </MessageBar>
        )}
        <div
          ref={area}
          id="batch-inspect-scroll"
          className="li-batch-viewer-viewport"
          tabIndex={0}
          aria-label="Image viewer: wheel to zoom, drag or arrow keys to pan, F to fit, 1 for actual size"
          onPointerDown={event => {
            if (event.button !== 0 || !sizes()) return;
            event.currentTarget.focus({ preventScroll: true });
            drag.current = { id: event.pointerId, x: event.clientX, y: event.clientY, camera: { ...camera.current } };
            event.currentTarget.setPointerCapture(event.pointerId);
            event.currentTarget.dataset.panning = 'true';
            event.preventDefault();
          }}
          onPointerMove={event => {
            const current = drag.current,
              size = sizes();
            if (!current || current.id !== event.pointerId || !size) return;
            camera.current = panPreview(
              current.camera,
              size.image,
              size.viewport,
              event.clientX - current.x,
              event.clientY - current.y,
            );
            render();
          }}
          onPointerUp={endDrag}
          onPointerCancel={endDrag}
          onLostPointerCapture={() => {
            drag.current = null;
            if (area.current) area.current.dataset.panning = 'false';
          }}
          onDoubleClick={() => setZoom(camera.current.scale === 'fit' ? '100' : 'fit')}
          onKeyDown={event => {
            if (event.key.toLowerCase() === 'f') setZoom('fit');
            else if (event.key === '1') setZoom('100');
            else if (event.key === '+' || event.key === '=') zoom(1.25);
            else if (event.key === '-') zoom(1 / 1.25);
            else if (['ArrowLeft', 'ArrowRight', 'ArrowUp', 'ArrowDown'].includes(event.key)) {
              const size = sizes();
              if (size)
                camera.current = panPreview(
                  camera.current,
                  size.image,
                  size.viewport,
                  event.key === 'ArrowLeft' ? 40 : event.key === 'ArrowRight' ? -40 : 0,
                  event.key === 'ArrowUp' ? 40 : event.key === 'ArrowDown' ? -40 : 0,
                );
              render();
            } else return;
            event.preventDefault();
            event.stopPropagation();
          }}
        >
          <img
            ref={image}
            id="batch-inspect-image"
            draggable={false}
            style={{ visibility: 'hidden' }}
            src={controller.api.previewUrl(state.active!.id, item.id, true, inspection.original)}
            alt={`Full-size ${inspection.original ? 'original' : 'cutout'} preview of ${item.name}`}
            onLoad={() => {
              setError('');
              render();
            }}
            onError={() => setError('This preview could not be loaded. Close the viewer and try again.')}
          />
        </div>
      </DialogContent>
      <p className="li-batch-note">
        {inspection.original ? 'Original' : 'Processed result'} · Wheel to zoom · Drag to pan
      </p>
    </DialogBody>
  );
}
