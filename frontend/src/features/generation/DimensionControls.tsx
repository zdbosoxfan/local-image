import { useState } from 'react';
import { Button, Field, Input, Menu, MenuItemRadio, MenuList, MenuPopover, MenuTrigger, Popover, PopoverSurface, PopoverTrigger, Tooltip } from '@fluentui/react-components';
import { Icon } from '../shell/Icon.tsx';

export interface DimensionChoice { value: string; label: string }

function DimensionIcon({ linked }: { linked?: boolean }) {
  return <svg className="li-dimension-icon" viewBox="0 0 24 24" aria-hidden="true" focusable="false">
    {linked === undefined ? <><rect x="3" y="5" width="18" height="14" rx="2" /><path d="M7 11V9h3m4 6h3v-2" /></> : linked
      ? <><path d="m9 15 6-6M8 13l-1 1a4 4 0 0 0 6 6l3-3a4 4 0 0 0 0-6M16 11l1-1a4 4 0 0 0-6-6L8 7a4 4 0 0 0 0 6" /></>
      : <><path d="m7 14-1 1a4 4 0 0 0 6 6l3-3m2-8 1-1a4 4 0 0 0-6-6L9 6M4 4l3 3m10 10 3 3M3 11h3m12 2h3" /></>}
  </svg>;
}

export function DimensionPresetMenu({ id, label, value, choices, disabled, onSelect }: {
  id: string; label: string; value: string; choices: readonly DimensionChoice[]; disabled?: boolean; onSelect(value: string): void;
}) {
  const selected = choices.find(choice => choice.value === value)?.label ?? value;
  return <Menu>
    <MenuTrigger disableButtonEnhancement><Tooltip content={`${label}: ${selected}`} relationship="description"><Button size="small" appearance="subtle" className="li-dimension-button" aria-label={`${label}: ${selected}`} disabled={disabled} icon={<DimensionIcon />} /></Tooltip></MenuTrigger>
    <MenuPopover><MenuList aria-label={label} checkedValues={{ [id]: [value] }}>
      {choices.map(choice => <MenuItemRadio key={choice.value} name={id} value={choice.value} disabled={disabled} onClick={() => onSelect(choice.value)}>{choice.label}</MenuItemRadio>)}
    </MenuList></MenuPopover>
  </Menu>;
}

function SizeMemoryInfo({ label }: { label: string }) {
  const [open, setOpen] = useState(false), accessibleLabel = `${label} size and memory information`;
  return <Popover open={open} onOpenChange={(_, data) => setOpen(data.open)} positioning={{ position: 'below', align: 'end', autoSize: 'height', overflowBoundaryPadding: 16 }} withArrow trapFocus>
    <PopoverTrigger disableButtonEnhancement><Tooltip content="Size & memory" relationship="description"><Button size="small" appearance="subtle" className="li-dimension-button" aria-label={accessibleLabel} icon={<Icon name="info"/>} /></Tooltip></PopoverTrigger>
    <PopoverSurface className="li-dimension-memory" role="dialog" aria-label={accessibleLabel}>
      <div className="li-dimension-memory-header"><strong>Size &amp; memory</strong><Button size="small" appearance="subtle" className="li-generation-icon-button" aria-label="Close size and memory information" icon={<Icon name="close"/>} onClick={() => setOpen(false)}/></div>
      <p>If generation runs out of memory, reduce width or height.</p>
      <p>Sizes adjust for the selected model.</p>
    </PopoverSurface>
  </Popover>;
}

export function DimensionControls({ label, width, height, bounds, linked, disabled, fixedLink, memoryInfo, onLink, onChange, onCommit, preset }: {
  label: string; width: number; height: number; linked: boolean; disabled?: boolean; fixedLink?: string;
  memoryInfo?: boolean;
  bounds: { minWidth: number; maxWidth: number; minHeight: number; maxHeight: number; widthStep: number; heightStep: number };
  onLink?(value: boolean): void; onChange(axis: 'width' | 'height', value: number): void; onCommit(axis: 'width' | 'height'): void;
  preset: { id: string; label: string; value: string; choices: readonly DimensionChoice[]; disabled?: boolean; onSelect(value: string): void };
}) {
  const linkLabel = `${label} link dimensions`, linkHint = fixedLink ?? (linked ? 'Unlink width and height' : 'Link width and height to preserve proportions');
  return <div className={`li-dimension-controls${memoryInfo ? ' li-dimension-with-info' : ''}`} role="group" aria-label={`${label} dimensions`}>
    <Field label="Width"><Input size="small" aria-label={`${label} width`} type="number" min={bounds.minWidth} max={Number.isFinite(bounds.maxWidth) ? bounds.maxWidth : undefined} step={bounds.widthStep} value={String(width)} disabled={disabled} onChange={(_, data) => onChange('width', Number(data.value))} onBlur={() => onCommit('width')} /></Field>
    <Tooltip content={linkHint} relationship="description"><Button size="small" appearance="subtle" className="li-dimension-button li-dimension-chain" aria-label={linkLabel} aria-pressed={linked} disabled={disabled || !onLink} icon={<DimensionIcon linked={linked} />} onClick={() => onLink?.(!linked)} /></Tooltip>
    <Field label="Height"><Input size="small" aria-label={`${label} height`} type="number" min={bounds.minHeight} max={Number.isFinite(bounds.maxHeight) ? bounds.maxHeight : undefined} step={bounds.heightStep} value={String(height)} disabled={disabled} onChange={(_, data) => onChange('height', Number(data.value))} onBlur={() => onCommit('height')} /></Field>
    <DimensionPresetMenu {...preset} />
    {memoryInfo && <SizeMemoryInfo label={label} />}
  </div>;
}
