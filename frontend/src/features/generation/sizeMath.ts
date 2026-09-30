/** Exact pure geometry extracted from backend/frontend/generation-size.js.
 * Only TypeScript annotations and exports differ; no DOM or workflow adapters. */
import type { SizeLimits, SizeMath } from './types.ts';

type Axis = 'width' | 'height';
interface AxisLimits { step: number; min: number; max: number }
interface CanvasLimits { width: AxisLimits; height: AxisLimits; pixels: number }
interface DimensionValues { width: number; height: number; ratio?: number; locked?: boolean; axis?: Axis; fallbackWidth?: number; fallbackHeight?: number }
interface Candidate { width: number; height: number; error: number }

const finite = (value: unknown, fallback: number) => Number.isFinite(Number(value)) && Number(value) > 0 ? Number(value) : fallback;
const dimensionValue = (value: unknown, fallback: number) => finite(value, fallback) <= Number.MAX_SAFE_INTEGER ? finite(value, fallback) : fallback;
export function sizeLimits(metadata: SizeLimits = {}): CanvasLimits {
  const dimension = (axis: Axis) => {
    const source = metadata[axis] || {};
    const step = Math.max(1, Math.round(finite(source.step, finite(metadata.dimension_step, 1))));
    const min = Math.ceil(finite(source.min, finite(metadata.min_dimension, 1)) / step) * step;
    const max = Math.max(min, Math.floor(finite(source.max, finite(metadata.max_dimension, Infinity)) / step) * step);
    return { step, min, max };
  };
  return { width: dimension('width'), height: dimension('height'), pixels: finite(metadata.max_pixels, Infinity) };
}
const snap = (value: number, limits: AxisLimits) => Math.max(limits.min, Math.min(limits.max, Math.round(value / limits.step) * limits.step));
function linkedCandidates(ratio: number, limits: CanvasLimits, desiredWidth = 1024): Candidate[] {
  const candidates: Candidate[] = [];
  const reportedMaximum = Math.min(limits.width.max, limits.height.max * ratio, Math.sqrt(limits.pixels * ratio));
  // Without a reported ceiling, search finite neighborhoods rather than
  // walking every pixel from zero to an arbitrarily large typed value.
  // These search extents are never exposed as input caps.
  const minimumWidth = Math.max(limits.width.min, limits.height.min * ratio);
  const widths = new Set<number>();
  const addWidth = (width: number) => { if (Number.isSafeInteger(width) && width >= limits.width.min && width <= reportedMaximum) widths.add(width); };
  if (Number.isFinite(reportedMaximum) && (reportedMaximum - limits.width.min) / limits.width.step <= 65536) {
    for (let width = limits.width.min; width <= reportedMaximum; width += limits.width.step) addWidth(width);
  } else {
    const centers = [minimumWidth, desiredWidth];if (Number.isFinite(reportedMaximum)) centers.push(reportedMaximum);
    for (const center of centers) {
      const widthUnits = Math.round(center / limits.width.step), heightUnits = Math.round(center / ratio / limits.height.step);
      for (let offset = -128; offset <= 128; offset++) {
        addWidth((widthUnits + offset) * limits.width.step);
        addWidth(Math.round((heightUnits + offset) * limits.height.step * ratio / limits.width.step) * limits.width.step);
      }
    }
  }
  for (const width of widths) {
    const ideal = width / ratio / limits.height.step;
    for (const units of new Set([Math.floor(ideal), Math.ceil(ideal)])) {
      const height = units * limits.height.step;
      if (height < limits.height.min || height > limits.height.max || width * height > limits.pixels) continue;
      candidates.push({ width, height, error: Math.abs(Math.log(width / height / ratio)) });
    }
  }
  const exact = candidates.filter(candidate => candidate.error < 1e-9);
  return exact.length ? exact : candidates;
}
export function fitDimensions(values: DimensionValues, metadata: SizeLimits = {}) {
  const limits = sizeLimits(metadata), axis = values.axis === 'height' ? 'height' : 'width';
  const width = dimensionValue(values.width, dimensionValue(values.fallbackWidth, 1024));
  const height = dimensionValue(values.height, dimensionValue(values.fallbackHeight, 1024));
  const ratio = finite(values.ratio, width / height);
  if (values.locked) {
    const candidates = linkedCandidates(ratio, limits, axis === 'height' ? height * ratio : width), desired = axis === 'height' ? height : width;
    if (candidates.length) {
      candidates.sort((a, b) => {
        const score = (candidate: Candidate) => Math.abs(Math.log(candidate[axis] / desired)) + candidate.error * 4;
        return score(a) - score(b) || a.error - b.error || a.width * a.height - b.width * b.height;
      });
      return { width: candidates[0].width, height: candidates[0].height };
    }
  }
  // Independent dimensions retain the other dimension, then cap the edited
  // side against the actual pixel budget as well as the per-side maximum.
  const result = { width: snap(width, limits.width), height: snap(height, limits.height) };
  const other = axis === 'width' ? 'height' : 'width';
  result[other] = Math.min(result[other], Math.floor(limits.pixels / limits[axis].min / limits[other].step) * limits[other].step);
  result[axis] = Math.min(result[axis], Math.floor(limits.pixels / result[other] / limits[axis].step) * limits[axis].step);
  return result;
}
export function dimensionBounds(values: DimensionValues, metadata: SizeLimits = {}) {
  const limits = sizeLimits(metadata);
  if (values.locked) {
    const ratio = finite(values.ratio, finite(values.width / values.height, 1));
    const knownMaximum = Math.min(limits.width.max, limits.height.max * ratio, Math.sqrt(limits.pixels * ratio));
    const candidates = linkedCandidates(ratio, limits, finite(values.width, 1024));
    const increment = (axis: Axis) => {
      const sorted = [...new Set(candidates.map(value => value[axis]))].sort((a, b) => a - b);
      return sorted.length > 1 ? Math.min(...sorted.slice(1).map((value, index) => value - sorted[index])) : limits[axis].step;
    };
    if (candidates.length) return {
      minWidth: Math.min(...candidates.map(value => value.width)), maxWidth: Number.isFinite(knownMaximum) ? Math.max(...candidates.map(value => value.width)) : Infinity,
      minHeight: Math.min(...candidates.map(value => value.height)), maxHeight: Number.isFinite(knownMaximum) ? Math.max(...candidates.map(value => value.height)) : Infinity,
      widthStep: increment('width'), heightStep: increment('height')
    };
  }
  return {
    minWidth: limits.width.min, minHeight: limits.height.min, widthStep: limits.width.step, heightStep: limits.height.step,
    maxWidth: Math.min(limits.width.max, Math.floor(limits.pixels / finite(values.height, limits.height.min) / limits.width.step) * limits.width.step),
    maxHeight: Math.min(limits.height.max, Math.floor(limits.pixels / finite(values.width, limits.width.min) / limits.height.step) * limits.height.step)
  };
}
export const sizeMath = Object.freeze({ sizeLimits, fitDimensions, dimensionBounds }) satisfies SizeMath;
