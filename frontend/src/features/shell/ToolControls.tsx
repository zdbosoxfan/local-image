import { useEffect, useState } from 'react';
import {
  Button,
  Field,
  Input,
  Menu,
  MenuItem,
  MenuList,
  MenuPopover,
  MenuTrigger,
  Slider,
  Tab,
  TabList,
  Toolbar,
  ToolbarRadioButton,
  ToolbarToggleButton,
} from '@fluentui/react-components';
import type { Transform, Workspace } from '../../contracts.ts';
import { Hint } from './Hint.tsx';
import { Icon, type IconName } from './Icon.tsx';
import './shell.css';
import { ChoiceSelect } from './ChoiceSelect.tsx';

export type Tool = 'heal' | 'brush' | 'pen' | 'rectangle' | 'ellipse' | 'move' | 'hand';
export interface ToolSnapshot {
  workspace: Workspace;
  tool: Exclude<Tool, 'heal' | 'hand'>;
  handActive: boolean;
  operation: 'heal' | 'ai';
  busy: boolean;
  hasDocument: boolean;
  showOriginal: boolean;
  canEdit: boolean;
  brushSize: number;
  subtract: boolean;
  canFinish: boolean;
  selectionActive: boolean;
  canApply: boolean;
  applyLabel: string;
  healMethod: string;
  healMethods: readonly { id: string; label: string; available?: boolean }[];
  aiProvider: 'klein' | 'qwen';
  qwenVariant: string;
  qwenVariants: readonly { id: string; label: string; available?: boolean }[];
  maskReady: boolean;
  canRemoveBackground: boolean;
  cutoutOperation: 'erase' | 'restore';
  transform: Transform | null;
  canTransform: boolean;
}
export interface ToolActions {
  workspace(value: Workspace): void;
  tool(value: Tool): void;
  brushSize(value: number): void;
  selectionMode(subtract: boolean): void;
  finishPath(): void;
  clearSelection(): void;
  applySelection(): unknown;
  healMethod(value: string): void;
  aiProvider(value: 'klein' | 'qwen'): void;
  qwenVariant(value: string): void;
  cutoutOperation(value: 'erase' | 'restore'): void;
  removeBackground(): unknown;
  importBackground(): unknown;
  browseBackgrounds(): unknown;
  generateBackground(): unknown;
  edgeOptions(): void;
  transform(value: Partial<Transform>): unknown;
}
const tools: Array<{ id: Tool; label: string; symbol: IconName }> = [
  { id: 'move', label: 'Move layer (V)', symbol: 'move' },
  { id: 'heal', label: 'Quick Heal brush (J)', symbol: 'heal' },
  { id: 'brush', label: 'Brush selection (B)', symbol: 'brush' },
  { id: 'pen', label: 'Pen selection (P)', symbol: 'pen' },
  { id: 'rectangle', label: 'Rectangle selection (R)', symbol: 'rectangle' },
  { id: 'ellipse', label: 'Ellipse selection (E)', symbol: 'ellipse' },
  { id: 'hand', label: 'Hand tool (H)', symbol: 'hand' },
];
export function WorkspaceTabs({ state, actions }: { state: ToolSnapshot; actions: ToolActions }) {
  return (
    <TabList
      className="li-workspaces"
      size="small"
      selectedValue={state.workspace}
      aria-label="Editing workspace"
      onTabSelect={(_, data) => actions.workspace(data.value as Workspace)}
    >
      <Tab id="workspace-retouch" value="retouch" disabled={state.busy}>
        Retouch
      </Tab>
      <Tab id="workspace-cutout" value="cutout" disabled={state.busy}>
        Cutout
      </Tab>
      <Tab id="workspace-generate" value="generate" disabled={state.busy}>
        Generate
      </Tab>
    </TabList>
  );
}
export function ToolRail({ state, actions }: { state: ToolSnapshot; actions: ToolActions }) {
  if (state.workspace === 'generate') return null;
  const isActive = (id: Tool) =>
    id === 'hand'
      ? state.handActive
      : !state.handActive &&
        (id === 'heal'
          ? state.operation === 'heal' && state.tool === 'brush'
          : id === 'brush'
            ? state.tool === 'brush' && (state.workspace === 'cutout' || state.operation === 'ai')
            : state.tool === id);
  const visible = tools.filter(
    tool =>
      tool.id === 'move' ||
      tool.id === 'hand' ||
      state.workspace === 'retouch' ||
      (state.maskReady && tool.id !== 'heal'),
  );
  const activeTool = visible.find(tool => isActive(tool.id))?.id;
  return (
    <Toolbar
      className="li-tool-rail"
      vertical
      size="small"
      aria-label="Editing tools"
      checkedValues={{ tools: activeTool ? [activeTool] : [] }}
    >
      {visible.map(tool => (
        <Hint key={tool.id} content={tool.label}>
          <ToolbarToggleButton
            name="tools"
            value={tool.id}
            appearance="subtle"
            aria-label={tool.label}
            disabled={tool.id === 'hand' ? !state.hasDocument : !state.canEdit}
            onClick={() => actions.tool(tool.id)}
            icon={<Icon name={tool.symbol} />}
          />
        </Hint>
      ))}
    </Toolbar>
  );
}
export function NumberDraft({
  label,
  displayLabel,
  value,
  min,
  max,
  step = 1,
  disabled,
  commit,
  suffix,
  hideLabel = false,
}: {
  label: string;
  displayLabel?: string;
  value: number;
  min: number;
  max: number;
  step?: number;
  disabled: boolean;
  commit(value: number): unknown;
  suffix?: string;
  hideLabel?: boolean;
}) {
  const [draft, setDraft] = useState(String(value));
  useEffect(() => {
    setDraft(String(value));
  }, [value, disabled]);
  const finish = () => {
    const next = Number(draft);
    if (draft.trim() && Number.isFinite(next) && next >= min && next <= max) {
      if (next !== value) commit(next);
    } else setDraft(String(value));
  };
  const input = (
    <Input
      size="small"
      aria-label={label}
      type="number"
      min={min}
      max={max}
      step={step}
      disabled={disabled}
      value={draft}
      contentAfter={suffix}
      onChange={(_, data) => setDraft(data.value)}
      onBlur={finish}
      onKeyDown={event => {
        event.stopPropagation();
        if (event.key === 'Enter') {
          event.preventDefault();
          (event.target as HTMLInputElement).blur();
        }
        if (event.key === 'Escape') {
          event.preventDefault();
          setDraft(String(value));
        }
      }}
    />
  );
  return hideLabel ? (
    input
  ) : (
    <Field label={displayLabel ?? label} orientation="horizontal">
      {input}
    </Field>
  );
}
export function ToolOptions({
  state,
  actions,
  menuOpen,
  setMenuOpen,
  unavailableReason,
  openSetup,
}: {
  state: ToolSnapshot;
  actions: ToolActions;
  menuOpen: boolean;
  setMenuOpen(value: boolean): void;
  unavailableReason?: string;
  openSetup(): void;
}) {
  if (state.workspace === 'generate') return null;
  const selecting = !state.handActive && state.tool !== 'move' && (state.workspace === 'retouch' || state.maskReady);
  return (
    <section className="li-tool-options" aria-label="Tool options">
      {state.workspace === 'cutout' && !state.maskReady && (
        <>
          <ChoiceSelect
            label="Cutout model precision"
            disabled={state.busy}
            value={state.qwenVariant}
            choices={state.qwenVariants.map(variant => ({
              value: variant.id,
              label: variant.label + (variant.available === false ? ' · not installed' : ''),
            }))}
            onSelect={actions.qwenVariant}
          />
          <Button size="small" disabled={!state.canRemoveBackground} onClick={() => actions.removeBackground()}>
            Remove background
          </Button>
        </>
      )}
      {state.workspace === 'cutout' && (
        <>
          <Menu open={menuOpen} onOpenChange={(_, data) => setMenuOpen(data.open)}>
            <MenuTrigger disableButtonEnhancement>
              <Button size="small" disabled={state.busy || !state.hasDocument}>
                Add background
              </Button>
            </MenuTrigger>
            <MenuPopover data-react-owned="true">
              <MenuList>
                <MenuItem onClick={() => actions.importBackground()}>Import image…</MenuItem>
                <MenuItem onClick={() => actions.browseBackgrounds()}>Choose from Assets</MenuItem>
                <MenuItem onClick={() => actions.generateBackground()}>Generate background…</MenuItem>
              </MenuList>
            </MenuPopover>
          </Menu>
          {state.maskReady && (
            <Button size="small" disabled={!state.canEdit} onClick={actions.edgeOptions}>
              Edge & shadow
            </Button>
          )}
        </>
      )}
      {state.tool === 'move' && !state.handActive && state.transform && (
        <div className="li-transform-fields">
          <NumberDraft
            label="Layer X"
            displayLabel="X"
            value={state.transform.offset_x}
            min={-100000}
            max={100000}
            disabled={!state.canTransform}
            commit={offset_x => actions.transform({ offset_x })}
          />
          <NumberDraft
            label="Layer Y"
            displayLabel="Y"
            value={state.transform.offset_y}
            min={-100000}
            max={100000}
            disabled={!state.canTransform}
            commit={offset_y => actions.transform({ offset_y })}
          />
          <NumberDraft
            label="Layer scale"
            displayLabel="Scale"
            value={state.transform.scale * 100}
            min={5}
            max={400}
            step={0.1}
            suffix="%"
            disabled={!state.canTransform}
            commit={scale => actions.transform({ scale: scale / 100 })}
          />
          <NumberDraft
            label="Layer angle"
            displayLabel="Angle"
            value={state.transform.rotation}
            min={-180}
            max={180}
            step={0.1}
            suffix="°"
            disabled={!state.canTransform}
            commit={rotation => actions.transform({ rotation })}
          />
        </div>
      )}
      {selecting && (
        <>
          {state.tool === 'brush' && (
            <div className="li-brush-size">
              <Field label="Size" orientation="horizontal">
                <Slider
                  aria-label="Brush size"
                  min={1}
                  max={2000}
                  value={state.brushSize}
                  disabled={!state.canEdit}
                  onChange={(_, data) => actions.brushSize(data.value)}
                />
              </Field>
              <NumberDraft
                label="Brush diameter"
                hideLabel
                value={state.brushSize}
                min={1}
                max={2000}
                suffix="px"
                disabled={!state.canEdit}
                commit={actions.brushSize}
              />
            </div>
          )}
          <Toolbar
            size="small"
            aria-label="Selection mode"
            checkedValues={{ selectionMode: [state.subtract ? 'subtract' : 'add'] }}
            onCheckedValueChange={(_, data) => actions.selectionMode(data.checkedItems[0] === 'subtract')}
          >
            <ToolbarRadioButton
              name="selectionMode"
              value="add"
              appearance="subtle"
              disabled={!state.canEdit}
              icon={<Icon name="select-add" />}
            >
              Add
            </ToolbarRadioButton>
            <ToolbarRadioButton
              name="selectionMode"
              value="subtract"
              appearance="subtle"
              disabled={!state.canEdit}
              icon={<Icon name="select-subtract" />}
            >
              Subtract
            </ToolbarRadioButton>
          </Toolbar>
          {state.tool === 'pen' && (
            <Button size="small" disabled={!state.canFinish} onClick={actions.finishPath}>
              Close path
            </Button>
          )}
          {state.workspace === 'retouch' &&
            (state.operation === 'heal' ? (
              <ChoiceSelect
                label="Quick Heal method"
                value={state.healMethod}
                disabled={!state.canEdit}
                choices={state.healMethods.map(method => ({
                  value: method.id,
                  label: method.label,
                  disabled: method.available === false,
                }))}
                onSelect={actions.healMethod}
              />
            ) : (
              <>
                <ChoiceSelect
                  label="AI removal provider"
                  value={state.aiProvider}
                  disabled={state.busy}
                  choices={[
                    { value: 'klein', label: 'FLUX.2 Klein' },
                    { value: 'qwen', label: 'Qwen Image 2.1' },
                  ]}
                  onSelect={value => actions.aiProvider(value as 'klein' | 'qwen')}
                />
                {state.aiProvider === 'qwen' && (
                  <ChoiceSelect
                    label="Removal precision"
                    value={state.qwenVariant}
                    disabled={state.busy}
                    choices={state.qwenVariants.map(variant => ({ value: variant.id, label: variant.label }))}
                    onSelect={actions.qwenVariant}
                  />
                )}
              </>
            ))}
          {state.workspace === 'cutout' && (
            <Toolbar
              size="small"
              aria-label="Mask operation"
              checkedValues={{ maskOperation: [state.cutoutOperation] }}
              onCheckedValueChange={(_, data) => actions.cutoutOperation(data.checkedItems[0] as 'erase' | 'restore')}
            >
              <ToolbarRadioButton
                name="maskOperation"
                value="erase"
                appearance="subtle"
                disabled={!state.canEdit}
                icon={<Icon name="erase" />}
              >
                Erase
              </ToolbarRadioButton>
              <ToolbarRadioButton
                name="maskOperation"
                value="restore"
                appearance="subtle"
                disabled={!state.canEdit}
                icon={<Icon name="restore" />}
              >
                Restore
              </ToolbarRadioButton>
            </Toolbar>
          )}
          <Button size="small" appearance="primary" disabled={!state.canApply} onClick={() => actions.applySelection()}>
            {state.applyLabel}
          </Button>
        </>
      )}
      {unavailableReason && state.hasDocument && (
        <div className="li-tool-readiness">
          <span role="status" title={unavailableReason}>
            {unavailableReason}
          </span>
          <Button size="small" disabled={state.busy} onClick={openSetup}>
            Set up AI
          </Button>
        </div>
      )}
    </section>
  );
}
