# Local Remove

**A Windows photo editor for local object removal, quick healing, and editable layers.**

Local Remove helps you remove unwanted objects and repair small distractions in photos. Quick Heal runs on your PC's CPU. AI Remove connects to an existing local ComfyUI installation for FLUX Klein or experimental Qwen removal.

The Windows installer includes the application, Python runtime, and healing tools. You do not need ChatGPT, Codex, a source checkout, or a separate Python installation to use it.

## Install

1. Open [Releases](https://github.com/zdbosoxfan/local-remove/releases) and download **Local-Remove-Setup-0.2.0.exe**.
2. Run the installer for your Windows account. It creates Start menu shortcuts and an optional desktop shortcut.
3. Open **Local Remove** from the Start menu.

**Requirements:** Windows 10 or 11, 64-bit. Setup installs Microsoft's WebView2 Runtime if it is missing; that prerequisite download requires an internet connection. The current preview installer is not code-signed.

Quick Heal is included and works without ComfyUI or model downloads. To enable AI Remove, follow the connection steps below.

## Edit a photo

1. Use **File → Open** or **Open Folder**, or drop images from File Explorer into the desktop window.
2. Select the object with the brush, pen, rectangle, or ellipse tool. Include the object's shadow when needed.
3. Choose **Quick Heal** for small distractions or **AI Remove** for larger objects and scene reconstruction.
4. Apply the removal, review the result, and use **Original** to compare it with the starting image.
5. Save your work using the appropriate option:

| Save option | What it keeps |
| --- | --- |
| **Save Project** | The original and applied layers in an editable `.lremove` file. |
| **Save Unique** | A separate flattened image beside the source file. |
| **Save Overwrite** | A flattened result that replaces the source image. |
| **Export** | A flattened image saved through a save dialog. |

Layers can be hidden, discarded, or merged into a new layer. Save an editable project to continue retouching later. Unapplied brush selections and unfinished pen paths are not included in projects.

Supported formats are **JPEG, PNG, TIFF, and WebP**. For camera RAW files, send a rendered image from your RAW editor.

### Choose a healing method

- **Texture repair** copies nearby texture to repair small objects and blemishes.
- **Dust & scratches** fills narrow defects using nearby colors.
- **AI Remove** uses the selected local AI model. FLUX Klein is the default; Qwen removal is experimental and may be slower or alter fine texture.

### Connect an existing ComfyUI installation

1. Start ComfyUI on this PC.
2. Open **Settings → AI connection** in Local Remove, or **AI connection settings** from its Start menu folder.
3. Enter the port used by ComfyUI. The default is `8188`; use the port shown by your installation.
4. Browse to the model folder that ComfyUI uses, then save.
5. Select an available model in Local Remove's Settings.

The installer does not install ComfyUI or download AI models. Both applications must use the same local model files:

| Choice | Required files beneath the model folder |
| --- | --- |
| **FLUX Klein** | `diffusion_models/flux-2-klein-base-4b.safetensors`<br>`text_encoders/qwen_3_4b.safetensors`<br>`vae/flux2-vae.safetensors`<br>`loras/flux-2-klein-object-remove.safetensors` |
| **Qwen removal** | `diffusion_models/qwen_image_edit_2511_fp8mixed.safetensors`<br>`text_encoders/qwen_2.5_vl_7b_fp8_scaled.safetensors`<br>`vae/qwen_image_vae.safetensors`<br>`loras/Qwen-Image-Edit-2511-Object-Remover-v2-9200.safetensors` |

ComfyUI must support the workflow nodes used by the selected model. The workflows are defined in [local_removal_models.py](backend/local_removal_models.py) and [specialized_removal.py](backend/specialized_removal.py).

### Use with Capture One

Choose **Edit With** in Capture One and select the installed `Local Remove.exe`. Send a rendered TIFF or JPEG. After editing, use **Save Overwrite** to update that rendered file. Also use **Save Project** if you want to retain the editable Local Remove layers.

The default executable location is `%LOCALAPPDATA%\Programs\Local Remove\Local Remove.exe`.

## Where your files are stored

Local Remove follows a per-user Windows installation layout. Updates replace application files without replacing your settings or recoverable edits.

| Files | Default location |
| --- | --- |
| Application and bundled runtime | `%LOCALAPPDATA%\Programs\Local Remove` |
| AI connection settings | `%LOCALAPPDATA%\Local Remove\config.json` |
| Editor settings and recoverable sessions | `%LOCALAPPDATA%\Local Remove\state` |
| Disposable image cache and thumbnails | `%LOCALAPPDATA%\Local Remove\cache` |
| Diagnostic logs | `%LOCALAPPDATA%\Local Remove\logs` |
| Embedded browser profile | `%LOCALAPPDATA%\Local Remove\WebView2` |
| Saved photos and `.lremove` projects | The location you choose in the app. |

`%LOCALAPPDATA%` is your Windows account's local AppData folder. Paste a path from the table into File Explorer's address bar to open it.

Logs rotate automatically. Thumbnail cleanup removes old or excess thumbnails when the app starts. Recoverable edits are kept separately and are never removed by disposable-cache cleanup. The background service shuts down after the last desktop window has been closed and it has been idle for about 75 seconds.

## Update or uninstall

Save your edits, close Local Remove, and run a newer installer to update it. Setup stops the idle background service before replacing program files.

To uninstall, use **Windows Settings → Apps → Installed apps → Local Remove → Uninstall**. Uninstalling removes the app and shortcuts while retaining settings, recoverable sessions, and your saved photos/projects.

If you used the earlier Documents-based version, save its unfinished work as `.lremove` projects and open those projects in the installed version. The installer leaves the earlier installation and its local data untouched.

## Troubleshooting

| Problem | Check |
| --- | --- |
| AI Remove is unavailable | Start ComfyUI, confirm its port in **AI connection**, and check the selected model files. Quick Heal remains available. |
| Texture repair is unavailable | Re-run the installer to restore the bundled helper and dependencies. |
| The editor window cannot load | Re-run setup and confirm Microsoft WebView2 installed successfully. Details are saved in the app's `logs` folder. |
| Save Unique or Save Overwrite is unavailable | Open the image through the desktop app's native file/folder picker or Explorer drag-and-drop. |
| An exported image has no editable layers | Open the `.lremove` project. JPEG, PNG, TIFF, and WebP exports are flattened. |

## Development and backup

- [Build and test instructions](docs/DEVELOPMENT.md)
- [Desktop host details](desktop/NativeHost-README.md)
- [Original backup provenance](docs/BACKUP.md)

`backend/` contains the runnable editor and server, `desktop/` contains the Windows host source, `packaging/` builds the installer, and `tests/` contains the regression tests. Generated files, credentials, caches, and personal photo projects are excluded from Git.

## Credits and licenses

Local Remove uses the RapidRAW AI Connector, Microsoft WebView2, Embark Studios texture-synthesis, OpenCV, and code derived from ComfyUI-DeGrid. Existing component licenses are retained:

- [Backend license](backend/LICENSE)
- [Third-party notices](backend/THIRD_PARTY_NOTICES.md)
- [Microsoft WebView2 license](desktop/licenses/Microsoft-WebView2-LICENSE.txt) and [notices](desktop/licenses/Microsoft-WebView2-NOTICE.txt)

No new project-wide license has been selected.
