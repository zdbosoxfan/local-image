import { test } from 'node:test';
import assert from 'node:assert/strict';
import { waitForSetupAction } from './setupAction.ts';

test('GPU unload observes the original setup job until completion without submitting again', async () => {
  let reads = 0,
    waits = 0;
  const result = await waitForSetupAction(
    { job: { action: 'eject', status: 'running' } },
    async () => ({ job: { action: 'eject', status: ++reads === 2 ? 'complete' : 'running' } }),
    {
      wait: async () => {
        waits++;
      },
    },
  );
  assert.equal(result.job.status, 'complete');
  assert.equal(reads, 2);
  assert.equal(waits, 2);
});
test('deferred GPU unload rejection is reported rather than announced as success', async () => {
  await assert.rejects(
    waitForSetupAction(
      { job: { action: 'eject', status: 'running' } },
      async () => ({ job: { action: 'eject', status: 'error', error: 'A Comfy job is still active.' } }),
      { wait: async () => {} },
    ),
    /Comfy job is still active/,
  );
});
test('GPU unload does not wait for an unrelated replacement setup job', async () => {
  await assert.rejects(
    waitForSetupAction(
      { job: { action: 'eject', status: 'running' } },
      async () => ({ job: { action: 'download', status: 'running' } }),
      { wait: async () => {} },
    ),
    /status changed/,
  );
  await assert.rejects(
    waitForSetupAction(
      { job: { id: 'own-job', action: 'eject', status: 'running' } },
      async () => ({ job: { id: 'replacement-job', action: 'eject', status: 'complete' } }),
      { wait: async () => {} },
    ),
    /status changed/,
  );
});
test('disposed or unconfirmed unload remains unknown and never resubmits', async () => {
  let reads = 0;
  await assert.rejects(
    waitForSetupAction(
      { job: { status: 'running' } },
      async () => {
        reads++;
        return { job: { status: 'running' } };
      },
      { active: () => false },
    ),
    /pending/,
  );
  assert.equal(reads, 0);
  await assert.rejects(
    waitForSetupAction(
      { job: { status: 'running' } },
      async () => {
        reads++;
        return { job: { status: 'running' } };
      },
      { wait: async () => {}, attempts: 1 },
    ),
    /could not be confirmed/,
  );
  assert.equal(reads, 1);
  await assert.rejects(
    waitForSetupAction({}, async () => {
      reads++;
      return {};
    }),
    /could not be confirmed/,
  );
  await assert.rejects(
    waitForSetupAction({ job: { status: 'unknown' } }, async () => {
      reads++;
      return {};
    }),
    /could not be confirmed/,
  );
  assert.equal(reads, 1);
});
