# Linux v2 release acceptance matrix

Refreshed read-only on 2026-10-12 against `/home/zdavidson/.codex/worktrees/v2-recovery/Local Image - Testing`, accepted candidate **528b6fd51ebc5dd6b82f63d1838720f58e8f29b1**. This is evidence bookkeeping, not final acceptance. No source changes, Cargo, tests, builds, runtime/process actions, network, push or install were performed; only this matrix was updated. Do not repeat broad gates while owned source handoffs are still pending.

## Current status — scoped milestones, not release completion

- **Installed and last remotely verified checkpoint: 3207abd2b23c6f1f4f93b2e06b25b2079d8a8810.** `interim-v2-push-now.json` records a normal fast-forward push and `ls-remote` verification at 2026-10-12T18:59:50Z; the local cached origin/v2 agrees. No fresh network observation was made in this refresh. `current-test-install.json` records the optimized-dev app/CLI with HEIF, isolated native smoke and clean shutdown. Leave user unit **`local-image-user-test-3207abd2.service`**, normal profile, and rollback **`/home/zdavidson/.local/share/local-image-dev/install-backups/20261012T185907Z`** untouched.
- **9095d440** accepted native clipping/text/CMYK and cropped-mask halo correction: 32 focused mask/clipping + 48 remaining exporter + 373 core library passes, with four existing core opt-ins excluded from pass totals. Final corrected lint passed. These are scoped source-manifest receipts, not a full final-candidate gate.
- **37e0d478** accepted explicit hardware scheduling: physical RTX 5090/Vulkan haze passed; the same compiled binary also passed the numerical entry point with only Lavapipe available, logging llvmpipe/Vulkan/CPU. Software numerical conformance does not count as physical hardware proof. Five physical gates remain REQUIRED final acceptance.
- **528b6fd5** accepted the 20-file portable queue source port: 59 Python regressions, syntax/hash/mode verification, Linux CI step. **No installed runtime changed and automatic dispatch remains disabled.** This source milestone is complete; do not reinstall or repeat its suite merely because later application source changes.
- Actual Rotate View/its eleven physical UI scheduling changes, corrected production PDF image fallback, and remembered New/filter defaults remain owner handoffs, **not accepted milestones**. Remaining original UX/performance/format obligations and final artifact/native/package/upgrade/normal-push/install proof are still open.

Historical provenance: the initial version of this matrix inspected 6acff845 and the interim 6acff845 unit/backup `20261012T121420Z`; those are superseded current-state facts, preserved by `previous-test-install-6acff845.json`. Older cached origin/v2 `97a456c39` was only a local observation, superseded by the later recorded remote verification above. The release-fonts receipt's old `embedded_in_current_candidate: false` predates the stamped interim HEIF builds; neither interim build certifies the future release binary.

## Evidence precedence and state

Status meanings: **complete** = implementation/input or historical evidence actually inspected; **incomplete** = implementation exists but final acceptance remains; **missing** = requested artifact/source/proof absent. Historical passing tests are never credited as exact-final-commit tests.

| Requirement | Status | Actual evidence | Remaining acceptance |
|---|---|---|---|
| Preserve interim user build/session | Complete receipt; preserve untouched | Current 3207abd2 receipt: app SHA-256 `8c543721ce127e0e7e821eec92a5fd2d2e1f179cf5cfc38eb083c1ff3aea9562`; CLI `776e88e082a9d24eef8c2c616267340eb7bafcf7fe206649163fd830eb448e13`; rollback `20261012T185907Z` | Root alone coordinates final replacement after final push; do not stop the current user unit or mutate its backup/profile during development |
| Exact final source freeze | Missing | Accepted candidate 528b6fd5; separate Editor/PDF/remembered-dialog handoffs remain pending; generated oracle/validation output is not release source | Integrate reviewed owned changes selectively, reconcile remote, freeze intended source; record full HEAD and source manifest. Do not count a pending handoff as landed |
| Rust/toolchain/dependency discipline | Incomplete | Local direct launcher pins Rust 1.98.1 and requires `--offline`; final commands below use `--frozen` | Source-freeze gate receipts with unchanged Cargo.lock; CI currently uses floating `dtolnay/rust-toolchain@stable`, no root rust-toolchain file |
| Controlled resources | Complete implementation/historical scheduler proof | Original `parallel-capacity-installed.json`: three lanes/four Cargo jobs/ten slots/two test threads; 44 Python passes in 6.204 s; installed gate SHA `cbed1a19ca3a3f1efa67d4b88b683f73d05a734ced572af8f9d690b65b13709a` | Reconfirm active scheduler hash at gate start; final workspace/release/LTO/GPU/AI runs exclusively; no throughput claim |
| CI rendering prerequisites | Complete source milestones; incomplete current hosted proof | Pinned craft-fonts, Linux Inkscape/Poppler and Windows oracle setup retained. e0f57cc0 hosted Linux failed hardware admission; 37e0 splits haze CI/hardware gates. 9aea6865 fixes Windows cached-template file access; 528b adds queue Python CI step | Fresh hosted results after final UI scheduling integration; Windows fix is locally checked, not a proven Windows rerun. CI uses default features/floating stable, so explicit Linux HEIF/toolchain gate remains |
| Pinned font input | Complete input audit | Re-read font checkout HEAD `3d9ab3a132ed0eff457e424226a435a901da30d3`; all seven hashes match `release-fonts.json`, all licence paths exist | Keep absolute CRAFT_FONTS_DIR + CRAFT_FONTS_REQUIRED=1 for every final build. Build script does not hash fonts itself and REQUIRED alone does not reject an unset DIR; preflight both |
| Font attribution and actual texts | Complete source inventory | Curated/generated attributions name Japanese, SC, Arabic, Thai families; all six family OFLs + craft-fonts notice present; Inter and JetBrains Mono OFLs present | Final archive byte comparison below; actual rendered supported-language controls and shaping inspection in native acceptance |
| Attributions coverage | Complete static inventory, incomplete final regeneration proof | Parsed `assets/attributions.json`: 57 distinct referenced paths, zero missing; every file under licenses and every asset LICENSE/OFL/ATTRIBUTION/NOTICE covered | Final workspace includes xtask freshness tests; if inputs change, regenerate before commit/freeze, then test. Do not use `--fetch` |
| Attribution/package regressions | Complete historical evidence | `release-attributions-package-tests.json`: 22 passed, exit 0, HEAD 258257c73; log hash reverified. Test `linux_package_includes_every_referenced_attribution_text` compares copied notices, including all six font families | These tests recur within final workspace, no duplicate standalone suite needed unless package code changes |
| Font regressions | Complete historical evidence | Reverified log hashes: coverage fixed 3 passes; UI reviewed 28; text reviewed 89 with 3 ignored; accepted lint exit 0, all at 1a84a91c6 | Final workspace + explicit disposition of ignores; font presence tests are not visual shaping/native UI acceptance |
| Final fmt/strict lint/workspace | Missing exact-candidate evidence | Older final-strict-clippy/workspace-check pass at fbf779e8; older final-union exits 101. Not a final gate for current source | Run sequential commands below once after freeze; record counts/ignores, source fingerprint, lock hash, logs and exits |
| Release app + CLI, including HEIF | Incomplete | Interim optimized-dev build includes `local-image/heif,local-image-cli/heif`. `xtask package linux` invokes plain release build without these features | Explicit HEIF release build then `package linux --skip-build`, as below; otherwise packager silently changes shipped capability. Fix canonical packager/CI if official invocation must itself guarantee HEIF |
| Linux tar/checksum/inventory | Missing final artifact | `xtask/src/main.rs` stages only binaries/README/LICENSE/licenses and referenced attribution paths; generates tar.gz and adjacent sha256 | Final archive checks below, app AND CLI versions and hashes, dependency/oracle readiness and native run from staged files |
| Isolated installer upgrade + rollback | Incomplete | `desktop_install.rs`: per-file temp/rename, copies adjacent CLI if present, desktop + eight icons; no settings/library writes. Existing interim receipt says sentinels preserved | Final release upgrade/rollback in private HOME, byte/permission comparisons; test both binaries because missing adjacent CLI is silently tolerated. Installer is not a whole-install transaction |
| Final native workflows / GPU / AI / exit | Scoped receipts reusable; final artifact workflow proof missing | 3207abd2 isolated native smoke; real AI/GPU/RAW receipts listed below, with verified log hashes; no blanket model rerun implied | Review changed paths against retained source/model/fixture receipts, rerun affected flows, then actual final installed-private-artifact startup/edit/save/export/cancel/restart/exit; five required physical gates remain explicit |
| Contract/fallback/ignore disposition | Incomplete overall; accepted clipping/CI milestones | 9095 native clipping/CMYK/halo accepted; 37e0 preserves hardware/numerical assertions with explicit scheduling. Old native-provider ignores in FINAL-GATE-NOTES are stale per AI acceptance inventory | Integrate/validate pending PDF fallback and Editor scheduling; retain all original semantic/fallback controls and identify each remaining opt-in. No blanket ignored suite or exemption from old reason text |
| Portable queue source in v2 | Complete scoped milestone at 528b6fd5 | `portable-queue-integrated.json`: 20 files, 59 tests, source accepted, installed_runtime_changed=false, automatic_dispatch_enabled=false | Preserve reviewed source and held coordinator-only policy; final CI includes same Python suite. No runtime reinstall, launch or repeated source-only suite without relevant changes |
| Remote v2 reconciliation / normal push | Interim checkpoint complete; final push missing | `interim-v2-push-now.json`: 3207abd2 normal fast-forward/remote verified at 18:59:50Z; cached ref agrees. Candidate 528b6fd5 is later | Root fetches again before final freeze/push, preserves all intervening work, revalidates affected integrations and pushes normally. Existing receipt is historical remote evidence, not current network freshness |
| Final local install identity / rollback receipt | Missing | Current receipt is explicitly an interim test, optimized-dev profile | Root installs exact pushed release binaries with verified backup and records installed hashes, version and separate restart smoke |

## Accepted scoped evidence and invalidation rules

Accepted commits are implementation milestones. Receipts may name an earlier HEAD because they tested owned working-tree changes before commit; use their source manifests plus the accepted integration record rather than rewriting provenance to the acceptance commit. Counts below are scoped and must not be summed into a “final suite” total.

| Accepted milestone | Actual receipts | Current interpretation |
|---|---|---|
| Native clipping/text/CMYK/halo 9095d440 | `clipping-cmyk-integrated.json`; candidate `clipping-text-core-affected.json` (373 pass, four opt-ins), `mask-halo-clipping-focused.json` (32), `mask-halo-export-affected.json` (48), `mask-halo-lint-corrected.json` (exit 0) | Completed scoped milestone. `mask-halo-final-lint.json` was an earlier failing attempt, superseded by corrected lint; do not cite it as passing. All original thresholds retained. Production PDF image fallback remains separate and pending. |
| Hosted/physical split 37e0d478 | Candidate `ci-haze-physical.json` (one pass, RTX 5090/Vulkan/DiscreteGpu), `ci-haze-software-vulkan.json` (one pass, forced Lavapipe ICD, same compiled binary SHA `14fa72980ec50b1bba5f57007e66570868c1ff8a88ad5c45ea231acb160bca5e`), `ci-mask-union-lint.json` (exit 0) | Complete admission/numerical proof for this source; actual physical assertion and full 24-case sorted oracle remain. The four other physical bodies were unchanged and not rerun merely for attributes. Final required commands still due. |
| Portable queue 528b6fd5 | Original `portable-queue-integrated.json`; candidate `portable-queue-v2.{json,log}` | 59 scoped Python passes, no runtime installation. Does not complete application release or authorize queue resumption. |

**Reuse retained AI/GPU/RAW proof by affected path, not by whole-commit identity.** Every successful AI receipt listed here has its retained log SHA-256 rechecked during this bookkeeping refresh. Their original HEAD/source/model/fixture/environment receipts remain the authority; this table does not reclassify them as executions at 528b6fd5.

| Retained real receipt under original `recovery/ai-runtime/validation/` | What remains reusable / actual change boundary |
|---|---|
| `ai-upscale-real-5090`, `upscale-seams-real` | CUDA upscale and CPU tiled/whole coverage passed. All four recorded scoped enhance/Comfy source entries checked for the former, and the recorded enhance implementation for the latter, still match. Do not repeat the 907.34 s seam inference solely for clipping, CI attributes or Python-port changes. Rerun affected model/equivalence only if inference, tiling/cancellation, selected weights or relevant backend behavior changes. |
| `denoise-equivalence-real-5090` | Real Linear/Bayer/RGB GPU-vs-tract receipt passed. Of 24 recorded `lc-gpu/src` + `li-seg/src` entries checked, only `remaining_tests.rs` differs (accepted test scheduling/helper extraction); production kernels unchanged in that comparison. This does not invalidate the costly model parity result. Required small graph/device gates below still run separately. |
| `raw-denoise-ui-real-5090` | Production RAW denoise Preview/strength receipt passed. All 47 recorded scoped RAW/denoise/UI-test entries compared still match. Keep model parity and full receipt; final UI changes require the affected interaction check, not automatic model-matrix repetition. |
| `host-raw-switch-real` | Actual RAW host route passed. Of 145 recorded scoped RAW/engine/host entries checked, only `apps/local-image/src/library_host.rs` differs. Recheck changed host/menu/selection flow and final native integration; do not infer RAW decoder or denoiser kernels regressed from host UI changes. |
| `generation-create-edit-cancel-real` | Real Generate/Edit/cancellation receipt passed. Of 35 recorded scoped li-ai/bridge/generation-UI entries checked, only `crates/li-ai/src/refine/tests.rs` differs (Windows file-timestamp fixture access, not production inference). Reuse this real model result; pending modal/gesture routing changes need focused cancellation/publication/input acceptance if they touch this route. |
| `ai-restoration-configured-5090`, `restoration-ui-real-5090`, `restore-cpu-real`, `restore-remove-ui-real`, `search-people-real` | Successful real-model/UI/CPU/people receipts retained; read their individual boundaries and source/model manifests before deciding affected final flows. The earlier `ai-restoration-real-5090` exit 101 remains historical failure; configured successful coverage does not erase its provenance or imply all refused models became supported. Later UI/modal work invalidates corresponding interaction evidence, not unchanged model-output quality by default. |

These are bounded recorded-source comparisons, not a claim that every transitive build/runtime dependency was audited. Changed relevant shaders, model files, feature flags, command recipes, alpha/colour conversion, decode or publication paths require corresponding focused evidence. Unchanged dependency edges/test attributes/docs/Python queue code do not by themselves justify blanket expensive inference reruns. Always keep the final packaged-artifact startup, representative workflow and clean exit checks; old development receipts cannot prove final binary packaging or shutdown. No prior test failure or quality limitation is waived.

The missing-RAW statements in the older AI/performance preparation reports are now superseded by `restored-raw-fixture.json`: approved CC0 file4465 is `/home/zdavidson/Documents/Local Image - Testing/tmp/release-fixtures/rawpixls-4465.ARW`, 63,216,896 bytes, SHA-256 `ddb72880da333b46a4c13286f78473dacc11377ea874bc3656e64e30a8122a88`. Actual RAW/denoise/host receipts above used approved inputs. Keep that provenance; no owner-photo substitution or new model download is required by this matrix.

## Required physical final acceptance and pending Editor handoff

The five explicit **REQUIRED** gates below are now committed in `docs/GPU-VALIDATION.md`. Use the existing isolated runner with both `LC_REQUIRE_GPU=1` and `LOCAL_IMAGE_REQUIRE_GPU=1`, Vulkan backend, installed compiler gate, Rust 1.98.1/offline/frozen and one test thread. For each row run `cargo +1.98.1 test --offline --frozen -p PACKAGE TARGET FILTER -- --ignored --exact --nocapture --test-threads=1`; require exactly one pass, logged physical adapter and no unexpected fallback. Do not invoke a whole `--ignored` suite. These small correctness gates do not run the real-model denoiser matrix.

| Package / target | Exact required filter |
|---|---|
| `lightcraft-gpu --lib` | `render::remaining_tests::ds_finding_native_haze_selection_matches_sorted_quantile` |
| `lightcraft-gpu --test denoise` | `denoiser_retains_its_device_across_context_release` |
| `lightcraft-gpu --test denoise` | `ai_denoise_gpu_transpose_and_shuffle` |
| `lightcraft-gpu --test denoise` | `ai_denoise_gpu_conv_pool_skip_and_sigmoid` |
| `photocraft-gpu --test scaled_view` | `scaled_view_rtx5090_vulkan_mixed_masks_clips_effects_tiled_parity` |

The Linux CI numerical-Vulkan haze step is additional conformance coverage, never a substitute for these physical runs. The **eleven Editor UI physical scheduling changes are pending with the actual Rotate View handoff**, not accepted by 37e0 or this refresh. Integrate/review their exact names, preserved bodies and explicit final commands with the Editor owner before claiming ordinary hosted-workspace compatibility. The rendering foundation's historical RTX evidence remains available but does not prove the not-yet-accepted activation/input changes.

## Original performance and remaining release scope

`STAGE4-PERFORMANCE-TASK.md`, `PERFORMANCE-CURRENT-TRIAGE.md` and `PERFORMANCE-EXECUTION-MATRIX.md` remain the full original workstream; no proposal drops out. Retain scoped AI watcher overhead; native Save As dispatch versus total encode/publication/scripts/assets; Navigator generation/publication; 2,000-object swatches; vector projection/Warp/node/selection/view-mode/Live Paint/mesh/Blob/text/shared Pen outliers; measurement-overlay control; fixed-tile Rescue; O040 firstMove/pan/zoom/cold phases; FC001 memory/cache cleanup; and GPU candidates across Develop/masks, compositing, vector/live effects, previews and resampling. Existing watcher/save/Navigator/cache implementations are not proposals to redo blindly.

The ddd5411e RTX5090/Vulkan 6000×4000/500-shape baseline remains **64 samples, median 8.068 ms, p95 8.662 ms, first 91.678 ms, release 8.304 ms, including GPU/readback**. It is a historical baseline, not a current standalone UI latency or speedup claim. First-phase breakdown and remaining controlled interleaved comparisons remain due. Accepted camera foundation and pending rotation/input work mean affected frame/performance acceptance must use the eventual final paths; unaffected expensive model results need not be repeated to profile those paths. Preserve original image/geometry tolerances, CPU fallback and cancellation.

Still open: corrected production PDF image fallback, actual Rotate View/remaining original UX, remembered New/filter defaults, measured performance dispositions, final frozen supported workspace/fmt/strict lint, explicit HEIF app+CLI release build, package/checksum/notice inventory, private install/upgrade/rollback/restart, affected real/native checks and artifact exit, fresh remote reconciliation, normal push and byte-verified final installation. macOS/Windows follow-up scope and Library scaling/full translation priorities remain as previously assigned; this bookkeeping does not expand them. Fresh hosted Windows CI is required before claiming the specific Windows fixture fix passed there.

## Final Linux command plan (do not execute before coordinator freeze)

Use the existing target/cache and the current original direct launcher. It pins 1.98.1, 4 jobs, 2 test threads, installed rustc gate/test runner, isolated settings, font bundle and Vulkan requirements, with `systemd-run --slice=li-jobs.slice`. It invokes candidate `docs/wip/v2-recovery-validation/run.py`, which uses a nonblocking candidate lock and writes source/log hashes. It does **not** reserve exclusivity against unrelated builds; root must establish exclusivity first. The accepted portable source port did not replace the live launcher. Keep both LOCAL_IMAGE_LIBRARY (host) and LIGHTCRAFT_LIBRARY (engine) scoped to the private run catalog; the earlier single-override smoke issue is recorded in current-test-install.json.

The following labels are new proposals; use new labels if already used when executed. Run commands sequentially, stopping at any failure. `fmt` is read-only and does not compile; the direct launcher requires `--offline` and is not suitable for cargo-fmt's argument interface.

```bash
cd '/home/zdavidson/.codex/worktrees/v2-recovery/Local Image - Testing'
export CRAFT_FONTS_DIR='/home/zdavidson/.local/share/local-image-dev/engine-sources/craft-fonts'
export CRAFT_FONTS_REQUIRED=1
export CARGO_NET_OFFLINE=true
export RUSTUP_TOOLCHAIN=1.98.1
export PATH='/home/zdavidson/.rustup/toolchains/1.98.1-x86_64-unknown-linux-gnu/bin:'"$PATH"
export PHOTOCRAFT_BUILD_SHA="$(git rev-parse HEAD)"
export PHOTOCRAFT_BUILD_DATE="$(date -u +%F)"
release_launcher='/home/zdavidson/Documents/Local Image - Testing/scripts/dev-queue/direct-validate.py'
cargo fmt --all -- --check
python3 "$release_launcher" final-v2-rc1-clippy clippy --offline --frozen --workspace --all-targets --features local-image/heif,local-image-cli/heif -- -D warnings
python3 "$release_launcher" final-v2-rc1-workspace test --offline --frozen --workspace --features local-image/heif,local-image-cli/heif --no-fail-fast
python3 "$release_launcher" final-v2-rc1-release build --offline --frozen --release -p local-image -p local-image-cli --features local-image/heif,local-image-cli/heif
python3 "$release_launcher" final-v2-rc1-package run --offline --frozen -p xtask -- package linux --skip-build
```

Record the fmt exit/log independently. The workspace test gate includes xtask; avoid a redundant full check/test cycle. Exact real-model, GPU, native UI and ignored-test commands must come from their owners after the final inventory. The generic runner sets LOCAL_IMAGE_MODELS_DIR to an empty isolated directory and LOCAL_IMAGE_COMFY to `127.0.0.1:9`; it is not a real-model integration launcher. Do not claim AI acceptance from this default environment.

Before these commands, verify font HEAD/manifest/file hashes against `release-fonts.json`, both font variables, scheduler installed/source hashes, Rust 1.98.1, Inkscape `--app-id-tag` support and `pdftoppm -v`. Record available graphics/display conditions. Avoid outputting private control tokens. Commit intended regenerated attributions before gate; do not regenerate after freeze without restarting affected acceptance.

## Exact archive and attribution checks

Expected current artifact: `dist/release/local-image-2.0.0-alpha.1-linux-x86_64.tar.gz` with `.tar.gz.sha256`. Derive version/architecture from final Cargo.toml/host instead of silently assuming if either changes. No archive has been created or accepted by this audit.

After packaging, verify the checksum from its directory, inspect the member list, and compare every regular file against the allowlist below. This checks original bytes rather than trusting filename existence. Run this script only against the newly built final archive:

```bash
python3 - <<'PY'
from pathlib import Path, PurePosixPath
import hashlib, json, subprocess, tarfile, tomllib
root = Path.cwd()
version = tomllib.loads((root/'Cargo.toml').read_text())['workspace']['package']['version']
arch = subprocess.check_output(['uname', '-m'], text=True).strip()
name = f'local-image-{version}-linux-{arch}'
archive = root/'dist/release'/f'{name}.tar.gz'
checksum = archive.with_suffix(archive.suffix+'.sha256').read_text().split()
assert checksum == [hashlib.sha256(archive.read_bytes()).hexdigest(), archive.name]
a = json.loads((root/'assets/attributions.json').read_text())
refs = set(a['app']['licence_files'])
for section in a['sections']:
    for entry in section['entries']:
        refs.update(entry.get('licence_files', []))
expected = {p: root/p for p in refs | {'README.md', 'LICENSE', 'assets/attributions.json'}}
expected.update({p.relative_to(root).as_posix(): p for p in (root/'licenses').rglob('*') if p.is_file()})
expected.update({b: root/'target/release'/b for b in ('local-image', 'local-image-cli')})
with tarfile.open(archive, 'r:gz') as tar:
    files = {}
    seen = set()
    for member in tar.getmembers():
        parts = PurePosixPath(member.name).parts
        assert not member.name.startswith('/') and '..' not in parts and parts[0] == name, member.name
        assert member.name not in seen, member.name
        seen.add(member.name)
        assert member.isdir() or member.isfile(), member.name  # no links/devices
        if member.isfile():
            rel = '/'.join(parts[1:])
            files[rel] = member
    assert set(files) == set(expected), (set(files)-set(expected), set(expected)-set(files))
    for rel, source in expected.items():
        assert tar.extractfile(files[rel]).read() == source.read_bytes(), rel
    for binary in ('local-image', 'local-image-cli'):
        assert files[binary].mode & 0o111, binary
for family in ('biz-ud-pgothic','biz-ud-mincho','shippori-mincho','noto-sans-cjk-sc','noto-sans-arabic','noto-sans-thai'):
    assert f'licenses/OFL-{family}.txt' in refs
assert {'assets/fonts/OFL-Inter.txt','assets/fonts/OFL-JetBrainsMono.txt','licenses/craft-fonts-NOTICE.md'} <= refs
print(json.dumps({'archive': str(archive), 'sha256': checksum[0], 'regular_files': len(expected), 'referenced_notice_files': len(refs)}, indent=2))
PY
```

Then extract the validated archive into a unique private staging directory outside source (never over an existing install), run BOTH `local-image --version` and `local-image-cli --version`, and compare their embedded short SHA to the full frozen/pushed SHA. Compare extracted executable SHA-256 to the release target hashes and save archive, extracted and installed hashes in one receipt. `xtask --skip-build` neither rebuilds nor validates app SHA; its CLI version print is insufficient by itself. Check runtime shared libraries with `ldd` and require no `not found`; actual native smoke remains decisive. Do not archive models, build caches, source runtime profiles, control tokens, `.env`, credentials or logs. The exact-member allowlist above excludes them automatically.

The installer does not copy licence texts to ~/.local; retain the accepted release archive and its notice tree as the distributed artifact. If an installed standalone notice-tree requirement is added, that requires a deliberate installer change, not a claim about existing behavior.

## Isolated upgrade and rollback procedure

Do not change the shell HOME or run `--system`. Use `subprocess.run(..., env={**os.environ, 'HOME': str(private_home), ...})` so only the installer subprocess sees the test HOME. Supply distinct XDG_CONFIG_HOME, XDG_DATA_HOME, XDG_CACHE_HOME, XDG_STATE_HOME, LOCAL_IMAGE_DATA_DIR, LOCAL_IMAGE_LIBRARY, LIGHTCRAFT_LIBRARY and LOCAL_IMAGE_MODELS_DIR under that same private root. Native smoke additionally uses a private loopback control port/token file and automation read/write roots, and closes only its own process.

1. Create a unique acceptance root under `/tmp` or the release staging directory; do not reuse normal user folders. Use the current interim staged pair `/home/zdavidson/.local/share/local-image-dev/test-builds/3207abd2b/local-image` and adjacent CLI after verifying their receipt hashes. Install this pair into the private HOME first; neither source binary nor installed user binary is changed.
2. Place nonempty sentinel files in private settings/library/model locations and separate Legacy/PhotoCraft binary/desktop/icon names. Record hashes. Existing frozen migration fixtures are copied to scratch rather than edited in place.
3. Snapshot the 11 Local Image-owned installed paths, bytes and modes: `.local/bin/local-image`, `.local/bin/local-image-cli`, `.local/share/applications/io.github.zdbosoxfan.LocalImage.desktop`, plus `.local/share/icons/hicolor/{16x16,24x24,32x32,48x48,64x64,128x128,256x256,512x512}/apps/io.github.zdbosoxfan.LocalImage.png`. Record missing paths separately (for first-install rollback).
4. Run final **extracted staged app** `--install` with the private environment and adjacent CLI present. Assert exit 0; both installed hashes equal stage; binary modes 0755, desktop/icons 0644; desktop Exec targets private `.local/bin/local-image`; eight icon PNG signatures and expected bytes; all unrelated sentinels unchanged. Verify both installed versions. Repeat install once to cover idempotent upgrade, without launching normal user profile.
5. Run native acceptance against this installed private pair/profile, including representative document operations and real GPU exit, and restart it once. Use the separate owner-designed native script and review screenshots; no general `pkill`, global systemd stop or user-unit replacement.
6. Restore only the 11 snapshot-owned paths with sibling temporary files and atomic rename, reinstating their saved modes (and remove only paths recorded absent before upgrade). Compare every hash/mode to snapshot; verify old app AND CLI versions and sentinel hashes. Run an old-version restart smoke on its private profile. Record success; retain backup and evidence.
7. On final live installation, root repeats the owned-path backup/final-stage install/hash checks using a **new timestamped** backup. Keep current interim unit untouched during development; final replacement is separately coordinated. Settings/library are outside this file rollback. If final app changes on-disk settings/catalog schemas, assess and preserve a closed/private pre-migration profile snapshot before testing rollback; restoring binaries alone does not prove data rollback.

## Portable dev-tool source port — accepted, installed runtime unchanged

`portable-queue-integrated.json` records accepted commit 528b6fd5, patch SHA-256 `379776186f77d0f90f0133d8fcd7b3a0d7772e278fe2cba454de0d53c7c94203`, 20 source files and 59 passing tests. Candidate `portable-queue-v2.log` records **59 tests in 6.392 s, OK**; its runner receipt exits 0. The earlier 44-test source/installed-capacity evidence remains historical baseline, superseded for this port by the reviewed 59-test run. Twelve Python syntax checks and exact 20-file hashes/modes were recorded. CI now runs the same suite with external private runtime paths.

The accepted port contains the four unchanged core gate/watchdog/state/validation tools; explicit `li_config.py` paths; coordinator-only dispatcher; dashboard; installer; service template; README/rules; existing watchdog/memory regressions and portability tests. The launcher/land/rework/target-sweep/update entry points are fail-closed stubs; no unrestricted launcher, automatic dirty commit/conflict resolution, target/cache deletion, or author-specific runtime trust bootstrap was ported. Installer checks idle/hold/hashes, refuses repository-contained runtime roots (including symlinks), backs up owned existing bytes, and does not activate timers/services. Prior automatic-launch tests now assert refusal and unchanged external state, not relaxed memory/resource behavior.

Do not port jobs.json, board.json, live queue holds, active-worker/direct-validation state, installed hash state, logs/locks/leases/backups/targets, tokens/config/keys or whole local-image-dev directories. Original `direct-validate.py`/recovery launchers remain machine-local. The 3-lane/4-job/10-slot installed tooling remains untouched. No `install.py --apply`, dispatcher launch, service/timer activation or runtime update is part of source acceptance. Re-run the Python suite only for relevant source/config changes or its normal final CI execution, not every unrelated application milestone.

## Remote v2 reconciliation and final identity (root only, later)

1. Inventory candidate source and owned handoffs; preserve unrelated dirty/generated state. Commit only explicit intended source paths (never `git add -A` across recovery artifacts). Record the frozen source tree and full commit SHA.
2. Fetch `origin v2` normally, with no pruning/force/push in this audit. Inspect `git log --left-right --oneline HEAD...origin/v2` and ancestry. If remote is already an ancestor, no merge required. If it advanced, integrate all intervening commits (normal merge is suitable for preserving both histories), resolve conflicts with file owners and freeze the resulting source before final acceptance. No reset/force push; do not assume the recorded/cached 3207abd2 remote checkpoint remains current.
3. Run final gates/artifact/native acceptance on the resulting commit. If fetch was delayed until after acceptance and adds source, old acceptance is no longer exact-candidate proof; rerun affected acceptance and regenerate/stamp the release artifact for the final commit. Prefer reconciling before the expensive final gate.
4. Immediately before normal `git push origin HEAD:v2`, confirm intended source and test receipts refer to that HEAD and no unreviewed source change remains. A push rejection is evidence of new remote work: fetch/integrate/freeze/revalidate; never override it.
5. Verify server v2 resolves to pushed full SHA (root's read-only remote query), then install the byte-identical accepted extracted pair with backup. Record full SHA, source manifest/Cargo.lock/font input hashes, artifact checksum, both stage/installed binary hashes, version strings, backup location, installer sentinel proof and restart smoke. No tag/GitHub release is required.

Prepared report is frozen for coordinator review. Remaining implementation and execution belong to assigned owners/root; no broad gate was repeated here.
