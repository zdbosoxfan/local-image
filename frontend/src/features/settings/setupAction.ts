import type { SetupState } from './types.ts';

/** Observe the existing setup job; never resubmit a machine operation. */
export async function waitForSetupAction(
  initial: SetupState,
  read: () => Promise<SetupState>,
  options: { active?: () => boolean; wait?: () => Promise<void>; attempts?: number } = {},
) {
  let state = initial;
  const action = initial.job?.action,
    jobId = initial.job?.id;
  for (let attempt = 0; ; attempt++) {
    if (options.active?.() === false) throw Error('GPU unload is still pending. Check Settings for its outcome.');
    if ((jobId && state.job?.id !== jobId) || (action && state.job?.action !== action))
      throw Error('GPU unload status changed. Check Settings for its outcome.');
    if (state.job?.status === 'error') throw Error(state.job.error || state.job.message || 'GPU unload failed.');
    if (state.job?.status === 'complete') return state;
    if (state.job?.status !== 'running')
      throw Error('GPU unload could not be confirmed. Check Settings for its outcome.');
    if (attempt >= (options.attempts ?? 120))
      throw Error('GPU unload could not be confirmed. Check Settings for its outcome.');
    await (options.wait?.() ?? new Promise<void>(resolve => setTimeout(resolve, 500)));
    state = await read();
  }
}
