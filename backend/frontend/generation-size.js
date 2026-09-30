/* Linked canvas dimensions shared by Create, Edit, Draft and Refine. */
(() => {
  'use strict';
  const finite = (value, fallback) => Number.isFinite(Number(value)) && Number(value) > 0 ? Number(value) : fallback;
  const dimensionValue = (value, fallback) => finite(value, fallback) <= Number.MAX_SAFE_INTEGER ? finite(value, fallback) : fallback;
  function sizeLimits(metadata = {}) {
    const dimension = axis => {
      const source = metadata[axis] || {};
      const step = Math.max(1, Math.round(finite(source.step, finite(metadata.dimension_step, 1))));
      const min = Math.ceil(finite(source.min, finite(metadata.min_dimension, 1)) / step) * step;
      const max = Math.max(min, Math.floor(finite(source.max, finite(metadata.max_dimension, Infinity)) / step) * step);
      return { step, min, max };
    };
    return { width: dimension('width'), height: dimension('height'), pixels: finite(metadata.max_pixels, Infinity) };
  }
  const snap = (value, limits) => Math.max(limits.min, Math.min(limits.max, Math.round(value / limits.step) * limits.step));
  function linkedCandidates(ratio, limits, desiredWidth = 1024) {
    const candidates = [];
    const reportedMaximum = Math.min(limits.width.max, limits.height.max * ratio, Math.sqrt(limits.pixels * ratio));
    // Without a reported ceiling, search finite neighborhoods rather than
    // walking every pixel from zero to an arbitrarily large typed value.
    // These search extents are never exposed as input caps.
    const minimumWidth = Math.max(limits.width.min, limits.height.min * ratio);
    const widths = new Set();
    const addWidth = width => { if (Number.isSafeInteger(width) && width >= limits.width.min && width <= reportedMaximum) widths.add(width); };
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
  function fitDimensions(values, metadata = {}) {
    const limits = sizeLimits(metadata), axis = values.axis === 'height' ? 'height' : 'width';
    const width = dimensionValue(values.width, dimensionValue(values.fallbackWidth, 1024));
    const height = dimensionValue(values.height, dimensionValue(values.fallbackHeight, 1024));
    const ratio = finite(values.ratio, width / height);
    if (values.locked) {
      const candidates = linkedCandidates(ratio, limits, axis === 'height' ? height * ratio : width), desired = axis === 'height' ? height : width;
      if (candidates.length) {
        candidates.sort((a, b) => {
          const score = candidate => Math.abs(Math.log(candidate[axis] / desired)) + candidate.error * 4;
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
  function dimensionBounds(values, metadata = {}) {
    const limits = sizeLimits(metadata);
    if (values.locked) {
      const ratio = finite(values.ratio, finite(values.width / values.height, 1));
      const knownMaximum = Math.min(limits.width.max, limits.height.max * ratio, Math.sqrt(limits.pixels * ratio));
      const candidates = linkedCandidates(ratio, limits, finite(values.width, 1024));
      const increment = axis => {
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
  const math = Object.freeze({ sizeLimits, fitDimensions, dimensionBounds });
  if (typeof module !== 'undefined' && module.exports) module.exports = math;
  if (typeof document === 'undefined') return;
  if (window.LocalImageGenerationSize) return;
  const byId = id => document.getElementById(id), profiles = new Map();
  let suppress = 0, refreshing = false, activeGenerationProfile = null;
  const configurations = [
    { key: 'gen', prefix: 'gen', aspect: 'gen-aspect', model: () => generationModel(), update: () => updateGenerationControls(), run: 'gen-run', note: 'gen-size-note' },
    ...['draft', 'final'].map(stage => ({ key: stage, prefix: 'refine-' + stage, aspect: stage === 'final' ? 'refine-aspect' : null, model: () => refineModel(stage), update: () => updateRefineControls(), run: 'refine-' + stage + '-run', note: 'refine-' + stage + '-size-note' }))
  ].filter(config => byId(config.prefix + '-width') && byId(config.prefix + '-height'));
  function effectiveLimits(config) {
    const base = config.model()?.limits || {};
    const references = config.key === 'gen'
      ? (typeof generationReferences !== 'undefined' && generationReferences.length > 0)
      : config.key === 'final' || (typeof refineReferences !== 'undefined' && refineReferences.length > 0);
    return references && base.reference_dimensions ? { ...base, ...base.reference_dimensions } : base;
  }
  const profileKey = config => config.key === 'gen' ? 'gen-' + (window.LocalImageGenerationStudio?.mode() || 'create') : config.key;
  function profile(config) {
    const key = profileKey(config);
    if (!profiles.has(key)) profiles.set(key, { locked: true, ratio: 1, initialized: false, dirty: false, axis: 'width', width: 1024, height: 1024, message: '' });
    return profiles.get(key);
  }
  const values = config => ({ width: Number(byId(config.prefix + '-width').value), height: Number(byId(config.prefix + '-height').value) });
  const modeBusy = config => config.key === 'gen' && window.LocalImageGenerationStudio?.isModeBusy();
  function aspectRatio(config) {
    const selected = config.aspect && byId(config.aspect)?.value;
    if (!selected || selected === 'custom') return null;
    const [width, height] = selected.split(':').map(Number);
    return width > 0 && height > 0 ? width / height : null;
  }
  function adopt(config, state) {
    const current = values(config);
    state.ratio = aspectRatio(config) || finite(current.width / current.height, 1);
    state.width = finite(current.width, 1024);state.height = finite(current.height, 1024);
    state.initialized = true;
  }
  function showState(config) {
    const state = profile(config), current = values(config);
    const bounds = dimensionBounds({ ...current, locked: state.locked, ratio: state.ratio }, effectiveLimits(config));
    const width = byId(config.prefix + '-width'), height = byId(config.prefix + '-height');
    width.min = bounds.minWidth;width.step = bounds.widthStep;
    height.min = bounds.minHeight;height.step = bounds.heightStep;
    if (Number.isFinite(bounds.maxWidth)) width.max = bounds.maxWidth;else width.removeAttribute('max');
    if (Number.isFinite(bounds.maxHeight)) height.max = bounds.maxHeight;else height.removeAttribute('max');
    const button = byId(config.prefix + '-ratio-lock');
    button.setAttribute('aria-pressed', String(state.locked));
    button.setAttribute('aria-label', state.locked ? 'Unlock aspect ratio' : 'Lock aspect ratio');
    button.title = (state.locked ? 'Aspect ratio linked' : 'Dimensions independent') + ' · ' + current.width + ' × ' + current.height;
    button.disabled = width.disabled || height.disabled;
    const note = byId(config.note);
    note.textContent = state.message;note.hidden = !state.message;
  }
  function commit(config, { axis = null, notify = true, external = false } = {}) {
    const state = profile(config), model = config.model();
    if (!model || modeBusy(config)) return;
    if (!state.initialized || external) adopt(config, state);
    const current = values(config), inputAxis = axis || state.axis;
    const metadata = effectiveLimits(config);
    const fitted = fitDimensions({ ...current, axis: inputAxis, locked: state.locked, ratio: state.ratio, fallbackWidth: state.width, fallbackHeight: state.height }, metadata);
    byId(config.prefix + '-width').value = String(fitted.width);byId(config.prefix + '-height').value = String(fitted.height);
    const steps = byId(config.prefix + '-steps'), minSteps = model.limits?.min_steps || 1, maxSteps = model.limits?.max_steps || 100;
    const desiredSteps = steps.value === '' ? NaN : Number(steps.value), fittedSteps = Math.max(minSteps, Math.min(maxSteps, Math.round(Number.isFinite(desiredSteps) ? desiredSteps : model.defaults?.steps || minSteps)));
    steps.min = minSteps;steps.max = maxSteps;steps.step = 1;steps.value = String(fittedSteps);
    const axisAdjusted = fitted[inputAxis] !== current[inputAxis];
    state.message = notify && axisAdjusted ? 'Adjusted to ' + fitted.width + ' × ' + fitted.height + ' for this workflow.' : '';
    if (notify && desiredSteps !== fittedSteps) state.message += (state.message ? ' ' : '') + 'Steps adjusted to ' + fittedSteps + '.';
    state.width = fitted.width;state.height = fitted.height;state.dirty = false;
    state.limits = JSON.stringify(metadata);
    if (!state.locked) state.ratio = fitted.width / fitted.height;
    showState(config);
  }
  function refresh(config) {
    if (suppress || refreshing || modeBusy(config) || !config.model()) return;
    refreshing = true;
    try {
      const state = profile(config);
      if (config.key === 'gen' && activeGenerationProfile !== profileKey(config)) {
        const sameSource = profileKey(config) !== 'gen-edit' || state.sourceId === (typeof session !== 'undefined' ? session?.id : null);
        if (state.initialized && sameSource) {
          byId(config.prefix + '-width').value = String(state.width);
          byId(config.prefix + '-height').value = String(state.height);
        }
        activeGenerationProfile = profileKey(config);
      }
      const current = values(config);
      if (!state.dirty && (!state.initialized || current.width !== state.width || current.height !== state.height)) commit(config, { external: true, notify: false });
      else if (state.limits !== JSON.stringify(effectiveLimits(config))) commit(config);
      if (config.key === 'gen' && profileKey(config) === 'gen-edit') state.sourceId = typeof session !== 'undefined' ? session?.id : null;
      showState(config);
    } finally { refreshing = false; }
  }
  function update(config) { config.update();showState(config); }
  for (const config of configurations) {
    const width = byId(config.prefix + '-width'), height = byId(config.prefix + '-height');
    const button = document.createElement('button');button.type = 'button';button.id = config.prefix + '-ratio-lock';button.className = 'generation-size-lock';
    button.innerHTML = '<svg viewBox="0 0 24 24" width="18" height="18" fill="none" stroke="currentColor" stroke-width="1.7" aria-hidden="true"><path d="m9 15 6-6m-8 9H5a4 4 0 0 1-2.8-6.8l4-4A4 4 0 0 1 13 10m-2 4a4 4 0 0 0 6.8 2.8l4-4A4 4 0 0 0 19 6h-2"/></svg>';
    const row = width.closest('.generation-size-fields') || width.closest('.generation-core-fields') || width.closest('.generation-dimensions') || width.closest('.refine-fields');
    if (row) { row.classList.add('generation-linked-dimensions');height.closest('label').after(button); }
    else height.after(button);
    let note = byId(config.note);
    if (!note) { note = document.createElement('p');note.id = config.note;(row || height.parentElement).after(note); }
    note.classList.add('generation-size-note');note.setAttribute('role', 'status');note.hidden = true;
    button.onclick = () => {
      const state = profile(config);commit(config);state.locked = !state.locked;state.ratio = values(config).width / values(config).height;
      state.message = '';showState(config);update(config);
    };
    for (const axis of ['width', 'height']) {
      const input = byId(config.prefix + '-' + axis);
      input.oninput = () => {
        const state = profile(config);if (!state.initialized) adopt(config, state);
        state.dirty = true;state.axis = axis;state.message = '';
        if (!state.locked && config.aspect) byId(config.aspect).value = 'custom';
        if (config.key !== 'gen' && typeof markRefineRecipeEdited === 'function') markRefineRecipeEdited();
        update(config);
      };
      input.addEventListener('change', () => { commit(config, { axis });update(config); });
      input.addEventListener('blur', () => { if (profile(config).dirty) { commit(config, { axis });update(config); } });
      input.addEventListener('keydown', event => { if (event.key === 'Enter') { commit(config, { axis });update(config); } });
    }
    const steps = byId(config.prefix + '-steps');
    steps.addEventListener('change', () => { commit(config);update(config); });
    steps.addEventListener('blur', () => { commit(config);update(config); });
    if (config.aspect) byId(config.aspect).addEventListener('change', () => {
      const state = profile(config), ratio = aspectRatio(config);
      if (ratio) { state.ratio = ratio;state.locked = true;state.initialized = true; }
      else state.ratio = values(config).width / values(config).height;
      commit(config, { axis: 'width' });update(config);
    });
    showState(config);
  }
  const originalGenerationSync = syncGenerationModel;
  syncGenerationModel = function (...args) {
    const config = configurations.find(item => item.key === 'gen'), state = profile(config), previous = values(config), preserve = state.initialized && !modeBusy(config);
    suppress++;
    try { originalGenerationSync.apply(this, args); } finally { suppress--; }
    if (preserve) { byId('gen-width').value = previous.width;byId('gen-height').value = previous.height; }
    commit(config, { notify: preserve });update(config);
  };
  const originalRefineSync = syncRefineModel;
  syncRefineModel = function (stage, ...args) {
    const config = configurations.find(item => item.key === stage), state = profile(config), previous = values(config), preserve = state.initialized;
    suppress++;
    try { originalRefineSync.call(this, stage, ...args); } finally { suppress--; }
    if (preserve) { byId(config.prefix + '-width').value = previous.width;byId(config.prefix + '-height').value = previous.height; }
    commit(config, { notify: preserve });update(config);
  };
  const originalGenerationUpdate = updateGenerationControls;
  updateGenerationControls = function (...args) {
    refresh(configurations[0]);const result = originalGenerationUpdate.apply(this, args);showState(configurations[0]);return result;
  };
  const originalRefineUpdate = updateRefineControls;
  updateRefineControls = function (...args) {
    for (const config of configurations.filter(item => item.key !== 'gen')) refresh(config);
    const result = originalRefineUpdate.apply(this, args);
    for (const config of configurations.filter(item => item.key !== 'gen')) showState(config);
    return result;
  };
  // A final guard covers keyboard commands, toolbar clicks and direct callers;
  // disabled-button click timing cannot submit a stale, invalid input value.
  const originalGenerate = generateImage;
  generateImage = function (...args) { commit(configurations[0]);update(configurations[0]);return originalGenerate.apply(this, args); };
  byId('gen-run').onclick = generateImage;
  const originalRefineRun = runRefineStage;
  runRefineStage = function (stage, ...args) {
    const config = configurations.find(item => item.key === stage);commit(config);update(config);return originalRefineRun.call(this, stage, ...args);
  };
  document.addEventListener('pointerdown', event => {
    const config = configurations.find(item => event.target.closest?.('#' + item.run));
    if (config) { commit(config);update(config); }
  }, true);
  let observedModeState = '';
  new MutationObserver(() => {
    const next = document.body.dataset.generationMode + ':' + document.body.dataset.generationModeBusy;
    if (next === observedModeState) return;
    observedModeState = next;
    if (!modeBusy(configurations[0])) { refresh(configurations[0]);update(configurations[0]); }
  }).observe(document.body, { attributes: true, attributeFilter: ['data-generation-mode', 'data-generation-mode-busy'] });
  window.LocalImageGenerationSize = Object.freeze({ ...math, commit: key => { const config = configurations.find(item => item.key === key);if (config) { commit(config);update(config); } }, state: key => ({ ...profile(configurations.find(item => item.key === key)) }) });
})();
