import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createEditorDialogs } from '../src/features/shell/editorDialogs.ts';
const plan = {
  all: true,
  dirtyCount: 2,
  names: ['A.png', 'B.png'],
  pendingSelection: true,
  nativeProjects: true,
  warning: 'Save projects before closing.',
};
test('close decisions do not perform saves or imply successful native completion', async () => {
  let focused = 0;
  const ui = createEditorDialogs({
    dontAskBeforeOverwrite() {
      throw Error('wrong decision');
    },
    focusCanvas() {
      focused++;
    },
    copy: async () => {},
    downloadCredits() {},
  });
  const pending = ui.confirmClose(plan),
    state = ui.getSnapshot();
  assert.equal(state.kind, 'close');
  if (state.kind !== 'close') throw Error();
  assert.ok(Object.isFrozen(state.plan.names));
  ui.respond(state.id + 1, 'discard');
  assert.equal(ui.isOpen(), true);
  ui.close();
  assert.equal(await pending, null);
  assert.equal(focused, 1);
  assert.equal(ui.isOpen(), false);
  const next = ui.confirmClose(plan),
    snapshot = ui.getSnapshot();
  if (snapshot.kind !== 'close') throw Error();
  ui.respond(snapshot.id, 'save');
  assert.equal(await next, 'save');
});
test('overwrite preference changes only on explicit overwrite approval, never Cancel or unique copy', async () => {
  let changes = 0;
  const ui = createEditorDialogs({
    dontAskBeforeOverwrite() {
      changes++;
    },
    focusCanvas() {},
    copy: async () => {},
    downloadCredits() {},
  });
  for (const choice of [null, 'unique', 'overwrite'] as const) {
    const answer = ui.confirmOverwrite('original.tif');
    ui.setDontAsk(true);
    const state = ui.getSnapshot();
    if (state.kind !== 'overwrite') throw Error();
    ui.respond(state.id, choice);
    assert.equal(await answer, choice);
  }
  assert.equal(changes, 1);
});
test('opening a second dialog never strands the first decision and failed clipboard never reports success', async () => {
  const ui = createEditorDialogs({
    dontAskBeforeOverwrite() {},
    focusCanvas() {},
    copy: async () => {
      throw Error('denied');
    },
    downloadCredits() {},
  });
  const first = ui.confirmOverwrite('A.tif');
  await assert.rejects(ui.confirmClose(plan), /current dialog/);
  ui.close();
  assert.equal(await first, null);
  ui.showCredits([{ label: 'Source', attribution: 'Fixture credit' }], 'Fixture credit');
  await ui.copyCredits();
  const state = ui.getSnapshot();
  assert.equal(state.kind, 'credits');
  if (state.kind === 'credits') {
    assert.equal(state.error, true);
    assert.doesNotMatch(state.status, /copied/);
  }
});
