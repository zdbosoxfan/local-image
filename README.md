# Local Remove

Private backup of the Local Remove code and installed desktop application from this PC, captured on September 21, 2026 (America/New_York).

## Contents

- `backend/`: the installed Python backend, web editor, workflow, dependency lists, startup script, and texture-synthesis helper.
- `desktop/`: the latest native Windows host source, build and update scripts, icon artwork, editor source snapshot, and existing regression tests.
- `desktop/native-host/`: the current installed executable and its WebView2 support files.
- `tests/`: additional existing texture-repair tests and shared test helpers.
- `BACKUP-MANIFEST.json`: original local paths, file sizes, and SHA-256 hashes for every copied file.

The backend editor files and desktop executable were checked against the latest `local-remove-v5` development output and match byte for byte. All copied files were checked against their original files using SHA-256. The original application folders were left unchanged.

## Restoring or developing

The original backend is in `C:\Users\Owner\Documents\RapidRAW-AI-Connector`; the installed desktop app is in `C:\Users\Owner\Documents\Local Remove`. The manifest records the development source locations as well. Source files and scripts are preserved unchanged, including their original local paths. Review those paths before using the scripts on another machine. Some development and test scripts expect the original sibling version folders or installed backend; the manifest identifies where their supporting files came from.

The backend requires a Python environment with the packages in `backend/requirements.txt`; `backend/installed-requirements.txt` records the installed package versions. Its local `.env` must be recreated with the appropriate `HOST`, `PORT`, `COMFY_HOST`, `COMFY_PORT`, `CACHE_DIR`, and `WORKFLOW_FILE` settings. The startup script also creates log folders and starts the separately installed ComfyUI services.

See `desktop/NativeHost-README.md` and `desktop/Build-NativeHost.ps1` for native build requirements, including the pinned Microsoft WebView2 SDK. AI removal also relies on the separately installed ComfyUI environment and model files named in the backend code. Those installations and downloaded models are outside this code backup.

Local `.env` files, launcher credentials, photo sessions, editable photo projects, browser profiles, logs, caches, virtual environments, old backup folders, and generated test images are excluded. Reinstall dependencies and recreate local settings when restoring; this repository is a code and application-file backup, not a full backup of the PC or photo library.

## Existing notices

Existing upstream license and attribution files are preserved with their components. No new project license or `.gitignore` was added.
