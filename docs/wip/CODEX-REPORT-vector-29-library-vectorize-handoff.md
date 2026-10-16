# Package 29 — Library Vectorize handoff

2026-10-10; continued base `6be90631`. Changes are uncommitted. The inherited
branch contains the shared vector foundations and package 24's source-only
registration handoff; it had no package 29 implementation to replace.

**Owned handoff implementation is supplied. Integrated product acceptance is
still pending packages 00/28; this report does not call the package fully done.**
The task explicitly reserves existing menus/module/job routers for 00 and says
to code against a missing provider's interface. Its final SPEED RULES prohibit
GPU acceptance runs, release builds, benchmarks and full test suites here.

## Implemented

- Library photo context contribution **Vectorize in Compositing…**, with a stable
  automation widget ID, bounded file-path eligibility and one transient request
  on the shared egui context. It captures the exact context PhotoId, catalog
  revision and file path; active photo and multiselection can differ. Queued
  requests disable duplicate gestures and are drained once. Stale catalog/file
  targets, demo sources, unavailable host and Camera Raw sessions are rejected.
- The action does not select photos, change Library/People filters or UI state,
  copy/import/stack files, save Develop settings, call an external editor or
  create a separate tracer. Existing Library/headless and open/module patterns
  were followed in place rather than introducing another decoder/service.
- Compositing state starts through the ordinary bounded `open_path`/`open_file`
  and existing background file-open job/progress tab. It correlates the exact
  JobId, resolves the returned document into DocId before job UI tab/script
  effects, and captures the new raster's ID/revision/singleton target selection.
  Errors/cancellation produce no launch; unrelated or duplicate events cannot
  retarget old work. Launch consumption rechecks staleness and occurs once.
  Cancel drops the request/job and retains any loaded photo/document/history.
- Loaded document origin persists through dialog Cancel. The shell can suppress
  its existing auto-select-photo/reset-view behavior when returning to Library,
  preserving the original browsing state. Closed origin markers are pruned.
  The host handoff also specifies a transient first-return flag for background
  failure/cancellation before any new document exists.
- Added append-only module registration in the two UI `lib.rs` files under the
  task's latest registration override. No shared menu/router, manifest, lock,
  People, Develop, engine, serialization schema or i18n catalog was edited.
  Old settings/documents require no migration or new persisted fields.
- Exact call signatures, menu selection-on-action fix, same-window host routing,
  batch-before-dispatch job hook, 28 modal launch, return/cancel behavior and test
  registration are in [integration/29.md](vector-tasks/integration/29.md).
  English literals are in [i18n/29.tsv](vector-tasks/i18n/29.tsv).

## Verification

All builds use Rust/Cargo 1.98.1, `--offline`, `CARGO_BUILD_JOBS=3` and the inherited
`RUSTC_WRAPPER=/home/zdavidson/.local/share/local-image-dev/bin/li-rustc-gate`.
Only one cargo build runs at a time. No desktop window, owner process termination,
ComfyUI job, benchmark or release build. Only this package's new tests are selected.

The actual branch's workspace check cannot load metadata: inherited
`crates/pc-affinity/Cargo.toml` is absent. Shared files remain untouched. A scratch
`git archive HEAD` checkout at `target/library29-registration/worktree` adds
only the documented package 24 manifest/workspace/engine dependency registration.
Package 29 sources/tests are symlinked from this worktree, and its two small UI
module registrations are copied in. Scratch lock comparison adds **one existing
internal package**, `photocraft-affinity@2.0.0-alpha.1`, and zero external
package/version pairs; the real branch's Cargo.lock/manifests are unchanged.

| Gate | Result |
|---|---|
| Actual branch `check --workspace --all-targets` | Blocked before compilation by missing inherited pc-affinity manifest |
| Scratch `check --workspace --all-targets` | Passed, one completed workspace check |
| Strict clippy, both changed UI crates, all targets | Passed, including final fixture changes |
| Library new `library_vectorize` tests | **3 passed, 0 failed, 1 ignored**, 233 unrelated tests filtered out |
| Compositing new `library_vectorize` tests | **6 passed, 0 failed, 0 ignored**, 1,040 unrelated unit tests filtered out |
| Revised valid-PNG background cancellation case | **1 passed, 0 failed, 0 ignored**, 1,045 unrelated unit tests filtered out |
| Owned sources rustfmt / `git diff --check` | Passed |

The three executed Library tests use the established **HeadlessView** driver and
actual secondary/primary pointer events to open a photo context menu and click
the owned action. Independent expected photo/selection/filter/People/catalog
snapshots prove target 2 is handed off while active 1 and selection [1,3] stay
unchanged; queued duplicates, disabled/demo sources and stale requests are tested.

The registered production **Headless** grid test is separately ignored for the
missing 00 menu hook. It clicks actual `thumb:2`, asserts that simply opening its
context menu preserves selection, then clicks the action with active People
filter IDs [17,21]. That is a dependency gate, not an isolated-widget pass.

The six Compositing tests cover kittest clicks, actual PNG decoding with independent
red/blue pixel oracles, exact new-raster launch/one-time consumption, retained
loaded/old documents and history, stale document/revision/selection, real worker
completion events, unrelated/duplicate events, decode failure and cancellation,
and refusal to retarget a previous raster when a nonraster document loads.
The worker tests observe the real Session events; permanent shell dispatch remains
00's integration responsibility. The initial Compositing compilation captured
the earlier cancellation fixture. The revised case alone was then compiled and
executed once: cancellation uses a valid PNG and requires `Cancelled`, while
corrupt bytes independently require `Failed`. No other test was rerun. The
filtered integration binaries ran zero cases (123 unrelated cases filtered out).

Logs: `target/library29-branch-check.log`, `target/library29-scratch-check.log`,
`target/library29-clippy.log`, `target/library29-clippy-final.log`,
`target/library29-library-tests.log`, `target/library29-compositing-tests.log`,
`target/library29-cancel-test.log`. Cargo fmt used the registered scratch
workspace; owned provider-dependent sources also pass direct rustfmt checks.
The only build-script warning is the inherited missing external craft-fonts
CJK bundle. No CJK/corpus gate is claimed successful.

## Supplied dialog / GPU gates and remaining integration

- `library_vectorize/dialog_tests.rs`: real package 28 `State::open/show/cancel`,
  clicked load and Cancel, exact decoded raster ID/revision, one launch and
  retained loaded document/history. **Not registered, compiled or executed**:
  28's dialog/output modules are absent in this branch. It uses the actual WIP
  provider API inspected read-only, with no substitute/fake modal.
- `library_vectorize/gpu_tests.rs`: explicitly ignored offscreen real-app test
  requiring **RTX 5090 / Vulkan / discrete**, Rgba32Float accumulation and
  float16 canvas support. It loads a synthetic PNG through the handoff, displays
  the actual 28 Result preview, compares sampled live GPU pixels to detached CPU
  preview bytes at native texel scale with premultiplied max diff <=1/255,
  performs pointer press/drag/release, checks parity again and clicks Cancel.
  **Not registered, compiled or executed**: 28 is absent, and the final owner
  speed rule forbids GPU acceptance runs here. No adapter skip is counted as a
  pass, and no hardware/performance result is claimed.
- 00 must wire the existing Library context menu without its current eager
  selection change; extend its existing file/module route without a second
  Develop open; observe a whole polled job batch before any tab/script effects;
  open 28 once after completion; and preserve Library selection/view on return.
  Exact integration fragments and the production-grid test are supplied.
- Restore inherited package 24's registration via its integration lane, merge
  28, register the supplied provider tests, then complete full host/menu/dialog
  and RTX acceptance on `wip/vector`. This branch cannot establish those gates
  while respecting the shared-file ownership contract.
- Full crate/workspace test execution, GPU runs, release builds and performance
  benchmarks were **not run**, as required by the final speed override. No
  proprietary/external corpus was required for the original synthetic fixtures;
  missing dependencies/fonts are reported separately from passing tests.

## Sources / attribution

See [29-SOURCES.md](vector-tasks/integration/29-SOURCES.md) for the existing
Library/headless/open/job bridge sources, exact baseline and pinned egui 0.36.2
official local API documentation, provider APIs, original fixture license,
image 0.25.10 encoder and parameters. All new code/fixtures are original Local
Image GPL-3.0-or-later. No upstream algorithm/file port, new external dependency,
unsafe code, C/-sys build addition, brand asset or new license/PORTS row. The
attribution fragment for 00 is explicitly empty.
