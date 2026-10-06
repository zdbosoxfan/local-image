import { useEffect, useRef, useState, useSyncExternalStore } from 'react';
import {
  Button,
  Checkbox,
  Dialog,
  DialogActions,
  DialogBody,
  DialogContent,
  DialogSurface,
  DialogTitle,
  Field,
  Textarea,
} from '@fluentui/react-components';
import type { DocumentController } from '../../editor/documentController.ts';
import type { ShellController } from './shellController.ts';
import { NumberDraft } from './ToolControls.tsx';

export function CutoutProperties({ controller, shell }: { controller: DocumentController; shell: ShellController }) {
  const state = useSyncExternalStore(controller.subscribe, controller.getSnapshot),
    ui = useSyncExternalStore(shell.subscribe, shell.getSnapshot);
  const layer = state.document?.layer_stack?.find(item => item.id === state.selectedLayerId),
    cutout = layer?.cutout;
  const heading = useRef<HTMLButtonElement>(null);
  useEffect(() => {
    if (ui.cutoutProperties) heading.current?.focus();
  }, [ui.cutoutProperties]);
  if (layer?.kind !== 'cutout' || state.workspace === 'generate') return null;
  const disabled = state.busy || state.showOriginal || layer.locked || !layer.visible,
    shadow = cutout?.shadow ?? {};
  const change = (patch: Record<string, unknown>) => controller.patchCutout({ shadow: patch });
  return (
    <section className="li-cutout-properties" aria-label="Cutout properties">
      <Button
        ref={heading}
        appearance="subtle"
        size="small"
        aria-expanded={ui.cutoutProperties}
        onClick={() => shell.showCutoutProperties(!ui.cutoutProperties)}
      >
        Edge & shadow {ui.cutoutProperties ? '−' : '+'}
      </Button>
      {ui.cutoutProperties && (
        <div className="li-cutout-fields">
          <NumberDraft
            label="Feather"
            value={cutout?.feather ?? 0}
            min={0}
            max={40}
            step={0.5}
            suffix="px"
            disabled={disabled}
            commit={feather => controller.patchCutout({ feather })}
          />
          <Checkbox
            label="Shadow"
            checked={shadow.enabled === true}
            disabled={disabled}
            onChange={(_, data) => void change({ enabled: data.checked === true })}
          />
          {shadow.enabled && (
            <>
              <NumberDraft
                label="Shadow opacity"
                value={Math.round((shadow.opacity ?? 0.25) * 100)}
                min={0}
                max={100}
                suffix="%"
                disabled={disabled}
                commit={opacity => change({ opacity: opacity / 100 })}
              />
              <NumberDraft
                label="Shadow softness"
                value={shadow.blur ?? 18}
                min={0}
                max={100}
                step={0.5}
                suffix="px"
                disabled={disabled}
                commit={blur => change({ blur })}
              />
              <NumberDraft
                label="Shadow X"
                value={shadow.offset_x ?? 12}
                min={-1000}
                max={1000}
                suffix="px"
                disabled={disabled}
                commit={offset_x => change({ offset_x })}
              />
              <NumberDraft
                label="Shadow Y"
                value={shadow.offset_y ?? 20}
                min={-1000}
                max={1000}
                suffix="px"
                disabled={disabled}
                commit={offset_y => change({ offset_y })}
              />
              <NumberDraft
                label="Shadow height"
                value={Math.round((shadow.squeeze ?? 1) * 100)}
                min={10}
                max={100}
                suffix="%"
                disabled={disabled}
                commit={squeeze => change({ squeeze: squeeze / 100 })}
              />
            </>
          )}
        </div>
      )}
    </section>
  );
}

export function BackgroundGenerator({ controller, shell }: { controller: DocumentController; shell: ShellController }) {
  const state = useSyncExternalStore(controller.subscribe, controller.getSnapshot),
    ui = useSyncExternalStore(shell.subscribe, shell.getSnapshot);
  const [prompt, setPrompt] = useState(''),
    [running, setRunning] = useState(false);
  const variant = state.health.qwen.variants.find(item => item.id === state.tools.qwenVariant);
  const ready = state.health.qwen.connected && variant?.available;
  async function generate() {
    if (running || state.busy || !ready || !prompt.trim()) return;
    setRunning(true);
    try {
      const result = await controller.generateBackground(prompt);
      if (result) shell.showBackgroundGenerator(false);
    } finally {
      setRunning(false);
    }
  }
  return (
    <Dialog
      open={ui.backgroundGenerator}
      onOpenChange={(_, data) => {
        if (!data.open && !running) shell.showBackgroundGenerator(false);
      }}
    >
      <DialogSurface className="li-editor-dialog" data-react-owned="true">
        <DialogBody>
          <DialogTitle>Generate background</DialogTitle>
          <DialogContent>
            <Field label="Describe the empty scene">
              <Textarea
                value={prompt}
                maxLength={2000}
                resize="vertical"
                rows={4}
                disabled={running}
                onChange={(_, data) => setPrompt(data.value)}
              />
            </Field>
            <p className="li-generation-note">
              Creates a separate background layer. Empty-scene instructions exclude people, products and text.
            </p>
            {!ready && (
              <p role="status">
                {variant?.reason || state.health.qwen.reason || 'Set up Qwen Image 2.1 to generate a background.'}
              </p>
            )}
            {state.statusError && <p role="alert">{state.status}</p>}
          </DialogContent>
          <DialogActions>
            <Button disabled={running} onClick={() => shell.showBackgroundGenerator(false)}>
              Cancel
            </Button>
            <Button
              appearance="primary"
              disabled={running || state.busy || !ready || !prompt.trim()}
              onClick={() => void generate()}
            >
              {running ? 'Generating…' : 'Generate background'}
            </Button>
          </DialogActions>
        </DialogBody>
      </DialogSurface>
    </Dialog>
  );
}
