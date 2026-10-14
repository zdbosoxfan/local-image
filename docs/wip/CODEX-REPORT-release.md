# Release hygiene — N13, S7, QW5, QW9

Continued `wip/cx-release` at `c83768d9`, preserving the inherited known-failure fixes and
`xtask assets` target-output exclusion. The owner's pre-existing `CODEX-TASK.md` change is
preserved. All four items are implemented; current validation gates pass. Changes remain uncommitted. No push, release creation, workflow dispatch, live app
launch, or owner-file modification was performed.

The referenced holistic review was absent from this checkout. Read the copy in the adjacent
integration worktree (same reviewed base), including its executive summary, A3/R6, R1,
G13/U8/R7 and N13/S7/QW5/QW9 roadmap rows.

## N13 — test-suite health

- Ran the complete workspace once, offline and without failure filtering: **12,028 passed,
  1 failed, 195 existing ignored**, across 220 suite summaries. The single failure was the
  translation inventory scanning the new update-window source while the workspace test
  binary still held the earlier pending-key inventory. Its nine new English messages are now
  registered, including the desktop-only fallback. The corrected inventory regression passes.
  No pre-existing application test failure reproduced.
- The external `~/.local/share/local-image-dev/land-known-failures.txt` contains comments only
  (cleared 2026-10-11). Read it; did not modify external coordinator state. No known failing
  test is allow-listed, removed or newly ignored.
- Added `cargo +1.98.1 xtask test-fast`: complete geometry, colour, metadata, catalog,
  document/format and xtask suites, then qualified update-service and UI interaction filters.
  Qualification excludes unrelated hardware/performance tests. Measured **2 min 6.56 s**
  wall time, including incremental compilation; the documented target is a warm cache. No new runner dependency or
  nextest installation is required. The existing `xtask` alias now also builds offline/locked.
- Kept the asset audit's inherited exclusion of untracked `target/` at any depth, including
  its regression. Tracked assets still require attribution. Added a matching recursive
  `.gitignore` entry for generated headless outputs; did not delete build caches.
- Added the opt-in nightly workflow and [lane documentation](../RELEASE.md). Full tests and
  strict lint run on dedicated owner-labelled Linux Vulkan / Apple Silicon Metal runners;
  native compositor, Develop and Vulkan haze gates remain explicit. Model/fixture-dependent
  ignored acceptance tests remain documented milestone gates, not presumed passes.
- Full-suite PDF tests rewrote tracked generated files under `crates/pc-pdf/target/refvec/pdf21`;
  restored those incidental outputs to HEAD, preserving source/reference fixtures.

TEST-CHANGED: `links::tests::help_menu_lists_links_and_updates_then_separator_then_system_info_and_about`
adds `help.checkUpdates` to the exact expected Help order. Keeps every existing link, separator,
System Info and About assertion; the new no-document click regression verifies the action is live.

TEST-CHANGED: `prefs_ui::tests::preference_labels_are_translated` checks that the one new
`general.checkForUpdatesAtStartup` caption is registered in the owner's deferred translation list.
Every existing caption and every choice label retains the strict translation assertion. No
existing untranslated string is exempted. This follows the task's explicit instruction to put
new strings on pending lists rather than completing translations in this job.

New UI fixtures wait for the established app font stack before painting shared buttons. The
actual click regression caught and fixed a missing `help.checkUpdates` enablement rule with
no document open. Fresh GPU viewports and dimension assertions prevent stale resize renders.

## S7 — packaging and CI

- Fast CI now runs on Linux x86_64 and verified arm64 `macos-15`. Retained the portable queue
  regression step. Existing broad Mac acceptance is manually dispatched; full workspace and
  GPU/shader coverage move to the documented nightly lane rather than every push/PR.
- Added manual `release-artifacts.yml` with read-only repository contents permission. Uploads
  Actions artifacts only; has no tag/push/release API step.
- Linux: existing HEIF-enabled Rust app/CLI tarball plus a standard AppDir/AppImage. The tool
  and type-2 runtime are downloaded separately with reviewed SHA-256 checks before use.
  Compression inherits the build-job limit. The launcher preserves file arguments and the
  smoke check uses `--version`, which exits before creating an app window.
- macOS: invokes the existing `cargo xtask package macos`, then produces an app ZIP and DMG
  alongside the installer/checksums. Developer ID Application/Installer identities and a
  temporary certificate keychain are wired to repository secrets. Complete notarization
  credentials enable `notarytool --wait`, stapling/validation and Gatekeeper assessment.
  Absent/partial credentials skip clearly; a configured signing/notarization error is fatal.
  Key material is cleaned on exit. Checksums use portable basenames.
- Default packages keep **SAM 3 off**. HEIF remains enabled. Packaging Cargo builds are offline.
- Verified workflow syntax with **actionlint 1.7.12**, shell syntax, three mocked Mac secret-gate
  regressions, launcher argument forwarding, and an actual AppImage tool/runtime build and
  execution of a synthetic AppDir fixture. This is packaging-tool proof, not a built release
  of the application. Verified tool build `854e19e` and its pinned hash.

## QW5 — Check for Updates

- Help action in Compositing and Library/Develop routes to one shared window/worker; the native
  editor Help menu inherits the same action. Works without an open document.
- Uses existing `ureq` + rustls + RustCrypto/webpki configuration. No new dependencies and
  **Cargo.lock is unchanged**. Worker requests have a configured 15-second deadline, 2 MiB
  response limit and no redirect following. Sends a generic User-Agent, no telemetry,
  authorization, photos, identifiers or model data.
- Compares numeric SemVer core and prerelease identifiers (build metadata ignored), rejects
  drafts/invalid tags, and validates the canonical release-page URL. Stable builds offer stable
  releases; alpha/beta builds can offer prereleases. Publication order cannot suggest a downgrade
  from V2 to 0.7.
- Only **Open Release Page** opens the browser; never downloads or installs assets. Explicit
  errors offer Retry, and Close/Escape remain available while a worker is pending. Silent
  startup errors/up-to-date results do not interrupt work; closing an explicit check is respected.
- **General → Check for updates at startup** is serde-defaulted to false and persists through
  the existing shared preferences. Old settings retain neighboring values and load with it off.
- Added real menu/preference/link/Retry/Close/Escape clicks, persistence/legacy checks, channel
  notification tests, HTTP-contract/size/error tests, and one mandatory-native-GPU screenshot
  fixture for both themes, 250/400 pt and 1.0/1.5 display scale.
- New strings are listed in the editor and Library i18n pending lists, per the owner's priority.

## QW9 — documentation

- What's New now names Local Image and describes the existing Smart Sort & Export UI.
- README describes requested/opt-in update checks, preview status of 0.7 releases, configured
  remote ComfyUI transfers, and supported Linux x86_64 / Apple Silicon macOS 13+ targets.
  Windows is explicitly experimental. Legacy Windows/Linux installer pages are labelled 0.7.
- Added [Coming from Lightroom Classic](../COMING-FROM-LIGHTROOM-CLASSIC.md) and
  [Coming from Photoshop / Affinity](../COMING-FROM-PHOTOSHOP-AFFINITY.md): module mappings,
  metadata/catalog versus rendered files, Develop layers, `.pcraft` masters, export and
  interchange limits, AI brush → adjust → confirm and update privacy.
- Updated development/Mac packaging instructions for fast/nightly lanes and SAM-off defaults.

## Validation

Validation logs are retained under `target/release-validation/` in this worktree. Rust/Cargo
1.98.1, offline builds, inherited `CARGO_BUILD_JOBS=4`, one Cargo command at a time, unchanged
target directory and existing shared compiler gate. Local font-embedding notices are inherited;
these English UI checks do not claim CJK rendering validation. CI pins the required font checkout.

| Gate | Result |
| --- | --- |
| Workspace reproduction, `--workspace --no-fail-fast` once | 12,028 passed / 1 new inventory failure / 195 existing ignored; fixed and revalidated below |
| `cargo check --offline -p local-image` | Passed |
| Release service focused tests | 4 passed |
| Editor update interactions + translation inventory | 6 passed |
| Library Help click | 1 passed |
| Native GPU screenshots, dark/light × widths × scales | 1 test passed, 8 dimension-checked renders; RTX 5090; all inspected |
| Strict Clippy, all targets of all six changed crates | Passed, `-D warnings` |
| Current complete changed-crate suites | **4,916 passed, 0 failed, 134 ignored** (five crates from the six-crate run; complete corrected editor rerun replaces its initial result) |
| Fast lane timing, including incremental compilation | **126.56 s wall time** (123.1 s inside xtask), **791 passed / 0 failed / 1 existing ignored** |
| Mac secret gating regressions | 3 passed |
| Actionlint / shell syntax / local document links | Passed |
| AppImage fixture / runtime / launcher arguments | Passed |
| Asset and dependency audits after test output | Passed: **2,050 attributed assets**, all referenced licences present; `deps-pure` passed |
| Formatting / diff whitespace / lockfile unchanged | Passed; owner task-file EOF whitespace preserved/excluded from diff check |

Commands for the final gates are `cargo +1.98.1 clippy --offline --all-targets` and
`cargo +1.98.1 test --offline --no-fail-fast`, selecting only `li-ai`, `photocraft-engine`,
`photocraft-ui-egui`, `lightcraft-ui-egui`, `local-image` and `xtask`; lint ends with `-- -D warnings`.
Focused regressions are qualified by module/name. The initial six-crate run had 4,914 passed,
2 failed and 134 ignored: only the two UI expectations described in TEST-CHANGED above failed.
After those changes the complete editor rerun passed **1,768 tests, 0 failed, 81 ignored**;
strict editor Clippy also passed again. The other five crates did not need a repeated suite.
No workspace rerun was used to develop this feature. Existing ignored annotations are retained;
the one new native-GPU screenshot test was run explicitly and passed.

## Established sources and scope

Followed the existing TLS provider in `crates/li-ai/src/tls.rs`, native bundle/signing code in
`xtask/src/macos.rs`, shared menu dispatch, preference serialization and font-bootstrap fixtures,
and [DESIGN-SYSTEM.md](../DESIGN-SYSTEM.md). All new controls use shared widgets and active theme
roles; no hard-coded chrome colours, spacing, radii or font sizes, new icon artwork or dependencies.

Official sources: [GitHub Releases API](https://docs.github.com/en/rest/releases/releases#list-releases),
[SemVer precedence](https://semver.org/#spec-item-11),
[AppDir specification](https://docs.appimage.org/reference/appdir.html),
[AppImage packaging](https://docs.appimage.org/packaging-guide/manual.html),
[Apple notarization](https://developer.apple.com/documentation/security/customizing-the-notarization-workflow),
[Developer ID](https://developer.apple.com/developer-id/),
[Lightroom catalogs](https://helpx.adobe.com/uk/lightroom-classic/desktop/manage-catalogs-and-files/lightroom-catalog-basics.html),
[Lightroom external editing](https://helpx.adobe.com/lightroom-classic/desktop/work-with-external-editors/open-lightroom-photos-external-applications.html),
[Photoshop nondestructive editing](https://helpx.adobe.com/photoshop/using/nondestructive-editing.html),
and [Affinity layer ordering](https://affinity.help/photo2/English.lproj/pages/LayerOperations/order.html).
Channel policy and live Develop-layer workflow descriptions are this app's design choices, not
claims of exact competitor parity.

Minimal cross-area changes: the engine's General preference boolean/default, Library's Help
entry and host-request flag/menu-row automation registration, and host routing/display of the
shared worker window. Coordinator reference documents appeared during the run (`REVIEW-holistic-fable.md`,
`RESEARCH-raw-decoders-quality.md`, `AUDIT-duplicate-systems-fable.md`); preserved them untouched. No Match Look files were read for implementation or changed.

## Limits / remaining external validation

Developer ID credentials and a native Mac are unavailable in this Linux job; actual certificate
signing, Gatekeeper distribution acceptance and notarized Mac artifacts must run on the configured
Apple Silicon workflow once credentials are supplied. Missing credentials are an expected,
tested skip, not claimed signing proof. Real Linux release/distribution compatibility and Mac
hardware acceptance still belong to the documented release/milestone lanes. No interactive image
performance improvement is claimed by this release-hygiene change.
