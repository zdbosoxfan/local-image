# Frontend milestone 1 — opt-in shell and Layers

> **Historical record.** The browser scripts and legacy interface files this report refers to were removed on 2026-10-06 (last present in `926fdd7`). Current checks are listed in [DEVELOPMENT.md](DEVELOPMENT.md).

Status: working source prototype tested against the real backend, ready for visual/function review, **not approved for cutover**. Default startup remains the legacy interface. This work stops at the first shell command strip and Layers slice; the remaining feature panels have not been migrated.

## Baseline and scope

The checkout began clean at `e42aab6a49f222796dbbc9dc1ebf4211f46e9876`, exactly the architecture review's commit. There were no intervening source changes and no repository or ancestor `AGENTS.md`. The supplied architecture review was read from `C:/Users/Owner/Downloads/architecture-review.md` as evidence; the user's pasted request defined the work. Historical README/QA images were not used as current-interface evidence.

Baseline execution found no failures in the safe script/renderer subset. Initially, project and layer-stack Python suites could not collect because the available runtimes lacked image/backend dependencies. Those blocked attempts were recorded rather than reported as passes. The user then explicitly authorized ordinary project-local dependency installation and actual backend integration. A fresh ignored `.venv` now uses the existing 64-bit Python 3.12.14, supported by the current docs, with all 52 packages resolved from `packaging/requirements-build.txt` and its included `backend/requirements.txt`. The original manifests were followed; historical `installed-requirements.txt` was not used. No global or installed-app environment was changed.

The small implementation sequence was: establish pinned browser/script evidence; extract explicit commands and accepted snapshots; build the opt-in React command strip and Layers controls; validate production assets/CSP, rollback and browser behavior; stop for review. Python image authority, ComfyUI, project serialization and the C# host stay in place.

## What owns what

| Region/boundary | Owner in React mode |
| --- | --- |
| Command strip | React/Fluent: Undo, Redo, Fit, Assets, Inspector, Export, Settings |
| Layers list and its properties/menu | React/Fluent: selection, new retouch layer, visibility, lock, rename, opacity and stack commands |
| Existing application menus, workspaces and contextual tool options | Existing implementation, still visible and functional; shared logical commands |
| Canvas subtree, camera, masks and gesture buffers | Existing persistent canvas controller; no React remount or pointer-state render loop |
| Assets, generation/refine, setup/settings and batch views | Existing implementation; not converted in this milestone |
| Persisted document, pixels, project format and bounded stack history | Existing Python backend |
| Native messaging/pickers/close | Existing single-listener implementation and C# host, exposed by a constrained facade |

`frontend/src/editorController.ts` publishes stable, deeply frozen snapshots through `useSyncExternalStore`. It retains accepted per-document revisions while accepting independent navigation/busy fields. `editorApi.ts` sends authenticated, typed stack operations, serializes writes per document, reads the latest accepted revision at execution, rejects mismatched/stale responses, invalidates queued dependent writes after failure, and never automatically retries a write.

`backend/frontend/migration-bridge.js` is an explicitly temporary adapter after all retained scripts. It exposes commands, subscriptions and the existing canvas/native lifetimes. Browser per-document camera, selection and pending edits remain in their existing maps behind that boundary. This milestone does not claim to have moved all global legacy state into TypeScript. Remaining legacy function wrappers must be retired by feature only after later acceptance gates.

The old Layers panel, rows, opacity field and action popup are removed in React mode, not retained as hidden command proxies. React controls call operations directly. Existing layer menus and pointer-release transforms call those same operations. No additional global keyboard, pointer or native-message listener is installed. The canvas DOM node survives panel toggles and selection changes. Input drafts remain local until commit. Selection history still takes priority over backend stack history; this is not a new unified/unbounded history.

Assets starts closed only in React mode and opens on demand. Other Assets logic remains in place. One dark FluentProvider supplies both React regions through its generated theme class; one nonce-aware Griffel renderer inserts styles. System fonts, the existing `--ui-font`/`--ui-note` density settings, and a 4px spacing scheme keep the slice compact and support the existing Large/200% preference.

## Delivery, dependencies and rollback

Exact direct versions, verified registry metadata, licenses and official references are recorded in [FRONTEND-DEPENDENCIES.md](FRONTEND-DEPENDENCIES.md). `frontend/package-lock.json` pins the resolved graph. Application imports use stable Fluent components; the umbrella package's unused preview exports are not application APIs.

Vite emits one hashed JS file and one hashed CSS file plus `.vite/manifest.json` into `backend/frontend_dist`. No public source maps, remote font files, UI CDN, dev server, Node application backend or SSR are introduced. The test harness's ephemeral Node HTTP fixture is test infrastructure only.

`/remove` remains the production/native entry. Each response receives a fresh nonce and signed browser token; no credentials are embedded in the bundle. Page tokens intentionally have no timeout that could invalidate a long-open picker. Existing in-process token callers remain compatible. The asset route only serves validated manifest-listed hashes within `frontend_dist`, with immutable caching and `nosniff`. The manifest, source files and backend state are not exposed. CSP permits the dedicated same-origin asset path and the nonce; it adds no `unsafe-inline` or `unsafe-eval`.

Legacy CSS is bounded by `@scope (:root) to ([data-react-owned])`, with inner `:root` selectors mapped to `:scope` so root tokens still apply. This was exercised in Edge 154; support in the actual packaged WebView2 runtime remains a release gate.

PyInstaller and the Windows build preflight now require and package the built directory and `THIRD_PARTY_NOTICES.txt`. The notice generator includes full texts for 92 resolved runtime packages. Four npm packages omit their license files; checked-in MIT texts and provenance recover them from each release's registry-reported source commit. End users do not need Node/Vite.

Enable for an approved local test by setting `LOCAL_IMAGE_FRONTEND=react` before starting the existing application/backend. Keep its normal loopback origin and `/remove` path. Stop/restart an already running backend to change modes. **Rollback:** unset the variable or set `LOCAL_IMAGE_FRONTEND=legacy`, then restart. The default is legacy, and no project data conversion is performed by this flag. A missing/invalid React build fails closed with a rollback instruction; it does not expose arbitrary files.

Developer build (Node 22.23.2 was tested):

```powershell
Set-Location frontend
npm.cmd ci --ignore-scripts --no-audit --no-fund
npm.cmd run build
npm.cmd test
```

The repository root now also pins the documented Playwright `1.62.1` in its own `package.json` and lockfile; `npm.cmd install --ignore-scripts --no-audit --no-fund` confirmed the existing local installation. Browser tests use installed Edge and do not download browser binaries. On this machine, npm's subprocess PATH initially failed because the inherited PATH contains a stray quotation mark. The executed build used this process-local repair; no system setting was changed:

```powershell
$env:Path = 'C:\Users\Owner\AppData\Local\hermes\node;' + $env:Path.Replace('"','')
npm.cmd run build
npm.cmd test
```

The dependency fetch executed `npm.cmd install --ignore-scripts --no-audit --no-fund` in `frontend/`. The initial sandbox network attempt made no progress and was stopped; the approved network execution added 112 packages. No package lifecycle scripts or system installers ran.

## Executed checks and evidence limits

All paths below are relative to the repository; the Python executable used was `C:/Users/Owner/.cache/codex-runtimes/codex-primary-runtime/dependencies/python/python.exe` (3.12.14).

| Exact command | Result/evidence |
| --- | --- |
| `node tests/test_ui_navigation.cjs` | PASS, editor VM: camera/selection navigation, input, save choices and errors |
| `node tests/test_ui_projects_layers.cjs` | PASS, editor VM: serialization, stale/failure handling, Save As and cancelled multi-document close |
| `node tests/test_ui_native_dialog.cjs` | PASS, fake-clock/native mocks: ten dialog actions survive >10 minutes; cancellation/correlation and nine machine deadlines |
| `node tests/test_ui_generation_size.cjs --unit` | PASS, 270 mixed-limit cases plus geometry assertions |
| `node tests/test_ui_migration_bridge.cjs` | PASS, immutable/coarse snapshots, direct commands, persistent canvas, native facade, hidden-history guards, inspector and stale response handling |
| `python tests/test_frontend_render.py` on baseline | PASS, 3 original tests |
| `python tests/test_app_paths.py` | PASS, 7 tests with isolated state |
| `python -m unittest discover -s tests -p test_frontend_render.py -v` after changes | PASS, 13 renderer/manifest/token tests |
| `python tests/test_layer_projects.py` on baseline runtime | Initially BLOCKED by missing `tifffile`; resolved by the subsequently authorized `.venv` setup below |
| `python tests/test_layer_stack.py` on baseline runtime | Same initial dependency blocker; subsequently passes in the project environment |
| `npm.cmd run build` in `frontend/` | PASS, strict TypeScript, Vite production build and license-notice generation |
| `npm.cmd test` in `frontend/` | PASS, 7 API/controller tests: revision ordering, independent documents, zero opacity, conflict invalidation, failures, no retries, stale responses and immutable navigation snapshots |
| `npm.cmd ls --depth=0` in `frontend/` | PASS, all nine direct dependencies at exact requested versions |
| `node tests/test_ui_migration.cjs --baseline` | PASS, actual Edge with Python-composed assets pinned to `e42aab6`; 6 captures |
| `node tests/test_ui_migration.cjs --legacy` | PASS, current-source legacy rollback page; 6 captures |
| `node tests/test_ui_migration.cjs` | PASS, 11 groups and 8 captures; actual Edge with production module/CSS and controlled HTTP/preview fixtures |
| `node tests/test_ui_react_integration.cjs` with the explicit URL/profile below | PASS, 9 groups against the real source backend, including real CPU editing, project bytes/reopen and decoded PNG export; no API mocks |
| `node --check` on changed legacy scripts; `git diff --check` | PASS; syntax/diff hygiene only |

The **fixture** browser harness blocks all nonfixture network origins. Its responses model commands/revisions; previews are controlled synthetic pixels. It does **not** execute Python compositing, reopen `.lremove`, run native saves, or establish image/export fidelity. It remains useful for targeted failures, stale replies and deterministic before/after layout comparisons. The separate **real-backend** browser harness performs actual image/project/export requests without replacing their responses. Both verify genuine browser DOM, style/CSP, focus and retained-canvas behavior. Their reports identify the dirty source baseline plus tested source/bundle SHA-256 hashes, explicit pass/failure status and whether files changed during the run.

Three genuine UI failures were caught and fixed during integration: scoped `:root` tokens initially disappeared and collapsed the canvas; React text initially failed to follow Large/200%; a rejected opacity write left its unaccepted value in the field. Browser tests now verify these corrections.

Captured screenshots and machine-readable results are local QA artifacts under:

- `qa-artifacts/migration/before/`: pinned source, empty and transformed fixture at 800×560, 1366×768, 1920×1080.
- `qa-artifacts/migration/rollback/`: current legacy mode at those sizes.
- `qa-artifacts/migration/after/`: React mode at those sizes, plus Large/200% and DPR2 captures.
- `qa-artifacts/migration/real-backend/`: actual backend editing, original PNG, downloaded/reopened `.lremove`, alpha PNG export, persisted snapshots and real reopened captures at all three sizes.

The fixture is a reproducible 4096×2732 still life drawn by the test itself, with a transformed cutout, retouch layer and locked original. It uses no user photos, provider calls or model jobs. Baseline page-ready times were 65–136ms empty and 250–291ms with the fixture; a 20-move drag took 333ms including browser-driver and HTTP-fixture overhead. Baseline renderer JS heap was 8,653,812 bytes. After-run measurements are in its results file. These single runs are not comparable production performance guarantees: the React run performs additional interactions, and JS heap excludes decoded image/GPU/total-process memory. **No performance budget is set from these fixture numbers.**

## Real backend setup and integration

`qa-artifacts/integration/environment.log` records setup commands, requirement-file hashes, all resolved versions and successful import/compatibility checks. The environment was created with `uv venv --python <supported-existing-python> --no-python-downloads .venv` and populated with `uv pip install --python .venv/Scripts/python.exe --only-binary :all: --default-index https://pypi.org/simple -r packaging/requirements-build.txt`. This follows the manifests' exact pins and constraints; unconstrained versions are recorded as run evidence rather than silently presented as a new project lockfile.

The backend uses `qa-artifacts/integration/react-profile` and port **51276**. Runtime identity/profile are checked before any integration write. A pre-existing service on 51274 belonged to another profile and was left untouched. ComfyUI is deliberately pointed at disconnected loopback port 51999 for these CPU tests. No user's application profile, settings or image library is used.

```powershell
# From the repository root; refuses to start if its test port is occupied.
.\.venv\Scripts\python.exe tests\helpers\start_integration_backend.py
$env:LOCAL_REMOVE_TEST_URL = 'http://127.0.0.1:51276'
$env:MIGRATION_REAL_PROFILE = Join-Path (Get-Location) 'qa-artifacts\integration\react-profile'
node tests\test_ui_react_integration.cjs
```

The first PowerShell `Start-Process` attempt failed on duplicate inherited `Path`/`PATH` keys. The checked-in helper starts the child with deduplicated process-local environment keys and no visible console. Its PID/profile and stdout/stderr are recorded under `qa-artifacts/integration`. This is the real uvicorn/FastAPI application, not the fixture HTTP server.

The real browser test uploads a deterministic image, runs **two real CPU Quick Heals**, edits the resulting repair layer through React, creates/refines a manual alpha mask, moves/reorders layers, exercises stack Undo/Redo and selection undo priority, downloads a v3 project, reopens it and compares complete layer/repair arrays. It verifies original archive bytes exactly. Its actual PNG export is decoded and checked for 640×480 size, opaque subject pixels and transparent background. Cancel close preserves the reopened session. The run has no API mocks, no page errors, no CSP violations and no live-provider or model requests. A browser project download is still not native-save confirmation.

The following suites ran in separate `.venv` processes using `python tests/<file> -v`. Summary: **193 tests, 192 passed, 1 expected skip, zero failures**:

| Suite | Tests |
| --- | ---: |
| `test_app_paths.py` | 7 |
| `test_frontend_render.py` | 13 |
| `test_layer_projects.py` | 32 |
| `test_layer_stack.py` | 14 |
| `test_cutout.py` | 26 |
| `test_batch_tools.py` | 23 |
| `texture/test_backend_texture.py` | 22 |
| `texture/test_fast_inpaint.py` | 8, including the one skip |
| `test_generation_library.py` | 11 |
| `test_stock_integration.py` | 9 |
| `test_image_generation.py` | 20 |
| `test_image_upscale.py` | 8 |

These suites execute real CPU compositing, source/save/project/export, native 16-bit TIFF samples, ICC, alpha, explicit 8-bit conversion and transparent-JPEG rejection in temporary directories; their GPU/model/provider boundaries use existing controlled mocks. The bundled native texture helper ran successfully. The skip requires a private historical photo fixture. A new explicit v1 archive test verifies reopen/resave with exact original/repair asset bytes; existing v2/v3 regressions pass.

Exact output is in `qa-artifacts/integration/backend-tests.log` and `backend-tests-summary.json`. An initial test-runner mistake set both profile environment variables, overriding test fixtures' temporary-directory selection. Its failures were preserved in `backend-tests-invalid-profile-attempt.log`; correcting the runner isolation produced the results above without changing application code.

`tests/test_http_precision.py` also passes one real HTTP end-to-end test with ten recorded checkpoints against this verified profile. It uses the isolated profile's launcher credential privately to exercise native-auth endpoints. It checks every 16-bit RGBA sample (including partial/zero alpha) and ICC bytes after TIFF export, exact explicit 8-bit PNG conversion with alpha/ICC, transparent-JPEG rejection, Save/subsequent Save/Save As, v3 reopen with exact original bytes, unique-name collisions, successful source overwrite, conflict rejection after an external source change and a unique rescue copy. Outputs and SHA-256 hashes are in `qa-artifacts/integration/http-precision/results.json`. This establishes real HTTP/backend file behavior; it simulates the native endpoint caller and does **not** prove a Windows picker or native-dialog save succeeded.

## Current-source package build

The current tree was compiled with the existing `packaging/local-remove.spec` using PyInstaller 6.22.3 into `dist/frontend-milestone1/package/backend/LocalRemoveBackend.exe`. The C# host was compiled into the same fresh package with `desktop/Build-NativeHost.ps1`. Its official WebView2 SDK 1.0.4191.47 NuGet archive matched the documented SHA-256 before extraction. Neither historical `dist` executable was used as evidence for this build, and no installer was built or executed.

The packaged frontend manifest, JS/CSS, notices and relevant HTML/bridge/scripts were checked byte-for-byte against the source build. The packaged Python archive contains the new token/renderer modules. Exact commands and inspected hashes are in `qa-artifacts/integration/package-build.log` and `package-files.json`.

The build warned that optional vendor codec DLLs `jxs.dll`, `jetraw.dll` and `dpcore.dll` are absent from the resolved imagecodecs package. Normal tested formats pass in the source environment; the build is not a claim of support for those specialized codecs.

The newly compiled host's `--self-test` exited 0: 15 trusted-origin checks, 7 project boundaries, 7 close-handshake checks, 6 setup boundaries and 3 batch boundaries, with installed WebView2 runtime `154.0.4258.37` on x64. Result: `qa-artifacts/integration/native-self-test.json`. This is real native code execution but does not initialize a full editing window or exercise picker dialogs.

The current host's `--probe-webview` then loaded `http://127.0.0.1:51247/remove` in actual WebView2 and reported the expected title. Its packaged backend used a separate verified `qa-artifacts/integration/packaged-profile`. This probe observes navigation completion/title; its hardcoded bridge description is not evidence that every bridge operation ran. The first test wrapper inherited an output pipe and timed out after the host had already written its successful probe result; `packaged-probe-bookkeeping.json` preserves that distinction. The backend later stopped normally at its 75-second idle deadline. It was restarted with a bounded authenticated heartbeat for the following actual package tests.

Installed Edge against **that packaged Python backend** passed the same nine real editing/project/export groups, with 133 successful HTTP responses and no page/CSP/blocked-request errors. It checked served JS/CSS hashes against the built assets. Evidence: `qa-artifacts/migration/packaged-backend/results.json` and four captures. The actual packaged-backend HTTP precision test also passed all ten checkpoints, including every tested 16-bit sample and ICC value, PNG conversion, save/reopen and source conflicts; see `qa-artifacts/integration/http-precision-packaged/results.json`. The optional JPEG XS/Jetraw warnings did not affect the tested TIFF/deflate and PNG paths.

After those checks, the test runtime's identity/profile was verified again and only that packaged backend was shut down through its authenticated endpoint; port 51247 closed normally. The source prototype at port 51276 remains available for review. Packaged-backend testing in Edge plus a separate native startup probe does not equal full native WebView2 interaction acceptance.

Final packaging hygiene: Git attributes preserve LF bytes for hashed text assets on Windows. The notice generator's final blank line was normalized after the functional runs, then the PyInstaller collection was rebuilt (`package-final-notices-build.log`). This changes only notice-file whitespace; the tested JS/CSS hashes and all application source remain identical. The reports retain the exact earlier notice hash instead of rewriting historical evidence.

## Remaining gates before broad migration or cutover

User visual/function approval of this slice is still required. It is an opt-in implementation, not a production replacement. The full current-interface baseline for assets, generation/refine, settings and batch also remains to be captured against a real backend before those features migrate.

The source backend suites and real-browser workflow now provide executed evidence for the named image/project regressions. Full current-interface baselines and cross-feature integration remain necessary before migrating assets, generation/refine or batch. No backend image engine or project format was rewritten.

Real packaged Windows/WebView2 acceptance remains mandatory for trusted-origin delivery offline, actual DPI and Windows Large-text settings, screen readers/high contrast, native Open Files/Folder/Project, Explorer drop, Capture One handoff, Save/Save As/overwrite conflicts/unique copies/exports, downloads, long-open pickers and cancelled multi-document close. Chromium DPR2 and mock native tests do not certify these.

No installer, live provider call, GPU/model job, model download or publishing was performed. Only the explicitly authorized isolated source/package test services were started. No native allowlist was broadened. The next action is prototype review and the remaining packaged/native gates, not migration of other panels or removal of legacy wrappers/CSS.

## Changed-file map

- `frontend/package.json`, `package-lock.json`, `tsconfig.json`, `vite.config.ts`: exact dependencies and build.
- `.gitattributes`: stable generated-text bytes across Windows checkouts.
- `frontend/src/{contracts,editorApi,editorController,App,main}.ts*`, `styles.css`, `vite-env.d.ts`: typed boundaries and the two React regions.
- `frontend/tests/*`, `frontend/scripts/licenses.mjs`, `frontend/licenses/*`: contract tests and reproducible notices.
- `backend/frontend/{editor,layers-studio,studio-shell,stock-studio,migration-bridge}.js`: guarded ownership handoff and retained operation/canvas/native adapters.
- `backend/local_remove_frontend.py`, `frontend_tokens.py`, restricted delivery changes in `local_remove.py`: manifest rendering, nonce/token and asset route.
- `backend/frontend_dist/`: production assets, manifest and notices to ship.
- `packaging/local-remove.spec`, `packaging/Build-Windows.ps1`: deliberate packaging contract.
- Root `package.json`/`package-lock.json`: exact documented Playwright development dependency.
- `tests/test_frontend_render.py`, `test_layer_projects.py`, `test_http_precision.py`, `test_ui_migration.cjs`, `test_ui_migration_bridge.cjs`, `test_ui_react_integration.cjs`, `tests/helpers/start_integration_backend.py`: retained and extended acceptance and isolated startup.
- This report and `docs/FRONTEND-DEPENDENCIES.md`: decisions, commands, results and open gates.
