import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createDocumentApi, DocumentApiError } from '../src/editor/documentApi.ts';
import type { DocumentMetadata } from '../src/editor/documentContracts.ts';

// Ports the API-specific cases from editorApi.test.ts to the production owner.
// The shared layer/asset execution test lives in documentController.test.mjs.
const doc = (id = 'a', revision = 1): DocumentMetadata => ({
  id,
  revision,
  name: 'Fixture',
  width: 10,
  height: 10,
  layers: [],
  layer_stack: [],
});
const turn = () => new Promise(resolve => setTimeout(resolve, 0));
const deferred = () => {
  let resolve!: () => void;
  const promise = new Promise<void>(yes => {
    resolve = yes;
  });
  return { promise, resolve };
};
const mutate = (api: ReturnType<typeof createDocumentApi>, id = 'a') =>
  api.mutate(doc(id), '/stack/layer/layer', { opacity: 0 }, 'PATCH');

test('document API serializes at execution revision and preserves zero opacity and page token', async () => {
  const sent: RequestInit[] = [],
    held = deferred();
  const api = createDocumentApi('page-token', (async (_url, options) => {
    sent.push(options!);
    if (sent.length === 1) await held.promise;
    return Response.json(doc('a', sent.length + 1));
  }) as typeof fetch);
  api.observe(doc());
  const first = mutate(api),
    second = mutate(api);
  await turn();
  assert.equal(sent.length, 1);
  held.resolve();
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

test('document API queues for different images do not block one another', async () => {
  const held = deferred();
  const api = createDocumentApi('token', (async url => {
    const id = String(url).includes('/a/') ? 'a' : 'b';
    if (id === 'a') await held.promise;
    return Response.json(doc(id, 2));
  }) as typeof fetch);
  const a = mutate(api);
  assert.equal((await mutate(api, 'b')).id, 'b');
  held.resolve();
  await a;
});

test('document API conflicts invalidate queued writes without retry and allow a deliberate later command', async () => {
  let count = 0;
  const api = createDocumentApi('token', (async () => {
    count++;
    return count === 1 ? Response.json({ detail: 'Revision conflict' }, { status: 409 }) : Response.json(doc('a', 3));
  }) as typeof fetch);
  const results = await Promise.allSettled([mutate(api), mutate(api)]);
  assert.equal(count, 1);
  assert.ok(results.every(result => result.status === 'rejected'));
  api.observe(doc('a', 2));
  await mutate(api);
  assert.equal(count, 2);
});

test('document API rejects stale and wrong-document responses', async () => {
  // Intentional classification difference from editorApi.test.ts: the current
  // readDocument treats a wrong identity as a protocol error (502); a valid
  // identity with an old revision remains a stale conflict (409).
  for (const [response, status] of [
    [doc('a', 0), 409],
    [doc('b', 2), 502],
  ] as const) {
    const sent: RequestInit[] = [];
    const api = createDocumentApi('token', (async (_url, options) => {
      sent.push(options!);
      return Response.json(sent.length === 1 ? response : doc('a', 2));
    }) as typeof fetch);
    const results = await Promise.allSettled([mutate(api), mutate(api)]);
    assert.equal(results[0].status, 'rejected');
    assert.equal(results[1].status, 'rejected');
    if (results[0].status === 'rejected') {
      assert.ok(results[0].reason instanceof DocumentApiError);
      assert.equal(results[0].reason.status, status);
    }
    if (results[1].status === 'rejected') {
      assert.ok(results[1].reason instanceof DocumentApiError);
      assert.equal(results[1].reason.status, 409);
    }
    assert.equal(sent.length, 1, 'Neither the failed request nor its invalidated queued successor is retried');
    assert.equal((await mutate(api)).revision, 2);
    assert.equal(
      JSON.parse(sent[1].body as string).revision,
      1,
      'Rejected response metadata does not advance the accepted source revision',
    );
  }
});

test('newer observed document revision overtakes an in-flight response', async () => {
  const held = deferred(),
    api = createDocumentApi('token', (async () => {
      await held.promise;
      return Response.json(doc('a', 2));
    }) as typeof fetch);
  const pending = mutate(api);
  await turn();
  api.observe(doc('a', 4));
  held.resolve();
  await assert.rejects(pending, /stale result/);
});

test('document API network and malformed JSON failures are never retried', async () => {
  for (const mode of ['network', 'malformed']) {
    let count = 0;
    const api = createDocumentApi('token', (async () => {
      count++;
      if (mode === 'network') throw new TypeError('Offline');
      return new Response('bad');
    }) as typeof fetch);
    await assert.rejects(mutate(api));
    assert.equal(count, 1);
  }
});
