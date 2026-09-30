import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createEditorController } from '../src/editorController.ts';
import type { EditorSnapshot, LegacyEditor } from '../src/contracts.ts';

test('stable immutable snapshots reject stale document revisions and keep navigation separate', () => {
  let source: EditorSnapshot = { document: { id: 'a', name: 'Image', revision: 2, width: 10, height: 10, layer_stack: [] }, selectedLayerId: null, busy: false, workspace: 'retouch', showOriginal: false, canUndo: true, canRedo: false, status: 'Ready' };
  let publish = () => {};
  let detached = false;
  const legacy = { getSnapshot: () => source, subscribe: (listener: () => void) => { publish = listener; return () => { detached = true; }; }, setStackTransport: () => {}, commands: {} } as unknown as LegacyEditor;
  const controller = createEditorController(legacy, 'private');
  const first = controller.getSnapshot(); let changes = 0;
  const unsubscribe = controller.subscribe(() => changes++);
  publish(); assert.equal(controller.getSnapshot(), first); assert.equal(changes, 0);
  assert.ok(Object.isFrozen(first.document?.layer_stack));
  source.document!.name = 'Changed'; assert.equal(first.document!.name, 'Image');
  source = { ...source, document: { ...source.document!, revision: 1 } }; publish(); assert.equal(controller.getSnapshot(), first);
  source = { ...source, document: { ...source.document!, id: 'b', revision: 0 } }; publish(); assert.equal(controller.getSnapshot().document?.id, 'b');
  source = { ...source, document: { ...source.document!, id: 'a', revision: 1 }, busy: true }; publish(); assert.equal(controller.getSnapshot().document?.id, 'a'); assert.equal(controller.getSnapshot().document?.revision, 2); assert.equal(controller.getSnapshot().busy, true);
  unsubscribe(); controller.dispose(); assert.ok(detached);
});
