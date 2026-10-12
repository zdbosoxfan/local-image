# Local Image coordinator tools (Linux)

Portable source for the existing build admission gate, test-process watchdog,
locked job ledger and loopback dashboard. This port does **not** start an automatic
queue. Work assignment, validation and Git integration belong to the explicitly
authorized parallel coordinator, with one owner per source scope.

## Supported policy

- Default admission is three independent build trees, four Cargo jobs per caller,
  two test threads per binary, ten global compiler slots, 10 GiB host reserve and
  2 GiB below each ancestor cgroup memory high/max limit. Clean inactive page cache
  receives conservative credit; dirty/writeback bytes do not. Hard limits remain.
- Resource capacity does not grant a build permit. Release/LTO, broad workspace,
  GPU and AI acceptance runs are exclusive. The coordinator schedules lighter
  disjoint work separately and records exact source/features/evidence once.
- Keep existing targets through rework. Do not sweep caches during development,
  hot-swap the gate, duplicate expensive validation, or recreate a cancelled monitor.
- `li-dispatch` only reconciles vanished legacy services. Coordinator-owned jobs
  (`dispatch_mode=coordinator`) are never automatically reconciled or modified.
  Queued/finished/rejected jobs are never launched, landed or reworked, even if the
  hold file is absent or paused=false. A failed service query leaves state intact.
- `codex-launch.sh`, `li-land`, `li-rework`, `li-sweep-targets` and `update-gate.sh`
  are nonzero compatibility stubs. They make no files, launch no CLI/model/service,
  rotate no evidence, commit/merge nothing and delete no cache. Use ordinary explicit
  reviewed Git integration and validation, not a legacy automatic landing policy.

## Paths and commands

Python 3, Linux `/proc`, `fcntl` and waitid(WNOWAIT) are required. The service template
and optional legacy reconciliation require user systemd. No Python dependencies
are installed. Windows/macOS process supervision is not claimed.

`LI_REPO` is the absolute intended Local Image Git checkout root. Source-tree tools
can infer their enclosing checkout after checking Cargo.toml, crates and .git;
installed dashboard tools require it explicitly. Paths with spaces are supported.
`LI_DEV_DIR` defaults to `~/.local/share/local-image-dev`; all jobs, locks, receipts,
backups and dashboard request/bug records stay there, outside the source tree.
`LI_BUILD_STATE_DIR` defaults to `/tmp/li-build-lanes` and must be shared by callers
using the same budget. `LI_RECOVERY_DIR` optionally points to coordinator evidence;
without it the dashboard reports no coordinator record instead of guessing one.
Every job's worktree location must be recorded explicitly; there is no implicit
personal checkout or `.claude/worktrees` layout.

Read-only source/hash preflight (does not query or alter services):

```sh
python3 scripts/dev-queue/install.py --dev-dir /absolute/private/queue-state
```

At a separately coordinated idle boundary, explicit checked copy:

```sh
python3 scripts/dev-queue/install.py --dev-dir /absolute/private/queue-state --apply
```

Fresh installs create an empty paused coordinator ledger and hold file. Upgrades
require the existing hold, preserve all job bytes, and accept existing tool bytes
only if they match the source or destination-local recorded installed hashes.
Unknown edits require a separately reviewed JSON mapping of relative tool names
to SHA256, passed with `--reviewed-hashes /absolute/reviewed.json`. No historical
machine hash file is treated as universal permission to replace another install.
Symlinked owned paths, active queue services/build processes and locked build leases
are refused. Installers serialize against queue/gate locks. Copied tools, manifests
and rollback receipts use atomic file replacement; prior owned bytes/modes are
backed up under queue-backups. A partial install is not a release: retain the error
and backup, stay held, and restore/review owned files before retrying. Never replace
jobs.json with an old backup merely to roll back executable source.

Optional `--unit-output /absolute/user-unit-directory/li-dashboard.service` writes
an escaped unit using LI_REPO and the chosen runtime path. It does **not** reload,
enable, start, stop or restart any service/timer. Different existing unit content is
refused. Unit activation is a separate explicit operation outside this installer.
The template in source is not directly installable until its placeholders resolve.

Run the dashboard manually only when wanted:

```sh
LI_REPO=/absolute/checkout LI_DEV_DIR=/absolute/private/queue-state \
  python3 -I /absolute/private/queue-state/dashboard/dashboard.py
```

It binds only `127.0.0.1:8765`. Its request/bug box records requests for a human
coordinator; it does not launch work. Reconciliation is a separate explicit call to
`python3 -I /absolute/private/queue-state/bin/li-dispatch`, not an installed timer.

Coordinator-approved scoped validation can use the unchanged wrappers directly:

```sh
RUSTC_WRAPPER=/absolute/private/queue-state/bin/li-rustc-gate \
  CARGO_BUILD_JOBS=4 RUST_TEST_THREADS=2 cargo check --offline -p chosen-crate
/absolute/private/queue-state/bin/li-test-runner /absolute/compiled-test-binary filter
```

The watchdog default group RSS budget is12 GiB, configurable through positive finite
`LI_TEST_MAX_GB`. Preserve each command's exit code. Exact-name retry evidence must
include a successful exit and the named test passing; zero selected tests is not a
pass. The helper module's scratch-checkout API is retained for tests/explicit callers,
not connected to an automatic land command.

## Source validation and excluded state

The original tool suite and installed watchdog/gate already have recorded evidence;
that history is not a fresh pass for this port. After changes, the coordinator runs
one scheduled batch:

```sh
python3 -B -m unittest discover -s scripts/dev-queue/tests -v
```

Tests use temporary state and mocked launch/systemd seams. Some existing gate and
watchdog tests spawn bounded process groups; schedule them apart from exclusive
GPU/AI acceptance. Installer tests never touch live user units or runtime. Do not
run a live installer/dispatcher as a source test.

Version only the reviewed allowlist. No jobs/board/hold state, request history,
active-worker/direct-validation receipts, logs, installed manifests, backups,
targets, locks/leases, config/key files, tokens, credentials or user profiles belong
here. Machine-specific direct-validation/recovery launchers and original hash
provenance remain outside the portable package. No unrestricted CLI launch body or
fixed model/effort selection is shipped.
