# Build and test Local Remove

End users should install the Windows release. These instructions are for working on the source code.

## Development environment

Use 64-bit Windows, Python 3.13, and Node.js for the editor tests. From the repository root:

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

The installed host starts `backend/LocalRemoveBackend.exe` beside its own executable. That packaged backend automatically exits after 75 seconds without a desktop heartbeat, once any active AI generation has finished. The service is bound to loopback and its native management endpoints require the per-user launcher credential.

## Build the installer

Install the [Inno Setup compiler](https://jrsoftware.org/isdl.php). The initial release was built with Inno Setup 6.4.3 and PyInstaller 6.22.3. PyInstaller bundles the interpreter and application dependencies, so users do not need Python; see its [bundle documentation](https://pyinstaller.org/en/stable/operating-mode.html).

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
  -InnoCompiler 'C:\Program Files (x86)\Inno Setup 6\ISCC.exe'
```

The build produces `dist/package/` and `dist/installer/Local-Remove-Setup-0.2.0.exe`, plus a SHA-256 checksum. The build downloads Microsoft's WebView2 bootstrapper and verifies its Microsoft signature. Setup invokes it only if the WebView2 Runtime is missing, following [Microsoft's deployment guidance](https://learn.microsoft.com/en-us/microsoft-edge/webview2/concepts/distribution).

Build artifacts are ignored by Git. Publish the installer and checksum as GitHub release assets. Production distribution should use a code-signing certificate; the first preview is unsigned.

## Tests

Run from the repository root:

```powershell
.\.venv\Scripts\python.exe tests\test_app_paths.py
.\.venv\Scripts\python.exe tests\test_layer_projects.py
.\.venv\Scripts\python.exe tests\texture\test_backend_texture.py
.\.venv\Scripts\python.exe tests\texture\test_fast_inpaint.py
node tests\test_ui_navigation.cjs
node tests\test_ui_projects_layers.cjs
```

The Python tests create isolated temporary data. An optional historical photo comparison is skipped when its private fixture is absent.

For a packaged smoke test, set an isolated data folder, start the native host with `--no-open`, then run:

```powershell
.\.venv\Scripts\python.exe tests\smoke_installed.py $env:LOCAL_REMOVE_DATA_DIR
```

The smoke test uses a generated 16-bit TIFF, runs both healing modes, saves an editable project, verifies the original is unchanged, and checks that unauthenticated runtime changes are rejected. It needs a running test backend and writes only beneath the specified test profile. Run it promptly after starting the host or keep a desktop window open to renew the service heartbeat.

## Runtime layout

`backend/app_paths.py` owns the user data layout. Code and bundled resources are read from the installation; mutable workflow settings are copied into AppData once and preserved across updates. Recovery sessions are separate from disposable caches. `LOCAL_REMOVE_DATA_DIR` is a development override; normal installations use the current user's `%LOCALAPPDATA%\Local Remove`.

The native host reads its packaged backend relative to its executable. It stores its WebView2 profile and logs in the same AppData root and verifies the backend's application identity, version, and profile before reading its credential. No user-specific development paths are needed at runtime.
