import { useEffect, useRef, useState, useSyncExternalStore } from 'react';
import {
  Accordion,
  AccordionHeader,
  AccordionItem,
  AccordionPanel,
  Button,
  Checkbox,
  Field,
  Input,
  ProgressBar,
  Select,
  Tab,
  TabList,
  Textarea,
} from '@fluentui/react-components';
import { Comparison } from './Comparison.tsx';
import type { GenerationController } from './controller.ts';
import type { DraftKey, GenerationMode } from './types.ts';
import { Icon } from '../shell/Icon.tsx';
import './generation.css';

function Stage({ controller, stage }: { controller: GenerationController; stage: DraftKey }) {
  const state = useSyncExternalStore(controller.subscribe, controller.getSnapshot),
    draft = state.drafts[stage],
    model = controller.modelFor(stage),
    cap = model?.capabilities;
  const refs = controller.referencesFor(stage),
    errors = controller.errorsFor(stage),
    disabled = state.working || state.context.busy || state.loading,
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
  return (
    <section className="li-generation-stage" aria-label={`${title} settings`}>
      <header>
        <h3>{title}</h3>
        <Button size="small" appearance="subtle" disabled={disabled} onClick={() => controller.openModels(stage)}>
          Models
        </Button>
      </header>
      <Field label="Model">
        <Select
          size="small"
          value={draft.modelId}
          disabled={disabled}
          onChange={(_, data) => controller.chooseModel(stage, data.value)}
        >
          {!candidates.some(item => item.id === draft.modelId) && draft.modelId && (
            <option value={draft.modelId}>Unavailable in this mode · {draft.modelId}</option>
          )}
          {candidates.map(item => (
            <option key={item.id} value={item.id}>
              {item.label}
              {item.available ? '' : ' · unavailable'}
            </option>
          ))}
        </Select>
      </Field>
      <Field label="Precision">
        <Select
          size="small"
          value={draft.variant}
          disabled={disabled}
          onChange={(_, data) => controller.setDraft(stage, { variant: data.value })}
        >
          {!model?.variants.some(item => item.id === draft.variant) && draft.variant && (
            <option value={draft.variant}>Unavailable · {draft.variant}</option>
          )}
          {model?.variants.map(item => (
            <option key={item.id} value={item.id}>
              {item.label || item.id}
              {item.available ? '' : ' · unavailable'}
            </option>
          ))}
        </Select>
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
      <div className="li-generation-size">
        <Field label="Width">
          <Input
            size="small"
            type="number"
            min={bounds.minWidth}
            max={Number.isFinite(bounds.maxWidth) ? bounds.maxWidth : undefined}
            step={bounds.widthStep}
            value={String(draft.width)}
            disabled={disabled}
            onChange={(_, data) => controller.setDraft(stage, { width: Number(data.value), aspect: 'custom' })}
            onBlur={() => controller.commitSize(stage, 'width')}
          />
        </Field>
        <Field label="Height">
          <Input
            size="small"
            type="number"
            min={bounds.minHeight}
            max={Number.isFinite(bounds.maxHeight) ? bounds.maxHeight : undefined}
            step={bounds.heightStep}
            value={String(draft.height)}
            disabled={disabled}
            onChange={(_, data) => controller.setDraft(stage, { height: Number(data.value), aspect: 'custom' })}
            onBlur={() => controller.commitSize(stage, 'height')}
          />
        </Field>
      </div>
      <div className="li-generation-size-controls">
        <Checkbox
          label="Link dimensions"
          checked={draft.locked}
          disabled={disabled}
          onChange={(_, data) =>
            controller.setDraft(stage, { locked: data.checked === true, ratio: draft.width / draft.height })
          }
        />
        <Select
          size="small"
          aria-label={`${title} aspect ratio`}
          value={draft.aspect}
          disabled={disabled}
          onChange={(_, data) => controller.setAspect(stage, data.value)}
        >
          {['1:1', '3:2', '2:3', '16:9', 'custom'].map(value => (
            <option key={value}>{value}</option>
          ))}
        </Select>
      </div>
      {!!model?.limits.resolution_note && <p className="li-generation-note">{model.limits.resolution_note}</p>}
      {!!cap?.transparent && (
        <Checkbox
          label="Transparent PNG"
          checked={draft.transparent}
          disabled={disabled}
          onChange={(_, data) => controller.setDraft(stage, { transparent: data.checked === true })}
        />
      )}
      <Accordion collapsible>
        <AccordionItem value="section">
          <AccordionHeader size="small">Sampling and seed</AccordionHeader>
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
            <Field label="Seed" hint="Blank chooses a new seed; zero is a valid fixed seed.">
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
      </Accordion>
      <Accordion collapsible>
        <AccordionItem value="section">
          <AccordionHeader size="small">
            Image inputs · {refs.length}/{cap?.max_references ?? 0}
          </AccordionHeader>
          <AccordionPanel>
            {stage !== 'final' && (
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
                <Button size="small" disabled={disabled} onClick={() => controller.openAssets('reference')}>
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
                <span>
                  {index + 1}. {item.name}
                </span>
                {stage !== 'final' && (
                  <>
                    <Button
                      size="small"
                      appearance="subtle"
                      aria-label={`Move ${item.name} earlier`}
                      disabled={disabled || index <= (stage === 'edit' ? 1 : 0)}
                      onClick={() => controller.moveReference(stage, item.id, -1)}
                      icon={<Icon name="move-up" />}
                    />
                    <Button
                      size="small"
                      appearance="subtle"
                      aria-label={`Remove ${item.name}`}
                      disabled={disabled || (stage === 'edit' && index === 0)}
                      onClick={() => controller.removeReference(stage, item.id)}
                      icon={<Icon name="close" />}
                    />
                  </>
                )}
              </div>
            ))}
            {draft.missingReferenceCount > refs.length && (
              <p className="li-generation-warning">
                The saved image used {draft.missingReferenceCount} references; reattach the original images to reproduce
                it.
              </p>
            )}
          </AccordionPanel>
        </AccordionItem>
      </Accordion>
      <div className="li-generation-actions">
        <Button
          size="small"
          disabled={disabled || !model || cap?.lora === false || cap?.loras === false}
          onClick={() => controller.openLoras(stage)}
        >
          Styles / LoRAs · {draft.loras.length}
        </Button>
        <span className="li-generation-note">{draft.loras.map(item => item.title || item.id).join(', ')}</span>
      </div>
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
    disabled = state.working || state.context.busy;
  const [recipeName, setRecipeName] = useState(''),
    [recipe, setRecipe] = useState('');
  const panel = useRef<HTMLElement>(null);
  useEffect(() => {
    if (panel.current) panel.current.scrollTop = 0;
  }, [state.mode]);
  const progress = state.progress,
    sampler =
      state.working && state.watching && progress?.active && progress.stage === 'sampling' ? progress.progress : null;
  const selectedDraft = controller.selectedDraft(),
    selectedResult = controller.selectedResult();
  return (
    <section ref={panel} className="li-generation" data-react-owned="true" aria-label="Image generation">
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
        <Button
          size="small"
          appearance="subtle"
          disabled={disabled || state.loading}
          onClick={() => void controller.refreshModels(true)}
        >
          Refresh models
        </Button>
      </header>
      {state.mode !== 'refine' ? (
        <Stage controller={controller} stage={state.mode} />
      ) : (
        <div className="li-refinement">
          <Accordion collapsible className="li-generation-recipes">
            <AccordionItem value="section">
              <AccordionHeader size="small">Refinement recipes</AccordionHeader>
              <AccordionPanel>
                <div className="li-generation-actions">
                  <Select
                    size="small"
                    aria-label="Saved refinement recipe"
                    value={recipe}
                    disabled={disabled}
                    onChange={(_, data) => setRecipe(data.value)}
                  >
                    <option value="">Choose a recipe</option>
                    {state.recipes.map(item => (
                      <option key={item.name}>{item.name}</option>
                    ))}
                  </Select>
                  <Button
                    size="small"
                    disabled={disabled || !recipe}
                    onClick={() => void controller.loadRecipe(recipe)}
                  >
                    Load
                  </Button>
                  <Button
                    size="small"
                    disabled={disabled || !recipe}
                    onClick={() => {
                      controller.deleteRecipe(recipe);
                      setRecipe('');
                    }}
                  >
                    Delete recipe
                  </Button>
                </div>
                <div className="li-generation-actions">
                  <Input
                    size="small"
                    aria-label="Recipe name"
                    maxLength={80}
                    value={recipeName}
                    disabled={disabled}
                    onChange={(_, data) => setRecipeName(data.value)}
                  />
                  <Button
                    size="small"
                    disabled={disabled || !recipeName.trim()}
                    onClick={() => controller.saveRecipe(recipeName)}
                  >
                    Save recipe
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
          <div className="li-generation-actions">
            <Button size="small" disabled={disabled || !state.context.document} onClick={controller.useCurrentDraft}>
              Use current as draft
            </Button>
            <Button size="small" disabled={disabled} onClick={() => controller.openAssets('draft')}>
              Choose library draft
            </Button>
          </div>
          <Comparison draft={selectedDraft?.session} result={selectedResult?.session} />
          <div className="li-refinement-strips">
            <div aria-label="Generated drafts">
              {state.draftImages.map(item => (
                <Button
                  key={item.session.id}
                  appearance="subtle"
                  size="small"
                  aria-pressed={item.session.id === state.selectedDraftId}
                  disabled={disabled}
                  onClick={() => controller.selectDraft(item.session.id)}
                >
                  <img
                    src={`/api/local-remove/session/${encodeURIComponent(item.session.id)}/preview?revision=${item.session.revision}`}
                    alt={item.session.name}
                    loading="lazy"
                  />
                </Button>
              ))}
            </div>
            <div aria-label="Refined images">
              {state.resultImages
                .filter(item => item.draftId === state.selectedDraftId)
                .map(item => (
                  <Button
                    key={item.session.id}
                    appearance="subtle"
                    size="small"
                    aria-pressed={item.session.id === state.selectedResultId}
                    disabled={disabled}
                    onClick={() => controller.selectResult(item.session.id)}
                  >
                    <img
                      src={`/api/local-remove/session/${encodeURIComponent(item.session.id)}/preview?revision=${item.session.revision}`}
                      alt={item.session.name}
                      loading="lazy"
                    />
                  </Button>
                ))}
            </div>
          </div>
          <div className="li-generation-actions">
            <Button
              size="small"
              disabled={disabled || !selectedDraft}
              onClick={() => void controller.openSelected(false)}
            >
              Open draft in editor
            </Button>
            <Button
              size="small"
              disabled={disabled || !selectedResult}
              onClick={() => void controller.openSelected(true)}
            >
              Open result in editor
            </Button>
          </div>
          <div className="li-refinement-settings">
            <Stage controller={controller} stage="draft" />
            <div>
              <Checkbox
                label="Include the draft's original references"
                checked={state.includeReferences}
                disabled={disabled || !selectedDraft?.references.length}
                onChange={(_, data) => controller.setIncludeReferences(data.checked === true)}
              />
              <Stage controller={controller} stage="final" />
            </div>
          </div>
          <Accordion collapsible className="li-generation-upscale">
            <AccordionItem value="section">
              <AccordionHeader size="small">Upscale with SeedVR2</AccordionHeader>
              <AccordionPanel>
                <p className="li-generation-note">
                  {state.upscaleInventory?.model.available
                    ? 'Enlarges the image and preserves alpha. Fine detail can change; review faces and lettering.'
                    : state.upscaleInventory?.model.reason || 'Upscale availability has not been checked.'}
                </p>
                <Checkbox
                  label="Upscale after refinement"
                  disabled={disabled || !state.upscaleInventory?.model.available}
                  checked={state.upscale.enabled}
                  onChange={(_, data) => controller.setUpscale({ enabled: data.checked === true })}
                />
                <Select
                  size="small"
                  aria-label="Upscale long edge"
                  disabled={disabled}
                  value={state.upscale.preset}
                  onChange={(_, data) => controller.setUpscale({ preset: data.value })}
                >
                  <option value="3840">4K long edge</option>
                  <option value="2048">2048 px long edge</option>
                  <option value="custom">Custom</option>
                </Select>
                {state.upscale.preset === 'custom' && (
                  <div className="li-generation-size">
                    {(['width', 'height'] as const).map(axis => (
                      <Field key={axis} label={`Upscale ${axis}`}>
                        <Input
                          size="small"
                          type="number"
                          value={String(state.upscale[axis])}
                          disabled={disabled}
                          onChange={(_, data) => controller.setUpscale({ [axis]: Number(data.value) })}
                        />
                      </Field>
                    ))}
                  </div>
                )}
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
        {state.error && <p role="alert">{state.error}</p>}
        <p role="status">
          {state.working && state.runningKey && state.watching
            ? progress?.stage_label || 'Waiting for backend progress…'
            : state.status}
        </p>
        {sampler && (
          <>
            <ProgressBar max={sampler.max} value={sampler.value} />
            <p>
              Sampler {sampler.value} / {sampler.max}
            </p>
          </>
        )}
        {state.working && progress?.connection_lost && (
          <p>Backend progress connection lost. The job may still be running.</p>
        )}
        {state.progressError && <p>{state.progressError}</p>}
        {state.working && state.runningKey && (
          <Button size="small" disabled={!state.watching} onClick={controller.stopWatching}>
            Pause progress updates
          </Button>
        )}
        {state.uncertain && (
          <Button size="small" onClick={() => void controller.checkOperation()}>
            Check backend operation
          </Button>
        )}
      </footer>
    </section>
  );
}
