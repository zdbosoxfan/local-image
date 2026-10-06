import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createEditorApi, EditorApiError } from '../src/editorApi.ts';
import type { EditorDocument, StackRequest } from '../src/contracts.ts';

const doc = (id = 'a', revision = 1): EditorDocument => ({
  id,
  revision,
  name: 'Fixture',
  width: 10,
  height: 10,
  layer_stack: [],
});
const operation = (documentId = 'a'): StackRequest => ({
  documentId,
  revision: 1,
  tail: '/stack/layer/layer',
  body: { opacity: 0 },
  method: 'PATCH',
});

test('serializes one document at execution revision, preserves zero opacity and token', async () => {
  const sent: RequestInit[] = [];
  let release!: () => void;
  const held = new Promise<void>(resolve => {
    release = resolve;
  });
  const api = createEditorApi('page-token', (async (_url, options) => {
    sent.push(options!);
    if (sent.length === 1) await held;
    return Response.json(doc('a', sent.length + 1));
  }) as typeof fetch);
  api.observe(doc());
  const first = api.mutate(operation());
  const second = api.mutate(operation());
  await new Promise(resolve => setTimeout(resolve, 0));
  assert.equal(sent.length, 1);
  release();
  await Promise.all([first, second]);
  assert.deepEqual(
    sent.map(options => JSON.parse(options.body as string)),
    [
      { opacity: 0, revision: 1 },
      { opacity: 0, revision: 2 },
    ],
  );
  assert.equal((sent[0].headers as Record<string, string>)['x-local-remove-token'], 'page-token');
});

test('different documents do not block each other', async () => {
  let release!: () => void;
  const held = new Promise<void>(resolve => {
    release = resolve;
  });
  const api = createEditorApi('token', (async url => {
    const id = String(url).includes('/a/') ? 'a' : 'b';
    if (id === 'a') await held;
    return Response.json(doc(id, 2));
  }) as typeof fetch);
  const a = api.mutate(operation());
  assert.equal((await api.mutate(operation('b'))).id, 'b');
  release();
  await a;
});

test('asset operations share the layer queue and capture document data only when their turn begins', async () => {
  const order: string[] = [];
  let release!: () => void;
  const held = new Promise<void>(resolve => {
    release = resolve;
  });
  const api = createEditorApi('token', (async () => {
    order.push('layer');
    await held;
    return Response.json(doc('a', 2));
  }) as typeof fetch);
  const layer = api.mutate(operation());
  const asset = api.runDocumentOperation('a', async () => {
    order.push('asset');
    return 'accepted';
  });
  await new Promise(resolve => setTimeout(resolve, 0));
  assert.deepEqual(order, ['layer']);
  release();
  await layer;
  assert.equal(await asset, 'accepted');
  assert.deepEqual(order, ['layer', 'asset']);
});

test('conflict invalidates queued writes and never retries; deliberate next command can recover', async () => {
  let count = 0;
  const api = createEditorApi('token', (async () => {
    count++;
    return count === 1 ? Response.json({ detail: 'Revision conflict' }, { status: 409 }) : Response.json(doc('a', 3));
  }) as typeof fetch);
  const results = await Promise.allSettled([api.mutate(operation()), api.mutate(operation())]);
  assert.equal(count, 1);
  assert.ok(results.every(result => result.status === 'rejected'));
  api.observe(doc('a', 2));
  await api.mutate(operation());
  assert.equal(count, 2);
});

test('stale and wrong-document responses are rejected', async () => {
  for (const response of [doc('a', 0), doc('b', 2)]) {
    const api = createEditorApi('token', (async () => Response.json(response)) as typeof fetch);
    await assert.rejects(
      api.mutate(operation()),
      (error: unknown) => error instanceof EditorApiError && error.status === 409,
    );
  }
});

test('newer accepted revision overtakes in-flight response', async () => {
  let release!: () => void;
  const held = new Promise<void>(resolve => {
    release = resolve;
  });
  const api = createEditorApi('token', (async () => {
    await held;
    return Response.json(doc('a', 2));
  }) as typeof fetch);
  const pending = api.mutate(operation());
  await new Promise(resolve => setTimeout(resolve, 0));
  api.observe(doc('a', 4));
  release();
  await assert.rejects(pending, /stale result/);
});

test('network and malformed failures are not retried', async () => {
  for (const mode of ['network', 'malformed']) {
    let count = 0;
    const api = createEditorApi('token', (async () => {
      count++;
      if (mode === 'network') throw new TypeError('Offline');
      return new Response('bad');
    }) as typeof fetch);
    await assert.rejects(api.mutate(operation()));
    assert.equal(count, 1);
  }
});
