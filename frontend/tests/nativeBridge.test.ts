import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createNativeBridge } from '../src/editor/nativeBridge.ts';
import type { NativeBridge, NativeMessageEvent, NativeTransport } from '../src/editor/nativeBridge.ts';

class Clock {
  nowValue = 1700000000000;
  next = 1;
  tasks = new Map<number, { callback: () => void; due: number }>();
  now = () => this.nowValue;
  setTimeout = (callback: () => void, delay: number) => {
    const id = this.next++;
    this.tasks.set(id, { callback, due: this.nowValue + delay });
    return id;
  };
  clearTimeout = (id: unknown) => {
    this.tasks.delete(id as number);
  };
  advance(milliseconds: number) {
    const until = this.nowValue + milliseconds;
    while (true) {
      const next = [...this.tasks].filter(([, value]) => value.due <= until).sort((a, b) => a[1].due - b[1].due)[0];
      if (!next) break;
      const [id, value] = next;
      this.nowValue = value.due;
      this.tasks.delete(id);
      value.callback();
    }
    this.nowValue = until;
  }
}
class Transport implements NativeTransport {
  messages: Array<Record<string, unknown>> = [];
  listeners = new Set<(event: NativeMessageEvent) => void>();
  additions = 0;
  throwOnPost = false;
  files: readonly File[] | null = null;
  addEventListener(_: 'message', listener: (event: NativeMessageEvent) => void) {
    this.additions++;
    this.listeners.add(listener);
  }
  removeEventListener(_: 'message', listener: (event: NativeMessageEvent) => void) {
    this.listeners.delete(listener);
  }
  postMessage(message: unknown) {
    if (this.throwOnPost) throw Error('Bridge transport failed');
    this.messages.push(message as Record<string, unknown>);
  }
  postMessageWithAdditionalObjects(message: unknown, files: readonly File[]) {
    this.postMessage(message);
    this.files = files;
  }
  reply(id: unknown, result: unknown, extra: Record<string, unknown> = {}) {
    for (const listener of this.listeners) listener({ data: { type: 'local-remove-native', id, result, ...extra } });
  }
  event(data: unknown) {
    for (const listener of this.listeners) listener({ data });
  }
}
const tick = () => new Promise<void>(resolve => setImmediate(resolve));
function track<T>(promise: Promise<T>) {
  const result: { state: string; value?: T; error?: Error; promise?: Promise<void> } = { state: 'pending' };
  result.promise = promise.then(
    value => {
      result.state = 'resolved';
      result.value = value;
    },
    error => {
      result.state = 'rejected';
      result.error = error;
    },
  );
  return result;
}
function fixture(review: (id: string) => boolean | Promise<boolean> = () => false) {
  const clock = new Clock(),
    transport = new Transport(),
    errors: Error[] = [];
  const bridge = createNativeBridge({ transport, clock, onCloseRequest: review, onError: error => errors.push(error) });
  return { clock, transport, bridge, errors };
}
async function connected(f: ReturnType<typeof fixture>) {
  const promise = f.bridge.connect();
  f.transport.reply(f.transport.messages.at(-1)!.id, {
    native: true,
    version: 2,
    projects: true,
    closeRequests: true,
    setup: true,
    batch: true,
  });
  await promise;
  return f;
}
const file = () => new File([new Uint8Array([1, 2, 3])], 'synthetic.png', { type: 'image/png' });

test('one idempotent ready handshake, one listener, stable capabilities and exact three-second deadline', async () => {
  const f = fixture();
  const first = f.bridge.connect(),
    second = f.bridge.connect();
  assert.equal(first, second);
  assert.equal(f.transport.additions, 1);
  assert.equal(f.transport.messages.length, 1);
  f.clock.advance(2999);
  await tick();
  assert.equal(f.bridge.getPendingCount(), 1);
  f.clock.advance(1);
  assert.equal((await first).ready, false);
  assert.equal(f.bridge.getPendingCount(), 0);
  assert.equal(f.clock.tasks.size, 0);
  await f.bridge.connect();
  assert.equal(f.transport.additions, 1);
  assert.equal(f.transport.messages.length, 1, 'No automatic ready retry');
  assert.throws(
    () => createNativeBridge({ transport: f.transport, onCloseRequest: () => false }),
    /already has an owner/,
  );
  f.bridge.dispose();
  assert.equal(f.transport.listeners.size, 0);
  const replacement = createNativeBridge({ transport: f.transport, clock: f.clock, onCloseRequest: () => false });
  const ready = replacement.connect();
  f.transport.reply(f.transport.messages.at(-1)!.id, {
    native: true,
    version: 2,
    projects: true,
    setup: true,
    batch: true,
    closeRequests: true,
  });
  await ready;
  assert.equal(f.transport.listeners.size, 1);
  assert.ok(Object.isFrozen(replacement.getSnapshot()));
  assert.equal(replacement.getSnapshot(), replacement.getSnapshot());
  replacement.dispose();
});

test('all ten owned picker actions remain pending beyond ten minutes and settle only on OK/Cancel', async () => {
  const f = await connected(fixture());
  const operations: Array<[string, () => ReturnType<NativeBridge['openFiles']>]> = [
    ['openFiles', () => f.bridge.openFiles()],
    ['openFolder', () => f.bridge.openFolder()],
    ['openProject', () => f.bridge.openProject()],
    ['saveProject', () => f.bridge.saveProject({ session_id: 'document', revision: 3, saveAs: true })],
    ['chooseBackgroundFolder', () => f.bridge.chooseBackgroundFolder()],
    ['batchExportFolder', () => f.bridge.batchExportFolder({ job_id: 'queue', item_ids: ['one', 'two'] })],
    ['configureAi', () => f.bridge.configureAi()],
    ['setupChooseComfyDirectory', () => f.bridge.setupChooseComfyDirectory()],
    ['setupChooseModelDirectory', () => f.bridge.setupChooseModelDirectory()],
    ['setupChooseInstallDirectory', () => f.bridge.setupChooseInstallDirectory()],
  ];
  for (const [action, invoke] of operations)
    for (const cancelled of [false, true]) {
      const pending = track(invoke()),
        request = f.transport.messages.at(-1)!;
      assert.equal(request.action, action);
      assert.equal(f.clock.tasks.size, 0);
      f.clock.advance(600001);
      await tick();
      assert.equal(pending.state, 'pending');
      f.transport.event({ type: 'unrelated', id: request.id, result: { wrong: true } });
      f.transport.reply('unknown-id', { wrong: true });
      await tick();
      assert.equal(pending.state, 'pending');
      const result = { saved: true };
      f.transport.reply(request.id, result, { cancelled });
      await pending.promise;
      assert.equal(pending.state, 'resolved');
      assert.deepEqual(pending.value, cancelled ? null : result);
      assert.equal(f.bridge.getPendingCount(), 0);
      f.transport.reply(request.id, { saved: 'duplicate' });
      assert.deepEqual(pending.value, cancelled ? null : result);
    }
  f.bridge.dispose();
});

test('all nine machine operations retain the ten-minute timeout and successful replies clear it', async () => {
  const f = await connected(fixture());
  const operations: Array<[string, () => ReturnType<NativeBridge['openFiles']>]> = [
    ['drop', () => f.bridge.drop([file()])],
    ['setupUseInstallation', () => f.bridge.setupUseInstallation('candidate')],
    ['setupDownloadModels', () => f.bridge.setupDownloadModels()],
    ['setupDownloadQwen', () => f.bridge.setupDownloadQwen('int8')],
    ['setupStart', () => f.bridge.setupStart()],
    ['setupDownloadGenerationModel', () => f.bridge.setupDownloadGenerationModel('qwen', 'int8')],
    [
      'loraDownload',
      () =>
        f.bridge.loraDownload({
          model: 'qwen',
          repo_id: 'publisher/style',
          filename: 'file.safetensors',
          revision: 'a'.repeat(40),
        }),
    ],
    ['setupEject', () => f.bridge.setupEject()],
    ['setupInstall', () => f.bridge.setupInstall()],
  ];
  for (const [action, invoke] of operations) {
    const pending = track(invoke());
    assert.equal(f.transport.messages.at(-1)!.action, action);
    assert.equal(f.clock.tasks.size, 1);
    f.clock.advance(599999);
    await tick();
    assert.equal(pending.state, 'pending');
    f.clock.advance(1);
    await pending.promise;
    assert.equal(pending.state, 'rejected');
    assert.match(pending.error!.message, /timed out/);
    assert.equal(f.bridge.getPendingCount(), 0);
    assert.equal(f.clock.tasks.size, 0);
  }
  const success = track(f.bridge.setupStart());
  f.transport.reply(f.transport.messages.at(-1)!.id, { started: true });
  await success.promise;
  assert.equal(f.clock.tasks.size, 0);
  f.clock.advance(600001);
  await tick();
  assert.equal(success.state, 'resolved');
  f.bridge.dispose();
});

test('native errors, transport throws and missing File-object transport clean pending requests', async () => {
  const f = await connected(fixture());
  const error = track(f.bridge.openFolder());
  f.transport.reply(f.transport.messages.at(-1)!.id, null, { error: 'Folder is unavailable' });
  await error.promise;
  assert.match(error.error!.message, /Folder is unavailable/);
  assert.equal(f.bridge.getPendingCount(), 0);
  f.transport.throwOnPost = true;
  for (const invoke of [() => f.bridge.openFiles(), () => f.bridge.setupStart(), () => f.bridge.drop([file()])]) {
    const failure = track(invoke());
    await failure.promise;
    assert.match(failure.error!.message, /Bridge transport failed/);
    assert.equal(f.clock.tasks.size, 0);
    assert.equal(f.bridge.getPendingCount(), 0);
  }
  f.transport.throwOnPost = false;
  f.transport.postMessageWithAdditionalObjects = undefined as unknown as Transport['postMessageWithAdditionalObjects'];
  await assert.rejects(f.bridge.drop([file()]), /Use File/);
  assert.equal(f.bridge.getPendingCount(), 0);
  assert.equal(f.clock.tasks.size, 0);
  f.bridge.dispose();
  const browser = createNativeBridge({ transport: null, onCloseRequest: () => false });
  assert.equal((await browser.connect()).ready, false);
  await assert.rejects(browser.openFiles(), /desktop app/);
  browser.dispose();
});

test('typed fixed methods preserve additional File objects and strip privileged path/URL fields', async () => {
  const f = await connected(fixture());
  const chosen = file(),
    drop = track(f.bridge.drop([chosen]));
  assert.equal(f.transport.files?.[0], chosen);
  assert.deepEqual(Object.keys(f.transport.messages.at(-1)!).sort(), ['action', 'id']);
  f.transport.reply(f.transport.messages.at(-1)!.id, { opened: true });
  await drop.promise;
  const save = track(
    f.bridge.saveProject({
      session_id: 'document',
      revision: 8,
      saveAs: true,
      path: 'C:/forbidden',
      expected_hash: 'secret',
    } as Parameters<NativeBridge['saveProject']>[0]),
  );
  assert.deepEqual(Object.keys(f.transport.messages.at(-1)!).sort(), [
    'action',
    'id',
    'revision',
    'saveAs',
    'session_id',
  ]);
  f.transport.reply(f.transport.messages.at(-1)!.id, { saved: true });
  await save.promise;
  const adapter = track(
    f.bridge.loraDownload({
      model: 'qwen',
      repo_id: 'publisher/style',
      filename: 'file.safetensors',
      revision: 'a'.repeat(40),
      allow_unverified: true,
      url: 'https://forbidden.example',
      destination: 'C:/forbidden',
    } as Parameters<NativeBridge['loraDownload']>[0]),
  );
  const message = f.transport.messages.at(-1)!;
  assert.equal('url' in message, false);
  assert.equal('destination' in message, false);
  f.transport.reply(message.id, { started: true });
  await adapter.promise;
  assert.equal('request' in f.bridge, false);
  assert.equal('postMessage' in f.bridge, false);
  assert.equal('launcherKey' in f.bridge, false);
  f.bridge.dispose();
});

test('close review has no deadline, deduplicates request IDs and sends only explicit correlated approval', async () => {
  let resolve!: (value: boolean) => void;
  let reviews = 0;
  const f = await connected(
    fixture(() => {
      reviews++;
      return new Promise<boolean>(yes => {
        resolve = yes;
      });
    }),
  );
  f.transport.event({ type: 'local-remove-native', action: 'requestClose', id: 'close-1' });
  f.transport.event({ type: 'local-remove-native', action: 'requestClose', id: 'close-1' });
  assert.equal(reviews, 1);
  assert.equal(f.bridge.getCloseReviewCount(), 1);
  f.clock.advance(3600000);
  await tick();
  assert.equal(
    f.transport.messages.filter(value => value.action === 'closeReady').length,
    0,
    'Time cannot approve a close',
  );
  resolve(false);
  await tick();
  assert.deepEqual(f.transport.messages.at(-1), { action: 'closeReady', id: 'close-1', approved: false });
  assert.equal(f.bridge.getCloseReviewCount(), 0);
  f.transport.event({ type: 'local-remove-native', action: 'requestClose', id: 'close-1' });
  assert.equal(reviews, 1, 'Replayed close IDs do not re-open review');
  f.transport.event({ type: 'local-remove-native', action: 'requestClose', id: 'close-2' });
  resolve(true);
  await tick();
  assert.deepEqual(f.transport.messages.at(-1), { action: 'closeReady', id: 'close-2', approved: true });
  f.bridge.dispose();
});

test('failed close review denies approval and disposal rejects pending commands without approving closure', async () => {
  const f = await connected(
    fixture(async () => {
      throw Error('Project save failed');
    }),
  );
  f.transport.event({ type: 'local-remove-native', action: 'requestClose', id: 'failed-close' });
  await tick();
  assert.deepEqual(f.transport.messages.at(-1), { action: 'closeReady', id: 'failed-close', approved: false });
  assert.match(f.errors[0].message, /Project save failed/);
  const picker = track(f.bridge.openFolder()),
    machine = track(f.bridge.setupStart());
  assert.equal(f.bridge.getPendingCount(), 2);
  f.bridge.dispose();
  await Promise.all([picker.promise, machine.promise]);
  assert.equal(picker.state, 'rejected');
  assert.equal(machine.state, 'rejected');
  assert.equal(f.clock.tasks.size, 0);
  assert.equal(f.transport.listeners.size, 0);
  assert.equal(f.bridge.getPendingCount(), 0);
});

test('native drop requests carry only the host identity and rejection reports callback failures', async () => {
  const transport = new Transport(),
    clock = new Clock(),
    ids: string[] = [],
    errors: Error[] = [];
  const bridge = createNativeBridge({
    transport,
    clock,
    onCloseRequest: () => false,
    onDropRequest: id => {
      ids.push(id);
      if (id === 'failed') throw Error('Drop failed');
    },
    onError: error => errors.push(error),
  });
  const ready = bridge.connect();
  transport.reply(transport.messages.at(-1)!.id, { native: true, version: 2 });
  await ready;
  transport.event({ type: 'local-remove-native', action: 'requestDrop', id: 'owned', paths: ['/untrusted'] });
  transport.event({ type: 'local-remove-native', action: 'requestDrop', id: 'failed' });
  await tick();
  assert.deepEqual(ids, ['owned', 'failed']);
  assert.equal(errors[0].message, 'Drop failed');
  const accepted = bridge.acceptDrop('owned');
  assert.equal(transport.messages.at(-1)!.drop_id, 'owned');
  assert.equal(transport.messages.at(-1)!.accept, true);
  assert.equal('paths' in transport.messages.at(-1)!, false);
  transport.reply(transport.messages.at(-1)!.id, { collection: { id: 'test' } });
  await accepted;
  bridge.dispose();
});
