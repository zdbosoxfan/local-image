# Native test crash hunt — 2026-10-11

Continued `wip/codex-crash-hunt` at `2be11e2d`, including the previous builder's
warm-up counting, compiler-thread join, exit barrier and existing GPU test
locks. The owner's initial edit to `CODEX-TASK.md` is preserved. All changes
remain uncommitted. No dependencies, unsafe code, manifests or lockfiles were
added. No GUI windows were opened, and neither Local Image nor ComfyUI was
stopped or sent work.

## Diagnosis and core evidence

The engine failures are native NVIDIA shader-compiler/process-exit races. The
Rust-side lifetime gap is that the compute context, its compiled pipelines and
its free buffers lived in a static `OnceLock`; Rust does not drop statics at
exit. Waiting for application submissions and joining our compiler thread does
not destroy that retained native device. The earlier implementation also locked
device initialization rather than the complete unit-test GPU lifetime. Engine
tests can reach GPU compute indirectly through ordinary sessions, exports,
thumbnails and render jobs, alongside tests that mutate global GPU preferences
or inject device failures.

Evidence saved under `target/crash-hunt/`:

| Core | Failure and relevant threads |
| --- | --- |
| `1670067`, October 11 10:38:55 | Original engine SIGABRT. Crashing thread `1670503`: `abort → malloc_printerr → _int_free_merge_chunk → libnvidia-gpucomp.so.615.71.09 → _nv002nvvm / libnvidia-glvkspirv → libnvidia-glcore → start_thread`. Main thread is in LLVM static destruction and `__run_exit_handlers / exit`. |
| `1728835`, October 11 06:28 | Earlier engine SIGSEGV in the native NVIDIA compiler, with main in NVIDIA/GLX exit handlers. |
| `2039400`, October 11 11:20:08 | Another coordinator engine SIGSEGV during this investigation, subsequently appended to `flaky-crashes.log` at 11:26. Thread `2039703` is compiling in `libnvidia-gpucomp`; main is in `libGLX_nvidia → __run_exit_handlers → exit`. `core-2039400-gdb.txt` includes all thread backtraces. |
| `3852526`, October 10 18:33 | Historical compositor engine GPU failure in the Vulkan loader's `loader_get_icd_and_device / SetDebugUtilsObjectName`, during the node GPU test. Current node tests already use the inherited compositor GPU lock; that lock was retained and stress-tested. |

The engine exit cores have no running Rust test/render/compiler worker; Rayon
workers are sleeping. Therefore there is no active Rust test to blame for the
exit crash. The native compiler remains alive after the Rust test work has
returned. The original system collector's stored stack traces are authoritative
for the old core: the coordinator replaced its executable before the later GDB
attempt, which warns about a build-ID mismatch. That attempt is retained as
`core-1670067-gdb.txt` but is not used to infer Rust frames.

`lc-sysmem`'s native unsafe path is macOS-only; this is Linux. The crashing stacks
point to Vulkan/NVIDIA, not tract, mmap or Rust allocator callers. This is a
diagnosis of the failure class and an identified application lifetime defect,
not a claim to know the proprietary driver's exact bad free. The upstream
[wgpu NVIDIA Vulkan exit issue #8365](https://github.com/gfx-rs/wgpu/issues/8365)
documents the same native exit/destruction class and is classified as a driver
bug; [older issue #1439](https://github.com/gfx-rs/wgpu/issues/1439) also records
NVIDIA exit crashes. These support the diagnosis but do not independently prove
the precise defect in driver 615.71.09.

## Fix

- `lightcraft-gpu` now caches an owned `Arc<Gpu>` behind a short-held mutex,
  with a separate initialization lock. Status queries do not wait for kernel
  compilation. Foreground initialization joins the same counted-work barrier
  as warm-up workers.
- `shutdown(timeout)` first finishes tracked work and queue submissions, then
  takes and drops the cached native context outside the cache mutex. A timeout
  preserves the context. Callers must stop scheduling and release caches and
  denoisers first; outstanding owned leases remain valid, and a later use can
  initialize another context. `Device::destroy()` alone would only invalidate
  the API device; inspecting the cached wgpu 30.0.1 implementation showed it
  would not release all retained native resources.
- GPU unit tests and integration tests hold their existing shared lock through
  rendering, local resource drops, polling and native context destruction.
  Numerical-test devices have scoped ownership instead of another leaked
  static. The engine's GPU lock is reentrant and acquired by test sessions,
  covering indirect GPU users. A session's guard is its last field, after
  caches and worker state. Engine unit-test teardown releases the cached GPU
  denoise model too. The UI's existing last-field test guard releases the
  compute context after its app/renderer fields drop. Engine integration coverage
  that runs view/face jobs and reads GPU bookkeeping uses the same guard as
  compute integration tests, including teardown on panic.
- Buffers carry our own context identity and cannot enter another context's
  pool after shutdown/reinitialization. A regression first exposed that wgpu
  device-handle equality can report equal across distinct instances, so it is
  insufficient for this ownership check. Context destruction also subtracts
  its free buffers from the pooled-byte counter. Late buffers dropped outside
  a render are released immediately when no cached context exists.
- A denoiser owns the context that created its tensors and weights. It continues
  using that context if the global cache is released and another is created.

The production application's existing `GpuExit` barrier is retained. The new
explicit release is used by test scopes; this work does not claim to validate
desktop shutdown or change the application's long-lived model-cache policy.
No driver upgrade, GPU-off fallback or software adapter is used as a substitute
for the hardware tests.

The existing empty-Library shutdown regression takes an outer reentrant GPU
guard too. This preserves its original post-app-drop `ready` and adapter
assertions: `GpuExit` must complete warm-up before they run, and the outer guard
then releases the context. Without it the newly released cache correctly reads
unready, and the adapter diagnostic could create an unguarded replacement.

Four regressions were added: cached context/free-buffer release (including weak
ownership and zero allocated/pooled bytes), buffers surviving into a new context,
denoiser operation across context release, and nested engine session GPU scopes.
A controlled counterfactual changed only `shutdown`'s cache `take()` to `clone()`
to reinstate retained-context behavior. Both context-release regressions failed
(`0 passed, 2 failed`, exit 101, `retained-context-control.log`); the source was
restored immediately. This is a structural regression control, not an asserted
reproduction of the driver's intermittent abort.

## Load-sensitive test and fixture changes

**TEST-CHANGED:**
`tests_people::ds_finding_large_people_query_keeps_ui_cancellable` no longer
asserts that frames take less than two wall-clock seconds. It keeps all 5,000
photos and the real UI Cancel click. A test-only channel gate holds the real
query worker pending until frames and the click complete, proves it runs on a
different thread, then releases it and verifies that it observed cancellation
and cannot publish people. The original widget/data assertions remain, with
additional worker and late-result assertions. The 120-second channel watchdogs
detect deadlock/missing delivery under saturated CPUs; they are not frame-speed
budgets. The gate also has a watchdog so an inline-worker regression cannot
block forever. Production query behavior is unchanged.

Parallel full-process exploration exposed two additional fixture collisions:
the face-cache and embedding-cache deletion regressions used fixed directories
under `target/`. They now use their modules' existing PID/counter-based temporary
directory helpers. Their deletion/rebuild expectations are unchanged. No test
was removed or newly ignored, no equivalence tolerance was relaxed, and no
assertion was replaced with a skip.

## Reproduction and stress regression

`scripts/stress-native-tests.py` repeats prebuilt test binaries, pins executable
copies and SHA-256 hashes, validates nonempty test discovery, supplies isolated
preference/model/temp directories, and records timestamped test output, PID,
duration and actual process exit status. This includes crashes after a passing
Rust summary. Concurrent processes provide GPU stress; optional bounded-memory
CPU workers provide CPU stress. Hardware selection is RTX 5090/Vulkan, with GPU
required and explicitly enabled. A per-process-tree 12 GiB memory watchdog
matches the machine runner's cap, and a separate hang watchdog is documented.
On ABRT/SEGV/BUS/ILL it collects `coredumpctl info` and all-thread GDB backtraces.
Only its own child process groups are stopped on interruption.

Example (after an offline, gated build; use the current executable shown by
Cargo):

```sh
python3 scripts/stress-native-tests.py \
  --binary target/debug/deps/<lightcraft-engine-lib-test> \
  --runs 4 --parallel 2 --cpu-workers 16 --timeout 1200 \
  --output target/crash-hunt/new-engine-stress \
  -- --test-threads=16 --nocapture
```

Baseline exploration used a pinned valid coordinator engine executable:
12 fault/fallback runs with four parallel processes and 16 CPU workers, and 20
save-over GPU parity runs with four parallel processes overlapping that load,
all exited zero. Thus my targeted baseline attempts did **not** independently
reproduce the intermittent native abort. The additional coordinator core above
is the live recurrence observed during this investigation. A four-process full
baseline exploration was interrupted after the two shared-fixture failures;
it is not counted as a passing full batch. It also used coordinator compile-time
fixture paths, so subsequent full runs use this worktree's own binary.

Two exploratory artifacts are deliberately excluded: an inherited incomplete
linker `.tmp` executable crashed before test discovery (PID `1993764`, unrelated
to the driver failure); and an early 32-run warm-up batch was not pinned and
overlapped Cargo replacing the binary, so it is not a single-version baseline.
The checked-in harness now prevents both mistakes through copying, hashing and
test-list validation.

## Final validation

All final commands/processes exited zero. Full affected-crate coverage totals
**1,194 active tests**: engine 839, GPU 97, UI 258. Existing ignores remain
unchanged (engine 6, GPU 6, UI 4); doc-test targets discover zero tests.

| Validation | Result | Evidence |
| --- | --- | --- |
| Engine lib, final rebuilt source | 455 passed, 4 existing ignores; native exit 0 | `engine-final-unit.log` |
| Engine coverage + backend, final scopes | 381 + 1 passed; native exits 0 | `engine-final-integration.log` |
| Engine panic-hunt normal targets | 2 passed, 2 existing opt-in ignores | `full-tests.log` |
| Complete GPU crate, rebuilt after accounting fix | 44 unit + 3 denoise + 3 fallback + 44 parity/toolset + memory/quiesce/warm-up (one each) = 97 passed; native exits 0 | `gpu-final-tests.log` |
| Complete final UI binary | 258 passed, 4 existing ignores; native exit 0, no watchdog failures | `ui-final-full/0-000/tests.log`, `result.json` |
| Final People regression after bounded gate update | 1 passed | `people-final-test.log` |
| Workspace check | exit 0 | `workspace-check.log` |
| Strict all-target Clippy, all three changed crates | exit 0, no lint suppressions added | `strict-clippy.log` |
| Formatting check of all three changed crates | exit 0 | `final-fmt-check.log` |
| Python compilation; patch whitespace check | passed | `python3 -m py_compile scripts/stress-native-tests.py`; `git diff --check` |

The first complete combined run (`full-tests.log`, exit 101) caught two
implementation follow-ups: the new context regression detected stale pooled
accounting, and the empty-Library test's post-drop diagnostic needed an outer
scope. Both were fixed, preserving the shutdown test's original assertions.
The corrected GPU crate, changed engine integration targets and complete UI
binary were rerun; engine lib was rebuilt and rerun too. Required static checks
passed on the final Rust source. The inherited machine test runner sometimes
prints a `/proc` RSS arithmetic diagnostic as a process exits; those native/Cargo
exits are zero. The machine script was not edited. Direct stress runs use the
checked-in harness's process-tree RSS monitor instead.

The sequential commands (with the isolated profile and required hardware
environment described above) were:

```sh
cargo +1.98.1 test --offline --no-fail-fast \
  -p lightcraft-engine -p lightcraft-gpu -p lightcraft-ui-egui -- --test-threads=16
cargo +1.98.1 test --offline -p lightcraft-gpu -- --test-threads=16 --nocapture
cargo +1.98.1 test --offline -p lightcraft-engine \
  --test coverage --test develop_backend -- --test-threads=16 --nocapture
cargo +1.98.1 test --offline -p lightcraft-ui-egui \
  tests_people::ds_finding_large_people_query_keeps_ui_cancellable -- --exact --nocapture
cargo +1.98.1 check --offline --workspace
CARGO_TARGET_DIR=target/clippy cargo +1.98.1 clippy --offline \
  -p lightcraft-engine -p lightcraft-gpu -p lightcraft-ui-egui --all-targets -- -D warnings
cargo +1.98.1 fmt -p lightcraft-engine -p lightcraft-gpu -p lightcraft-ui-egui --check
cargo +1.98.1 test --offline -p lightcraft-engine --lib -- --test-threads=16 --nocapture
```

The final full UI binary ran through `scripts/stress-native-tests.py` with one
run, 16 test threads and a 1,200-second hang watchdog. It overlapped the rebuilt
engine suite, and its initial portion overlapped the CPU-loaded UI exit batch.
It completed in 213.6 seconds, with all assertions passing and actual exit 0.
Machine-readable Cargo results are in `final-validation.json`; every stress
batch has `binaries.json`, `invocation.json`, `summary.json` and per-run logs.

| Pinned repeated-process batch | Runs / parallel / CPU workers | Result |
| --- | --- | --- |
| Full engine lib, scoped native-context fix | 4 / 2 / 16 | 4 clean exits, 1,820 active test executions; 239.9–377.5 s/run |
| Separate-process immediate warm-up / release | 32 / 4 / 16 | 32 clean exits; hardware logged in each run |
| People cancellation | 20 / 4 / 16 | 20 passes and clean exits; 4.3–13.8 s for the entire process, including fixture/teardown, not a measured frame budget |
| Existing compositor node GPU lock | 20 / 4 / 16 | 20 passes and clean exits on discrete RTX 5090/Vulkan |
| Final context ownership + pool bookkeeping | 16 / 4 / 16 | 32 regression executions, 16 clean exits |
| Final empty-Library Cmd+Q / warm-up / release | 16 / 4 / 16 | 16 passes and clean exits on RTX 5090/Vulkan |

These are **108 clean repeated-process runs**, plus the final full UI process.
There were no native crashes, hang-watchdog failures or memory-watchdog failures
in these batches. The four engine and 32 warm-up snapshots precede the pooled
counter follow-up; their context ownership/locking fix is the same. The final
context, empty-Library, full UI, full GPU and rebuilt engine validations cover
that follow-up and the final scopes. The full engine stress batch ended at
load averages 62.0 / 60.5 / 47.4 on this 16-thread machine; the People batch ended
at 48.7 / 27.2 / 30.3. Load averages are stress context, not speed claims.

## Rules and remaining limits

All compilation uses Rust 1.98.1 and inherited
`RUSTC_WRAPPER=/home/zdavidson/.local/share/local-image-dev/bin/li-rustc-gate`,
offline, with one Cargo invocation at a time. `CARGO_BUILD_JOBS` is unset,
following the current machine gate/build-lane policy; the older shared rules'
fixed jobs cap is superseded. CPU stress uses separate bounded-memory processes.
Hardware tests log the physical RTX 5090/Vulkan adapter; no software skips are
counted as GPU validation. Task-local test profiles avoid the owner's AI settings
and model roots. System logs/cores and shared instructions were read only.

Finite clean stress runs cannot prove a proprietary native driver will never
crash. They validate the scoped ownership/locking fix under the tested load.
No change is claimed for unrelated desktop driver or compositor bugs.

Final re-read at **2026-10-11T11:58:42.629854-04:00**: `flaky-tests.log` still contains only the
People cancellation test, now covered by the deterministic regression and load
runs. `flaky-crashes.log` contains the original double free and the 11:26 status
139 entry; both engine cores were analyzed above. Copies of both final logs are
in `target/crash-hunt/flaky-*-final.log`. `coredumpctl list --since
"2026-10-11 11:30:00"` found no subsequent cores. All task-owned stress workers
and validation processes have finished.
