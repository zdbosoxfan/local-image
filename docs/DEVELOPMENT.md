# Build and test Local Image

End users should install the Windows release. These instructions are for working on the **0.6.0** source code, including Retouch, Cutout, Image Gen and deployment to other PCs. User-facing setup is described in [Installation and storage](INSTALLATION.md); Qwen graphs, models, and licensing are covered in [Qwen Image 2.1](QWEN-IMAGE-21.md).

## Development environment

Use 64-bit Windows, Python 3.12 or 3.13, and Node.js for the editor tests. From the repository root:

```powershell
python -m venv .venv
.\.venv\Scripts\python.exe -m pip install -r packaging\requirements-build.txt
```

The application requirements include `tifffile` and `imagecodecs` for TIFF support. `backend/installed-requirements.txt` is a historical record from the original backup, not the current dependency lockfile.

For browser development, start the server directly:

```powershell
$env:LOCAL_REMOVE_DATA_DIR = Join-Path $env:TEMP 'Local Remove Development'
Set-Location .\backend
..\.venv\Scripts\python.exe -m uvicorn main:app --host 127.0.0.1 --port 51247
```

Open `http://127.0.0.1:51247/remove`. This development mode stays running until Ctrl+C. Native file access and native save dialogs require the desktop host. Set `LOCAL_REMOVE_DATA_DIR` on both the host and backend when testing an isolated profile. Do not run two profiles on the same port simultaneously.

The installed host starts `backend/LocalRemoveBackend.exe` beside its own executable. That packaged backend automatically exits after 75 seconds without a desktop heartbeat, once any active AI generation or setup job has finished. The service is bound to loopback and its native management endpoints require the per-user launcher credential.

## Build the installer

Install the [Inno Setup compiler](https://jrsoftware.org/isdl.php). The 0.6.0 build uses Inno Setup **6.7.3** and PyInstaller **6.22.3**. The example uses `dist/tools/inno/ISCC.exe`; it is an ignored build tool, so a fresh checkout must supply its own compiler. PyInstaller bundles the interpreter and application dependencies, so users do not need Python; see its [bundle documentation](https://pyinstaller.org/en/stable/operating-mode.html).

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
  -OutputDirectory .\dist\local-image-v06
```

With that output argument, the build produces `dist/local-image-v06/package/` and `dist/local-image-v06/installer/Local-Image-Setup-0.6.0.exe`, plus a SHA-256 checksum. The build derives the maximum application-folder length from its bundled file and directory paths, leaving room below Windows' path limits. The build downloads Microsoft's WebView2 bootstrapper and verifies its Microsoft signature. Setup invokes it only if the WebView2 Runtime is missing, following [Microsoft's deployment guidance](https://learn.microsoft.com/en-us/microsoft-edge/webview2/concepts/distribution). This bootstrapper needs internet access on a PC without the runtime; the package does not claim fully offline prerequisite installation.

Build artifacts are ignored by Git. Publish the installer and checksum as GitHub release assets. Production distribution should use a code-signing certificate; the first preview is unsigned.

## Tests

Run from the repository root:

```powershell
.\.venv\Scripts\python.exe tests\test_app_paths.py
.\.venv\Scripts\python.exe tests\test_managed_ai.py
.\.venv\Scripts\python.exe tests\test_setup_routes.py
.\.venv\Scripts\python.exe tests\test_frontend_render.py
.\.venv\Scripts\python.exe tests\test_layer_projects.py
.\.venv\Scripts\python.exe tests\test_cutout.py
.\.venv\Scripts\python.exe tests\test_qwen_image.py
.\.venv\Scripts\python.exe tests\test_qwen_setup.py
.\.venv\Scripts\python.exe tests\texture\test_backend_texture.py
.\.venv\Scripts\python.exe tests\texture\test_fast_inpaint.py
node tests\test_ui_navigation.cjs
node tests\test_ui_projects_layers.cjs
```

The Python tests create isolated temporary data. An optional historical photo comparison is skipped when its private fixture is absent.

For browser acceptance, start the development server above with an isolated data
folder, then install the optional test driver and run the browser suite:

```powershell
npm install --no-save --package-lock=false playwright@1.62.1
node tests\test_ui_browser.cjs
node tests\test_ui_setup.cjs
node tests\test_ui_cutout.cjs
```

Set `LOCAL_REMOVE_TEST_URL` to an isolated development server URL when the installed app is already using port 51247. For example, use `http://127.0.0.1:51248` with a separate `LOCAL_REMOVE_DATA_DIR`. The browser test creates synthetic images and checks repair, export, projects, desktop menus, and four desktop window sizes.

The browser suites use installed Microsoft Edge in headless mode. The original suite creates a
synthetic image, applies real local texture repair, compares layers, exports an
image and editable project, and checks keyboard operation and narrow layouts.
Screenshots and downloads go into ignored `qa-artifacts/` (or the directory
passed as the first argument). It does not contact an AI model or use personal
photos. These are automated checks, not human usability-study results.

The Cutout browser suite checks toolset switching, the hidden single-image
filmstrip and bottom multi-image filmstrip, manual alpha refinement, backgrounds,
shadows, editable projects, export, and selected-model availability. It uses
controlled Qwen capability responses and does not execute a GPU model. Python
regressions separately cover native download authorization and integrity, API
graph wiring, alpha handling, mask compositing, and project round trips.

For a packaged smoke test, set an isolated data folder, start the native host with `--no-open`, then run:

```powershell
.\.venv\Scripts\python.exe tests\smoke_installed.py $env:LOCAL_REMOVE_DATA_DIR
```

The smoke test uses a generated 16-bit TIFF, runs both healing modes, saves an editable project, verifies the original is unchanged, and checks that unauthenticated runtime changes are rejected. It needs a running test backend and writes only beneath the specified test profile. Run it promptly after starting the host or keep a desktop window open to renew the service heartbeat. When scripting startup, use `Start-Process -PassThru` followed by the returned process's `WaitForExit()`; PowerShell's `Start-Process -Wait` waits for the entire descendant tree, including the idle backend.

The AI setup regressions are `tests/test_managed_ai.py`, `tests/test_setup_routes.py`, and `tests/test_ui_setup.cjs` (Playwright). The first two use isolated fixtures; the browser suite simulates the native bridge and never installs software.

The installer has a plan-only QA mode that emits its real wizard/CLI choices and aborts before installing files or registering associations. To check path validation against a built installer:

```powershell
.\.venv\Scripts\python.exe tests\installer_plan_smoke.py `
  .\dist\local-image-v06\installer\Local-Image-Setup-0.6.0.exe `
  --output .\qa-artifacts\v06\installer-plan-matrix
```

The runner uses `/CURRENTUSER /VERYSILENT /SUPPRESSMSGBOXES /SP- /PLANONLY=1` and a unique report path for each case. It checks recommended, custom, Unicode, protected, relative and invalid locations, with assertions that the application and AI destinations remain uncreated. This matrix does not exercise UAC elevation, installation, uninstall or actual downloads; those require separate acceptance checks. `/PLANONLY` is a diagnostics option, not a normal installation mode.

`tests/smoke_windows_installer.py` provides a separate per-user EXE install/uninstall acceptance test. It first checks HKCU/HKLM registration and skips if an existing registered app could be affected. It also inspects existing legacy shortcut targets and hashes, which must remain unchanged when installing into a separate folder. Use `--check-only` to inspect those conditions without installing. When the conditions are clear, it installs into a unique workspace QA folder, verifies the installed version and Unicode storage preferences, creates sibling storage markers and runs only that QA installation's own uninstaller. It verifies application registrations are removed, markers remain and existing shortcut files have identical hashes and targets. The child processes use an isolated AppData profile, and the test never starts the editor or ComfyUI. Do not bypass its existing-app checks.

For a PC that already has a registered production installation, use the separate
isolated-identity runner:

```powershell
.\.venv\Scripts\python.exe tests\smoke_windows_installer_qa.py `
  --package .\dist\local-image-v06\package `
  --prerequisites .\dist\local-image-v06\prerequisites `
  --compiler .\dist\tools\inno\ISCC.exe `
  --output .\qa-artifacts\v06\qa-installer
```

This compiles the same installer and packaged runtime with unique `AppIdentity`
and `ProjectIdentity` values. Those two registration identifiers are the only
differences from the release build. It snapshots the complete original uninstall,
project association and `.lremove` registry trees in HKCU/HKLM and both registry
views, plus the hashes of existing Local Remove/Image shortcut files. It then
installs per user with `/NOICONS` into a short, unique QA folder, verifies version
0.6.0 and Unicode model/runtime choices, and runs that folder's registered
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

The editor sources are split between `backend/local_remove.html` (markup),
`backend/frontend/editor.css` (presentation), and `backend/frontend/editor.js`
(interaction). `local_remove_frontend.py` assembles these local resources for each
`/remove` response, with the request nonce and current session token. This keeps
the existing Content Security Policy and desktop trusted-page boundary intact;
there is no frontend build step or remote asset dependency. The installer bundles
the `frontend` directory alongside the markup.

`qwen_image.py` builds native ComfyUI graphs and preserves transparent results.
`qwen_setup.py` owns model-download routes and progress; `cutout_composite.py`
owns mask refinement, subject placement, background composition, and shadows.
Cutout assets and settings are part of the editable project and recovery session.
The UI keeps Retouch and Cutout in one document and renders a bottom filmstrip
only for a multi-image collection. The Openverse connector searches and imports
Flickr-hosted stock with portable image credits. Pexels and Unsplash remain
external search shortcuts followed by local import.

`backend/app_paths.py` owns the user data layout. Code and bundled resources are read from the installation; mutable workflow settings are copied into AppData once and preserved across updates. Recovery sessions are separate from disposable caches. `LOCAL_REMOVE_DATA_DIR` is a development override; fresh profiles use the current user's `%LOCALAPPDATA%\Local Image`, while existing Local Remove profiles retain their legacy location. The host and backend must resolve the same root.

The native host reads its packaged backend relative to its executable. It stores its WebView2 profile and logs in the same AppData root and verifies the backend's application identity, version, and profile before reading its credential. No user-specific development paths are needed at runtime.

## Local Image 0.5.0 additions

Use `-OutputDirectory dist/local-image` for this release to keep the earlier preview build separate. The native executable is now `Local Image.exe`. Existing profile directories, native message identifiers, API paths and project extensions retain their earlier names for compatibility.

New Python regressions cover `tests/test_image_generation.py`, `tests/test_z_image.py`, `tests/test_flux2_image.py`, `tests/test_lora_library.py` and `tests/test_hardware_guide.py`. Check actual filenames in `tests/` when running individually. The generation smoke runner is explicit real-GPU work:

```powershell
python tests/smoke_generation_live.py --url http://127.0.0.1:51249 --models qwen z-image-turbo flux2-klein-4b flux2-klein-9b --output qa-artifacts/v05/generation
```

Install each requested preset before running the smoke test. Klein 9B requires publisher-approved download access. The runner generates new documents, exercises each model's image-input route, verifies alpha and dimensions, and exports projects with generation parameters. Qwen also gets a transparent T2I and two-reference test. A pass establishes successful graph execution and file invariants; visual prompt adherence is assessed separately.

`tests/smoke_stock_browser_live.cjs` explicitly exercises the live Openverse provider through the UI, all three import destinations and project-credit persistence. Set `LOCAL_REMOVE_TEST_URL` to target a source or packaged app. `tests/test_ui_stock.cjs` uses controlled stock responses for repeatable interaction checks.
