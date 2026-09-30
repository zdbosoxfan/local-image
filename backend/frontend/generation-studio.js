/* Presentation adapter: preserve the editor's real controls, sessions and handlers. */
(() => {
  'use strict';
  const byId = id => document.getElementById(id);
  const dialog = byId('refine-dialog');
  const shell = document.querySelector('main.shell');
  const context = byId('generation-context');
  if (!dialog || !shell || !context || dialog.dataset.inlineStudio) return;
  let generationMode = 'create', createIsBlank = true, generationModeBusy = false;
  let generationModeEpoch = 0, generationInternalNavigation = 0, generationModeRunning = false;
  let refineCurrentImageAllowed = false;
  const generationModes = { edit: { settings: null, document: null }, create: { settings: null, document: null } };

  const element = (tag, className, text) => {
    const node = document.createElement(tag);
    if (className) node.className = className;
    if (text !== undefined) node.textContent = text;
    return node;
  };
  const disclosure = (label, className = '') => {
    const node = element('details', 'studio-disclosure ' + className);
    node.append(element('summary', '', label));
    return node;
  };
  const labelsFor = id => [...document.querySelectorAll('label[for="' + id + '"]')];
  const moveLabelAndControl = (id, target) => {
    for (const label of labelsFor(id)) target.append(label);
    target.append(byId(id));
  };

  // The view switch stays in the same location; it is not a modal launch action.
  const viewTabs = element('div', 'generation-view-tabs');
  viewTabs.setAttribute('role', 'tablist');
  viewTabs.setAttribute('aria-label', 'Generation workflow');
  const editTab = element('button', '', 'Edit image');
  editTab.id = 'generation-edit-tab';
  const createTab = element('button', '', 'Create new');
  createTab.id = 'generation-create-tab';
  const refineTab = byId('draft-refine-open');
  refineTab.textContent = 'Draft & Refine';
  for (const [tab, panel] of [[editTab, 'generation-panel'], [createTab, 'generation-panel'], [refineTab, 'refine-dialog']]) {
    tab.type = 'button';
    tab.setAttribute('role', 'tab');
    tab.setAttribute('aria-controls', panel);
    viewTabs.append(tab);
  }
  document.querySelector('.workspace-modes').after(viewTabs);
  byId('generation-summary').classList.add('generation-view-summary');
  byId('generation-summary').hidden = true;
  byId('generated-library-open').hidden = true;
  byId('generated-library-open').textContent = 'Image library';
  byId('gen-prompt-tab').textContent = 'Prompt';
  const resultTrigger = element('button', 'generation-use-result', 'Use result');
  resultTrigger.id = 'generation-use-result';
  resultTrigger.type = 'button';
  resultTrigger.setAttribute('aria-haspopup', 'menu');
  resultTrigger.setAttribute('aria-expanded', 'false');
  resultTrigger.setAttribute('aria-controls', 'generation-result-menu');
  const resultMenu = element('div', 'generation-result-menu');
  resultMenu.id = 'generation-result-menu';
  resultMenu.setAttribute('role', 'menu');
  resultMenu.setAttribute('aria-label', 'Use generated result');
  resultMenu.setAttribute('popover', 'auto');
  const resultButtons = ['generated-retouch', 'generated-cutout', 'generated-background'].map(byId);
  for (const button of resultButtons) {
    button.setAttribute('role', 'menuitem');
    resultMenu.append(button);
    button.addEventListener('click', () => closeResultMenu());
  }
  viewTabs.after(resultTrigger);
  document.body.append(resultMenu);
  function closeResultMenu(restoreFocus = false) {
    if (resultMenu.matches(':popover-open')) resultMenu.hidePopover();
    resultTrigger.setAttribute('aria-expanded', 'false');
    if (restoreFocus && !resultTrigger.hidden) resultTrigger.focus();
  }
  function openResultMenu(focusFirst = false) {
    if (resultTrigger.hidden || resultTrigger.disabled) return;
    closeMenus();
    const bounds = resultTrigger.getBoundingClientRect();
    resultMenu.style.left = Math.max(8, Math.min(bounds.left, innerWidth - 260)) + 'px';
    resultMenu.style.top = Math.min(bounds.bottom + 6, innerHeight - 150) + 'px';
    resultMenu.showPopover();
    resultTrigger.setAttribute('aria-expanded', 'true');
    if (focusFirst) resultButtons.find(button => !button.disabled)?.focus();
  }
  resultTrigger.addEventListener('click', () => resultMenu.matches(':popover-open') ? closeResultMenu() : openResultMenu());
  resultTrigger.addEventListener('keydown', event => {
    if (event.key === 'ArrowDown') { event.preventDefault();openResultMenu(true); }
  });
  resultMenu.addEventListener('toggle', () => resultTrigger.setAttribute('aria-expanded', String(resultMenu.matches(':popover-open'))));
  resultMenu.addEventListener('keydown', event => {
    if (event.key === 'Escape') { event.preventDefault();event.stopPropagation();closeResultMenu(true);return; }
    if (event.key === 'Tab') { closeResultMenu();return; }
    const available = resultButtons.filter(button => !button.disabled);
    const current = available.indexOf(document.activeElement);
    if (['ArrowDown', 'ArrowUp', 'Home', 'End'].includes(event.key)) {
      event.preventDefault();event.stopPropagation();
      const next = event.key === 'Home' ? 0 : event.key === 'End' ? available.length - 1 : (current + (event.key === 'ArrowDown' ? 1 : -1) + available.length) % available.length;
      available[next]?.focus();
    }
  });
  function syncGenerationPresentation() {
    const generating = document.body.dataset.persona === 'generate';
    resultTrigger.hidden = !generating || dialog.open || isCreatingBlank() || generationModeBusy || !resultButtons.some(button => !button.disabled);
    if (resultTrigger.hidden) closeResultMenu();
    const empty = String(typeof session === 'undefined' || !session || isCreatingBlank());
    if (document.body.dataset.generationEmpty !== empty) document.body.dataset.generationEmpty = empty;
  }
  const resultObserver = new MutationObserver(syncGenerationPresentation);
  for (const button of resultButtons) resultObserver.observe(button, { attributes: true, attributeFilter: ['disabled'] });
  const blankCanvas = element('div', 'generation-blank-canvas');
  blankCanvas.id = 'generation-blank-canvas';
  blankCanvas.append(element('h2', '', 'Create a new image'), element('p', '', 'Describe your image in the prompt panel.'));
  byId('viewport').append(blankCanvas);
  const editSource = element('p', 'generation-edit-source');
  editSource.id = 'generation-edit-source';
  byId('gen-prompt').before(editSource);

  // Create: keep the frequently used numbers next to the prompt, while the
  // Output tab contains seed, alpha and advanced sampling controls.
  const essentials = element('div', 'generation-create-essentials');
  const fields = element('div', 'generation-core-fields');
  for (const [id, title] of [['gen-width', 'Width'], ['gen-height', 'Height'], ['gen-steps', 'Steps']]) {
    const input = byId(id);
    const previousLabels = labelsFor(id);
    const label = element('label', '', title);
    label.htmlFor = id;
    label.append(input);
    fields.append(label);
    for (const previous of previousLabels) previous.remove();
  }
  document.querySelector('#generation-panel .generation-dimensions')?.remove();
  essentials.append(fields, byId('gen-sampling-note'));
  byId('gen-prompt').after(essentials);
  byId('gen-prompt').rows = 5;
  const precisionDetails = disclosure('Model precision', 'generation-precision-disclosure');
  precisionDetails.id = 'generation-precision-options';
  const precision = element('div', 'generation-precision');
  precision.append(byId('gen-variant-label'), byId('gen-variant'));
  precisionDetails.append(precision);
  byId('gen-model-state').after(precisionDetails);
  const syncPrecisionVisibility = () => { precisionDetails.hidden = byId('gen-variant').hidden; };
  new MutationObserver(syncPrecisionVisibility).observe(byId('gen-variant'), { attributes: true, attributeFilter: ['hidden'] });
  syncPrecisionVisibility();

  // Draft & Refine uses real source images, full-resolution comparison and
  // the original independent controls. Only their visual placement changes.
  dialog.dataset.inlineStudio = 'true';
  dialog.setAttribute('role', 'region');
  dialog.setAttribute('aria-modal', 'false');
  shell.append(dialog);
  const nativeShow = HTMLDialogElement.prototype.show.bind(dialog);
  const nativeClose = HTMLDialogElement.prototype.close.bind(dialog);
  const inspector = element('aside', 'generation-refine-inspector');
  inspector.setAttribute('aria-label', 'Draft and refinement settings');
  const inspectorTitle = element('h2', 'generation-inspector-title', 'Stage settings');
  const stageTabs = element('div', 'generation-stage-tabs');
  stageTabs.setAttribute('role', 'tablist');
  stageTabs.setAttribute('aria-label', 'Stage settings');
  const inspectorScroll = element('div', 'generation-inspector-scroll');
  inspector.append(inspectorTitle, stageTabs, inspectorScroll);
  dialog.append(inspector);
  let activeStage = 'draft';
  const stagePanels = new Map();
  const stageButtons = new Map();

  function selectStage(stage, focus = false) {
    activeStage = stage;
    for (const [name, panel] of stagePanels) {
      panel.hidden = name !== stage;
      const button = stageButtons.get(name);
      button.setAttribute('aria-selected', String(name === stage));
      button.tabIndex = name === stage ? 0 : -1;
    }
    dialog.dataset.activeStage = stage;
    if (focus) stageButtons.get(stage)?.focus();
  }

  const panes = [...dialog.querySelectorAll('.refine-pane')];
  for (const [index, stage] of ['draft', 'final'].entries()) {
    const pane = panes[index];
    pane.dataset.stage = stage;
    const settings = pane.querySelector('.refine-settings');
    const stageName = stage === 'draft' ? 'Draft' : 'Refine';
    const tab = element('button', '', stageName);
    tab.id = 'generation-stage-' + stage + '-tab';
    tab.type = 'button';
    tab.setAttribute('role', 'tab');
    tab.setAttribute('aria-controls', 'generation-stage-' + stage + '-settings');
    tab.addEventListener('click', () => selectStage(stage));
    stageTabs.append(tab);
    stageButtons.set(stage, tab);
    settings.id = 'generation-stage-' + stage + '-settings';
    settings.setAttribute('role', 'tabpanel');
    settings.setAttribute('aria-labelledby', tab.id);
    inspectorScroll.append(settings);
    stagePanels.set(stage, settings);

    const core = element('div', 'generation-stage-core');
    const model = element('div', 'generation-stage-model');
    moveLabelAndControl('refine-' + stage + '-model', model);
    model.append(byId('refine-' + stage + '-model-note'));
    const values = element('div', 'generation-core-fields');
    for (const name of ['width', 'height', 'steps']) {
      const input = byId('refine-' + stage + '-' + name);
      values.append(input.closest('label'));
    }
    core.append(model, values, byId('refine-' + stage + '-recommended'));
    pane.querySelector('.refine-pane-actions').before(core);

    const precisionDetails = disclosure('Model precision & status', 'generation-model-details');
    const variant = byId('refine-' + stage + '-variant');
    precisionDetails.append(variant);
    precisionDetails.querySelector('summary').textContent = 'Model precision';
    const syncStagePrecision = () => { precisionDetails.hidden = variant.hidden; };
    new MutationObserver(syncStagePrecision).observe(variant, { attributes: true, attributeFilter: ['hidden'] });
    syncStagePrecision();
    settings.append(precisionDetails);
    const prompt = byId('refine-' + stage + '-prompt');
    prompt.rows = 5;
    const promptLabel = labelsFor(prompt.id)[0];
    if (promptLabel) promptLabel.textContent = stage === 'draft' ? 'Draft prompt' : 'Refinement prompt';
    const styles = pane.querySelector('.refine-stage-output') || settings.querySelector('.refine-stage-output');
    if (styles) styles.classList.add('generation-stage-style');
    if (stage === 'draft') {
      byId('refine-draft-options').querySelector('summary').textContent = 'References & advanced sampling';
    } else {
      const upscale = disclosure('Upscale output', 'generation-upscale-disclosure');
      byId('refine-upscale-options').before(upscale);
      upscale.append(byId('refine-upscale-options'));
      const syncUpscaleVisibility = () => { upscale.hidden = byId('refine-upscale-options').hidden; };
      new MutationObserver(syncUpscaleVisibility).observe(byId('refine-upscale-options'), { attributes: true, attributeFilter: ['hidden'] });
      syncUpscaleVisibility();
      const note = pane.querySelector('.refine-change-note');
      if (note) settings.append(note);
      const reuse = element('button', 'generation-reuse-prompt', 'Use draft prompt');
      reuse.type = 'button';
      reuse.id = 'refine-copy-draft-prompt';
      reuse.addEventListener('click', () => {
        if (prompt.disabled) return;
        prompt.value = byId('refine-draft-prompt').value;
        prompt.dispatchEvent(new Event('input', { bubbles: true }));
        prompt.focus();
      });
      promptLabel?.after(reuse);
    }
    // Selecting an image or its main action reveals the settings for that stage.
    pane.addEventListener('focusin', () => selectStage(stage));
    pane.querySelector('.refine-image-area').addEventListener('pointerdown', () => selectStage(stage));
  }
  const recipe = dialog.querySelector('.refine-recipe-panel');
  inspectorScroll.append(recipe);
  inspectorScroll.append(byId('refine-refresh-models'));
  const localNote = element('p', 'generation-local-note', 'Runs on this PC');
  inspector.append(localNote);
  selectStage('draft');

  // Keep the return action visible, without restoring the former dialog header.
  const close = byId('refine-close');
  close.replaceChildren(document.createTextNode('Back to Create'));
  close.setAttribute('aria-label', 'Back to Create');
  close.className = 'generation-return-create';
  dialog.querySelector('.refine-comparison-bar').append(close);
  byId('refine-title').closest('.dialog-header').hidden = true;

  const saveCommands = new Set(['save', 'save-project', 'save-project-as', 'save-unique', 'return']);
  const openCommands = new Set(['open', 'open-project', 'open-folder']);
  const editorOnlyCommands = new Set(['undo', 'redo', 'merge', 'restore', 'close-image', 'clear', 'finish', 'add', 'subtract', 'before', 'actual-size', 'fit', 'zoom-in', 'zoom-out', 'zoom', 'size', 'remove', 'cutout-refine', 'cutout-restore', 'cutout-erase', 'heal-brush', 'hand', 'folder-previous', 'folder-next']);
  const savedCommandStates = new Map();
  let selectedCommandPending = false;
  const isInlineRefining = () => dialog.open && document.body.dataset.persona === 'generate';
  function isCreatingBlank() { return document.body.dataset.persona === 'generate' && !dialog.open && generationMode === 'create' && createIsBlank; }
  function hasHiddenEditor() { return isInlineRefining() || isCreatingBlank() || (document.body.dataset.persona === 'generate' && generationModeBusy); }
  function modeCommandsLocked() { return isInlineRefining() ? close.disabled : busy || generationModeBusy; }
  function rememberCommand(node) {
    if (!savedCommandStates.has(node)) savedCommandStates.set(node, { disabled: node.disabled, hidden: node.hidden });
  }
  function syncInlineCommands() {
    if (!hasHiddenEditor()) return;
    const selected = isInlineRefining() && typeof refineSelectedResult === 'function' && (refineSelectedResult() || refineSelectedDraft());
    const locked = modeCommandsLocked() || selectedCommandPending;
    for (const node of document.querySelectorAll('button,input,select,[data-command]')) {
      const command = node.dataset.command || node.id;
      if (editorOnlyCommands.has(command) || node.matches('[data-tool],#layers button,#layers input')) {
        rememberCommand(node);node.disabled = true;
      } else if (saveCommands.has(command)) {
        rememberCommand(node);node.disabled = locked || !selected;
        // The visible workflow exports generated copies; original-source save
        // actions are not meaningful here, including their document proxies.
        if (command === 'return' || command === 'save-unique') node.hidden = true;
      } else if (openCommands.has(command)) {
        rememberCommand(node);node.disabled = locked;
      }
    }
  }
  function restoreEditorCommands() {
    for (const [node, state] of savedCommandStates) {
      if (!node.isConnected) continue;
      node.disabled = state.disabled;node.hidden = state.hidden;
    }
    savedCommandStates.clear();
  }
  async function runSelectedCommand(command) {
    if (!isInlineRefining()) { message('Generate an image before saving. Your other document is preserved.');return; }
    if (selectedCommandPending || close.disabled) return;
    const selected = typeof refineSelectedResult === 'function' && (refineSelectedResult() || refineSelectedDraft());
    if (!selected) { refineStatus('Generate or select an image before saving.');return; }
    selectedCommandPending = true;syncInlineCommands();
    try {
      closeMenus();
      await openRefineDocument(selected);
      if (!dialog.open && session?.id === selected.session.id) {
        // Never carry Overwrite original / Save beside source into a hidden
        // editor document. Both become an explicit exported copy of selection.
        byId(command === 'return' || command === 'save-unique' ? 'save' : command)?.click();
      }
    } catch (error) { message(error.message, true); }
    finally { selectedCommandPending = false;syncInlineCommands();window.LocalImageStudio?.sync(); }
  }
  document.addEventListener('click', event => {
    if (!hasHiddenEditor() || document.querySelector('dialog:modal')) return;
    const target = event.target instanceof Element ? event.target.closest('button,input,select,[data-command]') : null;
    if (!target || dialog.contains(target)) return;
    const command = target.dataset.command || target.id;
    if (editorOnlyCommands.has(command) || target.matches('[data-tool],#layers button,#layers input')) {
      event.preventDefault();event.stopImmediatePropagation();return;
    }
    if (saveCommands.has(command)) {
      event.preventDefault();event.stopImmediatePropagation();runSelectedCommand(command);return;
    }
    if (openCommands.has(command) || target.closest('#recent')) {
      if (modeCommandsLocked() || selectedCommandPending) { event.preventDefault();event.stopImmediatePropagation();return; }
      if (dialog.open) leaveRefinement();
    }
  }, true);
  document.addEventListener('drop', event => {
    if (!hasHiddenEditor() || document.querySelector('dialog:modal') || !Array.from(event.dataTransfer?.types || []).includes('Files')) return;
    if (modeCommandsLocked() || selectedCommandPending) { event.preventDefault();event.stopImmediatePropagation();return; }
    if (dialog.open) leaveRefinement();
  }, true);

  function updateView(refining) {
    const wasRefining = document.body.dataset.generationView === 'refine';
    document.body.dataset.generationView = refining ? 'refine' : generationMode;
    editTab.setAttribute('aria-selected', String(!refining && generationMode === 'edit'));
    createTab.setAttribute('aria-selected', String(!refining && generationMode === 'create'));
    refineTab.setAttribute('aria-selected', String(refining));
    editTab.tabIndex = !refining && generationMode === 'edit' ? 0 : -1;
    createTab.tabIndex = !refining && generationMode === 'create' ? 0 : -1;
    refineTab.tabIndex = refining ? 0 : -1;
    if (!refining && wasRefining) {
      restoreEditorCommands();
      if (typeof controls === 'function') controls();
    }
    syncInlineCommands();
    syncGenerationPresentation();
    window.LocalImageStudio?.sync();
    if (typeof resize === 'function') requestAnimationFrame(resize);
  }
  function leaveRefinement() {
    if (close.disabled) return;
    if (dialog.open) nativeClose();
    updateView(false);
  }
  editTab.addEventListener('click', () => activateGenerationMode('edit'));
  createTab.addEventListener('click', () => activateGenerationMode('create', { fresh: true }));
  close.onclick = () => activateGenerationMode('create', { fresh: true });
  dialog.showModal = () => {
    if (document.body.dataset.persona !== 'generate' && typeof setWorkspace === 'function') setWorkspace('generate');
    if (window.LocalImageStockStudio?.close) window.LocalImageStockStudio.close();
    saveGenerationMode();
    refineCurrentImageAllowed = !isCreatingBlank() && !generationModeBusy && !!session;
    for (const stage of ['draft', 'final']) {
      const model = byId('refine-' + stage + '-model');
      if (!model.options.length) {
        model.add(new Option('Loading local models…', ''));
        model.disabled = true;
      }
    }
    updateView(true);
    if (!dialog.open) nativeShow();
    syncInlineCommands();
    syncGenerationPresentation();
    window.LocalImageStudio?.sync();
    requestAnimationFrame(() => {
      if (typeof applyRefineComparison === 'function') applyRefineComparison();
      stageButtons.get(activeStage)?.focus();
    });
  };
  dialog.close = (...args) => {
    nativeClose(...args);
    updateView(false);
  };
  dialog.addEventListener('close', () => updateView(dialog.open));
  new MutationObserver(() => {
    createTab.disabled = busy || generationModeBusy || (dialog.open && close.disabled);
    editTab.disabled = createTab.disabled || !generationModes.edit.document;
  }).observe(close, { attributes: true, attributeFilter: ['disabled'] });
  new MutationObserver(() => {
    if (document.body.dataset.persona !== 'generate' && dialog.open) {
      nativeClose();
      updateView(false);
    }
    syncGenerationPresentation();
  }).observe(document.body, { attributes: true, attributeFilter: ['data-persona'] });
  const keyboardTabs = (host, items, activate) => host.addEventListener('keydown', event => {
    if (!['ArrowLeft', 'ArrowRight', 'Home', 'End'].includes(event.key)) return;
    const current = items.indexOf(event.target);
    if (current < 0) return;
    event.preventDefault();
    const index = event.key === 'Home' ? 0 : event.key === 'End' ? items.length - 1 : (current + (event.key === 'ArrowRight' ? 1 : -1) + items.length) % items.length;
    if (items[index].disabled) return;
    items[index].focus();
    activate(index);
  });
  keyboardTabs(viewTabs, [editTab, createTab, refineTab], index => [editTab, createTab, refineTab][index].click());
  keyboardTabs(stageTabs, [...stageButtons.values()], index => selectStage(index ? 'final' : 'draft', true));
  // Inline comparison shares the app window but must never save or undo the
  // editor document hidden behind it. Native text undo is deliberately retained.
  document.addEventListener('keydown', event => {
    if (!hasHiddenEditor()) return;
    if (document.querySelector('dialog:modal')) return;
    const key = event.key.toLowerCase();
    const typing = event.target instanceof Element && !!event.target.closest('textarea,input:not([type=checkbox]):not([type=radio]),[contenteditable=true]');
    if (!(event.ctrlKey || event.metaKey)) {
      const editorShortcut = !event.altKey && ['h', 'j', 'b', 'p', 'r', 'e', 'v', '\\', 'f', '1', '+', '-', '_', '=', 'pageup', 'pagedown', '[', ']'].includes(key);
      const folderShortcut = event.altKey && ['arrowleft', 'arrowright'].includes(key);
      if (!typing && !dialog.contains(event.target) && (editorShortcut || folderShortcut)) {
        event.preventDefault();event.stopImmediatePropagation();
      }
      return;
    }
    if (['z', 'y'].includes(key)) {
      event.stopImmediatePropagation();
      if (!typing) {
        event.preventDefault();
        if (typeof refineStatus === 'function') refineStatus('Choose a previous draft or refined image from its variation strip to return to it.');
      }
      return;
    }
    if (key === 's') {
      event.preventDefault();
      event.stopImmediatePropagation();
      runSelectedCommand(event.altKey ? 'save-project' : 'save');
      return;
    }
    if (key === 'w') {
      event.preventDefault();event.stopImmediatePropagation();
      if (dialog.open) leaveRefinement();else if (!generationModeBusy && generationModes.edit.document) activateGenerationMode('edit');
      return;
    }
    if (event.altKey && event.shiftKey && key === 'e') {
      event.preventDefault();event.stopImmediatePropagation();return;
    }
    if (key === 'o') {
      if (modeCommandsLocked()) { event.preventDefault();event.stopImmediatePropagation(); }
      else if (dialog.open) leaveRefinement();
    }
  }, true);
  // Comparison zoom/pan handlers run on their own elements; their keys do not
  // then bubble into the hidden photo canvas's global shortcuts.
  dialog.addEventListener('keydown', event => {
    if (!event.ctrlKey && !event.metaKey) event.stopPropagation();
  });
  const previousUpdateRefineControls = updateRefineControls;
  updateRefineControls = function (...args) {
    const result = previousUpdateRefineControls.apply(this, args);
    if (dialog.open && !refineCurrentImageAllowed) {
      byId('refine-use-current').disabled = true;byId('refine-current-reference').disabled = true;
    }
    for (const stage of ['draft', 'final']) {
      const note = byId('refine-' + stage + '-recommended');
      if (note.textContent.includes(' · ')) {
        note.title = note.textContent;
        note.textContent = note.textContent.split(' · ')[0] + '.';
      }
    }
    syncInlineCommands();
    window.LocalImageStudio?.sync();
    return result;
  };
  const generationFieldIds = ['gen-variant','gen-prompt','gen-negative','gen-width','gen-height','gen-steps','gen-guidance','gen-denoise','gen-aspect','gen-seed','gen-transparent'];
  const copyValue = value => JSON.parse(JSON.stringify(value));
  const supportsImageEditing = model => !!(model?.capabilities?.image_reference || model?.capabilities?.image_to_image || model?.capabilities?.references) && (model?.capabilities?.max_references || 0) > 0;
  const currentModeDocument = () => generationModes[generationMode].document;
  const freshDocument = data => data && (openDocuments.get(data.id)?.revision >= data.revision ? openDocuments.get(data.id) : data);
  function captureGenerationSettings() {
    const fields = {};
    for (const id of generationFieldIds) fields[id] = byId(id).type === 'checkbox' ? byId(id).checked : byId(id).value;
    return { model: generationModelId, fields, references: copyValue(generationReferences), loras: copyValue(generationLoras), missing: generationMissingReferences, tab: generationStudioTab, target: generationTargetSession };
  }
  function initialGenerationSettings(mode, source) {
    const selected = generationModels.find(model => !model.historical && model.available && (mode !== 'edit' || supportsImageEditing(model)) && model.id === 'qwen') || generationModels.find(model => !model.historical && (mode !== 'edit' || supportsImageEditing(model)));
    const defaults = selected?.defaults || {}, limits = selected?.limits || {};
    let width = defaults.width || 1024, height = defaults.height || 1024;
    if (mode === 'edit' && source?.width && source?.height) {
      const step = limits.dimension_step || 32, maximum = limits.max_dimension || 4096, minimum = limits.min_dimension || 256;
      const scale = Math.min(1, maximum / source.width, maximum / source.height, Math.sqrt((limits.max_pixels || 4194304) / (source.width * source.height)));
      width = Math.max(minimum, Math.floor(source.width * scale / step) * step);
      height = Math.max(minimum, Math.floor(source.height * scale / step) * step);
    }
    return { model: selected?.id || generationModelId, fields: { 'gen-variant': defaults.variant || selected?.variants?.[0]?.id || '', 'gen-prompt': '', 'gen-negative': '', 'gen-width': String(width), 'gen-height': String(height), 'gen-steps': String(defaults.steps || 25), 'gen-guidance': String(defaults.guidance || 1), 'gen-denoise': '65', 'gen-aspect': mode === 'edit' ? 'custom' : '1:1', 'gen-seed': '', 'gen-transparent': false }, references: [], loras: [], missing: 0, tab: 'prompt', target: mode === 'create' ? generationTargetSession : null };
  }
  function saveGenerationMode() {
    if (document.body.dataset.persona !== 'generate' || dialog.open || generationModeBusy) return;
    const profile = generationModes[generationMode];
    profile.settings = captureGenerationSettings();
    if (!isCreatingBlank() && session) profile.document = freshDocument(session);
  }
  function primaryReference(data) {
    return { id: data.id, name: data.name, thumbnail: '/api/local-remove/session/' + encodeURIComponent(data.id) + '/preview?revision=' + data.revision };
  }
  function bindEditingSource(data, previousId) {
    data = freshDocument(data);
    if (!data) return;
    const extras = generationReferences.filter(reference => reference.id !== data.id && reference.id !== previousId);
    generationModes.edit.document = data;
    generationReferences = [primaryReference(data), ...extras];
    generationMissingReferences = 0;
  }
  function restoreGenerationModeSettings(settings, source) {
    generationModelId = settings.model;
    if (!generationModels.some(model => model.id === generationModelId)) generationModelId = generationModels.find(model => !model.historical)?.id || generationModelId;
    if (generationMode === 'edit' && !supportsImageEditing(generationModel())) generationModelId = generationModels.find(model => model.available && supportsImageEditing(model))?.id || generationModels.find(supportsImageEditing)?.id || generationModelId;
    generationSamplingModel = generationModelId;
    generationReferences = copyValue(settings.references || []);
    generationLoras = copyValue(settings.loras || []);
    generationMissingReferences = settings.missing || 0;
    generationTargetSession = settings.target || null;
    renderGenerationModelOptions();syncGenerationModel();
    for (const [id, value] of Object.entries(settings.fields)) {
      if (byId(id).type === 'checkbox') byId(id).checked = !!value;
      else if (id !== 'gen-variant' || [...byId(id).options].some(option => option.value === value)) byId(id).value = String(value);
    }
    if (generationMode === 'edit' && source) {
      const prior = generationReferences[0]?.id;
      bindEditingSource(source, prior);
    }
    byId('gen-denoise-value').textContent = byId('gen-denoise').value + '%';
    renderSelectedLoras();renderGenerationReferences();selectGenerationTab(settings.tab || 'prompt');
  }
  function syncGenerationModeChrome() {
    const generating = document.body.dataset.persona === 'generate';
    const editing = generationMode === 'edit';
    const source = generationModes.edit.document;
    const blank = generating && !dialog.open && ((generationMode === 'create' && createIsBlank) || (generationModeBusy && currentModeDocument()?.id !== session?.id));
    document.body.dataset.generationMode = generationMode;
    document.body.dataset.generationBlank = String(blank);
    document.body.dataset.generationModeBusy = String(generationModeBusy);
    editTab.disabled = busy || generationModeBusy || (dialog.open && close.disabled) || !source;
    editTab.title = source ? 'Edit image · ' + source.name : 'Open or generate an image to edit';
    createTab.disabled = busy || generationModeBusy || (dialog.open && close.disabled);
    if (generationModeBusy) refineTab.disabled = true;
    const promptLabel = labelsFor('gen-prompt')[0];
    if (promptLabel) promptLabel.textContent = editing ? 'Changes' : 'Prompt';
    byId('gen-prompt').placeholder = editing ? 'Describe what to change in this image. Keep anything else that matters.' : 'Describe your image, its lighting, composition and style…';
    editSource.hidden = !editing;
    editSource.textContent = source ? 'Editing ' + source.name : 'Open an image to edit.';
    byId('gen-use-current').hidden = editing || isCreatingBlank();
    if (isCreatingBlank()) byId('gen-use-current').disabled = true;
    if (editing && !supportsImageEditing(generationModel())) {
      byId('gen-run').disabled = true;
      editSource.textContent = 'Choose an image-editing model, or use Create new for text-only generation.';
      editSource.classList.add('error');
    } else editSource.classList.remove('error');
    if (editing && (!source || source.id !== session?.id)) byId('gen-run').disabled = true;
    if (generationModeBusy) byId('gen-run').disabled = true;
    byId('gen-run').textContent = busy && activeTask === 'generate' ? (editing ? 'Editing…' : 'Generating…') : (editing ? 'Apply edit' : 'Generate image');
    for (const option of byId('gen-model').options) option.disabled = editing && !supportsImageEditing(generationModels.find(model => model.id === option.value));
    if (generating && !dialog.open) byId('tool-hint').textContent = editing ? 'Describe the changes to the current image' : 'Create a new image from a prompt';
    syncGenerationPresentation();
  }
  const originalModeOpenSession = openSession;
  async function activateGenerationMode(mode, { source = null, fresh = false, capture = true } = {}) {
    if (!['edit','create'].includes(mode) || busy || generationModeBusy || (dialog.open && close.disabled)) return false;
    if (mode === 'edit' && !source && !generationModes.edit.document) return false;
    if (capture) saveGenerationMode();
    const epoch = ++generationModeEpoch;
    if (dialog.open) nativeClose();
    restoreEditorCommands();
    generationMode = mode;generationModeBusy = true;
    const profile = generationModes[mode];
    if (source) profile.document = freshDocument(source);
    if (mode === 'create' && fresh) profile.document = null;
    createIsBlank = mode === 'create' && !profile.document;
    byId('gen-result-note').textContent = '';
    updateView(false);syncGenerationModeChrome();
    try {
      if (generationLoading) await generationLoadPromise;
      else if (!generationModels.length) await loadGenerationModels();
      if (epoch !== generationModeEpoch || workspace !== 'generate') return false;
      if (!profile.settings) profile.settings = initialGenerationSettings(mode, profile.document);
      if (profile.document && (session?.id !== profile.document.id || session?.revision !== freshDocument(profile.document)?.revision)) {
        generationInternalNavigation++;
        try { await originalModeOpenSession(freshDocument(profile.document)); }
        finally { generationInternalNavigation--; }
      }
      if (epoch !== generationModeEpoch || workspace !== 'generate') return false;
      restoreGenerationModeSettings(profile.settings, profile.document);
      profile.settings = captureGenerationSettings();
      return true;
    } catch (error) { message(error.message, true);return false; }
    finally {
      if (epoch === generationModeEpoch) {
        generationModeBusy = false;restoreEditorCommands();controls();updateView(dialog.open);syncGenerationModeChrome();
      }
    }
  }
  const originalGenerationControls = updateGenerationControls;
  updateGenerationControls = function (...args) {
    const result = originalGenerationControls.apply(this, args);
    syncGenerationModeChrome();syncInlineCommands();window.LocalImageStudio?.sync();
    return result;
  };
  const originalGenerationReferences = renderGenerationReferences;
  renderGenerationReferences = function (...args) {
    const result = originalGenerationReferences.apply(this, args);
    if (generationMode === 'edit' && generationReferences[0]?.id === generationModes.edit.document?.id) {
      const row = byId('gen-references').firstElementChild;
      if (row) {
        row.dataset.primarySource = 'true';
        const remove = row.querySelector('button');if (remove) { remove.hidden = true;remove.disabled = true; }
        const label = row.querySelector('span');if (label) label.textContent = 'Current image · ' + generationReferences[0].name;
      }
    }
    return result;
  };
  const originalGenerationImage = generateImage;
  generateImage = async function (...args) {
    if (busy || generationModeBusy || byId('gen-run').disabled) return;
    const mode = generationMode, priorResult = generationResultId;
    if (mode === 'edit') {
      if (!supportsImageEditing(generationModel()) || session?.id !== generationModes.edit.document?.id) return;
      bindEditingSource(session, generationReferences[0]?.id);
      renderGenerationReferences();
    }
    generationModeRunning = true;
    try {
      await originalGenerationImage.apply(this, args);
      if (generationResultId !== priorResult && session?.id === generationResultId) {
        generationModes[mode].document = freshDocument(session);
        if (mode === 'create') {
          createIsBlank = false;
          if (!generationModes.edit.document) generationModes.edit.document = freshDocument(session);
        }
        else bindEditingSource(session, generationReferences[0]?.id);
        generationModes[mode].settings = captureGenerationSettings();
        renderGenerationReferences();
      }
    } finally {
      generationModeRunning = false;restoreEditorCommands();controls();updateView(dialog.open);syncGenerationModeChrome();
    }
  };
  byId('gen-run').onclick = generateImage;
  const originalGenerationWorkspace = setWorkspace;
  setWorkspace = function (value) {
    const previous = workspace, visible = session;
    if (previous === 'generate' && value !== 'generate' && !busy) {
      saveGenerationMode();generationModeEpoch++;generationModeBusy = false;restoreEditorCommands();
    }
    const result = originalGenerationWorkspace.apply(this, arguments);
    if (workspace === 'generate' && previous !== 'generate') activateGenerationMode(visible ? 'edit' : 'create', { source: visible, fresh: !visible, capture: false });
    return result;
  };
  openSession = async function (...args) {
    const result = await originalModeOpenSession.apply(this, args);
    if (!generationInternalNavigation && !generationModeRunning && workspace === 'generate' && !dialog.open && session?.id === args[0]?.id) {
      await activateGenerationMode('edit', { source: session, capture: false });
    }
    return result;
  };
  const previousControls = controls;
  controls = function (...args) {
    const result = previousControls.apply(this, args);
    syncInlineCommands();
    syncGenerationPresentation();
    syncGenerationModeChrome();
    return result;
  };
  updateView(false);
  window.LocalImageGenerationStudio = Object.freeze({
    showCreate: () => activateGenerationMode('create'),
    showEdit: () => activateGenerationMode('edit'),
    showRefine: () => { if (!dialog.open) refineTab.click(); },
    selectStage,
    hasSelectedImage: () => typeof refineSelectedResult === 'function' && !!(refineSelectedResult() || refineSelectedDraft()),
    openSelectedInEditor: async () => {
      const selected = typeof refineSelectedResult === 'function' && (refineSelectedResult() || refineSelectedDraft());
      if (!selected || close.disabled || typeof openRefineDocument !== 'function') return false;
      await openRefineDocument(selected);
      return !dialog.open && typeof session !== 'undefined' && session?.id === selected.session.id;
    },
    isCreatingBlank,
    hasVisibleDocument: () => !generationModeBusy && (isInlineRefining() ? !!(refineSelectedResult() || refineSelectedDraft()) : !isCreatingBlank() && !!session),
    prepareSelectedForExport: async () => {
      if (busy || generationModeBusy || isCreatingBlank()) return false;
      if (isInlineRefining()) {
        const selected = refineSelectedResult() || refineSelectedDraft();
        if (!selected || close.disabled) return false;
        await openRefineDocument(selected);
        return !dialog.open && session?.id === selected.session.id;
      }
      return !!session;
    },
    mode: () => generationMode,
    isModeBusy: () => generationModeBusy,
    isRefining: () => dialog.open && document.body.dataset.generationView === 'refine'
  });
})();
