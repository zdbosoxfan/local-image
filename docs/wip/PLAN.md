# Parallel plan (2026-10-09, after the drain)

Owner: finish coding every planned feature, then one big bug-fixing run. Translations last. Use Codex, Sonnet,
DeepSeek and the local Strata model at the same time; cloud inference is unlimited, the local CPU (16 threads) is the
bottleneck, so local build/test work is queued and batched.

## Where we are

Merged into `claude/sleepy-franklin-egimjb` today: every Develop tool on the GPU; Compositing GPU (Blend If, noisy
effects, CMYK/Lab, stutter); heal 1.5–2.5× faster; exit crash fixed; Smart Sort engine + faces + people engine +
sessions/tokens/bursts/examples/keys groundwork; vectorizer engine; one model per function + custom models;
attributions; ~300 new tests; lean builds (3.7 GB per job instead of 100–400 GB), seeded worktrees, build gate.

| Group | Item | State |
|---|---|---|
| A. In flight (finish, merge) | Codex: Smart Sort 2a ⇄ people merge; colour icons; vector V1; Library scale; tooltips + tool sets | running |
| | Sonnet WIP (stopped, saved): pure-Rust deps (blake3 `pure` + tree check left); HDR fix (tests left); vector V0 (1 commit) | small finishes |
| | DeepSeek coverage: 12 passed (review + merge), 12 in flight, 113 queued | paused queue |
| | Infra: seed cache refresh (running, 23 GB so far); 4 unpushed commits; vectorizer reference run (stopped) | |
| B. Feature chains (Codex) | Smart Sort 2b (sessions/tokens/examples/bursts/keys in the dialog) → 3 UI (bubbles, People toggles, confirm queue, headshots, Find This Person) | after 2a |
| | Vector: V1 → V1-Q (test harness) → V2 (persona + vector tool sets) → V3 (pen/node/shapes/snapping) → V4… V10 → Affinity import | sequential |
| | Vectorizer T2 (dialog, centreline, cleanup) | after T1 (merged) |
| | Colour icons contact sheet → owner review | after icons |
| C. Engineering speed | One test binary per crate (pc-io has 36); nextest; finish pure-Rust deps | Sonnet/DeepSeek |
| | Split pc-ui-egui (88k lines) and pc-engine (67k) | after features (touches everything) |
| D. Final bug-fix run | `docs/wip/BUGFIX-BACKLOG.md`: 5090 tone-eq parity, HDR, parked translations, lost script, full + GPU suites, benches, package, reinstall | last |

## Architecture: generate in the cloud, verify locally in a queue

1. **Generation tier (cloud, unlimited):** Codex (5–6 jobs), Sonnet (3–4), DeepSeek (dozens). They write code into
   seeded worktrees. DeepSeek never builds on its own any more.
2. **Verification tier (local CPU, queued):** one build queue (`build-queue.py`) owns the CPU. Entries = (worktree,
   command, priority). It runs one entry at a time with all 16 threads (two when both are small), in priority order:
   merge-gate checks for finished jobs > DeepSeek batch checks > seed refresh > benchmarks. Results go back to the job
   (DeepSeek gets compile errors for its next round; Codex/Sonnet jobs keep building inside their sandboxes but the
   compile gate caps the machine at 16 compiles and gives the queue priority via `LI_BUILD_PRIO`).
3. **DeepSeek batching:** coverage jobs are grouped **per crate** into one worktree each; every job writes its own test
   file; one check compiles the crate once for all of them and routes errors per file. ~10× less compiling than today.
4. **Strata (local, GPU + ~50 GB RAM):** it cannot run beside builds (60 GB machine; that is what froze the PC). It
   gets *windows*: the queue pauses builds (gate pause flag), Strata processes a batch of no-build tasks (review of
   DeepSeek diffs, triage of gave-up jobs, test-idea generation, doc summaries), then builds resume. Windows run when the
   build queue is empty or every ~2 h, whichever first.

## Allocation

| Model | Role | First batch |
|---|---|---|
| Codex | hard, sequential features | finish A; then SS 2b, V1-Q, T2, Library scale follow-ups; later V2/V3, crate split |
| Sonnet | medium work, finishing, reviews | finish pure-Rust + HDR WIPs; test-binary consolidation; recreate trace fixture script; spec prep for V2/SS3 |
| DeepSeek | small, parallel, no-build generation | 113 coverage tasks (per-crate batches); docs; pure-function groundwork modules for SS 2b/3 and V2 (tool-set tables, dialog state models) |
| Strata | no-build review/triage in windows | review the 12 passed DeepSeek test files; triage GAVEUP outputs; propose missing test cases |

## Order of operations
1. Drain A; push; merge; clean worktrees. 2. Stand up build-queue + per-crate DeepSeek batching + Strata window.
3. Launch B chains on Codex, C on Sonnet, coverage on DeepSeek. 4. When every feature is merged: D.
