# Portable development queue independent review

**READY for root integration and the scheduled Python suite: PQ-R1 is resolved by independent v2 source review.** The reviewed port preserves the held/coordinator-only operational boundary. The original14plus one new lane-separation test have appropriately mocked dangerous process/service seams;59total test methods are now authored/retained, not run by this reviewer. No source edits, tests, installer/dispatcher/dashboard invocation, process/service inspection/control, network or builds occurred; this report is the only write.

## V2 correction accepted

Reviewed `/tmp/portable-dev-queue-v1-corrections.patch`, SHA-256 `d28e84d32b9c0bd84e71e012ddbe4824ef2cd8057bd288d13dc9d4f776365f03`, and its exact three-file before/after manifest. Updated full20-file patch SHA-256 `379776186f77d0f90f0133d8fcd7b3a0d7772e278fe2cba454de0d53c7c94203`; all20 source hashes/modes match the refreshed handoff. Core4 and retained memory/watchdog tests were re-compared and remain byte-identical to original (core4 also matched installed in the initial comparison).

`build_state_dir` now applies the same no-symlink/outside-source rule as DEV. `install.apply` resolves and validates lane root against explicit/source repo before verify/idle/any mkdir or lock creation, and uses the captured checked root in the critical section. No extra process/service invocation is added. The new temporary-repo negative covers direct in-checkout lane path and a symlinked ancestor, asserting no installer state directory, no scheduler path/lock and unchanged repository path inventory. It inherits the same mocked assert_idle and fatal unexpected subprocess.run seam. Thus the concrete source-write hole is closed without modifying preserved gate/watchdog code.

Read-only CLI preflight remains accurately described as source syntax/target-hash checking; it does not query idle services. Full runtime configuration/admission is enforced before explicit apply writes. No live installer, dashboard, dispatcher or suite was executed for this re-review.

Disposition: **PQ-R1 closed; READY**. Root should run the updated59-method suite once in its scheduled non-GPU interval, then record the actual result. No approval to install/activate is inferred from source readiness.

## Original v1 scope and hashes

Reviewed `/tmp/portable-dev-queue-handoff.patch`, SHA-256 `5c93e716f868f23a62c2511ad8e5ccea64ed853ff1c171420357fe57520e0ba8` (110212bytes), matching JSONmanifest and checkout `/home/zdavidson/.codex/worktrees/portable-dev-queue/Local Image - Testing`, base `9aea6865df63d8f6359c695975dcb07a42485453`. All20 source hashes and file modes match. Actual source reviewed for li_config.py, install.py, li-dispatch, dashboard, systemd template, fixed held entrypoints and changed tests.

Core preservation verified by direct SHA/byte comparison against both original source and installed core tools:

| File | SHA-256 | Comparison |
|---|---|---|
| bin/li-rustc-gate | cbed1a19ca3a3f1efa67d4b88b683f73d05a734ced572af8f9d690b65b13709a | original+installed exact |
| bin/li-test-runner | 632a5f78929607dbc6ec0dcb1fe79e364ac5975583bdf0bc38a549f74facf7b2 | original+installed exact |
| bin/li_state.py | abc9201bc1d2869586a80ddd880d26bbff30558429b7fb7fcb32c43293a0cf99 | original+installed exact |
| bin/li_validation.py | 0d649a876c71c6a56f74df38e044b5d3c65d7f433bb11dcde1850407c282f670 | original+installed exact |
| tests/test_memory_admission.py | bc4ee3608d73fb4df0cac8545920832fd5e33d13b9f23de705a907618682e018 | original exact; no installed test copy |
| tests/test_runner.py | 5eba5e4be6d54d45832a3c8fb4c8ffb7ec2ba925819d425454b15e54b97a0fa0 | original exact; no installed test copy |

No secret values, personal `/home/...` paths, historical runtime/ledger contents or imported credentials were found in the changed portable source. The core's retained `.claude/worktrees` out-dir recognition is a compatibility identity parser, not an author-checkout fallback. DeepSeek dashboard code is inert read-only status parsing, not a model launcher/API client. Explicit Linux-only scope is accurate.

## Historical v1 PQ-R1 — P2 (resolved in v2 above)

`bin/li_config.py:46-47` calls only `absolute` for LI_BUILD_STATE_DIR. Unlike LI_DEV_DIR, it never calls the source-containment check. `install.py:179-184` then uses that accepted path to mkdir/open scheduler `.lock` and marker files. A caller setting `LI_BUILD_STATE_DIR=$LI_REPO/queue-state` therefore gets scheduler runtime written into the checkout despite the port's explicit runtime-outside-source contract. Symlink refusal does not prevent this ordinary absolute-path case. The default `/tmp/li-build-lanes` is safe; the configured case is not rejected.

Minimal correction in the already-owned config/installer scope: validate the scheduler root against the same recognized/explicit repo roots as DEV, and resolve/validate it before apply creates installation or scheduler locks (also include it in read-only preflight configuration validation). Reuse that checked value for installer locking and unit generation. Add the case to the existing mocked temp-repo `test_runtime_inside_repo_and_relative_or_invalid_repo_refused`: set lane root below synthetic repo, assert refusal and no scheduler file/directory in that repo. No gate/watchdog byte changes are needed; their direct callers still supply the coordinator-approved shared external scheduler path.

Sent exact finding to root. This is a concrete configured-path failure, not a request for an unrelated portability rewrite.

## Operational boundaries that are sound in frozen v1

- `li-dispatch` performs one read-only systemd state query before a locked ledger transaction; failed query prevents reconciliation. Coordinator-tagged jobs are skipped entirely. It can only reconcile vanished legacy running/starting/landing records. `launch_builder`/`launch_land` raise immediately; tick has no dispatch/landing branch even with no hold and paused=false.
- All five legacy action entrypoints (launcher, land, rework, sweep, update-gate) are byte-identical four-line `/bin/sh` printf+exit2 stubs. No args are evaluated, no subcommands/model CLI, Git mutation, task copying, cache deletion or service activation exists in them. This includes root's explicit li-land policy correction.
- Configuration checks absolute/nonempty paths, invalid control characters and symlinked ancestors; source checkout inference requires .git/Cargo.toml/crates and installed dashboard requires explicit LI_REPO. No fallback to a personal checkout or guessed worktree exists.
- Dashboard uses only recorded job worktrees, cached status and loopback127.0.0.1:8765. Request/bug writes go below DEV/dashboard with path validation, O_NOFOLLOW, flock, flush/fsync. They record requests only; they cannot start work. Optional recovery evidence stays separate. Installed service output is generated only on explicit apply and no unit/timer activation/reload command exists.
- Installer accepts existing owned files only when bytes match source, destination's installed manifest, or an explicitly reviewed hash map. Unknown edited bytes fail before replacement; backups preserve prior owned bytes/modes. Existing hold and job bytes are retained; fresh install creates only empty held coordinator state. It rejects missing hold, invalid ledger container and symlinked owned paths. It never copies old jobs/state into the checkout or silently adopts an original machine hash file.
- Apply snapshots source bytes and verifies hashes, serializes on install/dispatch/land/jobs and gate mutex/lease locks, performs idle checks before and after locking, and atomically fsync/replaces each allowlisted file. No automatic rollback/retry is claimed for a partial multi-file install; backup and hold remain. Optional unit output is quoted/escaped and rejects differing existing content. No live install occurred in this review.

## All14 new portability tests: process safety audit

`tests/test_install.py` has13 PortableTests methods plus one DisabledEntrypointTests method:

- Every PortableTests setup creates TemporaryDirectory with private DEV, lane and synthetic repo roots; patches installer.assert_idle; additionally patches the shared subprocess.run method to raise AssertionError on any unexpected process/service call. Apply/preflight/config/dashboard functions invoked by these tests contain no other subprocess seam. Locks, backups, atomic-write failure and request-file tests operate on those private temporary paths.
- Module loading only defines functions/configured paths and does not enter dashboard main/server or installer main. The dashboard status test mocks process/lanes scanning; it does not activate a listener. Dashboard record test explicitly rejects a symlink into the synthetic repo and preserves its sentinel.
- Unit-generation test inspects text only; the process seam remains fatal if invoked. It also checks escaping and different-existing-content refusal.
- The one intentional subprocess method invokes only fixed `/bin/sh` on the five verified held stubs with an explicit minimal environment and PATH=/nonexistent, timeout2s. printf/exit are shell builtins. It asserts exit2, held message and unchanged sentinel/directory. There is no path to systemctl/model/Git/Cargo from these exact stubs.
- The three changed legacy launch tests now assert held refusal, unchanged job/task state and no command invocation; the previous automatic-launch expectations were explicitly declared TEST-CHANGED. Other existing reconciliation/coordinator-exclusion tests preserve substantive state controls.

The full58-method suite also retains existing bounded process-group/compiler simulations, concurrent ledger children and temporary Git-checkout tests. Those are not mocked into nonexistent work, and root should schedule them outside exclusive GPU/AI activity as already documented. This review does not claim the58methods were run or passed. Current14new tests do not call live installer/service manager or mutate user runtime.

## Disposition

PQ-R1 corrected and narrow delta/full-source hashes independently verified; no additional blocker found in this bounded review. Root owns execution of the updated59-method suite. Source portability is not authority to run --apply or activate the dashboard/queue. Return to the pending PDF runtime lease re-review when its corrected artifact is ready.

Reviewed 2026-10-12T19:15:12.695482+00:00.
