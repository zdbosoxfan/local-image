# Portable queue source port — frozen implementation brief

Read-only survey, 2026-10-12. Candidate still has no `scripts/dev-queue` directory.
Compared the original source with RELEASE-ACCEPTANCE-MATRIX.md's selective-port
section. No scripts, tests, builds, services, installers, dispatcher or CLI workers
were executed. Only this brief was written. This is the next bounded source slot,
not a new scheduling system or an authorization to resume the legacy queue.

## Exact source disposition

Paths below are relative to `scripts/dev-queue/`.

| Files | Disposition / smallest required change |
|---|---|
| `bin/li-rustc-gate`, `bin/li-test-runner`, `bin/li_state.py`, `bin/li_validation.py` | Portable Linux source: retain behavior. Gate already takes LI_BUILD_STATE_DIR and resource environment knobs; state takes LI_DEV_DIR; validation helper accepts explicit repo/scratch/branch. Watchdog deliberately depends on Linux `/proc`, process groups and waitid(WNOWAIT): document Linux-only rather than claim cross-platform support. |
| `tests/test_memory_admission.py`, `tests/test_runner.py` | Copy source unchanged; fixtures derive checkout paths and isolate process/state resources. Existing evidence is sufficient for this survey. |
| `tests/test_queue.py` | Retain assertions and mocks; adjust only fixture configuration needed for explicit repository/legacy-launch refusal. Add focused portability/installer refusal tests here or a dedicated `tests/test_install.py`. |
| `dashboard/dashboard.py` | Replace author REPO and implicit `.claude/worktrees` root with validated LI_REPO and per-job worktree (legacy root only when explicitly configured). Respect LI_DEV_DIR / LI_BUILD_STATE_DIR. Store requests/bugs/history under DEV/dashboard, never alongside versioned source (`HERE` currently does this). Recovery evidence directory is optional explicit configuration; absent evidence must show unavailable, never invent tasks. Keep loopback127.0.0.1, cached snapshots, locked request writes and no queue launch endpoint. |
| `bin/li-dispatch` | Remove author LI_REPO fallback. Resolve configuration only when an operation needs it, so import/mocked tests remain safe. Reconcile existing legacy states but preserve coordinator-owned exclusions. Public tick must refuse all automatic builder/landing admission in this port even if hold disappears: coordinator mode is the supported policy. Refusal must happen before creating worktrees, copying task files or moving reports. Existing backend bounded-admission tests may exercise mocked internals, but add public-entrypoint no-launch proof. |
| `bin/li-land` | Keep reviewed implementation only as explicit manual tooling: require validated repo, exact existing worktree and destination ref; remove personal R, `.claude` layout and implicit `wip/vector` destination. Respect LI_DEV_DIR and configured Cargo; preserve locks/owned scratch checks and validation evidence. No dispatcher path may call it automatically. No execution during port. |
| `bin/li-rework`, `bin/li-sweep-targets`, `bin/update-gate.sh` | Replace executable entrypoints with clear nonzero fail-closed stubs in this bounded port. Rework currently rotates evidence before CLI launch; sweeper hardcodes personal branches and deletes caches; update-gate bypasses idle/hash installer checks. None is necessary for the user-authorized parallel coordinator workflow. Documentation points to explicit coordinator review/checked installer, not auto-retry or cache deletion. |
| `codex-launch.sh` | Include only a short fail-closed explanatory stub, never the legacy body. Current body sets `SB=danger-full-access`, model/effort defaults, `/usr/lib/chatgpt/...`, personal paths and rewrites task prompts. Stub exits before any filesystem/service/process mutation. No env flag silently restores unrestricted launch. |
| `run-recovery-worker.py` | Omit from portable runtime/install list (or equivalent nonzero stub only). Current helper fixes a model, effort, binary location and “only worker” policy; conflicts with explicit current parallel coordinator ownership. Original recovery copy remains untouched. |
| `direct-validate.py` | Do not copy unchanged. For this bounded port prefer omit from installer and leave original recovery helper untouched. A future portable adapter must require explicit worktree/output/toolchain/font locations and a versioned validation entrypoint; current file hardcodes v2 worktree, Linux toolchain, machine fonts, systemd slice, unversioned recovery run.py and tracked-tree runtime receipts. Core gate/runner are directly usable without it. Do not quietly transplant recovery run.py/logs to satisfy the dependency. |
| `README.md`, `codex-rules.md` | Rewrite as portable usage/policy documentation. Keep 3 build lanes /4 Cargo jobs /10 global compiler slots and separate coding permits; large release/GPU/AI validations remain exclusive. Remove “runs without sandbox”, fixed host hardware/model and historical enabled-service claims. State supported coordinator dispatch, held legacy jobs, no unattended rework, no recurring monitor recreation. Human instructions never come from copied stale task prompts. |
| `install.py`, `systemd/li-dashboard.service` | See installer seam below. Unit becomes relocatable/generated rather than an author-specific absolute path; source copying must not enable/start/restart it. |
| `original-hashes.json` | Omit from executable installer trust bootstrap. It is historical machine provenance, not a universal approved baseline. Existing original copy remains for audit; don't treat it as portable target authorization. |

Recommended port inventory: the four core files, three regression files, dispatcher,
manual land implementation, dashboard, three held maintenance stubs, launcher stub,
README/rules, checked installer and service template (18 existing paths), plus one
small shared path-configuration helper and focused installer tests if needed. Omit
three recovery/provenance paths listed above. Keep this exact allowlist; do not copy
the entire scripts tree or installed directory recursively.

## Configuration and installer seam

Use one small `bin/li_config.py` if needed so LI_DEV_DIR, LI_REPO and build-state
paths cannot diverge across tools. Source-checkout default repo can derive from
script location only after validating the expected checkout; installed tools must
require explicit LI_REPO/config. Never default to this author's original repository.
Reject empty/non-directory/wrong Git roots before mutable operations. Existing
job.worktree is authoritative; no filename-derived path traversal or implicit
checkout creation. Private configuration lives outside repository and contains no
serialized environment/key dump. Keep resource defaults distinct from permission
to run several builders: the coordinator still assigns each build lane.

Installer must support read-only preflight and explicit source-only apply with a
consistent install FILES allowlist (no missing launch dependencies). Use source
hash equality or a destination-local installed manifest for upgrades; unknown
existing destination bytes require explicit reviewed adoption, not author hashes.
Brand-new private installs may initialize empty jobs with paused=true and the hold
file, never populate historical jobs. Existing jobs/state/hold are preserved. Keep
idle/held preconditions, atomic owned-file writes and backups; do not hot-swap gates.
Remove unconditional timer stop/restart, dashboard enable/restart and reconciliation
execution from source installation. An optional generated systemd unit may be written
only when explicitly requested; lifecycle activation remains separately authorized.
For standard paths `%h` can replace personal home; custom paths require correct
systemd escaping/template substitution and an explicit LI_REPO environment setting.

## Boundaries and evidence

Never port jobs.json, board.json, queue-hold, request/bug/history JSONL, active-worker
or direct-validation receipts, installed manifests, logs, lock/slot/lease trees,
backups, targets, user config, tokens, private keys, or `.codex/.agents/.aws` trees.
Do not read/export credential contents. No service/timer activation, legacy auto
builder/lander/rework, sandbox bypass or extra autonomous CLI model calls is part
of this port. The approved parallel coordinator/worker model remains authoritative.

Read-only byte comparison confirms original source equals installed versions:
- gate SHA256 `cbed1a19ca3a3f1efa67d4b88b683f73d05a734ced572af8f9d690b65b13709a`;
- watchdog SHA256 `632a5f78929607dbc6ec0dcb1fe79e364ac5975583bdf0bc38a549f74facf7b2`.

Existing installed39-test and later original44-test evidence is recorded in the
coordinator/matrix; neither was rerun. README contains several historical counts,
so a portable rewrite must label those historical rather than claim a fresh pass.
After actual edits, the coordinator schedules one source-only Python suite/syntax
pass and path/launcher scan, with mocked systemd/launch/installer seams and temporary
state. Add fresh-install/unknown-existing refusal, custom paths with spaces, no
activation/dispatch, launcher zero-side-effect refusal, runtime files outside
checkout, and coordinator jobs untouched controls. No live --apply/dispatcher run.
