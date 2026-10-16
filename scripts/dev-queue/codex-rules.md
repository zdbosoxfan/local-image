# Coordinator development policy

These are repository tool notes, not autonomous worker launch instructions.
Follow the current user-authorized coordinator scope, source ownership and validation
plan. No model, sandbox override, credential access or external request is implied.

- Keep one coding owner per overlapping area; use isolated worktrees and stable
  target directories. Preserve existing work and exact before/after source evidence.
- Default resource caps: three admitted build trees, four Cargo jobs per caller,
  two test threads, ten global compiler slots. Coding permits are assigned separately.
  Large release/LTO/workspace/GPU/AI runs remain exclusive regardless of spare slots.
- Run bounded relevant tests once per coherent change; coordinator owns affected and
  integration gates. Keep source stable during validation. Use offline Cargo where
  dependencies are available. Record failures, interruptions and admission waits
  honestly; do not silently retry whole batches or weaken assertions.
- Preserve unfinished/rejected caches and reports. Never hot-patch installed tools,
  delete active caches or update job JSON outside the shared transaction lock.
- Legacy automatic launching, landing and rework remain held. Coordinator-owned
  jobs are display/record state, not dispatch authority. An absent hold file does not
  authorize an automatic worker. Do not recreate user-cancelled recurring monitors.
- Use explicit reviewed ordinary Git integration and the current approval rules.
  No tool in this package supplies model launch, automatic dirty commits, automatic
  conflict resolution or failure allowlists. Respect the current installed app and
  unrelated files. No build benchmark or hardware claim follows from scheduler tests.
