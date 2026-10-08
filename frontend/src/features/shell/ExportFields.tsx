import { useState } from 'react';
import { Button, Checkbox, DialogActions, DialogContent, Field, Input } from '@fluentui/react-components';
import { ChoiceSelect } from './ChoiceSelect.tsx';
import type { EditorDialogsController, ExportOptions, ExportPlan } from './editorDialogs.ts';

export function ExportFields({
  id,
  plan,
  controller,
}: {
  id: number;
  plan: ExportPlan;
  controller: EditorDialogsController;
}) {
  const [format, setFormat] = useState(plan.format),
    [name, setName] = useState(plan.name.replace(/\.[^.]+$/, '') + '-export'),
    [width, setWidth] = useState(String(plan.width)),
    [height, setHeight] = useState(String(plan.height)),
    [scale, setScale] = useState('100'),
    [locked, setLocked] = useState(true),
    [folder, setFolder] = useState<string | null>(null),
    [choosing, setChoosing] = useState(false),
    [error, setError] = useState('');
  const w = Number(width),
    h = Number(height),
    suffix = format === 'jpg' ? '.jpg' : '.' + format;
  const filename = name.trim().replace(/\.(png|jpe?g|tiff?|webp)$/i, '') + suffix;
  const validSize =
    Number.isInteger(w) && Number.isInteger(h) && w >= 1 && h >= 1 && w <= 32768 && h <= 32768 && w * h <= 150000000;
  const validName =
    name.trim().length > 0 &&
    filename.length <= 255 &&
    !/[<>:"/\\|?*\x00-\x1f]/.test(name) &&
    !/[. ]$/.test(name.trim()) &&
    !/^(con|prn|aux|nul|com[1-9]|lpt[1-9])(?:\.|$)/i.test(name.trim());
  function dimensions(axis: 'width' | 'height', value: string) {
    const number = Number(value);
    if (axis === 'width') {
      setWidth(value);
      if (locked && number > 0) setHeight(String(Math.max(1, Math.round((number * plan.height) / plan.width))));
    } else {
      setHeight(value);
      if (locked && number > 0) setWidth(String(Math.max(1, Math.round((number * plan.width) / plan.height))));
    }
    setScale(String(Number(((number / (axis === 'width' ? plan.width : plan.height)) * 100).toFixed(2))));
  }
  async function chooseFolder() {
    setChoosing(true);
    setError('');
    try {
      const next = await controller.chooseExportFolder();
      if (next) setFolder(next);
    } catch (error) {
      setError(error instanceof Error ? error.message : String(error));
    } finally {
      setChoosing(false);
    }
  }
  return (
    <>
      <DialogContent>
        <div className="li-export-fields">
          <ChoiceSelect
            label="File format"
            value={format}
            disabled={choosing}
            choices={[
              { value: 'png', label: 'PNG' },
              { value: 'jpg', label: 'JPEG' },
              { value: 'webp', label: 'WebP' },
              { value: 'tif', label: plan.bitDepth === 16 ? 'TIFF · 16-bit' : 'TIFF' },
            ]}
            onSelect={value => setFormat(value as ExportOptions['format'])}
          />
          <Field
            label="Filename"
            validationMessage={!validName ? 'Enter a filename without folder paths or reserved characters.' : undefined}
            validationState={!validName ? 'error' : 'none'}
          >
            <Input value={name} contentAfter={suffix} disabled={choosing} onChange={(_, data) => setName(data.value)} />
          </Field>
          <div className="li-export-dimensions">
            <Field label="Scale (%)">
              <Input
                type="number"
                min={0.01}
                step="any"
                value={scale}
                disabled={choosing}
                onChange={(_, data) => {
                  setScale(data.value);
                  const percent = Number(data.value);
                  if (percent > 0) {
                    setWidth(String(Math.max(1, Math.round((plan.width * percent) / 100))));
                    setHeight(String(Math.max(1, Math.round((plan.height * percent) / 100))));
                  }
                }}
              />
            </Field>
            <Field label="Width (px)">
              <Input
                type="number"
                min={1}
                max={32768}
                value={width}
                disabled={choosing}
                onChange={(_, data) => dimensions('width', data.value)}
              />
            </Field>
            <Field label="Height (px)">
              <Input
                type="number"
                min={1}
                max={32768}
                value={height}
                disabled={choosing}
                onChange={(_, data) => dimensions('height', data.value)}
              />
            </Field>
          </div>
          <Checkbox
            label="Lock aspect ratio"
            checked={locked}
            disabled={choosing}
            onChange={(_, data) => {
              setLocked(data.checked === true);
              if (data.checked) setHeight(String(Math.max(1, Math.round((w * plan.height) / plan.width))));
            }}
          />
          {!validSize && <p role="alert">Choose dimensions up to 32,768 pixels per side and 150 megapixels.</p>}
          <Field label="Location">
            {plan.nativeFolder ? (
              <>
                <Input value={folder ?? ''} readOnly placeholder="Choose an output folder" aria-label="Output folder" />
                <Button disabled={choosing} onClick={() => void chooseFolder()}>
                  {choosing ? 'Choosing…' : 'Choose folder…'}
                </Button>
              </>
            ) : (
              <p>Browser download. The browser controls the save location.</p>
            )}
          </Field>
          <p>
            {format === 'jpg' ? 'JPEG requires an opaque background.' : 'Transparency is preserved.'}
            {plan.bitDepth === 16 && format !== 'tif' ? ' This format exports at 8-bit.' : ''}
          </p>
          {error && <p role="alert">{error}</p>}
        </div>
      </DialogContent>
      <DialogActions>
        <Button disabled={choosing} onClick={controller.close}>
          Cancel
        </Button>
        <Button
          appearance="primary"
          disabled={choosing || !validName || !validSize || (plan.nativeFolder && !folder)}
          onClick={() =>
            controller.respondExport(id, {
              format,
              filename,
              width: w,
              height: h,
              destination: plan.nativeFolder ? 'folder' : 'download',
            })
          }
        >
          Export
        </Button>
      </DialogActions>
    </>
  );
}
