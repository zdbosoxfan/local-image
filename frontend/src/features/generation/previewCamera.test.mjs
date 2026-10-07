import { test } from 'node:test';
import assert from 'node:assert/strict';
import {
  fitPreview,
  panPreview,
  previewLayout,
  refinementPreview,
  wheelZoomFactor,
  zoomPreview,
} from './previewCamera.ts';

const viewport = { width: 960, height: 720 },
  image = { width: 3840, height: 2160 };
const close = (actual, expected) => assert.ok(Math.abs(actual - expected) < 1e-8, `${actual} != ${expected}`);
const pixelAt = (camera, anchor, dimensions = image) => {
  const view = previewLayout(camera, dimensions, viewport);
  return { x: (anchor.x - view.left) / view.scale, y: (anchor.y - view.top) / view.scale };
};

test('Fit contains landscape and portrait images without cropping or distortion', () => {
  assert.deepEqual(previewLayout(fitPreview(), image, viewport), {
    scale: 0.25,
    width: 960,
    height: 540,
    left: 0,
    top: 90,
  });
  const portrait = previewLayout(fitPreview(), { width: 3072, height: 4096 }, viewport);
  close(portrait.width, 540);
  close(portrait.height, 720);
  close(portrait.left, 210);
  close(portrait.top, 0);
  const resized = previewLayout(fitPreview(), image, { width: 480, height: 360 });
  close(resized.scale, 0.125);
  close(resized.width / resized.height, image.width / image.height);
});

test('Wheel zoom anchors the same source pixel for 4K, low resolution and portrait images', () => {
  for (const dimensions of [image, { width: 500, height: 500 }, { width: 1536, height: 2048 }]) {
    for (const anchor of [
      { x: 480, y: 360 },
      { x: 600, y: 460 },
      { x: 360, y: 260 },
    ]) {
      const before = fitPreview(),
        pixel = pixelAt(before, anchor, dimensions);
      const zoomed = zoomPreview(before, dimensions, viewport, 1.5, anchor);
      const nextPixel = pixelAt(zoomed, anchor, dimensions);
      close(nextPixel.x, pixel.x);
      close(nextPixel.y, pixel.y);
      const restored = zoomPreview(zoomed, dimensions, viewport, 1 / 1.5, anchor);
      close(restored.x, before.x);
      close(restored.y, before.y);
      close(previewLayout(restored, dimensions, viewport).scale, previewLayout(before, dimensions, viewport).scale);
    }
  }
});

test('Dragging moves the displayed image by pointer distance and bounds the image center', () => {
  const before = { scale: 1, x: 0.5, y: 0.5 },
    moved = panPreview(before, image, viewport, 160, -100);
  const first = previewLayout(before, image, viewport),
    next = previewLayout(moved, image, viewport);
  close(next.left - first.left, 160);
  close(next.top - first.top, -100);
  assert.equal(moved.scale, 1);
  const far = panPreview(before, image, viewport, 1e9, -1e9);
  assert.equal(far.x, 0);
  assert.equal(far.y, 1);
  assert.deepEqual(before, { scale: 1, x: 0.5, y: 0.5 });
});

test('Wheel units normalize and magnification remains bounded for extreme inputs', () => {
  close(wheelZoomFactor(1, 1, 720), wheelZoomFactor(16, 0, 720));
  close(wheelZoomFactor(0.25, 2, 720), wheelZoomFactor(180, 0, 720));
  assert.ok(wheelZoomFactor(-100, 0, 720) > 1);
  assert.ok(wheelZoomFactor(100, 0, 720) < 1);
  assert.ok(Number.isFinite(wheelZoomFactor(-1e300, 0, 720)));
  assert.equal(zoomPreview(fitPreview(), image, viewport, 1e6).scale, 8);
  assert.equal(zoomPreview(fitPreview(), image, viewport, 1e-6).scale, 0.02);
  const before = fitPreview();
  assert.equal(zoomPreview(before, image, viewport, NaN), before);
});

test('Draft and Refine expose one selected document and preserve the 4K final across switches', () => {
  const draft = { id: 'draft', name: '500 px draft', width: 500, height: 500 },
    final = { id: 'final-4k', name: '4K final', width: 3840, height: 3840 };
  for (let count = 0; count < 30; count++) {
    assert.equal(refinementPreview('draft', draft, final).document, draft);
    const refined = refinementPreview('final', draft, final);
    assert.equal(refined.document, final);
    assert.equal(refined.notice, '');
  }
  assert.equal(final.width, 3840);
  assert.equal(draft.width, 500);
  const pending = refinementPreview('final', draft);
  assert.equal(pending.document, draft);
  assert.match(pending.notice, /Showing the draft/);
  assert.equal(refinementPreview('final').document, undefined);
});
