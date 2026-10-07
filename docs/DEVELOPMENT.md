# Build and test Local Image

End users should install a bundled [Windows release](INSTALLATION.md) or [Linux preview](LINUX-INSTALLATION.md). These instructions cover source development, including Retouch, Cutout, Image Gen and deployment. The Windows packaging instructions below describe 0.7.0; the Linux preview is 0.7.2-linux-preview. Qwen graphs, models, and licensing are covered in [Qwen Image 2.1](QWEN-IMAGE-21.md).

## Development environment

Use 64-bit Windows, Python 3.12 or 3.13, and Node.js for the editor tests. From the repository root:

```powershell
python -m venv .venv
.\.venv\Scripts\python.exe -m pip install -r packaging\requirements-build.txt
```

The application requirements include `tifffile` and `imagecodecs` for TIFF support. `backend/installed-requirements.txt` is a historical record from the original backup, not the current dependency lockfile.

For browser development, start the server directly:

```powershell
$env:LOCAL_IMAGE_DATA_DIR = Join-Path $env:TEMP 'Local Image Development'
Set-Location .\backend
..\.venv\Scripts\python.exe -m uvicorn main:app --host 127.0.0.1 --port 51247
```

Open `http://127.0.0.1:51247/remove`. This development mode stays running until Ctrl+C. Native file access and native save dialogs require the desktop host. Set `LOCAL_IMAGE_DATA_DIR` on both the host and backend when testing an isolated profile. Do not run two profiles on the same port simultaneously.

The installed host starts `backend/LocalImageBackend.exe` beside its own executable. That packaged backend automatically exits after 75 seconds without a desktop heartbeat, once any active AI generation or setup job has finished. The service is bound to loopback and its native management endpoints require the per-user launcher credential.

## Build the installer

Install the [Inno Setup compiler](https://jrsoftware.org/isdl.php). The current build uses Inno Setup **6.7.3** and PyInstaller **6.22.3**. The example uses `dist/tools/inno/ISCC.exe`; it is an ignored build tool, so a fresh checkout must supply its own compiler. PyInstaller bundles the interpreter and application dependencies, so users do not need Python; see its [bundle documentation](https://pyinstaller.org/en/stable/operating-mode.html).

Download **Microsoft.Web.WebView2 1.0.4191.47** from NuGet and extract the package into `desktop/webview2-sdk/package/`. The original package SHA-256 is:

```text
F492BBF547D0DA329553B6727435B677579B1E9F91CC9E4A1AD029366D5F23D0
```

The native build uses the .NET Framework compiler at `%WINDIR%\Microsoft.NET\Framework64\v4.0.30319\csc.exe`. It requires .NET Framework 4.6.2 or later.

Run from the repository root, replacing the compiler path if necessary:

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\packaging\Build-Windows.ps1 `
  -Python .\.venv\Scripts\python.exe `
  -WebView2Package .\desktop\webview2-sdk\package `
  -InnoCompiler .\dist\tools\inno\ISCC.exe `
  -OutputDirectory .\dist\local-image-v07
```

With that output argument, the build produces `dist/local-image-v07/package/` and `dist/local-image-v07/installer/Local-Image-Setup-0.7.0.exe`, plus a SHA-256 checksum. The build derives the maximum application-folder length from its bundled file and directory paths, leaving room below Windows' path limits. The build downloads Microsoft's WebView2 bootstrapper and verifies its Microsoft signature. Setup invokes it only if the WebView2 Runtime is missing, following [Microsoft's deployment guidance](https://learn.microsoft.com/en-us/microsoft-edge/webview2/concepts/distribution). This bootstrapper needs internet access on a PC without the runtime; the package does not claim fully offline prerequisite installation.

Build artifacts are ignored by Git. Publish the installer and checksum as GitHub release assets. Production distribution should use a code-signing certificate; the first preview is unsigned.

## Linux source and release builds

For source development on an x86-64 Linux desktop, create a Python environment and build the pinned React assets from the repository root:

```sh
python3 -m venv .venv
.venv/bin/python -m pip install -r backend/requirements.txt -r desktop/linux/requirements.txt
npm --prefix frontend ci --ignore-scripts
npm --prefix frontend run build
.venv/bin/python desktop/linux/local_image.py
```

Python and Node.js are build/development requirements; the [Linux release](LINUX-INSTALLATION.md) bundles its own Python interpreter, Qt libraries, backend and compiled frontend. ComfyUI remains a separate optional environment. The source host uses native Qt file dialogs and the same React/Fluent interface as Windows.

Use `LOCAL_IMAGE_DATA_DIR` or the compatible `LOCAL_REMOVE_DATA_DIR` override for an isolated development profile, setting it on the host and backend consistently. Otherwise Linux stores data in `$XDG_DATA_HOME/local-image`, falling back to `~/.local/share/local-image`. Do not launch two backend profiles on port 51247 simultaneously. `.venv/bin/python desktop/linux/local_image.py --smoke-test` creates its own disposable profile, opens the actual React interface, checks the native bridge and exits; it does not load user documents or run AI.

Install `packaging/requirements-linux-build.txt` into the build environment, then run the release builder from the repository root on Ubuntu 24.04:

```sh
.venv/bin/python -m pip install -r packaging/requirements-linux-build.txt
bash packaging/Build-Linux.sh --python .venv/bin/python --version 0.7.2-linux-preview --output dist/linux
```

It produces `Local-Image-0.7.2-linux-preview-linux-x86_64.tar.gz`, the matching `.deb` and `SHA256SUMS` under the output directory. The Linux release uses separate PyInstaller one-folder bundles for the host and backend. Ship the whole directory, including Qt plugins, WebEngine resources and library symlinks. The static Texture helper is included in the Linux package. Normal launch must find the packaged backend relative to the host without a developer virtual environment or current-working-directory assumption. Retain `licenses/Local-Image-LICENSE.txt`, `THIRD_PARTY_NOTICES.md`, `React-THIRD_PARTY_NOTICES.txt` and dependency license resources in the package. See [the Linux host](../desktop/linux/README.md) for its persistent profile and native bridge.

Build release binaries on the oldest supported system, currently **Ubuntu 24.04 x86-64/glibc 2.39**. PyInstaller bundles Python and application dependencies but [does not bundle glibc](https://pyinstaller.org/en/stable/usage.html#making-gnu-linux-apps-forward-compatible); building on a newer distribution can raise the runtime requirement. Qt's [Linux dependency reference](https://doc.qt.io/qt-6/linux-requirements.html) distinguishes runtime libraries from development headers. Validate the frozen binaries on the target desktop rather than inferring compatibility from a successful source run.

The preview distributes a tar archive with a per-user installer and an Ubuntu `.deb`. The tar format preserves the one-folder bundle's symlinks and executable permissions and needs no AppImage/FUSE runtime. These are packaging choices for this app; [Qt documents multiple supported deployment approaches](https://doc.qt.io/qtforpython-6/deployment/index.html), including its own deployment tool and third-party freezers.

## Tests

### Current full React migration checks

The current mounted frontend and its evidence are described in
[Full React migration status](FRONTEND-FULL-MIGRATION.md). From `frontend/`, run
`npm.cmd test` and `npm.cmd run typecheck` using the pinned dependencies, and
`npm.cmd run format:check` (Prettier) before committing; `npm.cmd run format`
applies the house style. The recorded migration unit sets passed **155 frontend
tests and 122 focused backend tests, without skips**; see
`qa-artifacts/migration/final-regression/`. Retiring the legacy interface removed
the ten frontend tests that covered only its bridges and the disconnected
first-stage `App.tsx`/`editorController.ts`/`editorApi.ts`, so the current
frontend set is 145 tests. Interface conventions are in
[Design system](DESIGN-SYSTEM.md).

For current browser acceptance, build the frontend, select React mode before
starting an authorized isolated backend, and set `LOCAL_REMOVE_TEST_URL` and
`MIGRATION_REAL_PROFILE` to that backend's verified URL/profile. Install the
root's pinned Playwright dependencies with `npm.cmd ci --ignore-scripts` only
when local test dependency installation is authorized. Run the relevant drivers
from the repository root:

```powershell
node tests\test_ui_full_integration.cjs
node tests\test_ui_full_failures.cjs
node tests\test_ui_react_assets.cjs
node tests\test_ui_react_generation.cjs
node tests\test_ui_react_models.cjs
node tests\test_ui_react_settings.cjs
node tests\test_ui_react_batch.cjs
```

These suites create owned test data, record served asset identity, and distinguish
actual document/backend operations from controlled provider/inference responses.
They do not authorize model/provider execution, and browser runs do not establish
native dialog or Capture One/Explorer GUI acceptance. The batch suites prepare
their cutouts from an editable mask (Layer → Add editable mask, then Erase
selection) so no background-removal model runs; the Remove backgrounds button
is only shown in the Cutout workspace. Keep each report's build
and scope when citing it. The user subsequently approved installing React as the
default; ordinary packaged startup is verified separately from these UI reports.

Native acceptance preparation uses `tests/helpers/native_acceptance.py` and the
test-only `NativeQaWindow.cs` metadata observer. The compiled helper supports
only `capabilities`, owned `windows` metadata and read-only UIA `inspect`.
Executable hash, PID/start-time, QA manifest/path and window-owner checks remain
mandatory. It has no resize, close, invoke, select, expand or text-input command.
Actual Windows UI inputs use the separately authorized Computer Use session;
the helper's process-local DPI awareness only makes its geometry reads accurate
and does not change application windows or global OS scaling.

### Backend and fixture checks

Run from the repository root:

```powershell
.\.venv\Scripts\python.exe tests\test_app_paths.py
.\.venv\Scripts\python.exe tests\test_app_update.py
.\.venv\Scripts\python.exe tests\test_managed_ai.py
.\.venv\Scripts\python.exe tests\test_setup_routes.py
.\.venv\Scripts\python.exe tests\test_frontend_render.py
.\.venv\Scripts\python.exe tests\test_layer_projects.py
.\.venv\Scripts\python.exe tests\test_cutout.py
.\.venv\Scripts\python.exe tests\test_qwen_image.py
.\.venv\Scripts\python.exe tests\test_qwen_setup.py
.\.venv\Scripts\python.exe tests\texture\test_backend_texture.py
.\.venv\Scripts\python.exe tests\texture\test_fast_inpaint.py
node tests\test_canvas_controller.cjs
node tests\test_ui_generation_size.cjs
```

The Python tests create isolated temporary data. An optional historical photo comparison is skipped when its private fixture is absent.

### Application updates

`backend/app_update.py` checks `https://api.github.com/repos/zdbosoxfan/local-image/releases` for the newest
non-draft release that carries this platform's package and its checksum (pre-releases count): on Windows
`Local-Image-Setup-<version>.exe` with its `.sha256` asset, on Linux `Local-Image-<version>-linux-x86_64.deb`
(or the `.tar.gz` where `dpkg` is absent) listed in `SHA256SUMS`. Versions order by their numeric part, so
`0.7.3-linux-preview` is newer than `0.7.2-linux-preview`. `GET /api/local-remove/update` reports status
(including `package`), `?refresh=true` performs a quiet check at most every 15 minutes, `POST …/update/check`
forces one, and `POST …/update/download` streams the package through `managed_ai.download_verified` into the
profile's `updates` folder, publishing it only when size and checksum match. Only the desktop host, with its
launcher credential, can read the package path (`GET …/update/installer`); the page sees `installer_ready`
alone. The host checks the path, name and SHA-256 itself (`desktop/LocalImageLauncher.cs` on Windows,
`verified_update_package` in `desktop/linux/protocol.py` on Linux) and runs the installer, or opens the package
with `xdg-open`, after the window has closed. `tests/test_app_update.py` covers release selection on both
platforms, checksum parsing, the download/verify flow and tamper detection with simulated GitHub responses;
`tests/test_native_protocol.py` covers the Linux `updateInstall` verb.

### Retired legacy interface

The original single-page interface (`backend/local_remove.html`, the
`backend/frontend/*.js`/`*.css` scripts and their transitional bridges) and the
browser suites that drove its globals and control IDs were removed after the
React interface became the default. The React drivers above own those
workflows. Pinned historical excerpts remain under
`tests/fixtures/legacy-frontend-e42/` for the parity oracles. To inspect the
retired sources, use the last revision that contained them:

```powershell
git show 926fdd7f6f8eca009ebea3097d3d3529d35e3bd9:backend/frontend/editor.js
git checkout 926fdd7f6f8eca009ebea3097d3d3529d35e3bd9 -- backend/local_remove.html backend/frontend
```

For a packaged smoke test, set an isolated data folder, start the native host with `--no-open`, then run:

```powershell
.\.venv\Scripts\python.exe tests\smoke_installed.py $env:LOCAL_REMOVE_DATA_DIR
```

The smoke test uses a generated 16-bit TIFF, runs both healing modes, saves an editable project, verifies the original is unchanged, and checks that unauthenticated runtime changes are rejected. It needs a running test backend and writes only beneath the specified test profile. Run it promptly after starting the host or keep a desktop window open to renew the service heartbeat. When scripting startup, use `Start-Process -PassThru` followed by the returned process's `WaitForExit()`; PowerShell's `Start-Process -Wait` waits for the entire descendant tree, including the idle backend.

The AI setup regressions are `tests/test_managed_ai.py` and `tests/test_setup_routes.py`, which use isolated fixtures; Settings → Local AI in the interface is covered by `tests/test_ui_react_settings.cjs`.

The 0.7.0 additions have targeted Python regressions in `test_batch_tools.py`,
`test_lora_previews.py`, `test_comfy_inventory.py` and `test_operation_progress.py`.

The installer has a plan-only QA mode that emits its real wizard/CLI choices and aborts before installing files or registering associations. To check path validation against a built installer:

```powershell
.\.venv\Scripts\python.exe tests\installer_plan_smoke.py `
  .\dist\local-image-v07\installer\Local-Image-Setup-0.7.0.exe `
  --output .\qa-artifacts\v07\installer-plan-matrix
```

The runner uses `/CURRENTUSER /VERYSILENT /SUPPRESSMSGBOXES /SP- /PLANONLY=1` and a unique report path for each case. It checks recommended, custom, Unicode, protected, relative and invalid locations, with assertions that the application and AI destinations remain uncreated. This matrix does not exercise UAC elevation, installation, uninstall or actual downloads; those require separate acceptance checks. `/PLANONLY` is a diagnostics option, not a normal installation mode.

`tests/smoke_windows_installer.py` provides a separate per-user EXE install/uninstall acceptance test. It first checks HKCU/HKLM registration and skips if an existing registered app could be affected. It also inspects existing legacy shortcut targets and hashes, which must remain unchanged when installing into a separate folder. Use `--check-only` to inspect those conditions without installing. When the conditions are clear, it installs into a unique workspace QA folder, verifies the installed version and Unicode storage preferences, creates sibling storage markers and runs only that QA installation's own uninstaller. It verifies application registrations are removed, markers remain and existing shortcut files have identical hashes and targets. The child processes use an isolated AppData profile, and the test never starts the editor or ComfyUI. Do not bypass its existing-app checks.

For a PC that already has a registered production installation, use the separate
isolated-identity runner:

```powershell
.\.venv\Scripts\python.exe tests\smoke_windows_installer_qa.py `
  --package .\dist\local-image-v07\package `
  --prerequisites .\dist\local-image-v07\prerequisites `
  --compiler .\dist\tools\inno\ISCC.exe `
  --output .\qa-artifacts\v07\qa-installer
```

This compiles the same installer and packaged runtime with unique `AppIdentity`
and `ProjectIdentity` values. Those two registration identifiers are the only
differences from the release build. It snapshots the complete original uninstall,
project association and `.lremove` registry trees in HKCU/HKLM and both registry
views, plus the hashes of existing Local Remove/Image shortcut files. It then
installs per user with `/NOICONS` into a short, unique QA folder, verifies version
the packaged release version and Unicode model/runtime choices, and runs that folder's registered
uninstaller. The final registry and shortcut snapshots must exactly match their
original state, and model, runtime and profile markers must survive. It requires
the normal Windows account and an idle Local Image port; it never launches the
editor, ComfyUI, generation or model downloads. A failed run leaves its evidence
and QA installation reviewable, rather than manually deleting registry entries.

For an explicit real-GPU acceptance run after starting a configured ComfyUI backend:

```powershell
.\.venv\Scripts\python.exe tests\smoke_ai_installed.py "$env:LOCALAPPDATA\Local Remove" .\qa-artifacts\gpu
```

This performs one FLUX removal on a synthetic scene, saves a preview and project, verifies the original, records GPU memory, and closes its own session. It does not start, stop or unload ComfyUI. Use Settings' **Eject model** afterward to test unloading separately.

### Qwen setup and explicit GPU checks

FLUX setup and its smoke test remain supported. Qwen Image 2.1 adds a separate
adapter and optional model download; its Compact INT8 and Full BF16 presets share
the same 7B architecture. Use the desktop Cutout panel's **Model files & download**
control to download the selected variant into the configured model folder.
Both app and command-line downloads use `backend/qwen_download_catalog.py`, which
pins a publisher revision, file sizes, and SHA-256 hashes. Existing model files
are preserved and must match their expected checksums to be reused.

For a manual, explicit model download:

```powershell
.\.venv\Scripts\python.exe scripts\download_qwen.py `
  --models-dir 'C:\path\to\ComfyUI\models' --variant int8
```

Choose `bf16` for the full precision preset or `both` to install both. The app
download API deliberately accepts only one preset per operation. It uses the
native launcher credential, keeps progress in memory, rejects concurrent setup,
and does not restart ComfyUI. See the [Qwen guide](QWEN-IMAGE-21.md) for the required
native nodes, total disk sizes, negative-prompt behavior, and RGBA handling.

After preparing the generated `backpack-cup.png` and `cat-fur.png` test fixtures
in an ignored fixture folder, start an isolated app backend and a configured
ComfyUI, then run:

```powershell
.\.venv\Scripts\python.exe tests\smoke_qwen_live.py `
  --url http://127.0.0.1:51248 --variant int8 `
  --fixture-dir .\qa-artifacts\fixtures --output .\qa-artifacts\qwen\live
```

This is an explicit GPU test, not part of routine unit tests. It exercises object
removal, transparent extraction, generated backgrounds, compositing, shadows,
export, and project persistence. It writes outputs and JSON measurements beneath
the specified output directory and leaves its test sessions available for review.
Repeat with `--variant bf16` only after the full preset is available. Measurements
and visual review of a specific run should be recorded separately from these
instructions; mocked tests do not establish generated-image quality.

The [Qwen Research License](https://github.com/QwenLM/Qwen-Image-2.1/blob/main/LICENSE)
limits the default grant to noncommercial research/evaluation. Commercial use
requires a separate Qwen license. The installer does not bundle model weights.

## Runtime layout

The complete React/TypeScript/Vite/Fluent v9 interface is implemented under
`frontend/src/`. `main.tsx` creates the application and one dark FluentProvider;
`application.ts` wires the explicit document, canvas, native and feature
controllers into `FullApp.tsx`. `backend/frontend/react.html` contains the
persistent canvas subtree and React mounts. The React page executes no legacy
editor/adapter script and loads no legacy stylesheet or hidden legacy controls.
Python remains the image/project authority and the C# host retains native dialogs
and filesystem permissions. See [current ownership](FRONTEND-OWNERSHIP.md).

React is the only interface. `LOCAL_IMAGE_FRONTEND` is ignored, and stale
legacy files left in an old checkout are never loaded. The recoverable
pre-cutover revision is `390f2f4` (`codex/pre-clean-install-20260930`); the last
revision containing the legacy sources is `926fdd7`. [Full migration status](FRONTEND-FULL-MIGRATION.md)
retains the evidence and its limits; [milestone 1](FRONTEND-MILESTONE-1.md)
is historical shell/Layers evidence rather than the current interface scope.

Production/native navigation remains `/remove`. Build the pinned assets and
license notices with `npm.cmd ci --ignore-scripts` and `npm.cmd run build` inside
`frontend/`; `backend/frontend_dist` is deliberately packaged by PyInstaller.
Only manifest-listed hashed assets are served. Every page receives a fresh nonce
and signed browser token; credentials are never baked into bundles. End users
need neither Node nor Vite. The root `package.json` pins Playwright for development
browser tests; use `npm.cmd ci --ignore-scripts` at the repository root when
installing those authorized local test dependencies.

Current frontend controller/API/parity tests run with `npm.cmd test` in
`frontend/` (145 tests). Full-interface browser drivers are
`tests/test_ui_full_integration.cjs`, `tests/test_ui_full_failures.cjs` and the
`tests/test_ui_react_{assets,generation,models,settings,batch}.cjs` feature suites.
They require a verified isolated backend/profile and retain per-build reports.
Layout drivers (`test_ui_brush_layout`, `test_ui_large_density`,
`test_ui_narrow_large_layers`, `test_ui_editor_dialog_layout`,
`test_ui_batch_inspection_layout`, `test_ui_full_performance`) use the same
backend/profile variables.

The current `CIBtEhRP` packaged backend passed 12 real HTTP precision checks;
that test explicitly excludes WebView2 dialog/download/GPU certification. Its
source-browser core/failure rerun passed 12/2 groups with unchanged inventory,
and separate native UI copy/conflict checks are recorded in the evidence index.
Earlier native Layers clipping, Batch preview overlap and Inspector-hide failures
remain recorded along with their repairs. Final candidate `DCWGvea3` /
`DrOsjePE` passed visual inspection of six exact native editor states: 800 x 560,
1366 x 768 and 2195 x 1164 **CSS** viewports, each Comfortable and Large, at
actual 168 DPI/DPR 1.75. Inspector toggle restored canvas height approximately
143 -> 275 -> 143 px; full opacity/transform properties were reached by wheel,
with the saved v3 document identity, revision and saved flags unchanged. The
[review captures](frontend-full-proof/README.md) preserve genuine screenshots
byte-for-byte and distinguish source-browser from native build/state evidence.
No UI layout bugs remain known. The user approved React-default installation
after the migration review. The later Explorer gesture has separate user-performed
and persisted-result evidence; Capture One GUI testing was stopped without a
completed round-trip claim. Real argument-path handoff and native save tests do
not establish Capture One GUI execution. Native
UI actions use Computer Use only; the metadata helper never supplies inputs.
No development command here authorizes installers, model jobs/downloads,
provider traffic or publication.

`qwen_image.py` builds native ComfyUI graphs and preserves transparent results.
`qwen_setup.py` owns model-download routes and progress; `cutout_composite.py`
owns mask refinement, subject placement, background composition, and shadows.
Cutout assets and settings are part of the editable project and recovery session.
The UI keeps Retouch and Cutout in one document and renders a bottom filmstrip
only for a multi-image collection. The Openverse connector searches and imports
Flickr-hosted stock with portable image credits. Pexels and Unsplash remain
external search shortcuts followed by local import.

`batch_tools.py` owns saved treatment backdrops and durable queue snapshots under
the active profile's `batch-tools/`. Prepared queue settings are immutable; resume
uses the same queue and records completed unique exports so an interrupted request
does not publish duplicates. Native folder export accepts IDs through the trusted
bridge and gets the destination from its own picker. Queue cleanup validates owned
files and does not delete source documents, projects or folder exports.

`lora_previews.py` serves bundled exact-adapter examples and bounded publisher
previews through local PNG routes. Compatibility comes from `lora_library.py`
and the exact artifact catalog, not an example image. Bundled gallery images
are test examples intended as product resources; full diagnostic fixtures and
test outputs remain excluded from the release package.

`operation_progress.py` follows the active ComfyUI prompt through its reported
events. It records stages and real sampling counts with a stale-connection fallback;
it does not estimate completion or inject sampling changes. `comfy_inventory.py`
shares bounded loader capability reads among status checks. Explicit refresh
invalidates cached loader choices after model or adapter changes.

`backend/app_paths.py` owns the user data layout. Code and bundled resources are read from the installation; mutable workflow settings are copied into AppData once and preserved across updates. Recovery sessions are separate from disposable caches. `LOCAL_REMOVE_DATA_DIR` is a development override; fresh profiles use the current user's `%LOCALAPPDATA%\Local Image`, while existing Local Remove profiles retain their legacy location. The host and backend must resolve the same root.

The native host reads its packaged backend relative to its executable. It stores its WebView2 profile and logs in the same AppData root and verifies the backend's application identity, version, and profile before reading its credential. No user-specific development paths are needed at runtime.

## Earlier Local Image 0.5.0 additions

That earlier release used `-OutputDirectory dist/local-image`. Use the 0.7.0 output directory above for current builds. The native executable remains `Local Image.exe`. Existing profile directories, native message identifiers, API paths and project extensions retain their earlier names for compatibility.

New Python regressions cover `tests/test_image_generation.py`, `tests/test_z_image.py`, `tests/test_flux2_image.py`, `tests/test_lora_library.py` and `tests/test_hardware_guide.py`. Check actual filenames in `tests/` when running individually. The generation smoke runner is explicit real-GPU work:

```powershell
python tests/smoke_generation_live.py --url http://127.0.0.1:51249 --models qwen z-image-turbo flux2-klein-4b flux2-klein-9b --output qa-artifacts/v05/generation
```

Install each requested preset before running the smoke test. Klein 9B requires publisher-approved download access. The runner generates new documents, exercises each model's image-input route, verifies alpha and dimensions, and exports projects with generation parameters. Qwen also gets a transparent T2I and two-reference test. A pass establishes successful graph execution and file invariants; visual prompt adherence is assessed separately.

Stock interaction in the interface is covered by `tests/test_ui_react_assets.cjs` with controlled responses; `tests/test_stock_library.py` and `tests/test_stock_integration.py` cover the provider and import routes.
