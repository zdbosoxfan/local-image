# Safe command panic hunt

2026-10-10.

Task: `docs/wip/CODEX-TASK.md`. Continued the existing opt-in Compositing fuzz and its four
adversarial parameter sets on this branch. No commits, dependencies, unsafe code, or GUI changes.

## Implementation

1. Compositing's fuzz now denies nine commands: the platform file manager, Print / Print One
   Copy, and the six ComfyUI operations. The existing `Session::authorize` hook enforces the
   same deny-list on nested action/script calls. Registered engine commands do not open file
   dialogs; the desktop shell handles those. The cited `layer_menu_cmds::spawn` currently serves
   Reveal in Finder; Smart Object Edit Contents opens an engine document. Save/export destinations
   and the document's default path are inside a fresh temporary directory. Preferences and preset stores stay
   session-only. Segmentation models point to an empty directory.
2. Each command/parameter case runs in a separate instance of the integration-test executable.
   The child has private working, temporary-file, config, data and cache directories. After its
   deadline (four seconds normally; 15 for Library's two cold GPU entry points), the parent
   kills and reaps it before cleaning up. This replaces the old detached-thread
   timeout, which could leave a hung command running. A process-local panic hook also detects
   panics caught internally by an engine or background-job guard (both engines already catch
   command panics, so the previous outer `catch_unwind` alone could miss them). Output goes to a
   temporary log file so pipe backpressure cannot deadlock the worker. The runner rejects a child that
   exits successfully without running the requested test.
3. No command panics or actual hangs were found, so no production command fixes or per-command
   regression tests were needed. The initial Library startup timeout reports were reproduced
   and resolved in the harness as described below. The runner's own regression checks remain.
4. Library had no equivalent. Added an opt-in registry fuzz using the same runner. Every case
   creates a durable library and a 24×16 PNG under its temporary root, selects the photo and
   adds a brush mask. It exercises filesystem operations against these disposable inputs.
   Explicit output destinations, models and caches are confined there. Model download and
   camera/card volume enumeration are denied.

Both fuzzers retain their opt-in status. Normal tests verify nested denial, deny-list entries,
successful file writes, detection of internally caught panics, timeout/reaping, directory
cleanup on all outcomes, and detection of a nonexistent child-test filter. Library's write
check writes a real XMP sidecar; Compositing's writes a native document copy.

The deny-lists and path routing were audited against the current registries and filesystem /
process call sites. They cover this deterministic corpus and must be reviewed when commands
or path-bearing corpus cases are added; this is not an operating-system sandbox. The corpus
does not exhaust all parameter combinations or document/photo states.

## Sources followed

- Existing `pc-engine/src/lib.rs` / `jobs.rs` authorization hook (including nested execution),
  `file_cmds.rs`, `layer_menu_cmds.rs`, `print_cmds.rs`, `ai_cmds.rs`, and registered writers.
- Existing Library filesystem/import setup (`files.rs`, `library.rs`, `cmd/*`) and its
  last-resort panic guard (`guard.rs`, `Session::run_command`).
- Rust's official [Child documentation](https://doc.rust-lang.org/std/process/struct.Child.html)
  for `try_wait`, `kill`, `wait` and the lack of automatic child cleanup, and
  [Command documentation](https://doc.rust-lang.org/std/process/struct.Command.html) for
  child-only environment and working-directory settings. No algorithm port was needed.

## Validation

- `cargo +1.98.1 check --offline -p photocraft-engine`: passed.
- `cargo +1.98.1 check --offline -p lightcraft-engine`: passed.
- Compositing's normal harness checks: 2 passed, 2 opt-in entry points ignored.
- `cargo +1.98.1 test --offline -p photocraft-engine --test it panic_hunt -- --ignored
  --nocapture`: passed, **850 registered commands, 9 denied, 3,364 cases**, no panics or
  timeouts, 19.16 s on the final isolation settings. The two passed tests comprise the registry
  fuzz and its child entry point.
- Library's normal harness checks: 2 passed, 2 opt-in entry points ignored.
- `cargo +1.98.1 test --offline -p lightcraft-engine --test panic_hunt -- --ignored
  --nocapture`: passed, **290 registered commands, 2 denied, 1,152 cases**, no panics or
  timeouts, 28.37 s. The two passed tests comprise the registry fuzz and its child entry point.
- `CARGO_TARGET_DIR=target/clippy cargo +1.98.1 clippy --offline -p photocraft-engine
  -p lightcraft-engine --all-targets -- -D warnings`: passed.
- `cargo +1.98.1 test --offline -p photocraft-engine -p lightcraft-engine -- --nocapture`:
  **2,119 passed, 0 failed, 21 ignored**, running the full suites of only the two changed
  crates once. Both doc-test targets had zero tests.

| Crate / target | Passed | Ignored |
|---|---:|---:|
| Library unit tests | 417 | 3 |
| Library coverage | 261 | 0 |
| Library develop backend | 1 | 0 |
| Library panic-hunt harness checks | 2 | 2 |
| Compositing unit tests | 832 | 11 |
| Compositing coverage | 560 | 0 |
| Compositing integration tests | 46 | 5 |

All Cargo commands used `PATH=~/.cargo/bin:$PATH`, toolchain **1.98.1**, offline builds and
`CARGO_BUILD_JOBS=3`, with one Cargo build at a time. The final suites used three test/Rayon
threads, config/data/cache/model paths under `target/panic-hunt-final/`, and
`VK_DRIVER_FILES=/usr/share/vulkan/icd.d/nvidia_icd.x86_64.json`,
`LIGHTCRAFT_GPU_BACKEND=vulkan`, `LC_REQUIRE_GPU=1`. Format checks for both crates and
`git diff --check` passed. The shared test helper fits both crates' different rustfmt widths.
After the final formatting adjustment, the four normal panic-hunt harness checks were rerun
and passed; clippy was rerun and passed. The full suites were not repeated.

Logs are retained in `target/panic-hunt-{pc,lc,clippy,full-tests}.log`. The existing missing
`CRAFT_FONTS_DIR` build warning remains; the Japanese watermark-face check reported its
existing missing-font skip. No GPU-adapter skip or llvmpipe result occurred.

Library's first registry run reported four-second timeouts for `spot.findDust` and `app.gpu`.
A focused, isolated diagnostic of each with `{}` and the zero/invalid-index corpus case passed
all four cases, with process durations of 2.790, 2.727, 3.574 and 3.650 seconds. These commands
create the GPU device and compile its kernels on first use (`lc-gpu/src/ctx.rs`, `device()`),
and every child has an empty cache. The four-second deadline was too close to normal startup
under contention; the final fuzz allows 15 seconds for these two commands. This measures
test-process startup/work, not interactive rendering performance. The diagnostic log is
`target/panic-hunt-lc-gpu-diagnostic.log`.

GPU tests added: none; this task changes command fuzzing, not UI or GPU rendering. The existing
Develop backend test confirmed **NVIDIA GeForce RTX 5090 (Vulkan)**. All five Library
`tests_gpu::*` tests passed: AI Remove, lens profiles, native tone equalizer/layers/depth,
new CFA methods/segmentation, and raw-options re-decoding. No interactive speed claim is made.

No implementation work remains for this task. External-effect commands remain deliberately
excluded from the fuzz, and the corpus remains finite as noted above. All changes are left
uncommitted in the working tree.
