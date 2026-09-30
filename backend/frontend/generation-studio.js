/* Presentation adapter: preserve the editor's real controls, sessions and handlers. */
(() => {
  'use strict';
  const byId = id => document.getElementById(id);
  const dialog = byId('refine-dialog');
  const shell = document.querySelector('main.shell');
  const context = byId('generation-context');
  if (!dialog || !shell || !context || dialog.dataset.inlineStudio) return;

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
  const createTab = element('button', '', 'Create');
  createTab.id = 'generation-create-tab';
  const refineTab = byId('draft-refine-open');
  refineTab.textContent = 'Draft & Refine';
  for (const [tab, panel] of [[createTab, 'generation-panel'], [refineTab, 'refine-dialog']]) {
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
    resultTrigger.hidden = !generating || dialog.open || !resultButtons.some(button => !button.disabled);
    if (resultTrigger.hidden) closeResultMenu();
    const empty = String(typeof session === 'undefined' || !session);
    if (document.body.dataset.generationEmpty !== empty) document.body.dataset.generationEmpty = empty;
  }
  const resultObserver = new MutationObserver(syncGenerationPresentation);
  for (const button of resultButtons) resultObserver.observe(button, { attributes: true, attributeFilter: ['disabled'] });

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
  function rememberCommand(node) {
    if (!savedCommandStates.has(node)) savedCommandStates.set(node, { disabled: node.disabled, hidden: node.hidden });
  }
  function syncInlineCommands() {
    if (!isInlineRefining()) return;
    const selected = typeof refineSelectedResult === 'function' && (refineSelectedResult() || refineSelectedDraft());
    const locked = close.disabled || selectedCommandPending;
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
    if (!isInlineRefining() || document.querySelector('dialog:modal')) return;
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
      if (close.disabled || selectedCommandPending) { event.preventDefault();event.stopImmediatePropagation();return; }
      leaveRefinement();
    }
  }, true);
  document.addEventListener('drop', event => {
    if (!isInlineRefining() || document.querySelector('dialog:modal') || !Array.from(event.dataTransfer?.types || []).includes('Files')) return;
    if (close.disabled || selectedCommandPending) { event.preventDefault();event.stopImmediatePropagation();return; }
    leaveRefinement();
  }, true);

  function updateView(refining) {
    const wasRefining = document.body.dataset.generationView === 'refine';
    document.body.dataset.generationView = refining ? 'refine' : 'create';
    createTab.setAttribute('aria-selected', String(!refining));
    refineTab.setAttribute('aria-selected', String(refining));
    createTab.tabIndex = refining ? -1 : 0;
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
  createTab.addEventListener('click', leaveRefinement);
  dialog.showModal = () => {
    if (document.body.dataset.persona !== 'generate' && typeof setWorkspace === 'function') setWorkspace('generate');
    if (window.LocalImageStockStudio?.close) window.LocalImageStockStudio.close();
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
    createTab.disabled = close.disabled;
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
  keyboardTabs(viewTabs, [createTab, refineTab], index => index ? refineTab.click() : createTab.click());
  keyboardTabs(stageTabs, [...stageButtons.values()], index => selectStage(index ? 'final' : 'draft', true));
  // Inline comparison shares the app window but must never save or undo the
  // editor document hidden behind it. Native text undo is deliberately retained.
  document.addEventListener('keydown', event => {
    if (!dialog.open || document.body.dataset.persona !== 'generate') return;
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
      event.preventDefault();event.stopImmediatePropagation();leaveRefinement();return;
    }
    if (event.altKey && event.shiftKey && key === 'e') {
      event.preventDefault();event.stopImmediatePropagation();return;
    }
    if (key === 'o') {
      if (close.disabled) { event.preventDefault();event.stopImmediatePropagation(); }
      else leaveRefinement();
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
  const previousControls = controls;
  controls = function (...args) {
    const result = previousControls.apply(this, args);
    syncInlineCommands();
    syncGenerationPresentation();
    return result;
  };
  updateView(false);
  window.LocalImageGenerationStudio = Object.freeze({
    showCreate: leaveRefinement,
    showRefine: () => { if (!dialog.open) refineTab.click(); },
    selectStage,
    hasSelectedImage: () => typeof refineSelectedResult === 'function' && !!(refineSelectedResult() || refineSelectedDraft()),
    openSelectedInEditor: async () => {
      const selected = typeof refineSelectedResult === 'function' && (refineSelectedResult() || refineSelectedDraft());
      if (!selected || close.disabled || typeof openRefineDocument !== 'function') return false;
      await openRefineDocument(selected);
      return !dialog.open && typeof session !== 'undefined' && session?.id === selected.session.id;
    },
    isRefining: () => dialog.open && document.body.dataset.generationView === 'refine'
  });
})();
