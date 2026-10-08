import { useEffect, useRef, useState, useSyncExternalStore } from 'react';
import {
  Button,
  CounterBadge,
  Field,
  Input,
  Menu,
  MenuItem,
  MenuList,
  MenuPopover,
  MenuTrigger,
} from '@fluentui/react-components';
import type { EditorSnapshot, EditorCommands, Layer } from '../../contracts.ts';
import { Icon } from './Icon.tsx';
import { Hint } from './Hint.tsx';
export interface LayerController {
  getSnapshot(): EditorSnapshot;
  subscribe(listener: () => void): () => void;
  commands: Pick<
    EditorCommands,
    'selectLayer' | 'patchLayer' | 'createRetouch' | 'reorderLayer' | 'addMask' | 'mergeLayers' | 'restoreLayer'
  >;
}

export function Layers({
  controller,
  renameRequest,
  menuOpen,
  setMenuOpen,
}: {
  menuOpen: boolean;
  setMenuOpen(value: boolean): void;
  controller: LayerController;
  renameRequest?: { id: string; sequence: number } | null;
}) {
  const state = useSyncExternalStore(controller.subscribe, controller.getSnapshot);
  const { document: doc, selectedLayerId } = state;
  const commands = controller.commands;
  const layers = (doc?.layer_stack ?? [])
    .filter(layer => !layer.discarded)
    .slice()
    .reverse();
  const selected = layers.find(layer => layer.id === selectedLayerId);
  const active = !!doc && !state.busy && !state.showOriginal && !state.creatingBlank;
  const editable = active && !!selected && !selected.locked && selected.visible;
  const [renaming, setRenaming] = useState<string | null>(null);
  const [name, setName] = useState('');
  const [opacity, setOpacity] = useState('100');
  const nameRef = useRef<HTMLInputElement>(null);
  const opacityRef = useRef<HTMLInputElement>(null);
  const listRef = useRef<HTMLDivElement>(null);
  const renameHandled = useRef(false);
  useEffect(() => {
    setRenaming(null);
  }, [doc?.id, selectedLayerId]);
  useEffect(() => {
    // A rejected write must also replace the committed draft with the accepted
    // value, even though the backend revision did not change on failure.
    if (!state.busy) setOpacity(String(Math.round((selected?.opacity ?? 1) * 100)));
  }, [doc?.id, doc?.revision, selectedLayerId, selected?.opacity, state.busy]);
  function beginRename(layer: Layer) {
    renameHandled.current = false;
    setName(layer.name);
    setRenaming(layer.id);
  }
  useEffect(() => {
    if (renaming) {
      nameRef.current?.focus();
      nameRef.current?.select();
    }
  }, [renaming]);
  useEffect(() => {
    if (renameRequest && active && selected?.id === renameRequest.id) beginRename(selected);
  }, [renameRequest?.sequence]);

  function finishRename(save: boolean) {
    if (renameHandled.current) return;
    renameHandled.current = true;
    const id = renaming;
    setRenaming(null);
    if (save && id && name.trim() && name.trim() !== selected?.name) commands.patchLayer(id, { name: name.trim() });
  }
  function focusLayer(index: number) {
    const layer = layers[index];
    if (!layer) return;
    commands.selectLayer(layer.id);
    listRef.current?.querySelector<HTMLElement>(`[data-layer-id="${CSS.escape(layer.id)}"]`)?.focus();
  }
  function applyOpacity() {
    const value = Number(opacity);
    if (editable && selected && opacity.trim() && Number.isFinite(value) && value >= 0 && value <= 100) {
      if (value / 100 !== selected.opacity) commands.patchLayer(selected.id, { opacity: value / 100 });
    } else setOpacity(String(Math.round((selected?.opacity ?? 1) * 100)));
  }
  return (
    <section className="li-layers" data-react-owned="true" aria-label="Layers">
      <header className="li-panel-heading">
        <h2>Layers</h2>
        <CounterBadge
          className="li-count"
          count={layers.length}
          appearance="ghost"
          color="informative"
          aria-label={`${layers.length} layers`}
        />
        <Hint content="New retouch layer">
          <Button
            size="small"
            appearance="subtle"
            aria-label="New retouch layer"
            disabled={!active}
            onClick={() => commands.createRetouch()}
            icon={<Icon name="add" />}
          />
        </Hint>
        <Menu open={menuOpen} onOpenChange={(_, data) => setMenuOpen(data.open)}>
          <MenuTrigger disableButtonEnhancement>
            <Hint content="Layer commands">
              <Button
                size="small"
                appearance="subtle"
                aria-label="Layer commands"
                disabled={!doc}
                icon={<Icon name="more" />}
              />
            </Hint>
          </MenuTrigger>
          <MenuPopover data-react-owned="true">
            <MenuList>
              <MenuItem disabled={!active} onClick={() => commands.createRetouch()}>
                New retouch layer
              </MenuItem>
              <MenuItem disabled={!active || !selected} onClick={() => selected && beginRename(selected)}>
                Rename layer
              </MenuItem>
              <MenuItem
                disabled={!active || !selected}
                onClick={() => selected && commands.patchLayer(selected.id, { locked: !selected.locked })}
              >
                {selected?.locked ? 'Unlock layer' : 'Lock layer'}
              </MenuItem>
              <MenuItem
                disabled={!editable || layers[0]?.id === selectedLayerId}
                onClick={() => commands.reorderLayer(1)}
              >
                Move layer up
              </MenuItem>
              <MenuItem
                disabled={!editable || layers.at(-1)?.id === selectedLayerId}
                onClick={() => commands.reorderLayer(-1)}
              >
                Move layer down
              </MenuItem>
              <MenuItem
                disabled={!editable}
                onClick={() =>
                  selected &&
                  commands.patchLayer(selected.id, { transform: { offset_x: 0, offset_y: 0, scale: 1, rotation: 0 } })
                }
              >
                Reset transform
              </MenuItem>
              <MenuItem
                disabled={!active || !selected?.visible || selected?.kind === 'cutout'}
                onClick={() => commands.addMask()}
              >
                Add editable mask
              </MenuItem>
              <MenuItem
                disabled={!active || layers.filter(layer => layer.visible).length < 2}
                onClick={() => commands.mergeLayers()}
              >
                Merge visible to new layer
              </MenuItem>
              <MenuItem
                disabled={!active || !doc?.layer_stack?.some(layer => layer.discarded)}
                onClick={() => commands.restoreLayer()}
              >
                Restore discarded layer
              </MenuItem>
              <MenuItem
                disabled={!active || !selected || selected.locked || selected.kind === 'original'}
                onClick={() => selected && commands.patchLayer(selected.id, { discarded: true })}
              >
                Delete layer
              </MenuItem>
            </MenuList>
          </MenuPopover>
        </Menu>
      </header>
      <div
        className="li-layer-list"
        role="listbox"
        aria-label="Document layers"
        aria-multiselectable="false"
        ref={listRef}
      >
        {!doc && <p className="li-empty">Open an image to see its layers.</p>}
        {layers.map((layer, index) => (
          <div
            key={layer.id}
            className="li-layer-row"
            data-layer-id={layer.id}
            data-visible={layer.visible}
            role="option"
            aria-selected={selectedLayerId === layer.id}
            tabIndex={selectedLayerId === layer.id ? 0 : -1}
            onClick={() => {
              if (active) commands.selectLayer(layer.id);
            }}
            onKeyDown={event => {
              if (event.target !== event.currentTarget || !active) return;
              if (['ArrowUp', 'ArrowDown', 'Home', 'End'].includes(event.key)) {
                event.preventDefault();
                event.stopPropagation();
                focusLayer(
                  event.key === 'Home'
                    ? 0
                    : event.key === 'End'
                      ? layers.length - 1
                      : Math.max(0, Math.min(layers.length - 1, index + (event.key === 'ArrowUp' ? -1 : 1))),
                );
              } else if (event.key === 'F2') {
                event.preventDefault();
                event.stopPropagation();
                beginRename(layer);
              } else if (event.key === 'Delete') {
                event.preventDefault();
                event.stopPropagation();
                if (!layer.locked && layer.kind !== 'original') commands.patchLayer(layer.id, { discarded: true });
              } else if (event.key === 'Enter' || event.key === ' ') {
                event.preventDefault();
                event.stopPropagation();
                commands.selectLayer(layer.id);
              }
            }}
          >
            <Hint content={`${layer.visible ? 'Hide' : 'Show'} ${layer.name}`}>
              <Button
                size="small"
                appearance="subtle"
                className="li-row-button"
                aria-label={`${layer.visible ? 'Hide' : 'Show'} ${layer.name}`}
                disabled={!active}
                icon={<Icon name={layer.visible ? 'eye' : 'eye-off'} />}
                onClick={event => {
                  event.stopPropagation();
                  commands.patchLayer(layer.id, { visible: !layer.visible });
                }}
              />
            </Hint>
            <img
              className="li-thumbnail"
              loading="lazy"
              alt=""
              src={`/api/local-remove/session/${encodeURIComponent(doc!.id)}/stack/layer/${encodeURIComponent(layer.id)}/display?r=${encodeURIComponent(layer.display_key || String(doc!.revision))}`}
            />
            <div
              className="li-layer-label"
              onDoubleClick={() => {
                if (active) beginRename(layer);
              }}
            >
              {renaming === layer.id ? (
                <Input
                  size="small"
                  ref={nameRef}
                  aria-label="Layer name"
                  maxLength={120}
                  value={name}
                  onChange={(_, data) => setName(data.value)}
                  onBlur={() => finishRename(true)}
                  onClick={event => event.stopPropagation()}
                  onKeyDown={event => {
                    event.stopPropagation();
                    if (event.key === 'Enter' || event.key === 'Escape') {
                      event.preventDefault();
                      finishRename(event.key === 'Enter');
                    }
                  }}
                />
              ) : (
                <span className="li-layer-name" title={layer.name}>
                  {layer.name}
                </span>
              )}
              <span className="li-layer-kind">
                {layer.kind === 'retouch'
                  ? `${layer.patch_ids?.length ?? 0} repairs`
                  : layer.kind === 'cutout'
                    ? 'Editable mask'
                    : layer.kind === 'original'
                      ? 'Original image'
                      : 'Image'}
              </span>
            </div>
            <Hint content={`${layer.locked ? 'Unlock' : 'Lock'} ${layer.name}`}>
              <Button
                size="small"
                appearance="subtle"
                className="li-row-button"
                aria-label={`${layer.locked ? 'Unlock' : 'Lock'} ${layer.name}`}
                aria-pressed={layer.locked}
                disabled={!active}
                onClick={event => {
                  event.stopPropagation();
                  commands.patchLayer(layer.id, { locked: !layer.locked });
                }}
                icon={<Icon name={layer.locked ? 'lock' : 'unlock'} />}
              />
            </Hint>
          </div>
        ))}
      </div>
      <div className="li-properties">
        <Field label="Opacity" orientation="horizontal">
          <Input
            ref={opacityRef}
            size="small"
            type="number"
            aria-label="Layer opacity percent"
            min={0}
            max={100}
            step={1}
            value={opacity}
            disabled={!editable}
            contentAfter="%"
            onChange={(_, data) => setOpacity(data.value)}
            onBlur={applyOpacity}
            onKeyDown={event => {
              event.stopPropagation();
              if (event.key === 'Enter') {
                event.preventDefault();
                opacityRef.current?.blur();
              }
              if (event.key === 'Escape') setOpacity(String(Math.round((selected?.opacity ?? 1) * 100)));
            }}
          />
        </Field>
        {selected && (
          <div className="li-transform-summary" aria-label="Layer transform">
            X {Math.round(selected.transform?.offset_x ?? 0)} · Y {Math.round(selected.transform?.offset_y ?? 0)}
            <span>
              {Math.round((selected.transform?.scale ?? 1) * 100)}% · {Math.round(selected.transform?.rotation ?? 0)}°
            </span>
          </div>
        )}
      </div>
    </section>
  );
}
