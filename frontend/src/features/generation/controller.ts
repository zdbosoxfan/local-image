import type { EditorDocument } from '../../contracts.ts';
import { createGenerationApi, GenerationApiError } from './api.ts';
import type { GenerationApi } from './api.ts';
import type {
  DraftKey,
  GenerationContext,
  GenerationDraft,
  GenerationHost,
  GenerationMode,
  GenerationModel,
  GenerationReference,
  GenerationState,
  LoraDraftPort,
  LoraSelection,
  RefinementImage,
  RefinementRecipe,
} from './types.ts';
import { generationPayload, supportsReferences, validateDraft, workflowLimits } from './validation.ts';

const RECIPE_KEY = 'local-image.refinement-recipes.v1';
const keys: DraftKey[] = ['create', 'edit', 'draft', 'final'];
const blankDraft = (): GenerationDraft => ({
  modelId: '',
  variant: '',
  prompt: '',
  negativePrompt: '',
  width: 1024,
  height: 1024,
  steps: 40,
  guidance: 1,
  seed: '',
  transparent: false,
  denoise: 0.65,
  references: [],
  loras: [],
  locked: true,
  ratio: 1,
  aspect: '1:1',
  missingReferenceCount: 0,
});
function freeze<T>(value: T): T {
  if (value && typeof value === 'object' && !Object.isFrozen(value)) {
    for (const child of Object.values(value)) freeze(child);
    Object.freeze(value);
  }
  return value;
}
function reference(document: EditorDocument): GenerationReference {
  return {
    id: document.id,
    name: document.name,
    revision: document.revision,
    thumbnail: `/api/local-remove/session/${encodeURIComponent(document.id)}/preview?revision=${document.revision}`,
    attribution: document.source_attribution ?? document.reference_attributions,
  };
}
function errorText(error: unknown) {
  return error instanceof Error ? error.message : String(error);
}
export function createGenerationController(
  host: GenerationHost,
  token: string,
  options: { api?: GenerationApi; storage?: Pick<Storage, 'getItem' | 'setItem'>; pollMilliseconds?: number } = {},
) {
  const api = options.api ?? createGenerationApi(token),
    listeners = new Set<() => void>();
  const storage = options.storage ?? (typeof localStorage !== 'undefined' ? localStorage : undefined);
  function recipes(): RefinementRecipe[] {
    try {
      const values = JSON.parse(storage?.getItem(RECIPE_KEY) ?? '[]');
      return Array.isArray(values)
        ? values
            .filter(
              value =>
                value?.schema === 1 && typeof value.name === 'string' && value.stages?.draft && value.stages?.final,
            )
            .slice(0, 40)
        : [];
    } catch {
      return [];
    }
  }
  let state: GenerationState = freeze({
    mode: 'create',
    context: structuredClone(host.getContext()),
    models: [],
    loading: false,
    working: false,
    error: null,
    status: '',
    drafts: { create: blankDraft(), edit: blankDraft(), draft: blankDraft(), final: blankDraft() },
    modeDocuments: { create: null, edit: null },
    draftImages: [],
    resultImages: [],
    selectedDraftId: null,
    selectedResultId: null,
    includeReferences: true,
    upscale: { enabled: false, preset: '3840', width: 3840, height: 3840 },
    upscaleInventory: null,
    progress: null,
    watching: true,
    uncertain: false,
    progressError: null,
    runningKey: null,
    recipes: recipes(),
    recipeWarnings: [],
  });
  let disposed = false,
    activatingMode = false,
    modelEpoch = 0,
    modelAbort: AbortController | null = null,
    pollTimer: ReturnType<typeof setTimeout> | null = null,
    jobSequence = 0;
  function update(change: Partial<GenerationState>) {
    if (disposed) return;
    state = freeze({ ...state, ...change });
    listeners.forEach(listener => listener());
  }
  function draftChange(key: DraftKey, change: Partial<GenerationDraft>) {
    update({ drafts: { ...state.drafts, [key]: { ...state.drafts[key], ...change } } });
  }
  const modelFor = (key: DraftKey) => state.models.find(model => model.id === state.drafts[key].modelId);
  const selectedDraft = () => state.draftImages.find(item => item.session.id === state.selectedDraftId);
  const selectedResult = () => state.resultImages.find(item => item.session.id === state.selectedResultId);
  const backgroundResult = () => {
    const document =
      state.mode === 'refine' ? (selectedResult() ?? selectedDraft())?.session : state.modeDocuments[state.mode];
    return document && (document.generation || document.upscale) ? document : null;
  };
  async function applyGeneratedBackground() {
    const document = backgroundResult(),
      context = structuredClone(host.getContext()),
      target = context.backgroundTarget;
    if (state.working || context.busy || !document || !target || target.id === document.id) return false;
    update({ working: true, error: null, progress: null, runningKey: null, status: 'Applying generated background…' });
    try {
      const accepted = await host.applyGeneratedBackground(document, context);
      update({
        status: accepted
          ? `Generated background applied to ${target.name}.`
          : 'The background was not applied. Review the editor status; the generated image remains available.',
      });
      return accepted;
    } catch (error) {
      update({
        error: errorText(error),
        status: 'The generated image remains available. The operation was not retried.',
      });
      return false;
    } finally {
      update({ working: false, context: structuredClone(host.getContext()) });
    }
  }
  function applyCatalog(models: GenerationModel[]) {
    update({ models });
    for (const key of keys)
      if (!state.drafts[key].modelId) {
        const candidates = models.filter(model =>
          key === 'final'
            ? model.capabilities.image_reference
            : key === 'edit'
              ? supportsReferences(model)
              : model.capabilities.text_to_image,
        );
        const preferred =
          key === 'final'
            ? ['flux2-klein-9b', 'qwen']
            : key === 'draft'
              ? ['flux2-klein-4b', 'z-image-turbo', 'qwen']
              : ['qwen'];
        const chosen =
          preferred.map(id => candidates.find(item => item.id === id && item.available)).find(Boolean) ??
          candidates.find(item => item.available) ??
          candidates[0];
        if (chosen) chooseModel(key, chosen.id);
      }
  }
  function referencesFor(key: DraftKey): GenerationReference[] {
    if (key !== 'final') return state.drafts[key].references;
    const image = selectedDraft();
    if (!image) return [];
    return [
      reference(image.session),
      ...(state.includeReferences ? image.references.filter(item => item.id !== image.session.id) : []),
    ];
  }
  function chooseModel(key: DraftKey, id: string, variant?: string) {
    if (state.working) return;
    const model = state.models.find(item => item.id === id),
      before = state.drafts[key];
    if (!model) {
      draftChange(key, { modelId: id, variant: variant ?? before.variant });
      return;
    }
    const same = before.modelId === id;
    const editSource = key === 'edit' ? state.modeDocuments.edit : null;
    const size = host.sizeMath.fitDimensions(
      {
        width: before.modelId ? before.width : (editSource?.width ?? model.defaults.width ?? 1024),
        height: before.modelId ? before.height : (editSource?.height ?? model.defaults.height ?? 1024),
        ratio: before.ratio,
        locked: before.locked,
      },
      workflowLimits(model, referencesFor(key).length > 0),
    );
    draftChange(key, {
      modelId: id,
      variant: variant ?? (same ? before.variant : model.defaults.variant),
      ...size,
      steps: same ? before.steps : model.defaults.steps,
      guidance: same ? before.guidance : model.defaults.guidance,
      denoise: model.defaults.denoise ?? before.denoise,
      transparent: !!model.capabilities.transparent && before.transparent,
      loras: same ? before.loras : [],
    });
  }
  async function refreshModels(force = false) {
    if (state.working) return;
    const epoch = ++modelEpoch;
    modelAbort?.abort();
    modelAbort = new AbortController();
    update({ loading: true, error: null });
    try {
      const [catalog, upscale] = await Promise.allSettled([
        api.models(force, modelAbort.signal),
        api.upscaleModels(modelAbort.signal),
      ]);
      if (disposed || epoch !== modelEpoch) return;
      if (catalog.status === 'rejected') throw catalog.reason;
      update({
        upscaleInventory: upscale.status === 'fulfilled' ? upscale.value : state.upscaleInventory,
        progressError: upscale.status === 'rejected' ? 'Upscale availability could not be checked.' : null,
      });
      applyCatalog(catalog.value.models);
    } catch (error) {
      if (epoch === modelEpoch) update({ error: errorText(error) });
    } finally {
      if (epoch === modelEpoch) update({ loading: false });
    }
  }
  function acceptCatalog(models: GenerationModel[]) {
    // A newer catalog accepted from the model browser must win over an older
    // background refresh still in flight. Only metadata reads are aborted.
    modelEpoch++;
    modelAbort?.abort();
    update({ loading: false });
    applyCatalog(models);
  }
  function bindEdit(document: EditorDocument) {
    const old = state.modeDocuments.edit,
      draft = state.drafts.edit;
    const references = [
      reference(document),
      ...draft.references.filter(item => item.id !== document.id && item.id !== old?.id),
    ];
    update({ modeDocuments: { ...state.modeDocuments, edit: structuredClone(document) } });
    const initialSize = !old
      ? host.sizeMath.fitDimensions(
          { width: document.width, height: document.height, ratio: document.width / document.height, locked: true },
          workflowLimits(modelFor('edit'), true),
        )
      : {};
    draftChange('edit', {
      references,
      missingReferenceCount: 0,
      ...initialSize,
      ...(!old ? { ratio: document.width / document.height, aspect: 'custom' } : {}),
    });
  }
  const unsubscribe = host.subscribeContext(() => {
    const context = structuredClone(host.getContext());
    update({ context });
    if (
      !state.working &&
      !activatingMode &&
      state.mode === 'edit' &&
      context.document &&
      context.document.id !== state.modeDocuments.edit?.id
    )
      bindEdit(context.document);
  });
  async function setMode(mode: GenerationMode, source?: EditorDocument) {
    if (state.working || activatingMode) return false;
    const shown =
      mode === 'edit'
        ? (source ?? state.modeDocuments.edit ?? host.getContext().document)
        : mode === 'create'
          ? state.modeDocuments.create
          : null;
    if (mode === 'edit' && !shown) {
      update({ error: 'Open an image before using Edit.' });
      return false;
    }
    activatingMode = true;
    try {
      if (!(await host.activateMode(mode, shown))) return false;
      if (mode === 'edit' && shown) bindEdit(shown);
      update({ mode, error: null });
      return true;
    } finally {
      activatingMode = false;
    }
  }
  function addReference(key: DraftKey, document: EditorDocument) {
    if (state.working || key === 'final') return false;
    const model = modelFor(key),
      draft = state.drafts[key];
    if (!supportsReferences(model) || draft.references.length >= (model?.capabilities.max_references ?? 0)) {
      update({ error: 'The selected model cannot accept another image input.' });
      return false;
    }
    if (draft.references.some(item => item.id === document.id)) return true;
    draftChange(key, { references: [...draft.references, reference(document)] });
    update({ error: null });
    return true;
  }
  function addDraft(document: EditorDocument, references: GenerationReference[] = []) {
    const item = { session: structuredClone(document), references: structuredClone(references) };
    update({
      draftImages: state.draftImages.some(image => image.session.id === document.id)
        ? state.draftImages
        : [...state.draftImages, item],
      selectedDraftId: document.id,
      selectedResultId: state.resultImages.filter(image => image.draftId === document.id).at(-1)?.session.id ?? null,
    });
  }
  function errorsFor(key: DraftKey) {
    const errors = validateDraft(key, state.drafts[key], modelFor(key), referencesFor(key), host.sizeMath);
    if (key === 'edit' && !state.modeDocuments.edit) errors.unshift('Open an image before using Edit.');
    if (key === 'final' && !selectedDraft()) errors.unshift('Choose a draft to refine.');
    if (state.uncertain)
      errors.unshift('The previous request outcome is unknown. Check backend operation status before running again.');
    return errors;
  }
  function stopPolling() {
    if (pollTimer !== null) clearTimeout(pollTimer);
    pollTimer = null;
  }
  async function poll(sequence: number, baseline: string | null, model: string) {
    if (disposed || sequence !== jobSequence || !state.working || !state.watching) return;
    try {
      const progress = await api.progress();
      if (sequence !== jobSequence || !state.watching || !state.working) return;
      if (progress.job_id && progress.job_id !== baseline && (!progress.model || progress.model === model))
        update({ progress, progressError: null });
    } catch (error) {
      if (sequence === jobSequence && state.working)
        update({ progressError: 'Progress updates are unavailable. The request has not been cancelled.' });
    }
    if (!disposed && sequence === jobSequence && state.working && state.watching)
      pollTimer = setTimeout(() => void poll(sequence, baseline, model), options.pollMilliseconds ?? 1000);
  }
  function current(context: GenerationContext) {
    const now = host.getContext();
    return context.navigationEpoch === now.navigationEpoch && context.document?.id === now.document?.id;
  }
  function upscaleTarget(document: EditorDocument) {
    if (state.upscale.preset === 'custom') return { width: state.upscale.width, height: state.upscale.height };
    const ratio = Number(state.upscale.preset) / Math.max(document.width, document.height),
      bounds = host.sizeMath.sizeLimits(state.upscaleInventory?.limits ?? {});
    return {
      width: Math.round((document.width * ratio) / bounds.width.step) * bounds.width.step,
      height: Math.round((document.height * ratio) / bounds.height.step) * bounds.height.step,
    };
  }
  function validUpscale(document: EditorDocument) {
    const target = upscaleTarget(document),
      bounds = host.sizeMath.sizeLimits(state.upscaleInventory?.limits ?? {});
    return (
      state.upscaleInventory?.enabled &&
      state.upscaleInventory.model.available &&
      Number.isSafeInteger(target.width) &&
      Number.isSafeInteger(target.height) &&
      target.width >= Math.max(document.width, bounds.width.min) &&
      target.height >= Math.max(document.height, bounds.height.min) &&
      (target.width > document.width || target.height > document.height) &&
      target.width <= bounds.width.max &&
      target.height <= bounds.height.max &&
      target.width % bounds.width.step === 0 &&
      target.height % bounds.height.step === 0 &&
      target.width * target.height <= bounds.pixels &&
      Math.min(
        Math.abs(target.height - (target.width * document.height) / document.width),
        Math.abs(target.width - (target.height * document.width) / document.height),
      ) <= 2
    );
  }
  function recordResult(document: EditorDocument, draftId: string | null) {
    update({
      resultImages: [
        ...state.resultImages,
        { session: structuredClone(document), references: [], ...(draftId ? { draftId } : {}) },
      ],
      selectedResultId: state.selectedDraftId === draftId ? document.id : state.selectedResultId,
    });
  }
  async function requestRun(key: DraftKey | 'upscale') {
    if (state.working || state.context.busy || state.loading) return;
    const errors = key === 'upscale' ? [] : errorsFor(key);
    if (errors.length) {
      update({ error: errors.join(' ') });
      return;
    }
    const upscaleSource = selectedResult() ?? selectedDraft();
    if (key === 'upscale' && (!upscaleSource || !validUpscale(upscaleSource.session) || state.uncertain)) {
      update({
        error:
          state.upscaleInventory?.model.reason ||
          'Choose a supported larger output that preserves the image aspect ratio.',
      });
      return;
    }
    const invocationContext = structuredClone(host.getContext());
    const chosenDraftId = state.selectedDraftId,
      mode = state.mode,
      payload =
        key === 'upscale'
          ? null
          : generationPayload(structuredClone(state.drafts[key]), modelFor(key)!, structuredClone(referencesFor(key)));
    const references = key === 'upscale' ? [] : structuredClone(referencesFor(key));
    if (
      key === 'final' &&
      state.upscale.enabled &&
      !validUpscale({ ...docForSize(), width: payload!.width, height: payload!.height })
    ) {
      update({ error: 'The configured upscale must enlarge the refinement canvas and preserve its aspect ratio.' });
      return;
    }
    const sequence = ++jobSequence;
    let baseline: string | null = null;
    update({
      working: true,
      runningKey: key,
      watching: true,
      progress: null,
      progressError: null,
      error: null,
      status: '',
    });
    try {
      await host.runGeneration(async context => {
        if (!current(invocationContext))
          throw new GenerationApiError(
            'The active document changed before the request started. No image operation was submitted.',
            409,
          );
        try {
          baseline = (await api.progress()).job_id;
        } catch {
          /* POST remains authoritative; no synthetic progress. */
        }
        void poll(sequence, baseline, key === 'upscale' ? 'seedvr2' : payload!.model);
        const result =
          key === 'upscale'
            ? await api.upscale(upscaleSource!.session, upscaleTarget(upscaleSource!.session))
            : await api.generate(payload!);
        if (disposed || sequence !== jobSequence) return;
        if (key === 'draft') addDraft(result.session, references);
        else if (key === 'final' || key === 'upscale') recordResult(result.session, chosenDraftId);
        else {
          update({ modeDocuments: { ...state.modeDocuments, [key]: structuredClone(result.session) } });
          if (current(context) && mode === state.mode) await host.acceptResult(result.session, context, mode);
          if (key === 'edit') bindEdit(result.session);
        }
        update({
          status: `Image created${result.seed === undefined ? '' : ` · Seed ${result.seed}`}${result.library_warning ? ` · ${result.library_warning}` : ' · Library copy saved.'}`,
        });
        if (key === 'final' && state.upscale.enabled) {
          // This second operation is explicitly selected by the user. A failure
          // retains the completed refinement and never resubmits either stage.
          try {
            update({ runningKey: 'upscale', progress: null });
            stopPolling();
            baseline = (await api.progress()).job_id;
            void poll(sequence, baseline, 'seedvr2');
            const upscaled = await api.upscale(result.session, upscaleTarget(result.session));
            recordResult(upscaled.session, chosenDraftId);
            update({ status: 'Refinement and upscale saved as separate library copies.' });
          } catch (error) {
            update({
              error: 'Refinement saved. Upscale failed: ' + errorText(error),
              uncertain: !(error instanceof GenerationApiError) || error.status >= 500,
            });
          }
        }
      });
    } catch (error) {
      const uncertain = !(error instanceof GenerationApiError) || error.status >= 500;
      update({
        error: errorText(error),
        uncertain,
        status: uncertain
          ? 'The backend may still be running. Check operation status; the request was not retried.'
          : '',
      });
    } finally {
      stopPolling();
      if (sequence === jobSequence)
        update({
          working: false,
          runningKey: null,
          progress: null,
          progressError: state.uncertain ? state.progressError : null,
          context: structuredClone(host.getContext()),
        });
    }
  }
  function docForSize(): EditorDocument {
    return {
      id: 'size-validation',
      revision: 0,
      name: '',
      width: state.drafts.final.width,
      height: state.drafts.final.height,
    };
  }
  function loraPort(key: DraftKey): LoraDraftPort {
    return {
      contextId: key,
      context: key === 'draft' || key === 'final' ? key : 'generate',
      read: () => ({
        modelId: state.drafts[key].modelId,
        modelLabel: modelFor(key)?.label,
        selected: structuredClone(state.drafts[key].loras),
        referenceCount: referencesFor(key).length,
        supportsLoras: modelFor(key)?.capabilities.lora !== false && modelFor(key)?.capabilities.loras !== false,
      }),
      setSelected: loras => {
        if (!state.working) draftChange(key, { loras: structuredClone(loras) });
      },
      appendPrompt: text => {
        if (!state.working)
          draftChange(key, { prompt: [state.drafts[key].prompt.trim(), text.trim()].filter(Boolean).join(', ') });
      },
      applySampling: value => {
        if (!state.working) draftChange(key, value);
      },
    };
  }
  function writeRecipes(values: RefinementRecipe[]) {
    if (!storage) throw new Error('Recipe storage is unavailable.');
    storage.setItem(RECIPE_KEY, JSON.stringify(values));
    update({ recipes: values });
  }
  function restoreDocument(key: DraftKey, document: EditorDocument) {
    if (state.working) return;
    const value = document.generation;
    if (!value || typeof value !== 'object') return;
    const saved = value as Record<string, unknown>,
      prior = state.drafts[key];
    draftChange(key, {
      modelId: String(saved.model ?? prior.modelId),
      variant: String(saved.variant ?? prior.variant),
      prompt: String(saved.prompt ?? ''),
      negativePrompt: String(saved.negative_prompt ?? ''),
      width: Number(saved.width ?? document.width),
      height: Number(saved.height ?? document.height),
      steps: Number(saved.steps ?? prior.steps),
      guidance: Number(saved.guidance ?? prior.guidance),
      seed: saved.seed == null ? '' : String(saved.seed),
      transparent: saved.transparent === true,
      denoise: Number(saved.denoise ?? prior.denoise),
      missingReferenceCount: Number(saved.reference_count ?? 0),
      references: [],
      loras: (Array.isArray(saved.loras) ? saved.loras : []).map(item => ({ ...item, missing: true })),
    });
    update({
      status:
        'Saved generation settings loaded. Original references and adapter availability must be checked before reproducing this image.',
    });
  }
  async function loadRecipe(name: string) {
    if (state.working || state.loading) return;
    const recipe = state.recipes.find(item => item.name === name);
    if (!recipe) return;
    const warnings: string[] = [];
    update({ loading: true, error: null });
    try {
      for (const key of ['draft', 'final'] as const) {
        const saved = recipe.stages[key],
          modelId = String(saved.model ?? ''),
          model = state.models.find(item => item.id === modelId);
        const loras = (Array.isArray(saved.loras) ? saved.loras.slice(0, 3) : []).map(item => ({
          ...item,
          missing: true,
        })) as LoraSelection[];
        if (!model?.available)
          warnings.push(`${key === 'draft' ? 'Draft' : 'Refinement'} model unavailable: ${modelId}`);
        if (loras.length) {
          try {
            const inventory = await api.installedLoras(modelId);
            for (const item of loras) {
              const installed = inventory.installed.find(value => value.id === item.id && value.supported !== false);
              item.missing = !installed || !Number.isFinite(item.strength) || item.strength < -2 || item.strength > 2;
              if (installed) Object.assign(item, { title: installed.title, usage: installed.usage });
              if (item.missing) warnings.push(`Adapter unavailable or invalid: ${item.title || item.id}`);
            }
          } catch {
            warnings.push(`Could not verify ${key} adapters.`);
          }
        }
        draftChange(key, {
          modelId,
          variant: String(saved.variant ?? ''),
          prompt: String(saved.prompt ?? ''),
          width: Number(saved.width),
          height: Number(saved.height),
          steps: Number(saved.steps),
          guidance: Number(saved.guidance),
          seed: String(saved.seed ?? ''),
          transparent: saved.transparent === true,
          loras,
        });
      }
      draftChange('final', { negativePrompt: recipe.negative ?? '', aspect: recipe.aspect ?? 'custom' });
      draftChange('draft', { denoise: Number(recipe.denoise ?? 0.65) });
      if (recipe.upscale?.enabled && !state.upscaleInventory?.model.available)
        warnings.push('SeedVR2 is unavailable; the saved upscale option cannot run.');
      update({
        includeReferences: !!recipe.includeReferences,
        upscale: {
          enabled: !!recipe.upscale?.enabled,
          preset: recipe.upscale?.preset ?? '3840',
          width: Number(recipe.upscale?.width) || 3840,
          height: Number(recipe.upscale?.height) || 3840,
        },
        recipeWarnings: warnings,
        status: `Loaded ${recipe.name}. Choose the draft and reference images for this run.`,
      });
    } catch (error) {
      update({ error: 'Recipe could not be fully loaded: ' + errorText(error) });
    } finally {
      update({ loading: false });
    }
  }
  return {
    getSnapshot: () => state,
    subscribe(listener: () => void) {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
    refreshModels,
    acceptCatalog,
    setMode,
    chooseModel,
    modelFor,
    referencesFor,
    errorsFor,
    addReference,
    addDraft,
    selectedDraft,
    selectedResult,
    backgroundResult,
    applyGeneratedBackground,
    validUpscale,
    upscaleTarget,
    getLoraPort: loraPort,
    restoreDocument,
    activeDraftKey: (): DraftKey => (state.mode === 'refine' ? 'draft' : state.mode),
    boundsFor: (key: DraftKey) =>
      host.sizeMath.dimensionBounds(state.drafts[key], workflowLimits(modelFor(key), referencesFor(key).length > 0)),
    forgetDocuments(ids: string[]) {
      const removed = new Set(ids),
        drafts = { ...state.drafts };
      for (const key of keys) {
        const draft = drafts[key],
          references = draft.references.filter(item => !removed.has(item.id));
        drafts[key] = {
          ...draft,
          references,
          missingReferenceCount:
            references.length < draft.references.length
              ? Math.max(draft.missingReferenceCount, draft.references.length)
              : draft.missingReferenceCount,
        };
      }
      update({
        drafts,
        modeDocuments: {
          create: removed.has(state.modeDocuments.create?.id ?? '') ? null : state.modeDocuments.create,
          edit: removed.has(state.modeDocuments.edit?.id ?? '') ? null : state.modeDocuments.edit,
        },
        draftImages: state.draftImages
          .filter(item => !removed.has(item.session.id))
          .map(item => ({ ...item, references: item.references.filter(ref => !removed.has(ref.id)) })),
        resultImages: state.resultImages.filter(item => !removed.has(item.session.id)),
        selectedDraftId: removed.has(state.selectedDraftId ?? '') ? null : state.selectedDraftId,
        selectedResultId: removed.has(state.selectedResultId ?? '') ? null : state.selectedResultId,
      });
    },
    run: requestRun,
    setDraft(key: DraftKey, change: Partial<GenerationDraft>) {
      if (!state.working) draftChange(key, change);
    },
    commitSize(key: DraftKey, axis: 'width' | 'height' = 'width') {
      const draft = state.drafts[key];
      draftChange(
        key,
        host.sizeMath.fitDimensions({ ...draft, axis }, workflowLimits(modelFor(key), referencesFor(key).length > 0)),
      );
    },
    setAspect(key: DraftKey, aspect: string) {
      if (state.working) return;
      if (aspect === 'custom') {
        draftChange(key, { aspect });
        return;
      }
      const [w, h] = aspect.split(':').map(Number);
      if (!(w > 0 && h > 0)) return;
      const draft = state.drafts[key],
        ratio = w / h;
      draftChange(key, {
        aspect,
        ratio,
        locked: true,
        ...host.sizeMath.fitDimensions(
          { ...draft, ratio, locked: true },
          workflowLimits(modelFor(key), referencesFor(key).length > 0),
        ),
      });
    },
    removeReference(key: DraftKey, id: string) {
      if (!state.working && !(key === 'edit' && state.drafts.edit.references[0]?.id === id))
        draftChange(key, { references: state.drafts[key].references.filter(item => item.id !== id) });
    },
    moveReference(key: DraftKey, id: string, direction: number) {
      if (state.working) return;
      const values = [...state.drafts[key].references],
        from = values.findIndex(item => item.id === id),
        to = from + direction;
      if (from < 0 || to < (key === 'edit' ? 1 : 0) || to >= values.length || (key === 'edit' && from === 0)) return;
      [values[from], values[to]] = [values[to], values[from]];
      draftChange(key, { references: values });
    },
    async importReferences(key: DraftKey, files: File[]) {
      if (state.working || state.loading) return;
      const target = state.drafts[key].modelId,
        available = (modelFor(key)?.capabilities.max_references ?? 0) - state.drafts[key].references.length;
      if (files.length > available) {
        update({ error: `This model has room for ${Math.max(0, available)} more image inputs.` });
        return;
      }
      update({ loading: true, error: null });
      try {
        for (const file of files) {
          if (target !== state.drafts[key].modelId) break;
          const document = await api.importReference(file);
          if (target === state.drafts[key].modelId) addReference(key, document);
        }
      } catch (error) {
        update({ error: errorText(error) });
      } finally {
        update({ loading: false });
      }
    },
    useCurrent(key: DraftKey) {
      const document = host.getContext().document;
      if (document) return addReference(key, document);
      return false;
    },
    useCurrentDraft() {
      const document = host.getContext().document;
      if (document && !state.working) addDraft(document);
    },
    selectDraft(id: string) {
      if (!state.working && state.draftImages.some(item => item.session.id === id))
        update({
          selectedDraftId: id,
          selectedResultId: state.resultImages.filter(item => item.draftId === id).at(-1)?.session.id ?? null,
        });
    },
    selectResult(id: string) {
      if (
        !state.working &&
        state.resultImages.some(item => item.session.id === id && item.draftId === state.selectedDraftId)
      )
        update({ selectedResultId: id });
    },
    async openSelected(result = true) {
      if (state.working) return;
      const item = result ? selectedResult() : selectedDraft();
      if (item && (await host.acceptResult(item.session, host.getContext(), 'edit'))) {
        bindEdit(item.session);
        update({ mode: 'edit' });
      }
    },
    setIncludeReferences(includeReferences: boolean) {
      if (!state.working) update({ includeReferences });
    },
    setUpscale(change: Partial<GenerationState['upscale']>) {
      if (!state.working) update({ upscale: { ...state.upscale, ...change } });
    },
    stopWatching() {
      stopPolling();
      update({ watching: false, status: 'Progress updates paused. The backend operation has not been cancelled.' });
    },
    async checkOperation() {
      try {
        const progress = await api.progress();
        update({
          progress,
          uncertain: progress.active,
          progressError: null,
          status: progress.active
            ? 'The backend still reports an active operation.'
            : 'The backend is idle. Check the generated library for any completed image before starting another request.',
        });
      } catch (error) {
        update({ progressError: errorText(error) });
      }
    },
    openAssets: (destination: 'reference' | 'draft') => host.openAssets(destination),
    openModels(key: DraftKey) {
      const draft = state.drafts[key];
      host.openModels({
        selectedModelId: draft.modelId,
        selectedVariant: draft.variant,
        onUse: (model, variant) => chooseModel(key, model, variant),
      });
    },
    openLoras: (key: DraftKey) => host.openLoras(loraPort(key)),
    saveRecipe(name: string) {
      if (state.working) return;
      name = name.trim();
      if (!name || name.length > 80) {
        update({ error: 'Enter a recipe name of 1–80 characters.' });
        return;
      }
      const capture = (key: 'draft' | 'final') => {
        const draft = state.drafts[key];
        return {
          model: draft.modelId,
          variant: draft.variant,
          prompt: draft.prompt,
          width: String(draft.width),
          height: String(draft.height),
          steps: String(draft.steps),
          guidance: String(draft.guidance),
          seed: draft.seed,
          transparent: draft.transparent,
          loras: draft.loras.map(({ id, title, strength }) => ({ id, title, strength })),
        };
      };
      const recipe: RefinementRecipe = {
        schema: 1,
        name,
        stages: { draft: capture('draft'), final: capture('final') },
        aspect: state.drafts.final.aspect,
        negative: state.drafts.final.negativePrompt,
        denoise: String(state.drafts.draft.denoise),
        includeReferences: state.includeReferences,
        upscale: { ...state.upscale, width: String(state.upscale.width), height: String(state.upscale.height) },
      };
      const values = [...state.recipes.filter(item => item.name !== name), recipe];
      if (values.length > 40) {
        update({ error: 'Delete a recipe before saving more than 40.' });
        return;
      }
      try {
        writeRecipes(values);
        update({ status: `Saved ${name}. Both stages and output settings are included.` });
      } catch (error) {
        update({ error: errorText(error) });
      }
    },
    loadRecipe,
    deleteRecipe(name: string) {
      if (!state.working)
        try {
          writeRecipes(state.recipes.filter(item => item.name !== name));
        } catch (error) {
          update({ error: errorText(error) });
        }
    },
    dispose() {
      disposed = true;
      stopPolling();
      modelAbort?.abort();
      unsubscribe();
      listeners.clear(); /* Do not abort an in-flight inference request. */
    },
  };
}
export type GenerationController = ReturnType<typeof createGenerationController>;
