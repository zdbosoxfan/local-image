import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
import { readFileSync } from 'node:fs';
import { sizeMath } from './sizeMath.ts';

const require = createRequire(import.meta.url);
const legacy = require('../../../../tests/fixtures/legacy-frontend-e42/generation-size.cjs');
const large = { min_dimension: 256, max_dimension: 4096, dimension_step: 32, max_pixels: 4194304 };
const small = { min_dimension: 256, max_dimension: 1536, dimension_step: 64, max_pixels: 1048576 };
const connected = { min_dimension: 16, max_dimension: 16384, dimension_step: 16, max_pixels: null };
function parity(values, limits) {
  for (const name of ['fitDimensions', 'dimensionBounds'])
    assert.deepEqual(
      sizeMath[name](values, limits),
      legacy[name](values, limits),
      name + ' parity for ' + JSON.stringify({ values, limits }),
    );
  assert.deepEqual(sizeMath.sizeLimits(limits), legacy.sizeLimits(limits));
}

test('exact parity for all existing 270 mixed-limit inputs and their linked bounds', () => {
  let count = 0;
  for (const limits of [small, large, { ...large, dimension_step: 16, max_pixels: 2097152 }])
    for (const ratio of [1, 3 / 2, 2 / 3, 16 / 9, 1000 / 733])
      for (const axis of ['width', 'height'])
        for (const size of [0, 1, 257, 512, 1024, 1695, 4096, Infinity, 99999]) {
          parity({ width: size, height: size, ratio, axis, locked: true }, limits);
          count++;
        }
  assert.equal(count, 270);
});

test('preserves uncapped 4K/8K, missing ceilings, per-axis grids and huge-input fallbacks', () => {
  const fixtures = [
    [{ width: 3840, height: 2160, ratio: 16 / 9, locked: true }, connected],
    [{ width: 4096, height: 2304, ratio: 16 / 9, locked: true }, connected],
    [{ width: 8192, height: 8192, ratio: 1, locked: true }, connected],
    [{ width: 8000, height: 4500, ratio: 16 / 9, locked: true }, { max_pixels: null }],
    [
      { width: 4096, height: 2304, ratio: 16 / 9, locked: true },
      { width: { min: 16, max: 8192, step: 16 }, height: { min: 32, max: 4096, step: 32 }, max_pixels: null },
    ],
    [{ width: 1e9, height: 1e9, ratio: 16 / 9, locked: true }, {}],
    [{ width: 1e308, height: 1e308, ratio: 1, locked: true }, {}],
    [{ width: 1e9, height: 16, ratio: 1e9, locked: true }, {}],
    [{ width: 0, height: NaN, ratio: 1, fallbackWidth: 1920, fallbackHeight: 1080, locked: false }, {}],
  ];
  for (const [values, limits] of fixtures) parity(values, limits);
  assert.deepEqual(sizeMath.fitDimensions(fixtures[0][0], connected), { width: 3840, height: 2160 });
  assert.deepEqual(sizeMath.fitDimensions(fixtures[2][0], connected), { width: 8192, height: 8192 });
  assert.equal(sizeMath.dimensionBounds(fixtures[3][0], fixtures[3][1]).maxWidth, Infinity);
  assert.deepEqual(sizeMath.fitDimensions(fixtures[6][0]), { width: 1024, height: 1024 });
});

test('seeded per-axis limits and independent edits match all original rounding and caps', () => {
  let seed = 0x51a7e;
  const random = () => {
    seed = (Math.imul(seed, 1664525) + 1013904223) >>> 0;
    return seed / 2 ** 32;
  };
  const choices = values => values[Math.floor(random() * values.length)];
  for (let index = 0; index < 240; index++) {
    const stepWidth = choices([1, 8, 16, 32, 64]),
      stepHeight = choices([1, 8, 16, 32, 64]);
    const metadata = {
      width: { min: choices([1, 16, 255, 256]), max: choices([1024, 2048, 4096, null]), step: stepWidth },
      height: { min: choices([1, 32, 256, 257]), max: choices([1024, 1536, 3072, null]), step: stepHeight },
      max_pixels: choices([1048576, 2097152, 4194304, null]),
    };
    parity(
      {
        width: choices([0, 257, 1024, 1695, 3840, 8192]),
        height: choices([0, 255, 576, 1080, 2160, 4096]),
        ratio: choices([1, 3 / 2, 2 / 3, 16 / 9, 1000 / 733]),
        axis: choices(['width', 'height']),
        locked: choices([true, false]),
      },
      metadata,
    );
  }
});

test('retains coercion and fallback behavior without introducing DOM dependencies', () => {
  for (const value of [undefined, null, '', '1536', -1, NaN, Infinity, 0, 1e308]) {
    parity(
      { width: value, height: value, ratio: value, locked: true, fallbackWidth: 1024, fallbackHeight: 768 },
      { width: { min: value, step: value, max: value }, height: { min: 32, step: 32 }, max_pixels: value },
    );
  }
  assert.equal(Object.isFrozen(sizeMath), true);
  const source = readFileSync(new URL('./sizeMath.ts', import.meta.url), 'utf8');
  assert.equal(/\b(?:document|window|HTMLElement|ResizeObserver|querySelector|addEventListener)\b/.test(source), false);
});
