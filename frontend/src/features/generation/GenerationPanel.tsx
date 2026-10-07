import { useEffect, useMemo, useRef, useState, useSyncExternalStore } from 'react';
import {
  Accordion,
  AccordionHeader,
  AccordionItem,
  AccordionPanel,
  Button,
  Checkbox,
  Field,
  Input,
  Menu,
  MenuItemRadio,
  MenuList,
  MenuPopover,
  MenuTrigger,
  ProgressBar,
  Tab,
  TabList,
  Textarea,
  Tooltip,
} from '@fluentui/react-components';
import { Comparison } from './Comparison.tsx';
import { refinementPreview } from './previewCamera.ts';
import { DimensionControls } from './DimensionControls.tsx';
import { Icon } from '../shell/Icon.tsx';
import { hardwareUsageSummary } from './hardwareUsage.ts';
import type { GenerationController } from './controller.ts';
import type { DraftKey, GenerationMode } from './types.ts';
import './generation.css';

function GenerationChoiceMenu({
  id,
  label,
  value,
  choices,
  disabled,
  onSelect,
}: {
  id: string;
  label: string;
  value: string;
  choices: { value: string; label: string }[];
  disabled?: boolean;
  onSelect(value: string): void;
}) {
  const selected = choices.find(choice => choice.value === value)?.label || value || 'No options available';
  return (
    <Menu>
      <MenuTrigger disableButtonEnhancement>
        <Button
          id={id}
          size="small"
          appearance="subtle"
          className="li-generation-choice"
          aria-label={`${label}: ${selected}`}
          disabled={disabled || !choices.length}
        >
          <span title={selected}>{selected}</span>
          <Icon name="chevron-down" />
        </Button>
      </MenuTrigger>
      <MenuPopover className="li-generation-choice-popover" data-react-owned="true">
        <MenuList aria-label={label} checkedValues={{ [id]: [value] }}>
          {choices.map(choice => (
            <MenuItemRadio
              key={choice.value}
              name={id}
              value={choice.value}
              disabled={disabled}
              onClick={() => onSelect(choice.value)}
            >
              {choice.label}
            </MenuItemRadio>
          ))}
        </MenuList>
      </MenuPopover>
    </Menu>
  );
}

function Stage({ controller, stage }: { controller: GenerationController; stage: DraftKey }) {
  const state = useSyncExternalStore(controller.subscribe, controller.getSnapshot),
    draft = state.drafts[stage],
    model = controller.modelFor(stage),
    cap = model?.capabilities;
  const refs = controller.referencesFor(stage),
    errors = controller.errorsFor(stage),
    disabled = state.working || state.context.busy || state.loading || state.ejecting,
    fileInput = useRef<HTMLInputElement>(null);
  const title = { create: 'Create new', edit: 'Edit image', draft: 'Draft', final: 'Refinement' }[stage];
  const textToImage = stage === 'create' || stage === 'draft';
  const bounds = controller.boundsFor(stage);
  const candidates = state.models.filter(item =>
    textToImage
      ? item.capabilities.text_to_image
      : stage === 'final'
        ? item.capabilities.image_reference
        : (item.capabilities.max_references ?? 0) > 0,
  );
  const modelChoices = candidates.map(item => ({
    value: item.id,
    label: `${item.label}${item.available ? '' : ' · unavailable'}`,
  }));
  if (draft.modelId && !candidates.some(item => item.id === draft.modelId))
    modelChoices.unshift({ value: draft.modelId, label: `Unavailable in this mode · ${draft.modelId}` });
  const variantChoices =
    model?.variants.map(item => ({
      value: item.id,
      label: `${item.label || item.id}${item.available ? '' : ' · unavailable'}`,
    })) || [];
  if (draft.variant && !model?.variants.some(item => item.id === draft.variant))
    variantChoices.unshift({ value: draft.variant, label: `Unavailable · ${draft.variant}` });
  const supportsReferences = (cap?.max_references ?? 0) > 0;
  const showReferences = supportsReferences || refs.length > 0 || draft.missingReferenceCount > 0;
  const supportsLoras = !!model && cap?.lora !== false && cap?.loras !== false;
  return (
    <section className="li-generation-stage" aria-label={`${title} settings`}>
      <header>
        <h3>{title}</h3>
        <Tooltip content="Browse models" relationship="description">
          <Button
            size="small"
            appearance="subtle"
            className="li-generation-icon-button"
            aria-label={`Browse ${title.toLowerCase()} models`}
            disabled={disabled}
            icon={<Icon name="info" />}
            onClick={() => controller.openModels(stage)}
          />
        </Tooltip>
      </header>
      <Field label="Model">
        <GenerationChoiceMenu
          id={`${stage}-model`}
          label={`${title} model`}
          value={draft.modelId}
          disabled={disabled}
          choices={modelChoices}
          onSelect={value => controller.chooseModel(stage, value)}
        />
      </Field>
      <Field label="Precision">
        <GenerationChoiceMenu
          id={`${stage}-precision`}
          label={`${title} precision`}
          value={draft.variant}
          disabled={disabled}
          choices={variantChoices}
          onSelect={value => controller.setDraft(stage, { variant: value })}
        />
      </Field>
      <Field label={stage === 'edit' || stage === 'final' ? 'Describe the changes' : 'Prompt'}>
        <Textarea
          resize="vertical"
          value={draft.prompt}
          disabled={disabled}
          maxLength={4000}
          onChange={(_, data) => controller.setDraft(stage, { prompt: data.value })}
        />
      </Field>
      <DimensionControls
        label={title}
        width={draft.width}
        height={draft.height}
        bounds={bounds}
        linked={draft.locked}
        disabled={disabled}
        memoryInfo={!!model?.limits.resolution_note}
        onLink={linked => controller.setDimensionsLinked(stage, linked)}
        onChange={(axis, value) => controller.setDraft(stage, { [axis]: value, aspect: 'custom' })}
        onCommit={axis => controller.commitSize(stage, axis)}
        preset={{
          id: `${stage}-aspect`,
          label: `${title} aspect ratio`,
          value: draft.aspect,
          choices: [
            { value: '1:1', label: '1:1 · Square' },
            { value: '3:2', label: '3:2 · Landscape' },
            { value: '2:3', label: '2:3 · Portrait' },
            { value: '16:9', label: '16:9 · Widescreen' },
            { value: 'custom', label: 'Custom' },
          ],
          disabled,
          onSelect: value => controller.setAspect(stage, value),
        }}
      />
      {!!cap?.transparent && (
        <Checkbox
          label="Transparent PNG"
          checked={draft.transparent}
          disabled={disabled}
          onChange={(_, data) => controller.setDraft(stage, { transparent: data.checked === true })}
        />
      )}
      <Accordion multiple collapsible className="li-generation-disclosures">
        <AccordionItem value="sampling">
          <AccordionHeader>Sampling and seed</AccordionHeader>
          <AccordionPanel>
            <div className="li-generation-size">
              <Field label="Steps">
                <Input
                  size="small"
                  type="number"
                  min={model?.limits.min_steps ?? 1}
                  max={model?.limits.max_steps ?? 100}
                  value={String(draft.steps)}
                  disabled={disabled}
                  onChange={(_, data) => controller.setDraft(stage, { steps: Number(data.value) })}
                />
              </Field>
              <Field label="Guidance">
                <Input
                  size="small"
                  type="number"
                  step="0.1"
                  min={model?.limits.min_guidance ?? 1}
                  max={model?.limits.max_guidance ?? 1}
                  value={String(draft.guidance)}
                  disabled={disabled}
                  onChange={(_, data) => controller.setDraft(stage, { guidance: Number(data.value) })}
                />
              </Field>
            </div>
            <Field label="Seed" hint="Blank uses a new seed; zero keeps a fixed seed.">
              <Input
                size="small"
                value={draft.seed}
                disabled={disabled}
                onChange={(_, data) => controller.setDraft(stage, { seed: data.value })}
              />
            </Field>
            {cap?.negative_prompt && (
              <Field label="Negative prompt">
                <Textarea
                  value={draft.negativePrompt}
                  disabled={disabled || draft.guidance <= 1}
                  maxLength={2000}
                  onChange={(_, data) => controller.setDraft(stage, { negativePrompt: data.value })}
                />
              </Field>
            )}
            {cap?.denoise && !!refs.length && (
              <Field label="Variation strength">
                <Input
                  size="small"
                  type="number"
                  min={0.05}
                  max={1}
                  step={0.05}
                  value={String(draft.denoise)}
                  disabled={disabled}
                  onChange={(_, data) => controller.setDraft(stage, { denoise: Number(data.value) })}
                />
              </Field>
            )}
          </AccordionPanel>
        </AccordionItem>
        {showReferences && (
          <AccordionItem value="references">
            <AccordionHeader>
              Image inputs · {refs.length}
              {supportsReferences ? `/${cap?.max_references}` : ''}
            </AccordionHeader>
            <AccordionPanel>
              {stage !== 'final' && supportsReferences && (
                <div className="li-generation-actions">
                  <Button
                    size="small"
                    disabled={disabled || !state.context.document || refs.length >= (cap?.max_references ?? 0)}
                    onClick={() => controller.useCurrent(stage)}
                  >
                    Use current
                  </Button>
                  <Button
                    size="small"
                    disabled={disabled || refs.length >= (cap?.max_references ?? 0)}
                    onClick={() => fileInput.current?.click()}
                  >
                    Add files
                  </Button>
                  <Button
                    size="small"
                    appearance="subtle"
                    disabled={disabled}
                    onClick={() => controller.openAssets('reference')}
                  >
                    Assets
                  </Button>
                </div>
              )}
              <input
                ref={fileInput}
                type="file"
                hidden
                multiple
                accept=".jpg,.jpeg,.png,.tif,.tiff,.webp"
                onChange={event => {
                  const files = [...(event.currentTarget.files ?? [])];
                  event.currentTarget.value = '';
                  void controller.importReferences(stage, files);
                }}
              />
              {refs.map((item, index) => (
                <div className="li-generation-reference" key={item.id}>
                  <img src={item.thumbnail} alt="" loading="lazy" />
                  <span title={item.name}>
                    {index + 1}. {item.name}
                  </span>
                  {stage !== 'final' && (
                    <>
                      <Tooltip content="Move earlier" relationship="description">
                        <Button
                          size="small"
                          appearance="subtle"
                          className="li-generation-icon-button"
                          aria-label={`Move ${item.name} earlier`}
                          disabled={disabled || index <= (stage === 'edit' ? 1 : 0)}
                          icon={<Icon name="arrow-up" />}
                          onClick={() => controller.moveReference(stage, item.id, -1)}
                        />
                      </Tooltip>
                      <Tooltip content="Remove image input" relationship="description">
                        <Button
                          size="small"
                          appearance="subtle"
                          className="li-generation-icon-button"
                          aria-label={`Remove ${item.name}`}
                          disabled={disabled || (stage === 'edit' && index === 0)}
                          icon={<Icon name="close" />}
                          onClick={() => controller.removeReference(stage, item.id)}
                        />
                      </Tooltip>
                    </>
                  )}
                </div>
              ))}
              {draft.missingReferenceCount > refs.length && (
                <p className="li-generation-warning">
                  The saved image used {draft.missingReferenceCount} references; reattach the original images to
                  reproduce it.
                </p>
              )}
            </AccordionPanel>
          </AccordionItem>
        )}
      </Accordion>
      {(supportsLoras || draft.loras.length > 0) && (
        <div className="li-generation-actions">
          <Button
            size="small"
            appearance="subtle"
            disabled={disabled || (!supportsLoras && !draft.loras.length)}
            onClick={() => controller.openLoras(stage)}
          >
            Styles / LoRAs · {draft.loras.length}
          </Button>
          <span className="li-generation-note">{draft.loras.map(item => item.title || item.id).join(', ')}</span>
        </div>
      )}
      {errors.length > 0 && (
        <p className="li-generation-warning" role="status">
          {errors[0]}
        </p>
      )}
      <Button appearance="primary" disabled={disabled || errors.length > 0} onClick={() => void controller.run(stage)}>
        {state.runningKey === stage
          ? 'Running…'
          : stage === 'edit'
            ? 'Apply edit'
            : stage === 'final'
              ? 'Refine selected draft'
              : stage === 'draft'
                ? 'Generate draft'
                : 'Generate image'}
      </Button>
    </section>
  );
}

export function GenerationPanel({ controller }: { controller: GenerationController }) {
  const state = useSyncExternalStore(controller.subscribe, controller.getSnapshot),
    disabled = state.working || state.context.busy || state.ejecting;
  const [recipeName, setRecipeName] = useState(''),
    [recipe, setRecipe] = useState('');
  const [refinementStep, setRefinementStep] = useState<'draft' | 'final'>('draft');
  const panel = useRef<HTMLElement>(null);
  useEffect(() => {
    if (panel.current) panel.current.scrollTop = 0;
  }, [state.mode]);
  useEffect(() => {
    const element = panel.current;
    if (!element) return;
    const owner = element.ownerDocument;
    let intersecting = false;
    const visible = () => controller.setHardwareVisible(intersecting && owner.visibilityState !== 'hidden');
    const observer = new IntersectionObserver(entries => {
      intersecting = entries.some(entry => entry.target === element && entry.isIntersecting);
      visible();
    });
    observer.observe(element);
    owner.addEventListener('visibilitychange', visible);
    return () => {
      observer.disconnect();
      owner.removeEventListener('visibilitychange', visible);
      controller.setHardwareVisible(false);
    };
  }, [controller]);
  const progress = state.progress,
    sampler =
      state.working && state.watching && progress?.active && progress.stage === 'sampling' ? progress.progress : null;
  const selectedDraft = controller.selectedDraft(),
    selectedResult = controller.selectedResult();
  const preview = refinementPreview(refinementStep, selectedDraft?.session, selectedResult?.session);
  const stageImages =
    refinementStep === 'draft'
      ? state.draftImages
      : state.resultImages.filter(item => item.draftId === state.selectedDraftId);
  const upscaleSource = (selectedResult ?? selectedDraft)?.session;
  const upscaleSize = useMemo(
    () => controller.upscaleSizeControls(),
    [
      controller,
      state.upscale,
      state.upscaleInventory,
      upscaleSource?.width,
      upscaleSource?.height,
      state.drafts.final.width,
      state.drafts.final.height,
    ],
  );
  const hardware = hardwareUsageSummary(state.hardware, state.hardwareError, state.hardwareLoading);
  const stopHint = state.stopping
    ? 'Waiting for the AI backend to stop this job'
    : controller.canStop()
      ? 'Stop the current image generation'
      : progress?.job_id
        ? 'This operation is finishing or cannot be stopped safely'
        : 'Waiting for the AI backend to identify this job';
  const ejectHint =
    state.working || state.context.busy
      ? 'Finish or stop the current operation before unloading GPU models'
      : state.ejecting
        ? 'Unloading GPU models…'
        : controller.canEjectModels()
          ? 'Unload GPU models; keep files on disk'
          : state.hardware?.comfy_connected
            ? 'Open the desktop app to unload GPU models'
            : 'Start the AI backend to unload its GPU models';
  return (
    <section
      ref={panel}
      className="li-generation"
      data-mode={state.mode}
      data-react-owned="true"
      aria-label="Image generation"
    >
      <header className="li-generation-header">
        <TabList
          size="small"
          aria-label="Generation mode"
          selectedValue={state.mode}
          onTabSelect={(_, data) => void controller.setMode(data.value as GenerationMode)}
        >
          <Tab value="create" disabled={disabled}>
            Create
          </Tab>
          <Tab value="edit" disabled={disabled || (!state.context.document && !state.modeDocuments.edit)}>
            Edit
          </Tab>
          <Tab value="refine" disabled={disabled}>
            Refine
          </Tab>
        </TabList>
        <Tooltip content="Refresh models" relationship="description">
          <Button
            size="small"
            appearance="subtle"
            className="li-generation-icon-button"
            aria-label="Refresh models"
            disabled={disabled || state.loading}
            icon={<Icon name="refresh" />}
            onClick={() => void controller.refreshModels(true)}
          />
        </Tooltip>
      </header>
      {state.mode !== 'refine' ? (
        <div className="li-generation-main">
          <Stage controller={controller} stage={state.mode} />
        </div>
      ) : (
        <div className="li-refinement">
          <div className="li-refinement-step">
            <TabList
              size="small"
              aria-label="Refinement step"
              selectedValue={refinementStep}
              onTabSelect={(_, data) => setRefinementStep(data.value as 'draft' | 'final')}
            >
              <Tab value="draft" disabled={disabled}>
                Draft
              </Tab>
              <Tab value="final" disabled={disabled}>
                Refine
              </Tab>
            </TabList>
            <p className="li-generation-note">
              {refinementStep === 'draft' ? 'Create or choose a starting image.' : 'Improve the selected draft.'}
            </p>
          </div>
          <div className="li-refinement-workspace">
            <div className="li-refinement-viewer">
              <Comparison document={preview.document} label={preview.label} notice={preview.notice} />
              {stageImages.length > 0 && (
                <div
                  className="li-refinement-strip"
                  aria-label={refinementStep === 'draft' ? 'Generated drafts' : 'Refined images'}
                >
                  {stageImages.map(item => (
                    <Tooltip key={item.session.id} content={item.session.name} relationship="description">
                      <Button
                        appearance="subtle"
                        size="small"
                        aria-label={`Show ${item.session.name}`}
                        aria-pressed={
                          item.session.id ===
                          (refinementStep === 'draft' ? state.selectedDraftId : state.selectedResultId)
                        }
                        disabled={disabled}
                        onClick={() =>
                          refinementStep === 'draft'
                            ? controller.selectDraft(item.session.id)
                            : controller.selectResult(item.session.id)
                        }
                      >
                        <img
                          src={`/api/local-remove/session/${encodeURIComponent(item.session.id)}/preview?revision=${item.session.revision}`}
                          alt=""
                          loading="lazy"
                        />
                      </Button>
                    </Tooltip>
                  ))}
                </div>
              )}
              <div className="li-generation-actions li-refinement-viewer-actions">
                <span className="li-generation-note" title={preview.document?.name}>
                  {preview.document?.name || 'No image selected'}
                </span>
                {refinementStep === 'draft' ? (
                  <Button
                    size="small"
                    appearance="subtle"
                    disabled={disabled || !selectedDraft}
                    onClick={() => void controller.openSelected(false)}
                  >
                    Open draft in editor
                  </Button>
                ) : (
                  <Button
                    size="small"
                    appearance="subtle"
                    disabled={disabled || !selectedResult}
                    onClick={() => void controller.openSelected(true)}
                  >
                    Open result in editor
                  </Button>
                )}
              </div>
            </div>
            <aside
              className="li-refinement-settings"
              aria-label={refinementStep === 'draft' ? 'Draft options' : 'Refine options'}
            >
              {refinementStep === 'draft' ? (
                <div className="li-generation-actions li-refinement-source">
                  <Button
                    size="small"
                    disabled={disabled || !state.context.document}
                    onClick={controller.useCurrentDraft}
                  >
                    Use current as draft
                  </Button>
                  <Button
                    size="small"
                    appearance="subtle"
                    disabled={disabled}
                    onClick={() => controller.openAssets('draft')}
                  >
                    Choose library draft
                  </Button>
                </div>
              ) : (
                <div className="li-refinement-source">
                  <p className="li-generation-note" title={selectedDraft?.session.name}>
                    Draft: {selectedDraft?.session.name || 'None selected'}
                  </p>
                  <Checkbox
                    label="Include original references"
                    checked={state.includeReferences}
                    disabled={disabled || !selectedDraft?.references.length}
                    onChange={(_, data) => controller.setIncludeReferences(data.checked === true)}
                  />
                </div>
              )}
              <Stage controller={controller} stage={refinementStep} />
              {refinementStep === 'final' && (
                <Accordion collapsible className="li-generation-upscale li-generation-disclosures">
                  <AccordionItem value="upscale">
                    <AccordionHeader>Upscale with SeedVR2</AccordionHeader>
                    <AccordionPanel>
                      <p className="li-generation-note">
                        {state.upscaleInventory?.model.available
                          ? 'Preserves transparency. Review faces and lettering after upscaling.'
                          : state.upscaleInventory?.model.reason || 'Upscale availability has not been checked.'}
                      </p>
                      <Checkbox
                        label="Upscale after refinement"
                        disabled={disabled || !state.upscaleInventory?.model.available}
                        checked={state.upscale.enabled}
                        onChange={(_, data) => controller.setUpscale({ enabled: data.checked === true })}
                      />
                      <DimensionControls
                        label="Upscale"
                        width={upscaleSize.width}
                        height={upscaleSize.height}
                        bounds={upscaleSize.bounds}
                        linked
                        disabled={disabled || state.upscale.preset !== 'custom'}
                        fixedLink="SeedVR2 keeps the selected image's original proportions."
                        memoryInfo={!!state.upscaleInventory?.limits.resolution_note}
                        onChange={(axis, value) => controller.setUpscale({ [axis]: value })}
                        onCommit={axis => controller.commitUpscaleSize(axis)}
                        preset={{
                          id: 'upscale-size',
                          label: 'Upscale size preset',
                          value: state.upscale.preset,
                          choices: [
                            { value: '3840', label: '4K long edge' },
                            { value: '2048', label: '2048 px long edge' },
                            { value: 'custom', label: 'Custom' },
                          ],
                          disabled,
                          onSelect: value => controller.setUpscale({ preset: value }),
                        }}
                      />
                      <Button
                        size="small"
                        disabled={
                          disabled ||
                          !(selectedResult || selectedDraft) ||
                          !controller.validUpscale((selectedResult || selectedDraft)!.session)
                        }
                        onClick={() => void controller.run('upscale')}
                      >
                        Upscale selected image only
                      </Button>
                    </AccordionPanel>
                  </AccordionItem>
                </Accordion>
              )}
              <Accordion collapsible className="li-generation-recipes li-generation-disclosures">
                <AccordionItem value="recipes">
                  <AccordionHeader>Refinement recipes</AccordionHeader>
                  <AccordionPanel>
                    <div className="li-generation-recipe-row">
                      <Field label="Saved recipe">
                        <GenerationChoiceMenu
                          id="refinement-recipe"
                          label="Saved refinement recipe"
                          value={recipe}
                          disabled={disabled}
                          choices={[
                            { value: '', label: 'Choose a recipe' },
                            ...state.recipes.map(item => ({ value: item.name, label: item.name })),
                          ]}
                          onSelect={setRecipe}
                        />
                      </Field>
                      <Button
                        size="small"
                        disabled={disabled || !recipe}
                        onClick={() => void controller.loadRecipe(recipe)}
                      >
                        Load
                      </Button>
                      <Button
                        size="small"
                        appearance="subtle"
                        disabled={disabled || !recipe}
                        onClick={() => {
                          controller.deleteRecipe(recipe);
                          setRecipe('');
                        }}
                      >
                        Delete
                      </Button>
                    </div>
                    <div className="li-generation-recipe-row">
                      <Field label="Recipe name">
                        <Input
                          size="small"
                          maxLength={80}
                          value={recipeName}
                          disabled={disabled}
                          onChange={(_, data) => setRecipeName(data.value)}
                        />
                      </Field>
                      <Button
                        size="small"
                        disabled={disabled || !recipeName.trim()}
                        onClick={() => controller.saveRecipe(recipeName)}
                      >
                        Save
                      </Button>
                    </div>
                    {state.recipeWarnings.map(warning => (
                      <p className="li-generation-warning" key={warning}>
                        {warning}
                      </p>
                    ))}
                  </AccordionPanel>
                </AccordionItem>
              </Accordion>
            </aside>
          </div>
        </div>
      )}
      {state.context.backgroundTarget &&
        controller.backgroundResult() &&
        state.context.backgroundTarget.id !== controller.backgroundResult()!.id && (
          <div className="li-generation-result-actions">
            <Button size="small" disabled={disabled} onClick={() => void controller.applyGeneratedBackground()}>
              Use as background
            </Button>
            <span className="li-generation-note" title={state.context.backgroundTarget.name}>
              For {state.context.backgroundTarget.name}
            </span>
          </div>
        )}
      <footer className="li-generation-status">
        <div className="li-generation-runtime" aria-label="GPU and generation controls">
          <Tooltip content={hardware.description} relationship="description">
            <span className="li-generation-gpu" aria-label={hardware.description}>
              {hardware.label}
            </span>
          </Tooltip>
          <div className="li-generation-runtime-actions">
            <span className="li-generation-stop-slot">
              {state.working && state.runningKey && (
                <Tooltip content={stopHint} relationship="description">
                  <Button
                    size="small"
                    aria-label="Stop generation"
                    disabled={!controller.canStop()}
                    icon={<Icon name="stop" />}
                    onClick={() => void controller.stopGeneration()}
                  >
                    Stop
                  </Button>
                </Tooltip>
              )}
            </span>
            <Tooltip content={ejectHint} relationship="description">
              <Button
                size="small"
                appearance="subtle"
                className="li-generation-icon-button"
                aria-label="Unload GPU models"
                disabled={!controller.canEjectModels()}
                icon={<Icon name="eject" />}
                onClick={() => void controller.ejectModels()}
              />
            </Tooltip>
          </div>
        </div>
        <div className="li-generation-feedback">
          {state.error && <p role="alert">{state.error}</p>}
          {state.cancelError && <p role="alert">{state.cancelError}</p>}
          <p role="status">
            {state.stopping
              ? progress?.stage === 'cancelled'
                ? 'Generation stopped. Finishing cleanup…'
                : 'Stopping generation…'
              : state.working && state.runningKey && state.watching
                ? progress?.stage_label || 'Waiting for backend progress…'
                : state.status}
            {sampler ? ` · ${sampler.value} / ${sampler.max}` : ''}
          </p>
          {sampler && <ProgressBar max={sampler.max} value={sampler.value} aria-label="Generation sampling progress" />}
          {state.working && progress?.connection_lost && (
            <p>Backend progress connection lost. The job may still be running.</p>
          )}
          {state.progressError && <p>{state.progressError}</p>}
          {state.uncertain && (
            <Button size="small" onClick={() => void controller.checkOperation()}>
              Check backend operation
            </Button>
          )}
        </div>
      </footer>
    </section>
  );
}
